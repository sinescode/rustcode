//! # Main store — SQLite-backed persistent storage.
//!
//! This is the primary storage layer for OpenCode++.
//! All reads go through SQLite. All writes go to both SQLite and JSONL archive.

use crate::jsonl::SessionJsonl;
use crate::schema::*;
use crate::migration::Migration;
use serde_json::Value;
use sqlx::sqlite::{SqliteConnectOptions, SqlitePoolOptions};
use sqlx::{Executor, SqlitePool};
use std::path::PathBuf;
use std::sync::Arc;
use std::time::Duration;
use tokio::sync::RwLock;
use tracing::{debug, error, info};

/// Configuration for the store.
#[derive(Debug, Clone)]
pub struct StoreConfig {
    /// Path to the SQLite database file.
    pub db_path: PathBuf,
    /// Directory for JSONL archives.
    pub jsonl_dir: Option<PathBuf>,
    /// Maximum JSONL file size before rotation.
    pub jsonl_max_size: u64,
    /// Pool size.
    pub pool_size: u32,
    /// Connection timeout.
    pub connection_timeout: Duration,
    /// Whether to run migrations on open.
    pub run_migrations: bool,
}

impl Default for StoreConfig {
    fn default() -> Self {
        Self {
            db_path: PathBuf::from("opencode.db"),
            jsonl_dir: None,
            jsonl_max_size: 10 * 1024 * 1024, // 10 MB
            pool_size: 4,
            connection_timeout: Duration::from_secs(10),
            run_migrations: true,
        }
    }
}

impl StoreConfig {
    /// Create a new store configuration.
    pub fn new(db_path: impl Into<PathBuf>) -> Self {
        Self {
            db_path: db_path.into(),
            ..Default::default()
        }
    }

    /// Set the JSONL archive directory.
    pub fn with_jsonl(mut self, dir: impl Into<PathBuf>) -> Self {
        self.jsonl_dir = Some(dir.into());
        self
    }
}

/// Error types for store operations.
#[derive(Debug, thiserror::Error)]
pub enum StoreError {
    /// SQLite error.
    #[error("SQLite: {0}")]
    Sqlite(#[from] sqlx::Error),

    /// JSON error.
    #[error("JSON: {0}")]
    Json(#[from] serde_json::Error),

    /// I/O error.
    #[error("I/O: {0}")]
    Io(#[from] std::io::Error),

    /// JSONL archive error.
    #[error("JSONL: {0}")]
    Jsonl(#[from] super::jsonl::JsonlError),

    /// Migration error.
    #[error("Migration: {0}")]
    Migration(String),

    /// Not found.
    #[error("Not found: {0}")]
    NotFound(String),
}

/// The main store — SQLite-backed persistent storage.
///
/// # Example
///
/// ```ignore
/// let config = StoreConfig::new("opencode.db")
///     .with_jsonl("archives/");
/// let store = Store::open(&config).await.unwrap();
/// ```
#[derive(Debug, Clone)]
pub struct Store {
    /// SQLite connection pool.
    pool: SqlitePool,
    /// Store configuration.
    config: Arc<StoreConfig>,
    /// JSONL archive (optional).
    jsonl: Option<Arc<RwLock<SessionJsonl>>>,
}

impl Store {
    /// Open a store, creating tables and running migrations.
    pub async fn open(config: &StoreConfig) -> Result<Self, StoreError> {
        // Ensure parent directory exists
        if let Some(parent) = config.db_path.parent() {
            tokio::fs::create_dir_all(parent).await?;
        }

        // Configure SQLite connection
        let connect_options = SqliteConnectOptions::new()
            .filename(&config.db_path)
            .create_if_missing(true);

        let pool = SqlitePoolOptions::new()
            .max_connections(config.pool_size)
            .connect_with(connect_options)
            .await?;

        // Run DDL
        Self::initialize_schema(&pool).await?;

        // Run migrations
        if config.run_migrations {
            Migration::new(pool.clone()).run().await?;
        }

        // Open JSONL archive if configured
        let jsonl = if let Some(ref dir) = config.jsonl_dir {
            let archive = SessionJsonl::open(dir, config.jsonl_max_size).await?;
            Some(Arc::new(RwLock::new(archive)))
        } else {
            None
        };

        info!(
            "store: opened at {}, pool={}",
            config.db_path.display(),
            config.pool_size
        );

        Ok(Self {
            pool,
            config: Arc::new(config.clone()),
            jsonl,
        })
    }

    /// Initialize the database schema.
    async fn initialize_schema(pool: &SqlitePool) -> Result<(), StoreError> {
        for ddl in ALL_DDL {
            sqlx::query(ddl).execute(pool).await?;
        }
        debug!("store: schema initialized");
        Ok(())
    }

    /// Get the SQLite connection pool.
    pub fn pool(&self) -> &SqlitePool {
        &self.pool
    }

    /// Get the store configuration.
    pub fn config(&self) -> &StoreConfig {
        &self.config
    }

    // ─── Session CRUD ─────────────────────────────────────────────

    /// Save a session to the store.
    pub async fn save_session(&self, session: &SessionRow) -> Result<(), StoreError> {
        sqlx::query(
            r#"
            INSERT INTO sessions (id, created_at, updated_at, state, agent_id, model, provider, metadata)
            VALUES ($1, $2, $3, $4, $5, $6, $7, $8)
            ON CONFLICT(id) DO UPDATE SET
                updated_at = $3,
                state = $4,
                agent_id = $5,
                model = $6,
                provider = $7,
                metadata = $8
            "#,
        )
        .bind(&session.id)
        .bind(&session.created_at)
        .bind(&session.updated_at)
        .bind(&session.state)
        .bind(&session.agent_id)
        .bind(&session.model)
        .bind(&session.provider)
        .bind(&session.metadata)
        .execute(&self.pool)
        .await?;

        self.archive_event("session.saved", &session.id).await;
        Ok(())
    }

    /// Load a session by ID.
    pub async fn load_session(&self, id: &str) -> Result<SessionRow, StoreError> {
        sqlx::query_as::<_, SessionRow>("SELECT * FROM sessions WHERE id = $1")
            .bind(id)
            .fetch_optional(&self.pool)
            .await?
            .ok_or_else(|| StoreError::NotFound(format!("session {id}")))
    }

    /// List all sessions.
    pub async fn list_sessions(&self, limit: i64, offset: i64) -> Result<Vec<SessionRow>, StoreError> {
        Ok(sqlx::query_as::<_, SessionRow>(
            "SELECT * FROM sessions ORDER BY updated_at DESC LIMIT $1 OFFSET $2",
        )
        .bind(limit)
        .bind(offset)
        .fetch_all(&self.pool)
        .await?)
    }

    /// Delete a session and all its turns.
    pub async fn delete_session(&self, id: &str) -> Result<(), StoreError> {
        sqlx::query("DELETE FROM sessions WHERE id = $1")
            .bind(id)
            .execute(&self.pool)
            .await?;
        Ok(())
    }

    // ─── Turn CRUD ────────────────────────────────────────────────

    /// Save a turn to the store.
    pub async fn save_turn(&self, turn: &TurnRow) -> Result<(), StoreError> {
        sqlx::query(
            r#"
            INSERT INTO turns (id, session_id, seq, role, content, tool_name, tool_call_id, is_error, tokens, created_at)
            VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10)
            ON CONFLICT(session_id, seq) DO UPDATE SET
                is_error = $8,
                tokens = $9
            "#,
        )
        .bind(&turn.id)
        .bind(&turn.session_id)
        .bind(turn.seq)
        .bind(&turn.role)
        .bind(&turn.content)
        .bind(&turn.tool_name)
        .bind(&turn.tool_call_id)
        .bind(turn.is_error)
        .bind(turn.tokens)
        .bind(&turn.created_at)
        .execute(&self.pool)
        .await?;

        self.archive_event("turn.saved", &turn.session_id).await;
        Ok(())
    }

    /// Load turns for a session.
    pub async fn load_turns(
        &self,
        session_id: &str,
        limit: i64,
        offset: i64,
    ) -> Result<Vec<TurnRow>, StoreError> {
        Ok(sqlx::query_as::<_, TurnRow>(
            "SELECT * FROM turns WHERE session_id = $1 ORDER BY seq ASC LIMIT $2 OFFSET $3",
        )
        .bind(session_id)
        .bind(limit)
        .bind(offset)
        .fetch_all(&self.pool)
        .await?)
    }

    /// Count turns for a session.
    pub async fn count_turns(&self, session_id: &str) -> Result<i64, StoreError> {
        let (count,): (i64,) = sqlx::query_as(
            "SELECT COUNT(*) FROM turns WHERE session_id = $1",
        )
        .bind(session_id)
        .fetch_one(&self.pool)
        .await?;
        Ok(count)
    }

    // ─── Tool Cache ───────────────────────────────────────────────

    /// Cache a tool result by hash.
    pub async fn cache_tool(
        &self,
        hash: &str,
        name: &str,
        args: &Value,
        result: &str,
        is_error: bool,
        duration_ms: i64,
    ) -> Result<(), StoreError> {
        sqlx::query(
            r#"
            INSERT INTO tools (hash, name, args, result, is_error, duration_ms)
            VALUES ($1, $2, $3, $4, $5, $6)
            ON CONFLICT(hash) DO UPDATE SET
                result = $4,
                is_error = $5,
                duration_ms = $6
            "#,
        )
        .bind(hash)
        .bind(name)
        .bind(serde_json::to_string(args)?)
        .bind(result)
        .bind(is_error)
        .bind(duration_ms)
        .execute(&self.pool)
        .await?;
        Ok(())
    }

    /// Look up a cached tool result by hash.
    pub async fn lookup_tool(&self, hash: &str) -> Result<Option<(String, bool)>, StoreError> {
        let row = sqlx::query_as::<_, (String, bool)>(
            "SELECT result, is_error FROM tools WHERE hash = $1",
        )
        .bind(hash)
        .fetch_optional(&self.pool)
        .await?;
        Ok(row)
    }

    // ─── Store Stats ──────────────────────────────────────────────

    /// Get store statistics.
    pub async fn stats(&self) -> Result<StoreStats, StoreError> {
        let (session_count,): (i64,) =
            sqlx::query_as("SELECT COUNT(*) FROM sessions")
                .fetch_one(&self.pool)
                .await?;

        let (turn_count,): (i64,) =
            sqlx::query_as("SELECT COUNT(*) FROM turns")
                .fetch_one(&self.pool)
                .await?;

        let (event_count,): (i64,) =
            sqlx::query_as("SELECT COUNT(*) FROM events")
                .fetch_one(&self.pool)
                .await?;

        let (schema_version,): (i64,) =
            sqlx::query_as("SELECT COALESCE(MAX(version), 0) FROM schema_version")
                .fetch_one(&self.pool)
                .await?;

        let db_size = tokio::fs::metadata(&self.config.db_path)
            .await
            .map(|m| m.len() as i64)
            .unwrap_or(0);

        Ok(StoreStats {
            session_count,
            turn_count,
            event_count,
            schema_version,
            db_size_bytes: db_size,
            jsonl_size_bytes: 0, // computed separately if needed
        })
    }

    /// Archive an event (if JSONL is configured).
    async fn archive_event(&self, event_type: &str, session_id: &str) {
        if let Some(ref jsonl) = self.jsonl {
            if let Ok(mut archive) = jsonl.try_write() {
                let entry = serde_json::json!({
                    "type": event_type,
                    "session_id": session_id,
                    "timestamp": chrono::Utc::now().to_rfc3339(),
                });
                if let Err(e) = archive.append(session_id, &entry).await {
                    error!(error = %e, "store: failed to archive event");
                }
            }
        }
    }

    /// Close the store.
    pub async fn close(self) -> Result<(), StoreError> {
        self.pool.close().await;
        if let Some(ref jsonl) = self.jsonl {
            let mut archive = jsonl.write().await;
            archive.close_all().await?;
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    struct TestStore {
        store: Store,
        _dir: TempDir,
    }

    impl TestStore {
        async fn new() -> Self {
            let dir = TempDir::new().unwrap();
            let config = StoreConfig::new(dir.path().join("test.db"));
            let store = Store::open(&config).await.unwrap();
            Self { store, _dir: dir }
        }
    }

    async fn test_store() -> Store {
        TestStore::new().await.store
    }

    #[tokio::test]
    async fn test_open_and_stats() {
        let store = test_store().await;
        let stats = store.stats().await.unwrap();
        assert!(stats.session_count >= 0);
    }

    #[tokio::test]
    async fn test_save_and_load_session() {
        let store = test_store().await;
        let session = SessionRow {
            id: "test-session-1".into(),
            created_at: "2026-01-01T00:00:00Z".into(),
            updated_at: "2026-01-01T00:00:00Z".into(),
            state: "idle".into(),
            agent_id: None,
            model: Some("claude".into()),
            provider: Some("anthropic".into()),
            metadata: "{}".into(),
        };
        store.save_session(&session).await.unwrap();

        let loaded = store.load_session("test-session-1").await.unwrap();
        assert_eq!(loaded.id, "test-session-1");
        assert_eq!(loaded.state, "idle");
    }

    #[tokio::test]
    async fn test_save_and_load_turns() {
        let store = test_store().await;
        let session = SessionRow {
            id: "turn-test".into(),
            created_at: "2026-01-01T00:00:00Z".into(),
            updated_at: "2026-01-01T00:00:00Z".into(),
            state: "idle".into(),
            agent_id: None,
            model: None,
            provider: None,
            metadata: "{}".into(),
        };
        store.save_session(&session).await.unwrap();

        let turn = TurnRow {
            id: "turn-1".into(),
            session_id: "turn-test".into(),
            seq: 1,
            role: "user".into(),
            content: "Hello".into(),
            tool_name: None,
            tool_call_id: None,
            is_error: false,
            tokens: Some(10),
            created_at: "2026-01-01T00:00:01Z".into(),
        };
        store.save_turn(&turn).await.unwrap();

        let turns = store.load_turns("turn-test", 10, 0).await.unwrap();
        assert_eq!(turns.len(), 1);
        assert_eq!(turns[0].content, "Hello");
    }

    #[tokio::test]
    async fn test_tool_cache() {
        let store = test_store().await;
        store
            .cache_tool(
                "abc123",
                "bash",
                &serde_json::json!({"cmd": "echo hi"}),
                "hi\n",
                false,
                5,
            )
            .await
            .unwrap();

        let cached = store.lookup_tool("abc123").await.unwrap();
        assert!(cached.is_some());
        let (result, is_error) = cached.unwrap();
        assert_eq!(result, "hi\n");
        assert!(!is_error);
    }

    #[tokio::test]
    async fn test_list_sessions() {
        let store = test_store().await;
        let s1 = SessionRow {
            id: "list-1".into(),
            created_at: "2026-01-01T00:00:00Z".into(),
            updated_at: "2026-01-01T00:00:01Z".into(),
            state: "idle".into(),
            agent_id: None,
            model: None,
            provider: None,
            metadata: "{}".into(),
        };
        let s2 = SessionRow {
            id: "list-2".into(),
            created_at: "2026-01-01T00:00:00Z".into(),
            updated_at: "2026-01-01T00:00:02Z".into(),
            state: "active".into(),
            agent_id: None,
            model: None,
            provider: None,
            metadata: "{}".into(),
        };
        store.save_session(&s1).await.unwrap();
        store.save_session(&s2).await.unwrap();

        let sessions = store.list_sessions(10, 0).await.unwrap();
        assert!(sessions.len() >= 2);
    }

    #[tokio::test]
    async fn test_delete_session() {
        let store = test_store().await;
        let session = SessionRow {
            id: "delete-me".into(),
            created_at: "2026-01-01T00:00:00Z".into(),
            updated_at: "2026-01-01T00:00:00Z".into(),
            state: "idle".into(),
            agent_id: None,
            model: None,
            provider: None,
            metadata: "{}".into(),
        };
        store.save_session(&session).await.unwrap();
        store.delete_session("delete-me").await.unwrap();

        let result = store.load_session("delete-me").await;
        assert!(result.is_err());
    }
}
