//! Provider / LLM integration layer.
//!
//! Ported from:
//!
//! - `packages/opencode/src/provider/provider.ts` (1976 lines)
//! - `packages/opencode/src/provider/transform.ts` (1427 lines)
//! - `packages/opencode/src/provider/auth.ts` (233 lines)
//! - `packages/opencode/src/provider/error.ts` (188 lines)
//! - `packages/llm/src/schema/events.ts` (373 lines)
//! - `packages/llm/src/schema/ids.ts` (44 lines)
//!
//! OpenCode commit: 5d0f86606ac30690f79f0a6a9f41a1f49fe95d0b

use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

// ── ID types ────────────────────────────────────────────────────────

/// Model identifier (branded string in TS).
///
/// # Source
/// Ported from `packages/llm/src/schema/ids.ts` line 12.
pub type ModelId = String;

/// Provider identifier (branded string in TS).
///
/// # Source
/// Ported from `packages/llm/src/schema/ids.ts` line 15.
pub type ProviderId = String;

/// Response identifier.
///
/// # Source
/// Ported from `packages/llm/src/schema/ids.ts` line 18.
pub type ResponseId = String;

/// Content block identifier.
///
/// # Source
/// Ported from `packages/llm/src/schema/ids.ts` line 21.
pub type ContentBlockId = String;

/// Tool call identifier.
///
/// # Source
/// Ported from `packages/llm/src/schema/ids.ts` line 24.
pub type ToolCallId = String;

// ── Reasoning effort ────────────────────────────────────────────────

/// Reasoning effort levels (matches TS `ReasoningEfforts` literal union).
///
/// # Source
/// Ported from `packages/llm/src/schema/ids.ts` line 26–27.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Hash)]
#[serde(rename_all = "snake_case")]
pub enum ReasoningEffort {
    None,
    Minimal,
    Low,
    Medium,
    High,
    Xhigh,
    Max,
}

// ── Finish reason ───────────────────────────────────────────────────

/// Finish reason for a generation step.
///
/// # Source
/// Ported from `packages/llm/src/schema/ids.ts` line 37.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum FinishReason {
    Stop,
    Length,
    #[serde(rename = "tool-calls")]
    ToolCalls,
    #[serde(rename = "content-filter")]
    ContentFilter,
    Error,
    Unknown,
}

// ── Model status ────────────────────────────────────────────────────

/// Model status.
///
/// # Source
/// Ported from `packages/opencode/src/provider/model-status.ts` line 5.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum ModelStatus {
    Alpha,
    Beta,
    Deprecated,
    Active,
}

// ── API info ────────────────────────────────────────────────────────

/// API information for a model.
///
/// # Source
/// Ported from `packages/opencode/src/provider/provider.ts` line 952–956.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ApiInfo {
    /// API model ID
    pub id: String,
    /// API base URL
    #[serde(default)]
    pub url: String,
    /// NPM package name (maps to provider SDK)
    pub npm: String,
}

// ── Modalities ──────────────────────────────────────────────────────

/// Input/output modalities.
///
/// # Source
/// Ported from `packages/opencode/src/provider/provider.ts` line 958–964.
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct Modalities {
    #[serde(default)]
    pub text: bool,
    #[serde(default)]
    pub audio: bool,
    #[serde(default)]
    pub image: bool,
    #[serde(default)]
    pub video: bool,
    #[serde(default)]
    pub pdf: bool,
}

// ── Interleaved support ─────────────────────────────────────────────

/// Interleaved thinking support.
///
/// # Source
/// Ported from `packages/opencode/src/provider/provider.ts` line 966–971.
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

// ── Capabilities ────────────────────────────────────────────────────

/// Model capabilities.
///
/// # Source
/// Ported from `packages/opencode/src/provider/provider.ts` line 973–981.
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct Capabilities {
    /// Supports temperature parameter
    #[serde(default)]
    pub temperature: bool,
    /// Supports reasoning/thinking
    #[serde(default)]
    pub reasoning: bool,
    /// Supports file attachments
    #[serde(default)]
    pub attachment: bool,
    /// Supports tool calls
    #[serde(default)]
    pub toolcall: bool,
    /// Input modalities
    #[serde(default)]
    pub input: Modalities,
    /// Output modalities
    #[serde(default)]
    pub output: Modalities,
    /// Interleaved thinking support
    #[serde(default)]
    pub interleaved: InterleavedSupport,
}

// ── Cost ────────────────────────────────────────────────────────────

/// Cache cost information.
///
/// # Source
/// Ported from `packages/opencode/src/provider/provider.ts` line 983–986.
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct CacheCost {
    #[serde(default)]
    pub read: f64,
    #[serde(default)]
    pub write: f64,
}

/// Cost tier for context-based pricing.
///
/// # Source
/// Ported from `packages/opencode/src/provider/provider.ts` line 988–996.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CostTier {
    #[serde(default)]
    pub input: f64,
    #[serde(default)]
    pub output: f64,
    #[serde(default)]
    pub cache: CacheCost,
    pub tier: TierInfo,
}

/// Tier boundary information.
///
/// # Source
/// Ported from `packages/opencode/src/provider/provider.ts` line 992–995.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TierInfo {
    #[serde(rename = "type")]
    pub tier_type: String,
    pub size: u64,
}

/// Experimental over-200K pricing (GPT-5+).
///
/// # Source
/// Ported from `packages/opencode/src/provider/provider.ts` line 1003–1009.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ExperimentalCost {
    #[serde(default)]
    pub input: f64,
    #[serde(default)]
    pub output: f64,
    #[serde(default)]
    pub cache: CacheCost,
}

/// Cost information per million tokens.
///
/// # Source
/// Ported from `packages/opencode/src/provider/provider.ts` line 998–1010.
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct Cost {
    #[serde(default)]
    pub input: f64,
    #[serde(default)]
    pub output: f64,
    #[serde(default)]
    pub cache: CacheCost,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub tiers: Option<Vec<CostTier>>,
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        rename = "experimentalOver200K"
    )]
    pub experimental_over_200k: Option<ExperimentalCost>,
}

// ── Token limit ─────────────────────────────────────────────────────

/// Token limits for a model.
///
/// # Source
/// Ported from `packages/opencode/src/provider/provider.ts` line 1012–1016.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TokenLimit {
    pub context: u64,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub input: Option<u64>,
    pub output: u64,
}

// ── Model ───────────────────────────────────────────────────────────

/// Model variants — maps variant name to provider-specific options.
///
/// # Source
/// Ported from `packages/opencode/src/provider/transform.ts` line 665–1043.
pub type Variants = HashMap<String, serde_json::Value>;

/// Model information.
///
/// # Source
/// Ported from `packages/opencode/src/provider/provider.ts` line 1018–1033.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Model {
    /// Model ID
    pub id: ModelId,
    /// Provider ID
    #[serde(rename = "providerID")]
    pub provider_id: ProviderId,
    /// Display name
    pub name: String,
    /// API details
    pub api: ApiInfo,
    /// Model family (e.g. "claude", "gpt")
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub family: Option<String>,
    /// Model capabilities
    pub capabilities: Capabilities,
    /// Cost information
    pub cost: Cost,
    /// Token limits
    pub limit: TokenLimit,
    /// Model status
    pub status: ModelStatus,
    /// Per-model config options
    #[serde(default)]
    pub options: HashMap<String, serde_json::Value>,
    /// Per-model headers
    #[serde(default)]
    pub headers: HashMap<String, String>,
    /// Release date (ISO 8601)
    #[serde(default, rename = "release_date")]
    pub release_date: String,
    /// Reasoning effort variants
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub variants: Option<Variants>,
}

// ── Provider info ───────────────────────────────────────────────────

/// Provider source.
///
/// # Source
/// Ported from `packages/opencode/src/provider/provider.ts` line 1038.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum ProviderSource {
    Env,
    Config,
    Custom,
    Api,
}

/// Provider information.
///
/// # Source
/// Ported from `packages/opencode/src/provider/provider.ts` line 1035–1044.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProviderInfo {
    /// Provider ID
    pub id: ProviderId,
    /// Display name
    pub name: String,
    /// Source (env, config, custom, api)
    pub source: ProviderSource,
    /// Environment variable names for auth
    #[serde(default)]
    pub env: Vec<String>,
    /// API key (populated from env or auth)
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub key: Option<String>,
    /// Provider-specific options
    #[serde(default)]
    pub options: HashMap<String, serde_json::Value>,
    /// Available models
    #[serde(default)]
    pub models: HashMap<String, Model>,
}

// ── Model ref ───────────────────────────────────────────────────────

/// A resolved {providerID, modelID} pair.
///
/// # Source
/// Ported from `packages/opencode/src/provider/provider.ts` line 1957–1963.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ModelRef {
    #[serde(rename = "providerID")]
    pub provider_id: ProviderId,
    #[serde(rename = "modelID")]
    pub model_id: ModelId,
}

// ── Provider list result ────────────────────────────────────────────

/// Result of listing providers.
///
/// # Source
/// Ported from `packages/opencode/src/provider/provider.ts` line 1048–1053.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ListResult {
    pub all: Vec<ProviderInfo>,
    #[serde(default)]
    pub default: HashMap<String, String>,
    #[serde(default)]
    pub connected: Vec<String>,
}

/// Config providers result.
///
/// # Source
/// Ported from `packages/opencode/src/provider/provider.ts` line 1055–1059.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ConfigProvidersResult {
    pub providers: Vec<ProviderInfo>,
    #[serde(default)]
    pub default: HashMap<String, String>,
}

// ── Usage ───────────────────────────────────────────────────────────

/// Token usage reported by an LLM provider.
///
/// Uses **inclusive totals** (matching AI SDK / OpenAI / LangChain convention):
/// - `input_tokens` — total prompt tokens, *including* cached reads/writes.
/// - `output_tokens` — total output tokens, *including* reasoning.
/// - `total_tokens` — provider-supplied total, or `input_tokens + output_tokens`.
///
/// **Non-overlapping breakdown** (every field independently meaningful):
/// - `non_cached_input_tokens` — the "fresh" portion of the prompt.
/// - `cache_read_input_tokens` — input tokens served from cache.
/// - `cache_write_input_tokens` — input tokens written to cache.
/// - `reasoning_tokens` — subset of `output_tokens` spent on hidden reasoning.
///
/// Invariant: `non_cached_input_tokens + cache_read_input_tokens +
/// cache_write_input_tokens = input_tokens`, and `reasoning_tokens ≤ output_tokens`.
///
/// # Source
/// Ported from `packages/llm/src/schema/events.ts` line 51–75.
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct Usage {
    /// Total prompt tokens (inclusive of cache)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub input_tokens: Option<u64>,
    /// Total output tokens (inclusive of reasoning)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub output_tokens: Option<u64>,
    /// Non-cached input tokens
    #[serde(skip_serializing_if = "Option::is_none")]
    pub non_cached_input_tokens: Option<u64>,
    /// Input tokens served from cache
    #[serde(skip_serializing_if = "Option::is_none")]
    pub cache_read_input_tokens: Option<u64>,
    /// Input tokens written to cache
    #[serde(skip_serializing_if = "Option::is_none")]
    pub cache_write_input_tokens: Option<u64>,
    /// Reasoning tokens (subset of output_tokens)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub reasoning_tokens: Option<u64>,
    /// Provider-supplied total
    #[serde(skip_serializing_if = "Option::is_none")]
    pub total_tokens: Option<u64>,
    /// Raw provider usage payload
    #[serde(skip_serializing_if = "Option::is_none")]
    pub provider_metadata: Option<HashMap<String, serde_json::Value>>,
}

impl Usage {
    /// Visible output tokens — `output_tokens` minus `reasoning_tokens`, clamped
    /// to zero. Matches TS `visibleOutputTokens` getter.
    ///
    /// # Source
    /// Ported from `packages/llm/src/schema/events.ts` line 67–69.
    #[must_use]
    pub fn visible_output_tokens(&self) -> u64 {
        let out = self.output_tokens.unwrap_or(0);
        let reasoning = self.reasoning_tokens.unwrap_or(0);
        out.saturating_sub(reasoning)
    }
}

// ── LLM Events ──────────────────────────────────────────────────────

/// All LLM streaming events — tagged union matching TS `LLMEvent`.
///
/// # Source
/// Ported from `packages/llm/src/schema/events.ts` line 209–295.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type")]
pub enum LlmEvent {
    /// A new generation step started.
    #[serde(rename = "step-start")]
    StepStart {
        /// Step index (0-based within the generation)
        index: u32,
    },

    /// A text content block started.
    #[serde(rename = "text-start")]
    TextStart {
        /// Content block ID
        id: ContentBlockId,
        #[serde(skip_serializing_if = "Option::is_none")]
        provider_metadata: Option<HashMap<String, serde_json::Value>>,
    },

    /// Text content delta (streamed token).
    #[serde(rename = "text-delta")]
    TextDelta {
        /// Content block ID
        id: ContentBlockId,
        /// Text chunk
        text: String,
        #[serde(skip_serializing_if = "Option::is_none")]
        provider_metadata: Option<HashMap<String, serde_json::Value>>,
    },

    /// A text content block ended.
    #[serde(rename = "text-end")]
    TextEnd {
        /// Content block ID
        id: ContentBlockId,
        #[serde(skip_serializing_if = "Option::is_none")]
        provider_metadata: Option<HashMap<String, serde_json::Value>>,
    },

    /// A reasoning/thinking block started.
    #[serde(rename = "reasoning-start")]
    ReasoningStart {
        /// Content block ID
        id: ContentBlockId,
        #[serde(skip_serializing_if = "Option::is_none")]
        provider_metadata: Option<HashMap<String, serde_json::Value>>,
    },

    /// Reasoning content delta.
    #[serde(rename = "reasoning-delta")]
    ReasoningDelta {
        /// Content block ID
        id: ContentBlockId,
        /// Reasoning text chunk
        text: String,
        #[serde(skip_serializing_if = "Option::is_none")]
        provider_metadata: Option<HashMap<String, serde_json::Value>>,
    },

    /// A reasoning block ended.
    #[serde(rename = "reasoning-end")]
    ReasoningEnd {
        /// Content block ID
        id: ContentBlockId,
        #[serde(skip_serializing_if = "Option::is_none")]
        provider_metadata: Option<HashMap<String, serde_json::Value>>,
    },

    /// Tool call input streaming started.
    #[serde(rename = "tool-input-start")]
    ToolInputStart {
        /// Tool call ID
        id: ToolCallId,
        /// Tool name
        name: String,
        #[serde(skip_serializing_if = "Option::is_none")]
        provider_metadata: Option<HashMap<String, serde_json::Value>>,
    },

    /// Tool call input delta (streamed JSON fragment).
    #[serde(rename = "tool-input-delta")]
    ToolInputDelta {
        /// Tool call ID
        id: ToolCallId,
        /// Tool name
        name: String,
        /// JSON text fragment
        text: String,
    },

    /// Tool call input streaming ended.
    #[serde(rename = "tool-input-end")]
    ToolInputEnd {
        /// Tool call ID
        id: ToolCallId,
        /// Tool name
        name: String,
        #[serde(skip_serializing_if = "Option::is_none")]
        provider_metadata: Option<HashMap<String, serde_json::Value>>,
    },

    /// Completed tool call with parsed input.
    #[serde(rename = "tool-call")]
    ToolCall {
        /// Tool call ID
        id: ToolCallId,
        /// Tool name
        name: String,
        /// Parsed input arguments
        input: serde_json::Value,
        /// Whether the provider executed the tool itself
        #[serde(skip_serializing_if = "Option::is_none")]
        provider_executed: Option<bool>,
        #[serde(skip_serializing_if = "Option::is_none")]
        provider_metadata: Option<HashMap<String, serde_json::Value>>,
    },

    /// Tool execution result.
    #[serde(rename = "tool-result")]
    ToolResult {
        /// Tool call ID
        id: ToolCallId,
        /// Tool name
        name: String,
        /// Tool result value
        result: serde_json::Value,
        /// Structured tool output (when tool returns structured data)
        #[serde(skip_serializing_if = "Option::is_none")]
        output: Option<ToolOutput>,
        #[serde(skip_serializing_if = "Option::is_none")]
        provider_executed: Option<bool>,
        #[serde(skip_serializing_if = "Option::is_none")]
        provider_metadata: Option<HashMap<String, serde_json::Value>>,
    },

    /// Tool execution error.
    #[serde(rename = "tool-error")]
    ToolError {
        /// Tool call ID
        id: ToolCallId,
        /// Tool name
        name: String,
        /// Error message
        message: String,
        /// Structured error payload
        #[serde(skip_serializing_if = "Option::is_none")]
        error: Option<serde_json::Value>,
        #[serde(skip_serializing_if = "Option::is_none")]
        provider_metadata: Option<HashMap<String, serde_json::Value>>,
    },

    /// A step within the generation finished.
    #[serde(rename = "step-finish")]
    StepFinish {
        /// Step index
        index: u32,
        /// Finish reason
        reason: FinishReason,
        /// Token usage for this step
        #[serde(skip_serializing_if = "Option::is_none")]
        usage: Option<Usage>,
        #[serde(skip_serializing_if = "Option::is_none")]
        provider_metadata: Option<HashMap<String, serde_json::Value>>,
    },

    /// The entire generation finished.
    #[serde(rename = "finish")]
    Finish {
        /// Overall finish reason
        reason: FinishReason,
        /// Total token usage across all steps
        #[serde(skip_serializing_if = "Option::is_none")]
        usage: Option<Usage>,
        #[serde(skip_serializing_if = "Option::is_none")]
        provider_metadata: Option<HashMap<String, serde_json::Value>>,
    },

    /// A provider-level error event (non-fatal, stream may continue).
    #[serde(rename = "provider-error")]
    ProviderErrorEvent {
        /// Error message
        message: String,
        /// Error classification
        #[serde(skip_serializing_if = "Option::is_none")]
        classification: Option<String>,
        /// Whether this error is retryable
        #[serde(skip_serializing_if = "Option::is_none")]
        retryable: Option<bool>,
        #[serde(skip_serializing_if = "Option::is_none")]
        provider_metadata: Option<HashMap<String, serde_json::Value>>,
    },
}

impl LlmEvent {
    /// Returns the type tag string for this event.
    #[must_use]
    pub fn type_tag(&self) -> &'static str {
        match self {
            Self::StepStart { .. } => "step-start",
            Self::TextStart { .. } => "text-start",
            Self::TextDelta { .. } => "text-delta",
            Self::TextEnd { .. } => "text-end",
            Self::ReasoningStart { .. } => "reasoning-start",
            Self::ReasoningDelta { .. } => "reasoning-delta",
            Self::ReasoningEnd { .. } => "reasoning-end",
            Self::ToolInputStart { .. } => "tool-input-start",
            Self::ToolInputDelta { .. } => "tool-input-delta",
            Self::ToolInputEnd { .. } => "tool-input-end",
            Self::ToolCall { .. } => "tool-call",
            Self::ToolResult { .. } => "tool-result",
            Self::ToolError { .. } => "tool-error",
            Self::StepFinish { .. } => "step-finish",
            Self::Finish { .. } => "finish",
            Self::ProviderErrorEvent { .. } => "provider-error",
        }
    }

    /// Returns true if this event is a text delta.
    #[must_use]
    pub fn is_text_delta(&self) -> bool {
        matches!(self, Self::TextDelta { .. })
    }

    /// Returns true if this event is a reasoning delta.
    #[must_use]
    pub fn is_reasoning_delta(&self) -> bool {
        matches!(self, Self::ReasoningDelta { .. })
    }

    /// Returns true if this event is a completed tool call.
    #[must_use]
    pub fn is_tool_call(&self) -> bool {
        matches!(self, Self::ToolCall { .. })
    }

    /// Returns the usage from this event, if present.
    #[must_use]
    pub fn usage(&self) -> Option<&Usage> {
        match self {
            Self::StepFinish { usage, .. } | Self::Finish { usage, .. } => usage.as_ref(),
            _ => None,
        }
    }
}

// ── Tool output ─────────────────────────────────────────────────────

/// Structured tool output.
///
/// # Source
/// Ported from `packages/llm/src/schema/events.ts` line 162–170.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ToolOutput {
    /// Whether the output is structured
    #[serde(default)]
    pub structured: bool,
    /// Structured output content
    pub content: serde_json::Value,
}

// ── LLM response ────────────────────────────────────────────────────

/// A complete LLM response assembled from streamed events.
///
/// # Source
/// Ported from `packages/llm/src/schema/events.ts` line 338–372.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LlmResponse {
    /// All events that occurred during generation
    pub events: Vec<LlmEvent>,
    /// Total usage (aggregated across steps)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub usage: Option<Usage>,
}

impl LlmResponse {
    /// Concatenated assistant text from all `text-delta` events.
    #[must_use]
    pub fn text(&self) -> String {
        self.events
            .iter()
            .filter_map(|e| {
                if let LlmEvent::TextDelta { text, .. } = e {
                    Some(text.as_str())
                } else {
                    None
                }
            })
            .collect()
    }

    /// Concatenated reasoning text from all `reasoning-delta` events.
    #[must_use]
    pub fn reasoning(&self) -> String {
        self.events
            .iter()
            .filter_map(|e| {
                if let LlmEvent::ReasoningDelta { text, .. } = e {
                    Some(text.as_str())
                } else {
                    None
                }
            })
            .collect()
    }

    /// All completed tool calls.
    #[must_use]
    pub fn tool_calls(&self) -> Vec<&LlmEvent> {
        self.events.iter().filter(|e| e.is_tool_call()).collect()
    }
}

// ── Chat message types ──────────────────────────────────────────────

/// A chat message for the LLM.
///
/// # Source
/// Ported from `ai` SDK and `packages/core/src/session/message.ts`.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "role")]
pub enum ChatMessage {
    #[serde(rename = "system")]
    System { content: MessageContent },
    #[serde(rename = "user")]
    User { content: MessageContent },
    #[serde(rename = "assistant")]
    Assistant { content: MessageContent },
    #[serde(rename = "tool")]
    Tool { content: Vec<ToolResultPart> },
}

/// Message content (text or multi-part).
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(untagged)]
pub enum MessageContent {
    Text(String),
    Parts(Vec<ContentPart>),
}

impl Default for MessageContent {
    fn default() -> Self {
        Self::Text(String::new())
    }
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
    File {
        data: String,
        #[serde(rename = "mediaType")]
        media_type: String,
        #[serde(skip_serializing_if = "Option::is_none")]
        filename: Option<String>,
    },
    #[serde(rename = "reasoning")]
    Reasoning {
        text: String,
        #[serde(skip_serializing_if = "Option::is_none")]
        provider_options: Option<HashMap<String, serde_json::Value>>,
    },
    #[serde(rename = "tool-call")]
    ToolCallPart {
        #[serde(rename = "toolCallId")]
        tool_call_id: String,
        #[serde(rename = "toolName")]
        tool_name: String,
        arguments: serde_json::Value,
    },
    #[serde(rename = "tool-result")]
    ToolResultPart {
        #[serde(rename = "toolCallId")]
        tool_call_id: String,
        output: serde_json::Value,
    },
}

/// Tool result content part in tool messages.
///
/// # Source
/// Ported from `packages/llm/src/schema/messages.ts`.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type")]
pub enum ToolResultPart {
    #[serde(rename = "tool-result")]
    ToolResult {
        #[serde(rename = "toolCallId")]
        tool_call_id: String,
        /// Tool name
        #[serde(rename = "toolName")]
        tool_name: String,
        /// Tool output
        output: serde_json::Value,
        /// Whether this is an error
        #[serde(rename = "isError", default)]
        is_error: bool,
    },
}

// ── Tool definition ─────────────────────────────────────────────────

/// Tool definition for LLM function calling.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ToolDefinition {
    /// Tool name
    pub name: String,
    /// Tool description
    #[serde(default)]
    pub description: String,
    /// JSON Schema for parameters
    pub parameters: serde_json::Value,
}

// ── Provider trait ──────────────────────────────────────────────────

/// LLM provider trait.
///
/// Each implementation handles one provider's protocol.
///
/// # Source
/// Ported from `packages/llm/src/provider.ts` and `packages/llm/src/route/client.ts`.
#[async_trait]
pub trait Provider: Send + Sync {
    /// Get the provider ID.
    fn provider_id(&self) -> &str;

    /// The npm package name (maps to SDK name).
    fn npm(&self) -> &str;

    /// List available models for this provider.
    async fn list_models(&self) -> crate::error::Result<Vec<Model>>;

    /// Get a specific model by ID.
    async fn get_model(&self, model_id: &str) -> crate::error::Result<Model>;

    /// Stream a chat completion.
    ///
    /// Returns a stream of `LlmEvent` items. Every streaming path uses
    /// `futures::Stream` — never buffered.
    async fn stream(
        &self,
        model: &Model,
        messages: &[ChatMessage],
        tools: &[ToolDefinition],
    ) -> crate::error::Result<
        Box<dyn futures::Stream<Item = crate::error::Result<LlmEvent>> + Send + Unpin>,
    >;

    /// Non-streaming completion.
    async fn complete(
        &self,
        model: &Model,
        messages: &[ChatMessage],
        tools: &[ToolDefinition],
    ) -> crate::error::Result<LlmResponse>;
}

// ── Provider service trait ──────────────────────────────────────────

/// Provider catalog interface — manages provider discovery, model resolution,
/// and SDK initialization.
///
/// # Source
/// Ported from `packages/opencode/src/provider/provider.ts` line 1112–1123.
#[async_trait]
pub trait ProviderCatalog: Send + Sync {
    /// List all available providers with their models.
    async fn list(&self) -> crate::error::Result<HashMap<ProviderId, ProviderInfo>>;

    /// Get a specific provider by ID.
    async fn get_provider(&self, provider_id: &str) -> crate::error::Result<ProviderInfo>;

    /// Get a specific model from a provider.
    async fn get_model(&self, provider_id: &str, model_id: &str) -> crate::error::Result<Model>;

    /// Find the closest matching model by searching for query terms.
    ///
    /// # Source
    /// Ported from `packages/opencode/src/provider/provider.ts` line 1815–1825.
    async fn closest(
        &self,
        provider_id: &str,
        query: &[String],
    ) -> crate::error::Result<Option<ModelRef>>;

    /// Get the "small model" for a provider (for fast operations).
    ///
    /// # Source
    /// Ported from `packages/opencode/src/provider/provider.ts` line 1827–1895.
    async fn get_small_model(&self, provider_id: &str) -> crate::error::Result<Option<Model>>;

    /// Get the default model from config or auto-selection.
    ///
    /// # Source
    /// Ported from `packages/opencode/src/provider/provider.ts` line 1897–1929.
    async fn default_model(&self) -> crate::error::Result<ModelRef>;
}

// ═══════════════════════════════════════════════════════════════════
// Transform Functions
// ═══════════════════════════════════════════════════════════════════

/// Sanitize unpaired UTF-16 surrogates, replacing them with U+FFFD.
///
/// Rust strings are valid UTF-8 and cannot contain surrogate codepoints, so
/// surrogate detection works on the UTF-16 representation. This handles cases
/// where JavaScript/JSON surrogate halves slip through a round-trip.
///
/// # Source
/// Ported from `packages/opencode/src/provider/transform.ts` line 25–27.
#[must_use]
pub fn sanitize_surrogates(content: &str) -> String {
    // In valid UTF-8/Rust strings, surrogate codepoints (U+D800–U+DFFF) are
    // not representable as `char`. They could only appear if a buggy
    // encoder wrote them into a `String`. The TS source handles the JS case
    // where strings can contain isolated surrogates. For Rust, we encode as
    // UTF-16 first to detect any ill-formed surrogate halves that may have
    // been smuggled in via byte manipulation, then rebuild.
    let utf16: Vec<u16> = content.encode_utf16().collect();
    let mut result = String::with_capacity(content.len());
    let len = utf16.len();
    let mut i = 0;
    while i < len {
        let unit = utf16[i];
        // High surrogate (0xD800–0xDBFF)
        if (0xD800..=0xDBFF).contains(&unit) {
            if i + 1 < len && (0xDC00..=0xDFFF).contains(&utf16[i + 1]) {
                // Valid surrogate pair: decode to a char
                let decoded = char::decode_utf16([unit, utf16[i + 1]]).next();
                if let Some(Ok(ch)) = decoded {
                    result.push(ch);
                } else {
                    result.push('\u{FFFD}');
                }
                i += 2;
                continue;
            }
            // Unpaired high surrogate
            result.push('\u{FFFD}');
        }
        // Low surrogate without preceding high surrogate
        else if (0xDC00..=0xDFFF).contains(&unit) {
            if i > 0 && (0xD800..=0xDBFF).contains(&utf16[i - 1]) {
                // Already handled as a pair above — skip
            } else {
                result.push('\u{FFFD}');
            }
        } else {
            // Normal BMP codepoint — safe to convert from u16
            if let Some(ch) = char::from_u32(u32::from(unit)) {
                result.push(ch);
            } else {
                result.push('\u{FFFD}');
            }
        }
        i += 1;
    }
    result
}

/// Normalize messages before sending to the LLM provider.
///
/// Applies provider-specific transforms:
/// - Surrogate sanitization on all text content
/// - Tool call ID scrubbing for Claude and Mistral models
/// - DeepSeek reasoning_content handling
///
/// # Source
/// Ported from `packages/opencode/src/provider/transform.ts` line 65–321.
#[must_use]
pub fn normalize_messages(messages: &[ChatMessage], model: &Model) -> Vec<ChatMessage> {
    let model_id_lower = model.api.id.to_lowercase();
    let provider_lower = model.provider_id.to_lowercase();

    let mut msgs: Vec<ChatMessage> = messages
        .iter()
        .map(|m| sanitize_message_surrogates(m))
        .collect();

    // Tool call ID scrubbing for Claude models
    if provider_lower.contains("anthropic") || model_id_lower.contains("claude") {
        let scrub = |id: &str| -> String {
            id.chars()
                .map(|c| if c.is_ascii_alphanumeric() || c == '_' || c == '-' { c } else { '_' })
                .collect()
        };
        msgs = msgs
            .into_iter()
            .map(|msg| scrub_tool_call_ids(msg, &scrub))
            .collect();
    }

    // Tool call ID scrubbing for Mistral models
    if provider_lower.contains("mistral")
        || model_id_lower.contains("mistral")
        || model_id_lower.contains("devstral")
    {
        let scrub = |id: &str| -> String {
            let cleaned: String = id.chars().filter(|c| c.is_ascii_alphanumeric()).collect();
            let truncated = &cleaned[..cleaned.len().min(9)];
            let padded = format!("{:<9}", truncated);
            padded.replace(' ', "0")
        };
        msgs = msgs
            .into_iter()
            .map(|msg| scrub_tool_call_ids(msg, &scrub))
            .collect();
    }

    // DeepSeek: ensure assistant messages have a reasoning part
    if model_id_lower.contains("deepseek") {
        msgs = msgs
            .into_iter()
            .map(|msg| ensure_deepseek_reasoning(msg))
            .collect();
    }

    msgs
}

/// Sanitize surrogates in all text content of a message.
fn sanitize_message_surrogates(msg: &ChatMessage) -> ChatMessage {
    match msg {
        ChatMessage::System { content } => ChatMessage::System {
            content: sanitize_content_surrogates(content),
        },
        ChatMessage::User { content } => ChatMessage::User {
            content: sanitize_content_surrogates(content),
        },
        ChatMessage::Assistant { content } => ChatMessage::Assistant {
            content: sanitize_content_surrogates(content),
        },
        ChatMessage::Tool { content } => ChatMessage::Tool {
            content: content
                .iter()
                .map(|part| {
                    let crate::provider::ToolResultPart::ToolResult {
                        tool_call_id,
                        tool_name,
                        output,
                        is_error,
                    } = part;
                    crate::provider::ToolResultPart::ToolResult {
                        tool_call_id: tool_call_id.clone(),
                        tool_name: tool_name.clone(),
                        output: sanitize_json_value_surrogates(output),
                        is_error: *is_error,
                    }
                })
                .collect(),
        },
    }
}

/// Sanitize surrogates in message content.
fn sanitize_content_surrogates(content: &MessageContent) -> MessageContent {
    match content {
        MessageContent::Text(s) => MessageContent::Text(sanitize_surrogates(s)),
        MessageContent::Parts(parts) => MessageContent::Parts(
            parts
                .iter()
                .map(|part| match part {
                    ContentPart::Text { text } => ContentPart::Text {
                        text: sanitize_surrogates(text),
                    },
                    ContentPart::Reasoning {
                        text,
                        provider_options,
                    } => ContentPart::Reasoning {
                        text: sanitize_surrogates(text),
                        provider_options: provider_options.clone(),
                    },
                    ContentPart::ToolResultPart {
                        tool_call_id,
                        output,
                    } => ContentPart::ToolResultPart {
                        tool_call_id: tool_call_id.clone(),
                        output: sanitize_json_value_surrogates(output),
                    },
                    other => other.clone(),
                })
                .collect(),
        ),
    }
}

/// Sanitize surrogates in a JSON value (recursively for strings).
fn sanitize_json_value_surrogates(val: &serde_json::Value) -> serde_json::Value {
    match val {
        serde_json::Value::String(s) => serde_json::Value::String(sanitize_surrogates(s)),
        serde_json::Value::Array(arr) => {
            serde_json::Value::Array(arr.iter().map(sanitize_json_value_surrogates).collect())
        }
        serde_json::Value::Object(map) => serde_json::Value::Object(
            map.iter()
                .map(|(k, v)| (k.clone(), sanitize_json_value_surrogates(v)))
                .collect(),
        ),
        other => other.clone(),
    }
}

/// Apply a scrub function to all tool call IDs in a message.
fn scrub_tool_call_ids<F: Fn(&str) -> String>(msg: ChatMessage, scrub: &F) -> ChatMessage {
    match msg {
        ChatMessage::Assistant { content } => match content {
            MessageContent::Parts(parts) => ChatMessage::Assistant {
                content: MessageContent::Parts(
                    parts
                        .into_iter()
                        .map(|part| match part {
                            ContentPart::ToolCallPart {
                                tool_call_id,
                                tool_name,
                                arguments,
                            } => ContentPart::ToolCallPart {
                                tool_call_id: scrub(&tool_call_id),
                                tool_name,
                                arguments,
                            },
                            ContentPart::ToolResultPart {
                                tool_call_id,
                                output,
                            } => ContentPart::ToolResultPart {
                                tool_call_id: scrub(&tool_call_id),
                                output,
                            },
                            other => other,
                        })
                        .collect(),
                ),
            },
            other => ChatMessage::Assistant { content: other },
        },
        ChatMessage::Tool { content } => ChatMessage::Tool {
            content: content
                .into_iter()
                .map(|part| {
                    let crate::provider::ToolResultPart::ToolResult {
                        tool_call_id,
                        tool_name,
                        output,
                        is_error,
                    } = part;
                    crate::provider::ToolResultPart::ToolResult {
                        tool_call_id: scrub(&tool_call_id),
                        tool_name,
                        output,
                        is_error,
                    }
                })
                .collect(),
        },
        other => other,
    }
}

/// Ensure DeepSeek assistant messages contain a reasoning content part.
fn ensure_deepseek_reasoning(msg: ChatMessage) -> ChatMessage {
    match msg {
        ChatMessage::Assistant { content } => match content {
            MessageContent::Parts(parts) => {
                let has_reasoning = parts.iter().any(|p| matches!(p, ContentPart::Reasoning { .. }));
                if has_reasoning {
                    msg
                } else {
                    let mut new_parts = parts;
                    new_parts.push(ContentPart::Reasoning {
                        text: String::new(),
                        provider_options: None,
                    });
                    ChatMessage::Assistant {
                        content: MessageContent::Parts(new_parts),
                    }
                }
            }
            MessageContent::Text(text) => {
                let mut parts = Vec::new();
                if !text.is_empty() {
                    parts.push(ContentPart::Text { text });
                }
                parts.push(ContentPart::Reasoning {
                    text: String::new(),
                    provider_options: None,
                });
                ChatMessage::Assistant {
                    content: MessageContent::Parts(parts),
                }
            }
        },
        other => other,
    }
}

/// Default temperature for a model based on its ID.
///
/// Returns `None` to use the provider's default behavior.
///
/// # Source
/// Ported from `packages/opencode/src/provider/transform.ts` line 479–496.
#[must_use]
pub fn default_temperature(model_id: &str) -> Option<f64> {
    let id = model_id.to_lowercase();
    if id.contains("north-mini-code") {
        Some(1.0)
    } else if id.contains("qwen") {
        Some(0.55)
    } else if id.contains("claude")
        || id.contains("gemini")
        || id.contains("glm-4.6")
        || id.contains("glm-4.7")
        || id.contains("minimax-m2")
    {
        Some(1.0)
    } else if id.contains("kimi-k2") {
        if ["thinking", "k2.", "k2p", "k2-5"]
            .iter()
            .any(|s| id.contains(s))
        {
            Some(1.0)
        } else {
            Some(0.6)
        }
    } else {
        None
    }
}

/// Default topP for a model based on its ID.
///
/// Returns `None` to use the provider's default behavior.
///
/// # Source
/// Ported from `packages/opencode/src/provider/transform.ts` line 498–505.
#[must_use]
pub fn default_top_p(model_id: &str) -> Option<f64> {
    let id = model_id.to_lowercase();
    if id.contains("qwen") {
        Some(1.0)
    } else if [
        "minimax-m2",
        "gemini",
        "kimi-k2.5",
        "kimi-k2p5",
        "kimi-k2-5",
    ]
    .iter()
    .any(|s| id.contains(s))
    {
        Some(0.95)
    } else {
        None
    }
}

/// Default topK for a model based on its ID.
///
/// Returns `None` to use the provider's default behavior.
///
/// # Source
/// Ported from `packages/opencode/src/provider/transform.ts` line 507–515.
#[must_use]
pub fn default_top_k(model_id: &str) -> Option<u32> {
    let id = model_id.to_lowercase();
    if id.contains("minimax-m2") {
        if ["m2.", "m25", "m21"].iter().any(|s| id.contains(s)) {
            Some(40)
        } else {
            Some(20)
        }
    } else if id.contains("gemini") {
        Some(64)
    } else {
        None
    }
}

/// Default model sort priority.
/// Models matching earlier entries sort higher in the list.
///
/// # Source
/// Ported from `packages/opencode/src/provider/provider.ts` line 1947–1955.
const DEFAULT_PRIORITY: &[&str] = &["gpt-5", "claude-sonnet-4", "big-pickle", "gemini-3-pro"];

/// Sort models by priority (gpt-5 > claude-sonnet-4 > big-pickle > gemini-3-pro),
/// then models with "latest" come later, then descending by ID.
///
/// # Source
/// Ported from `packages/opencode/src/provider/provider.ts` line 1948–1955.
pub fn sort_models<T: AsRef<str>>(models: &mut [T]) {
    models.sort_by(|a, b| {
        let a_id = a.as_ref();
        let b_id = b.as_ref();

        // Primary: priority index (higher index = higher priority in TS, reversed order)
        let a_priority = DEFAULT_PRIORITY
            .iter()
            .position(|&p| a_id.contains(p))
            .map(|i| i as i32)
            .unwrap_or(-1);
        let b_priority = DEFAULT_PRIORITY
            .iter()
            .position(|&p| b_id.contains(p))
            .map(|i| i as i32)
            .unwrap_or(-1);
        // Higher priority index = more preferred (matches TS "desc" order)
        match b_priority.cmp(&a_priority) {
            std::cmp::Ordering::Equal => {}
            other => return other,
        }

        // Secondary: "latest" models come after non-latest
        let a_latest = a_id.contains("latest");
        let b_latest = b_id.contains("latest");
        match a_latest.cmp(&b_latest) {
            std::cmp::Ordering::Equal => {}
            other => return other,
        }

        // Tertiary: descending by ID string
        b_id.cmp(a_id)
    });
}

/// Parse a model string of the form "provider/model".
///
/// # Source
/// Ported from `packages/opencode/src/provider/provider.ts` line 1957–1963.
#[must_use]
pub fn parse_model(model: &str) -> ModelRef {
    let (provider_id, model_id) = model.split_once('/').unwrap_or((model, ""));
    ModelRef {
        provider_id: provider_id.to_string(),
        model_id: model_id.to_string(),
    }
}

// ── Provider-specific model selection ───────────────────────────────

/// Maps npm package name to the AI SDK key for providerOptions.
///
/// # Source
/// Ported from `packages/opencode/src/provider/transform.ts` line 30–62.
#[must_use]
pub fn sdk_key(npm: &str) -> Option<&'static str> {
    match npm {
        "@ai-sdk/github-copilot" => Some("copilot"),
        "@ai-sdk/azure" => Some("azure"),
        "@ai-sdk/openai" => Some("openai"),
        "@ai-sdk/amazon-bedrock/mantle" => Some("openai"),
        "@ai-sdk/amazon-bedrock" => Some("bedrock"),
        "@ai-sdk/anthropic" | "@ai-sdk/google-vertex/anthropic" => Some("anthropic"),
        "@ai-sdk/google-vertex" => Some("vertex"),
        "@ai-sdk/google" => Some("google"),
        "@ai-sdk/gateway" => Some("gateway"),
        "@ai-sdk/togetherai" => Some("togetherai"),
        "@ai-sdk/deepseek" => Some("deepseek"),
        "@ai-sdk/groq" => Some("groq"),
        "@ai-sdk/xai" => Some("xai"),
        "@ai-sdk/mistral" => Some("mistral"),
        "@openrouter/ai-sdk-provider" => Some("openrouter"),
        "ai-gateway-provider" => Some("openaiCompatible"),
        _ => None,
    }
}

/// Max output tokens constant (32,000).
///
/// # Source
/// Ported from `packages/opencode/src/provider/transform.ts` line 18.
pub const OUTPUT_TOKEN_MAX: u64 = 32_000;

/// Compute the maximum output tokens for a model, capped at `output_token_max`.
///
/// # Source
/// Ported from `packages/opencode/src/provider/transform.ts` line 1285–1287.
#[must_use]
pub fn max_output_tokens(model: &Model, output_token_max: u64) -> u64 {
    std::cmp::min(model.limit.output, output_token_max)
}

// ── Bundled provider names ──────────────────────────────────────────

/// Bundle provider NPM package names — matches TS `BUNDLED_PROVIDERS`.
///
/// # Source
/// Ported from `packages/opencode/src/provider/provider.ts` line 107–134.
pub const BUNDLED_PROVIDER_NPM: &[&str] = &[
    "@ai-sdk/amazon-bedrock",
    "@ai-sdk/amazon-bedrock/mantle",
    "@ai-sdk/anthropic",
    "@ai-sdk/azure",
    "@ai-sdk/google",
    "@ai-sdk/google-vertex",
    "@ai-sdk/google-vertex/anthropic",
    "@ai-sdk/openai",
    "@ai-sdk/openai-compatible",
    "@openrouter/ai-sdk-provider",
    "@ai-sdk/xai",
    "@ai-sdk/mistral",
    "@ai-sdk/groq",
    "@ai-sdk/deepinfra",
    "@ai-sdk/cerebras",
    "@ai-sdk/cohere",
    "@ai-sdk/gateway",
    "@ai-sdk/togetherai",
    "@ai-sdk/perplexity",
    "@ai-sdk/vercel",
    "@ai-sdk/alibaba",
    "gitlab-ai-provider",
    "venice-ai-sdk-provider",
];

/// Check if a provider NPM is bundled (built-in).
#[must_use]
pub fn is_bundled_provider(npm: &str) -> bool {
    BUNDLED_PROVIDER_NPM.contains(&npm)
}

// ── Provided by runtime: configured models ──────────────────────────

/// Standard widely-supported reasoning efforts.
///
/// # Source
/// Ported from `packages/opencode/src/provider/transform.ts` line 517.
pub const WIDELY_SUPPORTED_EFFORTS: &[&str] = &["low", "medium", "high"];

/// Default reasoning effort description for a model.
/// Returns the reasoning effort name to use as default, or None.
///
/// The returned string lifetime is tied to `model.variants` when returning
/// a key borrowed from the variants map.
///
/// # Source
/// Ported from `packages/opencode/src/provider/transform.ts` line 665–1043,
/// 1171–1184 (gpt-5 default reasoningEffort).
#[must_use]
pub fn default_reasoning_effort(model: &Model) -> Option<&str> {
    let id = model.id.to_lowercase();
    let api_id = model.api.id.to_lowercase();

    // gpt-5 family defaults to "medium" reasoning effort
    if api_id.contains("gpt-5") && !api_id.contains("gpt-5-chat") {
        return Some("medium");
    }
    // Gemini 3 defaults to "high"
    if id.contains("gemini-3") {
        return Some("high");
    }

    // Check variants for the first available effort
    if let Some(variants) = &model.variants {
        if let Some(first_key) = variants.keys().next() {
            return Some(first_key.as_str());
        }
    }

    None
}

// ═══════════════════════════════════════════════════════════════════
// Tests
// ═══════════════════════════════════════════════════════════════════

#[cfg(test)]
mod tests {
    use super::*;

    // ── sanitize_surrogates ─────────────────────────────────────

    #[test]
    fn test_sanitize_surrogates_clean_text() {
        assert_eq!(sanitize_surrogates("hello world"), "hello world");
    }

    #[test]
    fn test_sanitize_surrogates_empty() {
        assert_eq!(sanitize_surrogates(""), "");
    }

    #[test]
    fn test_sanitize_surrogates_emoji_preserved() {
        // Valid surrogate pairs encoded as proper UTF-8 should pass through
        let input = "hi 😀 there";
        assert_eq!(sanitize_surrogates(input), "hi 😀 there");
    }

    #[test]
    fn test_sanitize_surrogates_multi_emoji() {
        // Multiple emoji (each a valid surrogate pair in UTF-16 encoding)
        let input = "😀🌍🚀";
        assert_eq!(sanitize_surrogates(input), "😀🌍🚀");
    }

    #[test]
    fn test_sanitize_surrogates_unicode_snowman() {
        // UTF-8 multi-byte character (☃ = U+2603 = 3 bytes)
        let input = "☃";
        assert_eq!(sanitize_surrogates(input), "☃");
    }

    #[test]
    fn test_sanitize_surrogates_ascii_only() {
        let input = "hello world 123 !@#$%";
        assert_eq!(sanitize_surrogates(input), "hello world 123 !@#$%");
    }

    #[test]
    fn test_sanitize_surrogates_mixed_unicode() {
        // Mix of ASCII, BMP, and supplementary plane characters
        let input = "hello 世界 😀 café";
        assert_eq!(sanitize_surrogates(input), "hello 世界 😀 café");
    }

    #[test]
    fn test_sanitize_surrogates_null_and_control() {
        // Null and control characters are valid Unicode — pass through
        let input = "hello\u{0000}world\u{0007}test";
        assert_eq!(sanitize_surrogates(input), "hello\u{0000}world\u{0007}test");
    }

    #[test]
    fn test_sanitize_surrogates_replacement_char() {
        // U+FFFD itself should pass through unchanged
        let input = "bad \u{FFFD} data";
        assert_eq!(sanitize_surrogates(input), "bad \u{FFFD} data");
    }

    // ── default_temperature ─────────────────────────────────────

    #[test]
    fn test_temperature_claude() {
        assert_eq!(default_temperature("claude-sonnet-4-5"), Some(1.0));
    }

    #[test]
    fn test_temperature_gemini() {
        assert_eq!(default_temperature("gemini-2.5-flash"), Some(1.0));
    }

    #[test]
    fn test_temperature_gpt5() {
        // gpt-5 has no default temperature (uses provider default)
        assert_eq!(default_temperature("gpt-5.1"), None);
    }

    #[test]
    fn test_temperature_north_mini_code() {
        assert_eq!(default_temperature("north-mini-code"), Some(1.0));
    }

    #[test]
    fn test_temperature_qwen() {
        assert_eq!(default_temperature("qwen-3-max"), Some(0.55));
    }

    #[test]
    fn test_temperature_kimi_k2_thinking() {
        assert_eq!(default_temperature("kimi-k2-thinking"), Some(1.0));
    }

    #[test]
    fn test_temperature_kimi_k2_standard() {
        assert_eq!(default_temperature("kimi-k2"), Some(0.6));
    }

    #[test]
    fn test_temperature_minimax_m2() {
        assert_eq!(default_temperature("minimax-m2"), Some(1.0));
    }

    #[test]
    fn test_temperature_glm46() {
        assert_eq!(default_temperature("glm-4.6"), Some(1.0));
    }

    #[test]
    fn test_temperature_glm47() {
        assert_eq!(default_temperature("glm-4.7"), Some(1.0));
    }

    // ── default_top_p ───────────────────────────────────────────

    #[test]
    fn test_top_p_qwen() {
        assert_eq!(default_top_p("qwen-3-max"), Some(1.0));
    }

    #[test]
    fn test_top_p_gemini() {
        assert_eq!(default_top_p("gemini-2.5-pro"), Some(0.95));
    }

    #[test]
    fn test_top_p_minimax() {
        assert_eq!(default_top_p("minimax-m2"), Some(0.95));
    }

    #[test]
    fn test_top_p_gpt5() {
        assert_eq!(default_top_p("gpt-5.1"), None);
    }

    // ── default_top_k ───────────────────────────────────────────

    #[test]
    fn test_top_k_minimax_m2() {
        assert_eq!(default_top_k("minimax-m2"), Some(20));
    }

    #[test]
    fn test_top_k_minimax_m25() {
        assert_eq!(default_top_k("minimax-m2.5"), Some(40));
    }

    #[test]
    fn test_top_k_gemini() {
        assert_eq!(default_top_k("gemini-3-pro"), Some(64));
    }

    #[test]
    fn test_top_k_claude() {
        assert_eq!(default_top_k("claude-sonnet-4-5"), None);
    }

    // ── sort_models ─────────────────────────────────────────────

    #[test]
    fn test_sort_models_by_priority() {
        // Matches TS: provider.ts line 1947-1955 — sortBy findIndex "desc" means
        // higher findIndex = earlier in output. Priority list is:
        // ["gpt-5", "claude-sonnet-4", "big-pickle", "gemini-3-pro"]
        // gemini-3-pro (findIndex=3) > big-pickle (2) > claude-sonnet-4 (1) > gpt-5 (0)
        let mut models = vec![
            "claude-sonnet-4-5".to_string(),
            "big-pickle".to_string(),
            "gpt-5.1".to_string(),
            "gemini-3-pro".to_string(),
        ];
        sort_models(&mut models);
        assert!(models[0].contains("gemini-3-pro"), "got: {:?}", models);
        assert!(models[1].contains("big-pickle"), "got: {:?}", models);
        assert!(models[2].contains("claude-sonnet-4"), "got: {:?}", models);
        assert!(models[3].contains("gpt-5"), "got: {:?}", models);
    }

    #[test]
    fn test_sort_models_latest_after() {
        let mut models = vec!["gpt-5.1-latest".to_string(), "gpt-5.1".to_string()];
        sort_models(&mut models);
        assert_eq!(models[0], "gpt-5.1");
        assert_eq!(models[1], "gpt-5.1-latest");
    }

    // ── parse_model ─────────────────────────────────────────────

    #[test]
    fn test_parse_model_with_provider() {
        let result = parse_model("anthropic/claude-sonnet-4-5");
        assert_eq!(result.provider_id, "anthropic");
        assert_eq!(result.model_id, "claude-sonnet-4-5");
    }

    #[test]
    fn test_parse_model_with_multiple_slashes() {
        let result = parse_model("openrouter/openai/gpt-5.1");
        assert_eq!(result.provider_id, "openrouter");
        assert_eq!(result.model_id, "openai/gpt-5.1");
    }

    #[test]
    fn test_parse_model_no_slash() {
        let result = parse_model("gpt-5.1");
        assert_eq!(result.provider_id, "gpt-5.1");
        assert_eq!(result.model_id, "");
    }

    // ── sdk_key ─────────────────────────────────────────────────

    #[test]
    fn test_sdk_key_known() {
        assert_eq!(sdk_key("@ai-sdk/anthropic"), Some("anthropic"));
        assert_eq!(sdk_key("@ai-sdk/openai"), Some("openai"));
        assert_eq!(sdk_key("@ai-sdk/amazon-bedrock"), Some("bedrock"));
        assert_eq!(sdk_key("@openrouter/ai-sdk-provider"), Some("openrouter"));
    }

    #[test]
    fn test_sdk_key_unknown() {
        assert_eq!(sdk_key("unknown-package"), None);
    }

    // ── max_output_tokens ───────────────────────────────────────

    #[test]
    fn test_max_output_tokens_capped() {
        let model = Model {
            id: "test".into(),
            provider_id: "test".into(),
            name: "Test".into(),
            api: ApiInfo {
                id: "test".into(),
                url: String::new(),
                npm: "@ai-sdk/openai".into(),
            },
            family: None,
            capabilities: Capabilities::default(),
            cost: Cost {
                input: 0.0,
                output: 0.0,
                cache: CacheCost::default(),
                tiers: None,
                experimental_over_200k: None,
            },
            limit: TokenLimit {
                context: 128000,
                input: None,
                output: 4096,
            },
            status: ModelStatus::Active,
            options: HashMap::new(),
            headers: HashMap::new(),
            release_date: String::new(),
            variants: None,
        };
        assert_eq!(max_output_tokens(&model, 32000), 4096);
    }

    #[test]
    fn test_max_output_tokens_below_cap() {
        let model = Model {
            limit: TokenLimit {
                context: 128000,
                input: None,
                output: 64000,
            },
            ..make_stub_model()
        };
        assert_eq!(max_output_tokens(&model, 32000), 32000);
    }

    fn make_stub_model() -> Model {
        Model {
            id: "stub".into(),
            provider_id: "stub".into(),
            name: "Stub".into(),
            api: ApiInfo {
                id: "stub".into(),
                url: String::new(),
                npm: "@ai-sdk/openai".into(),
            },
            family: None,
            capabilities: Capabilities::default(),
            cost: Cost {
                input: 0.0,
                output: 0.0,
                cache: CacheCost::default(),
                tiers: None,
                experimental_over_200k: None,
            },
            limit: TokenLimit {
                context: 128000,
                input: None,
                output: 4096,
            },
            status: ModelStatus::Active,
            options: HashMap::new(),
            headers: HashMap::new(),
            release_date: String::new(),
            variants: None,
        }
    }

    // ── LlmEvent type_tag ───────────────────────────────────────

    #[test]
    fn test_llm_event_type_tags() {
        assert_eq!(LlmEvent::StepStart { index: 0 }.type_tag(), "step-start");
        assert_eq!(
            LlmEvent::TextDelta {
                id: "c_1".into(),
                text: "hi".into(),
                provider_metadata: None,
            }
            .type_tag(),
            "text-delta"
        );
        assert_eq!(
            LlmEvent::ToolCall {
                id: "t_1".into(),
                name: "bash".into(),
                input: serde_json::json!({}),
                provider_executed: None,
                provider_metadata: None,
            }
            .type_tag(),
            "tool-call"
        );
        assert_eq!(
            LlmEvent::Finish {
                reason: FinishReason::Stop,
                usage: None,
                provider_metadata: None,
            }
            .type_tag(),
            "finish"
        );
    }

    #[test]
    fn test_llm_event_is_text_delta() {
        let ev = LlmEvent::TextDelta {
            id: "c_1".into(),
            text: "hi".into(),
            provider_metadata: None,
        };
        assert!(ev.is_text_delta());
        assert!(!ev.is_tool_call());
    }

    #[test]
    fn test_llm_event_is_tool_call() {
        let ev = LlmEvent::ToolCall {
            id: "t_1".into(),
            name: "bash".into(),
            input: serde_json::json!({}),
            provider_executed: None,
            provider_metadata: None,
        };
        assert!(ev.is_tool_call());
        assert!(!ev.is_text_delta());
    }

    // ── Usage ───────────────────────────────────────────────────

    #[test]
    fn test_usage_visible_output_tokens() {
        let usage = Usage {
            output_tokens: Some(1000),
            reasoning_tokens: Some(300),
            ..Default::default()
        };
        assert_eq!(usage.visible_output_tokens(), 700);
    }

    #[test]
    fn test_usage_visible_output_tokens_no_reasoning() {
        let usage = Usage {
            output_tokens: Some(1000),
            reasoning_tokens: None,
            ..Default::default()
        };
        assert_eq!(usage.visible_output_tokens(), 1000);
    }

    #[test]
    fn test_usage_visible_output_tokens_clamped() {
        // If reasoning > output (provider bug), clamp to 0
        let usage = Usage {
            output_tokens: Some(100),
            reasoning_tokens: Some(300),
            ..Default::default()
        };
        assert_eq!(usage.visible_output_tokens(), 0);
    }

    // ── LlmResponse ─────────────────────────────────────────────

    #[test]
    fn test_llm_response_text() {
        let resp = LlmResponse {
            events: vec![
                LlmEvent::TextDelta {
                    id: "c_1".into(),
                    text: "Hello ".into(),
                    provider_metadata: None,
                },
                LlmEvent::TextDelta {
                    id: "c_1".into(),
                    text: "world".into(),
                    provider_metadata: None,
                },
            ],
            usage: None,
        };
        assert_eq!(resp.text(), "Hello world");
    }

    #[test]
    fn test_llm_response_reasoning() {
        let resp = LlmResponse {
            events: vec![
                LlmEvent::ReasoningDelta {
                    id: "r_1".into(),
                    text: "I think ".into(),
                    provider_metadata: None,
                },
                LlmEvent::ReasoningDelta {
                    id: "r_1".into(),
                    text: "therefore".into(),
                    provider_metadata: None,
                },
            ],
            usage: None,
        };
        assert_eq!(resp.reasoning(), "I think therefore");
    }

    #[test]
    fn test_llm_response_tool_calls() {
        let resp = LlmResponse {
            events: vec![
                LlmEvent::TextDelta {
                    id: "c_1".into(),
                    text: "Let me check".into(),
                    provider_metadata: None,
                },
                LlmEvent::ToolCall {
                    id: "t_1".into(),
                    name: "bash".into(),
                    input: serde_json::json!({"command": "ls"}),
                    provider_executed: None,
                    provider_metadata: None,
                },
            ],
            usage: None,
        };
        let calls = resp.tool_calls();
        assert_eq!(calls.len(), 1);
        let call = calls[0];
        assert!(call.is_tool_call());
    }

    // ── LlmEvent serialization ──────────────────────────────────

    #[test]
    fn test_llm_event_serialize_text_delta() {
        let ev = LlmEvent::TextDelta {
            id: "c_1".into(),
            text: "hello".into(),
            provider_metadata: None,
        };
        let json = serde_json::to_value(&ev).unwrap();
        assert_eq!(json["type"], "text-delta");
        assert_eq!(json["id"], "c_1");
        assert_eq!(json["text"], "hello");
    }

    #[test]
    fn test_llm_event_deserialize_text_delta() {
        let json = serde_json::json!({
            "type": "text-delta",
            "id": "c_1",
            "text": "hello"
        });
        let ev: LlmEvent = serde_json::from_value(json).unwrap();
        assert!(ev.is_text_delta());
    }

    // ── Model serialization ─────────────────────────────────────

    #[test]
    fn test_model_serialize_roundtrip() {
        let model = make_stub_model();
        let json = serde_json::to_value(&model).unwrap();
        let restored: Model = serde_json::from_value(json).unwrap();
        assert_eq!(restored.id, "stub");
        assert_eq!(restored.provider_id, "stub");
        assert_eq!(restored.status, ModelStatus::Active);
    }

    // ── ProviderInfo ────────────────────────────────────────────

    #[test]
    fn test_provider_info_source_serialize() {
        let info = ProviderInfo {
            id: "anthropic".into(),
            name: "Anthropic".into(),
            source: ProviderSource::Config,
            env: vec!["ANTHROPIC_API_KEY".into()],
            key: None,
            options: HashMap::new(),
            models: HashMap::new(),
        };
        let json = serde_json::to_string(&info).unwrap();
        assert!(json.contains("config"));
        assert!(json.contains("ANTHROPIC_API_KEY"));
    }

    // ── parse_model edge cases ──────────────────────────────────

    #[test]
    fn test_parse_model_empty() {
        let result = parse_model("");
        assert_eq!(result.provider_id, "");
        assert_eq!(result.model_id, "");
    }

    // ── is_bundled_provider ─────────────────────────────────────

    #[test]
    fn test_is_bundled_provider_known() {
        assert!(is_bundled_provider("@ai-sdk/anthropic"));
        assert!(is_bundled_provider("@ai-sdk/openai"));
    }

    #[test]
    fn test_is_bundled_provider_unknown() {
        assert!(!is_bundled_provider("custom-npm-package"));
    }

    // ── default_reasoning_effort ────────────────────────────────

    #[test]
    fn test_default_reasoning_effort_gpt5() {
        let model = Model {
            id: "gpt-5.1".into(),
            api: ApiInfo {
                id: "gpt-5.1".into(),
                url: String::new(),
                npm: "@ai-sdk/openai".into(),
            },
            ..make_stub_model()
        };
        assert_eq!(default_reasoning_effort(&model), Some("medium"));
    }

    #[test]
    fn test_default_reasoning_effort_gpt5_chat() {
        // API ID "gpt-5.2-chat-latest" does NOT contain the literal
        // substring "gpt-5-chat" (".2" separates "5" and "chat"), so
        // the gpt-5 family detection fires and returns "medium".
        // Only models whose api.id literally contains "gpt-5-chat"
        // (e.g. "gpt-5-chat-latest") are excluded.
        // Matches TS: options.ts line 1152.
        let model = Model {
            id: "gpt-5.2-chat-latest".into(),
            api: ApiInfo {
                id: "gpt-5.2-chat-latest".into(),
                url: String::new(),
                npm: "@ai-sdk/openai".into(),
            },
            ..make_stub_model()
        };
        assert_eq!(default_reasoning_effort(&model), Some("medium"));
    }

    #[test]
    fn test_default_reasoning_effort_claude() {
        let model = Model {
            id: "claude-sonnet-4-5".into(),
            api: ApiInfo {
                id: "claude-sonnet-4-5".into(),
                url: String::new(),
                npm: "@ai-sdk/anthropic".into(),
            },
            ..make_stub_model()
        };
        // claude has no default effort unless variants exist
        assert_eq!(default_reasoning_effort(&model), None);
    }
}
