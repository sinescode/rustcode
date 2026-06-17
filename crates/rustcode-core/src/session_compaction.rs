//! Session compaction types — input/output, strategy, and related enums.
//!
//! Ported from:
//! - `packages/core/src/session/compaction.ts` (lines 1–247)
//! - `packages/opencode/src/session/compaction.ts` (lines 1–621)

use serde::{Deserialize, Serialize};
use crate::session_info::SessionId;

// ══════════════════════════════════════════════════════════════════════════════
// Compaction Constants
// ══════════════════════════════════════════════════════════════════════════════

/// Default buffer tokens reserved for compaction.
///
/// # Source
/// `packages/core/src/session/compaction.ts` line 23 `DEFAULT_BUFFER`.
pub const DEFAULT_COMPACTION_BUFFER: u64 = 20_000;

/// Default tokens to keep (recent context).
///
/// # Source
/// `packages/core/src/session/compaction.ts` line 24 `DEFAULT_KEEP_TOKENS`.
pub const DEFAULT_KEEP_TOKENS: u64 = 8_000;

/// Max characters for tool output during compaction.
///
/// # Source
/// `packages/core/src/session/compaction.ts` line 25 `TOOL_OUTPUT_MAX_CHARS`.
pub const TOOL_OUTPUT_MAX_CHARS: usize = 2_000;

/// Max tokens for summary output.
///
/// # Source
/// `packages/core/src/session/compaction.ts` line 26 `SUMMARY_OUTPUT_TOKENS`.
pub const SUMMARY_OUTPUT_TOKENS: u64 = 4_096;

/// Minimum tokens pruned before action is taken.
///
/// # Source
/// `packages/opencode/src/session/compaction.ts` line 38 `PRUNE_MINIMUM`.
pub const PRUNE_MINIMUM_TOKENS: u64 = 20_000;

/// Tokens to protect from pruning.
///
/// # Source
/// `packages/opencode/src/session/compaction.ts` line 39 `PRUNE_PROTECT`.
pub const PRUNE_PROTECT_TOKENS: u64 = 40_000;

/// Default number of recent turns to keep.
///
/// # Source
/// `packages/opencode/src/session/compaction.ts` line 43 `DEFAULT_TAIL_TURNS`.
pub const DEFAULT_TAIL_TURNS: u64 = 2;

/// Minimum tokens to preserve in recent context.
///
/// # Source
/// `packages/opencode/src/session/compaction.ts` line 44 `MIN_PRESERVE_RECENT_TOKENS`.
pub const MIN_PRESERVE_RECENT_TOKENS: u64 = 2_000;

/// Maximum tokens to preserve in recent context.
///
/// # Source
/// `packages/opencode/src/session/compaction.ts` line 45 `MAX_PRESERVE_RECENT_TOKENS`.
pub const MAX_PRESERVE_RECENT_TOKENS: u64 = 8_000;

// ══════════════════════════════════════════════════════════════════════════════
// Compaction Settings
// ══════════════════════════════════════════════════════════════════════════════

/// Compaction configuration settings.
///
/// # Source
/// `packages/core/src/session/compaction.ts` lines 58–62 `Settings`.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CompactionSettings {
    /// Whether auto-compaction is enabled
    #[serde(default = "default_auto")]
    pub auto: bool,
    /// Buffer tokens to reserve
    #[serde(default = "default_buffer")]
    pub buffer: u64,
    /// Tokens to keep in recent context
    #[serde(default = "default_tokens")]
    pub tokens: u64,
}

fn default_auto() -> bool {
    true
}
fn default_buffer() -> u64 {
    DEFAULT_COMPACTION_BUFFER
}
fn default_tokens() -> u64 {
    DEFAULT_KEEP_TOKENS
}

impl Default for CompactionSettings {
    fn default() -> Self {
        Self {
            auto: true,
            buffer: DEFAULT_COMPACTION_BUFFER,
            tokens: DEFAULT_KEEP_TOKENS,
        }
    }
}

// ══════════════════════════════════════════════════════════════════════════════
// Compaction Input / Output Types
// ══════════════════════════════════════════════════════════════════════════════

/// Input for processing compaction.
///
/// # Source
/// `packages/opencode/src/session/compaction.ts` lines 146–151 `process` input.
#[derive(Debug, Clone)]
pub struct CompactionProcessInput {
    /// Parent user message ID
    pub parent_id: String,
    /// All messages in the session
    pub messages: serde_json::Value,
    /// Session identifier
    pub session_id: SessionId,
    /// Whether this is auto-triggered
    pub auto: bool,
    /// Whether triggered by overflow
    pub overflow: Option<bool>,
}

/// Input for creating a compaction request.
///
/// # Source
/// `packages/opencode/src/session/compaction.ts` lines 153–159 `create` input.
#[derive(Debug, Clone)]
pub struct CompactionCreateInput {
    pub session_id: SessionId,
    pub agent: String,
    pub model: crate::session_info::ModelRef,
    pub auto: bool,
    pub overflow: Option<bool>,
}

/// Result of compaction processing.
///
/// # Source
/// `packages/opencode/src/session/compaction.ts` line 152 `process` return.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum CompactionResult {
    /// Continue the loop
    Continue,
    /// Stop the loop
    Stop,
}

/// Selected context split — head and recent tail.
///
/// # Source
/// `packages/core/src/session/compaction.ts` lines 136–139 `select` return.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CompactionSelection {
    /// Older context to summarize
    pub head: String,
    /// Recent tail to keep verbatim
    pub recent: String,
}

/// Compaction strategy representation.
///
/// # Source
/// `packages/core/src/session/compaction.ts` — overall compaction flow.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum CompactionStrategy {
    /// System auto-triggered (overflow)
    Auto,
    /// User-initiated manual compaction
    Manual,
}

/// Turn boundary tracking for selecting recent context.
///
/// # Source
/// `packages/opencode/src/session/compaction.ts` lines 46–49 `Turn`.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CompactionTurn {
    pub start: usize,
    pub end: usize,
    pub id: String,
}

/// Tail tracking for recent context selection.
///
/// # Source
/// `packages/opencode/src/session/compaction.ts` lines 51–54 `Tail`.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CompactionTail {
    pub start: usize,
    pub id: String,
}

/// A completed compaction — user index, assistant index, summary text.
///
/// # Source
/// `packages/opencode/src/session/compaction.ts` lines 56–60 `CompletedCompaction`.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CompletedCompaction {
    pub user_index: usize,
    pub assistant_index: usize,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub summary: Option<String>,
}

// ══════════════════════════════════════════════════════════════════════════════
// Compaction Event
// ══════════════════════════════════════════════════════════════════════════════

/// Payload for the "session.compacted" event.
///
/// # Source
/// `packages/opencode/src/session/compaction.ts` lines 29–36 `Event.Compacted`.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CompactionCompactedEvent {
    pub session_id: SessionId,
}

// ══════════════════════════════════════════════════════════════════════════════
// Tests
// ══════════════════════════════════════════════════════════════════════════════

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_compaction_settings_defaults() {
        let s = CompactionSettings::default();
        assert!(s.auto);
        assert_eq!(s.buffer, DEFAULT_COMPACTION_BUFFER);
        assert_eq!(s.tokens, DEFAULT_KEEP_TOKENS);
    }

    #[test]
    fn test_compaction_settings_serialization() {
        let s = CompactionSettings {
            auto: false,
            buffer: 30_000,
            tokens: 4_000,
        };
        let json = serde_json::to_string(&s).expect("serialize");
        assert!(json.contains("30000"));
        assert!(json.contains("4000"));
        let parsed: CompactionSettings = serde_json::from_str(&json).expect("deserialize");
        assert!(!parsed.auto);
        assert_eq!(parsed.buffer, 30_000);
        assert_eq!(parsed.tokens, 4_000);
    }

    #[test]
    fn test_compaction_result_serialization() {
        assert_eq!(
            serde_json::to_string(&CompactionResult::Continue).expect("serialize"),
            r#""continue""#
        );
        assert_eq!(
            serde_json::to_string(&CompactionResult::Stop).expect("serialize"),
            r#""stop""#
        );
    }

    #[test]
    fn test_compaction_strategy_serialization() {
        assert_eq!(
            serde_json::to_string(&CompactionStrategy::Auto).expect("serialize"),
            r#""auto""#
        );
        assert_eq!(
            serde_json::to_string(&CompactionStrategy::Manual).expect("serialize"),
            r#""manual""#
        );
    }

    #[test]
    fn test_compaction_selection_roundtrip() {
        let sel = CompactionSelection {
            head: "Older context...".into(),
            recent: "Recent conversation...".into(),
        };
        let json = serde_json::to_string(&sel).expect("serialize");
        let parsed: CompactionSelection = serde_json::from_str(&json).expect("deserialize");
        assert_eq!(parsed.head, "Older context...");
        assert_eq!(parsed.recent, "Recent conversation...");
    }

    #[test]
    fn test_compaction_turn_roundtrip() {
        let turn = CompactionTurn {
            start: 0,
            end: 5,
            id: "msg_001".into(),
        };
        let json = serde_json::to_string(&turn).expect("serialize");
        let parsed: CompactionTurn = serde_json::from_str(&json).expect("deserialize");
        assert_eq!(parsed.start, 0);
        assert_eq!(parsed.end, 5);
        assert_eq!(parsed.id, "msg_001");
    }

    #[test]
    fn test_completed_compaction_with_summary() {
        let cc = CompletedCompaction {
            user_index: 2,
            assistant_index: 3,
            summary: Some("Fixed auth bug".into()),
        };
        let json = serde_json::to_string(&cc).expect("serialize");
        assert!(json.contains("Fixed auth bug"));
        let parsed: CompletedCompaction = serde_json::from_str(&json).expect("deserialize");
        assert_eq!(parsed.summary.as_deref(), Some("Fixed auth bug"));
    }

    #[test]
    fn test_completed_compaction_without_summary() {
        let cc = CompletedCompaction {
            user_index: 1,
            assistant_index: 2,
            summary: None,
        };
        let json = serde_json::to_string(&cc).expect("serialize");
        assert!(!json.contains("summary"));
    }

    #[test]
    fn test_constants_are_nonzero() {
        assert!(DEFAULT_COMPACTION_BUFFER > 0);
        assert!(DEFAULT_KEEP_TOKENS > 0);
        assert!(TOOL_OUTPUT_MAX_CHARS > 0);
        assert!(SUMMARY_OUTPUT_TOKENS > 0);
        assert!(PRUNE_MINIMUM_TOKENS > 0);
        assert!(PRUNE_PROTECT_TOKENS > 0);
        assert!(DEFAULT_TAIL_TURNS > 0);
        assert!(MIN_PRESERVE_RECENT_TOKENS > 0);
        assert!(MAX_PRESERVE_RECENT_TOKENS > 0);
    }

    #[test]
    fn test_compaction_compacted_event() {
        let event = CompactionCompactedEvent {
            session_id: "ses_compacted".into(),
        };
        let json = serde_json::to_string(&event).expect("serialize");
        assert!(json.contains("ses_compacted"));
    }
}
