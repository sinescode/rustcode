//! Event sourcing projection pipeline — reads events from the event store,
//! projects them to reconstruct session state, and tracks projection progress.
//!
//! Ported from:
//! - `packages/core/src/event.ts` (lines 85–91, 96–133, 431–451)
//! - `packages/blazecode/src/sync/index.ts` (lines 75–171)
//! - `packages/blazecode/src/session/projectors.ts` (lines 1–85)

use crate::database::{DatabaseService, DatabaseServiceError, EventRow};
use crate::event::{
    EventCursor, EventError, EventPayload, EventRegistry, SerializedEvent, SyncConfig,
};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::RwLock;

// ══════════════════════════════════════════════════════════════════════════════
// ProjectionState — tracks the last projected sequence per aggregate
// ══════════════════════════════════════════════════════════════════════════════

/// Tracks the last projected event sequence number for an aggregate (session).
///
/// # Source
/// Ported from `packages/core/src/event.ts` — the `sync` aggregate tracking
/// in `EventSequenceTable`.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProjectionState {
    /// Aggregate identifier (session ID).
    pub aggregate_id: String,
    /// The last sequence number that was projected.
    pub last_seq: u64,
}

// ══════════════════════════════════════════════════════════════════════════════
// ProjectorFn — function that projects a single event
// ══════════════════════════════════════════════════════════════════════════════

/// Function that handles projecting a single event to update read models.
///
/// # Source
/// Ported from `packages/blazecode/src/session/projectors.ts` — each
/// `SyncEvent.project()` call creates a projector function.
pub type ProjectEventHandler = Arc<
    dyn Fn(EventPayload) -> Result<(), EventError> + Send + Sync,
>;

// ══════════════════════════════════════════════════════════════════════════════
// EventProjector — the main projection engine
// ══════════════════════════════════════════════════════════════════════════════

/// Reads events from the event store and projects them to reconstruct
/// current session state. Supports catch-up projection from a checkpoint.
///
/// # Source
/// Ported from `packages/blazecode/src/sync/index.ts` — the `process` function
/// and `replay` path.
pub struct EventProjector {
    /// Database service for reading/writing events and state.
    db: Arc<DatabaseService>,
    /// Event registry for looking up event definitions.
    registry: Arc<EventRegistry>,
    /// Registered projectors per event type.
    projectors: RwLock<HashMap<String, Vec<ProjectEventHandler>>>,
    /// In-memory projection state (aggregate_id -> last_seq).
    state: RwLock<HashMap<String, ProjectionState>>,
}

impl EventProjector {
    /// Create a new EventProjector.
    pub fn new(db: Arc<DatabaseService>, registry: Arc<EventRegistry>) -> Self {
        Self {
            db,
            registry,
            projectors: RwLock::new(HashMap::new()),
            state: RwLock::new(HashMap::new()),
        }
    }

    /// Register a projector function for a specific event type.
    ///
    /// # Source
    /// Ported from `packages/blazecode/src/sync/index.ts` — `SyncEvent.init`'s
    /// registration of projector functions.
    pub async fn register(&self, event_type: &str, projector: ProjectEventHandler) {
        let mut projs = self.projectors.write().await;
        projs
            .entry(event_type.to_string())
            .or_default()
            .push(projector);
    }

    /// Project a single event through all registered projectors.
    ///
    /// Returns the number of projectors that processed the event.
    ///
    /// # Source
    /// Ported from `packages/blazecode/src/sync/index.ts` — the `process`
    /// function which looks up the projector and runs it.
    pub async fn project_event(&self, event: &EventPayload) -> Result<usize, EventError> {
        let projectors = {
            let projs = self.projectors.read().await;
            projs.get(&event.event_type).cloned().unwrap_or_default()
        };

        if projectors.is_empty() {
            return Ok(0);
        }

        for projector in &projectors {
            projector(event.clone()).map_err(|e| {
                EventError::Internal(format!(
                    "Projector failed for event type {}: {e}",
                    event.event_type
                ))
            })?;
        }

        // Update projection state
        if let Some(seq) = event.seq {
            if let Some(aggregate_id) = event.data.get("sessionID").and_then(|v| v.as_str()) {
                let mut state = self.state.write().await;
                state.insert(
                    aggregate_id.to_string(),
                    ProjectionState {
                        aggregate_id: aggregate_id.to_string(),
                        last_seq: seq,
                    },
                );
            }
        }

        Ok(projectors.len())
    }

    /// Catch-up projection: reads events from the database from the last
    /// checkpoint to the latest available, and projects each one.
    ///
    /// Returns the number of events projected.
    ///
    /// # Source
    /// Ported from `packages/blazecode/src/sync/index.ts` — the `replay` path
    /// which reads events from `EventTable` and processes them sequentially.
    pub async fn catch_up(
        &self,
        aggregate_id: &str,
    ) -> Result<u64, EventError> {
        // Get the last projected sequence from in-memory state
        let last_seq = {
            let state = self.state.read().await;
            state
                .get(aggregate_id)
                .map(|s| s.last_seq)
                .unwrap_or(0)
        };

        // Also check the database for persisted projection checkpoint
        let db_last_seq = self
            .db
            .get_event_sequence(aggregate_id)
            .await
            .map_err(|e| EventError::Internal(format!("DB error reading sequence: {e}")))?
            .map(|r| r.seq as u64)
            .unwrap_or(0);

        let effective_last = last_seq.max(db_last_seq);

        // Read events after the checkpoint
        let rows: Vec<EventRow> = self
            .db
            .list_events_after(aggregate_id, Some(effective_last as i64), Some(500))
            .await
            .map_err(|e| EventError::Internal(format!("DB error listing events: {e}")))?;

        if rows.is_empty() {
            return Ok(0);
        }

        let mut count = 0u64;
        for row in &rows {
            let definition = self.registry.get(&row.event_type).await;

            // Convert database row to EventPayload
            let data: serde_json::Value = serde_json::from_str(&row.data)
                .map_err(|e| EventError::Internal(format!("Invalid event JSON: {e}")))?;

            let payload = EventPayload {
                id: crate::event::EventId::from(row.id.clone()),
                event_type: row.event_type.clone(),
                data,
                seq: Some(row.seq as u64),
                version: definition.as_ref().and_then(|d| d.sync.as_ref().map(|s| s.version)),
                location: None,
                metadata: None,
                replay: true,
            };

            self.project_event(&payload).await?;
            count += 1;
        }

        // Persist the new checkpoint
        if let Some(last_row) = rows.last() {
            self.db
                .upsert_event_sequence(aggregate_id, last_row.seq, None)
                .await
                .map_err(|e| EventError::Internal(format!("DB error saving sequence: {e}")))?;

            // Update in-memory state
            let mut state = self.state.write().await;
            state.insert(
                aggregate_id.to_string(),
                ProjectionState {
                    aggregate_id: aggregate_id.to_string(),
                    last_seq: last_row.seq as u64,
                },
            );
        }

        Ok(count)
    }

    /// Get the last projected sequence for an aggregate.
    pub async fn last_projected_seq(&self, aggregate_id: &str) -> Option<u64> {
        let state = self.state.read().await;
        state.get(aggregate_id).map(|s| s.last_seq)
    }

    /// Load the projection state from the database for all known aggregates.
    ///
    /// # Source
    /// Ported from `packages/blazecode/src/sync/index.ts` — the `replay` path
    /// reads from `EventSequenceTable`.
    pub async fn load_state_from_db(&self, aggregate_id: &str) -> Result<Option<u64>, EventError> {
        let row = self
            .db
            .get_event_sequence(aggregate_id)
            .await
            .map_err(|e| EventError::Internal(format!("DB error: {e}")))?;

        Ok(row.map(|r| r.seq as u64))
    }

    /// Rebuild the full state for an aggregate by replaying all events.
    ///
    /// # Source
    /// Ported from `packages/blazecode/src/sync/index.ts` — the `replayAll`
    /// path which replays all events for an aggregate.
    pub async fn rebuild(&self, aggregate_id: &str) -> Result<u64, EventError> {
        // Clear in-memory state
        {
            let mut state = self.state.write().await;
            state.remove(aggregate_id);
        }

        // Reset DB checkpoint and re-project all events
        self.db
            .upsert_event_sequence(aggregate_id, 0, None)
            .await
            .map_err(|e| EventError::Internal(format!("DB error resetting sequence: {e}")))?;

        self.catch_up(aggregate_id).await
    }
}

// ══════════════════════════════════════════════════════════════════════════════
// Helper: CommitSyncEvent
// ══════════════════════════════════════════════════════════════════════════════

/// Commit a synchronous event: writes to EventTable, updates sequence, and
/// projects through registered handlers in a single transactional flow.
///
/// # Source
/// Ported from `packages/blazecode/src/sync/index.ts` — the `run` function
/// which writes event + sequence atomically with the projector.
pub async fn commit_sync_event(
    db: &DatabaseService,
    projector: &EventProjector,
    aggregate_id: &str,
    event_id: &str,
    event_type: &str,
    data: &serde_json::Value,
    sync_version: u32,
) -> Result<(u64, EventPayload), EventError> {
    use sqlx::Transaction;

    // Get the next sequence number
    let current_seq = db
        .get_event_sequence(aggregate_id)
        .await
        .map_err(|e| EventError::Internal(format!("DB error reading sequence: {e}")))?
        .map(|r| r.seq)
        .unwrap_or(0);

    let next_seq = current_seq + 1;

    // Serialize event data
    let data_str = serde_json::to_string(data)
        .map_err(|e| EventError::Internal(format!("Failed to serialize event data: {e}")))?;

    let versioned_type = if sync_version > 0 {
        format!("{event_type}.{sync_version}")
    } else {
        event_type.to_string()
    };

    // Use a transaction to atomically insert event + update sequence
    let mut tx = db.begin().await
        .map_err(|e| EventError::Internal(format!("DB error starting transaction: {e}")))?;

    // Insert event record within transaction
    sqlx::query(
        "INSERT INTO event (id, aggregate_id, seq, type, data) VALUES (?1, ?2, ?3, ?4, ?5)"
    )
    .bind(event_id)
    .bind(aggregate_id)
    .bind(next_seq as i64)
    .bind(&versioned_type)
    .bind(&data_str)
    .execute(&mut *tx)
    .await
    .map_err(|e| EventError::Internal(format!("DB error inserting event: {e}")))?;

    // Update event sequence within transaction
    sqlx::query(
        "INSERT INTO event_sequence (aggregate_id, seq, time_updated) VALUES (?1, ?2, ?3)
         ON CONFLICT(aggregate_id) DO UPDATE SET seq = ?2, time_updated = ?3"
    )
    .bind(aggregate_id)
    .bind(next_seq as i64)
    .bind(chrono::Utc::now().timestamp_millis())
    .execute(&mut *tx)
    .await
    .map_err(|e| EventError::Internal(format!("DB error updating sequence: {e}")))?;

    tx.commit().await
        .map_err(|e| EventError::Internal(format!("DB error committing transaction: {e}")))?;

    // Build the event payload for projection
    let payload = EventPayload {
        id: crate::event::EventId::from(event_id.to_string()),
        event_type: event_type.to_string(),
        data: data.clone(),
        seq: Some(next_seq as u64),
        version: Some(sync_version),
        location: None,
        metadata: None,
        replay: false,
    };

    // Project the event (outside the transaction to avoid long-held locks)
    projector.project_event(&payload).await?;

    Ok((next_seq as u64, payload))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::event::{EventDefinition, EventId};
    use serde_json::json;

    #[tokio::test]
    async fn test_event_projector_new() {
        let registry = Arc::new(EventRegistry::new());
        let proj = EventProjector::new(
            Arc::new(DatabaseService::new(
                sqlx::SqlitePool::connect(":memory:")
                    .await
                    .expect("create pool"),
            )),
            registry,
        );
        assert!(proj.last_projected_seq("test_session").await.is_none());
    }

    #[tokio::test]
    async fn test_event_projector_register_and_project() {
        let registry = Arc::new(EventRegistry::new());
        let pool = sqlx::SqlitePool::connect(":memory:")
            .await
            .expect("create pool");
        // Create event_sequence table
        sqlx::query(
            "CREATE TABLE IF NOT EXISTS event_sequence (
                aggregate_id text PRIMARY KEY,
                seq integer NOT NULL,
                owner_id text
            )",
        )
        .execute(&pool)
        .await
        .unwrap();
        // Create event table
        sqlx::query(
            "CREATE TABLE IF NOT EXISTS event (
                id text PRIMARY KEY,
                aggregate_id text NOT NULL,
                seq integer NOT NULL,
                type text NOT NULL,
                data text NOT NULL
            )",
        )
        .execute(&pool)
        .await
        .unwrap();

        let db = Arc::new(DatabaseService::new(pool));
        let proj = EventProjector::new(Arc::clone(&db), registry);

        let projected = Arc::new(RwLock::new(0u32));
        let projected_clone = projected.clone();

        proj.register(
            "test.event",
            Arc::new(move |_payload| {
                let proj = projected_clone.clone();
                let mut count = proj.blocking_write();
                *count += 1;
                Ok(())
            }),
        )
        .await;

        let payload = EventPayload::new(
            EventId::create(),
            "test.event",
            json!({"sessionID": "ses_001"}),
        )
        .with_version(1);

        let result = proj.project_event(&payload).await.unwrap();
        assert_eq!(result, 1);
        assert_eq!(*projected.read().await, 1);
    }

    #[tokio::test]
    async fn test_commit_sync_event() {
        let registry = Arc::new(EventRegistry::new());
        let pool = sqlx::SqlitePool::connect(":memory:")
            .await
            .expect("create pool");
        // Create tables
        sqlx::query(
            "CREATE TABLE IF NOT EXISTS event_sequence (
                aggregate_id text PRIMARY KEY,
                seq integer NOT NULL,
                owner_id text
            )",
        )
        .execute(&pool)
        .await
        .unwrap();
        sqlx::query(
            "CREATE TABLE IF NOT EXISTS event (
                id text PRIMARY KEY,
                aggregate_id text NOT NULL,
                seq integer NOT NULL,
                type text NOT NULL,
                data text NOT NULL
            )",
        )
        .execute(&pool)
        .await
        .unwrap();

        let db = Arc::new(DatabaseService::new(pool));
        let proj = EventProjector::new(Arc::clone(&db), registry);

        let result = commit_sync_event(
            &db,
            &proj,
            "ses_001",
            "evt_test_001",
            "session.test.event",
            &json!({"sessionID": "ses_001", "value": 42}),
            1,
        )
        .await;

        assert!(result.is_ok());
        let (seq, payload) = result.unwrap();
        assert_eq!(seq, 1);
        assert_eq!(payload.data["value"], 42);

        // Verify sequence was persisted
        let seq_row = db.get_event_sequence("ses_001").await.unwrap().unwrap();
        assert_eq!(seq_row.seq, 1);
    }
}
