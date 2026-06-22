//! # Error types for session operations.

use crate::SessionState;
use thiserror::Error;

/// An invalid state transition was attempted.
#[derive(Debug, Clone, Error)]
#[error("Invalid state transition: {from} → {to}")]
pub struct StateTransitionError {
    /// The current state.
    pub from: SessionState,
    /// The attempted next state.
    pub to: SessionState,
}

impl StateTransitionError {
    /// Create a new transition error.
    pub fn Invalid(from: SessionState, to: SessionState) -> Self {
        Self { from, to }
    }
}

/// An error that occurred during session processing.
#[derive(Debug, Error)]
pub enum SessionError {
    /// Invalid state transition.
    #[error("invalid transition: {0}")]
    StateTransition(#[from] StateTransitionError),

    /// Provider returned an error.
    #[error("provider error: {0}")]
    Provider(String),

    /// Tool execution failed.
    #[error("tool error: {0}")]
    Tool(String),

    /// Compaction failed.
    #[error("compaction error: {0}")]
    Compaction(String),

    /// Timeout.
    #[error("timeout after {0:?}")]
    Timeout(std::time::Duration),

    /// Internal error.
    #[error("internal: {0}")]
    Internal(String),

    /// I/O error.
    #[error(transparent)]
    Io(#[from] std::io::Error),
}
