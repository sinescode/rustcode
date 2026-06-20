//! Background job types — status tracking for long-running operations.
//!
//! Ported from: `packages/core/src/background-job.ts`
//! OpenCode commit: 5d0f86606ac30690f79f0a6a9f41a1f49fe95d0b
//!
//! This module provides the data types and runtime service for background job
//! lifecycle management.
//!
//! The TS source uses Effect.ts for the full job registry (scoped, process-local,
//! with `Deferred`-based synchronization). The Rust port uses `tokio::sync::watch`
//! channels for completion signaling and `tokio::spawn` for async task execution.

use std::collections::HashMap;
use std::future::Future;
use std::sync::Arc;

use serde::{Deserialize, Serialize};
use tokio::sync::{watch, Notify, RwLock};

/// Callback type for the `onPromote` effect.
///
/// In the TS source this is an `Effect<void>` that runs when the job is promoted
/// from background to foreground. The Rust port uses a boxed closure.
pub type OnPromoteFn = Arc<dyn Fn() + Send + Sync>;

// ── JobStatus ─────────────────────────────────────────────────────────────

/// Background job status.
///
/// Maps to the TS string union: `"running" | "completed" | "error" | "cancelled"`.
///
/// # Source
/// `packages/core/src/background-job.ts` — `type Status`
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum JobStatus {
    /// Job is currently executing.
    Running,
    /// Job finished successfully.
    Completed,
    /// Job finished with an error.
    Error,
    /// Job was explicitly cancelled.
    Cancelled,
}

impl JobStatus {
    /// Returns `true` if this status represents a terminal (non-running) state.
    pub fn is_terminal(&self) -> bool {
        matches!(
            self,
            JobStatus::Completed | JobStatus::Error | JobStatus::Cancelled
        )
    }
}

// ── JobInfo ───────────────────────────────────────────────────────────────

/// Information about a background job — its identity, status, timing, and output.
///
/// # Source
/// `packages/core/src/background-job.ts` — `type Info`
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct JobInfo {
    /// Unique job identifier (ascending sortable).
    pub id: String,

    /// Job type discriminator (e.g. `"tool_call"`, `"file_scan"`).
    ///
    /// Renamed from `type` (reserved keyword in Rust).
    #[serde(rename = "type")]
    pub type_: String,

    /// Optional human-readable title.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub title: Option<String>,

    /// Current job status.
    pub status: JobStatus,

    /// Unix timestamp in milliseconds when the job was started.
    pub started_at: i64,

    /// Unix timestamp in milliseconds when the job reached a terminal state.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub completed_at: Option<i64>,

    /// Final output text (set when job produces output).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub output: Option<String>,

    /// Error message (set when job fails).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,

    /// Arbitrary metadata attached at job creation.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub metadata: Option<serde_json::Value>,
}

impl JobInfo {
    /// Create a new [`JobInfo`] in the `Running` state, with `started_at` set
    /// to the current Unix timestamp in milliseconds.
    ///
    /// # Source
    /// `packages/core/src/background-job.ts` — `start()` fn, lines 201–252
    pub fn new(id: String, type_: String) -> Self {
        let started_at = chrono::Utc::now().timestamp_millis();
        Self {
            id,
            type_,
            title: None,
            status: JobStatus::Running,
            started_at,
            completed_at: None,
            output: None,
            error: None,
            metadata: None,
        }
    }

    /// Transition the job to `Completed` status, recording the current time
    /// as `completed_at`.
    ///
    /// # Source
    /// `packages/core/src/background-job.ts` — `settle()` fn (success branch), line 145
    pub fn complete(&mut self) {
        self.status = JobStatus::Completed;
        self.completed_at = Some(chrono::Utc::now().timestamp_millis());
    }

    /// Transition the job to `Error` status, recording the current time
    /// as `completed_at` and storing the error message.
    ///
    /// # Source
    /// `packages/core/src/background-job.ts` — `settle()` fn (error branch), line 149
    pub fn fail(&mut self, error: String) {
        self.status = JobStatus::Error;
        self.completed_at = Some(chrono::Utc::now().timestamp_millis());
        self.error = Some(error);
    }

    /// Transition the job to `Cancelled` status, recording the current time
    /// as `completed_at`.
    ///
    /// # Source
    /// `packages/core/src/background-job.ts` — `cancel()` fn, lines 336–357
    pub fn cancel(&mut self) {
        self.status = JobStatus::Cancelled;
        self.completed_at = Some(chrono::Utc::now().timestamp_millis());
    }

    /// Set the output text for this job.
    ///
    /// # Source
    /// `packages/core/src/background-job.ts` — `settle()` fn, `output` field update, line 139
    pub fn set_output(&mut self, output: String) {
        self.output = Some(output);
    }

    /// Returns `true` if the job is in a terminal state (not `Running`).
    ///
    /// # Source
    /// `packages/core/src/background-job.ts` — status checks in `wait()`, `extend()`, `cancel()`
    pub fn is_terminal(&self) -> bool {
        self.status.is_terminal()
    }
}

// ── JobStartInput ─────────────────────────────────────────────────────────

/// Input for starting a new background job.
///
/// # Source
/// `packages/core/src/background-job.ts` — `type StartInput`
#[derive(Clone)]
pub struct JobStartInput {
    /// Optional job identifier. If omitted, one is generated.
    pub id: Option<String>,

    /// Job type discriminator.
    ///
    /// Renamed from `type` (reserved keyword in Rust).
    pub type_: String,

    /// Optional human-readable title.
    pub title: Option<String>,

    /// Arbitrary metadata attached at creation.
    pub metadata: Option<serde_json::Value>,

    /// Callback that runs when the job is promoted from background to foreground.
    pub on_promote: Option<OnPromoteFn>,
}

impl std::fmt::Debug for JobStartInput {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("JobStartInput")
            .field("id", &self.id)
            .field("type_", &self.type_)
            .field("title", &self.title)
            .field("metadata", &self.metadata)
            .field(
                "on_promote",
                &self.on_promote.as_ref().map(|_| "OnPromoteFn"),
            )
            .finish()
    }
}

impl JobStartInput {
    /// Create a new [`JobStartInput`] with the given type and no optional fields.
    ///
    /// # Source
    /// `packages/core/src/background-job.ts` — `StartInput` type, line 63
    pub fn new(type_: String) -> Self {
        Self {
            id: None,
            type_,
            title: None,
            metadata: None,
            on_promote: None,
        }
    }
}

// ── JobExtendInput ────────────────────────────────────────────────────────

/// Input for extending a running job with additional work.
///
/// # Source
/// `packages/core/src/background-job.ts` — `type ExtendInput`
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct JobExtendInput {
    /// The identifier of the job to extend.
    pub id: String,
}

// ── JobWaitInput ──────────────────────────────────────────────────────────

/// Input for waiting on a job to complete.
///
/// # Source
/// `packages/core/src/background-job.ts` — `type WaitInput`
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct JobWaitInput {
    /// The identifier of the job to wait for.
    pub id: String,

    /// Optional timeout in milliseconds. If the job does not complete within
    /// this duration the result will have `timed_out = true`.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub timeout: Option<u64>,
}

// ── JobWaitResult ─────────────────────────────────────────────────────────

/// Result of waiting on a job.
///
/// # Source
/// `packages/core/src/background-job.ts` — `type WaitResult`
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct JobWaitResult {
    /// The job info at the time the wait resolved. `None` if the job was never
    /// registered.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub info: Option<JobInfo>,

    /// `true` if the wait timed out before the job completed.
    pub timed_out: bool,
}

// ── ActiveJob ─────────────────────────────────────────────────────────────

/// Internal bookkeeping for a single running job.
///
/// Holds the job's info snapshot plus the `watch` senders that signal completion
/// and cancellation to waiters.
struct ActiveJob {
    info: JobInfo,
    done_tx: watch::Sender<Option<JobInfo>>,
    cancel: watch::Sender<bool>,
    promote_tx: watch::Sender<bool>,
    on_promote: Option<OnPromoteFn>,
}

// ── BackgroundJobService ──────────────────────────────────────────────────

/// Registry that manages the lifecycle of background jobs.
///
/// Ported from: `packages/core/src/background-job.ts` — `BackgroundJob` service
///
/// Jobs are spawned as independent tokio tasks. Completion and cancellation are
/// communicated through `tokio::sync::watch` channels so that multiple waiters
/// can observe the same terminal event.
#[derive(Clone)]
pub struct BackgroundJobService {
    jobs: Arc<RwLock<HashMap<String, ActiveJob>>>,
}

impl Default for BackgroundJobService {
    fn default() -> Self {
        Self::new()
    }
}

impl BackgroundJobService {
    /// Create a new, empty job service.
    pub fn new() -> Self {
        Self {
            jobs: Arc::new(RwLock::new(HashMap::new())),
        }
    }

    /// List all registered jobs, sorted by `started_at` (oldest first).
    pub async fn list(&self) -> Vec<JobInfo> {
        let jobs = self.jobs.read().await;
        let mut infos: Vec<JobInfo> = jobs.values().map(|j| j.info.clone()).collect();
        infos.sort_by_key(|i| i.started_at);
        infos
    }

    /// Get the current info snapshot for a job by id.
    pub async fn get(&self, id: &str) -> Option<JobInfo> {
        let jobs = self.jobs.read().await;
        jobs.get(id).map(|j| j.info.clone())
    }

    /// Start a new background job.
    ///
    /// `run_fn` is called inside a `tokio::spawn` task. It receives no arguments;
    /// the returned future resolves to `Ok(output)` or `Err(message)`.
    pub async fn start<F, Fut>(&self, input: JobStartInput, run_fn: F) -> JobInfo
    where
        F: FnOnce() -> Fut + Send + 'static,
        Fut: Future<Output = Result<String, String>> + Send + 'static,
    {
        let id = input.id.unwrap_or_else(|| {
            crate::id::ascending(crate::id::IdPrefix::Job, None)
                .expect("ID generation should not fail")
        });

        let mut info = JobInfo::new(id.clone(), input.type_);
        info.title = input.title;
        info.metadata = input.metadata;

        let (done_tx, _done_rx): (watch::Sender<Option<JobInfo>>, _) = watch::channel(None);
        let (cancel_tx, cancel_rx) = watch::channel(false);
        let (promote_tx, _promote_rx) = watch::channel(false);

        let active = ActiveJob {
            info: info.clone(),
            done_tx: done_tx.clone(),
            cancel: cancel_tx.clone(),
            promote_tx: promote_tx.clone(),
            on_promote: input.on_promote,
        };
        self.jobs.write().await.insert(id.clone(), active);

        let jobs_ref = self.jobs.clone();
        let done_tx2 = done_tx;
        let id_clone = id.clone();

        tokio::spawn(async move {
            let result = tokio::select! {
                res = run_fn() => res,
                _ = cancelled(&cancel_rx) => {
                    // Mark as cancelled
                    let mut jobs = jobs_ref.write().await;
                    if let Some(active) = jobs.get_mut(&id_clone) {
                        active.info.cancel();
                        let snapshot = active.info.clone();
                        let _ = active.done_tx.send(Some(snapshot));
                    }
                    return;
                }
            };

            let mut jobs = jobs_ref.write().await;
            if let Some(active) = jobs.get_mut(&id_clone) {
                match result {
                    Ok(output) => {
                        active.info.set_output(output);
                        active.info.complete();
                    }
                    Err(err) => {
                        active.info.fail(err);
                    }
                }
                let snapshot = active.info.clone();
                let _ = done_tx2.send(Some(snapshot));
            }
        });

        info
    }

    /// Extend a running job by chaining additional work after the current task.
    ///
    /// The new `run_fn` is spawned immediately; when it completes the job is
    /// settled with the new result. If the job is already terminal this is a
    /// no-op and returns the current info.
    pub async fn extend<F, Fut>(&self, input: JobExtendInput, run_fn: F) -> JobInfo
    where
        F: FnOnce() -> Fut + Send + 'static,
        Fut: Future<Output = Result<String, String>> + Send + 'static,
    {
        let jobs = self.jobs.read().await;
        let active = match jobs.get(&input.id) {
            Some(a) => a,
            None => return JobInfo::new(input.id.clone(), "unknown".into()),
        };
        if active.info.is_terminal() {
            return active.info.clone();
        }
        let cancel_rx = active.cancel.subscribe();
        let done_tx = active.done_tx.clone();
        drop(jobs);

        let jobs_ref = self.jobs.clone();
        let id = input.id.clone();

        tokio::spawn(async move {
            let result = tokio::select! {
                res = run_fn() => res,
                _ = cancelled(&cancel_rx) => {
                    let mut jobs = jobs_ref.write().await;
                    if let Some(active) = jobs.get_mut(&id) {
                        active.info.cancel();
                        let snapshot = active.info.clone();
                        let _ = active.done_tx.send(Some(snapshot));
                    }
                    return;
                }
            };

            let mut jobs = jobs_ref.write().await;
            if let Some(active) = jobs.get_mut(&id) {
                match result {
                    Ok(output) => {
                        active.info.set_output(output);
                        active.info.complete();
                    }
                    Err(err) => {
                        active.info.fail(err);
                    }
                }
                let snapshot = active.info.clone();
                let _ = done_tx.send(Some(snapshot));
            }
        });

        self.get(&input.id)
            .await
            .unwrap_or_else(|| JobInfo::new(input.id.clone(), "unknown".into()))
    }

    /// Wait for a job to reach a terminal state.
    ///
    /// Returns a [`JobWaitResult`] with the final info snapshot. If `timeout`
    /// is set and the job doesn't complete in time, `timed_out` is `true`.
    pub async fn wait(&self, input: JobWaitInput) -> JobWaitResult {
        let rx = {
            let jobs = self.jobs.read().await;
            match jobs.get(&input.id) {
                Some(a) => {
                    if a.info.is_terminal() {
                        return JobWaitResult {
                            info: Some(a.info.clone()),
                            timed_out: false,
                        };
                    }
                    a.done_tx.subscribe()
                }
                None => {
                    return JobWaitResult {
                        info: None,
                        timed_out: false,
                    };
                }
            }
        };

        let wait_fut = async {
            let mut rx = rx;
            // Skip the initial None value if still running.
            while rx.changed().await.is_ok() {
                let snap = rx.borrow().clone();
                if let Some(info) = snap {
                    return JobWaitResult {
                        info: Some(info),
                        timed_out: false,
                    };
                }
            }
            // Channel closed — job was removed or panicked.
            JobWaitResult {
                info: None,
                timed_out: false,
            }
        };

        let result = match input.timeout {
            Some(ms) => {
                let timeout = std::time::Duration::from_millis(ms);
                match tokio::time::timeout(timeout, wait_fut).await {
                    Ok(r) => r,
                    Err(_) => {
                        let jobs = self.jobs.read().await;
                        let info = jobs.get(&input.id).map(|a| a.info.clone());
                        JobWaitResult {
                            info,
                            timed_out: true,
                        }
                    }
                }
            }
            None => wait_fut.await,
        };

        result
    }

    /// Cancel a running job.
    ///
    /// Sends the cancellation signal through the watch channel. The spawned
    /// task will observe the signal on its next `tokio::select!` poll and
    /// settle the job as `Cancelled`.
    pub async fn cancel(&self, id: &str) -> Option<JobInfo> {
        let jobs = self.jobs.read().await;
        if let Some(active) = jobs.get(id) {
            if active.info.is_terminal() {
                return Some(active.info.clone());
            }
            let _ = active.cancel.send(true);
        }
        drop(jobs);

        // Give the task a moment to process the cancellation signal.
        // We poll briefly to let the spawned task settle.
        tokio::task::yield_now().await;

        let jobs = self.jobs.read().await;
        jobs.get(id).map(|a| a.info.clone())
    }

    /// Wait for a job to be promoted from background to foreground.
    ///
    /// If the job already has `metadata.background == true`, returns the current
    /// info snapshot immediately. Otherwise, waits on the promotion channel.
    /// Returns `None` if the job does not exist.
    pub async fn wait_for_promotion(&self, id: &str) -> Option<JobInfo> {
        let jobs = self.jobs.read().await;
        let active = match jobs.get(id) {
            Some(a) => a,
            None => return None,
        };

        // Check if already a background job
        if let Some(meta) = &active.info.metadata {
            if meta.get("background").and_then(|v| v.as_bool()) == Some(true) {
                return Some(active.info.clone());
            }
        }

        // Subscribe to promotion channel
        let mut rx = active.promote_tx.subscribe();
        drop(jobs);

        // Wait for promotion signal
        while rx.changed().await.is_ok() {
            if *rx.borrow() {
                let jobs = self.jobs.read().await;
                return jobs.get(id).map(|a| a.info.clone());
            }
        }

        // Channel closed — job was removed
        None
    }

    /// Promotes a background job to foreground.
    ///
    /// Sets `metadata.background = true` and resolves the promotion channel,
    /// waking any waiters. Returns the updated info, or `None` if the job
    /// does not exist.
    pub async fn promote(&self, id: &str) -> Option<JobInfo> {
        let jobs = self.jobs.read().await;
        let active = match jobs.get(id) {
            Some(a) => a,
            None => return None,
        };

        // Already promoted
        if let Some(meta) = &active.info.metadata {
            if meta.get("background").and_then(|v| v.as_bool()) == Some(true) {
                return Some(active.info.clone());
            }
        }

        let on_promote = active.on_promote.clone();
        let promote_tx = active.promote_tx.clone();
        drop(jobs);

        // Set metadata.background = true
        let mut jobs = self.jobs.write().await;
        if let Some(active) = jobs.get_mut(id) {
            let mut meta = active.info.metadata.take().unwrap_or(serde_json::json!({}));
            meta.as_object_mut()
                .expect("metadata must be an object")
                .insert("background".to_string(), serde_json::json!(true));
            active.info.metadata = Some(meta);

            // Signal promotion
            let _ = promote_tx.send(true);

            // Run the on_promote callback if set
            if let Some(cb) = on_promote {
                (cb)();
            }

            Some(active.info.clone())
        } else {
            None
        }
    }
}

/// Helper future that resolves when `cancel_rx` receives `true`.
async fn cancelled(rx: &watch::Receiver<bool>) {
    let mut rx = rx.clone();
    while rx.changed().await.is_ok() {
        if *rx.borrow() {
            return;
        }
    }
}

// ── Tests ─────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    // ── JobStatus ─────────────────────────────────────────────────────

    #[test]
    fn test_job_status_serialization_running() {
        let status = JobStatus::Running;
        let json = serde_json::to_string(&status).expect("serialize Running");
        assert_eq!(json, r#""running""#);
        let roundtrip: JobStatus = serde_json::from_str(&json).expect("deserialize Running");
        assert_eq!(roundtrip, JobStatus::Running);
    }

    #[test]
    fn test_job_status_serialization_completed() {
        let status = JobStatus::Completed;
        let json = serde_json::to_string(&status).expect("serialize Completed");
        assert_eq!(json, r#""completed""#);
        let roundtrip: JobStatus = serde_json::from_str(&json).expect("deserialize Completed");
        assert_eq!(roundtrip, JobStatus::Completed);
    }

    #[test]
    fn test_job_status_serialization_error() {
        let status = JobStatus::Error;
        let json = serde_json::to_string(&status).expect("serialize Error");
        assert_eq!(json, r#""error""#);
        let roundtrip: JobStatus = serde_json::from_str(&json).expect("deserialize Error");
        assert_eq!(roundtrip, JobStatus::Error);
    }

    #[test]
    fn test_job_status_serialization_cancelled() {
        let status = JobStatus::Cancelled;
        let json = serde_json::to_string(&status).expect("serialize Cancelled");
        assert_eq!(json, r#""cancelled""#);
        let roundtrip: JobStatus = serde_json::from_str(&json).expect("deserialize Cancelled");
        assert_eq!(roundtrip, JobStatus::Cancelled);
    }

    #[test]
    fn test_job_status_is_terminal() {
        assert!(!JobStatus::Running.is_terminal());
        assert!(JobStatus::Completed.is_terminal());
        assert!(JobStatus::Error.is_terminal());
        assert!(JobStatus::Cancelled.is_terminal());
    }

    // ── JobInfo lifecycle ─────────────────────────────────────────────

    #[test]
    fn test_job_info_new_in_running_state() {
        let info = JobInfo::new("job_001".into(), "tool_call".into());
        assert_eq!(info.id, "job_001");
        assert_eq!(info.type_, "tool_call");
        assert_eq!(info.status, JobStatus::Running);
        assert!(info.started_at > 0);
        assert_eq!(info.completed_at, None);
        assert_eq!(info.output, None);
        assert_eq!(info.error, None);
        assert_eq!(info.metadata, None);
        assert!(!info.is_terminal());
    }

    #[test]
    fn test_job_info_complete_transition() {
        let mut info = JobInfo::new("job_002".into(), "build".into());
        info.complete();
        assert_eq!(info.status, JobStatus::Completed);
        assert!(info.completed_at.is_some());
        assert!(info.completed_at.expect("completed_at set") >= info.started_at);
        assert!(info.is_terminal());
    }

    #[test]
    fn test_job_info_fail_transition() {
        let mut info = JobInfo::new("job_003".into(), "deploy".into());
        let err = "connection refused".to_string();
        info.fail(err.clone());
        assert_eq!(info.status, JobStatus::Error);
        assert!(info.completed_at.is_some());
        assert_eq!(info.error.as_deref(), Some("connection refused"));
        assert!(info.is_terminal());
    }

    #[test]
    fn test_job_info_cancel_transition() {
        let mut info = JobInfo::new("job_004".into(), "scan".into());
        info.cancel();
        assert_eq!(info.status, JobStatus::Cancelled);
        assert!(info.completed_at.is_some());
        assert!(info.is_terminal());
    }

    #[test]
    fn test_job_info_set_output() {
        let mut info = JobInfo::new("job_005".into(), "lint".into());
        info.set_output("no issues found".into());
        assert_eq!(info.output.as_deref(), Some("no issues found"));
        // Output alone does NOT make a job terminal
        assert!(!info.is_terminal());
    }

    #[test]
    fn test_job_info_full_lifecycle() {
        let mut info = JobInfo::new("job_006".into(), "test".into());

        // Running
        assert_eq!(info.status, JobStatus::Running);
        assert!(!info.is_terminal());

        // Set output while running
        info.set_output("test results: 5 passed".into());
        assert_eq!(info.output.as_deref(), Some("test results: 5 passed"));

        // Complete
        info.complete();
        assert_eq!(info.status, JobStatus::Completed);
        assert!(info.is_terminal());
        assert!(info.completed_at.is_some());
        assert!(info.error.is_none());
    }

    // ── JobInfo serialization ─────────────────────────────────────────

    #[test]
    fn test_job_info_serialization_roundtrip() {
        let mut info = JobInfo::new("job_007".into(), "format".into());
        info.complete();
        info.set_output("formatted".into());

        let json = serde_json::to_string(&info).expect("serialize JobInfo");

        // Verify key fields use camelCase
        let parsed: serde_json::Value = serde_json::from_str(&json).expect("parse as JSON value");
        assert_eq!(parsed["id"], "job_007");
        // "type" not "type_" in JSON
        assert_eq!(parsed["type"], "format");
        assert_eq!(parsed["status"], "completed");
        assert!(parsed["startedAt"].is_number());
        assert!(parsed["completedAt"].is_number());
        assert_eq!(parsed["output"], "formatted");
        // Roundtrip back to struct
        let roundtrip: JobInfo = serde_json::from_str(&json).expect("deserialize JobInfo");
        assert_eq!(roundtrip, info);
    }

    #[test]
    fn test_job_info_serialization_minimal() {
        let info = JobInfo::new("min_001".into(), "noop".into());
        let json = serde_json::to_string(&info).expect("serialize minimal JobInfo");
        let parsed: serde_json::Value = serde_json::from_str(&json).expect("parse as JSON value");

        // Required fields present
        assert_eq!(parsed["id"], "min_001");
        assert_eq!(parsed["type"], "noop");
        assert_eq!(parsed["status"], "running");
        assert!(parsed["startedAt"].is_number());
        // Optional fields skipped when None
        assert!(parsed.get("title").is_none());
        assert!(parsed.get("completedAt").is_none());
        assert!(parsed.get("output").is_none());
        assert!(parsed.get("error").is_none());
        assert!(parsed.get("metadata").is_none());
    }

    // ── JobStartInput ─────────────────────────────────────────────────

    #[test]
    fn test_job_start_input_new() {
        let input = JobStartInput::new("scan".into());
        assert_eq!(input.type_, "scan");
        assert_eq!(input.id, None);
        assert_eq!(input.title, None);
        assert_eq!(input.metadata, None);
        assert!(input.on_promote.is_none());
    }

    // ── JobExtendInput ────────────────────────────────────────────────

    #[test]
    fn test_job_extend_input_serialization() {
        let input = JobExtendInput {
            id: "job_042".into(),
        };
        let json = serde_json::to_string(&input).expect("serialize JobExtendInput");
        assert_eq!(json, r#"{"id":"job_042"}"#);
        let roundtrip: JobExtendInput =
            serde_json::from_str(&json).expect("deserialize JobExtendInput");
        assert_eq!(roundtrip.id, "job_042");
    }

    // ── JobWaitInput ──────────────────────────────────────────────────

    #[test]
    fn test_job_wait_input_serialization_with_timeout() {
        let input = JobWaitInput {
            id: "job_050".into(),
            timeout: Some(30_000),
        };
        let json = serde_json::to_string(&input).expect("serialize JobWaitInput");
        let parsed: serde_json::Value = serde_json::from_str(&json).expect("parse as JSON value");
        assert_eq!(parsed["id"], "job_050");
        assert_eq!(parsed["timeout"], 30_000);
    }

    #[test]
    fn test_job_wait_input_serialization_without_timeout() {
        let input = JobWaitInput {
            id: "job_051".into(),
            timeout: None,
        };
        let json = serde_json::to_string(&input).expect("serialize JobWaitInput");
        let parsed: serde_json::Value = serde_json::from_str(&json).expect("parse as JSON value");
        assert_eq!(parsed["id"], "job_051");
        assert!(parsed.get("timeout").is_none());
    }

    // ── JobWaitResult ─────────────────────────────────────────────────

    #[test]
    fn test_job_wait_result_completed() {
        let mut info = JobInfo::new("job_060".into(), "build".into());
        info.complete();
        let result = JobWaitResult {
            info: Some(info),
            timed_out: false,
        };
        let json = serde_json::to_string(&result).expect("serialize JobWaitResult");
        let parsed: serde_json::Value = serde_json::from_str(&json).expect("parse as JSON value");
        assert_eq!(parsed["timedOut"], false);
        assert!(parsed["info"].is_object());
        assert_eq!(parsed["info"]["status"], "completed");
    }

    #[test]
    fn test_job_wait_result_timeout() {
        let info = JobInfo::new("job_061".into(), "deploy".into());
        let result = JobWaitResult {
            info: Some(info),
            timed_out: true,
        };
        let json = serde_json::to_string(&result).expect("serialize JobWaitResult");
        let parsed: serde_json::Value = serde_json::from_str(&json).expect("parse as JSON value");
        assert_eq!(parsed["timedOut"], true);
        // timed_out: true means the job was still running at timeout
        assert_eq!(parsed["info"]["status"], "running");
    }

    #[test]
    fn test_job_wait_result_no_info() {
        let result = JobWaitResult {
            info: None,
            timed_out: false,
        };
        let json = serde_json::to_string(&result).expect("serialize JobWaitResult");
        let parsed: serde_json::Value = serde_json::from_str(&json).expect("parse as JSON value");
        assert_eq!(parsed["timedOut"], false);
        assert!(parsed.get("info").is_none());
    }

    // ── BackgroundJobService ───────────────────────────────────────────

    #[tokio::test]
    async fn test_service_new_is_empty() {
        let svc = BackgroundJobService::new();
        assert!(svc.list().await.is_empty());
    }

    #[tokio::test]
    async fn test_service_start_and_complete() {
        let svc = BackgroundJobService::new();
        let input = JobStartInput::new("test_run".into());
        let info = svc.start(input, || async { Ok("done".into()) }).await;
        assert_eq!(info.status, JobStatus::Running);

        let waited = svc
            .wait(JobWaitInput {
                id: info.id.clone(),
                timeout: None,
            })
            .await;
        assert!(!waited.timed_out);
        let final_info = waited.info.expect("job should exist");
        assert_eq!(final_info.status, JobStatus::Completed);
        assert_eq!(final_info.output.as_deref(), Some("done"));
    }

    #[tokio::test]
    async fn test_service_start_and_fail() {
        let svc = BackgroundJobService::new();
        let input = JobStartInput::new("fail_run".into());
        let info = svc.start(input, || async { Err("boom".into()) }).await;

        let waited = svc
            .wait(JobWaitInput {
                id: info.id.clone(),
                timeout: None,
            })
            .await;
        let final_info = waited.info.expect("job should exist");
        assert_eq!(final_info.status, JobStatus::Error);
        assert_eq!(final_info.error.as_deref(), Some("boom"));
    }

    #[tokio::test]
    async fn test_service_cancel() {
        let svc = BackgroundJobService::new();
        let input = JobStartInput::new("cancel_run".into());
        let info = svc
            .start(input, || async {
                // Simulate long work — will be cancelled before completing.
                tokio::time::sleep(std::time::Duration::from_secs(60)).await;
                Ok("never".into())
            })
            .await;

        let cancelled = svc.cancel(&info.id).await;
        let final_info = cancelled.expect("cancelled info");
        assert_eq!(final_info.status, JobStatus::Cancelled);
    }

    #[tokio::test]
    async fn test_service_wait_with_timeout() {
        let svc = BackgroundJobService::new();
        let input = JobStartInput::new("timeout_run".into());
        let info = svc
            .start(input, || async {
                tokio::time::sleep(std::time::Duration::from_secs(60)).await;
                Ok("late".into())
            })
            .await;

        let waited = svc
            .wait(JobWaitInput {
                id: info.id.clone(),
                timeout: Some(50),
            })
            .await;
        assert!(waited.timed_out);
        let info_snapshot = waited.info.expect("info should be present on timeout");
        assert_eq!(info_snapshot.status, JobStatus::Running);
    }

    #[tokio::test]
    async fn test_service_list_sorted_by_started_at() {
        let svc = BackgroundJobService::new();

        let info1 = svc
            .start(JobStartInput::new("a".into()), || async { Ok("1".into()) })
            .await;
        tokio::time::sleep(std::time::Duration::from_millis(5)).await;
        let info2 = svc
            .start(JobStartInput::new("b".into()), || async { Ok("2".into()) })
            .await;

        let list = svc.list().await;
        assert_eq!(list.len(), 2);
        // First job should have smaller started_at
        assert!(list[0].started_at <= list[1].started_at);
        // IDs match
        assert_eq!(list[0].id, info1.id);
        assert_eq!(list[1].id, info2.id);
    }

    #[tokio::test]
    async fn test_service_get() {
        let svc = BackgroundJobService::new();
        let input = JobStartInput::new("get_test".into());
        let info = svc.start(input, || async { Ok("ok".into()) }).await;

        let found = svc.get(&info.id).await;
        assert!(found.is_some());
        assert_eq!(found.unwrap().id, info.id);

        let missing = svc.get("nonexistent").await;
        assert!(missing.is_none());
    }

    #[tokio::test]
    async fn test_service_extend() {
        let svc = BackgroundJobService::new();
        let input = JobStartInput::new("extend_test".into());
        let info = svc.start(input, || async { Ok("first".into()) }).await;

        // Wait for initial job to complete
        let _ = svc
            .wait(JobWaitInput {
                id: info.id.clone(),
                timeout: None,
            })
            .await;

        // Extend the completed job — should settle immediately
        let extended = svc
            .extend(
                JobExtendInput {
                    id: info.id.clone(),
                },
                || async { Ok("second".into()) },
            )
            .await;
        // Since the job is already terminal, extend returns current info
        assert_eq!(extended.status, JobStatus::Completed);
    }

    #[tokio::test]
    async fn test_service_cancel_already_terminal() {
        let svc = BackgroundJobService::new();
        let input = JobStartInput::new("terminal_cancel".into());
        let info = svc.start(input, || async { Ok("done".into()) }).await;

        // Wait for completion
        let _ = svc
            .wait(JobWaitInput {
                id: info.id.clone(),
                timeout: None,
            })
            .await;

        // Cancel on already-completed job returns info without error
        let result = svc.cancel(&info.id).await;
        assert!(result.is_some());
        assert_eq!(result.unwrap().status, JobStatus::Completed);
    }

    #[tokio::test]
    async fn test_service_wait_nonexistent() {
        let svc = BackgroundJobService::new();
        let result = svc
            .wait(JobWaitInput {
                id: "nonexistent".into(),
                timeout: None,
            })
            .await;
        assert!(!result.timed_out);
        assert!(result.info.is_none());
    }

    // ── Promotion ─────────────────────────────────────────────────────

    #[tokio::test]
    async fn test_wait_for_promotion_already_background() {
        let svc = BackgroundJobService::new();
        let input = JobStartInput {
            id: Some("promoted_01".into()),
            type_: "test".into(),
            title: None,
            metadata: Some(serde_json::json!({"background": true})),
            on_promote: None,
        };
        let info = svc.start(input, || async { Ok("done".into()) }).await;

        // Already has metadata.background == true → returns immediately
        let result = svc.wait_for_promotion(&info.id).await;
        assert!(result.is_some());
        assert_eq!(result.unwrap().id, info.id);
    }

    #[tokio::test]
    async fn test_wait_for_promotion_nonexistent() {
        let svc = BackgroundJobService::new();
        let result = svc.wait_for_promotion("does_not_exist").await;
        assert!(result.is_none());
    }

    #[tokio::test]
    async fn test_promote_sets_metadata_background() {
        let svc = BackgroundJobService::new();
        let input = JobStartInput::new("promote_01".into());
        let info = svc
            .start(input, || async {
                tokio::time::sleep(std::time::Duration::from_secs(60)).await;
                Ok("done".into())
            })
            .await;

        let promoted = svc.promote(&info.id).await;
        let p = promoted.expect("promoted info");
        assert_eq!(p.id, info.id);
        let meta = p.metadata.expect("metadata present");
        assert_eq!(meta.get("background").unwrap(), true);
    }

    #[tokio::test]
    async fn test_promote_nonexistent() {
        let svc = BackgroundJobService::new();
        let result = svc.promote("nonexistent").await;
        assert!(result.is_none());
    }

    #[tokio::test]
    async fn test_promote_already_background_returns_immediately() {
        let svc = BackgroundJobService::new();
        let input = JobStartInput {
            id: Some("double_promote".into()),
            type_: "test".into(),
            title: None,
            metadata: Some(serde_json::json!({"background": true})),
            on_promote: None,
        };
        let info = svc.start(input, || async { Ok("done".into()) }).await;

        // Already promoted → should return immediately
        let result = svc.promote(&info.id).await;
        let p = result.expect("already promoted");
        assert_eq!(p.id, info.id);
    }

    #[tokio::test]
    async fn test_promote_runs_on_promote_callback() {
        use std::sync::atomic::{AtomicUsize, Ordering};
        use std::sync::Arc;

        let counter = Arc::new(AtomicUsize::new(0));
        let counter_clone = counter.clone();
        let on_promote: OnPromoteFn = Arc::new(move || {
            counter_clone.fetch_add(1, Ordering::SeqCst);
        });

        let svc = BackgroundJobService::new();
        let input = JobStartInput {
            id: Some("cb_test".into()),
            type_: "test".into(),
            title: None,
            metadata: None,
            on_promote: Some(on_promote),
        };
        let info = svc
            .start(input, || async {
                tokio::time::sleep(std::time::Duration::from_secs(60)).await;
                Ok("done".into())
            })
            .await;

        assert_eq!(counter.load(Ordering::SeqCst), 0);
        let _ = svc.promote(&info.id).await;
        assert_eq!(counter.load(Ordering::SeqCst), 1);
    }

    #[tokio::test]
    async fn test_wait_for_promotion_after_promote() {
        let svc = BackgroundJobService::new();
        let input = JobStartInput::new("wait_promote".into());
        let info = svc
            .start(input, || async {
                tokio::time::sleep(std::time::Duration::from_secs(60)).await;
                Ok("done".into())
            })
            .await;

        let id = info.id.clone();

        // Spawn a task that promotes after a short delay
        let svc_clone = svc.clone();
        let id_clone = id.clone();
        tokio::spawn(async move {
            tokio::time::sleep(std::time::Duration::from_millis(50)).await;
            svc_clone.promote(&id_clone).await;
        });

        // This should return once promoted
        let result = svc.wait_for_promotion(&id).await;
        let p = result.expect("should have promotion info");
        assert_eq!(p.id, id);
        let meta = p.metadata.expect("metadata present");
        assert_eq!(meta.get("background").unwrap(), true);
    }

    #[test]
    fn test_on_promote_fn_type_is_send_sync() {
        fn assert_send_sync<T: Send + Sync>() {}
        assert_send_sync::<OnPromoteFn>();
    }
}
