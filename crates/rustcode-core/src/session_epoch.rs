//! Context epoch persistence — initialize, prepare, and request replacement.
//!
//! Ported from:
//! - `packages/core/src/session/history.ts` — epoch management (lines 58–79)
//! - `packages/opencode/src/session/context-epoch.ts` (lines 1–120)

use crate::database::{ContextEpochRow, DatabaseService, DatabaseServiceError};
use crate::session_history::ContextEpoch;
use serde_json;
use std::sync::Arc;

/// Error type for context epoch operations.
#[derive(Debug, thiserror::Error)]
pub enum EpochError {
    #[error("Database error: {0}")]
    Database(String),

    #[error("Session not found: {0}")]
    NotFound(String),

    #[error("Epoch not initialized for session: {0}")]
    NotInitialized(String),

    #[error("{0}")]
    Other(String),
}

impl From<DatabaseServiceError> for EpochError {
    fn from(e: DatabaseServiceError) -> Self {
        EpochError::Database(e.to_string())
    }
}

// ══════════════════════════════════════════════════════════════════════════════
// EpochManager
// ══════════════════════════════════════════════════════════════════════════════

/// Manages context epoch lifecycle — initialization, preparation, replacement.
///
/// # Source
/// Ported from:
/// - `packages/core/src/session/history.ts` — epoch management
/// - `packages/opencode/src/session/context-epoch.ts` — full lifecycle
pub struct EpochManager {
    db: Arc<DatabaseService>,
}

impl EpochManager {
    /// Create a new epoch manager.
    pub fn new(db: Arc<DatabaseService>) -> Self {
        Self { db }
    }

    /// Initialize a new context epoch for a session.
    ///
    /// Sets up the initial baseline with the provided snapshot and agent.
    /// If an epoch already exists, it is replaced.
    ///
    /// # Source
    /// Ported from `packages/core/src/session/history.ts` — `initializeEpoch`.
    pub async fn initialize_epoch(
        &self,
        session_id: &str,
        baseline: &str,
        agent: &str,
        snapshot: &serde_json::Value,
        baseline_seq: u64,
    ) -> Result<ContextEpoch, EpochError> {
        let snapshot_str = serde_json::to_string(snapshot)
            .map_err(|e| EpochError::Other(format!("Serialize snapshot: {e}")))?;

        self.db
            .upsert_context_epoch(
                session_id,
                baseline,
                agent,
                &snapshot_str,
                baseline_seq as i64,
                None, // replacement_seq starts as None
                1,    // Initial revision
            )
            .await?;

        Ok(ContextEpoch {
            session_id: session_id.to_string(),
            baseline: baseline.to_string(),
            agent: agent.to_string(),
            snapshot: snapshot.clone(),
            baseline_seq,
            replacement_seq: None,
            revision: 1,
        })
    }

    /// Prepare an epoch for new messages — update the baseline and snapshot.
    ///
    /// This is called after a compaction or context update to establish
    /// a new baseline for subsequent messages.
    ///
    /// # Source
    /// Ported from `packages/core/src/session/history.ts` — `prepareEpoch`.
    pub async fn prepare_epoch(
        &self,
        session_id: &str,
        baseline: &str,
        snapshot: &serde_json::Value,
    ) -> Result<ContextEpoch, EpochError> {
        // Get existing epoch
        let existing = self
            .db
            .get_context_epoch(session_id)
            .await?
            .ok_or_else(|| EpochError::NotInitialized(session_id.to_string()))?;

        let snapshot_str = serde_json::to_string(snapshot)
            .map_err(|e| EpochError::Other(format!("Serialize snapshot: {e}")))?;

        let new_revision = existing.revision + 1;

        self.db
            .upsert_context_epoch(
                session_id,
                baseline,
                &existing.agent,
                &snapshot_str,
                existing.baseline_seq,
                existing.replacement_seq,
                new_revision,
            )
            .await?;

        Ok(ContextEpoch {
            session_id: session_id.to_string(),
            baseline: baseline.to_string(),
            agent: existing.agent,
            snapshot: snapshot.clone(),
            baseline_seq: existing.baseline_seq as u64,
            replacement_seq: existing.replacement_seq.map(|s| s as u64),
            revision: new_revision as u64,
        })
    }

    /// Request a replacement for the current epoch — marks it for replacement.
    ///
    /// This is called when a new context window needs to be established
    /// (e.g., after the first prompt in a new session, before the response).
    ///
    /// # Source
    /// Ported from `packages/core/src/session/history.ts` — `requestReplacement`.
    pub async fn request_replacement(
        &self,
        session_id: &str,
    ) -> Result<ContextEpoch, EpochError> {
        // Get existing epoch
        let existing = self
            .db
            .get_context_epoch(session_id)
            .await?
            .ok_or_else(|| EpochError::NotInitialized(session_id.to_string()))?;

        let new_revision = existing.revision + 1;

        // Set replacement_seq to the current baseline_seq (indicating that
        // a new context window is needed after this point)
        let replacement_seq = Some(existing.baseline_seq);

        self.db
            .upsert_context_epoch(
                session_id,
                &existing.baseline,
                &existing.agent,
                &existing.snapshot,
                existing.baseline_seq,
                replacement_seq,
                new_revision,
            )
            .await?;

        Ok(ContextEpoch {
            session_id: session_id.to_string(),
            baseline: existing.baseline,
            agent: existing.agent,
            snapshot: serde_json::from_str(&existing.snapshot)
                .unwrap_or(serde_json::Value::Null),
            baseline_seq: existing.baseline_seq as u64,
            replacement_seq: replacement_seq.map(|s| s as u64),
            revision: new_revision as u64,
        })
    }

    /// Get the current epoch for a session.
    pub async fn get_epoch(
        &self,
        session_id: &str,
    ) -> Result<Option<ContextEpoch>, EpochError> {
        let row = self.db.get_context_epoch(session_id).await?;
        match row {
            Some(r) => {
                let snapshot: serde_json::Value = serde_json::from_str(&r.snapshot)
                    .unwrap_or(serde_json::Value::Null);

                Ok(Some(ContextEpoch {
                    session_id: r.session_id,
                    baseline: r.baseline,
                    agent: r.agent,
                    snapshot,
                    baseline_seq: r.baseline_seq as u64,
                    replacement_seq: r.replacement_seq.map(|s| s as u64),
                    revision: r.revision as u64,
                }))
            }
            None => Ok(None),
        }
    }

    /// Delete the epoch for a session.
    pub async fn delete_epoch(&self, session_id: &str) -> Result<(), EpochError> {
        self.db.delete_context_epoch(session_id).await?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    async fn create_test_db() -> Arc<DatabaseService> {
        let pool = sqlx::SqlitePool::connect(":memory:")
            .await
            .expect("create pool");

        // Create session table
        sqlx::query(
            "CREATE TABLE IF NOT EXISTS session (
                id text PRIMARY KEY,
                project_id text NOT NULL,
                slug text NOT NULL,
                directory text NOT NULL,
                title text NOT NULL,
                version text NOT NULL,
                cost real DEFAULT 0 NOT NULL,
                tokens_input integer DEFAULT 0 NOT NULL,
                tokens_output integer DEFAULT 0 NOT NULL,
                tokens_reasoning integer DEFAULT 0 NOT NULL,
                tokens_cache_read integer DEFAULT 0 NOT NULL,
                tokens_cache_write integer DEFAULT 0 NOT NULL,
                time_created integer NOT NULL,
                time_updated integer NOT NULL
            )",
        )
        .execute(&pool)
        .await
        .unwrap();

        // Create session_context_epoch table
        sqlx::query(
            "CREATE TABLE IF NOT EXISTS session_context_epoch (
                session_id text PRIMARY KEY,
                baseline text NOT NULL,
                agent text DEFAULT 'build' NOT NULL,
                snapshot text NOT NULL,
                baseline_seq integer NOT NULL,
                replacement_seq integer,
                revision integer DEFAULT 0 NOT NULL,
                CONSTRAINT fk_session_context_epoch_session_id_session_id_fk FOREIGN KEY (session_id) REFERENCES session(id) ON DELETE CASCADE
            )",
        )
        .execute(&pool)
        .await
        .unwrap();

        // Insert a test session
        let now = chrono::Utc::now().timestamp_millis();
        sqlx::query(
            "INSERT INTO session (id, project_id, slug, directory, title, version, time_created, time_updated) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)",
        )
        .bind("ses_001")
        .bind("proj_001")
        .bind("test-session")
        .bind("/tmp")
        .bind("Test Session")
        .bind("1.0.0")
        .bind(now)
        .bind(now)
        .execute(&pool)
        .await
        .unwrap();

        Arc::new(DatabaseService::new(pool))
    }

    #[tokio::test]
    async fn test_initialize_epoch() {
        let db = create_test_db().await;
        let mgr = EpochManager::new(db);

        let epoch = mgr
            .initialize_epoch("ses_001", "Baseline summary", "build", &json!({"files": []}), 10)
            .await
            .unwrap();

        assert_eq!(epoch.session_id, "ses_001");
        assert_eq!(epoch.baseline, "Baseline summary");
        assert_eq!(epoch.revision, 1);
        assert_eq!(epoch.baseline_seq, 10);
    }

    #[tokio::test]
    async fn test_prepare_epoch() {
        let db = create_test_db().await;
        let mgr = EpochManager::new(db);

        mgr.initialize_epoch("ses_001", "Initial baseline", "build", &json!({"files": []}), 5)
            .await
            .unwrap();

        let epoch = mgr
            .prepare_epoch("ses_001", "Updated baseline", &json!({"files": ["src/main.rs"]}))
            .await
            .unwrap();

        assert_eq!(epoch.baseline, "Updated baseline");
        assert_eq!(epoch.revision, 2);
    }

    #[tokio::test]
    async fn test_request_replacement() {
        let db = create_test_db().await;
        let mgr = EpochManager::new(db);

        mgr.initialize_epoch("ses_001", "Initial", "build", &json!({}), 5)
            .await
            .unwrap();

        let epoch = mgr.request_replacement("ses_001").await.unwrap();

        assert!(epoch.replacement_seq.is_some());
        assert_eq!(epoch.replacement_seq, Some(5));
        assert_eq!(epoch.revision, 2);
    }

    #[tokio::test]
    async fn test_get_epoch_not_found() {
        let db = create_test_db().await;
        let mgr = EpochManager::new(db);

        let epoch = mgr.get_epoch("ses_002").await.unwrap();
        assert!(epoch.is_none());
    }

    #[tokio::test]
    async fn test_delete_epoch() {
        let db = create_test_db().await;
        let mgr = EpochManager::new(db);

        mgr.initialize_epoch("ses_001", "Test", "build", &json!({}), 1)
            .await
            .unwrap();

        mgr.delete_epoch("ses_001").await.unwrap();

        let epoch = mgr.get_epoch("ses_001").await.unwrap();
        assert!(epoch.is_none());
    }
}
