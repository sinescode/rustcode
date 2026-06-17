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
//! OpenCode commit: 5d0f86606ac30690f79f0a6a9f41a1f49fe95d0b

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
        assert_eq!(
            err.to_string(),
            "source directory not found: /missing"
        );
        let err = ProjectCopyError::StrategyUnavailable {
            strategy: StrategyId::new("unknown"),
        };
        assert!(err.to_string().contains("unknown"));
    }
}
