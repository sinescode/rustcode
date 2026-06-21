# LLM Provider Parity — Pass 2: Dedicated xAI and GitHub Copilot Modules

**Date**: 2026-06-21  
**Previous report**: `reports/05-providers.md`  
**Scope**: Promote xAI and GitHub Copilot from generic `openai_compatible` profiles to dedicated provider modules

---

## Summary

Two providers were promoted from generic OpenAI-compatible profiles to dedicated modules:

| Provider | Old (Pass 1) | New (Pass 2) | Auth | Streaming |
|----------|-------------|--------------|------|-----------|
| **xAI (Grok)** | `openai_compatible` profile (~15 lines config) | `xai.rs` dedicated module (~780 lines) | `XAI_API_KEY` env var, Bearer token | SSE, tool calling, reasoning |
| **GitHub Copilot** | `openai_compatible` profile (~15 lines config) | `github_copilot.rs` dedicated module (~750 lines) | `GITHUB_TOKEN` → Copilot token exchange (or `COPILOT_TOKEN` direct) | SSE, tool calling, reasoning |

---

## Files Created

### 1. `/root/opencodesport/rustcode/crates/rustcode-core/src/providers/xai.rs` (780 lines)

**Dedicated xAI (Grok) provider module.**

- **Auth**: `XAI_API_KEY` environment variable → Bearer token
- **Endpoint**: `https://api.x.ai/v1/chat/completions`
- **Streaming**: SSE with OpenAI-compatible event format (`choices[].delta`)
- **Features**:
  - Chat completions (streaming and non-streaming via `complete()`)
  - Tool/function calling with streaming JSON accumulation
  - Reasoning content (`reasoning_content` field)
  - Reasoning effort configuration (default "medium" for Grok 3+)
  - Usage reporting with cached/reasoning token breakdown
- **Models** (8 models):
  - `grok-4` — 1M context, 128K output, reasoning+vision ($2.50/$10.00)
  - `grok-3` — 128K context, 16K output, reasoning+vision ($2.00/$8.00)
  - `grok-3-mini` — 128K context, 16K output, reasoning ($1.00/$4.00)
  - `grok-3-fast` — 128K context, 16K output, vision ($2.00/$8.00)
  - `grok-3-latest` — 128K context, 16K output, reasoning+vision ($2.00/$8.00)
  - `grok-2` / `grok-2-latest` — 128K context, 16K output, vision ($1.50/$6.00)
  - `grok-beta` — 128K context, 8K output ($0.50/$2.00)
- **Provider trait**: Full implementation (`list_models`, `get_model`, `stream`, `complete`)
- **Error handling**: HTTP status classification (401→Auth, 429→RateLimit, 400/413→context overflow, 500→ProviderInternal)
- **`auto_detect()`**: Returns `ProviderInfo` if `XAI_API_KEY` is set
- **Tests**: 6 test cases (finish reason mapping, error classification, model catalog, usage mapping, key resolution, text extraction, finish events)

### 2. `/root/opencodesport/rustcode/crates/rustcode-core/src/providers/github_copilot.rs` (750 lines)

**Dedicated GitHub Copilot provider module.**

- **Auth flow** (unique among providers):
  1. Try direct `COPILOT_TOKEN` env var (for pre-exchanged tokens)
  2. Exchange `GITHUB_TOKEN` via `POST https://api.github.com/copilot_internal/v2/token`
  3. Use returned `token` as Bearer credential
- **Synchronous**: `new()` — reads `COPILOT_TOKEN` or `GITHUB_TOKEN` directly (no async exchange)
- **Async**: `new_async()` — performs live token exchange via HTTP
- **Direct**: `with_token(copilot_token)` — uses an explicit pre-exchanged token
- **Endpoint**: `https://api.githubcopilot.com/chat/completions`
- **Headers**: VS Code integration headers (`Copilot-Integration-Id: vscode`, `Editor-Version: vscode/1.95.0`)
- **Features**:
  - Chat completions with SSE streaming
  - Tool/function calling
  - Reasoning content
  - Auto-selects model API: `should_use_responses_api()` for GPT-5+ detection
- **Models** (4 models):
  - `gpt-4o-copilot` — 128K context, 16K output, vision
  - `claude-sonnet-4-copilot` — 200K context, 8K output, vision
  - `claude-3.5-sonnet-copilot` — 200K context, 8K output, vision
  - `gpt-4o-mini-copilot` — 128K context, 16K output, vision
- **`auto_detect()`**: Returns `ProviderInfo` if `GITHUB_TOKEN` or `COPILOT_TOKEN` is set
- **Tests**: 7 test cases (responses API detection, finish reason mapping, error classification, model catalog, usage mapping, token error classification)

## Files Modified

### 3. `/root/opencodesport/rustcode/crates/rustcode-core/src/providers/mod.rs`

**Changes**:
- Added `pub mod github_copilot;` declaration
- Added `pub mod xai;` declaration
- Updated module doc comment table with xAI and GitHub Copilot rows
- Added dedicated auto-detection for `xai::XaiProvider::new()` and `github_copilot::GitHubCopilotProvider::new()` in `auto_detect_all()`
- Added note that these providers were promoted from `openai_compatible` profiles

### 4. `/root/opencodesport/rustcode/crates/rustcode-core/src/providers/openai_compatible.rs`

**Changes**:
- Removed xAI profile (`provider_id: "xai"`) from `PROFILES` array
- Removed GitHub Copilot profile (`provider_id: "github_copilot"`) from `PROFILES` array
- Updated module doc comment to remove xAI and GitHub Copilot from the generic coverage list

---

## Provider Parity Update

| Metric | Before (Pass 1) | After (Pass 2) |
|--------|-----------------|----------------|
| **OpenAI-compatible profiles** | 25 | 23 |
| **Dedicated protocol modules** | 7 | 9 |
| **Auth methods** | Bearer, x-api-key, x-goog-api-key, api-key, SigV4 | Bearer, x-api-key, x-goog-api-key, api-key, SigV4, **OAuth token exchange** |
| **Documented providers** | 34 | 34 (xAI and Copilot promoted to dedicated) |

The new GitHub Copilot module adds an **OAuth token exchange** auth method (`GITHUB_TOKEN` → Copilot token), which was previously unavailable in the generic profile (which only supported Bearer token auth).

---

## Design Decisions

### xAI module
- Follows the same architecture as `openai.rs` — typed SSE event structs, `build_body()` method, `events_from_chat()` mapper
- Reuses `ToolStreamAccumulator` for streaming tool call JSON accumulation
- Reasoning effort is set to "medium" by default for Grok 3+ models, matching opencode's behavior
- The module doc comment cites both `xai.ts` and `openai-compatible-profile.ts` as sources

### GitHub Copilot module
- The token exchange flow is the key differentiator from the generic profile
- `new()` is synchronous and reads `COPILOT_TOKEN` or `GITHUB_TOKEN` directly (no HTTP exchange)
- `new_async()` performs the live exchange — this is the recommended path for production use
- `auto_detect()` checks for either env var, so users with a direct token are also detected
- Copilot-specific headers (`Copilot-Integration-Id`, `Editor-Version`) are sent with every request
- The `should_use_responses_api()` function mirrors opencode's `ModelOptions.shouldUseResponsesApi()` for GPT-5+ model detection
- Cost is set to default (free/zero) since Copilot is typically bundled with GitHub subscriptions
- The module doc comment cites `github-copilot.ts` as the source

---

## Recommendations for Future Work

1. **xAI Responses API**: Add Responses API protocol (like opencode's `OpenAIResponses.route`) for newer xAI models
2. **GitHub Copilot device OAuth flow**: Implement the full device OAuth flow for environments where `GITHUB_TOKEN` is not available
3. **Copilot token refresh**: Add automatic token refresh when the Copilot token expires (respecting `expires_at`)
4. **Dynamic model catalogs**: Replace hardcoded model lists with runtime fetching from provider APIs

---

## Files Changed

| File | Action | Lines |
|------|--------|-------|
| `src/providers/xai.rs` | **Created** | 780 |
| `src/providers/github_copilot.rs` | **Created** | 750 |
| `src/providers/mod.rs` | Modified | - |
| `src/providers/openai_compatible.rs` | Modified | -2 profiles |

No `cargo` commands were executed. All changes are source-only.
