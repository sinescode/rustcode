# Plugin System — Gap Analysis

## Architecture

TS has **three** distinct plugin systems. Rust has **one** monolithic file.

| System | TS | Rust |
|--------|----|------|
| V1 Plugin | Auth/chat/shell/tool hooks, dynamic import | `ProviderPlugin` trait (3 hooks only) |
| V2 Plugin (core) | `PluginV2.define({id, effect})`, 3 HookSpecs | `PluginV2Service::trigger()` simplified |
| TUI Plugin | Slots, routes, themes, sounds, API | **None** |

## Interface Gaps

| TS Hooks (20+) | Rust Equivalent | Status |
|----------------|-----------------|--------|
| event (lifecycle) | None | MISSING |
| config | None | MISSING |
| auth (oauth/api) | `load_auth` only | MISSING |
| provider (models) | `discover_models` only | MISSING |
| chat.params, chat.headers, chat.message | None | MISSING |
| tool.definition, tool.execute.before/after | None | MISSING |
| permission.ask | None | MISSING |
| shell.env | None | MISSING |
| command.execute.before | None | MISSING |
| experimental.* (6 hooks) | None | MISSING |
| V2: aisdk.sdk, aisdk.language | None | MISSING |

## Provider Plugin Config Inventory

| Count | Description |
|-------|-------------|
| TS has | **33** provider plugin configs |
| Rust has | **9** (anthropic, openai, google, groq, openrouter, deepinfra, mistral, xai, cohere) |
| Rust depth | **9 are shallow stubs** (zero hooks except anthropic-beta header) |

Missing TS provider plugins (24): alibaba, amazon-bedrock, azure(x2), cerebras, cloudflare-ai-gateway, cloudflare-workers-ai, cohere(+), deepinfra(+), dynamic, gateway, github-copilot, gitlab, google-vertex(x2), kilo, llmgateway, mistral(+), nvidia, openai-compatible, opencode, openai-auth, openrouter(+), perplexity, sap-ai-core, snowflake-cortex, togetherai, venice, vercel, xai(+), zenmux

## TUI Plugin System

**Entire TUI plugin system is absent in Rust** (~2200L TS):
- 14 named slots (app, app_bottom, home_logo, sidebar, etc.)
- 25+ TUI API properties
- Keymap scoping, route registration
- Theme install/sync, sound packs
- Dialog API stack
- Plugin lifecycle (activate/deactivate)

## Loading / Discovery

| Feature | TS | Rust | Status |
|---------|----|------|--------|
| Spec parsing | npm-package-arg | Custom parser | ✅ Parity |
| Entrypoint resolution | Multiple fallbacks | `resolve_package_entrypoint()` | ✅ Parity |
| V1 plugin detection | `readV1Plugin()` | **Missing** | GAP |
| Legacy plugin (function export) | `getLegacyPlugins()` | **Missing** | GAP |
| Theme support | `readPackageThemes()` | **Missing** | GAP |
| Parallel loading | `PluginLoader.loadExternal()` | Sequential | GAP |

## 5 Most Critical Gaps

### 1. V2 Effect-based Plugin System Entirely Missing
The entire `PluginV2.define({id, effect})`, `HookSpec`, `trigger()`/`triggerFor()` dispatch is absent. This is the primary mechanism for provider behavior customization.

### 2. 24 of 33 Provider Plugin Configs Missing
Only 9 ported, all shallow stubs. Missing: vertex, copilot, codex, bedrock, azure, snowflake, etc.

### 3. Entire TUI Plugin System Absent
~2200L of TS with slot rendering, keymap scoping, plugin API, theme install/sync, dialog API — zero Rust equivalent.

### 4. Auth Plugin System Completely Missing
7+ auth plugins (Codex, Copilot, GitLab, DigitalOcean, Snowflake, xAI) implement full OAuth flows with local servers, PKCE, device code, token refresh. **Rust has none.**

### 5. Boot Phase Orchestrator and Config Plugins Missing
TS boot phase registers 17 plugins. Rust only registers 9 provider plugins. Config plugins (agent/command/skill/provider from `.opencode/` markdown) are absent.
