# Feature Parity Matrix — OpenCode (TypeScript) vs RustCode (Rust)

> **Generated**: 2026-06-19  
> **OpenCode version**: 1.17.8 (commit `5d0f86606ac30690f79f0a6a9f41a1f49fe95d0b`)  
> **RustCode version**: 0.1.0

**Status values**: ✅ Full — 🔶 Partial(80%) — ⚠️ Partial(50%) — ❌ Missing — 📝 Scaffold (types exist, no logic)

---

## 1. CLI Commands

| Category | Feature | OpenCode Location | RustCode Location | Status | Gap Description | Priority |
|----------|---------|-------------------|-------------------|--------|-----------------|----------|
| CLI | Entry point | `packages/opencode/src/index.ts` | `src/main.rs` | ✅ Full | Both parse args and dispatch to subcommand handlers | P0 |
| CLI | Global --help / -h | `index.ts:49-50` | clap derive `#[command]` | ✅ Full | Both show help via yargs/clap | P0 |
| CLI | Global --version / -v | `index.ts:51-52` | `Commands::Version` variant | ✅ Full | RustCode has explicit `version` subcommand + `CARGO_PKG_VERSION` | P0 |
| CLI | Global --print-logs | `index.ts:53-56` | `Cli.print_logs` | ✅ Full | Both set env/tracing filter | P0 |
| CLI | Global --log-level | `index.ts:57-61` | `Cli.log_level` | ✅ Full | Both support DEBUG/INFO/WARN/ERROR | P0 |
| CLI | Global --pure | `index.ts:62-65` | `Cli.pure` | ✅ Full | Both skip external plugins | P0 |
| CLI | Shell completion | `index.ts:80` | ❌ Missing | clap can generate shell completions but not wired | P3 |
| CLI | `acp` | `packages/opencode/src/cli/cmd/acp.ts` | `Commands::Acp` | 🔶 Partial(80%) | RustCode has args struct, handler prints placeholder | P1 |
| CLI | `mcp add` | `packages/opencode/src/cli/cmd/mcp.ts` | `McpCommand::Add` | 🔶 Partial(80%) | RustCode implements add with url/env/headers; TS also supports interactive prompts | P1 |
| CLI | `mcp list` / `mcp ls` | `packages/opencode/src/cli/cmd/mcp.ts` | `McpCommand::List` | 🔶 Partial(80%) | RustCode lists from config files; TS also shows live server statuses | P1 |
| CLI | `mcp auth` | `packages/opencode/src/cli/cmd/mcp.ts` | `McpCommand::Auth` | ⚠️ Partial(50%) | RustCode implements OAuth flow discovery but browser callback + token exchange not wired | P1 |
| CLI | `mcp logout` | `packages/opencode/src/cli/cmd/mcp.ts` | `McpCommand::Logout` | 🔶 Partial(80%) | RustCode removes credentials; TS full implementation | P1 |
| CLI | `mcp debug` | `packages/opencode/src/cli/cmd/mcp.ts` | `McpCommand::Debug` | 🔶 Partial(80%) | RustCode shows config, tokens, HTTP connectivity, MCP initialize test | P1 |
| CLI | `tui` / default | `packages/opencode/src/cli/cmd/tui.ts` | `Commands::Tui(TuiArgs)` | 🔶 Partial(80%) | RustCode TUI launches ratatui app with session layers, but not full TS TUI feature set | P0 |
| CLI | `attach` | `packages/opencode/src/cli/cmd/attach.ts` | `Commands::Attach(AttachArgs)` | ✅ Full | Both attach to remote server with auth, dir, continue, fork | P0 |
| CLI | `run` | `packages/opencode/src/cli/cmd/run.ts` | `Commands::Run(RunArgs)` | 🔶 Partial(80%) | RustCode run works with local providers and remote SSE attach; not all TS features | P0 |
| CLI | `run --command` | `run.ts` | `RunArgs.command` | ✅ Full | Both support -c | P0 |
| CLI | `run --continue / -c` | `run.ts` | `RunArgs.r#continue` | ✅ Full | Both continue last session | P0 |
| CLI | `run --session / -s` | `run.ts` | `RunArgs.session` | ✅ Full | Both specify session ID | P0 |
| CLI | `run --fork` | `run.ts` | `RunArgs.fork` | ✅ Full | Both fork session before continuing | P0 |
| CLI | `run --share` | `run.ts` | `RunArgs.share` | ✅ Full | Both share session | P1 |
| CLI | `run --model / -m` | `run.ts` | `RunArgs.model` | ✅ Full | Both specify provider/model | P0 |
| CLI | `run --agent` | `run.ts` | `RunArgs.agent` | ✅ Full | Both specify agent | P0 |
| CLI | `run --format` | `run.ts` | `RunArgs.format` | ✅ Full | Both support "default" and "json" | P0 |
| CLI | `run --file / -f` | `run.ts` | `RunArgs.file` | ✅ Full | Both attach files | P1 |
| CLI | `run --title` | `run.ts` | `RunArgs.title` | ✅ Full | Both set session title | P1 |
| CLI | `run --attach` | `run.ts` | `RunArgs.attach` | ✅ Full | Both connect to remote server | P0 |
| CLI | `run --password / -p` | `run.ts` | `RunArgs.password` | ✅ Full | Both basic auth password | P0 |
| CLI | `run --username / -u` | `run.ts` | `RunArgs.username` | ✅ Full | Both basic auth username | P0 |
| CLI | `run --dir` | `run.ts` | `RunArgs.dir` | ✅ Full | Both remote directory | P1 |
| CLI | `run --port` | `run.ts` | `RunArgs.port` | ✅ Full | Both local server port | P1 |
| CLI | `run --variant` | `run.ts` | `RunArgs.variant` | ✅ Full | Both reasoning effort variant | P1 |
| CLI | `run --thinking` | `run.ts` | `RunArgs.thinking` | ✅ Full | Both show thinking blocks | P1 |
| CLI | `run --replay` | `run.ts` | `RunArgs.replay` | ✅ Full | Both replay interactive session history | P2 |
| CLI | `run --replay-limit` | `run.ts` | `RunArgs.replay_limit` | ✅ Full | Both cap replay messages | P2 |
| CLI | `run --interactive / -i` | `run.ts` | `RunArgs.interactive` | ✅ Full | Both direct interactive mode | P0 |
| CLI | `run --dangerously-skip-permissions` | `run.ts` | `RunArgs.dangerously_skip_permissions` | ✅ Full | Both auto-approve | P1 |
| CLI | `run --demo` | `run.ts` | `RunArgs.demo` | ✅ Full | Both demo slash commands | P2 |
| CLI | `generate` | `packages/opencode/src/cli/cmd/generate.ts` | `Commands::Generate` | 📝 Scaffold | Both have command stub; RustCode handler not implemented | P3 |
| CLI | `debug config` | `packages/opencode/src/cli/cmd/debug/config.ts` | `DebugCommand::Config` | 📝 Scaffold | Both have stub; RustCode shows config path but not full dump | P2 |
| CLI | `debug lsp diagnostics` | `packages/opencode/src/cli/cmd/debug/lsp.ts` | `DebugLspCommand::Diagnostics` | 📝 Scaffold | RustCode has args struct, handler prints placeholder | P2 |
| CLI | `debug lsp symbols` | `packages/opencode/src/cli/cmd/debug/lsp.ts` | `DebugLspCommand::Symbols` | 📝 Scaffold | Same as above | P2 |
| CLI | `debug lsp document-symbols` | `packages/opencode/src/cli/cmd/debug/lsp.ts` | `DebugLspCommand::DocumentSymbols` | 📝 Scaffold | Same as above | P2 |
| CLI | `debug rg files` | `packages/opencode/src/cli/cmd/debug/ripgrep.ts` | `DebugRgCommand::Files` | 📝 Scaffold | RustCode has args, handler not implemented | P2 |
| CLI | `debug rg search` | `packages/opencode/src/cli/cmd/debug/ripgrep.ts` | `DebugRgCommand::Search` | 📝 Scaffold | Same as above | P2 |
| CLI | `debug file search` | `packages/opencode/src/cli/cmd/debug/file.ts` | `DebugFileCommand::Search` | 📝 Scaffold | Same as above | P2 |
| CLI | `debug file read` | `packages/opencode/src/cli/cmd/debug/file.ts` | `DebugFileCommand::Read` | 📝 Scaffold | Same as above | P2 |
| CLI | `debug file list` | `packages/opencode/src/cli/cmd/debug/file.ts` | `DebugFileCommand::List` | 📝 Scaffold | Same as above | P2 |
| CLI | `debug scrap` | `packages/opencode/src/cli/cmd/debug/scrap.ts` | `DebugCommand::Scrap` | 📝 Scaffold | RustCode has variant, handler not implemented | P2 |
| CLI | `debug skill` | `packages/opencode/src/cli/cmd/debug/skill.ts` | `DebugCommand::Skill` | 📝 Scaffold | Same as above | P2 |
| CLI | `debug snapshot track` | `packages/opencode/src/cli/cmd/debug/snapshot.ts` | `DebugSnapshotCommand::Track` | 📝 Scaffold | Same as above | P2 |
| CLI | `debug snapshot patch` | `packages/opencode/src/cli/cmd/debug/snapshot.ts` | `DebugSnapshotCommand::Patch` | 📝 Scaffold | Same as above | P2 |
| CLI | `debug snapshot diff` | `packages/opencode/src/cli/cmd/debug/snapshot.ts` | `DebugSnapshotCommand::Diff` | 📝 Scaffold | Same as above | P2 |
| CLI | `debug startup` | `packages/opencode/src/cli/cmd/debug/startup.ts` | `DebugCommand::Startup` | 📝 Scaffold | RustCode has variant; handler prints elapsed_ms placeholder | P2 |
| CLI | `debug agent` | `packages/opencode/src/cli/cmd/debug/agent.ts` | `DebugCommand::Agent` | 📝 Scaffold | RustCode has args struct with name, tool, params | P2 |
| CLI | `debug v2` | `packages/opencode/src/cli/cmd/debug/v2.ts` | `DebugCommand::V2` | 📝 Scaffold | Both have stubs | P2 |
| CLI | `debug info` | `packages/opencode/src/cli/cmd/debug/index.ts` | `DebugCommand::Info` | 📝 Scaffold | RustCode has variant, handler not implemented | P2 |
| CLI | `debug paths` | `packages/opencode/src/cli/cmd/debug/index.ts` | `DebugCommand::Paths` | 📝 Scaffold | Same as above | P2 |
| CLI | `debug wait` | `packages/opencode/src/cli/cmd/debug/index.ts` | `DebugCommand::Wait` | 📝 Scaffold | Same as above | P2 |
| CLI | `console login` | `packages/opencode/src/cli/cmd/account.ts` | `ConsoleCommand::Login` | ⚠️ Partial(50%) | RustCode has args struct; handler prints placeholder | P1 |
| CLI | `console logout` | `account.ts` | `ConsoleCommand::Logout` | 📝 Scaffold | RustCode has variant; handler prints placeholder | P1 |
| CLI | `console switch` | `account.ts` | `ConsoleCommand::Switch` | 📝 Scaffold | Same as above | P1 |
| CLI | `console orgs` | `account.ts` | `ConsoleCommand::Orgs` | 📝 Scaffold | Same as above | P1 |
| CLI | `console open` | `account.ts` | `ConsoleCommand::Open` | 📝 Scaffold | Same as above | P1 |
| CLI | `providers list` / `providers ls` | `packages/opencode/src/cli/cmd/providers.ts` | `ProvidersCommand::List` | 🔶 Partial(80%) | RustCode lists env vars and auth.json credentials | P0 |
| CLI | `providers login` | `providers.ts` | `ProvidersCommand::Login` | ⚠️ Partial(50%) | RustCode supports API key entry for provider; no OAuth flow | P0 |
| CLI | `providers logout` | `providers.ts` | `ProvidersCommand::Logout` | 🔶 Partial(80%) | RustCode removes from auth.json | P0 |
| CLI | `agent create` | `packages/opencode/src/cli/cmd/agent.ts` | `AgentCommand::Create` | ⚠️ Partial(50%) | RustCode shows path/desc/mode/perm/model; no LLM generation | P1 |
| CLI | `agent list` | `agent.ts` | `AgentCommand::List` | 🔶 Partial(80%) | RustCode scans global and local agent dirs, reads frontmatter | P1 |
| CLI | `upgrade` | `packages/opencode/src/cli/cmd/upgrade.ts` | `Commands::Upgrade(UpgradeArgs)` | 📝 Scaffold | RustCode has args struct; handler not implemented | P2 |
| CLI | `uninstall` | `packages/opencode/src/cli/cmd/uninstall.ts` | `Commands::Uninstall(UninstallArgs)` | 📝 Scaffold | RustCode has args (keep-config, keep-data, dry-run, force); handler shows paths | P3 |
| CLI | `serve` | `packages/opencode/src/cli/cmd/serve.ts` | `Commands::Serve(NetworkArgs)` | 🔶 Partial(80%) | RustCode starts axum server with port/host/mdns/cors; not all TS features | P0 |
| CLI | `web` | `packages/opencode/src/cli/cmd/web.ts` | `Commands::Web(NetworkArgs)` | 🔶 Partial(80%) | Same as serve + opens browser; TS web has full Next.js frontend | P0 |
| CLI | `models` | `packages/opencode/src/cli/cmd/models.ts` | `Commands::Models(ModelsArgs)` | 🔶 Partial(80%) | RustCode lists models from providers; TS also queries models.dev | P0 |
| CLI | `models --refresh` | `models.ts` | `ModelsArgs.refresh` | 📝 Scaffold | RustCode has flag but models.dev API not wired | P2 |
| CLI | `models --verbose` | `models.ts` | `ModelsArgs.verbose` | ✅ Full | RustCode prints full model JSON | P1 |
| CLI | `stats` | `packages/opencode/src/cli/cmd/stats.ts` | `Commands::Stats(StatsArgs)` | 🔶 Partial(80%) | RustCode queries SQLite for overview, model, tool stats from session DB | P1 |
| CLI | `stats --days` | `stats.ts` | `StatsArgs.days` | ✅ Full | Both filter by time range | P1 |
| CLI | `stats --tools` | `stats.ts` | `StatsArgs.tools` | ✅ Full | Both show tool usage | P1 |
| CLI | `stats --models` | `stats.ts` | `StatsArgs.models` | ✅ Full | Both show model usage | P1 |
| CLI | `stats --project` | `stats.ts` | `StatsArgs.project` | ✅ Full | Both filter by project | P1 |
| CLI | `export` | `packages/opencode/src/cli/cmd/export.ts` | `Commands::Export(ExportArgs)` | 🔶 Partial(80%) | RustCode exports session+messages+parts as JSON with sanitize option | P1 |
| CLI | `import` | `packages/opencode/src/cli/cmd/import.ts` | `Commands::Import(ImportArgs)` | 🔶 Partial(80%) | RustCode imports from file or URL, inserts session+messages+parts | P1 |
| CLI | `github install` | `packages/opencode/src/cli/cmd/github.ts` | `GithubCommand::Install` | 📝 Scaffold | RustCode has variant; handler not implemented | P2 |
| CLI | `github run` | `github.ts` | `GithubCommand::Run` | 📝 Scaffold | RustCode has args (event, event_payload, token); handler not implemented | P2 |
| CLI | `pr` | `packages/opencode/src/cli/cmd/pr.ts` | `Commands::Pr(PrArgs)` | 📝 Scaffold | RustCode has number arg; handler not implemented | P2 |
| CLI | `session list` | `packages/opencode/src/cli/cmd/session.ts` | `SessionCommand::List` | 🔶 Partial(80%) | RustCode queries SQLite, supports table/json format, max-count | P1 |
| CLI | `session delete` | `session.ts` | `SessionCommand::Delete` | 🔶 Partial(80%) | RustCode deletes session + child sessions + messages + parts | P1 |
| CLI | `plugin` / `plug` | `packages/opencode/src/cli/cmd/plug.ts` | `Commands::Plugin(PluginArgs)` | 📝 Scaffold | RustCode has args (module, global, force); handler not implemented | P2 |
| CLI | `db` | `packages/opencode/src/cli/cmd/db.ts` | `Commands::Db(DbArgs)` | 📝 Scaffold | RustCode has query + format args; handler not implemented | P2 |
| CLI | `version` | `index.ts` (built-in) | `Commands::Version` | ✅ Full | RustCode has explicit version subcommand | P0 |
| CLI | `completion` | `index.ts:80` | ❌ Missing | TS has shell completion generation; RustCode not wired | P3 |
| CLI | Network options (port) | `packages/opencode/src/cli/network.ts` | `NetworkArgs.port` | ✅ Full | Both support port config | P0 |
| CLI | Network options (hostname) | `network.ts` | `NetworkArgs.hostname` | ✅ Full | Both support hostname config | P0 |
| CLI | Network options (mdns) | `network.ts` | `NetworkArgs.mdns` | ✅ Full | Both support mDNS discovery | P1 |
| CLI | Network options (mdns-domain) | `network.ts` | `NetworkArgs.mdns_domain` | ✅ Full | Both support custom mDNS domain | P1 |
| CLI | Network options (cors) | `network.ts` | `NetworkArgs.cors` | ✅ Full | Both support CORS origins | P1 |
| CLI | Effect command wrapper | `packages/opencode/src/cli/effect-cmd.ts` | ❌ Missing | TS wraps commands with Effect runtime; RustCode uses raw tokio | P2 |

## 2. Provider Integrations

| Category | Feature | OpenCode Location | RustCode Location | Status | Gap Description | Priority |
|----------|---------|-------------------|-------------------|--------|-----------------|----------|
| Provider | Provider trait | `packages/llm/src/provider.ts` | `crates/rustcode-core/src/provider.rs` | ✅ Full | Both define Provider trait with stream/complete/list_models | P0 |
| Provider | Provider catalog | `packages/opencode/src/provider/provider.ts` | `provider.rs` | 🔶 Partial(80%) | RustCode has ProviderCatalog trait; not all methods wired | P1 |
| Provider | Anthropic | `packages/core/src/plugin/provider/anthropic.ts` | `crates/rustcode-core/src/providers/anthropic.rs` | 🔶 Partial(80%) | RustCode has streaming + non-streaming via reqwest; no SDK | P0 |
| Provider | OpenAI | `packages/core/src/plugin/provider/openai.ts` | `crates/rustcode-core/src/providers/openai.rs` | 🔶 Partial(80%) | Same as above | P0 |
| Provider | Google Gemini | `packages/core/src/plugin/provider/google.ts` | `crates/rustcode-core/src/providers/gemini.rs` | 🔶 Partial(80%) | Same as above | P0 |
| Provider | Azure OpenAI | `packages/core/src/plugin/provider/azure.ts` | `crates/rustcode-core/src/providers/azure.rs` | ⚠️ Partial(50%) | RustCode has stub; not fully tested | P1 |
| Provider | Amazon Bedrock | `packages/core/src/plugin/provider/amazon-bedrock.ts` | `crates/rustcode-core/src/providers/bedrock.rs` | ⚠️ Partial(50%) | RustCode has stub; AWS SDK integration not wired | P1 |
| Provider | Google Vertex AI | `packages/core/src/plugin/provider/google-vertex.ts` | ❌ Missing | No Vertex AI provider in RustCode | P1 |
| Provider | OpenRouter | `packages/core/src/plugin/provider/openrouter.ts` | `crates/rustcode-core/src/providers/openrouter.rs` | ⚠️ Partial(50%) | RustCode has stub; uses Anthropic-compatible API | P1 |
| Provider | Groq | `packages/core/src/plugin/provider/groq.ts` | `crates/rustcode-core/src/providers/groq.rs` | ⚠️ Partial(50%) | RustCode has stub | P1 |
| Provider | Mistral | `packages/core/src/plugin/provider/mistral.ts` | `crates/rustcode-core/src/providers/mistral.rs` | ⚠️ Partial(50%) | RustCode has stub | P1 |
| Provider | xAI | `packages/core/src/plugin/provider/xai.ts` | `crates/rustcode-core/src/providers/xai.rs` | ⚠️ Partial(50%) | RustCode has stub | P1 |
| Provider | Cerebras | `packages/core/src/plugin/provider/cerebras.ts` | `crates/rustcode-core/src/providers/cerebras.rs` | ⚠️ Partial(50%) | RustCode has stub | P1 |
| Provider | Cohere | `packages/core/src/plugin/provider/cohere.ts` | `crates/rustcode-core/src/providers/cohere.rs` | ⚠️ Partial(50%) | RustCode has stub | P1 |
| Provider | Together AI | `packages/core/src/plugin/provider/togetherai.ts` | `crates/rustcode-core/src/providers/together.rs` | ⚠️ Partial(50%) | RustCode has stub | P1 |
| Provider | Perplexity | `packages/core/src/plugin/provider/perplexity.ts` | `crates/rustcode-core/src/providers/perplexity.rs` | ⚠️ Partial(50%) | RustCode has stub | P1 |
| Provider | Fireworks AI | `packages/core/src/plugin/provider/fireworks.ts` (via openai-compatible) | `crates/rustcode-core/src/providers/fireworks.rs` | ⚠️ Partial(50%) | RustCode has stub; TS uses openai-compatible | P1 |
| Provider | DeepInfra | `packages/core/src/plugin/provider/deepinfra.ts` | ❌ Missing | No DeepInfra provider in RustCode | P2 |
| Provider | Cloudflare AI Gateway | `packages/core/src/plugin/provider/cloudflare-ai-gateway.ts` | ❌ Missing | No CFL AI Gateway provider in RustCode | P2 |
| Provider | Cloudflare Workers AI | `packages/core/src/plugin/provider/cloudflare-workers-ai.ts` | ❌ Missing | No CFL Workers AI provider in RustCode | P2 |
| Provider | GitHub Copilot | `packages/core/src/plugin/provider/github-copilot.ts` | `crates/rustcode-core/src/providers/github_copilot.rs` | ⚠️ Partial(50%) | RustCode has stub | P2 |
| Provider | GitLab AI | `packages/core/src/plugin/provider/gitlab.ts` | ❌ Missing | No GitLab AI provider in RustCode | P2 |
| Provider | NVIDIA | `packages/core/src/plugin/provider/nvidia.ts` | ❌ Missing | No NVIDIA provider in RustCode | P2 |
| Provider | Vercel | `packages/core/src/plugin/provider/vercel.ts` | ❌ Missing | No Vercel provider in RustCode | P2 |
| Provider | Alibaba | `packages/core/src/plugin/provider/alibaba.ts` | ❌ Missing | No Alibaba provider in RustCode | P2 |
| Provider | SAP AI Core | `packages/core/src/plugin/provider/sap-ai-core.ts` | ❌ Missing | No SAP AI Core provider in RustCode | P2 |
| Provider | Snowflake Cortex | `packages/core/src/plugin/provider/snowflake-cortex.ts` | ❌ Missing | No Snowflake provider in RustCode | P2 |
| Provider | Venice | `packages/core/src/plugin/provider/venice.ts` | ❌ Missing | No Venice provider in RustCode | P2 |
| Provider | DeepSeek | `packages/core/src/plugin/provider/deepseek.ts` (via openai-compatible) | `crates/rustcode-core/src/providers/deepseek.rs` | ⚠️ Partial(50%) | RustCode has stub | P1 |
| Provider | AI21 | ❌ Missing | `crates/rustcode-core/src/providers/ai21.rs` | 📝 Scaffold | RustCode has AI21 module that TS doesn't bundle | P2 |
| Provider | OpenAI Compatible | `packages/core/src/plugin/provider/openai-compatible.ts` | `crates/rustcode-core/src/providers/openai_compatible.rs` | ⚠️ Partial(50%) | RustCode has generic compatible provider | P1 |
| Provider | Dynamic | `packages/core/src/plugin/provider/dynamic.ts` | ❌ Missing | Dynamic provider loading from config | P2 |
| Provider | Gateway | `packages/core/src/plugin/provider/gateway.ts` | ❌ Missing | AI Gateway provider | P2 |
| Provider | Kilo | `packages/core/src/plugin/provider/kilo.ts` | ❌ Missing | Kilo provider | P3 |
| Provider | LLM Gateway | `packages/core/src/plugin/provider/llmgateway.ts` | ❌ Missing | LLM Gateway provider | P3 |
| Provider | OpenCode Console | `packages/core/src/plugin/provider/opencode.ts` | ❌ Missing | OpenCode-managed providers | P2 |
| Provider | ZenMux | `packages/core/src/plugin/provider/zenmux.ts` | ❌ Missing | ZenMux provider | P3 |
| Provider | OpenAI Auth | `packages/core/src/plugin/provider/openai-auth.ts` | ❌ Missing | OAuth-based OpenAI auth flow | P2 |
| Provider | Auth storage | `packages/opencode/src/provider/auth.ts` | `crates/rustcode-core/src/credential.rs` | 🔶 Partial(80%) | RustCode reads/writes auth.json; TS has full console login | P1 |
| Provider | Error types | `packages/opencode/src/provider/error.ts` | `crates/rustcode-core/src/error.rs` | 🔶 Partial(80%) | RustCode has Error enum with provider variants | P1 |
| Provider | Model status | `packages/opencode/src/provider/model-status.ts` | `provider.rs` | ✅ Full | Both have Alpha/Beta/Deprecated/Active | P1 |
| Provider | Transform functions | `packages/opencode/src/provider/transform.ts` | `provider.rs` | 🔶 Partial(80%) | RustCode has sanitize_surrogates, temperature defaults, top_p, top_k, sort_models | P1 |
| Provider | Reasoning effort | `packages/llm/src/schema/ids.ts` | `provider.rs` | ✅ Full | Both have ReasoningEffort enum | P1 |
| Provider | Finish reason | `packages/llm/src/schema/ids.ts` | `provider.rs` | ✅ Full | Both have FinishReason enum | P1 |
| Provider | LLM Events | `packages/llm/src/schema/events.ts` | `provider.rs` | ✅ Full | Both have LlmEvent tagged union with all variants | P1 |
| Provider | Chat message types | `packages/llm/src/schema/messages.ts` | `provider.rs` | ✅ Full | Both have ChatMessage, MessageContent, ContentPart | P1 |
| Provider | Token usage | `packages/llm/src/schema/events.ts` | `provider.rs` | ✅ Full | Both have Usage struct with cache fields | P1 |
| Provider | Model info | `packages/opencode/src/provider/provider.ts` | `provider.rs` | ✅ Full | Both have Model struct with cost, capabilities, limits | P1 |
| Provider | Bundled provider NPM | `packages/opencode/src/provider/provider.ts` | `provider.rs` | ✅ Full | Both list BUNDLED_PROVIDER_NPM | P1 |
| Provider | SDK key mapping | `packages/opencode/src/provider/transform.ts` | `provider.rs` | ✅ Full | Both map npm packages to AI SDK keys | P1 |
| Provider | Default temperature | `packages/opencode/src/provider/transform.ts` | `provider.rs` | ✅ Full | Both compute per-model default temperature | P1 |
| Provider | Model sort priority | `packages/opencode/src/provider/provider.ts` | `provider.rs` | ✅ Full | Both sort by gpt-5 > claude > big-pickle > gemini | P1 |

## 3. Tools

| Category | Feature | OpenCode Location | RustCode Location | Status | Gap Description | Priority |
|----------|---------|-------------------|-------------------|--------|-----------------|----------|
| Tool | Tool trait | `packages/opencode/src/tool/tool.ts`, `packages/core/src/tool/tool.ts` | `crates/rustcode-core/src/tool.rs` | ✅ Full | Both define Tool trait with id/description/execute/parameters_schema | P0 |
| Tool | Tool registry | `packages/opencode/src/tool/registry.ts`, `packages/core/src/tool/registry.ts` | `crates/rustcode-core/src/tool.rs` | 🔶 Partial(80%) | RustCode has ToolRegistry with register/execute; TS has Effect-based layers | P0 |
| Tool | Bash | `packages/core/src/tool/bash.ts`, `packages/opencode/src/tool/shell.ts` | `crates/rustcode-core/src/tool_impls.rs` (BashTool) | 🔶 Partial(80%) | RustCode executes /bin/sh with timeout, abort; TS version has richer shell handling | P0 |
| Tool | Read | `packages/core/src/tool/read.ts`, `packages/opencode/src/tool/read.ts` | `tool_impls.rs` (ReadTool) | 🔶 Partial(80%) | RustCode reads files with glob expansion, line ranges; TS richer | P0 |
| Tool | Write | `packages/core/src/tool/write.ts`, `packages/opencode/src/tool/write.ts` | `tool_impls.rs` (WriteTool) | 🔶 Partial(80%) | RustCode writes files; TS version with permission checks | P0 |
| Tool | Edit | `packages/core/src/tool/edit.ts`, `packages/opencode/src/tool/edit.ts` | `tool_impls.rs` (EditTool) | 🔶 Partial(80%) | RustCode has block-based search-and-replace; TS version has fuzzy matching | P0 |
| Tool | Glob | `packages/core/src/tool/glob.ts`, `packages/opencode/src/tool/glob.ts` | `tool_impls.rs` (GlobTool) | 🔶 Partial(80%) | RustCode globs with ignore rules | P0 |
| Tool | Grep | `packages/core/src/tool/grep.ts`, `packages/opencode/src/tool/grep.ts` | `tool_impls.rs` (GrepTool) | 🔶 Partial(80%) | RustCode uses ripgrep crate; TS uses ripgrep-js | P0 |
| Tool | WebFetch | `packages/core/src/tool/webfetch.ts`, `packages/opencode/src/tool/webfetch.ts` | `tool_impls.rs` (WebFetchTool) | 🔶 Partial(80%) | RustCode fetches URLs with markdown conversion | P0 |
| Tool | WebSearch | `packages/core/src/tool/websearch.ts`, `packages/opencode/src/tool/websearch.ts` | `tool_impls.rs` (WebSearchTool) | ⚠️ Partial(50%) | RustCode has search trait; no actual search backend wired (no Exa/Parallel API key) | P0 |
| Tool | ApplyPatch | `packages/core/src/tool/apply-patch.ts`, `packages/opencode/src/tool/apply_patch.ts` | `tool_impls.rs` (ApplyPatchTool) | 🔶 Partial(80%) | RustCode applies patches with similar crate; TS richer | P0 |
| Tool | Task | `packages/opencode/src/tool/task.ts` | `tool_impls.rs` (TaskTool) | 🔶 Partial(80%) | RustCode spawns sub-session; TS has background/foreground modes, subagent | P1 |
| Tool | Question | `packages/core/src/tool/question.ts`, `packages/opencode/src/tool/question.ts` | `tool_impls.rs` (QuestionTool) | 🔶 Partial(80%) | RustCode asks user question via stdin/event bus; TS richer | P0 |
| Tool | Skill | `packages/core/src/tool/skill.ts`, `packages/opencode/src/tool/skill.ts` | `tool_impls.rs` (SkillTool) | 🔶 Partial(80%) | RustCode discovers skills from .opencode/skills/*.md | P1 |
| Tool | TodoWrite | `packages/core/src/tool/todowrite.ts`, `packages/opencode/src/tool/todo.ts` | `tool_impls.rs` (TodoWriteTool) | ⚠️ Partial(50%) | RustCode writes todo items; TS version reads and writes todos | P1 |
| Tool | PlanEnter | `packages/opencode/src/tool/plan-enter.txt` | `tool_impls.rs` (PlanEnterTool) | 📝 Scaffold | RustCode has struct; handler placeholder | P2 |
| Tool | PlanExit | `packages/opencode/src/tool/plan.ts`, `plan-exit.txt` | `tool_impls.rs` (PlanExitTool) | 📝 Scaffold | RustCode has struct; handler placeholder | P2 |
| Tool | ExitPlanMode | ❌ Missing in TS (part of plan flow) | `tool_impls.rs` (ExitPlanModeTool) | 📝 Scaffold | RustCode-specific extra tool; no TS equivalent | P2 |
| Tool | Stash | ❌ Missing in TS standalone tool | `tool_impls.rs` (StashTool) | 📝 Scaffold | RustCode-specific extra tool; TS has stash in dialog-stash.tsx UI | P2 |
| Tool | NotebookEdit | ❌ Missing in TS standalone tool | `tool_impls.rs` (NotebookEditTool) | 📝 Scaffold | RustCode-specific extra tool | P2 |
| Tool | TaskOutput | ❌ Missing in TS standalone tool | `tool_impls.rs` (TaskOutputTool) | 📝 Scaffold | RustCode-specific extra tool | P2 |
| Tool | LSP | `packages/opencode/src/tool/lsp.ts` | `tool_impls.rs` (LspTool) | 📝 Scaffold | RustCode has struct; handler placeholder | P1 |
| Tool | Invalid | `packages/opencode/src/tool/invalid.ts` | `tool_impls.rs` (InvalidTool) | 🔶 Partial(80%) | Both return invalid argument errors | P2 |
| Tool | External Directory | `packages/opencode/src/tool/external-directory.ts` | ❌ Missing | No external directory tool in RustCode | P2 |
| Tool | MCP WebSearch | `packages/opencode/src/tool/mcp-websearch.ts` | ❌ Missing | No MCP-powered web search in RustCode | P2 |
| Tool | Tool schema | `packages/opencode/src/tool/schema.ts`, `packages/opencode/src/tool/json-schema.ts` | `crates/rustcode-core/src/tool.rs` | 🔶 Partial(80%) | RustCode has ToolDefinition, JSON schema, truncation | P1 |
| Tool | Tool truncation | `packages/opencode/src/tool/truncate.ts`, `truncation-dir.ts` | `crates/rustcode-core/src/tool.rs` (TruncateConfig) | ⚠️ Partial(50%) | RustCode has TruncateConfig struct but not full truncation logic | P1 |
| Tool | Tool output store | ❌ Missing in TS standalone | `crates/rustcode-core/src/tool_output_store.rs` | 📝 Scaffold | RustCode has dedicated output store module | P2 |
| Tool | Tool streaming | `packages/opencode/src/tool/stream.transport.ts` | `crates/rustcode-core/src/tool_stream.rs` | 📝 Scaffold | RustCode has tool_stream module | P2 |
| Tool | Tool file mutation | `packages/core/src/tool/edit.ts` (partial) | `crates/rustcode-core/src/file_mutation.rs` | 📝 Scaffold | RustCode has dedicated file mutation module | P2 |
| Tool | Tool shell | `packages/opencode/src/tool/shell/` | `crates/rustcode-core/src/shell.rs` | 📝 Scaffold | RustCode has shell module | P2 |
| Tool | Tool application-tools | `packages/core/src/tool/application-tools.ts` | ❌ Missing | No application-tools in RustCode | P2 |
| Tool | Tool builtins layer | `packages/core/src/tool/builtins.ts` | ❌ Missing | No builtins layer composition in RustCode | P2 |
| Tool | Tool registry (core) | `packages/core/src/tool/registry.ts` | ❌ Missing | RustCode tool registry is simpler, not Effect-based | P2 |
| Tool | Tool context | `packages/opencode/src/tool/tool.ts` | `crates/rustcode-core/src/tool.rs` | ✅ Full | Both have ToolContext with session_id, message_id, agent, abort, call_id | P0 |

## 4. Core Systems

| Category | Feature | OpenCode Location | RustCode Location | Status | Gap Description | Priority |
|----------|---------|-------------------|-------------------|--------|-----------------|----------|
| Core | Error types | `packages/core/src/error.ts` | `crates/rustcode-core/src/error.rs` | 🔶 Partial(80%) | RustCode has Error enum with 14+ variants; TS uses Effect system | P0 |
| Core | ID generation | `packages/core/src/id/id.ts` | `crates/rustcode-core/src/id.rs` | 🔶 Partial(80%) | RustCode has ascending/descending ID generation | P0 |
| Core | Environment | `packages/opencode/src/env/` | `crates/rustcode-core/src/env.rs` | 📝 Scaffold | RustCode has basic Env HashMap wrapper | P1 |
| Core | Event bus | `packages/opencode/src/bus/global.ts` | `crates/rustcode-core/src/bus.rs` | 🔶 Partial(80%) | RustCode has SharedBus with broadcast channel and GlobalEvent | P0 |
| Core | Event system | `packages/core/src/event/` | `crates/rustcode-core/src/event.rs` | 📝 Scaffold | RustCode has event module for event-sourced events | P1 |
| Core | Config loading | `packages/opencode/src/config/config.ts` | `crates/rustcode-core/src/config.rs` | 🔶 Partial(80%) | RustCode loads from ~/.config/opencode/opencode.jsonc, opencode.json, .opencode/, env vars | P0 |
| Core | Config schema | `packages/core/src/v1/config/config.ts` | `crates/rustcode-core/src/config.rs` (Info) | 🔶 Partial(80%) | RustCode has all config fields; not all validated | P0 |
| Core | Permission system | `packages/opencode/src/permission/`, `packages/core/src/permission/` | `crates/rustcode-core/src/permission.rs` | 🔶 Partial(80%) | RustCode has Allow/Deny/Ask, wildcard matching, rules, permission service with event bus | P0 |
| Core | Saved permissions | `packages/core/src/permission/saved.ts` | `permission.rs` | 🔶 Partial(80%) | RustCode has SavedPermissions with DB CRUD | P1 |
| Core | Permission schema | `packages/core/src/permission/schema.ts` | `permission.rs` | ✅ Full | Both have PermissionAction, PermissionRule, PermissionRuleset | P1 |
| Core | Permission SQL | `packages/core/src/permission/sql.ts` | `permission.rs` | 🔶 Partial(80%) | RustCode has DB table definitions and queries | P1 |
| Core | Storage | `packages/opencode/src/storage/` | `crates/rustcode-core/src/storage.rs` | 📝 Scaffold | RustCode has Storage trait (JSON file-based) + Database (SQLite) | P1 |
| Core | Session system | `packages/opencode/src/session/` | `crates/rustcode-core/src/session.rs` | 🔶 Partial(80%) | RustCode has Session, SessionManager, message types, processor, error types | P0 |
| Core | Session V2 core | `packages/core/src/session/` | `crates/rustcode-core/src/v2_schema.rs` | 📝 Scaffold | RustCode has v2_schema module; core V2 session types not ported | P1 |
| Core | Session runner | `packages/core/src/session/runner/` | `crates/rustcode-core/src/session_runner.rs` | 🔶 Partial(80%) | RustCode has SessionRunner with run/run_with_messages; TS has Effect-based | P0 |
| Core | Session prompt | `packages/opencode/src/session/prompt.ts` | `crates/rustcode-core/src/session_prompt.rs` | 🔶 Partial(80%) | RustCode has PromptPart, SessionPromptInput, system prompt builder | P1 |
| Core | Session execution | `packages/core/src/session/execution/` | `crates/rustcode-core/src/session_execution.rs` | 📝 Scaffold | RustCode has session_execution module | P1 |
| Core | Session history | `packages/core/src/session/history.ts` | `crates/rustcode-core/src/session_history.rs` | 📝 Scaffold | RustCode has session_history module | P1 |
| Core | Session info | `packages/core/src/session/info.ts` | `crates/rustcode-core/src/session_info.rs` | 📝 Scaffold | RustCode has session_info module | P1 |
| Core | Session message | `packages/core/src/session/message.ts` | `crates/rustcode-core/src/session_message.rs` | 📝 Scaffold | RustCode has session_message module | P1 |
| Core | Session compaction | `packages/opencode/src/session/compaction.ts` | `crates/rustcode-core/src/session_compaction.rs` | 📝 Scaffold | RustCode has session_compaction module | P1 |
| Core | Session todo | `packages/opencode/src/tool/todo.ts`, `packages/core/src/tool/todowrite.ts` | `crates/rustcode-core/src/session_todo.rs` | 📝 Scaffold | RustCode has session_todo module | P2 |
| Core | Session context epoch | `packages/core/src/session/context-epoch.ts` | `crates/rustcode-core/src/system_context.rs` | 📝 Scaffold | RustCode has system_context module | P2 |
| Core | Agent system | `packages/opencode/src/agent/` | `crates/rustcode-core/src/agent.rs` | 🔶 Partial(80%) | RustCode has Agent, AgentMode, AgentService; not all TS features | P1 |
| Core | Plugin system | `packages/opencode/src/plugin/`, `packages/core/src/plugin/` | `crates/rustcode-core/src/plugin.rs` | 📝 Scaffold | RustCode has PluginManager, Plugin trait; not wired | P1 |
| Core | Skill system | `packages/opencode/src/skill/`, `packages/core/src/skill/` | `crates/rustcode-core/src/skill.rs` | 🔶 Partial(80%) | RustCode discovers skills from .opencode/skills/*.md; TS richer | P1 |
| Core | Skill catalog dir | `packages/core/src/plugin/skill/` | `crates/rustcode-core/src/skill/` | 📝 Scaffold | RustCode has skill submodule for catalog | P2 |
| Core | Snapshot | `packages/opencode/src/snapshot/` | `crates/rustcode-core/src/snapshot.rs` | 📝 Scaffold | RustCode has Snapshot, SnapshotService | P1 |
| Core | Git | `packages/opencode/src/git/` | `crates/rustcode-core/src/git.rs` | 📝 Scaffold | RustCode has Git status/diff/worktree | P1 |
| Core | Worktree | `packages/opencode/src/worktree/` | `crates/rustcode-core/src/worktree.rs` | 📝 Scaffold | RustCode has worktree module | P1 |
| Core | Workspace | `packages/core/src/workspace/` | `crates/rustcode-core/src/workspace.rs` | 📝 Scaffold | RustCode has workspace module | P2 |
| Core | Project | `packages/core/src/project/` | `crates/rustcode-core/src/project.rs` | 📝 Scaffold | RustCode has project module | P2 |
| Core | Location | `packages/core/src/location/` | `crates/rustcode-core/src/location.rs` | 📝 Scaffold | RustCode has location module for file paths | P2 |
| Core | State | `packages/opencode/src/state/` | `crates/rustcode-core/src/state.rs` | 📝 Scaffold | RustCode has state module | P2 |
| Core | Flag | `packages/core/src/flag/` | `crates/rustcode-core/src/flag.rs` | 📝 Scaffold | RustCode has flag module | P2 |
| Core | Format | `packages/opencode/src/format/` | `crates/rustcode-core/src/format.rs` | 📝 Scaffold | RustCode has token/cost formatting utilities | P2 |
| Core | Image/MIME | `packages/opencode/src/image/` | `crates/rustcode-core/src/image.rs` | 📝 Scaffold | RustCode has MIME type detection | P2 |
| Core | Question | `packages/opencode/src/question/` | `crates/rustcode-core/src/question.rs` | 📝 Scaffold | RustCode has question service | P1 |
| Core | Command | `packages/core/src/command/` | `crates/rustcode-core/src/command.rs` | 📝 Scaffold | RustCode has command definitions | P2 |
| Core | Integration | `packages/core/src/integration/` | `crates/rustcode-core/src/integration.rs` | 📝 Scaffold | RustCode has integration service for OAuth/API-key connections | P2 |
| Core | Reference | `packages/core/src/reference/` | `crates/rustcode-core/src/reference.rs` | 📝 Scaffold | RustCode has reference service | P2 |
| Core | Repository | `packages/core/src/repository.ts` | `crates/rustcode-core/src/repository.rs` | 📝 Scaffold | RustCode has repository module | P2 |
| Core | Ripgrep | `packages/opencode/src/ripgrep/` | `crates/rustcode-core/src/ripgrep.rs` | 📝 Scaffold | RustCode has ripgrep wrapper module | P2 |
| Core | NPM | `packages/opencode/src/npm/` | `crates/rustcode-core/src/npm.rs` | 📝 Scaffold | RustCode has npm module | P2 |
| Core | Process | `packages/opencode/src/process/` | `crates/rustcode-core/src/process.rs` | 📝 Scaffold | RustCode has process module | P2 |
| Core | PTY | `packages/opencode/src/pty/` | `crates/rustcode-core/src/pty.rs` | 📝 Scaffold | RustCode has pty module | P2 |
| Core | Policy | `packages/core/src/policy/` | `crates/rustcode-core/src/policy.rs` | 📝 Scaffold | RustCode has policy module | P2 |
| Core | Catalog | `packages/opencode/src/catalog/` | `crates/rustcode-core/src/catalog.rs` | 📝 Scaffold | RustCode has catalog module for built-in plugins | P2 |
| Core | Account | `packages/opencode/src/account/` | `crates/rustcode-core/src/account.rs` | 📝 Scaffold | RustCode has account module for console accounts | P2 |
| Core | Background job | `packages/opencode/src/background/` | `crates/rustcode-core/src/background_job.rs` | 📝 Scaffold | RustCode has background job module | P2 |
| Core | Credential | `packages/core/src/credential/` | `crates/rustcode-core/src/credential.rs` | 📝 Scaffold | RustCode has credential module | P2 |
| Core | Filesystem | `packages/core/src/filesystem/` | `crates/rustcode-core/src/filesystem.rs` | 📝 Scaffold | RustCode has filesystem module | P2 |
| Core | FS Util | `packages/core/src/fs-util.ts` | `crates/rustcode-core/src/fs_util.rs` | 📝 Scaffold | RustCode has fs_util module | P2 |
| Core | Global | `packages/core/src/global.ts` | `crates/rustcode-core/src/global.rs` | 📝 Scaffold | RustCode has global module for app constants | P2 |
| Core | Instruction context | `packages/opencode/src/instruction/` | `crates/rustcode-core/src/instruction_context.rs` | 📝 Scaffold | RustCode has instruction_context module | P2 |
| Core | Model | `packages/opencode/src/model/` | `crates/rustcode-core/src/model.rs` | 📝 Scaffold | RustCode has model module | P2 |
| Core | Observability | `packages/opencode/src/observability/` | `crates/rustcode-core/src/observability.rs` | 📝 Scaffold | RustCode has observability/tracing module | P2 |
| Core | Patch | `packages/core/src/patch/` | `crates/rustcode-core/src/patch.rs` | 📝 Scaffold | RustCode has patch module | P2 |
| Core | Runtime | `packages/opencode/src/runtime/` | `crates/rustcode-core/src/runtime.rs` | 🔶 Partial(80%) | RustCode has initialize_runtime that discovers providers, creates session manager | P0 |
| Core | Schema | `packages/core/src/schema/`, `packages/llm/src/schema/` | `crates/rustcode-core/src/schema.rs` | 📝 Scaffold | RustCode has schema module | P2 |
| Core | SSE | `packages/opencode/src/server/event.ts` | `crates/rustcode-core/src/sse.rs` | 📝 Scaffold | RustCode has SSE event module | P1 |
| Core | Model catalog | `packages/core/src/plugin/models-dev.ts` | `crates/rustcode-core/src/catalog.rs` | 📝 Scaffold | RustCode not connected to models.dev API | P2 |
| Core | AISDK | `packages/llm/src/` (various) | `crates/rustcode-core/src/aisdk.rs` | 📝 Scaffold | RustCode has aisdk module for AI SDK compatibility | P2 |

## 5. Crate/Package Mapping

| Category | Feature | OpenCode Location | RustCode Location | Status | Gap Description | Priority |
|----------|---------|-------------------|-------------------|--------|-----------------|----------|
| Mapping | Main CLI | `packages/opencode/src/index.ts` | `src/main.rs` + `Cargo.toml` | ✅ Full | Both are binary entry points | P0 |
| Mapping | Core library | `packages/core/src/` | `crates/rustcode-core/` | ⚠️ Partial(50%) | RustCode core has 70+ modules with type scaffolds; implementations WIP | P0 |
| Mapping | Server | `packages/server/src/` | `crates/rustcode-server/` | ⚠️ Partial(50%) | RustCode server has axum-based router, routes, SSE, AppState; not full TS parity | P0 |
| Mapping | TUI | `packages/tui/src/` | `crates/rustcode-tui/` | ⚠️ Partial(50%) | RustCode TUI uses ratatui; TS uses React/Ink | P0 |
| Mapping | LSP | `packages/opencode/src/lsp/` | `crates/rustcode-lsp/` | ⚠️ Partial(50%) | RustCode LSP has LspManager, LspClient, diagnostics, symbols | P1 |
| Mapping | MCP | `packages/opencode/src/mcp/` | `crates/rustcode-mcp/` | ⚠️ Partial(50%) | RustCode MCP has stdio/http transport, discovery, OAuth, tool executor | P1 |
| Mapping | LLM | `packages/llm/src/` | (in rustcode-core) | 🔶 Partial(80%) | RustCode merged LLM protocol layer into rustcode-core provider module | P0 |
| Mapping | Web frontend | `packages/web/src/` | ❌ Missing | No RustCode web frontend; RustCode serves API | P2 |
| Mapping | App (desktop) | `packages/app/src/` | ❌ Missing | No RustCode desktop app | P3 |
| Mapping | UI components | `packages/ui/src/` | ❌ Missing | No RustCode shared UI component library | P3 |
| Mapping | Console | `packages/console/` | ❌ Missing | No RustCode console backend | P3 |
| Mapping | SDK | `packages/sdk/js/` | ❌ Missing | No RustCode SDK; TS has JavaScript SDK + OpenAPI spec | P2 |
| Mapping | Stats | `packages/stats/` | ❌ Missing | No RustCode separate stats package; merged into main binary | P2 |
| Mapping | Identity | `packages/identity/` | ❌ Missing | No RustCode identity package | P3 |
| Mapping | Desktop | `packages/desktop/` | ❌ Missing | No RustCode desktop application | P3 |
| Mapping | Slack | `packages/slack/` | ❌ Missing | No RustCode Slack integration | P3 |
| Mapping | Enterprise | `packages/enterprise/` | ❌ Missing | No RustCode enterprise features | P3 |
| Mapping | Function | `packages/function/` | ❌ Missing | No RustCode function package | P3 |
| Mapping | Containers | `packages/containers/` | ❌ Missing | No RustCode container support | P3 |
| Mapping | Script | `packages/script/` | ❌ Missing | No RustCode script utilities | P3 |
| Mapping | HTTP Recorder | `packages/http-recorder/` | ❌ Missing | No RustCode HTTP recording | P3 |
| Mapping | Docs | `packages/docs/` | ❌ Missing | No RustCode documentation package | P3 |
| Mapping | Storybook | `packages/storybook/` | ❌ Missing | No RustCode storybook | P3 |
| Mapping | Effect Drizzle SQLite | `packages/effect-drizzle-sqlite/` | ❌ Missing | RustCode uses sqlx directly, no Effect abstraction | P2 |
| Mapping | Effect SQLite Node | `packages/effect-sqlite-node/` | ❌ Missing | Same as above | P2 |
| Mapping | Plugin CLI/package | `packages/plugin/` | ❌ Missing | No RustCode standalone plugin package | P2 |
| Mapping | CLI (subpackage) | `packages/cli/` | ❌ Missing | RustCode has no separate CLI subpackage | P2 |

## 6. Database Features

| Category | Feature | OpenCode Location | RustCode Location | Status | Gap Description | Priority |
|----------|---------|-------------------|-------------------|--------|-----------------|----------|
| DB | SQLite driver | `packages/core/src/database/sqlite.ts` | `sqlx` (workspace dep) | ✅ Full | Both use SQLite; TS uses drizzle-orm, Rust uses sqlx | P0 |
| DB | Database path | `packages/core/src/database/path.ts` | `crates/rustcode-core/src/database.rs` | ✅ Full | RustCode computes path from XDG data dir + OPENCODE_DB env | P0 |
| DB | Connection PRAGMAs | `packages/core/src/database/database.ts` | `database.rs` | ✅ Full | Both set WAL, synchronous=NORMAL, foreign_keys=ON, etc. | P0 |
| DB | Migration system | `packages/core/src/database/migration.ts` | `database.rs` | 📝 Scaffold | RustCode has Migration/MigrationMeta types; no migration runner | P1 |
| DB | Migration gen | `packages/core/src/database/migration.gen.ts` | ❌ Missing | No auto-generated migration code in RustCode | P2 |
| DB | Migration files | `packages/core/src/database/migration/` | ❌ Missing | No migration SQL files in RustCode | P2 |
| DB | Schema gen | `packages/core/src/database/schema.gen.ts` | ❌ Missing | No auto-generated schema in RustCode | P2 |
| DB | Schema SQL | `packages/core/src/database/schema.sql.ts` | `database.rs` | 📝 Scaffold | RustCode has SQL SHOW CREATE TABLE equivalents | P1 |
| DB | Session table | `packages/core/src/database/schema.sql.ts` | `database.rs` | 🔶 Partial(80%) | RustCode defines all table schemas as constants | P0 |
| DB | Message table | `schema.sql.ts` | `database.rs` | 🔶 Partial(80%) | Same as above | P0 |
| DB | Part table | `schema.sql.ts` | `database.rs` | 🔶 Partial(80%) | Same as above | P0 |
| DB | Project table | `schema.sql.ts` | `database.rs` | ✅ Full | RustCode defines project table | P1 |
| DB | Workspace table | `schema.sql.ts` | `database.rs` | ✅ Full | RustCode defines workspace table | P1 |
| DB | Todo table | `schema.sql.ts` | `database.rs` | ✅ Full | RustCode defines todo table | P1 |
| DB | Account table | `schema.sql.ts` | `database.rs` | ✅ Full | RustCode defines account table | P2 |
| DB | Credential table | `schema.sql.ts` | `database.rs` | ✅ Full | RustCode defines credential table | P2 |
| DB | Permission table | `schema.sql.ts` | `database.rs` | ✅ Full | RustCode defines permission table | P2 |
| DB | Event table | `packages/core/src/event/` | `database.rs` | ✅ Full | RustCode defines event and event_sequence tables | P2 |
| DB | Data migration table | `schema.sql.ts` | `database.rs` | ✅ Full | RustCode defines data_migration table | P2 |
| DB | Migration journal | `schema.sql.ts` | `database.rs` | ✅ Full | RustCode defines migration table | P2 |
| DB | Session share table | `schema.sql.ts` | `database.rs` | ✅ Full | RustCode defines session_share table | P2 |
| DB | Session input table | `schema.sql.ts` | `database.rs` | ✅ Full | RustCode defines session_input table | P2 |
| DB | Session context epoch | `schema.sql.ts` | `database.rs` | ✅ Full | RustCode defines session_context_epoch table | P2 |
| DB | Session message table | `schema.sql.ts` | `database.rs` | ✅ Full | RustCode defines session_message table | P2 |
| DB | Control account table | `schema.sql.ts` | `database.rs` | ✅ Full | RustCode defines control_account table | P2 |
| DB | Account state table | `schema.sql.ts` | `database.rs` | ✅ Full | RustCode defines account_state table | P2 |
| DB | Data migration SQL | `packages/core/src/data-migration.sql.ts` | `database.rs` | 📝 Scaffold | RustCode has data migration types but no runner | P2 |

## 7. LSP Features

| Category | Feature | OpenCode Location | RustCode Location | Status | Gap Description | Priority |
|----------|---------|-------------------|-------------------|--------|-----------------|----------|
| LSP | LSP client | `packages/opencode/src/lsp/client.ts` | `crates/rustcode-lsp/src/lib.rs` | 🔶 Partial(80%) | RustCode has LspClient with process management, JSON-RPC framing | P1 |
| LSP | LSP server manager | `packages/opencode/src/lsp/server.ts` | `rustcode-lsp/src/lib.rs` (LspManager) | 🔶 Partial(80%) | RustCode auto-detects and manages language servers | P1 |
| LSP | LSP launcher | `packages/opencode/src/lsp/launch.ts` | (in LspManager) | 🔶 Partial(80%) | RustCode launches LSP servers as child processes | P1 |
| LSP | Language ID detection | `packages/opencode/src/lsp/language.ts` | `rustcode-lsp/src/lib.rs` | ✅ Full | Both map file extensions to language IDs | P1 |
| LSP | Diagnostics | `packages/opencode/src/lsp/diagnostic.ts` | `rustcode-lsp/src/lib.rs` | 🔶 Partial(80%) | RustCode has LspDiagnostic type and collection | P1 |
| LSP | Document symbols | `packages/opencode/src/lsp/lsp.ts` | `rustcode-lsp/src/lib.rs` | 🔶 Partial(80%) | RustCode has LspDocumentSymbol type | P1 |
| LSP | Workspace symbols | `packages/opencode/src/lsp/lsp.ts` | `rustcode-lsp/src/lib.rs` | 🔶 Partial(80%) | RustCode has LspSymbol type | P1 |
| LSP | Go to definition | `packages/opencode/src/tool/lsp.ts` | ❌ Missing | Not in LSP tool handler | P2 |
| LSP | Find references | `lsp.ts` | ❌ Missing | Same as above | P2 |
| LSP | Hover | `lsp.ts` | ❌ Missing | Same as above | P2 |
| LSP | Go to implementation | `lsp.ts` | ❌ Missing | Same as above | P2 |
| LSP | Call hierarchy | `lsp.ts` | ❌ Missing | Same as above | P2 |
| LSP | Core LSP types | `packages/opencode/src/lsp/lsp.ts` | `crates/rustcode-core/src/lsp.rs` | 🔶 Partial(80%) | RustCode core has LspClientInfo, LspServerInfo, LspStatus, LspDiagnostic | P1 |

## 8. MCP Features

| Category | Feature | OpenCode Location | RustCode Location | Status | Gap Description | Priority |
|----------|---------|-------------------|-------------------|--------|-----------------|----------|
| MCP | MCP client | `packages/opencode/src/mcp/index.ts` | `crates/rustcode-core/src/mcp.rs` | 🔶 Partial(80%) | RustCode has McpClient with JSON-RPC request/response | P1 |
| MCP | MCP server registry | `packages/opencode/src/mcp/catalog.ts` | `crates/rustcode-core/src/mcp.rs` | 🔶 Partial(80%) | RustCode has McpServerRegistry with add/remove/discover | P1 |
| MCP | OAuth auth flow | `packages/opencode/src/mcp/auth.ts` | `crates/rustcode-mcp/src/lib.rs` | ⚠️ Partial(50%) | RustCode has OAuth discovery, browser opening, token storage; callback server not wired | P1 |
| MCP | OAuth callback | `packages/opencode/src/mcp/oauth-callback.ts` | ❌ Missing | No OAuth callback HTTP server in RustCode | P1 |
| MCP | OAuth provider | `packages/opencode/src/mcp/oauth-provider.ts` | ❌ Missing | No OAuth provider abstraction in RustCode | P2 |
| MCP | MCP transport | (in core mcp) | `crates/rustcode-mcp/src/lib.rs` | 🔶 Partial(80%) | RustCode has McpTransport trait, StdioTransport, HttpTransport | P1 |
| MCP | MCP tool executor | (in tool registry) | `crates/rustcode-mcp/src/lib.rs` (McpToolExecutor) | 📝 Scaffold | RustCode has McpToolExecutor for wrapping MCP tools | P1 |
| MCP | MCP discovery | `packages/opencode/src/mcp/catalog.ts` | `crates/rustcode-mcp/src/lib.rs` (McpDiscovery) | 🔶 Partial(80%) | RustCode discovers from config files, env vars | P1 |
| MCP | MCP tool registration | (in tool registry) | `crates/rustcode-core/src/mcp.rs` | 📝 Scaffold | RustCode has McpTool, McpServerSummary types | P1 |
| MCP | JSON-RPC types | `packages/opencode/src/mcp/index.ts` | `crates/rustcode-core/src/mcp.rs` | ✅ Full | Both have JsonRpcRequest, JsonRpcResponse, JsonRpcError | P1 |
| MCP | Core MCP types | ❌ Missing in TS core | `crates/rustcode-core/src/mcp.rs` | ✅ Full | RustCode has dedicated mcp module in core | P1 |

## 9. Server API Features

| Category | Feature | OpenCode Location | RustCode Location | Status | Gap Description | Priority |
|----------|---------|-------------------|-------------------|--------|-----------------|----------|
| Server | HTTP server | `packages/server/src/server.ts` | `crates/rustcode-server/src/server.rs` | 🔶 Partial(80%) | RustCode uses axum 0.8; TS uses express-like server | P0 |
| Server | Router | `packages/server/src/routes.ts` | `crates/rustcode-server/src/routes/` | 🔶 Partial(80%) | RustCode has modular route handlers | P0 |
| Server | CORS | `packages/server/src/cors.ts` | `crates/rustcode-server/src/cors.rs` | ✅ Full | Both have CORS configuration | P0 |
| Server | SSE event streaming | `packages/opencode/src/server/event.ts` | `crates/rustcode-server/src/sse.rs` | 🔶 Partial(80%) | RustCode has SSE endpoint for bus events | P0 |
| Server | Auth middleware | `packages/server/src/auth.ts` | (in server crate) | 📝 Scaffold | RustCode checks OPENCODE_SERVER_PASSWORD; full auth not implemented | P1 |
| Server | Health endpoint | `packages/server/src/routes/health.ts` | `crates/rustcode-server/src/routes/` (health) | ✅ Full | Both have /api/health | P0 |
| Server | Agent endpoints | `packages/server/src/handlers/agent.ts` | `server routes` | 📝 Scaffold | Not fully wired in RustCode | P1 |
| Server | Command endpoints | `packages/server/src/handlers/command.ts` | `server routes` | 📝 Scaffold | Same as above | P2 |
| Server | Credential endpoints | `packages/server/src/handlers/credential.ts` | `server routes` | 📝 Scaffold | Same as above | P2 |
| Server | Event endpoints | `packages/server/src/handlers/event.ts` | `server routes` | 📝 Scaffold | Same as above | P2 |
| Server | FS endpoints | `packages/server/src/handlers/fs.ts` | `server routes` | 📝 Scaffold | Same as above | P2 |
| Server | Integration endpoints | `packages/server/src/handlers/integration.ts` | `server routes` | 📝 Scaffold | Same as above | P2 |
| Server | Location endpoints | `packages/server/src/handlers/location.ts` | `server routes` | 📝 Scaffold | Same as above | P2 |
| Server | Message endpoints | `packages/server/src/handlers/message.ts` | `server routes` | 📝 Scaffold | Same as above | P1 |
| Server | Model endpoints | `packages/server/src/handlers/model.ts` | `server routes` | 📝 Scaffold | Same as above | P1 |
| Server | Permission endpoints | `packages/server/src/handlers/permission.ts` | `server routes` | 📝 Scaffold | Same as above | P1 |
| Server | Provider endpoints | `packages/server/src/handlers/provider.ts` | `server routes` | 📝 Scaffold | Same as above | P1 |
| Server | PTY endpoints | `packages/server/src/handlers/pty.ts` | `server routes` | 📝 Scaffold | Same as above | P2 |
| Server | Question endpoints | `packages/server/src/handlers/question.ts` | `server routes` | 📝 Scaffold | Same as above | P1 |
| Server | Reference endpoints | `packages/server/src/handlers/reference.ts` | `server routes` | 📝 Scaffold | Same as above | P2 |
| Server | Session endpoints | `packages/server/src/handlers/session.ts` | `server routes` | 📝 Scaffold | RustCode has session CRUD routes in progress | P1 |
| Server | Skill endpoints | `packages/server/src/handlers/skill.ts` | `server routes` | 📝 Scaffold | Same as above | P2 |
| Server | Project copy endpoint | `packages/server/src/handlers/project-copy.ts` | `server routes` | 📝 Scaffold | Same as above | P2 |
| Server | mDNS | `packages/opencode/src/server/mdns.ts` | (in main.rs) | ✅ Full | Both have mDNS service discovery flags | P1 |
| Server | API spec | `packages/sdk/openapi.json` | ❌ Missing | No RustCode OpenAPI spec | P2 |
| Server | Error handling | `packages/server/src/errors.ts` | `server crate` | 📝 Scaffold | Basic error handling; not full TS parity | P1 |
| Server | Groups | `packages/server/src/groups/` | ❌ Missing | No endpoint grouping in RustCode | P2 |
| Server | Shared utils | `packages/server/src/shared/` | ❌ Missing | No shared server utilities in RustCode | P2 |
| Server | Middleware | `packages/server/src/middleware/` | ❌ Missing | No custom middleware layer in RustCode | P2 |
| Server | AppState | (in server routes) | `crates/rustcode-server/src/server.rs` | ✅ Full | RustCode has AppState with bus, sessions, tools, providers, permissions, runner | P0 |
| Server | Agent service | `packages/opencode/src/server/agent.ts` | `server.rs` (build_agent_service) | 🔶 Partial(80%) | RustCode builds agent service from config | P1 |
| Server | Command data | `packages/opencode/src/server/command.ts` | `server.rs` (build_command_data) | 🔶 Partial(80%) | RustCode builds command data from config | P1 |
| Server | Integration service | `packages/opencode/src/server/integration.ts` | `server.rs` | 📝 Scaffold | RustCode has empty integration service | P2 |
| Server | Reference service | `packages/opencode/src/server/reference.ts` | `server.rs` | 📝 Scaffold | RustCode has reference service | P2 |

## 10. TUI/UI Features

| Category | Feature | OpenCode Location | RustCode Location | Status | Gap Description | Priority |
|----------|---------|-------------------|-------------------|--------|-----------------|----------|
| TUI | Terminal UI | `packages/tui/src/index.tsx` | `crates/rustcode-tui/src/app.rs` | ⚠️ Partial(50%) | RustCode uses ratatui; TS uses React/Ink; RustCode has basic app shell | P0 |
| TUI | Conversation view | `packages/tui/src/component/` | `crates/rustcode-tui/src/components/` | 📝 Scaffold | RustCode has component module; not full conversation rendering | P1 |
| TUI | Input area | `packages/tui/src/component/prompt/` | `rustcode-tui/src/components/` | 📝 Scaffold | Same as above | P1 |
| TUI | Status line | `packages/tui/src/context/` | `rustcode-tui/src/components/` | 📝 Scaffold | Same as above | P1 |
| TUI | Permission prompt | `packages/tui/src/component/` | `rustcode-tui/src/components/` | 📝 Scaffold | Same as above | P1 |
| TUI | Question prompt | `packages/tui/src/component/` | `rustcode-tui/src/components/` | 📝 Scaffold | Same as above | P1 |
| TUI | Theme system | `packages/tui/src/theme/` | `crates/rustcode-tui/src/theme.rs` | 📝 Scaffold | RustCode has theme module | P2 |
| TUI | Keymap | `packages/tui/src/keymap.tsx` | `crates/rustcode-tui/src/keymap.rs` | 📝 Scaffold | RustCode has keymap module | P2 |
| TUI | Clipboard | `packages/tui/src/clipboard.ts` | `crates/rustcode-tui/src/clipboard.rs` | 📝 Scaffold | RustCode has clipboard module | P2 |
| TUI | SSE event client | `packages/tui/src/context/` | `crates/rustcode-tui/src/sse_client.rs` | 📝 Scaffold | RustCode has SseClient | P2 |
| TUI | Editor integration | `packages/tui/src/editor.ts` | `crates/rustcode-tui/src/editor.rs` | 📝 Scaffold | RustCode has editor module | P2 |
| TUI | Audio | `packages/tui/src/audio.ts` | ❌ Missing | No audio support in RustCode TUI | P3 |
| TUI | Dialogs (model, agent, etc.) | `packages/tui/src/component/dialog-*.tsx` | ❌ Missing | No dialog components in RustCode TUI | P2 |
| TUI | Plugin adapters | `packages/tui/src/plugin/` | ❌ Missing | No TUI plugin system in RustCode | P2 |
| TUI | Feature plugins | `packages/tui/src/feature-plugins/` | ❌ Missing | No feature plugins in RustCode | P2 |
| TUI | Event system | `packages/tui/src/context/event.ts` | `crates/rustcode-tui/src/event.rs` | 📝 Scaffold | RustCode has event module | P2 |
| TUI | Web frontend | `packages/web/src/` | ❌ Missing | No web frontend in RustCode | P2 |
| TUI | Desktop app | `packages/app/src/` | ❌ Missing | No desktop app in RustCode | P3 |

## 11. Plugin System

| Category | Feature | OpenCode Location | RustCode Location | Status | Gap Description | Priority |
|----------|---------|-------------------|-------------------|--------|-----------------|----------|
| Plugin | Plugin manager | `packages/opencode/src/plugin/` | `crates/rustcode-core/src/plugin.rs` | 📝 Scaffold | RustCode has PluginManager skeleton; not wired | P1 |
| Plugin | Plugin trait | `packages/core/src/plugin/provider.ts` | `plugin.rs` | 📝 Scaffold | RustCode has Plugin trait; not implemented | P1 |
| Plugin | Plugin loading | `packages/core/src/plugin/boot.ts` | ❌ Missing | No plugin boot/loading in RustCode | P1 |
| Plugin | Plugin events | `packages/opencode/src/plugin/` | ❌ Missing | Plugin event hooks not in RustCode | P2 |
| Plugin | Plugin layers | `packages/core/src/plugin/layer-map.example.ts` | ❌ Missing | No plugin layer map in RustCode | P2 |
| Plugin | Plugin command | `packages/core/src/plugin/command.ts` | ❌ Missing | No plugin command system in RustCode | P2 |
| Plugin | Plugin boot | `packages/core/src/plugin/boot.ts` | ❌ Missing | No plugin boot sequence in RustCode | P2 |

## 12. Testing Infrastructure

| Category | Feature | OpenCode Location | RustCode Location | Status | Gap Description | Priority |
|----------|---------|-------------------|-------------------|--------|-----------------|----------|
| Test | Unit tests | Various `*.test.ts` files | `#[cfg(test)] mod tests` in source files | 🔶 Partial(80%) | RustCode has tests in provider.rs, permission.rs, etc.; not all modules | P1 |
| Test | Test runner | `bun test` | `cargo test` | ✅ Full | Both have test runners | P0 |
| Test | CI pipeline | GitHub Actions | `.github/workflows/ci.yml` | ✅ Full | RustCode has fmt + clippy + test + cargo-deny | P0 |
| Test | Linting | `tsgo --noEmit` | `cargo clippy -- -D warnings` | ✅ Full | Both have linting in CI | P0 |
| Test | Format check | biome/prettier | `cargo fmt --all -- --check` | ✅ Full | Both format-check in CI | P0 |
| Test | HTTP API tests | `packages/opencode/script/httpapi-exercise.ts` | ❌ Missing | No HTTP API test suite in RustCode | P2 |
| Test | Benchmark tests | `packages/opencode/script/bench-test-suite.ts` | ❌ Missing | No benchmark suite in RustCode | P3 |
| Test | Profile tests | `packages/opencode/script/profile-test-files.ts` | ❌ Missing | No profiling tests in RustCode | P3 |
| Test | Test fixtures | `packages/**/test/fixtures/` | ❌ Missing | No test fixture data in RustCode | P2 |
| Test | Integration tests | `packages/opencode/test/` | ❌ Missing | No integration test directory in RustCode | P2 |
| Test | Session tests | `packages/opencode/test/session/` | ❌ Missing | No session-specific tests in RustCode | P2 |
| Test | Server tests | `packages/opencode/test/server/` | ❌ Missing | No server tests in RustCode | P2 |
| Test | Control plane tests | `packages/opencode/test/control-plane/` | ❌ Missing | No control plane tests in RustCode | P2 |

## 13. Session V2 Features

| Category | Feature | OpenCode Location | RustCode Location | Status | Gap Description | Priority |
|----------|---------|-------------------|-------------------|--------|-----------------|----------|
| V2 | Session V2 core | `packages/core/src/session/` | `crates/rustcode-core/src/v2_schema.rs` | 📝 Scaffold | RustCode has v2_schema types; not wired | P1 |
| V2 | Event-sourced sessions | `packages/core/src/session/event.ts` | ❌ Missing | Event sourcing not implemented | P1 |
| V2 | Session store | `packages/core/src/session/store.ts` | ❌ Missing | No V2 session store | P1 |
| V2 | Session prompt (V2) | `packages/core/src/session/prompt.ts` | ❌ Missing | No V2 prompt admission | P2 |
| V2 | Session projector | `packages/core/src/session/projector.ts` | ❌ Missing | No V2 session projector | P2 |
| V2 | Session run coordinator | `packages/core/src/session/run-coordinator.ts` | ❌ Missing | No run coordinator | P2 |
| V2 | Session input | `packages/core/src/session/input.ts` | ❌ Missing | No session input inbox | P2 |
| V2 | Session message updater | `packages/core/src/session/message-updater.ts` | ❌ Missing | No message updater | P2 |
| V2 | Session message ID | `packages/core/src/session/message-id.ts` | ❌ Missing | No V2 message ID | P2 |
| V2 | Session logging | `packages/core/src/session/logging.ts` | ❌ Missing | No V2 logging | P2 |
| V2 | Session error | `packages/core/src/session/error.ts` | ❌ Missing | No V2 error types | P2 |

## 14. Additional Packages

| Category | Feature | OpenCode Location | RustCode Location | Status | Gap Description | Priority |
|----------|---------|-------------------|-------------------|--------|-----------------|----------|
| Extra | Slack package | `packages/slack/` | ❌ Missing | No RustCode Slack integration | P3 |
| Extra | Stats server | `packages/stats/server/` | ❌ Missing | No stats server in RustCode | P3 |
| Extra | Stats app | `packages/stats/app/` | ❌ Missing | No stats app in RustCode | P3 |
| Extra | Stats core | `packages/stats/core/` | ❌ Missing | No stats core in RustCode | P3 |
| Extra | Identity package | `packages/identity/` | ❌ Missing | No identity management in RustCode | P3 |
| Extra | Script utilities | `packages/script/` | ❌ Missing | No script utilities in RustCode | P3 |
| Extra | HTTP recorder | `packages/http-recorder/` | ❌ Missing | No HTTP recording in RustCode | P3 |
| Extra | Enterprise features | `packages/enterprise/` | ❌ Missing | No enterprise features in RustCode | P3 |
| Extra | Function package | `packages/function/` | ❌ Missing | No function package in RustCode | P3 |
| Extra | Containers | `packages/containers/` | ❌ Missing | No containers in RustCode | P3 |
| Extra | Desktop app | `packages/desktop/` | ❌ Missing | No desktop app in RustCode | P3 |
| Extra | Docs | `packages/docs/` | ❌ Missing | No documentation site in RustCode | P3 |
| Extra | Storybook | `packages/storybook/` | ❌ Missing | No component explorer in RustCode | P3 |

## Summary Statistics

| Category | Total Features | ✅ Full | 🔶 Partial(80%) | ⚠️ Partial(50%) | 📝 Scaffold | ❌ Missing |
|----------|---------------|--------|-----------------|-----------------|-------------|------------|
| 1. CLI Commands | 95 | 42 | 23 | 4 | 19 | 1 |
| 2. Provider Integrations | 69 | 18 | 14 | 14 | 10 | 13 |
| 3. Tools | 44 | 3 | 15 | 3 | 12 | 11 |
| 4. Core Systems | 65 | 1 | 8 | 7 | 44 | 5 |
| 5. Crate/Package Mapping | 26 | 1 | 2 | 3 | 0 | 20 |
| 6. Database Features | 27 | 14 | 4 | 0 | 3 | 6 |
| 7. LSP Features | 13 | 2 | 6 | 0 | 0 | 5 |
| 8. MCP Features | 12 | 2 | 5 | 1 | 2 | 2 |
| 9. Server API Features | 30 | 3 | 5 | 0 | 15 | 7 |
| 10. TUI/UI Features | 23 | 0 | 0 | 2 | 12 | 9 |
| 11. Plugin System | 7 | 0 | 0 | 0 | 2 | 5 |
| 12. Testing Infrastructure | 15 | 4 | 1 | 0 | 0 | 10 |
| 13. Session V2 Features | 10 | 0 | 0 | 0 | 1 | 9 |
| 14. Additional Packages | 11 | 0 | 0 | 0 | 0 | 11 |
| **Total** | **447** | **90** | **83** | **34** | **120** | **114** |

## Key Gaps Summary

1. **Provider coverage**: RustCode has 20 providers vs 33 in TS. Missing: Google Vertex, DeepInfra, Cloudflare, GitLab, NVIDIA, Vercel, Alibaba, SAP, Snowflake, Venice, Dynamic, Gateway, Kilo, LLM Gateway, OpenCode Console, ZenMux, OpenAI Auth.

2. **Additional tools**: RustCode has 5 extra tools not in TS core: StashTool, NotebookEditTool, TaskOutputTool, ExitPlanModeTool.

3. **Plugin system**: RustCode plugin system is scaffold-only. TS has full plugin boot, events, layer maps.

4. **Session V2**: TS has full event-sourced session V2 with projection, run coordinator, context epochs. RustCode only has v2_schema types.

5. **Web/Desktop/UI**: TS has full React/Ink TUI, Next.js web app, Electron desktop app. RustCode has basic ratatui shell only.

6. **Slack/Enterprise/Packages**: 11 TS packages with no RustCode equivalent.

7. **Testing infra**: RustCode has no HTTP API tests, integration tests, fixtures, or benchmark suite.

8. **Debug CLI commands**: 12 debug subcommands exist in RustCode as type stubs but handlers are not implemented.
