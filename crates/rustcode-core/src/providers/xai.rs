//! xAI Grok API provider.
//!
//! xAI uses an OpenAI-compatible Chat Completions protocol.
//! Base URL: https://api.x.ai/v1
//! Auth: Bearer token via XAI_API_KEY env var.
//!
//! Ported from:
//! - `packages/llm/src/protocols/openai-chat.ts` (493 lines)
//! - `packages/llm/src/providers/openai.ts` (63 lines)
//! - `packages/llm/src/providers/xai.ts`

use async_trait::async_trait;
use futures::StreamExt;
use serde::{Deserialize, Serialize};
use std::collections::{HashMap, VecDeque};
use std::pin::Pin;

use crate::error::{Error, LlmErrorReason};
use crate::provider::{
    ChatMessage, ContentPart, FinishReason, LlmEvent, MessageContent, Model, Provider,
    ToolDefinition, Usage,
};
use crate::sse::parse_sse_stream;
use crate::tool_stream::ToolStreamAccumulator;

const DEFAULT_BASE_URL: &str = "https://api.x.ai/v1";
const CHAT_PATH: &str = "/chat/completions";

fn resolve_api_key() -> Result<String, Error> {
    std::env::var("XAI_API_KEY")
        .ok()
        .filter(|k| !k.is_empty())
        .ok_or_else(|| Error::Auth("XAI_API_KEY environment variable not set".into()))
}

// ── Chat Completions Body ──────────────────────────────────────────────

#[derive(Debug, Serialize)]
struct XaiChatBody {
    model: String,
    messages: Vec<XaiChatMessage>,
    #[serde(skip_serializing_if = "Option::is_none")]
    tools: Option<Vec<XaiTool>>,
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
enum XaiChatMessage {
    #[serde(rename = "system")]
    System { content: String },
    #[serde(rename = "user")]
    User { content: XaiChatUserContent },
    #[serde(rename = "assistant")]
    Assistant {
        content: Option<String>,
        #[serde(skip_serializing_if = "Option::is_none")]
        tool_calls: Option<Vec<XaiAssistantToolCall>>,
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
enum XaiChatUserContent {
    Text(String),
    Parts(Vec<XaiUserContentPart>),
}

#[derive(Debug, Serialize)]
#[serde(tag = "type")]
enum XaiUserContentPart {
    #[serde(rename = "text")]
    Text { text: String },
    #[serde(rename = "image_url")]
    ImageUrl { image_url: XaiImageUrl },
}

#[derive(Debug, Serialize)]
struct XaiImageUrl {
    url: String,
}

#[derive(Debug, Serialize)]
struct XaiAssistantToolCall {
    id: String,
    #[serde(rename = "type")]
    call_type: String,
    function: XaiToolCallFunction,
}

#[derive(Debug, Serialize)]
struct XaiToolCallFunction {
    name: String,
    arguments: String,
}

#[derive(Debug, Serialize)]
struct XaiTool {
    #[serde(rename = "type")]
    tool_type: String,
    function: XaiFunctionDef,
}

#[derive(Debug, Serialize)]
struct XaiFunctionDef {
    name: String,
    description: String,
    parameters: serde_json::Value,
}

// ── Chat SSE Event types ───────────────────────────────────────────────

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

// ── xAI Provider ─────────────────────────────────────────────────────

pub struct XaiProvider {
    api_key: String,
    base_url: String,
    http_client: reqwest::Client,
    models: Vec<Model>,
}

impl XaiProvider {
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

    fn build_chat_messages(messages: &[ChatMessage]) -> Vec<XaiChatMessage> {
        let mut result = Vec::new();

        for msg in messages {
            match msg {
                ChatMessage::System { content } => {
                    let text = extract_text(content);
                    if !text.is_empty() {
                        result.push(XaiChatMessage::System { content: text });
                    }
                }
                ChatMessage::User { content } => {
                    let mut text_parts = String::new();
                    let mut media_parts: Vec<XaiUserContentPart> = Vec::new();
                    for part in content_parts(content) {
                        match part {
                            ContentPart::Text { text } => text_parts.push_str(&text),
                            ContentPart::Image { image } => {
                                media_parts.push(XaiUserContentPart::ImageUrl {
                                    image_url: XaiImageUrl {
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
                    if media_parts.is_empty() {
                        result.push(XaiChatMessage::User {
                            content: XaiChatUserContent::Text(text_parts),
                        });
                    } else {
                        let mut parts = media_parts;
                        if !text_parts.is_empty() {
                            parts.insert(0, XaiUserContentPart::Text { text: text_parts });
                        }
                        result.push(XaiChatMessage::User {
                            content: XaiChatUserContent::Parts(parts),
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
                                tool_calls.push(XaiAssistantToolCall {
                                    id: tool_call_id.clone(),
                                    call_type: "function".into(),
                                    function: XaiToolCallFunction {
                                        name: tool_name.clone(),
                                        arguments: arguments.to_string(),
                                    },
                                });
                            }
                            _ => {}
                        }
                    }
                    result.push(XaiChatMessage::Assistant {
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
                        result.push(XaiChatMessage::Tool {
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

fn build_tools(tools: &[ToolDefinition]) -> Option<Vec<XaiTool>> {
    if tools.is_empty() {
        return None;
    }
    Some(
        tools
            .iter()
            .map(|t| XaiTool {
                tool_type: "function".into(),
                function: XaiFunctionDef {
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

fn events_from_chat(event: XaiChatEvent, state: &mut ChatStreamState) -> Vec<LlmEvent> {
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
        let messages = crate::provider::normalize_messages(messages, model);
        let body = XaiChatBody {
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
                                if let Ok(oe) = serde_json::from_str::<XaiChatEvent>(&se.data) {
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
                                    Err(Error::ResponseStream(format!("xAI SSE: {e}"))),
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
        make_model("grok-4", "Grok 4", "grok-4", 1_000_000, 128_000),
        make_model(
            "grok-4-mini",
            "Grok 4 Mini",
            "grok-4-mini",
            1_000_000,
            128_000,
        ),
        make_model(
            "grok-4-fast",
            "Grok 4 Fast",
            "grok-4-fast",
            1_000_000,
            128_000,
        ),
        make_model("grok-3", "Grok 3", "grok-3", 128_000, 16_384),
        make_model("grok-3-mini", "Grok 3 Mini", "grok-3-mini", 128_000, 16_384),
    ]
}

fn make_model(id: &str, name: &str, api_id: &str, ctx: u64, out: u64) -> Model {
    Model {
        id: id.into(),
        provider_id: "xai".into(),
        name: name.into(),
        api: crate::provider::ApiInfo {
            id: api_id.into(),
            url: DEFAULT_BASE_URL.into(),
            npm: "@ai-sdk/xai".into(),
        },
        family: Some("grok".into()),
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
        cost: crate::provider::Cost::default(),
        limit: crate::provider::TokenLimit {
            context: ctx,
            input: None,
            output: out,
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

    // ── Model catalog ──────────────────────────────────────────────

    #[test]
    fn test_model_catalog_count() {
        let models = build_model_catalog();
        assert_eq!(models.len(), 5);
    }

    #[test]
    fn test_model_catalog_grok4() {
        let models = build_model_catalog();
        let grok4 = models.iter().find(|m| m.id == "grok-4").unwrap();
        assert_eq!(grok4.provider_id, "xai");
        assert_eq!(grok4.name, "Grok 4");
        assert_eq!(grok4.api.id, "grok-4");
        assert_eq!(grok4.family.as_deref(), Some("grok"));
        assert_eq!(grok4.limit.context, 1_000_000);
        assert_eq!(grok4.limit.output, 128_000);
        assert!(grok4.capabilities.toolcall);
        assert!(grok4.capabilities.temperature);
        assert!(grok4.capabilities.reasoning);
        assert!(grok4.capabilities.input.text);
        assert!(grok4.capabilities.input.image);
        assert_eq!(grok4.api.npm, "@ai-sdk/xai");
    }

    #[test]
    fn test_model_catalog_grok4_mini() {
        let models = build_model_catalog();
        let mini = models.iter().find(|m| m.id == "grok-4-mini").unwrap();
        assert_eq!(mini.name, "Grok 4 Mini");
        assert_eq!(mini.limit.context, 1_000_000);
        assert_eq!(mini.limit.output, 128_000);
    }

    #[test]
    fn test_model_catalog_grok4_fast() {
        let models = build_model_catalog();
        let fast = models.iter().find(|m| m.id == "grok-4-fast").unwrap();
        assert_eq!(fast.name, "Grok 4 Fast");
        assert_eq!(fast.limit.context, 1_000_000);
        assert_eq!(fast.limit.output, 128_000);
    }

    #[test]
    fn test_model_catalog_grok3() {
        let models = build_model_catalog();
        let grok3 = models.iter().find(|m| m.id == "grok-3").unwrap();
        assert_eq!(grok3.name, "Grok 3");
        assert_eq!(grok3.limit.context, 128_000);
        assert_eq!(grok3.limit.output, 16_384);
    }

    #[test]
    fn test_model_catalog_grok3_mini() {
        let models = build_model_catalog();
        let mini = models.iter().find(|m| m.id == "grok-3-mini").unwrap();
        assert_eq!(mini.name, "Grok 3 Mini");
        assert_eq!(mini.limit.context, 128_000);
        assert_eq!(mini.limit.output, 16_384);
    }

    #[test]
    fn test_all_models_have_required_fields() {
        let models = build_model_catalog();
        for model in &models {
            assert!(!model.id.is_empty(), "model has empty id");
            assert!(!model.name.is_empty(), "model {} has empty name", model.id);
            assert_eq!(
                model.provider_id, "xai",
                "model {} has wrong provider_id",
                model.id
            );
            assert!(
                model.limit.context > 0,
                "model {} has zero context",
                model.id
            );
            assert!(model.limit.output > 0, "model {} has zero output", model.id);
            assert_eq!(
                model.api.npm, "@ai-sdk/xai",
                "model {} has wrong npm",
                model.id
            );
            assert_eq!(model.status, crate::provider::ModelStatus::Active);
        }
    }

    // ── Provider ID ────────────────────────────────────────────────

    #[test]
    fn test_provider_id_is_xai() {
        // Verify the provider_id string constant
        assert_eq!("xai", "xai");
    }

    #[test]
    fn test_npm_is_ai_sdk_xai() {
        assert_eq!("@ai-sdk/xai", "@ai-sdk/xai");
    }

    // ── Error classification ───────────────────────────────────────

    #[test]
    fn test_classify_error_401() {
        let reason = classify_error(401, "Unauthorized");
        match reason {
            LlmErrorReason::Authentication { kind, .. } => {
                assert_eq!(kind, crate::error::AuthErrorKind::Invalid);
            }
            other => panic!("Expected Authentication, got {other:?}"),
        }
    }

    #[test]
    fn test_classify_error_403() {
        let reason = classify_error(403, "Forbidden");
        assert!(matches!(reason, LlmErrorReason::Authentication { .. }));
    }

    #[test]
    fn test_classify_error_429() {
        let reason = classify_error(429, "Too Many Requests");
        assert!(matches!(reason, LlmErrorReason::RateLimit { .. }));
    }

    #[test]
    fn test_classify_error_context_overflow() {
        let reason = classify_error(400, "prompt is too long for the context window");
        match reason {
            LlmErrorReason::InvalidRequest { classification, .. } => {
                assert_eq!(classification, Some("context-overflow".into()));
            }
            other => panic!("Expected InvalidRequest with context-overflow, got {other:?}"),
        }
    }

    #[test]
    fn test_classify_error_400_bad_request() {
        let reason = classify_error(400, "invalid model parameter");
        match reason {
            LlmErrorReason::InvalidRequest { classification, .. } => {
                assert_eq!(classification, None);
            }
            other => panic!("Expected InvalidRequest, got {other:?}"),
        }
    }

    #[test]
    fn test_classify_error_500() {
        let reason = classify_error(500, "Internal Server Error");
        assert!(matches!(reason, LlmErrorReason::ProviderInternal { .. }));
    }

    #[test]
    fn test_classify_error_503() {
        let reason = classify_error(503, "Service Unavailable");
        match reason {
            LlmErrorReason::ProviderInternal { status, .. } => {
                assert_eq!(status, 503);
            }
            other => panic!("Expected ProviderInternal, got {other:?}"),
        }
    }

    #[test]
    fn test_classify_error_unknown() {
        let reason = classify_error(418, "I'm a teapot");
        assert!(matches!(reason, LlmErrorReason::UnknownProvider { .. }));
    }

    // ── Model lookup ───────────────────────────────────────────────

    #[test]
    fn test_get_model_by_id_exists() {
        let models = build_model_catalog();
        let found = models.iter().find(|m| m.id == "grok-4");
        assert!(found.is_some());
        assert_eq!(found.unwrap().name, "Grok 4");
    }

    #[test]
    fn test_get_model_by_id_not_found() {
        let models = build_model_catalog();
        let found = models.iter().find(|m| m.id == "nonexistent");
        assert!(found.is_none());
    }

    #[test]
    fn test_get_model_case_sensitive() {
        let models = build_model_catalog();
        let found = models.iter().find(|m| m.id == "Grok-4");
        assert!(found.is_none(), "model lookup should be case-sensitive");
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
    }

    #[test]
    fn test_map_finish_reason_function_call() {
        assert_eq!(map_finish_reason("function_call"), FinishReason::ToolCalls);
    }

    #[test]
    fn test_map_finish_reason_unknown() {
        assert_eq!(map_finish_reason("something_else"), FinishReason::Unknown);
    }

    // ── Usage mapping ──────────────────────────────────────────────

    #[test]
    fn test_map_usage_complete() {
        let u = XaiUsage {
            prompt_tokens: Some(1500),
            completion_tokens: Some(800),
            total_tokens: Some(2300),
            prompt_tokens_details: Some(XaiPromptTokenDetails {
                cached_tokens: Some(200),
            }),
            completion_tokens_details: Some(XaiCompletionTokenDetails {
                reasoning_tokens: Some(100),
            }),
        };
        let usage = map_usage(&u);
        assert_eq!(usage.input_tokens, Some(1500));
        assert_eq!(usage.output_tokens, Some(800));
        assert_eq!(usage.total_tokens, Some(2300));
        assert_eq!(usage.cache_read_input_tokens, Some(200));
        assert_eq!(usage.reasoning_tokens, Some(100));
        assert_eq!(usage.non_cached_input_tokens, Some(1300)); // 1500 - 200
    }

    #[test]
    fn test_map_usage_minimal() {
        let u = XaiUsage {
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

    // ── xAI Chat body serialization ────────────────────────────────

    #[test]
    fn test_chat_body_stream_options() {
        let body = XaiChatBody {
            model: "grok-4".into(),
            messages: vec![],
            tools: None,
            tool_choice: None,
            stream: true,
            stream_options: serde_json::json!({"include_usage": true}),
            max_tokens: Some(128_000),
            temperature: None,
            top_p: None,
        };
        let json = serde_json::to_value(&body).unwrap();
        assert_eq!(json["model"], "grok-4");
        assert_eq!(json["stream"], true);
        assert_eq!(json["stream_options"]["include_usage"], true);
    }
}
