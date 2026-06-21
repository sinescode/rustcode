//! LLM provider implementations.
//!
//! Each submodule implements the [`Provider`](crate::provider::Provider) trait
//! for a specific LLM provider's API protocol.
//!
//! ## Wire Protocol Coverage
//!
//! | Provider | Protocol | Auth | Streaming |
//! |----------|----------|------|-----------|
//! | Anthropic | Messages API (`/v1/messages`) | `x-api-key` header | SSE |
//! | OpenAI | Chat Completions (`/v1/chat/completions`) | Bearer token | SSE |
//! | Google Gemini | generateContent (`:streamGenerateContent?alt=sse`) | `x-goog-api-key` | SSE |
//! | OpenAI Compatible | Chat Completions (any base URL) | Bearer token | SSE |
//! | OpenRouter | Chat Completions (extended) | Bearer token | SSE |
//! | Amazon Bedrock | Chat Completions (Converse API bridge) | AWS SigV4 | SSE |
//! | Azure OpenAI | Chat Completions (deployment-scoped) | `api-key` header | SSE |
//! | Cloudflare AI | Chat Completions (`/ai/run`) | Bearer token | SSE |
//! | xAI (Grok) | Chat Completions (`/v1/chat/completions`) | Bearer token (`XAI_API_KEY`) | SSE |
//! | GitHub Copilot | Chat Completions (`/chat/completions`) | Bearer token (GITHUB_TOKEN → Copilot token exchange) | SSE |
//!
//! The following providers are served by the generic [`openai_compatible`]
//! module via [`openai_compatible::CompatConfig`] profiles:
//! DeepSeek, Groq, TogetherAI, Mistral, Cerebras,
//! Fireworks, AI21, Cohere, Perplexity, DeepInfra, Alibaba, Vercel Gateway.

pub mod anthropic;
pub mod azure;
pub mod bedrock;
pub mod cloudflare;
pub mod gemini;
pub mod github_copilot;
pub mod openai;
pub mod openai_compatible;
pub mod openrouter;
pub mod xai;

/// Try to create all auto-detectable providers from environment variables.
///
/// Checks for known API key env vars and returns all providers that can be
/// constructed. This is the recommended way to bootstrap providers at startup.
pub fn auto_detect_all() -> Vec<Box<dyn crate::provider::Provider>> {
    let mut providers: Vec<Box<dyn crate::provider::Provider>> = Vec::new();

    // Unique-protocol providers
    if let Ok(p) = anthropic::AnthropicProvider::new() {
        tracing::info!("Detected Anthropic provider (ANTHROPIC_API_KEY)");
        providers.push(Box::new(p));
    }
    if let Ok(p) = openai::OpenAIProvider::new() {
        tracing::info!("Detected OpenAI provider (OPENAI_API_KEY)");
        providers.push(Box::new(p));
    }
    if let Ok(p) = gemini::GeminiProvider::new() {
        tracing::info!("Detected Google Gemini provider (GOOGLE_GENERATIVE_AI_API_KEY)");
        providers.push(Box::new(p));
    }
    if let Ok(p) = openrouter::OpenRouterProvider::new() {
        tracing::info!("Detected OpenRouter provider (OPENROUTER_API_KEY)");
        providers.push(Box::new(p));
    }
    if let Ok(p) = bedrock::BedrockProvider::new() {
        tracing::info!("Detected Amazon Bedrock provider (AWS_ACCESS_KEY_ID)");
        providers.push(Box::new(p));
    }
    if let Ok(p) = azure::AzureProvider::new() {
        tracing::info!("Detected Azure OpenAI provider (AZURE_OPENAI_API_KEY)");
        providers.push(Box::new(p));
    }
    if let Ok(p) = cloudflare::CloudflareProvider::new() {
        tracing::info!("Detected Cloudflare Workers AI provider (CLOUDFLARE_API_TOKEN)");
        providers.push(Box::new(p));
    }

    // Dedicated xAI provider (was previously handled by openai_compatible profile)
    if let Ok(p) = xai::XaiProvider::new() {
        tracing::info!("Detected xAI Grok provider (XAI_API_KEY)");
        providers.push(Box::new(p));
    }

    // Dedicated GitHub Copilot provider (was previously handled by openai_compatible profile)
    if let Ok(p) = github_copilot::GitHubCopilotProvider::new() {
        tracing::info!("Detected GitHub Copilot provider (GITHUB_TOKEN)");
        providers.push(Box::new(p));
    }

    // All OpenAI-compatible providers (generic, profile-based)
    // Note: xAI and github_copilot have been promoted to dedicated modules
    // and are no longer in the PROFILES array.
    for config in openai_compatible::PROFILES {
        if let Ok(p) = openai_compatible::OpenAICompatibleProvider::from_config(config) {
            tracing::info!("Detected {} provider ({})", config.name, config.env_var);
            providers.push(Box::new(p));
        }
    }

    providers
}
