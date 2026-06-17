//! Session prompt types — prompt construction input, template, and enums.
//!
//! Ported from:
//! - `packages/core/src/session/prompt.ts` (lines 1–47)
//! - `packages/opencode/src/session/prompt.ts` (lines 1594–1723)

use serde::{Deserialize, Serialize};
use crate::session_info::{ModelRef, SessionId};

// ══════════════════════════════════════════════════════════════════════════════
// Prompt Input
// ══════════════════════════════════════════════════════════════════════════════

/// The full input to `SessionPrompt.prompt()`.
///
/// # Source
/// `packages/opencode/src/session/prompt.ts` lines 1594–1616 `PromptInput`.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SessionPromptInput {
    pub session_id: SessionId,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub message_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub model: Option<ModelRef>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub agent: Option<String>,
    #[serde(default)]
    pub no_reply: bool,
    /// @deprecated tools and permissions merged — set permissions on session
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tools: Option<std::collections::HashMap<String, bool>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub format: Option<PromptFormat>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub system: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub variant: Option<String>,
    pub parts: Vec<PromptPart>,
}

/// Parts of a prompt input — text, file, agent, or subtask.
///
/// # Source
/// `packages/opencode/src/session/prompt.ts` lines 1607–1614 `parts`.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type")]
pub enum PromptPart {
    /// Plain text
    #[serde(rename = "text")]
    Text(PromptTextPart),

    /// File attachment
    #[serde(rename = "file")]
    File(PromptFilePart),

    /// Agent reference
    #[serde(rename = "agent")]
    Agent(PromptAgentPart),

    /// Subtask delegation
    #[serde(rename = "subtask")]
    Subtask(PromptSubtaskPart),
}

/// A text part in a prompt.
///
/// # Source
/// `packages/opencode/src/session/prompt.ts` — `TextPartInput`.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PromptTextPart {
    /// Optional ID (for updates)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub id: Option<String>,
    /// The text content
    pub text: String,
    /// Whether this is synthetic (system-generated)
    #[serde(default)]
    pub synthetic: bool,
}

/// A file part in a prompt.
///
/// # Source
/// `packages/opencode/src/session/prompt.ts` — `FilePartInput`.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PromptFilePart {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub id: Option<String>,
    pub mime: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub filename: Option<String>,
    pub url: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub source: Option<PromptFileSource>,
    #[serde(default)]
    pub synthetic: bool,
}

/// Source information for a file reference.
///
/// # Source
/// `packages/opencode/src/session/prompt.ts` lines 1651 — `FilePartSource`.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PromptFileSource {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub start: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub end: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub value: Option<String>,
}

/// An agent part in a prompt.
///
/// # Source
/// `packages/opencode/src/session/prompt.ts` — `AgentPartInput`.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PromptAgentPart {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub id: Option<String>,
    /// Agent name
    pub name: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub source: Option<PromptFileSource>,
    #[serde(default)]
    pub synthetic: bool,
}

/// A subtask part — delegates work to a subagent.
///
/// # Source
/// `packages/opencode/src/session/prompt.ts` — `SubtaskPartInput`.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PromptSubtaskPart {
    pub agent: String,
    pub description: String,
    pub prompt: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub command: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub model: Option<ModelRef>,
}

// ══════════════════════════════════════════════════════════════════════════════
// Format Types
// ══════════════════════════════════════════════════════════════════════════════

/// Output format for a prompt.
///
/// # Source
/// `packages/opencode/src/session/prompt.ts` line 1605 `format`.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type")]
pub enum PromptFormat {
    /// Plain text output
    #[serde(rename = "text")]
    Text,
    /// JSON schema structured output
    #[serde(rename = "json_schema")]
    JsonSchema { schema: serde_json::Value },
}

// ══════════════════════════════════════════════════════════════════════════════
// Shell / Command Input Types
// ══════════════════════════════════════════════════════════════════════════════

/// Input for running a shell command in a session.
///
/// # Source
/// `packages/opencode/src/session/prompt.ts` lines 1622–1628 `ShellInput`.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ShellInput {
    pub session_id: SessionId,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub message_id: Option<String>,
    pub agent: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub model: Option<ModelRef>,
    pub command: String,
}

/// Input for executing a command in a session.
///
/// # Source
/// `packages/opencode/src/session/prompt.ts` lines 1631–1656 `CommandInput`.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CommandInput {
    pub session_id: SessionId,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub message_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub agent: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub model: Option<String>,
    pub arguments: String,
    pub command: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub variant: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub parts: Option<Vec<PromptFilePart>>,
}

/// Input for the loop operation.
///
/// # Source
/// `packages/opencode/src/session/prompt.ts` lines 1618–1620 `LoopInput`.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LoopInput {
    pub session_id: SessionId,
}

// ══════════════════════════════════════════════════════════════════════════════
// Tests
// ══════════════════════════════════════════════════════════════════════════════

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_prompt_text_part() {
        let part = PromptPart::Text(PromptTextPart {
            id: Some("part_1".into()),
            text: "Hello".into(),
            synthetic: false,
        });
        let json = serde_json::to_string(&part).expect("serialize");
        assert!(json.contains(r#""type":"text""#));
        assert!(json.contains("Hello"));
    }

    #[test]
    fn test_prompt_file_part() {
        let part = PromptPart::File(PromptFilePart {
            id: None,
            mime: "text/plain".into(),
            filename: Some("main.rs".into()),
            url: "file:///tmp/main.rs".into(),
            source: Some(PromptFileSource {
                start: Some(1),
                end: Some(100),
                value: None,
            }),
            synthetic: false,
        });
        let json = serde_json::to_string(&part).expect("serialize");
        assert!(json.contains(r#""type":"file""#));
        assert!(json.contains("main.rs"));
    }

    #[test]
    fn test_prompt_agent_part() {
        let part = PromptPart::Agent(PromptAgentPart {
            id: None,
            name: "build".into(),
            source: None,
            synthetic: false,
        });
        let json = serde_json::to_string(&part).expect("serialize");
        assert!(json.contains(r#""type":"agent""#));
        assert!(json.contains("build"));
    }

    #[test]
    fn test_prompt_subtask_part() {
        let part = PromptPart::Subtask(PromptSubtaskPart {
            agent: "explore".into(),
            description: "Search for patterns".into(),
            prompt: "Find all TODO comments".into(),
            command: None,
            model: None,
        });
        let json = serde_json::to_string(&part).expect("serialize");
        assert!(json.contains(r#""type":"subtask""#));
        assert!(json.contains("explore"));
        assert!(json.contains("Find all TODO comments"));
    }

    #[test]
    fn test_session_prompt_input_minimal() {
        let input = SessionPromptInput {
            session_id: "ses_001".into(),
            message_id: None,
            model: None,
            agent: Some("build".into()),
            no_reply: false,
            tools: None,
            format: None,
            system: None,
            variant: None,
            parts: vec![PromptPart::Text(PromptTextPart {
                id: None,
                text: "Do something".into(),
                synthetic: false,
            })],
        };
        let json = serde_json::to_string(&input).expect("serialize");
        assert!(json.contains("ses_001"));
        assert!(json.contains("Do something"));
        let parsed: SessionPromptInput = serde_json::from_str(&json).expect("deserialize");
        assert_eq!(parsed.session_id, "ses_001");
        assert_eq!(parsed.parts.len(), 1);
    }

    #[test]
    fn test_prompt_format_text() {
        let fmt = PromptFormat::Text;
        let json = serde_json::to_string(&fmt).expect("serialize");
        assert!(json.contains(r#""type":"text""#));
    }

    #[test]
    fn test_prompt_format_json_schema() {
        let fmt = PromptFormat::JsonSchema {
            schema: serde_json::json!({
                "type": "object",
                "properties": {"name": {"type": "string"}}
            }),
        };
        let json = serde_json::to_string(&fmt).expect("serialize");
        assert!(json.contains("json_schema"));
        assert!(json.contains("properties"));
    }

    #[test]
    fn test_shell_input_full() {
        let input = ShellInput {
            session_id: "ses_001".into(),
            message_id: Some("msg_001".into()),
            agent: "build".into(),
            model: Some(ModelRef {
                id: "gpt-5".into(),
                provider_id: "openai".into(),
                variant: None,
            }),
            command: "ls -la".into(),
        };
        let json = serde_json::to_string(&input).expect("serialize");
        assert!(json.contains("ls -la"));
        assert!(json.contains("gpt-5"));
    }

    #[test]
    fn test_command_input() {
        let input = CommandInput {
            session_id: "ses_001".into(),
            message_id: None,
            agent: Some("build".into()),
            model: None,
            arguments: "arg1 arg2".into(),
            command: "test".into(),
            variant: None,
            parts: None,
        };
        let json = serde_json::to_string(&input).expect("serialize");
        assert!(json.contains("arg1 arg2"));
        assert!(json.contains("test"));
    }

    #[test]
    fn test_loop_input() {
        let input = LoopInput {
            session_id: "ses_loop".into(),
        };
        let json = serde_json::to_string(&input).expect("serialize");
        assert!(json.contains("ses_loop"));
    }
}
