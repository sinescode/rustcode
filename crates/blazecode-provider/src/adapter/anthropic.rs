//! # Anthropic provider adapter implementation.
//!
//! Implements the Claude API (messages API with streaming).

use super::*;
use async_trait::async_trait;
use futures::stream::BoxStream;
use reqwest::Client;
use tracing::debug;

/// Anthropic API client.
#[derive(Debug, Clone)]
pub struct AnthropicProvider {
    /// Provider config.
    config: ProviderConfig,
    /// HTTP client.
    http: Client,
}

impl AnthropicProvider {
    /// Create a new Anthropic provider.
    pub fn new(config: ProviderConfig) -> Self {
        Self {
            http: Client::builder()
                .timeout(std::time::Duration::from_secs(config.timeout_secs))
                .build()
                .expect("valid HTTP client"),
            config,
        }
    }
}

#[async_trait]
impl ProviderAdapter for AnthropicProvider {
    fn provider_id(&self) -> &str {
        &self.config.id
    }

    fn capabilities(&self) -> Capabilities {
        Capabilities::claude_sonnet_4()
    }

    async fn complete(&self, config: &RequestConfig, messages: &[Message]) -> ProviderResult<ProviderResponse> {
        debug!(
            provider = %self.config.id,
            model = %config.model,
            messages = %messages.len(),
            "anthropic: sending completion request"
        );
        // TODO: Implement actual Anthropic messages API
        Err(ProviderError::Other("Anthropic adapter not yet implemented — stub".into()))
    }

    async fn complete_stream(&self, config: &RequestConfig, messages: &[Message]) -> ProviderResult<BoxStream<'static, StreamEvent>> {
        debug!(
            provider = %self.config.id,
            model = %config.model,
            messages = %messages.len(),
            "anthropic: opening stream"
        );
        // TODO: Implement actual Anthropic streaming API
        Err(ProviderError::Other("Anthropic stream adapter not yet implemented — stub".into()))
    }

    async fn validate(&self) -> ProviderResult<()> {
        Ok(())
    }
}
