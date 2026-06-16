# CLAUDE.md — rustcode

Rust port of [OpenCode](https://github.com/sst/opencode) (TypeScript/Bun AI coding agent).
OpenCode source pinned at commit `5d0f86606ac30690f79f0a6a9f41a1f49fe95d0b`.

## Absolute Rules

1. **NEVER run any `cargo` command locally** — no `cargo build`, `cargo check`, `cargo test`, `cargo clippy`, `cargo fmt`, `cargo clean`, `cargo install`. All compilation and validation happens in GitHub Actions CI. Write correct code; CI verifies it.
2. **`#![forbid(unsafe_code)]`** in every crate.
3. **No `.unwrap()` in library code** — use `?`, `.ok_or()`, `.unwrap_or()`, or `expect()` with a reason string.
4. **Stream everything** — use `tokio::sync::broadcast`, `tokio_stream`, `futures::Stream`. No buffering full LLM responses.
5. **Cite the TS source** — doc comments on public items should include `/// Ported from: packages/<pkg>/src/<path>`.
6. **Atomic commits** — one logical change per commit, imperative mood, describe "why" not "what".
7. **Green CI before next module** — never move to a new module while CI is red.

## Workspace Layout

```
rustcode/                    # workspace root + binary crate
├── Cargo.toml               # workspace manifest + root [package]
├── src/main.rs              # CLI entry point (clap: Run, Serve, Session, Version)
├── deny.toml                # cargo-deny license/advisory config (v2 format)
├── .github/workflows/ci.yml # CI: fmt, clippy -D warnings, test (ubuntu+macos), cargo-deny
└── crates/
    ├── rustcode-core/       # core library — config, provider, session, tool, permission, etc.
    ├── rustcode-server/     # HTTP/SSE server (axum) — stub
    ├── rustcode-tui/        # terminal UI (ratatui) — stub
    ├── rustcode-lsp/        # LSP integration — stub
    └── rustcode-mcp/        # MCP (Model Context Protocol) — stub
```

## CI Pipeline (`.github/workflows/ci.yml`)

4 jobs, all must pass:
- **Format**: `cargo fmt --all -- --check`
- **Clippy**: `cargo clippy --all-targets --all-features -- -D warnings`
- **Test**: `cargo build && cargo test` on ubuntu-latest + macos-latest
- **Cargo Deny**: `EmbarkStudios/cargo-deny-action@v2` — license allowlist + advisory checks

## Lint Policy

Currently in **scaffold phase** — relaxed lints:
- `#![warn(clippy::all)]` only — pedantic and nursery disabled
- `#![allow(dead_code, unused_imports, unused_variables)]` in rustcode-core
- Re-enable `clippy::pedantic` and `clippy::nursery` per-module as each reaches production quality

## rustcode-core Modules (20 modules)

All in `crates/rustcode-core/src/`. Current status: scaffold (type skeletons + key traits).

| Module | TS Source | Key types |
|---|---|---|
| `error.rs` | cross-cutting | `Error` (14 variants), `Result<T>` |
| `id.rs` | `core/id/` | `ascending()`, `descending()`, `create()` |
| `env.rs` | `opencode/env/` | `Env` (HashMap wrapper) |
| `bus.rs` | `opencode/bus/global.ts` | `EventBus`, `SharedBus`, `GlobalEvent` |
| `config.rs` | `opencode/config/` + `core/config/` | `Config`, `ProviderConfig`, `AgentConfig`, `McpConfig` |
| `storage.rs` | `opencode/storage/` + `core/database/` | `Storage` (JSON), `Database` (SQLite placeholder) |
| `permission.rs` | `opencode/permission/` + `core/permission/` | `evaluate()`, `PermissionRule`, wildcard matching, tests |
| `provider.rs` | `opencode/provider/` + `core/plugin/provider/` | `Provider` trait, `Model`, `StreamChunk`, `ChatMessage` |
| `tool.rs` | `opencode/tool/` + `core/tool/` | `Tool` trait, `ToolRegistry`, `ToolResult` |
| `agent.rs` | `opencode/agent/` + `core/agent.ts` | `Agent`, `AgentMode`, permissions |
| `session.rs` | `opencode/session/` + `core/session/` | `Session`, `Message`, `ToolState`, `SessionProcessor` |
| `git.rs` | `opencode/git/` + `core/git.ts` | `Git` (status, diff, worktree) |
| `snapshot.rs` | `opencode/snapshot/` + `core/snapshot.ts` | `Snapshot`, `SnapshotService` |
| `worktree.rs` | `opencode/worktree/` | Git worktree management |
| `format.rs` | `opencode/format/` | Token/cost formatting |
| `image.rs` | `opencode/image/` + `core/image/` | MIME type detection |
| `plugin.rs` | `opencode/plugin/` + `core/plugin/` | `PluginManager`, `Plugin` |
| `skill.rs` | `opencode/skill/` + `core/skill/` | `Skill`, `discover()` from `.opencode/skills/*.md` |
| `question.rs` | `opencode/question/` | User prompt types |
| `lsp.rs` / `mcp.rs` | `opencode/lsp/` / `opencode/mcp/` | Placeholder — main impl in `rustcode-lsp` / `rustcode-mcp` |

## OpenCode Source Reference

The upstream TS source lives at `/home/kali/gitaction/opencodess/opencode/`.

| Package | Path | Files | Purpose |
|---|---|---|---|
| opencode | `packages/opencode/src/` | 355 | CLI, agent, session, provider, tool, permission, config, server |
| core | `packages/core/src/` | 313 | Database (18 SQLite tables, 35 migrations), session runner, tool impl, filesystem |
| llm | `packages/llm/src/` | 55 | Protocol adapters: Anthropic, OpenAI, Bedrock, Gemini, Azure, XAI |
| tui | `packages/tui/src/` | 146 | React/Ink terminal UI (maps to ratatui) |

Key TS patterns and their Rust equivalents:
- **Effect.ts** (`Effect.gen`, `Context.Service`, `Layer`) → `async fn` + `thiserror` + dependency injection via struct fields
- **Vercel AI SDK** (`@ai-sdk/*`) → `reqwest` + custom protocol adapters per provider
- **drizzle ORM** → `sqlx` with raw SQL
- **React/Ink TUI** → `ratatui` (future)
- **EventEmitter** → `tokio::sync::broadcast`

## Implementation Order

Modules are implemented bottom-up by dependency:

1. `error` → 2. `id` → 3. `env` → 4. `bus` → 5. `config` → 6. `storage` → 7. `permission` → 8. `provider` → 9. `tool` → 10. `agent` → 11. `session` → 12. `git` → 13. `snapshot` → 14–25. (remaining)

## Key Dependencies

| Crate | Version | Purpose |
|---|---|---|
| tokio | 1 (full) | Async runtime |
| serde / serde_json | 1 | Serialization |
| thiserror | 2 | Error derive |
| sqlx | 0.8 (sqlite) | Database |
| axum | 0.8 | HTTP server |
| reqwest | 0.12 (rustls-tls) | HTTP client for LLM APIs |
| clap | 4 (derive) | CLI argument parsing |
| chrono | 0.4 | Timestamps |
| toml | 0.8 | Config file parsing |
| futures / tokio-stream | 0.3 / 0.1 | Streaming |
| dashmap | 6 | Concurrent maps |

## Commit Style

- Short, imperative mood: `fix(ci): relax lints for scaffold`
- Prefix with scope: `feat(provider):`, `fix(session):`, `refactor(tool):`
- **Never add `Co-Authored-By` or any co-author trailer**
