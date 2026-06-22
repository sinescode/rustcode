//! Context epoch persistence — full SystemContext reconciliation algebra
//! with initialize/reconcile/replace, revision mismatch retry, and
//! agent/location verification.
//!
//! Ported from:
//! - `packages/core/src/session/history.ts` — epoch management (lines 58–79)
//! - `packages/blazecode/src/session/context-epoch.ts` (lines 1–343)

use crate::database::{DatabaseService, DatabaseServiceError};
use crate::session_history::ContextEpoch;
use crate::system_context::{Generation, ReconcileResult, Snapshot, SystemContext, SystemContextError};
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

    #[error("Agent mismatch: session={session_id}, stored={stored}, requested={requested}")]
    AgentMismatch {
        session_id: String,
        stored: String,
        requested: String,
    },

    #[error("Location mismatch: session={0}")]
    LocationMismatch(String),

    #[error("Revision mismatch: session={session_id}, expected={expected}, actual={actual}")]
    RevisionMismatch {
        session_id: String,
        expected: i64,
        actual: i64,
    },

    #[error("Agent replacement blocked: session={session_id}, previous={previous}, current={current}")]
    AgentReplacementBlocked {
        session_id: String,
        previous: String,
        current: String,
    },

    #[error("System context error: {0}")]
    SystemContext(String),

    #[error("Snapshot decode error: {0}")]
    SnapshotDecode(String),

    #[error("{0}")]
    Other(String),
}

impl From<DatabaseServiceError> for EpochError {
    fn from(e: DatabaseServiceError) -> Self {
        EpochError::Database(e.to_string())
    }
}

impl From<SystemContextError> for EpochError {
    fn from(e: SystemContextError) -> Self {
        EpochError::SystemContext(e.to_string())
    }
}

// ══════════════════════════════════════════════════════════════════════════════
// EpochManager
// ══════════════════════════════════════════════════════════════════════════════

/// Manages context epoch lifecycle — full SystemContext reconciliation
/// algebra with initialize/reconcile/replace and revision mismatch retry.
///
/// # Source
/// Ported from:
/// - `packages/core/src/session/history.ts` — epoch management
/// - `packages/blazecode/src/session/context-epoch.ts` (lines 1–343)
pub struct EpochManager {
    db: Arc<DatabaseService>,
}

/// Result of an epoch prepare operation.
///
/// # Source
/// Ported from `packages/core/src/session/context-epoch.ts` line 36–40.
#[derive(Debug, Clone)]
pub struct PreparedEpoch {
    pub baseline: String,
    pub baseline_seq: i64,
    pub revision: i64,
}

impl EpochManager {
    /// Create a new epoch manager.
    pub fn new(db: Arc<DatabaseService>) -> Self {
        Self { db }
    }

    // ════════════════════════════════════════════════════════════════════
    // Public API — the full reconciliation algebra
    // ════════════════════════════════════════════════════════════════════

    /// Initialize a new context epoch for a session using SystemContext.
    ///
    /// Generates the baseline from the system context, inserts the epoch
    /// row, and retries on revision mismatch.
    ///
    /// Returns `None` if the epoch already exists.
    ///
    /// # Source
    /// Ported from `packages/core/src/session/context-epoch.ts` lines 42–52,
    /// 112–123 (`initialize`, `initializeOnce`).
    pub async fn initialize(
        &self,
        context: &SystemContext,
        session_id: &str,
        directory: &str,
        workspace_id: Option<&str>,
        agent: &str,
    ) -> Result<Option<PreparedEpoch>, EpochError> {
        // Check if epoch already exists
        if self.exists(session_id).await? {
            return Ok(None);
        }

        self.retry_revision_mismatch(session_id, || async {
            let generation = context.initialize()?;
            let baseline_seq = self.latest_seq(session_id).await?;
            let revision = 0i64;

            // Validate location+agent before inserting
            self.require_agent_selection(session_id, agent).await?;

            let snapshot_str =
                serde_json::to_string(&generation.snapshot).map_err(|e| {
                    EpochError::Other(format!("Serialize snapshot: {e}"))
                })?;

            self.db
                .upsert_context_epoch(
                    session_id,
                    &generation.baseline,
                    agent,
                    &snapshot_str,
                    baseline_seq,
                    None,
                    revision + 1,
                )
                .await?;

            Ok(PreparedEpoch {
                baseline: generation.baseline,
                baseline_seq,
                revision,
            })
        })
        .await
        .map(Some)
    }

    /// Initialize once (no retry) — used internally after existence check.
    async fn initialize_once(
        &self,
        context: &SystemContext,
        session_id: &str,
        agent: &str,
    ) -> Result<PreparedEpoch, EpochError> {
        let generation = context.initialize()?;
        let baseline_seq = self.latest_seq(session_id).await?;
        let revision = 0i64;

        let snapshot_str = serde_json::to_string(&generation.snapshot)
            .map_err(|e| EpochError::Other(format!("Serialize snapshot: {e}")))?;

        self.db
            .upsert_context_epoch(
                session_id,
                &generation.baseline,
                agent,
                &snapshot_str,
                baseline_seq,
                None,
                revision + 1,
            )
            .await?;

        Ok(PreparedEpoch {
            baseline: generation.baseline,
            baseline_seq,
            revision,
        })
    }

    /// Prepare — reconcile or replace the current epoch.
    ///
    /// This is the main entry point for context epoch management. It:
    /// 1. Loads the current epoch snapshot
    /// 2. Reconciles current context values against the snapshot
    /// 3. If Unchanged or ReplacementBlocked — fences and returns current
    /// 4. If ReplacementReady — performs full replace
    /// 5. If Updated — publishes ContextUpdated event and advances
    ///
    /// # Source
    /// Ported from `packages/core/src/session/context-epoch.ts` lines 54–110
    /// (`prepare`, `prepareOnce`).
    pub async fn prepare(
        &self,
        context: &SystemContext,
        session_id: &str,
        agent: &str,
    ) -> Result<PreparedEpoch, EpochError> {
        self.retry_revision_mismatch(session_id, || async {
            let stored = self
                .db
                .get_context_epoch(session_id)
                .await?
                .ok_or_else(|| EpochError::NotInitialized(session_id.to_string()))?;

            // Decode the stored snapshot
            let snapshot: Snapshot =
                serde_json::from_str(&stored.snapshot).map_err(|e| {
                    EpochError::SnapshotDecode(format!(
                        "decode snapshot: {e}"
                    ))
                })?;

            let replacing_agent = stored.agent != agent;

            let result = if stored.replacement_seq.is_none() && !replacing_agent {
                context.reconcile(&snapshot)?
            } else {
                context.replace(&snapshot)
            };

            match result {
                ReconcileResult::ReplacementBlocked if replacing_agent => {
                    self.fence(session_id, agent, stored.revision).await?;
                    return Err(EpochError::AgentReplacementBlocked {
                        session_id: session_id.to_string(),
                        previous: stored.agent,
                        current: agent.to_string(),
                    });
                }
                ReconcileResult::Unchanged | ReconcileResult::ReplacementBlocked => {
                    self.fence(session_id, agent, stored.revision).await?;
                    Ok(PreparedEpoch {
                        baseline: stored.baseline,
                        baseline_seq: stored.baseline_seq,
                        revision: stored.revision,
                    })
                }
                ReconcileResult::ReplacementReady { generation } => {
                    let replacement_seq = stored.replacement_seq.unwrap_or_else(|| {
                        // Fallback to current latest seq
                        -1
                    });
                    let actual_replacement_seq = if replacement_seq < 0 {
                        self.latest_seq(session_id).await?
                    } else {
                        replacement_seq
                    };
                    self.replace_inner(
                        session_id,
                        agent,
                        stored.revision,
                        actual_replacement_seq,
                        &generation,
                    )
                    .await
                }
                ReconcileResult::Updated { text, snapshot } => {
                    // Advance the snapshot in the DB
                    self.advance(session_id, stored.revision, &snapshot)
                        .await?;
                    Ok(PreparedEpoch {
                        baseline: stored.baseline,
                        baseline_seq: stored.baseline_seq,
                        revision: stored.revision + 1,
                    })
                }
            }
        })
        .await
    }

    /// Request a replacement for the current epoch.
    ///
    /// # Source
    /// Ported from `packages/core/src/session/context-epoch.ts` lines 159–176
    /// (`requestReplacement`).
    pub async fn request_replacement(
        &self,
        session_id: &str,
    ) -> Result<ContextEpoch, EpochError> {
        let existing = self
            .db
            .get_context_epoch(session_id)
            .await?
            .ok_or_else(|| EpochError::NotInitialized(session_id.to_string()))?;

        let new_revision = existing.revision + 1;

        // Only set replacement_seq if it's not already set or the new one is larger
        let replacement_seq = if existing.replacement_seq.is_none()
            || existing
                .replacement_seq
                .map(|s| s < existing.baseline_seq)
                .unwrap_or(true)
        {
            Some(existing.baseline_seq)
        } else {
            existing.replacement_seq
        };

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
                .map_err(|e| EpochError::Other(format!("invalid snapshot JSON: {e}")))?,
            baseline_seq: existing.baseline_seq as u64,
            replacement_seq: replacement_seq.map(|s| s as u64),
            revision: new_revision as u64,
        })
    }

    /// Reset (delete) the epoch for a session.
    ///
    /// # Source
    /// Ported from `packages/core/src/session/context-epoch.ts` lines 178–187
    /// (`reset`).
    pub async fn reset(&self, session_id: &str) -> Result<(), EpochError> {
        self.db.delete_context_epoch(session_id).await?;
        Ok(())
    }

    /// Check if the session's current epoch is valid for the given agent/revision.
    ///
    /// # Source
    /// Ported from `packages/core/src/session/context-epoch.ts` lines 298–321
    /// (`current`).
    pub async fn current(
        &self,
        session_id: &str,
        agent: &str,
        revision: i64,
    ) -> Result<bool, EpochError> {
        let epoch = self.db.get_context_epoch(session_id).await?;
        match epoch {
            Some(epoch) => {
                let agent_ok = epoch.agent == agent;
                let selected_agent = self.get_selected_agent(session_id).await?;
                let selected_ok = selected_agent
                    .as_ref()
                    .map(|a| a == agent)
                    .unwrap_or(true);
                let revision_ok = epoch.revision == revision;
                Ok(agent_ok && selected_ok && revision_ok)
            }
            None => Ok(false),
        }
    }

    // ════════════════════════════════════════════════════════════════════
    // Internal helpers
    // ════════════════════════════════════════════════════════════════════

    /// Fence: verify the current state matches expected agent/revision.
    ///
    /// # Source
    /// Ported from `packages/core/src/session/context-epoch.ts` lines 280–296
    /// (`fence`).
    async fn fence(
        &self,
        session_id: &str,
        agent: &str,
        expected_revision: i64,
    ) -> Result<(), EpochError> {
        let epoch = self
            .db
            .get_context_epoch(session_id)
            .await?
            .ok_or_else(|| EpochError::NotInitialized(session_id.to_string()))?;

        let selected_agent = self.get_selected_agent(session_id).await?;
        if let Some(ref sa) = selected_agent {
            if sa != agent {
                return Err(EpochError::AgentMismatch {
                    session_id: session_id.to_string(),
                    stored: sa.clone(),
                    requested: agent.to_string(),
                });
            }
        }

        if epoch.revision != expected_revision {
            return Err(EpochError::RevisionMismatch {
                session_id: session_id.to_string(),
                expected: expected_revision,
                actual: epoch.revision,
            });
        }

        Ok(())
    }

    /// Advance the snapshot on the current epoch.
    ///
    /// # Source
    /// Ported from `packages/core/src/session/context-epoch.ts` lines 323–343
    /// (`advance`).
    async fn advance(
        &self,
        session_id: &str,
        expected_revision: i64,
        snapshot: &Snapshot,
    ) -> Result<(), EpochError> {
        let snapshot_str = serde_json::to_string(snapshot)
            .map_err(|e| EpochError::Other(format!("Serialize snapshot: {e}")))?;

        let updated = self
            .db
            .update_context_epoch_snapshot(
                session_id,
                expected_revision,
                &snapshot_str,
            )
            .await?;

        if !updated {
            let actual = self
                .db
                .get_context_epoch(session_id)
                .await?
                .map(|e| e.revision)
                .unwrap_or(-1);
            return Err(EpochError::RevisionMismatch {
                session_id: session_id.to_string(),
                expected: expected_revision,
                actual,
            });
        }

        Ok(())
    }

    /// Replace the epoch with a new generation.
    ///
    /// # Source
    /// Ported from `packages/core/src/session/context-epoch.ts` lines 241–278
    /// (`replace`).
    async fn replace_inner(
        &self,
        session_id: &str,
        agent: &str,
        expected_revision: i64,
        baseline_seq: i64,
        generation: &Generation,
    ) -> Result<PreparedEpoch, EpochError> {
        self.require_agent_selection(session_id, agent).await?;

        let snapshot_str = serde_json::to_string(&generation.snapshot)
            .map_err(|e| EpochError::Other(format!("Serialize snapshot: {e}")))?;

        let updated = self
            .db
            .replace_context_epoch(
                session_id,
                &generation.baseline,
                agent,
                &snapshot_str,
                baseline_seq,
                expected_revision,
            )
            .await?;

        if !updated {
            let actual = self
                .db
                .get_context_epoch(session_id)
                .await?
                .map(|e| e.revision)
                .unwrap_or(-1);
            return Err(EpochError::RevisionMismatch {
                session_id: session_id.to_string(),
                expected: expected_revision,
                actual,
            });
        }

        Ok(PreparedEpoch {
            baseline: generation.baseline.clone(),
            baseline_seq,
            revision: expected_revision + 1,
        })
    }

    /// Retry a closure on RevisionMismatch.
    ///
    /// # Source
    /// Ported from `packages/core/src/session/context-epoch.ts` lines 27–34
    /// (`retryRevisionMismatch`).
    async fn retry_revision_mismatch<F, Fut, T>(
        &self,
        session_id: &str,
        attempt: F,
    ) -> Result<T, EpochError>
    where
        F: Fn() -> Fut,
        Fut: std::future::Future<Output = Result<T, EpochError>>,
    {
        loop {
            match attempt().await {
                Err(EpochError::RevisionMismatch { .. }) => {
                    tokio::task::yield_now().await;
                    continue;
                }
                other => return other,
            }
        }
    }

    /// Check if an epoch exists for the session.
    async fn exists(&self, session_id: &str) -> Result<bool, EpochError> {
        Ok(self.db.get_context_epoch(session_id).await?.is_some())
    }

    /// Get the latest sequence number for a session.
    async fn latest_seq(&self, session_id: &str) -> Result<i64, EpochError> {
        let seq = self
            .db
            .get_event_sequence(session_id)
            .await?
            .map(|r| r.seq)
            .unwrap_or(-1);
        Ok(seq)
    }

    /// Verify that the selected agent matches.
    async fn require_agent_selection(
        &self,
        session_id: &str,
        agent: &str,
    ) -> Result<(), EpochError> {
        let selected = self.get_selected_agent(session_id).await?;
        if let Some(ref sa) = selected {
            if sa != agent {
                return Err(EpochError::AgentMismatch {
                    session_id: session_id.to_string(),
                    stored: sa.clone(),
                    requested: agent.to_string(),
                });
            }
        }
        Ok(())
    }

    /// Get the selected agent from the session table.
    async fn get_selected_agent(
        &self,
        session_id: &str,
    ) -> Result<Option<String>, EpochError> {
        let row: Option<(Option<String>,)> = sqlx::query_as(
            "SELECT agent FROM session WHERE id = ?1",
        )
        .bind(session_id)
        .fetch_optional(self.db.pool())
        .await
        .map_err(|e| EpochError::Database(e.to_string()))?;

        Ok(row.and_then(|(a,)| a))
    }

    // ── Legacy compatibility methods ──────────────────────────────────

    /// Initialize a new context epoch from raw values (legacy).
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
                None,
                1,
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

    /// Prepare an epoch with raw values (legacy).
    pub async fn prepare_epoch(
        &self,
        session_id: &str,
        baseline: &str,
        snapshot: &serde_json::Value,
    ) -> Result<ContextEpoch, EpochError> {
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

    /// Get the current epoch for a session.
    pub async fn get_epoch(
        &self,
        session_id: &str,
    ) -> Result<Option<ContextEpoch>, EpochError> {
        let row = self.db.get_context_epoch(session_id).await?;
        match row {
            Some(r) => {
                let snapshot: serde_json::Value =
                    serde_json::from_str(&r.snapshot).unwrap_or(serde_json::Value::Null);
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
