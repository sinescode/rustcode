//! Anthropic-compatible provider profiles.
//!
//! Defines profile configurations for providers that speak the Anthropic
//! Messages API wire format but are not the official Anthropic service.
//!
//! Uses the profile-based [`AnthropicProvider`](super::anthropic::AnthropicProvider)
//! system to avoid duplicating stream parsing and message building logic.

use crate::error::Error;
use crate::provider::Provider;
use super::anthropic::{AnthropicProfile, ModelSpec, AnthropicProvider};

// ── OpenModel profile ──────────────────────────────────────────────────

/// OpenModel - Anthropic-compatible API gateway.
pub static OPENMODEL_PROFILE: AnthropicProfile = AnthropicProfile {
    provider_id: "openmodel",
    name: "OpenModel",
    npm: "@ai-sdk/anthropic",
    base_url: "https://api.openmodel.ai",
    env_var: "OPENMODEL_API_KEY",
    models: &OPENMODEL_MODELS,
    extra_headers: &[],
};

/// OpenModel model catalog.
pub static OPENMODEL_MODELS: &[ModelSpec] = &[
    ModelSpec { id: "deepseek-v4-flash", name: "DeepSeek V4 Flash", ctx: 200_000, out: 16_000, family: Some("deepseek"), input_price: 0.15, output_price: 0.60, cache_write_price: 0.075, cache_read_price: 0.15 },
    ModelSpec { id: "deepseek-v4-pro", name: "DeepSeek V4 Pro", ctx: 200_000, out: 16_000, family: Some("deepseek"), input_price: 2.0, output_price: 8.0, cache_write_price: 1.0, cache_read_price: 2.0 },
    ModelSpec { id: "claude-sonnet-4-6", name: "Claude Sonnet 4.6", ctx: 200_000, out: 8_192, family: Some("claude"), input_price: 3.0, output_price: 15.0, cache_write_price: 0.75, cache_read_price: 3.75 },
    ModelSpec { id: "claude-haiku-4-5-20251001", name: "Claude Haiku", ctx: 200_000, out: 8_192, family: Some("claude"), input_price: 1.0, output_price: 5.0, cache_write_price: 0.25, cache_read_price: 1.25 },
    ModelSpec { id: "gemini-3-flash-preview", name: "Gemini 3 Flash Preview", ctx: 1_000_000, out: 16_384, family: Some("gemini"), input_price: 0.10, output_price: 0.40, cache_write_price: 0.05, cache_read_price: 0.10 },
    ModelSpec { id: "gpt-5.4-mini", name: "GPT-5.4 Mini", ctx: 128_000, out: 16_384, family: Some("openai"), input_price: 0.15, output_price: 0.60, cache_write_price: 0.075, cache_read_price: 0.15 },
];

/// All Anthropic-compatible profiles.
pub const PROFILES: &[&AnthropicProfile] = &[
    &OPENMODEL_PROFILE,
];

/// Create a provider from an Anthropic-compatible profile.
pub fn from_profile(profile: &'static AnthropicProfile) -> Result<Box<dyn Provider>, Error> {
    Ok(Box::new(AnthropicProvider::from_profile(profile)?))
}

/// Try to auto-detect all Anthropic-compatible providers from env vars.
pub fn try_all() -> Vec<Box<dyn Provider>> {
    PROFILES
        .iter()
        .filter_map(|p| from_profile(p).ok())
        .collect()
}
