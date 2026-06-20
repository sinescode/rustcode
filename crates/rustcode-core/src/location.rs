//! Location types — Ref, Info, mutation resolution, permission resources.
//!
//! Ported from:
//! - `packages/core/src/location.ts` — Ref, Info, Interface, response helper
//! - `packages/core/src/location-layer.ts` — LocationServiceMap (type-only stub)
//! - `packages/core/src/location-mutation.ts` — Kind, ResolveInput, PathError, Target
//!
//! OpenCode commit: 5d0f86606ac30690f79f0a6a9f41a1f49fe95d0b

use std::path::{Path, PathBuf};

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
        Self::NonDirectoryAncestor { path: path.into() }
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

/// Internal entry stored in [`LocationServiceMap`].
struct LocationServiceEntry {
    location: LocationFull,
    last_accessed: std::time::Instant,
}

/// A map of location service keys to resolved locations with idle TTL eviction.
///
/// Entries are evicted after being idle for longer than the configured TTL.
/// The default TTL is 60 minutes.
///
/// # Source
/// `packages/core/src/location-layer.ts` — `LocationServiceMap` (LayerMap with 60-min idle TTL).
pub struct LocationServiceMap {
    entries: std::collections::HashMap<LocationServiceKey, LocationServiceEntry>,
    ttl: std::time::Duration,
}

impl Default for LocationServiceMap {
    fn default() -> Self {
        Self {
            entries: std::collections::HashMap::new(),
            ttl: std::time::Duration::from_secs(60 * 60),
        }
    }
}

impl LocationServiceMap {
    /// Create a new map with the given TTL.
    #[must_use]
    pub fn new(ttl: std::time::Duration) -> Self {
        Self {
            entries: std::collections::HashMap::new(),
            ttl,
        }
    }

    /// Get an existing entry or resolve and insert a new one.
    ///
    /// If the key exists and has not expired, its `last_accessed` time is
    /// refreshed and the cached location is returned. Otherwise, the resolver
    /// is called. If it returns `Some`, the result is inserted and returned.
    /// If it returns `None`, `None` is returned.
    pub fn get_or_resolve(
        &mut self,
        key: LocationServiceKey,
        resolver: impl FnOnce() -> Option<LocationFull>,
    ) -> Option<LocationFull> {
        if let Some(entry) = self.entries.get_mut(&key) {
            if entry.last_accessed.elapsed() < self.ttl {
                entry.last_accessed = std::time::Instant::now();
                return Some(entry.location.clone());
            }
            self.entries.remove(&key);
        }

        let location = resolver()?;
        self.entries.insert(
            key,
            LocationServiceEntry {
                location: location.clone(),
                last_accessed: std::time::Instant::now(),
            },
        );
        Some(location)
    }

    /// Remove all entries that have been idle longer than the TTL.
    pub fn evict_expired(&mut self) {
        let ttl = self.ttl;
        self.entries
            .retain(|_, entry| entry.last_accessed.elapsed() < ttl);
    }

    /// Return the number of entries in the map (including potentially expired ones).
    #[must_use]
    pub fn len(&self) -> usize {
        self.entries.len()
    }

    /// Return `true` if the map contains no entries.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }
}

// ══════════════════════════════════════════════════════════════════════════════
// Location Resolver
// ══════════════════════════════════════════════════════════════════════════════

/// Error produced when a location cannot be resolved.
///
/// # Source
/// `packages/core/src/location-layer.ts` — `Unresolvable`.
#[derive(Debug, Clone, thiserror::Error, Serialize, Deserialize)]
pub enum LocationError {
    #[error("directory could not be resolved to a project: {directory}")]
    Unresolvable { directory: String },
}

/// Resolve a [`LocationRef`] into a [`LocationFull`].
///
/// Takes a resolver function that maps a directory path to
/// `(project_id, project_directory, optional_vcs)`. Returns `None` if
/// the resolver cannot map the directory.
///
/// # Source
/// `packages/core/src/location.ts` lines 32–45 — `layer`.
pub fn resolve_location(
    ref_: &LocationRef,
    resolver: impl FnOnce(&str) -> Option<(ProjectId, String, Option<ProjectVcs>)>,
) -> Option<LocationFull> {
    let (project_id, project_directory, vcs) = resolver(&ref_.directory)?;
    Some(LocationFull {
        directory: ref_.directory.clone(),
        workspace_id: ref_.workspace_id.clone(),
        project: LocationProjectRef {
            id: project_id,
            directory: project_directory,
        },
        vcs,
    })
}

/// Resolve a [`LocationRef`] into a [`LocationFull`], returning an error on failure.
///
/// Like [`resolve_location`] but returns [`LocationError::Unresolvable`] instead
/// of `None` when the resolver cannot map the directory.
///
/// # Source
/// `packages/core/src/location-layer.ts` — `layer`.
pub fn location_layer(
    ref_: &LocationRef,
    project_resolver: impl FnOnce(&str) -> Option<(ProjectId, String, Option<ProjectVcs>)>,
) -> Result<LocationFull, LocationError> {
    let (project_id, project_directory, vcs) =
        project_resolver(&ref_.directory).ok_or_else(|| LocationError::Unresolvable {
            directory: ref_.directory.clone(),
        })?;
    Ok(LocationFull {
        directory: ref_.directory.clone(),
        workspace_id: ref_.workspace_id.clone(),
        project: LocationProjectRef {
            id: project_id,
            directory: project_directory,
        },
        vcs,
    })
}

/// Trait for resolving directories to project information.
///
/// # Source
/// `packages/core/src/location.ts` — `layer` (Project.Service integration).
pub trait ProjectResolver {
    fn resolve(&self, directory: &str) -> Option<(ProjectId, String, Option<ProjectVcs>)>;
}

/// Create a location layer that resolves via [`ProjectResolver`].
///
/// Ported from: `packages/core/src/location.ts` — `layer`.
///
/// # Source
/// `packages/core/src/location-layer.ts` — `layer`.
pub fn layer(
    ref_: &LocationRef,
    project_service: &dyn ProjectResolver,
) -> Result<LocationFull, LocationError> {
    let resolved =
        project_service
            .resolve(&ref_.directory)
            .ok_or_else(|| LocationError::Unresolvable {
                directory: ref_.directory.clone(),
            })?;
    Ok(LocationFull {
        directory: ref_.directory.clone(),
        workspace_id: ref_.workspace_id.clone(),
        project: LocationProjectRef {
            id: resolved.0,
            directory: resolved.1,
        },
        vcs: resolved.2,
    })
}

// ── Internal path helpers ─────────────────────────────────────────────────

/// Check whether `child` is lexically contained within `parent`.
///
/// Both paths are expected to be absolute and normalized. Returns `true` when
/// `child` equals `parent` or starts with `parent/`.
///
/// # Source
/// `packages/core/src/fs-util.ts` — `FSUtil.contains`.
fn path_contains(parent: &str, child: &str) -> bool {
    let parent = parent.replace('\\', "/");
    let child = child.replace('\\', "/");
    let parent = parent.trim_end_matches('/');
    let child = child.trim_end_matches('/');

    if child == parent {
        return true;
    }
    child.starts_with(&format!("{}/", parent))
}

/// Lexically normalize a path by resolving `.` and `..` components.
///
/// This is a pure-string operation — no filesystem access.
fn path_normalize(p: &str) -> String {
    let is_absolute = p.starts_with('/');
    let parts: Vec<&str> = p
        .split('/')
        .filter(|s| !s.is_empty() && *s != ".")
        .collect();
    let mut result: Vec<&str> = Vec::new();
    for part in parts {
        if part == ".." {
            result.pop();
        } else {
            result.push(part);
        }
    }
    let normalized = result.join("/");
    if is_absolute {
        format!("/{normalized}")
    } else if normalized.is_empty() {
        ".".to_string()
    } else {
        normalized
    }
}

/// Replace backslashes with forward slashes.
///
/// # Source
/// `packages/core/src/location-mutation.ts` line 76 — `slash`.
fn slash_path(value: &str) -> String {
    value.replace('\\', "/")
}

// ══════════════════════════════════════════════════════════════════════════════
// Mutation Service
// ══════════════════════════════════════════════════════════════════════════════

/// Service for resolving mutation paths against the current location.
///
/// # Source
/// `packages/core/src/location-mutation.ts` lines 60–61, 78–153 —
/// `Service` and `layer`.
pub struct MutationService {
    /// Canonical absolute path of the location root.
    pub location_root: String,
}

impl MutationService {
    /// Create a new mutation service with the given location root.
    ///
    /// The root is normalized by stripping trailing slashes so that
    /// containment checks and resource path extraction work correctly.
    #[must_use]
    pub fn new(location_root: impl Into<String>) -> Self {
        let mut root: String = location_root.into();
        // Strip trailing slashes (but preserve "/" as-is).
        while root.len() > 1 && root.ends_with('/') {
            root.pop();
        }
        Self {
            location_root: root,
        }
    }

    /// Resolve a mutation input path against the location root.
    ///
    /// Performs lexical path normalization and filesystem validation:
    /// - Rejects relative paths that escape the location root
    /// - Detects symlinks inside the location that point outside it (`LocationEscape`)
    /// - Checks that ancestors are directories (`NonDirectoryAncestor`)
    ///
    /// Returns a [`MutationTarget`] with the canonical path, permission
    /// resource, and optional external directory authorization when the
    /// target is outside the root.
    ///
    /// # Source
    /// `packages/core/src/location-mutation.ts` lines 119–149 — `resolve`.
    pub fn resolve(
        &self,
        input: &MutationResolveInput,
    ) -> Result<MutationTarget, MutationPathError> {
        self.resolve_with_fs(input)
    }

    /// Resolve a mutation input path with filesystem validation.
    ///
    /// This method performs the same lexical check as [`resolve`] but additionally:
    /// - Uses `std::fs::canonicalize()` to follow symlinks
    /// - Returns `LocationEscape` if a symlink inside the location points outside it
    /// - Uses `std::fs::metadata()` to verify ancestors are directories
    ///
    /// # Errors
    ///
    /// Returns [`MutationPathError::RelativeEscape`] if a relative path escapes the root.
    /// Returns [`MutationPathError::LocationEscape`] if a symlink escapes the location.
    /// Returns [`MutationPathError::NonDirectoryAncestor`] if an ancestor is not a directory.
    ///
    /// # Source
    /// `packages/core/src/fs-util.ts` — `FSUtil.Service` (`realPath()`, `stat()`).
    pub fn resolve_with_fs(
        &self,
        input: &MutationResolveInput,
    ) -> Result<MutationTarget, MutationPathError> {
        let is_absolute = input.path.starts_with('/');

        // Resolve relative paths against the location root.
        let absolute = if is_absolute {
            path_normalize(&input.path)
        } else {
            path_normalize(&format!("{}/{}", self.location_root, input.path))
        };

        let lexically_internal = path_contains(&self.location_root, &absolute);

        // Relative paths that escape the location root are rejected.
        if !is_absolute && !lexically_internal {
            return Err(MutationPathError::relative_escape(&input.path));
        }

        // Filesystem checks for paths that are lexically inside the location.
        let canonical = if lexically_internal {
            let lex_path = Path::new(&absolute);

            // Check ancestors are directories.
            self.check_ancestor_directories(lex_path)?;

            // Canonicalize to follow symlinks — only if the path exists.
            match std::fs::canonicalize(lex_path) {
                Ok(canon_buf) => {
                    let canon_str = path_to_string(&canon_buf);
                    let canon_normalized = path_normalize(&canon_str);

                    // If canonical path escapes the location root, it's a symlink escape.
                    if !path_contains(&self.location_root, &canon_normalized) {
                        return Err(MutationPathError::location_escape(&absolute));
                    }

                    canon_normalized
                }
                // Path doesn't exist yet — use the lexical path as canonical.
                Err(_) => absolute.clone(),
            }
        } else {
            absolute.clone()
        };

        // Permission resource: location-relative for internal, canonical for external.
        let resource = if lexically_internal {
            let rel = if canonical == self.location_root {
                ".".to_string()
            } else {
                canonical[self.location_root.len() + 1..].to_string()
            };
            slash_path(&rel)
        } else {
            slash_path(&canonical)
        };

        // External directory authorization when outside the location root.
        let external_directory = if !lexically_internal {
            let external_dir = if input.kind == Some(MutationKind::Directory) {
                canonical.clone()
            } else {
                let parts: Vec<&str> = canonical.split('/').collect();
                if parts.len() <= 1 {
                    "/".to_string()
                } else {
                    parts[..parts.len() - 1].join("/")
                }
            };
            let external_resource = format!("{external_dir}/*");
            Some(ExternalDirectoryAuthorization {
                action: "external_directory".into(),
                directory: slash_path(&external_dir),
                resource: external_resource.clone(),
                save: external_resource,
            })
        } else {
            None
        };

        Ok(MutationTarget {
            canonical,
            resource,
            external_directory,
        })
    }

    /// Check that every ancestor directory component of `path` is actually a
    /// directory on the filesystem. Stops at the filesystem root.
    ///
    /// Returns `Ok(())` if all ancestors are directories (or don't exist yet).
    /// Returns `NonDirectoryAncestor` if any component is a file.
    fn check_ancestor_directories(&self, path: &Path) -> Result<(), MutationPathError> {
        let mut current = match path.parent() {
            Some(p) => p.to_path_buf(),
            None => return Ok(()),
        };
        loop {
            if !current.starts_with(&self.location_root) {
                break;
            }
            if current == Path::new(&self.location_root) {
                break;
            }
            // Check if this component exists and is not a directory.
            if let Ok(meta) = std::fs::metadata(&current) {
                if !meta.is_dir() {
                    return Err(MutationPathError::non_directory_ancestor(path_to_string(
                        &current,
                    )));
                }
            }
            // If metadata fails (e.g., parent doesn't exist yet), that's fine —
            // the file creation will handle it.
            match current.parent() {
                Some(parent) => current = parent.to_path_buf(),
                None => break,
            }
        }
        Ok(())
    }
}

/// Convert a `PathBuf` to a `String`, normalizing the path.
fn path_to_string(p: &Path) -> String {
    let s = p.to_string_lossy().to_string();
    path_normalize(&s)
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
        assert_eq!(key.workspace_id.unwrap().as_str(), "wrk_key1");
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

    // ── LocationServiceKey Hash ──────────────────────────────────────

    #[test]
    fn test_location_service_key_hash() {
        use std::collections::HashMap;

        let ws1 = WorkspaceId::ascending("wrk_hash1").expect("valid");
        let ws2 = WorkspaceId::ascending("wrk_hash2").expect("valid");

        let key1 = LocationServiceKey {
            directory: "/app".into(),
            workspace_id: Some(ws1),
        };
        let key2 = LocationServiceKey {
            directory: "/app".into(),
            workspace_id: Some(ws2),
        };
        let key3 = LocationServiceKey {
            directory: "/other".into(),
            workspace_id: None,
        };

        let mut map: HashMap<LocationServiceKey, i32> = HashMap::new();
        map.insert(key1.clone(), 1);
        map.insert(key2.clone(), 2);
        map.insert(key3.clone(), 3);

        assert_eq!(map.get(&key1).expect("key1 present"), &1);
        assert_eq!(map.get(&key2).expect("key2 present"), &2);
        assert_eq!(map.get(&key3).expect("key3 present"), &3);
        assert_eq!(map.len(), 3);

        // Same key (no workspace) should overwrite
        let key_no_ws = LocationServiceKey {
            directory: "/tmp".into(),
            workspace_id: None,
        };
        map.insert(key_no_ws.clone(), 10);
        assert_eq!(map.get(&key_no_ws).expect("key_no_ws present"), &10);

        let same_key = LocationServiceKey {
            directory: "/tmp".into(),
            workspace_id: None,
        };
        map.insert(same_key, 20);
        // HashMap with 3 original keys + 1 new key (overwritten) = 4 entries
        assert_eq!(map.len(), 4);
        assert_eq!(
            map.get(&LocationServiceKey {
                directory: "/tmp".into(),
                workspace_id: None,
            })
            .expect("overwritten key present"),
            &20
        );
    }

    // ── resolve_location ─────────────────────────────────────────────

    #[test]
    fn test_resolve_location_valid_directory() {
        let ref_ = LocationRef::new("/home/user/myproject");
        let result = resolve_location(&ref_, |dir| {
            assert_eq!(dir, "/home/user/myproject");
            Some((
                ProjectId::new("proj_abc"),
                "/home/user/myproject".to_string(),
                Some(ProjectVcs::git("/home/user/myproject/.git")),
            ))
        });
        let full = result.expect("resolver mapped the directory");
        assert_eq!(full.directory, "/home/user/myproject");
        assert_eq!(full.project.id.0, "proj_abc");
        assert_eq!(full.project.directory, "/home/user/myproject");
        assert!(full.workspace_id.is_none());
        assert!(full.vcs.is_some());
        match &full.vcs {
            Some(ProjectVcs::Git { store }) => {
                assert_eq!(store, "/home/user/myproject/.git");
            }
            None => {}
        }
    }

    #[test]
    fn test_resolve_location_unresolvable_directory() {
        let ref_ = LocationRef::new("/unknown/path");
        let result = resolve_location(&ref_, |_dir| None);
        assert!(result.is_none());
    }

    #[test]
    fn test_resolve_location_with_workspace_id() {
        let ws = WorkspaceId::ascending("wrk_resloc").expect("valid");
        let ref_ = LocationRef::with_workspace("/ws/proj", ws.clone());
        let result = resolve_location(&ref_, |_dir| {
            Some((ProjectId::new("p_ws"), "/ws/proj".to_string(), None))
        });
        let full = result.expect("resolver mapped the directory");
        assert_eq!(
            full.workspace_id.as_ref().expect("has workspace").as_str(),
            "wrk_resloc"
        );
        assert_eq!(full.project.id.0, "p_ws");
    }

    // ── LocationFull roundtrip with workspace_id ─────────────────────

    #[test]
    fn test_location_full_roundtrip_all_fields() {
        let ws = WorkspaceId::ascending("wrk_full").expect("valid");
        let full = LocationFull {
            directory: "/repo".into(),
            workspace_id: Some(ws),
            project: LocationProjectRef {
                id: ProjectId::new("proj_full"),
                directory: "/repo".into(),
            },
            vcs: Some(ProjectVcs::git("/repo/.git")),
        };
        let json = serde_json::to_string(&full).expect("serialize");
        let parsed: LocationFull = serde_json::from_str(&json).expect("deserialize");
        assert_eq!(parsed.directory, "/repo");
        assert_eq!(
            parsed.workspace_id.expect("has workspace_id").as_str(),
            "wrk_full"
        );
        assert_eq!(parsed.project.id.0, "proj_full");
        assert_eq!(parsed.project.directory, "/repo");
        let vcs = parsed.vcs.expect("has vcs");
        match vcs {
            ProjectVcs::Git { store } => assert_eq!(store, "/repo/.git"),
        }
    }

    // ── MutationService::resolve — internal paths ────────────────────

    #[test]
    fn test_mutation_resolve_internal_relative() {
        let svc = MutationService::new("/home/user/project");
        let input = MutationResolveInput {
            path: "src/main.rs".into(),
            kind: Some(MutationKind::File),
        };
        let target = svc.resolve(&input).expect("resolve internal relative");
        assert_eq!(target.canonical, "/home/user/project/src/main.rs");
        assert_eq!(target.resource, "src/main.rs");
        assert!(target.external_directory.is_none());
    }

    #[test]
    fn test_mutation_resolve_internal_relative_directory_kind() {
        let svc = MutationService::new("/app");
        let input = MutationResolveInput {
            path: "lib".into(),
            kind: Some(MutationKind::Directory),
        };
        let target = svc.resolve(&input).expect("resolve internal directory");
        assert_eq!(target.canonical, "/app/lib");
        assert_eq!(target.resource, "lib");
        assert!(target.external_directory.is_none());
    }

    #[test]
    fn test_mutation_resolve_internal_absolute_inside_root() {
        let svc = MutationService::new("/app");
        let input = MutationResolveInput {
            path: "/app/src/lib.rs".into(),
            kind: None,
        };
        let target = svc.resolve(&input).expect("resolve internal absolute");
        assert_eq!(target.canonical, "/app/src/lib.rs");
        assert_eq!(target.resource, "src/lib.rs");
        assert!(target.external_directory.is_none());
    }

    #[test]
    fn test_mutation_resolve_internal_normalization() {
        // "src/../src/main.rs" should normalize to "src/main.rs"
        let svc = MutationService::new("/app");
        let input = MutationResolveInput {
            path: "src/../src/main.rs".into(),
            kind: Some(MutationKind::File),
        };
        let target = svc.resolve(&input).expect("resolve with .. normalization");
        assert_eq!(target.canonical, "/app/src/main.rs");
        assert_eq!(target.resource, "src/main.rs");
    }

    #[test]
    fn test_mutation_resolve_internal_dot_path() {
        let svc = MutationService::new("/app");
        let input = MutationResolveInput {
            path: ".".into(),
            kind: Some(MutationKind::Directory),
        };
        let target = svc.resolve(&input).expect("resolve dot path");
        assert_eq!(target.canonical, "/app");
        assert_eq!(target.resource, ".");
    }

    #[test]
    fn test_mutation_resolve_internal_trailing_slash_root() {
        // Constructor should normalize trailing slashes on the root.
        let svc = MutationService::new("/app/");
        assert_eq!(svc.location_root, "/app");
        let input = MutationResolveInput {
            path: "src/main.rs".into(),
            kind: Some(MutationKind::File),
        };
        let target = svc
            .resolve(&input)
            .expect("resolve with trailing-slash root");
        assert_eq!(target.canonical, "/app/src/main.rs");
        assert_eq!(target.resource, "src/main.rs");
    }

    #[test]
    fn test_mutation_resolve_internal_multiple_trailing_slashes() {
        let svc = MutationService::new("///app///");
        // Multiple leading slashes are not collapsed by our normalization,
        // but trailing slashes are. The root is "///app".
        assert_eq!(svc.location_root, "///app");
    }

    // ── MutationService::resolve — external paths ────────────────────

    #[test]
    fn test_mutation_resolve_external_absolute() {
        let svc = MutationService::new("/app");
        let input = MutationResolveInput {
            path: "/mnt/data/output.txt".into(),
            kind: Some(MutationKind::File),
        };
        let target = svc.resolve(&input).expect("resolve external absolute");
        assert_eq!(target.canonical, "/mnt/data/output.txt");
        assert_eq!(target.resource, "/mnt/data/output.txt");
        let auth = target
            .external_directory
            .expect("has external_directory authorization");
        assert_eq!(auth.action, "external_directory");
        assert_eq!(auth.directory, "/mnt/data");
        assert_eq!(auth.resource, "/mnt/data/*");
        assert_eq!(auth.save, "/mnt/data/*");
    }

    #[test]
    fn test_mutation_resolve_external_directory_kind() {
        let svc = MutationService::new("/app");
        let input = MutationResolveInput {
            path: "/mnt/shared".into(),
            kind: Some(MutationKind::Directory),
        };
        let target = svc.resolve(&input).expect("resolve external directory");
        let auth = target
            .external_directory
            .expect("has external_directory authorization");
        // When kind is Directory, the boundary is the target itself
        assert_eq!(auth.directory, "/mnt/shared");
        assert_eq!(auth.resource, "/mnt/shared/*");
    }

    #[test]
    fn test_mutation_resolve_external_no_kind() {
        let svc = MutationService::new("/app");
        let input = MutationResolveInput {
            path: "/etc/config".into(),
            kind: None,
        };
        let target = svc.resolve(&input).expect("resolve external without kind");
        let auth = target
            .external_directory
            .expect("has external_directory authorization");
        // When kind is None, the boundary is the parent directory
        assert_eq!(auth.directory, "/etc");
        assert_eq!(auth.resource, "/etc/*");
    }

    // ── MutationService::resolve — escape errors ─────────────────────

    #[test]
    fn test_mutation_resolve_relative_escape() {
        let svc = MutationService::new("/app");
        let input = MutationResolveInput {
            path: "../outside".into(),
            kind: None,
        };
        let err = svc
            .resolve(&input)
            .expect_err("relative escape should fail");
        match err {
            MutationPathError::RelativeEscape { path } => {
                assert_eq!(path, "../outside");
            }
            other => panic!("expected RelativeEscape, got {other:?}"),
        }
    }

    #[test]
    fn test_mutation_resolve_relative_escape_deep() {
        let svc = MutationService::new("/home/user/project");
        let input = MutationResolveInput {
            path: "../../../etc/passwd".into(),
            kind: None,
        };
        let err = svc
            .resolve(&input)
            .expect_err("deep relative escape should fail");
        match err {
            MutationPathError::RelativeEscape { path } => {
                assert_eq!(path, "../../../etc/passwd");
            }
            other => panic!("expected RelativeEscape, got {other:?}"),
        }
    }

    // ── MutationService::resolve — serialization roundtrip ───────────

    #[test]
    fn test_mutation_resolve_result_serde_roundtrip() {
        let svc = MutationService::new("/app");
        let input = MutationResolveInput {
            path: "src/lib.rs".into(),
            kind: Some(MutationKind::File),
        };
        let target = svc.resolve(&input).expect("resolve");
        let json = serde_json::to_string(&target).expect("serialize target");
        let parsed: MutationTarget = serde_json::from_str(&json).expect("deserialize target");
        assert_eq!(parsed.canonical, target.canonical);
        assert_eq!(parsed.resource, target.resource);
        assert!(parsed.external_directory.is_none());
    }

    // ── path_contains ────────────────────────────────────────────────

    #[test]
    fn test_path_contains_identical() {
        assert!(path_contains("/app", "/app"));
    }

    #[test]
    fn test_path_contains_child() {
        assert!(path_contains("/app", "/app/src"));
    }

    #[test]
    fn test_path_contains_deep_child() {
        assert!(path_contains("/app", "/app/src/lib/util.rs"));
    }

    #[test]
    fn test_path_contains_outside() {
        assert!(!path_contains("/app", "/mnt/data"));
    }

    #[test]
    fn test_path_contains_prefix_but_not_child() {
        // "/app2" should not be contained in "/app"
        assert!(!path_contains("/app", "/app2"));
        assert!(!path_contains("/app", "/application"));
    }

    #[test]
    fn test_path_contains_trailing_slashes() {
        assert!(path_contains("/app/", "/app/src"));
        assert!(path_contains("/app", "/app/src/"));
    }

    #[test]
    fn test_path_contains_backslash_normalization() {
        assert!(path_contains("\\app", "\\app\\src"));
    }

    // ── path_normalize ───────────────────────────────────────────────

    #[test]
    fn test_path_normalize_identity() {
        assert_eq!(path_normalize("/foo/bar"), "/foo/bar");
    }

    #[test]
    fn test_path_normalize_dot() {
        assert_eq!(path_normalize("/foo/./bar"), "/foo/bar");
    }

    #[test]
    fn test_path_normalize_dotdot() {
        assert_eq!(path_normalize("/foo/bar/../baz"), "/foo/baz");
    }

    #[test]
    fn test_path_normalize_relative_dotdot_escape() {
        assert_eq!(path_normalize("../outside"), "outside");
    }

    #[test]
    fn test_path_normalize_multiple_dotdots() {
        assert_eq!(path_normalize("/a/b/c/../../d"), "/a/d");
    }

    #[test]
    fn test_path_normalize_empty() {
        assert_eq!(path_normalize(""), ".");
    }

    #[test]
    fn test_path_normalize_dot_only() {
        assert_eq!(path_normalize("."), ".");
    }

    #[test]
    fn test_path_normalize_deep_escape() {
        assert_eq!(
            path_normalize("/home/user/project/../../../etc/passwd"),
            "/etc/passwd"
        );
    }

    // ── slash_path ───────────────────────────────────────────────────

    #[test]
    fn test_slash_path_backslashes() {
        assert_eq!(slash_path(r"C:\Users\test"), "C:/Users/test");
    }

    #[test]
    fn test_slash_path_no_backslashes() {
        assert_eq!(slash_path("/usr/local/bin"), "/usr/local/bin");
    }

    // ── MutationService::resolve_with_fs — symlink escape ──────────

    #[test]
    fn test_resolve_with_fs_symlink_escape() {
        let tmp = std::env::temp_dir().join("rustcode_test_symlink_escape");
        let _ = std::fs::remove_dir_all(&tmp);
        std::fs::create_dir_all(tmp.join("location")).expect("create location dir");
        std::fs::create_dir_all(tmp.join("outside")).expect("create outside");
        std::fs::write(tmp.join("outside/file.txt"), "data").expect("write file in outside");

        // Create a symlink inside location pointing outside
        #[cfg(unix)]
        std::os::unix::fs::symlink(tmp.join("outside"), tmp.join("location/link"))
            .expect("symlink");

        let location_dir = tmp.join("location");
        let svc = MutationService::new(location_dir.to_string_lossy());

        let input = MutationResolveInput {
            path: "link/file.txt".into(),
            kind: Some(MutationKind::File),
        };

        #[cfg(unix)]
        {
            let err = svc
                .resolve_with_fs(&input)
                .expect_err("symlink escape should fail");
            match err {
                MutationPathError::LocationEscape { path } => {
                    assert!(path.contains("link/file.txt"));
                }
                other => panic!("expected LocationEscape, got {other:?}"),
            }
        }

        // Cleanup
        let _ = std::fs::remove_dir_all(&tmp);
    }

    #[test]
    fn test_resolve_with_fs_symlink_stays_inside() {
        let tmp = std::env::temp_dir().join("rustcode_test_symlink_inside");
        let _ = std::fs::remove_dir_all(&tmp);
        std::fs::create_dir_all(tmp.join("location/subdir")).expect("create dirs");
        std::fs::write(tmp.join("location/subdir/file.txt"), "data").expect("write file");

        // Create a symlink inside location pointing to another location inside
        #[cfg(unix)]
        std::os::unix::fs::symlink(tmp.join("location/subdir"), tmp.join("location/link"))
            .expect("symlink");

        let location_dir = tmp.join("location");
        let svc = MutationService::new(location_dir.to_string_lossy());

        let input = MutationResolveInput {
            path: "link/file.txt".into(),
            kind: Some(MutationKind::File),
        };

        #[cfg(unix)]
        {
            let target = svc
                .resolve_with_fs(&input)
                .expect("resolve symlink staying inside");
            assert!(target.canonical.contains("subdir/file.txt"));
        }

        // Cleanup
        let _ = std::fs::remove_dir_all(&tmp);
    }

    // ── MutationService::resolve_with_fs — non-directory ancestor ──

    #[test]
    fn test_resolve_with_fs_non_directory_ancestor() {
        let tmp = std::env::temp_dir().join("rustcode_test_non_dir_ancestor");
        let _ = std::fs::remove_dir_all(&tmp);
        std::fs::create_dir_all(&tmp).expect("create dirs");

        // Create a file where a directory component should be
        std::fs::write(tmp.join("not_a_dir"), "file content").expect("write file");

        let location_dir = tmp.to_path_buf();
        let svc = MutationService::new(location_dir.to_string_lossy());

        let input = MutationResolveInput {
            path: "not_a_dir/child/file.txt".into(),
            kind: Some(MutationKind::File),
        };

        let err = svc
            .resolve_with_fs(&input)
            .expect_err("non-directory ancestor should fail");
        match err {
            MutationPathError::NonDirectoryAncestor { path } => {
                assert!(path.contains("not_a_dir"));
            }
            other => panic!("expected NonDirectoryAncestor, got {other:?}"),
        }

        // Cleanup
        let _ = std::fs::remove_dir_all(&tmp);
    }

    #[test]
    fn test_resolve_with_fs_missing_path_ok() {
        let tmp = std::env::temp_dir().join("rustcode_test_missing_path");
        let _ = std::fs::remove_dir_all(&tmp);
        std::fs::create_dir_all(&tmp).expect("create dirs");

        let location_dir = tmp.to_path_buf();
        let svc = MutationService::new(location_dir.to_string_lossy());

        // Path doesn't exist yet — should succeed (no fs check failure)
        let input = MutationResolveInput {
            path: "new_dir/file.txt".into(),
            kind: Some(MutationKind::File),
        };

        let target = svc
            .resolve_with_fs(&input)
            .expect("missing path should resolve");
        assert_eq!(
            target.canonical,
            tmp.join("new_dir/file.txt").to_string_lossy()
        );

        // Cleanup
        let _ = std::fs::remove_dir_all(&tmp);
    }

    // ── MutationService::resolve delegates to resolve_with_fs ─────

    #[test]
    fn test_resolve_delegates_to_resolve_with_fs() {
        let tmp = std::env::temp_dir().join("rustcode_test_resolve_delegates");
        let _ = std::fs::remove_dir_all(&tmp);
        std::fs::create_dir_all(tmp.join("location")).expect("create dirs");
        std::fs::write(tmp.join("location/file.txt"), "data").expect("write file");

        let location_dir = tmp.join("location");
        let svc = MutationService::new(location_dir.to_string_lossy());

        let input = MutationResolveInput {
            path: "file.txt".into(),
            kind: Some(MutationKind::File),
        };

        // Both resolve and resolve_with_fs should return the same result
        let target_resolve = svc.resolve(&input).expect("resolve");
        let target_fs = svc.resolve_with_fs(&input).expect("resolve_with_fs");
        assert_eq!(target_resolve.canonical, target_fs.canonical);
        assert_eq!(target_resolve.resource, target_fs.resource);

        // Cleanup
        let _ = std::fs::remove_dir_all(&tmp);
    }

    // ── location_layer ──────────────────────────────────────────────

    #[test]
    fn test_location_layer_success() {
        let ref_ = LocationRef::new("/home/user/project");
        let full = location_layer(&ref_, |dir| {
            Some((
                ProjectId::new("proj_layer"),
                dir.to_string(),
                Some(ProjectVcs::git("/home/user/project/.git")),
            ))
        })
        .expect("layer should resolve");
        assert_eq!(full.directory, "/home/user/project");
        assert_eq!(full.project.id.0, "proj_layer");
        assert!(full.vcs.is_some());
    }

    #[test]
    fn test_location_layer_unresolvable() {
        let ref_ = LocationRef::new("/unknown/dir");
        let err = location_layer(&ref_, |_| None).expect_err("should fail");
        match err {
            LocationError::Unresolvable { directory } => {
                assert_eq!(directory, "/unknown/dir");
            }
        }
    }

    #[test]
    fn test_location_layer_with_workspace() {
        let ws = WorkspaceId::ascending("wrk_layer").expect("valid");
        let ref_ = LocationRef::with_workspace("/ws/proj", ws);
        let full = location_layer(&ref_, |_dir| {
            Some((ProjectId::new("p_layer"), "/ws/proj".to_string(), None))
        })
        .expect("layer should resolve");
        assert_eq!(
            full.workspace_id.expect("has workspace").as_str(),
            "wrk_layer"
        );
    }

    #[test]
    fn test_location_error_display() {
        let err = LocationError::Unresolvable {
            directory: "/foo".into(),
        };
        assert!(err.to_string().contains("/foo"));
        assert!(err.to_string().contains("could not be resolved"));
    }

    // ── layer() with ProjectResolver ────────────────────────────────

    struct MockProjectResolver;

    impl ProjectResolver for MockProjectResolver {
        fn resolve(&self, directory: &str) -> Option<(ProjectId, String, Option<ProjectVcs>)> {
            match directory {
                "/home/user/project" => Some((
                    ProjectId::new("proj_mock"),
                    directory.to_string(),
                    Some(ProjectVcs::git("/home/user/project/.git")),
                )),
                "/ws/proj" => Some((ProjectId::new("p_ws_mock"), directory.to_string(), None)),
                _ => None,
            }
        }
    }

    #[test]
    fn test_layer_success() {
        let ref_ = LocationRef::new("/home/user/project");
        let resolver = MockProjectResolver;
        let full = layer(&ref_, &resolver).expect("layer should resolve");
        assert_eq!(full.directory, "/home/user/project");
        assert_eq!(full.project.id.0, "proj_mock");
        assert_eq!(full.project.directory, "/home/user/project");
        assert!(full.vcs.is_some());
        assert!(full.workspace_id.is_none());
    }

    #[test]
    fn test_layer_unresolvable() {
        let ref_ = LocationRef::new("/unknown/dir");
        let resolver = MockProjectResolver;
        let err = layer(&ref_, &resolver).expect_err("should fail");
        match err {
            LocationError::Unresolvable { directory } => {
                assert_eq!(directory, "/unknown/dir");
            }
        }
    }

    #[test]
    fn test_layer_with_workspace() {
        let ws = WorkspaceId::ascending("wrk_layer_dyn").expect("valid");
        let ref_ = LocationRef::with_workspace("/ws/proj", ws);
        let resolver = MockProjectResolver;
        let full = layer(&ref_, &resolver).expect("layer should resolve");
        assert_eq!(
            full.workspace_id.expect("has workspace").as_str(),
            "wrk_layer_dyn"
        );
        assert_eq!(full.project.id.0, "p_ws_mock");
    }

    #[test]
    fn test_layer_no_vcs() {
        let ref_ = LocationRef::new("/ws/proj");
        let resolver = MockProjectResolver;
        let full = layer(&ref_, &resolver).expect("layer should resolve");
        assert!(full.vcs.is_none());
    }

    // ── LocationServiceMap ─────────────────────────────────────────

    fn make_key(dir: &str) -> LocationServiceKey {
        LocationServiceKey {
            directory: dir.into(),
            workspace_id: None,
        }
    }

    #[test]
    fn test_service_map_get_or_resolve_insert() {
        let mut map = LocationServiceMap::new(std::time::Duration::from_secs(60));
        let key = make_key("/app");
        let result = map.get_or_resolve(key.clone(), || {
            Some(LocationFull {
                directory: "/app".into(),
                workspace_id: None,
                project: LocationProjectRef {
                    id: ProjectId::new("p1"),
                    directory: "/app".into(),
                },
                vcs: None,
            })
        });
        assert!(result.is_some());
        let loc = result.unwrap();
        assert_eq!(loc.project.id.0, "p1");
        assert_eq!(map.len(), 1);
    }

    #[test]
    fn test_service_map_get_or_resolve_cached() {
        let mut map = LocationServiceMap::new(std::time::Duration::from_secs(60));
        let key = make_key("/app");

        // First call inserts
        map.get_or_resolve(key.clone(), || {
            Some(LocationFull {
                directory: "/app".into(),
                workspace_id: None,
                project: LocationProjectRef {
                    id: ProjectId::new("p1"),
                    directory: "/app".into(),
                },
                vcs: None,
            })
        });

        // Second call should hit cache, resolver should NOT be called
        let result = map.get_or_resolve(key, || panic!("resolver should not be called"));
        assert!(result.is_some());
        assert_eq!(result.unwrap().project.id.0, "p1");
    }

    #[test]
    fn test_service_map_get_or_resolve_returns_none() {
        let mut map = LocationServiceMap::new(std::time::Duration::from_secs(60));
        let key = make_key("/unknown");
        let result = map.get_or_resolve(key, || None);
        assert!(result.is_none());
        assert!(map.is_empty());
    }

    #[test]
    fn test_service_map_len_and_is_empty() {
        let mut map = LocationServiceMap::new(std::time::Duration::from_secs(60));
        assert!(map.is_empty());
        assert_eq!(map.len(), 0);

        map.get_or_resolve(make_key("/a"), || {
            Some(LocationFull {
                directory: "/a".into(),
                workspace_id: None,
                project: LocationProjectRef {
                    id: ProjectId::new("pa"),
                    directory: "/a".into(),
                },
                vcs: None,
            })
        });
        assert!(!map.is_empty());
        assert_eq!(map.len(), 1);

        map.get_or_resolve(make_key("/b"), || {
            Some(LocationFull {
                directory: "/b".into(),
                workspace_id: None,
                project: LocationProjectRef {
                    id: ProjectId::new("pb"),
                    directory: "/b".into(),
                },
                vcs: None,
            })
        });
        assert_eq!(map.len(), 2);
    }

    #[test]
    fn test_service_map_evict_expired() {
        let mut map = LocationServiceMap::new(std::time::Duration::from_secs(0));
        map.get_or_resolve(make_key("/expired"), || {
            Some(LocationFull {
                directory: "/expired".into(),
                workspace_id: None,
                project: LocationProjectRef {
                    id: ProjectId::new("pe"),
                    directory: "/expired".into(),
                },
                vcs: None,
            })
        });
        assert_eq!(map.len(), 1);

        // TTL is 0 seconds, so the entry is already expired
        map.evict_expired();
        assert!(map.is_empty());
        assert_eq!(map.len(), 0);
    }

    #[test]
    fn test_service_map_evict_preserves_fresh() {
        let mut map = LocationServiceMap::new(std::time::Duration::from_secs(60));
        map.get_or_resolve(make_key("/fresh"), || {
            Some(LocationFull {
                directory: "/fresh".into(),
                workspace_id: None,
                project: LocationProjectRef {
                    id: ProjectId::new("pf"),
                    directory: "/fresh".into(),
                },
                vcs: None,
            })
        });
        assert_eq!(map.len(), 1);

        // Entry was just inserted, so it should NOT be evicted
        map.evict_expired();
        assert_eq!(map.len(), 1);
    }

    #[test]
    fn test_service_map_default_ttl() {
        let map = LocationServiceMap::default();
        assert_eq!(map.ttl, std::time::Duration::from_secs(60 * 60));
        assert!(map.is_empty());
    }
}
