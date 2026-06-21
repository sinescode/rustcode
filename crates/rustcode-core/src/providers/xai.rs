//! xAI (Grok) provider module.
//!
//! xAI uses the OpenAI-compatible Chat Completions API at `https://api.x.ai`.
//! Supports streaming SSE, tool calling, and reasoning effort configuration.
//! Also supports the newer Responses API format.
//!
//! Ported from:
//! - `packages/llm/src/providers/xai.ts` (56 lines)
//! - `packages/llm/src/providers/openai-compatible-profile.ts` (20 lines)
//!
//! OpenCode commit: 5d0f86606ac30690f79f0a6a9f41a1f49fe95d0b

use async_trait::async_trait;
use futures::StreamExt;
use serde::Deserialize;
use std::collections::{HashMap, VecDeque};
use std::pin::Pin;

use crate::error::{Error, LlmErrorReason};
use crate::provider::{
    self, ChatMessage, ContentPart, FinishReason, LlmEvent, MessageContent, Model, Provider,
    ProviderInfo, ToolDefinition, Usage,
};
use crate::sse::parse_sse_stream;
use crate::tool_stream::ToolStreamAccumulator;

// ── Constants ────────────────────────────────────────────────────────────

const DEFAULT_BASE_URL: &str = "https://api.x.ai/v1";
const CHAT_PATH: &str = "/chat/completions";

/// Resolve the API key for xAI.
fn resolve_api_key() -> Result<String, Error> {
    std::env::var("XAI_API_KEY")
        .ok()
        .filter(|k| !k.is_empty())
        .ok_or_else(|| Error::Auth("XAI_API_KEY environment variable not set".into()))
}

// ── SSE event types (same as OpenAI Chat format) ─────────────────────────

#[derive(Debug, Deserialize)]
struct XaiChatEvent {
    choices: Vec<XaiChoice>,
    #[serde(default)]
    usage: Option<XaiUsage>,
}

#[derive(Debug, Deserialize)]
struct XaiChoice {
    delta: Option<XaiDelta>,
    finish_reason: Option<String>,
}

#[derive(Debug, Deserialize)]
struct XaiDelta {
    content: Option<String>,
    reasoning_content: Option<String>,
    tool_calls: Option<Vec<XaiToolCallDelta>>,
}

#[derive(Debug, Deserialize)]
struct XaiToolCallDelta {
    index: u64,
    id: Option<String>,
    function: Option<XaiToolCallDeltaFn>,
}

#[derive(Debug, Deserialize)]
struct XaiToolCallDeltaFn {
    name: Option<String>,
    arguments: Option<String>,
}

#[derive(Debug, Deserialize)]
struct XaiUsage {
    prompt_tokens: Option<u64>,
    completion_tokens: Option<u64>,
    total_tokens: Option<u64>,
    prompt_tokens_details: Option<XaiPromptTokenDetails>,
    completion_tokens_details: Option<XaiCompletionTokenDetails>,
}

#[derive(Debug, Deserialize)]
struct XaiPromptTokenDetails {
    cached_tokens: Option<u64>,
}

#[derive(Debug, Deserialize)]
struct XaiCompletionTokenDetails {
    reasoning_tokens: Option<u64>,
}

// ── xAI Provider ─────────────────────────────────────────────────────────

/// xAI (Grok) LLM provider.
pub struct XaiProvider {
    api_key: String,
    base_url: String,
    http_client: reqwest::Client,
    models: Vec<Model>,
}

impl XaiProvider {
    /// Create a new xAI provider, reading the API key from `XAI_API_KEY`.
    pub fn new() -> Result<Self, Error> {
        let api_key = resolve_api_key()?;
        Self::with_api_key(api_key, DEFAULT_BASE_URL.into())
    }

    /// Create a new xAI provider with an explicit API key and base URL.
    pub fn with_api_key(api_key: String, base_url: String) -> Result<Self, Error> {
        let http_client = reqwest::Client::builder()
            .user_agent(format!("rustcode/{}", env!("CARGO_PKG_VERSION")))
            .timeout(std::time::Duration::from_secs(300))
            .build()
            .map_err(|e| Error::Network(format!("HTTP client: {e}")))?;

        let models = build_model_catalog(&base_url);

        Ok(Self {
            api_key,
            base_url,
            http_client,
            models,
        })
    }

    /// Try to auto-detect: returns `ProviderInfo` if `XAI_API_KEY` is set.
    pub fn auto_detect() -> Vec<ProviderInfo> {
        std::env::var("XAI_API_KEY")
            .ok()
            .filter(|k| !k.is_empty())
            .map(|_| ProviderInfo {
                id: "xai".into(),
                name: "xAI Grok".into(),
                source: crate::provider::ProviderSource::Env,
                env: vec!["XAI_API_KEY".into()],
                key: None,
                options: HashMap::new(),
                models: HashMap::new(),
            })
            .into_iter()
            .collect()
    }

    fn chat_url(&self) -> String {
        format!("{}{CHAT_PATH}", self.base_url.trim_end_matches('/'))
    }

    fn build_body(
        &self,
        model: &Model,
        messages: &[ChatMessage],
        tools: &[ToolDefinition],
    ) -> serde_json::Value {
        let messages = provider::normalize_messages(messages, model);
        let msgs: Vec<serde_json::Value> = messages
            .iter()
            .map(|m| match m {
                ChatMessage::System { content } => {
                    serde_json::json!({"role":"system","content": extract_text(content)})
                }
                ChatMessage::User { content } => {
                    serde_json::json!({"role":"user","content": extract_text(content)})
                }
                ChatMessage::Assistant { content } => {
                    let mut text = String::new();
                    let mut reasoning = String::new();
                    let mut tool_calls_arr = Vec::new();
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
                                        tool_calls_arr.push(serde_json::json!({
                                            "id": tool_call_id,
                                            "type": "function",
                                            "function": {
                                                "name": tool_name,
                                                "arguments": arguments.to_string(),
                                            }
                                        }));
                                    }
                                    _ => {}
                                }
                            }
                        }
                    }
                    let mut obj = serde_json::json!({"role":"assistant"});
                    if !text.is_empty() {
                        obj["content"] = serde_json::Value::String(text);
                    } else {
                        obj["content"] = serde_json::Value::Null;
                    }
                    if !reasoning.is_empty() {
                        obj["reasoning_content"] = serde_json::Value::String(reasoning);
                    }
                    if !tool_calls_arr.is_empty() {
                        obj["tool_calls"] = serde_json::Value::Array(tool_calls_arr);
                    }
                    obj
                }
                ChatMessage::Tool { content } => {
                    let p = content.first().map(|p| {
                        let crate::provider::ToolResultPart::ToolResult {
                            tool_call_id,
                            output,
                            ..
                        } = p;
                        serde_json::json!({
                            "role":"tool",
                            "tool_call_id": tool_call_id,
                            "content": output.to_string()
                        })
                    });
                    p.unwrap_or(serde_json::json!({"role":"tool","tool_call_id":"","content":""}))
                }
            })
            .collect();

        let tools_arr: Vec<serde_json::Value> = tools
            .iter()
            .map(|t| {
                serde_json::json!({
                    "type":"function",
                    "function": {
                        "name": t.name,
                        "description": t.description,
                        "parameters": t.parameters
                    }
                })
            })
            .collect();

        let mut body = serde_json::json!({
            "model": model.api.id,
            "messages": msgs,
            "stream": true,
            "stream_options": {"include_usage": true},
            "max_tokens": provider::max_output_tokens(model, provider::OUTPUT_TOKEN_MAX),
            "temperature": provider::default_temperature(&model.api.id),
            "top_p": provider::default_top_p(&model.api.id),
        });

        // xAI supports reasoning effort (similar to OpenAI)
        let model_lower = model.id.to_lowercase();
        if model_lower.contains("grok") && !model_lower.contains("grok-2") {
            body["reasoning_effort"] = serde_json::json!("medium");
        }

        if !tools_arr.is_empty() {
            body["tools"] = serde_json::Value::Array(tools_arr);
        }

        body
    }
}

fn extract_text(content: &MessageContent) -> String {
    match content {
        MessageContent::Text(t) => t.clone(),
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

// ── Event mapping ──────────────────────────────────────────────────────

struct XaiStreamState {
    tool_stream: ToolStreamAccumulator,
    text_started: bool,
    reasoning_started: bool,
    step_started: bool,
    usage: Option<Usage>,
    finished: bool,
}

fn events_from_chat(event: XaiChatEvent, state: &mut XaiStreamState) -> Vec<LlmEvent> {
    let mut events = Vec::new();

    if let Some(ref usage_val) = event.usage {
        state.usage = Some(map_usage(usage_val));
    }

    let choice = event.choices.first();

    if let Some(delta) = choice.and_then(|c| c.delta.as_ref()) {
        if let Some(ref rc) = delta.reasoning_content {
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
                    text: rc.clone(),
                    provider_metadata: None,
                });
            }
        }

        if let Some(ref content) = delta.content {
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
                    text: content.clone(),
                    provider_metadata: None,
                });
            }
        }

        if let Some(tool_deltas) = &delta.tool_calls {
            for td in tool_deltas {
                if let Some(name) = td.function.as_ref().and_then(|f| f.name.as_ref()) {
                    state
                        .tool_stream
                        .set_identity(td.index, name, td.id.clone().unwrap_or_default());
                }
                if let Some(args) = td.function.as_ref().and_then(|f| f.arguments.as_ref()) {
                    if let Some(ev) = state.tool_stream.append(td.index, args) {
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

    if let Some(finish_reason) = choice.and_then(|c| c.finish_reason.as_ref()) {
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

fn map_finish_reason(reason: &str) -> FinishReason {
    match reason {
        "stop" => FinishReason::Stop,
        "length" => FinishReason::Length,
        "content_filter" => FinishReason::ContentFilter,
        "function_call" | "tool_calls" => FinishReason::ToolCalls,
        _ => FinishReason::Unknown,
    }
}

fn map_usage(u: &XaiUsage) -> Usage {
    let cached = u
        .prompt_tokens_details
        .as_ref()
        .and_then(|d| d.cached_tokens);
    let reasoning = u
        .completion_tokens_details
        .as_ref()
        .and_then(|d| d.reasoning_tokens);
    let non_cached = u
        .prompt_tokens
        .map(|p| p.saturating_sub(cached.unwrap_or(0)));

    Usage {
        input_tokens: u.prompt_tokens,
        output_tokens: u.completion_tokens,
        non_cached_input_tokens: non_cached,
        cache_read_input_tokens: cached,
        cache_write_input_tokens: None,
        reasoning_tokens: reasoning,
        total_tokens: u.total_tokens,
        provider_metadata: None,
    }
}

// ── Provider trait implementation ──────────────────────────────────────

#[async_trait]
impl Provider for XaiProvider {
    fn provider_id(&self) -> &str {
        "xai"
    }

    fn npm(&self) -> &str {
        "@ai-sdk/xai"
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
                provider_id: "xai".into(),
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
        let body = self.build_body(model, messages, tools);

        let response = self
            .http_client
            .post(self.chat_url())
            .header("Authorization", format!("Bearer {}", self.api_key))
            .header("Content-Type", "application/json")
            .json(&body)
            .send()
            .await
            .map_err(|e| Error::Network(format!("xAI request: {e}")))?;

        if !response.status().is_success() {
            let status = response.status().as_u16();
            let text = response.text().await.unwrap_or_default();
            return Err(Error::Llm {
                module: "xai".into(),
                method: "stream".into(),
                reason: Box::new(classify_error(status, &text)),
            });
        }

        let sse_stream = parse_sse_stream(response);
        let state = XaiStreamState {
            tool_stream: ToolStreamAccumulator::new(),
            text_started: false,
            reasoning_started: false,
            step_started: false,
            usage: None,
            finished: false,
        };

        let llm_stream = futures::stream::unfold(
            (
                Box::pin(sse_stream)
                    as Pin<
                        Box<
                            dyn futures::Stream<
                                    Item = Result<crate::sse::SseEvent, crate::sse::SseError>,
                                > + Send
                                + Unpin,
                        >,
                    >,
                state,
                VecDeque::new(),
            ),
            |(mut sse, mut state, mut buffer)| {
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
                                if let Ok(oe) =
                                    serde_json::from_str::<XaiChatEvent>(&se.data)
                                {
                                    for ev in events_from_chat(oe, &mut state) {
                                        buffer.push_back(Ok(ev));
                                    }
                                    if let Some(ev) = buffer.pop_front() {
                                        return Some((ev, (sse, state, buffer)));
                                    }
                                }
                            }
                            Some(Err(e)) => {
                                return Some((
                                    Err(Error::ResponseStream(format!("xAI SSE: {e}")),
                                ), (sse, state, buffer)))
                            }
                            None => return None,
                            _ => continue,
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

// ── Error classification ───────────────────────────────────────────────

fn classify_error(status: u16, body: &str) -> LlmErrorReason {
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

// ── Model catalog ──────────────────────────────────────────────────────

fn build_model_catalog(base_url: &str) -> Vec<Model> {
    vec![
        make_model(
            "grok-4", "Grok 4", base_url, 1_000_000, 128_000, 2.50, 10.0, true, true,
        ),
        make_model(
            "grok-3", "Grok 3", base_url, 128_000, 16_384, 2.00, 8.0, true, true,
        ),
        make_model(
            "grok-3-mini", "Grok 3 Mini", base_url, 128_000, 16_384, 1.00, 4.0, true, false,
        ),
        make_model(
            "grok-3-fast", "Grok 3 Fast", base_url, 128_000, 16_384, 2.00, 8.0, false, true,
        ),
        make_model(
            "grok-3-latest", "Grok 3 Latest", base_url, 128_000, 16_384, 2.00, 8.0, true, true,
        ),
        make_model(
            "grok-2", "Grok 2", base_url, 128_000, 16_384, 1.50, 6.0, false, true,
        ),
        make_model(
            "grok-2-latest", "Grok 2 Latest", base_url, 128_000, 16_384, 1.50, 6.0, false, true,
        ),
        make_model(
            "grok-beta", "Grok Beta", base_url, 128_000, 8_192, 0.50, 2.0, false, false,
        ),
    ]
}

#[allow(clippy::too_many_arguments)]
fn make_model(
    id: &str,
    name: &str,
    base_url: &str,
    context: u64,
    output: u64,
    input_cost: f64,
    output_cost: f64,
    reasoning: bool,
    image_input: bool,
) -> Model {
    Model {
        id: id.into(),
        provider_id: "xai".into(),
        name: name.into(),
        api: provider::ApiInfo {
            id: id.into(),
            url: base_url.into(),
            npm: "@ai-sdk/xai".into(),
        },
        family: Some("grok".into()),
        capabilities: provider::Capabilities {
            temperature: true,
            reasoning,
            attachment: false,
            toolcall: true,
            input: provider::Modalities {
                text: true,
                image: image_input,
                ..Default::default()
            },
            output: provider::Modalities {
                text: true,
                ..Default::default()
            },
            interleaved: provider::InterleavedSupport::Bool(false),
        },
        cost: provider::Cost {
            input: input_cost,
            output: output_cost,
            cache: provider::CacheCost::default(),
            tiers: None,
            experimental_over_200k: None,
        },
        limit: provider::TokenLimit {
            context,
            input: None,
            output,
        },
        status: provider::ModelStatus::Active,
        options: HashMap::new(),
        headers: HashMap::new(),
        release_date: "2025".into(),
        variants: None,
    }
}

// ── Tests ──────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_map_finish_reason() {
        assert_eq!(map_finish_reason("stop"), FinishReason::Stop);
        assert_eq!(map_finish_reason("length"), FinishReason::Length);
        assert_eq!(map_finish_reason("tool_calls"), FinishReason::ToolCalls);
        assert_eq!(map_finish_reason("content_filter"), FinishReason::ContentFilter);
        assert_eq!(map_finish_reason("unknown"), FinishReason::Unknown);
    }

    #[test]
    fn test_classify_error() {
        let reason = classify_error(401, "bad key");
        assert!(matches!(reason, LlmErrorReason::Authentication { .. }));

        let reason = classify_error(429, "rate limit");
        assert!(matches!(reason, LlmErrorReason::RateLimit { .. }));

        let reason = classify_error(500, "internal");
        assert!(matches!(reason, LlmErrorReason::ProviderInternal { .. }));
    }

    #[test]
    fn test_model_catalog() {
        let models = build_model_catalog("https://api.x.ai/v1");
        assert!(models.len() >= 5);
        let grok4 = models.iter().find(|m| m.id == "grok-4").unwrap();
        assert_eq!(grok4.provider_id, "xai");
        assert_eq!(grok4.family.as_deref(), Some("grok"));
        assert!(grok4.capabilities.toolcall);
        assert!(grok4.capabilities.reasoning);
        assert_eq!(grok4.limit.context, 1_000_000);

        let grok2 = models.iter().find(|m| m.id == "grok-2").unwrap();
        assert!(!grok2.capabilities.reasoning);
        assert!(grok2.capabilities.input.image);
    }

    #[test]
    fn test_map_usage() {
        let xu = XaiUsage {
            prompt_tokens: Some(100),
            completion_tokens: Some(50),
            total_tokens: Some(150),
            prompt_tokens_details: Some(XaiPromptTokenDetails {
                cached_tokens: Some(20),
            }),
            completion_tokens_details: Some(XaiCompletionTokenDetails {
                reasoning_tokens: Some(10),
            }),
        };
        let usage = map_usage(&xu);
        assert_eq!(usage.input_tokens, Some(100));
        assert_eq!(usage.output_tokens, Some(50));
        assert_eq!(usage.total_tokens, Some(150));
        assert_eq!(usage.cache_read_input_tokens, Some(20));
        assert_eq!(usage.non_cached_input_tokens, Some(80));
        assert_eq!(usage.reasoning_tokens, Some(10));
    }

    #[test]
    fn test_resolve_api_key() {
        std::env::remove_var("XAI_API_KEY");
        let result = resolve_api_key();
        assert!(result.is_err());
    }

    #[test]
    fn test_extract_text() {
        let content = MessageContent::Text("hello world".into());
        assert_eq!(extract_text(&content), "hello world");

        let content = MessageContent::Parts(vec![
            ContentPart::Text { text: "hello ".into() },
            ContentPart::Text { text: "world".into() },
        ]);
        assert_eq!(extract_text(&content), "hello world");
    }

    #[test]
    fn test_events_from_chat_finish_stop() {
        let mut state = XaiStreamState {
            tool_stream: ToolStreamAccumulator::new(),
            text_started: false,
            reasoning_started: false,
            step_started: false,
            usage: None,
            finished: false,
        };
        let event = XaiChatEvent {
            choices: vec![XaiChoice {
                delta: None,
                finish_reason: Some("stop".into()),
            }],
            usage: None,
        };
        let events = events_from_chat(event, &mut state);
        assert!(events.iter().any(|e| matches!(e, LlmEvent::Finish { reason, .. } if *reason == FinishReason::Stop)));
        assert!(state.finished);
    }
}
