# Tool System — Gap Analysis

## Tool Inventory

| Tool | TS `core` | TS `opencode` | Rust | Status |
|------|-----------|---------------|------|--------|
| `bash`/`shell` | Full | ShellTool (657L) + Tree-sitter | Simple BashTool | **Partial** |
| `read` | Full | Full | Full | **Good** |
| `write` | Full | Full | Full | **Good** |
| `edit` | Full | Full (737L) + replacers | Full (replacers ported) | **Good** |
| `glob` | Full | Full | Full | **Good** |
| `grep` | Full | Full | Full | **Good** |
| `webfetch` | Full | Full | Full (HTML→MD) | **Good** |
| `websearch` | Full (Exa/Parallel MCP) | Full | **Placeholder stub** | **Stub** |
| `apply_patch` | Full | Full (313L) | **Missing add/delete/move** | **Partial** |
| `task` | — | Full (346L, subagent) | **Placeholder stub** | **Stub** |
| `question` | Full | Full | **Event-bus only, no real ask** | **Stub** |
| `skill` | Full | Full | Frontmatter parsing ported | **Partial** |
| `todowrite` | Full | Full | Good (no persistence) | **Good** |
| `lsp` | — | Full (9 operations) | **Missing** | **Missing** |
| `invalid` | — | Present (21L) | **Missing** | **Missing** |
| `plan_enter` | — | — | Rust-only | Rust extra |
| `plan_exit` | — | Plan tool | Stub | **Stub** |
| `stash` | — | — | Full (Rust-only) | Rust extra |
| `notebook_edit` | — | — | Full (Rust-only) | Rust extra |

## Tool Prompt Template Comparison

TS has **14 `.txt` prompt files** with detailed LLF-facing instructions. Rust has **none** — all descriptions are inline `&str` literals, significantly shorter.

## 5 Most Critical Gaps

### 1. ShellTool Not Ported (Tree-sitter missing)
TS ShellTool uses `web-tree-sitter` for bash/PowerShell AST parsing, permission scanning, shell-specific prompts, streaming output, and multi-shell support.

**Rust**: Simple `tokio::process::Command` — no permission scanning at command level, no PowerShell/cmd support.

**TS**: `opencode/tool/shell.ts:1-657`, `shell/prompt.ts:1-307`, `core/bash.ts:1-206`
**Rust**: `tool_impls.rs:546-795`

### 2. WebSearchTool Is a Placeholder
TS calls real MCP endpoints (Exa, Parallel) with structured search args. Rust returns hardcoded text.

**TS**: `core/websearch.ts:1-246`, `opencode/websearch.ts:1-143`, `mcp-websearch.ts:1-96`
**Rust**: `tool_impls.rs:2327-2425`

### 3. Missing LSP Tool
TS has a full LSP tool with 9 operations and diagnostics. Rust has no LSP tool.

**TS**: `opencode/lsp.ts:1-113`
**Rust**: **MISSING**

### 4. TaskTool Is a Placeholder
TS has full subagent delegation with permission derivation, session creation, background jobs, interrupt handling.

**Rust**: Returns placeholder text.

**TS**: `opencode/task.ts:1-346`
**Rust**: `tool_impls.rs:2824-2945`

### 5. QuestionTool Is a Placeholder
TS synchronously asks the user and returns answers. Rust publishes an event and returns "pending" — not wired for response delivery.

**TS**: `core/question.ts:1-86`, `opencode/question.ts:1-44`
**Rust**: `tool_impls.rs:2948-3102`
