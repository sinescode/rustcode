//! OpenRouter provider — OpenAI Chat with extra OpenRouter fields.
//!
//! Uses shared [`chat_completions`] module for body construction, SSE parsing,
//! and error classification. OpenRouter-specific:
//! - `HTTP-Referer` header for API attribution
//!
//! Ported from: `packages/llm/src/providers/openrouter.ts` (98 lines)

use async_trait::async_trait;
use futures::StreamExt;
use std::collections::HashMap;

use crate::error::Error;
use crate::provider::{ChatMessage, LlmEvent, Model, Provider, ToolDefinition};

use super::chat_completions::{self, BodyOptions};

const DEFAULT_BASE_URL: &str = "https://openrouter.ai/api/v1";

pub struct OpenRouterProvider {
    api_key: String,
    base_url: String,
    http_client: reqwest::Client,
    models: Vec<Model>,
}

impl OpenRouterProvider {
    pub fn new() -> Result<Self, Error> {
        let api_key = std::env::var("OPENROUTER_API_KEY")
            .ok()
            .filter(|k| !k.is_empty())
            .ok_or_else(|| Error::Auth("OPENROUTER_API_KEY not set".into()))?;
        Self::with_key(api_key, DEFAULT_BASE_URL.into())
    }

    pub fn with_key(api_key: String, base_url: String) -> Result<Self, Error> {
        let client = reqwest::Client::builder()
            .user_agent(format!("blazecode/{}", env!("CARGO_PKG_VERSION")))
            .timeout(std::time::Duration::from_secs(300))
            .build()
            .map_err(|e| Error::Network(format!("HTTP: {e}")))?;
        Ok(Self {
            api_key,
            base_url,
            http_client: client,
            models: vec![
                mk("openai/gpt-5.2", "GPT-5.2", "gpt"),
                mk("anthropic/claude-sonnet-4-6", "Claude Sonnet 4.6", "claude"),
                mk("google/gemini-3.0-pro", "Gemini 3.0 Pro", "gemini"),
                mk("meta-llama/llama-4-maverick", "Llama 4 Maverick", "llama"),
            ],
        })
    }

    fn chat_url(&self) -> String {
        format!("{}/chat/completions", self.base_url.trim_end_matches('/'))
    }
}

fn mk(id: &str, name: &str, family: &str) -> Model {
    Model {
        id: id.into(),
        provider_id: "openrouter".into(),
        name: name.into(),
        api: crate::provider::ApiInfo {
            id: id.into(),
            url: DEFAULT_BASE_URL.into(),
            npm: "@openrouter/ai-sdk-provider".into(),
        },
        family: Some(family.into()),
        capabilities: crate::provider::Capabilities {
            temperature: true,
            reasoning: true,
            attachment: true,
            toolcall: true,
            ..Default::default()
        },
        cost: Default::default(),
        limit: crate::provider::TokenLimit {
            context: 200_000,
            input: None,
            output: 16_384,
        },
        status: crate::provider::ModelStatus::Active,
        options: HashMap::new(),
        headers: HashMap::new(),
        release_date: "2026".into(),
        variants: None,
    }
}

// ── Provider impl ──────────────────────────────────────────────────────

#[async_trait]
impl Provider for OpenRouterProvider {
    fn provider_id(&self) -> &str {
        "openrouter"
    }
    fn npm(&self) -> &str {
        "@openrouter/ai-sdk-provider"
    }
    async fn list_models(&self) -> crate::error::Result<Vec<Model>> {
        Ok(self.models.clone())
    }
    async fn get_model(&self, id: &str) -> crate::error::Result<Model> {
        self.models
            .iter()
            .find(|m| m.id == id)
            .cloned()
            .ok_or_else(|| Error::ModelNotFound {
                provider_id: "openrouter".into(),
                model_id: id.into(),
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

        let resp = self
            .http_client
            .post(self.chat_url())
            .header("Authorization", format!("Bearer {}", self.api_key))
            .header("Content-Type", "application/json")
            .header("HTTP-Referer", "https://github.com/sinescode/blazecode")
            .json(&body)
            .send()
            .await
            .map_err(|e| Error::Network(format!("OpenRouter: {e}")))?;

        if !resp.status().is_success() {
            let status = resp.status().as_u16();
            let text = resp.text().await.unwrap_or_default();
            return Err(Error::Llm { http_context: None, 
                module: "openrouter".into(),
                method: "stream".into(),
                reason: Box::new(chat_completions::classify_error(status, &text)),
            });
        }

        Ok(chat_completions::create_chat_stream(resp, "openrouter"))
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

    #[test]
    fn test_model_catalog() {
        let models = OpenRouterProvider::with_key("test-key".into(), DEFAULT_BASE_URL.into())
            .unwrap()
            .models;
        assert!(models.len() >= 4);
        let gpt = models.iter().find(|m| m.id == "openai/gpt-5.2").unwrap();
        assert_eq!(gpt.provider_id, "openrouter");
        assert!(gpt.capabilities.toolcall);

        let claude = models.iter().find(|m| m.id == "anthropic/claude-sonnet-4-6").unwrap();
        assert_eq!(claude.family.as_deref(), Some("claude"));
        assert_eq!(claude.limit.context, 200_000);
    }
}
