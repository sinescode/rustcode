//! # JSONL archive — append-only, crash-proof event archival.
//!
//! JSONL (JSON Lines) is used as a side-channel for append-only archiving.
//! Every event is appended as a single JSON line, making it:
//! - Crash-proof: `fsync` after every write
//! - Replayable: can rebuild SQLite from JSONL
//! - Compressible: external zstd compression on rotation

use serde_json::Value;
use std::path::{Path, PathBuf};
use tokio::fs::{self, File, OpenOptions};
use tokio::io::{AsyncWriteExt};
use tracing::debug;

/// Error type for JSONL archive operations.
#[derive(Debug, thiserror::Error)]
pub enum JsonlError {
    /// I/O error.
    #[error("I/O: {0}")]
    Io(#[from] std::io::Error),
    /// JSON serialization error.
    #[error("JSON: {0}")]
    Json(#[from] serde_json::Error),
}

/// Append-only JSONL archive.
///
/// Writes JSON lines to a file. Each line is a single JSON object.
/// The file is fsynced after every write for crash safety.
#[derive(Debug)]
pub struct JsonlArchive {
    /// The file being written to.
    file: File,
    /// Path to the archive file.
    path: PathBuf,
    /// Current line count.
    line_count: u64,
    /// Maximum file size before rotation (in bytes).
    max_size: u64,
}

impl JsonlArchive {
    /// Open or create a JSONL archive at the given path.
    ///
    /// `max_size` is the maximum file size in bytes before rotation.
    /// Set to 0 for no rotation.
    pub async fn open(path: impl AsRef<Path>, max_size: u64) -> Result<Self, JsonlError> {
        let path = path.as_ref().to_path_buf();

        // Count existing lines
        let existing_lines = if path.exists() {
            let file = fs::read_to_string(&path).await?;
            file.lines().count() as u64
        } else {
            // Create parent directory
            if let Some(parent) = path.parent() {
                fs::create_dir_all(parent).await?;
            }
            0
        };

        let file = OpenOptions::new()
            .create(true)
            .append(true)
            .write(true)
            .read(true)
            .open(&path)
            .await?;

        Ok(Self {
            file,
            path,
            line_count: existing_lines,
            max_size,
        })
    }

    /// Append a JSON value as a single line to the archive.
    ///
    /// This is fsynced after every write for crash safety.
    pub async fn append(&mut self, value: &Value) -> Result<u64, JsonlError> {
        let mut line = serde_json::to_string(value)?;
        line.push('\n');

        self.file.write_all(line.as_bytes()).await?;
        self.file.flush().await?;

        // Ensure data is on disk
        self.file.sync_all().await?;

        self.line_count += 1;

        // Check if rotation is needed
        let metadata = self.file.metadata().await?;
        if self.max_size > 0 && metadata.len() > self.max_size {
            self.rotate().await?;
        }

        Ok(self.line_count)
    }

    /// Rotate the archive file.
    ///
    /// Renames the current file to `<path>.1` and creates a new one.
    async fn rotate(&mut self) -> Result<(), JsonlError> {
        let rotated_path = self.path.with_extension("jsonl.old");
        fs::rename(&self.path, &rotated_path).await?;

        let new_file = OpenOptions::new()
            .create(true)
            .append(true)
            .write(true)
            .read(true)
            .open(&self.path)
            .await?;

        self.file = new_file;
        self.line_count = 0;

        debug!("rotated JSONL archive: {} → {:?}", self.path.display(), rotated_path);
        Ok(())
    }

    /// Replay all lines from the archive.
    ///
    /// Returns a vector of all deserialized JSON values.
    pub async fn replay(&self) -> Result<Vec<Value>, JsonlError> {
        let content = fs::read_to_string(&self.path).await?;
        let mut result = Vec::new();

        for line in content.lines() {
            if !line.trim().is_empty() {
                result.push(serde_json::from_str(line)?);
            }
        }

        Ok(result)
    }

    /// Get the number of lines in the archive.
    pub fn line_count(&self) -> u64 {
        self.line_count
    }

    /// Close the archive, syncing all data.
    pub async fn close(self) -> Result<(), JsonlError> {
        self.file.sync_all().await?;
        Ok(())
    }

    /// Get the path to the archive file.
    pub fn path(&self) -> &Path {
        &self.path
    }
}

/// A multi-file JSONL archive that writes to a directory.
///
/// Each session gets its own JSONL file.
#[derive(Debug)]
pub struct SessionJsonl {
    /// Directory containing JSONL files.
    dir: PathBuf,
    /// Open archives by session ID.
    archives: Vec<(String, JsonlArchive)>,
    /// Max size per file.
    max_size: u64,
}

impl SessionJsonl {
    /// Open the JSONL directory.
    pub async fn open(dir: impl AsRef<Path>, max_size: u64) -> Result<Self, JsonlError> {
        fs::create_dir_all(dir.as_ref()).await?;
        Ok(Self {
            dir: dir.as_ref().to_path_buf(),
            archives: Vec::new(),
            max_size,
        })
    }

    /// Append an event for a session.
    pub async fn append(&mut self, session_id: &str, value: &Value) -> Result<u64, JsonlError> {
        // Find or create archive for this session
        let idx = self.archives.iter().position(|(id, _)| id == session_id);
        let idx = match idx {
            Some(i) => i,
            None => {
                let path = self.dir.join(format!("{}.jsonl", session_id));
                let archive = JsonlArchive::open(&path, self.max_size).await?;
                self.archives.push((session_id.to_string(), archive));
                self.archives.len() - 1
            }
        };

        self.archives[idx].1.append(value).await
    }

    /// Close all archives.
    pub async fn close_all(&mut self) -> Result<(), JsonlError> {
        for (_, archive) in self.archives.iter_mut() {
            // Sync each archive — close() takes ownership, so we just sync instead
            archive.file.flush().await?;
            archive.file.sync_all().await?;
        }
        self.archives.clear();
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[tokio::test]
    async fn test_basic_append_and_replay() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("test.jsonl");

        let mut archive = JsonlArchive::open(&path, 0).await.unwrap();
        archive.append(&json!({"name": "alice"})).await.unwrap();
        archive.append(&json!({"name": "bob"})).await.unwrap();

        let entries = archive.replay().await.unwrap();
        assert_eq!(entries.len(), 2);
        assert_eq!(entries[0]["name"], "alice");
        assert_eq!(entries[1]["name"], "bob");
    }

    #[tokio::test]
    async fn test_line_count() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("count.jsonl");

        let mut archive = JsonlArchive::open(&path, 0).await.unwrap();
        assert_eq!(archive.line_count(), 0);

        archive.append(&json!({"x": 1})).await.unwrap();
        assert_eq!(archive.line_count(), 1);
    }

    #[tokio::test]
    async fn test_session_jsonl() {
        let dir = tempfile::tempdir().unwrap();
        let mut sj = SessionJsonl::open(dir.path(), 0).await.unwrap();

        sj.append("session1", &json!({"event": "start"})).await.unwrap();
        sj.append("session2", &json!({"event": "start"})).await.unwrap();
        sj.append("session1", &json!({"event": "end"})).await.unwrap();

        sj.close_all().await.unwrap();

        // Verify files exist
        assert!(dir.path().join("session1.jsonl").exists());
        assert!(dir.path().join("session2.jsonl").exists());
    }
}
