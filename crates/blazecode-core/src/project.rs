//! Project types — ID, VCS, directories, copy strategies, SQL schema.
//!
//! Ported from:
//! - `packages/core/src/project.ts` — ProjectV2 namespace, Info, Resolved, Interface
//! - `packages/core/src/project/schema.ts` — ID (branded), Vcs union
//! - `packages/core/src/project/directories.ts` — Directory, CreateInput, RemoveInput, ListInput/Output
//! - `packages/core/src/project/sql.ts` — ProjectTable, ProjectDirectoryTable
//! - `packages/core/src/project/copy.ts` — Strategy, Copy, CreateInput/RemoveInput/RefreshInput
//! - `packages/core/src/project/copy-strategies.ts` — Git worktree strategy
//!
//! BlazeCode commit: 5d0f86606ac30690f79f0a6a9f41a1f49fe95d0b

use serde::{Deserialize, Serialize};

// ══════════════════════════════════════════════════════════════════════════════
// Project Schema — ID, Vcs
// ══════════════════════════════════════════════════════════════════════════════

/// Project identifier — a branded string with a `global` sentinel.
///
/// # Source
/// `packages/core/src/project/schema.ts` lines 6–12.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct ProjectId(pub String);

impl ProjectId {
    /// The global project ID used when no specific project is resolved.
    ///
    /// # Source
    /// `packages/core/src/project/schema.ts` line 10 — `ID.global`.
    pub const GLOBAL: &'static str = "global";

    /// Create a new project ID.
    pub fn new(id: impl Into<String>) -> Self {
        Self(id.into())
    }

    /// Return the global project ID.
    #[must_use]
    pub fn global() -> Self {
        Self(Self::GLOBAL.to_string())
    }

    /// Check whether this is the global sentinel.
    #[must_use]
    pub fn is_global(&self) -> bool {
        self.0 == Self::GLOBAL
    }
}

impl std::fmt::Display for ProjectId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.0)
    }
}

impl AsRef<str> for ProjectId {
    fn as_ref(&self) -> &str {
        &self.0
    }
}

/// Version control system metadata attached to a resolved project.
///
/// # Source
/// `packages/core/src/project/schema.ts` lines 14–20.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "type")]
pub enum ProjectVcs {
    /// A git repository — identified by its `.git` store directory.
    #[serde(rename = "git")]
    Git {
        /// Absolute path to the `.git` store directory.
        store: String,
    },
}

impl ProjectVcs {
    /// Create a git VCS entry.
    #[must_use]
    pub fn git(store: impl Into<String>) -> Self {
        Self::Git {
            store: store.into(),
        }
    }
}

// ══════════════════════════════════════════════════════════════════════════════
// Project Info and Resolved
// ══════════════════════════════════════════════════════════════════════════════

/// Minimal project info — just the ID.
///
/// # Source
/// `packages/core/src/project.ts` lines 20–22.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ProjectInfo {
    pub id: ProjectId,
}

/// Fully resolved project information after directory resolution.
///
/// # Source
/// `packages/core/src/project.ts` lines 30–35.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ProjectResolved {
    /// The previous project ID, if a cached ID was found before re-resolution.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub previous: Option<ProjectId>,

    /// The current project ID (never `None` — defaults to global).
    pub id: ProjectId,

    /// The root directory of the resolved project.
    pub directory: String,

    /// Optional VCS metadata (e.g., git store location).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub vcs: Option<ProjectVcs>,
}

// ══════════════════════════════════════════════════════════════════════════════
// Project Directories
// ══════════════════════════════════════════════════════════════════════════════

/// A project directory entry — directory path and optional copy strategy.
///
/// # Source
/// `packages/core/src/project/directories.ts` lines 12–15.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ProjectDirectory {
    /// Absolute path to the directory.
    pub directory: String,

    /// Optional copy strategy name (e.g., `"git_worktree"`).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub strategy: Option<String>,
}

/// Input for creating a project directory record.
///
/// # Source
/// `packages/core/src/project/directories.ts` lines 17–23.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ProjectDirectoryCreateInput {
    /// The project this directory belongs to.
    pub project_id: ProjectId,

    /// Absolute path to the directory.
    pub directory: String,

    /// Optional copy strategy identifier.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub strategy: Option<String>,

    /// Conflict resolution behavior.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub behavior: Option<ProjectDirectoryCreateBehavior>,
}

/// Behavior when a directory record already exists.
///
/// # Source
/// `packages/core/src/project/directories.ts` line 22.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ProjectDirectoryCreateBehavior {
    /// Silently skip the insert on conflict.
    Ignore,
    /// Replace the existing row (update strategy if changed).
    Replace,
}

/// Input for removing a project directory record.
///
/// # Source
/// `packages/core/src/project/directories.ts` lines 25–29.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ProjectDirectoryRemoveInput {
    pub project_id: ProjectId,
    pub directory: String,
}

/// Input for listing project directories.
///
/// # Source
/// `packages/core/src/project/directories.ts` lines 34–37.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ProjectDirectoryListInput {
    pub project_id: ProjectId,
}

/// Output entry for listing project directories.
///
/// # Source
/// `packages/core/src/project/directories.ts` lines 39–44.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ProjectDirectoryListOutput {
    pub directory: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub strategy: Option<String>,
}

// ══════════════════════════════════════════════════════════════════════════════
// Project Copy
// ══════════════════════════════════════════════════════════════════════════════

/// Project copy strategy identifier — a non-empty trimmed string brand.
///
/// # Source
/// `packages/core/src/project/copy.ts` line 18.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct StrategyId(pub String);

impl StrategyId {
    /// Create a new strategy ID.
    #[must_use]
    pub fn new(id: impl Into<String>) -> Self {
        Self(id.into())
    }

    /// The built-in git worktree strategy identifier.
    pub const GIT_WORKTREE: &'static str = "git_worktree";
}

impl std::fmt::Display for StrategyId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.0)
    }
}

/// Input for creating a project copy.
///
/// # Source
/// `packages/core/src/project/copy.ts` lines 21–28.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ProjectCopyCreateInput {
    pub project_id: ProjectId,
    pub strategy: StrategyId,
    pub source_directory: String,
    pub directory: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,
}

/// Input for removing a project copy.
///
/// # Source
/// `packages/core/src/project/copy.ts` lines 30–35.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ProjectCopyRemoveInput {
    pub project_id: ProjectId,
    pub directory: String,
    /// Whether to force removal (bypass safety checks).
    pub force: bool,
}

/// Input for refreshing all copies of a project.
///
/// # Source
/// `packages/core/src/project/copy.ts` lines 37–40.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ProjectCopyRefreshInput {
    pub project_id: ProjectId,
}

/// Result of a project copy refresh operation.
///
/// # Source
/// `packages/core/src/project/copy.ts` lines 42–46.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ProjectCopyRefreshResult {
    /// Directories that were updated during the refresh.
    pub updated: Vec<String>,
    /// Directories that were removed during the refresh.
    pub removed: Vec<String>,
}

/// A project copy — just the directory it lives in.
///
/// # Source
/// `packages/core/src/project/copy.ts` lines 48–51.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ProjectCopyInfo {
    pub directory: String,
}

/// Entry in a copy listing — distinguishes root from copy directories.
///
/// # Source
/// `packages/core/src/project/copy.ts` lines 53–57.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "type")]
pub enum ProjectCopyListEntry {
    /// The original root directory.
    #[serde(rename = "root")]
    Root { directory: String },
    /// A derived copy directory.
    #[serde(rename = "copy")]
    Copy { directory: String },
}

/// Project copy errors.
///
/// # Source
/// `packages/core/src/project/copy.ts` lines 59–95.
#[derive(Debug, Clone, thiserror::Error, Serialize, Deserialize)]
pub enum ProjectCopyError {
    /// Source directory does not exist or is not tracked.
    #[error("source directory not found: {directory}")]
    SourceDirectoryNotFound { directory: String },

    /// Destination directory already exists.
    #[error("destination already exists: {directory}")]
    DestinationExists { directory: String },

    /// Directory is unavailable (not a valid directory).
    #[error("directory unavailable: {directory}")]
    DirectoryUnavailable { directory: String },

    /// Directory is invalid for the requested operation.
    #[error("invalid directory: {directory}")]
    InvalidDirectory { directory: String },

    /// The requested copy strategy is not registered.
    #[error("strategy unavailable: {strategy}")]
    StrategyUnavailable { strategy: StrategyId },

    /// A strategy with this ID is already registered.
    #[error("duplicate strategy: {strategy}")]
    DuplicateStrategy { strategy: StrategyId },

    /// Git worktree-related errors.
    #[error("git worktree error: {0}")]
    Worktree(String),
}

// ══════════════════════════════════════════════════════════════════════════════
// Project SQL — table definitions (type-only; SQLx impl uses separate DDL)
// ══════════════════════════════════════════════════════════════════════════════

/// Column names for the `project` SQLite table.
///
/// # Source
/// `packages/core/src/project/sql.ts` lines 6–18.
pub const PROJECT_TABLE_COLUMNS: &[&str] = &[
    "id",
    "worktree",
    "vcs",
    "name",
    "icon_url",
    "icon_url_override",
    "icon_color",
    "time_created",
    "time_updated",
    "time_deleted",
    "time_initialized",
    "sandboxes",
    "commands",
];

/// A row from the `project` SQLite table.
///
/// # Source
/// `packages/core/src/project/sql.ts` lines 6–18.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ProjectRow {
    pub id: ProjectId,
    pub worktree: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub vcs: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub icon_url: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub icon_url_override: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub icon_color: Option<String>,
    pub time_created: i64,
    pub time_updated: i64,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub time_deleted: Option<i64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub time_initialized: Option<i64>,
    pub sandboxes: Vec<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub commands: Option<ProjectCommandsConfig>,
}

/// JSON payload stored in the `commands` column.
///
/// # Source
/// `packages/core/src/project/sql.ts` line 17.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct ProjectCommandsConfig {
    /// Start command for the project sandbox.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub start: Option<String>,
}

/// Column names for the `project_directory` SQLite table.
///
/// # Source
/// `packages/core/src/project/sql.ts` lines 20–35.
pub const PROJECT_DIRECTORY_TABLE_COLUMNS: &[&str] = &[
    "project_id",
    "directory",
    "type",
    "strategy",
    "time_created",
];

/// A row from the `project_directory` SQLite table.
///
/// # Source
/// `packages/core/src/project/sql.ts` lines 20–35.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ProjectDirectoryRow {
    pub project_id: ProjectId,
    pub directory: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub r#type: Option<ProjectDirectoryType>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub strategy: Option<String>,
    pub time_created: i64,
}

/// The `type` column of `project_directory` — distinguishes main, root, and git worktree entries.
///
/// # Source
/// `packages/core/src/project/sql.ts` line 28.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ProjectDirectoryType {
    Main,
    Root,
    #[serde(rename = "git_worktree")]
    GitWorktree,
}

// ══════════════════════════════════════════════════════════════════════════════
// Project Service — detection, creation, listing, validation
// ══════════════════════════════════════════════════════════════════════════════

use std::path::{Path, PathBuf};

/// Errors that can occur during project service operations.
///
/// # Source
/// Ported from error patterns in `packages/core/src/project.ts`.
#[derive(Debug, thiserror::Error)]
pub enum ProjectServiceError {
    /// The project directory does not exist.
    #[error("directory not found: {0}")]
    DirectoryNotFound(String),

    /// No project could be detected in or above the given directory.
    #[error("no project found in or above: {0}")]
    NoProjectFound(String),

    /// The project already exists (conflict on create).
    #[error("project already exists: {0}")]
    AlreadyExists(String),

    /// Project validation failed.
    #[error("project validation failed: {0}")]
    ValidationFailed(String),

    /// Git operation failed.
    #[error("git error: {0}")]
    GitError(String),

    /// An I/O error occurred.
    #[error("io error: {0}")]
    Io(#[from] std::io::Error),
}

/// Result of project detection from a directory.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ProjectDetection {
    /// The detected project ID.
    pub id: ProjectId,
    /// The project root directory.
    pub directory: String,
    /// Whether a .git directory was found.
    pub has_git: bool,
    /// The path to the .git store (if found).
    pub git_store: Option<String>,
    /// The detected VCS type.
    pub vcs: Option<String>,
    /// Whether the detection found an blazecode config file.
    pub has_blazecode_config: bool,
}

/// A project entry for listing.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ProjectEntry {
    /// The project ID.
    pub id: ProjectId,
    /// A human-readable name.
    pub name: Option<String>,
    /// The project root directory.
    pub directory: String,
    /// VCS type.
    pub vcs: Option<String>,
    /// When the project was created (epoch millis).
    pub time_created: i64,
    /// When the project was last updated (epoch millis).
    pub time_updated: i64,
}

/// Project service — detects, creates, lists, and validates projects.
///
/// # Source
/// Ported from `packages/core/src/project.ts` `ProjectV2` namespace.
pub struct ProjectService {
    /// Root directory to search for projects (defaults to home or cwd).
    search_root: PathBuf,
}

impl ProjectService {
    /// Create a new project service.
    pub fn new(search_root: impl Into<PathBuf>) -> Self {
        Self {
            search_root: search_root.into(),
        }
    }

    /// Detect the current project from the given directory (or cwd).
    ///
    /// Walks up from `start_dir` looking for `.git` or an blazecode config file.
    /// Returns the first project found, or `NoProjectFound` if none exists.
    ///
    /// # Source
    /// Ported from `packages/core/src/project.ts` `resolve()` method (lines 110–122).
    pub fn detect(&self, start_dir: &Path) -> Result<ProjectDetection, ProjectServiceError> {
        let start = if start_dir.is_absolute() {
            start_dir.to_path_buf()
        } else {
            std::env::current_dir()
                .map_err(ProjectServiceError::Io)?
                .join(start_dir)
        };

        // Canonicalize if possible
        let start = start.canonicalize().unwrap_or(start);

        // Walk up looking for .git or blazecode config
        let mut current = start.clone();
        loop {
            let git_path = current.join(".git");
            let blazecode_path = current.join(".blazecode");
            let blazecode_config = current.join("blazecode.json");

            let has_git = git_path.exists();
            let has_blazecode_config = blazecode_path.exists() || blazecode_config.exists();

            if has_git || has_blazecode_config {
                let project_id = self.compute_project_id(&current);
                let vcs = if has_git {
                    Some("git".to_string())
                } else {
                    None
                };

                return Ok(ProjectDetection {
                    id: project_id,
                    directory: current.display().to_string(),
                    has_git,
                    git_store: if has_git {
                        Some(git_path.display().to_string())
                    } else {
                        None
                    },
                    vcs,
                    has_blazecode_config,
                });
            }

            // Go up one level
            if let Some(parent) = current.parent() {
                if parent == current {
                    // Reached filesystem root
                    break;
                }
                current = parent.to_path_buf();
            } else {
                break;
            }
        }

        Err(ProjectServiceError::NoProjectFound(
            start.display().to_string(),
        ))
    }

    /// Detect the current project from the current working directory.
    pub fn current(&self) -> Result<ProjectDetection, ProjectServiceError> {
        let cwd = std::env::current_dir().map_err(ProjectServiceError::Io)?;
        self.detect(&cwd)
    }

    /// Create a new project at the given directory with initial configuration.
    ///
    /// # Source
    /// Ported from project initialization patterns in the TS codebase.
    pub fn create(
        &self,
        directory: &Path,
        name: Option<&str>,
    ) -> Result<ProjectDetection, ProjectServiceError> {
        let dir = if directory.is_absolute() {
            directory.to_path_buf()
        } else {
            std::env::current_dir()
                .map_err(ProjectServiceError::Io)?
                .join(directory)
        };

        // Ensure directory exists
        if dir.exists() {
            // Check if already a project
            if dir.join(".git").exists() || dir.join(".blazecode").exists() {
                return Err(ProjectServiceError::AlreadyExists(
                    dir.display().to_string(),
                ));
            }
        } else {
            std::fs::create_dir_all(&dir)?;
        }

        // Initialize git if needed
        self.init_git(&dir)?;

        // Write blazecode config file
        let blazecode_dir = dir.join(".blazecode");
        std::fs::create_dir_all(&blazecode_dir)?;

        let config = serde_json::json!({
            "name": name.unwrap_or_else(|| dir.file_name()
                .and_then(|n| n.to_str())
                .unwrap_or("unnamed")),
            "version": "0.1.0",
            "created_at": chrono::Utc::now().timestamp_millis(),
        });

        let config_path = blazecode_dir.join("project.json");
        std::fs::write(
            &config_path,
            serde_json::to_string_pretty(&config)
                .map_err(|e| ProjectServiceError::Io(std::io::Error::other(e.to_string())))?,
        )?;

        Ok(ProjectDetection {
            id: self.compute_project_id(&dir),
            directory: dir.display().to_string(),
            has_git: true,
            git_store: Some(dir.join(".git").display().to_string()),
            vcs: Some("git".to_string()),
            has_blazecode_config: true,
        })
    }

    /// List known projects under the search root.
    ///
    /// Scans for directories containing `.git` or `.blazecode` markers.
    pub fn list(&self, max_depth: u32) -> Result<Vec<ProjectEntry>, ProjectServiceError> {
        let mut projects: Vec<ProjectEntry> = Vec::new();
        self.scan_for_projects(&self.search_root, max_depth, 0, &mut projects)?;

        // Deduplicate by directory
        projects.sort_by(|a, b| a.directory.cmp(&b.directory));
        projects.dedup_by(|a, b| a.directory == b.directory);

        Ok(projects)
    }

    /// Initialize a git repository at the given directory.
    ///
    /// # Source
    /// Ported from `packages/core/src/project.ts` git init patterns.
    pub fn init_git(&self, directory: &Path) -> Result<(), ProjectServiceError> {
        if directory.join(".git").exists() {
            return Ok(()); // Already initialized
        }

        // Try running `git init`
        let output = std::process::Command::new("git")
            .args(["init", "--quiet"])
            .current_dir(directory)
            .output()
            .map_err(|e| ProjectServiceError::GitError(format!("failed to run git init: {e}")))?;

        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr);
            return Err(ProjectServiceError::GitError(format!(
                "git init failed: {stderr}"
            )));
        }

        Ok(())
    }

    /// Validate a project's integrity.
    ///
    /// Checks:
    /// - The directory exists and is accessible
    /// - The blazecode config file is valid JSON (if present)
    /// - The git repository is not corrupted (light check)
    pub fn validate(&self, directory: &Path) -> Result<Vec<String>, ProjectServiceError> {
        let mut issues: Vec<String> = Vec::new();

        if !directory.exists() {
            return Err(ProjectServiceError::DirectoryNotFound(
                directory.display().to_string(),
            ));
        }

        if !directory.is_dir() {
            issues.push(format!("path is not a directory: {}", directory.display()));
            return Ok(issues);
        }

        // Check blazecode config
        let config_path = directory.join(".blazecode").join("project.json");
        if config_path.exists() {
            match std::fs::read_to_string(&config_path) {
                Ok(content) => {
                    if serde_json::from_str::<serde_json::Value>(&content).is_err() {
                        issues.push(format!(
                            "invalid JSON in blazecode config: {}",
                            config_path.display()
                        ));
                    }
                }
                Err(e) => {
                    issues.push(format!("cannot read blazecode config: {e}"));
                }
            }
        }

        // Check git integrity (light)
        let git_dir = directory.join(".git");
        if git_dir.exists() {
            if !git_dir.is_dir() {
                issues.push(format!(".git exists but is not a directory: {directory:?}"));
            } else {
                // Check that HEAD exists (basic integrity check)
                let head_path = git_dir.join("HEAD");
                if !head_path.exists() {
                    issues.push(format!("git HEAD missing: {}", head_path.display()));
                }
            }
        }

        Ok(issues)
    }

    // ── Internal helpers ─────────────────────────────────────────────

    /// Compute a stable project ID from the directory path.
    fn compute_project_id(&self, directory: &Path) -> ProjectId {
        let path_str = directory.display().to_string();
        // Use SHA-256 hex of the normalized path as the project ID
        use sha2::Digest;
        let hash = sha2::Sha256::digest(path_str.as_bytes());
        let hex = hex::encode(&hash[..16]); // First 16 bytes = 32 hex chars
        ProjectId::new(format!("proj_{hex}"))
    }

    /// Recursively scan for projects.
    fn scan_for_projects(
        &self,
        dir: &Path,
        max_depth: u32,
        current_depth: u32,
        results: &mut Vec<ProjectEntry>,
    ) -> Result<(), ProjectServiceError> {
        if current_depth > max_depth || !dir.is_dir() {
            return Ok(());
        }

        // Check if this directory itself is a project
        if dir.join(".git").exists() || dir.join(".blazecode").exists() {
            let project_id = self.compute_project_id(dir);
            let now = chrono::Utc::now().timestamp_millis();

            // Try to read project name from config
            let name = dir
                .join(".blazecode")
                .join("project.json")
                .exists()
                .then(|| {
                    std::fs::read_to_string(dir.join(".blazecode").join("project.json"))
                        .ok()
                        .and_then(|s| serde_json::from_str::<serde_json::Value>(&s).ok())
                        .and_then(|v| v.get("name")?.as_str().map(String::from))
                })
                .flatten()
                .or_else(|| dir.file_name().and_then(|n| n.to_str()).map(String::from));

            results.push(ProjectEntry {
                id: project_id,
                name,
                directory: dir.display().to_string(),
                vcs: if dir.join(".git").exists() {
                    Some("git".to_string())
                } else {
                    None
                },
                time_created: now,
                time_updated: now,
            });
            return Ok(()); // Don't recurse into project directories
        }

        // Recurse into subdirectories (skip hidden dirs)
        if let Ok(entries) = std::fs::read_dir(dir) {
            for entry in entries.flatten() {
                let name = entry.file_name();
                let name_str = name.to_string_lossy();
                // Skip hidden directories and common non-project dirs
                if name_str.starts_with('.')
                    || name_str == "node_modules"
                    || name_str == "target"
                    || name_str == "dist"
                    || name_str == "build"
                {
                    continue;
                }
                if entry.file_type().map(|ft| ft.is_dir()).unwrap_or(false) {
                    self.scan_for_projects(&entry.path(), max_depth, current_depth + 1, results)?;
                }
            }
        }

        Ok(())
    }
}

// ══════════════════════════════════════════════════════════════════════════════
// Tests
// ══════════════════════════════════════════════════════════════════════════════

#[cfg(test)]
mod tests {
    use super::*;

    // ── ProjectId ──────────────────────────────────────────────────

    #[test]
    fn test_project_id_new() {
        let id = ProjectId::new("proj_abc123");
        assert_eq!(id.0, "proj_abc123");
    }

    #[test]
    fn test_project_id_global() {
        let id = ProjectId::global();
        assert_eq!(id.0, "global");
        assert!(id.is_global());
    }

    #[test]
    fn test_project_id_is_global_false() {
        let id = ProjectId::new("custom");
        assert!(!id.is_global());
    }

    #[test]
    fn test_project_id_display() {
        let id = ProjectId::new("test-id");
        assert_eq!(id.to_string(), "test-id");
        assert_eq!(id.as_ref(), "test-id");
    }

    #[test]
    fn test_project_id_serde_roundtrip() {
        let id = ProjectId::new("proj_1");
        let json = serde_json::to_string(&id).expect("serialize");
        let parsed: ProjectId = serde_json::from_str(&json).expect("deserialize");
        assert_eq!(parsed, id);
    }

    // ── ProjectVcs ─────────────────────────────────────────────────

    #[test]
    fn test_project_vcs_git() {
        let vcs = ProjectVcs::git("/repo/.git");
        assert_eq!(
            serde_json::to_string(&vcs).expect("serialize"),
            r#"{"type":"git","store":"/repo/.git"}"#
        );
    }

    #[test]
    fn test_project_vcs_deserialize() {
        let json = r#"{"type":"git","store":"/home/proj/.git"}"#;
        let vcs: ProjectVcs = serde_json::from_str(json).expect("deserialize");
        assert_eq!(vcs, ProjectVcs::git("/home/proj/.git"));
    }

    // ── ProjectInfo ────────────────────────────────────────────────

    #[test]
    fn test_project_info_serde() {
        let info = ProjectInfo {
            id: ProjectId::new("proj_x"),
        };
        let json = serde_json::to_string(&info).expect("serialize");
        let parsed: ProjectInfo = serde_json::from_str(&json).expect("deserialize");
        assert_eq!(parsed.id.0, "proj_x");
    }

    // ── ProjectResolved ────────────────────────────────────────────

    #[test]
    fn test_project_resolved_full() {
        let resolved = ProjectResolved {
            previous: Some(ProjectId::new("old_id")),
            id: ProjectId::new("new_id"),
            directory: "/home/proj".into(),
            vcs: Some(ProjectVcs::git("/home/proj/.git")),
        };
        let json = serde_json::to_string(&resolved).expect("serialize");
        let parsed: ProjectResolved = serde_json::from_str(&json).expect("deserialize");
        assert_eq!(parsed.id, ProjectId::new("new_id"));
        assert_eq!(parsed.previous, Some(ProjectId::new("old_id")));
        assert!(parsed.vcs.is_some());
    }

    #[test]
    fn test_project_resolved_minimal() {
        let resolved = ProjectResolved {
            previous: None,
            id: ProjectId::global(),
            directory: "/".into(),
            vcs: None,
        };
        let json = serde_json::to_string(&resolved).expect("serialize");
        let parsed: ProjectResolved = serde_json::from_str(&json).expect("deserialize");
        assert!(parsed.id.is_global());
        assert!(parsed.vcs.is_none());
    }

    // ── ProjectDirectoryCreateBehavior ─────────────────────────────

    #[test]
    fn test_directory_create_behavior_serde() {
        assert_eq!(
            serde_json::to_string(&ProjectDirectoryCreateBehavior::Ignore).expect("serialize"),
            r#""ignore""#
        );
        assert_eq!(
            serde_json::to_string(&ProjectDirectoryCreateBehavior::Replace).expect("serialize"),
            r#""replace""#
        );
    }

    // ── ProjectDirectoryCreateInput ────────────────────────────────

    #[test]
    fn test_directory_create_input_serde() {
        let input = ProjectDirectoryCreateInput {
            project_id: ProjectId::new("p1"),
            directory: "/src".into(),
            strategy: Some("git_worktree".into()),
            behavior: Some(ProjectDirectoryCreateBehavior::Ignore),
        };
        let json = serde_json::to_string(&input).expect("serialize");
        let parsed: ProjectDirectoryCreateInput = serde_json::from_str(&json).expect("deserialize");
        assert_eq!(parsed.project_id, ProjectId::new("p1"));
        assert_eq!(parsed.strategy.as_deref(), Some("git_worktree"));
    }

    // ── StrategyId ─────────────────────────────────────────────────

    #[test]
    fn test_strategy_id_git_worktree() {
        let id = StrategyId::new(StrategyId::GIT_WORKTREE);
        assert_eq!(id.0, "git_worktree");
    }

    // ── ProjectCopyRefreshResult ───────────────────────────────────

    #[test]
    fn test_refresh_result_empty() {
        let result = ProjectCopyRefreshResult {
            updated: vec![],
            removed: vec![],
        };
        let json = serde_json::to_string(&result).expect("serialize");
        let parsed: ProjectCopyRefreshResult = serde_json::from_str(&json).expect("deserialize");
        assert!(parsed.updated.is_empty());
        assert!(parsed.removed.is_empty());
    }

    // ── ProjectCopyListEntry ───────────────────────────────────────

    #[test]
    fn test_copy_list_entry_root() {
        let entry = ProjectCopyListEntry::Root {
            directory: "/root".into(),
        };
        let json = serde_json::to_string(&entry).expect("serialize");
        assert!(json.contains(r#""type":"root""#));
        let parsed: ProjectCopyListEntry = serde_json::from_str(&json).expect("deserialize");
        match parsed {
            ProjectCopyListEntry::Root { directory } => assert_eq!(directory, "/root"),
            _ => panic!("expected Root"),
        }
    }

    #[test]
    fn test_copy_list_entry_copy() {
        let entry = ProjectCopyListEntry::Copy {
            directory: "/copy".into(),
        };
        let json = serde_json::to_string(&entry).expect("serialize");
        assert!(json.contains(r#""type":"copy""#));
        let parsed: ProjectCopyListEntry = serde_json::from_str(&json).expect("deserialize");
        match parsed {
            ProjectCopyListEntry::Copy { directory } => assert_eq!(directory, "/copy"),
            _ => panic!("expected Copy"),
        }
    }

    // ── ProjectRow / ProjectDirectoryRow ───────────────────────────

    #[test]
    fn test_project_row_serde() {
        let row = ProjectRow {
            id: ProjectId::new("p1"),
            worktree: "/home/proj".into(),
            vcs: Some("git".into()),
            name: Some("my-project".into()),
            icon_url: None,
            icon_url_override: None,
            icon_color: None,
            time_created: 1000,
            time_updated: 2000,
            time_deleted: None,
            time_initialized: Some(1500),
            sandboxes: vec![],
            commands: Some(ProjectCommandsConfig {
                start: Some("npm start".into()),
            }),
        };
        let json = serde_json::to_string(&row).expect("serialize");
        let parsed: ProjectRow = serde_json::from_str(&json).expect("deserialize");
        assert_eq!(parsed.name.as_deref(), Some("my-project"));
        assert_eq!(
            parsed.commands.as_ref().and_then(|c| c.start.as_deref()),
            Some("npm start")
        );
    }

    #[test]
    fn test_project_directory_row_serde() {
        let row = ProjectDirectoryRow {
            project_id: ProjectId::new("p2"),
            directory: "/src".into(),
            r#type: Some(ProjectDirectoryType::Main),
            strategy: None,
            time_created: 3000,
        };
        let json = serde_json::to_string(&row).expect("serialize");
        let parsed: ProjectDirectoryRow = serde_json::from_str(&json).expect("deserialize");
        assert_eq!(parsed.r#type, Some(ProjectDirectoryType::Main));
        assert_eq!(parsed.directory, "/src");
    }

    // ── ProjectCopyError ───────────────────────────────────────────

    #[test]
    fn test_copy_error_display() {
        let err = ProjectCopyError::SourceDirectoryNotFound {
            directory: "/missing".into(),
        };
        assert_eq!(err.to_string(), "source directory not found: /missing");
        let err = ProjectCopyError::StrategyUnavailable {
            strategy: StrategyId::new("unknown"),
        };
        assert!(err.to_string().contains("unknown"));
    }

    // ── ProjectService tests ─────────────────────────────────────────

    fn setup_project_fs() -> (tempfile::TempDir, PathBuf) {
        let dir = tempfile::tempdir().expect("create temp dir");
        let root = dir.path().to_path_buf();

        // Create a project structure
        let project_dir = root.join("my-project");
        std::fs::create_dir_all(&project_dir).unwrap();
        std::fs::write(project_dir.join("README.md"), "# My Project\n").unwrap();
        std::fs::write(
            project_dir.join("Cargo.toml"),
            "[package]\nname = \"my-project\"\n",
        )
        .unwrap();

        std::fs::create_dir_all(project_dir.join("src")).unwrap();
        std::fs::write(project_dir.join("src/main.rs"), "fn main() {}\n").unwrap();

        // Create a nested project
        let nested = root.join("my-project").join("sub-lib");
        std::fs::create_dir_all(&nested).unwrap();
        std::fs::write(nested.join("lib.rs"), "pub fn x() {}\n").unwrap();

        (dir, root)
    }

    #[test]
    fn test_detect_project_with_git() {
        let (_dir, root) = setup_project_fs();
        let project_dir = root.join("my-project");

        // Init git
        std::process::Command::new("git")
            .args(["init", "--quiet"])
            .current_dir(&project_dir)
            .output()
            .expect("git init");

        let svc = ProjectService::new(root.clone());
        let detection = svc.detect(&project_dir).expect("detect project");
        assert!(detection.has_git);
        assert!(detection.git_store.is_some());
        assert_eq!(detection.vcs.as_deref(), Some("git"));
        assert_eq!(detection.directory, project_dir.display().to_string());
    }

    #[test]
    fn test_detect_project_from_subdirectory() {
        let (_dir, root) = setup_project_fs();
        let project_dir = root.join("my-project");

        // Init git in parent
        std::process::Command::new("git")
            .args(["init", "--quiet"])
            .current_dir(&project_dir)
            .output()
            .expect("git init");

        // Detect from a subdirectory
        let sub_dir = project_dir.join("src");
        let svc = ProjectService::new(root.clone());
        let detection = svc.detect(&sub_dir).expect("detect from subdir");
        assert_eq!(detection.directory, project_dir.display().to_string());
    }

    #[test]
    fn test_detect_no_project() {
        let dir = tempfile::tempdir().expect("create temp dir");
        let root = dir.path();

        // Create empty dir with no .git
        let empty_dir = root.join("empty");
        std::fs::create_dir_all(&empty_dir).unwrap();

        let svc = ProjectService::new(root.to_path_buf());
        let result = svc.detect(&empty_dir);
        assert!(matches!(
            result,
            Err(ProjectServiceError::NoProjectFound(_))
        ));
    }

    #[test]
    fn test_create_project() {
        let dir = tempfile::tempdir().expect("create temp dir");
        let root = dir.path();
        let project_dir = root.join("new-project");

        let svc = ProjectService::new(root.to_path_buf());
        let detection = svc
            .create(&project_dir, Some("new-project"))
            .expect("create project");

        assert!(project_dir.exists());
        assert!(project_dir.join(".git").exists());
        assert!(project_dir.join(".blazecode").join("project.json").exists());
        assert_eq!(detection.directory, project_dir.display().to_string());
        assert!(detection.has_git);
        assert!(detection.has_blazecode_config);
    }

    #[test]
    fn test_create_project_already_exists() {
        let dir = tempfile::tempdir().expect("create temp dir");
        let root = dir.path();
        let project_dir = root.join("exists-project");

        // Init git to make it look like a project
        std::fs::create_dir_all(&project_dir).unwrap();
        std::process::Command::new("git")
            .args(["init", "--quiet"])
            .current_dir(&project_dir)
            .output()
            .expect("git init");

        let svc = ProjectService::new(root.to_path_buf());
        let result = svc.create(&project_dir, None);
        assert!(matches!(result, Err(ProjectServiceError::AlreadyExists(_))));
    }

    #[test]
    fn test_list_projects() {
        let dir = tempfile::tempdir().expect("create temp dir");
        let root = dir.path();

        // Create multiple projects
        for name in &["proj-a", "proj-b", "not-a-project"] {
            let project_dir = root.join(name);
            std::fs::create_dir_all(&project_dir).unwrap();
        }

        // Init git in proj-a and proj-b
        for name in &["proj-a", "proj-b"] {
            std::process::Command::new("git")
                .args(["init", "--quiet"])
                .current_dir(root.join(name))
                .output()
                .expect("git init");
        }

        let svc = ProjectService::new(root.to_path_buf());
        let projects = svc.list(3).expect("list projects");
        assert_eq!(projects.len(), 2);

        let names: Vec<&str> = projects.iter().filter_map(|p| p.name.as_deref()).collect();
        assert!(names.contains(&"proj-a"));
        assert!(names.contains(&"proj-b"));
    }

    #[test]
    fn test_list_projects_empty() {
        let dir = tempfile::tempdir().expect("create temp dir");
        let root = dir.path();

        let svc = ProjectService::new(root.to_path_buf());
        let projects = svc.list(3).expect("list projects");
        assert!(projects.is_empty());
    }

    #[test]
    fn test_validate_project_valid() {
        let dir = tempfile::tempdir().expect("create temp dir");
        let root = dir.path();
        let project_dir = root.join("valid-project");

        std::fs::create_dir_all(&project_dir).unwrap();
        std::process::Command::new("git")
            .args(["init", "--quiet"])
            .current_dir(&project_dir)
            .output()
            .expect("git init");

        // Create valid blazecode config
        std::fs::create_dir_all(project_dir.join(".blazecode")).unwrap();
        std::fs::write(
            project_dir.join(".blazecode").join("project.json"),
            r#"{"name": "valid", "version": "0.1.0"}"#,
        )
        .unwrap();

        let svc = ProjectService::new(root.to_path_buf());
        let issues = svc.validate(&project_dir).expect("validate");
        assert!(issues.is_empty(), "expected no issues, got: {issues:?}");
    }

    #[test]
    fn test_validate_project_missing_dir() {
        let svc = ProjectService::new(PathBuf::from("/tmp"));
        let result = svc.validate(Path::new("/nonexistent/dir"));
        assert!(matches!(
            result,
            Err(ProjectServiceError::DirectoryNotFound(_))
        ));
    }

    #[test]
    fn test_validate_project_invalid_config() {
        let dir = tempfile::tempdir().expect("create temp dir");
        let root = dir.path();
        let project_dir = root.join("bad-config");

        std::fs::create_dir_all(&project_dir).unwrap();
        std::fs::create_dir_all(project_dir.join(".blazecode")).unwrap();
        // Write invalid JSON
        std::fs::write(
            project_dir.join(".blazecode").join("project.json"),
            "not valid json {{{",
        )
        .unwrap();

        let svc = ProjectService::new(root.to_path_buf());
        let issues = svc.validate(&project_dir).expect("validate");
        assert!(!issues.is_empty());
        assert!(issues.iter().any(|i| i.contains("invalid JSON")));
    }

    #[test]
    fn test_project_detection_with_blazecode_config() {
        let dir = tempfile::tempdir().expect("create temp dir");
        let root = dir.path();
        let project_dir = root.join("config-only-project");

        std::fs::create_dir_all(&project_dir).unwrap();
        // Create .blazecode marker but no .git
        std::fs::create_dir_all(project_dir.join(".blazecode")).unwrap();

        let svc = ProjectService::new(root.to_path_buf());
        let detection = svc.detect(&project_dir).expect("detect project");
        assert!(detection.has_blazecode_config);
        assert!(!detection.has_git);
        assert!(detection.vcs.is_none());
    }

    #[test]
    fn test_compute_project_id_stable() {
        let dir = tempfile::tempdir().expect("create temp dir");
        let root = dir.path();
        let svc = ProjectService::new(root.to_path_buf());

        let project_dir = root.join("stable-id");
        std::fs::create_dir_all(&project_dir).unwrap();

        let id1 = svc.compute_project_id(&project_dir);
        let id2 = svc.compute_project_id(&project_dir);
        assert_eq!(id1, id2, "project ID should be stable");

        let other = root.join("other-dir");
        std::fs::create_dir_all(&other).unwrap();
        let id3 = svc.compute_project_id(&other);
        assert_ne!(id1, id3, "different dirs should have different IDs");
    }

    #[test]
    fn test_list_projects_respects_depth() {
        let dir = tempfile::tempdir().expect("create temp dir");
        let root = dir.path();

        // Create a project at depth 2
        let deep = root.join("level1").join("level2");
        std::fs::create_dir_all(&deep).unwrap();
        std::process::Command::new("git")
            .args(["init", "--quiet"])
            .current_dir(&deep)
            .output()
            .expect("git init");

        // With depth 1, should NOT find it
        let svc = ProjectService::new(root.to_path_buf());
        let shallow = svc.list(1).expect("list shallow");
        assert!(
            shallow.is_empty(),
            "should not find project at depth 2 with max_depth=1"
        );

        // With depth 3, should find it
        let deep_result = svc.list(3).expect("list deep");
        assert_eq!(deep_result.len(), 1);
    }

    #[test]
    fn test_detect_project_stops_at_root() {
        let dir = tempfile::tempdir().expect("create temp dir");
        let root = dir.path();

        // Create an empty subdir with no git parents — detection should fail
        let empty = root.join("no-git-here");
        std::fs::create_dir_all(&empty).unwrap();

        let svc = ProjectService::new(root.to_path_buf());
        let result = svc.detect(&empty);
        assert!(result.is_err());
    }
}
