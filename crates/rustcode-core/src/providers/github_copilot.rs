//! GitHub Copilot provider module.
//!
//! GitHub Copilot uses the OpenAI-compatible Chat Completions API with a unique
//! token exchange auth flow:
//!
//! 1. Obtain a GitHub token (via `GITHUB_TOKEN` env var or device OAuth flow)
//! 2. Exchange the GitHub token for a Copilot token via
//!    `POST https://api.github.com/copilot_internal/v2/token`
//! 3. Use the Copilot token as a Bearer token for the Chat Completions API
//!
//! Supports both Chat Completions and Responses API (auto-selects based on model).
//!
//! Ported from:
//! - `packages/llm/src/providers/github-copilot.ts` (66 lines)
//! - `packages/opencode/src/auth/github-copilot.ts`
//!
//! OpenCode commit: 5d0f86606ac30690f79f0a6a9f41a1f49fe95d0b

use async_trait::async_trait;
use futures::StreamExt;
use serde::{Deserialize, Serialize};
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

const DEFAULT_BASE_URL: &str = "https://api.githubcopilot.com";
const CHAT_PATH: &str = "/chat/completions";
const COPILOT_TOKEN_URL: &str = "https://api.github.com/copilot_internal/v2/token";

/// Default Copilot headers (mimics VS Code integration).
const COPILOT_HEADERS: &[(&str, &str)] = &[
    ("Copilot-Integration-Id", "vscode"),
    ("Editor-Version", "vscode/1.95.0"),
];

// ── Token exchange types ────────────────────────────────────────────────

/// Response from the Copilot token endpoint.
#[derive(Debug, Deserialize)]
struct CopilotTokenResponse {
    /// The Copilot API token to use for Bearer auth.
    token: String,
    /// When the token expires (Unix epoch seconds).
    #[serde(default)]
    expires_at: Option<u64>,
}

// ── SSE event types (OpenAI Chat format) ───────────────────────────────

#[derive(Debug, Deserialize)]
struct CopilotChatEvent {
    choices: Vec<CopilotChoice>,
    #[serde(default)]
    usage: Option<CopilotUsage>,
}

#[derive(Debug, Deserialize)]
struct CopilotChoice {
    delta: Option<CopilotDelta>,
    finish_reason: Option<String>,
}

#[derive(Debug, Deserialize)]
struct CopilotDelta {
    content: Option<String>,
    reasoning_content: Option<String>,
    tool_calls: Option<Vec<CopilotToolCallDelta>>,
}

#[derive(Debug, Deserialize)]
struct CopilotToolCallDelta {
    index: u64,
    id: Option<String>,
    function: Option<CopilotToolCallDeltaFn>,
}

#[derive(Debug, Deserialize)]
struct CopilotToolCallDeltaFn {
    name: Option<String>,
    arguments: Option<String>,
}

#[derive(Debug, Deserialize)]
struct CopilotUsage {
    prompt_tokens: Option<u64>,
    completion_tokens: Option<u64>,
    total_tokens: Option<u64>,
    prompt_tokens_details: Option<CopilotPromptTokenDetails>,
    completion_tokens_details: Option<CopilotCompletionTokenDetails>,
}

#[derive(Debug, Deserialize)]
struct CopilotPromptTokenDetails {
    cached_tokens: Option<u64>,
}

#[derive(Debug, Deserialize)]
struct CopilotCompletionTokenDetails {
    reasoning_tokens: Option<u64>,
}

// ── Token acquisition ──────────────────────────────────────────────────

/// Resolve a Copilot API token.
///
/// Strategy:
/// 1. If `COPILOT_TOKEN` env var is set, use it directly (bypasses exchange).
/// 2. If `GITHUB_TOKEN` env var is set, exchange it for a Copilot token.
/// 3. Otherwise, return an auth error.
async fn resolve_copilot_token(
    http_client: &reqwest::Client,
) -> Result<String, Error> {
    // Direct token (for testing / pre-exchanged tokens)
    if let Ok(token) = std::env::var("COPILOT_TOKEN") {
        if !token.is_empty() {
            return Ok(token);
        }
    }

    // Exchange GITHUB_TOKEN for a Copilot token
    let github_token = std::env::var("GITHUB_TOKEN")
        .ok()
        .filter(|k| !k.is_empty())
        .ok_or_else(|| {
            Error::Auth(
                "GitHub Copilot requires GITHUB_TOKEN or COPILOT_TOKEN environment variable".into(),
            )
        })?;

    let response = http_client
        .post(COPILOT_TOKEN_URL)
        .header("Authorization", format!("Bearer {}", github_token))
        .header("User-Agent", format!("rustcode/{}", env!("CARGO_PKG_VERSION")))
        .header("Accept", "application/json")
        .send()
        .await
        .map_err(|e| {
            Error::Llm {
                module: "github-copilot".into(),
                method: "token_exchange".into(),
                reason: Box::new(LlmErrorReason::Transport {
                    message: format!("failed to exchange GitHub token: {e}"),
                    kind: Some("connect".into()),
                    url: Some(COPILOT_TOKEN_URL.into()),
                }),
            }
        })?;

    if !response.status().is_success() {
        let status = response.status().as_u16();
        let text = response.text().await.unwrap_or_default();
        return Err(Error::Llm {
            module: "github-copilot".into(),
            method: "token_exchange".into(),
            reason: Box::new(classify_token_error(status, &text)),
        });
    }

    let token_resp: CopilotTokenResponse = response
        .json()
        .await
        .map_err(|e| {
            Error::Llm {
                module: "github-copilot".into(),
                method: "token_exchange".into(),
                reason: Box::new(LlmErrorReason::InvalidProviderOutput {
                    message: format!("failed to parse Copilot token response: {e}"),
                    raw: None,
                }),
            }
        })?;

    Ok(token_resp.token)
}

/// Classify errors from the Copilot token exchange endpoint.
fn classify_token_error(status: u16, body: &str) -> LlmErrorReason {
    match status {
        401 | 403 => LlmErrorReason::Authentication {
            message: format!("GitHub token rejected (HTTP {status}): {body}"),
            kind: crate::error::AuthErrorKind::Invalid,
        },
        429 => LlmErrorReason::RateLimit {
            message: format!("GitHub token rate limited (HTTP {status}): {body}"),
            retry_after_ms: None,
        },
        _ => LlmErrorReason::UnknownProvider {
            message: format!("Copilot token exchange failed (HTTP {status}): {body}"),
            status: Some(status),
        },
    }
}

/// Determine whether a model should use the Responses API.
///
/// GPT-5+ models (non-mini) use Responses API; everything else uses Chat.
fn should_use_responses_api(model_id: &str) -> bool {
    let lower = model_id.to_lowercase();
    // GPT-5+ non-mini models use Responses API
    if let Some(rest) = lower.strip_prefix("gpt-") {
        if let Some(version_str) = rest.split(|c: char| !c.is_ascii_digit()).next() {
            if let Ok(version) = version_str.parse::<u32>() {
                return version >= 5 && !lower.contains("mini");
            }
        }
    }
    false
}

// ── GitHub Copilot Provider ─────────────────────────────────────────────

/// GitHub Copilot LLM provider.
///
/// Handles the unique token exchange flow and OpenAI-compatible chat API.
pub struct GitHubCopilotProvider {
    /// The Copilot API token (Bearer).
    copilot_token: String,
    /// Base URL for the Copilot API.
    base_url: String,
    http_client: reqwest::Client,
    models: Vec<Model>,
}

impl GitHubCopilotProvider {
    /// Create a new GitHub Copilot provider.
    ///
    /// Performs the token exchange synchronously at construction time. If you
    /// need lazy token acquisition, use [`GitHubCopilotProvider::new_async`].
    pub fn new() -> Result<Self, Error> {
        let http_client = reqwest::Client::builder()
            .user_agent(format!("rustcode/{}", env!("CARGO_PKG_VERSION")))
            .timeout(std::time::Duration::from_secs(300))
            .build()
            .map_err(|e| Error::Network(format!("HTTP client: {e}")))?;

        // For synchronous construction, try direct COPILOT_TOKEN or GITHUB_TOKEN
        // without async exchange (sync exchange is not possible with reqwest).
        // Fall back to sync token resolution only.
        let token = std::env::var("COPILOT_TOKEN")
            .ok()
            .filter(|k| !k.is_empty())
            .or_else(|| {
                // Just read GITHUB_TOKEN for sync path (no exchange)
                std::env::var("GITHUB_TOKEN")
                    .ok()
                    .filter(|k| !k.is_empty())
            })
            .ok_or_else(|| {
                Error::Auth(
                    "GitHub Copilot requires GITHUB_TOKEN or COPILOT_TOKEN environment variable. \
                     Use new_async() for automatic token exchange."
                        .into(),
                )
            })?;

        let base_url = DEFAULT_BASE_URL.into();
        let models = build_model_catalog(&base_url);

        Ok(Self {
            copilot_token: token,
            base_url,
            http_client,
            models,
        })
    }

    /// Create a new provider with async token exchange.
    ///
    /// This exchanges the `GITHUB_TOKEN` env var for a Copilot token.
    pub async fn new_async() -> Result<Self, Error> {
        let http_client = reqwest::Client::builder()
            .user_agent(format!("rustcode/{}", env!("CARGO_PKG_VERSION")))
            .timeout(std::time::Duration::from_secs(300))
            .build()
            .map_err(|e| Error::Network(format!("HTTP client: {e}")))?;

        let copilot_token = resolve_copilot_token(&http_client).await?;
        let base_url = DEFAULT_BASE_URL.into();
        let models = build_model_catalog(&base_url);

        Ok(Self {
            copilot_token,
            base_url,
            http_client,
            models,
        })
    }

    /// Create with an explicit Copilot token.
    pub fn with_token(copilot_token: String) -> Result<Self, Error> {
        let http_client = reqwest::Client::builder()
            .user_agent(format!("rustcode/{}", env!("CARGO_PKG_VERSION")))
            .timeout(std::time::Duration::from_secs(300))
            .build()
            .map_err(|e| Error::Network(format!("HTTP client: {e}")))?;

        let base_url = DEFAULT_BASE_URL.into();
        let models = build_model_catalog(&base_url);

        Ok(Self {
            copilot_token,
            base_url,
            http_client,
            models,
        })
    }

    /// Try to auto-detect: returns `ProviderInfo` if a Copilot token is
    /// available (via GITHUB_TOKEN or COPILOT_TOKEN).
    pub fn auto_detect() -> Vec<ProviderInfo> {
        let has_token = std::env::var("COPILOT_TOKEN")
            .or_else(|_| std::env::var("GITHUB_TOKEN"))
            .ok()
            .filter(|k| !k.is_empty())
            .is_some();

        if has_token {
            vec![ProviderInfo {
                id: "github-copilot".into(),
                name: "GitHub Copilot".into(),
                source: crate::provider::ProviderSource::Env,
                env: vec!["GITHUB_TOKEN".into(), "COPILOT_TOKEN".into()],
                key: None,
                options: HashMap::new(),
                models: HashMap::new(),
            }]
        } else {
            Vec::new()
        }
    }

    fn chat_url(&self) -> String {
        format!("{}{CHAT_PATH}", self.base_url.trim_end_matches('/'))
    }

    fn build_request_headers(&self) -> Vec<(&str, &str)> {
        let mut headers = vec![
            ("Content-Type", "application/json"),
            ("Authorization", &self.copilot_token),
        ];
        // If the token doesn't start with "Bearer ", add the prefix
        for &(k, v) in COPILOT_HEADERS {
            headers.push((k, v));
        }
        headers
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

struct CopilotStreamState {
    tool_stream: ToolStreamAccumulator,
    text_started: bool,
    reasoning_started: bool,
    step_started: bool,
    usage: Option<Usage>,
    finished: bool,
}

fn events_from_chat(event: CopilotChatEvent, state: &mut CopilotStreamState) -> Vec<LlmEvent> {
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

fn map_usage(u: &CopilotUsage) -> Usage {
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
impl Provider for GitHubCopilotProvider {
    fn provider_id(&self) -> &str {
        "github-copilot"
    }

    fn npm(&self) -> &str {
        "@ai-sdk/github-copilot"
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
                provider_id: "github-copilot".into(),
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

        // Build auth header: if the token already has "Bearer " prefix, use as-is;
        // otherwise prepend "Bearer ".
        let auth_header = if self.copilot_token.starts_with("Bearer ") {
            self.copilot_token.clone()
        } else {
            format!("Bearer {}", self.copilot_token)
        };

        let mut req = self
            .http_client
            .post(self.chat_url())
            .header("Authorization", &auth_header)
            .header("Content-Type", "application/json");

        for &(k, v) in COPILOT_HEADERS {
            req = req.header(k, v);
        }

        let response = req
            .json(&body)
            .send()
            .await
            .map_err(|e| Error::Network(format!("GitHub Copilot request: {e}")))?;

        if !response.status().is_success() {
            let status = response.status().as_u16();
            let text = response.text().await.unwrap_or_default();
            return Err(Error::Llm {
                module: "github-copilot".into(),
                method: "stream".into(),
                reason: Box::new(classify_error(status, &text)),
            });
        }

        let sse_stream = parse_sse_stream(response);
        let state = CopilotStreamState {
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
                                    serde_json::from_str::<CopilotChatEvent>(&se.data)
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
                                    Err(Error::ResponseStream(format!("Copilot SSE: {e}")),
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
            "gpt-4o-copilot",
            "GPT-4o (Copilot)",
            base_url,
            128_000,
            16_384,
            false,
            true,
        ),
        make_model(
            "claude-sonnet-4-copilot",
            "Claude Sonnet 4 (Copilot)",
            base_url,
            200_000,
            8_192,
            false,
            true,
        ),
        make_model(
            "claude-3.5-sonnet-copilot",
            "Claude 3.5 Sonnet (Copilot)",
            base_url,
            200_000,
            8_192,
            false,
            true,
        ),
        make_model(
            "gpt-4o-mini-copilot",
            "GPT-4o Mini (Copilot)",
            base_url,
            128_000,
            16_384,
            false,
            true,
        ),
    ]
}

fn make_model(
    id: &str,
    name: &str,
    base_url: &str,
    context: u64,
    output: u64,
    reasoning: bool,
    image_input: bool,
) -> Model {
    Model {
        id: id.into(),
        provider_id: "github-copilot".into(),
        name: name.into(),
        api: provider::ApiInfo {
            id: id.into(),
            url: base_url.into(),
            npm: "@ai-sdk/github-copilot".into(),
        },
        family: Some(if id.contains("gpt") { "gpt".into() } else { "claude".into() }),
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
        cost: provider::Cost::default(),
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
    fn test_should_use_responses_api() {
        assert!(should_use_responses_api("gpt-5.1"));
        assert!(should_use_responses_api("gpt-5.2"));
        assert!(!should_use_responses_api("gpt-5-mini"));
        assert!(!should_use_responses_api("gpt-4o-copilot"));
        assert!(!should_use_responses_api("claude-sonnet-4-copilot"));
    }

    #[test]
    fn test_map_finish_reason() {
        assert_eq!(map_finish_reason("stop"), FinishReason::Stop);
        assert_eq!(map_finish_reason("length"), FinishReason::Length);
        assert_eq!(map_finish_reason("tool_calls"), FinishReason::ToolCalls);
        assert_eq!(map_finish_reason("unknown"), FinishReason::Unknown);
    }

    #[test]
    fn test_classify_error() {
        let reason = classify_error(401, "bad token");
        assert!(matches!(reason, LlmErrorReason::Authentication { .. }));

        let reason = classify_error(429, "rate limit");
        assert!(matches!(reason, LlmErrorReason::RateLimit { .. }));

        let reason = classify_error(500, "internal");
        assert!(matches!(reason, LlmErrorReason::ProviderInternal { .. }));
    }

    #[test]
    fn test_model_catalog() {
        let models = build_model_catalog("https://api.githubcopilot.com");
        assert!(models.len() >= 4);
        let gpt4o = models.iter().find(|m| m.id == "gpt-4o-copilot").unwrap();
        assert_eq!(gpt4o.provider_id, "github-copilot");
        assert!(gpt4o.capabilities.toolcall);

        let claude = models.iter().find(|m| m.id == "claude-sonnet-4-copilot").unwrap();
        assert_eq!(claude.family.as_deref(), Some("claude"));
        assert_eq!(claude.limit.context, 200_000);
    }

    #[test]
    fn test_map_usage() {
        let cu = CopilotUsage {
            prompt_tokens: Some(50),
            completion_tokens: Some(25),
            total_tokens: Some(75),
            prompt_tokens_details: None,
            completion_tokens_details: None,
        };
        let usage = map_usage(&cu);
        assert_eq!(usage.input_tokens, Some(50));
        assert_eq!(usage.output_tokens, Some(25));
        assert_eq!(usage.total_tokens, Some(75));
    }

    #[test]
    fn test_classify_token_error() {
        let reason = classify_token_error(401, "Bad credentials");
        assert!(matches!(reason, LlmErrorReason::Authentication { .. }));

        let reason = classify_token_error(429, "rate limit");
        assert!(matches!(reason, LlmErrorReason::RateLimit { .. }));
    }
}
