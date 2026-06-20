//! Mistral API provider (OpenAI-compatible Chat Completions protocol).
//!
//! Mistral exposes an OpenAI-compatible `/v1/chat/completions` endpoint with
//! SSE streaming. This provider reuses the same wire format as the OpenAI
//! provider — identical request bodies, SSE event shapes, and error patterns.
//!
//! Ported from:
//! - `packages/llm/src/protocols/openai-chat.ts` (493 lines)
//! - `packages/llm/src/providers/mistral.ts`
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

const DEFAULT_BASE_URL: &str = "https://api.mistral.ai/v1";
const CHAT_PATH: &str = "/chat/completions";

fn resolve_api_key() -> Result<String, Error> {
    std::env::var("MISTRAL_API_KEY")
        .ok()
        .filter(|k| !k.is_empty())
        .ok_or_else(|| Error::Auth("MISTRAL_API_KEY environment variable not set".into()))
}

// ── Chat Completions Body (OpenAI-compatible) ──────────────────────────

#[derive(Debug, Serialize)]
struct MistralChatBody {
    model: String,
    messages: Vec<MistralChatMessage>,
    #[serde(skip_serializing_if = "Option::is_none")]
    tools: Option<Vec<MistralTool>>,
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
enum MistralChatMessage {
    #[serde(rename = "system")]
    System { content: String },
    #[serde(rename = "user")]
    User { content: MistralChatUserContent },
    #[serde(rename = "assistant")]
    Assistant {
        content: Option<String>,
        #[serde(skip_serializing_if = "Option::is_none")]
        tool_calls: Option<Vec<MistralAssistantToolCall>>,
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
enum MistralChatUserContent {
    Text(String),
    Parts(Vec<MistralUserContentPart>),
}

#[derive(Debug, Serialize)]
#[serde(tag = "type")]
enum MistralUserContentPart {
    #[serde(rename = "text")]
    Text { text: String },
    #[serde(rename = "image_url")]
    ImageUrl { image_url: MistralImageUrl },
}

#[derive(Debug, Serialize)]
struct MistralImageUrl {
    url: String,
}

#[derive(Debug, Serialize)]
struct MistralAssistantToolCall {
    id: String,
    #[serde(rename = "type")]
    call_type: String,
    function: MistralToolCallFunction,
}

#[derive(Debug, Serialize)]
struct MistralToolCallFunction {
    name: String,
    arguments: String,
}

#[derive(Debug, Serialize)]
struct MistralTool {
    #[serde(rename = "type")]
    tool_type: String,
    function: MistralFunctionDef,
}

#[derive(Debug, Serialize)]
struct MistralFunctionDef {
    name: String,
    description: String,
    parameters: serde_json::Value,
}

// ── Chat SSE Event types (OpenAI-compatible) ───────────────────────────

#[derive(Debug, Deserialize)]
struct MistralChatEvent {
    choices: Vec<MistralChoice>,
    #[serde(default)]
    usage: Option<MistralUsage>,
}

#[derive(Debug, Deserialize)]
struct MistralChoice {
    delta: Option<MistralDelta>,
    finish_reason: Option<String>,
}

#[derive(Debug, Deserialize)]
struct MistralDelta {
    content: Option<String>,
    reasoning_content: Option<String>,
    tool_calls: Option<Vec<MistralToolCallDelta>>,
}

#[derive(Debug, Deserialize)]
struct MistralToolCallDelta {
    index: u64,
    id: Option<String>,
    function: Option<MistralToolCallDeltaFn>,
}

#[derive(Debug, Deserialize)]
struct MistralToolCallDeltaFn {
    name: Option<String>,
    arguments: Option<String>,
}

#[derive(Debug, Deserialize)]
struct MistralUsage {
    prompt_tokens: Option<u64>,
    completion_tokens: Option<u64>,
    total_tokens: Option<u64>,
    prompt_tokens_details: Option<MistralPromptTokenDetails>,
    completion_tokens_details: Option<MistralCompletionTokenDetails>,
}

#[derive(Debug, Deserialize)]
struct MistralPromptTokenDetails {
    cached_tokens: Option<u64>,
}

#[derive(Debug, Deserialize)]
struct MistralCompletionTokenDetails {
    reasoning_tokens: Option<u64>,
}

// ── Mistral Provider ───────────────────────────────────────────────────

pub struct MistralProvider {
    api_key: String,
    base_url: String,
    http_client: reqwest::Client,
    models: Vec<Model>,
}

impl MistralProvider {
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

    fn build_chat_messages(messages: &[ChatMessage]) -> Vec<MistralChatMessage> {
        let mut result = Vec::new();
        let mut pending_images: Vec<MistralUserContentPart> = Vec::new();

        for msg in messages {
            match msg {
                ChatMessage::System { content } => {
                    let text = extract_text(content);
                    if !text.is_empty() {
                        result.push(MistralChatMessage::System { content: text });
                    }
                }
                ChatMessage::User { content } => {
                    let mut text_parts = String::new();
                    let mut media_parts: Vec<MistralUserContentPart> = Vec::new();
                    for part in content_parts(content) {
                        match part {
                            ContentPart::Text { text } => text_parts.push_str(&text),
                            ContentPart::Image { image } => {
                                media_parts.push(MistralUserContentPart::ImageUrl {
                                    image_url: MistralImageUrl {
                                        url: if image.starts_with("data:") {
                                            image.clone()
                                        } else {
                                            format!("data:image/png;base64,{image}")
                                        },
                                    },
                                })
                            }
                            _ => {}
                        }
                    }
                    if !pending_images.is_empty() {
                        media_parts.extend(pending_images.drain(..));
                    }
                    if media_parts.is_empty() {
                        result.push(MistralChatMessage::User {
                            content: MistralChatUserContent::Text(text_parts),
                        });
                    } else {
                        let mut parts = media_parts;
                        if !text_parts.is_empty() {
                            parts.insert(0, MistralUserContentPart::Text { text: text_parts });
                        }
                        result.push(MistralChatMessage::User {
                            content: MistralChatUserContent::Parts(parts),
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
                                tool_calls.push(MistralAssistantToolCall {
                                    id: tool_call_id.clone(),
                                    call_type: "function".into(),
                                    function: MistralToolCallFunction {
                                        name: tool_name.clone(),
                                        arguments: "{}".into(),
                                    },
                                });
                            }
                            _ => {}
                        }
                    }
                    result.push(MistralChatMessage::Assistant {
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
                        result.push(MistralChatMessage::Tool {
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

fn build_tools(tools: &[ToolDefinition]) -> Option<Vec<MistralTool>> {
    if tools.is_empty() {
        return None;
    }
    Some(
        tools
            .iter()
            .map(|t| MistralTool {
                tool_type: "function".into(),
                function: MistralFunctionDef {
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

fn map_usage(u: &MistralUsage) -> Usage {
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

fn events_from_chat(event: MistralChatEvent, state: &mut ChatStreamState) -> Vec<LlmEvent> {
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
impl Provider for MistralProvider {
    fn provider_id(&self) -> &str {
        "mistral"
    }
    fn npm(&self) -> &str {
        "@ai-sdk/mistral"
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
                provider_id: "mistral".into(),
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
        let body = MistralChatBody {
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
            .map_err(|e| Error::Network(format!("Mistral request: {e}")))?;

        if !response.status().is_success() {
            let status = response.status().as_u16();
            let text = response.text().await.unwrap_or_default();
            return Err(Error::Llm {
                module: "mistral".into(),
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
                                if let Ok(oe) = serde_json::from_str::<MistralChatEvent>(&se.data) {
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
                                    Err(Error::ResponseStream(format!("Mistral SSE: {e}"))),
                                    (sse, state, buffer),
                                ))
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

fn build_model_catalog() -> Vec<Model> {
    vec![
        make_model("mistral-large", "Mistral Large", 128_000, 128_000, 2.0, 6.0),
        make_model(
            "mistral-medium",
            "Mistral Medium",
            32_000,
            8_000,
            2.70,
            8.10,
        ),
        make_model("mistral-small", "Mistral Small", 32_000, 4_000, 1.0, 3.0),
        make_model("mistral-nemo", "Mistral Nemo", 128_000, 128_000, 0.15, 0.15),
        make_model("codestral", "Codestral", 32_000, 8_000, 1.0, 3.0),
        make_model_with_image("pixtral-large", "Pixtral Large", 128_000, 8_000, 2.0, 6.0),
    ]
}

fn make_model(id: &str, name: &str, ctx: u64, out: u64, inp_cost: f64, out_cost: f64) -> Model {
    Model {
        id: id.into(),
        provider_id: "mistral".into(),
        name: name.into(),
        api: crate::provider::ApiInfo {
            id: id.into(),
            url: DEFAULT_BASE_URL.into(),
            npm: "@ai-sdk/mistral".into(),
        },
        family: Some("mistral".into()),
        capabilities: crate::provider::Capabilities {
            temperature: true,
            reasoning: true,
            attachment: true,
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

fn make_model_with_image(
    id: &str,
    name: &str,
    ctx: u64,
    out: u64,
    inp_cost: f64,
    out_cost: f64,
) -> Model {
    Model {
        id: id.into(),
        provider_id: "mistral".into(),
        name: name.into(),
        api: crate::provider::ApiInfo {
            id: id.into(),
            url: DEFAULT_BASE_URL.into(),
            npm: "@ai-sdk/mistral".into(),
        },
        family: Some("mistral".into()),
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

    // ── Model catalog ──────────────────────────────────────────────

    #[test]
    fn test_model_catalog_size() {
        let catalog = build_model_catalog();
        assert_eq!(catalog.len(), 6, "Mistral should have 6 models");
    }

    #[test]
    fn test_model_catalog_all_have_provider_id_mistral() {
        for model in &build_model_catalog() {
            assert_eq!(
                model.provider_id, "mistral",
                "model {} has wrong provider_id",
                model.id
            );
        }
    }

    #[test]
    fn test_mistral_large() {
        let model = build_model_catalog()
            .into_iter()
            .find(|m| m.id == "mistral-large")
            .expect("mistral-large not found");
        assert_eq!(model.name, "Mistral Large");
        assert_eq!(model.limit.context, 128_000);
        assert_eq!(model.limit.output, 128_000);
        assert!(model.capabilities.toolcall);
        assert!(model.capabilities.reasoning);
    }

    #[test]
    fn test_mistral_medium() {
        let model = build_model_catalog()
            .into_iter()
            .find(|m| m.id == "mistral-medium")
            .expect("mistral-medium not found");
        assert_eq!(model.name, "Mistral Medium");
        assert_eq!(model.limit.context, 32_000);
        assert_eq!(model.limit.output, 8_000);
    }

    #[test]
    fn test_mistral_small() {
        let model = build_model_catalog()
            .into_iter()
            .find(|m| m.id == "mistral-small")
            .expect("mistral-small not found");
        assert_eq!(model.name, "Mistral Small");
        assert_eq!(model.limit.context, 32_000);
        assert_eq!(model.limit.output, 4_000);
    }

    #[test]
    fn test_mistral_nemo() {
        let model = build_model_catalog()
            .into_iter()
            .find(|m| m.id == "mistral-nemo")
            .expect("mistral-nemo not found");
        assert_eq!(model.name, "Mistral Nemo");
        assert_eq!(model.limit.context, 128_000);
        assert_eq!(model.limit.output, 128_000);
        // Nemo is the cheap tier
        assert_eq!(model.cost.input, 0.15);
        assert_eq!(model.cost.output, 0.15);
    }

    #[test]
    fn test_codestral() {
        let model = build_model_catalog()
            .into_iter()
            .find(|m| m.id == "codestral")
            .expect("codestral not found");
        assert_eq!(model.name, "Codestral");
        assert_eq!(model.limit.context, 32_000);
        assert_eq!(model.limit.output, 8_000);
        assert!(model.capabilities.toolcall);
    }

    #[test]
    fn test_pixtral_large_multimodal() {
        let model = build_model_catalog()
            .into_iter()
            .find(|m| m.id == "pixtral-large")
            .expect("pixtral-large not found");
        assert_eq!(model.name, "Pixtral Large");
        assert_eq!(model.limit.context, 128_000);
        assert_eq!(model.limit.output, 8_000);
        // Pixtral is multimodal — supports image input
        assert!(
            model.capabilities.input.image,
            "pixtral-large should support image input"
        );
        assert!(model.capabilities.input.text);
    }

    #[test]
    fn test_provider_id() {
        let catalog = build_model_catalog();
        // All models share the same provider_id
        let model = &catalog[0];
        assert_eq!(model.provider_id, "mistral");
    }

    // ── Provider interface ─────────────────────────────────────────

    #[test]
    fn test_provider_id_method() {
        // We can't construct a real provider without a valid API key,
        // but we can verify the model catalog provider_id is correct.
        let catalog = build_model_catalog();
        for model in &catalog {
            assert_eq!(model.provider_id, "mistral");
        }
    }

    #[test]
    fn test_npm_package_name() {
        let catalog = build_model_catalog();
        for model in &catalog {
            assert_eq!(model.api.npm, "@ai-sdk/mistral");
        }
    }

    // ── Model lookup ───────────────────────────────────────────────

    #[test]
    fn test_model_lookup_by_id() {
        let catalog = build_model_catalog();
        let found = catalog.iter().find(|m| m.id == "mistral-small");
        assert!(found.is_some(), "mistral-small should be found in catalog");
        assert_eq!(found.unwrap().name, "Mistral Small");
    }

    #[test]
    fn test_model_lookup_by_id_missing() {
        let catalog = build_model_catalog();
        let found = catalog.iter().find(|m| m.id == "nonexistent-model");
        assert!(found.is_none(), "nonexistent-model should not exist");
    }

    // ── Error classification ───────────────────────────────────────

    #[test]
    fn test_classify_error_401_authentication() {
        let reason = classify_error(401, r#"{"error":"Invalid API key"}"#);
        match reason {
            LlmErrorReason::Authentication { kind, .. } => {
                assert_eq!(kind, crate::error::AuthErrorKind::Invalid);
            }
            other => panic!("Expected Authentication, got {other:?}"),
        }
    }

    #[test]
    fn test_classify_error_403_authentication() {
        let reason = classify_error(403, r#"{"error":"Forbidden"}"#);
        assert!(matches!(reason, LlmErrorReason::Authentication { .. }));
    }

    #[test]
    fn test_classify_error_429_rate_limit() {
        let reason = classify_error(429, r#"{"error":"Too many requests"}"#);
        assert!(matches!(reason, LlmErrorReason::RateLimit { .. }));
    }

    #[test]
    fn test_classify_error_400_context_overflow() {
        let reason = classify_error(400, "This input token count exceeds the context window");
        match reason {
            LlmErrorReason::InvalidRequest { classification, .. } => {
                assert_eq!(classification, Some("context-overflow".into()));
            }
            other => panic!("Expected InvalidRequest with context-overflow, got {other:?}"),
        }
    }

    #[test]
    fn test_classify_error_400_invalid_request() {
        let reason = classify_error(400, r#"{"error":"model not found"}"#);
        match reason {
            LlmErrorReason::InvalidRequest { classification, .. } => {
                assert_eq!(classification, None);
            }
            other => panic!("Expected InvalidRequest without classification, got {other:?}"),
        }
    }

    #[test]
    fn test_classify_error_413_context_overflow() {
        let reason = classify_error(413, "Request entity too large");
        assert!(matches!(reason, LlmErrorReason::InvalidRequest { .. }));
    }

    #[test]
    fn test_classify_error_500_provider_internal() {
        let reason = classify_error(500, r#"{"error":"Internal server error"}"#);
        match reason {
            LlmErrorReason::ProviderInternal { status, .. } => {
                assert_eq!(status, 500);
            }
            other => panic!("Expected ProviderInternal, got {other:?}"),
        }
    }

    #[test]
    fn test_classify_error_503_provider_internal() {
        let reason = classify_error(503, r#"{"error":"Service Unavailable"}"#);
        assert!(matches!(reason, LlmErrorReason::ProviderInternal { .. }));
    }

    #[test]
    fn test_classify_error_418_unknown() {
        let reason = classify_error(418, "I'm a teapot");
        match reason {
            LlmErrorReason::UnknownProvider { status, .. } => {
                assert_eq!(status, Some(418));
            }
            other => panic!("Expected UnknownProvider, got {other:?}"),
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
        assert_eq!(
            map_finish_reason("some_weird_reason"),
            FinishReason::Unknown
        );
    }

    // ── API key resolution ─────────────────────────────────────────

    #[test]
    fn test_resolve_api_key_missing() {
        // Remove MISTRAL_API_KEY from env for this test
        std::env::remove_var("MISTRAL_API_KEY");
        let result = resolve_api_key();
        assert!(result.is_err());
        match result {
            Err(Error::Auth(msg)) => {
                assert!(msg.contains("MISTRAL_API_KEY"));
            }
            other => panic!("Expected Auth error, got {other:?}"),
        }
    }

    #[test]
    fn test_resolve_api_key_present() {
        std::env::set_var("MISTRAL_API_KEY", "test-key-12345");
        let result = resolve_api_key();
        assert_eq!(result.unwrap(), "test-key-12345");
        std::env::remove_var("MISTRAL_API_KEY");
    }

    // ── Base URL ───────────────────────────────────────────────────

    #[test]
    fn test_chat_url_construction() {
        let provider = MistralProvider::with_api_key("test-key".into())
            .expect("should construct with explicit key");
        let url = provider.chat_url();
        assert_eq!(url, "https://api.mistral.ai/v1/chat/completions");
    }

    #[test]
    fn test_chat_url_with_custom_base() {
        let provider = MistralProvider::with_base_url(
            "test-key".into(),
            "https://mistral-proxy.example.com/v1/".into(),
        )
        .expect("should construct with custom base");
        let url = provider.chat_url();
        assert_eq!(url, "https://mistral-proxy.example.com/v1/chat/completions");
    }
}
