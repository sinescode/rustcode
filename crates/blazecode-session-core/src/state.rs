//! # SessionState — the formal session state machine
//!
//! Every session is in exactly one state. Transitions are validated by
//! `can_transition_to()`. Invalid transitions return a `StateTransitionError`.

use crate::error::StateTransitionError;
use serde::{Deserialize, Serialize};
use std::fmt;

/// The formal session state.
///
/// ## Invariants
///
/// 1. A session is always in exactly one state.
/// 2. All states except `Idle`, `Error`, and `Done` belong to exactly one turn.
/// 3. `Compacting` is always followed by `Retry`.
/// 4. `Retry` is always followed by `SendingRequest`.
/// 5. `ProcessingTool` is always followed by `MidTurnPrecheck`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum SessionState {
    /// No active turn. Session is ready for input.
    Idle,

    /// Building the context for the next provider request.
    /// This includes system prompt, tool schemas, message history, and policies.
    BuildingContext {
        /// Number of messages being assembled.
        message_count: usize,
        /// Total estimated tokens.
        estimated_tokens: usize,
    },

    /// Sending the request to the AI provider.
    SendingRequest {
        /// Which provider/model is being used.
        provider: String,
        /// Model identifier.
        model: String,
        /// Attempt number (for retries).
        attempt: u8,
    },

    /// Waiting for a streaming response from the provider.
    WaitingStream {
        /// Provider-assigned request ID.
        request_id: Option<String>,
        /// Tokens received so far.
        tokens_received: usize,
    },

    /// Executing a tool called by the model.
    ProcessingTool {
        /// Tool name (e.g., "bash", "read_file").
        tool_name: String,
        /// Tool call ID.
        call_id: String,
        /// Elapsed time since tool started.
        elapsed: std::time::Duration,
    },

    /// Checking if the context is still within limits after a tool result.
    MidTurnPrecheck {
        /// Estimated current token count.
        current_tokens: usize,
        /// Whether compaction was triggered by overflow.
        overflow_detected: bool,
    },

    /// Compacting the context to reduce token usage.
    Compacting {
        /// Why compaction was triggered.
        reason: CompactionReason,
        /// Current token count before compaction.
        tokens_before: usize,
    },

    /// Retrying after compaction.
    Retry {
        /// Which attempt number.
        attempt: u8,
        /// Maximum retries before failing.
        max_attempts: u8,
    },

    /// An error occurred that cannot be recovered from.
    Error {
        /// The error.
        reason: SessionErrorState,
        /// Whether the session can be resumed with a /new.
        recoverable: bool,
    },

    /// Session is finished / closed.
    Done,
}

/// Why compaction was triggered.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum CompactionReason {
    /// Context token count exceeded the model's limit.
    ContextOverflow,
    /// Mid-turn precheck detected imminent overflow.
    MidTurnPrecheck,
    /// User explicitly requested compaction.
    Manual,
    /// Transcript file size exceeded the limit.
    TranscriptSize,
}

impl SessionState {
    /// Check if a transition to `next` is valid.
    ///
    /// Returns `Ok(())` if allowed, `Err(StateTransitionError)` if not.
    ///
    /// # Examples
    ///
    /// ```
    /// use blazecode_session_core::SessionState;
    ///
    /// let idle = SessionState::Idle;
    /// assert!(idle.can_transition_to(&SessionState::BuildingContext { message_count: 0, estimated_tokens: 0 }).is_ok());
    /// assert!(idle.can_transition_to(&SessionState::ProcessingTool { tool_name: "bash".into(), call_id: "".into(), elapsed: std::time::Duration::ZERO }).is_err());
    /// ```
    pub fn can_transition_to(&self, next: &Self) -> Result<(), StateTransitionError> {
        use SessionState::*;

        match (self, next) {
            // From Idle
            (Idle, BuildingContext { .. }) => Ok(()),
            (Idle, Done) => Ok(()),

            // BuildingContext → SendingRequest
            (BuildingContext { .. }, SendingRequest { .. }) => Ok(()),

            // SendingRequest → WaitingStream
            (SendingRequest { .. }, WaitingStream { .. }) => Ok(()),
            // SendingRequest → Error (provider unavailable)
            (SendingRequest { .. }, Error { .. }) => Ok(()),

            // WaitingStream → ProcessingTool
            (WaitingStream { .. }, ProcessingTool { .. }) => Ok(()),
            // WaitingStream → Idle (stream ended, no tool calls)
            (WaitingStream { .. }, Idle) => Ok(()),

            // ProcessingTool → MidTurnPrecheck
            (ProcessingTool { .. }, MidTurnPrecheck { .. }) => Ok(()),

            // MidTurnPrecheck → SendingRequest (continue)
            (MidTurnPrecheck { overflow_detected: false, .. }, SendingRequest { .. }) => Ok(()),
            // MidTurnPrecheck → Compacting (overflow)
            (MidTurnPrecheck { .. }, Compacting { .. }) => Ok(()),

            // Compacting → Retry
            (Compacting { .. }, Retry { .. }) => Ok(()),

            // Retry → SendingRequest
            (Retry { .. }, SendingRequest { .. }) => Ok(()),
            // Retry → Error (exhausted)
            (Retry { attempt, max_attempts }, Error { .. }) if *attempt >= *max_attempts => Ok(()),

            // Any → Error
            (_, Error { .. }) => Ok(()),

            // Error → Idle (recoverable)
            (Error { recoverable: true, .. }, Idle) => Ok(()),

            _ => Err(StateTransitionError::Invalid(self.clone(), next.clone())),
        }
    }

    /// Returns true if the session is actively running a turn.
    pub fn is_active(&self) -> bool {
        !matches!(self, SessionState::Idle | SessionState::Done | SessionState::Error { .. })
    }

    /// Returns true if the session can accept user input.
    pub fn can_accept_input(&self) -> bool {
        matches!(self, SessionState::Idle)
    }

    /// Human-readable label for the state.
    pub fn label(&self) -> &'static str {
        match self {
            SessionState::Idle => "idle",
            SessionState::BuildingContext { .. } => "building context",
            SessionState::SendingRequest { .. } => "sending request",
            SessionState::WaitingStream { .. } => "waiting for stream",
            SessionState::ProcessingTool { .. } => "processing tool",
            SessionState::MidTurnPrecheck { .. } => "mid-turn precheck",
            SessionState::Compacting { .. } => "compacting",
            SessionState::Retry { .. } => "retry",
            SessionState::Error { .. } => "error",
            SessionState::Done => "done",
        }
    }
}

impl fmt::Display for SessionState {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Idle => write!(f, "Idle"),
            Self::BuildingContext { message_count, .. } => {
                write!(f, "BuildingContext({} msgs)", message_count)
            }
            Self::SendingRequest { provider, model, attempt } => {
                write!(f, "SendingRequest({}/{}, attempt {})", provider, model, attempt)
            }
            Self::WaitingStream { tokens_received, .. } => {
                write!(f, "WaitingStream({} tokens)", tokens_received)
            }
            Self::ProcessingTool { tool_name, elapsed, .. } => {
                write!(f, "ProcessingTool({} @ {:?})", tool_name, elapsed)
            }
            Self::MidTurnPrecheck { current_tokens, overflow_detected } => {
                write!(f, "MidTurnPrecheck({} tokens, overflow={})", current_tokens, overflow_detected)
            }
            Self::Compacting { reason, tokens_before } => {
                write!(f, "Compacting({:?}, {}→)", reason, tokens_before)
            }
            Self::Retry { attempt, max_attempts } => {
                write!(f, "Retry({}/{})", attempt, max_attempts)
            }
            Self::Error { recoverable, .. } => {
                write!(f, "Error(recoverable={})", recoverable)
            }
            Self::Done => write!(f, "Done"),
        }
    }
}

/// Serializable error reason for session error state.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SessionErrorState {
    /// Error code.
    pub code: String,
    /// Human-readable message.
    pub message: String,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_valid_transition_idle_to_building() {
        let from = SessionState::Idle;
        let to = SessionState::BuildingContext { message_count: 1, estimated_tokens: 100 };
        assert!(from.can_transition_to(&to).is_ok());
    }

    #[test]
    fn test_invalid_transition_idle_to_processing_tool() {
        let from = SessionState::Idle;
        let to = SessionState::ProcessingTool {
            tool_name: "bash".into(),
            call_id: "call1".into(),
            elapsed: std::time::Duration::ZERO,
        };
        assert!(from.can_transition_to(&to).is_err());
    }

    #[test]
    fn test_full_cycle_valid() {
        let transitions: Vec<(SessionState, SessionState)> = vec![
            (SessionState::Idle, SessionState::BuildingContext { message_count: 5, estimated_tokens: 1000 }),
            (SessionState::BuildingContext { message_count: 5, estimated_tokens: 1000 },
             SessionState::SendingRequest { provider: "anthropic".into(), model: "claude".into(), attempt: 1 }),
            (SessionState::SendingRequest { provider: "anthropic".into(), model: "claude".into(), attempt: 1 },
             SessionState::WaitingStream { request_id: None, tokens_received: 0 }),
            (SessionState::WaitingStream { request_id: None, tokens_received: 0 },
             SessionState::ProcessingTool { tool_name: "bash".into(), call_id: "c1".into(), elapsed: std::time::Duration::ZERO }),
            (SessionState::ProcessingTool { tool_name: "bash".into(), call_id: "c1".into(), elapsed: std::time::Duration::ZERO },
             SessionState::MidTurnPrecheck { current_tokens: 50000, overflow_detected: false }),
            (SessionState::MidTurnPrecheck { current_tokens: 50000, overflow_detected: false },
             SessionState::SendingRequest { provider: "anthropic".into(), model: "claude".into(), attempt: 1 }),
        ];

        for (from, to) in &transitions {
            assert!(from.can_transition_to(to).is_ok(),
                "expected valid transition: {from} → {to}");
        }
    }

    #[test]
    fn test_compaction_cycle() {
        let transitions: Vec<(SessionState, SessionState)> = vec![
            (SessionState::Idle, SessionState::BuildingContext { message_count: 1, estimated_tokens: 50000 }),
            (SessionState::BuildingContext { message_count: 1, estimated_tokens: 50000 },
             SessionState::SendingRequest { provider: "anthropic".into(), model: "claude".into(), attempt: 1 }),
            (SessionState::SendingRequest { provider: "anthropic".into(), model: "claude".into(), attempt: 1 },
             SessionState::WaitingStream { request_id: Some("req1".into()), tokens_received: 90000 }),
            (SessionState::WaitingStream { request_id: Some("req1".into()), tokens_received: 90000 },
             SessionState::ProcessingTool { tool_name: "bash".into(), call_id: "c1".into(), elapsed: std::time::Duration::ZERO }),
            (SessionState::ProcessingTool { tool_name: "bash".into(), call_id: "c1".into(), elapsed: std::time::Duration::ZERO },
             SessionState::MidTurnPrecheck { current_tokens: 195000, overflow_detected: true }),
            (SessionState::MidTurnPrecheck { current_tokens: 195000, overflow_detected: true },
             SessionState::Compacting { reason: CompactionReason::MidTurnPrecheck, tokens_before: 195000 }),
            (SessionState::Compacting { reason: CompactionReason::MidTurnPrecheck, tokens_before: 195000 },
             SessionState::Retry { attempt: 1, max_attempts: 3 }),
            (SessionState::Retry { attempt: 1, max_attempts: 3 },
             SessionState::SendingRequest { provider: "anthropic".into(), model: "claude".into(), attempt: 2 }),
        ];

        for (from, to) in &transitions {
            assert!(from.can_transition_to(to).is_ok(),
                "expected valid transition: {from} → {to}");
        }
    }

    #[test]
    fn test_state_labels() {
        assert_eq!(SessionState::Idle.label(), "idle");
        assert_eq!(SessionState::Done.label(), "done");
    }

    #[test]
    fn test_is_active() {
        assert!(!SessionState::Idle.is_active());
        assert!(SessionState::BuildingContext { message_count: 0, estimated_tokens: 0 }.is_active());
        assert!(!SessionState::Done.is_active());
    }

    #[test]
    fn test_can_accept_input() {
        assert!(SessionState::Idle.can_accept_input());
        assert!(!SessionState::Done.can_accept_input());
        assert!(!SessionState::SendingRequest { provider: "".into(), model: "".into(), attempt: 0 }.can_accept_input());
    }

    #[test]
    fn test_retry_exhausted_transitions_to_error() {
        let retry = SessionState::Retry { attempt: 3, max_attempts: 3 };
        let error = SessionState::Error {
            reason: SessionErrorState {
                code: "retry_exhausted".into(),
                message: "Max retries reached".into(),
            },
            recoverable: true,
        };
        assert!(retry.can_transition_to(&error).is_ok());
    }

    #[test]
    fn test_error_recoverable_transitions_to_idle() {
        let error = SessionState::Error {
            reason: SessionErrorState {
                code: "rate_limited".into(),
                message: "API rate limited".into(),
            },
            recoverable: true,
        };
        assert!(error.can_transition_to(&SessionState::Idle).is_ok());
    }
}
