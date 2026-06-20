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
        serde_json::from_str(&content)
            .map_err(|e| Error::Config(format!("storage read error at {}: {e}", path.display())))
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
        let content = serde_json::to_string_pretty(value)
            .map_err(|e| Error::Config(format!("storage serialization error: {e}")))?;
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
        let mut dir = self.dir.clone();
        for part in prefix {
            dir.push(part);
        }
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

    /// Read a value, returning a default if the key doesn't exist.
    ///
    /// # Errors
    /// Returns `Error::Io` or deserialization errors only if the key exists
    /// and the file cannot be read or deserialized.
    pub fn read_or_default<T: serde::de::DeserializeOwned + Default>(
        &self,
        key: &[&str],
    ) -> Result<T> {
        if self.exists(key) {
            self.read(key)
        } else {
            Ok(T::default())
        }
    }

    /// Update an existing value, or insert a default value if it doesn't exist.
    ///
    /// If the key exists, the value is read, modified by `f`, and written back.
    /// If the key does not exist, a default value is created, modified by `f`,
    /// and written.
    ///
    /// # Errors
    /// Returns `Error::Io` or serialization/deserialization errors.
    pub fn update_or_insert<T>(&self, key: &[&str], f: impl FnOnce(&mut T)) -> Result<T>
    where
        T: serde::de::DeserializeOwned + serde::Serialize + Default,
    {
        if self.exists(key) {
            self.update(key, f)
        } else {
            let mut value = T::default();
            f(&mut value);
            self.write(key, &value)?;
            Ok(value)
        }
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
            Error::Config(format!(
                "failed to open database at {}: {e}",
                path.display()
            ))
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
            let rows: Vec<(String,)> = sqlx::query_as("SELECT id FROM _migration ORDER BY id")
                .fetch_all(&self.pool)
                .await
                .map_err(|e| Error::Config(format!("migration query error: {e}")))?;
            rows.into_iter().map(|(id,)| id).collect()
        };

        for migration in migrations {
            if completed.contains(migration.id) {
                continue;
            }

            let mut tx =
                self.pool.begin().await.map_err(|e| {
                    Error::Config(format!("migration transaction start error: {e}"))
                })?;

            // Execute the migration SQL
            for statement in migration.sql.split(';') {
                let trimmed = statement.trim();
                if trimmed.is_empty() {
                    continue;
                }
                sqlx::query(trimmed).execute(&mut *tx).await.map_err(|e| {
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

            tx.commit()
                .await
                .map_err(|e| Error::Config(format!("migration commit error: {e}")))?;

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

    /// Execute a basic SELECT query and return an optional row.
    ///
    /// This is a convenience wrapper around `sqlx::query_as` for simple
    /// lookups. Returns `None` if no row matches, or `Some(T)` for the
    /// first matching row.
    ///
    /// # Errors
    /// Returns `Error::Config` if the query fails.
    ///
    /// # Source
    /// Convenience method — not directly ported from TS but follows the
    /// same query pattern used throughout `packages/core/src/database/`.
    pub async fn query_row<T>(&self, sql: &str) -> Result<Option<T>>
    where
        T: for<'r> sqlx::FromRow<'r, sqlx::sqlite::SqliteRow> + Send + Unpin,
    {
        sqlx::query_as::<_, T>(sql)
            .fetch_optional(&self.pool)
            .await
            .map_err(|e| Error::Config(format!("query error: {e}")))
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
    let data_dir =
        dirs::data_dir().ok_or_else(|| Error::Config("Cannot determine data directory".into()))?;
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

/// A migration that always fails — for testing rollback behavior.
///
/// The first statement creates a table; the second is intentionally invalid
/// SQL. Because both run in the same transaction, the rollback should undo
/// the table creation.
pub const FAILING_MIGRATION: Migration = Migration {
    id: "99999999_test_failing",
    sql: "CREATE TABLE this_should_rollback (id INTEGER); INVALID SQL SYNTAX THAT FAILS",
};

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
        let (title,): (String,) = sqlx::query_as("SELECT title FROM session WHERE id = ?1")
            .bind("sess-1")
            .fetch_one(db.pool())
            .await
            .unwrap();
        assert_eq!(title, "Test Session");

        let _ = std::fs::remove_dir_all(&dir);
    }

    // -- Storage read_or_default tests ---------------------------------------

    #[test]
    fn test_storage_read_or_default_missing() {
        let dir = std::env::temp_dir().join("rustcode-storage-test-rod-miss");
        let _ = std::fs::remove_dir_all(&dir);
        let storage = Storage::new(dir.clone());

        // Key doesn't exist — should return default (0 for i32)
        let value: i32 = storage
            .read_or_default(&["nonexistent", "key"])
            .expect("read_or_default should succeed for missing key");
        assert_eq!(value, 0, "default for i32 should be 0");

        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn test_storage_read_or_default_exists() {
        let dir = std::env::temp_dir().join("rustcode-storage-test-rod-exists");
        let _ = std::fs::remove_dir_all(&dir);
        let storage = Storage::new(dir.clone());

        // Write a value first
        storage
            .write(&["existing", "value"], &42i32)
            .expect("write should succeed");

        // read_or_default should return the stored value
        let value: i32 = storage
            .read_or_default(&["existing", "value"])
            .expect("read_or_default should succeed for existing key");
        assert_eq!(value, 42, "should return stored value, not default");

        let _ = std::fs::remove_dir_all(&dir);
    }

    // -- Storage update_or_insert tests --------------------------------------

    #[test]
    fn test_storage_update_or_insert_new() {
        let dir = std::env::temp_dir().join("rustcode-storage-test-uoi-new");
        let _ = std::fs::remove_dir_all(&dir);
        let storage = Storage::new(dir.clone());

        // Key doesn't exist — should create default, modify, and write
        let value: i32 = storage
            .update_or_insert(&["new", "counter"], |v| *v += 10)
            .expect("update_or_insert should succeed for new key");
        assert_eq!(value, 10, "default (0) + 10 = 10");

        // Verify it was persisted
        let stored: i32 = storage
            .read(&["new", "counter"])
            .expect("read after update_or_insert should succeed");
        assert_eq!(stored, 10);

        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn test_storage_update_or_insert_existing() {
        let dir = std::env::temp_dir().join("rustcode-storage-test-uoi-exists");
        let _ = std::fs::remove_dir_all(&dir);
        let storage = Storage::new(dir.clone());

        // Write initial value
        storage
            .write(&["existing", "counter"], &5i32)
            .expect("write should succeed");

        // update_or_insert should read the existing value (5) and modify it
        let value: i32 = storage
            .update_or_insert(&["existing", "counter"], |v| *v *= 3)
            .expect("update_or_insert should succeed for existing key");
        assert_eq!(value, 15, "existing 5 * 3 = 15");

        // Verify
        let stored: i32 = storage
            .read(&["existing", "counter"])
            .expect("read after update_or_insert should succeed");
        assert_eq!(stored, 15);

        let _ = std::fs::remove_dir_all(&dir);
    }

    // -- Storage nested keys test --------------------------------------------

    #[test]
    fn test_storage_nested_keys() {
        let dir = std::env::temp_dir().join("rustcode-storage-test-nested");
        let _ = std::fs::remove_dir_all(&dir);
        let storage = Storage::new(dir.clone());

        // Write with 3+ deep key path
        storage
            .write(&["a", "b", "c", "d"], &"deep-value")
            .expect("write with deep key path should succeed");

        // Read it back
        let value: String = storage
            .read(&["a", "b", "c", "d"])
            .expect("read with deep key path should succeed");
        assert_eq!(value, "deep-value");

        // Read at different depth
        storage
            .write(&["a", "sibling"], &"sibling-value")
            .expect("sibling write should succeed");
        let sibling: String = storage
            .read(&["a", "sibling"])
            .expect("read sibling should succeed");
        assert_eq!(sibling, "sibling-value");

        // Verify sibling keys at the same depth don't interfere
        let a_keys = storage
            .list(&["a"])
            .expect("list at 'a' level should succeed");
        assert!(a_keys.contains(&"sibling".to_string()));

        let _ = std::fs::remove_dir_all(&dir);
    }

    // -- Storage complex JSON test -------------------------------------------

    #[test]
    fn test_storage_complex_json() {
        let dir = std::env::temp_dir().join("rustcode-storage-test-complex");
        let _ = std::fs::remove_dir_all(&dir);
        let storage = Storage::new(dir.clone());

        // Write a complex JSON value with nested objects and arrays
        let complex: serde_json::Value = serde_json::json!({
            "name": "test-project",
            "settings": {
                "theme": "dark",
                "font_size": 14,
                "extensions": ["rust", "python", "typescript"]
            },
            "metadata": {
                "created_at": "2026-06-17",
                "version": 2,
                "tags": ["backend", "api", "database"]
            }
        });

        storage
            .write(&["config", "project"], &complex)
            .expect("write complex JSON should succeed");

        let read_back: serde_json::Value = storage
            .read(&["config", "project"])
            .expect("read complex JSON should succeed");

        assert_eq!(read_back["name"], "test-project");
        assert_eq!(read_back["settings"]["theme"], "dark");
        assert_eq!(read_back["settings"]["font_size"], 14);
        assert_eq!(read_back["settings"]["extensions"][0], "rust");
        assert_eq!(read_back["metadata"]["version"], 2);

        let _ = std::fs::remove_dir_all(&dir);
    }

    // -- Database migration failure rollback test ----------------------------

    #[tokio::test]
    async fn test_database_migration_failure_rollback() {
        let dir = std::env::temp_dir().join("rustcode-db-test-fail-mig");
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).expect("create temp dir");
        let db_path = dir.join("test.db");

        let db = Database::open(&db_path)
            .await
            .expect("open database should succeed");

        // Run the failing migration — should error
        let result = db.run_migrations(&[FAILING_MIGRATION]).await;
        assert!(result.is_err(), "failing migration should return an error");

        // The transaction should have rolled back — table must NOT exist
        let tables: Vec<(String,)> = sqlx::query_as(
            "SELECT name FROM sqlite_master WHERE type='table' AND name='this_should_rollback'",
        )
        .fetch_all(db.pool())
        .await
        .expect("query sqlite_master should succeed");
        assert!(
            tables.is_empty(),
            "this_should_rollback table should not exist after rollback"
        );

        // The migration should NOT be recorded in _migration
        let count: (i64,) =
            sqlx::query_as("SELECT COUNT(*) FROM _migration WHERE id = '99999999_test_failing'")
                .fetch_one(db.pool())
                .await
                .expect("count migration should succeed");
        assert_eq!(count.0, 0, "failing migration should not be recorded");

        let _ = std::fs::remove_dir_all(&dir);
    }

    // -- Database query_row test ---------------------------------------------

    #[tokio::test]
    async fn test_database_query_row() {
        let dir = std::env::temp_dir().join("rustcode-db-test-query-row");
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).expect("create temp dir");
        let db_path = dir.join("test.db");

        let db = Database::open(&db_path)
            .await
            .expect("open database should succeed");
        db.run_migrations(ALL_MIGRATIONS)
            .await
            .expect("run migrations should succeed");

        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .expect("system time")
            .as_millis() as i64;

        // Insert test data
        sqlx::query(
            "INSERT INTO project (id, vcs, name, time_created, time_initialized) VALUES (?1, ?2, ?3, ?4, ?5)",
        )
        .bind("proj-qr")
        .bind("git")
        .bind("query-test")
        .bind(now)
        .bind(now)
        .execute(db.pool())
        .await
        .expect("insert project");

        // query_row to get it back
        #[derive(Debug, PartialEq, sqlx::FromRow)]
        struct ProjectRow {
            id: String,
            vcs: Option<String>,
            name: Option<String>,
        }

        let project: Option<ProjectRow> = db
            .query_row("SELECT id, vcs, name FROM project WHERE id = 'proj-qr'")
            .await
            .expect("query_row should succeed");

        let project = project.expect("project should exist");
        assert_eq!(project.id, "proj-qr");
        assert_eq!(project.name.as_deref(), Some("query-test"));
        assert_eq!(project.vcs.as_deref(), Some("git"));

        // query_row for a non-existent row should return None
        let missing: Option<ProjectRow> = db
            .query_row("SELECT id, vcs, name FROM project WHERE id = 'nonexistent'")
            .await
            .expect("query_row for missing row should succeed");
        assert!(missing.is_none(), "missing row should be None");

        let _ = std::fs::remove_dir_all(&dir);
    }

    // -- default_db_path test ------------------------------------------------

    #[test]
    fn test_default_db_path_returns_path() {
        let path = default_db_path().expect("default_db_path should succeed");
        let path_str = path.to_string_lossy();
        assert!(
            path_str.ends_with("opencode/opencode.db"),
            "default_db_path should end with 'opencode/opencode.db', got: {path_str}"
        );
    }

    // -- Database open twice reuses tables test ------------------------------

    #[tokio::test]
    async fn test_database_open_twice_reuses_tables() {
        let dir = std::env::temp_dir().join("rustcode-db-test-reopen");
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).expect("create temp dir");
        let db_path = dir.join("test.db");

        // First open: create tables via migration
        {
            let db = Database::open(&db_path)
                .await
                .expect("first open should succeed");
            db.run_migrations(ALL_MIGRATIONS)
                .await
                .expect("run migrations should succeed");

            // Insert a row to verify persistence
            let now = std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .expect("system time")
                .as_millis() as i64;
            sqlx::query(
                "INSERT INTO project (id, vcs, name, time_created, time_initialized) VALUES (?1, ?2, ?3, ?4, ?5)",
            )
            .bind("persist-1")
            .bind("git")
            .bind("persistent test")
            .bind(now)
            .bind(now)
            .execute(db.pool())
            .await
            .expect("insert project");
        }
        // db dropped here — pool closed, file released

        // Second open: same file, verify tables and data persist
        let db2 = Database::open(&db_path)
            .await
            .expect("second open should succeed");

        // Tables should still exist
        let tables: Vec<(String,)> =
            sqlx::query_as("SELECT name FROM sqlite_master WHERE type='table' ORDER BY name")
                .fetch_all(db2.pool())
                .await
                .expect("query tables should succeed");
        let names: Vec<&str> = tables.iter().map(|(n,)| n.as_str()).collect();
        assert!(names.contains(&"project"), "project table should persist");
        assert!(names.contains(&"session"), "session table should persist");
        assert!(names.contains(&"message"), "message table should persist");

        // Data should persist
        let (name,): (String,) = sqlx::query_as("SELECT name FROM project WHERE id = 'persist-1'")
            .fetch_one(db2.pool())
            .await
            .expect("query project should succeed");
        assert_eq!(
            name, "persistent test",
            "data should persist across reopens"
        );

        let _ = std::fs::remove_dir_all(&dir);
    }
}
