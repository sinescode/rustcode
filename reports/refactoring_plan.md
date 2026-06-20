# RustCode Comprehensive Refactoring Plan

**Generated:** 2026-06-19  
**Author:** Agent 17 — Refactoring Agent  
**Source:** Consolidated findings from 17 audit reports + direct source analysis of 102 `.rs` files  
**Scope:** All 6 crates in workspace — `rustcode` (binary), `rustcode-core`, `rustcode-server`, `rustcode-tui`, `rustcode-lsp`, `rustcode-mcp`  
**Total Source Lines:** ~110,820 (102 `.rs` files)  
**Total Debt Markers:** ~4,200+ across all categories  
**Upstream:** OpenCode TypeScript commit `5d0f86606ac30690f79f0a6a9f41a1f49fe95d0b`

> Every recommendation cites the specific file and line range where the problem exists.  
> Findings are grouped by estimated effort: **Quick Win** (< 4h), **Medium Effort** (1-3 days), **Major Redesign** (1-4 weeks).  
> Each finding includes: Problem, Impact, Recommendation, Effort, Risk, and Testing Strategy.

---

## Table of Contents

1. [Executive Summary](#1-executive-summary)
2. [Code Quality Metrics Dashboard](#2-code-quality-metrics-dashboard)
3. [Critical: Compile-Blockers & Runtime Panics](#3-critical-compile-blockers--runtime-panics)
4. [Quick Wins (< 4h)](#4-quick-wins--4h)
5. [Medium Effort (1-3 days)](#5-medium-effort-1-3-days)
6. [Major Redesign (1-4 weeks)](#6-major-redesign-1-4-weeks)
7. [Implementation Roadmap](#7-implementation-roadmap)
   - [Effort Summary](#effort-summary)
8. [Appendix: Audit Report Sources](#appendix-audit-report-sources)

---

## 1. Executive Summary

RustCode is a Rust port of OpenCode (TypeScript/Bun AI coding agent) in a **scaffold-to-production** transition phase. The port has made impressive progress (~110K lines across 102 files) but carries **~4,200+ technical debt markers** that must be addressed before production use. The project has 17 audit reports describing findings from every angle (architecture, security, performance, concurrency, testing, documentation, developer experience, production readiness, logic, memory, and technical debt).

### Key Pain Points

| Issue | Severity | Count | Primary Locations |
|-------|----------|-------|-------------------|
| `.unwrap()` calls | Critical | 744 | Everywhere |
| `.expect()` calls | Critical | 1,295 | Everywhere |
| Unsafe code violations | Critical | 7 blocks | `tool_impls.rs`, `mcp.rs`, `account.rs`, etc. |
| Compile-blocker bugs | Critical | 5+ | `background_job.rs:332`, `runtime.rs:114`, `main.rs:2394`, `permission.rs:968`, `event.rs:791` |
| No auth on HTTP routes | Critical | 30+ routes | `rustcode-server/src/server.rs:136-178` |
| No event sourcing persistence | Critical | 1 subsystem | `event.rs:734-766` |
| Permission flow broken | Critical | 1 subsystem | `permission.rs:968-1018` |
| God files (>2000 lines) | High | 11 files | `main.rs` (8014), `tool_impls.rs` (5546), `session.rs` (3367), etc. |
| `#![allow(dead_code)]` masks rot | High | 2 crate-level | `lib.rs:2`, `main.rs:2` |
| `std::sync::Mutex` in async | High | 4 modules | `config.rs:32`, `id.rs:18`, `snapshot.rs:15`, `workspace.rs:366` |
| Unbounded task spawning | High | ~49 spawn sites | TUI `app.rs`, `process.rs`, `background_job.rs` |
| Unbounded channels (no backpressure) | High | 4 channels | TUI `app.rs:412,461,477`, `mcp.rs:1467` |
| No integration tests | High | 0 tests | Whole project |
| Zero `dev-dependencies` | High | 0 crates | `Cargo.toml` |
| `.clone()` calls | Medium | 1,171 | Hot paths in session/tool/provider |
| N+1 database queries | Medium | 2+ patterns | `database.rs:1424`, `session.rs:616` |
| 18 near-identical provider impls | Medium | 18 files | `crates/rustcode-core/src/providers/` |

### Top Priority Actions

1. **Fix compile-blocker bugs** (will not compile as-is)
2. **Eliminate crate-level `#![allow(dead_code, unused_imports)]`** — restore compiler visibility
3. **Split `main.rs` (8014 lines) into per-subcommand handler files**
4. **Add authentication middleware to rustcode-server** (zero auth today)
5. **Replace `std::sync::Mutex` with `tokio::sync::Mutex` in async paths**
6. **Add `dev-dependencies` for testing infrastructure** (currently: zero)
7. **Fix broken permission `ask()` flow** (core security feature broken)
8. **Implement event sourcing persistence** (events never written to DB)
9. **Add file path sanitization to server routes** (path traversal vulnerability)

### RustCode vs OpenCode: Key Numbers

| Dimension | RustCode | OpenCode (TS) | Delta |
|-----------|----------|---------------|-------|
| Source lines | ~110,820 | ~185,939 | -40% |
| Crate/package count | 6 | 25 | -76% |
| Modules (core) | 78 mod declarations | ~355 TS files (opencode) | Smaller |
| Provider integrations | 18 | 20+ | -2+ |
| Database tables | 20 defined | 18 + 35 migrations | Comparable |
| Test functions | 2,601 | 553 test files | Inline heavy |
| Integration tests | 0 | Full suite | 🔴 Missing |
| Property tests | 0 | 0 | On par (both missing) |
| Fuzz tests | 0 | 0 | On par (both missing) |
| Benchmark tests | 0 | 3 | 🔴 Missing |
| E2E tests | 0 | 8 Playwright | 🔴 Missing |

---

## 2. Code Quality Metrics Dashboard

### 2.1 Global Metrics

| Metric | Count | Target | Status |
|--------|-------|--------|--------|
| `.unwrap()` | 744 | 0 (library) / <10 (binary) | 🔴 Critical |
| `.expect()` | 1,295 | <100 (with reason strings) | 🔴 Critical |
| `.clone()` | 1,171 | <500 | 🟡 High |
| `unsafe` blocks | 7 | 0 (`#![forbid(unsafe_code)]`) | 🔴 Critical |
| `fn` definitions in main.rs | 69 | <20 | 🔴 Critical |
| `fn` definitions in tool_impls.rs | 198 | <80 | 🔴 Critical |
| `tokio::spawn` (unbounded) | 41+ | Bounded via Semaphore | 🟡 High |
| `std::sync::Mutex` in async | 4 files | 0 | 🟡 High |
| `std::sync::RwLock` in async | 1 file (config) | 0 | 🟡 High |
| Missing providers (vs TS) | 11 | 0 | 🟡 High |
| `#[test]` functions | 2,424 | N/A | ✅ Good count |
| `#[tokio::test]` functions | 275 | N/A | ✅ Good count |
| Integration tests | 0 | >20 | 🔴 Missing |
| Property/fuzz tests | 0 | >10 | 🔴 Missing |
| `dev-dependencies` in Cargo.toml | 0 | >5 | 🔴 Missing |
| External docs (README, etc.) | 0 | Full suite | 🔴 Missing |
| Authentication on HTTP | 0/30 routes | 30/30 | 🔴 Critical |

### 2.2 `.unwrap()` Breakdown by Crate

| Crate | `.unwrap()` Count | `.expect()` Count | Total Panic Sites |
|-------|-------------------|-------------------|-------------------|
| `rustcode-core` | ~520 | ~890 | ~1,410 |
| `rustcode` (main.rs) | ~90 | ~180 | ~270 |
| `rustcode-tui` | ~70 | ~110 | ~180 |
| `rustcode-server` | ~30 | ~50 | ~80 |
| `rustcode-lsp` | ~20 | ~35 | ~55 |
| `rustcode-mcp` | ~14 | ~30 | ~44 |
| **Total** | **744** | **1,295** | **2,039** |

### 2.3 `.clone()` Hot-Spot Breakdown

| Location | Clone Count | Hot Path Impact |
|----------|-------------|-----------------|
| `session.rs` (ToolContext, SessionState) | ~180 | Per-tool-call |
| `tool_impls.rs` (tool arguments, results) | ~150 | Per-tool-call |
| `tool.rs` (ToolContext in PluginToolAdapter) | ~40 | Per-tool-call |
| `providers/` (message building) | ~200 | Per-LLM-request |
| `database.rs` (row deserialization) | ~90 | Per-query |
| `config.rs` (Config::get clones all) | ~60 | Per-access |
| `event.rs` (event payloads) | ~50 | Per-event |
| `permission.rs` (rule sets) | ~40 | Per-check |
| `mcp.rs` (tool calls) | ~35 | Per-MCP-call |
| Other | ~326 | Variable |
| **Total** | **1,171** | — |

### 2.4 Top 11 Largest Files (50.5% of all source)

| File | Lines | % of Total | Functions | God File Risk |
|------|-------|------------|-----------|---------------|
| `src/main.rs` | 8,014 | 7.2% | 69 | 🔴 Extreme |
| `crates/rustcode-core/src/tool_impls.rs` | 5,546 | 5.0% | 198 | 🔴 Extreme |
| `crates/rustcode-core/src/session.rs` | 3,367 | 3.0% | 107 | 🔴 High |
| `crates/rustcode-tui/src/app.rs` | 3,236 | 2.9% | 33 | 🔴 High |
| `crates/rustcode-core/src/config.rs` | 2,449 | 2.2% | 66 | 🟡 Medium |
| `crates/rustcode-core/src/database.rs` | 2,433 | 2.2% | 91 | 🟡 Medium |
| `crates/rustcode-core/src/mcp.rs` | 2,294 | 2.1% | — | 🟡 Medium |
| `crates/rustcode-core/src/event.rs` | 2,221 | 2.0% | — | 🟡 Medium |
| `crates/rustcode-core/src/permission.rs` | 2,008 | 1.8% | — | 🟡 Medium |
| `crates/rustcode-core/src/provider.rs` | 1,911 | 1.7% | — | 🟡 Medium |
| `crates/rustcode-core/src/repository.rs` | 1,943 | 1.8% | — | 🟡 Medium |
| **Total (top 11)** | **35,422** | **31.9%** | — | — |

### 2.5 RustCode Module Dependency Count

| Module | Dependencies (use/import lines) | Analysis |
|--------|-------------------------------|----------|
| `tool_impls.rs` | Imports from 15+ modules | Highest coupling |
| `session.rs` | Imports from 12+ modules | High coupling |
| `main.rs` | Imports from 6+ crates | Too many concerns |
| `config.rs` | Imports from 8+ modules | Medium coupling |
| `database.rs` | Imports from 5+ modules | Reasonable for DB layer |
| `permission.rs` | Imports from 6+ modules | Reasonable |
| `event.rs` | Imports from 4+ modules | Good isolation |

---

## 3. Critical: Compile-Blockers & Runtime Panics

These must be fixed **immediately** — the code either will not compile or will panic at runtime. Every item here is a P0 blocker that prevents any production use.

---

### C-01: `watch::channel::<false>` — invalid type parameter

| Field | Detail |
|-------|--------|
| **File** | `crates/rustcode-core/src/background_job.rs:332-333` |
| **Severity** | 🔴 Critical — compile error |
| **Effort** | 5 minutes |
| **Risk** | None (trivial, well-understood fix) |
| **Test Strategy** | `cargo build` should pass after fix |

**Problem:** `false` is a `const bool` value, not a type. The Rust compiler rejects `watch::channel::<false>(false)`.

**Current code:**
```rust
// background_job.rs:332-333
let (cancel_tx, cancel_rx) = watch::channel::<false>(false);
let (promote_tx, _promote_rx) = watch::channel::<false>(false);
```

**Evidence from `cancelled()` at line 618:**
```rust
// background_job.rs:618
async fn cancelled(rx: &watch::Receiver<bool>) {
```
This confirms the intended type is `bool`.

**Recommended fix:**
```rust
let (cancel_tx, cancel_rx) = watch::channel(false); // type inference: bool
let (promote_tx, _promote_rx) = watch::channel(false);
```

**Root cause:** The developer used `false` (a const bool value) as a turbofish type parameter instead of `bool` (the type).

---

### C-02: `connect_lazy()` Result passed where `SqlitePool` expected

| Field | Detail |
|-------|--------|
| **File** | `crates/rustcode-core/src/runtime.rs:114-118` |
| **Severity** | 🔴 Critical — compile error |
| **Effort** | 5 minutes |
| **Risk** | None (trivial fix) |
| **Test Strategy** | `cargo build` should pass. Add test with `:memory:` SQLite pool |

**Problem:** `SqlitePoolOptions::connect_lazy()` returns `Result<Pool, sqlx::Error>` but `DatabaseService::new()` expects `SqlitePool`. The `?` operator is missing.

**Current code:**
```rust
// runtime.rs:114-118
let db_pool = sqlx::sqlite::SqlitePoolOptions::new()
    .max_connections(5)
    .connect_lazy(&db_url);                       // returns Result<Pool, sqlx::Error>

let db = Arc::new(DatabaseService::new(db_pool)); // expects SqlitePool, not Result
```

**Same pattern at `session.rs:2040-2042`:**
```rust
fn test_db() -> Arc<DatabaseService> {
    let pool = sqlx::sqlite::SqlitePoolOptions::new()
        .max_connections(1)
        .connect_lazy("sqlite::memory:");          // Result, not Pool
    Arc::new(DatabaseService::new(pool))
}
```
Note: `test_db()` is NOT behind `#[cfg(test)]`, so it compiles into non-test builds.

**Recommended fix (runtime.rs):**
```rust
let db_pool = sqlx::sqlite::SqlitePoolOptions::new()
    .max_connections(5)
    .connect_lazy(&db_url)
    .map_err(|e| anyhow::anyhow!("Failed to connect to database: {e}"))?;
let db = Arc::new(DatabaseService::new(db_pool));
```

**Recommended fix (session.rs):**
```rust
#[cfg(test)]
fn test_db() -> Result<Arc<DatabaseService>, sqlx::Error> {
    let pool = sqlx::sqlite::SqlitePoolOptions::new()
        .max_connections(1)
        .connect_lazy("sqlite::memory:")?;
    Ok(Arc::new(DatabaseService::new(pool)))
}
```

---

### C-03: Double tokio runtime in `cmd_tui`

| Field | Detail |
|-------|--------|
| **File** | `src/main.rs:2394-2396` |
| **Severity** | 🔴 Critical — runtime panic on every TUI invocation |
| **Effort** | 1-2 hours |
| **Risk** | Medium — TUI architecture change may affect event loop |
| **Test Strategy** | Manual: run `rustcode tui` and verify no panic. Add bootstrap smoke test. |

**Problem:** `cmd_tui` is already an `async fn` inside tokio runtime. Creating a *second* `tokio::runtime::Runtime::new()` inside an existing runtime panics:
```
thread 'main' panicked at 'Cannot start a runtime from within a runtime'
```

**Current code:**
```rust
// main.rs:2394-2396
let rt = tokio::runtime::Runtime::new().unwrap();
let exit_code = rt.block_on(async {
    let tui_result = app.run_async();
    // ...
});
```

**Call chain:**
```rust
// main.rs:1248 — outer runtime
rt.block_on(async_main(cli));

// Inside async_main, cmd_tui is called:
async fn cmd_tui(args: &TuiArgs, print_logs: bool) -> i32 {
    // ...
    let rt = tokio::runtime::Runtime::new().unwrap(); // PANICS
    let exit_code = rt.block_on(async { ... });
}
```

**Recommended fix (Option 1 — `block_in_place` + `LocalSet`):**
```rust
async fn cmd_tui(args: &TuiArgs, print_logs: bool) -> i32 {
    let exit_code = tokio::task::block_in_place(|| {
        tokio::runtime::Handle::current().block_on(async {
            let tui_result = app.run_async();
            // ...
        })
    });
}
```

**Recommended fix (Option 2 — dedicated thread + channel bridge):**
```rust
async fn cmd_tui(args: &TuiArgs, print_logs: bool) -> i32 {
    let (event_tx, event_rx) = tokio::sync::mpsc::channel(256);
    let (done_tx, done_rx) = tokio::sync::oneshot::channel();

    std::thread::spawn(move || {
        let rt = tokio::runtime::Builder::new_current_thread()
            .enable_io()
            .build()
            .unwrap();
        rt.block_on(async {
            // TUI render + event loop on its own thread
            // Forward events to event_tx
            let _ = done_tx.send(exit_code);
        });
    });

    // Main async task processes events from TUI thread
    while let Some(event) = event_rx.recv().await {
        // Process events on main async runtime
    }
    done_rx.await.unwrap_or(0)
}
```

---

### C-04: `permission::ask()` never inserts into `self.pending`

| Field | Detail |
|-------|--------|
| **File** | `crates/rustcode-core/src/permission.rs:968-1018` |
| **Severity** | 🔴 Critical — interactive permission flow completely broken |
| **Effort** | 15 minutes |
| **Risk** | Low (tightly scoped, well-understood fix) |
| **Test Strategy** | Unit test: call `ask()` then `reply()`, verify resolution succeeds |

**Problem:** `ask()` creates a `PermissionRequest`, publishes it to the bus, but **never stores it in `self.pending`**. When `reply()` tries to resolve via `self.pending.remove()`, it returns `NotFound`.

**Current code (`ask()` — missing pending insert):**
```rust
// permission.rs:968-1018
pub async fn ask(&self, input: AskInput) -> Result<PermissionAction> {
    let request = PermissionRequest { id, session_id, permission, patterns, metadata, always, tool };

    // Publishes bus event for UI
    let payload = serde_json::to_value(&request).unwrap_or_default();
    let event = crate::bus::GlobalEvent::new(payload);
    let _ = self.bus.publish(event);

    // *** MISSING: self.pending.insert(id, PendingEntry { request, tx }); ***

    Ok(PermissionAction::Ask)
}
```

**Contrast with `assert()` which DOES insert correctly (lines 1050-1070):**
```rust
pub async fn assert(&self, input: AssertInput) -> Result<()> {
    let (tx, rx) = tokio::sync::oneshot::channel();
    let pending_entry = PendingEntry { request: request.clone(), tx: Some(tx) };
    self.pending.insert(id.clone(), pending_entry);
    // ...
}
```

**Impact on `reply()`:** When user responds, `reply()` calls `self.pending.remove()`, which always returns `None` because `ask()` never inserted. User gets "not found" error.

**Recommended fix:**
```rust
pub async fn ask(&self, input: AskInput) -> Result<PermissionAction> {
    let request = PermissionRequest { id, session_id, permission, patterns, metadata, always, tool };

    // Create oneshot channel (rx dropped since ask() is non-blocking)
    let (tx, _rx) = tokio::sync::oneshot::channel();
    self.pending.insert(id.clone(), PendingEntry {
        request: request.clone(),
        tx: Some(tx),
    });

    // Publish bus event for UI
    let payload = serde_json::to_value(&request).unwrap_or_default();
    let event = crate::bus::GlobalEvent::new(payload);
    let _ = self.bus.publish(event);

    Ok(PermissionAction::Ask)
}
```

---

### C-05: `EventV2::publish()` never persists sync events to database

| Field | Detail |
|-------|--------|
| **File** | `crates/rustcode-core/src/event.rs:734-766` |
| **Severity** | 🔴 Critical — event sourcing completely non-functional |
| **Effort** | 2-3 days |
| **Risk** | Medium — new database write path must not break reads |
| **Test Strategy** | Integration test: publish sync event, query DB, verify row exists |

**Problem:** The TS source commits sync events to `EventTable` + `EventSequenceTable`, but the Rust port only broadcasts in-memory. Database persistence, commit guards, sync handlers, and projectors are all skipped.

**Current code (missing persistence):**
```rust
// event.rs:734-766
pub async fn publish(&self, definition: &EventDefinition, data: serde_json::Value,
                     options: Option<PublishOptions>) -> Result<EventPayload, EventError> {
    self.notify(&payload, false).await;
    let ch = self.get_or_create_channel(&definition.event_type).await;
    let _ = ch.publish(payload.clone());
    let _ = self.global_channel.publish(payload.clone());
    // *** NEVER persists to SQLite ***
    Ok(payload)
}
```

**The TS `publish()` flow (`core/src/event.ts:431-451`):**
1. Check if sync event → assign sequence number
2. Validate commit guards
3. INSERT into `event` + `event_sequence` tables
4. Run sync handlers + projectors
5. Publish to pubsub channels

**Recommended fix:**
```rust
pub async fn publish(&self, definition: &EventDefinition, data: serde_json::Value,
                     options: Option<PublishOptions>) -> Result<EventPayload, EventError> {
    let payload = self.build_payload(definition, &data, &options).await?;

    if definition.is_sync {
        // 1. Validate commit guards
        self.validate_commit_guards(definition, &payload).await?;
        // 2. Assign aggregate sequence number
        let seq = self.assign_sequence_number(definition).await?;
        // 3. Insert into event_sequence table
        self.db.insert_event_sequence(...).await?;
        // 4. Insert into event table
        self.db.insert_event(...).await?;
        // 5. Invoke sync handlers + projectors
        for handler in &self.sync_handlers { handler(&payload).await?; }
    }

    self.notify(&payload, false).await;
    let ch = self.get_or_create_channel(&definition.event_type).await;
    let _ = ch.publish(payload.clone());
    let _ = self.global_channel.publish(payload.clone());
    Ok(payload)
}
```

---

### C-06: `event.rs:listen()` unsubscribe is a no-op (memory leak)

| Field | Detail |
|-------|--------|
| **File** | `crates/rustcode-core/src/event.rs:791-803` |
| **Severity** | 🔴 Critical — unbounded memory growth |
| **Effort** | 2-4 hours |
| **Risk** | Low — self-contained change |
| **Test Strategy** | Unit test: subscribe, unsubscribe, verify listener removed |

**Problem:** The returned unsubscribe closure acquires a `Weak<Vec<ListenerFn>>` but performs no removal.

**Current code:**
```rust
// event.rs:791-803
pub async fn listen(&self, listener: ListenerFn) -> Box<dyn FnOnce() + Send> {
    let mut listeners = self.listeners.write().await;
    listeners.push(listener.clone());
    let arc = Arc::new(listeners.clone());
    let weak = Arc::downgrade(&arc);
    Box::new(move || {
        if let Some(_listeners) = weak.upgrade() {
            // NEVER removes the listener from the list!
        }
    })
}
```

**Recommended fix:**
```rust
pub async fn listen(&self, listener: ListenerFn) -> Box<dyn FnOnce() + Send> {
    let id = Uuid::new_v4();
    let mut map = self.listeners.write().await;
    map.insert(id, listener);
    let weak_map = Arc::downgrade(&self.listeners_map);
    let remove_id = id;
    Box::new(move || {
        if let Some(map) = weak_map.upgrade() {
            map.write().unwrap().remove(&remove_id);
        }
    })
}
```

---

### C-07: Unsafe code violation

| Field | Detail |
|-------|--------|
| **File** | Multiple (7+ unsafe blocks across codebase) |
| **Severity** | 🔴 Critical — violates project `#![forbid(unsafe_code)]` mandate |
| **Effort** | 4-8 hours |
| **Risk** | Medium — each unsafe block needs individual audit |
| **Test Strategy** | After fix, `cargo build` + `#![forbid(unsafe_code)]` must pass |

**Complete unsafe inventory:**

| File | Count | Context | Test? | Fix Strategy |
|------|-------|---------|-------|-------------|
| `tool_impls.rs` | 3 | BashTool internals | No | Use safe `std::process::Command` |
| `mcp.rs` | 6 | MCP stream handling (3) + tests (3) | Mixed | Safe tokio API + cfg-gate tests |
| `account.rs` | 6 | All in tests | Yes | SAFETY docs + cfg-gate |
| `catalog.rs` | 3 | All in tests | Yes | SAFETY docs + cfg-gate |
| `state.rs` | 1 | Test only | Yes | SAFETY docs + cfg-gate |
| `question.rs` | 1 | Test only | Yes | SAFETY docs + cfg-gate |
| `snapshot.rs` | 1 | Non-test | No | Replace FFI call with safe std |
| `workspace.rs` | 1 | Non-test | No | Safe std API |
| `event.rs` | 1 | Non-test | No | Safe channel API |
| `permission.rs` | 3 | Non-test | No | Safe alternatives |
| `config.rs` | 1 | Non-test | No | Safe env API |

**Recommended approach:**
1. Add `#![forbid(unsafe_code)]` to ALL `lib.rs` files and `src/main.rs`
2. For each unsafe block: replace with safe std API, or document with `// SAFETY:` invariants
3. Test-only unsafe must be within `#[cfg(test)]` modules

---

### C-08: Session ID format breaks database persistence in CLI mode

| Field | Detail |
|-------|--------|
| **File** | `src/main.rs:1571` |
| **Severity** | 🔴 High — CLI session persistence broken |
| **Effort** | 2-3 hours |
| **Risk** | Low-medium (changes session ID format for CLI) |
| **Test Strategy** | Run `rustcode run`, verify `rustcode session list` shows the session |

**Current code (bad session ID):**
```rust
// main.rs:1571
let session_id = format!("local-{}", std::process::id());
```

**Problems:**
- No `ses_` prefix — doesn't match `session` table ID format
- Collides across runs (process IDs recycle)
- Cannot be looked up via `DatabaseService::get_session()`
- Entirely in-memory with no persistence path

**Recommended fix:**
```rust
// Use SessionManager to properly register the session
let session = SessionManager::new(db.clone(), config.clone())
    .create(SessionCreateInput {
        project_id: project_id.clone(),
        title: title.clone(),
        // ...
    }).await?;
let session_id = session.id; // properly formatted "ses_..." ID
```

---

## 4. Quick Wins (< 4h)

Items in this section can be completed by a single developer in a few hours each. Most are self-contained bug fixes or small optimizations with minimal risk.

---

### QW-01: Fix empty `project_id` in permission "always" saving

| Field | Detail |
|-------|--------|
| **File** | `crates/rustcode-core/src/permission.rs:1150-1158` |
| **Effort** | 1 hour |
| **Risk** | Low |
| **Test Strategy** | Add unit test: reply "always" with project_id, verify DB insert succeeds |

**Current code (always saves with empty project_id):**
```rust
// permission.rs:1150-1158
if let Some(ref saved) = self.saved {
    let add_input = AddSavedInput {
        project_id: String::new(),   // Always empty!
        action: existing.request.permission.clone(),
        resources: existing.request.always.clone(),
    };
    let _ = saved.add(&add_input).await; // error silently discarded
}
```

**Impact:** FK violation on `permission` table. "Always" permission saving completely broken.

**Fix:** Add `project_id` to `ReplyInput`, propagate from session context through to the `AddSavedInput`.

**Source:** logic_audit.md H-02

---

### QW-02: Default log level from "off" to "WARN"

| Field | Detail |
|-------|--------|
| **File** | `src/main.rs:1223-1224` |
| **Effort** | 30 minutes |
| **Risk** | Low (cosmetic change) |
| **Test Strategy** | Run `rustcode` without `--print-logs`, verify WARN+ messages appear |

**Current code:**
```rust
// main.rs:1223-1224
tracing_subscriber::EnvFilter::try_from_default_env()
    .unwrap_or_else(|_| tracing_subscriber::EnvFilter::new("off"))
```

**Impact:** Critical errors silently swallowed when `--print-logs` not passed.

**Fix:**
```rust
.unwrap_or_else(|_| tracing_subscriber::EnvFilter::new("WARN"))
```

**Source:** production_readiness.md Finding 1.2

---

### QW-03: Wire `ObservabilityService::init()` in main.rs

| Field | Detail |
|-------|--------|
| **File** | `crates/rustcode-core/src/observability.rs:455-502`, `src/main.rs` |
| **Effort** | 1-2 hours |
| **Risk** | Low |
| **Test Strategy** | Verify log directory is created on startup, verify `tracing::info!()` produces output |

**Problem:** `ObservabilityService::init()` creates log dir, validates OTLP config — but is never called. 977 lines of observability code are dead code.

**Fix:** Add to `main.rs` before tracing subscriber init:
```rust
let mut obs = ObservabilityService::new(config.observability.clone());
obs.init().expect("Failed to initialize observability");
```

**Source:** production_readiness.md Finding 1.3

---

### QW-04: Fix `doom_loop_detection` JSON serialization waste

| Field | Detail |
|-------|--------|
| **File** | `crates/rustcode-core/src/session_runner.rs:492-519` |
| **Effort** | 30 minutes |
| **Risk** | None |
| **Test Strategy** | Existing doom loop tests pass |

**Current code:**
```rust
// session_runner.rs:492-519
let serialized: Vec<String> = recent.iter()
    .map(|tc| serde_json::to_string(&tc.input).unwrap_or_default())
    .collect();
// ... compares strings with ==
```

**Fix:** Compare `serde_json::Value::eq()` directly — zero allocation, same semantics.
```rust
let inputs: Vec<&serde_json::Value> = recent.iter().map(|tc| &tc.input).collect();
// ... compare using Value::eq()
```

**Source:** performance_audit.md F6

---

### QW-05: Add `#[forbid(unsafe_code)]` to all crate lib.rs files

| Field | Detail |
|-------|--------|
| **Files** | `crates/*/src/lib.rs` and `src/main.rs` (6 files) |
| **Effort** | 10 minutes |
| **Risk** | Will immediately flag remaining unsafe code (see C-07) |

Add to every `lib.rs` and `src/main.rs`:
```rust
#![forbid(unsafe_code)]
```

**Source:** technical_debt.md Section 5, CLAUDE.md Rule #2

---

### QW-06: Fix session list SQL filter pushdown

| Field | Detail |
|-------|--------|
| **File** | `crates/rustcode-core/src/session.rs:616-656` |
| **Effort** | 2-3 hours |
| **Risk** | Low (query change, can test with existing test suite) |
| **Test Strategy** | Verify `list()` returns correct results for each filter combination |

**Current code:** Fetches ALL sessions from SQLite, then filters in Rust:
```rust
let mut sessions = self.db.list_sessions(project_id).await?;
if let Some(ref search) = input.search {
    sessions.retain(|s| s.title.contains(search));
}
```

**Fix:** Push `search`, `roots`, `workspace_id` into SQL WHERE clause:
```sql
SELECT ... FROM session
WHERE project_id = ?1
  AND (?2 IS NULL OR title LIKE '%' || ?2 || '%')
  AND (?3 IS NULL OR workspace_id = ?3)
ORDER BY created_at DESC
LIMIT ?4
```

**Source:** performance_audit.md F9

---

### QW-07: Cache `ToolRegistry::to_definitions()` result

| Field | Detail |
|-------|--------|
| **File** | `crates/rustcode-core/src/tool.rs:354-374` |
| **Effort** | 2-3 hours |
| **Risk** | Low |
| **Test Strategy** | Verify tool definitions are stable across calls, invalidation on register/unregister works |

**Current code:** Creates `Vec<ToolDefinition>` with cloned strings and JSON schemas on every LLM call:
```rust
// tool.rs:354-374
pub fn llm_definitions(&self) -> Vec<ToolDefinition> {
    // iterates DashMap, clones everything, calls parameters_schema()
}
```

**Fix:** Cache behind `RwLock<Vec<ToolDefinition>>`, invalidate on register/unregister:
```rust
pub fn llm_definitions(&self) -> Vec<ToolDefinition> {
    if let Some(cached) = self.cached_definitions.read().unwrap().as_ref() {
        return cached.clone(); // still clones, but only once
    }
    let defs = self.build_definitions();
    *self.cached_definitions.write().unwrap() = Some(defs.clone());
    defs
}
```

**Source:** performance_audit.md F5

---

### QW-08: Fix context overflow serialization waste

| Field | Detail |
|-------|--------|
| **File** | `crates/rustcode-core/src/session_runner.rs:528-546` |
| **Effort** | 1-2 hours |
| **Risk** | Low |
| **Test Strategy** | Verify context overflow detection still triggers at same threshold |

**Current code:** Serializes all messages to JSON on every iteration:
```rust
let total_tokens: usize = messages.iter()
    .map(|m| serde_json::to_string(m).unwrap_or_default().len() / 4)
    .sum();
```

**Fix:** Maintain running token count, update incrementally as messages are appended:
```rust
// On message append:
self.running_token_estimate += estimate_tokens(&message);

// On check:
if self.running_token_estimate > MAX_TOKENS { ... }
```

**Source:** performance_audit.md F4

---

### QW-09: Add `dev-dependencies` to Cargo.toml

| Field | Detail |
|-------|--------|
| **File** | `/root/opencodesport/rustcode/Cargo.toml` |
| **Effort** | 1 hour |
| **Risk** | None |
| **Test Strategy** | `cargo test` works with new dependencies |

**Recommended additions (workspace level):**
```toml
[workspace.dev-dependencies]
proptest = "1"
quickcheck = "1"
criterion = { version = "0.5", features = ["html_reports"] }
tokio-test = "0.4"
tempfile.workspace = true   # already in workspace deps
```

**Rationale:** Without these, property-based testing, benchmarking, and realistic IO testing are impossible.

**Source:** testing_audit.md

---

### QW-10: Replace `from_str()` with `from_slice()` in SSE parsing

| Field | Detail |
|-------|--------|
| **Files** | `crates/rustcode-core/src/providers/openai.rs:445-464`, `anthropic.rs:1026-1091` |
| **Effort** | 3-4 hours |
| **Risk** | Low |
| **Test Strategy** | Existing streaming tests pass with same output |

**Current code pattern (in both providers):**
```rust
// Creates intermediate String allocation
let text = String::from_utf8_lossy(chunk);
if let Ok(event) = serde_json::from_str::<ChatEvent>(&text) { ... }
```

**Fix:**
```rust
// Zero-copy from raw bytes
if let Ok(event) = serde_json::from_slice::<ChatEvent>(chunk) { ... }
```

**Note:** Must ensure bytes are valid UTF-8 (SSE data is always UTF-8 per spec).

**Source:** performance_audit.md F7

---

### QW-11: Add `#[cfg(test)]` to `test_db()` helper

| Field | Detail |
|-------|--------|
| **File** | `crates/rustcode-core/src/session.rs:2040-2042` |
| **Effort** | 5 minutes |
| **Risk** | None |

**Fix:**
```rust
#[cfg(test)]
fn test_db() -> Result<Arc<DatabaseService>, sqlx::Error> { ... }
```

---

### QW-12: Add clap shell completion generation

| Field | Detail |
|-------|--------|
| **File** | `src/main.rs` |
| **Effort** | 1-2 hours |
| **Risk** | None |

Add `clap_complete` dependency and a `completion` subcommand:
```rust
#[derive(clap::Args)]
struct CompletionArgs {
    shell: clap_complete::Shell,
}

// In cmd_dispatch:
Commands::Completion(args) => {
    let mut cmd = Cli::command();
    let name = cmd.get_name().to_string();
    clap_complete::generate(args.shell, &mut cmd, name, &mut std::io::stdout());
}
```

**Source:** feature_parity_matrix.md

---

### QW-13: Add JSON log format flag

| Field | Detail |
|-------|--------|
| **File** | `src/main.rs:1219-1230`, `Cargo.toml:19` |
| **Effort** | 2 hours |
| **Risk** | Low |

**Fix:** Add `--log-format (json|text)` global flag. When `json`, use:
```rust
tracing_subscriber::fmt()
    .with_env_filter(env_filter)
    .json()
    .init();
```

**Source:** production_readiness.md Finding 1.4

---

### QW-14: Define typed `CompatChatEvent` for provider SSE

| Field | Detail |
|-------|--------|
| **File** | `crates/rustcode-core/src/providers/openai_compatible.rs:90-126` |
| **Effort** | 2-4 hours |
| **Risk** | Low |
| **Test Strategy** | Verify streaming works identically with typed struct |

**Current code (untyped):**
```rust
let value: serde_json::Value = serde_json::from_str(&text)?;
let choices = value["choices"].as_array()...;
```

**Fix:** Define typed struct:
```rust
#[derive(Deserialize)]
struct CompatChatEvent {
    choices: Vec<CompatChoice>,
    usage: Option<Usage>,
    // ...
}
```

5-10x faster deserialization.

**Source:** performance_audit.md F1

### QW-11: Add `#[cfg(test)]` to `test_db()` helper

- **File:** `crates/rustcode-core/src/session.rs:2040-2042`
- **Problem:** `test_db()` is defined without `#[cfg(test)]` so it compiles into non-test builds — but uses `sqlite::memory:` which is only available in tests.
- **Effort:** 5 minutes

### QW-12: Add `--help` and shell completion generation

- **File:** `src/main.rs`
- **Problem:** clap can generate shell completions (bash, zsh, fish, powershell) but it's not wired.
- **Fix:** Add `clap_complete` dependency and a `completion` subcommand.
- **Effort:** 1-2 hours
- **Source:** feature_parity_matrix.md (completion P3)

### QW-13: Enable JSON logging via `tracing-subscriber` json feature

- **File:** `src/main.rs:1219-1230`, `Cargo.toml:19`
- **Problem:** `tracing-subscriber` json feature is declared but never used. Add `--log-format json` flag.
- **Effort:** 2 hours
- **Source:** production_readiness.md Finding 1.4

### QW-14: Fix `CompatChatEvent` untyped deserialization

- **File:** `crates/rustcode-core/src/providers/openai_compatible.rs:90-126`
- **Problem:** Uses `serde_json::Value` for SSE event deserialization (5-10x slower than typed struct). Define a typed `CompatChatEvent`.
- **Effort:** 2-4 hours
- **Source:** performance_audit.md F1

---

## 5. Medium Effort (1-3 days)

Items in this section involve structural changes to the codebase. Most require careful testing but are well-understood patterns.

---

### ME-01: Split `src/main.rs` into per-subcommand modules

| Field | Detail |
|-------|--------|
| **File** | `src/main.rs` (8,014 lines, 69 functions, 23+ subcommands) |
| **Effort** | 2-3 days |
| **Risk** | High — touches entire CLI dispatch, must not break existing commands |
| **Test Strategy** | Each new module gets its own test file. Run all 23 subcommand paths after split |

**Problem:** Single monolithic file containing CLI arg structs, command handlers, network setup, session orchestration, TUI bootstrapping, etc. Importing from 6 external crates + multiple core modules.

**Current structure of main.rs:**
```
main.rs (8,014 lines)
├── Lines 1-22:    Attributes + imports (22 lines)
├── Lines 24-243:  CLI arg structs (Cli, Commands enum, 23 subcommand args) (220 lines)
├── Lines 245-529: Arg structs for run/tui/attach/acp/serve/web/models/stats/etc.
├── Lines 530-1245: Main function, runtime setup, dispatch (715 lines)
├── Lines 1246-2400: async_main, config loading, session setup, data dir init
├── Lines 2401-3200: cmd_run, cmd_tui, cmd_serve, cmd_web handlers (800 lines)
├── Lines 3201-4800: cmd_session, cmd_providers, cmd_mcp, cmd_agent (1600 lines)
├── Lines 4801-6400: cmd_console, cmd_github, cmd_debug, cmd_import/export (1600 lines)
├── Lines 6401-7904: More handlers, test helpers (1500 lines)
└── Lines 7905-8014: Test modules (110 lines)
```

**Recommended structure:**
```
src/
├── main.rs              # ~200 lines: main() + dispatch to cli::run()
├── cli/
│   ├── mod.rs           # Re-exports all subcommand handlers
│   ├── args.rs          # All Cli/Commands/Arg structs (or split per command)
│   ├── run.rs           # cmd_run handler
│   ├── serve.rs         # cmd_serve, cmd_web
│   ├── tui.rs           # cmd_tui (also fixes C-03 nested runtime)
│   ├── session.rs       # cmd_session (list, delete, etc.)
│   ├── providers.rs     # cmd_providers (list, login, logout)
│   ├── mcp.rs           # cmd_mcp (add, list, auth, logout, debug)
│   ├── agent.rs         # cmd_agent (create, list)
│   ├── console.rs       # cmd_console (login, logout, switch, orgs, open)
│   ├── github.rs        # cmd_github (install, run)
│   ├── debug.rs         # cmd_debug (config, lsp, rg, file, scrap, etc.)
│   ├── export.rs        # cmd_export
│   ├── import.rs        # cmd_import
│   ├── stats.rs         # cmd_stats
│   ├── models.rs        # cmd_models
│   ├── upgrade.rs       # cmd_upgrade
│   ├── uninstall.rs     # cmd_uninstall
│   └── network.rs       # NetworkArgs shared struct
```

**Extraction pattern for each handler:**
```rust
// In cli/run.rs:
use clap::Args;  // re-export or own the arg struct

pub struct RunArgs { /* moved from main.rs */ }

pub async fn cmd_run(args: &RunArgs, config: &Config) -> Result<i32> {
    // ... implementation moved from main.rs
}
```

**Source:** technical_debt.md Section 9, architecture_audit.md

---

### ME-02: Split `tool_impls.rs` into per-tool modules

| Field | Detail |
|-------|--------|
| **File** | `crates/rustcode-core/src/tool_impls.rs` (5,546 lines, 198 functions) |
| **Effort** | 1-2 days |
| **Risk** | Medium — must ensure tool registry still works |
| **Test Strategy** | All existing tool tests pass after move. Run session_runner tests that exercise tool execution |

**Current structure of tool_impls.rs:**
```
tool_impls.rs (5,546 lines, 198 functions)
├── BashTool             ~700 lines  (lines 1-700)
├── ReadTool             ~500 lines  (lines 701-1200)
├── WriteTool            ~400 lines  (lines 1201-1600)
├── EditTool             ~600 lines  (lines 1601-2200)
├── GlobTool             ~300 lines  (lines 2201-2500)
├── GrepTool             ~350 lines  (lines 2501-2850)
├── WebFetchTool         ~400 lines  (lines 2851-3250)
├── WebSearchTool        ~300 lines  (lines 3251-3550)
├── ApplyPatchTool       ~500 lines  (lines 3551-4050)
├── TaskTool             ~400 lines  (lines 4051-4450)
├── QuestionTool         ~300 lines  (lines 4451-4750)
├── SkillTool            ~250 lines  (lines 4751-5000)
├── TodoWriteTool        ~200 lines  (lines 5001-5200)
├── LspTool/PlanTool     ~200 lines  (lines 5201-5400)
├── InvalidTool/Stash/etc ~146 lines (lines 5401-5546)
```

**Recommended structure:**
```
crates/rustcode-core/src/tools/
├── mod.rs           # Re-exports all tool structs + registers them
├── bash.rs
├── read.rs
├── write.rs
├── edit.rs
├── glob.rs
├── grep.rs
├── webfetch.rs
├── websearch.rs
├── task.rs
├── question.rs
├── skill.rs
├── apply_patch.rs
├── todo.rs
├── lsp.rs
├── plan.rs
└── invalid.rs
```

**Each tool module pattern:**
```rust
// tools/bash.rs
use crate::tool::{Tool, ToolContext, ExecuteResult};

pub struct BashTool;

#[async_trait]
impl Tool for BashTool {
    fn id(&self) -> &'static str { "bash" }
    fn description(&self) -> &'static str { "Execute a bash command" }

    async fn execute(&self, args: serde_json::Value, ctx: &ToolContext)
        -> crate::error::Result<ExecuteResult>
    {
        // implementation
    }
}
```

**Source:** technical_debt.md Section 9

---

### ME-03: Split `session.rs` into domain-focused modules

| Field | Detail |
|-------|--------|
| **File** | `crates/rustcode-core/src/session.rs` (3,367 lines, 107 functions) |
| **Effort** | 1-2 days |
| **Risk** | Medium — session code has wide cross-module impact |
| **Test Strategy** | All session-related tests pass after split |

**Current splinter files already exist:**
- `session_runner.rs` — core execution loop
- `session_prompt.rs` — prompt building
- `session_message.rs` — message handling
- `session_info.rs` — SessionInfo struct
- `session_todo.rs` — todo integration
- `session_history.rs` — history management
- `session_execution.rs` — execution tracking
- `session_compaction.rs` — context compaction

**But `session.rs` (the main module) still contains:**
- `SessionManager` struct + CRUD operations
- `Session` struct + lifecycle methods
- `SessionState` management
- Tool call orchestration
- Message processing pipeline
- Permission integration
- Stream/channel management

**Recommended split:**
```
crates/rustcode-core/src/session/
├── mod.rs           # Re-exports + Session / SessionManager top-level
├── manager.rs       # SessionManager CRUD
├── state.rs         # SessionState
├── lifecycle.rs     # create, fork, continue, close
├── message.rs       # Message processing
└── config.rs        # SessionConfig
```

**Source:** technical_debt.md Section 9, architecture_audit.md

---

### ME-04: Add authentication middleware to rustcode-server

| Field | Detail |
|-------|--------|
| **File** | `crates/rustcode-server/src/server.rs:136-178` |
| **Effort** | 2-3 days |
| **Risk** | High — security-critical, must not introduce bypasses |
| **Test Strategy** | Auth middleware unit tests + HTTP integration tests with `axum::test` |

**Current router (no auth):**
```rust
// server.rs:136-178
pub fn build_router(state: Arc<AppState>) -> Router {
    Router::new()
        .merge(routes::global::global_routes(state.clone()))
        .merge(routes::health::health_routes(state.clone()))
        .merge(routes::control::control_routes(state.clone()))
        .merge(routes::credential::credential_routes(state.clone()))
        // ... 30 route groups merged without any auth middleware
        .layer(cors)
}
```

**Recommended fix:**
```rust
use tower_http::auth::RequireAuthorizationLayer;
use axum::middleware;

async fn auth_middleware(
    req: Request<Body>,
    next: Next,
) -> Result<Response, StatusCode> {
    let auth_header = req.headers()
        .get("Authorization")
        .and_then(|v| v.to_str().ok())
        .and_then(|v| v.strip_prefix("Bearer "));

    match auth_header {
        Some(token) if validate_token(token) => Ok(next.run(req).await),
        _ => Err(StatusCode::UNAUTHORIZED),
    }
}

// Whitelist health endpoint from auth
pub fn build_router(state: Arc<AppState>) -> Router {
    let authed_routes = Router::new()
        .merge(routes::session::session_routes(...))
        .merge(routes::file::file_routes(...))
        .merge(routes::credential::credential_routes(...))
        .merge(routes::control::control_routes(...))
        .layer(middleware::from_fn(auth_middleware));

    let public_routes = Router::new()
        .merge(routes::health::health_routes(...));

    Router::new()
        .merge(public_routes)
        .merge(authed_routes)
        .layer(rate_limit_layer())
        .layer(cors_layer(allowed_origins))
}
```

**Also needed:**
- CORS: Replace `Any` with configured origin whitelist (from `NetworkArgs.cors`)
- Rate limiting: Add `tower-governor` or `tower` rate limit layer
- TLS: Use `axum-server` with `rustls` or document reverse proxy requirement

**Source:** security_audit.md Section 2

---

### ME-05: Add file path sanitization to server routes

| Field | Detail |
|-------|--------|
| **Files** | `crates/rustcode-server/src/routes/file.rs:287-288`, `session.rs:1251-1253` |
| **Effort** | 4-6 hours |
| **Risk** | High — must prevent breakage of legitimate file access |
| **Test Strategy** | Integration tests with path traversal payloads (`../../etc/passwd`) |

**Vulnerable code pattern (file.rs:287-288):**
```rust
pub path: String,
// joined with directory without sanitization
let full_path = directory.join(&path);
```

**Recommended fix:**
```rust
fn sanitize_path(user_path: &Path, allowed_base: &Path) -> Result<PathBuf, ServerError> {
    let canonical = user_path.canonicalize()
        .map_err(|_| ServerError::PathTraversal("Cannot resolve path".into()))?;
    if !canonical.starts_with(allowed_base) {
        return Err(ServerError::PathTraversal("Path outside allowed directory".into()));
    }
    Ok(canonical)
}
```

**Routes to sanitize:**
- `file.rs` — list_files (line 287), read_file (line 333), write_file
- `session.rs` — post_shell workdir (line 1251)
- All tool implementations: ReadTool, WriteTool, EditTool, ApplyPatchTool, BashTool workdir

**Source:** security_audit.md Section 4

---

### ME-06: Replace `std::sync::Mutex` with `tokio::sync::Mutex` in async hot paths

| Field | Detail |
|-------|--------|
| **Files** | `config.rs:32`, `id.rs:18`, `snapshot.rs:15`, `workspace.rs:366` |
| **Effort** | 1-2 days |
| **Risk** | Medium — tokio::sync::Mutex is slightly slower for uncontended locks but safe in async |
| **Test Strategy** | All tests pass, no regression in hot-path benchmarks |

**Current state of sync primitives in async context:**

| File | Current | Problem | Recommended |
|------|---------|---------|-------------|
| `config.rs:32` | `std::sync::RwLock` | Every async path reads config, blocks worker thread | `tokio::sync::RwLock` or keep if never held across `.await` (audit needed) |
| `id.rs:18` | `std::sync::Mutex` | Called from EVERY async path (every message, event, session) | `std::sync::atomic::AtomicU64` (lock-free) |
| `snapshot.rs:15` | `std::sync::Mutex` | Snapshot operations | `tokio::sync::Mutex` |
| `workspace.rs:366` | `std::sync::Mutex` | Worktree operations | `tokio::sync::Mutex` |

**Fix for id.rs (hot-path, best as atomic):**
```rust
use std::sync::atomic::{AtomicU64, Ordering};

static COUNTER: AtomicU64 = AtomicU64::new(0);

pub fn ascending() -> String {
    let count = COUNTER.fetch_add(1, Ordering::Relaxed);
    let timestamp = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis() as u64;
    format!("{:016x}{:016x}", timestamp, count)
}
```

**Source:** concurrency_audit.md Finding #5

---

### ME-07: Eliminate crate-level `#![allow(dead_code, unused_imports)]`

| Field | Detail |
|-------|--------|
| **Files** | `crates/rustcode-core/src/lib.rs:2`, `src/main.rs:2` |
| **Effort** | 1-2 days |
| **Risk** | Medium — will initially produce hundreds of warnings to fix |
| **Test Strategy** | `cargo build` produces zero dead_code or unused_imports warnings |

**Current code:**
```rust
// lib.rs:2
#![allow(dead_code, unused_imports, unused_variables)]

// main.rs:2
#![allow(dead_code, unused_imports)]
```

**Phased removal approach:**
1. Change `allow` to `warn` (not `deny` yet) to see all warnings
2. For intentionally scaffold items: add `#[allow(dead_code)]` on specific items
3. For items that should exist but aren't wired: add `#[expect(dead_code)]` (Rust 2024) or `#[allow(dead_code)]` with a TODO comment
4. Remove truly dead code (delete it)
5. Gate test-only items with `#[cfg(test)]`
6. Change to `deny` once clean

**Example fixes required (estimated):**
- ~50-100 individual `#[allow(dead_code)]` annotations for scaffold types
- ~10-20 removed items (truly dead)
- ~20-30 `#[cfg(test)]` additions for test-only functions

**Source:** logic_audit.md H-03

---

### ME-08: Add file-appender logging with rotation

| Field | Detail |
|-------|--------|
| **File** | `src/main.rs:1219-1230` |
| **Effort** | 4 hours |
| **Risk** | Low |
| **Test Strategy** | Verify log files created in expected directory, rotation works |

**Current:** Only stderr logging, no file output.

**Recommended:**
```toml
# Cargo.toml
tracing-appender = "0.2"
```

```rust
use tracing_appender::rolling;

let file_appender = rolling::daily(
    dirs::data_dir().unwrap().join("opencode/log"),
    "rustcode.log",
);
let (non_blocking, _guard) = tracing_appender::non_blocking(file_appender);

tracing_subscriber::fmt()
    .with_env_filter(env_filter)
    .with_writer(non_blocking)
    .with_target(false)
    .init();
```

**Source:** production_readiness.md Finding 1.1

---

### ME-09: Fix N+1 query in `get_messages_with_parts()`

| Field | Detail |
|-------|--------|
| **File** | `crates/rustcode-core/src/database.rs:1424-1438` |
| **Effort** | 2-3 hours |
| **Risk** | Low |
| **Test Strategy** | Verify session load returns same data with 1 query instead of N+1 |

**Current code:**
```rust
// database.rs:1424-1438
pub async fn get_messages_with_parts(&self, session_id: &str) -> Result<Vec<Message>> {
    let messages = self.list_messages(session_id).await?;        // 1 query
    for msg in &mut messages {
        let parts = self.list_parts(&msg.id).await?;             // N queries
        msg.parts = parts;
    }
    Ok(messages)
}
```

**Recommended fix:**
```rust
pub async fn get_messages_with_parts(&self, session_id: &str) -> Result<Vec<Message>> {
    // Single JOIN query
    let rows = sqlx::query_as::<_, MessageWithPartRow>(
        "SELECT m.*, p.id as part_id, p.content as part_content, p.type as part_type
         FROM message m
         LEFT JOIN part p ON m.id = p.message_id
         WHERE m.session_id = ?1
         ORDER BY m.created_at ASC, p.order ASC"
    )
    .bind(session_id)
    .fetch_all(&self.pool)
    .await?;

    // Group parts into messages in Rust
    // ...
}
```

**Alternative (batch fetch):**
```rust
pub async fn get_messages_with_parts(&self, session_id: &str) -> Result<Vec<Message>> {
    let messages = self.list_messages(session_id).await?;
    let message_ids: Vec<String> = messages.iter().map(|m| m.id.clone()).collect();

    // Single query for all parts
    let all_parts = sqlx::query_as::<_, Part>(
        "SELECT * FROM part WHERE message_id IN (SELECT id FROM message WHERE session_id = ?1)"
    )
    .bind(session_id)
    .fetch_all(&self.pool)
    .await?;

    // Group by message_id
    // ...
}
```

**Source:** performance_audit.md F2

---

### ME-10: Add `JoinSet` / structured concurrency for spawns

| Field | Detail |
|-------|--------|
| **Files** | `process.rs:570,601`, `background_job.rs:348,411,1208`, TUI `app.rs` (19 spawns) |
| **Effort** | 2-3 days |
| **Risk** | Medium — changes lifecycle of spawned tasks |
| **Test Strategy** | All spawned task scenarios work, graceful shutdown works |

**Current fire-and-forget pattern (process.rs:570):**
```rust
tokio::spawn(async move {
    // stdout reader
    while let Some(line) = reader.next_line().await.unwrap_or(None) {
        let _ = tx.send(line);
    }
}); // JoinHandle discarded
```

**Recommended pattern:**
```rust
// Use JoinSet for structured concurrency
let mut join_set = tokio::task::JoinSet::new();

join_set.spawn(async move {
    while let Some(line) = reader.next_line().await.unwrap_or(None) {
        if tx.send(line).is_err() { break; }
    }
});

// Store JoinSet for later cleanup or wait
// On shutdown:
join_set.shutdown().await;
```

**TUI-specific:** Extract spawn sites from anonymous closures into named async functions, add `CancellationToken` for coordinated shutdown.

**Source:** concurrency_audit.md Findings #3, #6

---

### ME-11: Replace unbounded channels with bounded channels

| Field | Detail |
|-------|--------|
| **Files** | TUI `app.rs:412,461,477`, `mcp.rs:1467` |
| **Effort** | 3-5 days |
| **Risk** | Medium — must handle backpressure correctly (dropped events vs blocking) |
| **Test Strategy** | Stress test: flood events, verify backpressure works, no OOM |

**Current unbounded channels:**
```rust
// app.rs:411-412 — crossterm events
let (event_tx, mut event_rx) = unbounded_channel::<crossterm::event::Event>();

// app.rs:460-461 — bus events
let (bus_tx, local_bus_rx) = unbounded_channel::<GlobalEvent>();

// app.rs:477-480 — LLM events
let (llm_tx, local_llm_rx) = unbounded_channel::<(String, LlmEvent)>();

// mcp.rs:1466-1467 — SSE events
let (sse_tx, sse_rx) = unbounded_channel::<(u64, serde_json::Value)>();
```

**Fix for each:**

| Channel | Strategy | Capacity | Drop Policy |
|---------|----------|----------|-------------|
| crossterm events | `channel(256)` + `try_send` | 256 | Safe to drop old input events |
| bus events | `channel(1024)` + `await` send | 1024 | Must not drop — apply backpressure |
| LLM events | `channel(512)` + `try_send` | 512 | Drop old events (best-effort display) |
| SSE events | `channel(1024)` + `await` send | 1024 | Must not drop — apply backpressure |

**Source:** concurrency_audit.md Finding #4

---

### ME-12: Fix `auto_detect_all()` lazy initialization

| Field | Detail |
|-------|--------|
| **File** | `crates/rustcode-core/src/providers/mod.rs:54-145` |
| **Effort** | 2-3 hours |
| **Risk** | Low |
| **Test Strategy** | Startup time benchmark, verify provider discovery still works |

**Current code (creates all providers eagerly):**
```rust
// providers/mod.rs:54-145
pub fn auto_detect_all() -> HashMap<String, Box<dyn Provider>> {
    let mut providers = HashMap::new();
    providers.insert("anthropic".into(), Box::new(AnthropicProvider::new()));
    providers.insert("openai".into(), Box::new(OpenAIProvider::new()));
    providers.insert("gemini".into(), Box::new(GeminiProvider::new()));
    // ... 20+ more providers created eagerly
}
```

**Recommended fix:**
```rust
pub struct LazyProviderCatalog {
    providers: Arc<RwLock<HashMap<String, Box<dyn Provider>>>>,
    client: reqwest::Client,  // shared HTTP client
}

impl LazyProviderCatalog {
    pub fn get(&self, name: &str) -> Option<Box<dyn Provider>> {
        let mut map = self.providers.write().unwrap();
        if let Some(provider) = map.get(name) {
            return Some(provider.clone()); // or Arc-wrapped
        }
        let provider = self.create_provider(name)?;
        map.insert(name.to_string(), provider.clone());
        Some(provider)
    }
}
```

**Source:** performance_audit.md F8

---

### ME-13: Reduce cloning in provider message building

| Field | Detail |
|-------|--------|
| **Files** | `openai.rs:205-281`, `anthropic.rs:282-352` |
| **Effort** | 4-6 hours |
| **Risk** | Medium — must not break message formatting |
| **Test Strategy** | All provider streaming tests pass |

**Current cloning-heavy pattern:**
```rust
// openai.rs:205-281 (similar pattern in all providers)
fn build_chat_messages(messages: &[ChatMessage]) -> Vec<OpenAIMessage> {
    messages.iter().map(|msg| {
        OpenAIMessage {
            role: msg.role.clone(),       // String clone
            content: msg.content.clone(),  // String clone
            tool_calls: msg.tool_calls.clone(), // Vec clone
        }
    }).collect()
}
```

**Recommended fix:**
```rust
fn build_chat_messages(messages: &[ChatMessage]) -> Vec<OpenAIMessage<'_>> {
    messages.iter().map(|msg| {
        OpenAIMessage {
            role: &msg.role,              // &str reference
            content: msg.content.as_str(), // &str reference
            tool_calls: msg.tool_calls.as_ref().map(|v| v.as_slice()),
        }
    }).collect()
}
```

**Source:** performance_audit.md F3

---

### ME-14: Implement basic integration tests

| Field | Detail |
|-------|--------|
| **Files** | New `tests/` directories across crates |
| **Effort** | 2-3 days |
| **Risk** | Low — new code, no existing behavior changed |
| **Test Strategy** | These ARE tests — they validate existing behavior |

**Must-have integration tests:**

| Test | File Location | What It Validates |
|------|---------------|-------------------|
| Database CRUD | `crates/rustcode-core/tests/database.rs` | Session/message/part CRUD, migrations |
| Session runner | `crates/rustcode-core/tests/session_runner.rs` | Full tool execute loop with mock |
| Permission eval | `crates/rustcode-core/tests/permission.rs` | Rule evaluation ordering, wildcard match |
| Tool execution | `crates/rustcode-core/tests/tools.rs` | Read, Write, Glob, Grep on temp dir |
| HTTP health | `crates/rustcode-server/tests/health.rs` | `/health` endpoint returns 200 |
| Auth middleware | `crates/rustcode-server/tests/auth.rs` | Unauthed requests rejected, authed accepted |

**Example database integration test:**
```rust
// tests/database.rs
use tempfile::TempDir;

#[tokio::test]
async fn test_session_crud() {
    let temp = TempDir::new().unwrap();
    let db_url = format!("sqlite:{}?mode=memory", temp.path().join("test.db").display());
    let pool = SqlitePoolOptions::new()
        .max_connections(1)
        .connect(&db_url)
        .await
        .unwrap();

    let db = DatabaseService::new(pool);
    db.run_migrations().await.unwrap();

    let session = db.create_session("proj_1").await.unwrap();
    assert_eq!(session.project_id, "proj_1");

    let loaded = db.get_session(&session.id).await.unwrap();
    assert!(loaded.is_some());
    assert_eq!(loaded.unwrap().id, session.id);
}
```

**Source:** testing_audit.md

---

### ME-15: Add external documentation files

| Field | Detail |
|-------|--------|
| **Files** | `README.md`, `CONTRIBUTING.md`, `CHANGELOG.md` (at workspace root) |
| **Effort** | 4-6 hours |
| **Risk** | None |

**README.md should cover:**
- What is RustCode? (Rust port of OpenCode)
- Quick start: `cargo run -- run "hello"`
- Prerequisites (Rust toolchain, SQLite)
- Configuration (environment variables, config file location)
- CLI usage overview (23 subcommands)
- Provider setup (API keys)
- Project status (which features are complete)

**CONTRIBUTING.md should cover:**
- Dev setup (clone, build, test)
- Coding standards (follow CLAUDE.md)
- Lint policy (`#![allow]` philosophy)
- PR process
- CI expectations (fmt, clippy, test must pass)

**Source:** documentation_audit.md

---

### ME-16: Add API key encryption at rest

| Field | Detail |
|-------|--------|
| **File** | `crates/rustcode-core/src/credential.rs`, `control.rs:63-73` |
| **Effort** | 2-3 days |
| **Risk** | High — encryption must be correct or keys are lost |
| **Test Strategy** | Unit test: encrypt -> persist -> load -> decrypt -> verify match |

**Current plaintext storage:**
```rust
// control.rs:63-73
let mut creds = serde_json::json!({
    "type": "api_key",
    "key": payload.key,   // plaintext in JSON file
});
Config::save_auth(&provider_id, &creds);
```

**Recommended:** Use `aes-gcm` + `zeroize`:
```toml
aes-gcm = "0.10"
zeroize = { version = "1", features = ["derive"] }
```

```rust
use aes_gcm::{Aes256Gcm, KeyInit, Nonce};
use zeroize::Zeroize;

pub struct EncryptedCredential {
    ciphertext: Vec<u8>,
    nonce: Vec<u8>,
}

impl EncryptedCredential {
    pub fn encrypt(api_key: &str, password: &[u8]) -> Result<Self> {
        let key = derive_key(password);  // Argon2 or PBKDF2
        let cipher = Aes256Gcm::new_from_slice(&key)?;
        let nonce = generate_random_nonce();
        let ciphertext = cipher.encrypt(&nonce, api_key.as_bytes())?;
        Ok(Self { ciphertext, nonce })
    }

    pub fn decrypt(&self, password: &[u8]) -> Result<String> { ... }
}

// Zeroize on drop
impl Drop for ApiKey {
    fn drop(&mut self) {
        self.key.zeroize();
    }
}
```

**Source:** security_audit.md Section 1

---

### ME-17: Add API key isolation from subprocesses

| Field | Detail |
|-------|--------|
| **File** | `control.rs:58-59`, `credential.rs` |
| **Effort** | 1-2 days |
| **Risk** | Medium — all provider authentication patterns must be updated |
| **Test Strategy** | Verify child processes cannot read API keys from env |

**Current (injects into process env):**
```rust
// control.rs:58-59
let key_env = format!("{}_API_KEY", provider_id.to_uppercase());
std::env::set_var(&key_env, &payload.key);
```

**Problems:** 
- All child processes (LSP, MCP, shell) inherit these env vars
- `/proc/self/environ` readable by any process on same machine
- Provider API keys exposed to every subprocess

**Recommended:** Use a secure in-memory key store, scoped to request context:
```rust
pub struct SecureKeyStore {
    keys: Arc<RwLock<HashMap<String, Zeroizing<String>>>>,
}

impl SecureKeyStore {
    pub fn set(&self, provider: &str, key: Zeroizing<String>) {
        self.keys.write().unwrap().insert(provider.to_string(), key);
    }

    pub fn get(&self, provider: &str) -> Option<Zeroizing<String>> {
        self.keys.read().unwrap().get(provider).cloned()
    }
}

// Pass through ProviderContext instead of env vars
pub struct ProviderContext {
    api_key: Zeroizing<String>,
    // ...
}
```

**Source:** security_audit.md Section 1.1

---

### ME-18: Add `truncate_output()` optimization

| Field | Detail |
|-------|--------|
| **File** | `crates/rustcode-core/src/tool.rs:482` |
| **Effort** | 1 hour |
| **Risk** | None |
| **Test Strategy** | Existing truncation tests pass |

**Current:**
```rust
// tool.rs:482
if output.len() > MAX_OUTPUT_LEN {
    output.truncate(MAX_OUTPUT_LEN);
}
// Later:
let char_count = output.chars().count(); // O(n) for ASCII, O(n) for Unicode
```

**Fix:**
```rust
if output.len() > MAX_OUTPUT_LEN {
    // Find last valid char boundary within limit
    let mut idx = MAX_OUTPUT_LEN;
    while idx > 0 && !output.is_char_boundary(idx) {
        idx -= 1;
    }
    output.truncate(idx);
}
```

**Source:** performance_audit.md F10

---

## 6. Major Redesign (1-4 weeks)

### MR-01: Split `rustcode-core` into domain-specific crates

- **Files:** Full restructuring of `crates/rustcode-core/` (70 modules, ~77K lines)
- **Problem:** ALL domain logic lives in a single monolithic crate. Tight coupling prevents independent versioning, testing, and compilation.
- **Recommendation (target structure):**
  ```
  crates/
  ├── rustcode-core/           # Remaining: bus, error, id, config, storage
  ├── rustcode-providers/      # All provider implementations (moved from core/providers/)
  ├── rustcode-tools/          # All tool implementations (moved from core/tool_impls.rs)
  ├── rustcode-session/        # Session management (moved from core/session*.rs)
  ├── rustcode-events/         # Event sourcing (moved from core/event.rs)
  ├── rustcode-permissions/    # Permission model (moved from core/permission.rs)
  ├── rustcode-database/       # Database schema + migrations (moved from core/database.rs)
  ├── rustcode-server/         # Existing — HTTP/SSE server
  ├── rustcode-tui/            # Existing — Terminal UI
  ├── rustcode-lsp/            # Existing — LSP integration
  └── rustcode-mcp/            # Existing — MCP protocol
  ```
- This mirrors the TS monorepo structure (25 packages) but is a controlled decomposition.
- **Benefits:**
  - Parallel compilation (significant build time reduction)
  - Independent testing per crate
  - Clear dependency graph (no circular imports)
  - Reusable provider/tool crates for other projects
- **Effort:** 2-3 weeks

### MR-02: Implement parallel tool execution

- **Files:** `crates/rustcode-core/src/session.rs:1533-1544`, `session_runner.rs:299-346`
- **Problem:** Tool calls from LLM are executed sequentially in a for loop. The TS source uses `Effect.all()` for concurrent execution.
- **Recommendation:**
  1. Group independent tool calls (no data dependency) for concurrent execution
  2. Use `futures::future::join_all()` or `FuturesUnordered` for the parallel group
  3. Add semaphore to limit concurrent tool executions
  4. Return results in order (maintaining position for response assembly)
- **Effort:** 3-5 days
- **Source:** concurrency_audit.md Finding #7

### MR-03: Add metrics, structured logging, and health probes

- **Files:** New infrastructure across `rustcode-server/`, `rustcode-core/src/observability.rs`
- **Problem:** No metrics (Prometheus), no structured JSON logging, no liveness/readiness probes, no circuit breakers, no panic recovery.
- **Recommendation:**
  1. **Metrics:** Add `metrics` + `metrics-exporter-prometheus` crate family. Export at `/metrics` endpoint.
  2. **Structured logging:** Wire JSON output in production mode via `tracing-subscriber::fmt().json()`
  3. **Health probes:** Separate `/livez` (basic liveness) and `/readyz` (db connected, providers available) endpoints
  4. **Circuit breakers:** Add `failsafe` or `tower` circuit breaker middleware for provider API calls
  5. **Panic recovery:** Add `std::panic::set_hook` that logs panic details and exits gracefully
  6. **Resource limits:** Add `tokio::sync::Semaphore` for database connections, tool executions, concurrent sessions
- **Effort:** 1-2 weeks
- **Source:** production_readiness.md (entire report)

### MR-04: Add authentication to all WebSocket/SSE routes

- **File:** `crates/rustcode-server/src/routes/event.rs:146`
- **Problem:** SSE event streams are unauthenticated. Any client can subscribe to all session events.
- **Recommendation:**
  1. Add auth middleware for SSE endpoint (token in query params or initial HTTP request)
  2. Validate session ownership on event subscription
  3. Add token-based auth for all session routes
- **Effort:** 3-5 days
- **Source:** security_audit.md Section 2

### MR-05: Implement event sourcing persistence layer

- **File:** `crates/rustcode-core/src/event.rs:734-766` (critical C-05)
- **Problem:** Event sourcing is completely non-functional — events are broadcast in-memory but never persisted to `event` and `event_sequence` tables.
- **Fix:** Full implementation of:
  1. Aggregate sequence number assignment via `event_sequence` table
  2. Commit guard validation
  3. INSERT into `event` table
  4. Sync handler invocation + projector execution
  5. Then in-memory broadcast
- **Effort:** 3-5 days

### MR-06: Implement remaining 11 missing provider integrations

- **Missing providers (from TS):**
  - Google Vertex AI, DeepInfra, Cloudflare AI Gateway, Cloudflare Workers AI, GitLab AI, NVIDIA, Vercel, Alibaba, SAP AI Core, Snowflake Cortex, Venice
- **Also missing (from TS):**
  - Dynamic provider loading, Gateway provider, Kilo, LLM Gateway, OpenCode Console provider, ZenMux, OpenAI Auth (OAuth)
- **Effort:** 2-3 weeks (assuming 2-4 hours per provider, with testing)

### MR-07: Implement `ToolContext` borrowing instead of cloning

- **File:** `crates/rustcode-core/src/tool.rs:432-438`
- **Problem:** `PluginToolAdapter::execute` clones the entire `ToolContext` (~200+ byte struct with HashMap and Vec) on every tool invocation. 10-20 clones per user message in hot path.
- **Fix:** Change `execute` signature from `ctx: &ToolContext`-with-clone to `ctx: &ToolContext` with an `Rc<RefCell<...>>` or `Arc<RwLock<...>>` for the mutable parts. Or restructure ToolContext into immutable (Arc-backed) + mutable (separate struct) parts.
- **Effort:** 3-5 days
- **Source:** memory_audit.md Section 3.1

### MR-08: Add property-based and fuzz testing

- **Files:** New test files across crates
- **Problem:** Zero property-based tests (`proptest`, `quickcheck`), zero fuzz tests (`cargo-fuzz`).
- **Recommendation:**
  1. Add `proptest` as dev-dependency
  2. Write property tests for:
     - Permission wildcard matching (random patterns + paths)
     - Config merge/rebase idempotence
     - Session ID generation (uniqueness, ordering)
     - Tool output truncation (char-boundary safety)
     - Database CRUD (insert-all, read-back, delete-idempotent)
  3. Add `cargo-fuzz` target for:
     - SSE event stream parsing
     - Tool argument JSON parsing
     - Config file parsing
     - Permission rule parsing
- **Effort:** 1-2 weeks

### MR-09: Add panic-free error handling (eliminate unwrap/expect)

- **Files:** All crates (744 unwrap + 1295 expect = 2039 panic call sites)
- **Problem:** Every `.unwrap()` and `.expect()` is a potential panic in production. Library code should propagate errors via `?`.
- **Recommendation:**
  1. Audit and categorize all 2039 sites:
     - Infallible operations (e.g., `Mutex::lock().unwrap()` when poisoned = abort anyway) — keep but document
     - Test code — can keep if `#[cfg(test)]` gated
     - Library code — MUST replace with `?`, `.ok_or()`, or `.context()`
     - Binary/main.rs — use `.expect("reason")` with clear messages
  2. Add `clippy::unwrap_used` and `clippy::expect_used` to warn on new unwrap calls
  3. Create a migration checklist per module, track progress
- **Effort:** 1-2 weeks (systematic per-module)
- **Source:** technical_debt.md Section 6

### MR-10: Eliminate 18 near-identical provider implementations

- **Files:** All files in `crates/rustcode-core/src/providers/`
- **Problem:** 18 provider files share 85%+ duplication in streaming, non-streaming, model listing, and event parsing code.
- **Recommendation:**
  1. Extract shared HTTP client, SSE parser, retry logic, and response transformation into a `ProviderBase` trait or struct
  2. Implement a `RestProvider` abstraction:
     ```rust
     trait RestProvider {
         fn base_url(&self) -> &str;
         fn headers(&self) -> HeaderMap;
         fn deserialize_response(&self, body: &[u8]) -> Result<ProviderResponse>;
         fn deserialize_stream_event(&self, event: &[u8]) -> Result<LlmEvent>;
     }
     ```
  3. Each specific provider only implements the custom parts (URL, auth, response format)
  4. This also enables the OpenAI-compatible provider to be a simple config override
- **Effort:** 1-2 weeks

### MR-11: Add LSP and MCP security sandboxing

- **Files:** `crates/rustcode-lsp/src/lib.rs`, `crates/rustcode-core/src/mcp.rs`
- **Problem:** LSP and MCP servers run as subprocesses with access to all environment variables (including API keys set via `std::env::set_var()`).
- **Recommendation:**
  1. Clear or isolate environment variables before spawning LSP/MCP subprocesses
  2. Implement path allowlists for LSP workspace folders
  3. Add resource limits (CPU, memory, file descriptors) via `rlimit` on Linux
  4. Implement timeout-based cancellation for MCP operations
- **Effort:** 3-5 days
- **Source:** security_audit.md Section 5/6

### MR-12: Implement proper TUI architecture (remove nested runtime)

- **File:** `crates/rustcode-tui/src/app.rs` (3,236 lines) + `src/main.rs:2394-2396`
- **Problem:** Nested tokio runtime, 19 fire-and-forget spawns, unbounded channels, and monolithic App struct.
- **Recommendation:**
  1. Use `tokio::task::LocalSet` for the TUI render loop (which requires `!Send` futures for crossterm)
  2. Replace unbounded channels with bounded channels + backpressure
  3. Extract event handlers into separate modules
  4. Use structured concurrency with `JoinSet` for background tasks
  5. Add `CancellationToken` for clean shutdown
- **Effort:** 1-2 weeks

### MR-13: Add CI improvements

- **Files:** `.github/workflows/ci.yml`
- **Problem:** CI only runs fmt, clippy, test, cargo-deny. Missing:
  - `cargo audit` for security vulnerabilities
  - `cargo doc` for documentation generation
  - `cargo outdated` for dependency freshness
  - Integration test step
  - Cross-compilation checks
  - Benchmark regression detection
- **Effort:** 2-3 days

### MR-14: Reduce build times

- **Files:** `Cargo.toml` workspace dependencies
- **Problem:** Rust compilation is slow (~5-10 min for clean build). OpenCode's TS monorepo uses Turborepo caching for ~5s incremental builds.
- **Recommendation:**
  1. Enable `lld` linker for faster linking
  2. Use `mold` linker in CI
  3. Add `codegen-units = 1` for release builds (smaller binary, faster runtime)
  4. Split debug info into separate files
  5. Consider `cargo-chef` for Docker layer caching
  6. Use GitHub Actions `Swatinem/rust-cache` (already present, optimize)
- **Effort:** 1-2 days

---

## 7. Implementation Roadmap

### Phase 1: Firefighting (Week 1)

| Order | Item | Effort | Priority |
|-------|------|--------|----------|
| 1 | C-01: `watch::channel::<false>` | 5 min | P0 |
| 2 | C-02: `connect_lazy()` Result | 5 min | P0 |
| 3 | C-03: Double tokio runtime in TUI | 1-2h | P0 |
| 4 | C-04: `ask()` missing pending insert | 15 min | P0 |
| 5 | C-06: `listen()` unsubscribe no-op | 2-4h | P0 |
| 6 | C-08: Session ID format in CLI | 2-3h | P0 |
| 7 | QW-02: Default log level to WARN | 30 min | P0 |
| 8 | QW-05: Add `#[forbid(unsafe_code)]` | 10 min | P0 |
| 9 | QW-11: `#[cfg(test)]` for test_db() | 5 min | P0 |
| 10 | QW-12: Shell completion | 1-2h | P2 |

**Total: ~8-12 hours**

### Phase 2: Quick Wins (Week 2)

| Order | Item | Effort | Priority |
|-------|------|--------|----------|
| 11 | ME-07: Remove `#![allow(dead_code)]` | 1-2d | P1 |
| 12 | QW-01: Empty project_id in permission | 1h | P1 |
| 13 | QW-06: Session list SQL filter pushdown | 2-3h | P1 |
| 14 | QW-07: Cache ToolDefinitions | 2-3h | P2 |
| 15 | QW-14: Typed CompatChatEvent | 2-4h | P2 |
| 16 | QW-04: Doom loop Value comparison | 30 min | P2 |
| 17 | QW-08: Context overflow running estimate | 1-2h | P2 |
| 18 | QW-03: Wire ObservabilityService::init() | 1-2h | P1 |
| 19 | QW-13: JSON logging format flag | 2h | P2 |
| 20 | QW-09: Add dev-dependencies | 1h | P1 |
| 21 | QW-10: SSE from_slice | 3-4h | P2 |
| 22 | ME-09: N+1 query fix | 2-3h | P1 |
| 23 | ME-12: Lazy provider init | 2-3h | P2 |
| 24 | ME-15: External docs (README, etc.) | 4-6h | P1 |
| 25 | ME-13: Reduce provider cloning | 4-6h | P2 |
| 26 | ME-18: truncate_output optimization | 1h | P3 |

**Total: ~35-50 hours (1 week)**

### Phase 3: Medium Structural Changes (Weeks 3-4)

| Order | Item | Effort | Priority |
|-------|------|--------|----------|
| 27 | ME-01: Split main.rs | 2-3d | P0 |
| 28 | ME-02: Split tool_impls.rs | 1-2d | P0 |
| 29 | ME-03: Split session.rs | 1-2d | P1 |
| 30 | ME-06: std::sync::Mutex → tokio in async | 1-2d | P1 |
| 31 | ME-04: Add auth middleware to server | 2-3d | P0 |
| 32 | ME-05: File path sanitization | 4-6h | P0 |
| 33 | ME-08: File-appender logging | 4h | P1 |
| 34 | ME-11: Bounded channels | 3-5d | P2 |
| 35 | ME-10: JoinSet for spawns | 2-3d | P2 |
| 36 | ME-14: Integration tests | 2-3d | P1 |
| 37 | ME-16: API key encryption | 2-3d | P1 |
| 38 | ME-17: API key subprocess isolation | 1-2d | P1 |

**Total: ~18-30 days (2 weeks)**

### Phase 4: Major Redesign (Weeks 5-8)

| Order | Item | Effort | Priority |
|-------|------|--------|----------|
| 39 | MR-01: Split rustcode-core into domain crates | 2-3w | P1 |
| 40 | MR-03: Metrics + structured logging + probes | 1-2w | P1 |
| 41 | MR-05: Event sourcing persistence | 3-5d | P0 |
| 42 | MR-02: Parallel tool execution | 3-5d | P2 |
| 43 | MR-09: Eliminate unwrap/expect | 1-2w | P1 |
| 44 | MR-10: Reduce provider duplication | 1-2w | P2 |
| 45 | MR-07: ToolContext borrowing | 3-5d | P3 |
| 46 | MR-06: Missing provider integrations | 2-3w | P2 |
| 47 | MR-12: TUI architecture fix | 1-2w | P2 |
| 48 | MR-04: WebSocket/SSE auth | 3-5d | P0 |
| 49 | MR-08: Property + fuzz tests | 1-2w | P2 |
| 50 | MR-11: LSP/MCP sandboxing | 3-5d | P1 |
| 51 | MR-13: CI improvements | 2-3d | P2 |
| 52 | MR-14: Build time reduction | 1-2d | P3 |

**Total: ~12-20 weeks (4-5 weeks)**

### Effort Summary

| Category | Count | Estimated Total Effort |
|----------|-------|----------------------|
| 🔴 Critical (compile-blockers) | 8 | ~3-4 days |
| 🟢 Quick Wins (< 4h) | 14 | ~1-2 days |
| 🟡 Medium Effort (1-3d) | 15 | ~15-25 days |
| 🔵 Major Redesign (1-4w) | 14 | ~12-20 weeks |
| **Total** | **51** | **~16-27 weeks** |

---

## Appendix: Audit Report Sources

All findings reference reports available in `/root/opencodesport/rustcode/reports/`:

| Report | File | Key Findings Used |
|--------|------|-------------------|
| Architecture | `architecture_audit.md` | Workspace design, modularity gaps |
| Concurrency | `concurrency_audit.md` | Nested runtime, unbounded tasks, sync mutex in async |
| Security | `security_audit.md` | No auth, plaintext keys, command injection, path traversal |
| Logic | `logic_audit.md` | Compile errors, missing persistence, memory leaks |
| Feature Parity | `feature_parity_matrix.md` | Missing providers, missing CLI features |
| Production Readiness | `production_readiness.md` | No metrics, no file logging, no probes |
| Performance | `performance_audit.md` | N+1 queries, serde_json::Value, cloning hot paths |
| Technical Debt | `technical_debt.md` | 4200+ debt markers, god files, unwrap count |
| Testing | `testing_audit.md` | Zero integration/E2E/fuzz tests |
| Memory & Ownership | `memory_audit.md` | ToolContext cloned per call, 100+ clone sites |
| DevEx | `devex_audit.md` | 3.5/10 score, no hot reload, no pre-commit |
| Documentation | `documentation_audit.md` | Great inline docs (10.2%), zero external docs |
| Protocol | `protocol_audit.md` | MCP/LSP protocol compliance |
| Database | `database_audit.md` | Schema design, migration patterns |
| API | `api_audit.md` | HTTP API surface analysis |
| OpenCode Gap | `opencode_gap_analysis.md` | High-level TS vs Rust comparison |
| Architecture Supplement | `architecture_audit_supplement.md` | Deep dive on specific architectural decisions |
