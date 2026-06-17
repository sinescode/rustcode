# Phase 1 Plan — Module [08] Provider

## Source Files Read

| File | Lines | Purpose |
|------|-------|---------|
| `packages/opencode/src/provider/provider.ts` | 1976 | Provider service, model resolution, SDK loading, bundled providers |
| `packages/opencode/src/provider/transform.ts` | 1427 | Message transforms, caching, temperature/topP/topK, variant reasoning efforts, options building |
| `packages/opencode/src/provider/auth.ts` | 233 | Provider auth (OAuth, API key methods, authorize/callback) |
| `packages/opencode/src/provider/error.ts` | 188 | HeaderTimeoutError, ResponseStreamError, parseAPICallError, parseStreamError |
| `packages/opencode/src/provider/model-status.ts` | 9 | ModelStatus literal (alpha/beta/deprecated/active) |
| `packages/llm/src/schema/events.ts` | 373 | LLMEvent tagged union (16 variants), Usage, LLMResponse, PreparedRequest |
| `packages/llm/src/schema/ids.ts` | 44 | ProtocolID, RouteID, ModelID, ProviderID, ResponseID, ContentBlockID, ToolCallID, FinishReason, ReasoningEffort |
| `packages/llm/src/llm.ts` | 187 | LLMClient.generate/stream, request/requestInput, generateObject |
| `packages/llm/src/provider.ts` | 37 | Provider Definition type, ModelFactory |
| `packages/llm/src/provider-error.ts` | 33 | isContextOverflow, isContextOverflowFailure |
| `packages/llm/src/schema/index.ts` | 6 | Re-exports |

## Interface Contract Summary

### 2a. Public API Surface

**Data types** (must match TS exactly):
- `Model` — id, providerID, api (npm/url/id), name, family, capabilities, cost, limit, status, options, headers, release_date, variants
- `ProviderInfo` — id, name, source, env (list), key, options, models
- `Capabilities` — temperature, reasoning, attachment, toolcall, input/output modalities, interleaved
- `Cost` — input, output, cache {read, write}, tiers (optional), experimentalOver200K (optional)
- `TokenLimit` — context, input (optional), output
- `ModelStatus` — "alpha" | "beta" | "deprecated" | "active"
- `ApiInfo` — id, url, npm

**LLM Event types** (16 variants):
- `StepStart`, `TextStart`, `TextDelta`, `TextEnd`
- `ReasoningStart`, `ReasoningDelta`, `ReasoningEnd`
- `ToolInputStart`, `ToolInputDelta`, `ToolInputEnd`
- `ToolCall`, `ToolResult`, `ToolError`
- `StepFinish`, `Finish`, `ProviderErrorEvent`

**Provider service interface**:
- `list()` → `Record<ProviderID, ProviderInfo>`
- `getProvider(id)` → `ProviderInfo`
- `getModel(providerId, modelId)` → `Model`
- `getLanguage(model)` → `LanguageModelV3`
- `closest(providerId, query)` → `{providerID, modelID} | undefined`
- `getSmallModel(providerId)` → `Model | undefined`
- `defaultModel()` → `{providerID, modelID}`

**Transform functions**:
- `message(msgs, model, options)` → transformed messages
- `variants(model)` → reasoning effort variants
- `temperature(model)` → optional default temperature
- `topP(model)` → optional default topP
- `topK(model)` → optional default topK
- `options({model, sessionID, providerOptions})` → provider options
- `smallOptions(model)` → small model options
- `maxOutputTokens(model)` → max output tokens
- `schema(model, jsonSchema)` → sanitized schema
- `providerOptions(model, options)` → provider-namespaced options
- `sanitizeSurrogates(content)` → UTF-16 surrogate fixer

**Error types**:
- `ModelNotFoundError` — {providerID, modelID, suggestions?}
- `InitError` — {providerID, cause?}
- `NoProvidersError` — {}
- `NoModelsError` — {providerID}
- `HeaderTimeoutError` — {ms}
- `ResponseStreamError` — {message}
- `ProviderAuth errors` — OauthMissing, OauthCodeMissing, OauthCallbackFailed, ValidationFailed

### 2b/2c. Bus Events
- Provider does NOT emit bus events directly — it is a dependency service
- Events are consumed through LLM client streaming (LLMEvent stream)

### 2d. Owned vs Borrowed State
- ProviderService holds: `providers: HashMap`, `catalog: HashMap`, `sdk: HashMap`, `modelLoaders: HashMap`, `varsLoaders: HashMap`
- Uses InstanceState pattern (Arc<DashMap> + Arc<RwLock<>>)

### 2e. Error Handling
- All provider operations return Effect with typed error channels
- ModelNotFoundError with fuzzy suggestions
- Context overflow detection via regex patterns
- Retryable classification for 429/5xx errors

### 2f. Performance-Sensitive Paths
- SDK resolution is cached by hash key (npm + options hash)
- Model resolution walks sorted model lists
- SSE streaming with timeout wrapper

### 2g. Permission Integration
- Provider module has NO direct permission integration
- Permissions are checked at the tool/session level, not at provider level

### 2h. Config Dependencies
- Config provider section: `cfg.provider`, `cfg.disabled_providers`, `cfg.enabled_providers`
- `cfg.model` — default model
- `cfg.small_model` — small model override

### 2i. Database Interactions
- No direct database interactions
- Auth state persisted by Auth service (separate module)

### 2k. Network Interactions
- HTTP fetch with: header timeout, chunk timeout, abort signals, combined signals
- Custom fetch hooks per provider
- Base URL resolution with variable substitution

## Rust Design

### 3a. Crate & File Layout
```
crates/rustcode-core/src/
├── provider.rs         # Main provider types + ProviderService trait (single file)
```

This module stays in `rustcode-core` as a single file (~2500 lines estimated) since the existing workspace layout uses single-file modules.

### 3b. Key Additions to Public API

```rust
// Full LLM event enum (16 variants)
pub enum LlmEvent { StepStart{index: u32}, TextStart{id: String}, ... }

// Model variants support
pub type Variants = HashMap<String, HashMap<String, serde_json::Value>>;

// Provider service trait
#[async_trait]
pub trait ProviderService: Send + Sync {
    async fn list(&self) -> Result<HashMap<String, ProviderInfo>>;
    async fn get_provider(&self, provider_id: &str) -> Result<ProviderInfo>;
    async fn get_model(&self, provider_id: &str, model_id: &str) -> Result<Model>;
    async fn closest(&self, provider_id: &str, query: &[String]) -> Result<Option<ModelRef>>;
    async fn get_small_model(&self, provider_id: &str) -> Result<Option<Model>>;
    async fn default_model(&self) -> Result<ModelRef>;
    // SDK/catalog operations
    // Transform operations
}
```

### 3c. No New Crates Needed
All dependencies already in Cargo.toml.

### 3d. Concurrency Model
- ProviderService state: `Arc<RwLock<ProviderState>>`
- State contains: providers HashMap, catalog HashMap, language model cache (Arc<DashMap>)
- SDK loading is async but cached — single-flight pattern

### 3e. Error Enums
Already covered by existing `error.rs` — add `ModelNotFound.suggestions` field, `ProviderAuthError` enum.

### 3f. No SQLite Changes for Provider Module
Auth state handled by Auth module separately.

### 3g. Streaming Design
- LlmEvent stream: `Pin<Box<dyn Stream<Item = Result<LlmEvent>> + Send>>`
- Events map 1:1 to TS `LLMEvent` tagged union

### 3i. Testing Strategy
- Model struct serialization round-trip tests
- LlmEvent parsing tests
- Temperature/topP/topK default lookup tests
- Model sort order tests
- parseModel test
- Variant computation tests (for each provider family)
- SanitizeSurrogates tests
- Context overflow detection tests

## Implementation Steps (Phase 2)

### Step A: Expand data structures
- Add `LlmEvent` enum (16 variants)
- Add `Usage` struct with all fields
- Add `LlmResponse` struct
- Add `Variants` type alias
- Add `Model` variants field
- Add `ModelRef` struct
- Add `TokenLimit.input` optional field
- Add `Cost` tiers and experimentalOver200K fields

### Step B: Add transform functions
- `sanitize_surrogates()`
- `temperature()`
- `top_p()`
- `top_k()`
- `sort_models()`
- `parse_model()`
- `model_suggestions()` (fuzzy matching)

### Step C: Add full Model and ProviderInfo types
- `Model` with all fields from TS
- `from_models_dev_model()`
- `from_models_dev_provider()`

### Step D: Add variant computation
- `variants()` for all provider families
- Reasoning efforts lookup per model/provider

### Step E: Add ProviderService trait + default implementation
- All 7 methods with full logic

### Step F: Add tests
- ~40+ tests covering all the above

## Behavioral Parity Checklist
- [ ] All Model fields present (id, providerID, api, name, family, capabilities, cost, limit, status, options, headers, release_date, variants)
- [ ] All 16 LlmEvent variants
- [ ] Usage with inclusive totals + breakdown
- [ ] Model sort by priority (gpt-5, claude-sonnet-4, big-pickle, gemini-3-pro)
- [ ] parseModel "provider/model" parsing
- [ ] Fuzzy model suggestions
- [ ] Temperature defaults per model family
- [ ] topP defaults
- [ ] topK defaults
- [ ] Sanitize UTF-16 surrogates
- [ ] Context overflow regex detection
