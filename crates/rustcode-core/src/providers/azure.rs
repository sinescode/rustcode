//! Microsoft Azure OpenAI provider — OpenAI-compatible Chat Completions protocol.
//!
//! Azure OpenAI speaks the same OpenAI Chat Completions wire format with SSE
//! streaming, so this provider reuses the same body builder and SSE event parser
//! patterns as the OpenAI provider, with Azure-specific:
//! - Base URL constructed from env: `{endpoint}/openai/deployments/{deployment}`
//! - Auth: `api-key` header via `AZURE_OPENAI_API_KEY` env var
//! - Deployment via `AZURE_OPENAI_DEPLOYMENT` env var
//! - Endpoint via `AZURE_OPENAI_ENDPOINT` env var
//! - API version: `2025-01-01-preview`
//! - Model catalog: gpt-5.2, gpt-5.2-mini, gpt-5.1
//!
//! Ported from:
//! - `packages/llm/src/protocols/openai-chat.ts` (493 lines)
//! - `packages/llm/src/providers/openai-compatible.ts` (66 lines)
//! - `packages/llm/src/providers/openai-compatible-profile.ts` (21 lines)

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

const CHAT_PATH: &str = "/chat/completions?api-version=2025-01-01-preview";

fn resolve_config() -> Result<(String, String, String), Error> {
    let api_key = std::env::var("AZURE_OPENAI_API_KEY")
        .ok()
        .filter(|k| !k.is_empty())
        .ok_or_else(|| Error::Auth("AZURE_OPENAI_API_KEY environment variable not set".into()))?;
    let endpoint = std::env::var("AZURE_OPENAI_ENDPOINT")
        .ok()
        .filter(|k| !k.is_empty())
        .ok_or_else(|| Error::Auth("AZURE_OPENAI_ENDPOINT environment variable not set".into()))?;
    let deployment = std::env::var("AZURE_OPENAI_DEPLOYMENT")
        .ok()
        .filter(|k| !k.is_empty())
        .ok_or_else(|| {
            Error::Auth("AZURE_OPENAI_DEPLOYMENT environment variable not set".into())
        })?;
    Ok((api_key, endpoint, deployment))
}

// ── Chat Completions Body ──────────────────────────────────────────────

#[derive(Debug, Serialize)]
struct AzureChatBody {
    model: String,
    messages: Vec<AzureChatMessage>,
    #[serde(skip_serializing_if = "Option::is_none")]
    tools: Option<Vec<AzureTool>>,
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
enum AzureChatMessage {
    #[serde(rename = "system")]
    System { content: String },
    #[serde(rename = "user")]
    User { content: AzureUserContent },
    #[serde(rename = "assistant")]
    Assistant {
        content: Option<String>,
        #[serde(skip_serializing_if = "Option::is_none")]
        tool_calls: Option<Vec<AzureAssistantToolCall>>,
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
enum AzureUserContent {
    Text(String),
    Parts(Vec<AzureUserContentPart>),
}

#[derive(Debug, Serialize)]
#[serde(tag = "type")]
enum AzureUserContentPart {
    #[serde(rename = "text")]
    Text { text: String },
    #[serde(rename = "image_url")]
    ImageUrl { image_url: AzureImageUrl },
}

#[derive(Debug, Serialize)]
struct AzureImageUrl {
    url: String,
}

#[derive(Debug, Serialize)]
struct AzureAssistantToolCall {
    id: String,
    #[serde(rename = "type")]
    call_type: String,
    function: AzureToolCallFunction,
}

#[derive(Debug, Serialize)]
struct AzureToolCallFunction {
    name: String,
    arguments: String,
}

#[derive(Debug, Serialize)]
struct AzureTool {
    #[serde(rename = "type")]
    tool_type: String,
    function: AzureFunctionDef,
}

#[derive(Debug, Serialize)]
struct AzureFunctionDef {
    name: String,
    description: String,
    parameters: serde_json::Value,
}

// ── Chat SSE Event types ───────────────────────────────────────────────

#[derive(Debug, Deserialize)]
struct AzureChatEvent {
    choices: Vec<AzureChoice>,
    #[serde(default)]
    usage: Option<AzureUsage>,
}

#[derive(Debug, Deserialize)]
struct AzureChoice {
    delta: Option<AzureDelta>,
    finish_reason: Option<String>,
}

#[derive(Debug, Deserialize)]
struct AzureDelta {
    content: Option<String>,
    reasoning_content: Option<String>,
    tool_calls: Option<Vec<AzureToolCallDelta>>,
}

#[derive(Debug, Deserialize)]
struct AzureToolCallDelta {
    index: u64,
    id: Option<String>,
    function: Option<AzureToolCallDeltaFn>,
}

#[derive(Debug, Deserialize)]
struct AzureToolCallDeltaFn {
    name: Option<String>,
    arguments: Option<String>,
}

#[derive(Debug, Deserialize)]
struct AzureUsage {
    prompt_tokens: Option<u64>,
    completion_tokens: Option<u64>,
    total_tokens: Option<u64>,
    prompt_tokens_details: Option<AzurePromptTokenDetails>,
    completion_tokens_details: Option<AzureCompletionTokenDetails>,
}

#[derive(Debug, Deserialize)]
struct AzurePromptTokenDetails {
    cached_tokens: Option<u64>,
}

#[derive(Debug, Deserialize)]
struct AzureCompletionTokenDetails {
    reasoning_tokens: Option<u64>,
}

// ── Azure Provider ─────────────────────────────────────────────────────

#[derive(Debug)]
pub struct AzureProvider {
    api_key: String,
    base_url: String,
    http_client: reqwest::Client,
    models: Vec<Model>,
}

impl AzureProvider {
    /// Create a new Azure OpenAI provider from env vars:
    /// `AZURE_OPENAI_API_KEY`, `AZURE_OPENAI_ENDPOINT`, `AZURE_OPENAI_DEPLOYMENT`.
    pub fn new() -> Result<Self, Error> {
        let (api_key, endpoint, deployment) = resolve_config()?;
        Self::with_config(api_key, endpoint, deployment)
    }

    /// Create with explicit configuration values.
    /// Constructs the base URL as `{endpoint}/openai/deployments/{deployment}`.
    pub fn with_config(
        api_key: String,
        endpoint: String,
        deployment: String,
    ) -> Result<Self, Error> {
        let base_url = format!(
            "{}/openai/deployments/{}",
            endpoint.trim_end_matches('/'),
            deployment
        );
        let http_client = reqwest::Client::builder()
            .user_agent(format!("rustcode/{}", env!("CARGO_PKG_VERSION")))
            .timeout(std::time::Duration::from_secs(300))
            .build()
            .map_err(|e| Error::Network(format!("HTTP client: {e}")))?;
        Ok(Self {
            api_key,
            base_url: base_url.clone(),
            http_client,
            models: build_model_catalog(&base_url),
        })
    }

    fn chat_url(&self) -> String {
        format!("{}{CHAT_PATH}", self.base_url.trim_end_matches('/'))
    }

    fn build_chat_messages(messages: &[ChatMessage]) -> Vec<AzureChatMessage> {
        let mut result = Vec::new();
        let mut pending_images: Vec<AzureUserContentPart> = Vec::new();

        for msg in messages {
            match msg {
                ChatMessage::System { content } => {
                    let text = extract_text(content);
                    if !text.is_empty() {
                        result.push(AzureChatMessage::System { content: text });
                    }
                }
                ChatMessage::User { content } => {
                    let mut text_parts = String::new();
                    let mut media_parts: Vec<AzureUserContentPart> = Vec::new();
                    for part in content_parts(content) {
                        match part {
                            ContentPart::Text { text } => text_parts.push_str(text),
                            ContentPart::Image { image } => {
                                media_parts.push(AzureUserContentPart::ImageUrl {
                                    image_url: AzureImageUrl {
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
                        media_parts.append(&mut pending_images);
                    }
                    if media_parts.is_empty() {
                        result.push(AzureChatMessage::User {
                            content: AzureUserContent::Text(text_parts),
                        });
                    } else {
                        let mut parts = media_parts;
                        if !text_parts.is_empty() {
                            parts.insert(0, AzureUserContentPart::Text { text: text_parts });
                        }
                        result.push(AzureChatMessage::User {
                            content: AzureUserContent::Parts(parts),
                        });
                    }
                }
                ChatMessage::Assistant { content } => {
                    let mut text = String::new();
                    let mut tool_calls = Vec::new();
                    let mut reasoning = String::new();
                    for part in content_parts(content) {
                        match part {
                            ContentPart::Text { text: t } => text.push_str(t),
                            ContentPart::Reasoning { text: r, .. } => reasoning.push_str(r),
                            ContentPart::ToolCallPart {
                                tool_call_id,
                                tool_name,
                                arguments,
                            } => {
                                tool_calls.push(AzureAssistantToolCall {
                                    id: tool_call_id.clone(),
                                    call_type: "function".into(),
                                    function: AzureToolCallFunction {
                                        name: tool_name.clone(),
                                        arguments: arguments.to_string(),
                                    },
                                });
                            }
                            _ => {}
                        }
                    }
                    result.push(AzureChatMessage::Assistant {
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
                        result.push(AzureChatMessage::Tool {
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

fn build_tools(tools: &[ToolDefinition]) -> Option<Vec<AzureTool>> {
    if tools.is_empty() {
        return None;
    }
    Some(
        tools
            .iter()
            .map(|t| AzureTool {
                tool_type: "function".into(),
                function: AzureFunctionDef {
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

fn map_usage(u: &AzureUsage) -> Usage {
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

fn events_from_chat(event: AzureChatEvent, state: &mut ChatStreamState) -> Vec<LlmEvent> {
    let mut events = Vec::new();
    let usage = event.usage.as_ref().map(map_usage).or(state.usage.clone());
    let choice = event.choices.first();

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
                if let Some(name) = td.function.as_ref().and_then(|f| f.name.as_ref()) {
                    state.tool_stream.set_identity(
                        td.index,
                        name,
                        td.id.clone().unwrap_or_default(),
                    );
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

// ── Error Classification ───────────────────────────────────────────────

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

fn build_model_catalog(base_url: &str) -> Vec<Model> {
    vec![
        make_model(
            "gpt-5.2", "GPT-5.2", 128_000, 16_384, "gpt", true, true, base_url,
        ),
        make_model(
            "gpt-5.2-mini",
            "GPT-5.2 Mini",
            128_000,
            16_384,
            "gpt",
            true,
            true,
            base_url,
        ),
        make_model(
            "gpt-5.1", "GPT-5.1", 128_000, 16_384, "gpt", true, true, base_url,
        ),
    ]
}

#[allow(clippy::too_many_arguments)]
fn make_model(
    id: &str,
    name: &str,
    ctx: u64,
    out: u64,
    family: &str,
    temperature: bool,
    reasoning: bool,
    base_url: &str,
) -> Model {
    Model {
        id: id.into(),
        provider_id: "azure".into(),
        name: name.into(),
        api: crate::provider::ApiInfo {
            id: id.into(),
            url: base_url.into(),
            npm: "@ai-sdk/azure".into(),
        },
        family: Some(family.into()),
        capabilities: crate::provider::Capabilities {
            temperature,
            reasoning,
            attachment: false,
            toolcall: true,
            input: crate::provider::Modalities {
                text: true,
                ..Default::default()
            },
            output: crate::provider::Modalities {
                text: true,
                ..Default::default()
            },
            interleaved: Default::default(),
        },
        cost: crate::provider::Cost {
            input: 0.0,
            output: 0.0,
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
        release_date: "2025".into(),
        variants: None,
    }
}

// ── Provider impl ──────────────────────────────────────────────────────

#[async_trait]
impl Provider for AzureProvider {
    fn provider_id(&self) -> &str {
        "azure"
    }

    fn npm(&self) -> &str {
        "@ai-sdk/azure"
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
                provider_id: "azure".into(),
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
        let body = AzureChatBody {
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
            .header("api-key", &self.api_key)
            .header("Content-Type", "application/json")
            .json(&body)
            .send()
            .await
            .map_err(|e| Error::Network(format!("Azure request: {e}")))?;

        if !response.status().is_success() {
            let status = response.status().as_u16();
            let text = response.text().await.unwrap_or_default();
            return Err(Error::Llm {
                module: "azure".into(),
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
                                if let Ok(oe) = serde_json::from_str::<AzureChatEvent>(&se.data) {
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
                                    Err(Error::ResponseStream(format!("Azure SSE: {e}"))),
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

// ── Tests ──────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    fn test_base_url() -> String {
        "https://test.openai.azure.com/openai/deployments/gpt-5.2".into()
    }

    fn make_test_provider() -> AzureProvider {
        AzureProvider::with_config(
            "test-api-key".into(),
            "https://test.openai.azure.com".into(),
            "gpt-5.2".into(),
        )
        .expect("create test provider")
    }

    // ── Model catalog ────────────────────────────────────────────

    #[test]
    fn test_model_catalog_count() {
        let models = build_model_catalog(&test_base_url());
        assert_eq!(models.len(), 3, "expected 3 models in catalog");
    }

    #[test]
    fn test_model_catalog_ids() {
        let models = build_model_catalog(&test_base_url());
        let ids: Vec<&str> = models.iter().map(|m| m.id.as_str()).collect();
        assert!(ids.contains(&"gpt-5.2"));
        assert!(ids.contains(&"gpt-5.2-mini"));
        assert!(ids.contains(&"gpt-5.1"));
    }

    #[test]
    fn test_model_catalog_names() {
        let models = build_model_catalog(&test_base_url());
        let names: Vec<&str> = models.iter().map(|m| m.name.as_str()).collect();
        assert!(names.contains(&"GPT-5.2"));
        assert!(names.contains(&"GPT-5.2 Mini"));
        assert!(names.contains(&"GPT-5.1"));
    }

    #[test]
    fn test_model_catalog_provider_id() {
        let models = build_model_catalog(&test_base_url());
        for m in &models {
            assert_eq!(m.provider_id, "azure");
        }
    }

    #[test]
    fn test_model_catalog_npm() {
        let models = build_model_catalog(&test_base_url());
        for m in &models {
            assert_eq!(m.api.npm, "@ai-sdk/azure");
        }
    }

    #[test]
    fn test_model_catalog_context_window() {
        let models = build_model_catalog(&test_base_url());
        for m in &models {
            assert_eq!(m.limit.context, 128_000, "model {} context mismatch", m.id);
        }
    }

    #[test]
    fn test_model_catalog_output_tokens() {
        let models = build_model_catalog(&test_base_url());
        for m in &models {
            assert_eq!(m.limit.output, 16_384, "model {} output mismatch", m.id);
        }
    }

    #[test]
    fn test_model_catalog_capabilities_gpt52() {
        let models = build_model_catalog(&test_base_url());
        let model = models
            .iter()
            .find(|m| m.id == "gpt-5.2")
            .expect("gpt-5.2 not found");
        assert!(model.capabilities.temperature);
        assert!(model.capabilities.reasoning);
        assert!(model.capabilities.toolcall);
        assert!(model.capabilities.input.text);
        assert!(model.capabilities.output.text);
    }

    #[test]
    fn test_model_catalog_capabilities_gpt52_mini() {
        let models = build_model_catalog(&test_base_url());
        let model = models
            .iter()
            .find(|m| m.id == "gpt-5.2-mini")
            .expect("gpt-5.2-mini not found");
        assert!(model.capabilities.temperature);
        assert!(model.capabilities.reasoning);
        assert!(model.capabilities.toolcall);
        assert!(model.capabilities.input.text);
        assert!(model.capabilities.output.text);
    }

    #[test]
    fn test_model_catalog_capabilities_gpt51() {
        let models = build_model_catalog(&test_base_url());
        let model = models
            .iter()
            .find(|m| m.id == "gpt-5.1")
            .expect("gpt-5.1 not found");
        assert!(model.capabilities.temperature);
        assert!(model.capabilities.reasoning);
        assert!(model.capabilities.toolcall);
        assert!(model.capabilities.input.text);
        assert!(model.capabilities.output.text);
    }

    #[test]
    fn test_model_catalog_families() {
        let models = build_model_catalog(&test_base_url());
        for m in &models {
            assert_eq!(
                m.family.as_deref(),
                Some("gpt"),
                "model {} family mismatch",
                m.id
            );
        }
    }

    #[test]
    fn test_model_catalog_status_active() {
        let models = build_model_catalog(&test_base_url());
        for m in &models {
            assert_eq!(
                m.status,
                crate::provider::ModelStatus::Active,
                "model {} not active",
                m.id
            );
        }
    }

    // ── get_model ────────────────────────────────────────────────

    #[test]
    fn test_get_model_by_id() {
        let models = build_model_catalog(&test_base_url());
        let model = models.iter().find(|m| m.id == "gpt-5.2").unwrap();
        assert_eq!(model.id, "gpt-5.2");
        assert_eq!(model.name, "GPT-5.2");
    }

    #[test]
    fn test_get_model_not_found() {
        let models = build_model_catalog(&test_base_url());
        let result = models.iter().find(|m| m.id == "nonexistent-model");
        assert!(result.is_none());
    }

    // ── Provider ID ──────────────────────────────────────────────

    #[test]
    fn test_provider_id() {
        let models = build_model_catalog(&test_base_url());
        for m in &models {
            assert_eq!(m.provider_id, "azure");
        }
    }

    #[test]
    fn test_npm_package() {
        let models = build_model_catalog(&test_base_url());
        for m in &models {
            assert_eq!(m.api.npm, "@ai-sdk/azure");
        }
    }

    // ── Chat URL construction ────────────────────────────────────

    #[test]
    fn test_chat_url_with_trailing_slash() {
        let provider = AzureProvider::with_config(
            "sk-test".into(),
            "https://test.openai.azure.com/".into(),
            "gpt-5.2".into(),
        )
        .expect("create provider");
        assert_eq!(
            provider.chat_url(),
            "https://test.openai.azure.com/openai/deployments/gpt-5.2/chat/completions?api-version=2025-01-01-preview"
        );
    }

    #[test]
    fn test_chat_url_without_trailing_slash() {
        let provider = AzureProvider::with_config(
            "sk-test".into(),
            "https://test.openai.azure.com".into(),
            "gpt-5.2".into(),
        )
        .expect("create provider");
        assert_eq!(
            provider.chat_url(),
            "https://test.openai.azure.com/openai/deployments/gpt-5.2/chat/completions?api-version=2025-01-01-preview"
        );
    }

    #[test]
    fn test_chat_url_different_deployment() {
        let provider = AzureProvider::with_config(
            "sk-test".into(),
            "https://test.openai.azure.com".into(),
            "gpt-5.2-mini".into(),
        )
        .expect("create provider");
        assert_eq!(
            provider.chat_url(),
            "https://test.openai.azure.com/openai/deployments/gpt-5.2-mini/chat/completions?api-version=2025-01-01-preview"
        );
    }

    // ── Error classification ─────────────────────────────────────

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
    fn test_classify_error_auth_403() {
        let reason = classify_error(403, "Forbidden");
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
    fn test_classify_error_invalid_request_400() {
        let reason = classify_error(400, "Bad request");
        assert!(matches!(reason, LlmErrorReason::InvalidRequest { .. }));
    }

    #[test]
    fn test_classify_error_context_overflow() {
        let reason = classify_error(400, "This input exceeds the context window of the model");
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
    fn test_classify_error_provider_internal_503() {
        let reason = classify_error(503, "Service unavailable");
        assert!(matches!(
            reason,
            LlmErrorReason::ProviderInternal { status: 503, .. }
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

    // ── Finish reason mapping ────────────────────────────────────

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

    // ── Usage mapping ────────────────────────────────────────────

    #[test]
    fn test_map_usage_basic() {
        let u = AzureUsage {
            prompt_tokens: Some(100),
            completion_tokens: Some(50),
            total_tokens: Some(150),
            prompt_tokens_details: None,
            completion_tokens_details: None,
        };
        let usage = map_usage(&u);
        assert_eq!(usage.input_tokens, Some(100));
        assert_eq!(usage.output_tokens, Some(50));
        assert_eq!(usage.total_tokens, Some(150));
        assert_eq!(usage.reasoning_tokens, None);
        assert_eq!(usage.cache_read_input_tokens, None);
    }

    #[test]
    fn test_map_usage_with_cached_tokens() {
        let u = AzureUsage {
            prompt_tokens: Some(1000),
            completion_tokens: Some(500),
            total_tokens: Some(1500),
            prompt_tokens_details: Some(AzurePromptTokenDetails {
                cached_tokens: Some(300),
            }),
            completion_tokens_details: None,
        };
        let usage = map_usage(&u);
        assert_eq!(usage.input_tokens, Some(1000));
        assert_eq!(usage.cache_read_input_tokens, Some(300));
        assert_eq!(usage.non_cached_input_tokens, Some(700));
    }

    #[test]
    fn test_map_usage_with_reasoning_tokens() {
        let u = AzureUsage {
            prompt_tokens: Some(500),
            completion_tokens: Some(1000),
            total_tokens: Some(1500),
            prompt_tokens_details: None,
            completion_tokens_details: Some(AzureCompletionTokenDetails {
                reasoning_tokens: Some(400),
            }),
        };
        let usage = map_usage(&u);
        assert_eq!(usage.output_tokens, Some(1000));
        assert_eq!(usage.reasoning_tokens, Some(400));
    }

    #[test]
    fn test_map_usage_empty() {
        let u = AzureUsage {
            prompt_tokens: None,
            completion_tokens: None,
            total_tokens: None,
            prompt_tokens_details: None,
            completion_tokens_details: None,
        };
        let usage = map_usage(&u);
        assert_eq!(usage.input_tokens, None);
        assert_eq!(usage.output_tokens, None);
        assert_eq!(usage.total_tokens, None);
    }

    // ── Provider trait methods ───────────────────────────────────

    #[test]
    fn test_provider_trait_provider_id() {
        let provider = make_test_provider();
        assert_eq!(provider.provider_id(), "azure");
    }

    #[test]
    fn test_provider_trait_npm() {
        let provider = make_test_provider();
        assert_eq!(provider.npm(), "@ai-sdk/azure");
    }

    #[test]
    fn test_provider_trait_list_models() {
        let provider = make_test_provider();
        let rt = tokio::runtime::Runtime::new().expect("create runtime");
        let models = rt.block_on(provider.list_models()).expect("list models");
        assert_eq!(models.len(), 3);
    }

    #[test]
    fn test_provider_trait_get_model_found() {
        let provider = make_test_provider();
        let rt = tokio::runtime::Runtime::new().expect("create runtime");
        let model = rt
            .block_on(provider.get_model("gpt-5.2"))
            .expect("get model");
        assert_eq!(model.id, "gpt-5.2");
        assert_eq!(model.name, "GPT-5.2");
    }

    #[test]
    fn test_provider_trait_get_model_not_found() {
        let provider = make_test_provider();
        let rt = tokio::runtime::Runtime::new().expect("create runtime");
        let result = rt.block_on(provider.get_model("nonexistent"));
        assert!(result.is_err());
        if let Err(Error::ModelNotFound {
            provider_id,
            model_id,
        }) = result
        {
            assert_eq!(provider_id, "azure");
            assert_eq!(model_id, "nonexistent");
        } else {
            panic!("expected ModelNotFound error");
        }
    }

    // ── Chat message building ────────────────────────────────────

    #[test]
    fn test_build_chat_messages_system() {
        let messages = vec![ChatMessage::System {
            content: MessageContent::Text("You are helpful.".into()),
        }];
        let result = AzureProvider::build_chat_messages(&messages);
        assert_eq!(result.len(), 1);
        match &result[0] {
            AzureChatMessage::System { content } => {
                assert_eq!(content, "You are helpful.");
            }
            _ => panic!("expected System message"),
        }
    }

    #[test]
    fn test_build_chat_messages_user_text() {
        let messages = vec![ChatMessage::User {
            content: MessageContent::Parts(vec![ContentPart::Text {
                text: "Hello".into(),
            }]),
        }];
        let result = AzureProvider::build_chat_messages(&messages);
        assert_eq!(result.len(), 1);
        match &result[0] {
            AzureChatMessage::User { content } => match content {
                AzureUserContent::Text(t) => assert_eq!(t, "Hello"),
                _ => panic!("expected Text user content"),
            },
            _ => panic!("expected User message"),
        }
    }

    #[test]
    fn test_build_chat_messages_assistant() {
        let messages = vec![ChatMessage::Assistant {
            content: MessageContent::Parts(vec![ContentPart::Text {
                text: "Hi there!".into(),
            }]),
        }];
        let result = AzureProvider::build_chat_messages(&messages);
        assert_eq!(result.len(), 1);
        match &result[0] {
            AzureChatMessage::Assistant { content, .. } => {
                assert_eq!(content.as_deref(), Some("Hi there!"));
            }
            _ => panic!("expected Assistant message"),
        }
    }

    #[test]
    fn test_build_chat_messages_mixed() {
        let messages = vec![
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
        let result = AzureProvider::build_chat_messages(&messages);
        assert_eq!(result.len(), 3);
    }

    #[test]
    fn test_build_chat_messages_empty() {
        let messages: Vec<ChatMessage> = vec![];
        let result = AzureProvider::build_chat_messages(&messages);
        assert_eq!(result.len(), 0);
    }

    // ── Tool building ────────────────────────────────────────────

    #[test]
    fn test_build_tools_empty() {
        let tools: Vec<ToolDefinition> = vec![];
        assert!(build_tools(&tools).is_none());
    }

    #[test]
    fn test_build_tools_single() {
        let tools = vec![ToolDefinition {
            name: "bash".into(),
            description: "Run a shell command".into(),
            parameters: serde_json::json!({"type": "object", "properties": {}}),
        }];
        let result = build_tools(&tools).expect("should have tools");
        assert_eq!(result.len(), 1);
        assert_eq!(result[0].tool_type, "function");
        assert_eq!(result[0].function.name, "bash");
        assert_eq!(result[0].function.description, "Run a shell command");
    }

    // ── Auth error ───────────────────────────────────────────────

    #[test]
    fn test_missing_api_key_error() {
        let saved_key = std::env::var("AZURE_OPENAI_API_KEY").ok();
        let saved_endpoint = std::env::var("AZURE_OPENAI_ENDPOINT").ok();
        let saved_deployment = std::env::var("AZURE_OPENAI_DEPLOYMENT").ok();
        std::env::remove_var("AZURE_OPENAI_API_KEY");
        std::env::remove_var("AZURE_OPENAI_ENDPOINT");
        std::env::remove_var("AZURE_OPENAI_DEPLOYMENT");
        let result = AzureProvider::new();
        if let Some(key) = saved_key {
            std::env::set_var("AZURE_OPENAI_API_KEY", key);
        }
        if let Some(endpoint) = saved_endpoint {
            std::env::set_var("AZURE_OPENAI_ENDPOINT", endpoint);
        }
        if let Some(deployment) = saved_deployment {
            std::env::set_var("AZURE_OPENAI_DEPLOYMENT", deployment);
        }
        assert!(result.is_err());
        assert!(matches!(result.unwrap_err(), Error::Auth(_)));
    }

    #[test]
    fn test_missing_endpoint_error() {
        let saved_key = std::env::var("AZURE_OPENAI_API_KEY").ok();
        let saved_endpoint = std::env::var("AZURE_OPENAI_ENDPOINT").ok();
        let saved_deployment = std::env::var("AZURE_OPENAI_DEPLOYMENT").ok();
        std::env::set_var("AZURE_OPENAI_API_KEY", "test-key");
        std::env::remove_var("AZURE_OPENAI_ENDPOINT");
        std::env::remove_var("AZURE_OPENAI_DEPLOYMENT");
        let result = AzureProvider::new();
        if saved_key.is_none() {
            std::env::remove_var("AZURE_OPENAI_API_KEY");
        }
        if let Some(key) = saved_key {
            std::env::set_var("AZURE_OPENAI_API_KEY", key);
        }
        if let Some(endpoint) = saved_endpoint {
            std::env::set_var("AZURE_OPENAI_ENDPOINT", endpoint);
        }
        if let Some(deployment) = saved_deployment {
            std::env::set_var("AZURE_OPENAI_DEPLOYMENT", deployment);
        }
        assert!(result.is_err());
        assert!(matches!(result.unwrap_err(), Error::Auth(_)));
    }

    // ── Event mapping smoke tests ────────────────────────────────

    #[test]
    fn test_events_from_chat_text_delta() {
        let event = AzureChatEvent {
            choices: vec![AzureChoice {
                delta: Some(AzureDelta {
                    content: Some("Hello".into()),
                    reasoning_content: None,
                    tool_calls: None,
                }),
                finish_reason: None,
            }],
            usage: None,
        };
        let mut state = ChatStreamState {
            tool_stream: ToolStreamAccumulator::new(),
            text_started: false,
            reasoning_started: false,
            step_started: false,
            usage: None,
            finished: false,
        };
        let events = events_from_chat(event, &mut state);
        assert!(!events.is_empty());
        // Should include TextStart + TextDelta
        assert!(events
            .iter()
            .any(|e| matches!(e, LlmEvent::TextStart { .. })));
        assert!(events
            .iter()
            .any(|e| matches!(e, LlmEvent::TextDelta { .. })));
        assert!(state.text_started);
    }

    #[test]
    fn test_events_from_chat_reasoning_delta() {
        let event = AzureChatEvent {
            choices: vec![AzureChoice {
                delta: Some(AzureDelta {
                    content: None,
                    reasoning_content: Some("Let me think...".into()),
                    tool_calls: None,
                }),
                finish_reason: None,
            }],
            usage: None,
        };
        let mut state = ChatStreamState {
            tool_stream: ToolStreamAccumulator::new(),
            text_started: false,
            reasoning_started: false,
            step_started: false,
            usage: None,
            finished: false,
        };
        let events = events_from_chat(event, &mut state);
        assert!(!events.is_empty());
        assert!(events
            .iter()
            .any(|e| matches!(e, LlmEvent::ReasoningStart { .. })));
        assert!(events
            .iter()
            .any(|e| matches!(e, LlmEvent::ReasoningDelta { .. })));
        assert!(state.reasoning_started);
    }

    #[test]
    fn test_events_from_chat_finish() {
        let event = AzureChatEvent {
            choices: vec![AzureChoice {
                delta: None,
                finish_reason: Some("stop".into()),
            }],
            usage: Some(AzureUsage {
                prompt_tokens: Some(10),
                completion_tokens: Some(5),
                total_tokens: Some(15),
                prompt_tokens_details: None,
                completion_tokens_details: None,
            }),
        };
        let mut state = ChatStreamState {
            tool_stream: ToolStreamAccumulator::new(),
            text_started: false,
            reasoning_started: false,
            step_started: false,
            usage: None,
            finished: false,
        };
        let events = events_from_chat(event, &mut state);
        assert!(state.finished);
        assert!(events.iter().any(|e| matches!(e, LlmEvent::Finish { .. })));
        assert!(events
            .iter()
            .any(|e| matches!(e, LlmEvent::StepFinish { .. })));
    }

    // ── Config resolution ────────────────────────────────────────

    #[test]
    fn test_resolve_config_all_set() {
        let saved_key = std::env::var("AZURE_OPENAI_API_KEY").ok();
        let saved_endpoint = std::env::var("AZURE_OPENAI_ENDPOINT").ok();
        let saved_deployment = std::env::var("AZURE_OPENAI_DEPLOYMENT").ok();
        std::env::set_var("AZURE_OPENAI_API_KEY", "my-key");
        std::env::set_var("AZURE_OPENAI_ENDPOINT", "https://my.openai.azure.com");
        std::env::set_var("AZURE_OPENAI_DEPLOYMENT", "gpt-5.2");
        let result = resolve_config();
        if let Some(key) = saved_key {
            std::env::set_var("AZURE_OPENAI_API_KEY", key);
        } else {
            std::env::remove_var("AZURE_OPENAI_API_KEY");
        }
        if let Some(endpoint) = saved_endpoint {
            std::env::set_var("AZURE_OPENAI_ENDPOINT", endpoint);
        } else {
            std::env::remove_var("AZURE_OPENAI_ENDPOINT");
        }
        if let Some(deployment) = saved_deployment {
            std::env::set_var("AZURE_OPENAI_DEPLOYMENT", deployment);
        } else {
            std::env::remove_var("AZURE_OPENAI_DEPLOYMENT");
        }
        assert!(result.is_ok());
        let (key, endpoint, deployment) = result.unwrap();
        assert_eq!(key, "my-key");
        assert_eq!(endpoint, "https://my.openai.azure.com");
        assert_eq!(deployment, "gpt-5.2");
    }

    #[test]
    fn test_resolve_config_empty_key() {
        let saved = std::env::var("AZURE_OPENAI_API_KEY").ok();
        std::env::set_var("AZURE_OPENAI_API_KEY", "");
        let result = resolve_config();
        if let Some(key) = saved {
            std::env::set_var("AZURE_OPENAI_API_KEY", key);
        } else {
            std::env::remove_var("AZURE_OPENAI_API_KEY");
        }
        assert!(result.is_err());
    }
}
