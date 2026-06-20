//! Groq API provider — OpenAI-compatible Chat Completions protocol.
//!
//! Groq uses the same Chat Completions protocol as OpenAI with SSE streaming
//! via `/openai/v1/chat/completions`.
//!
//! Base URL: https://api.groq.com/openai/v1
//! Auth: Bearer token via GROQ_API_KEY env var.
//! npm: @ai-sdk/groq
//!
//! Ported from:
//! - `packages/llm/src/protocols/openai-chat.ts` (493 lines)
//! - `packages/llm/src/providers/openai.ts` (63 lines)
//! - `packages/llm/src/providers/openai-compatible.ts` (66 lines)

use async_trait::async_trait;
use futures::StreamExt;
use serde::{Deserialize, Serialize};
use std::collections::{HashMap, VecDeque};

use crate::error::Error;
use crate::provider::{
    ChatMessage, ContentPart, FinishReason, LlmEvent, MessageContent, Model, Provider,
    ToolDefinition, Usage,
};
use crate::sse::parse_sse_stream;
use crate::tool_stream::ToolStreamAccumulator;

const DEFAULT_BASE_URL: &str = "https://api.groq.com/openai/v1";
const CHAT_PATH: &str = "/chat/completions";

fn resolve_api_key() -> Result<String, Error> {
    std::env::var("GROQ_API_KEY")
        .ok()
        .filter(|k| !k.is_empty())
        .ok_or_else(|| Error::Auth("GROQ_API_KEY environment variable not set".into()))
}

// ── Chat Completions Body (OpenAI-compatible) ──────────────────────────

#[derive(Debug, Serialize)]
struct GroqChatBody {
    model: String,
    messages: Vec<GroqChatMessage>,
    #[serde(skip_serializing_if = "Option::is_none")]
    tools: Option<Vec<GroqTool>>,
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
enum GroqChatMessage {
    #[serde(rename = "system")]
    System { content: String },
    #[serde(rename = "user")]
    User { content: GroqChatUserContent },
    #[serde(rename = "assistant")]
    Assistant {
        content: Option<String>,
        #[serde(skip_serializing_if = "Option::is_none")]
        tool_calls: Option<Vec<GroqAssistantToolCall>>,
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
enum GroqChatUserContent {
    Text(String),
    Parts(Vec<GroqUserContentPart>),
}

#[derive(Debug, Serialize)]
#[serde(tag = "type")]
enum GroqUserContentPart {
    #[serde(rename = "text")]
    Text { text: String },
    #[serde(rename = "image_url")]
    ImageUrl { image_url: GroqImageUrl },
}

#[derive(Debug, Serialize)]
struct GroqImageUrl {
    url: String,
}

#[derive(Debug, Serialize)]
struct GroqAssistantToolCall {
    id: String,
    #[serde(rename = "type")]
    call_type: String,
    function: GroqToolCallFunction,
}

#[derive(Debug, Serialize)]
struct GroqToolCallFunction {
    name: String,
    arguments: String,
}

#[derive(Debug, Serialize)]
struct GroqTool {
    #[serde(rename = "type")]
    tool_type: String,
    function: GroqFunctionDef,
}

#[derive(Debug, Serialize)]
struct GroqFunctionDef {
    name: String,
    description: String,
    parameters: serde_json::Value,
}

// ── Chat SSE Event types (OpenAI-compatible) ───────────────────────────

#[derive(Debug, Deserialize)]
struct GroqChatEvent {
    choices: Vec<GroqChoice>,
    #[serde(default)]
    usage: Option<GroqUsage>,
}

#[derive(Debug, Deserialize)]
struct GroqChoice {
    delta: Option<GroqDelta>,
    finish_reason: Option<String>,
}

#[derive(Debug, Deserialize)]
struct GroqDelta {
    content: Option<String>,
    reasoning_content: Option<String>,
    tool_calls: Option<Vec<GroqToolCallDelta>>,
}

#[derive(Debug, Deserialize)]
struct GroqToolCallDelta {
    index: u64,
    id: Option<String>,
    function: Option<GroqToolCallDeltaFn>,
}

#[derive(Debug, Deserialize)]
struct GroqToolCallDeltaFn {
    name: Option<String>,
    arguments: Option<String>,
}

#[derive(Debug, Deserialize)]
struct GroqUsage {
    prompt_tokens: Option<u64>,
    completion_tokens: Option<u64>,
    total_tokens: Option<u64>,
    prompt_tokens_details: Option<GroqPromptTokenDetails>,
    completion_tokens_details: Option<GroqCompletionTokenDetails>,
}

#[derive(Debug, Deserialize)]
struct GroqPromptTokenDetails {
    cached_tokens: Option<u64>,
}

#[derive(Debug, Deserialize)]
struct GroqCompletionTokenDetails {
    reasoning_tokens: Option<u64>,
}

// ── Groq Provider ─────────────────────────────────────────────────────

pub struct GroqProvider {
    api_key: String,
    base_url: String,
    http_client: reqwest::Client,
    models: Vec<Model>,
}

impl GroqProvider {
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

    fn chat_url(&self) -> String {
        format!("{}{CHAT_PATH}", self.base_url.trim_end_matches('/'))
    }

    fn build_chat_messages(messages: &[ChatMessage]) -> Vec<GroqChatMessage> {
        let mut result = Vec::new();
        let mut pending_images: Vec<GroqUserContentPart> = Vec::new();

        for msg in messages {
            match msg {
                ChatMessage::System { content } => {
                    let text = extract_text(content);
                    if !text.is_empty() {
                        result.push(GroqChatMessage::System { content: text });
                    }
                }
                ChatMessage::User { content } => {
                    let mut text_parts = String::new();
                    let mut media_parts: Vec<GroqUserContentPart> = Vec::new();
                    for part in content_parts(content) {
                        match part {
                            ContentPart::Text { text } => text_parts.push_str(&text),
                            ContentPart::Image { image } => {
                                media_parts.push(GroqUserContentPart::ImageUrl {
                                    image_url: GroqImageUrl {
                                        url: if image.starts_with("data:") {
                                            image.clone()
                                        } else {
                                            format!("data:image/png;base64,{image}")
                                        },
                                    },
                                });
                            }
                            _ => {}
                        }
                    }
                    if !pending_images.is_empty() {
                        media_parts.extend(pending_images.drain(..));
                    }
                    if media_parts.is_empty() {
                        result.push(GroqChatMessage::User {
                            content: GroqChatUserContent::Text(text_parts),
                        });
                    } else {
                        let mut parts = media_parts;
                        if !text_parts.is_empty() {
                            parts.insert(0, GroqUserContentPart::Text { text: text_parts });
                        }
                        result.push(GroqChatMessage::User {
                            content: GroqChatUserContent::Parts(parts),
                        });
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
                            ContentPart::ToolCallPart {
                                tool_call_id,
                                tool_name,
                                arguments,
                            } => {
                                tool_calls.push(GroqAssistantToolCall {
                                    id: tool_call_id.clone(),
                                    call_type: "function".into(),
                                    function: GroqToolCallFunction {
                                        name: tool_name.clone(),
                                        arguments: arguments.to_string(),
                                    },
                                });
                            }
                            _ => {}
                        }
                    }
                    result.push(GroqChatMessage::Assistant {
                        content: if text.is_empty() { None } else { Some(text) },
                        tool_calls: if tool_calls.is_empty() {
                            None
                        } else {
                            Some(tool_calls)
                        },
                        reasoning_content: if reasoning.is_empty() {
                            None
                        } else {
                            Some(reasoning)
                        },
                    });
                }
                ChatMessage::Tool { content } => {
                    for part in content {
                        let crate::provider::ToolResultPart::ToolResult {
                            tool_call_id,
                            output,
                            ..
                        } = part;
                        result.push(GroqChatMessage::Tool {
                            tool_call_id: tool_call_id.clone(),
                            content: output.to_string(),
                        });
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

fn content_parts(content: &MessageContent) -> &[ContentPart] {
    static EMPTY: Vec<ContentPart> = Vec::new();
    match content {
        MessageContent::Parts(p) => p,
        _ => &EMPTY,
    }
}

fn build_tools(tools: &[ToolDefinition]) -> Option<Vec<GroqTool>> {
    if tools.is_empty() {
        return None;
    }
    Some(
        tools
            .iter()
            .map(|t| GroqTool {
                tool_type: "function".into(),
                function: GroqFunctionDef {
                    name: t.name.clone(),
                    description: t.description.clone(),
                    parameters: t.parameters.clone(),
                },
            })
            .collect(),
    )
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

fn map_usage(u: &GroqUsage) -> Usage {
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

fn events_from_chat(event: GroqChatEvent, state: &mut ChatStreamState) -> Vec<LlmEvent> {
    let mut events = Vec::new();
    let usage = event.usage.as_ref().map(map_usage).or(state.usage.clone());
    let choice = &event.choices.first();

    if let Some(delta) = choice.and_then(|c| c.delta.as_ref()) {
        if let Some(ref rc) = delta.reasoning_content {
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
        if let Some(ref content) = delta.content {
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
        if let Some(tool_deltas) = &delta.tool_calls {
            for td in tool_deltas {
                if let Some(ref name) = td.function.as_ref().and_then(|f| f.name.as_ref()) {
                    state.tool_stream.set_identity(
                        td.index,
                        name.clone(),
                        td.id.clone().unwrap_or_default(),
                    );
                }
                if let Some(ref args) = td.function.as_ref().and_then(|f| f.arguments.as_ref()) {
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
            usage: usage.clone(),
            provider_metadata: None,
        });
        events.push(LlmEvent::Finish {
            reason,
            usage: usage.clone(),
            provider_metadata: None,
        });
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
impl Provider for GroqProvider {
    fn provider_id(&self) -> &str {
        "groq"
    }

    fn npm(&self) -> &str {
        "@ai-sdk/groq"
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
                provider_id: "groq".into(),
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
        let body = GroqChatBody {
            model: model.api.id.clone(),
            messages: Self::build_chat_messages(&messages),
            tools: build_tools(tools),
            tool_choice: None,
            stream: true,
            stream_options: serde_json::json!({"include_usage": true}),
            max_tokens: Some(crate::provider::max_output_tokens(
                model,
                crate::provider::OUTPUT_TOKEN_MAX,
            )),
            temperature: crate::provider::default_temperature(&model.api.id),
            top_p: crate::provider::default_top_p(&model.api.id),
        };

        let response = self
            .http_client
            .post(self.chat_url())
            .header("Authorization", format!("Bearer {}", self.api_key))
            .header("Content-Type", "application/json")
            .json(&body)
            .send()
            .await
            .map_err(|e| Error::Network(format!("Groq request: {e}")))?;

        if !response.status().is_success() {
            let status = response.status().as_u16();
            let text = response.text().await.unwrap_or_default();
            return Err(Error::Llm {
                module: "groq".into(),
                method: "stream".into(),
                reason: Box::new(classify_error(status, &text)),
            });
        }

        let sse_stream = parse_sse_stream(response);
        let state = ChatStreamState {
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
                                if let Ok(oe) = serde_json::from_str::<GroqChatEvent>(&se.data) {
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
                                    Err(Error::ResponseStream(format!("Groq SSE: {e}"))),
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
            match r {
                Ok(ev) => {
                    if let Some(u) = ev.usage() {
                        usage = Some(u.clone());
                    }
                    events.push(ev);
                }
                Err(_) => {}
            }
        }
        Ok(crate::provider::LlmResponse { events, usage })
    }
}

use crate::error::LlmErrorReason;
use std::pin::Pin;

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

// ── Model Catalog ──────────────────────────────────────────────────────

fn build_model_catalog() -> Vec<Model> {
    vec![
        make_model(
            "llama-4-maverick",
            "Llama 4 Maverick",
            "llama",
            128_000,
            8_000,
            0.20,
            0.60,
        ),
        make_model(
            "llama-4-scout",
            "Llama 4 Scout",
            "llama",
            128_000,
            8_000,
            0.18,
            0.54,
        ),
        make_model(
            "llama-3.3-70b-versatile",
            "Llama 3.3 70B",
            "llama",
            128_000,
            8_000,
            0.59,
            0.79,
        ),
        make_model(
            "mixtral-8x7b-32768",
            "Mixtral 8x7B",
            "mixtral",
            32_000,
            4_000,
            0.24,
            0.24,
        ),
        make_model(
            "gemma2-9b-it",
            "Gemma 2 9B",
            "gemma",
            8_000,
            4_000,
            0.20,
            0.20,
        ),
    ]
}

fn make_model(
    id: &str,
    name: &str,
    family: &str,
    ctx: u64,
    out: u64,
    inp_cost: f64,
    out_cost: f64,
) -> Model {
    Model {
        id: id.into(),
        provider_id: "groq".into(),
        name: name.into(),
        api: crate::provider::ApiInfo {
            id: id.into(),
            url: DEFAULT_BASE_URL.into(),
            npm: "@ai-sdk/groq".into(),
        },
        family: Some(family.into()),
        capabilities: crate::provider::Capabilities {
            temperature: true,
            reasoning: false,
            attachment: false,
            toolcall: true,
            input: crate::provider::Modalities {
                text: true,
                image: false,
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
        options: HashMap::new(),
        headers: HashMap::new(),
        release_date: "2024".into(),
        variants: None,
    }
}

// ── Tests ──────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    // ── Model catalog ───────────────────────────────────────────────

    #[test]
    fn test_model_catalog_count() {
        let models = build_model_catalog();
        assert_eq!(models.len(), 5);
    }

    #[test]
    fn test_model_catalog_has_llama4_maverick() {
        let models = build_model_catalog();
        let m = models
            .iter()
            .find(|m| m.id == "llama-4-maverick")
            .expect("llama-4-maverick not found");
        assert_eq!(m.name, "Llama 4 Maverick");
        assert_eq!(m.provider_id, "groq");
        assert_eq!(m.limit.context, 128_000);
        assert_eq!(m.limit.output, 8_000);
        assert_eq!(m.family.as_deref(), Some("llama"));
        assert!(m.capabilities.toolcall);
        assert!(m.capabilities.temperature);
        assert_eq!(m.api.npm, "@ai-sdk/groq");
    }

    #[test]
    fn test_model_catalog_has_llama4_scout() {
        let models = build_model_catalog();
        let m = models
            .iter()
            .find(|m| m.id == "llama-4-scout")
            .expect("llama-4-scout not found");
        assert_eq!(m.name, "Llama 4 Scout");
        assert_eq!(m.limit.context, 128_000);
        assert_eq!(m.limit.output, 8_000);
        assert_eq!(m.family.as_deref(), Some("llama"));
    }

    #[test]
    fn test_model_catalog_has_llama33_70b() {
        let models = build_model_catalog();
        let m = models
            .iter()
            .find(|m| m.id == "llama-3.3-70b-versatile")
            .expect("llama-3.3-70b-versatile not found");
        assert_eq!(m.name, "Llama 3.3 70B");
        assert_eq!(m.limit.context, 128_000);
        assert_eq!(m.limit.output, 8_000);
        assert_eq!(m.family.as_deref(), Some("llama"));
    }

    #[test]
    fn test_model_catalog_has_mixtral_8x7b() {
        let models = build_model_catalog();
        let m = models
            .iter()
            .find(|m| m.id == "mixtral-8x7b-32768")
            .expect("mixtral-8x7b-32768 not found");
        assert_eq!(m.name, "Mixtral 8x7B");
        assert_eq!(m.limit.context, 32_000);
        assert_eq!(m.limit.output, 4_000);
        assert_eq!(m.family.as_deref(), Some("mixtral"));
    }

    #[test]
    fn test_model_catalog_has_gemma2_9b() {
        let models = build_model_catalog();
        let m = models
            .iter()
            .find(|m| m.id == "gemma2-9b-it")
            .expect("gemma2-9b-it not found");
        assert_eq!(m.name, "Gemma 2 9B");
        assert_eq!(m.limit.context, 8_000);
        assert_eq!(m.limit.output, 4_000);
        assert_eq!(m.family.as_deref(), Some("gemma"));
    }

    #[test]
    fn test_all_models_have_groq_provider_id() {
        let models = build_model_catalog();
        for m in &models {
            assert_eq!(
                m.provider_id, "groq",
                "model {} has wrong provider_id: {}",
                m.id, m.provider_id
            );
        }
    }

    #[test]
    fn test_all_models_active() {
        let models = build_model_catalog();
        for m in &models {
            assert_eq!(
                m.status,
                crate::provider::ModelStatus::Active,
                "model {} should be Active",
                m.id
            );
        }
    }

    #[test]
    fn test_all_models_have_toolcall() {
        let models = build_model_catalog();
        for m in &models {
            assert!(
                m.capabilities.toolcall,
                "model {} should support tool calls",
                m.id
            );
        }
    }

    #[test]
    fn test_all_models_have_text_io() {
        let models = build_model_catalog();
        for m in &models {
            assert!(
                m.capabilities.input.text,
                "model {} should support text input",
                m.id
            );
            assert!(
                m.capabilities.output.text,
                "model {} should support text output",
                m.id
            );
        }
    }

    // ── Provider ID / npm ──────────────────────────────────────────

    #[test]
    fn test_provider_id_static() {
        let provider = GroqProvider::with_base_url("test-key".into(), DEFAULT_BASE_URL.into())
            .expect("should construct with test key");
        assert_eq!(provider.provider_id(), "groq");
        assert_eq!(provider.npm(), "@ai-sdk/groq");
    }

    // ── Model lookup ───────────────────────────────────────────────

    #[tokio::test]
    async fn test_get_model_found() {
        let provider = GroqProvider::with_base_url("test-key".into(), DEFAULT_BASE_URL.into())
            .expect("should construct");
        let model = provider.get_model("llama-4-maverick").await;
        assert!(model.is_ok());
        let m = model.unwrap();
        assert_eq!(m.id, "llama-4-maverick");
        assert_eq!(m.name, "Llama 4 Maverick");
    }

    #[tokio::test]
    async fn test_get_model_found_mixtral() {
        let provider = GroqProvider::with_base_url("test-key".into(), DEFAULT_BASE_URL.into())
            .expect("should construct");
        let model = provider.get_model("mixtral-8x7b-32768").await;
        assert!(model.is_ok());
        let m = model.unwrap();
        assert_eq!(m.id, "mixtral-8x7b-32768");
        assert_eq!(m.name, "Mixtral 8x7B");
    }

    #[tokio::test]
    async fn test_get_model_not_found() {
        let provider = GroqProvider::with_base_url("test-key".into(), DEFAULT_BASE_URL.into())
            .expect("should construct");
        let result = provider.get_model("nonexistent-model").await;
        assert!(result.is_err());
        match result {
            Err(Error::ModelNotFound {
                provider_id,
                model_id,
            }) => {
                assert_eq!(provider_id, "groq");
                assert_eq!(model_id, "nonexistent-model");
            }
            other => panic!("expected ModelNotFound, got: {other:?}"),
        }
    }

    #[tokio::test]
    async fn test_get_model_case_sensitive() {
        let provider = GroqProvider::with_base_url("test-key".into(), DEFAULT_BASE_URL.into())
            .expect("should construct");
        let result = provider.get_model("Llama-4-Maverick").await;
        assert!(result.is_err(), "model lookup should be case-sensitive");
    }

    #[tokio::test]
    async fn test_list_models_returns_all() {
        let provider = GroqProvider::with_base_url("test-key".into(), DEFAULT_BASE_URL.into())
            .expect("should construct");
        let models = provider.list_models().await;
        assert!(models.is_ok());
        assert_eq!(models.unwrap().len(), 5);
    }

    // ── Error classification ───────────────────────────────────────

    #[test]
    fn test_classify_error_auth_401() {
        let err = classify_error(401, r#"{"error":"Invalid API key"}"#);
        match err {
            LlmErrorReason::Authentication { kind, .. } => {
                assert_eq!(kind, crate::error::AuthErrorKind::Invalid);
            }
            other => panic!("expected Authentication, got: {other:?}"),
        }
    }

    #[test]
    fn test_classify_error_auth_403() {
        let err = classify_error(403, r#"{"error":"Forbidden"}"#);
        assert!(matches!(err, LlmErrorReason::Authentication { .. }));
    }

    #[test]
    fn test_classify_error_rate_limit() {
        let err = classify_error(429, r#"{"error":"Rate limited"}"#);
        assert!(matches!(err, LlmErrorReason::RateLimit { .. }));
    }

    #[test]
    fn test_classify_error_context_overflow() {
        let err = classify_error(400, "prompt is too long: input exceeds the context window");
        match err {
            LlmErrorReason::InvalidRequest { classification, .. } => {
                assert_eq!(classification, Some("context-overflow".into()));
            }
            other => panic!("expected InvalidRequest with context-overflow, got: {other:?}"),
        }
    }

    #[test]
    fn test_classify_error_invalid_request_400() {
        let err = classify_error(400, r#"{"error":"bad request"}"#);
        match err {
            LlmErrorReason::InvalidRequest { classification, .. } => {
                assert_eq!(classification, None);
            }
            other => panic!("expected InvalidRequest, got: {other:?}"),
        }
    }

    #[test]
    fn test_classify_error_invalid_request_413() {
        let err = classify_error(413, "Payload too large");
        assert!(matches!(err, LlmErrorReason::InvalidRequest { .. }));
    }

    #[test]
    fn test_classify_error_provider_internal_500() {
        let err = classify_error(500, r#"{"error":"Internal Server Error"}"#);
        match err {
            LlmErrorReason::ProviderInternal { status, .. } => {
                assert_eq!(status, 500);
            }
            other => panic!("expected ProviderInternal, got: {other:?}"),
        }
    }

    #[test]
    fn test_classify_error_provider_internal_503() {
        let err = classify_error(503, r#"{"error":"Service Unavailable"}"#);
        match err {
            LlmErrorReason::ProviderInternal { status, .. } => {
                assert_eq!(status, 503);
            }
            other => panic!("expected ProviderInternal, got: {other:?}"),
        }
    }

    #[test]
    fn test_classify_error_unknown() {
        let err = classify_error(418, "I'm a teapot");
        match err {
            LlmErrorReason::UnknownProvider { status, .. } => {
                assert_eq!(status, Some(418));
            }
            other => panic!("expected UnknownProvider, got: {other:?}"),
        }
    }

    // ── Finish reason mapping ──────────────────────────────────────

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
        assert_eq!(map_finish_reason("weird_reason"), FinishReason::Unknown);
    }

    // ── API key resolution ─────────────────────────────────────────

    #[test]
    fn test_resolve_api_key_missing() {
        // Groq's env var is unlikely to be set in CI; verify error path
        let has_key = std::env::var("GROQ_API_KEY").is_ok();
        if !has_key {
            let result = resolve_api_key();
            assert!(result.is_err());
            match result {
                Err(Error::Auth(msg)) => {
                    assert!(msg.contains("GROQ_API_KEY"));
                }
                other => panic!("expected Auth error, got: {other:?}"),
            }
        }
    }

    #[test]
    fn test_resolve_api_key_present() {
        std::env::set_var("GROQ_API_KEY", "gsk_test12345");
        let result = resolve_api_key();
        assert_eq!(result.unwrap(), "gsk_test12345");
        std::env::remove_var("GROQ_API_KEY");
    }

    // ── Base URL construction ──────────────────────────────────────

    #[test]
    fn test_chat_url_default() {
        let provider = GroqProvider::with_base_url("test-key".into(), DEFAULT_BASE_URL.into())
            .expect("should construct");
        assert_eq!(
            provider.chat_url(),
            "https://api.groq.com/openai/v1/chat/completions"
        );
    }

    #[test]
    fn test_chat_url_trailing_slash() {
        let provider = GroqProvider::with_base_url(
            "test-key".into(),
            "https://api.groq.com/openai/v1/".into(),
        )
        .expect("should construct");
        assert_eq!(
            provider.chat_url(),
            "https://api.groq.com/openai/v1/chat/completions"
        );
    }

    #[test]
    fn test_chat_url_custom_base() {
        let provider = GroqProvider::with_base_url(
            "test-key".into(),
            "https://groq-proxy.example.com/v1".into(),
        )
        .expect("should construct");
        assert_eq!(
            provider.chat_url(),
            "https://groq-proxy.example.com/v1/chat/completions"
        );
    }

    // ── Chat message building ──────────────────────────────────────

    #[test]
    fn test_build_chat_messages_system() {
        let messages = vec![ChatMessage::System {
            content: MessageContent::Text("You are helpful.".into()),
        }];
        let built = GroqProvider::build_chat_messages(&messages);
        assert_eq!(built.len(), 1);
    }

    #[test]
    fn test_build_chat_messages_empty_system_skipped() {
        let messages = vec![ChatMessage::System {
            content: MessageContent::Text(String::new()),
        }];
        let built = GroqProvider::build_chat_messages(&messages);
        assert_eq!(built.len(), 0);
    }

    #[test]
    fn test_build_chat_messages_user_text() {
        let messages = vec![ChatMessage::User {
            content: MessageContent::Text("Hello".into()),
        }];
        let built = GroqProvider::build_chat_messages(&messages);
        assert_eq!(built.len(), 1);
    }

    // ── Usage mapping ──────────────────────────────────────────────

    #[test]
    fn test_map_usage_complete() {
        let u = GroqUsage {
            prompt_tokens: Some(1500),
            completion_tokens: Some(800),
            total_tokens: Some(2300),
            prompt_tokens_details: Some(GroqPromptTokenDetails {
                cached_tokens: Some(200),
            }),
            completion_tokens_details: Some(GroqCompletionTokenDetails {
                reasoning_tokens: Some(100),
            }),
        };
        let usage = map_usage(&u);
        assert_eq!(usage.input_tokens, Some(1500));
        assert_eq!(usage.output_tokens, Some(800));
        assert_eq!(usage.total_tokens, Some(2300));
        assert_eq!(usage.cache_read_input_tokens, Some(200));
        assert_eq!(usage.reasoning_tokens, Some(100));
        assert_eq!(usage.non_cached_input_tokens, Some(1300));
    }

    #[test]
    fn test_map_usage_minimal() {
        let u = GroqUsage {
            prompt_tokens: Some(500),
            completion_tokens: Some(300),
            total_tokens: Some(800),
            prompt_tokens_details: None,
            completion_tokens_details: None,
        };
        let usage = map_usage(&u);
        assert_eq!(usage.input_tokens, Some(500));
        assert_eq!(usage.output_tokens, Some(300));
        assert_eq!(usage.cache_read_input_tokens, None);
        assert_eq!(usage.reasoning_tokens, None);
        assert_eq!(usage.non_cached_input_tokens, Some(500));
    }

    // ── extract_text ───────────────────────────────────────────────

    #[test]
    fn test_extract_text_simple() {
        let content = MessageContent::Text("hello world".into());
        assert_eq!(extract_text(&content), "hello world");
    }

    #[test]
    fn test_extract_text_parts() {
        let content = MessageContent::Parts(vec![
            ContentPart::Text {
                text: "hello ".into(),
            },
            ContentPart::Text {
                text: "world".into(),
            },
        ]);
        assert_eq!(extract_text(&content), "hello world");
    }

    #[test]
    fn test_extract_text_mixed_parts() {
        let content = MessageContent::Parts(vec![
            ContentPart::Text { text: "hi ".into() },
            ContentPart::Reasoning {
                text: "let me think...".into(),
                provider_options: None,
            },
            ContentPart::Text {
                text: "there".into(),
            },
        ]);
        assert_eq!(extract_text(&content), "hi there");
    }

    // ── Chat body serialization ────────────────────────────────────

    #[test]
    fn test_chat_body_stream_options() {
        let body = GroqChatBody {
            model: "llama-4-maverick".into(),
            messages: vec![],
            tools: None,
            tool_choice: None,
            stream: true,
            stream_options: serde_json::json!({"include_usage": true}),
            max_tokens: Some(8_000),
            temperature: None,
            top_p: None,
        };
        let json = serde_json::to_value(&body).unwrap();
        assert_eq!(json["model"], "llama-4-maverick");
        assert_eq!(json["stream"], true);
        assert_eq!(json["stream_options"]["include_usage"], true);
    }

    #[test]
    fn test_chat_body_includes_tools_when_present() {
        let tools = vec![ToolDefinition {
            name: "get_weather".into(),
            description: "Get the weather".into(),
            parameters: serde_json::json!({"type": "object"}),
        }];
        let body = GroqChatBody {
            model: "llama-4-maverick".into(),
            messages: vec![],
            tools: build_tools(&tools),
            tool_choice: None,
            stream: true,
            stream_options: serde_json::json!({"include_usage": true}),
            max_tokens: Some(8_000),
            temperature: None,
            top_p: None,
        };
        let json = serde_json::to_value(&body).unwrap();
        assert!(json["tools"].is_array());
        assert_eq!(json["tools"][0]["type"], "function");
        assert_eq!(json["tools"][0]["function"]["name"], "get_weather");
    }

    // ── Model cost sanity ──────────────────────────────────────────

    #[test]
    fn test_model_costs_are_reasonable() {
        let models = build_model_catalog();
        for m in &models {
            assert!(
                m.cost.input >= 0.0,
                "model {} has negative input cost",
                m.id
            );
            assert!(
                m.cost.output >= 0.0,
                "model {} has negative output cost",
                m.id
            );
        }
    }

    #[test]
    fn test_model_limits_are_positive() {
        let models = build_model_catalog();
        for m in &models {
            assert!(m.limit.context > 0, "model {} has zero context", m.id);
            assert!(m.limit.output > 0, "model {} has zero output", m.id);
        }
    }
}
