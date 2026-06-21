//! Session sharing — create and manage shared session links.
//!
//! Ported from:
//! - `packages/opencode/src/share/share-next.ts` — ShareNext service (API layer)
//! - `packages/opencode/src/share/session.ts` — SessionShare service (high-level)
//! - `packages/core/src/share/sql.ts` — share table schema
//!
//! OpenCode commit: 5d0f86606ac30690f79f0a6a9f41a1f49fe95d0b
//!
//! ## Architecture
//!
//! Sharing uploads session data (messages, parts, diffs, models) to a remote
//! API, creating a publicly-viewable URL. The ShareNext service handles the
//! HTTP transport; SessionShare adds auto-share and unshare logic on top.

use serde::{Deserialize, Serialize};

// ── Types ─────────────────────────────────────────────────────────────

/// API endpoint configuration for share operations.
///
/// # Source
/// `packages/opencode/src/share/share-next.ts` lines 25–30.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ShareApi {
    pub create: String,
    pub sync: String,
    pub remove: String,
    pub data: String,
}

/// HTTP request context for share operations.
///
/// # Source
/// `packages/opencode/src/share/share-next.ts` lines 32–36.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ShareReq {
    pub headers: std::collections::HashMap<String, String>,
    pub api: ShareApi,
    pub base_url: String,
}

/// A created share — contains the public URL and secret for data access.
///
/// # Source
/// `packages/opencode/src/share/share-next.ts` lines 38–43.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Share {
    pub id: String,
    pub url: String,
    pub secret: String,
}

// ── ShareNext Interface (HTTP transport) ──────────────────────────────

/// The low-level share service interface.
///
/// # Source
/// `packages/opencode/src/share/share-next.ts` lines 73–79.
pub trait ShareNextInterface: Send + Sync {
    /// Initialize the share service (fetch account info, etc.).
    fn init(&self) -> impl std::future::Future<Output = Result<(), crate::error::Error>> + Send;

    /// Get the share URL for the current project.
    fn url(&self) -> impl std::future::Future<Output = Result<String, crate::error::Error>> + Send;

    /// Get the HTTP request context (headers, API endpoints).
    fn request(&self) -> impl std::future::Future<Output = Result<ShareReq, crate::error::Error>> + Send;

    /// Create a new share for a session.
    fn create(
        &self,
        session_id: &str,
    ) -> impl std::future::Future<Output = Result<Share, crate::error::Error>> + Send;

    /// Remove a share for a session.
    fn remove(
        &self,
        session_id: &str,
    ) -> impl std::future::Future<Output = Result<(), crate::error::Error>> + Send;
}

// ── SessionShare Interface (high-level) ───────────────────────────────

/// High-level session share operations.
///
/// # Source
/// `packages/opencode/src/share/session.ts` lines 9–13.
pub trait SessionShareInterface: Send + Sync {
    /// Create a new session, optionally auto-sharing it.
    fn create(
        &self,
        session_id: Option<&str>,
    ) -> impl std::future::Future<Output = Result<String, crate::error::Error>> + Send;

    /// Share an existing session and return the public URL.
    fn share(
        &self,
        session_id: &str,
    ) -> impl std::future::Future<Output = Result<String, crate::error::Error>> + Send;

    /// Unshare a session (revoke public access).
    fn unshare(
        &self,
        session_id: &str,
    ) -> impl std::future::Future<Output = Result<(), crate::error::Error>> + Send;
}

// ── Tests ─────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_share_serde() {
        let share = Share {
            id: "shr_abc123".into(),
            url: "https://opencode.ai/share/abc123".into(),
            secret: "secret_key".into(),
        };
        let json = serde_json::to_string(&share).unwrap();
        let parsed: Share = serde_json::from_str(&json).unwrap();
        assert_eq!(parsed.id, "shr_abc123");
        assert_eq!(parsed.url, "https://opencode.ai/share/abc123");
    }

    #[test]
    fn test_share_req_serde() {
        let mut headers = std::collections::HashMap::new();
        headers.insert("Authorization".into(), "Bearer token".into());
        let req = ShareReq {
            headers,
            api: ShareApi {
                create: "/api/share".into(),
                sync: "/api/share/sync".into(),
                remove: "/api/share/remove".into(),
                data: "/api/share/data".into(),
            },
            base_url: "https://opencode.ai".into(),
        };
        let json = serde_json::to_string(&req).unwrap();
        let parsed: ShareReq = serde_json::from_str(&json).unwrap();
        assert_eq!(parsed.base_url, "https://opencode.ai");
        assert_eq!(parsed.api.create, "/api/share");
    }
}
