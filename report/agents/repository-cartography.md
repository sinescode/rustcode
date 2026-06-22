# Repository Cartography Report: BlazeCode vs BlazeCode

**Agent 01 — Repository Cartographer**  
**Date:** 2026-06-21  
**BlazeCode commit:** Local workspace at `/root/blazecodesport/blazecode`  
**BlazeCode commit:** Local workspace at `/root/blazecodesport/blazecode`  

---

## 1. Repository Overview Table

| Metric | BlazeCode | BlazeCode |
|---|---|---|
| **Primary Language** | Rust (edition 2021) | TypeScript (5.8) |
| **Total Files** | 181 `.rs` files | ~2,610 `.ts`/`.tsx` files |
| **Total LOC** | 171,409 | ~511,835 |
| **Source LOC** | ~168,000 (excl. build artifacts) | ~191,048 (non-generated) |
| **Crates/Packages** | 6 cargo workspace members | 29 npm workspaces across 4 groups |
| **Build System** | Cargo workspace (resolver v2) | Turbo v2.8.13 + Bun 1.3.14 |
| **Central Library** | `blazecode-core` (101 files, 126,855 LOC, 74%) | `@blazecode-ai/core` (largest package) |
| **Binary / CLI Crate** | `blazecode` (`src/main.rs`, 8,575 LOC) | `blazecode` (`packages/blazecode/`) |
| **Public Modules** | 85 `pub mod` in blazecode-core | ~350 source files across blazecode |
| **Test Count** | 2,655 `#[test]` annotations | ~553 `.test.*` files |
| **Largest File(s)** | `main.rs` 8,575, `tool_impls.rs` 7,235, `plugin.rs` 6,236 | `types.gen.ts` 11,271, `sdk.gen.ts` 6,836, `icons/index.tsx` 4,454 |
| **Infrastructure Targets** | Linux/macOS (native binary) | Cloudflare Workers (SST v4), AWS, desktop (Electron) |
| **Database** | SQLite via sqlx (raw SQL) | SQLite via Drizzle ORM + Effect, PlanetScale (console/stats) |
| **UI Framework** | ratatui + crossterm (TUI only) | OpenTUI (terminal), SolidJS (web), SolidJS/Vite (app) |
| **Effect System** | `tokio` async + `Arc<RwLock<>>` shared state | Effect v4 beta (functional effects, layers, fibers) |
| **LLM Integration** | Custom trait-based provider model | AI SDK providers + custom Effect-native layer |
| **LSP Server** | Built-in (`blazecode-lsp`, 3,099 LOC) | External (separate binary in SDK) |
| **MCP Support** | Built-in (`blazecode-mcp`, 1,774 LOC) | Built-in (`packages/blazecode/src/mcp/`) |
| **Auth** | API key env vars + server password | OAuth (OpenAuth), API keys, SSO, GitHub device flow |
| **HTTP Framework** | Axum 0.8 | Hono 4.10 |
| **ORM** | sqlx (raw SQL) | Drizzle ORM + Effect SQL |
| **Tree-sitter** | tree-sitter-bash for shell parsing | web-tree-sitter for bash/PowerShell |

---

## 2. Module Graph

### 2A. BlazeCode — Complete Module Tree (`blazecode-core`)

All 85 public modules in `blazecode-core`, organized by domain:

```
blazecode-core (src/lib.rs)
│
├── CORE INFRASTRUCTURE
│   ├── lib.rs              — crate root, re-exports
│   ├── error.rs            — unified Error type (thiserror)
│   ├── id.rs               — branded ID generation (prefix-based ascending IDs)
│   ├── env.rs              — environment variable helpers
│   ├── flag.rs             — feature flag resolution
│   ├── format.rs           — text formatting utilities
│   ├── util.rs             — general utility functions
│   ├── fs_util.rs          — filesystem utility functions
│   └── global.rs           — global path derivation (XDG)
│
├── CONFIGURATION
│   ├── config.rs           — Config struct, V2ConfigInfo, TOML parsing
│   ├── model.rs            — ModelInfo, ModelRequest, model catalog types
│   └── policy.rs           — policy rules engine
│
├── DATABASE & STORAGE
│   ├── database.rs         — SQLite: tables, columns, migrations, paths
│   ├── storage.rs          — JSON file-based key-value store + SQLite pool
│   ├── snapshot.rs         — snapshot/restore for session state
│   ├── schema.rs           — schema types for session messages
│   ├── v2_schema.rs        — V2 session message schema
│   └── state.rs            — application state management
│
├── EVENT SYSTEM
│   ├── event.rs            — EventV2: event sourcing, pub/sub, replay
│   ├── event_projector.rs  — event projector logic
│   ├── session_projector.rs— session-specific projectors
│   ├── publish_llm_event.rs— LLM event publishing
│   └── bus.rs              — event bus (tokio::broadcast)
│
├── PROVIDERS (LLM)
│   ├── provider.rs         — Provider trait, ChatMessage, Usage, LlmEvent
│   ├── provider_service.rs — ProviderCatalog, model resolution
│   ├── providers/mod.rs    — auto_detect_all()
│   ├── providers/anthropic.rs        — Anthropic Messages API
│   ├── providers/openai.rs           — OpenAI Chat Completions
│   ├── providers/openai_responses.rs — OpenAI Responses API
│   ├── providers/openai_compatible.rs— Generic OpenAI-compatible (DeepSeek, Groq, etc.)
│   ├── providers/gemini.rs           — Google Gemini
│   ├── providers/openrouter.rs       — OpenRouter
│   ├── providers/bedrock.rs          — AWS Bedrock (Chat Completions bridge)
│   ├── providers/bedrock_converse.rs — AWS Bedrock Converse (native)
│   ├── providers/azure.rs            — Azure OpenAI
│   ├── providers/cloudflare.rs       — Cloudflare Workers AI
│   ├── providers/xai.rs              — xAI Grok
│   ├── providers/github_copilot.rs   — GitHub Copilot token exchange
│   ├── providers/chat_completions.rs — Generic Chat Completions wire protocol
│   └── aisdk.rs            — AI SDK compatibility layer
│
├── SESSION MANAGEMENT
│   ├── session.rs          — SessionManager, Session lifecycle
│   ├── session_runner.rs   — V2 turn orchestration, stream/iterate loop
│   ├── session_execution.rs— RunCoordinator, Demand, DrainFn, RunError
│   ├── session_message.rs  — session message types
│   ├── session_info.rs     — session metadata / info
│   ├── session_model.rs    — model resolution for sessions
│   ├── session_history.rs  — ContextEpoch, history management, input delivery
│   ├── session_prompt.rs   — SessionPromptBuilder, PromptPart
│   ├── session_epoch.rs    — EpochManager
│   ├── session_compaction.rs— session compaction / summarization
│   ├── session_input_inbox.rs— user input inbox
│   ├── session_todo.rs     — todo items within sessions
│   ├── session_reminders.rs— session reminders
│   └── session_revert.rs   — session revert logic
│
├── TOOL SYSTEM
│   ├── tool.rs             — Tool trait, ToolContext, ToolRegistry
│   ├── tool_impls.rs       — all built-in tool implementations (18+ tools)
│   ├── tool_output_store.rs— tool output storage
│   └── tool_stream.rs      — streaming tool output
│
├── FILESYSTEM & CODE
│   ├── filesystem.rs       — file read/write operations
│   ├── file_mutation.rs    — file mutation tracking
│   ├── patch.rs            — unified diff/patch application
│   ├── ripgrep.rs          — ripgrep integration for search
│   ├── git.rs              — git operations
│   ├── location.rs         — location-aware file resolution
│   ├── repository.rs       — repository management (clone/fetch)
│   └── worktree.rs         — worktree management
│
├── PROCESS/SHELL
│   ├── pty.rs              — PTY/reverse-PTY terminal multiplexer
│   ├── process.rs          — process spawning
│   ├── shell.rs            — shell command resolution
│   ├── shell_parser.rs     — shell parser (tree-sitter-bash)
│   └── command.rs          — command dispatch
│
├── PERMISSION & AUTH
│   ├── permission.rs       — permission service (ask/allow/deny)
│   ├── credential.rs       — credential storage
│   ├── auth.rs             — authentication
│   ├── account.rs          — account management
│   └── integration.rs      — integration (SSH, GitHub, etc.)
│
├── PLUGIN SYSTEM
│   ├── plugin.rs           — plugin loading, management, lifecycle
│   └── npm.rs              — npm package resolution for plugins
│
├── MCP (Model Context Protocol)
│   ├── mcp.rs              — MCP client/server implementation
│   └── mcp_oauth.rs        — MCP OAuth flow
│
├── LSP
│   ├── lsp.rs              — LSP integration in core
│   └── (blazecode-lsp crate)— standalone LSP server (3,099 LOC)
│
├── SKILLS & AGENTS
│   ├── skill.rs            — skill discovery, loading, guidance
│   ├── agent.rs            — AgentService, agent definitions
│   ├── instruction_context.rs— system instruction context
│   └── system_context.rs   — system context builder
│
├── NETWORKING
│   ├── sse.rs              — SSE client (EventSource)
│   ├── reference.rs        — external reference resolution
│   └── share.rs            — session sharing via URL
│
├── BACKGROUND JOBS
│   ├── background_job.rs   — async background job scheduler
│   ├── sync.rs             — sync service
│   └── catalog.rs          — provider/model catalog
│
├── IMAGE PROCESSING
│   └── image.rs            — image handling/processing
│
├── TRUNCATION
│   ├── truncate.rs         — token-aware truncation
│   └── tool_output_store.rs— truncation-aware output store
│
├── OBSERVABILITY
│   ├── observability.rs    — OpenTelemetry tracing
│   └── flock.rs            — file locking
│
├── IDE INTEGRATION
│   ├── ide.rs              — IDE detection/lookup
│   ├── installation.rs     — installation channel management
│   ├── project.rs          — project metadata
│   └── workspace.rs        — workspace management
│
├── QUESTION HANDLING
│   └── question.rs         — user question service
│
└── TEMPLATING
    └── runtime.rs          — shared runtime initialization (wires everything together)
```

### Crate Dependency Graph

```
┌─────────────────────────────────────────────────────────────┐
│                        blazecode (bin)                        │
│                   src/main.rs — 8,575 LOC                    │
│  CLI dispatch: ACP, MCP, TUI, Attach, Run, Console, ...    │
└────────┬──────────┬──────────┬──────────┬──────────┬────────┘
         │          │          │          │          │
         ▼          ▼          ▼          ▼          ▼
┌──────────────┐ ┌──────────┐ ┌──────────┐ ┌──────────┐ ┌──────────┐
│ blazecode-core │ │blazecode- │ │blazecode- │ │blazecode- │ │blazecode- │
│  (lib)        │ │ server   │ │   tui    │ │   lsp    │ │   mcp    │
│  101 files    │ │ 42 files │ │ 25 files │ │ 1 file   │ │ 1 file   │
│  126,855 LOC  │ │ 9,282 LOC│ │17,824 LOC│ │ 3,099 LOC│ │ 1,774 LOC│
└──────┬────────┘ └────┬─────┘ └────┬─────┘ └─────┬─────┘ └────┬─────┘
       │               │            │             │            │
       │◄──────────────┘◄───────────┘◄────────────┘◄───────────┘
       │            All depend on blazecode-core
       ▼
┌──────────────────────────────────────────────────────────────────┐
│                   External Dependencies                           │
├──────────────────────────────────────────────────────────────────┤
│ HTTP:     reqwest, axum, tower, tower-http, tokio-tungstenite    │
│ DB:       sqlx (SQLite), tempfile                                │
│ CLI:      clap, dialoguer, indicatif                             │
│ AI/LLM:   (custom trait Provider, no external AI SDK)            │
│ Parse:    tree-sitter, tree-sitter-bash, serde, schemars         │
│ Async:    tokio, tokio-stream, tokio-util, futures               │
│ Diff:     similar                                                │
│ Search:   ignore, glob, walkdir, ripgrep                         │
│ TUI:      ratatui, crossterm                                     │
│ Image:    image                                                  │
│ Crypto:   sha2, hmac, base64, hex, uuid                         │
│ Time:     chrono                                                 │
│ FS:       notify, dirs                                           │
│ Tracing:  tracing, tracing-subscriber, tracing-appender          │
│ Error:    thiserror, anyhow                                      │
│ Data:     serde_json, serde_yaml, toml                           │
└──────────────────────────────────────────────────────────────────┘
```

### 2B. BlazeCode — Package Dependency Graph

```
┌──────────────────────────────────────────────────────────────────────┐
│                        BLAZECODE MONOREPO                             │
│                 ~190,520 LOC TypeScript, 29 packages                 │
├──────────────────────────────────────────────────────────────────────┤
│                                                                      │
│  ┌─────────────────────────────────────────────────────────────────┐ │
│  │                     CORE LAYER (8 packages)                     │ │
│  ├─────────────────────────────────────────────────────────────────┤ │
│  │                                                                  │ │
│  │  @blazecode-ai/core ─── central library (session, DB, tools,     │ │
│  │      │                providers, events, filesystem, etc.)       │ │
│  │      ├── @blazecode-ai/effect-drizzle-sqlite (Drizzle ORM glue)   │ │
│  │      ├── @blazecode-ai/effect-sqlite-node (SQLite node binding)   │ │
│  │      ├── @blazecode-ai/llm (provider abstractions, protocols)     │ │
│  │      └── @blazecode-ai/plugin (plugin system)                     │ │
│  │                                                                  │ │
│  │  @blazecode-ai/cli (CLI command definitions, shared lib)          │ │
│  │  @blazecode-ai/http-recorder (HTTP recording for tests)          │ │
│  │                                                                  │ │
│  └─────────────────────────────────────────────────────────────────┘ │
│                                    │                                  │
│  ┌─────────────────────────────────────────────────────────────────┐ │
│  │                  APPLICATIONS (4 packages)                      │ │
│  ├─────────────────────────────────────────────────────────────────┤ │
│  │                                                                  │ │
│  │  blazecode ───── CLI binary + server + LSP + MCP + provider init │ │
│  │      │          350 TS source files                              │ │
│  │      ├── @blazecode-ai/tui ──── OpenTUI terminal UI               │ │
│  │      ├── @blazecode-ai/ui ───── SolidJS component library         │ │
│  │      ├── @blazecode-ai/web ──── SolidJS web app                   │ │
│  │      ├── @blazecode-ai/app ──── SolidJS/Vite SPA                  │ │
│  │      └── @blazecode-ai/desktop ── Electron desktop shell          │ │
│  │                                                                  │ │
│  └─────────────────────────────────────────────────────────────────┘ │
│                                    │                                  │
│  ┌─────────────────────────────────────────────────────────────────┐ │
│  │            INFRASTRUCTURE (5 packages)                          │ │
│  ├─────────────────────────────────────────────────────────────────┤ │
│  │                                                                  │ │
│  │  @blazecode-ai/server ─── Hono HTTP server                       │ │
│  │  @blazecode-ai/slack ──── Slack bot integration                  │ │
│  │  @blazecode-ai/enterprise ── Enterprise SSO/auth                 │ │
│  │  @blazecode-ai/function ──── Function/sidecar runners            │ │
│  │  @blazecode-ai/script ───── Script execution engine              │ │
│  │                                                                  │ │
│  └─────────────────────────────────────────────────────────────────┘ │
│                                    │                                  │
│  ┌─────────────────────────────────────────────────────────────────┐ │
│  │            CONSOLE SUITE (5 packages, PlanetScale)              │ │
│  ├─────────────────────────────────────────────────────────────────┤ │
│  │                                                                  │ │
│  │  @blazecode-ai/console/app ── Console web frontend               │ │
│  │  @blazecode-ai/console/core ── Console business logic            │ │
│  │  @blazecode-ai/console/resource ── Resource management           │ │
│  │  @blazecode-ai/console/function ── Console function runner       │ │
│  │  @blazecode-ai/console/mail ── Email service                     │ │
│  │  @blazecode-ai/console/support ── Support ticket system          │ │
│  │                                                                  │ │
│  └─────────────────────────────────────────────────────────────────┘ │
│                                    │                                  │
│  ┌─────────────────────────────────────────────────────────────────┐ │
│  │            STATS SUITE (3 packages, PlanetScale)                │ │
│  ├─────────────────────────────────────────────────────────────────┤ │
│  │                                                                  │ │
│  │  @blazecode-ai/stats/app ──── Analytics dashboard (SolidJS)      │ │
│  │  @blazecode-ai/stats/core ─── Analytics data models              │ │
│  │  @blazecode-ai/stats/server ── Analytics API server              │ │
│  │                                                                  │ │
│  └─────────────────────────────────────────────────────────────────┘ │
│                                    │                                  │
│  ┌─────────────────────────────────────────────────────────────────┐ │
│  │            SDK & DEVTOOLS (4 packages)                          │ │
│  ├─────────────────────────────────────────────────────────────────┤ │
│  │                                                                  │ │
│  │  @blazecode-ai/sdk/js ──── JavaScript SDK (generated)            │ │
│  │  sdks/vscode ──────────── VS Code extension                     │ │
│  │  @blazecode-ai/storybook ── UI component storybook               │ │
│  │  @blazecode-ai/blazecode ── (the main package, in app layer)      │ │
│  │                                                                  │ │
│  └─────────────────────────────────────────────────────────────────┘ │
│                                                                      │
│              External Dependencies (grouped by category)             │
├──────────────────────────────────────────────────────────────────────┤
│                                                                      │
│  HTTP: Hono, @blazecode-ai/server,               BlazeCode equiv:     │
│        SSE event stream                              reqwest + axum  │
│                                                                      │
│  DB:   Drizzle ORM, Effect SQL,                    BlazeCode equiv:  │
│        @effect/sql-sqlite-bun                        sqlx (SQLite)   │
│        drizzle-kit                                   raw SQL         │
│                                                                      │
│  AI:   @ai-sdk/* (17 providers)                    BlazeCode equiv:  │
│        @blazecode-ai/llm                              custom Provider │
│        @agentclientprotocol/sdk                       trait system   │
│                                                                      │
│  CLI:  yargs, @clack/prompts                       BlazeCode equiv:  │
│        @opentui/core                                 clap + ratatui  │
│                                                                      │
│  UI:   SolidJS, OpenTUI, Kobalte,                  BlazeCode equiv:  │
│        TanStack Virtual, Tailwind CSS                ratatui (TUI)   │
│                                                                      │
│  Auth: @openauthjs/openauth                          BlazeCode: none │
│        @aws-sdk/credential-providers                 (env var based) │
│        Google Auth Library                                           │
│                                                                      │
│  Parse: marked (markdown), shiki (syntax highlight)  BlazeCode: none │
│         htmlparser2, turndown, gray-matter           (tree-sitter)  │
│                                                                      │
│  Cloud: SST v4, Cloudflare Workers, AWS S3          BlazeCode: none  │
│         @sentry/solid, OpenTelemetry                  tokio + tracing│
│                                                                      │
│  Effect: effect (v4 beta)                           BlazeCode equiv: │
│          @effect/platform-node                        tokio async    │
│          @effect/opentelemetry                        Arc<RwLock>    │
│                                                                      │
└──────────────────────────────────────────────────────────────────────┘
```

---

## 3. Crate/Package Dependency Graph

### BlazeCode — Full Dependency Graph

```
┌──────────┐    ┌──────────┐    ┌──────────┐    ┌──────────┐    ┌──────────┐
│ blazecode  │    │blazecode- │    │blazecode- │    │blazecode- │    │blazecode- │
│  (bin)    │    │ server   │    │   tui    │    │   lsp    │    │   mcp    │
└─────┬─────┘    └─────┬─────┘    └─────┬─────┘    └─────┬─────┘    └─────┬─────┘
      │                │                │                │                │
      └────────┬───────┴───────┬────────┴───────┬────────┴───────┬────────┘
               │               │                │               │
               ▼               ▼                ▼               ▼
        ┌────────────────────────────────────────────────────────────┐
        │                     blazecode-core                           │
        │  101 files, 126,855 LOC — the sun everything orbits        │
        └────────────────────────────────────────────────────────────┘
                        │
        ┌───────────────┼───────────────────┐
        ▼               ▼                   ▼
┌──────────────┐ ┌──────────────┐ ┌──────────────────┐
│  Internal    │ │  External    │ │  Dev/Tests        │
│  re-exports  │ │  crates.io   │ │  (temp-env, etc)  │
└──────────────┘ └──────────────┘ └──────────────────┘

Internal module interdependencies (key paths):

  runtime.rs ──► bus.rs, database.rs, session_runner.rs,
  │               provider_service.rs, tool.rs, permission.rs,
  │               question.rs, agent.rs, background_job.rs
  │
  session_runner.rs ──► session_epoch.rs, session_history.rs,
  │                      session_prompt.rs, session_input_inbox.rs,
  │                      session_compaction.rs, session_execution.rs,
  │                      session_info.rs
  │
  tool_impls.rs ──► tool.rs, filesystem.rs, ripgrep.rs,
  │                  patch.rs, git.rs, shell.rs, pty.rs,
  │                  agent.rs, question.rs, skill.rs,
  │                  lsp.rs, repository.rs, glob, web_fetch (via reqwest)
  │
  provider.rs ◄──► providers/*.rs, provider_service.rs
  │
  plugin.rs ──► npm.rs, config.rs, credential.rs
  │
  config.rs ◄──► model.rs, policy.rs, flag.rs
  │
  event.rs ──► database.rs, bus.rs
  │
  database.rs ──► storage.rs, snapshot.rs
```

### BlazeCode — Package Dependency Graph

```
                    ┌──────────────────────────────────────────────────┐
                    │                   blazecode (CLI)                  │
                    │  350 TS files — CLI dispatch, server, session    │
                    └────┬─────────┬──────────┬──────────┬────────────┘
                         │         │          │          │
          ┌──────────────┼─────────┼──────────┼──────────┼──────────────┐
          ▼              ▼         ▼          ▼          ▼              ▼
   ┌──────────┐  ┌──────────┐ ┌──────────┐ ┌──────────┐ ┌──────────┐ ┌──────────┐
   │@oc/core  │  │@oc/llm   │ │@oc/tui   │ │@oc/plugin│ │@oc/      │ │@oc/      │
   │(session, │  │(provider │ │(OpenTUI) │ │(plugin   │ │server    │ │cli       │
   │  tools,  │  │ abstract)│ │          │ │  loader) │ │(Hono)    │ │(cmds)    │
   │  DB, FS) │  │          │ │          │ │          │ │          │ │          │
   └────┬─────┘  └────┬─────┘ └──────────┘ └──────────┘ └──────────┘ └──────────┘
        │             │
        └──────┬──────┘
               ▼
        ┌──────────────┐
        │@oc/effect-   │
        │drizzle-sqlite│
        │ +            │
        │effect-sqlite-│
        │node          │
        └──────────────┘

Production packages (non-core):
  @oc/app ──► @oc/core, @oc/ui, SolidJS
  @oc/web ──► @oc/core, @oc/ui, SolidJS
  @oc/desktop ──► @oc/app, Electron
  @oc/slack ──► @oc/core
  @oc/enterprise ──► @oc/core, @oc/console/core
  @oc/function ──► @oc/core

Infrastructure deps:
  console/app ──► console/core, console/resource, console/function
  stats/app ──► stats/core, stats/server
```

---

## 4. Import Graph

### BlazeCode — Top-Level Import Patterns

**Binary (`main.rs`):**
```
clap::Parser, clap::Subcommand       ← CLI parsing
blazecode_core::*                     ← everything
blazecode_core::config::Config
blazecode_core::database::*
tokio::*                             ← async runtime
tracing::*                           ← logging
serde_json::*                        ← JSON
dirs, chrono, uuid                   ← utilities
sqlx::*                              ← DB
dialoguer, indicatif                 ← CLI UI
blazecode_tui::*                      ← TUI entry
```

**Each provider module imports:**
```
crate::provider::{Provider, ChatMessage, ...}   ← trait + types
serde::{Serialize, Deserialize}                 ← wire format
reqwest::Client                                 ← HTTP
tokio_stream::StreamExt                         ← SSE streaming
tracing                                         ← telemetry
```

**Session runner imports (pattern):**
```
crate::session_epoch::EpochManager
crate::session_history::ContextEpoch
crate::session_prompt::SessionPromptBuilder
crate::session_compaction::SessionCompaction
crate::session_input_inbox::SessionInputInbox
crate::session_execution::RunCoordinator
crate::provider::*                              ← LLM types
crate::tool::ToolRegistry                       ← tool execution
crate::agent::AgentService
```

**Tool implementations import:**
```
crate::tool::{Tool, ToolContext, ToolRegistry}
crate::filesystem::*
crate::ripgrep::*
crate::shell::*                                 ← bash tool
crate::pty::*                                   ← pty terminal
crate::git::*
crate::patch::*
crate::agent::*
crate::question::*
crate::skill::*
crate::lsp::*                                   ← LSP tool
crate::repository::*
reqwest                                          ← web_fetch/web_search
```

### BlazeCode — Top-Level Import Patterns

**Core (`@blazecode-ai/core`):**
```
effect                        ← Effect<T, E, R> everywhere
drizzle-orm                   ← DB queries
@blazecode-ai/effect-drizzle-sqlite
@blazecode-ai/llm              ← provider types
@ai-sdk/provider              ← LLM SDK types
zod                           ← validation
```

**CLI (`blazecode/src`):**
```
effect                        ← Effect-based commands
yargs                         ← CLI parsing
@blazecode-ai/core             ← core services
@blazecode-ai/cli              ← shared CLI utils
@blazecode-ai/llm              ← provider resolution
```

**TUI (`@blazecode-ai/tui`):**
```
@opentui/core                 ← TUI framework
solid-js                      ← reactive UI
@blazecode-ai/core             ← core services
@tanstack/solid-virtual       ← virtual list
```

---

## 5. Runtime Graph

### BlazeCode — Execution Flow

```
STARTUP
  │
  ▼
main.rs: main()
  │
  ├── tracing_subscriber::init()           ← logging setup
  ├── Config::load()                       ← TOML config parsing
  ├── database_path()                      ← XDG path resolution
  ├── initialize_runtime()                 ← WIRES EVERYTHING
  │     ├── Bus::new()                     ← event bus
  │     ├── DatabaseService::open()        ← SQLite pool + migrations
  │     ├── SessionManager::new()          ← session lifecycle
  │     ├── ToolRegistry::new()            ← tool registration
  │     │     └── register_builtins()      ← 18+ built-in tools
  │     ├── PermissionService::new()
  │     ├── QuestionService::new()
  │     ├── AgentService::new()
  │     └── BackgroundJobService::new()
  │
  ├── clap::Cli::parse()                   ← COMMAND DISPATCH
  │     │
  │     ├── Commands::Run     → run_handler()
  │     │     └── SessionRunner::run(prompt)
  │     │           ├── SessionPromptBuilder::build()
  │     │           ├── Provider::stream()
  │     │           ├── tool execution loop (up to 25 iterations)
  │     │           │     ├── ToolRegistry::execute()
  │     │           │     │     ├── BashTool::run()
  │     │           │     │     ├── ReadTool::run()
  │     │           │     │     ├── WriteTool::run()
  │     │           │     │     ├── EditTool::run()
  │     │           │     │     ├── GlobTool::run()
  │     │           │     │     ├── GrepTool::run()
  │     │           │     │     ├── WebFetchTool::run()
  │     │           │     │     └── ... (18 tools)
  │     │           │     └── truncate_output()
  │     │           └── output → stdout/TUI
  │     │
  │     ├── Commands::Tui     → tui_handler()
  │     │     └── TuiApp::run()
  │     │           ├── SseClient::connect()   ← SSE event stream
  │     │           ├── ratatui event loop
  │     │           └── component rendering
  │     │
  │     ├── Commands::Serve   → serve_handler()
  │     │     └── build_router() → axum::serve()
  │     │           ├── 30 API route groups
  │     │           ├── SSE endpoint (GET /event)
  │     │           ├── WebSocket endpoint
  │     │           └── CORS + compression middleware
  │     │
  │     ├── Commands::Mcp     → mcp_handler()
  │     │     └── mcp::list/connect/call
  │     │
  │     ├── Commands::Acp     → acp_handler()
  │     ├── Commands::Console → console_handler()
  │     ├── Commands::Agent   → agent_handler()
  │     ├── Commands::Plug    → plugin_handler()
  │     ├── Commands::Auth    → auth_handler()
  │     ├── Commands::Rg      → ripgrep_handler()
  │     ├── Commands::Session → session_handler()
  │     ├── Commands::Db      → db_handler()
  │     ├── Commands::Import  → import_handler()
  │     ├── Commands::Export  → export_handler()
  │     ├── Commands::Stats   → stats_handler()
  │     ├── Commands::Config  → config_handler()
  │     ├── Commands::Version → version_handler()
  │     └── Commands::Completion → completion_handler()
  │
  └── shutdown (graceful via tokio::signal)
```

### BlazeCode — Execution Flow

```
STARTUP (bun run packages/blazecode/src/index.ts)
  │
  ├── Effect.runMain(pipe(
  │     Effect.provide(Layer...),     ← Effect Layer composition
  │     Effect.flatMap(mainLogic)
  │   ))
  │
  ├── Config / env loading via Effect Config
  │
  ├── Database layer (Drizzle + Effect SQL)
  │
  ├── CLI dispatch (yargs + Effect cmd wrappers)
  │
  └── Same logical flow: run → session → LLM → tools → output
      But with:
      - Effect fibers for concurrency
      - Layer-based dependency injection
      - Structured concurrency with Scopes/Scopes
```

---

## 6. Database Schema Graph

### BlazeCode — SQLite Database (sqlx, raw SQL)

**Tables (18 + migration tracking):**

```
┌─────────────────────────────────────────────────────────────────────┐
│                      blazecode SQLite Schema                          │
├─────────────────────────────────────────────────────────────────────┤
│                                                                      │
│  workspace ──┬── project_id ──► project(id) ON DELETE CASCADE       │
│  │            │                                                       │
│  ├── id (TEXT PK)         ├── id (TEXT PK)                          │
│  ├── type (TEXT)          ├── directory (TEXT)                      │
│  ├── name (TEXT)          └── ...                                    │
│  ├── branch (TEXT)                                                   │
│  └── directory (TEXT)     project_directory ──┬── project_id ──►    │
│                                                  project(id)        │
│  session ──┬── project_id ──► project(id)                           │
│  │         │                                                         │
│  ├── id (TEXT PK)         permission ──┬── project_id ──► project   │
│  ├── model (TEXT)         │            └── project(id) ON DELETE    │
│  ├── provider (TEXT)      │                CASCADE                   │
│  ├── status (TEXT)         │                                         │
│  └── ...                   ├── id (TEXT PK)                          │
│                            ├── action (TEXT)                         │
│  session_message ──┬──     ├── resource (TEXT)                       │
│    session_id ──►  │       └── ...                                   │
│    session(id)     │                                                  │
│  (event-sourced)    │      event_sequence                            │
│                    │      ├── aggregate_id (TEXT PK)                 │
│  session_input     │      └── seq (INTEGER)                          │
│  ├── (inbox for    │                                                  │
│  │   user input)   │      event ──┬── aggregate_id ──►               │
│  │                 │      │       event_sequence(id)                 │
│  session_context_epoch           ON DELETE CASCADE                    │
│  │ (epoch tracking) ├── id (TEXT PK)                                 │
│  │                  ├── aggregate_id (TEXT NOT NULL)                  │
│  session_share      ├── seq (INTEGER NOT NULL)                       │
│  │ (shared URLs)    ├── type (TEXT NOT NULL)                         │
│  │                  └── data (TEXT NOT NULL)                          │
│  todo ──┬──                                                           │
│  │     session_id ──► session(id)    data_migration                  │
│  │                                   ├── name (TEXT PK)              │
│  account                             └── time_completed (INTEGER)    │
│  ├── id (TEXT PK)                                                     │
│  ├── email (TEXT)                   migration (SQL journal)          │
│  ├── url (TEXT)                     ├── id (INTEGER PK)              │
│  ├── access_token (TEXT)            ├── name (TEXT)                  │
│  └── ...                             ├── applied_at (TEXT)           │
│                                      └── checksum (TEXT)             │
│  control_account                                                      │
│  ├── email (TEXT PK)                  message ── (legacy)            │
│  ├── url (TEXT PK)                    part ──── (legacy)             │
│  └── ...                                                              │
│                                                                      │
│  account_state                                                       │
│  ├── id (INTEGER PK)                                                 │
│  ├── active_account_id ──► account(id) ON DELETE SET NULL           │
│  └── active_org_id                                                   │
│                                                                      │
│  credential                                                          │
│  ├── id (TEXT PK)                                                    │
│  ├── integration_id (TEXT)                                           │
│  ├── label (TEXT)                                                    │
│  └── value (TEXT)     ← encrypted                                    │
│                                                                      │
└─────────────────────────────────────────────────────────────────────┘

Migration path:
  database.rs → CONNECTION_PRAGMAS → migration table check →
  apply pending migrations → ready
  (35+ SQL migrations, tracked in migration table)
```

### BlazeCode — SQLite Database (Drizzle ORM + Effect SQL)

**Same 18-table schema** (port was based on BlazeCode commit `5d0f8660`) **plus:**

```
Additional tables/infrastructure in BlazeCode:
  - Console/Stats: PlanetScale MySQL (separate schema)
  - Enterprise: additional org/team tables
  - Control plane: instance registry, usage tracking

Migration framework:
  drizzle-kit (code-first migrations)
  Manual SQL migrations in packages/core/src/database/migration/
  Auto-generated schema in schema.gen.ts
  Data migrations in data-migration.sql.ts
```

---

## 7. Network Architecture

### BlazeCode

```
┌──────────────────────────────────────────────────────────────────────────────┐
│                          BLAZECODE NETWORK ARCHITECTURE                        │
├──────────────────────────────────────────────────────────────────────────────┤
│                                                                               │
│  ┌──────────────┐    HTTP/SSE      ┌──────────────────┐                      │
│  │  TUI Client   │◄──────────────►│  blazecode-server  │                      │
│  │  (ratatui)    │    WebSocket    │  (axum 0.8)      │                      │
│  └──────────────┘                 │  30 API routes    │                      │
│                                   │  SSE /event       │                      │
│  ┌──────────────┐    HTTP/SSE     │  WSS /ws          │                      │
│  │  IDE Plugin   │◄──────────────►│  MCP SSE /mcp/*   │                      │
│  │  (LSP)        │                │  ACP agent protocol│                     │
│  └──────────────┘                 └─────────┬─────────┘                      │
│                                             │                                 │
│  ┌──────────────┐                           ▼                                 │
│  │  External     │◄─────────────────────────────────────────┐                │
│  │  LLM APIs     │                                          │                │
│  │  (Anthropic,  │      ┌─────────────────────────┐         │                │
│  │   OpenAI,     │◄─────│   Provider (reqwest)     │         │                │
│  │   Gemini,     │      └─────────────────────────┘         │                │
│  │   Bedrock...) │                                          │                │
│  └──────────────┘                                          │                │
│                                                              │                │
│  ┌──────────────┐             ┌───────────────────┐          │                │
│  │  MCP Servers  │◄──────────►│  blazecode-mcp      │──────────┘                │
│  │  (external)   │            │  (MCP client)      │                           │
│  └──────────────┘             └───────────────────┘                           │
│                                                              │                │
│  ┌──────────────┐             ┌───────────────────┐          │                │
│  │  Local LSP    │◄──────────►│  blazecode-lsp      │──────────┘                │
│  │  (rust-analy) │            │  (LSP server)      │                           │
│  └──────────────┘             └───────────────────┘                           │
│                                                                               │
│  API Endpoints (30 route groups):                                             │
│  ─────────────────────────────                                                 │
│  GET    /health                   POST   /session/{id}/input                  │
│  GET    /event (SSE)              GET    /session/{id}/output                 │
│  POST   /agent                    POST   /project                             │
│  GET    /agent/messages           GET    /project/{id}                        │
│  GET    /api                      POST   /provider/chat                       │
│  POST   /command                  GET    /config                              │
│  GET    /config                   POST   /config                              │
│  POST   /control/register         POST   /credential                          │
│  GET    /control/status           GET    /credential                          │
│  POST   /experimental/*           POST   /mcp/connect                         │
│  GET    /file/read                DELETE /mcp/disconnect                      │
│  POST   /file/write               POST   /file/mcp/*                          │
│  GET    /file/glob                POST   /permission/respond                  │
│  GET    /file/grep                POST   /question/respond                    │
│  POST   /pty                      POST   /reference/add                       │
│  POST   /sync                     POST   /skill/load                          │
│  GET    /workspace                POST   /workspace                           │
│  GET    /workspace/{id}                                                    │
│  GET    /instance                                                        │
│  GET    /instance/metadata                                              │
│  POST   /instance/tui_event                                           │
│  ...                                                                 │
│                                                                      │
│  SSE Event Types:                                                    │
│  ───────────────                                                      │
│  session_message     — new message in a session                      │
│  session_error       — session encountered error                      │
│  session_status      — session state change                           │
│  permission_request  — user permission needed                         │
│  question_request    — user input needed                              │
│  llm_event           — LLM stream delta                               │
│  tool_result         — tool execution result                          │
│  tool_error          — tool execution error                           │
│  project_update      — project metadata changed                       │
│  workspace_update    — workspace state change                         │
│  mcp_status          — MCP connection status                          │
│  lsp_status          — LSP connection status                          │
│  sync_status         — sync/backup status                             │
│                                                                      │
└──────────────────────────────────────────────────────────────────────────────┘
```

### BlazeCode

```
┌──────────────────────────────────────────────────────────────────────────────┐
│                          BLAZECODE NETWORK ARCHITECTURE                        │
├──────────────────────────────────────────────────────────────────────────────┤
│                                                                               │
│  ┌──────────────┐    HTTP/SSE      ┌──────────────────┐                      │
│  │  TUI Client   │◄──────────────►│  Hono Server      │                      │
│  │  (OpenTUI)    │    WebSocket    │  (packages/       │                      │
│  └──────────────┘                 │   server/)        │                      │
│                                   │                   │                      │
│  ┌──────────────┐    HTTP/SSE     │  Same route set   │                      │
│  │  Web App      │◄──────────────►│  + Cloudflare      │                      │
│  │  (SolidJS)    │                │    Workers deploy  │                      │
│  └──────────────┘                 │  + AWS secondary   │                      │
│                                   │  + SST v4 infra    │                      │
│  ┌──────────────┐    HTTP/SSE     └─────────┬─────────┘                      │
│  │  VS Code Ext  │◄──────────────►          │                                 │
│  └──────────────┘                           ▼                                 │
│                                             │                                 │
│  ┌──────────────┐                           │                                 │
│  │  AI SDK pro-  │◄─────────────────────────┘                                 │
│  │  viders (17)  │                                                             │
│  │  Anthropic,   │                                                             │
│  │  OpenAI,      │                                                             │
│  │  Google, etc. │                                                             │
│  └──────────────┘                                                             │
│                                                                               │
│  ┌──────────────┐             ┌───────────────────┐                          │
│  │  MCP Servers  │◄──────────►│  MCP client (src/  │                         │
│  │  (external)   │            │    mcp/)            │                         │
│  └──────────────┘             └───────────────────┘                          │
│                                                                               │
│  ┌──────────────┐             ┌───────────────────┐                          │
│  │  Console App  │◄──────────►│  PlanetScale DB   │                          │
│  │  (SolidJS)    │            │  (console/*)       │                         │
│  └──────────────┘             └───────────────────┘                          │
│                                                                               │
│  ┌──────────────┐             ┌───────────────────┐                          │
│  │  Stats App    │◄──────────►│  PlanetScale DB   │                          │
│  │  (SolidJS)    │            │  (stats/*)         │                         │
│  └──────────────┘             └───────────────────┘                          │
│                                                                               │
│  Additional endpoints (BlazeCode-only):                                       │
│  ─────────────────────────────────────                                        │
│  POST /control-plane/register    ← self-hosted control plane                  │
│  GET  /control-plane/heartbeat                                              │
│  POST /control-plane/usage                                                  │
│  GET  /ssh/authorize                                                        │
│  POST /slack/command                                                        │
│  ─────────────────────────────────────                                        │
│  Deployment: SST v4 → Cloudflare Workers (primary), AWS (secondary)          │
│                                                                               │
└──────────────────────────────────────────────────────────────────────────────┘
```

---

## 8. File Size Distribution

### BlazeCode — File Size Histogram (176 source files, excluding target/)

```
   Range          Count    Bar
   ─────────────  ─────    ────────────────────
   0–100 LOC        19     ████
   101–300 LOC      34     ████████
   301–500 LOC      27     ██████
   501–1K LOC       41     ██████████
   1K–2K LOC        36     ████████
   2K–3K LOC         8     ██
   3K–5K LOC         7     ██
   5K+ LOC           4     █
   ─────────────  ─────
   Total:          176

Largest files (need splitting):
  1. src/main.rs                      8,575 LOC  ← CLI dispatch + 25+ command handlers
  2. crates/core/src/tool_impls.rs    7,235 LOC  ← 18 tool implementations in one file
  3. crates/core/src/plugin.rs        6,236 LOC  ← Plugin system (load, manage, lifecycle)
  4. crates/core/src/config.rs        4,861 LOC  ← Config parsing (TOML) + all config types
  5. crates/core/src/database.rs      4,758 LOC  ← Schema defs + migrations + storage
  6. crates/core/src/session.rs       4,133 LOC  ← SessionManager lifecycle
  7. crates/tui/src/app.rs            3,769 LOC  ← TUI app state + rendering
  8. crates/lsp/src/lib.rs            3,099 LOC  ← LSP server (single file)
  9. crates/core/src/mcp.rs           3,033 LOC  ← MCP client/server
 10. crates/core/src/provider.rs      3,018 LOC  ← Provider trait + types

⚠️ CRITICAL: tool_impls.rs (7,235 LOC) should be split per-tool.
⚠️ HIGH: main.rs (8,575 LOC) should be split into per-command modules.
⚠️ MEDIUM: plugin.rs (6,236 LOC) should be split into plugin/ subdirectory.
```

### BlazeCode — File Size Histogram (~2,610 source files)

```
   Range          Count    Bar
   ─────────────  ─────    ────────────────────
   0–100 LOC       1,444   ████████████████████████████████
   101–300 LOC       723   ████████████████
   301–500 LOC       210   ████
   501–1K LOC        163   ███
   1K–2K LOC          56   █
   2K–3K LOC           8
   3K–5K LOC           4
   5K+ LOC             4
   ─────────────  ─────
   Total:          2,610

Largest files:
  1. packages/sdk/js/src/v2/gen/types.gen.ts   11,271 LOC  ← GENERATED (auto-generated SDK types)
  2. packages/sdk/js/src/v2/gen/sdk.gen.ts      6,836 LOC  ← GENERATED
  3. packages/web/src/components/icons/index.tsx 4,454 LOC  ← Icon SVGs
  4. packages/blazecode/test/provider/transform.test.ts 4,408 LOC  ← Tests
  5. packages/sdk/js/src/gen/types.gen.ts       3,907 LOC  ← GENERATED

⚠️ MEDIUM: Generated files are acceptable (SDK), but test files > 4K LOC indicate
   test suite consolidation needed.
```

**Comparison:** BlazeCode has far more small files (1,444 under 100 LOC vs BlazeCode's 19). BlazeCode has concentrated logic in fewer, larger files. BlazeCode uses a more granular module structure (2,610 files vs 181), which is more maintainable but has higher overhead.

---

## 9. Cyclomatic Complexity Map

### BlazeCode — High-Complexity Modules

| Module | File | LOC | Complexity Factors | Recommendation |
|---|---|---|---|---|
| **CLI Dispatch** | `src/main.rs` | 8,575 | 25+ commands, clap derives, async dispatch | Split per-command handlers into files |
| **Tool Implementations** | `tool_impls.rs` | 7,235 | 18 tools, each with full IO, error handling, streaming | Split into `tools/` directory (one file per tool) |
| **Plugin System** | `plugin.rs` | 6,236 | Loader, manager, lifecycle, npm resolution, hot-reload | Split into `plugin/` directory |
| **Config** | `config.rs` | 4,861 | TOML parser, 30+ config structs, merge logic, validation | Split into `config/` directory |
| **Database** | `database.rs` | 4,758 | Schema definitions, migration system, storage backends | Split into `database/` directory |
| **Session Manager** | `session.rs` | 4,133 | Lifecycle, CRUD, event emission, cross-session operations | Split into `session/` directory |
| **TUI App** | `tui/app.rs` | 3,769 | Ratatui rendering, state management, 17 components | Already componentized; keep pattern |
| **Provider Trait** | `provider.rs` | 3,018 | Core trait, 10+ message types, serialization | Moderate; OK as single file |
| **SSE / Streaming** | `sse.rs` | ~1,200 | EventSource client, reconnection, backpressure | OK |

### BlazeCode — High-Complexity Modules

| Module | File | LOC | Complexity Factors |
|---|---|---|---|
| **SDK Types (gen)** | `types.gen.ts` | 11,271 | Generated — acceptable |
| **Provider Transform Test** | `transform.test.ts` | 4,408 | Test complexity, snapshot testing |
| **Session Runner Test** | `session-runner.test.ts` | 3,574 | Integration test with many scenarios |
| **Session Layout** | `tui/src/routes/session/index.tsx` | 2,665 | TUI session route, multiple sub-components |
| **Web Layout** | `app/src/pages/layout.tsx` | 2,563 | App shell with sidebar, header, routing |
| **Message Part Component** | `ui/src/components/message-part.tsx` | 2,436 | Complex rendering of different message types |
| **SSE Stream Transport** | `stream.transport.test.ts` | 2,363 | Test for stream transport |

**Structural Complexity Difference:** BlazeCode's Effect v4 architecture provides structured concurrency, making complex orchestration more manageable. BlazeCode's tokio + Arc<RwLock> pattern requires manual lock management, increasing cognitive complexity in the session runner and tool execution paths.

---

## 10. Call Graph — Major Call Chains

### BlazeCode — Critical Call Chains

```
CLI CHAIN (run):
  main.rs::main()
  → Cli::parse()
  → Commands::Run
  → run_handler()
  → initialize_runtime()        [runtime.rs]
  → SessionRunner::run()         [session_runner.rs]
  → SessionPromptBuilder::build() [session_prompt.rs]
  → loop {
      Provider::stream()          [provider.rs → providers/*.rs]
      → reqwest::Client::post()  [HTTP to LLM API]
      → stream response
      → parse SSE events
      → for each tool_call:
          ToolRegistry::execute()  [tool.rs]
          → match tool name:
              "bash"       → BashTool::run()       [tool_impls.rs:575]
              "read"       → ReadTool::run()        [tool_impls.rs:975]
              "write"      → WriteTool::run()       [tool_impls.rs:1306]
              "edit"       → EditTool::run()        [tool_impls.rs:1418]
              "glob"       → GlobTool::run()        [tool_impls.rs:1681]
              "grep"       → GrepTool::run()        [tool_impls.rs:1816]
              "web_fetch"  → WebFetchTool::run()    [tool_impls.rs:2059]
              "web_search" → WebSearchTool::run()   [tool_impls.rs:2491]
              "apply_diff" → ApplyPatchTool::run()  [tool_impls.rs:2835]
              "task"       → TaskTool::run()        [tool_impls.rs:3646]
              "question"   → QuestionTool::run()    [tool_impls.rs:4016]
              "skill"      → SkillTool::run()       [tool_impls.rs:4216]
              "todo_write" → TodoWriteTool::run()   [tool_impls.rs:4402]
              "stash"      → StashTool::run()       [tool_impls.rs:4686]
              "notebook"   → NotebookEditTool::run() [tool_impls.rs:4961]
              "lsp"        → LspTool::run()          [tool_impls.rs:5431]
              "output"     → TaskOutputTool::run()   [tool_impls.rs:5213]
          → truncate_output()   [truncate.rs]
      }
  → SessionRunResult

SERVER CHAIN:
  axum::serve()
  → router dispatch
  → route handler (e.g., routes/session.rs)
  → SessionManager::load()
  → ProviderService::resolve_model()
  → SessionRunner::run()

TUI CHAIN:
  TuiApp::run()
  → SseClient::connect()
  → event loop
  → match event type:
      session_message → Conversation component
      tool_result     → ToolRender component
      permission_request → PermissionDialog
      question_request   → QuestionDialog
  → ratatui::Terminal::draw()
  → component::render()

MCP CHAIN:
  blazecode-mcp::McpClient::connect()
  → JSON-RPC over SSE/stdio
  → tools/list, tools/call, resources/read
  → dispatches to blazecode-core tools
```

### BlazeCode — Equivalent Call Chains

```
openode CLI:
  index.ts → yargs parse → Effect.runMain(...)
  → Layer.provide(Database, Bus, Tools, Providers)
  → Command handler (RunCmd)
  → SessionRunner.run()
  → Provider.chat() [via @ai-sdk/provider + @blazecode-ai/llm]
  → Tool execution [via @blazecode-ai/core/tool]
  → Output

Key difference: Effect v4 wraps everything in Effect<A, E, R>
with automatic resource cleanup, structured concurrency via
Fibers, and Layer-based dependency injection. BlazeCode does
this manually with Arc<RwLock<>> and tokio::spawn.
```

---

## Gap Analysis: BlazeCode vs BlazeCode

| Location (File, Line) | BlazeCode Implementation | BlazeCode Implementation | Gap | Consequence | Recommendation | Severity |
|---|---|---|---|---|---|---|
| `packages/core/src/providers/*.ts` | 17 AI SDK providers (Alibaba, Cerebras, Cohere, DeepInfra, etc.) via `@ai-sdk/*` | 14 providers in `providers/*.rs` | Missing: Cerebras, Cohere, DeepInfra, TogetherAI, Mistral, Perplexity, Groq, DeepSeek, Alibaba, Gateway, Vertex, Vercel | Users cannot use these LLM providers directly | Add OpenAI-compatible profiles for all 11 missing providers | HIGH |
| `packages/core/src/database/sqlite.ts` | Drizzle ORM with Effect SQL integration | Raw sqlx queries | Missing ORM layer, no type-safe query builder | More verbose/safer queries, higher maintenance | Consider sqlx integration or keep raw SQL (acceptable) | LOW |
| `packages/blazecode/src/server/routes/` | Full route set (31 groups) | 30 route groups | Missing: control-plane registration endpoint | No self-hosted control plane support | Add control-plane route | MEDIUM |
| `packages/core/src/event.ts` | EventV2 with Effect PubSub | Tokio broadcast bus | Missing: typed subscriber effects, structured concurrency | Less type-safe event dispatch | Current impl adequate for parity | LOW |
| `packages/core/src/github-copilot/` | Full GitHub Copilot integration (auth, token exchange, chat) | `github_copilot.rs` (basic) | Missing: copilot plugin GUI, extended auth flows | Limited Copilot integration | Expand copilot module | MEDIUM |
| `packages/blazecode/src/provider/provider.ts` | Effect-based provider composition, middleware chains | Simple provider trait | No effect composition for retry, fallback, rate-limiting | Less sophisticated provider orchestration | Add retry/fallback wrapper provider | LOW |
| `packages/app/`, `packages/web/` | SolidJS web + desktop apps | No web/desktop UI | Missing: web application, desktop Electron app | CLI/TUI only; no GUI for non-terminal users | Out of scope for Rust port | LOW |
| `packages/console/*`, `packages/stats/*` | Console and analytics dashboards | No console/stats | Missing: PlanetScale-backed admin console | No web-based admin interface | Out of scope for Rust port | LOW |
| `packages/core/src/session/runner/index.ts` | Effect-based Layer composition for DI | Manual `Arc<RwLock<>>` wiring in `runtime.rs` | Missing: structured concurrency, automatic resource scoping | Manual wiring is fragile; errors harder to trace | Add `RuntimeContext` builder pattern (already done well) | MEDIUM |
| `packages/blazecode/src/auth/` | OpenAuth OAuth, SSO, device flow | Basic env-var auth + server password | Missing: OAuth, SSO, device flow, token refresh | Users must use API keys directly | Add OAuth flow | MEDIUM |
| `packages/core/src/event.ts` (projectors) | Event projector system with Effect fibers | `event_projector.rs` + `session_projector.rs` | No fiber-based projection | Projectors run sequentially | Add async projector dispatch | LOW |
| `packages/blazecode/src/control-plane/` | Self-hosted control plane (instance registry, heartbeat, usage) | No control plane | No instance registry or heartbeat | Cannot manage distributed instances | Add optional control plane | HIGH |
| `packages/core/src/skill/discovery.ts` | Remote skill discovery via HTTP index | `skill.rs` (basic local discovery) | No remote skill pulling | Only local skills supported | Add HTTP skill index | MEDIUM |
| `packages/core/src/session/runner/llm.ts` | V2 runner with Effect fibers for concurrent I/O | `session_runner.rs` (tokio sequential loop) | No concurrent tool execution | Serial tool execution (slower) | Add parallel tool execution | MEDIUM |

---

## Feature Completeness Scorecard

| Module | BlazeCode Files | BlazeCode Files | Est. Completeness | Grade |
|---|---|---|---|---|
| **Config** | `packages/core/src/config/*`, `packages/blazecode/src/config/*` | `config.rs` | 90% | A |
| **Database** | `packages/core/src/database/*` (10 files) | `database.rs`, `storage.rs` | 95% | A |
| **Session Management** | `packages/core/src/session/*` (20+ files) | `session*.rs` (14 files) | 95% | A |
| **LLM Providers** | `@blazecode-ai/llm` + `@ai-sdk/*` (17 providers) | `providers/*.rs` (14 providers) | 65% | C |
| **Tool System** | `packages/blazecode/src/tool/*` | `tool.rs`, `tool_impls.rs` | 95% | A |
| **Event System** | `packages/core/src/event.ts` | `event.rs`, `event_projector.rs` | 90% | A |
| **Plugin System** | `packages/core/src/plugin/*`, `packages/blazecode/src/plugin/*` | `plugin.rs`, `npm.rs` | 85% | B |
| **MCP** | `packages/blazecode/src/mcp/*` | `mcp.rs`, `mcp_oauth.rs`, `blazecode-mcp` | 90% | A |
| **LSP** | `packages/blazecode/src/lsp/*` | `lsp.rs`, `blazecode-lsp` (3K file) | 85% | B |
| **TUI** | `packages/tui/*` (SolidJS/OpenTUI) | `blazecode-tui/*` (ratatui) | 95% | A |
| **Server** | `packages/blazecode/src/server/*` | `blazecode-server/*` (42 files) | 95% | A |
| **Account/Auth** | `packages/blazecode/src/account/*`, `auth/*` | `account.rs`, `auth.rs`, `credential.rs` | 70% | C |
| **Skills** | `packages/core/src/skill/*`, `packages/blazecode/src/skill/*` | `skill.rs` | 75% | C |
| **Permission** | `packages/core/src/permission/*` | `permission.rs` | 95% | A |
| **Filesystem** | `packages/core/src/filesystem/*` | `filesystem.rs`, `fs_util.rs` | 95% | A |
| **PTY/Process** | `packages/core/src/pty/*` | `pty.rs`, `process.rs` | 90% | A |
| **Observability** | `packages/core/src/observability/*` | `observability.rs` | 80% | B |
| **Background Jobs** | `packages/blazecode/src/background/*` | `background_job.rs` | 80% | B |
| **Console/Stats** | `packages/console/*` (5), `packages/stats/*` (3) | — | 0% | F |
| **Web/Desktop App** | `packages/app/`, `packages/web/`, `packages/desktop/` | — | 0% | F |
| **Control Plane** | `packages/blazecode/src/control-plane/*` | — | 0% | F |
| **Auth/OAuth** | `packages/blazecode/src/auth/*` | `auth.rs` (basic) | 20% | D |

**Overall Port Completeness: ~75%** (by features, ~85% by LOC)

---

## Scorecard Summary

```
Category                  BlazeCode Score   BlazeCode Score    Parity
────────────────────────  ───────────────  ───────────────  ──────
Architecture Match        85%              100%              HIGH
Module Organization        80%              95%              HIGH
Provider Coverage         65%              100%              MEDIUM
Network/API Parity         95%             100%              HIGH
Database Parity            95%             100%              HIGH
UI Completeness           70%              100%              MEDIUM
Build Infrastructure      95%              100%              HIGH
Testing Coverage          70%              80%               MEDIUM
Auth/Security             40%              100%              LOW
Console/Web                0%              N/A (extra)       N/A
────────────────────────  ───────────────  ───────────────  ──────
**Weighted Total**        **72%**          **97%**           —
```

**Letter Grade: B-** (BlazeCode is a competent port of the core engine, missing primarily the web/console layer, control plane, and some long-tail providers)

---

*Report generated by Agent 01 — Repository Cartographer*
