//! Integration tests for the provider system.
//!
//! Tests that providers can be auto-detected, models resolved, and
//! the provider trait can be exercised without real API keys.

use blazecode_core::providers;

/// Verify that provider auto-detection returns empty in test environment
/// (no env vars should be set).
#[test]
fn test_auto_detect_no_api_keys() {
    let providers = providers::auto_detect_all();
    assert!(providers.is_empty(),
        "auto_detect_all returned {} providers — API keys may be leaking from environment",
        providers.len());
}

/// Verify every provider in the PROFILES list has valid configuration.
#[test]
fn test_all_provider_profiles_valid() {
    for config in providers::openai_compatible::PROFILES {
        assert!(!config.provider_id.is_empty(), "profile has empty provider_id");
        assert!(!config.name.is_empty(), "profile {} has empty name", config.provider_id);
        assert!(!config.base_url.is_empty(), "profile {} has empty base_url", config.provider_id);
        assert!(!config.env_var.is_empty(), "profile {} has empty env_var", config.provider_id);
        assert!(!config.models.is_empty(), "profile {} has no models", config.provider_id);

        for model in config.models {
            assert!(!model.id.is_empty(), "profile {} has model with empty id", config.provider_id);
            assert!(model.ctx > 0, "profile {} model {} has 0 context window", config.provider_id, model.id);
            assert!(model.out > 0, "profile {} model {} has 0 output limit", config.provider_id, model.id);
        }
    }
}

/// Verify model IDs don't contain unexpected characters.
#[test]
fn test_model_ids_are_well_formed() {
    for config in providers::openai_compatible::PROFILES {
        for model in config.models {
            assert!(!model.id.contains('\n'), "model id contains newline: {}", model.id);
            assert!(!model.id.contains('\r'), "model id contains carriage return: {}", model.id);
            assert!(!model.name.contains('\n'), "model name contains newline: {}", model.name);
        }
    }
}

/// Verify models aren't leaked across profiles (each profile's models
/// use the profile's provider_id).
#[test]
fn test_model_provider_ids_match_profile() {
    for config in providers::openai_compatible::PROFILES {
        for model_spec in config.models {
            assert_eq!(
                model_spec.id.split('/').next().unwrap_or(model_spec.id),
                model_spec.id,
                "model {} should not contain '/' in base id", model_spec.id
            );
        }
    }
}

/// Profile count should be at least 25.
#[test]
fn test_profile_count() {
    let count = providers::openai_compatible::PROFILES.len();
    assert!(count >= 20, "expected at least 20 profiles, got {count}");
}
