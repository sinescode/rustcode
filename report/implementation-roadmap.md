# Implementation Roadmap — BlazeCode Transformation Program

**Generated:** 2026-06-21  
**Sources:** Architecture (Agent 02), Feature Gap (Agent 08), Competitive Intelligence (Agent 17), Refactoring (Agent 18), Technical Debt (Agent 19)  
**Program Duration:** 72 weeks (~18 months)  
**Total Estimated Effort:** 120–150 person-weeks  

---

## Phase 0 — Critical Fixes (Weeks 1–2)

### Duration
2 weeks

### Objectives
- Fix data-corrupting bugs that corrupt persistent state
- Close security vulnerabilities that bypass permission enforcement
- Eliminate crash vectors from `unwrap()` and `panic!()` in production paths
- Enable compiler dead-code detection to prevent further quality erosion

### Detailed Tasks

| # | Task | Owner/Role | Effort |
|---|------|------------|--------|
| 1 | Fix `clear_revert` SQL NULL corruption — change `Some("null")` to `None` | Core Team | 0.1 pd |
| 2 | Fix V1 permission bypass in `run_loop` — wire `ask_fn` and switch to `execute_with_pipeline` | Security Team | 1 pd |
| 3 | Fix epoch snapshot JSON corruption — replace `compact_result.as_ref().unwrap()` + `Some()` wrappers in `serde_json::json!()` | Core Team | 0.5 pd |
| 4 | Fix TOCTOU race in `wake()` — add atomic state guard | Core Team | 1 pd |
| 5 | Add encryption at rest for credentials — use `age` or `sodiumoxide` for credential store | Security Team | 3 pd |
| 6 | Add `{file:}` path traversal protection — validate resolved path is within allowed roots | Security Team | 2 pd |
| 7 | Add MCP subprocess sandboxing — resource limits, capability dropping, timeout enforcement | Platform Team | 5 pd |
| 8 | Remove `#![allow(dead_code, unused_imports, unused_variables)]` — fix all resulting warnings with `#[expect]` or `#[cfg(scaffold)]` gates | Core Team | 0.5 pd |
| 9 | Add proper timeouts to all LLM provider HTTP calls — default 120s with configurable override | Core Team | 1 pd |

### Dependencies
None — all tasks are self-contained within existing codebase.

### Risks and Mitigations
| Risk | Likelihood | Impact | Mitigation |
|------|-----------|--------|------------|
| Removing `#[allow(dead_code)]` exposes 50+ dead items, causing CI failure | High | Medium | Fix in batches; gate scaffold items with `#[cfg(scaffold)]` |
| Permission fix changes V1 behavior subtly | Medium | High | Add integration test for V1 tool call with permission check |
| Encryption adds key-management complexity | Low | Medium | Require only a file-based key at launch; document clearly |

### Success Criteria
- [ ] `clear_revert` writes SQL `NULL`, verified by unit test
- [ ] V1 tool calls are blocked when permission is denied, verified by integration test
- [ ] JSON snapshots contain no `Some(...)` wrappers
- [ ] `wake()` has no concurrent scheduling races
- [ ] Credential file encrypted at rest; plaintext never written to disk
- [ ] `{file:}` paths are rejected if they escape allowed workspace root
- [ ] MCP subprocesses are sandboxed with timeout, killed on timeout
- [ ] `cargo build` succeeds without `#![allow(dead_code)]`
- [ ] All provider HTTP calls have 120s timeout

### Estimated Person-Weeks
**3 person-weeks** (14 person-days total, parallelizable across 2–3 engineers)

### Key Deliverables
- `CHANGELOG-CRITICAL.md` documenting each fix
- CI pipeline with dead-code detection enforced
- Permission system integration tests

### Task Table

| Task | Effort (pw) | Dependencies | Risk | Priority |
|------|------------|-------------|------|----------|
| Fix `clear_revert` SQL NULL corruption | 0.02 | None | Low | Critical |
| Fix V1 permission bypass | 0.2 | QW-3 (SessionUpdate refactor recommended) | Medium | Critical |
| Fix epoch snapshot JSON corruption | 0.1 | None | Low | Critical |
| Fix TOCTOU race in `wake()` | 0.2 | None | Low | Critical |
| Add credential encryption at rest | 0.6 | None | Medium | Critical |
| Add path traversal protection | 0.4 | None | Low | Critical |
| Add MCP subprocess sandboxing | 1.0 | None | High | Critical |
| Remove `#[allow(dead_code)]` | 0.1 | None | Medium | Critical |
| Add provider call timeouts | 0.2 | None | Low | High |

---

## Phase 1 — Foundational Refactoring (Weeks 3–6)

### Duration
4 weeks

### Objectives
- Establish shared infrastructure crates with zero implementation dependencies
- Implement structured concurrency to prevent resource leaks
- Add Rust-specific quality infrastructure (semver checks, coverage)
- Replace stringly-typed IDs with compiler-enforced newtypes

### Detailed Tasks

| # | Task | Owner/Role | Effort |
|---|------|------------|--------|
| 1 | Extract `blazecode-types` crate — IDs (`SessionId`, `MessageId`, `ModelId`, `ProviderId`), core enums (`TurnControl`, `AgentMode`), value objects (`ChatMessage`, `ToolResult`) | Core Team | 5 pd |
| 2 | Extract `blazecode-error` crate — unified `Error` enum with `#[from]` for all sub-errors; remove `Error::NotImplemented` | Core Team | 2 pd |
| 3 | Extract `blazecode-config` crate — move config parsing, validation, schema from monolithic `config.rs` | Core Team | 5 pd |
| 4 | Extract `blazecode-observability` crate — tracing subscriber setup, OTLP exporter, span lifecycle, metric collection | Platform Team | 3 pd |
| 5 | Implement structured concurrency — `ScopedFiberSet<T>` with automatic cancel-and-join on drop; replace `DashMap<FiberId, JoinHandle>` with scoped fibers | Core Team | 15 pd |
| 6 | Replace `String` type aliases with newtypes — `SessionId(String)`, `MessageId(String)`, `ProviderId(String)` with `#[serde(transparent)]`, validation, `Display`, `FromStr` | Core Team | 5 pd |
| 7 | Add `cargo-semver-checks` to CI — enforce semver compatibility on all public API changes | Platform Team | 1 pd |
| 8 | Add coverage reporting — `cargo-tarpaulin` or `cargo-llvm-cov` + Codecov upload in CI | Platform Team | 1 pd |

### Dependencies
- Phase 0 must be complete (dead-code detection needed to audit public API surface for newtypes)
- `blazecode-types` must be extracted before ID newtypes can be implemented

### Risks and Mitigations
| Risk | Likelihood | Impact | Mitigation |
|------|-----------|--------|------------|
| Newtype ID migration touches hundreds of call sites | High | Medium | Do module-by-module with temporary `From<String>` conversions; verify with `cargo check` after each module |
| Structured concurrency changes async control flow significantly | Medium | High | Implement scoped fibers in a new module; migrate callers incrementally |
| Crate extraction breaks downstream consumers temporarily | High | Medium | Use workspace `path` dependencies; keep old re-exports during transition with deprecation notices |

### Success Criteria
- [ ] `blazecode-types`, `blazecode-error`, `blazecode-config`, `blazecode-observability` publishable as standalone crates
- [ ] No crate-wide `#[allow(dead_code)]` anywhere in workspace
- [ ] `ScopedFiberSet` implemented with unit tests proving cancel-and-join on drop
- [ ] All `String` ID type aliases replaced with newtypes; compilation fails on type confusion
- [ ] CI includes `cargo-semver-checks` and coverage upload steps
- [ ] Coverage baseline >5% (from current <2%)

### Estimated Person-Weeks
**8 person-weeks** (37 person-days with some parallellism possible)

### Key Deliverables
- `crates/blazecode-types/`, `crates/blazecode-error/`, `crates/blazecode-config/`, `crates/blazecode-observability/`
- `ScopedFiberSet` with unit tests
- ID newtypes across entire codebase
- CI workflow with coverage + semver checks

### Task Table

| Task | Effort (pw) | Dependencies | Risk | Priority |
|------|------------|-------------|------|----------|
| Extract `blazecode-types` crate | 1.0 | Phase 0 | Medium | Critical |
| Extract `blazecode-error` crate | 0.4 | blazecode-types | Medium | Critical |
| Extract `blazecode-config` crate | 1.0 | blazecode-types, blazecode-error | Medium | High |
| Extract `blazecode-observability` crate | 0.6 | blazecode-types | Medium | High |
| Structured concurrency (ScopedFiberSet) | 3.0 | None | High | Critical |
| Newtype ID replacement | 1.0 | blazecode-types | High | High |
| `cargo-semver-checks` CI | 0.2 | Phase 0 CI changes | Low | Medium |
| Coverage reporting CI | 0.2 | None | Low | Medium |

---

## Phase 2 — Domain Extraction (Weeks 7–14)

### Duration
8 weeks

### Objectives
- Split monolithic `blazecode-core` into domain-specific crates with clear responsibilities
- Extract infrastructure behind traits (Database, FileSystem, HTTP Client)
- Enforce module visibility discipline with `pub(crate)` throughout
- Break up all files >1,000 lines into focused sub-modules

### Detailed Tasks

| # | Task | Owner/Role | Effort |
|---|------|------------|--------|
| 1 | Extract `blazecode-session-core` — `Session`, `Message`, `SessionManager`, session lifecycle types | Session Team | 5 pd |
| 2 | Extract `blazecode-tool-core` — `Tool` trait, `ToolRegistry`, `ToolContext`, tool result types | Tool Team | 5 pd |
| 3 | Extract `blazecode-database` trait — `#[async_trait] pub trait Database` with CRUD methods; `MockDatabase` for testing; migrate all consumers to `Arc<dyn Database>` | Core Team | 10 pd |
| 4 | Extract `blazecode-filesystem` trait — `FileSystem` with `read`, `write`, `exists`, `list`, `search`; `TokioFileSystem` adapter using `tokio::fs` | Platform Team | 8 pd |
| 5 | Extract `blazecode-provider-core` — `Provider` trait (with GAT `Stream` type), `Model`, `ChatMessage`, `StreamChunk`; route-based architecture foundation | Provider Team | 5 pd |
| 6 | Extract `blazecode-plugin-core` — `Plugin`, `PluginManager` traits; `ClosureProviderPlugin` refactored into trait-based hook system | Platform Team | 3 pd |
| 7 | Implement visibility control — audit all 95 modules; mark 70+ as `pub(crate)`, define explicit `pub use` re-exports in `lib.rs` | All Teams | 5 pd |
| 8 | Split files >2,000 lines — 14 files into directory modules: `session/`, `provider/`, `tool/`, `database/`, `filesystem/`, `config/`, `permission/`, `plugin/` | All Teams | 10 pd |

### Dependencies
- Phase 1 must be complete (newtype IDs, types crate, error crate are prerequisites)
- Task 7 (visibility) enables tasks 1–6 by clarifying public API surfaces
- Task 8 (file splits) should follow task 7 to avoid rework

### Risks and Mitigations
| Risk | Likelihood | Impact | Mitigation |
|------|-----------|--------|------------|
| Splitting core breaks every downstream dependency | High | High | Extract one crate per week; maintain temporary re-exports in `blazecode-core` during transition |
| Database trait extraction touches every service | High | High | Define trait in core first; move SQLite adapter to new crate after all consumers compile against the trait |
| File splits create merge conflicts during parallel work | Medium | Medium | Use task assignments per module; avoid overlapping changes to same directories |
| `pub(crate)` reveals hidden dependencies between modules | High | Medium | Temporarily promote to `pub` with `#[doc(hidden)]` where needed; schedule re-export cleanup in Phase 4 |

### Success Criteria
- [ ] `blazecode-core` split into 5+ domain crates, all compiling independently
- [ ] `Database` trait extracted with `SqliteDatabase` adapter in separate crate
- [ ] `FileSystem` trait extracted with `TokioFileSystem` adapter
- [ ] No module exposes all items as `pub` — `pub(crate)` discipline enforced
- [ ] Zero files >2,000 lines remaining
- [ ] `MockDatabase` available in test crate
- [ ] Architecture score improves from 25/100 to 50/100

### Estimated Person-Weeks
**10 person-weeks** (51 person-days, significant parallelization across domain teams)

### Key Deliverables
- `crates/blazecode-session-core/`, `blazecode-tool-core/`, `blazecode-database/`, `blazecode-filesystem/`, `blazecode-provider-core/`, `blazecode-plugin-core/`
- Directory-based module structure for all previously monolithic files
- `blazecode-core/src/lib.rs` with explicit `pub use` API surface (<30 re-exports)
- `ADAPTERS.md` documenting trait extraction patterns

### Task Table

| Task | Effort (pw) | Dependencies | Risk | Priority |
|------|------------|-------------|------|----------|
| Extract `blazecode-session-core` | 1.0 | Phase 1, Task 7 (visibility) | High | Critical |
| Extract `blazecode-tool-core` | 1.0 | Phase 1, Task 7 | High | Critical |
| Extract `blazecode-database` trait | 2.0 | Phase 1, Task 7 | High | Critical |
| Extract `blazecode-filesystem` trait | 1.6 | Phase 1, Task 7 | High | Critical |
| Extract `blazecode-provider-core` | 1.0 | Phase 1, Task 7 | Medium | Critical |
| Extract `blazecode-plugin-core` | 0.6 | Phase 1, Task 7 | Medium | High |
| Visibility control (`pub(crate)`) | 1.0 | Phase 0 (dead-code removal) | Medium | Critical |
| Split files >2,000 lines | 2.0 | Task 7 (visibility) | Medium | Critical |

---

## Phase 3 — Provider Implementation (Weeks 15–22)

### Duration
8 weeks

### Objectives
- Implement all major LLM provider adapters with streaming support
- Port the route-based LLM architecture from BlazeCode's `packages/llm/src/route/`
- Implement consistent retry, timeout, fallback logic across all providers
- Support credential management for all provider auth strategies

### Detailed Tasks

| # | Task | Owner/Role | Effort |
|---|------|------------|--------|
| 1 | Implement Anthropic provider — Messages API with SSE streaming, tool use, system message support | Provider Team | 8 pd |
| 2 | Implement OpenAI provider — Responses API with streaming, tool calling, structured outputs | Provider Team | 8 pd |
| 3 | Implement Google Gemini provider — streaming via SSE, safety settings, function calling | Provider Team | 5 pd |
| 4 | Implement AWS Bedrock provider — Converse API, cross-region inference, IAM auth | Provider Team | 8 pd |
| 5 | Implement Azure OpenAI provider — tenant-specific endpoints, Entra ID auth | Provider Team | 3 pd |
| 6 | Implement remaining providers — xAI (Grok), OpenRouter, DeepSeek, Groq, Together, Fireworks as OpenAI-compatible configs | Provider Team | 8 pd |
| 7 | Port route-based LLM architecture — `Protocol` trait (body/step/frame), `Route` compositor (protocol + endpoint + auth + framing), shared protocol logic for OpenAI-compatible providers | Provider Team | 15 pd |
| 8 | Implement provider retry/timeout/fallback — exponential backoff with jitter, circuit breaker, fallback chain | Provider Team | 5 pd |

### Dependencies
- Phase 1 must be complete (`blazecode-types`, `blazecode-config`, `blazecode-error` needed by provider trait)
- Phase 2 Task 5 (`blazecode-provider-core` trait extraction) must be complete
- Phase 2 Task 4 (HTTP client trait) should be complete for consistent retry/timeout

### Risks and Mitigations
| Risk | Likelihood | Impact | Mitigation |
|------|-----------|--------|------------|
| API changes in provider protocols during implementation | Medium | High | Pin to known API versions; log warnings on unexpected response fields |
| SSE streaming edge cases (partial frames, reconnection) | High | Medium | Implement SSE parser with state machine; test with recorded fixtures |
| Route architecture port is complex and may delay per-provider work | High | High | Ship Anthropic + OpenAI providers with direct implementation first (weeks 1–4); port route architecture in parallel (weeks 4–8); migrate providers after route architecture is stable |
| API rate limits during integration testing | Medium | Low | Use `httpmock` or `wiremock` for most tests; run live tests on schedule, not per-commit |

### Success Criteria
- [ ] Anthropic + OpenAI providers work end-to-end with streaming tool-calling sessions
- [ ] 10+ providers configured and tested with recorded fixtures
- [ ] Route architecture implemented: `Protocol`, `Endpoint`, `Auth`, `Framing` traits with shared `OpenAIChatProtocol` reused across 14 providers
- [ ] Retry with exponential backoff + jitter works; circuit breaker opens after N consecutive failures
- [ ] Timeout configured per-provider with default 120s
- [ ] Credential management loads API keys from encrypted store; supports env-var fallback

### Estimated Person-Weeks
**16 person-weeks** (60 person-days, high parallelism across providers)

### Key Deliverables
- `crates/blazecode-provider-anthropic/`, `blazecode-provider-openai/`, `blazecode-provider-gemini/`, `blazecode-provider-bedrock/`, `blazecode-provider-azure/`
- `crates/blazecode-provider-route/` with route architecture
- Provider integration test suite with HTTP fixtures
- Provider benchmark: time-to-first-token, tokens-per-second

### Task Table

| Task | Effort (pw) | Dependencies | Risk | Priority |
|------|------------|-------------|------|----------|
| Anthropic provider | 1.6 | blazecode-provider-core, HTTP trait | Medium | Critical |
| OpenAI provider | 1.6 | blazecode-provider-core, HTTP trait | Medium | Critical |
| Google Gemini provider | 1.0 | blazecode-provider-core, HTTP trait | Medium | High |
| AWS Bedrock provider | 1.6 | blazecode-provider-core, HTTP trait, AWS SDK | High | High |
| Azure OpenAI provider | 0.6 | blazecode-provider-core, HTTP trait | Medium | High |
| Remaining providers (7+) | 1.6 | OpenAI route architecture | Low | Medium |
| Route-based LLM architecture | 3.0 | blazecode-provider-core, HTTP trait | High | Critical |
| Retry/timeout/fallback logic | 1.0 | blazecode-provider-core | Medium | High |

---

## Phase 4 — Session V2 Architecture (Weeks 23–30)

### Duration
8 weeks

### Objectives
- Implement BlazeCode's V2 session architecture in Rust: durable prompt admission, algebraic system context, EventV2 event sourcing
- Enable crash-recoverable sessions with replayable event streams
- Implement context epochs with immutable baselines and mid-conversation system messages
- Port all 129 rules from `blazecode/CONTEXT.md` as test cases

### Detailed Tasks

| # | Task | Owner/Role | Effort |
|---|------|------------|--------|
| 1 | Implement durable prompt admission — `session_input` SQLite table, promotion lifecycle (admitted → visible), delivery modes (steer/queue), retry reconciliation via prompt IDs | Session Team | 10 pd |
| 2 | Implement algebraic system context — `ContextSource` trait with stable key, JSON codec, baseline/update/removal renderers; `SystemContextRegistry` with scoped contributions | Session Team | 15 pd |
| 3 | Implement EventV2 with replay — SQLite-backed event store with aggregate sequence cursor; `subscribe(after: SequenceCursor)` returns tail events; event-sourced session reconstruction | Session Team | 15 pd |
| 4 | Implement session snapshots and compaction — tail-turns preservation, summary generation, overflow detection; compaction triggers at configurable token threshold | Session Team | 8 pd |
| 5 | Implement session recovery — on load, replay events from last snapshot cursor; rebuild `SessionInfo`, `Message` list, and `ContextEpoch` state | Session Team | 5 pd |
| 6 | Implement session projector system — event-to-message mapping; `SessionProjector` rebuilds message state from event stream on replay | Session Team | 8 pd |
| 7 | Port System Context algebra — `ContextEpoch` with immutable `Baseline`, epoch-level `Snapshot`, chronological `MidConversationSystemMessage`; 129 rules from CONTEXT.md as test cases | Session Team | 20 pd |

### Dependencies
- Phase 2 must be complete (database trait, session-core extraction)
- Phase 3 must be partially complete (at least Anthropic/OpenAI providers for end-to-end testing)
- Phase 1 structured concurrency should be in place for fiber management

### Risks and Mitigations
| Risk | Likelihood | Impact | Mitigation |
|------|-----------|--------|------------|
| Algebraic system context is complex — 129 rules in CONTEXT.md | High | High | Implement incrementally: baseline first, then epochs, then mid-conversation messages. Write test per rule. |
| EventV2 port from TypeScript Effect pattern is non-trivial | High | High | Simplify: Rust async fn instead of Effect; `tokio::sync::broadcast` for live tail, SQLite for durable store |
| Session recovery correctness under edge cases | Medium | High | Property-based tests with `proptest` for event sequence replay; compare rebuilt state to original |
| Projector system may not match TS behavior exactly | Medium | Medium | Use CONTEXT.md rules as specification; verify each rule independently |

### Success Criteria
- [ ] Prompt admission durable across process restart; admitted prompts survive crash
- [ ] System context produces correct epoch baselines; mid-conversation messages injected at correct positions
- [ ] EventV2 store persists events; replay from cursor produces identical state
- [ ] Session compaction triggers at configurable threshold; tail turns preserved
- [ ] Session recovery from last snapshot + remaining events produces identical state to pre-crash
- [ ] All 129 CONTEXT.md rules pass as test cases
- [ ] Architecture score improves from 50/100 to 70/100

### Estimated Person-Weeks
**18 person-weeks** (81 person-days, some work parallelizable between event store and system context)

### Key Deliverables
- `crates/blazecode-session-event/` with EventV2 implementation
- `crates/blazecode-system-context/` with algebraic context engine
- `crates/blazecode-session-projector/` with projector system
- `CONTEXT_RULES.md` — all 129 rules with test status
- Session recovery integration tests

### Task Table

| Task | Effort (pw) | Dependencies | Risk | Priority |
|------|------------|-------------|------|----------|
| Durable prompt admission | 2.0 | Database trait, session-core | High | Critical |
| Algebraic system context | 3.0 | session-core, EventV2? | High | Critical |
| EventV2 with replay | 3.0 | Database trait, bus | High | Critical |
| Session snapshots/compaction | 1.6 | EventV2, provider (for summarization) | Medium | High |
| Session recovery | 1.0 | EventV2, snapshots | Medium | Critical |
| Session projector system | 1.6 | EventV2 | Medium | High |
| System Context algebra (129 rules) | 4.0 | system context module | Very High | Critical |

---

## Phase 5 — Plugin & SDK (Weeks 31–38)

### Duration
8 weeks

### Objectives
- Publish `blazecode-sdk` on crates.io for third-party plugin development
- Implement WASM-based plugin sandboxing (Rust's unique security moat)
- Build plugin discovery, loading, permissions, and lifecycle management
- Create example plugins and developer guide

### Detailed Tasks

| # | Task | Owner/Role | Effort |
|---|------|------------|--------|
| 1 | Publish `blazecode-sdk` crate to crates.io — core trait definitions (`Plugin`, `ProviderPlugin`, `ToolPlugin`), minimal dependencies, stable API with semver guarantees | Platform Team | 5 pd |
| 2 | Implement WASM plugin sandbox — `wasmtime`-based runtime; define WIT interface for plugin ↔ host communication; resource limits (memory, CPU, file system access) | Platform Team | 20 pd |
| 3 | Add plugin discovery and loading — scan `~/.blazecode/plugins/` for WASM files; load and validate at startup; plugin registry with versioning | Platform Team | 5 pd |
| 4 | Add plugin permissions system — capability-based permissions (read/write/network); user-approval prompts for elevated capabilities; permission cache for session | Platform Team | 8 pd |
| 5 | Add TUI plugin support — plugin-defined TUI panels; ratatui widget registration; theme extension | TUI Team | 5 pd |
| 6 | Add tool plugin support — plugin-defined tools with `#[tool]` proc macro; tool discovery from plugin providers | Tool Team | 5 pd |
| 7 | Write plugin developer guide — setup, crate template, publishing checklist, best practices | DevRel | 5 pd |
| 8 | Create 3 example plugins — `example-weather` (simple tool), `example-formatter` (output transform), `example-lint` (context source) | DevRel | 5 pd |

### Dependencies
- Phase 2 must be complete (plugin-core crate, tool-core crate)
- Phase 4 session architecture useful but not strictly required for basic plugin SDK
- WASM plugin sandbox requires `wasmtime` dependency investigation

### Risks and Mitigations
| Risk | Likelihood | Impact | Mitigation |
|------|-----------|--------|------------|
| WASM sandbox adds significant complexity and maintenance burden | High | Medium | Start with simple `libloading` (`.so`/`.dylib`) plugins first; WASM sandbox as optional tier-2 support |
| `wasmtime` version upgrades may break plugin ABI | Medium | Medium | Pin `wasmtime` version in SDK; communicate breaking changes clearly |
| Plugin SDK API churn during core refactoring | High | Medium | Keep SDK crate minimal — trait-only, no core dependencies; version independently |
| Low third-party adoption initially | High | Low | Focus on first-party plugins first; SDK maturity before community outreach |

### Success Criteria
- [ ] `blazecode-sdk` published on crates.io with docs.rs documentation
- [ ] WASM plugin loads, executes tool call, returns result within sandbox resource limits
- [ ] Plugin permissions system blocks unauthorized filesystem/network access
- [ ] 3 example plugins compile and run
- [ ] Plugin developer guide published alongside SDK
- [ ] `cargo test` in plugin template passes with SDK dependency

### Estimated Person-Weeks
**14 person-weeks** (58 person-days, parallelizable across SDK, WASM, TUI, and tools tracks)

### Key Deliverables
- `crates/blazecode-sdk/` published on crates.io
- `crates/blazecode-plugin-wasm/` with `wasmtime` sandbox
- `crates/blazecode-plugin-permissions/` with capability-based security
- 3 example plugin repositories
- Plugin developer guide (`docs/plugin-development.md`)

### Task Table

| Task | Effort (pw) | Dependencies | Risk | Priority |
|------|------------|-------------|------|----------|
| Publish `blazecode-sdk` to crates.io | 1.0 | Phase 2 domain crates | Medium | Critical |
| WASM plugin sandbox | 4.0 | Plugin-core crate | Very High | High |
| Plugin discovery and loading | 1.0 | Plugin-core crate | Medium | High |
| Plugin permissions system | 1.6 | Plugin-core, permission modules | High | High |
| TUI plugin support | 1.0 | TUI crate, plugin-core | Medium | Medium |
| Tool plugin support (`#[tool]` macro) | 1.0 | Tool-core, blazecode-derive | Medium | High |
| Plugin developer guide | 1.0 | SDK published | Low | Medium |
| Example plugins (3) | 1.0 | SDK published | Low | Medium |

---

## Phase 6 — Scale & Deploy (Weeks 39–52)

### Duration
12 weeks

### Objectives
- Enable server-mode deployment with PostgreSQL backend
- Add distributed event bus for multi-instance coordination
- Implement production monitoring (health checks, metrics, rate limiting)
- Provide Kubernetes/Docker deployment artifacts

### Detailed Tasks

| # | Task | Owner/Role | Effort |
|---|------|------------|--------|
| 1 | Implement PostgreSQL backend for `Database` trait — `sqlx` PostgreSQL adapter, connection pooling with `deadpool`, migration system with `sqlx::migrate!` | Backend Team | 15 pd |
| 2 | Add connection pooling and migration system — environment-specific config, migration version tracking, rollback support | Backend Team | 8 pd |
| 3 | Implement distributed event bus — optional NATS or Redis pub/sub for cross-instance session events; bridge to local `tokio::sync::broadcast` | Backend Team | 10 pd |
| 4 | Add health checks and metrics endpoints — `/health` (liveness), `/ready` (readiness), `/metrics` (Prometheus OpenMetrics format) | Backend Team | 8 pd |
| 5 | Add rate limiting and resource limits — per-user rate limiting (token bucket), per-session resource quotas (tokens, time, tool calls) | Backend Team | 8 pd |
| 6 | Implement workspace routing for multi-instance — session ownership hash ring, sticky routing to instance holding session | Backend Team | 10 pd |
| 7 | Add Kubernetes manifests / Docker Compose — `Dockerfile` (multi-stage, ~15MB binary), `kustomize` overlays for dev/staging/prod | DevOps | 8 pd |
| 8 | Add CI/CD for server deployment — GitHub Actions deploy to staging on main merge; canary release to production | DevOps | 5 pd |

### Dependencies
- Phase 2 `Database` trait must be complete (PostgreSQL is another adapter)
- Phase 4 EventV2 needed for distributed event bus integration
- Phase 3 providers needed for server-mode LLM calls
- Phase 5 plugin system useful but not blocking

### Risks and Mitigations
| Risk | Likelihood | Impact | Mitigation |
|------|-----------|--------|------------|
| PostgreSQL adapter has different semantics than SQLite (transactions, locking) | High | High | Build comprehensive integration test suite that runs against both backends |
| Distributed event bus adds operational complexity | Medium | Medium | NATS is lightweight; document deployment topology; default to single-instance mode |
| Rate limiting introduces new failure modes (false positives) | Medium | Medium | Configurable limits; per-user override; clear error messages with retry-after headers |
| Workspace routing requires sticky sessions | High | Medium | Session ownership in database; routing table with heartbeat; rebalance on failure |

### Success Criteria
- [ ] PostgreSQL backend passes same integration test suite as SQLite
- [ ] Migrations run on startup; migration history tracked in database
- [ ] NATS/Redis event bus delivers session events across instances with <100ms latency
- [ ] Health check endpoint returns 200 with version info; metrics endpoint produces valid Prometheus output
- [ ] Rate limiting rejects requests above configured threshold with proper headers
- [ ] Multi-instance deployment routes session requests to correct owner
- [ ] `kubectl apply -k overlays/prod` deploys functioning server
- [ ] CI/CD pipeline deploys to staging automatically

### Estimated Person-Weeks
**18 person-weeks** (72 person-days, parallelizable across backend, DevOps, and infrastructure tracks)

### Key Deliverables
- `crates/blazecode-database-postgres/` with PostgreSQL adapter
- `crates/blazecode-event-bus-nats/` or `blazecode-event-bus-redis/`
- `Dockerfile` + `docker-compose.yml` + `kustomize/` directory
- Deployment CI/CD workflow
- Operations runbook (`docs/operations.md`)

### Task Table

| Task | Effort (pw) | Dependencies | Risk | Priority |
|------|------------|-------------|------|----------|
| PostgreSQL backend | 3.0 | Database trait (Phase 2) | High | Critical |
| Connection pooling and migrations | 1.6 | PostgreSQL backend | Medium | Critical |
| Distributed event bus | 2.0 | EventV2 (Phase 4) | High | High |
| Health checks and metrics | 1.6 | Server crate | Medium | High |
| Rate limiting and resource limits | 1.6 | Server crate, auth | Medium | Medium |
| Workspace routing (multi-instance) | 2.0 | Session distribution | High | Medium |
| K8s manifests / Docker Compose | 1.6 | All server components | Medium | High |
| CI/CD for server deployment | 1.0 | K8s manifests | Medium | Medium |

---

## Phase 7 — Platform Expansion (Weeks 53–72)

### Duration
18 weeks

### Objectives
- Extend BlazeCode to desktop, web, and IDE platforms
- Enable internationalization for global reach
- Add opt-in analytics and crash reporting for product insights

### Detailed Tasks

| # | Task | Owner/Role | Effort |
|---|------|------------|--------|
| 1 | Build Electron desktop app — Tauri (Rust-native, lighter than Electron) shell; session list UI; settings panel; auto-update via `tauri-updater` | Desktop Team | 20 pd |
| 2 | Build web interface — SolidJS (matching BlazeCode's web stack) or Yew/Leptos (Rust-native WASM); REPL mode, session history, settings | Web Team | 30 pd |
| 3 | Build VS Code extension — LSP-backed extension; inline diff view; AI chat panel; command palette integration | IDE Team | 20 pd |
| 4 | Add internationalization — `rust-i18n` or `fluent-rs` for CLI/TUI/desktop messages; locale auto-detection; message externalization from all user-facing strings | Platform Team | 10 pd |
| 5 | Add usage analytics (opt-in) — anonymous event telemetry: session count, provider usage, tool usage, error rates; GDPR-compliant consent flow | Platform Team | 5 pd |
| 6 | Add crash reporting (opt-in) — `sentry-rust` or `backtrace`-based crash report generation; upload to configurable endpoint; include non-PII context | Platform Team | 5 pd |

### Dependencies
- Phase 6 server infrastructure should be operational (analytics and crash reporting need endpoints)
- CLI/TUI should be feature-complete (Phases 0–5)
- VS Code extension depends on LSP crate

### Risks and Mitigations
| Risk | Likelihood | Impact | Mitigation |
|------|-----------|--------|------------|
| Web interface significantly expands scope | High | High | Consider SolidJS to reuse BlazeCode's web architecture pattern; Yew/Leptos pure-Rust option is riskier; defer web to post-MVP |
| Electron/Tauri desktop has high maintenance cost | High | Medium | Tauri is lighter than Electron; focus on CLI-first, desktop as secondary |
| VS Code extension requires LSP maturity | Medium | High | Ship VS Code extension with MCP integration instead of LSP for faster path to IDE integration |
| Low engagement with analytics/crash reporting | Medium | Low | Ship opt-in with clear privacy policy; use analytics to prioritize features rather than tracking individuals |
| i18n requires significant string externalization | Medium | Low | Start with Japanese and German for highest-impact markets; crowd-source translations |

### Success Criteria
- [ ] Tauri desktop app launches, loads sessions, and can run basic prompts
- [ ] Web interface allows prompt input and displays session history (read-only MVP)
- [ ] VS Code extension shows AI chat panel and can apply edits to open files
- [ ] CLI/TUI displays messages in user's locale (where translation exists)
- [ ] Analytics pipeline captures session count, provider usage, error rate
- [ ] Crash reports include backtrace + context; no PII transmitted
- [ ] Privacy policy published for analytics/crash reporting

### Estimated Person-Weeks
**24 person-weeks** (90 person-days, high parallelism across desktop, web, IDE, i18n tracks)

### Key Deliverables
- `crates/blazecode-desktop/` (Tauri app)
- `crates/blazecode-web/` (SolidJS or Yew frontend)
- `sdks/vscode/` directory with VS Code extension
- `locales/` directory with `.ftl` (Fluent) translation files
- Analytics dashboard reference deployment
- Privacy policy document

### Task Table

| Task | Effort (pw) | Dependencies | Risk | Priority |
|------|------------|-------------|------|----------|
| Tauri desktop app | 4.0 | Core CLI (Phase 0–4) | High | Medium |
| Web interface (SolidJS/Yew) | 6.0 | Server (Phase 6) | Very High | Medium |
| VS Code extension | 4.0 | LSP/MCP (Phase 2) | High | High |
| Internationalization | 2.0 | CLI/TUI completion | Medium | Low |
| Usage analytics (opt-in) | 1.0 | Server (Phase 6) | Low | Medium |
| Crash reporting (opt-in) | 1.0 | Observability crate | Low | Medium |

---

## Summary Resource Plan

| Phase | Weeks | Person-Weeks | Key Milestone |
|-------|-------|-------------|---------------|
| P0: Critical Fixes | 1–2 | 3 | Security audit passes; no data-corrupting bugs |
| P1: Foundational Refactoring | 3–6 | 8 | Types/error/config crates extracted; structured concurrency |
| P2: Domain Extraction | 7–14 | 10 | Core split into 5+ domain crates; all infra behind traits |
| P3: Provider Implementation | 15–22 | 16 | 10+ providers working; route architecture in place |
| P4: Session V2 Architecture | 23–30 | 18 | EventV2 session recovery; 129 context rules passing |
| P5: Plugin & SDK | 31–38 | 14 | SDK on crates.io; WASM sandbox prototype |
| P6: Scale & Deploy | 39–52 | 18 | PostgreSQL backend; K8s deployment; metrics |
| P7: Platform Expansion | 53–72 | 24 | Desktop app; VS Code extension; i18n |
| **Total** | **1–72** | **111** | |

### Team Composition Recommendation

| Role | Phase 0 | Phase 1 | Phase 2 | Phase 3 | Phase 4 | Phase 5 | Phase 6 | Phase 7 |
|------|---------|---------|---------|---------|---------|---------|---------|---------|
| Core Team (2–3) | 2 | 2 | 2 | 1 | 1 | 1 | 1 | 1 |
| Session Team (2) | — | — | 1 | — | 3 | — | — | — |
| Provider Team (2) | — | — | — | 3 | — | — | 1 | — |
| Platform Team (2) | 1 | 1 | 1 | — | — | 2 | 1 | 1 |
| Tool Team (1) | — | — | 1 | — | — | 1 | — | — |
| Backend Team (2) | — | — | — | — | — | — | 2 | — |
| DevOps (1) | — | — | — | — | — | — | 1 | — |
| Desktop/Web/IDE (2–3) | — | — | — | — | — | — | — | 3 |
| DevRel (1) | — | — | — | — | — | 1 | — | 1 |
| **Total Headcount** | **3** | **3** | **5** | **4** | **4** | **5** | **6** | **6** |

---

## Architecture Score Progression

| Metric | Before | After P0 | After P1 | After P2 | After P3 | After P4 | After P5 | After P6–7 |
|--------|--------|----------|----------|----------|----------|----------|----------|------------|
| Architecture Score | 25/100 | 30/100 | 35/100 | 50/100 | 55/100 | 70/100 | 75/100 | 80/100 |
| Public modules (blazecode-core) | 95 (all pub) | 95 (all pub) | 95 (30 pub, 65 `pub(crate)`) | 8 crates | 10+ crates | 12+ crates | 14+ crates | 16+ crates |
| Files >2,000 lines | 14 | 14 | 14 | 0 | 0 | 0 | 0 | 0 |
| `unwrap()` in non-test code | 500+ | ~300 | ~70 | ~5 | ~0 | 0 | 0 | 0 |
| Test coverage | <2% | <5% | <10% | ~25% | ~35% | ~55% | ~65% | >75% |
| Provider implementations | 0 | 0 | 0 | 0 | 10+ | 10+ | 10+ | 12+ |
| Plugins supported | 0 | 0 | 0 | 0 | 0 | 0 | WASM+lib | WASM+lib |
| Session recovery | No | No | No | No | No | Yes | Yes | Yes |

---

## Gantt-Style Timeline

```
Week       0    5    10   15   20   25   30   35   40   45   50   55   60   65   70
Phase      |    |    |    |    |    |    |    |    |    |    |    |    |    |    |
P0: Fixes  ████                                                                      
P1: Found  ████████                                                                  
P2: Domain ████████████████                                                          
P3: Prov.       ████████████████████                                                 
P4: V2 Sess                 ████████████████████                                     
P5: Plugin                              ████████████████████                         
P6: Scale                                          ████████████████████████          
P7: Plat.                                                        ████████████████████

Team Load  ░░  ░░  ░░░  ░░░  ░░░  ░░░  ░░░  ░░░  ░░░  ░░░  ░░░  ░░░  ░░░  ░░░  ░░░
           ░░  ░░  ░░░  ░░░  ░░░  ░░░  ░░░  ░░░  ░░░  ░░░  ░░░  ░░░  ░░░  ░░░  ░░░

Key: ████ = active phase    ░░  = team active (2–6 people)
```

### Phase Dependency Diagram

```
P0 → P1 → P2 → P3 ─┐
           ↓        ↓
           P5    P4 ─┐
                 ↓   ↓
                 P6 → P7
```

---

## Decision Log

| Decision | Rationale | Date |
|----------|-----------|------|
| Phase 0 is separate from Phase 1 refactoring | Critical bugs must ship before structural changes create merge conflicts | 2026-06-21 |
| Phase 3 provider implementation runs partially in parallel with Phase 2 | Providers depend on trait extraction (Phase 2), not on all domain crates | 2026-06-21 |
| Route architecture port is prioritized over per-provider volume | Correctness and maintenance savings outweigh initial effort; Anthropic+OpenAI shipped first | 2026-06-21 |
| WASM plugin sandbox deferred to Phase 5 | Plugin SDK on crates.io is higher priority than sandboxing; simple `.so` loading first | 2026-06-21 |
| Tauri over Electron for desktop | Rust-native, smaller binary, better fit with BlazeCode's existing Rust stack | 2026-06-21 |
| PostgreSQL in Phase 6 (not Phase 2) | Database trait abstraction allows swapping; PostgreSQL adds operational complexity | 2026-06-21 |
| i18n deferred to Phase 7 | No internationalized strings exist yet; focus on feature completeness first | 2026-06-21 |

---

*Report generated by Implementation Roadmap Agent — synthesized from Architecture (02), Feature Gap (08), Competitive Intelligence (17), Refactoring (18), and Technical Debt (19) reports.*
