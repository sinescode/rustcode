# Architecture Analysis Report — Agent 02

**Date:** 2026-06-21
**Analyzed Repos:** BlazeCode (Rust port) vs BlazeCode (TypeScript source)
**Scope:** Deep architectural analysis across 8 dimensions

---

## 1. Layering

- **Location:** `blazecode/crates/blazecode-core/src/lib.rs:1-95`
- **BlazeCode:** 26 packages with clean dependency direction. `packages/blazecode/src/` (CLI + orchestration) depends on `packages/core/src/` (business logic, DB, session runner) which depends on `packages/llm/src/` (LLM protocol adapters). The dependency graph is a DAG: `core → llm`, `server → core`, `tui → core`, `cli → core`. No circular or upward dependencies. V2 adds `packages/effect-drizzle-sqlite` and `packages/effect-sqlite-node` as infrastructure layers below core.
- **BlazeCode:** Two layers exist: `blazecode-core` (business logic) and 5 thin wrapper crates (`blazecode-server`, `blazecode-tui`, `blazecode-lsp`, `blazecode-mcp`). The binary (`src/main.rs`, 8,575 lines) is NOT a thin CLI — it contains inline implementations of commands, sessions, providers, database queries, SSE handling, and configuration. The binary depends on wrapper crates but the wrapper crates are stub-level thin. The main.rs directly imports `blazecode_core::config::Config`, `sqlx`, `blazecode_core::*`, blurring the boundary.
- **Gap:** BlazeCode's binary is a monolith (8,575 lines) that merges CLI argument parsing, business logic, and infrastructure. BlazeCode's entrypoint is a thin CLI dispatch in `packages/blazecode/src/index.ts` (~200 lines) that delegates to Effect layers. BlazeCode wrapper crates are stubs; server, tui, lsp, mcp business logic lives in `blazecode-core` rather than in their respective crates.
- **Consequence:** BlazeCode has effectively 1.5 layers (core + thin wrappers) vs BlazeCode's 4+ layers (blazecode → core → llm → infrastructure). The thick main.rs makes testing, swapping, and parallel development harder.
- **Recommendation:** Move logic from main.rs into `blazecode-core` or the appropriate wrapper crate. Make main.rs a thin CLI dispatch (<500 lines). Extract server, TUI, LSP, and MCP logic from core into their respective crates.
- **Severity:** High

---

## 2. Boundaries

- **Location:** `blazecode/crates/blazecode-core/src/lib.rs:1-95` (all 95 modules pub, no re-export filtering)
- **BlazeCode:** Clean package boundaries enforced by TypeScript module resolution. `packages/core/src/` exports only specific symbols (e.g., `Session`, `Tool`, `Account`). Internal modules are truly private (not exported). Packages declare explicit dependencies in `package.json`. Cross-package references are explicit — no star re-exports. The V2 `packages/llm/src/` hides protocol adapter internals behind a clean `llm.stream(request)` interface.
- **BlazeCode:** All 95 modules in `blazecode-core` are `pub mod` — zero visibility filtering. Every module is accessible to every consumer. The `blazecode-server` crate's `lib.rs` imports from `blazecode_core::lsp`, `blazecode_core::mcp`, etc. — but the server crate re-exports core types rather than owning its own abstractions. The MCP crate (`blazecode-mcp`) re-exports from `blazecode_core::mcp` as its primary API surface.
- **Gap:** BlazeCode has no enforced module boundaries. All modules are world-public. This means a change to `session.rs` could affect `mcp.rs` consumers with no compiler guard.
- **Consequence:** Impossible to reason about which modules are internal implementation details vs public API. Refactoring requires understanding the full 95-module graph. Circular dependencies are possible at the logical level even if Rust's module system prevents them physically.
- **Recommendation:** Use `pub(crate)` for internal modules. Define a `lib.rs` that re-exports only the intended public API surface. Apply `#[doc(hidden)]` and module visibility discipline. Split blazecode-core into sub-crates (e.g., `blazecode-provider`, `blazecode-session`, `blazecode-config`).
- **Severity:** Critical

---

## 3. Coupling

- **Location:** `blazecode/crates/blazecode-core/src/lib.rs:1-95` (all 95 modules flat), `blazecode/src/main.rs:1-8575`
- **BlazeCode:** Low coupling via package boundaries and Effect's injection system. `packages/core` depends on `packages/llm` through interfaces (ports), not concrete implementations. `packages/blazecode` depends on `packages/core` through stable abstractions. Effect's `Layer` and `Context.Service` provide dependency injection without concrete coupling. The V2 session architecture (`packages/core/src/session-v2/`) is a distinct module with its own bounded context — no coupling to legacy session code.
- **BlazeCode:** Extreme coupling. All 95 modules in `blazecode-core` are flat-scoped public modules. The binary (`main.rs, 8,575 lines`) directly imports and uses `sqlx`, `blazecode_core::config`, `blazecode_core::*`, and embeds business logic that conceptually belongs in separate crates. The `blazecode-lsp` crate re-exports types from `blazecode_core::lsp` but the core LSP module is a stub while the real LSP logic lives in `blazecode-lsp` crate — this is circular at the semantic level. The provider module (`provider.rs`) references `database.rs`, `config.rs`, `tool.rs` — a dense dependency web.
- **Gap:** BlazeCode achieves low coupling through (a) physical package boundaries, (b) Effect's algebraic effect system for dependency injection, (c) event-driven architecture. BlazeCode uses flat module visibility with no DI pattern — modules import each other directly.
- **Consequence:** High coupling makes BlazeCode brittle. A change to `config.rs` can ripple through all 94 other modules. Testing any module in isolation requires importing the entire core crate. The scaffold phase keeps this manageable, but coupling will become the dominant cost as the codebase matures.
- **Recommendation:** Introduce trait-based dependency inversion within `blazecode-core`. Split into multiple crates (at minimum: `blazecode-provider`, `blazecode-session`, `blazecode-config`, `blazecode-tool`, `blazecode-core-types`). Dependency injection via constructor injection, not global state. Use `thiserror` for typed errors across crate boundaries.
- **Severity:** Critical

---

## 4. Cohesion

- **Location:** `blazecode/crates/blazecode-core/src/lib.rs:1-95`
- **BlazeCode:** High cohesion. Modules group related functionality by domain: `packages/llm/src/anthropic/`, `packages/llm/src/openai/`, `packages/core/src/session/`, `packages/core/src/tool/`. Each domain module contains types, logic, and tests for that specific concern. V2 introduces `packages/core/src/system-context/` with a clear algebraic design. The `packages/llm` package is cohesive — all 55 files deal with LLM protocol adaptation.
- **BlazeCode:** Mixed. Some modules are cohesive (e.g., `tool.rs`, `permission.rs`, `provider.rs` — each owns a single domain concept). Others are low-cohesion aggregations: `session_runner.rs`, `session_prompt.rs`, `session_projector.rs`, `session_history.rs`, `session_input_inbox.rs`, `session_epoch.rs`, `session_compaction.rs`, `session_message.rs`, `session_execution.rs`, `session_model.rs`, `session_reminders.rs`, `session_revert.rs`, `session_todo.rs`, `session_info.rs` — these 14 session-related modules are split into a high-level cluster but are all flat in the same crate with no sub-module grouping. Similarly, there are 8+ provider-related modules (`provider.rs`, `provider_service.rs`, `providers/`, `aisdk.rs`, `catalog.rs`, `model.rs`) all flat in `blazecode-core/src/`.
- **Gap:** BlazeCode uses flat name prefixing (`session_*`) instead of sub-modules or sub-crates. This is a filesystem convention masquerading as organization. When Rust's module system supports `pub mod session { ... }`, flat files indicate a failure to use language-level cohesion.
- **Consequence:** The 95 flat modules create a heavy cognitive load. Developers must scan the full list to understand what exists. Sub-module grouping would reduce the apparent surface area from 95 to ~15 groups. Module-internal types cannot be hidden.
- **Recommendation:** Group related modules into sub-modules: `pub mod session { ... }`, `pub mod provider { ... }`, `pub mod config { ... }`. Use `pub(crate)` within groups. At minimum, create directory-based modules for session (14 files → `session/mod.rs` + sub-files), provider (8 files → `provider/mod.rs`), tool (3 files → `tool/mod.rs`).
- **Severity:** High

---

## 5. Domain Design

- **Location:** `blazecode/AGENTS.md:148-158`, `blazecode/CONTEXT.md:1-129`, `blazecode/crates/blazecode-core/src/lib.rs:11-95`
- **BlazeCode:** Explicit domain model with bounded contexts:
  - **Account** — identity, auth, credentials
  - **Session** — durable conversational state, event sourcing (EventV2), execution lifecycle
  - **Tool** — registry, execution, output bounding, MCP integration
  - **Event** — event sourcing with replayable event streams, EventV2
  - **Provider** — LLM protocol adaptation, Model, StreamChunk, catalog
  - **System Context** — algebraic context composition: Context Source, Context Epoch, Baseline, Snapshot, Mid-Conversation System Message
  - **Permission** — policy evaluation, rules, agent-scoped authorization
  - **Location** — workspace scoping, filesystem authority
  - **Project** — project discovery, instruction loading
- **BlazeCode:** Flattened domain. All domains are present (session, provider, tool, permission, config, etc.) but as flat modules without bounded context. Key missing abstractions:
  - **System Context** — the CONTEXT.md algebra (epoch, baseline, snapshot, mid-conversation messages) is not represented in BlazeCode's 95-module list (`system_context` module exists but context source, epoch, and reconciliation concepts are absent)
  - **EventV2** — the event-sourcing architecture (replayable event streams, event store) is not present
  - **Location** — the Location-scoped service pattern is not yet represented
  - **Account** — account module exists as `account.rs` but without the full Identity domain
- **Gap:** BlazeCode V2 has a mature domain model with carefully separated concerns (System Context algebra is a first-class design). BlazeCode ported the V1/early-V2 module structure but lacks the V2 domain innovations (System Context algebra, EventV2, Location services, Context Epoch).
- **Consequence:** BlazeCode will diverge in capabilities as BlazeCode's V2 architecture matures. The System Context algebra (CONTEXT.md, 129 rules) is the core of BlazeCode's session intelligence — without it, BlazeCode cannot match session behavior. Porting these concepts into the existing flat module structure would be difficult.
- **Recommendation:** Map the V2 domain model before deeper implementation. Create dedicated modules for System Context algebra (`system_context/` with sub-modules for `epoch.rs`, `baseline.rs`, `snapshot.rs`, `source.rs`). Implement EventV2 event sourcing. Model `Location` as a first-class domain concept. Study `blazecode/CONTEXT.md` as a specification document and implement each rule as a test case.
- **Severity:** High

---

## 6. Hexagonal Architecture (Ports/Adapters)

- **Location:** `blazecode/docs/plugin-system.md:199-243` (provider trait), `blazecode/packages/llm/src/` (LLM port/adapter)
- **BlazeCode:** Emerging hexagonal architecture in V2:
  - **Ports:** `packages/llm/src/` defines a clean `llm.stream(request)` port with provider-neutral types (ModelRequestOptions, GenerationControls). The System Context algebra defines ports for Context Source producers, registry, and reconciliation. `packages/core/src/` defines ports for `SessionStore`, `ToolRegistry`, `PermissionEvaluator`.
  - **Adapters:** `packages/llm/src/anthropic/`, `packages/llm/src/openai/`, `packages/llm/src/bedrock/` are protocol adapters implementing the LLM port. `packages/effect-drizzle-sqlite/` is a database adapter. `packages/plugin/` (`@blazecode-ai/plugin`) is a plugin SDK adapter.
  - **Infrastructure:** Infrastructure concerns (SQLite, filesystem, HTTP server) are injected via Effect's Layer system, not imported directly by core.
- **BlazeCode:** Partial hexagonal architecture:
  - **Provider trait** (`blazecode_core::provider::Provider`) is a clean port — `ProviderCatalog` holds `Box<dyn Provider>`. The `OpenAICompatibleProvider` in `openai_compatible.rs` is an adapter with 14 pre-configured profiles. This is genuinely good port/adapter design.
  - **Plugin trait** (`blazecode_core::plugin::ProviderPlugin`) is another clean port with three hooks. The 3-plugin-type system (config, closure, trait) is well-designed.
  - **However:** The database (`sqlx`), HTTP server (`axum`), and filesystem access are imported directly in `main.rs` and throughout `blazecode-core`. There is no infrastructure abstraction layer — core code directly calls SQL queries and filesystem operations.
  - **Plugin Portal:** The plugin system is provider-only. There is no general plugin hook for tool registration, session hooks, or configuration augmentation.
- **Gap:** BlazeCode has good port/adapter for LLM providers but lacks it everywhere else. BlazeCode applies the pattern consistently across all domains (LLM, database, filesystem, plugin, session store).
- **Consequence:** Testing BlazeCode requires real infrastructure (SQLite files, real filesystem, network). BlazeCode's hexagonal architecture allows each domain to be tested with in-memory or mock adapters. BlazeCode's infrastructure coupling will grow as more domains are implemented.
- **Recommendation:** Generalize the port/adapter pattern. Define traits for `Database`, `FileSystem`, `EventStore`, `SessionStore`. Move infrastructure implementations to adapter crates (`blazecode-database-sqlite`, `blazecode-filesystem-local`). Follow BlazeCode's approach: core depends on ports (traits), adapters depend on core + concrete infrastructure.
- **Severity:** High

---

## 7. Clean Architecture (Dependency Rule)

- **Location:** `blazecode/Cargo.toml:1-96`, `blazecode/src/main.rs:1-8575`
- **BlazeCode:** Clean Architecture dependency rule is maintained:
  - Outer layers (CLI, server, TUI) depend on inner layers (core)
  - Core depends on nothing (except Effect runtime)
  - LLM is a port — core depends on the port interface, not on adapters
  - Infrastructure adapters (SQLite, filesystem) are injected at the composition root
  - Dependencies point inward: `blazecode(CLI) → core → llm(port)`
  - No infra import in core code
- **BlazeCode:** Dependency rule is violated:
  - `main.rs` (outermost layer) is 8,575 lines and directly calls business logic
  - `blazecode-core` (inner layer) imports `sqlx`, `reqwest`, `serde_json`, `tracing` — infrastructure concerns
  - `blazecode-core` has `pub mod database` with SQLite schema definitions and queries inline
  - `blazecode-core` directly constructs HTTP clients and makes network requests in provider code
  - 5 thin wrapper crates exist but most logic is in core, not delegated outward
  - Dependencies from core to infra are direct, not inverted
  - The dependency graph is roughly: `main → core → (infra + all domains + business logic)` — a flat star, not nested layers
- **Gap:** BlazeCode violates the Dependency Inversion Principle (DIP). Core should define ports; infrastructure should implement them. Instead, core imports infrastructure directly.
- **Consequence:** Impossible to swap infrastructure (e.g., SQLite → PostgreSQL, reqwest → hyper) without modifying core code. Business logic is polluted with serialization formats, HTTP status codes, and SQL queries. This is the single largest architectural debt in BlazeCode.
- **Recommendation:** Invert all dependencies. Define `Database` trait in core, implement in `blazecode-database-sqlite`. Define `HttpClient` trait in core, implement with `reqwest` in `blazecode-http`. Define `FileSystem` trait in core, implement in `blazecode-filesystem`. Apply Clean Architecture: entities/use-cases in core, interface adapters in middle layer, frameworks/infrastructure in outermost layer.
- **Severity:** Critical

---

## 8. Modularization

- **Location:** `blazecode/Cargo.toml:1-96` (5 crates), `blazecode/packages/` (26 packages)
- **BlazeCode:** Strategically modularized:
  - **26 packages** in monorepo (turborepo managed)
  - Granularity varies: packages for single responsibility: `core`, `llm`, `tui`, `cli`, `server`, `app`, `plugin`, `desktop`, `web`, `slack`, `script`, `ui`, `function`, `http-recorder`
  - Internal structure within packages uses directory-based modules
  - V2 introduces `packages/effect-drizzle-sqlite` and `packages/effect-sqlite-node` as infrastructure-layer packages
  - `packages/plugin/` is an SDK published as `@blazecode-ai/plugin` — independent of the main monorepo
  - Plugin SDK has its own build pipeline and versioning
- **BlazeCode:** Minimal modularization:
  - **5 crates** in workspace: core, server, tui, lsp, mcp
  - 4 of 5 crates are stubs — they re-export from core or have minimal logic
  - No infrastructure crates (database, http, filesystem) — all in-mixed in core
  - No plugin SDK crate — plugin system is core-internal
  - No event-store crate
  - No CLI crate (CLI logic is in main.rs, not in a reusable library crate)
  - No schema/migration crate (database schema is inline in `blazecode-core/src/database.rs`)
- **Gap:** BlazeCode's modularization is 5x fewer crates than BlazeCode's 26 packages. The 4 wrapper crates are placeholder-level, providing no real separation. Core acts as a dump for all concerns. The lack of a reusable CLI crate means tests and alternative front-ends cannot reuse CLI logic.
- **Consequence:** Build times will degrade as core grows (Rust recompiles all 95 modules even for small changes). No reuse path (cannot publish `@blazecode-ai/plugin` equivalent). Third-party contributions are harder — each new feature either goes into core (inflating it) or requires creating a new crate (high ceremony).
- **Recommendation:** Extract into more granular crates following BlazeCode's package boundaries:
  - `blazecode-core-types` — shared types, traits, no implementation
  - `blazecode-provider` — LLM provider traits + implementations
  - `blazecode-session` — session management + event sourcing
  - `blazecode-database-sqlite` — SQLite implementation
  - `blazecode-http` — HTTP client abstraction
  - `blazecode-plugin-sdk` — plugin trait + SDK (publishable as standalone)
  - `blazecode-cli` — CLI argument parsing + dispatch (library)
  - Keep `blazecode-core` for pure business logic only
- **Severity:** Critical

---

## Architecture Scores

### BlazeCode Architecture Score: 85/100

**Justification:**
- Strong domain model with explicit bounded contexts (+15)
- Emerging hexagonal architecture with clean ports/adapters (+15)
- Clean dependency direction (inward dependencies) (+15)
- Excellent modularization (26 packages, clear responsibilities) (+15)
- Effect-system for algebraic dependency injection (+10)
- Event sourcing architecture (+10)
- Not yet fully migrated to V2 architecture (some legacy code remains) (-5)
- Some packages still carry mixed concerns (-5)
- 355 files in `packages/blazecode/src/` is still being extracted to core (-10)
- Cross-cutting concerns (plugin hot-reload) not fully designed (-5)

### BlazeCode Architecture Score: 25/100

**Justification:**
- Single monolithic core crate with 95 flat public modules (-20)
- 8,575-line binary entry point with mixed concerns (-15)
- No visibility discipline (all `pub`, no `pub(crate)`) (-15)
- Infrastructure dependency in core (direct sqlx usage) (-10)
- Clean provider trait and plugin system (+10)
- Good workspace structure (5 crates) (+5, but stubs negate this)
- Plugin system is well-designed for providers (+5)
- No hexagonal architecture outside providers (-10)
- Missing V2 domain model (System Context, EventV2, Location) (-10)
- Clean code rules (forbid unsafe, no unwrap) (+5)
- No testing strategy for architecture (-5)

**Score breakdown:** 25/100 = 10 (clean rules) + 5 (workspace) + 5 (provider trait) + 5 (plugin system) - 20 (monolith) - 15 (main.rs) - 15 (public all) - 10 (infra in core) - 10 (no hex arch) - 10 (missing V2 domains) - 5 (no test strategy) = 25

---

## Summary of Critical vs High Findings

| # | Finding | Severity | Dimension |
|---|---------|----------|-----------|
| 1 | 95 flat public modules with no visibility filtering | Critical | Boundaries |
| 2 | Extreme coupling across all modules | Critical | Coupling |
| 3 | Infrastructure dependency in core (sqlx, reqwest) | Critical | Clean Arch |
| 4 | 5 crates vs 26 — insufficient modularization | Critical | Modularization |
| 5 | 8,575-line main.rs with business logic mixed in | Critical | Layering |
| 6 | No sub-module grouping for 14+ session modules | High | Cohesion |
| 7 | Missing V2 domain model (System Context, EventV2) | High | Domain Design |
| 8 | No hexagonal architecture outside provider trait | High | Hexagonal |
| 9 | 4 of 5 wrapper crates are stubs | High | Modularization |
| 10 | No database/filesystem/event-store port abstractions | High | Hexagonal |

---

## Recommended Migration Path

1. **Phase 1 (Immediate):** Add `pub(crate)` visibility to 80% of modules. Define a clean `lib.rs` re-export surface. Move logic out of `main.rs` into library crates.
2. **Phase 2 (Short-term):** Create `blazecode-core-types` for shared traits. Extract database behind a trait (`Database`). Extract HTTP client behind a trait (`HttpClient`).
3. **Phase 3 (Medium-term):** Group flat modules into sub-modules (session/, provider/, tool/). Split core into 5+ crates following BlazeCode package boundaries.
4. **Phase 4 (Long-term):** Implement V2 domain model: System Context algebra, EventV2 event sourcing, Location-scoped services. Port all 129 rules from `CONTEXT.md`.
