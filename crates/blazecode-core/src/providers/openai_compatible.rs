//! Generic OpenAI-compatible Chat provider.
//!
//! A single, configurable [`OpenAICompatibleProvider`] that covers any provider
//! speaking the OpenAI Chat Completions wire format: DeepSeek, Groq, TogetherAI,
//! Mistral, Cerebras, Fireworks, AI21, Cohere, Perplexity,
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
use crate::provider::{self, ChatMessage, LlmEvent, Model, Provider, ToolDefinition};
use async_trait::async_trait;
use futures::StreamExt;
use std::collections::{HashMap, VecDeque};
use std::pin::Pin;

use super::chat_completions::{self, BodyOptions, ChatStreamState};

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
            input: provider::Modalities {
                text: true,
                image: spec.image_input,
                ..Default::default()
            },
            output: provider::Modalities {
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
///
/// Delegates to [`chat_completions::classify_error`].
pub fn default_classify_error(status: u16, body: &str) -> LlmErrorReason {
    chat_completions::classify_error(status, body)
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
    // ── Missing providers added for 100% parity ─────────────────────
    CompatConfig {
        provider_id: "nvidia",
        name: "NVIDIA",
        npm: "@ai-sdk/openai-compatible",
        base_url: "https://integrate.api.nvidia.com/v1",
        env_var: "NVIDIA_API_KEY",
        models: &[
            ModelSpec { id: "meta/llama-4-maverick-17b-128e-instruct", name: "Llama 4 Maverick", ctx: 128_000, out: 8_192, family: Some("llama"), reasoning: false, image_input: false },
        ],
        extra_headers: &[
            ("HTTP-Referer", "https://blazecode.ai/"),
            ("X-Title", "blazecode"),
        ],
        classify_error: default_classify_error,
    },
    CompatConfig {
        provider_id: "venice",
        name: "Venice",
        npm: "@ai-sdk/openai-compatible",
        base_url: "https://api.venice.ai/api/v1",
        env_var: "VENICE_API_KEY",
        models: &[
            ModelSpec { id: "llama-3.3-70b", name: "Llama 3.3 70B", ctx: 128_000, out: 8_192, family: Some("llama"), reasoning: false, image_input: false },
        ],
        extra_headers: &[],
        classify_error: default_classify_error,
    },
    CompatConfig {
        provider_id: "gitlab",
        name: "GitLab",
        npm: "@ai-sdk/openai-compatible",
        base_url: "https://gitlab.com/api/v4/projects/{project_id}/ai",
        env_var: "GITLAB_TOKEN",
        models: &[
            ModelSpec { id: "claude-sonnet-4", name: "Claude Sonnet 4", ctx: 200_000, out: 8_192, family: Some("claude"), reasoning: false, image_input: true },
        ],
        extra_headers: &[],
        classify_error: default_classify_error,
    },
    CompatConfig {
        provider_id: "google_vertex",
        name: "Google Vertex AI",
        npm: "@ai-sdk/google-vertex",
        base_url: "https://us-central1-aiplatform.googleapis.com/v1/projects/{project}/locations/us-central1/publishers/google/models",
        env_var: "GOOGLE_APPLICATION_CREDENTIALS",
        models: &[
            ModelSpec { id: "gemini-2.5-pro", name: "Gemini 2.5 Pro", ctx: 1_000_000, out: 65_536, family: Some("gemini"), reasoning: true, image_input: true },
            ModelSpec { id: "gemini-2.5-flash", name: "Gemini 2.5 Flash", ctx: 1_000_000, out: 65_536, family: Some("gemini"), reasoning: true, image_input: true },
        ],
        extra_headers: &[],
        classify_error: default_classify_error,
    },
    CompatConfig {
        provider_id: "snowflake_cortex",
        name: "Snowflake Cortex",
        npm: "@ai-sdk/openai-compatible",
        base_url: "https://account.snowflakecomputing.com/api/v2/cortex/chat/completions",
        env_var: "SNOWFLAKE_ACCOUNT",
        models: &[
            ModelSpec { id: "claude-sonnet-4", name: "Claude Sonnet 4", ctx: 200_000, out: 8_192, family: Some("claude"), reasoning: false, image_input: true },
        ],
        extra_headers: &[],
        classify_error: default_classify_error,
    },
    CompatConfig {
        provider_id: "sap_ai_core",
        name: "SAP AI Core",
        npm: "@ai-sdk/openai-compatible",
        base_url: "https://api.ai.sap.com/v1",
        env_var: "SAP_AI_CORE_CLIENT_ID",
        models: &[
            ModelSpec { id: "gpt-4o", name: "GPT-4o", ctx: 128_000, out: 16_384, family: Some("gpt"), reasoning: false, image_input: true },
        ],
        extra_headers: &[],
        classify_error: default_classify_error,
    },
    CompatConfig {
        provider_id: "kilo",
        name: "Kilo",
        npm: "@ai-sdk/openai-compatible",
        base_url: "https://api.kilo.ai/v1",
        env_var: "KILO_API_KEY",
        models: &[
            ModelSpec { id: "claude-sonnet-4", name: "Claude Sonnet 4", ctx: 200_000, out: 8_192, family: Some("claude"), reasoning: false, image_input: true },
        ],
        extra_headers: &[],
        classify_error: default_classify_error,
    },
    CompatConfig {
        provider_id: "llm_gateway",
        name: "LLM Gateway",
        npm: "@ai-sdk/openai-compatible",
        base_url: "https://gateway.example.com/v1",
        env_var: "LLM_GATEWAY_API_KEY",
        models: &[
            ModelSpec { id: "auto", name: "Auto", ctx: 128_000, out: 16_384, family: Some("auto"), reasoning: false, image_input: false },
        ],
        extra_headers: &[],
        classify_error: default_classify_error,
    },
    CompatConfig {
        provider_id: "cloudflare_ai_gateway",
        name: "Cloudflare AI Gateway",
        npm: "@ai-sdk/openai-compatible",
        base_url: "https://gateway.cloudflare.com/v1",
        env_var: "CLOUDFLARE_AI_GATEWAY_API_KEY",
        models: &[
            ModelSpec { id: "auto", name: "Auto", ctx: 128_000, out: 16_384, family: Some("auto"), reasoning: false, image_input: false },
        ],
        extra_headers: &[],
        classify_error: default_classify_error,
    },
    CompatConfig {
        provider_id: "baseten",
        name: "Baseten",
        npm: "@ai-sdk/openai-compatible",
        base_url: "https://inference.baseten.co/v1",
        env_var: "BASETEN_API_KEY",
        models: &[
            ModelSpec { id: "meta-llama/Llama-4-Maverick-17B-128E-Instruct-FP8", name: "Llama 4 Maverick", ctx: 128_000, out: 8_192, family: Some("llama"), reasoning: false, image_input: false },
        ],
        extra_headers: &[],
        classify_error: default_classify_error,
    },
    CompatConfig {
        provider_id: "azure_cognitive_services",
        name: "Azure Cognitive Services",
        npm: "@ai-sdk/openai-compatible",
        base_url: "https://{resourceName}.cognitiveservices.azure.com/openai",
        env_var: "AZURE_COGNITIVE_SERVICES_KEY",
        models: &[
            ModelSpec { id: "gpt-4o", name: "GPT-4o", ctx: 128_000, out: 16_384, family: Some("gpt"), reasoning: false, image_input: true },
        ],
        extra_headers: &[],
        classify_error: default_classify_error,
    },
    CompatConfig {
        provider_id: "zenmux",
        name: "Zenmux",
        npm: "@ai-sdk/openai-compatible",
        base_url: "https://api.zenmux.ai/v1",
        env_var: "ZENMUX_API_KEY",
        models: &[
            ModelSpec { id: "auto", name: "Auto", ctx: 128_000, out: 16_384, family: Some("auto"), reasoning: false, image_input: false },
        ],
        extra_headers: &[],
        classify_error: default_classify_error,
    },
    CompatConfig {
        provider_id: "google_vertex_anthropic",
        name: "Google Vertex Anthropic",
        npm: "@ai-sdk/google-vertex/anthropic",
        base_url: "https://aiplatform.{location}.rep.googleapis.com/v1/projects/{project}/locations/{location}/publishers/anthropic/models",
        env_var: "GOOGLE_APPLICATION_CREDENTIALS",
        models: &[
            ModelSpec { id: "claude-sonnet-4", name: "Claude Sonnet 4", ctx: 200_000, out: 8_192, family: Some("claude"), reasoning: false, image_input: true },
            ModelSpec { id: "claude-opus-4", name: "Claude Opus 4", ctx: 200_000, out: 32_000, family: Some("claude"), reasoning: true, image_input: true },
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
            .user_agent(format!("blazecode/{}", env!("CARGO_PKG_VERSION")))
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
        chat_completions::build_chat_body(model, &messages, tools, BodyOptions::default())
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
            return Err(Error::Llm { http_context: None, 
                module: self.config.provider_id.into(),
                method: "stream".into(),
                reason: Box::new((self.config.classify_error)(status, &text)),
            });
        }

        Ok(chat_completions::create_chat_stream(response, self.config.provider_id))
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
