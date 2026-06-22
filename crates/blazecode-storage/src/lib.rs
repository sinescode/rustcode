//! # blazecode-storage
//!
//! SQLite + JSONL storage layer with WAL crash recovery.
//!
//! ## Architecture
//!
//! ```text
//!          ┌───────────────────┐
//!          │   JSONL Archive    │  ← Append-only, compressed, immutable
//!          └────────┬──────────┘
//!                    │ replay
//!          ┌────────▼──────────┐
//!          │    SQLite Store    │  ← Indexed, fast queries, WAL mode
//!          │  (sessions, turns, │
//!          │   events, tools)   │
//!          └────────┬──────────┘
//!                    │ load/save
//!          ┌────────▼──────────┐
//!          │  Rust code state   │
//!          │  (Session, Turn)   │
//!          └───────────────────┘
//! ```
//!
//! ## Why not just SQLite?
//!
//! - JSONL provides crash-proof append-only archiving
//! - SQLite provides O(1) indexed reads, tail queries, and fast deletes
//! - Combined: SQLite for active queries, JSONL for cold storage + replay

#![deny(unsafe_code)]
#![deny(missing_docs)]

pub mod store;
pub mod schema;
pub mod jsonl;
pub mod migration;

pub use store::{Store, StoreConfig, StoreError};
pub use schema::*;
pub use jsonl::JsonlArchive;
pub use migration::Migration;
