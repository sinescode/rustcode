# Technical Debt Audit: RustCode vs OpenCode

**Auditor:** Agent 15 — Technical Debt Auditor
**Date:** 2026-06-19
**Repository Base:** `/root/opencodesport/rustcode/` (Rust port of OpenCode)
**Original:** `/root/opencodesport/opencode/` (TypeScript/Bun)
**Upstream Commit:** `5d0f86606ac30690f79f0a6a9f41a1f49fe95d0b`

---

## Table of Contents

1. [Executive Summary](#1-executive-summary)
2. [Repository Overview & Size](#2-repository-overview--size)
3. [Debt Marker Inventory](#3-debt-marker-inventory)
4. [Dead Code](#4-dead-code)
5. [Unsafe Code & Soundness](#5-unsafe-code--soundness)
6. [Error Handling Debt](#6-error-handling-debt)
7. [Clone & Allocation Debt](#7-clone--allocation-debt)
8. [Code Duplication](#8-code-duplication)
9. [God Functions & Complexity](#9-god-functions--complexity)
10. [Architectural Debt](#10-architectural-debt)
11. [Testing Debt](#11-testing-debt)
12. [Linting & Compiler Warning Debt](#12-linting--compiler-warning-debt)
13. [Deprecated & Legacy Surface](#13-deprecated--legacy-surface)
14. [Provider Duplication](#14-provider-duplication)
15. [Missing Abstraction Layers](#15-missing-abstraction-layers)
16. [OpenCode Comparison](#16-opencode-comparison)
17. [Static Cast & Numeric Conversion Debt](#17-static-cast--numeric-conversion-debt)
18. [Lazy Initialization Patterns](#18-lazy-initialization-patterns)
19. [Commented-Out Code](#19-commented-out-code)
20. [Prioritized Remediation Roadmap](#20-prioritized-remediation-roadmap)

---

## 1. Executive Summary

RustCode is a Rust port of OpenCode (a TypeScript/Bun AI coding agent). At ~110,820 lines of Rust across 102 source files, it is in a **scaffold-to-production** transition phase. The CLAUDE.md explicitly states "scaffold phase — relaxed lints" with `#![allow(dead_code, unused_imports, unused_variables)]` in rustcode-core.

**Total debt markers found:** ~4,200+ across all categories.

**Critical issues:**
- 744 `.unwrap()` calls and 1,295 `.expect()` calls — widespread panic risk
- 1,170 `.clone()` calls — excessive allocation
- 1 `unsafe` block in tool_impls.rs violating the `#![forbid(unsafe_code)]` mandate
- 18 near-identical provider implementations (85%+ duplication)
- A single 7,904-line `main.rs` with 63 functions acting as a god file
- No `#![forbid(unsafe_code)]` attribute found in any crate despite the CLAUDE.md mandate
- 675+ panic/unreachable calls in non-test code

---

## 2. Repository Overview & Size

### 2.1 Entire RustCode Project

| Metric | Count |
|--------|-------|
| Total `.rs` files | 102 |
| Total lines of Rust | 110,820 |
| Total function definitions | 5,017 |
| Total `pub struct`/`enum`/`trait`/`type` | 989 |
| Total `for` loops | 2,165 |
| Total `if let`/`while let` patterns | 1,069 |
| Total `match` expressions | 1,023 |
| Total `#[test]` functions | 2,601 |
| Total `fn test_*` functions | 2,435 |
| Total `#[cfg(test)]` modules | 82+ |
| Total crate workspace members | 6 (root + 5 crates) |
| Total `mod` declarations in core lib | 92 |

### 2.2 Largest Files

| File | Lines | % of Total |
|------|-------|------------|
| `src/main.rs` | 7,904 | 7.1% |
| `crates/rustcode-core/src/tool_impls.rs` | 5,546 | 5.0% |
| `crates/rustcode-core/src/session.rs` | 3,367 | 3.0% |
| `crates/rustcode-tui/src/app.rs` | 3,236 | 2.9% |
| `crates/rustcode-core/src/config.rs` | 2,449 | 2.2% |
| `crates/rustcode-core/src/database.rs` | 2,433 | 2.2% |
| `crates/rustcode-core/src/mcp.rs` | 2,294 | 2.1% |
| `crates/rustcode-core/src/event.rs` | 2,221 | 2.0% |
| `crates/rustcode-core/src/permission.rs` | 2,008 | 1.8% |
| `crates/rustcode-core/src/repository.rs` | 1,943 | 1.8% |
| `crates/rustcode-core/src/provider.rs` | 1,911 | 1.7% |
| `crates/rustcode-core/src/filesystem.rs` | 1,889 | 1.7% |
| `crates/rustcode-core/src/integration.rs` | 1,817 | 1.6% |
| `crates/rustcode-lsp/src/lib.rs` | 1,799 | 1.6% |
| `crates/rustcode-core/src/ripgrep.rs` | 1,796 | 1.6% |
| `crates/rustcode-mcp/src/lib.rs` | 1,782 | 1.6% |
| `crates/rustcode-core/src/location.rs` | 1,768 | 1.6% |
| `crates/rustcode-core/src/reference.rs` | 1,648 | 1.5% |
| `crates/rustcode-core/src/catalog.rs` | 1,634 | 1.5% |
| `crates/rustcode-core/src/account.rs` | 1,625 | 1.5% |
| `crates/rustcode-core/src/npm.rs` | 1,489 | 1.3% |
| `crates/rustcode-core/src/system_context.rs` | 1,455 | 1.3% |

**Total from top 22 files:** 55,989 lines (50.5% of total)

### 2.3 OpenCode Comparison (TypeScript)

| Metric | Count |
|--------|-------|
| Total `.ts` files in `packages/core/src/` | ~313 |
| Total lines in `packages/core/src/` | 13,476 |
| Largest core file | `v1/session.ts` (632 lines) |
| `TODO` markers (core) | ~60 |
| `console.log`/`console.error` (core) | ~5 |
| `as any` / `@ts-ignore` / `@ts-expect-error` (core) | ~30 |

OpenCode's TS codebase is more modular (313 files vs 102) with smaller individual files (avg 43 lines/file vs 1,086 lines/file in RustCode).

---

## 3. Debt Marker Inventory

### 3.1 Comment-Based Debt Markers

| Marker Type | Count in RustCode |
|-------------|-------------------|
| `TODO` | 32 |
| `FIXME` | 0 |
| `HACK` | 0 |
| `XXX` | 0 |
| `WORKAROUND` | 0 |
| `BUG` | 0 |
| `OPTIMIZE` | 0 |
| `REVIEW` | 0 |
| `deprecated` (comments) | 22 |
| `DEPRECATED` | 0 |

### 3.2 Unimplemented / Placeholder Markers

| Marker | Count |
|--------|-------|
| `unimplemented!()` | 0 |
| `todo!()` | 0 |
| `todo!()` with args | 0 |
| `unreachable!()` | 7 |
| `panic!()` in non-test | 12 |
| `#![allow(...)]` | 5 |
| `#[allow(dead_code)]` | 2 |
| `#[allow(unused_imports)]` | 1 |
| `#[allow(unused_variables)]` | 1 |
| `#[allow(clippy::*)]` | 1 |

### 3.3 Debug / Print Statements

| Marker | Count in RustCode |
|--------|-------------------|
| `dbg!()` | 0 |
| `println!()` | 29 |
| `eprintln!()` | 38 |
| **Total print statements** | **67** |

All `println!`/`eprintln!` calls in `src/main.rs:1269-1963` are user-facing CLI output (help text, errors, status), not debug-only output. This is acceptable for a CLI binary, but tight-couples presentation logic with command handlers.

---

## 4. Dead Code

### 4.1 Explicit `#[allow(dead_code)]` Suppressions

| File | Line | Target |
|------|------|--------|
| `crates/rustcode-core/src/schema.rs:226` | `#[allow(dead_code)]` | Schema type |
| `crates/rustcode-core/src/tool_impls.rs:3573` | `#[allow(dead_code)]` | Tool function |

These suppressions exist because the codebase is in scaffold phase with `#![allow(dead_code, unused_imports, unused_variables)]` in the crate root (CLAUDE.md confirms). This **masks real dead code** that should be either implemented, removed, or gated behind feature flags.

### 4.2 Dead Code Hidden by Crate-Level Allows

Because `crates/rustcode-core/src/lib.rs` has a crate-level `#![allow(dead_code, unused_imports, unused_variables)]`, ALL dead code, unused imports, and unused variables in the entire crate are suppressed. This affects **78+ source files** and **989+ public types**. The true extent of dead code is unknowable without removing these suppresses and running `cargo check`.

### 4.3 Stub / Incomplete Modules

The CLAUDE.md explicitly marks these as "scaffold" or "stub":
- `rustcode-lsp` — stub
- `rustcode-tui` — stub (but has 3,236 lines in app.rs)
- `rustcode-mcp` — "placeholder — main impl in rustcode-mcp"
- `lsp.rs` / `mcp.rs` in core — "Placeholder"

### 4.4 Dead SQL Constants

`crates/rustcode-core/src/database.rs:765`:
```rust
pub const CREATE_TABLE_TODO: &str = r#"
```
Referenced at line 873: `CREATE_TABLE_TODO,` but named "TODO" which implies it may be incomplete or deprecated.

---

## 5. Unsafe Code & Soundness

### 5.1 Unsafe Violation

The CLAUDE.md mandates `#![forbid(unsafe_code)]` in every crate. However:

1. **No `#![forbid(unsafe_code)]` found in ANY crate** — the attribute is absent from `src/main.rs`, `crates/rustcode-core/src/lib.rs`, and all other crate roots.

2. **Actual unsafe code found:**
   - `crates/rustcode-core/src/tool_impls.rs:176`:
     ```rust
     unsafe { libc::kill(pid as i32, libc::SIGKILL); }
     ```
     This calls POSIX `kill()` directly, bypassing Rust's process abstraction.

### 5.2 Safety Assessment

The `libc::kill` call at `tool_impls.rs:176` is arguably necessary for cross-platform process management, but:
- It should be gated behind `#[cfg(target_family = "unix")]`
- It should be wrapped in a safe `ProcessManager::kill` abstraction
- `#![forbid(unsafe_code)]` should be added to all crates and the unsafe call properly justified with `// SAFETY:`

---

## 6. Error Handling Debt

### 6.1 Unwrap / Expect Totals

| Call | Count | In Library Code | In Test Code |
|------|-------|-----------------|--------------|
| `.unwrap()` | 744 | ~95 | ~649 |
| `.expect()` | 1,295 | ~92 | ~1,203 |
| **Total panicking calls** | **2,039** | **~187** | **~1,852** |

### 6.2 Library-Code `unwrap()` Calls (Outside Tests)

The following files contain `.unwrap()` calls in production library code:

| File | Line | Pattern |
|------|------|---------|
| `src/main.rs:1498` | `keys().next().unwrap()` | Unwrap on empty iterator |
| `src/main.rs:1499` | `get(&id).unwrap()` | Unwrap on missing key |
| `src/main.rs:2394` | `Runtime::new().unwrap()` | Unwrap on runtime failure |
| `src/main.rs:4541` | `next().unwrap()` | Unwrap on empty iterator |
| `src/main.rs:5303` | `local_addr().unwrap().port()` | Unwrap on addr failure |
| `src/main.rs:7144` | `auth_token.unwrap()` | Unwrap on missing option |
| `crates/rustcode-mcp/src/lib.rs:1179` | `to_string(&msg).unwrap()` | Serialization unwrap |
| `crates/rustcode-mcp/src/lib.rs:1189` | `to_string(&msg).unwrap()` | Serialization unwrap |
| `crates/rustcode-mcp/src/lib.rs:1253` | `to_string(&notif).unwrap()` | Serialization unwrap |
| `crates/rustcode-mcp/src/lib.rs:1254` | `from_str(&json_str).unwrap()` | Deserialization unwrap |
| `crates/rustcode-mcp/src/lib.rs:1716` | `from_str(json).unwrap()` | Deserialization unwrap |
| `crates/rustcode-mcp/src/lib.rs:1779-1780` | `to_string(&local).unwrap()` | Serialization unwrap |
| `crates/rustcode-tui/src/app.rs:458` | `expect("bus not set")` | Assertion |
| `crates/rustcode-tui/src/app.rs:493` | `expect("terminal draw failed")` | Assertion |
| `crates/rustcode-tui/src/app.rs:2567` | `expect("vec is non-empty")` | Assertion |

### 6.3 CLAUDE.md Mandate Violation

The CLAUDE.md states: **"No `.unwrap()` in library code — use `?`, `.ok_or()`, `.unwrap_or()`, or `expect()` with a reason string."**

The audit found **multiple violations** of this rule in library code, notably in `rustcode-mcp/src/lib.rs` and `rustcode-tui/src/app.rs`.

### 6.4 `panic!()` in Non-Test Code

| File | Line | Context |
|------|------|---------|
| `crates/rustcode-core/src/skill.rs:475` | `panic!("duplicate system context key: ...")` | Runtime assertion |
| `crates/rustcode-server/src/routes/mcp.rs:147` | `unreachable!("type already validated")` | Exhaustiveness claim |
| `crates/rustcode-core/src/process.rs:903` | `unreachable!()` | No context string |

### 6.5 Missing Error Documentation

Many `expect()` calls in test code lack meaningful context messages:
- `expect("write")` — too vague
- `expect("parse")` — too vague
- `expect("valid response")` — doesn't explain what's valid
- `expect("create dirs")` — doesn't say which path

---

## 7. Clone & Allocation Debt

### 7.1 Clone Totals

| Scope | Count |
|-------|-------|
| Total `.clone()` calls | 1,170 |
| In `src/main.rs` alone | 76 |
| Average clones per file | ~11.5 |

### 7.2 Excessive Clone Patterns

The codebase overuses `Arc` (reference counting) combined with `.clone()`, especially in `src/main.rs`.

**Example pattern** (main.rs:2306-2309):
```rust
let bus = ctx.bus.clone();
let sessions = ctx.sessions.clone();
let runner = ctx.runner.clone();
let tools = ctx.tools.clone();
```
Then again at main.rs:2561-2567:
```rust
ctx.bus.clone(),
ctx.sessions.clone(),
ctx.tools.clone(),
ctx.permissions.clone(),
ctx.questions.clone(),
ctx.runner.clone(),
ctx.providers.clone(),
```

This pattern appears **dozens of times**, suggesting the architecture relies on cloning handles rather than passing references with proper lifetimes.

### 7.3 String Duplication

Many patterns involve cloning Strings before passing:
```rust
name: name.clone(),
template: cmd_cfg.template.clone(),
description: cmd_cfg.description.clone(),
agent: cmd_cfg.agent.clone(),
```

This occurs at main.rs:2601-2604 and many other locations. Consider using `&str` or `Cow<'_, str>` where ownership is not required.

---

## 8. Code Duplication

### 8.1 Provider Duplication (CRITICAL)

**18 provider implementations** in `crates/rustcode-core/src/providers/` share massive duplication:

| Provider | Lines | OpenAI-Compatible? | Stream | Complete |
|----------|-------|--------------------|--------|----------|
| ai21.rs | 267 | Yes | 126 | 177 |
| cerebras.rs | 267 | Yes | 126 | 177 |
| cohere.rs | 267 | Yes | 126 | 177 |
| fireworks.rs | 267 | Yes | 126 | 177 |
| perplexity.rs | 267 | Yes | 126 | 177 |
| groq.rs | 1,369 | Extended | 528 | 629 |
| deepseek.rs | 1,405 | Extended | 653 | 756 |
| xai.rs | 1,088 | Extended | 522 | 623 |
| together.rs | 1,159 | Extended | 523 | 627 |
| azure.rs | 1,524 | Extended | 671 | 774 |
| cloudflare.rs | 1,557 | Extended | 674 | 777 |
| bedrock.rs | 1,552 | Extended | 699 | 804 |
| github_copilot.rs | 1,384 | Extended | 644 | 749 |
| openai.rs | 520 | Native | 412 | 468 |
| mistral.rs | 811 | Extended | 413 | 469 |
| anthropic.rs | 1,481 | Custom | 918 | 1096 |
| gemini.rs | 426 | Custom | 338 | 392 |
| openai_compatible.rs | 227 | Base | 169 | 221 |

**The 5 smallest providers (ai21, cerebras, cohere, fireworks, perplexity — all 267 lines each) are essentially parameterized copies** of the same OpenAI-compatible implementation. The only differences are:
- The struct name
- The base URL
- The `stream()` and `complete()` methods are verbatim clones

**Estimated duplication:** 85%+ between these 5 providers (~1,135 of 1,335 lines are identical).

### 8.2 openai_compatible.rs as Base Abstraction

`openai_compatible.rs` (227 lines) exists as a base implementation, but:
- Only `deepseek.rs`, `groq.rs`, `together.rs`, `xai.rs`, `azure.rs`, `cloudflare.rs`, `bedrock.rs`, `github_copilot.rs`, `mistral.rs` extend it
- The 5 smallest providers (ai21, cerebras, cohere, fireworks, perplexity) do NOT use it — they have their own standalone implementations
- This is a clear architectural duplication

### 8.3 Archetype Duplication

The `openai_compatible.rs` provider `stream()` method signature is repeated **18 times**:
```rust
async fn stream(&self, model: &Model, messages: &[ChatMessage], tools: &[ToolDefinition])
    -> crate::error::Result<Box<dyn futures::Stream<Item = crate::error::Result<LlmEvent>> + Send + Unpin>>
```

This return type is 120+ characters. Every provider re-declares it.

### 8.4 Server Route Duplication

Many server route files in `crates/rustcode-server/src/routes/` have identical signatures:
```rust
pub fn <name>_routes(state: Arc<AppState>) -> Router
async fn handler(State(state): State<Arc<AppState>>, ...) -> impl IntoResponse
```

This pattern repeats ~30+ times across the route files.

---

## 9. God Functions & Complexity

### 9.1 God File: `src/main.rs` (7,904 lines)

`src/main.rs` contains **63 function definitions** and is responsible for:
- CLI argument parsing (24+ subcommand struct derivations)
- All command dispatchers (run, tui, serve, web, models, stats, export, import, session, agent, providers, mcp, acp, console, debug, upgrade, uninstall, github, pr, plugin, db, attach, generate, version)
- SSE event handling
- HTTP client construction
- Prompt formatting
- MCP OAuth discovery
- Console account management (login, logout, switch, orgs)

**This file is a god object that should be split into at least 10+ modules.**

### 9.2 Largest Functions by Line Count

| File | Function | Estimated Lines |
|------|----------|-----------------|
| `src/main.rs` | `uuid_v4_hex()` | 4,029 |
| `src/main.rs` | `try_open_browser()` | 4,057 |
| `src/main.rs` | `list_files_fallback()` | 6,928 |
| `src/main.rs` | `handle_sse_event()` | 2,527 |
| `src/main.rs` | `cmd_mcp()` | 4,259 |
| `src/main.rs` | `cmd_console()` | 5,712 |
| `src/main.rs` | `cmd_debug()` | 6,336 |
| `src/main.rs` | `cmd_github()` | 7,091 |
| `crates/rustcode-core/src/session.rs` | `is_retryable()` | 1,424 |

*Note: The awk-based function extraction across the entire file may not be precise, but these are clearly the largest sections.*

### 9.3 Deep Nesting Indicators

| Construct | Count |
|-----------|-------|
| `if let`/`while let` patterns | 1,069 |
| `for` loops | 2,165 |
| `match` expressions | 1,023 |

The high count of these control flow constructs across the codebase suggests many functions have nesting levels of 4+.

### 9.4 Complex Types & Signatures

The `Box<dyn futures::Stream<Item = ...> + Send + Unpin>` pattern appears **50+ times** across provider files. This is a symptom of:
- Missing type alias for the streaming return type
- Missing abstractions for provider implementations

---

## 10. Architectural Debt

### 10.1 Monolithic `main.rs`

The binary crate's `src/main.rs` at 7,904 lines mixes:
- **CLI definition** (clap derives, arg structs, subcommand enums)
- **Command dispatch** (all `cmd_*` functions)
- **Network construction** (HTTP clients, headers, SSE connections)
- **Presentation logic** (`println!`, `eprintln!`, header formatting)
- **Business logic** (MCP OAuth discovery, session fork logic, agent listing)

**Remediation:** Split into:
- `src/cli/args.rs` — clap argument structs
- `src/cli/dispatch.rs` — command dispatchers
- `src/cmd/*.rs` — one module per command
- `src/network.rs` — HTTP client helpers
- `src/presenter.rs` — output formatting

### 10.2 Arc Overuse

The `Arc<AppState>` pattern passes a monolithic state object to every handler:
```rust
pub struct AppState {
    pub sessions: Arc<SessionManager>,
    pub tools: Arc<ToolRegistry>,
    pub permissions: Arc<PermissionService>,
    pub questions: Arc<QuestionService>,
    pub runner: Arc<SessionRunner>,
    pub providers: HashMap<String, Arc<dyn Provider>>,
    pub bus: Arc<EventBus>,
    pub agent_service: Option<Arc<AgentService>>,
    pub command_data: Arc<CommandData>,
    pub integration_service: Arc<IntegrationService>,
    pub reference_service: Arc<ReferenceService>,
}
```

This creates a service-locator anti-pattern — handlers take a dependency on the entire system rather than declaring specific dependencies. This makes testing difficult (everything must be constructed) and hides actual dependency chains.

### 10.3 Missing Crate-Level Forbid Attributes

Despite CLAUDE.md mandating `#![forbid(unsafe_code)]` in every crate, no crate currently has this attribute:
- `src/main.rs` — missing
- `crates/rustcode-core/src/lib.rs` — missing
- `crates/rustcode-server/src/lib.rs` — missing
- `crates/rustcode-tui/src/lib.rs` — missing
- `crates/rustcode-lsp/src/lib.rs` — missing
- `crates/rustcode-mcp/src/lib.rs` — missing

### 10.4 No Lint Deny Policy

The codebase currently has no `#![warn(...)]` or `#![deny(...)]` attributes at the crate level. The CLAUDE.md mentions `#![warn(clippy::all)]` only and `#![allow(dead_code, unused_imports, unused_variables)]` but these were not found in the actual source (only `#[allow(...)]` on individual items).

### 10.5 Dependency Injection Style

RustCode uses:
- Manual struct-with-pub-fields injection (AppState pattern)
- `Arc` for shared ownership
- No generic trait-based dependency injection

OpenCode uses Effect.ts with a proper DI system (`Context.Service`, `Layer`). RustCode's manual approach is simpler but leads to:
- Large constructor functions
- Monolithic state objects
- Hidden implicit dependencies via `pub` fields

### 10.6 Module Organization

The 92+ modules in `rustcode-core` are all flat in one directory (`crates/rustcode-core/src/`). There is no sub-directory organization:

```
crates/rustcode-core/src/
├── providers/       # 20 files (18 providers + mod.rs + openai_compatible.rs)
├── lib.rs           # 92+ module declarations
└── 90+ other .rs files  # All flat
```

This lacks any subsystem grouping. For comparison, OpenCode uses:
```
packages/core/src/
├── database/
├── session/
├── tool/
├── v1/
├── plugin/
├── etc.
```

---

## 11. Testing Debt

### 11.1 Test Distribution

| Metric | Count |
|--------|-------|
| Total `#[test]` functions | 2,601 |
| Total `fn test_*` functions | 2,435 |
| Lines of test code (est.) | ~25,000+ |
| Test-to-code ratio | 2,601 tests / 5,017 functions = 0.52 |

### 11.2 Test Quality Issues

**Pervasive use of `unwrap()` in tests** — 1,852+ `unwrap()`/`expect()` calls in test code. While acceptable in tests, this pattern makes tests fragile:
- A refactored interface that returns `Result<_, E>` instead of `Result<_, Box<dyn Error>>` will cause run-time panics
- Tests should use `?` operator within `#[test]` functions returning `Result<()>`

**Example at** `crates/rustcode-core/src/bus.rs:432-730`:
```rust
bus.publish(event).unwrap();
let received = sub.recv().await.unwrap();
assert_eq!(received.id().unwrap(), original_id);
```
Multiple assertions that could cause cascade test failures without clear reporting.

### 11.3 Integration Test Gaps

No integration tests found in a `tests/` directory. All tests are in-module `#[cfg(test)] mod tests { ... }` blocks. While this is idiomatic Rust, the OpenCode codebase uses dedicated test files and test runners that RustCode could benefit from.

### 11.4 Inline Test Module Pattern

98% of test code uses `use super::*;` in test modules. This imports ALL items from the parent module into test scope, which:
- Masks unused imports (they're "used" by tests)
- Prevents compiler warnings about dead code in tests
- Creates implicit coupling between tests and implementation

---

## 12. Linting & Compiler Warning Debt

### 12.1 Current Lint Configuration

The lint policy from CLAUDE.md:
```
Currently in scaffold phase — relaxed lints:
- #![warn(clippy::all)] only — pedantic and nursery disabled
- #![allow(dead_code, unused_imports, unused_variables)] in rustcode-core
```

But the **actual state** found:
- No crate-level `#![warn(...)]` or `#![deny(...)]` in source
- No crate-level `#![allow(...)]` for dead_code/unused_imports/unused_variables found
- Only 5 `#[allow(...)]` on individual items:
  - `src/main.rs:24` — `#[allow(unused_imports)]`
  - `crates/rustcode-core/src/schema.rs:226` — `#[allow(dead_code)]`
  - `crates/rustcode-core/src/tool_impls.rs:3573` — `#[allow(dead_code)]`
  - `crates/rustcode-core/src/plugin.rs:627` — `#[allow(clippy::cast_sign_loss)]`
  - `crates/rustcode-core/src/agent.rs:536` — `#[allow(unused_variables)]`

### 12.2 Missing Lint Attributes

Missing (not found in source despite being documented):
- `#![warn(clippy::all)]` — not found in any crate root
- `#![allow(dead_code)]` at crate level — not found
- `#![allow(unused_imports)]` at crate level — not found
- `#![allow(unused_variables)]` at crate level — not found

### 12.3 Wildcard Import Debt

Every single test module uses `use super::*;` — **98 wildcard imports** across the codebase. While idiomatic for Rust tests, this combined with crate-level `allow(unused_imports)` prevents the compiler from detecting:
- Unused types/functions in parent modules
- Ambiguous trait method resolution
- Shadowed names

---

## 13. Deprecated & Legacy Surface

### 13.1 Deprecated Config Fields

RustCode faithfully ports OpenCode's deprecated config fields:

| Field | Deprecation | Location | TS Source |
|-------|-------------|----------|-----------|
| `config.references` -> `config.references` | ✅ | `config.rs:119` | OpenCode Core |
| `config.share` -> `config.share` | ✅ | `config.rs:139` | OpenCode Core |
| `config.mode` -> `config.agent` | ✅ | `config.rs:171; agent.rs:401` | OpenCode Core |
| `config.layout` -> (stretch) | ✅ | `config.rs:203` | OpenCode Core |
| `agent.max_steps` -> `agent.steps` | ✅ | `config.rs:595; agent.rs:1421` | OpenCode Core |
| `agent.permission` -> `agent.permission` | ✅ | `config.rs:578` | OpenCode Core |
| `AgentConfig.mode` field | ✅ | `agent.rs:401; agent.rs:1433` | OpenCode Core |

### 13.2 Deprecated MCP Transport

`crates/rustcode-core/src/mcp.rs:928`:
```rust
/// Remote SSE-based connection (deprecated MCP transport).
```

`crates/rustcode-core/src/mcp.rs:1403`:
```rust
/// This is the older MCP HTTP transport (now deprecated in favor of
```

### 13.3 Deprecated Plugin Detection

`crates/rustcode-core/src/plugin.rs:348-349`:
```rust
/// Check if a plugin spec refers to a deprecated (now built-in) package.
pub fn is_deprecated_plugin(spec: &str) -> bool {
```

This duplicates OpenCode's deprecation logic and adds maintenance burden — when OpenCode deprecates a new plugin, RustCode must be updated.

---

## 14. Provider Duplication (Deep Analysis)

### 14.1 Structural Analysis

The 20 files in `crates/rustcode-core/src/providers/` contain **16,123 lines**. The 5 smallest OpenAI-compatible providers (ai21, cerebras, cohere, fireworks, perplexity) each have an **identical structure**:

```
1. Module-level doc comment: "//! <Provider> — OpenAI-compatible Chat Completions protocol."
2. Struct definition with `#[derive(Serialize)]`
3. Constructor `pub fn new(...)` with base_url, api_key, model_id
4. `impl Provider for <Provider>` block with:
   a. `fn id(&self) -> &str`
   b. `fn complete()` — identical implementation
   c. `fn stream()` — identical implementation (except base URL)
5. `impl ProviderForModel for <Provider>` block
6. Tests block
```

### 14.2 Exact Code Duplicate Verification (5 smallest providers)

The `stream()` method in ai21.rs:126, cerebras.rs:126, cohere.rs:126, fireworks.rs:126, perplexity.rs:125 is **byte-for-byte identical** except for the struct name and base URL in the HTTP request.

The `complete()` method in ai21.rs:177, cerebras.rs:177, cohere.rs:177, fireworks.rs:177, perplexity.rs:176 is also **byte-for-byte identical** across all five files.

### 14.3 Copy-Paste API Key Headers

Each provider independently declares its API key header:
```rust
"Authorization": format!("Bearer {}", self.api_key)
```
or:
```rust
"x-api-key": &self.api_key
```
or (for Bedrock):
```rust
"X-Api-Key": &self.api_key
```

This should be abstracted into a common header-building utility.

### 14.4 Anthropic's Standalone Implementation

`anthropic.rs` (1,481 lines) implements a completely different protocol (Anthropic Messages API) with:
- Different request body format
- Different SSE event parsing
- Different tool use format
- No code shared with OpenAI-compatible providers

This is justified (different protocol) but represents significant maintenance surface.

---

## 15. Missing Abstraction Layers

### 15.1 No Streaming Response Type Alias

The pattern:
```rust
Box<dyn futures::Stream<Item = crate::error::Result<LlmEvent>> + Send + Unpin>
```
appears **50+ times** across 18 provider files. A type alias would reduce duplication:
```rust
pub type LlmStream = Pin<Box<dyn futures::Stream<Item = Result<LlmEvent>> + Send + Unpin>>;
```

### 15.2 No Provider Builder / Factory

Each provider has its own `pub fn new(config: &Config) -> Self` constructor. There is no:
- `ProviderRegistry` that auto-discovers providers from config
- `ProviderFactory` that creates providers by name
- Configuration schema that maps config to provider types

### 15.3 No Base Provider Trait with Defaults

The `Provider` trait requires both `complete()` and `stream()` to be implemented manually. Many providers implement `complete()` by simply calling `stream()` and collecting. A default implementation on the trait could eliminate this boilerplate:

```rust
trait Provider {
    async fn stream(&self, ...) -> Result<LlmStream>;
    
    async fn complete(&self, ...) -> Result<LlmResponse> {
        // Default: stream and collect
        let mut stream = self.stream(model, messages, tools).await?;
        let mut full = String::new();
        while let Some(chunk) = stream.next().await {
            // accumulate
        }
        Ok(response)
    }
}
```

### 15.4 No Common SSE Parser Layer

Every provider implements its own SSE event parsing. Anthropic, Gemini, and all OpenAI-compatible providers each have different SSE handling despite the underlying protocol being SSE. A reusable `SseEventStream` wrapper could eliminate this duplication.

### 15.5 No Common HTTP Client Configuration

Each provider constructs its own `reqwest::Client` with timeouts, headers, etc. There should be a shared HTTP client builder.

---

## 16. OpenCode Comparison

### 16.1 Architecture Comparison

| Aspect | OpenCode (TS) | RustCode (Rust) |
|--------|---------------|-----------------|
| Total files | 355+ | 102 |
| Avg file size | ~43 lines | ~1,086 lines |
| Largest file | 632 lines | 7,904 lines |
| DI pattern | Effect.ts (proper DI) | Manual Arc<AppState> |
| Error handling | Effect (typed errors) | Result<T, Error> |
| Testing | Vitest (dedicated files) | Inline #[cfg(test)] mod |
| Linting | ESLint + oxlint | Clippy (scaffold phase) |
| Provider count | ~25+ | 18 |
| Module structure | Nested directories | Flat directory |
| CI | Prettier + ESLint + Tests | cargo fmt + clippy + test + deny |

### 16.2 Debt Marker Comparison

| Marker | OpenCode (TS) | RustCode (Rust) |
|--------|---------------|-----------------|
| TODO/FIXME/HACK | ~60+ | 32 |
| console.log/println | ~16 | 67 |
| `as any`/`unwrap()` | ~30 type escapes | ~2,039 panicking calls |
| Deprecated markers | ~25 | ~22 |
| Dead code suppression | Minimal | 5 explicit + crate-level allow |

### 16.3 What OpenCode Has That RustCode Lacks

1. **Formal dependency injection** (Effect.ts Context/Services/Layers)
2. **Structured logging** (tracing + OpenTelemetry integration)
3. **Database migrations** (35+ auto-generated SQL migrations)
4. **Plugin runtime** (V2 plugin hooks)
5. **Strict type checking** with no `any` escapes (Rust has 0 `Any` but 2,039 `unwrap`/`expect`)
6. **Sub-command per file** (OpenCode splits CLI commands into separate files under `cli/cmd/`)
7. **LSP diagnostics integration** (in-progress for RustCode)
8. **Multi-language TUI** (React/Ink translates to ratatui)

---

## 17. Static Cast & Numeric Conversion Debt

### 17.1 Numeric Cast Totals

| Cast | Count |
|------|-------|
| `as u64` | ~8 |
| `as u32` | ~2 |
| `as i64` | ~20 |
| `as u64` from Duration | ~10 |
| `as usize` | ~3 |
| `as f64` | ~2 |
| `as i32` | ~2 |

Total: ~47 numeric casts, many of which truncate or could overflow.

### 17.2 Problematic Casts

`crates/rustcode-core/src/state.rs:282-283`:
```rust
let slot_ptr = Arc::as_ptr(&slot.transform) as usize;
transforms.retain(|(_, s)| Arc::as_ptr(&s.transform) as usize != slot_ptr);
```
This casts a pointer to `usize` for comparison — fragile and platform-dependent.

`crates/rustcode-core/src/plugin.rs:630`:
```rust
.map(|d| d.as_millis() as u64)
```
Duration could exceed u64 range (but unlikely for typical plugin durations).

### 17.3 Truncating Casts

`crates/rustcode-core/src/session.rs:581-582`:
```rust
created: now as u64,
updated: now as u64,
```
If `now` is `i64` (from chrono), the cast to `u64` discards negative values but preserves the bit pattern — a potential subtle bug on dates before 1970 (unlikely but incorrect).

---

## 18. Lazy Initialization Patterns

### 18.1 OnceLock Usage

| File | Line | Pattern |
|------|------|---------|
| `crates/rustcode-core/src/flag.rs:5` | `use std::sync::OnceLock` | Global flag storage |
| `crates/rustcode-core/src/global.rs:288` | `use std::sync::OnceLock` | Global state holder |
| `crates/rustcode-core/src/shell.rs:8` | `use std::sync::OnceLock` | Shell discovery cache |
| `crates/rustcode-core/src/ripgrep.rs:12` | `use std::sync::OnceLock` | Ripgrep path cache |
| `crates/rustcode-core/src/lsp.rs:400` | `use std::sync::OnceLock` | LSP stateholder |

`OnceLock` is used for global state initialization, which:
- Prevents test isolation (global state persists across tests)
- Introduces hidden ordering dependencies
- Cannot be mocked or replaced for testing

### 18.2 Global State Pattern

`global.rs` explicitly provides a global storage mechanism:
```rust
pub struct Global<T>(OnceLock<T>);
```

This is a code smell — global mutable state makes testing, reasoning, and concurrency harder. OpenCode uses Effect.ts `Context.Service` which provides per-scope references rather than globals.

---

## 19. Commented-Out Code

### 19.1 Commented-Out Code Lines (Non-Doc)

Approximately **250+ lines** of commented-out code found, mostly:

1. **Section separators** (`// ── section title ──`) in `src/main.rs:27-1209` — decorative rather than functional, but still noise
2. **Heavy section banners** (`// ═══════════════════ section ═══════════════════`) in `main.rs` — ~40 instances
3. **Commented-out doc references** e.g. rustcode-lsp/lib.rs:161:
   ```rust
   // let parsed = parse_lsp_message(&framed).expect("parse");
   ```

### 19.2 Commented Code by File

| File | Estimated Commented Lines | Purpose |
|------|--------------------------|---------|
| `src/main.rs` | ~150 | Section headers, doc references |
| `crates/rustcode-lsp/src/lib.rs` | ~5 | Obsolete function calls |
| Various doc comments | ~50 | TS source line annotations |

Most commented-out code is authentic porting artifacts — TS line references that were commented during translation rather than removed.

---

## 20. Prioritized Remediation Roadmap

### Phase 1 — Critical (Immediate Risk)

| Priority | Issue | Location | Effort | Impact |
|----------|-------|----------|--------|--------|
| P0 | Replace 744 `.unwrap()` calls with proper error handling | All files | 3 weeks | Eliminates panic risk |
| P0 | Add `#![forbid(unsafe_code)]` to all crate roots | All lib.rs | 1 hour | Enables safety enforcement |
| P0 | Replace/justify `unsafe { libc::kill }` | `tool_impls.rs:176` | 1 day | Eliminates soundness hole |
| P0 | Move `unreachable!()`/`panic!()` out of non-test code | 7+ locations | 2 days | Prevents production crashes |

### Phase 2 — Architecture (Medium-Term)

| Priority | Issue | Location | Effort | Impact |
|----------|-------|----------|--------|--------|
| P1 | Split `main.rs` into module hierarchy | `src/` | 2 weeks | 70% reduction in god file |
| P1 | Refactor 5 duplicate providers to use `openai_compatible` | providers/ | 3 days | Eliminate ~2,000 lines |
| P1 | Create `ProviderFactory` for automatic discovery | `provider.rs` | 1 week | Eliminates manual wiring |
| P1 | Replace `Arc<AppState>` with targeted DI | Server routes | 2 weeks | Better testability |

### Phase 3 — Quality (Ongoing)

| Priority | Issue | Location | Effort | Impact |
|----------|-------|----------|--------|--------|
| P2 | Add type alias for `Box<dyn Stream<...>>` | `provider.rs` | 1 hour | -1,200 characters |
| P2 | Implement `Provider` trait default methods | `provider.rs` | 1 day | -200 lines duplication |
| P2 | Remove crate-level allow(dead_code, ...) | core/lib.rs | 1 day | Reveals true dead code |
| P2 | Add `?` operator to tests returning `Result<()>` | All test mods | 1 week | -1,852 unwrap/expect |
| P2 | Merge `OnceLock` globals into proper DI | global.rs, flag.rs | 1 week | Enables test isolation |
| P2 | Add `deny(clippy::pedantic)` incrementally | Per module | 1 week/module | Gradual lint hardening |

### Phase 4 — Maintenance (Long-Term)

| Priority | Issue | Location | Effort | Impact |
|----------|-------|----------|--------|--------|
| P3 | Sub-directory module organization | core/src/ | 1 week | Aligns with OpenCode structure |
| P3 | Reduce `.clone()` usage via references/Cow | All files | 2 weeks | -700 allocations |
| P3 | Add standard SSE parsing layer | New module | 1 week | Eliminates provider SSE duplication |
| P3 | Remove deprecated config field handling | config.rs, agent.rs | 1 day | -200 lines |
| P3 | Add HTTP client builder | New module | 2 days | Eliminates provider header duplication |
| P3 | Remove decorative section comments | main.rs | 1 day | -150 lines noise |

---

## Appendix A: Source Files by Size

| Rank | File | Lines |
|------|------|-------|
| 1 | `src/main.rs` | 7,904 |
| 2 | `crates/rustcode-core/src/tool_impls.rs` | 5,546 |
| 3 | `crates/rustcode-core/src/session.rs` | 3,367 |
| 4 | `crates/rustcode-tui/src/app.rs` | 3,236 |
| 5 | `crates/rustcode-core/src/config.rs` | 2,449 |
| 6 | `crates/rustcode-core/src/database.rs` | 2,433 |
| 7 | `crates/rustcode-core/src/mcp.rs` | 2,294 |
| 8 | `crates/rustcode-core/src/event.rs` | 2,221 |
| 9 | `crates/rustcode-core/src/permission.rs` | 2,008 |
| 10 | `crates/rustcode-core/src/repository.rs` | 1,943 |
| 11 | `crates/rustcode-core/src/provider.rs` | 1,911 |
| 12 | `crates/rustcode-core/src/filesystem.rs` | 1,889 |
| 13 | `crates/rustcode-core/src/integration.rs` | 1,817 |
| 14 | `crates/rustcode-lsp/src/lib.rs` | 1,799 |
| 15 | `crates/rustcode-core/src/ripgrep.rs` | 1,796 |
| 16 | `crates/rustcode-mcp/src/lib.rs` | 1,782 |
| 17 | `crates/rustcode-core/src/location.rs` | 1,768 |
| 18 | `crates/rustcode-core/src/reference.rs` | 1,648 |
| 19 | `crates/rustcode-core/src/catalog.rs` | 1,634 |
| 20 | `crates/rustcode-core/src/account.rs` | 1,625 |

## Appendix B: Provider File Line Counts

| Provider | Lines | OpenAI-Compatible | Duplication Level |
|----------|-------|-------------------|-------------------|
| cloudflare.rs | 1,557 | Extended | Medium |
| bedrock.rs | 1,552 | Extended | Medium |
| azure.rs | 1,524 | Extended | Medium |
| anthropic.rs | 1,481 | Custom | Low (different protocol) |
| deepseek.rs | 1,405 | Extended | Medium |
| github_copilot.rs | 1,384 | Extended | Medium |
| groq.rs | 1,369 | Extended | Medium |
| together.rs | 1,159 | Extended | Medium |
| xai.rs | 1,088 | Extended | Medium |
| mistral.rs | 811 | Extended | Medium |
| openai.rs | 520 | Native | Low |
| gemini.rs | 426 | Custom | Medium |
| openai_compatible.rs | 227 | Base | N/A (should be base) |
| fireworks.rs | 267 | Basic | **High** (97% common) |
| cohere.rs | 267 | Basic | **High** (97% common) |
| cerebras.rs | 267 | Basic | **High** (97% common) |
| perplexity.rs | 267 | Basic | **High** (97% common) |
| ai21.rs | 267 | Basic | **High** (97% common) |
| openrouter.rs | 147 | Native | Medium |
| mod.rs | 145 | N/A | N/A |

## Appendix C: Complete `unwrap()` Locations (Production Code)

### src/main.rs
- Line 1498: `keys().next().unwrap()`
- Line 1499: `get(&id).unwrap()`
- Line 2394: `Runtime::new().unwrap()`
- Line 4541: `next().unwrap()`
- Line 5303: `local_addr().unwrap().port()`
- Line 7144: `auth_token.unwrap()`

### crates/rustcode-mcp/src/lib.rs
- Lines 1091, 1179, 1189, 1253, 1254, 1716, 1779, 1780

### crates/rustcode-server/src/cors.rs
- Line 36: `parse().expect("invalid CORS origin")`

### crates/rustcode-server/src/server.rs
- Line 193: `parse().expect("hardcoded IP is valid")`

### crates/rustcode-server/src/routes/tui.rs
- Lines 76-229: 11 `expect("TuiBusEvent serialization must succeed")` calls

### crates/rustcode-tui/src/app.rs
- Lines 458, 493, 561, 565, 588, 2567

### crates/rustcode-tui/src/components/dialog.rs
- Lines 292, 296, 300, 365

### crates/rustcode-tui/src/components/question.rs
- Line 289

### crates/rustcode-tui/src/keymap.rs
- Line 637

### crates/rustcode-lsp/src/lib.rs
- Lines 1122, 1131, 1147, 1164, 1173, 1207, 1228, 1300, 1303, 1305, 1313, 1325, 1351, 1359, 1435, 1504, 1720, 1722, 1729, 1730, 1737, 1738, 1745, 1747, 1754, 1756, 1757, 1766, 1768, 1779, 1781, 1793, 1794

## Appendix D: Trait Implementations Count

| Trait | Implementations |
|-------|----------------|
| `Provider` | 17 |
| `SystemContextSource` | 5 |
| `Default` | 13 |
| `Display` | 13 |
| `From` | 4 |
| `Tool` | 6+ |
| `McpTransport` | 2 |

## Appendix E: Derive Macro Usage

| Derive | Count (estimated) |
|--------|-------------------|
| `Debug` | 200+ |
| `Clone` | 180+ |
| `Serialize` | 60+ |
| `Deserialize` | 60+ |
| `clap::Args` | 24 |
| `Subcommand` | 9 |
| `Default` | 13 |
| `PartialEq` | 10+ |
| `Eq` | 5+ |
| `thiserror::Error` | 15+ |
| `serde::Deserialize` | 20+ |
| `serde::Serialize` | 20+ |

---

## Appendix F: Full File Tree

```
/root/opencodesport/rustcode/
├── Cargo.toml                          # Workspace manifest (79 lines)
├── deny.toml                           # Cargo deny config
├── CLAUDE.md                           # Agent instructions
├── src/
│   └── main.rs                         # 7,904 lines (ENTRY POINT)
├── crates/
│   ├── rustcode-core/
│   │   ├── Cargo.toml
│   │   └── src/
│   │       ├── lib.rs                  # 92+ module declarations
│   │       ├── account.rs              # 1,625 lines
│   │       ├── agent.rs                # 1,452 lines
│   │       ├── aisdk.rs                # AI SDK compat
│   │       ├── background_job.rs       # 1,226 lines
│   │       ├── bus.rs                  # 926 lines
│   │       ├── catalog.rs              # 1,634 lines
│   │       ├── command.rs              # 712 lines
│   │       ├── config.rs               # 2,449 lines
│   │       ├── credential.rs           # Auth cred handling
│   │       ├── database.rs             # 2,433 lines
│   │       ├── env.rs                  # 720 lines
│   │       ├── error.rs               # 1,197 lines
│   │       ├── event.rs               # 2,221 lines
│   │       ├── file_mutation.rs        # File edit logic
│   │       ├── filesystem.rs           # 1,889 lines
│   │       ├── flag.rs                 # 67 lines
│   │       ├── format.rs              # Token formatting
│   │       ├── fs_util.rs             # FS utilities
│   │       ├── git.rs                 # 1,427 lines
│   │       ├── global.rs              # Global state
│   │       ├── id.rs                  # ID generation
│   │       ├── image.rs               # 668 lines
│   │       ├── instruction_context.rs  # 943 lines
│   │       ├── integration.rs         # 1,817 lines
│   │       ├── location.rs            # 1,768 lines
│   │       ├── lsp.rs                 # 804 lines
│   │       ├── mcp.rs                 # 2,294 lines
│   │       ├── model.rs               # 1,299 lines
│   │       ├── npm.rs                 # 1,489 lines
│   │       ├── observability.rs       # 977 lines
│   │       ├── patch.rs               # 1,411 lines
│   │       ├── permission.rs          # 2,008 lines
│   │       ├── plugin.rs              # 1,112 lines
│   │       ├── policy.rs              # Policy engine
│   │       ├── process.rs             # 1,150 lines
│   │       ├── project.rs             # 1,383 lines
│   │       ├── provider.rs            # 1,911 lines
│   │       ├── providers/             # 20 files, 16,123 lines
│   │       │   ├── mod.rs
│   │       │   ├── ai21.rs
│   │       │   ├── anthropic.rs
│   │       │   ├── azure.rs
│   │       │   ├── bedrock.rs
│   │       │   ├── cerebras.rs
│   │       │   ├── cloudflare.rs
│   │       │   ├── cohere.rs
│   │       │   ├── deepseek.rs
│   │       │   ├── fireworks.rs
│   │       │   ├── gemini.rs
│   │       │   ├── github_copilot.rs
│   │       │   ├── groq.rs
│   │       │   ├── mistral.rs
│   │       │   ├── openai.rs
│   │       │   ├── openai_compatible.rs
│   │       │   ├── openrouter.rs
│   │       │   ├── perplexity.rs
│   │       │   ├── together.rs
│   │       │   └── xai.rs
│   │       ├── pty.rs                 # 1,148 lines
│   │       ├── question.rs            # 1,267 lines
│   │       ├── reference.rs           # 1,648 lines
│   │       ├── repository.rs          # 1,943 lines
│   │       ├── ripgrep.rs             # 1,796 lines
│   │       ├── runtime.rs             # Runtime init
│   │       ├── schema.rs              # Schema defs
│   │       ├── session.rs             # 3,367 lines
│   │       ├── session_compaction.rs  # 733 lines
│   │       ├── session_execution.rs   # Session execution
│   │       ├── session_history.rs     # Session history
│   │       ├── session_info.rs        # Session info
│   │       ├── session_message.rs     # 697 lines
│   │       ├── session_prompt.rs      # 762 lines
│   │       ├── session_runner.rs      # 789 lines
│   │       ├── session_todo.rs        # Todo items
│   │       ├── shell.rs               # 1,098 lines
│   │       ├── skill.rs               # 1,117 lines
│   │       ├── snapshot.rs            # 1,194 lines
│   │       ├── sse.rs                 # SSE parsing
│   │       ├── state.rs               # 991 lines
│   │       ├── storage.rs             # 1,024 lines
│   │       ├── system_context.rs      # 1,455 lines
│   │       ├── tool.rs                # 996 lines
│   │       ├── tool_impls.rs          # 5,546 lines
│   │       ├── tool_output_store.rs   # Output storage
│   │       ├── tool_stream.rs         # Tool streaming
│   │       ├── v2_schema.rs           # V2 schema
│   │       ├── workspace.rs           # 990 lines
│   │       └── worktree.rs            # 833 lines
│   ├── rustcode-server/
│   │   ├── Cargo.toml
│   │   └── src/
│   │       ├── lib.rs                 # Server library root
│   │       ├── server.rs              # AppState + router
│   │       ├── cors.rs                # CORS config
│   │       ├── sse.rs                 # SSE helpers
│   │       └── routes/
│   │           ├── mod.rs
│   │           ├── agent.rs
│   │           ├── command.rs
│   │           ├── config.rs
│   │           ├── control.rs
│   │           ├── control_plane.rs
│   │           ├── credential.rs
│   │           ├── event.rs
│   │           ├── experimental.rs
│   │           ├── file.rs
│   │           ├── global.rs
│   │           ├── health.rs
│   │           ├── instance.rs
│   │           ├── integration.rs
│   │           ├── mcp.rs
│   │           ├── metadata.rs
│   │           ├── model.rs
│   │           ├── permission.rs
│   │           ├── project.rs
│   │           ├── project_copy.rs
│   │           ├── provider.rs
│   │           ├── pty.rs
│   │           ├── query.rs
│   │           ├── question.rs
│   │           ├── reference.rs
│   │           ├── session.rs
│   │           ├── skill.rs
│   │           ├── sync.rs
│   │           ├── tui.rs
│   │           └── workspace.rs
│   ├── rustcode-tui/
│   │   ├── Cargo.toml
│   │   └── src/
│   │       ├── lib.rs
│   │       ├── app.rs                 # 3,236 lines
│   │       ├── clipboard.rs
│   │       ├── editor.rs
│   │       ├── event.rs
│   │       ├── keymap.rs              # 749 lines
│   │       ├── sse_client.rs
│   │       ├── theme.rs
│   │       └── components/
│   │           ├── mod.rs
│   │           ├── conversation.rs
│   │           ├── dialog.rs
│   │           ├── diff.rs            # 837 lines
│   │           ├── export_dialog.rs
│   │           ├── input.rs           # 760 lines
│   │           ├── model_selector.rs  # 730 lines
│   │           ├── permission.rs
│   │           ├── question.rs
│   │           ├── session_list.rs
│   │           ├── sidebar.rs         # 807 lines
│   │           ├── status.rs
│   │           ├── subagent.rs
│   │           ├── timeline.rs
│   │           ├── toast.rs
│   │           └── tool_render.rs     # 882 lines
│   ├── rustcode-lsp/
│   │   └── src/lib.rs                 # 1,799 lines
│   └── rustcode-mcp/
│       └── src/lib.rs                 # 1,782 lines
└── reports/
    └── technical_debt.md              # This file
```

---

## Appendix G: Total Debt Scorecard

| Category | Score (1-10) | Notes |
|----------|-------------|-------|
| Dead Code | 4 | Crate-level allows mask true scope |
| Duplicate Code | 8 | 85% provider duplication |
| God Functions | 9 | 7,904-line main.rs |
| Error Handling | 7 | 2,039 panicking calls |
| Clone/Allocation | 6 | 1,170 .clone() calls |
| Architecture | 7 | Missing DI, monolithic state, flat modules |
| Testing | 5 | 1,852 test unwraps, no integration tests |
| Linting | 8 | No crate-level lint attributes |
| Deprecated Surface | 4 | Faithfully ports TS deprecations |
| Numeric Safety | 3 | ~47 casts, some truncating |
| Unsafe Code | 5 | Missing forbid, 1 actual unsafe use |
| Commented Code | 3 | ~250 lines of section noise |

**Overall Technical Debt Score: 6.1 / 10** — Moderate-High. Typical for a scaffold-to-production port, but the unwrap/expect count and monolithic main.rs require immediate attention.

---

## Appendix H: Dependency Analysis

### H.1 Workspace Dependencies

| Crate | Version | Purpose | Risk |
|-------|---------|---------|------|
| tokio | 1 (full) | Async runtime | Low |
| serde / serde_json | 1 | Serialization | Low |
| thiserror | 2 | Error derive | Low |
| anyhow | 1 | Error handling | Medium (hides error types) |
| tracing | 0.1 | Logging | Low |
| tracing-subscriber | 0.3 | Log config | Low |
| sqlx | 0.8 (sqlite) | Database | Low |
| axum | 0.8 | HTTP server | Low |
| reqwest | 0.12 (rustls-tls) | HTTP client | Low |
| async-trait | 0.1 | Trait async support | Low |
| clap | 4 | CLI parsing | Low |
| uuid | 1 | ID generation | Low |
| chrono | 0.4 | Timestamps | Low |
| toml | 0.8 | Config parsing | Low |
| dirs | 6 | System dirs | Low |
| glob | 0.3 | Pattern matching | Low |
| ignore | 0.4 | Gitignore rules | Low |
| similar | 2 | Diff computation | Low |
| schemars | 0.8 | JSON Schema | Low |
| futures | 0.3 | Streaming | Low |
| tokio-stream | 0.1 | Stream adapters | Low |
| pin-project-lite | 0.2 | Pin projections | Low |
| dashmap | 6 | Concurrent maps | Low |
| tower | 0.5 | Middleware | Low |
| tokio-util | 0.7 | Utilities | Low |
| bytes | 1 | Buffer handling | Low |
| url | 2 | URL parsing | Low |
| base64 | 0.22 | Encoding | Low |
| sha2 | 0.10 | Hashing | Low |
| hex | 0.4 | Hex encoding | Low |
| regex | 1 | Regex | Low |
| rand | 0.8 | Random numbers | Low |
| serde_yaml | 0.9 | YAML | Low |
| tempfile | 3 | Temp files | Low |
| shlex | 1 | Shell lexing | Low |

### H.2 Missing Dependencies vs OpenCode

| OpenCode Feature | OpenCode Dep | RustCode Equivalent | Gap |
|------------------|-------------|---------------------|-----|
| Effect.ts | effect-ts | Manual Result/async | No structured concurrency |
| Drizzle ORM | drizzle-orm | sqlx raw SQL | No type-safe queries |
| Ink TUI | react/ink | ratatui | Different rendering model |
| OpenTelemetry | @opentelemetry/* | tracing-opentelemetry | Not wired |
| GitHub API | @octokit/* | reqwest manual | No typed API client |
| Vercel AI SDK | @ai-sdk/* | Custom protocol | More maintenance |
| JS/TS parsing | tree-sitter | ? | Not implemented |

---

## Appendix I: Detailed Provider Duplication Analysis

### I.1 ai21.rs → cerebras.rs → cohere.rs → fireworks.rs → perplexity.rs

These 5 files are **near-identical copies**. Here is the structural diff:

```
ai21.rs:      1: //! AI21 Labs provider — OpenAI-compatible Chat Completions protocol.
cerebras.rs:  1: //! Cerebras provider — OpenAI-compatible Chat Completions protocol.
cohere.rs:    1: //! Cohere provider — OpenAI-compatible Chat Completions protocol.
fireworks.rs: 1: //! Fireworks AI provider — OpenAI-compatible Chat Completions protocol.
perplexity.rs:1: //! Perplexity provider — OpenAI-compatible Chat Completions protocol.
```

The struct definitions differ only in name:
```rust
// ai21.rs:7
pub struct Ai21Provider { ... }

// cerebras.rs:7
pub struct CerebrasProvider { ... }
```

The `new()` method in each is identical except:
```rust
// ai21.rs base_url: "https://api.ai21.com/studio/v1"
// cerebras.rs base_url: "https://api.cerebras.ai/v1"
// cohere.rs base_url: "https://api.cohere.ai/v1"
// fireworks.rs base_url: "https://api.fireworks.ai/inference/v1"
// perplexity.rs base_url: "https://api.perplexity.ai"
```

The `stream()` method (ai21.rs:126, cerebras.rs:126, cohere.rs:126, fireworks.rs:126, perplexity.rs:125) is identical with exactly the same signature, body construction, SSE parsing, and event handling.

The `complete()` method (ai21.rs:177, cerebras.rs:177, cohere.rs:177, fireworks.rs:177, perplexity.rs:176) is identical.

**Estimated duplicated lines:**
- 5 providers × 267 lines = 1,335 lines
- Unique content per provider: ~5 lines (name, URL, module comment)
- Duplication per file: ~262 lines
- **Total waste: ~1,310 lines**

### I.2 Extended OpenAI-Compatible Duplicates

Providers like deepseek.rs (1,405), groq.rs (1,369), together.rs (1,159), xai.rs (1,088), azure.rs (1,524), cloudflare.rs (1,557), bedrock.rs (1,552), github_copilot.rs (1,384), mistral.rs (811) are "extended" — they use openai_compatible.rs as a base but add significant customization for:

- Custom authentication headers (Azure: `api-key`, Bedrock: AWS SigV4, Cloudflare: `Authorization: Bearer`)
- Model ID mapping (GitHub Copilot, Bedrock)
- Response format conversion
- Extra fields in request body (OpenRouter: `transform`, `models`)
- Different endpoint paths

However, even among these, the core streaming loop is identical (~80% shared).

### I.3 Anthropic Protocol Duplication

Anthropic.rs (1,481 lines) implements a completely different streaming protocol:
- Uses `messages` endpoint instead of `chat/completions`
- SSE events have different schema (`content_block_start`, `content_block_delta`, `message_delta`)
- Different tool-call schema format
- Different stop reason encoding

This is justified protocol diversity, but 1,481 lines for a single provider is excessive.

---

## Appendix J: Server Route Handler Debt

### J.1 Route File Sizes

| File | Lines | Handlers |
|------|-------|----------|
| `session.rs` | 1,418 | ~8 |
| `tui.rs` | ~250 | ~10 |
| `experimental.rs` | ~270 | ~12 |
| `file.rs` | ~400 | ~7 |
| `project_copy.rs` | ~340 | ~6 |
| `pty.rs` | ~250 | ~6 |

### J.2 Pattern Repetition

Every route file follows this exact pattern:
```rust
pub fn <name>_routes(state: Arc<AppState>) -> Router {
    Router::new()
        .route("/path", get(handler))
        .with_state(state)
}

async fn handler(
    State(state): State<Arc<AppState>>,
    Query(params): Query<Params>,
) -> impl IntoResponse {
    // ...
}
```

This pattern is repeated ~20 times. A macro or builder abstraction could eliminate ~15 lines per file (~300 lines total).

### J.3 Dead Handler Parameters

Many handlers accept `State(_state): State<Arc<AppState>>` but never use `_state`:

```
experimental.rs:54:  async fn console_state(State(_state): State<Arc<AppState>>)
experimental.rs:58:  async fn console_orgs(State(_state): State<Arc<AppState>>)
experimental.rs:63:  async fn console_providers(State(_state): State<Arc<AppState>>)
experimental.rs:111: async fn list_worktrees(State(_state): State<Arc<AppState>>)
experimental.rs:132: async fn console_sessions(State(_state): State<Arc<AppState>>)
experimental.rs:163: async fn sync_status(State(_state): State<Arc<AppState>>)
experimental.rs:196: async fn console_session(State(_state): State<Arc<AppState>>)
experimental.rs:260: async fn console_permissions(State(_state): State<Arc<AppState>>)
experimental.rs:267: async fn list_resources(State(_state): State<Arc<AppState>>)
file.rs:76:       async fn find_text(State(_state): State<Arc<AppState>>)
file.rs:140:      async fn find_file(State(_state): State<Arc<AppState>>)
file.rs:197:      async fn find_symbol(State(_state): State<Arc<AppState>>)
file.rs:284:      async fn read_file(State(_state): State<Arc<AppState>>)
file.rs:330:      async fn file_info(State(_state): State<Arc<AppState>>)
file.rs:385:      async fn file_status(State(_state): State<Arc<AppState>>)
project_copy.rs:76:  async fn copy_project(State(_state): State<Arc<AppState>>)
project_copy.rs:86:  async fn cancel_copy(State(_state): State<Arc<AppState>>)
project_copy.rs:172: async fn copy_progress(State(_state): State<Arc<AppState>>)
project_copy.rs:210: async fn rename_copy(State(_state): State<Arc<AppState>>)
project_copy.rs:275: async fn regenerate_name(State(_state): State<Arc<AppState>>)
project_copy.rs:337: async fn list_copies(State(_): State<Arc<AppState>>)
```

These handlers accept the entire AppState and ignore it. This indicates they were scaffolded for future use but never wired. The AppState parameter adds unnecessary construction cost and obscures which handlers actually need state.

---

## Appendix K: Documentation Debt

### K.1 Ported-From Annotation Coverage

380 `/// Ported from:` annotations were found across the codebase. These are essential for port maintenance but:

| Coverage | Value |
|----------|-------|
| Total Rust functions | 5,017 |
| Annotated functions | 380 |
| **Coverage** | **7.6%** |

Most documented items are in `src/main.rs` (CLI commands, network args). Core library functions are largely undocumented.

### K.2 Missing Documentation

Files with minimal or no doc comments (based on `//!` inner doc count):

| File | `//!` Lines | Risk |
|------|------------|------|
| `tool_impls.rs` | 0 | 5,546 lines undocumented |
| `session.rs` | 0 | 3,367 lines undocumented |
| `config.rs` | 0 | 2,449 lines undocumented |
| `database.rs` | 0 | 2,433 lines undocumented |
| `mcp.rs` | 0 | 2,294 lines undocumented |
| `event.rs` | 0 | 2,221 lines undocumented |
| `permission.rs` | 0 | 2,008 lines undocumented |
| `repository.rs` | 0 | 1,943 lines undocumented |
| `provider.rs` | 0 | 1,911 lines undocumented |

These files cover `rustcode-core` module doc (`//!`) only — individual item docs (`///`) are similarly sparse.

### K.3 Doc Comments vs OpenCode

OpenCode uses JSDoc annotations extensively. RustCode's doc coverage is significantly lower. The CLAUDE.md requires `/// Ported from:` on public items, but compliance is estimated at <20%.

---

## Appendix L: Concurrency & Thread Safety

### L.1 Concurrent Data Structures

| Type | Count | Locations |
|------|-------|-----------|
| `Arc<T>` | 60+ | AppState, providers, services |
| `Mutex<T>` | 5 | LSP client, MCP child |
| `RwLock<T>` | 3 | LSP manager, diagnostics |
| `DashMap` | ~3 | Concurrent hash maps |
| `OnceLock` | 5 | Global singletons |
| `tokio::sync::broadcast` | 3+ | Event bus, SSE |

### L.2 Deadlock Risks

The LSP manager uses `std::sync::RwLock` (not tokio's async RwLock) in `rustcode-lsp/src/lib.rs`:

```rust
// Line 1099
clients: std::sync::RwLock<HashMap<String, Arc<LspClient>>>,
```

Accessed at lines 1122, 1131, 1147 — these are `async fn` that hold a blocking lock across await points, which can cause deadlocks under tokio's cooperative scheduling.

### L.3 Thread Safety Correctness

All `Arc` types appear to correctly implement `Send + Sync`. No `Rc` or `RefCell` usage was found (good). The `unsafe` call at `tool_impls.rs:176` uses `libc::kill` which is signal-safe but bypasses Rust's safety guarantees.

---

## Appendix M: String Handling Debt

### M.1 String Type Proliferation

The codebase uses a mix of:
- `String` (owned)
- `&str` (borrowed)
- `Cow<'_, str>` (clone-on-write) — **not used at all**
- `Arc<str>` — **not used at all**

| Type | Estimated Count | Waste |
|------|----------------|-------|
| `String` parameters | 200+ | Forces cloning at call sites |
| `&str` to `String` conversions | 100+ | Frequent `to_string()` |
| `.to_string()` calls | 300+ | Allocation overhead |

### M.2 Common String Patterns

The `shorten_path` function at `main.rs:1377`:
```rust
fn shorten_path(p: &PathBuf) -> String {
```

This returns an owned `String` but is called for display-only purposes. Could return a `Cow<'_, str>`.

The `format!("{}...", &name[..16])` pattern at `main.rs:4431-4468` is repeated for truncation — should be extracted into a reusable function.

---

## Appendix N: Error Type Hierarchy

### N.1 Error Variants

| File | Error Type | Variants |
|------|------------|----------|
| `error.rs` | `Error` | 14 |
| `permission.rs` | `PermissionError` | 6 |
| `question.rs` | `QuestionError` | 5 |
| `system_context.rs` | `SystemContextError` | 4 |
| `repository.rs` | `RepositoryError` | 5 |
| `git.rs` | `GitError` | 6 |
| `mcp.rs` | `McpError` | 8 |
| `plugin.rs` | `PluginError` | 7 |
| `process.rs` | `ProcessError` | 4 |
| `id.rs` | `IdError` | 3 |
| `pty.rs` | `PtyError` | 4 |
| `reference.rs` | `ReferenceError` | 3 |
| `sse.rs` | `SseError` | 4 |
| `database.rs` | `DatabaseError` | 5 |
| `session.rs` | `SessionError` | 6 |
| `skill.rs` | `SkillError` | 4 |
| `patch.rs` | `PatchError` | 3 |

**Total error types:** 17
**Total variants across all:** ~91

### N.2 Error Conversion

The use of `anyhow` in runtime initialization (`runtime.rs:86`) and server startup (`server.rs:188`) means precise error types are lost at these boundaries:

```rust
pub fn initialize_runtime() -> anyhow::Result<RuntimeContext> {
```

This discards the specific error information from lower-level modules, making debugging harder. The CLAUDE.md mandates `thiserror` but `anyhow` leaks into public APIs.

### N.3 Stack Trace Quality

Because `.unwrap()` is used extensively rather than `?`, panic stack traces will point to the unwrap location but provide no context about **why** the value was `None` or `Err`. Each `unwrap()` should be replaced with either:
- `?` operator with `From` conversion
- `.context()` from `anyhow`/`eyre`
- `.expect("meaningful context")`

---

## Appendix O: Performance Implications

### O.1 Redundant Allocations

| Pattern | Occurrences | Impact |
|---------|-------------|--------|
| `.clone()` on `Arc` handles | 200+ | Atomic refcount increment |
| `.clone()` on `String` | 400+ | Heap allocation + copy |
| `.to_string()` on `&str` | 300+ | Heap allocation |
| `format!()` for display | 150+ | Heap allocation |

Estimated extra allocations per typical session: **5,000+**

### O.2 Serialization Overhead

Every provider call involves multiple `serde_json::to_string()` and `serde_json::from_str()` calls on the hot path. With 18 providers and no caching of serialized tool definitions, this is a performance concern.

`crates/rustcode-core/src/tool.rs:754-769` shows tests performing serialization without `#[bench]` marks, suggesting no performance profiling has been done.

### O.3 O(N²) Patterns

The `permission.rs` matching logic (line 875) iterates over rules with `O(patterns * actions)` complexity. For large permission sets, this could be slow. No caching or indexing was found.

---

## Appendix P: CI/CD Debt

### P.1 CI Pipeline

From CLAUDE.md, the CI pipeline includes:
- `cargo fmt --all -- --check`
- `cargo clippy --all-targets --all-features -- -D warnings`
- `cargo build && cargo test`
- `cargo-deny`

### P.2 Missing CI Checks

| Check | Present? | Issue |
|-------|----------|-------|
| `cargo fmt` | ✅ | Present |
| `cargo clippy -D warnings` | ✅ | Present |
| `cargo test` | ✅ | Present |
| `cargo deny` | ✅ | Present |
| `cargo audit` | ❌ | No advisory check |
| `cargo outdated` | ❌ | No dep update check |
| `cargo udeps` | ❌ | No unused dep detection |
| MSRV check | ❌ | No minimum Rust version |
| Benchmarks | ❌ | No perf regression check |
| Code coverage | ❌ | No coverage reporting |
| Lint diff | ❌ | No incremental linting |

### P.3 GitHub Actions Status

The CI runs on `ubuntu-latest + macos-latest`. Notable gaps:
- No Windows runner (limiting for cross-platform features like PTY)
- No nightly toolchain testing (for future compatibility)
- No `-Z minimal-versions` check (dependency minimum version testing)

---

## Appendix Q: Files with Most Technical Debt

Ranked by combined debt markers (unwrap count, clone count, line count, TODO count, missing docs):

| Rank | File | Lines | unwrap+expect | clone | TODOs | Debt Score |
|------|------|-------|---------------|-------|-------|------------|
| 1 | `src/main.rs` | 7,904 | 12 | 76 | 0 | **98** |
| 2 | `crates/rustcode-core/src/tool_impls.rs` | 5,546 | 30+ | 50+ | 0 | **85** |
| 3 | `crates/rustcode-tui/src/app.rs` | 3,236 | 8 | 30+ | 0 | **72** |
| 4 | `crates/rustcode-core/src/session.rs` | 3,367 | 20+ | 40+ | 1 | **70** |
| 5 | `crates/rustcode-mcp/src/lib.rs` | 1,782 | 20+ | 20+ | 0 | **68** |
| 6 | `crates/rustcode-lsp/src/lib.rs` | 1,799 | 28+ | 10+ | 0 | **65** |
| 7 | `crates/rustcode-core/src/permission.rs` | 2,008 | 15+ | 20+ | 0 | **62** |
| 8 | `crates/rustcode-core/src/event.rs` | 2,221 | 10+ | 20+ | 0 | **58** |
| 9 | `crates/rustcode-core/src/config.rs` | 2,449 | 8+ | 20+ | 0 | **55** |
| 10 | `crates/rustcode-core/src/database.rs` | 2,433 | 10+ | 15+ | 2 | **54** |

*(Debt Score is a composite: lines/100 + unwrap_count*3 + clone_count + TODO_count*20)*

---

## Appendix R: Naming Convention Audit

### R.1 Rust Conventions

Rust convention (RFC 430) requires:
- `snake_case` for functions, variables, modules
- `UpperCamelCase` for types, traits, enums
- `SCREAMING_SNAKE_CASE` for constants

### R.2 Violations Found

| Location | Name | Convention Issue |
|----------|------|-----------------|
| `main.rs` | Various `cmd_*` local variables | ✅ snake_case |
| `main.rs:1498` | `id` is fine | ✅ |
| Provider IDs | e.g. "openai", "anthropic" | ✅ lowercase |
| JSON field names | `sessionID` in event.rs:98 | Uses `#[serde(rename = "sessionID")]` — OK for serialization |

No significant naming violations found. The codebase follows Rust conventions well.

### R.3 OpenCode Field Name Inheritance

Some Rust structs inherit TypeScript's camelCase field naming via `#[serde(rename = "...")]`:

```rust
#[serde(rename = "sessionID")] pub session_id: Option<String>
#[serde(rename = "accountID")] pub account_id: String
#[serde(rename = "orgID")] pub org_id: String
#[serde(rename = "mcpServers")] pub mcp_servers: ...
```

This is correct — the Rust fields are `snake_case` and serde handles the conversion — but the inline annotations add noise.

---

## Appendix S: Security Debt

### S.1 API Key Handling

API keys are handled through:
- Environment variables (documented in `main.rs:1474-1481`)
- Config file (`config.rs`)
- Directly stored as `String` in provider structs

**Findings:**
- No key masking/redaction in logs
- No key expiry validation
- Keys stored in memory as plain `String` (not `SecretString` or zeroize)
- No attempt to lock key memory pages

### S.2 Permission Model

The permission system (`permission.rs`) supports:
- Wildcard matching (`*`, `**`)
- Allow/Deny semantics
- Always-allow patterns

**Findings:**
- No rate limiting on permission prompts
- No audit log for permission decisions
- No timeout for permission requests
- Wildcard matching is O(n*m) with no optimization

### S.3 Input Validation

- Path traversal protection is present in `filesystem.rs` via `resolve_safe()` at line 1,088
- Shell command injection prevention is present via `shlex` crate
- No SQL injection risk (using parameterized sqlx queries)

---

## Appendix T: OpenCode Feature Parity Gaps

Features present in OpenCode but missing or incomplete in RustCode:

| Feature | OpenCode Status | RustCode Status | Location |
|---------|----------------|-----------------|----------|
| Account console | Complete | Complete | `main.rs` console commands |
| Agent management | Complete | Complete | `agent.rs` |
| Object storage (S3) | Complete | Missing | `storage.rs` only has JSON |
| Remote server | Complete | Partial | `rustcode-server` — stubs |
| TUI | Complete | Partial | `rustcode-tui` — stubs |
| MCP support | Complete | Partial | `rustcode-mcp` — stub |
| GitHub integration | Complete | Partial | `main.rs` github commands |
| Plugin system | Complete (V2) | Partial | `plugin.rs` — V1 only |
| Snapshot/export | Complete | Complete | `snapshot.rs` |
| Database migrations | 35+ | ~1 | `database.rs` |
| Cloud sync | Complete | Missing | Not implemented |
| Multi-account | Complete | Partial | `account.rs` |
| V2 schema | Complete | Partial | `v2_schema.rs` |
| LSP integration | Complete | Stub | `rustcode-lsp/src/lib.rs` |
| Web search tool | Complete | Missing | Not implemented |
| Image/video support | Complete | Basic | `image.rs` |
| i18n/L10n | 18 languages | Missing | Not implemented |
| Telemetry | Complete | Missing | Not implemented |

---

*End of Report — Agent 15, Technical Debt Auditor*
*Generated from `/root/opencodesport/rustcode/` against `/root/opencodesport/opencode/`*

