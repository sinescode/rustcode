//! Cloudflare Workers AI provider — OpenAI-compatible Chat Completions protocol.
//!
//! Cloudflare Workers AI speaks the OpenAI Chat Completions wire format with
//! SSE streaming. Uses shared [`chat_completions`] module for body construction,
//! SSE parsing, and error classification. Cloudflare-specific:
//! - Base URL: `https://api.cloudflare.com/client/v4/accounts/{account_id}/ai/run/{model_id}`
//! - Auth: Bearer token via `CLOUDFLARE_API_TOKEN` env var
//! - Account ID: `CLOUDFLARE_ACCOUNT_ID` env var
//! - Model catalog: @cf/meta/llama-4-maverick, @cf/meta/llama-4-scout,
//!   @cf/deepseek/deepseek-v3, @cf/qwen/qwen3
//!
//! Ported from:
//! - `packages/llm/src/protocols/openai-chat.ts` (493 lines)
//! - `packages/llm/src/providers/openai-compatible.ts` (66 lines)
//! - `packages/llm/src/providers/openai-compatible-profile.ts` (21 lines)

use async_trait::async_trait;
use futures::StreamExt;
use std::collections::HashMap;

use crate::error::Error;
use crate::provider::{ChatMessage, LlmEvent, Model, Provider, ToolDefinition};

use super::chat_completions::{self, BodyOptions};

const API_BASE: &str = "https://api.cloudflare.com/client/v4";

fn build_base_url(account_id: &str) -> String {
    format!("{API_BASE}/accounts/{account_id}/ai/run")
}

fn resolve_config() -> Result<(String, String), Error> {
    let account_id = std::env::var("CLOUDFLARE_ACCOUNT_ID")
        .ok()
        .filter(|k| !k.is_empty())
        .ok_or_else(|| Error::Auth("CLOUDFLARE_ACCOUNT_ID environment variable not set".into()))?;
    let api_token = std::env::var("CLOUDFLARE_API_TOKEN")
        .ok()
        .filter(|k| !k.is_empty())
        .ok_or_else(|| Error::Auth("CLOUDFLARE_API_TOKEN environment variable not set".into()))?;
    Ok((account_id, api_token))
}

// ── Cloudflare Provider ──────────────────────────────────────────────────

#[derive(Debug)]
pub struct CloudflareProvider {
    api_token: String,
    account_id: String,
    base_url: String,
    http_client: reqwest::Client,
    models: Vec<Model>,
}

impl CloudflareProvider {
    /// Create a new Cloudflare provider from env vars:
    /// `CLOUDFLARE_ACCOUNT_ID` and `CLOUDFLARE_API_TOKEN`.
    pub fn new() -> Result<Self, Error> {
        let (account_id, api_token) = resolve_config()?;
        let base_url = build_base_url(&account_id);
        Self::with_config(account_id, api_token, base_url)
    }

    /// Create with explicit credentials and a custom base URL
    /// (for proxies or testing).
    pub fn with_config(
        account_id: String,
        api_token: String,
        base_url: String,
    ) -> Result<Self, Error> {
        let http_client = reqwest::Client::builder()
            .user_agent(format!("blazecode/{}", env!("CARGO_PKG_VERSION")))
            .timeout(std::time::Duration::from_secs(300))
            .build()
            .map_err(|e| Error::Network(format!("HTTP client: {e}")))?;
        Ok(Self {
            api_token,
            account_id,
            base_url,
            http_client,
            models: build_model_catalog(),
        })
    }

    /// Build the chat URL for a specific model. Cloudflare embeds the model
    /// ID in the URL path rather than the request body alone.
    fn chat_url(&self, model_id: &str) -> String {
        format!("{}/{}", self.base_url.trim_end_matches('/'), model_id)
    }
}

// ── Model Catalog ──────────────────────────────────────────────────────

fn build_model_catalog() -> Vec<Model> {
    vec![
        make_model(
            "@cf/meta/llama-4-maverick",
            "Llama 4 Maverick",
            128_000,
            4_096,
            "llama",
            true,
            false,
        ),
        make_model(
            "@cf/meta/llama-4-scout",
            "Llama 4 Scout",
            128_000,
            4_096,
            "llama",
            true,
            false,
        ),
        make_model(
            "@cf/deepseek/deepseek-v3",
            "DeepSeek V3",
            128_000,
            8_192,
            "deepseek",
            true,
            true,
        ),
        make_model(
            "@cf/qwen/qwen3",
            "Qwen 3",
            32_000,
            8_192,
            "qwen",
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
        provider_id: "cloudflare".into(),
        name: name.into(),
        api: crate::provider::ApiInfo {
            id: id.into(),
            url: API_BASE.into(),
            npm: "@ai-sdk/cloudflare".into(),
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
impl Provider for CloudflareProvider {
    fn provider_id(&self) -> &str {
        "cloudflare"
    }

    fn npm(&self) -> &str {
        "@ai-sdk/cloudflare"
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
                provider_id: "cloudflare".into(),
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
            .post(self.chat_url(&model.api.id))
            .header("Authorization", format!("Bearer {}", self.api_token))
            .header("Content-Type", "application/json")
            .json(&body)
            .send()
            .await
            .map_err(|e| Error::Network(format!("Cloudflare request: {e}")))?;

        if !response.status().is_success() {
            let status = response.status().as_u16();
            let text = response.text().await.unwrap_or_default();
            return Err(Error::Llm { http_context: None, 
                module: "cloudflare".into(),
                method: "stream".into(),
                reason: Box::new(chat_completions::classify_error(status, &text)),
            });
        }

        Ok(chat_completions::create_chat_stream(response, "cloudflare"))
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

    // ── Model catalog ────────────────────────────────────────────

    #[test]
    fn test_model_catalog_count() {
        let models = build_model_catalog();
        assert_eq!(models.len(), 4, "expected 4 models in catalog");
    }

    #[test]
    fn test_model_catalog_ids() {
        let models = build_model_catalog();
        let ids: Vec<&str> = models.iter().map(|m| m.id.as_str()).collect();
        assert!(ids.contains(&"@cf/meta/llama-4-maverick"));
        assert!(ids.contains(&"@cf/meta/llama-4-scout"));
        assert!(ids.contains(&"@cf/deepseek/deepseek-v3"));
        assert!(ids.contains(&"@cf/qwen/qwen3"));
    }

    #[test]
    fn test_model_catalog_names() {
        let models = build_model_catalog();
        let names: Vec<&str> = models.iter().map(|m| m.name.as_str()).collect();
        assert!(names.contains(&"Llama 4 Maverick"));
        assert!(names.contains(&"Llama 4 Scout"));
        assert!(names.contains(&"DeepSeek V3"));
        assert!(names.contains(&"Qwen 3"));
    }

    #[test]
    fn test_api_base() {
        assert_eq!(API_BASE, "https://api.cloudflare.com/client/v4");
    }

    #[test]
    fn test_build_base_url() {
        let url = build_base_url("test-account-123");
        assert_eq!(url, "https://api.cloudflare.com/client/v4/accounts/test-account-123/ai/run");
    }

    #[test]
    fn test_chat_url() {
        let provider = CloudflareProvider::with_config("acct-1".into(), "tok-1".into(), "https://api.cloudflare.com/client/v4/accounts/acct-1/ai/run".into()).unwrap();
        assert_eq!(provider.chat_url("@cf/meta/llama-4-maverick"), "https://api.cloudflare.com/client/v4/accounts/acct-1/ai/run/@cf/meta/llama-4-maverick");
    }

    #[test]
    fn test_provider_trait() {
        let provider = CloudflareProvider::with_config("acct-1".into(), "tok-1".into(), "https://api.cloudflare.com/client/v4/accounts/acct-1/ai/run".into()).unwrap();
        assert_eq!(provider.provider_id(), "cloudflare");
        assert_eq!(provider.npm(), "@ai-sdk/cloudflare");
    }
}
