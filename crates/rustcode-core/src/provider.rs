//! Provider / LLM integration layer.
//!
//! Ported from: `packages/opencode/src/provider/provider.ts`

use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

/// Model information.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Model {
    /// Model ID
    pub id: String,
    /// Provider ID
    pub provider_id: String,
    /// Display name
    pub name: String,
    /// API details
    pub api: ApiInfo,
    /// Model capabilities
    pub capabilities: Capabilities,
    /// Cost information
    pub cost: Cost,
    /// Token limits
    pub limit: TokenLimit,
    /// Model status
    pub status: ModelStatus,
    /// Release date
    pub release_date: String,
}

/// API information for a model.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ApiInfo {
    /// API model ID
    pub id: String,
    /// API base URL
    pub url: String,
    /// NPM package (provider SDK)
    pub npm: String,
}

/// Model capabilities.
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct Capabilities {
    /// Supports temperature parameter
    pub temperature: bool,
    /// Supports reasoning/thinking
    pub reasoning: bool,
    /// Supports file attachments
    pub attachment: bool,
    /// Supports tool calls
    pub toolcall: bool,
    /// Input modalities
    pub input: Modalities,
    /// Output modalities
    pub output: Modalities,
    /// Interleaved thinking support
    pub interleaved: InterleavedSupport,
}

/// Input/output modalities.
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct Modalities {
    pub text: bool,
    pub audio: bool,
    pub image: bool,
    pub video: bool,
    pub pdf: bool,
}

/// Interleaved thinking support.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(untagged)]
pub enum InterleavedSupport {
    /// Simple boolean
    Bool(bool),
    /// Field-specific support
    Field { field: String },
}

impl Default for InterleavedSupport {
    fn default() -> Self {
        Self::Bool(false)
    }
}

/// Cost information per million tokens.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Cost {
    pub input: f64,
    pub output: f64,
    pub cache: CacheCost,
}

/// Cache cost information.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CacheCost {
    pub read: f64,
    pub write: f64,
}

/// Token limits for a model.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TokenLimit {
    pub context: u64,
    pub output: u64,
}

/// Model status.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ModelStatus {
    Active,
    Deprecated,
    Alpha,
}

/// Provider information.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProviderInfo {
    /// Provider ID
    pub id: String,
    /// Display name
    pub name: String,
    /// Source (env, config, custom, api)
    pub source: String,
    /// Environment variable names for auth
    pub env: Vec<String>,
    /// Available models
    pub models: HashMap<String, Model>,
}

/// Streaming chunk from an LLM response.
#[derive(Debug, Clone)]
pub enum StreamChunk {
    /// Text content delta
    TextDelta(String),
    /// Reasoning/thinking delta
    ReasoningDelta { id: String, text: String },
    /// Tool call input started
    ToolInputStart { id: String, name: String },
    /// Tool call input delta
    ToolInputDelta { id: String, text: String },
    /// Tool call input ended
    ToolInputEnd { id: String },
    /// Tool call with parsed input
    ToolCall { id: String, name: String, input: serde_json::Value },
    /// Tool result
    ToolResult { id: String, name: String, output: String },
    /// Step started
    StepStart,
    /// Step finished
    StepFinish { finish_reason: String, usage: Usage },
    /// Stream ended
    Done,
    /// Error
    Error(String),
}

/// Token usage information.
#[derive(Debug, Clone, Default)]
pub struct Usage {
    pub input_tokens: u64,
    pub output_tokens: u64,
    pub reasoning_tokens: u64,
    pub cache_read_tokens: u64,
    pub cache_write_tokens: u64,
}

/// A chat message for the LLM.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "role")]
pub enum ChatMessage {
    #[serde(rename = "system")]
    System { content: String },
    #[serde(rename = "user")]
    User { content: MessageContent },
    #[serde(rename = "assistant")]
    Assistant { content: MessageContent },
}

/// Message content (text or multi-part).
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(untagged)]
pub enum MessageContent {
    Text(String),
    Parts(Vec<ContentPart>),
}

/// A part of a message.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type")]
pub enum ContentPart {
    #[serde(rename = "text")]
    Text { text: String },
    #[serde(rename = "image")]
    Image { image: String },
    #[serde(rename = "file")]
    File { data: String, mime_type: String },
}

/// LLM provider trait.
///
/// # Source
/// Ported from `packages/opencode/src/provider/provider.ts`.
#[async_trait]
pub trait Provider: Send + Sync {
    /// Get the provider ID.
    fn id(&self) -> &str;

    /// List available models.
    async fn list_models(&self) -> crate::error::Result<Vec<Model>>;

    /// Get a specific model.
    async fn get_model(&self, model_id: &str) -> crate::error::Result<Model>;

    /// Stream a chat completion.
    ///
    /// Returns a stream of `StreamChunk` items.
    async fn stream(
        &self,
        model: &Model,
        messages: &[ChatMessage],
        tools: &[ToolDefinition],
    ) -> crate::error::Result<Box<dyn futures::Stream<Item = crate::error::Result<StreamChunk>> + Send + Unpin>>;

    /// Non-streaming completion.
    async fn complete(
        &self,
        model: &Model,
        messages: &[ChatMessage],
        tools: &[ToolDefinition],
    ) -> crate::error::Result<String>;
}

/// Tool definition for LLM function calling.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ToolDefinition {
    /// Tool name
    pub name: String,
    /// Tool description
    pub description: String,
    /// JSON Schema for parameters
    pub parameters: serde_json::Value,
}
