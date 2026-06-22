# Provider Plugin System

Extensibility architecture for adding custom LLM providers to blazecode.

## Overview

The plugin system lets you add LLM providers in three ways:

1. **Config-based** — declare providers in `blazecode.json`
2. **Closure plugins** — quick ad-hoc customization in Rust code
3. **Trait plugins** — full `ProviderPlugin` implementation for complex needs

All three converge through the same initialization pipeline.

## How It Works

### Startup Flow

```
blazecode.json loaded
        │
        ▼
provider_service::init_providers(config)
        │
        ├─ Phase 1: auto_detect_all()
        │   Reads env vars, creates built-in providers
        │   (Anthropic, OpenAI, Gemini, Bedrock, Azure, Cloudflare,
        │    OpenRouter, DeepSeek, Groq, Together, xAI, Mistral, ...)
        │
        ├─ Phase 2: config.provider iteration
        │   For each provider in blazecode.json:
        │   • Collect model overrides
        │   • If not auto-detected, create from config
        │     (needs base_url + env var with API key)
        │
        ├─ Phase 3: transform_catalog (plugin hook)
        │   Each plugin can inject headers, modify options
        │   Plugin can disable a provider by setting enabled=false
        │
        ├─ Phase 4: discover_models (plugin hook)
        │   Plugin can replace a provider's model catalog
        │
        ├─ Phase 5: load_auth (plugin hook)
        │   Plugin can provide custom auth (OAuth, tokens)
        │
        └─ Phase 6: enabled/disabled filters
            Remove disabled_providers, filter by enabled_providers
```

### Key Types

| Type | Location | Purpose |
|------|----------|---------|
| `ProviderPlugin` trait | `plugin.rs` | Three hooks: transform_catalog, discover_models, load_auth |
| `ProviderPluginRegistry` | `plugin.rs` | Stores and triggers plugins during init |
| `ClosureProviderPlugin` | `plugin.rs` | Quick ad-hoc plugins from closures |
| `CustomProviderConfig` | `plugin.rs` | User-defined providers from config |
| `ProviderCatalog` | `provider_service.rs` | Final provider catalog with models and overrides |
| `init_providers()` | `provider_service.rs` | Async entry point for full pipeline |

## Usage

### 1. Config-Based Custom Provider

Add a custom provider in `blazecode.json`:

```json
{
  "provider": {
    "acme": {
      "name": "Acme AI",
      "env": ["ACME_API_KEY"],
      "base_url": "https://api.acme.ai/v1",
      "models": {
        "acme-7b": {
          "name": "Acme 7B",
          "context": 32000,
          "output": 4096
        },
        "acme-70b": {
          "name": "Acme 70B",
          "context": 128000,
          "output": 8192,
          "reasoning": true
        }
      },
      "headers": {
        "X-Custom-Header": "value"
      }
    }
  }
}
```

Set the API key:

```bash
export ACME_API_KEY="sk-..."
```

The provider will be auto-created on startup if the env var is set. It uses the
generic OpenAI-compatible Chat Completions protocol under the hood.

### 2. Closure Plugin

Quick ad-hoc customization without defining a struct:

```rust
use std::sync::Arc;
use blazecode_core::plugin::{
    ClosureProviderPlugin, ProviderPluginRegistry,
};

let mut registry = ProviderPluginRegistry::new();

// Inject a custom header into all Anthropic requests
let plugin = ClosureProviderPlugin::new("my-plugin", "My Plugin")
    .with_transform(|ctx| {
        Box::pin(async move {
            if ctx.provider_id == "anthropic" {
                ctx.headers.insert(
                    "anthropic-beta".to_string(),
                    "interleaved-thinking-2025-05-14,...".to_string(),
                );
            }
        })
    });

registry.register(Arc::new(plugin));
```

### 3. Trait Plugin

Full control over provider behavior:

```rust
use std::collections::HashMap;
use std::sync::Arc;
use async_trait::async_trait;
use blazecode_core::plugin::{
    ProviderPlugin, ProviderPluginRegistry,
    CatalogTransformContext, ModelDiscoverContext, AuthLoadContext,
};
use blazecode_core::provider::Model;

struct MyProviderPlugin;

#[async_trait]
impl ProviderPlugin for MyProviderPlugin {
    fn id(&self) -> &str {
        "my-provider-plugin"
    }

    fn name(&self) -> &str {
        "My Provider Plugin"
    }

    async fn transform_catalog(&self, ctx: &mut CatalogTransformContext<'_>) {
        if ctx.provider_id == "anthropic" {
            ctx.headers.insert(
                "anthropic-beta".to_string(),
                "interleaved-thinking-2025-05-14".to_string(),
            );
        }
    }

    async fn discover_models(
        &self,
        ctx: &ModelDiscoverContext<'_>,
    ) -> Option<Vec<Model>> {
        if ctx.provider_id == "my-provider" {
            // Return custom model list
            Some(vec![
                // ... build models
            ])
        } else {
            None
        }
    }

    async fn load_auth(
        &self,
        ctx: &AuthLoadContext<'_>,
    ) -> Option<HashMap<String, serde_json::Value>> {
        if ctx.provider_id == "my-provider" {
            let mut opts = HashMap::new();
            opts.insert("apiKey".to_string(), serde_json::json!("sk-..."));
            Some(opts)
        } else {
            None
        }
    }
}

let mut registry = ProviderPluginRegistry::new();
registry.register(Arc::new(MyProviderPlugin));
```

## Provider Architecture

### Built-in Providers

| Provider | Protocol | Auth | Module |
|----------|----------|------|--------|
| Anthropic | Messages API | `x-api-key` | `anthropic.rs` |
| OpenAI | Chat Completions | Bearer token | `openai.rs` |
| Gemini | generateContent | `x-goog-api-key` | `gemini.rs` |
| Bedrock | Chat Completions | AWS SigV4 | `bedrock.rs` |
| Azure | Chat Completions | `api-key` header | `azure.rs` |
| Cloudflare | Chat Completions | Bearer token | `cloudflare.rs` |
| OpenRouter | Chat Completions | Bearer token | `openrouter.rs` |
| DeepSeek, Groq, Together, xAI, Mistral, Copilot, Cerebras, Fireworks, AI21, Cohere, Perplexity, DeepInfra, Alibaba | Chat Completions | Bearer token | `openai_compatible.rs` (generic) |

### Generic Provider

The `OpenAICompatibleProvider` in `openai_compatible.rs` handles any provider
that speaks the OpenAI Chat Completions wire protocol. Provider-specific
differences (auth, model catalogs, extra headers) are expressed via
`CompatConfig`:

```rust
pub struct CompatConfig {
    pub provider_id: &'static str,
    pub name: &'static str,
    pub npm: &'static str,
    pub base_url: &'static str,
    pub env_var: &'static str,
    pub models: &'static [ModelSpec],
    pub extra_headers: &'static [(&'static str, &'static str)],
    pub classify_error: fn(u16, &str) -> LlmErrorReason,
}
```

14 pre-configured profiles are defined in `PROFILES`:

```rust
pub const PROFILES: &[CompatConfig] = &[
    CompatConfig { provider_id: "deepseek", ... },
    CompatConfig { provider_id: "groq", ... },
    CompatConfig { provider_id: "togetherai", ... },
    // ... 11 more
];
```

### Custom Provider from Config

`CustomProviderConfig` builds models from `blazecode.json`:

```rust
let config: CustomProviderConfig = serde_json::from_value(json)?;
let models = config.build_models("my-provider", "https://api.example.com/v1");
// Returns Vec<Model> with proper capabilities, limits, etc.
```

## API Reference

### `init_providers(config) -> Result<ProviderCatalog>`

Main entry point. Runs the full 6-phase initialization pipeline.

### `ProviderCatalog`

```rust
pub struct ProviderCatalog {
    pub providers: HashMap<String, Box<dyn Provider>>,
    pub model_overrides: HashMap<String, ModelConfig>,
    pub disabled: Vec<String>,
    pub enabled: Option<Vec<String>>,
}
```

### `ProviderPlugin` trait

```rust
#[async_trait]
pub trait ProviderPlugin: Send + Sync {
    fn id(&self) -> &str;
    fn name(&self) -> &str;

    async fn transform_catalog(&self, ctx: &mut CatalogTransformContext<'_>) {}
    async fn discover_models(&self, ctx: &ModelDiscoverContext<'_>) -> Option<Vec<Model>> { None }
    async fn load_auth(&self, ctx: &AuthLoadContext<'_>) -> Option<HashMap<String, serde_json::Value>> { None }
}
```

### `ClosureProviderPlugin`

```rust
ClosureProviderPlugin::new("id", "Name")
    .with_transform(|ctx| Box::pin(async { ... }))
    .with_discover(|ctx| Box::pin(async { ... }))
    .with_auth(|ctx| Box::pin(async { ... }))
```
