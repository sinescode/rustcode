//! Session input inbox — admit/promote lifecycle for session inputs.
//!
//! Implements the durable prompt inbox used in BlazeCode:
//! - `admit_input` — queue an input for processing
//! - `promote_steers` — promote all steer-mode inputs up to a cutoff
//! - `legacy_prompted` — handle legacy prompt format
//!
//! Ported from:
//! - `packages/core/src/session/input.ts` (lines 1–354)

use crate::database::{DatabaseService, DatabaseServiceError, SessionInputRow};
use crate::event::{
    EventDefinition, EventError, EventV2, SyncConfig,
};
use crate::session_history::{
    AdmitInputParams, AdmittedInput, InputDelivery, LegacyPromptedParams,
    PromoteSteersParams,
};
use crate::session_message::Prompt;
use std::sync::Arc;

/// Error type for session input operations.
#[derive(Debug, thiserror::Error)]
pub enum InputInboxError {
    #[error("Lifecycle conflict: {0}")]
    LifecycleConflict(String),

    #[error("Database error: {0}")]
    Database(String),

    #[error("Session not found: {0}")]
    NotFound(String),

    #[error("{0}")]
    Other(String),
}

impl From<DatabaseServiceError> for InputInboxError {
    fn from(e: DatabaseServiceError) -> Self {
        InputInboxError::Database(e.to_string())
    }
}

impl From<EventError> for InputInboxError {
    fn from(e: EventError) -> Self {
        InputInboxError::Other(e.to_string())
    }
}

// ══════════════════════════════════════════════════════════════════════════════
// SessionInputInbox
// ══════════════════════════════════════════════════════════════════════════════

/// Manages the session input inbox — admitting, promoting, and querying inputs.
///
/// # Source
/// Ported from `packages/core/src/session/input.ts` (lines 1–354).
pub struct SessionInputInbox {
    db: Arc<DatabaseService>,
}

impl SessionInputInbox {
    /// Create a new inbox service.
    pub fn new(db: Arc<DatabaseService>) -> Self {
        Self { db }
    }

    /// Admit a new input into the session input inbox.
    ///
    /// Returns the created `AdmittedInput`.
    ///
    /// # Source
    /// Ported from `packages/core/src/session/input.ts` lines 54–93 `admit`.
    pub async fn admit_input(
        &self,
        params: AdmitInputParams,
    ) -> Result<AdmittedInput, InputInboxError> {
        let now = chrono::Utc::now().timestamp_millis() as u64;

        // Get the next admitted sequence number
        let admitted_seq = self
            .db
            .get_next_admitted_seq(&params.session_id)
            .await
            .map_err(|e| InputInboxError::Database(e.to_string()))?;

        // Serialize the prompt
        let prompt_str = serde_json::to_string(&params.prompt)
            .map_err(|e| InputInboxError::Other(format!("Failed to serialize prompt: {e}")))?;

        // Insert the input record
        self.db
            .insert_session_input(
                &params.id,
                &params.session_id,
                &prompt_str,
                if params.delivery == InputDelivery::Steer {
                    "steer"
                } else {
                    "queue"
                },
                admitted_seq,
                now as i64,
            )
            .await?;

        Ok(AdmittedInput {
            admitted_seq: admitted_seq as u64,
            id: params.id,
            session_id: params.session_id,
            prompt: params.prompt,
            delivery: params.delivery,
            time_created: now,
            promoted_seq: None,
        })
    }

    /// Promote steer-mode inputs up to a given cutoff sequence.
    ///
    /// Returns the list of promoted inputs.
    ///
    /// # Source
    /// Ported from `packages/core/src/session/input.ts` lines 300–321 `promoteSteers`.
    pub async fn promote_steers(
        &self,
        params: PromoteSteersParams,
    ) -> Result<Vec<AdmittedInput>, InputInboxError> {
        // Get all pending inputs
        let pending = self.db.list_pending_inputs(&params.session_id).await?;

        // Filter to steer mode within cutoff
        let steer_pending: Vec<SessionInputRow> = pending
            .into_iter()
            .filter(|r| {
                r.delivery == "steer" && (r.admitted_seq as u64) <= params.cutoff
            })
            .collect();

        let mut promoted = Vec::new();

        for row in steer_pending {
            // Promote the input
            let promoted_seq = row.admitted_seq; // Use admitted_seq as promoted_seq
            self.db
                .promote_input(&row.id, promoted_seq)
                .await?;

            // Deserialize the prompt
            let prompt: Prompt = serde_json::from_str(&row.prompt)
                .map_err(|e| InputInboxError::Other(format!("Deserialize prompt: {e}")))?;

            promoted.push(AdmittedInput {
                admitted_seq: row.admitted_seq as u64,
                id: row.id,
                session_id: row.session_id,
                prompt,
                delivery: if row.delivery == "steer" {
                    InputDelivery::Steer
                } else {
                    InputDelivery::Queue
                },
                time_created: row.time_created as u64,
                promoted_seq: Some(promoted_seq as u64),
            });
        }

        Ok(promoted)
    }

    /// Promote the next queued input (FIFO) for a session.
    ///
    /// Returns the promoted input, or `None` if no queued inputs are pending.
    ///
    /// # Source
    /// Ported from `packages/core/src/session/input.ts` — `promoteNextQueued`.
    pub async fn promote_next_queued(
        &self,
        session_id: &str,
    ) -> Result<Option<AdmittedInput>, InputInboxError> {
        // Get all pending inputs
        let pending = self.db.list_pending_inputs(session_id).await?;

        // Find the first queued input by admitted_seq
        let queued = pending
            .into_iter()
            .filter(|r| r.delivery == "queue")
            .min_by_key(|r| r.admitted_seq);

        match queued {
            Some(row) => {
                let promoted_seq = row.admitted_seq;
                self.db.promote_input(&row.id, promoted_seq).await?;

                let prompt: Prompt = serde_json::from_str(&row.prompt)
                    .map_err(|e| InputInboxError::Other(format!("Deserialize prompt: {e}")))?;

                Ok(Some(AdmittedInput {
                    admitted_seq: row.admitted_seq as u64,
                    id: row.id,
                    session_id: row.session_id,
                    prompt,
                    delivery: InputDelivery::Queue,
                    time_created: row.time_created as u64,
                    promoted_seq: Some(promoted_seq as u64),
                }))
            }
            None => Ok(None),
        }
    }

    /// Handle a legacy prompted input — project it as already promoted.
    ///
    /// # Source
    /// Ported from `packages/core/src/session/input.ts` lines 242–270 `projectLegacyPrompted`.
    pub async fn legacy_prompted(
        &self,
        params: LegacyPromptedParams,
    ) -> Result<AdmittedInput, InputInboxError> {
        // Check if input already exists
        let existing = self.db.list_session_inputs(&params.session_id).await?;
        if existing.iter().any(|r| r.id == params.id) {
            return Err(InputInboxError::LifecycleConflict(format!(
                "Input already exists: {}",
                params.id
            )));
        }

        let admitted_seq = params.promoted_seq; // Legacy: admitted_seq = promoted_seq

        let prompt_str = serde_json::to_string(&params.prompt)
            .map_err(|e| InputInboxError::Other(format!("Serialize prompt: {e}")))?;

        // Insert as already promoted
        self.db
            .insert_session_input(
                &params.id,
                &params.session_id,
                &prompt_str,
                if params.delivery == InputDelivery::Steer {
                    "steer"
                } else {
                    "queue"
                },
                admitted_seq as i64,
                params.time_created as i64,
            )
            .await?;

        // Mark as promoted
        self.db
            .promote_input(&params.id, admitted_seq as i64)
            .await?;

        Ok(AdmittedInput {
            admitted_seq,
            id: params.id,
            session_id: params.session_id,
            prompt: params.prompt,
            delivery: params.delivery,
            time_created: params.time_created,
            promoted_seq: Some(params.promoted_seq),
        })
    }

    /// EventV2-driven admit — publishes a PromptLifecycle.Admitted event
    /// and relies on the projector to persist the session_input row.
    ///
    /// Conflict detection: if an input with the same ID already exists,
    /// returns the existing `AdmittedInput` instead of creating a new one.
    ///
    /// # Source
    /// Ported from `packages/core/src/session/input.ts` lines 54–93 (`admit`).
    pub async fn admit_through_events(
        &self,
        events: &EventV2,
        params: AdmitInputParams,
    ) -> Result<AdmittedInput, InputInboxError> {
        // Check for existing input with this ID
        let existing = self.find_input(&params.id).await?;
        if let Some(existing) = existing {
            return Ok(existing);
        }

        let timestamp = chrono::Utc::now().timestamp_millis() as u64;

        // Define the admitted event
        let definition = EventDefinition::new(
            crate::event::session_event_types::PROMPT_ADMITTED,
            Some(SyncConfig {
                version: 1,
                aggregate: "sessionID".to_string(),
            }),
            serde_json::json!({
                "type": "object",
                "properties": {
                    "messageID": {"type": "string"},
                    "sessionID": {"type": "string"},
                    "timestamp": {"type": "number"},
                    "prompt": {"type": "object"},
                    "delivery": {"type": "string"}
                }
            }),
        );

        let data = serde_json::json!({
            "messageID": params.id,
            "sessionID": params.session_id,
            "timestamp": timestamp,
            "prompt": params.prompt,
            "delivery": match params.delivery {
                InputDelivery::Steer => "steer",
                InputDelivery::Queue => "queue",
            },
        });

        // Publish the event — the projector will persist the session_input row
        let event_result = events.publish(&definition, data, None).await.map_err(|e| {
            InputInboxError::Other(format!("publish admitted event: {e}"))
        })?;

        let admitted_seq = event_result.seq.unwrap_or(0);

        Ok(AdmittedInput {
            admitted_seq,
            id: params.id,
            session_id: params.session_id,
            prompt: params.prompt,
            delivery: params.delivery,
            time_created: timestamp,
            promoted_seq: None,
        })
    }

    /// EventV2-driven promote for steers — publishes PromptLifecycle.Promoted
    /// events for all steer-mode inputs up to the cutoff.
    ///
    /// Returns the number of inputs promoted.
    ///
    /// # Source
    /// Ported from `packages/core/src/session/input.ts` lines 300–321 (`promoteSteers`).
    pub async fn promote_steers_through_events(
        &self,
        events: &EventV2,
        params: PromoteSteersParams,
    ) -> Result<usize, InputInboxError> {
        let rows = self.db.list_pending_inputs(&params.session_id).await?;
        let steer_pending: Vec<SessionInputRow> = rows
            .into_iter()
            .filter(|r| {
                r.delivery == "steer" && (r.admitted_seq as u64) <= params.cutoff
            })
            .collect();

        let definition = EventDefinition::new(
            crate::event::session_event_types::PROMPT_PROMOTED,
            Some(SyncConfig {
                version: 1,
                aggregate: "sessionID".to_string(),
            }),
            serde_json::json!({
                "type": "object",
                "properties": {
                    "messageID": {"type": "string"},
                    "sessionID": {"type": "string"},
                    "timestamp": {"type": "number"},
                    "prompt": {"type": "object"},
                    "timeCreated": {"type": "number"}
                }
            }),
        );

        let mut count = 0usize;
        for row in &steer_pending {
            let prompt: Prompt = serde_json::from_str(&row.prompt)
                .map_err(|e| InputInboxError::Other(format!("deserialize prompt: {e}")))?;
            let timestamp = chrono::Utc::now().timestamp_millis();

            let data = serde_json::json!({
                "messageID": row.id,
                "sessionID": row.session_id,
                "timestamp": timestamp,
                "prompt": prompt,
                "timeCreated": row.time_created,
            });

            match events.publish(&definition, data, None).await {
                Ok(_) => count += 1,
                Err(e) => {
                    tracing::warn!(
                        "Failed to publish promote event for input {}: {e}",
                        row.id
                    );
                }
            }
        }

        Ok(count)
    }

    /// EventV2-driven promote for next queued input.
    ///
    /// # Source
    /// Ported from `packages/core/src/session/input.ts` lines 323–343 (`promoteNextQueued`).
    pub async fn promote_next_queued_through_events(
        &self,
        events: &EventV2,
        session_id: &str,
    ) -> Result<bool, InputInboxError> {
        let rows = self.db.list_pending_inputs(session_id).await?;
        let queued: Vec<SessionInputRow> = rows
            .into_iter()
            .filter(|r| r.delivery == "queue")
            .collect();

        let Some(row) = queued.into_iter().min_by_key(|r| r.admitted_seq) else {
            return Ok(false);
        };

        let prompt: Prompt = serde_json::from_str(&row.prompt)
            .map_err(|e| InputInboxError::Other(format!("deserialize prompt: {e}")))?;
        let timestamp = chrono::Utc::now().timestamp_millis();

        let definition = EventDefinition::new(
            crate::event::session_event_types::PROMPT_PROMOTED,
            Some(SyncConfig {
                version: 1,
                aggregate: "sessionID".to_string(),
            }),
            serde_json::json!({}),
        );

        let data = serde_json::json!({
            "messageID": row.id,
            "sessionID": row.session_id,
            "timestamp": timestamp,
            "prompt": prompt,
            "timeCreated": row.time_created,
        });

        events.publish(&definition, data, None).await.map_err(|e| {
            InputInboxError::Other(format!("publish promote event: {e}"))
        })?;

        Ok(true)
    }

    /// Find an admitted input by ID.
    pub async fn find_input(
        &self,
        id: &str,
    ) -> Result<Option<AdmittedInput>, InputInboxError> {
        let row = self.db.find_session_input(id).await?;
        match row {
            Some(r) => {
                let prompt: Prompt = serde_json::from_str(&r.prompt)
                    .map_err(|e| InputInboxError::Other(format!("deserialize prompt: {e}")))?;
                Ok(Some(AdmittedInput {
                    admitted_seq: r.admitted_seq as u64,
                    id: r.id,
                    session_id: r.session_id,
                    prompt,
                    delivery: if r.delivery == "steer" {
                        InputDelivery::Steer
                    } else {
                        InputDelivery::Queue
                    },
                    time_created: r.time_created as u64,
                    promoted_seq: r.promoted_seq.map(|s| s as u64),
                }))
            }
            None => Ok(None),
        }
    }

    /// List all admitted inputs for a session.
    ///
    /// # Source
    /// Ported from `packages/core/src/session/input.ts` — query helper.
    pub async fn list_inputs(
        &self,
        session_id: &str,
    ) -> Result<Vec<AdmittedInput>, InputInboxError> {
        let rows = self.db.list_session_inputs(session_id).await?;
        let mut inputs = Vec::with_capacity(rows.len());

        for row in rows {
            let prompt: Prompt = serde_json::from_str(&row.prompt)
                .map_err(|e| InputInboxError::Other(format!("Deserialize prompt: {e}")))?;

            inputs.push(AdmittedInput {
                admitted_seq: row.admitted_seq as u64,
                id: row.id,
                session_id: row.session_id,
                prompt,
                delivery: if row.delivery == "steer" {
                    InputDelivery::Steer
                } else {
                    InputDelivery::Queue
                },
                time_created: row.time_created as u64,
                promoted_seq: row.promoted_seq.map(|s| s as u64),
            });
        }

        Ok(inputs)
    }

    /// Get pending (non-promoted) inputs for a session.
    ///
    /// # Source
    /// Ported from `packages/core/src/session/input.ts` — query helper.
    pub async fn pending_inputs(
        &self,
        session_id: &str,
    ) -> Result<Vec<AdmittedInput>, InputInboxError> {
        let rows = self.db.list_pending_inputs(session_id).await?;
        let mut inputs = Vec::with_capacity(rows.len());

        for row in rows {
            let prompt: Prompt = serde_json::from_str(&row.prompt)
                .map_err(|e| InputInboxError::Other(format!("Deserialize prompt: {e}")))?;

            inputs.push(AdmittedInput {
                admitted_seq: row.admitted_seq as u64,
                id: row.id,
                session_id: row.session_id,
                prompt,
                delivery: if row.delivery == "steer" {
                    InputDelivery::Steer
                } else {
                    InputDelivery::Queue
                },
                time_created: row.time_created as u64,
                promoted_seq: None,
            });
        }

        Ok(inputs)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::session_message::Prompt;

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

        // Create session_input table
        sqlx::query(
            "CREATE TABLE IF NOT EXISTS session_input (
                id text PRIMARY KEY,
                session_id text NOT NULL,
                prompt text NOT NULL,
                delivery text NOT NULL,
                admitted_seq integer NOT NULL,
                promoted_seq integer,
                time_created integer NOT NULL,
                CONSTRAINT fk_session_input_session_id_session_id_fk FOREIGN KEY (session_id) REFERENCES session(id) ON DELETE CASCADE
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
    async fn test_admit_input() {
        let db = create_test_db().await;
        let inbox = SessionInputInbox::new(db);

        let result = inbox
            .admit_input(AdmitInputParams {
                id: "msg_001".into(),
                session_id: "ses_001".into(),
                prompt: Prompt {
                    text: "Hello, world!".into(),
                    files: None,
                    agents: None,
                },
                delivery: InputDelivery::Steer,
            })
            .await;

        assert!(result.is_ok());
        let admitted = result.unwrap();
        assert_eq!(admitted.id, "msg_001");
        assert_eq!(admitted.admitted_seq, 1);
        assert_eq!(admitted.prompt.text, "Hello, world!");
    }

    #[tokio::test]
    async fn test_promote_steers() {
        let db = create_test_db().await;
        let inbox = SessionInputInbox::new(db);

        // Admit a steer input
        inbox
            .admit_input(AdmitInputParams {
                id: "msg_001".into(),
                session_id: "ses_001".into(),
                prompt: Prompt {
                    text: "Steer input".into(),
                    files: None,
                    agents: None,
                },
                delivery: InputDelivery::Steer,
            })
            .await
            .unwrap();

        // Promote steers
        let promoted = inbox
            .promote_steers(PromoteSteersParams {
                session_id: "ses_001".into(),
                cutoff: 100,
            })
            .await
            .unwrap();

        assert_eq!(promoted.len(), 1);
        assert_eq!(promoted[0].id, "msg_001");
        assert!(promoted[0].promoted_seq.is_some());
    }

    #[tokio::test]
    async fn test_legacy_prompted() {
        let db = create_test_db().await;
        let inbox = SessionInputInbox::new(db);

        let result = inbox
            .legacy_prompted(LegacyPromptedParams {
                id: "msg_001".into(),
                session_id: "ses_001".into(),
                prompt: Prompt {
                    text: "Legacy prompt".into(),
                    files: None,
                    agents: None,
                },
                delivery: InputDelivery::Steer,
                time_created: 1700000000000,
                promoted_seq: 5,
            })
            .await;

        assert!(result.is_ok());
        let admitted = result.unwrap();
        assert_eq!(admitted.promoted_seq, Some(5));
        assert_eq!(admitted.admitted_seq, 5);

        // Duplicate should fail
        let dup = inbox
            .legacy_prompted(LegacyPromptedParams {
                id: "msg_001".into(),
                session_id: "ses_001".into(),
                prompt: Prompt {
                    text: "Duplicate".into(),
                    files: None,
                    agents: None,
                },
                delivery: InputDelivery::Queue,
                time_created: 1700000000000,
                promoted_seq: 6,
            })
            .await;

        assert!(dup.is_err());
    }
}
