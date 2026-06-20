//! Generic OpenAI-compatible Chat provider.
//!
//! A single, configurable [`OpenAICompatibleProvider`] that covers any provider
//! speaking the OpenAI Chat Completions wire format: DeepSeek, Groq, TogetherAI,
//! xAI, Mistral, GitHub Copilot, Cerebras, Fireworks, AI21, Cohere, Perplexity,
//! Alibaba, and more.
//!
//! Provider-specific differences (auth headers, extra headers, URL construction,
//! model catalogs) are expressed via [`CompatConfig`] — no per-provider code
//! duplication required.
//!
//! Ported from:
//! - `packages/llm/src/protocols/openai-compatible-chat.ts` (25 lines)
//! - `packages/llm/src/providers/openai-compatible.ts` (66 lines)
//! - `packages/llm/src/providers/openai-compatible-profile.ts` (21 lines)

use crate::error::{Error, LlmErrorReason};
use crate::provider::{
    self, ChatMessage, ContentPart, FinishReason, LlmEvent, MessageContent, Model, Provider,
    ToolDefinition, Usage,
};
use crate::tool_stream::ToolStreamAccumulator;
use async_trait::async_trait;
use futures::StreamExt;
use std::collections::{HashMap, VecDeque};
use std::pin::Pin;

// ── Configuration ───────────────────────────────────────────────────────

/// Pre-defined model for a provider.
#[derive(Debug, Clone)]
pub struct ModelSpec {
    pub id: &'static str,
    pub name: &'static str,
    pub ctx: u64,
    pub out: u64,
    pub family: Option<&'static str>,
    pub reasoning: bool,
    pub image_input: bool,
}

/// Configuration for an OpenAI-compatible provider.
///
/// All provider-specific differences are captured here. The generic
/// [`OpenAICompatibleProvider`] uses this config to handle auth, URLs,
/// error classification, and model catalogs.
#[derive(Debug, Clone)]
pub struct CompatConfig {
    pub provider_id: &'static str,
    pub name: &'static str,
    pub npm: &'static str,
    pub base_url: &'static str,
    pub env_var: &'static str,
    pub models: &'static [ModelSpec],
    /// Extra HTTP headers to send with every request (e.g. Copilot headers).
    pub extra_headers: &'static [(&'static str, &'static str)],
    /// Classify an HTTP error status + body into an [`LlmErrorReason`].
    pub classify_error: fn(u16, &str) -> LlmErrorReason,
}

/// Build a [`Model`] from a [`ModelSpec`].
fn build_model(spec: &ModelSpec, cfg: &CompatConfig, base_url: &str) -> Model {
    Model {
        id: spec.id.into(),
        provider_id: cfg.provider_id.into(),
        name: spec.name.into(),
        api: provider::ApiInfo {
            id: spec.id.into(),
            url: base_url.into(),
            npm: cfg.npm.into(),
        },
        family: spec.family.map(|f| f.into()),
        capabilities: provider::Capabilities {
            temperature: true,
            reasoning: spec.reasoning,
            attachment: false,
            toolcall: true,
            input: provider::Modality {
                text: true,
                image: spec.image_input,
                ..Default::default()
            },
            output: provider::Modality {
                text: true,
                ..Default::default()
            },
            ..Default::default()
        },
        cost: provider::Cost::default(),
        limit: provider::TokenLimit {
            context: spec.ctx,
            input: None,
            output: spec.out,
        },
        status: provider::ModelStatus::Active,
        options: HashMap::new(),
        headers: HashMap::new(),
        release_date: "2025".into(),
        variants: None,
    }
}

// ── Error classification (shared) ──────────────────────────────────────

/// Default error classifier for OpenAI-compatible APIs.
pub fn default_classify_error(status: u16, body: &str) -> LlmErrorReason {
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

// ── Provider profiles ──────────────────────────────────────────────────

/// Profiles for all Bearer-token OpenAI-compatible providers.
///
/// Each profile defines a complete [`CompatConfig`] including model catalog.
pub const PROFILES: &[CompatConfig] = &[
    CompatConfig {
        provider_id: "deepseek",
        name: "DeepSeek",
        npm: "@ai-sdk/deepseek",
        base_url: "https://api.deepseek.com/v1",
        env_var: "DEEPSEEK_API_KEY",
        models: &[
            ModelSpec { id: "deepseek-chat", name: "DeepSeek Chat", ctx: 128_000, out: 8_192, family: Some("chat"), reasoning: false, image_input: false },
            ModelSpec { id: "deepseek-reasoner", name: "DeepSeek Reasoner", ctx: 128_000, out: 8_192, family: Some("reasoner"), reasoning: true, image_input: false },
            ModelSpec { id: "deepseek-v3", name: "DeepSeek V3", ctx: 128_000, out: 8_192, family: Some("v3"), reasoning: true, image_input: false },
        ],
        extra_headers: &[],
        classify_error: default_classify_error,
    },
    CompatConfig {
        provider_id: "groq",
        name: "Groq",
        npm: "@ai-sdk/groq",
        base_url: "https://api.groq.com/openai/v1",
        env_var: "GROQ_API_KEY",
        models: &[
            ModelSpec { id: "llama-4-maverick", name: "Llama 4 Maverick", ctx: 128_000, out: 8_192, family: Some("llama"), reasoning: false, image_input: false },
            ModelSpec { id: "llama-3.3-70b-versatile", name: "Llama 3.3 70B", ctx: 128_000, out: 8_192, family: Some("llama"), reasoning: false, image_input: false },
            ModelSpec { id: "llama-3.1-8b-instant", name: "Llama 3.1 8B", ctx: 128_000, out: 8_192, family: Some("llama"), reasoning: false, image_input: false },
            ModelSpec { id: "gemma2-9b-it", name: "Gemma 2 9B", ctx: 8_192, out: 8_192, family: Some("gemma"), reasoning: false, image_input: false },
            ModelSpec { id: "mixtral-8x7b-32768", name: "Mixtral 8x7B", ctx: 32_768, out: 4_096, family: Some("mixtral"), reasoning: false, image_input: false },
        ],
        extra_headers: &[],
        classify_error: default_classify_error,
    },
    CompatConfig {
        provider_id: "togetherai",
        name: "Together AI",
        npm: "@ai-sdk/togetherai",
        base_url: "https://api.together.xyz/v1",
        env_var: "TOGETHER_API_KEY",
        models: &[
            ModelSpec { id: "meta-llama/Llama-4-Maverick-17B-128E-Instruct-FP8", name: "Llama 4 Maverick", ctx: 128_000, out: 8_192, family: Some("llama"), reasoning: false, image_input: false },
            ModelSpec { id: "meta-llama/Llama-3.3-70B-Instruct-Turbo", name: "Llama 3.3 70B", ctx: 128_000, out: 8_192, family: Some("llama"), reasoning: false, image_input: false },
            ModelSpec { id: "Qwen/Qwen3-235B-A22B-Thinking-2507", name: "Qwen3 235B", ctx: 128_000, out: 8_192, family: Some("qwen"), reasoning: true, image_input: false },
            ModelSpec { id: "deepseek-ai/DeepSeek-V3-0324", name: "DeepSeek V3", ctx: 128_000, out: 8_192, family: Some("deepseek"), reasoning: false, image_input: false },
            ModelSpec { id: "mistralai/Mistral-Small-3.1-24B-Instruct-2503", name: "Mistral Small 3.1", ctx: 128_000, out: 8_192, family: Some("mistral"), reasoning: false, image_input: true },
            ModelSpec { id: "Qwen/Qwen-2.5-72B-Instruct-Turbo", name: "Qwen 2.5 72B", ctx: 32_768, out: 8_192, family: Some("qwen"), reasoning: false, image_input: false },
        ],
        extra_headers: &[],
        classify_error: default_classify_error,
    },
    CompatConfig {
        provider_id: "xai",
        name: "xAI Grok",
        npm: "@ai-sdk/xai",
        base_url: "https://api.x.ai/v1",
        env_var: "XAI_API_KEY",
        models: &[
            ModelSpec { id: "grok-4", name: "Grok 4", ctx: 1_000_000, out: 128_000, family: Some("grok"), reasoning: true, image_input: true },
            ModelSpec { id: "grok-3", name: "Grok 3", ctx: 128_000, out: 16_384, family: Some("grok"), reasoning: false, image_input: true },
            ModelSpec { id: "grok-3-mini", name: "Grok 3 Mini", ctx: 128_000, out: 16_384, family: Some("grok"), reasoning: true, image_input: false },
            ModelSpec { id: "grok-3-fast", name: "Grok 3 Fast", ctx: 128_000, out: 16_384, family: Some("grok"), reasoning: false, image_input: true },
            ModelSpec { id: "grok-2", name: "Grok 2", ctx: 128_000, out: 16_384, family: Some("grok"), reasoning: false, image_input: true },
        ],
        extra_headers: &[],
        classify_error: default_classify_error,
    },
    CompatConfig {
        provider_id: "mistral",
        name: "Mistral",
        npm: "@ai-sdk/mistral",
        base_url: "https://api.mistral.ai/v1",
        env_var: "MISTRAL_API_KEY",
        models: &[
            ModelSpec { id: "mistral-large-latest", name: "Mistral Large", ctx: 128_000, out: 128_000, family: Some("mistral"), reasoning: false, image_input: true },
            ModelSpec { id: "mistral-small-latest", name: "Mistral Small", ctx: 128_000, out: 32_768, family: Some("mistral"), reasoning: false, image_input: true },
            ModelSpec { id: "codestral-latest", name: "Codestral", ctx: 32_768, out: 32_768, family: Some("codestral"), reasoning: false, image_input: false },
            ModelSpec { id: "pixtral-large-latest", name: "Pixtral Large", ctx: 128_000, out: 128_000, family: Some("pixtral"), reasoning: false, image_input: true },
            ModelSpec { id: "open-mistral-nemo", name: "Open Mistral Nemo", ctx: 128_000, out: 32_768, family: Some("mistral"), reasoning: false, image_input: false },
            ModelSpec { id: "ministral-8b-latest", name: "Ministral 8B", ctx: 32_768, out: 8_192, family: Some("mistral"), reasoning: false, image_input: false },
        ],
        extra_headers: &[],
        classify_error: default_classify_error,
    },
    CompatConfig {
        provider_id: "github_copilot",
        name: "GitHub Copilot",
        npm: "@ai-sdk/copilot",
        base_url: "https://api.githubcopilot.com",
        env_var: "GITHUB_TOKEN",
        models: &[
            ModelSpec { id: "gpt-4o", name: "GPT-4o", ctx: 128_000, out: 16_384, family: Some("gpt"), reasoning: false, image_input: true },
            ModelSpec { id: "claude-sonnet-4", name: "Claude Sonnet 4", ctx: 200_000, out: 8_192, family: Some("claude"), reasoning: false, image_input: true },
        ],
        extra_headers: &[
            ("Copilot-Integration-Id", "vscode"),
            ("Editor-Version", "vscode/1.95.0"),
        ],
        classify_error: default_classify_error,
    },
    CompatConfig {
        provider_id: "cerebras",
        name: "Cerebras",
        npm: "@ai-sdk/cerebras",
        base_url: "https://api.cerebras.ai/v1",
        env_var: "CEREBRAS_API_KEY",
        models: &[
            ModelSpec { id: "llama-4-maverick", name: "Llama 4 Maverick", ctx: 128_000, out: 8_192, family: Some("llama"), reasoning: false, image_input: false },
        ],
        extra_headers: &[],
        classify_error: default_classify_error,
    },
    CompatConfig {
        provider_id: "fireworks",
        name: "Fireworks",
        npm: "@ai-sdk/fireworks",
        base_url: "https://api.fireworks.ai/inference/v1",
        env_var: "FIREWORKS_API_KEY",
        models: &[
            ModelSpec { id: "accounts/fireworks/models/llama-v3p3-70b-instruct", name: "Llama 3.3 70B", ctx: 128_000, out: 8_192, family: Some("llama"), reasoning: false, image_input: false },
        ],
        extra_headers: &[],
        classify_error: default_classify_error,
    },
    CompatConfig {
        provider_id: "ai21",
        name: "AI21 Labs",
        npm: "@ai-sdk/ai21",
        base_url: "https://api.ai21.com/studio/v1",
        env_var: "AI21_API_KEY",
        models: &[
            ModelSpec { id: "jamba-1.5-large", name: "Jamba 1.5 Large", ctx: 256_000, out: 8_192, family: Some("jamba"), reasoning: false, image_input: false },
        ],
        extra_headers: &[],
        classify_error: default_classify_error,
    },
    CompatConfig {
        provider_id: "cohere",
        name: "Cohere",
        npm: "@ai-sdk/cohere",
        base_url: "https://api.cohere.ai/v1",
        env_var: "COHERE_API_KEY",
        models: &[
            ModelSpec { id: "command-r-plus", name: "Command R+", ctx: 128_000, out: 4_096, family: Some("command-r"), reasoning: false, image_input: false },
        ],
        extra_headers: &[],
        classify_error: default_classify_error,
    },
    CompatConfig {
        provider_id: "perplexity",
        name: "Perplexity",
        npm: "@ai-sdk/perplexity",
        base_url: "https://api.perplexity.ai",
        env_var: "PERPLEXITY_API_KEY",
        models: &[
            ModelSpec { id: "sonar-pro", name: "Sonar Pro", ctx: 200_000, out: 8_192, family: Some("sonar"), reasoning: false, image_input: false },
        ],
        extra_headers: &[],
        classify_error: default_classify_error,
    },
    CompatConfig {
        provider_id: "deepinfra",
        name: "DeepInfra",
        npm: "@ai-sdk/deepinfra",
        base_url: "https://api.deepinfra.com/v1/openai",
        env_var: "DEEPINFRA_API_KEY",
        models: &[
            ModelSpec { id: "meta-llama/Llama-4-Maverick-17B-128E-Instruct-FP8", name: "Llama 4 Maverick", ctx: 128_000, out: 8_192, family: Some("llama"), reasoning: false, image_input: false },
        ],
        extra_headers: &[],
        classify_error: default_classify_error,
    },
    CompatConfig {
        provider_id: "alibaba",
        name: "Alibaba Qwen",
        npm: "@ai-sdk/alibaba",
        base_url: "https://dashscope-intl.aliyuncs.com/compatible-mode/v1",
        env_var: "DASHSCOPE_API_KEY",
        models: &[
            ModelSpec { id: "qwen-max", name: "Qwen Max", ctx: 32_768, out: 8_192, family: Some("qwen"), reasoning: false, image_input: false },
        ],
        extra_headers: &[],
        classify_error: default_classify_error,
    },
    CompatConfig {
        provider_id: "vercel",
        name: "Vercel AI Gateway",
        npm: "@ai-sdk/gateway",
        base_url: "https://api.vercel.ai/v1",
        env_var: "VERCEL_AI_GATEWAY_KEY",
        models: &[
            ModelSpec { id: "auto", name: "Auto", ctx: 128_000, out: 16_384, family: Some("auto"), reasoning: false, image_input: false },
        ],
        extra_headers: &[],
        classify_error: default_classify_error,
    },
];

// ── Provider struct ────────────────────────────────────────────────────

/// Generic OpenAI-compatible provider.
///
/// Handles any provider that speaks the Chat Completions wire protocol
/// with Bearer-token authentication. Provider-specific differences are
/// expressed via [`CompatConfig`].
pub struct OpenAICompatibleProvider {
    config: CompatConfig,
    api_key: String,
    base_url: String,
    http_client: reqwest::Client,
    models: Vec<Model>,
}

impl OpenAICompatibleProvider {
    /// Create from a [`CompatConfig`], reading the API key from the
    /// configured environment variable.
    pub fn from_config(config: &CompatConfig) -> Result<Self, Error> {
        let api_key = std::env::var(config.env_var)
            .ok()
            .filter(|k| !k.is_empty())
            .ok_or_else(|| Error::Auth(format!("{} not set", config.env_var)))?;
        let base_url = config.base_url.to_string();
        let models = config
            .models
            .iter()
            .map(|m| build_model(m, config, &base_url))
            .collect();
        let http_client = reqwest::Client::builder()
            .user_agent(format!("rustcode/{}", env!("CARGO_PKG_VERSION")))
            .timeout(std::time::Duration::from_secs(300))
            .build()
            .map_err(|e| Error::Network(format!("HTTP client: {e}")))?;
        Ok(Self {
            config: config.clone(),
            api_key,
            base_url,
            http_client,
            models,
        })
    }

    /// Try to auto-detect all providers from environment variables.
    pub fn try_all() -> Vec<Self> {
        PROFILES
            .iter()
            .filter_map(|p| Self::from_config(p).ok())
            .collect()
    }

    fn chat_url(&self) -> String {
        format!("{}/chat/completions", self.base_url.trim_end_matches('/'))
    }

    fn build_request_headers(&self) -> Vec<(&str, &str)> {
        let mut headers = vec![("Content-Type", "application/json")];
        for &(k, v) in self.config.extra_headers {
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
                    serde_json::json!({"role":"system","content": msg_text(content)})
                }
                ChatMessage::User { content } => {
                    serde_json::json!({"role":"user","content": msg_text(content)})
                }
                ChatMessage::Assistant { content } => {
                    let mut tool_calls_arr = Vec::new();
                    let mut text = String::new();
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
                        _ => {}
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
                    let mut arr = Vec::new();
                    for p in content {
                        let crate::provider::ToolResultPart::ToolResult {
                            tool_call_id,
                            output,
                            ..
                        } = p;
                        arr.push(serde_json::json!({
                            "role":"tool",
                            "tool_call_id": tool_call_id,
                            "content": output.to_string()
                        }));
                    }
                    arr.first()
                        .cloned()
                        .unwrap_or(serde_json::json!({"role":"tool","tool_call_id":"","content":""}))
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

fn msg_text(content: &MessageContent) -> String {
    match content {
        MessageContent::Text(t) => t.clone(),
        MessageContent::Parts(p) => p
            .iter()
            .filter_map(|p| {
                if let ContentPart::Text { text } = p {
                    Some(text.as_str())
                } else {
                    None
                }
            })
            .collect::<Vec<_>>()
            .join(""),
    }
}

// ── SSE event parsing (shared) ─────────────────────────────────────────

struct CompatStreamState {
    tool_stream: ToolStreamAccumulator,
    text_started: bool,
    reasoning_started: bool,
    step_started: bool,
    usage: Option<Usage>,
    finished: bool,
}

fn events_from_chat(event: &serde_json::Value, state: &mut CompatStreamState) -> Vec<LlmEvent> {
    let mut events = Vec::new();

    if let Some(usage_val) = event.get("usage") {
        if let Ok(u) = serde_json::from_value::<CompatUsage>(usage_val.clone()) {
            state.usage = Some(map_usage(&u));
        }
    }

    let choice = event
        .get("choices")
        .and_then(|c| c.as_array())
        .and_then(|c| c.first());

    if let Some(delta) = choice.and_then(|c| c.get("delta")) {
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

        if let Some(content) = delta.get("content").and_then(|c| c.as_str()) {
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

        if let Some(tool_calls) = delta.get("tool_calls").and_then(|t| t.as_array()) {
            for td in tool_calls {
                let index = td.get("index").and_then(|i| i.as_u64()).unwrap_or(0) as u32;
                let name = td
                    .get("function")
                    .and_then(|f| f.get("name"))
                    .and_then(|n| n.as_str())
                    .map(|s| s.to_string());
                let id = td
                    .get("id")
                    .and_then(|i| i.as_str())
                    .map(|s| s.to_string());
                let args = td
                    .get("function")
                    .and_then(|f| f.get("arguments"))
                    .and_then(|a| a.as_str())
                    .map(|s| s.to_string());

                if let Some(name) = name {
                    state
                        .tool_stream
                        .set_identity(index, name, id.unwrap_or_default());
                }
                if let Some(args) = args {
                    if let Some(ev) = state.tool_stream.append(index, &args) {
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

    if let Some(finish_reason) = choice
        .and_then(|c| c.get("finish_reason"))
        .and_then(|r| r.as_str())
    {
        for tool_ev in state.tool_stream.finish_all() {
            events.push(tool_ev);
        }

        let reason = match finish_reason {
            "stop" => FinishReason::Stop,
            "length" => FinishReason::Length,
            "tool_calls" | "function_call" => FinishReason::ToolCalls,
            "content_filter" => FinishReason::ContentFilter,
            _ => FinishReason::Unknown,
        };

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
        events.push(LlmEvent::Finish {
            reason,
            usage: state.usage.clone(),
            provider_metadata: None,
        });
        state.finished = true;
    }

    events
}

#[derive(serde::Deserialize)]
struct CompatUsage {
    prompt_tokens: Option<u64>,
    completion_tokens: Option<u64>,
    total_tokens: Option<u64>,
    prompt_tokens_details: Option<CompatUsageDetails>,
    completion_tokens_details: Option<CompatUsageDetails>,
}

#[derive(serde::Deserialize)]
struct CompatUsageDetails {
    cached_tokens: Option<u64>,
    reasoning_tokens: Option<u64>,
}

fn map_usage(u: &CompatUsage) -> Usage {
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

// ── Provider trait impl ────────────────────────────────────────────────

#[async_trait]
impl Provider for OpenAICompatibleProvider {
    fn provider_id(&self) -> &str {
        self.config.provider_id
    }
    fn npm(&self) -> &str {
        self.config.npm
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
                provider_id: self.config.provider_id.into(),
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

        let url = self.chat_url();
        let auth_header = format!("Bearer {}", self.api_key);

        let mut req = self
            .http_client
            .post(&url)
            .header("Authorization", &auth_header);
        for &(k, v) in &self.build_request_headers() {
            req = req.header(k, v);
        }
        let response = req
            .json(&body)
            .send()
            .await
            .map_err(|e| Error::Network(format!("{} request: {e}", self.config.provider_id)))?;

        if !response.status().is_success() {
            let status = response.status().as_u16();
            let text = response.text().await.unwrap_or_default();
            return Err(Error::Llm {
                module: self.config.provider_id.into(),
                method: "stream".into(),
                reason: Box::new((self.config.classify_error)(status, &text)),
            });
        }

        let sse_stream = crate::sse::parse_sse_stream(response);
        let provider_id = self.config.provider_id.to_string();
        let state = CompatStreamState {
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
            move |(mut sse, mut state, mut buf)| {
                let pid = provider_id.clone();
                Box::pin(async move {
                    loop {
                        if let Some(ev) = buf.pop_front() {
                            return Some((ev, (sse, state, buf)));
                        }
                        if state.finished {
                            return None;
                        }
                        match sse.next().await {
                            Some(Ok(se)) if !se.is_done() && se.has_data() => {
                                if let Ok(ce) =
                                    serde_json::from_str::<serde_json::Value>(&se.data)
                                {
                                    for ev in events_from_chat(&ce, &mut state) {
                                        buf.push_back(Ok(ev));
                                    }
                                }
                            }
                            Some(Err(e)) => {
                                return Some((
                                    Err(Error::ResponseStream(format!("{pid} SSE: {e}"))),
                                    (sse, state, buf),
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
