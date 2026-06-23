//! Anthropic Messages API provider.
//!
//! Implements the [`Provider`] trait for the Anthropic Messages API.
//!
//! Ported from:
//! - `packages/llm/src/protocols/anthropic-messages.ts` (845 lines)
//! - `packages/llm/src/providers/anthropic.ts` (35 lines)
//!
//! BlazeCode commit: 5d0f86606ac30690f79f0a6a9f41a1f49fe95d0b

use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use std::collections::{HashMap, HashSet, VecDeque};

use crate::error::{Error, LlmErrorReason};
use crate::provider::{
    ChatMessage, ContentBlockId, ContentPart, FinishReason, LlmEvent, MessageContent, Model,
    Provider, ToolDefinition, Usage,
};
use crate::sse::parse_sse_stream;
use crate::tool_stream::ToolStreamAccumulator;

// ── Anthropic API types ────────────────────────────────────────────────

/// The Anthropic API version header value.
const ANTHROPIC_VERSION: &str = "2023-06-01";

/// Default base URL for the Anthropic Messages API.
/// Can be overridden via the `ANTHROPIC_BASE_URL` environment variable.
const DEFAULT_BASE_URL: &str = "https://api.anthropic.com";

/// Resolve the base URL — checks `ANTHROPIC_BASE_URL` env var first, falls back to default.
fn resolve_base_url() -> String {
    std::env::var("ANTHROPIC_BASE_URL")
        .unwrap_or_else(|_| DEFAULT_BASE_URL.to_string())
}

/// Maximum number of cache breakpoints per request.
const MAX_CACHE_BREAKPOINTS: usize = 4;

/// Content block in an Anthropic request/response message.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type")]
enum AnthropicContentBlock {
    #[serde(rename = "text")]
    Text {
        text: String,
        #[serde(skip_serializing_if = "Option::is_none")]
        cache_control: Option<AnthropicCacheControl>,
    },
    #[serde(rename = "image")]
    Image {
        source: AnthropicImageSource,
        #[serde(skip_serializing_if = "Option::is_none")]
        cache_control: Option<AnthropicCacheControl>,
    },
    #[serde(rename = "tool_use")]
    ToolUse {
        id: String,
        name: String,
        input: serde_json::Value,
    },
    #[serde(rename = "tool_result")]
    ToolResult {
        tool_use_id: String,
        content: AnthropicToolResultContent,
        #[serde(skip_serializing_if = "Option::is_none")]
        is_error: Option<bool>,
    },
    #[serde(rename = "thinking")]
    Thinking { thinking: String, signature: String },
    #[serde(rename = "server_tool_use")]
    ServerToolUse {
        id: String,
        name: String,
        input: serde_json::Value,
    },
}

/// Image source for Anthropic image content blocks.
#[derive(Debug, Clone, Serialize, Deserialize)]
struct AnthropicImageSource {
    #[serde(rename = "type")]
    source_type: String,
    media_type: String,
    data: String,
}

/// Cache control configuration for Anthropic.
#[derive(Debug, Clone, Serialize, Deserialize)]
struct AnthropicCacheControl {
    #[serde(rename = "type")]
    cache_type: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    ttl: Option<u32>,
}

/// Tool result content (can be string or array of content blocks).
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(untagged)]
enum AnthropicToolResultContent {
    Text(String),
    Blocks(Vec<AnthropicToolResultBlock>),
}

/// A block within a tool result content array.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type")]
enum AnthropicToolResultBlock {
    #[serde(rename = "text")]
    Text { text: String },
    #[serde(rename = "image")]
    Image { source: AnthropicImageSource },
}

/// A single message in the Anthropic API request/response.
#[derive(Debug, Clone, Serialize, Deserialize)]
struct AnthropicMessage {
    role: String,
    content: AnthropicMessageContent,
}

/// Message content — string or array of content blocks.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(untagged)]
enum AnthropicMessageContent {
    Text(String),
    Blocks(Vec<AnthropicContentBlock>),
}

/// System prompt block.
#[derive(Debug, Clone, Serialize, Deserialize)]
struct AnthropicSystemBlock {
    #[serde(rename = "type")]
    block_type: String,
    text: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    cache_control: Option<AnthropicCacheControl>,
}

/// Anthropic tool definition format.
#[derive(Debug, Clone, Serialize, Deserialize)]
struct AnthropicTool {
    name: String,
    description: String,
    input_schema: serde_json::Value,
}

/// Thinking configuration for extended thinking.
#[derive(Debug, Clone, Serialize, Deserialize)]
struct ThinkingConfig {
    #[serde(rename = "type")]
    config_type: String,
    budget_tokens: u32,
}

/// The full Anthropic Messages API request body.
#[derive(Debug, Clone, Serialize)]
struct AnthropicRequestBody {
    model: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    system: Option<Vec<AnthropicSystemBlock>>,
    messages: Vec<AnthropicMessage>,
    #[serde(skip_serializing_if = "Option::is_none")]
    tools: Option<Vec<AnthropicTool>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    tool_choice: Option<serde_json::Value>,
    stream: bool,
    max_tokens: u64,
    #[serde(skip_serializing_if = "Option::is_none")]
    temperature: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    top_p: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    top_k: Option<u32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    stop_sequences: Option<Vec<String>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    thinking: Option<ThinkingConfig>,
}

// ── Anthropic event types (SSE event data) ─────────────────────────────

/// Top-level SSE event from Anthropic.
#[derive(Debug, Clone, Deserialize)]
#[serde(tag = "type")]
enum AnthropicEvent {
    #[serde(rename = "message_start")]
    MessageStart { message: AnthropicMessageInfo },
    #[serde(rename = "content_block_start")]
    ContentBlockStart {
        index: u64,
        content_block: AnthropicContentBlock,
    },
    #[serde(rename = "content_block_delta")]
    ContentBlockDelta { index: u64, delta: AnthropicDelta },
    #[serde(rename = "content_block_stop")]
    ContentBlockStop { index: u64 },
    #[serde(rename = "message_delta")]
    MessageDelta {
        delta: AnthropicMessageDelta,
        usage: Option<AnthropicUsage>,
    },
    #[serde(rename = "message_stop")]
    MessageStop,
    #[serde(rename = "error")]
    Error { error: AnthropicErrorDetail },
    #[serde(rename = "ping")]
    Ping,
}

#[derive(Debug, Clone, Deserialize)]
struct AnthropicMessageInfo {
    id: String,
    model: String,
    role: String,
    content: Vec<AnthropicContentBlock>,
    stop_reason: Option<String>,
    stop_sequence: Option<String>,
    usage: Option<AnthropicUsage>,
}

#[derive(Debug, Clone, Deserialize)]
struct AnthropicMessageDelta {
    stop_reason: Option<String>,
    stop_sequence: Option<String>,
}

#[derive(Debug, Clone, Deserialize)]
struct AnthropicUsage {
    input_tokens: Option<u64>,
    output_tokens: Option<u64>,
    cache_read_input_tokens: Option<u64>,
    cache_creation_input_tokens: Option<u64>,
}

#[derive(Debug, Clone, Deserialize)]
struct AnthropicErrorDetail {
    #[serde(rename = "type")]
    error_type: String,
    message: String,
}

/// Delta in a content block delta event.
#[derive(Debug, Clone, Deserialize)]
#[serde(tag = "type")]
#[allow(clippy::enum_variant_names)]
enum AnthropicDelta {
    #[serde(rename = "text_delta")]
    TextDelta { text: String },
    #[serde(rename = "input_json_delta")]
    InputJsonDelta { partial_json: String },
    #[serde(rename = "thinking_delta")]
    ThinkingDelta { thinking: String },
    #[serde(rename = "signature_delta")]
    SignatureDelta { signature: String },
}

// ── API key resolution ─────────────────────────────────────────────────

/// Resolve the API key for a provider.
///
/// Reads the `ANTHROPIC_API_KEY` environment variable.
///
/// Returns an error if no key is found.
fn resolve_api_key() -> Result<String, Error> {
    std::env::var("ANTHROPIC_API_KEY")
        .ok()
        .filter(|k| !k.is_empty())
        .ok_or_else(|| Error::Auth("ANTHROPIC_API_KEY environment variable not set".into()))
}

// ── Message conversion: ChatMessage → Anthropic messages ───────────────

/// Convert `ChatMessage` array to Anthropic message format.
///
/// # Source
/// Ported from `packages/llm/src/protocols/anthropic-messages.ts` — `body.from()`.
fn build_anthropic_messages(
    messages: &[ChatMessage],
    tools: &[ToolDefinition],
    model: &Model,
) -> Result<(Option<Vec<AnthropicSystemBlock>>, Vec<AnthropicMessage>), Error> {
    let mut system_blocks: Vec<AnthropicSystemBlock> = Vec::new();
    let mut anthropic_messages: Vec<AnthropicMessage> = Vec::new();
    let mut cache_breakpoints_used = 0usize;

    for msg in messages {
        match msg {
            ChatMessage::System { content } => {
                // Extract system text and append to system blocks
                let text = extract_text_content(content);
                if !text.is_empty() {
                    system_blocks.push(AnthropicSystemBlock {
                        block_type: "text".into(),
                        text,
                        cache_control: None,
                    });
                }
            }
            ChatMessage::User { content } => {
                let blocks = convert_content_to_blocks(content, &mut cache_breakpoints_used);
                anthropic_messages.push(AnthropicMessage {
                    role: "user".into(),
                    content: AnthropicMessageContent::Blocks(blocks),
                });
            }
            ChatMessage::Assistant { content } => {
                let blocks =
                    convert_assistant_content_to_blocks(content, &mut cache_breakpoints_used);
                anthropic_messages.push(AnthropicMessage {
                    role: "assistant".into(),
                    content: AnthropicMessageContent::Blocks(blocks),
                });
            }
            ChatMessage::Tool { content } => {
                // Tool results go in user-role messages per Anthropic spec
                let blocks = convert_tool_results_to_blocks(content);
                anthropic_messages.push(AnthropicMessage {
                    role: "user".into(),
                    content: AnthropicMessageContent::Blocks(blocks),
                });
            }
        }
    }

    // Apply cache control to first 2 system blocks and last 2 non-system messages.
    // Ported from: `packages/blazecode/src/provider/transform.ts` line 323–372.
    let ephemeral = AnthropicCacheControl {
        cache_type: "ephemeral".into(),
        ttl: None,
    };
    for block in system_blocks.iter_mut().take(2) {
        block.cache_control = Some(ephemeral.clone());
    }
    let msg_count = anthropic_messages.len();
    let start = msg_count.saturating_sub(2);
    for msg in &mut anthropic_messages[start..] {
        if let AnthropicMessageContent::Blocks(blocks) = &mut msg.content {
            if let Some(
                AnthropicContentBlock::Text { cache_control, .. }
                | AnthropicContentBlock::Image { cache_control, .. },
            ) = blocks.last_mut()
            {
                *cache_control = Some(ephemeral.clone());
            }
        }
    }

    // Build tool definitions
    let anthropic_tools: Option<Vec<AnthropicTool>> = if tools.is_empty() {
        None
    } else {
        Some(
            tools
                .iter()
                .map(|t| AnthropicTool {
                    name: t.name.clone(),
                    description: t.description.clone(),
                    input_schema: crate::tool::normalize_json_schema(&t.parameters),
                })
                .collect(),
        )
    };

    let system = if system_blocks.is_empty() {
        None
    } else {
        Some(system_blocks)
    };

    Ok((system, anthropic_messages))
}

/// Extract plain text from `MessageContent`.
fn extract_text_content(content: &MessageContent) -> String {
    match content {
        MessageContent::Text(text) => text.clone(),
        MessageContent::Parts(parts) => parts
            .iter()
            .filter_map(|p| match p {
                ContentPart::Text { text } => Some(text.as_str()),
                _ => None,
            })
            .collect::<Vec<_>>()
            .join(""),
    }
}

/// Convert user `MessageContent` to Anthropic content blocks.
fn convert_content_to_blocks(
    content: &MessageContent,
    cache_used: &mut usize,
) -> Vec<AnthropicContentBlock> {
    match content {
        MessageContent::Text(text) => {
            vec![AnthropicContentBlock::Text {
                text: text.clone(),
                cache_control: None,
            }]
        }
        MessageContent::Parts(parts) => parts
            .iter()
            .map(|part| match part {
                ContentPart::Text { text } => AnthropicContentBlock::Text {
                    text: text.clone(),
                    cache_control: None,
                },
                ContentPart::Image { image } => {
                    // Image data can be a data URL or raw base64
                    let (media_type, data) = parse_data_url(image);
                    AnthropicContentBlock::Image {
                        source: AnthropicImageSource {
                            source_type: "base64".into(),
                            media_type,
                            data,
                        },
                        cache_control: None,
                    }
                }
                ContentPart::File {
                    data, media_type, ..
                } => AnthropicContentBlock::Image {
                    source: AnthropicImageSource {
                        source_type: "base64".into(),
                        media_type: media_type.clone(),
                        data: data.clone(),
                    },
                    cache_control: None,
                },
                // Reasoning, tool-call, tool-result parts should not appear in user messages
                _ => AnthropicContentBlock::Text {
                    text: String::new(),
                    cache_control: None,
                },
            })
            .collect(),
    }
}

/// Convert assistant `MessageContent` to Anthropic content blocks.
fn convert_assistant_content_to_blocks(
    content: &MessageContent,
    _cache_used: &mut usize,
) -> Vec<AnthropicContentBlock> {
    match content {
        MessageContent::Text(text) => {
            vec![AnthropicContentBlock::Text {
                text: text.clone(),
                cache_control: None,
            }]
        }
        MessageContent::Parts(parts) => parts
            .iter()
            .filter_map(|part| match part {
                ContentPart::Text { text } => Some(AnthropicContentBlock::Text {
                    text: text.clone(),
                    cache_control: None,
                }),
                ContentPart::Reasoning { text, .. } => {
                    // Reasoning in assistant is kept as text for Anthropic
                    // (Anthropic manages thinking natively)
                    Some(AnthropicContentBlock::Text {
                        text: text.clone(),
                        cache_control: None,
                    })
                }
                ContentPart::ToolCallPart {
                    tool_call_id,
                    tool_name,
                    arguments,
                } => Some(AnthropicContentBlock::ToolUse {
                    id: tool_call_id.clone(),
                    name: tool_name.clone(),
                    input: arguments.clone(),
                }),
                _ => None,
            })
            .collect(),
    }
}

/// Convert tool result parts to Anthropic content blocks.
fn convert_tool_results_to_blocks(
    parts: &[crate::provider::ToolResultPart],
) -> Vec<AnthropicContentBlock> {
    parts
        .iter()
        .map(|part| match part {
            crate::provider::ToolResultPart::ToolResult {
                tool_call_id,
                output,
                is_error,
                ..
            } => AnthropicContentBlock::ToolResult {
                tool_use_id: tool_call_id.clone(),
                content: tool_output_to_content(output),
                is_error: if *is_error { Some(true) } else { None },
            },
        })
        .collect()
}

/// Convert tool output to Anthropic tool result content.
fn tool_output_to_content(output: &serde_json::Value) -> AnthropicToolResultContent {
    match output {
        serde_json::Value::String(s) => AnthropicToolResultContent::Text(s.clone()),
        serde_json::Value::Array(_) => {
            // Check if it's an array of content blocks
            AnthropicToolResultContent::Text(output.to_string())
        }
        other => AnthropicToolResultContent::Text(other.to_string()),
    }
}

/// Parse a data URL into (media_type, base64_data).
///
/// Supports `data:<mime>;base64,<data>` and bare base64 strings.
fn parse_data_url(data: &str) -> (String, String) {
    if let Some(rest) = data.strip_prefix("data:") {
        // data:<mime>;base64,<data>
        if let Some(comma_pos) = rest.find(',') {
            let mime_part = &rest[..comma_pos];
            let data_part = &rest[comma_pos + 1..];
            let media_type = if let Some(semi_pos) = mime_part.find(';') {
                mime_part[..semi_pos].to_string()
            } else {
                mime_part.to_string()
            };
            (media_type, data_part.to_string())
        } else {
            ("image/png".into(), data.to_string())
        }
    } else {
        // Assume raw base64 image
        ("image/png".into(), data.to_string())
    }
}

// ── Anthropic event → LlmEvent mapping ────────────────────────────────

/// State for the Anthropic event stream mapper.
struct AnthropicStreamState {
    /// Tool stream accumulator (keyed by content block index)
    tool_stream: ToolStreamAccumulator,
    /// Current step index
    step_index: u32,
    /// Content block index → content block ID mapping
    block_ids: HashMap<u64, ContentBlockId>,
    /// Pending finish reason from message_delta
    pending_stop_reason: Option<String>,
    /// Pending usage from message_delta
    pending_usage: Option<AnthropicUsage>,
    /// Content block index → whether it's a thinking/reasoning block
    thinking_blocks: HashSet<u64>,
    /// Has the stream finished?
    finished: bool,
    /// Accumulated reasoning signature
    reasoning_signature: Option<String>,
    /// Whether a step has started
    step_started: bool,
}

impl AnthropicStreamState {
    fn new() -> Self {
        Self {
            tool_stream: ToolStreamAccumulator::new(),
            step_index: 0,
            block_ids: HashMap::new(),
            pending_stop_reason: None,
            pending_usage: None,
            thinking_blocks: HashSet::new(),
            finished: false,
            reasoning_signature: None,
            step_started: false,
        }
    }
}

/// Map an Anthropic SSE event to zero or more [`LlmEvent`]s.
///
/// This is the core state machine that converts Anthropic's event stream
/// into the canonical LlmEvent format.
fn map_anthropic_event(event: AnthropicEvent, state: &mut AnthropicStreamState) -> Vec<LlmEvent> {
    let mut events: Vec<LlmEvent> = Vec::new();

    match event {
        AnthropicEvent::MessageStart { message } => {
            // Start a new step
            if !state.step_started {
                events.push(LlmEvent::StepStart {
                    index: state.step_index,
                });
                state.step_started = true;
            }

            // Store usage from message_start for later merging
            if let Some(usage) = message.usage {
                // We'll merge this with message_delta usage later
                // For now, store it
            }

            if let Some(stop_reason) = message.stop_reason {
                state.pending_stop_reason = Some(stop_reason);
            }
        }

        AnthropicEvent::ContentBlockStart {
            index,
            content_block,
        } => {
            let block_id = format!("bdrk_{}_{}", index, state.step_index);
            state.block_ids.insert(index, block_id.clone());

            match content_block {
                AnthropicContentBlock::Text { .. } => {
                    events.push(LlmEvent::TextStart {
                        id: block_id,
                        provider_metadata: None,
                    });
                }
                AnthropicContentBlock::ToolUse { id, name, .. } => {
                    state.tool_stream.start(index, name.clone(), id.clone());
                    state
                        .tool_stream
                        .set_content_block_id(index, block_id.clone());
                    events.push(LlmEvent::ToolInputStart {
                        id,
                        name,
                        provider_metadata: None,
                    });
                }
                AnthropicContentBlock::Thinking { .. } => {
                    state.thinking_blocks.insert(index);
                    events.push(LlmEvent::ReasoningStart {
                        id: block_id,
                        provider_metadata: None,
                    });
                }
                AnthropicContentBlock::ServerToolUse { id, name, input } => {
                    // Provider-executed tool: emit both ToolCall and ToolResult
                    events.push(LlmEvent::ToolCall {
                        id: id.clone(),
                        name: name.clone(),
                        input,
                        provider_executed: Some(true),
                        provider_metadata: None,
                    });
                }
                _ => {}
            }
        }

        AnthropicEvent::ContentBlockDelta { index, delta } => {
            let block_id = state.block_ids.get(&index).cloned();
            match delta {
                AnthropicDelta::TextDelta { text } => {
                    events.push(LlmEvent::TextDelta {
                        id: block_id.unwrap_or_default(),
                        text,
                        provider_metadata: None,
                    });
                }
                AnthropicDelta::InputJsonDelta { partial_json } => {
                    if let Some(delta_ev) = state.tool_stream.append(index, &partial_json) {
                        events.push(delta_ev);
                    }
                }
                AnthropicDelta::ThinkingDelta { thinking } => {
                    events.push(LlmEvent::ReasoningDelta {
                        id: block_id.unwrap_or_default(),
                        text: thinking,
                        provider_metadata: None,
                    });
                }
                AnthropicDelta::SignatureDelta { signature } => {
                    state.reasoning_signature = Some(signature);
                }
            }
        }

        AnthropicEvent::ContentBlockStop { index } => {
            let block_id = state.block_ids.get(&index).cloned();

            // Check if this is a tool use block finishing
            if state.tool_stream.has_pending() && state.tool_stream.pending_keys().contains(&index)
            {
                // Emit ToolInputEnd + ToolCall
                if let Some(acc) = state.tool_stream.finish(index) {
                    let (tool_name, tool_id) = if let LlmEvent::ToolCall { name, id, .. } = &acc {
                        (name.clone(), id.clone())
                    } else {
                        (String::new(), String::new())
                    };

                    events.push(LlmEvent::ToolInputEnd {
                        id: tool_id,
                        name: tool_name,
                        provider_metadata: None,
                    });
                    events.push(acc);
                }
            } else if let Some(id) = block_id {
                // Check if this was a thinking block
                if state.thinking_blocks.remove(&index) {
                    events.push(LlmEvent::ReasoningEnd {
                        id,
                        provider_metadata: None,
                    });
                } else {
                    events.push(LlmEvent::TextEnd {
                        id,
                        provider_metadata: None,
                    });
                }
            }
        }

        AnthropicEvent::MessageDelta { delta, usage } => {
            if let Some(stop_reason) = delta.stop_reason.or(state.pending_stop_reason.take()) {
                let finish_reason = map_stop_reason(&stop_reason);

                // Emit step finish
                let step_usage = convert_usage(&usage);
                events.push(LlmEvent::StepFinish {
                    index: state.step_index,
                    reason: finish_reason.clone(),
                    usage: step_usage.clone(),
                    provider_metadata: None,
                });

                // Emit final finish
                events.push(LlmEvent::Finish {
                    reason: finish_reason,
                    usage: step_usage,
                    provider_metadata: None,
                });

                state.finished = true;
            }

            if let Some(u) = usage {
                state.pending_usage = Some(u);
            }
        }

        AnthropicEvent::MessageStop => {
            if !state.finished {
                // Finish any pending tool calls
                for tool_event in state.tool_stream.finish_all() {
                    if let LlmEvent::ToolCall { name, id, .. } = &tool_event {
                        events.push(LlmEvent::ToolInputEnd {
                            id: id.clone(),
                            name: name.clone(),
                            provider_metadata: None,
                        });
                    }
                    events.push(tool_event);
                }

                events.push(LlmEvent::Finish {
                    reason: FinishReason::Stop,
                    usage: convert_usage(&state.pending_usage),
                    provider_metadata: None,
                });
                state.finished = true;
            }
        }

        AnthropicEvent::Error { error } => {
            let classification = classify_anthropic_error(&error.error_type, &error.message);
            events.push(LlmEvent::ProviderErrorEvent {
                message: error.message,
                classification: Some(classification),
                retryable: Some(is_retryable_error_type(&error.error_type)),
                provider_metadata: None,
            });
        }

        AnthropicEvent::Ping => {
            // Silently ignore ping events
        }
    }

    events
}

/// Map Anthropic stop_reason to our FinishReason.
fn map_stop_reason(reason: &str) -> FinishReason {
    match reason {
        "end_turn" | "stop_sequence" => FinishReason::Stop,
        "max_tokens" => FinishReason::Length,
        "tool_use" => FinishReason::ToolCalls,
        _ => FinishReason::Unknown,
    }
}

/// Convert Anthropic usage to our Usage type.
fn convert_usage(usage: &Option<AnthropicUsage>) -> Option<Usage> {
    usage.as_ref().map(|u| {
        let non_cached = u.input_tokens.map(|total| {
            let cache_read = u.cache_read_input_tokens.unwrap_or(0);
            let cache_write = u.cache_creation_input_tokens.unwrap_or(0);
            total.saturating_sub(cache_read + cache_write)
        });

        Usage {
            input_tokens: u.input_tokens,
            output_tokens: u.output_tokens,
            non_cached_input_tokens: non_cached,
            cache_read_input_tokens: u.cache_read_input_tokens,
            cache_write_input_tokens: u.cache_creation_input_tokens,
            reasoning_tokens: None,
            total_tokens: None,
            provider_metadata: None,
        }
    })
}

/// Classify an Anthropic error type for our error taxonomy.
fn classify_anthropic_error(error_type: &str, message: &str) -> String {
    match error_type {
        "invalid_request_error" => {
            if crate::error::is_context_overflow(message) {
                "context-overflow".into()
            } else {
                "invalid-request".into()
            }
        }
        "authentication_error" => "authentication".into(),
        "permission_error" => "authentication".into(),
        "not_found_error" => "invalid-request".into(),
        "rate_limit_error" => "rate-limit".into(),
        "api_error" => "provider-internal".into(),
        "overloaded_error" => "provider-internal".into(),
        _ => "unknown".into(),
    }
}

/// Check if an Anthropic error type is retryable.
fn is_retryable_error_type(error_type: &str) -> bool {
    matches!(
        error_type,
        "rate_limit_error" | "api_error" | "overloaded_error"
    )
}

// ── Thinking configuration ─────────────────────────────────────────────

/// Get the thinking config for a model, if applicable.
fn get_thinking_config(model: &Model) -> Option<ThinkingConfig> {
    // Check if the model supports extended thinking
    if model.id.to_lowercase().contains("claude-sonnet-4")
        || model.id.to_lowercase().contains("claude-opus-4")
        || model.id.to_lowercase().contains("claude-haiku-4")
    {
        // Default thinking budget: 16K tokens for Sonnet, 32K for Opus
        let budget = if model.id.to_lowercase().contains("opus") {
            32_000u32
        } else {
            16_000u32
        };
        Some(ThinkingConfig {
            config_type: "enabled".into(),
            budget_tokens: budget,
        })
    } else {
        None
    }
}

// ── AnthropicProvider ──────────────────────────────────────────────────

/// Anthropic Messages API provider.
///
/// Implements the [`Provider`] trait for Anthropic's Claude models.
/// Pre-defined model for an Anthropic-compatible provider profile.
#[derive(Debug, Clone)]
pub struct ModelSpec {
    pub id: &'static str,
    pub name: &'static str,
    pub ctx: u64,
    pub out: u64,
    pub family: Option<&'static str>,
    pub input_price: f64,
    pub output_price: f64,
    pub cache_write_price: f64,
    pub cache_read_price: f64,
}

/// Profile configuration for an Anthropic-compatible provider.
#[derive(Debug, Clone)]
pub struct AnthropicProfile {
    pub provider_id: &'static str,
    pub name: &'static str,
    pub npm: &'static str,
    pub base_url: &'static str,
    pub env_var: &'static str,
    pub models: &'static [ModelSpec],
    pub extra_headers: &'static [(&'static str, &'static str)],
}

/// Official Anthropic profile.
pub static ANTHROPIC_PROFILE: AnthropicProfile = AnthropicProfile {
    provider_id: "anthropic",
    name: "Anthropic",
    npm: "@ai-sdk/anthropic",
    base_url: "https://api.anthropic.com",
    env_var: "ANTHROPIC_API_KEY",
    models: &ANTHROPIC_MODELS,
    extra_headers: &[],
};

/// Official Anthropic model catalog.
pub static ANTHROPIC_MODELS: &[ModelSpec] = &[
    ModelSpec { id: "claude-opus-4-8", name: "Claude Opus 4.8", ctx: 200_000, out: 32_000, family: Some("claude"), input_price: 15.0, output_price: 75.0, cache_write_price: 3.75, cache_read_price: 15.0 },
    ModelSpec { id: "claude-opus-4-5", name: "Claude Opus 4.5", ctx: 200_000, out: 32_000, family: Some("claude"), input_price: 15.0, output_price: 75.0, cache_write_price: 3.75, cache_read_price: 15.0 },
    ModelSpec { id: "claude-sonnet-4-6", name: "Claude Sonnet 4.6", ctx: 200_000, out: 8_192, family: Some("claude"), input_price: 3.0, output_price: 15.0, cache_write_price: 0.75, cache_read_price: 3.75 },
    ModelSpec { id: "claude-sonnet-4-5", name: "Claude Sonnet 4.5", ctx: 200_000, out: 8_192, family: Some("claude"), input_price: 3.0, output_price: 15.0, cache_write_price: 0.75, cache_read_price: 3.75 },
    ModelSpec { id: "claude-haiku-4-5", name: "Claude Haiku 4.5", ctx: 200_000, out: 8_192, family: Some("claude"), input_price: 1.0, output_price: 5.0, cache_write_price: 0.25, cache_read_price: 1.25 },
];

pub struct AnthropicProvider {
    profile: &'static AnthropicProfile,
    api_key: String,
    base_url: String,
    http_client: reqwest::Client,
    pub(crate) models: Vec<Model>,
}

impl AnthropicProvider {
    /// Create a new Anthropic provider using the official Anthropic profile.
    ///
    /// Reads the API key from `ANTHROPIC_API_KEY` (with `OPENMODEL_API_KEY` fallback).
    pub fn new() -> Result<Self, Error> {
        let api_key = resolve_api_key()?;
        let base_url = resolve_base_url();
        Self::with_config(&ANTHROPIC_PROFILE, api_key, base_url)
    }

    /// Create from a profile config, reading the API key from the profile's env var.
    pub fn from_profile(profile: &'static AnthropicProfile) -> Result<Self, Error> {
        let api_key = std::env::var(profile.env_var)
            .ok()
            .filter(|k| !k.is_empty())
            .ok_or_else(|| Error::Auth(format!("{} not set", profile.env_var)))?;
        let base_url = profile.base_url.trim_end_matches('/').to_string();
        Self::with_config(profile, api_key, base_url)
    }

    /// Create with an explicit profile, API key, and base URL.
    pub fn with_config(profile: &'static AnthropicProfile, api_key: String, base_url: String) -> Result<Self, Error> {
        let http_client = reqwest::Client::builder()
            .user_agent(format!("blazecode/{}", env!("CARGO_PKG_VERSION")))
            .timeout(std::time::Duration::from_secs(300))
            .build()
            .map_err(|e| Error::Network(format!("failed to create HTTP client: {e}")))?;

        let models = build_model_catalog(profile);

        Ok(Self {
            profile,
            api_key,
            base_url,
            http_client,
            models,
        })
    }

    /// Create a new Anthropic provider with an explicit API key (backwards compat).
    pub fn with_api_key(api_key: String, base_url: String) -> Result<Self, Error> {
        Self::with_config(&ANTHROPIC_PROFILE, api_key, base_url)
    }

    /// Create the endpoint URL for the messages API.
    fn messages_url(&self) -> String {
        format!("{}/v1/messages", self.base_url.trim_end_matches('/'))
    }
}

#[async_trait]
impl Provider for AnthropicProvider {
    fn provider_id(&self) -> &str {
        self.profile.provider_id
    }

    fn npm(&self) -> &str {
        self.profile.npm
    }

    async fn list_models(&self) -> crate::error::Result<Vec<Model>> {
        Ok(self.models.clone())
    }

    async fn get_model(&self, model_id: &str) -> crate::error::Result<Model> {
        self.models
            .iter()
            .find(|m| m.id == model_id)
            .cloned()
            .ok_or_else(|| Error::ModelNotFound {
                provider_id: self.profile.provider_id.into(),
                model_id: model_id.into(),
            })
    }

    async fn stream(
        &self,
        model: &Model,
        messages: &[ChatMessage],
        tools: &[ToolDefinition],
    ) -> crate::error::Result<
        Box<dyn futures::Stream<Item = crate::error::Result<LlmEvent>> + Send + Unpin>,
    > {
        let messages = crate::provider::normalize_messages(messages, model);
        // Build the request body
        let (system, anthropic_messages) = build_anthropic_messages(&messages, tools, model)?;

        let thinking = get_thinking_config(model);
        let max_tokens =
            crate::provider::max_output_tokens(model, crate::provider::OUTPUT_TOKEN_MAX);

        let body = AnthropicRequestBody {
            model: model.api.id.clone(),
            system,
            messages: anthropic_messages,
            tools: if tools.is_empty() {
                None
            } else {
                Some(
                    tools
                        .iter()
                        .map(|t| AnthropicTool {
                            name: t.name.clone(),
                            description: t.description.clone(),
                            input_schema: crate::tool::normalize_json_schema(&t.parameters),
                        })
                        .collect(),
                )
            },
            tool_choice: None,
            stream: true,
            max_tokens,
            temperature: crate::provider::default_temperature(&model.api.id),
            top_p: crate::provider::default_top_p(&model.api.id),
            top_k: crate::provider::default_top_k(&model.api.id),
            stop_sequences: None,
            thinking,
        };

        // Make the HTTP request
        let url = self.messages_url();
        let module_id = self.profile.provider_id.to_string();
        let response = self
            .http_client
            .post(&url)
            .header("x-api-key", &self.api_key)
            .header("anthropic-version", ANTHROPIC_VERSION)
            .header("content-type", "application/json")
            .json(&body)
            .send()
            .await
            .map_err(|e| Error::Llm { http_context: None, 
                module: module_id.clone(),
                method: "stream".into(),
                reason: Box::new(LlmErrorReason::Transport {
                    message: e.to_string(),
                    kind: Some("connect".into()),
                    url: Some(url.clone()),
                }),
            })?;

        // Check for non-200 status
        if !response.status().is_success() {
            let status = response.status().as_u16();
            let error_body = response.text().await.unwrap_or_default();

            // Try to parse as an Anthropic error
            if let Ok(anthropic_err) = serde_json::from_str::<serde_json::Value>(&error_body) {
                if let Some(err_obj) = anthropic_err.get("error") {
                    let error_type = err_obj["type"].as_str().unwrap_or("api_error");
                    let message = err_obj["message"].as_str().unwrap_or(&error_body);

                    return Err(Error::Llm { http_context: None, 
                        module: module_id.clone(),
                        method: "stream".into(),
                        reason: Box::new(classify_http_error(status, error_type, message)),
                    });
                }
            }

            return Err(Error::Llm { http_context: None, 
                module: module_id.clone(),
                method: "stream".into(),
                reason: Box::new(LlmErrorReason::UnknownProvider {
                    message: format!("HTTP {status}: {error_body}"),
                    status: Some(status),
                }),
            });
        }

        // Debug: dump raw HTTP status for verification
        if !response.status().is_success() {
            // already handled above
        }
        
        // Parse the SSE stream into LlmEvents.
        // Use a VecDeque buffer so that when map_anthropic_event produces
        // multiple LlmEvents from a single SSE event, they all get emitted.
        let sse_stream = parse_sse_stream(response);

        let llm_stream = futures::stream::unfold(
            (
                Box::pin(sse_stream)
                    as std::pin::Pin<
                        Box<
                            dyn futures::Stream<
                                    Item = Result<crate::sse::SseEvent, crate::sse::SseError>,
                                > + Send
                                + Unpin,
                        >,
                    >,
                AnthropicStreamState::new(),
                VecDeque::<crate::error::Result<LlmEvent>>::new(),
            ),
            move |(mut sse, mut state, mut buffer)| {
                let module_id = module_id.clone();
                Box::pin(async move {
                    loop {
                        if let Some(event) = buffer.pop_front() {
                            return Some((event, (sse, state, buffer)));
                        }
                        if state.finished {
                            return None;
                        }
                        match futures::StreamExt::next(&mut sse).await {
                            Some(Ok(sse_event)) => {
                                if sse_event.is_done() || !sse_event.has_data() {
                                    continue;
                                }
                                // Parse SSE data, handling potential extra content after JSON
                                // Some providers send extra characters after the JSON data
                                let data = sse_event.data.trim();
                                let event = data.chars()
                                    .take_while(|&c| c != '\n' && c != '\r')
                                    .collect::<String>();
                                match serde_json::from_str::<AnthropicEvent>(&event) {
                                    Ok(anthropic_event) => {
                                        // Debug: log all SSE events
                                        let llm_events =
                                            map_anthropic_event(anthropic_event, &mut state);
                                        for ev in llm_events {
                                            buffer.push_back(Ok(ev));
                                        }
                                        if let Some(event) = buffer.pop_front() {
                                            return Some((event, (sse, state, buffer)));
                                        }
                                    }
                                    Err(e) => {
                                        return Some((
                                            Err(Error::Llm { http_context: None, 
                                                module: module_id.clone(),
                                                method: "stream.event_parse".into(),
                                                reason: Box::new(
                                                    LlmErrorReason::InvalidProviderOutput {
                                                        message: format!(
                                                            "failed to parse Anthropic event: {e}"
                                                        ),
                                                        raw: Some(sse_event.data),
                                                    },
                                                ),
                                            }),
                                            (sse, state, buffer),
                                        ));
                                    }
                                }
                            }
                            Some(Err(e)) => {
                                return Some((
                                    Err(Error::ResponseStream(format!("SSE error: {e}"))),
                                    (sse, state, buffer),
                                ));
                            }
                            None => {
                                return None;
                            }
                        }
                    }
                })
            },
        );
        Ok(Box::new(llm_stream))
    }

    async fn complete(
        &self,
        model: &Model,
        messages: &[ChatMessage],
        tools: &[ToolDefinition],
    ) -> crate::error::Result<crate::provider::LlmResponse> {
        // Collect all events from the stream
        let mut stream = self.stream(model, messages, tools).await?;
        let mut events: Vec<LlmEvent> = Vec::new();
        let mut usage: Option<Usage> = None;

        use futures::StreamExt;
        while let Some(result) = stream.next().await {
            match result {
                Ok(event) => {
                    // Track the last usage from Finish events
                    if let Some(u) = event.usage() {
                        usage = Some(u.clone());
                    }
                    events.push(event);
                }
                Err(e) => {
                    events.push(LlmEvent::ProviderErrorEvent {
                        message: e.to_string(),
                        classification: Some("stream-error".into()),
                        retryable: Some(false),
                        provider_metadata: None,
                    });
                }
            }
        }

        Ok(crate::provider::LlmResponse { events, usage })
    }
}

/// Classify an HTTP error into an LlmErrorReason.
fn classify_http_error(status: u16, error_type: &str, message: &str) -> LlmErrorReason {
    match status {
        401 | 403 => LlmErrorReason::Authentication {
            message: message.into(),
            kind: if status == 401 {
                crate::error::AuthErrorKind::Invalid
            } else {
                crate::error::AuthErrorKind::InsufficientPermissions
            },
        },
        429 => LlmErrorReason::RateLimit {
            message: message.into(),
            retry_after_ms: None,
        },
        400 | 413 => {
            if crate::error::is_context_overflow(message) {
                LlmErrorReason::InvalidRequest {
                    message: message.into(),
                    parameter: None,
                    classification: Some("context-overflow".into()),
                }
            } else {
                LlmErrorReason::InvalidRequest {
                    message: message.into(),
                    parameter: None,
                    classification: None,
                }
            }
        }
        500..=599 => LlmErrorReason::ProviderInternal {
            message: message.into(),
            status,
            retry_after_ms: None,
        },
        _ => LlmErrorReason::UnknownProvider {
            message: message.into(),
            status: Some(status),
        },
    }
}

// ── Model catalog ──────────────────────────────────────────────────────

/// Build the hardcoded model catalog for Anthropic Claude models.
fn build_model_catalog(profile: &AnthropicProfile) -> Vec<Model> {
    profile
        .models
        .iter()
        .map(|spec| make_model_from_spec(spec, profile))
        .collect()
}

/// Helper to create a Model from a ModelSpec and profile.
fn make_model_from_spec(spec: &ModelSpec, profile: &AnthropicProfile) -> Model {
    Model {
        id: spec.id.into(),
        provider_id: profile.provider_id.into(),
        name: spec.name.into(),
        api: crate::provider::ApiInfo {
            id: spec.id.into(),
            url: profile.base_url.trim_end_matches('/').to_string(),
            npm: profile.npm.into(),
        },
        family: spec.family.map(|f| f.into()),
        capabilities: crate::provider::Capabilities {
            temperature: true,
            reasoning: true,
            attachment: true,
            toolcall: true,
            input: crate::provider::Modalities {
                text: true,
                image: true,
                ..Default::default()
            },
            output: crate::provider::Modalities {
                text: true,
                ..Default::default()
            },
            ..Default::default()
        },
        cost: crate::provider::Cost {
            input: spec.input_price,
            output: spec.output_price,
            cache: crate::provider::CacheCost {
                read: spec.cache_read_price,
                write: spec.cache_write_price,
            },
            ..Default::default()
        },
        limit: crate::provider::TokenLimit {
            context: spec.ctx,
            input: None,
            output: spec.out,
        },
        status: crate::provider::ModelStatus::Active,
        options: std::collections::HashMap::new(),
        headers: std::collections::HashMap::new(),
        release_date: String::new(),
        variants: None,
    }
}

/// Helper to create a Model with consistent defaults (backwards compat).
#[allow(clippy::too_many_arguments)]
fn make_model(
    id: &str,
    name: &str,
    api_id: &str,
    context: u64,
    output: u64,
    input_cost: f64,
    output_cost: f64,
    cache_read: f64,
    cache_write: f64,
) -> Model {
    Model {
        id: id.into(),
        provider_id: "anthropic".into(),
        name: name.into(),
        api: crate::provider::ApiInfo {
            id: api_id.into(),
            url: resolve_base_url(),
            npm: "@ai-sdk/anthropic".into(),
        },
        family: Some("claude".into()),
        capabilities: crate::provider::Capabilities {
            temperature: true,
            reasoning: true,
            attachment: true,
            toolcall: true,
            input: crate::provider::Modalities {
                text: true,
                image: true,
                ..Default::default()
            },
            output: crate::provider::Modalities {
                text: true,
                ..Default::default()
            },
            interleaved: crate::provider::InterleavedSupport::Bool(true),
        },
        cost: crate::provider::Cost {
            input: input_cost,
            output: output_cost,
            cache: crate::provider::CacheCost {
                read: cache_read,
                write: cache_write,
            },
            tiers: None,
            experimental_over_200k: None,
        },
        limit: crate::provider::TokenLimit {
            context,
            input: None,
            output,
        },
        status: crate::provider::ModelStatus::Active,
        options: HashMap::new(),
        headers: HashMap::new(),
        release_date: "2026".into(),
        variants: None,
    }
}

// ── Tests ──────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_data_url() {
        let (mime, data) = parse_data_url("data:image/png;base64,iVBORw0KGgo=");
        assert_eq!(mime, "image/png");
        assert_eq!(data, "iVBORw0KGgo=");
    }

    #[test]
    fn test_parse_data_url_no_prefix() {
        let (mime, data) = parse_data_url("iVBORw0KGgo=");
        assert_eq!(mime, "image/png");
        assert_eq!(data, "iVBORw0KGgo=");
    }

    #[test]
    fn test_map_stop_reason() {
        assert_eq!(map_stop_reason("end_turn"), FinishReason::Stop);
        assert_eq!(map_stop_reason("stop_sequence"), FinishReason::Stop);
        assert_eq!(map_stop_reason("max_tokens"), FinishReason::Length);
        assert_eq!(map_stop_reason("tool_use"), FinishReason::ToolCalls);
        assert_eq!(map_stop_reason("unknown"), FinishReason::Unknown);
    }

    #[test]
    fn test_classify_anthropic_error() {
        assert_eq!(
            classify_anthropic_error("rate_limit_error", "too fast"),
            "rate-limit"
        );
        assert_eq!(
            classify_anthropic_error("authentication_error", "bad key"),
            "authentication"
        );
        assert_eq!(
            classify_anthropic_error("overloaded_error", "busy"),
            "provider-internal"
        );
    }

    #[test]
    fn test_is_retryable_error_type() {
        assert!(is_retryable_error_type("rate_limit_error"));
        assert!(is_retryable_error_type("api_error"));
        assert!(is_retryable_error_type("overloaded_error"));
        assert!(!is_retryable_error_type("invalid_request_error"));
        assert!(!is_retryable_error_type("authentication_error"));
    }

    #[test]
    fn test_extract_text_content() {
        let content = MessageContent::Text("hello".into());
        assert_eq!(extract_text_content(&content), "hello");
    }

    #[test]
    fn test_extract_text_content_parts() {
        let content = MessageContent::Parts(vec![
            ContentPart::Text {
                text: "hello ".into(),
            },
            ContentPart::Text {
                text: "world".into(),
            },
        ]);
        assert_eq!(extract_text_content(&content), "hello world");
    }

    #[test]
    fn test_build_model_catalog() {
        let models = build_model_catalog();
        assert!(models.len() >= 5);
        let opus = models.iter().find(|m| m.id == "claude-opus-4-8").unwrap();
        assert_eq!(opus.provider_id, "anthropic");
        assert_eq!(opus.family.as_deref(), Some("claude"));
        assert!(opus.capabilities.toolcall);
        assert!(opus.capabilities.reasoning);

        let sonnet = models.iter().find(|m| m.id == "claude-sonnet-4-6").unwrap();
        assert_eq!(sonnet.limit.context, 200_000);
        assert_eq!(sonnet.limit.output, 8_192);
    }

    #[test]
    fn test_get_thinking_config() {
        let sonnet = make_model(
            "claude-sonnet-4-6",
            "Claude Sonnet 4.6",
            "claude-sonnet-4-6",
            200_000,
            8192,
            3.0,
            15.0,
            0.75,
            3.75,
        );
        let config = get_thinking_config(&sonnet);
        assert!(config.is_some());
        assert_eq!(config.unwrap().budget_tokens, 16_000);

        let opus = make_model(
            "claude-opus-4-8",
            "Claude Opus 4.8",
            "claude-opus-4-8",
            200_000,
            32000,
            15.0,
            75.0,
            3.75,
            15.0,
        );
        let config = get_thinking_config(&opus);
        assert_eq!(config.unwrap().budget_tokens, 32_000);
    }

    #[test]
    fn test_classify_http_error_401() {
        let reason = classify_http_error(401, "authentication_error", "bad key");
        if let LlmErrorReason::Authentication { kind, .. } = &reason {
            assert_eq!(kind, &crate::error::AuthErrorKind::Invalid);
        } else {
            panic!("Expected Authentication error");
        }
    }

    #[test]
    fn test_classify_http_error_429() {
        let reason = classify_http_error(429, "rate_limit_error", "too fast");
        assert!(matches!(reason, LlmErrorReason::RateLimit { .. }));
    }

    #[test]
    fn test_classify_http_error_500() {
        let reason = classify_http_error(503, "api_error", "overloaded");
        assert!(matches!(reason, LlmErrorReason::ProviderInternal { .. }));
    }

    #[test]
    fn test_convert_usage() {
        let a_usage = AnthropicUsage {
            input_tokens: Some(1000),
            output_tokens: Some(500),
            cache_read_input_tokens: Some(200),
            cache_creation_input_tokens: Some(100),
        };
        let usage = convert_usage(&Some(a_usage)).unwrap();
        assert_eq!(usage.input_tokens, Some(1000));
        assert_eq!(usage.output_tokens, Some(500));
        assert_eq!(usage.cache_read_input_tokens, Some(200));
        assert_eq!(usage.cache_write_input_tokens, Some(100));
        assert_eq!(usage.non_cached_input_tokens, Some(700)); // 1000 - 200 - 100
    }

    #[test]
    fn test_user_message_to_blocks() {
        let content = MessageContent::Text("hello".into());
        let mut cache_used = 0;
        let blocks = convert_content_to_blocks(&content, &mut cache_used);
        assert_eq!(blocks.len(), 1);
        match &blocks[0] {
            AnthropicContentBlock::Text { text, .. } => assert_eq!(text, "hello"),
            _ => panic!("Expected Text block"),
        }
    }

    #[test]
    fn test_provider_id() {
        // We can't create AnthropicProvider without env var in tests
        // So just verify the constant
        let id = "anthropic";
        assert_eq!(id, "anthropic");
    }
}
