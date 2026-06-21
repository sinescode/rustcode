//! OpenAI Responses API protocol implementation.
//!
//! Implements the OpenAI Responses API (used by GPT-5 family models) as a
//! [`Provider`](crate::provider::Provider) with full streaming support.
//!
//! ## Sections
//!
//! 1. **`ResponsesBody`** — request body construction: message lowering to
//!    Responses `input` items, tool lowering, and `build_responses_body()`.
//! 2. **`ResponsesStreamParser`** — SSE event parsing: dispatches on the
//!    `event:` field of each SSE frame, drives `ResponsesStreamState`, emits
//!    `LlmEvent` variants (text_delta, reasoning_delta, tool_call_delta,
//!    finish_reason, etc.).
//! 3. **`ResponsesProvider`** — [`Provider`] trait impl: `stream()` and
//!    `complete()` using the body builder + stream parser.
//!
//! Ported from: `packages/llm/src/protocols/openai-responses.ts` (1004 lines)

use futures::StreamExt;
use std::collections::HashMap;
use std::collections::VecDeque;

use crate::error::{Error, LlmErrorReason};
use crate::provider::{
    ChatMessage, ContentPart, FinishReason, LlmEvent, MessageContent, Model, ToolDefinition, Usage,
};
use crate::sse::{parse_sse_stream, SseError, SseEvent};

// ═══════════════════════════════════════════════════════════════════════
// Constants
// ═══════════════════════════════════════════════════════════════════════

const DEFAULT_BASE_URL: &str = "https://api.openai.com/v1";
const RESPONSES_PATH: &str = "/responses";

// ═══════════════════════════════════════════════════════════════════════
// 1. Body Building
// ═══════════════════════════════════════════════════════════════════════

/// Options for customizing the OpenAI Responses request body.
#[derive(Debug, Default)]
pub struct ResponsesBodyOptions {
    pub max_output_tokens: Option<u64>,
    pub temperature: Option<f64>,
    pub top_p: Option<f64>,
    pub store: Option<bool>,
    pub instructions: Option<String>,
    pub reasoning_effort: Option<String>,
    pub reasoning_summary: Option<String>,
    pub include: Option<Vec<String>>,
    pub text_verbosity: Option<String>,
    pub tool_choice: Option<serde_json::Value>,
    pub extra_fields: Option<serde_json::Map<String, serde_json::Value>>,
}

/// Build the request body for the OpenAI Responses API.
///
/// Returns a JSON value ready for serialization. The caller adds the
/// `Authorization` header and sends the request.
pub fn build_responses_body(
    model: &Model,
    messages: &[ChatMessage],
    tools: &[ToolDefinition],
    options: ResponsesBodyOptions,
) -> serde_json::Value {
    let input = lower_messages_to_input(messages);
    let tools_arr = lower_tools_to_responses(tools);

    let mut body = serde_json::json!({
        "model": model.api.id,
        "input": input,
        "stream": true,
    });

    // Instructions (system prompt at top level)
    if let Some(ref instructions) = options.instructions {
        body["instructions"] = serde_json::json!(instructions);
    }

    // Model parameters
    let mt = options
        .max_output_tokens
        .or_else(|| Some(crate::provider::max_output_tokens(model, crate::provider::OUTPUT_TOKEN_MAX)));
    if let Some(v) = mt {
        body["max_output_tokens"] = serde_json::json!(v);
    }
    let t = options.temperature.or_else(|| crate::provider::default_temperature(&model.api.id));
    if let Some(v) = t {
        body["temperature"] = serde_json::json!(v);
    }
    let tp = options.top_p.or_else(|| crate::provider::default_top_p(&model.api.id));
    if let Some(v) = tp {
        body["top_p"] = serde_json::json!(v);
    }
    if let Some(v) = options.store {
        body["store"] = serde_json::json!(v);
    }

    // Reasoning config
    let has_effort = options.reasoning_effort.is_some();
    let has_summary = options.reasoning_summary.is_some();
    if has_effort || has_summary {
        let mut reasoning = serde_json::Map::new();
        if let Some(ref effort) = options.reasoning_effort {
            reasoning.insert("effort".into(), serde_json::json!(effort));
        }
        if let Some(ref summary) = options.reasoning_summary {
            reasoning.insert("summary".into(), serde_json::json!(summary));
        }
        body["reasoning"] = serde_json::Value::Object(reasoning);
    }

    // Include array
    if let Some(ref include) = options.include {
        if !include.is_empty() {
            body["include"] = serde_json::json!(include);
        }
    }

    // Text verbosity
    if let Some(ref verbosity) = options.text_verbosity {
        body["text"] = serde_json::json!({ "verbosity": verbosity });
    }

    // Tools
    if !tools_arr.is_empty() {
        body["tools"] = serde_json::Value::Array(tools_arr);
    }
    if let Some(tc) = options.tool_choice {
        body["tool_choice"] = tc;
    }

    // Extra fields
    if let Some(ref extra) = options.extra_fields {
        let obj = body.as_object_mut().unwrap();
        for (k, v) in extra {
            obj.insert(k.clone(), v.clone());
        }
    }

    body
}

/// Extract plain text from `MessageContent`.
pub fn extract_text(content: &MessageContent) -> String {
    match content {
        MessageContent::Text(t) => t.clone(),
        MessageContent::Parts(p) => p
            .iter()
            .filter_map(|p| match p {
                ContentPart::Text { text } => Some(text.as_str()),
                _ => None,
            })
            .collect::<Vec<_>>()
            .join(""),
    }
}

/// Lower `ChatMessage`s to the OpenAI Responses API `input` format.
///
/// The Responses API uses an `input` array of items rather than the
/// Chat Completions `messages` array. Key differences:
/// - System messages are extracted into a top-level `instructions` field.
/// - User messages (including chronological system updates) go into `input`
///   as `{ role: "user", content: [...] }`.
/// - Assistant messages go into `input` as `{ role: "assistant", content: [...] }`.
/// - Tool call parts become `{ type: "function_call", ... }` items.
/// - Tool result parts become `{ type: "function_call_output", ... }` items.
/// - Reasoning parts become `{ type: "reasoning", ... }` items.
pub fn lower_messages_to_input(messages: &[ChatMessage]) -> Vec<serde_json::Value> {
    let mut input: Vec<serde_json::Value> = Vec::new();

    for msg in messages {
        match msg {
            ChatMessage::System { content } => {
                let text = extract_text(content);
                if !text.is_empty() {
                    input.push(serde_json::json!({
                        "role": "system",
                        "content": text,
                    }));
                }
            }
            ChatMessage::User { content } => {
                let mut content_items: Vec<serde_json::Value> = Vec::new();

                let (text_parts, media_parts) = extract_content_parts(content);

                for t in text_parts {
                    content_items.push(serde_json::json!({
                        "type": "input_text",
                        "text": t,
                    }));
                }

                // Add image items
                for img in media_parts {
                    content_items.push(img);
                }

                input.push(serde_json::json!({
                    "role": "user",
                    "content": content_items,
                }));
            }
            ChatMessage::Assistant { content } => {
                let mut text_parts: Vec<String> = Vec::new();
                let mut tool_calls: Vec<serde_json::Value> = Vec::new();

                flush_assistant_text(&mut input, &mut text_parts);

                match content {
                    MessageContent::Text(t) => {
                        text_parts.push(t.clone());
                    }
                    MessageContent::Parts(parts) => {
                        for part in parts {
                            match part {
                                ContentPart::Text { text } => {
                                    text_parts.push(text.clone());
                                }
                                ContentPart::Reasoning { text, .. } => {
                                    // Reasoning parts become reasoning items
                                    flush_assistant_text(&mut input, &mut text_parts);
                                    input.push(serde_json::json!({
                                        "type": "reasoning",
                                        "id": format!("reasoning-{}", input.len()),
                                        "summary": [{"type": "summary_text", "text": text}],
                                    }));
                                }
                                ContentPart::ToolCallPart {
                                    tool_call_id,
                                    tool_name,
                                    arguments,
                                } => {
                                    flush_assistant_text(&mut input, &mut text_parts);
                                    input.push(serde_json::json!({
                                        "type": "function_call",
                                        "call_id": tool_call_id,
                                        "name": tool_name,
                                        "arguments": arguments.to_string(),
                                    }));
                                }
                                _ => {}
                            }
                        }
                    }
                }

                // Flush remaining text
                flush_assistant_text(&mut input, &mut text_parts);
            }
            ChatMessage::Tool { content } => {
                for part in content {
                    let crate::provider::ToolResultPart::ToolResult {
                        tool_call_id,
                        tool_name: _,
                        output,
                        is_error: _,
                    } = part;
                    input.push(serde_json::json!({
                        "type": "function_call_output",
                        "call_id": tool_call_id,
                        "output": output.to_string(),
                    }));
                }
            }
        }
    }

    input
}

/// Flush accumulated assistant text parts into an output_text item.
fn flush_assistant_text(input: &mut Vec<serde_json::Value>, text_parts: &mut Vec<String>) {
    if text_parts.is_empty() {
        return;
    }
    let text = text_parts.join("");
    input.push(serde_json::json!({
        "role": "assistant",
        "content": [{"type": "output_text", "text": text}],
    }));
    text_parts.clear();
}

/// Extract text and media parts from user message content.
fn extract_content_parts(content: &MessageContent) -> (Vec<String>, Vec<serde_json::Value>) {
    let mut text_parts = Vec::new();
    let mut media_parts = Vec::new();

    match content {
        MessageContent::Text(t) => {
            text_parts.push(t.clone());
        }
        MessageContent::Parts(parts) => {
            for part in parts {
                match part {
                    ContentPart::Text { text } => {
                        text_parts.push(text.clone());
                    }
                    ContentPart::Image { image } => {
                        let url = if image.starts_with("data:") {
                            image.clone()
                        } else {
                            format!("data:image/png;base64,{image}")
                        };
                        media_parts.push(serde_json::json!({
                            "type": "input_image",
                            "image_url": url,
                        }));
                    }
                    _ => {}
                }
            }
        }
    }

    (text_parts, media_parts)
}

/// Lower `ToolDefinition`s to the OpenAI Responses tools format.
pub fn lower_tools_to_responses(tools: &[ToolDefinition]) -> Vec<serde_json::Value> {
    tools
        .iter()
        .map(|t| {
            serde_json::json!({
                "type": "function",
                "name": t.name,
                "description": t.description,
                "parameters": t.parameters,
            })
        })
        .collect()
}

// ═══════════════════════════════════════════════════════════════════════
// 2. Stream Parsing — State Machine
// ═══════════════════════════════════════════════════════════════════════

/// Internal tracked state for a pending tool call during streaming.
#[derive(Debug, Clone)]
struct PendingToolCall {
    call_id: String,
    name: String,
    arguments: String,
    has_started: bool,
}

/// Parser state for streaming OpenAI Responses SSE events.
///
/// Tracks text/reasoning start flags, pending tool calls keyed by item ID,
/// usage, and the finished sentinel.
#[derive(Debug, Clone)]
pub struct ResponsesStreamState {
    /// Whether a text block has been started for the current response.
    pub text_started: bool,
    /// Whether a reasoning block has been started.
    pub reasoning_started: bool,
    /// Whether a step has been started.
    pub step_started: bool,
    /// Pending tool calls keyed by OpenAI item ID (string).
    pub pending_tools: HashMap<String, PendingToolCall>,
    /// Accumulated usage from the last event that carried it.
    pub usage: Option<Usage>,
    /// Whether the stream has received a terminal event.
    pub finished: bool,
    /// Whether a tool call was present in the response (determines finish reason).
    pub has_function_call: bool,
}

impl ResponsesStreamState {
    pub fn new() -> Self {
        Self {
            text_started: false,
            reasoning_started: false,
            step_started: false,
            pending_tools: HashMap::new(),
            usage: None,
            finished: false,
            has_function_call: false,
        }
    }
}

impl Default for ResponsesStreamState {
    fn default() -> Self {
        Self::new()
    }
}

/// Map an OpenAI Responses incomplete_details reason to `FinishReason`.
pub fn map_responses_finish_reason(
    incomplete_reason: Option<&str>,
    has_function_call: bool,
) -> FinishReason {
    match incomplete_reason {
        None | Some("") => {
            if has_function_call {
                FinishReason::ToolCalls
            } else {
                FinishReason::Stop
            }
        }
        Some("max_output_tokens") => FinishReason::Length,
        Some("content_filter") => FinishReason::ContentFilter,
        _ => {
            if has_function_call {
                FinishReason::ToolCalls
            } else {
                FinishReason::Unknown
            }
        }
    }
}

/// Map usage from an OpenAI Responses usage object.
pub fn map_responses_usage(usage: &serde_json::Value) -> Option<Usage> {
    let input_tokens = usage.get("input_tokens").and_then(|v| v.as_u64());
    let output_tokens = usage.get("output_tokens").and_then(|v| v.as_u64());
    let total_tokens = usage.get("total_tokens").and_then(|v| v.as_u64());

    let cached = usage
        .get("input_tokens_details")
        .and_then(|d| d.get("cached_tokens"))
        .and_then(|v| v.as_u64());

    let reasoning = usage
        .get("output_tokens_details")
        .and_then(|d| d.get("reasoning_tokens"))
        .and_then(|v| v.as_u64());

    let non_cached = input_tokens.map(|p| p.saturating_sub(cached.unwrap_or(0)));

    Some(Usage {
        input_tokens,
        output_tokens,
        non_cached_input_tokens: non_cached,
        cache_read_input_tokens: cached,
        cache_write_input_tokens: None,
        reasoning_tokens: reasoning,
        total_tokens,
        provider_metadata: None,
    })
}

/// Process one OpenAI Responses SSE event and emit zero or more `LlmEvent`s.
///
/// Dispatches on the SSE `event_type` field, handling all event types
/// documented in the OpenAI Responses API:
/// - `response.output_text.delta` → text delta
/// - `response.reasoning_summary_text.delta` → reasoning delta
/// - `response.output_item.added` → tool input start / reasoning start
/// - `response.function_call_arguments.delta` → tool input delta
/// - `response.output_item.done` → tool call / hosted tool call
/// - `response.completed` / `response.incomplete` → finish
/// - `response.failed` → provider error
/// - `error` → provider error
pub fn events_from_responses_event(
    event: &SseEvent,
    data: &serde_json::Value,
    state: &mut ResponsesStreamState,
) -> Vec<LlmEvent> {
    let event_type = event.event_type.as_deref().unwrap_or("");
    match event_type {
        "response.output_text.delta" => on_output_text_delta(data, state),
        "response.reasoning_summary_text.delta" | "response.reasoning_text.delta" => {
            on_reasoning_delta(data, state)
        }
        "response.output_item.added" => on_output_item_added(data, state),
        "response.function_call_arguments.delta" => on_function_call_arguments_delta(data, state),
        "response.output_item.done" => on_output_item_done(data, state),
        "response.completed" | "response.incomplete" => on_response_finish(data, state),
        "response.failed" => on_response_failed(data, state),
        "error" => on_error_event(data, state),
        _ => Vec::new(),
    }
}

/// Handle `response.output_text.delta` events.
fn on_output_text_delta(data: &serde_json::Value, state: &mut ResponsesStreamState) -> Vec<LlmEvent> {
    let mut events = Vec::new();
    let delta = match data.get("delta").and_then(|d| d.as_str()) {
        Some(d) if !d.is_empty() => d,
        _ => return events,
    };

    if !state.text_started {
        state.text_started = true;
        events.push(LlmEvent::StepStart { index: 0 });
        state.step_started = true;
        events.push(LlmEvent::TextStart {
            id: "text-0".into(),
            provider_metadata: None,
        });
    }

    events.push(LlmEvent::TextDelta {
        id: "text-0".into(),
        text: delta.to_string(),
        provider_metadata: None,
    });

    events
}

/// Handle `response.reasoning_summary_text.delta` events.
fn on_reasoning_delta(data: &serde_json::Value, state: &mut ResponsesStreamState) -> Vec<LlmEvent> {
    let mut events = Vec::new();
    let delta = match data.get("delta").and_then(|d| d.as_str()) {
        Some(d) if !d.is_empty() => d,
        _ => return events,
    };

    if !state.reasoning_started {
        state.reasoning_started = true;
        if !state.step_started {
            events.push(LlmEvent::StepStart { index: 0 });
            state.step_started = true;
        }
        events.push(LlmEvent::ReasoningStart {
            id: "reasoning-0".into(),
            provider_metadata: None,
        });
    }

    events.push(LlmEvent::ReasoningDelta {
        id: "reasoning-0".into(),
        text: delta.to_string(),
        provider_metadata: None,
    });

    events
}

/// Handle `response.output_item.added` events.
///
/// May signal a new function_call item (tool call start) or a reasoning item.
fn on_output_item_added(data: &serde_json::Value, state: &mut ResponsesStreamState) -> Vec<LlmEvent> {
    let item = match data.get("item") {
        Some(item) => item,
        None => return Vec::new(),
    };

    let item_type = match item.get("type").and_then(|t| t.as_str()) {
        Some(t) => t,
        None => return Vec::new(),
    };

    match item_type {
        "function_call" => on_function_call_started(item, state),
        "reasoning" => on_reasoning_started(item, state),
        // Hosted tools (web_search_call, file_search_call, etc.)
        s if is_hosted_tool_type(s) => on_hosted_tool_started(item, state, s),
        _ => Vec::new(),
    }
}

/// Check if a type string represents a hosted (provider-executed) tool.
fn is_hosted_tool_type(t: &str) -> bool {
    matches!(
        t,
        "web_search_call"
            | "web_search_preview_call"
            | "file_search_call"
            | "code_interpreter_call"
            | "computer_use_call"
            | "image_generation_call"
            | "mcp_call"
            | "local_shell_call"
    )
}

/// Handle a function_call item being added.
fn on_function_call_started(item: &serde_json::Value, state: &mut ResponsesStreamState) -> Vec<LlmEvent> {
    let mut events = Vec::new();
    let item_id = match item.get("id").and_then(|v| v.as_str()) {
        Some(id) if !id.is_empty() => id.to_string(),
        _ => return events,
    };
    let call_id = item
        .get("call_id")
        .and_then(|v| v.as_str())
        .unwrap_or("")
        .to_string();
    let name = item
        .get("name")
        .and_then(|v| v.as_str())
        .unwrap_or("")
        .to_string();
    let arguments = item
        .get("arguments")
        .and_then(|v| v.as_str())
        .unwrap_or("")
        .to_string();

    if !state.step_started {
        events.push(LlmEvent::StepStart { index: 0 });
        state.step_started = true;
    }

    // If arguments are already present in the added event, parse them
    let tool_input_start = LlmEvent::ToolInputStart {
        id: call_id.clone(),
        name: name.clone(),
        provider_metadata: None,
    };
    events.push(tool_input_start);

    if !arguments.is_empty() {
        events.push(LlmEvent::ToolInputDelta {
            id: call_id.clone(),
            name: name.clone(),
            text: arguments.clone(),
        });
    }

    state.pending_tools.insert(
        item_id,
        PendingToolCall {
            call_id,
            name,
            arguments,
            has_started: true,
        },
    );

    events
}

/// Handle a reasoning item being added.
fn on_reasoning_started(item: &serde_json::Value, state: &mut ResponsesStreamState) -> Vec<LlmEvent> {
    let mut events = Vec::new();
    let item_id = item
        .get("id")
        .and_then(|v| v.as_str())
        .unwrap_or("reasoning-0");

    if !state.step_started {
        events.push(LlmEvent::StepStart { index: 0 });
        state.step_started = true;
    }

    if !state.reasoning_started {
        state.reasoning_started = true;
        events.push(LlmEvent::ReasoningStart {
            id: item_id.to_string(),
            provider_metadata: None,
        });
    }

    events
}

/// Handle a hosted tool item being added (e.g. web_search_call).
fn on_hosted_tool_started(
    item: &serde_json::Value,
    state: &mut ResponsesStreamState,
    tool_type: &str,
) -> Vec<LlmEvent> {
    let mut events = Vec::new();
    let item_id = match item.get("id").and_then(|v| v.as_str()) {
        Some(id) if !id.is_empty() => id.to_string(),
        _ => return events,
    };

    // Map hosted tool types to display names
    let tool_name = match tool_type {
        "web_search_call" => "web_search",
        "web_search_preview_call" => "web_search_preview",
        "file_search_call" => "file_search",
        "code_interpreter_call" => "code_interpreter",
        "computer_use_call" => "computer_use",
        "image_generation_call" => "image_generation",
        "mcp_call" => "mcp",
        "local_shell_call" => "local_shell",
        _ => tool_type,
    };

    if !state.step_started {
        events.push(LlmEvent::StepStart { index: 0 });
        state.step_started = true;
    }

    // Hosted tools emit tool-call + tool-result pair immediately
    let input = hosted_tool_input(item, tool_type);
    let provider_metadata = Some({
        let mut m = HashMap::new();
        m.insert(
            "itemId".into(),
            serde_json::Value::String(item_id.clone()),
        );
        m
    });

    events.push(LlmEvent::ToolCall {
        id: item_id.clone(),
        name: tool_name.to_string(),
        input,
        provider_executed: Some(true),
        provider_metadata: provider_metadata.clone(),
    });

    // For hosted tools, also emit a tool-result
    let result = hosted_tool_result(item);
    events.push(LlmEvent::ToolResult {
        id: item_id,
        name: tool_name.to_string(),
        result,
        output: None,
        provider_executed: Some(true),
        provider_metadata,
    });

    state.has_function_call = true;

    events
}

/// Extract the typed input from a hosted tool item.
fn hosted_tool_input(item: &serde_json::Value, tool_type: &str) -> serde_json::Value {
    match tool_type {
        "web_search_call" | "web_search_preview_call" => {
            item.get("action").cloned().unwrap_or(serde_json::json!({}))
        }
        "file_search_call" => serde_json::json!({
            "queries": item.get("queries").cloned().unwrap_or(serde_json::json!([])),
        }),
        "code_interpreter_call" => serde_json::json!({
            "code": item.get("code"),
            "container_id": item.get("container_id"),
        }),
        "computer_use_call" => {
            item.get("action").cloned().unwrap_or(serde_json::json!({}))
        }
        "image_generation_call" => serde_json::json!({}),
        "mcp_call" => serde_json::json!({
            "server_label": item.get("server_label"),
            "name": item.get("name"),
            "arguments": item.get("arguments"),
        }),
        "local_shell_call" => {
            item.get("action").cloned().unwrap_or(serde_json::json!({}))
        }
        _ => serde_json::json!({}),
    }
}

/// Extract the result from a hosted tool item.
fn hosted_tool_result(item: &serde_json::Value) -> serde_json::Value {
    let has_error = item.get("error").and_then(|e| e.as_str()).is_some();
    if has_error {
        serde_json::json!({
            "type": "error",
            "value": item.get("error"),
        })
    } else {
        serde_json::json!({
            "type": "json",
            "value": item.clone(),
        })
    }
}

/// Handle `response.function_call_arguments.delta` events.
fn on_function_call_arguments_delta(
    data: &serde_json::Value,
    state: &mut ResponsesStreamState,
) -> Vec<LlmEvent> {
    let mut events = Vec::new();
    let item_id = match data.get("item_id").and_then(|v| v.as_str()) {
        Some(id) => id.to_string(),
        _ => return events,
    };
    let delta = match data.get("delta").and_then(|d| d.as_str()) {
        Some(d) if !d.is_empty() => d,
        _ => return events,
    };

    let tool = match state.pending_tools.get_mut(&item_id) {
        Some(tool) => tool,
        None => return events,
    };

    tool.arguments.push_str(delta);

    if !state.step_started {
        events.push(LlmEvent::StepStart { index: 0 });
        state.step_started = true;
    }

    events.push(LlmEvent::ToolInputDelta {
        id: tool.call_id.clone(),
        name: tool.name.clone(),
        text: delta.to_string(),
    });

    events
}

/// Handle `response.output_item.done` events.
///
/// Completes a function_call item (parsing accumulated JSON into a ToolCall),
/// or handles reasoning item completion.
fn on_output_item_done(data: &serde_json::Value, state: &mut ResponsesStreamState) -> Vec<LlmEvent> {
    let item = match data.get("item") {
        Some(item) => item,
        None => return Vec::new(),
    };

    let item_type = match item.get("type").and_then(|t| t.as_str()) {
        Some(t) => t,
        None => return Vec::new(),
    };

    match item_type {
        "function_call" => on_function_call_finished(item, state),
        "reasoning" => on_reasoning_finished(item, state),
        s if is_hosted_tool_type(s) => {
            // Hosted tools are already handled in output_item_added
            // This is a no-op to avoid duplicate events
            Vec::new()
        }
        _ => Vec::new(),
    }
}

/// Handle a function_call item completing.
fn on_function_call_finished(item: &serde_json::Value, state: &mut ResponsesStreamState) -> Vec<LlmEvent> {
    let mut events = Vec::new();
    let item_id = match item.get("id").and_then(|v| v.as_str()) {
        Some(id) if !id.is_empty() => id.to_string(),
        _ => return events,
    };
    let call_id = item
        .get("call_id")
        .and_then(|v| v.as_str())
        .unwrap_or("")
        .to_string();
    let name = item
        .get("name")
        .and_then(|v| v.as_str())
        .unwrap_or("")
        .to_string();

    // Check if we already have a pending tool for this item
    if let Some(pending) = state.pending_tools.remove(&item_id) {
        // Use the accumulated arguments from streaming
        let arguments = &pending.arguments;
        let input: serde_json::Value = if arguments.trim().is_empty() {
            serde_json::Value::Object(serde_json::Map::new())
        } else {
            serde_json::from_str(arguments).unwrap_or_else(|_| {
                serde_json::Value::String(arguments.clone())
            })
        };

        if !state.step_started {
            events.push(LlmEvent::StepStart { index: 0 });
            state.step_started = true;
        }

        events.push(LlmEvent::ToolInputEnd {
            id: pending.call_id.clone(),
            name: pending.name.clone(),
            provider_metadata: None,
        });

        events.push(LlmEvent::ToolCall {
            id: pending.call_id,
            name: pending.name,
            input,
            provider_executed: None,
            provider_metadata: None,
        });

        state.has_function_call = true;
    } else {
        // No pending tool (arguments arrived entirely in output_item.added)
        let arguments = item
            .get("arguments")
            .and_then(|v| v.as_str())
            .unwrap_or("");
        let input: serde_json::Value = if arguments.trim().is_empty() {
            serde_json::Value::Object(serde_json::Map::new())
        } else {
            serde_json::from_str(arguments).unwrap_or_else(|_| {
                serde_json::Value::String(arguments.to_string())
            })
        };

        if !state.step_started {
            events.push(LlmEvent::StepStart { index: 0 });
            state.step_started = true;
        }

        events.push(LlmEvent::ToolCall {
            id: call_id,
            name,
            input,
            provider_executed: None,
            provider_metadata: None,
        });

        state.has_function_call = true;
    }

    events
}

/// Handle a reasoning item completing.
fn on_reasoning_finished(
    _item: &serde_json::Value,
    state: &mut ResponsesStreamState,
) -> Vec<LlmEvent> {
    let mut events = Vec::new();

    if state.reasoning_started {
        state.reasoning_started = false;
        events.push(LlmEvent::ReasoningEnd {
            id: "reasoning-0".into(),
            provider_metadata: None,
        });
    }

    events
}

/// Handle `response.completed` and `response.incomplete` events.
fn on_response_finish(data: &serde_json::Value, state: &mut ResponsesStreamState) -> Vec<LlmEvent> {
    let mut events = Vec::new();

    // Parse usage
    if let Some(usage_val) = data.get("response").and_then(|r| r.get("usage")) {
        if let Some(u) = map_responses_usage(usage_val) {
            state.usage = Some(u);
        }
    }

    // Close any open blocks
    if state.text_started {
        events.push(LlmEvent::TextEnd {
            id: "text-0".into(),
            provider_metadata: None,
        });
    }
    if state.reasoning_started {
        events.push(LlmEvent::ReasoningEnd {
            id: "reasoning-0".into(),
            provider_metadata: None,
        });
    }

    // Determine finish reason
    let incomplete_reason = data
        .get("response")
        .and_then(|r| r.get("incomplete_details"))
        .and_then(|d| d.get("reason"))
        .and_then(|v| v.as_str());
    let reason = map_responses_finish_reason(incomplete_reason, state.has_function_call);

    // Extract provider metadata
    let response_id = data
        .get("response")
        .and_then(|r| r.get("id"))
        .and_then(|v| v.as_str());
    let service_tier = data
        .get("response")
        .and_then(|r| r.get("service_tier"))
        .and_then(|v| v.as_str());
    let provider_metadata = response_id.or(service_tier).map(|_| {
        let mut m = HashMap::new();
        if let Some(id) = response_id {
            m.insert("responseId".into(), serde_json::Value::String(id.to_string()));
        }
        if let Some(tier) = service_tier {
            m.insert("serviceTier".into(), serde_json::Value::String(tier.to_string()));
        }
        m
    });

    events.push(LlmEvent::StepFinish {
        index: 0,
        reason: reason.clone(),
        usage: state.usage.clone(),
        provider_metadata: provider_metadata.clone(),
    });

    events.push(LlmEvent::Finish {
        reason,
        usage: state.usage.clone(),
        provider_metadata,
    });

    state.finished = true;

    events
}

/// Handle `response.failed` events.
fn on_response_failed(data: &serde_json::Value, _state: &mut ResponsesStreamState) -> Vec<LlmEvent> {
    let error = data.get("response").and_then(|r| r.get("error"));
    let message = error
        .and_then(|e| e.get("message"))
        .and_then(|v| v.as_str())
        .unwrap_or("OpenAI Responses response failed");
    let code = error
        .and_then(|e| e.get("code"))
        .and_then(|v| v.as_str());

    let full_message = match code {
        Some(c) => format!("{c}: {message}"),
        None => message.to_string(),
    };

    let classification = if code == Some("context_length_exceeded")
        || crate::error::is_context_overflow(&full_message)
    {
        Some("context-overflow".into())
    } else {
        None
    };

    vec![LlmEvent::ProviderErrorEvent {
        message: full_message,
        classification,
        retryable: None,
        provider_metadata: None,
    }]
}

/// Handle `error` events from the SSE stream.
fn on_error_event(data: &serde_json::Value, _state: &mut ResponsesStreamState) -> Vec<LlmEvent> {
    let message = data
        .get("message")
        .and_then(|v| v.as_str())
        .unwrap_or("OpenAI Responses stream error");
    let code = data.get("code").and_then(|v| v.as_str());

    let full_message = match code {
        Some(c) => format!("{c}: {message}"),
        None => message.to_string(),
    };

    let classification = if code == Some("context_length_exceeded")
        || crate::error::is_context_overflow(&full_message)
    {
        Some("context-overflow".into())
    } else {
        None
    };

    vec![LlmEvent::ProviderErrorEvent {
        message: full_message,
        classification,
        retryable: None,
        provider_metadata: None,
    }]
}

/// SSE framing helper that wraps [`parse_sse_stream`] and drives the
/// [`ResponsesStreamState`] machine, yielding `Result<LlmEvent>` items.
///
/// Dispatches on the `event:` field of each SSE frame to determine the
/// Responses API event type.
pub fn create_responses_stream(
    response: reqwest::Response,
    provider_id: impl Into<String> + Clone + Send + 'static,
) -> Box<dyn futures::Stream<Item = Result<LlmEvent, Error>> + Send + Unpin> {
    let sse_stream = parse_sse_stream(response);
    let state = ResponsesStreamState::new();
    let provider_id = provider_id.into();

    let stream = futures::stream::unfold(
        (
            Box::pin(sse_stream)
                as std::pin::Pin<
                    Box<
                        dyn futures::Stream<Item = Result<SseEvent, SseError>> + Send + Unpin,
                    >,
                >,
            state,
            VecDeque::<Result<LlmEvent, Error>>::new(),
        ),
        move |(mut sse, mut state, mut buffer)| {
            let pid = provider_id.clone();
            Box::pin(async move {
                loop {
                    if let Some(ev) = buffer.pop_front() {
                        return Some((ev, (sse, state, buffer)));
                    }
                    if state.finished {
                        return None;
                    }
                    match sse.next().await {
                        Some(Ok(se)) if !se.is_done() && se.has_data() => {
                            if let Ok(value) =
                                serde_json::from_str::<serde_json::Value>(&se.data)
                            {
                                let evts =
                                    events_from_responses_event(&se, &value, &mut state);
                                for ev in evts {
                                    buffer.push_back(Ok(ev));
                                }
                            }
                            if let Some(ev) = buffer.pop_front() {
                                return Some((ev, (sse, state, buffer)));
                            }
                        }
                        Some(Err(e)) => {
                            return Some((
                                Err(Error::ResponseStream(format!("{pid} SSE: {e}"))),
                                (sse, state, buffer),
                            ));
                        }
                        None => return None,
                        _ => continue,
                    }
                }
            })
        },
    );
    Box::new(stream)
}

/// Classify an HTTP error response from the OpenAI Responses API.
///
/// Reuses the same status-code-based classification as Chat Completions
/// since the Responses API uses the same HTTP error conventions.
pub fn classify_error(status: u16, body: &str) -> LlmErrorReason {
    let msg = || body.to_string();
    match status {
        401 | 403 => LlmErrorReason::Authentication {
            message: msg(),
            kind: crate::error::AuthErrorKind::Invalid,
        },
        429 => LlmErrorReason::RateLimit {
            message: msg(),
            retry_after_ms: None,
        },
        400 | 413 => {
            if crate::error::is_context_overflow(body) {
                LlmErrorReason::InvalidRequest {
                    message: msg(),
                    parameter: None,
                    classification: Some("context-overflow".into()),
                }
            } else {
                LlmErrorReason::InvalidRequest {
                    message: msg(),
                    parameter: None,
                    classification: None,
                }
            }
        }
        500..=599 => LlmErrorReason::ProviderInternal {
            message: msg(),
            status,
            retry_after_ms: None,
        },
        _ => LlmErrorReason::UnknownProvider {
            message: msg(),
            status: Some(status),
        },
    }
}

// ═══════════════════════════════════════════════════════════════════════
// 3. ResponsesProvider
// ═══════════════════════════════════════════════════════════════════════

pub struct ResponsesProvider {
    api_key: String,
    base_url: String,
    http_client: reqwest::Client,
    models: Vec<Model>,
}

impl ResponsesProvider {
    pub fn new() -> Result<Self, Error> {
        Self::with_base_url(resolve_api_key()?, DEFAULT_BASE_URL.into())
    }

    pub fn with_base_url(api_key: String, base_url: String) -> Result<Self, Error> {
        let http_client = reqwest::Client::builder()
            .user_agent(format!("rustcode/{}", env!("CARGO_PKG_VERSION")))
            .timeout(std::time::Duration::from_secs(300))
            .build()
            .map_err(|e| Error::Network(format!("HTTP client: {e}")))?;
        Ok(Self {
            api_key,
            base_url,
            http_client,
            models: build_model_catalog(),
        })
    }

    pub fn with_api_key(api_key: String) -> Result<Self, Error> {
        Self::with_base_url(api_key, DEFAULT_BASE_URL.into())
    }

    fn responses_url(&self) -> String {
        format!("{}{RESPONSES_PATH}", self.base_url.trim_end_matches('/'))
    }

    /// Build options from a model's config and defaults.
    fn build_options(&self, model: &Model) -> ResponsesBodyOptions {
        let mut options = ResponsesBodyOptions::default();

        // Extract options from model.options and config defaults
        let api_id = model.api.id.to_lowercase();

        // GPT-5 defaults: medium reasoning effort, auto summary
        if api_id.contains("gpt-5") && !api_id.contains("gpt-5-chat") {
            options.reasoning_effort = Some("medium".into());
            options.reasoning_summary = Some("auto".into());
            options.include = Some(vec!["reasoning.encrypted_content".into()]);
            // textVerbosity for non-chat GPT-5.x
            if api_id.contains("gpt-5.")
                && !api_id.contains("codex")
                && !api_id.contains("-chat")
            {
                options.text_verbosity = Some("low".into());
            }
        }

        // Override from model.options if present
        if let Some(reasoning_effort) = model.options.get("reasoningEffort") {
            if let Some(s) = reasoning_effort.as_str() {
                options.reasoning_effort = Some(s.to_string());
            }
        }
        if let Some(reasoning_summary) = model.options.get("reasoningSummary") {
            if let Some(s) = reasoning_summary.as_str() {
                options.reasoning_summary = Some(s.to_string());
            }
        }
        if let Some(include) = model.options.get("include") {
            if let Some(arr) = include.as_array() {
                let strs: Vec<String> = arr
                    .iter()
                    .filter_map(|v| v.as_str().map(|s| s.to_string()))
                    .collect();
                if !strs.is_empty() {
                    options.include = Some(strs);
                }
            }
        }
        if let Some(text_verbosity) = model.options.get("textVerbosity") {
            if let Some(s) = text_verbosity.as_str() {
                options.text_verbosity = Some(s.to_string());
            }
        }
        if let Some(store) = model.options.get("store") {
            if let Some(b) = store.as_bool() {
                options.store = Some(b);
            }
        }

        options
    }
}

fn resolve_api_key() -> Result<String, Error> {
    std::env::var("OPENAI_API_KEY")
        .ok()
        .filter(|k| !k.is_empty())
        .ok_or_else(|| Error::Auth("OPENAI_API_KEY environment variable not set".into()))
}

#[async_trait::async_trait]
impl crate::provider::Provider for ResponsesProvider {
    fn provider_id(&self) -> &str {
        "openai-responses"
    }
    fn npm(&self) -> &str {
        "@ai-sdk/openai"
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
                provider_id: "openai".into(),
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
        let options = self.build_options(model);
        let body = build_responses_body(model, &messages, tools, options);

        let response = self
            .http_client
            .post(self.responses_url())
            .header("Authorization", format!("Bearer {}", self.api_key))
            .header("Content-Type", "application/json")
            .json(&body)
            .send()
            .await
            .map_err(|e| Error::Network(format!("OpenAI Responses request: {e}")))?;

        if !response.status().is_success() {
            let status = response.status().as_u16();
            let text = response.text().await.unwrap_or_default();
            return Err(Error::Llm { http_context: None, 
                module: "openai-responses".into(),
                method: "stream".into(),
                reason: Box::new(classify_error(status, &text)),
            });
        }

        Ok(create_responses_stream(response, "openai-responses"))
    }

    async fn complete(
        &self,
        model: &Model,
        messages: &[ChatMessage],
        tools: &[ToolDefinition],
    ) -> crate::error::Result<crate::provider::LlmResponse> {
        let mut stream = self.stream(model, messages, tools).await?;
        let mut events = Vec::new();
        let mut usage = None;
        while let Some(r) = stream.next().await {
            if let Ok(ev) = r {
                if let Some(u) = ev.usage() {
                    usage = Some(u.clone());
                }
                events.push(ev);
            }
        }
        Ok(crate::provider::LlmResponse { events, usage })
    }
}

fn build_model_catalog() -> Vec<Model> {
    vec![
        make_model(
            "gpt-5.2",
            "GPT-5.2",
            200_000,
            128_000,
            1.75,
            14.0,
        ),
        make_model(
            "gpt-5.1",
            "GPT-5.1",
            200_000,
            128_000,
            1.75,
            14.0,
        ),
        make_model(
            "gpt-5.1-codex",
            "GPT-5.1 Codex",
            200_000,
            128_000,
            1.75,
            14.0,
        ),
        make_model(
            "gpt-5.1-mini",
            "GPT-5.1 Mini",
            200_000,
            128_000,
            0.35,
            1.40,
        ),
        make_model(
            "gpt-5.1-nano",
            "GPT-5.1 Nano",
            200_000,
            128_000,
            0.10,
            0.40,
        ),
    ]
}

fn make_model(id: &str, name: &str, ctx: u64, out: u64, inp_cost: f64, out_cost: f64) -> Model {
    Model {
        id: id.into(),
        provider_id: "openai-responses".into(),
        name: name.into(),
        api: crate::provider::ApiInfo {
            id: id.into(),
            url: DEFAULT_BASE_URL.into(),
            npm: "@ai-sdk/openai".into(),
        },
        family: Some("gpt".into()),
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
            interleaved: Default::default(),
        },
        cost: crate::provider::Cost {
            input: inp_cost,
            output: out_cost,
            cache: Default::default(),
            tiers: None,
            experimental_over_200k: None,
        },
        limit: crate::provider::TokenLimit {
            context: ctx,
            input: None,
            output: out,
        },
        status: crate::provider::ModelStatus::Active,
        options: {
            let mut opts = std::collections::HashMap::new();
            opts.insert("store".into(), serde_json::json!(false));
            opts
        },
        headers: std::collections::HashMap::new(),
        release_date: "2026".into(),
        variants: None,
    }
}

// ═══════════════════════════════════════════════════════════════════════
// Tests
// ═══════════════════════════════════════════════════════════════════════

#[cfg(test)]
mod tests {
    use super::*;
    use crate::provider::{ContentPart, MessageContent};

    // ── extract_text ──────────────────────────────────────────────

    #[test]
    fn test_extract_text_from_text() {
        let content = MessageContent::Text("hello world".into());
        assert_eq!(extract_text(&content), "hello world");
    }

    #[test]
    fn test_extract_text_from_parts() {
        let content = MessageContent::Parts(vec![
            ContentPart::Text { text: "hello ".into() },
            ContentPart::Text { text: "world".into() },
        ]);
        assert_eq!(extract_text(&content), "hello world");
    }

    // ── lower_messages_to_input (system) ──────────────────────────

    #[test]
    fn test_lower_system_message() {
        let msgs = vec![ChatMessage::System {
            content: MessageContent::Text("You are a helpful assistant.".into()),
        }];
        let input = lower_messages_to_input(&msgs);
        assert_eq!(input.len(), 1);
        assert_eq!(input[0]["role"], "system");
        assert_eq!(input[0]["content"], "You are a helpful assistant.");
    }

    // ── lower_messages_to_input (user text) ───────────────────────

    #[test]
    fn test_lower_user_text() {
        let msgs = vec![ChatMessage::User {
            content: MessageContent::Text("Hello".into()),
        }];
        let input = lower_messages_to_input(&msgs);
        assert_eq!(input.len(), 1);
        assert_eq!(input[0]["role"], "user");
        assert_eq!(input[0]["content"][0]["type"], "input_text");
        assert_eq!(input[0]["content"][0]["text"], "Hello");
    }

    // ── lower_messages_to_input (user with image) ─────────────────

    #[test]
    fn test_lower_user_with_image() {
        let msgs = vec![ChatMessage::User {
            content: MessageContent::Parts(vec![
                ContentPart::Text { text: "What's in this image? ".into() },
                ContentPart::Image { image: "base64data".into() },
            ]),
        }];
        let input = lower_messages_to_input(&msgs);
        assert_eq!(input.len(), 1);
        let content = input[0]["content"].as_array().unwrap();
        assert_eq!(content.len(), 2);
        assert_eq!(content[0]["type"], "input_text");
        assert_eq!(content[1]["type"], "input_image");
    }

    // ── lower_messages_to_input (assistant text) ──────────────────

    #[test]
    fn test_lower_assistant_text() {
        let msgs = vec![ChatMessage::Assistant {
            content: MessageContent::Text("I can help!".into()),
        }];
        let input = lower_messages_to_input(&msgs);
        assert_eq!(input.len(), 1);
        assert_eq!(input[0]["role"], "assistant");
        assert_eq!(input[0]["content"][0]["type"], "output_text");
        assert_eq!(input[0]["content"][0]["text"], "I can help!");
    }

    // ── lower_messages_to_input (assistant with tool call) ────────

    #[test]
    fn test_lower_assistant_tool_call() {
        let msgs = vec![ChatMessage::Assistant {
            content: MessageContent::Parts(vec![
                ContentPart::Text { text: "Looking up..." .into() },
                ContentPart::ToolCallPart {
                    tool_call_id: "call_1".into(),
                    tool_name: "bash".into(),
                    arguments: serde_json::json!({"cmd": "ls"}),
                },
            ]),
        }];
        let input = lower_messages_to_input(&msgs);
        // Should have: assistant output_text + function_call
        assert_eq!(input.len(), 2);
        assert_eq!(input[0]["role"], "assistant");
        assert_eq!(input[1]["type"], "function_call");
        assert_eq!(input[1]["call_id"], "call_1");
        assert_eq!(input[1]["name"], "bash");
    }

    // ── lower_messages_to_input (tool result) ─────────────────────

    #[test]
    fn test_lower_tool_result() {
        let msgs = vec![ChatMessage::Tool {
            content: vec![super::crate::provider::ToolResultPart::ToolResult {
                tool_call_id: "call_1".into(),
                tool_name: "bash".into(),
                output: serde_json::json!({"stdout": "hello"}),
                is_error: false,
            }],
        }];
        let input = lower_messages_to_input(&msgs);
        assert_eq!(input.len(), 1);
        assert_eq!(input[0]["type"], "function_call_output");
        assert_eq!(input[0]["call_id"], "call_1");
    }

    // ── lower_tools_to_responses ──────────────────────────────────

    #[test]
    fn test_lower_tools_empty() {
        let tools: Vec<ToolDefinition> = vec![];
        assert!(lower_tools_to_responses(&tools).is_empty());
    }

    #[test]
    fn test_lower_tools_single() {
        let tools = vec![ToolDefinition {
            name: "bash".into(),
            description: "Run a shell command".into(),
            parameters: serde_json::json!({"type": "object", "properties": {}}),
        }];
        let result = lower_tools_to_responses(&tools);
        assert_eq!(result.len(), 1);
        assert_eq!(result[0]["type"], "function");
        assert_eq!(result[0]["name"], "bash");
    }

    // ── build_responses_body ──────────────────────────────────────

    fn stub_model() -> Model {
        Model {
            id: "gpt-5.1".into(),
            provider_id: "openai".into(),
            name: "GPT-5.1".into(),
            api: crate::provider::ApiInfo {
                id: "gpt-5.1".into(),
                url: DEFAULT_BASE_URL.into(),
                npm: "@ai-sdk/openai".into(),
            },
            family: Some("gpt".into()),
            capabilities: crate::provider::Capabilities::default(),
            cost: crate::provider::Cost::default(),
            limit: crate::provider::TokenLimit {
                context: 200_000,
                input: None,
                output: 128_000,
            },
            status: crate::provider::ModelStatus::Active,
            options: std::collections::HashMap::new(),
            headers: std::collections::HashMap::new(),
            release_date: String::new(),
            variants: None,
        }
    }

    #[test]
    fn test_build_responses_body_basic() {
        let model = stub_model();
        let msgs = vec![ChatMessage::User {
            content: MessageContent::Text("Hello".into()),
        }];
        let body = build_responses_body(&model, &msgs, &[], ResponsesBodyOptions::default());
        assert_eq!(body["model"], "gpt-5.1");
        assert_eq!(body["stream"], true);
        assert_eq!(body["input"][0]["role"], "user");
    }

    #[test]
    fn test_build_responses_body_with_instructions() {
        let model = stub_model();
        let msgs = vec![ChatMessage::User {
            content: MessageContent::Text("Hi".into()),
        }];
        let body = build_responses_body(
            &model,
            &msgs,
            &[],
            ResponsesBodyOptions {
                instructions: Some("You are helpful.".into()),
                ..ResponsesBodyOptions::default()
            },
        );
        assert_eq!(body["instructions"], "You are helpful.");
    }

    #[test]
    fn test_build_responses_body_with_reasoning() {
        let model = stub_model();
        let body = build_responses_body(
            &model,
            &[],
            &[],
            ResponsesBodyOptions {
                reasoning_effort: Some("medium".into()),
                reasoning_summary: Some("auto".into()),
                ..ResponsesBodyOptions::default()
            },
        );
        assert_eq!(body["reasoning"]["effort"], "medium");
        assert_eq!(body["reasoning"]["summary"], "auto");
    }

    // ── map_responses_finish_reason ───────────────────────────────

    #[test]
    fn test_finish_reason_stop() {
        assert_eq!(
            map_responses_finish_reason(None, false),
            FinishReason::Stop
        );
    }

    #[test]
    fn test_finish_reason_tool_calls() {
        assert_eq!(
            map_responses_finish_reason(None, true),
            FinishReason::ToolCalls
        );
    }

    #[test]
    fn test_finish_reason_length() {
        assert_eq!(
            map_responses_finish_reason(Some("max_output_tokens"), false),
            FinishReason::Length
        );
    }

    #[test]
    fn test_finish_reason_content_filter() {
        assert_eq!(
            map_responses_finish_reason(Some("content_filter"), false),
            FinishReason::ContentFilter
        );
    }

    // ── map_responses_usage ───────────────────────────────────────

    #[test]
    fn test_map_responses_usage_basic() {
        let u = serde_json::json!({
            "input_tokens": 100,
            "output_tokens": 50,
            "total_tokens": 150,
        });
        let usage = map_responses_usage(&u).unwrap();
        assert_eq!(usage.input_tokens, Some(100));
        assert_eq!(usage.output_tokens, Some(50));
        assert_eq!(usage.total_tokens, Some(150));
    }

    #[test]
    fn test_map_responses_usage_with_cache_and_reasoning() {
        let u = serde_json::json!({
            "input_tokens": 1000,
            "output_tokens": 500,
            "total_tokens": 1500,
            "input_tokens_details": {"cached_tokens": 300},
            "output_tokens_details": {"reasoning_tokens": 200},
        });
        let usage = map_responses_usage(&u).unwrap();
        assert_eq!(usage.input_tokens, Some(1000));
        assert_eq!(usage.cache_read_input_tokens, Some(300));
        assert_eq!(usage.non_cached_input_tokens, Some(700));
        assert_eq!(usage.reasoning_tokens, Some(200));
    }

    // ── Stream state machine ──────────────────────────────────────

    #[test]
    fn test_stream_state_default() {
        let state = ResponsesStreamState::new();
        assert!(!state.text_started);
        assert!(!state.reasoning_started);
        assert!(!state.step_started);
        assert!(!state.finished);
        assert!(!state.has_function_call);
        assert!(state.usage.is_none());
        assert!(state.pending_tools.is_empty());
    }

    #[test]
    fn test_on_output_text_delta_starts_text() {
        let data = serde_json::json!({"delta": "Hello"});
        let mut state = ResponsesStreamState::new();
        let events = on_output_text_delta(&data, &mut state);
        assert!(!events.is_empty());
        assert!(events.iter().any(|e| matches!(e, LlmEvent::TextStart { .. })));
        assert!(events.iter().any(|e| matches!(e, LlmEvent::TextDelta { .. })));
        assert!(state.text_started);
        assert!(state.step_started);
    }

    #[test]
    fn test_on_output_text_delta_subsequent_no_start() {
        let data1 = serde_json::json!({"delta": "Hello"});
        let data2 = serde_json::json!({"delta": " World"});
        let mut state = ResponsesStreamState::new();
        let _ = on_output_text_delta(&data1, &mut state);
        let events = on_output_text_delta(&data2, &mut state);
        // No TextStart on subsequent deltas
        assert!(!events.iter().any(|e| matches!(e, LlmEvent::TextStart { .. })));
        assert!(events.iter().any(|e| matches!(e, LlmEvent::TextDelta { .. })));
    }

    #[test]
    fn test_on_reasoning_delta_starts_reasoning() {
        let data = serde_json::json!({"delta": "Let me think..."});
        let mut state = ResponsesStreamState::new();
        let events = on_reasoning_delta(&data, &mut state);
        assert!(events.iter().any(|e| matches!(e, LlmEvent::ReasoningStart { .. })));
        assert!(events.iter().any(|e| matches!(e, LlmEvent::ReasoningDelta { .. })));
        assert!(state.reasoning_started);
    }

    #[test]
    fn test_on_output_text_delta_empty_noop() {
        let data = serde_json::json!({"delta": ""});
        let mut state = ResponsesStreamState::new();
        let events = on_output_text_delta(&data, &mut state);
        assert!(events.is_empty());
    }

    #[test]
    fn test_on_reasoning_delta_empty_noop() {
        let data = serde_json::json!({"delta": ""});
        let mut state = ResponsesStreamState::new();
        let events = on_reasoning_delta(&data, &mut state);
        assert!(events.is_empty());
    }

    #[test]
    fn test_on_function_call_added_starts_tool() {
        let data = serde_json::json!({
            "item": {
                "id": "item_1",
                "type": "function_call",
                "call_id": "call_1",
                "name": "bash",
                "arguments": ""
            }
        });
        let mut state = ResponsesStreamState::new();
        let events = on_output_item_added(&data, &mut state);
        assert!(events.iter().any(|e| matches!(e, LlmEvent::ToolInputStart { .. })));
        assert!(state.pending_tools.contains_key("item_1"));
    }

    #[test]
    fn test_on_function_call_arguments_delta_appends() {
        let data_start = serde_json::json!({
            "item": {
                "id": "item_1",
                "type": "function_call",
                "call_id": "call_1",
                "name": "bash",
                "arguments": ""
            }
        });
        let mut state = ResponsesStreamState::new();
        let _ = on_output_item_added(&data_start, &mut state);

        let data_delta = serde_json::json!({
            "item_id": "item_1",
            "delta": "{\"cmd\":\"ls\"}"
        });
        let events = on_function_call_arguments_delta(&data_delta, &mut state);
        assert!(events.iter().any(|e| matches!(e, LlmEvent::ToolInputDelta { .. })));
        assert_eq!(
            state.pending_tools.get("item_1").unwrap().arguments,
            "{\"cmd\":\"ls\"}"
        );
    }

    #[test]
    fn test_on_function_call_done_emits_tool_call() {
        let mut state = ResponsesStreamState::new();

        // Start the tool call
        let data_start = serde_json::json!({
            "item": {
                "id": "item_1",
                "type": "function_call",
                "call_id": "call_1",
                "name": "bash",
                "arguments": ""
            }
        });
        let _ = on_output_item_added(&data_start, &mut state);

        // Add arguments
        let data_delta = serde_json::json!({
            "item_id": "item_1",
            "delta": "{\"cmd\":\"ls\"}"
        });
        let _ = on_function_call_arguments_delta(&data_delta, &mut state);

        // Complete the tool call
        let data_done = serde_json::json!({
            "item": {
                "id": "item_1",
                "type": "function_call",
                "call_id": "call_1",
                "name": "bash",
                "arguments": "{\"cmd\":\"ls\"}"
            }
        });
        let events = on_output_item_done(&data_done, &mut state);
        assert!(events.iter().any(|e| matches!(e, LlmEvent::ToolCall { .. })));
        assert!(state.has_function_call);
    }

    #[test]
    fn test_on_response_complete_emits_finish() {
        let mut state = ResponsesStreamState::new();
        state.text_started = true;

        let data = serde_json::json!({
            "response": {
                "id": "resp_1",
                "usage": {
                    "input_tokens": 100,
                    "output_tokens": 50,
                    "total_tokens": 150
                }
            }
        });
        let events = on_response_finish(&data, &mut state);
        assert!(events.iter().any(|e| matches!(e, LlmEvent::TextEnd { .. })));
        assert!(events.iter().any(|e| matches!(e, LlmEvent::StepFinish { .. })));
        assert!(events.iter().any(|e| matches!(e, LlmEvent::Finish { .. })));
        assert!(state.finished);
        assert!(state.usage.is_some());
    }

    #[test]
    fn test_on_response_failed_emits_provider_error() {
        let mut state = ResponsesStreamState::new();
        let data = serde_json::json!({
            "response": {
                "error": {
                    "code": "rate_limit_exceeded",
                    "message": "Too many requests"
                }
            }
        });
        let events = on_response_failed(&data, &mut state);
        assert!(!events.is_empty());
        assert!(events.iter().any(|e| matches!(e, LlmEvent::ProviderErrorEvent { .. })));
    }

    #[test]
    fn test_on_error_event_emits_provider_error() {
        let mut state = ResponsesStreamState::new();
        let data = serde_json::json!({
            "code": "context_length_exceeded",
            "message": "Input exceeds context window"
        });
        let events = on_error_event(&data, &mut state);
        assert!(!events.is_empty());
        assert!(events.iter().any(|e| {
            matches!(e, LlmEvent::ProviderErrorEvent { classification: Some(c), .. } if c == "context-overflow")
        }));
    }

    // ── Hosted tools ──────────────────────────────────────────────

    #[test]
    fn test_hosted_tool_web_search() {
        let data = serde_json::json!({
            "item": {
                "id": "hs_1",
                "type": "web_search_call",
                "action": {"query": "weather"}
            }
        });
        let mut state = ResponsesStreamState::new();
        let events = on_output_item_added(&data, &mut state);
        assert!(events.iter().any(|e| matches!(e, LlmEvent::ToolCall { provider_executed: Some(true), .. })));
        assert!(events.iter().any(|e| matches!(e, LlmEvent::ToolResult { .. })));
    }

    // ── classify_error ────────────────────────────────────────────

    #[test]
    fn test_classify_error_auth() {
        let reason = classify_error(401, "Unauthorized");
        assert!(matches!(
            reason,
            LlmErrorReason::Authentication { .. }
        ));
    }

    #[test]
    fn test_classify_error_rate_limit() {
        let reason = classify_error(429, "Rate limited");
        assert!(matches!(
            reason,
            LlmErrorReason::RateLimit { .. }
        ));
    }

    #[test]
    fn test_classify_error_context_overflow() {
        let reason = classify_error(400, "This input exceeds the context window");
        assert!(matches!(
            reason,
            LlmErrorReason::InvalidRequest {
                classification: Some(ref c),
                ..
            } if c == "context-overflow"
        ));
    }

    #[test]
    fn test_classify_error_provider_internal() {
        let reason = classify_error(500, "Internal error");
        assert!(matches!(
            reason,
            LlmErrorReason::ProviderInternal { status: 500, .. }
        ));
    }

    // ── events_from_responses_event dispatch ──────────────────────

    #[test]
    fn test_events_from_responses_event_unknown_type() {
        let se = SseEvent {
            event_type: Some("unknown.event".into()),
            data: "{}".into(),
            id: None,
            retry_ms: None,
        };
        let data = serde_json::json!({});
        let mut state = ResponsesStreamState::new();
        let events = events_from_responses_event(&se, &data, &mut state);
        assert!(events.is_empty());
    }

    #[test]
    fn test_events_from_responses_event_text_delta() {
        let se = SseEvent {
            event_type: Some("response.output_text.delta".into()),
            data: r#"{"delta":"Hello"}"#.into(),
            id: None,
            retry_ms: None,
        };
        let data = serde_json::from_str(&se.data).unwrap();
        let mut state = ResponsesStreamState::new();
        let events = events_from_responses_event(&se, &data, &mut state);
        assert!(!events.is_empty());
        assert!(events.iter().any(|e| matches!(e, LlmEvent::TextDelta { .. })));
    }

    // ── ResponsesStreamState default ──────────────────────────────

    #[test]
    fn test_responses_stream_state_default() {
        let state = ResponsesStreamState::default();
        assert!(!state.text_started);
        assert!(!state.reasoning_started);
        assert!(!state.finished);
    }

    // ── flush_assistant_text ──────────────────────────────────────

    #[test]
    fn test_flush_assistant_text_empty() {
        let mut input = Vec::new();
        let mut text_parts = Vec::new();
        flush_assistant_text(&mut input, &mut text_parts);
        assert!(input.is_empty());
    }

    #[test]
    fn test_flush_assistant_text_with_content() {
        let mut input = Vec::new();
        let mut text_parts = vec!["Hello".into(), " world".into()];
        flush_assistant_text(&mut input, &mut text_parts);
        assert_eq!(input.len(), 1);
        assert_eq!(input[0]["content"][0]["text"], "Hello world");
        assert!(text_parts.is_empty());
    }
}
