//! Provider initialization pipeline.
//!
//! Orchestrates provider startup: reads config, applies plugin hooks,
//! resolves auth, and produces the final provider catalog.
//!
//! # Source
//! Ported from `packages/opencode/src/provider/provider.ts` lines 1300–1550.

use std::collections::HashMap;
use std::sync::Arc;

use crate::config::{Config, ModelConfig, ProviderConfig, ProviderOptions};
use crate::error::Error;
use crate::plugin::{
    AuthLoadContext, CatalogTransformContext, CustomProviderConfig, ModelDiscoverContext,
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
pub async fn init_providers(config: &Config) -> Result<ProviderCatalog, Error> {
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

    let info = config.get();
    for (provider_id, provider_cfg) in &info.provider {
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
        if let Some(cfg) = info.provider.get(provider_id) {
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
        let base_url = info
            .provider
            .get(provider_id)
            .and_then(|p| p.options.as_ref())
            .and_then(|o| o.base_url.clone())
            .unwrap_or_default();

        let api_key = info
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
        let env_vars: Vec<String> = info
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
    let disabled = info.disabled_providers.clone();
    let enabled = if info.enabled_providers.is_empty() {
        None
    } else {
        Some(info.enabled_providers.clone())
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
            // Try to find base_url in extra options
            cfg.options
                .as_ref()
                .and_then(|o| o.extra.get("baseUrl"))
                .and_then(|v| v.as_str())
                .map(|s| s.to_string())
        });

    let env_var = cfg.env.first().cloned().unwrap_or_default();

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

    // Create a CompatConfig for this provider
    let config = crate::providers::openai_compatible::CompatConfig {
        provider_id: Box::leak(provider_id.to_string().into_boxed_str()),
        name: Box::leak(
            cfg.name
                .clone()
                .unwrap_or_else(|| provider_id.to_string())
                .into_boxed_str(),
        ),
        npm: Box::leak(
            cfg.npm
                .clone()
                .unwrap_or_else(|| format!("@ai-sdk/{provider_id}"))
                .into_boxed_str(),
        ),
        base_url: Box::leak(base_url.into_boxed_str()),
        env_var: Box::leak(env_var.into_boxed_str()),
        models: Box::leak(models.into_boxed_slice()),
        extra_headers: &[],
        classify_error: crate::providers::openai_compatible::default_classify_error,
    };

    let provider =
        crate::providers::openai_compatible::OpenAICompatibleProvider::from_config(&config)?;

    Ok(Some(Box::new(provider)))
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
pub fn find_model(catalog: &ProviderCatalog, model_id: &str) -> Option<(String, Model)> {
    for (provider_id, provider) in &catalog.providers {
        if let Ok(models) = futures::executor::block_on(provider.list_models()) {
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
