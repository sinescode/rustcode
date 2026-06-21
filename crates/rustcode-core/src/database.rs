//! Database types — SQLite tables, columns, migrations, paths, and configuration.
//!
//! Ported from: `packages/core/src/database/database.ts`
//!              `packages/core/src/database/path.ts`
//!              `packages/core/src/database/migration.ts`
//!              `packages/core/src/database/migration.gen.ts`
//!              `packages/core/src/database/schema.gen.ts`
//!              `packages/core/src/database/schema.sql.ts`
//!              `packages/core/src/database/sqlite.ts`
//!              `packages/core/src/data-migration.sql.ts`
//!              `packages/core/src/global.ts`
//! OpenCode commit: 5d0f86606ac30690f79f0a6a9f41a1f49fe95d0b
//!
//! ## Architecture
//!
//! The TS codebase uses drizzle-orm for SQLite with 18 tables and 35+
//! migrations. This module provides the Rust equivalents:
//!
//! - Table definitions as SQL constants (the canonical schema)
//! - Column type wrappers for AbsolutePath storage
//! - Migration types (`Migration`, `MigrationMeta`)
//! - Database path computation (XDG data dirs)
//! - Database connection configuration
//!
//! ## Tables
//!
//! | Table | Purpose |
//! |---|---|
//! | `workspace` | Workspace instances |
//! | `project` | Project metadata |
//! | `project_directory` | Per-project directory configs |
//! | `session` | Session records |
//! | `session_message` | Session messages (event-sourced) |
//! | `session_input` | User input inbox for sessions |
//! | `session_context_epoch` | Context epoch and baseline |
//! | `session_share` | Shared session URLs |
//! | `message` | (legacy) Messages |
//! | `part` | (legacy) Message parts |
//! | `todo` | Session todo items |
//! | `account` | Account credentials |
//! | `control_account` | Control account credentials |
//! | `account_state` | Active account/organization state |
//! | `credential` | Integration credentials |
//! | `permission` | Permission rules |
//! | `event` | Event-sourced events |
//! | `event_sequence` | Per-aggregate sequence numbers |
//! | `data_migration` | Data migration bookkeeping |
//! | `migration` | SQL migration journal |

use serde::{Deserialize, Serialize};

// ── PRAGMA configuration ────────────────────────────────────────────────

/// SQLite PRAGMA statements applied on every connection open.
///
/// # Source
/// Ported from `packages/core/src/database/database.ts` lines 27–32
/// (the PRAGMA statements inside `makeDatabase`).
pub const CONNECTION_PRAGMAS: &[&str] = &[
    "PRAGMA journal_mode = WAL",
    "PRAGMA synchronous = NORMAL",
    "PRAGMA busy_timeout = 5000",
    "PRAGMA cache_size = -64000",
    "PRAGMA foreign_keys = ON",
    "PRAGMA wal_checkpoint(PASSIVE)",
];

// ── Database path computation ───────────────────────────────────────────

/// Application name used for XDG path derivation.
///
/// # Source
/// Ported from `packages/core/src/global.ts` line 11 (`const app = "opencode"`).
pub const APP_NAME: &str = "opencode";

/// Database file name for the default channel.
///
/// # Source
/// Ported from `packages/core/src/database/database.ts` line 53
/// (`return join(Global.Path.data, "opencode.db")`).
pub const DEFAULT_DB_FILE: &str = "opencode.db";

/// Compute the database path following the same logic as the TS `path()` function.
///
/// # Source
/// Ported from `packages/core/src/database/database.ts` lines 43–55.
///
/// Priority:
/// 1. If `OPENCODE_DB` is set and is `:memory:` or an absolute path, use it.
/// 2. If `OPENCODE_DB` is set and is relative, join with `data` dir.
/// 3. If channel is `latest`, `beta`, or `prod`, use `opencode.db`.
/// 4. Otherwise, use `opencode-{sanitized_channel}.db`.
pub fn database_path(
    data_dir: &str,
    opencode_db: Option<&str>,
    channel: Option<&str>,
    disable_channel_db: bool,
) -> String {
    if let Some(db_path) = opencode_db {
        if db_path == ":memory:" {
            return db_path.to_string();
        }
        if db_path.starts_with('/') {
            return db_path.to_string();
        }
        // Relative — join with data dir
        return format!("{}/{}", data_dir.trim_end_matches('/'), db_path);
    }

    let channel = channel.unwrap_or("latest");
    let is_default_channel = matches!(channel, "latest" | "beta" | "prod") || disable_channel_db;

    if is_default_channel {
        format!("{}/{}", data_dir.trim_end_matches('/'), DEFAULT_DB_FILE)
    } else {
        let sanitized = sanitize_channel_name(channel);
        format!("{data_dir}/opencode-{sanitized}.db")
    }
}

/// Sanitize the installation channel name for use in a filename.
///
/// # Source
/// Ported from `packages/core/src/database/database.ts` line 54
/// (`InstallationChannel.replace(/[^a-zA-Z0-9._-]/g, "-")`).
fn sanitize_channel_name(channel: &str) -> String {
    channel
        .chars()
        .map(|c| {
            if c.is_ascii_alphanumeric() || c == '.' || c == '_' || c == '-' {
                c
            } else {
                '-'
            }
        })
        .collect()
}

// ── XDG directory paths ─────────────────────────────────────────────────

/// Global path configuration (XDG-based).
///
/// # Source
/// Ported from `packages/core/src/global.ts` lines 11–29
/// (`Path` object with XDG directories).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GlobalPaths {
    /// User home directory
    pub home: String,
    /// XDG data directory: `$XDG_DATA_HOME/opencode`
    pub data: String,
    /// Binary cache directory: `$XDG_CACHE_HOME/opencode/bin`
    pub bin: String,
    /// Log directory: `$XDG_DATA_HOME/opencode/log`
    pub log: String,
    /// Repos directory: `$XDG_DATA_HOME/opencode/repos`
    pub repos: String,
    /// XDG cache directory: `$XDG_CACHE_HOME/opencode`
    pub cache: String,
    /// XDG config directory: `$XDG_CONFIG_HOME/opencode`
    pub config: String,
    /// XDG state directory: `$XDG_STATE_HOME/opencode`
    pub state: String,
    /// Temp directory: `$TMPDIR/opencode`
    pub tmp: String,
}

impl Default for GlobalPaths {
    fn default() -> Self {
        let home = dirs::home_dir()
            .map(|p| p.display().to_string())
            .unwrap_or_else(|| "/tmp".to_string());
        let data = dirs::data_dir()
            .map(|p| format!("{}/{APP_NAME}", p.display()))
            .unwrap_or_else(|| format!("{home}/.local/share/{APP_NAME}"));
        let cache = dirs::cache_dir()
            .map(|p| format!("{}/{APP_NAME}", p.display()))
            .unwrap_or_else(|| format!("{home}/.cache/{APP_NAME}"));
        let config = dirs::config_dir()
            .map(|p| format!("{}/{APP_NAME}", p.display()))
            .unwrap_or_else(|| format!("{home}/.config/{APP_NAME}"));
        let state = dirs::state_dir()
            .map(|p| format!("{}/{APP_NAME}", p.display()))
            .unwrap_or_else(|| format!("{home}/.local/state/{APP_NAME}"));
        let tmp = std::env::temp_dir().join(APP_NAME).display().to_string();

        Self {
            bin: format!("{cache}/bin"),
            log: format!("{data}/log"),
            repos: format!("{data}/repos"),
            home,
            data,
            cache,
            config,
            state,
            tmp,
        }
    }
}

impl GlobalPaths {
    /// Create paths, optionally overriding specific directories.
    ///
    /// # Source
    /// Ported from `packages/core/src/global.ts` lines 59–72
    /// (`make(input)` function).
    pub fn new(overrides: PathsOverride) -> Self {
        let defaults = Self::default();
        Self {
            home: overrides.home.unwrap_or(defaults.home),
            data: overrides.data.unwrap_or(defaults.data),
            bin: overrides.bin.unwrap_or(defaults.bin),
            log: overrides.log.unwrap_or(defaults.log),
            repos: overrides.repos.unwrap_or(defaults.repos),
            cache: overrides.cache.unwrap_or(defaults.cache),
            config: overrides.config.unwrap_or(defaults.config),
            state: overrides.state.unwrap_or(defaults.state),
            tmp: overrides.tmp.unwrap_or(defaults.tmp),
        }
    }

    /// Get the full database path.
    pub fn database_path(&self, opencode_db: Option<&str>, channel: Option<&str>) -> String {
        database_path(&self.data, opencode_db, channel, false)
    }
}

/// Optional overrides for individual path components.
#[derive(Debug, Default, Clone, Serialize, Deserialize)]
pub struct PathsOverride {
    /// Override home directory
    #[serde(skip_serializing_if = "Option::is_none")]
    pub home: Option<String>,
    /// Override data directory
    #[serde(skip_serializing_if = "Option::is_none")]
    pub data: Option<String>,
    /// Override bin directory
    #[serde(skip_serializing_if = "Option::is_none")]
    pub bin: Option<String>,
    /// Override log directory
    #[serde(skip_serializing_if = "Option::is_none")]
    pub log: Option<String>,
    /// Override repos directory
    #[serde(skip_serializing_if = "Option::is_none")]
    pub repos: Option<String>,
    /// Override cache directory
    #[serde(skip_serializing_if = "Option::is_none")]
    pub cache: Option<String>,
    /// Override config directory
    #[serde(skip_serializing_if = "Option::is_none")]
    pub config: Option<String>,
    /// Override state directory
    #[serde(skip_serializing_if = "Option::is_none")]
    pub state: Option<String>,
    /// Override tmp directory
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tmp: Option<String>,
}

// ── Database connection configuration ───────────────────────────────────

/// SQLite database connection mode.
///
/// # Source
/// Ported from `packages/core/src/database/sqlite.ts`
/// (the dual `Native` / `Drizzle` services).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "lowercase")]
pub enum SqliteMode {
    /// Local file-based SQLite (via `sqlx` or `rusqlite`)
    #[default]
    File,
    /// In-memory SQLite (for testing)
    Memory,
}

/// Database connection configuration.
///
/// # Source
/// Ported from `packages/core/src/database/database.ts` lines 22–37
/// (`layer` and `layerFromPath`).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DatabaseConfig {
    /// Path to the SQLite database file
    pub filename: String,
    /// Connection mode
    #[serde(default)]
    pub mode: SqliteMode,
    /// WAL journal mode (default: true)
    #[serde(default = "default_true")]
    pub wal: bool,
    /// Foreign keys enforcement (default: true)
    #[serde(default = "default_true")]
    pub foreign_keys: bool,
    /// Busy timeout in milliseconds
    #[serde(default = "default_busy_timeout")]
    pub busy_timeout: u32,
    /// Cache size in KB (negative means pages)
    #[serde(default = "default_cache_size")]
    pub cache_size: i32,
}

const fn default_true() -> bool {
    true
}
const fn default_busy_timeout() -> u32 {
    5000
}
const fn default_cache_size() -> i32 {
    -64000
}

impl Default for DatabaseConfig {
    fn default() -> Self {
        let paths = GlobalPaths::default();
        Self {
            filename: paths.database_path(None, Some("latest")),
            mode: SqliteMode::File,
            wal: true,
            foreign_keys: true,
            busy_timeout: 5000,
            cache_size: -64000,
        }
    }
}

impl DatabaseConfig {
    /// In-memory database (for tests).
    pub fn memory() -> Self {
        Self {
            filename: ":memory:".to_string(),
            mode: SqliteMode::Memory,
            ..Default::default()
        }
    }

    /// Build the PRAGMA statements to apply on connection open.
    pub fn pragmas(&self) -> Vec<String> {
        let mut pragmas = Vec::new();
        if self.wal {
            pragmas.push("PRAGMA journal_mode = WAL".to_string());
        }
        pragmas.push(format!("PRAGMA busy_timeout = {}", self.busy_timeout));
        pragmas.push(format!("PRAGMA cache_size = {}", self.cache_size));
        if self.foreign_keys {
            pragmas.push("PRAGMA foreign_keys = ON".to_string());
        }
        pragmas.push("PRAGMA synchronous = NORMAL".to_string());
        pragmas.push("PRAGMA wal_checkpoint(PASSIVE)".to_string());
        pragmas
    }
}

// ── Timestamp column helpers ────────────────────────────────────────────

/// Standard timestamp column configuration for SQLite.
///
/// # Source
/// Ported from `packages/core/src/database/schema.sql.ts` lines 1–10
/// (`Timestamps = { time_created, time_updated }`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct Timestamps {
    /// Creation timestamp (epoch milliseconds)
    pub time_created: i64,
    /// Last update timestamp (epoch milliseconds)
    pub time_updated: i64,
}

impl Timestamps {
    /// Create a new Timestamps record with `now` for both fields.
    pub fn now() -> Self {
        let now = chrono::Utc::now().timestamp_millis();
        Self {
            time_created: now,
            time_updated: now,
        }
    }

    /// Create with explicit timestamps.
    pub fn new(time_created: i64, time_updated: i64) -> Self {
        Self {
            time_created,
            time_updated,
        }
    }

    /// Touch — update `time_updated` to now.
    pub fn touch(&mut self) {
        self.time_updated = chrono::Utc::now().timestamp_millis();
    }
}

// ── Migration types ─────────────────────────────────────────────────────

/// A database migration with an ID and an up-migration SQL.
///
/// # Source
/// Ported from `packages/core/src/database/migration.ts` lines 13–16
/// (`Migration` type — `{ id: string, up: (tx: Transaction) => Effect }`).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Migration {
    /// Unique migration identifier (timestamp-based)
    pub id: String,
    /// SQL statements to apply this migration
    pub up: Vec<String>,
}

/// Metadata tracking which migrations have been applied.
///
/// # Source
/// Ported from `packages/core/src/database/migration.ts` line 30
/// (the `migration` table — `id TEXT PRIMARY KEY, time_completed INTEGER NOT NULL`).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MigrationMeta {
    /// Migration ID that was applied
    pub id: String,
    /// Timestamp when it was applied (epoch millis)
    pub time_completed: i64,
}

/// The set of all known migrations, in dependency order.
///
/// # Source
/// Ported from `packages/core/src/database/migration.gen.ts`
/// (the `migrations` array — 35 migrations).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MigrationSet {
    /// All migrations in order
    pub migrations: Vec<Migration>,
}

impl MigrationSet {
    /// Return the IDs of all migrations.
    pub fn ids(&self) -> Vec<&str> {
        self.migrations.iter().map(|m| m.id.as_str()).collect()
    }

    /// Return the number of migrations.
    pub fn len(&self) -> usize {
        self.migrations.len()
    }

    /// Returns true if there are no migrations.
    pub fn is_empty(&self) -> bool {
        self.migrations.is_empty()
    }
}

// ── SQL Table definitions ───────────────────────────────────────────────

/// Named SQL table with its CREATE TABLE statement.
///
/// # Source
/// Ported from `packages/core/src/database/schema.gen.ts`
/// (the 18+ CREATE TABLE statements).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TableDef {
    /// Table name
    pub name: String,
    /// The CREATE TABLE SQL statement
    pub sql: String,
    /// Optional associated CREATE INDEX statements
    pub indexes: Vec<String>,
}

// ── SQL constants for every table ───────────────────────────────────────

/// SQL to create the `workspace` table.
///
/// # Source
/// Ported from `packages/core/src/database/schema.gen.ts` lines 8–18.
pub const CREATE_TABLE_WORKSPACE: &str = r#"
CREATE TABLE `workspace` (
  `id` text PRIMARY KEY,
  `type` text NOT NULL,
  `name` text DEFAULT '' NOT NULL,
  `branch` text,
  `directory` text,
  `extra` text,
  `project_id` text NOT NULL,
  `time_used` integer NOT NULL,
  CONSTRAINT `fk_workspace_project_id_project_id_fk` FOREIGN KEY (`project_id`) REFERENCES `project`(`id`) ON DELETE CASCADE
);
"#;

/// SQL to create the `data_migration` table.
///
/// # Source
/// Ported from `packages/core/src/schema.gen.ts` lines 20–24.
pub const CREATE_TABLE_DATA_MIGRATION: &str = r#"
CREATE TABLE `data_migration` (
  `name` text PRIMARY KEY,
  `time_completed` integer NOT NULL
);
"#;

/// SQL to create the `account_state` table.
///
/// # Source
/// Ported from `packages/core/src/database/schema.gen.ts` lines 27–33.
pub const CREATE_TABLE_ACCOUNT_STATE: &str = r#"
CREATE TABLE `account_state` (
  `id` integer PRIMARY KEY,
  `active_account_id` text,
  `active_org_id` text,
  CONSTRAINT `fk_account_state_active_account_id_account_id_fk` FOREIGN KEY (`active_account_id`) REFERENCES `account`(`id`) ON DELETE SET NULL
);
"#;

/// SQL to create the `account` table.
///
/// # Source
/// Ported from `packages/core/src/database/schema.gen.ts` lines 35–45.
pub const CREATE_TABLE_ACCOUNT: &str = r#"
CREATE TABLE `account` (
  `id` text PRIMARY KEY,
  `email` text NOT NULL,
  `url` text NOT NULL,
  `access_token` text NOT NULL,
  `refresh_token` text NOT NULL,
  `token_expiry` integer,
  `time_created` integer NOT NULL,
  `time_updated` integer NOT NULL
);
"#;

/// SQL to create the `control_account` table.
///
/// # Source
/// Ported from `packages/core/src/database/schema.gen.ts` lines 47–58.
pub const CREATE_TABLE_CONTROL_ACCOUNT: &str = r#"
CREATE TABLE `control_account` (
  `email` text NOT NULL,
  `url` text NOT NULL,
  `access_token` text NOT NULL,
  `refresh_token` text NOT NULL,
  `token_expiry` integer,
  `active` integer NOT NULL,
  `time_created` integer NOT NULL,
  `time_updated` integer NOT NULL,
  CONSTRAINT `control_account_pk` PRIMARY KEY(`email`, `url`)
);
"#;

/// SQL to create the `credential` table.
///
/// # Source
/// Ported from `packages/core/src/database/schema.gen.ts` lines 60–72.
pub const CREATE_TABLE_CREDENTIAL: &str = r#"
CREATE TABLE `credential` (
  `id` text PRIMARY KEY,
  `integration_id` text,
  `label` text NOT NULL,
  `value` text NOT NULL,
  `connector_id` text,
  `method_id` text,
  `active` integer,
  `time_created` integer NOT NULL,
  `time_updated` integer NOT NULL
);
"#;

/// SQL to create the `event_sequence` table.
///
/// # Source
/// Ported from `packages/core/src/database/schema.gen.ts` lines 74–77.
pub const CREATE_TABLE_EVENT_SEQUENCE: &str = r#"
CREATE TABLE `event_sequence` (
  `aggregate_id` text PRIMARY KEY,
  `seq` integer NOT NULL,
  `owner_id` text
);
"#;

/// SQL to create the `event` table.
///
/// # Source
/// Ported from `packages/core/src/database/schema.gen.ts` lines 79–87.
pub const CREATE_TABLE_EVENT: &str = r#"
CREATE TABLE `event` (
  `id` text PRIMARY KEY,
  `aggregate_id` text NOT NULL,
  `seq` integer NOT NULL,
  `type` text NOT NULL,
  `data` text NOT NULL,
  CONSTRAINT `fk_event_aggregate_id_event_sequence_aggregate_id_fk` FOREIGN KEY (`aggregate_id`) REFERENCES `event_sequence`(`aggregate_id`) ON DELETE CASCADE
);
"#;

/// SQL to create the `permission` table.
///
/// # Source
/// Ported from `packages/core/src/database/schema.gen.ts` lines 89–99.
pub const CREATE_TABLE_PERMISSION: &str = r#"
CREATE TABLE `permission` (
  `id` text PRIMARY KEY,
  `project_id` text NOT NULL,
  `action` text NOT NULL,
  `resource` text NOT NULL,
  `time_created` integer NOT NULL,
  `time_updated` integer NOT NULL,
  CONSTRAINT `fk_permission_project_id_project_id_fk` FOREIGN KEY (`project_id`) REFERENCES `project`(`id`) ON DELETE CASCADE
);
"#;

/// SQL to create the `project_directory` table.
///
/// # Source
/// Ported from `packages/core/src/database/schema.gen.ts` lines 101–109.
pub const CREATE_TABLE_PROJECT_DIRECTORY: &str = r#"
CREATE TABLE `project_directory` (
  `project_id` text NOT NULL,
  `directory` text NOT NULL,
  `type` text,
  `strategy` text,
  `time_created` integer NOT NULL,
  CONSTRAINT `project_directory_pk` PRIMARY KEY(`project_id`, `directory`),
  CONSTRAINT `fk_project_directory_project_id_project_id_fk` FOREIGN KEY (`project_id`) REFERENCES `project`(`id`) ON DELETE CASCADE
);
"#;

/// SQL to create the `project` table.
///
/// # Source
/// Ported from `packages/core/src/database/schema.gen.ts` lines 111–126.
pub const CREATE_TABLE_PROJECT: &str = r#"
CREATE TABLE `project` (
  `id` text PRIMARY KEY,
  `worktree` text NOT NULL,
  `vcs` text,
  `name` text,
  `icon_url` text,
  `icon_url_override` text,
  `icon_color` text,
  `time_created` integer NOT NULL,
  `time_updated` integer NOT NULL,
  `time_initialized` integer,
  `sandboxes` text NOT NULL,
  `commands` text
);
"#;

/// SQL to create the `message` table (legacy).
///
/// # Source
/// Ported from `packages/core/src/database/schema.gen.ts` lines 128–135.
pub const CREATE_TABLE_MESSAGE: &str = r#"
CREATE TABLE `message` (
  `id` text PRIMARY KEY,
  `session_id` text NOT NULL,
  `time_created` integer NOT NULL,
  `time_updated` integer NOT NULL,
  `data` text NOT NULL,
  CONSTRAINT `fk_message_session_id_session_id_fk` FOREIGN KEY (`session_id`) REFERENCES `session`(`id`) ON DELETE CASCADE
);
"#;

/// SQL to create the `part` table (legacy).
///
/// # Source
/// Ported from `packages/core/src/database/schema.gen.ts` lines 137–147.
pub const CREATE_TABLE_PART: &str = r#"
CREATE TABLE `part` (
  `id` text PRIMARY KEY,
  `message_id` text NOT NULL,
  `session_id` text NOT NULL,
  `time_created` integer NOT NULL,
  `time_updated` integer NOT NULL,
  `data` text NOT NULL,
  CONSTRAINT `fk_part_message_id_message_id_fk` FOREIGN KEY (`message_id`) REFERENCES `message`(`id`) ON DELETE CASCADE
);
"#;

/// SQL to create the `session_context_epoch` table.
///
/// # Source
/// Ported from `packages/core/src/database/schema.gen.ts` lines 149–159.
pub const CREATE_TABLE_SESSION_CONTEXT_EPOCH: &str = r#"
CREATE TABLE `session_context_epoch` (
  `session_id` text PRIMARY KEY,
  `baseline` text NOT NULL,
  `agent` text DEFAULT 'build' NOT NULL,
  `snapshot` text NOT NULL,
  `baseline_seq` integer NOT NULL,
  `replacement_seq` integer,
  `revision` integer DEFAULT 0 NOT NULL,
  CONSTRAINT `fk_session_context_epoch_session_id_session_id_fk` FOREIGN KEY (`session_id`) REFERENCES `session`(`id`) ON DELETE CASCADE
);
"#;

/// SQL to create the `session_input` table.
///
/// # Source
/// Ported from `packages/core/src/database/schema.gen.ts` lines 161–171.
pub const CREATE_TABLE_SESSION_INPUT: &str = r#"
CREATE TABLE `session_input` (
  `id` text PRIMARY KEY,
  `session_id` text NOT NULL,
  `prompt` text NOT NULL,
  `delivery` text NOT NULL,
  `admitted_seq` integer NOT NULL,
  `promoted_seq` integer,
  `time_created` integer NOT NULL,
  CONSTRAINT `fk_session_input_session_id_session_id_fk` FOREIGN KEY (`session_id`) REFERENCES `session`(`id`) ON DELETE CASCADE
);
"#;

/// SQL to create the `session_message` table.
///
/// # Source
/// Ported from `packages/core/src/database/schema.gen.ts` lines 173–183.
pub const CREATE_TABLE_SESSION_MESSAGE: &str = r#"
CREATE TABLE `session_message` (
  `id` text PRIMARY KEY,
  `session_id` text NOT NULL,
  `type` text NOT NULL,
  `seq` integer NOT NULL,
  `time_created` integer NOT NULL,
  `time_updated` integer NOT NULL,
  `data` text NOT NULL,
  CONSTRAINT `fk_session_message_session_id_session_id_fk` FOREIGN KEY (`session_id`) REFERENCES `session`(`id`) ON DELETE CASCADE
);
"#;

/// SQL to create the `session` table.
///
/// # Source
/// Ported from `packages/core/src/database/schema.gen.ts` lines 185–217.
pub const CREATE_TABLE_SESSION: &str = r#"
CREATE TABLE `session` (
  `id` text PRIMARY KEY,
  `project_id` text NOT NULL,
  `workspace_id` text,
  `parent_id` text,
  `slug` text NOT NULL,
  `directory` text NOT NULL,
  `path` text,
  `title` text NOT NULL,
  `version` text NOT NULL,
  `share_url` text,
  `summary_additions` integer,
  `summary_deletions` integer,
  `summary_files` integer,
  `summary_diffs` text,
  `metadata` text,
  `cost` real DEFAULT 0 NOT NULL,
  `tokens_input` integer DEFAULT 0 NOT NULL,
  `tokens_output` integer DEFAULT 0 NOT NULL,
  `tokens_reasoning` integer DEFAULT 0 NOT NULL,
  `tokens_cache_read` integer DEFAULT 0 NOT NULL,
  `tokens_cache_write` integer DEFAULT 0 NOT NULL,
  `revert` text,
  `permission` text,
  `agent` text,
  `model` text,
  `time_created` integer NOT NULL,
  `time_updated` integer NOT NULL,
  `time_compacting` integer,
  `time_archived` integer,
  CONSTRAINT `fk_session_project_id_project_id_fk` FOREIGN KEY (`project_id`) REFERENCES `project`(`id`) ON DELETE CASCADE
);
"#;

/// SQL to create the `todo` table.
///
/// # Source
/// Ported from `packages/core/src/database/schema.gen.ts` lines 219–230.
pub const CREATE_TABLE_TODO: &str = r#"
CREATE TABLE `todo` (
  `session_id` text NOT NULL,
  `content` text NOT NULL,
  `status` text NOT NULL,
  `priority` text NOT NULL,
  `position` integer NOT NULL,
  `time_created` integer NOT NULL,
  `time_updated` integer NOT NULL,
  CONSTRAINT `todo_pk` PRIMARY KEY(`session_id`, `position`),
  CONSTRAINT `fk_todo_session_id_session_id_fk` FOREIGN KEY (`session_id`) REFERENCES `session`(`id`) ON DELETE CASCADE
);
"#;

/// SQL to create the `session_share` table.
///
/// # Source
/// Ported from `packages/core/src/database/schema.gen.ts` lines 232–241.
pub const CREATE_TABLE_SESSION_SHARE: &str = r#"
CREATE TABLE `session_share` (
  `session_id` text PRIMARY KEY,
  `id` text NOT NULL,
  `secret` text NOT NULL,
  `url` text NOT NULL,
  `time_created` integer NOT NULL,
  `time_updated` integer NOT NULL,
  CONSTRAINT `fk_session_share_session_id_session_id_fk` FOREIGN KEY (`session_id`) REFERENCES `session`(`id`) ON DELETE CASCADE
);
"#;

/// SQL to create the `migration` journal table.
///
/// # Source
/// Ported from `packages/core/src/database/migration.ts` line 30
/// (the `migration` journal table).
pub const CREATE_TABLE_MIGRATION: &str = r#"
CREATE TABLE `migration` (
  `id` text PRIMARY KEY,
  `time_completed` integer NOT NULL
);
"#;

// ── Index SQL constants ─────────────────────────────────────────────────

/// All CREATE INDEX statements from the canonical schema.
///
/// # Source
/// Ported from `packages/core/src/database/schema.gen.ts` lines 242–274.
pub const CREATE_INDEXES: &[&str] = &[
    "CREATE UNIQUE INDEX `event_aggregate_seq_idx` ON `event` (`aggregate_id`,`seq`);",
    "CREATE INDEX `event_aggregate_type_seq_idx` ON `event` (`aggregate_id`,`type`,`seq`);",
    "CREATE UNIQUE INDEX `permission_project_action_resource_idx` ON `permission` (`project_id`,`action`,`resource`);",
    "CREATE INDEX `message_session_time_created_id_idx` ON `message` (`session_id`,`time_created`,`id`);",
    "CREATE INDEX `part_message_id_id_idx` ON `part` (`message_id`,`id`);",
    "CREATE INDEX `part_session_idx` ON `part` (`session_id`);",
    "CREATE INDEX `session_input_session_pending_delivery_seq_idx` ON `session_input` (`session_id`,`promoted_seq`,`delivery`,`admitted_seq`);",
    "CREATE UNIQUE INDEX `session_input_session_admitted_seq_idx` ON `session_input` (`session_id`,`admitted_seq`);",
    "CREATE UNIQUE INDEX `session_input_session_promoted_seq_idx` ON `session_input` (`session_id`,`promoted_seq`);",
    "CREATE UNIQUE INDEX `session_message_session_seq_idx` ON `session_message` (`session_id`,`seq`);",
    "CREATE INDEX `session_message_session_type_seq_idx` ON `session_message` (`session_id`,`type`,`seq`);",
    "CREATE INDEX `session_message_session_time_created_id_idx` ON `session_message` (`session_id`,`time_created`,`id`);",
    "CREATE INDEX `session_message_time_created_idx` ON `session_message` (`time_created`);",
    "CREATE INDEX `session_project_idx` ON `session` (`project_id`);",
    "CREATE INDEX `session_workspace_idx` ON `session` (`workspace_id`);",
    "CREATE INDEX `session_parent_idx` ON `session` (`parent_id`);",
    "CREATE INDEX `todo_session_idx` ON `todo` (`session_id`);",
];

// ── All table names ─────────────────────────────────────────────────────

/// All table names in the canonical schema.
///
/// # Source
/// Ported from the union of all table names in `schema.gen.ts`.
pub const ALL_TABLE_NAMES: &[&str] = &[
    "workspace",
    "project",
    "project_directory",
    "session",
    "session_message",
    "session_input",
    "session_context_epoch",
    "session_share",
    "message",
    "part",
    "todo",
    "account",
    "control_account",
    "account_state",
    "credential",
    "permission",
    "event",
    "event_sequence",
    "data_migration",
    "migration",
];

/// All CREATE TABLE statements in dependency order.
pub const ALL_CREATE_TABLES: &[&str] = &[
    CREATE_TABLE_PROJECT,
    CREATE_TABLE_WORKSPACE,
    CREATE_TABLE_SESSION,
    CREATE_TABLE_SESSION_MESSAGE,
    CREATE_TABLE_SESSION_INPUT,
    CREATE_TABLE_SESSION_CONTEXT_EPOCH,
    CREATE_TABLE_SESSION_SHARE,
    CREATE_TABLE_MESSAGE,
    CREATE_TABLE_PART,
    CREATE_TABLE_TODO,
    CREATE_TABLE_ACCOUNT,
    CREATE_TABLE_ACCOUNT_STATE,
    CREATE_TABLE_CONTROL_ACCOUNT,
    CREATE_TABLE_CREDENTIAL,
    CREATE_TABLE_PERMISSION,
    CREATE_TABLE_EVENT_SEQUENCE,
    CREATE_TABLE_EVENT,
    CREATE_TABLE_DATA_MIGRATION,
    CREATE_TABLE_PROJECT_DIRECTORY,
    CREATE_TABLE_MIGRATION,
];

// ── Path column types ───────────────────────────────────────────────────

/// Serialize an absolute path for SQLite storage — normalizes to POSIX-style slashes.
///
/// # Source
/// Ported from `packages/core/src/database/path.ts` lines 14–19
/// (`absolute()` — validates and normalizes path).
pub fn db_absolute_path(input: &str) -> Result<String, String> {
    let normalized = if cfg!(windows) {
        input.replace('\\', "/")
    } else {
        input.to_string()
    };
    // Must be absolute on the current platform
    if normalized.starts_with('/') || (cfg!(windows) && is_win_abs(&normalized)) {
        Ok(normalized)
    } else {
        Err(format!("Path is not absolute: {input}"))
    }
}

/// Serialize a path for SQLite storage — normalize slashes only.
///
/// # Source
/// Ported from `packages/core/src/database/path.ts` lines 61–75
/// (`pathColumn` — `storagePath` wrapper).
pub fn db_path(input: &str) -> String {
    if cfg!(windows) {
        input.replace('\\', "/")
    } else {
        input.to_string()
    }
}

/// Serialize an array of absolute paths for SQLite storage (JSON blob).
///
/// # Source
/// Ported from `packages/core/src/database/path.ts` lines 77–91
/// (`absoluteArrayColumn` — JSON-serialized array of absolute paths).
pub fn db_absolute_path_array(paths: &[&str]) -> Result<String, String> {
    let normalized: Result<Vec<String>, String> =
        paths.iter().map(|p| db_absolute_path(p)).collect();
    serde_json::to_string(&normalized?).map_err(|e| format!("JSON serialization error: {e}"))
}

/// Deserialize an array of absolute paths from SQLite storage (JSON parsed).
pub fn db_parse_absolute_path_array(json: &str) -> Result<Vec<String>, String> {
    let paths: Vec<String> =
        serde_json::from_str(json).map_err(|e| format!("JSON parse error: {e}"))?;
    // Re-validate each path
    for path in &paths {
        db_absolute_path(path)?;
    }
    Ok(paths)
}

/// Restore a storage path to the platform-native format.
///
/// On Windows, converts POSIX `/` back to `\` for Windows absolute paths.
/// On Unix, returns the input unchanged.
///
/// # Source
/// Ported from `packages/core/src/database/path.ts` lines 22–25
/// (`toPlatform()`).
pub fn to_platform_path(input: &str) -> String {
    if cfg!(windows) && is_win_abs(input) {
        input.replace('/', "\\")
    } else {
        input.to_string()
    }
}

/// Check if a normalized path is a Windows absolute path.
fn is_win_abs(input: &str) -> bool {
    // Drive letter: `C:/...`
    if input.len() >= 3
        && input.as_bytes()[0].is_ascii_alphabetic()
        && input.as_bytes()[1] == b':'
        && input.as_bytes()[2] == b'/'
    {
        return true;
    }
    // UNC: `//...`
    input.starts_with("//")
}

// ── Known migration IDs ─────────────────────────────────────────────────

/// All known migration IDs from the TS codebase.
///
/// # Source
/// Ported from `packages/core/src/database/migration.gen.ts`
/// (the 35 migration imports).
pub const KNOWN_MIGRATION_IDS: &[&str] = &[
    "20260127222353_familiar_lady_ursula",
    "20260211171708_add_project_commands",
    "20260213144116_wakeful_the_professor",
    "20260225215848_workspace",
    "20260227213759_add_session_workspace_id",
    "20260228203230_blue_harpoon",
    "20260303231226_add_workspace_fields",
    "20260309230000_move_org_to_state",
    "20260312043431_session_message_cursor",
    "20260323234822_events",
    "20260410174513_workspace-name",
    "20260413175956_chief_energizer",
    "20260423070820_add_icon_url_override",
    "20260427172553_slow_nightmare",
    "20260428004200_add_session_path",
    "20260501142318_next_venus",
    "20260504145000_add_sync_owner",
    "20260507164347_add_workspace_time",
    "20260510033149_session_usage",
    "20260511000411_data_migration_state",
    "20260511173437_session-metadata",
    "20260601010001_normalize_storage_paths",
    "20260601202201_amazing_prowler",
    "20260602002951_lowly_union_jack",
    "20260602182828_add_project_directories",
    "20260603001617_session_message_projection_indexes",
    "20260603040000_session_message_projection_order",
    "20260603141458_session_input_inbox",
    "20260603160727_jittery_ezekiel_stane",
    "20260604172448_event_sourced_session_input",
    "20260605003541_add_session_context_snapshot",
    "20260605042240_add_context_epoch_agent",
    "20260611035744_credential",
    "20260611192811_lush_chimera",
    "20260612174303_project_dir_strategy",
];

// ── Typed JSON column helpers ───────────────────────────────────────────

/// Serialize a value to a JSON string for storage in a TEXT column.
pub fn json_column_serialize<T: serde::Serialize>(value: &T) -> Result<String, String> {
    serde_json::to_string(value).map_err(|e| format!("JSON serialize error: {e}"))
}

/// Deserialize a value from a JSON string stored in a TEXT column.
pub fn json_column_deserialize<'a, T: serde::Deserialize<'a>>(json: &'a str) -> Result<T, String> {
    serde_json::from_str(json).map_err(|e| format!("JSON deserialize error: {e}"))
}

/// A JSON column that stores an array of absolute paths with validation.
pub fn json_absolute_path_array_column(paths: &[&str]) -> Result<String, String> {
    let validated: Vec<String> = paths
        .iter()
        .map(|p| db_absolute_path(p))
        .collect::<Result<Vec<_>, _>>()?;
    serde_json::to_string(&validated).map_err(|e| format!("JSON serialize error: {e}"))
}

/// Parse and validate a JSON array of absolute paths from a TEXT column.
pub fn json_parse_absolute_path_array(json: &str) -> Result<Vec<String>, String> {
    let paths: Vec<String> = serde_json::from_str(json).map_err(|e| format!("JSON parse error: {e}"))?;
    for path in &paths {
        db_absolute_path(path)?;
    }
    Ok(paths)
}

/// A typed JSON column wrapper for SQLite storage.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct JsonColumn<T: Clone>(#[serde(bound = "")] pub T);

impl<T: Clone + serde::de::DeserializeOwned> JsonColumn<T> {
    pub fn from_db(raw: &str) -> Result<Self, String> {
        let value: T = serde_json::from_str(raw).map_err(|e| format!("JSON column parse error: {e}"))?;
        Ok(Self(value))
    }

    pub fn to_db(&self) -> Result<String, String> {
        serde_json::to_string(&self.0).map_err(|e| format!("JSON column serialize error: {e}"))
    }
}

// ── Helper: list of tables discovered during migration ──────────────────

/// Returns the table names that signal an existing installation.
///
/// # Source
/// Ported from `packages/core/src/database/migration.ts` line 24
/// (`if (tables.some((table) => table.name === "session"))`).
pub fn is_existing_install(tables: &[&str]) -> bool {
    tables.contains(&"session")
}

/// Detect a fresh install — no tables at all.
///
/// When the database has no tables, the migration system can skip the
/// Drizzle journal import and go straight to creating the schema + journal.
///
/// # Source
/// Ported from `packages/core/src/database/migration.ts` lines 25–26
/// (`if (tables.length > 0) return Effect.die(...)`) inverted logic.
pub fn is_fresh_install(tables: &[&str]) -> bool {
    tables.is_empty()
}

/// Import existing Drizzle migration names into the `migration` journal.
///
/// Existing installs used Drizzle's migration journal (`__drizzle_migrations`).
/// This function seeds the new `migration` table once so TypeScript migrations
/// don't replay old SQL.
///
/// # Source
/// Ported from `packages/core/src/database/migration.ts` lines 54–66.
///
/// Returns the set of completed migration IDs after the import.
pub async fn import_drizzle_journal(
    db: &sqlx::SqlitePool,
) -> Result<std::collections::HashSet<String>, String> {
    // Check if the drizzle journal table exists
    let has_drizzle: bool = sqlx::query_scalar::<_, i64>(
        "SELECT COUNT(*) FROM sqlite_master WHERE type = 'table' AND name = '__drizzle_migrations'",
    )
    .fetch_one(db)
    .await
    .map_err(|e| format!("check drizzle table: {e}"))? > 0;

    if !has_drizzle {
        // No drizzle journal — just return current migration set
        let completed: Vec<(String,)> = sqlx::query_as("SELECT id FROM migration")
            .fetch_all(db)
            .await
            .map_err(|e| format!("read migration journal: {e}"))?;
        return Ok(completed.into_iter().map(|(id,)| id).collect());
    }

    // Import drizzle migration names into our journal
    let now = chrono::Utc::now().timestamp_millis();
    sqlx::query(
        "INSERT OR IGNORE INTO migration (id, time_completed) \
         SELECT name, ?1 FROM __drizzle_migrations WHERE name IS NOT NULL",
    )
    .bind(now)
    .execute(db)
    .await
    .map_err(|e| format!("import drizzle migrations: {e}"))?;

    // Re-read the completed set
    let completed: Vec<(String,)> = sqlx::query_as("SELECT id FROM migration")
        .fetch_all(db)
        .await
        .map_err(|e| format!("re-read migration journal: {e}"))?;
    Ok(completed.into_iter().map(|(id,)| id).collect())
}

// ── Database service — session/message/part CRUD helpers ─────────────────

/// Error type for database service operations.
///
/// # Source
/// Ported from error handling patterns in the TS database layer.
#[derive(Debug, thiserror::Error)]
pub enum DatabaseServiceError {
    /// A database query or execution error.
    #[error("database error: {0}")]
    Database(String),

    /// The requested entity was not found.
    #[error("not found: {0}")]
    NotFound(String),

    /// A constraint violation occurred.
    #[error("constraint violation: {0}")]
    ConstraintViolation(String),
}

/// Structured session update — replaces the 19-parameter `update_session`.
///
/// All fields are optional. `None` means "don't update", `Some(value)` updates.
#[derive(Debug, Clone, Default)]
pub struct SessionUpdateFields {
    pub time_updated: Option<i64>,
    pub title: Option<String>,
    pub cost: Option<f64>,
    pub tokens_input: Option<i64>,
    pub tokens_output: Option<i64>,
    pub tokens_reasoning: Option<i64>,
    pub tokens_cache_read: Option<i64>,
    pub tokens_cache_write: Option<i64>,
    pub share_url: Option<String>,
    pub summary_additions: Option<i64>,
    pub summary_deletions: Option<i64>,
    pub summary_files: Option<i64>,
    pub summary_diffs: Option<String>,
    pub metadata: Option<String>,
    pub revert: Option<String>,
    pub permission: Option<String>,
    pub time_compacting: Option<i64>,
    pub time_archived: Option<i64>,
}

/// High-level database service providing CRUD operations for core tables.
///
/// Wraps a `sqlx::SqlitePool` (obtained from `crate::storage::Database::pool()`)
/// and provides typed INSERT, UPDATE, DELETE, and SELECT helpers for the
/// session, message, and part tables.
///
/// # Source
/// Ported from the drizzle-orm query patterns in the TS codebase.
#[derive(Clone)]
pub struct DatabaseService {
    pool: sqlx::SqlitePool,
}

impl DatabaseService {
    /// Create a new DatabaseService from an existing pool.
    pub fn new(pool: sqlx::SqlitePool) -> Self {
        Self { pool }
    }

    /// Get a reference to the connection pool.
    pub fn pool(&self) -> &sqlx::SqlitePool {
        &self.pool
    }

    /// Begin a new database transaction.
    ///
    /// Returns `None` if the pool is closed or an error occurs.
    pub async fn begin(&self) -> Result<sqlx::Transaction<'_, sqlx::Sqlite>, DatabaseServiceError> {
        self.pool.begin().await.map_err(|e| DatabaseServiceError::Database(e.to_string()))
    }

    // ── Migration status ─────────────────────────────────────────────

    /// Query the migration journal and return the list of applied migrations.
    ///
    /// # Source
    /// Ported from `packages/core/src/database/migration.ts` lines 43–51
    /// (`applyOnly` — reading `SELECT id FROM migration`).
    pub async fn migration_status(&self) -> Result<Vec<MigrationMeta>, DatabaseServiceError> {
        let rows: Vec<(String, i64)> =
            sqlx::query_as("SELECT id, time_completed FROM migration ORDER BY time_completed")
                .fetch_all(&self.pool)
                .await
                .map_err(|e| {
                    DatabaseServiceError::Database(format!("migration status query: {e}"))
                })?;

        Ok(rows
            .into_iter()
            .map(|(id, time_completed)| MigrationMeta { id, time_completed })
            .collect())
    }

    /// Check whether a specific migration has been applied.
    pub async fn is_migration_applied(
        &self,
        migration_id: &str,
    ) -> Result<bool, DatabaseServiceError> {
        let result: Option<(String,)> = sqlx::query_as("SELECT id FROM migration WHERE id = ?1")
            .bind(migration_id)
            .fetch_optional(&self.pool)
            .await
            .map_err(|e| DatabaseServiceError::Database(format!("migration check: {e}")))?;

        Ok(result.is_some())
    }

    /// Return the count of applied migrations.
    pub async fn migration_count(&self) -> Result<i64, DatabaseServiceError> {
        let (count,): (i64,) = sqlx::query_as("SELECT COUNT(*) FROM migration")
            .fetch_one(&self.pool)
            .await
            .map_err(|e| DatabaseServiceError::Database(format!("migration count: {e}")))?;

        Ok(count)
    }

    // ── Session CRUD ─────────────────────────────────────────────────

    /// Insert a new session row.
    #[allow(clippy::too_many_arguments)]
    pub async fn insert_session(
        &self,
        id: &str,
        project_id: &str,
        workspace_id: Option<&str>,
        parent_id: Option<&str>,
        slug: &str,
        directory: &str,
        path: Option<&str>,
        title: &str,
        version: &str,
        time_created: i64,
        time_updated: i64,
        agent: Option<&str>,
        model: Option<&str>,
        cost: Option<f64>,
        tokens_input: Option<i64>,
        tokens_output: Option<i64>,
    ) -> Result<(), DatabaseServiceError> {
        sqlx::query(
            "INSERT INTO session (id, project_id, workspace_id, parent_id, slug, directory, path, title, version, time_created, time_updated, agent, model, cost, tokens_input, tokens_output)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14, ?15, ?16)",
        )
        .bind(id)
        .bind(project_id)
        .bind(workspace_id)
        .bind(parent_id)
        .bind(slug)
        .bind(directory)
        .bind(path)
        .bind(title)
        .bind(version)
        .bind(time_created)
        .bind(time_updated)
        .bind(agent)
        .bind(model)
        .bind(cost)
        .bind(tokens_input)
        .bind(tokens_output)
        .execute(&self.pool)
        .await
        .map_err(|e| DatabaseServiceError::Database(format!("insert session: {e}")))?;

        Ok(())
    }

    /// Update a session using a structured [`SessionUpdateFields`] struct.
    ///
    /// Preferred over the 19-parameter `update_session` for new code.
    pub async fn update_session_fields(
        &self,
        id: &str,
        fields: &SessionUpdateFields,
    ) -> Result<(), DatabaseServiceError> {
        sqlx::query(
            "UPDATE session SET time_updated = COALESCE(?2, time_updated),
             title = COALESCE(?3, title), cost = COALESCE(?4, cost),
             tokens_input = COALESCE(?5, tokens_input),
             tokens_output = COALESCE(?6, tokens_output),
             tokens_reasoning = COALESCE(?7, tokens_reasoning),
             tokens_cache_read = COALESCE(?8, tokens_cache_read),
             tokens_cache_write = COALESCE(?9, tokens_cache_write),
             share_url = COALESCE(?10, share_url),
             summary_additions = COALESCE(?11, summary_additions),
             summary_deletions = COALESCE(?12, summary_deletions),
             summary_files = COALESCE(?13, summary_files),
             summary_diffs = COALESCE(?14, summary_diffs),
             metadata = COALESCE(?15, metadata),
             revert = COALESCE(?16, revert),
             permission = COALESCE(?17, permission),
             time_compacting = COALESCE(?18, time_compacting),
             time_archived = COALESCE(?19, time_archived)
             WHERE id = ?1",
        )
        .bind(id)
        .bind(fields.time_updated)
        .bind(fields.title.as_deref())
        .bind(fields.cost)
        .bind(fields.tokens_input)
        .bind(fields.tokens_output)
        .bind(fields.tokens_reasoning)
        .bind(fields.tokens_cache_read)
        .bind(fields.tokens_cache_write)
        .bind(fields.share_url.as_deref())
        .bind(fields.summary_additions)
        .bind(fields.summary_deletions)
        .bind(fields.summary_files)
        .bind(fields.summary_diffs.as_deref())
        .bind(fields.metadata.as_deref())
        .bind(fields.revert.as_deref())
        .bind(fields.permission.as_deref())
        .bind(fields.time_compacting)
        .bind(fields.time_archived)
        .execute(&self.pool)
        .await
        .map_err(|e| DatabaseServiceError::Database(format!("update session fields: {e}")))?;
        Ok(())
    }

    /// Update a session's time_updated and optional fields.
    ///
    /// Supports all mutable session columns. Only non-None optional fields
    /// are updated.
    #[allow(clippy::too_many_arguments)]
    pub async fn update_session(
        &self,
        id: &str,
        time_updated: i64,
        title: Option<&str>,
        cost: Option<f64>,
        tokens_input: Option<i64>,
        tokens_output: Option<i64>,
        tokens_reasoning: Option<i64>,
        tokens_cache_read: Option<i64>,
        tokens_cache_write: Option<i64>,
        share_url: Option<&str>,
        summary_additions: Option<i64>,
        summary_deletions: Option<i64>,
        summary_files: Option<i64>,
        summary_diffs: Option<&str>,
        metadata: Option<&str>,
        revert: Option<&str>,
        permission: Option<&str>,
        time_compacting: Option<i64>,
        time_archived: Option<i64>,
    ) -> Result<(), DatabaseServiceError> {
        sqlx::query(
            "UPDATE session SET time_updated = ?2, title = COALESCE(?3, title),
             cost = COALESCE(?4, cost), tokens_input = COALESCE(?5, tokens_input),
             tokens_output = COALESCE(?6, tokens_output),
             tokens_reasoning = COALESCE(?7, tokens_reasoning),
             tokens_cache_read = COALESCE(?8, tokens_cache_read),
             tokens_cache_write = COALESCE(?9, tokens_cache_write),
             share_url = COALESCE(?10, share_url),
             summary_additions = COALESCE(?11, summary_additions),
             summary_deletions = COALESCE(?12, summary_deletions),
             summary_files = COALESCE(?13, summary_files),
             summary_diffs = COALESCE(?14, summary_diffs),
             metadata = COALESCE(?15, metadata),
             revert = COALESCE(?16, revert),
             permission = COALESCE(?17, permission),
             time_compacting = COALESCE(?18, time_compacting),
             time_archived = COALESCE(?19, time_archived)
             WHERE id = ?1",
        )
        .bind(id)
        .bind(time_updated)
        .bind(title)
        .bind(cost)
        .bind(tokens_input)
        .bind(tokens_output)
        .bind(tokens_reasoning)
        .bind(tokens_cache_read)
        .bind(tokens_cache_write)
        .bind(share_url)
        .bind(summary_additions)
        .bind(summary_deletions)
        .bind(summary_files)
        .bind(summary_diffs)
        .bind(metadata)
        .bind(revert)
        .bind(permission)
        .bind(time_compacting)
        .bind(time_archived)
        .execute(&self.pool)
        .await
        .map_err(|e| DatabaseServiceError::Database(format!("update session: {e}")))?;

        Ok(())
    }

    // ── List sessions globally (across projects) ─────────────────────
    /// List all sessions with optional filters.
    ///
    /// Supports directory, search, roots, cursor, archived, and limit filters.
    pub async fn list_sessions_global(
        &self,
        directory: Option<&str>,
        search: Option<&str>,
        roots: Option<bool>,
        cursor: Option<i64>,
        archived: Option<bool>,
        limit: Option<u32>,
    ) -> Result<Vec<SessionRow>, DatabaseServiceError> {
        let limit = limit.unwrap_or(100) as i64;
        let mut conditions: Vec<String> = Vec::new();
        let mut next_bind = 2u32;

        if let Some(_dir) = directory {
            conditions.push(format!("directory = ?{next_bind}"));
            next_bind += 1;
        }
        if roots.unwrap_or(false) {
            conditions.push("parent_id IS NULL".to_string());
        }
        if let Some(_c) = cursor {
            conditions.push(format!("time_updated < ?{next_bind}"));
            next_bind += 1;
        }
        // Default: exclude archived unless explicitly included
        if !archived.unwrap_or(false) {
            conditions.push("time_archived IS NULL".to_string());
        }
        if let Some(_s) = search {
            conditions.push(format!("title LIKE ?{next_bind}"));
            next_bind += 1;
        }

        let where_clause = if conditions.is_empty() {
            String::new()
        } else {
            format!("WHERE {}", conditions.join(" AND "))
        };

        let sql = format!(
            "SELECT id, project_id, workspace_id, parent_id, slug, directory, path, title, version, \
             share_url, summary_additions, summary_deletions, summary_files, summary_diffs, \
             metadata, cost, tokens_input, tokens_output, tokens_reasoning, tokens_cache_read, \
             tokens_cache_write, revert, permission, agent, model, \
             time_created, time_updated, time_compacting, time_archived \
             FROM session {} ORDER BY time_updated DESC, id DESC LIMIT ?1",
            where_clause
        );

        let mut query = sqlx::query_as::<_, SessionRowRaw>(&sql).bind(limit);
        if let Some(dir) = directory {
            query = query.bind(dir);
        }
        if let Some(c) = cursor {
            query = query.bind(c);
        }
        if let Some(s) = search {
            query = query.bind(format!("%{s}%"));
        }

        let rows = query
            .fetch_all(&self.pool)
            .await
            .map_err(|e| DatabaseServiceError::Database(format!("list sessions global: {{e}}")))?;

        Ok(rows.into_iter().map(SessionRowRaw::into_row).collect())
    }

    // ── List child sessions ───────────────────────────────────────────
    /// List sessions with a given parent_id.
    pub async fn list_child_sessions(
        &self,
        parent_id: &str,
    ) -> Result<Vec<SessionRow>, DatabaseServiceError> {
        let rows: Vec<SessionRowRaw> = sqlx::query_as(
            "SELECT id, project_id, workspace_id, parent_id, slug, directory, path, title, version, \
             share_url, summary_additions, summary_deletions, summary_files, summary_diffs, \
             metadata, cost, tokens_input, tokens_output, tokens_reasoning, tokens_cache_read, \
             tokens_cache_write, revert, permission, agent, model, \
             time_created, time_updated, time_compacting, time_archived \
             FROM session WHERE parent_id = ?1 ORDER BY time_updated DESC",
        )
        .bind(parent_id)
        .fetch_all(&self.pool)
        .await
        .map_err(|e| DatabaseServiceError::Database(format!("list child sessions: {{e}}")))?;

        Ok(rows.into_iter().map(SessionRowRaw::into_row).collect())
    }

    // ── Get part by ID ────────────────────────────────────────────────
    /// Get a single part by its ID.
    pub async fn get_part_by_id(
        &self,
        part_id: &str,
    ) -> Result<Option<PartRow>, DatabaseServiceError> {
        #[derive(Debug, sqlx::FromRow)]
        struct PartRowQuery {
            id: String,
            message_id: String,
            session_id: String,
            data: String,
            time_created: i64,
            time_updated: i64,
        }

        let row: Option<PartRowQuery> = sqlx::query_as(
            "SELECT id, message_id, session_id, data, time_created, time_updated \
             FROM part WHERE id = ?1",
        )
        .bind(part_id)
        .fetch_optional(&self.pool)
        .await
        .map_err(|e| DatabaseServiceError::Database(format!("get part by id: {{e}}")))?;

        Ok(row.map(|r| PartRow {
            id: r.id,
            message_id: r.message_id,
            session_id: r.session_id,
            data: r.data,
            time_created: r.time_created,
            time_updated: r.time_updated,
        }))
    }

    // ── Update session workspace_id ───────────────────────────────────
    /// Update a session's workspace_id.
    pub async fn update_session_workspace(
        &self,
        id: &str,
        workspace_id: Option<&str>,
    ) -> Result<(), DatabaseServiceError> {
        let now = chrono::Utc::now().timestamp_millis();
        sqlx::query("UPDATE session SET workspace_id = ?2, time_updated = ?3 WHERE id = ?1")
            .bind(id)
            .bind(workspace_id)
            .bind(now)
            .execute(&self.pool)
            .await
            .map_err(|e| DatabaseServiceError::Database(format!("update session workspace: {{e}}")))?;

        Ok(())
    }

    /// Delete a session by ID.
    pub async fn delete_session(&self, id: &str) -> Result<(), DatabaseServiceError> {
        let rows = sqlx::query("DELETE FROM session WHERE id = ?1")
            .bind(id)
            .execute(&self.pool)
            .await
            .map_err(|e| DatabaseServiceError::Database(format!("delete session: {e}")))?;

        if rows.rows_affected() == 0 {
            return Err(DatabaseServiceError::NotFound(format!("session {id}")));
        }
        Ok(())
    }

    /// Query sessions for a project, ordered by most recently updated.
    pub async fn list_sessions(
        &self,
        project_id: &str,
        limit: Option<u32>,
    ) -> Result<Vec<SessionRow>, DatabaseServiceError> {
        let limit = limit.unwrap_or(50) as i64;
        let rows: Vec<SessionRowRaw> = sqlx::query_as(
            "SELECT id, project_id, workspace_id, parent_id, slug, directory, path, title, version, \
             share_url, summary_additions, summary_deletions, summary_files, summary_diffs, \
             metadata, cost, tokens_input, tokens_output, tokens_reasoning, tokens_cache_read, \
             tokens_cache_write, revert, permission, agent, model, \
             time_created, time_updated, time_compacting, time_archived \
             FROM session WHERE project_id = ?1 ORDER BY time_updated DESC LIMIT ?2",
        )
        .bind(project_id)
        .bind(limit)
        .fetch_all(&self.pool)
        .await
        .map_err(|e| DatabaseServiceError::Database(format!("list sessions: {e}")))?;

        Ok(rows.into_iter().map(SessionRowRaw::into_row).collect())
    }

    // ── Message CRUD (legacy) ────────────────────────────────────────

    /// Insert a message record.
    pub async fn insert_message(
        &self,
        id: &str,
        session_id: &str,
        data: &str,
        time_created: i64,
        time_updated: i64,
    ) -> Result<(), DatabaseServiceError> {
        sqlx::query(
            "INSERT INTO message (id, session_id, data, time_created, time_updated) VALUES (?1, ?2, ?3, ?4, ?5)",
        )
        .bind(id)
        .bind(session_id)
        .bind(data)
        .bind(time_created)
        .bind(time_updated)
        .execute(&self.pool)
        .await
        .map_err(|e| DatabaseServiceError::Database(format!("insert message: {e}")))?;

        Ok(())
    }

    /// Query messages for a session, ordered by time_created.
    pub async fn list_messages(
        &self,
        session_id: &str,
        limit: Option<u32>,
    ) -> Result<Vec<MessageRow>, DatabaseServiceError> {
        let limit = limit.unwrap_or(100) as i64;
        let rows: Vec<MessageRowRaw> = sqlx::query_as(
            "SELECT id, session_id, data, time_created, time_updated
             FROM message WHERE session_id = ?1 ORDER BY time_created ASC LIMIT ?2",
        )
        .bind(session_id)
        .bind(limit)
        .fetch_all(&self.pool)
        .await
        .map_err(|e| DatabaseServiceError::Database(format!("list messages: {e}")))?;

        Ok(rows.into_iter().map(MessageRowRaw::into_row).collect())
    }

    /// Delete a message by ID.
    pub async fn delete_message(&self, id: &str) -> Result<(), DatabaseServiceError> {
        let rows = sqlx::query("DELETE FROM message WHERE id = ?1")
            .bind(id)
            .execute(&self.pool)
            .await
            .map_err(|e| DatabaseServiceError::Database(format!("delete message: {e}")))?;

        if rows.rows_affected() == 0 {
            return Err(DatabaseServiceError::NotFound(format!("message {id}")));
        }
        Ok(())
    }

    // ── Part CRUD (legacy) ───────────────────────────────────────────

    /// Insert a part record.
    pub async fn insert_part(
        &self,
        id: &str,
        message_id: &str,
        session_id: &str,
        data: &str,
        time_created: i64,
        time_updated: i64,
    ) -> Result<(), DatabaseServiceError> {
        sqlx::query(
            "INSERT INTO part (id, message_id, session_id, data, time_created, time_updated) VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
        )
        .bind(id)
        .bind(message_id)
        .bind(session_id)
        .bind(data)
        .bind(time_created)
        .bind(time_updated)
        .execute(&self.pool)
        .await
        .map_err(|e| DatabaseServiceError::Database(format!("insert part: {e}")))?;

        Ok(())
    }

    /// Query parts for a message, ordered by time_created.
    pub async fn list_parts(&self, message_id: &str) -> Result<Vec<PartRow>, DatabaseServiceError> {
        let rows: Vec<PartRowRaw> = sqlx::query_as(
            "SELECT id, message_id, session_id, data, time_created, time_updated
             FROM part WHERE message_id = ?1 ORDER BY time_created ASC",
        )
        .bind(message_id)
        .fetch_all(&self.pool)
        .await
        .map_err(|e| DatabaseServiceError::Database(format!("list parts: {e}")))?;

        Ok(rows.into_iter().map(PartRowRaw::into_row).collect())
    }

    /// Delete parts for a message.
    pub async fn delete_parts_for_message(
        &self,
        message_id: &str,
    ) -> Result<u64, DatabaseServiceError> {
        let rows = sqlx::query("DELETE FROM part WHERE message_id = ?1")
            .bind(message_id)
            .execute(&self.pool)
            .await
            .map_err(|e| DatabaseServiceError::Database(format!("delete parts: {e}")))?;

        Ok(rows.rows_affected())
    }

    // ── Session message CRUD ─────────────────────────────────────────

    /// Insert a session_message record.
    #[allow(clippy::too_many_arguments)]
    pub async fn insert_session_message(
        &self,
        id: &str,
        session_id: &str,
        msg_type: &str,
        seq: i64,
        data: &str,
        time_created: i64,
        time_updated: i64,
    ) -> Result<(), DatabaseServiceError> {
        sqlx::query(
            "INSERT INTO session_message (id, session_id, type, seq, data, time_created, time_updated)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)",
        )
        .bind(id)
        .bind(session_id)
        .bind(msg_type)
        .bind(seq)
        .bind(data)
        .bind(time_created)
        .bind(time_updated)
        .execute(&self.pool)
        .await
        .map_err(|e| DatabaseServiceError::Database(format!("insert session_message: {e}")))?;

        Ok(())
    }

    /// Query session messages ordered by seq.
    pub async fn list_session_messages(
        &self,
        session_id: &str,
        limit: Option<u32>,
    ) -> Result<Vec<SessionMessageRow>, DatabaseServiceError> {
        let limit = limit.unwrap_or(200) as i64;
        let rows: Vec<SessionMessageRowRaw> = sqlx::query_as(
            "SELECT id, session_id, type, seq, data, time_created, time_updated
             FROM session_message WHERE session_id = ?1 ORDER BY seq ASC LIMIT ?2",
        )
        .bind(session_id)
        .bind(limit)
        .fetch_all(&self.pool)
        .await
        .map_err(|e| DatabaseServiceError::Database(format!("list session_messages: {e}")))?;

        Ok(rows
            .into_iter()
            .map(SessionMessageRowRaw::into_row)
            .collect())
    }

    // ── Single session fetch ─────────────────────────────────────────
    /// Get a single session by ID.
    pub async fn get_session(&self, id: &str) -> Result<Option<SessionRow>, DatabaseServiceError> {
        let row: Option<SessionRowRaw> = sqlx::query_as(
            "SELECT id, project_id, workspace_id, parent_id, slug, directory, path, title, version, \
             share_url, summary_additions, summary_deletions, summary_files, summary_diffs, \
             metadata, cost, tokens_input, tokens_output, tokens_reasoning, tokens_cache_read, \
             tokens_cache_write, revert, permission, agent, model, \
             time_created, time_updated, time_compacting, time_archived \
             FROM session WHERE id = ?1",
        )
        .bind(id)
        .fetch_optional(&self.pool)
        .await
        .map_err(|e| DatabaseServiceError::Database(format!("get session: {e}")))?;

        Ok(row.map(SessionRowRaw::into_row))
    }

    // ── Messages with parts (joined query) ──────────────────────────
    /// Get messages for a session, each with its parts.
    ///
    /// Uses a single LEFT JOIN query to avoid N+1.
    /// Ported from: `packages/core/src/database/message.ts`
    pub async fn get_messages_with_parts(
        &self,
        session_id: &str,
        limit: Option<u32>,
    ) -> Result<Vec<(MessageRow, Vec<PartRow>)>, DatabaseServiceError> {
        let limit = limit.unwrap_or(100) as i64;
        // Single query with LEFT JOIN to fetch messages and their parts together
        #[derive(sqlx::FromRow)]
        struct MsgPartRaw {
            id: String,
            session_id: String,
            data: String,
            time_created: i64,
            time_updated: i64,
            part_id: Option<String>,
            part_message_id: Option<String>,
            part_data: Option<String>,
            part_time_created: Option<i64>,
        }

        let rows: Vec<MsgPartRaw> = sqlx::query_as(
            "SELECT m.id, m.session_id, m.data, m.time_created, m.time_updated,
                    p.id AS part_id, p.message_id AS part_message_id,
                    p.data AS part_data, p.time_created AS part_time_created
             FROM message m
             LEFT JOIN part p ON p.message_id = m.id
             WHERE m.session_id = ?1
             ORDER BY m.time_created ASC, p.time_created ASC
             LIMIT ?2"
        )
        .bind(session_id)
        .bind(limit)
        .fetch_all(&self.pool)
        .await
        .map_err(|e| DatabaseServiceError::Database(format!("get messages with parts: {e}")))?;

        // Group parts by message ID, preserving insertion order
        let mut msg_order: Vec<String> = Vec::new();
        let mut msg_map: std::collections::HashMap<String, (MessageRow, Vec<PartRow>)> = std::collections::HashMap::new();
        for row in rows {
            if !msg_map.contains_key(&row.id) {
                let msg = MessageRow {
                    id: row.id.clone(),
                    session_id: row.session_id.clone(),
                    data: row.data,
                    time_created: row.time_created,
                    time_updated: row.time_updated,
                };
                msg_map.insert(row.id.clone(), (msg, Vec::new()));
                msg_order.push(row.id.clone());
            }
            if let (Some(pid), Some(pmid), Some(pdata), Some(ptime)) = (row.part_id, row.part_message_id, row.part_data, row.part_time_created) {
                if let Some((_, parts)) = msg_map.get_mut(&row.id) {
                    parts.push(PartRow {
                        id: pid,
                        message_id: pmid,
                        data: pdata,
                        time_created: ptime,
                    });
                }
            }
        }

        Ok(msg_order.into_iter().filter_map(|id| msg_map.remove(&id)).collect())
    }

    // ── Message v2 (structured JSON data) ────────────────────────────
    /// Insert a message using the new structured schema.
    /// Fields are serialized into the `data` JSON column.
    #[allow(clippy::too_many_arguments)]
    pub async fn insert_message_v2(
        &self,
        id: &str,
        session_id: &str,
        role: &str,
        content: &str,
        model: Option<&str>,
        tokens: Option<i64>,
        cost: Option<f64>,
        error: Option<&str>,
        created_at: i64,
    ) -> Result<(), DatabaseServiceError> {
        let data = serde_json::json!({
            "role": role,
            "content": content,
            "model": model,
            "tokens": tokens,
            "cost": cost,
            "error": error,
        });
        let data_str = serde_json::to_string(&data)
            .map_err(|e| DatabaseServiceError::Database(format!("serialize message v2: {e}")))?;

        let now = chrono::Utc::now().timestamp_millis();
        self.insert_message(id, session_id, &data_str, created_at, now)
            .await
    }

    // ── Part v2 (structured JSON data) ───────────────────────────────
    /// Insert a part using the new structured schema.
    /// Fields are serialized into the `data` JSON column.
    #[allow(clippy::too_many_arguments)]
    pub async fn insert_part_v2(
        &self,
        id: &str,
        message_id: &str,
        session_id: &str,
        part_type: &str,
        content: &str,
        metadata: Option<&serde_json::Value>,
        created_at: i64,
    ) -> Result<(), DatabaseServiceError> {
        let data = serde_json::json!({
            "type": part_type,
            "content": content,
            "metadata": metadata,
        });
        let data_str = serde_json::to_string(&data)
            .map_err(|e| DatabaseServiceError::Database(format!("serialize part v2: {e}")))?;

        let now = chrono::Utc::now().timestamp_millis();
        self.insert_part(id, message_id, session_id, &data_str, created_at, now)
            .await
    }

    // ── Update part data ─────────────────────────────────────────────
    /// Update a part's data JSON blob.
    pub async fn update_part(&self, id: &str, data: &str) -> Result<(), DatabaseServiceError> {
        let now = chrono::Utc::now().timestamp_millis();
        let rows = sqlx::query("UPDATE part SET data = ?2, time_updated = ?3 WHERE id = ?1")
            .bind(id)
            .bind(data)
            .bind(now)
            .execute(&self.pool)
            .await
            .map_err(|e| DatabaseServiceError::Database(format!("update part: {e}")))?;

        if rows.rows_affected() == 0 {
            return Err(DatabaseServiceError::NotFound(format!("part {id}")));
        }
        Ok(())
    }

    // ── Cascade delete ───────────────────────────────────────────────
    /// Delete a session and all related records (child sessions, messages, parts).
    ///
    /// Foreign keys handle the session→message→part cascade automatically.
    /// Child sessions (parent_id) are deleted explicitly since there is no
    /// self-referencing FK with ON DELETE CASCADE.
    pub async fn delete_session_cascade(&self, id: &str) -> Result<(), DatabaseServiceError> {
        // Delete child sessions first (they reference this session via parent_id)
        sqlx::query("DELETE FROM session WHERE parent_id = ?1")
            .bind(id)
            .execute(&self.pool)
            .await
            .map_err(|e| DatabaseServiceError::Database(format!("delete child sessions: {e}")))?;

        // Delete the session itself (cascades to messages, parts via FK)
        let rows = sqlx::query("DELETE FROM session WHERE id = ?1")
            .bind(id)
            .execute(&self.pool)
            .await
            .map_err(|e| DatabaseServiceError::Database(format!("delete session cascade: {e}")))?;

        if rows.rows_affected() == 0 {
            return Err(DatabaseServiceError::NotFound(format!("session {id}")));
        }
        Ok(())
    }

    // ── Context Epoch CRUD ──────────────────────────────────────────────

    /// Upsert a context epoch row for a session.
    ///
    /// # Source
    /// Ported from `packages/core/src/session/context-epoch.ts`
    pub async fn upsert_context_epoch(
        &self,
        session_id: &str,
        baseline: &str,
        agent: &str,
        snapshot: &str,
        baseline_seq: i64,
        replacement_seq: Option<i64>,
        revision: i64,
    ) -> Result<(), DatabaseServiceError> {
        sqlx::query(
            "INSERT INTO session_context_epoch (session_id, baseline, agent, snapshot, baseline_seq, replacement_seq, revision)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)
             ON CONFLICT(session_id) DO UPDATE SET
                baseline = excluded.baseline,
                agent = excluded.agent,
                snapshot = excluded.snapshot,
                baseline_seq = excluded.baseline_seq,
                replacement_seq = excluded.replacement_seq,
                revision = excluded.revision",
        )
        .bind(session_id)
        .bind(baseline)
        .bind(agent)
        .bind(snapshot)
        .bind(baseline_seq)
        .bind(replacement_seq)
        .bind(revision)
        .execute(&self.pool)
        .await
        .map_err(|e| DatabaseServiceError::Database(format!("upsert context epoch: {e}")))?;

        Ok(())
    }

    /// Get the context epoch for a session.
    pub async fn get_context_epoch(
        &self,
        session_id: &str,
    ) -> Result<Option<ContextEpochRow>, DatabaseServiceError> {
        let row = sqlx::query_as::<_, ContextEpochRowRaw>(
            "SELECT session_id, baseline, agent, snapshot, baseline_seq, replacement_seq, revision
             FROM session_context_epoch WHERE session_id = ?1",
        )
        .bind(session_id)
        .fetch_optional(&self.pool)
        .await
        .map_err(|e| DatabaseServiceError::Database(format!("get context epoch: {e}")))?;

        Ok(row.map(ContextEpochRowRaw::into_row))
    }

    /// Delete the context epoch for a session.
    pub async fn delete_context_epoch(
        &self,
        session_id: &str,
    ) -> Result<(), DatabaseServiceError> {
        sqlx::query("DELETE FROM session_context_epoch WHERE session_id = ?1")
            .bind(session_id)
            .execute(&self.pool)
            .await
            .map_err(|e| DatabaseServiceError::Database(format!("delete context epoch: {e}")))?;

        Ok(())
    }

    /// Update the snapshot on a context epoch with revision guard.
    ///
    /// Returns `true` if the update matched a row (revision matched).
    ///
    /// # Source
    /// Ported from `packages/core/src/session/context-epoch.ts` — `advance`.
    pub async fn update_context_epoch_snapshot(
        &self,
        session_id: &str,
        expected_revision: i64,
        snapshot: &str,
    ) -> Result<bool, DatabaseServiceError> {
        let result = sqlx::query(
            "UPDATE session_context_epoch SET snapshot = ?1, revision = ?2 \
             WHERE session_id = ?3 AND revision = ?4 AND replacement_seq IS NULL",
        )
        .bind(snapshot)
        .bind(expected_revision + 1)
        .bind(session_id)
        .bind(expected_revision)
        .execute(&self.pool)
        .await
        .map_err(|e| DatabaseServiceError::Database(format!("update epoch snapshot: {e}")))?;

        Ok(result.rows_affected() > 0)
    }

    /// Replace a context epoch with a new generation (revision guard).
    ///
    /// Returns `true` if the update matched a row (revision matched).
    ///
    /// # Source
    /// Ported from `packages/core/src/session/context-epoch.ts` — `replace`.
    pub async fn replace_context_epoch(
        &self,
        session_id: &str,
        baseline: &str,
        agent: &str,
        snapshot: &str,
        baseline_seq: i64,
        expected_revision: i64,
    ) -> Result<bool, DatabaseServiceError> {
        let result = sqlx::query(
            "UPDATE session_context_epoch SET \
                baseline = ?1, agent = ?2, snapshot = ?3, \
                baseline_seq = ?4, replacement_seq = NULL, revision = ?5 \
             WHERE session_id = ?6 AND revision = ?7",
        )
        .bind(baseline)
        .bind(agent)
        .bind(snapshot)
        .bind(baseline_seq)
        .bind(expected_revision + 1)
        .bind(session_id)
        .bind(expected_revision)
        .execute(&self.pool)
        .await
        .map_err(|e| DatabaseServiceError::Database(format!("replace context epoch: {e}")))?;

        Ok(result.rows_affected() > 0)
    }

    // ── Session Input Inbox CRUD ────────────────────────────────────────

    /// Get the next admitted sequence number for a session.
    ///
    /// Returns the current max admitted_seq + 1, or 1 if no inputs exist.
    ///
    /// # Source
    /// Ported from `packages/core/src/session/input.ts`
    pub async fn get_next_admitted_seq(
        &self,
        session_id: &str,
    ) -> Result<i64, DatabaseServiceError> {
        let row: Option<(Option<i64>,)> = sqlx::query_as(
            "SELECT MAX(admitted_seq) FROM session_input WHERE session_id = ?1",
        )
        .bind(session_id)
        .fetch_optional(&self.pool)
        .await
        .map_err(|e| DatabaseServiceError::Database(format!("get next admitted seq: {e}")))?;

        let max_seq = row.and_then(|r| r.0).unwrap_or(0);
        Ok(max_seq + 1)
    }

    /// Insert a session input record.
    pub async fn insert_session_input(
        &self,
        id: &str,
        session_id: &str,
        prompt: &str,
        delivery: &str,
        admitted_seq: i64,
        time_created: i64,
    ) -> Result<(), DatabaseServiceError> {
        sqlx::query(
            "INSERT INTO session_input (id, session_id, prompt, delivery, admitted_seq, time_created)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
        )
        .bind(id)
        .bind(session_id)
        .bind(prompt)
        .bind(delivery)
        .bind(admitted_seq)
        .bind(time_created)
        .execute(&self.pool)
        .await
        .map_err(|e| DatabaseServiceError::Database(format!("insert session input: {e}")))?;

        Ok(())
    }

    /// List all session inputs for a session, ordered by admitted_seq.
    pub async fn list_session_inputs(
        &self,
        session_id: &str,
    ) -> Result<Vec<SessionInputRow>, DatabaseServiceError> {
        let rows = sqlx::query_as::<_, SessionInputRowRaw>(
            "SELECT id, session_id, prompt, delivery, admitted_seq, promoted_seq, time_created
             FROM session_input WHERE session_id = ?1 ORDER BY admitted_seq ASC",
        )
        .bind(session_id)
        .fetch_all(&self.pool)
        .await
        .map_err(|e| DatabaseServiceError::Database(format!("list session inputs: {e}")))?;

        Ok(rows.into_iter().map(SessionInputRowRaw::into_row).collect())
    }

    /// List pending (non-promoted) inputs for a session.
    pub async fn list_pending_inputs(
        &self,
        session_id: &str,
    ) -> Result<Vec<SessionInputRow>, DatabaseServiceError> {
        let rows = sqlx::query_as::<_, SessionInputRowRaw>(
            "SELECT id, session_id, prompt, delivery, admitted_seq, promoted_seq, time_created
             FROM session_input WHERE session_id = ?1 AND promoted_seq IS NULL ORDER BY admitted_seq ASC",
        )
        .bind(session_id)
        .fetch_all(&self.pool)
        .await
        .map_err(|e| DatabaseServiceError::Database(format!("list pending inputs: {e}")))?;

        Ok(rows.into_iter().map(SessionInputRowRaw::into_row).collect())
    }

    /// Promote an input by setting its promoted_seq.
    pub async fn promote_input(
        &self,
        id: &str,
        promoted_seq: i64,
    ) -> Result<(), DatabaseServiceError> {
        sqlx::query("UPDATE session_input SET promoted_seq = ?2 WHERE id = ?1")
            .bind(id)
            .bind(promoted_seq)
            .execute(&self.pool)
            .await
            .map_err(|e| DatabaseServiceError::Database(format!("promote input: {e}")))?;

        Ok(())
    }

    /// Find a single session input by ID.
    pub async fn find_session_input(
        &self,
        id: &str,
    ) -> Result<Option<SessionInputRow>, DatabaseServiceError> {
        let row = sqlx::query_as::<_, SessionInputRowRaw>(
            "SELECT id, session_id, prompt, delivery, admitted_seq, promoted_seq, time_created \
             FROM session_input WHERE id = ?1",
        )
        .bind(id)
        .fetch_optional(&self.pool)
        .await
        .map_err(|e| DatabaseServiceError::Database(format!("find session input: {e}")))?;

        Ok(row.map(SessionInputRowRaw::into_row))
    }

    // ── Project CRUD ─────────────────────────────────────────────────

    /// List all projects.
    pub async fn list_projects(&self) -> Result<Vec<ProjectRow>, DatabaseServiceError> {
        let rows = sqlx::query_as::<_, ProjectRowRaw>(
            "SELECT id, worktree, vcs, name, icon_url, icon_url_override, icon_color, \
             time_created, time_updated, time_initialized, sandboxes, commands \
             FROM project ORDER BY time_created DESC",
        )
        .fetch_all(&self.pool)
        .await
        .map_err(|e| DatabaseServiceError::Database(format!("list projects: {e}")))?;

        Ok(rows.into_iter().map(ProjectRowRaw::into_row).collect())
    }

    /// Get a single project by ID.
    pub async fn get_project(&self, id: &str) -> Result<Option<ProjectRow>, DatabaseServiceError> {
        let row = sqlx::query_as::<_, ProjectRowRaw>(
            "SELECT id, worktree, vcs, name, icon_url, icon_url_override, icon_color, \
             time_created, time_updated, time_initialized, sandboxes, commands \
             FROM project WHERE id = ?1",
        )
        .bind(id)
        .fetch_optional(&self.pool)
        .await
        .map_err(|e| DatabaseServiceError::Database(format!("get project: {e}")))?;

        Ok(row.map(ProjectRowRaw::into_row))
    }

    /// Insert a new project.
    pub async fn insert_project(
        &self,
        id: &str,
        worktree: &str,
        vcs: Option<&str>,
        name: Option<&str>,
        icon_url: Option<&str>,
        icon_url_override: Option<&str>,
        icon_color: Option<&str>,
        time_created: i64,
        time_updated: i64,
        time_initialized: Option<i64>,
        sandboxes: &str,
        commands: Option<&str>,
    ) -> Result<(), DatabaseServiceError> {
        sqlx::query(
            "INSERT INTO project (id, worktree, vcs, name, icon_url, icon_url_override, icon_color, \
             time_created, time_updated, time_initialized, sandboxes, commands) \
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12)",
        )
        .bind(id)
        .bind(worktree)
        .bind(vcs)
        .bind(name)
        .bind(icon_url)
        .bind(icon_url_override)
        .bind(icon_color)
        .bind(time_created)
        .bind(time_updated)
        .bind(time_initialized)
        .bind(sandboxes)
        .bind(commands)
        .execute(&self.pool)
        .await
        .map_err(|e| DatabaseServiceError::Database(format!("insert project: {e}")))?;

        Ok(())
    }

    /// Update a project's mutable fields.
    pub async fn update_project(
        &self,
        id: &str,
        name: Option<&str>,
        icon_url: Option<&str>,
        icon_url_override: Option<&str>,
        icon_color: Option<&str>,
        vcs: Option<&str>,
        commands: Option<&str>,
    ) -> Result<(), DatabaseServiceError> {
        let now = chrono::Utc::now().timestamp_millis();
        sqlx::query(
            "UPDATE project SET time_updated = ?2, \
             name = COALESCE(?3, name), \
             icon_url = COALESCE(?4, icon_url), \
             icon_url_override = COALESCE(?5, icon_url_override), \
             icon_color = COALESCE(?6, icon_color), \
             vcs = COALESCE(?7, vcs), \
             commands = COALESCE(?8, commands) \
             WHERE id = ?1",
        )
        .bind(id)
        .bind(now)
        .bind(name)
        .bind(icon_url)
        .bind(icon_url_override)
        .bind(icon_color)
        .bind(vcs)
        .bind(commands)
        .execute(&self.pool)
        .await
        .map_err(|e| DatabaseServiceError::Database(format!("update project: {e}")))?;

        Ok(())
    }

    /// Delete a project by ID.
    pub async fn delete_project(&self, id: &str) -> Result<(), DatabaseServiceError> {
        let rows = sqlx::query("DELETE FROM project WHERE id = ?1")
            .bind(id)
            .execute(&self.pool)
            .await
            .map_err(|e| DatabaseServiceError::Database(format!("delete project: {e}")))?;

        if rows.rows_affected() == 0 {
            return Err(DatabaseServiceError::NotFound(format!("project {id}")));
        }
        Ok(())
    }

    // ── Project Directory CRUD ───────────────────────────────────────

    /// List directories for a project.
    pub async fn list_project_directories(
        &self,
        project_id: &str,
    ) -> Result<Vec<ProjectDirectoryRow>, DatabaseServiceError> {
        let rows = sqlx::query_as::<_, ProjectDirectoryRowRaw>(
            "SELECT project_id, directory, type, strategy, time_created \
             FROM project_directory WHERE project_id = ?1 ORDER BY directory ASC",
        )
        .bind(project_id)
        .fetch_all(&self.pool)
        .await
        .map_err(|e| DatabaseServiceError::Database(format!("list project directories: {e}")))?;

        Ok(rows
            .into_iter()
            .map(ProjectDirectoryRowRaw::into_row)
            .collect())
    }

    /// Insert a project directory.
    pub async fn insert_project_directory(
        &self,
        project_id: &str,
        directory: &str,
        dir_type: Option<&str>,
        strategy: Option<&str>,
        time_created: i64,
    ) -> Result<(), DatabaseServiceError> {
        sqlx::query(
            "INSERT INTO project_directory (project_id, directory, type, strategy, time_created) \
             VALUES (?1, ?2, ?3, ?4, ?5)",
        )
        .bind(project_id)
        .bind(directory)
        .bind(dir_type)
        .bind(strategy)
        .bind(time_created)
        .execute(&self.pool)
        .await
        .map_err(|e| DatabaseServiceError::Database(format!("insert project directory: {e}")))?;

        Ok(())
    }

    /// Delete a project directory by project_id and directory.
    pub async fn delete_project_directory(
        &self,
        project_id: &str,
        directory: &str,
    ) -> Result<(), DatabaseServiceError> {
        let rows = sqlx::query(
            "DELETE FROM project_directory WHERE project_id = ?1 AND directory = ?2",
        )
        .bind(project_id)
        .bind(directory)
        .execute(&self.pool)
        .await
        .map_err(|e| DatabaseServiceError::Database(format!("delete project directory: {e}")))?;

        if rows.rows_affected() == 0 {
            return Err(DatabaseServiceError::NotFound(format!(
                "project_directory {project_id}/{directory}"
            )));
        }
        Ok(())
    }

    // ── Account CRUD ─────────────────────────────────────────────────

    /// List all accounts.
    pub async fn list_accounts(&self) -> Result<Vec<AccountRow>, DatabaseServiceError> {
        let rows = sqlx::query_as::<_, AccountRowRaw>(
            "SELECT id, email, url, access_token, refresh_token, token_expiry, \
             time_created, time_updated FROM account ORDER BY time_created DESC",
        )
        .fetch_all(&self.pool)
        .await
        .map_err(|e| DatabaseServiceError::Database(format!("list accounts: {e}")))?;

        Ok(rows.into_iter().map(AccountRowRaw::into_row).collect())
    }

    /// Get a single account by ID.
    pub async fn get_account(&self, id: &str) -> Result<Option<AccountRow>, DatabaseServiceError> {
        let row = sqlx::query_as::<_, AccountRowRaw>(
            "SELECT id, email, url, access_token, refresh_token, token_expiry, \
             time_created, time_updated FROM account WHERE id = ?1",
        )
        .bind(id)
        .fetch_optional(&self.pool)
        .await
        .map_err(|e| DatabaseServiceError::Database(format!("get account: {e}")))?;

        Ok(row.map(AccountRowRaw::into_row))
    }

    /// Insert a new account.
    pub async fn insert_account(
        &self,
        id: &str,
        email: &str,
        url: &str,
        access_token: &str,
        refresh_token: &str,
        token_expiry: Option<i64>,
        time_created: i64,
        time_updated: i64,
    ) -> Result<(), DatabaseServiceError> {
        sqlx::query(
            "INSERT INTO account (id, email, url, access_token, refresh_token, token_expiry, time_created, time_updated) \
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)",
        )
        .bind(id)
        .bind(email)
        .bind(url)
        .bind(access_token)
        .bind(refresh_token)
        .bind(token_expiry)
        .bind(time_created)
        .bind(time_updated)
        .execute(&self.pool)
        .await
        .map_err(|e| DatabaseServiceError::Database(format!("insert account: {e}")))?;

        Ok(())
    }

    /// Update an account's tokens and optional fields.
    pub async fn update_account(
        &self,
        id: &str,
        access_token: Option<&str>,
        refresh_token: Option<&str>,
        token_expiry: Option<i64>,
    ) -> Result<(), DatabaseServiceError> {
        let now = chrono::Utc::now().timestamp_millis();
        sqlx::query(
            "UPDATE account SET time_updated = ?2, \
             access_token = COALESCE(?3, access_token), \
             refresh_token = COALESCE(?4, refresh_token), \
             token_expiry = COALESCE(?5, token_expiry) \
             WHERE id = ?1",
        )
        .bind(id)
        .bind(now)
        .bind(access_token)
        .bind(refresh_token)
        .bind(token_expiry)
        .execute(&self.pool)
        .await
        .map_err(|e| DatabaseServiceError::Database(format!("update account: {e}")))?;

        Ok(())
    }

    /// Delete an account by ID.
    pub async fn delete_account(&self, id: &str) -> Result<(), DatabaseServiceError> {
        let rows = sqlx::query("DELETE FROM account WHERE id = ?1")
            .bind(id)
            .execute(&self.pool)
            .await
            .map_err(|e| DatabaseServiceError::Database(format!("delete account: {e}")))?;

        if rows.rows_affected() == 0 {
            return Err(DatabaseServiceError::NotFound(format!("account {id}")));
        }
        Ok(())
    }

    // ── Account State CRUD (singleton row, id = 1) ───────────────────

    /// Get the active account/organization state.
    pub async fn get_account_state(
        &self,
    ) -> Result<Option<AccountStateRow>, DatabaseServiceError> {
        let row = sqlx::query_as::<_, AccountStateRowRaw>(
            "SELECT id, active_account_id, active_org_id FROM account_state WHERE id = 1",
        )
        .fetch_optional(&self.pool)
        .await
        .map_err(|e| DatabaseServiceError::Database(format!("get account state: {e}")))?;

        Ok(row.map(AccountStateRowRaw::into_row))
    }

    /// Insert or replace the active account state (singleton row).
    pub async fn upsert_account_state(
        &self,
        active_account_id: Option<&str>,
        active_org_id: Option<&str>,
    ) -> Result<(), DatabaseServiceError> {
        sqlx::query(
            "INSERT INTO account_state (id, active_account_id, active_org_id) \
             VALUES (1, ?1, ?2) \
             ON CONFLICT(id) DO UPDATE SET \
                active_account_id = excluded.active_account_id, \
                active_org_id = excluded.active_org_id",
        )
        .bind(active_account_id)
        .bind(active_org_id)
        .execute(&self.pool)
        .await
        .map_err(|e| DatabaseServiceError::Database(format!("upsert account state: {e}")))?;

        Ok(())
    }

    /// Delete the account state row.
    pub async fn delete_account_state(&self) -> Result<(), DatabaseServiceError> {
        sqlx::query("DELETE FROM account_state WHERE id = 1")
            .execute(&self.pool)
            .await
            .map_err(|e| DatabaseServiceError::Database(format!("delete account state: {e}")))?;

        Ok(())
    }

    // ── Control Account CRUD ─────────────────────────────────────────

    /// List all control accounts.
    pub async fn list_control_accounts(
        &self,
    ) -> Result<Vec<ControlAccountRow>, DatabaseServiceError> {
        let rows = sqlx::query_as::<_, ControlAccountRowRaw>(
            "SELECT email, url, access_token, refresh_token, token_expiry, active, \
             time_created, time_updated FROM control_account ORDER BY time_created DESC",
        )
        .fetch_all(&self.pool)
        .await
        .map_err(|e| DatabaseServiceError::Database(format!("list control accounts: {e}")))?;

        Ok(rows
            .into_iter()
            .map(ControlAccountRowRaw::into_row)
            .collect())
    }

    /// Get a control account by email and url.
    pub async fn get_control_account(
        &self,
        email: &str,
        url: &str,
    ) -> Result<Option<ControlAccountRow>, DatabaseServiceError> {
        let row = sqlx::query_as::<_, ControlAccountRowRaw>(
            "SELECT email, url, access_token, refresh_token, token_expiry, active, \
             time_created, time_updated FROM control_account WHERE email = ?1 AND url = ?2",
        )
        .bind(email)
        .bind(url)
        .fetch_optional(&self.pool)
        .await
        .map_err(|e| DatabaseServiceError::Database(format!("get control account: {e}")))?;

        Ok(row.map(ControlAccountRowRaw::into_row))
    }

    /// Insert a new control account.
    pub async fn insert_control_account(
        &self,
        email: &str,
        url: &str,
        access_token: &str,
        refresh_token: &str,
        token_expiry: Option<i64>,
        active: bool,
        time_created: i64,
        time_updated: i64,
    ) -> Result<(), DatabaseServiceError> {
        sqlx::query(
            "INSERT INTO control_account (email, url, access_token, refresh_token, token_expiry, active, time_created, time_updated) \
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)",
        )
        .bind(email)
        .bind(url)
        .bind(access_token)
        .bind(refresh_token)
        .bind(token_expiry)
        .bind(active)
        .bind(time_created)
        .bind(time_updated)
        .execute(&self.pool)
        .await
        .map_err(|e| DatabaseServiceError::Database(format!("insert control account: {e}")))?;

        Ok(())
    }

    /// Update a control account's tokens and active flag.
    pub async fn update_control_account(
        &self,
        email: &str,
        url: &str,
        access_token: Option<&str>,
        refresh_token: Option<&str>,
        token_expiry: Option<i64>,
        active: Option<bool>,
    ) -> Result<(), DatabaseServiceError> {
        let now = chrono::Utc::now().timestamp_millis();
        sqlx::query(
            "UPDATE control_account SET time_updated = ?3, \
             access_token = COALESCE(?4, access_token), \
             refresh_token = COALESCE(?5, refresh_token), \
             token_expiry = COALESCE(?6, token_expiry), \
             active = COALESCE(?7, active) \
             WHERE email = ?1 AND url = ?2",
        )
        .bind(email)
        .bind(url)
        .bind(now)
        .bind(access_token)
        .bind(refresh_token)
        .bind(token_expiry)
        .bind(active)
        .execute(&self.pool)
        .await
        .map_err(|e| DatabaseServiceError::Database(format!("update control account: {e}")))?;

        Ok(())
    }

    /// Delete a control account by email and url.
    pub async fn delete_control_account(
        &self,
        email: &str,
        url: &str,
    ) -> Result<(), DatabaseServiceError> {
        let rows = sqlx::query("DELETE FROM control_account WHERE email = ?1 AND url = ?2")
            .bind(email)
            .bind(url)
            .execute(&self.pool)
            .await
            .map_err(|e| DatabaseServiceError::Database(format!("delete control account: {e}")))?;

        if rows.rows_affected() == 0 {
            return Err(DatabaseServiceError::NotFound(format!(
                "control_account {email}/{url}"
            )));
        }
        Ok(())
    }

    // ── Event CRUD ───────────────────────────────────────────────────

    /// Insert an event record.
    pub async fn insert_event(
        &self,
        id: &str,
        aggregate_id: &str,
        seq: i64,
        event_type: &str,
        data: &str,
    ) -> Result<(), DatabaseServiceError> {
        sqlx::query(
            "INSERT INTO event (id, aggregate_id, seq, type, data) VALUES (?1, ?2, ?3, ?4, ?5)",
        )
        .bind(id)
        .bind(aggregate_id)
        .bind(seq)
        .bind(event_type)
        .bind(data)
        .execute(&self.pool)
        .await
        .map_err(|e| DatabaseServiceError::Database(format!("insert event: {e}")))?;

        Ok(())
    }

    /// List events for an aggregate after a given sequence number.
    pub async fn list_events_after(
        &self,
        aggregate_id: &str,
        after_seq: Option<i64>,
        limit: Option<u32>,
    ) -> Result<Vec<EventRow>, DatabaseServiceError> {
        let limit = limit.unwrap_or(500) as i64;
        let after_seq = after_seq.unwrap_or(0);

        let rows = sqlx::query_as::<_, EventRowRaw>(
            "SELECT id, aggregate_id, seq, type, data \
             FROM event WHERE aggregate_id = ?1 AND seq > ?2 \
             ORDER BY seq ASC LIMIT ?3",
        )
        .bind(aggregate_id)
        .bind(after_seq)
        .bind(limit)
        .fetch_all(&self.pool)
        .await
        .map_err(|e| DatabaseServiceError::Database(format!("list events after: {e}")))?;

        Ok(rows.into_iter().map(EventRowRaw::into_row).collect())
    }

    /// List all events for an aggregate.
    pub async fn list_events_by_aggregate(
        &self,
        aggregate_id: &str,
    ) -> Result<Vec<EventRow>, DatabaseServiceError> {
        let rows = sqlx::query_as::<_, EventRowRaw>(
            "SELECT id, aggregate_id, seq, type, data \
             FROM event WHERE aggregate_id = ?1 ORDER BY seq ASC",
        )
        .bind(aggregate_id)
        .fetch_all(&self.pool)
        .await
        .map_err(|e| DatabaseServiceError::Database(format!("list events by aggregate: {e}")))?;

        Ok(rows.into_iter().map(EventRowRaw::into_row).collect())
    }

    // ── Event Sequence CRUD ──────────────────────────────────────────

    /// Get the event sequence record for an aggregate.
    pub async fn get_event_sequence(
        &self,
        aggregate_id: &str,
    ) -> Result<Option<EventSequenceRow>, DatabaseServiceError> {
        let row = sqlx::query_as::<_, EventSequenceRowRaw>(
            "SELECT aggregate_id, seq, owner_id FROM event_sequence WHERE aggregate_id = ?1",
        )
        .bind(aggregate_id)
        .fetch_optional(&self.pool)
        .await
        .map_err(|e| DatabaseServiceError::Database(format!("get event sequence: {e}")))?;

        Ok(row.map(EventSequenceRowRaw::into_row))
    }

    /// Upsert an event sequence record.
    pub async fn upsert_event_sequence(
        &self,
        aggregate_id: &str,
        seq: i64,
        owner_id: Option<&str>,
    ) -> Result<(), DatabaseServiceError> {
        sqlx::query(
            "INSERT INTO event_sequence (aggregate_id, seq, owner_id) VALUES (?1, ?2, ?3) \
             ON CONFLICT(aggregate_id) DO UPDATE SET \
                seq = excluded.seq, \
                owner_id = COALESCE(excluded.owner_id, event_sequence.owner_id)",
        )
        .bind(aggregate_id)
        .bind(seq)
        .bind(owner_id)
        .execute(&self.pool)
        .await
        .map_err(|e| DatabaseServiceError::Database(format!("upsert event sequence: {e}")))?;

        Ok(())
    }

    // ── Permission CRUD ──────────────────────────────────────────────

    /// List all permissions for a project.
    pub async fn list_permissions(
        &self,
        project_id: &str,
    ) -> Result<Vec<PermissionRow>, DatabaseServiceError> {
        let rows = sqlx::query_as::<_, PermissionRowRaw>(
            "SELECT id, project_id, action, resource, time_created, time_updated \
             FROM permission WHERE project_id = ?1 ORDER BY time_created ASC",
        )
        .bind(project_id)
        .fetch_all(&self.pool)
        .await
        .map_err(|e| DatabaseServiceError::Database(format!("list permissions: {e}")))?;

        Ok(rows.into_iter().map(PermissionRowRaw::into_row).collect())
    }

    /// Get a single permission by ID.
    pub async fn get_permission(
        &self,
        id: &str,
    ) -> Result<Option<PermissionRow>, DatabaseServiceError> {
        let row = sqlx::query_as::<_, PermissionRowRaw>(
            "SELECT id, project_id, action, resource, time_created, time_updated \
             FROM permission WHERE id = ?1",
        )
        .bind(id)
        .fetch_optional(&self.pool)
        .await
        .map_err(|e| DatabaseServiceError::Database(format!("get permission: {e}")))?;

        Ok(row.map(PermissionRowRaw::into_row))
    }

    /// Insert a new permission.
    pub async fn insert_permission(
        &self,
        id: &str,
        project_id: &str,
        action: &str,
        resource: &str,
        time_created: i64,
        time_updated: i64,
    ) -> Result<(), DatabaseServiceError> {
        sqlx::query(
            "INSERT INTO permission (id, project_id, action, resource, time_created, time_updated) \
             VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
        )
        .bind(id)
        .bind(project_id)
        .bind(action)
        .bind(resource)
        .bind(time_created)
        .bind(time_updated)
        .execute(&self.pool)
        .await
        .map_err(|e| DatabaseServiceError::Database(format!("insert permission: {e}")))?;

        Ok(())
    }

    /// Delete a permission by ID.
    pub async fn delete_permission(&self, id: &str) -> Result<(), DatabaseServiceError> {
        let rows = sqlx::query("DELETE FROM permission WHERE id = ?1")
            .bind(id)
            .execute(&self.pool)
            .await
            .map_err(|e| DatabaseServiceError::Database(format!("delete permission: {e}")))?;

        if rows.rows_affected() == 0 {
            return Err(DatabaseServiceError::NotFound(format!("permission {id}")));
        }
        Ok(())
    }

    // ── Workspace CRUD ───────────────────────────────────────────────

    /// List all workspaces for a project.
    pub async fn list_workspaces(
        &self,
        project_id: &str,
    ) -> Result<Vec<WorkspaceRow>, DatabaseServiceError> {
        let rows = sqlx::query_as::<_, WorkspaceRowRaw>(
            "SELECT id, type, name, branch, directory, extra, project_id, time_used \
             FROM workspace WHERE project_id = ?1 ORDER BY time_used DESC",
        )
        .bind(project_id)
        .fetch_all(&self.pool)
        .await
        .map_err(|e| DatabaseServiceError::Database(format!("list workspaces: {e}")))?;

        Ok(rows.into_iter().map(WorkspaceRowRaw::into_row).collect())
    }

    /// Get a single workspace by ID.
    pub async fn get_workspace(
        &self,
        id: &str,
    ) -> Result<Option<WorkspaceRow>, DatabaseServiceError> {
        let row = sqlx::query_as::<_, WorkspaceRowRaw>(
            "SELECT id, type, name, branch, directory, extra, project_id, time_used \
             FROM workspace WHERE id = ?1",
        )
        .bind(id)
        .fetch_optional(&self.pool)
        .await
        .map_err(|e| DatabaseServiceError::Database(format!("get workspace: {e}")))?;

        Ok(row.map(WorkspaceRowRaw::into_row))
    }

    /// List workspaces by directory.
    pub async fn get_workspace_by_directory(
        &self,
        directory: &str,
    ) -> Result<Vec<WorkspaceRow>, DatabaseServiceError> {
        let rows = sqlx::query_as::<_, WorkspaceRowRaw>(
            "SELECT id, type, name, branch, directory, extra, project_id, time_used \
             FROM workspace WHERE directory = ?1 ORDER BY time_used DESC",
        )
        .bind(directory)
        .fetch_all(&self.pool)
        .await
        .map_err(|e| DatabaseServiceError::Database(format!("get workspace by directory: {e}")))?;

        Ok(rows.into_iter().map(WorkspaceRowRaw::into_row).collect())
    }

    /// List all event_sequence records (aggregate_id → seq mapping).
    pub async fn list_all_event_sequences(
        &self,
    ) -> Result<Vec<EventSequenceRow>, DatabaseServiceError> {
        let rows = sqlx::query_as::<_, EventSequenceRowRaw>(
            "SELECT aggregate_id, seq, owner_id FROM event_sequence",
        )
        .fetch_all(&self.pool)
        .await
        .map_err(|e| DatabaseServiceError::Database(format!("list event sequences: {e}")))?;

        Ok(rows.into_iter().map(EventSequenceRowRaw::into_row).collect())
    }

    /// Insert a new workspace.
    pub async fn insert_workspace(
        &self,
        id: &str,
        ws_type: &str,
        name: &str,
        branch: Option<&str>,
        directory: Option<&str>,
        extra: Option<&str>,
        project_id: &str,
        time_used: i64,
    ) -> Result<(), DatabaseServiceError> {
        sqlx::query(
            "INSERT INTO workspace (id, type, name, branch, directory, extra, project_id, time_used) \
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)",
        )
        .bind(id)
        .bind(ws_type)
        .bind(name)
        .bind(branch)
        .bind(directory)
        .bind(extra)
        .bind(project_id)
        .bind(time_used)
        .execute(&self.pool)
        .await
        .map_err(|e| DatabaseServiceError::Database(format!("insert workspace: {e}")))?;

        Ok(())
    }

    /// Update a workspace's mutable fields.
    pub async fn update_workspace(
        &self,
        id: &str,
        name: Option<&str>,
        branch: Option<&str>,
        directory: Option<&str>,
        extra: Option<&str>,
        time_used: Option<i64>,
    ) -> Result<(), DatabaseServiceError> {
        let now = time_used.unwrap_or_else(|| chrono::Utc::now().timestamp_millis());
        sqlx::query(
            "UPDATE workspace SET \
             name = COALESCE(?2, name), \
             branch = COALESCE(?3, branch), \
             directory = COALESCE(?4, directory), \
             extra = COALESCE(?5, extra), \
             time_used = ?6 \
             WHERE id = ?1",
        )
        .bind(id)
        .bind(name)
        .bind(branch)
        .bind(directory)
        .bind(extra)
        .bind(now)
        .execute(&self.pool)
        .await
        .map_err(|e| DatabaseServiceError::Database(format!("update workspace: {e}")))?;

        Ok(())
    }

    /// Delete a workspace by ID.
    pub async fn delete_workspace(&self, id: &str) -> Result<(), DatabaseServiceError> {
        let rows = sqlx::query("DELETE FROM workspace WHERE id = ?1")
            .bind(id)
            .execute(&self.pool)
            .await
            .map_err(|e| DatabaseServiceError::Database(format!("delete workspace: {e}")))?;

        if rows.rows_affected() == 0 {
            return Err(DatabaseServiceError::NotFound(format!("workspace {id}")));
        }
        Ok(())
    }

    // ── Session Share CRUD ───────────────────────────────────────────

    /// Get the session share record by session_id.
    pub async fn get_session_share(
        &self,
        session_id: &str,
    ) -> Result<Option<SessionShareRow>, DatabaseServiceError> {
        let row = sqlx::query_as::<_, SessionShareRowRaw>(
            "SELECT session_id, id, secret, url, time_created, time_updated \
             FROM session_share WHERE session_id = ?1",
        )
        .bind(session_id)
        .fetch_optional(&self.pool)
        .await
        .map_err(|e| DatabaseServiceError::Database(format!("get session share: {e}")))?;

        Ok(row.map(SessionShareRowRaw::into_row))
    }

    /// Insert a new session share.
    pub async fn insert_session_share(
        &self,
        session_id: &str,
        id: &str,
        secret: &str,
        url: &str,
        time_created: i64,
        time_updated: i64,
    ) -> Result<(), DatabaseServiceError> {
        sqlx::query(
            "INSERT INTO session_share (session_id, id, secret, url, time_created, time_updated) \
             VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
        )
        .bind(session_id)
        .bind(id)
        .bind(secret)
        .bind(url)
        .bind(time_created)
        .bind(time_updated)
        .execute(&self.pool)
        .await
        .map_err(|e| DatabaseServiceError::Database(format!("insert session share: {e}")))?;

        Ok(())
    }

    /// Update a session share's secret and url.
    pub async fn update_session_share(
        &self,
        session_id: &str,
        secret: Option<&str>,
        url: Option<&str>,
    ) -> Result<(), DatabaseServiceError> {
        let now = chrono::Utc::now().timestamp_millis();
        sqlx::query(
            "UPDATE session_share SET time_updated = ?2, \
             secret = COALESCE(?3, secret), \
             url = COALESCE(?4, url) \
             WHERE session_id = ?1",
        )
        .bind(session_id)
        .bind(now)
        .bind(secret)
        .bind(url)
        .execute(&self.pool)
        .await
        .map_err(|e| DatabaseServiceError::Database(format!("update session share: {e}")))?;

        Ok(())
    }

    /// Delete a session share by session_id.
    pub async fn delete_session_share(
        &self,
        session_id: &str,
    ) -> Result<(), DatabaseServiceError> {
        let rows = sqlx::query("DELETE FROM session_share WHERE session_id = ?1")
            .bind(session_id)
            .execute(&self.pool)
            .await
            .map_err(|e| DatabaseServiceError::Database(format!("delete session share: {e}")))?;

        if rows.rows_affected() == 0 {
            return Err(DatabaseServiceError::NotFound(format!(
                "session_share {session_id}"
            )));
        }
        Ok(())
    }

    // ── Todo CRUD ────────────────────────────────────────────────────

    /// List all todo items for a session, ordered by position.
    pub async fn list_todos(
        &self,
        session_id: &str,
    ) -> Result<Vec<TodoRow>, DatabaseServiceError> {
        let rows = sqlx::query_as::<_, TodoRowRaw>(
            "SELECT session_id, content, status, priority, position, time_created, time_updated \
             FROM todo WHERE session_id = ?1 ORDER BY position ASC",
        )
        .bind(session_id)
        .fetch_all(&self.pool)
        .await
        .map_err(|e| DatabaseServiceError::Database(format!("list todos: {e}")))?;

        Ok(rows.into_iter().map(TodoRowRaw::into_row).collect())
    }

    /// Insert a new todo item.
    pub async fn insert_todo(
        &self,
        session_id: &str,
        content: &str,
        status: &str,
        priority: &str,
        position: i64,
        time_created: i64,
        time_updated: i64,
    ) -> Result<(), DatabaseServiceError> {
        sqlx::query(
            "INSERT INTO todo (session_id, content, status, priority, position, time_created, time_updated) \
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)",
        )
        .bind(session_id)
        .bind(content)
        .bind(status)
        .bind(priority)
        .bind(position)
        .bind(time_created)
        .bind(time_updated)
        .execute(&self.pool)
        .await
        .map_err(|e| DatabaseServiceError::Database(format!("insert todo: {e}")))?;

        Ok(())
    }

    /// Update a todo item's content, status, and priority.
    pub async fn update_todo(
        &self,
        session_id: &str,
        position: i64,
        content: Option<&str>,
        status: Option<&str>,
        priority: Option<&str>,
    ) -> Result<(), DatabaseServiceError> {
        let now = chrono::Utc::now().timestamp_millis();
        sqlx::query(
            "UPDATE todo SET time_updated = ?3, \
             content = COALESCE(?4, content), \
             status = COALESCE(?5, status), \
             priority = COALESCE(?6, priority) \
             WHERE session_id = ?1 AND position = ?2",
        )
        .bind(session_id)
        .bind(position)
        .bind(now)
        .bind(content)
        .bind(status)
        .bind(priority)
        .execute(&self.pool)
        .await
        .map_err(|e| DatabaseServiceError::Database(format!("update todo: {e}")))?;

        Ok(())
    }

    /// Delete a todo item by session_id and position.
    pub async fn delete_todo(
        &self,
        session_id: &str,
        position: i64,
    ) -> Result<(), DatabaseServiceError> {
        let rows = sqlx::query("DELETE FROM todo WHERE session_id = ?1 AND position = ?2")
            .bind(session_id)
            .bind(position)
            .execute(&self.pool)
            .await
            .map_err(|e| DatabaseServiceError::Database(format!("delete todo: {e}")))?;

        if rows.rows_affected() == 0 {
            return Err(DatabaseServiceError::NotFound(format!(
                "todo {session_id}/{position}"
            )));
        }
        Ok(())
    }
}

// ── Row types for CRUD results ────────────────────────────────────────────

/// A row from the session table (query result).
///
/// # Source
/// Ported from `packages/core/src/database/schema.gen.ts` — session table.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SessionRow {
    pub id: String,
    pub project_id: String,
    pub workspace_id: Option<String>,
    pub parent_id: Option<String>,
    pub slug: String,
    pub directory: String,
    pub path: Option<String>,
    pub title: String,
    pub version: String,
    pub share_url: Option<String>,
    pub summary_additions: Option<i64>,
    pub summary_deletions: Option<i64>,
    pub summary_files: Option<i64>,
    pub summary_diffs: Option<String>,
    pub metadata: Option<String>,
    pub cost: f64,
    pub tokens_input: i64,
    pub tokens_output: i64,
    pub tokens_reasoning: i64,
    pub tokens_cache_read: i64,
    pub tokens_cache_write: i64,
    pub revert: Option<String>,
    pub permission: Option<String>,
    pub agent: Option<String>,
    pub model: Option<String>,
    pub time_created: i64,
    pub time_updated: i64,
    pub time_compacting: Option<i64>,
    pub time_archived: Option<i64>,
}

/// A row from the message table (query result).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MessageRow {
    pub id: String,
    pub session_id: String,
    pub data: String,
    pub time_created: i64,
    pub time_updated: i64,
}

/// A row from the part table (query result).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PartRow {
    pub id: String,
    pub message_id: String,
    pub session_id: String,
    pub data: String,
    pub time_created: i64,
    pub time_updated: i64,
}

/// A row from the session_message table (query result).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SessionMessageRow {
    pub id: String,
    pub session_id: String,
    pub msg_type: String,
    pub seq: i64,
    pub data: String,
    pub time_created: i64,
    pub time_updated: i64,
}

// ── sqlx::FromRow compatible raw types ────────────────────────────────────

#[derive(sqlx::FromRow)]
struct SessionRowRaw {
    id: String,
    project_id: String,
    workspace_id: Option<String>,
    parent_id: Option<String>,
    slug: String,
    directory: String,
    path: Option<String>,
    title: String,
    version: String,
    share_url: Option<String>,
    summary_additions: Option<i64>,
    summary_deletions: Option<i64>,
    summary_files: Option<i64>,
    summary_diffs: Option<String>,
    metadata: Option<String>,
    cost: f64,
    tokens_input: i64,
    tokens_output: i64,
    tokens_reasoning: i64,
    tokens_cache_read: i64,
    tokens_cache_write: i64,
    revert: Option<String>,
    permission: Option<String>,
    agent: Option<String>,
    model: Option<String>,
    time_created: i64,
    time_updated: i64,
    time_compacting: Option<i64>,
    time_archived: Option<i64>,
}

impl SessionRowRaw {
    fn into_row(self) -> SessionRow {
        SessionRow {
            id: self.id,
            project_id: self.project_id,
            workspace_id: self.workspace_id,
            parent_id: self.parent_id,
            slug: self.slug,
            directory: self.directory,
            path: self.path,
            title: self.title,
            version: self.version,
            share_url: self.share_url,
            summary_additions: self.summary_additions,
            summary_deletions: self.summary_deletions,
            summary_files: self.summary_files,
            summary_diffs: self.summary_diffs,
            metadata: self.metadata,
            cost: self.cost,
            tokens_input: self.tokens_input,
            tokens_output: self.tokens_output,
            tokens_reasoning: self.tokens_reasoning,
            tokens_cache_read: self.tokens_cache_read,
            tokens_cache_write: self.tokens_cache_write,
            revert: self.revert,
            permission: self.permission,
            agent: self.agent,
            model: self.model,
            time_created: self.time_created,
            time_updated: self.time_updated,
            time_compacting: self.time_compacting,
            time_archived: self.time_archived,
        }
    }
}

#[derive(sqlx::FromRow)]
struct MessageRowRaw {
    id: String,
    session_id: String,
    data: String,
    time_created: i64,
    time_updated: i64,
}

impl MessageRowRaw {
    fn into_row(self) -> MessageRow {
        MessageRow {
            id: self.id,
            session_id: self.session_id,
            data: self.data,
            time_created: self.time_created,
            time_updated: self.time_updated,
        }
    }
}

#[derive(sqlx::FromRow)]
struct PartRowRaw {
    id: String,
    message_id: String,
    session_id: String,
    data: String,
    time_created: i64,
    time_updated: i64,
}

impl PartRowRaw {
    fn into_row(self) -> PartRow {
        PartRow {
            id: self.id,
            message_id: self.message_id,
            session_id: self.session_id,
            data: self.data,
            time_created: self.time_created,
            time_updated: self.time_updated,
        }
    }
}

#[derive(sqlx::FromRow)]
struct SessionMessageRowRaw {
    id: String,
    session_id: String,
    #[sqlx(rename = "type")]
    msg_type: String,
    seq: i64,
    data: String,
    time_created: i64,
    time_updated: i64,
}

impl SessionMessageRowRaw {
    fn into_row(self) -> SessionMessageRow {
        SessionMessageRow {
            id: self.id,
            session_id: self.session_id,
            msg_type: self.msg_type,
            seq: self.seq,
            data: self.data,
            time_created: self.time_created,
            time_updated: self.time_updated,
        }
    }
}

// ── Tests ──────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    // ── Database path tests ─────────────────────────────────────────

    #[test]
    fn database_path_default_channel() {
        let path = database_path("/data/opencode", None, Some("latest"), false);
        assert_eq!(path, "/data/opencode/opencode.db");
    }

    #[test]
    fn database_path_beta_channel() {
        let path = database_path("/data/opencode", None, Some("beta"), false);
        assert_eq!(path, "/data/opencode/opencode.db");
    }

    #[test]
    fn database_path_custom_channel() {
        let path = database_path("/data/opencode", None, Some("nightly"), false);
        assert_eq!(path, "/data/opencode/opencode-nightly.db");
    }

    #[test]
    fn database_path_with_special_chars() {
        let path = database_path("/data/opencode", None, Some("dev/feature!@"), false);
        assert_eq!(path, "/data/opencode/opencode-dev-feature--.db");
    }

    #[test]
    fn database_path_memory() {
        let path = database_path("/data/opencode", Some(":memory:"), None, false);
        assert_eq!(path, ":memory:");
    }

    #[test]
    fn database_path_absolute_override() {
        let path = database_path("/data/opencode", Some("/tmp/mine.db"), None, false);
        assert_eq!(path, "/tmp/mine.db");
    }

    #[test]
    fn database_path_relative_override() {
        let path = database_path("/data/opencode", Some("custom.db"), None, false);
        assert_eq!(path, "/data/opencode/custom.db");
    }

    // ── GlobalPaths tests ───────────────────────────────────────────

    #[test]
    fn global_paths_default_has_all_fields() {
        let paths = GlobalPaths::default();
        assert!(!paths.home.is_empty());
        assert!(paths.data.ends_with("opencode"));
        assert!(paths.cache.ends_with("opencode"));
        assert!(paths.config.ends_with("opencode"));
        assert!(paths.state.ends_with("opencode"));
        assert!(!paths.log.is_empty());
        assert!(!paths.bin.is_empty());
        assert!(!paths.repos.is_empty());
        assert!(!paths.tmp.is_empty());
    }

    #[test]
    fn global_paths_with_overrides() {
        let paths = GlobalPaths::new(PathsOverride {
            data: Some("/custom/data".to_string()),
            ..Default::default()
        });
        assert_eq!(paths.data, "/custom/data");
        // Other paths should still be default-ish
        assert!(!paths.cache.is_empty());
    }

    // ── DatabaseConfig tests ────────────────────────────────────────

    #[test]
    fn database_config_default() {
        let config = DatabaseConfig::default();
        assert!(config.wal);
        assert!(config.foreign_keys);
        assert_eq!(config.busy_timeout, 5000);
        assert_eq!(config.cache_size, -64000);
    }

    #[test]
    fn database_config_memory() {
        let config = DatabaseConfig::memory();
        assert_eq!(config.filename, ":memory:");
        assert_eq!(config.mode, SqliteMode::Memory);
    }

    #[test]
    fn database_config_pragmas() {
        let config = DatabaseConfig::default();
        let pragmas = config.pragmas();
        assert!(pragmas.iter().any(|p| p.contains("WAL")));
        assert!(pragmas.iter().any(|p| p.contains("foreign_keys")));
        assert!(pragmas.iter().any(|p| p.contains("busy_timeout")));
    }

    // ── Table definition count ──────────────────────────────────────

    #[test]
    fn all_tables_count() {
        assert_eq!(ALL_TABLE_NAMES.len(), 20);
        assert_eq!(ALL_CREATE_TABLES.len(), 20);
    }

    #[test]
    fn indexes_count() {
        assert_eq!(CREATE_INDEXES.len(), 17);
    }

    // ── Migration IDs tests ─────────────────────────────────────────

    #[test]
    fn known_migration_ids_count() {
        assert_eq!(KNOWN_MIGRATION_IDS.len(), 35);
    }

    #[test]
    fn migration_ids_are_unique() {
        let mut sorted = KNOWN_MIGRATION_IDS.to_vec();
        sorted.sort();
        sorted.dedup();
        assert_eq!(sorted.len(), KNOWN_MIGRATION_IDS.len());
    }

    #[test]
    fn migration_ids_chronological() {
        // Verify they're in chronological order (by timestamp prefix)
        for window in KNOWN_MIGRATION_IDS.windows(2) {
            let prev = &window[0][..14]; // yyyymmddhhmmss
            let next = &window[1][..14];
            assert!(prev <= next, "{prev} should come before {next}");
        }
    }

    // ── Path helpers ────────────────────────────────────────────────

    #[test]
    fn db_absolute_path_valid() {
        let result = db_absolute_path("/home/user/project").unwrap();
        assert_eq!(result, "/home/user/project");
    }

    #[test]
    fn db_absolute_path_rejects_relative() {
        assert!(db_absolute_path("relative/path").is_err());
    }

    #[test]
    fn to_platform_path_unix_unchanged() {
        if !cfg!(windows) {
            assert_eq!(to_platform_path("/home/user"), "/home/user");
        }
    }

    // ── Timestamps tests ────────────────────────────────────────────

    #[test]
    fn timestamps_now() {
        let ts = Timestamps::now();
        assert!(ts.time_created > 0);
        assert_eq!(ts.time_created, ts.time_updated);
    }

    #[test]
    fn timestamps_touch() {
        let mut ts = Timestamps::new(100, 100);
        let old_updated = ts.time_updated;
        // Sleep a tiny bit to ensure different time
        std::thread::sleep(std::time::Duration::from_millis(1));
        ts.touch();
        assert!(ts.time_updated > old_updated);
    }

    // ── Connection pragmas ──────────────────────────────────────────

    #[test]
    fn connection_pragmas_count() {
        assert_eq!(CONNECTION_PRAGMAS.len(), 6);
    }

    // ── Migration set ───────────────────────────────────────────────

    #[test]
    fn migration_set_ids() {
        let set = MigrationSet {
            migrations: vec![
                Migration {
                    id: "a".into(),
                    up: vec![],
                },
                Migration {
                    id: "b".into(),
                    up: vec![],
                },
            ],
        };
        assert_eq!(set.len(), 2);
        assert!(!set.is_empty());
        assert_eq!(set.ids(), vec!["a", "b"]);
    }

    // ── Database service tests ───────────────────────────────────────

    /// Helper: create an in-memory SQLite database with schema and service.
    async fn setup_test_db() -> (sqlx::SqlitePool, DatabaseService) {
        use sqlx::SqlitePool;

        let pool = SqlitePool::connect("sqlite::memory:")
            .await
            .expect("connect in-memory");
        // Enable WAL + FK
        sqlx::query("PRAGMA journal_mode = WAL")
            .execute(&pool)
            .await
            .unwrap();
        sqlx::query("PRAGMA foreign_keys = ON")
            .execute(&pool)
            .await
            .unwrap();
        sqlx::query("PRAGMA synchronous = NORMAL")
            .execute(&pool)
            .await
            .unwrap();
        sqlx::query("PRAGMA busy_timeout = 5000")
            .execute(&pool)
            .await
            .unwrap();

        // Create all tables (including migration tracking)
        // ALL_CREATE_TABLES includes CREATE_TABLE_MIGRATION
        for sql in ALL_CREATE_TABLES {
            sqlx::query(sql).execute(&pool).await.unwrap();
        }

        let svc = DatabaseService::new(pool.clone());
        (pool, svc)
    }

    #[tokio::test]
    async fn test_migration_status_empty() {
        let (_pool, svc) = setup_test_db().await;
        let status = svc.migration_status().await.expect("migration status");
        assert!(status.is_empty());
    }

    #[tokio::test]
    async fn test_migration_status_populated() {
        let (_pool, svc) = setup_test_db().await;
        let now = chrono::Utc::now().timestamp_millis();

        // Insert some migration records
        sqlx::query("INSERT INTO migration (id, time_completed) VALUES (?1, ?2)")
            .bind("20260101000000_test_a")
            .bind(now)
            .execute(svc.pool())
            .await
            .unwrap();
        sqlx::query("INSERT INTO migration (id, time_completed) VALUES (?1, ?2)")
            .bind("20260102000000_test_b")
            .bind(now + 1000)
            .execute(svc.pool())
            .await
            .unwrap();

        let status = svc.migration_status().await.expect("migration status");
        assert_eq!(status.len(), 2);
        assert_eq!(status[0].id, "20260101000000_test_a");
        assert_eq!(status[1].id, "20260102000000_test_b");
    }

    #[tokio::test]
    async fn test_is_migration_applied() {
        let (_pool, svc) = setup_test_db().await;
        let now = chrono::Utc::now().timestamp_millis();

        sqlx::query("INSERT INTO migration (id, time_completed) VALUES (?1, ?2)")
            .bind("20260101000000_done")
            .bind(now)
            .execute(svc.pool())
            .await
            .unwrap();

        assert!(svc
            .is_migration_applied("20260101000000_done")
            .await
            .unwrap());
        assert!(!svc
            .is_migration_applied("20260101000000_pending")
            .await
            .unwrap());
    }

    #[tokio::test]
    async fn test_migration_count() {
        let (_pool, svc) = setup_test_db().await;
        let now = chrono::Utc::now().timestamp_millis();

        assert_eq!(svc.migration_count().await.unwrap(), 0);

        sqlx::query("INSERT INTO migration (id, time_completed) VALUES (?1, ?2)")
            .bind("20260101000000_a")
            .bind(now)
            .execute(svc.pool())
            .await
            .unwrap();

        assert_eq!(svc.migration_count().await.unwrap(), 1);
    }

    #[tokio::test]
    async fn test_insert_and_list_sessions() {
        let (_pool, svc) = setup_test_db().await;
        let now = chrono::Utc::now().timestamp_millis();

        // Need to insert a project first (FK constraint)
        sqlx::query(
            "INSERT INTO project (id, worktree, vcs, name, time_created, time_updated, sandboxes)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)",
        )
        .bind("proj-1")
        .bind("/home/proj")
        .bind("git")
        .bind("test-project")
        .bind(now)
        .bind(now)
        .bind("[]")
        .execute(svc.pool())
        .await
        .unwrap();

        // Insert sessions
        svc.insert_session(
            "sess-1",
            "proj-1",
            None,
            None,
            "my-session",
            "/home/proj",
            None,
            "Test Session",
            "1.0",
            now,
            now,
            Some("build"),
            Some("claude"),
            None,
            None,
            None,
        )
        .await
        .expect("insert session");

        svc.insert_session(
            "sess-2",
            "proj-1",
            None,
            None,
            "other-session",
            "/home/proj",
            None,
            "Other Session",
            "1.0",
            now + 1,
            now + 1,
            None,
            None,
            None,
            None,
            None,
        )
        .await
        .expect("insert session 2");

        let sessions = svc
            .list_sessions("proj-1", None)
            .await
            .expect("list sessions");
        assert_eq!(sessions.len(), 2);
        assert_eq!(sessions[0].id, "sess-2"); // Most recently updated first
        assert_eq!(sessions[0].title, "Other Session");
        assert_eq!(sessions[1].id, "sess-1");
    }

    #[tokio::test]
    async fn test_update_session() {
        let (_pool, svc) = setup_test_db().await;
        let now = chrono::Utc::now().timestamp_millis();

        sqlx::query(
            "INSERT INTO project (id, worktree, vcs, name, time_created, time_updated, sandboxes)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)",
        )
        .bind("proj-1")
        .bind("/home/proj")
        .bind("git")
        .bind("test-project")
        .bind(now)
        .bind(now)
        .bind("[]")
        .execute(svc.pool())
        .await
        .unwrap();

        svc.insert_session(
            "sess-1",
            "proj-1",
            None,
            None,
            "slug",
            "/dir",
            None,
            "Old Title",
            "1.0",
            now,
            now,
            None,
            None,
            None,
            None,
            None,
        )
        .await
        .unwrap();

        svc.update_session(
            "sess-1",
            now + 100,
            Some("New Title"),
            Some(0.05),
            Some(100),
            Some(50),
            None,
            None,
            None,
            None,
            None,
            None,
            None,
            None,
            None,
            None,
            None,
            None,
            None,
        )
        .await
        .unwrap();

        let sessions = svc.list_sessions("proj-1", None).await.unwrap();
        assert_eq!(sessions[0].title, "New Title");
        assert!((sessions[0].cost - 0.05).abs() < f64::EPSILON);
        assert_eq!(sessions[0].tokens_input, 100);
        assert_eq!(sessions[0].tokens_output, 50);
    }

    #[tokio::test]
    async fn test_delete_session() {
        let (_pool, svc) = setup_test_db().await;
        let now = chrono::Utc::now().timestamp_millis();

        sqlx::query(
            "INSERT INTO project (id, worktree, name, time_created, time_updated, sandboxes)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
        )
        .bind("proj-1")
        .bind("/proj")
        .bind("p")
        .bind(now)
        .bind(now)
        .bind("[]")
        .execute(svc.pool())
        .await
        .unwrap();

        svc.insert_session(
            "sess-1", "proj-1", None, None, "s", "/d", None, "T", "1", now, now, None, None, None, None, None,
        )
        .await
        .unwrap();

        svc.delete_session("sess-1").await.expect("delete session");
        let sessions = svc.list_sessions("proj-1", None).await.unwrap();
        assert!(sessions.is_empty());
    }

    #[tokio::test]
    async fn test_delete_session_not_found() {
        let (_pool, svc) = setup_test_db().await;
        let result = svc.delete_session("nonexistent").await;
        assert!(matches!(result, Err(DatabaseServiceError::NotFound(_))));
    }

    #[tokio::test]
    async fn test_message_crud() {
        let (_pool, svc) = setup_test_db().await;
        let now = chrono::Utc::now().timestamp_millis();

        // Need session FK
        sqlx::query(
            "INSERT INTO project (id, worktree, name, time_created, time_updated, sandboxes)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
        )
        .bind("proj-1")
        .bind("/p")
        .bind("p")
        .bind(now)
        .bind(now)
        .bind("[]")
        .execute(svc.pool())
        .await
        .unwrap();

        sqlx::query(
            "INSERT INTO session (id, project_id, slug, directory, title, version, time_created, time_updated)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)",
        )
        .bind("sess-1").bind("proj-1").bind("s").bind("/d").bind("T").bind("1").bind(now).bind(now)
        .execute(svc.pool()).await.unwrap();

        // Insert messages
        let data1 = r#"{"role":"user","content":"hello"}"#;
        svc.insert_message("msg-1", "sess-1", data1, now, now)
            .await
            .unwrap();
        let data2 = r#"{"role":"assistant","content":"hi there"}"#;
        svc.insert_message("msg-2", "sess-1", data2, now + 1, now + 1)
            .await
            .unwrap();

        let messages = svc.list_messages("sess-1", None).await.unwrap();
        assert_eq!(messages.len(), 2);
        assert_eq!(messages[0].id, "msg-1");
        assert!(messages[0].data.contains("hello"));

        // Delete
        svc.delete_message("msg-1").await.unwrap();
        let messages = svc.list_messages("sess-1", None).await.unwrap();
        assert_eq!(messages.len(), 1);
        assert_eq!(messages[0].id, "msg-2");
    }

    #[tokio::test]
    async fn test_part_crud() {
        let (_pool, svc) = setup_test_db().await;
        let now = chrono::Utc::now().timestamp_millis();

        // Setup FK chain
        sqlx::query(
            "INSERT INTO project (id, worktree, name, time_created, time_updated, sandboxes)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
        )
        .bind("proj-1")
        .bind("/p")
        .bind("p")
        .bind(now)
        .bind(now)
        .bind("[]")
        .execute(svc.pool())
        .await
        .unwrap();

        sqlx::query(
            "INSERT INTO session (id, project_id, slug, directory, title, version, time_created, time_updated)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)",
        )
        .bind("sess-1").bind("proj-1").bind("s").bind("/d").bind("T").bind("1").bind(now).bind(now)
        .execute(svc.pool()).await.unwrap();

        sqlx::query(
            "INSERT INTO message (id, session_id, data, time_created, time_updated)
             VALUES (?1, ?2, ?3, ?4, ?5)",
        )
        .bind("msg-1")
        .bind("sess-1")
        .bind("{}")
        .bind(now)
        .bind(now)
        .execute(svc.pool())
        .await
        .unwrap();

        // Insert parts
        svc.insert_part(
            "part-1",
            "msg-1",
            "sess-1",
            r#"{"type":"text","content":"a"}"#,
            now,
            now,
        )
        .await
        .unwrap();
        svc.insert_part(
            "part-2",
            "msg-1",
            "sess-1",
            r#"{"type":"text","content":"b"}"#,
            now + 1,
            now + 1,
        )
        .await
        .unwrap();

        let parts = svc.list_parts("msg-1").await.unwrap();
        assert_eq!(parts.len(), 2);
        assert_eq!(parts[0].id, "part-1");

        let deleted = svc.delete_parts_for_message("msg-1").await.unwrap();
        assert_eq!(deleted, 2);
        let parts = svc.list_parts("msg-1").await.unwrap();
        assert!(parts.is_empty());
    }

    #[tokio::test]
    async fn test_session_message_crud() {
        let (_pool, svc) = setup_test_db().await;
        let now = chrono::Utc::now().timestamp_millis();

        sqlx::query(
            "INSERT INTO project (id, worktree, name, time_created, time_updated, sandboxes)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
        )
        .bind("proj-1")
        .bind("/p")
        .bind("p")
        .bind(now)
        .bind(now)
        .bind("[]")
        .execute(svc.pool())
        .await
        .unwrap();

        sqlx::query(
            "INSERT INTO session (id, project_id, slug, directory, title, version, time_created, time_updated)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)",
        )
        .bind("sess-1").bind("proj-1").bind("s").bind("/d").bind("T").bind("1").bind(now).bind(now)
        .execute(svc.pool()).await.unwrap();

        svc.insert_session_message(
            "sm-1",
            "sess-1",
            "user",
            1,
            r#"{"role":"user","content":"hi"}"#,
            now,
            now,
        )
        .await
        .unwrap();
        svc.insert_session_message(
            "sm-2",
            "sess-1",
            "assistant",
            2,
            r#"{"role":"assistant","content":"hello"}"#,
            now + 1,
            now + 1,
        )
        .await
        .unwrap();

        let msgs = svc.list_session_messages("sess-1", None).await.unwrap();
        assert_eq!(msgs.len(), 2);
        assert_eq!(msgs[0].msg_type, "user");
        assert_eq!(msgs[0].seq, 1);
        assert_eq!(msgs[1].msg_type, "assistant");
        assert_eq!(msgs[1].seq, 2);
    }

    #[tokio::test]
    async fn test_migration_idempotency() {
        let (_pool, svc) = setup_test_db().await;
        let now = chrono::Utc::now().timestamp_millis();

        // Apply a migration
        sqlx::query("INSERT INTO migration (id, time_completed) VALUES (?1, ?2)")
            .bind("20260101000000_test")
            .bind(now)
            .execute(svc.pool())
            .await
            .unwrap();

        // Check it's applied
        assert!(svc
            .is_migration_applied("20260101000000_test")
            .await
            .unwrap());

        // Try inserting again — should fail (PK constraint)
        let result = sqlx::query("INSERT INTO migration (id, time_completed) VALUES (?1, ?2)")
            .bind("20260101000000_test")
            .bind(now + 1)
            .execute(svc.pool())
            .await;

        assert!(result.is_err(), "duplicate migration insert should fail");

        // Count should still be 1
        assert_eq!(svc.migration_count().await.unwrap(), 1);
    }

    #[tokio::test]
    async fn test_concurrent_access_multiple_connections() {
        // Open multiple pools to the same in-memory DB isn't possible.
        // Use a file-based DB instead.
        let dir = tempfile::tempdir().expect("create temp dir");
        let db_path = dir.path().join("concurrent.db");

        let url1 = format!("sqlite:{}?mode=rwc", db_path.display());
        let url2 = format!("sqlite:{}?mode=rwc", db_path.display());

        let pool1 = sqlx::SqlitePool::connect(&url1)
            .await
            .expect("connect pool1");
        let pool2 = sqlx::SqlitePool::connect(&url2)
            .await
            .expect("connect pool2");

        // Set PRAGMAs on both
        for pool in [&pool1, &pool2] {
            sqlx::query("PRAGMA journal_mode = WAL")
                .execute(pool)
                .await
                .unwrap();
            sqlx::query("PRAGMA foreign_keys = ON")
                .execute(pool)
                .await
                .unwrap();
            sqlx::query("PRAGMA busy_timeout = 5000")
                .execute(pool)
                .await
                .unwrap();
        }

        // Create tables on pool1
        for sql in ALL_CREATE_TABLES {
            sqlx::query(sql).execute(&pool1).await.unwrap();
        }

        let now = chrono::Utc::now().timestamp_millis();
        sqlx::query(
            "INSERT INTO project (id, worktree, name, time_created, time_updated, sandboxes)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
        )
        .bind("proj-1")
        .bind("/p")
        .bind("p")
        .bind(now)
        .bind(now)
        .bind("[]")
        .execute(&pool1)
        .await
        .unwrap();

        sqlx::query(
            "INSERT INTO session (id, project_id, slug, directory, title, version, time_created, time_updated)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)",
        )
        .bind("sess-1").bind("proj-1").bind("s").bind("/d").bind("T").bind("1").bind(now).bind(now)
        .execute(&pool1).await.unwrap();

        // Insert from pool2 concurrently
        let pool2_clone = pool2.clone();
        let handle = tokio::spawn(async move {
            let svc2 = DatabaseService::new(pool2_clone);
            svc2.insert_session(
                "sess-2",
                "proj-1",
                None,
                None,
                "slug2",
                "/dir2",
                None,
                "Title2",
                "1.0",
                now + 1,
                now + 1,
                None,
                None,
                None,
                None,
                None,
            )
            .await
        });

        // Insert from pool1 simultaneously
        let svc1 = DatabaseService::new(pool1.clone());
        svc1.insert_session(
            "sess-3",
            "proj-1",
            None,
            None,
            "slug3",
            "/dir3",
            None,
            "Title3",
            "1.0",
            now + 2,
            now + 2,
            None,
            None,
            None,
            None,
            None,
        )
        .await
        .expect("insert from pool1");

        handle.await.expect("join").expect("insert from pool2");

        // Both inserts should have succeeded
        let sessions = svc1.list_sessions("proj-1", None).await.unwrap();
        assert_eq!(sessions.len(), 3); // sess-1, sess-2, sess-3
    }

    #[tokio::test]
    async fn test_wal_checkpoint_behavior() {
        let dir = tempfile::tempdir().expect("create temp dir");
        let db_path = dir.path().join("wal_test.db");

        let pool = sqlx::SqlitePool::connect(&format!("sqlite:{}?mode=rwc", db_path.display()))
            .await
            .expect("connect");

        // Enable WAL
        sqlx::query("PRAGMA journal_mode = WAL")
            .execute(&pool)
            .await
            .unwrap();
        sqlx::query("PRAGMA foreign_keys = ON")
            .execute(&pool)
            .await
            .unwrap();

        // Create a table and insert data
        sqlx::query("CREATE TABLE IF NOT EXISTS wal_test (id INTEGER PRIMARY KEY, value TEXT)")
            .execute(&pool)
            .await
            .unwrap();

        for i in 0..50 {
            sqlx::query("INSERT INTO wal_test (id, value) VALUES (?1, ?2)")
                .bind(i)
                .bind(format!("value-{i}"))
                .execute(&pool)
                .await
                .unwrap();
        }

        // Verify WAL is in use
        let mode: (String,) = sqlx::query_as("PRAGMA journal_mode")
            .fetch_one(&pool)
            .await
            .unwrap();
        assert_eq!(mode.0.to_lowercase(), "wal");

        // Run a passive checkpoint
        sqlx::query("PRAGMA wal_checkpoint(PASSIVE)")
            .execute(&pool)
            .await
            .unwrap();

        // Data should still be intact
        let count: (i64,) = sqlx::query_as("SELECT COUNT(*) FROM wal_test")
            .fetch_one(&pool)
            .await
            .unwrap();
        assert_eq!(count.0, 50);

        // Verify we can still read and write after checkpoint
        sqlx::query("INSERT INTO wal_test (id, value) VALUES (?1, ?2)")
            .bind(100)
            .bind("after-checkpoint")
            .execute(&pool)
            .await
            .unwrap();

        let (val,): (String,) = sqlx::query_as("SELECT value FROM wal_test WHERE id = 100")
            .fetch_one(&pool)
            .await
            .unwrap();
        assert_eq!(val, "after-checkpoint");
    }

    #[tokio::test]
    async fn test_session_list_with_limit() {
        let (_pool, svc) = setup_test_db().await;
        let now = chrono::Utc::now().timestamp_millis();

        sqlx::query(
            "INSERT INTO project (id, worktree, name, time_created, time_updated, sandboxes)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
        )
        .bind("proj-1")
        .bind("/p")
        .bind("p")
        .bind(now)
        .bind(now)
        .bind("[]")
        .execute(svc.pool())
        .await
        .unwrap();

        for i in 0..5 {
            svc.insert_session(
                &format!("sess-{i}"),
                "proj-1",
                None,
                None,
                &format!("slug-{i}"),
                "/d",
                None,
                &format!("Title {i}"),
                "1",
                now + i,
                now + i,
                None,
                None,
                None,
                None,
                None,
            )
            .await
            .unwrap();
        }

        let sessions = svc.list_sessions("proj-1", Some(3)).await.unwrap();
        assert_eq!(sessions.len(), 3);
    }

    // ── JSON column helpers ──────────────────────────────────────────

    #[test]
    fn test_json_column_serialize_deserialize() {
        let data = vec!["hello", "world"];
        let json = json_column_serialize(&data).unwrap();
        assert_eq!(json, r#"["hello","world"]"#);
        let back: Vec<String> = json_column_deserialize(&json).unwrap();
        assert_eq!(back, vec!["hello", "world"]);
    }

    #[test]
    fn test_json_column_roundtrip() {
        let col: JsonColumn<Vec<String>> = JsonColumn(vec!["a".into(), "b".into()]);
        let db = col.to_db().unwrap();
        let parsed: JsonColumn<Vec<String>> = JsonColumn::from_db(&db).unwrap();
        assert_eq!(parsed.0, vec!["a", "b"]);
    }

    #[test]
    fn test_json_absolute_path_array_column() {
        let result = json_absolute_path_array_column(&["/home/user/proj", "/tmp/test"]);
        assert!(result.is_ok());
        let json = result.unwrap();
        assert!(json.contains("/home/user/proj"));

        let parsed = json_parse_absolute_path_array(&json).unwrap();
        assert_eq!(parsed.len(), 2);
        assert_eq!(parsed[0], "/home/user/proj");
    }

    #[test]
    fn test_json_absolute_path_array_column_rejects_relative() {
        let result = json_absolute_path_array_column(&["relative/path"]);
        assert!(result.is_err());
    }

    #[test]
    fn test_json_parse_invalid() {
        let result: Result<Vec<String>, String> = json_column_deserialize("not json");
        assert!(result.is_err());
    }
}

// ── Event row types ──────────────────────────────────────────────────────

/// A row from the event table.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EventRow {
    pub id: String,
    pub aggregate_id: String,
    pub seq: i64,
    pub event_type: String,
    pub data: String,
}

/// A row from the event_sequence table.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EventSequenceRow {
    pub aggregate_id: String,
    pub seq: i64,
    pub owner_id: Option<String>,
}

/// A row from the session_input table.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SessionInputRow {
    pub id: String,
    pub session_id: String,
    pub prompt: String,
    pub delivery: String,
    pub admitted_seq: i64,
    pub promoted_seq: Option<i64>,
    pub time_created: i64,
}

/// A row from the session_context_epoch table.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ContextEpochRow {
    pub session_id: String,
    pub baseline: String,
    pub agent: String,
    pub snapshot: String,
    pub baseline_seq: i64,
    pub replacement_seq: Option<i64>,
    pub revision: i64,
}

// ── sqlx::FromRow compatible raw types for event tables ─────────────────

#[derive(sqlx::FromRow)]
struct EventRowRaw {
    id: String,
    aggregate_id: String,
    seq: i64,
    #[sqlx(rename = "type")]
    event_type: String,
    data: String,
}

impl EventRowRaw {
    fn into_row(self) -> EventRow {
        EventRow {
            id: self.id,
            aggregate_id: self.aggregate_id,
            seq: self.seq,
            event_type: self.event_type,
            data: self.data,
        }
    }
}

#[derive(sqlx::FromRow)]
struct EventSequenceRowRaw {
    aggregate_id: String,
    seq: i64,
    owner_id: Option<String>,
}

impl EventSequenceRowRaw {
    fn into_row(self) -> EventSequenceRow {
        EventSequenceRow {
            aggregate_id: self.aggregate_id,
            seq: self.seq,
            owner_id: self.owner_id,
        }
    }
}

#[derive(sqlx::FromRow)]
struct SessionInputRowRaw {
    id: String,
    session_id: String,
    prompt: String,
    delivery: String,
    admitted_seq: i64,
    promoted_seq: Option<i64>,
    time_created: i64,
}

impl SessionInputRowRaw {
    fn into_row(self) -> SessionInputRow {
        SessionInputRow {
            id: self.id,
            session_id: self.session_id,
            prompt: self.prompt,
            delivery: self.delivery,
            admitted_seq: self.admitted_seq,
            promoted_seq: self.promoted_seq,
            time_created: self.time_created,
        }
    }
}

#[derive(sqlx::FromRow)]
struct ContextEpochRowRaw {
    session_id: String,
    baseline: String,
    agent: String,
    snapshot: String,
    baseline_seq: i64,
    replacement_seq: Option<i64>,
    revision: i64,
}

impl ContextEpochRowRaw {
    fn into_row(self) -> ContextEpochRow {
        ContextEpochRow {
            session_id: self.session_id,
            baseline: self.baseline,
            agent: self.agent,
            snapshot: self.snapshot,
            baseline_seq: self.baseline_seq,
            replacement_seq: self.replacement_seq,
            revision: self.revision,
        }
    }
}

// ── Row types for new tables ──────────────────────────────────────────────

/// A row from the project table.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProjectRow {
    pub id: String,
    pub worktree: String,
    pub vcs: Option<String>,
    pub name: Option<String>,
    pub icon_url: Option<String>,
    pub icon_url_override: Option<String>,
    pub icon_color: Option<String>,
    pub time_created: i64,
    pub time_updated: i64,
    pub time_initialized: Option<i64>,
    pub sandboxes: String,
    pub commands: Option<String>,
}

/// A row from the project_directory table.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProjectDirectoryRow {
    pub project_id: String,
    pub directory: String,
    pub dir_type: Option<String>,
    pub strategy: Option<String>,
    pub time_created: i64,
}

/// A row from the account table.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AccountRow {
    pub id: String,
    pub email: String,
    pub url: String,
    pub access_token: String,
    pub refresh_token: String,
    pub token_expiry: Option<i64>,
    pub time_created: i64,
    pub time_updated: i64,
}

/// A row from the account_state table.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AccountStateRow {
    pub id: i64,
    pub active_account_id: Option<String>,
    pub active_org_id: Option<String>,
}

/// A row from the control_account table.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ControlAccountRow {
    pub email: String,
    pub url: String,
    pub access_token: String,
    pub refresh_token: String,
    pub token_expiry: Option<i64>,
    pub active: bool,
    pub time_created: i64,
    pub time_updated: i64,
}

/// A row from the permission table.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PermissionRow {
    pub id: String,
    pub project_id: String,
    pub action: String,
    pub resource: String,
    pub time_created: i64,
    pub time_updated: i64,
}

/// A row from the workspace table.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WorkspaceRow {
    pub id: String,
    pub ws_type: String,
    pub name: String,
    pub branch: Option<String>,
    pub directory: Option<String>,
    pub extra: Option<String>,
    pub project_id: String,
    pub time_used: i64,
}

/// A row from the session_share table.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SessionShareRow {
    pub session_id: String,
    pub id: String,
    pub secret: String,
    pub url: String,
    pub time_created: i64,
    pub time_updated: i64,
}

/// A row from the todo table.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TodoRow {
    pub session_id: String,
    pub content: String,
    pub status: String,
    pub priority: String,
    pub position: i64,
    pub time_created: i64,
    pub time_updated: i64,
}

// ── sqlx::FromRow compatible raw types for new tables ────────────────────

#[derive(sqlx::FromRow)]
struct ProjectRowRaw {
    id: String,
    worktree: String,
    vcs: Option<String>,
    name: Option<String>,
    icon_url: Option<String>,
    icon_url_override: Option<String>,
    icon_color: Option<String>,
    time_created: i64,
    time_updated: i64,
    time_initialized: Option<i64>,
    sandboxes: String,
    commands: Option<String>,
}

impl ProjectRowRaw {
    fn into_row(self) -> ProjectRow {
        ProjectRow {
            id: self.id,
            worktree: self.worktree,
            vcs: self.vcs,
            name: self.name,
            icon_url: self.icon_url,
            icon_url_override: self.icon_url_override,
            icon_color: self.icon_color,
            time_created: self.time_created,
            time_updated: self.time_updated,
            time_initialized: self.time_initialized,
            sandboxes: self.sandboxes,
            commands: self.commands,
        }
    }
}

#[derive(sqlx::FromRow)]
struct ProjectDirectoryRowRaw {
    project_id: String,
    directory: String,
    #[sqlx(rename = "type")]
    dir_type: Option<String>,
    strategy: Option<String>,
    time_created: i64,
}

impl ProjectDirectoryRowRaw {
    fn into_row(self) -> ProjectDirectoryRow {
        ProjectDirectoryRow {
            project_id: self.project_id,
            directory: self.directory,
            dir_type: self.dir_type,
            strategy: self.strategy,
            time_created: self.time_created,
        }
    }
}

#[derive(sqlx::FromRow)]
struct AccountRowRaw {
    id: String,
    email: String,
    url: String,
    access_token: String,
    refresh_token: String,
    token_expiry: Option<i64>,
    time_created: i64,
    time_updated: i64,
}

impl AccountRowRaw {
    fn into_row(self) -> AccountRow {
        AccountRow {
            id: self.id,
            email: self.email,
            url: self.url,
            access_token: self.access_token,
            refresh_token: self.refresh_token,
            token_expiry: self.token_expiry,
            time_created: self.time_created,
            time_updated: self.time_updated,
        }
    }
}

#[derive(sqlx::FromRow)]
struct AccountStateRowRaw {
    id: i64,
    active_account_id: Option<String>,
    active_org_id: Option<String>,
}

impl AccountStateRowRaw {
    fn into_row(self) -> AccountStateRow {
        AccountStateRow {
            id: self.id,
            active_account_id: self.active_account_id,
            active_org_id: self.active_org_id,
        }
    }
}

#[derive(sqlx::FromRow)]
struct ControlAccountRowRaw {
    email: String,
    url: String,
    access_token: String,
    refresh_token: String,
    token_expiry: Option<i64>,
    active: bool,
    time_created: i64,
    time_updated: i64,
}

impl ControlAccountRowRaw {
    fn into_row(self) -> ControlAccountRow {
        ControlAccountRow {
            email: self.email,
            url: self.url,
            access_token: self.access_token,
            refresh_token: self.refresh_token,
            token_expiry: self.token_expiry,
            active: self.active,
            time_created: self.time_created,
            time_updated: self.time_updated,
        }
    }
}

#[derive(sqlx::FromRow)]
struct PermissionRowRaw {
    id: String,
    project_id: String,
    action: String,
    resource: String,
    time_created: i64,
    time_updated: i64,
}

impl PermissionRowRaw {
    fn into_row(self) -> PermissionRow {
        PermissionRow {
            id: self.id,
            project_id: self.project_id,
            action: self.action,
            resource: self.resource,
            time_created: self.time_created,
            time_updated: self.time_updated,
        }
    }
}

#[derive(sqlx::FromRow)]
struct WorkspaceRowRaw {
    id: String,
    #[sqlx(rename = "type")]
    ws_type: String,
    name: String,
    branch: Option<String>,
    directory: Option<String>,
    extra: Option<String>,
    project_id: String,
    time_used: i64,
}

impl WorkspaceRowRaw {
    fn into_row(self) -> WorkspaceRow {
        WorkspaceRow {
            id: self.id,
            ws_type: self.ws_type,
            name: self.name,
            branch: self.branch,
            directory: self.directory,
            extra: self.extra,
            project_id: self.project_id,
            time_used: self.time_used,
        }
    }
}

#[derive(sqlx::FromRow)]
struct SessionShareRowRaw {
    session_id: String,
    id: String,
    secret: String,
    url: String,
    time_created: i64,
    time_updated: i64,
}

impl SessionShareRowRaw {
    fn into_row(self) -> SessionShareRow {
        SessionShareRow {
            session_id: self.session_id,
            id: self.id,
            secret: self.secret,
            url: self.url,
            time_created: self.time_created,
            time_updated: self.time_updated,
        }
    }
}

#[derive(sqlx::FromRow)]
struct TodoRowRaw {
    session_id: String,
    content: String,
    status: String,
    priority: String,
    position: i64,
    time_created: i64,
    time_updated: i64,
}

impl TodoRowRaw {
    fn into_row(self) -> TodoRow {
        TodoRow {
            session_id: self.session_id,
            content: self.content,
            status: self.status,
            priority: self.priority,
            position: self.position,
            time_created: self.time_created,
            time_updated: self.time_updated,
        }
    }
}
