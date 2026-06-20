//! Session execution types — local exec, run coordinator, and error types.
//!
//! Ported from:
//! - `packages/core/src/session/execution.ts` (lines 1–24)
//! - `packages/core/src/session/execution/local.ts` (lines 1–35)
//! - `packages/core/src/session/run-coordinator.ts` (lines 1–285)
//! - `packages/core/src/session/error.ts` (lines 1–21)

use crate::session_info::SessionId;
use crate::session_message::SessionMessageId;
use serde::{Deserialize, Serialize};

// ══════════════════════════════════════════════════════════════════════════════
// Execution Interface
// ══════════════════════════════════════════════════════════════════════════════

/// Core execution interface — routes execution from session ID to runner.
///
/// # Source
/// `packages/core/src/session/execution.ts` lines 7–14 `Interface`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
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
}
