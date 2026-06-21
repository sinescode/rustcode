# Provider/LLM System — Gap Analysis

## Architecture

| Aspect | TS | Rust |
|--------|----|------|
| Service model | Effect.ts Context.Service + Layer | Trait objects (`Box<dyn Provider>`) |
| Streaming | `Stream.Stream<LLMEvent>` | `futures::Stream<Item=Result<LlmEvent>>` |
| Route composition | 4-axis (Protocol+Endpoint+Auth+Framing) | Monolithic per-provider struct |
| Protocol reuse | OpenAIChat shared by 15+ providers | Duplicated 1000+ line copies |
| Model catalog | Remote `models.dev` API | Hardcoded per provider |
| Files | ~70 files across opencode+llm | 13 files |

## Protocol Parity

| Provider | TS | Rust | Status |
|----------|----|------|--------|
| Anthropic | `anthropic-messages.ts` (845L) | `anthropic.rs` | ✅ Good |
| OpenAI Chat | `openai-chat.ts` (493L) | `openai.rs` | ✅ Good |
| **OpenAI Responses** | `openai-responses.ts` (1004L) | **Missing** | **CRITICAL** |
| OpenAI-compatible | 25L + 66L | 974L (monolithic) | ✅ Good |
| Azure | Full | Full | ✅ Good |
| Bedrock | Converse native + event-stream | Chat Completions bridge | ⚠️ Gap |
| Gemini | `gemini.ts` (487L) | `gemini.rs` | ✅ Good |
| xAI | 56L | 802L (14x dup code) | ✅ Good |
| GitHub Copilot | 66L | 985L | ✅ Good |
| OpenRouter | 98L | 616L | ✅ Good |
| Cloudflare Workers AI | Full | Full | ✅ Good |

## Missing TS Providers (in Rust)

- Cloudflare AI Gateway
- Google Vertex AI (ADC/OAuth)
- Google Vertex Anthropic
- SAP AI Core
- Snowflake Cortex
- LLM Gateway
- GitLab workflow

## Missing Features

| Feature | TS | Rust |
|---------|----|------|
| `store: false` default | `transform.ts` | Missing |
| `anthropic-beta` header injection | `provider.ts` | Missing |
| Reasoning effort variants | `transform.ts:665-1043` (37 branches) | Missing |
| Route composition | 12 files | Entirely absent |
| Models from remote catalog | `models-dev.ts` | Hardcoded |
| Snowflake Cortex fetch transform | 100+ lines | Missing |
| SSE per-chunk timeout | `wrapSSE()` | Missing |
| Bedrock credential chain | Full AWS SDK chain | Key-based only |
| Error classification | Centralized | 8 duplicated copies |

## 5 Most Critical Gaps

### 1. Missing OpenAI Responses API protocol (`openai-responses.ts:1004L`)
GPT-5 family uses Responses API. Rust only has Chat Completions.

### 2. No route composition system (`route/` — 12 files, ~1500L)
~5000 lines of duplicated Chat Completions logic across providers. The TS equivalent is 25 lines.

### 3. Hardcoded model catalogs vs remote models.dev
TS fetches model metadata dynamically. Rust requires recompilation for new models.

### 4. Missing reasoning effort variants (`transform.ts:665-1199`, 534L)
Rust has `Model.variants: Option<Variants>` but never populates it.

### 5. Missing Google Vertex AI / OAuth authentication
TS handles Vertex with full ADC, OAuth2 token refresh. Rust has only a bare profile.
