//! Worktree management.
//!
//! Ported from: `packages/opencode/src/worktree/index.ts`

/// Worktree information.
pub struct Worktree {
    /// Root path
    pub root: std::path::PathBuf,
}

impl Worktree {
    /// Create a new worktree.
    pub fn new(root: std::path::PathBuf) -> Self {
        Self { root }
    }

    /// Get the root path.
    pub fn root(&self) -> &std::path::Path {
        &self.root
    }
}
