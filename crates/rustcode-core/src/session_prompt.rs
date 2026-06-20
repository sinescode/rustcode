//! Session prompt types — prompt construction input, template, and enums.
//!
//! Ported from:
//! - `packages/core/src/session/prompt.ts` (lines 1–47)
//! - `packages/opencode/src/session/prompt.ts` (lines 1594–1723)

use crate::session_info::{ModelRef, SessionId};
use serde::{Deserialize, Serialize};

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
// Session Prompt Builder
// ══════════════════════════════════════════════════════════════════════════════

/// Builds the full LLM prompt from session state, instructions, and tool definitions.
///
/// Ported from:
/// - `packages/opencode/src/session/prompt.ts` (lines 1-1723)
pub struct SessionPromptBuilder {
    /// System prompt instructions
    system_instructions: Vec<String>,
    /// Tool definitions in human-readable form
    tool_descriptions: Vec<String>,
    /// Optional custom system prompt override
    custom_system: Option<String>,
}

impl SessionPromptBuilder {
    /// Create a new prompt builder.
    pub fn new() -> Self {
        Self {
            system_instructions: Vec::new(),
            tool_descriptions: Vec::new(),
            custom_system: None,
        }
    }

    /// Add a system instruction line.
    pub fn add_instruction(&mut self, instruction: impl Into<String>) {
        self.system_instructions.push(instruction.into());
    }

    /// Add a tool description.
    pub fn add_tool_description(&mut self, description: impl Into<String>) {
        self.tool_descriptions.push(description.into());
    }

    /// Set a custom system prompt (overrides default system prompt).
    pub fn set_custom_system(&mut self, system: impl Into<String>) {
        self.custom_system = Some(system.into());
    }

    /// Add instructions from a configuration file.
    ///
    /// Loads CLAUDE.md-style instruction files.
    pub fn add_instructions_from_file(&mut self, content: &str) {
        for line in content.lines() {
            let trimmed = line.trim();
            if !trimmed.is_empty() && !trimmed.starts_with('#') {
                self.add_instruction(trimmed);
            }
        }
    }

    /// Add instructions from a plugin definition.
    pub fn add_plugin_instructions(&mut self, plugin_name: &str, instructions: &[String]) {
        self.add_instruction(format!("## Plugin: {plugin_name}"));
        for instr in instructions {
            self.add_instruction(instr.as_str());
        }
    }

    /// Build the full system prompt string.
    ///
    /// Ported from: `packages/opencode/src/session/prompt.ts` — `buildPrompt`
    pub fn build_system_prompt(&self) -> String {
        if let Some(ref custom) = self.custom_system {
            return custom.clone();
        }

        let mut prompt = String::new();

        // Standard system instructions
        if !self.system_instructions.is_empty() {
            prompt.push_str(&self.system_instructions.join("\n"));
            prompt.push('\n');
        }

        // Tool descriptions
        if !self.tool_descriptions.is_empty() {
            prompt.push_str("\n## Available Tools\n\n");
            for desc in &self.tool_descriptions {
                prompt.push_str(desc);
                prompt.push('\n');
            }
        }

        prompt
    }

    /// Build the combined prompt (system + context + messages).
    ///
    /// Returns a list of instructions that can be passed to the LLM.
    ///
    /// Ported from: `packages/opencode/src/session/prompt.ts` — top-level `prompt()`
    pub fn build_prompt(
        &self,
        input: &SessionPromptInput,
        context: Option<&str>,
        messages: &[String],
    ) -> Vec<String> {
        let mut result: Vec<String> = Vec::new();

        // System prompt (always first)
        let system = self.build_system_prompt();
        if !system.is_empty() {
            result.push(system);
        }

        // Optional context (e.g., from compaction)
        if let Some(ctx) = context {
            if !ctx.is_empty() {
                result.push(format!("<context>\n{ctx}\n</context>"));
            }
        }

        // User input parts
        for part in &input.parts {
            match part {
                PromptPart::Text(t) => {
                    result.push(t.text.clone());
                }
                PromptPart::File(f) => {
                    result.push(format!(
                        "[File: {} ({})]",
                        f.filename.as_deref().unwrap_or("unnamed"),
                        f.mime
                    ));
                }
                PromptPart::Agent(a) => {
                    result.push(format!("[Agent: {}]", a.name));
                }
                PromptPart::Subtask(s) => {
                    result.push(format!("[Subtask: {} — {}]", s.agent, s.description));
                }
            }
        }

        // Message history
        for msg in messages {
            result.push(msg.clone());
        }

        result
    }

    /// Build tool descriptions from a tool registry (simplified).
    ///
    /// In production this would call into the ToolRegistry to get
    /// formatted descriptions of each available tool.
    pub fn assemble_tool_descriptions(
        &mut self,
        tools: &std::collections::HashMap<String, String>,
    ) {
        for (name, desc) in tools {
            self.add_tool_description(format!("- **{name}**: {desc}"));
        }
    }

    /// Get the current set of instructions.
    pub fn instructions(&self) -> &[String] {
        &self.system_instructions
    }

    /// Get the current tool descriptions.
    pub fn tool_descriptions(&self) -> &[String] {
        &self.tool_descriptions
    }

    /// Clear all instructions and tool descriptions.
    pub fn clear(&mut self) {
        self.system_instructions.clear();
        self.tool_descriptions.clear();
        self.custom_system = None;
    }
}

impl Default for SessionPromptBuilder {
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

    // ── SessionPromptBuilder tests ────────────────────────────────────

    #[test]
    fn test_prompt_builder_basic() {
        let mut builder = SessionPromptBuilder::new();
        builder.add_instruction("You are a helpful coding assistant.");
        builder.add_instruction("Always write tests.");

        let system = builder.build_system_prompt();
        assert!(system.contains("helpful coding assistant"));
        assert!(system.contains("Always write tests"));
    }

    #[test]
    fn test_prompt_builder_with_tools() {
        let mut builder = SessionPromptBuilder::new();
        builder.add_instruction("You are an assistant.");
        builder.add_tool_description("- **read_file**: Read a file from disk");
        builder.add_tool_description("- **write_file**: Write content to a file");

        let system = builder.build_system_prompt();
        assert!(system.contains("Available Tools"));
        assert!(system.contains("read_file"));
        assert!(system.contains("write_file"));
    }

    #[test]
    fn test_prompt_builder_custom_system() {
        let mut builder = SessionPromptBuilder::new();
        builder.add_instruction("Default instruction");
        builder.set_custom_system("Custom system override");

        let system = builder.build_system_prompt();
        assert_eq!(system, "Custom system override");
        // Custom override replaces all default instructions
    }

    #[test]
    fn test_prompt_builder_from_file() {
        let mut builder = SessionPromptBuilder::new();
        let claude_md = r#"# Project Instructions
Always use async/await.
Never use unwrap.
"#;
        builder.add_instructions_from_file(claude_md);

        let instructions = builder.instructions();
        assert_eq!(instructions.len(), 2);
        assert_eq!(instructions[0], "Always use async/await.");
        assert_eq!(instructions[1], "Never use unwrap.");
    }

    #[test]
    fn test_prompt_builder_plugin_instructions() {
        let mut builder = SessionPromptBuilder::new();
        builder.add_plugin_instructions(
            "my-plugin",
            &[
                "Use this plugin for database access.".into(),
                "Always sanitize inputs.".into(),
            ],
        );

        let instructions = builder.instructions();
        assert_eq!(instructions.len(), 3); // header + 2 instructions
        assert!(instructions[0].contains("my-plugin"));
    }

    #[test]
    fn test_prompt_builder_assemble_tools() {
        let mut builder = SessionPromptBuilder::new();
        let mut tools = std::collections::HashMap::new();
        tools.insert("search".to_string(), "Search the codebase".to_string());
        tools.insert("test".to_string(), "Run tests".to_string());
        builder.assemble_tool_descriptions(&tools);

        let descriptions = builder.tool_descriptions();
        assert_eq!(descriptions.len(), 2);
        assert!(descriptions.iter().any(|d| d.contains("search")));
    }

    #[test]
    fn test_prompt_builder_build_prompt() {
        let mut builder = SessionPromptBuilder::new();
        builder.add_instruction("You are an assistant.");

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
                text: "Fix the bug in main.rs".into(),
                synthetic: false,
            })],
        };

        let result = builder.build_prompt(&input, None, &[]);
        assert_eq!(result.len(), 2); // system prompt + user text
        assert!(result[0].contains("assistant"));
        assert_eq!(result[1], "Fix the bug in main.rs");
    }

    #[test]
    fn test_prompt_builder_build_prompt_with_context() {
        let mut builder = SessionPromptBuilder::new();
        builder.add_instruction("You are an assistant.");

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
            parts: vec![],
        };

        let context = "Previous session summary: Fixed authentication bug.";
        let result = builder.build_prompt(&input, Some(context), &[]);
        assert_eq!(result.len(), 2); // system + context
        assert!(result[1].contains("Previous session summary"));
    }

    #[test]
    fn test_prompt_builder_build_prompt_with_all_part_types() {
        let builder = SessionPromptBuilder::new();

        let input = SessionPromptInput {
            session_id: "ses_001".into(),
            message_id: None,
            model: None,
            agent: None,
            no_reply: false,
            tools: None,
            format: None,
            system: None,
            variant: None,
            parts: vec![
                PromptPart::Text(PromptTextPart {
                    id: None,
                    text: "Hello".into(),
                    synthetic: false,
                }),
                PromptPart::File(PromptFilePart {
                    id: None,
                    mime: "text/rust".into(),
                    filename: Some("main.rs".into()),
                    url: "file:///src/main.rs".into(),
                    source: None,
                    synthetic: false,
                }),
                PromptPart::Agent(PromptAgentPart {
                    id: None,
                    name: "build".into(),
                    source: None,
                    synthetic: false,
                }),
                PromptPart::Subtask(PromptSubtaskPart {
                    agent: "explore".into(),
                    description: "Find bugs".into(),
                    prompt: "Search for bugs in the codebase".into(),
                    command: None,
                    model: None,
                }),
            ],
        };

        let result = builder.build_prompt(&input, None, &[]);
        // Should have: 4 parts (system is empty, not pushed)
        assert_eq!(result.len(), 4);
        assert_eq!(result[0], "Hello");
        assert!(result[1].contains("main.rs"));
        assert!(result[2].contains("build"));
        assert!(result[3].contains("explore"));
    }

    #[test]
    fn test_prompt_builder_clear() {
        let mut builder = SessionPromptBuilder::new();
        builder.add_instruction("Something");
        builder.add_tool_description("Some tool");
        builder.set_custom_system("Custom");

        builder.clear();
        assert!(builder.instructions().is_empty());
        assert!(builder.tool_descriptions().is_empty());
    }

    #[test]
    fn test_prompt_builder_default() {
        let builder = SessionPromptBuilder::default();
        assert!(builder.instructions().is_empty());
        assert!(builder.tool_descriptions().is_empty());
    }
}
