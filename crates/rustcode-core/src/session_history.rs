//! Session history types — history load, logging, and input types.
//!
//! Ported from:
//! - `packages/core/src/session/history.ts` (lines 1–102)
//! - `packages/core/src/session/logging.ts` (lines 1–8)
//! - `packages/core/src/session/input.ts` (lines 1–354)

use crate::session_info::SessionId;
use crate::session_message::{Prompt, SessionMessageId};
use serde::{Deserialize, Serialize};

// ══════════════════════════════════════════════════════════════════════════════
// History Entry
// ══════════════════════════════════════════════════════════════════════════════

/// A history entry — a message with its sequence number.
///
/// # Source
/// `packages/core/src/session/history.ts` lines 90–99 `entriesForRunner`.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HistoryEntry {
    /// Monotonic sequence number
    pub seq: u64,
    /// The message at this sequence
    pub message: serde_json::Value,
}

/// Context epoch — baseline for session context management.
///
/// # Source
/// `packages/core/src/session/history.ts` lines 58–79 — returns from
/// `SessionContextEpochTable` lookup.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ContextEpoch {
    pub session_id: SessionId,
    /// The baseline summary
    pub baseline: String,
    /// Agent active in this epoch
    pub agent: String,
    /// System context snapshot
    pub snapshot: serde_json::Value,
    /// Baseline sequence number
    pub baseline_seq: u64,
    /// Replacement sequence number
    #[serde(skip_serializing_if = "Option::is_none")]
    pub replacement_seq: Option<u64>,
    /// Revision counter
    #[serde(default)]
    pub revision: u64,
}

// ══════════════════════════════════════════════════════════════════════════════
// History Load Params
// ══════════════════════════════════════════════════════════════════════════════

/// Parameters for loading session history.
///
/// # Source
/// `packages/core/src/session/history.ts` lines 66–80 `load`, lines 82–89 `loadForRunner`.
#[derive(Debug, Clone)]
pub struct HistoryLoadParams {
    pub session_id: SessionId,
    /// Baseline sequence for runner context
    pub baseline_seq: Option<u64>,
}

// ══════════════════════════════════════════════════════════════════════════════
// Logging
// ══════════════════════════════════════════════════════════════════════════════

/// Log message context for session failures.
///
/// # Source
/// `packages/core/src/session/logging.ts` lines 4–8 `logFailure`.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SessionLogEntry {
    /// Log message type
    pub message: LogMessageType,
    /// Session ID
    pub session_id: SessionId,
    /// Error cause details
    pub cause: String,
}

/// Kinds of session log messages.
///
/// # Source
/// `packages/core/src/session/logging.ts` line 5 `message` union.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum LogMessageType {
    /// Failed to drain a session
    #[serde(rename = "Failed to drain Session")]
    DrainFailed,
    /// Failed to wake a session
    #[serde(rename = "Failed to wake Session")]
    WakeFailed,
}

// ══════════════════════════════════════════════════════════════════════════════
// Session Input Types
// ══════════════════════════════════════════════════════════════════════════════

/// Delivery mode for session input.
///
/// # Source
/// `packages/core/src/session/input.ts` line 18 `Delivery`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum InputDelivery {
    /// Steer — immediate/live input
    Steer,
    /// Queue — buffered for later processing
    Queue,
}

/// An admitted session input.
///
/// # Source
/// `packages/core/src/session/input.ts` lines 21–29 `Admitted`.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AdmittedInput {
    /// Admission sequence number
    pub admitted_seq: u64,
    /// Message ID
    pub id: SessionMessageId,
    /// Owning session
    pub session_id: SessionId,
    /// The prompt
    pub prompt: Prompt,
    /// Delivery mode
    pub delivery: InputDelivery,
    /// When the input was created
    pub time_created: u64,
    /// When promoted (optional)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub promoted_seq: Option<u64>,
}

/// Lifecycle conflict error when input IDs collide.
///
/// # Source
/// `packages/core/src/session/input.ts` lines 50–52 `LifecycleConflict`.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LifecycleConflict {
    pub id: SessionMessageId,
}

impl std::fmt::Display for LifecycleConflict {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "Lifecycle conflict for input: {}", self.id)
    }
}

impl std::error::Error for LifecycleConflict {}

/// Parameters for promoting steers (cutoff-based).
///
/// # Source
/// `packages/core/src/session/input.ts` lines 300–321 `promoteSteers`.
#[derive(Debug, Clone)]
pub struct PromoteSteersParams {
    pub session_id: SessionId,
    /// Only promote inputs with admitted_seq <= cutoff
    pub cutoff: u64,
}

/// Parameters for admitting a new input.
///
/// # Source
/// `packages/core/src/session/input.ts` lines 54–93 `admit`.
#[derive(Debug, Clone)]
pub struct AdmitInputParams {
    pub id: SessionMessageId,
    pub session_id: SessionId,
    pub prompt: Prompt,
    pub delivery: InputDelivery,
}

/// Parameters for projecting a legacy prompted input.
///
/// # Source
/// `packages/core/src/session/input.ts` lines 242–270 `projectLegacyPrompted`.
#[derive(Debug, Clone)]
pub struct LegacyPromptedParams {
    pub id: SessionMessageId,
    pub session_id: SessionId,
    pub prompt: Prompt,
    pub delivery: InputDelivery,
    pub time_created: u64,
    pub promoted_seq: u64,
}

// ══════════════════════════════════════════════════════════════════════════════
// Session History Service
// ══════════════════════════════════════════════════════════════════════════════

/// Service for managing session message history.
///
/// Handles appending messages, replaying history, and context epoch management.
///
/// Ported from:
/// - `packages/core/src/session/history.ts` (lines 1-102)
pub struct SessionHistory {
    messages: Vec<HistoryEntry>,
    epoch: Option<ContextEpoch>,
}

impl SessionHistory {
    /// Create a new empty history.
    pub fn new() -> Self {
        Self {
            messages: Vec::new(),
            epoch: None,
        }
    }

    /// Append a message to the history.
    ///
    /// Returns the sequence number assigned to this message.
    ///
    /// Ported from: `packages/core/src/session/history.ts` — append logic
    pub fn append(&mut self, message: serde_json::Value) -> u64 {
        let seq = self.next_seq();
        self.messages.push(HistoryEntry { seq, message });
        seq
    }

    /// Get the next sequence number.
    fn next_seq(&self) -> u64 {
        self.messages.last().map(|e| e.seq + 1).unwrap_or(1)
    }

    /// Replay history as an iterator of (seq, message) pairs.
    ///
    /// Ported from: `packages/core/src/session/history.ts` — replay/iterator pattern
    pub fn replay(&self) -> impl Iterator<Item = &HistoryEntry> {
        self.messages.iter()
    }

    /// Replay messages starting from a specific sequence number.
    pub fn replay_from(&self, from_seq: u64) -> impl Iterator<Item = &HistoryEntry> {
        self.messages.iter().filter(move |e| e.seq >= from_seq)
    }

    /// Get the current epoch.
    pub fn epoch(&self) -> Option<&ContextEpoch> {
        self.epoch.as_ref()
    }

    /// Set or update the context epoch.
    ///
    /// The epoch establishes a baseline for context management.
    ///
    /// Ported from: `packages/core/src/session/history.ts` — epoch management
    pub fn set_epoch(&mut self, epoch: ContextEpoch) {
        self.epoch = Some(epoch);
    }

    /// Clear the epoch (e.g., after compaction).
    pub fn clear_epoch(&mut self) {
        self.epoch = None;
    }

    /// Get the total number of messages in history.
    pub fn len(&self) -> usize {
        self.messages.len()
    }

    /// Check if the history is empty.
    pub fn is_empty(&self) -> bool {
        self.messages.is_empty()
    }

    /// Get the latest sequence number.
    pub fn latest_seq(&self) -> Option<u64> {
        self.messages.last().map(|e| e.seq)
    }

    /// Get the latest message.
    pub fn latest_message(&self) -> Option<&serde_json::Value> {
        self.messages.last().map(|e| &e.message)
    }

    /// Load history from a vector of messages with auto-assigned seq numbers.
    pub fn load(&mut self, messages: Vec<serde_json::Value>) {
        self.messages.clear();
        for msg in messages {
            self.append(msg);
        }
    }

    /// Get messages suitable for a runner context (new since baseline).
    pub fn runner_context(&self, baseline_seq: Option<u64>) -> Vec<&HistoryEntry> {
        match baseline_seq {
            Some(seq) => self.messages.iter().filter(|e| e.seq > seq).collect(),
            None => self.messages.iter().collect(),
        }
    }


    /// Load messages for a runner context (since a baseline sequence).
    ///
    /// Filters messages to only those with seq > baseline_seq, applying
    /// compaction-aware filtering: if a compaction message exists, all messages
    /// with seq >= the compaction's seq are included.
    ///
    /// # Source
    /// `packages/core/src/session/history.ts` lines 82–88 `loadForRunner`.
    pub fn load_for_runner(&mut self, messages: Vec<serde_json::Value>, baseline_seq: u64) -> Vec<serde_json::Value> {
        let mut result = Vec::new();
        let mut compaction_seq: Option<u64> = None;

        // First pass: find compaction seq
        for msg in &messages {
            if msg.get("type").and_then(|t| t.as_str()) == Some("compaction") {
                if let Some(seq) = msg.get("seq").and_then(|s| s.as_u64()) {
                    compaction_seq = Some(seq);
                }
            }
        }

        // Second pass: filter messages
        for msg in messages {
            let seq = msg.get("seq").and_then(|s| s.as_u64()).unwrap_or(0);
            let msg_type = msg.get("type").and_then(|t| t.as_str()).unwrap_or("");

            let should_include = if let Some(comp_seq) = compaction_seq {
                // Compaction-aware: include messages >= compaction seq,
                // or system messages > baseline_seq
                if seq >= comp_seq {
                    true
                } else if msg_type == "system" && seq > baseline_seq {
                    true
                } else {
                    false
                }
            } else {
                // No compaction: include messages > baseline_seq
                // Always exclude system messages <= baseline_seq
                if msg_type == "system" && seq <= baseline_seq {
                    false
                } else {
                    seq > baseline_seq
                }
            };

            if should_include {
                result.push(msg);
            }
        }

        // Load into history for further processing
        self.load(result.clone());
        result
    }

    /// Get entries (seq + message) for a runner context.
    ///
    /// Returns history entries with sequence numbers, filtered by compaction-awareness.
    ///
    /// # Source
    /// `packages/core/src/session/history.ts` lines 90–99 `entriesForRunner`.
    pub fn entries_for_runner(
        &mut self,
        messages: Vec<serde_json::Value>,
        baseline_seq: u64,
    ) -> Vec<HistoryEntry> {
        let filtered = self.load_for_runner(messages, baseline_seq);
        self.messages.iter()
            .filter(|e| filtered.iter().any(|m| {
                m.get("seq").and_then(|s| s.as_u64()) == Some(e.seq)
            }))
            .cloned()
            .collect()
    }

    /// Filter compacted messages — reorder for model consumption.
    ///
    /// When compaction has occurred, messages are reordered so that the
    /// [compaction-user, summary, retained-tail, continue-user] sequence
    /// is presented to the model in the correct order.
    ///
    /// # Source
    /// `packages/opencode/src/session/message-v2.ts` lines 532–583 `filterCompacted`.
    pub fn filter_compacted(&self) -> Vec<&HistoryEntry> {
        if self.messages.is_empty() {
            return Vec::new();
        }

        // Find compaction messages
        let mut compaction_indices: Vec<usize> = Vec::new();
        for (i, entry) in self.messages.iter().enumerate() {
            if entry.message.get("type").and_then(|t| t.as_str()) == Some("compaction") {
                compaction_indices.push(i);
            }
        }

        if compaction_indices.is_empty() {
            return self.messages.iter().collect();
        }

        // Apply filter: keep compaction user + summary + tail after compaction + remaining
        // Find the last compaction
        let last_compaction_idx = *compaction_indices.last().unwrap();

        // Find the compaction user message (the user message with the compaction part)
        let mut compaction_user_idx = None;
        for (i, entry) in self.messages.iter().enumerate() {
            if i >= last_compaction_idx {
                break;
            }
            if entry.message.get("type").and_then(|t| t.as_str()) == Some("user") {
                // Check if this user has a compaction part
                if entry.message.get("parts").and_then(|p| p.as_array()).map_or(false, |parts| {
                    parts.iter().any(|p| p.get("type").and_then(|t| t.as_str()) == Some("compaction"))
                }) {
                    compaction_user_idx = Some(i);
                }
            }
        }

        // Find the summary assistant (assistant after compaction user with summary flag)
        let mut summary_idx = None;
        if let Some(comp_user_idx) = compaction_user_idx {
            for (i, entry) in self.messages.iter().enumerate() {
                if i > comp_user_idx {
                    if entry.message.get("type").and_then(|t| t.as_str()) == Some("assistant") {
                        if entry.message.get("summary").and_then(|s| s.as_bool()).unwrap_or(false) {
                            summary_idx = Some(i);
                            break;
                        }
                    }
                }
            }
        }

        // Find tail_start_id from the compaction part
        let tail_start_id = if let Some(comp_user_idx) = compaction_user_idx {
            self.messages[comp_user_idx].message.get("parts").and_then(|p| p.as_array()).and_then(|parts| {
                parts.iter().find(|p| p.get("type").and_then(|t| t.as_str()) == Some("compaction"))
                    .and_then(|p| p.get("tail_start_id").and_then(|t| t.as_str()))
            })
        } else {
            None
        };

        // Find tail index
        let tail_idx = tail_start_id.and_then(|id| {
            self.messages.iter().position(|e| {
                e.message.get("id").and_then(|i| i.as_str()) == Some(id)
            })
        });

        // Build reordered result
        if let (Some(comp_user_idx), Some(summary_idx), Some(tail_idx)) = (compaction_user_idx, summary_idx, tail_idx) {
            if tail_idx < comp_user_idx && summary_idx > comp_user_idx {
                // Reorder: [compaction-user, summary], [tail..compaction], [summary+1..]
                let mut result = Vec::new();

                // Part 1: compaction user through summary
                for i in comp_user_idx..=summary_idx {
                    result.push(&self.messages[i]);
                }

                // Part 2: tail through compaction (exclusive of compaction user)
                for i in tail_idx..comp_user_idx {
                    result.push(&self.messages[i]);
                }

                // Part 3: everything after summary
                for i in (summary_idx + 1)..self.messages.len() {
                    result.push(&self.messages[i]);
                }

                return result;
            }
        }

        // Fallthrough: return all messages as-is
        self.messages.iter().collect()
    }

    /// Convert messages to AI SDK model messages format.
    ///
    /// Converts session messages (WithParts) into the ModelMessage format
    /// suitable for LLM provider consumption.
    ///
    /// # Source
    /// `packages/opencode/src/session/message-v2.ts` lines 142–434 `toModelMessagesEffect`.
    pub fn to_model_messages(&self) -> Vec<serde_json::Value> {
        let mut result: Vec<serde_json::Value> = Vec::new();

        // First apply compaction filtering
        let entries = self.filter_compacted();

        for entry in &entries {
            let msg = &entry.message;
            let msg_type = msg.get("type").and_then(|t| t.as_str()).unwrap_or("");

            match msg_type {
                "user" => {
                    let parts = msg.get("parts").and_then(|p| p.as_array()).map(|a| a.to_vec()).unwrap_or_default();
                    if parts.is_empty() {
                        continue;
                    }

                    let mut user_parts: Vec<serde_json::Value> = Vec::new();
                    for part in &parts {
                        let part_type = part.get("type").and_then(|t| t.as_str()).unwrap_or("");
                        match part_type {
                            "text" => {
                                if let Some(text) = part.get("text").and_then(|t| t.as_str()) {
                                    if !text.is_empty() {
                                        user_parts.push(serde_json::json!({"type": "text", "text": text}));
                                    }
                                }
                            }
                            "file" => {
                                let mime = part.get("mime").and_then(|m| m.as_str()).unwrap_or("");
                                if mime != "text/plain" && mime != "application/x-directory" {
                                    user_parts.push(serde_json::json!({
                                        "type": "file",
                                        "url": part.get("url"),
                                        "mediaType": mime,
                                        "filename": part.get("filename"),
                                    }));
                                }
                            }
                            "compaction" => {
                                user_parts.push(serde_json::json!({
                                    "type": "text",
                                    "text": "What did we do so far?"
                                }));
                            }
                            "subtask" => {
                                user_parts.push(serde_json::json!({
                                    "type": "text",
                                    "text": "The following tool was executed by the user"
                                }));
                            }
                            _ => {}
                        }
                    }

                    if !user_parts.is_empty() {
                        result.push(serde_json::json!({
                            "role": "user",
                            "parts": user_parts,
                        }));
                    }
                }
                "assistant" => {
                    let parts = msg.get("parts").and_then(|p| p.as_array()).map(|a| a.to_vec()).unwrap_or_default();

                    let mut assistant_parts: Vec<serde_json::Value> = Vec::new();
                    let mut media: Vec<serde_json::Value> = Vec::new();

                    for part in &parts {
                        let part_type = part.get("type").and_then(|t| t.as_str()).unwrap_or("");
                        match part_type {
                            "text" => {
                                if let Some(text) = part.get("text").and_then(|t| t.as_str()) {
                                    assistant_parts.push(serde_json::json!({
                                        "type": "text",
                                        "text": if text.is_empty() { " " } else { text },
                                        "providerMetadata": part.get("metadata"),
                                    }));
                                }
                            }
                            "step-start" => {
                                assistant_parts.push(serde_json::json!({"type": "step-start"}));
                            }
                            "tool" => {
                                if let Some(state) = part.get("state") {
                                    if let Some(status) = state.get("status").and_then(|s| s.as_str()) {
                                        let tool_name = part.get("tool").and_then(|t| t.as_str()).unwrap_or("unknown");
                                        let tool_type = format!("tool-{}", tool_name);
                                        let tool_part = serde_json::json!({
                                            "type": tool_type,
                                            "toolCallId": part.get("call_id"),
                                            "input": state.get("input"),
                                        });
                                        assistant_parts.push(tool_part);
                                    }
                                }
                            }
                            "reasoning" => {
                                assistant_parts.push(serde_json::json!({
                                    "type": "reasoning",
                                    "text": part.get("text"),
                                    "providerMetadata": part.get("metadata"),
                                }));
                            }
                            _ => {}
                        }
                    }

                    if !assistant_parts.is_empty() {
                        result.push(serde_json::json!({
                            "role": "assistant",
                            "parts": assistant_parts,
                        }));
                    }
                }
                // Skip non-conversation types
                _ => {}
            }
        }

        result
    }
    /// Clear all history.
    pub fn clear(&mut self) {
        self.messages.clear();
        self.epoch = None;
    }
}

impl Default for SessionHistory {
    fn default() -> Self {
        Self::new()
    }
}

// ══════════════════════════════════════════════════════════════════════════════
// Tests
// ══════════════════════════════════════════════════════════════════════════════

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_history_entry_roundtrip() {
        let entry = HistoryEntry {
            seq: 42,
            message: serde_json::json!({"type": "user", "text": "hello"}),
        };
        let json = serde_json::to_string(&entry).expect("serialize");
        let parsed: HistoryEntry = serde_json::from_str(&json).expect("deserialize");
        assert_eq!(parsed.seq, 42);
        assert_eq!(parsed.message["type"], "user");
    }

    #[test]
    fn test_context_epoch_serialization() {
        let epoch = ContextEpoch {
            session_id: "ses_001".into(),
            baseline: "Summary of previous work".into(),
            agent: "build".into(),
            snapshot: serde_json::json!({"files": ["src/main.rs"]}),
            baseline_seq: 10,
            replacement_seq: None,
            revision: 3,
        };
        let json = serde_json::to_string(&epoch).expect("serialize");
        assert!(json.contains("ses_001"));
        assert!(json.contains("Summary of previous work"));
        let parsed: ContextEpoch = serde_json::from_str(&json).expect("deserialize");
        assert_eq!(parsed.revision, 3);
    }

    #[test]
    fn test_log_message_type_serialization() {
        assert_eq!(
            serde_json::to_string(&LogMessageType::DrainFailed).expect("serialize"),
            r#""Failed to drain Session""#
        );
        assert_eq!(
            serde_json::to_string(&LogMessageType::WakeFailed).expect("serialize"),
            r#""Failed to wake Session""#
        );
    }

    #[test]
    fn test_session_log_entry() {
        let entry = SessionLogEntry {
            message: LogMessageType::DrainFailed,
            session_id: "ses_001".into(),
            cause: "Provider timeout".into(),
        };
        let json = serde_json::to_string(&entry).expect("serialize");
        assert!(json.contains("Failed to drain Session"));
        assert!(json.contains("Provider timeout"));
    }

    #[test]
    fn test_input_delivery_serialization() {
        assert_eq!(
            serde_json::to_string(&InputDelivery::Steer).expect("serialize"),
            r#""steer""#
        );
        assert_eq!(
            serde_json::to_string(&InputDelivery::Queue).expect("serialize"),
            r#""queue""#
        );
    }

    #[test]
    fn test_admitted_input_full() {
        let input = AdmittedInput {
            admitted_seq: 5,
            id: "msg_001".into(),
            session_id: "ses_001".into(),
            prompt: Prompt {
                text: "Fix the bug".into(),
                files: None,
                agents: None,
            },
            delivery: InputDelivery::Steer,
            time_created: 1700000000000,
            promoted_seq: Some(6),
        };
        let json = serde_json::to_string(&input).expect("serialize");
        assert!(json.contains("msg_001"));
        assert!(json.contains("steer"));
        assert!(json.contains("Fix the bug"));
    }

    #[test]
    fn test_admitted_input_no_promotion() {
        let input = AdmittedInput {
            admitted_seq: 5,
            id: "msg_001".into(),
            session_id: "ses_001".into(),
            prompt: Prompt {
                text: "Hello".into(),
                files: None,
                agents: None,
            },
            delivery: InputDelivery::Queue,
            time_created: 1700000000000,
            promoted_seq: None,
        };
        let json = serde_json::to_string(&input).expect("serialize");
        assert!(!json.contains("promoted_seq"));
    }

    #[test]
    fn test_lifecycle_conflict_display() {
        let conflict = LifecycleConflict {
            id: "msg_conflict".into(),
        };
        assert_eq!(
            conflict.to_string(),
            "Lifecycle conflict for input: msg_conflict"
        );
    }

    #[test]
    fn test_session_history_append() {
        let mut history = SessionHistory::new();
        let seq = history.append(serde_json::json!({"type": "user", "text": "hello"}));
        assert_eq!(seq, 1);

        let seq2 = history.append(serde_json::json!({"type": "assistant", "text": "hi"}));
        assert_eq!(seq2, 2);
        assert_eq!(history.len(), 2);
    }

    #[test]
    fn test_session_history_replay() {
        let mut history = SessionHistory::new();
        history.append(serde_json::json!({"type": "user", "text": "msg1"}));
        history.append(serde_json::json!({"type": "user", "text": "msg2"}));

        let msgs: Vec<_> = history.replay().collect();
        assert_eq!(msgs.len(), 2);
        assert_eq!(msgs[0].seq, 1);
        assert_eq!(msgs[1].seq, 2);
    }

    #[test]
    fn test_session_history_replay_from() {
        let mut history = SessionHistory::new();
        history.append(serde_json::json!({"text": "a"}));
        history.append(serde_json::json!({"text": "b"}));
        history.append(serde_json::json!({"text": "c"}));

        let from_2: Vec<_> = history.replay_from(2).collect();
        assert_eq!(from_2.len(), 2);
        assert_eq!(from_2[0].message["text"], "b");
        assert_eq!(from_2[1].message["text"], "c");
    }

    #[test]
    fn test_session_history_epoch() {
        let mut history = SessionHistory::new();
        assert!(history.epoch().is_none());

        let epoch = ContextEpoch {
            session_id: "ses_001".into(),
            baseline: "Summary...".into(),
            agent: "build".into(),
            snapshot: serde_json::json!({}),
            baseline_seq: 5,
            replacement_seq: None,
            revision: 1,
        };
        history.set_epoch(epoch);
        assert!(history.epoch().is_some());
        assert_eq!(history.epoch().unwrap().baseline_seq, 5);

        history.clear_epoch();
        assert!(history.epoch().is_none());
    }

    #[test]
    fn test_session_history_load() {
        let mut history = SessionHistory::new();
        history.load(vec![
            serde_json::json!({"text": "first"}),
            serde_json::json!({"text": "second"}),
        ]);
        assert_eq!(history.len(), 2);
        assert_eq!(history.latest_seq(), Some(2));
    }

    #[test]
    fn test_session_history_empty() {
        let history = SessionHistory::new();
        assert!(history.is_empty());
        assert_eq!(history.len(), 0);
        assert!(history.latest_seq().is_none());
        assert!(history.latest_message().is_none());
    }

    #[test]
    fn test_session_history_runner_context() {
        let mut history = SessionHistory::new();
        for i in 1..=5 {
            history.append(serde_json::json!({"seq": i}));
        }

        let ctx = history.runner_context(Some(3));
        assert_eq!(ctx.len(), 2); // seq 4 and 5
        assert_eq!(ctx[0].seq, 4);
        assert_eq!(ctx[1].seq, 5);
    }

    #[test]
    fn test_session_history_clear() {
        let mut history = SessionHistory::new();
        history.append(serde_json::json!({"text": "msg"}));
        history.clear();
        assert!(history.is_empty());
        assert!(history.epoch().is_none());
    }

    #[test]
    fn test_session_history_default() {
        let history = SessionHistory::default();
        assert!(history.is_empty());
    }
}

    #[test]
    fn test_load_for_runner_with_compaction() {
        let mut history = SessionHistory::new();
        let messages = vec![
            serde_json::json!({"seq": 1, "type": "user", "text": "hello", "id": "msg_1"}),
            serde_json::json!({"seq": 2, "type": "assistant", "text": "hi", "id": "msg_2"}),
            serde_json::json!({"seq": 3, "type": "compaction", "summary": "worked", "id": "msg_3"}),
            serde_json::json!({"seq": 4, "type": "user", "text": "continue", "id": "msg_4"}),
            serde_json::json!({"seq": 5, "type": "assistant", "text": "ok", "id": "msg_5"}),
        ];
        let result = history.load_for_runner(messages, 0);
        // Should include messages >= compaction seq (3) and system > baseline
        // All messages from seq 3 onward should be included
        assert_eq!(result.len(), 3);
        assert_eq!(result[0]["seq"], 3);
        assert_eq!(result[2]["seq"], 5);
    }

    #[test]
    fn test_load_for_runner_without_compaction() {
        let mut history = SessionHistory::new();
        let messages = vec![
            serde_json::json!({"seq": 1, "type": "user", "text": "hello", "id": "msg_1"}),
            serde_json::json!({"seq": 2, "type": "assistant", "text": "hi", "id": "msg_2"}),
            serde_json::json!({"seq": 3, "type": "user", "text": "more", "id": "msg_3"}),
        ];
        let result = history.load_for_runner(messages, 1);
        // Should include messages with seq > 1 (seq 2 and 3)
        assert_eq!(result.len(), 2);
        assert_eq!(result[0]["seq"], 2);
        assert_eq!(result[1]["seq"], 3);
    }

    #[test]
    fn test_entries_for_runner() {
        let mut history = SessionHistory::new();
        let messages = vec![
            serde_json::json!({"seq": 1, "type": "user", "text": "hello"}),
            serde_json::json!({"seq": 2, "type": "assistant", "text": "hi"}),
        ];
        let entries = history.entries_for_runner(messages, 0);
        assert_eq!(entries.len(), 2);
        assert_eq!(entries[0].seq, 1);
        assert_eq!(entries[1].seq, 2);
    }

    #[test]
    fn test_filter_compacted_empty() {
        let history = SessionHistory::new();
        let result = history.filter_compacted();
        assert!(result.is_empty());
    }

    #[test]
    fn test_filter_compacted_no_compaction() {
        let mut history = SessionHistory::new();
        history.append(serde_json::json!({"type": "user", "text": "hello"}));
        history.append(serde_json::json!({"type": "assistant", "text": "hi"}));
        let result = history.filter_compacted();
        assert_eq!(result.len(), 2);
    }

    #[test]
    fn test_to_model_messages_user() {
        let mut history = SessionHistory::new();
        history.append(serde_json::json!({
            "type": "user",
            "parts": [
                {"type": "text", "text": "Hello, can you help?"}
            ]
        }));
        let result = history.to_model_messages();
        assert_eq!(result.len(), 1);
        assert_eq!(result[0]["role"], "user");
    }

    #[test]
    fn test_to_model_messages_assistant() {
        let mut history = SessionHistory::new();
        history.append(serde_json::json!({
            "type": "assistant",
            "parts": [
                {"type": "text", "text": "I can help!"}
            ]
        }));
        let result = history.to_model_messages();
        assert_eq!(result.len(), 1);
        assert_eq!(result[0]["role"], "assistant");
    }

    #[test]
    fn test_to_model_messages_skip_non_conversation() {
        let mut history = SessionHistory::new();
        history.append(serde_json::json!({"type": "system", "text": "system update"}));
        history.append(serde_json::json!({"type": "user", "parts": [{"type": "text", "text": "hello"}]}));
        let result = history.to_model_messages();
        assert_eq!(result.len(), 1);
        assert_eq!(result[0]["role"], "user");
    }

