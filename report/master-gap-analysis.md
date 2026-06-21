# Master Gap Analysis: RustCode vs OpenCode

**Generated**: 2026-06-21
**Source Agents**: 02–20 (19 reports)
**Scope**: RustCode (Rust port) vs OpenCode (TypeScript source)

---

## Master Gap Count Table

| Severity | Architecture | Rust Lang | Logic | Security | Performance | Scalability | Feature | API | Testing | Dependency | Maintainability | DevEx | Infrastructure | Database | Reliability | Competitive | Refactoring | Tech Debt | Production | **Total** |
|----------|:-----------:|:---------:|:-----:|:--------:|:-----------:|:-----------:|:------:|:---:|:-------:|:----------:|:--------------:|:-----:|:-------------:|:--------:|:-----------:|:-----------:|:-----------:|:---------:|:---------:|:--------:|
| Critical | 5 | 1 | 4 | 0 | 3 | 5 | 1 | 4 | 3 | 1 | 5 | 4 | 1 | 2 | 5 | 2 | 7 | 4 | 2 | **58** |
| High | 5 | 2 | 12 | 8 | 3 | 6 | 0 | 6 | 2 | 2 | 6 | 6 | 3 | 4 | 10 | 4 | 6 | 5 | 4 | **94** |
| Medium | 1 | 10 | 11 | 12 | 8 | 7 | 0 | 7 | 7 | 5 | 9 | 4 | 5 | 7 | 11 | 2 | 7 | 9 | 6 | **128** |
| Low | 0 | 6 | 10 | 10 | 4 | 1 | 0 | 4 | 0 | 3 | 3 | 2 | 1 | 3 | 4 | 0 | 4 | 6 | 0 | **62** |
| Info | 0 | 12 | 10 | 10 | 4 | 1 | 0 | 3 | 0 | 1 | 3 | 1 | 3 | 6 | 5 | 0 | 1 | 6 | 0 | **68** |
| **Total** | **11** | **31** | **47** | **40** | **22** | **20** | **1** | **24** | **12** | **12** | **26** | **17** | **13** | **22** | **35** | **8** | **25** | **30** | **12** | **410** |

---

## 1. Architecture Gaps

### Critical

- **Gap ID**: GAP-ARCH-001
- **Location**: `rustcode-core/src/lib.rs:1-95`
- **OpenCode**: 26 packages with clean dependency DAG; `core → llm`, `server → core`, `cli → core`
- **RustCode**: Single monolithic core crate with 95 flat public modules; every module is `pub mod`
- **Description**: No enforced module boundaries — all 95 modules are world-public with no `pub(crate)` discipline
- **Consequence**: Refactoring internal modules requires checking all consumers; circular logical dependencies possible
- **Recommendation**: Use `pub(crate)` for internal modules; define explicit `pub use` re-exports in `lib.rs`
- **Severity**: Critical
- **Source Agent**: Agent 02

- **Gap ID**: GAP-ARCH-002
- **Location**: `rustcode-core/src/lib.rs:1-95`, `src/main.rs:1-8575`
- **OpenCode**: Low coupling via package boundaries + Effect's injection system (`Layer`, `Context.Service`)
- **RustCode**: Extreme coupling — 95 flat modules, 8,575-line `main.rs` mixing CLI parsing, business logic, infrastructure
- **Description**: High coupling makes changes brittle — `config.rs` changes can ripple through all 94 other modules
- **Consequence**: Testing any module in isolation requires importing the entire core crate; dependency injection is manual
- **Recommendation**: Introduce trait-based DI; split into multiple crates; use constructor injection not global state
- **Severity**: Critical
- **Source Agent**: Agent 02

- **Gap ID**: GAP-ARCH-003
- **Location**: `rustcode-core/src/lib.rs:1-95` (`pub mod`), `rustcode-core/src/database.rs`, `rustcode-core/src/config.rs`
- **OpenCode**: Clean Architecture — core defines ports, infrastructure implements them; dependencies point inward
- **RustCode**: Core imports `sqlx`, `reqwest`, `std::fs` directly — infrastructure concerns in domain code
- **Description**: Dependency Inversion Principle violated repeatedly — core directly calls SQL queries and filesystem operations
- **Consequence**: Impossible to swap infrastructure without modifying core code; business logic polluted with serialization, HTTP status codes, SQL
- **Recommendation**: Define `Database` trait in core → implement in `rustcode-database-sqlite`; same for `HttpClient`, `FileSystem`
- **Severity**: Critical
- **Source Agent**: Agent 02

- **Gap ID**: GAP-ARCH-004
- **Location**: `rustcode/Cargo.toml:1-96` (5 crates), `opencode/packages/` (26 packages)
- **OpenCode**: 26 packages with clear single-responsibility boundaries; infrastructure-layer packages (`effect-drizzle-sqlite`, `effect-sqlite-node`)
- **RustCode**: 5 crates — 4 are stubs (server, tui, lsp, mcp); no infrastructure crates; no CLI crate (logic in main.rs)
- **Description**: Insufficient modularization — 5× fewer crates than OpenCode's 26 packages; core acts as dump for all concerns
- **Consequence**: Build times degrade as core grows; no reuse path; third-party contributions either bloat core or require creating new crate
- **Recommendation**: Extract into granular crates: `rustcode-core-types`, `rustcode-provider`, `rustcode-session`, `rustcode-database-sqlite`, `rustcode-http`, `rustcode-plugin-sdk`, `rustcode-cli`
- **Severity**: Critical
- **Source Agent**: Agent 02

- **Gap ID**: GAP-ARCH-005
- **Location**: `src/main.rs:1-8575`
- **OpenCode**: Thin CLI dispatch (~200 lines) delegating to Effect layers; `packages/opencode/src/index.ts`
- **RustCode**: 8,575-line monolith merging CLI argument parsing, business logic, database queries, SSE handling, configuration
- **Description**: Thick main.rs makes testing, swapping, and parallel development harder
- **Consequence**: Business logic mixed with CLI formatting; cannot test CLI handlers without spawning full process
- **Recommendation**: Move logic from main.rs into `rustcode-core` or wrapper crates; make main.rs a thin CLI dispatch (<500 lines)
- **Severity**: Critical
- **Source Agent**: Agent 02

### High

- **Gap ID**: GAP-ARCH-006
- **Location**: `rustcode-core/src/` (95 flat modules)
- **OpenCode**: Domain-cohesive modules: `packages/llm/src/anthropic/`, `packages/core/src/session/`
- **RustCode**: Flat name prefixing (`session_*`) instead of sub-modules — 14 session-related files, 8 provider-related files all flat
- **Description**: 95 flat modules create heavy cognitive load; developers must scan full list to understand what exists
- **Consequence**: Module-internal types cannot be hidden; sub-module grouping would reduce apparent surface area from 95 to ~15 groups
- **Recommendation**: Group related modules into sub-modules: `pub mod session { ... }`, `pub mod provider { ... }`; use `pub(crate)` within groups
- **Severity**: High
- **Source Agent**: Agent 02

- **Gap ID**: GAP-ARCH-007
- **Location**: `rustcode-core/src/` (module list), `opencode/CONTEXT.md:1-129`
- **OpenCode**: Explicit V2 domain model — System Context algebra, EventV2 event sourcing, Location-scoped services, Account identity
- **RustCode**: Flattened domain — System Context missing algebra (epoch, baseline, snapshot, mid-conversation messages), EventV2 absent, Location not first-class
- **Description**: OpenCode V2 has mature domain model with carefully separated concerns; RustCode ported V1/early-V2 structure but lacks V2 innovations
- **Consequence**: RustCode will diverge in capabilities as OpenCode's V2 matures; System Context algebra is core of session intelligence
- **Recommendation**: Map V2 domain model before deeper implementation; create dedicated modules for System Context algebra; implement EventV2 event sourcing
- **Severity**: High
- **Source Agent**: Agent 02

- **Gap ID**: GAP-ARCH-008
- **Location**: `rustcode-core/src/provider.rs` (Provider trait), whole codebase
- **OpenCode**: Emerging hexagonal architecture — ports defined in `llm` (provider-neutral), infrastructure injected via Effect layers
- **RustCode**: Good port/adapter for LLM providers only; database (`sqlx`), HTTP server (`axum`), filesystem imported directly in core
- **Description**: No infrastructure abstraction layer — core code directly calls SQL queries and filesystem operations
- **Consequence**: Testing requires real infrastructure (SQLite files, real filesystem, network); cannot swap implementations
- **Recommendation**: Generalize port/adapter pattern — define traits for `Database`, `FileSystem`, `EventStore`, `SessionStore`
- **Severity**: High
- **Source Agent**: Agent 02

- **Gap ID**: GAP-ARCH-009
- **Location**: `rustcode-crates/` (server, tui, lsp, mcp)
- **OpenCode**: 4+ real layers — `opencode(CLI) → core → llm → infrastructure`
- **RustCode**: 1.5 layers — core + thin wrapper crates that are stu-level; business logic for server, tui, lsp, mcp lives in core
- **Description**: Wrapper crates are stub-level thin; the thick main.rs exacerbates the layering problem
- **Consequence**: Cannot develop server, TUI, LSP, MCP independently; all changes require modifying core
- **Recommendation**: Move logic from main.rs into crates; extract server, TUI, LSP, MCP logic from core into respective crates
- **Severity**: High
- **Source Agent**: Agent 02

- **Gap ID**: GAP-ARCH-010
- **Location**: `rustcode-core/src/session_runner.rs` (V1 vs V2 code duplication)
- **OpenCode**: Single clean runner with Effect.stream pipeline
- **RustCode**: V1 `run_loop` (~200 lines) and V2 `run_turn_attempt` (~222 lines) duplicate LLM streaming loop, tool call collection, tool execution
- **Description**: V1/V2 code duplication in session runner — both iterate stream events, accumulate text deltas, collect tool calls
- **Consequence**: Bug fixes must be applied to both paths; V1 lacks doom-loop detection; V1 bypasses permission checks
- **Recommendation**: Extract shared stream processing and tool execution pipeline into shared helper function
- **Severity**: High
- **Source Agent**: Agent 04

---

## 2. Rust Language Gaps

### Critical

- **Gap ID**: GAP-RUST-001
- **Location**: `error.rs:23-352`, `session.rs:37-77`, `database.rs:1146-1158`, `rustcode-lsp/src/lib.rs:50-110`
- **OpenCode**: Single unified error type via Effect's `Schema.TaggedErrorClass`
- **RustCode**: Five separate error types: `Error` (34 variants), `SessionError` (12 variants), `DatabaseServiceError` (3 variants), `LspError` (10 variants), `McpError`
- **Description**: Fragmented error hierarchy — downstream code must match on 5+ enums; `crate::error::Error` lacks `#[from] SessionError`
- **Consequence**: Error conversion boilerplate; missed errors bubble as `SessionError::Other()` or `Error::Internal()` — losing type information
- **Recommendation**: Merge all into `crate::error::Error` with `#[from]` derives

- **Severity**: Critical
- **Source Agent**: Agent 03

### High

- **Gap ID**: GAP-RUST-002
- **Location**: `tool.rs:47`
- **OpenCode**: Messages passed by reference
- **RustCode**: `ToolContext.messages` is `Vec<ChatMessage>` — full deep clone of entire message history per tool call
- **Description**: Each bash/read/write tool call clones accumulated message history; for 50 messages at ~2KB each = 100KB per tool call
- **Consequence**: 2.5MB of cloned message data per 25-tool-call session; major contributor to memory pressure
- **Recommendation**: Store `Arc<[ChatMessage]>` or pass `&[ChatMessage]` with lifetime
- **Severity**: High
- **Source Agent**: Agent 03

- **Gap ID**: GAP-RUST-003
- **Location**: `rustcode-lsp/src/lib.rs:113`
- **OpenCode**: Unified error hierarchy
- **RustCode**: LSP crate defines own `Result<T>` alias incompatible with `crate::error::Result<T>`
- **Description**: LSP functions outside the crate cannot use `?` with `crate::error::Error`
- **Consequence**: Callers must `.map_err(|e| Error::LspInit(e.to_string()))` — loses type info
- **Recommendation**: Make `rustcode_lsp` use `crate::error::Result` or unified error type
- **Severity**: High
- **Source Agent**: Agent 03

### Medium

- **Gap ID**: GAP-RUST-004
- **Location**: `session.rs:866-891`
- **OpenCode**: TS passes mutable refs
- **RustCode**: `fork()` clones each `Part` via `p.clone()` then mutates IDs — all 13 Part variants cloned
- **Description**: Cloning large fields (text, output) unnecessarily; fork of session with large tool outputs duplicates all strings
- **Recommendation**: Add `fn map_ids(self, new_msg_id, new_sess_id)` that moves and mutates in-place
- **Severity**: Medium
- **Source Agent**: Agent 03

- **Gap ID**: GAP-RUST-005
- **Location**: `config.rs:47,1166`
- **OpenCode**: Effect `Layer` + `Ref<Info>`
- **RustCode**: `RwLock<Info>` with `.clone()` on every read
- **Description**: Config loaded once at startup, then `get()` clones entire `Info` struct (hundreds of fields) per tool invocation
- **Consequence**: Every tool invocation deep-clones the entire config tree
- **Recommendation**: Expose atomic reads via `&self` methods or use `arc_swap`
- **Severity**: Medium
- **Source Agent**: Agent 03

- **Gap ID**: GAP-RUST-006
- **Location**: `plugin.rs:210-229`
- **OpenCode**: Simple function closures
- **RustCode**: 4 function pointer fields with complex types suppressed by `#[allow(clippy::type_complexity)]`
- **Description**: Each closure is `Box<dyn Fn(&...) -> BoxFuture<...> + Send + Sync>` — type soup
- **Recommendation**: Define `trait HookHandler<Ctx>` with async method
- **Severity**: Medium
- **Source Agent**: Agent 03

- **Gap ID**: GAP-RUST-007
- **Location**: `config.rs:1177-1218`
- **OpenCode**: Config loading is Effect (async)
- **RustCode**: Filesystem reads done synchronously with `std::fs::read_to_string`
- **Description**: Blocking I/O in sync functions starves async runtime if called from spawned task
- **Recommendation**: Make config loading async with `tokio::fs`
- **Severity**: Medium
- **Source Agent**: Agent 03

- **Gap ID**: GAP-RUST-008
- **Location**: `session.rs:306-335`
- **OpenCode**: Union of 13 types in TS
- **RustCode**: Enum with 13 variants — largest variant (`ToolPart` with `serde_json::Value`) determines enum size
- **Description**: `Vec<Part>` is memory-heavy; boxing largest variants could reduce size by ~50%
- **Recommendation**: Box largest 2-3 variants: `Tool(Box<ToolPart>)`, `Text(Box<TextPart>)`, `Reasoning(Box<ReasoningPart>)`
- **Severity**: Medium
- **Source Agent**: Agent 03

- **Gap ID**: GAP-RUST-009
- **Location**: `session.rs:84-90`, `provider.rs:24-48`
- **OpenCode**: Branded string types (`type ModelId = string & { readonly __brand: "ModelId" }`)
- **RustCode**: `pub type SessionId = String;` — type aliases with zero type safety
- **Description**: `SessionId` and `ModelId` are interchangeable with `String`; no compile-time safety
- **Consequence**: Passing `ModelId` where `SessionId` expected compiles fine; runtime errors only guard
- **Recommendation**: Use proper newtypes: `struct SessionId(String);` with `new` + `as_str`
- **Severity**: Medium
- **Source Agent**: Agent 03

- **Gap ID**: GAP-RUST-010
- **Location**: `session.rs:1605-1663`
- **OpenCode**: Single update pattern in TS
- **RustCode**: Three functions (`set_id`, `set_message_id`, `set_session_id`) each with 13-arm matches — all exhaustive
- **Description**: Each function repeats same match pattern; adding new variant requires updating all three
- **Recommendation**: Use helper macro or method on `Part` returning `&mut CommonPartFields`
- **Severity**: Medium
- **Source Agent**: Agent 03

- **Gap ID**: GAP-RUST-011
- **Location**: `lib.rs:2`, `plugin.rs:209`
- **OpenCode**: Strict tsconfig with dead code detection
- **RustCode**: `#![allow(dead_code, unused_imports, unused_variables)]` crate-wide
- **Description**: Dead code allowance suppresses 50+ dead items; real dead code undetectable
- **Recommendation**: Scope allowances to individual items, not entire crate
- **Severity**: Medium
- **Source Agent**: Agent 03

- **Gap ID**: GAP-RUST-012
- **Location**: `event.rs:42-79`, cross-cutting
- **OpenCode**: Branded types for all IDs
- **RustCode**: Mixed — `EventId(String)` is proper newtype, but `SessionId`, `ModelId`, `ProviderId` etc. are type aliases
- **Description**: Inconsistent approach to ID types; `EventId` is correct but most other IDs are not
- **Recommendation**: Extend newtype pattern to all domain IDs
- **Severity**: Medium
- **Source Agent**: Agent 03

- **Gap ID**: GAP-RUST-013
- **Location**: `provider.rs:906-940`, whole codebase
- **OpenCode**: Effect's `Context.Service` + `Layer` DI
- **RustCode**: Manual injection via `Arc<DatabaseService>`, `SharedBus`, `Arc<ToolRegistry>`, etc.
- **Description**: Every new service dependency requires changing constructors and all call sites; no scoping (singleton vs request-scoped vs session-scoped)
- **Recommendation**: Introduce `ServiceRegistry` or `AppContext` struct holding all services as `Arc<dyn ...>`
- **Severity**: Medium
- **Source Agent**: Agent 03

---

## 3. Logic Gaps

### Critical

- **Gap ID**: GAP-LOGIC-001
- **Location**: `session.rs:1206-1215`
- **OpenCode**: `packages/opencode/src/session/session.ts:828-830`
- **RustCode**: `clear_revert` writes literal string `"null"` instead of SQL `NULL` — `Some("null")` passed to `update_session`
- **Description**: Column physically contains 4-character string `"null"` instead of `NULL`; `WHERE revert IS NULL` misses this row
- **Consequence**: Data corruption — SQL queries using `IS NULL` will miss this row; serde_json::from_str("null") returns string, not null
- **Recommendation**: Pass `None` instead of `Some("null")` to set column to SQL `NULL`
- **Severity**: Critical
- **Source Agent**: Agent 04

- **Gap ID**: GAP-LOGIC-002
- **Location**: `session_runner.rs:1086-1096`
- **OpenCode**: V1 run loop with permission checks
- **RustCode**: V1 `run_loop` sets `ask_fn: None`, `permission_source: None`, calls `execute_by_name` — zero permission checks
- **Description**: Both `ToolContext` fields are `None`; `execute_by_name` has no permission check; `execute_with_pipeline` (which has permission flow) is bypassed
- **Consequence**: LLM can call `bash`, `read`, `write`, `edit` without any allow/deny/ask check — complete permission bypass in V1 mode
- **Recommendation**: Wire `ask_fn` and `permission_source` into V1 paths; switch V1 to `execute_with_pipeline`
- **Severity**: Critical
- **Source Agent**: Agent 04

- **Gap ID**: GAP-LOGIC-003
- **Location**: `session_runner.rs:703-717`
- **OpenCode**: `packages/core/src/session/runner/llm.ts:345-357`
- **RustCode**: `compact_result.as_ref().unwrap()` violates rule #3; `serde_json::json!` with `.map()` inside produces `Some(...)` wrappers in JSON
- **Description**: Three issues: (1) unwrap on compact_result despite is_some() guard, (2) redundant `.map()` inside json! produces `{"summary": Some("...")}` with literal `Some(...)` wrappers, (3) prepare_epoch called twice
- **Consequence**: JSON corruption — epoch snapshot storage contains `Some(...)` wrappers; downstream deserialization fails
- **Recommendation**: Use `if let Some(ref result) = compact_result` pattern; access fields directly not via `.map()`
- **Severity**: Critical
- **Source Agent**: Agent 04

- **Gap ID**: GAP-LOGIC-004
- **Location**: `session.rs:1420`
- **OpenCode**: `packages/opencode/src/session/session.ts fromRow()`
- **RustCode**: `cost: f64` — prevents deriving `Eq` on `SessionInfo`; floating-point precision loss on monetary values
- **Description**: JSON round-trips of `cost` silently lose precision; `0.1 + 0.2 != 0.3`; cannot derive `Eq`
- **Consequence**: Cannot derive `Eq` on `SessionInfo`; monetary values accumulate rounding errors over repeated update cycles
- **Recommendation**: Use `ordered_float::OrderedFloat<f64>` or store cost as `i64` (millicents)
- **Severity**: Critical
- **Source Agent**: Agent 04

### High

- **Gap ID**: GAP-LOGIC-005
- **Location**: `session_execution.rs:744-755`
- **RustCode**: TOCTOU race in `wake()` — reads then drops lane reference, then reacquires for mutation
- **Description**: Between `drop(lane)` and `get_mut`, another task can modify/remove the lane
- **Consequence**: Wake silently lost; lane state changed between read and write; multiple concurrent wakes interleave
- **Recommendation**: Use `DashMap::alter` for atomic read-modify-write
- **Severity**: High
- **Source Agent**: Agent 04

- **Gap ID**: GAP-LOGIC-006
- **Location**: `session.rs:866-892`
- **RustCode**: `fork` loop breaks before inserting stop message into `id_map`
- **Description**: `clone_with_session` uses `id_map` to remap `parent_id` references; stop message mapping missing
- **Consequence**: Parent reference would be unmapped, producing empty/invalid parent_id
- **Recommendation**: Move insert before break in the loop
- **Severity**: High
- **Source Agent**: Agent 04

- **Gap ID**: GAP-LOGIC-007
- **Location**: `session_execution.rs:827-834`
- **RustCode**: `await_idle` — unbounded busy-wait with no timeout, no cancellation token
- **Description**: If drain task deadlocks, lane never removed, `await_idle` spins forever
- **Consequence**: A stuck drain fiber causes `run()` to hang forever; process won't terminate without SIGKILL
- **Recommendation**: Add timeout with max retry limit
- **Severity**: High
- **Source Agent**: Agent 04

- **Gap ID**: GAP-LOGIC-008
- **Location**: `session.rs:885`
- **RustCode**: `part_id` generation failure falls back to empty string via `unwrap_or_default()`
- **Description**: If `id::ascending()` fails, empty string `""` used as primary key; all failed parts get same empty ID
- **Consequence**: Unique constraint violations or silent overwrites in the database
- **Recommendation**: Propagate error instead of `unwrap_or_default()`
- **Severity**: High
- **Source Agent**: Agent 04

- **Gap ID**: GAP-LOGIC-009
- **Location**: `session_execution.rs:165,168`
- **RustCode**: FiberSet spawn results silently dropped if receiver is closed (`let _ = result_tx.send(...)`)
- **Description**: Both sends discard `SendError` if receiver dropped; fibers leak in `handles` and `cancels` maps
- **Consequence**: `await_empty` never returns; resource leak + hang
- **Recommendation**: Clean up fiber handle when send fails
- **Severity**: High
- **Source Agent**: Agent 04

- **Gap ID**: GAP-LOGIC-010
- **Location**: `session_runner.rs:1308-1323`
- **RustCode**: `check_context_overflow` assumes 1 token = 4 bytes; real tokenizers average 3-5 chars/token for English, 8-20 for code/JSON
- **Description**: Naive token estimation causes false positives — artificially triggers overflow compaction when real token count is within limits
- **Consequence**: Premature compaction shrinks context, losing session history; compaction loops
- **Recommendation**: Use more accurate estimator (`tiktoken-rs`) or adjust divisor to 5-6
- **Severity**: High
- **Source Agent**: Agent 04

- **Gap ID**: GAP-LOGIC-011
- **Location**: `session_runner.rs:928-947`
- **RustCode**: `parse_turn_control` — fragile string matching on encoded control flow
- **Description**: `TurnControl` serialized as string inside `Error::Internal`, parsed via substring matching
- **Consequence**: Format changes or localization silently break overflow recovery; provider error messages containing these strings cause false positives
- **Recommendation**: Use dedicated error variant: `Error::TurnControl(TurnControl)`
- **Severity**: High
- **Source Agent**: Agent 04

- **Gap ID**: GAP-LOGIC-012
- **Location**: `tool.rs:502`
- **RustCode**: Permission check always passes `"*"` as resource, regardless of tool
- **Description**: `ctx.ask(name, "*")` — hardcoded resource defeats pattern-based permission granularity
- **Consequence**: Users cannot configure fine-grained permissions like `"read": "/etc/*"` — the wildcard matches everything
- **Recommendation**: Extract real resource from tool arguments
- **Severity**: High
- **Source Agent**: Agent 04

- **Gap ID**: GAP-LOGIC-013
- **Location**: `session_execution.rs:153-179`
- **RustCode**: `JoinHandle` stored but never awaited in `cancel()` or `cancel_all()` — tasks leak on drop
- **Description**: `FiberSet::spawn` stores handle but `cancel()` only signals `CancellationToken`, does not await handle
- **Consequence**: Tasks leak on shutdown; in-flight tasks abruptly cancelled on runtime drop
- **Recommendation**: Add `cancel_and_join()` that awaits the handle
- **Severity**: High
- **Source Agent**: Agent 04

- **Gap ID**: GAP-LOGIC-014
- **Location**: `session_runner.rs:659-661`
- **RustCode**: V1 and V2 ignore `StepFinish` reason — `LlmEvent::StepFinish { reason, .. } => { let _ = reason; }`
- **Description**: Finish reason discarded; `needs_continuation` defaults to false; `FinishReason::Length` or `FinishReason::Error` not propagated
- **Consequence**: Truncated responses appear as complete; caller cannot distinguish complete vs truncated response
- **Recommendation**: Propagate finish reason; set `needs_continuation = true` if reason is `Length`
- **Severity**: High
- **Source Agent**: Agent 04

- **Gap ID**: GAP-LOGIC-015
- **Location**: `session_runner.rs:1179-1191`
- **RustCode**: Sends two system messages when `input.system` is set — most LLM providers only support single system message
- **Description**: `system_prompt` + `input.system` both pushed as separate `ChatMessage::System`
- **Consequence**: Provider API calls may fail or produce unexpected behavior for Anthropic, many OpenAI models
- **Recommendation**: Merge the two system messages into one
- **Severity**: High
- **Source Agent**: Agent 04

- **Gap ID**: GAP-LOGIC-016
- **Location**: `session_execution.rs:726-727, 946-947`
- **RustCode**: `Running` state never explicitly set to `Idle` after wake completes
- **Description**: `wake()` sets state to `Running` but returns immediately; state remains `Running` until fiber's `settle` runs
- **Consequence**: External observers see `Running` when system considers wake "submitted" not "active" — conflates two states
- **Recommendation**: Add third state `Pending` or clarify documentation
- **Severity**: High
- **Source Agent**: Agent 04

---

## 4. Security Gaps

### High

- **Gap ID**: GAP-SEC-001
- **Location**: `rustcode-core/src/auth.rs:195-206`, `mcp.rs:2263-2276`, `credential.rs:352-353`
- **OpenCode**: Same patterns (upstream)
- **RustCode**: `auth.json`, `mcp-auth.json`, SQLite credential values all stored in plaintext
- **Description**: OAuth tokens, API keys, credentials stored without encryption at rest
- **Consequence**: Anyone with filesystem access can read all stored credentials and tokens
- **Recommendation**: Encrypt at rest using OS keychain or `age`/`rage`; at minimum document plaintext storage
- **Severity**: High
- **Source Agent**: Agent 05

- **Gap ID**: GAP-SEC-002
- **Location**: `rustcode-core/src/providers/*:resolve_api_key()`
- **OpenCode**: Same pattern
- **RustCode**: API keys read from env vars into `String` — live in heap until process exit
- **Description**: No memory zeroing for secrets; keys may be swapped to disk, visible in core dumps
- **Recommendation**: Use `secrecy::SecretString` for all API key and token fields
- **Severity**: Medium
- **Source Agent**: Agent 05

- **Gap ID**: GAP-SEC-003
- **Location**: `deny.toml:3`
- **OpenCode**: N/A
- **RustCode**: `RUSTSEC-2024-0436` ignored without documented rationale
- **Description**: Supply chain advisory ignored; unmaintained `paste` crate is transitive dependency
- **Consequence**: Unpatched dependency vulnerability; no documented risk assessment
- **Recommendation**: Investigate and either fix or document rationale in `deny.toml` comments
- **Severity**: High
- **Source Agent**: Agent 05

- **Gap ID**: GAP-SEC-004
- **Location**: `rustcode-core/src/encryption/hmac.rs`
- **OpenCode**: HMAC-based encryption module exists
- **RustCode**: File does not exist — encryption module not ported
- **Description**: HMAC-based credential encryption from OpenCode has not been implemented
- **Consequence**: No encryption-at-rest for any stored credential
- **Recommendation**: Implement `encryption/hmac.rs` or equivalent
- **Severity**: High
- **Source Agent**: Agent 05

- **Gap ID**: GAP-SEC-005
- **Location**: `rustcode-core/src/mcp.rs:1044-1051`
- **OpenCode**: Same pattern
- **RustCode**: MCP local servers spawned with full user privileges via `tokio::process::Command::new(cmd)`
- **Description**: MCP servers run with user's full permissions; no sandboxing
- **Consequence**: Malicious MCP server config leads to arbitrary code execution
- **Recommendation**: Consider running MCP servers in restricted context (containers, landlock)
- **Severity**: High
- **Source Agent**: Agent 05

- **Gap ID**: GAP-SEC-006
- **Location**: `rustcode-core/src/config.rs:2796-2789, 2804-2818`
- **OpenCode**: Same pattern
- **RustCode**: `{file:path}` variable substitution reads arbitrary files — path traversal via `{file:../../etc/passwd}`
- **Description**: Config `{file:/etc/shadow}` reads sensitive files; no path restriction
- **Consequence**: Attacker with crafted config can exfiltrate local files
- **Recommendation**: Restrict `{file:}` paths to project directory; use canonicalized paths
- **Severity**: High
- **Source Agent**: Agent 05

- **Gap ID**: GAP-SEC-007
- **Location**: `rustcode-core/src/config.rs:2763`
- **OpenCode**: Same pattern
- **RustCode**: File read in variable substitution with no path restriction
- **Description**: Identical issue to SEC-006 in different code path
- **Recommendation**: Sanitize file paths; enforce project directory boundary
- **Severity**: High
- **Source Agent**: Agent 05

- **Gap ID**: GAP-SEC-008
- **Location**: `rustcode-server/src/auth.rs:81-87`
- **OpenCode**: Same pattern
- **RustCode**: `auth_token` query parameter supported — credentials in URL
- **Description**: Auth token in query params logged by proxies, web servers; visible in browser history; leaked via Referer headers
- **Recommendation**: Log warning; document that query-param auth is less secure
- **Severity**: Medium
- **Source Agent**: Agent 05

---

## 5. Performance Gaps

### Critical

- **Gap ID**: GAP-PERF-001
- **Location**: `tool_impls.rs:1065-1238`, `filesystem.rs:1281`
- **OpenCode**: `fs/promises` (async I/O via libuv thread pool)
- **RustCode**: Synchronous `std::fs` operations on tokio async runtime
- **Description**: All filesystem operations in tool implementations use blocking `std::fs` APIs; blocks tokio worker threads
- **Consequence**: Tokio worker threads blocked for milliseconds per file read; in server context, blocks all connected clients
- **Recommendation**: Use `tokio::fs` or wrap blocking I/O in `spawn_blocking`
- **Severity**: Critical
- **Source Agent**: Agent 06

- **Gap ID**: GAP-PERF-002
- **Location**: `filesystem.rs:1281`
- **OpenCode**: Delegates to ripgrep (memory-mapped, streaming)
- **RustCode**: Reads entire matching files into memory (`std::fs::read_to_string`)
- **Description**: grep_search reading 50 files of 5MB each allocates 250MB simultaneously
- **Consequence**: High peak memory usage during grep operations; potential OOM on large repos
- **Recommendation**: Delegate to ripgrep or read files in chunks with buffered I/O
- **Severity**: Critical
- **Source Agent**: Agent 06

- **Gap ID**: GAP-PERF-003
- **Location**: `tool_impls.rs:1226-1230`
- **OpenCode**: Reads up to cap
- **RustCode**: Reads full file then truncates to 50KB — for 500MB log file, reads all 500MB into memory
- **Description**: ReadTool reads entire file even though only 50KB is needed
- **Consequence**: 500MB heap allocation and I/O for 50KB of useful output
- **Recommendation**: Use `take(MAX_READ_BYTES)` on file handle to limit read
- **Severity**: Critical
- **Source Agent**: Agent 06

### High

- **Gap ID**: GAP-PERF-004
- **Location**: `session_runner.rs:749`
- **OpenCode**: Messages by reference
- **RustCode**: `ctx.messages = messages.clone()` — clones entire `Vec<ChatMessage>` history per tool call
- **Description**: For 50 messages at ~2KB each = ~100KB per tool call; with 25 tools = 2.5MB per session
- **Consequence**: Massive unnecessary memory allocation in hot path
- **Recommendation**: Wrap in `Arc<Vec<ChatMessage>>` or pass as `&[ChatMessage]`
- **Severity**: High
- **Source Agent**: Agent 06

- **Gap ID**: GAP-PERF-005
- **Location**: `event.rs:936-1035`
- **OpenCode**: Single-path event publishing
- **RustCode**: Each sync event clones `EventPayload` 8+ times (guards, projectors, sync handlers, aggregate subscribers, listeners, typed channel, global channel)
- **Description**: `EventPayload` contains EventId (String), event_type (String), data (Value), location (Option), metadata (Option) — 500+ bytes per clone
- **Consequence**: Each sync event pays ~4KB of unnecessary clone overhead
- **Recommendation**: Pass `&EventPayload` where possible; only clone for broadcast
- **Severity**: High
- **Source Agent**: Agent 06

- **Gap ID**: GAP-PERF-006
- **Location**: `event.rs:899-984`
- **OpenCode**: Transaction per atomic operation
- **RustCode**: Transaction held open during async operations (commit guards, projectors, commit hook)
- **Description**: If projectors take 100ms, transaction holds for 100ms blocking other DB writers
- **Consequence**: Blocks other database writers during projector execution
- **Recommendation**: Move projectors and commit hooks outside transaction; only seq UPSERT + event INSERT need to be in transaction
- **Severity**: High
- **Source Agent**: Agent 06

---

## 6. Scalability Gaps

### Critical

- **Gap ID**: GAP-SCALE-001
- **Location**: Whole codebase, `opencode/infra/`
- **OpenCode**: Cloudflare Workers (300+ locations), Durable Objects, PlanetScale (Vitess), distributed coordination
- **RustCode**: Single-process, single-node SQLite. No distributed primitives — no service discovery, no leader election
- **Description**: Complete absence of distributed infrastructure
- **Consequence**: Cannot run as multi-instance service; any clustering attempt encounters split-brain for writes
- **Recommendation**: Accept as local-first design; if distributed needed, use external KV store (Redis/FoundationDB)
- **Severity**: Critical
- **Source Agent**: Agent 07

- **Gap ID**: GAP-SCALE-002
- **Location**: `rustcode-core/src/database.rs:59-66`, `opencode/infra/console.ts:11-44`
- **OpenCode**: PlanetScale (MySQL-compatible Vitess) with horizontal read replicas, sharding
- **RustCode**: SQLite with WAL mode — single-writer bottleneck
- **Description**: SQLite is fundamentally single-writer; at ~100+ concurrent sessions, SQLite contention dominates
- **Consequence**: Write capacity stuck at 1 regardless of instances; SQLITE_BUSY errors at scale
- **Recommendation**: Keep SQLite for local; for production, abstract `DatabaseService` behind trait and implement PostgreSQL variant
- **Severity**: Critical
- **Source Agent**: Agent 07

- **Gap ID**: GAP-SCALE-003
- **Location**: `rustcode-core/src/event.rs:775-799`, `opencode/infra/app.ts`
- **OpenCode**: Stateless Workers — crash means next request hits another worker; Durable Objects persist to CF storage
- **RustCode**: No cross-node fault tolerance; crash loses all in-memory state (SharedBus subscribers, AppState, in-flight streams)
- **Description**: Process crash loses: pending SSE connections, in-flight LLM streams, event bus subscribers, session runner state
- **Consequence**: Any process restart forcibly disconnects all clients; sessions only survive if persisted to SQLite
- **Recommendation**: Implement SSE reconnect with event replay; persist bus events to SQLite
- **Severity**: Critical
- **Source Agent**: Agent 07

- **Gap ID**: GAP-SCALE-007
- **Location**: `rustcode-core/src/database.rs:59-66`
- **OpenCode**: PlanetScale MySQL (Vitess) — thousands of writers
- **RustCode**: SQLite single-file (WAL) — one writer
- **Description**: SQLite vs PlanetScale is the biggest architectural divergence
- **Consequence**: Hard wall at ~1 writer; read contention grows with reader count
- **Recommendation**: Abstract DatabaseService behind trait; implement PostgreSQL for multi-instance
- **Severity**: Critical
- **Source Agent**: Agent 07

- **Gap ID**: GAP-SCALE-012
- **Location**: `rustcode-core/src/`, database schema
- **OpenCode**: Full multi-tenant SaaS; PlanetScale, Cloudflare R2, auth middleware, org management, Stripe billing
- **RustCode**: Single-user, single-workspace. No tenant concept. Tables for `account`, `workspace` exist but are scaffold-only
- **Description**: No multi-tenant infrastructure; no auth middleware, no org management, no billing
- **Consequence**: Cannot serve multiple users or organizations; suitable only as local CLI tool
- **Recommendation**: Document as single-user only; implement workspace isolation via `workspace_id` column if needed
- **Severity**: Critical
- **Source Agent**: Agent 07

### High

- **Gap ID**: GAP-SCALE-005
- **Location**: `event.rs:1359-1422`, `session_runner.rs:467-517`
- **OpenCode**: EventV2 replay + SessionRunCoordinator.resume()
- **RustCode**: EventV2 port structurally complete but not wired into session recovery; no mechanism to resume interrupted turn
- **Description**: Crash recovery infrastructure exists but is disconnected from session runner
- **Consequence**: Crashed sessions must fully restart from last persisted epoch; all in-flight work lost
- **Recommendation**: Wire EventV2 replay into session initialization; implement `resume()` function
- **Severity**: High
- **Source Agent**: Agent 07

- **Gap ID**: GAP-SCALE-006
- **Location**: `bus.rs:208-258`, `sse.rs:29-58`
- **OpenCode**: Effect Stream with per-type PubSub + backpressure
- **RustCode**: `tokio::sync::broadcast` channel (capacity 1024) — lagged receivers skip events; no per-client backpressure
- **Description**: One slow consumer causes event loss for all consumers; no backpressure between LLM stream and client SSE
- **Consequence**: Under high throughput (50+ events/sec), slow SSE consumers lose events; gap between producer and consumer unbounded
- **Recommendation**: Replace broadcast with per-subscriber `mpsc` channels for SSE; add bounded buffer with rejection
- **Severity**: High
- **Source Agent**: Agent 07

- **Gap ID**: GAP-SCALE-009
- **Location**: `bus.rs:196-258`, `event.rs:632-697,855-1063`
- **OpenCode**: EventV2 unified — all events persisted, single pipeline
- **RustCode**: Dual bus — `SharedBus` (in-memory broadcast) and `EventV2` (DB-backed); they don't interoperate
- **Description**: Two bus systems that don't communicate — events on SharedBus are not persisted; events on EventV2 not on SharedBus
- **Consequence**: CRUD events lost on crash; no single event pipeline end-to-end; confusion about where to publish
- **Recommendation**: Merge into one event bus; route all events through EventV2; make SharedBus thin wrapper
- **Severity**: High
- **Source Agent**: Agent 07

- **Gap ID**: GAP-SCALE-011
- **Location**: `session_runner.rs:37-43`
- **OpenCode**: Enterprise tier-based limits (ZenLite/ZenBlack), stats pipeline, billing limits
- **RustCode**: Step limits only (MAX_STEPS=25); no per-session resource budgets (memory, tokens, cost); no global caps
- **Description**: No memory limits per session; no token budgeting; no cost tracking beyond DB schema
- **Consequence**: Runaway session can exhaust system memory; no cost control for LLM API usage
- **Recommendation**: Add per-session token budget with overflow handling; implement per-session cost tracking with hard limits
- **Severity**: High
- **Source Agent**: Agent 07

- **Gap ID**: GAP-SCALE-013
- **Location**: `database.rs:63`
- **OpenCode**: Upstash Redis for caching; Cloudflare CDN; PlanetScale query cache
- **RustCode**: SQLite page cache only (64MB); no in-memory session cache; no query result caching
- **Description**: Zero application-level caching — every session list, message read, part query goes to SQLite
- **Consequence**: Repeated `get_messages()` on same session re-queries SQLite each time; session listing requires full table scan at scale
- **Recommendation**: Implement in-memory session cache via `dashmap` with TTL; add LRU cache for part deserialization
- **Severity**: High
- **Source Agent**: Agent 07

- **Gap ID**: GAP-SCALE-014
- **Location**: `rustcode-server/src/server.rs:217`
- **OpenCode**: Upstash Redis + Cloudflare rate limiting + tier-based limits
- **RustCode**: No rate limiting; no token bucket, no leaky bucket, no per-user rate limits
- **Description**: No protection against abuse or runaway usage; provider API costs unbounded
- **Consequence**: Single client can exhaust LLM API budget, saturate SQLite, consume all SSE connections
- **Recommendation**: Implement token bucket rate limiter; add per-route rate limiting middleware; add exponential backoff for 429 responses
- **Severity**: High
- **Source Agent**: Agent 07

---

## 7. Feature Gaps

> Note: All 86 RustCode modules from the pinned OpenCode commit have corresponding `.rs` files — **structural parity is 100%**. However, functional parity is ~20% — most modules are type skeletons with key traits; actual business logic is largely unported.

### Critical

- **Gap ID**: GAP-FEAT-001
- **Location**: `rustcode-core/src/session*` (14 sub-modules)
- **OpenCode**: V2 Session architecture — Effect-native, durable prompt, algebraic system context, epoch-based state, input inbox, message projection, revert, reminders, todo, runner lifecycle
- **RustCode**: Core types exist but ~4,000 LOC of sophisticated state machine logic barely begun
- **Description**: The session system is the single biggest feature gap — no prompt assembly, no runner, no message projection, no input inbox
- **Consequence**: Core session loop missing — RustCode cannot run basic agent sessions
- **Recommendation**: Implement V2 Session architecture: durable prompt, algebraic system context, epoch-based state machine
- **Severity**: Critical
- **Source Agent**: Agent 08

- **Gap ID**: GAP-FEAT-002
- **Location**: `rustcode-core/src/event.rs`
- **OpenCode**: EventV2 — durable event streams with SQL persistence, replay, algebraic projection (~2,000 LOC)
- **RustCode**: EventV2 types and port exist but durable event streams, replay, and algebraic projection not fully implemented
- **Description**: EventV2 is the backbone of session state — without it, session state persistence and recovery are impossible
- **Consequence**: No crash recovery, no event-sourced session reconstruction, no replay
- **Recommendation**: Implement full EventV2 with append-only event store, replay, aggregate sequence cursor
- **Severity**: Critical
- **Source Agent**: Agent 08

- **Gap ID**: GAP-FEAT-003
- **Location**: `rustcode-core/src/provider.rs`, `rustcode-core/src/providers/`
- **OpenCode**: 30+ LLM providers using route-based architecture with 4-axis composition (Protocol, Endpoint, Auth, Framing)
- **RustCode**: Provider trait defined, only Anthropic implemented in detail; no route architecture
- **Description**: Each new protocol requires 300-400 lines of boilerplate; non-OpenAI protocols have zero shared protocol logic
- **Consequence**: Only 1/30+ providers implemented — no actual LLM calls work in production
- **Recommendation**: Implement route architecture; implement Anthropic and OpenAI providers first (cover ~80% of users)
- **Severity**: Critical
- **Source Agent**: Agent 08

- **Gap ID**: GAP-FEAT-004
- **Location**: `rustcode-core/src/tool_impls.rs:7235 LOC`
- **OpenCode**: All tool implementations complete (Bash, Read, Write, Edit, Glob, Grep, WebFetch, etc.)
- **RustCode**: 7,235 LOC of stub tool implementations — function signatures exist but logic is placeholders
- **Description**: Tool implementations are stub/trait-heavy with minimal actual execution logic
- **Consequence**: Tools don't function — cannot execute bash, read files, edit code, search, etc.
- **Recommendation**: Implement each tool with real logic; start with Bash, Read, Write, Edit
- **Severity**: Critical
- **Source Agent**: Agent 08

- **Gap ID**: GAP-FEAT-005
- **Location**: `rustcode-core/src/config.rs:4861 LOC`
- **OpenCode**: Full config parsing (toml/json/yaml), env interpolation, plugin config, tool-output config, TUI config
- **RustCode**: Config struct and types exist but config loading pipeline is not wired
- **Description**: Config parsing logic is scaffolded but cannot actually load `opencode.json`
- **Consequence**: Cannot read configuration; no persistent user settings
- **Recommendation**: Wire config loading pipeline; implement TOML parsing with env interpolation
- **Severity**: Critical
- **Source Agent**: Agent 08

---

## 8. API Gaps

### Critical

- **Gap ID**: GAP-API-001
- **Location**: `Cargo.toml` (all crates at version `0.1.0`)
- **OpenCode**: NPM packages use semver; `@opencode-ai/sdk@1.17.8`; V1/V2 coexistence with feature flags
- **RustCode**: All crates at `0.1.0`; no versioning policy; no changelog; no migration guide
- **Description**: No semver discipline; no `#[deprecated]` annotations; no public API compatibility testing
- **Consequence**: Any change could be breaking; downstream consumers cannot safely depend on any API surface
- **Recommendation**: Establish semver policy; add `cargo-semver-checks` in CI; add API compat test suite
- **Severity**: Critical
- **Source Agent**: Agent 09

- **Gap ID**: GAP-API-002
- **Location**: `rustcode-server/src/routes/api.rs:116-181`
- **OpenCode**: Server routes in `packages/server/src/api.ts` + 30+ group files, all implemented
- **RustCode**: 25+ route handlers are stubs — `api_session_prompt`, `api_session_compact`, `api_session_wait` return placeholder data
- **Description**: Handlers lack real implementation; many return `NO_CONTENT`; `api_fs` reads files directly without permission checks
- **Consequence**: Server crate cannot serve as real backend; SSE event streaming works but session prompt execution is stub-only
- **Recommendation**: Implement at minimum session CRUD + prompt execution paths; add auth middleware
- **Severity**: Critical
- **Source Agent**: Agent 09

- **Gap ID**: GAP-API-003
- **Location**: `packages/sdk/js/package.json` (OpenCode SDK)
- **OpenCode**: `@opencode-ai/sdk@1.17.8` with auto-generated REST client, typed V2 API surface, lifecycle helpers
- **RustCode**: No equivalent SDK crate; no `rustcode-client` crate; no auto-generated client
- **Description**: Critical gap — Rust consumers must embed the server crate directly, shell out to binary, or write own HTTP client
- **Consequence**: No "nice" programmatic API for Rust consumers; audience limited to embedding crate directly
- **Recommendation**: Create `rustcode-client` crate with typed async HTTP client similar to `@opencode-ai/sdk`
- **Severity**: Critical
- **Source Agent**: Agent 09

- **Gap ID**: GAP-API-004
- **Location**: `packages/sdk/js/src/gen/` (auto-generated), `rustcode-core/src/` (hand-written)
- **OpenCode**: SDK types and client auto-generated from OpenAPI spec via `@hey-api/openapi-ts`
- **RustCode**: All types hand-written ports from TypeScript; no code generation; no shared spec
- **Description**: Manual porting creates drift; as OpenCode evolves, RustCode types must be manually updated
- **Consequence**: RustCode will fall behind OpenCode's schema changes; type mismatches accumulate
- **Recommendation**: Generate Rust types from OpenAPI spec or TypeScript source; add schema validation test
- **Severity**: Critical
- **Source Agent**: Agent 09

### High

- **Gap ID**: GAP-API-005
- **Location**: `rustcode-core/src/lib.rs:11-95` (all 95 modules `pub`)
- **OpenCode**: Effect.ts `Context.Tag`, explicit service interfaces, `export`-controlled module boundaries
- **RustCode**: `pub mod` for all 95 modules — every module public with no sub-visibility
- **Description**: Zero API firewall; every internal helper, stub, and skeleton module is `pub`
- **Consequence**: Breaking changes can impact downstream consumers of any module; docs include all internal modules
- **Recommendation**: Add `#[doc(hidden)]`; adopt `pub(crate)` as default; promote to `pub` only for API boundary
- **Severity**: High
- **Source Agent**: Agent 09

- **Gap ID**: GAP-API-006
- **Location**: `provider.rs:298-299` (`providerID`, `modelID`), `config.rs:99` (`logLevel`)
- **OpenCode**: Consistent camelCase in JSON wire format
- **RustCode**: Mixed conventions — some serde `rename` attributes use camelCase, some use snake_case
- **Description**: Some Rust structs fail to match OpenCode's JSON wire format due to inconsistent/missing serde rename attributes
- **Consequence**: Clients receiving JSON get `provider_id` instead of expected `providerID`, breaking compatibility
- **Recommendation**: Audit all Serialize/Deserialize structs; add `#[serde(rename_all = "camelCase")]` consistently
- **Severity**: High
- **Source Agent**: Agent 09

- **Gap ID**: GAP-API-007
- **Location**: `rustcode-core/src/lib.rs:11-95`
- **OpenCode**: `_internal.ts` / `internal/` directories + explicit export lists
- **RustCode**: Every module `pub` regardless of whether it's internal implementation detail
- **Description**: No internal module hiding; `database.rs`, `flock.rs`, `ripgrep.rs` are implementation details but all `pub`
- **Consequence**: Impossible to refactor internals without potentially breaking external consumers
- **Recommendation**: Audit each module; use `pub(crate)` for internals; expose only through controlled re-exports
- **Severity**: High
- **Source Agent**: Agent 09

- **Gap ID**: GAP-API-008
- **Location**: `rustcode-core/src/error.rs:613-650` (ApiError), `rustcode-server/src/error.rs:19-243` (ServerError)
- **OpenCode**: Single error hierarchy; server layer maps domain errors to HTTP responses
- **RustCode**: Two separate error enums with overlapping semantics; `IntoServerError` trait exists but is empty
- **Description**: Duplicate error types — both have NotFound, InvalidRequest equivalents; no documented conversion path
- **Consequence**: Error mapping from core to server is ad-hoc; some handlers use `ServerError::unknown(e.to_string())` losing structured data
- **Recommendation**: Remove ApiError from core; use ServerError exclusively in server crate; implement proper `From<core::Error>`
- **Severity**: High
- **Source Agent**: Agent 09

- **Gap ID**: GAP-API-009
- **Location**: `rustcode-core/src/provider.rs:24-48` (ModelId, ProviderId), `session.rs:83-90` (SessionId, MessageId, PartId)
- **OpenCode**: Branded string types with runtime validation
- **RustCode**: All IDs are `pub type X = String` — zero compile-time type safety
- **Description**: Can pass SessionId where MessageId is expected with no compiler error
- **Consequence**: Runtime errors are the only guard against ID type confusion
- **Recommendation**: Convert each ID alias to a newtype wrapper with `new()`, `as_str()`, `Serialize`/`Deserialize`
- **Severity**: High
- **Source Agent**: Agent 09

- **Gap ID**: GAP-API-010
- **Location**: `rustcode-server/` (no spec file)
- **OpenCode**: `packages/sdk/openapi.json` — auto-generated OpenAPI spec driving SDK generation
- **RustCode**: No OpenAPI spec; no SDK generation; API surface documented only in code comments
- **Description**: Missing contract-first API documentation
- **Consequence**: Consumers must reverse-engineer API from handler code; no automated client generation
- **Recommendation**: Generate OpenAPI 3.0 spec from axum routes using `utoipa` or `aide`
- **Severity**: High
- **Source Agent**: Agent 09

- **Gap ID**: GAP-API-011
- **Location**: `rustcode-lsp/src/lib.rs:48-113` (LspError)
- **OpenCode**: LSP errors part of unified error hierarchy
- **RustCode**: `rustcode-lsp` defines its own `LspError` enum (10 variants); does NOT use `rustcode_core::error::Error`
- **Description**: Third error type in ecosystem (core Error, server ServerError, lsp LspError); no conversion between them
- **Consequence**: LSP callers must handle separate error type; cannot use `?` to propagate from LSP code to session code
- **Recommendation**: Remove `LspError`; use `rustcode_core::error::Error` with Lsp variant; implement `From<LspError>`
- **Severity**: High
- **Source Agent**: Agent 09

---

## 9. Testing Gaps

### Critical

- **Gap ID**: GAP-TEST-001
- **Location**: `crates/rustcode-core/src/integration.rs:885` (30 tests), `database.rs:3304` (20 tests)
- **OpenCode**: `http-recorder` package for deterministic replay of LLM provider API calls; Effect test layers
- **RustCode**: No HTTP recording/replay infrastructure; provider tests cannot safely execute code paths
- **Description**: No equivalent of OpenCode's `http-recorder` — cannot deterministically replay LLM provider conversations
- **Consequence**: Session runner tests lack recorded cassettes for deterministic replay; tests that touch network are skipped or fragile
- **Recommendation**: Port `http-recorder` pattern to Rust using reqwest middleware or custom HttpClient trait with recording/replay
- **Severity**: Critical
- **Source Agent**: Agent 10

- **Gap ID**: GAP-TEST-002
- **Location**: All crates — no e2e tests
- **OpenCode**: Playwright-based e2e in `packages/app/e2e/` (smoke, regression, performance)
- **RustCode**: Zero end-to-end tests
- **Description**: No CLI invocation tests, no TUI integration tests, no full workflow tests
- **Consequence**: Release quality depends entirely on unit tests; regression in CLI argument parsing, TUI rendering, or cross-crate interaction goes undetected
- **Recommendation**: Add `trycmd` or `assert_cmd` for CLI binary testing; add server→SSE→prompt→response integration test
- **Severity**: Critical
- **Source Agent**: Agent 10

- **Gap ID**: GAP-TEST-003
- **Location**: All crates
- **OpenCode**: Effect's Layer system for DI; `http-recorder` records/replays HTTP traffic
- **RustCode**: No mocking framework (`mockall`, `mockito`, `wiremock` not in deps); providers are traits but tests don't mock them
- **Description**: Without mock strategy, provider tests require real API keys; tests that touch network are skipped or fragile
- **Consequence**: Provider integration tests cannot run in CI; session runner tests cannot safely execute provider calls
- **Recommendation**: Implement HttpClient trait that can be swapped between real/replay/record modes; add `wiremock` or `mockito`
- **Severity**: Critical
- **Source Agent**: Agent 10

### High

- **Gap ID**: GAP-TEST-004
- **Location**: All crates
- **OpenCode**: No explicit coverage tool, but `bun test --coverage` available
- **RustCode**: No coverage tool configured; estimated coverage ~60-70% but 11 modules have zero tests
- **Description**: `providers/openai.rs`, `providers/gemini.rs`, `credential.rs`, `bus.rs`, `system_context.rs`, `model.rs`, `policy.rs`, `event.rs`, `v2_schema.rs` — 9 modules with `mod tests {}` but zero test functions
- **Consequence**: Coverage invisible and cannot be gated; regressions in untested modules undetected
- **Recommendation**: Add `cargo-tarpaulin` or `grcov` to CI; set minimum coverage threshold (e.g., 60%)
- **Severity**: High
- **Source Agent**: Agent 10

- **Gap ID**: GAP-TEST-005
- **Location**: All crates
- **OpenCode**: `packages/core/test/fixture/` provides `tmpdir.ts`, `git.ts`, `location.ts`, `recordings/`
- **RustCode**: `tempfile` crate used ad-hoc; no shared test fixtures, no database test setup helpers
- **Description**: Each test module reimplements setup boilerplate; no consistent pattern for database test setup
- **Consequence**: Test maintainability decreases as codebase grows; adding new module requires reinventing test scaffolding
- **Recommendation**: Create `test_support.rs` with: `TestDb::new()`, `TempDir::new()`, `TestConfigBuilder`, `MockProviderBuilder`
- **Severity**: High
- **Source Agent**: Agent 10

---

## 10. Dependency Gaps

### Critical

- **Gap ID**: GAP-DEP-001
- **Location**: `Cargo.toml` (workspace dependencies)
- **OpenCode**: 20+ `@ai-sdk/*` packages (Anthropic, OpenAI, Google, Bedrock, Azure, Groq, etc.)
- **RustCode**: 0 provider-specific crates; must implement 15+ HTTP protocol adapters from scratch
- **Description**: Complete absence of AI provider protocol implementations
- **Consequence**: Every provider requires 300-1000 lines of custom Rust code; no shared protocol logic
- **Recommendation**: Add provider protocol crates — start with Anthropic Messages + OpenAI Chat/Responses (cover ~80% of users)
- **Severity**: Critical
- **Source Agent**: Agent 11

### High

- **Gap ID**: GAP-DEP-002
- **Location**: `Cargo.toml`
- **OpenCode**: `@octokit/rest`, `@octokit/graphql`
- **RustCode**: `git2` not listed as dependency
- **Description**: Git library dependency missing; OpenCode uses GitHub API for many operations
- **Consequence**: Cannot perform GitHub operations, PR creation, or repository management
- **Recommendation**: Add `git2` crate for Git operations
- **Severity**: High
- **Source Agent**: Agent 11

- **Gap ID**: GAP-DEP-003
- **Location**: `Cargo.toml`
- **OpenCode**: `zod` 4.x for schema validation
- **RustCode**: `schemars` generates JSON Schema but does NOT validate
- **Description**: No runtime schema validation; tool call arguments, event data, config all lack validation
- **Consequence**: Invalid data accepted silently; deserialization errors surface at unpredictable points
- **Recommendation**: Add `jsonschema` or `valico` crate for runtime JSON Schema validation
- **Severity**: High
- **Source Agent**: Agent 11

---

## 11. Maintainability Gaps

### Critical

- **Gap ID**: GAP-MAIN-001
- **Location**: `crates/rustcode-tui/src/app.rs:1-1270`, `crates/rustcode-lsp/src/lib.rs:1-1383`, `crates/rustcode-mcp/src/lib.rs:1-1443`, `crates/rustcode-server/src/lib.rs:1-33`
- **OpenCode**: All 5 packages fully implemented
- **RustCode**: LSP, MCP, server, TUI crates labeled "stub" — ~70% of runtime logic missing or simplified
- **Description**: Stubs compile but don't function; false sense of progress
- **Consequence**: Integration points between crates never exercised; TUI cannot stream LLM output; server routes return placeholder data
- **Recommendation**: Remove stub modules from workspace or mark with feature gate; add integration tests that verify real execution paths
- **Severity**: Critical
- **Source Agent**: Agent 12

- **Gap ID**: GAP-MAIN-002
- **Location**: `crates/rustcode-tui/src/app.rs:96-200`
- **OpenCode**: React/Ink uses component composition; each component owns its own state
- **RustCode**: ~50 fields (component states, app state, backend services, streaming state, toggle flags, overlay states, etc.)
- **Description**: God Struct — TuiApp knows about everything: rendering, streaming, permissions, plugins, themes, pinning, audio
- **Consequence**: Changes to any feature require touching TuiApp; testing is difficult
- **Recommendation**: Split into focused sub-structs: `AppCore`, `StreamingState`, `UIOptions`, `PluginHost`
- **Severity**: Critical
- **Source Agent**: Agent 12

- **Gap ID**: GAP-MAIN-003
- **Location**: `crates/rustcode-core/src/database.rs:1284-1350`
- **OpenCode**: Effect.gen `Session.update()` with `SessionPatch` (optional fields)
- **RustCode**: 19 positional parameters (17 `Option`); every setter passes 14-17 `None` values
- **Description**: Long parameter list violates clean code; adding new column requires editing every call site (10+ locations)
- **Consequence**: Argument-ordering bugs invisible to compiler; 140+ `None` arguments across all call sites
- **Recommendation**: Replace with `SessionUpdate` struct with `#[derive(Default)]`; use `..Default::default()` at call sites
- **Severity**: Critical
- **Source Agent**: Agent 12

- **Gap ID**: GAP-MAIN-004
- **Location**: 14 files over 1000 lines including `session.rs` (1481+), `event.rs` (1422+), `config.rs` (1408+), `database.rs` (4758)
- **OpenCode**: TS source split into 668 files across 5 packages; most files 50-300 lines
- **RustCode**: 14 files contain all logic; `rustcode-core` alone has 13+ files all >1000 lines
- **Description**: Monolithic files violate Single Responsibility Principle; no sub-module hierarchy
- **Consequence**: Merge conflicts on large files; cognitive load; hard to navigate
- **Recommendation**: Split each >1000-line file into module directories: `session/` → `types.rs`, `manager.rs`, `messages.rs`, `prompt.rs`
- **Severity**: Critical
- **Source Agent**: Agent 12

- **Gap ID**: GAP-MAIN-005
- **Location**: All crates — <2% test coverage
- **OpenCode**: Effect TestLayer, 1000s of tests
- **RustCode**: ~54 trivial unit tests; no integration tests; no mocking infrastructure
- **Description**: 9+ modules with 0 tests; existing tests are trivial (e.g., `42 == 42`)
- **Consequence**: Cannot refactor with confidence; regressions go undetected; no regression suite for LLM provider behavior or session management
- **Recommendation**: Add mock implementations; write integration tests for each crate's public API; replace trivial tests with meaningful assertions
- **Severity**: Critical
- **Source Agent**: Agent 12

### High

- **Gap ID**: GAP-MAIN-006
- **Location**: `crates/rustcode-core/src/lib.rs:2`, `src/main.rs:2`
- **OpenCode**: TypeScript enforces `strict: true` + `noUnusedLocals` + `noUnusedParameters`
- **RustCode**: `#![allow(dead_code, unused_imports, unused_variables)]` on both core library and binary crate
- **Description**: Compiler's strongest quality signals deliberately suppressed
- **Consequence**: Dead code, unused imports, orphaned types accumulate silently; ~15-25 dead items across codebase
- **Recommendation**: Remove `allow` attributes; use `#[expect(dead_code)]` on individual items if needed
- **Severity**: High
- **Source Agent**: Agent 12

- **Gap ID**: GAP-MAIN-007
- **Location**: `crates/rustcode-tui/src/app.rs:204-318` vs `:325-418`
- **OpenCode**: React handles this via props/context
- **RustCode**: `TuiApp::new()` and `TuiApp::new_remote()` are 80% identical — ~100 lines duplicated
- **Description**: Both constructors initialize 30+ fields, set up terminal, create plugin managers, initialize same state objects
- **Consequence**: Adding new field requires editing both constructors; bug in one constructor's default absent in other
- **Recommendation**: Extract common initialization into helper functions or builder pattern
- **Severity**: High
- **Source Agent**: Agent 12

- **Gap ID**: GAP-MAIN-008
- **Location**: `crates/rustcode-tui/src/app.rs:870-1154`
- **OpenCode**: React setState pattern with reducers
- **RustCode**: Single 285-line match on `LlmEvent` with 13 arms, each containing nested match/if-let chains
- **Description**: McCabe complexity ~25
- **Consequence**: Hard to test, review, and maintain
- **Recommendation**: Split into `fn on_text_delta()`, `fn on_tool_call()`, `fn on_finish()`, etc.
- **Severity**: High
- **Source Agent**: Agent 12

- **Gap ID**: GAP-MAIN-009
- **Location**: `crates/rustcode-core/src/session.rs:1106-1262`
- **OpenCode**: Effect.gen `Session.update()` with `SessionPatch`
- **RustCode**: 10+ individual setter methods each calling `self.db.update_session(...)` with 19 params and 17 `None`s
- **Description**: Primitive obsession — all setters funnel through the same anti-pattern function
- **Consequence**: Same as MAIN-003 — fragile, error-prone, impossible to extend
- **Recommendation**: Each setter should build a `SessionPatch` with only the changed field
- **Severity**: High
- **Source Agent**: Agent 12

- **Gap ID**: GAP-MAIN-010
- **Location**: All server route handlers
- **OpenCode**: Express.js middleware with centralized error handler
- **RustCode**: 25+ handlers each repeat 6-10 lines of identical error wrapping (`match result { Ok => Json, Err => 500 }`)
- **Description**: ~200 lines of boilerplate; changing error format requires editing every handler
- **Consequence**: High maintenance burden; serialization errors silently swallowed by `unwrap_or_default()`
- **Recommendation**: Extract `fn ok_or_500<T: Serialize>(result: Result<T>) -> impl IntoResponse` helper
- **Severity**: Medium
- **Source Agent**: Agent 12

---

## 12. DevEx Gaps

### Critical

- **Gap ID**: GAP-DEVEX-001
- **Location**: `CLAUDE.md:8`
- **OpenCode**: Bun native runtime — near-instant startup; `bun dev` starts in <1s
- **RustCode**: CLAUDE.md Rule #1 prohibits all local compilation; CI-only model means zero local feedback loops
- **Description**: No incremental dev workflow; every code change requires full CI round-trip
- **Consequence**: Estimated 15-30min per iteration for full workspace rebuild; developer cannot validate code before pushing
- **Recommendation**: Remove CLAUDE.md Rule #1 prohibition on local `cargo check` and `cargo test`; add `cargo watch` for fast dev loops
- **Severity**: Critical
- **Source Agent**: Agent 13

- **Gap ID**: GAP-DEVEX-002
- **Location**: `rustcode/README.md` (missing)
- **OpenCode**: Comprehensive README.md with 26 locale translations
- **RustCode**: No user-facing README at all; `CLAUDE.md` serves as primary doc but targets AI agents
- **Description**: New users have zero entry point
- **Consequence**: Users cannot learn what RustCode is, how to build it, or how to contribute
- **Recommendation**: Write proper README.md: description, quick-start, build instructions, configuration
- **Severity**: Critical
- **Source Agent**: Agent 13

- **Gap ID**: GAP-DEVEX-003
- **Location**: `Cargo.toml:12-64`
- **OpenCode**: `bun install` (5-15s), `bun dev` (instant); Nix flake available
- **RustCode**: First build: 87 workspace dependencies, 15-30min; CLAUDE.md Rule #1 prohibits local build
- **Description**: No prebuilt binaries; no `rust-toolchain.toml`; no local iteration capability
- **Consequence**: Developer cannot build locally; only CI can compile
- **Recommendation**: Add `rust-toolchain.toml`; provide prebuilt binaries via CI artifacts; allow `cargo check` locally
- **Severity**: Critical
- **Source Agent**: Agent 13

- **Gap ID**: GAP-DEVEX-004
- **Location**: `rustcode/CONTRIBUTING.md` (missing)
- **OpenCode**: Comprehensive CONTRIBUTING.md (299 lines) with PR expectations, style preferences, debug setup
- **RustCode**: No human-facing contribution documentation; all developer guidance in CLAUDE.md (targets AI agents)
- **Description**: Complete absence of human-facing contribution documentation
- **Consequence**: Humans have no documented process for contributing; the only guide tells AI agents not to run cargo locally
- **Recommendation**: Write CONTRIBUTING.md with local setup, PR workflow, coding standards, test expectations, CI explanation
- **Severity**: Critical
- **Source Agent**: Agent 13

- **Gap ID**: GAP-DEVEX-005
- **Location**: `Cargo.toml:57` (notify dep)
- **OpenCode**: Turborepo watch mode; `bun --watch`; Vite hot reload
- **RustCode**: `notify` crate in deps but no watch/reload mechanism; CLAUDE.md prohibits local builds
- **Description**: Zero hot-reload capability; every code change requires CI run to validate
- **Consequence**: No watch/reload whatsoever; developer iteration measured in minutes per change
- **Recommendation**: Add `cargo watch` for auto-`cargo check` on file changes; remove CLAUDE.md Rule #1
- **Severity**: Critical
- **Source Agent**: Agent 13

### High

- **Gap ID**: GAP-DEVEX-006
- **Location**: `.github/workflows/ci.yml:36-37`
- **OpenCode**: Turbo cache (remote caching); sccache; parallel job splitting
- **RustCode**: `Swatinem/rust-cache@v2` only; no sccache; no parallel job splitting
- **Description**: CI rebuilds I/O-bound on cache restore; first-time CI takes 20-40min
- **Consequence**: Developer feedback loop is 15-25 min vs OpenCode's 5-10 min
- **Recommendation**: Add sccache to CI pipeline; use `cargo test --workspace --jobs 4`; consider `mold` linker
- **Severity**: High
- **Source Agent**: Agent 13

- **Gap ID**: GAP-DEVEX-007
- **Location**: `CLAUDE.md:35-36`, `.github/workflows/ci.yml:26-37`
- **OpenCode**: `oxlint` + `prettier` + TypeScript compiler; `.editorconfig`
- **RustCode**: `cargo fmt` + `cargo clippy` (CI-only); no pre-commit hooks; relaxed lints
- **Description**: Developers push unformatted code relying on CI to catch; dead code accumulates
- **Consequence**: CI minutes wasted on fmt/clippy failures; lint rules intentionally suppressed
- **Recommendation**: Add rustfmt pre-commit hook; add `.editorconfig`; remove `#![allow(dead_code, ...)]`
- **Severity**: High
- **Source Agent**: Agent 13

- **Gap ID**: GAP-DEVEX-008
- **Location**: `rustcode/` (no IDE config), `opencode/.vscode/`
- **OpenCode**: .vscode settings + launch configs; .zed/ config; debugger setup guide
- **RustCode**: No `.vscode/`, no `.zed/`, no rust-analyzer settings, no debug launch configurations
- **Description**: Zero IDE support configuration
- **Consequence**: Every developer must configure rust-analyzer from scratch; no shared editor settings
- **Recommendation**: Add `.vscode/settings.json` with rust-analyzer config; add `.vscode/launch.json` debug targets
- **Severity**: High
- **Source Agent**: Agent 13

- **Gap ID**: GAP-DEVEX-009
- **Location**: `rustcode/` (no hooks), `opencode/.husky/pre-push`
- **OpenCode**: Husky pre-push hook checking Bun version + typecheck
- **RustCode**: No pre-commit or pre-push hooks; CLAUDE.md explicitly prohibits local tooling
- **Description**: Complete absence of local git hooks; no guard against pushing broken code
- **Consequence**: Every push triggers full CI run only to discover fmt/clippy failures
- **Recommendation**: Add pre-push hook running `cargo fmt --check` and `cargo clippy -- -D warnings`
- **Severity**: High
- **Source Agent**: Agent 13

- **Gap ID**: GAP-DEVEX-010
- **Location**: `rustcode/CLAUDE.md:8`
- **OpenCode**: Full debug guide in CONTRIBUTING.md; VSCode launch configurations; `--inspect` flags
- **RustCode**: No debug documentation; no VSCode launch configs; CLI prohibits cargo build locally
- **Description**: Completely absent debug workflow
- **Consequence**: Developer cannot debug locally; bug diagnosis requires adding `eprintln!`/`tracing::debug!` and deploying to CI
- **Recommendation**: Provide VSCode launch configurations for debugging with `rust-gdb`/`lldb`; add `--verbose` flag
- **Severity**: High
- **Source Agent**: Agent 13

---

## 13. Infrastructure Gaps

### Critical

- **Gap ID**: GAP-INFRA-001
- **Location**: Whole codebase
- **OpenCode**: Honeycomb for distributed tracing; Sentry for error tracking; 6 Honeycomb Triggers with Discord alerts; log processor pipeline
- **RustCode**: Zero monitoring infrastructure; no crash reporting; no performance tracking; no telemetry
- **Description**: A crash in the field is invisible to maintainers
- **Consequence**: Critical bugs go undetected until users report them; no data on which platforms/features have issues
- **Recommendation**: Add opt-in telemetry via OpenTelemetry; add `sentry` crate for crash reporting; instrument key operations with tracing spans
- **Severity**: Critical
- **Source Agent**: Agent 14

### High

- **Gap ID**: GAP-INFRA-002
- **Location**: `infra/monitoring.ts:1-287` (OpenCode), `Cargo.toml:18-20` (RustCode tracing deps)
- **OpenCode**: Full observability stack — OTLP → Honeycomb; Sentry; structured alerting
- **RustCode**: Has tracing deps but no OTEL exporter wired; no Sentry; no metrics
- **Description**: Foundation exists (tracing crate) but no production pipeline connected
- **Consequence**: stdout/stderr logs only; no visibility into crashes or performance metrics
- **Recommendation**: Implement OpenTelemetry tracing with `opentelemetry-otlp`; add `sentry` crate for crash reporting
- **Severity**: High
- **Source Agent**: Agent 14

- **Gap ID**: GAP-INFRA-003
- **Location**: `infra/console.ts:11-43` (OpenCode), `storage.rs:658-697` (RustCode)
- **OpenCode**: PlanetScale automatic backups + branching + point-in-time recovery
- **RustCode**: SQLite local file with zero backup mechanism
- **Description**: No backup/disaster recovery for user data
- **Consequence**: If SQLite file is corrupted or deleted, all session history is lost permanently
- **Recommendation**: Implement periodic SQLite backups (WAL mode + VACUUM INTO); add `export`/`import` commands
- **Severity**: High
- **Source Agent**: Agent 14

- **Gap ID**: GAP-INFRA-004
- **Location**: `.github/workflows/release.yml:166-180` (RustCode signing)
- **OpenCode**: Azure Trusted Signing (Windows), Apple codesign (macOS); npm/Homebrew/AUR distribution
- **RustCode**: GPG signing only; no Windows/macOS code signing; no package manager distribution
- **Description**: No Windows Authenticode signing; no macOS codesign + notarization; no Homebrew/apt/scoop
- **Consequence**: Windows users get "unknown publisher" warnings; Gatekeeper blocks unsigned macOS binaries; narrower reach
- **Recommendation**: Add Azure Trusted Signing for Windows; Add Apple Developer ID signing; Create Homebrew tap
- **Severity**: High
- **Source Agent**: Agent 14

---

## 14. Database Gaps

### Critical

- **Gap ID**: GAP-DB-001
- **Location**: `event.rs:899-986` (publish method), `event.rs:943-948` (projectors)
- **OpenCode**: Projectors run after transaction commits (post-commit hooks)
- **RustCode**: Projectors run inside the database transaction
- **Description**: If a projector fails, the entire event write is rolled back. Running projectors inside the tx keeps transaction open longer.
- **Consequence**: Long-lived write transactions; valid events rolled back due to projector failures; potential deadlocks
- **Recommendation**: Move projectors to post-commit hooks (after `tx.commit().await`)
- **Severity**: Critical
- **Source Agent**: Agent 15

- **Gap ID**: GAP-DB-002
- **Location**: `event_projector.rs:276-331`
- **OpenCode**: Single transaction for event write + sequence update
- **RustCode**: `commit_sync_event` does NOT use a transaction — calls `db.insert_event()` then `db.upsert_event_sequence()` as separate queries
- **Description**: Missing atomicity — if insert_event succeeds but upsert_event_sequence fails, orphan event with no sequence tracking
- **Consequence**: Potential data corruption — orphan events or duplicate sequence numbers breaking event sourcing invariant
- **Recommendation**: Wrap both operations in a SQLite transaction
- **Severity**: Critical
- **Source Agent**: Agent 15

### High

- **Gap ID**: GAP-DB-003
- **Location**: `database.rs:472-807` (RustCode), `schema.gen.ts` (OpenCode)
- **OpenCode**: Drizzle ORM `sqliteTable("name", {...})` with compile-time schema verification
- **RustCode**: Duplicates 20 table schemas as `const &str` SQL literals — no compile-time verification
- **Description**: Raw SQL strings not validated against actual SQLite schema until runtime
- **Consequence**: Typo in column name passes cargo build and cargo test (if no test exercises that path); schema drift silent
- **Recommendation**: Add compile-time macro or build script comparing SQL strings against Drizzle schema; add integration test running all 35 migrations
- **Severity**: High
- **Source Agent**: Agent 15

- **Gap ID**: GAP-DB-004
- **Location**: `database.rs:1728-1744`
- **OpenCode**: Drizzle eager loading with JOINs
- **RustCode**: N+1 query problem — `get_messages_with_parts` performs 1 query for messages + N queries for parts
- **Description**: Loading all messages for a session requires 1 + N queries where N = number of messages
- **Consequence**: Session with 200 messages + 400 parts requires 201 SQL queries instead of 1
- **Recommendation**: Replace with `LEFT JOIN` query: `SELECT m.*, p.* FROM message m LEFT JOIN part p ON m.id = p.message_id WHERE m.session_id = ?1`
- **Severity**: High
- **Source Agent**: Agent 15

- **Gap ID**: GAP-DB-005
- **Location**: `database.rs:514-525` (account table)
- **OpenCode**: Account tokens stored in `text` columns (same issue)
- **RustCode**: No encryption at rest for `access_token` or `refresh_token` in SQLite
- **Description**: Anyone with filesystem access to the SQLite database file can read the user's API tokens
- **Consequence**: Stored API keys and tokens are accessible to anyone with file access
- **Recommendation**: Use OS keychain integration (macOS Keychain, Linux Secret Service) or encrypt token columns
- **Severity**: High
- **Source Agent**: Agent 15

- **Gap ID**: GAP-DB-006
- **Location**: `snapshot.rs:138`
- **OpenCode**: No global lock
- **RustCode**: `StdMutex<()>` global lock serializes ALL snapshot operations across ALL sessions/projects
- **Description**: Single global mutex for all snapshot operations (track, restore, revert, diff)
- **Consequence**: Taking snapshot for session A blocks restoring snapshot for session B
- **Recommendation**: Replace global Mutex with per-snapshot-repo locking (keyed by gitdir path)
- **Severity**: High
- **Source Agent**: Agent 15

---

## 15. Reliability Gaps

### Critical

- **Gap ID**: GAP-RELY-001
- **Location**: `crates/rustcode-core/src/database.rs:1280-1350`
- **OpenCode**: Effect's `tryPromise` + typed errors; Drizzle ORM ensures schema consistency
- **RustCode**: `sqlx::query` (unchecked) instead of `sqlx::query!` (compile-time checked)
- **Description**: Schema drift between migration and hand-written SQL causes hard panic at runtime
- **Consequence**: Adding a column without updating corresponding INSERT causes runtime crash on first INSERT
- **Recommendation**: Use `sqlx::query!` with compile-time checking, or add integration tests verifying all SQL against actual schema
- **Severity**: Critical
- **Source Agent**: Agent 16

- **Gap ID**: GAP-RELY-002
- **Location**: `crates/rustcode-core/src/flock.rs:222-227`
- **OpenCode**: Effect's structured concurrency + Fiber.join guarantees
- **RustCode**: TOCTOU race — checks `is_stale()`, creates `.breaker`, then re-checks staleness
- **Description**: If original lock holder renews heartbeat between two staleness checks, breaker can incorrectly delete live lock
- **Consequence**: Two processes could simultaneously believe they hold the same lock → concurrent session writes → data corruption
- **Recommendation**: Use breaker directory itself as authoritative lock; acquire breaker with mkdir atomicity
- **Severity**: Critical
- **Source Agent**: Agent 16

- **Gap ID**: GAP-RELY-003
- **Location**: `error.rs:456-458` (is_retryable defined but never called)
- **OpenCode**: Effect's built-in retry with `Schedule` (exponential backoff, jitter, max retries)
- **RustCode**: `LlmErrorReason::is_retryable()` defined but **never called**; no automatic retry implementation
- **Description**: Transient provider errors (rate limits, 503s) immediately fail the turn instead of being retried
- **Consequence**: Every transient provider error terminates current turn unnecessarily; dead `is_retryable()` code
- **Recommendation**: Wire `is_retryable()` into turn execution flow; add exponential backoff with jitter
- **Severity**: Critical
- **Source Agent**: Agent 16

- **Gap ID**: GAP-RELY-004
- **Location**: `session_runner.rs:625-634`, `session_runner.rs:990-994`
- **OpenCode**: Effect's `Effect.timeout()` at every layer
- **RustCode**: `provider.stream()` called with no timeout; `provider.complete()` has no timeout
- **Description**: No timeout on any provider call; provider hangs block session indefinitely
- **Consequence**: Non-responsive provider causes indefinite session hang; only recovery is process restart
- **Recommendation**: Add timeout parameter to Provider trait; default 60s for streaming, 30s for completion
- **Severity**: Critical
- **Source Agent**: Agent 16

- **Gap ID**: GAP-RELY-005
- **Location**: `src/main.rs:1233-1278`
- **OpenCode**: Effect's `Fiber` handles SIGINT/SIGTERM with graceful shutdown sequence
- **RustCode**: No signal handling — Ctrl+C causes immediate, ungraceful termination
- **Description**: No SIGINT/SIGTERM handlers; process terminates immediately on Ctrl+C
- **Consequence**: Tool executions in progress abruptly terminated; non-transactional writes lost; session state not persisted
- **Recommendation**: Use `tokio::signal::ctrl_c()` and `tokio::signal::unix::Signal` for graceful shutdown
- **Severity**: Critical
- **Source Agent**: Agent 16

- **Gap ID**: GAP-RELY-006
- **Location**: `src/main.rs:1337` (dispatch_inner)
- **OpenCode**: Effect's typed error system preserves full error context
- **RustCode**: `dispatch_inner` returns raw `i32` exit codes — all error context discarded
- **Description**: Command handlers return `i32` exit codes; `CliErrorFormatter::format_error` never called
- **Consequence**: All runtime errors from command handlers silently discarded; users see non-zero exit but no error message
- **Recommendation**: Change handlers to return `Result<(), anyhow::Error>` and propagate errors through `dispatch`
- **Severity**: Critical
- **Source Agent**: Agent 16

### High

- **Gap ID**: GAP-RELY-007
- **Location**: `event.rs:855-1063`
- **OpenCode**: Transactional pipeline: validate → guard → persist → project → notify
- **RustCode**: Listener/notification failures after DB commit cannot be rolled back
- **Description**: Notifications sent outside transaction; failure after DB commit leaves system inconsistent
- **Consequence**: Missed event notifications leading to stale read models after partial failure
- **Recommendation**: Implement outbox pattern with retry for failed notifications
- **Severity**: High
- **Source Agent**: Agent 16

- **Gap ID**: GAP-RELY-008
- **Location**: `session_execution.rs:637-888`
- **OpenCode**: Effect's `Ref` with clear transition validation
- **RustCode**: No runtime state machine validation; invalid sequences rely on caller correctness
- **Description**: `run()` while already running, `interrupt()` on already-interrupted lane — all allowed with no enforcement
- **Consequence**: Race conditions in concurrent execution requests could lead to multiple drain fibers for same session
- **Recommendation**: Add structured state machine with guard checks before each transition
- **Severity**: High
- **Source Agent**: Agent 16

- **Gap ID**: GAP-RELY-009
- **Location**: `snapshot.rs:825`
- **OpenCode**: `execa` git calls with Effect's tryPromise + timeout
- **RustCode**: `std::process::Command` (blocking) inside async context; no timeout on git commands
- **Description**: Blocking git operations in async context; if git hangs, entire async task thread pool blocked
- **Consequence**: Blocking tokio runtime thread pool if git hangs; corrupt snapshot gitdir if alternates file points to stale directory
- **Recommendation**: Use `tokio::process::Command` for all git operations; add configurable timeouts
- **Severity**: High
- **Source Agent**: Agent 16

- **Gap ID**: GAP-RELY-010
- **Location**: `session_runner.rs:639-679`
- **OpenCode**: Effect's `Stream` with built-in error recovery
- **RustCode**: Control flow encoded as string inside `Error::Internal` — fragile string-parsing approach
- **Description**: Overflow recovery uses string-encoded TurnControl within Error::Internal
- **Consequence**: If encoding changes, overflow recovery silently stops working; context overflow crashes become unrecoverable
- **Recommendation**: Use dedicated enum for control flow; add exhaustive test coverage for all turn control paths
- **Severity**: High
- **Source Agent**: Agent 16

- **Gap ID**: GAP-RELY-011
- **Location**: `session_revert.rs:219-267`
- **OpenCode**: Drizzle transaction for message removal
- **RustCode**: `DELETE FROM session_message` queries without wrapping transaction
- **Description**: Non-transactional cascade delete in session cleanup
- **Consequence**: Partial cleanup on crash → corrupted session with inconsistent message ordering and references to deleted parts
- **Recommendation**: Wrap all revert cleanup operations in SQLite transaction
- **Severity**: High
- **Source Agent**: Agent 16

- **Gap ID**: GAP-RELY-012
- **Location**: `session_runner.rs:960-1155`
- **OpenCode**: Persists each tool call result and LLM event incrementally
- **RustCode**: No incremental persistence during tool loop; only final SessionRunResult returned
- **Description**: Intermediate events and tool results not persisted during execution
- **Consequence**: Crash during long-running tool sequence loses all work; user must restart from scratch
- **Recommendation**: Persist each tool call result incrementally; use event sourcing for LLM events as they arrive
- **Severity**: High
- **Source Agent**: Agent 16

- **Gap ID**: GAP-RELY-013
- **Location**: No circuit breaker implementation
- **OpenCode**: Effect's `CircuitBreaker` for provider calls
- **RustCode**: No circuit breaker anywhere in codebase
- **Description**: After N consecutive failures, circuit should open and subsequent calls fail fast
- **Consequence**: Provider outages cause excessive retry storms and slow session degradation instead of fast failure
- **Recommendation**: Implement circuit breaker for provider calls with configurable thresholds
- **Severity**: High
- **Source Agent**: Agent 16

- **Gap ID**: GAP-RELY-014
- **Location**: `provider.rs` (fallback not implemented)
- **OpenCode**: Fallback chains — if primary provider fails, try next configured provider
- **RustCode**: No provider fallback mechanism; single provider = single point of failure
- **Description**: SessionRunner initialized with specific Arc<dyn Provider> and Model
- **Consequence**: If configured provider is down, session cannot proceed even if alternatives configured
- **Recommendation**: Implement provider fallback chain
- **Severity**: High
- **Source Agent**: Agent 16

- **Gap ID**: GAP-RELY-015
- **Location**: `session_runner.rs:740`
- **OpenCode**: Concurrent tool execution via `Promise.allSettled()`
- **RustCode**: Sequential tool execution in `for` loop — early exit on first failure
- **Description**: If third of five tool calls fails, remaining tools never executed
- **Consequence**: Single failing tool call cancels all subsequent tool calls, losing potentially successful work
- **Recommendation**: Execute independent tool calls concurrently using `FuturesUnordered` or `join_all`
- **Severity**: High
- **Source Agent**: Agent 16

- **Gap ID**: GAP-RELY-016
- **Location**: `session_execution.rs:786`
- **OpenCode**: RunCoordinator tracks all in-flight drains with global shutdown
- **RustCode**: No mechanism to interrupt all active sessions on shutdown
- **Description**: `RunCoordinator::interrupt()` exists per-lane but no global `shutdown()` method
- **Consequence**: On process termination, active drains for other sessions continue running, potentially executing tool commands
- **Recommendation**: Add `shutdown()` method to RunCoordinator that interrupts all active lanes
- **Severity**: High
- **Source Agent**: Agent 16

- **Gap ID**: GAP-RELY-017
- **Location**: `database.rs:59-66, 338-351`
- **OpenCode**: Same PRAGMAs + transactional writes everywhere
- **RustCode**: PRAGMAs match but some code paths perform SQL writes without transactions
- **Description**: Non-transactional writes (revert cleanup, individual message updates) leave database inconsistent after crash
- **Consequence**: Data inconsistency risk on crash for non-transactional write paths
- **Recommendation**: Audit all write paths to ensure they are transactional
- **Severity**: High
- **Source Agent**: Agent 16

- **Gap ID**: GAP-RELY-018
- **Location**: `storage.rs:454`
- **OpenCode**: `fs.writeFileSync` guarantees flush to disk
- **RustCode**: `std::fs::write()` does NOT guarantee data is flushed to disk
- **Description**: JSON storage writes not fsynced; crash after write() returns but before data reaches disk loses data
- **Consequence**: Data loss on crash for JSON file storage
- **Recommendation**: Use `File::create()` + `write_all()` + `sync_all()` for all JSON storage writes
- **Severity**: High
- **Source Agent**: Agent 16

- **Gap ID**: GAP-RELY-019
- **Location**: `snapshot.rs:138`
- **OpenCode**: Single-threaded, no blocking mutex concerns
- **RustCode**: `std::sync::Mutex<()>` in SnapshotService used inside async code
- **Description**: Blocking mutex held while awaiting futures (e.g., `snapshot_git`)
- **Consequence**: If snapshot operation holds std::sync::Mutex while awaiting git, all other async tasks on same thread blocked
- **Recommendation**: Replace `std::sync::Mutex` with `tokio::sync::Mutex` in all async code paths
- **Severity**: High
- **Source Agent**: Agent 16

---

## 16. Competitive Intelligence Gaps

### Critical

- **Gap ID**: GAP-COMP-001
- **Location**: Whole architecture
- **OpenCode**: Effect v4 — structured concurrency, algebraic effects, type-safe DI, `Effect.gen`, `Context.Service`, `Layer`, `Scope`, `Stream`
- **RustCode**: Raw `tokio::spawn` + `thiserror` + struct-field DI; no structured concurrency
- **Description**: Effect is the entire program composition model — not just an error library. `Layer` DI makes every service testable. `Scope` auto-cleans resources. `Stream` handles backpressure natively.
- **Consequence**: RustCode will accumulate resource leaks, untestable service graphs, ad-hoc error handling
- **Recommendation**: Adopt structured concurrency framework — port V2 session algebraic design; use `CancellationToken` + `JoinSet` for Scope-like lifecycle
- **Severity**: Critical
- **Source Agent**: Agent 17

- **Gap ID**: GAP-COMP-002
- **Location**: `rustcode-core/src/provider.rs` (Provider trait), `opencode/packages/llm/src/route/`
- **OpenCode**: 4-axis route composition (Protocol, Endpoint, Auth, Framing) — 14 OpenAI-compatible providers share 1 protocol implementation
- **RustCode**: Simple `Provider` trait; non-OpenAI protocols are separate implementations with zero shared protocol logic
- **Description**: Each new protocol requires 300-400 lines of boilerplate; bug fixes must be replicated across providers
- **Consequence**: Adding WebSocket transport requires reimplementing every provider; protocol bug fixes must be applied N times
- **Recommendation**: Implement Rust equivalent of route architecture with composable `Protocol`, `Endpoint`, `Auth`, `Framing` traits
- **Severity**: Critical
- **Source Agent**: Agent 17

### High

- **Gap ID**: GAP-COMP-003
- **Location**: `rustcode-core/src/session.rs`, `opencode/packages/core/src/session/`
- **OpenCode**: Algebraic prompt lifecycle with durable inbox, promotion, retry reconciliation
- **RustCode**: `Session::prompt(...)` appends to in-memory message list; no durable inbox; no promotion lifecycle
- **Description**: OpenCode's prompt admission is durable across process restarts; RustCode is in-memory
- **Consequence**: Sessions lost on crash; no retry deduplication; no queue-based delivery
- **Recommendation**: Implement durable session_input table, promotion lifecycle, delivery-mode routing
- **Severity**: High
- **Source Agent**: Agent 17

- **Gap ID**: GAP-COMP-004
- **Location**: `rustcode-core/src/event.rs`, `opencode/packages/core/src/event/`
- **OpenCode**: EventV2 — durable event replay with aggregate sequence cursor
- **RustCode**: `tokio::sync::broadcast` — in-memory event bus; no persistence; no replay; no cursor
- **Description**: Event system cannot survive process restart
- **Consequence**: Cannot rebuild session state from events; session recovery requires snapshot-only approach
- **Recommendation**: Port event sourcing with SQLite-backed event store + aggregate sequence cursor
- **Severity**: High
- **Source Agent**: Agent 17

- **Gap ID**: GAP-COMP-005
- **Location**: `rustcode-core/src/system_context.rs`, `opencode/CONTEXT.md:1-129`
- **OpenCode**: System Context Registry with scoped Context Source producers; epoch-level snapshots; chronological mid-conversation system messages
- **RustCode**: No concept of context epochs; system prompt is static string; no mid-conversation context updates
- **Description**: OpenCode dynamically adds/removes context sources across conversation turns
- **Consequence**: RustCode agents cannot adapt to changing context; users must restart sessions
- **Recommendation**: Implement `ContextSource` trait; implement `SystemContextRegistry` with epoch management
- **Severity**: High
- **Source Agent**: Agent 17

---

## 17. Refactoring Gaps

### Critical

- **Gap ID**: GAP-REFACTOR-001 (QW-1)
- **Location**: `rustcode-core/src/lib.rs:2`, `src/main.rs:2`
- **Description**: `#![allow(dead_code, unused_imports, unused_variables)]` — crate-wide suppression
- **Consequence**: 15-25 dead items silently accumulate; cannot detect real dead code
- **Recommendation**: Remove allow attributes; use individual `#[expect(dead_code)]`; add `#[cfg(scaffold)]` gate
- **Severity**: Critical
- **Source Agent**: Agent 18

- **Gap ID**: GAP-REFACTOR-002 (QW-2)
- **Location**: `session.rs:1208`
- **Description**: `clear_revert` writes literal `"null"` string instead of SQL NULL
- **Consequence**: Data corruption — `WHERE revert IS NULL` queries miss this row
- **Recommendation**: Change `Some("null")` to `None`
- **Severity**: Critical
- **Source Agent**: Agent 18

- **Gap ID**: GAP-REFACTOR-003 (QW-3)
- **Location**: `database.rs:1284-1350`
- **Description**: 19 positional `Option` parameters; call sites pass 14-17 `None` values
- **Consequence**: Argument-ordering bugs invisible to compiler; adding new column requires editing all call sites
- **Recommendation**: Replace with `SessionUpdate` struct with `#[derive(Default)]`
- **Severity**: Critical
- **Source Agent**: Agent 18

- **Gap ID**: GAP-REFACTOR-004 (QW-4)
- **Location**: `session_runner.rs:1086-1096`
- **Description**: V1 run_loop bypasses all permission checks — `ask_fn: None`, `permission_source: None`, calls `execute_by_name`
- **Consequence**: LLM can call bash, read, write, edit without any allow/deny/ask check
- **Recommendation**: Wire ask_fn; switch V1 from execute_by_name to execute_with_pipeline
- **Severity**: Critical
- **Source Agent**: Agent 18

- **Gap ID**: GAP-REFACTOR-005 (QW-5)
- **Location**: `session_runner.rs:703-717`
- **Description**: `compact_result.as_ref().unwrap()` + `Some()` wrappers in JSON via redundant `.map()` inside `json!()`
- **Consequence**: Epoch snapshot storage contains literal `Some(...)` wrappers in JSON; corrupt epoch data
- **Recommendation**: Use `if let Some(ref result)` pattern; access fields directly
- **Severity**: Critical
- **Source Agent**: Agent 18

- **Gap ID**: GAP-REFACTOR-006 (QW-8)
- **Location**: `session.rs:1420`
- **Description**: `cost: f64` — prevents deriving `Eq` on `SessionInfo`; floating-point precision loss
- **Consequence**: Cannot derive Eq; monetary values accumulate rounding errors
- **Recommendation**: Use `ordered_float::OrderedFloat<f64>` or store cost as `i64` (millicents)
- **Severity**: Critical
- **Source Agent**: Agent 18

- **Gap ID**: GAP-REFACTOR-007 (QW-9)
- **Location**: All codebase
- **Description**: Five fragmented error types with no `From` impls between them
- **Consequence**: Error conversion requires manual `.map_err()`; type information lost
- **Recommendation**: Add `#[from] SessionError`, `#[from] DatabaseServiceError`, `#[from] LspError` to `crate::error::Error`
- **Severity**: Critical
- **Source Agent**: Agent 18

### High

- **Gap ID**: GAP-REFACTOR-008 (QW-6)
- **Location**: `tool.rs:47`
- **Description**: `ToolContext.messages` is `Vec<ChatMessage>` — full deep clone per tool call
- **Consequence**: 2.5MB of cloned message data per 25-tool-call session
- **Recommendation**: Use `Arc<Vec<ChatMessage>>`
- **Severity**: High
- **Source Agent**: Agent 18

- **Gap ID**: GAP-REFACTOR-009 (QW-10)
- **Location**: All server route handlers
- **Description**: 25 route handlers each repeat identical error wrapping pattern
- **Consequence**: ~200 lines of boilerplate; serialization errors silently swallowed by `unwrap_or_default()`
- **Recommendation**: Extract `fn ok_or_500<T: Serialize>(result: Result<T>) -> impl IntoResponse`
- **Severity**: High
- **Source Agent**: Agent 18

- **Gap ID**: GAP-REFACTOR-010 (MR-1)
- **Location**: `rustcode-core/src/lib.rs:11-95`
- **Description**: All 95 modules are `pub mod` — no visibility discipline
- **Consequence**: Internal implementation details are part of public API; refactoring internals requires checking all consumers
- **Recommendation**: Audit all 95 modules; change internal modules to `pub(crate) mod`; define explicit `pub use` re-exports
- **Severity**: Critical
- **Source Agent**: Agent 18

- **Gap ID**: GAP-REFACTOR-011 (MR-2)
- **Location**: 14 files >1000 lines
- **Description**: No sub-module hierarchy — `session.rs` (1481+ lines), `event.rs` (1422+), `config.rs` (1408+), `tool_impls.rs` (1238+)
- **Consequence**: Hard to navigate, test, and review; merge conflicts on every change
- **Recommendation**: Split into directory-based modules with <300 lines each
- **Severity**: Critical
- **Source Agent**: Agent 18

- **Gap ID**: GAP-REFACTOR-012 (MR-3)
- **Location**: `database.rs` (4758 lines)
- **Description**: No database trait — core depends directly on sqlx
- **Consequence**: Cannot swap SQLite for PostgreSQL; cannot unit test with in-memory database
- **Recommendation**: Define `Database` trait in core; implement `SqliteDatabase` in adapter crate
- **Severity**: Critical
- **Source Agent**: Agent 18

- **Gap ID**: GAP-REFACTOR-013 (MR-7)
- **Location**: `session.rs:84-90`, `provider.rs:24-48`
- **Description**: Type aliases instead of newtypes for all domain IDs
- **Consequence**: No compile-time safety — SessionId and ModelId interchangeable with String
- **Recommendation**: Convert each ID alias to newtype with validation
- **Severity**: High
- **Source Agent**: Agent 18

---

## 18. Technical Debt Gaps

### Critical

- **Gap ID**: GAP-DEBT-001 (CRIT-1)
- **Location**: Throughout all crates — `tool_impls.rs`, `flock.rs`, `event_projector.rs`, `lsp.rs`, `integration.rs`, `ripgrep.rs`, `agent.rs`, `plugin.rs`, `npm.rs`, `account.rs`
- **Description**: 100+ `unwrap()` calls in library (non-test) code — directly violates CLAUDE.md Rule #3
- **Consequence**: Any unexpected Err/None crashes the process; in AI agent, this means mid-session data loss
- **Recommendation**: Replace every `unwrap()` in non-test code with `?`, `.context()`, or `.expect("reason")`
- **Estimated Fix Cost**: 80-120 person-hours
- **Severity**: Critical
- **Source Agent**: Agent 19

- **Gap ID**: GAP-DEBT-002 (CRIT-2)
- **Location**: `lib.rs:2`, `main.rs:2`, `app.rs:120`
- **Description**: `#![allow(dead_code, unused_imports, unused_variables)]` hides 50+ dead items
- **Consequence**: Dead code rots silently; CI passes despite broken references; prevents Clippy from catching real bugs
- **Recommendation**: Remove allow attributes; tag individual items with `#[expect(dead_code)]`
- **Estimated Fix Cost**: 20-30 person-hours
- **Severity**: Critical
- **Source Agent**: Agent 19

- **Gap ID**: GAP-DEBT-003 (CRIT-3)
- **Location**: `agent.rs:1293,1304` (+ 10+ other locations)
- **Description**: `Error::NotImplemented` used as production stub return for core functionality
- **Consequence**: Users hit "not implemented" errors during normal operation; core agent functionality incomplete
- **Recommendation**: Replace each NotImplemented with real implementation or feature-gated `todo!()` with tracking issue
- **Estimated Fix Cost**: 30-50 person-hours
- **Severity**: Critical
- **Source Agent**: Agent 19

- **Gap ID**: GAP-DEBT-004 (CRIT-4)
- **Location**: Every production module in `rustcode-core/src/`
- **Description**: 500+ `.unwrap()` + `.expect()` calls in non-test code — CLAUDE.md Rule #3 systematically violated
- **Consequence**: Any unwrap on None/Err crashes the process; users lose work mid-session
- **Recommendation**: Enforce via `clippy::unwrap_used` lint (deny in CI)
- **Estimated Fix Cost**: 40-60 person-hours
- **Severity**: Critical
- **Source Agent**: Agent 19

### High

- **Gap ID**: GAP-DEBT-005 (HIGH-1)
- **Location**: `database.rs:1233,1284,1657,1749,1781`, `plugin.rs:2413`, `session_runner.rs:577`, `providers/anthropic.rs:1252`
- **Description**: 15+ methods with `#[allow(clippy::too_many_arguments)]` — 10-19 parameters each
- **Consequence**: Callers unreadable; adding a field changes every call site
- **Recommendation**: Replace long parameter lists with typed input structs
- **Estimated Fix Cost**: 20-30 person-hours
- **Severity**: High
- **Source Agent**: Agent 19

- **Gap ID**: GAP-DEBT-006 (HIGH-2)
- **Location**: `provider.rs:906`
- **Description**: Provider trait defined but no real implementations (Anthropic, OpenAI, etc.)
- **Consequence**: Agent cannot call any LLM; CLI run commands fail
- **Recommendation**: Implement Anthropic and OpenAI providers first (cover ~80% of users)
- **Estimated Fix Cost**: 80-120 person-hours
- **Severity**: High
- **Source Agent**: Agent 19

- **Gap ID**: GAP-DEBT-007 (HIGH-3)
- **Location**: `session_compaction.rs`
- **Description**: Session compaction (context window management) is incomplete — actual logic missing or stubbed
- **Consequence**: Long sessions overflow context windows; no multi-turn agent loops
- **Recommendation**: Implement SessionCompactionService with tail-turns preservation, summary generation
- **Estimated Fix Cost**: 40-60 person-hours
- **Severity**: High
- **Source Agent**: Agent 19

- **Gap ID**: GAP-DEBT-008 (HIGH-4)
- **Location**: `config.rs:1166,1322`
- **Description**: `self.info.read().expect("Config lock poisoned")` — panics on lock poison
- **Consequence**: Panic in one thread poisons the lock for all threads; subsequent reads crash the process
- **Recommendation**: Use `lock().map_err(|_| Error::Internal(...))?`
- **Estimated Fix Cost**: 2-4 person-hours
- **Severity**: High
- **Source Agent**: Agent 19

- **Gap ID**: GAP-DEBT-009 (HIGH-5)
- **Location**: `storage.rs`, `database.rs`
- **Description**: SQLite pool creation deferred; `DatabaseService` still uses JSON file-based storage as primary
- **Consequence**: Session persistence, events all use JSON file storage or in-memory; data lost on restart
- **Recommendation**: Wire up `sqlx::SqlitePool` in runtime.rs init; run migrations on startup
- **Estimated Fix Cost**: 30-50 person-hours
- **Severity**: High
- **Source Agent**: Agent 19

---

## 19. Production Readiness Gaps

### Critical

- **Gap ID**: GAP-PROD-001
- **Location**: Session runner, provider modules, tool implementations
- **Description**: Core business logic — session runner, LLM provider integration, tool execution, TUI, LSP, MCP — exists only as type stubs
- **Consequence**: A production deployment has zero functional capability beyond database CRUD and config loading
- **Recommendation**: Implement minimum viable production paths: session runner, Anthropic/OpenAI providers, Bash/Read/Write/Edit tools
- **Severity**: Critical
- **Source Agent**: Agent 20

- **Gap ID**: GAP-PROD-002
- **Location**: `storage.rs:445-455`
- **Description**: Synchronous `std::fs::write()` in Storage module — blocks async runtime
- **Consequence**: Every storage write blocks tokio worker thread; in server context, blocks all connected clients
- **Recommendation**: Move to `tokio::fs` or use `spawn_blocking`
- **Severity**: Critical
- **Source Agent**: Agent 20

### High

- **Gap ID**: GAP-PROD-003
- **Location**: Session runner
- **Description**: No session crash recovery — if process dies mid-session, state is incomplete
- **Consequence**: Lost LLM response stream, inconsistent DB state; user must restart from scratch
- **Recommendation**: Implement incremental persistence during tool loop; event-sourced session state
- **Severity**: High
- **Source Agent**: Agent 20

- **Gap ID**: GAP-PROD-004
- **Location**: Provider calls
- **Description**: No circuit breaker or retry policy for transient provider failures
- **Consequence**: All transient provider errors immediately fail the turn; no graceful degradation
- **Recommendation**: Implement retry with exponential backoff; add circuit breaker for provider calls
- **Severity**: High
- **Source Agent**: Agent 20

- **Gap ID**: GAP-PROD-005
- **Location**: `rustcode-server/`
- **Description**: No TLS support — server runs HTTP only; credentials transmitted in plaintext
- **Consequence**: All server traffic unencrypted; API keys and session data visible on network
- **Recommendation**: Add `--tls-cert`/`--tls-key` CLI flags for HTTPS
- **Severity**: High
- **Source Agent**: Agent 20

- **Gap ID**: GAP-PROD-006
- **Location**: `storage.rs`
- **Description**: No backup or restore mechanism for SQLite database
- **Consequence**: SQLite corruption causes complete data loss; no recovery path
- **Recommendation**: Add `backup` and `restore` CLI subcommands using `.backup` SQLite API
- **Severity**: High
- **Source Agent**: Agent 20

---

## Consolidated Top 10 Recommendations

| Rank | Gap | Severity | Effort (person-days) | Impact |
|------|-----|----------|---------------------|--------|
| 1 | Fix V1 permission bypass (GAP-LOGIC-002 / GAP-REFACTOR-004) | Critical | 1 | Security — LLM can execute tools without permission checks |
| 2 | Fix `clear_revert` SQL NULL bug (GAP-LOGIC-001 / GAP-REFACTOR-002) | Critical | 0.1 | Data corruption — literal string "null" written to DB |
| 3 | Unify error hierarchy (GAP-RUST-001 / GAP-REFACTOR-007) | Critical | 1 | Error type info lost; cannot match on specific errors |
| 4 | Replace 19-param `update_session` with typed struct (GAP-MAIN-003 / GAP-REFACTOR-003) | Critical | 0.5 | Eliminates 140+ None arguments across 10 call sites |
| 5 | Remove dead code allow + restore compiler detection (GAP-MAIN-006 / GAP-REFACTOR-001) | Critical | 0.5 | Prevents accumulation of dead code; enables compiler to find bugs |
| 6 | Add provider retry with existing `is_retryable()` (GAP-RELY-003) | Critical | 2 | Transient provider errors always fail without retry |
| 7 | Add timeout to all provider calls (GAP-RELY-004) | Critical | 1 | Provider hangs block sessions indefinitely |
| 8 | Implement signal handling with graceful shutdown (GAP-RELY-005) | Critical | 2 | Ctrl+C causes immediate data loss |
| 9 | Use `Arc<Vec<ChatMessage>>` instead of clone in ToolContext (GAP-PERF-004 / GAP-REFACTOR-008) | High | 0.5 | Eliminates 2.5MB per-session clone overhead |
| 10 | Fix event projector atomicity — wrap in transaction (GAP-DB-001 / GAP-DB-002) | Critical | 2 | Orphan events, sequence corruption, data loss |
