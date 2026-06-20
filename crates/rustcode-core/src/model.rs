//! Model types — ModelV2, model requests, and ModelsDev catalog.
//!
//! Ported from: `packages/core/src/model.ts`
//!              `packages/core/src/model-request.ts`
//!              `packages/core/src/models-dev.ts`
//! OpenCode commit: 5d0f86606ac30690f79f0a6a9f41a1f49fe95d0b
//!
//! ## Overview
//!
//! Three type systems consolidated here:
//!
//! - [`ModelV2`] — the canonical model information type used throughout the
//!   agent/provider system. Includes ID, capabilities, cost, limit, and
//!   request configuration.
//!
//! - [`ModelRequest`] — generation parameters (temperature, maxTokens, etc.)
//!   and request body/headers configuration. Includes the merge and
//!   normalize utilities used by the catalog loader.
//!
//! - [`ModelsDev`] — the models.dev catalog fetcher types. Defines the
//!   `Provider`, `Model`, and `Cost` shapes as they appear in the remote
//!   `api.json` catalog.

use serde::{Deserialize, Serialize};
use std::collections::HashMap;

// ═════════════════════════════════════════════════════════════════════════
// ModelV2 types
// ═════════════════════════════════════════════════════════════════════════

/// ModelV2 branded identifier (e.g., `"claude-sonnet-4-20250514"`).
///
/// # Source
/// Ported from `packages/core/src/model.ts` lines 6–7
/// (`ID = Schema.String.pipe(Schema.brand("ModelV2.ID"))`).
pub type ModelV2Id = String;

/// Variant identifier for a model variant.
///
/// # Source
/// Ported from `packages/core/src/model.ts` lines 9–10
/// (`VariantID = Schema.String.pipe(Schema.brand("VariantID"))`).
pub type VariantId = String;

/// Model family grouping (e.g., `"claude"`, `"gpt"`).
///
/// # Source
/// Ported from `packages/core/src/model.ts` lines 13–14
/// (`Family = Schema.String.pipe(Schema.brand("Family"))`).
pub type Family = String;

/// Provider V2 branded identifier (e.g., `"anthropic"`, `"openai"`).
///
/// # Source
/// Ported from `packages/core/src/provider.ts` lines 6–22
/// (`ID = Schema.String.pipe(Schema.brand("ProviderV2.ID"))`).
pub type ProviderV2Id = String;

// ── Well-known provider IDs ─────────────────────────────────────────────

/// Well-known provider IDs from the TS codebase.
///
/// # Source
/// Ported from `packages/core/src/provider.ts` lines 9–21
/// (static `.opencode`, `.anthropic`, etc. on `ProviderV2.ID`).
pub mod well_known_providers {
    /// The built-in opencode provider
    pub const OPENCODE: &str = "opencode";
    /// Anthropic (Claude models)
    pub const ANTHROPIC: &str = "anthropic";
    /// OpenAI (GPT models)
    pub const OPENAI: &str = "openai";
    /// Google (Gemini models)
    pub const GOOGLE: &str = "google";
    /// Google Vertex AI
    pub const GOOGLE_VERTEX: &str = "google-vertex";
    /// GitHub Copilot
    pub const GITHUB_COPILOT: &str = "github-copilot";
    /// Amazon Bedrock
    pub const AMAZON_BEDROCK: &str = "amazon-bedrock";
    /// Microsoft Azure
    pub const AZURE: &str = "azure";
    /// OpenRouter
    pub const OPENROUTER: &str = "openrouter";
    /// Mistral AI
    pub const MISTRAL: &str = "mistral";
    /// GitLab AI
    pub const GITLAB: &str = "gitlab";
}

// ── Model capabilities ──────────────────────────────────────────────────

/// Model capabilities (tool calling, input/output modalities).
///
/// # Source
/// Ported from `packages/core/src/model.ts` lines 16–21
/// (`Capabilities` struct — `tools`, `input`, `output`).
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct Capabilities {
    /// Whether the model supports tool calling
    #[serde(default)]
    pub tools: bool,
    /// MIME patterns the model accepts as input (e.g., `["image/*", "text/*"]`)
    #[serde(default)]
    pub input: Vec<String>,
    /// MIME patterns the model produces as output
    #[serde(default)]
    pub output: Vec<String>,
}

// ── Model cost ──────────────────────────────────────────────────────────

/// Tiered pricing information for a model.
///
/// # Source
/// Ported from `packages/core/src/model.ts` lines 24–35
/// (`Cost` struct with `tier`, `input`, `output`, `cache`).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ModelCost {
    /// Optional context-size-based tier
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tier: Option<CostTier>,
    /// Cost per 1M input tokens
    pub input: f64,
    /// Cost per 1M output tokens
    pub output: f64,
    /// Cache pricing
    #[serde(default)]
    pub cache: CacheCost,
}

/// Context window tier discriminator.
///
/// # Source
/// Ported from `packages/core/src/model.ts` lines 25–28
/// (`tier: { type: "context", size: Int }`).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CostTier {
    /// Always `"context"` for context-window-based tiers
    #[serde(rename = "type")]
    pub tier_type: String,
    /// Context window size this tier applies at
    pub size: i32,
}

/// Cache token pricing.
///
/// # Source
/// Ported from `packages/core/src/model.ts` lines 31–34
/// (`cache: { read: Finite, write: Finite }`).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CacheCost {
    /// Cost per 1M cache-read tokens
    #[serde(default)]
    pub read: f64,
    /// Cost per 1M cache-write tokens
    #[serde(default)]
    pub write: f64,
}

impl Default for CacheCost {
    fn default() -> Self {
        Self {
            read: 0.0,
            write: 0.0,
        }
    }
}

// ── Model reference ─────────────────────────────────────────────────────

/// A lightweight reference to a model (used in session config).
///
/// # Source
/// Ported from `packages/core/src/model.ts` lines 37–41
/// (`Ref` struct — `id`, `providerID`, optional `variant`).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ModelRef {
    /// The model ID
    pub id: ModelV2Id,
    /// The provider ID
    #[serde(rename = "providerID")]
    pub provider_id: ProviderV2Id,
    /// Optional variant selection
    #[serde(skip_serializing_if = "Option::is_none")]
    pub variant: Option<VariantId>,
}

// ── Model API descriptor ────────────────────────────────────────────────

/// AI-SDK-based model API descriptor.
///
/// # Source
/// Ported from `packages/core/src/provider.ts` lines 25–30
/// (`AISDK` struct — `type: "aisdk"`, `package`, `url`, `settings`).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AiSdkApi {
    /// The model ID for this API
    pub id: ModelV2Id,
    /// The npm package name (e.g., `"@ai-sdk/anthropic"`)
    pub package: String,
    /// Optional base URL override
    #[serde(skip_serializing_if = "Option::is_none")]
    pub url: Option<String>,
    /// Provider-specific settings
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub settings: Option<HashMap<String, serde_json::Value>>,
}

/// Native provider model API descriptor.
///
/// # Source
/// Ported from `packages/core/src/provider.ts` lines 32–36
/// (`Native` struct — `type: "native"`, `url`, `settings`).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NativeApi {
    /// The model ID for this API
    pub id: ModelV2Id,
    /// Optional base URL override
    #[serde(skip_serializing_if = "Option::is_none")]
    pub url: Option<String>,
    /// Provider-specific settings
    #[serde(default)]
    pub settings: HashMap<String, serde_json::Value>,
}

/// Discriminated union of API descriptors.
///
/// # Source
/// Ported from `packages/core/src/model.ts` lines 44–53
/// (`Api = Schema.Union([AISDK, Native]).pipe(Schema.toTaggedUnion("type"))`).
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type")]
pub enum ModelApi {
    /// AI-SDK provider
    #[serde(rename = "aisdk")]
    AiSdk(AiSdkApi),
    /// Native provider
    #[serde(rename = "native")]
    Native(NativeApi),
}

// ── Model limits ────────────────────────────────────────────────────────

/// Model context and output limits.
///
/// # Source
/// Ported from `packages/core/src/model.ts` lines 77–81
/// (`limit: { context: Int, input?: Int, output: Int }`).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ModelLimits {
    /// Maximum context window size (tokens)
    pub context: i32,
    /// Maximum input tokens (optional — may equal context)
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub input: Option<i32>,
    /// Maximum output tokens
    pub output: i32,
}

// ── Model time ──────────────────────────────────────────────────────────

/// Model release timestamp.
///
/// # Source
/// Ported from `packages/core/src/model.ts` lines 72–74
/// (`time: { released: DateTimeUtcFromMillis }`).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ModelTime {
    /// Release date (epoch millis)
    pub released: i64,
}

// ── Model status ────────────────────────────────────────────────────────

/// Model lifecycle status.
///
/// # Source
/// Ported from `packages/core/src/model.ts` line 75
/// (`status: Schema.Literals(["alpha", "beta", "deprecated", "active"])`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum ModelStatus {
    /// Experimental / early access
    Alpha,
    /// Preview / pre-release
    Beta,
    /// No longer recommended
    Deprecated,
    /// Fully supported
    Active,
}

// ── ModelV2 Info ────────────────────────────────────────────────────────

/// Full model information record (ModelV2.Info).
///
/// This is the canonical model descriptor used by the provider and agent
/// system. Every model available for use has one of these records.
///
/// # Source
/// Ported from `packages/core/src/model.ts` lines 56–117
/// (`class Info extends Schema.Class<Info>("ModelV2.Info")`).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ModelInfo {
    /// Unique model identifier
    pub id: ModelV2Id,
    /// Provider that serves this model
    #[serde(rename = "providerID")]
    pub provider_id: ProviderV2Id,
    /// Model family (e.g., `"claude"`, `"gpt"`)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub family: Option<Family>,
    /// Human-readable display name
    pub name: String,
    /// API descriptor
    pub api: ModelApi,
    /// Model capabilities
    pub capabilities: Capabilities,
    /// Default request configuration
    pub request: ModelRequestConfig,
    /// Available variants
    #[serde(default)]
    pub variants: Vec<ModelVariant>,
    /// Release timestamp
    pub time: ModelTime,
    /// Pricing tiers
    #[serde(default)]
    pub cost: Vec<ModelCost>,
    /// Lifecycle status
    pub status: ModelStatus,
    /// Whether the model is enabled
    #[serde(default = "default_enabled")]
    pub enabled: bool,
    /// Token limits
    pub limit: ModelLimits,
}

const fn default_enabled() -> bool {
    true
}

/// Default request configuration embedded in model info.
///
/// # Source
/// Ported from `packages/core/src/model.ts` lines 62–66
/// (`request: { ...ModelRequest.Request.fields, variant?: String }`).
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct ModelRequestConfig {
    /// Default HTTP headers for API calls
    #[serde(default)]
    pub headers: HashMap<String, String>,
    /// Default request body parameters
    #[serde(default)]
    pub body: HashMap<String, serde_json::Value>,
    /// Default generation parameters
    #[serde(default)]
    pub generation: GenerationParams,
    /// Additional options
    #[serde(default)]
    pub options: HashMap<String, serde_json::Value>,
    /// Optional variant identifier
    #[serde(skip_serializing_if = "Option::is_none")]
    pub variant: Option<String>,
}

/// A model variant with its own request configuration.
///
/// # Source
/// Ported from `packages/core/src/model.ts` lines 67–70
/// (`variants: { id: VariantID, ...ModelRequest.Request.fields }`).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ModelVariant {
    /// Variant identifier
    pub id: VariantId,
    /// Headers specific to this variant
    #[serde(default)]
    pub headers: HashMap<String, String>,
    /// Body specific to this variant
    #[serde(default)]
    pub body: HashMap<String, serde_json::Value>,
    /// Generation parameters for this variant
    #[serde(default)]
    pub generation: GenerationParams,
    /// Options specific to this variant
    #[serde(default)]
    pub options: HashMap<String, serde_json::Value>,
}

impl ModelInfo {
    /// Create an empty/default model info record (mirrors TS `ModelV2.Info.empty()`).
    ///
    /// # Source
    /// Ported from `packages/core/src/model.ts` lines 83–116
    /// (`static empty(providerID, modelID)`).
    pub fn empty(provider_id: ProviderV2Id, model_id: ModelV2Id) -> Self {
        Self {
            id: model_id.clone(),
            provider_id,
            family: None,
            name: model_id.clone(),
            api: ModelApi::Native(NativeApi {
                id: model_id,
                url: None,
                settings: HashMap::new(),
            }),
            capabilities: Capabilities::default(),
            request: ModelRequestConfig::default(),
            variants: vec![],
            time: ModelTime { released: 0 },
            cost: vec![],
            status: ModelStatus::Active,
            enabled: true,
            limit: ModelLimits {
                context: 0,
                input: None,
                output: 0,
            },
        }
    }
}

// ── Model parsing ───────────────────────────────────────────────────────

/// Parsed model reference from a string like `"anthropic/claude-sonnet-4-20250514"`.
///
/// # Source
/// Ported from `packages/core/src/model.ts` lines 119–126
/// (`parse(input: string)`).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ParsedModelRef {
    /// The provider ID (first segment before `/`)
    pub provider_id: ProviderV2Id,
    /// The model ID (everything after the first `/`)
    pub model_id: ModelV2Id,
}

/// Parse a `"provider/model"` string into a [`ParsedModelRef`].
///
/// # Source
/// Ported from `packages/core/src/model.ts` lines 119–126
/// (`export function parse(input: string)`).
pub fn parse_model_ref(input: &str) -> Option<ParsedModelRef> {
    let mut parts = input.splitn(2, '/');
    let provider_id = parts.next()?.to_string();
    let model_id = parts.next().unwrap_or("").to_string();
    if provider_id.is_empty() || model_id.is_empty() {
        return None;
    }
    Some(ParsedModelRef {
        provider_id,
        model_id,
    })
}

// ═════════════════════════════════════════════════════════════════════════
// ModelRequest types
// ═════════════════════════════════════════════════════════════════════════

/// Generation parameters for a model request.
///
/// # Source
/// Ported from `packages/core/src/model-request.ts` lines 5–14
/// (`Generation` struct).
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct GenerationParams {
    /// Maximum tokens to generate
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub max_tokens: Option<f64>,
    /// Sampling temperature (0–2)
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub temperature: Option<f64>,
    /// Top-P (nucleus) sampling
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub top_p: Option<f64>,
    /// Top-K sampling
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub top_k: Option<f64>,
    /// Frequency penalty
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub frequency_penalty: Option<f64>,
    /// Presence penalty
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub presence_penalty: Option<f64>,
    /// Random seed
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub seed: Option<f64>,
    /// Stop sequences
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub stop: Option<Vec<String>>,
}

/// Full model request configuration (headers + body + generation + options).
///
/// # Source
/// Ported from `packages/core/src/model-request.ts` lines 17–30
/// (`Request` struct).
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct ModelRequest {
    /// HTTP headers to send with the request
    #[serde(default)]
    pub headers: HashMap<String, String>,
    /// Request body parameters
    #[serde(default)]
    pub body: HashMap<String, serde_json::Value>,
    /// Generation parameter overrides
    #[serde(default)]
    pub generation: GenerationParams,
    /// Additional provider-specific options
    #[serde(default)]
    pub options: HashMap<String, serde_json::Value>,
}

/// Map of known generation parameter key names to their canonical field names.
///
/// # Source
/// Ported from `packages/core/src/model-request.ts` lines 40–51
/// (`generationKeys` map).
pub fn generation_key_map() -> HashMap<&'static str, &'static str> {
    HashMap::from([
        ("maxOutputTokens", "max_tokens"),
        ("maxTokens", "max_tokens"),
        ("temperature", "temperature"),
        ("topP", "top_p"),
        ("topK", "top_k"),
        ("frequencyPenalty", "frequency_penalty"),
        ("presencePenalty", "presence_penalty"),
        ("seed", "seed"),
        ("stopSequences", "stop"),
        ("stop", "stop"),
    ])
}

/// Get the SDK provider namespace for a given AI-SDK package name.
///
/// # Source
/// Ported from `packages/core/src/model-request.ts` line 90
/// (`namespace(packageName)`).
pub fn ai_sdk_namespace(package_name: &str) -> Option<&'static str> {
    match package_name {
        "@ai-sdk/openai" | "@ai-sdk/openai-compatible" => Some("openai"),
        "@ai-sdk/anthropic" => Some("anthropic"),
        _ => None,
    }
}

/// Get semantic key mappings for a given AI-SDK package.
///
/// # Source
/// Ported from `packages/core/src/model-request.ts` lines 58–88
/// (the `profiles` map — semantic key mappings per package).
pub fn ai_sdk_semantics(package_name: &str) -> HashMap<&'static str, &'static str> {
    match package_name {
        "@ai-sdk/openai" => HashMap::from([
            ("store", "store"),
            ("promptCacheKey", "promptCacheKey"),
            ("reasoningEffort", "reasoningEffort"),
            ("reasoningSummary", "reasoningSummary"),
            ("include", "include"),
            ("textVerbosity", "textVerbosity"),
            ("serviceTier", "serviceTier"),
            ("service_tier", "serviceTier"),
        ]),
        "@ai-sdk/openai-compatible" => HashMap::from([
            ("store", "store"),
            ("promptCacheKey", "promptCacheKey"),
            ("reasoningEffort", "reasoningEffort"),
            ("reasoning_effort", "reasoningEffort"),
        ]),
        "@ai-sdk/anthropic" => HashMap::from([("thinking", "thinking")]),
        _ => HashMap::new(),
    }
}

/// Merge two model request configs (base + override).
///
/// Override values take precedence. Generation params are shallow-merged
/// so individual fields can be overridden without losing the rest.
///
/// # Source
/// Ported from `packages/core/src/model-request.ts` lines 92–97
/// (`merge(base, override)`).
pub fn merge_model_request(base: &ModelRequest, override_config: &ModelRequest) -> ModelRequest {
    let mut headers = base.headers.clone();
    headers.extend(override_config.headers.clone());

    let mut body = base.body.clone();
    body.extend(override_config.body.clone());

    let mut options = base.options.clone();
    options.extend(override_config.options.clone());

    // Generation is shallow-merged
    let gen = &override_config.generation;
    let base_gen = &base.generation;

    // We use default() and selectively override
    let generation = GenerationParams {
        max_tokens: gen.max_tokens.or(base_gen.max_tokens),
        temperature: gen.temperature.or(base_gen.temperature),
        top_p: gen.top_p.or(base_gen.top_p),
        top_k: gen.top_k.or(base_gen.top_k),
        frequency_penalty: gen.frequency_penalty.or(base_gen.frequency_penalty),
        presence_penalty: gen.presence_penalty.or(base_gen.presence_penalty),
        seed: gen.seed.or(base_gen.seed),
        stop: gen.stop.clone().or_else(|| base_gen.stop.clone()),
    };

    ModelRequest {
        headers,
        body,
        generation,
        options,
    }
}

/// Partition AI-SDK-shaped request options into generation, options, and body.
///
/// Mirrors the TS `normalizeAiSdkOptions` function which distributes
/// a flat key-value bag into the three canonical categories.
///
/// # Source
/// Ported from `packages/core/src/model-request.ts` lines 107–124
/// (`normalizeAiSdkOptions(packageName, input)`).
pub fn normalize_ai_sdk_options(
    package_name: Option<&str>,
    input: &HashMap<String, serde_json::Value>,
) -> NormalizedOptions {
    let gen_keys = generation_key_map();
    let semantics = package_name.map_or(HashMap::new(), ai_sdk_semantics);

    let mut generation: HashMap<String, serde_json::Value> = HashMap::new();
    let mut options: HashMap<String, serde_json::Value> = HashMap::new();
    let mut body: HashMap<String, serde_json::Value> = HashMap::new();

    for (key, value) in input {
        if let Some(&gen_key) = gen_keys.get(key.as_str()) {
            // stop sequences: only assign if value is an array of strings
            if gen_key == "stop" {
                if let serde_json::Value::Array(arr) = value {
                    if arr.iter().all(|v| v.is_string()) {
                        generation.insert(gen_key.to_string(), value.clone());
                        continue;
                    }
                }
            } else if value.is_number() {
                generation.insert(gen_key.to_string(), value.clone());
                continue;
            }
        }

        if let Some(&semantic_key) = semantics.get(key.as_str()) {
            options.insert(semantic_key.to_string(), value.clone());
        } else {
            body.insert(key.clone(), value.clone());
        }
    }

    NormalizedOptions {
        generation,
        options,
        body,
    }
}

/// Result of normalizing AI-SDK options.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NormalizedOptions {
    /// Generation parameters
    #[serde(default)]
    pub generation: HashMap<String, serde_json::Value>,
    /// Provider options
    #[serde(default)]
    pub options: HashMap<String, serde_json::Value>,
    /// Request body parameters
    #[serde(default)]
    pub body: HashMap<String, serde_json::Value>,
}

// ═════════════════════════════════════════════════════════════════════════
// ModelsDev catalog types
// ═════════════════════════════════════════════════════════════════════════

/// Catalog model status (restricted subset of lifecycle).
///
/// # Source
/// Ported from `packages/core/src/models-dev.ts` line 14
/// (`CatalogModelStatus = Schema.Literals(["alpha", "beta", "deprecated"])`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum CatalogModelStatus {
    /// Experimental
    Alpha,
    /// Preview
    Beta,
    /// No longer recommended
    Deprecated,
}

/// Cost tier from the models.dev catalog.
///
/// # Source
/// Ported from `packages/core/src/models-dev.ts` lines 19–28
/// (`CostTier` — tiered pricing with context window size).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CatalogCostTier {
    /// Input cost per 1M tokens
    pub input: f64,
    /// Output cost per 1M tokens
    pub output: f64,
    /// Cache read cost (optional)
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub cache_read: Option<f64>,
    /// Cache write cost (optional)
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub cache_write: Option<f64>,
    /// The tier discriminator
    pub tier: CatalogTierDiscriminator,
}

/// Context-window-based tier discriminator.
///
/// # Source
/// Ported from `packages/core/src/models-dev.ts` lines 24–27
/// (`tier: { type: "context", size: Finite }`).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CatalogTierDiscriminator {
    #[serde(rename = "type")]
    pub tier_type: String,
    pub size: f64,
}

/// Cost entry from the models.dev catalog.
///
/// # Source
/// Ported from `packages/core/src/models-dev.ts` lines 30–44
/// (`Cost` struct with optional tiers and context_over_200k).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CatalogCost {
    /// Input cost per 1M tokens
    pub input: f64,
    /// Output cost per 1M tokens
    pub output: f64,
    /// Cache read cost (optional)
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub cache_read: Option<f64>,
    /// Cache write cost (optional)
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub cache_write: Option<f64>,
    /// Tiered pricing (optional)
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub tiers: Option<Vec<CatalogCostTier>>,
    /// Pricing for contexts over 200K tokens (optional)
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub context_over_200k: Option<CatalogContextOver200k>,
}

/// Pricing surcharge for contexts over 200K tokens.
///
/// # Source
/// Ported from `packages/core/src/models-dev.ts` lines 36–43
/// (`context_over_200k: { input, output, cache_read?, cache_write? }`).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CatalogContextOver200k {
    pub input: f64,
    pub output: f64,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub cache_read: Option<f64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub cache_write: Option<f64>,
}

/// Model modalities (input/output types).
///
/// # Source
/// Ported from `packages/core/src/models-dev.ts` lines 69–74
/// (`modalities: { input: [...], output: [...] }`).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ModelModalities {
    /// Supported input modalities
    #[serde(default)]
    pub input: Vec<ModelModality>,
    /// Supported output modalities
    #[serde(default)]
    pub output: Vec<ModelModality>,
}

/// Individual modality types.
///
/// # Source
/// Ported from `packages/core/src/models-dev.ts` line 71
/// (`Literals(["text", "audio", "image", "video", "pdf"])`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum ModelModality {
    /// Plain text
    Text,
    /// Audio
    Audio,
    /// Image
    Image,
    /// Video
    Video,
    /// PDF document
    Pdf,
}

/// Model limits from the models.dev catalog.
///
/// # Source
/// Ported from `packages/core/src/models-dev.ts` lines 64–68
/// (`limit: { context: Finite, input?: Finite, output: Finite }`).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CatalogModelLimits {
    /// Maximum context window
    pub context: f64,
    /// Maximum input tokens (optional)
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub input: Option<f64>,
    /// Maximum output tokens
    pub output: f64,
}

/// Interleaved reasoning configuration.
///
/// # Source
/// Ported from `packages/core/src/models-dev.ts` lines 55–61
/// (`interleaved: true | { field: "reasoning" | "reasoning_content" | "reasoning_details" }`).
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(untagged)]
pub enum InterleavedReasoning {
    /// Simple boolean — interleaved is enabled
    Bool(bool),
    /// Object form with field specification
    Field {
        /// Which field contains the interleaved reasoning
        field: String,
    },
}

/// Provider-specific overrides in the models.dev catalog.
///
/// # Source
/// Ported from `packages/core/src/models-dev.ts` lines 83–93
/// (nested `experimental.modes.provider` struct).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ExperimentalModeProvider {
    /// Body overrides
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub body: Option<HashMap<String, serde_json::Value>>,
    /// Header overrides
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub headers: Option<HashMap<String, String>>,
}

/// Experimental mode entry.
///
/// # Source
/// Ported from `packages/core/src/models-dev.ts` lines 80–92
/// (nested `experimental.modes` entry).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ExperimentalMode {
    /// Cost overrides for this mode
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub cost: Option<CatalogCost>,
    /// Provider-specific overrides
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub provider: Option<ExperimentalModeProvider>,
}

// ── ModelsDev model entry ───────────────────────────────────────────────

/// A single model entry from the models.dev catalog.
///
/// # Source
/// Ported from `packages/core/src/models-dev.ts` lines 46–96
/// (`Model` struct — the full catalog model descriptor).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CatalogModel {
    /// Model identifier
    pub id: String,
    /// Human-readable name
    pub name: String,
    /// Optional model family
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub family: Option<String>,
    /// Release date (ISO format string)
    pub release_date: String,
    /// Whether the model supports attachment/vision
    #[serde(default)]
    pub attachment: bool,
    /// Whether the model supports reasoning/thinking
    #[serde(default)]
    pub reasoning: bool,
    /// Whether the model supports temperature
    #[serde(default)]
    pub temperature: bool,
    /// Whether the model supports tool calling
    #[serde(default)]
    pub tool_call: bool,
    /// Interleaved reasoning configuration
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub interleaved: Option<InterleavedReasoning>,
    /// Cost information
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub cost: Option<CatalogCost>,
    /// Token limits
    pub limit: CatalogModelLimits,
    /// Supported modalities
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub modalities: Option<ModelModalities>,
    /// Experimental features
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub experimental: Option<CatalogExperimental>,
    /// Lifecycle status
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub status: Option<CatalogModelStatus>,
    /// Provider metadata
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub provider: Option<CatalogModelProvider>,
}

/// Experimental features container.
///
/// # Source
/// Ported from `packages/core/src/models-dev.ts` lines 75–93
/// (`experimental: { modes?: Record<string, { cost?, provider? }> }`).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CatalogExperimental {
    /// Named experimental modes
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub modes: Option<HashMap<String, ExperimentalMode>>,
}

/// Provider metadata from the models.dev catalog.
///
/// # Source
/// Ported from `packages/core/src/models-dev.ts` lines 95–96
/// (`provider: { npm?, api? }`).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CatalogModelProvider {
    /// Optional npm package name
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub npm: Option<String>,
    /// Optional API base URL
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub api: Option<String>,
}

// ── ModelsDev provider ──────────────────────────────────────────────────

/// A provider entry from the models.dev catalog.
///
/// # Source
/// Ported from `packages/core/src/models-dev.ts` lines 100–108
/// (`Provider` struct — provider with its models).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CatalogProvider {
    /// Optional API base URL
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub api: Option<String>,
    /// Provider display name
    pub name: String,
    /// Required environment variables
    #[serde(default)]
    pub env: Vec<String>,
    /// Provider identifier
    pub id: String,
    /// Optional npm package name
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub npm: Option<String>,
    /// Models provided by this provider, keyed by model ID
    #[serde(default)]
    pub models: HashMap<String, CatalogModel>,
}

/// The full models.dev catalog — map of provider ID to provider.
pub type Catalog = HashMap<String, CatalogProvider>;

// ── Constant: default catalog URL ───────────────────────────────────────

/// Default URL for the models.dev catalog JSON endpoint.
///
/// # Source
/// Ported from `packages/core/src/models-dev.ts` line 142
/// (`const source = Flag.OPENCODE_MODELS_URL || "https://models.dev"`).
pub const DEFAULT_CATALOG_URL: &str = "https://models.dev";

/// Path to the local models.dev cache file.
pub const DEFAULT_CATALOG_CACHE_FILE: &str = "models.json";

// ── Events ──────────────────────────────────────────────────────────────

/// Event emitted when the models.dev catalog is refreshed.
///
/// # Source
/// Ported from `packages/core/src/models-dev.ts` lines 111–116
/// (`Event.Refreshed`).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ModelsDevRefreshedEvent {
    /// Event type discriminator
    #[serde(rename = "type")]
    pub event_type: String,
}

impl Default for ModelsDevRefreshedEvent {
    fn default() -> Self {
        Self {
            event_type: "models-dev.refreshed".to_string(),
        }
    }
}

// ── Tests ──────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    // ── parse_model_ref tests ───────────────────────────────────────

    #[test]
    fn parse_simple_ref() {
        let parsed = parse_model_ref("anthropic/claude-sonnet").unwrap();
        assert_eq!(parsed.provider_id, "anthropic");
        assert_eq!(parsed.model_id, "claude-sonnet");
    }

    #[test]
    fn parse_ref_with_slashes_in_model() {
        let parsed = parse_model_ref("openai/gpt-4/turbo").unwrap();
        assert_eq!(parsed.provider_id, "openai");
        assert_eq!(parsed.model_id, "gpt-4/turbo");
    }

    #[test]
    fn parse_ref_no_slash() {
        assert!(parse_model_ref("justamodel").is_none());
    }

    #[test]
    fn parse_ref_empty() {
        assert!(parse_model_ref("").is_none());
    }

    // ── ModelInfo.empty tests ───────────────────────────────────────

    #[test]
    fn model_info_empty() {
        let info = ModelInfo::empty("test-provider".into(), "test-model".into());
        assert_eq!(info.id, "test-model");
        assert_eq!(info.provider_id, "test-provider");
        assert_eq!(info.status, ModelStatus::Active);
        assert!(info.enabled);
        assert_eq!(info.limit.context, 0);
        assert_eq!(info.limit.output, 0);
        assert!(info.capabilities.input.is_empty());
        assert!(!info.capabilities.tools);
    }

    // ── merge_model_request tests ───────────────────────────────────

    #[test]
    fn merge_overrides_headers() {
        let base = ModelRequest {
            headers: HashMap::from([("A".into(), "1".into())]),
            ..Default::default()
        };
        let over = ModelRequest {
            headers: HashMap::from([("A".into(), "2".into()), ("B".into(), "3".into())]),
            ..Default::default()
        };
        let merged = merge_model_request(&base, &over);
        assert_eq!(merged.headers.get("A").unwrap(), "2");
        assert_eq!(merged.headers.get("B").unwrap(), "3");
    }

    #[test]
    fn merge_generation_overrides() {
        let base = ModelRequest {
            generation: GenerationParams {
                temperature: Some(0.7),
                max_tokens: Some(1000.0),
                ..Default::default()
            },
            ..Default::default()
        };
        let over = ModelRequest {
            generation: GenerationParams {
                temperature: Some(0.3),
                ..Default::default()
            },
            ..Default::default()
        };
        let merged = merge_model_request(&base, &over);
        assert_eq!(merged.generation.temperature, Some(0.3));
        assert_eq!(merged.generation.max_tokens, Some(1000.0)); // preserved from base
    }

    // ── normalize_ai_sdk_options tests ──────────────────────────────

    #[test]
    fn normalize_extracts_generation_params() {
        let input = HashMap::from([
            ("temperature".into(), serde_json::json!(0.5)),
            ("maxTokens".into(), serde_json::json!(2000)),
        ]);
        let result = normalize_ai_sdk_options(None, &input);
        assert!(result.generation.contains_key("temperature"));
        assert!(result.generation.contains_key("max_tokens"));
    }

    #[test]
    fn normalize_stop_sequences() {
        let input = HashMap::from([("stop".into(), serde_json::json!(["END", "DONE"]))]);
        let result = normalize_ai_sdk_options(None, &input);
        assert_eq!(
            result.generation.get("stop").unwrap(),
            &serde_json::json!(["END", "DONE"])
        );
    }

    #[test]
    fn normalize_semantic_keys() {
        let input = HashMap::from([
            ("reasoningEffort".into(), serde_json::json!("high")),
            ("unknownKey".into(), serde_json::json!(42)),
        ]);
        let result = normalize_ai_sdk_options(Some("@ai-sdk/openai"), &input);
        assert!(result.options.contains_key("reasoningEffort"));
        assert!(result.body.contains_key("unknownKey"));
    }

    // ── ai_sdk_namespace tests ──────────────────────────────────────

    #[test]
    fn namespace_known() {
        assert_eq!(ai_sdk_namespace("@ai-sdk/anthropic"), Some("anthropic"));
        assert_eq!(ai_sdk_namespace("@ai-sdk/openai"), Some("openai"));
        assert_eq!(
            ai_sdk_namespace("@ai-sdk/openai-compatible"),
            Some("openai")
        );
    }

    #[test]
    fn namespace_unknown() {
        assert_eq!(ai_sdk_namespace("@unknown/sdk"), None);
        assert_eq!(ai_sdk_namespace(""), None);
    }

    // ── generation_key_map tests ────────────────────────────────────

    #[test]
    fn generation_keys_known_mappings() {
        let map = generation_key_map();
        assert_eq!(map.get("maxOutputTokens"), Some(&"max_tokens"));
        assert_eq!(map.get("maxTokens"), Some(&"max_tokens"));
        assert_eq!(map.get("stopSequences"), Some(&"stop"));
        assert_eq!(map.get("stop"), Some(&"stop"));
    }

    // ── well_known_providers tests ──────────────────────────────────

    #[test]
    fn well_known_providers_values() {
        assert_eq!(well_known_providers::OPENCODE, "opencode");
        assert_eq!(well_known_providers::ANTHROPIC, "anthropic");
        assert_eq!(well_known_providers::OPENAI, "openai");
        assert_eq!(well_known_providers::GOOGLE, "google");
        assert_eq!(well_known_providers::OPENROUTER, "openrouter");
    }

    // ── Serialization roundtrip tests ───────────────────────────────

    #[test]
    fn model_info_serde_roundtrip() {
        let info = ModelInfo::empty("anthropic".into(), "claude-sonnet".into());
        let json = serde_json::to_string(&info).unwrap();
        let back: ModelInfo = serde_json::from_str(&json).unwrap();
        assert_eq!(info.id, back.id);
        assert_eq!(info.provider_id, back.provider_id);
        assert_eq!(info.status, back.status);
    }

    #[test]
    fn model_request_serde_roundtrip() {
        let req = ModelRequest {
            headers: HashMap::from([("Authorization".into(), "Bearer xyz".into())]),
            generation: GenerationParams {
                temperature: Some(0.5),
                ..Default::default()
            },
            ..Default::default()
        };
        let json = serde_json::to_string(&req).unwrap();
        let back: ModelRequest = serde_json::from_str(&json).unwrap();
        assert_eq!(back.headers.get("Authorization").unwrap(), "Bearer xyz");
        assert_eq!(back.generation.temperature, Some(0.5));
    }

    #[test]
    fn catalog_model_serde_roundtrip() {
        let model = CatalogModel {
            id: "gpt-4".into(),
            name: "GPT-4".into(),
            family: Some("gpt".into()),
            release_date: "2023-03-14".into(),
            attachment: true,
            reasoning: false,
            temperature: true,
            tool_call: true,
            interleaved: None,
            cost: None,
            limit: CatalogModelLimits {
                context: 8192.0,
                input: None,
                output: 4096.0,
            },
            modalities: None,
            experimental: None,
            status: None,
            provider: None,
        };
        let json = serde_json::to_string(&model).unwrap();
        let back: CatalogModel = serde_json::from_str(&json).unwrap();
        assert_eq!(back.id, "gpt-4");
        assert_eq!(back.limit.context, 8192.0);
    }

    // ── Cost / limit types tests ────────────────────────────────────

    #[test]
    fn model_limits_serde() {
        let limits = ModelLimits {
            context: 200000,
            input: Some(100000),
            output: 4096,
        };
        let json = serde_json::to_string(&limits).unwrap();
        assert!(json.contains("200000"));
        let back: ModelLimits = serde_json::from_str(&json).unwrap();
        assert_eq!(back.context, 200000);
        assert_eq!(back.input, Some(100000));
        assert_eq!(back.output, 4096);
    }

    #[test]
    fn model_status_display() {
        let json = serde_json::to_string(&ModelStatus::Active).unwrap();
        assert_eq!(json, r#""active""#);
        assert_eq!(
            serde_json::from_str::<ModelStatus>(r#""deprecated""#).unwrap(),
            ModelStatus::Deprecated
        );
    }
}
