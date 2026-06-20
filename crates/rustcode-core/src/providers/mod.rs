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
//! | DeepSeek | Chat Completions (`/v1/chat/completions`) | Bearer token | SSE |
//! | Together AI | Chat Completions (`/v1/chat/completions`) | Bearer token | SSE |
//! | Groq | Chat Completions (`/openai/v1/chat/completions`) | Bearer token | SSE |
//! | xAI Grok | Chat Completions (`/v1/chat/completions`) | Bearer token | SSE |
//! | Mistral | Chat Completions (`/v1/chat/completions`) | Bearer token | SSE |
//! | Amazon Bedrock | Chat Completions (Converse API bridge) | AWS headers | SSE |
//! | Azure OpenAI | Chat Completions (deployment-scoped) | `api-key` header | SSE |
//! | Cloudflare AI | Chat Completions (`/ai/run`) | Bearer token | SSE |
//! | GitHub Copilot | Chat Completions (`/chat/completions`) | Bearer token | SSE |
//! | Cerebras | Chat Completions (`/v1/chat/completions`) | Bearer token | SSE |
//! | Perplexity | Chat Completions (`/chat/completions`) | Bearer token | SSE |
//! | Cohere | Chat Completions (`/v1/chat/completions`) | Bearer token | SSE |
//! | Fireworks AI | Chat Completions (`/inference/v1/chat/completions`) | Bearer token | SSE |
//! | AI21 Labs | Chat Completions (`/studio/v1/chat/completions`) | Bearer token | SSE |

pub mod ai21;
pub mod anthropic;
pub mod azure;
pub mod bedrock;
pub mod cerebras;
pub mod cloudflare;
pub mod cohere;
pub mod deepseek;
pub mod fireworks;
pub mod gemini;
pub mod github_copilot;
pub mod groq;
pub mod mistral;
pub mod openai;
pub mod openai_compatible;
pub mod openrouter;
pub mod perplexity;
pub mod together;
pub mod xai;

/// Try to create all auto-detectable providers from environment variables.
///
/// Checks for known API key env vars and returns all providers that can be
/// constructed. This is the recommended way to bootstrap providers at startup.
pub fn auto_detect_all() -> Vec<Box<dyn crate::provider::Provider>> {
    let mut providers: Vec<Box<dyn crate::provider::Provider>> = Vec::new();

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
    if let Ok(p) = together::TogetherProvider::new() {
        tracing::info!("Detected Together AI provider (TOGETHER_API_KEY)");
        providers.push(Box::new(p));
    }
    if let Ok(p) = deepseek::DeepSeekProvider::new() {
        tracing::info!("Detected DeepSeek provider (DEEPSEEK_API_KEY)");
        providers.push(Box::new(p));
    }
    if let Ok(p) = xai::XaiProvider::new() {
        tracing::info!("Detected xAI Grok provider (XAI_API_KEY)");
        providers.push(Box::new(p));
    }
    if let Ok(p) = groq::GroqProvider::new() {
        tracing::info!("Detected Groq provider (GROQ_API_KEY)");
        providers.push(Box::new(p));
    }
    if let Ok(p) = mistral::MistralProvider::new() {
        tracing::info!("Detected Mistral provider (MISTRAL_API_KEY)");
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
    if let Ok(p) = github_copilot::CopilotProvider::new() {
        tracing::info!("Detected GitHub Copilot provider (GITHUB_TOKEN)");
        providers.push(Box::new(p));
    }

    // Dedicated per-provider auto-detection (OpenAI-compatible subclasses)
    if let Ok(p) = ai21::Ai21Provider::new() {
        tracing::info!("Detected AI21 Labs provider (AI21_API_KEY)");
        providers.push(Box::new(p));
    }
    if let Ok(p) = cerebras::CerebrasProvider::new() {
        tracing::info!("Detected Cerebras provider (CEREBRAS_API_KEY)");
        providers.push(Box::new(p));
    }
    if let Ok(p) = cohere::CohereProvider::new() {
        tracing::info!("Detected Cohere provider (COHERE_API_KEY)");
        providers.push(Box::new(p));
    }
    if let Ok(p) = fireworks::FireworksProvider::new() {
        tracing::info!("Detected Fireworks AI provider (FIREWORKS_API_KEY)");
        providers.push(Box::new(p));
    }
    if let Ok(p) = perplexity::PerplexityProvider::new() {
        tracing::info!("Detected Perplexity provider (PPLX_API_KEY)");
        providers.push(Box::new(p));
    }

    // OpenAI-compatible providers (auto-detect from env vars)
    for profile in openai_compatible::PROFILES {
        if let Ok(p) = openai_compatible::OpenAICompatibleProvider::from_profile(profile) {
            tracing::info!("Detected {} provider ({})", profile.name, profile.env_var);
            providers.push(Box::new(p));
        }
    }

    providers
}
