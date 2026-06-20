//! Session info types — schema, store interface, and SQL projection helpers.
//!
//! Ported from:
//! - `packages/core/src/session/info.ts` (lines 1–47)
//! - `packages/core/src/session/store.ts` (lines 1–62)
//! - `packages/core/src/session/schema.ts` (lines 1–50)
//! - `packages/core/src/session/sql.ts` (lines 1–178)

use serde::{Deserialize, Serialize};

// ══════════════════════════════════════════════════════════════════════════════
// Session ID
// ══════════════════════════════════════════════════════════════════════════════

/// Session identifier — branded string starting with "ses_".
///
/// # Source
/// `packages/core/src/session/schema.ts` lines 12–24 `ID`.
pub type SessionId = String;

/// Message identifier — branded string starting with "msg_".
///
/// # Source
/// `packages/core/src/session/message-id.ts` lines 7–13 `ID`.
pub type MessageId = String;

/// Part identifier.
///
/// # Source
/// `packages/opencode/src/session/message-v2.ts` line 6 `PartID`.
pub type PartId = String;

// ══════════════════════════════════════════════════════════════════════════════
// Session Info (V2)
// ══════════════════════════════════════════════════════════════════════════════

/// Complete session info — the V2 representation.
///
/// # Source
/// `packages/core/src/session/schema.ts` lines 25–49 `Info`.
/// `packages/core/src/session/info.ts` lines 12–47 `fromRow`.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SessionInfoV2 {
    /// Unique session identifier (ses_ prefix)
    pub id: SessionId,

    /// Optional parent session ID
    #[serde(skip_serializing_if = "Option::is_none")]
    pub parent_id: Option<SessionId>,

    /// Project identifier
    pub project_id: String,

    /// Agent name used for this session
    #[serde(skip_serializing_if = "Option::is_none")]
    pub agent: Option<String>,

    /// Model selection for this session
    #[serde(skip_serializing_if = "Option::is_none")]
    pub model: Option<ModelRef>,

    /// Accumulated cost in USD
    #[serde(default)]
    pub cost: f64,

    /// Token usage across all turns
    #[serde(default)]
    pub tokens: TokenUsage,

    /// Session timestamps
    pub time: SessionTime,

    /// Session title
    pub title: String,

    /// File-system location (directory + workspace)
    pub location: LocationRef,

    /// Sub-path within the workspace directory
    #[serde(skip_serializing_if = "Option::is_none")]
    pub subpath: Option<String>,

    /// Session slug for URL-safe reference
    #[serde(default)]
    pub slug: String,

    /// Share URL if session is shared
    #[serde(skip_serializing_if = "Option::is_none")]
    pub share_url: Option<String>,

    /// Summary of file changes
    #[serde(skip_serializing_if = "Option::is_none")]
    pub summary: Option<SessionSummaryV2>,

    /// Opaque metadata
    #[serde(skip_serializing_if = "Option::is_none")]
    pub metadata: Option<serde_json::Value>,

    /// Revert information
    #[serde(skip_serializing_if = "Option::is_none")]
    pub revert: Option<RevertInfo>,

    /// Permission ruleset active on this session
    #[serde(skip_serializing_if = "Option::is_none")]
    pub permission: Option<serde_json::Value>,
}

// ── Model Ref ────────────────────────────────────────────────────────────────

/// Cross-reference to a model (provider + model ID).
///
/// # Source
/// `packages/core/src/session/schema.ts` lines 30–31 `ModelV2.Ref`.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ModelRef {
    /// Model identifier
    pub id: String,
    /// Provider identifier
    #[serde(rename = "providerID")]
    pub provider_id: String,
    /// Model variant (default: "default")
    #[serde(skip_serializing_if = "Option::is_none")]
    pub variant: Option<String>,
}

// ── Token Usage ──────────────────────────────────────────────────────────────

/// Token usage breakdown.
///
/// # Source
/// `packages/core/src/session/schema.ts` lines 32–40 `tokens`.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct TokenUsage {
    /// Input (prompt) tokens
    #[serde(default)]
    pub input: u64,
    /// Output (completion) tokens
    #[serde(default)]
    pub output: u64,
    /// Reasoning tokens
    #[serde(default)]
    pub reasoning: u64,
    /// Cache token usage
    #[serde(default)]
    pub cache: CacheUsage,
}

/// Cache token breakdown.
///
/// # Source
/// `packages/core/src/session/schema.ts` lines 36–39 `cache`.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct CacheUsage {
    #[serde(default)]
    pub read: u64,
    #[serde(default)]
    pub write: u64,
}

// ── Session Time ─────────────────────────────────────────────────────────────

/// Session timestamps (V2).
///
/// # Source
/// `packages/core/src/session/schema.ts` lines 41–45 `time`.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SessionTime {
    /// Creation timestamp (epoch millis)
    pub created: u64,
    /// Last update timestamp (epoch millis)
    pub updated: u64,
    /// Archive timestamp
    #[serde(skip_serializing_if = "Option::is_none")]
    pub archived: Option<u64>,
}

// ── Location Ref ─────────────────────────────────────────────────────────────

/// File-system location reference.
///
/// # Source
/// `packages/core/src/session/schema.ts` line 47 `Location.Ref`.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LocationRef {
    /// Working directory
    pub directory: String,
    /// Optional workspace identifier
    #[serde(skip_serializing_if = "Option::is_none")]
    pub workspace_id: Option<String>,
}

// ── Session Summary (V2) ─────────────────────────────────────────────────────

/// Session file-change summary (V2).
///
/// # Source
/// `packages/core/src/session/sql.ts` lines 38–40 `summary_*` columns.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SessionSummaryV2 {
    /// Lines added
    #[serde(default)]
    pub additions: i64,
    /// Lines deleted
    #[serde(default)]
    pub deletions: i64,
    /// Files changed
    #[serde(default)]
    pub files: i64,
    /// File diffs
    #[serde(skip_serializing_if = "Option::is_none")]
    pub diffs: Option<Vec<FileDiffV2>>,
}

/// A single file diff entry.
///
/// # Source
/// `packages/core/src/session/sql.ts` line 40 `summary_diffs`.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FileDiffV2 {
    pub path: String,
    pub hash: String,
}

// ── Revert Info ──────────────────────────────────────────────────────────────

/// Revert/undo information stored on a session.
///
/// # Source
/// `packages/core/src/session/sql.ts` line 48 `revert` column.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RevertInfo {
    /// The message that triggered the revert
    pub message_id: String,
    /// Optional part that was reverted
    #[serde(skip_serializing_if = "Option::is_none")]
    pub part_id: Option<String>,
    /// Snapshot hash before the revert
    #[serde(skip_serializing_if = "Option::is_none")]
    pub snapshot: Option<String>,
    /// Git diff of the revert
    #[serde(skip_serializing_if = "Option::is_none")]
    pub diff: Option<String>,
}

// ══════════════════════════════════════════════════════════════════════════════
// Session Store Interface (CRUD signatures as traits)
// ══════════════════════════════════════════════════════════════════════════════

/// Parameters for listing sessions.
///
/// # Source
/// `packages/core/src/session/store.ts` lines 13–23 `Interface`.
#[derive(Debug, Clone, Default)]
pub struct ListSessionsParams {
    /// Filter by project ID
    pub project_id: Option<String>,
    /// Filter by workspace ID
    pub workspace_id: Option<String>,
    /// Filter by directory
    pub directory: Option<String>,
    /// Only root sessions (no parent)
    pub roots: Option<bool>,
    /// Search in title
    pub search: Option<String>,
    /// Maximum results
    pub limit: Option<usize>,
}

/// Context query parameters for loading session messages.
///
/// # Source
/// `packages/core/src/session/store.ts` lines 14–23 `context`, `runnerContext`.
#[derive(Debug, Clone, Default)]
pub struct ContextQuery {
    /// Session ID to load context for
    pub session_id: SessionId,
    /// Baseline sequence number for runner context
    pub baseline_seq: Option<u64>,
}

/// Query to find a single message by ID.
///
/// # Source
/// `packages/core/src/session/store.ts` lines 19–23 `message`.
#[derive(Debug, Clone, Default)]
pub struct MessageQuery {
    /// Message ID to look up
    pub message_id: MessageId,
}

/// Result of a single message lookup.
///
/// # Source
/// `packages/core/src/session/store.ts` line 20 `message` return type.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MessageLookupResult {
    /// Session that owns the message
    pub session_id: SessionId,
    /// The message data
    pub message: serde_json::Value,
}

// ══════════════════════════════════════════════════════════════════════════════
// Tests
// ══════════════════════════════════════════════════════════════════════════════

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_session_info_v2_serialization() {
        let info = SessionInfoV2 {
            id: "ses_001".into(),
            parent_id: None,
            project_id: "proj_1".into(),
            agent: Some("build".into()),
            model: Some(ModelRef {
                id: "claude-sonnet-4-20250514".into(),
                provider_id: "anthropic".into(),
                variant: None,
            }),
            cost: 1.25,
            tokens: TokenUsage {
                input: 5000,
                output: 2000,
                reasoning: 1000,
                cache: CacheUsage {
                    read: 500,
                    write: 100,
                },
            },
            time: SessionTime {
                created: 1700000000000,
                updated: 1700000001000,
                archived: None,
            },
            title: "Test Session".into(),
            location: LocationRef {
                directory: "/tmp/test".into(),
                workspace_id: Some("ws_001".into()),
            },
            subpath: None,
            slug: "test-slug".into(),
            share_url: None,
            summary: None,
            metadata: None,
            revert: None,
            permission: None,
        };

        let json = serde_json::to_string(&info).expect("serialization should succeed");
        let parsed: SessionInfoV2 =
            serde_json::from_str(&json).expect("deserialization should succeed");
        assert_eq!(parsed.id, "ses_001");
        assert_eq!(parsed.title, "Test Session");
        assert_eq!(parsed.tokens.input, 5000);
        assert_eq!(parsed.cost, 1.25);
    }

    #[test]
    fn test_model_ref_serialization() {
        let model = ModelRef {
            id: "gpt-5".into(),
            provider_id: "openai".into(),
            variant: Some("default".into()),
        };
        let json = serde_json::to_string(&model).expect("serialization should succeed");
        assert!(json.contains("providerID"));
        assert!(json.contains("gpt-5"));
    }

    #[test]
    fn test_token_usage_default() {
        let usage = TokenUsage::default();
        assert_eq!(usage.input, 0);
        assert_eq!(usage.output, 0);
        assert_eq!(usage.reasoning, 0);
        assert_eq!(usage.cache.read, 0);
        assert_eq!(usage.cache.write, 0);
    }

    #[test]
    fn test_location_ref_optional_workspace() {
        let with_ws = LocationRef {
            directory: "/tmp".into(),
            workspace_id: Some("ws_1".into()),
        };
        let json1 = serde_json::to_string(&with_ws).expect("serialization should succeed");
        assert!(json1.contains("workspace_id"));

        let without_ws = LocationRef {
            directory: "/tmp".into(),
            workspace_id: None,
        };
        let json2 = serde_json::to_string(&without_ws).expect("serialization should succeed");
        assert!(!json2.contains("workspace_id"));
    }

    #[test]
    fn test_session_summary_v2_roundtrip() {
        let summary = SessionSummaryV2 {
            additions: 150,
            deletions: 30,
            files: 5,
            diffs: Some(vec![FileDiffV2 {
                path: "src/main.rs".into(),
                hash: "abc123".into(),
            }]),
        };
        let json = serde_json::to_string(&summary).expect("serialize");
        let parsed: SessionSummaryV2 = serde_json::from_str(&json).expect("deserialize");
        assert_eq!(parsed.additions, 150);
        assert_eq!(parsed.files, 5);
        assert_eq!(parsed.diffs.as_ref().expect("diffs").len(), 1);
    }

    #[test]
    fn test_revert_info_serialization() {
        let revert = RevertInfo {
            message_id: "msg_001".into(),
            part_id: Some("part_001".into()),
            snapshot: Some("snap_abc".into()),
            diff: None,
        };
        let json = serde_json::to_string(&revert).expect("serialize");
        let parsed: RevertInfo = serde_json::from_str(&json).expect("deserialize");
        assert_eq!(parsed.message_id, "msg_001");
        assert_eq!(parsed.snapshot.as_deref(), Some("snap_abc"));
        assert!(parsed.diff.is_none());
    }

    #[test]
    fn test_list_sessions_params_default() {
        let params = ListSessionsParams::default();
        assert!(params.project_id.is_none());
        assert!(params.workspace_id.is_none());
        assert!(params.directory.is_none());
        assert!(params.search.is_none());
        assert!(params.limit.is_none());
    }

    #[test]
    fn test_context_query_with_baseline() {
        let q = ContextQuery {
            session_id: "ses_001".into(),
            baseline_seq: Some(42),
        };
        assert_eq!(q.session_id, "ses_001");
        assert_eq!(q.baseline_seq, Some(42));
    }
}
