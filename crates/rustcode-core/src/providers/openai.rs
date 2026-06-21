//! OpenAI Chat Completions + Responses API provider.
//!
//! Ported from:
//! - `packages/llm/src/protocols/openai-chat.ts` (493 lines)
//! - `packages/llm/src/protocols/openai-responses.ts` (1004 lines)
//! - `packages/llm/src/providers/openai.ts` (63 lines)
//! - `packages/llm/src/providers/openai-options.ts` (83 lines)

use async_trait::async_trait;
use futures::StreamExt;
use std::collections::HashMap;

use crate::error::Error;
use crate::provider::{ChatMessage, LlmEvent, Model, Provider, ToolDefinition};

use super::chat_completions::{self, BodyOptions};

const DEFAULT_BASE_URL: &str = "https://api.openai.com/v1";
const CHAT_PATH: &str = "/chat/completions";
const RESPONSES_PATH: &str = "/responses";

fn resolve_api_key() -> Result<String, Error> {
    std::env::var("OPENAI_API_KEY")
        .ok()
        .filter(|k| !k.is_empty())
        .ok_or_else(|| Error::Auth("OPENAI_API_KEY environment variable not set".into()))
}

pub struct OpenAIProvider {
    api_key: String,
    base_url: String,
    http_client: reqwest::Client,
    models: Vec<Model>,
}

impl OpenAIProvider {
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
}

// ── Provider impl ──────────────────────────────────────────────────────

#[async_trait]
impl Provider for OpenAIProvider {
    fn provider_id(&self) -> &str {
        "openai"
    }
    fn npm(&self) -> &str {
        "@ai-sdk/openai"
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
                provider_id: "openai".into(),
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
        let body = chat_completions::build_chat_body(&model, &messages, tools, BodyOptions::default());

        let response = self
            .http_client
            .post(self.chat_url())
            .header("Authorization", format!("Bearer {}", self.api_key))
            .header("Content-Type", "application/json")
            .json(&body)
            .send()
            .await
            .map_err(|e| Error::Network(format!("OpenAI request: {e}")))?;

        if !response.status().is_success() {
            let status = response.status().as_u16();
            let text = response.text().await.unwrap_or_default();
            return Err(Error::Llm {
                module: "openai".into(),
                method: "stream".into(),
                reason: Box::new(chat_completions::classify_error(status, &text)),
            });
        }

        Ok(chat_completions::create_chat_stream(response, "openai"))
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

fn build_model_catalog() -> Vec<Model> {
    vec![
        make_model("gpt-5.2", "GPT-5.2", 200_000, 128_000, 1.75, 14.0),
        make_model("gpt-5.1", "GPT-5.1", 200_000, 128_000, 1.75, 14.0),
        make_model(
            "gpt-5.1-codex",
            "GPT-5.1 Codex",
            200_000,
            128_000,
            1.75,
            14.0,
        ),
        make_model("gpt-5.1-mini", "GPT-5.1 Mini", 200_000, 128_000, 0.35, 1.40),
        make_model("gpt-5.1-nano", "GPT-5.1 Nano", 200_000, 128_000, 0.10, 0.40),
    ]
}

fn make_model(id: &str, name: &str, ctx: u64, out: u64, inp_cost: f64, out_cost: f64) -> Model {
    Model {
        id: id.into(),
        provider_id: "openai".into(),
        name: name.into(),
        api: crate::provider::ApiInfo {
            id: id.into(),
            url: DEFAULT_BASE_URL.into(),
            npm: "@ai-sdk/openai".into(),
        },
        family: Some("gpt".into()),
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
        release_date: "2026".into(),
        variants: None,
    }
}
