# Refactoring Plan — Agent 18

**Date:** 2026-06-21
**Inputs:** Architecture (Agent 02), Rust Expert (Agent 03), Logic Verification (Agent 04), Performance (Agent 06), Maintainability (Agent 12)
**Scope:** blazecode workspace — 6 crates, 95 modules, ~46k LOC

---

## Executive Summary

BlazeCode is a scaffold-phase Rust project with an Architecture Score of 25/100 and ~120–180 person-hours of technical debt. The codebase exhibits: a monolithic core crate with 95 flat public modules, a 3,000+ line `main.rs` mixing CLI dispatch and business logic, 5 fragmented error hierarchies, 14 files over 1,000 lines, 100+ `unwrap()` calls in library code, <2% test coverage, and direct infrastructure coupling (sqlx, reqwest, std::fs) in domain code.

The plan below organizes 27 refactoring opportunities across 4 levels, from quick wins to strategic transformations.

---

## Level 1: Quick Wins (1–2 days each)

Small, safe, high-impact refactorings.

---

### QW-1: Restore Compiler Dead-Code Detection

**Current State:** `#![allow(dead_code, unused_imports, unused_variables)]` in both `blazecode-core/src/lib.rs:2` and `src/main.rs:2`. 15–25 dead items silently accumulate.

**Target State:** Crate-wide `allow` removed. Individual `#[expect(dead_code)]` on items kept for TS-source symmetry. `#[cfg(scaffold)]` gate for scaffold-phase items.

**Implementation Plan:**
1. Remove the `allow` attributes from both `lib.rs` and `main.rs`
2. Run `cargo check` to identify dead items
3. For each dead item: either remove it, add `#[expect(dead_code)]`, or gate with `#[cfg(scaffold)]`
4. Add CI step that fails on dead code

**Risks:** Low — compiler tells you exactly what's dead
**Benefits:** Prevents accumulation of dead code; improves compile times; makes real unused-code bugs visible
**Dependencies:** None
**Estimated Effort:** 0.5 person-days
**Priority:** Critical

**Code Example:**
```rust
// Before
#![allow(dead_code, unused_imports, unused_variables)]

// After
// No crate-wide allow — use scoped #[expect(dead_code)] where needed
#[cfg(scaffold)]
#[expect(dead_code)]
pub fn stub_function() {}
```

---

### QW-2: Fix `clear_revert` Writing Literal `"null"` String Instead of SQL NULL

**Current State:** `session.rs:1208` passes `Some("null")` which writes the 4-character text `"null"` into the SQLite column instead of SQL `NULL`.

**Target State:** Pass `None` to write SQL `NULL`.

**Implementation Plan:** Change `Some("null")` to `None` in the one call site.

**Risks:** Minimal — the bug is in a single parameter value. The current behavior corrupts the column
**Benefits:** Fixes data corruption — `WHERE revert IS NULL` queries now work correctly
**Dependencies:** None
**Estimated Effort:** 0.1 person-days
**Priority:** Critical

**Code Example:**
```rust
// Before: line 1209
Some("null"), None, None, None, None)

// After:
None, None, None, None, None)
```

---

### QW-3: Replace 19-Positional-Arg `update_session` with Typed `SessionUpdate` Struct

**Current State:** `database.rs:1284-1350` — 19 positional `Option` parameters. Every call site passes 14–17 `None` values. Ordering bugs invisible to compiler.

**Target State:** `SessionUpdate` struct with `#[derive(Default)]` and named fields. `update_session(update: &SessionUpdate)`.

**Implementation Plan:**
1. Define `struct SessionUpdate` with all 19 fields as `Option`
2. Change `DatabaseService::update_session` to accept `&SessionUpdate`
3. Update all call sites to use `SessionUpdate { field: Some(val), ..Default::default() }`
4. Remove the old 19-param method

**Risks:** Medium — must update all 10+ call sites correctly
**Benefits:** Eliminates argument-ordering bugs; adding a new column requires editing only the struct and the query; compiler errors if fields missed
**Dependencies:** None
**Estimated Effort:** 0.5 person-days
**Priority:** Critical

**Code Example:**
```rust
// Before:
self.db.update_session(id, now, None, None, None, None, None, None, None, None, None, None, None, None, Some("null"), None, None, None, None)?;

// After:
self.db.update_session(&SessionUpdate {
    title: Some(&new_title),
    ..Default::default()
})?;
```

---

### QW-4: Fix V1 `run_loop` Permission Bypass

**Current State:** `session_runner.rs:1086-1096` — V1 `run_loop` sets `ask_fn: None` and `permission_source: None`, then calls `execute_by_name` which performs zero permission checks. All V1 tool executions bypass the permission system.

**Target State:** V1 uses `execute_with_pipeline` (which has permission checks) or provides a valid `ask_fn`.

**Implementation Plan:**
1. Wire `ask_fn` from the caller into the V1 `run_loop` path
2. Switch V1 from `execute_by_name` to `execute_with_pipeline`
3. Ensure `permission_source` is populated

**Risks:** Medium — changing V1 execution path may affect behavior
**Benefits:** Critical security fix — LLM can no longer call tools without permission enforcement
**Dependencies:** QW-3 (to understand session update flow)
**Estimated Effort:** 1 person-day
**Priority:** Critical

---

### QW-5: Fix `unwrap()` on `compact_result` and `Some()` Wrapping in JSON

**Current State:** `session_runner.rs:703-717` — `compact_result.as_ref().unwrap()` (violates project rules) and `serde_json::json!({ "summary": compact_result.as_ref().map(|r| r.summary.clone()) })` produces `{"summary": Some("...")}` with literal `Some(...)` wrappers in JSON.

**Target State:** Use `if let Some(ref result) = compact_result { ... }` with direct field access.

**Implementation Plan:**
1. Replace `if compact_result.is_some() { ... .unwrap() }` with `if let Some(ref result) = compact_result`
2. Remove the `.map()` calls inside `serde_json::json!()` — access `result.summary` directly
3. Fix the `prepare_epoch` double-call (lines 710 and 820-827)

**Risks:** Low — contained to one function
**Benefits:** Fixes JSON corruption (epoch snapshot storage contains `Some(...)` wrappers); eliminates redundant call
**Dependencies:** None
**Estimated Effort:** 0.5 person-days
**Priority:** Critical

**Code Example:**
```rust
// Before:
if compact_result.is_some() {
    let snapshot_val = serde_json::json!({
        "summary": compact_result.as_ref().map(|r| r.summary.clone()),
        "recent": compact_result.as_ref().map(|r| r.recent.clone()),
    });
    self.epoch_manager.prepare_epoch(session_id, &compact_result.as_ref().unwrap().summary, &snapshot_val)
}

// After:
if let Some(ref result) = compact_result {
    let snapshot_val = serde_json::json!({
        "summary": result.summary,
        "recent": result.recent,
    });
    self.epoch_manager.prepare_epoch(session_id, &result.summary, &snapshot_val)
        .await.map_err(|e| Error::Session(format!("epoch prepare: {e}")))?;
    return Err(Error::Internal(TurnControl::ContinueAfterOverflowCompaction.encode()));
}
```

---

### QW-6: Use `Arc<Vec<ChatMessage>>` in `ToolContext` Instead of `Vec<ChatMessage>`

**Current State:** `tool.rs:47` — `pub messages: Vec<ChatMessage>`. Every tool call deep-clones entire message history. For 50 messages at ~2KB each, that's 100KB per tool call. With 25 tool calls: 2.5MB of cloned message data.

**Target State:** `pub messages: Arc<Vec<ChatMessage>>`. Shared reference, zero-copy.

**Implementation Plan:**
1. Change `ToolContext.messages` type to `Arc<Vec<ChatMessage>>`
2. Update all construction sites to wrap in `Arc::new(messages)`
3. Update all consumers that read messages to deref as `&[ChatMessage]`

**Risks:** Low — `Arc` is immutable shared ownership. Any code that mutates messages through `ToolContext` will need adjustment
**Benefits:** Eliminates the single largest clone cost in the hot path. ~2.5MB saved per 25-tool-call session
**Dependencies:** None
**Estimated Effort:** 0.5 person-days
**Priority:** High

---

### QW-7: Scoped `#[allow]` Instead of Crate-Wide

**Current State:** Crate-wide `#![allow(dead_code, unused_imports, unused_variables)]`

**Target State:** Scoped `#[allow(..)]` on individual items or functions only.

**Implementation Plan:** After QW-1, replace remaining necessary allowances with item-level attributes.

**Risks:** Low
**Benefits:** Real dead code is detectable again
**Dependencies:** QW-1
**Estimated Effort:** 0.25 person-days
**Priority:** Medium

---

### QW-8: Fix `session_row_to_info` f64 Cost Precision

**Current State:** `session.rs:1420` — `cost: f64` prevents deriving `Eq` on `SessionInfo`. Monetary values accumulate floating-point rounding errors.

**Target State:** Store cost as `i64` (millicents) or use `ordered_float::OrderedFloat<f64>`.

**Implementation Plan:**
1. Change `SessionInfo.cost` to `ordered_float::OrderedFloat<f64>` (or `i64` millicents)
2. Update all arithmetic/comparison sites
3. Derive `PartialEq, Eq` on `SessionInfo`

**Risks:** Low — cost is a display-only field
**Benefits:** Enables `Eq` on `SessionInfo`; no precision loss
**Dependencies:** None
**Estimated Effort:** 0.5 person-days
**Priority:** Critical

---

### QW-9: Add `From<SessionError>` for `crate::error::Error`

**Current State:** Five fragmented error types. `crate::error::Error` has no `#[from] SessionError` — callers must `map_err` manually, often resorting to `SessionError::Other(err.to_string())` which loses type info.

**Target State:** `crate::error::Error` has `#[from] SessionError`, `#[from] DatabaseServiceError`, `#[from] LspError`. Unified error handling.

**Implementation Plan:**
1. Add `SessionError` variant to `Error` enum: `Error::SessionError(#[from] SessionError)`
2. Add similar variants for `DatabaseServiceError` and `LspError`
3. Remove string-based `Error::Session(String)` variant (or keep for backwards compat)
4. Update call sites to remove manual `map_err` chains

**Risks:** Low — additive change; old call sites still compile
**Benefits:** Error conversion becomes `?` instead of `.map_err(...)`. Downstream consumers can match on specific session errors
**Dependencies:** QW-3
**Estimated Effort:** 1 person-day
**Priority:** Critical

**Code Example:**
```rust
// Before:
let session = self.db.get_session(id)
    .map_err(|e| Error::Session(e.to_string()))?;

// After:
let session = self.db.get_session(id).await?;
```

---

### QW-10: Extract `ok_or_500()` Error Helper for Server Routes

**Current State:** 25 route handlers each repeat `match result { Ok(v) => Json(serde_json::to_value(v).unwrap_or_default()).into_response(), Err(e) => (StatusCode::INTERNAL_SERVER_ERROR, Json(json!({"error": e.to_string()}))).into_response() }` — ~200 lines identical.

**Target State:** Single `fn ok_or_500<T: Serialize>(result: Result<T>) -> impl IntoResponse` helper.

**Implementation Plan:**
1. Define `fn ok_or_500` in a helper module
2. Replace all route handler matches with `ok_or_500(result?)`
3. Remove `unwrap_or_default()` on `serde_json::to_value` (return 500 instead of silent `null`)

**Risks:** Low — mechanical replacement
**Benefits:** Eliminates 200 lines of boilerplate; fixes silent serialization failures
**Dependencies:** None
**Estimated Effort:** 0.5 person-days
**Priority:** High

---

## Level 2: Module Refactoring (1–2 weeks each)

Medium-sized structural changes.

---

### MR-1: Enforce Module Visibility Discipline — `pub(crate)` + Clean `lib.rs` API Surface

**Current State:** All 95 modules in `blazecode-core/src/lib.rs` are `pub mod` — every module is world-accessible. No `pub(crate)`. No re-export filtering. Internal implementation details are part of the public API.

**Target State:** Internal modules use `pub(crate) mod`. `lib.rs` re-exports only the intended public API surface. Consumers depend on the re-export layer, not on internal modules.

**Implementation Plan:**
1. Audit all 95 modules — classify as "public API" or "internal implementation"
2. Change internal modules to `pub(crate) mod`
3. Define explicit `pub use` re-exports in `lib.rs` for the public surface
4. Run `cargo check` on all downstream crates to identify breakage
5. Fix downstream crates to use re-exported types

**Risks:** Medium — downstream crates (`blazecode-server`, `blazecode-tui`, `blazecode-main`) import directly from internal modules. Will break compilation temporarily
**Benefits:** True API boundary. Refactoring internal modules no longer requires checking all consumers. Compiler prevents accidental public API expansion. Architecture score improves from 25 to ~40
**Dependencies:** QW-1 (remove dead code first to reduce audit surface)
**Estimated Effort:** 5 person-days
**Priority:** Critical

**Code Example:**
```rust
// Before (lib.rs):
pub mod session;
pub mod session_execution;
pub mod session_runner;
pub mod database;  // All 95 modules are public

// After (lib.rs):
// Internal implementation — not part of public API
pub(crate) mod database;
pub(crate) mod session_execution;
pub(crate) mod flock;
// ... 70+ internal modules

// Public API — explicit re-exports
pub mod config;
pub mod error;
pub mod provider;
pub mod tool;
pub mod bus;
pub mod session;  // Only session module (not session_execution, session_runner, etc.)

pub use config::Config;
pub use error::{Error, Result};
pub use provider::{Provider, Model, ChatMessage};
pub use tool::{Tool, ToolRegistry};
```

---

### MR-2: Split Monolithic Modules into Directory-Based Sub-Modules

**Current State:** 14 files over 1,000 lines:
- `session.rs` (1,481 lines) — SessionManager + Message types + Part types + 14 setter methods
- `event.rs` (1,422 lines) — EventV2 system, pub/sub, replay
- `provider.rs` (1,511 lines) — Types, LLM events, normalizers, transforms
- `tool_impls.rs` (1,238+ lines) — 21 tool implementations, 9 replacers
- `config.rs` (1,408+ lines) — 38-field struct + V2 config + 20+ sub-configs
- `database.rs` (4,758 lines) — Schema, queries, migrations, services
- `plugin.rs` (1,511+ lines) — V1/V2 plugin hooks, auth plugins
- `filesystem.rs` (1,557+ lines) — Watcher, read/write, search, ignore
- `permission.rs` (1,382+ lines) — Rules, evaluation, permission service

**Target State:** Directory-based modules:
```
session/
  mod.rs         — re-exports
  manager.rs     — SessionManager
  message.rs     — Message types
  part.rs        — Part enum + PartBuilder
  prompt.rs      — prompt building
  compaction.rs  — context overflow handling
  revert.rs      — revert/clear_revert
  setter.rs      — SessionPatch + setter methods
```

**Implementation Plan:**
1. For each >1k-file, create directory with `mod.rs`
2. Split into focused sub-modules (aim for <300 lines each)
3. Use `pub(crate)` within the directory to hide internal functions
4. Update all import paths across the codebase

**Risks:** Medium — import path changes across the entire codebase
**Benefits:** Each sub-module is a focused unit; easier to navigate, test, and review; parallel development possible; merge conflicts isolated
**Dependencies:** MR-1 (visibility discipline makes this safer)
**Estimated Effort:** 10 person-days
**Priority:** Critical

---

### MR-3: Extract Database Behind a Trait — Port/Adapter for Persistence

**Current State:** `database.rs` (4,758 lines) contains SQL constants, migration logic, `DatabaseService` with inline SQL, and direct `sqlx` calls. Core business logic imports `sqlx` types directly. No database trait — swapping from SQLite to PostgreSQL requires editing core code.

**Target State:** `Database` trait in core with async methods. `SqliteDatabase` adapter in `blazecode-database-sqlite` crate. Core code depends only on the trait.

**Implementation Plan:**
1. Define `#[async_trait] pub trait Database` in `blazecode-core/src/database/trait.rs` with core methods: `get_session`, `insert_session`, `update_session`, `get_messages`, etc.
2. Move `DatabaseService` implementation to new `blazecode-database-sqlite` crate
3. Change all core code to accept `Arc<dyn Database>` instead of `Arc<DatabaseService>`
4. Create in-memory mock `MockDatabase` for tests
5. Update composition root (`main.rs` and `runtime.rs`) to construct the SQLite adapter

**Risks:** High — breaking change to every service that takes `Arc<DatabaseService>`. All session, event, and storage code must switch to trait object
**Benefits:** Decouples core from SQLite; enables unit testing with in-memory DB; enables PostgreSQL support; follows hexagonal architecture
**Dependencies:** MR-1, MR-2
**Estimated Effort:** 10 person-days
**Priority:** Critical

**Code Example:**
```rust
// In blazecode-core:
#[async_trait]
pub trait Database: Send + Sync {
    async fn get_session(&self, id: &str) -> Result<SessionInfo>;
    async fn insert_session(&self, session: &SessionInfo) -> Result<()>;
    async fn update_session(&self, id: &str, patch: &SessionPatch) -> Result<()>;
    async fn get_messages(&self, session_id: &str) -> Result<Vec<Message>>;
    // ...
}

// In blazecode-database-sqlite:
pub struct SqliteDatabase {
    pool: sqlx::SqlitePool,
}

#[async_trait]
impl Database for SqliteDatabase {
    async fn get_session(&self, id: &str) -> Result<SessionInfo> {
        sqlx::query_as("SELECT * FROM session WHERE id = ?")
            .bind(id)
            .fetch_one(&self.pool)
            .await?;
        // ...
    }
}

// Usage in core:
pub struct SessionManager {
    db: Arc<dyn Database>,
    // No sqlx types imported
}
```

---

### MR-4: Extract HTTP Client Behind a Trait

**Current State:** `reqwest` is imported directly in core code. Provider implementations call `reqwest::Client::post(...)` directly. HTTP timeouts are not configured.

**Target State:** `HttpClient` trait in core with `get`, `post`, `stream` methods. `ReqwestHttpClient` adapter. Timeouts configured at construction.

**Implementation Plan:**
1. Define `HttpClient` trait in `blazecode-core`
2. Implement `ReqwestHttpClient` adapter
3. Update `Provider::stream()` signature to accept `&dyn HttpClient` or have providers hold an `Arc<dyn HttpClient>`
4. Set default 120s timeout on all HTTP requests
5. Add retry policy with exponential backoff + jitter

**Risks:** Medium — provider implementations must be updated
**Benefits:** Decouples from `reqwest`; enables testing with mock HTTP; consistent timeout/retry across all providers
**Dependencies:** MR-3 (similar pattern)
**Estimated Effort:** 5 person-days
**Priority:** High

---

### MR-5: Extract `blazecode-plugin-sdk` as Standalone Publishable Crate

**Current State:** Plugin system is embedded in `blazecode-core/src/plugin.rs` (1,511+ lines). Plugin consumers cannot depend on a lightweight SDK — they must depend on the entire core crate.

**Target State:** `blazecode-plugin-sdk` crate with just the `ProviderPlugin` trait, `PluginManager` trait, and minimal deps. Published separately on crates.io.

**Implementation Plan:**
1. Create `crates/blazecode-plugin-sdk/`
2. Extract minimal types: `ProviderPlugin`, `PluginManager`, `PluginContext`
3. Make core depend on plugin-sdk (invert dependency direction)
4. Add `build.rs` for SDK versioning
5. Write crate docs and publish script

**Risks:** Low — additive; old code still works during transition
**Benefits:** Third-party plugin authors depend on a lightweight SDK, not the entire core. Follows BlazeCode's `@blazecode-ai/plugin` pattern
**Dependencies:** MR-1 (visibility discipline clarifies the public API)
**Estimated Effort:** 3 person-days
**Priority:** Medium

---

### MR-6: Extract Filesystem Behind a Trait

**Current State:** Direct `std::fs` calls throughout core code — all blocking the async runtime. `filesystem.rs:1281` calls `std::fs::read_to_string` which blocks the tokio worker thread. No trait abstraction for filesystem operations.

**Target State:** `FileSystem` trait in core with async methods. `LocalFileSystem` adapter using `tokio::fs`. Tool implementations depend on the trait.

**Implementation Plan:**
1. Define `FileSystem` trait: `read(&self, path)`, `write(&self, path, content)`, `exists(&self, path)`, `list(&self, dir)`, `search(&self, pattern)`, etc.
2. Implement `TokioFileSystem` using `tokio::fs`
3. Have tool implementations take `Arc<dyn FileSystem>` in their context
4. For grep: delegate to ripgrep binary instead of pure-Rust regex on full file reads
5. Add `spawn_blocking` wrappers for any remaining `std::fs` calls

**Risks:** Medium — tool implementations extensively use `std::fs` directly
**Benefits:** Async I/O that doesn't block tokio workers; testable with in-memory mock filesystem; grep performance improves 10-100x by delegating to ripgrep
**Dependencies:** MR-3, MR-4 (pattern consistency)
**Estimated Effort:** 8 person-days
**Priority:** High

---

### MR-7: Replace Type Aliases with Proper Newtypes for IDs

**Current State:** `pub type SessionId = String;` — type aliases provide zero type safety. `fn foo(id: SessionId)` accepts any `String`. IDs are interchangeable with no compiler enforcement.

**Target State:** Newtype structs: `struct SessionId(String)`, `struct MessageId(String)`, `struct ModelId(String)`, `struct ProviderId(String)`. Each with `new()`, `as_str()`, `Display`, `FromStr`, `Serialize`/`Deserialize`.

**Implementation Plan:**
1. Define newtype structs in `blazecode-core/src/id.rs` (or new `types.rs`)
2. Implement conversion traits, validation (e.g., `SessionId` validates `ses_` prefix)
3. Update all function signatures across the codebase
4. Update `serde` derives to use `#[serde(transparent)]`

**Risks:** High — every function that takes/returns a `String` ID must be updated. Hundreds of call sites
**Benefits:** Compiler prevents ID confusion (passing `MessageId` where `SessionId` is expected). Self-documenting APIs. Validation at construction
**Dependencies:** MR-2 (module splits make this more manageable)
**Estimated Effort:** 5 person-days
**Priority:** High

**Code Example:**
```rust
// Before:
pub type SessionId = String;
pub type MessageId = String;

fn get_message(session_id: &SessionId, msg_id: &MessageId) -> Message;
// Compiler allows: get_message(&msg_id, &session_id) — swapped!

// After:
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(transparent)]
pub struct SessionId(String);

impl SessionId {
    pub fn new(id: impl Into<String>) -> Result<Self> {
        let s = id.into();
        // validate "ses_" prefix
        Ok(Self(s))
    }
    pub fn as_str(&self) -> &str { &self.0 }
}

fn get_message(session_id: &SessionId, msg_id: &MessageId) -> Message;
// Compiler catches: get_message(&msg_id, &session_id) — type mismatch!
```

---

## Level 3: Architectural Refactoring (1–2 months each)

Large-scale structural changes.

---

### AR-1: Split `blazecode-core` into 5–8 Granular Crates

**Current State:** Monolithic `blazecode-core` with 95 modules and all dependencies (sqlx, reqwest, axum, etc.) in a single crate. Build times degrade as core grows. No reuse path.

**Target State:** Workspace with 10–15 crates following BlazeCode's 26-package boundaries:

| New Crate | Responsibility | Dependencies |
|---|---|---|
| `blazecode-core-types` | Shared types, traits, newtypes | serde, thiserror |
| `blazecode-provider` | Provider trait, LLM events, adapters | core-types, http-client trait |
| `blazecode-session` | Session management, event sourcing | core-types, database trait |
| `blazecode-tool` | Tool trait, tool implementations | core-types, filesystem trait |
| `blazecode-config` | Config loading, parsing, validation | core-types |
| `blazecode-permission` | Permission evaluation, rules | core-types |
| `blazecode-event-store` | EventV2 pub/sub, replay | core-types, database trait |
| `blazecode-database-sqlite` | SQLite adapter (implements Database trait) | core-types, sqlx |
| `blazecode-http-reqwest` | HTTP client adapter | core-types, reqwest |
| `blazecode-filesystem` | Filesystem adapter | core-types, tokio |
| `blazecode-cli` | CLI argument parsing + dispatch (library) | clap, all other crates |
| `blazecode-llm` | LLM protocol adapters (Anthropic, OpenAI, Gemini, etc.) | core-types, provider trait |

**Implementation Plan:**
1. Create `blazecode-core-types` with shared traits and newtypes — zero implementation
2. Extract one crate at a time, starting with the leaf dependencies (types → config → permission → provider → tool → session)
3. Move each module group, update imports, add `pub use` re-exports
4. Update Cargo.toml workspace dependencies
5. Update `main.rs` to import from new crate locations
6. Verify compilation after each crate extraction

**Risks:** High — extensive import changes across the codebase. Compilation may break during transition
**Benefits:** Build times improve (changing `session.rs` no longer recompiles `provider.rs`). True bounded contexts. Enables publishing crates independently. Third-party contributions target specific crates. Architecture score improves from 25 to ~60
**Dependencies:** MR-1, MR-2, MR-3, MR-4, MR-5, MR-7 (all Level 2 refactorings set the stage)
**Estimated Effort:** 30 person-days
**Priority:** Critical

---

### AR-2: Extract Business Logic from `main.rs` into `blazecode-cli` Library Crate

**Current State:** `src/main.rs` is 3,000+ lines containing CLI argument parsing, `cmd_run` (1,500 lines with interactive REPL, SSE attach, provider resolution, permission handling, file resolution), `cmd_tui`, and business logic. The binary is a thick dispatch, not a thin CLI.

**Target State:** `blazecode-cli` library crate with all command handlers as public async functions. `main.rs` becomes ~30 lines: parse CLI args, call `blazecode_cli::run(cli).await`.

**Implementation Plan:**
1. Create `crates/blazecode-cli/`
2. Move all `cmd_*` functions into the new crate as public API
3. Move helper functions (`parse_model_spec`, `has_binary`, `print_header`, etc.)
4. Move `CliErrorFormatter` and formatting utilities
5. Reduce `main.rs` to: `fn main() { let cli = Cli::parse(); tokio::runtime::new().block_on(blazecode_cli::main(cli)); }`
6. Split `cmd_run` (1,500 lines) into focused sub-functions

**Risks:** Medium — import paths change; `cmd_run` is tightly coupled to core types
**Benefits:** Thin binary enables testing CLI logic without running the binary. Alternative front-ends (TUI, server) can reuse CLI dispatch logic. Clear separation of concerns
**Dependencies:** AR-1 (crate splitting)
**Estimated Effort:** 10 person-days
**Priority:** High

**Code Example:**
```rust
// Before (src/main.rs — 3,000+ lines):
#[tokio::main]
async fn main() {
    let cli = Cli::parse();
    // ... 3,000 lines of business logic
}

// After (src/main.rs — ~30 lines):
fn main() {
    let cli = Cli::parse();
    let exit_code = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .expect("runtime")
        .block_on(blazecode_cli::dispatch(cli));
    std::process::exit(exit_code);
}

// After (crates/blazecode-cli/src/lib.rs — all command handlers):
pub async fn dispatch(cli: Cli) -> i32 {
    match cli.command {
        Commands::Run(args) => cmd_run(args).await,
        Commands::Tui(args) => cmd_tui(args).await,
        // ...
    }
}
```

---

### AR-3: Implement Effect-Like Structured Concurrency with Scoped Fiber Management

**Current State:** Manual `CancellationToken` + `JoinHandle` management. `FiberSet::spawn` stores handles but never joins them — tasks leak on drop. `tokio::select!` used extensively. No scoped cancellation: if a session is dropped, its fibers continue running.

**Target State:** `ScopedFiberSet` that automatically cancels and awaits all fibers on drop. Structured concurrency: fiber lifetime is bounded by the scope that created it.

**Implementation Plan:**
1. Create `ScopedFiberSet<T>` that cancels all fibers on drop
2. Replace `DashMap<FiberId, JoinHandle<()>>` with `ScopedFiberSet`
3. Add `cancel_and_join()` method that awaits the join handle
4. Wire `CancellationToken` propagation through the scope chain
5. Replace manual `select!` loops with `FiberSet::race()` or `FiberSet::all()`

**Risks:** Medium — changes async control flow throughout `session_execution.rs`. Must carefully handle the `wake/run/interrupt` lifecycle
**Benefits:** No fiber leaks. Deterministic shutdown. Clear fiber lifecycle (parent scope → child scope). More readable async orchestration
**Dependencies:** AR-1, MR-2
**Estimated Effort:** 15 person-days
**Priority:** High

**Code Example:**
```rust
// Before:
pub fn spawn<F>(&self, future: F) -> FiberHandle {
    let handle = tokio::spawn(async move { ... });
    self.handles.insert(id, handle);  // handle never awaited
    FiberHandle { id, cancel }
}

// After:
impl Drop for ScopedFiberSet {
    fn drop(&mut self) {
        // Cancel all fibers and await their JoinHandles
        for cancel in self.cancels.values() {
            cancel.cancel();
        }
        // Wait for all handles (in a Drop impl, this must be best-effort)
    }
}

pub async fn spawn_and_join<F>(&self, future: F) -> Result<T> {
    let handle = tokio::spawn(future);
    self.handles.insert(id, handle);
    // On scope exit, all handles are cancelled + joined
}
```

---

### AR-4: Group Flat Modules into Domain Directories (session/, provider/, tool/)

**Current State:** 14 `session_*` modules and 8 `provider_*` modules are all flat files in `blazecode-core/src/`. No sub-module hierarchy. Developers scan 95 modules to find what exists.

**Target State:** Directory-based domain modules:
```
src/
  session/
    mod.rs        — re-exports SessionManager, Session
    manager.rs    — SessionManager (former session.rs core)
    message.rs    — Message, MessageInfo
    part.rs       — Part enum, PartBuilder
    epoch.rs      — Context epoch, baseline
    compaction.rs — Context overflow, compact
    execution.rs  — Coordinator, FiberSet, RunCoordinator
    history.rs    — History tracking
    prompt.rs     — Prompt building
    revert.rs     — Revert management
    info.rs       — SessionInfo, SessionPatch
    reminders.rs  — Reminder system
    todo.rs       — Todo items
  provider/
    mod.rs        — re-exports Provider, Model
    catalog.rs    — ProviderCatalog
    openai.rs     — OpenAI adapter
    anthropic.rs  — Anthropic adapter
    gemini.rs     — Gemini adapter
    service.rs    — ProviderService
  tool/
    mod.rs        — re-exports Tool, ToolRegistry
    registry.rs   — ToolRegistry
    impls.rs      — Tool implementations (bash, read, write, edit, etc.)
    plugin.rs     — PluginToolAdapter
  database/
    mod.rs        — Database trait
    sqlite.rs     — SQLite implementation
    migration.rs  — Migration logic
```

**Implementation Plan:**
1. Create directory structure for each domain
2. Move files into directories, splitting where needed
3. Update `mod.rs` re-exports
4. Update all internal imports (use `super::` or `crate::domain::`)
5. Run full workspace check

**Risks:** Medium — extensive import changes
**Benefits:** Navigation improves from scanning 95 flat names to ~10 domain groups. Internal module types can be `pub(crate)`. Cohesion improves
**Dependencies:** MR-2 (this is the full realization of MR-2)
**Estimated Effort:** 5 person-days
**Priority:** High

---

### AR-5: Add `Send + Sync` Bounds and Async Trait Hygiene

**Current State:** Missing explicit `Send`/`Sync` bounds on some `Arc<dyn Fn>` types. `#[async_trait]` used throughout (6+ traits). `Provider::stream()` returns `Box<dyn Stream>` without `'static` bound. `ClosureProviderPlugin` uses `#[allow(clippy::type_complexity)]` with function pointer soup.

**Target State:** Clean async trait design with explicit `Send + Sync + 'static` bounds. Use GAT for `Provider::Stream` type. Replace `ClosureProviderPlugin`'s 4 function pointer fields with a trait.

**Implementation Plan:**
1. Add `type Stream<'a>: Stream<Item = Result<LlmEvent>> + Send + Unpin + 'a` GAT to `Provider` trait
2. Replace `ClosureProviderPlugin` boxed closures with `trait HookHandler<Ctx>` with async method
3. Audit all `Arc<dyn Fn>` types for missing `Send + Sync` bounds
4. Replace `ask_fn: Option<Arc<dyn Fn(...) -> Pin<Box<dyn Future>>>>` with `Arc<dyn AskPermission>` trait object

**Risks:** Medium — changing trait signatures breaks all provider implementations
**Benefits:** Zero-cost static dispatch for streams (no per-event vtable call). Cleaner closure types. Compiler-enforced thread safety
**Dependencies:** AR-1, MR-2
**Estimated Effort:** 8 person-days
**Priority:** Medium

**Code Example:**
```rust
// Before (provider.rs):
#[async_trait]
pub trait Provider: Send + Sync {
    async fn stream(&self, model: &Model, messages: &[ChatMessage], ...) -> Result<Box<dyn Stream<Item = Result<LlmEvent>> + Send + Unpin>>;
}

// After:
pub trait Provider: Send + Sync {
    type Stream<'a>: futures::Stream<Item = Result<LlmEvent>> + Send + Unpin + 'a
    where
        Self: 'a;

    async fn stream(&self, model: &Model, messages: &[ChatMessage], ...) -> Result<Self::Stream<'_>>;
}
```

---

## Level 4: Strategic Refactoring (3–6 months)

Transformational changes.

---

### SR-1: Implement Effect-Like Dependency Injection System

**Current State:** Manual `Arc` injection — every service takes `Arc<DatabaseService>`, `Arc<ToolRegistry>`, `Arc<PermissionService>`, `SharedBus`, etc. Adding a new service requires changing constructors at every call site. No scoping (singleton vs request-scoped vs session-scoped).

**Target State:** `ServiceRegistry` or `AppContext` struct that holds all services as `Arc<dyn ...>` and is passed as a single parameter. Request-scoped layers for per-request service overrides.

**Implementation Plan:**
1. Define `struct AppContext { db: Arc<dyn Database>, http: Arc<dyn HttpClient>, fs: Arc<dyn FileSystem>, bus: SharedBus, permission: Arc<PermissionService>, tools: Arc<ToolRegistry>, ... }`
2. Change all service constructors from `new(db, http, fs, bus, permission, tools)` to `new(ctx: &AppContext)`
3. Add `Layer` trait for composing contexts (e.g., `TestLayer` overrides DB with in-memory)
4. Add scoping via `with_scope()` that creates a child context with overrides

**Risks:** High — pervasive change to every service's constructor
**Benefits:** Adding a new global service requires adding one field to `AppContext` instead of editing 10+ constructors. Testability: `AppContext::test()` creates all mock services. Request scoping enables per-session database transactions
**Dependencies:** AR-1 (crate split), MR-3, MR-4, MR-6 (all trait extractions)
**Estimated Effort:** 20 person-days
**Priority:** High

**Code Example:**
```rust
// Before:
pub struct SessionManager {
    db: Arc<DatabaseService>,
    bus: SharedBus,
    tool_registry: Arc<ToolRegistry>,
    permission: Arc<PermissionService>,
}

impl SessionManager {
    pub fn new(db: Arc<DatabaseService>, bus: SharedBus,
               tools: Arc<ToolRegistry>, perm: Arc<PermissionService>) -> Self { ... }
}

// After:
pub struct AppContext {
    pub db: Arc<dyn Database>,
    pub http: Arc<dyn HttpClient>,
    pub fs: Arc<dyn FileSystem>,
    pub bus: SharedBus,
    pub tools: Arc<ToolRegistry>,
    pub permission: Arc<PermissionService>,
    pub config: Arc<Config>,
}

impl AppContext {
    pub fn new() -> Self {
        // Production composition root
        Self {
            db: Arc::new(SqliteDatabase::new("data.db").await),
            http: Arc::new(ReqwestHttpClient::new()),
            fs: Arc::new(TokioFileSystem::new()),
            // ...
        }
    }
    pub fn test() -> Self {
        // Test composition root with mocks
        Self {
            db: Arc::new(MockDatabase::new()),
            // ...
        }
    }
}

pub struct SessionManager {
    ctx: Arc<AppContext>,
}

impl SessionManager {
    pub fn new(ctx: Arc<AppContext>) -> Self { Self { ctx } }
}
```

---

### SR-2: Migration from Legacy JSON Columns to Structured SQL Columns

**Current State:** `message.data` and `part.data` columns store JSON blobs. `SessionInfo` is loaded/saved through JSON serialization. Cannot query by message content in SQL. Full deserialization on every session load.

**Target State:** Structured columns: `message.text`, `message.role`, `part.content`, etc. Legacy JSON columns remain for backwards compat but new data uses structured columns. Migration path for existing data.

**Implementation Plan:**
1. Add structured columns alongside existing JSON columns
2. Write dual-write code: write to both JSON column and structured columns
3. Write migration to backfill structured columns from JSON for existing data
4. After migration validated, make structured reads primary with JSON fallback
5. Eventually drop JSON columns and legacy code

**Risks:** High — data migration could lose information. Must maintain backwards compatibility with existing session data
**Benefits:** SQL queryable data; no `serde_json` overhead on session load; type-safe access patterns; smaller storage size
**Dependencies:** MR-3 (database trait makes this a localized change)
**Estimated Effort:** 15 person-days
**Priority:** Medium

---

### SR-3: Full V2 Domain Model Implementation (System Context, EventV2, Location)

**Current State:** Missing V2 domain abstractions that BlazeCode has: System Context algebra (epoch, baseline, snapshot, mid-conversation messages), EventV2 event sourcing, Location-scoped services. The `system_context` module exists but is a stub. BlazeCode will diverge as BlazeCode's V2 matures.

**Target State:** Full V2 domain model:
- `system_context/` module with epoch, baseline, snapshot, source sub-modules
- `Location` as a first-class domain concept (workspace-scoped services)
- EventV2 event sourcing with replayable event streams
- All 129 rules from `blazecode/CONTEXT.md` implemented as test cases

**Implementation Plan:**
1. Study `blazecode/CONTEXT.md` as specification document — 129 rules of system context algebra
2. Implement `ContextSource`, `ContextEpoch`, `Baseline`, `Snapshot`, `MidConversationSystemMessage` types
3. Implement `EventV2` event sourcing with append-only event store
4. Implement `Location` as a first-class value with factory methods
5. Write one test per CONTEXT.md rule

**Risks:** Very high — large scope; TS reference may evolve during implementation
**Benefits:** Feature parity with BlazeCode V2. Session intelligence (compaction, baseline reconciliation) works correctly. Architecture score improves to ~75
**Dependencies:** AR-1, AR-4, SR-1, SR-2
**Estimated Effort:** 60 person-days
**Priority:** Medium

---

### SR-4: Comprehensive Test Suite with Property-Based Testing

**Current State:** <2% test coverage. ~54 trivial unit tests. No integration tests. No property-based tests. No mocking infrastructure. Cannot test most modules because they require real infrastructure (SQLite files, real filesystem, network).

**Target State:** >80% coverage on core types. Property-based tests for: `wildcard_match`, `edit_replace`, `session compaction`, `ID generation`. Integration tests for: session CRUD, tool execution, permission evaluation.

**Implementation Plan:**
1. After MR-3, MR-4, MR-6 (trait extraction): use mock implementations for database, HTTP, filesystem
2. Write unit tests for each module's public API surface
3. Write property-based tests (via `proptest`) for edit_replace and wildcard_match
4. Write integration tests for session lifecycle (create → append → fork → revert)
5. Add test CI step that runs with `--nocapture` for debugging

**Risks:** Low — additive, does not change behavior
**Benefits:** Catches regressions early. Enables confident refactoring. Documents expected behavior through tests
**Dependencies:** MR-3, MR-4, MR-6, SR-1 (trait extraction + DI make testing possible)
**Estimated Effort:** 40 person-days
**Priority:** High

---

### SR-5: Make Providers Pluggable via Dynamic Loading

**Current State:** All providers are statically compiled into `blazecode-core`. Adding a new provider requires modifying core code and recompiling. No plugin discovery mechanism.

**Target State:** Provider implementations can be loaded at runtime via `libloading` dynamic libraries. A `ProviderRegistry` discovers providers from a plugin directory.

**Implementation Plan:**
1. Define C-ABI interface for provider plugins (since Rust doesn't have stable ABI)
2. Create `ProviderPlugin` trait with `extern "C"` entry points
3. Implement `ProviderRegistry` that scans `~/.blazecode/providers/` for `.so`/`.dylib`/`.dll`
4. Keep existing statically-linked providers as fallback
5. Add `blazecode plugin install <path>` command

**Risks:** Very high — Rust has no stable ABI; dynamic loading is complex. Requires careful safety considerations around `#![forbid(unsafe_code)]` — dynamic loading inherently requires unsafe
**Benefits:** Third parties can add providers without recompiling blazecode. Follows BlazeCode's plugin model. Reduces compile times by moving providers out of core
**Dependencies:** AR-1, MR-5, SR-1
**Estimated Effort:** 30 person-days
**Priority:** Low

---

## Priority Matrix

| ID | Refactoring | Effort | Impact | Priority | Level |
|---|---|---|---|---|---|
| QW-2 | Fix `clear_revert` SQL NULL bug | 0.1d | Data corruption fix | Critical | 1 |
| QW-5 | Fix `unwrap()` + `Some()` JSON corruption | 0.5d | Data corruption fix | Critical | 1 |
| QW-4 | Fix V1 permission bypass | 1d | Security fix | Critical | 1 |
| QW-8 | Fix f64 cost precision | 0.5d | Data correctness | Critical | 1 |
| QW-1 | Restore dead-code detection | 0.5d | Quality infrastructure | Critical | 1 |
| QW-3 | Replace 19-param update_session | 0.5d | Maintainability | Critical | 1 |
| QW-9 | Unify error hierarchies (From impls) | 1d | Maintainability | Critical | 1 |
| QW-6 | Arc<Vec<ChatMessage>> in ToolContext | 0.5d | Performance | High | 1 |
| QW-10 | ok_or_500 helper | 0.5d | Maintainability | High | 1 |
| MR-1 | Module visibility discipline | 5d | Architecture | Critical | 2 |
| MR-2 | Split monolithic modules | 10d | Maintainability | Critical | 2 |
| MR-3 | Database trait extraction | 10d | Architecture | Critical | 2 |
| MR-7 | Newtype IDs | 5d | Type safety | High | 2 |
| MR-4 | HTTP client trait | 5d | Architecture | High | 2 |
| MR-6 | Filesystem trait | 8d | Architecture | High | 2 |
| MR-5 | Plugin SDK crate | 3d | Modularization | Medium | 2 |
| AR-1 | Split core into 5-8 crates | 30d | Architecture | Critical | 3 |
| AR-2 | Extract CLI library crate | 10d | Layering | High | 3 |
| AR-3 | Structured concurrency | 15d | Robustness | High | 3 |
| AR-4 | Domain directories | 5d | Cohesion | High | 3 |
| AR-5 | Async trait hygiene | 8d | Performance | Medium | 3 |
| SR-1 | Effect-like DI system | 20d | Architecture | High | 4 |
| SR-4 | Comprehensive test suite | 40d | Quality | High | 4 |
| SR-2 | JSON column migration | 15d | Performance | Medium | 4 |
| SR-3 | V2 domain model | 60d | Feature parity | Medium | 4 |
| SR-5 | Dynamic provider loading | 30d | Extensibility | Low | 4 |

---

## Recommended Execution Roadmap

### Phase 1: Quick Wins (Week 1-2)
Do all QW items first. Each is small, safe, and independently deployable. This fixes 4 critical bugs, restores dead-code detection, eliminates 2.5MB of per-session clones, unifies error hierarchies, and replaces the error-prone 19-param function.

### Phase 2: Module Restructuring (Weeks 3-6)
Execute MR-1, MR-2, MR-7 together. The visibility discipline (MR-1) makes module splitting (MR-2) safer. Newtype IDs (MR-7) are easiest when modules are already split. After this phase, the module structure is visible and the public API is bounded.

### Phase 3: Infrastructure Decoupling (Weeks 7-12)
Execute MR-3 (database trait), MR-4 (HTTP client trait), MR-6 (filesystem trait), MR-5 (plugin SDK). This converts the monolith into a hexagonally-architected system. Core code no longer depends on sqlx, reqwest, or std::fs. Testing becomes possible.

### Phase 4: Crate Extraction (Months 4-5)
Execute AR-1 (split core into 5-8 crates) and AR-2 (extract CLI crate). This is the largest structural change. Each crate has a bounded context. Build times improve. Dependencies are explicit.

### Phase 5: Advanced Patterns (Months 6-8)
Execute AR-3 (structured concurrency), AR-5 (async trait hygiene). Improve async safety and performance.

### Phase 6: Strategic (Months 9-12+)
Execute SR-1 (DI system), SR-4 (test suite), SR-2 (JSON migration), SR-3 (V2 domain model), SR-5 (dynamic loading). These are the long-term transformational changes that bring BlazeCode to parity with BlazeCode V2.

---

## Key Metrics

| Metric | Current | After Phase 1 | After Phase 3 | After Phase 6 |
|---|---|---|---|---|
| Architecture Score | 25/100 | 30/100 | 55/100 | 80/100 |
| Public modules | 95 (all pub) | 95 (30 pub, 65 pub(crate)) | 95 (20 pub, 75 pub(crate)) | 8 crates |
| Files >1000 lines | 14 | 14 | 4 | 0 |
| Error types | 5 separate | 5 (with From impls) | 1 unified | 1 unified |
| Database coupling | Inline sqlx | Inline sqlx | Database trait | Database trait |
| Test coverage | <2% | <5% | ~30% | >80% |
| `unwrap()` in lib code | 100+ | ~70 | ~5 | 0 |
| Newtype IDs | 0 (aliases) | 0 | 5+ domains | 10+ domains |
| Cargo crates | 6 | 6 | 6 | 12-15 |

---

*Report generated by Agent 18 — Refactoring Agent*
