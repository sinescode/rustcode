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
// Session Compaction Service
// ══════════════════════════════════════════════════════════════════════════════

/// Service that performs context window compaction.
///
/// When the LLM context is full, compaction summarizes older messages
/// into a concise form, preserving recent context.
///
/// Ported from:
/// - `packages/opencode/src/session/compaction.ts` (lines 1-621)
/// - `packages/core/src/session/compaction.ts` (lines 1-247)
pub struct SessionCompaction {
    settings: CompactionSettings,
}

impl SessionCompaction {
    /// Create a new compaction service with default settings.
    pub fn new() -> Self {
        Self {
            settings: CompactionSettings::default(),
        }
    }

    /// Create with custom settings.
    pub fn with_settings(settings: CompactionSettings) -> Self {
        Self { settings }
    }

    /// Determine if compaction should be triggered based on token count.
    ///
    /// Returns `true` if the total tokens in `head` exceed the usable context
    /// (model context limit minus buffer).
    ///
    /// Ported from: `packages/core/src/session/compaction.ts` — `shouldCompact`
    pub fn should_compact(
        &self,
        total_tokens: u64,
        model_context_limit: u64,
        output_token_max: u64,
    ) -> bool {
        if model_context_limit == 0 {
            return false;
        }

        let buffer = self.settings.buffer.min(output_token_max);
        let usable = model_context_limit.saturating_sub(buffer);
        total_tokens >= usable
    }

    /// Select which messages to compact and which to preserve.
    ///
    /// Splits the message list into a "head" (older context to summarize)
    /// and "recent" (last N turns to keep verbatim).
    ///
    /// Ported from: `packages/opencode/src/session/compaction.ts` — `select`
    pub fn select(
        &self,
        messages_json: &serde_json::Value,
        model_context_limit: u64,
    ) -> CompactionSelection {
        let messages = messages_json.as_array().map(|a| a.as_slice()).unwrap_or(&[]);
        let total_msgs = messages.len();

        if total_msgs == 0 {
            return CompactionSelection {
                head: String::new(),
                recent: String::new(),
            };
        }

        // Count turns (user → assistant pairs)
        let turns = self.identify_turns(messages);
        let tail_turns = self.settings.tokens as usize / 4_000; // rough char estimate
        let keep_turns = DEFAULT_TAIL_TURNS.max(tail_turns as u64).min(turns.len() as u64) as usize;

        if keep_turns >= turns.len() {
            // Not enough messages to split — return all as recent
            return CompactionSelection {
                head: String::new(),
                recent: serde_json::to_string_pretty(messages_json).unwrap_or_default(),
            };
        }

        let split_turn = turns[turns.len() - keep_turns];
        let split_index = split_turn.start;

        // Head: messages before the split
        let head_msgs = &messages[..split_index];
        // Recent: messages from split onward
        let recent_msgs = &messages[split_index..];

        CompactionSelection {
            head: serde_json::to_string_pretty(&serde_json::Value::Array(head_msgs.to_vec()))
                .unwrap_or_default(),
            recent: serde_json::to_string_pretty(&serde_json::Value::Array(recent_msgs.to_vec()))
                .unwrap_or_default(),
        }
    }

    /// Identify user→assistant turn boundaries in a message list.
    ///
    /// Ported from: `packages/opencode/src/session/compaction.ts` lines 46-54
    fn identify_turns(&self, messages: &[serde_json::Value]) -> Vec<CompactionTurn> {
        let mut turns = Vec::new();
        let mut turn_start: Option<usize> = None;

        for (i, msg) in messages.iter().enumerate() {
            let msg_type = msg.get("type").and_then(|t| t.as_str()).unwrap_or("");

            match msg_type {
                "user" => {
                    if let Some(start) = turn_start {
                        // Close previous turn if it had content
                        if i > start {
                            turns.push(CompactionTurn {
                                start,
                                end: i,
                                id: format!("turn_{}", turns.len()),
                            });
                        }
                    }
                    turn_start = Some(i);
                }
                "assistant" => {
                    // assistant is part of current turn
                }
                _ => {
                    // Non-conversation messages (agent-switched, system, etc.)
                }
            }
        }

        // Close final turn
        if let Some(start) = turn_start {
            if start < messages.len() {
                turns.push(CompactionTurn {
                    start,
                    end: messages.len(),
                    id: format!("turn_{}", turns.len()),
                });
            }
        }

        turns
    }

    /// Compute the effective compaction buffer size.
    ///
    /// Ported from: `packages/core/src/session/compaction.ts` — buffer calculation
    pub fn effective_buffer(&self) -> u64 {
        self.settings.buffer
    }

    /// Get the current compaction settings.
    pub fn settings(&self) -> &CompactionSettings {
        &self.settings
    }

    /// Calculate the target token count after compaction.
    ///
    /// This is the number of tokens to aim for in the compacted head.
    ///
    /// Ported from: `packages/core/src/session/compaction.ts` — target calculation
    pub fn target_tokens(&self, input_tokens: u64) -> u64 {
        // Aim to reduce head to ~50% of current tokens, but no less than keep_tokens
        let half = input_tokens / 2;
        half.max(self.settings.tokens)
    }

    /// Truncate tool output to a maximum character length.
    ///
    /// Ported from: `packages/core/src/session/compaction.ts` — `truncateToolOutput`
    pub fn truncate_tool_output(output: &str) -> String {
        if output.len() <= TOOL_OUTPUT_MAX_CHARS {
            output.to_string()
        } else {
            let truncated: String = output.chars().take(TOOL_OUTPUT_MAX_CHARS).collect();
            format!("{truncated}\n... [truncated]")
        }
    }
}

impl Default for SessionCompaction {
    fn default() -> Self {
        Self::new()
    }
}

// ── Compaction result helpers ──────────────────────────────────────────

impl CompactionSelection {
    /// Check if the selection has any head content to compact.
    pub fn has_head(&self) -> bool {
        !self.head.is_empty() && self.head != "[]" && self.head != "[\n\n]"
    }

    /// Check if the selection has any recent content.
    pub fn has_recent(&self) -> bool {
        !self.recent.is_empty() && self.recent != "[]" && self.recent != "[\n\n]"
    }

    /// Get the approximate character lengths of head and recent sections.
    pub fn char_lengths(&self) -> (usize, usize) {
        (self.head.len(), self.recent.len())
    }
}

impl CompletedCompaction {
    /// Create a completed compaction with a summary.
    pub fn with_summary(user_index: usize, assistant_index: usize, summary: String) -> Self {
        Self {
            user_index,
            assistant_index,
            summary: Some(summary),
        }
    }

    /// Create a completed compaction without a summary.
    pub fn without_summary(user_index: usize, assistant_index: usize) -> Self {
        Self {
            user_index,
            assistant_index,
            summary: None,
        }
    }
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

    // ── SessionCompaction tests ─────────────────────────────────────

    #[test]
    fn test_session_compaction_default() {
        let sc = SessionCompaction::default();
        assert!(sc.settings().auto);
        assert_eq!(sc.effective_buffer(), DEFAULT_COMPACTION_BUFFER);
    }

    #[test]
    fn test_session_compaction_custom_settings() {
        let settings = CompactionSettings {
            auto: false,
            buffer: 50_000,
            tokens: 4_000,
        };
        let sc = SessionCompaction::with_settings(settings.clone());
        assert!(!sc.settings().auto);
        assert_eq!(sc.effective_buffer(), 50_000);
    }

    #[test]
    fn test_should_compact_under_limit() {
        let sc = SessionCompaction::default();
        assert!(!sc.should_compact(50_000, 200_000, 16_000));
    }

    #[test]
    fn test_should_compact_over_limit() {
        let sc = SessionCompaction::default();
        assert!(sc.should_compact(190_000, 200_000, 16_000));
    }

    #[test]
    fn test_should_compact_zero_context() {
        let sc = SessionCompaction::default();
        assert!(!sc.should_compact(1_000_000, 0, 0));
    }

    #[test]
    fn test_select_empty_messages() {
        let sc = SessionCompaction::default();
        let messages = serde_json::json!([]);
        let selection = sc.select(&messages, 200_000);
        assert!(!selection.has_head());
        assert!(!selection.has_recent());
    }

    #[test]
    fn test_select_single_turn() {
        let sc = SessionCompaction::default();
        let messages = serde_json::json!([
            {"type": "user", "text": "Hello"},
            {"type": "assistant", "text": "Hi there!"}
        ]);
        let selection = sc.select(&messages, 200_000);
        // Single turn: all should be recent (not enough to split)
        assert!(!selection.has_head());
        assert!(selection.has_recent());
    }

    #[test]
    fn test_select_many_turns() {
        let sc = SessionCompaction::default();
        let mut messages = Vec::new();
        for i in 0..20 {
            messages.push(serde_json::json!({"type": "user", "text": format!("Question {i}")}));
            messages.push(serde_json::json!({"type": "assistant", "text": format!("Answer {i}")}));
        }
        let messages_json = serde_json::json!(messages);
        let selection = sc.select(&messages_json, 200_000);
        // Should split with head and recent
        // With 40 messages (20 turns), tail_turns keeps last 2 turns
        assert!(selection.has_head(), "Should have head content");
        assert!(selection.has_recent(), "Should have recent content");
    }

    #[test]
    fn test_target_tokens() {
        let sc = SessionCompaction::default();
        let target = sc.target_tokens(100_000);
        // Should be ~50% of input, at least keep_tokens
        assert!(target >= DEFAULT_KEEP_TOKENS);
        assert!(target <= 100_000);
    }

    #[test]
    fn test_target_tokens_small_input() {
        let sc = SessionCompaction::default();
        let target = sc.target_tokens(1_000);
        // Small input: target should be at least keep_tokens
        assert_eq!(target, DEFAULT_KEEP_TOKENS);
    }

    #[test]
    fn test_truncate_tool_output_short() {
        let output = "Short output";
        let result = SessionCompaction::truncate_tool_output(output);
        assert_eq!(result, output);
    }

    #[test]
    fn test_truncate_tool_output_long() {
        let output = "x".repeat(TOOL_OUTPUT_MAX_CHARS + 100);
        let result = SessionCompaction::truncate_tool_output(&output);
        assert!(result.len() <= TOOL_OUTPUT_MAX_CHARS + 20); // + "[truncated]" indicator
        assert!(result.contains("[truncated]"));
    }

    #[test]
    fn test_compaction_selection_helpers() {
        let sel = CompactionSelection {
            head: String::new(),
            recent: "[\n  {\"type\": \"user\"}\n]".to_string(),
        };
        assert!(!sel.has_head());
        assert!(sel.has_recent());
        let (h, r) = sel.char_lengths();
        assert_eq!(h, 0);
        assert!(r > 0);
    }

    #[test]
    fn test_completed_compaction_constructors() {
        let with = CompletedCompaction::with_summary(0, 1, "Fixed bug".into());
        assert_eq!(with.summary.as_deref(), Some("Fixed bug"));

        let without = CompletedCompaction::without_summary(0, 1);
        assert!(without.summary.is_none());
    }

    #[test]
    fn test_identify_turns_empty() {
        let sc = SessionCompaction::default();
        let turns = sc.identify_turns(&[]);
        assert!(turns.is_empty());
    }

    #[test]
    fn test_identify_turns_system_messages() {
        let sc = SessionCompaction::default();
        let messages = vec![
            serde_json::json!({"type": "system", "text": "System msg"}),
            serde_json::json!({"type": "agent-switched", "agent": "build"}),
        ];
        let turns = sc.identify_turns(&messages);
        // No user messages, so no turns
        assert!(turns.is_empty());
    }
}
