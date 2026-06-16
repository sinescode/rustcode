//! Unique identifier generation.
//!
//! Ported from: `packages/opencode/src/id/id.ts`

use chrono::Utc;
use uuid::Uuid;

/// Generate a unique identifier with an optional prefix.
///
/// # Source
/// Ported from `packages/opencode/src/id/id.ts`.
pub fn create(prefix: &str) -> String {
    let timestamp = Utc::now().timestamp_millis();
    let random = &Uuid::new_v4().to_string()[..8];
    format!("{prefix}_{timestamp}_{random}")
}

/// Generate a descending timestamp-based ID (newest first sort order).
pub fn descending(prefix: &str) -> String {
    let timestamp = u64::MAX - (Utc::now().timestamp_millis() as u64);
    let random = &Uuid::new_v4().to_string()[..8];
    format!("{prefix}_{timestamp}_{random}")
}

/// Generate an ascending timestamp-based ID (oldest first sort order).
pub fn ascending(prefix: &str) -> String {
    let timestamp = Utc::now().timestamp_millis() as u64;
    let random = &Uuid::new_v4().to_string()[..8];
    format!("{prefix}_{timestamp}_{random}")
}

/// Session ID type
pub type SessionId = String;

/// Message ID type
pub type MessageId = String;

/// Part ID type
pub type PartId = String;

/// Provider ID type
pub type ProviderId = String;

/// Model ID type
pub type ModelId = String;

/// Project ID type
pub type ProjectId = String;
