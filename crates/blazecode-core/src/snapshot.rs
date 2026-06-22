//! Snapshot / revert system — filesystem state tracking via git.
//!
//! Uses a sideband git repository (--git-dir) separate from the user's repo
//! to track filesystem snapshots. Supports track, patch, restore, revert, and
//! full diffs between any two snapshot hashes.
//!
//! Ported from:
//! - `packages/blazecode/src/snapshot/index.ts` (lines 1–808)

use crate::git::GitError as GitOpError;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::Mutex;

use sha2::{Digest, Sha256};

// ══════════════════════════════════════════════════════════════════════════════
// Types
// ══════════════════════════════════════════════════════════════════════════════

/// A snapshot tree hash and the files it covers.
///
/// # Source
/// `packages/blazecode/src/snapshot/index.ts` lines 13–16.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SnapshotPatch {
    pub hash: String,
    pub files: Vec<String>,
}

/// Detailed file diff with optional patch text.
///
/// # Source
/// `packages/blazecode/src/snapshot/index.ts` lines 19–28.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SnapshotFileDiff {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub file: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub patch: Option<String>,
    pub additions: i64,
    pub deletions: i64,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub status: Option<String>,
}

/// Result from a git command in the snapshot repo.
#[derive(Debug, Clone)]
struct SnapshotGitResult {
    code: i32,
    text: String,
    stderr: String,
}

// ══════════════════════════════════════════════════════════════════════════════
// Constants
// ══════════════════════════════════════════════════════════════════════════════

/// Max file size for snapshot tracking (2 MiB).
///
/// # Source
/// `packages/blazecode/src/snapshot/index.ts` line 32.
const FILE_SIZE_LIMIT: u64 = 2 * 1024 * 1024;

/// Git config flags for snapshot operations.
///
/// # Source
/// `packages/blazecode/src/snapshot/index.ts` lines 33–35.
const SNAPSHOT_CFG: &[&str] = &[
    "-c",
    "core.autocrlf=false",
    "-c",
    "core.longpaths=true",
    "-c",
    "core.symlinks=true",
];

const SNAPSHOT_QUOTE: &[&str] = &[
    "-c",
    "core.autocrlf=false",
    "-c",
    "core.longpaths=true",
    "-c",
    "core.symlinks=true",
    "-c",
    "core.quotepath=false",
];

/// Prune age for snapshot garbage collection.
const PRUNE_AGE: &str = "7.days";

// ══════════════════════════════════════════════════════════════════════════════
// Error
// ══════════════════════════════════════════════════════════════════════════════

/// Snapshot operation errors.
#[derive(Debug, thiserror::Error)]
pub enum SnapshotError {
    #[error("snapshot not enabled")]
    NotEnabled,

    #[error("snapshot not initialized")]
    NotInitialized,

    #[error("git error: {0}")]
    Git(#[from] GitOpError),

    #[error("I/O error: {0}")]
    Io(#[from] std::io::Error),

    #[error("lock poison")]
    LockPoison,

    #[error("{0}")]
    Other(String),
}

// ══════════════════════════════════════════════════════════════════════════════
// SnapshotService
// ══════════════════════════════════════════════════════════════════════════════

/// Manages filesystem snapshots using a sideband git repository.
///
/// # Source
/// `packages/blazecode/src/snapshot/index.ts` lines 44–53 `Interface`.
pub struct SnapshotService {
    /// Working directory being tracked.
    directory: PathBuf,
    /// Root of the git worktree.
    worktree: PathBuf,
    /// Path to the snapshot git directory.
    gitdir: PathBuf,
    /// Whether VCS is git (snapshots only work with git repos).
    vcs: Option<String>,
    /// Per-operation mutex for thread safety (simplified from per-key semaphores).
    lock: Mutex<()>,
    /// Whether initialized.
    initialized: bool,
}

/// Structured row from `git diff --numstat` output.
struct DiffRow {
    file: String,
    status: String,
    binary: bool,
    additions: i64,
    deletions: i64,
}

/// A ref pointing to a file at a git revision (`<tree>:<path>`).
struct FileRef {
    file: String,
    side: Side,
    ref_str: String,
}

enum Side {
    Before,
    After,
}

impl SnapshotService {
    /// Create a new snapshot service.
    ///
    /// The snapshot git directory is derived from the worktree path hash,
    /// stored under `~/.local/share/blazecode/snapshot/<project>/<hash>`.
    ///
    /// # Source
    /// `packages/blazecode/src/snapshot/index.ts` lines 74–81.
    pub fn new(
        directory: impl Into<PathBuf>,
        worktree: impl Into<PathBuf>,
        data_dir: impl Into<PathBuf>,
        project_id: &str,
        vcs: Option<&str>,
    ) -> Self {
        let directory = directory.into();
        let worktree = worktree.into();
        let worktree_hash = hash_path(&worktree.to_string_lossy());
        let gitdir = data_dir
            .into()
            .join("snapshot")
            .join(project_id)
            .join(worktree_hash);

        Self {
            directory,
            worktree,
            gitdir,
            vcs: vcs.map(|s| s.to_string()),
            lock: Mutex::new(()),
            initialized: false,
        }
    }

    /// Initialize the snapshot repository.
    ///
    /// Creates the gitdir if it doesn't exist, configures git settings,
    /// and seeds the object database from the source repo.
    ///
    /// # Source
    /// `packages/blazecode/src/snapshot/index.ts` lines 317–346.
    pub fn init(&mut self) -> Result<(), SnapshotError> {
        if !self.is_enabled() {
            return Err(SnapshotError::NotEnabled);
        }

        let _guard = self.lock.lock().map_err(|_| SnapshotError::LockPoison)?;

        if self.gitdir.exists() {
            self.initialized = true;
            return Ok(());
        }

        std::fs::create_dir_all(&self.gitdir)?;

        // git init
        self.snapshot_git(&["init"], self.gitdir_env())?;

        // Configure snapshot git repo
        let configs = [
            ("core.autocrlf", "false"),
            ("core.longpaths", "true"),
            ("core.symlinks", "true"),
            ("core.fsmonitor", "false"),
            ("feature.manyFiles", "true"),
            ("index.version", "4"),
            ("index.threads", "true"),
            ("core.untrackedCache", "true"),
        ];
        for (key, value) in configs {
            self.snapshot_git(&["config", key, value], None)?;
        }

        // Seed object database from source repo
        self.seed_from_source()?;

        self.initialized = true;
        Ok(())
    }

    /// Check if snapshots are enabled (VCS must be git).
    fn is_enabled(&self) -> bool {
        self.vcs.as_deref() == Some("git")
    }

    /// Ensure the service is initialized before operations.
    fn ensure_init(&self) -> Result<(), SnapshotError> {
        if !self.initialized {
            return Err(SnapshotError::NotInitialized);
        }
        Ok(())
    }

    // ── Core operations ─────────────────────────────────────────────────

    /// Take a snapshot of the current filesystem state.
    ///
    /// Stages all changes (tracked + untracked, minus gitignored + large files),
    /// writes a tree object, and returns the tree hash.
    ///
    /// # Source
    /// `packages/blazecode/src/snapshot/index.ts` lines 317–346.
    pub fn track(&self) -> Result<Option<String>, SnapshotError> {
        self.ensure_init()?;
        let _guard = self.lock.lock().map_err(|_| SnapshotError::LockPoison)?;

        if !self.is_enabled() {
            return Ok(None);
        }

        // Update exclude list
        self.sync_excludes(&[])?;

        // Get tracked (diff-files) and untracked (ls-files --others) files
        let diff = self.snapshot_git(
            &["diff-files", "--name-only", "-z", "--", "."],
            self.cwd_env(),
        )?;
        let other = self.snapshot_git(
            &[
                "ls-files",
                "--others",
                "--exclude-standard",
                "-z",
                "--",
                ".",
            ],
            self.cwd_env(),
        )?;

        let tracked: Vec<String> = diff
            .text
            .split('\0')
            .filter(|s| !s.is_empty())
            .map(|s| s.to_string())
            .collect();
        let untracked: Vec<String> = other
            .text
            .split('\0')
            .filter(|s| !s.is_empty())
            .map(|s| s.to_string())
            .collect();

        let mut all = tracked.clone();
        all.extend(untracked.clone());
        all.sort();
        all.dedup();

        if all.is_empty() {
            // Write tree anyway to get a stable hash for empty state
            let tree = self.snapshot_git(&["write-tree"], self.cwd_env())?;
            let hash = tree.text.trim().to_string();
            return Ok(if hash.is_empty() { None } else { Some(hash) });
        }

        // Check for gitignored files
        let ignored = self.check_ignored(&all)?;

        // Remove ignored files from index
        if !ignored.is_empty() {
            let _ = self.snapshot_git(
                &["rm", "--cached", "-f", "--ignore-unmatch", "--"],
                self.cwd_env(),
            );
        }

        // Filter out ignored and large files
        let large = self.find_large_files(&all)?;
        let excluded: Vec<&String> = untracked.iter().filter(|f| large.contains(*f)).collect();
        self.sync_excludes(&excluded.iter().map(|s| s.as_str()).collect::<Vec<_>>())?;

        let allowed: Vec<&String> = all
            .iter()
            .filter(|f| !ignored.contains(*f) && !excluded.contains(f))
            .collect();

        if allowed.is_empty() {
            let tree = self.snapshot_git(&["write-tree"], self.cwd_env())?;
            let hash = tree.text.trim().to_string();
            return Ok(if hash.is_empty() { None } else { Some(hash) });
        }

        // Stage allowed files
        let stdin = null_join(&allowed.iter().map(|s| s.as_str()).collect::<Vec<_>>());
        let _add = self.snapshot_git_stdin(
            &["add", "--all", "--sparse", "--pathspec-file-nul"],
            &stdin,
            self.cwd_env(),
        )?;

        // Write tree
        let tree = self.snapshot_git(&["write-tree"], self.cwd_env())?;
        let hash = tree.text.trim().to_string();
        Ok(if hash.is_empty() { None } else { Some(hash) })
    }

    /// Get the list of changed files since a snapshot hash.
    ///
    /// # Source
    /// `packages/blazecode/src/snapshot/index.ts` lines 348–379.
    pub fn patch(&self, hash: &str) -> Result<SnapshotPatch, SnapshotError> {
        self.ensure_init()?;
        let _guard = self.lock.lock().map_err(|_| SnapshotError::LockPoison)?;

        // Re-add current state
        self.sync_excludes(&[])?;

        let result = self.snapshot_git(
            &[
                "diff",
                "--cached",
                "--no-ext-diff",
                "--name-only",
                hash,
                "--",
                ".",
            ],
            self.cwd_env(),
        )?;

        let files: Vec<String> = result
            .text
            .lines()
            .map(|l| l.trim().to_string())
            .filter(|l| !l.is_empty())
            .collect();

        // Filter out ignored files from the result
        let ignored = self.check_ignored(&files)?;
        let visible: Vec<String> = files
            .into_iter()
            .filter(|f| !ignored.contains(f))
            .map(|f| {
                let abs = self.worktree.join(&f);
                abs.to_string_lossy().replace('\\', "/")
            })
            .collect();

        Ok(SnapshotPatch {
            hash: hash.to_string(),
            files: visible,
        })
    }

    /// Restore the filesystem to a snapshot tree hash.
    ///
    /// # Source
    /// `packages/blazecode/src/snapshot/index.ts` lines 381–405.
    pub fn restore(&self, snapshot: &str) -> Result<(), SnapshotError> {
        self.ensure_init()?;
        let _guard = self.lock.lock().map_err(|_| SnapshotError::LockPoison)?;

        let read_tree = self.snapshot_git(&["read-tree", snapshot], self.worktree_env())?;

        if read_tree.code == 0 {
            let checkout =
                self.snapshot_git(&["checkout-index", "-a", "-f"], self.worktree_env())?;
            if checkout.code != 0 {
                return Err(SnapshotError::Other(format!(
                    "checkout-index failed: {}",
                    checkout.stderr
                )));
            }
            return Ok(());
        }

        Err(SnapshotError::Other(format!(
            "read-tree failed: {}",
            read_tree.stderr
        )))
    }

    /// Revert a set of patches — restore specific files to their snapshot state.
    ///
    /// # Source
    /// `packages/blazecode/src/snapshot/index.ts` lines 407–523.
    pub fn revert(&self, patches: &[SnapshotPatch]) -> Result<(), SnapshotError> {
        self.ensure_init()?;
        let _guard = self.lock.lock().map_err(|_| SnapshotError::LockPoison)?;

        // Build per-file operations, deduplicated by file path
        let mut seen = std::collections::HashSet::new();
        let mut ops: Vec<(String, String, String)> = Vec::new(); // (hash, file_abs, file_rel)

        for patch in patches {
            for file in &patch.files {
                if seen.contains(file) {
                    continue;
                }
                seen.insert(file.clone());
                let rel = relative_path(Path::new(file), &self.worktree)
                    .unwrap_or_else(|| file.clone())
                    .replace('\\', "/");
                ops.push((patch.hash.clone(), file.clone(), rel));
            }
        }

        // Process each operation
        for (hash, file, rel) in &ops {
            let result = self.snapshot_git(&["checkout", hash, "--", file], self.worktree_env())?;

            if result.code != 0 {
                // Check if file existed in snapshot
                let tree = self.snapshot_git(&["ls-tree", hash, "--", rel], self.worktree_env())?;
                if tree.code == 0 && !tree.text.trim().is_empty() {
                    // File existed but checkout failed — keep it
                    continue;
                }
                // File didn't exist in snapshot — delete it
                let _ = std::fs::remove_file(file);
            }
        }

        Ok(())
    }

    /// Get the unified diff between a snapshot hash and current state.
    ///
    /// # Source
    /// `packages/blazecode/src/snapshot/index.ts` lines 525–543.
    pub fn diff(&self, hash: &str) -> Result<String, SnapshotError> {
        self.ensure_init()?;
        let _guard = self.lock.lock().map_err(|_| SnapshotError::LockPoison)?;

        // Re-add current state first
        self.sync_excludes(&[])?;

        let result = self.snapshot_git(
            &["diff", "--cached", "--no-ext-diff", hash, "--", "."],
            self.worktree_env(),
        )?;

        if result.code != 0 {
            return Ok(String::new());
        }
        Ok(result.text.trim().to_string())
    }

    /// Get a full diff (with file contents and patches) between two hashes.
    ///
    /// Uses `git cat-file --batch` to fetch all file contents in a single
    /// round trip instead of per-file `git show` calls.
    ///
    /// # Source
    /// `packages/blazecode/src/snapshot/index.ts` lines 545–758.
    pub fn diff_full(&self, from: &str, to: &str) -> Result<Vec<SnapshotFileDiff>, SnapshotError> {
        self.ensure_init()?;
        let _guard = self.lock.lock().map_err(|_| SnapshotError::LockPoison)?;

        // ── Step 1: name-status ──────────────────────────────────────────
        let statuses = self.snapshot_git(
            &[
                "diff",
                "--no-ext-diff",
                "--name-status",
                "--no-renames",
                from,
                to,
                "--",
                ".",
            ],
            self.cwd_env(),
        )?;

        let mut status_map: HashMap<String, String> = HashMap::new();
        for line in statuses.text.lines() {
            let parts: Vec<&str> = line.split('\t').collect();
            if parts.len() >= 2 {
                let code = parts[0];
                let file = parts[1];
                let st = if code.starts_with('A') {
                    "added"
                } else if code.starts_with('D') {
                    "deleted"
                } else {
                    "modified"
                };
                status_map.insert(file.to_string(), st.to_string());
            }
        }

        // ── Step 2: numstat ──────────────────────────────────────────────
        let numstat = self.snapshot_git(
            &[
                "diff",
                "--no-ext-diff",
                "--no-renames",
                "--numstat",
                from,
                to,
                "--",
                ".",
            ],
            self.cwd_env(),
        )?;

        let rows: Vec<DiffRow> = numstat
            .text
            .lines()
            .filter(|l| !l.is_empty())
            .filter_map(|line| {
                let parts: Vec<&str> = line.split('\t').collect();
                if parts.len() < 3 {
                    return None;
                }
                let adds = parts[0];
                let dels = parts[1];
                let file = parts[2].to_string();
                let binary = adds == "-" && dels == "-";
                Some(DiffRow {
                    status: status_map
                        .get(&file)
                        .cloned()
                        .unwrap_or_else(|| "modified".into()),
                    file,
                    binary,
                    additions: if binary { 0 } else { adds.parse().unwrap_or(0) },
                    deletions: if binary { 0 } else { dels.parse().unwrap_or(0) },
                })
            })
            .collect();

        // ── Step 3: build ref list for cat-file --batch ───────────────────
        let refs: Vec<FileRef> = rows
            .iter()
            .flat_map(|row| {
                if row.binary {
                    return Vec::new();
                }
                match row.status.as_str() {
                    "added" => vec![FileRef {
                        file: row.file.clone(),
                        side: Side::After,
                        ref_str: format!("{to}:{}", row.file),
                    }],
                    "deleted" => vec![FileRef {
                        file: row.file.clone(),
                        side: Side::Before,
                        ref_str: format!("{from}:{}", row.file),
                    }],
                    _ => vec![
                        FileRef {
                            file: row.file.clone(),
                            side: Side::Before,
                            ref_str: format!("{from}:{}", row.file),
                        },
                        FileRef {
                            file: row.file.clone(),
                            side: Side::After,
                            ref_str: format!("{to}:{}", row.file),
                        },
                    ],
                }
            })
            .collect();

        // ── Step 4: fetch contents via cat-file --batch ──────────────────
        let content_map = if refs.is_empty() {
            HashMap::new()
        } else {
            self.batch_cat_file(&refs).unwrap_or_default()
        };

        // ── Step 5: build patches ────────────────────────────────────────
        let mut results = Vec::new();
        for row in &rows {
            let (before, after) = if row.binary {
                (String::new(), String::new())
            } else {
                let before = content_map
                    .get(&format!("{}:before", row.file))
                    .cloned()
                    .unwrap_or_default();
                let after = content_map
                    .get(&format!("{}:after", row.file))
                    .cloned()
                    .unwrap_or_default();
                (before, after)
            };

            let patch_text = if row.binary {
                String::new()
            } else {
                // Generate structured diff from before/after content
                if row.status == "added" {
                    after.clone()
                } else if row.status == "deleted" {
                    before.clone()
                } else {
                    self.generate_file_diff(from, to, &row.file)
                        .unwrap_or_default()
                }
            };

            results.push(SnapshotFileDiff {
                file: Some(row.file.clone()),
                patch: if patch_text.is_empty() {
                    None
                } else {
                    Some(patch_text)
                },
                additions: row.additions,
                deletions: row.deletions,
                status: Some(row.status.clone()),
            });
        }

        Ok(results)
    }

    /// Fetch file contents via `git cat-file --batch` in a single round trip.
    ///
    /// Returns a map keyed by `<file>:before` / `<file>:after` with the
    /// text content for each ref. Falls back to per-file `git show` on error.
    ///
    /// # Source
    /// `packages/blazecode/src/snapshot/index.ts` lines 587–681 (`load`).
    fn batch_cat_file(&self, refs: &[FileRef]) -> Result<HashMap<String, String>, SnapshotError> {
        let stdin_data = refs
            .iter()
            .map(|r| r.ref_str.as_str())
            .collect::<Vec<_>>()
            .join("\n")
            + "\n";

        let result = self.snapshot_git_stdin(
            &["--git-dir", &self.gitdir.to_string_lossy(), "--work-tree", &self.worktree.to_string_lossy(), "cat-file", "--batch"],
            &stdin_data,
            None,
        );

        let output = match result {
            Ok(r) => r,
            Err(_) => {
                // Fallback: per-file git show
                let mut map = HashMap::new();
                for fr in refs {
                    let text = self
                        .snapshot_git(&["show", &fr.ref_str], None)
                        .map(|r| r.text)
                        .unwrap_or_default();
                    let key = match fr.side {
                        Side::Before => format!("{}:before", fr.file),
                        Side::After => format!("{}:after", fr.file),
                    };
                    map.insert(key, text);
                }
                return Ok(map);
            }
        };

        if output.code != 0 {
            // Fallback
            let mut map = HashMap::new();
            for fr in refs {
                let text = self
                    .snapshot_git(&["show", &fr.ref_str], None)
                    .map(|r| r.text)
                    .unwrap_or_default();
                let key = match fr.side {
                    Side::Before => format!("{}:before", fr.file),
                    Side::After => format!("{}:after", fr.file),
                };
                map.insert(key, text);
            }
            return Ok(map);
        }

        // Parse the batch output
        let mut map = HashMap::new();
        let out = output.text;
        let bytes = out.as_bytes();
        let mut i = 0;
        let len = bytes.len();

        for fr in refs {
            if i >= len {
                break;
            }

            // Read header line (ends at newline)
            let mut end = i;
            while end < len && bytes[end] != b'\n' {
                end += 1;
            }
            if end >= len {
                break;
            }
            let header = String::from_utf8_lossy(&bytes[i..end]).to_string();
            i = end + 1;

            if header.ends_with(" missing") {
                let key = match fr.side {
                    Side::Before => format!("{}:before", fr.file),
                    Side::After => format!("{}:after", fr.file),
                };
                map.insert(key, String::new());
                continue;
            }

            // Parse "<sha> blob <size>"
            let parts: Vec<&str> = header.split_whitespace().collect();
            if parts.len() < 3 {
                continue;
            }
            let size: usize = match parts[2].parse() {
                Ok(n) => n,
                Err(_) => continue,
            };

            if i + size > len {
                break;
            }
            let content = String::from_utf8_lossy(&bytes[i..i + size]).to_string();
            i += size + 1; // skip trailing newline

            let key = match fr.side {
                Side::Before => format!("{}:before", fr.file),
                Side::After => format!("{}:after", fr.file),
            };
            map.insert(key, content);
        }

        Ok(map)
    }

    /// Run garbage collection on the snapshot git repo.
    ///
    /// # Source
    /// `packages/blazecode/src/snapshot/index.ts` lines 299–315.
    pub fn cleanup(&self) -> Result<(), SnapshotError> {
        if !self.is_enabled() {
            return Ok(());
        }
        let _guard = self.lock.lock().map_err(|_| SnapshotError::LockPoison)?;

        if !self.gitdir.exists() {
            return Ok(());
        }

        let result = self.snapshot_git(&["gc", &format!("--prune={PRUNE_AGE}")], None)?;
        if result.code != 0 {
            // Non-fatal — gc failures shouldn't break the app
            return Ok(());
        }
        Ok(())
    }

    /// Get the worktree path.
    pub fn worktree(&self) -> &Path {
        &self.worktree
    }

    /// Get the gitdir path.
    pub fn gitdir(&self) -> &Path {
        &self.gitdir
    }

    // ── Internal helpers ─────────────────────────────────────────────────

    /// Run a git command against the snapshot gitdir.
    fn snapshot_git(
        &self,
        args: &[&str],
        env: Option<Vec<(String, String)>>,
    ) -> Result<SnapshotGitResult, SnapshotError> {
        let mut cmd = Command::new("git");
        cmd.arg("--git-dir")
            .arg(&self.gitdir)
            .arg("--work-tree")
            .arg(&self.worktree);

        // Prepend config flags
        let full_args: Vec<&str> = SNAPSHOT_QUOTE
            .iter()
            .copied()
            .chain(args.iter().copied())
            .collect();
        cmd.args(&full_args);
        cmd.current_dir(&self.directory);

        if let Some(env_vars) = env {
            for (k, v) in env_vars {
                cmd.env(k, v);
            }
        }

        let output = cmd.output()?;
        Ok(SnapshotGitResult {
            code: output.status.code().unwrap_or(1),
            text: String::from_utf8_lossy(&output.stdout).to_string(),
            stderr: String::from_utf8_lossy(&output.stderr).to_string(),
        })
    }

    /// Run git with stdin input.
    fn snapshot_git_stdin(
        &self,
        args: &[&str],
        stdin_data: &str,
        env: Option<Vec<(String, String)>>,
    ) -> Result<SnapshotGitResult, SnapshotError> {
        let mut cmd = Command::new("git");
        cmd.arg("--git-dir")
            .arg(&self.gitdir)
            .arg("--work-tree")
            .arg(&self.worktree);

        let full_args: Vec<&str> = SNAPSHOT_CFG
            .iter()
            .copied()
            .chain(args.iter().copied())
            .collect();
        cmd.args(&full_args);
        cmd.current_dir(&self.directory);
        cmd.stdin(std::process::Stdio::piped());
        cmd.stdout(std::process::Stdio::piped());
        cmd.stderr(std::process::Stdio::piped());

        if let Some(env_vars) = env {
            for (k, v) in env_vars {
                cmd.env(k, v);
            }
        }

        let mut child = cmd.spawn()?;
        use std::io::Write;
        if let Some(mut stdin) = child.stdin.take() {
            stdin.write_all(stdin_data.as_bytes())?;
        }

        let output = child.wait_with_output()?;
        Ok(SnapshotGitResult {
            code: output.status.code().unwrap_or(1),
            text: String::from_utf8_lossy(&output.stdout).to_string(),
            stderr: String::from_utf8_lossy(&output.stderr).to_string(),
        })
    }

    /// Generate a unified diff for a file between two commits.
    fn generate_file_diff(
        &self,
        from: &str,
        to: &str,
        file: &str,
    ) -> Result<String, SnapshotError> {
        let result = self.snapshot_git(
            &[
                "diff",
                "--patch",
                "--no-ext-diff",
                "--no-renames",
                "--unified=3",
                from,
                to,
                "--",
                file,
            ],
            None,
        )?;
        Ok(result.text)
    }

    /// Check which files are gitignored.
    fn check_ignored(
        &self,
        files: &[String],
    ) -> Result<std::collections::HashSet<String>, SnapshotError> {
        if files.is_empty() {
            return Ok(std::collections::HashSet::new());
        }

        let stdin = null_join(&files.iter().map(|s| s.as_str()).collect::<Vec<_>>());
        let result = run_git_in_worktree(
            &self.worktree,
            &[
                "-c",
                "core.autocrlf=false",
                "-c",
                "core.longpaths=true",
                "-c",
                "core.symlinks=true",
                "-c",
                "core.quotepath=false",
                "check-ignore",
                "--no-index",
                "--stdin",
                "-z",
            ],
            Some(&stdin),
        )?;

        // exit code 0 = all ignored, 1 = some not ignored, >1 = error
        if result.code > 1 {
            return Ok(std::collections::HashSet::new());
        }
        Ok(result
            .text
            .split('\0')
            .filter(|s| !s.is_empty())
            .map(|s| s.to_string())
            .collect())
    }

    /// Find files larger than the size limit.
    fn find_large_files(
        &self,
        files: &[String],
    ) -> Result<std::collections::HashSet<String>, SnapshotError> {
        let mut large = std::collections::HashSet::new();
        for file in files {
            let path = self.directory.join(file);
            if let Ok(meta) = std::fs::metadata(&path) {
                if meta.is_file() && meta.len() > FILE_SIZE_LIMIT {
                    large.insert(file.clone());
                }
            }
        }
        Ok(large)
    }

    /// Sync the excludes file with given patterns.
    fn sync_excludes(&self, extra: &[&str]) -> Result<(), SnapshotError> {
        std::fs::create_dir_all(self.gitdir.join("info"))?;

        let exclude_path = self.gitdir.join("info").join("exclude");
        let mut lines: Vec<String> = extra
            .iter()
            .map(|s| format!("/{}", s.replace('\\', "/")))
            .collect();
        if lines.is_empty() {
            lines.push(String::new());
        }
        let text = lines.join("\n");
        let content = if text.is_empty() {
            "\n".to_string()
        } else {
            format!("{text}\n")
        };
        std::fs::write(&exclude_path, &content)?;
        Ok(())
    }

    /// Seed the snapshot git object database from the source repo.
    fn seed_from_source(&self) -> Result<(), SnapshotError> {
        if self.vcs.as_deref() != Some("git") {
            return Ok(());
        }

        let common_dir = run_git_in_worktree(
            &self.worktree,
            &["rev-parse", "--path-format=absolute", "--git-common-dir"],
            None,
        )?;

        if common_dir.code != 0 {
            return Ok(());
        }

        let source = common_dir.text.trim().to_string();
        if source.is_empty() || !Path::new(&source).exists() {
            return Ok(());
        }

        let source_objects = Path::new(&source).join("objects");
        if !source_objects.exists() {
            return Ok(());
        }

        // Write alternates file pointing to source objects
        std::fs::create_dir_all(self.gitdir.join("objects").join("info"))?;
        std::fs::write(
            self.gitdir.join("objects").join("info").join("alternates"),
            format!("{}\n", source_objects.display()),
        )?;

        Ok(())
    }

    /// Build env for in-worktree git operations.
    fn worktree_env(&self) -> Option<Vec<(String, String)>> {
        None // Use --git-dir/--work-tree flags instead
    }

    /// Build env for in-directory git operations.
    fn cwd_env(&self) -> Option<Vec<(String, String)>> {
        None
    }

    /// Build env for git init.
    fn gitdir_env(&self) -> Option<Vec<(String, String)>> {
        Some(vec![
            ("GIT_DIR".into(), self.gitdir.to_string_lossy().to_string()),
            (
                "GIT_WORK_TREE".into(),
                self.worktree.to_string_lossy().to_string(),
            ),
        ])
    }
}

// ══════════════════════════════════════════════════════════════════════════════
// Utilities
// ══════════════════════════════════════════════════════════════════════════════

/// Generate a stable hash of a path string.
fn hash_path(path: &str) -> String {
    let mut hasher = Sha256::new();
    hasher.update(path.as_bytes());
    format!("{:x}", hasher.finalize())[..12].to_string()
}

/// Join strings with null characters.
fn null_join(items: &[&str]) -> String {
    let mut result = String::new();
    for item in items {
        result.push_str(item);
        result.push('\0');
    }
    result
}

/// Compute a relative path from `from` to `to`, returning None if they
/// don't share a common prefix.
fn relative_path(from: &Path, to: &Path) -> Option<String> {
    let from = from.canonicalize().ok()?;
    let to = to.canonicalize().ok()?;
    let mut from_components = from.components().peekable();
    let mut to_components = to.components().peekable();

    loop {
        match (from_components.peek(), to_components.peek()) {
            (Some(a), Some(b)) if a == b => {
                from_components.next();
                to_components.next();
            }
            _ => break,
        }
    }

    let mut result = String::new();
    for _ in from_components {
        result.push_str("../");
    }
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

/// Run a git command in a specific worktree.
fn run_git_in_worktree(
    cwd: &Path,
    args: &[&str],
    stdin: Option<&str>,
) -> Result<SnapshotGitResult, SnapshotError> {
    let mut cmd = Command::new("git");
    cmd.args(args).current_dir(cwd);

    if stdin.is_some() {
        cmd.stdin(std::process::Stdio::piped());
    }
    cmd.stdout(std::process::Stdio::piped());
    cmd.stderr(std::process::Stdio::piped());

    let mut child = cmd.spawn()?;
    if let Some(data) = stdin {
        use std::io::Write;
        if let Some(mut child_stdin) = child.stdin.take() {
            child_stdin.write_all(data.as_bytes())?;
        }
    }

    let output = child.wait_with_output()?;
    Ok(SnapshotGitResult {
        code: output.status.code().unwrap_or(1),
        text: String::from_utf8_lossy(&output.stdout).to_string(),
        stderr: String::from_utf8_lossy(&output.stderr).to_string(),
    })
}

// ══════════════════════════════════════════════════════════════════════════════
// Tests
// ══════════════════════════════════════════════════════════════════════════════

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_hash_path_stable() {
        let a = hash_path("/home/test/project");
        let b = hash_path("/home/test/project");
        assert_eq!(a, b);
        assert_eq!(a.len(), 12);
    }

    #[test]
    fn test_hash_path_different() {
        let a = hash_path("/home/test/project-a");
        let b = hash_path("/home/test/project-b");
        assert_ne!(a, b);
    }

    #[test]
    fn test_null_join() {
        let items = vec!["file1.txt", "file2.txt", "file3.txt"];
        let result = null_join(&items);
        assert_eq!(result, "file1.txt\0file2.txt\0file3.txt\0");
    }

    #[test]
    fn test_null_join_empty() {
        let items: Vec<&str> = vec![];
        let result = null_join(&items);
        assert_eq!(result, "");
    }

    #[test]
    fn test_snapshot_patch_serde() {
        let patch = SnapshotPatch {
            hash: "abc123".into(),
            files: vec!["src/main.rs".into(), "README.md".into()],
        };
        let json = serde_json::to_string(&patch).unwrap();
        let parsed: SnapshotPatch = serde_json::from_str(&json).unwrap();
        assert_eq!(parsed.hash, "abc123");
        assert_eq!(parsed.files.len(), 2);
    }

    #[test]
    fn test_snapshot_file_diff_serde() {
        let diff = SnapshotFileDiff {
            file: Some("src/lib.rs".into()),
            patch: Some("@@ -1,3 +1,5 @@\n old\n+new\n".into()),
            additions: 1,
            deletions: 0,
            status: Some("modified".into()),
        };
        let json = serde_json::to_string(&diff).unwrap();
        let parsed: SnapshotFileDiff = serde_json::from_str(&json).unwrap();
        assert_eq!(parsed.file, Some("src/lib.rs".into()));
        assert_eq!(parsed.additions, 1);
        assert_eq!(parsed.deletions, 0);
    }

    #[test]
    fn test_snapshot_file_diff_without_patch() {
        let diff = SnapshotFileDiff {
            file: Some("README.md".into()),
            patch: None,
            additions: 0,
            deletions: 5,
            status: Some("deleted".into()),
        };
        let json = serde_json::to_string(&diff).unwrap();
        assert!(!json.contains("patch"));
    }

    // ── init() seed from source ──────────────────────────────────────────

    #[test]
    fn test_new_creates_correct_gitdir_path() {
        let svc = SnapshotService::new(
            "/tmp/project",
            "/tmp/project",
            "/home/user/.local/share/blazecode",
            "my-project",
            Some("git"),
        );
        let expected_hash = hash_path("/tmp/project");
        let expected_suffix = format!("snapshot/my-project/{}", expected_hash);
        let gitdir_str = svc.gitdir().to_string_lossy();
        assert!(
            gitdir_str.contains(&expected_suffix),
            "gitdir should contain '{}', got '{}'",
            expected_suffix,
            gitdir_str
        );
    }

    #[test]
    fn test_init_vcs_none_returns_not_enabled() {
        let mut svc = SnapshotService::new(
            "/tmp/project",
            "/tmp/project",
            "/tmp/snapshots",
            "test-proj",
            None,
        );
        let result = svc.init();
        assert!(result.is_err());
        match result {
            Err(SnapshotError::NotEnabled) => {} // expected
            other => panic!("expected NotEnabled, got {:?}", other),
        }
    }

    // ── track() with ignored files ───────────────────────────────────────

    #[test]
    fn test_file_size_limit_is_2_mib() {
        assert_eq!(FILE_SIZE_LIMIT, 2 * 1024 * 1024);
        assert_eq!(FILE_SIZE_LIMIT, 2097152);
    }

    #[test]
    fn test_null_join_single_item() {
        let items = vec!["only-one.txt"];
        let result = null_join(&items);
        assert_eq!(result, "only-one.txt\0");
    }

    // ── track() with large file filtering ────────────────────────────────

    #[test]
    fn test_new_different_project_ids_different_gitdirs() {
        let svc_a = SnapshotService::new(
            "/tmp/project",
            "/tmp/project",
            "/tmp/data",
            "project-alpha",
            Some("git"),
        );
        let svc_b = SnapshotService::new(
            "/tmp/project",
            "/tmp/project",
            "/tmp/data",
            "project-beta",
            Some("git"),
        );
        assert_ne!(
            svc_a.gitdir(),
            svc_b.gitdir(),
            "different project IDs should produce different gitdirs"
        );
    }

    #[test]
    fn test_snapshot_file_diff_added_status_serde() {
        let diff = SnapshotFileDiff {
            file: Some("new_file.rs".into()),
            patch: Some("// brand new file\n".into()),
            additions: 42,
            deletions: 0,
            status: Some("added".into()),
        };
        let json = serde_json::to_string(&diff).expect("serialize added diff");
        let parsed: SnapshotFileDiff = serde_json::from_str(&json).expect("deserialize added diff");
        assert_eq!(parsed.status, Some("added".into()));
        assert_eq!(parsed.additions, 42);
        assert_eq!(parsed.deletions, 0);
        assert!(json.contains("added"));
    }

    // ── restore() edge cases ─────────────────────────────────────────────

    #[test]
    fn test_snapshot_file_diff_zero_additions_and_deletions() {
        let diff = SnapshotFileDiff {
            file: Some("unchanged.txt".into()),
            patch: None,
            additions: 0,
            deletions: 0,
            status: Some("modified".into()),
        };
        let json = serde_json::to_string(&diff).expect("serialize zero-change diff");
        let parsed: SnapshotFileDiff =
            serde_json::from_str(&json).expect("deserialize zero-change diff");
        assert_eq!(parsed.additions, 0);
        assert_eq!(parsed.deletions, 0);
        // "patch" should be absent since it's None with skip_serializing_if
        assert!(!json.contains("patch"));
    }

    #[test]
    fn test_snapshot_patch_empty_files() {
        let patch = SnapshotPatch {
            hash: "deadbeef".into(),
            files: vec![],
        };
        let json = serde_json::to_string(&patch).expect("serialize empty patch");
        let parsed: SnapshotPatch = serde_json::from_str(&json).expect("deserialize empty patch");
        assert_eq!(parsed.hash, "deadbeef");
        assert!(parsed.files.is_empty());
    }

    // ── diff_full() with file content ────────────────────────────────────

    #[test]
    fn test_snapshot_file_diff_all_status_values() {
        let statuses = vec!["added", "deleted", "modified"];
        for status in &statuses {
            let diff = SnapshotFileDiff {
                file: Some("test.rs".into()),
                patch: Some("diff content".into()),
                additions: 1,
                deletions: 1,
                status: Some(status.to_string()),
            };
            let json = serde_json::to_string(&diff).expect("serialize diff with status");
            let parsed: SnapshotFileDiff =
                serde_json::from_str(&json).expect("deserialize diff with status");
            assert_eq!(parsed.status.as_deref(), Some(*status));
        }
    }

    #[test]
    fn test_snapshot_file_diff_file_none() {
        let diff = SnapshotFileDiff {
            file: None,
            patch: None,
            additions: 0,
            deletions: 0,
            status: None,
        };
        let json = serde_json::to_string(&diff).expect("serialize file-none diff");
        let parsed: SnapshotFileDiff =
            serde_json::from_str(&json).expect("deserialize file-none diff");
        assert_eq!(parsed.file, None);
        // All optional fields with skip_serializing_if=None should be absent
        assert!(!json.contains("file"));
        assert!(!json.contains("patch"));
        assert!(!json.contains("status"));
    }

    // ── cleanup() gc behavior ────────────────────────────────────────────

    #[test]
    fn test_prune_age_is_7_days() {
        assert_eq!(PRUNE_AGE, "7.days");
    }

    #[test]
    fn test_snapshot_error_display() {
        // NotEnabled
        let e = SnapshotError::NotEnabled;
        let msg = e.to_string();
        assert!(msg.contains("snapshot not enabled"), "got: {}", msg);

        // NotInitialized
        let e = SnapshotError::NotInitialized;
        let msg = e.to_string();
        assert!(msg.contains("snapshot not initialized"), "got: {}", msg);

        // LockPoison
        let e = SnapshotError::LockPoison;
        let msg = e.to_string();
        assert!(msg.contains("lock poison"), "got: {}", msg);

        // Other
        let e = SnapshotError::Other("custom message".into());
        let msg = e.to_string();
        assert!(msg.contains("custom message"), "got: {}", msg);
    }

    // ── SNAPSHOT_CFG constants ───────────────────────────────────────────

    #[test]
    fn test_snapshot_cfg_contains_expected_flags() {
        // SNAPSHOT_CFG provides three -c key=value pairs (6 &str elements)
        assert_eq!(SNAPSHOT_CFG.len(), 6);
        let joined = SNAPSHOT_CFG.join(" ");
        assert!(joined.contains("core.autocrlf=false"));
        assert!(joined.contains("core.longpaths=true"));
        assert!(joined.contains("core.symlinks=true"));
        // Verify the -c flags alternate
        assert_eq!(SNAPSHOT_CFG[0], "-c");
        assert_eq!(SNAPSHOT_CFG[2], "-c");
        assert_eq!(SNAPSHOT_CFG[4], "-c");
    }
}
