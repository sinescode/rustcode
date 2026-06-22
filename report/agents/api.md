# BlazeCode vs BlazeCode: API Analysis Report

**Agent**: Agent 09 — API Agent  
**Date**: 2026-06-21  
**Scope**: Public & internal APIs across all crates

---

## 1. Public API Surface

### Finding 1.1 — BlazeCode `pub` on every module (no API firewall)

- **Location**: `blazecode-core/src/lib.rs:11-95`
- **BlazeCode**: Uses Effect.ts `Context.Tag`, explicit service interfaces, and `export`-controlled module boundaries. Only intended surfaces are exported from packages. `packages/sdk/js/` has a tightly controlled re-export surface (`index.ts`, `client.ts`, `server.ts`, `v2/`).
- **BlazeCode**: `pub mod` for **all 95 modules** — every module is public with no sub-visibility. `pub use` re-exports are minimal (e.g. `blazecode-server/src/lib.rs:31-33` exports only 3 items). The crate has no internal `pub(crate)` module discipline — everything is world-visible.
- **Gap**: BlazeCode has no API firewall. Every internal helper, stub, and skeleton module is `pub`. BlazeCode's SDK is a purpose-built, minimal API surface.
- **Consequence**: Breaking changes can impact downstream consumers of any module. Documentation generation includes all internal modules. Users cannot distinguish "what's intended for me" vs "internal implementation detail."
- **Recommendation**: Add `#[doc(hidden)]` to internal modules. Restructure with a `_internal` pattern or conditional compilation. Adopt `pub(crate)` as default, promoting to `pub` only for the public API boundary.
- **Severity**: **High**

### Finding 1.2 — blazecode-mcp re-exports from blazecode-core

- **Location**: `blazecode-mcp/src/lib.rs:44-48`
- **BlazeCode**: The MCP SDK (`@modelcontextprotocol/sdk`) is a separate dependency. BlazeCode's MCP logic in `packages/blazecode/src/mcp/` uses the SDK but does not re-export it.
- **BlazeCode**: `blazecode-mcp` re-exports ~12 types from `blazecode_core::mcp`. This creates a dual-surface problem — MCP types can be accessed via either crate.
- **Gap**: Dual public API surface for MCP types creates confusion about which crate is authoritative.
- **Consequence**: Users who depend on both `blazecode_core` and `blazecode-mcp` get redundant type paths.
- **Recommendation**: Re-exporting is fine for convenience, but `blazecode-core::mcp` should be the canonical definition. Document that `blazecode-mcp` re-exports are convenience aliases.
- **Severity**: **Low**

---

## 2. API Consistency

### Finding 2.1 — Naming convention: camelCase vs snake_case in serde

- **Location**: `blazecode-core/src/provider.rs:298-299` (`providerID`, `modelID`), `blazecode-core/src/config.rs:99` (`logLevel`)
- **BlazeCode**: Consistent camelCase in JSON wire format (standard JS convention).
- **BlazeCode**: Mixed conventions — most serde `rename` attributes use `camelCase`, but some use `snake_case`. Conditionally inconsistent within the same crate (`config.rs` uses `rename_all = "camelCase"` on some structs, default snake_case on others).
- **Gap**: Some Rust structs fail to match BlazeCode's JSON wire format because serde rename attributes are missing or inconsistent.
- **Consequence**: Clients receiving JSON from BlazeCode's server may get `provider_id` instead of the expected `providerID`, breaking compatibility with BlazeCode SDK clients.
- **Recommendation**: Audit all `Serialize`/`Deserialize` structs against BlazeCode TS types. Add `#[serde(rename_all = "camelCase")]` consistently. Add roundtrip tests that verify parity with TS snapshot outputs.
- **Severity**: **High**

### Finding 2.2 — `Option<Option<T>>` pattern in SessionPatch

- **Location**: `blazecode-core/src/session.rs:1496-1509`
- **BlazeCode**: Uses `undefined` vs `null` distinction at the JavaScript level to signal "unset" vs "set to null." The TS Effect.Schema handles this with `optional` and `Schema.optional(Schema, { exact: true, as: "Option" })`.
- **BlazeCode**: `SessionPatch` uses `Option<Option<T>>` to distinguish "don't update" (`None`) from "set to null" (`Some(None)`). This is technically correct but confusing.
- **Gap**: The `Option<Option<T>>` pattern is non-idiomatic Rust. No other patch type in the codebase uses it consistently.
- **Consequence**: Maintainability burden — developers must remember the double-option semantics.
- **Recommendation**: Introduce a `Patch<T>` newtype with clear semantics: `Patch::Unset`, `Patch::SetToNull`, `Patch::Set(T)`. Document the pattern once and reuse.
- **Severity**: **Medium**

---

## 3. API Stability

### Finding 3.1 — No versioning strategy or deprecation policy

- **Location**: All crates' `Cargo.toml` (version `0.1.0`)
- **BlazeCode**: NPM packages use semver (`@blazecode-ai/sdk@1.17.8`). The TS source has a `V2_SESSION` feature flag and coexisting V1/V2 paths. Breaking changes are managed through SDK generation (`@hey-api/openapi-ts`).
- **BlazeCode**: All crates at `0.1.0` with no versioning policy documented. No changelog or migration guide.
- **Gap**: No semver discipline. No `#[deprecated]` annotations. No public API compatibility testing.
- **Consequence**: Any change could be breaking. Downstream consumers cannot safely depend on any specific API surface.
- **Recommendation**: Establish semver policy. Add `#![warn(deprecated_safe)]`. Create an API compat test suite. Use `cargo-semver-checks` in CI.
- **Severity**: **Critical**

---

## 4. Internal vs External APIs

### Finding 4.1 — No `pub(crate)` discipline

- **Location**: `blazecode-core/src/lib.rs:11-95` (all 95 modules `pub`)
- **BlazeCode**: TS uses `_internal.ts` / `internal/` directories and explicit export lists. Effect.ts services are accessed through `Context.Tag`.
- **BlazeCode**: Every module is `pub` regardless of whether it's an internal implementation detail. For example, `database.rs`, `flock.rs`, `ripgrep.rs` are likely implementation details but are all `pub mod`.
- **Gap**: No internal module hiding. Everything is externally accessible.
- **Consequence**: Impossible to refactor internals without potentially breaking external consumers. Leaks implementation details into docs.
- **Recommendation**: Audit each module. Modules that are not part of the public API should be `pub(crate)`. Only expose through controlled re-exports in `lib.rs`.
- **Severity**: **High**

---

## 5. Trait Design

### Finding 5.1 — Provider trait is coherent and minimal

- **Location**: `blazecode-core/src/provider.rs:907-940`
- **BlazeCode**: `packages/llm/src/provider.ts` — the Vercel AI SDK's `LanguageModelV1` interface with `doStream()`, `doGenerate()`.
- **BlazeCode**: `Provider` trait has 5 methods: `provider_id()`, `npm()`, `list_models()`, `get_model()`, `stream()`, `complete()`. Well-balanced.
- **Gap**: The `stream()` return type uses `Box<dyn futures::Stream<Item = Result<LlmEvent>> + Send + Unpin>` — complex but necessary for async trait dispatch.
- **Consequence**: The trait is abstract enough for generic provider implementations but concrete enough to be useful.
- **Recommendation**: Consider making `stream` and `complete` default methods where one delegates to the other. No urgent fix needed.
- **Severity**: **Info**

### Finding 5.2 — Tool trait has a design tension: schema vs LLM definition

- **Location**: `blazecode-core/src/tool.rs:163-201`
- **BlazeCode**: `packages/blazecode/src/tool/tool.ts` — `Tool` interface with `schema`, `execute`, and separate LLM-facing `definition`.
- **BlazeCode**: `Tool` trait has both `json_schema()` (optional, what LLM sees) and `parameters_schema()` (always present). This dual-schema pattern is confusing — the relationship between them is undocumented.
- **Gap**: Two schema methods with unclear semantics. `json_schema()` returns `Option<Value>`, `parameters_schema()` returns `Value` unconditionally. When do they differ? Why?
- **Consequence**: Tool implementors are uncertain which schema to provide. Might lead to LLM seeing a different schema than the validator uses.
- **Recommendation**: Either merge into one method or add clear doc comments explaining the distinction (one is for LLM consumption, one for server-side validation). Better: use `parameters_schema()` everywhere and remove `json_schema()`.
- **Severity**: **Medium**

### Finding 5.3 — Plugin trait hierarchy is complex (V1 + V2 + Provider)

- **Location**: `blazecode-core/src/plugin.rs:80-115` (ProviderPlugin), `blazecode-core/src/plugin.rs:784-871` (PluginHooks), `blazecode-core/src/plugin.rs:1231-1340` (PluginV2Handler)
- **BlazeCode**: `packages/core/src/plugin.ts` — `PluginV2.define()` with hook-type dispatching. A single unified plugin definition.
- **BlazeCode**: Three separate plugin concepts: `ProviderPlugin` (provider-level hooks), `PluginHooks` (V1 lifecycle hooks with 18 methods), `PluginV2Handler` (V2 hooks with 3 methods). Each has a separate registry.
- **Gap**: Fragmented plugin architecture. Provider plugins and full plugins are separate traits with separate registries, unlike BlazeCode's unified `PluginV2.define()`.
- **Consequence**: Plugins that need both provider customization and lifecycle hooks must implement two traits and register with two registries.
- **Recommendation**: Consider merging into a single unified `Plugin` trait with optional hooks (default methods returning `None`/no-op), similar to BlazeCode's `PluginV2` approach.
- **Severity**: **Medium**

---

## 6. Error Types

### Finding 6.1 — Comprehensive error hierarchy with good documentation

- **Location**: `blazecode-core/src/error.rs:1-1315`
- **BlazeCode**: Effect.ts provides ~120+ `Schema.TaggedErrorClass` types. Rich structured errors with classification, retry hints, and HTTP context.
- **BlazeCode**: `Error` enum with ~50+ variants organized by domain. `LlmErrorReason` sub-enum with 10 variants. `PermissionError`, `WorktreeError`, `ImageError`, `SkillError` sub-enums. `ApiError` for HTTP layer. All with `thiserror::Error` derive and `#[error("...")]` format strings.
- **Gap**: None significant. The error hierarchy is one of the best-designed parts of BlazeCode. All variants have clear doc comments with source references.
- **Consequence**: Errors are matchable, displayable, and source-referenced. Excellent.
- **Recommendation**: Add `#[source]` annotations for nested errors (e.g., `Error::Io` could show the inner source). Ensure all `From` impls exist for the sub-enum types.
- **Severity**: **Info**

### Finding 6.2 — ServerError duplicates core Error

- **Location**: `blazecode-core/src/error.rs:613-650` (ApiError), `blazecode-server/src/error.rs:19-243` (ServerError)
- **BlazeCode**: Single error hierarchy; server layer maps domain errors to HTTP responses.
- **BlazeCode**: Two separate error enums: `ApiError` in core (8 variants) and `ServerError` in server (18+ variants). They overlap significantly (both have `NotFound`, `InvalidRequest` equivalents).
- **Gap**: Duplicate error types with overlapping semantics. No documented conversion path between them (`IntoServerError` trait exists but is empty `blazecode-server/src/error.rs:336-338`).
- **Consequence**: Error mapping from core to server is ad-hoc and inconsistent. Some handlers use `ServerError::unknown(e.to_string())` which loses structured error data.
- **Recommendation**: Remove `ApiError` from core and use `ServerError` exclusively in the server crate. Implement proper `From<blazecode_core::error::Error>` for `ServerError`.
- **Severity**: **High**

---

## 7. Type Safety

### Finding 7.1 — Stringly-typed IDs

- **Location**: `blazecode-core/src/provider.rs:24-48` (`ModelId = String`, `ProviderId = String`, etc.) and `blazecode-core/src/session.rs:83-90` (`SessionId = String`, `MessageId = String`, `PartId = String`)
- **BlazeCode**: TypeScript uses branded string types (`type ModelId = string & { readonly __brand: "ModelId" }`). The Effect schema provides runtime validation.
- **BlazeCode**: All IDs are `pub type X = String;` type aliases — no compile-time type safety. You can pass a `SessionId` where a `MessageId` is expected with no compiler error.
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

- **Location**: `blazecode-core/src/schema.rs:304-306`
- **BlazeCode**: Effect.Schema's `Newtype<Tag>()` factory for branded types.
- **BlazeCode**: `TaggedString` exists but is unused — it wraps a `String` with a const-generic tag... except `TaggedString` has no const generic. It's just `pub struct TaggedString(pub String);` — indistinguishable from a bare `String`.
- **Gap**: The `TaggedString` type is supposed to be a generic newtype factory but lacks the const-generic parameter that would make different tags different types.
- **Consequence**: Unused, broken abstraction. The branded type system promised by `schema.rs` is not actually delivered.
- **Recommendation**: Either implement the const-generic newtype properly or remove it. The newtype approach in Finding 7.1 is more practical.
- **Severity**: **Medium**

---

## 8. Serialization Contracts

### Finding 8.1 — JSON Schema for tool definitions is well-implemented

- **Location**: `blazecode-core/src/tool.rs:868-1154`
- **BlazeCode**: `packages/blazecode/src/tool/json-schema.ts` — same normalization, `$ref` inlining, `allOf` flattening, null stripping.
- **BlazeCode**: Complete port of the JSON Schema normalization pipeline: `normalize_json_schema()`, `inline_local_refs()`, `drop_defs_if_resolved()`. Has comprehensive logic for handling edge cases (anyOf unwrapping, integer bounds, null stripping).
- **Gap**: None significant. The schema normalization is a faithful port with good test coverage.
- **Consequence**: Tools produce LLM-compatible JSON schemas identical to BlazeCode.
- **Recommendation**: Add property-based testing (proptest) for JSON Schema normalization to catch edge cases.
- **Severity**: **Info**

### Finding 8.2 — Config schema has dual V1/V2 layout

- **Location**: `blazecode-core/src/config.rs:89-240` (V1 `Info`), `blazecode-core/src/config.rs:299-350` (V2 `V2ConfigInfo`)
- **BlazeCode**: `packages/core/src/v1/config/config.ts` + migration from V1 → V2. Separate schemas with migration path.
- **BlazeCode**: Two complete struct hierarchies (`Info` + sub-types; `V2ConfigInfo` + sub-types). Both coexist in the same module.
- **Gap**: The V1 → V2 migration logic is not implemented. `V2ConfigInfo` is defined but never used in production paths. No `migrate()` function.
- **Consequence**: Config loading uses V1 format only. V2 config is dead code.
- **Recommendation**: Either implement migration or remove the dead V2 schema types. Add a `migrate_v1_to_v2()` function if both are needed.
- **Severity**: **Medium**

---

## 9. API Versioning

### Finding 9.1 — V1 and V2 session representations coexist

- **Location**: `blazecode-core/src/session.rs` (V1 message/part model), `blazecode-core/src/v2_schema.rs` (V2 timestamp helpers)
- **BlazeCode**: V2 session core (`packages/core/src/session-v2/` and `packages/blazecode/src/session/`) with clear separation. V1 deprecated but supported.
- **BlazeCode**: Single `session.rs` implements the V2-style data model (Message, Part, SessionProcessor). No V1 session model exists. `v2_schema.rs` only has DateTime helpers.
- **Gap**: The session data model is already V2-native (which is good), but the `v2_schema.rs` module is misleading — it only provides serde helpers, not a V2 feature set.
- **Consequence**: Less of a gap; BlazeCode correctly jumped straight to V2 session model.
- **Recommendation**: Rename `v2_schema.rs` to `datetime.rs` or `serde_helpers.rs` to avoid confusion. Document that the session module is V2-native.
- **Severity**: **Low**

---

## 10. REST API Design

### Finding 10.1 — Route structure mirrors BlazeCode but is incomplete

- **Location**: `blazecode-server/src/routes/api.rs:116-181`
- **BlazeCode**: Server routes are split across `packages/server/src/api.ts` + 30+ group files under `routes/groups/`. All supported in production.
- **BlazeCode**: Single `api.rs` file with 25+ routes under `/api/`. Handlers are stubs or simplified — `api_session_prompt`, `api_session_compact`, `api_session_wait` are minimal stubs. `api_fs` reads files directly (no permission checks).
- **Gap**: Handlers lack real implementation. Many return placeholder data (`api_session_compact` returns `NO_CONTENT`; `api_session_wait` returns `NO_CONTENT`). No authentication middleware in the server crate.
- **Consequence**: The server crate cannot serve as a real backend. SSE event streaming works but session prompt execution is stub-only.
- **Recommendation**: Document which routes are real vs stubs. Implement at minimum the session CRUD + prompt execution paths. Add auth middleware.
- **Severity**: **Critical**

### Finding 10.2 — Route URL patterns are RESTful

- **Location**: `blazecode-server/src/routes/api.rs:116-181`
- **BlazeCode**: Same pattern — `/api/session/:sessionID`, `/api/session/:sessionID/prompt`, etc.
- **BlazeCode**: Uses `{sessionID}` (axum 0.8 path param syntax). Consistent URL patterns. Response formats match BlazeCode's JSON structure.
- **Gap**: None in the route definitions themselves. The URL structure is correct.
- **Consequence**: API surface is wire-compatible with BlazeCode clients.
- **Recommendation**: Add integration tests that validate response shapes against BlazeCode SDK types.
- **Severity**: **Low**

### Finding 10.3 — No OpenAPI specification for blazecode-server

- **Location**: `blazecode-server/` (no spec file)
- **BlazeCode**: `packages/sdk/openapi.json` — auto-generated OpenAPI spec that drives SDK generation.
- **BlazeCode**: No OpenAPI spec. No SDK generation. The API surface is documented only in code comments.
- **Gap**: Missing contract-first API documentation. No auto-generated client SDK.
- **Consequence**: Consumers must reverse-engineer the API from handler code. No automated client generation.
- **Recommendation**: Generate an OpenAPI 3.0 spec from axum routes using `utoipa` or `aide`. Use it for documentation and client generation.
- **Severity**: **High**

---

## 11. SSE Protocol

### Finding 11.1 — SSE event stream implemented but schema not verified

- **Location**: `blazecode-server/src/routes/api.rs:624-679`
- **BlazeCode**: Server-Sent Events on `GET /event` and `GET /api/event` with `server.connected`, `server.heartbeat`, and dynamically-typed event payloads (JSON).
- **BlazeCode**: Full SSE implementation — `server.connected` on connect, `server.heartbeat` every 10s, directory-filtered event dispatch. Uses `axum::response::sse::Sse` with `KeepAlive`.
- **Gap**: The event data is `serde_json::to_string(&event.payload)` — no schema validation of event payloads against BlazeCode's event types. Any JSON can be emitted.
- **Consequence**: Clients may receive malformed events that don't match expected schemas.
- **Recommendation**: Add typed event enums (like `McpEvent`) for all major event types. Validate at publish time, not just serialize-and-send.
- **Severity**: **Medium**

---

## 12. MCP Protocol Implementation

### Finding 12.1 — MCP client supports three transports (local, HTTP, SSE)

- **Location**: `blazecode-core/src/mcp.rs:938-972` (McpClientState), `blazecode-core/src/mcp.rs:1003-1188` (McpClient::connect)
- **BlazeCode**: `packages/blazecode/src/mcp/index.ts` — supports local stdio and remote StreamableHTTP transports.
- **BlazeCode**: Full MCP client with `Local` (stdio), `Remote` (HTTP POST/StreamableHTTP), and `RemoteSse` (deprecated SSE) transports. Complete JSON-RPC handshake (`initialize` → `notifications/initialized` → `tools/list` → tool calls).
- **Gap**: The MCP client has both `McpClient::connect()` and `McpClient::connect_http()` — two constructor paths. `connect()` handles both local and remote via `connect_with_fallback()`, while `connect_http()` is specifically for SSE. The `McpTransport` trait in `blazecode-mcp` duplicates this with `StdioTransport` and `HttpTransport`.
- **Consequence**: Dual transport API — `blazecode_core::mcp::McpClient` and `blazecode_mcp::StdioTransport`/`HttpTransport` — creates confusion.
- **Recommendation**: Consolidate transport abstractions into a single location. Either move `McpTransport` trait into `blazecode-core` or remove it and use `McpClient` everywhere.
- **Severity**: **Medium**

### Finding 12.2 — MCP tools correctly integrated with ToolRegistry

- **Location**: `blazecode-mcp/src/lib.rs:527-649` (McpToolExecutor)
- **BlazeCode**: MCP tools are registered via `PluginToolDef` in the tool registry, discovered via `tools/list` at connection time.
- **BlazeCode**: `McpToolExecutor` wraps `McpClient::call_tool()` behind the `Tool` trait. `to_plugin_def()` produces a `PluginToolDef` ready for registry. Sanitizes tool names to avoid collisions (`tool_key()`).
- **Gap**: None significant. The MCP-to-tool bridge is well-designed.
- **Consequence**: MCP tools work transparently with the same execution pipeline as built-in tools.
- **Recommendation**: Add reconnection logic for MCP servers that go offline. Currently, a failed MCP client is not automatically reconnected.
- **Severity**: **Low**

### Finding 12.3 — MCP protocol version hardcoded

- **Location**: `blazecode-core/src/mcp.rs:1059-1068` (`"protocolVersion": "2024-11-05"`), `blazecode-mcp/src/lib.rs:185-193` (same)
- **BlazeCode**: Same protocol version hardcoded.
- **BlazeCode**: Protocol version `"2024-11-05"` is hardcoded in two places (core and mcp crate).
- **Gap**: Protocol version is duplicated across crates.
- **Consequence**: Update requires changing two files. Risk of mismatch.
- **Recommendation**: Define `const MCP_PROTOCOL_VERSION: &str = "2024-11-05"` in `blazecode_core::mcp` and reference it from both places.
- **Severity**: **Low**

---

## 13. LSP Protocol Implementation

### Finding 13.1 — Extensive LSP implementation with known server catalog

- **Location**: `blazecode-lsp/src/lib.rs:1-1383+` (very large file)
- **BlazeCode**: `packages/blazecode/src/lsp/client.ts` (line ~970), `packages/blazecode/src/lsp/server.ts`
- **BlazeCode**: Comprehensive LSP client with: ~35 known server definitions (rust-analyzer, typescript-language-server, gopls, clangd, pyright, etc.), dynamic root detection (NearestRoot/StrictNearestRoot), workspace auto-detection via config file scanning, full JSON-RPC framing (`Content-Length` header), initialize handshake, shutdown sequence, background stdout reader with `dispatch_message()` for notifications, server requests (`workspace/configuration`, `client/registerCapability`), pull diagnostics, full diagnostic caching with version tracking.
- **Gap**: The LSP client is the most complete BlazeCode module. However, it's a single monolithic file (~2000+ lines).
- **Consequence**: Functionally comprehensive but hard to maintain. Single-file `lib.rs` should be split into modules.
- **Recommendation**: Split into `client.rs`, `manager.rs`, `framing.rs`, `catalog.rs`, `discovery.rs`. Use `pub(crate)` within the crate.
- **Severity**: **Medium**

### Finding 13.2 — LSP errors defined locally instead of using core errors

- **Location**: `blazecode-lsp/src/lib.rs:48-113` (LspError enum)
- **BlazeCode**: LSP errors are part of the unified error hierarchy.
- **BlazeCode**: `blazecode-lsp` defines its own `LspError` enum (10 variants) with `Result<T>` type alias. Does NOT use `blazecode_core::error::Error`.
- **Gap**: Third error type in the ecosystem (core `Error`, server `ServerError`, lsp `LspError`). No conversion between them.
- **Consequence**: LSP callers must handle a separate error type. Cannot use `?` to propagate from LSP code to session code.
- **Recommendation**: Remove `LspError` and use `blazecode_core::error::Error` with an `Lsp` variant. Implement `From<LspError> for Error`.
- **Severity**: **High**

---

## 14. SDK/Client Libraries

### Finding 14.1 — BlazeCode has `@blazecode-ai/sdk`; BlazeCode has no SDK crate

- **Location**: `packages/sdk/js/package.json` (npm: `@blazecode-ai/sdk@1.17.8`)
- **BlazeCode**: Publishes `@blazecode-ai/sdk` with: auto-generated REST client from OpenAPI spec (`src/gen/client/`), typed V2 API surface (`src/v2/client.ts`, `src/v2/server.ts`), lifecycle helpers (`createBlazecode()`, `createBlazecodeServer()`, `createBlazecodeClient()`), server launcher with timeout/abort signal, error interceptor for structured error handling.
- **BlazeCode**: No equivalent SDK crate. The `blazecode-server` binary can be used as a subprocess, but there's no client library for programmatic Rust usage. No `blazecode-client` crate. No auto-generated client from any API spec.
- **Gap**: **Critical gap.** Rust consumers must either use the server crate directly (embedding axum state), shell out to the binary, or write their own HTTP client. The SDK approach is fundamentally different — BlazeCode provides a thin client library; BlazeCode provides none.
- **Consequence**: BlazeCode has no "nice" programmatic API for Rust consumers. The audience for a Rust library is limited to embedding the crate directly.
- **Recommendation**: Create a `blazecode-client` crate that provides a typed async HTTP client for the REST API, similar to `@blazecode-ai/sdk/client`. Use `reqwest` and mirror the SDK's V2 typed methods. Consider generating the client from an OpenAPI spec (see Finding 10.3).
- **Severity**: **Critical**

### Finding 14.2 — BlazeCode SDK is generated; BlazeCode is hand-written

- **Location**: `packages/sdk/js/src/gen/` (auto-generated from OpenAPI spec via `@hey-api/openapi-ts`)
- **BlazeCode**: SDK types and client are **auto-generated** from `packages/sdk/openapi.json`. The `./script/build.ts` regenerates them. This ensures the SDK is always in sync with the server API.
- **BlazeCode**: All types in `blazecode-core` are **hand-written** ports from TypeScript. No code generation. No shared spec.
- **Gap**: Manual porting creates drift. As BlazeCode evolves, BlazeCode types must be manually updated. No spec-driven synchronization.
- **Consequence**: BlazeCode will fall behind BlazeCode's schema changes without dedicated maintenance. Type mismatches will accumulate.
- **Recommendation**: Generate Rust types from the OpenAPI spec or TypeScript source. Use `npm run build:sdk` → capture output → generate Rust structs with derive macros. At minimum, add a schema validation test that fetches the latest BlazeCode SDK types and compares them.
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

BlazeCode's API surface is **functionally comprehensive** but **structurally under-disciplined**. The most critical gap is the **SDK/client library absence** — BlazeCode offers no programmatic client API for Rust consumers, unlike BlazeCode's well-typed `@blazecode-ai/sdk`. The second-most critical issue is the **lack of API versioning and compatibility guarantees**, making the crate unsafe to depend on.

**Strengths**:
- Error type hierarchy is excellent (50+ detailed variants, sub-enums, good docs)
- MCP client implementation is complete and well-structured (three transports, full JSON-RPC)
- LSP client is the most feature-complete module (35+ server definitions, diagnostics, pull/push)
- Tool registry architecture mirrors BlazeCode faithfully (PluginToolDef, ToolInfo, LLM definitions)
- JSON Schema normalization is a near-perfect port

**Weaknesses**:
- Zero API firewall — everything is `pub`, no `pub(crate)` discipline
- No SDK/client crate for programmatic Rust consumers
- No versioning or deprecation policy
- All IDs are `String` aliases — no type safety
- Server handlers are stubs — not production-ready
- Hand-written types will inevitably drift from BlazeCode's evolved schemas

**Top 3 recommendations** (if only 3 could be done):
1. Create `blazecode-client` crate with typed async HTTP client
2. Add newtype wrappers for all ID types (`SessionId`, `MessageId`, `ModelId`, etc.)
3. Implement semver policy + `cargo-semver-checks` in CI + `pub(crate)` audit
