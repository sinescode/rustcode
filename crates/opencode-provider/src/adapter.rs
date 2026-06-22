//! # Provider adapter — the abstraction for different AI providers.
//!
//! Each provider (Anthropic, OpenAI, Google, etc.) implements this trait.

use crate::types::*;
use async_trait::async_trait;
use futures::stream::BoxStream;

/// Result type for provider operations.
pub type ProviderResult<T> = Result<T, ProviderError>;

/// Errors from provider operations.
#[derive(Debug, thiserror::Error)]
pub enum ProviderError {
    /// HTTP/network error.
    #[error("HTTP: {0}")]
    Http(#[from] reqwest::Error),

    /// API returned an error.
    #[error("API: {status} {message}")]
    Api {
        /// HTTP status code.
        status: u16,
        /// Error message from API.
        message: String,
    },

    /// Authentication error.
    #[error("Auth: {0}")]
    Auth(String),

    /// Rate limited.
    #[error("Rate limited: retry after {0:?}")]
    RateLimited(std::time::Duration),

    /// Context window exceeded.
    #[error("Context overflow: {0}")]
    ContextOverflow(String),

    /// JSON serialization/deserialization error.
    #[error("JSON: {0}")]
    Json(#[from] serde_json::Error),

    /// Invalid configuration.
    #[error("Config: {0}")]
    Config(String),

    /// Stream error.
    #[error("Stream: {0}")]
    Stream(String),

    /// Timeout.
    #[error("Timeout after {0:?}")]
    Timeout(std::time::Duration),

    /// Other error.
    #[error("Provider: {0}")]
    Other(String),
}

impl ProviderError {
    /// Whether this error is retryable.
    pub fn is_retryable(&self) -> bool {
        matches!(self, ProviderError::Http(_)
            | ProviderError::RateLimited(_)
            | ProviderError::Timeout(_)
            | ProviderError::Stream(_))
    }

    /// Whether this error is due to context overflow.
    pub fn is_context_overflow(&self) -> bool {
        matches!(self, ProviderError::ContextOverflow(_))
    }
}

/// Provider adapter — abstracts different AI providers.
#[async_trait]
pub trait ProviderAdapter: std::fmt::Debug + Send + Sync + 'static {
    /// Get the provider ID (e.g., "anthropic", "openai").
    fn provider_id(&self) -> &str;

    /// Get the capabilities of this provider.
    fn capabilities(&self) -> Capabilities;

    /// Send a non-streaming completion request.
    async fn complete(&self, config: &RequestConfig, messages: &[Message]) -> ProviderResult<ProviderResponse>;

    /// Send a streaming completion request.
    async fn complete_stream(&self, config: &RequestConfig, messages: &[Message]) -> ProviderResult<BoxStream<'static, StreamEvent>>;

    /// Validate the provider configuration.
    async fn validate(&self) -> ProviderResult<()>;
}

/// A provider adapter wrapped in Arc for clone-ability.
pub type ArcProvider = std::sync::Arc<dyn ProviderAdapter>;

/// Provider adapter for Anthropic's API.
pub mod anthropic;
