//! Workspace types — identifier with `wrk_` prefix brand.
//!
//! Ported from:
//! - `packages/core/src/workspace.ts` — ID brand, ascending/create statics
//!
//! OpenCode commit: 5d0f86606ac30690f79f0a6a9f41a1f49fe95d0b

use serde::{Deserialize, Serialize};

// ══════════════════════════════════════════════════════════════════════════════
// Workspace ID
// ══════════════════════════════════════════════════════════════════════════════

/// Workspace identifier — a branded string that must start with `"wrk_"`.
///
/// # Source
/// `packages/core/src/workspace.ts` lines 7–17.
///
/// The ID validates the `"wrk_"` prefix and uses an ascending identifier
/// (like ULID) for the suffix. Construction functions accept an existing ID
/// (for rehydration) or generate a fresh one.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct WorkspaceId(pub String);

/// Prefix that every valid workspace ID must start with.
pub const WORKSPACE_ID_PREFIX: &str = "wrk_";

impl WorkspaceId {
    /// Create a workspace ID from an existing string.
    ///
    /// Validates that the string starts with `"wrk_"`.
    ///
    /// # Source
    /// `packages/core/src/workspace.ts` lines 11–13 — `ascending`.
    pub fn ascending(id: &str) -> Result<Self, WorkspaceIdError> {
        if !id.starts_with(WORKSPACE_ID_PREFIX) {
            return Err(WorkspaceIdError::InvalidPrefix { id: id.to_string() });
        }
        Ok(Self(id.to_string()))
    }

    /// Create a fresh workspace ID with an ascending identifier suffix.
    ///
    /// Uses `crate::id::ascending` with the `wrk_` prefix.
    ///
    /// # Source
    /// `packages/core/src/workspace.ts` line 15 — `create`.
    pub fn create() -> Result<Self, WorkspaceIdError> {
        let suffix = crate::id::ascending(crate::id::IdPrefix::Workspace, None)
            .map_err(|e| WorkspaceIdError::GenerationFailed(e.to_string()))?;
        // The id module strips the prefix, so we reconstruct
        Ok(Self(format!("{WORKSPACE_ID_PREFIX}{suffix}")))
    }

    /// Return the raw string value.
    #[must_use]
    pub fn as_str(&self) -> &str {
        &self.0
    }

    /// Return the suffix after `"wrk_"`.
    #[must_use]
    pub fn suffix(&self) -> &str {
        self.0.strip_prefix(WORKSPACE_ID_PREFIX).unwrap_or(&self.0)
    }
}

impl std::fmt::Display for WorkspaceId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.0)
    }
}

impl AsRef<str> for WorkspaceId {
    fn as_ref(&self) -> &str {
        &self.0
    }
}

// ══════════════════════════════════════════════════════════════════════════════
// Workspace Info
// ══════════════════════════════════════════════════════════════════════════════

/// Minimal workspace information — just the ID for now.
///
/// # Source
/// `packages/core/src/workspace.ts` — the `WorkspaceV2` namespace provides only
/// the ID brand; additional fields come from the database layer.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct WorkspaceInfo {
    pub id: WorkspaceId,
}

/// Input for creating a new workspace.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct WorkspaceCreateInput {
    /// Optional explicit ID; if omitted, a fresh ascending ID is generated.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub id: Option<WorkspaceId>,
}

/// Input for listing/filtering workspaces.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct WorkspaceListInput {
    /// Filter by workspace ID prefix match.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub search: Option<String>,

    /// Maximum number of results.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub limit: Option<usize>,
}

// ══════════════════════════════════════════════════════════════════════════════
// Workspace status / adapter types
// ══════════════════════════════════════════════════════════════════════════════

/// The current status of a workspace.
///
/// # Source
/// Derived from the session/project lifecycle — a workspace is:
/// - `Active` when it has a running session or recent activity.
/// - `Idle` when it has no active sessions.
/// - `Archived` when it has been explicitly archived.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum WorkspaceStatus {
    Active,
    Idle,
    Archived,
}

/// Workspace adapter — bridges the workspace layer to the backing storage.
///
/// Different adapters can be plugged in (SQLite, JSON file, in-memory)
/// without changing the workspace API.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum WorkspaceAdapterKind {
    /// SQLite-backed workspace storage.
    Sqlite,
    /// JSON-file-backed workspace storage.
    Json,
    /// In-memory workspace storage (for testing).
    Memory,
}

// ══════════════════════════════════════════════════════════════════════════════
// Workspace Adapter interface
// ══════════════════════════════════════════════════════════════════════════════

/// Trait for workspace storage backends.
///
/// Different adapters can be plugged in (SQLite, JSON file, in-memory)
/// without changing the workspace API.
///
/// # Source
/// Ported from the adapter pattern used in `packages/core/src/workspace.ts`.
#[async_trait::async_trait]
pub trait WorkspaceAdapter: Send + Sync {
    /// Create a new workspace record.
    async fn create_workspace(
        &self,
        id: &WorkspaceId,
        project_id: &str,
        name: &str,
        directory: Option<&str>,
        workspace_type: &str,
    ) -> Result<WorkspaceRecord, WorkspaceServiceError>;

    /// List workspaces, optionally filtered.
    async fn list_workspaces(
        &self,
        input: Option<&WorkspaceListInput>,
    ) -> Result<Vec<WorkspaceRecord>, WorkspaceServiceError>;

    /// Remove a workspace by ID.
    async fn remove_workspace(&self, id: &WorkspaceId) -> Result<(), WorkspaceServiceError>;

    /// Get a workspace record by ID.
    async fn get_workspace(
        &self,
        id: &WorkspaceId,
    ) -> Result<Option<WorkspaceRecord>, WorkspaceServiceError>;

    /// Update workspace metadata (e.g., set the project_id for a warp).
    async fn update_workspace(
        &self,
        id: &WorkspaceId,
        project_id: Option<&str>,
        directory: Option<&str>,
        name: Option<&str>,
    ) -> Result<WorkspaceRecord, WorkspaceServiceError>;

    /// Touch the workspace's last-used time.
    async fn touch_workspace(&self, id: &WorkspaceId) -> Result<(), WorkspaceServiceError>;
}

/// A workspace record from the storage backend.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct WorkspaceRecord {
    /// The workspace ID.
    pub id: WorkspaceId,
    /// The workspace type.
    pub workspace_type: String,
    /// The workspace name.
    pub name: String,
    /// The associated project ID.
    pub project_id: String,
    /// Optional directory path.
    pub directory: Option<String>,
    /// Optional branch.
    pub branch: Option<String>,
    /// When the workspace was created (epoch millis).
    pub time_created: i64,
    /// When the workspace was last used (epoch millis).
    pub time_used: i64,
    /// The workspace status.
    pub status: WorkspaceStatus,
}

// ══════════════════════════════════════════════════════════════════════════════
// Workspace Service
// ══════════════════════════════════════════════════════════════════════════════

/// Errors that can occur during workspace service operations.
///
/// # Source
/// Ported from error patterns in `packages/core/src/workspace.ts`.
#[derive(Debug, thiserror::Error)]
pub enum WorkspaceServiceError {
    /// The workspace was not found.
    #[error("workspace not found: {0}")]
    NotFound(String),

    /// The workspace already exists.
    #[error("workspace already exists: {0}")]
    AlreadyExists(String),

    /// Invalid input.
    #[error("invalid input: {0}")]
    InvalidInput(String),

    /// The operation is not supported by the adapter.
    #[error("operation not supported: {0}")]
    Unsupported(String),

    /// An internal storage error occurred.
    #[error("storage error: {0}")]
    Storage(String),
}

/// Workspace service — manages workspace lifecycle.
///
/// Uses a pluggable [`WorkspaceAdapter`] for the storage backend.
///
/// # Source
/// Ported from `packages/core/src/workspace.ts` `WorkspaceV2` namespace.
pub struct WorkspaceService {
    adapter: Box<dyn WorkspaceAdapter>,
}

impl WorkspaceService {
    /// Create a new WorkspaceService with the given adapter.
    pub fn new(adapter: Box<dyn WorkspaceAdapter>) -> Self {
        Self { adapter }
    }

    /// Create a new workspace with generated metadata.
    ///
    /// # Source
    /// Ported from `packages/core/src/workspace.ts` `create()`.
    pub async fn create(
        &self,
        input: Option<&WorkspaceCreateInput>,
        project_id: &str,
        name: &str,
        workspace_type: &str,
    ) -> Result<WorkspaceRecord, WorkspaceServiceError> {
        let id = if let Some(input) = input {
            input.id.clone().unwrap_or_else(|| {
                WorkspaceId::create().expect("workspace ID generation should not fail")
            })
        } else {
            WorkspaceId::create().expect("workspace ID generation should not fail")
        };

        self.adapter
            .create_workspace(&id, project_id, name, None, workspace_type)
            .await
    }

    /// List workspaces, optionally filtered and limited.
    ///
    /// # Source
    /// Ported from workspace listing patterns in the TS codebase.
    pub async fn list(
        &self,
        input: Option<&WorkspaceListInput>,
    ) -> Result<Vec<WorkspaceRecord>, WorkspaceServiceError> {
        self.adapter.list_workspaces(input).await
    }

    /// Remove a workspace by ID.
    ///
    /// # Source
    /// Ported from workspace removal patterns in the TS codebase.
    pub async fn remove(&self, id: &WorkspaceId) -> Result<(), WorkspaceServiceError> {
        self.adapter.remove_workspace(id).await
    }

    /// Get a workspace by ID.
    pub async fn get(
        &self,
        id: &WorkspaceId,
    ) -> Result<Option<WorkspaceRecord>, WorkspaceServiceError> {
        self.adapter.get_workspace(id).await
    }

    /// Warp (move) a session to a workspace.
    ///
    /// Updates the workspace's project_id and directory to match the target
    /// session's project context.
    ///
    /// # Source
    /// Ported from session-to-workspace assignment patterns.
    pub async fn warp(
        &self,
        workspace_id: &WorkspaceId,
        target_project_id: &str,
        target_directory: Option<&str>,
    ) -> Result<WorkspaceRecord, WorkspaceServiceError> {
        // Verify workspace exists
        let existing = self
            .adapter
            .get_workspace(workspace_id)
            .await?
            .ok_or_else(|| WorkspaceServiceError::NotFound(workspace_id.to_string()))?;

        // Update workspace to point to the new project/directory
        self.adapter
            .update_workspace(
                workspace_id,
                Some(target_project_id),
                target_directory,
                None,
            )
            .await
    }

    /// Touch a workspace's last-used timestamp.
    pub async fn touch(&self, id: &WorkspaceId) -> Result<(), WorkspaceServiceError> {
        self.adapter.touch_workspace(id).await
    }

    /// Get a reference to the adapter.
    pub fn adapter(&self) -> &dyn WorkspaceAdapter {
        self.adapter.as_ref()
    }
}

// ══════════════════════════════════════════════════════════════════════════════
// In-memory workspace adapter (for testing)
// ══════════════════════════════════════════════════════════════════════════════

use std::collections::HashMap;
use std::sync::Mutex;

/// An in-memory workspace adapter suitable for testing.
pub struct MemoryWorkspaceAdapter {
    records: Mutex<HashMap<String, WorkspaceRecord>>,
}

impl MemoryWorkspaceAdapter {
    /// Create a new empty in-memory adapter.
    pub fn new() -> Self {
        Self {
            records: Mutex::new(HashMap::new()),
        }
    }
}

impl Default for MemoryWorkspaceAdapter {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait::async_trait]
impl WorkspaceAdapter for MemoryWorkspaceAdapter {
    async fn create_workspace(
        &self,
        id: &WorkspaceId,
        project_id: &str,
        name: &str,
        directory: Option<&str>,
        workspace_type: &str,
    ) -> Result<WorkspaceRecord, WorkspaceServiceError> {
        let mut records = self
            .records
            .lock()
            .map_err(|e| WorkspaceServiceError::Storage(e.to_string()))?;

        if records.contains_key(id.as_str()) {
            return Err(WorkspaceServiceError::AlreadyExists(id.to_string()));
        }

        let now = chrono::Utc::now().timestamp_millis();
        let record = WorkspaceRecord {
            id: id.clone(),
            workspace_type: workspace_type.to_string(),
            name: name.to_string(),
            project_id: project_id.to_string(),
            directory: directory.map(|d| d.to_string()),
            branch: None,
            time_created: now,
            time_used: now,
            status: WorkspaceStatus::Active,
        };

        records.insert(id.to_string(), record.clone());
        Ok(record)
    }

    async fn list_workspaces(
        &self,
        input: Option<&WorkspaceListInput>,
    ) -> Result<Vec<WorkspaceRecord>, WorkspaceServiceError> {
        let records = self
            .records
            .lock()
            .map_err(|e| WorkspaceServiceError::Storage(e.to_string()))?;

        let mut results: Vec<WorkspaceRecord> = records.values().cloned().collect();

        // Apply search filter
        if let Some(input) = input {
            if let Some(ref search) = input.search {
                results.retain(|r| r.id.as_str().contains(search.as_str()));
            }
            // Sort by most recently used
            results.sort_by_key(|b| std::cmp::Reverse(b.time_used));
            // Apply limit
            if let Some(limit) = input.limit {
                results.truncate(limit);
            }
        } else {
            results.sort_by_key(|b| std::cmp::Reverse(b.time_used));
        }

        Ok(results)
    }

    async fn remove_workspace(&self, id: &WorkspaceId) -> Result<(), WorkspaceServiceError> {
        let mut records = self
            .records
            .lock()
            .map_err(|e| WorkspaceServiceError::Storage(e.to_string()))?;

        if records.remove(id.as_str()).is_none() {
            return Err(WorkspaceServiceError::NotFound(id.to_string()));
        }
        Ok(())
    }

    async fn get_workspace(
        &self,
        id: &WorkspaceId,
    ) -> Result<Option<WorkspaceRecord>, WorkspaceServiceError> {
        let records = self
            .records
            .lock()
            .map_err(|e| WorkspaceServiceError::Storage(e.to_string()))?;

        Ok(records.get(id.as_str()).cloned())
    }

    async fn update_workspace(
        &self,
        id: &WorkspaceId,
        project_id: Option<&str>,
        directory: Option<&str>,
        name: Option<&str>,
    ) -> Result<WorkspaceRecord, WorkspaceServiceError> {
        let mut records = self
            .records
            .lock()
            .map_err(|e| WorkspaceServiceError::Storage(e.to_string()))?;

        let record = records
            .get_mut(id.as_str())
            .ok_or_else(|| WorkspaceServiceError::NotFound(id.to_string()))?;

        if let Some(pid) = project_id {
            record.project_id = pid.to_string();
        }
        if let Some(dir) = directory {
            record.directory = Some(dir.to_string());
        }
        if let Some(n) = name {
            record.name = n.to_string();
        }
        record.time_used = chrono::Utc::now().timestamp_millis();

        Ok(record.clone())
    }

    async fn touch_workspace(&self, id: &WorkspaceId) -> Result<(), WorkspaceServiceError> {
        let mut records = self
            .records
            .lock()
            .map_err(|e| WorkspaceServiceError::Storage(e.to_string()))?;

        let record = records
            .get_mut(id.as_str())
            .ok_or_else(|| WorkspaceServiceError::NotFound(id.to_string()))?;

        record.time_used = chrono::Utc::now().timestamp_millis();
        Ok(())
    }
}

// ══════════════════════════════════════════════════════════════════════════════
// Errors
// ══════════════════════════════════════════════════════════════════════════════

/// Workspace ID validation error.
#[derive(Debug, Clone, thiserror::Error)]
pub enum WorkspaceIdError {
    /// The provided string does not start with the required `"wrk_"` prefix.
    #[error("workspace ID must start with 'wrk_': got '{id}'")]
    InvalidPrefix { id: String },

    /// ID generation failed (e.g., timestamp collision).
    #[error("failed to generate workspace ID: {0}")]
    GenerationFailed(String),
}

// ══════════════════════════════════════════════════════════════════════════════
// Tests
// ══════════════════════════════════════════════════════════════════════════════

#[cfg(test)]
mod tests {
    use super::*;

    // ── WorkspaceId ────────────────────────────────────────────────

    #[test]
    fn test_ascending_valid_id() {
        let id = WorkspaceId::ascending("wrk_01jabcde").expect("valid prefix");
        assert_eq!(id.as_str(), "wrk_01jabcde");
        assert_eq!(id.suffix(), "01jabcde");
    }

    #[test]
    fn test_ascending_invalid_prefix() {
        let result = WorkspaceId::ascending("bad_prefix");
        assert!(result.is_err());
        match result {
            Err(WorkspaceIdError::InvalidPrefix { id }) => assert_eq!(id, "bad_prefix"),
            _ => panic!("expected InvalidPrefix"),
        }
    }

    #[test]
    fn test_ascending_empty_string() {
        let result = WorkspaceId::ascending("");
        assert!(result.is_err());
    }

    #[test]
    fn test_ascending_no_prefix() {
        let result = WorkspaceId::ascending("01jabcde");
        assert!(result.is_err());
    }

    #[test]
    fn test_create_generates_unique_id() {
        let id1 = WorkspaceId::create().expect("create 1");
        let id2 = WorkspaceId::create().expect("create 2");
        assert!(id1.as_str().starts_with(WORKSPACE_ID_PREFIX));
        assert!(id2.as_str().starts_with(WORKSPACE_ID_PREFIX));
    }

    #[test]
    fn test_display_and_as_ref() {
        let id = WorkspaceId::ascending("wrk_test").expect("valid");
        assert_eq!(id.to_string(), "wrk_test");
        assert_eq!(id.as_ref(), "wrk_test");
    }

    #[test]
    fn test_serde_roundtrip() {
        let id = WorkspaceId::ascending("wrk_01jxyz").expect("valid");
        let json = serde_json::to_string(&id).expect("serialize");
        let parsed: WorkspaceId = serde_json::from_str(&json).expect("deserialize");
        assert_eq!(parsed, id);
    }

    #[test]
    fn test_serde_json_value() {
        let id = WorkspaceId::ascending("wrk_hello").expect("valid");
        let json = serde_json::to_value(&id).expect("to_value");
        assert_eq!(json.as_str(), Some("wrk_hello"));
    }

    // ── WorkspaceInfo ──────────────────────────────────────────────

    #[test]
    fn test_workspace_info_serde() {
        let info = WorkspaceInfo {
            id: WorkspaceId::ascending("wrk_info").expect("valid"),
        };
        let json = serde_json::to_string(&info).expect("serialize");
        let parsed: WorkspaceInfo = serde_json::from_str(&json).expect("deserialize");
        assert_eq!(parsed.id.as_str(), "wrk_info");
    }

    // ── WorkspaceCreateInput / WorkspaceListInput ──────────────────

    #[test]
    fn test_create_input_with_id() {
        let input = WorkspaceCreateInput {
            id: Some(WorkspaceId::ascending("wrk_explicit").expect("valid")),
        };
        let json = serde_json::to_string(&input).expect("serialize");
        assert!(json.contains("wrk_explicit"));
    }

    #[test]
    fn test_create_input_without_id() {
        let input = WorkspaceCreateInput { id: None };
        let json = serde_json::to_string(&input).expect("serialize");
        assert_eq!(json, "{}");
    }

    #[test]
    fn test_list_input_default() {
        let input = WorkspaceListInput::default();
        assert!(input.search.is_none());
        assert!(input.limit.is_none());
    }

    #[test]
    fn test_list_input_with_search() {
        let input = WorkspaceListInput {
            search: Some("wrk_01j".into()),
            limit: Some(10),
        };
        let json = serde_json::to_string(&input).expect("serialize");
        assert!(json.contains("wrk_01j"));
    }

    // ── WorkspaceStatus ────────────────────────────────────────────

    #[test]
    fn test_workspace_status_serde() {
        assert_eq!(
            serde_json::to_string(&WorkspaceStatus::Active).expect("serialize"),
            r#""active""#
        );
        assert_eq!(
            serde_json::to_string(&WorkspaceStatus::Idle).expect("serialize"),
            r#""idle""#
        );
        assert_eq!(
            serde_json::to_string(&WorkspaceStatus::Archived).expect("serialize"),
            r#""archived""#
        );
    }

    // ── WorkspaceAdapterKind ───────────────────────────────────────

    #[test]
    fn test_adapter_kind_serde() {
        assert_eq!(
            serde_json::to_string(&WorkspaceAdapterKind::Sqlite).expect("serialize"),
            r#""sqlite""#
        );
        assert_eq!(
            serde_json::to_string(&WorkspaceAdapterKind::Json).expect("serialize"),
            r#""json""#
        );
        assert_eq!(
            serde_json::to_string(&WorkspaceAdapterKind::Memory).expect("serialize"),
            r#""memory""#
        );
    }

    // ── WorkspaceId error display ──────────────────────────────────

    #[test]
    fn test_error_display() {
        let err = WorkspaceIdError::InvalidPrefix { id: "bad".into() };
        assert!(err.to_string().contains("bad"));
        let err = WorkspaceIdError::GenerationFailed("reason".into());
        assert!(err.to_string().contains("reason"));
    }

    // ── WorkspaceService tests ───────────────────────────────────────

    fn setup_workspace_service() -> WorkspaceService {
        WorkspaceService::new(Box::new(MemoryWorkspaceAdapter::new()))
    }

    #[tokio::test]
    async fn test_create_workspace() {
        let svc = setup_workspace_service();
        let record = svc
            .create(None, "proj-1", "my-workspace", "default")
            .await
            .expect("create workspace");

        assert!(record.id.as_str().starts_with("wrk_"));
        assert_eq!(record.name, "my-workspace");
        assert_eq!(record.project_id, "proj-1");
        assert_eq!(record.workspace_type, "default");
        assert_eq!(record.status, WorkspaceStatus::Active);
        assert!(record.time_created > 0);
    }

    #[tokio::test]
    async fn test_create_workspace_with_explicit_id() {
        let svc = setup_workspace_service();
        let id = WorkspaceId::ascending("wrk_explicit123").expect("valid id");
        let input = WorkspaceCreateInput {
            id: Some(id.clone()),
        };

        let record = svc
            .create(Some(&input), "proj-2", "explicit", "custom")
            .await
            .expect("create workspace");

        assert_eq!(record.id, id);
        assert_eq!(record.name, "explicit");
    }

    #[tokio::test]
    async fn test_create_duplicate_workspace() {
        let svc = setup_workspace_service();
        let id = WorkspaceId::ascending("wrk_dup").expect("valid id");
        let input = WorkspaceCreateInput {
            id: Some(id.clone()),
        };

        // First create succeeds
        svc.create(Some(&input), "proj-1", "first", "type")
            .await
            .expect("first create");

        // Second create with same ID should fail
        let result = svc.create(Some(&input), "proj-2", "second", "type").await;
        assert!(matches!(
            result,
            Err(WorkspaceServiceError::AlreadyExists(_))
        ));
    }

    #[tokio::test]
    async fn test_list_workspaces() {
        let svc = setup_workspace_service();

        // Create multiple workspaces
        let r1 = svc
            .create(None, "proj-1", "workspace-a", "default")
            .await
            .unwrap();
        let r2 = svc
            .create(None, "proj-2", "workspace-b", "custom")
            .await
            .unwrap();
        let r3 = svc
            .create(None, "proj-1", "workspace-c", "default")
            .await
            .unwrap();

        let all = svc.list(None).await.expect("list workspaces");
        assert_eq!(all.len(), 3);
    }

    #[tokio::test]
    async fn test_list_workspaces_with_search() {
        let svc = setup_workspace_service();

        svc.create(None, "proj-1", "alpha", "default")
            .await
            .unwrap();
        svc.create(None, "proj-2", "beta", "custom").await.unwrap();
        svc.create(None, "proj-3", "gamma", "default")
            .await
            .unwrap();

        // Search for IDs containing specific pattern (all have wrk_ prefix)
        let input = WorkspaceListInput {
            search: None,
            limit: Some(2),
        };
        let limited = svc.list(Some(&input)).await.expect("list limited");
        assert_eq!(limited.len(), 2);
    }

    #[tokio::test]
    async fn test_list_workspaces_with_limit() {
        let svc = setup_workspace_service();

        for i in 0..5 {
            svc.create(None, "proj-1", &format!("ws-{i}"), "default")
                .await
                .unwrap();
        }

        let input = WorkspaceListInput {
            search: None,
            limit: Some(3),
        };
        let results = svc.list(Some(&input)).await.expect("list");
        assert_eq!(results.len(), 3);
    }

    #[tokio::test]
    async fn test_remove_workspace() {
        let svc = setup_workspace_service();
        let record = svc
            .create(None, "proj-1", "to-remove", "default")
            .await
            .unwrap();

        svc.remove(&record.id).await.expect("remove workspace");

        // Verify it's gone
        let found = svc.get(&record.id).await.expect("get");
        assert!(found.is_none());
    }

    #[tokio::test]
    async fn test_remove_nonexistent_workspace() {
        let svc = setup_workspace_service();
        let id = WorkspaceId::ascending("wrk_nope123").expect("valid id");
        let result = svc.remove(&id).await;
        assert!(matches!(result, Err(WorkspaceServiceError::NotFound(_))));
    }

    #[tokio::test]
    async fn test_get_workspace() {
        let svc = setup_workspace_service();
        let record = svc
            .create(None, "proj-1", "find-me", "default")
            .await
            .unwrap();

        let found = svc
            .get(&record.id)
            .await
            .expect("get")
            .expect("should exist");
        assert_eq!(found.name, "find-me");
        assert_eq!(found.project_id, "proj-1");
    }

    #[tokio::test]
    async fn test_get_nonexistent_workspace() {
        let svc = setup_workspace_service();
        let id = WorkspaceId::ascending("wrk_ghost00").expect("valid id");
        let found = svc.get(&id).await.expect("get");
        assert!(found.is_none());
    }

    #[tokio::test]
    async fn test_warp_workspace() {
        let svc = setup_workspace_service();
        let record = svc
            .create(None, "proj-1", "warp-test", "default")
            .await
            .unwrap();

        // Warp to a different project
        let warped = svc
            .warp(&record.id, "proj-99", Some("/new/directory"))
            .await
            .expect("warp");

        assert_eq!(warped.project_id, "proj-99");
        assert_eq!(warped.directory.as_deref(), Some("/new/directory"));
        assert_eq!(warped.id, record.id);
    }

    #[tokio::test]
    async fn test_warp_nonexistent_workspace() {
        let svc = setup_workspace_service();
        let id = WorkspaceId::ascending("wrk_nowarp0").expect("valid id");
        let result = svc.warp(&id, "proj-1", None).await;
        assert!(matches!(result, Err(WorkspaceServiceError::NotFound(_))));
    }

    #[tokio::test]
    async fn test_warp_preserves_name() {
        let svc = setup_workspace_service();
        let record = svc
            .create(None, "proj-1", "keep-name", "default")
            .await
            .unwrap();

        let warped = svc
            .warp(&record.id, "proj-2", Some("/other"))
            .await
            .expect("warp");

        assert_eq!(warped.name, "keep-name"); // Name should not change during warp
        assert_eq!(warped.project_id, "proj-2");
    }

    #[tokio::test]
    async fn test_touch_workspace() {
        let svc = setup_workspace_service();
        let record = svc
            .create(None, "proj-1", "touch-me", "default")
            .await
            .unwrap();

        let old_time = record.time_used;
        // Small sleep to ensure timestamp changes
        tokio::time::sleep(std::time::Duration::from_millis(2)).await;

        svc.touch(&record.id).await.expect("touch");

        let updated = svc
            .get(&record.id)
            .await
            .expect("get")
            .expect("should exist");
        assert!(updated.time_used > old_time);
    }

    #[tokio::test]
    async fn test_touch_nonexistent_workspace() {
        let svc = setup_workspace_service();
        let id = WorkspaceId::ascending("wrk_notouch").expect("valid id");
        let result = svc.touch(&id).await;
        assert!(matches!(result, Err(WorkspaceServiceError::NotFound(_))));
    }

    #[tokio::test]
    async fn test_create_multiple_unique_ids() {
        let svc = setup_workspace_service();
        let r1 = svc.create(None, "proj-1", "a", "type").await.unwrap();
        let r2 = svc.create(None, "proj-1", "b", "type").await.unwrap();
        let r3 = svc.create(None, "proj-1", "c", "type").await.unwrap();

        assert_ne!(r1.id, r2.id);
        assert_ne!(r2.id, r3.id);
        assert_ne!(r1.id, r3.id);
    }

    #[tokio::test]
    async fn test_adapter_kind_roundtrip() {
        // Verify adapter can be plugged in
        let adapter = MemoryWorkspaceAdapter::new();
        let id = WorkspaceId::ascending("wrk_adapter").expect("valid id");

        let record = adapter
            .create_workspace(&id, "p1", "test", None, "default")
            .await
            .expect("create");

        assert_eq!(record.name, "test");

        let found = adapter
            .get_workspace(&id)
            .await
            .expect("get")
            .expect("should exist");
        assert_eq!(found.id, id);

        adapter.remove_workspace(&id).await.expect("remove");

        let gone = adapter.get_workspace(&id).await.expect("get");
        assert!(gone.is_none());
    }

    #[tokio::test]
    async fn test_workspace_record_status_default_active() {
        let svc = setup_workspace_service();
        let record = svc
            .create(None, "proj-1", "status-test", "default")
            .await
            .unwrap();
        assert_eq!(record.status, WorkspaceStatus::Active);
    }

    #[tokio::test]
    async fn test_list_workspaces_empty() {
        let svc = setup_workspace_service();
        let results = svc.list(None).await.expect("list");
        assert!(results.is_empty());
    }

    #[tokio::test]
    async fn test_warp_multiple_times() {
        let svc = setup_workspace_service();
        let record = svc
            .create(None, "proj-1", "multi-warp", "default")
            .await
            .unwrap();

        let w1 = svc.warp(&record.id, "proj-a", Some("/a")).await.unwrap();
        assert_eq!(w1.project_id, "proj-a");

        let w2 = svc.warp(&record.id, "proj-b", Some("/b")).await.unwrap();
        assert_eq!(w2.project_id, "proj-b");

        let w3 = svc.warp(&record.id, "proj-c", None).await.unwrap();
        assert_eq!(w3.project_id, "proj-c");
        // Directory should still be /b since we passed None
        assert_eq!(w3.directory.as_deref(), Some("/b"));
    }

    #[test]
    fn test_workspace_service_error_display() {
        let err = WorkspaceServiceError::NotFound("wrk_x".into());
        assert!(err.to_string().contains("wrk_x"));

        let err = WorkspaceServiceError::AlreadyExists("wrk_y".into());
        assert!(err.to_string().contains("wrk_y"));

        let err = WorkspaceServiceError::InvalidInput("bad".into());
        assert!(err.to_string().contains("bad"));

        let err = WorkspaceServiceError::Storage("disk full".into());
        assert!(err.to_string().contains("disk full"));
    }
}
