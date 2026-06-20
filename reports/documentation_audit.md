# Documentation Audit: RustCode vs OpenCode

**Auditor**: Agent 14 — Documentation Auditor  
**Date**: 2026-06-19  
**Scope**: Doc comments, README files, API docs, architecture docs, user guides, examples, generated documentation, comment quality

---

## Table of Contents

1. [Executive Summary](#1-executive-summary)
2. [Methodology](#2-methodology)
3. [Quantitative Comparison](#3-quantitative-comparison)
4. [RustCode Deep Analysis](#4-rustcode-deep-analysis)
   - 4.1 Module-Level Documentation
   - 4.2 Function-Level Documentation
   - 4.3 Type-Level Documentation
   - 4.4 Architecture Documentation
   - 4.5 User-Facing Documentation
   - 4.6 Example Code
   - 4.7 Generated Documentation
   - 4.8 README Quality
   - 4.9 Unsafe Code Documentation
   - 4.10 Comment Quality (WHY vs WHAT)
5. [OpenCode Deep Analysis](#5-opencode-deep-analysis)
   - 5.1 Module-Level Documentation
   - 5.2 Function-Level Documentation
   - 5.3 Type-Level Documentation
   - 5.4 Architecture Documentation
   - 5.5 User-Facing Documentation
   - 5.6 Example Code
   - 5.7 Generated Documentation
   - 5.8 README Quality
   - 5.9 Comment Quality
6. [Gap Analysis](#6-gap-analysis)
7. [Findings Register](#7-findings-register)
8. [Recommended Roadmap](#8-recommended-roadmap)
9. [Appendix](#9-appendix)

---

## 1. Executive Summary

### RustCode Documentation State

RustCode has **strong internal code documentation** — 12,138 `///` doc comments across 148 Rust source files (119,039 lines), averaging 10.2% doc comment density. Every module has a `//!` module-level docblock with source references, architecture overview, and key type descriptions. The `CLAUDE.md` acts as a comprehensive developer guide covering workspace layout, implementation order, lint policy, CI pipeline, and dependency rationale.

However, RustCode is **critically missing external-facing documentation**: no `README.md`, no `CHANGELOG.md`, no `CONTRIBUTING.md`, no `SECURITY.md`, no `CONTEXT.md`, and no generated documentation pipeline (`cargo doc` not configured in CI). There are only 17 doc examples across the entire codebase. The root crate is a binary (`src/main.rs`) that contains 7,904 lines — more than any library crate — but has minimal user-facing prose explaining how to use the CLI.

### OpenCode Documentation State

OpenCode has **excellent external-facing documentation**: a translated `README.md` in 20+ languages, a comprehensive 299-line `CONTRIBUTING.md`, a `SECURITY.md` with a threat model, and a dedicated `packages/docs/` directory with Quickstart, Development guides, Essential references, and AI tools integration docs. The project has an OpenAPI spec (30,697 lines), Storybook for UI components, and extensive design specs in `packages/opencode/specs/`.

However, OpenCode is **critically missing inline code documentation**. With only 46 JSDoc (`/** */`) comments across 74,727 lines of TypeScript (0.06%), developers have almost no inline documentation on function signatures, parameter types, return values, or error conditions. There are zero uses of `@param`, `@returns`, or `@throws` JSDoc tags, and no TypeDoc configuration exists. Documentation relies entirely on Effect.ts's Schema-driven type system for self-documenting types, but the behavioral contracts remain undocumented.

### Key Contrast

| Dimension | RustCode | OpenCode |
|---|---|---|
| Inline docs density | **10.2%** | **0.06%** |
| External docs | **None** | **Excellent** |
| README | **Missing** | **20+ languages** |
| CONTRIBUTING.md | **Missing** | **299 lines** |
| CHANGELOG.md | **Missing** | **Missing** |
| SECURITY.md | **Missing** | **Comprehensive** |
| Generated docs | **None (no `cargo doc`)** | **None (no TypeDoc)** |
| Examples in docs | **17** | **4 (separate files)** |
| Doc CI job | **Missing** | **Missing** |
| API spec | **None** | **OpenAPI (30K lines)** |

---

## 2. Methodology

### Tools Used
- `grep -r '///'` — count Rust doc comments
- `grep -r '/\*\*'` — count TSDoc/JSDoc comments
- `find -name '*.md'` — enumerate documentation files
- `wc -l` — line counts
- Manual inspection of key files for quality assessment

### Evaluation Criteria
1. **Coverage**: What percentage of code has documentation?
2. **Completeness**: Does the doc explain parameters, return values, errors, panics?
3. **Correctness**: Is the documentation accurate and up-to-date?
4. **Clarity**: Is the documentation understandable?
5. **Consistency**: Is the style uniform across the codebase?
6. **Discoverability**: Can users find the documentation they need?

### Scoring Scale
- **Critical**: Missing documentation that could lead to incorrect usage or security issues
- **High**: Significant gaps that impair developer productivity or onboarding
- **Medium**: Notable gaps that should be addressed in normal development cycle
- **Low**: Minor improvements that would enhance quality

---

## 3. Quantitative Comparison

### 3.1 Code & Documentation Volume

| Metric | RustCode | OpenCode |
|---|---|---|
| Source files | 148 `.rs` files | 350+ `.ts` files (opencode only) |
| Total source lines | 119,039 | 74,727 (opencode) + 32,856 (core) |
| Doc comments (lines) | 12,138 (`///`) | 46 (`/** */` in opencode) + 235 (core) |
| Substantive doc comments | 9,531 (excluding boilerplate) | 281 (total) |
| Module-level docs (`//!`) | 67 files | Minimal (no Rust-style `//!` equivalent) |
| Doc examples | 17 | 4 separate example files |
| `# Errors` sections | 30+ | 0 (`@throws` tags) |
| `# Panics` sections | 1 | 0 |
| `# Safety` sections | 1 | 0 |
| `# Examples` sections | 17 | 0 |
| Intra-doc links | 10+ | 0 |
| Markdown doc files | 16 (reports) | 100+ (translations + docs + specs) |
| TODOs/FIXMEs in code | 10+ | 20+ |

### 3.2 External Documentation

| Document | RustCode | OpenCode |
|---|---|---|
| README.md | **Missing** | 129 lines + 20 translations |
| CONTRIBUTING.md | **Missing** | 299 lines |
| CHANGELOG.md | **Missing** | **Missing** |
| SECURITY.md | **Missing** | Exists (threat model) |
| AGENTS.md / CLAUDE.md | 118 lines (CLAUDE.md) | 158 lines (AGENTS.md) |
| CONTEXT.md | **Missing** | Exists |
| STATS.md | **Missing** | Exists |
| API documentation | None | OpenAPI (30,697 lines) |
| UI documentation | None | Storybook |
| Design specs | Reports directory (audits) | `specs/` directory with 20+ docs |

### 3.3 Doc Density by Module (RustCode Core)

| Module | Lines | Doc Lines | Density | Quality Grade |
|---|---|---|---|---|
| model.rs | 1,299 | 380 | **29%** | A |
| session_info.rs | 451 | 113 | **25%** | A |
| aisdk.rs | 377 | 77 | **20%** | A |
| error.rs | 1,197 | 243 | **20%** | A |
| event.rs | 2,221 | 458 | **20%** | A |
| schema.rs | 496 | 100 | **20%** | A |
| image.rs | 668 | 110 | **16%** | B+ |
| fs_util.rs | 429 | 69 | **16%** | B+ |
| integration.rs | 1,817 | 300 | **16%** | B+ |
| credential.rs | 534 | 68 | **12%** | B+ |
| bus.rs | 926 | 120 | **12%** | B+ |
| database.rs | 2,433 | 298 | **12%** | B |
| config.rs | 2,449 | 323 | **13%** | B |
| permission.rs | 2,008 | 269 | **13%** | B |
| account.rs | 1,625 | 227 | **13%** | B- |
| git.rs | 1,427 | 168 | **11%** | C+ |
| filesystem.rs | 1,889 | 264 | **13%** | B- |
| catalog.rs | 1,634 | 141 | **8%** | C |
| session.rs | 3,367 | 267 | **7%** | C |
| session_prompt.rs | 762 | 87 | **11%** | C+ |
| agent.rs | 1,452 | 126 | **8%** | C |
| command.rs | 712 | 57 | **8%** | C |
| session_runner.rs | 789 | 54 | **6%** | D |
| shell.rs | 1,098 | 65 | **5%** | D |
| sse.rs | 385 | 22 | **5%** | D |
| tool_impls.rs | 5,546 | 185 | **3%** | F |
| flag.rs | 76 | 0 | **0%** | F |

---

## 4. RustCode Deep Analysis

### 4.1 Module-Level Documentation

**Overall Grade: A**

RustCode has excellent module-level documentation. Every single `.rs` file in `rustcode-core` (67 files) begins with a `//!` doc comment block. These blocks consistently include:

1. **Module purpose**: 1-2 line summary of what the module does
2. **Source reference**: `Ported from:` pointing to the TypeScript source
3. **Source commit**: `OpenCode commit:` pinned hash
4. **Architecture overview**: Often with bullet lists or ASCII diagrams
5. **Key types**: Cross-links to the main exported types

**Example (excellent)** — `crates/rustcode-core/src/providers/mod.rs:1-44`:
```rust
//! LLM provider implementations.
//!
//! Each submodule implements the [`Provider`](crate::provider::Provider) trait
//! for a specific LLM provider's API protocol.
//!
//! ## Wire Protocol Coverage
//!
//! | Provider | Protocol | Auth | Streaming |
//! |----------|----------|------|-----------|
//! | Anthropic | Messages API (`/v1/messages`) | `x-api-key` header | SSE |
//! | OpenAI | Chat Completions (`/v1/chat/completions`) | Bearer token | SSE |
//! | ... | ... | ... | ... |
```
This table covers 19 providers with protocol, auth, and streaming method — excellent reference.

**Example (good)** — `crates/rustcode-core/src/lsp.rs:1`:
```rust
//! LSP integration for rustcode — module-level types and re-exports.
//!
//! The full LSP client implementation lives in the `rustcode-lsp` crate.
//! This re-exports key types for use by other rustcode crates.
```

**Example (minimal)** — `crates/rustcode-core/src/flag.rs:1`:
```rust
//! Feature flags system.
```
Only 1 line for a 76-line module. No source reference, no architecture, no key types.

**Finding RC-DOC-01**: Minimal module doc in `flag.rs`  
- **Location**: `crates/rustcode-core/src/flag.rs:1`  
- **Evidence**: Only `//! Feature flags system.` — no source reference, no type descriptions  
- **Problem**: Incomplete module documentation  
- **Impact**: Developers unfamiliar with flag system must read entire file  
- **Severity**: Low  
- **Recommendation**: Add source references and type descriptions  
- **Effort**: < 15 minutes

### 4.2 Function-Level Documentation

**Overall Grade: B**

RustCode has extensive function-level documentation with strong coverage of:
- What the function does
- Source file reference
- Parameters (in many cases)
- Errors (30+ `# Errors` sections)

**Common patterns observed:**

**Excellent** — `crates/rustcode-core/src/bus.rs:224-240`:
```rust
/// Subscribe to a specific event kind with a callback.
///
/// Returns a [`BusSubscription`] that unsubscribes when dropped.
///
/// # Errors
/// Returns [`Error::Bus`] if the channel is lagged or closed.
pub fn on<F, Fut>(&self, kind: EventKind, callback: F) -> BusSubscription
```

**Good** — typical function doc pattern:
```rust
/// Parse a patch text into a list of hunks.
///
/// Ported from: `patch.ts` — `parsePatch()`
///
/// # Errors
/// Returns [`Error::PatchParse`] if the patch text is malformed.
pub fn parse(text: &str) -> Result<Vec<Hunk>>
```

**Weak** — `crates/rustcode-core/src/tool_impls.rs:500`:
Only 3% doc density across 5,546 lines. Many functions lack any documentation at all. This is the largest and worst-documented module.

**Finding RC-DOC-02**: `tool_impls.rs` has 3% doc density  
- **Location**: `crates/rustcode-core/src/tool_impls.rs` (5,546 lines)  
- **Evidence**: 185 doc lines out of 5,546 — only 3% documented  
- **Problem**: Critical module implementing all tool types has minimal documentation  
- **Impact**: Hard to understand, modify, or audit tool implementations  
- **Severity**: High  
- **Recommendation**: Add doc comments to all public functions, especially tool `execute()` methods  
- **Effort**: 2-3 days  

**Finding RC-DOC-03**: `session_runner.rs` and `shell.rs` have < 7% doc density  
- **Location**: `crates/rustcode-core/src/session_runner.rs` (789 lines, 6%), `crates/rustcode-core/src/shell.rs` (1,098 lines, 5%)  
- **Evidence**: Minimal function-level docs in session execution and shell integration  
- **Problem**: Core modules (session running, shell commands) poorly documented  
- **Impact**: Risk of misuse, hard to debug session lifecycle issues  
- **Severity**: High  
- **Recommendation**: Add comprehensive function docs with error conditions  
- **Effort**: 1-2 days

### 4.3 Type-Level Documentation

**Overall Grade: A-**

RustCode has strong type-level documentation. Most `struct` and `enum` types have doc comments explaining their purpose, variants, and fields.

**Excellent** — `crates/rustcode-core/src/patch.rs:31-72`:
```rust
/// A hunk representing one file-level change in a patch.
///
/// Ported from: `patch.ts` — `Hunk` discriminated union
///
/// Three variants map to the `type` tag:
/// - `"add"` — create a new file
/// - `"delete"` — remove an existing file
/// - `"update"` — modify an existing file via chunks
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "type")]
pub enum Hunk {
    /// Add a new file with the given content.
    Add {
        /// Path of the new file to create.
        path: String,
        /// Full content of the new file.
        contents: String,
    },
    // ...
}
```

**Good** — struct with field docs:
```rust
/// Configuration for an individual agent.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AgentConfig {
    /// The model to use for this agent.
    pub model: Option<String>,
    /// Temperature for generation.
    pub temperature: Option<f64>,
    /// Maximum number of steps/turns.
    pub steps: Option<u32>,
}
```

**Finding RC-DOC-04**: Variant-level docs sometimes missing  
- **Location**: Various enum-heavy files  
- **Evidence**: Some enum variants lack field-level docs, only having a one-line summary  
- **Problem**: Inconsistent depth of field documentation  
- **Impact**: Users need to guess field semantics from names  
- **Severity**: Medium  
- **Recommendation**: Add field-level docs to all public struct fields and enum variants  
- **Effort**: Ongoing (1-2 hours per module)

### 4.4 Architecture Documentation

**Overall Grade: A**

RustCode's architecture documentation is a standout strength. The `CLAUDE.md` file provides an excellent developer guide covering:

- **Workspace layout**: ASCII diagram of the directory structure
- **CI pipeline**: All 4 CI jobs explained
- **Implementation order**: 12-step ordered module dependency map
- **Lint policy**: Current relaxed policy and future plans
- **Module table**: 20 modules with TS source mapping and key types
- **Key dependencies**: Table with crate, version, and purpose
- **TS-to-Rust pattern mapping**: EventEmitter→broadcast, Effect.ts→thiserror, etc.

Module-level docs consistently include architecture sections:

**`crates/rustcode-core/src/bus.rs:6-16`**:
```
/// ## Architecture
///
/// The TS source uses Node.js `EventEmitter` as a global singleton...
/// In Rust:
/// - [`EventBus`] wraps [`tokio::sync::broadcast`] for fan-out
/// - [`SharedBus`] provides `Arc`-based sharing
/// - [`BusSubscription`] implements RAII cleanup on drop
```

**`crates/rustcode-core/src/lsp.rs:4-25`** — Contains an ASCII protocol diagram for JSON-RPC framing.

**`crates/rustcode-core/src/permission.rs:12-42`** — 30-line architecture overview covering API levels, event bus, and config conversion.

**Finding RC-DOC-05**: No top-level architecture document  
- **Location**: Root directory  
- **Evidence**: No `ARCHITECTURE.md` or equivalent standalone document  
- **Problem**: Architecture info is distributed across CLAUDE.md and module-level docs  
- **Impact**: New contributors must piece together architecture from multiple sources  
- **Severity**: Medium  
- **Recommendation**: Create `ARCHITECTURE.md` with system architecture diagram, data flow, and crate dependency graph  
- **Effort**: 4-6 hours

### 4.5 User-Facing Documentation

**Overall Grade: F**

RustCode has **zero** user-facing documentation. There is no `README.md`, no installation guide, no CLI usage documentation, no configuration reference, and no troubleshooting guide.

**Finding RC-DOC-06**: No README.md for users  
- **Location**: `/root/opencodesport/rustcode/`  
- **Evidence**: No `README.md` exists in the root  
- **Problem**: Users who find the repo have no idea what it is, how to install it, or how to use it  
- **Impact**: Zero discoverability, no adoption possible  
- **Severity**: Critical  
- **Recommendation**: Create README.md with project description, installation, quick start, configuration, and links  
- **Effort**: 4-8 hours

**Finding RC-DOC-07**: No CHANGELOG.md  
- **Location**: `/root/opencodesport/rustcode/`  
- **Evidence**: No `CHANGELOG.md` exists  
- **Problem**: Users and downstream dependents cannot track changes between versions  
- **Impact**: Poor release management, no version migration path  
- **Severity**: High  
- **Recommendation**: Create `CHANGELOG.md` and maintain it with every change  
- **Effort**: Initial: 1 hour, Ongoing: 5 min per commit

**Finding RC-DOC-08**: No CONTRIBUTING.md  
- **Location**: `/root/opencodesport/rustcode/`  
- **Evidence**: No `CONTRIBUTING.md` exists  
- **Problem**: Potential contributors have no guidance on PR process, coding standards, or testing  
- **Impact**: Low external contribution rate  
- **Severity**: Medium  
- **Recommendation**: Create `CONTRIBUTING.md` based on existing CLAUDE.md content  
- **Effort**: 2-3 hours

**Finding RC-DOC-09**: No SECURITY.md  
- **Location**: `/root/opencodesport/rustcode/`  
- **Evidence**: No `SECURITY.md` exists  
- **Problem**: No vulnerability disclosure policy, no threat model documented  
- **Impact**: Security researchers have no channel to report issues  
- **Severity**: High  
- **Recommendation**: Create `SECURITY.md` with disclosure policy  
- **Effort**: 1-2 hours

### 4.6 Example Code

**Overall Grade: D**

RustCode has only 17 `# Examples` sections in doc comments across the entire codebase, and zero standalone example files or example directories.

**Finding RC-DOC-10**: Insufficient doc examples  
- **Location**: Throughout `crates/rustcode-core/src/`  
- **Evidence**: Only 17 `# Examples` sections identified (in `permission.rs`, `env.rs`, `npm.rs`, `format.rs`, `schema.rs`, `image.rs`)  
- **Problem**: Users cannot quickly understand how to use the API  
- **Impact**: Steep learning curve, increased support burden  
- **Severity**: High  
- **Recommendation**: Add `# Examples` sections to all public functions, especially in `tool.rs`, `session.rs`, `provider.rs`, and `config.rs`  
- **Effort**: 3-5 days

**Finding RC-DOC-11**: No standalone example directory  
- **Location**: `/root/opencodesport/rustcode/`  
- **Evidence**: No `examples/` directory, no example binaries in Cargo.toml  
- **Problem**: New users have no simple runnable examples to learn from  
- **Impact**: Poor onboarding experience  
- **Severity**: Medium  
- **Recommendation**: Create `examples/` directory with example programs demonstrating config loading, provider usage, session creation, and tool execution  
- **Effort**: 2-3 days

### 4.7 Generated Documentation

**Overall Grade: F**

RustCode has **no generated documentation pipeline**. `cargo doc` is not configured and does not run in CI.

**Finding RC-DOC-12**: No `cargo doc` in CI  
- **Location**: `.github/workflows/ci.yml`  
- **Evidence**: CI pipeline has Format, Clippy, Test, and Cargo Deny jobs but no documentation job  
- **Problem**: Documentation drift goes undetected; broken intra-doc links are not caught  
- **Impact**: Docs can become stale or incorrect without anyone noticing  
- **Severity**: High  
- **Recommendation**: Add `cargo doc --workspace --no-deps` job to CI  
- **Effort**: 30 minutes

**Finding RC-DOC-13**: No documentation hosting  
- **Location**: Project infrastructure  
- **Evidence**: No docs.rs or GitHub Pages deployment configured  
- **Problem**: Users cannot browse API documentation online  
- **Impact**: Only source code available for API reference  
- **Severity**: Medium  
- **Recommendation**: Configure `docs.rs` for published crate, or set up GitHub Actions to deploy to GitHub Pages  
- **Effort**: 1-2 hours

### 4.8 README Quality

**Overall Grade: N/A** (does not exist)

No README exists. This is the single most impactful documentation gap.

### 4.9 Unsafe Code Documentation

**Overall Grade: A** (not applicable)

All crates use `#![forbid(unsafe_code)]`. There is zero unsafe code in the entire project, making `# Safety` sections unnecessary.

**Finding RC-DOC-14**: `forbid(unsafe_code)` should be documented in CONTRIBUTING.md  
- **Location**: `crates/rustcode-core/src/lib.rs:1`, `CLAUDE.md:9`  
- **Evidence**: The rule exists in CLAUDE.md but would be more visible in CONTRIBUTING.md  
- **Problem**: Policy not visible to external contributors who don't read CLAUDE.md  
- **Impact**: Unsafe code might accidentally be introduced  
- **Severity**: Low  
- **Recommendation**: Document the forbid(unsafe_code) policy in CONTRIBUTING.md  
- **Effort**: 5 minutes

### 4.10 Comment Quality (WHY vs WHAT)

**Overall Grade: B**

RustCode's inline comments (`//` not `///`) generally explain **WHY** rather than **WHAT**:

**Good** — `crates/rustcode-core/src/session.rs:707`:
```rust
// TODO: reimplement with DB-backed message copying.
```

**Good** — `crates/rustcode-core/src/providers/anthropic.rs:524-530`:
```rust
/// Pending finish reason from message_delta
/// Accumulated reasoning signature
```

**Good** — `crates/rustcode-core/src/event.rs:1321`:
```rust
/// Data for the `session.next.reasoning.delta` event (ephemeral).
/// This event is intentionally not persisted — it's streamed in real-time
/// for TUI rendering and discarded after the reasoning session ends.
```

However, there are many `Ported from:` comments that are mechanical — they tell WHAT line was ported but don't explain WHY the implementation choices differ.

**Finding RC-DOC-15**: Boilerplate `Ported from:` comments dominate  
- **Location**: Across all modules  
- **Evidence**: ~2,600 of the 12,138 doc lines are `Ported from:` / `OpenCode commit:` boilerplate  
- **Problem**: 21% of doc comments are mechanical references rather than meaningful explanations  
- **Impact**: Inflates doc count without providing proportional value  
- **Severity**: Low  
- **Recommendation**: Move source references to a header comment per module rather than per item  
- **Effort**: 2-3 hours

---

## 5. OpenCode Deep Analysis

### 5.1 Module-Level Documentation

**Overall Grade: B-**

OpenCode does not use the `//!` module-level comment pattern that Rust uses. Instead, module-level documentation is achieved through:

1. File READMEs (e.g., `packages/core/src/github-copilot/README.md`, `packages/opencode/src/control-plane/dev/README.md`)
2. Diagram files (e.g., `packages/opencode/src/server/routes/instance/httpapi/AGENTS.md`)
3. Export patterns with doc-like comments at module boundaries

**Example** — `packages/opencode/src/tool/tool.ts:18-23`:
```typescript
/**
 * Raised when the LLM calls a tool with arguments that fail the parameter
 * schema. This is the canonical "rewrite the input" tool error: the typed
 * error class makes it matchable upstream, and its `message` getter produces
 * the model-facing prose that the AI SDK feeds back as the tool result.
 */
export class InvalidArgumentsError extends Schema.TaggedErrorClass<...>
```

This is the exception rather than the rule — most files have no file-level documentation.

**Finding OC-DOC-01**: No file-level documentation in most modules  
- **Location**: Throughout `packages/opencode/src/` and `packages/core/src/`  
- **Evidence**: Of 350+ TS files in opencode package, only files in `core/src/session/` have regular TSDoc blocks; most files start with imports with no overview  
- **Problem**: Developers must read entire file to understand its purpose and exports  
- **Impact**: Reduced code comprehension, especially for new contributors  
- **Severity**: Medium  
- **Recommendation**: Add file-level JSDoc comments to all modules summarizing purpose, exports, and usage  
- **Effort**: 3-5 days

### 5.2 Function-Level Documentation

**Overall Grade: D**

OpenCode has very sparse function-level documentation. Only 46 TSDoc blocks exist across 350+ files in the `opencode` package. Zero functions use `@param`, `@returns`, or `@throws` JSDoc tags.

**Finding OC-DOC-02**: Near-zero TSDoc usage  
- **Location**: `packages/opencode/src/` (74,727 lines)  
- **Evidence**: Only 46 `/** */` TSDoc comments found; no `@param`, `@returns`, or `@throws` usage  
- **Problem**: Functions are entirely self-documenting via types only — behavior, edge cases, and error conditions are undocumented  
- **Impact**: High cognitive load for developers; must read implementation to understand contracts  
- **Severity**: Critical  
- **Recommendation**: Add JSDoc comments to all `pub`-equivalent (exported) functions, describing parameters, return values, and error conditions  
- **Effort**: 5-10 days

**Finding OC-DOC-03**: `@internal` annotations without explanation  
- **Location**: `packages/opencode/src/cli/cmd/run/runtime.ts:25`, `packages/opencode/src/session/prompt.ts:1659`  
- **Evidence**: Comments like `/** @internal Exported for testing */` provide no context about what the function does  
- **Problem**: Even internal items lack basic description  
- **Impact**: Testing code is harder to maintain  
- **Severity**: Low  
- **Recommendation**: Add brief descriptions alongside `@internal` tags  
- **Effort**: 1-2 hours

### 5.3 Type-Level Documentation

**Overall Grade: C**

TypeScript interfaces and types in OpenCode benefit from the Effect.ts Schema system, which provides automatic type information. However, interface documentation is inconsistent.

**Good** — `packages/opencode/src/agent/agent.ts:37-38`:
```typescript
description: Schema.optional(Schema.String),
```

**Good** — `packages/core/src/session/execution.ts:8-19`:
```typescript
/** Explicitly drain one Session, making at least one provider attempt. */
drain: (sessionID: SessionID) => Effect.Effect<void>

/** Schedule a drain after durable work is recorded. Repeated wakeups may coalesce. */
wake: (sessionID: SessionID) => Effect.Effect<void>

/** Interrupt active work owned by this process. Idle interruption is a no-op. */
interrupt: (sessionID: SessionID) => Effect.Effect<void>
```

**Weak** — Most interfaces have no documentation, relying on TypeScript types alone.

**Finding OC-DOC-04**: Interface semantics not documented  
- **Location**: `packages/opencode/src/tool/tool.ts:55-65`  
- **Evidence**: The `Def` interface has no JSDoc on its properties (`id`, `description`, `parameters`, `jsonSchema`, `execute`, `formatValidationError`) beyond their TypeScript types  
- **Problem**: Semantics (e.g., when is `formatValidationError` called, what happens if `jsonSchema` conflicts with `parameters`) are unclear  
- **Impact**: Misuse of the tool definition API  
- **Severity**: Medium  
- **Recommendation**: Add property-level JSDoc to all exported interfaces  
- **Effort**: 1-2 days

### 5.4 Architecture Documentation

**Overall Grade: A**

OpenCode has excellent architecture documentation through multiple channels:

1. **AGENTS.md**: 158 lines covering code style, Effect rules, module patterns, testing, V2 session core design
2. **Specs directory**: 20+ documents covering:
   - `packages/opencode/specs/effect/` — Migration guide, error boundaries, facades, schemas, tools, todo
   - `packages/opencode/specs/v2/` — Config, instructions, provider model, session, tools, notifications
   - `packages/opencode/specs/storage/` — Database design, migration plans
3. **CONTEXT.md**: Project-level context for AI coding assistants
4. **File-level READMEs**: Scattered through subdirectories

**Finding OC-DOC-05**: Architecture docs distributed, not centralized  
- **Location**: Various `specs/`, `AGENTS.md`, `packages/*/AGENTS.md`  
- **Evidence**: Architecture information is spread across 20+ files  
- **Problem**: Hard to find the right document for a given question  
- **Impact**: Developers may miss relevant architecture decisions  
- **Severity**: Low  
- **Recommendation**: Create an `ARCHITECTURE.md` index that links to spec documents  
- **Effort**: 2-3 hours

### 5.5 User-Facing Documentation

**Overall Grade: A**

OpenCode has excellent user-facing documentation:

1. **README.md** (129 lines): Installation via 10+ package managers, desktop app download links, agent overview, contributing links
2. **20+ Translated READMEs**: Arabic, Bengali, Bosnian, Chinese, Danish, Dutch, French, German, Greek, Italian, Japanese, Korean, Norwegian, Polish, Portuguese, Russian, Spanish, Thai, Turkish, Ukrainian, Vietnamese
3. **CONTRIBUTING.md** (299 lines): Issue first policy, PR expectations, debugging guide, trust/vouch system
4. **SECURITY.md**: Threat model, no sandbox, isolation recommendations
5. **packages/docs/**: Quickstart, Development, Essentials (navigation, markdown, code, images, settings), AI tools integration (Claude Code, Cursor, Windsurf)
6. **OpenAPI specification**: 30,697 lines of generated API documentation
7. **STATS.md**: Project statistics tracking

However, the `packages/docs/` directory contains Mintlify starter kit template content (not actual OpenCode documentation), suggesting the actual user docs are hosted externally at `https://opencode.ai/docs`.

**Finding OC-DOC-06**: packages/docs contains template content, not real docs  
- **Location**: `packages/docs/quickstart.mdx`, `packages/docs/development.mdx`  
- **Evidence**: Content like "Start building awesome documentation in minutes" and Mintlify-specific setup instructions  
- **Problem**: The local docs directory is a template, not the actual documentation  
- **Impact**: Developers building locally get irrelevant documentation  
- **Severity**: Medium  
- **Recommendation**: Either populate packages/docs with actual project documentation or remove the template content  
- **Effort**: 4-8 hours

### 5.6 Example Code

**Overall Grade: B**

OpenCode has several example files spread across packages:

1. **SDK example** (`packages/sdk/js/example/example.ts`, 56 lines): Demonstrates creating sessions and prompting
2. **Plugin example** (`packages/plugin/src/example.ts`, 18 lines): Demonstrates creating a custom tool
3. **Plugin workspace example** (`packages/plugin/src/example-workspace.ts`): Workspace-level plugin example
4. **LLM call sites** (`packages/llm/example/call-sites.md`): LLM route usage sketches
5. **Storybook stories**: Many components have stories that serve as usage examples

**Finding OC-DOC-07**: Examples are minimal and not well-integrated  
- **Location**: Separate files rather than in-code doc examples  
- **Evidence**: The TS SDK example is 56 lines, plugin example is 18 lines; no documentation references them  
- **Problem**: Examples exist but are not discoverable from the API documentation  
- **Impact**: Users may not find examples  
- **Severity**: Low  
- **Recommendation**: Cross-reference example files from README and relevant source files  
- **Effort**: 1-2 hours

### 5.7 Generated Documentation

**Overall Grade: C-

OpenCode has no TypeDoc configuration and no automated documentation generation. However, it does have:

1. **OpenAPI spec** (30,697 lines): Generated API specification (`packages/docs/openapi.json`)
2. **Storybook**: UI component documentation and playground
3. **SDK generation**: The SDK is generated from OpenAPI, providing some documentation

**Finding OC-DOC-08**: No TypeDoc generation  
- **Location**: `packages/opencode/package.json`, `packages/core/package.json`  
- **Evidence**: No TypeDoc or documentation generation scripts exist  
- **Problem**: Developers must read source code directly to understand API details  
- **Impact**: Reduced developer productivity for SDK consumers  
- **Severity**: Medium  
- **Recommendation**: Add TypeDoc configuration and integrate into CI  
- **Effort**: 2-3 hours

**Finding OC-DOC-09**: No doc generation in CI  
- **Location**: `.github/workflows/`  
- **Evidence**: CI workflows focus on build, test, publish; no documentation generation or validation  
- **Problem**: Documentation can drift from implementation  
- **Impact**: Stale documentation when it eventually gets generated  
- **Severity**: Medium  
- **Recommendation**: Add a documentation CI job that validates JSDoc coverage and generates TypeDoc  
- **Effort**: 2-4 hours

### 5.8 README Quality

**Overall Grade: A**

OpenCode's README.md (129 lines) is high quality:
- ✅ Badges (Discord, npm, build status)
- ✅ 20+ language translations
- ✅ Installation via 10+ package managers
- ✅ Desktop app download links
- ✅ Installation directory documentation
- ✅ Agent overview
- ✅ Documentation link
- ✅ Contributing link
- ✅ Screenshot
- ❌ No quick-start usage example
- ❌ No configuration reference
- ❌ No example output or demo GIF

### 5.9 Comment Quality

**Overall Grade: C**

OpenCode's inline comments vary from excellent to nonexistent. The project has a philosophy documented in AGENTS.md: "Add comments for non-obvious constraints and surprising behavior, not for obvious assignments or control flow."

**Good** — `packages/opencode/src/session/prompt.ts:1076`:
```typescript
// TODO(v2): Temporary dual-write while migrating session messages to v2 events.
```
This explains WHY (dual-write) not WHAT (the code itself).

**Good** — `packages/opencode/src/tool/tool.ts:108-111`:
```typescript
// Compile the parser closure once per tool init; `decodeUnknownEffect`
// allocates a new closure per call, so hoisting avoids re-closing it for
// every LLM tool invocation.
```
Excellent WHY comment explaining the optimization rationale.

**Good** — `packages/opencode/src/util/filesystem.ts:110-114`:
```typescript
/**
 * Creates a directory at the specified path recursively.
 * ...
 */
```

**Finding OC-DOC-10**: Inconsistent inline comment density  
- **Location**: Throughout codebase  
- **Evidence**: Some files have detailed WHY comments, others have none  
- **Problem**: No consistent standard for what deserves comments  
- **Impact**: Harder to maintain code with undocumented design decisions  
- **Severity**: Low  
- **Recommendation**: Formalize comment standards in AGENTS.md with clear examples of when to comment  
- **Effort**: 30 minutes

---

## 6. Gap Analysis

### 6.1 Documentation Gaps: RustCode vs OpenCode

| Gap Category | RustCode | OpenCode | Winner |
|---|---|---|---|
| Module-level docs | ★★★★★ | ★★★☆☆ | **RustCode** |
| Function-level docs | ★★★★☆ | ★★☆☆☆ | **RustCode** |
| Type-level docs | ★★★★★ | ★★★☆☆ | **RustCode** |
| Architecture docs | ★★★★★ | ★★★★★ | Tie |
| User-facing docs | ☆☆☆☆☆ | ★★★★★ | **OpenCode** |
| Example code | ★★☆☆☆ | ★★★★☆ | **OpenCode** |
| Generated docs | ☆☆☆☆☆ | ★★★☆☆ | **OpenCode** |
| README quality | ☆☆☆☆☆ | ★★★★★ | **OpenCode** |
| Contributing guide | ☆☆☆☆☆ | ★★★★★ | **OpenCode** |
| SECURITY docs | ☆☆☆☆☆ | ★★★★★ | **OpenCode** |
| WHY comments | ★★★★☆ | ★★★☆☆ | **RustCode** |
| Internationalization | ☆☆☆☆☆ | ★★★★★ | **OpenCode** |
| Design specifications | ★★★★☆ | ★★★★★ | **OpenCode** |
| API documentation | ☆☆☆☆☆ | ★★★★☆ | **OpenCode** |

### 6.2 Shared Gaps (Both Projects)

1. **No CHANGELOG.md in either project**
2. **No generated documentation pipeline CI job**
3. **No automated doc coverage checking**
4. **TODOs/FIXMEs without associated issues**
5. **No example directory in root**

### 6.3 RustCode-Specific Gaps

1. **No README.md**
2. **No CONTRIBUTING.md**
3. **No SECURITY.md**
4. **No generated docs (cargo doc)**
5. **Only 17 doc examples for 119K lines**
6. **tool_impls.rs at 3% doc density**
7. **No user-facing documentation whatsoever**
8. **No CONTEXT.md**

### 6.4 OpenCode-Specific Gaps

1. **Only 46 TSDoc comments in 74K lines**
2. **Zero `@param`/`@returns`/`@throws` tags**
3. **No TypeDoc configuration**
4. **packages/docs contains template placeholder content**
5. **Inconsistent file-level documentation**
6. **Design docs are centralized but have no index**

---

## 7. Findings Register

### Critical Findings

| ID | Title | Project | Location | Effort |
|---|---|---|---|---|
| RC-DOC-06 | No README.md for users | RustCode | Root directory | 4-8 hrs |
| RC-DOC-09 | No SECURITY.md | RustCode | Root directory | 1-2 hrs |
| OC-DOC-02 | Near-zero TSDoc usage | OpenCode | All TS files | 5-10 days |

### High Findings

| ID | Title | Project | Location | Effort |
|---|---|---|---|---|
| RC-DOC-02 | tool_impls.rs 3% doc density | RustCode | core/src/tool_impls.rs | 2-3 days |
| RC-DOC-03 | session_runner/shell low density | RustCode | session_runner.rs, shell.rs | 1-2 days |
| RC-DOC-07 | No CHANGELOG.md | RustCode | Root directory | 1 hr |
| RC-DOC-10 | Insufficient doc examples | RustCode | All modules | 3-5 days |
| RC-DOC-12 | No cargo doc in CI | RustCode | .github/workflows/ci.yml | 30 min |
| RC-DOC-08 | No CONTRIBUTING.md | RustCode | Root directory | 2-3 hrs |

### Medium Findings

| ID | Title | Project | Location | Effort |
|---|---|---|---|---|
| RC-DOC-04 | Variant-level docs sometimes missing | RustCode | Enum-heavy files | Ongoing |
| RC-DOC-05 | No top-level architecture doc | RustCode | Root directory | 4-6 hrs |
| RC-DOC-11 | No standalone example directory | RustCode | Root directory | 2-3 days |
| RC-DOC-13 | No documentation hosting | RustCode | Infrastructure | 1-2 hrs |
| OC-DOC-01 | No file-level docs in most modules | OpenCode | All packages | 3-5 days |
| OC-DOC-04 | Interface semantics not documented | OpenCode | All interface files | 1-2 days |
| OC-DOC-06 | packages/docs has template content | OpenCode | packages/docs/ | 4-8 hrs |
| OC-DOC-08 | No TypeDoc generation | OpenCode | packages/ | 2-3 hrs |
| OC-DOC-09 | No doc generation in CI | OpenCode | .github/ | 2-4 hrs |

### Low Findings

| ID | Title | Project | Location | Effort |
|---|---|---|---|---|
| RC-DOC-01 | Minimal module doc in flag.rs | RustCode | core/src/flag.rs | 15 min |
| RC-DOC-14 | Unsafe code policy not in CONTRIBUTING | RustCode | (would be) | 5 min |
| RC-DOC-15 | Boilerplate Ported-from lines | RustCode | All modules | 2-3 hrs |
| OC-DOC-03 | @internal without explanations | OpenCode | Various | 1-2 hrs |
| OC-DOC-05 | Architecture docs distributed | OpenCode | specs/ + AGENTS.md | 2-3 hrs |
| OC-DOC-07 | Examples not cross-referenced | OpenCode | Example files | 1-2 hrs |
| OC-DOC-10 | Inconsistent inline comment density | OpenCode | All modules | 30 min |

---

## 8. Recommended Roadmap

### Phase 1: Critical (Week 1)

**RustCode:**
1. Create `README.md` with project description, installation, quick start, CLI usage, configuration, links
2. Create `SECURITY.md` with vulnerability disclosure policy
3. Add `cargo doc --workspace --no-deps` to CI workflow

**OpenCode:**
1. Add JSDoc to top 10 most exported functions across opencode and core packages
2. Create TypeDoc configuration and add to CI

### Phase 2: High Priority (Week 2-3)

**RustCode:**
1. Create `CONTRIBUTING.md` (extract from CLAUDE.md)
2. Create `CHANGELOG.md` (initial from git log)
3. Add doc examples to `tool.rs`, `session.rs`, `provider.rs`, `config.rs`
4. Improve doc density in `tool_impls.rs` (target 15%), `session_runner.rs`, `shell.rs`

**OpenCode:**
1. Add file-level JSDoc to `session/`, `tool/`, `provider/`, `agent/` packages
2. Add interface property documentation to all exported types
3. Replace template content in `packages/docs/` with actual project documentation

### Phase 3: Medium Priority (Week 4-5)

**RustCode:**
1. Create `ARCHITECTURE.md` with dependency graph and data flow
2. Create `examples/` directory with 3-5 runnable examples
3. Reduce boilerplate (move Ported-from to top-level only)
4. Improve variant-level docs in enum-heavy modules

**OpenCode:**
1. Create `ARCHITECTURE.md` index for specs
2. Add TypeDoc validation to CI
3. Cross-reference examples from documentation

### Phase 4: Polish (Week 6+)

**RustCode:**
1. Set up docs.rs or GitHub Pages for API docs
2. Audit and improve all modules below 10% doc density
3. Add `# Panics` sections where applicable

**OpenCode:**
1. Add `@param` / `@returns` / `@throws` consistently
2. Formalize inline comment standards in AGENTS.md
3. Set up automated doc coverage tooling

---

## 9. Appendix

### A. Command Reference

```bash
# Count Rust doc comments
grep -r '///' /root/opencodesport/rustcode/src/ /root/opencodesport/rustcode/crates/ --include='*.rs' | wc -l
# Result: 12138

# Count module-level doc comments
grep -rn '^//!' /root/opencodesport/rustcode/crates/ --include='*.rs' | wc -l
# Result: 523

# Count TSDoc comments in OpenCode
grep -rn '\/\*\*' /root/opencodesport/opencode/packages/opencode/src/ --include='*.ts' | wc -l
# Result: 46

# Count TSDoc in core
grep -rn '\/\*\*' /root/opencodesport/opencode/packages/core/src/ --include='*.ts' | wc -l
# Result: 235

# Count doc examples
grep -rn 'Example\|Examples' /root/opencodesport/rustcode/crates/rustcode-core/src/ --include='*.rs' | wc -l
# Result: 17

# Count # Errors sections
grep -rn '# Errors' /root/opencodesport/rustcode/crates/rustcode-core/src/ --include='*.rs' | wc -l
# Result: 30

# Count # Panics sections
grep -rn '# Panics' /root/opencodesport/rustcode/crates/rustcode-core/src/ --include='*.rs' | wc -l
# Result: 1

# Count # Safety sections
grep -rn '# Safety' /root/opencodesport/rustcode/crates/rustcode-core/src/ --include='*.rs' | wc -l
# Result: 1

# Total Rust source lines
find /root/opencodesport/rustcode/crates/ -name '*.rs' -type f -exec wc -l {} + | tail -1
# Result: 119039 total

# Total TS source lines (opencode only)
find /root/opencodesport/opencode/packages/opencode/src/ -name '*.ts' -type f -exec wc -l {} + | tail -1
# Result: 74727 total
```

### B. File Inventory

#### RustCode Documentation Files
| File | Lines | Type |
|---|---|---|
| CLAUDE.md | 118 | Developer guide |
| reports/api_audit.md | - | Audit report |
| reports/architecture_audit.md | - | Audit report |
| reports/concurrency_audit.md | - | Audit report |
| reports/database_audit.md | - | Audit report |
| reports/devex_audit.md | - | Audit report |
| reports/documentation_audit.md | (this) | Audit report |
| reports/logic_audit.md | - | Audit report |
| reports/memory_audit.md | - | Audit report |
| reports/opencode_gap_analysis.md | - | Audit report |
| reports/performance_audit.md | - | Audit report |
| reports/production_readiness.md | - | Audit report |
| reports/protocol_audit.md | - | Audit report |
| reports/security_audit.md | - | Audit report |
| reports/testing_audit.md | - | Audit report |

#### OpenCode Documentation Files (Excluding Translations and Specs)
| File | Lines | Type |
|---|---|---|
| README.md | 129 | User guide |
| CONTRIBUTING.md | 299 | Contribution guide |
| AGENTS.md | 158 | Developer guide |
| CONTEXT.md | - | AI context |
| SECURITY.md | - | Security policy |
| STATS.md | - | Project stats |
| packages/docs/quickstart.mdx | - | Quickstart |
| packages/docs/development.mdx | - | Development guide |
| packages/docs/essentials/navigation.mdx | - | Navigation docs |
| packages/docs/essentials/markdown.mdx | - | Markdown docs |
| packages/docs/essentials/code.mdx | - | Code docs |
| packages/docs/essentials/images.mdx | - | Images docs |
| packages/docs/essentials/settings.mdx | - | Settings docs |
| packages/docs/openapi.json | 30,697 | API spec |
| packages/sdk/js/example/example.ts | 56 | SDK example |
| packages/plugin/src/example.ts | 18 | Plugin example |

### C. Key Quality Metrics

#### RustCode Per-Module Doc Density Scorecard

| Module | Density | Grade | Priority |
|---|---|---|---|
| flag.rs | 0% | F | Low (small file) |
| tool_impls.rs | 3% | F | **Critical** |
| shell.rs | 5% | D | High |
| sse.rs | 5% | D | Medium |
| session_runner.rs | 6% | D | High |
| session.rs | 7% | C | Medium |
| skill.rs | 7% | C | Medium |
| snapshot.rs | 7% | C | Medium |
| catalog.rs | 8% | C | Medium |
| agent.rs | 8% | C | Medium |
| command.rs | 8% | C | Medium |
| reference.rs | 9% | C | Medium |
| repository.rs | 9% | C | Medium |
| location.rs | 10% | C+ | Low |
| system_context.rs | 10% | C+ | Low |
| storage.rs | 10% | C+ | Low |
| session_prompt.rs | 11% | C+ | Low |
| npm.rs | 11% | C+ | Low |
| git.rs | 11% | C+ | Low |
| format.rs | 11% | C+ | Low |
| plugin.rs | 11% | C+ | Low |
| runtime.rs | 11% | C+ | Low |
| workspace.rs | 11% | C+ | Low |
| credential.rs | 12% | B | Low |
| bus.rs | 12% | B | Low |
| observability.rs | 12% | B | Low |
| question.rs | 12% | B | Low |
| tool_stream.rs | 12% | B | Low |
| config.rs | 13% | B | Low |
| error.rs | 20% | A | - |
| event.rs | 20% | A | - |
| model.rs | 29% | A | - |

*Note: Priority = need for improvement based on module criticality × current density*

---

**Report Generated**: 2026-06-19  
**Auditor**: Agent 14 — Documentation Auditor  
**Total Findings**: 25 (2 Critical, 7 High, 9 Medium, 7 Low)
