//! Streaming tool-call JSON accumulator.
//!
//! Accumulates JSON fragments for in-progress tool calls during LLM streaming
//! and parses the completed JSON when the tool call input ends.
//!
//! Ported from:
//! - `packages/llm/src/protocols/utils/tool-stream.ts` (218 lines)
//!
//! OpenCode commit: 5d0f86606ac30690f79f0a6a9f41a1f49fe95d0b

use crate::provider::{ContentBlockId, LlmEvent, ToolCallId};
use std::collections::HashMap;

/// Accumulator state for a single in-progress tool call.
#[derive(Debug, Clone)]
struct Accumulator {
    /// Tool name
    name: String,
    /// Tool call ID
    id: ToolCallId,
    /// Accumulated JSON text
    json_text: String,
    /// Content block ID (for matching start/end)
    content_block_id: Option<ContentBlockId>,
}

/// Streaming tool-call JSON accumulator.
///
/// Supports multiple in-flight tool calls keyed by a provider-specific key
/// (numeric content block index for Anthropic/Bedrock, string for OpenAI Responses).
///
/// # Source
/// Ported from `packages/llm/src/protocols/utils/tool-stream.ts`.
#[derive(Debug, Clone, Default)]
pub struct ToolStreamAccumulator {
    tools: HashMap<u64, Accumulator>,
}

impl ToolStreamAccumulator {
    /// Create a new empty accumulator.
    pub fn new() -> Self {
        Self {
            tools: HashMap::new(),
        }
    }

    /// Start accumulating a new tool call.
    ///
    /// Called when the provider signals a new tool use block has started.
    /// The `id` and `name` may be partial at this point; they can be updated
    /// by subsequent calls to `set_identity`.
    pub fn start(&mut self, key: u64, name: impl AsRef<str>, id: ToolCallId) {
        self.tools.insert(
            key,
            Accumulator {
                name: name.as_ref().to_string(),
                id,
                json_text: String::new(),
                content_block_id: None,
            },
        );
    }

    /// Set or update the tool identity for an existing accumulator.
    ///
    /// Some protocols (OpenAI Chat) deliver the tool name/id on the first delta,
    /// not on a separate start event.
    pub fn set_identity(&mut self, key: u64, name: impl AsRef<str>, id: ToolCallId) {
        if let Some(acc) = self.tools.get_mut(&key) {
            acc.name = name.as_ref().to_string();
            acc.id = id;
        } else {
            self.start(key, name, id);
        }
    }

    /// Set the content block ID for a tool.
    pub fn set_content_block_id(&mut self, key: u64, content_block_id: ContentBlockId) {
        if let Some(acc) = self.tools.get_mut(&key) {
            acc.content_block_id = Some(content_block_id);
        }
    }

    /// Append a JSON fragment to an in-progress tool call.
    ///
    /// Returns `LlmEvent::ToolInputDelta` if the accumulator exists.
    pub fn append(&mut self, key: u64, delta: &str) -> Option<LlmEvent> {
        let acc = self.tools.get_mut(&key)?;
        acc.json_text.push_str(delta);
        Some(LlmEvent::ToolInputDelta {
            id: acc.id.clone(),
            name: acc.name.clone(),
            text: delta.to_string(),
        })
    }

    /// Get the name of a tool by key.
    pub fn name(&self, key: u64) -> Option<&str> {
        self.tools.get(&key).map(|a| a.name.as_str())
    }

    /// Finish a tool call — parse the accumulated JSON and return a `ToolCall` event.
    ///
    /// Returns `None` if no tool exists for this key.
    pub fn finish(&mut self, key: u64) -> Option<LlmEvent> {
        let acc = self.tools.remove(&key)?;

        // Parse the accumulated JSON text
        let input: serde_json::Value = if acc.json_text.trim().is_empty() {
            serde_json::Value::Object(serde_json::Map::new())
        } else {
            serde_json::from_str(&acc.json_text).unwrap_or_else(|_| {
                // If JSON parsing fails, wrap the raw text as a string
                serde_json::Value::String(acc.json_text.clone())
            })
        };

        Some(LlmEvent::ToolCall {
            id: acc.id,
            name: acc.name,
            input,
            provider_executed: None,
            provider_metadata: None,
        })
    }

    /// Finish all pending tool calls.
    ///
    /// Returns a vec of `ToolCall` events, one per pending tool.
    pub fn finish_all(&mut self) -> Vec<LlmEvent> {
        let keys: Vec<u64> = self.tools.keys().copied().collect();
        keys.into_iter().filter_map(|k| self.finish(k)).collect()
    }

    /// Check if any tool calls are pending.
    pub fn has_pending(&self) -> bool {
        !self.tools.is_empty()
    }

    /// Get the number of pending tool calls.
    pub fn pending_count(&self) -> usize {
        self.tools.len()
    }

    /// Get pending tool keys.
    pub fn pending_keys(&self) -> Vec<u64> {
        self.tools.keys().copied().collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_start_and_finish_simple() {
        let mut acc = ToolStreamAccumulator::new();
        acc.start(0, "bash", "toolu_01".to_string());
        acc.append(0, r#"{"command":""#);
        acc.append(0, r#"ls"}"#);

        let event = acc.finish(0).unwrap();
        if let LlmEvent::ToolCall {
            name, input, id, ..
        } = event
        {
            assert_eq!(name, "bash");
            assert_eq!(id, "toolu_01");
            assert_eq!(input["command"], "ls");
        } else {
            panic!("Expected ToolCall event, got {event:?}");
        }
    }

    #[test]
    fn test_finish_empty_json() {
        let mut acc = ToolStreamAccumulator::new();
        acc.start(0, "read", "toolu_02".to_string());
        // No append calls

        let event = acc.finish(0).unwrap();
        if let LlmEvent::ToolCall { input, .. } = event {
            assert_eq!(input, serde_json::json!({}));
        } else {
            panic!("Expected ToolCall event");
        }
    }

    #[test]
    fn test_append_returns_delta_event() {
        let mut acc = ToolStreamAccumulator::new();
        acc.start(0, "grep", "toolu_03".to_string());

        let delta = acc.append(0, r#"{"pattern":"#);
        assert!(delta.is_some());
        if let Some(LlmEvent::ToolInputDelta { text, .. }) = delta {
            assert_eq!(text, r#"{"pattern":"#);
        } else {
            panic!("Expected ToolInputDelta");
        }
    }

    #[test]
    fn test_finish_all() {
        let mut acc = ToolStreamAccumulator::new();
        acc.start(0, "bash", "t1".to_string());
        acc.append(0, r#"{"cmd":"ls"}"#);
        acc.start(1, "read", "t2".to_string());
        acc.append(1, r#"{"path":"/tmp"}"#);

        let events = acc.finish_all();
        assert_eq!(events.len(), 2);

        let names: Vec<String> = events
            .iter()
            .filter_map(|e| {
                if let LlmEvent::ToolCall { name, .. } = e {
                    Some(name.clone())
                } else {
                    None
                }
            })
            .collect();
        assert!(names.contains(&"bash".into()));
        assert!(names.contains(&"read".into()));
    }

    #[test]
    fn test_has_pending_and_count() {
        let mut acc = ToolStreamAccumulator::new();
        assert!(!acc.has_pending());
        assert_eq!(acc.pending_count(), 0);

        acc.start(0, "bash", "t1".to_string());
        assert!(acc.has_pending());
        assert_eq!(acc.pending_count(), 1);

        acc.start(1, "read", "t2".to_string());
        assert_eq!(acc.pending_count(), 2);

        acc.finish(0);
        assert_eq!(acc.pending_count(), 1);
    }

    #[test]
    fn test_set_identity_existing() {
        let mut acc = ToolStreamAccumulator::new();
        acc.start(0, "unknown", "t_unknown".to_string());
        acc.set_identity(0, "bash", "toolu_01".to_string());

        acc.append(0, r#"{"cmd":"pwd"}"#);
        let event = acc.finish(0).unwrap();
        if let LlmEvent::ToolCall { name, id, .. } = event {
            assert_eq!(name, "bash");
            assert_eq!(id, "toolu_01");
        } else {
            panic!("Expected ToolCall");
        }
    }

    #[test]
    fn test_set_identity_new() {
        let mut acc = ToolStreamAccumulator::new();
        acc.set_identity(5, "grep", "t_grep".to_string());
        assert_eq!(acc.pending_count(), 1);
        assert_eq!(acc.name(5), Some("grep"));
    }

    #[test]
    fn test_finish_malformed_json() {
        let mut acc = ToolStreamAccumulator::new();
        acc.start(0, "bash", "t1".to_string());
        acc.append(0, "{bad json that can't be parsed");

        let event = acc.finish(0).unwrap();
        if let LlmEvent::ToolCall { input, .. } = event {
            // Malformed JSON is wrapped as a string
            assert!(input.is_string());
        } else {
            panic!("Expected ToolCall event");
        }
    }

    #[test]
    fn test_content_block_id() {
        let mut acc = ToolStreamAccumulator::new();
        acc.start(0, "bash", "t1".to_string());
        acc.set_content_block_id(0, "bdrk_01".into());

        // The content_block_id is internal — verify through finish output
        let event = acc.finish(0).unwrap();
        let _ = event; // just verify it doesn't panic with the content block id set
    }
}
