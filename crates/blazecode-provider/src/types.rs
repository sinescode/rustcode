//! # Provider types — core data models for AI provider communication.

use serde::{Deserialize, Serialize};

/// A unique identifier for a provider (e.g., "anthropic", "openai").
pub type ProviderId = String;

/// A unique model identifier (e.g., "claude-sonnet-4-20250514").
pub type ModelId = String;

/// Token count.
pub type TokenCount = usize;

/// Provider role in a message.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum MessageRole {
    /// System prompt.
    System,
    /// User message.
    User,
    /// Assistant message.
    Assistant,
    /// Tool result.
    Tool,
}

/// A single message in a conversation.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Message {
    /// Message role.
    pub role: MessageRole,
    /// Message content (text or structured).
    pub content: MessageContent,
    /// Optional tool call ID (for tool results).
    pub tool_call_id: Option<String>,
    /// Optional tool name.
    pub tool_name: Option<String>,
}

impl Message {
    /// Create a new user message.
    pub fn user(content: impl Into<String>) -> Self {
        Self {
            role: MessageRole::User,
            content: MessageContent::Text(content.into()),
            tool_call_id: None,
            tool_name: None,
        }
    }

    /// Create a new assistant message.
    pub fn assistant(content: impl Into<String>) -> Self {
        Self {
            role: MessageRole::Assistant,
            content: MessageContent::Text(content.into()),
            tool_call_id: None,
            tool_name: None,
        }
    }

    /// Create a new system message.
    pub fn system(content: impl Into<String>) -> Self {
        Self {
            role: MessageRole::System,
            content: MessageContent::Text(content.into()),
            tool_call_id: None,
            tool_name: None,
        }
    }

    /// Create a new tool result message.
    pub fn tool_result(tool_call_id: impl Into<String>, content: impl Into<String>, is_error: bool) -> Self {
        Self {
            role: MessageRole::Tool,
            content: if is_error {
                MessageContent::Text(format!("Error: {}", content.into()))
            } else {
                MessageContent::Text(content.into())
            },
            tool_call_id: Some(tool_call_id.into()),
            tool_name: None,
        }
    }
}

/// Message content — text or multi-part.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(untagged)]
pub enum MessageContent {
    /// Plain text.
    Text(String),
    /// Structured content array (for multi-modal / tool calls).
    Parts(Vec<ContentPart>),
}

/// A single part within a content block.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type")]
pub enum ContentPart {
    /// Text part.
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

/// A tool definition sent to the provider.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ToolDefinition {
    /// Tool name.
    pub name: String,
    /// Human-readable description.
    pub description: String,
    /// JSON schema for input validation.
    pub input_schema: serde_json::Value,
}

/// Provider request configuration.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RequestConfig {
    /// Provider ID.
    pub provider: ProviderId,
    /// Model ID.
    pub model: ModelId,
    /// Maximum tokens to generate.
    pub max_tokens: TokenCount,
    /// Temperature (0.0 - 1.0).
    pub temperature: Option<f32>,
    /// Stop sequences.
    pub stop: Option<Vec<String>>,
    /// System prompt.
    pub system: Option<String>,
    /// Whether to enable streaming.
    pub stream: bool,
    /// Whether to enable thinking/reasoning.
    pub thinking: bool,
    /// Thinking budget tokens.
    pub thinking_budget: Option<TokenCount>,
    /// Additional provider-specific parameters.
    pub extra: Option<serde_json::Value>,
}

impl RequestConfig {
    /// Create a new request configuration.
    pub fn new(provider: impl Into<String>, model: impl Into<String>) -> Self {
        Self {
            provider: provider.into(),
            model: model.into(),
            max_tokens: 4096,
            temperature: None,
            stop: None,
            system: None,
            stream: true,
            thinking: false,
            thinking_budget: None,
            extra: None,
        }
    }
}

/// Reason why a response finished.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum FinishReason {
    /// Normal completion.
    Stop,
    /// Max tokens reached.
    MaxTokens,
    /// Model called a tool.
    ToolUse,
    /// Content was filtered.
    ContentFilter,
    /// An error occurred.
    Error,
    /// Other reason.
    Other,
}

/// Usage statistics for a request.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct Usage {
    /// Input tokens.
    pub input_tokens: TokenCount,
    /// Output tokens.
    pub output_tokens: TokenCount,
    /// Cache read tokens (if supported).
    pub cache_read_tokens: Option<TokenCount>,
    /// Cache creation tokens (if supported).
    pub cache_creation_tokens: Option<TokenCount>,
}

/// A delta chunk in a stream.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StreamDelta {
    /// Text delta.
    pub text: Option<String>,
    /// Reasoning delta.
    pub reasoning: Option<String>,
    /// Tool call delta.
    pub tool_call_delta: Option<ToolCallDelta>,
}

/// A partial tool call from a stream.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ToolCallDelta {
    /// Tool call ID.
    pub id: Option<String>,
    /// Tool name.
    pub name: Option<String>,
    /// Partial JSON input.
    pub input: Option<String>,
}

/// A tool call from the model.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ToolCall {
    /// Tool call ID.
    pub id: String,
    /// Tool name.
    pub name: String,
    /// Parsed JSON input.
    pub input: serde_json::Value,
}

/// A response from a provider.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProviderResponse {
    /// Response content (if any).
    pub content: Option<String>,
    /// Reasoning content (if thinking was enabled).
    pub reasoning: Option<String>,
    /// Tool calls (if any).
    pub tool_calls: Vec<ToolCall>,
    /// Finish reason.
    pub finish_reason: FinishReason,
    /// Usage statistics.
    pub usage: Usage,
    /// Provider-specific metadata.
    pub metadata: Option<serde_json::Value>,
}

/// A streaming event from a provider.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum StreamEvent {
    /// Text delta.
    Text(String),
    /// Reasoning delta.
    Reasoning(String),
    /// Tool call started.
    ToolCallStart {
        /// Tool call ID.
        id: String,
        /// Tool name.
        name: String,
    },
    /// Tool call delta (partial JSON input).
    ToolCallDelta {
        /// Tool call ID.
        id: String,
        /// Partial JSON input.
        input: String,
    },
    /// Tool call completed.
    ToolCallDone {
        /// Tool call ID.
        id: String,
        /// Tool name.
        name: String,
        /// Full parsed JSON input.
        input: serde_json::Value,
    },
    /// Stream finished.
    Done(ProviderResponse),
    /// Error occurred.
    Error(String),
}

/// Provider capability flags.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Capabilities {
    /// Supports streaming.
    pub stream: bool,
    /// Supports thinking/reasoning.
    pub thinking: bool,
    /// Supports tool use.
    pub tools: bool,
    /// Supports system prompts.
    pub system_prompt: bool,
    /// Supports vision/image inputs.
    pub vision: bool,
    /// Maximum context window.
    pub max_context: TokenCount,
}

impl Capabilities {
    /// Claude Sonnet 4 capabilities.
    pub fn claude_sonnet_4() -> Self {
        Self {
            stream: true,
            thinking: true,
            tools: true,
            system_prompt: true,
            vision: true,
            max_context: 200_000,
        }
    }

    /// GPT-4o capabilities.
    pub fn gpt4o() -> Self {
        Self {
            stream: true,
            thinking: false,
            tools: true,
            system_prompt: true,
            vision: true,
            max_context: 128_000,
        }
    }
}

/// Provider configuration (from config file).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProviderConfig {
    /// Provider ID.
    pub id: String,
    /// API base URL.
    pub url: String,
    /// API key (encrypted or from env).
    pub api_key: String,
    /// Default model.
    pub default_model: String,
    /// Capabilities.
    #[serde(default)]
    pub capabilities: Option<Capabilities>,
    /// Enabled models.
    #[serde(default)]
    pub models: Vec<String>,
    /// Request timeout in seconds.
    #[serde(default = "default_timeout")]
    pub timeout_secs: u64,
    /// Max retries.
    #[serde(default = "default_retries")]
    pub max_retries: u32,
}

fn default_timeout() -> u64 { 120 }
fn default_retries() -> u32 { 3 }
