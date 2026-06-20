# RustCode Logic Audit Report

**Date:** 2026-06-19  
**Scope:** Full logic-level comparison of RustCode vs OpenCode (TS)  
**TS Ref:** `5d0f86606ac30690f79f0a6a9f41a1f49fe95d0b`  
**RustCode ref:** Local workspace at `/root/opencodesport/rustcode`

---

## Severity Key

| Severity | Meaning |
|---|---|
| **Critical** | Compile-blocker, runtime panic, or silent data loss |
| **High** | Feature broken or incorrect under normal operation |
| **Medium** | Missing feature, incomplete port, degraded behavior |
| **Low** | Style, ergonomics, or future-proofing concern |

---

## CRITICAL Findings

### C-01: `connect_lazy()` Result passed where SqlitePool expected

**Location:** `rustcode-core/src/runtime.rs:114-118`  
**TS Source:** `packages/core/src/database/database.ts` — `makeDatabase` creates pool via `Effect.tryPromise` (proper error handling)

```rust
let db_pool = sqlx::sqlite::SqlitePoolOptions::new()
    .max_connections(5)
    .connect_lazy(&db_url);                       // returns Result<Pool, sqlx::Error>

let db = Arc::new(DatabaseService::new(db_pool)); // expects SqlitePool, not Result
```

**Problem:** `sqlx::SqlitePoolOptions::connect_lazy()` returns `Result<sqlx::SqlitePool, sqlx::Error>`, but `DatabaseService::new()` takes `sqlx::SqlitePool`. The compiler will emit a type mismatch error. The code silently creates the full type mismatch — the `#![allow(...)]` crate-level lints only suppress *warnings*, not type errors, so this would still fail compilation.

**Root cause:** The developer expected type inference to unwrap, or forgot to add `?` or `.map_err(...)`. The TS equivalent wraps this in `Effect.tryPromise` which produces a structured error channel.

**Impact:** **Compile-blocker.** The binary crate cannot be built. Entirely prevents any executable from being produced.

**Same pattern at `session.rs:2040-2042`:**
```rust
fn test_db() -> Arc<DatabaseService> {
    let pool = sqlx::sqlite::SqlitePoolOptions::new()
        .max_connections(1)
        .connect_lazy("sqlite::memory:");
    Arc::new(DatabaseService::new(pool))
}
```
This is not dead code because tests are compiled with `cargo test`. Any test referencing `test_db()` would also fail to compile.

**Recommendation:** Add `?.map_err(...)` to propagate the error:
```rust
let db_pool = sqlx::sqlite::SqlitePoolOptions::new()
    .max_connections(5)
    .connect_lazy(&db_url)
    .map_err(|e| anyhow::anyhow!("failed to connect to database: {e}"))?;
```

**Effort:** 5 minutes

---

### C-02: `permission::ask()` creates request but never inserts into `self.pending`

**Location:** `rustcode-core/src/permission.rs:968-1018`  
**TS Source:** `packages/opencode/src/permission/index.ts:78-118` — `ask()` inserts into pending via `pending.set(id, entry)`

```rust
pub async fn ask(&self, input: AskInput) -> Result<PermissionAction> {
    // ... evaluation loop ...
    // Creates a PermissionRequest ...
    let request = PermissionRequest { id, session_id, permission, patterns, metadata, always, tool };

    // Publishes bus event ...
    let payload = serde_json::to_value(&request).unwrap_or_default();
    let event = crate::bus::GlobalEvent::new(payload);
    let _ = self.bus.publish(event);

    // *** MISSING: self.pending.insert(id, PendingEntry { request, tx }); ***
    Ok(PermissionAction::Ask)
}
```

Compare with `assert()` at lines 1050-1070 which DOES insert:
```rust
self.pending.insert(id.clone(), PendingEntry { request, tx });
```

**Problem:** `ask()` is the non-blocking entry point for permission requests. It publishes a `permission.asked` event to the bus (for the TUI/CLI to present to the user), but never inserts into `self.pending`. When `reply()` attempts to resolve via `self.pending.remove(&input.request_id)`, it returns `NotFound` because the entry was never stored.

**Impact:** **Any permission check that results in `Ask` via the non-blocking path is permanently unresolvable.** The user will see a permission prompt but their reply will fail with "not found". This breaks the entire interactive permission flow, which is core to OpenCode's security model.

**Root cause:** `ask()` was written before the pending-entry infrastructure (oneshot channels, DashMap) was added to `assert()`. When `assert()` was implemented with proper insertion, `ask()` was not updated to match.

**Recommendation:** Add oneshot channel creation and pending insertion to `ask()`, matching the pattern in `assert()`:
```rust
let (tx, _rx) = tokio::sync::oneshot::channel(); // rx is dropped since ask() is non-blocking
self.pending.insert(id.clone(), PendingEntry { request, tx });
```

**Effort:** 15 minutes

---

### C-03: Double-tokio-runtime in `cmd_tui`

**Location:** `src/main.rs:2394`  
**TS Source:** `packages/opencode/src/cli/cmd/tui.ts:bootstrap()` — runs inside existing Effect runtime

```rust
async fn cmd_tui(args: &TuiArgs, print_logs: bool) -> i32 {
    // ... this is already async, called from dispatch->async_main->rt.block_on
    match rustcode_tui::TuiApp::new(...) {
        Ok(mut app) => {
            let rt = tokio::runtime::Runtime::new().unwrap(); // *** PANIC HERE ***
            let exit_code = rt.block_on(async {
                let tui_result = app.run_async();
                // ...
            });
        }
    }
}
```

**Problem:** `cmd_tui` is an `async fn` — it runs inside the tokio runtime created in `main()` at line 1243 (`tokio::runtime::Builder::new_current_thread().enable_all().build()`). Creating a *second* `tokio::runtime::Runtime::new()` inside an existing runtime context will panic with:
```
thread 'main' panicked at 'Cannot start a runtime from within a runtime'
```

**Impact:** **Runtime panic whenever `rustcode tui` is invoked.** The TUI command is completely non-functional.

**Root cause:** The TUI is being launched via `ratatui` which typically requires its own event loop. The developer assumed a new runtime was needed for the TUI's blocking render loop, but tokio prevents nested runtimes.

**Recommendation:** Replace with `tokio::task::block_in_place` + `tokio::task::LocalSet` or use `tokio::select!` to integrate the TUI's poll-based rendering into the existing runtime. Alternatively, use `tokio::task::spawn_blocking` for the TUI render thread.

**Effort:** 1-2 hours (depends on TUI architecture)

---

### C-04: `EventV2::publish()` never persists synchronized events to database

**Location:** `rustcode-core/src/event.rs:734-766`  
**TS Source:** `packages/core/src/event.ts:431-451` — `publish` commits sync events via `EventTable.insert`

```rust
pub async fn publish(&self, definition: &EventDefinition, data: serde_json::Value,
                     options: Option<PublishOptions>) -> Result<EventPayload, EventError> {
    // ... builds payload ...
    self.notify(&payload, false).await;
    let ch = self.get_or_create_channel(&definition.event_type).await;
    let _ = ch.publish(payload.clone());
    let _ = self.global_channel.publish(payload.clone());
    // *** MISSING: If definition.is_sync(), persist to SQLite event + event_sequence tables ***
    Ok(payload)
}
```

The TS `publish()`:
1. Checks if the event is a sync event
2. If sync, checks `commit_guards`, assigns sequence number, inserts into `EventTable` + `EventSequenceTable`
3. Calls `sync_handlers`
4. Runs projectors
5. Then publishes to pubsub channels

**Problem:** Rust's `publish()` skips all database persistence, sequence assignment, sync handler invocation, and projector execution. The event is broadcast in-memory but never reaches the database.

**Impact:** **Event sourcing is completely non-functional.** Synchronized events (session events, step events, etc.) are never persisted. Session replay, aggregate rebuilding, and cross-session consistency are all broken. The `event` and `event_sequence` tables are never written to.

**Root cause:** The database integration layer (sqlx queries for INSERT into `event` + `event_sequence`) was never implemented. The event data types and registry are fully defined, but the actual persistence code is pure stub.

**Recommendation:** Implement the sync event persistence path:
1. Assign aggregate sequence number via `event_sequence` table (SELECT or INSERT, then UPDATE seq)
2. Validate commit guards
3. INSERT into `event` table
4. Invoke sync handlers + projectors
5. Then publish to in-memory channels

**Effort:** 2-3 days

---

### C-05: `event.rs:listen()` unsubscribe function is a no-op

**Location:** `rustcode-core/src/event.rs:791-803`  
**TS Source:** `packages/core/src/event.ts:630-636` — removes listener by reference

```rust
pub async fn listen(&self, listener: ListenerFn) -> Box<dyn FnOnce() + Send> {
    let mut listeners = self.listeners.write().await;
    listeners.push(listener.clone());
    let arc = Arc::new(listeners.clone());
    let weak = Arc::downgrade(&arc);
    Box::new(move || {
        if let Some(listeners) = weak.upgrade() {
            // *** The comment says it all: "We cannot compare Fn trait objects..." ***
            // Actually: nothing happens here. The listener is never removed.
        }
    })
}
```

**Problem:** The returned unsubscribe function captures a `Weak<Vec<ListenerFn>>` but never actually removes the listener from the global list. The function body is a no-op — it acquires the weak pointer but performs no removal.

**Impact:** **Listener memory leak.** Every call to `listen()` adds a listener that can never be removed. Over time this accumulates, causing:
- Memory growth for every listener registration
- Performance degradation (every event is dispatched to an ever-growing list of dead listeners)
- Logic errors if listeners were supposed to stop receiving events

**Root cause:** Rust's trait objects (`Fn` trait) cannot be compared by identity, so index-based removal requires alternative approaches (e.g., storing an ID alongside the listener).

**Recommendation:** Replace `Vec<ListenerFn>` with a `HashMap<Uuid, ListenerFn>` and return the UUID as the unsubscribe handle.

**Effort:** 2-4 hours

---

## HIGH Findings

### H-01: Session ID format breaks database persistence in CLI mode

**Location:** `src/main.rs:1571`  
**TS Source:** `packages/core/src/session/schema.ts` — sessions use `ses_` prefixed IDs via `Identifier.create("ses", "descending")`

```rust
let session_id = format!("local-{}", std::process::id());
```

**Problem:** The TS codebase generates session IDs with the `ses_` prefix using the system's identifier generator (time-ordered, globally unique). The Rust CLI uses `format!("local-{}", std::process::id())` which:
- Has no `ses_` prefix — does not match the `session` table's ID format
- Collides across runs (process IDs are recycled)
- Cannot be looked up later via `DatabaseService::get_session()`
- Is entirely in-memory with no persistence path

**Impact:** **CLI sessions cannot be resumed, listed, or exported.** Any work done via `rustcode run` is invisible to `rustcode session list`, cannot be continued with `--continue`, and is not stored in the database.

**Root cause:** The session manager's `create()` method generates proper IDs, but `cmd_run` bypasses it and constructs its own `SessionPromptInput` with a fake session ID.

**Recommendation:** Use `SessionManager::create()` to properly register the session before running, and pass the returned session ID to the runner.

**Effort:** 2-3 hours

---

### H-02: `PermissionService::reply()` saves empty `project_id` for "always" permissions

**Location:** `rustcode-core/src/permission.rs:1150-1158`  
**TS Source:** `packages/core/src/permission/saved.ts:62-76` — uses real project_id from session context

```rust
if let Some(ref saved) = self.saved {
    let add_input = AddSavedInput {
        project_id: String::new(),   // *** Always empty! ***
        action: existing.request.permission.clone(),
        resources: existing.request.always.clone(),
    };
    let _ = saved.add(&add_input).await;
}
```

**Problem:** When a user replies "always" to a permission request, the approved patterns are saved to the database with an empty `project_id`. The `permission` table's FK constraint references `project(id)`, so inserting with `project_id = ''` will either violate the FK constraint (if no project with empty ID exists) or create orphaned rows (if one does).

**Impact:** **"Always" permission saving is broken.** The insert silently fails (error is discarded with `let _ =`) or violates the foreign key constraint. Users cannot permanently approve permissions.

**Root cause:** The `ReplyInput` struct doesn't carry a `project_id`, and the session context isn't propagated to the permission service.

**Recommendation:** Add `project_id` to `ReplyInput` and propagate it from the session context when replying.

**Effort:** 1 hour

---

### H-03: Crate-level `#![allow(dead_code, unused_imports, unused_variables)]` masks dead-code rot

**Location:** `rustcode-core/src/lib.rs:2`  
**TS Source:** N/A (TypeScript has no dead-code analysis at compile time, but ESLint with `@typescript-eslint/no-unused-vars` is standard)

```rust
#![allow(dead_code, unused_imports, unused_variables)]
```

Also at `src/main.rs:2`:
```rust
#![allow(dead_code, unused_imports)]
```

**Problem:** These lints suppress all warnings about:
- Unused functions, structs, enums, and constants (dead_code)
- Unused imports (unused_imports)
- Unused function/method parameters (unused_variables)

With 20 modules in `rustcode-core` and 78 module declarations in `lib.rs`, many of which are scaffolding with empty or stub implementations, these lints hide exactly which parts of the port are incomplete.

**Impact:** **Developers have no visibility into dead code.** New contributors cannot tell which modules are production-ready vs. incomplete scaffolding. CI passes green despite large swaths of dead code. A function that is never called anywhere in the codebase produces zero warnings.

**Root cause:** Intentional scaffold-phase relaxation per `CLAUDE.md` lint policy. However, this has been in place since the project began and there is no per-module cleanup plan.

**Recommendation:** Remove `dead_code` and `unused_imports` allows. Mark specific items with `#[allow(dead_code)]` individually where scaffolding is intentional. Add `#[expect(dead_code)]` (Rust 2024 edition feature) for planned-but-not-yet-wired code.

**Effort:** 1 hour (disable allow + fix warnings)

---

### H-04: `tokio::sync::broadcast` silently drops events on overflow

**Location:** `rustcode-core/src/bus.rs:229-236`, `event.rs:627-629`  
**TS Source:** `packages/opencode/src/bus/global.ts` — EventEmitter is unbounded; no event is ever dropped

```rust
// bus.rs:229
pub fn publish(&self, event: GlobalEvent) -> Result<usize, broadcast::error::SendError<GlobalEvent>> {
    // ...
    self.sender.send(event) // Returns Ok(receiver_count) if all receivers have capacity
                             // If any receiver's buffer is full, that receiver silently lags
                             // The event is delivered to fast receivers but NOT to slow ones
}
```

```rust
// bus.rs:345-355 (in recv())
Err(broadcast::error::RecvError::Lagged(skipped)) => {
    tracing::warn!(skipped, "bus subscriber lagged — {skipped} events skipped");
    self.receiver.recv().await.ok()  // Continues after lag, but events are lost forever
}
```

**Problem:** `tokio::sync::broadcast` uses a fixed-size ring buffer. When a subscriber fails to consume events fast enough (e.g., a slow TUI, a database write blocking), old events are overwritten. The `Lagged` error is logged but the lost events are not re-fetched or reconstructed.

The TS `EventEmitter` uses Node.js event loop semantics — events are delivered synchronously via `emit()` and `listeners()` arrays. There is no buffering and no overflow.

**Impact:** **Silent event loss under load.** If the database or TUI falls behind during a burst of LLM streaming events (text deltas, tool calls), these events are permanently lost. The TUI may display stale state. The database may have gaps in event sequences.

**Root cause:** `broadcast::channel` with capacity 256 or 1024 is not sufficient for bursty LLM streaming (a single tool call can emit 50+ text delta events). The capacity is a global backpressure throttle that drops events rather than blocking the producer.

**Recommendation:** Replace `broadcast` with `tokio::sync::mpsc` (unbounded or very large capacity) for the main event bus. Use `broadcast` only for cases where lag-skip is acceptable (e.g., TUI rendering hints that are superseded by later state).

**Effort:** 2-4 hours

---

### H-05: V2 event system types are defined but unused — V1 legacy message/part tables still power sessions

**Location:** `rustcode-core/src/event.rs:976-1610` (all V2 types) vs `rustcode-core/src/session.rs` (V1 flow)  
**TS Source:** `packages/core/src/session/event.ts` — V2 events drive the session lifecycle

The `event.rs` module defines:
- `EventV2` with `publish()`, `subscribe()`, `replay()`, `project()`
- 30+ session event types (AgentSwitchedEvent, StepStartedEvent, ToolCalledEvent, etc.)
- `EventRegistry`, `EventPubSub`, `ProjectorFn`, `ListenerFn`
- Session event type constants in `session_event_types` module

However, the `SessionManager` and `SessionRunner` in `session.rs` and `session_runner.rs` do NOT use `EventV2` for session events. They use:
- `SessionMessageRow` / `MessageRow` / `PartRow` (legacy V1 tables)
- Direct `DatabaseService` calls for CRUD
- The `bus::SharedBus` for runtime notifications

**Problem:** This creates a **parallel architecture** — V2 event types are fully defined (2000+ lines of code) but the session runtime is wired to V1 patterns. No session.prompted, session.step.started, or session.tool.called events are ever emitted through EventV2.

**Impact:** **The V2 event system is dead code.** All the session event types, the event registry, projectors, sync handlers, and the replay system are non-functional despite being fully defined. The 2000+ lines of V2 code will produce no runtime behavior.

**Root cause:** The V2 port was done top-down (types first, implementation later) as per the scaffold phase, but the V1 session impl was written independently. Neither was reconciled.

**Recommendation:** Either:
1. (Preferred) Rewire `SessionManager` and `SessionRunner` to emit V2 events and use `EventV2` for persistence, matching the TS architecture
2. (Minimal) Delete the V2 types and document that RustCode uses V1-only semantics. But this sacrifices event sourcing, replay, and cross-session consistency.

**Effort:** 1-2 weeks (full rewire) or 2 hours (delete + document)

---

### H-06: `config.rs` uses `Arc<RwLock<Config>>` — lock poisoning risk

**Location:** `rustcode-core/src/config.rs` (entire module)  
**TS Source:** `packages/core/src/config/index.ts` — uses Effect.ts `ImmutableState` + `Layer` — no mutable global state

The Rust `Config` is wrapped in `Arc<RwLock<Config>>` and shared across the application. The TS equivalent uses Effect.ts's managed references (`Ref` / `ImmutableState`) which are not susceptible to poisoning.

**Problem:** If any thread holds the write lock and panics (e.g., during config reload or merge), the `RwLock` becomes poisoned. Subsequent reads will panic (`PoisonError`). This is a hard crash with no recovery path.

In the TS codebase:
- Config is loaded once at startup and passed through `Context` (dependency injection)
- Per-scope overrides are provided via `Layer` (composable config layers)
- There is no mutable global config state

**Impact:** **A single panic during config write kills the process.** There is no mechanism to recover from a poisoned lock.

**Root cause:** `RwLock` is a Go-style mutex pattern; Effect.ts's `ImmutableState` uses copy-on-write semantics with structural sharing, which is immune to poisoning.

**Recommendation:**
- Short term: Use `std::sync::RwLock::clear_poison()` (nightly) or catch panics with `std::panic::catch_unwind` around write operations
- Long term: Replace with an immutable config store using `arc_swap` or redesign to load config once at startup without runtime mutation

**Effort:** 1-2 days (short-term fix) | 1-2 weeks (long-term redesign)

---

### H-07: `Agent.generate()` returns `NotImplemented`

**Location:** `rustcode-core/src/agent.rs:553`  
**TS Source:** `packages/opencode/src/agent/generate.ts` — full LLM-driven agent generation pipeline

```rust
Err(crate::error::Error::NotImplemented(
    "Agent.generate requires full provider pipeline — use provider directly".into(),
))
```

**Problem:** The `Agent.generate()` function, which creates new agent definitions via LLM prompting, is a stub that always returns `Error::NotImplemented`. The TS source uses this to power `opencode agent create --description "..."`, allowing users to describe an agent and have the LLM generate the config file.

**Impact:** **`opencode agent create --description` is non-functional.** Users cannot generate agents from natural language descriptions. They must manually write agent config files.

**Root cause:** The provider pipeline required for LLM calls (auth, model resolution, streaming) was not yet complete when agent.rs was written.

**Recommendation:** Wire the provider pipeline into `Agent.generate()` following the TS source at `packages/opencode/src/agent/generate.ts`.

**Effort:** 2-4 hours (after provider pipeline is functional)

---

## MEDIUM Findings

### M-01: `cmd_tui` creates `Runtime::new()` inside existing runtime (context from C-03)

**Location:** `src/main.rs:2394`  
Already described in C-03 with full detail. Categorized as Critical due to the runtime panic.

---

### M-02: `event.rs:publish()` does not invoke `commit_guards`, `sync_handlers`, or `projectors`

**Location:** `rustcode-core/src/event.rs:734-766`  
**TS Source:** `packages/core/src/event.ts:431-451`

The `publish()` method notifies listeners and sends to typed/global channels but:
- Does NOT evaluate commit guards (registered via `before_commit()`)
- Does NOT invoke sync handlers (registered via `sync()`)
- Does NOT invoke projectors (registered via `project()`)

These are all registered as `Arc<dyn Fn>` vectors in the `EventV2` struct, but `publish()` never calls them.

**Impact:** **Projectors and sync handlers are dead code.** Any side-effect logic attached to events (e.g., updating read models, triggering webhooks, updating the TUI) never fires.

**Recommendation:** Add invocation of commit guards (for sync events), sync handlers, and projectors to the `publish()` method, matching the TS dispatch order.

**Effort:** 4 hours

---

### M-03: No MCP server implementation in `rustcode-mcp`

**Location:** `crates/rustcode-mcp/src/`  
**TS Source:** `packages/opencode/src/mcp/` + `packages/core/src/mcp/`

The `rustcode-mcp` crate is listed as a workspace member but contains no meaningful implementation. MCP (Model Context Protocol) servers are a critical feature — they allow OpenCode to interact with external tools and data sources.

**Impact:** **Plugin/tool ecosystem is non-functional.** Users cannot connect to databases, APIs, or external services via MCP.

**Effort:** 1-2 weeks (full MCP protocol implementation)

---

### M-04: No PTY implementation

**Location:** `rustcode-core/src/pty.rs`  
**TS Source:** `packages/core/src/pty/`

The PTY (pseudo-terminal) module provides interactive shell execution for `bash` tool calls. The Rust module exists but does not implement actual PTY spawning or I/O.

**Impact:** **Bash tool cannot run interactive commands.** Process I/O is limited to non-interactive command execution.

**Effort:** 3-5 days

---

### M-05: LSP module is a stub

**Location:** `rustcode-core/src/lsp.rs`, `crates/rustcode-lsp/src/`  
**TS Source:** `packages/opencode/src/lsp/` + `packages/core/src/lsp/`

LSP (Language Server Protocol) provides code intelligence — diagnostics, completions, hover info, symbol search. Both the core LSP types and the LSP server crate are stubs.

**Impact:** **Code intelligence is missing.** No auto-completions, no diagnostics, no symbol search for the AI agent.

**Effort:** 2-4 weeks

---

### M-06: Server crate is a stub

**Location:** `crates/rustcode-server/src/`  
**TS Source:** `packages/opencode/src/server/` (SSE, WebSocket, HTTP API)

The axum-based HTTP/SSE server has endpoints defined in `main.rs` (at very large scale — 7904 lines) but the core server crate has minimal implementation.

**Impact:** **Remote operation is non-functional.** No `rustcode serve` or `rustcode web`.

**Effort:** 2-4 weeks

---

### M-07: EventV2 `sync()` unsubscribe is a no-op (same pattern as H-05)

**Location:** `rustcode-core/src/event.rs:812-818`

```rust
pub async fn sync(&self, handler: SyncFn) -> UnsubscribeFn {
    let mut handlers = self.sync_handlers.write().await;
    handlers.push(handler);
    Box::new(|| {
        // Manual unsubscribe via caller-held handle. — Actually: no-op
    })
}
```

Same issue as C-05. The returned `UnsubscribeFn` never removes the handler.

**Effort:** 1 hour

---

### M-08: CLI command handlers are stubs or empty

**Location:** `src/main.rs` — functions `cmd_*` at various locations  
**TS Source:** `packages/opencode/src/cli/cmd/*.ts` — 23 command handlers

Many `cmd_*` functions in main.rs are empty stubs or have minimal implementation:
- `cmd_generate()` — no implementation
- `cmd_upgrade()` — no implementation
- `cmd_uninstall()` — no implementation
- `cmd_web()` — no implementation
- `cmd_console()` — no implementation
- `cmd_github()` — no implementation
- `cmd_pr()` — no implementation
- `cmd_plugin()` — no implementation
- `cmd_db()` — can't execute queries without database pool
- `cmd_export()`, `cmd_import()` — no implementation

These functions exist to satisfy the match arms in `dispatch()` but return `eprintln!("Not yet implemented")` or similar.

**Impact:** **Most CLI subcommands don't work.** Only `run`, `tui`, `attach`, `debug config/paths`, `models`, `version` have functional implementations.

**Effort:** 2-4 weeks (full implementation of all commands)

---

### M-09: `permission.rs` evaluate function doesn't short-circuit on `Deny`

**Location:** `rustcode-core/src/permission.rs:317-343`  
**TS Source:** `packages/opencode/src/permission/index.ts:39-49`

```rust
pub fn evaluate(permission: &str, pattern: &str, rulesets: &[&PermissionRuleset]) -> EvaluatedPermission {
    for ruleset in rulesets.iter().rev() {
        for rule in ruleset.iter().rev() {
            if wildcard_match(permission, &rule.permission)
                && wildcard_match(pattern, &rule.pattern)
            {
                return EvaluatedPermission { action: rule.action, ... };
            }
        }
    }
    EvaluatedPermission { action: PermissionAction::Ask, ... }
}
```

The TS `evaluate()` uses `.findLast()` which also returns the last matching rule. However, the TS `ask()` at line 78-118 iterates patterns and evaluates each one. When `Deny` is returned, `ask()` immediately returns an error. The Rust `ask()` does the same — but the function `evaluate()` itself doesn't distinguish between the priority levels.

This is actually correct for the "last match wins" semantics — the evaluation function just returns the match. But consider that asking for permission on 5 patterns where pattern #3 is denied — the Rust code will evaluate all remaining patterns after #3 before returning. The TS code does the same, so this is consistent but suboptimal.

**Impact:** Low. Performance issue only, not a correctness bug.

**Recommendation:** Consider short-circuiting in `ask()` and `assert()` loops when a Deny is found.

**Effort:** 30 minutes

---

### M-10: `permission.rs` evaluates all patterns even if first is denied

**Location:** `rustcode-core/src/permission.rs:968-991` (in `ask()`) and `1027-1040` (in `assert()`)

Both `ask()` and `assert()` iterate ALL patterns (via the evaluation loop) before checking for Deny. The TS code does the same with a `for...of` loop and `return` on Deny. So this is a faithful port, but both are suboptimal.

**Impact:** Low. In practice, permission patterns are checked for 1-2 patterns per request.

---

### M-11: `database.rs` has no actual connection pool creation

**Location:** `rustcode-core/src/database.rs` (entire file, 2433 lines)

The database module defines:
- SQL table creation constants (20 tables, 17 indexes)
- Path helpers, migration types, timestamp types
- `DatabaseService` with full CRUD for sessions, messages, parts, session_messages
- 50+ test functions with real SQLite operations

But there is **no function that creates a `SqlitePool` from a `DatabaseConfig`**. The pool creation happens in:
- `runtime.rs:114-116` (has the `connect_lazy` bug)
- `session.rs:2040-2042` (same bug, test helper only)

The `database.rs` module defines `DatabaseConfig` with all connection parameters, but the actual `connect()` call is scattered across other modules. There is no `DatabaseConfig::connect()` method.

**Impact:** Medium. No single place to establish a database connection. The path computation, pragmas, and config are in `database.rs`, but the actual connection is in `runtime.rs`, making it hard to verify database setup correctness at a glance.

**Recommendation:** Add `pub async fn connect(&self) -> Result<SqlitePool, sqlx::Error>` to `DatabaseConfig`.

**Effort:** 30 minutes

---

### M-12: No `tracing` span propagation for request correlation

**Location:** All modules  
**TS Source:** Not applicable (TS has no built-in span tracing)

The TS codebase has no structured logging system. The Rust codebase uses `tracing` but:
- No `#[instrument]` annotations on key functions
- No span propagation for request correlation
- Every log line is a flat `tracing::info!()`/`tracing::debug!()` with no parent span

This means it's impossible to correlate log lines belonging to a single session, a single LLM request, or a single tool invocation.

**Impact:** Medium. Debugging production issues requires log correlation, which is impossible without spans.

**Recommendation:** Add `#[instrument]` to key async functions (`SessionManager::create`, `SessionRunner::run`, `PermissionService::ask`, etc.). Attach session_id as a span field.

**Effort:** 2 hours

---

### M-13: `IdPrefix` enum variant for session IDs not used in CLI mode

**Location:** `rustcode-core/src/id.rs` + `src/main.rs:1571`  
**TS Source:** `packages/core/src/id/` — `Identifier.create("ses", "descending")`

The `IdPrefix::Session` variant exists in the Rust `id.rs` but `cmd_run` uses `format!("local-{}", std::process::id())` instead of `id::descending(IdPrefix::Session, None)`. This is part of the same issue as H-01.

**Impact:** Medium (redundant with H-01 but highlights a design disconnect)

---

### M-14: `cmd_run` uses `SessionPromptInput` directly instead of `SessionManager::create()`

**Location:** `src/main.rs:1567-1591`

Instead of creating a session through `SessionManager::create()` (which registers it in the database and returns a proper session ID), `cmd_run` constructs a `SessionPromptInput` directly with a fake ID. This bypasses all session lifecycle management:

- No session row in the database
- No session row for `session list` to find
- No session to resume with `--continue`
- No session metadata (cost, tokens, model) persisted

**Effort:** 2-3 hours

---

### M-15: `session.rs:test_db()` is used in `#[ignore]` tests but still compiled

**Location:** `rustcode-core/src/session.rs:2039-2043`

The `test_db()` helper has the same `connect_lazy()` compile error as `runtime.rs`. The tests using it are `#[ignore = "needs test database with DatabaseService"]` but:
- The function is still compiled (it's not conditional on a feature flag)
- Any test that calls it would fail at compile time, not at test time

This is a ticking time bomb — if someone removes `#[ignore]` to test the session manager, they'll hit the compile error.

**Effort:** 5 minutes (same fix as C-01)

---

## LOW Findings

### L-01: No platform-specific test coverage for Windows path handling

**Location:** `rustcode-core/src/database.rs:894-973`

The `db_absolute_path()`, `to_platform_path()`, and `is_win_abs()` functions have Windows-specific logic but no CI coverage on Windows. The CLAUDE.md only mentions `ubuntu+macos` in CI.

**Impact:** Low. Path handling on Windows may have edge cases that are not detected.

**Effort:** N/A (not actionable without Windows CI)

---

### L-02: `permission.rs` wildcard regex compilation fallback compares normalized to escaped

**Location:** `rustcode-core/src/permission.rs:262-268`

```rust
Err(_) => {
    tracing::warn!(%pattern, "failed to compile wildcard regex, falling back to exact match");
    normalized == escaped  // Compares the input to the escaped pattern, NOT the original pattern
}
```

The fallback path compares the normalized input to the escaped pattern (with `*` → `.*` transformation), not the original pattern. This means the fallback would not work correctly. However, this code is unlikely to be reached because the regex escape is thorough.

**Impact:** Low. The fallback is only reached on regex compilation failure, which is improbable with the current escaping logic.

**Recommendation:** Fix the fallback to compare the input against the original pattern.

**Effort:** 10 minutes

---

### L-03: `permission.rs` ARITY map has inconsistent token counts

**Location:** `rustcode-core/src/permission.rs:372-511`

The arity map has entries like:
- `("cargo", 2)` — `cargo build` → 2 tokens
- `("git", 2)` — `git status` → 2 tokens  
- `("gh", 3)` — `gh pr create` → 3 tokens

Some entries seem inconsistent:
- `("git config", 3)` — but `git config user.name` would be 4 tokens
- `("npm run", 3)` — but `npm run build` would be 3 tokens (correct here)

This is faithfully ported from the TS source (`packages/opencode/src/permission/arity.ts`) so it's not a porting bug — but the TS source itself may have inconsistencies.

**Impact:** Low. Inconsistent arity values may show slightly incorrect command prefixes to users.

---

### L-04: `permission.rs` `bash_arity_prefix` does not handle `sudo`, `npx`, `bunx` wrappers

**Location:** `rustcode-core/src/permission.rs:539-556`  
**TS Source:** `packages/opencode/src/permission/arity.ts:12-14`

The TS source strips `sudo`, `npx`, `bunx` wrappers before resolving arity:
```ts
if (command.startsWith("sudo ")) command = command.slice(5)
if (command.startsWith("npx ")) command = command.slice(4)
```

The Rust code does not. This means `sudo git status` would be mapped to `sudo` (1 token) instead of `git status` (2 tokens).

**Impact:** Low. The command prefix shown to users would be `sudo` instead of `git status`, which makes permission prompts slightly less useful.

**Effort:** 10 minutes

---

### L-05: `bus.rs` auto-ID uses different encoding than TS

**Location:** `rustcode-core/src/id.rs`  
**TS Source:** `packages/core/src/id/index.ts`

The TS `Identifier.create("evt", "ascending")` uses:
- 12 hex chars (random)
- 14 base62 chars (timestamp + random)

The Rust `id::ascending` may use a different ID format. If bus events are serialized and transmitted across processes (e.g., `rustcode run --attach` connecting to a TS server), the ID format mismatch could cause parsing failures.

**Impact:** Low. Only matters for cross-compatibility between Rust and TS OpenCode instances.

**Effort:** Verify and match the exact TS ID format.

---

## Summary

| Severity | Count | Key Issues |
|---|---|---|
| **Critical** | 5 | Compile blocker (C-01), permission never resolvable (C-02), runtime panic (C-03), event sourcing non-functional (C-04), listener memory leak (C-05) |
| **High** | 7 | Session persistence broken (H-01), permission saving broken (H-02), dead code masked (H-03), silent event loss (H-04), V2 event system unused (H-05), lock poisoning risk (H-06), agent generation stub (H-07) |
| **Medium** | 15 | Projectors/sync handlers not invoked (M-02), missing MCP (M-03), missing PTY (M-04), missing LSP (M-05), missing server (M-06), missing CLI commands (M-08), etc. |
| **Low** | 5 | Regex fallback (L-02), arity wrapper handling (L-04), cross-compat (L-05), Windows coverage (L-01) |

### Top 5 Fixes by Impact

1. **C-01** — Fix `connect_lazy` Result (unblocks compilation, <1hr)
2. **C-02** — Add pending insertion to `ask()` (fixes permission flow, <1hr)
3. **C-03** — Fix double-runtime in `cmd_tui` (unblocks TUI, 1-2hr)
4. **C-05** — Fix listener unsubscribe (stops memory leak, 2-4hr)
5. **H-01** — Use proper session ID + SessionManager::create (enables persistence, 2-3hr)

### Architecture-Level Observations

- **78 modules declared, ~40 functionally empty**: The module tree in `lib.rs` largely declares modules that exist as files but with minimal or stub implementations
- **V1/V2 split**: The codebase has both a legacy V1 session system (message/part tables) and a fully-defined V2 event sourcing system (event/event_sequence tables) — the V2 is dead code until wired up
- **`main.rs` is 7904 lines**: The binary crate has ballooned because command handler implementations were placed inline rather than in separate files. This is contrary to the module structure in `rustcode-core` and should be refactored
- **`#![allow(dead_code)]` prevents CI from catching incomplete modules**: The relaxed lint policy, while intentional for scaffold phase, has persisted long enough to mask significant dead code
- **No cross-process compatibility guarantees**: The TS and Rust instances use different ID formats, different serialization, and different event systems — they cannot interoperate
