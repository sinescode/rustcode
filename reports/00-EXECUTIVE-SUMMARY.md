# Rustcode ↔ OpenCode (TS) — Gap Analysis Executive Summary

## Overview

- **OpenCode (TS)**: ~149,077 LOC across ~610 files (4 main packages)
- **Rustcode (Rust)**: ~153,845 LOC across 153 files (6 crates)
- **Pinned TS commit**: `5d0f86606ac30690f79f0a6a9f41a1f49fe95d0b`

## By the Numbers

| Metric | Value |
|--------|-------|
| Total Rust LOC | 153,845 |
| Total TS LOC (core+opencode+llm+tui) | ~149,077 |
| Rust crates | 6 |
| Rust modules in core | 89 files |
| TS packages | 4+ |
| TS files in scope | ~610 |

## Overall Parity Estimate

| Domain | Parity Estimate | Critical Gaps |
|--------|----------------|---------------|
| Session System | ~30% types, ~10% behavior | **5 critical** — EventV2, Runner orchestration, Context Epoch, Input lifecycle, Model resolution |
| Agent System | ~60% types, ~20% behavior | **5 critical** — V2 Agent Service, Agent generation, V2 runner, LLM event publishing, Model resolution |
| Tool System | ~70% | **5 critical** — ShellTool (Tree-sitter), WebSearch (stub), Task (stub), Question (stub), LSP tool missing |
| Provider/LLM | ~65% | **5 critical** — OpenAI Responses API, Route composition, Hardcoded catalogs, Reasoning variants, Vertex/OAuth |
| Config System | ~75% | **5 critical** — YAML frontmatter parsing, Remote/well-known config, TUI config system, NPM deps, Structured validation |
| Permission System | ~80% | **5 critical** — V2 `evaluateInput()`, `ask()` pending entries, Saved permissions project ID, Agent resolution, Cascade stale state |
| Database/Storage | ~60% | **5 critical** — Migration system constraints, JSON storage locking, Path validation, SessionStore context loading, 12 missing table CRUDs |
| Server System | ~65% | **5 critical** — Workspace routing middleware, Fence (sync barrier), Instance context, Proxy middleware, Schema validation |
| TUI System | ~45% | **5 critical** — No plugin system, No autocomplete, Reduced themes (35→8), No command palette, Monolithic architecture |
| LSP System | ~50% | **5 critical** — Pull diagnostics, Wait-for-diagnostics, Dynamic root detection, Spawn dedup, User config override |
| MCP System | ~55% | **5 critical** — OAuth flow missing, Tool execution is stub, CLI commands missing, No tolerant schema fallback, No progress/timeout reset |
| Plugin System | ~20% | **5 critical** — V2 Effect-based plugins missing, 24/33 provider plugins missing, Entire TUI plugin system, Auth plugins missing, Boot phase orchestrator |
| Event/Bus/Git/Support | ~60% | **5 critical** — Event publish no DB persistence, Image normalization missing, Share module skeleton, Event aggregateEvents no historical replay, Worktree no events |
| CLI/Commands | ~75% | **5 critical** — Interactive run mode, Interactive prompts, Error formatting, Plugin installation, Agent creation |
| Auth/Credentials/Identity | ~55% | **5 critical** — Credential CRUD service, OAuth device flow incomplete, Token refresh dedup, Installation/upgrade service, Account service features |
| Filesystem/Process/PTY/Util | ~50% | **5 critical** — PTY runtime missing, Filesystem watcher types-only, No FFF search engine, File locking absent, 95+ utility functions missing |

## Overall: ~55-60% parity

The Rust port has excellent type coverage (most structs, enums, traits are present) but significant behavioral/functional gaps in:

1. **Event sourcing** — Foundation of V2 sessions, completely in-memory
2. **LLM protocol adapters** — Missing Responses API, routing framework
3. **V2 session runner** — Context epochs, compaction, overflow recovery
4. **TUI architecture** — Plugin system, theming, autocomplete
5. **Plugin system** — 90% of hooks/auth plugins missing
