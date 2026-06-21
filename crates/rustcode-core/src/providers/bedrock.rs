//! Amazon Bedrock API provider — OpenAI-compatible Chat Completions protocol.
//!
//! Bedrock's Converse API is bridged through an OpenAI-compatible chat
//! completions endpoint. Uses shared [`chat_completions`] module for body
//! construction, SSE parsing, and error classification. Bedrock-specific:
//! - Base URL: <https://bedrock-runtime.{region}.amazonaws.com>
//! - Auth: AWS SigV4 request signing
//! - Model catalog: Claude Sonnet/Opus/Haiku 4, Llama 4 Maverick, Titan Text
//!
//! Ported from:
//! - `packages/llm/src/protocols/openai-chat.ts` (493 lines)
//! - `packages/llm/src/providers/openai-compatible.ts` (66 lines)
//! - `packages/llm/src/providers/openai-compatible-profile.ts` (21 lines)

use async_trait::async_trait;
use chrono::Utc;
use futures::StreamExt;
use hmac::{Hmac, Mac};
use sha2::{Digest, Sha256};
use std::collections::HashMap;

use crate::error::{Error, LlmErrorReason};
use crate::provider::{ChatMessage, LlmEvent, Model, Provider, ToolDefinition};

use super::chat_completions::{self, BodyOptions};

// ── AWS SigV4 Signing ──────────────────────────────────────────────────

type HmacSha256 = Hmac<Sha256>;

/// AWS SigV4 signer for Bedrock requests.
///
/// # Source
/// Implements the AWS Signature Version 4 signing algorithm per
/// <https://docs.aws.amazon.com/general/latest/gr/sigv4-signing.html>.
struct SigV4Signer {
    access_key: String,
    secret_key: String,
    region: String,
    service: &'static str,
}

impl SigV4Signer {
    fn new(access_key: &str, secret_key: &str, region: &str) -> Self {
        Self {
            access_key: access_key.into(),
            secret_key: secret_key.into(),
            region: region.into(),
            service: "bedrock",
        }
    }

    /// Sign a request and return the Authorization header value plus the
    /// x-amz-date header value.
    fn sign(
        &self,
        method: &str,
        host: &str,
        path: &str,
        body: &[u8],
        date: &str,
    ) -> Result<String, Error> {
        // Step 1: Create canonical request per AWS SigV4 spec
        let payload_hash = hex::encode(Sha256::digest(body));
        let signed_headers = "host;x-amz-content-sha256;x-amz-date";
        let canonical_headers =
            format!("host:{host}\nx-amz-content-sha256:{payload_hash}\nx-amz-date:{date}\n");
        let canonical_request =
            format!("{method}\n{path}\n\n{canonical_headers}\n{signed_headers}\n{payload_hash}");

        // Step 2: Create string to sign
        let credential_scope = format!("{date}/{}/{}/aws4_request", self.region, self.service);
        let canonical_request_hash = hex::encode(Sha256::digest(canonical_request.as_bytes()));
        let string_to_sign =
            format!("AWS4-HMAC-SHA256\n{date}\n{credential_scope}\n{canonical_request_hash}");

        // Step 3: Calculate signing key
        let k_date = self.hmac_raw(
            format!("AWS4{}", self.secret_key).as_bytes(),
            date.as_bytes(),
        );
        let k_region = self.hmac_raw(&k_date, self.region.as_bytes());
        let k_service = self.hmac_raw(&k_region, self.service.as_bytes());
        let k_signing = self.hmac_raw(&k_service, b"aws4_request");

        // Step 4: Calculate signature
        let signature = self.hmac_raw(&k_signing, string_to_sign.as_bytes());

        // Step 5: Build authorization header
        Ok(format!(
            "AWS4-HMAC-SHA256 Credential={}/{}, SignedHeaders={signed_headers}, Signature={}",
            self.access_key,
            credential_scope,
            hex::encode(signature),
        ))
    }

    fn hmac_raw(&self, key: &[u8], data: &[u8]) -> Vec<u8> {
        let mut mac = HmacSha256::new_from_slice(key).expect("HMAC can take key of any size");
        mac.update(data);
        mac.finalize().into_bytes().to_vec()
    }
}

const BASE_URL: &str = "https://bedrock-runtime.us-east-1.amazonaws.com";
const CHAT_PATH: &str = "/chat/completions";
const DEFAULT_REGION: &str = "us-east-1";

fn resolve_api_key() -> Result<String, Error> {
    std::env::var("AWS_ACCESS_KEY_ID")
        .ok()
        .filter(|k| !k.is_empty())
        .ok_or_else(|| Error::Auth("AWS_ACCESS_KEY_ID environment variable not set".into()))
}

fn resolve_secret_key() -> Result<String, Error> {
    std::env::var("AWS_SECRET_ACCESS_KEY")
        .ok()
        .filter(|k| !k.is_empty())
        .ok_or_else(|| Error::Auth("AWS_SECRET_ACCESS_KEY environment variable not set".into()))
}

fn resolve_region() -> String {
    std::env::var("AWS_REGION")
        .ok()
        .filter(|r| !r.is_empty())
        .unwrap_or_else(|| DEFAULT_REGION.into())
}

fn bedrock_base_url(region: &str) -> String {
    format!("https://bedrock-runtime.{}.amazonaws.com", region)
}

// ── Bedrock Provider ───────────────────────────────────────────────────

#[derive(Debug)]
pub struct BedrockProvider {
    api_key: String,
    secret_key: String,
    region: String,
    base_url: String,
    http_client: reqwest::Client,
    models: Vec<Model>,
}

impl BedrockProvider {
    /// Create a new Bedrock provider from `AWS_ACCESS_KEY_ID`,
    /// `AWS_SECRET_ACCESS_KEY`, and `AWS_REGION` env vars.
    pub fn new() -> Result<Self, Error> {
        let api_key = resolve_api_key()?;
        let secret_key = resolve_secret_key()?;
        let region = resolve_region();
        let base_url = bedrock_base_url(&region);
        Self::with_base_url(api_key, secret_key, region, base_url)
    }

    /// Create with explicit credentials and a custom base URL (for
    /// proxies, self-hosted deployments, or non-standard regions).
    pub fn with_base_url(
        api_key: String,
        secret_key: String,
        region: String,
        base_url: String,
    ) -> Result<Self, Error> {
        let http_client = reqwest::Client::builder()
            .user_agent(format!("rustcode/{}", env!("CARGO_PKG_VERSION")))
            .timeout(std::time::Duration::from_secs(300))
            .build()
            .map_err(|e| Error::Network(format!("HTTP client: {e}")))?;
        Ok(Self {
            api_key,
            secret_key,
            region,
            base_url,
            http_client,
            models: build_model_catalog(),
        })
    }

    fn chat_url(&self) -> String {
        format!("{}{CHAT_PATH}", self.base_url.trim_end_matches('/'))
    }
}

// ── Model Catalog ──────────────────────────────────────────────────────

fn build_model_catalog() -> Vec<Model> {
    vec![
        make_model(
            "claude-sonnet-4-20250514",
            "Claude Sonnet 4",
            200_000,
            8_192,
            "claude",
            true,
            true,
        ),
        make_model(
            "claude-opus-4-20250514",
            "Claude Opus 4",
            200_000,
            8_192,
            "claude",
            true,
            true,
        ),
        make_model(
            "claude-haiku-4-20250514",
            "Claude Haiku 4",
            200_000,
            8_192,
            "claude",
            true,
            false,
        ),
        make_model(
            "llama-4-maverick",
            "Llama 4 Maverick",
            128_000,
            4_096,
            "llama",
            true,
            false,
        ),
        make_model(
            "titan-text",
            "Titan Text",
            8_000,
            4_096,
            "titan",
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
        provider_id: "bedrock".into(),
        name: name.into(),
        api: crate::provider::ApiInfo {
            id: id.into(),
            url: BASE_URL.into(),
            npm: "@ai-sdk/amazon-bedrock".into(),
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
impl Provider for BedrockProvider {
    fn provider_id(&self) -> &str {
        "bedrock"
    }

    fn npm(&self) -> &str {
        "@ai-sdk/amazon-bedrock"
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
                provider_id: "bedrock".into(),
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

        let body_json = serde_json::to_vec(&body)
            .map_err(|e| Error::Network(format!("Bedrock body serialization: {e}")))?;

        let signer = SigV4Signer::new(&self.api_key, &self.secret_key, &self.region);
        let now = Utc::now().format("%Y%m%dT%H%M%SZ").to_string();
        let url = url::Url::parse(&self.chat_url())
            .map_err(|e| Error::Network(format!("Bedrock URL parse: {e}")))?;
        let host = url.host_str().unwrap_or("");
        let path = url.path();

        let authorization = signer
            .sign("POST", host, path, &body_json, &now)
            .map_err(|e| Error::Network(format!("Bedrock SigV4: {e}")))?;

        let response = self
            .http_client
            .post(self.chat_url())
            .header("Content-Type", "application/json")
            .header("Authorization", authorization)
            .header("X-Amz-Date", &now)
            .header(
                "X-Amz-Content-Sha256",
                hex::encode(Sha256::digest(&body_json)),
            )
            .body(body_json)
            .send()
            .await
            .map_err(|e| Error::Network(format!("Bedrock request: {e}")))?;

        if !response.status().is_success() {
            let status = response.status().as_u16();
            let text = response.text().await.unwrap_or_default();
            return Err(Error::Llm {
                module: "bedrock".into(),
                method: "stream".into(),
                reason: Box::new(chat_completions::classify_error(status, &text)),
            });
        }

        Ok(chat_completions::create_chat_stream(response, "bedrock"))
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
        assert_eq!(models.len(), 5, "expected 5 models in catalog");
    }

    #[test]
    fn test_model_catalog_ids() {
        let models = build_model_catalog();
        let ids: Vec<&str> = models.iter().map(|m| m.id.as_str()).collect();
        assert!(ids.contains(&"claude-sonnet-4-20250514"));
        assert!(ids.contains(&"claude-opus-4-20250514"));
        assert!(ids.contains(&"claude-haiku-4-20250514"));
        assert!(ids.contains(&"llama-4-maverick"));
        assert!(ids.contains(&"titan-text"));
    }

    #[test]
    fn test_model_catalog_names() {
        let models = build_model_catalog();
        let names: Vec<&str> = models.iter().map(|m| m.name.as_str()).collect();
        assert!(names.contains(&"Claude Sonnet 4"));
        assert!(names.contains(&"Claude Opus 4"));
        assert!(names.contains(&"Claude Haiku 4"));
        assert!(names.contains(&"Llama 4 Maverick"));
        assert!(names.contains(&"Titan Text"));
    }

    #[test]
    fn test_model_catalog_provider_id() {
        let models = build_model_catalog();
        for m in &models {
            assert_eq!(m.provider_id, "bedrock");
        }
    }

    #[test]
    fn test_model_catalog_npm() {
        let models = build_model_catalog();
        for m in &models {
            assert_eq!(m.api.npm, "@ai-sdk/amazon-bedrock");
        }
    }

    #[test]
    fn test_model_catalog_context_windows() {
        let models = build_model_catalog();
        let sonnet4 = models.iter().find(|m| m.id == "claude-sonnet-4-20250514").unwrap();
        assert_eq!(sonnet4.limit.context, 200_000);
        let opus4 = models.iter().find(|m| m.id == "claude-opus-4-20250514").unwrap();
        assert_eq!(opus4.limit.context, 200_000);
        let haiku4 = models.iter().find(|m| m.id == "claude-haiku-4-20250514").unwrap();
        assert_eq!(haiku4.limit.context, 200_000);
        let llama = models.iter().find(|m| m.id == "llama-4-maverick").unwrap();
        assert_eq!(llama.limit.context, 128_000);
        let titan = models.iter().find(|m| m.id == "titan-text").unwrap();
        assert_eq!(titan.limit.context, 8_000);
    }

    #[test]
    fn test_model_catalog_output_tokens() {
        let models = build_model_catalog();
        for id in &["claude-sonnet-4-20250514", "claude-opus-4-20250514", "claude-haiku-4-20250514"] {
            let m = models.iter().find(|m| m.id == *id).unwrap();
            assert_eq!(m.limit.output, 8_192, "model {id} output mismatch");
        }
        let llama = models.iter().find(|m| m.id == "llama-4-maverick").unwrap();
        assert_eq!(llama.limit.output, 4_096);
        let titan = models.iter().find(|m| m.id == "titan-text").unwrap();
        assert_eq!(titan.limit.output, 4_096);
    }

    #[test]
    fn test_model_catalog_capabilities() {
        let models = build_model_catalog();
        let sonnet = models.iter().find(|m| m.id == "claude-sonnet-4-20250514").unwrap();
        assert!(sonnet.capabilities.temperature && sonnet.capabilities.reasoning && sonnet.capabilities.toolcall);
        let haiku = models.iter().find(|m| m.id == "claude-haiku-4-20250514").unwrap();
        assert!(!haiku.capabilities.reasoning);
    }

    #[test]
    fn test_model_catalog_families() {
        let models = build_model_catalog();
        let sonnet = models.iter().find(|m| m.id == "claude-sonnet-4-20250514").unwrap();
        assert_eq!(sonnet.family.as_deref(), Some("claude"));
        let llama = models.iter().find(|m| m.id == "llama-4-maverick").unwrap();
        assert_eq!(llama.family.as_deref(), Some("llama"));
        let titan = models.iter().find(|m| m.id == "titan-text").unwrap();
        assert_eq!(titan.family.as_deref(), Some("titan"));
    }

    // ── Provider trait ───────────────────────────────────────────

    #[test]
    fn test_provider_trait_provider_id() {
        let provider = BedrockProvider::with_base_url("test-ak".into(), "test-sk".into(), "us-east-1".into(), "https://bedrock-runtime.us-east-1.amazonaws.com".into()).unwrap();
        assert_eq!(provider.provider_id(), "bedrock");
        assert_eq!(provider.npm(), "@ai-sdk/amazon-bedrock");
    }

    #[test]
    fn test_base_url_format() {
        assert_eq!(bedrock_base_url("us-west-2"), "https://bedrock-runtime.us-west-2.amazonaws.com");
    }

    #[test]
    fn test_chat_url_without_trailing_slash() {
        let provider = BedrockProvider::with_base_url("test-ak".into(), "test-sk".into(), "us-east-1".into(), "https://bedrock-runtime.us-east-1.amazonaws.com".into()).unwrap();
        assert_eq!(provider.chat_url(), "https://bedrock-runtime.us-east-1.amazonaws.com/chat/completions");
    }
}
