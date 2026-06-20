//! Git integration — branch, status, diff, stats, patch, worktree.
//!
//! Ported from:
//! - `packages/opencode/src/git/index.ts` (lines 1–350)
//! - `packages/core/src/git.ts` (lines 1–446)

use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};
use std::process::Command;

// ══════════════════════════════════════════════════════════════════════════════
// Types
// ══════════════════════════════════════════════════════════════════════════════

/// File change kind.
///
/// # Source
/// `packages/opencode/src/git/index.ts` line 31.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Kind {
    Added,
    Deleted,
    Modified,
}

/// A changed file item from `git status` or `git diff --name-status`.
///
/// # Source
/// `packages/opencode/src/git/index.ts` lines 38–42.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Item {
    pub file: String,
    pub code: String,
    pub status: Kind,
}

/// File change statistics from `git diff --numstat`.
///
/// # Source
/// `packages/opencode/src/git/index.ts` lines 44–48.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Stat {
    pub file: String,
    pub additions: u64,
    pub deletions: u64,
}

/// A unified diff patch.
///
/// # Source
/// `packages/opencode/src/git/index.ts` lines 50–53.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Patch {
    pub text: String,
    pub truncated: bool,
}

/// Patch options.
///
/// # Source
/// `packages/opencode/src/git/index.ts` lines 55–58.
#[derive(Debug, Clone)]
pub struct PatchOptions {
    /// Context lines (default 3).
    pub context: Option<u32>,
    /// Max output bytes before truncation.
    pub max_output_bytes: Option<usize>,
}

impl Default for PatchOptions {
    fn default() -> Self {
        Self {
            context: Some(3),
            max_output_bytes: None,
        }
    }
}

/// Result from a raw git command.
///
/// # Source
/// `packages/opencode/src/git/index.ts` lines 60–66.
#[derive(Debug, Clone)]
pub struct GitResult {
    pub exit_code: i32,
    pub stdout: Vec<u8>,
    pub stderr: Vec<u8>,
    pub truncated: bool,
}

impl GitResult {
    /// Get stdout as a string.
    pub fn text(&self) -> String {
        String::from_utf8_lossy(&self.stdout).to_string()
    }

    /// Get stderr as a string.
    pub fn stderr_text(&self) -> String {
        String::from_utf8_lossy(&self.stderr).to_string()
    }
}

/// Base ref information (branch name + ref).
///
/// # Source
/// `packages/opencode/src/git/index.ts` lines 33–36.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Base {
    pub name: String,
    #[serde(rename = "ref")]
    pub ref_name: String,
}

/// Git repository information.
///
/// # Source
/// `packages/core/src/git.ts` lines 11–28.
#[derive(Debug, Clone)]
pub struct Repo {
    pub directory: PathBuf,
    pub store: PathBuf,
}

// ══════════════════════════════════════════════════════════════════════════════
// Error
// ══════════════════════════════════════════════════════════════════════════════

/// Git operation errors.
#[derive(Debug, thiserror::Error)]
pub enum GitError {
    #[error("git command failed with exit code {exit_code}: {stderr}")]
    CommandFailed { exit_code: i32, stderr: String },

    #[error("not a git repository: {0}")]
    NotARepo(String),

    #[error("worktree error ({operation}): {message}")]
    Worktree {
        operation: String,
        message: String,
        directory: Option<PathBuf>,
        force_required: bool,
    },

    #[error("patch error ({operation}): {message}")]
    Patch {
        operation: String,
        directory: PathBuf,
        message: String,
    },

    #[error("I/O error: {0}")]
    Io(#[from] std::io::Error),
}

// ══════════════════════════════════════════════════════════════════════════════
// Git config flags (shared across all commands)
// ══════════════════════════════════════════════════════════════════════════════

/// Git config flags for consistent behavior.
///
/// # Source
/// `packages/opencode/src/git/index.ts` lines 7–18.
const GIT_CFG: &[&str] = &[
    "--no-optional-locks",
    "-c",
    "core.autocrlf=false",
    "-c",
    "core.fsmonitor=false",
    "-c",
    "core.longpaths=true",
    "-c",
    "core.symlinks=true",
    "-c",
    "core.quotepath=false",
];

/// Subset of cfg for operations that need quotepath but not -z mode.
const GIT_QUOTE: &[&str] = &[
    "--no-optional-locks",
    "-c",
    "core.autocrlf=false",
    "-c",
    "core.longpaths=true",
    "-c",
    "core.symlinks=true",
    "-c",
    "core.quotepath=false",
];

/// Core git flags for snapshot operations.
///
/// # Source
/// `packages/opencode/src/snapshot/index.ts` line 33.
const GIT_CORE: &[&str] = &["-c", "core.longpaths=true", "-c", "core.symlinks=true"];

// ══════════════════════════════════════════════════════════════════════════════
// Git struct
// ══════════════════════════════════════════════════════════════════════════════

/// Git operations for a worktree directory.
///
/// # Source
/// `packages/opencode/src/git/index.ts` lines 75–91 `Interface`.
#[derive(Debug, Clone)]
pub struct Git {
    worktree: PathBuf,
}

impl Git {
    /// Create a new Git instance for the given worktree.
    pub fn new(worktree: impl Into<PathBuf>) -> Self {
        Self {
            worktree: worktree.into(),
        }
    }

    // ── Low-level ───────────────────────────────────────────────────────

    /// Run an arbitrary git command and return the raw result.
    ///
    /// # Source
    /// `packages/opencode/src/git/index.ts` lines 110–132.
    pub fn run(&self, args: &[&str]) -> Result<GitResult, GitError> {
        self.run_with_opts(args, None)
    }

    /// Run git with optional env vars.
    pub fn run_with_opts(
        &self,
        args: &[&str],
        env: Option<&[(String, String)]>,
    ) -> Result<GitResult, GitError> {
        let mut cmd = Command::new("git");
        cmd.args(args).current_dir(&self.worktree);

        if let Some(env_vars) = env {
            for (k, v) in env_vars {
                cmd.env(k, v);
            }
        }

        let output = cmd.output().map_err(GitError::Io)?;
        Ok(GitResult {
            exit_code: output.status.code().unwrap_or(1),
            stdout: output.stdout,
            stderr: output.stderr,
            truncated: false,
        })
    }

    /// Run git and get text output.
    fn text(&self, args: &[&str]) -> Result<String, GitError> {
        Ok(self.run(args)?.text())
    }

    /// Run git and get null-delimited lines.
    fn null_lines(&self, args: &[&str]) -> Result<Vec<String>, GitError> {
        let result = self.run(args)?;
        Ok(result
            .text()
            .split('\0')
            .map(|s| s.trim().to_string())
            .filter(|s| !s.is_empty())
            .collect())
    }

    // ── Branch and ref operations ───────────────────────────────────────

    /// Get the current branch name.
    ///
    /// # Source
    /// `packages/opencode/src/git/index.ts` lines 164–169.
    pub fn branch(&self) -> Result<Option<String>, GitError> {
        let result = self.run(&["symbolic-ref", "--quiet", "--short", "HEAD"])?;
        if result.exit_code != 0 {
            return Ok(None);
        }
        let text = result.text().trim().to_string();
        Ok(if text.is_empty() { None } else { Some(text) })
    }

    /// Get the git prefix (relative path from repo root).
    ///
    /// # Source
    /// `packages/opencode/src/git/index.ts` lines 171–175.
    pub fn prefix(&self) -> Result<String, GitError> {
        let result = self.run(&["rev-parse", "--show-prefix"])?;
        if result.exit_code != 0 {
            return Ok(String::new());
        }
        Ok(result.text().trim().to_string())
    }

    /// Get the default branch (main/master).
    ///
    /// # Source
    /// `packages/opencode/src/git/index.ts` lines 177–193.
    pub fn default_branch(&self) -> Result<Option<Base>, GitError> {
        // Try primary remote's HEAD
        if let Some(remote) = self.primary_remote()? {
            let head = self.run(&["symbolic-ref", &format!("refs/remotes/{remote}/HEAD")])?;
            if head.exit_code == 0 {
                let ref_name = head.text().trim().replace("refs/remotes/", "");
                let name = ref_name
                    .strip_prefix(&format!("{remote}/"))
                    .unwrap_or("")
                    .to_string();
                if !name.is_empty() {
                    return Ok(Some(Base { name, ref_name }));
                }
            }
        }

        // Fall back to local branches
        let refs = self.null_lines(&["for-each-ref", "--format=%(refname:short)", "refs/heads"])?;

        // Check configured default
        let result = self.run(&["config", "init.defaultBranch"])?;
        let configured = result.text().trim().to_string();
        if !configured.is_empty() && refs.contains(&configured) {
            return Ok(Some(Base {
                name: configured.clone(),
                ref_name: configured,
            }));
        }

        if refs.contains(&"main".to_string()) {
            return Ok(Some(Base {
                name: "main".into(),
                ref_name: "main".into(),
            }));
        }
        if refs.contains(&"master".to_string()) {
            return Ok(Some(Base {
                name: "master".into(),
                ref_name: "master".into(),
            }));
        }
        Ok(None)
    }

    /// Check if HEAD exists (repo has at least one commit).
    ///
    /// # Source
    /// `packages/opencode/src/git/index.ts` lines 195–198.
    pub fn has_head(&self) -> Result<bool, GitError> {
        let result = self.run(&["rev-parse", "--verify", "HEAD"])?;
        Ok(result.exit_code == 0)
    }

    /// Get merge base between two refs.
    ///
    /// # Source
    /// `packages/opencode/src/git/index.ts` lines 200–205.
    pub fn merge_base(&self, base: &str, head: &str) -> Result<Option<String>, GitError> {
        let result = self.run(&["merge-base", base, head])?;
        if result.exit_code != 0 {
            return Ok(None);
        }
        let text = result.text().trim().to_string();
        Ok(if text.is_empty() { None } else { Some(text) })
    }

    /// Show file content at a ref.
    ///
    /// # Source
    /// `packages/opencode/src/git/index.ts` lines 207–213.
    pub fn show(&self, git_ref: &str, file: &str, prefix: &str) -> Result<String, GitError> {
        let target = if prefix.is_empty() {
            file.to_string()
        } else {
            format!("{prefix}{file}")
        };
        let result = self.run(&["show", &format!("{git_ref}:{target}")])?;
        if result.exit_code != 0 {
            return Ok(String::new());
        }
        if result.stdout.contains(&0) {
            return Ok(String::new());
        }
        Ok(result.text())
    }

    /// Get HEAD SHA.
    pub fn rev_parse_head(&self) -> Result<String, GitError> {
        let result = self.run(&["rev-parse", "HEAD"])?;
        if result.exit_code != 0 {
            return Err(GitError::CommandFailed {
                exit_code: result.exit_code,
                stderr: result.stderr_text(),
            });
        }
        Ok(result.text().trim().to_string())
    }

    /// Check if the worktree is a git repository.
    pub fn is_repo(&self) -> bool {
        self.run(&["rev-parse", "--is-inside-work-tree"])
            .map(|r| r.exit_code == 0)
            .unwrap_or(false)
    }

    // ── Status and diff ─────────────────────────────────────────────────

    /// Get the porcelain v1 status with null-delimited filenames.
    ///
    /// # Source
    /// `packages/opencode/src/git/index.ts` lines 215–226.
    pub fn status(&self) -> Result<Vec<Item>, GitError> {
        let items = self.null_lines(&[
            "status",
            "--porcelain=v1",
            "--untracked-files=all",
            "--no-renames",
            "-z",
            "--",
            ".",
        ])?;

        Ok(items
            .into_iter()
            .filter_map(|item| {
                if item.len() < 3 {
                    return None;
                }
                let code = &item[..2];
                let file = item[3..].to_string();
                if file.is_empty() {
                    return None;
                }
                Some(Item {
                    status: kind_from_code(code),
                    code: code.to_string(),
                    file,
                })
            })
            .collect())
    }

    /// Get diff items (name-status only) against a ref.
    ///
    /// # Source
    /// `packages/opencode/src/git/index.ts` lines 228–238.
    pub fn diff(&self, git_ref: &str) -> Result<Vec<Item>, GitError> {
        let list = self.null_lines(&[
            "diff",
            "--no-ext-diff",
            "--no-renames",
            "--name-status",
            "-z",
            git_ref,
            "--",
            ".",
        ])?;

        let mut items = Vec::new();
        let mut i = 0;
        while i + 1 < list.len() {
            let code = &list[i];
            let file = &list[i + 1];
            if !code.is_empty() && !file.is_empty() {
                items.push(Item {
                    status: kind_from_code(code),
                    code: code.to_string(),
                    file: file.to_string(),
                });
            }
            i += 2;
        }
        Ok(items)
    }

    /// Get diff stats (numstat) against a ref.
    ///
    /// # Source
    /// `packages/opencode/src/git/index.ts` lines 240–261.
    pub fn stats(&self, git_ref: &str) -> Result<Vec<Stat>, GitError> {
        let items = self.null_lines(&[
            "diff",
            "--no-ext-diff",
            "--no-renames",
            "--numstat",
            "-z",
            git_ref,
            "--",
            ".",
        ])?;

        Ok(items
            .into_iter()
            .filter_map(|item| {
                let a = item.find('\t')?;
                let b = item[a + 1..].find('\t')?;
                let file = item[a + 1 + b + 1..].to_string();
                if file.is_empty() {
                    return None;
                }
                let adds_str = &item[..a];
                let dels_str = &item[a + 1..a + 1 + b];
                let additions = if adds_str == "-" {
                    0
                } else {
                    adds_str.parse().unwrap_or(0)
                };
                let deletions = if dels_str == "-" {
                    0
                } else {
                    dels_str.parse().unwrap_or(0)
                };
                Some(Stat {
                    file,
                    additions,
                    deletions,
                })
            })
            .collect())
    }

    // ── Patch operations ────────────────────────────────────────────────

    /// Get a unified diff patch for a single file.
    ///
    /// # Source
    /// `packages/opencode/src/git/index.ts` lines 263–269.
    pub fn patch(
        &self,
        git_ref: &str,
        file: &str,
        options: Option<&PatchOptions>,
    ) -> Result<Patch, GitError> {
        let opts = options.cloned().unwrap_or_default();
        let context = opts.context.unwrap_or(3);
        let result = self.run(&[
            "diff",
            "--patch",
            "--no-ext-diff",
            "--no-renames",
            &format!("--unified={context}"),
            git_ref,
            "--",
            file,
        ])?;
        Ok(Patch {
            text: if result.truncated {
                String::new()
            } else {
                result.text()
            },
            truncated: result.truncated,
        })
    }

    /// Get a unified diff patch for all changed files.
    ///
    /// # Source
    /// `packages/opencode/src/git/index.ts` lines 271–277.
    pub fn patch_all(
        &self,
        git_ref: &str,
        options: Option<&PatchOptions>,
    ) -> Result<Patch, GitError> {
        let opts = options.cloned().unwrap_or_default();
        let context = opts.context.unwrap_or(3);
        let result = self.run(&[
            "diff",
            "--patch",
            "--no-ext-diff",
            "--no-renames",
            &format!("--unified={context}"),
            git_ref,
            "--",
            ".",
        ])?;
        Ok(Patch {
            text: result.text(),
            truncated: result.truncated,
        })
    }

    /// Get a unified diff for an untracked file (vs /dev/null).
    ///
    /// # Source
    /// `packages/opencode/src/git/index.ts` lines 279–299.
    pub fn patch_untracked(
        &self,
        file: &str,
        options: Option<&PatchOptions>,
    ) -> Result<Patch, GitError> {
        let opts = options.cloned().unwrap_or_default();
        let context = opts.context.unwrap_or(3);
        let result = self.run(&[
            "diff",
            "--no-index",
            "--patch",
            "--no-ext-diff",
            "--no-renames",
            &format!("--unified={context}"),
            "--",
            "/dev/null",
            file,
        ])?;
        Ok(Patch {
            text: if result.truncated {
                String::new()
            } else {
                result.text()
            },
            truncated: result.truncated,
        })
    }

    /// Get stats for an untracked file.
    ///
    /// # Source
    /// `packages/opencode/src/git/index.ts` lines 301–320.
    pub fn stat_untracked(&self, file: &str) -> Result<Option<Stat>, GitError> {
        let result = self.run(&["diff", "--no-index", "--numstat", "--", "/dev/null", file])?;
        if result.truncated {
            return Ok(None);
        }
        let text = result.text();
        let parts: Vec<&str> = text.split('\t').collect();
        if parts.len() < 2 {
            return Ok(None);
        }
        let additions = if parts[0] == "-" {
            0
        } else {
            parts[0].parse().unwrap_or(0)
        };
        let deletions = if parts[1] == "-" {
            0
        } else {
            parts[1].parse().unwrap_or(0)
        };
        Ok(Some(Stat {
            file: file.to_string(),
            additions,
            deletions,
        }))
    }

    /// Apply a patch via `git apply`.
    ///
    /// # Source
    /// `packages/opencode/src/git/index.ts` lines 322–324.
    pub fn apply_patch(&self, patch_text: &str) -> Result<GitResult, GitError> {
        let mut cmd = Command::new("git");
        cmd.args(&["apply", "-"])
            .current_dir(&self.worktree)
            .stdin(std::process::Stdio::piped())
            .stdout(std::process::Stdio::piped())
            .stderr(std::process::Stdio::piped());

        let mut child = cmd.spawn().map_err(GitError::Io)?;

        use std::io::Write;
        if let Some(mut stdin) = child.stdin.take() {
            stdin
                .write_all(patch_text.as_bytes())
                .map_err(GitError::Io)?;
        }

        let output = child.wait_with_output().map_err(GitError::Io)?;
        Ok(GitResult {
            exit_code: output.status.code().unwrap_or(1),
            stdout: output.stdout,
            stderr: output.stderr,
            truncated: false,
        })
    }

    // ── Higher-level operations (from core/src/git.ts) ──────────────────

    /// Find the git repository containing the given path.
    ///
    /// # Source
    /// `packages/core/src/git.ts` lines 85–102.
    pub fn find(&self) -> Result<Option<Repo>, GitError> {
        let top_level = self.run(&["rev-parse", "--show-toplevel"])?;
        let common_dir = self.run(&["rev-parse", "--git-common-dir"])?;

        if common_dir.exit_code != 0 {
            return Ok(None);
        }

        let directory = if top_level.exit_code == 0 {
            PathBuf::from(top_level.text().trim())
        } else {
            self.worktree.clone()
        };

        let store = resolve_path(&self.worktree, &common_dir.text().trim());

        Ok(Some(Repo { directory, store }))
    }

    /// Get the remote URL for the given repo.
    ///
    /// # Source
    /// `packages/core/src/git.ts` lines 104–108.
    pub fn remote(&self, name: &str) -> Result<Option<String>, GitError> {
        let result = self.run(&["remote", "get-url", name])?;
        if result.exit_code != 0 {
            return Ok(None);
        }
        let text = result.text().trim().to_string();
        Ok(if text.is_empty() { None } else { Some(text) })
    }

    /// Get root commits of HEAD.
    ///
    /// # Source
    /// `packages/core/src/git.ts` lines 110–118.
    pub fn roots(&self) -> Result<Vec<String>, GitError> {
        let result = self.run(&["rev-list", "--max-parents=0", "HEAD"])?;
        if result.exit_code != 0 {
            return Ok(Vec::new());
        }
        let mut commits: Vec<String> = result
            .text()
            .lines()
            .map(|l| l.trim().to_string())
            .filter(|l| !l.is_empty())
            .collect();
        commits.sort();
        Ok(commits)
    }

    /// Get the origin remote URL from config.
    ///
    /// # Source
    /// `packages/core/src/git.ts` lines 120–124.
    pub fn origin(&self) -> Result<Option<String>, GitError> {
        let result = self.run(&["config", "--get", "remote.origin.url"])?;
        if result.exit_code != 0 {
            return Ok(None);
        }
        let text = result.text().trim().to_string();
        Ok(if text.is_empty() { None } else { Some(text) })
    }

    /// Reset hard to HEAD + clean untracked files.
    ///
    /// # Source
    /// `packages/core/src/git.ts` lines 273–298.
    pub fn reset_changes(&self) -> Result<(), GitError> {
        let reset = self.run(&["reset", "--hard", "HEAD"])?;
        if reset.exit_code != 0 {
            return Err(GitError::Patch {
                operation: "reset".into(),
                directory: self.worktree.clone(),
                message: reset.stderr_text().trim().to_string(),
            });
        }
        let clean = self.run(&["clean", "-fd"])?;
        if clean.exit_code != 0 {
            return Err(GitError::Patch {
                operation: "reset".into(),
                directory: self.worktree.clone(),
                message: clean.stderr_text().trim().to_string(),
            });
        }
        Ok(())
    }

    /// Soft reset — checkout changed files and clean untracked.
    ///
    /// # Source
    /// `packages/core/src/git.ts` lines 301–327.
    pub fn soft_reset_changes(&self) -> Result<(), GitError> {
        let checkout = self.run(&["checkout", "--", "."])?;
        if checkout.exit_code != 0 {
            return Err(GitError::Patch {
                operation: "reset".into(),
                directory: self.worktree.clone(),
                message: checkout.stderr_text().trim().to_string(),
            });
        }
        let clean = self.run(&["clean", "-fd", "--", "."])?;
        if clean.exit_code != 0 {
            return Err(GitError::Patch {
                operation: "reset".into(),
                directory: self.worktree.clone(),
                message: clean.stderr_text().trim().to_string(),
            });
        }
        Ok(())
    }

    // ── Worktree operations ─────────────────────────────────────────────

    /// Create a new worktree.
    ///
    /// # Source
    /// `packages/core/src/git.ts` lines 353–355.
    pub fn worktree_create(&self, directory: &Path) -> Result<(), GitError> {
        let result = self.run(&[
            "worktree",
            "add",
            "--detach",
            &directory.to_string_lossy(),
            "HEAD",
        ])?;
        if result.exit_code != 0 {
            return Err(GitError::Worktree {
                operation: "create".into(),
                message: result.stderr_text().trim().to_string(),
                directory: Some(directory.to_path_buf()),
                force_required: false,
            });
        }
        Ok(())
    }

    /// Remove a worktree.
    ///
    /// # Source
    /// `packages/core/src/git.ts` lines 357–368.
    pub fn worktree_remove(&self, directory: &Path, force: bool) -> Result<(), GitError> {
        let mut args = vec!["worktree", "remove"];
        if force {
            args.push("--force");
        }
        let dir_str = directory.to_string_lossy().to_string();
        args.push(&dir_str);

        let result = self.run(&args.iter().map(|s| *s).collect::<Vec<_>>())?;
        if result.exit_code != 0 {
            let msg = result.stderr_text().trim().to_string();
            let msg_lower = msg.to_lowercase();
            let force_required = msg_lower.contains("contains modified or untracked files")
                || msg_lower.contains("is dirty");
            return Err(GitError::Worktree {
                operation: "remove".into(),
                message: msg,
                directory: Some(directory.to_path_buf()),
                force_required,
            });
        }
        Ok(())
    }

    /// List worktrees.
    ///
    /// # Source
    /// `packages/core/src/git.ts` lines 371–376.
    pub fn worktree_list(&self) -> Result<Vec<PathBuf>, GitError> {
        let result = self.run(&["worktree", "list", "--porcelain"])?;
        Ok(result
            .text()
            .lines()
            .filter(|line| line.starts_with("worktree "))
            .map(|line| {
                let path_str = line["worktree ".len()..].trim();
                resolve_path(&self.worktree, path_str)
            })
            .collect())
    }

    /// Capture a full binary patch of all changes in the worktree.
    ///
    /// # Source
    /// `packages/core/src/git.ts` lines 179–247.
    pub fn capture_patch(&self) -> Result<String, GitError> {
        let root_result = self.run(&["rev-parse", "--show-toplevel"])?;
        if root_result.exit_code != 0 {
            return Err(GitError::Patch {
                operation: "capture".into(),
                directory: self.worktree.clone(),
                message: "Failed to locate repository root".into(),
            });
        }

        let repo_root = PathBuf::from(root_result.text().trim());
        let scope = relative_path(&self.worktree, &repo_root)
            .unwrap_or_else(|| ".".to_string())
            .replace('\\', "/");

        // Tracked changes
        let tracked = self.git_at(&repo_root, &["diff", "--binary", "HEAD", "--", &scope])?;
        if tracked.exit_code != 0 {
            return Err(GitError::Patch {
                operation: "capture".into(),
                directory: self.worktree.clone(),
                message: tracked.stderr_text().trim().to_string(),
            });
        }

        // Untracked files
        let untracked = self.git_at(
            &repo_root,
            &[
                "ls-files",
                "--others",
                "--exclude-standard",
                "-z",
                "--",
                &scope,
            ],
        )?;
        if untracked.exit_code != 0 {
            return Err(GitError::Patch {
                operation: "capture".into(),
                directory: self.worktree.clone(),
                message: untracked.stderr_text().trim().to_string(),
            });
        }

        let mut patch_text = tracked.text();
        for file in untracked.text().split('\0').filter(|s| !s.is_empty()) {
            let diff_result = self.git_at(
                &repo_root,
                &["diff", "--binary", "--no-index", "--", "/dev/null", file],
            )?;
            // exit code 0 or 1 both mean we got diff output
            if diff_result.exit_code == 0 || diff_result.exit_code == 1 {
                patch_text.push_str(&diff_result.text());
            }
        }

        Ok(patch_text)
    }

    // ── Helpers ─────────────────────────────────────────────────────────

    /// Get the primary remote (origin > sole remote > upstream > first).
    fn primary_remote(&self) -> Result<Option<String>, GitError> {
        let lines = self.null_lines(&["remote"])?;
        if lines.contains(&"origin".to_string()) {
            return Ok(Some("origin".into()));
        }
        if lines.len() == 1 {
            return Ok(Some(lines[0].clone()));
        }
        if lines.contains(&"upstream".to_string()) {
            return Ok(Some("upstream".into()));
        }
        Ok(lines.into_iter().next())
    }

    /// Run git in a specific directory.
    fn git_at(&self, cwd: &Path, args: &[&str]) -> Result<GitResult, GitError> {
        let mut cmd = Command::new("git");
        cmd.args(args).current_dir(cwd);

        let output = cmd.output().map_err(GitError::Io)?;
        Ok(GitResult {
            exit_code: output.status.code().unwrap_or(1),
            stdout: output.stdout,
            stderr: output.stderr,
            truncated: false,
        })
    }
}

// ══════════════════════════════════════════════════════════════════════════════
// Utilities
// ══════════════════════════════════════════════════════════════════════════════

/// Map a porcelain status code to a Kind.
///
/// # Source
/// `packages/opencode/src/git/index.ts` lines 93–99.
fn kind_from_code(code: &str) -> Kind {
    if code == "??" {
        return Kind::Added;
    }
    if code.contains('U') {
        return Kind::Modified;
    }
    if code.contains('A') && !code.contains('D') {
        return Kind::Added;
    }
    if code.contains('D') && !code.contains('A') {
        return Kind::Deleted;
    }
    Kind::Modified
}

/// Resolve a path relative to a base directory.
///
/// # Source
/// `packages/core/src/git.ts` lines 439–445.
fn resolve_path(cwd: &Path, value: &str) -> PathBuf {
    let trimmed = value.trim_end_matches(['\r', '\n']);
    if trimmed.is_empty() {
        return cwd.to_path_buf();
    }
    let path = Path::new(trimmed);
    if path.is_absolute() {
        path.to_path_buf()
    } else {
        cwd.join(path)
    }
}

/// Compute a relative path from `from` to `to`, returning None if they
/// don't share a common prefix.
fn relative_path(from: &Path, to: &Path) -> Option<String> {
    let from = from.canonicalize().ok()?;
    let to = to.canonicalize().ok()?;
    let mut from_components = from.components().peekable();
    let mut to_components = to.components().peekable();

    // Skip common prefix
    loop {
        match (from_components.peek(), to_components.peek()) {
            (Some(a), Some(b)) if a == b => {
                from_components.next();
                to_components.next();
            }
            _ => break,
        }
    }

    // Build "../" for each remaining from component
    let mut result = String::new();
    for _ in from_components {
        result.push_str("../");
    }
    // Append remaining to components
    for component in to_components {
        if !result.is_empty() && !result.ends_with('/') {
            result.push('/');
        }
        result.push_str(&component.as_os_str().to_string_lossy());
    }

    if result.is_empty() {
        Some(".".to_string())
    } else {
        Some(result)
    }
}

// ══════════════════════════════════════════════════════════════════════════════
// Tests
// ══════════════════════════════════════════════════════════════════════════════

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_kind_from_code_untracked() {
        assert_eq!(kind_from_code("??"), Kind::Added);
    }

    #[test]
    fn test_kind_from_code_modified() {
        assert_eq!(kind_from_code(" M"), Kind::Modified);
        assert_eq!(kind_from_code("MM"), Kind::Modified);
    }

    #[test]
    fn test_kind_from_code_added() {
        assert_eq!(kind_from_code("A "), Kind::Added);
        assert_eq!(kind_from_code("A"), Kind::Added);
    }

    #[test]
    fn test_kind_from_code_deleted() {
        assert_eq!(kind_from_code(" D"), Kind::Deleted);
        assert_eq!(kind_from_code("D "), Kind::Deleted);
    }

    #[test]
    fn test_kind_from_code_renamed() {
        // Rename is M + A/D → classified as Modified
        assert_eq!(kind_from_code("RM"), Kind::Modified);
    }

    #[test]
    fn test_patch_options_default() {
        let opts = PatchOptions::default();
        assert_eq!(opts.context, Some(3));
        assert_eq!(opts.max_output_bytes, None);
    }

    #[test]
    fn test_git_result_text() {
        let result = GitResult {
            exit_code: 0,
            stdout: b"hello world\n".to_vec(),
            stderr: vec![],
            truncated: false,
        };
        assert_eq!(result.text(), "hello world\n");
    }

    #[test]
    fn test_kind_serde() {
        let added = Kind::Added;
        let json = serde_json::to_string(&added).unwrap();
        assert_eq!(json, r#""added""#);

        let parsed: Kind = serde_json::from_str(r#""modified""#).unwrap();
        assert_eq!(parsed, Kind::Modified);
    }

    #[test]
    fn test_stat_serde() {
        let stat = Stat {
            file: "src/main.rs".into(),
            additions: 10,
            deletions: 3,
        };
        let json = serde_json::to_string(&stat).unwrap();
        let parsed: Stat = serde_json::from_str(&json).unwrap();
        assert_eq!(parsed.file, "src/main.rs");
        assert_eq!(parsed.additions, 10);
        assert_eq!(parsed.deletions, 3);
    }

    // ── Status porcelain parsing edge cases ───────────────────────────────

    #[test]
    fn test_kind_from_code_all_porcelain_codes() {
        // Ignored: "!!" -> Modified
        assert_eq!(kind_from_code("!!"), Kind::Modified);
        // Added in index: "A " -> Added
        assert_eq!(kind_from_code("A "), Kind::Added);
        // Modified in worktree: " M" -> Modified
        assert_eq!(kind_from_code(" M"), Kind::Modified);
        // Modified in both: "MM" -> Modified
        assert_eq!(kind_from_code("MM"), Kind::Modified);
        // Deleted in worktree: " D" -> Deleted
        assert_eq!(kind_from_code(" D"), Kind::Deleted);
        // Deleted in index: "D " -> Deleted
        assert_eq!(kind_from_code("D "), Kind::Deleted);
        // Renamed in index: "R " -> Modified (falls through to default)
        assert_eq!(kind_from_code("R "), Kind::Modified);
        // Added + Modified: "AM" -> Added (contains A, no D)
        assert_eq!(kind_from_code("AM"), Kind::Added);
        // Added then Deleted: "AD" -> Modified (contains both A and D)
        assert_eq!(kind_from_code("AD"), Kind::Modified);
        // Type change in worktree: " T" -> Modified
        assert_eq!(kind_from_code(" T"), Kind::Modified);
    }

    #[test]
    fn test_kind_from_code_unmerged_status() {
        // Unmerged, both deleted: "DD" -> Deleted (contains D, no A)
        assert_eq!(kind_from_code("DD"), Kind::Deleted);
        // Unmerged, added by us: "AU" -> Modified (contains U)
        assert_eq!(kind_from_code("AU"), Kind::Modified);
        // Unmerged, deleted by them: "UD" -> Modified (contains U)
        assert_eq!(kind_from_code("UD"), Kind::Modified);
        // Unmerged, added by them: "UA" -> Modified (contains U)
        assert_eq!(kind_from_code("UA"), Kind::Modified);
        // Unmerged, deleted by us: "DU" -> Modified (contains U)
        assert_eq!(kind_from_code("DU"), Kind::Modified);
        // Unmerged, both added: "AA" -> Added (contains A, no D)
        assert_eq!(kind_from_code("AA"), Kind::Added);
        // Unmerged, both modified: "UU" -> Modified (contains U)
        assert_eq!(kind_from_code("UU"), Kind::Modified);
    }

    #[test]
    fn test_item_struct_creation() {
        let item = Item {
            file: "src/main.rs".into(),
            code: "M ".into(),
            status: Kind::Modified,
        };
        assert_eq!(item.file, "src/main.rs");
        assert_eq!(item.code, "M ");
        assert_eq!(item.status, Kind::Modified);

        let item = Item {
            file: "new_file.txt".into(),
            code: "??".into(),
            status: Kind::Added,
        };
        assert_eq!(item.file, "new_file.txt");
        assert_eq!(item.code, "??");
        assert_eq!(item.status, Kind::Added);

        let item = Item {
            file: "removed.rs".into(),
            code: "D ".into(),
            status: Kind::Deleted,
        };
        assert_eq!(item.file, "removed.rs");
        assert_eq!(item.code, "D ");
        assert_eq!(item.status, Kind::Deleted);
    }

    // ── Diff parsing edge cases ───────────────────────────────────────────

    #[test]
    fn test_item_diff_output_single_char_code() {
        // git diff --name-status uses single-character codes like "M", "A", "D"
        let item = Item {
            file: "src/main.rs".into(),
            code: "M".into(),
            status: Kind::Modified,
        };
        assert_eq!(item.file, "src/main.rs");
        assert_eq!(item.code, "M");
        assert_eq!(item.status, Kind::Modified);

        let item = Item {
            file: "lib/util.ts".into(),
            code: "A".into(),
            status: Kind::Added,
        };
        // kind_from_code("A") returns Added (contains A, no D)
        assert_eq!(kind_from_code("A"), Kind::Added);
        assert_eq!(item.status, Kind::Added);
    }

    #[test]
    fn test_stat_binary_file_dash_values() {
        // Binary files: additions and deletions are reported as "-"
        let stat = Stat {
            file: "binary.bin".into(),
            additions: 0,
            deletions: 0,
        };
        assert_eq!(stat.file, "binary.bin");
        assert_eq!(stat.additions, 0);
        assert_eq!(stat.deletions, 0);
    }

    #[test]
    fn test_stat_with_valid_numbers() {
        let stat = Stat {
            file: "src/lib.rs".into(),
            additions: 150,
            deletions: 42,
        };
        assert_eq!(stat.file, "src/lib.rs");
        assert_eq!(stat.additions, 150);
        assert_eq!(stat.deletions, 42);

        let stat = Stat {
            file: "Cargo.toml".into(),
            additions: 3,
            deletions: 1,
        };
        assert_eq!(stat.file, "Cargo.toml");
        assert_eq!(stat.additions, 3);
        assert_eq!(stat.deletions, 1);
    }

    // ── Patch untracked binary handling ───────────────────────────────────

    #[test]
    fn test_patch_with_truncated_flag() {
        // Binary content patches may be truncated
        let patch = Patch {
            text: String::new(),
            truncated: true,
        };
        assert!(patch.text.is_empty());
        assert!(patch.truncated);

        let patch = Patch {
            text: "diff --git a/file b/file\n...".into(),
            truncated: false,
        };
        assert!(!patch.text.is_empty());
        assert!(!patch.truncated);
    }

    #[test]
    fn test_patch_options_custom_values() {
        let opts = PatchOptions {
            context: Some(5),
            max_output_bytes: Some(1024 * 1024),
        };
        assert_eq!(opts.context, Some(5));
        assert_eq!(opts.max_output_bytes, Some(1024 * 1024));

        let opts = PatchOptions {
            context: None,
            max_output_bytes: None,
        };
        assert_eq!(opts.context, None);
        assert_eq!(opts.max_output_bytes, None);

        let opts = PatchOptions {
            context: Some(0),
            max_output_bytes: Some(512),
        };
        assert_eq!(opts.context, Some(0));
        assert_eq!(opts.max_output_bytes, Some(512));
    }

    // ── Branch listing ────────────────────────────────────────────────────

    #[test]
    fn test_base_serde_roundtrip() {
        let base = Base {
            name: "main".into(),
            ref_name: "origin/main".into(),
        };
        let json = serde_json::to_string(&base).expect("serialize Base");
        // The ref_name field should be serialized as "ref"
        assert!(json.contains("\"ref\""));
        assert!(json.contains("origin/main"));
        assert!(json.contains("main"));

        let parsed: Base = serde_json::from_str(&json).expect("deserialize Base");
        assert_eq!(parsed.name, "main");
        assert_eq!(parsed.ref_name, "origin/main");
    }

    // ── Merge base ────────────────────────────────────────────────────────

    #[test]
    fn test_repo_struct_fields() {
        let repo = Repo {
            directory: PathBuf::from("/home/user/project"),
            store: PathBuf::from("/home/user/project/.git"),
        };
        assert_eq!(repo.directory, PathBuf::from("/home/user/project"));
        assert_eq!(repo.store, PathBuf::from("/home/user/project/.git"));
    }

    #[test]
    fn test_git_result_non_zero_exit() {
        let result = GitResult {
            exit_code: 1,
            stdout: vec![],
            stderr: b"error: something went wrong\n".to_vec(),
            truncated: false,
        };
        assert_eq!(result.exit_code, 1);
        assert_eq!(result.stderr_text(), "error: something went wrong\n");
        assert_eq!(result.text(), "");
    }

    // ── Capture patch binary-safe ─────────────────────────────────────────

    #[test]
    fn test_git_result_with_binary_stdout() {
        // Binary data in stdout should be preserved as raw bytes
        let result = GitResult {
            exit_code: 0,
            stdout: vec![0x00, 0x01, 0x02, 0x80, 0xFF],
            stderr: vec![],
            truncated: false,
        };
        assert_eq!(result.exit_code, 0);
        assert_eq!(result.stdout, vec![0x00, 0x01, 0x02, 0x80, 0xFF]);
        assert_eq!(result.stdout.len(), 5);
    }

    #[test]
    fn test_git_result_text_lossy_conversion() {
        // Non-UTF8 bytes should use lossy conversion in text()
        let result = GitResult {
            exit_code: 0,
            stdout: vec![0x48, 0x65, 0x6C, 0x6C, 0x6F, 0xFF, 0xFE, 0x21],
            stderr: vec![],
            truncated: false,
        };
        let text = result.text();
        // Should contain the valid parts with replacement characters for invalid bytes
        assert!(text.starts_with("Hello"));
        assert!(text.contains('�'));
    }

    // ── Additional edge case tests ────────────────────────────────────────

    #[test]
    fn test_kind_from_code_edge_cases() {
        // Empty code -> Modified (doesn't match any branch, falls through to default)
        assert_eq!(kind_from_code(""), Kind::Modified);
        // Unknown code -> Modified
        assert_eq!(kind_from_code("XY"), Kind::Modified);
        // Single space -> Modified
        assert_eq!(kind_from_code("  "), Kind::Modified);
        // Code with only digits -> Modified
        assert_eq!(kind_from_code("12"), Kind::Modified);
    }

    #[test]
    fn test_patch_options_context_edge_values() {
        // Zero context (no surrounding lines)
        let opts = PatchOptions {
            context: Some(0),
            max_output_bytes: None,
        };
        assert_eq!(opts.context, Some(0));

        // Large context
        let opts = PatchOptions {
            context: Some(100),
            max_output_bytes: None,
        };
        assert_eq!(opts.context, Some(100));

        // No context specified (None, different from Default which is Some(3))
        let opts = PatchOptions {
            context: None,
            max_output_bytes: None,
        };
        assert_eq!(opts.context, None);

        // Default has context=Some(3)
        let opts = PatchOptions::default();
        assert_eq!(opts.context, Some(3));
    }

    #[test]
    fn test_item_serde_roundtrip() {
        let item = Item {
            file: "src/parser.rs".into(),
            code: "MM".into(),
            status: Kind::Modified,
        };
        let json = serde_json::to_string(&item).expect("serialize Item");
        let parsed: Item = serde_json::from_str(&json).expect("deserialize Item");
        assert_eq!(parsed.file, "src/parser.rs");
        assert_eq!(parsed.code, "MM");
        assert_eq!(parsed.status, Kind::Modified);

        // Test with Added status
        let item = Item {
            file: "new_file.txt".into(),
            code: "A ".into(),
            status: Kind::Added,
        };
        let json = serde_json::to_string(&item).expect("serialize Item");
        let parsed: Item = serde_json::from_str(&json).expect("deserialize Item");
        assert_eq!(parsed.file, "new_file.txt");
        assert_eq!(parsed.code, "A ");
        assert_eq!(parsed.status, Kind::Added);
    }
}
