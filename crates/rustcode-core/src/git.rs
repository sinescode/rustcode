//! Git integration.
//!
//! Ported from: `packages/opencode/src/git/index.ts`

use crate::error::{Error, Result};

/// Git operations.
pub struct Git {
    worktree: std::path::PathBuf,
}

impl Git {
    /// Create a new Git instance for the given worktree.
    pub fn new(worktree: std::path::PathBuf) -> Self {
        Self { worktree }
    }

    /// Get the current branch name.
    ///
    /// # Errors
    /// Returns an error if git command fails.
    pub fn branch(&self) -> Result<String> {
        let output = std::process::Command::new("git")
            .args(["branch", "--show-current"])
            .current_dir(&self.worktree)
            .output()?;
        Ok(String::from_utf8_lossy(&output.stdout).trim().to_string())
    }

    /// Get the current commit SHA.
    ///
    /// # Errors
    /// Returns an error if git command fails.
    pub fn rev_parse_head(&self) -> Result<String> {
        let output = std::process::Command::new("git")
            .args(["rev-parse", "HEAD"])
            .current_dir(&self.worktree)
            .output()?;
        Ok(String::from_utf8_lossy(&output.stdout).trim().to_string())
    }

    /// Get the git status.
    ///
    /// # Errors
    /// Returns an error if git command fails.
    pub fn status(&self) -> Result<String> {
        let output = std::process::Command::new("git")
            .args(["status", "--porcelain"])
            .current_dir(&self.worktree)
            .output()?;
        Ok(String::from_utf8_lossy(&output.stdout).to_string())
    }

    /// Get the diff.
    ///
    /// # Errors
    /// Returns an error if git command fails.
    pub fn diff(&self) -> Result<String> {
        let output = std::process::Command::new("git")
            .args(["diff"])
            .current_dir(&self.worktree)
            .output()?;
        Ok(String::from_utf8_lossy(&output.stdout).to_string())
    }

    /// Check if the worktree is a git repository.
    pub fn is_repo(&self) -> bool {
        std::process::Command::new("git")
            .args(["rev-parse", "--is-inside-work-tree"])
            .current_dir(&self.worktree)
            .output()
            .map(|o| o.status.success())
            .unwrap_or(false)
    }
}
