//! Shared Chat Completions protocol logic for OpenAI-compatible providers.
//!
//! Consolidates three concerns that were duplicated ~8 times across providers:
//!
//! 1. **`ChatCompletionsBody`** — request body construction: message lowering,
//!    tool lowering, and `build_chat_body()` with model parameters.
//! 2. **`ChatCompletionsStreamParser`** — SSE event parsing: parses `data: {...}`
//!    JSON, drives `ChatStreamState`, emits `LlmEvent` variants (text_delta,
//!    reasoning_delta, tool_call_delta, finish_reason, etc.).
//! 3. **`ChatCompletionsErrorMapper`** — HTTP error classification: maps status
//!    codes + error body to `LlmErrorReason`.
//!
//! Also provides `create_chat_stream()` which composes SSE parsing + state
//! machine into a single `futures::Stream<Item = Result<LlmEvent>>`, saving
//! ~50 lines of duplicated `futures::stream::unfold` per provider.
//!
//! Ported from: `packages/llm/src/protocols/openai-chat.ts` (493 lines)

use futures::StreamExt;
use std::collections::VecDeque;
use std::pin::Pin;

use crate::error::{Error, LlmErrorReason};
use crate::provider::{
    ChatMessage, ContentPart, FinishReason, LlmEvent, MessageContent, Model, ToolDefinition, Usage,
};
use crate::sse::{parse_sse_stream, SseError, SseEvent};
use crate::tool_stream::ToolStreamAccumulator;

// ═══════════════════════════════════════════════════════════════════════
// 1. Body Building
// ═══════════════════════════════════════════════════════════════════════

/// Options for customizing the Chat Completions request body.
#[derive(Debug, Default)]
pub struct BodyOptions {
    pub max_tokens: Option<u64>,
    pub temperature: Option<f64>,
    pub top_p: Option<f64>,
    pub frequency_penalty: Option<f64>,
    pub presence_penalty: Option<f64>,
    pub seed: Option<u64>,
    /// Extra top-level fields to merge into the body (e.g. reasoning_effort, store).
    pub extra_fields: Option<serde_json::Map<String, serde_json::Value>>,
    /// Tool choice override. If None, no tool_choice is sent.
    pub tool_choice: Option<serde_json::Value>,
}

/// Extract plain text from `MessageContent`, filtering out non-text parts.
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

/// Check if a message content has any image parts.
fn has_images(content: &MessageContent) -> bool {
    match content {
        MessageContent::Parts(p) => p.iter().any(|part| matches!(part, ContentPart::Image { .. })),
        _ => false,
    }
}

/// Build the request body for an OpenAI-compatible Chat Completions call.
///
/// Returns a JSON value ready for serialization. The caller adds `model`,
/// `stream`, and `stream_options` fields automatically.
pub fn build_chat_body(
    model: &Model,
    messages: &[ChatMessage],
    tools: &[ToolDefinition],
    options: BodyOptions,
) -> serde_json::Value {
    let msgs = lower_messages(messages);
    let tools_arr = lower_tools(tools);

    let mut body = serde_json::json!({
        "model": model.api.id,
        "messages": msgs,
        "stream": true,
        "stream_options": {"include_usage": true},
    });

    // Model parameters
    let mt = options
        .max_tokens
        .or_else(|| Some(crate::provider::max_output_tokens(model, crate::provider::OUTPUT_TOKEN_MAX)));
    if let Some(v) = mt {
        body["max_tokens"] = serde_json::json!(v);
    }
    let t = options.temperature.or_else(|| crate::provider::default_temperature(&model.api.id));
    if let Some(v) = t {
        body["temperature"] = serde_json::json!(v);
    }
    let tp = options.top_p.or_else(|| crate::provider::default_top_p(&model.api.id));
    if let Some(v) = tp {
        body["top_p"] = serde_json::json!(v);
    }
    if let Some(v) = options.frequency_penalty {
        body["frequency_penalty"] = serde_json::json!(v);
    }
    if let Some(v) = options.presence_penalty {
        body["presence_penalty"] = serde_json::json!(v);
    }
    if let Some(v) = options.seed {
        body["seed"] = serde_json::json!(v);
    }

    // Tools
    if !tools_arr.is_empty() {
        body["tools"] = serde_json::Value::Array(tools_arr);
    }
    if let Some(tc) = options.tool_choice {
        body["tool_choice"] = tc;
    }

    // Extra provider-specific fields
    if let Some(ref extra) = options.extra_fields {
        let obj = body.as_object_mut().unwrap();
        for (k, v) in extra {
            obj.insert(k.clone(), v.clone());
        }
    }

    body
}

/// Lower `ChatMessage`s to the OpenAI Chat Completions JSON format.
///
/// Handles system, user (text + image), assistant (text + reasoning + tool-call),
/// and tool messages. Images are embedded as data URIs.
pub fn lower_messages(messages: &[ChatMessage]) -> Vec<serde_json::Value> {
    let mut result = Vec::new();
    let mut pending_images: Vec<serde_json::Value> = Vec::new();

    for msg in messages {
        match msg {
            ChatMessage::System { content } => {
                let text = extract_text(content);
                if !text.is_empty() {
                    result.push(serde_json::json!({"role": "system", "content": text}));
                }
            }
            ChatMessage::User { content } => {
                let mut text_parts = String::new();
                let mut media_parts: Vec<serde_json::Value> = Vec::new();
                if let MessageContent::Parts(parts) = content {
                    for part in parts {
                        match part {
                            ContentPart::Text { text } => text_parts.push_str(text),
                            ContentPart::Image { image } => {
                                let url = if image.starts_with("data:") {
                                    image.clone()
                                } else {
                                    format!("data:image/png;base64,{image}")
                                };
                                media_parts.push(serde_json::json!({
                                    "type": "image_url",
                                    "image_url": { "url": url }
                                }));
                            }
                            _ => {}
                        }
                    }
                } else if let MessageContent::Text(t) = content {
                    text_parts = t.clone();
                }
                if !pending_images.is_empty() {
                    media_parts.append(&mut pending_images);
                }
                if media_parts.is_empty() {
                    result.push(serde_json::json!({"role": "user", "content": text_parts}));
                } else {
                    let mut parts = media_parts;
                    if !text_parts.is_empty() {
                        parts.insert(0, serde_json::json!({"type": "text", "text": text_parts}));
                    }
                    result.push(serde_json::json!({"role": "user", "content": parts}));
                }
            }
            ChatMessage::Assistant { content } => {
                let mut text = String::new();
                let mut tool_calls = Vec::new();
                let mut reasoning = String::new();
                match content {
                    MessageContent::Text(t) => text = t.clone(),
                    MessageContent::Parts(parts) => {
                        for part in parts {
                            match part {
                                ContentPart::Text { text: t } => text.push_str(t),
                                ContentPart::Reasoning { text: r, .. } => reasoning.push_str(r),
                                ContentPart::ToolCallPart {
                                    tool_call_id,
                                    tool_name,
                                    arguments,
                                } => {
                                    tool_calls.push(serde_json::json!({
                                        "id": tool_call_id,
                                        "type": "function",
                                        "function": {
                                            "name": tool_name,
                                            "arguments": arguments.to_string()
                                        }
                                    }));
                                }
                                _ => {}
                            }
                        }
                    }
                }
                let mut obj = serde_json::json!({"role": "assistant"});
                obj["content"] = if text.is_empty() {
                    serde_json::Value::Null
                } else {
                    serde_json::Value::String(text)
                };
                if !reasoning.is_empty() {
                    obj["reasoning_content"] = serde_json::Value::String(reasoning);
                }
                if !tool_calls.is_empty() {
                    obj["tool_calls"] = serde_json::Value::Array(tool_calls);
                }
                result.push(obj);
            }
            ChatMessage::Tool { content } => {
                for part in content {
                    let crate::provider::ToolResultPart::ToolResult {
                        tool_call_id,
                        output,
                        ..
                    } = part;
                    result.push(serde_json::json!({
                        "role": "tool",
                        "tool_call_id": tool_call_id,
                        "content": output.to_string()
                    }));
                }
            }
        }
    }
    result
}

/// Lower `ToolDefinition`s to the OpenAI tools array format.
pub fn lower_tools(tools: &[ToolDefinition]) -> Vec<serde_json::Value> {
    tools
        .iter()
        .map(|t| {
            serde_json::json!({
                "type": "function",
                "function": {
                    "name": t.name,
                    "description": t.description,
                    "parameters": t.parameters
                }
            })
        })
        .collect()
}

// ═══════════════════════════════════════════════════════════════════════
// 2. Stream Parsing — State Machine
// ═══════════════════════════════════════════════════════════════════════

/// Parser state for streaming Chat Completions SSE events.
///
/// Tracks text/reasoning/step start flags, tool call accumulation via
/// [`ToolStreamAccumulator`], usage, and the finished sentinel.
#[derive(Debug, Clone)]
pub struct ChatStreamState {
    /// Accumulator for streaming tool call JSON arguments.
    pub tool_stream: ToolStreamAccumulator,
    /// Whether a text delta has been emitted (for TextStart/TextEnd).
    pub text_started: bool,
    /// Whether a reasoning delta has been emitted (for ReasoningStart/ReasoningEnd).
    pub reasoning_started: bool,
    /// Whether a step has started (for StepStart).
    pub step_started: bool,
    /// Accumulated usage from the last event that carried it.
    pub usage: Option<Usage>,
    /// Whether the stream has received a finish_reason and should stop.
    pub finished: bool,
}

impl ChatStreamState {
    /// Create a new initial state.
    pub fn new() -> Self {
        Self {
            tool_stream: ToolStreamAccumulator::new(),
            text_started: false,
            reasoning_started: false,
            step_started: false,
            usage: None,
            finished: false,
        }
    }
}

impl Default for ChatStreamState {
    fn default() -> Self {
        Self::new()
    }
}

/// Map an OpenAI-style finish reason string to `FinishReason`.
pub fn map_finish_reason(reason: &str) -> FinishReason {
    match reason {
        "stop" => FinishReason::Stop,
        "length" => FinishReason::Length,
        "content_filter" => FinishReason::ContentFilter,
        "function_call" | "tool_calls" => FinishReason::ToolCalls,
        _ => FinishReason::Unknown,
    }
}

/// Parse usage from a `serde_json::Value` matching the OpenAI Chat Completions
/// usage object shape.
pub fn map_usage(usage: &serde_json::Value) -> Option<Usage> {
    let prompt_tokens = usage.get("prompt_tokens").and_then(|v| v.as_u64());
    let completion_tokens = usage.get("completion_tokens").and_then(|v| v.as_u64());
    let total_tokens = usage.get("total_tokens").and_then(|v| v.as_u64());

    let cached = usage
        .get("prompt_tokens_details")
        .and_then(|d| d.get("cached_tokens"))
        .and_then(|v| v.as_u64());

    let reasoning = usage
        .get("completion_tokens_details")
        .and_then(|d| d.get("reasoning_tokens"))
        .and_then(|v| v.as_u64());

    let non_cached = prompt_tokens.map(|p| p.saturating_sub(cached.unwrap_or(0)));

    Some(Usage {
        input_tokens: prompt_tokens,
        output_tokens: completion_tokens,
        non_cached_input_tokens: non_cached,
        cache_read_input_tokens: cached,
        cache_write_input_tokens: None,
        reasoning_tokens: reasoning,
        total_tokens,
        provider_metadata: None,
    })
}

/// Process one Chat Completions SSE event and emit zero or more `LlmEvent`s.
///
/// This is the core state machine. It handles:
/// - Reasoning deltas (start/delta)
/// - Text deltas (start/delta)
/// - Tool call deltas (accumulation via `ToolStreamAccumulator`)
/// - Finish reason → TextEnd, ReasoningEnd, StepFinish, Finish events
/// - Usage updates
pub fn events_from_chat(event: &serde_json::Value, state: &mut ChatStreamState) -> Vec<LlmEvent> {
    let mut events = Vec::new();

    // Update usage if present
    if let Some(usage_val) = event.get("usage") {
        if let Some(u) = map_usage(usage_val) {
            state.usage = Some(u);
        }
    }

    let choice = event
        .get("choices")
        .and_then(|c| c.as_array())
        .and_then(|c| c.first());

    let Some(choice) = choice else {
        return events;
    };

    let delta = choice.get("delta");

    if let Some(delta) = delta {
        // Reasoning content
        if let Some(rc) = delta.get("reasoning_content").and_then(|r| r.as_str()) {
            if !rc.is_empty() {
                if !state.reasoning_started {
                    state.reasoning_started = true;
                    events.push(LlmEvent::ReasoningStart {
                        id: "reasoning-0".into(),
                        provider_metadata: None,
                    });
                }
                events.push(LlmEvent::ReasoningDelta {
                    id: "reasoning-0".into(),
                    text: rc.to_string(),
                    provider_metadata: None,
                });
            }
        }

        // Text content
        if let Some(content) = delta.get("content").and_then(|c| c.as_str()) {
            if !content.is_empty() {
                if !state.text_started {
                    state.text_started = true;
                    events.push(LlmEvent::TextStart {
                        id: "text-0".into(),
                        provider_metadata: None,
                    });
                }
                events.push(LlmEvent::TextDelta {
                    id: "text-0".into(),
                    text: content.to_string(),
                    provider_metadata: None,
                });
            }
        }

        // Tool call deltas
        if let Some(tool_calls) = delta.get("tool_calls").and_then(|t| t.as_array()) {
            for td in tool_calls {
                let index = td.get("index").and_then(|i| i.as_u64()).unwrap_or(0);
                let name = td
                    .get("function")
                    .and_then(|f| f.get("name"))
                    .and_then(|n| n.as_str())
                    .map(|s| s.to_string());
                let id = td.get("id").and_then(|i| i.as_str()).map(|s| s.to_string());
                let args = td
                    .get("function")
                    .and_then(|f| f.get("arguments"))
                    .and_then(|a| a.as_str())
                    .map(|s| s.to_string());

                if let Some(ref t_name) = name {
                    state
                        .tool_stream
                        .set_identity(index, t_name, id.unwrap_or_default());
                }
                if let Some(ref t_args) = args {
                    if let Some(ev) = state.tool_stream.append(index, t_args) {
                        if !state.step_started {
                            events.push(LlmEvent::StepStart { index: 0 });
                            state.step_started = true;
                        }
                        events.push(ev);
                    }
                }
            }
        }
    }

    // Finish reason
    if let Some(finish_reason) = choice.get("finish_reason").and_then(|r| r.as_str()) {
        // Finish any pending tool calls
        for tool_ev in state.tool_stream.finish_all() {
            events.push(tool_ev);
        }

        let reason = map_finish_reason(finish_reason);

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
        events.push(LlmEvent::StepFinish {
            index: 0,
            reason: reason.clone(),
            usage: state.usage.clone(),
            provider_metadata: None,
        });
        events.push(LlmEvent::Finish {
            reason,
            usage: state.usage.clone(),
            provider_metadata: None,
        });
        state.finished = true;
    }

    events
}

/// SSE framing helper that wraps [`parse_sse_stream`] and drives the
/// [`ChatStreamState`] machine, yielding `Result<LlmEvent>` items.
///
/// This replaces ~50 lines of duplicated `futures::stream::unfold` in each
/// provider. The returned stream stops naturally when the SSE stream ends
/// or when `state.finished` is `true`.
pub fn create_chat_stream(
    response: reqwest::Response,
    provider_id: impl Into<String> + Clone + Send + 'static,
) -> Box<dyn futures::Stream<Item = Result<LlmEvent, Error>> + Send + Unpin> {
    let sse_stream = parse_sse_stream(response);
    let state = ChatStreamState::new();
    let provider_id = provider_id.into();

    let stream = futures::stream::unfold(
        (
            Box::pin(sse_stream)
                as Pin<
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
                    // Drain buffer
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
                                for ev in events_from_chat(&value, &mut state) {
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

// ═══════════════════════════════════════════════════════════════════════
// 3. Error Classification
// ═══════════════════════════════════════════════════════════════════════

/// Classify an HTTP error response from an OpenAI-compatible Chat Completions
/// API into a structured [`LlmErrorReason`].
///
/// Handles:
/// - 401/403 → Authentication
/// - 429 → RateLimit
/// - 400/413 → InvalidRequest (with context-overflow detection)
/// - 5xx → ProviderInternal
/// - Other → UnknownProvider
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

    #[test]
    fn test_extract_text_filters_non_text() {
        let content = MessageContent::Parts(vec![
            ContentPart::Text { text: "hello ".into() },
            ContentPart::Image { image: "base64data".into() },
            ContentPart::Text { text: "world".into() },
        ]);
        assert_eq!(extract_text(&content), "hello world");
    }

    // ── map_finish_reason ─────────────────────────────────────────

    #[test]
    fn test_map_finish_reason_stop() {
        assert_eq!(map_finish_reason("stop"), FinishReason::Stop);
    }

    #[test]
    fn test_map_finish_reason_length() {
        assert_eq!(map_finish_reason("length"), FinishReason::Length);
    }

    #[test]
    fn test_map_finish_reason_content_filter() {
        assert_eq!(
            map_finish_reason("content_filter"),
            FinishReason::ContentFilter
        );
    }

    #[test]
    fn test_map_finish_reason_tool_calls() {
        assert_eq!(map_finish_reason("tool_calls"), FinishReason::ToolCalls);
        assert_eq!(map_finish_reason("function_call"), FinishReason::ToolCalls);
    }

    #[test]
    fn test_map_finish_reason_unknown() {
        assert_eq!(
            map_finish_reason("some_unknown_reason"),
            FinishReason::Unknown
        );
    }

    // ── map_usage ─────────────────────────────────────────────────

    #[test]
    fn test_map_usage_basic() {
        let u = serde_json::json!({
            "prompt_tokens": 100,
            "completion_tokens": 50,
            "total_tokens": 150
        });
        let usage = map_usage(&u).unwrap();
        assert_eq!(usage.input_tokens, Some(100));
        assert_eq!(usage.output_tokens, Some(50));
        assert_eq!(usage.total_tokens, Some(150));
        assert_eq!(usage.reasoning_tokens, None);
        assert_eq!(usage.cache_read_input_tokens, None);
    }

    #[test]
    fn test_map_usage_with_cached_tokens() {
        let u = serde_json::json!({
            "prompt_tokens": 1000,
            "completion_tokens": 500,
            "total_tokens": 1500,
            "prompt_tokens_details": {"cached_tokens": 300}
        });
        let usage = map_usage(&u).unwrap();
        assert_eq!(usage.input_tokens, Some(1000));
        assert_eq!(usage.cache_read_input_tokens, Some(300));
        assert_eq!(usage.non_cached_input_tokens, Some(700));
    }

    #[test]
    fn test_map_usage_with_reasoning_tokens() {
        let u = serde_json::json!({
            "prompt_tokens": 500,
            "completion_tokens": 1000,
            "total_tokens": 1500,
            "completion_tokens_details": {"reasoning_tokens": 400}
        });
        let usage = map_usage(&u).unwrap();
        assert_eq!(usage.output_tokens, Some(1000));
        assert_eq!(usage.reasoning_tokens, Some(400));
    }

    #[test]
    fn test_map_usage_empty() {
        let u = serde_json::json!({
            "prompt_tokens": null,
            "completion_tokens": null,
            "total_tokens": null
        });
        let usage = map_usage(&u);
        // If prompt_tokens is null, should still return Some with None tokens
        assert!(usage.is_some());
        let usage = usage.unwrap();
        assert_eq!(usage.input_tokens, None);
        assert_eq!(usage.output_tokens, None);
        assert_eq!(usage.total_tokens, None);
    }

    #[test]
    fn test_map_usage_absent() {
        let u = serde_json::json!({});
        let usage = map_usage(&u);
        assert!(usage.is_some());
        let usage = usage.unwrap();
        assert_eq!(usage.input_tokens, None);
    }

    // ── events_from_chat (text delta) ─────────────────────────────

    #[test]
    fn test_events_from_chat_text_delta() {
        let event = serde_json::json!({
            "choices": [{
                "delta": {"content": "Hello"},
                "finish_reason": null
            }]
        });
        let mut state = ChatStreamState::new();
        let events = events_from_chat(&event, &mut state);
        assert!(!events.is_empty());
        assert!(events.iter().any(|e| matches!(e, LlmEvent::TextStart { .. })));
        assert!(events.iter().any(|e| matches!(e, LlmEvent::TextDelta { .. })));
        assert!(state.text_started);
    }

    // ── events_from_chat (reasoning delta) ────────────────────────

    #[test]
    fn test_events_from_chat_reasoning_delta() {
        let event = serde_json::json!({
            "choices": [{
                "delta": {"reasoning_content": "Let me think..."},
                "finish_reason": null
            }]
        });
        let mut state = ChatStreamState::new();
        let events = events_from_chat(&event, &mut state);
        assert!(!events.is_empty());
        assert!(events.iter().any(|e| matches!(e, LlmEvent::ReasoningStart { .. })));
        assert!(events.iter().any(|e| matches!(e, LlmEvent::ReasoningDelta { .. })));
        assert!(state.reasoning_started);
    }

    // ── events_from_chat (finish reason) ──────────────────────────

    #[test]
    fn test_events_from_chat_finish_stop() {
        let event = serde_json::json!({
            "choices": [{
                "delta": {"content": "Hello"},
                "finish_reason": "stop"
            }],
            "usage": {"prompt_tokens": 10, "completion_tokens": 5, "total_tokens": 15}
        });
        let mut state = ChatStreamState::new();
        let events = events_from_chat(&event, &mut state);
        assert!(events.iter().any(|e| matches!(e, LlmEvent::Finish { reason, .. } if *reason == FinishReason::Stop)));
        assert!(state.finished);
        assert!(state.usage.is_some());
    }

    #[test]
    fn test_events_from_chat_finish_tool_calls() {
        let event = serde_json::json!({
            "choices": [{
                "delta": null,
                "finish_reason": "tool_calls"
            }]
        });
        let mut state = ChatStreamState::new();
        let events = events_from_chat(&event, &mut state);
        assert!(events.iter().any(|e| matches!(e, LlmEvent::Finish { reason, .. } if *reason == FinishReason::ToolCalls)));
        assert!(state.finished);
    }

    // ── events_from_chat (tool calls) ─────────────────────────────

    #[test]
    fn test_events_from_chat_tool_call_delta() {
        let event = serde_json::json!({
            "choices": [{
                "delta": {
                    "tool_calls": [{
                        "index": 0,
                        "id": "call_1",
                        "function": {"name": "bash", "arguments": "{\"cmd\":"}
                    }]
                },
                "finish_reason": null
            }]
        });
        let mut state = ChatStreamState::new();
        let events = events_from_chat(&event, &mut state);
        assert!(events.iter().any(|e| matches!(e, LlmEvent::ToolInputDelta { .. })));
        assert!(state.step_started);
    }

    #[test]
    fn test_events_from_chat_tool_call_completion() {
        let mut state = ChatStreamState::new();

        // First delta: identity + partial args
        let event1 = serde_json::json!({
            "choices": [{
                "delta": {
                    "tool_calls": [{
                        "index": 0,
                        "id": "call_1",
                        "function": {"name": "bash", "arguments": "{\"cmd\":"}
                    }]
                },
                "finish_reason": null
            }]
        });
        let _ = events_from_chat(&event1, &mut state);

        // Second delta: more args
        let event2 = serde_json::json!({
            "choices": [{
                "delta": {
                    "tool_calls": [{
                        "index": 0,
                        "function": {"arguments": "\"ls\"}"}
                    }]
                },
                "finish_reason": null
            }]
        });
        let _ = events_from_chat(&event2, &mut state);

        // Finish
        let event3 = serde_json::json!({
            "choices": [{
                "delta": null,
                "finish_reason": "tool_calls"
            }]
        });
        let events = events_from_chat(&event3, &mut state);

        // Should include ToolCall event with parsed input
        assert!(events.iter().any(|e| matches!(e, LlmEvent::ToolCall { .. })));
        assert!(events.iter().any(|e| matches!(e, LlmEvent::Finish { reason, .. } if *reason == FinishReason::ToolCalls)));
    }

    // ── lower_messages ────────────────────────────────────────────

    fn make_text_part(text: &str) -> ContentPart {
        ContentPart::Text { text: text.into() }
    }

    fn make_tool_call_part(id: &str, name: &str, args: &str) -> ContentPart {
        ContentPart::ToolCallPart {
            tool_call_id: id.into(),
            tool_name: name.into(),
            arguments: args.into(),
        }
    }

    #[test]
    fn test_lower_messages_system() {
        let msgs = vec![ChatMessage::System {
            content: MessageContent::Text("You are helpful.".into()),
        }];
        let result = lower_messages(&msgs);
        assert_eq!(result.len(), 1);
        assert_eq!(result[0]["role"], "system");
        assert_eq!(result[0]["content"], "You are helpful.");
    }

    #[test]
    fn test_lower_messages_user_text() {
        let msgs = vec![ChatMessage::User {
            content: MessageContent::Text("Hello".into()),
        }];
        let result = lower_messages(&msgs);
        assert_eq!(result.len(), 1);
        assert_eq!(result[0]["role"], "user");
        assert_eq!(result[0]["content"], "Hello");
    }

    #[test]
    fn test_lower_messages_assistant_text() {
        let msgs = vec![ChatMessage::Assistant {
            content: MessageContent::Text("Hi there!".into()),
        }];
        let result = lower_messages(&msgs);
        assert_eq!(result.len(), 1);
        assert_eq!(result[0]["role"], "assistant");
        assert_eq!(result[0]["content"], "Hi there!");
    }

    #[test]
    fn test_lower_messages_assistant_with_tool_calls() {
        let msgs = vec![ChatMessage::Assistant {
            content: MessageContent::Parts(vec![
                make_text_part("I'll look that up."),
                make_tool_call_part("tc_01", "bash", r#"{"cmd":"ls"}"#),
            ]),
        }];
        let result = lower_messages(&msgs);
        assert_eq!(result.len(), 1);
        assert_eq!(result[0]["role"], "assistant");
        assert_eq!(result[0]["content"], "I'll look that up.");
        assert_eq!(result[0]["tool_calls"][0]["id"], "tc_01");
        assert_eq!(result[0]["tool_calls"][0]["function"]["name"], "bash");
    }

    #[test]
    fn test_lower_messages_mixed() {
        let msgs = vec![
            ChatMessage::System {
                content: MessageContent::Text("System prompt".into()),
            },
            ChatMessage::User {
                content: MessageContent::Text("User query".into()),
            },
            ChatMessage::Assistant {
                content: MessageContent::Text("Assistant reply".into()),
            },
        ];
        let result = lower_messages(&msgs);
        assert_eq!(result.len(), 3);
    }

    // ── lower_tools ───────────────────────────────────────────────

    #[test]
    fn test_lower_tools_empty() {
        let tools: Vec<ToolDefinition> = vec![];
        assert!(lower_tools(&tools).is_empty());
    }

    #[test]
    fn test_lower_tools_single() {
        let tools = vec![ToolDefinition {
            name: "bash".into(),
            description: "Run a shell command".into(),
            parameters: serde_json::json!({"type": "object", "properties": {}}),
        }];
        let result = lower_tools(&tools);
        assert_eq!(result.len(), 1);
        assert_eq!(result[0]["type"], "function");
        assert_eq!(result[0]["function"]["name"], "bash");
    }

    // ── build_chat_body ───────────────────────────────────────────

    #[test]
    fn test_build_chat_body_basic() {
        let model = Model {
            id: "gpt-4".into(),
            provider_id: "openai".into(),
            name: "GPT-4".into(),
            api: crate::provider::ApiInfo {
                id: "gpt-4".into(),
                url: "https://api.openai.com/v1".into(),
                ..Default::default()
            },
            ..Default::default()
        };
        let msgs = vec![ChatMessage::User {
            content: MessageContent::Text("Hello".into()),
        }];
        let body = build_chat_body(&model, &msgs, &[], BodyOptions::default());
        assert_eq!(body["model"], "gpt-4");
        assert_eq!(body["stream"], true);
        assert_eq!(body["messages"][0]["role"], "user");
    }

    #[test]
    fn test_build_chat_body_with_extra_fields() {
        let model = Model {
            id: "grok-3".into(),
            provider_id: "xai".into(),
            name: "Grok 3".into(),
            api: crate::provider::ApiInfo {
                id: "grok-3".into(),
                url: "https://api.x.ai/v1".into(),
                ..Default::default()
            },
            ..Default::default()
        };
        let mut extra = serde_json::Map::new();
        extra.insert("reasoning_effort".into(), serde_json::json!("medium"));
        let body = build_chat_body(
            &model,
            &[],
            &[],
            BodyOptions {
                extra_fields: Some(extra),
                ..BodyOptions::default()
            },
        );
        assert_eq!(body["reasoning_effort"], "medium");
    }

    // ── Error classification ──────────────────────────────────────

    #[test]
    fn test_classify_error_auth_401() {
        let reason = classify_error(401, r#"{"error":{"message":"Invalid API key"}}"#);
        assert!(matches!(
            reason,
            LlmErrorReason::Authentication {
                kind: crate::error::AuthErrorKind::Invalid,
                ..
            }
        ));
    }

    #[test]
    fn test_classify_error_rate_limit_429() {
        let reason = classify_error(429, "Too many requests");
        assert!(matches!(reason, LlmErrorReason::RateLimit { .. }));
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
    fn test_classify_error_provider_internal_500() {
        let reason = classify_error(500, "Internal server error");
        assert!(matches!(
            reason,
            LlmErrorReason::ProviderInternal { status: 500, .. }
        ));
    }

    #[test]
    fn test_classify_error_unknown() {
        let reason = classify_error(418, "I'm a teapot");
        assert!(matches!(
            reason,
            LlmErrorReason::UnknownProvider {
                status: Some(418),
                ..
            }
        ));
    }

    #[test]
    fn test_classify_error_retryable() {
        assert!(classify_error(429, "rate limit").is_retryable());
        assert!(classify_error(503, "overloaded").is_retryable());
        assert!(!classify_error(400, "bad request").is_retryable());
        assert!(!classify_error(401, "unauthorized").is_retryable());
    }

    #[test]
    fn test_chat_stream_state_default() {
        let state = ChatStreamState::new();
        assert!(!state.text_started);
        assert!(!state.reasoning_started);
        assert!(!state.step_started);
        assert!(!state.finished);
        assert!(state.usage.is_none());
        assert!(!state.tool_stream.has_pending());
    }

    #[test]
    fn test_events_from_chat_no_choices() {
        let event = serde_json::json!({});
        let mut state = ChatStreamState::new();
        let events = events_from_chat(&event, &mut state);
        assert!(events.is_empty());
    }

    #[test]
    fn test_events_from_chat_empty_delta() {
        let event = serde_json::json!({
            "choices": [{"delta": {}, "finish_reason": null}]
        });
        let mut state = ChatStreamState::new();
        let events = events_from_chat(&event, &mut state);
        assert!(events.is_empty());
    }

    #[test]
    fn test_events_from_chat_reasoning_finish_without_text() {
        let event = serde_json::json!({
            "choices": [{
                "delta": {"reasoning_content": "thinking..."},
                "finish_reason": null
            }]
        });
        let mut state = ChatStreamState::new();
        let _ = events_from_chat(&event, &mut state);
        assert!(state.reasoning_started);

        let finish = serde_json::json!({
            "choices": [{"delta": null, "finish_reason": "stop"}]
        });
        let events = events_from_chat(&finish, &mut state);
        assert!(events.iter().any(|e| matches!(e, LlmEvent::ReasoningEnd { .. })));
        assert!(!events.iter().any(|e| matches!(e, LlmEvent::TextEnd { .. })));
    }

    #[test]
    fn test_events_from_chat_text_and_reasoning_together() {
        let event = serde_json::json!({
            "choices": [{
                "delta": {
                    "reasoning_content": "thinking...",
                    "content": "Hello"
                },
                "finish_reason": null
            }]
        });
        let mut state = ChatStreamState::new();
        let events = events_from_chat(&event, &mut state);
        assert!(events.iter().any(|e| matches!(e, LlmEvent::ReasoningStart { .. })));
        assert!(events.iter().any(|e| matches!(e, LlmEvent::TextStart { .. })));
    }

    #[test]
    fn test_events_from_chat_multiple_tool_deltas() {
        let mut state = ChatStreamState::new();

        let d1 = serde_json::json!({
            "choices": [{
                "delta": {
                    "tool_calls": [
                        {"index": 0, "id": "c1", "function": {"name": "bash", "arguments": "{\"cmd\":"}},
                        {"index": 1, "id": "c2", "function": {"name": "read", "arguments": "{\"path\":"}}
                    ]
                },
                "finish_reason": null
            }]
        });
        let _ = events_from_chat(&d1, &mut state);
        assert_eq!(state.tool_stream.pending_count(), 2);

        let d2 = serde_json::json!({
            "choices": [{
                "delta": {
                    "tool_calls": [
                        {"index": 0, "function": {"arguments": "\"ls\"}"}},
                        {"index": 1, "function": {"arguments": "\"/tmp\"}"}}
                    ]
                },
                "finish_reason": "tool_calls"
            }]
        });
        let events = events_from_chat(&d2, &mut state);
        let tool_calls: Vec<_> = events.iter().filter(|e| matches!(e, LlmEvent::ToolCall { .. })).collect();
        assert_eq!(tool_calls.len(), 2);
    }
}
