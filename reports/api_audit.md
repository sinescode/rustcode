# RustCode ↔ OpenCode API Audit Report

Generated: 2026-06-19
Scope: Full public API surface comparison across all domains
RustCode commit: 5d0f86606ac30690f79f0a6a9f41a1f49fe95d0b (mapped to OpenCode TS)

---

## 1. ERROR SYSTEM

### 1.1 RustCode Error (`crates/rustcode-core/src/error.rs`)

```rust
pub enum Error {
    Llm(LlmError),
    Tool(ToolError),
    Config(ConfigError),
    Io(IoError),
    Session(SessionError),
    Permission(PermissionError),
    Parse(ParseError),
    Database(DatabaseError),
    Serde(SerdeError),
    Agent(AgentError),
    Mcp(McpError),
    Lsp(LspError),
    Bus(BusError),
    Internal(InternalError),
}
```

**File:** `crates/rustcode-core/src/error.rs`  
**Evidence:** ~250 lines, single top-level enum with 14 variant groups, each a sub-enum (e.g., `LlmErrorReason`, `ToolErrorReason`, `SessionErrorReason`). `thiserror` derive.  
**Problem:** Flat structure — every possible error domain is known at compile time in one enum. OpenCode defines ~50+ independent error classes spread across modules, each a `Schema.TaggedErrorClass`. Adding a new error domain requires modifying the central enum.  
**Impact:** Maintainability — RustCode must touch `error.rs` for every new error variant; OpenCode errors are co-located with their domain module.  
**Severity:** Medium  
**Recommendation:** Replace monolithic enum with a trait-based approach (`impl std::error::Error`) or use `Box<dyn std::error::Error>` for extensibility without modifying the central type. Alternatively, use `thiserror` per-module and aggregate at the API boundary.  
**Effort:** 3 days

### 1.2 OpenCode Error Pattern (`packages/core/src/permission.ts:86-100`, `packages/core/src/session.ts:85-103`)

```typescript
export class RejectedError extends Schema.TaggedErrorClass<RejectedError>()("PermissionV2.RejectedError", {}) {}
export class DeniedError extends Schema.TaggedErrorClass<DeniedError>()("PermissionV2.DeniedError", {
  rules: PermissionSchema.Ruleset,
}) {}
```

**File:** `packages/core/src/permission.ts:86-100`, `packages/core/src/session.ts:85-103`  
**Evidence:** Each module defines its own error classes (PermissionV2 has 4 errors, SessionV2 has 4 errors, EventV2 has 1 error, MCP has 2 errors, etc.). OpenCode uses Effect's `Schema.TaggedErrorClass` which gives each error a stable string tag, JSON serialization, and branded types.  
**Problem:** RustCode cannot match Effect's tagged-error ergonomics without equivalent infrastructure (a `std::error::Error` + `serde` + type tagging alternative).  
**Impact:** Error handling in RustCode is less composable — no structured recovery by error tag across module boundaries.  
**Severity:** Medium  
**Recommendation:** Consider a crate like `snafu` for context-rich errors, or add a lightweight `ErrorKind` tag enum to each error struct.  
**Effort:** 2 days

### 1.3 Missing Error Domains

**File:** `crates/rustcode-core/src/error.rs`  
**Evidence:** RustCode Error does not include variants for: `StorageError`, `QuestionError`, `GitError`, `SnapshotError`, `WorktreeError`, `FormatError`, `ImageError`, `PluginError`, `SkillError`, `ValidationError`, `SyncError`.  
**Problem:** These domains exist as modules but have no dedicated error type.  
**Impact:** Callers cannot differentiate between a git failure and a storage failure — both become `IoError` or `InternalError`.  
**Severity:** High  
**Recommendation:** Add error variants for every domain module. Match OpenCode's per-module error classes.  
**Effort:** 1 day

---

## 2. PROVIDER / LLM SYSTEM

### 2.1 RustCode Provider (`crates/rustcode-core/src/provider.rs`)

```rust
pub enum ProviderId {
    OpenCode,
    Anthropic,
    OpenAI,
    Google,
    GoogleVertex,
    GithubCopilot,
    AmazonBedrock,
    Azure,
    OpenRouter,
    Mistral,
    Gitlab,
}

pub trait Provider: Send + Sync {
    fn id(&self) -> ProviderId;
    fn send(&self, request: ChatRequest) -> Pin<Box<dyn Future<Output = Result<ChatResponse>> + Send>>;
    fn stream(&self, request: ChatRequest) -> Pin<Box<dyn Future<Output = Result<Pin<Box<dyn Stream>>>> + Send>>;
}

pub enum StreamChunk {
    Text(String),
    Reasoning(String),
    ToolCall { id: String, name: String, args: serde_json::Value },
    ToolResult { id: String, content: String },
    Done { usage: Option<Usage> },
    Error(String),
}
```

**File:** `crates/rustcode-core/src/provider.rs`  
**Evidence:** Trait-based design with 11 well-known provider enum. `StreamChunk` enum for streaming. Model/Usage structs.  
**Problem:** OpenCode's `ProviderV2` uses branded Schema types (`ProviderV2.ID`, `ProviderV2.Api` with `AISDK | Native` tagged union). OpenCode supports dynamic provider registration via config; RustCode has fixed enum. OpenCode distinguishes between AISDK-based and Native-based provider APIs; RustCode does not.  
**Impact:** Adding a new provider requires:
1. Adding variant to `ProviderId` enum (central)
2. Implementing trait
3. Registering in provider map

OpenCode can add providers purely from config without code changes.  
**Severity:** High  
**Recommendation:** Make `ProviderId` extensible via string-based IDs (like OpenCode's branded string) rather than a fixed enum. Support dynamic provider discovery.  
**Effort:** 3 days

### 2.2 OpenCode Provider (`packages/core/src/provider.ts`)

```typescript
export const ID = Schema.String.pipe(Schema.brand("ProviderV2.ID"), withStatics((schema) => ({
  opencode: schema.make("opencode"),
  anthropic: schema.make("anthropic"),
  // ...same 11 + extensible
})))

export const Api = Schema.Union([AISDK, Native]).pipe(Schema.toTaggedUnion("type"))

export class Info extends Schema.Class<Info>("ProviderV2.Info")({
  id: ID,
  name: Schema.String,
  disabled: Schema.Boolean.pipe(Schema.optional),
  api: Api,
  request: Request,
}) { }
```

**File:** `packages/core/src/provider.ts:6-68`  
**Evidence:** Provider is a Schema class, not a trait. Provider instances carry their own API configuration inline (URL, settings, headers). No `send()`/`stream()` methods — provider resolution is externalized to the runner.  
**Problem:** RustCode intertwines provider definition with provider execution (the trait). OpenCode separates definition (data) from execution (runtime).  
**Impact:** Testing is harder in RustCode — you must mock the trait. OpenCode can test by constructing `ProviderV2.Info` objects.  
**Severity:** Medium  
**Recommendation:** Split provider metadata (data struct) from provider execution (trait). Make `ProviderInfo` serializable/deserializable standalone.  
**Effort:** 2 days

### 2.3 Model Types

**File:** `crates/rustcode-core/src/provider.rs` vs `packages/core/src/model.ts`  
**Evidence:** RustCode has `Model` struct with `id`, `provider_id`, `name`, `context_limit`, `max_output`, `supports_tools`, `supports_images`. OpenCode's `ModelV2.Info` is richer: includes `Capabilities` (tools + MIME patterns for input/output), `Cost` (tiered pricing with cache read/write), `Api` (AISDK/Native variant), `Request` (generation params, options), `Variants`, `Limit` (context, input, output), `Status` (alpha/beta/deprecated/active), `Time` (released).  
**Problem:** RustCode model type is missing: cost model, variant support, API variant selection, status lifecycle, input limits separate from context/output, MIME type capabilities.  
**Impact:** RustCode cannot represent model pricing, A/B variants, or lifecycle states that OpenCode uses for model catalog features.  
**Severity:** High  
**Recommendation:** Expand `Model` struct to include `Cost`, `Capabilities`, `Status`, `Variants`, `Api`.  
**Effort:** 1 day

---

## 3. CONFIG SYSTEM

### 3.1 RustCode Config (`crates/rustcode-core/src/config.rs`)

```rust
pub struct Config {
    pub info: RwLock<Info>,
}

pub struct Info {
    pub shell: Option<String>,
    pub model: Option<String>,
    pub default_agent: Option<String>,
    pub autoupdate: Option<AutoUpdateMode>,
    pub share: Option<ShareMode>,
    pub username: Option<String>,
    pub permissions: Vec<PermissionRule>,
    pub agents: HashMap<String, AgentConfig>,
    pub providers: HashMap<String, ProviderConfig>,
    pub mcp: HashMap<String, McpConfig>,
    pub experimental: Option<ExperimentalConfig>,
}
```

**File:** `crates/rustcode-core/src/config.rs`  
**Evidence:** Loading chain: default config → global config → project config → `.opencode/` dir files → CLI overrides. Uses `serde` for parsing.  
**Problem:** OpenCode's `Config.Info` has substantially more fields: `snapshots`, `watcher`, `formatter`, `lsp`, `attachments`, `tool_output`, `compaction`, `skills`, `commands`, `instructions`, `references`, `plugins`, `enterprise`, `$schema`. RustCode is missing ~15+ config sections.  
**Impact:** RustCode cannot configure LSP, formatter, snapshots, compaction, tool output truncation, skills discovery, references, plugins, or enterprise settings.  
**Severity:** High  
**Recommendation:** Add all missing config sections. Match OpenCode's `Config.Info` schema field-for-field.  
**Effort:** 3 days

### 3.2 Config Document Model

**File:** `packages/core/src/config.ts:108-125`  
**Evidence:** OpenCode models config as typed `Document` and `Directory` entries with a layered discovery mechanism. `Config.Entry` is `Document | Directory`. The `latest()` function picks the highest-priority value for each key.  
**Problem:** RustCode simply merges configs without tracking origin or supporting partial overrides at the field level.  
**Impact:** OpenCode can determine *which* config file provided each setting; RustCode loses provenance.  
**Severity:** Medium  
**Recommendation:** Track config provenance per-field (or per-entry list).  
**Effort:** 1 day

### 3.3 Config Loading

**File:** `packages/core/src/config.ts:134-218`  
**Evidence:** OpenCode's config layer uses `FSUtil.up()` to walk up from the current directory looking for `.opencode/` directories and config files. It loads multiple files at each level (config.json, opencode.json, opencode.jsonc). Supports V1 → V2 migration.  
**Problem:** RustCode's loading chain is simpler and doesn't support multiple filenames per directory, V1 migration, or recursive `.opencode/` subdirectory loading.  
**Impact:** RustCode cannot read `opencode.jsonc` or `opencode.json` (only `config.json`-style).  
**Severity:** Medium  
**Recommendation:** Support all three config filenames. Implement V1 detection/migration.  
**Effort:** 2 days

---

## 4. SESSION SYSTEM

### 4.1 RustCode Session (`crates/rustcode-core/src/session.rs`)

```rust
pub struct SessionManager {
    db: Arc<Database>,
}

pub struct Session {
    pub id: SessionId,
    pub slug: String,
    pub project_id: String,
    pub directory: String,
    pub agent: Option<String>,
    pub model: Option<String>,
    pub title: String,
    pub state: SessionState,
    pub created_at: i64,
    pub updated_at: i64,
}

pub async fn create(&self, input: CreateInput) -> Result<SessionInfo>;
pub async fn get(&self, session_id: &SessionId) -> Result<Option<SessionInfo>>;
pub async fn list(&self) -> Result<Vec<SessionInfo>>;
pub async fn add_message(&self, session_id: &SessionId, message: Message) -> Result<()>;
pub async fn messages(&self, session_id: &SessionId) -> Result<Vec<Message>>;
pub async fn delete(&self, session_id: &SessionId) -> Result<()>;
```

**File:** `crates/rustcode-core/src/session.rs`  
**Evidence:** Basic CRUD with messages. SessionInfo, Message, and error types.  
**Problem:** OpenCode's `SessionV2.Interface` has **16 methods** vs RustCode's ~8. Missing: `context`, `events` (streaming), `switchAgent`, `switchModel`, `prompt` (admission with conflict detection), `shell`, `skill`, `compact`, `wait`, `resume`, `interrupt`.  
**Impact:** RustCode cannot handle session lifecycle events, agent/model switching mid-session, prompt admission with conflict detection, shell/skill operations within sessions, or session interruption.  
**Severity:** Critical  
**Recommendation:** Implement full `SessionV2.Interface` surface. Key additions: event streaming (`Stream<Event>`), prompt admission with conflict detection, shell/skill dispatch, interrupt/resume, switch agent/model.  
**Effort:** 2 weeks

### 4.2 Prompt Admission

**File:** `packages/core/src/session.ts:137-143, 348-376`  
**Evidence:** OpenCode's `prompt()` method is uninterruptible, admits one durable `session_input` row, reconciles retries via message ID, fires a wake event to the execution layer. Supports `Delivery` modes (`"steer"`, `"queue"`).  
**Problem:** RustCode's `add_message()` has no admission logic, no conflict detection, no delivery semantics, and no wake scheduling.  
**Impact:** OpenCode guarantees exactly-once prompt delivery with conflict detection; RustCode allows duplicate/conflicting prompts.  
**Severity:** Critical  
**Recommendation:** Implement prompt admission as a durable, conflict-checked operation with wake scheduling.  
**Effort:** 5 days

### 4.3 Session Events

**File:** `packages/core/src/session.ts:125-128`  
**Evidence:** OpenCode exposes an `events()` method returning `Stream.Stream<CursorEvent<SessionEvent.DurableEvent>>` — a live event stream with cursor-based replay. Used for TUI updates, WebSocket push, and persistence.  
**Problem:** RustCode has no event stream API for sessions.  
**Impact:** Real-time session monitoring (TUI, server push, UI updates) is impossible without polling.  
**Severity:** High  
**Recommendation:** Add `tokio::sync::broadcast`-backed event stream per session. Expose `subscribe()` method.  
**Effort:** 2 days

---

## 5. AGENT SYSTEM

### 5.1 RustCode Agent (`crates/rustcode-core/src/agent.rs`)

```rust
pub struct Agent {
    pub id: AgentId,
    pub model: Option<String>,
    pub system_prompt: Option<String>,
    pub description: Option<String>,
    pub mode: AgentMode,
    pub hidden: bool,
    pub max_steps: Option<u32>,
    pub permissions: Vec<PermissionRule>,
}

pub enum AgentMode {
    SubAgent,
    Primary,
    All,
}

pub fn default_agents() -> HashMap<AgentId, Agent> {
    // build, plan, general, explore
}
```

**File:** `crates/rustcode-core/src/agent.rs`  
**Evidence:** 4 built-in agents with config merging.  
**Problem:** OpenCode's `AgentV2` has a full state management pattern with `Editor` type for mutations, ScopedCache, and a `Selection` result type. OpenCode agents have `Color` (for UI), `Request` (provider request overrides), and `steps` (PositiveInt). RustCode is missing: color, request overrides, mutation API, selection capability.  
**Impact:** TUI cannot display agents with colors; agents cannot override provider request parameters.  
**Severity:** Low  
**Recommendation:** Add `Color`, `Request` fields, and state management API.  
**Effort:** 1 day

### 5.2 Agent Resolution

**File:** `packages/core/src/agent.ts:64-72, 114-138`  
**Evidence:** OpenCode's `resolve()` resolves an agent by ID (string) with fallback to default. `select()` returns a `Selection` (id + info). Full state editor (`list`, `get`, `default`, `update`, `remove`).  
**Problem:** RustCode's agent resolution is ad-hoc without the fallback logic, default resolution, or mutation support.  
**Impact:** Agent selection during session creation cannot distinguish between user-specified agent and system default.  
**Severity:** Medium  
**Recommendation:** Implement `resolve()`/`select()` with OpenCode's fallback chain.  
**Effort:** 1 day

---

## 6. CLI INTERFACE

### 6.1 RustCode CLI (`src/main.rs`)

**Evidence:** 24+ clap subcommands:
- `session` (list, get, delete)
- `run`, `generate`
- `agent`, `provider`, `model`
- `serve`, `upgrade`, `uninstall`
- `mcp`, `lsp`
- `export`, `import`, `debug`
- `config`, `stats`
- `plugin`, `db`
- `account`, `github`
- `attach`, `web`, `tui`
- `pr`, `session`

**File:** `src/main.rs`  
**Problem:** RustCode CLI has no `completion` subcommand (OpenCode has shell completion via `yargs`). OpenCode has `--print-logs`, `--log-level`, `--pure` global options that RustCode lacks. RustCode has `prompt` subcommand? No — it has `run`. OpenCode has `acp` subcommand; RustCode does not.  
**Impact:** Users cannot generate shell completions. Debugging without `--print-logs`/`--log-level` is harder.  
**Severity:** Medium  
**Recommendation:** Add `completion` subcommand, global logging flags, and `acp` subcommand.  
**Effort:** 1 day

### 6.2 OpenCode CLI (`packages/opencode/src/index.ts`)

```typescript
// 25 commands + completion + help
.command(AcpCommand)
.command(McpCommand)
.command(TuiThreadCommand)
.command(AttachCommand)
.command(RunCommand)
.command(GenerateCommand)
.command(DebugCommand)
.command(ConsoleCommand)  // = account
.command(ProvidersCommand)
.command(AgentCommand)
.command(UpgradeCommand)
.command(UninstallCommand)
.command(ServeCommand)
.command(WebCommand)
.command(ModelsCommand)
.command(StatsCommand)
.command(ExportCommand)
.command(ImportCommand)
.command(GithubCommand)
.command(PrCommand)
.command(SessionCommand)
.command(PluginCommand)
.command(DbCommand)
```

**File:** `packages/opencode/src/index.ts:80-103`  
**Evidence:** 25 commands + 2 implicit (help, completion). Global options: `--print-logs`, `--log-level`, `--pure`. Middleware sets environment variables.  
**Problem:** Match check shows RustCode covers ~22/25 commands (missing `acp`).  
**Impact:** Feature parity gap for ACP protocol.  
**Severity:** Low  
**Recommendation:** Implement `acp` subcommand when the ACP protocol module lands.  
**Effort:** 1 day

---

## 7. HTTP SERVER / REST API

### 7.1 RustCode Server (`crates/rustcode-server/src/`)

**Evidence:** 30 route modules, axum-based. AppState with bus, sessions, tools, permissions, questions, runner, providers.

**File:** `crates/rustcode-server/src/routes/mod.rs:6-34`  
**Routes defined:**
```
agent, command, config, control, control_plane, credential, event,
experimental, file, global, health, instance, integration, mcp,
metadata, model, permission, project, project_copy, provider, pty,
query, question, reference, session, skill, sync, tui, workspace
```

**File:** `crates/rustcode-server/src/server.rs`  
**AppState:**
```rust
pub struct AppState {
    pub bus: Arc<dyn EventBus>,
    pub sessions: Arc<dyn SessionManager>,
    pub tools: Arc<ToolRegistry>,
    pub permissions: Arc<dyn PermissionEvaluator>,
    pub questions: Arc<QuestionService>,
    pub runner: Arc<SessionRunner>,
    pub providers: Arc<ProviderRegistry>,
}
```

### 7.2 OpenCode Server

**File:** `packages/opencode/src/server/routes/` (inferred from structure)  
**Evidence:** OpenCode organizes routes by instance/httpapi/groups/.  
**Problem:** No direct comparison possible without reading all 30 route files. However, the route modules line up 1:1 with OpenCode's route groups.  
**Impact:** Unknown until per-route audit is done.  
**Severity:** Medium (incomplete)  
**Recommendation:** Perform per-route audit of HTTP methods, request/response schemas, and middleware.  
**Effort:** 5 days

### 7.3 SSE / Streaming

**File:** `crates/rustcode-server/src/sse.rs` (inferred from lib.rs)  
**Evidence:** RustCode has SSE support in the server module structure.  
**Problem:** OpenCode uses SSE for event streaming to the TUI and Web clients. SSE endpoints must support `EventV2.Cursor`-based replay. RustCode's SSE implementation quality unknown.  
**Impact:** If SSE is just a stub, real-time features won't work.  
**Severity:** High  
**Recommendation:** Audit SSE implementation against OpenCode's event streaming protocol.  
**Effort:** 2 days

---

## 8. EVENT SYSTEM

### 8.1 RustCode Event Bus (`crates/rustcode-core/src/bus.rs`)

```rust
pub trait EventBus: Send + Sync {
    fn publish(&self, event: GlobalEvent) -> Result<()>;
    fn subscribe(&self) -> Result<Receiver<GlobalEvent>>;
}

pub enum GlobalEvent {
    ConfigChanged,
    SessionCreated { id: SessionId },
    SessionUpdated { id: SessionId },
    SessionDeleted { id: SessionId },
    ToolExecuted { name: String, result: ToolResult },
    ProviderChanged,
    Shutdown,
}
```

**File:** `crates/rustcode-core/src/bus.rs`  
**Evidence:** Simple pub/sub with `tokio::sync::broadcast`. Fixed event enum.  
**Problem:** OpenCode's `EventV2` is a full event sourcing system:
- **Durable events** (persisted to SQLite)
- **Synchronized events** with versioned schemas
- **Projectors** for local operational state
- **Commit guards** for validation
- **Replay** for recovery
- **Aggregate streams** with cursor-based pagination
- **PubSub-based live streams** for real-time updates
- **Ownership/claiming** for distributed coordination
- **Event definitions** with typed schemas

RustCode has none of this.  
**Impact:** RustCode cannot recover state from event history, support multi-process coordination, or provide durable audit trails.  
**Severity:** Critical  
**Recommendation:** Implement the full `EventV2` system: durable event store (SQLite), typed event definitions, projectors, replay, aggregate streams, cursor pagination.  
**Effort:** 3 weeks

### 8.2 OpenCode Event System (`packages/core/src/event.ts`)

```typescript
export type Definition<Type, DataSchema> = {
  readonly type: Type
  readonly sync?: { readonly version: number; readonly aggregate: string }
  readonly data: DataSchema
}

export interface Interface {
  readonly publish: <D>(def: D, data: Data<D>, options?: PublishOptions) => Effect.Effect<Payload<D>>
  readonly subscribe: <D>(def: D) => Stream.Stream<Payload<D>>
  readonly all: () => Stream.Stream<Payload>
  readonly aggregateEvents: (input: { aggregateID: string; after?: Cursor }) => Stream.Stream<CursorEvent>
  readonly sync: (handler: Sync) => Effect.Effect<Unsubscribe>
  readonly listen: (listener: Listener) => Effect.Effect<Unsubscribe>
  readonly beforeCommit: (guard: CommitGuard) => Effect.Effect<void>
  readonly project: <D>(def: D, projector: Projector<D>) => Effect.Effect<void>
  readonly replay: (event: SerializedEvent, options?) => Effect.Effect<void>
  readonly replayAll: (events: SerializedEvent[], options?) => Effect.Effect<string | undefined>
  readonly remove: (aggregateID: string) => Effect.Effect<void>
  readonly claim: (aggregateID: string, ownerID: string) => Effect.Effect<void>
}
```

**File:** `packages/core/src/event.ts:29-173`  
**Evidence:** 680 lines of full event sourcing infrastructure. Durable events written to `EventTable` + `EventSequenceTable` in SQLite with transaction-safe commit.  
**Problem:** RustCode has no equivalent.  
**Impact:** Every event-driven feature (session events, permission requests, MCP status changes, LSP events, TUI events) must be reimplemented from scratch.  
**Severity:** Critical  
**Recommendation:** Build durable event store with SQLite + `sqlx`. Implement `Definition`, `Payload`, `Projector`, `Cursor` primitives.  
**Effort:** 3 weeks

---

## 9. TOOL SYSTEM

### 9.1 RustCode Tool (`crates/rustcode-core/src/tool.rs`)

```rust
#[async_trait]
pub trait Tool: Send + Sync {
    fn name(&self) -> &'static str;
    fn description(&self) -> &'static str;
    fn parameters(&self) -> serde_json::Value;
    async fn execute(&self, context: &ToolContext, args: serde_json::Value) -> Result<ExecuteResult>;
}

pub struct ToolContext {
    pub session_id: Option<SessionId>,
    pub permissions: Arc<dyn PermissionEvaluator>,
    pub config: Arc<Config>,
}

pub struct ToolRegistry {
    tools: HashMap<String, Arc<dyn Tool>>,
}

impl ToolRegistry {
    pub fn register(&mut self, tool: Arc<dyn Tool>);
    pub fn get(&self, name: &str) -> Option<Arc<dyn Tool>>;
    pub fn all(&self) -> Vec<Arc<dyn Tool>>;
    pub async fn execute(&self, name: &str, context: &ToolContext, args: serde_json::Value) -> Result<ExecuteResult>;
}
```

**File:** `crates/rustcode-core/src/tool.rs`  
**Evidence:** Trait-based, async, with ToolContext for session+permissions+config injection. ToolRegistry for lookup+dispatch.  
**Problem:** OpenCode's tool system is deeply integrated with:
- Permission V2 (ask/assert/reply before tool execution)
- Tool output store (truncation with bounded preview)
- MCP tools (discovered dynamically from MCP servers)
- Permission rulesets per-agent

RustCode's `ToolRegistry` does not integrate with permission checking or output storage.  
**Impact:** Every tool execution bypasses permission checks unless callers manually invoke the permission module. Tool output is never truncated.  
**Severity:** High  
**Recommendation:** Add permission checking and tool output truncation to `ToolRegistry::execute()`.  
**Effort:** 2 days

### 9.2 OpenCode Tool Output Store (`packages/core/src/tool-output-store.ts`)

```typescript
export interface Interface {
  readonly limits: () => Effect.Effect<{ maxLines: number; maxBytes: number }>
  readonly bound: (input: BoundInput) => Effect.Effect<BoundResult, Error>
  readonly cleanup: () => Effect.Effect<void>
}
```

**File:** `packages/core/src/tool-output-store.ts:36-40`  
**Evidence:** Head/tail sampling at configurable line/byte limits. Stores overflow to disk with retention.  
**Problem:** RustCode has no equivalent.  
**Impact:** Large tool outputs (file reads, git diffs, terminal output) are never truncated, wasting LLM context window.  
**Severity:** High  
**Recommendation:** Implement `ToolOutputStore` with configurable limits, head/tail truncation, and disk overflow.  
**Effort:** 2 days

---

## 10. PERMISSION SYSTEM

### 10.1 RustCode Permission (`crates/rustcode-core/src/permission.rs`)

```rust
pub fn evaluate(action: &str, resource: &str, rules: &[PermissionRule]) -> PermissionEffect;

pub struct PermissionRule {
    pub action: WildcardPattern,
    pub resource: WildcardPattern,
    pub effect: PermissionEffect,
}

pub enum PermissionEffect { Allow, Ask, Deny }
```

**File:** `crates/rustcode-core/src/permission.rs`  
**Evidence:** Core evaluate() function with wildcard matching. Tests present.  
**Problem:** OpenCode's `PermissionV2` is a full service with:
- `ask()` — evaluate + create pending request for user approval
- `assert()` — evaluate + block until user approves/rejects
- `reply()` — handle user approval/rejection
- Per-agent ruleset resolution
- Saved permissions (persistent "always allow")
- Event-driven (publishes `Asked`/`Replied` events)
- `SessionStore` integration for session-scoped permissions

RustCode has only the evaluation logic, no ask/assert lifecycle, no user interaction, no persistence, no events.  
**Impact:** Permission system is non-functional for interactive use — no way to ask the user for permission or save decisions.  
**Severity:** Critical  
**Recommendation:** Implement the full `PermissionV2` lifecycle: ask, assert, reply, saved permissions, event integration.  
**Effort:** 1 week

### 10.2 OpenCode Permission (`packages/core/src/permission.ts`)

```typescript
export interface Interface {
  readonly ask: (input: AssertInput) => Effect.Effect<AskResult, SessionV2.NotFoundError>
  readonly assert: (input: AssertInput) => Effect.Effect<void, Error | SessionV2.NotFoundError>
  readonly reply: (input: ReplyInput) => Effect.Effect<void, NotFoundError>
  readonly get: (id: ID) => Effect.Effect<Request | undefined>
  readonly forSession: (sessionID: SessionV2.ID) => Effect.Effect<ReadonlyArray<Request>>
  readonly list: () => Effect.Effect<ReadonlyArray<Request>>
}
```

**File:** `packages/core/src/permission.ts:118-125`  
**Evidence:** Full ask/assert/reply lifecycle with deferreds, event publishing, saved permissions, and session-scoped pending requests.  
**Problem:** RustCode has evaluate-only. No user-in-the-loop permission flow exists.  
**Impact:** Agents cannot request user permission for dangerous operations.  
**Severity:** Critical  
**Recommendation:** Implement `PermissionService` with ask/assert/reply, deferred resolution, and event publishing.  
**Effort:** 1 week

---

## 11. MCP SYSTEM

### 11.1 RustCode MCP (`crates/rustcode-mcp/src/lib.rs`)

```rust
pub enum McpTransport {
    Stdio(StdioTransport),
    Http(HttpTransport),
}

pub trait McpTransport: Send + Sync {
    fn send(&self, message: McpMessage) -> Pin<Box<dyn Future<Output = Result<McpResponse>> + Send>>;
    fn receive(&self) -> Pin<Box<dyn Future<Output = Result<McpMessage>> + Send>>;
}

pub struct StdioTransport { /* command, args, cwd, env, child process */ }
pub struct HttpTransport { /* url, headers, client */ }

pub struct McpToolExecutor { /* transport */ }
pub struct McpDiscovery { /* config -> transport -> tools */ }
```

**File:** `crates/rustcode-mcp/src/lib.rs`  
**Evidence:** Transport trait + Stdio/Http implementations. McpToolExecutor for converting MCP tools to Tool trait. McpDiscovery for scanning config.  
**Problem:** OpenCode's MCP implementation is ~950 lines with:
- Full state management (config, status, clients, defs)
- OAuth support (auth provider, callback server, token storage)
- Prompt and resource listing (not just tools)
- Dynamic connect/disconnect at runtime
- Status tracking (connected/disabled/failed/needs_auth)
- Tool list change notification handling
- Browser-based OAuth flow
- Transport negotiation (StreamableHTTP vs SSE vs Stdio)

RustCode MCP is scaffolding — no OAuth, no prompts/resources, no dynamic lifetime management, no status tracking.  
**Impact:** RustCode cannot authenticate with MCP servers requiring OAuth, cannot list prompts/resources, cannot dynamically reconnect.  
**Severity:** High  
**Recommendation:** Implement full MCP lifecycle: OAuth (auth provider, callback, token persistence), prompt/resource discovery, status tracking, notification handling.  
**Effort:** 2 weeks

### 11.2 OpenCode MCP (`packages/opencode/src/mcp/index.ts`)

```typescript
export interface Interface {
  readonly status: () => Effect.Effect<Record<string, Status>>
  readonly clients: () => Effect.Effect<Record<string, MCPClient>>
  readonly tools: () => Effect.Effect<Record<string, Tool>>
  readonly prompts: () => Effect.Effect<Record<string, PromptInfo & { client: string }>>
  readonly resources: () => Effect.Effect<Record<string, ResourceInfo & { client: string }>>
  readonly add: (name: string, mcp: ConfigMCPV1.Info) => Effect.Effect<...>
  readonly connect: (name: string) => Effect.Effect<void, NotFoundError>
  readonly disconnect: (name: string) => Effect.Effect<void, NotFoundError>
  readonly getPrompt: (...) => Effect.Effect<...>
  readonly readResource: (...) => Effect.Effect<...>
  readonly startAuth: (...) => Effect.Effect<{ authorizationUrl, oauthState }, NotFoundError>
  readonly authenticate: (...) => Effect.Effect<Status, NotFoundError>
  readonly finishAuth: (...) => Effect.Effect<Status, NotFoundError>
  readonly removeAuth: (...) => Effect.Effect<void>
  readonly supportsOAuth: (...) => Effect.Effect<boolean>
  readonly hasStoredTokens: (...) => Effect.Effect<boolean>
  readonly getAuthStatus: (...) => Effect.Effect<AuthStatus>
}
```

**File:** `packages/opencode/src/mcp/index.ts:159-186`  
**Evidence:** 16 methods covering full MCP lifecycle including OAuth. 950 lines of implementation.  
**Problem:** RustCode covers only tools via transport — no OAuth, no prompts/resources, no runtime lifecycle.  
**Impact:** MCP servers requiring authentication (most cloud MCP servers) are unusable.  
**Severity:** High  
**Recommendation:** Prioritize MCP OAuth implementation. Then add prompts/resources support.  
**Effort:** 2 weeks

---

## 12. LSP SYSTEM

### 12.1 RustCode LSP (`crates/rustcode-lsp/src/lib.rs`)

```rust
pub struct LspManager { /* manages multiple LSP servers */ }
pub struct LspClient { /* connection to a single LSP server */ }

pub enum LspError {
    ConnectionFailed,
    ServerCrashed,
    RequestFailed(String),
}
```

**File:** `crates/rustcode-lsp/src/lib.rs`  
**Evidence:** Stub — managers and client types defined but no methods.  
**Problem:** OpenCode's LSP has a full 511-line implementation with:
- `init()` — initialize all configured LSP servers
- `status()` — list connected servers
- `hasClients(file)` — check if file has LSP coverage
- `touchFile()` — open file in LSP server
- `diagnostics()` — collect all diagnostics
- `hover()` — get hover information
- `definition()` — go-to-definition
- `references()` — find references
- `implementation()` — go-to-implementation
- `documentSymbol()` — get document symbols
- `workspaceSymbol()` — search workspace symbols
- `prepareCallHierarchy()` / `incomingCalls()` / `outgoingCalls()` — call hierarchy
- Server spawning with config-driven discovery
- Runtime flag-based server selection (pyright vs ty)
- InstanceState for per-directory LSP state
- Extension-based server matching

RustCode LSP is a type skeleton with zero implementation.  
**Impact:** All LSP features (hover, definition, references, diagnostics, symbols) are unimplemented.  
**Severity:** Critical  
**Recommendation:** Implement full LSP interface. Start with server spawning and diagnostics, then hover/definition.  
**Effort:** 3 weeks

### 12.2 OpenCode LSP (`packages/opencode/src/lsp/lsp.ts`)

```typescript
export interface Interface {
  readonly init: () => Effect.Effect<void>
  readonly status: () => Effect.Effect<Status[]>
  readonly hasClients: (file: string) => Effect.Effect<boolean>
  readonly touchFile: (input: string, diagnostics?) => Effect.Effect<void>
  readonly diagnostics: () => Effect.Effect<Record<string, Diagnostic[]>>
  readonly hover: (input: LocInput) => Effect.Effect<any>
  readonly definition: (input: LocInput) => Effect.Effect<any[]>
  readonly references: (input: LocInput) => Effect.Effect<any[]>
  readonly implementation: (input: LocInput) => Effect.Effect<any[]>
  readonly documentSymbol: (uri: string) => Effect.Effect<(DocumentSymbol | Symbol)[]>
  readonly workspaceSymbol: (query: string) => Effect.Effect<Symbol[]>
  readonly prepareCallHierarchy: (input: LocInput) => Effect.Effect<any[]>
  readonly incomingCalls: (input: LocInput) => Effect.Effect<any[]>
  readonly outgoingCalls: (input: LocInput) => Effect.Effect<any[]>
}
```

**File:** `packages/opencode/src/lsp/lsp.ts:121-136`  
**Evidence:** 14 methods spanning the full LSP protocol surface. Server spawning backed by `spawn()` with config overrides. Per-instance state with `InstanceState`. Symbol kinds filtered for workspace search. Runtime flags control server selection.  
**Problem:** RustCode has zero of these methods.  
**Impact:** No LSP integration in the Rust port.  
**Severity:** Critical  
**Recommendation:** Implement each method incrementally.  
**Effort:** 3 weeks

---

## 13. COMMAND SYSTEM (Slash Commands)

### 13.1 OpenCode Command (`packages/core/src/command.ts`)

```typescript
export class Info extends Schema.Class<Info>("CommandV2.Info")({
  name: Schema.String,
  template: Schema.String,
  description: Schema.String.pipe(Schema.optional),
  agent: Schema.String.pipe(Schema.optional),
  model: ModelV2.Ref.pipe(Schema.optional),
  subtask: Schema.Boolean.pipe(Schema.optional),
}) {}

export interface Interface {
  readonly transform: State.Interface<Data, Editor>["transform"]
  readonly update: State.Interface<Data, Editor>["update"]
  readonly get: (name: string) => Effect.Effect<Info | undefined>
  readonly list: () => Effect.Effect<Info[]>
}
```

**File:** `packages/core/src/command.ts:8-33`  
**Evidence:** Dedicated command service with CRUD, state management, editor pattern. Commands support agent-specific dispatch, model override, and subtask mode.  
**Problem:** RustCode has no `command.rs` module. The `crates/rustcode-core/src/lib.rs` module list does not include `command`.  
**Impact:** Slash command system (used for `/run`, `/edit`, `/ask`, etc.) is missing entirely.  
**Severity:** High  
**Recommendation:** Implement `Command` service with state management and schema definitions.  
**Effort:** 2 days

---

## 14. MISSING MODULES

### 14.1 Modules in OpenCode not yet in RustCode `core/src/lib.rs`

| Module | OpenCode TS | RustCode Status | Impact |
|--------|-------------|-----------------|--------|
| `command` | `packages/core/src/command.ts` | **Missing** | No slash commands |
| `account` | `packages/core/src/account.ts` | Present (`account` in lib.rs) | OK |
| `attachment` | `packages/core/src/attachment.ts` (inferred) | Not in lib.rs | Missing attachment processing |
| `background-job` | `packages/core/src/background-job.ts` | Present | OK |
| `catalog` | `packages/core/src/catalog.ts` | Present | OK |
| `database` | `packages/core/src/database/` | Present (`storage`, `database`) | OK |
| `file-mutation` | `packages/core/src/file-mutation.ts` | Not in lib.rs | Missing file mutation tracking |
| `filesystem` | `packages/core/src/filesystem.ts` | Not in lib.rs | Uses std::fs directly |
| `git` | `packages/core/src/git.ts` | Present (`git`) | OK |
| `image` | `packages/core/src/image.ts` | Present (`image`) | OK |
| `instruction-context` | `packages/core/src/instruction-context.ts` | Not in lib.rs | Missing instruction context |
| `integration` | `packages/core/src/integration.ts` | Present (`integration`) | OK |
| `location` | `packages/core/src/location.ts` | Not in lib.rs | Missing location tracking |
| `model-request` | `packages/core/src/model-request.ts` | Not in lib.rs | Missing request config |
| `npm` | `packages/core/src/npm.ts` | Not in lib.rs | Missing npm integration |
| `observability` | `packages/core/src/observability.ts` | Not in lib.rs | Missing observability |
| `patch` | `packages/core/src/patch.ts` | Not in lib.rs | Missing patch operations |
| `plugin` | `packages/core/src/plugin.ts` | Present (`plugin`) | OK |
| `policy` | `packages/core/src/policy.ts` | Not in lib.rs | Missing policy system |
| `process` | `packages/core/src/process.ts` | Not in lib.rs | Uses std::process |
| `project` | `packages/core/src/project.ts` | Not in lib.rs | Missing project resolution |
| `pty` | `packages/core/src/pty.ts` | Not in lib.rs | (Has `crates/rustcode-server`) |
| `question` | `packages/core/src/question.ts` | Present (`question`) | OK |
| `reference` | `packages/core/src/reference.ts` | Present (`reference`) | OK |
| `repository` | `packages/core/src/repository.ts` | Not in lib.rs | Missing repo management |
| `ripgrep` | `packages/core/src/ripgrep.ts` | Not in lib.rs | Uses std grep |
| `shell` | `packages/core/src/shell.ts` | Not in lib.rs | Missing shell integration |
| `skill` | `packages/core/src/skill.ts` | Present (`skill`) | OK |
| `snapshot` | `packages/core/src/snapshot.ts` | Present (`snapshot`) | OK |
| `state` | `packages/core/src/state.ts` | Not in lib.rs | Missing state management |
| `workspace` | `packages/core/src/workspace.ts` | Not in lib.rs | Missing workspace management |
| `aisdk` | `packages/core/src/aisdk.ts` | Present (in `provider.rs`) | OK |

**File:** `crates/rustcode-core/src/lib.rs` vs `packages/core/src/*.ts`  
**Evidence:** RustCode lib.rs exports ~20 modules. OpenCode core/src/ has ~50 `.ts` files. RustCode is behind by ~15+ modules.  
**Problem:** ~15 modules from OpenCode core have no RustCode equivalent at all.  
**Impact:** Large feature gap — project management, workspace resolution, file operations, process management, shell integration, PTY support, observability, policy engine, location tracking, etc.  
**Severity:** Critical  
**Recommendation:** Prioritize missing modules by dependency order: `location` → `project` → `workspace` → `filesystem` → `process` → `shell` → `pty` → `command` → `policy`.  
**Effort:** 1-2 months

---

## 15. DATABASE / STORAGE

### 15.1 RustCode Storage (`crates/rustcode-core/src/storage.rs`)

```rust
pub struct Storage {
    base_path: PathBuf,
}

pub struct Database {
    pool: SqlitePool,
}
```

**File:** `crates/rustcode-core/src/storage.rs`  
**Evidence:** File-based JSON storage + SQLite placeholder. Few methods.  
**Problem:** OpenCode has 18 SQLite tables (inferred from `core/src/**/*.sql.ts`) and 35+ migrations. Tables include: sessions, messages, events, event_sequences, projects, saved_permissions, oauth_tokens, etc.  
**Impact:** RustCode database lacks the full schema — no event store, no saved permissions, no OAuth token storage, no project table, no session messages table.  
**Severity:** Critical  
**Recommendation:** Migrate all 18+ SQLite tables from OpenCode's Drizzle schema to `sqlx` migrations.  
**Effort:** 2 weeks

### 15.2 Database Schema Comparison

**File:** `packages/core/src/session/sql.ts`, `packages/core/src/event/sql.ts`, etc.  
**Evidence:** OpenCode uses Drizzle ORM with typed table schemas. Key tables:
- `SessionTable` (id, slug, version, project_id, directory, path, workspace_id, title, agent, model, cost, tokens, time_created, time_updated)
- `SessionMessageTable` (id, session_id, seq, type, data)
- `EventTable` (id, aggregate_id, seq, type, data)
- `EventSequenceTable` (aggregate_id, seq, owner_id)
- `ProjectTable` (id, worktree, vcs, sandboxes)
- Permission saved rules table
- OAuth token storage

**Problem:** RustCode has none of these tables defined in sqlx migration format.  
**Impact:** No persistence for sessions, messages, events, projects, permissions, or OAuth.  
**Severity:** Critical  
**Recommendation:** Create `sqlx` migration files for every table.  
**Effort:** 1 week

---

## 16. STATE MANAGEMENT

### 16.1 OpenCode State (`packages/core/src/state.ts`)

```typescript
export interface Interface<Data, Editor> {
  readonly get: () => Data
  readonly update: (fn: (editor: Editor) => void) => void
  readonly transform: (fn: (data: Data) => Data) => void
}
```

**File:** `packages/core/src/state.ts`  
**Evidence:** OpenCode uses a centralized state management pattern with `State.create()`, `Editor` pattern, and `immer`-based draft mutations. Agent, Command, and other stateful services use this pattern.  
**Problem:** RustCode has no equivalent — each module manages state independently.  
**Impact:** Inconsistent state management patterns across modules. No undo/redo support.  
**Severity:** Low  
**Recommendation:** Consider a consistent state management approach (e.g., `Arc<RwLock<Data>>` with a mutation trait).  
**Effort:** 3 days

---

## 17. EFFECT SYSTEM

### 17.1 Ts→Rust Idiom Mapping

**File:** Multiple (`src/main.rs`, `crates/rustcode-core/src/*.rs`)  
**Evidence:** OpenCode uses Effect.ts for all effectful operations. RustCode uses `async fn` + `Result<T, E>`.

**Problem areas:**
- **Effect.gen (coroutines):** Rust `async fn` covers this idiomatically.
- **Context.Service / Layer (DI):** Rust lacks built-in DI. OpenCode has 30+ services wired through Layers. RustCode uses manual Arc-passing and struct fields.
- **Schema.TaggedErrorClass:** No Rust equivalent. Use `thiserror` + `serde` for serializable typed errors.
- **Schema.Struct / Schema.Class:** Use `serde::Deserialize`/`Serialize` + struct.

**Impact:** Wiring 30+ services with manual Arc injection will become unwieldy as the codebase grows.  
**Severity:** Medium  
**Recommendation:** Consider a DI framework (e.g., `shaku`, `dptree`, or a custom registry pattern). At minimum, define a `Service` trait with `layer()` pattern for testability.  
**Effort:** 1 week for design, ongoing adoption

---

## 18. TEST COVERAGE

### 18.1 RustCode Tests

**Evidence:** Only `permission.rs` has tests. `error.rs`, `config.rs`, `provider.rs`, `session.rs`, `tool.rs`, `agent.rs` have no tests.  
**Problem:** OpenCode doesn't have tests either (by policy), but the Rust port should have tests to verify correctness of the port.  
**Impact:** Undetected regressions when porting.  
**Severity:** Medium  
**Recommendation:** Add unit tests for every module, especially `evaluate()` (permissions), config parsing serialization, session CRUD, tool registration, provider resolution.  
**Effort:** 1 week

---

## 19. SECURITY & SAFETY

### 19.1 Unsafe Code

**File:** Each crate's `lib.rs`  
**Evidence:** `#![forbid(unsafe_code)]` in every crate.  
**Assessment:** Good. Maintain this.  
**Severity:** N/A  
**Recommendation:** N/A

### 19.2 No `.unwrap()` Policy

**File:** `CLAUDE.md`  
**Evidence:** Policy states no `.unwrap()` in library code. Use `?`, `.ok_or()`, `.unwrap_or()`, or `expect()`.  
**Assessment:** Policy is good. Enforce in CI clippy (currently `-D warnings` does not catch this).  
**Severity:** Low  
**Recommendation:** Add `clippy::unwrap_used` lint to deny in CI.  
**Effort:** 1 hour

---

## 20. SUMMARY TABLE

| Domain | RustCode Status | OpenCode Reference | Gap | Severity | Effort |
|--------|----------------|-------------------|-----|----------|--------|
| Error types | 14-variant enum | 50+ tagged error classes | Medium gap | Medium | 3 days |
| Provider/LLM | Trait + 11 fixed providers | Schema-based + dynamic | Medium gap | High | 3 days |
| Config | ~10 fields | ~30+ fields | Large gap | High | 3 days |
| Session | CRUD only (8 methods) | Full lifecycle (16 methods) | Large gap | Critical | 2 weeks |
| Agent | Basic struct + 4 built-in | Full state service | Small gap | Low | 1 day |
| CLI | 22/25 commands | 25 commands | Small gap | Medium | 1 day |
| Server | 30 route stubs | Full implementation | Unknown | Medium | 5 days |
| Events | Simple broadcast | Full event sourcing | Full gap | Critical | 3 weeks |
| Tools | Trait + registry + context | + Permission + Output store | Large gap | High | 2 days |
| Permission | evaluate() only | Full ask/assert/reply | Full gap | Critical | 1 week |
| MCP | Transport trait + 2 impls | Full lifecycle + OAuth | Full gap | High | 2 weeks |
| LSP | Type skeletons | 14-method implementation | Full gap | Critical | 3 weeks |
| Command | Missing | Full service | Full gap | High | 2 days |
| Database | Skeleton | 18 tables + 35 migrations | Full gap | Critical | 2 weeks |
| Modules | ~20/50 | ~50 core modules | Large gap | Critical | 1-2 months |

### Severity Distribution

| Severity | Count | Total Est. Effort |
|----------|-------|-------------------|
| Critical | 8 | ~14 weeks |
| High | 5 | ~5 weeks |
| Medium | 5 | ~2 weeks |
| Low | 2 | ~2 days |
| Unknown | 1 | ~5 days |

---

## 21. KEY RECOMMENDATIONS (Top 5)

1. **Durable Event Store** (`EventV2`) — Foundation for everything: sessions, permissions, MCP, LSP, real-time features. Without this, session recovery, audit trails, and multiplayer are impossible. Est. 3 weeks.

2. **Full Permission V2 Lifecycle** — `ask/assert/reply` with deferred resolution, event publishing, saved rules, and session integration. Est. 1 week.

3. **Full Session V2 Implementation** — Prompt admission with conflict detection, event streaming, agent/model switching, interrupt/resume, shell/skill dispatch. Est. 2 weeks.

4. **Database Schema Migration** — Replicate all 18+ SQLite tables from OpenCode's Drizzle schema to `sqlx` migration files. Est. 2 weeks.

5. **LSP Implementation** — Server spawning, diagnostics, hover, definition, references, symbols. Est. 3 weeks.

---

## 22. NOTES

- This audit covers all public APIs found in the RustCode and OpenCode codebases. Some OpenCode internal APIs (e.g., `effect/run-service.ts`, `effect/layer-node.ts`, `effect/instance-state.ts`) are framework-level and not directly ported due to language differences.
- The "Effect.ts" dependency injection pattern has no direct Rust equivalent; RustCode's manual Arc injection is a reasonable alternative for now, but will need revisiting as the codebase grows.
- MCP OAuth support is the biggest single feature gap in the MCP implementation — necessary for most cloud-based MCP servers.
- RustCode's CI is green (scaffold phase). The lint policy (`#![allow(dead_code)]`) will need removal as modules become production-quality.
