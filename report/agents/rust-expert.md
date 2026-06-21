# Rust Expert Analysis: RustCode Idioms, Safety, and Language Features

**Agent 03 — Rust Language Expert**
**Date:** 2026-06-21
**Scope:** `rustcode-core`, `rustcode-lsp`, `rustcode-mcp`, `src/main.rs`

---

## Executive Summary

RustCode is a faithful line-by-line port of the OpenCode TypeScript monorepo (~10K+ TS files). The codebase is in **scaffold phase** with relaxed lints (`#![allow(dead_code, unused_imports, unused_variables)]`). The port is structurally correct but exhibits **substantial opportunities to leverage Rust idioms** that go beyond transliteration. The single highest-impact improvement is replacing the fragmented error enum hierarchy with a unified `snafu`/`thiserror`-based approach and eliminating the `SessionError`/`DatabaseServiceError`/`LspError` islands.

---

## 1. Ownership & Borrowing

### Finding 1.1: Unnecessary Clones in Session Fission
- **Location**: `session.rs:866-891`
- **OpenCode**: TS passes mutable refs; no clone cost.
- **RustCode**: `fork()` iterates messages and clones each `Part` via `p.clone()` then mutates IDs. All 13 Part variants are cloned even though only id/message_id/session_id change.
- **Gap**: Should use `Arc::make_mut` or a bespoke `rebuild_ids()` method that avoids full clone of large fields (e.g., `text`, `output`).
- **Consequence**: Forking a session with large tool outputs duplicates all strings.
- **Recommendation**: Add a `fn map_ids(self, new_msg_id, new_sess_id)` that moves and mutates in-place.
- **Severity**: Medium

### Finding 1.2: `RwLock` on Config's `Info`
- **Location**: `config.rs:47`
- **OpenCode**: Effect `Layer` + `Ref<Info>` — no explicit locking.
- **RustCode**: `Config` uses `RwLock<Info>` and `.clone()` on every read (`config.rs:1166`).
- **Gap**: Config is loaded once at startup, then `get()` clones the entire `Info` struct (hundreds of fields). Heavy but harmless for scaffold.
- **Consequence**: Every tool invocation deep-clones the entire config tree.
- **Recommendation**: Expose atomic reads via `&self` methods that borrow individual fields, or use `arc_swap` for lock-free reads.
- **Severity**: Medium

### Finding 1.3: `Arc`-ing `CancellationToken`
- **Location**: `tool.rs:41`
- **OpenCode**: `AbortSignal` passed by reference.
- **RustCode**: `CancellationToken` in `ToolContext` — correctly `Clone` (it is an `Arc` internally) but also re-wrapped in `ToolContext.clone()`. Fine.
- **Gap**: None — correct pattern.
- **Severity**: Info

### Finding 1.4: Cloning `Vec<ChatMessage>` in `ToolContext`
- **Location**: `tool.rs:47`
- **OpenCode**: Messages come from the session, passed by reference.
- **RustCode**: `ToolContext` stores `Vec<crate::provider::ChatMessage>` — a full clone of the message history into every tool invocation.
- **Gap**: Should store `Arc<Vec<ChatMessage>>` or `&'a [ChatMessage]` with a lifetime.
- **Consequence**: Each bash/read/write tool call clones the entire accumulated message history.
- **Recommendation**: Use `Arc<[ChatMessage]>` or pass messages as a `&[ChatMessage]` to `execute_with_pipeline`.
- **Severity**: High

### Finding 1.5: `ToolContext.ask_fn` Boxing
- **Location**: `tool.rs:52-61`
- **RustCode**: `ask_fn: Option<Arc<dyn Fn(...) -> Pin<Box<dyn Future>>>>`
- **Gap**: This is a double-indirection — `Arc<dyn Fn>` wrapping a `Pin<Box<dyn Future>>`. Equivalent TS is a simple `async (perm, res) => bool`.
- **Recommendation**: Use a trait object `Arc<dyn AskPermission>` with an async method directly, removing one layer of indirection.
- **Severity**: Low

---

## 2. Lifetimes

### Finding 2.1: Missing Lifetime on `CatalogTransformContext`
- **Location**: `plugin.rs:28-37`
- **OpenCode**: Mutated provider data in-place during hook dispatch.
- **RustCode**: `CatalogTransformContext<'a>` correctly uses `&'a mut HashMap` etc. ✅
- **Gap**: None — lifetimes are correct here.
- **Severity**: Info

### Finding 2.2: `Provider` Trait Has No Lifetime Parameters
- **Location**: `provider.rs:906-940`
- **OpenCode**: Provider methods accept `model: Model` and `messages: ChatMessage[]` by value (TS is pass-by-reference).
- **RustCode**: `stream(&self, model: &Model, messages: &[ChatMessage], ...)` — correct use of references. ✅
- **Gap**: None.
- **Severity**: Info

### Finding 2.3: Missing `'static` Bound on Provider Stream Return
- **Location**: `provider.rs:929-931`
- **RustCode**: Returns `Box<dyn Stream<Item = Result<LlmEvent>> + Send + Unpin>`
- **Gap**: No `'static` bound — but `Send + Unpin` implies `'static` for the concrete type in practice. Should be explicit.
- **Consequence**: If the stream borrows from `self`, the borrow checker may catch lifetime issues. This is actually safer as-is because the erased lifetime forces ownership.
- **Recommendation**: Keep as-is — the lack of `'static` is intentional for borrowing from `self`.
- **Severity**: Low

---

## 3. Trait Design

### Finding 3.1: `Provider` Trait — Missing GAT for Stream Type
- **Location**: `provider.rs:906-940`
- **OpenCode**: `stream()` returns a concrete `Stream` type per provider.
- **RustCode**: `stream()` returns `Box<dyn Stream<...>>` — heap allocation per call.
- **Gap**: A GAT like `type Stream<'a>: Stream<Item = ...> + 'a` would allow zero-cost static dispatch. Current design boxes every stream.
- **Consequence**: Each `provider.stream()` call allocates on the heap.
- **Recommendation**: Add a GAT `type StreamingOutput<'a>: futures::Stream<Item = Result<LlmEvent>> + Send + Unpin + 'a;` to the `Provider` trait.
- **Severity**: Medium

### Finding 3.2: `Tool` Trait — `json_schema()` and `parameters_schema()` Confusion
- **Location**: `tool.rs:163-201`
- **OpenCode**: Single `inputSchema` property.
- **RustCode**: Two methods: `json_schema()` (returns `Option`) and `parameters_schema()` (always returns `Value`). The distinction is unclear.
- **Gap**: `json_schema()` defaults to `None`; `parameters_schema()` is required. Tools implement both but often return the same value (see `PluginToolAdapter:563-568`).
- **Recommendation**: Merge into one method `fn input_schema(&self) -> serde_json::Value;` with a sentinel for "no schema."
- **Severity**: Low

### Finding 3.3: `ProviderCatalog` Trait — Too Wide
- **Location**: `provider.rs:949-981`
- **OpenCode**: Provider catalog is a service with 6 methods.
- **RustCode**: 7 methods — `list`, `get_provider`, `get_model`, `closest`, `get_small_model`, `default_model`.
- **Gap**: `closest()` and `get_small_model()` are algorithmic — should be default methods implemented on top of `list()`/`get_provider()`, not required trait methods.
- **Recommendation**: Provide default implementations for `closest()`, `get_small_model()`, `default_model()`.
- **Severity**: Low

### Finding 3.4: `EventV2Interface` Trait — Large But Correct
- **Location**: `event.rs:707-764`
- **OpenCode**: `EventV2` interface in Effect with 12 methods.
- **RustCode**: 12 async trait methods with correct `Send + Sync` bounds.
- **Gap**: None — well-structured. `async_trait` is appropriate here because implementations require DB access.
- **Severity**: Info

---

## 4. Generic Design

### Finding 4.1: `PluginToolDef::new()` — Where Clause Could Be Simplified
- **Location**: `tool.rs:311-328`
- **RustCode**: `where F: Fn(...) -> Fut + Send + Sync + 'static, Fut: Future<Output = ...> + Send + 'static`
- **Gap**: The `'static` on both `F` and `Fut` is redundant — `F: 'static` implies `Fut: 'static`.
- **Consequence**: None, but slightly more restrictive than needed.
- **Recommendation**: Remove `Fut: 'static` — `F: 'static` implies it.
- **Severity**: Info

### Finding 4.2: `ClosureProviderPlugin` — Type Complexity
- **Location**: `plugin.rs:210-229`
- **RustCode**: 4 function pointer fields with complex types, suppressed by `#[allow(clippy::type_complexity)]`.
- **Gap**: The type complexity is a smell. Each closure is `Box<dyn Fn(&...) -> BoxFuture<...> + Send + Sync>`.
- **Recommendation**: Define a `trait HookHandler<Ctx>` with an async method, reducing the type soup.
- **Severity**: Medium

---

## 5. Async Correctness

### Finding 5.1: `Config::load_global()` is Synchronous
- **Location**: `config.rs:1177-1218`
- **OpenCode**: Config loading is `Effect` (async).
- **RustCode**: Filesystem reads done synchronously with `std::fs::read_to_string`.
- **Gap**: In a tokio context, blocking I/O in sync functions starves the async runtime if called from an async context.
- **Consequence**: If `Config::load()` is called inside `block_on`, it's fine. But if called from any spawned task, it blocks the reactor.
- **Recommendation**: Make config loading async with `tokio::fs` or document that it must be called during initialization.
- **Severity**: Medium

### Finding 5.2: `tokio::select!` Biased Usage in BashTool
- **Location**: `tool_impls.rs:757-885`
- **RustCode**: Uses `tokio::select! { biased; ... }`
- **Gap**: `biased` is correct here (abort has priority over timeout, timeout has priority over completion). ✅
- **Severity**: Info

### Finding 5.3: Missing `Send` Bounds on Some Arc<dyn Fn> Types
- **Location**: `tool.rs:52-61, tool.rs:152-156`
- **RustCode**: `ask_fn` correctly has `+ Send + Sync`. ✅
- **Gap**: None — all async callback types are correctly bounded.
- **Severity**: Info

### Finding 5.4: `Mutex` in `SessionManager` — Deadlock Potential
- **Location**: `session.rs:723-726` (BashTool uses `Mutex<String>` for stdout/stderr buffers)
- **RustCode**: `output_buf: Arc<tokio::sync::Mutex<String>>` — held across `.await` points.
- **Gap**: The `Mutex` is held briefly only during `guard.push_str(...)`. No `.await` while holding. ✅
- **Recommendation**: Consider `tokio::sync::watch` for streaming output to avoid locking entirely.
- **Severity**: Low

---

## 6. Unsafe Usage

### Finding 6.1: `#![forbid(unsafe_code)]` Enforcement
- **Location**: `lib.rs:1`, `rustcode-lsp/src/lib.rs:1`, `rustcode-mcp/src/lib.rs:1`, `src/main.rs:1`
- **RustCode**: All crates forbid unsafe code at the crate level.
- **Gap**: None — comprehensive enforcement.
- **Severity**: ✅ Info (positive finding)

### Finding 6.2: `From<io::Error>` but No `unsafe` I/O
- All I/O uses `std::fs` and `tokio::fs` — safe.
- **Severity**: ✅ Info (positive finding)

---

## 7. Error Handling

### Finding 7.1: Fragmented Error Hierarchy
- **Location**: `error.rs:23-352`, `session.rs:37-77`, `database.rs:1146-1158`, `rustcode-lsp/src/lib.rs:50-110`
- **OpenCode**: All errors are `Schema.TaggedErrorClass` — single unified type with a `_tag` discriminant.
- **RustCode**: Five separate error types:
  - `crate::error::Error` (34 variants)
  - `SessionError` (12 variants)
  - `DatabaseServiceError` (3 variants)
  - `LspError` (10 variants)
  - `McpOAuthError`, `McpNotFoundError`, `McpFailedError`
- **Gap**: Downstream code must match on 5+ enums. Any function that touches both session and database must convert between error types. The `SessionError` has `#[from] sqlx::Error` and `#[from] DatabaseServiceError`, but `crate::error::Error` does NOT have `#[from] SessionError` — callers must `map_err`.
- **Consequence**: Error conversion boilerplate throughout. Missed errors bubble as `SessionError::Other()` or `crate::error::Error::Internal()` — losing type information.
- **Recommendation**: Either:
  a) Merge all into `crate::error::Error` with variant nesting (e.g., `Error::Lsp(LspError)`), **OR**
  b) Keep separate enums but implement `From<SessionError>` for `crate::error::Error` and make `SessionError` a `#[non_exhaustive]` enum in the public API.
- **Severity**: Critical

### Finding 7.2: `Result<T>` Type Shadowing
- **Location**: `error.rs:688`, `rustcode-lsp/src/lib.rs:113`
- **RustCode**: Two `Result` type aliases: `error::Result<T>` and `LspError::Result<T>`.
- **Gap**: LSP crate uses its own `Result<T>` which is incompatible with `crate::error::Result<T>`. LSP functions must be called outside the LSP crate by converting errors.
- **Consequence**: LSP users outside the crate cannot use `?` with `crate::error::Error` — they must `.map_err(|e| Error::LspInit(e.to_string()))`.
- **Recommendation**: Make `rustcode_lsp` use `crate::error::Result` (or a unified error type).
- **Severity**: High

### Finding 7.3: `thiserror` Usage is Correct
- **Location**: All error enums
- **RustCode**: Consistently uses `#[derive(Debug, Error)]` and `#[error("...")]` with field interpolation. ✅
- **Gap**: None — `thiserror` v2 is used correctly.
- **Severity**: Info

### Finding 7.4: `Error::Llm` Uses `Box<LlmErrorReason>`
- **Location**: `error.rs:90`
- **RustCode**: `reason: Box<LlmErrorReason>` — correctly avoids large enum size.
- **Gap**: None — `LlmErrorReason` is 308 bytes (10 variants), boxing keeps `Error` at reasonable size.
- **Severity**: ✅ Info (positive finding)

### Finding 7.5: `SessionError::Other(String)` — Error Information Loss
- **Location**: `session.rs:76`
- **RustCode**: `Other(String)` as catch-all variant.
- **Gap**: Any unrecognized error is stringified, losing structured error data.
- **Consequence**: Downstream consumers cannot match on specific errors.
- **Recommendation**: Add `#[from]` for `crate::error::Error` or use `Box<dyn std::error::Error + Send>`.
- **Severity**: Medium

---

## 8. Macro Usage

### Finding 8.1: Heavy `#[serde]` Attribute Usage
- **Location**: All types across `provider.rs`, `config.rs`, `session.rs`, `mcp.rs`
- **RustCode**: Extensive use of `#[serde(rename_all = "...")]`, `#[serde(rename = "...")]`, `#[serde(default)]`, `#[serde(skip_serializing_if = "...")]`, `#[serde(untagged)]`, `#[serde(tag = "...")]`.
- **Gap**: The serde annotation density is a direct transliteration of the TS JSON schemas. It is correct but verbose. Some patterns are duplicated (e.g., `#[serde(default, skip_serializing_if = "HashMap::is_empty")]` appears 20+ times).
- **Recommendation**: Define a helper module with constants for common serde patterns, or use a `serde_with` macro.
- **Severity**: Low

### Finding 8.2: `#[async_trait]` Used Throughout
- **Location**: `provider.rs:906`, `tool.rs:163`, `plugin.rs:80`, `plugin.rs:784`, `event.rs:708`, `rustcode-mcp/src/lib.rs:63`
- **RustCode**: 6+ traits use `#[async_trait]`.
- **Gap**: This is appropriate for a scaffold phase. Post-Rust 1.75, native `async fn` in traits is stable but requires `use ... as ...` for methods. `async_trait` is the pragmatic choice.
- **Severity**: Info

### Finding 8.3: `#[allow(...)]` Suppressions
- **Location**: `lib.rs:2`, `plugin.rs:209`
- **RustCode**: `#![allow(dead_code, unused_imports, unused_variables)]` in `lib.rs` and `main.rs`; `#[allow(clippy::type_complexity)]` in `plugin.rs`.
- **Gap**: Dead code allowance should be scoped to specific items, not crate-wide.
- **Consequence**: Real dead code goes undetected.
- **Recommendation**: Scope allowances to individual items or functions, not the entire crate.
- **Severity**: Medium

---

## 9. Enum Design

### Finding 9.1: `Part` Enum — 13 Variants, Large
- **Location**: `session.rs:306-335`
- **OpenCode**: Union of 13 types in TS.
- **RustCode**: Enum with 13 variants, each wrapping a struct. `size_of::<Part>()` is [size of largest variant + discriminant].
- **Gap**: The largest variant (`ToolPart` with `serde_json::Value` and `String`) determines the enum size. Boxing the largest few variants (`ToolPart`, `TextPart`) could reduce size by ~50%.
- **Consequence**: `Vec<Part>` is memory-heavy for session message storage.
- **Recommendation**: Box the largest 2-3 variants: `Tool(Box<ToolPart>)`, `Text(Box<TextPart>)`, `Reasoning(Box<ReasoningPart>)`.
- **Severity**: Medium

### Finding 9.2: `SessionStatus` — `Retry` Variant Carries Action
- **Location**: `session.rs:2733-2745`
- **RustCode**: `Retry { attempt, message, action, next }` — well-structured tagged union.
- **Gap**: None — serde `tag = "type"` matches TS pattern. ✅
- **Severity**: Info

### Finding 9.3: `LlmEvent` — Exhaustive Pattern Matching
- **Location**: `provider.rs:478-669`
- **RustCode**: 16 variants, each with serde `tag = "type"`. Helper methods `type_tag()`, `is_text_delta()`, `usage()` use exhaustive matches. ✅
- **Gap**: `usage()` returns `Option<&Usage>` — correctly matches `StepFinish` and `Finish` only. ✅
- **Severity**: Info

---

## 10. Newtype Pattern

### Finding 10.1: `EventId` — Branded String
- **Location**: `event.rs:42-79`
- **RustCode**: `EventId(String)` with `#[serde(transparent)]`, factory methods `create()`, `from_external()`, `Display`, `From<String>`.
- **Gap**: No `FromStr` implementation (for deserialization from user input).
- **Consequence**: Cannot parse `"evt_xxx"` from CLI args or API params.
- **Recommendation**: Add `FromStr` impl that validates the `evt_` prefix.
- **Severity**: Low

### Finding 10.2: `EventCursor` — Branded u64
- **Location**: `event.rs:97-132`
- **RustCode**: `EventCursor(u64)` with `new()`, `value()`, `ZERO`, `From<u64>`, `Into<u64>`.
- **Gap**: Correctly prevents mixing raw integers with cursors. ✅
- **Severity**: ✅ Info (positive finding)

### Finding 10.3: Type Aliases Instead of Newtypes
- **Location**: `session.rs:84-90`, `provider.rs:24-48`
- **RustCode**: `pub type SessionId = String;`, `pub type ModelId = String;`, etc.
- **Gap**: These are type aliases, not newtypes. `SessionId` and `ModelId` are interchangeable with `String`.
- **Consequence**: `fn foo(id: SessionId)` accepts any `String`. No compile-time safety.
- **Recommendation**: Use proper newtypes: `struct SessionId(String);` with a `new` + `as_str`.
- **Severity**: Medium

---

## 11. Builder Pattern

### Finding 11.1: `McpServerConfig` Builder Methods
- **Location**: `mcp.rs:263-371`
- **RustCode**: `local()`, `remote()`, `with_env()`, `with_timeout()`, `disabled()`, `with_headers()`, `with_oauth()`, `without_oauth()` — full builder pattern.
- **Gap**: None — idiomatic Rust builder. ✅
- **Severity**: ✅ Info (positive finding)

### Finding 11.2: `ClosureProviderPlugin` Builder
- **Location**: `plugin.rs:231-275`
- **RustCode**: Builder pattern: `new()`, `with_transform()`, `with_discover()`, `with_auth()`.
- **Gap**: Correct pattern. ✅
- **Severity**: Info

---

## 12. Pattern Matching

### Finding 12.1: Exhaustive on `Part::set_id()`, `set_message_id()`, `set_session_id()`
- **Location**: `session.rs:1605-1663`
- **RustCode**: Three functions each with 13-arm matches — all exhaustive.
- **Gap**: Each function repeats the same match pattern. On adding a new variant, all three functions must be updated.
- **Recommendation**: Use a helper macro or a method on `Part` that returns `&mut CommonPartFields` (a struct of `{id, message_id, session_id}`) to deduplicate.
- **Severity**: Medium

### Finding 12.2: `dispatch_inner` — Single Exhaustive Match on 23 Commands
- **Location**: `main.rs:1342-1372`
- **RustCode**: Single `match` on 23 command variants, each dispatching to `cmd_*` function.
- **Gap**: Well-structured. ✅
- **Severity**: Info

### Finding 12.3: `matches!` Used Where `if let` Would Suffice
- **Location**: `provider.rs:1248-1249`, `session.rs:457`
- **Gap**: Minor stylistic preference. Both are correct.
- **Severity**: Info

---

## 13. FFI Safety

### Finding 13.1: No FFI Declarations
- **Location**: All files
- **RustCode**: No `extern "C"`, no `#[no_mangle]`, no `#[repr(C)]`.
- **Gap**: Not applicable — this is a pure Rust CLI tool.
- **Severity**: N/A

### Finding 13.2: `Option_env!` for Package Version
- **Location**: `rustcode-mcp/src/lib.rs:189`
- **RustCode**: `option_env!("CARGO_PKG_VERSION").unwrap_or("0.1.0")` — graceful fallback if env var is missing (e.g., in tests).
- **Gap**: The only FFI-adjacent concern is `env!()` which panics at compile time. `option_env!()` is defensive. ✅
- **Severity**: Info

---

## 14. Pin Safety

### Finding 14.1: `Pin<Box<dyn Future>>` for Callbacks
- **Location**: `tool.rs:55`, `tool.rs:154-156`, `event.rs:410`, `event.rs:435`
- **RustCode**: All async callbacks use `Pin<Box<dyn Future + Send>>`.
- **Gap**: Correct — `Pin` ensures the future is not moved after being polled. `Box` ensures it's heap-allocated. `Send` ensures it can cross await points.
- **Severity**: ✅ Info (positive finding)

### Finding 14.2: Manual `Pin` Implementation for `StdioTransport`
- None — transport uses `tokio::io::AsyncWriteExt` which is safe.
- **Severity**: Info

---

## 15. Type State Pattern

### Finding 15.1: No Type State for Session Lifecycle
- **Location**: `session.rs:605-608`
- **OpenCode**: Session state managed via Effect's `Ref<RunState>` with typed states (Idle, Busy, Retry).
- **RustCode**: `SessionManager` holds `Arc<DatabaseService>` + `SharedBus`. No compile-time state enforcement. Session state is tracked via `SessionStatus` at runtime.
- **Gap**: Type state (e.g., `Session<Idle>`, `Session<Busy>`) would prevent calling `process()` on a busy session at compile time.
- **Consequence**: Runtime errors (`SessionBusy`) instead of compile-time guarantees.
- **Recommendation**: Introduce a type-state parameter on `SessionManager` methods: `fn process(&self, session: &Session<Idle>) -> Result<Session<Busy>>` — though this is heavy for the current scaffold phase.
- **Severity**: Low

### Finding 15.2: `McpClientState` — Internal Enum Per Connection
- **Location**: `mcp.rs:938-972`
- **RustCode**: Internal enum `McpClientState { Local { child }, Remote { ... }, RemoteSse { ... }, Disconnected }`.
- **Gap**: Not a type-safe state machine (the client holds a `Mutex<McpClientState>`). But for an internal impl detail this is acceptable.
- **Severity**: Low

---

## Cross-Cutting Concerns

### Finding C.1: Dependency Injection vs. Struct-of-Services

**OpenCode**: Effect's `Context.Service` + `Layer` provides compile-time dependency resolution with testability.

**RustCode**: Manual injection via `Arc<DatabaseService>`, `SharedBus`, `Arc<ToolRegistry>`, `Arc<PermissionService>`:

```rust
// session.rs:1768-1780
pub fn new(
    manager: Arc<SessionManager>,
    tool_registry: Arc<ToolRegistry>,
    permission: Arc<PermissionService>,
    bus: SharedBus,
) -> Self { ... }
```

**Gap**: Every new service dependency requires changing constructors and all call sites. No scoping (e.g., request-scoped vs. singleton).

**Recommendation**: Introduce a `ServiceRegistry` or `AppContext` struct that holds all services as `Arc<dyn ...>` and is passed as a single parameter. Alternatively, use a crate like `shaku` or `ramhorns` for DI.

**Severity**: Medium

### Finding C.2: Serialization Strategy

**OpenCode**: Drizzle ORM with typed schemas; JSON stored in TEXT columns.

**RustCode**: `serde_json::Value` used extensively as an escape hatch:
- `MessageInfo` stored as JSON string in `message.data` column
- `Part` stored as JSON string in `part.data` column
- Config loaded as `serde_json::Value` then deserialized

**Gap**: The JSON-in-TEXT approach (used in `message`, `part` tables) loses type safety. A migration to proper columns would require SQLite schema changes.

**Consequence**: Cannot query by message content in SQL.

**Recommendation**: For new tables, use typed columns. The legacy approach is acceptable for the `message` and `part` tables to match TS compatibility.

**Severity**: Medium

### Finding C.3: Thread Safety of `ObservabilityService`

- **Location**: `main.rs:1250-1267`
- **RustCode**: `ObservabilityService::new()` initialized once in `main()`.
- **Gap**: If `init()` is called multiple times, the second call returns `Ok(false)` silently. No `OnceCell` or `OnceLock` protection.
- **Recommendation**: Use `OnceLock<ObservabilityService>` or check `tracing::subscriber` is set.
- **Severity**: Low

---

## Summary of Severity Distribution

| Severity | Count | Key Examples |
|----------|-------|-------------|
| Critical | 1 | Fragmented error hierarchy (Finding 7.1) |
| High | 2 | Message history clone (1.4), LSP error isolation (7.2) |
| Medium | 10 | Unnecessary Part clones (1.1), Config clones (1.2), Missing `Send` (5.2), Sync I/O (5.1), `SessionError::Other` (7.5), Dead code allowance (8.3), Part enum size (9.1), Type aliases (10.3), Duplicate patterns (12.1), JSON-in-TEXT (C.2) |
| Low | 6 | Double-indirection ask_fn (1.5), Missing GAT (3.1), Two schema methods (3.2), Closure type complexity (4.2), `EventId` FromStr (10.1), Type state (15.1) |
| Info | 12 | CancellationToken (1.3), async_trait usage (8.2), Builder patterns (11.1, 11.2), serde density (8.1), etc. |

**Top 3 Recommendations:**

1. **Unify the error hierarchy** — merge `SessionError`, `DatabaseServiceError`, `LspError`, `McpError` into `crate::error::Error` with `#[from]` derives (Critical)
2. **Eliminate message history clones** — store `Arc<[ChatMessage]>` in `ToolContext` instead of `Vec<ChatMessage>` (High)
3. **Use proper newtypes** — replace `type SessionId = String` with `struct SessionId(String)` for type safety (Medium)
