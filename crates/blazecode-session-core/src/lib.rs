//! # blazecode-session-core
//!
//! The formal session state machine. Every session in BlazeCode++ is in exactly
//! one `SessionState` at any time. Transitions are validated at compile time.
//!
//! ## Why this exists
//!
//! The original BlazeCode session.rs is 4,184 lines with ad-hoc state management
//! using `bool` flags and `Option<SessionState>`. This led to:
//! - Race conditions from invalid state transitions
//! - Unclear what states a session could be in
//! - No way to reason about session lifecycle formally
//!
//! BlazeCode++ replaces this with a typed `SessionState` enum where every
//! transition is documented and validated.
//!
//! ## State machine diagram
//!
//! ```text
//! IDLE → BUILDING_CONTEXT → SENDING_REQUEST → WAITING_STREAM
//!                                                    │
//!                                                    ▼
//!                                          PROCESSING_TOOL
//!                                                    │
//!                                                    ▼
//!                                          MID_TURN_PRECHECK
//!                                           │           │
//!                                           ▼           ▼
//!                                       COMPACTING   SENDING_REQUEST
//!                                           │
//!                                           ▼
//!                                        RETRY → SENDING_REQUEST
//! ```

#![deny(unsafe_code)]
#![deny(missing_docs)]

pub mod context;
pub mod state;
pub mod error;

pub use state::SessionState;
pub use error::{SessionError, StateTransitionError};

/// A unique session identifier.
pub type SessionId = uuid::Uuid;

/// Server-side event kind.
pub type TurnId = uuid::Uuid;

/// A monotonic sequence number within a session.
pub type SeqNo = u64;

/// Number of tokens in a context / message.
pub type TokenCount = usize;

/// Unix timestamp milliseconds.
pub type TimestampMs = i64;
