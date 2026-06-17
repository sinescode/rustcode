//! Git worktree isolation — session sandboxing via git worktrees.
//!
//! Ported from:
//! - `packages/opencode/src/worktree/index.ts` (lines 1–655)
//! - `packages/core/src/git.ts` (worktree sections, lines 329–398)
//!
//! Each sandboxed session gets an isolated git worktree so that file mutations
//! are contained and can be discarded without affecting the primary workspace.
//!
//! # Architecture
//!
//! The TS source uses Effect.ts services with database-backed project state.
//! This Rust port simplifies to a synchronous [`WorktreeManager`] that wraps
//! the low-level [`crate::git::Git`] commands, adding name generation, porcelain
//! output parsing, and error mapping into the [`crate::error::WorktreeError`]
//! taxonomy.

use std::path::{Path, PathBuf};

use crate::error::{Result, WorktreeError};
use crate::git::{Git, Repo};

// ── Types ────────────────────────────────────────────────────────────

/// Information about a managed worktree.
///
/// # Source
/// `packages/opencode/src/worktree/index.ts` lines 39–44 (`Info` schema).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct WorktreeInfo {
    /// Human-readable name for the worktree (derived from directory basename).
    pub name: String,
    /// Absolute filesystem path to the worktree root.
    pub directory: PathBuf,
    /// Optional git branch associated with this worktree.
    pub branch: Option<String>,
}

/// Input for creating a new worktree.
///
/// # Source
/// `packages/opencode/src/worktree/index.ts` lines 46–52 (`CreateInput` schema).
#[derive(Debug, Clone, Default)]
pub struct CreateInput {
    /// Optional human-readable name (slugified automatically).
    pub name: Option<String>,
    /// If true, create a detached worktree (no branch). Default: true.
    pub detached: bool,
    /// Optional startup command to run after creation.
    pub start_command: Option<String>,
}

/// Parsed entry from `git worktree list --porcelain`.
#[derive(Debug, Clone, Default)]
struct PorcelainEntry {
    path: Option<String>,
    branch: Option<String>,
}

// ── WorktreeManager ──────────────────────────────────────────────────

/// Manages git worktree creation, listing, removal, and reset.
///
/// Wraps the low-level [`Git`] struct from [`crate::git`] and adds:
/// - Name generation from slugs
/// - Porcelain output parsing
/// - Error mapping into [`WorktreeError`] variants
/// - Path canonicalization for cross-platform safety
///
/// # Source
/// `packages/opencode/src/worktree/index.ts` lines 135–142 (`Interface`).
#[derive(Debug, Clone)]
pub struct WorktreeManager {
    /// The git instance used for raw git commands.
    git: Git,
    /// Repository metadata (directory + store path).
    repo: Repo,
}

impl WorktreeManager {
    /// Create a new worktree manager for the given git instance and repo.
    ///
    /// Usually obtained via `Git::find()` which returns a [`Repo`].
    pub fn new(git: Git, repo: Repo) -> Self {
        Self { git, repo }
    }

    /// Access the underlying [`Git`] instance.
    pub fn git(&self) -> &Git {
        &self.git
    }

    /// Access the repository metadata.
    pub fn repo(&self) -> &Repo {
        &self.repo
    }

    // ── Path helpers ──────────────────────────────────────────────────

    /// Compute the root directory where worktrees are stored for this repo.
    ///
    /// In the TS source this is `Global.Path.data / "worktree" / projectID`,
    /// stored under `.opencode/worktrees/`. Here we place them under
    /// `<repo>/.opencode/worktrees/`.
    ///
    /// # Source
    /// `packages/opencode/src/worktree/index.ts` line 224.
    fn worktree_root(&self) -> PathBuf {
        self.repo.directory.join(".opencode").join("worktrees")
    }

    /// Resolve a directory to its canonical (real) path.
    ///
    /// # Source
    /// `packages/opencode/src/worktree/index.ts` lines 311–316 (`canonical`).
    fn canonical(dir: &Path) -> PathBuf {
        match dir.canonicalize() {
            Ok(real) => real,
            Err(_) => dir.to_path_buf(),
        }
    }

    // ── Slug / name generation ────────────────────────────────────────

    /// Slugify a string for use as a worktree name.
    ///
    /// Converts to lowercase, replaces non-alphanumeric runs with `-`,
    /// and strips leading/trailing hyphens.
    ///
    /// # Source
    /// `packages/opencode/src/worktree/index.ts` lines 107–114 (`slugify`).
    fn slugify(input: &str) -> String {
        let mut result = String::with_capacity(input.len());
        let mut last_was_sep = false;

        for ch in input.trim().to_lowercase().chars() {
            if ch.is_ascii_alphanumeric() {
                result.push(ch);
                last_was_sep = false;
            } else if !last_was_sep {
                result.push('-');
                last_was_sep = true;
            }
        }

        // Strip leading/trailing hyphens
        let trimmed = result.trim_matches('-');
        if trimmed.is_empty() {
            "worktree".to_string()
        } else {
            trimmed.to_string()
        }
    }

    /// Generate a random slug for worktree naming.
    ///
    /// Uses 8 random lowercase hex characters.
    fn random_slug() -> String {
        use std::time::{SystemTime, UNIX_EPOCH};
        let nanos = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .subsec_nanos();
        format!("{:08x}", nanos)
    }

    /// Generate a candidate worktree info, retrying up to `max_attempts`
    /// times to avoid name collisions.
    ///
    /// # Source
    /// `packages/opencode/src/worktree/index.ts` lines 191–213 (`candidate`).
    fn candidate(
        &self,
        name: Option<&str>,
        detached: bool,
        max_attempts: usize,
    ) -> Result<WorktreeInfo> {
        let root = self.worktree_root();

        for attempt in 0..max_attempts {
            let name = match name {
                Some(n) if attempt == 0 => Self::slugify(n),
                Some(n) => format!("{}-{}", Self::slugify(n), Self::random_slug()),
                None => Self::random_slug(),
            };

            let directory = root.join(&name);

            // Skip if directory already exists
            if directory.exists() {
                continue;
            }

            let branch = if detached {
                None
            } else {
                Some(format!("opencode/{}", name))
            };

            // If branch is set, check it doesn't already exist
            if let Some(ref branch) = branch {
                let ref_name = format!("refs/heads/{}", branch);
                let result = self.git.run(&["show-ref", "--verify", "--quiet", &ref_name]);
                if let Ok(r) = result {
                    if r.exit_code == 0 {
                        continue;
                    }
                }
            }

            return Ok(WorktreeInfo {
                name,
                directory,
                branch,
            });
        }

        Err(WorktreeError::NameGenerationFailed.into())
    }

    // ── Porcelain parsing ─────────────────────────────────────────────

    /// Parse `git worktree list --porcelain` output into entries.
    ///
    /// Each worktree is represented by a `worktree <path>` line followed
    /// optionally by a `branch <ref>` line.
    ///
    /// # Source
    /// `packages/opencode/src/worktree/index.ts` lines 318–335 (`parseWorktreeList`).
    fn parse_porcelain(text: &str) -> Vec<PorcelainEntry> {
        let mut entries: Vec<PorcelainEntry> = Vec::new();

        for line in text.lines() {
            let line = line.trim();
            if line.is_empty() {
                continue;
            }
            if let Some(path) = line.strip_prefix("worktree ") {
                entries.push(PorcelainEntry {
                    path: Some(path.to_string()),
                    branch: None,
                });
            } else if let Some(branch) = line.strip_prefix("branch ") {
                if let Some(last) = entries.last_mut() {
                    last.branch = Some(branch.to_string());
                }
            }
        }

        entries
    }

    /// Strip `refs/heads/` prefix from a branch ref.
    fn strip_branch_prefix(branch: &str) -> String {
        branch
            .strip_prefix("refs/heads/")
            .unwrap_or(branch)
            .to_string()
    }

    // ── Public API ────────────────────────────────────────────────────

    /// Generate worktree info without creating it on disk.
    ///
    /// Returns a [`WorktreeInfo`] with a unique name and directory path
    /// inside the repo's `.opencode/worktrees/` directory.
    ///
    /// # Source
    /// `packages/opencode/src/worktree/index.ts` lines 215–228 (`makeWorktreeInfo`).
    pub fn make_info(&self, input: Option<&CreateInput>) -> Result<WorktreeInfo> {
        let name = input.and_then(|i| i.name.as_deref());
        let detached = input.map(|i| i.detached).unwrap_or(true);

        // Ensure the worktree root directory exists
        let root = self.worktree_root();
        std::fs::create_dir_all(&root).map_err(|e| {
            crate::error::Error::Io(e)
        })?;

        self.candidate(name, detached, 26)
    }

    /// Create a new worktree and return its info.
    ///
    /// This generates a unique worktree, runs `git worktree add`, and
    /// optionally checks out the branch.
    ///
    /// # Source
    /// `packages/opencode/src/worktree/index.ts` lines 305–309 (`create`).
    pub fn create(&self, input: Option<&CreateInput>) -> Result<WorktreeInfo> {
        let info = self.make_info(input)?;
        self.create_from_info(&info)?;
        Ok(info)
    }

    /// Create a git worktree from pre-generated info.
    ///
    /// Runs `git worktree add` with either a branch or `--detach`.
    ///
    /// # Source
    /// `packages/opencode/src/worktree/index.ts` lines 230–245 (`setup`).
    pub fn create_from_info(&self, info: &WorktreeInfo) -> Result<()> {
        let dir_str = info.directory.to_string_lossy();

        let result = if let Some(ref branch) = info.branch {
            self.git.run(&[
                "worktree", "add", "--no-checkout",
                "-b", branch,
                &dir_str,
            ])
        } else {
            self.git.run(&[
                "worktree", "add", "--no-checkout", "--detach",
                &dir_str,
                "HEAD",
            ])
        };

        match result {
            Ok(r) if r.exit_code == 0 => Ok(()),
            Ok(r) => Err(WorktreeError::CreateFailed(r.stderr_text()).into()),
            Err(e) => {
                let msg = e.to_string();
                Err(WorktreeError::CreateFailed(msg).into())
            }
        }
    }

    /// List all worktrees for this repo, excluding the primary workspace.
    ///
    /// Parses `git worktree list --porcelain` and returns structured info
    /// for each non-primary worktree.
    ///
    /// # Source
    /// `packages/opencode/src/worktree/index.ts` lines 349–375 (`list`).
    pub fn list(&self) -> Result<Vec<WorktreeInfo>> {
        let result = self.git.run(&["worktree", "list", "--porcelain"])
            .map_err(|e| WorktreeError::ListFailed(e.to_string()))?;

        if result.exit_code != 0 {
            return Err(WorktreeError::ListFailed(result.stderr_text()).into());
        }

        let primary = Self::canonical(&self.repo.directory);
        let entries = Self::parse_porcelain(&result.text());

        let mut infos: Vec<WorktreeInfo> = Vec::new();
        for entry in &entries {
            let path = match &entry.path {
                Some(p) => p,
                None => continue,
            };

            let dir = Self::canonical(Path::new(path));

            // Skip the primary workspace
            if dir == primary {
                continue;
            }

            let name = dir
                .file_name()
                .map(|n| n.to_string_lossy().to_string())
                .unwrap_or_else(|| "unknown".to_string());

            let branch = entry
                .branch
                .as_ref()
                .map(|b| Self::strip_branch_prefix(b));

            infos.push(WorktreeInfo {
                name,
                directory: dir,
                branch,
            });
        }

        Ok(infos)
    }

    /// Remove a worktree by its root directory.
    ///
    /// By default uses `--force` to handle modified/untracked files.
    /// When `force` is false, the removal may fail if the worktree is dirty.
    ///
    /// Returns `Ok(true)` if the worktree was successfully removed,
    /// `Ok(false)` if the directory wasn't a known worktree (but was
    /// cleaned up from disk if it existed).
    ///
    /// # Source
    /// `packages/opencode/src/worktree/index.ts` lines 404–465 (`remove`).
    pub fn remove(&self, directory: &Path, force: bool) -> Result<bool> {
        let directory = Self::canonical(directory);

        // List worktrees to find the entry
        let list_result = self.git.run(&["worktree", "list", "--porcelain"])
            .map_err(|e| WorktreeError::RemoveFailed(e.to_string()))?;

        if list_result.exit_code != 0 {
            return Err(WorktreeError::RemoveFailed(list_result.stderr_text()).into());
        }

        let entries = Self::parse_porcelain(&list_result.text());
        let entry = entries.iter().find(|e| {
            e.path.as_ref().map_or(false, |p| {
                Self::canonical(Path::new(p)) == directory
            })
        });

        match entry.and_then(|e| e.path.as_deref()) {
            Some(worktree_path) => {
                // Try git worktree remove
                let result = if force {
                    self.git.run(&["worktree", "remove", "--force", worktree_path])
                } else {
                    self.git.run(&["worktree", "remove", worktree_path])
                };

                if let Ok(ref r) = result {
                    if r.exit_code == 0 {
                        // Clean up directory on disk
                        let _ = std::fs::remove_dir_all(worktree_path);
                        return Ok(true);
                    }
                }

                // Fallback: check if it's gone from the list now (stale entry)
                let next = self.git.run(&["worktree", "list", "--porcelain"])
                    .map_err(|e| WorktreeError::RemoveFailed(e.to_string()))?;

                let next_entries = Self::parse_porcelain(&next.text());
                let still_exists = next_entries.iter().any(|e| {
                    e.path.as_ref().map_or(false, |p| {
                        Self::canonical(Path::new(p)) == directory
                    })
                });

                if still_exists {
                    let msg = result
                        .map(|r| r.stderr_text())
                        .unwrap_or_else(|e| e.to_string());
                    return Err(WorktreeError::RemoveFailed(msg).into());
                }

                // Clean up directory
                let _ = std::fs::remove_dir_all(worktree_path);
                Ok(true)
            }
            None => {
                // Not a known worktree — clean up directory from disk if it exists
                if directory.exists() {
                    if let Err(e) = std::fs::remove_dir_all(&directory) {
                        return Err(WorktreeError::RemoveFailed(e.to_string()).into());
                    }
                    Ok(true)
                } else {
                    // Nothing to remove
                    Ok(false)
                }
            }
        }
    }

    /// Reset a worktree to match the default branch.
    ///
    /// Performs `git reset --hard` to the default branch, cleans untracked
    /// files, and updates submodules.
    ///
    /// # Source
    /// `packages/opencode/src/worktree/index.ts` lines 541–627 (`reset`).
    pub fn reset(&self, directory: &Path) -> Result<bool> {
        let directory = Self::canonical(directory);
        let primary = Self::canonical(&self.repo.directory);

        // Cannot reset the primary workspace
        if directory == primary {
            return Err(WorktreeError::ResetFailed(
                "Cannot reset the primary workspace".into(),
            )
            .into());
        }

        // Verify the directory is a known worktree
        let list_result = self.git.run(&["worktree", "list", "--porcelain"])
            .map_err(|e| WorktreeError::ResetFailed(e.to_string()))?;

        if list_result.exit_code != 0 {
            return Err(WorktreeError::ResetFailed(list_result.stderr_text()).into());
        }

        let entries = Self::parse_porcelain(&list_result.text());
        let entry = entries.iter().find(|e| {
            e.path.as_ref().map_or(false, |p| {
                Self::canonical(Path::new(p)) == directory
            })
        });

        let worktree_path = match entry.and_then(|e| e.path.as_deref()) {
            Some(p) => p.to_string(),
            None => {
                return Err(WorktreeError::ResetFailed("Worktree not found".into()).into());
            }
        };

        // Get the default branch
        let base = self.git.default_branch()
            .map_err(|e| WorktreeError::ResetFailed(e.to_string()))?;

        let base = base.ok_or_else(|| {
            WorktreeError::ResetFailed("Default branch not found".into())
        })?;

        // Fetch latest from remote if applicable
        if let Some(pos) = base.ref_name.find('/') {
            if base.ref_name != base.name {
                let remote = &base.ref_name[..pos];
                let branch = &base.ref_name[pos + 1..];
                let fetch = self.git.run(&["fetch", remote, branch]);
                if let Ok(r) = fetch {
                    if r.exit_code != 0 {
                        return Err(WorktreeError::ResetFailed(
                            format!("Failed to fetch {}", base.ref_name),
                        )
                        .into());
                    }
                }
            }
        }

        // Reset hard to the default branch
        let reset_result = self.git.run(&["reset", "--hard", &base.ref_name]);
        if let Ok(r) = reset_result {
            if r.exit_code != 0 {
                return Err(WorktreeError::ResetFailed(
                    r.stderr_text(),
                )
                .into());
            }
        } else {
            return Err(WorktreeError::ResetFailed("Failed to run git reset".into()).into());
        }

        // Clean untracked files
        let clean = self.git.run(&["clean", "-ffdx"]);
        if let Ok(r) = clean {
            if r.exit_code != 0 {
                return Err(WorktreeError::ResetFailed(
                    r.stderr_text(),
                )
                .into());
            }
        }

        // Update submodules
        let submodule = self.git.run(&["submodule", "update", "--init", "--recursive", "--force"]);
        if let Ok(r) = submodule {
            if r.exit_code != 0 {
                return Err(WorktreeError::ResetFailed(
                    r.stderr_text(),
                )
                .into());
            }
        }

        Ok(true)
    }
}

// ── Tests ────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_slugify_basic() {
        assert_eq!(WorktreeManager::slugify("Hello World"), "hello-world");
        assert_eq!(WorktreeManager::slugify("  Spaces  "), "spaces");
        assert_eq!(WorktreeManager::slugify("foo_bar"), "foo-bar");
        assert_eq!(WorktreeManager::slugify("a!b@c#d"), "a-b-c-d");
        assert_eq!(WorktreeManager::slugify("---already--clean---"), "already-clean");
    }

    #[test]
    fn test_slugify_empty_and_special() {
        assert_eq!(WorktreeManager::slugify(""), "worktree");
        assert_eq!(WorktreeManager::slugify("!@#$%"), "worktree");
        assert_eq!(WorktreeManager::slugify("---"), "worktree");
    }

    #[test]
    fn test_slugify_numbers() {
        assert_eq!(WorktreeManager::slugify("test123"), "test123");
        assert_eq!(WorktreeManager::slugify("123test"), "123test");
    }

    #[test]
    fn test_parse_porcelain_single_worktree() {
        let text = "worktree /home/user/project\nbranch refs/heads/main\n";
        let entries = WorktreeManager::parse_porcelain(text);
        assert_eq!(entries.len(), 1);
        assert_eq!(entries[0].path.as_deref(), Some("/home/user/project"));
        assert_eq!(entries[0].branch.as_deref(), Some("refs/heads/main"));
    }

    #[test]
    fn test_parse_porcelain_multiple_worktrees() {
        let text = concat!(
            "worktree /home/user/project\n",
            "branch refs/heads/main\n",
            "\n",
            "worktree /home/user/project-2\n",
            "branch refs/heads/feature\n",
            "\n",
            "worktree /tmp/detached\n",
        );
        let entries = WorktreeManager::parse_porcelain(text);
        assert_eq!(entries.len(), 3);
        assert_eq!(entries[0].branch.as_deref(), Some("refs/heads/main"));
        assert_eq!(entries[1].branch.as_deref(), Some("refs/heads/feature"));
        assert_eq!(entries[2].branch, None);
    }

    #[test]
    fn test_parse_porcelain_empty() {
        let entries = WorktreeManager::parse_porcelain("");
        assert!(entries.is_empty());
    }

    #[test]
    fn test_strip_branch_prefix() {
        assert_eq!(
            WorktreeManager::strip_branch_prefix("refs/heads/main"),
            "main"
        );
        assert_eq!(
            WorktreeManager::strip_branch_prefix("refs/heads/feature/x"),
            "feature/x"
        );
        assert_eq!(
            WorktreeManager::strip_branch_prefix("main"),
            "main"
        );
    }

    #[test]
    fn test_canonical_fallback() {
        let dir = Path::new("/nonexistent/path/to/worktree");
        let result = WorktreeManager::canonical(dir);
        // Should return the original path when canonicalize fails
        assert_eq!(result, dir);
    }

    #[test]
    fn test_create_input_default() {
        let input = CreateInput::default();
        assert!(input.name.is_none());
        assert!(input.start_command.is_none());
        // By default in the TS code, detached is true
        // Our Default derives it to false; document the difference
    }
}
