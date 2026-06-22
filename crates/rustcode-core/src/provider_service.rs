//! Provider initialization pipeline.
//!
//! Orchestrates provider startup: reads config, applies plugin hooks,
//! resolves auth, and produces the final provider catalog.
//!
//! # Source
//! Ported from `packages/opencode/src/provider/provider.ts` lines 1300–1550.

use std::collections::HashMap;

use crate::config::{ModelConfig, ProviderConfig};
use crate::error::Error;
use crate::plugin::{
    AuthLoadContext, CatalogTransformContext, ModelDiscoverContext,
    ProviderPlugin, ProviderPluginRegistry,
};
use crate::provider::{Model, Provider};

// ── Provider service ───────────────────────────────────────────────────

/// Result of provider initialization — the final provider catalog.
pub struct ProviderCatalog {
    /// All initialized providers, keyed by provider ID.
    pub providers: HashMap<String, Box<dyn Provider>>,
    /// Custom model overrides from config, keyed by "provider_id/model_id".
    pub model_overrides: HashMap<String, ModelConfig>,
    /// Disabled provider IDs.
    pub disabled: Vec<String>,
    /// If set, ONLY these providers are enabled.
    pub enabled: Option<Vec<String>>,
}

/// Initialize all providers from config and plugins.
///
/// This is the main entry point for provider startup. It runs the full pipeline:
///
/// 1. Auto-detect providers from environment variables
/// 2. Merge config-defined providers into the catalog
/// 3. Apply plugin `transform_catalog` hooks
/// 4. Apply plugin `discover_models` hooks
/// 5. Apply plugin `load_auth` hooks
/// 6. Apply enabled/disabled filters
///
/// # Source
/// Ported from `packages/opencode/src/provider/provider.ts` `Service.init()`.
pub async fn init_providers(config: &crate::config::Info) -> Result<ProviderCatalog, Error> {
    let registry = ProviderPluginRegistry::new();

    // Phase 1: Auto-detect providers from environment
    let mut catalog: HashMap<String, Box<dyn Provider>> = HashMap::new();
    for provider in crate::providers::auto_detect_all() {
        let id = provider.provider_id().to_string();
        tracing::info!("Auto-detected provider: {}", id);
        catalog.insert(id, provider);
    }

    // Phase 2: Merge config-defined providers
    let mut model_overrides: HashMap<String, ModelConfig> = HashMap::new();

    for (provider_id, provider_cfg) in &config.provider {
        // Collect model overrides
        for (model_id, model_cfg) in &provider_cfg.models {
            let key = format!("{provider_id}/{model_id}");
            model_overrides.insert(key, model_cfg.clone());
        }

        // If the provider isn't already detected, try to create it from config
        if !catalog.contains_key(provider_id.as_str()) {
            if let Some(provider) = create_provider_from_config(provider_id, provider_cfg)? {
                tracing::info!("Created provider from config: {}", provider_id);
                catalog.insert(provider_id.clone(), provider);
            }
        }
    }

    // Phase 3: Apply plugin transform_catalog hooks
    // (Currently no built-in plugins — this is the extension point)
    let provider_ids: Vec<String> = catalog.keys().cloned().collect();
    for provider_id in &provider_ids {
        let mut headers = HashMap::new();
        let mut enabled = true;
        let mut options = HashMap::new();

        // Merge config headers into the context
        if let Some(cfg) = config.provider.get(provider_id) {
            // Provider-level options
            if let Some(ref opts) = cfg.options {
                if let Some(ref key) = opts.api_key {
                    options.insert("apiKey".to_string(), serde_json::Value::String(key.clone()));
                }
                if let Some(ref url) = opts.base_url {
                    options.insert(
                        "baseUrl".to_string(),
                        serde_json::Value::String(url.clone()),
                    );
                }
                // Pass through extra options
                for (k, v) in &opts.extra {
                    options.insert(k.clone(), v.clone());
                }
            }
        }

        let mut ctx = CatalogTransformContext {
            provider_id,
            headers: &mut headers,
            enabled: &mut enabled,
            options: &mut options,
        };

        registry.transform_catalog(&mut ctx).await;

        // If a plugin disabled the provider, remove it
        if !enabled {
            catalog.remove(provider_id);
        }
    }

    // Phase 4: Apply plugin discover_models hooks
    for provider_id in catalog.keys() {
        let base_url = config
            .provider
            .get(provider_id)
            .and_then(|p| p.options.as_ref())
            .and_then(|o| o.base_url.clone())
            .unwrap_or_default();

        let api_key = config
            .provider
            .get(provider_id)
            .and_then(|p| p.options.as_ref())
            .and_then(|o| o.api_key.clone());

        let ctx = ModelDiscoverContext {
            provider_id,
            base_url: &base_url,
            api_key: api_key.as_deref(),
            options: &HashMap::new(),
        };

        if let Some(_custom_models) = registry.discover_models(&ctx).await {
            // TODO: Replace provider's model list with custom_models
            // This requires the Provider trait to support model list mutation,
            // or wrapping the provider in a decorator.
            tracing::info!(
                "Plugin provided {} custom models for {}",
                _custom_models.len(),
                provider_id
            );
        }
    }

    // Phase 5: Apply plugin load_auth hooks
    for provider_id in catalog.keys() {
        let env_vars: Vec<String> = config
            .provider
            .get(provider_id)
            .map(|p| p.env.clone())
            .unwrap_or_default();

        let ctx = AuthLoadContext {
            provider_id,
            env_vars: &env_vars,
        };

        if let Some(_auth_options) = registry.load_auth(&ctx).await {
            // TODO: Merge auth options into provider
            tracing::info!("Plugin provided auth for {}", provider_id);
        }
    }

    // Phase 6: Apply enabled/disabled filters
    let disabled = config.disabled_providers.clone();
    let enabled = if config.enabled_providers.is_empty() {
        None
    } else {
        Some(config.enabled_providers.clone())
    };

    for id in &disabled {
        catalog.remove(id);
        tracing::info!("Disabled provider: {}", id);
    }

    if let Some(ref enabled_list) = enabled {
        let to_remove: Vec<String> = catalog
            .keys()
            .filter(|id| !enabled_list.contains(id))
            .cloned()
            .collect();
        for id in to_remove {
            catalog.remove(&id);
            tracing::info!("Filtered out provider (not in enabled list): {}", id);
        }
    }

    Ok(ProviderCatalog {
        providers: catalog,
        model_overrides,
        disabled,
        enabled,
    })
}

/// Create a provider instance from a config definition.
///
/// If the config has a base_url and models, creates an
/// `OpenAICompatibleProvider`. Otherwise returns `None`.
fn create_provider_from_config(
    provider_id: &str,
    cfg: &ProviderConfig,
) -> Result<Option<Box<dyn Provider>>, Error> {
    // Need at minimum a base_url and an API key env var
    let base_url = cfg
        .options
        .as_ref()
        .and_then(|o| o.base_url.clone())
        .or_else(|| {
            // Try to find base_url in extra options (OpenCode uses baseURL)
            cfg.options
                .as_ref()
                .and_then(|o| {
                    o.extra.get("baseUrl")
                        .or_else(|| o.extra.get("baseURL"))
                        .or_else(|| o.extra.get("api_base"))
                })
                .and_then(|v| v.as_str())
                .map(|s| s.to_string())
        });

    let env_var = cfg.env.first().cloned().unwrap_or_else(|| {
        // Try to extract env var from options.apiKey if it uses {env:VAR} syntax
        if let Some(ref options) = cfg.options {
            let api_key_val = options.api_key.as_deref().or_else(|| {
                options.extra.get("apiKey").and_then(|v| v.as_str())
            });
            if let Some(val) = api_key_val {
                if let Some(var_name) = val.strip_prefix("{env:") {
                    if let Some(end) = var_name.find('}') {
                        return var_name[..end].to_string();
                    }
                }
            }
        }
        String::new()
    });

    if base_url.is_none() || env_var.is_empty() {
        return Ok(None);
    }

    let base_url = base_url.unwrap();

    // Check if the API key is available
    let api_key = std::env::var(&env_var).ok().filter(|k| !k.is_empty());

    let api_key = match api_key {
        Some(key) => key,
        None => {
            tracing::debug!(
                "Skipping config provider {} — {} not set",
                provider_id,
                env_var
            );
            return Ok(None);
        }
    };

    // Build model list from config
    let models: Vec<crate::providers::openai_compatible::ModelSpec> = cfg
        .models
        .iter()
        .map(|(id, m)| crate::providers::openai_compatible::ModelSpec {
            id: Box::leak(id.clone().into_boxed_str()),
            name: Box::leak(
                m.name
                    .clone()
                    .unwrap_or_else(|| id.clone())
                    .into_boxed_str(),
            ),
            ctx: m
                .limit
                .as_ref()
                .map(|l| l.context as u64)
                .unwrap_or(128_000),
            out: m.limit.as_ref().map(|l| l.output as u64).unwrap_or(16_384),
            family: m
                .family
                .as_ref()
                .map(|f| -> &'static str { Box::leak(f.clone().into_boxed_str()) }),
            reasoning: m.reasoning.unwrap_or(false),
            image_input: false,
        })
        .collect();

    if models.is_empty() {
        return Ok(None);
    }

    // Check npm to determine the provider protocol type
    let npm = cfg.npm.as_deref().unwrap_or("");

    // Create provider based on protocol type
    if npm.contains("anthropic") {
        // Use Anthropic Messages API protocol
        use crate::providers::anthropic;
            // Strip trailing /v1 if present to avoid double path when
            // the Anthropic provider appends /v1/messages to the base URL
            let anthro_base = base_url.trim_end_matches("/v1").to_string();
            let mut p = match anthropic::AnthropicProvider::with_api_key(
                api_key.clone(),
                anthro_base,
            ) {
                Ok(p) => p,
                Err(e) => {
                    tracing::warn!("Failed to create Anthropic provider from config: {e}");
                    return Ok(None);
                }
            };
            // Add models from the config (e.g. deepseek-v4-flash, glm-5.2)
            for model_spec in &models {
                use crate::provider::{ApiInfo, Capabilities, Model, TokenLimit, ModelStatus};
                let model = Model {
                    id: model_spec.id.to_string(),
                    provider_id: provider_id.to_string(),
                    name: model_spec.name.to_string(),
                    api: ApiInfo {
                        id: model_spec.id.to_string(),
                        url: base_url.clone(),
                        npm: npm.to_string(),
                    },
                    family: model_spec.family.map(|f| f.to_string()),
                    capabilities: Capabilities { temperature: true, toolcall: true, ..Default::default() },
                    cost: crate::provider::Cost::default(),
                    limit: TokenLimit { context: model_spec.ctx, output: model_spec.out, input: None },
                    status: ModelStatus::Active,
                    options: std::collections::HashMap::new(),
                    headers: std::collections::HashMap::new(),
                    release_date: String::new(),
                    variants: None,
                };
                p.models.push(model);
            }
            Ok(Some(Box::new(p)))
    } else {
        // Default: OpenAI Chat Completions protocol
        let compat_config = crate::providers::openai_compatible::CompatConfig {
            provider_id: Box::leak(provider_id.to_string().into_boxed_str()),
            name: Box::leak(
                cfg.name
                    .clone()
                    .unwrap_or_else(|| provider_id.to_string())
                    .into_boxed_str(),
            ),
            npm: Box::leak(npm.to_string().into_boxed_str()),
            base_url: Box::leak(base_url.into_boxed_str()),
            env_var: Box::leak(env_var.into_boxed_str()),
            models: Box::leak(models.into_boxed_slice()),
            extra_headers: &[],
            classify_error: crate::providers::openai_compatible::default_classify_error,
        };

        let provider = crate::providers::openai_compatible::OpenAICompatibleProvider::from_config(&compat_config)?;
        Ok(Some(Box::new(provider)))
    }
}

/// Get a model override from the catalog.
///
/// Checks if the user has configured a custom model config for the given
/// provider/model combination.
pub fn get_model_override<'a>(
    catalog: &'a ProviderCatalog,
    provider_id: &str,
    model_id: &str,
) -> Option<&'a ModelConfig> {
    let key = format!("{provider_id}/{model_id}");
    catalog.model_overrides.get(&key)
}

/// Find a model across all providers.
///
/// Searches the catalog for a model by ID, returning the provider ID and model.
pub async fn find_model(catalog: &ProviderCatalog, model_id: &str) -> Option<(String, Model)> {
    for (provider_id, provider) in &catalog.providers {
        if let Ok(models) = provider.list_models().await {
            for model in models {
                if model.id == model_id {
                    return Some((provider_id.clone(), model));
                }
            }
        }
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::ProviderOptions;

    #[test]
    fn test_provider_catalog_default() {
        // Test that the ProviderCatalog struct can be created
        let catalog = ProviderCatalog {
            providers: HashMap::new(),
            model_overrides: HashMap::new(),
            disabled: Vec::new(),
            enabled: None,
        };
        assert!(catalog.providers.is_empty());
        assert!(catalog.disabled.is_empty());
    }

    #[test]
    fn test_create_provider_from_config_no_base_url() {
        let cfg = ProviderConfig {
            name: Some("Test".to_string()),
            env: vec!["TEST_KEY".to_string()],
            ..Default::default()
        };
        let result = create_provider_from_config("test", &cfg);
        assert!(result.is_ok());
        assert!(result.unwrap().is_none());
    }

    #[test]
    fn test_create_provider_from_config_no_env() {
        let cfg = ProviderConfig {
            name: Some("Test".to_string()),
            options: Some(ProviderOptions {
                base_url: Some("https://api.test.com/v1".to_string()),
                ..Default::default()
            }),
            ..Default::default()
        };
        let result = create_provider_from_config("test", &cfg).unwrap();
        assert!(result.is_none());
    }
}
