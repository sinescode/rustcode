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
use std::collections::HashMap;

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

impl ShareApi {
    /// Build the sync endpoint for a given share ID.
    pub fn sync_url(&self, share_id: &str) -> String {
        self.sync.replace("{shareID}", share_id)
    }

    /// Build the remove endpoint for a given share ID.
    pub fn remove_url(&self, share_id: &str) -> String {
        self.remove.replace("{shareID}", share_id)
    }

    /// Build the data endpoint for a given share ID.
    pub fn data_url(&self, share_id: &str) -> String {
        self.data.replace("{shareID}", share_id)
    }
}

/// HTTP request context for share operations.
///
/// # Source
/// `packages/opencode/src/share/share-next.ts` lines 32–36.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ShareReq {
    pub headers: HashMap<String, String>,
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

/// A share row stored in the database.
///
/// # Source
/// `packages/core/src/share/sql.ts` — `SessionShareTable` schema.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ShareRow {
    pub session_id: String,
    pub id: String,
    pub secret: String,
    pub url: String,
}

/// Data types that can be synced to a share.
///
/// # Source
/// `packages/opencode/src/share/share-next.ts` lines 51–71.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type")]
pub enum ShareData {
    #[serde(rename = "session")]
    Session { data: serde_json::Value },
    #[serde(rename = "message")]
    Message { data: serde_json::Value },
    #[serde(rename = "part")]
    Part { data: serde_json::Value },
    #[serde(rename = "session_diff")]
    SessionDiff { data: Vec<serde_json::Value> },
    #[serde(rename = "model")]
    Model { data: Vec<serde_json::Value> },
}

impl ShareData {
    /// Compute the dedup key for this data item (matching TS `key()` function).
    pub fn key(&self) -> String {
        match self {
            ShareData::Session { .. } => "session".to_string(),
            ShareData::Message { data } => {
                format!("message/{}", data.get("id").and_then(|v| v.as_str()).unwrap_or(""))
            }
            ShareData::Part { data } => {
                format!(
                    "part/{}/{}",
                    data.get("messageID").and_then(|v| v.as_str()).unwrap_or(""),
                    data.get("id").and_then(|v| v.as_str()).unwrap_or("")
                )
            }
            ShareData::SessionDiff { .. } => "session_diff".to_string(),
            ShareData::Model { .. } => "model".to_string(),
        }
    }
}

// ── ShareNext state ───────────────────────────────────────────────────

/// Runtime state for the ShareNext service.
///
/// # Source
/// `packages/opencode/src/share/share-next.ts` lines 45–49.
#[derive(Debug, Default)]
pub struct ShareNextState {
    /// Per-session queues of data items waiting to be synced.
    pub queue: HashMap<String, HashMap<String, ShareData>>,
    /// Cached share info per session (None = not shared, Some(None) = not cached yet).
    pub shared: HashMap<String, Option<Share>>,
}

// ── ShareNext Interface (HTTP transport) ──────────────────────────────

/// The low-level share service interface.
///
/// # Source
/// `packages/opencode/src/share/share-next.ts` lines 73–79.
#[async_trait::async_trait]
pub trait ShareNextInterface: Send + Sync {
    /// Initialize the share service (fetch account info, etc.).
    async fn init(&self) -> Result<(), crate::error::Error>;

    /// Get the share URL for the current project.
    async fn url(&self) -> Result<String, crate::error::Error>;

    /// Get the HTTP request context (headers, API endpoints).
    async fn request(&self) -> Result<ShareReq, crate::error::Error>;

    /// Create a new share for a session.
    async fn create(
        &self,
        session_id: &str,
    ) -> Result<Share, crate::error::Error>;

    /// Remove a share for a session.
    async fn remove(
        &self,
        session_id: &str,
    ) -> Result<(), crate::error::Error>;
}

// ── ShareNext service implementation ──────────────────────────────────

/// Concrete implementation of [`ShareNextInterface`] using reqwest.
///
/// # Source
/// Ported from `packages/opencode/src/share/share-next.ts` lines 112–363.
pub struct ShareNextService {
    /// Reqwest HTTP client.
    client: reqwest::Client,
    /// Database pool for share persistence.
    pool: Option<sqlx::SqlitePool>,
    /// Runtime state (queues, caches).
    state: std::sync::Mutex<ShareNextState>,
    /// Whether sharing is disabled.
    disabled: bool,
}

impl ShareNextService {
    /// Create a new ShareNext service.
    pub fn new(pool: Option<sqlx::SqlitePool>) -> Self {
        let disabled = std::env::var("OPENCODE_DISABLE_SHARE")
            .map(|v| v == "true" || v == "1")
            .unwrap_or(false);

        Self {
            client: reqwest::Client::new(),
            pool,
            state: std::sync::Mutex::new(ShareNextState::default()),
            disabled,
        }
    }

    /// Build the ShareApi from a resource name.
    ///
    /// # Source
    /// `packages/opencode/src/share/share-next.ts` lines 85–92.
    fn api(resource: &str) -> ShareApi {
        ShareApi {
            create: format!("/api/{resource}"),
            sync: format!("/api/{resource}/{{shareID}}/sync"),
            remove: format!("/api/{resource}/{{shareID}}"),
            data: format!("/api/{resource}/{{shareID}}/data"),
        }
    }

    /// Get the cached share for a session, querying DB if needed.
    ///
    /// # Source
    /// `packages/opencode/src/share/share-next.ts` lines 235–245.
    async fn get_cached(&self, session_id: &str) -> Result<Option<Share>, crate::error::Error> {
        {
            let state = self.state.lock().unwrap();
            if let Some(cached) = state.shared.get(session_id) {
                return Ok(cached.clone());
            }
        }

        // Query DB
        let share = if let Some(ref pool) = self.pool {
            let row: Option<(String, String, String)> =
                sqlx::query_as("SELECT id, secret, url FROM session_share WHERE session_id = ?1")
                    .bind(session_id)
                    .fetch_optional(pool)
                    .await
                    .map_err(|e| crate::error::Error::Database(e.to_string()))?;

            row.map(|(id, secret, url)| Share { id, secret, url })
        } else {
            None
        };

        {
            let mut state = self.state.lock().unwrap();
            state.shared.insert(session_id.to_string(), share.clone());
        }

        Ok(share)
    }

    /// Get or create the dedup queue for a session.
    fn get_queue<'a>(
        state: &'a mut ShareNextState,
        session_id: &str,
    ) -> &'a mut HashMap<String, ShareData> {
        state
            .queue
            .entry(session_id.to_string())
            .or_insert_with(HashMap::new)
    }

    /// Sync queued data items for a session to the remote API.
    ///
    /// # Source
    /// `packages/opencode/src/share/share-next.ts` lines 247–272.
    async fn flush(&self, session_id: &str) -> Result<(), crate::error::Error> {
        if self.disabled {
            return Ok(());
        }

        let queued = {
            let mut state = self.state.lock().unwrap();
            state.queue.remove(session_id)
        };

        let Some(items_map) = queued else {
            return Ok(());
        };

        let share = self.get_cached(session_id).await?;
        let Some(share) = share else {
            return Ok(());
        };

        let req = self.request().await?;
        let items: Vec<&ShareData> = items_map.values().collect();
        let body = serde_json::json!({
            "secret": share.secret,
            "data": items
        });

        let url = format!("{}{}", req.base_url, req.api.sync_url(&share.id));
        let res = self
            .client
            .post(&url)
            .headers(build_headers(&req.headers))
            .json(&body)
            .send()
            .await
            .map_err(|e| crate::error::Error::Network(e.to_string()))?;

        if res.status().as_u16() >= 400 {
            tracing::warn!(
                session_id,
                share_id = %share.id,
                status = %res.status(),
                "failed to sync share"
            );
        }

        Ok(())
    }

    /// Perform a full sync of all session data.
    ///
    /// # Source
    /// `packages/opencode/src/share/share-next.ts` lines 274–299.
    async fn full_sync(
        &self,
        session_id: &str,
        data: Vec<ShareData>,
    ) -> Result<(), crate::error::Error> {
        if self.disabled {
            return Ok(());
        }

        let mut state = self.state.lock().unwrap();
        let queue = Self::get_queue(&mut state, session_id);

        for item in data {
            queue.insert(item.key(), item);
        }

        // Queue will be flushed shortly after (not spawned as separate fiber in Rust,
        // but the caller should trigger flush).
        Ok(())
    }

    /// Enqueue data items for batched sync to the given session share.
    ///
    /// # Source
    /// `packages/opencode/src/share/share-next.ts` lines 124–147.
    pub fn sync(&self, session_id: &str, data: Vec<ShareData>) {
        if self.disabled {
            return;
        }

        let mut state = self.state.lock().unwrap();
        let queue = Self::get_queue(&mut state, session_id);

        for item in data {
            queue.insert(item.key(), item);
        }
    }
}

#[async_trait::async_trait]
impl ShareNextInterface for ShareNextService {
    /// Initialize the share service.
    ///
    /// # Source
    /// `packages/opencode/src/share/share-next.ts` lines 301–304.
    async fn init(&self) -> Result<(), crate::error::Error> {
        if self.disabled {
            return Ok(());
        }
        Ok(())
    }

    /// Get the base URL for sharing.
    ///
    /// # Source
    /// `packages/opencode/src/share/share-next.ts` lines 306–308.
    async fn url(&self) -> Result<String, crate::error::Error> {
        let req = self.request().await?;
        Ok(req.base_url)
    }

    /// Build the HTTP request context.
    ///
    /// # Source
    /// `packages/opencode/src/share/share-next.ts` lines 206–222.
    async fn request(&self) -> Result<ShareReq, crate::error::Error> {
        let base_url = std::env::var("OPENCODE_CONSOLE_URL")
            .unwrap_or_else(|_| "https://opncd.ai".to_string());

        let headers = HashMap::new();
        let api = Self::api("share");

        Ok(ShareReq {
            headers,
            api,
            base_url,
        })
    }

    /// Create a new share for a session.
    ///
    /// # Source
    /// `packages/opencode/src/share/share-next.ts` lines 310–336.
    async fn create(
        &self,
        session_id: &str,
    ) -> Result<Share, crate::error::Error> {
        if self.disabled {
            return Ok(Share {
                id: String::new(),
                url: String::new(),
                secret: String::new(),
            });
        }

        tracing::info!(session_id, "creating share");

        let req = self.request().await?;
        let url = format!("{}{}", req.base_url, req.api.create);
        let body = serde_json::json!({ "sessionID": session_id });

        let res = self
            .client
            .post(&url)
            .headers(build_headers(&req.headers))
            .json(&body)
            .send()
            .await
            .map_err(|e| crate::error::Error::Network(e.to_string()))?;

        if !res.status().is_success() {
            return Err(crate::error::Error::Network(format!(
                "share creation failed with status {}",
                res.status()
            )));
        }

        let share: Share = res
            .json()
            .await
            .map_err(|e| crate::error::Error::Network(format!("parse share response: {e}")))?;

        // Persist to DB
        if let Some(ref pool) = self.pool {
            sqlx::query(
                "INSERT OR REPLACE INTO session_share (session_id, id, secret, url) \
                 VALUES (?1, ?2, ?3, ?4)",
            )
            .bind(session_id)
            .bind(&share.id)
            .bind(&share.secret)
            .bind(&share.url)
            .execute(pool)
            .await
            .map_err(|e| crate::error::Error::Database(e.to_string()))?;
        }

        {
            let mut state = self.state.lock().unwrap();
            state.shared.insert(session_id.to_string(), Some(share.clone()));
        }

        // Full sync will be triggered by the caller
        Ok(share)
    }

    /// Remove a share for a session.
    ///
    /// # Source
    /// `packages/opencode/src/share/share-next.ts` lines 338–359.
    async fn remove(
        &self,
        session_id: &str,
    ) -> Result<(), crate::error::Error> {
        if self.disabled {
            return Ok(());
        }

        tracing::info!(session_id, "removing share");

        let share = self.get_cached(session_id).await?;

        let share = match share {
            Some(share) => share,
            None => {
                let mut state = self.state.lock().unwrap();
                state.shared.remove(session_id);
                state.queue.remove(session_id);
                return Ok(());
            }
        };

        let req = self.request().await?;
        let url = format!("{}{}", req.base_url, req.api.remove_url(&share.id));

        let body = serde_json::json!({ "secret": share.secret });
        let _ = self
            .client
            .delete(&url)
            .headers(build_headers(&req.headers))
            .json(&body)
            .send()
            .await;

        // Delete from DB
        if let Some(ref pool) = self.pool {
            let _ = sqlx::query("DELETE FROM session_share WHERE session_id = ?1")
                .bind(session_id)
                .execute(pool)
                .await;
        }

        {
            let mut state = self.state.lock().unwrap();
            state.shared.remove(session_id);
            state.queue.remove(session_id);
        }

        Ok(())
    }
}

// ── SessionShare Interface (high-level) ───────────────────────────────

/// High-level session share operations.
///
/// # Source
/// `packages/opencode/src/share/session.ts` lines 9–13.
#[async_trait::async_trait]
pub trait SessionShareInterface: Send + Sync {
    /// Create a new session, optionally auto-sharing it.
    async fn create(
        &self,
        session_id: Option<&str>,
    ) -> Result<String, crate::error::Error>;

    /// Share an existing session and return the public URL.
    async fn share(
        &self,
        session_id: &str,
    ) -> Result<String, crate::error::Error>;

    /// Unshare a session (revoke public access).
    async fn unshare(
        &self,
        session_id: &str,
    ) -> Result<(), crate::error::Error>;
}

// ── Helpers ───────────────────────────────────────────────────────────

/// Build reqwest header map from a string map.
fn build_headers(headers: &HashMap<String, String>) -> reqwest::header::HeaderMap {
    let mut map = reqwest::header::HeaderMap::new();
    for (key, value) in headers {
        if let (Ok(name), Ok(val)) = (
            reqwest::header::HeaderName::from_bytes(key.as_bytes()),
            reqwest::header::HeaderValue::from_str(value),
        ) {
            map.insert(name, val);
        }
    }
    map
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
                sync: "/api/share/{shareID}/sync".into(),
                remove: "/api/share/{shareID}".into(),
                data: "/api/share/{shareID}/data".into(),
            },
            base_url: "https://opencode.ai".into(),
        };
        let json = serde_json::to_string(&req).unwrap();
        let parsed: ShareReq = serde_json::from_str(&json).unwrap();
        assert_eq!(parsed.base_url, "https://opencode.ai");
        assert_eq!(parsed.api.create, "/api/share");
    }

    #[test]
    fn test_share_api_urls() {
        let api = ShareApi {
            create: "/api/share".into(),
            sync: "/api/share/{shareID}/sync".into(),
            remove: "/api/share/{shareID}".into(),
            data: "/api/share/{shareID}/data".into(),
        };
        assert_eq!(api.sync_url("shr_1"), "/api/share/shr_1/sync");
        assert_eq!(api.remove_url("shr_1"), "/api/share/shr_1");
        assert_eq!(api.data_url("shr_1"), "/api/share/shr_1/data");
    }

    #[test]
    fn test_share_data_key() {
        let session = ShareData::Session {
            data: serde_json::json!({"id": "ses_1"}),
        };
        assert_eq!(session.key(), "session");

        let msg = ShareData::Message {
            data: serde_json::json!({"id": "msg_1"}),
        };
        assert_eq!(msg.key(), "message/msg_1");

        let part = ShareData::Part {
            data: serde_json::json!({"messageID": "msg_1", "id": "part_1"}),
        };
        assert_eq!(part.key(), "part/msg_1/part_1");
    }

    #[test]
    fn test_share_row_serde() {
        let row = ShareRow {
            session_id: "ses_001".into(),
            id: "shr_001".into(),
            secret: "s3cret".into(),
            url: "https://opencode.ai/share/abc".into(),
        };
        let json = serde_json::to_string(&row).unwrap();
        let parsed: ShareRow = serde_json::from_str(&json).unwrap();
        assert_eq!(parsed.session_id, "ses_001");
        assert_eq!(parsed.id, "shr_001");
    }
}
