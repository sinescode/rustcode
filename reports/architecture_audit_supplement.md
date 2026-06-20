# Architecture Audit Supplement: Per-Crate Deep Dive

## Part A: Detailed Per-Crate Analysis

### A.1 rustcode-core — Deep Dive

**Location:** `/root/opencodesport/rustcode/crates/rustcode-core/`
**Cargo.toml:** 39 dependencies (all workspace dependencies)
**Source files:** 78+ .rs files  
**Estimated total lines:** ~50,000+

#### A.1.1 Dependency Analysis

All 39 workspace dependencies are pulled into rustcode-core. Here is the full categorized list:

**Async Runtime & Concurrency:**
- `tokio` (full) — async runtime, IO, sync primitives
- `futures` — Stream, Future combinators
- `tokio-stream` — Stream wrappers for tokio types
- `tokio-util` (rt) — Abortable, CancellationToken
- `pin-project-lite` — Safe pin projections
- `dashmap` — Concurrent HashMap

**Serialization:**
- `serde` (derive) — Serialization framework
- `serde_json` — JSON format
- `serde_yaml` — YAML format
- `toml` — TOML format
- `schemars` — JSON Schema generation

**Networking:**
- `reqwest` (json, stream, rustls-tls) — HTTP client for LLM APIs
- `url` — URL parsing
- `base64` — Base64 encoding
- `bytes` — Zero-copy byte buffers

**Database:**
- `sqlx` (runtime-tokio, sqlite, json, derive) — SQLite async driver

**Filesystem:**
- `dirs` — Platform-specific directories
- `glob` — Glob pattern matching
- `ignore` — .gitignore-aware file walking
- `tempfile` — Temporary files
- `shlex` — Shell-like string splitting

**Error Handling:**
- `thiserror` — Derive Error trait
- `anyhow` — Flexible error handling

**Observability:**
- `tracing` — Logging/tracing framework
- `tracing-subscriber` (env-filter, json) — Log subscriber

**CLI:**
- `clap` (derive) — Argument parsing (NOT directly used in core, but in workspace deps)

**Time & Identity:**
- `chrono` (serde) — Date/time
- `uuid` (v4, serde) — UUID generation
- `rand` — Random number generation

**Crypto:**
- `sha2` — SHA-256 hashing
- `hex` — Hex encoding

**Text Processing:**
- `regex` — Regular expressions
- `similar` — Text diffing

**Async Abstractions:**
- `async-trait` — Async trait methods

#### A.1.2 Module Breakdown

The 78 modules in lib.rs can be grouped into functional areas:

**Cross-cutting Infrastructure (19 modules):**
- `error.rs` (1,197 lines) — Top-level Error enum
- `id.rs` — ID generation (ascending, descending, create)
- `env.rs` — Environment variable wrapper
- `bus.rs` (926 lines) — Event bus (tokio::sync::broadcast wrapper)
- `event.rs` — Event types (partial - event sourcing not implemented)
- `flag.rs` — Feature flags
- `state.rs` — Global state
- `format.rs` — Token/cost formatting
- `observability.rs` — Stub
- `runtime.rs` — Runtime initialization
- `global.rs` — Global path config
- `schema.rs` — Shared schemas
- `v2_schema.rs` — V2 schema types
- `catalog.rs` — Model catalog
- `policy.rs` — Policy definitions
- `process.rs` — Process spawning
- `fs_util.rs` — Filesystem utilities
- `ripgrep.rs` — Code search integration
- `credential.rs` — Credential management

**Configuration (4 modules):**
- `config.rs` (2,449 lines) — Main configuration system
- `instruction_context.rs` — Instruction context
- `system_context.rs` — System context (V2)
- `reference.rs` — Code references

**Database & Storage (3 modules):**
- `database.rs` (2,433 lines) — SQLite schema, connection, and service
- `storage.rs` — JSON file storage
- `tool_output_store.rs` — Tool output persistence

**LLM & Providers (20 modules):**
- `provider.rs` (1,911 lines) — Provider trait, types, model definitions
- `model.rs` — Model types
- `aisdk.rs` — AI SDK bridge
- `providers/mod.rs` (145 lines) — Provider auto-detection
- `providers/anthropic.rs` — Anthropic Claude
- `providers/openai.rs` — OpenAI GPT
- `providers/gemini.rs` — Google Gemini
- `providers/azure.rs` — Azure OpenAI
- `providers/bedrock.rs` — AWS Bedrock
- `providers/deepseek.rs` — DeepSeek
- `providers/mistral.rs` — Mistral AI
- `providers/groq.rs` — Groq
- `providers/xai.rs` — xAI Grok
- `providers/openrouter.rs` — OpenRouter
- `providers/openai_compatible.rs` — Generic OpenAI-compatible
- `providers/cloudflare.rs` — Cloudflare AI
- `providers/github_copilot.rs` — GitHub Copilot
- `providers/cerebras.rs` — Cerebras
- `providers/perplexity.rs` — Perplexity
- `providers/cohere.rs` — Cohere
- `providers/fireworks.rs` — Fireworks AI
- `providers/together.rs` — Together AI
- `providers/ai21.rs` — AI21 Labs

**Session System (12 modules):**
- `session.rs` (3,367 lines) — Session manager, types, CRUD
- `session_message.rs` — Message types
- `session_prompt.rs` — Prompt construction
- `session_runner.rs` — Session execution loop
- `session_execution.rs` — Execution orchestration
- `session_compaction.rs` — Context compaction
- `session_history.rs` — History tracking
- `session_info.rs` — Session info queries
- `session_todo.rs` — Todo management
- `session_prompt.rs` — Prompt construction

**Tool System (4 modules):**
- `tool.rs` (996 lines) — Tool trait, registry, execution
- `tool_impls.rs` — Tool implementations (partial)
- `tool_stream.rs` — Tool output streaming
- `tool_output_store.rs` — Output persistence

**Plugin System (1 module):**
- `plugin.rs` (1,112 lines) — Plugin types + partial PluginManager

**MCP (1 module):**
- `mcp.rs` — MCP types (client, registry, config) — shared with rustcode-mcp

**LSP (1 module):**
- `lsp.rs` — LSP types (shared with rustcode-lsp)

**Domain Models (14 modules):**
- `account.rs` — Account management
- `agent.rs` — Agent types
- `background_job.rs` — Background job types (stub)
- `command.rs` — Command definitions
- `file_mutation.rs` — File mutation operations
- `filesystem.rs` — Filesystem abstractions
- `git.rs` — Git operations
- `image.rs` — Image handling
- `integration.rs` — Third-party integrations
- `location.rs` — Location types (V2)
- `npm.rs` — NPM package resolution
- `patch.rs` — Patch/diff types
- `permission.rs` — Permission evaluation
- `project.rs` — Project types
- `pty.rs` — PTY types
- `question.rs` — User question types
- `repository.rs` — Repository types
- `shell.rs` — Shell command execution
- `skill.rs` — Skill definitions
- `snapshot.rs` — Snapshot types
- `sse.rs` — SSE types
- `worktree.rs` — Git worktree management
- `workspace.rs` — Workspace types

#### A.1.3 Key Architecture Deficiencies

1. **No module-level encapsulation** — All 78 mods are `pub mod` with no `pub(crate)` visibility. Internal implementation details are exposed.

2. **Cross-cutting concerns mixed** — `database.rs` contains both SQL table definitions AND connection pooling AND migration logic. These should be separate modules.

3. **Missing integration** — `mcp.rs` and `lsp.rs` define types that are also implemented in `rustcode-mcp` and `rustcode-lsp` crates. This is good for sharing types but the boundaries are unclear.

4. **Stub implementations** — Several modules define types but have minimal functionality: `background_job.rs`, `observability.rs`, `tool_impls.rs`

### A.2 rustcode-server — Deep Dive

**Location:** `/root/opencodesport/rustcode/crates/rustcode-server/`
**Cargo.toml:** 11 dependencies  
**Source files:** 30+ .rs files (lib.rs + 29 route files + sse.rs + cors.rs + server.rs)
**Estimated total lines:** ~3,000+

#### A.2.1 Route Structure

The server exposes two API surfaces:

**Global/Control Routes (unauthenticated):**
- `routes/health.rs` — Health check endpoint
- `routes/control.rs` — Control plane operations
- `routes/control_plane.rs` — Additional control plane
- `routes/config.rs` — Global config retrieval
- `routes/global.rs` — Global operations
- `routes/metadata.rs` — Server metadata

**Instance Routes (workspace-scoped):**
- `routes/session.rs` — Session CRUD and messaging
- `routes/agent.rs` — Agent management
- `routes/command.rs` — Command execution
- `routes/credential.rs` — Credential management
- `routes/event.rs` — Event streaming (SSE)
- `routes/file.rs` — File operations
- `routes/integration.rs` — Third-party integrations
- `routes/mcp.rs` — MCP server management
- `routes/model.rs` — Model listing
- `routes/permission.rs` — Permission management
- `routes/project.rs` — Project operations
- `routes/project_copy.rs` — Project copy
- `routes/provider.rs` — Provider configuration
- `routes/pty.rs` — PTY operations
- `routes/query.rs` — Query operations
- `routes/question.rs` — Question management
- `routes/reference.rs` — Code references
- `routes/skill.rs` — Skill management
- `routes/sync.rs` — Sync operations
- `routes/tui.rs` — TUI-specific routes
- `routes/workspace.rs` — Workspace operations
- `routes/experimental.rs` — Experimental features

#### A.2.2 AppState Analysis

**Location:** `/root/opencodesport/rustcode/crates/rustcode-server/src/server.rs` (Lines 21-67)

```rust
pub struct AppState {
    pub bus: SharedBus,
    pub sessions: Arc<SessionManager>,
    pub tools: Arc<ToolRegistry>,
    pub permissions: Arc<PermissionService>,
    pub questions: Arc<QuestionService>,
    pub runner: Arc<SessionRunner>,
    pub providers: HashMap<String, Arc<dyn Provider>>,
    pub version: String,
    pub start_time: Instant,
    pub agent_service: Option<Arc<AgentService>>,
    pub command_data: Arc<CommandData>,
    pub integration_service: Arc<IntegrationService>,
    pub reference_service: Arc<ReferenceService>,
    pub server_features: Vec<String>,
}
```

The AppState is a monolithic struct containing every service. Compare with OpenCode's Effect layer system where services are composed via `Layer.buildLayer(app)`. The current approach:
1. Requires all services to be constructed before the server starts
2. Makes it impossible to lazily initialize services
3. Creates a single point of failure for service construction
4. Tightly couples all services together

#### A.2.3 Missing Middleware

OpenCode's server has these middleware layers that RustCode lacks:
- **Authentication** — Basic auth, bearer token validation
- **Authorization** — Session-level permission checks
- **Rate Limiting** — Request throttling
- **Request Logging** — Structured request/response logging
- **Error Normalization** — Consistent error response format
- **Session Location** — Middleware to resolve session workspace

### A.3 rustcode-tui — Deep Dive

**Location:** `/root/opencodesport/rustcode/crates/rustcode-tui/`
**Cargo.toml:** 11 dependencies  
**Source files:** 24+ .rs files  
**Estimated total lines:** ~4,000+

#### A.3.1 Component Architecture

The TUI uses ratatui (immediate-mode rendering) with these components:

**Core:**
- `app.rs` — Main application state machine
- `event.rs` — Input event handling
- `keymap.rs` — Keybinding configuration
- `theme.rs` — Color theme definitions
- `clipboard.rs` — Clipboard integration
- `editor.rs` — External editor integration
- `sse_client.rs` — SSE client for server events

**UI Components:**
- `components/mod.rs` — Component orchestration
- `components/conversation.rs` — Message conversation display
- `components/input.rs` — Prompt input area
- `components/status.rs` — Status bar
- `components/sidebar.rs` — Side panel
- `components/session_list.rs` — Session browser
- `components/diff.rs` — Diff viewer
- `components/dialog.rs` — General dialog
- `components/export_dialog.rs` — Export dialog
- `components/model_selector.rs` — Model selection
- `components/question.rs` — Question prompt
- `components/permission.rs` — Permission prompt
- `components/subagent.rs` — Sub-agent display
- `components/timeline.rs` — Event timeline
- `components/toast.rs` — Toast notifications
- `components/tool_render.rs` — Tool output rendering

#### A.3.2 Comparison with OpenCode TUI

OpenCode's TUI (`@opencode-ai/tui`) is substantially more sophisticated:

| Feature | rustcode-tui | @opencode-ai/tui |
|---|---|---|
| Rendering engine | Ratatui (immediate mode) | OpenTUI (SolidJS, virtual DOM) |
| Component model | Function-based | SolidJS reactive components |
| Plugin system | None | Plugin command shims, slot system |
| Keybinding system | Custom keymap.rs | `@opentui/keymap` with config |
| Theme system | Theme struct | Full theme with `@opentui/core` |
| Clipboard | crossterm clipboard | clipboardy + platform-specific |
| Attention management | None | `attention.ts` — focus tracking |
| Localization | None | Locale support (`util/locale.ts`) |
| Audio notifications | None | Audio support (`audio.ts`) |
| Prompt display | Basic | Rich prompt with attachments |
| File watching | None | Chokidar + watcher integration |
| SSE transport | SseClient struct | Effect-based streaming |

### A.4 rustcode-lsp — Deep Dive

**Location:** `/root/opencodesport/rustcode/crates/rustcode-lsp/`
**Cargo.toml:** 5 dependencies (6 with dev-deps)
**Source files:** 1 main file (lib.rs, 1,537+ lines)
**Estimated total lines:** ~1,537+

#### A.4.1 Architecture

```
LspManager (manages multiple servers)
  ├── LspClient (individual server connection)
  │     └── LspClientState (internal state machine)
  │           ├── child process (ChildStdin/ChildStdout/ChildStderr)
  │           ├── pending_requests (HashMap<u64, oneshot::Sender>)
  │           └── diagnostics (RwLock<Vec<LspDiagnostic>>)
  └── Auto-detection (known_servers + CONFIG_FILE_TO_SERVER)
```

#### A.4.2 Implementation Quality

The LSP crate is the most complete and well-tested crate:

- **JSON-RPC framing** — Proper `Content-Length: N\r\n\r\n<json>` protocol
- **Initialize handshake** — 45-second timeout, server capabilities negotiation
- **Request/response correlation** — ID-based matching with `oneshot` channels
- **Notification handling** — `publishDiagnostics`, `logMessage`, progress
- **Graceful shutdown** — `shutdown` → `exit` → force-kill with 500ms grace
- **Error handling** — 11 error variants covering all failure modes
- **Test coverage** — 68+ test cases for framing, parsing, server detection, edge cases
- **Workspace detection** — 30+ language servers, config-file-based auto-detection

#### A.4.3 Test Coverage Detail

```rust
#[cfg(test)]
mod tests {
    // JSON-RPC framing tests
    fn frame_contains_length_and_body()
    fn frame_content_length_matches_byte_count()
    fn parse_roundtrip()
    fn parse_notification_missing_id()
    fn parse_missing_header()
    fn parse_incomplete_body()
    fn parse_missing_content_length()
    fn parse_header_case_insensitive()
    fn parse_header_whitespace_insensitive()
    
    // extract_messages tests
    fn extract_single_message()
    fn extract_multiple_messages()
    fn extract_partial_message_waits()
    fn extract_complete_then_partial()
    fn extract_empty_input()
    fn extract_non_utf8_returns_empty()
    
    // frame_lsp_message edge cases
    fn frame_empty_json()
    fn frame_unicode_preserved()
    
    // Server-for-file detection
    fn rust_extension()
    fn typescript_extension()
    fn python_extension()
    fn go_extension()
    fn without_leading_dot()
    fn unknown_extension()
    fn every_extension_mapped_to_at_least_one_server()
    
    // Known server catalog integrity
    fn all_servers_have_command()
    fn all_servers_have_extensions()
    fn all_server_ids_unique()
    
    // Config file to server coverage
    fn config_files_map_to_known_servers()
    // ... additional tests
}
```

This crate demonstrates the ideal pattern for RustCode's architecture and can serve as a template for refactoring other crates.

### A.5 rustcode-mcp — Deep Dive

**Location:** `/root/opencodesport/rustcode/crates/rustcode-mcp/`
**Cargo.toml:** 7 dependencies (8 with dev-deps)
**Source files:** 1 main file (lib.rs, 1,452+ lines)
**Estimated total lines:** ~1,452+

#### A.5.1 Architecture

```
McpTransport (trait — Send + Sync)
  ├── StdioTransport — Local subprocess communication
  │     ├── Frame protocol: Content-Length: N\r\n\r\n<json>
  │     ├── Initialize handshake on connect()
  │     └── Mutex-protected child process access
  └── HttpTransport — Remote HTTP communication
        ├── POST JSON-RPC requests
        ├── Initialize handshake via initialize()
        └── Configurable headers and timeout

McpToolExecutor
  ├── Wraps McpClient for single tool execution
  ├── execute() — Raw JSON-RPC tool call
  ├── execute_formatted() — Human-readable output
  └── to_plugin_def() — Tool registry integration

McpDiscovery
  ├── from_claude_desktop_config() — JSON parsing
  ├── from_opencode_config() — JSON parsing
  └── from_env() — Environment variable parsing
        ├── MCP_SERVERS (JSON array/map)
        └── MCP_SERVER_<NAME>_<PROPERTY> prefix
```

#### A.5.2 Implementation Quality

The MCP crate is also well-implemented with:

- **Two transport implementations** — Stdio and HTTP
- **Complete JSON-RPC 2.0 support** — Requests, notifications, responses, errors
- **MCP spec compliance** — Initialize handshake, `2024-11-05` protocol version
- **Comprehensive discovery** — Three config sources with robust parsing
- **OAuth support** — `McpOAuthConfig` type for remote server auth
- **Test coverage** — 60+ test cases for framing, parsing, discovery, helpers

#### A.5.3 Test Coverage Detail

```rust
#[cfg(test)]
mod tests {
    // Frame helpers
    fn test_frame_message_format()
    fn test_frame_message_preserves_body()
    fn test_parse_content_length_valid()
    fn test_parse_content_length_with_whitespace()
    fn test_parse_content_length_zero()
    fn test_parse_content_length_missing()
    fn test_parse_content_length_invalid_number()
    fn test_parse_content_length_empty()
    
    // JSON-RPC helpers
    fn test_build_jsonrpc_request()
    fn test_build_jsonrpc_request_with_params()
    fn test_build_jsonrpc_notification_has_no_id()
    fn test_parse_jsonrpc_response_success()
    fn test_parse_jsonrpc_response_error()
    fn test_parse_jsonrpc_response_error_with_data()
    fn test_parse_jsonrpc_response_invalid_json()
    
    // JSON-RPC stream parsing
    fn test_parse_jsonrpc_stream_single_message()
    fn test_parse_jsonrpc_stream_multiple_messages()
    fn test_parse_jsonrpc_stream_incomplete_trailer()
    fn test_parse_jsonrpc_stream_empty()
    fn test_parse_jsonrpc_stream_no_header()
    
    // Tool key generation
    fn test_tool_key_basic()
    fn test_tool_key_sanitizes_special_chars()
    fn test_tool_key_with_hyphens_and_underscores()
    fn test_sanitize_name_preserves_alphanumeric()
    fn test_sanitize_name_replaces_symbols()
    fn test_sanitize_name_empty()
    
    // extract_mcp_content
    fn test_extract_mcp_content_text_blocks()
    fn test_extract_mcp_content_mixed_blocks()
    fn test_extract_mcp_content_no_text_blocks()
    fn test_extract_mcp_content_no_content_field()
    
    // McpDiscovery: Claude Desktop config
    fn test_parse_claude_server_entry_local()
    fn test_parse_claude_server_entry_remote()
    fn test_parse_claude_server_entry_remote_type()
    // ... additional tests
}
```

### A.6 rustcode (bin) — Deep Dive

**Location:** `/root/opencodesport/rustcode/src/main.rs`
**Dependencies:** 10 (all workspace)
**Estimated lines:** 2,000+

#### A.6.1 CLI Command Coverage

The CLI supports 24 subcommands, matching OpenCode's CLI surface:

| Subcommand | RustCode Lines | OpenCode Source | Status |
|---|---|---|---|
| acp | 730-743 | `cli/cmd/acp.ts` | Skeleton |
| mcp | 748-810 | `cli/cmd/mcp.ts` | Skeleton |
| tui | 431-486 | `cli/cmd/tui.ts` | Skeleton |
| attach | 488-534 | `cli/cmd/attach.ts` | Skeleton |
| run | 282-428 | `cli/cmd/run.ts` | Partial |
| generate | — | `cli/cmd/generate.ts` | Skeleton |
| debug | 816-1015 | `cli/cmd/debug/` | Skeleton |
| console | 1021-1053 | `cli/cmd/account.ts` | Skeleton |
| providers | 1059-1094 | `cli/cmd/providers.ts` | Skeleton |
| agent | 1096-1138 | `cli/cmd/agent.ts` | Skeleton |
| upgrade | 537-556 | `cli/cmd/upgrade.ts` | Skeleton |
| uninstall | 558-587 | `cli/cmd/uninstall.ts` | Skeleton |
| serve | 246-280 | `cli/cmd/serve.ts` | Skeleton |
| web | 246-280 | `cli/cmd/web.ts` | Skeleton |
| models | 589-611 | `cli/cmd/models.ts` | Skeleton |
| stats | 613-642 | `cli/cmd/stats.ts` | Skeleton |
| export | 644-660 | `cli/cmd/export.ts` | Skeleton |
| import | 662-672 | `cli/cmd/import.ts` | Skeleton |
| github | 1144-1174 | `cli/cmd/github.ts` | Skeleton |
| pr | 674-684 | `cli/cmd/pr.ts` | Skeleton |
| session | 1180-1205 | `cli/cmd/session.ts` | Skeleton |
| plugin | 686-708 | `cli/cmd/plug.ts` | Skeleton |
| db | 710-727 | `cli/cmd/db.ts` | Skeleton |
| version | 241-242 | `index.ts` | Complete |

**Status: "Complete" means all args mapped. "Partial" means args mapped + basic handler logic. "Skeleton" means args mapped but handler is a stub returning empty/error.**

---

## Part B: Cross-Crate Dependency Analysis

### B.1 Current Dependency Graph (Detailed)

```
rustcode (bin)
├── rustcode-core   (39 deps: tokio, serde, sqlx, reqwest, ...)
├── rustcode-server (11 deps)
│   └── rustcode-core
├── rustcode-tui    (11 deps)
│   └── rustcode-core
├── rustcode-lsp    (5 deps)
│   └── rustcode-core
└── rustcode-mcp    (7 deps)
    └── rustcode-core
```

### B.2 Target Dependency Graph (Recommended)

```
rustcode-types (0 deps — no external dependencies)
├── ModelId, ProviderId, SessionId
├── ChatMessage, StreamChunk, Usage
├── Error variants (simple)
└── Enums (FinishReason, ReasoningEffort, etc.)

rustcode-database (1 dep: sqlx)
├── SQL schema definitions
├── Connection management
├── Migration runner
├── Query helpers
└── depends on: rustcode-types

rustcode-config (2 deps: serde, toml)
├── Config loading & merging
├── Config sources (file, env)
├── AgentConfig, ProviderConfig, McpConfig
└── depends on: rustcode-types

rustcode-provider (4 deps: reqwest, serde, async-trait, tokio)
├── Provider trait
├── 18 provider implementations
├── Route system (Protocol, Endpoint, Auth, Framing)
└── depends on: rustcode-types

rustcode-tool (2 deps: serde, async-trait)
├── Tool trait
├── ToolRegistry
├── Tool implementations (read, write, edit, bash, ripgrep, lsp, agent)
└── depends on: rustcode-types, rustcode-provider

rustcode-session (3 deps: tokio, serde, chrono)
├── SessionManager (CRUD)
├── SessionRunner (execution loop)
├── SessionV2 (durable prompt, event sourcing)
├── Compaction, retry, history
└── depends on: rustcode-types, rustcode-provider, rustcode-tool, rustcode-database

rustcode-plugin (2 deps: serde, tokio)
├── PluginManager
├── Hook lifecycle
├── npm resolution
├── Plugin sandboxing
└── depends on: rustcode-types, rustcode-tool

rustcode-server (4 deps: axum, tower, tokio, serde)
├── HTTP routes
├── SSE streaming
├── Auth middleware
├── OpenAPI spec generation
└── depends on: all above crates

rustcode-tui (3 deps: ratatui, crossterm, tokio)
├── Component tree
├── Event handling
├── SSE client
└── depends on: rustcode-types, rustcode-server

rustcode-lsp (2 deps: tokio, serde)
├── LspManager, LspClient
├── JSON-RPC framing
├── Server catalog
└── depends on: rustcode-types

rustcode-mcp (3 deps: tokio, serde, reqwest)
├── McpTransport (Stdio + HTTP)
├── McpDiscovery
├── McpToolExecutor
└── depends on: rustcode-types

rustcode (bin — 1 dep: clap)
├── CLI definition
├── Command dispatch
└── depends on: all library crates

rustcode-sdk (1 dep: serde)
├── Public API re-exports
├── Type definitions
└── depends on: rustcode-types
```

### B.3 Compilation Impact Analysis

With the current monolithic core:
- Changing `error.rs` → recompiles: ALL 78 modules in core + ALL 4 dependent crates = ~55,000 lines
- Changing `provider.rs` → recompiles: ~10,000 lines (providers + tools + session + server + tui)
- Adding a new provider → recompiles: core + server + tui

With the proposed split:
- Changing `rustcode-types` → recompiles: all crates (but types changes are rare)
- Changing `rustcode-provider` → recompiles: session, server, tui (not database, not config)
- Changing `rustcode-database` → recompiles: session, server (not providers, not tools)
- Adding a new provider → recompiles: provider crate only (~10 files)

---

## Part C: Test Infrastructure Comparison

### C.1 Current Test Coverage

| Crate | Test Files | Test Count | Coverage Type |
|---|---|---|---|
| rustcode-core | Few inline | Minimal | Unit |
| rustcode-server | None | 0 | None |
| rustcode-tui | None | 0 | None |
| rustcode-lsp | Inline in lib.rs | 68+ | Unit (extensive) |
| rustcode-mcp | Inline in lib.rs | 60+ | Unit (extensive) |
| rustcode (bin) | None | 0 | None |

### C.2 OpenCode Test Infrastructure

- **Per-package test suites** — Each package has `bun test` with `--only-failures`
- **HTTP recorder** — `packages/http-recorder/` provides cassette-based provider testing
- **Recorded tests** — Deterministic replay of LLM provider interactions
- **Effect test helpers** — `testEffect(...)` for Effect-layer tests
- **E2E tests** — Playwright tests in `packages/app/e2e/`
- **CI pipelines** — GitHub Actions with typecheck, test, benchmark

### C.3 Recommended Test Architecture

```
rustcode/
  tests/                         (integration tests — workspace-level)
  crates/
    rustcode-provider/
      tests/
        recorded/                (cassette files for provider tests)
        anthropic_test.rs
        openai_test.rs
      src/ ...
    rustcode-session/
      tests/
        session_manager_test.rs
        session_runner_test.rs
      src/ ...
    rustcode-http-recorder/      (new crate)
      src/
        lib.rs                   (Cassette, RecorderMiddleware)
        recording.rs
        replay.rs
```

---

## Part D: Detailed Plugin System Analysis

### D.1 OpenCode Plugin Architecture

```
Plugin Discovery
  ├── Built-in plugins (catalog.ts)
  ├── npm packages (@opencode-ai/plugin-*)
  ├── File-based plugins (.opencode/plugins/*.ts)
  └── Remote configuration

Plugin Lifecycle
  ├── Install (npm install + config patching)
  ├── Load (resolve entrypoint, verify API)
  ├── Initialize (register hooks, tools, slots)
  ├── Execute (hook callbacks at defined points)
  └── Uninstall (remove config, clean up)

Hook Points
  ├── beforeTool — Intercept/modify tool calls
  ├── afterTool — Process tool results
  ├── beforePrompt — Modify system prompt
  ├── afterPrompt — Process assistant response
  ├── beforeRequest — Modify LLM request
  ├── afterRequest — Process LLM response
  ├── onEvent — React to any session event
  └── onError — Handle errors

Plugin API Surface (@opencode-ai/plugin)
  ├── tool — Register custom tools
  ├── tui — Add UI elements (command shims, slots)
  └── sdk — Programmatic access

Plugin Sandboxing
  ├── Process isolation (separate Bun process)
  ├── API surface limitation (only @opencode-ai/plugin exports)
  ├── Timeout enforcement
  └── Error containment
```

### D.2 RustCode Plugin Status

What's implemented:
- `PluginSource` enum (File, Npm)
- `PluginKind` enum (Server, Tui)
- `PluginManager` struct (empty)
- Basic plugin metadata types
- Plugin config file parsing

What's missing:
- ❌ Hook system (no hook registration or dispatch)
- ❌ NPM resolution (`npm.rs` exists but lacks plugin-specific logic)
- ❌ Plugin loading from `.opencode/plugins/`
- ❌ Plugin metadata store (persistence)
- ❌ Process isolation / sandboxing
- ❌ Plugin API crate (no `@opencode-ai/plugin` equivalent)
- ❌ TUI plugin slots
- ❌ Error recovery for failing plugins

---

## Part E: Database Migration System

### E.1 OpenCode Migration Architecture

```
packages/core/
  drizzle.config.ts              — Drizzle configuration
  src/
    database/
      migration.ts               — Migration runner
      migration.gen.ts           — Auto-generated migrations
      schema.gen.ts              — Auto-generated schema types
      schema.sql.ts              — Base SQL schema
```

OpenCode uses Drizzle Kit for migration generation:
1. Schema changes are made in TypeScript (`*.sql.ts` files)
2. `drizzle-kit` generates migration SQL
3. Migration runner applies pending migrations at startup
4. Migration metadata tracked in database tables

### E.2 RustCode Migration Status

RustCode defines 20 tables as raw SQL string constants in `database.rs`:
```rust
pub const CREATE_WORKSPACE_TABLE: &str = "CREATE TABLE IF NOT EXISTS workspace ( ... )";
```

This approach:
1. Lacks migration history tracking
2. Cannot evolve schema without risking data loss
3. Has no `ALTER TABLE` migration generation
4. Uses `CREATE TABLE IF NOT EXISTS` which silently ignores schema changes
5. Cannot roll back migrations

The `database.rs` file has `MigrationMeta` types defined (lines 2300+) but no migration runner implementation.

---

## Part F: Security Architecture Analysis

### F.1 Current State

| Security Concern | RustCode Status | OpenCode Status |
|---|---|---|
| Provider API key handling | Plain env vars | Credential service + OAuth |
| Permission system | Partial (permission.rs) | Full V1 permissions |
| Plugin sandboxing | None | Process isolation |
| LLM request auth headers | Per-provider impl | Centralized auth layer |
| User identity | Account types defined | Full account service |
| HTTP auth middleware | Not implemented | Basic + bearer auth |
| Session isolation | Not implemented | Location-scoped sessions |
| MCP OAuth | Types defined only | Implemented |

### F.2 Critical Security Gaps

1. **API keys are read from environment variables** with no encryption at rest
2. **No credential service** for secure API key storage (OpenCode has OAuth-based credential management)
3. **Plugin sandboxing is missing** — plugins would have full access to the host system
4. **No HTTP authentication** for the server (anyone on the network can connect)
5. **SQL injection possible** — raw SQL strings with string formatting in database queries

---

## Part G: Performance Considerations

### G.1 Architecture-Level Performance Implications

| Factor | RustCode | OpenCode |
|---|---|---|
| Async runtime | Tokio (multi-thread) | Bun (single-threaded event loop) |
| Memory model | Ownership + borrowing (no GC) | V8 GC |
| Serialization | serde (zero-copy via borrow) | JSON.parse/stringify |
| Database | sqlx async SQLite | Drizzle ORM + bun:sqlite |
| Compilation | AOT compiled (fast startup) | JIT compiled (slower startup) |
| Memory usage | Predictable (Rust allocator) | V8 heap (GC pauses possible) |
| Concurrency model | OS threads + async tasks | Single-threaded async |

### G.2 RustCode Advantages

1. **No GC pauses** — predictable latency for LLM streaming
2. **Faster startup** — AOT compilation means sub-millisecond startup
3. **Lower memory overhead** — Rust's ownership model eliminates reference counting overhead
4. **True parallelism** — Tokio multi-thread runtime can parallelize tool execution
5. **Zero-copy deserialization** — `serde_json::from_reader` can stream-parse large responses

### G.3 RustCode Disadvantages (Current Architecture)

1. **Monolithic crate** — Large recompilation surface, poor incremental compilation
2. **Arc/RwLock overhead** — Every shared state access goes through runtime locking
3. **Clone-heavy patterns** — Many types derive Clone instead of using references
4. **No streaming JSON parser** — LLM streaming responses are buffered before parsing
5. **Missing HTTP connection pooling** — reqwest::Client is recreated per provider

---

## Part H: LLM Provider Implementation Comparison

### H.1 Provider Coverage Matrix

| Provider | RustCode | OpenCode | Protocol |
|---|---|---|---|
| Anthropic Claude | ✅ | ✅ | Messages API |
| OpenAI GPT | ✅ | ✅ | Chat Completions |
| Google Gemini | ✅ | ✅ | generateContent |
| Azure OpenAI | ✅ | ✅ | Chat Completions |
| AWS Bedrock | ✅ | ✅ | Converse API |
| DeepSeek | ✅ | ✅ | OpenAI-compatible Chat |
| Mistral AI | ✅ | ✅ | OpenAI-compatible Chat |
| Groq | ✅ | ✅ | OpenAI-compatible Chat |
| xAI Grok | ✅ | ✅ | OpenAI-compatible Chat |
| OpenRouter | ✅ | ✅ | OpenAI-compatible Chat (extended) |
| Together AI | ✅ | ✅ | OpenAI-compatible Chat |
| Perplexity | ✅ | ✅ | OpenAI-compatible Chat |
| Cohere | ✅ | ✅ | OpenAI-compatible Chat |
| Fireworks AI | ✅ | ✅ | OpenAI-compatible Chat |
| AI21 Labs | ✅ | ✅ | OpenAI-compatible Chat |
| Cerebras | ✅ | ✅ | OpenAI-compatible Chat |
| Cloudflare AI | ✅ | ✅ | OpenAI-compatible Chat |
| GitHub Copilot | ✅ | ✅ | OpenAI-compatible Chat |
| Alibaba (Qwen) | ❌ | ✅ | OpenAI-compatible Chat |
| DeepInfra | ❌ | ✅ | OpenAI-compatible Chat |
| Google Vertex | ❌ | ✅ | Gemini |
| GitLab AI | ❌ | ✅ | GitLab provider |
| Venice AI | ❌ | ✅ | OpenAI-compatible Chat |

### H.2 Provider Implementation Size (RustCode)

| Provider | Lines (est) | Uniqueness |
|---|---|---|
| anthropic.rs | ~250 | Custom — Messages API |
| openai.rs | ~200 | Custom — Chat Completions |
| gemini.rs | ~300 | Custom — generateContent |
| azure.rs | ~200 | Custom — deployment-scoped |
| bedrock.rs | ~350 | Custom — AWS SigV4 |
| deepseek.rs | ~100 | OpenAI-compatible |
| mistral.rs | ~100 | OpenAI-compatible |
| groq.rs | ~100 | OpenAI-compatible |
| xai.rs | ~100 | OpenAI-compatible |
| openrouter.rs | ~150 | Extended OpenAI-compatible |
| together.rs | ~100 | OpenAI-compatible |
| perplexity.rs | ~100 | OpenAI-compatible |
| cohere.rs | ~100 | OpenAI-compatible |
| fireworks.rs | ~100 | OpenAI-compatible |
| ai21.rs | ~100 | OpenAI-compatible |
| cerebras.rs | ~100 | OpenAI-compatible |
| cloudflare.rs | ~150 | OpenAI-compatible |
| github_copilot.rs | ~200 | Custom — token acquisition |

**Observation:** 11 of 18 providers (61%) are "OpenAI-compatible" with near-identical code. This is the strongest argument for the route abstraction pattern — these 11 providers could be reduced to ~5-15 lines each using a shared `OpenAIChatProtocol`.

---

## Part I: Build and CI Analysis

### I.1 Current CI Pipeline

**Location:** `/root/opencodesport/rustcode/.github/workflows/ci.yml`

4 CI jobs:
1. **Format** — `cargo fmt --all -- --check`
2. **Clippy** — `cargo clippy --all-targets --all-features -- -D warnings`
3. **Test** — `cargo build && cargo test` (ubuntu-latest + macos-latest)
4. **Cargo Deny** — License + advisory checks

### I.2 CI Gaps vs OpenCode

| Feature | RustCode | OpenCode |
|---|---|---|
| Linting | clippy (warn only) | oxlint (strict) |
| Type checking | cargo check | tsgo --noEmit |
| Unit tests | cargo test | bun test per package |
| Integration tests | None | Playwright E2E |
| Provider tests | None | HTTP recorder + cassette |
| Benchmarking | None | bun run script/bench-test-suite.ts |
| Code coverage | Not tracked | Not tracked (both) |
| Dependency auditing | cargo-deny | — |
| Security scanning | None | gitleaks |
| Fuzzing | Not configured | Not configured |
| WASM builds | Not configured | Not applicable |

### I.3 cargo-deny Configuration

**Location:** `/root/opencodesport/rustcode/deny.toml`

Good practice — licenses and advisories are checked in CI. The configuration should be maintained as dependencies are added.

---

## Part J: Strategic Recommendations Roadmap

### J.1 Phase 1: Foundation (Weeks 1-3)

| Priority | Task | Dependencies | Effort |
|---|---|---|---|
| P0 | Extract `rustcode-types` crate | None | 1-2 days |
| P0 | Complete tool implementations | rustcode-types | 5-7 days |
| P0 | Implement event sourcing | rustcode-types, database | 5-7 days |
| P1 | Split core crate into focused crates | rustcode-types | 3-5 days |
| P1 | Add route abstraction for providers | rustcode-types | 5-7 days |
| P1 | Create rustcode-http-recorder | None | 3-4 days |

**Milestone:** Core architecture stabilized, tools working, event sourcing operational.

### J.2 Phase 2: Session System (Weeks 4-6)

| Priority | Task | Dependencies | Effort |
|---|---|---|---|
| P0 | Port V2 session architecture | Event sourcing | 4-6 weeks |
| P0 | Create SDK crate | rustcode-types | 4-5 days |
| P0 | Complete plugin system | rustcode-tool | 2-3 weeks |
| P1 | Basic service container | None | 3-5 days |
| P2 | Add observability | None | 2-3 days |

**Milestone:** V2 sessions reliable, plugin system operational, SDK published.

### J.3 Phase 3: Polish (Weeks 7-10)

| Priority | Task | Dependencies | Effort |
|---|---|---|---|
| P1 | Restructure modules into namespaces | None | 2-3 days |
| P2 | Split config file | None | 1 day |
| P2 | Split CLI main.rs | None | 1 day |
| P2 | Implement background jobs | Database | 3-4 days |
| P2 | Add compile-time SQL checking | Database | 3-5 days |
| P2 | Domain-specific error types | rustcode-types | 2-3 days |
| P3 | Add config source abstraction | None | 1 day |

**Milestone:** Codebase is modular, maintainable, and well-organized.

### J.4 Phase 4: Features (Weeks 11+)

| Priority | Task | Dependencies | Effort |
|---|---|---|---|
| P3 | Web frontend (SolidJS shared with OpenCode) | Server | Months |
| P3 | Desktop application (Electron + Tauri) | Server, Web | Months |
| P3 | Multi-user server | Auth, Session | Months |
| P3 | Clustering for Session Execution | Event sourcing | Months |

---

*Architecture Audit Supplement — Agent 1*
*Date: 2026-06-19*
*This supplement contains additional per-crate deep dives, dependency analysis, security assessment, performance analysis, and strategic roadmap that complement the main architecture audit report.*
