//! Amazon Bedrock API provider — OpenAI-compatible Chat Completions protocol.
//!
//! Bedrock's Converse API is bridged through an OpenAI-compatible chat
//! completions endpoint, so this provider reuses the same body builder and
//! SSE event parser patterns as the OpenAI provider, with Bedrock-specific:
//! - Base URL: <https://bedrock-runtime.{region}.amazonaws.com>
//! - Auth: AWS SigV4 request signing
//! - Model catalog: Claude Sonnet/Opus/Haiku 4, Llama 4 Maverick, Titan Text
//!
//! Ported from:
//! - `packages/llm/src/protocols/openai-chat.ts` (493 lines)
//! - `packages/llm/src/providers/openai-compatible.ts` (66 lines)
//! - `packages/llm/src/providers/openai-compatible-profile.ts` (21 lines)

use async_trait::async_trait;
use chrono::Utc;
use futures::StreamExt;
use hmac::{Hmac, Mac};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::collections::{HashMap, VecDeque};
use std::pin::Pin;

use crate::error::{Error, LlmErrorReason};
use crate::provider::{
    ChatMessage, ContentPart, FinishReason, LlmEvent, MessageContent, Model, Provider,
    ToolDefinition, Usage,
};
use crate::sse::parse_sse_stream;
use crate::tool_stream::ToolStreamAccumulator;

// ── AWS SigV4 Signing ──────────────────────────────────────────────────

type HmacSha256 = Hmac<Sha256>;

/// AWS SigV4 signer for Bedrock requests.
///
/// # Source
/// Implements the AWS Signature Version 4 signing algorithm per
/// <https://docs.aws.amazon.com/general/latest/gr/sigv4-signing.html>.
struct SigV4Signer {
    access_key: String,
    secret_key: String,
    region: String,
    service: &'static str,
}

impl SigV4Signer {
    fn new(access_key: &str, secret_key: &str, region: &str) -> Self {
        Self {
            access_key: access_key.into(),
            secret_key: secret_key.into(),
            region: region.into(),
            service: "bedrock",
        }
    }

    /// Sign a request and return the Authorization header value plus the
    /// x-amz-date header value.
    fn sign(
        &self,
        method: &str,
        host: &str,
        path: &str,
        body: &[u8],
        date: &str,
    ) -> Result<String, Error> {
        // Step 1: Create canonical request per AWS SigV4 spec
        let payload_hash = hex::encode(Sha256::digest(body));
        let signed_headers = "host;x-amz-content-sha256;x-amz-date";
        let canonical_headers = format!(
            "host:{host}\nx-amz-content-sha256:{payload_hash}\nx-amz-date:{date}\n"
        );
        let canonical_request = format!(
            "{method}\n{path}\n\n{canonical_headers}\n{signed_headers}\n{payload_hash}"
        );

        // Step 2: Create string to sign
        let credential_scope = format!("{date}/{}/{}/aws4_request", self.region, self.service);
        let canonical_request_hash = hex::encode(Sha256::digest(canonical_request.as_bytes()));
        let string_to_sign = format!(
            "AWS4-HMAC-SHA256\n{date}\n{credential_scope}\n{canonical_request_hash}"
        );

        // Step 3: Calculate signing key
        let k_date = self.hmac_raw(format!("AWS4{}", self.secret_key).as_bytes(), date.as_bytes());
        let k_region = self.hmac_raw(&k_date, self.region.as_bytes());
        let k_service = self.hmac_raw(&k_region, self.service.as_bytes());
        let k_signing = self.hmac_raw(&k_service, b"aws4_request");

        // Step 4: Calculate signature
        let signature = self.hmac_raw(&k_signing, string_to_sign.as_bytes());

        // Step 5: Build authorization header
        Ok(format!(
            "AWS4-HMAC-SHA256 Credential={}/{}, SignedHeaders={signed_headers}, Signature={}",
            self.access_key,
            credential_scope,
            hex::encode(signature),
        ))
    }

    fn hmac_raw(&self, key: &[u8], data: &[u8]) -> Vec<u8> {
        let mut mac =
            HmacSha256::new_from_slice(key).expect("HMAC can take key of any size");
        mac.update(data);
        mac.finalize().into_bytes().to_vec()
    }
}

const BASE_URL: &str = "https://bedrock-runtime.us-east-1.amazonaws.com";
const CHAT_PATH: &str = "/chat/completions";
const DEFAULT_REGION: &str = "us-east-1";

fn resolve_api_key() -> Result<String, Error> {
    std::env::var("AWS_ACCESS_KEY_ID")
        .ok()
        .filter(|k| !k.is_empty())
        .ok_or_else(|| Error::Auth("AWS_ACCESS_KEY_ID environment variable not set".into()))
}

fn resolve_secret_key() -> Result<String, Error> {
    std::env::var("AWS_SECRET_ACCESS_KEY")
        .ok()
        .filter(|k| !k.is_empty())
        .ok_or_else(|| Error::Auth("AWS_SECRET_ACCESS_KEY environment variable not set".into()))
}

fn resolve_region() -> String {
    std::env::var("AWS_REGION")
        .ok()
        .filter(|r| !r.is_empty())
        .unwrap_or_else(|| DEFAULT_REGION.into())
}

fn bedrock_base_url(region: &str) -> String {
    format!("https://bedrock-runtime.{}.amazonaws.com", region)
}

// ── Chat Completions Body ──────────────────────────────────────────────

#[derive(Debug, Serialize)]
struct BedrockChatBody {
    model: String,
    messages: Vec<BedrockChatMessage>,
    #[serde(skip_serializing_if = "Option::is_none")]
    tools: Option<Vec<BedrockTool>>,
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
enum BedrockChatMessage {
    #[serde(rename = "system")]
    System { content: String },
    #[serde(rename = "user")]
    User { content: BedrockUserContent },
    #[serde(rename = "assistant")]
    Assistant {
        content: Option<String>,
        #[serde(skip_serializing_if = "Option::is_none")]
        tool_calls: Option<Vec<BedrockAssistantToolCall>>,
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
enum BedrockUserContent {
    Text(String),
    Parts(Vec<BedrockUserContentPart>),
}

#[derive(Debug, Serialize)]
#[serde(tag = "type")]
enum BedrockUserContentPart {
    #[serde(rename = "text")]
    Text { text: String },
    #[serde(rename = "image_url")]
    ImageUrl { image_url: BedrockImageUrl },
}

#[derive(Debug, Serialize)]
struct BedrockImageUrl {
    url: String,
}

#[derive(Debug, Serialize)]
struct BedrockAssistantToolCall {
    id: String,
    #[serde(rename = "type")]
    call_type: String,
    function: BedrockToolCallFunction,
}

#[derive(Debug, Serialize)]
struct BedrockToolCallFunction {
    name: String,
    arguments: String,
}

#[derive(Debug, Serialize)]
struct BedrockTool {
    #[serde(rename = "type")]
    tool_type: String,
    function: BedrockFunctionDef,
}

#[derive(Debug, Serialize)]
struct BedrockFunctionDef {
    name: String,
    description: String,
    parameters: serde_json::Value,
}

// ── Chat SSE Event types ───────────────────────────────────────────────

#[derive(Debug, Deserialize)]
struct BedrockChatEvent {
    choices: Vec<BedrockChoice>,
    #[serde(default)]
    usage: Option<BedrockUsage>,
}

#[derive(Debug, Deserialize)]
struct BedrockChoice {
    delta: Option<BedrockDelta>,
    finish_reason: Option<String>,
}

#[derive(Debug, Deserialize)]
struct BedrockDelta {
    content: Option<String>,
    reasoning_content: Option<String>,
    tool_calls: Option<Vec<BedrockToolCallDelta>>,
}

#[derive(Debug, Deserialize)]
struct BedrockToolCallDelta {
    index: u64,
    id: Option<String>,
    function: Option<BedrockToolCallDeltaFn>,
}

#[derive(Debug, Deserialize)]
struct BedrockToolCallDeltaFn {
    name: Option<String>,
    arguments: Option<String>,
}

#[derive(Debug, Deserialize)]
struct BedrockUsage {
    prompt_tokens: Option<u64>,
    completion_tokens: Option<u64>,
    total_tokens: Option<u64>,
    prompt_tokens_details: Option<BedrockPromptTokenDetails>,
    completion_tokens_details: Option<BedrockCompletionTokenDetails>,
}

#[derive(Debug, Deserialize)]
struct BedrockPromptTokenDetails {
    cached_tokens: Option<u64>,
}

#[derive(Debug, Deserialize)]
struct BedrockCompletionTokenDetails {
    reasoning_tokens: Option<u64>,
}

// ── Bedrock Provider ───────────────────────────────────────────────────

#[derive(Debug)]
pub struct BedrockProvider {
    api_key: String,
    secret_key: String,
    region: String,
    base_url: String,
    http_client: reqwest::Client,
    models: Vec<Model>,
}

impl BedrockProvider {
    /// Create a new Bedrock provider from `AWS_ACCESS_KEY_ID`,
    /// `AWS_SECRET_ACCESS_KEY`, and `AWS_REGION` env vars.
    pub fn new() -> Result<Self, Error> {
        let api_key = resolve_api_key()?;
        let secret_key = resolve_secret_key()?;
        let region = resolve_region();
        let base_url = bedrock_base_url(&region);
        Self::with_base_url(api_key, secret_key, region, base_url)
    }

    /// Create with explicit credentials and a custom base URL (for
    /// proxies, self-hosted deployments, or non-standard regions).
    pub fn with_base_url(
        api_key: String,
        secret_key: String,
        region: String,
        base_url: String,
    ) -> Result<Self, Error> {
        let http_client = reqwest::Client::builder()
            .user_agent(format!("rustcode/{}", env!("CARGO_PKG_VERSION")))
            .timeout(std::time::Duration::from_secs(300))
            .build()
            .map_err(|e| Error::Network(format!("HTTP client: {e}")))?;
        Ok(Self {
            api_key,
            secret_key,
            region,
            base_url,
            http_client,
            models: build_model_catalog(),
        })
    }

    fn chat_url(&self) -> String {
        format!("{}{CHAT_PATH}", self.base_url.trim_end_matches('/'))
    }

    fn build_chat_messages(messages: &[ChatMessage]) -> Vec<BedrockChatMessage> {
        let mut result = Vec::new();
        let mut pending_images: Vec<BedrockUserContentPart> = Vec::new();

        for msg in messages {
            match msg {
                ChatMessage::System { content } => {
                    let text = extract_text(content);
                    if !text.is_empty() {
                        result.push(BedrockChatMessage::System { content: text });
                    }
                }
                ChatMessage::User { content } => {
                    let mut text_parts = String::new();
                    let mut media_parts: Vec<BedrockUserContentPart> = Vec::new();
                    for part in content_parts(content) {
                        match part {
                            ContentPart::Text { text } => text_parts.push_str(text),
                            ContentPart::Image { image } => {
                                media_parts.push(BedrockUserContentPart::ImageUrl {
                                    image_url: BedrockImageUrl {
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
                        result.push(BedrockChatMessage::User {
                            content: BedrockUserContent::Text(text_parts),
                        });
                    } else {
                        let mut parts = media_parts;
                        if !text_parts.is_empty() {
                            parts.insert(0, BedrockUserContentPart::Text { text: text_parts });
                        }
                        result.push(BedrockChatMessage::User {
                            content: BedrockUserContent::Parts(parts),
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
                                tool_calls.push(BedrockAssistantToolCall {
                                    id: tool_call_id.clone(),
                                    call_type: "function".into(),
                                    function: BedrockToolCallFunction {
                                        name: tool_name.clone(),
                                        arguments: arguments.to_string(),
                                    },
                                });
                            }
                            _ => {}
                        }
                    }
                    result.push(BedrockChatMessage::Assistant {
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
                        result.push(BedrockChatMessage::Tool {
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

fn build_tools(tools: &[ToolDefinition]) -> Option<Vec<BedrockTool>> {
    if tools.is_empty() {
        return None;
    }
    Some(
        tools
            .iter()
            .map(|t| BedrockTool {
                tool_type: "function".into(),
                function: BedrockFunctionDef {
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

fn map_usage(u: &BedrockUsage) -> Usage {
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

fn events_from_chat(event: BedrockChatEvent, state: &mut ChatStreamState) -> Vec<LlmEvent> {
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

fn build_model_catalog() -> Vec<Model> {
    vec![
        make_model(
            "claude-sonnet-4-20250514",
            "Claude Sonnet 4",
            200_000,
            8_192,
            "claude",
            true,
            true,
        ),
        make_model(
            "claude-opus-4-20250514",
            "Claude Opus 4",
            200_000,
            8_192,
            "claude",
            true,
            true,
        ),
        make_model(
            "claude-haiku-4-20250514",
            "Claude Haiku 4",
            200_000,
            8_192,
            "claude",
            true,
            false,
        ),
        make_model(
            "llama-4-maverick",
            "Llama 4 Maverick",
            128_000,
            4_096,
            "llama",
            true,
            false,
        ),
        make_model(
            "titan-text",
            "Titan Text",
            8_000,
            4_096,
            "titan",
            true,
            false,
        ),
    ]
}

fn make_model(
    id: &str,
    name: &str,
    ctx: u64,
    out: u64,
    family: &str,
    temperature: bool,
    reasoning: bool,
) -> Model {
    Model {
        id: id.into(),
        provider_id: "bedrock".into(),
        name: name.into(),
        api: crate::provider::ApiInfo {
            id: id.into(),
            url: BASE_URL.into(),
            npm: "@ai-sdk/amazon-bedrock".into(),
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
impl Provider for BedrockProvider {
    fn provider_id(&self) -> &str {
        "bedrock"
    }

    fn npm(&self) -> &str {
        "@ai-sdk/amazon-bedrock"
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
                provider_id: "bedrock".into(),
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
        let body = BedrockChatBody {
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

        let body_json = serde_json::to_vec(&body)
            .map_err(|e| Error::Network(format!("Bedrock body serialization: {e}")))?;

        let signer = SigV4Signer::new(&self.api_key, &self.secret_key, &self.region);
        let now = Utc::now().format("%Y%m%dT%H%M%SZ").to_string();
        let url = url::Url::parse(&self.chat_url())
            .map_err(|e| Error::Network(format!("Bedrock URL parse: {e}")))?;
        let host = url.host_str().unwrap_or("");
        let path = url.path();

        let authorization = signer
            .sign("POST", host, path, &body_json, &now)
            .map_err(|e| Error::Network(format!("Bedrock SigV4: {e}")))?;

        let response = self
            .http_client
            .post(self.chat_url())
            .header("Content-Type", "application/json")
            .header("Authorization", authorization)
            .header("X-Amz-Date", &now)
            .header("X-Amz-Content-Sha256", hex::encode(Sha256::digest(&body_json)))
            .body(body_json)
            .send()
            .await
            .map_err(|e| Error::Network(format!("Bedrock request: {e}")))?;

        if !response.status().is_success() {
            let status = response.status().as_u16();
            let text = response.text().await.unwrap_or_default();
            return Err(Error::Llm {
                module: "bedrock".into(),
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
                                if let Ok(oe) = serde_json::from_str::<BedrockChatEvent>(&se.data) {
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
                                    Err(Error::ResponseStream(format!("Bedrock SSE: {e}"))),
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

// ── Tests ──────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    // ── Model catalog ────────────────────────────────────────────

    #[test]
    fn test_model_catalog_count() {
        let models = build_model_catalog();
        assert_eq!(models.len(), 5, "expected 5 models in catalog");
    }

    #[test]
    fn test_model_catalog_ids() {
        let models = build_model_catalog();
        let ids: Vec<&str> = models.iter().map(|m| m.id.as_str()).collect();
        assert!(ids.contains(&"claude-sonnet-4-20250514"));
        assert!(ids.contains(&"claude-opus-4-20250514"));
        assert!(ids.contains(&"claude-haiku-4-20250514"));
        assert!(ids.contains(&"llama-4-maverick"));
        assert!(ids.contains(&"titan-text"));
    }

    #[test]
    fn test_model_catalog_names() {
        let models = build_model_catalog();
        let names: Vec<&str> = models.iter().map(|m| m.name.as_str()).collect();
        assert!(names.contains(&"Claude Sonnet 4"));
        assert!(names.contains(&"Claude Opus 4"));
        assert!(names.contains(&"Claude Haiku 4"));
        assert!(names.contains(&"Llama 4 Maverick"));
        assert!(names.contains(&"Titan Text"));
    }

    #[test]
    fn test_model_catalog_provider_id() {
        let models = build_model_catalog();
        for m in &models {
            assert_eq!(m.provider_id, "bedrock");
        }
    }

    #[test]
    fn test_model_catalog_npm() {
        let models = build_model_catalog();
        for m in &models {
            assert_eq!(m.api.npm, "@ai-sdk/amazon-bedrock");
        }
    }

    #[test]
    fn test_model_catalog_context_windows() {
        let models = build_model_catalog();
        // Claude models: 200_000
        let sonnet4 = models
            .iter()
            .find(|m| m.id == "claude-sonnet-4-20250514")
            .unwrap();
        assert_eq!(sonnet4.limit.context, 200_000);
        let opus4 = models
            .iter()
            .find(|m| m.id == "claude-opus-4-20250514")
            .unwrap();
        assert_eq!(opus4.limit.context, 200_000);
        let haiku4 = models
            .iter()
            .find(|m| m.id == "claude-haiku-4-20250514")
            .unwrap();
        assert_eq!(haiku4.limit.context, 200_000);
        // Llama: 128_000
        let llama = models.iter().find(|m| m.id == "llama-4-maverick").unwrap();
        assert_eq!(llama.limit.context, 128_000);
        // Titan: 8_000
        let titan = models.iter().find(|m| m.id == "titan-text").unwrap();
        assert_eq!(titan.limit.context, 8_000);
    }

    #[test]
    fn test_model_catalog_output_tokens() {
        let models = build_model_catalog();
        // Claude models: 8_192
        for id in &[
            "claude-sonnet-4-20250514",
            "claude-opus-4-20250514",
            "claude-haiku-4-20250514",
        ] {
            let m = models.iter().find(|m| m.id == *id).unwrap();
            assert_eq!(m.limit.output, 8_192, "model {id} output mismatch");
        }
        // Llama: 4_096
        let llama = models.iter().find(|m| m.id == "llama-4-maverick").unwrap();
        assert_eq!(llama.limit.output, 4_096);
        // Titan: 4_096
        let titan = models.iter().find(|m| m.id == "titan-text").unwrap();
        assert_eq!(titan.limit.output, 4_096);
    }

    #[test]
    fn test_model_catalog_capabilities_claude_sonnet() {
        let models = build_model_catalog();
        let m = models
            .iter()
            .find(|m| m.id == "claude-sonnet-4-20250514")
            .expect("claude-sonnet-4 not found");
        assert!(m.capabilities.temperature);
        assert!(m.capabilities.reasoning);
        assert!(m.capabilities.toolcall);
        assert!(m.capabilities.input.text);
        assert!(m.capabilities.output.text);
    }

    #[test]
    fn test_model_catalog_capabilities_claude_opus() {
        let models = build_model_catalog();
        let m = models
            .iter()
            .find(|m| m.id == "claude-opus-4-20250514")
            .expect("claude-opus-4 not found");
        assert!(m.capabilities.temperature);
        assert!(m.capabilities.reasoning);
        assert!(m.capabilities.toolcall);
        assert!(m.capabilities.input.text);
        assert!(m.capabilities.output.text);
    }

    #[test]
    fn test_model_catalog_capabilities_claude_haiku() {
        let models = build_model_catalog();
        let m = models
            .iter()
            .find(|m| m.id == "claude-haiku-4-20250514")
            .expect("claude-haiku-4 not found");
        assert!(m.capabilities.temperature);
        assert!(!m.capabilities.reasoning);
        assert!(m.capabilities.toolcall);
        assert!(m.capabilities.input.text);
        assert!(m.capabilities.output.text);
    }

    #[test]
    fn test_model_catalog_capabilities_llama() {
        let models = build_model_catalog();
        let m = models
            .iter()
            .find(|m| m.id == "llama-4-maverick")
            .expect("llama-4-maverick not found");
        assert!(m.capabilities.temperature);
        assert!(!m.capabilities.reasoning);
        assert!(m.capabilities.toolcall);
        assert!(m.capabilities.input.text);
        assert!(m.capabilities.output.text);
    }

    #[test]
    fn test_model_catalog_capabilities_titan() {
        let models = build_model_catalog();
        let m = models
            .iter()
            .find(|m| m.id == "titan-text")
            .expect("titan-text not found");
        assert!(m.capabilities.temperature);
        assert!(!m.capabilities.reasoning);
        assert!(m.capabilities.toolcall);
        assert!(m.capabilities.input.text);
        assert!(m.capabilities.output.text);
    }

    #[test]
    fn test_model_catalog_families() {
        let models = build_model_catalog();
        let sonnet = models
            .iter()
            .find(|m| m.id == "claude-sonnet-4-20250514")
            .unwrap();
        assert_eq!(sonnet.family.as_deref(), Some("claude"));
        let opus = models
            .iter()
            .find(|m| m.id == "claude-opus-4-20250514")
            .unwrap();
        assert_eq!(opus.family.as_deref(), Some("claude"));
        let haiku = models
            .iter()
            .find(|m| m.id == "claude-haiku-4-20250514")
            .unwrap();
        assert_eq!(haiku.family.as_deref(), Some("claude"));
        let llama = models.iter().find(|m| m.id == "llama-4-maverick").unwrap();
        assert_eq!(llama.family.as_deref(), Some("llama"));
        let titan = models.iter().find(|m| m.id == "titan-text").unwrap();
        assert_eq!(titan.family.as_deref(), Some("titan"));
    }

    #[test]
    fn test_model_catalog_status_active() {
        let models = build_model_catalog();
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
        let models = build_model_catalog();
        let sonnet = models
            .iter()
            .find(|m| m.id == "claude-sonnet-4-20250514")
            .unwrap();
        assert_eq!(sonnet.id, "claude-sonnet-4-20250514");
        assert_eq!(sonnet.name, "Claude Sonnet 4");
    }

    #[test]
    fn test_get_model_not_found() {
        let models = build_model_catalog();
        let result = models.iter().find(|m| m.id == "nonexistent-model");
        assert!(result.is_none());
    }

    // ── Provider ID ──────────────────────────────────────────────

    #[test]
    fn test_provider_id() {
        let models = build_model_catalog();
        for m in &models {
            assert_eq!(m.provider_id, "bedrock");
        }
    }

    #[test]
    fn test_npm_package() {
        let models = build_model_catalog();
        for m in &models {
            assert_eq!(m.api.npm, "@ai-sdk/amazon-bedrock");
        }
    }

    #[test]
    fn test_base_url_constant() {
        assert_eq!(BASE_URL, "https://bedrock-runtime.us-east-1.amazonaws.com");
    }

    #[test]
    fn test_bedrock_base_url_format() {
        assert_eq!(
            bedrock_base_url("us-west-2"),
            "https://bedrock-runtime.us-west-2.amazonaws.com"
        );
        assert_eq!(
            bedrock_base_url("eu-central-1"),
            "https://bedrock-runtime.eu-central-1.amazonaws.com"
        );
    }

    // ── Error classification ─────────────────────────────────────

    #[test]
    fn test_classify_error_auth_401() {
        let reason = classify_error(401, r#"{"error":{"message":"Invalid AWS credentials"}}"#);
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
        let u = BedrockUsage {
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
        let u = BedrockUsage {
            prompt_tokens: Some(1000),
            completion_tokens: Some(500),
            total_tokens: Some(1500),
            prompt_tokens_details: Some(BedrockPromptTokenDetails {
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
        let u = BedrockUsage {
            prompt_tokens: Some(500),
            completion_tokens: Some(1000),
            total_tokens: Some(1500),
            prompt_tokens_details: None,
            completion_tokens_details: Some(BedrockCompletionTokenDetails {
                reasoning_tokens: Some(400),
            }),
        };
        let usage = map_usage(&u);
        assert_eq!(usage.output_tokens, Some(1000));
        assert_eq!(usage.reasoning_tokens, Some(400));
    }

    #[test]
    fn test_map_usage_empty() {
        let u = BedrockUsage {
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

    // ── Chat URL construction ────────────────────────────────────

    #[test]
    fn test_chat_url_with_trailing_slash() {
        let provider = BedrockProvider::with_base_url(
            "test-access-key".into(),
            "test-secret-key".into(),
            "us-east-1".into(),
            "https://bedrock-runtime.us-east-1.amazonaws.com/".into(),
        )
        .expect("create provider");
        assert_eq!(
            provider.chat_url(),
            "https://bedrock-runtime.us-east-1.amazonaws.com/chat/completions"
        );
    }

    #[test]
    fn test_chat_url_without_trailing_slash() {
        let provider = BedrockProvider::with_base_url(
            "test-access-key".into(),
            "test-secret-key".into(),
            "us-east-1".into(),
            "https://bedrock-runtime.us-east-1.amazonaws.com".into(),
        )
        .expect("create provider");
        assert_eq!(
            provider.chat_url(),
            "https://bedrock-runtime.us-east-1.amazonaws.com/chat/completions"
        );
    }

    // ── Provider trait methods ───────────────────────────────────

    #[test]
    fn test_provider_trait_provider_id() {
        let provider = BedrockProvider::with_base_url(
            "test-ak".into(),
            "test-sk".into(),
            "us-east-1".into(),
            "https://bedrock-runtime.us-east-1.amazonaws.com".into(),
        )
        .expect("create provider");
        assert_eq!(provider.provider_id(), "bedrock");
    }

    #[test]
    fn test_provider_trait_npm() {
        let provider = BedrockProvider::with_base_url(
            "test-ak".into(),
            "test-sk".into(),
            "us-east-1".into(),
            "https://bedrock-runtime.us-east-1.amazonaws.com".into(),
        )
        .expect("create provider");
        assert_eq!(provider.npm(), "@ai-sdk/amazon-bedrock");
    }

    #[test]
    fn test_provider_trait_list_models() {
        let provider = BedrockProvider::with_base_url(
            "test-ak".into(),
            "test-sk".into(),
            "us-east-1".into(),
            "https://bedrock-runtime.us-east-1.amazonaws.com".into(),
        )
        .expect("create provider");
        let rt = tokio::runtime::Runtime::new().expect("create runtime");
        let models = rt.block_on(provider.list_models()).expect("list models");
        assert_eq!(models.len(), 5);
    }

    #[test]
    fn test_provider_trait_get_model_found() {
        let provider = BedrockProvider::with_base_url(
            "test-ak".into(),
            "test-sk".into(),
            "us-east-1".into(),
            "https://bedrock-runtime.us-east-1.amazonaws.com".into(),
        )
        .expect("create provider");
        let rt = tokio::runtime::Runtime::new().expect("create runtime");
        let model = rt
            .block_on(provider.get_model("claude-sonnet-4-20250514"))
            .expect("get model");
        assert_eq!(model.id, "claude-sonnet-4-20250514");
        assert_eq!(model.name, "Claude Sonnet 4");
    }

    #[test]
    fn test_provider_trait_get_model_not_found() {
        let provider = BedrockProvider::with_base_url(
            "test-ak".into(),
            "test-sk".into(),
            "us-east-1".into(),
            "https://bedrock-runtime.us-east-1.amazonaws.com".into(),
        )
        .expect("create provider");
        let rt = tokio::runtime::Runtime::new().expect("create runtime");
        let result = rt.block_on(provider.get_model("nonexistent"));
        assert!(result.is_err());
        if let Err(Error::ModelNotFound {
            provider_id,
            model_id,
        }) = result
        {
            assert_eq!(provider_id, "bedrock");
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
        let result = BedrockProvider::build_chat_messages(&messages);
        assert_eq!(result.len(), 1);
        match &result[0] {
            BedrockChatMessage::System { content } => {
                assert_eq!(content, "You are helpful.");
            }
            _ => panic!("expected System message"),
        }
    }

    #[test]
    fn test_build_chat_messages_user_text() {
        let messages = vec![ChatMessage::User {
            content: MessageContent::Text("Hello".into()),
        }];
        let result = BedrockProvider::build_chat_messages(&messages);
        assert_eq!(result.len(), 1);
        match &result[0] {
            BedrockChatMessage::User { content } => match content {
                BedrockUserContent::Text(t) => assert_eq!(t, "Hello"),
                _ => panic!("expected Text user content"),
            },
            _ => panic!("expected User message"),
        }
    }

    #[test]
    fn test_build_chat_messages_assistant() {
        let messages = vec![ChatMessage::Assistant {
            content: MessageContent::Text("Hi there!".into()),
        }];
        let result = BedrockProvider::build_chat_messages(&messages);
        assert_eq!(result.len(), 1);
        match &result[0] {
            BedrockChatMessage::Assistant { content, .. } => {
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
        let result = BedrockProvider::build_chat_messages(&messages);
        assert_eq!(result.len(), 3);
    }

    #[test]
    fn test_build_chat_messages_empty() {
        let messages: Vec<ChatMessage> = vec![];
        let result = BedrockProvider::build_chat_messages(&messages);
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
    fn test_missing_access_key_error() {
        let saved = std::env::var("AWS_ACCESS_KEY_ID").ok();
        std::env::remove_var("AWS_ACCESS_KEY_ID");
        let result = BedrockProvider::new();
        if let Some(key) = saved {
            std::env::set_var("AWS_ACCESS_KEY_ID", key);
        }
        assert!(result.is_err());
        assert!(matches!(result.unwrap_err(), Error::Auth(_)));
    }

    #[test]
    fn test_missing_secret_key_error() {
        let saved_access = std::env::var("AWS_ACCESS_KEY_ID").ok();
        let saved_secret = std::env::var("AWS_SECRET_ACCESS_KEY").ok();
        std::env::set_var("AWS_ACCESS_KEY_ID", "AKIATEST");
        std::env::remove_var("AWS_SECRET_ACCESS_KEY");
        let result = BedrockProvider::new();
        if let Some(key) = saved_access {
            std::env::set_var("AWS_ACCESS_KEY_ID", key);
        } else {
            std::env::remove_var("AWS_ACCESS_KEY_ID");
        }
        if let Some(key) = saved_secret {
            std::env::set_var("AWS_SECRET_ACCESS_KEY", key);
        }
        assert!(result.is_err());
        assert!(matches!(result.unwrap_err(), Error::Auth(_)));
    }

    // ── Event mapping smoke tests ────────────────────────────────

    #[test]
    fn test_events_from_chat_text_delta() {
        let event = BedrockChatEvent {
            choices: vec![BedrockChoice {
                delta: Some(BedrockDelta {
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
        let event = BedrockChatEvent {
            choices: vec![BedrockChoice {
                delta: Some(BedrockDelta {
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
        let event = BedrockChatEvent {
            choices: vec![BedrockChoice {
                delta: None,
                finish_reason: Some("stop".into()),
            }],
            usage: Some(BedrockUsage {
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
}
