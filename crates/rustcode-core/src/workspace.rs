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
            return Err(WorkspaceIdError::InvalidPrefix {
                id: id.to_string(),
            });
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
        self.0
            .strip_prefix(WORKSPACE_ID_PREFIX)
            .unwrap_or(&self.0)
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
        let err = WorkspaceIdError::InvalidPrefix {
            id: "bad".into(),
        };
        assert!(err.to_string().contains("bad"));
        let err = WorkspaceIdError::GenerationFailed("reason".into());
        assert!(err.to_string().contains("reason"));
    }
}
