//! Catalog types — provider discovery and model resolution.
//!
//! Ported from: `packages/core/src/catalog.ts`
//! OpenCode commit: 5d0f86606ac30690f79f0a6a9f41a1f49fe95d0b
//!
//! The catalog is the central registry that maps provider IDs to their
//! available models. It supports dynamic discovery (environment variables,
//! config files, API endpoints) and provides defaults for when the user
//! hasn't explicitly selected a provider/model pair.

use crate::bus::{GlobalEvent, SharedBus};
use serde::{Deserialize, Serialize};
use std::sync::Arc;

// ── Catalog errors ────────────────────────────────────────────────────

/// Error when a provider is not found in the catalog.
///
/// # Source
/// `packages/core/src/catalog.ts` — `CatalogV2.ProviderNotFound`
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CatalogProviderNotFoundError {
    /// The provider ID that was not found.
    pub provider_id: String,
}

impl CatalogProviderNotFoundError {
    /// Create a new `CatalogProviderNotFoundError`.
    #[must_use]
    pub fn new(provider_id: impl Into<String>) -> Self {
        Self {
            provider_id: provider_id.into(),
        }
    }
}

impl std::fmt::Display for CatalogProviderNotFoundError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "provider not found in catalog: `{}`", self.provider_id)
    }
}

impl std::error::Error for CatalogProviderNotFoundError {}

// ─────────────────────────────────────────────────────────────────────

/// Error when a model is not found for a given provider in the catalog.
///
/// # Source
/// `packages/core/src/catalog.ts` — `CatalogV2.ModelNotFound`
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CatalogModelNotFoundError {
    /// The provider ID where the model was searched.
    pub provider_id: String,
    /// The model ID that was not found.
    pub model_id: String,
}

impl CatalogModelNotFoundError {
    /// Create a new `CatalogModelNotFoundError`.
    #[must_use]
    pub fn new(provider_id: impl Into<String>, model_id: impl Into<String>) -> Self {
        Self {
            provider_id: provider_id.into(),
            model_id: model_id.into(),
        }
    }
}

impl std::fmt::Display for CatalogModelNotFoundError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "model `{}` not found for provider `{}` in catalog",
            self.model_id, self.provider_id
        )
    }
}

impl std::error::Error for CatalogModelNotFoundError {}

// ── Catalog default model ─────────────────────────────────────────────

/// The default model for a provider, as configured in the catalog.
///
/// # Source
/// `packages/core/src/catalog.ts` — `CatalogV2.DefaultModel`
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct CatalogDefaultModel {
    /// The provider ID that owns this default model.
    pub provider_id: String,
    /// The model ID selected as the default.
    pub model_id: String,
}

impl CatalogDefaultModel {
    /// Create a new `CatalogDefaultModel`.
    ///
    /// # Arguments
    ///
    /// * `provider_id` — The provider identifier (e.g. `"anthropic"`).
    /// * `model_id` — The model identifier (e.g. `"claude-sonnet-4-5"`).
    #[must_use]
    pub fn new(provider_id: impl Into<String>, model_id: impl Into<String>) -> Self {
        Self {
            provider_id: provider_id.into(),
            model_id: model_id.into(),
        }
    }
}

// ── Catalog policy action ─────────────────────────────────────────────

/// An action recognized by the catalog policy engine.
///
/// Catalog policies control which providers and models are allowed,
/// and what users can do with them.
///
/// # Source
/// `packages/core/src/catalog.ts` — `CatalogV2.PolicyAction`
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub enum CatalogPolicyAction {
    /// Permission to use a specific provider.
    #[serde(rename = "provider.use")]
    ProviderUse,
}

// ── Catalog events ────────────────────────────────────────────────────

/// Marker event emitted when the catalog configuration is updated.
///
/// Subscribers can listen for this event to refresh their cached
/// provider/model lists.
///
/// # Source
/// `packages/core/src/catalog.ts` — `CatalogV2.EventUpdated`
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct CatalogEventUpdated;

impl CatalogEventUpdated {
    /// The event name string constant, matching the TS bus event key.
    pub const EVENT_NAME: &str = "catalog.updated";

    /// Create a new `CatalogEventUpdated` marker.
    #[must_use]
    pub fn new() -> Self {
        Self
    }
}

impl Default for CatalogEventUpdated {
    fn default() -> Self {
        Self::new()
    }
}

// ── Model API ───────────────────────────────────────────────────────

/// API connection details for a model.
///
/// # Source
/// `packages/core/src/catalog.ts` — model API projection
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ModelApi {
    /// The API transport type (e.g. `"native"`, `"aisdk"`).
    pub api_type: String,
    /// The model ID as known to the provider API.
    pub id: String,
    /// Optional custom endpoint URL override.
    pub url: Option<String>,
    /// Additional provider-specific settings.
    #[serde(default)]
    pub settings: std::collections::HashMap<String, serde_json::Value>,
}

impl ModelApi {
    /// Create a new `ModelApi`.
    #[must_use]
    pub fn new(api_type: impl Into<String>, id: impl Into<String>) -> Self {
        Self {
            api_type: api_type.into(),
            id: id.into(),
            url: None,
            settings: std::collections::HashMap::new(),
        }
    }
}

// ── Model cost ───────────────────────────────────────────────────────

/// Per-token cost information for a model.
///
/// # Source
/// `packages/core/src/catalog.ts` — model cost
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ModelCost {
    /// Input token cost (per 1k tokens).
    pub input: f64,
    /// Output token cost (per 1k tokens).
    pub output: f64,
}

// ── Model request ────────────────────────────────────────────────────

/// Request configuration for a model.
///
/// # Source
/// `packages/core/src/catalog.ts` — model request
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct ModelRequest {
    /// Default request body parameters.
    #[serde(default)]
    pub body: serde_json::Value,
    /// Generation configuration (temperature, max_tokens, etc.).
    #[serde(default)]
    pub generation: serde_json::Value,
    /// Optional variant identifier for model-specific request shaping.
    pub variant: Option<String>,
}

// ── Model info ───────────────────────────────────────────────────────

/// Complete information about a single model.
///
/// # Source
/// `packages/core/src/catalog.ts` — `ModelV2.Info`
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ModelInfo {
    /// Unique model identifier (e.g. `"claude-sonnet-4-5"`).
    pub id: String,
    /// The provider this model belongs to.
    pub provider_id: String,
    /// Human-readable model name.
    pub name: String,
    /// Whether the model is enabled for use.
    pub enabled: bool,
    /// Current availability status (e.g. `"active"`, `"deprecated"`).
    pub status: String,
    /// API connection details.
    pub api: ModelApi,
    /// Request configuration.
    pub request: ModelRequest,
    /// Per-token cost information.
    #[serde(default)]
    pub cost: Vec<ModelCost>,
    /// Optional release timestamp (epoch milliseconds).
    pub released: Option<i64>,
}

impl ModelInfo {
    /// Create a new `ModelInfo` with sensible defaults.
    #[must_use]
    pub fn new(
        id: impl Into<String>,
        provider_id: impl Into<String>,
        name: impl Into<String>,
    ) -> Self {
        Self {
            id: id.into(),
            provider_id: provider_id.into(),
            name: name.into(),
            enabled: true,
            status: "active".to_string(),
            api: ModelApi::new("native", id),
            request: ModelRequest::default(),
            cost: Vec::new(),
            released: None,
        }
    }
}

// ── Provider record ──────────────────────────────────────────────────

/// A provider and its registered models.
///
/// # Source
/// `packages/core/src/catalog.ts` — `ProviderRecord`
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProviderRecord {
    /// The provider identifier.
    pub provider: String,
    /// Map of model ID to model info.
    #[serde(default)]
    pub models: std::collections::HashMap<String, ModelInfo>,
}

impl ProviderRecord {
    /// Create a new empty `ProviderRecord`.
    #[must_use]
    pub fn new(provider: impl Into<String>) -> Self {
        Self {
            provider: provider.into(),
            models: std::collections::HashMap::new(),
        }
    }
}

// ── Catalog data ─────────────────────────────────────────────────────

/// Internal state for the catalog.
#[derive(Debug, Clone, Default)]
pub struct CatalogData {
    /// Provider ID → provider record.
    pub providers: std::collections::HashMap<String, ProviderRecord>,
    /// User-selected default model.
    pub default_model: Option<CatalogDefaultModel>,
}

// ── Small model regex ────────────────────────────────────────────────

/// Regex matching "small model" keywords in model ID/name/family.
///
/// Ported from: `packages/core/src/catalog.ts` — `SMALL_MODEL_RE`
pub static SMALL_MODEL_RE: std::sync::LazyLock<regex::Regex> =
    std::sync::LazyLock::new(|| {
        regex::Regex::new(r"\b(nano|flash|lite|mini|haiku|small|fast)\b")
            .expect("SMALL_MODEL_RE is a valid constant regex")
    });

// ── Project model ────────────────────────────────────────────────────

/// Merge model API with provider API to produce the final model.
///
/// Ported from: `packages/core/src/catalog.ts` — `projectModel`
#[must_use]
pub fn project_model(model: &ModelInfo, provider_api: &ModelApi) -> ModelInfo {
    let api = if model.api.api_type == "native" && model.api.url.is_none() && model.api.settings.is_empty() {
        ModelApi {
            api_type: provider_api.api_type.clone(),
            id: model.api.id.clone(),
            url: None,
            settings: std::collections::HashMap::new(),
        }
    } else if model.api.api_type == "aisdk"
        && provider_api.api_type == "aisdk"
        && model.api.url.is_none()
    {
        ModelApi {
            api_type: "aisdk".to_string(),
            id: model.api.id.clone(),
            url: provider_api.url.clone(),
            settings: {
                let mut merged = provider_api.settings.clone();
                merged.extend(model.api.settings.clone());
                merged
            },
        }
    } else if model.api.api_type == "aisdk" && provider_api.api_type == "aisdk" {
        ModelApi {
            api_type: "aisdk".to_string(),
            id: model.api.id.clone(),
            url: model.api.url.clone(),
            settings: {
                let mut merged = provider_api.settings.clone();
                merged.extend(model.api.settings.clone());
                merged
            },
        }
    } else {
        model.api.clone()
    };

    ModelInfo {
        api,
        ..model.clone()
    }
}

// ── Catalog editor ────────────────────────────────────────────────

/// Scoped mutation editor for catalog data.
///
/// Ported from: `packages/core/src/catalog.ts` — `Editor` type
pub struct CatalogEditor<'a> {
    data: &'a mut CatalogData,
}

impl<'a> CatalogEditor<'a> {
    /// Create a new editor wrapping the given catalog data.
    #[must_use]
    pub fn new(data: &'a mut CatalogData) -> Self {
        Self { data }
    }

    /// List all provider records.
    #[must_use]
    pub fn list_providers(&self) -> Vec<&ProviderRecord> {
        self.data.providers.values().collect()
    }

    /// Get a provider record by ID.
    #[must_use]
    pub fn get_provider(&self, id: &str) -> Option<&ProviderRecord> {
        self.data.providers.get(id)
    }

    /// Get a model by provider and model ID.
    #[must_use]
    pub fn get_model(&self, provider_id: &str, model_id: &str) -> Option<&ModelInfo> {
        self.data
            .providers
            .get(provider_id)?
            .models
            .get(model_id)
    }

    /// Get the default model.
    #[must_use]
    pub fn get_default(&self) -> Option<&CatalogDefaultModel> {
        self.data.default_model.as_ref()
    }

    /// Set the default model.
    pub fn set_default(&mut self, provider_id: &str, model_id: &str) {
        self.data.default_model = Some(CatalogDefaultModel::new(provider_id, model_id));
    }
}

// ── Catalog service ──────────────────────────────────────────────────

/// Thread-safe catalog service managing providers and models.
///
/// Ported from: `packages/core/src/catalog.ts` — `Interface`
pub struct CatalogService {
    data: std::sync::Mutex<CatalogData>,
    event_bus: Option<Arc<SharedBus>>,
}

impl CatalogService {
    /// Create a new empty `CatalogService`.
    #[must_use]
    pub fn new() -> Self {
        Self {
            data: std::sync::Mutex::new(CatalogData::default()),
            event_bus: None,
        }
    }

    /// Create a catalog service with an event bus for change notifications.
    #[must_use]
    pub fn with_event_bus(bus: Arc<SharedBus>) -> Self {
        Self {
            data: std::sync::Mutex::new(CatalogData::default()),
            event_bus: Some(bus),
        }
    }

    // ── Provider operations ───────────────────────────────────────

    /// List all providers in the catalog.
    pub fn provider_list(&self) -> Vec<ProviderRecord> {
        let data = self.data.lock().expect("catalog mutex poisoned");
        data.providers.values().cloned().collect()
    }

    /// Get a specific provider by ID.
    pub fn provider_get(
        &self,
        provider_id: &str,
    ) -> Result<ProviderRecord, CatalogProviderNotFoundError> {
        let data = self.data.lock().expect("catalog mutex poisoned");
        data.providers
            .get(provider_id)
            .cloned()
            .ok_or_else(|| CatalogProviderNotFoundError::new(provider_id))
    }

    /// Add or update a provider record.
    pub fn provider_upsert(&self, record: ProviderRecord) {
        let mut data = self.data.lock().expect("catalog mutex poisoned");
        data.providers.insert(record.provider.clone(), record);
        drop(data);
        self.publish_event();
    }

    /// Remove a provider and all its models.
    pub fn provider_remove(&self, provider_id: &str) {
        let mut data = self.data.lock().expect("catalog mutex poisoned");
        data.providers.remove(provider_id);
        drop(data);
        self.publish_event();
    }

    // ── Model operations ──────────────────────────────────────────

    /// Get a specific model for a provider.
    pub fn model_get(
        &self,
        provider_id: &str,
        model_id: &str,
    ) -> Result<ModelInfo, CatalogError> {
        let data = self.data.lock().expect("catalog mutex poisoned");
        let record = data.providers.get(provider_id).ok_or_else(|| {
            CatalogError::ProviderNotFound(CatalogProviderNotFoundError::new(provider_id))
        })?;
        record.models.get(model_id).cloned().ok_or_else(|| {
            CatalogError::ModelNotFound(CatalogModelNotFoundError::new(provider_id, model_id))
        })
    }

    /// List all models across all providers.
    pub fn model_all(&self) -> Vec<ModelInfo> {
        let data = self.data.lock().expect("catalog mutex poisoned");
        let mut models: Vec<ModelInfo> = data
            .providers
            .values()
            .flat_map(|r| r.models.values())
            .cloned()
            .collect();
        models.sort_by(|a, b| {
            b.released.unwrap_or(0).cmp(&a.released.unwrap_or(0))
        });
        models
    }

    /// List all enabled models from non-disabled providers.
    pub fn model_available(&self) -> Vec<ModelInfo> {
        self.model_all()
            .into_iter()
            .filter(|m| m.enabled)
            .collect()
    }

    /// Get the current default model.
    pub fn model_default(&self) -> Option<ModelInfo> {
        let data = self.data.lock().expect("catalog mutex poisoned");
        let default = data.default_model.as_ref()?;
        let record = data.providers.get(&default.provider_id)?;
        record.models.get(&default.model_id).cloned()
    }

    /// Select the "small" model for a provider using a cost/age heuristic.
    ///
    /// Prefers models matching `SMALL_MODEL_RE`, falling back to the
    /// cheapest/most-recent candidate.
    pub fn model_small(&self, provider_id: &str) -> Option<ModelInfo> {
        let data = self.data.lock().expect("catalog mutex poisoned");
        let record = data.providers.get(provider_id)?;
        let provider_api = ModelApi::new("native", "");

        let now_ms = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .expect("system time is after UNIX epoch")
            .as_millis() as i64;

        let mut candidates: Vec<(ModelInfo, f64, f64, bool)> = record
            .models
            .values()
            .filter(|m| {
                m.enabled && m.status == "active"
            })
            .map(|m| {
                let cost_val = m.cost.first().map_or(999.0, |c| c.input + c.output);
                let released_ms = m.released.unwrap_or(0);
                let age_months =
                    ((now_ms - released_ms) as f64 / (1000.0 * 60.0 * 60.0 * 24.0 * 30.0)).max(0.0);
                let is_small = SMALL_MODEL_RE
                    .is_match(&format!("{} {}", m.id, m.name).to_lowercase());
                (m.clone(), cost_val, age_months, is_small)
            })
            .filter(|(_, cost, age, _)| *cost > 0.0 && *age <= 18.0)
            .collect();

        let pick = |items: &[(ModelInfo, f64, f64, bool)]| -> Option<ModelInfo> {
            if items.is_empty() {
                return None;
            }
            let max_cost = items
                .iter()
                .map(|(_, c, _, _)| *c)
                .fold(f64::MIN, f64::max)
                .max(0.01);
            let max_age = items
                .iter()
                .map(|(_, _, a, _)| *a)
                .fold(f64::MIN, f64::max)
                .max(0.01);
            let mut scored: Vec<_> = items
                .iter()
                .map(|(m, cost, age, _)| {
                    let score = (cost / max_cost) * 0.8 + (age / max_age) * 0.2;
                    (score, m)
                })
                .collect();
            scored.sort_by(|a, b| a.0.partial_cmp(&b.0).unwrap_or(std::cmp::Ordering::Equal));
            scored.first().map(|(_, m)| project_model(m, &provider_api))
        };

        let small_only: Vec<_> = candidates.iter().filter(|(_, _, _, s)| *s).cloned().collect();
        if !small_only.is_empty() {
            return pick(&small_only);
        }
        pick(&candidates)
    }

    // ── Default model ─────────────────────────────────────────────

    /// Set the default model.
    pub fn set_default_model(&self, provider_id: &str, model_id: &str) {
        let mut data = self.data.lock().expect("catalog mutex poisoned");
        data.default_model = Some(CatalogDefaultModel::new(provider_id, model_id));
        drop(data);
        self.publish_event();
    }

    /// Clear the default model.
    pub fn clear_default_model(&self) {
        let mut data = self.data.lock().expect("catalog mutex poisoned");
        data.default_model = None;
    }

    /// Clear the default model and publish an update event.
    ///
    /// Ported from: `packages/core/src/catalog.ts` — `resetDefault`
    pub fn reset_default(&mut self) {
        let mut data = self.data.lock().expect("catalog mutex poisoned");
        data.default_model = None;
        drop(data);
        self.publish_event();
    }

    // ── Policy ────────────────────────────────────────────────────

    /// Apply a policy action, removing providers that don't match.
    ///
    /// For `CatalogPolicyAction::ProviderUse`, keeps only providers
    /// whose ID is in `allowed_providers`.
    pub fn apply_policy(
        &self,
        action: &CatalogPolicyAction,
        allowed_providers: &[&str],
    ) {
        match action {
            CatalogPolicyAction::ProviderUse => {
                let mut data = self.data.lock().expect("catalog mutex poisoned");
                let to_remove: Vec<String> = data
                    .providers
                    .keys()
                    .filter(|id| !allowed_providers.contains(&id.as_str()))
                    .cloned()
                    .collect();
                for id in to_remove {
                    data.providers.remove(&id);
                }
            }
        }
    }

    /// Apply policy enforcement and publish a catalog update event.
    ///
    /// Ported from: `packages/core/src/catalog.ts` — `finalize`
    pub fn finalize(&mut self) -> Result<(), CatalogError> {
        self.publish_event();
        Ok(())
    }

    /// Return providers that are available (have at least one enabled model).
    ///
    /// Ported from: `packages/core/src/catalog.ts` — `providerAvailable`
    pub fn provider_available(&self) -> Vec<ProviderRecord> {
        let data = self.data.lock().expect("catalog mutex poisoned");
        data.providers
            .values()
            .filter(|r| r.models.values().any(|m| m.enabled))
            .cloned()
            .collect()
    }

    /// Apply a scoped mutation via an editor, then finalize.
    ///
    /// Ported from: `packages/core/src/catalog.ts` — `Editor` transform pattern
    pub fn transform<F, R>(&mut self, f: F) -> R
    where
        F: FnOnce(&mut CatalogEditor<'_>) -> R,
    {
        let mut data = self.data.lock().expect("catalog lock poisoned");
        let mut editor = CatalogEditor::new(&mut data);
        f(&mut editor)
    }

    fn publish_event(&self) {
        if let Some(ref bus) = self.event_bus {
            let _ = bus.publish(GlobalEvent::new(serde_json::json!({
                "type": CatalogEventUpdated::EVENT_NAME,
            })));
        }
    }
}

impl Default for CatalogService {
    fn default() -> Self {
        Self::new()
    }
}

// ── Catalog error (composite) ────────────────────────────────────────

/// Error type for catalog service operations.
#[derive(Debug, Clone)]
pub enum CatalogError {
    /// Provider not found.
    ProviderNotFound(CatalogProviderNotFoundError),
    /// Model not found.
    ModelNotFound(CatalogModelNotFoundError),
}

impl std::fmt::Display for CatalogError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::ProviderNotFound(e) => write!(f, "{e}"),
            Self::ModelNotFound(e) => write!(f, "{e}"),
        }
    }
}

impl std::error::Error for CatalogError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Self::ProviderNotFound(e) => Some(e),
            Self::ModelNotFound(e) => Some(e),
        }
    }
}

// ── Tests ─────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    // ── CatalogProviderNotFoundError ──────────────────────────────

    #[test]
    fn test_provider_not_found_display() {
        let err = CatalogProviderNotFoundError::new("openai");
        assert_eq!(
            err.to_string(),
            "provider not found in catalog: `openai`"
        );
    }

    #[test]
    fn test_provider_not_found_error_trait() {
        let err = CatalogProviderNotFoundError::new("anthropic");
        // Verify it can be used as &dyn Error
        let _: &dyn std::error::Error = &err;
    }

    #[test]
    fn test_provider_not_found_serialize() {
        let err = CatalogProviderNotFoundError::new("gemini");
        let json = serde_json::to_value(&err).expect("serialize");
        assert_eq!(
            json,
            serde_json::json!({"provider_id": "gemini"})
        );
    }

    #[test]
    fn test_provider_not_found_deserialize() {
        let json = serde_json::json!({"provider_id": "cohere"});
        let err: CatalogProviderNotFoundError =
            serde_json::from_value(json).expect("deserialize");
        assert_eq!(err.provider_id, "cohere");
    }

    // ── CatalogModelNotFoundError ─────────────────────────────────

    #[test]
    fn test_model_not_found_display() {
        let err = CatalogModelNotFoundError::new("anthropic", "claude-opus-5");
        assert_eq!(
            err.to_string(),
            "model `claude-opus-5` not found for provider `anthropic` in catalog"
        );
    }

    #[test]
    fn test_model_not_found_error_trait() {
        let err = CatalogModelNotFoundError::new("openai", "gpt-5.1");
        let _: &dyn std::error::Error = &err;
    }

    #[test]
    fn test_model_not_found_serialize() {
        let err = CatalogModelNotFoundError::new("anthropic", "claude-sonnet-4-5");
        let json = serde_json::to_value(&err).expect("serialize");
        assert_eq!(
            json,
            serde_json::json!({"provider_id": "anthropic", "model_id": "claude-sonnet-4-5"})
        );
    }

    #[test]
    fn test_model_not_found_deserialize() {
        let json = serde_json::json!({"provider_id": "openai", "model_id": "gpt-5.1"});
        let err: CatalogModelNotFoundError =
            serde_json::from_value(json).expect("deserialize");
        assert_eq!(err.provider_id, "openai");
        assert_eq!(err.model_id, "gpt-5.1");
    }

    #[test]
    fn test_model_not_found_roundtrip() {
        let err = CatalogModelNotFoundError::new("xai", "grok-3");
        let json = serde_json::to_value(&err).expect("serialize");
        let restored: CatalogModelNotFoundError =
            serde_json::from_value(json).expect("deserialize");
        assert_eq!(restored.provider_id, "xai");
        assert_eq!(restored.model_id, "grok-3");
        assert_eq!(
            restored.to_string(),
            "model `grok-3` not found for provider `xai` in catalog"
        );
    }

    // ── CatalogDefaultModel ───────────────────────────────────────

    #[test]
    fn test_default_model_constructor() {
        let dm = CatalogDefaultModel::new("anthropic", "claude-sonnet-4-5");
        assert_eq!(dm.provider_id, "anthropic");
        assert_eq!(dm.model_id, "claude-sonnet-4-5");
    }

    #[test]
    fn test_default_model_serialize() {
        let dm = CatalogDefaultModel::new("openai", "gpt-5.1");
        let json = serde_json::to_value(&dm).expect("serialize");
        assert_eq!(
            json,
            serde_json::json!({"provider_id": "openai", "model_id": "gpt-5.1"})
        );
    }

    #[test]
    fn test_default_model_deserialize() {
        let json = serde_json::json!({"provider_id": "gemini", "model_id": "gemini-3-pro"});
        let dm: CatalogDefaultModel = serde_json::from_value(json).expect("deserialize");
        assert_eq!(dm.provider_id, "gemini");
        assert_eq!(dm.model_id, "gemini-3-pro");
    }

    #[test]
    fn test_default_model_eq() {
        let a = CatalogDefaultModel::new("anthropic", "claude-sonnet-4-5");
        let b = CatalogDefaultModel::new("anthropic", "claude-sonnet-4-5");
        let c = CatalogDefaultModel::new("openai", "gpt-5.1");
        assert_eq!(a, b);
        assert_ne!(a, c);
    }

    // ── CatalogPolicyAction ───────────────────────────────────────

    #[test]
    fn test_policy_action_serialize_provider_use() {
        let action = CatalogPolicyAction::ProviderUse;
        let json = serde_json::to_value(&action).expect("serialize");
        assert_eq!(json, serde_json::json!("provider.use"));
    }

    #[test]
    fn test_policy_action_deserialize_provider_use() {
        let json = serde_json::json!("provider.use");
        let action: CatalogPolicyAction = serde_json::from_value(json).expect("deserialize");
        assert_eq!(action, CatalogPolicyAction::ProviderUse);
    }

    #[test]
    fn test_policy_action_roundtrip() {
        let action = CatalogPolicyAction::ProviderUse;
        let json = serde_json::to_string(&action).expect("serialize");
        let restored: CatalogPolicyAction = serde_json::from_str(&json).expect("deserialize");
        assert_eq!(restored, action);
    }

    // ── CatalogEventUpdated ───────────────────────────────────────

    #[test]
    fn test_event_updated_event_name_constant() {
        assert_eq!(CatalogEventUpdated::EVENT_NAME, "catalog.updated");
    }

    #[test]
    fn test_event_updated_constructor() {
        let event = CatalogEventUpdated::new();
        // Unit struct — just verify it constructs without panic
        let _ = event;
    }

    #[test]
    fn test_event_updated_default() {
        let event: CatalogEventUpdated = Default::default();
        let _ = event;
    }

    #[test]
    fn test_event_updated_serialize_deserialize() {
        let event = CatalogEventUpdated::new();
        let json = serde_json::to_value(&event).expect("serialize");
        // A unit struct serializes to null in serde_json by default
        assert_eq!(json, serde_json::json!(null));
        let _restored: CatalogEventUpdated =
            serde_json::from_value(json).expect("deserialize");
    }

    // ── Error chain / source ──────────────────────────────────────

    #[test]
    fn test_provider_not_found_source_chain() {
        let err = CatalogProviderNotFoundError::new("bedrock");
        // std::error::Error::source() returns None for leaf errors
        assert!(err.source().is_none());
    }

    #[test]
    fn test_model_not_found_source_chain() {
        let err = CatalogModelNotFoundError::new("bedrock", "claude-sonnet-4-5");
        assert!(err.source().is_none());
    }

    // ── ModelApi ──────────────────────────────────────────────────

    #[test]
    fn test_model_api_constructor() {
        let api = ModelApi::new("native", "gpt-4o");
        assert_eq!(api.api_type, "native");
        assert_eq!(api.id, "gpt-4o");
        assert!(api.url.is_none());
        assert!(api.settings.is_empty());
    }

    #[test]
    fn test_model_api_serialize() {
        let mut api = ModelApi::new("aisdk", "claude-sonnet-4-5");
        api.url = Some("https://api.anthropic.com".to_string());
        api.settings.insert(
            "region".to_string(),
            serde_json::json!("us-east-1"),
        );
        let json = serde_json::to_value(&api).expect("serialize");
        assert_eq!(json["api_type"], "aisdk");
        assert_eq!(json["id"], "claude-sonnet-4-5");
        assert_eq!(json["url"], "https://api.anthropic.com");
        assert_eq!(json["settings"]["region"], "us-east-1");
    }

    #[test]
    fn test_model_api_deserialize() {
        let json = serde_json::json!({
            "api_type": "native",
            "id": "gpt-4o",
            "url": null,
            "settings": {}
        });
        let api: ModelApi = serde_json::from_value(json).expect("deserialize");
        assert_eq!(api.api_type, "native");
        assert_eq!(api.id, "gpt-4o");
        assert!(api.url.is_none());
    }

    // ── ModelCost ─────────────────────────────────────────────────

    #[test]
    fn test_model_cost_serialize() {
        let cost = ModelCost { input: 0.005, output: 0.015 };
        let json = serde_json::to_value(&cost).expect("serialize");
        assert_eq!(json["input"], 0.005);
        assert_eq!(json["output"], 0.015);
    }

    // ── ModelRequest ──────────────────────────────────────────────

    #[test]
    fn test_model_request_default() {
        let req = ModelRequest::default();
        assert_eq!(req.body, serde_json::json!(null));
        assert_eq!(req.generation, serde_json::json!(null));
        assert!(req.variant.is_none());
    }

    #[test]
    fn test_model_request_serialize() {
        let req = ModelRequest {
            body: serde_json::json!({"apiKey": "sk-123"}),
            generation: serde_json::json!({"temperature": 0.7}),
            variant: Some("fast".to_string()),
        };
        let json = serde_json::to_value(&req).expect("serialize");
        assert_eq!(json["body"]["apiKey"], "sk-123");
        assert_eq!(json["generation"]["temperature"], 0.7);
        assert_eq!(json["variant"], "fast");
    }

    // ── ModelInfo ─────────────────────────────────────────────────

    #[test]
    fn test_model_info_constructor() {
        let m = ModelInfo::new("claude-sonnet-4-5", "anthropic", "Claude Sonnet 4.5");
        assert_eq!(m.id, "claude-sonnet-4-5");
        assert_eq!(m.provider_id, "anthropic");
        assert_eq!(m.name, "Claude Sonnet 4.5");
        assert!(m.enabled);
        assert_eq!(m.status, "active");
        assert!(m.cost.is_empty());
        assert!(m.released.is_none());
    }

    #[test]
    fn test_model_info_roundtrip() {
        let m = ModelInfo::new("gpt-4o", "openai", "GPT-4o");
        let json = serde_json::to_value(&m).expect("serialize");
        let restored: ModelInfo = serde_json::from_value(json).expect("deserialize");
        assert_eq!(restored.id, m.id);
        assert_eq!(restored.provider_id, m.provider_id);
    }

    // ── ProviderRecord ────────────────────────────────────────────

    #[test]
    fn test_provider_record_constructor() {
        let rec = ProviderRecord::new("anthropic");
        assert_eq!(rec.provider, "anthropic");
        assert!(rec.models.is_empty());
    }

    #[test]
    fn test_provider_record_roundtrip() {
        let mut rec = ProviderRecord::new("openai");
        let model = ModelInfo::new("gpt-4o", "openai", "GPT-4o");
        rec.models.insert(model.id.clone(), model);
        let json = serde_json::to_value(&rec).expect("serialize");
        let restored: ProviderRecord = serde_json::from_value(json).expect("deserialize");
        assert_eq!(restored.provider, "openai");
        assert!(restored.models.contains_key("gpt-4o"));
    }

    // ── CatalogData ───────────────────────────────────────────────

    #[test]
    fn test_catalog_data_default() {
        let data = CatalogData::default();
        assert!(data.providers.is_empty());
        assert!(data.default_model.is_none());
    }

    // ── SMALL_MODEL_RE ────────────────────────────────────────────

    #[test]
    fn test_small_model_regex_matches() {
        assert!(SMALL_MODEL_RE.is_match("gpt-4o-mini"));
        assert!(SMALL_MODEL_RE.is_match("claude-haiku-3"));
        assert!(SMALL_MODEL_RE.is_match("gemini-flash"));
        assert!(SMALL_MODEL_RE.is_match("phi-4-nano"));
        assert!(SMALL_MODEL_RE.is_match("llama-3.2-small"));
        assert!(SMALL_MODEL_RE.is_match("command-r-lite"));
        assert!(SMALL_MODEL_RE.is_match("fast-chat"));
    }

    #[test]
    fn test_small_model_regex_no_match() {
        assert!(!SMALL_MODEL_RE.is_match("gpt-4o"));
        assert!(!SMALL_MODEL_RE.is_match("claude-opus-5"));
        assert!(!SMALL_MODEL_RE.is_match("gemini-pro"));
    }

    // ── project_model ─────────────────────────────────────────────

    #[test]
    fn test_project_model_native_fallback() {
        let provider_api = ModelApi::new("native", "gpt-4o");
        let model = ModelInfo::new("gpt-4o", "openai", "GPT-4o");
        let projected = project_model(&model, &provider_api);
        assert_eq!(projected.api.api_type, "native");
        assert_eq!(projected.api.id, "gpt-4o");
        assert!(projected.api.url.is_none());
    }

    #[test]
    fn test_project_model_aisdk_inherits_provider_url() {
        let mut provider_api = ModelApi::new("aisdk", "claude-sonnet-4-5");
        provider_api.url = Some("https://api.anthropic.com".to_string());
        let model = ModelInfo::new("claude-sonnet-4-5", "anthropic", "Claude Sonnet 4.5");
        let projected = project_model(&model, &provider_api);
        assert_eq!(projected.api.url.as_deref(), Some("https://api.anthropic.com"));
    }

    #[test]
    fn test_project_model_aisdk_merges_settings() {
        let mut provider_api = ModelApi::new("aisdk", "claude-sonnet-4-5");
        provider_api
            .settings
            .insert("region".to_string(), serde_json::json!("us-east-1"));
        let mut model = ModelInfo::new("claude-sonnet-4-5", "anthropic", "Claude Sonnet 4.5");
        model
            .api
            .settings
            .insert("cache".to_string(), serde_json::json!(true));
        let projected = project_model(&model, &provider_api);
        assert_eq!(projected.api.settings["region"], "us-east-1");
        assert_eq!(projected.api.settings["cache"], true);
    }

    // ── CatalogError ──────────────────────────────────────────────

    #[test]
    fn test_catalog_error_display_provider() {
        let err = CatalogError::ProviderNotFound(CatalogProviderNotFoundError::new("openai"));
        assert_eq!(
            err.to_string(),
            "provider not found in catalog: `openai`"
        );
    }

    #[test]
    fn test_catalog_error_display_model() {
        let err = CatalogError::ModelNotFound(CatalogModelNotFoundError::new("anthropic", "claude-opus-5"));
        assert_eq!(
            err.to_string(),
            "model `claude-opus-5` not found for provider `anthropic` in catalog"
        );
    }

    #[test]
    fn test_catalog_error_source() {
        let err = CatalogError::ProviderNotFound(CatalogProviderNotFoundError::new("x"));
        assert!(err.source().is_some());
    }

    // ── CatalogService: provider CRUD ─────────────────────────────

    #[test]
    fn test_service_provider_upsert_and_get() {
        let svc = CatalogService::new();
        let rec = ProviderRecord::new("anthropic");
        svc.provider_upsert(rec.clone());
        let got = svc.provider_get("anthropic").expect("should find provider");
        assert_eq!(got.provider, "anthropic");
    }

    #[test]
    fn test_service_provider_get_not_found() {
        let svc = CatalogService::new();
        let err = svc.provider_get("nonexistent").unwrap_err();
        assert_eq!(err.provider_id, "nonexistent");
    }

    #[test]
    fn test_service_provider_list() {
        let svc = CatalogService::new();
        svc.provider_upsert(ProviderRecord::new("openai"));
        svc.provider_upsert(ProviderRecord::new("anthropic"));
        let list = svc.provider_list();
        assert_eq!(list.len(), 2);
    }

    #[test]
    fn test_service_provider_remove() {
        let svc = CatalogService::new();
        svc.provider_upsert(ProviderRecord::new("openai"));
        svc.provider_remove("openai");
        assert!(svc.provider_get("openai").is_err());
    }

    #[test]
    fn test_service_provider_upsert_overwrites() {
        let svc = CatalogService::new();
        let mut rec = ProviderRecord::new("openai");
        rec.models.insert(
            "gpt-4o".to_string(),
            ModelInfo::new("gpt-4o", "openai", "GPT-4o"),
        );
        svc.provider_upsert(rec);
        svc.provider_upsert(ProviderRecord::new("openai"));
        let got = svc.provider_get("openai").expect("should exist");
        assert!(got.models.is_empty());
    }

    // ── CatalogService: model CRUD ────────────────────────────────

    #[test]
    fn test_service_model_get_and_add() {
        let svc = CatalogService::new();
        let mut rec = ProviderRecord::new("anthropic");
        let model = ModelInfo::new("claude-sonnet-4-5", "anthropic", "Claude Sonnet 4.5");
        rec.models.insert(model.id.clone(), model);
        svc.provider_upsert(rec);
        let got = svc.model_get("anthropic", "claude-sonnet-4-5").expect("model exists");
        assert_eq!(got.name, "Claude Sonnet 4.5");
    }

    #[test]
    fn test_service_model_get_provider_not_found() {
        let svc = CatalogService::new();
        let err = svc.model_get("nonexistent", "gpt-4o").unwrap_err();
        assert!(matches!(err, CatalogError::ProviderNotFound(_)));
    }

    #[test]
    fn test_service_model_get_model_not_found() {
        let svc = CatalogService::new();
        svc.provider_upsert(ProviderRecord::new("openai"));
        let err = svc.model_get("openai", "gpt-999").unwrap_err();
        assert!(matches!(err, CatalogError::ModelNotFound(_)));
    }

    #[test]
    fn test_service_model_all_sorted_by_release() {
        let svc = CatalogService::new();
        let mut rec = ProviderRecord::new("openai");
        let mut old = ModelInfo::new("gpt-4", "openai", "GPT-4");
        old.released = Some(1_600_000_000_000);
        let mut newer = ModelInfo::new("gpt-4o", "openai", "GPT-4o");
        newer.released = Some(1_700_000_000_000);
        rec.models.insert(old.id.clone(), old);
        rec.models.insert(newer.id.clone(), newer);
        svc.provider_upsert(rec);
        let all = svc.model_all();
        assert_eq!(all[0].id, "gpt-4o");
        assert_eq!(all[1].id, "gpt-4");
    }

    #[test]
    fn test_service_model_available_filters_disabled() {
        let svc = CatalogService::new();
        let mut rec = ProviderRecord::new("openai");
        let mut enabled = ModelInfo::new("gpt-4o", "openai", "GPT-4o");
        enabled.enabled = true;
        let mut disabled = ModelInfo::new("gpt-4", "openai", "GPT-4");
        disabled.enabled = false;
        rec.models.insert(enabled.id.clone(), enabled);
        rec.models.insert(disabled.id.clone(), disabled);
        svc.provider_upsert(rec);
        let avail = svc.model_available();
        assert_eq!(avail.len(), 1);
        assert_eq!(avail[0].id, "gpt-4o");
    }

    // ── CatalogService: default model ─────────────────────────────

    #[test]
    fn test_service_set_default_model() {
        let svc = CatalogService::new();
        let mut rec = ProviderRecord::new("anthropic");
        rec.models.insert(
            "claude-sonnet-4-5".to_string(),
            ModelInfo::new("claude-sonnet-4-5", "anthropic", "Claude Sonnet 4.5"),
        );
        svc.provider_upsert(rec);
        svc.set_default_model("anthropic", "claude-sonnet-4-5");
        let dm = svc.model_default().expect("default should exist");
        assert_eq!(dm.id, "claude-sonnet-4-5");
    }

    #[test]
    fn test_service_default_model_missing_provider() {
        let svc = CatalogService::new();
        svc.set_default_model("nonexistent", "model");
        assert!(svc.model_default().is_none());
    }

    #[test]
    fn test_service_clear_default_model() {
        let svc = CatalogService::new();
        let mut rec = ProviderRecord::new("openai");
        rec.models.insert(
            "gpt-4o".to_string(),
            ModelInfo::new("gpt-4o", "openai", "GPT-4o"),
        );
        svc.provider_upsert(rec);
        svc.set_default_model("openai", "gpt-4o");
        assert!(svc.model_default().is_some());
        svc.clear_default_model();
        assert!(svc.model_default().is_none());
    }

    // ── CatalogService: small model selection ─────────────────────

    #[test]
    fn test_service_model_small_prefers_small_keyword() {
        let svc = CatalogService::new();
        let mut rec = ProviderRecord::new("anthropic");
        let now_ms = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_millis() as i64;

        let mut haiku = ModelInfo::new("claude-haiku-3", "anthropic", "Claude Haiku");
        haiku.cost = vec![ModelCost { input: 0.001, output: 0.005 }];
        haiku.released = Some(now_ms - 30 * 24 * 60 * 60 * 1000);

        let mut sonnet = ModelInfo::new("claude-sonnet-4-5", "anthropic", "Claude Sonnet");
        sonnet.cost = vec![ModelCost { input: 0.01, output: 0.03 }];
        sonnet.released = Some(now_ms - 30 * 24 * 60 * 60 * 1000);

        rec.models.insert(haiku.id.clone(), haiku);
        rec.models.insert(sonnet.id.clone(), sonnet);
        svc.provider_upsert(rec);

        let small = svc.model_small("anthropic").expect("should find small");
        assert_eq!(small.id, "claude-haiku-3");
    }

    #[test]
    fn test_service_model_small_fallback_no_small_keyword() {
        let svc = CatalogService::new();
        let mut rec = ProviderRecord::new("openai");
        let now_ms = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_millis() as i64;

        let mut gpt4o = ModelInfo::new("gpt-4o", "openai", "GPT-4o");
        gpt4o.cost = vec![ModelCost { input: 0.005, output: 0.015 }];
        gpt4o.released = Some(now_ms - 5 * 24 * 60 * 60 * 1000);

        let mut gpt4 = ModelInfo::new("gpt-4", "openai", "GPT-4");
        gpt4.cost = vec![ModelCost { input: 0.03, output: 0.06 }];
        gpt4.released = Some(now_ms - 300 * 24 * 60 * 60 * 1000);

        rec.models.insert(gpt4o.id.clone(), gpt4o);
        rec.models.insert(gpt4.id.clone(), gpt4);
        svc.provider_upsert(rec);

        let small = svc.model_small("openai").expect("should find small");
        assert_eq!(small.id, "gpt-4o");
    }

    #[test]
    fn test_service_model_small_provider_not_found() {
        let svc = CatalogService::new();
        assert!(svc.model_small("nonexistent").is_none());
    }

    #[test]
    fn test_service_model_small_excludes_old_models() {
        let svc = CatalogService::new();
        let mut rec = ProviderRecord::new("openai");
        let now_ms = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_millis() as i64;

        let mut old = ModelInfo::new("gpt-3.5-turbo", "openai", "GPT-3.5 Turbo");
        old.cost = vec![ModelCost { input: 0.001, output: 0.002 }];
        old.released = Some(now_ms - 20 * 30 * 24 * 60 * 60 * 1000);

        rec.models.insert(old.id.clone(), old);
        svc.provider_upsert(rec);

        assert!(svc.model_small("openai").is_none());
    }

    // ── CatalogService: policy filtering ──────────────────────────

    #[test]
    fn test_service_apply_policy_removes_unlisted_providers() {
        let svc = CatalogService::new();
        svc.provider_upsert(ProviderRecord::new("openai"));
        svc.provider_upsert(ProviderRecord::new("anthropic"));
        svc.provider_upsert(ProviderRecord::new("gemini"));

        svc.apply_policy(&CatalogPolicyAction::ProviderUse, &["openai", "anthropic"]);

        assert!(svc.provider_get("openai").is_ok());
        assert!(svc.provider_get("anthropic").is_ok());
        assert!(svc.provider_get("gemini").is_err());
    }

    #[test]
    fn test_service_apply_policy_keeps_all_when_all_allowed() {
        let svc = CatalogService::new();
        svc.provider_upsert(ProviderRecord::new("openai"));
        svc.provider_upsert(ProviderRecord::new("anthropic"));

        svc.apply_policy(
            &CatalogPolicyAction::ProviderUse,
            &["openai", "anthropic"],
        );

        assert_eq!(svc.provider_list().len(), 2);
    }

    #[test]
    fn test_service_apply_policy_removes_all_when_none_allowed() {
        let svc = CatalogService::new();
        svc.provider_upsert(ProviderRecord::new("openai"));
        svc.provider_upsert(ProviderRecord::new("anthropic"));

        svc.apply_policy(&CatalogPolicyAction::ProviderUse, &[]);

        assert!(svc.provider_list().is_empty());
    }

    // ── CatalogService: default + available model ─────────────────

    #[test]
    fn test_service_default_falls_back_when_not_available() {
        let svc = CatalogService::new();
        let mut rec = ProviderRecord::new("openai");
        let mut model = ModelInfo::new("gpt-4o", "openai", "GPT-4o");
        model.enabled = false;
        rec.models.insert(model.id.clone(), model);
        svc.provider_upsert(rec);
        svc.set_default_model("openai", "gpt-4o");
        assert!(svc.model_default().is_none());
    }

    // ── CatalogService: finalize ──────────────────────────────────

    #[test]
    fn test_service_finalize_returns_ok() {
        let mut svc = CatalogService::new();
        svc.provider_upsert(ProviderRecord::new("openai"));
        assert!(svc.finalize().is_ok());
    }

    #[test]
    fn test_service_finalize_on_empty_catalog() {
        let mut svc = CatalogService::new();
        assert!(svc.finalize().is_ok());
    }

    // ── CatalogService: provider_available ────────────────────────

    #[test]
    fn test_service_provider_available_filters_all_disabled() {
        let svc = CatalogService::new();
        let mut rec = ProviderRecord::new("openai");
        let mut disabled = ModelInfo::new("gpt-4o", "openai", "GPT-4o");
        disabled.enabled = false;
        rec.models.insert(disabled.id.clone(), disabled);
        svc.provider_upsert(rec);

        let available = svc.provider_available();
        assert!(available.is_empty());
    }

    #[test]
    fn test_service_provider_available_includes_with_enabled_model() {
        let svc = CatalogService::new();
        let mut rec = ProviderRecord::new("openai");
        let mut enabled = ModelInfo::new("gpt-4o", "openai", "GPT-4o");
        enabled.enabled = true;
        rec.models.insert(enabled.id.clone(), enabled);
        svc.provider_upsert(rec);

        let available = svc.provider_available();
        assert_eq!(available.len(), 1);
        assert_eq!(available[0].provider, "openai");
    }

    #[test]
    fn test_service_provider_available_mixed_enabled_disabled() {
        let svc = CatalogService::new();
        let mut rec = ProviderRecord::new("openai");
        let mut enabled = ModelInfo::new("gpt-4o", "openai", "GPT-4o");
        enabled.enabled = true;
        let mut disabled = ModelInfo::new("gpt-4", "openai", "GPT-4");
        disabled.enabled = false;
        rec.models.insert(enabled.id.clone(), enabled);
        rec.models.insert(disabled.id.clone(), disabled);
        svc.provider_upsert(rec);

        let available = svc.provider_available();
        assert_eq!(available.len(), 1);
        assert_eq!(available[0].provider, "openai");
    }

    #[test]
    fn test_service_provider_available_empty_providers() {
        let svc = CatalogService::new();
        let available = svc.provider_available();
        assert!(available.is_empty());
    }

    // ── CatalogEditor ──────────────────────────────────────────────

    #[test]
    fn test_catalog_editor_new() {
        let mut data = CatalogData::default();
        let editor = CatalogEditor::new(&mut data);
        assert!(editor.list_providers().is_empty());
    }

    #[test]
    fn test_catalog_editor_get_provider() {
        let mut data = CatalogData::default();
        data.providers.insert(
            "openai".to_string(),
            ProviderRecord::new("openai"),
        );
        let editor = CatalogEditor::new(&mut data);
        assert!(editor.get_provider("openai").is_some());
        assert!(editor.get_provider("anthropic").is_none());
    }

    #[test]
    fn test_catalog_editor_get_model() {
        let mut data = CatalogData::default();
        let mut rec = ProviderRecord::new("anthropic");
        rec.models.insert(
            "claude-sonnet-4-5".to_string(),
            ModelInfo::new("claude-sonnet-4-5", "anthropic", "Claude Sonnet 4.5"),
        );
        data.providers.insert("anthropic".to_string(), rec);

        let editor = CatalogEditor::new(&mut data);
        assert!(editor.get_model("anthropic", "claude-sonnet-4-5").is_some());
        assert!(editor.get_model("anthropic", "gpt-4o").is_none());
        assert!(editor.get_model("openai", "claude-sonnet-4-5").is_none());
    }

    #[test]
    fn test_catalog_editor_default() {
        let mut data = CatalogData::default();
        let editor = CatalogEditor::new(&mut data);
        assert!(editor.get_default().is_none());
    }

    #[test]
    fn test_catalog_editor_set_default() {
        let mut data = CatalogData::default();
        let mut editor = CatalogEditor::new(&mut data);
        editor.set_default("anthropic", "claude-sonnet-4-5");
        let dm = editor.get_default().expect("default should exist");
        assert_eq!(dm.provider_id, "anthropic");
        assert_eq!(dm.model_id, "claude-sonnet-4-5");
    }

    // ── CatalogService::transform ──────────────────────────────────

    #[test]
    fn test_catalog_service_transform_returns_value() {
        let mut svc = CatalogService::new();
        let count = svc.transform(|editor| {
            editor.data.providers.insert(
                "openai".to_string(),
                ProviderRecord::new("openai"),
            );
            editor.data.providers.insert(
                "anthropic".to_string(),
                ProviderRecord::new("anthropic"),
            );
            editor.list_providers().len()
        });
        assert_eq!(count, 2);
    }

    #[test]
    fn test_catalog_service_transform_modifies_data() {
        let mut svc = CatalogService::new();
        svc.transform(|editor| {
            editor.data.providers.insert(
                "gemini".to_string(),
                ProviderRecord::new("gemini"),
            );
        });
        assert!(svc.provider_get("gemini").is_ok());
    }

    #[test]
    fn test_catalog_service_transform_set_default() {
        let mut svc = CatalogService::new();
        let mut rec = ProviderRecord::new("anthropic");
        rec.models.insert(
            "claude-sonnet-4-5".to_string(),
            ModelInfo::new("claude-sonnet-4-5", "anthropic", "Claude Sonnet 4.5"),
        );
        svc.provider_upsert(rec);

        svc.transform(|editor| {
            editor.set_default("anthropic", "claude-sonnet-4-5");
        });
        let dm = svc.model_default().expect("default should exist");
        assert_eq!(dm.id, "claude-sonnet-4-5");
    }

    #[test]
    fn test_catalog_service_transform_read_only() {
        let mut svc = CatalogService::new();
        svc.provider_upsert(ProviderRecord::new("openai"));
        let count = svc.transform(|editor| editor.list_providers().len());
        assert_eq!(count, 1);
    }

    // ── CatalogService: reset_default ─────────────────────────────

    #[test]
    fn test_service_reset_default_clears() {
        let svc = CatalogService::new();
        let mut rec = ProviderRecord::new("openai");
        rec.models.insert(
            "gpt-4o".to_string(),
            ModelInfo::new("gpt-4o", "openai", "GPT-4o"),
        );
        svc.provider_upsert(rec);
        svc.set_default_model("openai", "gpt-4o");
        assert!(svc.model_default().is_some());

        svc.reset_default();
        assert!(svc.model_default().is_none());
    }

    #[test]
    fn test_service_reset_default_noop_when_none() {
        let svc = CatalogService::new();
        assert!(svc.model_default().is_none());
        svc.reset_default();
        assert!(svc.model_default().is_none());
    }

    // ── CatalogService: thread safety ─────────────────────────────

    #[test]
    fn test_service_concurrent_access() {
        use std::sync::Arc;
        let svc = Arc::new(CatalogService::new());
        let mut handles = Vec::new();
        for i in 0..10 {
            let svc = Arc::clone(&svc);
            handles.push(std::thread::spawn(move || {
                let rec = ProviderRecord::new(format!("provider-{i}"));
                svc.provider_upsert(rec);
                let _ = svc.provider_list();
                let _ = svc.model_all();
            }));
        }
        for h in handles {
            h.join().expect("thread should not panic");
        }
        assert_eq!(svc.provider_list().len(), 10);
    }
}
