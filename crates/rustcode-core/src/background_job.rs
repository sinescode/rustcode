//! Background job types — status tracking for long-running operations.
//!
//! Ported from: `packages/core/src/background-job.ts`
//! OpenCode commit: 5d0f86606ac30690f79f0a6a9f41a1f49fe95d0b
//!
//! This module provides the data types for background job lifecycle management.
//! The TS source uses Effect.ts for the full job registry (scoped, process-local,
//! with `Deferred`-based synchronization). The registry implementation itself lives
//! in a separate runtime module; this file only defines the pure data types and
//! their lifecycle helpers.

use serde::{Deserialize, Serialize};

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
        matches!(self, JobStatus::Completed | JobStatus::Error | JobStatus::Cancelled)
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
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct JobStartInput {
    /// Optional job identifier. If omitted, one is generated.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub id: Option<String>,

    /// Job type discriminator.
    ///
    /// Renamed from `type` (reserved keyword in Rust).
    #[serde(rename = "type")]
    pub type_: String,

    /// Optional human-readable title.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub title: Option<String>,

    /// Arbitrary metadata attached at creation.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub metadata: Option<serde_json::Value>,

    /// Placeholder for the TS `onPromote?: Effect.Effect<void>` callback.
    ///
    /// In the TS source this is an Effect that runs when the job is promoted
    /// from background to foreground. The Rust port stores this as opaque JSON
    /// until the Effect runtime layer is implemented.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub on_promote: Option<serde_json::Value>,
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
        let parsed: serde_json::Value =
            serde_json::from_str(&json).expect("parse as JSON value");
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
        let parsed: serde_json::Value =
            serde_json::from_str(&json).expect("parse as JSON value");

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
        assert_eq!(input.on_promote, None);
    }

    #[test]
    fn test_job_start_input_serialization() {
        let input = JobStartInput {
            id: Some("custom_id".into()),
            type_: "tool_call".into(),
            title: Some("My Tool".into()),
            metadata: Some(serde_json::json!({"priority": "high"})),
            on_promote: None,
        };
        let json = serde_json::to_string(&input).expect("serialize JobStartInput");
        let parsed: serde_json::Value =
            serde_json::from_str(&json).expect("parse as JSON value");
        assert_eq!(parsed["id"], "custom_id");
        assert_eq!(parsed["type"], "tool_call");
        assert_eq!(parsed["title"], "My Tool");
        assert_eq!(parsed["metadata"]["priority"], "high");
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
        let parsed: serde_json::Value =
            serde_json::from_str(&json).expect("parse as JSON value");
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
        let parsed: serde_json::Value =
            serde_json::from_str(&json).expect("parse as JSON value");
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
        let parsed: serde_json::Value =
            serde_json::from_str(&json).expect("parse as JSON value");
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
        let parsed: serde_json::Value =
            serde_json::from_str(&json).expect("parse as JSON value");
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
        let parsed: serde_json::Value =
            serde_json::from_str(&json).expect("parse as JSON value");
        assert_eq!(parsed["timedOut"], false);
        assert!(parsed.get("info").is_none());
    }
}
