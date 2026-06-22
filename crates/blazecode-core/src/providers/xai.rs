//! xAI (Grok) provider module.
//!
//! xAI uses the OpenAI-compatible Chat Completions API at `https://api.x.ai`.
//! Uses shared [`chat_completions`] module for body construction, SSE parsing,
//! and error classification. xAI-specific:
//! - Reasoner effort: `reasoning_effort: "medium"` for Grok 3+ models
//!
//! Ported from:
//! - `packages/llm/src/providers/xai.ts` (56 lines)
//! - `packages/llm/src/providers/openai-compatible-profile.ts` (20 lines)
//!
//! BlazeCode commit: 5d0f86606ac30690f79f0a6a9f41a1f49fe95d0b

use async_trait::async_trait;
use futures::StreamExt;
use std::collections::HashMap;

use crate::error::Error;
use crate::provider::{self, ChatMessage, LlmEvent, Model, Provider, ProviderInfo, ToolDefinition};

use super::chat_completions::{self, BodyOptions};

// ── Constants ────────────────────────────────────────────────────────────

const DEFAULT_BASE_URL: &str = "https://api.x.ai/v1";
const CHAT_PATH: &str = "/chat/completions";

/// Resolve the API key for xAI.
fn resolve_api_key() -> Result<String, Error> {
    std::env::var("XAI_API_KEY")
        .ok()
        .filter(|k| !k.is_empty())
        .ok_or_else(|| Error::Auth("XAI_API_KEY environment variable not set".into()))
}

// ── xAI Provider ─────────────────────────────────────────────────────────

/// xAI (Grok) LLM provider.
pub struct XaiProvider {
    api_key: String,
    base_url: String,
    http_client: reqwest::Client,
    models: Vec<Model>,
}

impl XaiProvider {
    /// Create a new xAI provider, reading the API key from `XAI_API_KEY`.
    pub fn new() -> Result<Self, Error> {
        let api_key = resolve_api_key()?;
        Self::with_api_key(api_key, DEFAULT_BASE_URL.into())
    }

    /// Create a new xAI provider with an explicit API key and base URL.
    pub fn with_api_key(api_key: String, base_url: String) -> Result<Self, Error> {
        let http_client = reqwest::Client::builder()
            .user_agent(format!("blazecode/{}", env!("CARGO_PKG_VERSION")))
            .timeout(std::time::Duration::from_secs(300))
            .build()
            .map_err(|e| Error::Network(format!("HTTP client: {e}")))?;

        let models = build_model_catalog(&base_url);

        Ok(Self {
            api_key,
            base_url,
            http_client,
            models,
        })
    }

    /// Try to auto-detect: returns `ProviderInfo` if `XAI_API_KEY` is set.
    pub fn auto_detect() -> Vec<ProviderInfo> {
        std::env::var("XAI_API_KEY")
            .ok()
            .filter(|k| !k.is_empty())
            .map(|_| ProviderInfo {
                id: "xai".into(),
                name: "xAI Grok".into(),
                source: crate::provider::ProviderSource::Env,
                env: vec!["XAI_API_KEY".into()],
                key: None,
                options: HashMap::new(),
                models: HashMap::new(),
            })
            .into_iter()
            .collect()
    }

    fn chat_url(&self) -> String {
        format!("{}{CHAT_PATH}", self.base_url.trim_end_matches('/'))
    }
}

// ── Provider trait implementation ──────────────────────────────────────

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
        let messages = provider::normalize_messages(messages, model);

        // xAI supports reasoning effort (similar to OpenAI)
        let mut extra = serde_json::Map::new();
        let model_lower = model.id.to_lowercase();
        if model_lower.contains("grok") && !model_lower.contains("grok-2") {
            extra.insert("reasoning_effort".into(), serde_json::json!("medium"));
        }

        let body = chat_completions::build_chat_body(
            &model,
            &messages,
            tools,
            BodyOptions {
                extra_fields: Some(extra),
                ..BodyOptions::default()
            },
        );

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
            return Err(Error::Llm { http_context: None, 
                module: "xai".into(),
                method: "stream".into(),
                reason: Box::new(chat_completions::classify_error(status, &text)),
            });
        }

        Ok(chat_completions::create_chat_stream(response, "xai"))
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

// ── Model catalog ──────────────────────────────────────────────────────

fn build_model_catalog(base_url: &str) -> Vec<Model> {
    vec![
        make_model(
            "grok-4", "Grok 4", base_url, 1_000_000, 128_000, 2.50, 10.0, true, true,
        ),
        make_model(
            "grok-3", "Grok 3", base_url, 128_000, 16_384, 2.00, 8.0, true, true,
        ),
        make_model(
            "grok-3-mini", "Grok 3 Mini", base_url, 128_000, 16_384, 1.00, 4.0, true, false,
        ),
        make_model(
            "grok-3-fast", "Grok 3 Fast", base_url, 128_000, 16_384, 2.00, 8.0, false, true,
        ),
        make_model(
            "grok-3-latest", "Grok 3 Latest", base_url, 128_000, 16_384, 2.00, 8.0, true, true,
        ),
        make_model(
            "grok-2", "Grok 2", base_url, 128_000, 16_384, 1.50, 6.0, false, true,
        ),
        make_model(
            "grok-2-latest", "Grok 2 Latest", base_url, 128_000, 16_384, 1.50, 6.0, false, true,
        ),
        make_model(
            "grok-beta", "Grok Beta", base_url, 128_000, 8_192, 0.50, 2.0, false, false,
        ),
    ]
}

#[allow(clippy::too_many_arguments)]
fn make_model(
    id: &str,
    name: &str,
    base_url: &str,
    context: u64,
    output: u64,
    input_cost: f64,
    output_cost: f64,
    reasoning: bool,
    image_input: bool,
) -> Model {
    Model {
        id: id.into(),
        provider_id: "xai".into(),
        name: name.into(),
        api: provider::ApiInfo {
            id: id.into(),
            url: base_url.into(),
            npm: "@ai-sdk/xai".into(),
        },
        family: Some("grok".into()),
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
        cost: provider::Cost {
            input: input_cost,
            output: output_cost,
            cache: provider::CacheCost::default(),
            tiers: None,
            experimental_over_200k: None,
        },
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
    fn test_model_catalog() {
        let models = build_model_catalog("https://api.x.ai/v1");
        assert!(models.len() >= 5);
        let grok4 = models.iter().find(|m| m.id == "grok-4").unwrap();
        assert_eq!(grok4.provider_id, "xai");
        assert_eq!(grok4.capabilities.toolcall, true);
        assert_eq!(grok4.limit.context, 1_000_000);
    }

    #[test]
    fn test_resolve_api_key_missing() {
        std::env::remove_var("XAI_API_KEY");
        let result = resolve_api_key();
        assert!(result.is_err());
    }
}
