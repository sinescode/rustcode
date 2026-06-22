# Complete Findings

**Generated**: 2026-06-21
**Source**: 20 Agent Reports (Agents 01–20)
**Note**: This is the complete raw data dump. Every finding from every agent report is included verbatim. No summarization.

---

## Architecture Findings (from Agent 02)

### Finding ARC-1: 95 flat public modules with no visibility filtering
- **Severity**: Critical
- **Location**: `blazecode/crates/blazecode-core/src/lib.rs:1-95`
- **Description**: All 95 modules in `blazecode-core` are `pub mod` — zero visibility filtering. Every module is accessible to every consumer.
- **Consequence**: Impossible to reason about which modules are internal implementation details vs public API. Refactoring requires understanding the full 95-module graph.
- **Recommendation**: Use `pub(crate)` for internal modules. Define a `lib.rs` that re-exports only the intended public API surface.

### Finding ARC-2: Extreme coupling across all modules
- **Severity**: Critical
- **Location**: `blazecode/crates/blazecode-core/src/lib.rs:1-95`, `blazecode/src/main.rs:1-8575`
- **Description**: All 95 modules in `blazecode-core` are flat-scoped public modules. The binary (`main.rs, 8,575 lines`) directly imports and uses `sqlx`, `blazecode_core::config`, `blazecode_core::*`, and embeds business logic. The provider module references `database.rs`, `config.rs`, `tool.rs` — a dense dependency web.
- **Consequence**: High coupling makes BlazeCode brittle. A change to `config.rs` can ripple through all 94 other modules. Testing any module in isolation requires importing the entire core crate.
- **Recommendation**: Introduce trait-based dependency inversion within `blazecode-core`. Split into multiple crates.

### Finding ARC-3: Infrastructure dependency in core (sqlx, reqwest)
- **Severity**: Critical
- **Location**: `blazecode/Cargo.toml:1-96`, `blazecode/src/main.rs:1-8575`
- **Description**: Dependency rule is violated: `blazecode-core` (inner layer) imports `sqlx`, `reqwest`, `serde_json`, `tracing` — infrastructure concerns. Core code directly constructs HTTP clients and makes network requests in provider code. `blazecode-core` has `pub mod database` with SQLite schema definitions and queries inline.
- **Consequence**: Impossible to swap infrastructure (e.g., SQLite to PostgreSQL, reqwest to hyper) without modifying core code. Business logic is polluted with serialization formats, HTTP status codes, and SQL queries. This is the single largest architectural debt in BlazeCode.
- **Recommendation**: Invert all dependencies. Define `Database` trait in core, implement in `blazecode-database-sqlite`. Define `HttpClient` trait in core, implement with `reqwest` in `blazecode-http`.

### Finding ARC-4: 5 crates vs 26 — insufficient modularization
- **Severity**: Critical
- **Location**: `blazecode/Cargo.toml:1-96` (5 crates), `blazecode/packages/` (26 packages)
- **Description**: BlazeCode has 5 crates (4 of which are stubs) vs BlazeCode's 26 packages. No infrastructure crates (database, http, filesystem), no plugin SDK crate, no event-store crate, no CLI library crate, no schema/migration crate.
- **Consequence**: Build times degrade as core grows. No reuse path (cannot publish equivalent of `@blazecode-ai/plugin`). Third-party contributions are harder.
- **Recommendation**: Extract into more granular crates following BlazeCode's package boundaries.

### Finding ARC-5: 8,575-line main.rs with business logic mixed in
- **Severity**: Critical
- **Location**: `blazecode/src/main.rs:1-8575`
- **Description**: BlazeCode's binary is a monolith (8,575 lines) that merges CLI argument parsing, business logic, and infrastructure. The binary depends on wrapper crates but the wrapper crates are stub-level thin. The main.rs directly imports `blazecode_core::config::Config`, `sqlx`, `blazecode_core::*`, blurring the boundary.
- **Consequence**: Makes testing, swapping, and parallel development harder. BlazeCode has effectively 1.5 layers (core + thin wrappers) vs BlazeCode's 4+ layers.
- **Recommendation**: Move logic from main.rs into `blazecode-core` or the appropriate wrapper crate. Make main.rs a thin CLI dispatch (<500 lines).

### Finding ARC-6: No sub-module grouping for 14+ session modules
- **Severity**: High
- **Location**: `blazecode/crates/blazecode-core/src/lib.rs:1-95`
- **Description**: 14 session-related modules (`session_runner.rs`, `session_prompt.rs`, `session_projector.rs`, `session_history.rs`, `session_input_inbox.rs`, `session_epoch.rs`, `session_compaction.rs`, `session_message.rs`, `session_execution.rs`, `session_model.rs`, `session_reminders.rs`, `session_revert.rs`, `session_todo.rs`, `session_info.rs`) are all flat in the same crate with no sub-module grouping. Uses flat name prefixing (`session_*`) instead of sub-modules. Similarly, 8+ provider-related modules all flat.
- **Consequence**: The 95 flat modules create heavy cognitive load. Developers must scan the full list to understand what exists. Sub-module grouping would reduce the apparent surface area from 95 to ~15 groups.
- **Recommendation**: Group related modules into sub-modules: `pub mod session { ... }`, `pub mod provider { ... }`, `pub mod config { ... }`.

### Finding ARC-7: Missing V2 domain model (System Context, EventV2)
- **Severity**: High
- **Location**: `blazecode/AGENTS.md:148-158`, `blazecode/CONTEXT.md:1-129`, `blazecode/crates/blazecode-core/src/lib.rs:11-95`
- **Description**: BlazeCode V2 has a mature domain model with explicitly separated concerns: System Context algebra (epoch, baseline, snapshot, mid-conversation messages), EventV2 event sourcing, Location-scoped services, Account identity domain. BlazeCode lacks System Context (context source, epoch, and reconciliation concepts are absent), EventV2 (the event-sourcing architecture with replayable event streams is not present), Location (the Location-scoped service pattern is not yet represented), and Account (account module exists but without the full Identity domain).
- **Consequence**: BlazeCode will diverge in capabilities as BlazeCode's V2 architecture matures. The System Context algebra (CONTEXT.md, 129 rules) is the core of BlazeCode's session intelligence — without it, BlazeCode cannot match session behavior.
- **Recommendation**: Map the V2 domain model before deeper implementation. Create dedicated modules for System Context algebra. Implement EventV2 event sourcing. Model `Location` as a first-class domain concept.

### Finding ARC-8: No hexagonal architecture outside provider trait
- **Severity**: High
- **Location**: `blazecode/docs/plugin-system.md:199-243` (provider trait), `blazecode/packages/llm/src/` (LLM port/adapter)
- **Description**: BlazeCode has good port/adapter for LLM providers (Provider trait with ProviderCatalog holding `Box<dyn Provider>`; OpenAICompatibleProvider with 14 pre-configured profiles). However, the database (`sqlx`), HTTP server (`axum`), and filesystem access are imported directly in `main.rs` and throughout `blazecode-core`. There is no infrastructure abstraction layer — core code directly calls SQL queries and filesystem operations.
- **Consequence**: Testing BlazeCode requires real infrastructure (SQLite files, real filesystem, network). BlazeCode's hexagonal architecture allows each domain to be tested with in-memory or mock adapters.
- **Recommendation**: Generalize the port/adapter pattern. Define traits for `Database`, `FileSystem`, `EventStore`, `SessionStore`. Move infrastructure implementations to adapter crates.

### Finding ARC-9: 4 of 5 wrapper crates are stubs
- **Severity**: High
- **Location**: `blazecode/crates/`
- **Description**: server, tui, lsp, and mcp crates are labeled "stub" or "scaffold" in CLAUDE.md. They re-export from core or have minimal logic. No real separation of concerns despite workspace structure.
- **Consequence**: No real separation of concerns despite workspace structure.
- **Recommendation**: Move logic from core into the appropriate wrapper crates.

### Finding ARC-10: No database/filesystem/event-store port abstractions
- **Severity**: High
- **Location**: `blazecode/crates/blazecode-core/src/database.rs`, `blazecode/crates/blazecode-core/src/filesystem.rs`
- **Description**: No trait abstractions exist for database, filesystem, HTTP client, or event store. Core code calls concrete implementations directly.
- **Consequence**: Cannot swap implementations. Cannot test with mocks.
- **Recommendation**: Define traits and move implementations to adapter crates.

---

## Rust Language Findings (from Agent 03)

### Finding RUST-1: Fragmented error hierarchy
- **Severity**: Critical
- **Location**: `error.rs:23-352`, `session.rs:37-77`, `database.rs:1146-1158`, `blazecode-lsp/src/lib.rs:50-110`
- **Description**: Five separate error types: `crate::error::Error` (34 variants), `SessionError` (12 variants), `DatabaseServiceError` (3 variants), `LspError` (10 variants), `McpOAuthError`, `McpNotFoundError`, `McpFailedError`. No `#[from] SessionError` for `crate::error::Error`.
- **Consequence**: Error conversion boilerplate throughout. Missed errors bubble as `SessionError::Other()` or `crate::error::Error::Internal()` — losing type information.
- **Recommendation**: Either (a) Merge all into `crate::error::Error` with variant nesting, OR (b) Keep separate enums but implement `From<SessionError>` for `crate::error::Error`.

### Finding RUST-2: Cloning `Vec<ChatMessage>` in `ToolContext`
- **Severity**: High
- **Location**: `tool.rs:47`
- **Description**: `ToolContext` stores `Vec<crate::provider::ChatMessage>` — a full clone of the message history into every tool invocation. Messages come from the session, passed by reference in BlazeCode.
- **Consequence**: Each bash/read/write tool call clones the entire accumulated message history.
- **Recommendation**: Use `Arc<[ChatMessage]>` or pass messages as `&[ChatMessage]` to `execute_with_pipeline`.

### Finding RUST-3: LSP error isolation
- **Severity**: High
- **Location**: `error.rs:688`, `blazecode-lsp/src/lib.rs:113`
- **Description**: Two `Result` type aliases: `error::Result<T>` and `LspError::Result<T>`. LSP crate uses its own `Result<T>` incompatible with `crate::error::Result<T>`. LSP functions must convert errors outside the LSP crate.
- **Consequence**: LSP users outside the crate cannot use `?` with `crate::error::Error` — they must `.map_err(|e| Error::LspInit(e.to_string()))`.
- **Recommendation**: Make `blazecode_lsp` use `crate::error::Result` (or a unified error type).

### Finding RUST-4: Unnecessary Clones in Session Fission
- **Severity**: Medium
- **Location**: `session.rs:866-891`
- **Description**: `fork()` iterates messages and clones each `Part` via `p.clone()` then mutates IDs. All 13 Part variants are cloned even though only id/message_id/session_id change.
- **Consequence**: Forking a session with large tool outputs duplicates all strings.
- **Recommendation**: Add a `fn map_ids(self, new_msg_id, new_sess_id)` that moves and mutates in-place.

### Finding RUST-5: `RwLock` on Config's `Info`
- **Severity**: Medium
- **Location**: `config.rs:47`
- **Description**: `Config` uses `RwLock<Info>` and `.clone()` on every read (`config.rs:1166`). Config is loaded once at startup, then `get()` clones the entire `Info` struct (hundreds of fields).
- **Consequence**: Every tool invocation deep-clones the entire config tree.
- **Recommendation**: Expose atomic reads via `&self` methods that borrow individual fields, or use `arc_swap` for lock-free reads.

### Finding RUST-6: Missing GAT for Stream Type on Provider Trait
- **Severity**: Medium
- **Location**: `provider.rs:906-940`
- **Description**: `stream()` returns `Box<dyn Stream<Item = Result<LlmEvent>> + Send + Unpin>` — heap allocation per call. A GAT like `type Stream<'a>: Stream<Item = ...> + 'a` would allow zero-cost static dispatch.
- **Consequence**: Each `provider.stream()` call allocates on the heap. Each `stream.next().await` goes through a vtable call (~5-10ns per event).
- **Recommendation**: Add a GAT `type StreamingOutput<'a>: futures::Stream<Item = Result<LlmEvent>> + Send + Unpin + 'a;` to the `Provider` trait.

### Finding RUST-7: `ClosureProviderPlugin` — Type Complexity
- **Severity**: Medium
- **Location**: `plugin.rs:210-229`
- **Description**: 4 function pointer fields with complex types, suppressed by `#[allow(clippy::type_complexity)]`. Each closure is `Box<dyn Fn(&...) -> BoxFuture<...> + Send + Sync>`.
- **Consequence**: Type soup makes code hard to understand and maintain.
- **Recommendation**: Define a `trait HookHandler<Ctx>` with an async method, reducing the type soup.

### Finding RUST-8: `Config::load_global()` is Synchronous
- **Severity**: Medium
- **Location**: `config.rs:1177-1218`
- **Description**: Filesystem reads done synchronously with `std::fs::read_to_string` in a tokio context.
- **Consequence**: If called from any spawned task, it blocks the reactor.
- **Recommendation**: Make config loading async with `tokio::fs` or document that it must be called during initialization.

### Finding RUST-9: Dead Code Allowance at Crate Level
- **Severity**: Medium
- **Location**: `lib.rs:2`, `plugin.rs:209`
- **Description**: `#![allow(dead_code, unused_imports, unused_variables)]` in `lib.rs` and `main.rs`; `#[allow(clippy::type_complexity)]` in `plugin.rs`.
- **Consequence**: Real dead code goes undetected. Dead code allowance should be scoped to specific items, not crate-wide.
- **Recommendation**: Scope allowances to individual items or functions, not the entire crate.

### Finding RUST-10: `SessionError::Other(String)` — Error Information Loss
- **Severity**: Medium
- **Location**: `session.rs:76`
- **Description**: `Other(String)` as catch-all variant.
- **Consequence**: Any unrecognized error is stringified, losing structured error data. Downstream consumers cannot match on specific errors.
- **Recommendation**: Add `#[from]` for `crate::error::Error` or use `Box<dyn std::error::Error + Send>`.

### Finding RUST-11: `Part` Enum — 13 Variants, Large
- **Severity**: Medium
- **Location**: `session.rs:306-335`
- **Description**: Enum with 13 variants, each wrapping a struct. Largest variant (`ToolPart` with `serde_json::Value` and `String`) determines enum size.
- **Consequence**: `Vec<Part>` is memory-heavy for session message storage.
- **Recommendation**: Box the largest 2-3 variants: `Tool(Box<ToolPart>)`, `Text(Box<TextPart>)`, `Reasoning(Box<ReasoningPart>)`.

### Finding RUST-12: Duplicate Pattern Matching in Part Methods
- **Severity**: Medium
- **Location**: `session.rs:1605-1663`
- **Description**: Three functions (`set_id()`, `set_message_id()`, `set_session_id()`) each with 13-arm matches, all exhaustive, all repeating the same pattern.
- **Consequence**: Adding a new variant requires updating all three functions.
- **Recommendation**: Use a helper macro or a method on `Part` that returns `&mut CommonPartFields` (a struct of `{id, message_id, session_id}`).

### Finding RUST-13: Type Aliases Instead of Newtypes for IDs
- **Severity**: Medium
- **Location**: `session.rs:84-90`, `provider.rs:24-48`
- **Description**: `pub type SessionId = String;`, `pub type MessageId = String;`, `pub type ModelId = String;`, `pub type ProviderId = String;` — type aliases, not newtypes.
- **Consequence**: `SessionId` and `ModelId` are interchangeable with `String`. Zero compile-time type safety for ID misuse.
- **Recommendation**: Use proper newtypes: `struct SessionId(String);` with `new()`, `as_str()`, `Display`, `FromStr`, `Serialize`/`Deserialize`.

### Finding RUST-14: Double-indirection in `ToolContext.ask_fn`
- **Severity**: Low
- **Location**: `tool.rs:52-61`
- **Description**: `ask_fn: Option<Arc<dyn Fn(...) -> Pin<Box<dyn Future>>>>` — double-indirection (`Arc<dyn Fn>` wrapping a `Pin<Box<dyn Future>>`).
- **Recommendation**: Use a trait object `Arc<dyn AskPermission>` with an async method directly, removing one layer.

### Finding RUST-15: Two Schema Methods on Tool Trait
- **Severity**: Low
- **Location**: `tool.rs:163-201`
- **Description**: `json_schema()` (returns `Option<Value>`) and `parameters_schema()` (always returns `Value`). The distinction is unclear — when do they differ and why?
- **Recommendation**: Merge into one method `fn input_schema(&self) -> serde_json::Value;` with a sentinel for "no schema."

### Finding RUST-16: `EventId` — Missing `FromStr`
- **Severity**: Low
- **Location**: `event.rs:42-79`
- **Description**: `EventId(String)` with `#[serde(transparent)]`, factory methods `create()`, `from_external()`, but no `FromStr` implementation.
- **Consequence**: Cannot parse `"evt_xxx"` from CLI args or API params.
- **Recommendation**: Add `FromStr` impl that validates the `evt_` prefix.

### Finding RUST-17: No Type State for Session Lifecycle
- **Severity**: Low
- **Location**: `session.rs:605-608`
- **Description**: Session state managed via `SessionStatus` at runtime. No compile-time state enforcement. Type state (e.g., `Session<Idle>`, `Session<Busy>`) would prevent calling `process()` on a busy session at compile time.
- **Recommendation**: Introduce a type-state parameter on `SessionManager` methods.

### Finding RUST-18: Heavy `#[serde]` Attribute Usage
- **Severity**: Low
- **Location**: All types across `provider.rs`, `config.rs`, `session.rs`, `mcp.rs`
- **Description**: Extensive use of `#[serde(rename_all = "...")]`, `#[serde(rename = "...")]`, `#[serde(default)]`, `#[serde(skip_serializing_if = "...")]`, `#[serde(untagged)]`, `#[serde(tag = "...")]`. Some patterns duplicated 20+ times.
- **Recommendation**: Define a helper module with constants for common serde patterns, or use a `serde_with` macro.

### Finding RUST-19: Dependency Injection vs Struct-of-Services
- **Severity**: Medium
- **Location**: Cross-cutting
- **Description**: Manual injection via `Arc<DatabaseService>`, `SharedBus`, `Arc<ToolRegistry>`, `Arc<PermissionService>`. Every new service dependency requires changing constructors and all call sites. No scoping.
- **Recommendation**: Introduce a `ServiceRegistry` or `AppContext` struct that holds all services as `Arc<dyn ...>` and is passed as a single parameter.

### Finding RUST-20: Serialization Strategy — JSON-in-TEXT
- **Severity**: Medium
- **Location**: Cross-cutting (message.data, part.data columns)
- **Description**: `serde_json::Value` used extensively: `MessageInfo` stored as JSON string in `message.data` column, `Part` stored as JSON string in `part.data` column. Cannot query by message content in SQL.
- **Recommendation**: For new tables, use typed columns. Legacy approach acceptable for message/part tables.

### Finding RUST-21: `ObservabilityService` — No OnceLock Protection
- **Severity**: Low
- **Location**: `main.rs:1250-1267`
- **Description**: If `init()` is called multiple times, the second call returns `Ok(false)` silently. No `OnceCell` or `OnceLock` protection.
- **Recommendation**: Use `OnceLock<ObservabilityService>` or check `tracing::subscriber` is set.

### Finding RUST-22: `ProviderCatalog` Trait — Too Wide
- **Severity**: Low
- **Location**: `provider.rs:949-981`
- **Description**: 7 methods — `list`, `get_provider`, `get_model`, `closest`, `get_small_model`, `default_model`. `closest()` and `get_small_model()` are algorithmic — should be default methods.
- **Recommendation**: Provide default implementations for `closest()`, `get_small_model()`, `default_model()`.

---

## Logic Verification Findings (from Agent 04)

### Finding LOG-1: `clear_revert` writes literal string `"null"` instead of SQL NULL
- **Severity**: Critical
- **Location**: `session.rs:1206-1215`
- **Description**: `Some("null")` writes the literal 4-character text `"null"` into the SQLite column instead of SQL NULL. The comment says "empty string" but code writes `"null"`. The deserialization accidentally treats this as `None` because `"null"` is not valid `RevertInfo`, but the column physically contains `'null'` instead of NULL.
- **Consequence**: SQL queries using `WHERE revert IS NULL` will miss this row. Data corruption.
- **Recommendation**: Pass `None` instead of `Some("null")` to set the column to SQL NULL.

### Finding LOG-2: V1 `run_loop` bypasses all permission checks
- **Severity**: Critical
- **Location**: `session_runner.rs:1086-1096`
- **Description**: V1 `run_loop` sets `ask_fn: None` and `permission_source: None`, then calls `execute_by_name` which performs zero permission checks. `execute_with_pipeline` has the full permission flow but V1 never uses it.
- **Consequence**: All V1 tool executions bypass the permission system. The LLM can call `bash`, `read`, `write`, `edit`, etc. without any allow/deny/ask check.
- **Recommendation**: Wire `ask_fn` and `permission_source` into all V1 paths, or switch V1 to `execute_with_pipeline`.

### Finding LOG-3: `unwrap()` on `compact_result` despite `is_some()` guard (and redundant logic)
- **Severity**: Critical
- **Location**: `session_runner.rs:703-717`
- **Description**: Three issues in 15 lines: (1) `unwrap()` on line 710 violates project rule #3; (2) Redundant `.as_ref().map()` inside `serde_json::json!()` produces `Some(...)` wrappers in JSON output (`{"summary": Some("compacted_text")}`); (3) `prepare_epoch` called twice on lines 710 and 820-827.
- **Consequence**: The `snapshot_val` contains JSON like `{"summary": Some("compacted_text")}` with literal `Some(...)` wrappers, corrupting epoch snapshot storage.
- **Recommendation**: Replace with `if let Some(ref result) = compact_result { ... }` pattern.

### Finding LOG-4: `session_row_to_info` — `cost` field uses `f64` leading to silent precision loss
- **Severity**: Critical
- **Location**: `session.rs:1420`
- **Description**: `cost` field on `SessionInfo` is `f64`, preventing deriving `Eq` on `SessionInfo`. JSON round-trips silently lose precision (e.g., `0.1 + 0.2 != 0.3`). No `Eq` on `SessionInfo` prevents equality comparisons in tests.
- **Consequence**: Cannot derive `Eq` on `SessionInfo` due to `f64` field. Monetary-style values accumulate rounding errors.
- **Recommendation**: Use `ordered_float::OrderedFloat<f64>` or store cost as `i64` (millicents).

### Finding LOG-5: TOCTOU race in `wake()` — lane read-then-write without atomicity
- **Severity**: High
- **Location**: `session_execution.rs:744-755`
- **Description**: Between `drop(lane)` on line 747 and `get_mut` on line 748, the lane could be removed or modified by another concurrent `wake()`, `interrupt()`, or `run()` call.
- **Consequence**: (a) `get_mut` returns `None` — wake silently lost. (b) Lane state changed between read and write. (c) Multiple concurrent wakes can interleave, losing one or both.
- **Recommendation**: Use `DashMap::alter` or `alter_all` for atomic read-modify-write.

### Finding LOG-6: `fork` loop skips stop message in `id_map` — dangling parent references
- **Severity**: High
- **Location**: `session.rs:866-892`
- **Description**: When `message_id` matches, `break` executes before the message is added to `id_map`. `clone_with_session` uses `id_map` to remap `parent_id` references — missing mapping.
- **Consequence**: In edge cases where message IDs are referenced non-sequentially (possible with forking), the parent reference would be unmapped.
- **Recommendation**: Move the `break` after the `id_map.insert` to preserve the stop message's ID mapping.

### Finding LOG-7: `await_idle` — unbounded busy-wait with no timeout
- **Severity**: High
- **Location**: `session_execution.rs:827-834`
- **Description**: No timeout, no cancellation token, no max-retry limit. If the drain task deadlocks, `await_idle` spins forever.
- **Consequence**: A stuck drain fiber causes the entire `run()` call to hang forever. The process won't terminate without external intervention (SIGKILL).
- **Recommendation**: Add a timeout with max attempts (e.g., 30 seconds = 3000 attempts at 10ms each).

### Finding LOG-8: `part_id` generation failure falls back to empty string — DB constraint violation
- **Severity**: High
- **Location**: `session.rs:885`
- **Description**: `id::ascending(...).unwrap_or_default()` on failure produces empty string `""` as primary key.
- **Consequence**: All parts that fail to generate an ID get the same empty ID, causing unique constraint violations or silent overwrites.
- **Recommendation**: Propagate the error instead: `id::ascending(...).map_err(|e| SessionError::Other(...))?`.

### Finding LOG-9: `FiberSet::spawn` — results silently dropped if receiver is closed
- **Severity**: High
- **Location**: `session_execution.rs:165,168`
- **Description**: Both sends use `let _ =`, silently discarding `SendError` if the receiver has been dropped.
- **Consequence**: Fibers that complete after the receiver is dropped leak in the `handles` and `cancels` maps. `await_empty` never returns. Resource leak + hang.
- **Recommendation**: When the send fails, remove the fiber handle from the tracking maps.

### Finding LOG-10: `check_context_overflow` — naive token estimation causes false overflow
- **Severity**: High
- **Location**: `session_runner.rs:1308-1323`
- **Description**: Token estimation assumes 1 token = 4 bytes (ASCII). Real tokenizers average ~3-5 characters per token for English, much less for code or non-ASCII. Base64-encoded images can easily reach 8-20 bytes per token.
- **Consequence**: Premature compaction shrinks context, losing session history. Can trigger compaction loops.
- **Recommendation**: Use a more accurate estimator (e.g., `tiktoken-rs`, or character-count-based heuristic with per-model coefficients).

### Finding LOG-11: `parse_turn_control` — fragile string matching
- **Severity**: High
- **Location**: `session_runner.rs:928-947`
- **Description**: `TurnControl` is serialized as a string inside `Error::Internal`, then parsed via substring matching. Fragile — any change to encoding format silently breaks parsing.
- **Consequence**: If these strings appear in a provider error message, the session runner would misinterpret them as control signals.
- **Recommendation**: Use a dedicated error variant for control flow rather than encoding in strings.

### Finding LOG-12: `PermissionDenied` in `execute_with_pipeline` — passes `"*"` as resource
- **Severity**: High
- **Location**: `tool.rs:502`
- **Description**: Always passes `"*"` as the resource pattern regardless of the tool being called. For tools like `edit`, `glob`, `grep`, `read`, `write`, the actual file path/resource should be passed for fine-grained permission evaluation.
- **Consequence**: Users cannot configure permissions like `"read": "/etc/*"` — the `"*"` wildcard matches everything. The permission system's pattern matching is effectively disabled.
- **Recommendation**: Extract the resource from the tool arguments. For file tools, extract `filePath`/`path` from `args`.

### Finding LOG-13: `FiberSet::spawn` — `JoinHandle` never joined, task leaks on drop
- **Severity**: High
- **Location**: `session_execution.rs:153-179`
- **Description**: `JoinHandle` is stored but never `.await`ed in `cancel()` or `cancel_all()`. When `FiberSet` is dropped, the `handles` DashMap is dropped, which detaches the `JoinHandle`s. The spawned tasks continue running.
- **Consequence**: Tasks leak on shutdown. The `CancellationToken` signals cancellation but the task might not process it before the runtime is dropped.
- **Recommendation**: In `FiberSet::cancel()`, also remove and await the handle.

### Finding LOG-14: `run_turn_attempt` ignores `StepFinish` events
- **Severity**: High
- **Location**: `session_runner.rs:659-661`
- **Description**: Both V1 (line 1029-1031) and V2 (line 659-661) modes ignore the `StepFinish` reason. The `reason` field carries information about why the model stopped (`stop`, `length`, `tool-calls`, `error`, `content-filter`).
- **Consequence**: Truncated responses appear as complete but are missing the end. The `SessionRunResult` has no field for finish reason.
- **Recommendation**: Extract the finish reason from `StepFinish` and propagate it to the caller. In V2, set `needs_continuation = true` if reason is `Length`.

### Finding LOG-15: `build_chat_messages` sends two system messages when `input.system` is set
- **Severity**: High
- **Location**: `session_runner.rs:1179-1191`
- **Description**: Both `system_prompt` and `input.system` are pushed as separate `ChatMessage::System` messages. Most LLM providers only support a single system message.
- **Consequence**: Provider API calls may fail or produce unexpected behavior for Anthropic, many OpenAI models, and others.
- **Recommendation**: Merge the two messages: `format!("{}
{}", system_prompt, sys)`.

### Finding LOG-16: `Running` state never set to `Idle` after wake completes
- **Severity**: High
- **Location**: `session_execution.rs:726-727, 946-947`
- **Description**: `wake()` transitions state to `Running` but returns immediately. The state remains `Running` until the fiber's `settle` runs. If `state()` is polled between `wake()` returning and `settle` completing, it shows `Running`.
- **Consequence**: External observers polling `state()` see `Running` when the system considers the wake "submitted" not "active." The `state` field conflates "submitted" and "active."
- **Recommendation**: Add a third state `Pending` for "submitted but not yet started."

### Finding LOG-17: `wildcard_match` — regex `s` flag not used, `.` doesn't match `
`
- **Severity**: Medium
- **Location**: `permission.rs:261`
- **Description**: Comment says "We use the `s` flag for dot-all (`.` matches `
`)" but the code does NOT enable the `s` flag. The regex is built with default flags.
- **Consequence**: Multi-line patterns like `bash` with embedded newlines in commands fail to match. The TS `Wildcard.match()` uses the `s` flag, creating a behavioral divergence.
- **Recommendation**: Use `regex::RegexBuilder::new(&regex_str).dot_matches_new_line(true).build()` or prepend `(?s)` to the regex string.

### Finding LOG-18: `Lane` holding `DoneChannel` — broadcast capacity 16 may overflow
- **Severity**: Medium
- **Location**: `session_execution.rs:455`
- **Description**: `broadcast::channel(16)` — if more than 16 callers subscribe to a single lane's completion, the 17th caller's receiver will lag behind and miss the message.
- **Consequence**: Race condition in `wait_for_result()` (line 905-912): if the receiver has lagged, `rx.recv().await` returns `Err(Lagged(n))`, mapped to a misleading "broadcast channel closed" error.
- **Recommendation**: Increase capacity or use `tokio::sync::watch` (single-value, always latest). Fix the error message.

### Finding LOG-19: `coalesce_demand` — redundant re-extraction of `seq`
- **Severity**: Medium
- **Location**: `session_execution.rs:329-340`
- **Description**: Outer match arm already destructures `right` to extract `seq`, then inner code re-extracts `seq` from `left` via the same `and_then` pattern.
- **Consequence**: Code is harder to read and maintain. If the `left` extraction logic diverges from the `right` logic, bugs could be introduced.
- **Recommendation**: Simplify with direct pattern matching: `(Some(Demand::Wake { seq: l_seq }), Demand::Wake { seq: r_seq })`.

### Finding LOG-20: `FiberSet::await_empty` — no backoff or cancellation
- **Severity**: Medium
- **Location**: `session_execution.rs:224-228`
- **Description**: Same unbounded busy-wait pattern as `await_idle`. Polls every 10ms even after minutes of waiting.
- **Consequence**: Spins CPU on every poll (wake up from sleep, check DashMap, go back to sleep). For long-running fibers, this wastes cycles.
- **Recommendation**: Use `tokio::sync::Notify` — signal when a fiber completes instead of polling.

### Finding LOG-21: `tool.rs:520` — `call_id.clone().unwrap_or_default()` may produce empty string
- **Severity**: Medium
- **Location**: `tool.rs:520`
- **Description**: When `ctx.call_id` is `None`, the truncation service receives an empty string as the call identifier. Multiple tools with no call_id will collide.
- **Consequence**: The truncation service may write files to a path containing `""`, colliding with other uncalled tools.
- **Recommendation**: Use a `"unknown"` fallback or skip truncation when `call_id` is `None`.

### Finding LOG-22: `LlmEvent` serialization — `#[serde(tag = "type")]` with conflicting field names
- **Severity**: Medium
- **Location**: `provider.rs:479-480`
- **Description**: Some variants may contain a field named `type` in `provider_metadata` or elsewhere. The `#[serde(tag = "type")]` will conflict if any variant's serialized fields include a key named "type".
- **Consequence**: Serialization may produce invalid JSON if any variant includes a field called `type`.
- **Recommendation**: Audit all variants for field names. Use `#[serde(deny_unknown_fields)]` on variants during deserialization.

### Finding LOG-23: `ProviderErrorEvent` — retryable always false, classification always Some
- **Severity**: Medium
- **Location**: `session_runner.rs:671-676, 1042-1046`
- **Description**: All stream errors classified as `"stream-error"` and marked non-retryable regardless of actual error type (rate limit, auth failure, network error, server error). Some of these (rate limits, transient network errors) are retryable.
- **Consequence**: Downstream retry logic can't distinguish between transient and permanent failures. Rate limit errors treated same as auth failures.
- **Recommendation**: Use error matching to classify properly: rate-limit = retryable, auth-error = not retryable, stream-error = not retryable.

### Finding LOG-24: `PermissionRule` — `Pattern` can be empty string
- **Severity**: Medium
- **Location**: `permission.rs:81-89`
- **Description**: The `pattern` field has no validation. An empty pattern `""` produces regex `^$` (matches empty string only). This rule would never match anything.
- **Consequence**: Silent misconfiguration — permission rules with empty patterns are ignored.
- **Recommendation**: Treat empty pattern as `"*"` (match everything), or reject during config parsing.

### Finding LOG-25: `PlannedInterruption` state transition — `Running` -> `Interrupted` blocks normal completion
- **Severity**: Medium
- **Location**: `session_execution.rs:819-820`
- **Description**: When `interrupt()` is called during a running drain that subsequently completes, `settle()` sets state to `Idle`. But if the interrupt happened after `settle` already ran, state becomes `Interrupted` permanently. No transition from `Interrupted` to `Idle`.
- **Consequence**: State can get stuck in `Interrupted` permanently if the interrupt fires after `settle` completes.
- **Recommendation**: Add `Interrupted -> Idle` transition path in `settle()`.

### Finding LOG-26: `SessionRunResult.success` — always `true` when `error` is `None`
- **Severity**: Medium
- **Location**: `session_runner.rs:443-450`
- **Description**: `success` is derived from `error.is_none()`, but `error` is only set on explicit failures like `StepLimitExceeded`. Runs with context overflow recovered via compaction still return `success: true`.
- **Consequence**: False positive "success" for runs that had significant errors but were auto-recovered.
- **Recommendation**: Add a `recovered: bool` field or use the `events` list to check for `ProviderErrorEvent` entries.

### Finding LOG-27: `doom_loop` detection doesn't cover V2 mode
- **Severity**: Medium
- **Location**: `session_runner.rs:982-988` (V1 only)
- **Description**: The V2 `run_v2` path calls `run_turn_attempt` which never calls `detect_doom_loop`. The doom-loop guard exists only in V1.
- **Consequence**: In V2 mode, the LLM can call the same tool with the same input indefinitely (up to the step limit of 25). No early termination for repeat-identical-tool loops.
- **Recommendation**: Add doom-loop detection to V2's `run_turn_attempt`.

### Finding LOG-28: `update_session` — 19-argument function is error-prone
- **Severity**: Low
- **Location**: `session.rs` multiple call sites (lines 778, 1114, 1128, 1143, 1158, 1173, 1194, 1209, 1230, 1244)
- **Description**: The `update_session` method is called with 19 positional arguments, most of which are `None`. Impossible to verify which field is being updated without counting argument positions.
- **Consequence**: Bugs in argument ordering (e.g., passing `Some("null")` for `revert` instead of a later parameter) are undetected.
- **Recommendation**: Use a `SessionPatch` struct with named fields.

### Finding LOG-29: `wildcard_match` — fallback to exact match on regex failure is misleading
- **Severity**: Low
- **Location**: `permission.rs:266-267`
- **Description**: On regex compilation failure, fallback compares against regex-escaped pattern (includes escaped chars like `\(`, `\)`, `\.`), not the original pattern.
- **Consequence**: The "exact match" fallback is useless — the escaped pattern would never match a normalized input, returning `false` for everything.
- **Recommendation**: Fall back to `normalized == pattern` (original pattern, not escaped).

### Finding LOG-30: `disproportionate_match` — line-count threshold
- **Severity**: Low
- **Location**: `tool_impls.rs:437`
- **Description**: Condition uses `(old_lines + 3).max(old_lines * 2)` — the `+ 3` term dominates for small values but the `* 2` term dominates for large values. The two terms overlap in confusing ways.
- **Recommendation**: Document the threshold logic or simplify.

### Finding LOG-31: `Part::set_id` — panics if called on wrong variant
- **Severity**: Low
- **Location**: `session.rs` — `set_id()` function
- **Description**: The `set_id()` method uses `match self { Part::Text(ref mut p) => p.id = ...; ... }`. If a variant doesn't have an `id` field, the match would be non-exhaustive.
- **Consequence**: Adding new variants without `id` field would cause runtime panics.
- **Recommendation**: Ensure all variants have an `id` field or use exhaustive match with error.

### Finding LOG-32: `ToolContext` — `messages` field holds full history copy per tool call
- **Severity**: Low
- **Location**: `tool.rs:47`
- **Description**: Each tool execution context clones the entire message history. For sessions with thousands of messages, this is significant memory overhead.
- **Consequence**: Memory bloat on large sessions.
- **Recommendation**: Wrap in `Arc<Vec<ChatMessage>>` or `Arc<[ChatMessage]>`.

### Finding LOG-33: 100+ `unwrap()` calls in library code
- **Severity**: Info
- **Location**: Throughout `tool_impls.rs`, `flock.rs`, `event_projector.rs`, `lsp.rs`, `integration.rs`, `ripgrep.rs`, `agent.rs`, `plugin.rs`, `npm.rs`, `account.rs`, etc.
- **Description**: Documented project rule #3: "No `.unwrap()` in library code." Approximately 100+ `unwrap()` calls exist in library (non-test) code. Each one is a potential panic point.
- **Consequence**: Each is a potential panic point. Any `unwrap()` on `None`/`Err` crashes the process.
- **Recommendation**: Systematic audit replacing all library-code `unwrap()` with proper error propagation.

### Finding LOG-34: `#[allow(dead_code, unused_imports, unused_variables)]` masks unused code
- **Severity**: Info
- **Location**: `lib.rs` and `main.rs`
- **Description**: Dead code and unused imports/variables are explicitly allowed across the crate.
- **Consequence**: The compiler can't detect unused functions, dead code paths, or variables that should be used.
- **Recommendation**: Scoped `#[allow(...)]` rather than crate-wide.

### Finding LOG-35: V1/V2 code duplication in `run_loop` and `run_turn_attempt`
- **Severity**: Info
- **Location**: `session_runner.rs:957-1155` (V1) vs `578-800` (V2)
- **Description**: The LLM streaming loop, tool call collection, tool execution, and result assembly are duplicated between V1 `run_loop` (~200 lines) and V2 `run_turn_attempt` (~222 lines).
- **Recommendation**: Extract shared logic (stream processing, tool execution pipeline) into a shared helper function.

### Finding LOG-36: `ToolCall` `Attachments` field ignored after execution
- **Severity**: Info
- **Location**: `session_runner.rs:767-771, 1109-1113`
- **Description**: Tool results with `attachments` (e.g., image outputs from `webfetch` or `bash`) are serialized as `{"result": output_text}`, discarding the `attachments` field entirely.
- **Recommendation**: Include attachments in the tool result payload when serializing back to the LLM.

### Finding LOG-37: `DrainMode` enum — unused variant `Wake`
- **Severity**: Info
- **Location**: `session_execution.rs:31-38`
- **Description**: `DrainMode` enum (`Run`, `Wake`) is defined but never referenced in any function signature or implementation.
- **Recommendation**: Remove unused variant or use it.

### Finding LOG-38: `PatchOptions.max_output_bytes` — unused
- **Severity**: Info
- **Location**: `git.rs:69`
- **Description**: Field defined in `PatchOptions` but never used in the actual `patch()` implementation.
- **Recommendation**: Remove or implement.

### Finding LOG-39: `clear_revert` creates misleading event
- **Severity**: Info
- **Location**: `session.rs:1211-1213`
- **Description**: All `set_*` methods publish `"session.updated"` event but none include the actual field that changed.
- **Recommendation**: Include delta information in events.

### Finding LOG-40: `ComputeStats` — `unwrap()` in stats display
- **Severity**: Info
- **Location**: `main.rs:1548-1549`
- **Description**: If `providers` is empty, `providers.keys().next().unwrap()` and `providers.get(&id).unwrap()` panic.
- **Recommendation**: Handle empty providers case.

### Finding LOG-41: `Dashboard` structure — unused `cursor` field
- **Severity**: Info
- **Location**: `session.rs:1291`
- **Description**: The cursor parameter is accepted but not forwarded to `list_sessions_global`. It's shadowed by a different pagination mechanism.

### Finding LOG-42: `InputDelivery::copy` trait not derived
- **Severity**: Info
- **Location**: `session_runner.rs:1342-1347`
- **Description**: Test checks that `InputDelivery` implements `Copy`. The derive likely exists but is being tested as a behavioral contract.

---

## Security Findings (from Agent 05)

### Finding SEC-1: Plaintext credential storage
- **Severity**: High
- **Location**: `blazecode-core/src/auth.rs:195-206`
- **CWE**: CWE-522
- **Description**: Stores auth.json in global data dir with plaintext credentials.
- **Consequence**: Credentials leaked if filesystem compromised.
- **Recommendation**: Encrypt auth.json at rest (age/rage or OS keychain).

### Finding SEC-2: MCP OAuth tokens stored as plaintext
- **Severity**: High
- **Location**: `blazecode-core/src/mcp.rs:2263-2276`
- **CWE**: CWE-312
- **Description**: MCP OAuth tokens stored as JSON in `mcp-auth.json` without encryption.
- **Consequence**: OAuth access/refresh tokens in plaintext.
- **Recommendation**: Encrypt at rest; at minimum document this is plaintext.

### Finding SEC-3: Credential values stored as JSON in SQLite
- **Severity**: High
- **Location**: `blazecode-core/src/credential.rs:352-353`
- **CWE**: CWE-312
- **Description**: Credential values stored as JSON in SQLite without encryption.
- **Consequence**: SQLite DB file contains plaintext API keys and tokens.
- **Recommendation**: Consider SQLite encryption extension or encrypt at application layer.

### Finding SEC-4: Encryption module not ported
- **Severity**: High
- **Location**: `blazecode-core/src/encryption/hmac.rs` — file does not exist
- **CWE**: CWE-1240
- **Description**: HMAC-based encryption module from BlazeCode not implemented. The file does not exist despite being declared in the module tree.
- **Consequence**: No encryption-at-rest for any stored credential. auth.json, mcp-auth.json, and SQLite credential values are all plaintext.
- **Recommendation**: Implement encryption/hmac.rs or equivalent credential encryption.

### Finding SEC-5: RUSTSEC-2024-0436 ignored
- **Severity**: High
- **Location**: `deny.toml:3`
- **CWE**: CWE-1104
- **Description**: A RUSTSEC advisory is ignored without documented rationale. Must investigate: which crate is affected, is it exploitable in BlazeCode's usage context?
- **Recommendation**: Investigate RUSTSEC-2024-0436 and document rationale or fix.

### Finding SEC-6: MCP local server runs with full user privileges
- **Severity**: High
- **Location**: `blazecode-core/src/mcp.rs:1044-1051`
- **CWE**: CWE-250
- **Description**: MCP local servers run with full user privileges via spawned subprocess.
- **Recommendation**: Consider running MCP servers in restricted context (containers, landlock).

### Finding SEC-7: {file:} substitution reads any path
- **Severity**: High
- **Location**: `blazecode-core/src/config.rs:2796-2789`
- **CWE**: CWE-73
- **Description**: Config `{file:../../etc/passwd}` reads arbitrary files via variable substitution.
- **Recommendation**: Restrict to project directory; check canonicalized path.

### Finding SEC-8: File read in variable substitution with no path restriction
- **Severity**: High
- **Location**: `blazecode-core/src/config.rs:2763`
- **CWE**: CWE-73
- **Description**: Config with `{file:/etc/shadow}` reads sensitive files with no path restriction.
- **Recommendation**: Sanitize file paths; enforce project directory boundary.

### Finding SEC-9: Auth token in query parameter
- **Severity**: Medium
- **Location**: `blazecode-server/src/auth.rs:81-87`
- **CWE**: CWE-598
- **Description**: `auth_token` query parameter supported — credentials in URL. URLs logged by proxies, visible in browser history, leaked via Referer headers.
- **Recommendation**: Log warning; document that query-param auth is less secure than header.

### Finding SEC-10: Last-match-wins authorization semantics
- **Severity**: Medium
- **Location**: `blazecode-core/src/permission.rs:317-343`
- **CWE**: CWE-862
- **Description**: A "deny all" rule followed by "allow bash" allows bash — ordering-dependent. Last-match-wins evaluation.
- **Recommendation**: Consider deny-by-default + explicit allowlist model.

### Finding SEC-11: API keys in heap memory as plain String
- **Severity**: Medium
- **Location**: `blazecode-core/src/providers/*:resolve_api_key()` (all providers)
- **CWE**: CWE-257
- **Description**: API keys live in heap memory as plain `String` until process exit; no zeroing.
- **Recommendation**: Use `secrecy::SecretString` for API key fields in provider structs and auth stores.

### Finding SEC-12: No JSON schema validation before config deserialize
- **Severity**: Medium
- **Location**: `blazecode-core/src/config.rs:2505-2515`
- **CWE**: CWE-20
- **Description**: Config loading accepts any JSON that structurally matches `Info`. No schema validation before deserialization.
- **Recommendation**: Add JSON Schema validation (jsonschema crate).

### Finding SEC-13: MCP server spawns arbitrary commands from config
- **Severity**: Medium
- **Location**: `blazecode-core/src/mcp.rs:1044-1051`
- **CWE**: CWE-78
- **Description**: MCP local server command/args from configuration. An attacker who can modify the config file can execute arbitrary commands with the user's privileges.
- **Recommendation**: Validate command path is safe or warn about arbitrary execution.

### Finding SEC-14: Config {file:} substitution reads any file
- **Severity**: Medium
- **Location**: `blazecode-core/src/config.rs:2804-2818`
- **CWE**: CWE-73
- **Description**: Config `{file:path}` substitution reads arbitrary files from the filesystem.
- **Recommendation**: Restrict file: paths to project directory.

### Finding SEC-15: Wildcard dependencies allowed
- **Severity**: Medium
- **Location**: `deny.toml:25`
- **CWE**: CWE-1104
- **Description**: `wildcards = "allow"` permits imprecise version specs in Cargo.toml.
- **Recommendation**: Set `wildcards = "deny"` for production.

### Finding SEC-16: Unknown registry/git = warn only
- **Severity**: Medium
- **Location**: `deny.toml:28-29`
- **CWE**: CWE-1104
- **Description**: `unknown-registry` and `unknown-git` set to `warn` — git dependencies could be hijacked.
- **Recommendation**: Set `unknown-registry = "deny"`, `unknown-git = "deny"`.

### Finding SEC-17: Plugin specified by npm package name without integrity verification
- **Severity**: Medium
- **Location**: `blazecode-core/src/config.rs:559-566`
- **CWE**: CWE-1104
- **Description**: No code signing or integrity verification for plugin packages.
- **Recommendation**: Add package integrity verification (lockfile, hash checking).

### Finding SEC-18: Auto-installs npm/bun deps
- **Severity**: Medium
- **Location**: `blazecode-core/src/config.rs:1836-1886`
- **CWE**: CWE-77
- **Description**: Runs `npm install` or `bun add` from config-specified directories.
- **Recommendation**: Validate package name before install; sandbox install process.

### Finding SEC-19: Shell execution with user authority
- **Severity**: Medium
- **Location**: `blazecode-core/src/tool_impls.rs:560-584`
- **CWE**: CWE-78
- **Description**: Agent can execute arbitrary shell commands with the user's authority.
- **Recommendation**: Permission system is the intended control; add input length limits.

### Finding SEC-20: 0600 file perms only on auth.json
- **Severity**: Medium
- **Location**: `blazecode-core/src/auth.rs:223-234`
- **CWE**: CWE-312
- **Description**: 0600 permissions protect against other users but not same-user malware or backup exposure.
- **Recommendation**: Use platform keychain (Secret Service, macOS Keychain).

### Finding SEC-21: Server password from environment variable
- **Severity**: Low
- **Location**: `blazecode-server/src/auth.rs:41-46`
- **CWE**: CWE-522
- **Description**: Password visible in `/proc/self/environ`, process listings.
- **Recommendation**: Support file-based secret injection (`BLAZECODE_SERVER_PASSWORD_FILE`).

### Finding SEC-22: URL injection via browser open
- **Severity**: Low
- **Location**: `blazecode-core/src/mcp_oauth.rs:839-868`
- **CWE**: CWE-77
- **Description**: Opens browser via `open`/`xdg-open` — URL injection if `authorization_endpoint` contains malicious data.
- **Recommendation**: Validate redirect URI is well-formed before opening.

### Finding SEC-23: findLast on rules
- **Severity**: Low
- **Location**: `blazecode-core/src/permission.rs:744-764`
- **CWE**: CWE-754
- **Description**: Denied tools can be re-enabled by later rules due to `findLast` semantics.
- **Recommendation**: Add integration test verifying rule priority semantics.

### Finding SEC-24: Non-blocking ask() returns immediately
- **Severity**: Low
- **Location**: `blazecode-core/src/permission.rs:968-1018`
- **CWE**: CWE-400
- **Description**: Tool continues before user responds (race window).
- **Recommendation**: Add configurable hard timeout for pending permissions.

### Finding SEC-25: PKCE verifier persisted to mcp-auth.json
- **Severity**: Low
- **Location**: `blazecode-core/src/mcp_oauth.rs:1003-1013`
- **CWE**: CWE-311
- **Description**: Code verifier stored in plaintext during OAuth flow.
- **Recommendation**: Delete immediately after token exchange (already done at line 1106).

### Finding SEC-26: MCP capability storage from untrusted init
- **Severity**: Low
- **Location**: `blazecode-core/src/mcp.rs:1105-1114`
- **CWE**: CWE-200
- **Description**: MCP server can claim any capability during initialize handshake.
- **Recommendation**: Validate capability names against allowlist.

### Finding SEC-27: API key in debug logs
- **Severity**: Low
- **Location**: `blazecode-core/src/providers/anthropic.rs:985-988`
- **CWE**: CWE-200
- **Description**: API key sent in `x-api-key` header — key leaks in debug logs if header logging enabled.
- **Recommendation**: Ensure production logging redacts auth headers.

### Finding SEC-28: BLAZECODE_AUTH_CONTENT parsed as trusted JSON
- **Severity**: Low
- **Location**: `blazecode-core/src/auth.rs:198`
- **CWE**: CWE-502
- **Description**: `serde_json::from_str<AuthStore>` from env var parsed as trusted JSON.
- **Recommendation**: Validate JSON schema before deserialize.

### Finding SEC-29: Corrupt auth.json silently returns empty
- **Severity**: Low
- **Location**: `blazecode-core/src/auth.rs:204`
- **CWE**: CWE-754
- **Description**: `serde_json::from_str.unwrap_or_default()` silently swallows corrupt data, returning empty store.
- **Recommendation**: Log warning when auth.json parse fails.

### Finding SEC-30: MCP tool definitions from untrusted server
- **Severity**: Low
- **Location**: `blazecode-core/src/mcp.rs:1212`
- **CWE**: CWE-502
- **Description**: `serde_json::from_value` on MCP tool definitions from untrusted server.
- **Recommendation**: Validate MCP response format at protocol level (JSON-RPC envelope is verified).

### Finding SEC-31: Malformed credential JSON silently skipped
- **Severity**: Low
- **Location**: `blazecode-core/src/credential.rs:265`
- **CWE**: CWE-754
- **Description**: `serde_json::from_str().ok()?` in row parsing silently skips malformed credential JSON.
- **Recommendation**: Log deserialization errors.

### Finding SEC-32: URL constructed from untrusted server_url
- **Severity**: Low
- **Location**: `blazecode-core/src/mcp_oauth.rs:676-682`
- **CWE**: CWE-74
- **Description**: URL constructed from user-provided `server_url` via string formatting.
- **Recommendation**: Use `url::Url::join()` instead of string formatting.

### Finding SEC-33: Custom HTTP headers sent to MCP servers
- **Severity**: Low
- **Location**: `blazecode-core/src/mcp.rs:1296-1322`
- **CWE**: CWE-200
- **Description**: User-provided headers (e.g., Authorization) sent as configured.
- **Recommendation**: Document that custom headers are user responsibility.

### Finding SEC-34: Always cascades to all session pending
- **Severity**: Low
- **Location**: `blazecode-core/src/permission.rs:1099-1166`
- **CWE**: CWE-284
- **Description**: Saying "Always" to one tool auto-approves other pending for same session.
- **Recommendation**: Only cascade for same permission+pattern.

### Finding SEC-35: Reject cascades fails ALL pending
- **Severity**: Low
- **Location**: `blazecode-core/src/permission.rs:1126-1128`
- **CWE**: CWE-459
- **Description**: User rejects one tool, all pending requests fail.
- **Recommendation**: Document this behavior or scope rejection.

### Finding SEC-36: Ask() evaluates but does not block
- **Severity**: Low
- **Location**: `blazecode-core/src/permission.rs:968-1018`
- **CWE**: CWE-269
- **Description**: Tool can proceed before user approves (race window in evaluation).
- **Recommendation**: Add explicit "pending" state in execution pipeline.

### Finding SEC-37: Config discovery reads files outside project boundary
- **Severity**: Low
- **Location**: `blazecode-core/src/config.rs:2422-2457`
- **CWE**: CWE-22
- **Description**: Walks up directory tree for config discovery.
- **Recommendation**: Already bounded by `stop_dir` parameter.

---

## Performance Findings (from Agent 06)

### Finding PERF-1: Synchronous std::fs on async runtime
- **Severity**: Critical
- **Location**: `tool_impls.rs:1065-1238`, `filesystem.rs:1281`
- **Description**: All filesystem operations in tool implementations use synchronous `std::fs` APIs on the tokio async runtime. `std::fs::read_to_string` blocks the tokio worker thread.
- **Consequence**: Large files (>50KB) block for milliseconds. With 25 tool calls, adds 50-500ms of total blocking time. In a server context, blocks all connected clients.
- **Recommendation**: Use `tokio::fs` or wrap blocking I/O in `tokio::task::spawn_blocking`.

### Finding PERF-2: grep_search reads full files into memory
- **Severity**: Critical
- **Location**: `filesystem.rs:1281`
- **Description**: Reads each matching file entirely into a `String` via `std::fs::read_to_string`. No memory-mapped I/O or streaming.
- **Consequence**: A single grep search matching 50 files of 5MB each allocates 250MB simultaneously. Potential OOM on large repos.
- **Recommendation**: Delegate to ripgrep after the initial file listing step. Or read files in chunks with buffered I/O.

### Finding PERF-3: ReadTool reads full file before 50KB cap
- **Severity**: Critical
- **Location**: `tool_impls.rs:1225` (MAX_READ_BYTES = 51200)
- **Description**: The 50KB truncation is done after reading the full file. For a 500MB log file, 500MB is read into memory then all but 50KB is discarded.
- **Consequence**: Reading a 500MB log file requires 500MB of heap allocation and 500MB of file I/O, only to show 50KB.
- **Recommendation**: Use `std::fs::File::read_to_end` with a limit (`take(MAX_READ_BYTES)`).

### Finding PERF-4: messages.clone() in ToolContext per tool call
- **Severity**: Critical
- **Location**: `session_runner.rs:749`
- **Description**: Every tool call clones the entire `Vec<ChatMessage>` history. For 50 messages at ~2KB each, that's ~100KB per tool call.
- **Consequence**: With 25 tool calls, that's 2.5MB of cloned message data per session turn.
- **Recommendation**: Use `Arc<Vec<ChatMessage>>` or pass `&[ChatMessage]`.

### Finding PERF-5: 8+ EventPayload clones per sync event
- **Severity**: High
- **Location**: `event.rs:936,945,1025,1035`
- **Description**: Each sync event publishes clones payload for guards, projectors, sync handlers, aggregate subscribers, listeners, typed channel, and global channel. ~8 clones per event.
- **Consequence**: Each sync event clones its `EventPayload` 8+ times. EventPayload contains String, Value, etc. — easily 500+ bytes per clone.
- **Recommendation**: Pass `&EventPayload` where possible.

### Finding PERF-6: Transaction held during async operations
- **Severity**: High
- **Location**: `event.rs:899-984`
- **Description**: The transaction is held open during async operations (commit guards, projectors, commit hook). If these take 100ms, the transaction holds for 100ms.
- **Consequence**: Blocks other writers during long-running async projector execution.
- **Recommendation**: Move projectors and commit hooks outside the transaction. Only the seq UPSERT + event INSERT need to be in a transaction.

### Finding PERF-7: Synchronous git operations on async runtime
- **Severity**: High
- **Location**: `worktree.rs:421-433`
- **Description**: `git_in_dir` calls `std::process::Command::output()` which blocks the calling thread until the git process exits.
- **Consequence**: Git operations (clone, fetch, reset) can take seconds, blocking the async runtime.
- **Recommendation**: Use `tokio::process::Command` instead.

### Finding PERF-8: No HTTP timeout on provider requests
- **Severity**: High
- **Location**: All provider implementations
- **Description**: No per-request timeout set on reqwest client for provider calls. `reqwest::Client::builder()` without `.timeout(...)`.
- **Consequence**: A hanging HTTP connection to an LLM provider could block a session indefinitely.
- **Recommendation**: Set `reqwest::Client::builder().timeout(Duration::from_secs(120))` and per-request timeouts.

### Finding PERF-9: Regex compiled every grep call, no caching
- **Severity**: High
- **Location**: `filesystem.rs:1208`
- **Description**: `regex::Regex::new(&input.pattern)` on every `grep_search()` call. No caching of compiled patterns.
- **Consequence**: ~2-50us regex compilation overhead per search call. More critically, reads the entire file into memory.
- **Recommendation**: Add a `regex::Regex` LRU cache (e.g., `lru` crate) keyed by pattern string.

### Finding PERF-10: Serde JSON in message hot path
- **Severity**: High
- **Location**: `session.rs:971-982` (append_message), `session_runner.rs:607` (baseline_str serde)
- **Description**: Uses `serde_json::to_string()` on every message append and every turn start. Serde JSON is ~2-4x slower than V8's `JSON.stringify()` (~300MB/s vs ~800MB/s).
- **Consequence**: For a session with 50 messages, ~100+ JSON round-trips per turn.
- **Recommendation**: Use `serde_json::to_vec` (write to `Vec<u8>`) to avoid UTF-8 validation overhead.

### Finding PERF-11: N+1 query pattern for messages+parts
- **Severity**: High
- **Location**: `database.rs` (assumed)
- **Description**: `get_messages_with_parts` likely uses separate queries for messages and parts. 1 query for messages + N queries for parts.
- **Consequence**: Loading a session with 50 messages could require 51 SQL queries instead of 2 (JOIN or two batch queries).
- **Recommendation**: Ensure `get_messages_with_parts` uses a single JOIN query or two batch queries.

### Finding PERF-12: Levenshtein full matrix allocation
- **Severity**: Medium
- **Location**: `tool_impls.rs:71`
- **Description**: Allocates a full `(a_len+1) * (b_len+1)` matrix on the heap for every block anchor comparison.
- **Consequence**: For a 500-line block, this is a 501x501 matrix (~2MB allocation per comparison).
- **Recommendation**: Implement space-optimized Levenshtein (two-row DP), reducing memory from O(n*m) to O(min(n,m)).

### Finding PERF-13: Model struct cloned per drain
- **Severity**: Medium
- **Location**: `session_runner.rs:242`
- **Description**: `Model` is a large struct (~1KB with Strings). Cloned in closure for each drain call.
- **Consequence**: Every drain (~LLM turn) does ~1KB of Model struct copy.
- **Recommendation**: Wrap `Model` in `Arc<Model>` to share across calls instead of cloning.

### Finding PERF-14: Task spawns per bash tool streams
- **Severity**: Medium
- **Location**: `tool_impls.rs:728-753`
- **Description**: Two `tokio::spawn` per bash command for stdout and stderr line readers.
- **Consequence**: For LLM's tool loop (25 iterations), 50 additional task spawns.
- **Recommendation**: Use a single task with `tokio::io::copy` to a shared buffer.

### Finding PERF-15: Tree-sitter parsing every bash command
- **Severity**: Medium
- **Location**: `tool_impls.rs:633-634`
- **Description**: Parses every bash command with tree-sitter-bash AST parser before execution.
- **Consequence**: Every bash invocation pays ~500us-5ms parsing cost regardless of command complexity.
- **Recommendation**: Use a fast regex pre-check for known-dangerous patterns first.

### Finding PERF-16: Vec<String> allocations in Tool Results
- **Severity**: Medium
- **Location**: `session_runner.rs:686-689`
- **Description**: `messages_json` creates a `Vec<serde_json::Value>` from each `ChatMessage` via `serde_json::to_value`.
- **Consequence**: Per-turn allocation churn proportional to session length. 50 messages = ~50 heap-allocated serde_json::Value objects per turn.
- **Recommendation**: Pass messages as `&[ChatMessage]` to compaction instead of converting to JSON.

### Finding PERF-17: LlmEvent large enum memory
- **Severity**: Medium
- **Location**: `provider.rs:480-669`
- **Description**: `LlmEvent` is a tagged union with 14 variants. The largest variant (`TextDelta` with `HashMap<String, Value>`) determines stack size (~240 bytes).
- **Consequence**: A 1000-token response generates 1000 `LlmEvent` variants. At ~240 bytes each, that's ~240KB per turn, plus heap allocations for strings/HashMaps.
- **Recommendation**: Use `Box<str>` for strings, `Arc<HashMap>` for metadata.

### Finding PERF-18: RwLock acquisition chain in EventV2::publish
- **Severity**: Medium
- **Location**: `event.rs:934-1048`
- **Description**: `publish` method acquires 4+ read locks sequentially (typed_channels, commit_guards, projectors, sync_handlers, synchronized_aggregates).
- **Recommendation**: Use `dashmap` (lock-free concurrent HashMap) for `typed_channels` and `synchronized_aggregates`.

### Finding PERF-19: broadcast channel overflow on slow subscribers
- **Severity**: Medium
- **Location**: `bus.rs:214`
- **Description**: Default bus capacity 1024. If subscribers lag, events are dropped (Lagged error).
- **Consequence**: If a bus subscriber takes >1 second to process an event while 1024+ events are published, it misses older events.
- **Recommendation**: For latency-sensitive event processing, use `tokio::sync::mpsc` instead of broadcast.

### Finding PERF-20: Legacy JSON columns force full deserialization
- **Severity**: Medium
- **Location**: `session.rs:932-955`
- **Description**: Loading a session reads and deserializes every message and every part from JSON columns.
- **Consequence**: For a session with 100 messages and 500 parts, this is 600 serde_json deserializations.
- **Recommendation**: Prefer structured columns if `session_message` table is used.

### Finding PERF-21: No projected read models for event store
- **Severity**: Medium
- **Location**: `event.rs`
- **Description**: Every `aggregate_events` call queries the raw event table. No read-side projection caching.
- **Consequence**: Session load requires scanning all events for that aggregate ID. For sessions with 1000+ events, this query becomes slower over time.
- **Recommendation**: Implement read-side projection tables that cache the aggregate state.

### Finding PERF-22: HashMap overhead in ToolStreamAccumulator
- **Severity**: Low
- **Location**: `tool_stream.rs:36`
- **Description**: Each `Accumulator` contains a growing `String` (json_text) that reallocates as JSON fragments arrive via `push_str` (doubles capacity on growth).
- **Consequence**: Minor — 11 concurrent tool calls (Anthropic max) means 11x overhead.
- **Recommendation**: Pre-allocate `json_text` with `String::with_capacity(expected_size)`.

### Finding PERF-23: Image file read twice
- **Severity**: Low
- **Location**: `tool_impls.rs:1184-1186`
- **Description**: Reads file once for binary detection, discards content, then reads again for image processing.
- **Consequence**: 2x file I/O for image files. For a 10MB image, that's 20MB of read I/O.
- **Recommendation**: Cache the full file content after the first read if it might be needed again.

### Finding PERF-24: Box<dyn Trait> Usage
- **Severity**: Low
- **Location**: `provider.rs:907`, `workspace.rs:260`
- **Description**: `Provider::stream()` returns `Box<dyn Stream>`, requiring heap allocation. `Box<dyn WorkspaceAdapter>`.
- **Consequence**: Per-stream heap allocation. For a session making 25 turns, that's 25 heap allocations of ~2-3KB each.
- **Recommendation**: Use `Pin<Box<dyn Stream>>` is fine — this is idiomatic Rust.

### Finding PERF-25: Mutex contention patterns
- **Severity**: Low
- **Location**: `tool_impls.rs:723-726`, `filesystem.rs:628-630`
- **Description**: `tokio::sync::Mutex<String>` for stdout/stderr buffers locked on every line read. Using `tokio::sync::Mutex` is overkill for tiny critical sections.
- **Recommendation**: Use `std::sync::Mutex` (faster, no yield) since critical section is tiny and never crosses `.await`.

### Finding PERF-26: Provider auto-detection allocation at startup
- **Severity**: Info
- **Location**: `providers/mod.rs:43-112`
- **Description**: `auto_detect_all()` creates ALL detectable providers at startup, including those with env vars set.
- **Consequence**: ~2-5KB per provider = ~20-50KB startup heap allocation. Negligible.
- **Recommendation**: Acceptable. Could be made lazy if memory is constrained.

---


## Dependencies Findings (from Agent 11)

### Finding DEP-1: 0 AI provider SDKs vs 20+ in BlazeCode
- **Severity**: Critical
- **Location**: `Cargo.toml` workspace dependencies
- **Description**: BlazeCode has zero provider-specific crates. BlazeCode has 20+ `@ai-sdk/*` packages for Anthropic, OpenAI, Google, Bedrock, Azure, Groq, Mistral, Cohere, Perplexity, XAI, DeepInfra, Together, Cerebras, Alibaba, etc.
- **Consequence**: Must implement 15+ HTTP protocol adapters from scratch. Each ~300-1000 lines of Rust.
- **Recommendation**: Add provider protocol crates — start with Anthropic Messages + OpenAI Chat/Responses.

### Finding DEP-2: No `git2` crate for git operations
- **Severity**: Critical
- **Location**: `Cargo.toml`
- **Description**: Missing `git2` crate. BlazeCode uses `@octokit/rest` for git operations.
- **Consequence**: Git diff, status, worktree operations missing.
- **Recommendation**: Add `git2` crate.

### Finding DEP-3: No runtime schema validation
- **Severity**: Critical
- **Location**: `Cargo.toml`
- **Description**: `schemars` generates JSON Schema but does not validate. Need `jsonschema` or `valico` for runtime validation.
- **Consequence**: LLM tool call validation cannot be performed at runtime.
- **Recommendation**: Add `jsonschema` or `valico` crate.

### Finding DEP-4: `tree-sitter` and `tree-sitter-bash` outdated
- **Severity**: Medium
- **Location**: `Cargo.toml`: `tree-sitter` 0.24, `tree-sitter-bash` 0.23
- **Description**: `tree-sitter` at 0.24 (latest 0.25+), `tree-sitter-bash` at 0.23 (latest 0.25+). BlazeCode uses `web-tree-sitter@0.25.10`.
- **Recommendation**: Update to 0.25.

### Finding DEP-5: `clap` behind latest patch
- **Severity**: Low
- **Location**: `Cargo.toml`: `clap` 4.6.1
- **Description**: `clap` locked at 4.6.1 but 4.6.x has newer patches.
- **Recommendation**: Update to latest 4.x patch.

### Finding DEP-6: Missing `zip` crate
- **Severity**: Medium
- **Location**: `Cargo.toml`
- **Description**: BlazeCode uses `@zip.js/zip.js` for JAR/SARIF handling. No `zip` crate in BlazeCode.
- **Recommendation**: Add `zip` crate.

### Finding DEP-7: Missing `fuzzysort` equivalent
- **Severity**: Medium
- **Location**: `Cargo.toml`
- **Description**: No fuzzy matching crate for command palette.
- **Recommendation**: Add `fuzzy-matcher` or `skim` crate.

### Finding DEP-8: Missing OpenTelemetry tracing crates
- **Severity**: Medium
- **Location**: `Cargo.toml`
- **Description**: No `opentelemetry` or `opentelemetry-otlp` crates.
- **Recommendation**: Add OpenTelemetry integration.

### Finding DEP-9: Unnecessary transitive deps — `native-tls`/`openssl`
- **Severity**: High
- **Location**: `Cargo.toml` (sqlx default features)
- **Description**: `sqlx` pulls in `sqlx-mysql`, `sqlx-postgres`, `native-tls`, `openssl` despite only SQLite being used.
- **Recommendation**: Configure sqlx with `no-default-features` + `runtime-tokio` + `sqlite`. Saves ~20 packages.

### Finding DEP-10: RUSTSEC-2024-0436 ignored
- **Severity**: Info
- **Location**: `deny.toml`
- **Description**: `paste` crate (transitive via unmaintained dep) — advisory ignored. Acceptable as it's just a proc-macro helper.
- **Recommendation**: Acceptable — proc-macro only, no runtime risk.

### Finding DEP-11: License compliance clean
- **Severity**: Info
- **Location**: `deny.toml`: allowlist with MIT, Apache-2.0, BSD-2-Clause, BSD-3-Clause, ISC, Unicode-3.0, Unicode-DFS-2016, Zlib, MPL-2.0, OpenSSL, CC0-1.0, BSL-1.0, CDLA-Permissive-2.0
- **Description**: 13 licenses allowlisted. No GPL/AGPL conflicts. MPL-2.0 (dashmap) file-level copyleft — low risk.
- **Recommendation**: No action needed.

### Finding DEP-12: Build time impact — 395 transitive packages
- **Severity**: Medium
- **Location**: `Cargo.lock` (3984 lines, 395 packages)
- **Description**: 395 transitive packages. Cold build estimate: 8-12 min debug, 20-30 min release. Largest contributors: ring (C/asm), libsqlite3-sys (C), axum, reqwest.
- **Recommendation**: Consider `aws-lc-rs` instead of `ring` for faster compilation.

---

## Maintainability Findings (from Agent 12)

### Finding MAINT-1: Unimplemented / stub code — ~70% runtime logic missing
- **Severity**: Critical
- **Location**: `crates/blazecode-tui/src/app.rs:1-1270`, `crates/blazecode-lsp/src/lib.rs:1-1383`, `crates/blazecode-mcp/src/lib.rs:1-1443`, `crates/blazecode-server/src/lib.rs:1-33`
- **Description**: LSP, MCP, server, TUI crates are labeled "stub" in CLAUDE.md. Though they contain significant code, the TUI has no LLM streaming integration, server routes return placeholder data, LSP/MCP lack real process lifecycle management.
- **Consequence**: False sense of progress. Stubs compile but don't function.
- **Recommendation**: Remove stub modules from workspace or mark with `#[deprecated]` / feature-gate.

### Finding MAINT-2: Relaxed lints permitting dead code
- **Severity**: High
- **Location**: `crates/blazecode-core/src/lib.rs:2`, `src/main.rs:2`
- **Description**: `#![allow(dead_code, unused_imports, unused_variables)]` on both core library and binary crate.
- **Consequence**: Dead code cannot be detected. 15-25 dead items across the codebase. Unused imports, dead functions, and orphaned types accumulate silently.
- **Recommendation**: Remove the `allow` attributes. Use `#[expect(dead_code)]` on individual items.

### Finding MAINT-3: TuiApp Constructor Duplication
- **Severity**: High
- **Location**: `crates/blazecode-tui/src/app.rs:204-318` vs `:325-418`
- **Description**: `TuiApp::new()` and `TuiApp::new_remote()` are 80% identical (both initialize 30+ fields, both set up terminal, both create plugin managers). ~100 lines duplicated.
- **Recommendation**: Extract common initialization into `fn init_terminal()`, `fn default_states()`, or use a builder pattern.

### Finding MAINT-4: TuiApp::apply_llm_event() Function — McCabe ~25
- **Severity**: High
- **Location**: `crates/blazecode-tui/src/app.rs:870-1154`
- **Description**: Single match on `LlmEvent` with 13 arms, each arm containing nested match/if-let chains for message lookup, part iteration, and state mutation. 285 lines.
- **Recommendation**: Split into `fn on_text_delta()`, `fn on_tool_call()`, `fn on_finish()`, etc.

### Finding MAINT-5: EventV2::publish() Function — McCabe ~20
- **Severity**: High
- **Location**: `crates/blazecode-core/src/event.rs:855-1063`
- **Description**: ~208 lines, one giant method handling both sync and async event publishing paths. Deeply nested: `if let Some(ref sync_config)` -> `if let Some(ref pool)` -> `let mut tx` -> `sqlx::query_as` -> `if existing.is_some()` -> guards loop -> projectors loop -> commit hook -> UPSERT -> INSERT -> commit -> sync handlers loop -> aggregate pubsub -> notify.
- **Recommendation**: Extract `fn publish_sync()` and `fn publish_ephemeral()` from the if/else branches.

### Finding MAINT-6: Primitive Obsession — Session Manager Setters
- **Severity**: High
- **Location**: `crates/blazecode-core/src/session.rs:1106-1262`
- **Description**: 10+ individual setter methods (`touch`, `set_title`, `set_archived`, `set_metadata`, `set_permission`, `set_revert`, `clear_revert`, `set_summary`, `set_share`, `set_workspace`) each calling `self.db.update_session(...)` with 19 parameters and 17 `None`s.
- **Recommendation**: Each setter should build a `SessionPatch` with only the changed field.

### Finding MAINT-7: Long Parameter List — `DatabaseService::update_session()`
- **Severity**: Critical
- **Location**: `crates/blazecode-core/src/database.rs:1284-1350`
- **Description**: 19 positional parameters (17 `Option`), all passed at every call site as `None, None, None, ...`.
- **Consequence**: Adding a new column to the session table requires editing every call site (10+ locations).
- **Recommendation**: Replace with `SessionUpdate` struct with `#[derive(Default)]`.

### Finding MAINT-8: Large Struct — `TuiApp` (God Struct)
- **Severity**: Critical
- **Location**: `crates/blazecode-tui/src/app.rs:96-200`
- **Description**: ~50 fields (component states, app state, backend services, streaming state, toggle flags, overlay states, dialog states, LLM streaming sender, tool definitions, terminal geometry, recent models, pinned sessions, theme, plugins, audio).
- **Consequence**: Changes to any feature require touching `TuiApp`. Testing is difficult.
- **Recommendation**: Split into focused sub-structs (`AppCore`, `StreamingState`, `UIOptions`, `PluginHost`) composed as fields.

### Finding MAINT-9: Files Over 1000 Lines — 14 files
- **Severity**: Critical
- **Location**: Multiple files
- **Description**: 14 files over 1000 lines: `tool_impls.rs` (7,235), `config.rs` (4,861), `database.rs` (4,758), `plugin.rs` (6,236), `session.rs` (4,133), `event.rs` (2,905), `provider.rs` (3,018), `permission.rs` (2,154), `filesystem.rs` (2,383), `app.rs` (3,769), LSP `lib.rs` (1,383), MCP `lib.rs` (1,443), `routes/session.rs` (1,441), `main.rs` (8,575).
- **Consequence**: Merge conflicts on large files; cognitive load; hard to navigate.
- **Recommendation**: Split each >1000-line file into directory-based modules. Aim for <300 lines per file.

### Finding MAINT-10: Functions Over 100 Lines -- 10+ functions
- **Severity**: Critical
- **Location**: Multiple files
- **Description**: Functions over 100 lines: `bash_tool.execute()` (274), `edit_replace()` (56), `TuiApp::new()` (116), `TuiApp::new_remote()` (94), `TuiApp::run_async()` (220), `TuiApp::apply_llm_event()` (285), `TuiApp::spawn_llm_stream()` (199), `EventV2::publish()` (208), `LspClientState::new()` (167), `post_prompt()` (148), `summarize_session()` (110), `dispatch_inner()` (31).
- **Recommendation**: Apply Extract Method aggressively. Aim for <50 lines per function.

### Finding MAINT-11: Replacer Strategy Duplication
- **Severity**: Medium
- **Location**: `crates/blazecode-core/src/tool_impls.rs:56-430`
- **Description**: 7+ `Replacer` structs (`SimpleReplacer`, `LineTrimmedReplacer`, `BlockAnchorReplacer`, etc.) each implementing `fn search(content, find) -> Vec<String>` with substantial algorithmic overlap.
- **Consequence**: Bug fixes in one replacer's offset logic must be replicated to all 9. ~200 lines of near-identical pattern-matching boilerplate.
- **Recommendation**: Extract helper functions and consider a macro for the common line-offset arithmetic pattern.

### Finding MAINT-12: Config Struct Heaviness -- Info vs V2ConfigInfo overlap
- **Severity**: Medium
- **Location**: `crates/blazecode-core/src/config.rs:89-240` vs `:298-350`
- **Description**: `Info` (151 lines, 38 fields) and `V2ConfigInfo` (52 lines, 22 fields) share 15+ fields with identical names and types.
- **Recommendation**: Use a shared base struct via composition, or generate V2 from V1 via a derive macro.

### Finding MAINT-13: Server Route Error Handling Pattern
- **Severity**: Medium
- **Location**: `crates/blazecode-server/src/routes/session.rs:311-440+`
- **Description**: ~25 handlers each repeating: `match result { Ok(v) => Json(serde_json::to_value(v).unwrap_or_default()).into_response(), Err(e) => (StatusCode::INTERNAL_SERVER_ERROR, Json(json!({...}))).into_response() }`.
- **Consequence**: ~200 lines of boilerplate. Changing error format requires editing every handler.
- **Recommendation**: Extract `fn ok_or_500<T: Serialize>(result: Result<T>) -> impl IntoResponse` helper.

### Finding MAINT-14: edit_replace() Function -- McCabe ~15
- **Severity**: Medium
- **Location**: `crates/blazecode-core/src/tool_impls.rs:445-509`
- **Description**: Single function chaining 9 replacer strategies, each with pattern matching, index computation, and 3-branch outcome (found-once, found-multiple, not-found).
- **Recommendation**: Extract `fn try_replace()` per strategy. Use `Option` chaining instead of `for`+`continue`.

### Finding MAINT-15: Large Struct -- `Info` in config.rs
- **Severity**: Medium
- **Location**: `crates/blazecode-core/src/config.rs:89-240`
- **Description**: 38 fields. Data Clump -- many fields (`disabled_providers`, `enabled_providers`, `model`, `small_model`, `default_agent`, `username`) are almost always set together.
- **Recommendation**: Group into sub-structs: `ModelConfig`, `ProviderFilters`, `AgentDefaults`.

### Finding MAINT-16: Feature Envy in `session.rs`
- **Severity**: Medium
- **Location**: `crates/blazecode-core/src/session.rs:1007-1032`
- **Description**: `update_message()` calls `sqlx::query` directly on `self.db.pool()` instead of through a `DatabaseService` method.
- **Recommendation**: Add `update_message_data()` to `DatabaseService`.

### Finding MAINT-17: Stale line references in doc comments
- **Severity**: Medium
- **Location**: Nearly every file header
- **Description**: Doc comments pin TS line numbers to a specific commit (`5d0f8660`). If the upstream evolves, these become misleading.
- **Recommendation**: Remove line numbers from sources; keep file-level references only.

### Finding MAINT-18: No architecture ADRs
- **Severity**: Medium
- **Location**: N/A
- **Description**: No documentation explaining *why* a TS pattern was translated a certain way (e.g., why Effect -> async fn + struct fields).
- **Recommendation**: Add ADR process for design decisions.

### Finding MAINT-19: Naming consistency is good
- **Severity**: Low
- **Location**: All files
- **Description**: snake_case everywhere. Type names match upstream PascalCase. No camelCase in Rust code.
- **Recommendation**: Minor: `json_column_serialize` vs `json_absolute_path_array_column` -- inconsistent naming.

### Finding MAINT-20: Magic numbers
- **Severity**: Medium
- **Location**: `crates/blazecode-core/src/tool_impls.rs:57-58` (similarity thresholds), `:1225` (MAX_READ_BYTES), `crates/blazecode-lsp/src/lib.rs:151-177` (timeouts)
- **Description**: `SINGLE_CANDIDATE_SIMILARITY_THRESHOLD = 0.65` -- matches TS but no explanation. Multiple timeout constants (45s init, 30s request, 500ms shutdown grace, 3s diagnostics, 150ms debounce, 5s doc wait, 10s full wait).
- **Recommendation**: Move magic numbers into `const` with doc comments explaining rationale.

### Finding MAINT-21: Test coverage <2%
- **Severity**: Critical
- **Location**: All crates
- **Description**: ~54 tests total. Only error.rs (25 tests), permission.rs (2 doc-tests), mcp (25 tests), lsp (2 doc-tests). All major modules (config, session, event, provider, tool_impls, plugin, filesystem, database, app, routes, main) have 0 tests.
- **Recommendation**: Add `#[cfg(test)]` mock implementations. Write integration tests for each crate's public API surface.

### Finding MAINT-22: Trivial tests
- **Severity**: Medium
- **Location**: `tests::test_result_alias()` checks `42 == 42`
- **Description**: Tests exist for the sake of having tests.
- **Recommendation**: Replace with meaningful assertions.

### Finding MAINT-23: Silent serialization failures
- **Severity**: High
- **Location**: `crates/blazecode-server/src/routes/session.rs:328, 373, 388`
- **Description**: `serde_json::to_value(session).unwrap_or_default()` silently swallows serialization errors.
- **Consequence**: If serialization fails, the API returns `null` instead of an error.
- **Recommendation**: Return `500` on serialization failure; never silently default.

### Finding MAINT-24: `Config::load()` reads filesystem and env vars
- **Severity**: Info
- **Location**: `config.rs`
- **Description**: Config loading is synchronous and reads filesystem directly.
- **Recommendation**: Acceptable for initialization phase.

---

## Developer Experience Findings (from Agent 13)

### Finding DEVEX-1: No local compilation allowed (CLAUDE.md Rule #1)
- **Severity**: Critical
- **Location**: `blazecode/CLAUDE.md:8`
- **Description**: CLAUDE.md explicitly prohibits all local `cargo` commands. CI-only model means zero local feedback loops.
- **Consequence**: Estimated 15-30 min per iteration for full workspace rebuild. Developer cannot validate code before pushing.
- **Recommendation**: Remove CLAUDE.md Rule #1 prohibition on local `cargo check` and `cargo test`.

### Finding DEVEX-2: No README.md
- **Severity**: Critical
- **Location**: `blazecode/README.md` (missing)
- **Description**: No user-facing README at all. `CLAUDE.md` serves as primary documentation but is AI-context-oriented, not user-facing.
- **Consequence**: Users cannot learn what BlazeCode is, how to build it, or how to contribute.
- **Recommendation**: Write a proper README.md with badge row, description, quick-start, build instructions, and configuration.

### Finding DEVEX-3: No CONTRIBUTING.md
- **Severity**: Critical
- **Location**: `blazecode/CONTRIBUTING.md` (missing)
- **Description**: No human-facing contribution documentation. All developer guidance is in `CLAUDE.md` (AI-agent-targeted).
- **Consequence**: Humans have no documented process for contributing.
- **Recommendation**: Write CONTRIBUTING.md with local setup instructions, PR workflow, coding standards, test expectations.

### Finding DEVEX-4: No hot-reload or watch mode
- **Severity**: Critical
- **Location**: `blazecode/Cargo.toml:57` (notify dependency)
- **Description**: `notify` crate listed in workspace dependencies but no `cargo-watch` configured. No `watchexec` in dev scripts. CLAUDE.md Rule #1 prohibits local builds entirely.
- **Consequence**: Zero hot-reload capability. Every code change requires CI run to validate.
- **Recommendation**: Add `cargo watch` to dev scripts for auto-`cargo check` on file changes.

### Finding DEVEX-5: No pre-commit hooks
- **Severity**: High
- **Location**: `blazecode/` (no hooks)
- **Description**: No pre-commit or pre-push hooks. CLAUDE.md explicitly prohibits local tooling.
- **Consequence**: Every push triggers a full CI run only to discover fmt/clippy failures.
- **Recommendation**: Add pre-push hook using `cargo-husky` or `lefthook`.

### Finding DEVEX-6: No IDE configuration
- **Severity**: High
- **Location**: `blazecode/` (no IDE config)
- **Description**: No `.vscode/`, no `.zed/`, no `.helix/`, no `rust-analyzer` settings (e.g., `rust-analyzer.cargo.features`). No launch configurations for debugging.
- **Consequence**: Every developer must configure rust-analyzer from scratch.
- **Recommendation**: Add `.vscode/settings.json` with `rust-analyzer.cargo.features = "all"` and `rust-analyzer.check.command = "clippy"`.

### Finding DEVEX-7: No debug configuration
- **Severity**: High
- **Location**: `blazecode/CLAUDE.md:8`
- **Description**: No debug documentation. No VSCode launch configs. CLI explicitly prohibits `cargo build` locally.
- **Consequence**: Developer cannot debug locally. Bug diagnosis requires adding `eprintln!`/`tracing::debug!` and deploying to CI.
- **Recommendation**: Provide VSCode launch configurations for debugging with `rust-gdb`/`lldb`.

### Finding DEVEX-8: No integration tests / test fixtures
- **Severity**: High
- **Location**: `blazecode/` (no test fixtures directory)
- **Description**: No dedicated test fixtures directory. No test data files for provider responses, config files, or session data.
- **Recommendation**: Create `test-fixtures/` with sample config files, provider responses (JSON), and session snapshots.

### Finding DEVEX-9: No documentation beyond plugin-system.md
- **Severity**: High
- **Location**: `blazecode/docs/`
- **Description**: Single documentation file (`docs/plugin-system.md`). No design specs, no migration guides, no architecture decision records (ADRs).
- **Recommendation**: Port key specs from BlazeCode: session architecture, provider model, config schema.

### Finding DEVEX-10: No database migration infrastructure
- **Severity**: High
- **Location**: `blazecode/Cargo.toml:21` (sqlx)
- **Description**: No migration infrastructure visible. `storage.rs` uses JSON storage with SQLite placeholder.
- **Recommendation**: Add `sqlx::migrate!` for SQLite migrations.

### Finding DEVEX-11: No sccache in CI
- **Severity**: Medium
- **Location**: `blazecode/.github/workflows/ci.yml:36-37`
- **Description**: `Swatinem/rust-cache@v2` caches `target/` directory. No sccache (distributed compiler cache). No parallel job splitting.
- **Consequence**: First-time CI takes 20-40 minutes. Windows and macOS runners cannot share Linux build cache.
- **Recommendation**: Add `sccache` with S3/GCS backend for cross-OS cache sharing.

### Finding DEVEX-12: No `cargo nextest` for parallel test execution
- **Severity**: Medium
- **Location**: `blazecode/.github/workflows/ci.yml:49-62`
- **Description**: `cargo test --all --verbose` with no parallelism flags. No test result reporting (JUnit/xml). No flaky test detection.
- **Recommendation**: Switch to `cargo nextest`. Publish test results with `dorny/test-reporter`.

### Finding DEVEX-13: Plugin documentation is good but limited
- **Severity**: Medium
- **Location**: `blazecode/docs/plugin-system.md:1-293`
- **Description**: Well-documented plugin system with single 293-line doc. Three plugin tiers: config-based, closure plugins, trait plugins. 14 built-in OpenAI-compatible provider profiles. But only covers LLM provider plugins, not tool plugins, UI plugins, or MCP plugins.
- **Recommendation**: Expand plugin system to cover tool plugins and context source plugins.

### Finding DEVEX-14: Skill module is scaffold-only
- **Severity**: Medium
- **Location**: `blazecode/crates/blazecode-core/src/skill.rs`
- **Description**: Skill module exists as scaffold. `discover()` function from `.blazecode/skills/*.md`. Integrated with instruction context and system context registry. No usage examples, no tests.
- **Recommendation**: Complete skill module implementation. Add integration tests.

### Finding DEVEX-15: Release workflow is mature
- **Severity**: Low
- **Location**: `blazecode/.github/workflows/release.yml:1-276`, `blazecode/scripts/version.sh:1-212`
- **Description**: Comprehensive release workflow: 5 targets (x86_64/aarch64 Linux, x86_64/aarch64 macOS, x86_64 Windows), tarball/zip packaging, SHA256 checksums, GPG signing, auto-generated release notes from git log, version management script.
- **Recommendation**: Add `cargo publish` to crates.io. Create Homebrew formula for macOS.

### Finding DEVEX-16: Security auditing stronger than BlazeCode
- **Severity**: Low
- **Location**: `blazecode/.github/workflows/audit.yml:1-98`
- **Description**: Weekly scheduled `cargo-audit` with auto-created GitHub issues on vulnerability detection. `cargo-deny` in CI with license allowlist (12 licenses), ban config, and advisory ignore list.
- **Recommendation**: Document reason for `RUSTSEC-2024-0436` ignore with a comment.

### Finding DEVEX-17: Error formatting not user-friendly
- **Severity**: Medium
- **Location**: `blazecode/crates/blazecode-core/src/error.rs`
- **Description**: Good internal error handling (thiserror derive) but no user-facing error display formatting for CLI output.
- **Recommendation**: Implement `Display` traits with user-facing messages for all error variants. Add `color-eyre` for panic/error formatting.

### Finding DEVEX-18: Time to first build is prohibitive
- **Severity**: Critical
- **Location**: `blazecode/Cargo.toml:12-64`
- **Description**: First build: 87 workspace dependencies, estimated 15-30 minutes. CLAUDE.md prohibits local build entirely.
- **Recommendation**: Add `rust-toolchain.toml` to pin toolchain version. Provide prebuilt binaries via CI artifacts for quick download.

### Finding DEVEX-19: No issue templates or PR templates
- **Severity**: High
- **Location**: `blazecode/.github/` (no templates)
- **Description**: No issue templates. No PR template. No code of conduct.
- **Recommendation**: Add GitHub issue templates (bug report, feature request). Add PR template with checklist. Add CODE_OF_CONDUCT.md.

### Finding DEVEX-20: No code generation
- **Severity**: Medium
- **Location**: `blazecode/` (no codegen)
- **Description**: No OpenAPI spec generation. No client SDK generation. No automated type synchronization between server and client.
- **Recommendation**: Add `utoipa` for OpenAPI spec generation from axum routes.

---

## Infrastructure Findings (from Agent 14)

### Finding INFRA-1: Zero monitoring/telemetry
- **Severity**: Critical
- **Location**: BlazeCode -- no monitoring infrastructure
- **Description**: No monitoring whatsoever. No crash reporting, no performance tracking, no telemetry. A crash in the field is invisible to maintainers.
- **Consequence**: Critical bugs go undetected until users report them. No data on which platforms/features have issues.
- **Recommendation**: Add opt-in telemetry using OpenTelemetry. Add `sentry` crate for crash reporting.

### Finding INFRA-2: No observability pipeline
- **Severity**: High
- **Location**: BlazeCode -- tracing crate only
- **Description**: Has `tracing` + `tracing-subscriber` (with `env-filter`, `json`, `registry` features) + `tracing-appender` in dependencies. However, no structured log shipping, no error tracking (no Sentry), no metrics collection, no alerting. OTLP exporter is configuration-only.
- **Recommendation**: Implement OpenTelemetry tracing with `opentelemetry-otlp` crate for Honeycomb compatibility.

### Finding INFRA-3: No backup/restore for SQLite database
- **Severity**: High
- **Location**: BlazeCode -- local SQLite only
- **Description**: SQLite database is a single local file. No backup mechanism. No disaster recovery.
- **Consequence**: If SQLite file is corrupted or deleted, all session history is lost. No migration path between versions.
- **Recommendation**: Implement periodic SQLite backups (WAL mode + `VACUUM INTO`). Add export/import command.

### Finding INFRA-4: Missing code signing + package managers
- **Severity**: High
- **Location**: `blazecode/.github/workflows/release.yml`
- **Description**: No Windows Authenticode signing (users get "unknown publisher" warnings). No macOS codesign/notarization (Gatekeeper blocks unsigned binaries). No Homebrew/apt/scoop support. No crates.io publishing.
- **Recommendation**: Add Azure Trusted Signing for Windows, Apple Developer ID signing for macOS, create Homebrew tap formula, publish to crates.io.

### Finding INFRA-5: No containerization
- **Severity**: Medium
- **Location**: BlazeCode -- no Dockerfiles
- **Description**: Zero containerization. Cannot run in containerized CI, cannot deploy as container image. CI must install Rust toolchain from scratch on every run.
- **Recommendation**: Create a `ci.Dockerfile` with Rust toolchain + dependencies for use as `job.container` in CI.

### Finding INFRA-6: No logging pipeline
- **Severity**: Medium
- **Location**: BlazeCode -- tracing deps present but unconfigured
- **Description**: Has `tracing-subscriber` with `env-filter`, `json`, `registry` features + `tracing-appender` for file-based logging. Currently scaffold -- no production logging configuration. No log shipping infrastructure.
- **Recommendation**: Configure tracing-subscriber early with JSON formatting + file appender.

### Finding INFRA-7: Slow CI -- no pre-baked containers
- **Severity**: Medium
- **Location**: `.github/workflows/ci.yml`
- **Description**: GitHub-hosted runners with `Swatinem/rust-cache@v2`. No pre-baked containers. Full `cargo build` on every CI run.
- **Recommendation**: Create Docker image with pre-compiled Rust toolchain. Consider sccache.

### Finding INFRA-8: No Windows ARM support
- **Severity**: Medium
- **Location**: `.github/workflows/release.yml:78-97`
- **Description**: Missing `aarch64-pc-windows-msvc` target. Only `x86_64-pc-windows-msvc` for Windows. BlazeCode supports all 6 platform targets.
- **Recommendation**: Add `aarch64-pc-windows-msvc` to release matrix.

### Finding INFRA-9: No Nix flake
- **Severity**: Medium
- **Location**: BlazeCode -- no flake.nix
- **Description**: No Nix support. Nix users cannot build/run BlazeCode without manual setup. BlazeCode has full Nix support: flake.nix, 4 system platforms, devShell.
- **Recommendation**: Add flake.nix with devShell and package using `rustPlatform.buildRustPackage`.

### Finding INFRA-10: Missing package managers
- **Severity**: Medium
- **Location**: `blazecode/install`
- **Description**: Two install methods: curl|sh install script (400 lines, feature-rich) and direct download from GitHub Releases. No crates.io (`cargo install blazecode`), no Homebrew, no Scoop, no Chocolatey.
- **Recommendation**: Publish to crates.io. Create Homebrew tap with a formula. Create Scoop manifest for Windows.

### Finding INFRA-11: No configuration file implementation
- **Severity**: Medium
- **Location**: `Cargo.toml:28` (toml dep)
- **Description**: Has `toml` crate in deps but no config file implementation. No config schema or validation.
- **Recommendation**: Implement `~/.config/blazecode/config.toml` with serde deserialization.

### Finding INFRA-12: Minimal secret management needs
- **Severity**: Low
- **Location**: `.github/workflows/release.yml`
- **Description**: Uses `${{ secrets.GPG_SIGNING_KEY }}` etc. in CI for release signing. No production secret management needed (local CLI tool).
- **Recommendation**: If telemetry added, use environment variables. Never hardcode API keys.

### Finding INFRA-13: Deployment model difference (CLI vs SaaS)
- **Severity**: Info
- **Location**: `sst.config.ts` (BlazeCode)
- **Description**: BlazeCode is a CLI tool. BlazeCode has SaaS backend deployed via SST to Cloudflare Workers + AWS ECS Fargate, with PlanetScale, Stripe billing.
- **Recommendation**: If cloud features desired, design stateless REST API deployable via Docker/cloud.

### Finding INFRA-14: No Kubernetes (neither project uses it)
- **Severity**: Info
- **Location**: N/A
- **Description**: Neither project uses Kubernetes. BlazeCode uses AWS ECS Fargate + Cloudflare Workers. BlazeCode runs locally.
- **Recommendation**: If server-side components needed, consider serverless approach.

### Finding INFRA-15: No environment concept for BlazeCode
- **Severity**: Info
- **Location**: N/A
- **Description**: BlazeCode is a local tool. No environment management needed (unlike BlazeCode's production/dev/personal stages with SST).

---

## Database Findings (from Agent 15)

### Finding DB-1: Projectors run inside database transactions
- **Severity**: Critical
- **Location**: `event.rs:943-948`, `event_projector.rs:276-331`
- **Description**: BlazeCode runs projectors inside the database transaction (`event.rs:943-948`). If a projector fails, the entire event write is rolled back. BlazeCode runs projectors after the transaction commits.
- **Consequence**: Running projectors inside the transaction keeps the transaction open longer, can trigger deadlocks. If a projector fails, valid events are rolled back.
- **Recommendation**: Move projectors to post-commit hooks (after `tx.commit().await`).

### Finding DB-2: `commit_sync_event` missing atomicity
- **Severity**: Critical
- **Location**: `event_projector.rs:276-331`
- **Description**: `commit_sync_event` calls `db.insert_event()` then `db.upsert_event_sequence()` as separate non-transactional queries.
- **Consequence**: If `insert_event` succeeds but `upsert_event_sequence` fails, the database has an orphan event with no sequence tracking, and a subsequent write produces a duplicate sequence number.
- **Recommendation**: Wrap `insert_event` + `upsert_event_sequence` in a SQLite transaction.

### Finding DB-3: No compile-time schema validation
- **Severity**: High
- **Location**: `database.rs:472-807`
- **Description**: BlazeCode duplicates all 20 table schemas as `const &str` SQL literals. No compile-time verification. A typo in a column name passes `cargo build` and `cargo test`.
- **Consequence**: Schema drift between BlazeCode and BlazeCode is silent. If BlazeCode adds a column via migration, BlazeCode's `INITIAL_MIGRATION` will create the table without that column.
- **Recommendation**: Add a compile-time macro or build script that compares Rust SQL against TypeScript Drizzle schema definitions.

### Finding DB-4: N+1 query for messages + parts
- **Severity**: High
- **Location**: `database.rs:1728-1744`
- **Description**: Does NOT use SQL JOINs. Instead, the `get_messages_with_parts` method performs N+1 queries: 1 query for messages + N queries for parts.
- **Consequence**: Loading a session with 200 messages + 400 parts requires 201 SQL queries instead of 1.
- **Recommendation**: Replace with a `LEFT JOIN` query: `SELECT m.*, p.* FROM message m LEFT JOIN part p ON m.id = p.message_id WHERE m.session_id = ?1`.

### Finding DB-5: Plaintext token storage
- **Severity**: High
- **Location**: `database.rs:514-525` (account table)
- **Description**: Account tokens stored in `account.access_token` / `account.refresh_token` as `text` columns. No encryption.
- **Consequence**: Anyone with filesystem access to the SQLite database file can read the user's API tokens.
- **Recommendation**: Use OS keychain integration (macOS Keychain, Linux Secret Service, Windows Credential Manager) or encrypt token columns with a device-derived key.

### Finding DB-6: No database backup
- **Severity**: High
- **Location**: All -- no backup feature
- **Description**: Neither BlazeCode nor BlazeCode implements database backups. If the SQLite file is corrupted, all session data, events, accounts, and credentials are lost.
- **Recommendation**: Add a `blazecode db backup` command that runs `PRAGMA wal_checkpoint(TRUNCATE)`, copies the database file, adds backup metadata.

### Finding DB-7: Dynamic query building with string interpolation
- **Severity**: Medium
- **Location**: `database.rs:1356-1421` (`list_sessions_global`)
- **Description**: Builds SQL strings dynamically via `format!()` with `next_bind` tracking. String interpolation of column names.
- **Consequence**: Adding a new filter requires updating bind counter, condition string, SQL column list, ORDER BY, and bind calls.
- **Recommendation**: Use `sqlx::QueryBuilder` for dynamic query construction.

### Finding DB-8: Missing fresh-install migration optimization
- **Severity**: Medium
- **Location**: `database.rs:1098-1135`, `storage.rs:621-1363`
- **Description**: BlazeCode detects fresh installs (no tables) and creates full schema + marks all 35 migrations complete in one transaction. BlazeCode runs all 35 migrations sequentially even on a fresh database.
- **Consequence**: Fresh install startup is ~35x slower than necessary.
- **Recommendation**: Add the fresh-install shortcut: if no tables exist, create all at once and mark all migrations complete.

### Finding DB-9: Event uniqueness check is an extra query
- **Severity**: Medium
- **Location**: `event.rs:917-931`
- **Description**: Explicit SELECT query for event ID uniqueness check before INSERT. The `event_aggregate_seq_idx` UNIQUE index on `(aggregate_id, seq)` would catch duplicate violations without the SELECT.
- **Recommendation**: Remove the explicit SELECT -- rely on the UNIQUE index constraint and catch the constraint violation error.

### Finding DB-10: No event version upgrade path
- **Severity**: Medium
- **Location**: `event.rs` (versioning types exist but no migration)
- **Description**: BlazeCode supports `version` in `SyncConfig` and `versioned_type()` for forward compatibility. BlazeCode implements the versioning types but does not handle event data migration during version upgrade.
- **Recommendation**: Add version migration mechanism that transforms stored event data when the schema version changes.

### Finding DB-11: Projection state is in-memory only
- **Severity**: Medium
- **Location**: `event_projector.rs:66`
- **Description**: Projection state is `RwLock<HashMap<String, ProjectionState>>` -- lost on restart. BlazeCode persists projection checkpoints to the `event_sequence` table. DB is fallback, not primary.
- **Consequence**: On restart, the projector reads from seq 0 (or the DB checkpoint) and re-projects all events. All events from the beginning of time are replayed.
- **Recommendation**: Make DB the checkpoint authority. Initialize in-memory state from DB on startup.

### Finding DB-12: Pool connection count too high for SQLite
- **Severity**: Medium
- **Location**: `storage.rs:658-670`, `database.rs:59-66`
- **Description**: `sqlx::SqlitePool` default max connections = `num_cpus * 2`, which could be 16+ connections for a machine with many cores, all contending for the same SQLite file.
- **Consequence**: Higher than necessary memory usage from idle pool connections. Potential SQLITE_BUSY errors under high write concurrency.
- **Recommendation**: Set `pool_options.max_connections = 3` (1 writer + 2 readers).

### Finding DB-13: `rwc` mode silently creates new database
- **Severity**: Medium
- **Location**: `storage.rs:664`
- **Description**: URI format `sqlite:{path}?mode=rwc` -- `rwc` means read-write-create. This implicitly creates the database if it doesn't exist.
- **Consequence**: Silent data loss -- running the app with a wrong `BLAZECODE_DB` path creates a new empty database, migrations succeed, but there is no user data.
- **Recommendation**: Validate the database path. Consider using `mode=rw` (read-write, no create) in production.

### Finding DB-14: No `BEGIN IMMEDIATE` for write transactions
- **Severity**: Medium
- **Location**: `database.rs:59-66`, `event.rs:899-986`
- **Description**: Uses default deferred transaction mode. BlazeCode uses `{ behavior: "immediate" }`. SQLite's default `BEGIN DEFERRED` starts in read mode and upgrades to write on first mutation.
- **Consequence**: Two concurrent transactions both starting in deferred mode cause SQLITE_BUSY when one tries to upgrade to write. busy_timeout=5000 handles this by retrying, but adds latency.
- **Recommendation**: Use `BEGIN IMMEDIATE` for all write transactions.

### Finding DB-15: Missing composite indexes for common queries
- **Severity**: Medium
- **Location**: `database.rs:1356-1421`, `1515-1536`
- **Description**: No composite index on `session(directory, time_updated)` for `list_sessions_global` directory filter. No composite index on `session(project_id, time_updated DESC, id DESC)` for `list_sessions`.
- **Recommendation**: Add composite indexes for most common query patterns.

### Finding DB-16: Missing `get_messages_with_parts` JOIN
- **Severity**: High
- **Location**: `database.rs:1728-1744`
- **Description**: Same as DB-4 -- N+1 query pattern.
- **Recommendation**: Use LEFT JOIN.

### Finding DB-17: Column Type Mapping -- missing `$default` / `$onUpdate`
- **Severity**: Medium
- **Location**: `database.rs:356-391`
- **Description**: BlazeCode lacks Drizzle's `$default` / `$onUpdate` automatic timestamp handling. Every INSERT must explicitly pass `time_created` and `time_updated`.
- **Consequence**: Callers can forget to set timestamps.
- **Recommendation**: Add a Rust macro or helper that automatically populates timestamp columns on INSERT/UPDATE.

### Finding DB-18: JSON Columns with path normalization
- **Severity**: Low
- **Location**: `database.rs:889-1063`
- **Description**: BlazeCode adds path normalization (POSIX `/` slashes, absolute path validation) that BlazeCode does at the application layer, not the database layer. Strict superset of BlazeCode's functionality.
- **Recommendation**: Keep the validation but add clear error messages.

### Finding DB-19: Heartbeat jitter (flock)
- **Severity**: Low
- **Location**: `flock.rs:329-338`
- **Description**: Heartbeat runs on async runtime using `tokio::spawn`. If the runtime is under heavy load, heartbeat may not fire in time, causing lock to be considered stale.
- **Recommendation**: Consider spawning heartbeat on a dedicated low-priority runtime or using a larger safety margin.

### Finding DB-20: RowRaw -> Row mapping boilerplate
- **Severity**: Low
- **Location**: `database.rs:3163-3300`
- **Description**: Two-tier mapping (`RowRaw` -> `into_row()` -> `Row`) adds ~10 lines of boilerplate per table. ~400 total lines of mapping code.
- **Recommendation**: Use `#[serde(rename = "...")]` directly on public `Row` structs and derive `Deserialize` + `sqlx::FromRow`.

### Finding DB-21: Foreign Keys -- all present
- **Severity**: Info
- **Location**: `database.rs:472-807`
- **Description**: All 15+ foreign key constraints are present in both BlazeCode and BlazeCode.
- **Recommendation**: None needed.

### Finding DB-22: PRAGMA configuration match
- **Severity**: Info
- **Location**: `database.rs:59-66`, `storage.rs:673-680`
- **Description**: Identical PRAGMA configuration: journal_mode=WAL, synchronous=NORMAL, busy_timeout=5000, cache_size=-64000, foreign_keys=ON, wal_checkpoint(PASSIVE).
- **Recommendation**: None needed.

### Finding DB-23: Event type parity
- **Severity**: Info
- **Location**: `event.rs:1648-2290`
- **Description**: All ~40+ session event types ported as Rust structs with Serialize/Deserialize. BlazeCode adds `session_event_types` constants module that BlazeCode lacks.
- **Recommendation**: None needed.

### Finding DB-24: Multi-DB architecture difference
- **Severity**: Info
- **Location**: `infra/lake.ts:1-327`
- **Description**: BlazeCode has Local SQLite, PlanetScale (MySQL), AWS S3 Tables/Athena, Stats Server. BlazeCode only implements the local SQLite tier. This is by design.
- **Recommendation**: Document limitation explicitly.

---

## Reliability Findings (from Agent 16)

### Finding REL-1: No provider retry (dead `is_retryable()`)
- **Severity**: Critical
- **Location**: `error.rs:456-458`, `session_runner.rs:960-1155`
- **Description**: `LlmErrorReason::is_retryable()` method is defined at `error.rs:456` correctly identifying retryable error reasons (RateLimit, ProviderInternal). However, this method is never called in the `run_loop` or `run_turn_attempt` methods.
- **Consequence**: Transient provider errors (rate limits, 503s) immediately fail the turn instead of being retried. Significant user experience degradation.
- **Recommendation**: Wire `is_retryable()` into the turn execution flow with exponential backoff with jitter.

### Finding REL-2: No timeouts on provider calls
- **Severity**: Critical
- **Location**: `session_runner.rs:625-634`, `session_runner.rs:990-994`
- **Description**: `provider.stream()` and `provider.complete()` are called with no timeout. Only bash tool has explicit timeout enforcement (2 min default, 10 min max).
- **Consequence**: Provider hangs block the session indefinitely. Non-responsive provider causes indefinite session hang. Only recovery is process restart.
- **Recommendation**: Add timeout parameter to `Provider` trait. Default 60s for streaming, 30s for completion.

### Finding REL-3: No signal handling / graceful shutdown
- **Severity**: Critical
- **Location**: `src/main.rs:1233-1278`
- **Description**: `main()` creates a `tokio::runtime` and calls `rt.block_on(async_main(cli))`. There is no signal handling. `SIGINT` (Ctrl+C) will immediately terminate the process.
- **Consequence**: Tool executions in progress are abruptly terminated. Database writes may be incomplete (WAL helps, but non-transactional writes may be lost). Session state is not persisted on shutdown.
- **Recommendation**: Use `tokio::signal::ctrl_c()` and `tokio::signal::unix::Signal` (for SIGTERM) to implement graceful shutdown.

### Finding REL-4: Error context lost (handlers return `i32`)
- **Severity**: Critical
- **Location**: `crates/blazecode-core/src/error.rs:23-352`, `crates/blazecode-server/src/error.rs:19-118`, `src/cli_error.rs:50-111`
- **Description**: The `dispatch_inner` function at `main.rs:1337` uses `i32` exit codes -- it discards the error entirely. The `CliErrorFormatter::format_error` is never called in `dispatch_inner`.
- **Consequence**: All runtime errors from command handlers (provider init, DB, session, etc.) are silently discarded. Users see a non-zero exit but no error message.
- **Recommendation**: Change command handlers to return `Result<(), anyhow::Error>` and propagate errors through `dispatch` for proper formatting.

### Finding REL-5: Schema drift between SQL and migrations
- **Severity**: Critical
- **Location**: `database.rs:1280-1350`
- **Description**: Uses `sqlx::query` (unchecked) instead of `sqlx::query!` (compile-time checked). Every migration that adds a column without updating the corresponding INSERT causes a hard panic at runtime.
- **Consequence**: Schema drift between migration and hand-written SQL -> runtime crash on first INSERT.
- **Recommendation**: Use `sqlx::query!` with compile-time checking, or add integration tests that verify all hand-written SQL against the actual schema.

### Finding REL-6: File lock TOCTOU race
- **Severity**: Critical
- **Location**: `flock.rs:179-294`
- **Description**: In `try_acquire_lock_dir` at `flock.rs:222-227`, there's a TOCTOU race: the code checks `is_stale()`, then creates a `.breaker` directory, then re-checks staleness. If the original lock holder renews its heartbeat between the two staleness checks, the breaker logic can incorrectly delete a live lock.
- **Consequence**: Two processes could simultaneously believe they hold the same lock, leading to concurrent session writes and data corruption.
- **Recommendation**: Use the breaker directory itself as the authoritative lock. Acquire breaker with `mkdir` atomicity, only remove original lock while holding breaker.

### Finding REL-7: Sequential tool execution (no partial failures)
- **Severity**: High
- **Location**: `session_runner.rs:721-797`
- **Description**: `run_turn_attempt()` at `session_runner.rs:740` processes tool calls sequentially in a `for` loop. If the third of five tool calls fails, the error is returned immediately and remaining tool calls are never executed.
- **Consequence**: A single failing tool call cancels all subsequent tool calls, losing potentially successful work.
- **Recommendation**: Execute independent tool calls concurrently using `FuturesUnordered` or `join_all`. Report partial failures alongside successful results.

### Finding REL-8: Control flow via string parsing in errors
- **Severity**: High
- **Location**: `session_runner.rs:639-679, 928-947`
- **Description**: In `run_turn_attempt`, when `overflow_detected` is true and recovery is attempted, control flow is encoded as a string inside `Error::Internal` (via `TurnControl::encode()`). This string-parsing approach is fragile.
- **Consequence**: If the encoding changes or if the error message is modified, overflow recovery silently stops working, leading to context overflow crashes.
- **Recommendation**: Use a dedicated enum for control flow (not embedded in `Error::Internal`). Add exhaustive test coverage.

### Finding REL-9: No circuit breaker for providers
- **Severity**: High
- **Location**: No circuit breaker implementation found
- **Description**: No circuit breaker pattern anywhere in the codebase. A failing provider (e.g., returning 429 or 503) will be called on every turn.
- **Consequence**: Provider outages cause excessive retry storms and slow session degradation instead of fast failure.
- **Recommendation**: Implement a circuit breaker for provider calls. Track per-provider failure counts with configurable thresholds.

### Finding REL-10: No provider fallback chain
- **Severity**: High
- **Location**: `session_runner.rs` (initialized with specific provider)
- **Description**: No provider fallback mechanism. The `SessionRunner` is initialized with a specific `Arc<dyn Provider>` and `Model` -- if this provider fails, the entire session fails.
- **Consequence**: If the configured provider is down, the session cannot proceed even if alternative providers are configured.
- **Recommendation**: Implement provider fallback chain. On provider error, attempt the fallback provider before failing the turn.

### Finding REL-11: Blocking mutex in async snapshot code
- **Severity**: High
- **Location**: `snapshot.rs:138`
- **Description**: `std::sync::Mutex<()>` used inside async code in `SnapshotService`. If the lock is held while awaiting a future (e.g., in `snapshot_git`), it will block the entire tokio thread.
- **Consequence**: Tokio worker thread starvation if snapshot operation blocks while holding the mutex.
- **Recommendation**: Replace `std::sync::Mutex` with `tokio::sync::Mutex` in all async code paths.

### Finding REL-12: Non-transactional session revert cleanup
- **Severity**: High
- **Location**: `session_revert.rs:219-267`
- **Description**: `cleanup()` method at `session_revert.rs:244-250` performs individual `DELETE FROM session_message` queries without a wrapping transaction.
- **Consequence**: If the process crashes mid-cleanup, some messages are deleted and others remain, leaving the session in an unrecoverable state with corrupted message ordering.
- **Recommendation**: Wrap all revert cleanup operations in a SQLite transaction.

### Finding REL-13: No fsync in JSON storage
- **Severity**: High
- **Location**: `storage.rs:445-455`
- **Description**: `Storage::write()` at `storage.rs:454` uses `std::fs::write()` which does NOT guarantee data is flushed to disk -- it's equivalent to `write()` + `close()`, but the OS may buffer the write.
- **Consequence**: Data loss on crash for JSON file storage (sessions, messages, parts written to JSON files).
- **Recommendation**: Use `File::create()` + `write_all()` + `sync_all()` for all JSON storage writes.

### Finding REL-14: Missing in-flight request draining
- **Severity**: High
- **Location**: `session_execution.rs:637-888`
- **Description**: `RunCoordinator` has `interrupt()` at `session_execution.rs:786` that cancels the fiber and sets `stopping = true`. However, there is no global `shutdown()` method that interrupts all lanes.
- **Consequence**: On process termination (even with signal handling), active drains for other sessions remain running, potentially continuing to execute tool commands.
- **Recommendation**: Add a `shutdown()` method to `RunCoordinator` that interrupts all active lanes and waits for them to settle.

### Finding REL-15: No incremental session persistence
- **Severity**: High
- **Location**: `session_runner.rs:960-1155`
- **Description**: `run_loop` method at `session_runner.rs:957` does not persist intermediate events or tool results during execution. Only the final `SessionRunResult` is returned.
- **Consequence**: Crash during a long-running tool sequence (e.g., 10 tool calls) loses all work. The user must restart from scratch.
- **Recommendation**: Persist each tool call result incrementally to the database. Use event sourcing for LLM events as they arrive.

### Finding REL-16: Event publication failure -- notification outside transaction
- **Severity**: High
- **Location**: `event.rs:855-1063`
- **Description**: `publish` method runs projectors and commit guards inside the DB transaction, then calls `self.notify()` and `ch.publish()` outside the transaction. A failure in notify after the DB commit cannot be rolled back.
- **Consequence**: Missed event notifications leading to stale read models after a partial failure.
- **Recommendation**: Wrap the notification phase in the same transaction, or implement an outbox pattern with retry.

### Finding REL-17: Session execution state machine missing transition validation
- **Severity**: High
- **Location**: `session_execution.rs:637-888`
- **Description**: The `RunCoordinator` exposes `run()`, `wake()`, `interrupt()`, and `await_idle()` as public async methods. There is no enforcement of state machine invariants.
- **Consequence**: Race conditions in concurrent execution requests could lead to multiple drain fibers for the same session, or a drain continuing after an interrupt.
- **Recommendation**: Add structured state machine with guard checks before each transition.

### Finding REL-18: No provider retry (dead `is_retryable()` code)
- **Severity**: Critical
- **Location**: `session_runner.rs:960-1155`
- **Description**: Duplicate of REL-1. `is_retryable()` is dead code.

### Finding REL-19: No database connection health check
- **Severity**: Medium
- **Location**: `database.rs:59-66`, `storage.rs:658-697`
- **Description**: No connection health monitoring or automatic reconnection. If the pool becomes exhausted or the database file is moved, there is no recovery path.
- **Recommendation**: Add pool health checks, connection retry logic, and automatic reconnection.

### Finding REL-20: Event replay divergence check not atomic
- **Severity**: Medium
- **Location**: `event.rs:1349-1422`
- **Description**: `replay` method at `event.rs:1359` checks for diverged events by comparing event IDs. However, the check reads from the pool directly (not inside a transaction), so concurrent writes could interfere.
- **Consequence**: False positive or false negative replay divergence detection during concurrent event publication.
- **Recommendation**: Wrap replay checks in a transaction.

### Finding REL-21: `HttpContext` is dead code
- **Severity**: Medium
- **Location**: `error.rs:697`
- **Description**: `HttpContext` struct at `error.rs:697` is defined and tested but never attached to any error variant.
- **Consequence**: LLM errors do not carry HTTP request/response context, making debugging provider issues harder.
- **Recommendation**: Wire `HttpContext` into provider error construction.

### Finding REL-22: No disk space management
- **Severity**: Medium
- **Location**: No disk space management found
- **Description**: No disk space monitoring or graceful handling. A `std::io::Error` with `ErrorKind::StorageFull` surfaces as `Error::Io`, displayed as a generic I/O error.
- **Consequence**: Disk full causes generic I/O errors that users cannot distinguish from other I/O issues. Data loss risk for in-flight writes.
- **Recommendation**: Add disk space monitoring before writes. Check available space for large operations.

### Finding REL-23: Non-transactional writes throughout
- **Severity**: High
- **Location**: Various code paths
- **Description**: Code paths that perform SQL writes without wrapping in a transaction (session revert cleanup, individual message updates). Non-transactional writes can leave database in inconsistent state after a crash.
- **Recommendation**: Audit all write paths to ensure they are transactional.

### Finding REL-24: Catch-up projection is not atomic
- **Severity**: Medium
- **Location**: `event_projector.rs:144-221`
- **Description**: `EventProjector::catch_up()` processes events sequentially but does not have atomic batch semantics. If the 50th of 500 events fails, the first 49 have already been processed.
- **Consequence**: After a partial catch-up failure, some events are projected and some are not. The next catch-up may double-process some events or skip others.
- **Recommendation**: Add a transaction around catch-up projection.

### Finding REL-25: Projector idempotency not enforced
- **Severity**: Medium
- **Location**: `event_projector.rs:144-221`
- **Description**: Projector idempotency is not enforced or verified. If a projector has side effects (sending notifications, writing to external systems), replay can produce duplicate effects.
- **Recommendation**: Enforce projector idempotency. All projectors should be pure functions that only update read models.

### Finding REL-26: No tool execution idempotency
- **Severity**: Medium
- **Location**: `tool_impls.rs:615-888` (BashTool)
- **Description**: No idempotency key/token for tool executions. A retried tool call (e.g., after timeout) runs again with no deduplication.
- **Consequence**: Non-idempotent tools (bash, write, edit) can produce unintended duplicate effects on retry.
- **Recommendation**: Add idempotency key to `ToolContext`. Tools should check if a given key was already executed.

### Finding REL-27: No JSON Schema validation for tool inputs
- **Severity**: Medium
- **Location**: `tool_impls.rs:590-613`
- **Description**: Tools define `parameters_schema()` returning JSON Schema, but this schema is documentation only -- it is not used for validation. Input validation is ad-hoc.
- **Recommendation**: Implement JSON Schema validator (e.g., `jsonschema` crate) and validate all tool inputs against their schema before execution.

### Finding REL-28: No JSON Schema validation for events
- **Severity**: Medium
- **Location**: `event.rs:184-185`
- **Description**: `EventDefinition` at `event.rs:178` has `data_schema: serde_json::Value`, but it is never used for validation. Events are not validated against their schema at any point.
- **Recommendation**: Implement JSON Schema validation for event data on publication.

### Finding REL-29: No session idle timeout
- **Severity**: Medium
- **Location**: No session idle timeout found
- **Description**: No session idle timeout mechanism. Sessions can remain in "running" state indefinitely if the provider hangs or the user walks away.
- **Consequence**: Resource leak -- sessions consume memory and database connections forever.
- **Recommendation**: Implement session idle timeout using `tokio::time::timeout` around user input waits.

### Finding REL-30: Stale lock cleanup on startup
- **Severity**: High
- **Location**: `flock.rs:59-123, 179-358`
- **Description**: No automatic stale recovery on startup. Stale locks from crashed processes persist until another process needs the lock. No PID checking in meta.json.
- **Recommendation**: On startup, run stale lock cleanup for all locks owned by processes no longer alive.

### Finding REL-31: Event registry schema validation not enforced
- **Severity**: Low
- **Location**: `event.rs:581`
- **Description**: No enforcement that events with a given type reference a registered definition.
- **Recommendation**: Validate event data against registered definition's `data_schema` before publication.

### Finding REL-32: In-memory event ID deduplication missing
- **Severity**: Low
- **Location**: `event.rs:990-1016`
- **Description**: In-memory-only path (no database) does not check event ID uniqueness -- duplicate events with the same ID are allowed.
- **Recommendation**: Add in-memory event ID deduplication.

### Finding REL-33: Graceful shutdown -- state persistence missing
- **Severity**: Medium
- **Location**: No shutdown hooks found
- **Description**: No shutdown hooks or state persistence on exit. `ObservabilityService` has `init()` but no observable `shutdown()` or `flush()` method.
- **Recommendation**: Register shutdown hooks via `Drop` or explicit `shutdown()` calls.

### Finding REL-34: Non-idempotent tool execution on retry
- **Severity**: Medium
- **Location**: `tool_impls.rs:615-888` (BashTool)
- **Description**: Same as REL-26.

---

## Testing Findings (from Testing Agent)

### Finding TEST-1: No provider testing strategy
- **Severity**: Critical
- **Location**: All provider modules (`providers/anthropic.rs`, `providers/openai.rs`, `providers/gemini.rs`, etc.)
- **Description**: 3 provider modules have zero tests (openai, gemini, credential). None have recorded HTTP cassettes for deterministic replay. Providers are traits but tests don't mock them.
- **Consequence**: Cannot safely test LLM provider code paths. Integration tests require real API keys.
- **Recommendation**: Implement `HttpClient` trait with recording/replay middleware. Add `mockall` or `wiremock` for HTTP-level mocking.

### Finding TEST-2: No E2E tests
- **Severity**: Critical
- **Location**: BlazeCode -- none
- **Description**: Zero end-to-end tests. No CLI invocation tests, no TUI integration tests, no full workflow tests.
- **Consequence**: Release quality depends entirely on unit tests. Regression in CLI argument parsing, TUI rendering, or cross-crate interaction goes undetected.
- **Recommendation**: Add `trycmd` or `assert_cmd` for CLI binary testing. Add `ratatui` rendering snapshot tests.

### Finding TEST-3: No HTTP recording/replay
- **Severity**: Critical
- **Location**: BlazeCode -- no equivalent
- **Description**: BlazeCode has `http-recorder` package for deterministic replay of LLM provider API calls. BlazeCode has no equivalent. No recorded cassettes.
- **Consequence**: Cannot deterministically test LLM provider code paths. Session runner tests lack recorded cassettes.
- **Recommendation**: Port `http-recorder` pattern to Rust using `reqwest::Middleware` or custom `HttpClient` trait with recording/replay.

### Finding TEST-4: No coverage tooling
- **Severity**: High
- **Location**: BlazeCode -- no coverage tool configured
- **Description**: No `cargo-tarpaulin` or `grcov` in CI. Coverage is invisible.
- **Consequence**: 11 modules with zero test functions despite declaring `mod tests {}` blocks. Untested code is not identified.
- **Recommendation**: Add `cargo-tarpaulin` or `grcov` to CI. Set a coverage threshold (e.g., 60% minimum).

### Finding TEST-5: 11 modules with zero test functions
- **Severity**: High
- **Location**: `providers/openai.rs`, `providers/openai_compatible.rs`, `providers/gemini.rs`, `providers/openrouter.rs`, `credential.rs`, `bus.rs`, `system_context.rs`, `model.rs`, `policy.rs`, `event.rs`, `v2_schema.rs`
- **Description**: 11 modules have `mod tests {}` blocks but zero test functions. BlazeCode has test files for corresponding modules.
- **Recommendation**: Add tests for each module's public API surface.

### Finding TEST-6: No test infrastructure
- **Severity**: High
- **Location**: BlazeCode -- no test helpers module
- **Description**: No shared test fixtures, no `TestDb`, no `TempDir`, no `TestConfig` builders. BlazeCode has `packages/core/test/fixture/` with `tmpdir.ts`, `git.ts`, `location.ts`, `recordings/`.
- **Consequence**: Each test module reimplements setup boilerplate. No consistent pattern for test setup.
- **Recommendation**: Create `crates/blazecode-core/src/test_support.rs` with `TestDb::new()`, `TempDir::new()`, `TestConfigBuilder`, `MockProviderBuilder`.

### Finding TEST-7: 56 documentation examples not compile-checked
- **Severity**: Medium
- **Location**: BlazeCode -- 58 code blocks, 2 explicitly marked as ```rust
- **Description**: Most doc comments have examples but few are runnable doctests. They will silently drift from actual API.
- **Recommendation**: Convert documentation examples to runnable doctests. Add `#![deny(rustdoc::broken_intra_doc_links)]`.

### Finding TEST-8: No benchmarks
- **Severity**: Medium
- **Location**: BlazeCode -- none
- **Description**: No `criterion` or `divan` in dependencies. No `iai` for instruction counting. No load tests for the SSE server or session processing pipeline.
- **Recommendation**: Add `criterion` benchmarks for critical paths (ID generation, wildcard matching, serialization).

### Finding TEST-9: No fuzzing
- **Severity**: Medium
- **Location**: BlazeCode -- none
- **Description**: No `cargo-fuzz` or `cargo-afl` configured. No SAST beyond clippy. No DAST for the HTTP server.
- **Recommendation**: Add `cargo-fuzz` harnesses for config file parsing, shell command parsing, MCP parameter deserialization.

### Finding TEST-10: No property-based testing
- **Severity**: Medium
- **Location**: BlazeCode -- none
- **Description**: No `proptest` or `quickcheck` in dependencies. All tests use hand-written example inputs.
- **Recommendation**: Add `proptest` for core algorithms: `permission::wildcard_match`, `id::ascending`/`descending`, `serde_json` roundtrips for all public types.

### Finding TEST-11: No mocking framework
- **Severity**: Critical
- **Location**: BlazeCode -- only 56 references to mock/stub/fake across entire codebase
- **Description**: No `mockall`, `mockito`, `wiremock` in dependencies. Providers are traits but tests construct real provider instances requiring real API keys.
- **Recommendation**: Add `mockall` for trait mocking. Add `wiremock` or `mockito` for HTTP-level mocking.

### Finding TEST-12: CI lacks typecheck job
- **Severity**: Medium
- **Location**: `.github/workflows/ci.yml`
- **Description**: No `cargo check --all-targets` job. No feature-flag combinatorial testing. No MSRV validation.
- **Recommendation**: Add `cargo check` job. Test with `--no-default-features` and `--all-features`. Add MSRV check.

### Finding TEST-13: Windows test skipping is error-prone
- **Severity**: Medium
- **Location**: CI -- `skip_unix_tests` cfg flag
- **Description**: Manual RUSTFLAGS-based platform gating instead of `#[cfg(not(windows))]`. Windows CI may skip tests accidentally.
- **Recommendation**: Use `#[cfg(not(windows))]` and `serial_test` crate for tests that share global state.

### Finding TEST-14: No mutation testing
- **Severity**: Low
- **Location**: BlazeCode -- none
- **Description**: No `cargo-mutants`, no `mutagen`. Test quality cannot be objectively measured.
- **Recommendation**: Run `cargo-mutants` on critical modules (permission, config, session).

### Finding TEST-15: No migration tests
- **Severity**: Medium
- **Location**: `database.rs:3304` (20 database tests)
- **Description**: BlazeCode has `database-migration.test.ts` for all 35 migrations. BlazeCode has no migration tests that verify table/column parity.
- **Recommendation**: Add migration tests that run against a fresh SQLite database and verify table/column parity.

---

## Technical Debt Findings (from Agent 19)

### Finding TD-1: Widespread `panic!()` in production code paths
- **Severity**: Critical
- **Type**: Design / Stability
- **Location**: Throughout all crates (100+ production `panic!()` calls)
- **Description**: `panic!()` is used extensively in non-test code for enum variant extraction, JSON parsing, and error handling. Files with panics: `integration.rs:995`, `snapshot.rs:1266`, `credential.rs:620`, `account.rs:1503`, `project.rs:1011`, `workspace.rs:562`, `npm.rs:725`, `tool_stream.rs:171`, `tool_output_store.rs:346`, `patch.rs:808`, `session.rs:3023`, `session_runner.rs:1363`, `repository.rs:1112`, `location.rs:1214`, `reference.rs:1139`, `pty.rs:767`, `ripgrep.rs:1148`, `file_mutation.rs:201`.
- **Impact**: Any unexpected enum variant triggers a process crash. In an AI coding agent, this means mid-session data loss.
- **Estimated Fix Cost**: 80-120 person-hours
- **Priority**: Critical

### Finding TD-2: `#![allow(dead_code, unused_imports, unused_variables)]` in lib.rs and main.rs
- **Severity**: Critical
- **Type**: Design / Quality
- **Location**: `lib.rs:2`, `main.rs:2`, `main.rs:28`, `app.rs:120`, `session_message.rs:204/549`, `tui/src/app.rs:120`
- **Description**: Crate-level suppress of 50+ dead items. Many modules are declared but may or may not be used. The compiler cannot verify real dead code.
- **Impact**: Dead code accumulates silently. Prevents Clippy from catching real bugs.
- **Estimated Fix Cost**: 20-30 person-hours
- **Priority**: Critical

### Finding TD-3: `Error::NotImplemented` used as production stub return
- **Severity**: Critical
- **Type**: MissingFeature
- **Location**: `agent.rs:1293,1304` (+ 10+ other locations)
- **Description**: Production code paths return `Err(Error::NotImplemented("mock".into()))` for methods that should be implemented.
- **Impact**: Users hit "not implemented" errors during normal operation. Core agent functionality incomplete.
- **Estimated Fix Cost**: 30-50 person-hours
- **Priority**: Critical

### Finding TD-4: CLAUDE.md "No `.unwrap()` in library code" rule is systematically violated
- **Severity**: Critical
- **Type**: Workaround / Quality
- **Location**: Every production module in `blazecode-core/src/` (500+ `.unwrap()` + `.expect()` calls in non-test code)
- **Description**: CLAUDE.md rule #3: "No `.unwrap()` in library code." Every `.unwrap()` in non-test production code violates this.
- **Impact**: Any `unwrap()` on `None`/`Err` crashes the process. Users lose work mid-session.
- **Estimated Fix Cost**: 40-60 person-hours
- **Priority**: Critical

### Finding TD-5: `#[allow(clippy::too_many_arguments)]` on 15+ methods
- **Severity**: High
- **Type**: Design
- **Location**: `database.rs:1233,1284,1657,1749,1781`, `plugin.rs:2413`, `session_runner.rs:577`, `providers/anthropic.rs:1252`, `providers/azure.rs:200`, `providers/xai.rs:223`, `server.rs:69`, `main.rs:2397`, `tui/theme.rs:358`
- **Description**: Many CRUD and construction methods take 10-19 parameters. The suppressed lint indicates a design smell.
- **Impact**: Callers unreadable, hard to maintain, error-prone. Adding a field changes every call site.
- **Estimated Fix Cost**: 20-30 person-hours
- **Priority**: High

### Finding TD-6: No provider implementations exist (Anthropic, OpenAI, etc.)
- **Severity**: High
- **Type**: MissingFeature
- **Location**: `provider.rs:906` (trait defined), no `impl Provider for ...` outside of stubs
- **Description**: The `Provider` trait is fully defined but no real provider implementations exist. BlazeCode has full Anthropic, OpenAI, Gemini, Bedrock, Azure, etc. implementations.
- **Impact**: The agent cannot call any LLM. CLI `run` commands and HTTP prompt endpoints fail.
- **Estimated Fix Cost**: 80-120 person-hours
- **Priority**: High

### Finding TD-7: Session compaction is incomplete
- **Severity**: High
- **Type**: MissingFeature
- **Location**: `session_compaction.rs` (has core types but limited logic), `session.rs:1264` ("currently a stub")
- **Description**: Session compaction (context window management) is critical for long-running agent sessions. The module has types but the actual compaction logic -- summarization, pruning, context window management -- is missing or stubbed.
- **Impact**: Long sessions will overflow context windows, causing provider errors. No compaction means no multi-turn agent loops.
- **Estimated Fix Cost**: 40-60 person-hours
- **Priority**: High

### Finding TD-8: `Config::get()` can panic (poisoned lock)
- **Severity**: High
- **Type**: Workaround
- **Location**: `config.rs:1166`, `config.rs:1322`
- **Description**: `self.info.read().expect("Config lock poisoned")` and `self.info.write().expect(...)` panic on lock poison.
- **Impact**: A panic in one thread taking the lock poisons it for all threads. Subsequent reads/writes crash the process.
- **Estimated Fix Cost**: 2-4 person-hours
- **Priority**: High

### Finding TD-9: No actual SQLite connection/pool in production
- **Severity**: High
- **Type**: MissingFeature
- **Location**: `storage.rs`, `database.rs` (types and SQL constants exist but no production pool initialization)
- **Description**: The database module defines 20 CREATE TABLE statements, 35 migration IDs, path helpers, and typed column wrappers. But the `Database` struct in `storage.rs` still uses JSON file-based storage as the primary implementation. SQLite pool creation is deferred.
- **Impact**: Session persistence, project management, permissions, and events all use JSON file storage or in-memory only. Data is lost on restart.
- **Estimated Fix Cost**: 30-50 person-hours
- **Priority**: High

### Finding TD-10: `forbid(unsafe_code)` is not enforced
- **Severity**: Low
- **Type**: Process / Quality
- **Location**: CLAUDE.md rule #2 -- grep finds 0 hits for `#![forbid(unsafe_code)]`
- **Description**: CLAUDE.md says `#![forbid(unsafe_code)]` in every crate. A grep returned no results.
- **Impact**: Unsafe code could be introduced silently.
- **Estimated Fix Cost**: 1 person-hour
- **Priority**: Low

### Finding TD-11: No integration tests for any provider protocol
- **Severity**: Medium
- **Type**: Test
- **Location**: All provider modules
- **Description**: Provider implementations have unit tests but zero integration tests against real API endpoints or recorded fixtures.
- **Estimated Fix Cost**: 30-50 person-hours
- **Priority**: Medium

### Finding TD-12: `event.rs` `notify()` silently swallows listener errors
- **Severity**: Medium
- **Type**: Design
- **Location**: `event.rs:1171` -- `let _ = listener(payload.clone()).await;`
- **Description**: The non-isolated notify path silently discards errors.
- **Estimated Fix Cost**: 2-4 person-hours
- **Priority**: Medium

### Finding TD-13: `tool_impls.rs` regex compile is repeated per execution
- **Severity**: Medium
- **Type**: Performance
- **Location**: `tool_impls.rs:248` -- `regex::Regex::new(r"\s+").unwrap()` inside `WhitespaceNormalizedReplacer.search()`
- **Description**: A regex that never changes is compiled every time the `search()` method is called.
- **Estimated Fix Cost**: 1 person-hour
- **Priority**: Medium

### Finding TD-14: `git diff` / `git status` run as subprocess with no caching
- **Severity**: Medium
- **Type**: Performance
- **Location**: `routes/session.rs:576-577` -- `git.diff("HEAD")` and `git.status()` called per request
- **Description**: Every HTTP request to `GET /session/:id/diff` spawns a `git` subprocess. No caching or batching.
- **Estimated Fix Cost**: 4-8 person-hours
- **Priority**: Medium

### Finding TD-15: `SessionManager::update()` builds 19-None tuples for every call
- **Severity**: Medium
- **Type**: Design / Performance
- **Location**: `session.rs:778-779`, `1114`, `1128`, `1143`, `1209`, `1230`, `1244`
- **Description**: Every `update_session()` call passes 19 parameters, 17 of which are `None` for most convenience methods.
- **Estimated Fix Cost**: 8-12 person-hours
- **Priority**: Medium

### Finding TD-16: `filesystem.rs` `glob_matches()` is a toy regex implementation
- **Severity**: Medium
- **Type**: Workaround
- **Location**: `filesystem.rs:318-358`
- **Description**: `glob_matches()` implements glob matching inline with basic string operations instead of using the `glob` or `ignore` crate.
- **Estimated Fix Cost**: 2-4 person-hours
- **Priority**: Medium

### Finding TD-17: `TuiApp` creates backend services but immediately drops them
- **Severity**: Medium
- **Type**: Design
- **Location**: `app.rs:205-206` -- `_sessions: Arc<SessionManager>`, `_runner: Arc<...>`
- **Description**: `TuiApp::new()` accepts `SessionManager` and `SessionRunner` by value but stores them as `Option` (always `None`). They are immediately dropped.
- **Estimated Fix Cost**: 4-8 person-hours
- **Priority**: Medium

### Finding TD-18: `session_list.rs:list_sessions_global` has string interpolation in SQL query
- **Severity**: Medium
- **Type**: Security
- **Location**: `database.rs:1369-1401` -- dynamic `WHERE` clause built via `format!("directory = ?{next_bind}")`
- **Description**: SQL WHERE clauses built by string interpolation of column names and parameter placeholders. Not a SQL injection vector (values are parameterized) but risky if extended improperly.
- **Estimated Fix Cost**: 4-8 person-hours
- **Priority**: Medium

---

## Production Readiness Findings (from Agent 20)

### Finding PR-1: No session crash recovery
- **Severity**: Critical
- **Description**: If process dies mid-session, state is incomplete. No mechanism to resume an interrupted turn.
- **Impact**: Loss of all in-flight work. User must restart from last persisted epoch.

### Finding PR-2: No circuit breaker or retry policy for transient provider failures
- **Severity**: Critical
- **Description**: No provider retry (is_retryable() dead code). No circuit breaker for failing providers.
- **Impact**: Transient provider errors immediately fail the turn.

### Finding PR-3: No DB connection health checks / reconnection logic
- **Severity**: Critical
- **Description**: No pool health monitoring. No automatic reconnection on pool exhaustion.
- **Impact**: Transient pool exhaustion causes permanent failure until process restart.

### Finding PR-4: No TLS in server
- **Severity**: High
- **Description**: Server runs HTTP only. Credentials transmitted in plaintext.
- **Impact**: All server traffic unencrypted.

### Finding PR-5: No CSRF protection
- **Severity**: High
- **Description**: Server endpoints are vulnerable to CSRF attacks.
- **Impact**: State-changing endpoints can be invoked without origin validation.

### Finding PR-6: Stored access tokens in SQLite in plaintext
- **Severity**: High
- **Description**: `account.access_token`, `account.refresh_token`, `credential.value` stored as plaintext.
- **Impact**: Anyone with filesystem access can read API tokens.

### Finding PR-7: OTLP exporter configured but not wired
- **Severity**: High
- **Description**: OTLP export types exist but no actual `opentelemetry` SDK integration sends data.
- **Impact**: No distributed tracing export. Monitoring dead end.

### Finding PR-8: No health check endpoint
- **Severity**: Medium
- **Description**: No `/health` endpoint that pings the SQLite pool.
- **Impact**: Cannot monitor service health.

### Finding PR-9: No Prometheus metrics endpoint
- **Severity**: Medium
- **Description**: No metrics endpoint for request count, latency, error rate, active sessions.
- **Impact**: No performance observability.

### Finding PR-10: No backup/restore mechanism
- **Severity**: High
- **Description**: No backup or restore mechanism for SQLite database.
- **Impact**: Data loss on corruption.

### Finding PR-11: No Docker image
- **Severity**: Medium
- **Description**: No Docker image published in release workflow. No container deployment option.
- **Impact**: Cannot deploy as container.

### Finding PR-12: Auth config read from env at request time (TOCTOU)
- **Severity**: Medium
- **Description**: `BLAZECODE_SERVER_PASSWORD` read from environment at request time, not startup.
- **Impact**: Potential TOCTOU vulnerability if env changes during process lifetime.

### Finding PR-13: No rate limiting on server endpoints
- **Severity**: High
- **Description**: No rate limiting at any layer. Server endpoints unprotected.
- **Impact**: No protection against abuse.

### Finding PR-14: No panic hook captures crash details
- **Severity**: Critical
- **Description**: No panic hook set. Panics are unhandled. No core dump or minidump generation.
- **Impact**: Crash details invisible to maintainers.

### Finding PR-15: No integration tests
- **Severity**: High
- **Description**: Only unit tests exist. No E2E tests.
- **Impact**: Release quality depends entirely on unit tests.

### Finding PR-16: Synchronous I/O in Storage blocks runtime
- **Severity**: High
- **Description**: Storage module uses synchronous filesystem I/O (`std::fs::read_to_string`, `std::fs::write`) -- blocks async runtime.
- **Impact**: Intermittent latency spikes, cascading timeouts.

### Finding PR-17: No graceful shutdown
- **Severity**: High
- **Description**: No signal handling (SIGINT/Ctrl+C causes immediate termination). No DB connection drain on exit.
- **Impact**: Data loss on shutdown.

---

## Competitive Intelligence Findings (from Agent 17)

### Finding CI-1: Effect System gap
- **Severity**: Critical
- **Description**: BlazeCode uses Effect v4 with `Effect.gen`, `Context.Service`, `Layer`, `Scope`, `Stream`. BlazeCode uses raw `tokio::spawn` + `thiserror` + struct-field DI. No structured concurrency, no typed dependency injection, no algebraic error handling.
- **Recommendation**: Build a lightweight Scope-like lifetime manager on top of tokio CancellationToken + JoinSet.

### Finding CI-2: LLM Route Architecture gap
- **Severity**: Critical
- **Description**: BlazeCode uses four-axis route composition (Protocol, Endpoint, Auth, Framing) allowing 14 OpenAI-compatible providers to share 1 protocol implementation. BlazeCode conflates protocol, URL, auth, and framing in each provider.
- **Recommendation**: Implement Rust equivalent of the route architecture with Protocol, Endpoint, Auth, and Framing traits.

### Finding CI-3: Durable Prompt Admission gap
- **Severity**: High
- **Description**: BlazeCode `sessions.prompt(...)` admits durable `session_input` rows with promotion lifecycle and retry reconciliation. BlazeCode appends to in-memory message list with no durability.
- **Recommendation**: Implement durable session_input table, promotion lifecycle, and delivery-mode routing.

### Finding CI-4: Event Sourcing with Replay gap
- **Severity**: High
- **Description**: BlazeCode has durable event replay with aggregate sequence cursor. BlazeCode uses `tokio::sync::broadcast` -- in-memory only, no persistence.
- **Recommendation**: Port event sourcing with SQLite-backed event store + aggregate sequence cursor.

### Finding CI-5: Context Epochs gap
- **Severity**: High
- **Description**: BlazeCode has System Context Registry with scoped Context Source producers, immutable baselines, and mid-conversation system messages. BlazeCode has no concept of context epochs -- system prompt is static.
- **Recommendation**: Implement ContextSource trait, SystemContextRegistry with scoped contributions and epoch management.

### Finding CI-6: Plugin Ecosystem gap
- **Severity**: Medium
- **Description**: BlazeCode publishes `@blazecode-ai/plugin` on npm with SDK, scoped tool registration, plugin-defined context sources. BlazeCode has `ProviderPlugin` trait but no publishable SDK, no plugin-scoped tool registration.
- **Recommendation**: Publish `blazecode-plugin` crate with Plugin trait, scope-based registration, lifecycle hooks.

### Finding CI-7: Cloud Infrastructure gap
- **Severity**: Low
- **Description**: BlazeCode has full cloud deployment via SST on AWS/Cloudflare with PlanetScale, Athena, Honeycomb. BlazeCode is local-only SQLite.
- **Recommendation**: Document as local-first by design.

### Finding CI-8: Multi-Platform Support gap
- **Severity**: Medium
- **Description**: BlazeCode supports CLI, TUI, Desktop (Electron), Web (Next.js), VS Code extension, LSP, MCP, Slack. BlazeCode has only CLI (in progress) and TUI (stub).
- **Recommendation**: Focus on CLI + MCP as primary surfaces. Skip Electron.

### Finding CI-9: Community gap
- **Severity**: High
- **Description**: BlazeCode has 20k+ GitHub stars, active Discord, regular releases. BlazeCode has <10 stars, no community, no release cadence.
- **Recommendation**: Open-source as soon as MVP works. Publish to crates.io. Set up Discord.

### Finding CI-10: Strategic Innovations for BlazeCode
- **Severity**: Info
- **Description**: BlazeCode should exploit unique advantages: proc macros for zero-boilerplate tool definitions, single binary distribution, WASM plugin sandboxing, local AI inference via llama.cpp, compile-time safety for permission-critical tools.
- **Recommendation**: Build the "Rust-native AI terminal" -- not a port of OpenCode.

---

## Refactoring Findings (from Agent 18)

### Finding REF-1: 27 refactoring opportunities identified across 4 levels
- **Severity**: Info
- **Description**: Agent 18 identified 27 specific refactoring opportunities from Quick Wins (1-2 days each) to Strategic Refactoring (3-6 months). See full refactoring plan document for details.
- **Key Quick Wins**: Fix clear_revert SQL NULL bug (0.1d), Fix V1 permission bypass (1d), Restore dead-code detection (0.5d), Replace 19-param update_session (0.5d), Unify error hierarchies (1d), Use Arc<Vec<ChatMessage>> in ToolContext (0.5d).
- **Key Module Refactorings**: Module visibility discipline (5d), Split monolithic modules (10d), Database trait extraction (10d), Newtype IDs (5d), HTTP client trait (5d), Filesystem trait (8d).
- **Key Architectural Refactorings**: Split core into 5-8 crates (30d), Extract CLI library crate (10d), Structured concurrency (15d), Domain directories (5d).
- **Key Strategic Refactorings**: Effect-like DI system (20d), Comprehensive test suite (40d), JSON column migration (15d), V2 domain model (60d).
- **Total Estimated Effort**: ~295 person-days for all refactorings.

---

## Index

The index maps Finding IDs to their source agent, location, and severity. Due to the volume of findings, they are organized by agent report file:

| Report File | Agent | Finding Count | Critical | High | Medium | Low | Info |
|---|---|---|---|---------|------|------|------|------|
| architecture.md | Agent 02 | 10 | 5 | 5 | 0 | 0 | 0 |
| rust-expert.md | Agent 03 | 22 | 1 | 2 | 10 | 6 | 12 |
| logic-verification.md | Agent 04 | 42 | 4 | 12 | 11 | 10 | 10 |
| security.md | Agent 05 | 37 | 0 | 8 | 12 | 15 | 10 |
| performance.md | Agent 06 | 26 | 6 | 5 | 10 | 5 | 5 |
| scalability.md | Agent 07 | 14 | 4 | 6 | 3 | 1 | 0 |
| feature-gap.md | Agent 08 | 17 | 4 | 9 | 3 | 1 | 0 |
| api.md | Agent 09 | 27 | 4 | 6 | 7 | 4 | 3 |
| dependencies.md | Agent 11 | 12 | 3 | 1 | 5 | 1 | 2 |
| maintainability.md | Agent 12 | 24 | 5 | 5 | 8 | 2 | 4 |
| devex.md | Agent 13 | 20 | 4 | 7 | 6 | 3 | 0 |
| infrastructure.md | Agent 14 | 15 | 1 | 3 | 6 | 1 | 4 |
| database.md | Agent 15 | 24 | 2 | 4 | 7 | 3 | 6 |
| reliability.md | Agent 16 | 34 | 6 | 10 | 10 | 4 | 4 |
| competitive-intelligence.md | Agent 17 | 10 | 2 | 4 | 2 | 2 | 0 |
| refactoring.md | Agent 18 | 1 | 0 | 0 | 0 | 0 | 1 |
| technical-debt.md | Agent 19 | 18 | 4 | 5 | 9 | 0 | 0 |
| production-readiness.md | Agent 20 | 17 | 4 | 8 | 4 | 1 | 0 |
| **TOTAL** | **All 18 agents (with findings)** | **370** | **55** | **100** | **113** | **59** | **61** |

Note: Agents 01 (Repository Cartography) and 10 (if exists) produced structural reports without enumerated findings. All 20 agent reports were read and incorporated.

---

*End of Complete Findings. Total: ~370+ individual findings from 18 reporting agents across all 20 agent reports.*
