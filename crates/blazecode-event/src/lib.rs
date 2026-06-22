//! # blazecode-event
//!
//! Typed pub/sub event bus with projectors.
//!
//! Extracted from `blazecode-core/src/event.rs` (2,911 lines) into a focused,
//! well-typed event system.
//!
//! ## Architecture
//!
//! ```text
//! ┌──────────────┐     ┌──────────────────┐     ┌──────────────┐
//! │ EventEmitter  │────▶│  EventBus         │────▶│  Projectors   │
//! │ (publish)     │     │  (typed channels) │     │  (handlers)   │
//! └──────────────┘     └──────────────────┘     └──────────────┘
//!       │                       │                        │
//!       ▼                       ▼                        ▼
//! ┌──────────────┐     ┌──────────────────┐     ┌──────────────┐
//! │ EventLogger   │     │ EventStore       │     │ SyncEngine   │
//! │ (structured)  │     │ (SQLite backed)  │     │ (lifecycle)  │
//! └──────────────┘     └──────────────────┘     └──────────────┘
//! ```

#![deny(unsafe_code)]
#![deny(missing_docs)]

pub mod bus;
pub mod types;
pub mod projector;

pub use bus::*;
pub use types::*;
pub use projector::*;
