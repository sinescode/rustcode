//! # SQLite schema definitions for the store.

use serde::{Deserialize, Serialize};

/// The current schema version.
pub const SCHEMA_VERSION: i64 = 1;

/// SQL to create the sessions table.
pub const CREATE_SESSIONS: &str = r#"
CREATE TABLE IF NOT EXISTS sessions (
    id TEXT PRIMARY KEY NOT NULL,
    created_at TEXT NOT NULL DEFAULT (datetime('now')),
    updated_at TEXT NOT NULL DEFAULT (datetime('now')),
    state TEXT NOT NULL DEFAULT 'idle',
    agent_id TEXT,
    model TEXT,
    provider TEXT,
    metadata TEXT DEFAULT '{}'
);
"#;

/// SQL to create the turns table.
pub const CREATE_TURNS: &str = r#"
CREATE TABLE IF NOT EXISTS turns (
    id TEXT PRIMARY KEY NOT NULL,
    session_id TEXT NOT NULL REFERENCES sessions(id) ON DELETE CASCADE,
    seq INTEGER NOT NULL,
    role TEXT NOT NULL CHECK(role IN ('user', 'assistant', 'tool')),
    content TEXT NOT NULL,
    tool_name TEXT,
    tool_call_id TEXT,
    is_error INTEGER NOT NULL DEFAULT 0,
    tokens INTEGER,
    created_at TEXT NOT NULL DEFAULT (datetime('now')),
    UNIQUE(session_id, seq)
);
"#;

/// SQL to create the events table.
pub const CREATE_EVENTS: &str = r#"
CREATE TABLE IF NOT EXISTS events (
    id TEXT PRIMARY KEY NOT NULL,
    session_id TEXT REFERENCES sessions(id) ON DELETE SET NULL,
    event_type TEXT NOT NULL,
    data TEXT NOT NULL DEFAULT '{}',
    seq INTEGER NOT NULL,
    created_at TEXT NOT NULL DEFAULT (datetime('now'))
);
"#;

/// SQL to create the tools table (cached tool results).
pub const CREATE_TOOLS: &str = r#"
CREATE TABLE IF NOT EXISTS tools (
    hash TEXT PRIMARY KEY NOT NULL,
    name TEXT NOT NULL,
    args TEXT NOT NULL DEFAULT '{}',
    result TEXT NOT NULL DEFAULT '',
    is_error INTEGER NOT NULL DEFAULT 0,
    duration_ms INTEGER,
    created_at TEXT NOT NULL DEFAULT (datetime('now'))
);
"#;

/// SQL to create the schema version table.
pub const CREATE_SCHEMA: &str = r#"
CREATE TABLE IF NOT EXISTS schema_version (
    version INTEGER PRIMARY KEY NOT NULL,
    applied_at TEXT NOT NULL DEFAULT (datetime('now'))
);
"#;

/// SQL to create index on turns by session + seq.
pub const CREATE_IDX_TURNS_SESSION: &str = r#"
CREATE INDEX IF NOT EXISTS idx_turns_session_seq
    ON turns(session_id, seq);
"#;

/// SQL to create index on events by session.
pub const CREATE_IDX_EVENTS_SESSION: &str = r#"
CREATE INDEX IF NOT EXISTS idx_events_session
    ON events(session_id);
"#;

/// SQL to create index on events by type.
pub const CREATE_IDX_EVENTS_TYPE: &str = r#"
CREATE INDEX IF NOT EXISTS idx_events_type
    ON events(event_type);
"#;

/// SQL to enable WAL mode.
pub const ENABLE_WAL: &str = "PRAGMA journal_mode=WAL;";

/// SQL to enable foreign keys.
pub const ENABLE_FOREIGN_KEYS: &str = "PRAGMA foreign_keys=ON;";

/// SQL to set busy timeout.
pub const SET_BUSY_TIMEOUT: &str = "PRAGMA busy_timeout=5000;";

/// SQL to enable memory-mapped reads.
pub const ENABLE_MMAP: &str = "PRAGMA mmap_size=268435456;";  // 256 MB

/// All schema DDL statements in order.
pub const ALL_DDL: &[&str] = &[
    ENABLE_WAL,
    ENABLE_FOREIGN_KEYS,
    SET_BUSY_TIMEOUT,
    ENABLE_MMAP,
    CREATE_SESSIONS,
    CREATE_TURNS,
    CREATE_EVENTS,
    CREATE_TOOLS,
    CREATE_SCHEMA,
    CREATE_IDX_TURNS_SESSION,
    CREATE_IDX_EVENTS_SESSION,
    CREATE_IDX_EVENTS_TYPE,
];

/// A session row from the database.
#[derive(Debug, Clone, Serialize, Deserialize, sqlx::FromRow)]
pub struct SessionRow {
    /// Session UUID.
    pub id: String,
    /// Creation timestamp.
    pub created_at: String,
    /// Last update timestamp.
    pub updated_at: String,
    /// Current state (e.g., "idle", "running").
    pub state: String,
    /// Agent ID.
    pub agent_id: Option<String>,
    /// Model name.
    pub model: Option<String>,
    /// Provider name.
    pub provider: Option<String>,
    /// JSON metadata blob.
    pub metadata: String,
}

/// A turn row from the database.
#[derive(Debug, Clone, Serialize, Deserialize, sqlx::FromRow)]
pub struct TurnRow {
    /// Turn UUID.
    pub id: String,
    /// Session UUID.
    pub session_id: String,
    /// Sequence number within session.
    pub seq: i64,
    /// Message role.
    pub role: String,
    /// Message content.
    pub content: String,
    /// Tool name (for tool messages).
    pub tool_name: Option<String>,
    /// Tool call ID.
    pub tool_call_id: Option<String>,
    /// Whether this was an error result.
    pub is_error: bool,
    /// Token count.
    pub tokens: Option<i64>,
    /// Creation timestamp.
    pub created_at: String,
}

/// An event row from the database.
#[derive(Debug, Clone, Serialize, Deserialize, sqlx::FromRow)]
pub struct EventRow {
    /// Event UUID.
    pub id: String,
    /// Session UUID.
    pub session_id: Option<String>,
    /// Event type.
    pub event_type: String,
    /// JSON data.
    pub data: String,
    /// Sequence number.
    pub seq: i64,
    /// Creation timestamp.
    pub created_at: String,
}

/// Statistics about the store.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StoreStats {
    /// Number of sessions.
    pub session_count: i64,
    /// Number of turns.
    pub turn_count: i64,
    /// Number of events.
    pub event_count: i64,
    /// Schema version.
    pub schema_version: i64,
    /// Database size in bytes.
    pub db_size_bytes: i64,
    /// JSONL archive size in bytes (0 if disabled).
    pub jsonl_size_bytes: i64,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_all_ddl_non_empty() {
        assert!(!ALL_DDL.is_empty());
        assert!(ALL_DDL.contains(&ENABLE_WAL));
    }

    #[test]
    fn test_schema_version_positive() {
        assert!(SCHEMA_VERSION > 0);
    }
}
