//! # Context building
//!
//! Assembles the provider request context from session state.
//! This includes: system prompt, tool schemas, message history, permission policies.

use serde::{Deserialize, Serialize};
use std::fmt;

/// The assembled context ready to send to a provider.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProviderContext {
    /// System prompt (compiled from agent definition + settings).
    pub system_prompt: String,

    /// Tool definitions (JSON schemas).
    pub tools: Vec<ToolDefinition>,

    /// Conversation messages (user + assistant + tool results).
    pub messages: Vec<ContextMessage>,

    /// Token count estimate.
    pub estimated_tokens: usize,
}

/// A single message in the conversation context.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ContextMessage {
    /// Message role: user, assistant, or tool.
    pub role: MessageRole,

    /// Message content.
    pub content: MessageContent,

    /// Optional message ID for reference.
    pub id: Option<String>,
}

/// Message role.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum MessageRole {
    /// User message.
    User,
    /// Assistant message.
    Assistant,
    /// Tool result message.
    Tool,
}

/// Message content — text or structured.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(untagged)]
pub enum MessageContent {
    /// Plain text.
    Text(String),
    /// Structured array (for multi-modal / tool calls).
    Content(Vec<ContentBlock>),
}

/// A content block within a message.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type")]
pub enum ContentBlock {
    /// Text content.
    #[serde(rename = "text")]
    Text {
        /// The text content.
        text: String,
    },
    /// Tool call.
    #[serde(rename = "tool_call")]
    ToolCall {
        /// Tool call ID.
        id: String,
        /// Tool name.
        name: String,
        /// Tool arguments.
        input: serde_json::Value,
    },
    /// Tool result.
    #[serde(rename = "tool_result")]
    ToolResult {
        /// Tool call ID.
        id: String,
        /// Tool output.
        content: String,
        /// Whether the tool failed.
        is_error: bool,
    },
}

/// A tool definition (JSON schema form).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ToolDefinition {
    /// Tool name.
    pub name: String,
    /// Human-readable description.
    pub description: String,
    /// JSON schema for tool inputs.
    pub input_schema: serde_json::Value,
}

/// Context building options.
#[derive(Debug, Clone)]
pub struct ContextOptions {
    /// Maximum number of recent messages to include.
    pub max_messages: usize,
    /// Target token budget for the context.
    pub target_tokens: usize,
    /// Include system prompt.
    pub include_system: bool,
    /// Include tool definitions.
    pub include_tools: bool,
}

impl Default for ContextOptions {
    fn default() -> Self {
        Self {
            max_messages: 100,
            target_tokens: 100_000,
            include_system: true,
            include_tools: true,
        }
    }
}

/// Build a provider context from messages and configuration.
pub fn build_context(
    system_prompt: &str,
    tools: &[ToolDefinition],
    messages: &[ContextMessage],
    options: &ContextOptions,
) -> ProviderContext {
    let recent = if messages.len() > options.max_messages {
        &messages[messages.len() - options.max_messages..]
    } else {
        messages
    };

    // Estimate tokens: very rough heuristic (4 chars ≈ 1 token)
    let estimated_tokens = options.system_prompt_estimate(system_prompt)
        + options.tools_estimate(tools)
        + recent.iter().map(|m| m.content.estimate_tokens()).sum::<usize>();

    ProviderContext {
        system_prompt: if options.include_system { system_prompt.to_string() } else { String::new() },
        tools: if options.include_tools { tools.to_vec() } else { Vec::new() },
        messages: recent.to_vec(),
        estimated_tokens,
    }
}

/// Trait for estimating token counts.
pub trait TokenEstimator {
    /// Rough estimate of the token count for this item.
    fn estimate_tokens(&self) -> usize;
}

impl TokenEstimator for str {
    fn estimate_tokens(&self) -> usize {
        // ~4 characters per token for English text
        (self.len() + 3) / 4
    }
}

impl TokenEstimator for ContextMessage {
    fn estimate_tokens(&self) -> usize {
        match &self.content {
            MessageContent::Text(t) => t.estimate_tokens(),
            MessageContent::Content(blocks) => {
                blocks.iter().map(|b| b.estimate_tokens()).sum()
            }
        }
    }
}

impl TokenEstimator for ContentBlock {
    fn estimate_tokens(&self) -> usize {
        match self {
            ContentBlock::Text { text } => text.estimate_tokens(),
            ContentBlock::ToolCall { input, .. } => {
                let json_str = serde_json::to_string(input).unwrap_or_default();
                json_str.estimate_tokens()
            }
            ContentBlock::ToolResult { content, .. } => content.estimate_tokens(),
        }
    }
}

impl TokenEstimator for ToolDefinition {
    fn estimate_tokens(&self) -> usize {
        let schema = serde_json::to_string(&self.input_schema).unwrap_or_default();
        self.name.estimate_tokens()
            + self.description.estimate_tokens()
            + schema.estimate_tokens()
    }
}

impl ContextOptions {
    /// Estimate tokens for the system prompt.
    pub fn system_prompt_estimate(&self, prompt: &str) -> usize {
        if self.include_system { prompt.estimate_tokens() } else { 0 }
    }

    /// Estimate tokens for all tool definitions.
    pub fn tools_estimate(&self, tools: &[ToolDefinition]) -> usize {
        if self.include_tools {
            tools.iter().map(|t| t.estimate_tokens()).sum()
        } else {
            0
        }
    }
}

impl TokenEstimator for MessageContent {
    fn estimate_tokens(&self) -> usize {
        match self {
            MessageContent::Text(t) => t.estimate_tokens(),
            MessageContent::Content(blocks) => blocks.iter().map(|b| b.estimate_tokens()).sum(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_build_context_truncates_messages() {
        let msgs: Vec<ContextMessage> = (0..200)
            .map(|i| ContextMessage {
                role: MessageRole::User,
                content: MessageContent::Text(format!("Message {}", i)),
                id: None,
            })
            .collect();

        let ctx = build_context(
            "system",
            &[],
            &msgs,
            &ContextOptions { max_messages: 10, ..Default::default() },
        );

        assert_eq!(ctx.messages.len(), 10);
        assert_eq!(ctx.messages[0].content.to_string(), "Message 190");
    }

    #[test]
    fn test_token_estimate_text() {
        let estimate = "hello world".estimate_tokens();
        assert_eq!(estimate, 3); // 11 chars / 4 = 2 (integer division)
    }

    #[test]
    fn test_build_context_empty() {
        let ctx = build_context("", &[], &[], &ContextOptions::default());
        assert!(ctx.messages.is_empty());
        assert!(ctx.tools.is_empty());
    }
}

// Manual Display implementation for MessageContent used in tests
impl fmt::Display for MessageContent {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            MessageContent::Text(t) => write!(f, "{}", t),
            MessageContent::Content(blocks) => {
                for block in blocks {
                    write!(f, "{:?} ", block)?;
                }
                Ok(())
            }
        }
    }
}
