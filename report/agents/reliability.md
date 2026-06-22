# Reliability Analysis: BlazeCode vs BlazeCode

**Agent 16 — Reliability Agent**

---

## 1. Failure Modes

### 1.1 SQLite Database Failure Modes

- **Location**: `crates/blazecode-core/src/database.rs:1280-1350`, `crates/blazecode-core/src/storage.rs:658-697`
- **BlazeCode**: Effect's structured concurrency ensures DB operations are wrapped in Effect.`tryPromise`. Transaction failures are caught at the Effect boundary with typed errors (`DatabaseError`). WAL checkpoint failures are logged but non-fatal.
- **BlazeCode**: Raw `sqlx::query(...).execute()` calls with stringly-typed SQL. The `INSERT INTO session` query at `database.rs:1254` uses 16 positional params — if the table schema diverges between the SQL constant and the actual migration, this causes a runtime crash. No compile-time query validation (sqlx `query!` macro is not used).
- **Gap**: BlazeCode uses `sqlx::query` (unchecked) instead of `sqlx::query!` (compile-time checked). Any schema migration that adds a column without updating the corresponding INSERT causes a hard panic at runtime.
- **Consequence**: Schema drift between migration and hand-written SQL → runtime crash on first INSERT.
- **Recommendation**: Use `sqlx::query!` with compile-time checking, or add integration tests that verify all hand-written SQL against the actual schema.
- **Severity**: **Critical**

### 1.2 Event Publication Failure

- **Location**: `crates/blazecode-core/src/event.rs:855-1063`
- **BlazeCode**: Event publication goes through Effect's structured pipeline: `validate → guard → persist → project → notify`. Each step returns typed errors. The pipeline is transactional — if any step fails, the entire publish is rolled back.
- **BlazeCode**: The `publish` method at `event.rs:855` runs projectors and commit guards inside the DB transaction at lines 933-948, but then calls `self.notify()` and `ch.publish()` outside the transaction at lines 1040-1047. A failure in notify after the DB commit cannot be rolled back.
- **Gap**: Incomplete transactional boundary — listener/notification failures after DB commit leave the system in an inconsistent state where the event is durable but subscribers were not notified.
- **Consequence**: Missed event notifications leading to stale read models after a partial failure.
- **Recommendation**: Wrap the notification phase in the same transaction, or implement an outbox pattern with retry for failed notifications.
- **Severity**: **High**

### 1.3 Session Execution State Machine — Missing State Transition Validation

- **Location**: `crates/blazecode-core/src/session_execution.rs:637-888`
- **BlazeCode**: The TS run-coordinator uses Effect's `Ref` (atomic reference) with clear transition validation. Invalid state transitions return typed errors.
- **BlazeCode**: The `RunCoordinator` at `session_execution.rs:557` exposes `run()`, `wake()`, `interrupt()`, and `await_idle()` as public async methods. There is no enforcement of state machine invariants — e.g., calling `interrupt()` on an already-interrupted lane is allowed (lines 786-821) and simply updates the interrupt_seq. The `state` field is an `Arc<RwLock<CoordinatorState>>` but is only used for external observation, not for enforcement.
- **Gap**: No runtime state machine validation. Invalid sequences (e.g., `run()` while already running with a different lane) rely on the caller to be correct.
- **Consequence**: Race conditions in concurrent execution requests could lead to multiple drain fibers for the same session, or a drain continuing after an interrupt.
- **Recommendation**: Add structured state machine with guard checks before each transition, matching the TS coordinator's strict lifecycle.
- **Severity**: **High**

### 1.4 Snapshot Git Repository Corruption

- **Location**: `crates/blazecode-core/src/snapshot.rs:184-221, 824-901`
- **BlazeCode**: Snapshot operations use Effect's `tryPromise` around `execa` git calls, with structured error handling and timeout management.
- **BlazeCode**: The `snapshot_git` function at `snapshot.rs:825` uses `std::process::Command` (blocking) inside async context. If git hangs (e.g., on a locked `.git/index.lock`), the entire async task thread pool can be blocked. The `seed_from_source()` method at `snapshot.rs:1008` writes to the git alternates file without checking if the source objects directory is valid. No timeout on any git command.
- **Gap**: Blocking git operations in async context. No timeout on git subprocess invocations. No validation of the alternates file content before writing.
- **Consequence**: Blocking the tokio runtime thread pool if git hangs. Corrupt snapshot gitdir if alternates file points to a non-existent or stale objects directory.
- **Recommendation**: Use `tokio::process::Command` for all git operations. Add configurable timeouts. Validate the source objects directory before writing alternates.
- **Severity**: **High**

### 1.5 Tool Execution Stream Error Handling

- **Location**: `crates/blazecode-core/src/session_runner.rs:639-679`
- **BlazeCode**: The TS runner uses Effect's `Stream` with built-in error recovery and structured error types for each stream chunk.
- **BlazeCode**: In `run_turn_attempt` at `session_runner.rs:639`, the LLM stream error handling has a subtle issue: when `overflow_detected` is true and recovery is attempted, control flow is encoded as a string inside `Error::Internal` (via `TurnControl::encode()` at line 714). This string-parsing approach is fragile — any change to the encoding format silently breaks the control flow.
- **Gap**: Control flow encoded in error messages parsed by string matching (lines 928-947). This is a well-known anti-pattern. The error is discarded if the string format changes.
- **Consequence**: If the encoding changes or if the error message is modified, overflow recovery silently stops working, leading to context overflow crashes that could have been recovered.
- **Recommendation**: Use a dedicated enum for control flow (not embedded in `Error::Internal`). Add exhaustive test coverage for all turn control paths.
- **Severity**: **High**

### 1.6 File Lock Stale Detection — Race Condition

- **Location**: `crates/blazecode-core/src/flock.rs:179-294`
- **BlazeCode**: The TS `flock.ts` uses the same directory-based lock pattern, but with Effect's structured concurrency and `Fiber.join` guarantees.
- **BlazeCode**: In `try_acquire_lock_dir` at `flock.rs:222-227`, there's a TOCTOU (time-of-check-time-of-use) race: the code checks `is_stale()`, then creates a `.breaker` directory, then re-checks staleness. If the original lock holder renews its heartbeat between the two staleness checks, the breaker logic can incorrectly delete a live lock.
- **Gap**: Race window between staleness check and stale lock removal. The breaker protocol does not guarantee mutual exclusion when the original holder is still alive.
- **Consequence**: Two processes could simultaneously believe they hold the same lock, leading to concurrent session writes and data corruption.
- **Recommendation**: Use the breaker directory itself as the authoritative lock (not the lock directory). The breaker should be acquired with `mkdir` atomicity, and the original lock should only be removed while holding the breaker.
- **Severity**: **Critical**

### 1.7 Session Revert — Message Deletion Without Transaction

- **Location**: `crates/blazecode-core/src/session_revert.rs:219-267`
- **BlazeCode**: The TS revert system processes message removal within a Drizzle transaction.
- **BlazeCode**: The `cleanup()` method at `session_revert.rs:244-250` performs individual `DELETE FROM session_message` queries without a wrapping transaction. If the process crashes mid-cleanup, some messages are deleted and others remain, leaving the session in an unrecoverable state.
- **Gap**: Non-transactional cascade delete in session cleanup.
- **Consequence**: Partial cleanup on crash → corrupted session with inconsistent message ordering and references to deleted parts.
- **Recommendation**: Wrap all revert cleanup operations in a SQLite transaction.
- **Severity**: **High**

---

## 2. Recovery Mechanisms

### 2.1 Crash Recovery — Database

- **Location**: `crates/blazecode-core/src/database.rs:59-66`, `crates/blazecode-core/src/storage.rs:658-697`
- **BlazeCode**: WAL mode + `synchronous = NORMAL` (same). The TS codebase has an explicit `wal_checkpoint(PASSIVE)` on connect. Additionally, Effect's `Layer` system ensures that database connections are properly scoped and closed on error.
- **BlazeCode**: PRAGMAs match BlazeCode exactly (WAL + NORMAL + busy_timeout 5000). The `Database::open()` at `storage.rs:658` applies all PRAGMAs. However, there is no connection pooling health check or automatic reconnection on pool exhaustion.
- **Gap**: No connection health monitoring or automatic reconnection. If the pool becomes exhausted or the database file is moved, there is no recovery path.
- **Consequence**: A transient pool exhaustion (e.g., from a burst of concurrent requests) causes permanent failure until the process restarts.
- **Recommendation**: Add pool health checks, connection retry logic, and automatic reconnection.
- **Severity**: **Medium**

### 2.2 Session Recovery — Crash During LLM Stream

- **Location**: `crates/blazecode-core/src/session_runner.rs:960-1155`
- **BlazeCode**: The TS runner persists each tool call result and LLM event to the database as it happens. On restart, it can replay events from the last persisted state.
- **BlazeCode**: The `run_loop` method at `session_runner.rs:957` does not persist intermediate events or tool results during execution. Only the final `SessionRunResult` is returned (and presumably persisted by the caller). If the process crashes mid-tool-loop, all progress is lost.
- **Gap**: No incremental persistence during the tool loop. The session state is only updated after the full run completes.
- **Consequence**: Crash during a long-running tool sequence (e.g., 10 tool calls) loses all work. The user must restart from scratch.
- **Recommendation**: Persist each tool call result incrementally to the database. Use event sourcing for LLM events as they arrive.
- **Severity**: **High**

### 2.3 Event Replay Recovery

- **Location**: `crates/blazecode-core/src/event.rs:1349-1422`
- **BlazeCode**: Event replay uses Effect's `Stream` with `replayAll()` which reconstructs state by re-applying all events sequentially. Divergence detection uses event ID + sequence number matching.
- **BlazeCode**: The `replay` method at `event.rs:1359` checks for diverged events by comparing event IDs (lines 1410-1417). If the stored event at a given sequence has a different ID, it returns `ReplayDiverged`. However, the check reads from the pool directly (not inside a transaction), so concurrent writes could interfere.
- **Gap**: Replay divergence check is not atomic — a concurrent event publication could change the stored event between the seq read and the ID comparison.
- **Consequence**: False positive or false negative replay divergence detection during concurrent event publication.
- **Recommendation**: Wrap replay checks in a transaction with `SELECT ... FOR UPDATE` (or SQLite's implicit transaction isolation).
- **Severity**: **Medium**

---

## 3. Resilience Patterns

### 3.1 Retry Logic

- **Location**: `crates/blazecode-core/src/error.rs:456-458`, `crates/blazecode-core/src/flock.rs:320-357`
- **BlazeCode**: Effect provides first-class retry policies (`Schedule`) with exponential backoff, jitter, max retries, and predicate-based retry decisions. The TS `LlmErrorReason` has a `retryable` getter that feeds into Effect's retry schedule.
- **BlazeCode**: The `LlmErrorReason::is_retryable()` method at `error.rs:456` correctly identifies retryable error reasons (RateLimit, ProviderInternal). However, this method is **defined but never called** — the `run_loop` and `run_turn_attempt` methods never check retryability or implement automatic retry. The flock module has its own retry loop with exponential backoff and jitter (flock.rs:320-357), but this is specific to file locking.
- **Gap**: LLM provider retry is a no-op. `is_retryable()` is dead code. Provider errors are returned directly without retry.
- **Consequence**: Transient provider errors (rate limits, 503s) immediately fail the turn instead of being retried. This degrades user experience significantly.
- **Recommendation**: Wire `is_retryable()` into the turn execution flow. Add exponential backoff with jitter for retryable provider errors, matching BlazeCode's `retryPolicy` implementation.
- **Severity**: **Critical**

### 3.2 Circuit Breakers

- **Location**: No circuit breaker implementation found
- **BlazeCode**: Uses Effect's `CircuitBreaker` for provider calls — after N consecutive failures, the circuit opens and subsequent calls fail fast without hitting the provider, with automatic half-open probing.
- **BlazeCode**: No circuit breaker implementation anywhere in the codebase.
- **Gap**: Complete absence of circuit breaker pattern. A failing provider (e.g., returning 429 or 503) will be called on every turn, wasting time and potentially exacerbating the provider's load.
- **Consequence**: Provider outages cause excessive retry storms and slow session degradation instead of fast failure.
- **Recommendation**: Implement a circuit breaker for provider calls. Track per-provider failure counts with configurable thresholds. Use `tokio::sync::watch` or similar for state notification.
- **Severity**: **High**

### 3.3 Bulkheads / Resource Isolation

- **Location**: `crates/blazecode-core/src/session_execution.rs:127-239`
- **BlazeCode**: Effect's `FiberSet` provides natural bulkhead isolation — each session gets its own fiber, and fiber sets can be scoped to resource pools.
- **BlazeCode**: The `FiberSet` at `session_execution.rs:127` uses `tokio::spawn` for all fibers. There is no per-session or per-provider resource limit. A single runaway session with many tool calls can consume all available tokio tasks.
- **Gap**: No resource isolation between sessions. One session's tool loop can starve other sessions of async tasks.
- **Consequence**: A session executing many parallel tool calls (e.g., multiple file reads) can exhaust the tokio thread pool, causing latency spikes for other sessions.
- **Recommendation**: Add per-session concurrency limits using `tokio::sync::Semaphore`. Use a dedicated `JoinSet` per session with a maximum task count.
- **Severity**: **Medium**

### 3.4 Timeouts

- **Location**: `crates/blazecode-core/src/tool_impls.rs:553-554, 757-802`, `crates/blazecode-core/src/flock.rs:26-29`
- **BlazeCode**: The TS codebase has configurable timeouts at every layer — provider request timeout, tool execution timeout, session idle timeout, compaction timeout. Timeouts are propagated through Effect's `Effect.timeout()` with `Timer`.
- **BlazeCode**: BashTool has explicit timeouts (default 2 min, max 10 min) at `tool_impls.rs:553-554`. The timeout is enforced via `tokio::select!` and kills the process group (lines 757-802). The FlockLock has configurable acquire timeout (default 5 min) at `flock.rs:26`. However, provider calls in `session_runner.rs` have **no timeout** — `provider.stream()` and `provider.complete()` could hang indefinitely.
- **Gap**: No timeout on provider API calls. If the provider never responds (e.g., TCP half-open, DNS hang), the session thread hangs forever.
- **Consequence**: A non-responsive provider causes indefinite session hang. The only recovery is process restart.
- **Recommendation**: Add mandatory timeouts to all provider calls. Default to 60s for streaming, 30s for completion. Make timeouts configurable in provider config.
- **Severity**: **Critical**

---

## 4. Error Propagation

### 4.1 Cross-Crate Error Conversion

- **Location**: `crates/blazecode-core/src/error.rs:23-352`, `crates/blazecode-server/src/error.rs:19-118`, `src/cli_error.rs:50-111`
- **BlazeCode**: Effect's typed error system ensures every error carries its full context. Errors flow through `Effect.catchAll` and `Effect.catchTag` with exhaustive pattern matching. The TS `Schema.TaggedErrorClass` pattern preserves error identity across boundaries.
- **BlazeCode**: Error propagation relies on `thiserror` derives and `anyhow` in the binary layer. The core `Error` enum at `error.rs:23` is a flat enum with ~50 variants. The server layer has its own `ServerError` enum at `server/error.rs:19` with different variant structure. The `cli_error.rs` formatter at `cli_error.rs:50` attempts to downcast from `anyhow::Error` to `blazecode_core::error::Error`, but the `dispatch_inner` function at `main.rs:1337` uses `i32` exit codes — it discards the error entirely and just returns a number.
- **Gap**: The `dispatch_inner` functions return raw `i32` exit codes, discarding all error context. The actual error is lost and cannot be surfaced to the user. The `CliErrorFormatter::format_error` is never called in `dispatch_inner` — the user never sees the actual error message.
- **Consequence**: All runtime errors from command handlers (provider init, DB, session, etc.) are silently discarded and only the exit code is returned. Users see a non-zero exit but no error message.
- **Recommendation**: Change command handlers to return `Result<(), anyhow::Error>` and propagate errors through `dispatch` for proper formatting.
- **Severity**: **Critical**

### 4.2 Error Context Preservation

- **Location**: `crates/blazecode-core/src/error.rs:86-91, 196-200`
- **BlazeCode**: The TS `LLMError` includes `module`, `method`, `reason`, and an optional `httpContext` field with full request/response details.
- **BlazeCode**: The `Error::Llm` variant at `error.rs:86` includes `module`, `method`, and `reason` (a `Box<LlmErrorReason>`). The `HttpContext` struct at `error.rs:697` is defined and tested but **not used** — it is never attached to any error variant.
- **Gap**: `HttpContext` is dead code. LLM errors do not carry HTTP request/response context, making debugging provider issues much harder.
- **Consequence**: When a provider returns an error, the user cannot see the actual HTTP response body, headers, or status code that caused it.
- **Recommendation**: Wire `HttpContext` into provider error construction. Include it in the `Error::Llm` variant.
- **Severity**: **Medium**

---

## 5. Graceful Degradation

### 5.1 Provider Unavailability

- **Location**: `crates/blazecode-core/src/provider.rs` (not fully read, but referenced from session_runner.rs)
- **BlazeCode**: The TS provider layer implements fallback chains — if the primary provider fails, the system tries the next configured provider. Effect's `Service` system allows dependency injection of alternative provider implementations.
- **BlazeCode**: No provider fallback mechanism. The `SessionRunner` is initialized with a specific `Arc<dyn Provider>` and `Model` — if this provider fails, the entire session fails.
- **Gap**: No provider fallback/failover. Single provider of failure.
- **Consequence**: If the configured provider is down, the session cannot proceed even if alternative providers are configured.
- **Recommendation**: Implement provider fallback chain. On provider error, attempt the fallback provider before failing the turn.
- **Severity**: **High**

### 5.2 SQLite Corruption

- **Location**: `crates/blazecode-core/src/storage.rs:658-697`
- **BlazeCode**: The TS database layer uses Drizzle ORM with automated WAL checkpointing and integrity checks on connection open. Effect's `Layer` system ensures clean initialization with health checks.
- **BlazeCode**: `Database::open()` at `storage.rs:658` applies PRAGMAs but does not run `PRAGMA integrity_check` to verify the database is not corrupted. There is no recovery path if the database is corrupted.
- **Gap**: No database integrity verification on startup. No recovery/migration from corrupted database.
- **Consequence**: A corrupted SQLite file is silently opened and used, causing unpredictable runtime errors (constraint violations, missing data, etc.).
- **Recommendation**: Run `PRAGMA integrity_check` on database open. If corruption is detected, attempt recovery from WAL or backup, or create a fresh database and re-sync from remote sources.
- **Severity**: **Medium**

### 5.3 Disk Full

- **Location**: No disk space management found
- **BlazeCode**: Handles disk full errors through Effect's error system — `ENOSPC` is caught and reported as a typed error with clear user guidance.
- **BlazeCode**: No disk space monitoring or graceful handling. A `std::io::Error` with `ErrorKind::StorageFull` would surface as `Error::Io`, which is displayed as a generic I/O error. There is no specific handling or user guidance.
- **Gap**: No disk space management, monitoring, or graceful degradation on `ENOSPC`.
- **Consequence**: Disk full causes generic I/O errors that users cannot distinguish from other I/O issues. Data loss risk for in-flight writes.
- **Recommendation**: Add disk space monitoring before writes. Check available space for large operations (tool output storage, snapshot creation). Surface clear error messages when disk is full.
- **Severity**: **Medium**

---

## 6. Data Durability

### 6.1 Write-Ahead Logging

- **Location**: `crates/blazecode-core/src/database.rs:59-66, 338-351`
- **BlazeCode**: WAL mode with `synchronous = NORMAL` provides crash consistency — transactions that return success are guaranteed to survive a crash.
- **BlazeCode**: Same PRAGMAs at `database.rs:59-66`. The `DatabaseConfig::pragmas()` method at `database.rs:338` generates the same PRAGMA set. The `Database::open()` at `storage.rs:658` also applies them.
- **Gap**: While the PRAGMAs match, there are code paths that perform SQL writes without wrapping in a transaction (e.g., session revert cleanup, individual message updates).
- **Consequence**: Non-transactional writes can leave the database in an inconsistent state after a crash.
- **Recommendation**: Audit all write paths to ensure they are transactional. Add a lint or runtime check.
- **Severity**: **High**

### 6.2 fsync Guarantees

- **Location**: `crates/blazecode-core/src/storage.rs:445-455`, `crates/blazecode-core/src/storage.rs:454`
- **BlazeCode**: JSON file storage uses `fs.writeFileSync` which ensures the data is flushed to disk (Node.js guarantees fsync for `writeFileSync`).
- **BlazeCode**: `Storage::write()` at `storage.rs:454` uses `std::fs::write()` which does **not** guarantee data is flushed to disk — it's equivalent to `write()` + `close()`, but the OS may buffer the write. No explicit `fsync()` call.
- **Gap**: JSON storage writes are not fsynced. A crash after `write()` returns but before the data reaches disk loses the written data.
- **Consequence**: Data loss on crash for JSON file storage (sessions, messages, parts written to JSON files).
- **Recommendation**: Use `File::create()` + `write_all()` + `sync_all()` for all JSON storage writes. `sync_all()` calls `fsync` on Unix.
- **Severity**: **High**

### 6.3 Event Table — Non-Transactional Projector Execution

- **Location**: `crates/blazecode-core/src/event_projector.rs:100-134`
- **BlazeCode**: Projectors run inside the event commit transaction. If a projector fails, the event is rolled back.
- **BlazeCode**: `EventProjector::project_event()` at `event_projector.rs:100` runs projectors and updates in-memory state, but does **not** run inside the transaction that commits the event. The caller (`EventV2::publish` at `event.rs:944`) runs projectors inside the transaction, but the `EventProjector` itself also has non-transactional projector execution paths.
- **Gap**: Dual paths for projector execution — one transactional (in EventV2.publish) and one non-transactional (in EventProjector.catch_up). If a projector fails during catch-up, the in-memory projection state is updated but the DB checkpoint doesn't reflect the failure.
- **Consequence**: Catch-up projection can silently skip events without updating the checkpoint, causing events to be re-processed on the next catch-up.
- **Recommendation**: Ensure catch-up projection updates the checkpoint atomically with projector success. Use a transaction for each projected event.
- **Severity**: **Medium**

---

## 7. Timeout Handling

### 7.1 Provider Timeouts

- **Location**: `crates/blazecode-core/src/session_runner.rs:625-634`, `crates/blazecode-core/src/session_runner.rs:990-994`
- **BlazeCode**: The TS provider interface has explicit timeout configuration per provider. Streams use `AbortSignal.timeout()` to enforce timeouts. Effect's `Effect.timeout()` wraps all provider calls.
- **BlazeCode**: `provider.stream()` at `session_runner.rs:625` is called with no timeout. `provider.complete()` in compaction at `session_compaction.rs:989` also has no timeout. Only the bash tool has explicit timeout enforcement.
- **Gap**: No timeout on any provider call (streaming or completion).
- **Consequence**: Provider hangs block the session indefinitely. See 3.4 for severity.
- **Recommendation**: Add timeout parameter to `Provider` trait. Pass timeout from config to all provider invocations.
- **Severity**: **Critical**

### 7.2 Session Idle Timeout

- **Location**: No session idle timeout found
- **BlazeCode**: The TS session system has configurable idle timeouts. If the session receives no user input for a specified period, it's automatically paused/interrupted.
- **BlazeCode**: No session idle timeout mechanism.
- **Gap**: Sessions can remain in the "running" state indefinitely if the provider hangs or the user walks away.
- **Consequence**: Resource leak — sessions consume memory and database connections forever.
- **Recommendation**: Implement session idle timeout using `tokio::time::timeout` around user input waits. Auto-interrupt idle sessions after a configurable period.
- **Severity**: **Medium**

---

## 8. Retry Logic

### 8.1 Provider Retry

- **Location**: `crates/blazecode-core/src/session_runner.rs:960-1155`
- **BlazeCode**: The TS runner wraps provider calls in Effect's `retry` with a schedule: exponential backoff, max 3 retries, only for retryable errors.
- **BlazeCode**: No provider retry logic. The `LlmErrorReason::is_retryable()` method exists but is not used. The session runner passes provider errors directly to the caller.
- **Gap**: Complete absence of provider retry. Dead `is_retryable()` code.
- **Consequence**: Every transient provider error terminates the current turn unnecessarily.
- **Recommendation**: Implement retry loop in `run_turn_attempt` for retryable errors. Use exponential backoff with jitter from the `retry_after_ms` field.
- **Severity**: **Critical**

### 8.2 Database Transient Error Retry

- **Location**: `crates/blazecode-core/src/database.rs:59-66`
- **BlazeCode**: Drizzle ORM with WAL mode + busy_timeout handles SQLITE_BUSY transparently.
- **BlazeCode**: `busy_timeout = 5000` at `database.rs:62` and `storage.rs:678` handles the most common transient error (SQLITE_BUSY). However, there is no retry logic for other transient DB errors (e.g., `SQLITE_IOERR`, `SQLITE_NOMEM`).
- **Gap**: No retry for non-BUSY transient SQLite errors.
- **Consequence**: Rare transient errors (memory pressure, I/O scheduler delays) cause immediate failure instead of being retried.
- **Recommendation**: Add a retry wrapper around critical DB operations that retries on transient sqlx errors.
- **Severity**: **Low**

---

## 9. Graceful Shutdown

### 9.1 Signal Handling

- **Location**: `src/main.rs:1233-1278`
- **BlazeCode**: The TS entry point registers signal handlers for SIGINT, SIGTERM, and SIGQUIT via Effect's `Fiber`. Signals trigger a graceful shutdown sequence: abort in-flight provider calls → persist session state → close database → exit.
- **BlazeCode**: `main()` at `main.rs:1233` creates a `tokio::runtime` and calls `rt.block_on(async_main(cli))`. There is **no signal handling**. `SIGINT` (Ctrl+C) will immediately terminate the process, possibly with partial writes.
- **Gap**: No signal handlers. Ctrl+C causes immediate, ungraceful termination.
- **Consequence**: Tool executions in progress are abruptly terminated. Database writes may be incomplete (WAL helps, but non-transactional writes may be lost). Session state is not persisted on shutdown.
- **Recommendation**: Use `tokio::signal::ctrl_c()` and `tokio::signal::unix::Signal` (for SIGTERM) to implement graceful shutdown: cancel in-flight operations, run finalize hooks, persist state, close connections.
- **Severity**: **Critical**

### 9.2 In-Flight Request Draining

- **Location**: `crates/blazecode-core/src/session_execution.rs:637-888`
- **BlazeCode**: The TS RunCoordinator tracks all in-flight drains and provides `interrupt()` which cancels the fiber and awaits completion. On shutdown, all active lanes are interrupted and drained.
- **BlazeCode**: `RunCoordinator` has `interrupt()` at `session_execution.rs:786` that cancels the fiber and sets `stopping = true`. However, there is no global `shutdown()` method that interrupts all lanes.
- **Gap**: No mechanism to interrupt all active sessions on shutdown.
- **Consequence**: On process termination (even with signal handling), active drains for other sessions remain running, potentially continuing to execute tool commands and write to the database.
- **Recommendation**: Add a `shutdown()` method to `RunCoordinator` that interrupts all active lanes and waits for them to settle.
- **Severity**: **High**

### 9.3 State Persistence on Shutdown

- **Location**: No shutdown hooks found
- **BlazeCode**: The TS `State` module's `finalize` hook runs on every state change. On shutdown, all services flush their state to durable storage.
- **BlazeCode**: No shutdown hooks or state persistence on exit. The `ObservabilityService` at `main.rs:1250` has an `init()` but no observable `shutdown()` or `flush()` method.
- **Gap**: No last-will persistence on shutdown. In-memory state (e.g., `AppState`, event projector state) is lost on exit.
- **Consequence**: Unflushed logs, unpersisted state changes, lost metric data.
- **Recommendation**: Register shutdown hooks via `Drop` or explicit `shutdown()` calls. Use `tokio::signal` to trigger orderly shutdown.
- **Severity**: **Medium**

---

## 10. State Consistency

### 10.1 Eventual Consistency Guarantees

- **Location**: `crates/blazecode-core/src/event_projector.rs:144-221`
- **BlazeCode**: The TS event system uses strong consistency for sync events and eventual consistency for projections. Read models are rebuilt from the event log, ensuring eventual consistency.
- **BlazeCode**: `EventProjector::catch_up()` at `event_projector.rs:144` reads events from the database and projects them sequentially. The checkpoint is updated after all events are processed. If a projector fails mid-way, the checkpoint is not updated and events are re-processed on the next catch-up — this is idempotent for most projectors but not guaranteed.
- **Gap**: Projector idempotency is not enforced or verified. If a projector has side effects (sending notifications, writing to external systems), replay can produce duplicate effects.
- **Consequence**: Non-idempotent projectors produce duplicate side effects on event replay/catch-up.
- **Recommendation**: Enforce projector idempotency. All projectors should be pure functions that only update read models. Side effects should use an outbox pattern with deduplication.
- **Severity**: **Medium**

### 10.2 Strong Consistency Requirements

- **Location**: `crates/blazecode-core/src/event.rs:899-986`
- **BlazeCode**: Sync events are published with strong consistency — the sequence number is computed and stored atomically with the event data in a transaction.
- **BlazeCode**: `EventV2::publish()` at `event.rs:899` uses a transaction for the sequence, event insert, and projectors. This provides strong consistency for the event log. Sequence numbers are monotonically increasing per aggregate.
- **Gap**: The `EventRegistry::define()` at `event.rs:581` uses a simple version check to replace definitions, but there is no enforcement that events with a given type reference a registered definition.
- **Consequence**: Events can be published with types that don't match registered definitions, leading to projector deserialization failures.
- **Recommendation**: Validate event data against the registered definition's `data_schema` before publication.
- **Severity**: **Low**

---

## 11. Idempotency

### 11.1 Tool Execution Idempotency

- **Location**: `crates/blazecode-core/src/tool_impls.rs:615-888` (BashTool), `crates/blazecode-core/src/session_runner.rs:741-790`
- **BlazeCode**: The TS tool system does not guarantee tool idempotency either, but Effect's structured concurrency allows clean abort-and-retry patterns. Read tools (read, glob, grep) are naturally idempotent; write tools (bash, write, edit) are not.
- **BlazeCode**: Same lack of idempotency guarantees. The `BashTool.execute()` method is fundamentally non-idempotent — running `rm -rf /tmp/foo` twice has different effects. The `edit_replace` function at `tool_impls.rs:445` is deterministic and idempotent. The `ReadTool` is idempotent.
- **Gap**: No idempotency key/token for tool executions. A retried tool call (e.g., after timeout) runs again with no deduplication.
- **Consequence**: Non-idempotent tools (bash, write, edit with replace_all) can produce unintended duplicate effects on retry.
- **Recommendation**: Add idempotency key to `ToolContext`. Tools should check if a given key was already executed and return the cached result. Document which tools are idempotent and which are not.
- **Severity**: **Medium**

### 11.2 Event Publishing Idempotency

- **Location**: `crates/blazecode-core/src/event.rs:913-931`
- **BlazeCode**: Event ID uniqueness ensures idempotent event publication — the same event ID cannot be published twice.
- **BlazeCode**: `EventV2::publish()` at `event.rs:913-931` checks event ID uniqueness before inserting. If the event ID already exists, it returns `EventError::EventAlreadyExists`. This provides strong idempotency guarantees for event publication.
- **Gap**: The in-memory-only path (no database) at `event.rs:990-1016` does not check event ID uniqueness — duplicate events with the same ID are allowed.
- **Consequence**: In in-memory mode, duplicate event publications can produce duplicate state changes.
- **Recommendation**: Add in-memory event ID deduplication for the no-database path.
- **Severity**: **Low**

---

## 12. Concurrent Access

### 12.1 Multi-Process Safety (flock)

- **Location**: `crates/blazecode-core/src/flock.rs:59-123, 179-358`
- **BlazeCode**: The TS flock uses the same directory-based lock pattern. TypeScript's single-threaded event loop naturally serializes access within a process. Cross-process locking uses the same `mkdir` atomicity.
- **BlazeCode**: `FlockLease` at `flock.rs:61` provides cross-process mutual exclusion via directory-based locking with heartbeat and stale detection. The `acquire()` function at `flock.rs:302` uses `mkdir` as the atomic primitive and supports timeouts with exponential backoff and jitter. The `Drop` impl at `flock.rs:110` provides panic-safe release.
- **Gap**: The stale detection race (1.6) and lack of automatic stale recovery on startup (if a lock was held by a crashed process, it won't be detected until another process tries to acquire it).
- **Consequence**: Stale locks from crashed processes persist until another process needs the lock.
- **Recommendation**: On startup, run a stale lock cleanup for all locks owned by processes that are no longer alive. Check PID in meta.json.
- **Severity**: **High**

### 12.2 Multi-Thread Safety (Mutex/RwLock)

- **Location**: `crates/blazecode-core/src/session.rs:25`, `crates/blazecode-core/src/state.rs:26`, `crates/blazecode-core/src/snapshot.rs:138`
- **BlazeCode**: The TS codebase is single-threaded, so no thread safety concerns. Effect's `Ref` provides atomic state management within the single-threaded runtime.
- **BlazeCode**: Uses `tokio::sync::Mutex` (session.rs:25), `std::sync::Mutex` (snapshot.rs:138), and `tokio::sync::RwLock` (event.rs:30) throughout. The `SnapshotService` uses `std::sync::Mutex<()>` at `snapshot.rs:138` for per-operation mutual exclusion — but this is a **blocking mutex** used inside async code. If the lock is held while awaiting a future (e.g., in `snapshot_git`), it will block the entire tokio thread.
- **Gap**: `std::sync::Mutex` used in async context in `SnapshotService`. This can cause tokio worker thread starvation.
- **Consequence**: If a snapshot operation holds the std::sync::Mutex while awaiting git, all other async tasks on the same thread are blocked.
- **Recommendation**: Replace `std::sync::Mutex` with `tokio::sync::Mutex` in all async code paths. The snapshot service lock should be async-compatible.
- **Severity**: **High**

---

## 13. Partial Failure

### 13.1 Batch Event Replay

- **Location**: `crates/blazecode-core/src/event.rs:750-754`
- **BlazeCode**: The TS `replayAll()` processes events in sequence and fails atomically — if any event fails, the batch is rolled back.
- **BlazeCode**: `replay_all()` at `event.rs:750` is a trait method but the implementation is not shown in the files reviewed. The `EventProjector::catch_up()` at `event_projector.rs:144` processes events sequentially but does not have atomic batch semantics — if the 50th of 500 events fails, the first 49 have already been processed and their state changes committed.
- **Gap**: Catch-up projection is not atomic — partial replay can leave the system in a mixed state.
- **Consequence**: After a partial catch-up failure, some events are projected and some are not. The next catch-up may double-process some events or skip others.
- **Recommendation**: Add a transaction around catch-up projection. If any event fails, roll back to the last known-good checkpoint.
- **Severity**: **Medium**

### 13.2 Multiple Tool Call Handling

- **Location**: `crates/blazecode-core/src/session_runner.rs:721-797`
- **BlazeCode**: The TS runner processes tool calls concurrently using `Promise.allSettled()` — if one tool call fails, the others still complete and results are returned.
- **BlazeCode**: `run_turn_attempt()` at `session_runner.rs:740` processes tool calls sequentially in a `for` loop. If the third of five tool calls fails, the error is returned immediately and the remaining tool calls are never executed.
- **Gap**: Sequential tool execution with early exit on first failure. No partial success handling.
- **Consequence**: A single failing tool call cancels all subsequent tool calls, losing potentially successful work.
- **Recommendation**: Execute independent tool calls concurrently. Use `FuturesUnordered` or `join_all` to collect all results. Report partial failures alongside successful results.
- **Severity**: **High**

---

## 14. Validation

### 14.1 Input Validation

- **Location**: `crates/blazecode-core/src/tool_impls.rs:590-613, 1044-1062`
- **BlazeCode**: The TS tool system uses Zod schemas for input validation. Every tool call is validated against its schema before execution, providing type-safe and structured validation errors.
- **BlazeCode**: Tools define a `parameters_schema()` method returning `serde_json::Value` (a JSON Schema). However, this schema is **documentation only** — it is not used for validation. The `BashTool.execute()` at `tool_impls.rs:616` manually checks for `command` field with `args["command"].as_str().ok_or_else(...)`. The `ReadTool.execute()` does similar manual field extraction.
- **Gap**: JSON Schema is defined but not enforced. Input validation is ad-hoc and inconsistent across tools.
- **Consequence**: Missing or invalid arguments produce inconsistent error messages (some are ToolInvalidArguments, some are generic Tool errors). Complex nested schemas may not be validated at all.
- **Recommendation**: Implement a JSON Schema validator (e.g., `jsonschema` crate) and validate all tool inputs against their schema before execution.
- **Severity**: **Medium**

### 14.2 Output Validation

- **Location**: `crates/blazecode-core/src/storage.rs:116-162`
- **BlazeCode**: The TS storage system validates output via Drizzle ORM's type system — column types are checked at compile time.
- **BlazeCode**: `Storage::write()` at `storage.rs:445` does not validate the written value against any schema. The `read_with_schema()` method at `storage.rs:421` validates on read, but there is no write-time validation.
- **Gap**: Corrupt or invalid data can be written to storage. The corruption is only detected on read, potentially after the original data is overwritten.
- **Consequence**: If a bug in the application writes invalid data, it corrupts the storage and the error is only detected later when the data is read.
- **Recommendation**: Add write-time schema validation using the same `StorageSchema` system used for reads.
- **Severity**: **Low**

### 14.3 JSON Schema Validation

- **Location**: `crates/blazecode-core/src/event.rs:184-185`
- **BlazeCode**: The TS `EventDefinition` includes a `dataSchema` that's used for runtime validation of event data before publication.
- **BlazeCode**: `EventDefinition` at `event.rs:178` has `data_schema: serde_json::Value`, but it is **never used** for validation. The `publish()` method at `event.rs:855` does not validate event data against the schema.
- **Gap**: Event schema definition is dead code — events are not validated against their schema at any point.
- **Consequence**: Events with invalid data (wrong types, missing fields) can be published, causing projector deserialization failures.
- **Recommendation**: Implement JSON Schema validation for event data on publication. Reject events that don't match their definition's schema.
- **Severity**: **Medium**

---

## 15. BlazeCode Comparison

BlazeCode (TypeScript) uses Effect.ts for structured concurrency and error handling, which provides:

| Aspect | BlazeCode (Effect.ts) | BlazeCode |
|---|---|---|
| **Error types** | ~120+ `Schema.TaggedErrorClass` tagged unions | ~50-variant `thiserror` flat enum |
| **Concurrency** | `Fiber`, `FiberSet`, `Scope` for structured concurrency | `tokio::spawn`, `FiberSet` (basic) |
| **Retry** | Built-in `Schedule` with configurable policies | None (dead `is_retryable()`) |
| **Circuit breaker** | Built-in `CircuitBreaker` | None |
| **State management** | `Ref`, `MutableRef` with atomic semantics | `AppState` (Mutex-based, no atomicity) |
| **Streaming** | `Stream` with backpressure, error recovery | `futures::Stream` from `tokio::sync::broadcast` |
| **Resource management** | `Scope` with automatic cleanup (Scope.finalizer) | `Drop` trait (limited), no structured scope |
| **Dependency injection** | `Context` + `Layer` (compile-time checked) | Constructor injection (manual, unchecked) |
| **Transaction safety** | Effect guarantees atomicity | Mixed — some transactional, some raw queries |
| **Signal handling** | Built-in `Fiber` interruption on signal | None |
| **Timeout** | `Effect.timeout()` at every layer | Ad-hoc (bash tool only) |
| **Provider fallback** | Fallback chains in provider config | None |

### Key Reliability Gaps Summary

| # | Gap | Severity | Impact |
|---|---|---|---|
| 1 | No provider retry (dead `is_retryable()`) | Critical | Transient provider errors always fail |
| 2 | No timeouts on provider calls | Critical | Provider hangs block sessions forever |
| 3 | No signal handling / graceful shutdown | Critical | Ctrl+C causes data loss |
| 4 | Error context lost (handlers return `i32`) | Critical | Users see no error messages |
| 5 | Schema drift between SQL and migrations | Critical | Runtime crashes on DB writes |
| 6 | File lock TOCTOU race | Critical | Concurrent lock ownership |
| 7 | Sequential tool execution (no partial failures) | High | One failing tool cancels all others |
| 8 | Control flow via string parsing in errors | High | Fragile overflow recovery |
| 9 | No circuit breaker for providers | High | Retry storms on provider issues |
| 10 | No provider fallback chain | High | Single provider of failure |
| 11 | Blocking mutex in async snapshot code | High | Tokio thread starvation |
| 12 | Non-transactional session revert cleanup | High | Corrupted sessions on crash |
| 13 | No fsync in JSON storage | High | Data loss on crash |
| 14 | Missing in-flight request draining | High | Post-shutdown tool execution |
| 15 | No incremental session persistence | High | Total progress loss on crash |

### Recommendations Priority

1. **Immediate (Critical)**:
   - Add provider retry using the existing `is_retryable()` method
   - Add mandatory timeouts to all provider calls
   - Implement signal handling (SIGINT, SIGTERM) with graceful shutdown
   - Fix error propagation — handlers should return errors, not `i32`
   - Use `sqlx::query!` for compile-time SQL checking

2. **Short-term (High)**:
   - Implement circuit breaker for LLM providers
   - Add provider fallback/failover
   - Fix TOCTOU race in file locking
   - Replace `std::sync::Mutex` with `tokio::sync::Mutex` in snapshot
   - Add fsync to JSON storage writes
   - Execute independent tool calls concurrently
   - Persist session state incrementally

3. **Medium-term**:
   - Add JSON Schema validation for events and tool inputs
   - Implement resource isolation (per-session concurrency limits)
   - Add database integrity checks on startup
   - Add session idle timeout
   - Implement event-driven architecture for reliable notifications
   - Add structured state machine validation in RunCoordinator

4. **Long-term**:
   - Full event sourcing with replay and projection (partially done)
   - Distributed tracing integration
   - Automated chaos testing
   - Performance benchmarking and optimization
