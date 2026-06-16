//! Snapshot / revert system.
//!
//! Ported from: `packages/opencode/src/snapshot/index.ts`

use serde::{Deserialize, Serialize};

/// File diff information.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FileDiff {
    /// File path
    pub file: String,
    /// Number of additions
    pub additions: u64,
    /// Number of deletions
    pub deletions: u64,
}

/// Snapshot service — tracks filesystem state for revert.
pub struct Snapshot {
    worktree: std::path::PathBuf,
}

impl Snapshot {
    /// Create a new snapshot service.
    pub fn new(worktree: std::path::PathBuf) -> Self {
        Self { worktree }
    }

    /// Take a snapshot and return its hash.
    ///
    /// # Errors
    /// Returns an error if snapshot creation fails.
    pub fn track(&self) -> crate::error::Result<String> {
        let output = std::process::Command::new("git")
            .args(["rev-parse", "HEAD"])
            .current_dir(&self.worktree)
            .output()?;
        Ok(String::from_utf8_lossy(&output.stdout).trim().to_string())
    }

    /// Get the diff between two snapshots.
    ///
    /// # Errors
    /// Returns an error if diff fails.
    pub fn diff(&self, from: &str, to: &str) -> crate::error::Result<Vec<FileDiff>> {
        let output = std::process::Command::new("git")
            .args(["diff", "--stat", from, to])
            .current_dir(&self.worktree)
            .output()?;
        let stdout = String::from_utf8_lossy(&output.stdout);
        // Parse git diff --stat output
        let mut diffs = Vec::new();
        for line in stdout.lines() {
            if line.contains('|') {
                let parts: Vec<&str> = line.split('|').map(|s| s.trim()).collect();
                if parts.len() >= 2 {
                    let stats: Vec<&str> = parts[1].split_whitespace().collect();
                    let additions = stats.first().and_then(|s| s.parse().ok()).unwrap_or(0);
                    let deletions = stats.get(1).and_then(|s| s.parse().ok()).unwrap_or(0);
                    diffs.push(FileDiff {
                        file: parts[0].trim().to_string(),
                        additions,
                        deletions,
                    });
                }
            }
        }
        Ok(diffs)
    }
}
