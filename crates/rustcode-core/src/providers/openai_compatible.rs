//! OpenAI-Compatible Chat provider — covers DeepSeek, Groq, TogetherAI,
//! Cerebras, Fireworks, DeepInfra, Baseten, Cloudflare Workers AI,
//! Cloudflare AI Gateway, Perplexity, Mistral, xAI, and any other
//! provider that speaks the OpenAI Chat Completions wire format.
//!
//! Ported from:
//! - `packages/llm/src/protocols/openai-compatible-chat.ts` (25 lines)
//! - `packages/llm/src/providers/openai-compatible.ts` (66 lines)
//! - `packages/llm/src/providers/openai-compatible-profile.ts` (21 lines)

use async_trait::async_trait;
use futures::StreamExt;
use std::collections::{HashMap, VecDeque};
use std::pin::Pin;

use crate::error::{Error, LlmErrorReason};
use crate::provider::{
    ChatMessage, FinishReason, LlmEvent, Model, Provider, ToolDefinition, Usage,
};

/// Pre-configured profiles for popular OpenAI-compatible providers.
#[derive(Debug, Clone)]
pub struct CompatProfile {
    pub provider_id: &'static str,
    pub name: &'static str,
    pub base_url: &'static str,
    pub env_var: &'static str,
    pub npm: &'static str,
    pub default_model: &'static str,
    pub family: &'static str,
}

pub const PROFILES: &[CompatProfile] = &[
    CompatProfile {
        provider_id: "deepseek",
        name: "DeepSeek",
        base_url: "https://api.deepseek.com/v1",
        env_var: "DEEPSEEK_API_KEY",
        npm: "@ai-sdk/deepseek",
        default_model: "deepseek-chat",
        family: "deepseek",
    },
    CompatProfile {
        provider_id: "groq",
        name: "Groq",
        base_url: "https://api.groq.com/openai/v1",
        env_var: "GROQ_API_KEY",
        npm: "@ai-sdk/groq",
        default_model: "llama-4-maverick",
        family: "llama",
    },
    CompatProfile {
        provider_id: "togetherai",
        name: "TogetherAI",
        base_url: "https://api.together.xyz/v1",
        env_var: "TOGETHER_API_KEY",
        npm: "@ai-sdk/togetherai",
        default_model: "mistral-large",
        family: "mistral",
    },
    CompatProfile {
        provider_id: "cerebras",
        name: "Cerebras",
        base_url: "https://api.cerebras.ai/v1",
        env_var: "CEREBRAS_API_KEY",
        npm: "@ai-sdk/cerebras",
        default_model: "llama-4-maverick",
        family: "llama",
    },
    CompatProfile {
        provider_id: "fireworks",
        name: "Fireworks",
        base_url: "https://api.fireworks.ai/inference/v1",
        env_var: "FIREWORKS_API_KEY",
        npm: "@ai-sdk/fireworks",
        default_model: "llama-4-maverick",
        family: "llama",
    },
    CompatProfile {
        provider_id: "deepinfra",
        name: "DeepInfra",
        base_url: "https://api.deepinfra.com/v1/openai",
        env_var: "DEEPINFRA_API_KEY",
        npm: "@ai-sdk/deepinfra",
        default_model: "llama-4-maverick",
        family: "llama",
    },
    CompatProfile {
        provider_id: "xai",
        name: "xAI Grok",
        base_url: "https://api.x.ai/v1",
        env_var: "XAI_API_KEY",
        npm: "@ai-sdk/xai",
        default_model: "grok-4",
        family: "grok",
    },
    CompatProfile {
        provider_id: "mistral",
        name: "Mistral",
        base_url: "https://api.mistral.ai/v1",
        env_var: "MISTRAL_API_KEY",
        npm: "@ai-sdk/mistral",
        default_model: "mistral-large",
        family: "mistral",
    },
    CompatProfile {
        provider_id: "perplexity",
        name: "Perplexity",
        base_url: "https://api.perplexity.ai",
        env_var: "PERPLEXITY_API_KEY",
        npm: "@ai-sdk/perplexity",
        default_model: "sonar-pro",
        family: "sonar",
    },
    CompatProfile {
        provider_id: "cohere",
        name: "Cohere",
        base_url: "https://api.cohere.ai/v1",
        env_var: "COHERE_API_KEY",
        npm: "@ai-sdk/cohere",
        default_model: "command-r-plus",
        family: "command-r",
    },
    CompatProfile {
        provider_id: "alibaba",
        name: "Alibaba Qwen",
        base_url: "https://dashscope-intl.aliyuncs.com/compatible-mode/v1",
        env_var: "DASHSCOPE_API_KEY",
        npm: "@ai-sdk/alibaba",
        default_model: "qwen-max",
        family: "qwen",
    },
    CompatProfile {
        provider_id: "vercel",
        name: "Vercel AI Gateway",
        base_url: "https://api.vercel.ai/v1",
        env_var: "VERCEL_AI_GATEWAY_KEY",
        npm: "@ai-sdk/gateway",
        default_model: "auto",
        family: "auto",
    },
];

/// OpenAI-compatible provider. Uses the same Chat Completions wire protocol
/// as OpenAI but with a different base URL and auth.
///
/// Reuses the same SSE event parser and body builder from the OpenAI provider
/// but wraps them with profile-specific configuration.
pub struct OpenAICompatibleProvider {
    provider_id: String,
    name: String,
    api_key: String,
    base_url: String,
    npm: String,
    http_client: reqwest::Client,
    models: Vec<Model>,
}

impl OpenAICompatibleProvider {
    /// Create from a known profile.
    pub fn from_profile(profile: &CompatProfile) -> Result<Self, Error> {
        let api_key = std::env::var(profile.env_var)
            .ok()
            .filter(|k| !k.is_empty())
            .ok_or_else(|| {
                Error::Auth(format!("{} environment variable not set", profile.env_var))
            })?;
        Self::new(
            &profile.provider_id,
            &profile.name,
            &api_key,
            &profile.base_url,
            &profile.npm,
            &profile.default_model,
            &profile.family,
        )
    }

    /// Try to auto-detect a provider from environment.
    pub fn try_all() -> Vec<Self> {
        PROFILES
            .iter()
            .filter_map(|p| Self::from_profile(p).ok())
            .collect()
    }

    /// Generic constructor.
    pub fn new(
        provider_id: &str,
        name: &str,
        api_key: &str,
        base_url: &str,
        _npm: &str,
        default_model: &str,
        family: &str,
    ) -> Result<Self, Error> {
        let http_client = reqwest::Client::builder()
            .user_agent(format!("rustcode/{}", env!("CARGO_PKG_VERSION")))
            .timeout(std::time::Duration::from_secs(300))
            .build()
            .map_err(|e| Error::Network(format!("HTTP client: {e}")))?;
        let models = vec![make_simple_model(
            default_model,
            name,
            provider_id,
            base_url,
            family,
            128_000,
            16_384,
        )];
        Ok(Self {
            provider_id: provider_id.into(),
            name: name.into(),
            api_key: api_key.into(),
            base_url: base_url.into(),
            npm: format!("@ai-sdk/{provider_id}"),
            http_client,
            models,
        })
    }

    fn chat_url(&self) -> String {
        format!("{}/chat/completions", self.base_url.trim_end_matches('/'))
    }

    /// Get a reference to the inner OpenAI-style body builder.
    /// We build the body manually using the same format as OpenAI Chat.
    fn build_body(
        &self,
        model: &Model,
        messages: &[ChatMessage],
        tools: &[ToolDefinition],
    ) -> serde_json::Value {
        use crate::provider::MessageContent;
        let msgs: Vec<serde_json::Value> = messages.iter().map(|m| match m {
            ChatMessage::System { content } => serde_json::json!({"role":"system","content": msg_text(content)}),
            ChatMessage::User { content } => serde_json::json!({"role":"user","content": msg_text(content)}),
            ChatMessage::Assistant { content } => {
                let text = msg_text(content);
                let mut obj = serde_json::json!({"role":"assistant"});
                if !text.is_empty() { obj["content"] = serde_json::Value::String(text); } else { obj["content"] = serde_json::Value::Null; }
                obj
            },
            ChatMessage::Tool { content } => {
                let mut arr = Vec::new();
                for p in content {
                    let crate::provider::ToolResultPart::ToolResult { tool_call_id, output, .. } = p;
                    arr.push(serde_json::json!({"role":"tool","tool_call_id":tool_call_id,"content":output.to_string()}));
                }
                arr.first().cloned().unwrap_or(serde_json::json!({"role":"tool","tool_call_id":"","content":""}))
            },
        }).collect();

        let tools_arr: Vec<serde_json::Value> = tools.iter().map(|t| serde_json::json!({
            "type":"function","function":{"name":t.name,"description":t.description,"parameters":t.parameters}
        })).collect();

        let mut body = serde_json::json!({
            "model": model.api.id,
            "messages": msgs,
            "stream": true,
            "stream_options": {"include_usage": true},
            "max_tokens": crate::provider::max_output_tokens(model, crate::provider::OUTPUT_TOKEN_MAX),
        });
        if !tools_arr.is_empty() {
            body["tools"] = serde_json::Value::Array(tools_arr);
        }
        body
    }
}

fn msg_text(content: &crate::provider::MessageContent) -> String {
    match content {
        crate::provider::MessageContent::Text(t) => t.clone(),
        crate::provider::MessageContent::Parts(p) => p
            .iter()
            .filter_map(|p| {
                if let crate::provider::ContentPart::Text { text } = p {
                    Some(text.as_str())
                } else {
                    None
                }
            })
            .collect::<Vec<_>>()
            .join(""),
    }
}

pub(crate) fn make_simple_model(
    id: &str,
    name: &str,
    provider_id: &str,
    base_url: &str,
    family: &str,
    ctx: u64,
    out: u64,
) -> Model {
    Model {
        id: id.into(),
        provider_id: provider_id.into(),
        name: name.into(),
        api: crate::provider::ApiInfo {
            id: id.into(),
            url: base_url.into(),
            npm: format!("@ai-sdk/{provider_id}"),
        },
        family: Some(family.into()),
        capabilities: crate::provider::Capabilities {
            temperature: true,
            reasoning: false,
            attachment: false,
            toolcall: true,
            ..Default::default()
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

// ── OpenAI Chat Event Types (reused) ──────────────────────────────────

#[derive(Debug, serde::Deserialize)]
struct CompatChatEvent {
    choices: Vec<serde_json::Value>,
    #[serde(default)]
    usage: Option<serde_json::Value>,
}

// ── Provider impl ─────────────────────────────────────────────────────

#[async_trait]
impl Provider for OpenAICompatibleProvider {
    fn provider_id(&self) -> &str {
        &self.provider_id
    }
    fn npm(&self) -> &str {
        &self.npm
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
                provider_id: self.provider_id.clone(),
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
            .map_err(|e| Error::Network(format!("{} request: {e}", self.provider_id)))?;

        if !response.status().is_success() {
            let status = response.status().as_u16();
            let text = response.text().await.unwrap_or_default();
            return Err(Error::Llm {
                module: self.provider_id.clone(),
                method: "stream".into(),
                reason: Box::new(LlmErrorReason::UnknownProvider {
                    message: format!("HTTP {status}: {text}"),
                    status: Some(status),
                }),
            });
        }

        let sse_stream = crate::sse::parse_sse_stream(response);
        let provider_id = self.provider_id.clone();
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
                false,
                String::new(),
                VecDeque::new(),
            ),
            move |(mut sse, mut finished, mut text, mut buf)| {
                let pid = provider_id.clone();
                Box::pin(async move {
                    loop {
                        if let Some(ev) = buf.pop_front() {
                            return Some((ev, (sse, finished, text, buf)));
                        }
                        if finished {
                            return None;
                        }
                        match sse.next().await {
                            Some(Ok(se)) if !se.is_done() && se.has_data() => {
                                if let Ok(ce) = serde_json::from_str::<serde_json::Value>(&se.data)
                                {
                                    // Simple extraction: get delta content from first choice
                                    if let Some(choices) = ce["choices"].as_array() {
                                        if let Some(c) = choices.first() {
                                            if let Some(delta_content) =
                                                c["delta"]["content"].as_str()
                                            {
                                                text.push_str(delta_content);
                                                buf.push_back(Ok(LlmEvent::TextDelta {
                                                    id: "text-0".into(),
                                                    text: delta_content.into(),
                                                    provider_metadata: None,
                                                }));
                                            }
                                            if let Some(fr) = c["finish_reason"].as_str() {
                                                let reason = match fr {
                                                    "stop" => FinishReason::Stop,
                                                    "length" => FinishReason::Length,
                                                    "tool_calls" | "function_call" => {
                                                        FinishReason::ToolCalls
                                                    }
                                                    "content_filter" => FinishReason::ContentFilter,
                                                    _ => FinishReason::Unknown,
                                                };
                                                buf.push_back(Ok(LlmEvent::TextEnd {
                                                    id: "text-0".into(),
                                                    provider_metadata: None,
                                                }));
                                                buf.push_back(Ok(LlmEvent::Finish {
                                                    reason,
                                                    usage: None,
                                                    provider_metadata: None,
                                                }));
                                                finished = true;
                                            }
                                        }
                                    }
                                }
                                if let Some(ev) = buf.pop_front() {
                                    return Some((ev, (sse, finished, text, buf)));
                                }
                            }
                            Some(Err(e)) => {
                                return Some((
                                    Err(Error::ResponseStream(format!("{} SSE: {e}", pid))),
                                    (sse, finished, text, buf),
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
        while let Some(r) = stream.next().await {
            if let Ok(ev) = r {
                events.push(ev);
            }
        }
        Ok(crate::provider::LlmResponse {
            events,
            usage: None,
        })
    }
}
