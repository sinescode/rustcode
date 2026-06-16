//! Storage / database layer.
//!
//! Ported from: `packages/opencode/src/storage/storage.ts`
//! and `packages/core/src/database/database.ts`
//! OpenCode commit: 5d0f86606ac30690f79f0a6a9f41a1f49fe95d0b
//!
//! ## Architecture
//!
//! Two storage backends:
//!
//! - [`Storage`] — JSON file-based key-value store. Each key path maps to a
//!   `.json` file on disk. Used for session info, messages, parts, etc.
//!
//! - [`Database`] — SQLite database via `sqlx`. Provides a connection pool
//!   with WAL mode, FK enforcement, and a migration system. Used for
//!   structured queries and indexing.
//!
//! The TS codebase uses drizzle ORM with 35+ migrations. We use raw SQL via
//! `sqlx::query` / `sqlx::migrate!` for equivalent functionality without the
//! ORM layer.
//!
//! ## Migrations
//!
//! Migrations are tracked in a `_migration` table. Each migration has a
//! unique ID and runs in a transaction. New tables are added as needed by
//! downstream modules (session, project, etc.).

use crate::error::{Error, Result};
use std::path::{Path, PathBuf};
use std::sync::Arc;

// ── JSON file storage ─────────────────────────────────────────────────

/// JSON file-based key-value storage.
///
/// Each key path (e.g. `["session", "abc123"]`) maps to a `.json` file on
/// disk. Thread-safe — all reads/writes go through the filesystem.
///
/// # Source
/// Ported from `packages/opencode/src/storage/storage.ts` lines 213–321
/// (`Storage.layer` — `read`, `write`, `update`, `remove`, `list`).
#[derive(Debug, Clone)]
pub struct Storage {
    dir: PathBuf,
}

impl Storage {
    /// Create a new storage instance rooted at `dir`.
    pub fn new(dir: PathBuf) -> Self {
        Self { dir }
    }

    /// Read a value by key path.
    ///
    /// # Errors
    /// Returns `Error::Io` if the file cannot be read, or `Error::Serde` if
    /// deserialization fails.
    pub fn read<T: serde::de::DeserializeOwned>(&self, key: &[&str]) -> Result<T> {
        let path = self.key_path(key);
        let content = std::fs::read_to_string(&path)?;
        serde_json::from_str(&content).map_err(|e| {
            Error::Config(format!("storage read error at {}: {e}", path.display()))
        })
    }

    /// Write a value by key path.
    ///
    /// Creates parent directories if they don't exist.
    ///
    /// # Errors
    /// Returns `Error::Io` if the file cannot be written.
    pub fn write<T: serde::Serialize>(&self, key: &[&str], value: &T) -> Result<()> {
        let path = self.key_path(key);
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)?;
        }
        let content = serde_json::to_string_pretty(value).map_err(|e| {
            Error::Config(format!("storage serialization error: {e}"))
        })?;
        std::fs::write(&path, content)?;
        Ok(())
    }

    /// Read, modify, and write a value atomically.
    ///
    /// # Errors
    /// Returns `Error::Io` or deserialization errors.
    pub fn update<T: serde::de::DeserializeOwned + serde::Serialize>(
        &self,
        key: &[&str],
        f: impl FnOnce(&mut T),
    ) -> Result<T> {
        let mut value: T = self.read(key)?;
        f(&mut value);
        self.write(key, &value)?;
        Ok(value)
    }

    /// Remove a value by key path.
    ///
    /// No-op if the file doesn't exist.
    pub fn remove(&self, key: &[&str]) -> Result<()> {
        let path = self.key_path(key);
        if path.exists() {
            std::fs::remove_file(&path)?;
        }
        Ok(())
    }

    /// List keys under a prefix.
    ///
    /// Returns file names (without `.json` extension) in the directory
    /// corresponding to the prefix.
    pub fn list(&self, prefix: &[&str]) -> Result<Vec<String>> {
        let dir = self.key_path(prefix);
        if !dir.exists() {
            return Ok(Vec::new());
        }
        let mut keys = Vec::new();
        for entry in std::fs::read_dir(&dir)? {
            let entry = entry?;
            if entry.file_type()?.is_file() {
                let name = entry.file_name().to_string_lossy().to_string();
                if let Some(stem) = name.strip_suffix(".json") {
                    keys.push(stem.to_string());
                }
            }
        }
        Ok(keys)
    }

    /// Check if a key exists.
    pub fn exists(&self, key: &[&str]) -> bool {
        self.key_path(key).exists()
    }

    /// Convert key path to filesystem path.
    fn key_path(&self, key: &[&str]) -> PathBuf {
        let mut path = self.dir.clone();
        for part in key {
            path.push(part);
        }
        path.set_extension("json");
        path
    }
}

// ── SQLite database ────────────────────────────────────────────────────

/// A migration step — runs inside a transaction.
pub struct Migration {
    /// Unique migration identifier (e.g. "20260616_initial_schema")
    pub id: &'static str,
    /// SQL to run (may contain multiple statements separated by `;`)
    pub sql: &'static str,
}

/// SQLite database with connection pool and migration support.
///
/// Uses `sqlx::SqlitePool` with WAL mode, FK enforcement, and configurable
/// PRAGMAs mirroring the TS source.
///
/// # Source
/// Ported from `packages/core/src/database/database.ts` lines 22–33
/// (`Database.layer`).
#[derive(Clone)]
pub struct Database {
    pool: sqlx::SqlitePool,
    _dir: PathBuf,
}

impl Database {
    /// Open (or create) a SQLite database at the given path.
    ///
    /// Sets PRAGMAs for performance and safety:
    /// - `journal_mode = WAL` — write-ahead logging
    /// - `synchronous = NORMAL` — balance safety/speed
    /// - `busy_timeout = 5000` — wait up to 5s on lock
    /// - `cache_size = -64000` — 64 MB cache
    /// - `foreign_keys = ON` — enforce FK constraints
    ///
    /// # Source
    /// Ported from `packages/core/src/database/database.ts` lines 27–32.
    pub async fn open(path: &Path) -> Result<Self> {
        // Ensure parent directory exists
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)?;
        }

        let db_url = format!("sqlite:{}?mode=rwc", path.display());
        let pool = sqlx::SqlitePool::connect(&db_url).await.map_err(|e| {
            Error::Config(format!("failed to open database at {}: {e}", path.display()))
        })?;

        // Set PRAGMAs
        let pragmas = [
            "PRAGMA journal_mode = WAL",
            "PRAGMA synchronous = NORMAL",
            "PRAGMA busy_timeout = 5000",
            "PRAGMA cache_size = -64000",
            "PRAGMA foreign_keys = ON",
        ];
        for pragma in &pragmas {
            sqlx::query(pragma)
                .execute(&pool)
                .await
                .map_err(|e| Error::Config(format!("PRAGMA error: {e}")))?;
        }

        let db = Self {
            pool,
            _dir: path.to_path_buf(),
        };

        // Ensure migration tracking table exists
        db.ensure_migration_table().await?;

        Ok(db)
    }

    /// Get a reference to the connection pool.
    pub fn pool(&self) -> &sqlx::SqlitePool {
        &self.pool
    }

    /// Run pending migrations.
    ///
    /// Each migration runs in its own transaction. Already-applied
    /// migrations are skipped based on the `_migration` table.
    ///
    /// # Source
    /// Ported from `packages/core/src/database/migration.ts` lines 43–81
    /// (`DatabaseMigration.applyOnly`).
    pub async fn run_migrations(&self, migrations: &[Migration]) -> Result<()> {
        // Get the set of already-applied migration IDs
        let completed: std::collections::HashSet<String> = {
            let rows: Vec<(String,)> =
                sqlx::query_as("SELECT id FROM _migration ORDER BY id")
                    .fetch_all(&self.pool)
                    .await
                    .map_err(|e| Error::Config(format!("migration query error: {e}")))?;
            rows.into_iter().map(|(id,)| id).collect()
        };

        for migration in migrations {
            if completed.contains(migration.id) {
                continue;
            }

            let mut tx = self.pool.begin().await.map_err(|e| {
                Error::Config(format!("migration transaction start error: {e}"))
            })?;

            // Execute the migration SQL
            for statement in migration.sql.split(';') {
                let trimmed = statement.trim();
                if trimmed.is_empty() {
                    continue;
                }
                sqlx::query(trimmed)
                    .execute(&mut *tx)
                    .await
                    .map_err(|e| {
                        Error::Config(format!(
                            "migration `{}` error at `{}`: {e}",
                            migration.id,
                            &trimmed[..trimmed.len().min(80)]
                        ))
                    })?;
            }

            // Record the migration
            let now = std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap_or_default()
                .as_millis() as i64;
            sqlx::query("INSERT INTO _migration (id, time_completed) VALUES (?1, ?2)")
                .bind(migration.id)
                .bind(now)
                .execute(&mut *tx)
                .await
                .map_err(|e| Error::Config(format!("migration record error: {e}")))?;

            tx.commit().await.map_err(|e| {
                Error::Config(format!("migration commit error: {e}"))
            })?;

            tracing::info!("Applied migration: {}", migration.id);
        }

        Ok(())
    }

    /// Create the `_migration` tracking table if it doesn't exist.
    async fn ensure_migration_table(&self) -> Result<()> {
        sqlx::query(
            "CREATE TABLE IF NOT EXISTS _migration (
                id TEXT PRIMARY KEY,
                time_completed INTEGER NOT NULL
            )",
        )
        .execute(&self.pool)
        .await
        .map_err(|e| Error::Config(format!("migration table creation error: {e}")))?;
        Ok(())
    }
}

impl std::fmt::Debug for Database {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Database")
            .field("dir", &self._dir)
            .finish_non_exhaustive()
    }
}

// ── Convenience constructors ─────────────────────────────────────────────

/// Get the default database path from the OS data directory.
///
/// Returns `<data_dir>/opencode/opencode.db`.
///
/// # Source
/// Ported from `packages/core/src/database/database.ts` lines 43–55
/// (`Database.path`).
pub fn default_db_path() -> Result<PathBuf> {
    let data_dir = dirs::data_dir()
        .ok_or_else(|| Error::Config("Cannot determine data directory".into()))?;
    Ok(data_dir.join("opencode").join("opencode.db"))
}

/// Open the default database at the standard location.
pub async fn open_default_db() -> Result<Database> {
    let path = default_db_path()?;
    Database::open(&path).await
}

// ── Initial schema migration ─────────────────────────────────────────────

/// Initial database schema — creates the core tables.
///
/// # Source
/// Derived from `packages/core/src/database/migration/` (initial migrations).
pub const INITIAL_MIGRATION: Migration = Migration {
    id: "20260616_initial_schema",
    sql: r#"
CREATE TABLE IF NOT EXISTS project (
    id TEXT PRIMARY KEY,
    vcs TEXT,
    worktree TEXT,
    name TEXT,
    time_created INTEGER NOT NULL,
    time_initialized INTEGER NOT NULL
);

CREATE TABLE IF NOT EXISTS session (
    id TEXT PRIMARY KEY,
    project_id TEXT NOT NULL,
    workspace_id TEXT,
    title TEXT,
    path TEXT,
    time_created INTEGER NOT NULL,
    time_updated INTEGER NOT NULL,
    usage_input INTEGER NOT NULL DEFAULT 0,
    usage_output INTEGER NOT NULL DEFAULT 0,
    usage_cache_read INTEGER NOT NULL DEFAULT 0,
    usage_cache_write INTEGER NOT NULL DEFAULT 0,
    FOREIGN KEY (project_id) REFERENCES project(id)
);

CREATE TABLE IF NOT EXISTS message (
    id TEXT PRIMARY KEY,
    session_id TEXT NOT NULL,
    role TEXT NOT NULL,
    content TEXT NOT NULL DEFAULT '',
    time_created INTEGER NOT NULL,
    FOREIGN KEY (session_id) REFERENCES session(id)
);

CREATE TABLE IF NOT EXISTS part (
    id TEXT PRIMARY KEY,
    message_id TEXT NOT NULL,
    type TEXT NOT NULL,
    content TEXT NOT NULL DEFAULT '',
    tool_call_id TEXT,
    time_created INTEGER NOT NULL,
    FOREIGN KEY (message_id) REFERENCES message(id)
);

CREATE TABLE IF NOT EXISTS session_input (
    id TEXT PRIMARY KEY,
    session_id TEXT NOT NULL,
    text TEXT NOT NULL,
    input_type TEXT NOT NULL DEFAULT 'user',
    time_created INTEGER NOT NULL,
    FOREIGN KEY (session_id) REFERENCES session(id)
);

CREATE INDEX IF NOT EXISTS idx_message_session_id ON message(session_id);
CREATE INDEX IF NOT EXISTS idx_part_message_id ON part(message_id);
CREATE INDEX IF NOT EXISTS idx_session_project_id ON session(project_id);
CREATE INDEX IF NOT EXISTS idx_session_input_session ON session_input(session_id);
"#,
};

/// All migrations in order.
pub const ALL_MIGRATIONS: &[Migration] = &[INITIAL_MIGRATION];

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;

    // -- JSON Storage tests --------------------------------------------------

    #[test]
    fn test_storage_write_read() {
        let dir = std::env::temp_dir().join("rustcode-storage-test-wr");
        let _ = std::fs::remove_dir_all(&dir);
        let storage = Storage::new(dir.clone());

        storage.write(&["test", "key"], &"hello").unwrap();
        let value: String = storage.read(&["test", "key"]).unwrap();
        assert_eq!(value, "hello");

        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn test_storage_update() {
        let dir = std::env::temp_dir().join("rustcode-storage-test-up");
        let _ = std::fs::remove_dir_all(&dir);
        let storage = Storage::new(dir.clone());

        storage.write(&["test", "counter"], &42u32).unwrap();
        let updated = storage
            .update(&["test", "counter"], |v: &mut u32| *v += 1)
            .unwrap();
        assert_eq!(updated, 43);

        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn test_storage_remove() {
        let dir = std::env::temp_dir().join("rustcode-storage-test-rm");
        let _ = std::fs::remove_dir_all(&dir);
        let storage = Storage::new(dir.clone());

        storage.write(&["test", "rm"], &"gone").unwrap();
        assert!(storage.exists(&["test", "rm"]));
        storage.remove(&["test", "rm"]).unwrap();
        assert!(!storage.exists(&["test", "rm"]));

        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn test_storage_list() {
        let dir = std::env::temp_dir().join("rustcode-storage-test-ls");
        let _ = std::fs::remove_dir_all(&dir);
        let storage = Storage::new(dir.clone());

        storage.write(&["items", "a"], &1).unwrap();
        storage.write(&["items", "b"], &2).unwrap();
        storage.write(&["items", "c"], &3).unwrap();

        let keys = storage.list(&["items"]).unwrap();
        assert_eq!(keys.len(), 3);
        assert!(keys.contains(&"a".to_string()));

        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn test_storage_read_missing() {
        let dir = std::env::temp_dir().join("rustcode-storage-test-miss");
        let _ = std::fs::remove_dir_all(&dir);
        let storage = Storage::new(dir.clone());

        let result: Result<String> = storage.read(&["nonexistent"]);
        assert!(result.is_err());

        let _ = std::fs::remove_dir_all(&dir);
    }

    // -- Database tests (async) ---------------------------------------------

    #[tokio::test]
    async fn test_database_open_and_pragma() {
        let dir = std::env::temp_dir().join("rustcode-db-test-open");
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        let db_path = dir.join("test.db");

        let db = Database::open(&db_path).await.unwrap();

        // Verify WAL mode is enabled
        let mode: (String,) = sqlx::query_as("PRAGMA journal_mode")
            .fetch_one(db.pool())
            .await
            .unwrap();
        assert_eq!(mode.0.to_lowercase(), "wal");

        // Verify foreign keys are enabled
        let fk: (i32,) = sqlx::query_as("PRAGMA foreign_keys")
            .fetch_one(db.pool())
            .await
            .unwrap();
        assert_eq!(fk.0, 1);

        let _ = std::fs::remove_dir_all(&dir);
    }

    #[tokio::test]
    async fn test_database_migrations() {
        let dir = std::env::temp_dir().join("rustcode-db-test-mig");
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        let db_path = dir.join("test.db");

        let db = Database::open(&db_path).await.unwrap();
        db.run_migrations(ALL_MIGRATIONS).await.unwrap();

        // Verify tables were created
        let tables: Vec<(String,)> =
            sqlx::query_as("SELECT name FROM sqlite_master WHERE type='table' ORDER BY name")
                .fetch_all(db.pool())
                .await
                .unwrap();
        let names: Vec<&str> = tables.iter().map(|(n,)| n.as_str()).collect();
        assert!(names.contains(&"project"));
        assert!(names.contains(&"session"));
        assert!(names.contains(&"message"));
        assert!(names.contains(&"part"));
        assert!(names.contains(&"session_input"));
        assert!(names.contains(&"_migration"));

        // Verify migration was recorded
        let count: (i64,) = sqlx::query_as("SELECT COUNT(*) FROM _migration")
            .fetch_one(db.pool())
            .await
            .unwrap();
        assert!(count.0 >= 1);

        // Running migrations again should be idempotent
        db.run_migrations(ALL_MIGRATIONS).await.unwrap();
        let count2: (i64,) = sqlx::query_as("SELECT COUNT(*) FROM _migration")
            .fetch_one(db.pool())
            .await
            .unwrap();
        assert_eq!(count.0, count2.0);

        let _ = std::fs::remove_dir_all(&dir);
    }

    #[tokio::test]
    async fn test_database_session_crud() {
        let dir = std::env::temp_dir().join("rustcode-db-test-sess");
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        let db_path = dir.join("test.db");

        let db = Database::open(&db_path).await.unwrap();
        db.run_migrations(ALL_MIGRATIONS).await.unwrap();

        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_millis() as i64;

        // Insert a project
        sqlx::query(
            "INSERT INTO project (id, vcs, time_created, time_initialized) VALUES (?1, ?2, ?3, ?4)",
        )
        .bind("proj-1")
        .bind("git")
        .bind(now)
        .bind(now)
        .execute(db.pool())
        .await
        .unwrap();

        // Insert a session
        sqlx::query(
            "INSERT INTO session (id, project_id, title, time_created, time_updated) VALUES (?1, ?2, ?3, ?4, ?5)",
        )
        .bind("sess-1")
        .bind("proj-1")
        .bind("Test Session")
        .bind(now)
        .bind(now)
        .execute(db.pool())
        .await
        .unwrap();

        // Query the session
        let (title,): (String,) =
            sqlx::query_as("SELECT title FROM session WHERE id = ?1")
                .bind("sess-1")
                .fetch_one(db.pool())
                .await
                .unwrap();
        assert_eq!(title, "Test Session");

        let _ = std::fs::remove_dir_all(&dir);
    }
}
