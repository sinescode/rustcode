//! AI SDK integration layer — provider initialization, model options, and
//! NPM-to-provider-ID mappings.
//!
//! # Source
//! Ported from:
//!
//! - `packages/core/src/aisdk.ts` (182 lines) — provider SDK layer, init error,
//!   `prepareOptions()`, and plugin trigger orchestration.
//!
//! BlazeCode commit: 5d0f86606ac30690f79f0a6a9f41a1f49fe95d0b

use serde::{Deserialize, Serialize};
use std::collections::HashMap;

// ── AisdkInitError ────────────────────────────────────────────────────

/// Error during AI SDK provider initialization.
///
/// Maps to the Effect `Schema.TaggedErrorClass` `InitError` in the TS source.
/// The `cause` field is the string representation of the Effect `Cause` that
/// triggered the failure (produced by `Cause.squash()` in the TS layer).
///
/// # Source
/// `packages/core/src/aisdk.ts` — `AISDK.InitError` (class `InitError`,
/// lines 110–113)
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AisdkInitError {
    /// The provider ID for which initialization failed.
    ///
    /// # Source
    /// `packages/core/src/aisdk.ts` line 111 — `providerID: ProviderV2.ID`
    pub provider_id: String,

    /// A human-readable string describing the failure cause.
    ///
    /// # Source
    /// `packages/core/src/aisdk.ts` line 112 — `cause: Schema.Defect`
    /// (rendered via `Cause.squash()` at line 116)
    pub cause: String,
}

impl std::fmt::Display for AisdkInitError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "AI SDK init error for provider `{}`: {}",
            self.provider_id, self.cause
        )
    }
}

impl std::error::Error for AisdkInitError {}

// ── AisdkModelOptions ─────────────────────────────────────────────────

/// Options prepared for an AI SDK model before invoking the provider.
///
/// Built by the equivalent of `prepareOptions()` (TS lines 60–107), which
/// merges the provider ID, API-level settings, request body overrides, and
/// a custom `fetch` wrapper that handles timeouts and chunk abort signals.
///
/// # Source
/// `packages/core/src/aisdk.ts` — return type of `prepareOptions()`
/// (constructed at lines 61–107)
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AisdkModelOptions {
    /// Provider name (maps to `model.providerID` in TS).
    ///
    /// # Source
    /// `packages/core/src/aisdk.ts` line 62 — `options.name = model.providerID`
    pub name: String,

    /// Optional base URL override for the API endpoint.
    ///
    /// # Source
    /// `packages/core/src/aisdk.ts` line 66 — `options.baseURL = model.api.url`
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub base_url: Option<String>,

    /// Optional API key for authentication.
    ///
    /// Populated from the provider or environment; may be `None` when the
    /// provider uses ambient credentials (e.g. cloud IAM).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub api_key: Option<String>,

    /// Additional provider-specific settings merged from the model's API
    /// configuration and request body overrides.
    ///
    /// # Source
    /// `packages/core/src/aisdk.ts` lines 63–64 —
    /// `...(model.api.settings ?? {})` and `...model.request.body`
    #[serde(default)]
    pub settings: HashMap<String, serde_json::Value>,
}

impl AisdkModelOptions {
    /// Create new model options with the given provider name.
    ///
    /// `base_url`, `api_key`, and `settings` are initialized to their
    /// defaults and can be set after construction.
    ///
    /// # Source
    /// `packages/core/src/aisdk.ts` line 62 — initial `name` assignment
    /// in `prepareOptions()`.
    pub fn new(name: impl Into<String>) -> Self {
        Self {
            name: name.into(),
            base_url: None,
            api_key: None,
            settings: HashMap::new(),
        }
    }

    /// Set the base URL and return self for chaining.
    #[must_use]
    pub fn with_base_url(mut self, url: impl Into<String>) -> Self {
        self.base_url = Some(url.into());
        self
    }

    /// Set the API key and return self for chaining.
    #[must_use]
    pub fn with_api_key(mut self, key: impl Into<String>) -> Self {
        self.api_key = Some(key.into());
        self
    }

    /// Insert a setting and return self for chaining.
    #[must_use]
    pub fn with_setting(
        mut self,
        key: impl Into<String>,
        value: impl Into<serde_json::Value>,
    ) -> Self {
        self.settings.insert(key.into(), value.into());
        self
    }
}

// ── AisdkProviderMapping ──────────────────────────────────────────────

/// Maps an AI SDK NPM package name to an BlazeCode provider ID.
///
/// This relationship is used by the provider catalog to route model lookups
/// through the correct AI SDK adapter and plugin hooks. For example,
/// `"@ai-sdk/openai"` maps to `"openai"` and `"@ai-sdk/anthropic"` maps to
/// `"anthropic"`.
///
/// # Source
/// `packages/core/src/aisdk.ts` — derived from the plugin trigger
/// `"aisdk.sdk"` (line 153) where the `package` field carries the NPM
/// name and the provider context is the BlazeCode provider ID.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct AisdkProviderMapping {
    /// NPM package name for the AI SDK provider.
    ///
    /// Examples: `"@ai-sdk/openai"`, `"@ai-sdk/anthropic"`,
    /// `"@openrouter/ai-sdk-provider"`.
    ///
    /// # Source
    /// `packages/core/src/aisdk.ts` line 153 — `package: model.api.package`
    pub package: String,

    /// The BlazeCode provider ID associated with this package.
    ///
    /// Examples: `"openai"`, `"anthropic"`, `"openrouter"`.
    pub provider_id: String,
}

// ═══════════════════════════════════════════════════════════════════════
// Tests
// ═══════════════════════════════════════════════════════════════════════

#[cfg(test)]
mod tests {
    use super::*;

    // ── AisdkInitError ─────────────────────────────────────────────

    #[test]
    fn test_init_error_display() {
        let err = AisdkInitError {
            provider_id: "openai".into(),
            cause: "API key not found".into(),
        };
        let msg = err.to_string();
        assert!(
            msg.contains("openai"),
            "message should contain provider: {msg}"
        );
        assert!(
            msg.contains("API key not found"),
            "message should contain cause: {msg}"
        );
    }

    #[test]
    fn test_init_error_trait_object() {
        let err = AisdkInitError {
            provider_id: "anthropic".into(),
            cause: "Connection refused".into(),
        };
        let _: &dyn std::error::Error = &err;
    }

    #[test]
    fn test_init_error_serialize_roundtrip() {
        let err = AisdkInitError {
            provider_id: "openai".into(),
            cause: "timeout after 30s".into(),
        };
        let json = serde_json::to_string(&err).expect("serialize should succeed");
        let restored: AisdkInitError =
            serde_json::from_str(&json).expect("deserialize should succeed");
        assert_eq!(restored.provider_id, "openai");
        assert_eq!(restored.cause, "timeout after 30s");
    }

    #[test]
    fn test_init_error_json_structure() {
        let err = AisdkInitError {
            provider_id: "gemini".into(),
            cause: "invalid auth".into(),
        };
        let value = serde_json::to_value(&err).expect("serialize should succeed");
        assert_eq!(value["provider_id"], "gemini");
        assert_eq!(value["cause"], "invalid auth");
    }

    // ── AisdkModelOptions ──────────────────────────────────────────

    #[test]
    fn test_model_options_new() {
        let opts = AisdkModelOptions::new("openai");
        assert_eq!(opts.name, "openai");
        assert!(opts.base_url.is_none());
        assert!(opts.api_key.is_none());
        assert!(opts.settings.is_empty());
    }

    #[test]
    fn test_model_options_builder() {
        let opts = AisdkModelOptions::new("anthropic")
            .with_base_url("https://api.anthropic.com/v1")
            .with_api_key("sk-ant-key")
            .with_setting("max_tokens", 4096)
            .with_setting("temperature", 1.0);
        assert_eq!(opts.name, "anthropic");
        assert_eq!(
            opts.base_url.as_deref(),
            Some("https://api.anthropic.com/v1")
        );
        assert_eq!(opts.api_key.as_deref(), Some("sk-ant-key"));
        assert_eq!(
            opts.settings.get("max_tokens").and_then(|v| v.as_u64()),
            Some(4096)
        );
        assert_eq!(
            opts.settings.get("temperature").and_then(|v| v.as_f64()),
            Some(1.0)
        );
    }

    #[test]
    fn test_model_options_serialize_roundtrip() {
        let mut settings = HashMap::new();
        settings.insert("top_p".into(), serde_json::json!(0.95));
        settings.insert("stream".into(), serde_json::json!(true));
        let opts = AisdkModelOptions {
            name: "openai".into(),
            base_url: Some("https://api.openai.com/v1".into()),
            api_key: Some("sk-test".into()),
            settings,
        };
        let json = serde_json::to_string(&opts).expect("serialize should succeed");
        let restored: AisdkModelOptions =
            serde_json::from_str(&json).expect("deserialize should succeed");
        assert_eq!(restored.name, "openai");
        assert_eq!(
            restored.base_url.as_deref(),
            Some("https://api.openai.com/v1")
        );
        assert_eq!(restored.api_key.as_deref(), Some("sk-test"));
        assert_eq!(
            restored.settings.get("top_p").and_then(|v| v.as_f64()),
            Some(0.95)
        );
    }

    #[test]
    fn test_model_options_serialize_minimal() {
        let opts = AisdkModelOptions::new("minimal");
        let json = serde_json::to_value(&opts).expect("serialize should succeed");
        assert_eq!(json["name"], "minimal");
        // Optional fields should be absent when None/empty
        assert!(json.get("base_url").is_none() || json["base_url"].is_null());
        assert!(json.get("api_key").is_none() || json["api_key"].is_null());
    }

    #[test]
    fn test_model_options_deserialize_minimal() {
        let json = serde_json::json!({
            "name": "bare-provider"
        });
        let opts: AisdkModelOptions =
            serde_json::from_value(json).expect("deserialize should succeed");
        assert_eq!(opts.name, "bare-provider");
        assert!(opts.base_url.is_none());
        assert!(opts.api_key.is_none());
        assert!(opts.settings.is_empty());
    }

    // ── AisdkProviderMapping ───────────────────────────────────────

    #[test]
    fn test_provider_mapping_equality() {
        let a = AisdkProviderMapping {
            package: "@ai-sdk/openai".into(),
            provider_id: "openai".into(),
        };
        let b = AisdkProviderMapping {
            package: "@ai-sdk/openai".into(),
            provider_id: "openai".into(),
        };
        assert_eq!(a, b);
    }

    #[test]
    fn test_provider_mapping_serialize_roundtrip() {
        let mapping = AisdkProviderMapping {
            package: "@ai-sdk/anthropic".into(),
            provider_id: "anthropic".into(),
        };
        let json = serde_json::to_string(&mapping).expect("serialize should succeed");
        let restored: AisdkProviderMapping =
            serde_json::from_str(&json).expect("deserialize should succeed");
        assert_eq!(restored, mapping);
    }

    #[test]
    fn test_provider_mapping_json_structure() {
        let mapping = AisdkProviderMapping {
            package: "@openrouter/ai-sdk-provider".into(),
            provider_id: "openrouter".into(),
        };
        let value = serde_json::to_value(&mapping).expect("serialize should succeed");
        assert_eq!(value["package"], "@openrouter/ai-sdk-provider");
        assert_eq!(value["provider_id"], "openrouter");
    }

    #[test]
    fn test_provider_mapping_clone() {
        let mapping = AisdkProviderMapping {
            package: "@ai-sdk/google".into(),
            provider_id: "google".into(),
        };
        let cloned = mapping.clone();
        assert_eq!(cloned, mapping);
    }

    // ── Integration-style: error plus options ──────────────────────

    #[test]
    fn test_error_and_options_together() {
        // Simulate a failed init that leaves both an error and partial options
        let err = AisdkInitError {
            provider_id: "azure".into(),
            cause: "base URL is required but not provided".into(),
        };
        let partial_opts = AisdkModelOptions::new("azure")
            .with_api_key("sk-azure-key")
            .with_setting("resource_name", "my-resource");

        // Error should be displayable
        let err_msg = err.to_string();
        assert!(err_msg.contains("azure"));
        assert!(err_msg.contains("base URL"));

        // Options should be inspectable
        assert_eq!(partial_opts.name, "azure");
        assert_eq!(partial_opts.api_key.as_deref(), Some("sk-azure-key"));
        assert_eq!(partial_opts.base_url, None); // never set — part of the error
    }
}
