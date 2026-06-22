# Architecture Roadmap — BlazeCode Target Architecture

**Date:** 2026-06-21
**Status:** Planning (Phase 0)
**Current Score:** 25/100
**Target Score:** 85/100

---

## 1. Current Architecture Assessment

```
┌─────────────────────────────────────────────────────────────────┐
│                      src/main.rs (8,575 lines)                   │
│  CLI parsing, business logic, DB queries, SSE, infrastructure    │
└──────────┬──────────────────────────────────────┬────────────────┘
           │ depends                              │ depends
           ▼                                      ▼
┌──────────────────────┐   ┌──────────────────────────────────────┐
│   blazecode-server     │   │         blazecode-core (95 modules)   │
│   blazecode-tui        │   │  ┌────────────────────────────────┐  │
│   blazecode-lsp        │   │  │ config provider session tool    │  │
│   blazecode-mcp        │   │  │ database permission plugin     │  │
│   (4 stubs, re-export)│   │  │ filesystem event git snapshot   │  │
└──────────────────────┘   │  │ worktree format image skill     │  │
                           │  │ question lsp mcp agent bus      │  │
                           │  │ env id error storage             │  │
                           │  │ (ALL pub, no pub(crate),         │  │
                           │  │  14 files >1000 lines)           │  │
                           │  └────────────────────────────────┘  │
                           │  Depends: sqlx, reqwest, std::fs     │
                           │  (infrastructure IN core)            │
                           └──────────────────────────────────────┘
```

### Key Problems

| Problem | Severity | Detail |
|---------|----------|--------|
| Monolithic core | Critical | 95 flat public modules, all `pub`, no visibility filtering |
| Thick binary | Critical | 8,575-line `main.rs` mixing CLI, business logic, infrastructure |
| Infrastructure in core | Critical | `sqlx`, `reqwest`, `std::fs` imported directly in domain code |
| No layering | Critical | Effectively 1.5 layers (core + 4 stub wrappers) vs BlazeCode's 4+ |
| Extreme coupling | Critical | All modules flat-scoped, change to `config.rs` ripples through 94 others |
| Low cohesion | High | 14 `session_*` files flat with no sub-module grouping |
| Missing V2 domains | High | No System Context algebra, EventV2 event sourcing, Location concepts |
| No hexagonal arch | High | Only provider trait has port/adapter pattern |
| 14 files >1000 lines | High | `database.rs`: 4,758 lines, `session.rs`: 1,481, `provider.rs`: 1,511 |
| <2% test coverage | High | No mocking infrastructure, tests require real SQLite/FS/network |

### Architecture Score: 25/100

```
Breakdown:
  +10  Clean code rules (forbid unsafe, no unwrap rules)
  +5   Workspace structure (5 crates)
  +5   Provider trait + plugin system design
  +5   Good provider port/adapter for LLM
  -20  Monolithic core, 95 flat pub modules
  -15  Thick main.rs with mixed concerns
  -15  No visibility discipline (all pub)
  -10  Infrastructure dependency in core (sqlx, reqwest)
  -10  No hexagonal architecture outside providers
  -10  Missing V2 domain model (System Context, EventV2)
  -5   No architecture-level testing strategy
```

---

## 2. Target Architecture Principles

### Hexagonal Architecture (Ports/Adapters)

```
                    ┌─────────────┐
                    │  Domain      │
                    │  (use cases) │
                    └──────┬──────┘
                           │ defines ports (traits)
              ┌────────────┼────────────┐
              ▼            ▼            ▼
     ┌────────────┐ ┌────────────┐ ┌────────────┐
     │ SQLite     │ │ Local FS   │ │ Reqwest    │
     │ Adapter    │ │ Adapter    │ │ HTTP Adpt  │
     └────────────┘ └────────────┘ └────────────┘
```

- Core defines traits (ports); infrastructure crates implement them (adapters)
- No infrastructure import in domain code
- Swappable backends: SQLite ↔ PostgreSQL, local FS ↔ SSH FS, reqwest ↔ hyper

### Domain-Driven Design (Bounded Contexts)

```
┌──────────┐ ┌──────────┐ ┌──────────┐ ┌──────────┐
│ Session  │ │ Tool     │ │ Provider │ │ Plugin   │
│ Domain   │ │ Domain   │ │ Domain   │ │ Domain   │
└────┬─────┘ └────┬─────┘ └────┬─────┘ └────┬─────┘
     │            │             │            │
     └────────────┴──────┬──────┴────────────┘
                         │ via domain events
                    ┌────┴────┐
                    │ Event   │
                    │ Bus     │
                    └─────────┘
```

- Each domain is a crate with explicit public API
- Domains communicate via domain events (not direct imports)
- Each domain owns its data (database-per-domain or shared with ownership)

### Structured Concurrency (Scoped Tasks, Cancellation)

```
┌─────────────────────────────────┐
│ Scope: run_session              │
│  ├─ Fiber: heartbeat            │
│  ├─ Fiber: stream_llm           │
│  │   └─ Scope: process_chunk    │
│  │       ├─ Fiber: parse_chunk  │
│  │       └─ Fiber: emit_event   │
│  └─ Fiber: tool_execution       │
│      └─ Scope: tool_timeout     │
└─────────────────────────────────┘
         │ on scope drop: ALL fibers cancelled + joined
         ▼
```

- `ScopedFiberSet` cancels and awaits all fibers on drop
- No fiber leaks (current `FiberSet` stores handles but never joins)
- CancellationToken propagation through scope chain
- Deterministic shutdown

### Zero-Cost Abstractions

- Traits use GATs for zero-cost static dispatch (no `Box<dyn Stream>` per event)
- `#[derive(Tool)]` proc macros generate optimal code at compile time
- No runtime overhead for abstractions (Rust philosophy)
- `Arc<Vec<ChatMessage>>` instead of clones in hot path

### Modular Monolith (Crate Boundaries as Module Boundaries)

- ~25 crates matching BlazeCode's 26 packages
- Each crate has explicit dependencies (Cargo.toml)
- `pub(crate)` visibility discipline within crates
- Clean `lib.rs` re-export surface per crate
- Single binary composes all crates at the composition root

### Observable by Default

- `tracing` spans on every domain operation
- OpenTelemetry export (traces + metrics)
- Structured JSON logging (not ad-hoc `println!`)
- Metrics: session count, tool latency, provider latency, error rates

### Secure by Design

- `#![forbid(unsafe_code)]` in every crate
- WASM plugin sandbox with controlled file/network access
- Capability-based permission system (tools require explicit grants)
- No secrets in logs (structured redaction)
- `cargo-deny` for dependency auditing

---

## 3. Target Crate Architecture

```
┌─────────────────────────────────────────────────────────────────┐
│                       Composition Root                          │
│                    main.rs (~50 lines)                          │
├─────────────────────────────────────────────────────────────────┤
│  blazecode-cli        blazecode-server     blazecode-tui           │
│  blazecode-lsp        blazecode-mcp        blazecode-sdk           │
├─────────────────────────────────────────────────────────────────┤
│  blazecode-session-core   blazecode-tool-core   blazecode-plugin   │
│  blazecode-provider-core  blazecode-event-store                   │
├─────────────────────────────────────────────────────────────────┤
│  blazecode-config     blazecode-schema      blazecode-types        │
│  blazecode-error      blazecode-observability                     │
├─────────────────────────────────────────────────────────────────┤
│  Infrastructure Adapters (one crate per backend)                │
│  blazecode-database-sqlite   blazecode-database-postgres          │
│  blazecode-filesystem-local  blazecode-filesystem-ssh             │
│  blazecode-http-reqwest      blazecode-provider-anthropic         │
│  blazecode-provider-openai   blazecode-provider-gemini            │
│  blazecode-plugin-wasm                                            │
└─────────────────────────────────────────────────────────────────┘
```

### Crate Dependency Graph

```
                                    ┌────────────────────┐
                                    │   blazecode-types    │
                                    │  (zero deps, pure   │
                                    │   domain types)     │
                                    └────────┬───────────┘
                                             │
                    ┌────────────────────────┼────────────────────┐
                    │                        │                    │
               ┌────┴────┐           ┌───────┴───────┐    ┌──────┴──────┐
               │ blazecode │           │  blazecode      │    │ blazecode   │
               │ -error   │           │  -schema       │    │ -config    │
               └────┬────┘           └───────┬───────┘    └──────┬──────┘
                    │                        │                    │
                    └────────────┬───────────┴────────────────────┘
                                 │
                    ┌────────────┴────────────┐
                    │   blazecode-observability │
                    └────────────┬────────────┘
                                 │
          ┌──────────────────────┼──────────────────────┐
          │                      │                      │
     ┌────┴────┐          ┌──────┴──────┐        ┌──────┴──────┐
     │ blazecode │          │ blazecode    │        │ blazecode    │
     │ -database │         │ -filesystem │        │ -provider  │
     │ (trait)   │         │ (trait)     │        │ -core      │
     └────┬─────┘         └──────┬──────┘        └──────┬──────┘
          │                      │                      │
     ┌────┴─────┐          ┌──────┴──────┐        ┌──────┴──────┐
     │ blazecode │          │ blazecode    │        │ blazecode    │
     │-database │          │-filesystem  │        │-provider    │
     │-sqlite   │          │-local       │        │-anthropic   │
     └──────────┘          └─────────────┘        └─────────────┘

          ┌──────────────────────┼──────────────────────┐
          │                      │                      │
     ┌────┴────┐          ┌──────┴──────┐        ┌──────┴──────┐
     │ blazecode │          │ blazecode    │        │ blazecode    │
     │-session │          │ -tool-core  │        │ -plugin     │
     │-core    │          │             │        │ -core       │
     └────┬─────┘         └──────┬──────┘        └──────┬──────┘
          │                      │                      │
          └──────────────────────┼──────────────────────┘
                                 │
                    ┌────────────┴────────────┐
                    │     blazecode-event-store  │
                    └────────────┬────────────┘
                                 │
          ┌──────────────────────┼──────────────────────┐
          │                      │                      │
     ┌────┴────┐          ┌──────┴──────┐        ┌──────┴──────┐
     │ blazecode │          │ blazecode    │        │ blazecode    │
     │ -cli     │          │ -server     │        │ -tui        │
     │ (library)│          │             │        │             │
     └──────────┘          └─────────────┘        └─────────────┘
```

### Crate Catalog (~27 crates)

| # | Crate | Type | Purpose | Deps (blazecode) | Deps (external) |
|---|-------|------|---------|-----------------|-----------------|
| 1 | `blazecode-types` | Foundation | Core domain types, newtypes, traits | none | serde, thiserror |
| 2 | `blazecode-schema` | Foundation | JSON Schema types, serialization contracts | types | serde, serde_json, jsonschema |
| 3 | `blazecode-config` | Foundation | Config model, TOML parsing, validation | types, schema, error | serde, toml |
| 4 | `blazecode-error` | Foundation | Unified error hierarchy, `Error`, `Result` | types | thiserror |
| 5 | `blazecode-observability` | Foundation | Tracing, metrics, logging infrastructure | types, error | tracing, opentelemetry |
| 6 | `blazecode-database` | Port | `Database` trait, session/event/tool repos | types, error | async-trait |
| 7 | `blazecode-filesystem` | Port | `FileSystem` trait, glob, read/write/search | types, error | async-trait, tokio |
| 8 | `blazecode-http` | Port | `HttpClient` trait, streaming, retries | types, error, observability | async-trait |
| 9 | `blazecode-provider-core` | Port | `Provider` trait, `Model`, `StreamChunk` | types, error, http | async-trait, serde |
| 10 | `blazecode-provider-anthropic` | Adapter | Anthropic Messages API | provider-core, http | reqwest, serde |
| 11 | `blazecode-provider-openai` | Adapter | OpenAI Chat Completions API | provider-core, http | reqwest, serde |
| 12 | `blazecode-provider-gemini` | Adapter | Google Gemini API | provider-core, http | reqwest, serde |
| 13 | `blazecode-provider-bedrock` | Adapter | AWS Bedrock API | provider-core, http | reqwest, aws-sigv4 |
| 14 | `blazecode-provider-ollama` | Adapter | Local Ollama API | provider-core, http | reqwest |
| 15 | `blazecode-provider-openai-compatible` | Adapter | Generic OpenAI-compatible (14 variants) | provider-core, http | reqwest |
| 16 | `blazecode-session-core` | Domain | Session model, event sourcing, lifecycle | types, error, database, event-store | async-trait |
| 17 | `blazecode-tool-core` | Domain | `Tool` trait, execution pipeline, registry | types, error, filesystem | async-trait |
| 18 | `blazecode-tool-impls` | Domain | Built-in tools (bash, read, write, edit, grep) | tool-core, types, filesystem | tokio |
| 19 | `blazecode-permission` | Domain | Permission evaluation, rules, policies | types, error | none |
| 20 | `blazecode-plugin-core` | Domain | `Plugin` trait, discovery, lifecycle | types, error | async-trait |
| 21 | `blazecode-plugin-wasm` | Infrastructure | WASM plugin sandbox using wasmtime | plugin-core, types | wasmtime, wit |
| 22 | `blazecode-event-store` | Infrastructure | EventV2 event sourcing, replay | types, error, database | async-trait |
| 23 | `blazecode-database-sqlite` | Adapter | SQLite implementation of `Database` trait | database, types | sqlx, tokio |
| 24 | `blazecode-filesystem-local` | Adapter | Local filesystem via `tokio::fs` | filesystem, types | tokio |
| 25 | `blazecode-sdk` | Public API | Public SDK for Rust consumers | types, session-core, tool-core, provider-core | tokio |
| 26 | `blazecode-cli` | Application | CLI argument parsing + command dispatch | all domain crates, config | clap, tokio |
| 27 | `blazecode-server` | Application | HTTP/SSE server (axum) | session-core, config, observability | axum, tower |
| 28 | `blazecode-tui` | Application | Terminal UI (ratatui) | session-core, config | ratatui, tokio |
| 29 | `blazecode-lsp` | Application | LSP integration | session-core, config | tower-lsp |
| 30 | `blazecode-mcp` | Application | MCP integration | tool-core, config | serde |

---

## 4. Module Architecture per Crate

### `blazecode-types` — Core Domain Types

**Purpose:** Zero-dependency crate holding all shared domain types, newtypes, and core traits. Every other crate depends on this one.

**Public interface:**
```rust
pub use id::*;
pub use types::*;
pub use traits::*;
```

**Key types:**
- `SessionId(String)` — newtype with `ses_` prefix validation
- `MessageId(String)` — newtype with `msg_` prefix validation
- `ToolId(String)` — newtype
- `ProviderId(String)` — newtype
- `ModelId(String)` — newtype
- `ChatMessage { role, content, parts }` — LLM message
- `Part { text, tool_call, tool_result, ... }` — message parts
- `Model { id, provider, capabilities, context_window, ... }`
- `StreamChunk { delta, stop_reason, usage, ... }`

**Dependencies:** None (serde + thiserror only)

**External deps:** `serde`, `thiserror`, `chrono`

**Module organization:**
```
src/
  lib.rs           — re-exports
  id.rs            — newtypes: SessionId, MessageId, ToolId, ProviderId, ModelId
  message.rs       — ChatMessage, Part, PartBuilder
  model.rs         — Model, ModelCapabilities, ModelPricing
  stream.rs        — StreamChunk, LlmEvent
  time.rs          — Timestamp, Duration newtypes
  traits.rs        — Shared traits (Identifiable, Named, Described)
  permission.rs    — Permission types (Role, Capability, Grant)
  session.rs       — SessionStatus, SessionMode types
  tool.rs          — ToolCall, ToolResult types
```

**Testing strategy:**
- Unit tests for ID validation (prefix, format)
- Unit tests for message/part construction
- Serde round-trip tests for all types
- Property-based tests for ID generation (ascending/descending order)

---

### `blazecode-error` — Unified Error Hierarchy

**Purpose:** Single `Error` enum with `thiserror` derives. Every crate converts its errors into this hierarchy via `#[from]`.

**Public interface:**
```rust
#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error("config: {0}")]
    Config(#[from] ConfigError),
    #[error("session: {0}")]
    Session(#[from] SessionError),
    #[error("tool execution: {0}")]
    Tool(#[from] ToolError),
    #[error("provider: {0}")]
    Provider(#[from] ProviderError),
    #[error("database: {0}")]
    Database(#[from] DatabaseError),
    #[error("filesystem: {0}")]
    Filesystem(#[from] FilesystemError),
    #[error("http: {0}")]
    Http(#[from] HttpError),
    #[error("plugin: {0}")]
    Plugin(#[from] PluginError),
    #[error("permission denied: {0}")]
    Permission(#[from] PermissionError),
    #[error("serialization: {0}")]
    Serialization(#[from] serde_json::Error),
    #[error("internal: {0}")]
    Internal(String),
}

pub type Result<T> = std::result::Result<T, Error>;
```

**Sub-error types** (`ConfigError`, `SessionError`, etc.) live in their respective crates and implement `std::error::Error + Send + Sync`.

**No dependencies on blazecode crates** (except `types` for type info).

---

### `blazecode-config` — Configuration Model

**Purpose:** Load, parse, validate configuration from TOML files + env vars + CLI overrides.

**Public interface:**
```rust
pub struct Config {
    pub providers: Vec<ProviderConfig>,
    pub agent: AgentConfig,
    pub permission: PermissionConfig,
    pub server: Option<ServerConfig>,
    pub plugins: Vec<PluginConfig>,
}

impl Config {
    pub fn load() -> Result<Self>;          // from default paths + env
    pub fn load_from(path: &Path) -> Result<Self>;
    pub fn merge(self, cli_overrides: CliOverrides) -> Result<Self>;
    pub fn validate(&self) -> Result<()>;
}
```

**Key types:** `Config`, `ProviderConfig`, `AgentConfig`, `McpConfig`, `ServerConfig`, `PluginConfig`

**Dependencies:** `blazecode-types`, `blazecode-error`, `blazecode-schema`

**External deps:** `serde`, `toml`, `schemars`

**Module organization:**
```
src/
  lib.rs           — Config struct, load/merge/validate
  provider.rs      — ProviderConfig, ProviderAuth (api_key, env_var)
  agent.rs         — AgentConfig, AgentMode (plan, auto, chat)
  permission.rs    — PermissionConfig, default policies
  server.rs        — ServerConfig, bind address, CORS
  plugin.rs        — PluginConfig, discovery paths
  cli.rs           — CliOverrides, CLI argument merging
  validation.rs    — Config validation rules (no duplicate providers, etc.)
```

**Testing strategy:**
- Load from valid TOML strings
- Error on invalid TOML
- Merge precedence (CLI > env > config file > defaults)
- Validation: missing provider key, duplicate provider IDs, invalid URLs

---

### `blazecode-observability` — Tracing, Metrics, Logging

**Purpose:** Set up `tracing` subscriber, OpenTelemetry export, structured logging, metrics collection.

**Public interface:**
```rust
pub fn init(config: &ObservabilityConfig);
pub fn shutdown();

pub struct Metrics {
    pub sessions_active: Counter,
    pub tools_executed: Counter,
    pub provider_latency: Histogram,
    pub errors_total: Counter,
}
```

**Dependencies:** `blazecode-types`, `blazecode-error`

**External deps:** `tracing`, `tracing-subscriber`, `opentelemetry`, `opentelemetry-otlp`, `metrics`, `metrics-exporter-prometheus`

**Module organization:**
```
src/
  lib.rs           — init(), shutdown(), Metrics struct
  tracing.rs       — tracing subscriber setup (JSON, OTLP)
  metrics.rs       — metrics registry, exporters
  logging.rs       — structured log formatting, redaction
  spans.rs         — reusable span definitions
```

---

### `blazecode-database` — Database Port (Trait)

**Purpose:** Define `Database` trait as the port for all persistence operations. No implementation — just the contract.

**Public interface:**
```rust
#[async_trait]
pub trait Database: Send + Sync + 'static {
    // Session CRUD
    async fn get_session(&self, id: &SessionId) -> Result<SessionRow>;
    async fn list_sessions(&self, filter: &SessionFilter) -> Result<Vec<SessionRow>>;
    async fn insert_session(&self, session: &SessionRow) -> Result<()>;
    async fn update_session(&self, id: &SessionId, patch: &SessionPatch) -> Result<()>;
    async fn delete_session(&self, id: &SessionId) -> Result<()>;

    // Message CRUD
    async fn get_messages(&self, session_id: &SessionId) -> Result<Vec<MessageRow>>;
    async fn insert_message(&self, message: &MessageRow) -> Result<()>;
    async fn insert_messages_batch(&self, messages: &[MessageRow]) -> Result<()>;

    // Event store
    async fn append_event(&self, event: &EventRow) -> Result<()>;
    async fn read_events(&self, after_seq: Option<i64>) -> Result<Vec<EventRow>>;

    // Migrations
    async fn run_migrations(&self) -> Result<()>;
    async fn version(&self) -> Result<i64>;
}
```

**Key types:** `SessionRow`, `MessageRow`, `EventRow`, `SessionFilter`, `SessionPatch`

**Dependencies:** `blazecode-types`, `blazecode-error`

**External deps:** `async-trait`, `serde`

**Module organization:**
```
src/
  lib.rs           — Database trait
  types.rs         — Row types, SessionFilter, SessionPatch
  error.rs         — DatabaseError enum
  migration.rs     — Migration trait (version, up, down)
```

---

### `blazecode-database-sqlite` — SQLite Adapter

**Purpose:** Implements `Database` trait using `sqlx` + SQLite.

**Key types:** `SqliteDatabase { pool: SqlitePool }`

**Dependencies:** `blazecode-database`, `blazecode-types`, `blazecode-error`

**External deps:** `sqlx` (sqlite), `tokio`, `serde`

**Module organization:**
```
src/
  lib.rs           — SqliteDatabase impl
  schema.rs        — CREATE TABLE statements
  migrations.rs    — Migration scripts
  queries/
    session.rs     — session CRUD queries
    message.rs     — message CRUD queries
    event.rs       — event store queries
```

---

### `blazecode-provider-core` — Provider Port

**Purpose:** Define `Provider` trait as the port for LLM provider abstractions. Route-based architecture (protocol/endpoint/auth/framing).

**Public interface:**
```rust
pub trait Provider: Send + Sync {
    type Stream<'a>: Stream<Item = Result<LlmEvent>> + Send + Unpin + 'a
    where Self: 'a;

    async fn stream(&self, request: ChatRequest) -> Result<Self::Stream<'_>>;
    fn model(&self) -> &Model;
}

// Route architecture components
pub trait Protocol<Body, Event> {
    fn build_request(&self, request: &ChatRequest) -> Result<Body>;
    fn decode_chunk(&self, bytes: &[u8]) -> Result<Option<Event>>;
    fn step(&self, state: &mut State, event: Event) -> Result<Vec<LlmEvent>>;
}

pub trait Auth {
    fn headers(&self) -> Result<HeaderMap>;
    fn name(&self) -> &'static str;
}

pub enum Framing {
    Sse,
    JsonLines,
    Raw,
    WebSocket,
}

pub struct Route<P: Protocol<..>> {
    pub protocol: P,
    pub endpoint: Endpoint,
    pub auth: Box<dyn Auth>,
    pub framing: Framing,
    pub client: Arc<dyn HttpClient>,
}
```

**Dependencies:** `blazecode-types`, `blazecode-error`, `blazecode-http`

**External deps:** `async-trait`, `serde`, `futures`

**Module organization:**
```
src/
  lib.rs           — Provider trait, re-exports
  route.rs         — Route struct, Protocol/Auth/Framing traits
  endpoint.rs      — Endpoint (url, path, query params)
  auth.rs          — Auth trait, BearerToken, ApiKey, EnvVar
  protocol/
    mod.rs
    openai_chat.rs — OpenAI Chat protocol
    anthropic.rs   — Anthropic Messages protocol
    gemini.rs      — Gemini protocol
  framing.rs       — Sse, JsonLines, Raw framers
  catalog.rs       — ProviderCatalog (registry of providers)
  error.rs         — ProviderError
```

---

### `blazecode-provider-anthropic` — Anthropic Provider

**Purpose:** Single provider implementation using the route architecture. ~50 lines.

```rust
pub fn anthropic_provider(http: Arc<dyn HttpClient>, api_key: String) -> impl Provider {
    Route {
        protocol: AnthropicMessagesProtocol,
        endpoint: Endpoint::path("https://api.anthropic.com/v1/messages"),
        auth: Auth::bearer(api_key),
        framing: Framing::Sse,
        client: http,
    }
}
```

**Dependencies:** `blazecode-provider-core`, `blazecode-http`, `blazecode-types`

**External deps:** `serde`, `serde_json`

---

### `blazecode-session-core` — Session Domain

**Purpose:** Session model, event sourcing, lifecycle management, prompt admission.

**Public interface:**
```rust
pub struct Session {
    pub id: SessionId,
    pub status: SessionStatus,
    pub config: SessionConfig,
    pub messages: Vec<Message>,
    pub epoch: ContextEpoch,
}

impl Session {
    pub fn create(config: SessionConfig) -> Self;
    pub fn restore(events: Vec<SessionEvent>) -> Result<Self>;  // event sourcing replay
    pub fn prompt(&mut self, input: PromptInput) -> Result<PromptId>;
    pub fn execute_turn(&mut self, turn: &ProviderOutput) -> Result<()>;
    pub fn compact(&mut self) -> Result<CompactionResult>;
    pub fn fork(&self, at_message: &MessageId) -> Result<Session>;
    pub fn revert(&mut self, to_message: &MessageId) -> Result<()>;
}

// Event sourcing
pub enum SessionEvent {
    Created { id: SessionId, config: SessionConfig, timestamp: Timestamp },
    PromptAdmitted { prompt_id: PromptId, content: String },
    MessageAppended { message: Message },
    ToolExecuted { call: ToolCall, result: ToolResult },
    EpochAdvanced { from: EpochId, to: EpochId, reason: String },
    Compacted { summary: String, baseline: ContextBaseline },
}
```

**Dependencies:** `blazecode-types`, `blazecode-error`, `blazecode-database`, `blazecode-event-store`

**External deps:** `async-trait`, `tokio`, `serde`

**Module organization:**
```
src/
  lib.rs           — Session struct, public API
  manager.rs       — SessionManager (create, load, save, list, delete)
  event.rs         — SessionEvent enum, event sourcing
  prompt.rs        — PromptInput, PromptId, admission lifecycle
  message.rs       — Message type, construction
  epoch.rs         — ContextEpoch, epoch management
  compaction.rs    — Context overflow detection, compaction strategy
  revert.rs        — Fork, revert, clear_revert
  config.rs        — SessionConfig
  error.rs         — SessionError
```

---

### `blazecode-tool-core` — Tool Domain

**Purpose:** `Tool` trait, `ToolRegistry`, execution pipeline with permission checks.

**Public interface:**
```rust
#[async_trait]
pub trait Tool: Send + Sync {
    fn name(&self) -> &'static str;
    fn description(&self) -> &'static str;
    fn schema(&self) -> JsonSchemaValue;
    fn requires_permission(&self) -> bool;

    async fn execute(&self, context: &ToolContext, input: JsonValue) -> Result<ToolOutput>;
}

pub struct ToolRegistry {
    tools: HashMap<String, Box<dyn Tool>>,
}

impl ToolRegistry {
    pub fn register(&mut self, tool: Box<dyn Tool>);
    pub fn get(&self, name: &str) -> Option<&dyn Tool>;
    pub fn list(&self) -> Vec<&dyn Tool>;
}

pub struct ToolContext {
    pub messages: Arc<Vec<ChatMessage>>,  // Arc, not cloned per call
    pub filesystem: Arc<dyn FileSystem>,
    pub permission: Arc<dyn PermissionEvaluator>,
    pub session_id: SessionId,
}

// Proc-macro tool definition (blazecode-derive):
// #[tool(description = "Read file contents")]
// async fn read_file(path: String) -> Result<String> { ... }
```

**Dependencies:** `blazecode-types`, `blazecode-error`, `blazecode-filesystem`, `blazecode-permission`

**External deps:** `async-trait`, `serde`, `serde_json`, `schemars`

**Module organization:**
```
src/
  lib.rs           — Tool trait, re-exports
  registry.rs      — ToolRegistry
  context.rs       — ToolContext
  pipeline.rs      — Execution pipeline (permission check → execute → log)
  error.rs         — ToolError
```

---

### `blazecode-permission` — Permission Domain

**Purpose:** Evaluate whether a tool call / action is permitted based on rules, policies, and user configuration.

**Public interface:**
```rust
#[async_trait]
pub trait PermissionEvaluator: Send + Sync {
    async fn evaluate(&self, request: &PermissionRequest) -> Result<PermissionResult>;
}

pub struct PermissionRequest {
    pub action: Action,
    pub target: Target,
    pub context: RequestContext,
}

pub enum PermissionResult {
    Allowed,
    Denied { reason: String },
    NeedsApproval { prompt: String },
}

pub struct PermissionRule {
    pub pattern: GlobPattern,
    pub effect: Effect,
    pub scope: Scope,
}
```

**Dependencies:** `blazecode-types`, `blazecode-error`

**External deps:** `globset` (for wildcard matching)

**Module organization:**
```
src/
  lib.rs           — PermissionEvaluator trait, re-exports
  rule.rs          — PermissionRule, Pattern, Effect, Scope
  eval.rs          — Rule evaluation engine
  service.rs       — PermissionService (manages rules, evaluates requests)
  defaults.rs      — Default rules (safe defaults)
  error.rs         — PermissionError
```

---

### `blazecode-plugin-core` — Plugin Domain

**Purpose:** Plugin trait, discovery, lifecycle management.

**Public interface:**
```rust
#[async_trait]
pub trait Plugin: Send + Sync {
    fn name(&self) -> &str;
    fn version(&self) -> &str;

    async fn on_activate(&self, context: &PluginContext) -> Result<()>;
    async fn on_deactivate(&self) -> Result<()>;

    fn registered_tools(&self) -> Vec<Box<dyn Tool>>;
    fn registered_providers(&self) -> Vec<Box<dyn Provider>>;
    fn registered_context_sources(&self) -> Vec<Box<dyn ContextSource>>;
}

pub struct PluginRegistry {
    plugins: Vec<Box<dyn Plugin>>,
}

impl PluginRegistry {
    pub fn discover(path: &Path) -> Result<Vec<Box<dyn Plugin>>>;
    pub fn activate(&mut self, plugin: Box<dyn Plugin>) -> Result<()>;
    pub fn deactivate(&mut self, name: &str) -> Result<()>;
}
```

**Dependencies:** `blazecode-types`, `blazecode-error`, `blazecode-tool-core`, `blazecode-provider-core`

**External deps:** `async-trait`

---

### `blazecode-plugin-wasm` — WASM Plugin Sandbox

**Purpose:** Load plugins compiled to WASM, sandbox execution with controlled file/network access.

**Public interface:**
```rust
pub struct WasmPlugin {
    instance: wasmtime::Instance,
    sandbox: Sandbox,
}

impl WasmPlugin {
    pub fn load(wasm_bytes: &[u8], permissions: PluginPermissions) -> Result<Self>;
}

pub struct PluginPermissions {
    pub allowed_paths: Vec<PathBuf>,
    pub allowed_hosts: Vec<String>,
    pub max_memory: usize,
    pub max_cpu_time: Duration,
}
```

**Dependencies:** `blazecode-plugin-core`, `blazecode-types`, `blazecode-error`

**External deps:** `wasmtime`, `wit`

---

### `blazecode-event-store` — EventV2 Event Sourcing

**Purpose:** Durable event store with replay, cursors, and pub/sub.

**Public interface:**
```rust
#[async_trait]
pub trait EventStore: Send + Sync {
    async fn append(&self, stream: &StreamId, events: &[DomainEvent]) -> Result<SeqNumber>;
    async fn read(&self, stream: &StreamId, after: Option<SeqNumber>) -> Result<Vec<StoredEvent>>;
    async fn subscribe(&self, stream: &StreamId) -> Result<Pin<Box<dyn Stream<Item = StoredEvent>>>>;
}

pub struct StoredEvent {
    pub seq: SeqNumber,
    pub stream: StreamId,
    pub event: DomainEvent,
    pub timestamp: Timestamp,
}
```

**Dependencies:** `blazecode-types`, `blazecode-error`, `blazecode-database`

**External deps:** `async-trait`, `futures`, `tokio`

---

### `blazecode-cli` — CLI Library Crate

**Purpose:** All CLI command handlers as public async functions. Thin binary dispatches to this.

**Public interface:**
```rust
pub async fn dispatch(cli: Cli) -> i32;
pub async fn cmd_run(args: RunArgs) -> Result<i32>;
pub async fn cmd_serve(args: ServeArgs) -> Result<i32>;
pub async fn cmd_session(args: SessionArgs) -> Result<i32>;
pub async fn cmd_version() -> i32;
```

**Dependencies:** all domain crates, `blazecode-config`, `blazecode-observability`

**External deps:** `clap`, `tokio`

### `blazecode-server` — HTTP/SSE Server

**Purpose:** Axum-based HTTP server for remote session management, SSE streaming.

**Dependencies:** `blazecode-session-core`, `blazecode-config`, `blazecode-observability`, `blazecode-types`

**External deps:** `axum`, `tower`, `tokio`, `serde_json`

### `blazecode-tui` — Terminal UI

**Purpose:** Ratatui-based terminal user interface. Reuses `blazecode-session-core`.

**Dependencies:** `blazecode-session-core`, `blazecode-config`, `blazecode-types`

**External deps:** `ratatui`, `crossterm`, `tokio`

### `blazecode-lsp` — LSP Integration

**Purpose:** LSP server for IDE integration.

**Dependencies:** `blazecode-session-core`, `blazecode-tool-core`, `blazecode-config`

**External deps:** `tower-lsp`, `serde_json`, `tokio`

### `blazecode-mcp` — MCP Integration

**Purpose:** Model Context Protocol server for MCP-compatible tools.

**Dependencies:** `blazecode-tool-core`, `blazecode-config`, `blazecode-types`

**External deps:** `serde`, `serde_json`

### `blazecode-sdk` — Public SDK

**Purpose:** Re-export all public APIs for Rust consumers. Single `blazecode` crate that bundles the public surface.

```rust
pub use blazecode_types::*;
pub use blazecode_session_core::{Session, SessionManager, SessionConfig};
pub use blazecode_tool_core::{Tool, ToolRegistry, ToolContext};
pub use blazecode_provider_core::{Provider, ChatRequest, LlmEvent};
pub use blazecode_config::Config;
pub use blazecode_error::{Error, Result};
```

---

## 5. Domain Boundaries

### Bounded Context Map

```
┌─────────────────────────────────────────────────────────────────────┐
│                                                                     │
│  ┌────────────────────┐    ┌────────────────────┐                   │
│  │  Configuration     │    │    Storage         │                   │
│  │  Domain            │    │    Domain          │                   │
│  │                    │    │                    │                   │
│  │  Entities: Config  │    │  Entities:         │                   │
│  │  Value Obj:        │    │    SessionRow      │                   │
│  │    ProviderConfig  │    │    MessageRow      │                   │
│  │    AgentConfig     │    │    EventRow        │                   │
│  │  Service:          │    │  Service:          │                   │
│  │    ConfigLoader    │    │    DatabaseService │                   │
│  │    ConfigValidator │    │  Ports: Database   │                   │
│  └────────┬───────────┘    └─────────┬──────────┘                   │
│           │                          │                             │
│           │                          │                             │
│  ┌────────┴──────────────────────────┴──────────┐                  │
│  │              Session Domain                   │                  │
│  │                                                │                 │
│  │  Aggregate: Session { id, messages, epoch }   │                 │
│  │  Value Obj: Message, Part, PromptInput,       │                 │
│  │             ContextEpoch, ContextBaseline     │                 │
│  │  Domain Events: SessionCreated,               │                 │
│  │    MessageAppended, ToolExecuted,             │                 │
│  │    EpochAdvanced, Compacted                   │                 │
│  │  Domain Svc: SessionManager                   │                 │
│  │  App Svc: RunCoordinator, TurnProcessor       │                 │
│  └────────┬──────────────────────────────────────┘                 │
│           │                                                        │
│           │                                                        │
│  ┌────────┴──────────┐    ┌────────────────────┐                  │
│  │   Tool Domain      │    │   Provider/LLM     │                  │
│  │                     │    │   Domain           │                  │
│  │  Entities:         │    │                    │                  │
│  │    ToolRegistry    │    │  Entities:         │                  │
│  │  Value Obj:        │    │    Model, Provider │                  │
│  │    ToolCall        │    │  Value Obj:        │                  │
│  │    ToolResult      │    │    ChatRequest     │                  │
│  │    ToolOutput      │    │    LlmEvent        │                  │
│  │  Ports: FileSystem │    │    StreamChunk     │                  │
│  │  Domain Svc:       │    │  Ports: HttpClient │                  │
│  │    ToolRegistry    │    │    Protocol, Auth  │                  │
│  │    ExecutionPipe   │    │    Framing         │                  │
│  └────────┬───────────┘    └────────┬───────────┘                  │
│           │                         │                             │
│           │                         │                             │
│  ┌────────┴──────────┐    ┌────────┴───────────┐                  │
│  │   Plugin Domain    │    │  Permission Domain │                  │
│  │                    │    │                    │                  │
│  │  Entities:        │    │  Entities:         │                  │
│  │    PluginRegistry │    │    RuleEngine      │                  │
│  │  Value Obj:       │    │  Value Obj:        │                  │
│  │    PluginManifest │    │    PermissionRule  │                  │
│  │    PluginConfig   │    │    PermissionReq   │                  │
│  │  Ports: Plugin    │    │    Policy          │                  │
│  │  App Svc:         │    │  Domain Svc:       │                  │
│  │    WasmSandbox    │    │    PermissionSvc   │                  │
│  │    PluginLoader   │    │                    │                  │
│  └───────────────────┘    └────────────────────┘                  │
│                                                                     │
│  ┌────────────────────────────────────────────────────┐            │
│  │              Integration Domain                     │            │
│  │                                                     │            │
│  │  Adapters: blazecode-server (axum HTTP/SSE)          │            │
│  │            blazecode-tui (ratatui)                   │            │
│  │            blazecode-lsp (tower-lsp)                 │            │
│  │            blazecode-mcp (MCP protocol)              │            │
│  └─────────────────────────────────────────────────────┘            │
│                                                                     │
└─────────────────────────────────────────────────────────────────────┘
```

### Domain Summaries

| Domain | Crates | Aggregate Root | Key Events | Ports (Traits) |
|--------|--------|---------------|------------|---------------|
| Session | `blazecode-session-core` | `Session` | Created, MessageAppended, ToolExecuted, Compacted, EpochAdvanced | `Database`, `EventStore` |
| Tool | `blazecode-tool-core`, `blazecode-tool-impls` | `ToolRegistry` | ToolExecuting, ToolExecuted, ToolFailed | `FileSystem`, `PermissionEvaluator` |
| Provider | `blazecode-provider-core`, `blazecode-provider-*` | `ProviderCatalog` | StreamStarted, ChunkReceived, StreamComplete | `HttpClient` |
| Plugin | `blazecode-plugin-core`, `blazecode-plugin-wasm` | `PluginRegistry` | PluginActivated, PluginDeactivated, PluginError | — |
| Config | `blazecode-config` | — | ConfigLoaded, ConfigChanged | — |
| Storage | `blazecode-database` | — | — | `Database` |
| Permission | `blazecode-permission` | `RuleEngine` | PermissionChecked, PermissionDenied | — |
| Integration | `blazecode-server`, `blazecode-tui`, `blazecode-lsp`, `blazecode-mcp` | — | — | — |

---

## 6. Integration Architecture

### Domain Event Flow

```
                      ┌──────────────────────────┐
                      │       Event Bus           │
                      │  (in-memory broadcast)    │
                      └──┬───────┬───────┬───────┘
                         │       │       │
              ┌──────────┘       │       └──────────┐
              ▼                  ▼                  ▼
      ┌──────────────┐  ┌──────────────┐  ┌──────────────┐
      │ Session      │  │ Tool         │  │ Observability│
      │ Domain       │  │ Domain       │  │ (subscriber) │
      └──────────────┘  └──────────────┘  └──────────────┘

Events published:
  SessionCreated       → ToolDomain: register session tools
  ToolExecuted         → SessionDomain: append result to messages
  ProviderStreamChunk  → SessionDomain: update message with delta
  PermissionDenied     → SessionDomain: notify user
  PluginActivated      → ToolDomain: register plugin tools
```

### Event Bus Design

```rust
pub struct EventBus {
    tx: broadcast::Sender<DomainEvent>,
}

impl EventBus {
    pub fn publish(&self, event: DomainEvent);
    pub fn subscribe(&self) -> broadcast::Receiver<DomainEvent>;
}

// Optional distributed event bus for multi-instance deployment:
pub struct DistributedEventBus {
    local: EventBus,
    remote: Option<Arc<dyn RemoteEventBus>>,  // NATS, Kafka, etc.
}
```

### Cross-Domain Communication Rules

1. **Domains never import each other's crates directly**
2. All cross-domain communication is via `DomainEvent` on the bus
3. Each domain owns its data — no shared mutable state
4. `SessionDomain` is the orchestrator: it receives events from Tool/Provider domains and advances the session state

### API Contracts Between Domains

```
SessionDomain         → ToolDomain:   ToolCall { id, name, args }
ToolDomain            → SessionDomain: ToolResult { call_id, output, error? }

SessionDomain         → ProviderDomain: ChatRequest { model, messages, tools }
ProviderDomain        → SessionDomain: LlmEvent { delta, stop_reason, usage? }

SessionDomain         → PermissionDomain: PermissionRequest { action, target }
PermissionDomain      → SessionDomain: PermissionResult { allowed | denied | needs_approval }

SessionDomain         → StorageDomain: SessionRow / MessageRow (via Database trait)
```

### Database Strategy

| Data | Domain Owner | Storage | Backend |
|------|-------------|---------|---------|
| Session metadata | Session | `sessions` table | SQLite (default), PostgreSQL (server) |
| Messages | Session | `messages` table | SQLite (default), PostgreSQL (server) |
| Events (EventV2) | Session | `events` table | SQLite (default), PostgreSQL (server) |
| Tool call history | Tool | `tool_calls` table | SQLite (default), PostgreSQL (server) |
| Plugin state | Plugin | `plugins` table | SQLite (default) |
| Config | Config | File system | TOML file |

**Default: shared SQLite database** with per-domain table namespacing. Server mode: PostgreSQL with schema-per-domain or shared schema.

**Why not database-per-domain:** For a CLI tool running locally, multiple databases create unnecessary complexity. In server mode, PostgreSQL supports schema-per-domain within a single database instance.

---

## 7. Deployment Architecture

### Option A: CLI (Primary)

```
┌─────────────────────────────────────┐
│         blazecode binary (~15MB)      │
│                                     │
│  ┌─────────────────────────────┐    │
│  │  blazecode-cli (thin CLI)    │    │
│  └─────────────┬───────────────┘    │
│                │                    │
│  ┌─────────────▼───────────────┐    │
│  │  All domain crates (static) │    │
│  │  + SQLite (local data)      │    │
│  │  + Local filesystem         │    │
│  └─────────────────────────────┘    │
│                                     │
│  ~/.blazecode/                       │
│    config.toml                      │
│    data/                            │
│      blazecode.db (SQLite)           │
│      sessions/                      │
│      plugins/                       │
└─────────────────────────────────────┘
```

**Distribution:** Single binary via `cargo install blazecode` or GitHub releases.

### Option B: Server (Multi-Instance)

```
┌─────────────┐  ┌─────────────┐  ┌─────────────┐
│ blazecode    │  │ blazecode    │  │ blazecode    │
│ server 1    │  │ server 2    │  │ server 3    │
│ (axum)      │  │ (axum)      │  │ (axum)      │
└──────┬──────┘  └──────┬──────┘  └──────┬──────┘
       │                │                │
       └────────────────┼────────────────┘
                        │
              ┌─────────▼─────────┐
              │   PostgreSQL       │
              │   (shared state)   │
              └───────────────────┘

Optional:
  ┌─────────────┐  ┌─────────────┐
  │  NATS/Redis  │  │  S3/MinIO   │
  │  (event bus) │  │  (session   │
  │              │  │   storage)  │
  └─────────────┘  └─────────────┘
```

### Option C: Desktop (Future)

```
┌─────────────────────────────────────┐
│         Electron Shell              │
│  ┌─────────────────────────────┐    │
│  │  Built-in blazecode binary   │    │
│  │  (bundled per-platform)     │    │
│  └─────────────────────────────┘    │
│                                     │
│  OR: Sidecar process                │
│  Rust binary runs as child process  │
│  Electron provides UI shell only    │
└─────────────────────────────────────┘
```

**Recommendation:** Skip Electron. Focus on CLI + VS Code extension via LSP. Electron is high maintenance with poor Rust fit.

### Option D: Web (Future)

```
┌─────────────┐  ┌─────────────┐
│ Browser     │  │ Browser     │
│ (Next.js)   │  │ (Next.js)   │
└──────┬──────┘  └──────┬──────┘
       │                │
       │    HTTPS/SSE   │
       └────────────────┘
               │
       ┌───────▼───────┐
       │ blazecode      │
       │ server        │
       │ (axum)        │
       └───────┬───────┘
               │
       ┌───────▼───────┐
       │  PostgreSQL   │
       └───────────────┘
```

**WASM build of TUI:** Feasible but not recommended for initial release. Rust TUI tools (`ratatui`) are terminal-native and don't compile to WASM without significant effort.

---

## 8. Migration Strategy

### Phase 0: Preparation (Current — Week 2)

**Goal:** Stop the bleeding. Fix critical bugs, restore dead-code detection.

**Activities:**
- Fix `clear_revert` SQL NULL bug (QW-2)
- Fix `unwrap()` + `Some()` JSON corruption (QW-5)
- Fix V1 permission bypass (QW-4)
- Fix f64 cost precision (QW-8)
- Restore dead-code detection (QW-1)
- Replace 19-param `update_session` with `SessionUpdate` struct (QW-3)
- Unify error hierarchies with `#[from]` impls (QW-9)
- `Arc<Vec<ChatMessage>>` in ToolContext (QW-6)
- `ok_or_500()` helper for server routes (QW-10)

**Deliverables:**
- All critical bugs fixed
- `#![allow(dead_code)]` removed from crate roots
- Dead items gated with `#[cfg(scaffold)]` or removed
- `SessionUpdate` struct replaces 19 positional params

**Architecture Score:** 25 → 35

```
 ┌────────────────────────────────────┐
 │         PHASE 0 (Week 1-2)         │
 │  ┌─────┐ ┌─────┐ ┌─────┐ ┌─────┐ │
 │  │Bug  │ │Dead │ │Error│ │Perf │ │
 │  │Fixes│ │Code │ │Unify│ │     │ │
 │  └─────┘ └─────┘ └─────┘ └─────┘ │
 └────────────────────────────────────┘
```

---

### Phase 1: Module Restructuring (Weeks 3-6)

**Goal:** Enforce visibility discipline, split monolithic files, introduce newtype IDs.

**Activities:**
1. **Module visibility discipline (MR-1):**
   - Audit 95 modules → classify as public API vs internal
   - Internal modules: `pub(crate) mod`
   - Define clean `lib.rs` re-export surface
   - Downstream crates import from re-exports only

2. **Split monolithic modules (MR-2):**
   - Break `session.rs` (1,481 lines) into `session/` directory
   - Break `provider.rs` (1,511 lines) into `provider/` directory
   - Break `database.rs` (4,758 lines) into `database/` directory
   - Break `config.rs` (1,408 lines) into `config/` directory
   - Break `plugin.rs` (1,511 lines) into `plugin/` directory
   - Break `filesystem.rs` (1,557 lines) into `filesystem/` directory
   - Break `permission.rs` (1,382 lines) into `permission/` directory
   - Target: <300 lines per sub-module

3. **Newtype IDs (MR-7):**
   - `SessionId(String)`, `MessageId(String)`, etc.
   - Validation at construction
   - `#[serde(transparent)]` for serialization compat
   - Update all function signatures

**Deliverables:**
- `lib.rs` exports ~15-20 public items (not 95 modules)
- `pub(crate)` applied to 70+ modules
- 14 files >1000 lines → 0 files >1000 lines
- Type-safe IDs: compiler catches ID confusion
- Architecture score: 35 → 50

```
 ┌────────────────────────────────────┐
 │         PHASE 1 (Weeks 3-6)        │
 │  ┌──────────────┐ ┌──────────────┐ │
 │  │ pub(crate)   │ │ Directory    │ │
 │  │ Visibility   │ │ Modules      │ │
 │  │ Discipline   │ │ (splitting)  │ │
 │  └──────────────┘ └──────────────┘ │
 │  ┌──────────────┐                  │
 │  │ Newtype IDs  │                  │
 │  └──────────────┘                  │
 └────────────────────────────────────┘
```

---

### Phase 2: Infrastructure Decoupling (Weeks 7-12)

**Goal:** Extract all infrastructure behind trait boundaries. Core depends on ports only.

**Activities:**
1. **Database trait extraction (MR-3):**
   - Define `Database` trait in `blazecode-database` crate
   - Move SQLite impl to `blazecode-database-sqlite`
   - Core accepts `Arc<dyn Database>` instead of `Arc<DatabaseService>`
   - Create `MockDatabase` for tests

2. **HTTP client trait extraction (MR-4):**
   - Define `HttpClient` trait in `blazecode-http` crate
   - Implement `ReqwestHttpClient` adapter
   - Provider implementations use `Arc<dyn HttpClient>`
   - Add 120s timeout + retry policy

3. **Filesystem trait extraction (MR-6):**
   - Define `FileSystem` trait in `blazecode-filesystem` crate
   - Implement `TokioFileSystem` using `tokio::fs`
   - Tool implementations use `Arc<dyn FileSystem>`
   - Replace blocking `std::fs` calls with async equivalents

4. **Plugin SDK extraction (MR-5):**
   - Create `blazecode-plugin-sdk` with minimal dependencies
   - Extract `Plugin` trait, `PluginManager` trait
   - Core depends on plugin-sdk (inverted dependency)

**Deliverables:**
- `blazecode-core` no longer imports `sqlx`, `reqwest`, `std::fs`
- All infrastructure behind trait boundaries
- `MockDatabase`, `MockHttpClient`, `MockFileSystem` for testing
- Test coverage: <2% → ~30%
- Architecture score: 50 → 65

```
 ┌────────────────────────────────────┐
 │      PHASE 2 (Weeks 7-12)          │
 │  ┌────────┐ ┌────────┐ ┌────────┐ │
 │  │Database│ │ HTTP   │ │ File-  │ │
 │  │ Trait  │ │ Client │ │ system │ │
 │  └────────┘ └────────┘ └────────┘ │
 │  ┌────────┐                        │
 │  │ Plugin │                        │
 │  │ SDK    │                        │
 │  └────────┘                        │
 └────────────────────────────────────┘
```

---

### Phase 3: Crate Extraction (Months 4-5)

**Goal:** Split monolithic `blazecode-core` into 8+ domain crates.

**Activities:**
1. **Create `blazecode-types`** — extract shared types, newtypes, error types
2. **Create `blazecode-config`** — extract config loading, parsing, validation
3. **Create `blazecode-provider-core`** — extract Provider trait, route architecture
4. **Create `blazecode-session-core`** — extract session model, event sourcing
5. **Create `blazecode-tool-core`** — extract Tool trait, execution pipeline
6. **Create `blazecode-permission`** — extract permission evaluation
7. **Create `blazecode-event-store`** — extract EventV2 event sourcing
8. **Extract CLI library (AR-2):**
   - Create `blazecode-cli` library crate
   - Move all `cmd_*` functions from `main.rs`
   - Reduce `main.rs` to ~30 lines

**Deliverables:**
- 8+ domain crates with explicit dependencies
- `main.rs` < 50 lines (thin CLI dispatch)
- Each crate independently testable
- Build times improve (change one crate doesn't recompile all)
- Architecture score: 65 → 75

```
 ┌────────────────────────────────────┐
 │      PHASE 3 (Months 4-5)          │
 │                                     │
 │  ┌─────┐ ┌─────┐ ┌─────┐ ┌─────┐ │
 │  │types│ │conf │ │prov │ │sess │ │
 │  └─────┘ └─────┘ └─────┘ └─────┘ │
 │  ┌─────┐ ┌─────┐ ┌─────┐ ┌─────┐ │
 │  │tool │ │perm │ │event│ │cli  │ │
 │  └─────┘ └─────┘ └─────┘ └─────┘ │
 └────────────────────────────────────┘
```

---

### Phase 4: Advanced Patterns (Months 6-8)

**Goal:** Structured concurrency, async trait hygiene, advanced providers.

**Activities:**
1. **Structured concurrency (AR-3):**
   - Create `ScopedFiberSet<T>` with auto-cancel on drop
   - Replace `DashMap<FiberId, JoinHandle<()>>` with scoped fibers
   - Wire `CancellationToken` propagation through scope chain
   - No more fiber leaks

2. **Async trait hygiene (AR-5):**
   - GAT-based `Provider::Stream` type
   - Clean `Send + Sync + 'static` bounds everywhere
   - Replace `ClosureProviderPlugin` function-pointer soup with trait

3. **Provider crate creation:**
   - `blazecode-provider-anthropic`
   - `blazecode-provider-openai`
   - `blazecode-provider-gemini`
   - `blazecode-provider-openai-compatible` (14 variants)

**Deliverables:**
- Deterministic shutdown, no fiber leaks
- Zero-cost static dispatch for provider streams
- 4+ provider crates implementing the route architecture
- Architecture score: 75 → 80

---

### Phase 5: Plugin + WASM Support (Months 9-10)

**Goal:** WASM plugin sandbox, published plugin SDK, proc-macro tool definitions.

**Activities:**
1. **WASM plugin sandbox (SR-5 variant):**
   - `blazecode-plugin-wasm` crate with `wasmtime`
   - WIT interface for plugin ↔ host communication
   - Sandbox with controlled file/network access

2. **Proc-macro crate:**
   - `blazecode-derive` with `#[derive(Tool)]`
   - `#[tool(description = "...")]` attribute macro on functions
   - `#[derive(Provider)]` for route-based providers

3. **Publish `blazecode-plugin-sdk` on crates.io**

**Architecture Score:** 80 → 83

---

### Phase 6: Multi-Instance Server (Months 11-12)

**Goal:** Production-grade server mode with PostgreSQL, distributed event bus, session sync.

**Activities:**
1. `blazecode-database-postgres` adapter
2. Distributed event bus (NATS/Redis backend)
3. Session migration from SQLite to PostgreSQL
4. Health checks, metrics export, structured audit logging

**Architecture Score:** 83 → 85

---

### Phase 7: Desktop/Web/IDE Extensions (Future)

**Goal:** Reach users on every platform.

**Activities:**
1. VS Code extension via LSP
2. Web frontend (optional)
3. Multi-platform desktop (Electron sidecar or Tauri)

---

## Migration Timeline

```
Week  1  2  3  4  5  6  7  8  9 10 11 12 13 14 15 16 17 18 19 20 21 22 23 24
Phase
P0    ██▓▓░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░
P1    ░░░░████████████▓▓░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░
P2    ░░░░░░░░░░░░░░░░████████████████▓▓░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░
P3    ░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░████████████████▓▓░░░░░░░░░░░░░░░░░░
P4    ░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░████████████▓▓░░░░░░
P5    ░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░████░░░░
P6    ░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░████
```

---

## Architecture Score Progression

```
Score
 85 │                                                              ██ P7
 80 │                                                  ██ P5      ██
 75 │                                      ██ P4      ██          ██
 70 │                          ██ P3      ██          ██          ██
 65 │              ██ P2      ██          ██          ██          ██
 60 │              ██          ██          ██          ██          ██
 55 │              ██          ██          ██          ██          ██
 50 │   ██ P1      ██          ██          ██          ██          ██
 45 │   ██          ██          ██          ██          ██          ██
 40 │   ██          ██          ██          ██          ██          ██
 35 │ ██ P0        ██          ██          ██          ██          ██
 30 │ ██           ██          ██          ██          ██          ██
 25 ██             ██          ██          ██          ██          ██
    └──────────────────────────────────────────────────────────────────
     0  1  2  3  4  5  6  7  8  9 10 11 12     Months

Current  25 ──► P0: 35 ──► P1: 50 ──► P2: 65 ──► P3: 75 ──► P4: 80 ──► P5: 83 ──► P6+: 85
```

---

## Key Metrics

| Metric | Current | P0 | P1 | P2 | P3 | P4 | P5 | P6+ |
|--------|---------|----|----|----|----|----|----|-----|
| Architecture Score | 25 | 35 | 50 | 65 | 75 | 80 | 83 | 85 |
| Public modules | 95 (all pub) | 95 | 30 pub / 65 pub(crate) | 30 / 65 | per-crate | per-crate | per-crate | per-crate |
| Files >1000 lines | 14 | 14 | 0 | 0 | 0 | 0 | 0 | 0 |
| Cargo crates | 6 | 6 | 6 | 8 | 15 | 18 | 22 | 27 |
| main.rs lines | 8,575 | 8,000 | 5,000 | 3,000 | <50 | <50 | <50 | <50 |
| unwrap() in lib | 100+ | 70 | 40 | ~5 | 0 | 0 | 0 | 0 |
| Test coverage | <2% | <5% | <10% | ~30% | ~50% | ~60% | ~70% | >80% |
| Error types | 5 separate | 5 + From | 5 + From | 1 unified | 1 | 1 | 1 | 1 |
| Database coupling | inline sqlx | inline | inline | trait | trait | trait | trait | trait |
| Structured concurrency | no | no | no | no | no | yes | yes | yes |
| WASM plugins | no | no | no | no | no | no | yes | yes |
| Proc-macro tools | no | no | no | no | no | yes | yes | yes |

---

## Appendix A: BlazeCode Moat — What BlazeCode Cannot Replicate

| Moat | Current | Target | Implementation |
|------|---------|--------|----------------|
| **Single binary** | ✅ Already have | ✅ Maintain | Static linking, `cargo install blazecode` |
| **Proc macros** | ❌ Not yet | ✅ P4-P5 | `blazecode-derive` crate with `#[derive(Tool)]` |
| **WASM sandbox** | ❌ Not yet | ✅ P5 | `wasmtime` + WIT interface |
| **Local AI** | ❌ Not yet | ✅ Future | `llama-cpp-rs` or `candle` binding |
| **Compile-time safety** | ✅ Already have | ✅ Maintain | `forbid(unsafe_code)`, type-state patterns |
| **Performance** | ✅ Partial | ✅ P4 | GAT streams, `Arc` sharing, ripgrep delegation |
| **Startup time** | ✅ Already have | ✅ Maintain | <10ms binary startup |

## Appendix B: Risk Register

| Risk | Impact | Likelihood | Mitigation |
|------|--------|-----------|------------|
| Phase 3 crate split breaks compilation | High | High | Incremental extraction, verify after each crate |
| Trait extraction reveals missing abstractions | Medium | High | Extend trait surface during extraction, keep adapters in core initially |
| WASM plugin sandbox conflicts with `forbid(unsafe_code)` | High | Medium | Evaluate `wasmtime` safety guarantees; `unsafe` in sandbox adapter only, not in core |
| Community migration from BlazeCode doesn't happen | High | Medium | Ship working CLI first, then add unique features (proc macros, WASM) |
| BlazeCode V2 evolves faster than BlazeCode ports | High | Medium | Focus on unique moats, not parity; implement CONTEXT.md rules as test cases |

## Appendix C: Glossary

| Term | Definition |
|------|-----------|
| Port | Trait that defines a boundary between domain and infrastructure |
| Adapter | Concrete implementation of a port (e.g., SqliteDatabase implements Database) |
| Bounded Context | A domain boundary within which a particular model is defined and applicable |
| Aggregate Root | An entity that guarantees consistency for a group of domain objects |
| Domain Event | An event that domain experts care about (e.g., "SessionCompacted") |
| Structured Concurrency | Pattern where async tasks are bounded by a scope and automatically cancelled on scope exit |
| Event Sourcing | Pattern where state changes are stored as an append-only log of events |
| Composition Root | The single place in an application where dependencies are wired together |
| Route Architecture | BlazeCode's pattern of composing a provider from orthogonal Protocol, Endpoint, Auth, Framing pieces |
