//! Storage / database layer.
//!
//! Ported from: `packages/opencode/src/storage/storage.ts`

use crate::error::{Error, Result};
use std::path::PathBuf;

/// JSON file-based storage.
pub struct Storage {
    dir: PathBuf,
}

impl Storage {
    /// Create a new storage instance.
    pub fn new(dir: PathBuf) -> Self {
        Self { dir }
    }

    /// Read a value by key path.
    pub fn read<T: serde::de::DeserializeOwned>(&self, key: &[&str]) -> Result<T> {
        let path = self.key_path(key);
        let content = std::fs::read_to_string(&path)?;
        let value = serde_json::from_str(&content)?;
        Ok(value)
    }

    /// Write a value by key path.
    pub fn write<T: serde::Serialize>(&self, key: &[&str], value: &T) -> Result<()> {
        let path = self.key_path(key);
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)?;
        }
        let content = serde_json::to_string_pretty(value)?;
        std::fs::write(path, content)?;
        Ok(())
    }

    /// Update a value by key path.
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
    pub fn remove(&self, key: &[&str]) -> Result<()> {
        let path = self.key_path(key);
        if path.exists() {
            std::fs::remove_file(&path)?;
        }
        Ok(())
    }

    /// List keys under a prefix.
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
                if name.ends_with(".json") {
                    keys.push(name[..name.len() - 5].to_string());
                }
            }
        }
        Ok(keys)
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

/// SQLite database storage.
pub struct Database {
    // Placeholder for sqlx SQLite pool
    _dir: PathBuf,
}

impl Database {
    /// Create a new database instance.
    pub fn new(dir: PathBuf) -> Self {
        Self { _dir: dir }
    }

    /// Initialize the database schema.
    ///
    /// # Errors
    /// Returns an error if initialization fails.
    pub async fn initialize(&self) -> Result<()> {
        // TODO: Run migrations
        tracing::info!("Database initialized at {:?}", self._dir);
        Ok(())
    }
}
