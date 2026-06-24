//! # blazecode-sandbox
//!
//! **THE single gate for all process execution.**
//!
//! No other crate in BlazeCode++ may call `std::process::Command` directly.
//! This is enforced by:
//!   1. `cargo-udeps` custom lint reviewing all direct `std::process` calls
//!   2. Code review: every `std::process::Command` must reference this crate
//!   3. `cargo deny` — binaries produced must have zero raw Command calls
//!
//! ## Architecture
//!
//! ```text
//! ┌─────────────────────────────────────────────┐
//! │              blazecode-sandbox                │
//! │                                             │
//! │  ┌──────────────┐  ┌──────────────────────┐ │
//! │  │ SecurityPolicy│  │ ExecKind             │ │
//! │  │ - read_paths  │  │ - Sandboxed(seccomp) │ │
//! │  │ - write_paths │  │ - Containerized      │ │
//! │  │ - network     │  │ - Unsafe(opt-in)     │ │
//! │  │ - env_allow   │  └──────────────────────┘ │
//! │  │ - timeout     │                           │
//! │  └──────┬───────┘                           │
//! │         │                                     │
//! │  ┌──────┴─────────────────────────────────┐ │
//! │  │ run(SecurityPolicy, CommandSpec)        │ │
//! │  │  -> ExecResult                          │ │
//! │  │                                         │ │
//! │  │  1. Validate policy                     │ │
//! │  │  2. Apply Landlock rules (Linux)        │ │
//! │  │  3. Apply seccomp-bpf (Linux)           │ │
//! │  │  4. Spawn process                       │ │
//! │  │  5. Audit & return                      │ │
//! │  └─────────────────────────────────────────┘ │
//! └─────────────────────────────────────────────┘
//! ```

#![deny(unsafe_code)]
#![deny(missing_docs)]
#![forbid(unsafe_op_in_unsafe_fn)]

use std::time::Duration;
use thiserror::Error;

// ---------------------------------------------------------------------------
// Re-exports
// ---------------------------------------------------------------------------

pub use exec::*;
pub use policy::*;

// ---------------------------------------------------------------------------
// Public modules
// ---------------------------------------------------------------------------

pub mod exec;
pub mod policy;
mod audit;

#[cfg(target_os = "linux")]
pub mod linux;

#[cfg(test)]
pub mod mock;

/// Error types for sandbox operations.
#[derive(Debug, Error)]
pub enum SandboxError {
    /// The security policy rejected the operation.
    #[error("policy violation: {0}")]
    PolicyViolation(String),

    /// A sandbox mechanism failed to apply.
    #[error("sandbox setup failed: {0}")]
    SetupFailed(String),

    /// The process was killed by a signal.
    #[error("process terminated by signal: {0}")]
    SignalTerminated(String),

    /// The process timed out.
    #[error("process timed out after {0:?}")]
    Timeout(Duration),

    /// Internal error.
    #[error("{0}")]
    Internal(String),

    /// I/O error wrapping.
    #[error(transparent)]
    Io(#[from] std::io::Error),
}
