//! # Provider client — high-level API for sending requests to providers.
//!
//! Wraps the provider adapter and cache for convenient use.

use crate::adapter::{ArcProvider, ProviderResult};
use crate::cache::PromptCache;
use crate::types::*;

/// A client that sends requests to providers using a resolved adapter.
pub struct ProviderClient {
    /// Cached provider adapter.
    provider: ArcProvider,
    /// Prompt cache.
    cache: PromptCache,
}

impl ProviderClient {
    /// Create a new provider client.
    pub fn new(provider: ArcProvider, cache: PromptCache) -> Self {
        Self { provider, cache }
    }

    /// Send a streaming completion request.
    pub async fn complete_stream(
        &self,
        config: &RequestConfig,
        messages: &[Message],
    ) -> ProviderResult<futures::stream::BoxStream<'static, super::types::StreamEvent>> {
        self.provider.complete_stream(config, messages).await
    }

    /// Send a non-streaming completion request.
    pub async fn complete(
        &self,
        config: &RequestConfig,
        messages: &[Message],
    ) -> ProviderResult<ProviderResponse> {
        self.provider.complete(config, messages).await
    }

    /// Validate the provider configuration.
    pub async fn validate(&self) -> ProviderResult<()> {
        self.provider.validate().await
    }
}
