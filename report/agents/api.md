# RustCode vs OpenCode: API Analysis Report

**Agent**: Agent 09 — API Agent  
**Date**: 2026-06-21  
**Scope**: Public & internal APIs across all crates

---

## 1. Public API Surface

### Finding 1.1 — RustCode `pub` on every module (no API firewall)

- **Location**: `rustcode-core/src/lib.rs:11-95`
- **OpenCode**: Uses Effect.ts `Context.Tag`, explicit service interfaces, and `export`-controlled module boundaries. Only intended surfaces are exported from packages. `packages/sdk/js/` has a tightly controlled re-export surface (`index.ts`, `client.ts`, `server.ts`, `v2/`).
- **RustCode**: `pub mod` for **all 95 modules** — every module is public with no sub-visibility. `pub use` re-exports are minimal (e.g. `rustcode-server/src/lib.rs:31-33` exports only 3 items). The crate has no internal `pub(crate)` module discipline — everything is world-visible.
- **Gap**: RustCode has no API firewall. Every internal helper, stub, and skeleton module is `pub`. OpenCode's SDK is a purpose-built, minimal API surface.
- **Consequence**: Breaking changes can impact downstream consumers of any module. Documentation generation includes all internal modules. Users cannot distinguish "what's intended for me" vs "internal implementation detail."
- **Recommendation**: Add `#[doc(hidden)]` to internal modules. Restructure with a `_internal` pattern or conditional compilation. Adopt `pub(crate)` as default, promoting to `pub` only for the public API boundary.
- **Severity**: **High**

### Finding 1.2 — rustcode-mcp re-exports from rustcode-core

- **Location**: `rustcode-mcp/src/lib.rs:44-48`
- **OpenCode**: The MCP SDK (`@modelcontextprotocol/sdk`) is a separate dependency. OpenCode's MCP logic in `packages/opencode/src/mcp/` uses the SDK but does not re-export it.
- **RustCode**: `rustcode-mcp` re-exports ~12 types from `rustcode_core::mcp`. This creates a dual-surface problem — MCP types can be accessed via either crate.
- **Gap**: Dual public API surface for MCP types creates confusion about which crate is authoritative.
- **Consequence**: Users who depend on both `rustcode_core` and `rustcode-mcp` get redundant type paths.
- **Recommendation**: Re-exporting is fine for convenience, but `rustcode-core::mcp` should be the canonical definition. Document that `rustcode-mcp` re-exports are convenience aliases.
- **Severity**: **Low**

---

## 2. API Consistency

### Finding 2.1 — Naming convention: camelCase vs snake_case in serde

- **Location**: `rustcode-core/src/provider.rs:298-299` (`providerID`, `modelID`), `rustcode-core/src/config.rs:99` (`logLevel`)
- **OpenCode**: Consistent camelCase in JSON wire format (standard JS convention).
- **RustCode**: Mixed conventions — most serde `rename` attributes use `camelCase`, but some use `snake_case`. Conditionally inconsistent within the same crate (`config.rs` uses `rename_all = "camelCase"` on some structs, default snake_case on others).
- **Gap**: Some Rust structs fail to match OpenCode's JSON wire format because serde rename attributes are missing or inconsistent.
- **Consequence**: Clients receiving JSON from RustCode's server may get `provider_id` instead of the expected `providerID`, breaking compatibility with OpenCode SDK clients.
- **Recommendation**: Audit all `Serialize`/`Deserialize` structs against OpenCode TS types. Add `#[serde(rename_all = "camelCase")]` consistently. Add roundtrip tests that verify parity with TS snapshot outputs.
- **Severity**: **High**

### Finding 2.2 — `Option<Option<T>>` pattern in SessionPatch

- **Location**: `rustcode-core/src/session.rs:1496-1509`
- **OpenCode**: Uses `undefined` vs `null` distinction at the JavaScript level to signal "unset" vs "set to null." The TS Effect.Schema handles this with `optional` and `Schema.optional(Schema, { exact: true, as: "Option" })`.
- **RustCode**: `SessionPatch` uses `Option<Option<T>>` to distinguish "don't update" (`None`) from "set to null" (`Some(None)`). This is technically correct but confusing.
- **Gap**: The `Option<Option<T>>` pattern is non-idiomatic Rust. No other patch type in the codebase uses it consistently.
- **Consequence**: Maintainability burden — developers must remember the double-option semantics.
- **Recommendation**: Introduce a `Patch<T>` newtype with clear semantics: `Patch::Unset`, `Patch::SetToNull`, `Patch::Set(T)`. Document the pattern once and reuse.
- **Severity**: **Medium**

---

## 3. API Stability

### Finding 3.1 — No versioning strategy or deprecation policy

- **Location**: All crates' `Cargo.toml` (version `0.1.0`)
- **OpenCode**: NPM packages use semver (`@opencode-ai/sdk@1.17.8`). The TS source has a `V2_SESSION` feature flag and coexisting V1/V2 paths. Breaking changes are managed through SDK generation (`@hey-api/openapi-ts`).
- **RustCode**: All crates at `0.1.0` with no versioning policy documented. No changelog or migration guide.
- **Gap**: No semver discipline. No `#[deprecated]` annotations. No public API compatibility testing.
- **Consequence**: Any change could be breaking. Downstream consumers cannot safely depend on any specific API surface.
- **Recommendation**: Establish semver policy. Add `#![warn(deprecated_safe)]`. Create an API compat test suite. Use `cargo-semver-checks` in CI.
- **Severity**: **Critical**

---

## 4. Internal vs External APIs

### Finding 4.1 — No `pub(crate)` discipline

- **Location**: `rustcode-core/src/lib.rs:11-95` (all 95 modules `pub`)
- **OpenCode**: TS uses `_internal.ts` / `internal/` directories and explicit export lists. Effect.ts services are accessed through `Context.Tag`.
- **RustCode**: Every module is `pub` regardless of whether it's an internal implementation detail. For example, `database.rs`, `flock.rs`, `ripgrep.rs` are likely implementation details but are all `pub mod`.
- **Gap**: No internal module hiding. Everything is externally accessible.
- **Consequence**: Impossible to refactor internals without potentially breaking external consumers. Leaks implementation details into docs.
- **Recommendation**: Audit each module. Modules that are not part of the public API should be `pub(crate)`. Only expose through controlled re-exports in `lib.rs`.
- **Severity**: **High**

---

## 5. Trait Design

### Finding 5.1 — Provider trait is coherent and minimal

- **Location**: `rustcode-core/src/provider.rs:907-940`
- **OpenCode**: `packages/llm/src/provider.ts` — the Vercel AI SDK's `LanguageModelV1` interface with `doStream()`, `doGenerate()`.
- **RustCode**: `Provider` trait has 5 methods: `provider_id()`, `npm()`, `list_models()`, `get_model()`, `stream()`, `complete()`. Well-balanced.
- **Gap**: The `stream()` return type uses `Box<dyn futures::Stream<Item = Result<LlmEvent>> + Send + Unpin>` — complex but necessary for async trait dispatch.
- **Consequence**: The trait is abstract enough for generic provider implementations but concrete enough to be useful.
- **Recommendation**: Consider making `stream` and `complete` default methods where one delegates to the other. No urgent fix needed.
- **Severity**: **Info**

### Finding 5.2 — Tool trait has a design tension: schema vs LLM definition

- **Location**: `rustcode-core/src/tool.rs:163-201`
- **OpenCode**: `packages/opencode/src/tool/tool.ts` — `Tool` interface with `schema`, `execute`, and separate LLM-facing `definition`.
- **RustCode**: `Tool` trait has both `json_schema()` (optional, what LLM sees) and `parameters_schema()` (always present). This dual-schema pattern is confusing — the relationship between them is undocumented.
- **Gap**: Two schema methods with unclear semantics. `json_schema()` returns `Option<Value>`, `parameters_schema()` returns `Value` unconditionally. When do they differ? Why?
- **Consequence**: Tool implementors are uncertain which schema to provide. Might lead to LLM seeing a different schema than the validator uses.
- **Recommendation**: Either merge into one method or add clear doc comments explaining the distinction (one is for LLM consumption, one for server-side validation). Better: use `parameters_schema()` everywhere and remove `json_schema()`.
- **Severity**: **Medium**

### Finding 5.3 — Plugin trait hierarchy is complex (V1 + V2 + Provider)

- **Location**: `rustcode-core/src/plugin.rs:80-115` (ProviderPlugin), `rustcode-core/src/plugin.rs:784-871` (PluginHooks), `rustcode-core/src/plugin.rs:1231-1340` (PluginV2Handler)
- **OpenCode**: `packages/core/src/plugin.ts` — `PluginV2.define()` with hook-type dispatching. A single unified plugin definition.
- **RustCode**: Three separate plugin concepts: `ProviderPlugin` (provider-level hooks), `PluginHooks` (V1 lifecycle hooks with 18 methods), `PluginV2Handler` (V2 hooks with 3 methods). Each has a separate registry.
- **Gap**: Fragmented plugin architecture. Provider plugins and full plugins are separate traits with separate registries, unlike OpenCode's unified `PluginV2.define()`.
- **Consequence**: Plugins that need both provider customization and lifecycle hooks must implement two traits and register with two registries.
- **Recommendation**: Consider merging into a single unified `Plugin` trait with optional hooks (default methods returning `None`/no-op), similar to OpenCode's `PluginV2` approach.
- **Severity**: **Medium**

---

## 6. Error Types

### Finding 6.1 — Comprehensive error hierarchy with good documentation

- **Location**: `rustcode-core/src/error.rs:1-1315`
- **OpenCode**: Effect.ts provides ~120+ `Schema.TaggedErrorClass` types. Rich structured errors with classification, retry hints, and HTTP context.
- **RustCode**: `Error` enum with ~50+ variants organized by domain. `LlmErrorReason` sub-enum with 10 variants. `PermissionError`, `WorktreeError`, `ImageError`, `SkillError` sub-enums. `ApiError` for HTTP layer. All with `thiserror::Error` derive and `#[error("...")]` format strings.
- **Gap**: None significant. The error hierarchy is one of the best-designed parts of RustCode. All variants have clear doc comments with source references.
- **Consequence**: Errors are matchable, displayable, and source-referenced. Excellent.
- **Recommendation**: Add `#[source]` annotations for nested errors (e.g., `Error::Io` could show the inner source). Ensure all `From` impls exist for the sub-enum types.
- **Severity**: **Info**

### Finding 6.2 — ServerError duplicates core Error

- **Location**: `rustcode-core/src/error.rs:613-650` (ApiError), `rustcode-server/src/error.rs:19-243` (ServerError)
- **OpenCode**: Single error hierarchy; server layer maps domain errors to HTTP responses.
- **RustCode**: Two separate error enums: `ApiError` in core (8 variants) and `ServerError` in server (18+ variants). They overlap significantly (both have `NotFound`, `InvalidRequest` equivalents).
- **Gap**: Duplicate error types with overlapping semantics. No documented conversion path between them (`IntoServerError` trait exists but is empty `rustcode-server/src/error.rs:336-338`).
- **Consequence**: Error mapping from core to server is ad-hoc and inconsistent. Some handlers use `ServerError::unknown(e.to_string())` which loses structured error data.
- **Recommendation**: Remove `ApiError` from core and use `ServerError` exclusively in the server crate. Implement proper `From<rustcode_core::error::Error>` for `ServerError`.
- **Severity**: **High**

---

## 7. Type Safety

### Finding 7.1 — Stringly-typed IDs

- **Location**: `rustcode-core/src/provider.rs:24-48` (`ModelId = String`, `ProviderId = String`, etc.) and `rustcode-core/src/session.rs:83-90` (`SessionId = String`, `MessageId = String`, `PartId = String`)
- **OpenCode**: TypeScript uses branded string types (`type ModelId = string & { readonly __brand: "ModelId" }`). The Effect schema provides runtime validation.
- **RustCode**: All IDs are `pub type X = String;` type aliases — no compile-time type safety. You can pass a `SessionId` where a `MessageId` is expected with no compiler error.
- **Gap**: No newtype wrappers for semantically distinct IDs. Zero type-level safety for ID misuse.
- **Consequence**: Easy to mix up ID types (passing `ModelId` as `ProviderId`, etc.) at compile time with no protection. Runtime errors are the only guard.
- **Recommendation**: Convert each ID alias to a newtype wrapper:

```rust
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct SessionId(String);

impl SessionId {
    pub fn new(s: impl Into<String>) -> Self { Self(s.into()) }
    pub fn as_str(&self) -> &str { &self.0 }
}
```

Or use a macro to reduce boilerplate. This is the most impactful type-safety improvement available.
- **Severity**: **High**

### Finding 7.2 — TaggedString and branded types in schema.rs

- **Location**: `rustcode-core/src/schema.rs:304-306`
- **OpenCode**: Effect.Schema's `Newtype<Tag>()` factory for branded types.
- **RustCode**: `TaggedString` exists but is unused — it wraps a `String` with a const-generic tag... except `TaggedString` has no const generic. It's just `pub struct TaggedString(pub String);` — indistinguishable from a bare `String`.
- **Gap**: The `TaggedString` type is supposed to be a generic newtype factory but lacks the const-generic parameter that would make different tags different types.
- **Consequence**: Unused, broken abstraction. The branded type system promised by `schema.rs` is not actually delivered.
- **Recommendation**: Either implement the const-generic newtype properly or remove it. The newtype approach in Finding 7.1 is more practical.
- **Severity**: **Medium**

---

## 8. Serialization Contracts

### Finding 8.1 — JSON Schema for tool definitions is well-implemented

- **Location**: `rustcode-core/src/tool.rs:868-1154`
- **OpenCode**: `packages/opencode/src/tool/json-schema.ts` — same normalization, `$ref` inlining, `allOf` flattening, null stripping.
- **RustCode**: Complete port of the JSON Schema normalization pipeline: `normalize_json_schema()`, `inline_local_refs()`, `drop_defs_if_resolved()`. Has comprehensive logic for handling edge cases (anyOf unwrapping, integer bounds, null stripping).
- **Gap**: None significant. The schema normalization is a faithful port with good test coverage.
- **Consequence**: Tools produce LLM-compatible JSON schemas identical to OpenCode.
- **Recommendation**: Add property-based testing (proptest) for JSON Schema normalization to catch edge cases.
- **Severity**: **Info**

### Finding 8.2 — Config schema has dual V1/V2 layout

- **Location**: `rustcode-core/src/config.rs:89-240` (V1 `Info`), `rustcode-core/src/config.rs:299-350` (V2 `V2ConfigInfo`)
- **OpenCode**: `packages/core/src/v1/config/config.ts` + migration from V1 → V2. Separate schemas with migration path.
- **RustCode**: Two complete struct hierarchies (`Info` + sub-types; `V2ConfigInfo` + sub-types). Both coexist in the same module.
- **Gap**: The V1 → V2 migration logic is not implemented. `V2ConfigInfo` is defined but never used in production paths. No `migrate()` function.
- **Consequence**: Config loading uses V1 format only. V2 config is dead code.
- **Recommendation**: Either implement migration or remove the dead V2 schema types. Add a `migrate_v1_to_v2()` function if both are needed.
- **Severity**: **Medium**

---

## 9. API Versioning

### Finding 9.1 — V1 and V2 session representations coexist

- **Location**: `rustcode-core/src/session.rs` (V1 message/part model), `rustcode-core/src/v2_schema.rs` (V2 timestamp helpers)
- **OpenCode**: V2 session core (`packages/core/src/session-v2/` and `packages/opencode/src/session/`) with clear separation. V1 deprecated but supported.
- **RustCode**: Single `session.rs` implements the V2-style data model (Message, Part, SessionProcessor). No V1 session model exists. `v2_schema.rs` only has DateTime helpers.
- **Gap**: The session data model is already V2-native (which is good), but the `v2_schema.rs` module is misleading — it only provides serde helpers, not a V2 feature set.
- **Consequence**: Less of a gap; RustCode correctly jumped straight to V2 session model.
- **Recommendation**: Rename `v2_schema.rs` to `datetime.rs` or `serde_helpers.rs` to avoid confusion. Document that the session module is V2-native.
- **Severity**: **Low**

---

## 10. REST API Design

### Finding 10.1 — Route structure mirrors OpenCode but is incomplete

- **Location**: `rustcode-server/src/routes/api.rs:116-181`
- **OpenCode**: Server routes are split across `packages/server/src/api.ts` + 30+ group files under `routes/groups/`. All supported in production.
- **RustCode**: Single `api.rs` file with 25+ routes under `/api/`. Handlers are stubs or simplified — `api_session_prompt`, `api_session_compact`, `api_session_wait` are minimal stubs. `api_fs` reads files directly (no permission checks).
- **Gap**: Handlers lack real implementation. Many return placeholder data (`api_session_compact` returns `NO_CONTENT`; `api_session_wait` returns `NO_CONTENT`). No authentication middleware in the server crate.
- **Consequence**: The server crate cannot serve as a real backend. SSE event streaming works but session prompt execution is stub-only.
- **Recommendation**: Document which routes are real vs stubs. Implement at minimum the session CRUD + prompt execution paths. Add auth middleware.
- **Severity**: **Critical**

### Finding 10.2 — Route URL patterns are RESTful

- **Location**: `rustcode-server/src/routes/api.rs:116-181`
- **OpenCode**: Same pattern — `/api/session/:sessionID`, `/api/session/:sessionID/prompt`, etc.
- **RustCode**: Uses `{sessionID}` (axum 0.8 path param syntax). Consistent URL patterns. Response formats match OpenCode's JSON structure.
- **Gap**: None in the route definitions themselves. The URL structure is correct.
- **Consequence**: API surface is wire-compatible with OpenCode clients.
- **Recommendation**: Add integration tests that validate response shapes against OpenCode SDK types.
- **Severity**: **Low**

### Finding 10.3 — No OpenAPI specification for rustcode-server

- **Location**: `rustcode-server/` (no spec file)
- **OpenCode**: `packages/sdk/openapi.json` — auto-generated OpenAPI spec that drives SDK generation.
- **RustCode**: No OpenAPI spec. No SDK generation. The API surface is documented only in code comments.
- **Gap**: Missing contract-first API documentation. No auto-generated client SDK.
- **Consequence**: Consumers must reverse-engineer the API from handler code. No automated client generation.
- **Recommendation**: Generate an OpenAPI 3.0 spec from axum routes using `utoipa` or `aide`. Use it for documentation and client generation.
- **Severity**: **High**

---

## 11. SSE Protocol

### Finding 11.1 — SSE event stream implemented but schema not verified

- **Location**: `rustcode-server/src/routes/api.rs:624-679`
- **OpenCode**: Server-Sent Events on `GET /event` and `GET /api/event` with `server.connected`, `server.heartbeat`, and dynamically-typed event payloads (JSON).
- **RustCode**: Full SSE implementation — `server.connected` on connect, `server.heartbeat` every 10s, directory-filtered event dispatch. Uses `axum::response::sse::Sse` with `KeepAlive`.
- **Gap**: The event data is `serde_json::to_string(&event.payload)` — no schema validation of event payloads against OpenCode's event types. Any JSON can be emitted.
- **Consequence**: Clients may receive malformed events that don't match expected schemas.
- **Recommendation**: Add typed event enums (like `McpEvent`) for all major event types. Validate at publish time, not just serialize-and-send.
- **Severity**: **Medium**

---

## 12. MCP Protocol Implementation

### Finding 12.1 — MCP client supports three transports (local, HTTP, SSE)

- **Location**: `rustcode-core/src/mcp.rs:938-972` (McpClientState), `rustcode-core/src/mcp.rs:1003-1188` (McpClient::connect)
- **OpenCode**: `packages/opencode/src/mcp/index.ts` — supports local stdio and remote StreamableHTTP transports.
- **RustCode**: Full MCP client with `Local` (stdio), `Remote` (HTTP POST/StreamableHTTP), and `RemoteSse` (deprecated SSE) transports. Complete JSON-RPC handshake (`initialize` → `notifications/initialized` → `tools/list` → tool calls).
- **Gap**: The MCP client has both `McpClient::connect()` and `McpClient::connect_http()` — two constructor paths. `connect()` handles both local and remote via `connect_with_fallback()`, while `connect_http()` is specifically for SSE. The `McpTransport` trait in `rustcode-mcp` duplicates this with `StdioTransport` and `HttpTransport`.
- **Consequence**: Dual transport API — `rustcode_core::mcp::McpClient` and `rustcode_mcp::StdioTransport`/`HttpTransport` — creates confusion.
- **Recommendation**: Consolidate transport abstractions into a single location. Either move `McpTransport` trait into `rustcode-core` or remove it and use `McpClient` everywhere.
- **Severity**: **Medium**

### Finding 12.2 — MCP tools correctly integrated with ToolRegistry

- **Location**: `rustcode-mcp/src/lib.rs:527-649` (McpToolExecutor)
- **OpenCode**: MCP tools are registered via `PluginToolDef` in the tool registry, discovered via `tools/list` at connection time.
- **RustCode**: `McpToolExecutor` wraps `McpClient::call_tool()` behind the `Tool` trait. `to_plugin_def()` produces a `PluginToolDef` ready for registry. Sanitizes tool names to avoid collisions (`tool_key()`).
- **Gap**: None significant. The MCP-to-tool bridge is well-designed.
- **Consequence**: MCP tools work transparently with the same execution pipeline as built-in tools.
- **Recommendation**: Add reconnection logic for MCP servers that go offline. Currently, a failed MCP client is not automatically reconnected.
- **Severity**: **Low**

### Finding 12.3 — MCP protocol version hardcoded

- **Location**: `rustcode-core/src/mcp.rs:1059-1068` (`"protocolVersion": "2024-11-05"`), `rustcode-mcp/src/lib.rs:185-193` (same)
- **OpenCode**: Same protocol version hardcoded.
- **RustCode**: Protocol version `"2024-11-05"` is hardcoded in two places (core and mcp crate).
- **Gap**: Protocol version is duplicated across crates.
- **Consequence**: Update requires changing two files. Risk of mismatch.
- **Recommendation**: Define `const MCP_PROTOCOL_VERSION: &str = "2024-11-05"` in `rustcode_core::mcp` and reference it from both places.
- **Severity**: **Low**

---

## 13. LSP Protocol Implementation

### Finding 13.1 — Extensive LSP implementation with known server catalog

- **Location**: `rustcode-lsp/src/lib.rs:1-1383+` (very large file)
- **OpenCode**: `packages/opencode/src/lsp/client.ts` (line ~970), `packages/opencode/src/lsp/server.ts`
- **RustCode**: Comprehensive LSP client with: ~35 known server definitions (rust-analyzer, typescript-language-server, gopls, clangd, pyright, etc.), dynamic root detection (NearestRoot/StrictNearestRoot), workspace auto-detection via config file scanning, full JSON-RPC framing (`Content-Length` header), initialize handshake, shutdown sequence, background stdout reader with `dispatch_message()` for notifications, server requests (`workspace/configuration`, `client/registerCapability`), pull diagnostics, full diagnostic caching with version tracking.
- **Gap**: The LSP client is the most complete RustCode module. However, it's a single monolithic file (~2000+ lines).
- **Consequence**: Functionally comprehensive but hard to maintain. Single-file `lib.rs` should be split into modules.
- **Recommendation**: Split into `client.rs`, `manager.rs`, `framing.rs`, `catalog.rs`, `discovery.rs`. Use `pub(crate)` within the crate.
- **Severity**: **Medium**

### Finding 13.2 — LSP errors defined locally instead of using core errors

- **Location**: `rustcode-lsp/src/lib.rs:48-113` (LspError enum)
- **OpenCode**: LSP errors are part of the unified error hierarchy.
- **RustCode**: `rustcode-lsp` defines its own `LspError` enum (10 variants) with `Result<T>` type alias. Does NOT use `rustcode_core::error::Error`.
- **Gap**: Third error type in the ecosystem (core `Error`, server `ServerError`, lsp `LspError`). No conversion between them.
- **Consequence**: LSP callers must handle a separate error type. Cannot use `?` to propagate from LSP code to session code.
- **Recommendation**: Remove `LspError` and use `rustcode_core::error::Error` with an `Lsp` variant. Implement `From<LspError> for Error`.
- **Severity**: **High**

---

## 14. SDK/Client Libraries

### Finding 14.1 — OpenCode has `@opencode-ai/sdk`; RustCode has no SDK crate

- **Location**: `packages/sdk/js/package.json` (npm: `@opencode-ai/sdk@1.17.8`)
- **OpenCode**: Publishes `@opencode-ai/sdk` with: auto-generated REST client from OpenAPI spec (`src/gen/client/`), typed V2 API surface (`src/v2/client.ts`, `src/v2/server.ts`), lifecycle helpers (`createOpencode()`, `createOpencodeServer()`, `createOpencodeClient()`), server launcher with timeout/abort signal, error interceptor for structured error handling.
- **RustCode**: No equivalent SDK crate. The `rustcode-server` binary can be used as a subprocess, but there's no client library for programmatic Rust usage. No `rustcode-client` crate. No auto-generated client from any API spec.
- **Gap**: **Critical gap.** Rust consumers must either use the server crate directly (embedding axum state), shell out to the binary, or write their own HTTP client. The SDK approach is fundamentally different — OpenCode provides a thin client library; RustCode provides none.
- **Consequence**: RustCode has no "nice" programmatic API for Rust consumers. The audience for a Rust library is limited to embedding the crate directly.
- **Recommendation**: Create a `rustcode-client` crate that provides a typed async HTTP client for the REST API, similar to `@opencode-ai/sdk/client`. Use `reqwest` and mirror the SDK's V2 typed methods. Consider generating the client from an OpenAPI spec (see Finding 10.3).
- **Severity**: **Critical**

### Finding 14.2 — OpenCode SDK is generated; RustCode is hand-written

- **Location**: `packages/sdk/js/src/gen/` (auto-generated from OpenAPI spec via `@hey-api/openapi-ts`)
- **OpenCode**: SDK types and client are **auto-generated** from `packages/sdk/openapi.json`. The `./script/build.ts` regenerates them. This ensures the SDK is always in sync with the server API.
- **RustCode**: All types in `rustcode-core` are **hand-written** ports from TypeScript. No code generation. No shared spec.
- **Gap**: Manual porting creates drift. As OpenCode evolves, RustCode types must be manually updated. No spec-driven synchronization.
- **Consequence**: RustCode will fall behind OpenCode's schema changes without dedicated maintenance. Type mismatches will accumulate.
- **Recommendation**: Generate Rust types from the OpenAPI spec or TypeScript source. Use `npm run build:sdk` → capture output → generate Rust structs with derive macros. At minimum, add a schema validation test that fetches the latest OpenCode SDK types and compares them.
- **Severity**: **Critical**

---

## Summary: Severity Distribution

| Severity  | Count | Key Issues |
|-----------|-------|------------|
| Critical  | 4     | No versioning (3.1), stub server handlers (10.1), no SDK crate (14.1), hand-written types drift (14.2) |
| High      | 6     | No API firewall (1.1, 4.1), serde naming (2.1), string IDs (7.1), no OpenAPI spec (10.3), duplicate errors (6.2, 13.2) |
| Medium    | 7     | `Option<Option<T>>` (2.2), Tool trait dual-schema (5.2), fragmented plugins (5.3), `TaggedString` unused (7.2), MCP transport ambiguity (12.1), LSP monolithic (13.1), V2 config dead code (8.2) |
| Low       | 4     | MCP re-exports (1.2), `v2_schema.rs` naming (9.1), route patterns (10.2), protocol version hardcoded (12.3) |
| Info      | 3     | Provider trait solid (5.1), error hierarchy quality (6.1), JSON Schema good (8.1) |

---

## Overall Assessment

RustCode's API surface is **functionally comprehensive** but **structurally under-disciplined**. The most critical gap is the **SDK/client library absence** — RustCode offers no programmatic client API for Rust consumers, unlike OpenCode's well-typed `@opencode-ai/sdk`. The second-most critical issue is the **lack of API versioning and compatibility guarantees**, making the crate unsafe to depend on.

**Strengths**:
- Error type hierarchy is excellent (50+ detailed variants, sub-enums, good docs)
- MCP client implementation is complete and well-structured (three transports, full JSON-RPC)
- LSP client is the most feature-complete module (35+ server definitions, diagnostics, pull/push)
- Tool registry architecture mirrors OpenCode faithfully (PluginToolDef, ToolInfo, LLM definitions)
- JSON Schema normalization is a near-perfect port

**Weaknesses**:
- Zero API firewall — everything is `pub`, no `pub(crate)` discipline
- No SDK/client crate for programmatic Rust consumers
- No versioning or deprecation policy
- All IDs are `String` aliases — no type safety
- Server handlers are stubs — not production-ready
- Hand-written types will inevitably drift from OpenCode's evolved schemas

**Top 3 recommendations** (if only 3 could be done):
1. Create `rustcode-client` crate with typed async HTTP client
2. Add newtype wrappers for all ID types (`SessionId`, `MessageId`, `ModelId`, etc.)
3. Implement semver policy + `cargo-semver-checks` in CI + `pub(crate)` audit
