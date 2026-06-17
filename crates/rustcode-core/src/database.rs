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
    let is_default_channel =
        matches!(channel, "latest" | "beta" | "prod") || disable_channel_db;

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
        .map(|c| if c.is_ascii_alphanumeric() || c == '.' || c == '_' || c == '-' { c } else { '-' })
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
        let tmp = std::env::temp_dir()
            .join(APP_NAME)
            .display()
            .to_string();

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
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum SqliteMode {
    /// Local file-based SQLite (via `sqlx` or `rusqlite`)
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
    pub name: &'static str,
    /// The CREATE TABLE SQL statement
    pub sql: &'static str,
    /// Optional associated CREATE INDEX statements
    pub indexes: &'static [&'static str],
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
    let normalized: Result<Vec<String>, String> = paths
        .iter()
        .map(|p| db_absolute_path(p))
        .collect();
    serde_json::to_string(&normalized?)
        .map_err(|e| format!("JSON serialization error: {e}"))
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

// ── Helper: list of tables discovered during migration ──────────────────

/// Returns the table names that signal an existing installation.
///
/// # Source
/// Ported from `packages/core/src/database/migration.ts` line 24
/// (`if (tables.some((table) => table.name === "session"))`).
pub fn is_existing_install(tables: &[&str]) -> bool {
    tables.contains(&"session")
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
                Migration { id: "a".into(), up: vec![] },
                Migration { id: "b".into(), up: vec![] },
            ],
        };
        assert_eq!(set.len(), 2);
        assert!(!set.is_empty());
        assert_eq!(set.ids(), vec!["a", "b"]);
    }
}
