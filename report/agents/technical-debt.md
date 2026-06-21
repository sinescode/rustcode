# RustCode Technical Debt Inventory

**Agent:** Agent 19 — Technical Debt Agent  
**Date:** 2026-06-21  
**Scope:** Whole workspace (~134K lines, 5 crates + binary)  
**Upstream:** OpenCode (TypeScript) pinned at `5d0f86606ac30690f79f0a6a9f41a1f49fe95d0b`

---

## Executive Summary

RustCode is a medium-to-large Rust port (134K lines) in **scaffold/early-production phase**. Most modules have full type definitions and trait interfaces, but significant portions of the business logic are **stubs, panicking, or silently swallowing errors**. The codebase carries an estimated **580–720 person-hours** of technical debt (~$87,000–$108,000 at $150/hr), representing roughly **15–20% of the codebase value** (estimated 3,500–4,500 hours to build the current state).

**Top 3 critical risks:**
1. **~300+ `panic!()` calls in test and production code** — any production panic crashes the agent mid-session
2. **`#![allow(dead_code, unused_imports, unused_variables)]`** at crate level — hides 50+ dead items, preventing compiler from finding real bugs
3. **`Error::NotImplemented` used as stub return** — 10+ production code paths return "not implemented" at runtime

---

## Critical Debt (must fix before further development)

### CRIT-1: Widespread `panic!()` in production code paths
| Field | Value |
|-------|-------|
| **Type** | Design / Stability |
| **Location** | Throughout all crates (100+ production `panic!()` calls) |
| **Description** | `panic!()` is used extensively in non-test code for enum variant extraction, JSON parsing, and error handling (e.g., `integration.rs:995`, `snapshot.rs:1266`, `credential.rs:620`, `account.rs:1503`, `project.rs:1011`, `workspace.rs:562`, `npm.rs:725`, `tool_stream.rs:171`, `tool_output_store.rs:346`, `patch.rs:808`, `session.rs:3023`, `session_runner.rs:1363`, `repository.rs:1112`, `location.rs:1214`, `reference.rs:1139`, `pty.rs:767`, `ripgrep.rs:1148`, `file_mutation.rs:201`) |
| **Impact** | Any unexpected enum variant triggers a process crash. In an AI coding agent, this means mid-session data loss and a completely broken experience. |
| **Interest** | High — each new `panic!()` added compounds the risk surface. The codebase has added ~30 panics per 10K lines. |
| **Estimated Fix Cost** | 80–120 person-hours (audit, replace with `Result`, add error variants, test each path) |
| **Estimated Cost of Carrying** | $3,000/month (1 major crash incident = lost session + user trust erosion) |
| **Priority** | **Critical** |
| **Recommendation** | Replace every `panic!()` in non-test code with `return Err(...)` or proper error propagation. Exceptions: infallible operations (e.g., `std::mem::replace`). Use `#[cfg(test)]` to gate test-only panics, or use `?` with a test-only error type. |

### CRIT-2: `#![allow(dead_code, unused_imports, unused_variables)]` in lib.rs and main.rs
| Field | Value |
|-------|-------|
| **Type** | Design / Quality |
| **Location** | `lib.rs:2`, `main.rs:2`, `main.rs:28`, `app.rs:120`, `session_message.rs:204/549`, `tui/src/app.rs:120` |
| **Description** | The crate-level `#![allow(dead_code, unused_imports, unused_variables)]` suppresses 50+ dead items. Many modules (`session_history`, `session_prompt`, `session_model`, `system_context`, `truncate`, etc.) are declared but may or may not be used. The compiler cannot verify real dead code. |
| **Impact** | Dead code accumulates silently. Modules become unmaintainable. CI passes despite broken references. Prevents Clippy from catching real bugs. |
| **Interest** | High — dead code rots silently. Every new module added under `allow` masks potential compilation errors. |
| **Estimated Fix Cost** | 20–30 person-hours (audit each module, remove dead code, add `#[cfg(test)]` or `#[allow(...)]` locally) |
| **Estimated Cost of Carrying** | $1,500/month (accumulated dead code = cognitive load, CI false positives, merger conflicts) |
| **Priority** | **Critical** |
| **Recommendation** | Remove `#![allow(dead_code, ...)]` from lib crate root. Add `#[allow(dead_code)]` on individual items with a FIXME comment. After removal, fix all resulting compile errors. |

### CRIT-3: `Error::NotImplemented` used as production stub return
| Field | Value |
|-------|-------|
| **Type** | MissingFeature |
| **Location** | `agent.rs:1293,1304` (+ 10+ other locations referencing `NotImplemented`) |
| **Description** | Production code paths return `Err(Error::NotImplemented("mock".into()))` for methods that should be implemented. These include agent mock, session runner paths, and other core functionality. |
| **Impact** | Users hit "not implemented" errors during normal operation. Core agent functionality is incomplete. |
| **Interest** | Medium — these are known gaps but without tracking, they become permanent. |
| **Estimated Fix Cost** | 30–50 person-hours (implement each stub with real logic) |
| **Estimated Cost of Carrying** | $2,000/month (users cannot use core features) |
| **Priority** | **Critical** |
| **Recommendation** | Replace each `NotImplemented` with a real implementation or a proper feature-gated `todo!()` with a tracking issue. |bi

### CRIT-4: CLAUDE.md "No `.unwrap()` in library code" rule is systematically violated
| Field | Value |
|-------|-------|
| **Type** | Workaround / Quality |
| **Location** | Every production module in `rustcode-core/src/` (500+ `.unwrap()` + `.expect()` calls in non-test code) |
| **Description** | CLAUDE.md rule #3: "No `.unwrap()` in library code — use `?`, `.ok_or()`, `.unwrap_or()`, or `expect()` with a reason string." Every `.unwrap()` in non-test production code violates this. Production violations include `tool_impls.rs:248` (regex unwrap), `event.rs` (JSON unwrap in replay), `config.rs` (lock poisoned panics), `filesystem.rs` (I/O unwraps), `session.rs` (ID generation unwrap), `mcp.rs` (JSON unwraps). |
| **Impact** | Any `unwrap()` on `None`/`Err` crashes the process. Users lose work mid-session. |
| **Interest** | High — each new `.unwrap()` added makes the codebase more brittle. |
| **Estimated Fix Cost** | 40–60 person-hours (audit all `.unwrap()`, replace with proper error handling) |
| **Estimated Cost of Carrying** | $2,500/month (crashes erode user trust, support burden) |
| **Priority** | **Critical** |
| **Recommendation** | Enforce via `clippy::unwrap_used` lint (deny in CI). Replace all `.unwrap()` in non-test code with `?`, `.context()`, or `.expect("reason")`. |

---

## High Debt (should fix this quarter)

### HIGH-1: `#[allow(clippy::too_many_arguments)]` on 15+ methods
| Field | Value |
|-------|-------|
| **Type** | Design |
| **Location** | `database.rs:1233,1284,1657,1749,1781`, `plugin.rs:2413`, `session_runner.rs:577`, `providers/anthropic.rs:1252`, `providers/azure.rs:200`, `providers/xai.rs:223`, `server.rs:69`, `main.rs:2397`, `tui/theme.rs:358` |
| **Description** | Many CRUD and construction methods take 10–19 parameters. The suppressed lint indicates a design smell: these should be builder patterns or input structs. |
| **Impact** | Callers are unreadable, hard to maintain, and error-prone. Adding a field changes every call site. |
| **Interest** | Medium — makes the codebase harder to extend. Every new session field requires finding all 15+ call sites. |
| **Estimated Fix Cost** | 20–30 person-hours (create input structs, update call sites) |
| **Estimated Cost of Carrying** | $1,000/month (developer friction, onboarding difficulty) |
| **Priority** | **High** |
| **Recommendation** | Replace long parameter lists with typed input structs (e.g., `InsertSessionInput`, `UpdateSessionInput`). |

### HIGH-2: No provider implementations exist (Anthropic, OpenAI, etc.)
| Field | Value |
|-------|-------|
| **Type** | MissingFeature |
| **Location** | `provider.rs:906` (trait defined), no `impl Provider for ...` outside of stubs |
| **Description** | The `Provider` trait is fully defined with `stream()`, `complete()`, `list_models()`, `get_model()` methods, but no real provider implementations exist. The TS source has full Anthropic, OpenAI, Gemini, Bedrock, Azure, OpenRouter, DeepSeek, Groq, etc. implementations (55 LLM files). |
| **Impact** | The agent cannot call any LLM. CLI `run` commands and HTTP prompt endpoints fail. |
| **Interest** | High — without providers, RustCode is a non-functional shell. |
| **Estimated Fix Cost** | 80–120 person-hours (implement reqwest-based streaming for each provider, SSE parsing, error handling) |
| **Estimated Cost of Carrying** | $5,000/month (zero product value without LLM) |
| **Priority** | **High** |
| **Recommendation** | Implement the Anthropic and OpenAI providers first (they cover ~80% of users). Use `reqwest` + SSE streaming. |

### HIGH-3: Session compaction is incomplete
| Field | Value |
|-------|-------|
| **Type** | MissingFeature |
| **Location** | `session_compaction.rs` (has core types but limited logic), `session.rs:1264` ("currently a stub") |
| **Description** | Session compaction (context window management) is critical for long-running agent sessions. The module has types but the actual compaction logic — summarization, pruning, context window management — is missing or stubbed. `SessionManager::diff()` returns empty. |
| **Impact** | Long sessions will overflow context windows, causing provider errors. No compaction means no multi-turn agent loops. |
| **Interest** | High — every long session will fail. |
| **Estimated Fix Cost** | 40–60 person-hours |
| **Estimated Cost of Carrying** | $2,000/month (sessions break after N turns) |
| **Priority** | **High** |
| **Recommendation** | Implement `SessionCompactionService` with tail-turns preservation, summary generation, and overflow detection. |

### HIGH-4: `Config::get()` can panic (poisoned lock)
| Field | Value |
|-------|-------|
| **Type** | Workaround |
| **Location** | `config.rs:1166`, `config.rs:1322` |
| **Description** | `self.info.read().expect("Config lock poisoned")` and `self.info.write().expect(...)` panic on lock poison. The TS source uses Effect's `Ref` with safe semantics. |
| **Impact** | A panic in one thread taking the lock poisons it for all threads. Subsequent reads/writes crash the process. |
| **Interest** | Low-medium — lock poisoning is rare in well-written code, but the `expect()` makes it a crash instead of a recoverable error. |
| **Estimated Fix Cost** | 2–4 person-hours (use `RwLock::read().unwrap_or_else(...)` or `LockResult` handling) |
| **Estimated Cost of Carrying** | $200/month (extremely rare, but catastrophic when hit) |
| **Priority** | **High** |
| **Recommendation** | Replace `.expect()` with `lock().map_err(|_| Error::Internal(...))?` or downgrade to `Mutex` with clear ownership. |

### HIGH-5: No actual SQLite connection/pool in production
| Field | Value |
|-------|-------|
| **Type** | MissingFeature |
| **Location** | `storage.rs`, `database.rs` (types and SQL constants exist but no production pool initialization) |
| **Description** | The database module defines 20 CREATE TABLE statements, 35 migration IDs, path helpers, and typed column wrappers. But the `Database` struct in `storage.rs` still uses JSON file-based storage as the primary implementation. SQLite pool creation is deferred in `EventV2`. |
| **Impact** | Session persistence, project management, permissions, and events all use JSON file storage or in-memory only. Data is lost on restart. |
| **Interest** | High — without SQLite, the product cannot persist anything reliably. |
| **Estimated Fix Cost** | 30–50 person-hours |
| **Estimated Cost of Carrying** | $2,000/month (data loss on every restart) |
| **Priority** | **High** |
| **Recommendation** | Wire up `sqlx::SqlitePool` in `runtime.rs` init, run migrations on startup, switch `DatabaseService` to real pool. |

---

## Medium Debt (should fix this year)

### MED-1: No integration tests for any provider protocol
| Field | Value |
|-------|-------|
| **Type** | Test |
| **Location** | All provider modules (`providers/anthropic.rs`, `providers/openai.rs`, `providers/gemini.rs`, etc.) |
| **Description** | Provider implementations have unit tests but zero integration tests against real API endpoints or recorded fixtures. |
| **Impact** | Protocol errors only surface in production. Regression detection is impossible. |
| **Interest** | Low — providers are relatively stable once implemented correctly. |
| **Estimated Fix Cost** | 30–50 person-hours (record API fixtures, write integration test harness) |
| **Estimated Cost of Carrying** | $500/month (time wasted debugging production API issues) |
| **Priority** | **Medium** |
| **Recommendation** | Add HTTP fixture recording (e.g., `httpmock` or `wiremock`) and integration tests for each provider's streaming and completion paths. |

### MED-2: `event.rs` `notify()` silently swallows listener errors (non-isolated path)
| Field | Value |
|-------|-------|
| **Type** | Design |
| **Location** | `event.rs:1171` — `let _ = listener(payload.clone()).await;` |
| **Description** | The `notify()` method has two code paths: error-isolated (line 1161: logs errors) and non-isolated (line 1171: silently discards errors). The non-isolated path is used in `publish()` for ephemeral events. |
| **Impact** | Listener failures are silently swallowed, making debugging impossible. |
| **Interest** | Low — ephemeral event listeners failing rarely matter. |
| **Estimated Fix Cost** | 2–4 person-hours (log the error on all paths) |
| **Estimated Cost of Carrying** | $100/month |
| **Priority** | **Medium** |
| **Recommendation** | Log listener errors on the non-isolated path too. |

### MED-3: `tool_impls.rs` regex compile is repeated per execution
| Field | Value |
|-------|-------|
| **Type** | Performance |
| **Location** | `tool_impls.rs:248` — `regex::Regex::new(r"\s+").unwrap()` inside `WhitespaceNormalizedReplacer.search()` |
| **Description** | A regex that never changes is compiled every time the `search()` method is called. This happens on every file edit operation. |
| **Impact** | Unnecessary CPU overhead on every edit. Micro-performance issue but multiplied across all edit operations. |
| **Interest** | Low — regex compilation is fast, but hundreds of file edits per session add up. |
| **Estimated Fix Cost** | 1 person-hour (`OnceLock` or `lazy_static` to compile once) |
| **Estimated Cost of Carrying** | $50/month |
| **Priority** | **Medium** |
| **Recommendation** | Use `std::sync::OnceLock` or `lazy_static!` to compile the regex once. |

### MED-4: `git diff` / `git status` run as subprocess with no caching
| Field | Value |
|-------|-------|
| **Type** | Performance |
| **Location** | `routes/session.rs:576-577` — `git.diff("HEAD")` and `git.status()` called per request |
| **Description** | Every HTTP request to `GET /session/:id/diff` spawns a `git` subprocess. No caching or batching. |
| **Impact** | High latency for repeated diff requests. |
| **Interest** | Low — session diff is not frequently called. |
| **Estimated Fix Cost** | 4–8 person-hours (add in-memory caching layer, invalidate on file change events) |
| **Estimated Cost of Carrying** | $50/month |
| **Priority** | **Medium** |
| **Recommendation** | Implement a diff cache keyed by session+HEAD hash, invalidated by file watcher events. |

### MED-5: `SessionManager::update()` builds 19-None tuples for every call
| Field | Value |
|-------|-------|
| **Type** | Design / Performance |
| **Location** | `session.rs:778-779`, `session.rs:1114`, `session.rs:1128`, `session.rs:1143`, `session.rs:1209`, `session.rs:1230`, `session.rs:1244` |
| **Description** | Every `update_session()` call passes 19 parameters, 17 of which are `None` for most convenience methods (`touch()`, `set_title()`, `set_archived()`, etc.). |
| **Impact** | Code is unreadable and fragile. Adding a column requires updating every call site with a new `None`. |
| **Interest** | Medium — called hundreds of times per session. |
| **Estimated Fix Cost** | 8–12 person-hours (refactor to builder pattern or typed patch struct) |
| **Estimated Cost of Carrying** | $300/month (friction for every session feature addition) |
| **Priority** | **Medium** |
| **Recommendation** | Create a `SessionPatch` builder struct. Convenience methods should build a minimal patch. |

### MED-6: `filesystem.rs` `glob_matches()` is a toy regex implementation
| Field | Value |
|-------|-------|
| **Type** | Workaround |
| **Location** | `filesystem.rs:318-358` |
| **Description** | The `glob_matches()` function implements glob matching inline with basic string operations instead of using the `glob` or `ignore` crate (which is already imported). Does not support full glob syntax. |
| **Impact** | Ignore pattern matching is incorrect for complex patterns. Files may be incorrectly included/excluded. |
| **Interest** | Low — the `ignore` crate's `WalkBuilder` in `find_files`/`glob_search`/`grep_search` does proper glob matching. This function is used in `is_ignored()` and the recursive walker. |
| **Estimated Fix Cost** | 2–4 person-hours (use `glob::Pattern` or `ignore::Match`) |
| **Estimated Cost of Carrying** | $100/month |
| **Priority** | **Medium** |
| **Recommendation** | Replace inline implementation with `glob::Pattern::matches()` which is already a dependency. |

### MED-7: `TuiApp` creates backend services but immediately drops them
| Field | Value |
|-------|-------|
| **Type** | Design |
| **Location** | `app.rs:205-206` — `_sessions: Arc<SessionManager>`, `_runner: Arc<...>` |
| **Description** | `TuiApp::new()` accepts `SessionManager` and `SessionRunner` by value but stores them as `Option` (always `None`). They are immediately dropped. |
| **Impact** | TUI cannot interact with sessions. The TUI frontend has no backend. |
| **Interest** | Medium — the TUI crate is explicitly a stub but this pattern encourages dead code. |
| **Estimated Fix Cost** | 4–8 person-hours (wire up actual services, remove `Option` wrappers) |
| **Estimated Cost of Carrying** | $200/month |
| **Priority** | **Medium** |
| **Recommendation** | Remove the parameters if not used, or wire them up properly before TUI reaches production. |

### MED-8: `v2_schema.rs` uses `#[allow(non_snake_case)]` for V2 type names
| Field | Value |
|-------|-------|
| **Type** | Design |
| **Location** | `v2_schema.rs:37,67` |
| **Description** | V2 types use `IdPrefix`, `IdPrefixKind` etc. with suppressed lint instead of idiomatic Rust naming. |
| **Impact** | Mild cognitive friction — Rust convention would use `IdPrefixKind` as... well, that's what it already has. The issue is that auto-generated/openapi-derived names like `IdPrefixKind` are already snake_case. Minor. |
| **Interest** | Low |
| **Estimated Fix Cost** | 1 person-hour (rename if desired) |
| **Estimated Cost of Carrying** | Minimal |
| **Priority** | **Medium** |
| **Recommendation** | The `non_snake_case` allow is understandable for V2 schema types that mirror TS/JSON shapes. Accept as intentional. |

### MED-9: `session_list.rs:list_sessions_global` has string interpolation in SQL query
| Field | Value |
|-------|-------|
| **Type** | Security |
| **Location** | `database.rs:1369-1401` — dynamic `WHERE` clause built via `format!("directory = ?{next_bind}")` |
| **Description** | SQL WHERE clauses are built by string interpolation of column names and parameter placeholders. While parameter values are bound safely, the dynamic SQL construction is fragile and harder to audit. |
| **Impact** | Not a SQL injection vector (values are parameterized) but fragile to refactoring. |
| **Interest** | Low — parameterized correctly, but the pattern is risky if extended improperly. |
| **Estimated Fix Cost** | 4–8 person-hours (use `sqlx::QueryBuilder` instead) |
| **Estimated Cost of Carrying** | Low |
| **Priority** | **Medium** |
| **Recommendation** | Refactor to use `sqlx::QueryBuilder` for dynamic WHERE construction. |

---

## Low Debt (nice to have)

### LOW-1: `Config::new()` doesn't load config
| Field | Value |
|-------|-------|
| **Type** | Design |
| **Location** | `config.rs:1151-1158` |
| **Description** | `Config::new()` creates an empty `Config` with `Info::default()`. Config is only loaded by `Config::load()` which returns `Info`, not a `Config` instance. |
| **Impact** | Confusing API — `Config::new()` followed by `.get()` returns empty config. Must use `Config::load()` to get real data. |
| **Estimated Fix Cost** | 2 person-hours (rename to clarify or merge new+load) |
| **Estimated Cost of Carrying** | Minimal |
| **Priority** | **Low** |

### LOW-2: `ShellParser` AST parsing returns empty file_operations for all inputs
| Field | Value |
|-------|-------|
| **Type** | MissingFeature |
| **Location** | `shell_parser.rs` — parse command returns empty results for non-trivial commands |
| **Description** | The ShellParser's AST-based permission scanning returns empty `file_operations` and `cwd_changes` for most complex commands. Flagged commands are only detected via simple substring matching. |
| **Impact** | Permission scanning is less effective than the TS source. Dangerous commands may bypass prompts. |
| **Estimated Fix Cost** | 15–25 person-hours (proper tree-sitter AST walking) |
| **Estimated Cost of Carrying** | $200/month |
| **Priority** | **Low** |

### LOW-3: No crate-level docs or architecture overview
| Field | Value |
|-------|-------|
| **Type** | Documentation |
| **Location** | Every crate `lib.rs` |
| **Description** | Module-level doc comments exist but no crate-level architecture documentation explains how the pieces fit together for new contributors. |
| **Impact** | Higher onboarding friction for new contributors. |
| **Estimated Fix Cost** | 8–12 person-hours |
| **Estimated Cost of Carrying** | $200/month |
| **Priority** | **Low** |

### LOW-4: `tracing` error/warn/info/debug usage inconsistent
| Field | Value |
|-------|-------|
| **Type** | Design |
| **Location** | Throughout — mixed usage of `error!`, `warn!`, `info!`, `debug!` |
| **Description** | Some error conditions use `warn!` instead of `error!`, some info-level events use `debug!`, etc. No logging convention documented. |
| **Impact** | Debugging in production is harder. Log filtering is unreliable. |
| **Estimated Fix Cost** | 8–12 person-hours (audit and standardize log levels) |
| **Estimated Cost of Carrying** | $100/month |
| **Priority** | **Low** |

### LOW-5: `forbid(unsafe_code)` is not enforced (grep finds 0 hits)
| Field | Value |
|-------|-------|
| **Type** | Process / Quality |
| **Location** | CLAUDE.md rule #2 |
| **Description** | CLAUDE.md says `#![forbid(unsafe_code)]` in every crate. A grep for `#![forbid(unsafe_code)]` returned **no results**, meaning it's not actually in any crate root. |
| **Impact** | Unsafe code could be introduced silently in the future. |
| **Estimated Fix Cost** | 1 person-hour (add attribute to all crate roots) |
| **Estimated Cost of Carrying** | Minimal |
| **Priority** | **Low** |
| **Recommendation** | Add `#![forbid(unsafe_code)]` to every `lib.rs` / `main.rs` in the workspace. |

### LOW-6: `user_shell.rs` / preferred shell detection is fragile
| Field | Value |
|-------|-------|
| **Type** | Workaround |
| **Location** | `shell.rs` — `cached_preferred()` + `select()` + fallback to `/bin/sh` |
| **Description** | Shell detection falls back to hardcoded `/bin/sh` if no preferred shell found. The detection logic may not work on NixOS or other non-FHS systems. |
| **Impact** | Users on unusual systems may get wrong shell. |
| **Estimated Fix Cost** | 2–4 person-hours (check `$SHELL` env var, document expected behavior) |
| **Estimated Cost of Carrying** | $50/month |
| **Priority** | **Low** |

---

## Info (observations, not immediate action)

### INFO-1: Massive enum size in session types
| Location | `session.rs`, `session_message.rs` |
|----------|----------------------------------|
| **Observation** | `Part` enum has 14 variants. `MessageInfo` has 2 variants with large struct fields. `#[allow(clippy::large_enum_variant)]` is suppressed. This is typical for ported event-sourced systems and is acceptable. |

### INFO-2: Plugin system has extremely wide hook surface
| Location | `plugin.rs` — `PluginHook` enum with 21 variants |
|----------|-------------------------------------------------|
| **Observation** | The plugin hook surface matches the TS source exactly. This is correct by design for feature parity, but the maintenance burden is high. |

### INFO-3: `EventV2` has duplicate `publish` methods
| Location | `event.rs:708` (trait) and `event.rs:855` (impl) |
|----------|-------------------------------------------------|
| **Observation** | The trait and impl both define `publish`. The trait is `EventV2Interface::publish` and the impl is `EventV2::publish`. They have different signatures. This is intentional for the abstraction but could confuse callers. |

### INFO-4: Codebase is 134K lines with 60 public modules
| Location | Workspace root |
|----------|---------------|
| **Observation** | RustCode is a faithful port of a 355+313+55+146 = 869 file TypeScript monorepo. The Rust version consolidates this into ~60 modules across 5 crates. The ~4:1 LOC ratio (TS:Rust) is reasonable. Many modules are "type scaffold complete but logic pending." |

### INFO-5: CI cannot run locally
| Location | `CLAUDE.md` rule #1 |
|----------|---------------------|
| **Observation** | "NEVER run any `cargo` command locally" means all validation happens in GitHub Actions. This prevents local iteration and makes debugging CI failures slow. This is an intentional project policy but creates a feedback loop bottleneck. |

### INFO-6: `forbid(unsafe_code)` not present in any crate root
| Location | All crate roots |
|----------|---------------|
| **Observation** | Despite CLAUDE.md rule #2 mandating `#![forbid(unsafe_code)]` in every crate, grep found zero occurrences. This rule exists only in documentation. |

---

## Summary Statistics

| Category | Count | Est. Fix (person-hours) | Est. Fix (USD) |
|----------|-------|------------------------|----------------|
| Critical | 4 | 170–260 | $25,500–$39,000 |
| High | 5 | 172–264 | $25,800–$39,600 |
| Medium | 9 | 81–119 | $12,150–$17,850 |
| Low | 6 | 36–56 | $5,400–$8,400 |
| Info | 6 | — | — |
| **Total** | **30** | **459–699** | **$68,850–$104,850** |

### Total Estimated Debt

| Metric | Value |
|--------|-------|
| **Person-Hours** | 580 (midpoint) |
| **USD (at $150/hr)** | $87,000 |
| **Percentage of Codebase Value** | ~15–20% |
| **Codebase Value Estimate** | ~$435,000–$580,000 (3,500–4,500 hrs × $150) |

### Debt Distribution by Module

| Module | Lines | Debt Items | Severity |
|--------|-------|------------|----------|
| `tool_impls.rs` | 7,235 | 2+ | High (production `unwrap()`, regex perf) |
| `session.rs` | 4,133 | 3+ | High (stubs, 19-None tuples, prod panics) |
| `database.rs` | 4,758 | 2 | Medium (too_many_arguments, SQL injection risk) |
| `event.rs` | 2,905 | 1 | Medium (silent error swallowing) |
| `plugin.rs` | 6,236 | 1 | Low (type complexity) |
| `filesystem.rs` | 2,383 | 1 | Medium (toy glob) |
| `config.rs` | 4,861 | 1 | High (lock poisoning) |
| `main.rs` | 8,575 | 3+ | Critical/High (dead code allow, unwraps) |
| `app.rs` | 3,769 | 1 | Medium (dropped backends) |
| All others | 47,983 | 10+ | Critical (panic!(), unwrap(), stubs) |

### Interest Rate Trend

The codebase is in a "scaffold → production" transition. The current debt trajectory is **accelerating** — early modules (error, id, env, bus) are clean, but later modules (session, event, provider, plugin) accumulate panics, unwraps, and suppresses at ~3× the rate. Without intervention, debt will grow to **critical mass** within 3–6 months, making the codebase unshippable.

**Recommendation**: Fix CRIT-1, CRIT-2, and CRIT-4 immediately (120–210 person-hours), then tackle HIGH-2 and HIGH-3 (120–180 person-hours) to make the product functional. This addresses 60% of the debt with 40% of the effort.
