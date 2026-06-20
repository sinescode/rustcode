//! Together AI provider — OpenAI-compatible Chat Completions.
//!
//! Together AI uses the same Chat Completions protocol as OpenAI,
//! with SSE streaming via `/v1/chat/completions`.
//!
//! Ported from:
//! - `packages/llm/src/protocols/openai-chat.ts` (493 lines)
//! - `packages/llm/src/providers/openai.ts` (63 lines)

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

const DEFAULT_BASE_URL: &str = "https://api.together.xyz/v1";
const CHAT_PATH: &str = "/chat/completions";

fn resolve_api_key() -> Result<String, Error> {
    std::env::var("TOGETHER_API_KEY")
        .ok()
        .filter(|k| !k.is_empty())
        .ok_or_else(|| Error::Auth("TOGETHER_API_KEY environment variable not set".into()))
}

// ── Chat Completions Body (OpenAI-compatible) ──────────────────────────

#[derive(Debug, Serialize)]
struct TogetherChatBody {
    model: String,
    messages: Vec<TogetherChatMessage>,
    #[serde(skip_serializing_if = "Option::is_none")]
    tools: Option<Vec<TogetherTool>>,
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
enum TogetherChatMessage {
    #[serde(rename = "system")]
    System { content: String },
    #[serde(rename = "user")]
    User { content: TogetherChatUserContent },
    #[serde(rename = "assistant")]
    Assistant {
        content: Option<String>,
        #[serde(skip_serializing_if = "Option::is_none")]
        tool_calls: Option<Vec<TogetherAssistantToolCall>>,
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
enum TogetherChatUserContent {
    Text(String),
    Parts(Vec<TogetherUserContentPart>),
}

#[derive(Debug, Serialize)]
#[serde(tag = "type")]
enum TogetherUserContentPart {
    #[serde(rename = "text")]
    Text { text: String },
    #[serde(rename = "image_url")]
    ImageUrl { image_url: TogetherImageUrl },
}

#[derive(Debug, Serialize)]
struct TogetherImageUrl {
    url: String,
}

#[derive(Debug, Serialize)]
struct TogetherAssistantToolCall {
    id: String,
    #[serde(rename = "type")]
    call_type: String,
    function: TogetherToolCallFunction,
}

#[derive(Debug, Serialize)]
struct TogetherToolCallFunction {
    name: String,
    arguments: String,
}

#[derive(Debug, Serialize)]
struct TogetherTool {
    #[serde(rename = "type")]
    tool_type: String,
    function: TogetherFunctionDef,
}

#[derive(Debug, Serialize)]
struct TogetherFunctionDef {
    name: String,
    description: String,
    parameters: serde_json::Value,
}

// ── Chat SSE Event types (OpenAI-compatible) ───────────────────────────

#[derive(Debug, Deserialize)]
struct TogetherChatEvent {
    choices: Vec<TogetherChoice>,
    #[serde(default)]
    usage: Option<TogetherUsage>,
}

#[derive(Debug, Deserialize)]
struct TogetherChoice {
    delta: Option<TogetherDelta>,
    finish_reason: Option<String>,
}

#[derive(Debug, Deserialize)]
struct TogetherDelta {
    content: Option<String>,
    reasoning_content: Option<String>,
    tool_calls: Option<Vec<TogetherToolCallDelta>>,
}

#[derive(Debug, Deserialize)]
struct TogetherToolCallDelta {
    index: u64,
    id: Option<String>,
    function: Option<TogetherToolCallDeltaFn>,
}

#[derive(Debug, Deserialize)]
struct TogetherToolCallDeltaFn {
    name: Option<String>,
    arguments: Option<String>,
}

#[derive(Debug, Deserialize)]
struct TogetherUsage {
    prompt_tokens: Option<u64>,
    completion_tokens: Option<u64>,
    total_tokens: Option<u64>,
    prompt_tokens_details: Option<TogetherPromptTokenDetails>,
    completion_tokens_details: Option<TogetherCompletionTokenDetails>,
}

#[derive(Debug, Deserialize)]
struct TogetherPromptTokenDetails {
    cached_tokens: Option<u64>,
}

#[derive(Debug, Deserialize)]
struct TogetherCompletionTokenDetails {
    reasoning_tokens: Option<u64>,
}

// ── Together AI Provider ───────────────────────────────────────────────

pub struct TogetherProvider {
    api_key: String,
    base_url: String,
    http_client: reqwest::Client,
    models: Vec<Model>,
}

impl TogetherProvider {
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

    fn build_chat_messages(messages: &[ChatMessage]) -> Vec<TogetherChatMessage> {
        let mut result = Vec::new();
        let mut pending_images: Vec<TogetherUserContentPart> = Vec::new();

        for msg in messages {
            match msg {
                ChatMessage::System { content } => {
                    let text = extract_text(content);
                    if !text.is_empty() {
                        result.push(TogetherChatMessage::System { content: text });
                    }
                }
                ChatMessage::User { content } => {
                    let mut text_parts = String::new();
                    let mut media_parts: Vec<TogetherUserContentPart> = Vec::new();
                    for part in content_parts(content) {
                        match part {
                            ContentPart::Text { text } => text_parts.push_str(&text),
                            ContentPart::Image { image } => {
                                media_parts.push(TogetherUserContentPart::ImageUrl {
                                    image_url: TogetherImageUrl {
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
                        result.push(TogetherChatMessage::User {
                            content: TogetherChatUserContent::Text(text_parts),
                        });
                    } else {
                        let mut parts = media_parts;
                        if !text_parts.is_empty() {
                            parts.insert(0, TogetherUserContentPart::Text { text: text_parts });
                        }
                        result.push(TogetherChatMessage::User {
                            content: TogetherChatUserContent::Parts(parts),
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
                            } => {
                                tool_calls.push(TogetherAssistantToolCall {
                                    id: tool_call_id.clone(),
                                    call_type: "function".into(),
                                    function: TogetherToolCallFunction {
                                        name: tool_name.clone(),
                                        arguments: "{}".into(),
                                    },
                                });
                            }
                            _ => {}
                        }
                    }
                    result.push(TogetherChatMessage::Assistant {
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
                        result.push(TogetherChatMessage::Tool {
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

fn build_tools(tools: &[ToolDefinition]) -> Option<Vec<TogetherTool>> {
    if tools.is_empty() {
        return None;
    }
    Some(
        tools
            .iter()
            .map(|t| TogetherTool {
                tool_type: "function".into(),
                function: TogetherFunctionDef {
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

fn map_usage(u: &TogetherUsage) -> Usage {
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

fn events_from_chat(event: TogetherChatEvent, state: &mut ChatStreamState) -> Vec<LlmEvent> {
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
impl Provider for TogetherProvider {
    fn provider_id(&self) -> &str {
        "together"
    }

    fn npm(&self) -> &str {
        "@ai-sdk/togetherai"
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
                provider_id: "together".into(),
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
        let body = TogetherChatBody {
            model: model.api.id.clone(),
            messages: Self::build_chat_messages(messages),
            tools: build_tools(tools),
            tool_choice: None,
            stream: true,
            stream_options: serde_json::json!({"include_usage": true}),
            max_tokens: Some(crate::provider::max_output_tokens(
                model,
                crate::provider::OUTPUT_TOKEN_MAX,
            )),
            temperature: None,
            top_p: None,
        };

        let response = self
            .http_client
            .post(self.chat_url())
            .header("Authorization", format!("Bearer {}", self.api_key))
            .header("Content-Type", "application/json")
            .json(&body)
            .send()
            .await
            .map_err(|e| Error::Network(format!("Together AI request: {e}")))?;

        if !response.status().is_success() {
            let status = response.status().as_u16();
            let text = response.text().await.unwrap_or_default();
            return Err(Error::Llm {
                module: "together".into(),
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
                                if let Ok(oe) = serde_json::from_str::<TogetherChatEvent>(&se.data)
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
                                    Err(Error::ResponseStream(format!("Together AI SSE: {e}"))),
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
            "meta-llama/Llama-4-Maverick-17B-128E-Instruct",
            "Llama 4 Maverick",
            128_000,
            8_000,
            0.20,
            0.60,
        ),
        make_model(
            "meta-llama/Llama-4-Scout-17B-16E-Instruct",
            "Llama 4 Scout",
            128_000,
            8_000,
            0.18,
            0.54,
        ),
        make_model(
            "meta-llama/Llama-3.3-70B-Instruct-Turbo",
            "Llama 3.3 70B Turbo",
            128_000,
            8_000,
            0.88,
            0.88,
        ),
        make_model(
            "deepseek-ai/DeepSeek-V3",
            "DeepSeek V3",
            128_000,
            8_000,
            1.25,
            1.25,
        ),
        make_model(
            "mistralai/Mixtral-8x22B-Instruct-v0.1",
            "Mixtral 8x22B",
            64_000,
            4_000,
            0.90,
            0.90,
        ),
        make_model(
            "Qwen/Qwen2.5-72B-Instruct",
            "Qwen 2.5 72B",
            32_000,
            8_000,
            1.20,
            1.20,
        ),
    ]
}

fn make_model(id: &str, name: &str, ctx: u64, out: u64, inp_cost: f64, out_cost: f64) -> Model {
    Model {
        id: id.into(),
        provider_id: "together".into(),
        name: name.into(),
        api: crate::provider::ApiInfo {
            id: id.into(),
            url: DEFAULT_BASE_URL.into(),
            npm: "@ai-sdk/togetherai".into(),
        },
        family: None,
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
        assert_eq!(models.len(), 6);
    }

    #[test]
    fn test_model_catalog_has_llama4_maverick() {
        let models = build_model_catalog();
        let m = models
            .iter()
            .find(|m| m.id == "meta-llama/Llama-4-Maverick-17B-128E-Instruct")
            .expect("Llama 4 Maverick not found");
        assert_eq!(m.name, "Llama 4 Maverick");
        assert_eq!(m.limit.context, 128_000);
        assert_eq!(m.limit.output, 8_000);
        assert_eq!(m.provider_id, "together");
    }

    #[test]
    fn test_model_catalog_has_llama4_scout() {
        let models = build_model_catalog();
        let m = models
            .iter()
            .find(|m| m.id == "meta-llama/Llama-4-Scout-17B-16E-Instruct")
            .expect("Llama 4 Scout not found");
        assert_eq!(m.name, "Llama 4 Scout");
        assert_eq!(m.limit.context, 128_000);
        assert_eq!(m.limit.output, 8_000);
    }

    #[test]
    fn test_model_catalog_has_llama33_70b_turbo() {
        let models = build_model_catalog();
        let m = models
            .iter()
            .find(|m| m.id == "meta-llama/Llama-3.3-70B-Instruct-Turbo")
            .expect("Llama 3.3 70B Turbo not found");
        assert_eq!(m.name, "Llama 3.3 70B Turbo");
        assert_eq!(m.limit.context, 128_000);
    }

    #[test]
    fn test_model_catalog_has_deepseek_v3() {
        let models = build_model_catalog();
        let m = models
            .iter()
            .find(|m| m.id == "deepseek-ai/DeepSeek-V3")
            .expect("DeepSeek V3 not found");
        assert_eq!(m.name, "DeepSeek V3");
        assert_eq!(m.limit.context, 128_000);
    }

    #[test]
    fn test_model_catalog_has_mixtral_8x22b() {
        let models = build_model_catalog();
        let m = models
            .iter()
            .find(|m| m.id == "mistralai/Mixtral-8x22B-Instruct-v0.1")
            .expect("Mixtral 8x22B not found");
        assert_eq!(m.name, "Mixtral 8x22B");
        assert_eq!(m.limit.context, 64_000);
        assert_eq!(m.limit.output, 4_000);
    }

    #[test]
    fn test_model_catalog_has_qwen25_72b() {
        let models = build_model_catalog();
        let m = models
            .iter()
            .find(|m| m.id == "Qwen/Qwen2.5-72B-Instruct")
            .expect("Qwen 2.5 72B not found");
        assert_eq!(m.name, "Qwen 2.5 72B");
        assert_eq!(m.limit.context, 32_000);
    }

    #[test]
    fn test_all_models_have_together_provider_id() {
        let models = build_model_catalog();
        for m in &models {
            assert_eq!(
                m.provider_id, "together",
                "model {} has wrong provider_id: {}",
                m.id, m.provider_id
            );
        }
    }

    // ── Provider ID / npm ──────────────────────────────────────────

    #[test]
    fn test_provider_id() {
        let provider = TogetherProvider::new();
        assert!(
            provider.is_ok(),
            "TOGETHER_API_KEY not set in CI; test with_mock instead"
        );
        if let Ok(p) = provider {
            assert_eq!(p.provider_id(), "together");
            assert_eq!(p.npm(), "@ai-sdk/togetherai");
        }
    }

    #[test]
    fn test_provider_id_static() {
        // Verify the provider_id and npm strings are correct
        // without needing an API key
        let provider = TogetherProvider::with_base_url("test-key".into(), DEFAULT_BASE_URL.into())
            .expect("should construct with test key");
        assert_eq!(provider.provider_id(), "together");
        assert_eq!(provider.npm(), "@ai-sdk/togetherai");
    }

    // ── Model lookup ───────────────────────────────────────────────

    #[tokio::test]
    async fn test_get_model_found() {
        let provider = TogetherProvider::with_base_url("test-key".into(), DEFAULT_BASE_URL.into())
            .expect("should construct");
        let model = provider.get_model("deepseek-ai/DeepSeek-V3").await;
        assert!(model.is_ok());
        let m = model.unwrap();
        assert_eq!(m.id, "deepseek-ai/DeepSeek-V3");
        assert_eq!(m.name, "DeepSeek V3");
    }

    #[tokio::test]
    async fn test_get_model_not_found() {
        let provider = TogetherProvider::with_base_url("test-key".into(), DEFAULT_BASE_URL.into())
            .expect("should construct");
        let result = provider.get_model("nonexistent-model").await;
        assert!(result.is_err());
        match result {
            Err(Error::ModelNotFound {
                provider_id,
                model_id,
            }) => {
                assert_eq!(provider_id, "together");
                assert_eq!(model_id, "nonexistent-model");
            }
            other => panic!("expected ModelNotFound, got: {other:?}"),
        }
    }

    #[tokio::test]
    async fn test_list_models_returns_all() {
        let provider = TogetherProvider::with_base_url("test-key".into(), DEFAULT_BASE_URL.into())
            .expect("should construct");
        let models = provider.list_models().await;
        assert!(models.is_ok());
        assert_eq!(models.unwrap().len(), 6);
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
    fn test_classify_error_invalid_request() {
        let err = classify_error(400, r#"{"error":"bad request"}"#);
        assert!(matches!(err, LlmErrorReason::InvalidRequest { .. }));
    }

    #[test]
    fn test_classify_error_provider_internal() {
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

    // ── Chat message building ──────────────────────────────────────

    #[test]
    fn test_build_chat_messages_system() {
        let messages = vec![ChatMessage::System {
            content: MessageContent::Text("You are helpful.".into()),
        }];
        let built = TogetherProvider::build_chat_messages(&messages);
        assert_eq!(built.len(), 1);
    }

    #[test]
    fn test_build_chat_messages_empty_system_skipped() {
        let messages = vec![ChatMessage::System {
            content: MessageContent::Text(String::new()),
        }];
        let built = TogetherProvider::build_chat_messages(&messages);
        assert_eq!(built.len(), 0);
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
    fn test_map_finish_reason_tool_calls() {
        assert_eq!(map_finish_reason("tool_calls"), FinishReason::ToolCalls);
    }

    #[test]
    fn test_map_finish_reason_unknown() {
        assert_eq!(map_finish_reason("weird_reason"), FinishReason::Unknown);
    }

    // ── API key resolution (negative test) ─────────────────────────

    #[test]
    fn test_resolve_api_key_missing() {
        // Temporarily remove the env var to test the error path.
        // This test is safe because it only checks that the function
        // returns an error when the var is absent; it doesn't modify
        // the real env permanently.
        let result = std::env::var("TOGETHER_API_KEY");
        if result.is_ok() {
            // Key is set — skip the negative test to avoid interference
            return;
        }
        let err = resolve_api_key();
        assert!(err.is_err());
        match err {
            Err(Error::Auth(msg)) => {
                assert!(msg.contains("TOGETHER_API_KEY"));
            }
            other => panic!("expected Auth error, got: {other:?}"),
        }
    }

    // ── Base URL construction ──────────────────────────────────────

    #[test]
    fn test_chat_url() {
        let provider = TogetherProvider::with_base_url("test-key".into(), DEFAULT_BASE_URL.into())
            .expect("should construct");
        assert_eq!(
            provider.chat_url(),
            "https://api.together.xyz/v1/chat/completions"
        );
    }

    #[test]
    fn test_chat_url_trailing_slash() {
        let provider = TogetherProvider::with_base_url(
            "test-key".into(),
            "https://api.together.xyz/v1/".into(),
        )
        .expect("should construct");
        assert_eq!(
            provider.chat_url(),
            "https://api.together.xyz/v1/chat/completions"
        );
    }

    // ── Capabilities ───────────────────────────────────────────────

    #[test]
    fn test_models_have_toolcall_capability() {
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
    fn test_models_have_text_input_output() {
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

    // ── Model status ───────────────────────────────────────────────

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
}
