//! Reference/citation types — local and git sources, materialization, guidance.
//!
//! Ported from:
//! - `packages/core/src/reference.ts` — Reference namespace, LocalSource, GitSource, Source union,
//!   Info, Data, Editor, Interface, Service, Event
//! - `packages/core/src/reference/guidance.ts` — ReferenceGuidance namespace, Summary, render, Interface
//!
//! OpenCode commit: 5d0f86606ac30690f79f0a6a9f41a1f49fe95d0b

use serde::{Deserialize, Serialize};

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

/// Message shown when references are removed.
///
/// # Source
/// `packages/core/src/reference/guidance.ts` line 62.
pub const REFERENCE_REMOVED_MESSAGE: &str =
    "Project reference guidance is no longer available. Do not use previously listed references.";

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
}
