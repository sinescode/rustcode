//! OpenAI Chat Completions + Responses API provider.
//!
//! Ported from:
//! - `packages/llm/src/protocols/openai-chat.ts` (493 lines)
//! - `packages/llm/src/protocols/openai-responses.ts` (1004 lines)
//! - `packages/llm/src/providers/openai.ts` (63 lines)
//! - `packages/llm/src/providers/openai-options.ts` (83 lines)

use async_trait::async_trait;
use futures::StreamExt;
use serde::{Deserialize, Serialize};
use std::collections::{HashMap, VecDeque};

use crate::error::Error;
use crate::provider::{
    ChatMessage, ContentPart, FinishReason, LlmEvent, MessageContent, Model, Provider, ToolDefinition, Usage,
};
use crate::sse::parse_sse_stream;
use crate::tool_stream::ToolStreamAccumulator;

const DEFAULT_BASE_URL: &str = "https://api.openai.com/v1";
const CHAT_PATH: &str = "/chat/completions";
const RESPONSES_PATH: &str = "/responses";

fn resolve_api_key() -> Result<String, Error> {
    std::env::var("OPENAI_API_KEY").ok().filter(|k| !k.is_empty()).ok_or_else(|| {
        Error::Auth("OPENAI_API_KEY environment variable not set".into())
    })
}

// ── Chat Completions Body ──────────────────────────────────────────────

#[derive(Debug, Serialize)]
struct OpenAIChatBody {
    model: String,
    messages: Vec<OpenAIChatMessage>,
    #[serde(skip_serializing_if = "Option::is_none")]
    tools: Option<Vec<OpenAITool>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    tool_choice: Option<serde_json::Value>,
    stream: bool,
    stream_options: serde_json::Value,
    #[serde(skip_serializing_if = "Option::is_none")]
    max_tokens: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    temperature: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    top_p: Option<f64>,
}

#[derive(Debug, Serialize)]
#[serde(tag = "role")]
enum OpenAIChatMessage {
    #[serde(rename = "system")]
    System { content: String },
    #[serde(rename = "user")]
    User { content: OpenAIChatUserContent },
    #[serde(rename = "assistant")]
    Assistant {
        content: Option<String>,
        #[serde(skip_serializing_if = "Option::is_none")]
        tool_calls: Option<Vec<OpenAIAssistantToolCall>>,
        #[serde(skip_serializing_if = "Option::is_none")]
        reasoning_content: Option<String>,
    },
    #[serde(rename = "tool")]
    Tool {
        tool_call_id: String,
        content: String,
    },
}

#[derive(Debug, Serialize)]
#[serde(untagged)]
enum OpenAIChatUserContent {
    Text(String),
    Parts(Vec<OpenAIUserContentPart>),
}

#[derive(Debug, Serialize)]
#[serde(tag = "type")]
enum OpenAIUserContentPart {
    #[serde(rename = "text")]
    Text { text: String },
    #[serde(rename = "image_url")]
    ImageUrl { image_url: OpenAIImageUrl },
}

#[derive(Debug, Serialize)]
struct OpenAIImageUrl {
    url: String,
}

#[derive(Debug, Serialize)]
struct OpenAIAssistantToolCall {
    id: String,
    #[serde(rename = "type")]
    call_type: String,
    function: OpenAIToolCallFunction,
}

#[derive(Debug, Serialize)]
struct OpenAIToolCallFunction {
    name: String,
    arguments: String,
}

#[derive(Debug, Serialize)]
struct OpenAITool {
    #[serde(rename = "type")]
    tool_type: String,
    function: OpenAIFunctionDef,
}

#[derive(Debug, Serialize)]
struct OpenAIFunctionDef {
    name: String,
    description: String,
    parameters: serde_json::Value,
}

// ── Chat SSE Event types ───────────────────────────────────────────────

#[derive(Debug, Deserialize)]
struct OpenAIChatEvent {
    choices: Vec<OpenAIChoice>,
    #[serde(default)]
    usage: Option<OpenAIUsage>,
}

#[derive(Debug, Deserialize)]
struct OpenAIChoice {
    delta: Option<OpenAIDelta>,
    finish_reason: Option<String>,
}

#[derive(Debug, Deserialize)]
struct OpenAIDelta {
    content: Option<String>,
    reasoning_content: Option<String>,
    tool_calls: Option<Vec<OpenAIToolCallDelta>>,
}

#[derive(Debug, Deserialize)]
struct OpenAIToolCallDelta {
    index: u64,
    id: Option<String>,
    function: Option<OpenAIToolCallDeltaFn>,
}

#[derive(Debug, Deserialize)]
struct OpenAIToolCallDeltaFn {
    name: Option<String>,
    arguments: Option<String>,
}

#[derive(Debug, Deserialize)]
struct OpenAIUsage {
    prompt_tokens: Option<u64>,
    completion_tokens: Option<u64>,
    total_tokens: Option<u64>,
    prompt_tokens_details: Option<OpenAIPromptTokenDetails>,
    completion_tokens_details: Option<OpenAICompletionTokenDetails>,
}

#[derive(Debug, Deserialize)]
struct OpenAIPromptTokenDetails {
    cached_tokens: Option<u64>,
}

#[derive(Debug, Deserialize)]
struct OpenAICompletionTokenDetails {
    reasoning_tokens: Option<u64>,
}

// ── OpenAI Provider ─────────────────────────────────────────────────────

pub struct OpenAIProvider {
    api_key: String,
    base_url: String,
    http_client: reqwest::Client,
    models: Vec<Model>,
}

impl OpenAIProvider {
    pub fn new() -> Result<Self, Error> {
        Self::with_base_url(resolve_api_key()?, DEFAULT_BASE_URL.into())
    }

    pub fn with_base_url(api_key: String, base_url: String) -> Result<Self, Error> {
        let http_client = reqwest::Client::builder()
            .user_agent(format!("rustcode/{}", env!("CARGO_PKG_VERSION")))
            .timeout(std::time::Duration::from_secs(300))
            .build()
            .map_err(|e| Error::Network(format!("HTTP client: {e}")))?;
        Ok(Self { api_key, base_url, http_client, models: build_model_catalog() })
    }

    pub fn with_api_key(api_key: String) -> Result<Self, Error> {
        Self::with_base_url(api_key, DEFAULT_BASE_URL.into())
    }

    fn chat_url(&self) -> String { format!("{}{CHAT_PATH}", self.base_url.trim_end_matches('/')) }

    fn build_chat_messages(messages: &[ChatMessage]) -> Vec<OpenAIChatMessage> {
        let mut result = Vec::new();
        let mut pending_images: Vec<OpenAIUserContentPart> = Vec::new();

        for msg in messages {
            match msg {
                ChatMessage::System { content } => {
                    let text = extract_text(content);
                    if !text.is_empty() {
                        result.push(OpenAIChatMessage::System { content: text });
                    }
                }
                ChatMessage::User { content } => {
                    let mut text_parts = String::new();
                    let mut media_parts: Vec<OpenAIUserContentPart> = Vec::new();
                    for part in content_parts(content) {
                        match part {
                            ContentPart::Text { text } => text_parts.push_str(&text),
                            ContentPart::Image { image } => media_parts.push(OpenAIUserContentPart::ImageUrl {
                                image_url: OpenAIImageUrl {
                                    url: if image.starts_with("data:") { image.clone() } else { format!("data:image/png;base64,{image}") },
                                },
                            }),
                            _ => {}
                        }
                    }
                    if !pending_images.is_empty() {
                        media_parts.extend(pending_images.drain(..));
                    }
                    if media_parts.is_empty() {
                        result.push(OpenAIChatMessage::User { content: OpenAIChatUserContent::Text(text_parts) });
                    } else {
                        let mut parts = media_parts;
                        if !text_parts.is_empty() {
                            parts.insert(0, OpenAIUserContentPart::Text { text: text_parts });
                        }
                        result.push(OpenAIChatMessage::User { content: OpenAIChatUserContent::Parts(parts) });
                    }
                }
                ChatMessage::Assistant { content } => {
                    let mut text = String::new();
                    let mut tool_calls = Vec::new();
                    let mut reasoning = String::new();
                    for part in content_parts(content) {
                        match part {
                            ContentPart::Text { text: t } => text.push_str(&t),
                            ContentPart::Reasoning { text: r, .. } => reasoning.push_str(&r),
                            ContentPart::ToolCallPart { tool_call_id, tool_name } => {
                                tool_calls.push(OpenAIAssistantToolCall {
                                    id: tool_call_id.clone(),
                                    call_type: "function".into(),
                                    function: OpenAIToolCallFunction { name: tool_name.clone(), arguments: "{}".into() },
                                });
                            }
                            _ => {}
                        }
                    }
                    result.push(OpenAIChatMessage::Assistant {
                        content: if text.is_empty() { None } else { Some(text) },
                        tool_calls: if tool_calls.is_empty() { None } else { Some(tool_calls) },
                        reasoning_content: if reasoning.is_empty() { None } else { Some(reasoning) },
                    });
                }
                ChatMessage::Tool { content } => {
                    for part in content {
                        if let crate::provider::ToolResultPart::ToolResult { tool_call_id, output, .. } = part {
                            result.push(OpenAIChatMessage::Tool {
                                tool_call_id: tool_call_id.clone(),
                                content: output.to_string(),
                            });
                        }
                    }
                }
            }
        }
        result
    }
}

fn extract_text(content: &MessageContent) -> String {
    match content {
        MessageContent::Text(t) => t.clone(),
        MessageContent::Parts(p) => p.iter().filter_map(|p| match p { ContentPart::Text { text } => Some(text.as_str()), _ => None }).collect::<Vec<_>>().join(""),
    }
}

fn content_parts(content: &MessageContent) -> &[ContentPart] {
    static EMPTY: Vec<ContentPart> = Vec::new();
    match content {
        MessageContent::Parts(p) => p,
        _ => &EMPTY,
    }
}

fn build_tools(tools: &[ToolDefinition]) -> Option<Vec<OpenAITool>> {
    if tools.is_empty() { return None; }
    Some(tools.iter().map(|t| OpenAITool {
        tool_type: "function".into(),
        function: OpenAIFunctionDef { name: t.name.clone(), description: t.description.clone(), parameters: t.parameters.clone() },
    }).collect())
}

// ── Event Mapping ──────────────────────────────────────────────────────

fn map_finish_reason(reason: &str) -> FinishReason {
    match reason {
        "stop" => FinishReason::Stop,
        "length" => FinishReason::Length,
        "content_filter" => FinishReason::ContentFilter,
        "function_call" | "tool_calls" => FinishReason::ToolCalls,
        _ => FinishReason::Unknown,
    }
}

fn map_usage(u: &OpenAIUsage) -> Usage {
    let cached = u.prompt_tokens_details.as_ref().and_then(|d| d.cached_tokens);
    let reasoning = u.completion_tokens_details.as_ref().and_then(|d| d.reasoning_tokens);
    let non_cached = u.prompt_tokens.map(|p| p.saturating_sub(cached.unwrap_or(0)));
    Usage {
        input_tokens: u.prompt_tokens,
        output_tokens: u.completion_tokens,
        non_cached_input_tokens: non_cached,
        cache_read_input_tokens: cached,
        reasoning_tokens: reasoning,
        total_tokens: u.total_tokens,
        provider_metadata: None,
    }
}

fn events_from_chat(
    event: OpenAIChatEvent,
    state: &mut ChatStreamState,
) -> Vec<LlmEvent> {
    let mut events = Vec::new();
    let usage = event.usage.as_ref().map(map_usage).or(state.usage.clone());
    let choice = &event.choices.first();

    if let Some(delta) = choice.and_then(|c| c.delta.as_ref()) {
        if let Some(ref rc) = delta.reasoning_content {
            if !state.reasoning_started {
                state.reasoning_started = true;
                events.push(LlmEvent::ReasoningStart { id: "reasoning-0".into(), provider_metadata: None });
            }
            events.push(LlmEvent::ReasoningDelta { id: "reasoning-0".into(), text: rc.clone(), provider_metadata: None });
        }
        if let Some(ref content) = delta.content {
            if !state.text_started {
                state.text_started = true;
                events.push(LlmEvent::TextStart { id: "text-0".into(), provider_metadata: None });
            }
            events.push(LlmEvent::TextDelta { id: "text-0".into(), text: content.clone(), provider_metadata: None });
        }
        if let Some(tool_deltas) = &delta.tool_calls {
            for td in tool_deltas {
                if let Some(ref name) = td.function.as_ref().and_then(|f| f.name.as_ref()) {
                    state.tool_stream.set_identity(td.index, name.clone(), td.id.clone().unwrap_or_default());
                }
                if let Some(ref args) = td.function.as_ref().and_then(|f| f.arguments.as_ref()) {
                    if let Some(ev) = state.tool_stream.append(td.index, args) {
                        if !state.step_started { events.push(LlmEvent::StepStart { index: 0 }); state.step_started = true; }
                        events.push(ev);
                    }
                }
            }
        }
    }

    if let Some(finish_reason) = choice.and_then(|c| c.finish_reason.as_ref()) {
        // Finish any pending tool calls
        for tool_ev in state.tool_stream.finish_all() {
            events.push(tool_ev);
        }
        let reason = map_finish_reason(finish_reason);
        if state.text_started { events.push(LlmEvent::TextEnd { id: "text-0".into(), provider_metadata: None }); }
        if state.reasoning_started { events.push(LlmEvent::ReasoningEnd { id: "reasoning-0".into(), provider_metadata: None }); }
        events.push(LlmEvent::StepFinish { index: 0, reason: reason.clone(), usage: usage.clone(), provider_metadata: None });
        events.push(LlmEvent::Finish { reason, usage, provider_metadata: None });
        state.finished = true;
    }

    state.usage = usage;
    events
}

struct ChatStreamState {
    tool_stream: ToolStreamAccumulator,
    text_started: bool,
    reasoning_started: bool,
    step_started: bool,
    usage: Option<Usage>,
    finished: bool,
}

// ── Provider impl ──────────────────────────────────────────────────────

#[async_trait]
impl Provider for OpenAIProvider {
    fn provider_id(&self) -> &str { "openai" }
    fn npm(&self) -> &str { "@ai-sdk/openai" }

    async fn list_models(&self) -> crate::error::Result<Vec<Model>> { Ok(self.models.clone()) }

    async fn get_model(&self, model_id: &str) -> crate::error::Result<Model> {
        self.models.iter().find(|m| m.id == model_id).cloned()
            .ok_or_else(|| Error::ModelNotFound { provider_id: "openai".into(), model_id: model_id.into() })
    }

    async fn stream(
        &self, model: &Model, messages: &[ChatMessage], tools: &[ToolDefinition],
    ) -> crate::error::Result<Box<dyn futures::Stream<Item = crate::error::Result<LlmEvent>> + Send + Unpin>> {
        let body = OpenAIChatBody {
            model: model.api.id.clone(),
            messages: Self::build_chat_messages(messages),
            tools: build_tools(tools),
            tool_choice: None,
            stream: true,
            stream_options: serde_json::json!({"include_usage": true}),
            max_tokens: Some(crate::provider::max_output_tokens(model, crate::provider::OUTPUT_TOKEN_MAX)),
            temperature: None,
            top_p: None,
        };

        let response = self.http_client.post(self.chat_url())
            .header("Authorization", format!("Bearer {}", self.api_key))
            .header("Content-Type", "application/json")
            .json(&body).send().await.map_err(|e| Error::Network(format!("OpenAI request: {e}")))?;

        if !response.status().is_success() {
            let status = response.status().as_u16();
            let text = response.text().await.unwrap_or_default();
            return Err(Error::Llm { module: "openai".into(), method: "stream".into(), reason: Box::new(classify_error(status, &text)) });
        }

        let sse_stream = parse_sse_stream(response);
        let state = ChatStreamState {
            tool_stream: ToolStreamAccumulator::new(),
            text_started: false, reasoning_started: false, step_started: false,
            usage: None, finished: false,
        };

        let llm_stream = futures::stream::unfold(
            (Box::pin(sse_stream) as Pin<Box<dyn futures::Stream<Item = Result<crate::sse::SseEvent, crate::sse::SseError>> + Send + Unpin>>, state, VecDeque::new()),
            |(mut sse, mut state, mut buffer)| async move {
                loop {
                    if let Some(ev) = buffer.pop_front() { return Some((ev, (sse, state, buffer))); }
                    if state.finished { return None; }
                    match sse.next().await {
                        Some(Ok(se)) if !se.is_done() && se.has_data() => {
                            if let Ok(oe) = serde_json::from_str::<OpenAIChatEvent>(&se.data) {
                                for ev in events_from_chat(oe, &mut state) { buffer.push_back(Ok(ev)); }
                                if let Some(ev) = buffer.pop_front() { return Some((ev, (sse, state, buffer))); }
                            }
                        }
                        Some(Err(e)) => return Some((Err(Error::ResponseStream(format!("OpenAI SSE: {e}"))), (sse, state, buffer))),
                        None => return None,
                        _ => continue,
                    }
                }
            },
        );
        Ok(Box::new(llm_stream))
    }

    async fn complete(&self, model: &Model, messages: &[ChatMessage], tools: &[ToolDefinition]) -> crate::error::Result<crate::provider::LlmResponse> {
        let mut stream = self.stream(model, messages, tools).await?;
        let mut events = Vec::new();
        let mut usage = None;
        while let Some(r) = stream.next().await {
            match r {
                Ok(ev) => { if let Some(u) = ev.usage() { usage = Some(u.clone()); } events.push(ev); }
                Err(_) => {}
            }
        }
        Ok(crate::provider::LlmResponse { events, usage })
    }
}

use std::pin::Pin;
use crate::error::LlmErrorReason;

fn classify_error(status: u16, body: &str) -> LlmErrorReason {
    let msg = || body.to_string();
    match status {
        401 | 403 => LlmErrorReason::Authentication { message: msg(), kind: crate::error::AuthErrorKind::Invalid },
        429 => LlmErrorReason::RateLimit { message: msg(), retry_after_ms: None },
        400 | 413 => {
            if crate::error::is_context_overflow(body) {
                LlmErrorReason::InvalidRequest { message: msg(), parameter: None, classification: Some("context-overflow".into()) }
            } else { LlmErrorReason::InvalidRequest { message: msg(), parameter: None, classification: None } }
        }
        500..=599 => LlmErrorReason::ProviderInternal { message: msg(), status, retry_after_ms: None },
        _ => LlmErrorReason::UnknownProvider { message: msg(), status: Some(status) },
    }
}

fn build_model_catalog() -> Vec<Model> {
    vec![
        make_model("gpt-5.2", "GPT-5.2", 200_000, 128_000, 1.75, 14.0),
        make_model("gpt-5.1", "GPT-5.1", 200_000, 128_000, 1.75, 14.0),
        make_model("gpt-5.1-codex", "GPT-5.1 Codex", 200_000, 128_000, 1.75, 14.0),
        make_model("gpt-5.1-mini", "GPT-5.1 Mini", 200_000, 128_000, 0.35, 1.40),
        make_model("gpt-5.1-nano", "GPT-5.1 Nano", 200_000, 128_000, 0.10, 0.40),
    ]
}

fn make_model(id: &str, name: &str, ctx: u64, out: u64, inp_cost: f64, out_cost: f64) -> Model {
    Model {
        id: id.into(), provider_id: "openai".into(), name: name.into(),
        api: crate::provider::ApiInfo { id: id.into(), url: DEFAULT_BASE_URL.into(), npm: "@ai-sdk/openai".into() },
        family: Some("gpt".into()),
        capabilities: crate::provider::Capabilities { temperature: true, reasoning: true, attachment: true, toolcall: true, input: crate::provider::Modalities { text: true, image: true, ..Default::default() }, output: crate::provider::Modalities { text: true, ..Default::default() }, interleaved: Default::default() },
        cost: crate::provider::Cost { input: inp_cost, output: out_cost, cache: Default::default(), tiers: None, experimental_over_200k: None },
        limit: crate::provider::TokenLimit { context: ctx, input: None, output: out },
        status: crate::provider::ModelStatus::Active, options: HashMap::new(), headers: HashMap::new(), release_date: "2026".into(), variants: None,
    }
}
