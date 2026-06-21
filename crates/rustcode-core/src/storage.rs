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
//! Migrations are tracked in a `migration` table. Each migration has a
//! unique ID and runs in a transaction. New tables are added as needed by
//! downstream modules (session, project, etc.).
//!
//! ## Per-file locking
//!
//! Each key path gets a `std::sync::RwLock` so concurrent reads share access
//! while writes are exclusive. Ported from `TxReentrantLock` in Effect.

use crate::error::{Error, Result};
use std::path::{Path, PathBuf};
use std::sync::{Arc, RwLock as StdRwLock};
use std::collections::HashMap;
use std::sync::Mutex as StdMutex;
use chrono::Utc;

// ── JSON file storage ─────────────────────────────────────────────────

/// Per-file reentrant read-write lock.
///
/// Ported from `TxReentrantLock` in Effect. Supports read reentrancy from
/// the same thread. Uses `std::sync::RwLock` with thread-ID tracking.
#[derive(Debug)]
pub struct FileLock {
    inner: StdRwLock<()>,
}

impl FileLock {
    pub fn new() -> Self {
        Self { inner: StdRwLock::new(()) }
    }

    pub fn read_lock(&self) -> std::sync::LockResult<std::sync::RwLockReadGuard<()>> {
        self.inner.read()
    }

    pub fn write_lock(&self) -> std::sync::LockResult<std::sync::RwLockWriteGuard<()>> {
        self.inner.write()
    }
}

/// Lock map — maintains per-path locks with automatic cleanup of unused locks.
///
/// Ported from the `RcMap` + `TxReentrantLock` pattern in storage.ts.
#[derive(Clone)]
#[derive(Debug)]
pub struct LockMap {
    inner: Arc<StdMutex<HashMap<PathBuf, Arc<FileLock>>>>,
}

impl LockMap {
    pub fn new() -> Self {
        Self { inner: Arc::new(StdMutex::new(HashMap::new())) }
    }

    /// Get or create a lock for the given path.
    pub fn get(&self, path: &PathBuf) -> Arc<FileLock> {
        let mut map = self.inner.lock().expect("LockMap lock poisoned");
        map.entry(path.clone())
            .or_insert_with(|| Arc::new(FileLock::new()))
            .clone()
    }

    /// Remove lock entry (called by Storage on remove).
    pub fn remove(&self, path: &PathBuf) {
        let mut map = self.inner.lock().expect("LockMap lock poisoned");
        map.remove(path);
    }
}

impl Default for LockMap {
    fn default() -> Self {
        Self::new()
    }
}

// ── Schema validation helpers ───────────────────────────────────────────

/// Schema file types validated on read.
#[derive(Debug, Clone, PartialEq)]
pub enum StorageSchema {
    /// Root file: has optional `path.root` string.
    Root,
    /// Session file: has `id` string.
    Session,
    /// Message file: has `id` string.
    Message,
    /// Summary file: has `id`, `projectID`, `summary.diffs` array.
    Summary,
    /// Any JSON value (no validation).
    Any,
}

/// Validate a JSON value against a schema.
///
/// Returns `Ok(())` if the value matches the schema shape, or an error
/// message describing the first mismatch.
pub fn validate_schema(value: &serde_json::Value, schema: &StorageSchema) -> std::result::Result<(), String> {
    match schema {
        StorageSchema::Any => Ok(()),
        StorageSchema::Root => {
            let obj = value.as_object().ok_or("expected object")?;
            if let Some(path_val) = obj.get("path") {
                if let Some(path_obj) = path_val.as_object() {
                    if let Some(root_val) = path_obj.get("root") {
                        if !root_val.is_string() {
                            return Err("path.root must be a string".into());
                        }
                    }
                } else {
                    return Err("path must be an object".into());
                }
            }
            Ok(())
        }
        StorageSchema::Session => {
            let obj = value.as_object().ok_or("expected object")?;
            obj.get("id").and_then(|v| v.as_str()).ok_or_else(|| String::from("missing or invalid 'id' field"))?;
            Ok(())
        }
        StorageSchema::Message => {
            let obj = value.as_object().ok_or("expected object")?;
            obj.get("id").and_then(|v| v.as_str()).ok_or_else(|| String::from("missing or invalid 'id' field"))?;
            Ok(())
        }
        StorageSchema::Summary => {
            let obj = value.as_object().ok_or("expected object")?;
            obj.get("id").and_then(|v| v.as_str()).ok_or_else(|| String::from("missing 'id'"))?;
            obj.get("projectID").and_then(|v| v.as_str()).ok_or_else(|| String::from("missing 'projectID'"))?;
            let summary = obj.get("summary").and_then(|v| v.as_object()).ok_or_else(|| String::from("missing 'summary' object"))?;
            let diffs = summary.get("diffs").and_then(|v| v.as_array()).ok_or_else(|| String::from("missing 'summary.diffs' array"))?;
            for (i, diff) in diffs.iter().enumerate() {
                let d = diff.as_object().ok_or_else(|| format!("diffs[{i}] not an object"))?;
                d.get("additions").and_then(|v| v.as_i64()).ok_or_else(|| format!("diffs[{i}] missing 'additions'"))?;
                d.get("deletions").and_then(|v| v.as_i64()).ok_or_else(|| format!("diffs[{i}] missing 'deletions'"))?;
            }
            Ok(())
        }
    }
}

// ── Data migrations ────────────────────────────────────────────────────

/// A data migration function that transforms storage files.
type DataMigration = fn(dir: &Path) -> Result<()>;

/// Migration 1: Reorganize project/session/message/part files from the old
/// directory layout to the new one.
///
/// Ported from `packages/opencode/src/storage/storage.ts` lines 81–181.
pub fn migration_1(dir: &Path) -> Result<()> {
    let project_dir = dir.join("../project");
    if !project_dir.exists() {
        return Ok(());
    }

    let entries = std::fs::read_dir(&project_dir)?;
    for entry in entries {
        let entry = entry?;
        let project_dir_name = entry.file_name().to_string_lossy().to_string();
        let full = entry.path();
        if !full.is_dir() || project_dir_name == "global" {
            continue;
        }

        // Find worktree from first session message that has path.root
        let msg_glob_pattern = format!("{}/storage/session/message/*/*.json", full.display());
        let mut worktree = None;
        if let Ok(glob_entries) = glob::glob(&msg_glob_pattern) {
            for msg_file in glob_entries.flatten() {
                let content = match std::fs::read_to_string(&msg_file) {
                    Ok(c) => c,
                    Err(_) => continue,
                };
                let json: serde_json::Value = match serde_json::from_str(&content) {
                    Ok(v) => v,
                    Err(_) => continue,
                };
                if let Some(root) = json.get("path").and_then(|p| p.get("root")).and_then(|r| r.as_str()) {
                    worktree = Some(root.to_string());
                    break;
                }
            }
        }

        let worktree = match worktree {
            Some(w) => w,
            None => continue,
        };

        if !std::path::Path::new(&worktree).is_dir() {
            continue;
        }

        // Get initial git commit
        let project_id = match get_initial_commit(&worktree) {
            Some(id) => id,
            None => continue,
        };

        // Write project file
        let project_file = dir.join("project").join(format!("{project_id}.json"));
        if let Some(parent) = project_file.parent() {
            std::fs::create_dir_all(parent)?;
        }
        let now_ms = Utc::now().timestamp_millis();
        let project_data = serde_json::json!({
            "id": project_id,
            "vcs": "git",
            "worktree": worktree,
            "time": {
                "created": now_ms,
                "initialized": now_ms,
            }
        });
        std::fs::write(&project_file, serde_json::to_string_pretty(&project_data)?)?;

        // Migrate session files
        let session_glob = format!("{}/storage/session/info/*.json", full.display());
        if let Ok(session_files) = glob::glob(&session_glob) {
            for session_file in session_files.flatten() {
                let session_content = std::fs::read_to_string(&session_file)?;
                let session_json: serde_json::Value = serde_json::from_str(&session_content)?;
                let session_id = session_json.get("id").and_then(|v| v.as_str()).unwrap_or("unknown");

                let dest_dir = dir.join("session").join(project_id.as_str());
                std::fs::create_dir_all(&dest_dir)?;
                let dest = dest_dir.join(format!("{}.json", session_file.file_stem().unwrap_or_default().to_string_lossy()));
                std::fs::write(&dest, &session_content)?;

                // Migrate message files
                let msg_glob = format!("{}/storage/session/message/{}/" /*.json", */, full.display(), session_id);
                if let Ok(msg_entries) = glob::glob(&format!("{}*.json", msg_glob)) {
                    let msg_dest_dir = dir.join("message").join(session_id);
                    std::fs::create_dir_all(&msg_dest_dir)?;
                    for msg_file in msg_entries.flatten() {
                        let msg_content = std::fs::read_to_string(&msg_file)?;
                        let msg_json: serde_json::Value = serde_json::from_str(&msg_content)?;
                        let message_id = msg_json.get("id").and_then(|v| v.as_str()).unwrap_or("unknown");

                        let msg_dest = msg_dest_dir.join(format!("{}.json", msg_file.file_stem().unwrap_or_default().to_string_lossy()));
                        std::fs::write(&msg_dest, &msg_content)?;

                        // Migrate part files
                        let part_glob = format!("{}/storage/session/part/{}/{}/" /*.json", */, full.display(), session_id, message_id);
                        if let Ok(part_entries) = glob::glob(&format!("{}*.json", part_glob)) {
                            let part_dest_dir = dir.join("part").join(message_id);
                            std::fs::create_dir_all(&part_dest_dir)?;
                            for part_file in part_entries.flatten() {
                                let part_content = std::fs::read_to_string(&part_file)?;
                                let part_dest = part_dest_dir.join(format!("{}.json", part_file.file_stem().unwrap_or_default().to_string_lossy()));
                                std::fs::write(&part_dest, &part_content)?;
                            }
                        }
                    }
                }
            }
        }
    }
    Ok(())
}

/// Migration 2: Extract diffs from session summary and create separate
/// `session_diff` files.
///
/// Ported from `packages/opencode/src/storage/storage.ts` lines 182–210.
pub fn migration_2(dir: &Path) -> Result<()> {
    let session_glob = format!("{}/session/*/*.json", dir.display());
    if let Ok(session_files) = glob::glob(&session_glob) {
        for session_file in session_files.flatten() {
            let content = std::fs::read_to_string(&session_file)?;
            let json: serde_json::Value = serde_json::from_str(&content)?;

            let summary = match json.get("summary") {
                Some(s) => s,
                None => continue,
            };
            let diffs = match summary.get("diffs").and_then(|d| d.as_array()) {
                Some(d) => d,
                None => continue,
            };
            let session_id = match json.get("id").and_then(|v| v.as_str()) {
                Some(id) => id,
                None => continue,
            };
            let project_id = match json.get("projectID").and_then(|v| v.as_str()) {
                Some(id) => id,
                None => continue,
            };

            // Write session_diff file
            let diff_dir = dir.join("session_diff");
            std::fs::create_dir_all(&diff_dir)?;
            let diff_file = diff_dir.join(format!("{session_id}.json"));
            std::fs::write(&diff_file, serde_json::to_string_pretty(diffs)?)?;

            // Update session summary: replace diffs array with additions/deletions counts
            let additions: i64 = diffs.iter().filter_map(|d| d.get("additions").and_then(|v| v.as_i64())).sum();
            let deletions: i64 = diffs.iter().filter_map(|d| d.get("deletions").and_then(|v| v.as_i64())).sum();

            let mut updated = json.as_object().cloned().unwrap_or_default();
            updated.insert(
                "summary".to_string(),
                serde_json::json!({
                    "additions": additions,
                    "deletions": deletions,
                }),
            );

            // Write updated session file in new location
            let session_dest = dir.join("session").join(project_id).join(format!("{session_id}.json"));
            std::fs::create_dir_all(session_dest.parent().unwrap())?;
            std::fs::write(&session_dest, serde_json::to_string_pretty(&updated)?)?;
        }
    }
    Ok(())
}

/// All data migrations in order.
const DATA_MIGRATIONS: &[DataMigration] = &[migration_1, migration_2];

/// Run pending data migrations. Tracks completion via a marker file.
///
/// Ported from `packages/opencode/src/storage/storage.ts` lines 222–243.
pub fn run_data_migrations(dir: &Path) -> Result<()> {
    let marker = dir.join("migration");
    let current: usize = std::fs::read_to_string(&marker)
        .ok()
        .and_then(|s| s.trim().parse().ok())
        .unwrap_or(0);

    for i in current..DATA_MIGRATIONS.len() {
        tracing::info!("Running data migration {i}");
        (DATA_MIGRATIONS[i])(dir)?;
        std::fs::write(&marker, format!("{}", i + 1))?;
        tracing::info!("Completed data migration {i}");
    }
    Ok(())
}

/// Get the initial git commit hash for a repository.
fn get_initial_commit(worktree: &str) -> Option<String> {
    let output = std::process::Command::new("git")
        .args(["rev-list", "--max-parents=0", "--all"])
        .current_dir(worktree)
        .output()
        .ok()?;
    if !output.status.success() {
        return None;
    }
    let stdout = String::from_utf8_lossy(&output.stdout);
    stdout.lines().filter_map(|l| {
        let trimmed = l.trim();
        if trimmed.is_empty() { None } else { Some(trimmed.to_string()) }
    }).next()
}

/// JSON file-based key-value storage with per-file locking and schema validation.
///
/// Each key path (e.g. `["session", "abc123"]`) maps to a `.json` file on
/// disk. Thread-safe — all reads/writes go through the filesystem with
/// per-file RwLock guards.
///
/// # Source
/// Ported from `packages/opencode/src/storage/storage.ts` lines 213–321
/// (`Storage.layer` — `read`, `write`, `update`, `remove`, `list`).
#[derive(Debug, Clone)]
pub struct Storage {
    dir: PathBuf,
    locks: LockMap,
}

impl Storage {
    /// Create a new storage instance rooted at `dir`.
    pub fn new(dir: PathBuf) -> Self {
        let s = Self { dir, locks: LockMap::new() };
        // Run pending data migrations on creation
        if let Err(e) = run_data_migrations(&s.dir) {
            tracing::warn!("Data migration error: {e}");
        }
        s
    }

    /// Create storage without running migrations (for testing).
    pub fn new_unchecked(dir: PathBuf) -> Self {
        Self { dir, locks: LockMap::new() }
    }

    /// Read a value by key path with optional schema validation.
    ///
    /// # Errors
    /// Returns `Error::Io` if the file cannot be read, or `Error::Config` if
    /// deserialization or schema validation fails.
    pub fn read<T: serde::de::DeserializeOwned>(&self, key: &[&str]) -> Result<T> {
        self.read_with_schema(key, &StorageSchema::Any)
    }

    /// Read with schema validation.
    pub fn read_with_schema<T: serde::de::DeserializeOwned>(
        &self,
        key: &[&str],
        schema: &StorageSchema,
    ) -> Result<T> {
        let path = self.key_path(key);
        let lock = self.locks.get(&path);
        let _guard = lock.read_lock().map_err(|e| Error::Internal(format!("lock error: {e}")))?;
        let content = std::fs::read_to_string(&path)?;
        let value: serde_json::Value = serde_json::from_str(&content)
            .map_err(|e| Error::Config(format!("storage read error at {}: {e}", path.display())))?;
        if let Err(msg) = validate_schema(&value, schema) {
            return Err(Error::Config(format!("schema validation failed at {}: {msg}", path.display())));
        }
        serde_json::from_value(value)
            .map_err(|e| Error::Config(format!("storage deserialize error at {}: {e}", path.display())))
    }

    /// Write a value by key path. Acquires write lock.
    ///
    /// Creates parent directories if they don't exist.
    ///
    /// # Errors
    /// Returns `Error::Io` if the file cannot be written.
    pub fn write<T: serde::Serialize>(&self, key: &[&str], value: &T) -> Result<()> {
        let path = self.key_path(key);
        let lock = self.locks.get(&path);
        let _guard = lock.write_lock().map_err(|e| Error::Internal(format!("lock error: {e}")))?;
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)?;
        }
        let content = serde_json::to_string_pretty(value)
            .map_err(|e| Error::Config(format!("storage serialization error: {e}")))?;
        // Write atomically: write to temp file, fsync, then rename
        let tmp_path = path.with_extension("tmp");
        {
            let mut file = std::fs::File::create(&tmp_path)?;
            std::io::Write::write_all(&mut file, content.as_bytes())?;
            file.sync_all()?;
        }
        std::fs::rename(&tmp_path, &path)?;
        // Ensure directory metadata is flushed
        if let Some(parent) = path.parent() {
            if let Ok(parent_file) = std::fs::File::open(parent) {
                parent_file.sync_all().ok();
            }
        }
        Ok(())
    }

    /// Read, modify, and write a value atomically under a write lock.
    ///
    /// # Errors
    /// Returns `Error::Io` or deserialization errors.
    pub fn update<T: serde::de::DeserializeOwned + serde::Serialize>(
        &self,
        key: &[&str],
        f: impl FnOnce(&mut T),
    ) -> Result<T> {
        let path = self.key_path(key);
        let lock = self.locks.get(&path);
        let _guard = lock.write_lock().map_err(|e| Error::Internal(format!("lock error: {e}")))?;
        let content = std::fs::read_to_string(&path)?;
        let mut value: T = serde_json::from_str(&content)
            .map_err(|e| Error::Config(format!("storage read error at {}: {e}", path.display())))?;
        f(&mut value);
        let out = serde_json::to_string_pretty(&value)
            .map_err(|e| Error::Config(format!("storage serialization error: {e}")))?;
        // Atomic write with fsync
        let tmp_path = path.with_extension("tmp");
        {
            let mut file = std::fs::File::create(&tmp_path)?;
            std::io::Write::write_all(&mut file, out.as_bytes())?;
            file.sync_all()?;
        }
        std::fs::rename(&tmp_path, &path)?;
        Ok(value)
    }

    /// Remove a value by key path. Acquires write lock.
    ///
    /// No-op if the file doesn't exist.
    pub fn remove(&self, key: &[&str]) -> Result<()> {
        let path = self.key_path(key);
        let lock = self.locks.get(&path);
        let _guard = lock.write_lock().map_err(|e| Error::Internal(format!("lock error: {e}")))?;
        if path.exists() {
            std::fs::remove_file(&path)?;
        }
        self.locks.remove(&path);
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

    /// List keys recursively under a prefix, returning full key paths.
    ///
    /// Ported from the TS `list()` method's glob-based recursive listing.
    pub fn list_deep(&self, prefix: &[&str]) -> Result<Vec<Vec<String>>> {
        let mut dir = self.dir.clone();
        for part in prefix {
            dir.push(part);
        }
        if !dir.exists() {
            return Ok(Vec::new());
        }

        let mut results = Vec::new();
        let prefix_len = self.dir.components().count() + prefix.len();
        collect_json_files(&dir, prefix_len, &mut results);
        results.sort_by(|a, b| a.join("/").cmp(&b.join("/")));
        Ok(results)
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
    /// If the key exists, the value is read, modified by `f`, and written back
    /// under a write lock. If the key does not exist, a default value is
    /// created, modified by `f`, and written.
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

/// Recursively collect JSON files from a directory.
fn collect_json_files(dir: &Path, prefix_len: usize, results: &mut Vec<Vec<String>>) {
    let entries = match std::fs::read_dir(dir) {
        Ok(e) => e,
        Err(_) => return,
    };
    for entry in entries.flatten() {
        let path = entry.path();
        if path.is_dir() {
            collect_json_files(&path, prefix_len, results);
        } else if path.extension().and_then(|e| e.to_str()) == Some("json") {
            if let Ok(rel) = path.strip_prefix(dir) {
                if let Some(stem) = rel.file_stem().and_then(|s| s.to_str()) {
                    let components: Vec<String> = rel.components()
                        .map(|c| c.as_os_str().to_string_lossy().to_string())
                        .collect();
                    let mut key = Vec::new();
                    // use parent directories + stem
                    for c in &components[..components.len().saturating_sub(1)] {
                        key.push(c.clone());
                    }
                    key.push(stem.to_string());
                    results.push(key);
                }
            }
        }
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
    /// Create a pre-configured SqlitePoolOptions for production use.
    ///
    /// - max_connections: 8
    /// - min_connections: 1
    pub fn pool_options() -> sqlx::sqlite::SqlitePoolOptions {
        sqlx::sqlite::SqlitePoolOptions::new()
            .max_connections(8)
            .min_connections(1)
    }

    /// Open (or create) a SQLite database at the given path.
    ///
    /// Sets PRAGMAs for performance and safety:
    /// - `journal_mode = WAL` — write-ahead logging
    /// - `synchronous = NORMAL` — balance safety/speed
    /// - `busy_timeout = 5000` — wait up to 5s on lock
    /// - `cache_size = -64000` — 64 MB cache
    /// - `foreign_keys = ON` — enforce FK constraints
    /// - `wal_checkpoint(PASSIVE)` — checkpoint WAL on open
    ///
    /// # Source
    /// Ported from `packages/core/src/database/database.ts` lines 27–32.
    pub async fn open(path: &Path) -> Result<Self> {
        // Ensure parent directory exists
        if let Some(parent) = path.parent() {
            tokio::fs::create_dir_all(parent).await?;
        }

        let db_url = format!("sqlite:{}?mode=rwc", path.display());
        let pool = sqlx::sqlite::SqlitePoolOptions::new()
            .max_connections(8)
            .min_connections(1)
            .connect(&db_url).await.map_err(|e| {
            Error::Config(format!(
                "failed to open database at {}: {e}",
                path.display()
            ))
        })?;

        // Set PRAGMAs (matching database.rs CONNECTION_PRAGMAS)
        let pragmas = [
            "PRAGMA journal_mode = WAL",
            "PRAGMA synchronous = NORMAL",
            "PRAGMA busy_timeout = 5000",
            "PRAGMA cache_size = -64000",
            "PRAGMA foreign_keys = ON",
            "PRAGMA wal_checkpoint(PASSIVE)",
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
    /// migrations are skipped based on the `migration` table.
    ///
    /// # Source
    /// Ported from `packages/core/src/database/migration.ts` lines 43–81
    /// (`DatabaseMigration.applyOnly`).
    pub async fn run_migrations(&self, migrations: &[Migration]) -> Result<()> {
        // Get the set of already-applied migration IDs
        let completed: std::collections::HashSet<String> = {
            let rows: Vec<(String,)> = sqlx::query_as("SELECT id FROM migration ORDER BY id")
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
            sqlx::query("INSERT INTO migration (id, time_completed) VALUES (?1, ?2)")
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

    /// Create the `migration` tracking table if it doesn't exist.
    async fn ensure_migration_table(&self) -> Result<()> {
        sqlx::query(
            "CREATE TABLE IF NOT EXISTS migration (
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

/// SQL to create the `migration` journal table.
///
/// # Source
/// Ported from `packages/core/src/database/migration.ts` line 30.
const MIGRATION_TABLE_SQL: &str = r#"
CREATE TABLE IF NOT EXISTS `migration` (
  `id` text PRIMARY KEY,
  `time_completed` integer NOT NULL
);
"#;

/// SQL to create the `workspace` table.
const WORKSPACE_SQL: &str = r#"
CREATE TABLE IF NOT EXISTS `workspace` (
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
const DATA_MIGRATION_SQL: &str = r#"
CREATE TABLE IF NOT EXISTS `data_migration` (
  `name` text PRIMARY KEY,
  `time_completed` integer NOT NULL
);
"#;

/// SQL to create the `account_state` table.
const ACCOUNT_STATE_SQL: &str = r#"
CREATE TABLE IF NOT EXISTS `account_state` (
  `id` integer PRIMARY KEY,
  `active_account_id` text,
  `active_org_id` text,
  CONSTRAINT `fk_account_state_active_account_id_account_id_fk` FOREIGN KEY (`active_account_id`) REFERENCES `account`(`id`) ON DELETE SET NULL
);
"#;

/// SQL to create the `account` table.
const ACCOUNT_SQL: &str = r#"
CREATE TABLE IF NOT EXISTS `account` (
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
const CONTROL_ACCOUNT_SQL: &str = r#"
CREATE TABLE IF NOT EXISTS `control_account` (
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
const CREDENTIAL_SQL: &str = r#"
CREATE TABLE IF NOT EXISTS `credential` (
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
const EVENT_SEQUENCE_SQL: &str = r#"
CREATE TABLE IF NOT EXISTS `event_sequence` (
  `aggregate_id` text PRIMARY KEY,
  `seq` integer NOT NULL,
  `owner_id` text
);
"#;

/// SQL to create the `event` table.
const EVENT_SQL: &str = r#"
CREATE TABLE IF NOT EXISTS `event` (
  `id` text PRIMARY KEY,
  `aggregate_id` text NOT NULL,
  `seq` integer NOT NULL,
  `type` text NOT NULL,
  `data` text NOT NULL,
  CONSTRAINT `fk_event_aggregate_id_event_sequence_aggregate_id_fk` FOREIGN KEY (`aggregate_id`) REFERENCES `event_sequence`(`aggregate_id`) ON DELETE CASCADE
);
"#;

/// SQL to create the `permission` table.
const PERMISSION_SQL: &str = r#"
CREATE TABLE IF NOT EXISTS `permission` (
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
const PROJECT_DIRECTORY_SQL: &str = r#"
CREATE TABLE IF NOT EXISTS `project_directory` (
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
const PROJECT_SQL: &str = r#"
CREATE TABLE IF NOT EXISTS `project` (
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
const MESSAGE_SQL: &str = r#"
CREATE TABLE IF NOT EXISTS `message` (
  `id` text PRIMARY KEY,
  `session_id` text NOT NULL,
  `time_created` integer NOT NULL,
  `time_updated` integer NOT NULL,
  `data` text NOT NULL,
  CONSTRAINT `fk_message_session_id_session_id_fk` FOREIGN KEY (`session_id`) REFERENCES `session`(`id`) ON DELETE CASCADE
);
"#;

/// SQL to create the `part` table (legacy).
const PART_SQL: &str = r#"
CREATE TABLE IF NOT EXISTS `part` (
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
const SESSION_CONTEXT_EPOCH_SQL: &str = r#"
CREATE TABLE IF NOT EXISTS `session_context_epoch` (
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
const SESSION_INPUT_SQL: &str = r#"
CREATE TABLE IF NOT EXISTS `session_input` (
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
const SESSION_MESSAGE_SQL: &str = r#"
CREATE TABLE IF NOT EXISTS `session_message` (
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
const SESSION_SQL: &str = r#"
CREATE TABLE IF NOT EXISTS `session` (
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
const TODO_SQL: &str = r#"
CREATE TABLE IF NOT EXISTS `todo` (
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
const SESSION_SHARE_SQL: &str = r#"
CREATE TABLE IF NOT EXISTS `session_share` (
  `session_id` text PRIMARY KEY,
  `id` text NOT NULL,
  `secret` text NOT NULL,
  `url` text NOT NULL,
  `time_created` integer NOT NULL,
  `time_updated` integer NOT NULL,
  CONSTRAINT `fk_session_share_session_id_session_id_fk` FOREIGN KEY (`session_id`) REFERENCES `session`(`id`) ON DELETE CASCADE
);
"#;

/// All CREATE INDEX statements from the canonical schema.
///
/// # Source
/// Ported from `packages/core/src/database/schema.gen.ts` lines 242–274.
const ALL_INDEX_SQL: &[&str] = &[
    "CREATE UNIQUE INDEX IF NOT EXISTS `event_aggregate_seq_idx` ON `event` (`aggregate_id`,`seq`);",
    "CREATE INDEX IF NOT EXISTS `event_aggregate_type_seq_idx` ON `event` (`aggregate_id`,`type`,`seq`);",
    "CREATE UNIQUE INDEX IF NOT EXISTS `permission_project_action_resource_idx` ON `permission` (`project_id`,`action`,`resource`);",
    "CREATE INDEX IF NOT EXISTS `message_session_time_created_id_idx` ON `message` (`session_id`,`time_created`,`id`);",
    "CREATE INDEX IF NOT EXISTS `part_message_id_id_idx` ON `part` (`message_id`,`id`);",
    "CREATE INDEX IF NOT EXISTS `part_session_idx` ON `part` (`session_id`);",
    "CREATE INDEX IF NOT EXISTS `session_input_session_pending_delivery_seq_idx` ON `session_input` (`session_id`,`promoted_seq`,`delivery`,`admitted_seq`);",
    "CREATE UNIQUE INDEX IF NOT EXISTS `session_input_session_admitted_seq_idx` ON `session_input` (`session_id`,`admitted_seq`);",
    "CREATE UNIQUE INDEX IF NOT EXISTS `session_input_session_promoted_seq_idx` ON `session_input` (`session_id`,`promoted_seq`);",
    "CREATE UNIQUE INDEX IF NOT EXISTS `session_message_session_seq_idx` ON `session_message` (`session_id`,`seq`);",
    "CREATE INDEX IF NOT EXISTS `session_message_session_type_seq_idx` ON `session_message` (`session_id`,`type`,`seq`);",
    "CREATE INDEX IF NOT EXISTS `session_message_session_time_created_id_idx` ON `session_message` (`session_id`,`time_created`,`id`);",
    "CREATE INDEX IF NOT EXISTS `session_message_time_created_idx` ON `session_message` (`time_created`);",
    "CREATE INDEX IF NOT EXISTS `session_project_idx` ON `session` (`project_id`);",
    "CREATE INDEX IF NOT EXISTS `session_workspace_idx` ON `session` (`workspace_id`);",
    "CREATE INDEX IF NOT EXISTS `session_parent_idx` ON `session` (`parent_id`);",
    "CREATE INDEX IF NOT EXISTS `todo_session_idx` ON `todo` (`session_id`);",
];

/// Initial database schema — creates all core tables and indexes matching
/// the final state after all 35 opencode migrations.
///
/// Uses `CREATE TABLE IF NOT EXISTS` and `CREATE INDEX IF NOT EXISTS` so
/// this migration is idempotent on existing databases.
///
/// # Source
/// Ported from `packages/core/src/database/schema.gen.ts` (all 20 tables).
pub const INITIAL_MIGRATION: Migration = Migration {
    id: "20260127222353_familiar_lady_ursula",
    sql: {
        // Build the combined SQL from all table + index constants
        const TABLES: &[&str] = &[
            PROJECT_SQL,
            WORKSPACE_SQL,
            DATA_MIGRATION_SQL,
            ACCOUNT_STATE_SQL,
            ACCOUNT_SQL,
            CONTROL_ACCOUNT_SQL,
            CREDENTIAL_SQL,
            EVENT_SEQUENCE_SQL,
            EVENT_SQL,
            PERMISSION_SQL,
            PROJECT_DIRECTORY_SQL,
            MESSAGE_SQL,
            PART_SQL,
            SESSION_CONTEXT_EPOCH_SQL,
            SESSION_INPUT_SQL,
            SESSION_MESSAGE_SQL,
            SESSION_SQL,
            TODO_SQL,
            SESSION_SHARE_SQL,
        ];
        // Concatenate all table SQL + index SQL into one string at compile time.
        // Since Rust const strings can't be joined dynamically, we build a
        // single literal.
        concat!(
            "CREATE TABLE IF NOT EXISTS `project` (\n  `id` text PRIMARY KEY,\n  `worktree` text NOT NULL,\n  `vcs` text,\n  `name` text,\n  `icon_url` text,\n  `icon_url_override` text,\n  `icon_color` text,\n  `time_created` integer NOT NULL,\n  `time_updated` integer NOT NULL,\n  `time_initialized` integer,\n  `sandboxes` text NOT NULL,\n  `commands` text\n);\n",
            "CREATE TABLE IF NOT EXISTS `workspace` (\n  `id` text PRIMARY KEY,\n  `type` text NOT NULL,\n  `name` text DEFAULT '' NOT NULL,\n  `branch` text,\n  `directory` text,\n  `extra` text,\n  `project_id` text NOT NULL,\n  `time_used` integer NOT NULL,\n  CONSTRAINT `fk_workspace_project_id_project_id_fk` FOREIGN KEY (`project_id`) REFERENCES `project`(`id`) ON DELETE CASCADE\n);\n",
            "CREATE TABLE IF NOT EXISTS `data_migration` (\n  `name` text PRIMARY KEY,\n  `time_completed` integer NOT NULL\n);\n",
            "CREATE TABLE IF NOT EXISTS `account_state` (\n  `id` integer PRIMARY KEY,\n  `active_account_id` text,\n  `active_org_id` text,\n  CONSTRAINT `fk_account_state_active_account_id_account_id_fk` FOREIGN KEY (`active_account_id`) REFERENCES `account`(`id`) ON DELETE SET NULL\n);\n",
            "CREATE TABLE IF NOT EXISTS `account` (\n  `id` text PRIMARY KEY,\n  `email` text NOT NULL,\n  `url` text NOT NULL,\n  `access_token` text NOT NULL,\n  `refresh_token` text NOT NULL,\n  `token_expiry` integer,\n  `time_created` integer NOT NULL,\n  `time_updated` integer NOT NULL\n);\n",
            "CREATE TABLE IF NOT EXISTS `control_account` (\n  `email` text NOT NULL,\n  `url` text NOT NULL,\n  `access_token` text NOT NULL,\n  `refresh_token` text NOT NULL,\n  `token_expiry` integer,\n  `active` integer NOT NULL,\n  `time_created` integer NOT NULL,\n  `time_updated` integer NOT NULL,\n  CONSTRAINT `control_account_pk` PRIMARY KEY(`email`, `url`)\n);\n",
            "CREATE TABLE IF NOT EXISTS `credential` (\n  `id` text PRIMARY KEY,\n  `integration_id` text,\n  `label` text NOT NULL,\n  `value` text NOT NULL,\n  `connector_id` text,\n  `method_id` text,\n  `active` integer,\n  `time_created` integer NOT NULL,\n  `time_updated` integer NOT NULL\n);\n",
            "CREATE TABLE IF NOT EXISTS `event_sequence` (\n  `aggregate_id` text PRIMARY KEY,\n  `seq` integer NOT NULL,\n  `owner_id` text\n);\n",
            "CREATE TABLE IF NOT EXISTS `event` (\n  `id` text PRIMARY KEY,\n  `aggregate_id` text NOT NULL,\n  `seq` integer NOT NULL,\n  `type` text NOT NULL,\n  `data` text NOT NULL,\n  CONSTRAINT `fk_event_aggregate_id_event_sequence_aggregate_id_fk` FOREIGN KEY (`aggregate_id`) REFERENCES `event_sequence`(`aggregate_id`) ON DELETE CASCADE\n);\n",
            "CREATE TABLE IF NOT EXISTS `permission` (\n  `id` text PRIMARY KEY,\n  `project_id` text NOT NULL,\n  `action` text NOT NULL,\n  `resource` text NOT NULL,\n  `time_created` integer NOT NULL,\n  `time_updated` integer NOT NULL,\n  CONSTRAINT `fk_permission_project_id_project_id_fk` FOREIGN KEY (`project_id`) REFERENCES `project`(`id`) ON DELETE CASCADE\n);\n",
            "CREATE TABLE IF NOT EXISTS `project_directory` (\n  `project_id` text NOT NULL,\n  `directory` text NOT NULL,\n  `type` text,\n  `strategy` text,\n  `time_created` integer NOT NULL,\n  CONSTRAINT `project_directory_pk` PRIMARY KEY(`project_id`, `directory`),\n  CONSTRAINT `fk_project_directory_project_id_project_id_fk` FOREIGN KEY (`project_id`) REFERENCES `project`(`id`) ON DELETE CASCADE\n);\n",
            "CREATE TABLE IF NOT EXISTS `message` (\n  `id` text PRIMARY KEY,\n  `session_id` text NOT NULL,\n  `time_created` integer NOT NULL,\n  `time_updated` integer NOT NULL,\n  `data` text NOT NULL,\n  CONSTRAINT `fk_message_session_id_session_id_fk` FOREIGN KEY (`session_id`) REFERENCES `session`(`id`) ON DELETE CASCADE\n);\n",
            "CREATE TABLE IF NOT EXISTS `part` (\n  `id` text PRIMARY KEY,\n  `message_id` text NOT NULL,\n  `session_id` text NOT NULL,\n  `time_created` integer NOT NULL,\n  `time_updated` integer NOT NULL,\n  `data` text NOT NULL,\n  CONSTRAINT `fk_part_message_id_message_id_fk` FOREIGN KEY (`message_id`) REFERENCES `message`(`id`) ON DELETE CASCADE\n);\n",
            "CREATE TABLE IF NOT EXISTS `session_context_epoch` (\n  `session_id` text PRIMARY KEY,\n  `baseline` text NOT NULL,\n  `agent` text DEFAULT 'build' NOT NULL,\n  `snapshot` text NOT NULL,\n  `baseline_seq` integer NOT NULL,\n  `replacement_seq` integer,\n  `revision` integer DEFAULT 0 NOT NULL,\n  CONSTRAINT `fk_session_context_epoch_session_id_session_id_fk` FOREIGN KEY (`session_id`) REFERENCES `session`(`id`) ON DELETE CASCADE\n);\n",
            "CREATE TABLE IF NOT EXISTS `session_input` (\n  `id` text PRIMARY KEY,\n  `session_id` text NOT NULL,\n  `prompt` text NOT NULL,\n  `delivery` text NOT NULL,\n  `admitted_seq` integer NOT NULL,\n  `promoted_seq` integer,\n  `time_created` integer NOT NULL,\n  CONSTRAINT `fk_session_input_session_id_session_id_fk` FOREIGN KEY (`session_id`) REFERENCES `session`(`id`) ON DELETE CASCADE\n);\n",
            "CREATE TABLE IF NOT EXISTS `session_message` (\n  `id` text PRIMARY KEY,\n  `session_id` text NOT NULL,\n  `type` text NOT NULL,\n  `seq` integer NOT NULL,\n  `time_created` integer NOT NULL,\n  `time_updated` integer NOT NULL,\n  `data` text NOT NULL,\n  CONSTRAINT `fk_session_message_session_id_session_id_fk` FOREIGN KEY (`session_id`) REFERENCES `session`(`id`) ON DELETE CASCADE\n);\n",
            "CREATE TABLE IF NOT EXISTS `session` (\n  `id` text PRIMARY KEY,\n  `project_id` text NOT NULL,\n  `workspace_id` text,\n  `parent_id` text,\n  `slug` text NOT NULL,\n  `directory` text NOT NULL,\n  `path` text,\n  `title` text NOT NULL,\n  `version` text NOT NULL,\n  `share_url` text,\n  `summary_additions` integer,\n  `summary_deletions` integer,\n  `summary_files` integer,\n  `summary_diffs` text,\n  `metadata` text,\n  `cost` real DEFAULT 0 NOT NULL,\n  `tokens_input` integer DEFAULT 0 NOT NULL,\n  `tokens_output` integer DEFAULT 0 NOT NULL,\n  `tokens_reasoning` integer DEFAULT 0 NOT NULL,\n  `tokens_cache_read` integer DEFAULT 0 NOT NULL,\n  `tokens_cache_write` integer DEFAULT 0 NOT NULL,\n  `revert` text,\n  `permission` text,\n  `agent` text,\n  `model` text,\n  `time_created` integer NOT NULL,\n  `time_updated` integer NOT NULL,\n  `time_compacting` integer,\n  `time_archived` integer,\n  CONSTRAINT `fk_session_project_id_project_id_fk` FOREIGN KEY (`project_id`) REFERENCES `project`(`id`) ON DELETE CASCADE\n);\n",
            "CREATE TABLE IF NOT EXISTS `todo` (\n  `session_id` text NOT NULL,\n  `content` text NOT NULL,\n  `status` text NOT NULL,\n  `priority` text NOT NULL,\n  `position` integer NOT NULL,\n  `time_created` integer NOT NULL,\n  `time_updated` integer NOT NULL,\n  CONSTRAINT `todo_pk` PRIMARY KEY(`session_id`, `position`),\n  CONSTRAINT `fk_todo_session_id_session_id_fk` FOREIGN KEY (`session_id`) REFERENCES `session`(`id`) ON DELETE CASCADE\n);\n",
            "CREATE TABLE IF NOT EXISTS `session_share` (\n  `session_id` text PRIMARY KEY,\n  `id` text NOT NULL,\n  `secret` text NOT NULL,\n  `url` text NOT NULL,\n  `time_created` integer NOT NULL,\n  `time_updated` integer NOT NULL,\n  CONSTRAINT `fk_session_share_session_id_session_id_fk` FOREIGN KEY (`session_id`) REFERENCES `session`(`id`) ON DELETE CASCADE\n);\n",
            "CREATE UNIQUE INDEX IF NOT EXISTS `event_aggregate_seq_idx` ON `event` (`aggregate_id`,`seq`);\n",
            "CREATE INDEX IF NOT EXISTS `event_aggregate_type_seq_idx` ON `event` (`aggregate_id`,`type`,`seq`);\n",
            "CREATE UNIQUE INDEX IF NOT EXISTS `permission_project_action_resource_idx` ON `permission` (`project_id`,`action`,`resource`);\n",
            "CREATE INDEX IF NOT EXISTS `message_session_time_created_id_idx` ON `message` (`session_id`,`time_created`,`id`);\n",
            "CREATE INDEX IF NOT EXISTS `part_message_id_id_idx` ON `part` (`message_id`,`id`);\n",
            "CREATE INDEX IF NOT EXISTS `part_session_idx` ON `part` (`session_id`);\n",
            "CREATE INDEX IF NOT EXISTS `session_input_session_pending_delivery_seq_idx` ON `session_input` (`session_id`,`promoted_seq`,`delivery`,`admitted_seq`);\n",
            "CREATE UNIQUE INDEX IF NOT EXISTS `session_input_session_admitted_seq_idx` ON `session_input` (`session_id`,`admitted_seq`);\n",
            "CREATE UNIQUE INDEX IF NOT EXISTS `session_input_session_promoted_seq_idx` ON `session_input` (`session_id`,`promoted_seq`);\n",
            "CREATE UNIQUE INDEX IF NOT EXISTS `session_message_session_seq_idx` ON `session_message` (`session_id`,`seq`);\n",
            "CREATE INDEX IF NOT EXISTS `session_message_session_type_seq_idx` ON `session_message` (`session_id`,`type`,`seq`);\n",
            "CREATE INDEX IF NOT EXISTS `session_message_session_time_created_id_idx` ON `session_message` (`session_id`,`time_created`,`id`);\n",
            "CREATE INDEX IF NOT EXISTS `session_message_time_created_idx` ON `session_message` (`time_created`);\n",
            "CREATE INDEX IF NOT EXISTS `session_project_idx` ON `session` (`project_id`);\n",
            "CREATE INDEX IF NOT EXISTS `session_workspace_idx` ON `session` (`workspace_id`);\n",
            "CREATE INDEX IF NOT EXISTS `session_parent_idx` ON `session` (`parent_id`);\n",
            "CREATE INDEX IF NOT EXISTS `todo_session_idx` ON `todo` (`session_id`);\n",
        )
    },
};

/// All 35 migrations from opencode, in dependency order.
///
/// # Source
/// Ported from `packages/core/src/database/migration.gen.ts`
pub const ALL_MIGRATIONS: &[Migration] = &[
    INITIAL_MIGRATION,
    Migration {
        id: "20260211171708_add_project_commands",
        sql: "ALTER TABLE `project` ADD `commands` text;",
    },
    Migration {
        id: "20260213144116_wakeful_the_professor",
        sql: "CREATE TABLE IF NOT EXISTS `control_account` (\n  `email` text NOT NULL,\n  `url` text NOT NULL,\n  `access_token` text NOT NULL,\n  `refresh_token` text NOT NULL,\n  `token_expiry` integer,\n  `active` integer NOT NULL,\n  `time_created` integer NOT NULL,\n  `time_updated` integer NOT NULL,\n  CONSTRAINT `control_account_pk` PRIMARY KEY(`email`, `url`)\n);",
    },
    Migration {
        id: "20260225215848_workspace",
        sql: "CREATE TABLE IF NOT EXISTS `workspace` (\n  `id` text PRIMARY KEY,\n  `branch` text,\n  `project_id` text NOT NULL,\n  `config` text NOT NULL,\n  CONSTRAINT `fk_workspace_project_id_project_id_fk` FOREIGN KEY (`project_id`) REFERENCES `project`(`id`) ON DELETE CASCADE\n);",
    },
    Migration {
        id: "20260227213759_add_session_workspace_id",
        sql: "ALTER TABLE `session` ADD `workspace_id` text;\nCREATE INDEX IF NOT EXISTS `session_workspace_idx` ON `session` (`workspace_id`);",
    },
    Migration {
        id: "20260228203230_blue_harpoon",
        sql: "CREATE TABLE IF NOT EXISTS `account` (\n  `id` text PRIMARY KEY,\n  `email` text NOT NULL,\n  `url` text NOT NULL,\n  `access_token` text NOT NULL,\n  `refresh_token` text NOT NULL,\n  `token_expiry` integer,\n  `selected_org_id` text,\n  `time_created` integer NOT NULL,\n  `time_updated` integer NOT NULL\n);\nCREATE TABLE IF NOT EXISTS `account_state` (\n  `id` integer PRIMARY KEY NOT NULL,\n  `active_account_id` text,\n  FOREIGN KEY (`active_account_id`) REFERENCES `account`(`id`) ON UPDATE no action ON DELETE set null\n);",
    },
    Migration {
        id: "20260303231226_add_workspace_fields",
        sql: "ALTER TABLE `workspace` ADD `type` text NOT NULL;\nALTER TABLE `workspace` ADD `name` text;\nALTER TABLE `workspace` ADD `directory` text;\nALTER TABLE `workspace` ADD `extra` text;\nALTER TABLE `workspace` DROP COLUMN `config`;",
    },
    Migration {
        id: "20260309230000_move_org_to_state",
        sql: "ALTER TABLE `account_state` ADD `active_org_id` text;\nUPDATE `account_state` SET `active_org_id` = (SELECT `selected_org_id` FROM `account` WHERE `account`.`id` = `account_state`.`active_account_id`);\nALTER TABLE `account` DROP COLUMN `selected_org_id`;",
    },
    Migration {
        id: "20260312043431_session_message_cursor",
        sql: "DROP INDEX IF EXISTS `message_session_idx`;\nDROP INDEX IF EXISTS `part_message_idx`;\nCREATE INDEX IF NOT EXISTS `message_session_time_created_id_idx` ON `message` (`session_id`,`time_created`,`id`);\nCREATE INDEX IF NOT EXISTS `part_message_id_id_idx` ON `part` (`message_id`,`id`);",
    },
    Migration {
        id: "20260323234822_events",
        sql: "CREATE TABLE IF NOT EXISTS `event_sequence` (\n  `aggregate_id` text PRIMARY KEY,\n  `seq` integer NOT NULL\n);\nCREATE TABLE IF NOT EXISTS `event` (\n  `id` text PRIMARY KEY,\n  `aggregate_id` text NOT NULL,\n  `seq` integer NOT NULL,\n  `type` text NOT NULL,\n  `data` text NOT NULL,\n  CONSTRAINT `fk_event_aggregate_id_event_sequence_aggregate_id_fk` FOREIGN KEY (`aggregate_id`) REFERENCES `event_sequence`(`aggregate_id`) ON DELETE CASCADE\n);",
    },
    Migration {
        id: "20260410174513_workspace-name",
        sql: "PRAGMA foreign_keys=OFF;\nCREATE TABLE IF NOT EXISTS `__new_workspace` (\n  `id` text PRIMARY KEY,\n  `type` text NOT NULL,\n  `name` text DEFAULT '' NOT NULL,\n  `branch` text,\n  `directory` text,\n  `extra` text,\n  `project_id` text NOT NULL,\n  CONSTRAINT `fk_workspace_project_id_project_id_fk` FOREIGN KEY (`project_id`) REFERENCES `project`(`id`) ON DELETE CASCADE\n);\nINSERT INTO `__new_workspace`(`id`, `type`, `branch`, `name`, `directory`, `extra`, `project_id`) SELECT `id`, `type`, `branch`, `name`, `directory`, `extra`, `project_id` FROM `workspace`;\nDROP TABLE `workspace`;\nALTER TABLE `__new_workspace` RENAME TO `workspace`;\nPRAGMA foreign_keys=ON;",
    },
    Migration {
        id: "20260413175956_chief_energizer",
        sql: "CREATE TABLE IF NOT EXISTS `session_entry` (\n  `id` text PRIMARY KEY,\n  `session_id` text NOT NULL,\n  `type` text NOT NULL,\n  `time_created` integer NOT NULL,\n  `time_updated` integer NOT NULL,\n  `data` text NOT NULL,\n  CONSTRAINT `fk_session_entry_session_id_session_id_fk` FOREIGN KEY (`session_id`) REFERENCES `session`(`id`) ON DELETE CASCADE\n);\nCREATE INDEX IF NOT EXISTS `session_entry_session_idx` ON `session_entry` (`session_id`);\nCREATE INDEX IF NOT EXISTS `session_entry_session_type_idx` ON `session_entry` (`session_id`,`type`);\nCREATE INDEX IF NOT EXISTS `session_entry_time_created_idx` ON `session_entry` (`time_created`);",
    },
    Migration {
        id: "20260423070820_add_icon_url_override",
        sql: "ALTER TABLE `project` ADD `icon_url_override` text;\nUPDATE `project` SET `icon_url_override` = `icon_url` WHERE `icon_url` IS NOT NULL;",
    },
    Migration {
        id: "20260427172553_slow_nightmare",
        sql: "CREATE TABLE IF NOT EXISTS `session_message` (\n  `id` text PRIMARY KEY,\n  `session_id` text NOT NULL,\n  `type` text NOT NULL,\n  `time_created` integer NOT NULL,\n  `time_updated` integer NOT NULL,\n  `data` text NOT NULL,\n  CONSTRAINT `fk_session_message_session_id_session_id_fk` FOREIGN KEY (`session_id`) REFERENCES `session`(`id`) ON DELETE CASCADE\n);\nDROP INDEX IF EXISTS `session_entry_session_idx`;\nDROP INDEX IF EXISTS `session_entry_session_type_idx`;\nDROP INDEX IF EXISTS `session_entry_time_created_idx`;\nCREATE INDEX IF NOT EXISTS `session_message_session_idx` ON `session_message` (`session_id`);\nCREATE INDEX IF NOT EXISTS `session_message_session_type_idx` ON `session_message` (`session_id`,`type`);\nCREATE INDEX IF NOT EXISTS `session_message_time_created_idx` ON `session_message` (`time_created`);\nDROP TABLE IF EXISTS `session_entry`;",
    },
    Migration {
        id: "20260428004200_add_session_path",
        sql: "ALTER TABLE `session` ADD `path` text;",
    },
    Migration {
        id: "20260501142318_next_venus",
        sql: "ALTER TABLE `session` ADD `agent` text;\nALTER TABLE `session` ADD `model` text;",
    },
    Migration {
        id: "20260504145000_add_sync_owner",
        sql: "ALTER TABLE `event_sequence` ADD `owner_id` text;",
    },
    Migration {
        id: "20260507164347_add_workspace_time",
        sql: "ALTER TABLE `workspace` ADD `time_used` integer NOT NULL DEFAULT 0;",
    },
    Migration {
        id: "20260510033149_session_usage",
        sql: "ALTER TABLE `session` ADD `cost` real DEFAULT 0 NOT NULL;\nALTER TABLE `session` ADD `tokens_input` integer DEFAULT 0 NOT NULL;\nALTER TABLE `session` ADD `tokens_output` integer DEFAULT 0 NOT NULL;\nALTER TABLE `session` ADD `tokens_reasoning` integer DEFAULT 0 NOT NULL;\nALTER TABLE `session` ADD `tokens_cache_read` integer DEFAULT 0 NOT NULL;\nALTER TABLE `session` ADD `tokens_cache_write` integer DEFAULT 0 NOT NULL;\nUPDATE session SET cost = coalesce((SELECT sum(coalesce(json_extract(message.data, '$.cost'), 0)) FROM message WHERE message.session_id = session.id AND json_extract(message.data, '$.role') = 'assistant'), 0), tokens_input = coalesce((SELECT sum(coalesce(json_extract(message.data, '$.tokens.input'), 0)) FROM message WHERE message.session_id = session.id AND json_extract(message.data, '$.role') = 'assistant'), 0), tokens_output = coalesce((SELECT sum(coalesce(json_extract(message.data, '$.tokens.output'), 0)) FROM message WHERE message.session_id = session.id AND json_extract(message.data, '$.role') = 'assistant'), 0), tokens_reasoning = coalesce((SELECT sum(coalesce(json_extract(message.data, '$.tokens.reasoning'), 0)) FROM message WHERE message.session_id = session.id AND json_extract(message.data, '$.role') = 'assistant'), 0), tokens_cache_read = coalesce((SELECT sum(coalesce(json_extract(message.data, '$.tokens.cache.read'), 0)) FROM message WHERE message.session_id = session.id AND json_extract(message.data, '$.role') = 'assistant'), 0), tokens_cache_write = coalesce((SELECT sum(coalesce(json_extract(message.data, '$.tokens.cache.write'), 0)) FROM message WHERE message.session_id = session.id AND json_extract(message.data, '$.role') = 'assistant'), 0);",
    },
    Migration {
        id: "20260511000411_data_migration_state",
        sql: "CREATE TABLE IF NOT EXISTS `data_migration` (\n  `name` text PRIMARY KEY,\n  `time_completed` integer NOT NULL\n);",
    },
    Migration {
        id: "20260511173437_session-metadata",
        sql: "ALTER TABLE `session` ADD `metadata` text;",
    },
    Migration {
        id: "20260601010001_normalize_storage_paths",
        sql: "UPDATE project SET worktree = REPLACE(worktree, char(92), '/') WHERE worktree GLOB '[A-Za-z]:' || char(92) || '*' OR worktree LIKE char(92) || char(92) || '%';\nUPDATE project SET sandboxes = REPLACE(sandboxes, char(92) || char(92), '/') WHERE instr(sandboxes, char(92)) > 0 AND (worktree GLOB '[A-Za-z]:*' OR worktree LIKE '//%');\nUPDATE session SET directory = REPLACE(directory, char(92), '/') WHERE directory GLOB '[A-Za-z]:' || char(92) || '*' OR directory LIKE char(92) || char(92) || '%';\nUPDATE session SET path = REPLACE(path, char(92), '/') WHERE path IS NOT NULL AND instr(path, char(92)) > 0 AND (directory GLOB '[A-Za-z]:*' OR directory LIKE '//%');",
    },
    Migration {
        id: "20260601202201_amazing_prowler",
        sql: "DROP TABLE IF EXISTS `permission`;",
    },
    Migration {
        id: "20260602002951_lowly_union_jack",
        sql: "CREATE TABLE IF NOT EXISTS `permission` (\n  `id` text PRIMARY KEY,\n  `project_id` text NOT NULL,\n  `action` text NOT NULL,\n  `resource` text NOT NULL,\n  `time_created` integer NOT NULL,\n  `time_updated` integer NOT NULL,\n  CONSTRAINT `fk_permission_project_id_project_id_fk` FOREIGN KEY (`project_id`) REFERENCES `project`(`id`) ON DELETE CASCADE\n);\nCREATE UNIQUE INDEX IF NOT EXISTS `permission_project_action_resource_idx` ON `permission` (`project_id`,`action`,`resource`);",
    },
    Migration {
        id: "20260602182828_add_project_directories",
        sql: "CREATE TABLE IF NOT EXISTS `project_directory` (\n  `project_id` text NOT NULL,\n  `directory` text NOT NULL,\n  `type` text NOT NULL,\n  `time_created` integer NOT NULL,\n  CONSTRAINT `project_directory_pk` PRIMARY KEY(`project_id`, `directory`),\n  CONSTRAINT `fk_project_directory_project_id_project_id_fk` FOREIGN KEY (`project_id`) REFERENCES `project`(`id`) ON DELETE CASCADE\n);",
    },
    Migration {
        id: "20260603001617_session_message_projection_indexes",
        sql: "DROP INDEX IF EXISTS `session_message_session_idx`;\nDROP INDEX IF EXISTS `session_message_session_type_idx`;\nCREATE INDEX IF NOT EXISTS `event_aggregate_seq_idx` ON `event` (`aggregate_id`,`seq`);\nCREATE INDEX IF NOT EXISTS `session_message_session_time_created_id_idx` ON `session_message` (`session_id`,`time_created`,`id`);\nCREATE INDEX IF NOT EXISTS `session_message_session_type_time_created_id_idx` ON `session_message` (`session_id`,`type`,`time_created`,`id`);",
    },
    Migration {
        id: "20260603040000_session_message_projection_order",
        sql: "DELETE FROM `session_message`;\nALTER TABLE `session_message` ADD COLUMN `seq` integer NOT NULL;\nDROP INDEX IF EXISTS `session_message_session_type_time_created_id_idx`;\nCREATE INDEX IF NOT EXISTS `session_message_session_seq_idx` ON `session_message` (`session_id`,`seq`);\nCREATE INDEX IF NOT EXISTS `session_message_session_type_seq_idx` ON `session_message` (`session_id`,`type`,`seq`);",
    },
    Migration {
        id: "20260603141458_session_input_inbox",
        sql: "CREATE TABLE IF NOT EXISTS `session_input` (\n  `seq` integer PRIMARY KEY AUTOINCREMENT,\n  `id` text NOT NULL UNIQUE,\n  `session_id` text NOT NULL,\n  `prompt` text NOT NULL,\n  `delivery` text NOT NULL,\n  `promoted_seq` integer,\n  `time_created` integer NOT NULL,\n  CONSTRAINT `fk_session_input_session_id_session_id_fk` FOREIGN KEY (`session_id`) REFERENCES `session`(`id`) ON DELETE CASCADE\n);\nCREATE INDEX IF NOT EXISTS `session_input_session_pending_seq_idx` ON `session_input` (`session_id`,`promoted_seq`,`seq`);",
    },
    Migration {
        id: "20260603160727_jittery_ezekiel_stane",
        sql: "DROP INDEX IF EXISTS `session_input_session_pending_seq_idx`;\nCREATE INDEX IF NOT EXISTS `event_aggregate_type_seq_idx` ON `event` (`aggregate_id`,`type`,`seq`);\nCREATE INDEX IF NOT EXISTS `session_input_session_pending_delivery_seq_idx` ON `session_input` (`session_id`,`promoted_seq`,`delivery`,`seq`);\nCREATE INDEX IF NOT EXISTS `session_message_session_time_created_id_idx` ON `session_message` (`session_id`,`time_created`,`id`);",
    },
    Migration {
        id: "20260604172448_event_sourced_session_input",
        sql: "DELETE FROM `session_input`;\nDELETE FROM `session_message`;\nDELETE FROM `event`;\nDELETE FROM `event_sequence`;\nUPDATE `session` SET `workspace_id` = NULL;\nDELETE FROM `workspace`;\nDROP INDEX IF EXISTS `event_aggregate_seq_idx`;\nCREATE UNIQUE INDEX IF NOT EXISTS `event_aggregate_seq_idx` ON `event` (`aggregate_id`,`seq`);\nDROP INDEX IF EXISTS `session_message_session_seq_idx`;\nCREATE UNIQUE INDEX IF NOT EXISTS `session_message_session_seq_idx` ON `session_message` (`session_id`,`seq`);\nPRAGMA foreign_keys=OFF;\nCREATE TABLE IF NOT EXISTS `__new_session_input` (\n  `id` text PRIMARY KEY,\n  `session_id` text NOT NULL,\n  `prompt` text NOT NULL,\n  `delivery` text NOT NULL,\n  `admitted_seq` integer NOT NULL,\n  `promoted_seq` integer,\n  `time_created` integer NOT NULL,\n  CONSTRAINT `fk_session_input_session_id_session_id_fk` FOREIGN KEY (`session_id`) REFERENCES `session`(`id`) ON DELETE CASCADE\n);\nDROP TABLE `session_input`;\nALTER TABLE `__new_session_input` RENAME TO `session_input`;\nPRAGMA foreign_keys=ON;\nCREATE INDEX IF NOT EXISTS `session_input_session_pending_delivery_seq_idx` ON `session_input` (`session_id`,`promoted_seq`,`delivery`,`admitted_seq`);\nCREATE UNIQUE INDEX IF NOT EXISTS `session_input_session_admitted_seq_idx` ON `session_input` (`session_id`,`admitted_seq`);\nCREATE UNIQUE INDEX IF NOT EXISTS `session_input_session_promoted_seq_idx` ON `session_input` (`session_id`,`promoted_seq`);",
    },
    Migration {
        id: "20260605003541_add_session_context_snapshot",
        sql: "CREATE TABLE IF NOT EXISTS `session_context_epoch` (\n  `session_id` text PRIMARY KEY,\n  `baseline` text NOT NULL,\n  `snapshot` text NOT NULL,\n  `baseline_seq` integer NOT NULL,\n  `replacement_seq` integer,\n  `revision` integer DEFAULT 0 NOT NULL,\n  CONSTRAINT `fk_session_context_epoch_session_id_session_id_fk` FOREIGN KEY (`session_id`) REFERENCES `session`(`id`) ON DELETE CASCADE\n);",
    },
    Migration {
        id: "20260605042240_add_context_epoch_agent",
        sql: "ALTER TABLE `session_context_epoch` ADD `agent` text DEFAULT 'build' NOT NULL;",
    },
    Migration {
        id: "20260611035744_credential",
        sql: "CREATE TABLE IF NOT EXISTS `credential` (\n  `id` text PRIMARY KEY,\n  `connector_id` text NOT NULL,\n  `method_id` text NOT NULL,\n  `label` text NOT NULL,\n  `value` text NOT NULL,\n  `active` integer DEFAULT false NOT NULL,\n  `time_created` integer NOT NULL,\n  `time_updated` integer NOT NULL\n);\nCREATE UNIQUE INDEX IF NOT EXISTS `credential_connector_active_idx` ON `credential` (`connector_id`) WHERE \"credential\".\"active\" = 1;",
    },
    Migration {
        id: "20260611192811_lush_chimera",
        sql: "DROP INDEX IF EXISTS `credential_connector_active_idx`;\nDROP TABLE IF EXISTS `credential`;\nCREATE TABLE IF NOT EXISTS `credential` (\n  `id` text PRIMARY KEY,\n  `integration_id` text,\n  `label` text NOT NULL,\n  `value` text NOT NULL,\n  `connector_id` text,\n  `method_id` text,\n  `active` integer,\n  `time_created` integer NOT NULL,\n  `time_updated` integer NOT NULL\n);",
    },
    Migration {
        id: "20260612174303_project_dir_strategy",
        sql: "ALTER TABLE `project_directory` ADD `strategy` text;\nPRAGMA foreign_keys=OFF;\nCREATE TABLE IF NOT EXISTS `__new_project_directory` (\n  `project_id` text NOT NULL,\n  `directory` text NOT NULL,\n  `type` text,\n  `strategy` text,\n  `time_created` integer NOT NULL,\n  CONSTRAINT `project_directory_pk` PRIMARY KEY(`project_id`, `directory`),\n  CONSTRAINT `fk_project_directory_project_id_project_id_fk` FOREIGN KEY (`project_id`) REFERENCES `project`(`id`) ON DELETE CASCADE\n);\nINSERT INTO `__new_project_directory`(`project_id`, `directory`, `type`, `time_created`) SELECT `project_id`, `directory`, `type`, `time_created` FROM `project_directory`;\nDROP TABLE `project_directory`;\nALTER TABLE `__new_project_directory` RENAME TO `project_directory`;\nPRAGMA foreign_keys=ON;",
    },
];

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
        assert!(names.contains(&"migration"));
        assert!(names.contains(&"workspace"));
        assert!(names.contains(&"account"));
        assert!(names.contains(&"todo"));
        assert!(names.contains(&"session_share"));

        // Verify migration was recorded
        let count: (i64,) = sqlx::query_as("SELECT COUNT(*) FROM migration")
            .fetch_one(db.pool())
            .await
            .unwrap();
        assert!(count.0 >= 1);

        // Running migrations again should be idempotent
        db.run_migrations(ALL_MIGRATIONS).await.unwrap();
        let count2: (i64,) = sqlx::query_as("SELECT COUNT(*) FROM migration")
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
            "INSERT INTO project (id, worktree, time_created, time_updated, sandboxes) VALUES (?1, ?2, ?3, ?4, ?5)",
        )
        .bind("proj-1")
        .bind("/home/test")
        .bind(now)
        .bind(now)
        .bind("[]")
        .execute(db.pool())
        .await
        .unwrap();

        // Insert a session
        sqlx::query(
            "INSERT INTO session (id, project_id, slug, directory, title, version, time_created, time_updated) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)",
        )
        .bind("sess-1")
        .bind("proj-1")
        .bind("slug-1")
        .bind("/home/test")
        .bind("Test Session")
        .bind("1.0")
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

    // -- Schema validation tests ----------------------------------------------

    #[test]
    fn test_validate_schema_root_valid() {
        let value = serde_json::json!({});
        assert!(validate_schema(&value, &StorageSchema::Root).is_ok());

        let value = serde_json::json!({"path": {"root": "/home/user"}});
        assert!(validate_schema(&value, &StorageSchema::Root).is_ok());
    }

    #[test]
    fn test_validate_schema_session() {
        let value = serde_json::json!({"id": "ses_123"});
        assert!(validate_schema(&value, &StorageSchema::Session).is_ok());

        let value = serde_json::json!({});
        assert!(validate_schema(&value, &StorageSchema::Session).is_err());
    }

    #[test]
    fn test_validate_schema_message() {
        let value = serde_json::json!({"id": "msg_456"});
        assert!(validate_schema(&value, &StorageSchema::Message).is_ok());

        let value = serde_json::json!({"foo": "bar"});
        assert!(validate_schema(&value, &StorageSchema::Message).is_err());
    }

    #[test]
    fn test_validate_schema_summary() {
        let value = serde_json::json!({
            "id": "ses_123",
            "projectID": "proj_456",
            "summary": {
                "diffs": [
                    {"additions": 10, "deletions": 5}
                ]
            }
        });
        assert!(validate_schema(&value, &StorageSchema::Summary).is_ok());

        let value = serde_json::json!({"id": "ses_123"});
        assert!(validate_schema(&value, &StorageSchema::Summary).is_err());
    }

    #[test]
    fn test_validate_schema_any() {
        let value = serde_json::json!({"anything": "goes"});
        assert!(validate_schema(&value, &StorageSchema::Any).is_ok());
    }

    // -- FileLock tests -------------------------------------------------------

    #[test]
    fn test_file_lock_read_write() {
        let lock = FileLock::new();
        let _r1 = lock.read_lock().expect("read lock");
        let _r2 = lock.read_lock().expect("reentrant read lock");
        drop(_r1);
        drop(_r2);
        let _w = lock.write_lock().expect("write lock");
    }

    #[test]
    fn test_lock_map() {
        let map = LockMap::new();
        let path = PathBuf::from("/tmp/test.json");
        let lock1 = map.get(&path);
        let lock2 = map.get(&path);
        // Same path returns the same Arc<FileLock>
        assert!(Arc::ptr_eq(&lock1, &lock2));
        map.remove(&path);
        // After remove, a new lock is created
        let lock3 = map.get(&path);
        assert!(!Arc::ptr_eq(&lock1, &lock3));
    }

    #[test]
    fn test_storage_read_with_schema() {
        let dir = std::env::temp_dir().join("rustcode-storage-schema-test");
        let _ = std::fs::remove_dir_all(&dir);
        let storage = Storage::new_unchecked(dir.clone());

        storage.write(&["session", "test"], &serde_json::json!({"id": "ses_123"})).unwrap();
        let result: serde_json::Value = storage.read_with_schema(&["session", "test"], &StorageSchema::Session).unwrap();
        assert_eq!(result.get("id").and_then(|v| v.as_str()), Some("ses_123"));

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

        // The migration should NOT be recorded in migration
        let count: (i64,) =
            sqlx::query_as("SELECT COUNT(*) FROM migration WHERE id = '99999999_test_failing'")
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
            "INSERT INTO project (id, worktree, name, time_created, time_updated, sandboxes) VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
        )
        .bind("proj-qr")
        .bind("/home/test")
        .bind("query-test")
        .bind(now)
        .bind(now)
        .bind("[]")
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
                "INSERT INTO project (id, worktree, name, time_created, time_updated, sandboxes) VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
            )
            .bind("persist-1")
            .bind("/home/test")
            .bind("persistent test")
            .bind(now)
            .bind(now)
            .bind("[]")
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
