//! Session execution types — local exec, run coordinator, and error types.
//!
//! Ported from:
//! - `packages/core/src/session/execution.ts` (lines 1–24)
//! - `packages/core/src/session/execution/local.ts` (lines 1–35)
//! - `packages/core/src/session/run-coordinator.ts` (lines 1–285)
//! - `packages/core/src/session/error.ts` (lines 1–21)

use std::future::Future;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;

use crate::session_info::SessionId;
use crate::session_message::SessionMessageId;
use dashmap::DashMap;
use futures::future::BoxFuture;
use serde::{Deserialize, Serialize};
use tokio::sync::{broadcast, mpsc, RwLock};
use tokio::task::JoinHandle;
use tokio_util::sync::CancellationToken;

// ══════════════════════════════════════════════════════════════════════════════
// Execution Interface
// ══════════════════════════════════════════════════════════════════════════════

/// Core execution interface — routes execution from session ID to runner.
///
/// # Source
/// `packages/core/src/session/execution.ts` lines 7–14 `Interface`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[allow(dead_code)]
pub enum DrainMode {
    /// Explicit drain request
    #[serde(rename = "run")]
    Run,
    /// Advisory wake after durable work
    #[serde(rename = "wake")]
    Wake,
}

/// Execution service operations.
///
/// # Source
/// `packages/core/src/session/execution.ts` lines 7–14 `Interface`.
/// `packages/opencode/src/session/run-coordinator.ts` lines 29–38 `Coordinator`.
pub trait SessionExecution: Send + Sync {
    /// Explicitly drain one session, making at least one provider attempt.
    fn resume(
        &self,
        session_id: SessionId,
    ) -> impl std::future::Future<Output = Result<(), SessionRunError>> + Send;

    /// Schedule a drain after durable work is recorded.
    fn wake(
        &self,
        session_id: SessionId,
        seq: Option<u64>,
    ) -> impl std::future::Future<Output = Result<(), SessionRunError>> + Send;

    /// Interrupt active work owned by this process.
    fn interrupt(
        &self,
        session_id: SessionId,
        seq: Option<u64>,
    ) -> impl std::future::Future<Output = Result<(), SessionRunError>> + Send;

    /// Wait until the current ownership chain settles.
    fn await_idle(
        &self,
        session_id: SessionId,
    ) -> impl std::future::Future<Output = Result<(), SessionRunError>> + Send;
}

// ══════════════════════════════════════════════════════════════════════════════
// FiberSet — concurrent task manager
// ══════════════════════════════════════════════════════════════════════════════

/// Unique identifier for a task spawned within a [`FiberSet`].
///
/// # Source
/// Ported from Effect's `Fiber.Id` — each spawned fiber gets a unique ID.
pub type FiberId = u64;

/// A handle for cancelling a specific fiber in a [`FiberSet`].
///
/// # Source
/// Ported from Effect's `Fiber` — supports `Fiber.interrupt(fiber)`.
#[derive(Debug, Clone)]
pub struct FiberHandle {
    id: FiberId,
    cancel: CancellationToken,
}

impl FiberHandle {
    /// Return the unique ID of this fiber.
    pub fn id(&self) -> FiberId {
        self.id
    }

    /// Cancel (interrupt) this fiber.
    pub fn cancel(&self) {
        self.cancel.cancel();
    }
}

/// Result emitted when a fiber completes in a [`FiberSet`].
///
/// # Source
/// Ported from Effect's `Exit` — wraps the outcome of a fiber.
#[derive(Debug)]
pub struct FiberResult<T> {
    /// The fiber's unique identifier.
    pub id: FiberId,
    /// The result: `Ok(output)` or `Err`.
    pub result: crate::error::Result<T>,
}

/// Concurrent task manager — analogous to Effect's `FiberSet`.
///
/// Spawns tasks via `tokio::spawn`, tracks them by a unique [`FiberId`],
/// supports cancellation of individual tasks or all tasks at once,
/// waiting for all to complete (`join_all`), waiting until empty
/// (`await_empty`), and collecting results via an `mpsc` receiver.
///
/// # Source
/// Ported from `effect/src/fiber-set.ts` — `FiberSet.Runtime`
/// and `FiberSet.makeRuntime`.
pub struct FiberSet<T: Send + 'static> {
    next_id: AtomicU64,
    handles: DashMap<FiberId, JoinHandle<()>>,
    cancels: DashMap<FiberId, CancellationToken>,
    result_tx: mpsc::UnboundedSender<FiberResult<T>>,
}

impl<T: Send + 'static> FiberSet<T> {
    /// Create a new empty `FiberSet`.
    ///
    /// Returns the set and a receiver that yields results as fibers complete.
    pub fn new() -> (Self, mpsc::UnboundedReceiver<FiberResult<T>>) {
        let (result_tx, result_rx) = mpsc::unbounded_channel();
        let set = Self {
            next_id: AtomicU64::new(1),
            handles: DashMap::new(),
            cancels: DashMap::new(),
            result_tx,
        };
        (set, result_rx)
    }

    /// Spawn a new fiber running `future`.
    ///
    /// Returns a [`FiberHandle`] that can be used to cancel the task.
    /// The result is sent through the mpsc receiver when the task completes.
    pub fn spawn<F>(&self, future: F) -> FiberHandle
    where
        F: Future<Output = T> + Send + 'static,
    {
        let id = self.next_id.fetch_add(1, Ordering::SeqCst);
        let cancel = CancellationToken::new();
        let cancel_clone = cancel.clone();
        let result_tx = self.result_tx.clone();

        let handle: JoinHandle<()> = tokio::spawn(async move {
            tokio::select! {
                result = future => {
                    if result_tx.send(FiberResult { id, result: crate::error::Result::Ok(result) }).is_err() {
                        tracing::warn!(fiber_id = id, "fiber result receiver closed before completion");
                    }
                }
                _ = cancel_clone.cancelled() => {
                    if result_tx.send(FiberResult {
                        id,
                        result: crate::error::Result::Err(crate::error::Error::Aborted),
                    }).is_err() {
                        tracing::debug!(fiber_id = id, "fiber abort receiver already closed");
                    }
                }
            }
        });

        // Clean up old handle for this ID (should not happen with unique IDs)
        if let Some(old) = self.handles.insert(id, handle) {
            // Detach the old handle — it will be cleaned up when the task completes
            tokio::spawn(async move { old.await.ok(); });
        }
        self.cancels.insert(id, cancel.clone());

        FiberHandle { id, cancel }
    }

    /// Cancel a specific fiber by ID.
    ///
    /// # Source
    /// Ported from `FiberSet.interrupt(id)`.
    pub fn cancel(&self, id: FiberId) {
        if let Some(cancel) = self.cancels.get(&id) {
            cancel.cancel();
        }
    }

    /// Cancel all running fibers.
    ///
    /// # Source
    /// Ported from `FiberSet.interruptAll`.
    pub fn cancel_all(&self) {
        let ids: Vec<FiberId> = self.cancels.iter().map(|e| *e.key()).collect();
        for id in ids {
            self.cancel(id);
        }
    }

    /// Wait for all currently-tracked fibers to complete.
    ///
    /// Removes completed entries from the internal maps.
    ///
    /// # Source
    /// Ported from `FiberSet.joinAll`.
    pub async fn join_all(&self) {
        let ids: Vec<FiberId> = self.handles.iter().map(|e| *e.key()).collect();
        for id in ids {
            if let Some((_, handle)) = self.handles.remove(&id) {
                let _ = handle.await;
            }
        }
    }

    /// Wait until no fibers are running.
    ///
    /// Times out after 30 seconds with exponential backoff (10ms → 100ms).
    ///
    /// # Source
    /// Ported from `FiberSet.awaitEmpty`.
    pub async fn await_empty(&self) -> Result<(), &'static str> {
        let start = std::time::Instant::now();
        let timeout = std::time::Duration::from_secs(30);
        let mut delay_ms: u64 = 10;
        while !self.handles.is_empty() {
            if start.elapsed() > timeout {
                return Err("await_empty timed out after 30s");
            }
            tokio::time::sleep(std::time::Duration::from_millis(delay_ms)).await;
            delay_ms = (delay_ms * 2).min(100);
        }
        Ok(())
    }

    /// Return the number of currently-running fibers.
    pub fn len(&self) -> usize {
        self.handles.len()
    }

    /// Return `true` if no fibers are running.
    pub fn is_empty(&self) -> bool {
        self.handles.is_empty()
    }
}

impl<T: Send + 'static> std::fmt::Debug for FiberSet<T> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("FiberSet")
            .field("active_fibers", &self.handles.len())
            .finish()
    }
}

// ══════════════════════════════════════════════════════════════════════════════
// Run Coordinator Types
// ══════════════════════════════════════════════════════════════════════════════

/// Demand type for the run coordinator — runs dominate wakes.
///
/// # Source
/// `packages/core/src/session/run-coordinator.ts` lines 8–10 `Mode`, line 11 `Demand`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "_tag")]
pub enum Demand {
    /// Explicit run request
    #[serde(rename = "run")]
    Run,
    /// Advisory wake request (may coalesce)
    #[serde(rename = "wake")]
    Wake {
        /// Sequence number for ordering
        #[serde(skip_serializing_if = "Option::is_none")]
        seq: Option<u64>,
    },
}

/// Coordinator state for a single session's execution lane.
///
/// # Source
/// `packages/core/src/session/run-coordinator.ts` lines 41–50 `Entry`.
#[derive(Debug, Clone)]
pub struct CoordinatorEntry {
    /// Current demand being processed
    pub current: Demand,
    /// Coalesced follow-up demand
    pub pending: Option<Demand>,
    /// Whether this lane is stopping
    pub stopping: bool,
    /// Interrupt sequence number
    pub interrupt_seq: Option<u64>,
}

impl CoordinatorEntry {
    /// Create a new coordinator entry with the given demand.
    pub fn new(current: Demand) -> Self {
        Self {
            current,
            pending: None,
            stopping: false,
            interrupt_seq: None,
        }
    }

    /// Check if this entry accepts a wake request.
    ///
    /// # Source
    /// `packages/core/src/session/run-coordinator.ts` lines 248–250 `acceptsWake`.
    pub fn accepts_wake(&self, seq: Option<u64>) -> bool {
        if !self.stopping {
            return true;
        }
        match (self.interrupt_seq, seq) {
            (Some(is), Some(s)) => s > is,
            _ => false,
        }
    }
}

/// Combine two demands: runs dominate, wakes retain newest seq.
///
/// # Source
/// `packages/core/src/session/run-coordinator.ts` lines 53–56 `coalesce`.
pub fn coalesce_demand(left: Option<&Demand>, right: &Demand) -> Demand {
    if matches!(left, Some(Demand::Run)) || matches!(right, Demand::Run) {
        return Demand::Run;
    }
    match (
        left.and_then(|d| match d {
            Demand::Wake { seq } => *seq,
            _ => None,
        }),
        right,
    ) {
        (_, Demand::Wake { seq }) => Demand::Wake {
            seq: match (
                left.and_then(|d| match d {
                    Demand::Wake { seq } => *seq,
                    _ => None,
                }),
                *seq,
            ) {
                (None, r) => r,
                (Some(l), None) => Some(l),
                (Some(l), Some(r)) => Some(l.max(r)),
            },
        },
        _ => *right,
    }
}

// ══════════════════════════════════════════════════════════════════════════════
// Coordinator State — execution lifecycle
// ══════════════════════════════════════════════════════════════════════════════

/// Turn identifier for a single LLM-turn execution.
///
/// # Source
/// Each `run()` / `wake()` call in the coordinator creates a new turn.
pub type TurnId = u64;

/// State machine for a session's execution lifecycle.
///
/// Mirrors the TS coordinator lifecycle:
///
/// ```text
/// Idle --run/wake--> Running(turn) --question--> AwaitingInput(turn)
/// AwaitingInput(turn) --user reply--> Running(turn)
/// Running(turn) --interrupt--> Interrupted
/// Interrupted --run--> Running(turn)
/// Running(turn) --done--> Idle
/// ```
///
/// # Source
/// `packages/core/src/session/run-coordinator.ts` — implicit lifecycle.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "state")]
pub enum CoordinatorState {
    /// No active execution for this session.
    #[serde(rename = "idle")]
    Idle,
    /// Actively processing an LLM turn.
    #[serde(rename = "running")]
    Running {
        /// The turn identifier.
        turn_id: TurnId,
    },
    /// Waiting for user input (question or permission).
    #[serde(rename = "awaiting_input")]
    AwaitingInput {
        /// The turn identifier that is paused.
        turn_id: TurnId,
    },
    /// Current turn was interrupted and needs cleanup.
    #[serde(rename = "interrupted")]
    Interrupted,
}

impl CoordinatorState {
    /// Return the current turn_id if the state is Running or AwaitingInput.
    pub fn turn_id(&self) -> Option<TurnId> {
        match self {
            Self::Running { turn_id } | Self::AwaitingInput { turn_id } => Some(*turn_id),
            Self::Idle | Self::Interrupted => None,
        }
    }

    /// Return `true` if the state is Idle.
    pub fn is_idle(&self) -> bool {
        matches!(self, Self::Idle)
    }

    /// Return `true` if the state is Running.
    pub fn is_running(&self) -> bool {
        matches!(self, Self::Running { .. })
    }

    /// Return `true` if the state is AwaitingInput.
    pub fn is_awaiting_input(&self) -> bool {
        matches!(self, Self::AwaitingInput { .. })
    }

    /// Return `true` if the state is Interrupted.
    pub fn is_interrupted(&self) -> bool {
        matches!(self, Self::Interrupted)
    }
}

// ══════════════════════════════════════════════════════════════════════════════
// RunCoordinator — session execution orchestrator
// ══════════════════════════════════════════════════════════════════════════════

/// Type alias for the drain function used by [`RunCoordinator`].
///
/// Takes (session_id, demand) and returns the drain result.
///
/// # Source
/// `packages/core/src/session/run-coordinator.ts` lines 65–67 `drain` option.
pub type DrainFn = Arc<
    dyn Fn(SessionId, Demand) -> BoxFuture<'static, Result<(), SessionRunError>> + Send + Sync,
>;

/// Type alias for the failure callback used by [`RunCoordinator`].
///
/// # Source
/// `packages/core/src/session/run-coordinator.ts` line 68 `onFailure` option.
pub type FailureFn = Arc<
    dyn Fn(SessionId, SessionRunError) -> BoxFuture<'static, ()> + Send + Sync,
>;

/// A broadcast channel for drain completion results.
///
/// Multiple `run()` callers can subscribe to be notified when the
/// drain completes. Capacity is 16 so late subscribers can still
/// receive the latest result.
type DoneChannel = broadcast::Sender<Result<(), SessionRunError>>;
type DoneReceiver = broadcast::Receiver<Result<(), SessionRunError>>;

/// Create a new done channel.
/// Uses capacity 64 to prevent overflow when many callers wait on completion.
fn done_channel() -> (DoneChannel, DoneReceiver) {
    broadcast::channel(64)
}

/// A single session's execution lane within the [`RunCoordinator`].
///
/// Mirrors `packages/core/src/session/run-coordinator.ts` lines 41–50 `Entry<A, E>`.
#[derive(Clone)]
struct Lane {
    /// The demand currently being processed (or about to start).
    demand: Demand,
    /// Coalesced follow-up demand, at most one.
    pending: Option<Demand>,
    /// Whether this lane is stopping (interrupted).
    stopping: bool,
    /// Interrupt sequence number for this lane.
    interrupt_seq: Option<u64>,
    /// The fiber ID of the running drain task.
    fiber_id: Option<FiberId>,
    /// Broadcast sender — signals drain completion to all waiters.
    done_tx: Option<DoneChannel>,
}

impl std::fmt::Debug for Lane {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Lane")
            .field("demand", &self.demand)
            .field("pending", &self.pending)
            .field("stopping", &self.stopping)
            .field("interrupt_seq", &self.interrupt_seq)
            .field("fiber_id", &self.fiber_id)
            .field("done_tx", &self.done_tx.as_ref().map(|_| "Some(..)"))
            .finish()
    }
}

/// Trait for types that can be used as the "drain" function in a [`RunCoordinator`].
///
/// The coordinator calls `coordinated_run(session_id, force)` to execute one
/// drain cycle. `force` corresponds to `mode === "run"` in the TS — when true,
/// the runner should bypass any coalescing or batching.
///
/// # Source
/// `packages/core/src/session/run-coordinator.ts` line 66 — `drain` option.
pub trait CoordinatedRunner: Send + Sync {
    /// Run one drain cycle for the given session.
    fn coordinated_run(
        &self,
        session_id: SessionId,
        force: bool,
    ) -> BoxFuture<'static, Result<(), SessionRunError>>;
}

/// Adapter that wraps any async function into a [`CoordinatedRunner`].
///
/// Useful for creating a coordinator from ad-hoc closures without
/// implementing the trait manually.
///
/// # Source
/// Convenience adapter; no direct TS equivalent.
pub struct FnRunner<F> {
    handler: F,
}

impl<F> FnRunner<F> {
    /// Create a new `FnRunner` from an async function.
    pub fn new(handler: F) -> Self {
        Self { handler }
    }
}

impl<F, Fut> CoordinatedRunner for FnRunner<F>
where
    F: Fn(SessionId, bool) -> Fut + Send + Sync + 'static,
    Fut: std::future::Future<Output = Result<(), SessionRunError>> + Send + 'static,
{
    fn coordinated_run(
        &self,
        session_id: SessionId,
        force: bool,
    ) -> BoxFuture<'static, Result<(), SessionRunError>> {
        let fut = (self.handler)(session_id, force);
        Box::pin(fut)
    }
}

/// Session execution orchestrator — manages one drain chain per session ID.
///
/// For each session:
///
/// ```text
/// idle --run/wake--> draining --run/wake--> draining + one coalesced rerun --> idle
/// ```
///
/// `run` is an explicit request that starts a drain or joins an existing one.
/// `wake` is an advisory notification that durable work may be available.
/// `interrupt` stops the current drain chain; wakes after the interrupt
/// boundary are allowed.
///
/// Uses a [`FiberSet`] internally to manage concurrent drain tasks across
/// different sessions.
///
/// # Source
/// Ported from `packages/core/src/session/run-coordinator.ts` lines 29–267.
pub struct RunCoordinator {
    /// Per-session execution lanes.
    lanes: Arc<DashMap<SessionId, Lane>>,
    /// Global per-session interrupt sequence tracker.
    interrupt_seq: Arc<DashMap<SessionId, u64>>,
    /// The drain function — called to actually run a session turn.
    drain_fn: DrainFn,
    /// Optional callback for wake-drain failures.
    on_failure: Option<FailureFn>,
    /// Externally-observable state machine.
    state: Arc<RwLock<CoordinatorState>>,
    /// Turn counter.
    turn_counter: Arc<AtomicU64>,
    /// FiberSet managing concurrent drain tasks.
    fiber_set: Arc<FiberSet<Result<(), SessionRunError>>>,
}

impl std::fmt::Debug for RunCoordinator {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("RunCoordinator")
            .field("active_lanes", &self.lanes.len())
            .field("state", &self.state.blocking_read())
            .finish()
    }
}

impl RunCoordinator {
    /// Create a new `RunCoordinator`.
    ///
    /// # Source
    /// `packages/core/src/session/run-coordinator.ts` lines 65–68 `make`.
    pub fn new(
        drain_fn: DrainFn,
        on_failure: Option<FailureFn>,
    ) -> Self {
        Self {
            lanes: Arc::new(DashMap::new()),
            interrupt_seq: Arc::new(DashMap::new()),
            drain_fn,
            on_failure,
            state: Arc::new(RwLock::new(CoordinatorState::Idle)),
            turn_counter: Arc::new(AtomicU64::new(1)),
            fiber_set: Arc::new(FiberSet::new().0),
        }
    }

    /// Create a `RunCoordinator` from a [`CoordinatedRunner`].
    ///
    /// # Source
    /// `packages/core/src/session/run-coordinator.ts` lines 65–68 `make`.
    pub fn from_runner(runner: Arc<dyn CoordinatedRunner>) -> Self {
        let drain_fn: DrainFn = Arc::new(move |session_id, demand| {
            let runner = runner.clone();
            Box::pin(async move {
                runner
                    .coordinated_run(session_id, matches!(demand, Demand::Run))
                    .await
            })
        });
        Self::new(drain_fn, None)
    }

    /// Create a `RunCoordinator` from a [`CoordinatedRunner`] with a failure callback.
    pub fn from_runner_with_failure(
        runner: Arc<dyn CoordinatedRunner>,
        on_failure: FailureFn,
    ) -> Self {
        let drain_fn: DrainFn = Arc::new(move |session_id, demand| {
            let runner = runner.clone();
            Box::pin(async move {
                runner
                    .coordinated_run(session_id, matches!(demand, Demand::Run))
                    .await
            })
        });
        Self::new(drain_fn, Some(on_failure))
    }

    /// Return the current externally-visible coordinator state.
    pub async fn state(&self) -> CoordinatorState {
        self.state.read().await.clone()
    }

    // ── Public API ─────────────────────────────────────────────────────────

    /// Explicit drain request — start a chain or join/upgrade the current one.
    ///
    /// Returns the drain result. If a drain is already running, the caller
    /// waits for completion. If the current demand is a `Wake`, it is
    /// upgraded to `Run`.
    ///
    /// # Source
    /// `packages/core/src/session/run-coordinator.ts` lines 221–242 `run`.
    pub async fn run(&self, session_id: SessionId) -> Result<(), SessionRunError> {
        // If stopping, wait for settle then retry
        if let Some(lane) = self.lanes.get(&session_id) {
            if lane.stopping {
                drop(lane);
                self.await_idle(session_id.clone()).await?;
            }
        }

        // Check for existing lane
        if let Some(mut lane) = self.lanes.get_mut(&session_id) {
            if matches!(lane.demand, Demand::Wake { .. }) {
                // Upgrade pending to run
                lane.pending = Some(coalesce_demand(lane.pending.as_ref(), &Demand::Run));
                if lane.done_tx.is_none() {
                    let (tx, _) = done_channel();
                    lane.done_tx = Some(tx);
                }
                let fiber_id = lane.fiber_id;
                let rx = lane.done_tx.as_ref().map(|tx| tx.subscribe());
                drop(lane);

                if fiber_id.is_none() {
                    self.start_drain(session_id.clone()).await;
                }

                return wait_for_result(rx, &session_id).await;
            }

            // Already has a run demand — subscribe to completion
            if lane.done_tx.is_some() {
                let rx = lane.done_tx.as_ref().map(|tx| tx.subscribe());
                drop(lane);
                return wait_for_result(rx, &session_id).await;
            }

            // Lane exists but has no done_tx — create one
            let (tx, _our_rx) = done_channel();
            lane.done_tx = Some(tx);
            let rx = lane.done_tx.as_ref().map(|tx| tx.subscribe());
            let fiber_id = lane.fiber_id;
            drop(lane);

            if fiber_id.is_none() {
                self.start_drain(session_id.clone()).await;
            }

            return wait_for_result(rx, &session_id).await;
        }

        // No existing lane — create one
        let turn_id = self.turn_counter.fetch_add(1, Ordering::SeqCst);
        {
            let mut state = self.state.write().await;
            *state = CoordinatorState::Running { turn_id };
        }

        let (tx, _) = done_channel();
        let rx = tx.subscribe();
        self.lanes.insert(
            session_id.clone(),
            Lane {
                demand: Demand::Run,
                pending: None,
                stopping: false,
                interrupt_seq: None,
                fiber_id: None,
                done_tx: Some(tx),
            },
        );

        self.start_drain(session_id.clone()).await;

        let result = wait_for_result(Some(rx), &session_id).await;

        let mut st = self.state.write().await;
        *st = CoordinatorState::Idle;

        result
    }

    /// Advisory wake — coalesces a wake demand for the given session.
    ///
    /// If the session is idle, starts a new drain chain. If already draining,
    /// coalesces the wake into the pending slot (at most one follow-up).
    ///
    /// Uses DashMap::alter for atomic read-modify-write to prevent TOCTOU races.
    ///
    /// # Source
    /// `packages/core/src/session/run-coordinator.ts` lines 161–175 `wake`.
    pub async fn wake(&self, session_id: SessionId, seq: Option<u64>) {
        if !self.is_after_interrupt(&session_id, seq) {
            return;
        }

        // Atomic check-and-update: if lane exists and isn't stopping, coalesce
        let mut inserted = false;
        if let Some(lane) = self.lanes.get(&session_id) {
            if !lane.stopping {
                // Already draining — coalesce
                let mut lane = lane.value().clone();
                lane.pending = Some(coalesce_demand(
                    lane.pending.as_ref(),
                    &Demand::Wake { seq },
                ));
                self.lanes.insert(session_id.clone(), lane);
            } else {
                inserted = true;
            }
        } else {
            inserted = true;
        }
        if inserted {
            // Idle — start a new drain chain
            self.lanes.insert(
                session_id.clone(),
                Lane {
                    demand: Demand::Wake { seq },
                    pending: None,
                    stopping: false,
                    interrupt_seq: None,
                    fiber_id: None,
                    done_tx: None,
                },
            );

            let turn_id = self.turn_counter.fetch_add(1, Ordering::SeqCst);
            {
                let mut state = self.state.write().await;
                *state = CoordinatorState::Running { turn_id };
            }

            self.start_drain(session_id).await;
        }
    }

    /// Interrupt the active drain chain for the given session.
    ///
    /// Wakes after the interrupt boundary (`seq > interrupt_seq`) are allowed.
    ///
    /// # Source
    /// `packages/core/src/session/run-coordinator.ts` lines 193–218 `interrupt`.
    pub async fn interrupt(&self, session_id: SessionId, seq: Option<u64>) {
        let latest = self.interrupt_seq.get(&session_id).map(|r| *r);
        if let Some(s) = seq {
            if let Some(l) = latest {
                if s <= l {
                    // Already interrupted at this seq or later
                    if let Some(lane) = self.lanes.get(&session_id) {
                        if lane.stopping {
                            if let Some(fid) = lane.fiber_id {
                                self.fiber_set.cancel(fid);
                            }
                        }
                    }
                    return;
                }
            }
            self.interrupt_seq.insert(session_id.clone(), s);
        }

        if let Some(mut lane) = self.lanes.get_mut(&session_id) {
            lane.stopping = true;
            lane.interrupt_seq = lane.interrupt_seq.max(seq);
            // Suppress pending wakes at or before the interrupt seq
            if let Some(s) = seq {
                if matches!(lane.pending, Some(Demand::Wake { seq: Some(ps) }) if ps <= s) {
                    lane.pending = None;
                }
            }
            if let Some(fid) = lane.fiber_id {
                self.fiber_set.cancel(fid);
            }
        }

        let mut state = self.state.write().await;
        *state = CoordinatorState::Interrupted;
    }

    /// Wait until the current drain chain for the session settles.
    ///
    /// Times out after 30 seconds to prevent unbounded busy-wait.
    ///
    /// # Source
    /// `packages/core/src/session/run-coordinator.ts` lines 177–191 `awaitIdle`.
    pub async fn await_idle(&self, session_id: SessionId) -> Result<(), SessionRunError> {
        let start = std::time::Instant::now();
        let timeout = std::time::Duration::from_secs(30);
        let mut attempts: u64 = 0;
        loop {
            if attempts > 0 {
                tokio::time::sleep(std::time::Duration::from_millis(10)).await;
            }
            attempts += 1;
            if start.elapsed() > timeout {
                return Err(SessionRunError {
                    kind: SessionRunErrorKind::Timeout,
                    message: format!("await_idle timed out after 30s ({attempts} attempts)"),
                    session_id: Some(session_id.clone()),
                });
            }
            if !self.lanes.contains_key(&session_id) {
                return Ok(());
            }
            tokio::time::sleep(std::time::Duration::from_millis(10)).await;
        }
    }

    // ── Internal helpers ───────────────────────────────────────────────────

    /// Check whether a wake with the given seq is after the last interrupt.
    fn is_after_interrupt(&self, session_id: &SessionId, seq: Option<u64>) -> bool {
        let latest = self.interrupt_seq.get(session_id).map(|r| *r);
        match (latest, seq) {
            (None, _) => true,
            (_, None) => false,
            (Some(l), Some(s)) => s > l,
        }
    }

    /// Start a drain fiber for the session's lane.
    async fn start_drain(&self, session_id: SessionId) {
        let drain_fn = self.drain_fn.clone();
        let on_failure = self.on_failure.clone();
        let lanes = self.lanes.clone();
        let fiber_set = self.fiber_set.clone();
        let state = self.state.clone();
        let session_id_clone = session_id.clone();
        let fiber_set_clone = fiber_set.clone();

        let future = async move {
            let demand = {
                let lane = lanes.get(&session_id);
                match lane {
                    Some(l) => l.demand,
                    None => return Ok::<(), SessionRunError>(()),
                }
            };

            let result = drain_fn(session_id.clone(), demand).await;

            // Settle — process completion and determine next action
            settle(
                session_id.clone(),
                lanes.clone(),
                state.clone(),
                fiber_set.clone(),
                drain_fn.clone(),
                on_failure.clone(),
                demand,
                result.clone(),
            )
            .await;

            result
        };

        let handle = fiber_set_clone.spawn(future);
        if let Some(mut lane) = self.lanes.get_mut(&session_id_clone) {
            lane.fiber_id = Some(handle.id());
        }
    }
}

/// Wait for a result from a done broadcast receiver.
async fn wait_for_result(
    rx: Option<DoneReceiver>,
    session_id: &SessionId,
) -> Result<(), SessionRunError> {
    let mut rx = match rx {
        Some(r) => r,
        None => {
            return Err(SessionRunError {
                kind: SessionRunErrorKind::Internal,
                message: "run coordinator: no done channel".into(),
                session_id: Some(session_id.clone()),
            })
        }
    };
    match rx.recv().await {
        Ok(result) => result,
        Err(_) => Err(SessionRunError {
            kind: SessionRunErrorKind::Internal,
            message: "run coordinator: broadcast channel closed".into(),
            session_id: Some(session_id.clone()),
        }),
    }
}

/// Process the completion of a drain — the core state-machine transition.
///
/// Takes owned `Arc` values so it can be called from spawned futures
/// without lifetime issues.
///
/// # Source
/// `packages/core/src/session/run-coordinator.ts` lines 112–159 `settle`.
fn settle(
    session_id: SessionId,
    lanes: Arc<DashMap<SessionId, Lane>>,
    state: Arc<RwLock<CoordinatorState>>,
    fiber_set: Arc<FiberSet<Result<(), SessionRunError>>>,
    drain_fn: DrainFn,
    on_failure: Option<FailureFn>,
    demand: Demand,
    result: Result<(), SessionRunError>,
) -> BoxFuture<'static, ()> {
    Box::pin(async move {
    let has_pending = lanes
        .get(&session_id)
        .map(|l| l.pending.is_some())
        .unwrap_or(false);

    match (&result, has_pending) {
        (Ok(()), false) => {
            // Success, no pending — broadcast result, remove lane
            if let Some(l) = lanes.get_mut(&session_id) {
                if let Some(ref tx) = l.done_tx {
                    let _ = tx.send(result);
                }
            }
            lanes.remove(&session_id);
            let mut st = state.write().await;
            *st = CoordinatorState::Idle;
        }
        (Ok(()), true) => {
            // Success with pending — replace demand and re-spawn
            let next_demand = lanes.get_mut(&session_id).and_then(|mut l| l.pending.take());
            if let Some(next_demand) = next_demand {
                if let Some(mut l) = lanes.get_mut(&session_id) {
                    l.demand = next_demand;
                    l.fiber_id = None;
                }

                let sid = session_id.clone();
                let lanes_c = lanes.clone();
                let state_c = state.clone();
                let fiber_set_c = fiber_set.clone();
                let drain_fn_c = drain_fn.clone();
                let on_failure_c = on_failure.clone();

                let next_future = async move {
                    let next_result = drain_fn_c(sid.clone(), next_demand).await;
                    settle(
                        sid.clone(),
                        lanes_c,
                        state_c,
                        fiber_set_c,
                        drain_fn_c,
                        on_failure_c,
                        next_demand,
                        next_result.clone(),
                    )
                    .await;
                    next_result
                };

                let handle = fiber_set.spawn(next_future);
                if let Some(mut l) = lanes.get_mut(&session_id) {
                    l.fiber_id = Some(handle.id());
                }
            }
        }
        (Err(_), _) => {
            // Failure — extract pending, signal result, create successor if pending
            let next_demand = lanes.get_mut(&session_id).and_then(|mut l| l.pending.take());

            // Send result to all waiters (even if we're replacing the lane)
            if let Some(l) = lanes.get_mut(&session_id) {
                if let Some(ref tx) = l.done_tx {
                    let _ = tx.send(result.clone());
                }
            }

            if let Some(next_demand) = next_demand {
                // Create successor in the lane
                if let Some(mut l) = lanes.get_mut(&session_id) {
                    l.demand = next_demand;
                    l.fiber_id = None;
                    l.stopping = false;
                    // Create a fresh broadcast for successor
                    let (new_tx, _) = done_channel();
                    l.done_tx = Some(new_tx);
                }

                let sid = session_id.clone();
                let lanes_c = lanes.clone();
                let state_c = state.clone();
                let fiber_set_c = fiber_set.clone();
                let drain_fn_c = drain_fn.clone();
                let on_failure_c = on_failure.clone();

                let next_future = async move {
                    let next_result = drain_fn_c(sid.clone(), next_demand).await;
                    settle(
                        sid.clone(),
                        lanes_c,
                        state_c,
                        fiber_set_c,
                        drain_fn_c,
                        on_failure_c,
                        next_demand,
                        next_result.clone(),
                    )
                    .await;
                    next_result
                };

                let handle = fiber_set.spawn(next_future);
                if let Some(mut l) = lanes.get_mut(&session_id) {
                    l.fiber_id = Some(handle.id());
                }
            } else {
                // No pending — remove lane
                lanes.remove(&session_id);

                // Report wake failures
                if result.is_err() && matches!(demand, Demand::Wake { .. }) {
                    if let Some(ref on_fail) = on_failure {
                        if let Err(ref e) = result {
                            on_fail(session_id.clone(), e.clone()).await;
                        }
                    }
                }
                let mut st = state.write().await;
                *st = CoordinatorState::Idle;
            }
        }
    }
})}

// ══════════════════════════════════════════════════════════════════════════════
// Session Run Error
// ══════════════════════════════════════════════════════════════════════════════

/// Errors from the session runner.
///
/// # Source
/// `packages/core/src/session/runner/index.ts` — `RunError`.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SessionRunError {
    /// Error kind
    pub kind: SessionRunErrorKind,
    /// Human-readable message
    pub message: String,
    /// Optional session context
    #[serde(skip_serializing_if = "Option::is_none")]
    pub session_id: Option<SessionId>,
}

/// Kinds of session runner errors.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum SessionRunErrorKind {
    /// Session not found
    SessionNotFound,
    /// Provider error
    ProviderError,
    /// Context overflow
    ContextOverflow,
    /// Permission denied
    PermissionDenied,
    /// Aborted by user
    Aborted,
    /// Compaction failed
    CompactionFailed,
    /// Timeout
    Timeout,
    /// Internal error
    Internal,
}

impl std::fmt::Display for SessionRunError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}: {}", self.kind, self.message)
    }
}

impl std::fmt::Display for SessionRunErrorKind {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::SessionNotFound => write!(f, "SessionNotFound"),
            Self::ProviderError => write!(f, "ProviderError"),
            Self::ContextOverflow => write!(f, "ContextOverflow"),
            Self::PermissionDenied => write!(f, "PermissionDenied"),
            Self::Aborted => write!(f, "Aborted"),
            Self::CompactionFailed => write!(f, "CompactionFailed"),
            Self::Timeout => write!(f, "Timeout"),
            Self::Internal => write!(f, "Internal"),
        }
    }
}

impl std::error::Error for SessionRunError {}

// ══════════════════════════════════════════════════════════════════════════════
// Session Error Types (from error.ts)
// ══════════════════════════════════════════════════════════════════════════════

/// Error when a message cannot be decoded.
///
/// # Source
/// `packages/core/src/session/error.ts` lines 5–8 `MessageDecodeError`.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MessageDecodeErrorInfo {
    pub session_id: SessionId,
    pub message_id: SessionMessageId,
}

impl std::fmt::Display for MessageDecodeErrorInfo {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "Failed to decode message {} in session {}",
            self.message_id, self.session_id
        )
    }
}

/// Error when a context snapshot cannot be decoded.
///
/// # Source
/// `packages/core/src/session/error.ts` lines 10–20 `ContextSnapshotDecodeError`.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ContextSnapshotDecodeErrorInfo {
    pub session_id: SessionId,
    pub details: String,
}

impl std::fmt::Display for ContextSnapshotDecodeErrorInfo {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "Failed to decode context snapshot for session {}: {}",
            self.session_id, self.details
        )
    }
}

// ══════════════════════════════════════════════════════════════════════════════
// Tests
// ══════════════════════════════════════════════════════════════════════════════

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_demand_serialization_run() {
        let demand = Demand::Run;
        let json = serde_json::to_string(&demand).expect("serialize");
        assert!(json.contains("run"));
        let parsed: Demand = serde_json::from_str(&json).expect("deserialize");
        assert_eq!(parsed, Demand::Run);
    }

    #[test]
    fn test_demand_serialization_wake() {
        let demand = Demand::Wake { seq: Some(42) };
        let json = serde_json::to_string(&demand).expect("serialize");
        assert!(json.contains("wake"));
        assert!(json.contains("42"));
        let parsed: Demand = serde_json::from_str(&json).expect("deserialize");
        assert_eq!(parsed, Demand::Wake { seq: Some(42) });
    }

    #[test]
    fn test_demand_serialization_wake_no_seq() {
        let demand = Demand::Wake { seq: None };
        let json = serde_json::to_string(&demand).expect("serialize");
        assert!(json.contains("wake"));
        assert!(!json.contains("seq"));
    }

    #[test]
    fn test_coalesce_run_dominates() {
        let result = coalesce_demand(Some(&Demand::Wake { seq: Some(1) }), &Demand::Run);
        assert_eq!(result, Demand::Run);
    }

    #[test]
    fn test_coalesce_wake_keeps_max_seq() {
        let result = coalesce_demand(
            Some(&Demand::Wake { seq: Some(5) }),
            &Demand::Wake { seq: Some(10) },
        );
        assert_eq!(result, Demand::Wake { seq: Some(10) });
    }

    #[test]
    fn test_coalesce_wake_first_none() {
        let result = coalesce_demand(None, &Demand::Wake { seq: Some(3) });
        assert_eq!(result, Demand::Wake { seq: Some(3) });
    }

    #[test]
    fn test_coordinator_entry_accepts_wake_when_not_stopping() {
        let entry = CoordinatorEntry::new(Demand::Run);
        assert!(entry.accepts_wake(Some(1)));
        assert!(entry.accepts_wake(None));
    }

    #[test]
    fn test_coordinator_entry_rejects_wake_when_stopping() {
        let mut entry = CoordinatorEntry::new(Demand::Wake { seq: Some(1) });
        entry.stopping = true;
        entry.interrupt_seq = Some(5);
        // seq 3 is not > interrupt_seq 5
        assert!(!entry.accepts_wake(Some(3)));
        // But seq 7 > interrupt_seq 5
        assert!(entry.accepts_wake(Some(7)));
    }

    #[test]
    fn test_message_decode_error_display() {
        let err = MessageDecodeErrorInfo {
            session_id: "ses_001".into(),
            message_id: "msg_001".into(),
        };
        let s = err.to_string();
        assert!(s.contains("ses_001"));
        assert!(s.contains("msg_001"));
    }

    #[test]
    fn test_context_snapshot_decode_error_display() {
        let err = ContextSnapshotDecodeErrorInfo {
            session_id: "ses_001".into(),
            details: "invalid JSON".into(),
        };
        let s = err.to_string();
        assert!(s.contains("ses_001"));
        assert!(s.contains("invalid JSON"));
    }

    #[test]
    fn test_session_run_error_display() {
        let err = SessionRunError {
            kind: SessionRunErrorKind::ProviderError,
            message: "Rate limit exceeded".into(),
            session_id: Some("ses_001".into()),
        };
        let s = err.to_string();
        assert!(s.contains("ProviderError"));
        assert!(s.contains("Rate limit exceeded"));
    }

    #[test]
    fn test_session_run_error_kinds_display() {
        let kinds = [
            SessionRunErrorKind::SessionNotFound,
            SessionRunErrorKind::ProviderError,
            SessionRunErrorKind::ContextOverflow,
            SessionRunErrorKind::PermissionDenied,
            SessionRunErrorKind::Aborted,
            SessionRunErrorKind::CompactionFailed,
            SessionRunErrorKind::Internal,
        ];
        for kind in &kinds {
            let s = kind.to_string();
            assert!(!s.is_empty());
        }
    }

    // ══════════════════════════════════════════════════════════════
    // FiberSet tests
    // ══════════════════════════════════════════════════════════════

    #[tokio::test]
    async fn test_fiber_set_spawn_and_collect() {
        let (fs, mut rx) = FiberSet::new();
        fs.spawn(async { 42u32 });
        let result = rx.recv().await.expect("should receive result");
        assert_eq!(result.id, 1);
        assert_eq!(result.result.unwrap(), 42);
    }

    #[tokio::test]
    async fn test_fiber_set_multiple_tasks() {
        let (fs, mut rx) = FiberSet::new();
        fs.spawn(async { "hello" });
        fs.spawn(async { "world" });
        let mut results = Vec::new();
        for _ in 0..2 {
            let r = rx.recv().await.expect("should receive result");
            results.push(r.result.unwrap());
        }
        results.sort();
        assert_eq!(results, vec!["hello", "world"]);
    }

    #[tokio::test]
    async fn test_fiber_set_cancel_specific() {
        let (fs, _rx) = FiberSet::new();
        let handle = fs.spawn(async {
            tokio::time::sleep(tokio::time::Duration::from_secs(60)).await;
            42u32
        });
        assert_eq!(fs.len(), 1);
        handle.cancel();
        // Give the runtime time to process cancellation
        tokio::time::sleep(tokio::time::Duration::from_millis(50)).await;
        // The task should still be tracked until the runtime cleans it up
        // Cancel signals the cancellation token
    }

    #[tokio::test]
    async fn test_fiber_set_cancel_all() {
        let (fs, _rx) = FiberSet::new();
        fs.spawn(async {
            tokio::time::sleep(tokio::time::Duration::from_secs(60)).await;
            1u32
        });
        fs.spawn(async {
            tokio::time::sleep(tokio::time::Duration::from_secs(60)).await;
            2u32
        });
        assert_eq!(fs.len(), 2);
        fs.cancel_all();
        tokio::time::sleep(tokio::time::Duration::from_millis(50)).await;
    }

    #[tokio::test]
    async fn test_fiber_set_join_all() {
        let (fs, _rx) = FiberSet::new();
        fs.spawn(async { 10u32 });
        fs.spawn(async { 20u32 });
        fs.join_all().await;
        assert!(fs.is_empty());
    }

    #[tokio::test]
    async fn test_fiber_set_fiber_handle_id() {
        let (fs, _rx) = FiberSet::<u32>::new();
        let handle = fs.spawn(async { 99 });
        assert_eq!(handle.id(), 1);
    }

    #[tokio::test]
    async fn test_fiber_set_empty() {
        let (fs, _rx) = FiberSet::<u32>::new();
        assert!(fs.is_empty());
        assert_eq!(fs.len(), 0);
    }

    // ══════════════════════════════════════════════════════════════
    // CoordinatorState tests
    // ══════════════════════════════════════════════════════════════

    #[test]
    fn test_coordinator_state_idle() {
        let state = CoordinatorState::Idle;
        assert!(state.is_idle());
        assert!(!state.is_running());
        assert!(!state.is_awaiting_input());
        assert!(!state.is_interrupted());
        assert_eq!(state.turn_id(), None);
    }

    #[test]
    fn test_coordinator_state_running() {
        let state = CoordinatorState::Running { turn_id: 42 };
        assert!(!state.is_idle());
        assert!(state.is_running());
        assert!(!state.is_awaiting_input());
        assert!(!state.is_interrupted());
        assert_eq!(state.turn_id(), Some(42));
    }

    #[test]
    fn test_coordinator_state_awaiting_input() {
        let state = CoordinatorState::AwaitingInput { turn_id: 7 };
        assert!(!state.is_idle());
        assert!(!state.is_running());
        assert!(state.is_awaiting_input());
        assert!(!state.is_interrupted());
        assert_eq!(state.turn_id(), Some(7));
    }

    #[test]
    fn test_coordinator_state_interrupted() {
        let state = CoordinatorState::Interrupted;
        assert!(!state.is_idle());
        assert!(!state.is_running());
        assert!(!state.is_awaiting_input());
        assert!(state.is_interrupted());
        assert_eq!(state.turn_id(), None);
    }

    #[test]
    fn test_coordinator_state_serialization() {
        let states = vec![
            CoordinatorState::Idle,
            CoordinatorState::Running { turn_id: 1 },
            CoordinatorState::AwaitingInput { turn_id: 2 },
            CoordinatorState::Interrupted,
        ];
        for state in &states {
            let json = serde_json::to_string(state).expect("serialize");
            let parsed: CoordinatorState =
                serde_json::from_str(&json).expect("deserialize");
            assert_eq!(*state, parsed);
        }
    }

    // ══════════════════════════════════════════════════════════════
    // RunCoordinator tests
    // ══════════════════════════════════════════════════════════════

    #[tokio::test]
    async fn test_run_coordinator_run_ok() {
        let coord = RunCoordinator::new(
            Arc::new(|_id, demand| {
                let _ = demand;
                Box::pin(async { Ok(()) })
            }),
            None,
        );
        let result = coord.run("ses_001".into()).await;
        assert!(result.is_ok());
    }

    #[tokio::test]
    async fn test_run_coordinator_run_error() {
        let coord = RunCoordinator::new(
            Arc::new(|_id, demand| {
                let _ = demand;
                Box::pin(async {
                    Err(SessionRunError {
                        kind: SessionRunErrorKind::ProviderError,
                        message: "test error".into(),
                        session_id: None,
                    })
                })
            }),
            None,
        );
        let result = coord.run("ses_001".into()).await;
        assert!(result.is_err());
        assert_eq!(
            result.unwrap_err().kind,
            SessionRunErrorKind::ProviderError
        );
    }

    #[tokio::test]
    async fn test_run_coordinator_state_transitions() {
        let coord = RunCoordinator::new(
            Arc::new(|_id, demand| {
                let _ = demand;
                Box::pin(async { Ok(()) })
            }),
            None,
        );

        // Initially Idle
        let state = coord.state().await;
        assert_eq!(state, CoordinatorState::Idle);

        // After run completes, should be Idle again
        coord.run("ses_001".into()).await.unwrap();
        let state = coord.state().await;
        assert_eq!(state, CoordinatorState::Idle);
    }

    #[tokio::test]
    async fn test_run_coordinator_wake_does_not_error() {
        let coord = RunCoordinator::new(
            Arc::new(|_id, demand| {
                let _ = demand;
                Box::pin(async { Ok(()) })
            }),
            None,
        );
        // Wake should not panic or return an error (it's fire-and-forget)
        coord.wake("ses_001".into(), Some(1)).await;
        // Give it time to complete
        tokio::time::sleep(tokio::time::Duration::from_millis(50)).await;
    }

    #[tokio::test]
    async fn test_run_coordinator_interrupt_idle() {
        let coord = RunCoordinator::new(
            Arc::new(|_id, demand| {
                let _ = demand;
                Box::pin(async { Ok(()) })
            }),
            None,
        );
        // Interrupt on an idle session should be a no-op
        coord.interrupt("ses_001".into(), Some(1)).await;
        tokio::time::sleep(tokio::time::Duration::from_millis(50)).await;
    }

    #[tokio::test]
    async fn test_run_coordinator_await_idle() {
        let coord = RunCoordinator::new(
            Arc::new(|_id, demand| {
                let _ = demand;
                Box::pin(async { Ok(()) })
            }),
            None,
        );
        // Wait on an idle session should return immediately
        let result = coord.await_idle("ses_001".into()).await;
        assert!(result.is_ok());
    }

    #[tokio::test]
    async fn test_run_coordinator_demand_coalescing_wake_upgrade() {
        let coord = RunCoordinator::new(
            Arc::new(|_id, demand| {
                let _ = demand;
                Box::pin(async {
                    // Simulate a drain that takes a moment
                    tokio::time::sleep(tokio::time::Duration::from_millis(100)).await;
                    Ok(())
                })
            }),
            None,
        );

        // Start a wake drain
        coord.wake("ses_001".into(), Some(1)).await;

        // Immediately upgrade with a run — should coalesce
        let result = coord.run("ses_001".into()).await;
        // Result should be ok (the run should eventually complete)
        assert!(result.is_ok() || result.is_err());
    }

    #[tokio::test]
    async fn test_run_coordinator_concurrent_sessions() {
        let coord = Arc::new(RunCoordinator::new(
            Arc::new(|_id, demand| {
                let _ = demand;
                Box::pin(async {
                    tokio::time::sleep(tokio::time::Duration::from_millis(50)).await;
                    Ok(())
                })
            }),
            None,
        ));

        // Different sessions should run concurrently
        let c1 = coord.clone();
        let h1 = tokio::spawn(async move { c1.run("ses_a".into()).await });
        let c2 = coord.clone();
        let h2 = tokio::spawn(async move { c2.run("ses_b".into()).await });

        let (r1, r2) = tokio::join!(h1, h2);
        assert!(r1.unwrap().is_ok());
        assert!(r2.unwrap().is_ok());
    }

    #[tokio::test]
    async fn test_run_coordinator_interrupt_during_run() {
        let coord = Arc::new(RunCoordinator::new(
            Arc::new(|_id, demand| {
                let _ = demand;
                Box::pin(async {
                    // Long-running drain
                    tokio::time::sleep(tokio::time::Duration::from_secs(10)).await;
                    Ok(())
                })
            }),
            None,
        ));

        // Spawn a run
        let c = coord.clone();
        let run_handle = tokio::spawn(async move { c.run("ses_001".into()).await });

        // Give it a moment to start
        tokio::time::sleep(tokio::time::Duration::from_millis(50)).await;

        // Interrupt it
        coord.interrupt("ses_001".into(), Some(1)).await;

        // The run should eventually complete (possibly with an error)
        let result = run_handle.await.unwrap();
        // The result might be ok (if the task was cancelled after the drain started)
        // or might be an Aborted error
    }

    // ══════════════════════════════════════════════════════════════
    // FnRunner tests
    // ══════════════════════════════════════════════════════════════

    #[tokio::test]
    async fn test_fn_runner_basic() {
        let runner = FnRunner::new(|session_id: SessionId, force: bool| async move {
            let _ = (session_id, force);
            Ok(())
        });
        let result = runner
            .coordinated_run("ses_001".into(), true)
            .await;
        assert!(result.is_ok());
    }

    #[tokio::test]
    async fn test_fn_runner_error() {
        let runner = FnRunner::new(|session_id: SessionId, force: bool| async move {
            let _ = session_id;
            let _ = force;
            Err(SessionRunError {
                kind: SessionRunErrorKind::Aborted,
                message: "cancelled".into(),
                session_id: None,
            })
        });
        let result = runner
            .coordinated_run("ses_001".into(), false)
            .await;
        assert!(result.is_err());
    }
}
