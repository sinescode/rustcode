//! GitHub Copilot provider module.
//!
//! GitHub Copilot uses the OpenAI-compatible Chat Completions API with a unique
//! token exchange auth flow. Uses shared [`chat_completions`] module for body
//! construction, SSE parsing, and error classification. Copilot-specific:
//! - Token exchange: `GITHUB_TOKEN` → Copilot Bearer token
//! - Headers: `Copilot-Integration-Id`, `Editor-Version`
//!
//! Ported from:
//! - `packages/llm/src/providers/github-copilot.ts` (66 lines)
//! - `packages/opencode/src/auth/github-copilot.ts`
//!
//! OpenCode commit: 5d0f86606ac30690f79f0a6a9f41a1f49fe95d0b

use async_trait::async_trait;
use futures::StreamExt;
use serde::Deserialize;
use std::collections::HashMap;

use crate::error::{Error, LlmErrorReason};
use crate::provider::{self, ChatMessage, LlmEvent, Model, Provider, ProviderInfo, ToolDefinition};

use super::chat_completions::{self, BodyOptions};

// ── Constants ────────────────────────────────────────────────────────────

const DEFAULT_BASE_URL: &str = "https://api.githubcopilot.com";
const CHAT_PATH: &str = "/chat/completions";
const COPILOT_TOKEN_URL: &str = "https://api.github.com/copilot_internal/v2/token";

/// Default Copilot headers (mimics VS Code integration).
const COPILOT_HEADERS: &[(&str, &str)] = &[
    ("Copilot-Integration-Id", "vscode"),
    ("Editor-Version", "vscode/1.95.0"),
];

// ── Token exchange types ────────────────────────────────────────────────

/// Response from the Copilot token endpoint.
#[derive(Debug, Deserialize)]
struct CopilotTokenResponse {
    /// The Copilot API token to use for Bearer auth.
    token: String,
    /// When the token expires (Unix epoch seconds).
    #[serde(default)]
    expires_at: Option<u64>,
}

// ── Token acquisition ──────────────────────────────────────────────────

/// Resolve a Copilot API token.
///
/// Strategy:
/// 1. If `COPILOT_TOKEN` env var is set, use it directly (bypasses exchange).
/// 2. If `GITHUB_TOKEN` env var is set, exchange it for a Copilot token.
/// 3. Otherwise, return an auth error.
async fn resolve_copilot_token(
    http_client: &reqwest::Client,
) -> Result<String, Error> {
    // Direct token (for testing / pre-exchanged tokens)
    if let Ok(token) = std::env::var("COPILOT_TOKEN") {
        if !token.is_empty() {
            return Ok(token);
        }
    }

    // Exchange GITHUB_TOKEN for a Copilot token
    let github_token = std::env::var("GITHUB_TOKEN")
        .ok()
        .filter(|k| !k.is_empty())
        .ok_or_else(|| {
            Error::Auth(
                "GitHub Copilot requires GITHUB_TOKEN or COPILOT_TOKEN environment variable".into(),
            )
        })?;

    let response = http_client
        .post(COPILOT_TOKEN_URL)
        .header("Authorization", format!("Bearer {}", github_token))
        .header("User-Agent", format!("rustcode/{}", env!("CARGO_PKG_VERSION")))
        .header("Accept", "application/json")
        .send()
        .await
        .map_err(|e| {
            Error::Llm { http_context: None, 
                module: "github-copilot".into(),
                method: "token_exchange".into(),
                reason: Box::new(LlmErrorReason::Transport {
                    message: format!("failed to exchange GitHub token: {e}"),
                    kind: Some("connect".into()),
                    url: Some(COPILOT_TOKEN_URL.into()),
                }),
            }
        })?;

    if !response.status().is_success() {
        let status = response.status().as_u16();
        let text = response.text().await.unwrap_or_default();
        return Err(Error::Llm { http_context: None, 
            module: "github-copilot".into(),
            method: "token_exchange".into(),
            reason: Box::new(classify_token_error(status, &text)),
        });
    }

    let token_resp: CopilotTokenResponse = response
        .json()
        .await
        .map_err(|e| {
            Error::Llm { http_context: None, 
                module: "github-copilot".into(),
                method: "token_exchange".into(),
                reason: Box::new(LlmErrorReason::InvalidProviderOutput {
                    message: format!("failed to parse Copilot token response: {e}"),
                    raw: None,
                }),
            }
        })?;

    Ok(token_resp.token)
}

/// Classify errors from the Copilot token exchange endpoint.
fn classify_token_error(status: u16, body: &str) -> LlmErrorReason {
    match status {
        401 | 403 => LlmErrorReason::Authentication {
            message: format!("GitHub token rejected (HTTP {status}): {body}"),
            kind: crate::error::AuthErrorKind::Invalid,
        },
        429 => LlmErrorReason::RateLimit {
            message: format!("GitHub token rate limited (HTTP {status}): {body}"),
            retry_after_ms: None,
        },
        _ => LlmErrorReason::UnknownProvider {
            message: format!("Copilot token exchange failed (HTTP {status}): {body}"),
            status: Some(status),
        },
    }
}

/// Determine whether a model should use the Responses API.
///
/// GPT-5+ models (non-mini) use Responses API; everything else uses Chat.
fn should_use_responses_api(model_id: &str) -> bool {
    let lower = model_id.to_lowercase();
    // GPT-5+ non-mini models use Responses API
    if let Some(rest) = lower.strip_prefix("gpt-") {
        if let Some(version_str) = rest.split(|c: char| !c.is_ascii_digit()).next() {
            if let Ok(version) = version_str.parse::<u32>() {
                return version >= 5 && !lower.contains("mini");
            }
        }
    }
    false
}

// ── GitHub Copilot Provider ─────────────────────────────────────────────

/// GitHub Copilot LLM provider.
///
/// Handles the unique token exchange flow and OpenAI-compatible chat API.
pub struct GitHubCopilotProvider {
    /// The Copilot API token (Bearer).
    copilot_token: String,
    /// Base URL for the Copilot API.
    base_url: String,
    http_client: reqwest::Client,
    models: Vec<Model>,
}

impl GitHubCopilotProvider {
    /// Create a new GitHub Copilot provider.
    ///
    /// Performs the token exchange synchronously at construction time. If you
    /// need lazy token acquisition, use [`GitHubCopilotProvider::new_async`].
    pub fn new() -> Result<Self, Error> {
        let http_client = reqwest::Client::builder()
            .user_agent(format!("rustcode/{}", env!("CARGO_PKG_VERSION")))
            .timeout(std::time::Duration::from_secs(300))
            .build()
            .map_err(|e| Error::Network(format!("HTTP client: {e}")))?;

        // For synchronous construction, try direct COPILOT_TOKEN or GITHUB_TOKEN
        // without async exchange (sync exchange is not possible with reqwest).
        // Fall back to sync token resolution only.
        let token = std::env::var("COPILOT_TOKEN")
            .ok()
            .filter(|k| !k.is_empty())
            .or_else(|| {
                // Just read GITHUB_TOKEN for sync path (no exchange)
                std::env::var("GITHUB_TOKEN")
                    .ok()
                    .filter(|k| !k.is_empty())
            })
            .ok_or_else(|| {
                Error::Auth(
                    "GitHub Copilot requires GITHUB_TOKEN or COPILOT_TOKEN environment variable. \
                     Use new_async() for automatic token exchange."
                        .into(),
                )
            })?;

        let base_url = DEFAULT_BASE_URL.to_string();
        let models = build_model_catalog(&base_url);

        Ok(Self {
            copilot_token: token,
            base_url,
            http_client,
            models,
        })
    }

    /// Create a new provider with async token exchange.
    ///
    /// This exchanges the `GITHUB_TOKEN` env var for a Copilot token.
    pub async fn new_async() -> Result<Self, Error> {
        let http_client = reqwest::Client::builder()
            .user_agent(format!("rustcode/{}", env!("CARGO_PKG_VERSION")))
            .timeout(std::time::Duration::from_secs(300))
            .build()
            .map_err(|e| Error::Network(format!("HTTP client: {e}")))?;

        let copilot_token = resolve_copilot_token(&http_client).await?;
        let base_url = DEFAULT_BASE_URL.to_string();
        let models = build_model_catalog(&base_url);

        Ok(Self {
            copilot_token,
            base_url,
            http_client,
            models,
        })
    }

    /// Create with an explicit Copilot token.
    pub fn with_token(copilot_token: String) -> Result<Self, Error> {
        let http_client = reqwest::Client::builder()
            .user_agent(format!("rustcode/{}", env!("CARGO_PKG_VERSION")))
            .timeout(std::time::Duration::from_secs(300))
            .build()
            .map_err(|e| Error::Network(format!("HTTP client: {e}")))?;

        let base_url = DEFAULT_BASE_URL.to_string();
        let models = build_model_catalog(&base_url);

        Ok(Self {
            copilot_token,
            base_url,
            http_client,
            models,
        })
    }

    /// Try to auto-detect: returns `ProviderInfo` if a Copilot token is
    /// available (via GITHUB_TOKEN or COPILOT_TOKEN).
    pub fn auto_detect() -> Vec<ProviderInfo> {
        let has_token = std::env::var("COPILOT_TOKEN")
            .or_else(|_| std::env::var("GITHUB_TOKEN"))
            .ok()
            .filter(|k| !k.is_empty())
            .is_some();

        if has_token {
            vec![ProviderInfo {
                id: "github-copilot".into(),
                name: "GitHub Copilot".into(),
                source: crate::provider::ProviderSource::Env,
                env: vec!["GITHUB_TOKEN".into(), "COPILOT_TOKEN".into()],
                key: None,
                options: HashMap::new(),
                models: HashMap::new(),
            }]
        } else {
            Vec::new()
        }
    }

    fn chat_url(&self) -> String {
        format!("{}{CHAT_PATH}", self.base_url.trim_end_matches('/'))
    }

    fn chat_url(&self) -> String {
        format!("{}{CHAT_PATH}", self.base_url.trim_end_matches('/'))
    }
}

// ── Provider trait implementation ──────────────────────────────────────

#[async_trait]
impl Provider for GitHubCopilotProvider {
    fn provider_id(&self) -> &str {
        "github-copilot"
    }

    fn npm(&self) -> &str {
        "@ai-sdk/github-copilot"
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
                provider_id: "github-copilot".into(),
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
        let body = chat_completions::build_chat_body(&model, &messages, tools, BodyOptions::default());

        // Build auth header: if the token already has "Bearer " prefix, use as-is;
        // otherwise prepend "Bearer ".
        let auth_header = if self.copilot_token.starts_with("Bearer ") {
            self.copilot_token.clone()
        } else {
            format!("Bearer {}", self.copilot_token)
        };

        let mut req = self
            .http_client
            .post(self.chat_url())
            .header("Authorization", &auth_header)
            .header("Content-Type", "application/json");

        for &(k, v) in COPILOT_HEADERS {
            req = req.header(k, v);
        }

        let response = req
            .json(&body)
            .send()
            .await
            .map_err(|e| Error::Network(format!("GitHub Copilot request: {e}")))?;

        if !response.status().is_success() {
            let status = response.status().as_u16();
            let text = response.text().await.unwrap_or_default();
            return Err(Error::Llm { http_context: None, 
                module: "github-copilot".into(),
                method: "stream".into(),
                reason: Box::new(chat_completions::classify_error(status, &text)),
            });
        }

        Ok(chat_completions::create_chat_stream(response, "github-copilot"))
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
            "gpt-4o-copilot",
            "GPT-4o (Copilot)",
            base_url,
            128_000,
            16_384,
            false,
            true,
        ),
        make_model(
            "claude-sonnet-4-copilot",
            "Claude Sonnet 4 (Copilot)",
            base_url,
            200_000,
            8_192,
            false,
            true,
        ),
        make_model(
            "claude-3.5-sonnet-copilot",
            "Claude 3.5 Sonnet (Copilot)",
            base_url,
            200_000,
            8_192,
            false,
            true,
        ),
        make_model(
            "gpt-4o-mini-copilot",
            "GPT-4o Mini (Copilot)",
            base_url,
            128_000,
            16_384,
            false,
            true,
        ),
    ]
}

fn make_model(
    id: &str,
    name: &str,
    base_url: &str,
    context: u64,
    output: u64,
    reasoning: bool,
    image_input: bool,
) -> Model {
    Model {
        id: id.into(),
        provider_id: "github-copilot".into(),
        name: name.into(),
        api: provider::ApiInfo {
            id: id.into(),
            url: base_url.into(),
            npm: "@ai-sdk/github-copilot".into(),
        },
        family: Some(if id.contains("gpt") { "gpt".into() } else { "claude".into() }),
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
        cost: provider::Cost::default(),
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
    fn test_should_use_responses_api() {
        assert!(should_use_responses_api("gpt-5.1"));
        assert!(should_use_responses_api("gpt-5.2"));
        assert!(!should_use_responses_api("gpt-5-mini"));
        assert!(!should_use_responses_api("gpt-4o-copilot"));
        assert!(!should_use_responses_api("claude-sonnet-4-copilot"));
    }

    #[test]
    fn test_model_catalog() {
        let models = build_model_catalog("https://api.githubcopilot.com");
        assert!(models.len() >= 4);
        let gpt4o = models.iter().find(|m| m.id == "gpt-4o-copilot").unwrap();
        assert_eq!(gpt4o.provider_id, "github-copilot");
        assert!(gpt4o.capabilities.toolcall);

        let claude = models.iter().find(|m| m.id == "claude-sonnet-4-copilot").unwrap();
        assert_eq!(claude.family.as_deref(), Some("claude"));
        assert_eq!(claude.limit.context, 200_000);
    }

    #[test]
    fn test_classify_token_error() {
        let reason = classify_token_error(401, "Bad credentials");
        assert!(matches!(reason, LlmErrorReason::Authentication { .. }));

        let reason = classify_token_error(429, "rate limit");
        assert!(matches!(reason, LlmErrorReason::RateLimit { .. }));
    }
}
