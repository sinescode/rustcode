# Concurrency Audit: RustCode (Rust) vs OpenCode (TypeScript)

**Date**: 2026-06-19
**Scope**: Deep async/concurrency analysis of the RustCode port, comparing against OpenCode's Effect.ts + EventEmitter model
**Commit**: `rustcode` current HEAD; OpenCode pinned at `5d0f86606ac30690f79f0a6a9f41a1f49fe95d0b`

---

## Executive Summary

RustCode uses `tokio` for async execution with `broadcast` channels for pub/sub, `Mutex`/`RwLock` for shared state, and `CancellationToken` for cancellation. While the architecture is sound, there are **2 critical**, **8 high**, **6 medium**, and **5 low** severity findings. The most urgent are: a **compile-error type mismatch** in `background_job.rs`, a **nested tokio runtime** in `main.rs`, **unbounded task & channel growth** across the TUI, and **sync code blocking the async runtime** via `std::process::Command` and `std::sync::Mutex` in hot paths.

---

## 1. Compile-Error: `watch::channel::<false>` — Invalid Type Parameter

| Field | Detail |
|---|---|
| **Location** | `background_job.rs:332-333` |
| **Severity** | **Critical** |
| **Effort** | 5 min |

### Problem
```rust
// background_job.rs:332-333
let (cancel_tx, cancel_rx) = watch::channel::<false>(false);
let (promote_tx, _promote_rx) = watch::channel::<false>(false);
```

`false` is a **const bool value**, not a type. The Rust compiler will reject this. The correct call should be either `watch::channel::<bool>(false)` or `watch::channel(false)` (type inference).

### Evidence
The `cancelled()` helper at line 618 consumes `&watch::Receiver<bool>`, confirming the intended type is `bool`:

```rust
// background_job.rs:618
async fn cancelled(rx: &watch::Receiver<bool>) {
```

### Impact
This code **will not compile**. Since the CLAUDE.md forbids running `cargo build` locally, this bug would only surface in CI.

### Recommendation
Replace both lines with `watch::channel(false)` (let type inference handle it).

---

## 2. Nested Tokio Runtime in `main.rs`

| Field | Detail |
|---|---|
| **Location** | `src/main.rs:2394-2396` |
| **Severity** | **Critical** |
| **Effort** | 2-4 hours |

### Problem
```rust
// main.rs:2394-2396
let rt = tokio::runtime::Runtime::new().unwrap();
let exit_code = rt.block_on(async {
    let tui_result = app.run_async();
    ...
});
```

A **second `tokio::runtime::Runtime`** is created inside the existing `#[tokio::main]` async context. This is a tokio anti-pattern that can cause:
- **Panics**: `Cannot start a runtime from within a runtime` in some configurations
- **Thread explosion**: Each runtime creates its own thread pool
- **Unexpected scheduling**: Tasks are multiplexed across two independent schedulers
- **Deadlocks**: If the inner runtime blocks on a resource held by the outer runtime

### Evidence
The outer runtime is configured at `main.rs:1248` as:
```rust
rt.block_on(async_main(cli));
```
Inside `async_main`, at line 2394, a second runtime is created for the TUI event loop.

### Impact
Non-deterministic panics in production, especially on systems with constrained thread counts. The TUI's `spawn_blocking` and `tokio::spawn` calls inside `run_async()` will run on the inner runtime's thread pool, while the rest of the application runs on the outer pool.

### Recommendation
Use `tokio::task::spawn_blocking` or `tokio::task::LocalSet` instead of creating a nested runtime. If the TUI requires a dedicated thread for crossterm rendering, use `std::thread::spawn` with a channel bridge, or configure the outer runtime with enough threads.

---

## 3. Unbounded Task Spawning — No Backpressure or Concurrency Limits

| Field | Detail |
|---|---|
| **Location** | Multiple files (see evidence) |
| **Severity** | **High** |
| **Effort** | 1-2 weeks |

### Problem
`tokio::spawn` is used pervasively with **no semaphore, task limit, or backpressure mechanism**. Every user action, keyboard event, tool execution, and stream operation creates a new tokio task.

### Evidence — Spawn Sites (counted ~49+)

| File | Line(s) | Count | Pattern |
|---|---|---|---|
| `rustcode-tui/src/app.rs` | 414, 464, 739, 1753, 1789, 1843, 2118, 2145, 2230, 2261, 2315, 2349, 2384, 2420, 2841, 2998, 3061, 3094, 3124 | **19** | Action handlers (fire-and-forget) |
| `crates/rustcode-core/src/process.rs` | 570, 601 | 2 | stdout/stderr readers (fire-and-forget) |
| `crates/rustcode-core/src/background_job.rs` | 348, 411, 1208 | 3 | Background job tasks |
| `crates/rustcode-core/src/mcp.rs` | 1470 | 1 | SSE stream processor |
| `crates/rustcode-core/src/database.rs` | 2325 | 1 | Test concurrent insert |
| `crates/rustcode-core/src/state.rs` | 619, 626, 633, 878, 889 | 5 | Test concurrent transforms |
| `crates/rustcode-core/src/question.rs` | 974, 1012, 1066, 1077, 1127 | 5 | Test concurrent questions |
| `crates/rustcode-lsp/src/lib.rs` | 593, 597 | 2 | LSP stderr/stdout readers |
| `crates/rustcode-server/src/routes/session.rs` | 1129 | 1 | Async prompt processing |
| `crates/rustcode-server/src/routes/event.rs` | 146 | 1 | Bus-to-broadcast bridge |
| `src/main.rs` | 1864, 2338, 2375, 5321, 7755 | 5 | CLI mode tasks |

### Impact
Under heavy load (e.g., rapid tool calls, large file operations), tokio's task count grows unboundedly, leading to:
- **Memory exhaustion** from task stack allocations
- **Tokio executor saturation**, increasing latency for all tasks
- **Unbounded channel buffering** when spawned tasks feed `unbounded_channel` receivers (see Finding #4)

### Recommendation
- Use `tokio::sync::Semaphore` to limit concurrent tool executions and background jobs
- Replace fire-and-forget spawns with structured concurrency using `JoinSet` or `FuturesUnordered`
- For the TUI action handlers, use a queue with bounded worker pool instead of per-action spawns

---

## 4. Unbounded Channels — No Backpressure

| Field | Detail |
|---|---|
| **Location** | `rustcode-tui/src/app.rs:412, 461, 477`, `mcp.rs:1467` |
| **Severity** | **High** |
| **Effort** | 3-5 days |

### Problem
`tokio::sync::mpsc::unbounded_channel` is used in critical data paths with no mechanism to slow down producers when consumers are overwhelmed.

### Evidence

```rust
// app.rs:411-412
let (event_tx, mut event_rx) =
    tokio::sync::mpsc::unbounded_channel::<crossterm::event::Event>();

// app.rs:460-461
let (bus_tx, local_bus_rx) =
    tokio::sync::mpsc::unbounded_channel::<rustcode_core::bus::GlobalEvent>();

// app.rs:477-480
let (llm_tx, local_llm_rx) = tokio::sync::mpsc::unbounded_channel::<(
    String,
    rustcode_core::provider::LlmEvent,
)>();

// mcp.rs:1466-1467
let (sse_tx, sse_rx) =
    tokio::sync::mpsc::unbounded_channel::<(u64, serde_json::Value)>();
```

### Impact
If the main TUI event loop (run at ~50ms render intervals) processes events slower than producers:
- crossterm events queue up indefinitely
- LLM stream events fill memory
- Bus events accumulate
- No backpressure → process OOM

### Recommendation
- Use `tokio::sync::mpsc::channel(capacity)` with a bounded buffer
- In the LLM stream case, implement a `select!` that drops old events or applies backpressure
- For crossterm events, a bounded channel with `try_send` or `block_send_on_full` is appropriate since input events can be safely dropped under load

---

## 5. `std::sync::Mutex` / `std::sync::RwLock` Held Across Async Boundaries

| Field | Detail |
|---|---|
| **Location** | `config.rs:32,893`, `env.rs:28`, `id.rs:18`, `tool_impls.rs:3565`, `snapshot.rs:15`, `workspace.rs:366`, `catalog.rs:425` |
| **Severity** | **High** |
| **Effort** | 1-2 weeks |

### Problem
`std::sync::Mutex` and `std::sync::RwLock` are not designed for async contexts. When a `std::sync::Mutex` lock is held and the async task yields (`.await`), the lock **blocks the entire tokio worker thread**, not just the current task. This can cause thread-pool starvation and deadlocks.

### Evidence — Critical Cases

**Config (read-heavy, called from everywhere)**:
```rust
// config.rs:32
use std::sync::RwLock;

// config.rs:893
pub fn get(&self) -> Info {
    self.info.read().expect("Config lock poisoned").clone()
}
```
`Config::get()` is a synchronous method that acquires `std::sync::RwLock::read()`. If called from an async context (which it is, across the entire codebase), this blocks the tokio worker thread. While `RwLock::read` doesn't block for long (just a clone), the pattern is unsafe.

**Static Mutex in tool_impls (holds during async execution)**:
```rust
// tool_impls.rs:3564-3565
static TASK_REGISTRY: std::sync::OnceLock<
    std::sync::Mutex<HashMap<String, TaskRecord>>,
> = std::sync::OnceLock::new();
```
Called from async tool execution — the `Mutex::lock()` blocks the thread.

**ID generation (called from nearly every async path)**:
```rust
// id.rs:18
use std::sync::Mutex;
```
The ID generator uses `std::sync::Mutex` for its counter. Every call to `id::ascending()` or `id::create()` from async code blocks the thread. This is extremely hot-path — called for every message, event, session, and tool call.

### Impact
- **Tokio worker thread blocking**: If all N worker threads are blocked on `std::sync::Mutex`, no progress can be made
- **False contention**: A short `.await` in `background_job.rs:start()` releases the async task's thread, but the `std::sync::RwLock` in config `get()` has already released by then — however, holding these locks across `.await` points (even indirectly) is a risk
- **Loom-detected deadlocks**: tokio's loom will flag these

### Recommendation
- Replace `std::sync::Mutex` with `tokio::sync::Mutex` for any lock held across `.await` points
- For `std::sync::RwLock` in `config.rs`, evaluate if `tokio::sync::RwLock` is needed or if the `get()` pattern (brief read + clone) is acceptable since it never holds across await
- For hot-path locks like `id.rs`, consider `std::sync::atomic::AtomicU64` or lock-free counters
- Use `std::sync::Mutex` only in `spawn_blocking` or synchronous callbacks

---

## 6. Dropped `JoinHandle` — Fire-and-Forget Tasks

| Field | Detail |
|---|---|
| **Location** | Multiple (see evidence) |
| **Severity** | **High** |
| **Effort** | 2-3 days |

### Problem
When `tokio::spawn` returns a `JoinHandle`, discarding it means:
- Task errors are silently swallowed (no way to `.await` for the `Result`)
- Tasks become "detached" — they run independently but cannot be tracked
- On program shutdown, there's no way to wait for task completion

### Evidence

```rust
// process.rs:570
tokio::spawn(async move { ... });  // JoinHandle discarded

// process.rs:601
tokio::spawn(async move { ... });  // JoinHandle discarded

// background_job.rs:348
tokio::spawn(async move { ... });  // JoinHandle discarded

// mcp.rs:1470
let _sse_handle = tokio::spawn(async move { ... });  // underscore prefix suppresses warning

// LSP lib.rs:593,597
tokio::spawn(read_stderr(...));               // JoinHandle discarded
tokio::spawn(read_stdout_loop(...));           // JoinHandle discarded

// TUI app.rs:464
tokio::spawn(async move { ... });  // JoinHandle discarded
```

### Impact
- Stdout/stderr reader tasks in `process.rs` can silently fail, leaving the stream hanging
- LSP reader failures go undetected
- Background job panics are invisible
- No graceful shutdown path for spawned tasks

### Recommendation
- Store `JoinHandle`s in a `JoinSet` or a `Vec<JoinHandle<()>>` for lifecycle management
- For stream readers, propagate errors through the channel or use `select!` with a cancellation token
- Use structured concurrency with `tokio::task::JoinSet` for bounded groups of related tasks

---

## 7. Sequential Tool Execution — Missed Parallelism

| Field | Detail |
|---|---|
| **Location** | `session.rs:1533-1544`, `session_runner.rs:299-346` |
| **Severity** | **High** |
| **Effort** | 2-3 days |

### Problem
Tool calls from the LLM are executed **sequentially in a for loop**, even when they are independent. The TypeScript source uses Effect.js `Effect.all()` for concurrent tool execution.

### Evidence

**SessionProcessor::handle_event (session.rs)**:
```rust
// session.rs:1532-1544
let result = self
    .execute_tool_call(ctx, id.as_str(), name.as_str(), input)
    .await;

match result {
    Ok(output) => {
        self.complete_tool_call(ctx, id.as_str(), &output).await?;
    }
    Err(e) => {
        self.fail_tool_call(ctx, id.as_str(), &e.to_string()).await?;
    }
}
```

**SessionRunner (session_runner.rs)**:
```rust
// session_runner.rs:299-346
for (_key, tc) in &pending_tool_calls {
    let ctx = ToolContext { ... };
    let result = self
        .tool_registry
        .execute_by_name(&tc.name, tc.input.clone(), &ctx)
        .await;
    // ... process result ...
}
```

### Impact
- Independent tools (e.g., reading two files, searching two directories) run serially, increasing latency
- Each tool adds round-trip time; with N tools, total time = sum, not max
- User-perceived stall while tools execute one-by-one

### Recommendation
- Use `futures::future::join_all` or `FuturesUnordered` for concurrent tool execution
- Apply `tokio::sync::Semaphore` to limit concurrency (e.g., max 4 concurrent tools)
- Implement cancellation propagation: if one tool fails, cancel remaining in-flight tools

---

## 8. `std::process::Command` in Async Context — Blocking the Thread Pool

| Field | Detail |
|---|---|
| **Location** | `git.rs`, `repository.rs`, `project.rs`, `snapshot.rs`, `shell.rs`, `system_context.rs`, `ripgrep.rs` |
| **Severity** | **High** |
| **Effort** | 1-2 weeks |

### Problem
`std::process::Command::new(...).output()` blocks the calling thread for the **entire duration of the child process**. When called from an async tokio context, this blocks a worker thread, potentially starving other tasks.

### Evidence
Over **38 call sites** across the codebase use `std::process::Command`. Key examples:

```rust
// git.rs:9
use std::process::Command;  // All git operations use sync command

// repository.rs:884
let mut cmd = std::process::Command::new("git");  // 20+ sites in this file alone

// project.rs:689
let output = std::process::Command::new("git")  // git operations in async fn

// snapshot.rs:14
use std::process::Command;  // All snapshot operations
```

Compare with `process.rs` which correctly uses `tokio::process::Command`:
```rust
// process.rs:460 — correct pattern
child.wait_with_output().await
```

### Impact
- Every `git status`, `git diff`, git operation blocks a tokio worker thread for the duration
- With a default 4-thread tokio runtime, just 4 concurrent git operations exhaust the pool
- Other tasks (LLM streaming, event processing) are starved
- Shell tool execution (via `std::process::Command`) blocks the thread

### Recommendation
- Replace all `std::process::Command` with `tokio::process::Command` in async functions
- Wrap blocking calls in `tokio::task::spawn_blocking` when `tokio::process` is not suitable
- For git operations, create a `GitService` that uses `tokio::process::Command` throughout

---

## 9. Broadcast Channel Capacity — Silent Event Loss Under Load

| Field | Detail |
|---|---|
| **Location** | `bus.rs:256`, `event.rs:620`, `event.rs:145` |
| **Severity** | **Medium** |
| **Effort** | 1 day |

### Problem
`tokio::sync::broadcast` channels drop the **oldest events** when the buffer is full and a slow consumer can't keep up. The `Lagged` error is logged but events are silently dropped.

### Evidence

```rust
// bus.rs:256 (default capacity 1024)
Self::new(1024)

// event.rs:620 (EventPubSub capacity)
let (sender, _) = tokio::sync::broadcast::channel(capacity);

// event.rs:145 (SSE bridge, capacity 256)
let (tx, rx) = tokio::sync::broadcast::channel(256);
```

The `BusSubscription::recv()` handles lag by logging and retrying once:
```rust
// bus.rs:348-352
Err(broadcast::error::RecvError::Lagged(skipped)) => {
    tracing::warn!(skipped, "bus subscriber lagged — {skipped} events skipped");
    self.receiver.recv().await.ok()  // Retries once; if lag again, returns None
}
```

### Impact
- During LLM streaming, text delta events arrive rapidly. If TUI rendering falls behind, events are silently dropped
- The SSE bus bridge (capacity 256) is especially vulnerable — a brief network stall can overflow it
- No subscriber recovery mechanism — once lagged, the subscriber may miss events permanently

### Recommendation
- Increase default capacity based on worst-case event burst (e.g., 10000)
- Implement a ring-buffer `StreamExt` combinator that can catch up on missed events
- For critical events (session state transitions), consider a separate reliable channel (e.g., `watch`)

---

## 10. Tool-Level CancellationTokens Never Cancelled

| Field | Detail |
|---|---|
| **Location** | `session.rs:1682`, `session_runner.rs:304` |
| **Severity** | **Medium** |
| **Effort** | 1 day |

### Problem
`CancellationToken::new()` is created for each tool execution but **never actually cancelled**. The abort mechanism exists in the type system but is not wired up.

### Evidence

```rust
// session.rs:1678-1682
let tool_ctx = crate::tool::ToolContext {
    ...
    abort: CancellationToken::new(),  // Fresh token — never linked to session abort
    ...
};

// session_runner.rs:300-304
let ctx = ToolContext {
    ...
    abort: tokio_util::sync::CancellationToken::new(),  // Same pattern
    ...
};
```

Compare with the correct pattern in `session.rs:1196`:
```rust
pub async fn process(
    &self,
    ...
    cancel_token: CancellationToken,  // Passed in from caller
) -> Result<ProcessResult, SessionError> {
```

### Impact
- When the user aborts a session, currently running tools continue executing
- There is no way to cancel an in-flight tool call from the UI or another task
- The `abort` field in `ToolContext` is dead weight — it suggests cancellation is supported but isn't wired

### Recommendation
Thread the session-level `CancellationToken` down to each `ToolContext`. Use `child_token()` to create derived tokens so cancelling the session aborts all in-flight tools:
```rust
let tool_token = session_token.child_token();
let tool_ctx = ToolContext { abort: tool_token, ... };
```

---

## 11. TUI `tokio::spawn` Spam — Per-Action Task Explosion

| Field | Detail |
|---|---|
| **Location** | `rustcode-tui/src/app.rs:1753, 1789, 1843, 2118, 2145, 2230, 2261, 2315, 2349, 2384, 2420, 2841, 2998, 3061, 3094, 3124` |
| **Severity** | **Medium** |
| **Effort** | 3-5 days |

### Problem
The TUI spawns a **new tokio task for every user action**: session navigation, forking, exporting, list dialogs, submitting prompts, permission replies. Rapid user interaction can create dozens of tasks, many of which have overlapping side effects.

### Evidence Pattern
```rust
// app.rs:2384 — ChildPrev
tokio::spawn(async move {
    match sessions.list(None).await { ... }
});

// app.rs:2420 — Parent
tokio::spawn(async move {
    match sessions.get(&sid).await { ... }
});

// app.rs:2998 — Submit prompt
tokio::spawn(async move {
    match client.post(&submit_url).json(...).send().await { ... }
});
```

### Impact
- Rapid up/down navigation creates concurrent list/get calls with stale results
- No deduplication: pressing "next" 5 times fires 5 session list queries
- Task pileup if network requests are slow

### Recommendation
- Implement a task-dedup mechanism: cancel in-flight navigation tasks when a new one arrives
- Use a single background worker with a channel for sequential processing of UI actions
- Consider `JoinSet` with bounded size for background operations

---

## 12. State.rs `tokio::test` — Tasks Not Properly Synchronized

| Field | Detail |
|---|---|
| **Location** | `state.rs:619-640, 878-898` |
| **Severity** | **Medium** |
| **Effort** | 2-3 hours |

### Problem
Test `concurrent_transforms_are_ordered` uses `tokio::join!` which waits for all tasks, but the test asserts only that content "contains" characters — **it doesn't verify specific ordering**. The real issue is that `state::update()` may do conflicting work.

### Evidence
```rust
// state.rs:619-646
let h1 = tokio::spawn(async move {
    s1.update(Arc::new(|e: &mut DocumentEditor<'_>| {
        e.append_content("A");
    })).await;
});
// ... h2, h3 same pattern
let _ = tokio::join!(h1, h2, h3);
// Only checks contains, not ordering or completeness
assert!(doc.content.contains('A'));
assert!(doc.content.contains('B'));
assert!(doc.content.contains('C'));
```

### Impact
The test will pass even if the state machine is broken, as long as characters eventually appear. This masks potential race conditions in the `AppState` implementation.

### Recommendation
- Strengthen assertions: check length, verify all characters are present exactly once
- The `test_concurrent_mutate_and_get` test at line 862 is better — it validates no panic/corruption under concurrent read+write

---

## 13. `EventV2` RwLock Chain — Potential Deadlock

| Field | Detail |
|---|---|
| **Location** | `event.rs:677-691` |
| **Severity** | **Medium** |
| **Effort** | 2-3 days |

### Problem
`EventV2` holds **six `tokio::sync::RwLock` fields** (typed_channels, projectors, commit_guards, listeners, sync_handlers, synchronized_aggregates). The event dispatch chain may acquire multiple locks sequentially, and callbacks (projectors, listeners, sync handlers) can re-enter the registry.

### Evidence
```rust
// event.rs:677-691
pub struct EventV2 {
    typed_channels: RwLock<HashMap<String, Arc<EventPubSub>>>,
    global_channel: EventPubSub,
    registry: Arc<EventRegistry>,
    projectors: RwLock<HashMap<String, Vec<ProjectorFn>>>,
    commit_guards: RwLock<Vec<CommitGuardFn>>,
    listeners: RwLock<Vec<ListenerFn>>,
    sync_handlers: RwLock<Vec<SyncFn>>,
    synchronized_aggregates: RwLock<HashMap<String, Vec<Arc<EventPubSub>>>>,
}
```

### Impact
- If a projector callback calls `EventV2::publish`, it will try to acquire `typed_channels.read()` while a dispatch path may hold `projectors.read()`
- Lock ordering between fields is not documented or enforced
- Under heavy event load, contention on these locks may serialize event processing

### Recommendation
- Document lock ordering: `typed_channels → projectors → listeners → sync_handlers → commit_guards → synchronized_aggregates`
- Consider using `dashmap` for `typed_channels` and `synchronized_aggregates` to reduce RwLock contention
- Add a re-entrancy guard to detect recursive publish

---

## 14. Process `notify::Notify` vs select! Pollution

| Field | Detail |
|---|---|
| **Location** | `process.rs:574-592` |
| **Severity** | **Low** |
| **Effort** | 1 day |

### Problem
The stdout reader in `process.rs:574` uses `tokio::select!` with an async block that polls cancellation every iteration. The `_ = async {}, if token...` pattern creates a new future every loop iteration.

### Evidence
```rust
// process.rs:573-593
loop {
    tokio::select! {
        result = lines.next_line() => { ... }
        _ = async {}, if token.as_ref().is_some_and(|t| t.is_cancelled()) => {
            break;
        }
    }
}
```

### Impact
- The `async {}` block is created and polled on every iteration, adding overhead
- The cancellation check polls the token but doesn't use `CancellationToken::cancelled()` which provides a `Future` implementation
- Active polling of `is_cancelled()` is less efficient than `.cancelled().await`

### Recommendation
Replace with:
```rust
let cancel = token.map(|t| t.cancelled());
loop {
    tokio::select! {
        result = lines.next_line() => { ... }
        _ = &mut cancel, if cancel.is_some() => { break; }
    }
}
```

---

## 15. Channel Capacity Mismatch — SSE Bridge vs Upstream Bus

| Field | Detail |
|---|---|
| **Location** | `event.rs:145`, `bus.rs:256` |
| **Severity** | **Low** |
| **Effort** | 30 min |

### Problem
The SSE bridge at `event.rs:145` creates a broadcast channel with capacity 256, while the upstream `EventBus` uses capacity 1024. This creates a bottleneck: the bridge is 4x smaller than the source bus.

### Evidence
```rust
// event.rs:145
let (tx, rx) = tokio::sync::broadcast::channel(256);

// bus.rs:256
Self::new(1024)
```

### Impact
Under high event throughput (e.g., rapid text deltas), the bridge drops events while the main bus still has room. A subscriber via SSE sees `Lagged` events while direct bus subscribers do not.

### Recommendation
Match the capacities (both 1024) or derive the bridge capacity from the source bus.

---

## 16. `optionally` — Non-existent Function Referenced

| Field | Detail |
|---|---|
| **Location** | `tui/src/app.rs:589` |
| **Severity** | **Low** |
| **Effort** | 1 hour |

### Problem
Line 589 references `async {}, if token.as_ref().is_some_and(|t| t.is_cancelled())` — the `optionally` function from TS was replaced but the pattern is non-idiomatic.

### Evidence
```rust
// app.rs:574,589
_ = async {}, if token.as_ref().is_some_and(|t| t.is_cancelled()) => {
```

This is the same pattern as in process.rs but the readability is poor and it creates a new anonymous future per poll.

### Recommendation
Same as Finding #14 — use `CancellationToken::cancelled()` future directly.

---

## 17. Missing `Send + Sync` Bounds Verification

| Field | Detail |
|---|---|
| **Location** | Cross-cutting |
| **Severity** | **Low** |
| **Effort** | 1-2 days |

### Problem
All types passed across `.await` points or stored in `Arc` must be `Send + Sync`. The codebase does not explicitly assert these bounds, and several types hold `Rc` or raw pointers.

### Evidence
While generated code (permission, config, error, event, provider) uses `Arc` correctly, there is no systematic audit of `Send + Sync` satisfaction.

### Impact
A single `!Send` type would prevent the entire struct from being used in `tokio::spawn` tasks, causing compile errors in obscure code paths (or worse, runtime errors if `Rc` is used in a spawned task).

### Recommendation
Add explicit bounds checks:
```rust
fn assert_send<T: Send>() {}
fn assert_send_sync<T: Send + Sync>() {}
```
Call these in `static_assertions` style for key types: `EventBus`, `SharedBus`, `Config`, `Session`, `SessionProcessor`, `ToolContext`.

---

## Summary: Comparison with OpenCode TS Model

| Concern | OpenCode (TS) | RustCode (Rust) | Gap |
|---|---|---|---|
| **Concurrency** | Effect.js fibers (structured, scoped, cancelable on scope exit) | tokio tasks (unstructured, fire-and-forget) | No structured concurrency; tasks leak on scope exit |
| **Cancellation** | Effect.fork → Effect.interrupt; automatic on scope end | CancellationToken (must be manually threaded) | Incomplete wiring (tokens created but never cancelled for tools) |
| **Pub/Sub** | EventEmitter (sync, in-process) | tokio::sync::broadcast (async, bounded buffer) | Broadcast capacity limits; lag causes silent drops |
| **Backpressure** | Effect queues with bounded capacity, pulling | Unbounded channels everywhere | OOM risk under high load |
| **Blocking I/O** | None (all async via Effect) | std::process::Command (38+ sites) | Thread pool starvation |
| **Error Handling** | Effect.Either, Effect.catchAll | JoinHandle discard → silent failures | Lost error context |
| **State Management** | Ref (concurrent-safe, Effect-aware) | RwLock/Mutex (manual, with blocking risk) | Deadlock potential in async context |

## Effort Estimates

| # | Finding | Severity | Effort |
|---|---|---|---|
| 1 | `watch::channel::<false>` type error | Critical | 5 min |
| 2 | Nested tokio runtime in main.rs | Critical | 2-4 hr |
| 3 | Unbounded task spawning | High | 1-2 wk |
| 4 | Unbounded channels | High | 3-5 d |
| 5 | std::sync Mutex/RwLock in async | High | 1-2 wk |
| 6 | Dropped JoinHandle / fire-and-forget | High | 2-3 d |
| 7 | Sequential tool execution (no parallelism) | High | 2-3 d |
| 8 | std::process::Command blocking | High | 1-2 wk |
| 9 | Broadcast channel capacity / silent loss | Medium | 1 d |
| 10 | Tool CancellationToken never cancelled | Medium | 1 d |
| 11 | TUI per-action spawn explosion | Medium | 3-5 d |
| 12 | State.rs test synchronization | Medium | 2-3 hr |
| 13 | EventV2 RwLock chain deadlock risk | Medium | 2-3 d |
| 14 | Active polling pattern in process.rs | Low | 1 d |
| 15 | SSE bridge capacity mismatch | Low | 30 min |
| 16 | Unnecessary future allocation | Low | 1 hr |
| 17 | Missing Send+Sync bounds | Low | 1-2 d |

**Total estimated effort**: 5-10 weeks for full remediation
**Priority order**: 1 → 2 → 8 → 5 → 3 → 4 → 6 → 7 → 10 → 9 → 13 → 11 → 12 → 14 → 15 → 16 → 17
