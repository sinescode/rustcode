//! Session compaction types — input/output, strategy, and related enums.
//!
//! Ported from:
//! - `packages/core/src/session/compaction.ts` (lines 1–247)
//! - `packages/opencode/src/session/compaction.ts` (lines 1–621)

use crate::config;
use crate::error::Result;
use crate::session_info::SessionId;
use crate::provider::{ChatMessage, MessageContent, Model, Provider};
use serde::{Deserialize, Serialize};

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
        let messages = messages_json
            .as_array()
            .map(|a| a.as_slice())
            .unwrap_or(&[]);
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
        let keep_turns = DEFAULT_TAIL_TURNS
            .max(tail_turns as u64)
            .min(turns.len() as u64) as usize;

        if keep_turns >= turns.len() {
            // Not enough messages to split — return all as recent
            return CompactionSelection {
                head: String::new(),
                recent: serde_json::to_string_pretty(messages_json).unwrap_or_default(),
            };
        }

        let split_turn = turns[turns.len() - keep_turns].clone();
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
// Token Estimation
// ══════════════════════════════════════════════════════════════════════════════

/// Roughly estimate token count from character length (~4 chars/token).
fn estimate_tokens(text: &str) -> u64 {
    (text.len() as u64 + 2) / 4
}

/// Summary template string for the compaction LLM prompt.
///
/// Ported from: `packages/core/src/session/compaction.ts` line 16–51 `SUMMARY_TEMPLATE`.
const SUMMARY_TEMPLATE: &str = r#"Output exactly the Markdown structure shown inside <template> and keep the section order unchanged. Do not include the <template> tags in your response.
<template>
## Goal
- [single-sentence task summary]

## Constraints & Preferences
- [user constraints, preferences, specs, or "(none)"]

## Progress
### Done
- [completed work or "(none)"]

### In Progress
- [current work or "(none)"]

### Blocked
- [blockers or "(none)"]

## Key Decisions
- [decision and why, or "(none)"]

## Next Steps
- [ordered next actions or "(none)"]

## Critical Context
- [important technical facts, errors, open questions, or "(none)"]

## Relevant Files
- [file or directory path: why it matters, or "(none)"]
</template>

Rules:
- Keep every section, even when empty.
- Use terse bullets, not prose paragraphs.
- Preserve exact file paths, commands, error strings, and identifiers when known.
- Do not mention the summary process or that context was compacted."#;

// ══════════════════════════════════════════════════════════════════════════════
// Compaction Selector — picks which messages to compact
// ══════════════════════════════════════════════════════════════════════════════

/// An identified user->assistant turn boundary.
///
/// Ported from: `packages/opencode/src/session/compaction.ts` lines 46–49 `Turn`.
#[derive(Debug, Clone)]
struct Turn {
    start: usize,
    end: usize,
    id: String,
}

/// Identify user→assistant turn boundaries, skipping compaction messages.
///
/// Ported from: `packages/opencode/src/session/compaction.ts` lines 97–113 `turns`.
fn identify_turns_v2(messages: &[serde_json::Value]) -> Vec<Turn> {
    let mut result = Vec::new();
    for (i, msg) in messages.iter().enumerate() {
        let msg_type = msg.get("type").and_then(|t| t.as_str()).unwrap_or("");
        if msg_type != "user" {
            continue;
        }
        let is_compaction = msg
            .get("parts")
            .and_then(|p| p.as_array())
            .map_or(false, |parts| {
                parts
                    .iter()
                    .any(|p| p.get("type").and_then(|t| t.as_str()) == Some("compaction"))
            });
        if is_compaction {
            continue;
        }
        result.push(Turn {
            start: i,
            end: messages.len(),
            id: msg
                .get("id")
                .and_then(|i| i.as_str())
                .unwrap_or("")
                .to_string(),
        });
    }
    for i in 0..result.len().saturating_sub(1) {
        result[i].end = result[i + 1].start;
    }
    result
}

/// Selector that determines which messages to compact vs. preserve.
///
/// Preserves the last N tail turns of conversation (configurable) and marks
/// everything older as "head" (to be summarized). Respects a token budget for
/// the preserved recent context.
///
/// Ported from: `packages/opencode/src/session/compaction.ts` — `select`
#[derive(Debug, Clone)]
pub struct CompactionSelector {
    /// Number of recent tail turns to preserve verbatim
    pub tail_turns: u64,
    /// Token budget for preserving recent context
    pub preserve_recent_tokens: u64,
}

impl CompactionSelector {
    /// Create a new selector with default parameters.
    pub fn new() -> Self {
        Self {
            tail_turns: DEFAULT_TAIL_TURNS,
            preserve_recent_tokens: DEFAULT_KEEP_TOKENS,
        }
    }

    /// Create a selector from compaction config and model context limit.
    ///
    /// Ported from: `packages/opencode/src/session/compaction.ts` —
    /// `preserveRecentBudget` + `select` config reading.
    pub fn from_config(cfg: &config::CompactionConfig, model_context_limit: u64) -> Self {
        let tail_turns = cfg.tail_turns.unwrap_or(DEFAULT_TAIL_TURNS as u32) as u64;
        let preserve_recent_tokens = cfg
            .preserve_recent_tokens
            .map(|v| v as u64)
            .unwrap_or_else(|| {
                let usable = model_context_limit.saturating_sub(SUMMARY_OUTPUT_TOKENS);
                let raw = usable / 4;
                raw.clamp(MIN_PRESERVE_RECENT_TOKENS, MAX_PRESERVE_RECENT_TOKENS)
            });
        Self {
            tail_turns,
            preserve_recent_tokens,
        }
    }

    /// Select which messages go to head (compaction target) and tail (preserve).
    ///
    /// Returns `(head_indices, recent_indices)`.
    pub fn select(&self, messages: &[serde_json::Value]) -> (Vec<usize>, Vec<usize>) {
        if messages.is_empty() {
            return (Vec::new(), Vec::new());
        }

        let all_turns = identify_turns_v2(messages);
        if all_turns.is_empty() || self.tail_turns == 0 {
            let head: Vec<_> = (0..messages.len()).collect();
            return (head, Vec::new());
        }

        let tail_count = (self.tail_turns as usize).min(all_turns.len());
        let split_idx = all_turns.len() - tail_count;
        let recent_turns = &all_turns[split_idx..];

        let mut budget = self.preserve_recent_tokens;
        let mut keep_start: Option<usize> = None;

        // Walk backwards through recent turns, accumulating budget
        for turn in recent_turns.iter().rev() {
            let turn_text: String = messages[turn.start..turn.end]
                .iter()
                .map(|m| CompactionSerializer::serialize(m))
                .collect::<Vec<_>>()
                .join("\n");
            let turn_tokens = estimate_tokens(&turn_text);

            if turn_tokens <= budget {
                budget -= turn_tokens;
                keep_start = Some(turn.start);
            } else {
                let remaining = budget;
                if remaining > 0 {
                    // Try to split the last turn at a message boundary
                    let mut partial_total = 0u64;
                    let mut split_at = turn.end;
                    for i in (turn.start..turn.end).rev() {
                        let msg_text = CompactionSerializer::serialize(&messages[i]);
                        let msg_tokens = estimate_tokens(&msg_text);
                        if partial_total + msg_tokens > remaining {
                            break;
                        }
                        partial_total += msg_tokens;
                        split_at = i;
                    }
                    if split_at < turn.end {
                        keep_start = Some(split_at);
                    }
                }
                break;
            }
        }

        let keep = keep_start.unwrap_or(messages.len());
        let head: Vec<_> = (0..keep).collect();
        let recent: Vec<_> = (keep..messages.len()).collect();
        (head, recent)
    }

    /// Get the tail turns configuration.
    pub fn tail_turns(&self) -> u64 {
        self.tail_turns
    }

    /// Get the recent token budget.
    pub fn preserve_recent_tokens(&self) -> u64 {
        self.preserve_recent_tokens
    }
}

impl Default for CompactionSelector {
    fn default() -> Self {
        Self::new()
    }
}

// ══════════════════════════════════════════════════════════════════════════════
// Compaction Serializer — serializes messages and builds the LLM prompt
// ══════════════════════════════════════════════════════════════════════════════

/// Serializer that converts session messages into text for the compaction prompt.
///
/// Ported from: `packages/core/src/session/compaction.ts` — `serialize`, `buildPrompt`
pub struct CompactionSerializer;

impl CompactionSerializer {
    /// Truncate tool output to max chars.
    fn truncate_output(output: &str) -> String {
        if output.len() <= TOOL_OUTPUT_MAX_CHARS {
            output.to_string()
        } else {
            let truncated: String = output.chars().take(TOOL_OUTPUT_MAX_CHARS).collect();
            format!("{truncated}\n... [truncated]")
        }
    }

    /// Serialize tool content parts to text.
    ///
    /// Ported from: `packages/core/src/session/compaction.ts` — `serializeToolContent`
    pub fn serialize_tool_content(content: &serde_json::Value) -> String {
        let parts = match content.as_array() {
            Some(arr) => arr,
            None => return String::new(),
        };
        parts
            .iter()
            .map(|item| {
                let item_type = item.get("type").and_then(|t| t.as_str()).unwrap_or("");
                match item_type {
                    "text" => item
                        .get("text")
                        .and_then(|t| t.as_str())
                        .unwrap_or("")
                        .to_string(),
                    _ => {
                        let mime =
                            item.get("mime").and_then(|m| m.as_str()).unwrap_or("unknown");
                        let name = item.get("name").and_then(|n| n.as_str());
                        match name {
                            Some(n) => format!("[Attached {mime}: {n}]"),
                            None => format!("[Attached {mime}]"),
                        }
                    }
                }
            })
            .collect::<Vec<_>>()
            .join("\n")
    }

    /// Serialize a single session message to text for the compaction prompt.
    ///
    /// Ported from: `packages/core/src/session/compaction.ts` lines 91–117 `serialize`
    pub fn serialize(message: &serde_json::Value) -> String {
        let msg_type = message.get("type").and_then(|t| t.as_str()).unwrap_or("");
        match msg_type {
            "user" => {
                let text = message.get("text").and_then(|t| t.as_str()).unwrap_or("");
                let files = message.get("files").and_then(|f| f.as_array());
                let mut parts = vec![format!("[User]: {text}")];
                if let Some(files) = files {
                    for file in files {
                        let mime =
                            file.get("mime").and_then(|m| m.as_str()).unwrap_or("unknown");
                        let uri = file.get("uri").and_then(|u| u.as_str()).unwrap_or("");
                        let name = file.get("name").and_then(|n| n.as_str());
                        let suffix = name.map(|n| format!(": {n}")).unwrap_or_default();
                        parts.push(format!("[Attached {mime}{suffix}]"));
                        let _ = uri;
                    }
                }
                parts.join("\n")
            }
            "assistant" => {
                let content = message.get("content").and_then(|c| c.as_array());
                let content = match content {
                    Some(c) => c,
                    None => return String::new(),
                };
                let lines: Vec<String> = content
                    .iter()
                    .flat_map(|part| {
                        let part_type =
                            part.get("type").and_then(|t| t.as_str()).unwrap_or("");
                        match part_type {
                            "text" => {
                                let text = part
                                    .get("text")
                                    .and_then(|t| t.as_str())
                                    .unwrap_or("");
                                vec![format!("[Assistant]: {text}")]
                            }
                            "reasoning" => {
                                let text = part
                                    .get("text")
                                    .and_then(|t| t.as_str())
                                    .unwrap_or("");
                                if text.is_empty() {
                                    vec![]
                                } else {
                                    vec![format!("[Assistant reasoning]: {text}")]
                                }
                            }
                            "tool" | "tool-call" | "tool_call" => {
                                let name = part
                                    .get("tool")
                                    .and_then(|t| t.as_str())
                                    .or_else(|| {
                                        part.get("toolName").and_then(|t| t.as_str())
                                    })
                                    .unwrap_or("unknown");
                                let input_val = part
                                    .get("state")
                                    .and_then(|s| s.get("input"))
                                    .or_else(|| part.get("input"));
                                let input_str = match input_val {
                                    Some(serde_json::Value::String(s)) => s.clone(),
                                    Some(other) => {
                                        serde_json::to_string(other).unwrap_or_default()
                                    }
                                    None => String::new(),
                                };

                                let status = part
                                    .get("state")
                                    .and_then(|s| s.get("status"))
                                    .and_then(|s| s.as_str())
                                    .unwrap_or("");

                                let mut lines =
                                    vec![format!("[Assistant tool call]: {name}({input_str})")];
                                match status {
                                    "completed" => {
                                        let content_val = part
                                            .get("state")
                                            .and_then(|s| s.get("content"));
                                        if let Some(c) = content_val {
                                            let result = Self::truncate_output(
                                                &Self::serialize_tool_content(c),
                                            );
                                            lines.push(format!("[Tool result]: {result}"));
                                        }
                                    }
                                    "error" => {
                                        let err = part
                                            .get("state")
                                            .and_then(|s| s.get("error"))
                                            .and_then(|e| e.get("message"))
                                            .and_then(|m| m.as_str())
                                            .unwrap_or("unknown error");
                                        lines.push(format!("[Tool error]: {err}"));
                                    }
                                    _ => {}
                                }
                                lines
                            }
                            _ => vec![],
                        }
                    })
                    .collect();
                lines.join("\n")
            }
            "system" => {
                let text = message.get("text").and_then(|t| t.as_str()).unwrap_or("");
                format!("[System update]: {text}")
            }
            "synthetic" => {
                let text = message.get("text").and_then(|t| t.as_str()).unwrap_or("");
                format!("[Synthetic context]: {text}")
            }
            "shell" => {
                let command = message
                    .get("command")
                    .and_then(|c| c.as_str())
                    .unwrap_or("");
                let output = message
                    .get("output")
                    .and_then(|o| o.as_str())
                    .unwrap_or("");
                let truncated = Self::truncate_output(output);
                format!("[Shell]: {command}\n{truncated}")
            }
            _ => String::new(),
        }
    }

    /// Build the compaction LLM prompt.
    ///
    /// If a previous summary exists, prompts the model to update it.
    /// Otherwise, asks for a new summary.
    ///
    /// Ported from: `packages/core/src/session/compaction.ts` lines 166–173 `buildPrompt`
    pub fn build_prompt(previous_summary: Option<&str>, context: &[String]) -> String {
        let intro = match previous_summary {
            Some(summary) => format!(
                "Update the anchored summary below using the conversation history above.\n\
                 Preserve still-true details, remove stale details, and merge in the new facts.\n\
                 <previous-summary>\n{summary}\n</previous-summary>"
            ),
            None => "Create a new anchored summary from the conversation history.".to_string(),
        };
        let mut parts = vec![intro, SUMMARY_TEMPLATE.to_string()];
        parts.extend_from_slice(context);
        parts.join("\n\n")
    }

    /// Serialize an array of messages to text, filtering out compaction messages.
    pub fn serialize_messages(messages: &[serde_json::Value]) -> Vec<String> {
        messages
            .iter()
            .filter(|m| {
                let msg_type = m.get("type").and_then(|t| t.as_str()).unwrap_or("");
                msg_type != "compaction"
            })
            .map(|m| Self::serialize(m))
            .filter(|s| !s.is_empty())
            .collect()
    }
}

// ══════════════════════════════════════════════════════════════════════════════
// Compact Result
// ══════════════════════════════════════════════════════════════════════════════

/// Result of a successful compaction execution.
#[derive(Debug, Clone)]
pub struct CompactResult {
    /// The summary text produced by the LLM
    pub summary: String,
    /// The serialized recent context preserved verbatim
    pub recent: String,
}

// ══════════════════════════════════════════════════════════════════════════════
// Compaction Execution — calls LLM to produce summary
// ══════════════════════════════════════════════════════════════════════════════

impl SessionCompaction {
    /// Execute LLM-based compaction on the given messages.
    ///
    /// This performs the full compaction pipeline:
    /// 1. Use [`CompactionSelector`] to split messages into head (compact) and recent (preserve)
    /// 2. Serialize head messages via [`CompactionSerializer`]
    /// 3. Build the LLM compaction prompt
    /// 4. Call `provider.complete()` with the prompt
    /// 5. Extract and return the summary
    ///
    /// Returns `None` if compaction is not possible (no messages to compact,
    /// prompt exceeds context, empty response).
    ///
    /// Ported from:
    /// - `packages/core/src/session/compaction.ts` — `compactIfNeeded`, `compactAfterOverflow`
    /// - `packages/opencode/src/session/compaction.ts` — `process`
    pub async fn compact(
        &self,
        messages: &[serde_json::Value],
        model: &Model,
        provider: &dyn Provider,
        previous_summary: Option<&str>,
        cfg: &config::CompactionConfig,
    ) -> Result<Option<CompactResult>> {
        if messages.is_empty() {
            return Ok(None);
        }

        // Step 1: Select messages to compact vs. preserve
        let selector = CompactionSelector::from_config(cfg, model.limit.context);
        let (head_indices, recent_indices) = selector.select(messages);

        if head_indices.is_empty() {
            return Ok(None);
        }

        // Step 2: Collect head messages
        let head_msgs: Vec<serde_json::Value> =
            head_indices.iter().map(|&i| messages[i].clone()).collect();

        // Step 3: Serialize head messages
        let serialized: Vec<String> = CompactionSerializer::serialize_messages(&head_msgs);
        if serialized.is_empty() {
            return Ok(None);
        }

        // Step 4: Build the compaction prompt
        let prompt = CompactionSerializer::build_prompt(previous_summary, &serialized);

        // Step 5: Check if the prompt fits within context limits
        let prompt_tokens = estimate_tokens(&prompt);
        let summary_tokens = SUMMARY_OUTPUT_TOKENS.min(model.limit.output);
        let usable = model.limit.context.saturating_sub(self.settings.buffer);

        if prompt_tokens + summary_tokens > usable {
            return Ok(None);
        }

        // Step 6: Call the provider
        let chat_message = ChatMessage::User {
            content: MessageContent::Text(prompt),
        };
        let response = provider.complete(model, &[chat_message], &[]).await?;

        // Step 7: Extract summary text from response
        let summary = response.text();
        let summary = summary.trim();
        if summary.is_empty() {
            return Ok(None);
        }

        // Step 8: Build recent text
        let recent_text = recent_indices
            .iter()
            .map(|&i| CompactionSerializer::serialize(&messages[i]))
            .collect::<Vec<_>>()
            .join("\n\n");

        Ok(Some(CompactResult {
            summary: summary.to_string(),
            recent: recent_text,
        }))
    }

    /// Determine if compaction should be triggered and execute it.
    ///
    /// Only triggers if:
    /// - Auto-compaction is enabled in settings
    /// - Estimated total tokens exceed the context limit minus buffer
    ///
    /// Ported from: `packages/core/src/session/compaction.ts` — `compactIfNeeded`
    pub async fn compact_if_needed(
        &self,
        messages: &[serde_json::Value],
        model: &Model,
        provider: &dyn Provider,
        previous_summary: Option<&str>,
        cfg: &crate::config::CompactionConfig,
    ) -> Result<Option<CompactResult>> {
        if !self.settings.auto {
            return Ok(None);
        }

        // Estimate total tokens from all messages
        let total_text: String = messages
            .iter()
            .filter_map(|m| serde_json::to_string(m).ok())
            .collect::<Vec<_>>()
            .join("\n");
        let total_tokens = estimate_tokens(&total_text);

        if !self.should_compact(total_tokens, model.limit.context, model.limit.output) {
            return Ok(None);
        }

        self.compact(messages, model, provider, previous_summary, cfg)
            .await
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
        const {
            assert!(DEFAULT_COMPACTION_BUFFER > 0);
        }
        const {
            assert!(DEFAULT_KEEP_TOKENS > 0);
        }
        const {
            assert!(TOOL_OUTPUT_MAX_CHARS > 0);
        }
        const {
            assert!(SUMMARY_OUTPUT_TOKENS > 0);
        }
        const {
            assert!(PRUNE_MINIMUM_TOKENS > 0);
        }
        const {
            assert!(PRUNE_PROTECT_TOKENS > 0);
        }
        const {
            assert!(DEFAULT_TAIL_TURNS > 0);
        }
        const {
            assert!(MIN_PRESERVE_RECENT_TOKENS > 0);
        }
        const {
            assert!(MAX_PRESERVE_RECENT_TOKENS > 0);
        }
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

    // ── CompactionSelector tests ───────────────────────────────────

    #[test]
    fn test_selector_default() {
        let sel = CompactionSelector::default();
        assert_eq!(sel.tail_turns(), 2);
        assert_eq!(sel.preserve_recent_tokens(), 8_000);
    }

    #[test]
    fn test_selector_from_config() {
        let cfg = config::CompactionConfig {
            tail_turns: Some(3),
            preserve_recent_tokens: Some(4000),
            ..Default::default()
        };
        let sel = CompactionSelector::from_config(&cfg, 200_000);
        assert_eq!(sel.tail_turns(), 3);
        assert_eq!(sel.preserve_recent_tokens(), 4000);
    }

    #[test]
    fn test_selector_empty_messages() {
        let sel = CompactionSelector::default();
        let (head, recent) = sel.select(&[]);
        assert!(head.is_empty());
        assert!(recent.is_empty());
    }

    #[test]
    fn test_selector_single_turn_no_head() {
        let sel = CompactionSelector::new();
        let messages = vec![
            serde_json::json!({"type": "user", "text": "Hello", "id": "msg_1"}),
            serde_json::json!({"type": "assistant", "text": "Hi", "id": "msg_2"}),
        ];
        let (head, recent) = sel.select(&messages);
        // With 1 turn and tail_turns=2, all should be recent
        assert!(head.is_empty());
        assert_eq!(recent.len(), 2);
    }

    #[test]
    fn test_selector_many_turns_has_head() {
        let sel = CompactionSelector::new();
        let mut messages = Vec::new();
        for i in 0..10 {
            messages.push(serde_json::json!({"type": "user", "text": format!("Q{i}"), "id": format!("msg_q{i}")}));
            messages.push(serde_json::json!({"type": "assistant", "text": format!("A{i}"), "id": format!("msg_a{i}")}));
        }
        let (head, recent) = sel.select(&messages);
        // With 10 turns and tail_turns=2, head should have first 16 messages (8 turns)
        assert!(!head.is_empty(), "Should have head messages");
        assert!(!recent.is_empty(), "Should have recent messages");
        assert!(head.len() < messages.len(), "Head should be less than total");
        assert!(
            head.len() + recent.len() == messages.len(),
            "Head + recent should equal total"
        );
    }

    #[test]
    fn test_selector_zero_tail_turns() {
        let sel = CompactionSelector {
            tail_turns: 0,
            preserve_recent_tokens: 8000,
        };
        let messages = vec![
            serde_json::json!({"type": "user", "text": "Hi", "id": "msg_1"}),
            serde_json::json!({"type": "assistant", "text": "Hello", "id": "msg_2"}),
        ];
        let (head, recent) = sel.select(&messages);
        // With 0 tail turns, all should be head
        assert!(!head.is_empty());
        assert!(recent.is_empty());
    }

    #[test]
    fn test_selector_skips_compaction_messages() {
        let sel = CompactionSelector::new();
        let messages = vec![
            serde_json::json!({"type": "user", "text": "Old Q", "id": "msg_1"}),
            serde_json::json!({"type": "assistant", "text": "Old A", "id": "msg_2"}),
            serde_json::json!({"type": "user", "parts": [{"type": "compaction"}], "id": "msg_3"}),
            serde_json::json!({"type": "assistant", "summary": true, "parts": [{"type": "text", "text": "Summary"}], "id": "msg_4"}),
            serde_json::json!({"type": "user", "text": "New Q", "id": "msg_5"}),
            serde_json::json!({"type": "assistant", "text": "New A", "id": "msg_6"}),
        ];
        let (head, recent) = sel.select(&messages);
        // Compaction user should not count as a turn.
        // 2 real turns (msg_1 and msg_5), tail_turns=2, so all as recent
        assert!(head.is_empty());
        assert_eq!(recent.len(), 6);
    }

    // ── CompactionSerializer tests ─────────────────────────────────

    #[test]
    fn test_serializer_user_message() {
        let msg = serde_json::json!({
            "type": "user",
            "text": "Fix the bug in main.rs"
        });
        let result = CompactionSerializer::serialize(&msg);
        assert_eq!(result, "[User]: Fix the bug in main.rs");
    }

    #[test]
    fn test_serializer_user_with_files() {
        let msg = serde_json::json!({
            "type": "user",
            "text": "Check this file",
            "files": [
                {"mime": "text/rust", "name": "main.rs", "uri": "file:///main.rs"}
            ]
        });
        let result = CompactionSerializer::serialize(&msg);
        assert!(result.contains("[User]: Check this file"));
        assert!(result.contains("[Attached text/rust: main.rs]"));
    }

    #[test]
    fn test_serializer_assistant_text() {
        let msg = serde_json::json!({
            "type": "assistant",
            "content": [
                {"type": "text", "text": "Here is the fix"}
            ]
        });
        let result = CompactionSerializer::serialize(&msg);
        assert!(result.contains("[Assistant]: Here is the fix"));
    }

    #[test]
    fn test_serializer_assistant_with_tool() {
        let msg = serde_json::json!({
            "type": "assistant",
            "content": [
                {"type": "text", "text": "Let me search"},
                {
                    "type": "tool",
                    "tool": "grep",
                    "state": {
                        "status": "completed",
                        "input": "fn main",
                        "content": [{"type": "text", "text": "fn main() {}"}]
                    }
                }
            ]
        });
        let result = CompactionSerializer::serialize(&msg);
        assert!(result.contains("[Assistant]: Let me search"));
        assert!(result.contains("[Assistant tool call]: grep(fn main)"));
        assert!(result.contains("[Tool result]: fn main() {}"));
    }

    #[test]
    fn test_serializer_system_message() {
        let msg = serde_json::json!({
            "type": "system",
            "text": "Switching to build agent"
        });
        let result = CompactionSerializer::serialize(&msg);
        assert_eq!(result, "[System update]: Switching to build agent");
    }

    #[test]
    fn test_serializer_shell_message() {
        let msg = serde_json::json!({
            "type": "shell",
            "command": "ls -la",
            "output": "file1.rs\nfile2.rs"
        });
        let result = CompactionSerializer::serialize(&msg);
        assert!(result.contains("[Shell]: ls -la"));
        assert!(result.contains("file1.rs"));
    }

    #[test]
    fn test_serializer_unknown_type() {
        let msg = serde_json::json!({"type": "unknown_type"});
        let result = CompactionSerializer::serialize(&msg);
        assert_eq!(result, "");
    }

    #[test]
    fn test_serialize_tool_content_text() {
        let content = serde_json::json!([
            {"type": "text", "text": "Hello world"}
        ]);
        let result = CompactionSerializer::serialize_tool_content(&content);
        assert_eq!(result, "Hello world");
    }

    #[test]
    fn test_serialize_tool_content_attachment() {
        let content = serde_json::json!([
            {"type": "file", "mime": "image/png", "name": "screenshot.png"}
        ]);
        let result = CompactionSerializer::serialize_tool_content(&content);
        assert_eq!(result, "[Attached image/png: screenshot.png]");
    }

    #[test]
    fn test_build_prompt_new_summary() {
        let context = vec!["[User]: Hello".to_string(), "[Assistant]: Hi".to_string()];
        let prompt = CompactionSerializer::build_prompt(None, &context);
        assert!(prompt.contains("Create a new anchored summary"));
        assert!(prompt.contains("## Goal"));
        assert!(prompt.contains("[User]: Hello"));
        assert!(prompt.contains("[Assistant]: Hi"));
    }

    #[test]
    fn test_build_prompt_update_summary() {
        let context = vec!["[User]: Fixed the bug".to_string()];
        let prompt =
            CompactionSerializer::build_prompt(Some("Previous summary"), &context);
        assert!(prompt.contains("Update the anchored summary"));
        assert!(prompt.contains("<previous-summary>"));
        assert!(prompt.contains("Previous summary"));
        assert!(prompt.contains("</previous-summary>"));
    }

    #[test]
    fn test_serialize_messages_filters_compaction() {
        let messages = vec![
            serde_json::json!({"type": "compaction", "summary": "old"}),
            serde_json::json!({"type": "user", "text": "Hello"}),
            serde_json::json!({"type": "assistant", "content": [{"type": "text", "text": "Hi"}]}),
        ];
        let result = CompactionSerializer::serialize_messages(&messages);
        assert_eq!(result.len(), 2);
        assert!(result[0].contains("[User]: Hello"));
        assert!(result[1].contains("[Assistant]: Hi"));
    }

    // ── CompactResult tests ────────────────────────────────────────

    #[test]
    fn test_compact_result_creation() {
        let result = CompactResult {
            summary: "Fixed the bug".to_string(),
            recent: "[User]: Continue".to_string(),
        };
        assert_eq!(result.summary, "Fixed the bug");
        assert_eq!(result.recent, "[User]: Continue");
    }

    // ── Token estimation tests ─────────────────────────────────────

    #[test]
    fn test_estimate_tokens_empty() {
        assert_eq!(estimate_tokens(""), 0);
    }

    #[test]
    fn test_estimate_tokens_short() {
        // "hello" = 5 chars, (5 + 2) / 4 = 1
        assert_eq!(estimate_tokens("hello"), 1);
    }

    #[test]
    fn test_estimate_tokens_long() {
        let text = "a".repeat(100);
        // (100 + 2) / 4 = 25
        assert_eq!(estimate_tokens(&text), 25);
    }
}
