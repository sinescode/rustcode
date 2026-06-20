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
