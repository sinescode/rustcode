//! Reference/citation types — local and git sources, materialization, guidance.
//!
//! Ported from:
//! - `packages/core/src/reference.ts` — Reference namespace, LocalSource, GitSource, Source union,
//!   Info, Data, Editor, Interface, Service, Event
//! - `packages/core/src/reference/guidance.ts` — ReferenceGuidance namespace, Summary, render, Interface
//!
//! OpenCode commit: 5d0f86606ac30690f79f0a6a9f41a1f49fe95d0b

use crate::bus::{GlobalEvent, SharedBus};
use crate::repository::{
    parse_remote_repository, RepositoryCacheEnsureInput, RepositoryReference, RepositoryService,
};
use serde::{Deserialize, Serialize};
use std::path::Path;
use std::sync::Arc;

use crate::system_context::{SystemContextKey, SystemContextSource};

// ══════════════════════════════════════════════════════════════════════════════
// Reference Sources — local directories and git repositories
// ══════════════════════════════════════════════════════════════════════════════

/// A reference to a local directory on the filesystem.
///
/// # Source
/// `packages/core/src/reference.ts` lines 12–17.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct LocalReferenceSource {
    /// Always `"local"` for local directory references.
    #[serde(rename = "type")]
    pub source_type: String,

    /// Absolute path to the referenced directory.
    pub path: String,

    /// Optional description of what this directory contains.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,

    /// Whether this reference should be hidden from the user.
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub hidden: bool,
}

impl LocalReferenceSource {
    /// Create a new local reference source.
    #[must_use]
    pub fn new(path: impl Into<String>) -> Self {
        Self {
            source_type: "local".into(),
            path: path.into(),
            description: None,
            hidden: false,
        }
    }
}

/// A reference to a git repository (remote, cached locally).
///
/// # Source
/// `packages/core/src/reference.ts` lines 19–25.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct GitReferenceSource {
    /// Always `"git"` for git repository references.
    #[serde(rename = "type")]
    pub source_type: String,

    /// The repository identifier (URL or shorthand like `"owner/repo"`).
    pub repository: String,

    /// Optional branch to checkout (defaults to the repository default).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub branch: Option<String>,

    /// Optional description of what this repository contains.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,

    /// Whether this reference should be hidden from the user.
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub hidden: bool,
}

impl GitReferenceSource {
    /// Create a new git reference source.
    #[must_use]
    pub fn new(repository: impl Into<String>) -> Self {
        Self {
            source_type: "git".into(),
            repository: repository.into(),
            branch: None,
            description: None,
            hidden: false,
        }
    }
}

/// Tagged union of reference source types.
///
/// # Source
/// `packages/core/src/reference.ts` lines 27–28.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "type")]
pub enum ReferenceSource {
    /// A local directory on the filesystem.
    #[serde(rename = "local")]
    Local(LocalReferenceSource),

    /// A git repository that will be cached locally.
    #[serde(rename = "git")]
    Git(GitReferenceSource),
}

impl ReferenceSource {
    /// Check whether this is a local source.
    #[must_use]
    pub fn is_local(&self) -> bool {
        matches!(self, Self::Local(_))
    }

    /// Check whether this is a git source.
    #[must_use]
    pub fn is_git(&self) -> bool {
        matches!(self, Self::Git(_))
    }

    /// Get the source type string.
    #[must_use]
    pub fn source_type(&self) -> &str {
        match self {
            Self::Local(_) => "local",
            Self::Git(_) => "git",
        }
    }

    /// Get the optional description from either local or git source.
    #[must_use]
    pub fn description(&self) -> Option<&str> {
        match self {
            Self::Local(l) => l.description.as_deref(),
            Self::Git(g) => g.description.as_deref(),
        }
    }

    /// Check whether this source is marked as hidden.
    #[must_use]
    pub fn is_hidden(&self) -> bool {
        match self {
            Self::Local(l) => l.hidden,
            Self::Git(g) => g.hidden,
        }
    }

    /// Get a hint for the filesystem path of this source.
    ///
    /// For local sources this is the directory path; for git sources
    /// this is the repository identifier.
    #[must_use]
    pub fn path_hint(&self) -> &str {
        match self {
            Self::Local(l) => &l.path,
            Self::Git(g) => &g.repository,
        }
    }
}

// ══════════════════════════════════════════════════════════════════════════════
// Reference Errors
// ══════════════════════════════════════════════════════════════════════════════

/// Errors that can occur during reference materialization.
///
/// # Source
/// `packages/core/src/reference.ts` — error handling for git source materialization.
#[derive(Debug, Clone, thiserror::Error)]
pub enum ReferenceError {
    /// A git clone or fetch operation failed.
    #[error("git clone failed for '{repository}': {message}")]
    GitCloneFailed {
        /// The repository identifier that failed.
        repository: String,
        /// Description of the failure.
        message: String,
    },

    /// The named reference does not exist.
    #[error("reference not found: '{name}'")]
    NotFound {
        /// The name that was looked up.
        name: String,
    },

    /// The source is not a git source and cannot be materialized from a remote.
    #[error("source is not a git reference: '{name}'")]
    SourceNotGit {
        /// The name of the non-git source.
        name: String,
    },
}

// ══════════════════════════════════════════════════════════════════════════════
// Reference Info — materialized reference metadata
// ══════════════════════════════════════════════════════════════════════════════

/// Complete information about a materialized reference.
///
/// # Source
/// `packages/core/src/reference.ts` lines 34–40.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ReferenceInfo {
    /// The reference name (unique key).
    pub name: String,

    /// The absolute path where the reference is materialized on disk.
    pub path: String,

    /// Optional description for display and filtering.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,

    /// Whether this reference should be hidden from the user.
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub hidden: bool,

    /// The source configuration (local directory or git repository).
    pub source: ReferenceSource,
}

// ══════════════════════════════════════════════════════════════════════════════
// Reference Data — the mutable state map
// ══════════════════════════════════════════════════════════════════════════════

/// The mutable state for all registered references.
///
/// # Source
/// `packages/core/src/reference.ts` lines 42–44.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct ReferenceData {
    /// Map of reference names to their source configurations.
    pub sources: std::collections::HashMap<String, ReferenceSource>,
}

impl ReferenceData {
    /// Add or replace a reference source.
    pub fn add(&mut self, name: impl Into<String>, source: ReferenceSource) {
        self.sources.insert(name.into(), source);
    }

    /// Remove a reference by name.
    pub fn remove(&mut self, name: &str) -> bool {
        self.sources.remove(name).is_some()
    }

    /// List all registered (name, source) pairs.
    #[must_use]
    pub fn list(&self) -> Vec<(&str, &ReferenceSource)> {
        let mut entries: Vec<_> = self.sources.iter().map(|(k, v)| (k.as_str(), v)).collect();
        entries.sort_by(|a, b| a.0.cmp(b.0));
        entries
    }

    /// Get a reference source by name.
    #[must_use]
    pub fn get(&self, name: &str) -> Option<&ReferenceSource> {
        self.sources.get(name)
    }

    /// Materialize all git sources by cloning/fetching into the cache directory.
    ///
    /// Iterates all registered sources; for each git source, parses the
    /// repository URL, clones into `cache_root`, and replaces the source
    /// with a local reference pointing to the cached location.
    ///
    /// Returns one result per source, in the order of [`list()`](Self::list).
    pub fn materialize(&mut self, cache_root: &Path) -> Vec<Result<String, ReferenceError>> {
        let names: Vec<String> = self.sources.keys().cloned().collect();
        names
            .into_iter()
            .map(|name| self.materialize_one(&name, cache_root))
            .collect()
    }

    fn materialize_one(
        &mut self,
        name: &str,
        cache_root: &Path,
    ) -> Result<String, ReferenceError> {
        let source = self
            .sources
            .get(name)
            .ok_or_else(|| ReferenceError::NotFound {
                name: name.to_string(),
            })?;

        let git_source = match source {
            ReferenceSource::Git(g) => g.clone(),
            _ => {
                return Err(ReferenceError::SourceNotGit {
                    name: name.to_string(),
                })
            }
        };

        let remote_ref = parse_remote_repository(&git_source.repository).map_err(|e| {
            ReferenceError::GitCloneFailed {
                repository: git_source.repository.clone(),
                message: e.to_string(),
            }
        })?;

        let repo_svc = RepositoryService::new(cache_root);
        let local_path =
            repo_svc.cache_path(&RepositoryReference::Remote(remote_ref.clone()));

        let input = RepositoryCacheEnsureInput {
            reference: remote_ref,
            refresh: false,
            branch: git_source.branch.clone(),
        };

        let rt = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .map_err(|e| ReferenceError::GitCloneFailed {
                repository: git_source.repository.clone(),
                message: format!("failed to create runtime: {e}"),
            })?;

        rt.block_on(repo_svc.ensure(&input)).map_err(|e| ReferenceError::GitCloneFailed {
            repository: git_source.repository.clone(),
            message: e.to_string(),
        })?;

        self.sources.insert(
            name.to_string(),
            ReferenceSource::Local(LocalReferenceSource {
                source_type: "local".into(),
                path: local_path.display().to_string(),
                description: git_source.description,
                hidden: git_source.hidden,
            }),
        );

        Ok(name.to_string())
    }
}

// ══════════════════════════════════════════════════════════════════════════════
// Reference Editor — scoped mutation editor
// ══════════════════════════════════════════════════════════════════════════════

/// Scoped mutation editor for reference data.
///
/// Ported from: `packages/core/src/reference.ts` — `Editor` type
pub struct ReferenceEditor<'a> {
    data: &'a mut ReferenceData,
}

impl<'a> ReferenceEditor<'a> {
    /// Create a new editor wrapping the given reference data.
    #[must_use]
    pub fn new(data: &'a mut ReferenceData) -> Self {
        Self { data }
    }

    /// Add or replace a reference source.
    pub fn set_source(&mut self, name: &str, source: ReferenceSource) {
        self.data.add(name, source);
    }

    /// Remove a reference by name.
    pub fn remove_source(&mut self, name: &str) -> bool {
        self.data.remove(name)
    }

    /// List all sources.
    #[must_use]
    pub fn list_sources(&self) -> Vec<(&str, &ReferenceSource)> {
        self.data.list()
    }

    /// Get a source by name.
    #[must_use]
    pub fn get_source(&self, name: &str) -> Option<&ReferenceSource> {
        self.data.get(name)
    }
}

// ══════════════════════════════════════════════════════════════════════════════
// Reference Service
// ══════════════════════════════════════════════════════════════════════════════

/// Service for managing and looking up references.
///
/// Wraps [`ReferenceData`] and provides filtered lookups that produce
/// [`ReferenceSummary`] values suitable for system-context rendering.
///
/// # Source
/// `packages/core/src/reference.ts` — Reference.Service context.
#[derive(Debug, Clone, Default)]
pub struct ReferenceService {
    data: ReferenceData,
    event_bus: Option<Arc<SharedBus>>,
}

impl ReferenceService {
    /// Create a new empty reference service.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Create a reference service with an event bus for change notifications.
    #[must_use]
    pub fn with_event_bus(bus: Arc<SharedBus>) -> Self {
        Self {
            data: ReferenceData::default(),
            event_bus: Some(bus),
        }
    }

    /// Add or replace a reference source.
    pub fn add(&mut self, name: impl Into<String>, source: ReferenceSource) {
        self.data.add(name, source);
        self.publish_event();
    }

    /// Remove a reference by name. Returns `true` if it existed.
    pub fn remove(&mut self, name: &str) -> bool {
        let removed = self.data.remove(name);
        if removed {
            self.publish_event();
        }
        removed
    }

    /// Get a reference source by name.
    #[must_use]
    pub fn get(&self, name: &str) -> Option<&ReferenceSource> {
        self.data.get(name)
    }

    /// List all registered (name, source) pairs, sorted by name.
    #[must_use]
    pub fn list(&self) -> Vec<(&str, &ReferenceSource)> {
        self.data.list()
    }

    /// Look up materialized references, filtered to those with descriptions
    /// and excluding hidden sources.
    #[must_use]
    pub fn lookup(&self) -> Vec<ReferenceSummary> {
        self.data
            .list()
            .into_iter()
            .filter(|(_, source)| !source.is_hidden())
            .filter(|(_, source)| source.description().is_some())
            .map(|(name, source)| ReferenceSummary {
                name: name.to_string(),
                path: source.path_hint().to_string(),
                description: source.description().map(String::from),
            })
            .collect()
    }

    /// Look up all references (including those without descriptions),
    /// still excluding hidden sources.
    #[must_use]
    pub fn lookup_all(&self) -> Vec<ReferenceSummary> {
        self.data
            .list()
            .into_iter()
            .filter(|(_, source)| !source.is_hidden())
            .map(|(name, source)| ReferenceSummary {
                name: name.to_string(),
                path: source.path_hint().to_string(),
                description: source.description().map(String::from),
            })
            .collect()
    }

    /// Materialize a single git reference by cloning/fetching into `cache_root`.
    ///
    /// Parses the repository URL from the named source, clones into `cache_root`
    /// using [`RepositoryService`](crate::repository::RepositoryService), and
    /// replaces the source with a local reference pointing to the cached location.
    pub fn materialize_git_source(
        &mut self,
        name: &str,
        cache_root: &Path,
    ) -> Result<(), ReferenceError> {
        self.data.materialize_one(name, cache_root)?;
        self.publish_event();
        Ok(())
    }

    /// Apply a scoped mutation via an editor, then finalize.
    ///
    /// Ported from: `packages/core/src/reference.ts` — `Editor` transform pattern
    pub fn transform<F, R>(&mut self, f: F) -> R
    where
        F: FnOnce(&mut ReferenceEditor<'_>) -> R,
    {
        let mut editor = ReferenceEditor::new(&mut self.data);
        f(&mut editor)
    }

    fn publish_event(&self) {
        if let Some(ref bus) = self.event_bus {
            let _ = bus.publish(GlobalEvent::new(serde_json::json!({
                "type": reference_event::UPDATED,
            })));
        }
    }
}

// ══════════════════════════════════════════════════════════════════════════════
// Reference Events
// ══════════════════════════════════════════════════════════════════════════════

/// Event type constants for reference changes.
///
/// # Source
/// `packages/core/src/reference.ts` lines 30–32.
pub mod reference_event {
    /// Published when the reference list is updated.
    pub const UPDATED: &str = "reference.updated";
}

// ══════════════════════════════════════════════════════════════════════════════
// Reference Guidance — system context rendering
// ══════════════════════════════════════════════════════════════════════════════

/// Summary of a reference for system context rendering.
///
/// # Source
/// `packages/core/src/reference/guidance.ts` lines 7–12.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ReferenceSummary {
    /// Reference name.
    pub name: String,

    /// Absolute path where the reference is materialized.
    pub path: String,

    /// Optional description — references without descriptions are excluded from guidance.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
}

/// Render reference summaries into the system context format.
///
/// # Source
/// `packages/core/src/reference/guidance.ts` lines 14–26.
pub fn render_reference_guidance(references: &[ReferenceSummary]) -> String {
    if references.is_empty() {
        return String::new();
    }

    let mut lines = vec![
        "Project references provide additional directories that can be accessed when relevant.".to_string(),
        "<available_references>".to_string(),
    ];

    for r in references {
        lines.push("  <reference>".to_string());
        lines.push(format!("    <name>{}</name>", r.name));
        lines.push(format!("    <path>{}</path>", r.path));
        if let Some(ref desc) = r.description {
            lines.push(format!("    <description>{desc}</description>"));
        }
        lines.push("  </reference>".to_string());
    }

    lines.push("</available_references>".to_string());
    lines.join("\n")
}

/// Generate the update message when references change.
///
/// # Source
/// `packages/core/src/reference/guidance.ts` lines 57–61.
#[must_use]
pub fn reference_update_message(current: &[ReferenceSummary]) -> String {
    let mut lines = vec![
        "The available project references have changed. This list supersedes the previous reference list."
            .to_string(),
        render_reference_guidance(current),
    ];
    lines.join("\n")
}

/// Generate guidance text for LLM context from a list of references.
///
/// Produces a compact human-readable list suitable for injecting into
/// the system prompt or other LLM-facing context.
///
/// # Source
/// `packages/core/src/reference/guidance.ts` — referenceLookupText equivalent.
#[must_use]
pub fn reference_lookup_text(references: &[ReferenceSummary]) -> String {
    if references.is_empty() {
        return String::new();
    }

    let mut lines = vec![
        "The following project references are available:".to_string(),
    ];

    for r in references {
        match &r.description {
            Some(desc) => {
                lines.push(format!("- {} ({}) — {}", r.name, r.path, desc));
            }
            None => {
                lines.push(format!("- {} ({})", r.name, r.path));
            }
        }
    }

    lines.join("\n")
}

/// Message shown when references are removed.
///
/// # Source
/// `packages/core/src/reference/guidance.ts` line 62.
pub const REFERENCE_REMOVED_MESSAGE: &str =
    "Project reference guidance is no longer available. Do not use previously listed references.";

// ══════════════════════════════════════════════════════════════════════════════
// ReferenceGuidance — SystemContext integration
// ══════════════════════════════════════════════════════════════════════════════

/// System context source for reference guidance.
///
/// Registers as `core/reference-guidance` and provides baseline, update, and
/// removal renderers that keep LLM system prompts aware of available project
/// references.
///
/// # Source
/// Ported from `packages/core/src/reference/guidance.ts` — `ReferenceGuidance.Service`.
#[derive(Debug, Clone)]
pub struct ReferenceGuidanceSource {
    service: ReferenceService,
    key: SystemContextKey,
}

impl ReferenceGuidanceSource {
    /// Create a new reference guidance source wrapping the given service.
    #[must_use]
    pub fn new(service: ReferenceService) -> Self {
        Self {
            service,
            key: SystemContextKey::new("core/reference-guidance")
                .expect("hardcoded valid system context key"),
        }
    }
}

impl SystemContextSource for ReferenceGuidanceSource {
    fn key(&self) -> &SystemContextKey {
        &self.key
    }

    fn load(&self) -> Result<serde_json::Value, Box<dyn std::error::Error + Send + Sync>> {
        let references = self.service.lookup();
        let json = serde_json::to_value(references)?;
        Ok(json)
    }

    fn baseline(&self, data: &serde_json::Value) -> String {
        let references: Vec<ReferenceSummary> =
            serde_json::from_value(data.clone()).unwrap_or_default();
        render_reference_guidance(&references)
    }

    fn update(&self, data: &serde_json::Value) -> String {
        let references: Vec<ReferenceSummary> =
            serde_json::from_value(data.clone()).unwrap_or_default();
        reference_update_message(&references)
    }

    fn removed(&self) -> String {
        REFERENCE_REMOVED_MESSAGE.to_string()
    }
}

/// Service for reference guidance system context integration.
///
/// Wraps a [`ReferenceService`] and exposes a [`ReferenceGuidanceSource`] that
/// can be registered with a [`SystemContext`](crate::system_context::SystemContext).
///
/// # Source
/// Ported from `packages/core/src/reference/guidance.ts` — `ReferenceGuidance.Service`.
#[derive(Debug, Clone)]
pub struct ReferenceGuidanceService {
    source: ReferenceGuidanceSource,
}

impl ReferenceGuidanceService {
    /// Create a new reference guidance service.
    #[must_use]
    pub fn new(service: ReferenceService) -> Self {
        Self {
            source: ReferenceGuidanceSource::new(service),
        }
    }

    /// Get the system context source for registration.
    #[must_use]
    pub fn source(&self) -> &ReferenceGuidanceSource {
        &self.source
    }

    /// Consume the service and return the inner source.
    #[must_use]
    pub fn into_source(self) -> ReferenceGuidanceSource {
        self.source
    }
}

// ══════════════════════════════════════════════════════════════════════════════
// Tests
// ══════════════════════════════════════════════════════════════════════════════

#[cfg(test)]
mod tests {
    use super::*;

    // ── LocalReferenceSource ───────────────────────────────────────

    #[test]
    fn test_local_source_new() {
        let src = LocalReferenceSource::new("/home/user/docs");
        assert_eq!(src.source_type, "local");
        assert_eq!(src.path, "/home/user/docs");
        assert!(!src.hidden);
    }

    #[test]
    fn test_local_source_with_description() {
        let src = LocalReferenceSource {
            source_type: "local".into(),
            path: "/docs".into(),
            description: Some("Project documentation".into()),
            hidden: true,
        };
        let json = serde_json::to_string(&src).expect("serialize");
        assert!(json.contains("Project documentation"));
        assert!(json.contains(r#""hidden":true"#));
    }

    #[test]
    fn test_local_source_serde() {
        let src = LocalReferenceSource::new("/test");
        let json = serde_json::to_string(&src).expect("serialize");
        let parsed: LocalReferenceSource = serde_json::from_str(&json).expect("deserialize");
        assert_eq!(parsed.path, "/test");
        assert_eq!(parsed.source_type, "local");
    }

    // ── GitReferenceSource ─────────────────────────────────────────

    #[test]
    fn test_git_source_new() {
        let src = GitReferenceSource::new("owner/repo");
        assert_eq!(src.source_type, "git");
        assert_eq!(src.repository, "owner/repo");
        assert!(src.branch.is_none());
    }

    #[test]
    fn test_git_source_with_branch() {
        let src = GitReferenceSource {
            source_type: "git".into(),
            repository: "owner/repo".into(),
            branch: Some("main".into()),
            description: Some("Main repo".into()),
            hidden: false,
        };
        let json = serde_json::to_string(&src).expect("serialize");
        assert!(json.contains(r#""branch":"main""#));
    }

    // ── ReferenceSource ────────────────────────────────────────────

    #[test]
    fn test_reference_source_local_tagged_union() {
        let src = ReferenceSource::Local(LocalReferenceSource::new("/tmp"));
        let json = serde_json::to_string(&src).expect("serialize");
        assert!(json.contains(r#""type":"local""#));
        let parsed: ReferenceSource = serde_json::from_str(&json).expect("deserialize");
        assert!(parsed.is_local());
    }

    #[test]
    fn test_reference_source_git_tagged_union() {
        let src = ReferenceSource::Git(GitReferenceSource::new("org/repo"));
        let json = serde_json::to_string(&src).expect("serialize");
        assert!(json.contains(r#""type":"git""#));
        assert!(json.contains(r#""repository":"org/repo""#));
        let parsed: ReferenceSource = serde_json::from_str(&json).expect("deserialize");
        assert!(parsed.is_git());
    }

    #[test]
    fn test_reference_source_is_local_is_git() {
        let local = ReferenceSource::Local(LocalReferenceSource::new("/x"));
        let git = ReferenceSource::Git(GitReferenceSource::new("a/b"));
        assert!(local.is_local());
        assert!(!local.is_git());
        assert!(git.is_git());
        assert!(!git.is_local());
    }

    // ── ReferenceInfo ──────────────────────────────────────────────

    #[test]
    fn test_reference_info_serde() {
        let info = ReferenceInfo {
            name: "docs".into(),
            path: "/home/user/docs".into(),
            description: Some("Documentation".into()),
            hidden: false,
            source: ReferenceSource::Local(LocalReferenceSource::new("/home/user/docs")),
        };
        let json = serde_json::to_string(&info).expect("serialize");
        let parsed: ReferenceInfo = serde_json::from_str(&json).expect("deserialize");
        assert_eq!(parsed.name, "docs");
        assert_eq!(parsed.description.as_deref(), Some("Documentation"));
    }

    // ── ReferenceData ──────────────────────────────────────────────

    #[test]
    fn test_reference_data_add_and_get() {
        let mut data = ReferenceData::default();
        data.add("docs", ReferenceSource::Local(LocalReferenceSource::new("/docs")));
        assert!(data.get("docs").is_some());
        assert!(data.get("missing").is_none());
    }

    #[test]
    fn test_reference_data_remove() {
        let mut data = ReferenceData::default();
        data.add("temp", ReferenceSource::Git(GitReferenceSource::new("x/y")));
        assert!(data.remove("temp"));
        assert!(!data.remove("temp"));
        assert!(data.get("temp").is_none());
    }

    #[test]
    fn test_reference_data_list_sorted() {
        let mut data = ReferenceData::default();
        data.add("z", ReferenceSource::Local(LocalReferenceSource::new("/z")));
        data.add("a", ReferenceSource::Local(LocalReferenceSource::new("/a")));
        let list = data.list();
        assert_eq!(list[0].0, "a");
        assert_eq!(list[1].0, "z");
    }

    // ── ReferenceSummary / render_reference_guidance ───────────────

    #[test]
    fn test_render_reference_guidance_empty() {
        assert_eq!(render_reference_guidance(&[]), "");
    }

    #[test]
    fn test_render_reference_guidance_with_entries() {
        let entries = vec![
            ReferenceSummary {
                name: "docs".into(),
                path: "/docs".into(),
                description: Some("API docs".into()),
            },
            ReferenceSummary {
                name: "lib".into(),
                path: "/lib".into(),
                description: None,
            },
        ];
        let rendered = render_reference_guidance(&entries);
        assert!(rendered.contains("<available_references>"));
        assert!(rendered.contains("<name>docs</name>"));
        assert!(rendered.contains("<name>lib</name>"));
        assert!(rendered.contains("API docs"));
        assert!(rendered.contains("</available_references>"));
    }

    #[test]
    fn test_render_reference_guidance_no_description() {
        let entries = vec![ReferenceSummary {
            name: "lib".into(),
            path: "/lib".into(),
            description: None,
        }];
        let rendered = render_reference_guidance(&entries);
        // Should NOT contain a description tag
        assert!(!rendered.contains("<description>"));
    }

    #[test]
    fn test_reference_update_message() {
        let entries = vec![ReferenceSummary {
            name: "new-ref".into(),
            path: "/new".into(),
            description: Some("New".into()),
        }];
        let msg = reference_update_message(&entries);
        assert!(msg.contains("supersedes the previous reference list"));
        assert!(msg.contains("new-ref"));
    }

    #[test]
    fn test_reference_removed_message_exists() {
        assert!(REFERENCE_REMOVED_MESSAGE.contains("no longer available"));
    }

    // ── ReferenceSource helpers ────────────────────────────────────

    #[test]
    fn test_reference_source_description() {
        let local = ReferenceSource::Local(LocalReferenceSource {
            source_type: "local".into(),
            path: "/x".into(),
            description: Some("desc".into()),
            hidden: false,
        });
        assert_eq!(local.description(), Some("desc"));

        let git = ReferenceSource::Git(GitReferenceSource::new("a/b"));
        assert_eq!(git.description(), None);
    }

    #[test]
    fn test_reference_source_is_hidden() {
        let hidden_src = ReferenceSource::Local(LocalReferenceSource {
            source_type: "local".into(),
            path: "/x".into(),
            description: None,
            hidden: true,
        });
        assert!(hidden_src.is_hidden());

        let visible = ReferenceSource::Local(LocalReferenceSource::new("/y"));
        assert!(!visible.is_hidden());
    }

    #[test]
    fn test_reference_source_path_hint() {
        let local = ReferenceSource::Local(LocalReferenceSource::new("/home/user"));
        assert_eq!(local.path_hint(), "/home/user");

        let git = ReferenceSource::Git(GitReferenceSource::new("owner/repo"));
        assert_eq!(git.path_hint(), "owner/repo");
    }

    // ── ReferenceService ───────────────────────────────────────────

    #[test]
    fn test_reference_service_lookup() {
        let mut svc = ReferenceService::new();
        svc.add(
            "docs",
            ReferenceSource::Local(LocalReferenceSource {
                source_type: "local".into(),
                path: "/docs".into(),
                description: Some("API documentation".into()),
                hidden: false,
            }),
        );
        svc.add(
            "lib",
            ReferenceSource::Local(LocalReferenceSource {
                source_type: "local".into(),
                path: "/lib".into(),
                description: Some("Core library".into()),
                hidden: false,
            }),
        );

        let results = svc.lookup();
        assert_eq!(results.len(), 2);
        assert!(results.iter().any(|r| r.name == "docs" && r.path == "/docs"));
        assert!(results.iter().any(|r| r.name == "lib" && r.path == "/lib"));
    }

    #[test]
    fn test_reference_service_lookup_filters_empty_description() {
        let mut svc = ReferenceService::new();
        svc.add(
            "with-desc",
            ReferenceSource::Local(LocalReferenceSource {
                source_type: "local".into(),
                path: "/a".into(),
                description: Some("Has description".into()),
                hidden: false,
            }),
        );
        svc.add(
            "no-desc",
            ReferenceSource::Local(LocalReferenceSource {
                source_type: "local".into(),
                path: "/b".into(),
                description: None,
                hidden: false,
            }),
        );

        // lookup() filters to only those with descriptions
        let described = svc.lookup();
        assert_eq!(described.len(), 1);
        assert_eq!(described[0].name, "with-desc");

        // lookup_all() includes everything (except hidden)
        let all = svc.lookup_all();
        assert_eq!(all.len(), 2);
        let names: Vec<&str> = all.iter().map(|r| r.name.as_str()).collect();
        assert!(names.contains(&"with-desc"));
        assert!(names.contains(&"no-desc"));
    }

    #[test]
    fn test_reference_service_add_remove_list() {
        let mut svc = ReferenceService::new();
        svc.add("a", ReferenceSource::Local(LocalReferenceSource::new("/a")));
        svc.add("b", ReferenceSource::Git(GitReferenceSource::new("org/b")));
        svc.add("c", ReferenceSource::Local(LocalReferenceSource::new("/c")));

        assert_eq!(svc.list().len(), 3);

        // Remove the middle one
        assert!(svc.remove("b"));
        assert!(!svc.remove("b")); // already gone

        let remaining = svc.list();
        assert_eq!(remaining.len(), 2);
        assert_eq!(remaining[0].0, "a");
        assert_eq!(remaining[1].0, "c");
    }

    // ── Serialization edge cases ───────────────────────────────────

    #[test]
    fn test_local_source_hidden_field() {
        // hidden=true should appear in serialized JSON
        let src = LocalReferenceSource {
            source_type: "local".into(),
            path: "/secret".into(),
            description: None,
            hidden: true,
        };
        let json = serde_json::to_string(&src).expect("serialize");
        assert!(json.contains(r#""hidden":true"#));

        // default hidden=false should be omitted
        let default_src = LocalReferenceSource::new("/public");
        let json2 = serde_json::to_string(&default_src).expect("serialize");
        assert!(!json2.contains("hidden"));
    }

    #[test]
    fn test_git_source_complete() {
        let src = GitReferenceSource {
            source_type: "git".into(),
            repository: "https://github.com/owner/repo.git".into(),
            branch: Some("develop".into()),
            description: Some("Full-featured git reference".into()),
            hidden: false,
        };
        let json = serde_json::to_string(&src).expect("serialize");
        let parsed: GitReferenceSource = serde_json::from_str(&json).expect("deserialize");

        assert_eq!(parsed.source_type, "git");
        assert_eq!(parsed.repository, "https://github.com/owner/repo.git");
        assert_eq!(parsed.branch.as_deref(), Some("develop"));
        assert_eq!(parsed.description.as_deref(), Some("Full-featured git reference"));
        assert!(!parsed.hidden);
    }

    #[test]
    fn test_reference_info_comprehensive() {
        let info = ReferenceInfo {
            name: "my-docs".into(),
            path: "/home/user/project/docs".into(),
            description: Some("Project documentation directory".into()),
            hidden: false,
            source: ReferenceSource::Local(LocalReferenceSource {
                source_type: "local".into(),
                path: "/home/user/project/docs".into(),
                description: Some("Project documentation directory".into()),
                hidden: false,
            }),
        };

        let json = serde_json::to_string(&info).expect("serialize");
        let parsed: ReferenceInfo = serde_json::from_str(&json).expect("deserialize");

        assert_eq!(parsed.name, "my-docs");
        assert_eq!(parsed.path, "/home/user/project/docs");
        assert_eq!(
            parsed.description.as_deref(),
            Some("Project documentation directory")
        );
        assert!(!parsed.hidden);
        assert!(parsed.source.is_local());
    }

    #[test]
    fn test_reference_source_tagged_enum_deserialization() {
        // Local JSON with type="local"
        let local_json =
            r#"{"type":"local","path":"/tmp/test","description":null,"hidden":false}"#;
        let parsed: ReferenceSource =
            serde_json::from_str(local_json).expect("deserialize local");
        assert!(parsed.is_local());
        assert!(!parsed.is_git());
        if let ReferenceSource::Local(l) = &parsed {
            assert_eq!(l.path, "/tmp/test");
        } else {
            panic!("expected Local variant");
        }

        // Git JSON with type="git"
        let git_json = r#"{"type":"git","repository":"org/repo","branch":"main","description":"a repo","hidden":false}"#;
        let parsed: ReferenceSource =
            serde_json::from_str(git_json).expect("deserialize git");
        assert!(parsed.is_git());
        assert!(!parsed.is_local());
        if let ReferenceSource::Git(g) = &parsed {
            assert_eq!(g.repository, "org/repo");
            assert_eq!(g.branch.as_deref(), Some("main"));
        } else {
            panic!("expected Git variant");
        }
    }

    #[test]
    fn test_render_reference_guidance_empty_list() {
        // Edge case: empty slice produces empty string
        let result = render_reference_guidance(&[]);
        assert!(result.is_empty());
    }

    #[test]
    fn test_render_guidance_single_entry_without_description() {
        let entries = vec![ReferenceSummary {
            name: "bare-ref".into(),
            path: "/bare".into(),
            description: None,
        }];
        let rendered = render_reference_guidance(&entries);
        // Should include the reference but NOT a description tag
        assert!(rendered.contains("<name>bare-ref</name>"));
        assert!(rendered.contains("<path>/bare</path>"));
        assert!(!rendered.contains("<description>"));
    }

    #[test]
    fn test_reference_update_message_updated() {
        let entries = vec![ReferenceSummary {
            name: "proj".into(),
            path: "/proj".into(),
            description: Some("Main project".into()),
        }];
        let msg = reference_update_message(&entries);
        assert!(msg.contains("The available project references have changed."));
        assert!(msg.contains("supersedes the previous reference list"));
        assert!(msg.contains("<available_references>"));
        assert!(msg.contains("<name>proj</name>"));
        assert!(msg.contains("</available_references>"));
    }

    // ── reference_lookup_text ──────────────────────────────────────

    #[test]
    fn test_reference_lookup_text_empty() {
        let result = reference_lookup_text(&[]);
        assert!(result.is_empty());
    }

    #[test]
    fn test_reference_lookup_text_with_entries() {
        let entries = vec![
            ReferenceSummary {
                name: "api".into(),
                path: "/api".into(),
                description: Some("REST API".into()),
            },
            ReferenceSummary {
                name: "bare".into(),
                path: "/bare".into(),
                description: None,
            },
        ];
        let text = reference_lookup_text(&entries);
        assert!(text.contains("The following project references are available:"));
        assert!(text.contains("- api (/api)"));
        assert!(text.contains("REST API"));
        assert!(text.contains("- bare (/bare)"));
    }

    // ── ReferenceGuidanceSource ────────────────────────────────────

    use crate::system_context::{SystemContext, SystemContextSource};

    #[test]
    fn test_reference_guidance_source_key() {
        let svc = ReferenceService::new();
        let source = ReferenceGuidanceSource::new(svc);
        assert_eq!(source.key().as_str(), "core/reference-guidance");
    }

    #[test]
    fn test_reference_guidance_source_load_empty() {
        let svc = ReferenceService::new();
        let source = ReferenceGuidanceSource::new(svc);
        let data = source.load().expect("load");
        let refs: Vec<ReferenceSummary> = serde_json::from_value(data).expect("deserialize");
        assert!(refs.is_empty());
    }

    #[test]
    fn test_reference_guidance_source_load_with_references() {
        let mut svc = ReferenceService::new();
        svc.add(
            "docs",
            ReferenceSource::Local(LocalReferenceSource {
                source_type: "local".into(),
                path: "/docs".into(),
                description: Some("API documentation".into()),
                hidden: false,
            }),
        );
        svc.add(
            "hidden",
            ReferenceSource::Local(LocalReferenceSource {
                source_type: "local".into(),
                path: "/hidden".into(),
                description: Some("Hidden ref".into()),
                hidden: true,
            }),
        );

        let source = ReferenceGuidanceSource::new(svc);
        let data = source.load().expect("load");
        let refs: Vec<ReferenceSummary> = serde_json::from_value(data).expect("deserialize");
        // hidden refs are filtered out by lookup()
        assert_eq!(refs.len(), 1);
        assert_eq!(refs[0].name, "docs");
    }

    #[test]
    fn test_reference_guidance_source_baseline_empty() {
        let svc = ReferenceService::new();
        let source = ReferenceGuidanceSource::new(svc);
        let data = serde_json::json!([]);
        let text = source.baseline(&data);
        // empty references → empty string from render_reference_guidance
        assert!(text.is_empty());
    }

    #[test]
    fn test_reference_guidance_source_baseline_with_references() {
        let data = serde_json::json!([
            {"name": "docs", "path": "/docs", "description": "API docs"},
            {"name": "lib", "path": "/lib", "description": "Core library"}
        ]);
        let svc = ReferenceService::new();
        let source = ReferenceGuidanceSource::new(svc);
        let text = source.baseline(&data);
        assert!(text.contains("<available_references>"));
        assert!(text.contains("<name>docs</name>"));
        assert!(text.contains("<name>lib</name>"));
        assert!(text.contains("</available_references>"));
    }

    #[test]
    fn test_reference_guidance_source_update() {
        let data = serde_json::json!([
            {"name": "new-ref", "path": "/new", "description": "New reference"}
        ]);
        let svc = ReferenceService::new();
        let source = ReferenceGuidanceSource::new(svc);
        let text = source.update(&data);
        assert!(text.contains("supersedes the previous reference list"));
        assert!(text.contains("new-ref"));
    }

    #[test]
    fn test_reference_guidance_source_removed() {
        let svc = ReferenceService::new();
        let source = ReferenceGuidanceSource::new(svc);
        let text = source.removed();
        assert_eq!(text, REFERENCE_REMOVED_MESSAGE);
    }

    #[test]
    fn test_reference_guidance_source_is_send_sync() {
        fn assert_send_sync<T: Send + Sync>() {}
        assert_send_sync::<ReferenceGuidanceSource>();
    }

    #[test]
    fn test_reference_guidance_source_clone() {
        let mut svc = ReferenceService::new();
        svc.add(
            "docs",
            ReferenceSource::Local(LocalReferenceSource::new("/docs")),
        );
        let source = ReferenceGuidanceSource::new(svc);
        let cloned = source.clone();
        assert_eq!(source.key(), cloned.key());
    }

    // ── ReferenceGuidanceService ───────────────────────────────────

    #[test]
    fn test_reference_guidance_service_new() {
        let svc = ReferenceService::new();
        let guidance = ReferenceGuidanceService::new(svc);
        assert_eq!(guidance.source().key().as_str(), "core/reference-guidance");
    }

    #[test]
    fn test_reference_guidance_service_into_source() {
        let svc = ReferenceService::new();
        let guidance = ReferenceGuidanceService::new(svc);
        let source = guidance.into_source();
        assert_eq!(source.key().as_str(), "core/reference-guidance");
    }

    #[test]
    fn test_reference_guidance_service_with_system_context() {
        let mut svc = ReferenceService::new();
        svc.add(
            "docs",
            ReferenceSource::Local(LocalReferenceSource {
                source_type: "local".into(),
                path: "/docs".into(),
                description: Some("API documentation".into()),
                hidden: false,
            }),
        );

        let guidance = ReferenceGuidanceService::new(svc);
        let ctx = SystemContext::make(guidance.into_source());
        assert_eq!(ctx.len(), 1);

        let gen = ctx.initialize().expect("initialize");
        assert!(gen.baseline.contains("available_references"));
        assert!(gen.baseline.contains("<name>docs</name>"));
        assert!(gen.baseline.contains("/docs"));
    }

    #[test]
    fn test_reference_guidance_service_reconcile_unchanged() {
        let mut svc = ReferenceService::new();
        svc.add(
            "docs",
            ReferenceSource::Local(LocalReferenceSource {
                source_type: "local".into(),
                path: "/docs".into(),
                description: Some("API docs".into()),
                hidden: false,
            }),
        );

        let guidance = ReferenceGuidanceService::new(svc);
        let ctx = SystemContext::make(guidance.into_source());
        let gen = ctx.initialize().expect("initialize");

        // Same data → Unchanged
        let result = ctx.reconcile(&gen.snapshot).expect("reconcile");
        assert!(matches!(
            result,
            crate::system_context::ReconcileResult::Unchanged
        ));
    }

    #[test]
    fn test_reference_guidance_service_reconcile_removed() {
        let mut svc = ReferenceService::new();
        svc.add(
            "docs",
            ReferenceSource::Local(LocalReferenceSource {
                source_type: "local".into(),
                path: "/docs".into(),
                description: Some("API docs".into()),
                hidden: false,
            }),
        );

        let guidance = ReferenceGuidanceService::new(svc);
        let ctx = SystemContext::make(guidance.into_source());
        let gen = ctx.initialize().expect("initialize");

        // Now remove the reference and create a new service with empty data
        let empty_svc = ReferenceService::new();
        let guidance2 = ReferenceGuidanceService::new(empty_svc);
        let ctx2 = SystemContext::make(guidance2.into_source());

        let result = ctx2.reconcile(&gen.snapshot).expect("reconcile");
        match result {
            crate::system_context::ReconcileResult::Updated { text, .. } => {
                assert!(text.contains("no longer available"));
            }
            other => panic!("expected Updated with removal text, got: {other:?}"),
        }
    }

    // ── ReferenceError ─────────────────────────────────────────────

    #[test]
    fn test_reference_error_not_found_display() {
        let err = ReferenceError::NotFound {
            name: "my-ref".into(),
        };
        assert!(err.to_string().contains("my-ref"));
        assert!(err.to_string().contains("not found"));
    }

    #[test]
    fn test_reference_error_source_not_git_display() {
        let err = ReferenceError::SourceNotGit {
            name: "local-ref".into(),
        };
        assert!(err.to_string().contains("local-ref"));
        assert!(err.to_string().contains("not a git reference"));
    }

    #[test]
    fn test_reference_error_git_clone_failed_display() {
        let err = ReferenceError::GitCloneFailed {
            repository: "owner/repo".into(),
            message: "network timeout".into(),
        };
        assert!(err.to_string().contains("owner/repo"));
        assert!(err.to_string().contains("network timeout"));
    }

    #[test]
    fn test_reference_error_clone_is_clone() {
        let err = ReferenceError::GitCloneFailed {
            repository: "a/b".into(),
            message: "boom".into(),
        };
        let cloned = err.clone();
        assert_eq!(err.to_string(), cloned.to_string());
    }

    // ── ReferenceService::materialize_git_source ───────────────────

    #[test]
    fn test_materialize_git_source_not_found() {
        let mut svc = ReferenceService::new();
        let result = svc.materialize_git_source("nonexistent", Path::new("/tmp"));
        assert!(matches!(
            result,
            Err(ReferenceError::NotFound { ref name } if name == "nonexistent")
        ));
    }

    #[test]
    fn test_materialize_git_source_not_git() {
        let mut svc = ReferenceService::new();
        svc.add(
            "docs",
            ReferenceSource::Local(LocalReferenceSource::new("/docs")),
        );
        let result = svc.materialize_git_source("docs", Path::new("/tmp"));
        assert!(matches!(
            result,
            Err(ReferenceError::SourceNotGit { ref name } if name == "docs")
        ));
    }

    #[test]
    fn test_materialize_git_source_invalid_repo() {
        let mut svc = ReferenceService::new();
        svc.add("bad", ReferenceSource::Git(GitReferenceSource::new("")));
        let result = svc.materialize_git_source("bad", Path::new("/tmp"));
        assert!(result.is_err());
        assert!(matches!(
            result.unwrap_err(),
            ReferenceError::GitCloneFailed { .. }
        ));
    }

    // ── ReferenceData::materialize ─────────────────────────────────

    #[test]
    fn test_reference_data_materialize_empty() {
        let mut data = ReferenceData::default();
        let results = data.materialize(Path::new("/tmp"));
        assert!(results.is_empty());
    }

    #[test]
    fn test_reference_data_materialize_mixed_sources() {
        let mut data = ReferenceData::default();
        data.add(
            "local-docs",
            ReferenceSource::Local(LocalReferenceSource::new("/docs")),
        );
        data.add(
            "local-lib",
            ReferenceSource::Local(LocalReferenceSource::new("/lib")),
        );

        let results = data.materialize(Path::new("/tmp"));
        assert_eq!(results.len(), 2);

        // Both should fail with SourceNotGit since they're local references
        for result in &results {
            assert!(matches!(result, Err(ReferenceError::SourceNotGit { .. })));
        }
    }

    #[test]
    fn test_reference_data_materialize_not_found() {
        let mut data = ReferenceData::default();
        data.add(
            "existing",
            ReferenceSource::Local(LocalReferenceSource::new("/x")),
        );
        // Manually remove to simulate a race condition
        data.remove("existing");

        let results = data.materialize(Path::new("/tmp"));
        // No sources remain, so empty results
        assert!(results.is_empty());
    }

    #[test]
    fn test_materialize_git_source_invalid_repo_url() {
        let mut svc = ReferenceService::new();
        svc.add(
            "bad-url",
            ReferenceSource::Git(GitReferenceSource::new("not a valid repo")),
        );
        let result = svc.materialize_git_source("bad-url", Path::new("/tmp"));
        assert!(matches!(
            result,
            Err(ReferenceError::GitCloneFailed { ref repository, .. })
                if repository == "not a valid repo"
        ));
    }

    // ── ReferenceEditor ────────────────────────────────────────────

    #[test]
    fn test_reference_editor_new() {
        let mut data = ReferenceData::default();
        let editor = ReferenceEditor::new(&mut data);
        assert!(editor.list_sources().is_empty());
    }

    #[test]
    fn test_reference_editor_set_source() {
        let mut data = ReferenceData::default();
        let mut editor = ReferenceEditor::new(&mut data);
        editor.set_source(
            "docs",
            ReferenceSource::Local(LocalReferenceSource::new("/docs")),
        );
        assert!(editor.get_source("docs").is_some());
        assert_eq!(editor.list_sources().len(), 1);
    }

    #[test]
    fn test_reference_editor_remove_source() {
        let mut data = ReferenceData::default();
        data.add(
            "docs",
            ReferenceSource::Local(LocalReferenceSource::new("/docs")),
        );
        let mut editor = ReferenceEditor::new(&mut data);
        assert!(editor.remove_source("docs"));
        assert!(!editor.remove_source("docs"));
        assert!(editor.get_source("docs").is_none());
    }

    #[test]
    fn test_reference_editor_list_sources_sorted() {
        let mut data = ReferenceData::default();
        data.add(
            "z-ref",
            ReferenceSource::Local(LocalReferenceSource::new("/z")),
        );
        data.add(
            "a-ref",
            ReferenceSource::Local(LocalReferenceSource::new("/a")),
        );
        let editor = ReferenceEditor::new(&mut data);
        let sources = editor.list_sources();
        assert_eq!(sources.len(), 2);
        assert_eq!(sources[0].0, "a-ref");
        assert_eq!(sources[1].0, "z-ref");
    }

    // ── ReferenceService::transform ────────────────────────────────

    #[test]
    fn test_reference_service_transform_returns_value() {
        let mut svc = ReferenceService::new();
        let count = svc.transform(|editor| {
            editor.set_source(
                "docs",
                ReferenceSource::Local(LocalReferenceSource::new("/docs")),
            );
            editor.set_source(
                "lib",
                ReferenceSource::Local(LocalReferenceSource::new("/lib")),
            );
            editor.list_sources().len()
        });
        assert_eq!(count, 2);
    }

    #[test]
    fn test_reference_service_transform_modifies_data() {
        let mut svc = ReferenceService::new();
        svc.transform(|editor| {
            editor.set_source(
                "temp",
                ReferenceSource::Local(LocalReferenceSource::new("/tmp")),
            );
        });
        // Data persists after transform closure returns
        assert!(svc.get("temp").is_some());
    }

    #[test]
    fn test_reference_service_transform_remove_inside_closure() {
        let mut svc = ReferenceService::new();
        svc.add(
            "docs",
            ReferenceSource::Local(LocalReferenceSource::new("/docs")),
        );
        let removed = svc.transform(|editor| editor.remove_source("docs"));
        assert!(removed);
        assert!(svc.get("docs").is_none());
    }
}
