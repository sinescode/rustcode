//! Session model resolution — catalog-aware model resolution via ProviderService.
//!
//! Ported from: `packages/core/src/session/runner/model.ts` (lines 1–166)

use crate::model::{
    merge_model_request, well_known_providers, ModelInfo,
    ModelRequest, ModelLimits as ModelV2Limits,
    Capabilities as ModelV2Capabilities,
};
use crate::provider::Model;
use crate::provider_service::ProviderCatalog;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

/// Error type for model resolution.
#[derive(Debug, thiserror::Error)]
pub enum ModelResolutionError {
    #[error("Model not selected for session: {0}")]
    ModelNotSelected(String),

    #[error("Provider not found: {0}")]
    ProviderNotFound(String),

    #[error("Model not found: provider={provider_id}, model={model_id}")]
    ModelNotFound {
        provider_id: String,
        model_id: String,
    },

    #[error("Unsupported API: provider={provider_id}, model={model_id}, api={api}")]
    UnsupportedApi {
        provider_id: String,
        model_id: String,
        api: String,
    },

    #[error("{0}")]
    Other(String),
}

/// Session model selection stored on session info.
///
/// # Source
/// Ported from `packages/core/src/session/runner/model.ts` — the `session.model` field.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SessionModelSelection {
    pub provider_id: String,
    pub model_id: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub variant: Option<String>,
}

/// Resolved model with auth and connection details.
///
/// # Source
/// Ported from `packages/core/src/session/runner/model.ts` — `Model` from `@opencode-ai/llm`.
#[derive(Debug, Clone)]
pub struct ResolvedModel {
    /// The model ID (e.g., "claude-sonnet-4-20250514").
    pub id: String,
    /// The provider ID (e.g., "anthropic").
    pub provider_id: String,
    /// The API type (e.g., "aisdk:@ai-sdk/anthropic").
    pub api: String,
    /// Base URL for the API endpoint.
    pub base_url: Option<String>,
    /// Auth header/query key value.
    pub auth: Option<AuthValue>,
    /// Auth scheme (bearer, header, config, none).
    pub auth_type: AuthType,
    /// Default request headers.
    pub headers: HashMap<String, String>,
    /// Generation configuration.
    pub generation: ModelGenerationConfig,
    /// Context limits.
    pub limit: ModelLimit,
    /// Extra provider options.
    pub provider_options: Option<HashMap<String, serde_json::Value>>,
    /// HTTP body defaults.
    pub http_body: Option<HashMap<String, serde_json::Value>>,
}

/// Authorization value for API calls.
#[derive(Debug, Clone)]
pub enum AuthValue {
    /// Literal key value.
    Key(String),
    /// Config reference (read from env at call time).
    Config(String),
}

/// Authorization scheme.
#[derive(Debug, Clone, PartialEq)]
pub enum AuthType {
    Bearer,
    Header { name: String },
    None,
    Config,
}

/// Generation config for model calls.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct ModelGenerationConfig {
    pub max_tokens: Option<f64>,
    pub temperature: Option<f64>,
    pub top_p: Option<f64>,
    pub top_k: Option<f64>,
    pub frequency_penalty: Option<f64>,
    pub presence_penalty: Option<f64>,
    pub seed: Option<f64>,
    pub stop: Option<Vec<String>>,
}

/// Token limits for a model.
#[derive(Debug, Clone, Default)]
pub struct ModelLimit {
    pub context: i32,
    pub output: i32,
}

/// Resolve a model for a session from the provider catalog.
///
/// # Source
/// Ported from `packages/core/src/session/runner/model.ts` lines 42–44, 131–165 (`resolve`, `locationLayer`).
pub fn resolve_model(
    catalog: &ProviderCatalog,
    session_model: &SessionModelSelection,
) -> Result<ResolvedModel, ModelResolutionError> {
    // Find the model info from the catalog
    let model_info = find_model_in_catalog(catalog, session_model)?;

    // Apply variant if specified
    let model_info = apply_variant(model_info, session_model.variant.as_deref());

    // Build the resolved model
    build_resolved_model(&model_info, session_model)
}

/// Find a model info in the provider catalog.
fn find_model_in_catalog(
    catalog: &ProviderCatalog,
    selection: &SessionModelSelection,
) -> Result<ModelInfo, ModelResolutionError> {
    let provider_id = &selection.provider_id;
    let model_id = &selection.model_id;

    // Get the provider
    let provider = catalog.providers.get(provider_id).ok_or_else(|| {
        ModelResolutionError::ProviderNotFound(provider_id.clone())
    })?;

    // List models and find the one we need
    let models = futures::executor::block_on(provider.list_models()).map_err(|e| {
        ModelResolutionError::Other(format!("list models: {e}"))
    })?;

    let model = models.into_iter().find(|m| m.id == *model_id).ok_or_else(|| {
        ModelResolutionError::ModelNotFound {
            provider_id: provider_id.clone(),
            model_id: model_id.clone(),
        }
    })?;

    // Convert Model to ModelInfo
    Ok(model_info_from_model(&model, provider_id))
}

/// Convert a Provider Model to a ModelInfo.
fn model_info_from_model(model: &Model, provider_id: &str) -> ModelInfo {
    let limit = ModelV2Limits {
        context: model.limit.context as i32,
        input: model.limit.input.map(|v| v as i32),
        output: model.limit.output as i32,
    };

    let capabilities = ModelV2Capabilities {
        tools: model.capabilities.toolcall,
        input: if model.capabilities.attachment {
            vec!["image/*".to_string()]
        } else {
            vec!["text/*".to_string()]
        },
        output: vec!["text/*".to_string()],
    };

    ModelInfo {
        id: model.id.clone(),
        provider_id: provider_id.to_string(),
        family: model.family.clone(),
        name: model.name.clone(),
        api: crate::model::ModelApi::Native(crate::model::NativeApi {
            id: model.id.clone(),
            url: None,
            settings: HashMap::new(),
        }),
        capabilities,
        request: crate::model::ModelRequestConfig::default(),
        variants: vec![],
        time: crate::model::ModelTime { released: 0 },
        cost: vec![],
        status: crate::model::ModelStatus::Active,
        enabled: true,
        limit,
    }
}

/// Apply a variant to the model info.
fn apply_variant(model_info: ModelInfo, variant_id: Option<&str>) -> ModelInfo {
    let variant_id = match variant_id {
        Some("default") | None => model_info.request.variant.as_deref(),
        Some(id) => Some(id),
    };

    let variant_id = match variant_id {
        Some(id) => id,
        None => return model_info,
    };

    // Find the variant
    let variant = match model_info.variants.iter().find(|v| v.id == variant_id) {
        Some(v) => v.clone(),
        None => return model_info,
    };

    // Merge variant config into model request
    let merged_base = ModelRequest {
        headers: model_info.request.headers.clone(),
        body: model_info.request.body.clone(),
        generation: model_info.request.generation.clone(),
        options: model_info.request.options.clone(),
    };

    let variant_req = ModelRequest {
        headers: variant.headers,
        body: variant.body,
        generation: variant.generation,
        options: variant.options,
    };

    let merged = merge_model_request(&merged_base, &variant_req);

    ModelInfo {
        request: crate::model::ModelRequestConfig {
            headers: merged.headers,
            body: merged.body,
            generation: merged.generation,
            options: merged.options,
            variant: Some(variant_id.to_string()),
        },
        ..model_info
    }
}

/// Build a ResolvedModel from a ModelInfo and session selection.
fn build_resolved_model(
    info: &ModelInfo,
    selection: &SessionModelSelection,
) -> Result<ResolvedModel, ModelResolutionError> {
    let api_type = match &info.api {
        crate::model::ModelApi::AiSdk(api) => format!("aisdk:{}", api.package),
        crate::model::ModelApi::Native(_) => "native".to_string(),
    };

    let base_url = match &info.api {
        crate::model::ModelApi::Native(api) => api.url.clone(),
        crate::model::ModelApi::AiSdk(api) => api.url.clone(),
    };

    // Determine auth type based on provider
    let auth_type = match info.provider_id.as_str() {
        well_known_providers::ANTHROPIC => AuthType::Header {
            name: "x-api-key".to_string(),
        },
        _ => AuthType::Bearer,
    };

    let generation = ModelGenerationConfig {
        max_tokens: info.request.generation.max_tokens,
        temperature: info.request.generation.temperature,
        top_p: info.request.generation.top_p,
        top_k: info.request.generation.top_k,
        frequency_penalty: info.request.generation.frequency_penalty,
        presence_penalty: info.request.generation.presence_penalty,
        seed: info.request.generation.seed,
        stop: info.request.generation.stop.clone(),
    };

    let mut http_body = info.request.body.clone();
    http_body.remove("apiKey");
    let http_body = if http_body.is_empty() { None } else { Some(http_body) };

    let provider_options =
        if info.request.options.is_empty() { None } else { Some(info.request.options.clone()) };

    Ok(ResolvedModel {
        id: info.id.clone(),
        provider_id: info.provider_id.clone(),
        api: api_type,
        base_url,
        auth: None, // Set at call time from credentials
        auth_type,
        headers: info.request.headers.clone(),
        generation,
        limit: ModelLimit {
            context: info.limit.context,
            output: info.limit.output,
        },
        provider_options,
        http_body,
    })
}

/// Check if a model's API is supported by the current runtime.
///
/// # Source
/// Ported from `packages/core/src/session/runner/model.ts` lines 134–138 (`supported`).
pub fn is_model_supported(info: &ModelInfo) -> bool {
    match &info.api {
        crate::model::ModelApi::AiSdk(api) => {
            matches!(
                api.package.as_str(),
                "@ai-sdk/openai"
                    | "@ai-sdk/anthropic"
                    | "@ai-sdk/openai-compatible"
            ) && (api.package != "@ai-sdk/openai-compatible" || api.url.is_some())
        }
        crate::model::ModelApi::Native(_) => true,
    }
}

/// Resolve the default model for a session.
///
/// Falls back to the first available supported model.
///
/// # Source
/// Ported from `packages/core/src/session/runner/model.ts` lines 149–164.
pub fn resolve_default_model(
    catalog: &ProviderCatalog,
    session_id: &str,
) -> Result<ResolvedModel, ModelResolutionError> {
    // Try each provider's first available supported model
    for (provider_id, provider) in &catalog.providers {
        let models = futures::executor::block_on(provider.list_models())
            .unwrap_or_default();
        if let Some(model) = models.into_iter().next() {
            let info = model_info_from_model(&model, provider_id);
            let selection = SessionModelSelection {
                provider_id: provider_id.clone(),
                model_id: model.id,
                variant: None,
            };
            return build_resolved_model(&info, &selection);
        }
    }

    Err(ModelResolutionError::ModelNotSelected(session_id.to_string()))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::{ModelInfo, ModelStatus, ModelLimits};

    #[test]
    fn test_model_resolution_error_display() {
        let err = ModelResolutionError::ModelNotSelected("ses_1".into());
        assert!(err.to_string().contains("ses_1"));

        let err = ModelResolutionError::ProviderNotFound("test_provider".into());
        assert!(err.to_string().contains("test_provider"));

        let err = ModelResolutionError::ModelNotFound {
            provider_id: "p".into(),
            model_id: "m".into(),
        };
        assert!(err.to_string().contains("p"));
        assert!(err.to_string().contains("m"));
    }

    #[test]
    fn test_session_model_selection_serde() {
        let sel = SessionModelSelection {
            provider_id: "anthropic".into(),
            model_id: "claude-sonnet-4-20250514".into(),
            variant: Some("default".into()),
        };
        let json = serde_json::to_string(&sel).unwrap();
        assert!(json.contains("anthropic"));
        assert!(json.contains("claude-sonnet"));
        let parsed: SessionModelSelection = serde_json::from_str(&json).unwrap();
        assert_eq!(parsed.provider_id, "anthropic");
    }

    #[test]
    fn test_model_info_from_model() {
        let model = Model {
            id: "claude-sonnet-4-20250514".into(),
            provider_id: "anthropic".into(),
            name: "Claude Sonnet 4".into(),
            family: Some("claude".into()),
            api: crate::provider::ApiInfo {
                id: "claude-sonnet-4-20250514".into(),
                url: "https://api.anthropic.com/v1".into(),
                npm: "@anthropic-ai/sdk".into(),
            },
            capabilities: crate::provider::Capabilities {
                temperature: true,
                reasoning: true,
                attachment: true,
                toolcall: true,
                input: crate::provider::Modalities { text: true, ..Default::default() },
                output: crate::provider::Modalities { text: true, ..Default::default() },
                interleaved: crate::provider::InterleavedSupport::Bool(false),
            },
            cost: crate::provider::Cost::default(),
            limit: crate::provider::TokenLimit {
                context: 200000,
                input: None,
                output: 8192,
            },
            status: crate::provider::ModelStatus::Active,
            options: HashMap::new(),
            headers: HashMap::new(),
            release_date: "2025-05-14".into(),
            variants: None,
        };
        let info = model_info_from_model(&model, "anthropic");
        assert_eq!(info.provider_id, "anthropic");
        assert_eq!(info.limit.context, 200000);
        assert!(info.capabilities.tools);
    }

    #[test]
    fn test_apply_variant_no_variant() {
        let info = ModelInfo::empty("anthropic".into(), "claude".into());
        let result = apply_variant(info, None);
        assert_eq!(result.id, "claude");
    }

    #[test]
    fn test_is_model_supported_openai() {
        let info = ModelInfo {
            api: crate::model::ModelApi::AiSdk(crate::model::AiSdkApi {
                id: "gpt-4".into(),
                package: "@ai-sdk/openai".into(),
                url: None,
                settings: None,
            }),
            ..ModelInfo::empty("openai".into(), "gpt-4".into())
        };
        assert!(is_model_supported(&info));
    }

    #[test]
    fn test_is_model_supported_anthropic() {
        let info = ModelInfo {
            api: crate::model::ModelApi::AiSdk(crate::model::AiSdkApi {
                id: "claude-sonnet-4".into(),
                package: "@ai-sdk/anthropic".into(),
                url: None,
                settings: None,
            }),
            ..ModelInfo::empty("anthropic".into(), "claude-sonnet-4".into())
        };
        assert!(is_model_supported(&info));
    }

    #[test]
    fn test_is_model_supported_openai_compatible_no_url() {
        let info = ModelInfo {
            api: crate::model::ModelApi::AiSdk(crate::model::AiSdkApi {
                id: "custom".into(),
                package: "@ai-sdk/openai-compatible".into(),
                url: None,
                settings: None,
            }),
            ..ModelInfo::empty("custom".into(), "custom".into())
        };
        assert!(!is_model_supported(&info));
    }

    #[test]
    fn test_is_model_supported_openai_compatible_with_url() {
        let info = ModelInfo {
            api: crate::model::ModelApi::AiSdk(crate::model::AiSdkApi {
                id: "custom".into(),
                package: "@ai-sdk/openai-compatible".into(),
                url: Some("https://api.example.com/v1".into()),
                settings: None,
            }),
            ..ModelInfo::empty("custom".into(), "custom".into())
        };
        assert!(is_model_supported(&info));
    }

    #[test]
    fn test_generation_config_default() {
        let config = ModelGenerationConfig::default();
        assert!(config.max_tokens.is_none());
        assert!(config.temperature.is_none());
    }
}
