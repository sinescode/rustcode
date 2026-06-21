# CLI/Commands System — Gap Analysis

## Command Inventory

**All 23 TS commands have corresponding Rust CLI definitions** ✅

| Command | TS | Rust | Handler Parity |
|---------|----|------|----------------|
| `run` | Full | Full (basic REPL) | **Partial** — missing piped stdin, interactive mode features |
| `tui` | Full | Full | ✅ Good |
| `attach` | Full | Full | ✅ Good |
| `upgrade` | Full | Basic (prints commands) | **Stub** |
| `uninstall` | Full | Full | ✅ Good |
| `models` | Full | Stub | **Stub** (no refresh) |
| `stats` | Full | Full | ✅ Good |
| `export` | Full | Full | ✅ Good |
| `import` | Full | Basic | **Partial** |
| `pr` | Full | Prints instruction only | **Stub** |
| `plugin` | Full | Prints instruction only | **Stub** |
| `db` | Full | Delegates to sqlite3 | **Different approach** |
| `acp` | Full | Full | ✅ Good |
| `mcp` | Full (add, list, auth, logout, debug) | Missing auth list | **Partial** |
| `debug` | 11 subcommands | 11 stubs (~6 functional) | **Partial** |
| `console` | Full | Full (login/logout/switch/orgs/open) | ✅ Good |
| `providers` | Full | Basic (no interactive prompts) | **Partial** |
| `agent` | Full | Stub (list only partial) | **Stub** |
| `github` | Full | Stub | **Stub** |
| `session` | Full | Full | ✅ Good |
| `generate` | Full | Full | ✅ Good |
| `serve` | Full | Full | ✅ Good |
| `web` | Full | Full | ✅ Good |
| `version` | TS flag | Subcommand | Different mechanism |

## Missing Commands

- **`completion`** — shell completion script generation (TS `index.ts:80`)

## CLI Infrastructure Gaps

| Feature | TS | Rust | Status |
|---------|----|------|--------|
| **Error formatting** | 12 typed error formatters with suggestions | Plain `eprintln!()` | **CRITICAL** |
| **Interactive prompts** | `@clack/prompts`: select, text, password, autocomplete, spinner | None | **CRITICAL** |
| **ANSI logo** | 20-line ASCII art with shading | Box-drawing only | **Missing** |
| **Style constants** | 12 (TEXT_HIGHLIGHT, TEXT_DIM, etc.) | None | **Missing** |
| **Spinner** | Built-in spinner | None | **Missing** |
| **Auto-upgrade check** | Background version check | None | **Missing** |
| **Heap monitoring** | Auto heap snapshot >2GB RSS | None | **Missing** |

## 5 Most Critical Gaps

### 1. Interactive Run Mode
TS has a full split-footer TUI with file resolution, session management, background subagents, and demo mode. Rust has a basic REPL loop.

### 2. Interactive Prompts
TS uses `@clack/prompts` (select, text, password, autocomplete, spinner) across login, agent creation, provider login, export, MCP add. Rust has zero interactive prompts.

### 3. Error Formatting
TS has 12 specialized error formatters with formatted messages and suggestions. Rust uses plain `eprintln!`.

**TS**: `cli/error.ts:35-126`

### 4. Plugin Installation
TS does npm install + manifest read + config patch. Rust prints instructions.

**TS**: `cmd/plug.ts:70-176`
**Rust**: `main.rs:7799-7844`

### 5. Agent Creation
TS uses LLM to generate system prompt with interactive workflow. Rust prints instructions.

**TS**: `cmd/agent.ts:61-231`
**Rust**: `main.rs:3856-3892`
