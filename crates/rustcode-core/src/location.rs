//! Location types — Ref, Info, mutation resolution, permission resources.
//!
//! Ported from:
//! - `packages/core/src/location.ts` — Ref, Info, Interface, response helper
//! - `packages/core/src/location-layer.ts` — LocationServiceMap (type-only stub)
//! - `packages/core/src/location-mutation.ts` — Kind, ResolveInput, PathError, Target
//!
//! OpenCode commit: 5d0f86606ac30690f79f0a6a9f41a1f49fe95d0b

use serde::{Deserialize, Serialize};

use crate::project::{ProjectId, ProjectVcs};
use crate::workspace::WorkspaceId;

// ══════════════════════════════════════════════════════════════════════════════
// Location Ref and Info
// ══════════════════════════════════════════════════════════════════════════════

/// A location reference — the minimum needed to resolve a location.
///
/// # Source
/// `packages/core/src/location.ts` lines 8–11.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct LocationRef {
    /// Absolute path to the working directory.
    pub directory: String,

    /// Optional workspace ID — omitted when not inside a known workspace.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub workspace_id: Option<WorkspaceId>,
}

/// Resolved location information — directory, workspace, and project.
///
/// # Source
/// `packages/core/src/location.ts` lines 13–20.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct LocationInfo {
    /// Absolute path to the working directory.
    pub directory: String,

    /// Optional workspace ID.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub workspace_id: Option<WorkspaceId>,

    /// The resolved project that owns this location.
    pub project: LocationProjectRef,
}

/// Nested project reference within a location.
///
/// # Source
/// `packages/core/src/location.ts` lines 16–18 — the `project` struct inside Info.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct LocationProjectRef {
    pub id: ProjectId,
    pub directory: String,
}

/// The full resolved location including optional VCS information.
///
/// # Source
/// `packages/core/src/location.ts` lines 22–24 — `Interface extends Info`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct LocationFull {
    pub directory: String,

    #[serde(skip_serializing_if = "Option::is_none")]
    pub workspace_id: Option<WorkspaceId>,

    pub project: LocationProjectRef,

    /// Optional VCS metadata (e.g., git store location).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub vcs: Option<ProjectVcs>,
}

impl LocationRef {
    /// Create a new location reference.
    #[must_use]
    pub fn new(directory: impl Into<String>) -> Self {
        Self {
            directory: directory.into(),
            workspace_id: None,
        }
    }

    /// Create a new location reference with a workspace.
    #[must_use]
    pub fn with_workspace(directory: impl Into<String>, workspace_id: WorkspaceId) -> Self {
        Self {
            directory: directory.into(),
            workspace_id: Some(workspace_id),
        }
    }
}

/// Helper to build a response envelope with location and data payload.
///
/// # Source
/// `packages/core/src/location.ts` lines 26–28 — `response(data)`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct LocationResponse<D> {
    pub location: LocationInfo,
    pub data: D,
}

impl<D> LocationResponse<D> {
    /// Wrap data with a location info envelope.
    #[must_use]
    pub fn new(location: LocationInfo, data: D) -> Self {
        Self { location, data }
    }
}

// ══════════════════════════════════════════════════════════════════════════════
// Location Mutation
// ══════════════════════════════════════════════════════════════════════════════

/// The kind of filesystem entry targeted by a mutation path.
///
/// # Source
/// `packages/core/src/location-mutation.ts` line 8.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum MutationKind {
    File,
    Directory,
}

/// Input for resolving a mutation target path against the current location.
///
/// # Source
/// `packages/core/src/location-mutation.ts` lines 16–20.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct MutationResolveInput {
    /// The path to resolve — relative (within location) or absolute (for external).
    pub path: String,

    /// Selects the external approval boundary; does not validate the target type.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub kind: Option<MutationKind>,
}

/// Error when resolving a mutation path against a location.
///
/// # Source
/// `packages/core/src/location-mutation.ts` lines 23–26.
#[derive(Debug, Clone, thiserror::Error, Serialize, Deserialize)]
pub enum MutationPathError {
    /// A relative path escaped outside the location.
    #[error("relative path escaped location: {path}")]
    RelativeEscape { path: String },

    /// A path inside the location resolved to a symlinked target outside it.
    #[error("path escaped location through symlink: {path}")]
    LocationEscape { path: String },

    /// An ancestor of the target path is not a directory.
    #[error("ancestor is not a directory: {path}")]
    NonDirectoryAncestor { path: String },
}

impl MutationPathError {
    /// Create a relative escape error.
    #[must_use]
    pub fn relative_escape(path: impl Into<String>) -> Self {
        Self::RelativeEscape { path: path.into() }
    }

    /// Create a location escape error.
    #[must_use]
    pub fn location_escape(path: impl Into<String>) -> Self {
        Self::LocationEscape { path: path.into() }
    }

    /// Create a non-directory ancestor error.
    #[must_use]
    pub fn non_directory_ancestor(path: impl Into<String>) -> Self {
        Self::NonDirectoryAncestor {
            path: path.into(),
        }
    }
}

/// Authorization details required for external directory access.
///
/// # Source
/// `packages/core/src/location-mutation.ts` lines 28–35.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ExternalDirectoryAuthorization {
    /// Always `"external_directory"`.
    pub action: String,

    /// The canonical existing directory used as the external approval boundary.
    pub directory: String,

    /// The `external_directory` permission resource pattern.
    pub resource: String,

    /// The save pattern for persisted permission.
    pub save: String,
}

/// The resolved target of a mutation path.
///
/// # Source
/// `packages/core/src/location-mutation.ts` lines 43–49.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct MutationTarget {
    /// The canonical existing path, or the path beneath a canonical directory.
    pub canonical: String,

    /// Permission resource — location-relative for internal paths, canonical for external.
    pub resource: String,

    /// External directory authorization, only present when the target is outside the location.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub external_directory: Option<ExternalDirectoryAuthorization>,
}

// ══════════════════════════════════════════════════════════════════════════════
// Location Service Map (type-only stub)
// ══════════════════════════════════════════════════════════════════════════════

/// Key for the location service map — identifies a specific location.
///
/// # Source
/// `packages/core/src/location-layer.ts` — the `LayerMap.Service` key type uses
/// `Location.Ref` as its lookup key, with a 60-minute idle TTL.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct LocationServiceKey {
    pub directory: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub workspace_id: Option<WorkspaceId>,
}

impl From<LocationRef> for LocationServiceKey {
    fn from(r: LocationRef) -> Self {
        Self {
            directory: r.directory,
            workspace_id: r.workspace_id,
        }
    }
}

// ══════════════════════════════════════════════════════════════════════════════
// Tests
// ══════════════════════════════════════════════════════════════════════════════

#[cfg(test)]
mod tests {
    use super::*;

    // ── LocationRef ────────────────────────────────────────────────

    #[test]
    fn test_location_ref_new() {
        let r = LocationRef::new("/home/user/project");
        assert_eq!(r.directory, "/home/user/project");
        assert!(r.workspace_id.is_none());
    }

    #[test]
    fn test_location_ref_with_workspace() {
        let ws = WorkspaceId::ascending("wrk_test1").expect("valid");
        let r = LocationRef::with_workspace("/home/user/project", ws);
        assert_eq!(r.directory, "/home/user/project");
        assert!(r.workspace_id.is_some());
        assert_eq!(r.workspace_id.unwrap().as_str(), "wrk_test1");
    }

    #[test]
    fn test_location_ref_serde() {
        let r = LocationRef::new("/tmp/test");
        let json = serde_json::to_string(&r).expect("serialize");
        let parsed: LocationRef = serde_json::from_str(&json).expect("deserialize");
        assert_eq!(parsed.directory, "/tmp/test");
    }

    #[test]
    fn test_location_ref_serde_with_workspace() {
        let ws = WorkspaceId::ascending("wrk_loc1").expect("valid");
        let r = LocationRef::with_workspace("/home/proj", ws);
        let json = serde_json::to_string(&r).expect("serialize");
        let parsed: LocationRef = serde_json::from_str(&json).expect("deserialize");
        assert!(parsed.workspace_id.is_some());
    }

    // ── LocationInfo / LocationProjectRef ──────────────────────────

    #[test]
    fn test_location_info_serde() {
        let info = LocationInfo {
            directory: "/app".into(),
            workspace_id: None,
            project: LocationProjectRef {
                id: ProjectId::new("proj_1"),
                directory: "/app".into(),
            },
        };
        let json = serde_json::to_string(&info).expect("serialize");
        let parsed: LocationInfo = serde_json::from_str(&json).expect("deserialize");
        assert_eq!(parsed.project.id.0, "proj_1");
        assert_eq!(parsed.directory, "/app");
    }

    // ── LocationFull ───────────────────────────────────────────────

    #[test]
    fn test_location_full_with_vcs() {
        let full = LocationFull {
            directory: "/repo".into(),
            workspace_id: None,
            project: LocationProjectRef {
                id: ProjectId::new("p_git"),
                directory: "/repo".into(),
            },
            vcs: Some(ProjectVcs::git("/repo/.git")),
        };
        let json = serde_json::to_string(&full).expect("serialize");
        let parsed: LocationFull = serde_json::from_str(&json).expect("deserialize");
        assert!(parsed.vcs.is_some());
    }

    #[test]
    fn test_location_full_without_vcs() {
        let full = LocationFull {
            directory: "/tmp/not-repo".into(),
            workspace_id: None,
            project: LocationProjectRef {
                id: ProjectId::global(),
                directory: "/".into(),
            },
            vcs: None,
        };
        let json = serde_json::to_string(&full).expect("serialize");
        let parsed: LocationFull = serde_json::from_str(&json).expect("deserialize");
        assert!(parsed.vcs.is_none());
    }

    // ── LocationResponse ───────────────────────────────────────────

    #[test]
    fn test_location_response() {
        let info = LocationInfo {
            directory: "/app".into(),
            workspace_id: None,
            project: LocationProjectRef {
                id: ProjectId::new("proj_1"),
                directory: "/app".into(),
            },
        };
        let resp = LocationResponse::new(info, "payload");
        let json = serde_json::to_string(&resp).expect("serialize");
        assert!(json.contains("payload"));
        assert!(json.contains("proj_1"));
    }

    // ── MutationKind ───────────────────────────────────────────────

    #[test]
    fn test_mutation_kind_serde() {
        assert_eq!(
            serde_json::to_string(&MutationKind::File).expect("serialize"),
            r#""file""#
        );
        assert_eq!(
            serde_json::to_string(&MutationKind::Directory).expect("serialize"),
            r#""directory""#
        );
    }

    // ── MutationResolveInput ───────────────────────────────────────

    #[test]
    fn test_mutation_resolve_input_serde() {
        let input = MutationResolveInput {
            path: "src/main.rs".into(),
            kind: Some(MutationKind::File),
        };
        let json = serde_json::to_string(&input).expect("serialize");
        let parsed: MutationResolveInput = serde_json::from_str(&json).expect("deserialize");
        assert_eq!(parsed.path, "src/main.rs");
        assert_eq!(parsed.kind, Some(MutationKind::File));
    }

    // ── MutationPathError ──────────────────────────────────────────

    #[test]
    fn test_mutation_path_error_display() {
        let err = MutationPathError::relative_escape("../outside");
        assert!(err.to_string().contains("../outside"));
        assert!(err.to_string().contains("escaped location"));

        let err = MutationPathError::location_escape("/etc/passwd");
        assert!(err.to_string().contains("/etc/passwd"));

        let err = MutationPathError::non_directory_ancestor("/foo/bar/..");
        assert!(err.to_string().contains("/foo/bar/.."));
    }

    // ── ExternalDirectoryAuthorization ─────────────────────────────

    #[test]
    fn test_external_directory_authorization_serde() {
        let auth = ExternalDirectoryAuthorization {
            action: "external_directory".into(),
            directory: "/mnt/data".into(),
            resource: "/mnt/data/*".into(),
            save: "/mnt/data/*".into(),
        };
        let json = serde_json::to_string(&auth).expect("serialize");
        let parsed: ExternalDirectoryAuthorization =
            serde_json::from_str(&json).expect("deserialize");
        assert_eq!(parsed.directory, "/mnt/data");
        assert_eq!(parsed.resource, "/mnt/data/*");
    }

    // ── MutationTarget ─────────────────────────────────────────────

    #[test]
    fn test_mutation_target_internal() {
        let target = MutationTarget {
            canonical: "/app/src/main.rs".into(),
            resource: "src/main.rs".into(),
            external_directory: None,
        };
        let json = serde_json::to_string(&target).expect("serialize");
        assert!(!json.contains("external_directory"));
    }

    #[test]
    fn test_mutation_target_external() {
        let target = MutationTarget {
            canonical: "/mnt/data/output.txt".into(),
            resource: "/mnt/data/output.txt".into(),
            external_directory: Some(ExternalDirectoryAuthorization {
                action: "external_directory".into(),
                directory: "/mnt/data".into(),
                resource: "/mnt/data/*".into(),
                save: "/mnt/data/*".into(),
            }),
        };
        let json = serde_json::to_string(&target).expect("serialize");
        assert!(json.contains("external_directory"));
        let parsed: MutationTarget = serde_json::from_str(&json).expect("deserialize");
        assert!(parsed.external_directory.is_some());
    }

    // ── LocationServiceKey ─────────────────────────────────────────

    #[test]
    fn test_location_service_key_from_ref() {
        let r = LocationRef::new("/home/proj");
        let key: LocationServiceKey = r.into();
        assert_eq!(key.directory, "/home/proj");
        assert!(key.workspace_id.is_none());
    }

    #[test]
    fn test_location_service_key_with_workspace() {
        let ws = WorkspaceId::ascending("wrk_key1").expect("valid");
        let r = LocationRef::with_workspace("/ws/proj", ws);
        let key: LocationServiceKey = r.into();
        assert_eq!(
            key.workspace_id.unwrap().as_str(),
            "wrk_key1"
        );
    }

    #[test]
    fn test_location_service_key_serde() {
        let ws = WorkspaceId::ascending("wrk_skey").expect("valid");
        let key = LocationServiceKey {
            directory: "/data".into(),
            workspace_id: Some(ws),
        };
        let json = serde_json::to_string(&key).expect("serialize");
        let parsed: LocationServiceKey = serde_json::from_str(&json).expect("deserialize");
        assert_eq!(parsed.directory, "/data");
    }
}
