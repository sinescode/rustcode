//! Microsoft Azure OpenAI provider — OpenAI-compatible Chat Completions protocol.
//!
//! Azure OpenAI speaks the same OpenAI Chat Completions wire format with SSE
//! streaming. Uses shared [`chat_completions`] module for body construction,
//! SSE parsing, and error classification. Azure-specific:
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
use std::collections::HashMap;

use crate::error::Error;
use crate::provider::{ChatMessage, LlmEvent, Model, Provider, ToolDefinition};

use super::chat_completions::{self, BodyOptions};

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
        let body = chat_completions::build_chat_body(&model, &messages, tools, BodyOptions::default());

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
            return Err(Error::Llm { http_context: None, 
                module: "azure".into(),
                method: "stream".into(),
                reason: Box::new(chat_completions::classify_error(status, &text)),
            });
        }

        Ok(chat_completions::create_chat_stream(response, "azure"))
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
            return Err(Error::Llm { http_context: None, 
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

    // ── Error classification (via shared module) ─────────────────

    #[test]
    fn test_classify_error_auth_401() {
        let reason = chat_completions::classify_error(401, r#"{"error":{"message":"Invalid API key"}}"#);
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
        let reason = chat_completions::classify_error(429, "Too many requests");
        assert!(matches!(reason, LlmErrorReason::RateLimit { .. }));
    }

    #[test]
    fn test_classify_error_retryable() {
        assert!(chat_completions::classify_error(429, "rate limit").is_retryable());
        assert!(chat_completions::classify_error(503, "overloaded").is_retryable());
        assert!(!chat_completions::classify_error(400, "bad request").is_retryable());
        assert!(!chat_completions::classify_error(401, "unauthorized").is_retryable());
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

    // ── Chat message building (via shared module) ────────────────

    #[test]
    fn test_lower_messages_system() {
        use crate::provider::{ChatMessage, MessageContent};
        let messages = vec![ChatMessage::System {
            content: MessageContent::Text("You are helpful.".into()),
        }];
        let result = chat_completions::lower_messages(&messages);
        assert_eq!(result.len(), 1);
        assert_eq!(result[0]["role"], "system");
        assert_eq!(result[0]["content"], "You are helpful.");
    }

    #[test]
    fn test_lower_messages_user_text() {
        use crate::provider::{ChatMessage, MessageContent};
        let messages = vec![ChatMessage::User {
            content: MessageContent::Text("Hello".into()),
        }];
        let result = chat_completions::lower_messages(&messages);
        assert_eq!(result.len(), 1);
        assert_eq!(result[0]["role"], "user");
        assert_eq!(result[0]["content"], "Hello");
    }

    #[test]
    fn test_lower_messages_mixed() {
        use crate::provider::ChatMessage;
        let messages = vec![
            ChatMessage::System {
                content: crate::provider::MessageContent::Text("System prompt".into()),
            },
            ChatMessage::User {
                content: crate::provider::MessageContent::Text("User query".into()),
            },
            ChatMessage::Assistant {
                content: crate::provider::MessageContent::Text("Assistant reply".into()),
            },
        ];
        let result = chat_completions::lower_messages(&messages);
        assert_eq!(result.len(), 3);
    }

    #[test]
    fn test_lower_tools_single() {
        let tools = vec![ToolDefinition {
            name: "bash".into(),
            description: "Run a shell command".into(),
            parameters: serde_json::json!({"type": "object", "properties": {}}),
        }];
        let result = chat_completions::lower_tools(&tools);
        assert_eq!(result.len(), 1);
        assert_eq!(result[0]["type"], "function");
        assert_eq!(result[0]["function"]["name"], "bash");
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
