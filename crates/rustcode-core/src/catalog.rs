//! Catalog types — provider discovery and model resolution.
//!
//! Ported from: `packages/core/src/catalog.ts`
//! OpenCode commit: 5d0f86606ac30690f79f0a6a9f41a1f49fe95d0b
//!
//! The catalog is the central registry that maps provider IDs to their
//! available models. It supports dynamic discovery (environment variables,
//! config files, API endpoints) and provides defaults for when the user
//! hasn't explicitly selected a provider/model pair.

use serde::{Deserialize, Serialize};

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
}
