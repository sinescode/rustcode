# RustCode ↔ OpenCode Parity Roadmap

**Date:** 2026-06-21
**Source Data:** Feature Gap Report (agent 08), Architecture Report (agent 02), Database Report (agent 07)

---

## 1. Current Parity Assessment

| Metric | Value | Notes |
|--------|-------|-------|
| Structural parity | ~100% | All 86 modules exist as `.rs` files in `rustcode-core/src/` |
| Functional parity | ~20% | Most modules are type skeletons + traits; business logic is stubbed |
| Feature parity | ~15% | 21 OpenCode-only features have zero RustCode equivalent |
| Working features | ~5% | Config scaffold, error types, basic ID generation, partial Anthropic provider |
| Total RustCode LOC | 140,884 | 115,477 in core + 25,407 in supplementary crates (server, tui, lsp, mcp) |
| Total OpenCode LOC | 101,847 | Core + opencode packages at pinned commit `5d0f866` |

### Key Findings

1. **Structural parity is deceptive** — all module files exist but most are hollow. The `tool_impls.rs` (7,235 LOC) has function signatures for all tools but only stub implementations.
2. **The session system is the critical gap** — OpenCode's V2 Session architecture (~4,000 LOC of Effect-native state machine logic) is barely scaffolded.
3. **Provider ecosystem is 1/30+** — only Anthropic is substantially implemented; 25+ provider protocol adapters are missing.
4. **RustCode is ahead in supplementary crates** — 25,407 LOC in server, TUI, LSP, MCP show real investment (especially `rustcode-tui` at 8,190 LOC).
5. **Estimated 3.5 person-years** to reach full parity.

---

## 2. Feature Parity Matrix

### 2.1 Core Modules (86 modules)

| # | Module | Status | RustCode Eq. | Complete % | Effort | Priority |
|---|--------|--------|-------------|------------|--------|----------|
| 1 | **account** | PARTIAL | `account.rs` | 30% | Medium | P3 |
| 2 | **agent** | PARTIAL | `agent.rs` | 40% | Medium | P1 |
| 3 | **aisdk** | PARTIAL | `aisdk.rs` | 30% | Small | P2 |
| 4 | **auth** | PARTIAL | `auth.rs` | 30% | Medium | P3 |
| 5 | **background_job** | PARTIAL | `background_job.rs` | 30% | Medium | P4 |
| 6 | **bus** | PARTIAL | `bus.rs` | 60% | Small | P2 |
| 7 | **catalog** | PARTIAL | `catalog.rs` | 40% | Medium | P3 |
| 8 | **command** | PARTIAL | `command.rs` | 40% | Small | P4 |
| 9 | **config** | PARTIAL | `config.rs` | 70% | Large | P1 |
| 10 | **credential** | PARTIAL | `credential.rs` | 40% | Medium | P3 |
| 11 | **database** | PARTIAL | `database.rs` | 60% | Large | P1 |
| 12 | **env** | PARTIAL | `env.rs` | 50% | Small | P4 |
| 13 | **error** | PARTIAL | `error.rs` | 80% | Small | P2 |
| 14 | **event** | PARTIAL | `event.rs` | 40% | Large | P1 |
| 15 | **event_projector** | PARTIAL | `event_projector.rs` | 30% | Medium | P2 |
| 16 | **file_mutation** | PARTIAL | `file_mutation.rs` | 30% | Small | P4 |
| 17 | **filesystem** | PARTIAL | `filesystem.rs` | 50% | Large | P2 |
| 18 | **flag** | PARTIAL | `flag.rs` | 30% | Small | P4 |
| 19 | **flock** | PARTIAL | `flock.rs` | 50% | Small | P4 |
| 20 | **format** | PARTIAL | `format.rs` | 40% | Small | P4 |
| 21 | **fs_util** | PARTIAL | `fs_util.rs` | 50% | Small | P4 |
| 22 | **git** | PARTIAL | `git.rs` | 20% | Medium | P1 |
| 23 | **global** | PARTIAL | `global.rs` | 30% | Small | P4 |
| 24 | **id** | PARTIAL | `id.rs` | 70% | Small | P3 |
| 25 | **ide** | PARTIAL | `ide.rs` | 10% | Medium | P4 |
| 26 | **image** | PARTIAL | `image.rs` | 40% | Small | P4 |
| 27 | **installation** | PARTIAL | `installation.rs` | 30% | Small | P4 |
| 28 | **instruction_context** | PARTIAL | `instruction_context.rs` | 30% | Small | P2 |
| 29 | **integration** | PARTIAL | `integration.rs` | 30% | Medium | P3 |
| 30 | **location** | PARTIAL | `location.rs` | 30% | Medium | P3 |
| 31 | **lsp** | PARTIAL | `lsp.rs` | 20% | Large | P3 |
| 32 | **mcp** | PARTIAL | `mcp.rs` | 40% | Large | P3 |
| 33 | **mcp_oauth** | PARTIAL | `mcp_oauth.rs` | 40% | Medium | P3 |
| 34 | **model** | PARTIAL | `model.rs` | 40% | Medium | P2 |
| 35 | **npm** | PARTIAL | `npm.rs` | 30% | Medium | P4 |
| 36 | **observability** | PARTIAL | `observability.rs` | 30% | Large | P3 |
| 37 | **patch** | PARTIAL | `patch.rs` | 30% | Medium | P2 |
| 38 | **permission** | PARTIAL | `permission.rs` | 30% | Medium | P2 |
| 39 | **plugin** | PARTIAL | `plugin.rs` | 50% | Large | P2 |
| 40 | **policy** | PARTIAL | `policy.rs` | 20% | Small | P4 |
| 41 | **process** | PARTIAL | `process.rs` | 30% | Medium | P3 |
| 42 | **project** | PARTIAL | `project.rs` | 30% | Medium | P2 |
| 43 | **provider** | PARTIAL | `provider.rs` | 30% | Large | P1 |
| 44 | **provider_service** | PARTIAL | `provider_service.rs` | 20% | Small | P4 |
| 45 | **pty** | PARTIAL | `pty.rs` | 30% | Medium | P2 |
| 46 | **publish_llm_event** | PARTIAL | `publish_llm_event.rs` | 30% | Medium | P4 |
| 47 | **question** | PARTIAL | `question.rs` | 30% | Medium | P3 |
| 48 | **reference** | PARTIAL | `reference.rs` | 30% | Medium | P3 |
| 49 | **repository** | PARTIAL | `repository.rs` | 30% | Medium | P2 |
| 50 | **ripgrep** | PARTIAL | `ripgrep.rs` | 30% | Medium | P3 |
| 51 | **runtime** | PARTIAL | `runtime.rs` | 10% | Large | P1 |
| 52 | **schema** | PARTIAL | `schema.rs` | 30% | Small | P3 |
| 53 | **session** | PARTIAL | `session.rs` | 35% | Large | P1 |
| 54 | **session_compaction** | PARTIAL | `session_compaction.rs` | 30% | Medium | P2 |
| 55 | **session_epoch** | PARTIAL | `session_epoch.rs` | 30% | Medium | P2 |
| 56 | **session_execution** | PARTIAL | `session_execution.rs` | 30% | Medium | P2 |
| 57 | **session_history** | PARTIAL | `session_history.rs` | 30% | Medium | P3 |
| 58 | **session_info** | PARTIAL | `session_info.rs` | 30% | Small | P4 |
| 59 | **session_input_inbox** | PARTIAL | `session_input_inbox.rs` | 30% | Medium | P2 |
| 60 | **session_message** | PARTIAL | `session_message.rs` | 30% | Medium | P2 |
| 61 | **session_model** | PARTIAL | `session_model.rs` | 30% | Small | P2 |
| 62 | **session_projector** | PARTIAL | `session_projector.rs` | 30% | Medium | P2 |
| 63 | **session_prompt** | PARTIAL | `session_prompt.rs` | 30% | Medium | P2 |
| 64 | **session_reminders** | PARTIAL | `session_reminders.rs` | 10% | Small | P4 |
| 65 | **session_revert** | PARTIAL | `session_revert.rs` | 20% | Small | P3 |
| 66 | **session_runner** | PARTIAL | `session_runner.rs` | 20% | Large | P1 |
| 67 | **session_todo** | PARTIAL | `session_todo.rs` | 20% | Small | P4 |
| 68 | **share** | PARTIAL | `share.rs` | 20% | Medium | P4 |
| 69 | **shell** | PARTIAL | `shell.rs` | 30% | Medium | P3 |
| 70 | **shell_parser** | PARTIAL | `shell_parser.rs` | 30% | Small | P4 |
| 71 | **skill** | PARTIAL | `skill.rs` | 40% | Medium | P2 |
| 72 | **snapshot** | PARTIAL | `snapshot.rs` | 30% | Medium | P3 |
| 73 | **sse** | PARTIAL | `sse.rs` | 30% | Small | P4 |
| 74 | **state** | PARTIAL | `state.rs` | 30% | Medium | P3 |
| 75 | **storage** | PARTIAL | `storage.rs` | 30% | Medium | P3 |
| 76 | **sync** | PARTIAL | `sync.rs` | 5% | Medium | P4 |
| 77 | **system_context** | PARTIAL | `system_context.rs` | 30% | Medium | P2 |
| 78 | **tool** | PARTIAL | `tool.rs` | 40% | Large | P1 |
| 79 | **tool_impls** | PARTIAL | `tool_impls.rs` | 10% | Large | P1 |
| 80 | **tool_output_store** | PARTIAL | `tool_output_store.rs` | 20% | Small | P4 |
| 81 | **tool_stream** | PARTIAL | `tool_stream.rs` | 20% | Small | P4 |
| 82 | **truncate** | PARTIAL | `truncate.rs` | 20% | Small | P4 |
| 83 | **util** | PARTIAL | `util.rs` | 30% | Medium | P4 |
| 84 | **v2_schema** | PARTIAL | `v2_schema.rs` | 30% | Small | P3 |
| 85 | **workspace** | PARTIAL | `workspace.rs` | 30% | Medium | P2 |
| 86 | **worktree** | PARTIAL | `worktree.rs` | 30% | Medium | P3 |

### 2.2 OpenCode-Only Features (21 features, zero RustCode equivalent)

| # | Feature | OpenCode LOC | RustCode Equivalent | Priority | Effort | Strategy |
|---|---------|-------------|-------------------|----------|--------|----------|
| 1 | Console (Cloud Platform) | ~15,000 | None | P2 | Very Large (50 pd) | Build — new Rust web backend |
| 2 | Web App | ~10,000 | None | P2 | Large (25 pd) | Build — Yew/Leptos frontend |
| 3 | Desktop App | ~5,000 | None | P3 | Large (25 pd) | Build — Tauri with embedded CLI |
| 4 | VS Code Extension | ~5,000 | None | P1 | Large (25 pd) | Build — TypeScript wrapping CLI |
| 5 | TUI (Terminal UI) | ~5,000 | `rustcode-tui` (8,190 LOC) | P2 | Medium (10 pd) | Port — extend existing ratatui TUI |
| 6 | V2 Session Architecture | ~4,000 | `session*.rs` (scaffold) | P1 | Large (15 pd) | Port — translate Effect-native to tokio |
| 7 | Slack Integration | ~3,000 | None | P4 | Medium (10 pd) | Build — add to rustcode-server |
| 8 | Stats/Telemetry | ~3,000 | `observability.rs` (stub) | P3 | Medium (10 pd) | Build — Prometheus/OTLP pipeline |
| 9 | Storybook UI Library | ~3,000 | None | P4 | 0 | **Skip** — not applicable to Rust |
| 10 | Documentation Site | ~5,000 | None | P3 | Medium (10 pd) | Build — rustdoc + mdBook site |
| 11 | GitHub Copilot Integration | ~2,000 | `providers/github_copilot.rs` (stub) | P1 | Medium (10 pd) | Port — complete provider impl |
| 12 | Enterprise (Teams/SSO) | ~2,000 | None | P3 | Large (15 pd) | Build — PostgreSQL + multi-tenant |
| 13 | ACP (Agent Client Protocol) | ~2,000 | None | P2 | Medium (10 pd) | Port — translate from OpenCode |
| 14 | Plugin SDK | ~2,000 | None | P2 | Medium (10 pd) | Build — publish rustcode-sdk crate |
| 15 | EventV2 (Durable Events) | ~2,000 | `event.rs` (stub) | P1 | Large (15 pd) | Port — SQL-persisted event streams |
| 16 | LLM Package | ~2,000 | `providers/` (partial) | P1 | Medium (10 pd) | Port — complete 25+ providers |
| 17 | HTTP Recorder | ~1,000 | None | P4 | Small (3 pd) | Build — testing utility crate |
| 18 | FFF Abstraction | ~500 | `filesystem.rs` (partial) | P2 | Small (3 pd) | Port — cross-platform fs layer |
| 19 | Effect Drizzle SQLite | ~1,000 | N/A | P4 | 0 | **Skip** — not applicable (use sqlx) |
| 20 | Security Scanning | ~50 | None | P2 | Small (1 pd) | Integrate — cargo-audit + cargo-deny |
| 21 | Identity Package | ~1,000 | None | P4 | Small (3 pd) | Integrate — use existing Rust crates |

---

## 3. Missing Feature Implementation Plans

### 3.1 Console (Cloud Platform)
- **Status:** Missing
- **Strategy:** Build
- **Effort:** 10 person-weeks
- **Priority:** P2
- **Description:** OpenCode's cloud console provides billing, auth, team management, and workspace management via a web UI.
- **Plan:**
  1. Phase 1: Design API schema (2 wks)
  2. Phase 2: Implement auth/team/workspace CRUD in rustcode-server (4 wks)
  3. Phase 3: Build minimal web admin UI (2 wks) — defer full console
  4. Phase 4: Add billing integration via Stripe (2 wks)
- **Decision rationale:** Cloud platform is a differentiator but not needed for local-first RustCode MVP. Build minimal viable console after core parity is achieved.

### 3.2 Enterprise (Teams/SSO)
- **Status:** Missing
- **Strategy:** Build
- **Effort:** 3 person-weeks
- **Priority:** P3
- **Description:** Team deployment, SSO (SAML/OIDC), organization management, role-based access.
- **Plan:**
  1. Add PostgreSQL support alongside SQLite to rustcode-core/database (1 wk)
  2. Implement multi-tenant organization model (1 wk)
  3. Add SSO provider integration (OIDC, SAML) (1 wk)
- **Decision rationale:** Required for enterprise adoption. Build after core single-user parity is done.

### 3.3 Desktop App
- **Status:** Missing
- **Strategy:** Build
- **Effort:** 5 person-weeks
- **Priority:** P3
- **Description:** Electron or Tauri desktop application for non-CLI usage.
- **Plan:**
  1. Create Tauri v2 project wrapping rustcode CLI (2 wks)
  2. Build settings UI and session viewer (2 wks)
  3. Package for macOS/Windows/Linux (1 wk)
- **Decision rationale:** Tauri is superior to Electron for Rust projects. Bundle CLI as subprocess. Defer until core parity and web app are done.

### 3.4 Slack Integration
- **Status:** Missing
- **Strategy:** Build
- **Effort:** 2 person-weeks
- **Priority:** P4
- **Description:** Slack bot for interacting with RustCode via Slack messages.
- **Plan:**
  1. Add Slack API client to rustcode-server (1 wk)
  2. Implement message handler → session pipeline (0.5 wk)
  3. Add Slack event subscription and OAuth (0.5 wk)
- **Decision rationale:** Low priority, narrow use case. Build if Slack becomes primary distribution channel.

### 3.5 VS Code Extension
- **Status:** Missing
- **Strategy:** Build
- **Effort:** 5 person-weeks
- **Priority:** P1
- **Description:** VS Code extension wrapping RustCode CLI for in-editor agent interaction.
- **Plan:**
  1. Scaffold TypeScript extension with `yo code` (0.5 wk)
  2. Implement CLI subprocess launcher (1 wk)
  3. Build chat panel UI using VS Code WebView API (2 wks)
  4. Add session list, settings view, inline diff (1 wk)
  5. Package and publish to Marketplace (0.5 wk)
- **Decision rationale:** Critical for adoption — most users interact via IDE. Build in Phase 1 alongside core MVP.

### 3.6 Web App
- **Status:** Missing
- **Strategy:** Build
- **Effort:** 5 person-weeks
- **Priority:** P2
- **Description:** Full web application for browser-based RustCode usage.
- **Plan:**
  1. Choose framework: Yew vs Leptos vs SolidJS + WASM (1 wk evaluation)
  2. Build session UI: chat interface, tool output viewer, file tree (2 wks)
  3. Add settings, project management, history views (1 wk)
  4. Deploy via rustcode-server with WebSocket transport (1 wk)
- **Decision rationale:** Opens browser-based usage. Build after VS Code extension is stable.

### 3.7 Storybook
- **Status:** Missing
- **Strategy:** Skip
- **Effort:** 0
- **Priority:** P4
- **Description:** UI component library and documentation tool.
- **Decision rationale:** Storybook is a JavaScript/React tool. Rust UI components (TUI, web) use different paradigms (ratatui widgets, Yew components). Not applicable.

### 3.8 GitHub Copilot Integration
- **Status:** Missing (stub exists at `providers/github_copilot.rs`)
- **Strategy:** Port
- **Effort:** 2 person-weeks
- **Priority:** P1
- **Description:** GitHub Copilot provider implementing chat and responses API.
- **Plan:**
  1. Port OpenCode's GitHub Copilot auth flow (token exchange, device flow) (0.5 wk)
  2. Implement Copilot chat completions API adapter (1 wk)
  3. Add Copilot responses API support (0.5 wk)
- **Decision rationale:** Critical provider — many users expect Copilot integration. Build in Phase 1.

### 3.9 ACP (Agent Client Protocol)
- **Status:** Missing
- **Strategy:** Port
- **Effort:** 2 person-weeks
- **Priority:** P2
- **Description:** Agent Client Protocol for standardized agent-to-agent communication.
- **Plan:**
  1. Port OpenCode's ACP type definitions and message schemas (0.5 wk)
  2. Implement ACP server in rustcode-server (1 wk)
  3. Implement ACP client for agent orchestration (0.5 wk)
- **Decision rationale:** Enables multi-agent workflows and third-party agent integration.

### 3.10 Plugin SDK
- **Status:** Missing
- **Strategy:** Build
- **Effort:** 2 person-weeks
- **Priority:** P2
- **Description:** Published SDK crate for developing RustCode plugins (provider plugins, tool plugins, skill plugins).
- **Plan:**
  1. Define plugin trait API and ABI boundary (1 wk)
  2. Build plugin discovery and loading system (0.5 wk)
  3. Publish `rustcode-sdk` to crates.io with examples (0.5 wk)
- **Decision rationale:** Enables ecosystem growth. Build plugin system first, then publish SDK.

### 3.11 HTTP Recorder
- **Status:** Missing
- **Strategy:** Build
- **Effort:** 0.5 person-weeks
- **Priority:** P4
- **Description:** HTTP request recording utility for testing provider integrations.
- **Plan:**
  1. Build `rustcode-http-recorder` crate with `reqwest` middleware (0.3 wk)
  2. Add replay mode for deterministic testing (0.2 wk)
- **Decision rationale:** Testing utility — build when provider integration tests become painful.

### 3.12 FFF (File-File-File) Abstraction
- **Status:** Missing (partial in `filesystem.rs`)
- **Strategy:** Port
- **Effort:** 0.5 person-weeks
- **Priority:** P2
- **Description:** Cross-platform file system abstraction layer (File-File-File pattern).
- **Plan:**
  1. Port OpenCode's FFF trait definitions from `core/filesystem/fff.*.ts` (0.3 wk)
  2. Implement platform-specific backends (Unix, Windows) (0.2 wk)
- **Decision rationale:** Needed for cross-platform file operations. Integrate into existing filesystem module.

### 3.13 Effect Drizzle SQLite
- **Status:** N/A
- **Strategy:** Skip
- **Effort:** 0
- **Priority:** P4
- **Description:** Effect-native Drizzle ORM SQLite adapter.
- **Decision rationale:** Not applicable to Rust. RustCode uses `sqlx` directly with raw SQL/migrations. Effect pattern is replaced by `async fn` + `thiserror`.

### 3.14 Security Scanning
- **Status:** Missing
- **Strategy:** Integrate
- **Effort:** 0.2 person-weeks
- **Priority:** P2
- **Description:** Automated security vulnerability scanning via cargo-audit, cargo-deny, gitleaks.
- **Plan:**
  1. Add `cargo-deny` action to CI (already partially done — check `deny.toml`) (0.1 wk)
  2. Add `cargo-audit` to CI workflow (0.05 wk)
  3. Add `gitleaks` secret scanning to CI (0.05 wk)
- **Decision rationale:** Low-effort, high-value. Integrate existing Rust ecosystem tools.

### 3.15 Stats/Telemetry
- **Status:** Missing (stub in `observability.rs`)
- **Strategy:** Build
- **Effort:** 2 person-weeks
- **Priority:** P3
- **Description:** Anonymous usage statistics, performance metrics, error reporting pipeline.
- **Plan:**
  1. Implement OTLP metrics exporter in observability module (1 wk)
  2. Add structured logging pipeline with `tracing` (0.5 wk)
  3. Build opt-in telemetry with privacy controls (0.5 wk)
- **Decision rationale:** Useful for understanding usage patterns. Build when rustcode-server is deployed.

### 3.16 Documentation Site
- **Status:** Missing
- **Strategy:** Build
- **Effort:** 2 person-weeks
- **Priority:** P3
- **Description:** Documentation website with API docs, guides, examples.
- **Plan:**
  1. Generate rustdoc documentation with intra-doc links (0.5 wk)
  2. Build mdBook-based guide with getting started, configuration, examples (1 wk)
  3. Set up CI to deploy to GitHub Pages (0.5 wk)
- **Decision rationale:** Needed for developer adoption. Build when Plugin SDK and ACP are stable.

### 3.17 i18n
- **Status:** Missing
- **Strategy:** Integrate
- **Effort:** 2 person-weeks
- **Priority:** P4
- **Description:** Internationalization support for CLI output, TUI, error messages.
- **Plan:**
  1. Integrate `rust-i18n` or `fluent-rs` crate (1 wk)
  2. Extract translatable strings from codebase (0.5 wk)
  3. Provide initial translations (en, ja, zh, de) (0.5 wk)
- **Decision rationale:** Nice-to-have for global adoption. Defer post-v1.

### 3.18 API Versioning
- **Status:** Missing
- **Strategy:** Build
- **Effort:** 0.5 person-weeks
- **Priority:** P3
- **Description:** Semver policy for rustcode-core public API, CLI interface, server API.
- **Plan:**
  1. Define public API surface and semver policy document (0.3 wk)
  2. Add API compatibility tests and breaking change detection (0.2 wk)
- **Decision rationale:** Only needed when SDK/plugin ecosystem is established.

### 3.19 Multi-Platform (Web/Desktop/IDE)
- **Status:** Missing
- **Strategy:** Build
- **Effort:** Ongoing (covered by individual platform efforts above)
- **Priority:** P2
- **Description:** RustCode available on web, desktop, and IDE platforms.
- **Decision rationale:** This is a meta-feature encompassing items 3.3, 3.5, 3.6. Not a separate implementation item.

### 3.20 Cloud Deployment
- **Status:** Missing
- **Strategy:** Build
- **Effort:** 2 person-weeks
- **Priority:** P4
- **Description:** One-click cloud deployment for rustcode-server (AWS/GCP/Azure).
- **Plan:**
  1. Create Docker image for rustcode-server with SQLite/PostgreSQL (0.5 wk)
  2. Write Pulumi/Terraform deployment scripts (1 wk)
  3. Add Fly.io/DigitalOcean app spec (0.5 wk)
- **Decision rationale:** Post-v1 feature. Defer until Console (3.1) is built.

### 3.21 Nix Package
- **Status:** Missing
- **Strategy:** Build
- **Effort:** 0.5 person-weeks
- **Priority:** P3
- **Description:** Nix flake for reproducible RustCode builds and shell environments.
- **Plan:**
  1. Create `flake.nix` with `naersk` or `crane` build (0.3 wk)
  2. Add dev shell with all dependencies, CI integration (0.2 wk)
- **Decision rationale:** Important for NixOS users and reproducible builds. Low effort.

---

## 4. Effort Estimation

### 4.1 Core Modules (86 modules)

| Effort Category | Count | Per Module | Total Person-Days |
|----------------|-------|-----------|-------------------|
| Small (<1 week) | 25 | 2 pd | 50 |
| Medium (1-2 weeks) | 38 | 6 pd | 228 |
| Large (2-4 weeks) | 23 | 15 pd | 345 |
| **Total** | **86** | — | **623 pd** |

### 4.2 OpenCode-Only Features (21 features)

| Effort Category | Count | Per Feature | Total Person-Days |
|----------------|-------|-------------|-------------------|
| Small (<1 week) | 6 | 3 pd | 18 |
| Medium (1-3 weeks) | 9 | 10 pd | 90 |
| Large (1-2 months) | 5 | 25 pd | 125 |
| Very Large (2-4 months) | 1 (Console) | 50 pd | 50 |
| Skipped | 2 (Storybook, Effect Drizzle) | 0 | 0 |
| **Total** | **19 (implementable)** | — | **283 pd** |

### 4.3 Grand Total

| Category | Person-Days | Person-Months | Person-Years |
|----------|-------------|--------------|--------------|
| Core modules (86) | 623 | 29 | 2.4 |
| OpenCode-only features (19) | 283 | 13 | 1.1 |
| **Total (105 work items)** | **906** | **42** | **3.5** |

### 4.4 Cost Estimate

| Rate | Core Modules | OpenCode Features | Total |
|------|-------------|-------------------|-------|
| $150/hr, 8hr/day | $747,600 | $339,600 | **$1,087,200** |
| $200/hr, 8hr/day | $996,800 | $452,800 | **$1,449,600** |

---

## 5. Build vs Buy vs Skip Decisions

| Feature | Decision | Rationale |
|---------|----------|-----------|
| Console | **Build** — minimal viable version | Cloud platform is core to OpenCode value prop; no off-the-shelf alternative |
| Enterprise (SSO) | **Build** on top of existing crates | `axum-session`, `oauth2`, `openidconnect` crates available |
| Desktop App | **Build** with Tauri | Tauri is Rust-native; no Electron dependency needed |
| Slack Integration | **Build** minimal | Existing Rust Slack API crates (`slack-api`, `slack-morphism`) |
| VS Code Extension | **Build** from scratch | VS Code extensions are TypeScript; no Rust alternative |
| Web App | **Build** with Yew or Leptos | WASM frameworks are Rust-native; SolidJS would add JS toolchain dependency |
| Storybook | **Skip** | JS/React tool; not applicable to Rust UI components |
| GitHub Copilot | **Port** from OpenCode | Provider protocol translation; logic is already designed |
| ACP | **Port** from OpenCode | Protocol translation; design is stable |
| Plugin SDK | **Build** | Publish `rustcode-sdk` crate; no existing Rust alternative |
| EventV2 | **Port** from OpenCode | Core architectural pattern; must be faithful to original design |
| HTTP Recorder | **Build** | Small testing utility; no existing crate does exactly this |
| FFF Abstraction | **Port** from OpenCode | Small filesystem layer; logic is straightforward |
| Effect Drizzle | **Skip** | Not applicable — Rust uses `sqlx` + raw SQL |
| Security Scanning | **Integrate** | `cargo-audit` + `cargo-deny` are mature, existing Rust ecosystem tools |
| Stats/Telemetry | **Build** with `tracing` + `opentelemetry` | Excellent Rust telemetry ecosystem; integrate existing crates |
| Documentation Site | **Build** | `rustdoc` + `mdBook` are Rust-native; no extra toolchain needed |
| i18n | **Integrate** | `fluent-rs` or `rust-i18n` crates are mature |
| API Versioning | **Build** policy | Policy decision, not implementation; low code cost |
| Cloud Deployment | **Build** with Docker + Pulumi | Standard DevOps; Terraform/Pulumi support Rust natively |
| Nix Package | **Build** | Low effort; `flake.nix` with crate2nix or naersk |
| Identity Package | **Integrate** | Use `didkit` or `ssi` crate for decentralized identity |

---

## 6. Phased Implementation Plan

### Phase 0: Foundation (Weeks 1-2)
**Team: 3 engineers | Effort: 30 person-days**

**Modules:**
- `error.rs` (complete remaining variants, `std::error::Error` impls)
- `id.rs` (finalize all ID generation strategies)
- `env.rs` (add `.env` parsing, platform detection)
- `bus.rs` (complete typed event routing)
- `runtime.rs` (implement scoped tasks, cancellation, interruption)

**Dependencies:** None (bottom of dependency tree)
**Deliverable:** RustCode compiles with `#![forbid(unsafe_code)]` and zero warnings

### Phase 1: Core Session MVP (Weeks 3-6)
**Team: 4 engineers | Effort: 80 person-days**

**Modules:**
- `config.rs` (TOML/YAML/JSON parsing, env interpolation, migrations)
- `database.rs` (port all 35 migrations, schema definitions)
- `event.rs` (EventV2: durable SQL-persisted event streams)
- `session*.rs` (message types, prompt assembly, input inbox, projector)
- `provider.rs` (complete Anthropic, port primary providers: OpenAI, Gemini, Groq)
- `tool_impls.rs` (implement Read, Write, Edit, Bash, Glob, Grep, WebFetch)
- `model.rs` (request builder, parameter validation)

**Dependencies:** Phase 0
**Deliverable:** RustCode can run a basic agent session: load config → prompt LLM → execute tools → persist events

### Phase 2: Session Runner & Providers (Weeks 7-14)
**Team: 4 engineers | Effort: 160 person-days**

**Modules:**
- `session_runner.rs` (LLM interaction loop, tool-stream orchestration, interrupts)
- `session_execution.rs` (execution lifecycle, agent sub-execution)
- `session_compaction.rs` (context window management, strategy selection)
- `session_epoch.rs` (epoch lifecycle, context switching)
- `providers/*.rs` (port 10+ additional providers: Bedrock, Azure, XAI, OpenRouter, Cloudflare, etc.)
- `git.rs` (complete commit, branch, stash operations)
- `permission.rs` (arity evaluation, saved permission persistence)
- `patch.rs` (complete apply/revert with hunk-level operations)

**Dependencies:** Phase 1
**Deliverable:** Full session lifecycle with multi-provider support, context management, git integration

### Phase 3: Plugin System & IDE Integration (Weeks 15-22)
**Team: 5 engineers | Effort: 200 person-days**

**Modules:**
- `plugin.rs` (dynamic loading, boot lifecycle, provider/agent/skill plugins)
- `skill.rs` (directory scanning, guidance injection, dependency resolution)
- `system_context.rs` (builtin providers, context registry, index generation)
- `filesystem.rs` (FFF abstraction, ignore patterns, file watcher)
- `project.rs` (bootstrap, copy strategies, VCS integration)
- `workspace.rs` (workspace lifecycle, context management)
- `lsp.rs` (full LSP protocol: diagnostics, completion, hover, code actions)
- `mcp.rs` (OAuth flow, catalog management, SSE transport)
- **VS Code Extension** (build and publish)

**Dependencies:** Phase 2
**Deliverable:** Extensible plugin ecosystem with IDE integration

### Phase 4: Server, Auth & Infrastructure (Weeks 23-30)
**Team: 4 engineers | Effort: 160 person-days**

**Modules:**
- `auth.rs` (OAuth providers: GitHub, Google; device flow; token exchange)
- `account.rs` (workspace membership, org-level account linking)
- `integration.rs` (connection CRUD, OAuth flow, webhook handling)
- `repository.rs` (caching, clone logic, remote operations)
- `credential.rs` (encryption-at-rest, SQL credential CRUD)
- `rustcode-server` (HTTP/SSE API, session management, auth middleware)
- `observability.rs` (OTLP exporter, span lifecycle, metric collection)
- **Security Scanning** (CI integration with cargo-audit, cargo-deny, gitleaks)
- **API Versioning** (semver policy, compatibility tests)

**Dependencies:** Phase 3
**Deliverable:** RustCode server with auth, observability, and production infrastructure

### Phase 5: Web App & TUI Polish (Weeks 31-38)
**Team: 4 engineers | Effort: 160 person-days**

**Features:**
- **Web App** (Yew/Leptos frontend with session UI, settings, history)
- **TUI** (complete ratatui interface with session viewer, settings, file browser)
- `rustcode-tui` (finalize: chat panel, tool output, diff viewer, session list)
- **GitHub Copilot Integration** (complete provider with auth + chat/responses API)
- **ACP** (Agent Client Protocol server and client)

**Dependencies:** Phase 4
**Deliverable:** RustCode accessible via web browser and polished TUI

### Phase 6: Desktop, Enterprise & Cloud (Weeks 39-50)
**Team: 5 engineers | Effort: 300 person-days**

**Features:**
- **Desktop App** (Tauri bundle with embedded CLI, settings UI, session viewer)
- **Enterprise** (PostgreSQL support, multi-tenant, SSO/SAML/OIDC)
- **Console** (minimal web admin: billing, teams, workspace management)
- **Plugin SDK** (publish `rustcode-sdk` to crates.io)
- **Nix Package** (flake.nix with dev shell and build)
- **Cloud Deployment** (Docker image, Pulumi/Terraform scripts)
- **Documentation Site** (rustdoc + mdBook with guides and examples)
- `database.rs` (PostgreSQL driver alongside SQLite)

**Dependencies:** Phase 5
**Deliverable:** Enterprise-ready RustCode with desktop app and cloud deployment

### Phase 7: Platform Expansion & Polish (Weeks 51-70)
**Team: 3 engineers | Effort: 300 person-days**

**Features:**
- **Slack Integration** (Slack bot with session interaction)
- **HTTP Recorder** (testing utility crate)
- **i18n** (fluent-rs integration, translations)
- **Stats/Telemetry** (usage dashboard, opt-in analytics)
- **Identity Package** (decentralized identity via didkit/ssi)
- Remaining P3/P4 modules: `background_job`, `sync`, `share`, `flag`, `image`, `installation`, `npm`, `policy`, `flock`, `shell_parser`, `truncate`, `session_reminders`, `session_todo`, etc.
- Performance optimization, fuzz testing, security audit

**Dependencies:** Phase 6
**Deliverable:** Feature parity with OpenCode across all dimensions

---

## 7. Summary

| Phase | Duration | Team | Person-Days | Cumulative % | Milestone |
|-------|----------|------|-------------|--------------|-----------|
| 0 — Foundation | 2 wks | 3 | 30 | 5% | Clean compile |
| 1 — Core Session MVP | 4 wks | 4 | 80 | 15% | Basic agent session runs |
| 2 — Session & Providers | 8 wks | 4 | 160 | 35% | Full session lifecycle |
| 3 — Plugins & IDE | 8 wks | 5 | 200 | 55% | VS Code extension ships |
| 4 — Server & Auth | 8 wks | 4 | 160 | 75% | Production server ready |
| 5 — Web & TUI | 8 wks | 4 | 160 | 85% | Web app + polished TUI |
| 6 — Desktop & Enterprise | 12 wks | 5 | 300 | 95% | Desktop + enterprise features |
| 7 — Platform Expansion | 20 wks | 3 | 300 | 100% | Full parity reached |

**Key Recommendations:**

1. **Phase 0-2 is non-negotiable** — 14 weeks minimum to reach core viability
2. **Team scaling matters** — 3 engineers for Phase 0, 4 for Phase 1-2, 5 for Phase 3-4, then scale down
3. **Skip decisions are firm** — Storybook and Effect Drizzle are not applicable to Rust ecosystem
4. **VS Code extension in Phase 3** — critical for adoption, not before core works
5. **Console is the last major feature** — cloud platform adds complexity before core parity is reached
6. **Total investment: ~$1M at $150/hr** — or ~$1.4M at $200/hr
