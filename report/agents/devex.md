# DevEx Analysis: BlazeCode vs BlazeCode

**Agent 13 — Developer Experience Report**
**Date**: 2026-06-21

---

## 1. Build System

### 1.1 Build Speed & Incremental Compilation

- **Location**: `blazecode/Cargo.toml:1-96`, `blazecode/CLAUDE.md:8`
- **BlazeCode**: Bun native runtime — near-instant startup. `bun dev` starts dev server in <1s. Turborepo caches task outputs across workspaces. TypeScript-only, no compilation step for development.
- **BlazeCode**: Cargo workspace with 6 crates (root, core, server, tui, lsp, mcp). Full LLVM compilation required for every change. Workspace compilation means a change in core recompiles all dependents. CLAUDE.md explicitly forbids local `cargo build` — CI-only model means **zero local feedback loops**.
- **Gap**: BlazeCode has no incremental dev workflow. CLAUDE.md Rule #1 bans all local compilation. Every code change requires a full CI round-trip.
- **Consequence**: Estimated 15-30min per iteration for full workspace rebuild. No `cargo check` or `cargo test` locally. Developer cannot validate code before pushing.
- **Recommendation**: Remove CLAUDE.md Rule #1 prohibition on local `cargo check` and `cargo test`. Add `cargo watch` or `cargo lambda` for fast dev loops. Use `cargo check --workspace` (skips LLVM codegen) for sub-5s feedback. Add `CARGO_INCREMENTAL=1` to CI.
- **Severity**: **Critical**

### 1.2 Caching & Parallelism

- **Location**: `blazecode/.github/workflows/ci.yml:36-37`
- **BlazeCode**: Turbo cache (remote caching on Vercel/Blacksmith), `node_modules` caching via `actions/cache`. Workspace-level task orchestration.
- **BlazeCode**: `Swatinem/rust-cache@v2` in CI — caches `target/` directory. No sccache (distributed compiler cache). Single `cargo build` command per job.
- **Gap**: No sccache integration. No parallel job splitting for large crate graph. No `CARGO_BUILD_JOBS` tuning.
- **Consequence**: CI rebuilds are I/O-bound on cache restore. First-time CI takes 20-40min. Dependency graph has 60+ transitive crates.
- **Recommendation**: Add `sccache` to CI pipeline. Use `cargo test --workspace --jobs 4` for parallelism. Add `CARGO_NET_RETRY=3` for reliability. Consider `mold` linker for Linux.
- **Severity**: **High**

---

## 2. Tooling

### 2.1 Formatting & Linting

- **Location**: `blazecode/CLAUDE.md:35-36`, `blazecode/.github/workflows/ci.yml:26-37`
- **BlazeCode**: `oxlint` (Rust-based, fast) with `oxlintrc.json` config. `prettier` for formatting. TypeScript compiler (`tsc`) for type checking. `.editorconfig` for cross-editor consistency.
- **BlazeCode**: `cargo fmt` + `cargo clippy` (both CI-only). CLI restrictions in `CLAUDE.md:8` explicitly ban `cargo fmt` locally. `deny.toml` for license/advisory checks.
- **Gap**: No pre-commit hook for fmt/clippy. No `.editorconfig`. No local lint enforcement. Clippy is relaxed (`#![allow(dead_code, unused_imports, unused_variables)]`) in scaffold phase.
- **Consequence**: Developers push unformatted code and rely on CI to catch it. Dead code accumulates. Lint rules intentionally suppressed.
- **Recommendation**: Add `rustfmt` pre-commit hook (via `cargo-husky` or `lefthook`). Add `.editorconfig`. Remove `#![allow(dead_code, unused_imports, unused_variables)]` from production modules. Add `clippy::pedantic` incrementally.
- **Severity**: **High**

### 2.2 rust-analyzer Integration

- **Location**: `blazecode/crates/blazecode-core/src/lib.rs:1-95`, `blazecode/.vscode/`
- **BlazeCode**: `.vscode/launch.example.json`, `.vscode/settings.example.json`, `.zed/` config directory. First-class IDE integration documentation.
- **BlazeCode**: No `.vscode/` or `.zed/` config. No `rust-analyzer` settings (e.g., `rust-analyzer.cargo.features`). No launch configurations for debugging.
- **Gap**: No IDE workspace settings provided. Developers must manually configure rust-analyzer for the workspace.
- **Consequence**: `rust-analyzer` won't understand the workspace layout out-of-box. No debug launch configurations.
- **Recommendation**: Add `.vscode/settings.json` with `rust-analyzer.cargo.features = "all"` and `rust-analyzer.check.command = "clippy"`. Add `.vscode/launch.json` for debugging. Document IDE setup in `CONTRIBUTING.md`.
- **Severity**: **High**

### 2.3 Pre-commit Hooks

- **Location**: `blazecode/.husky/pre-push`, `blazecode/`
- **BlazeCode**: Husky pre-push hook that checks Bun version match and runs `bun typecheck`. CI guardrails prevent root-level test execution.
- **BlazeCode**: No pre-commit or pre-push hooks. `CLAUDE.md:8` explicitly prohibits local tooling.
- **Gap**: Complete absence of local git hooks. No guard against pushing broken code.
- **Consequence**: Every push triggers a full CI run only to discover fmt/clippy failures. Wastes CI minutes and developer time.
- **Recommendation**: Add pre-push hook that runs `cargo fmt --check` and `cargo clippy --all-targets -- -D warnings`. Use `cargo-husky` or `lefthook` for cross-platform hook management.
- **Severity**: **High**

---

## 3. CI/CD

### 3.1 CI Speed & Parallelism

- **Location**: `blazecode/.github/workflows/ci.yml:1-68`, `blazecode/.github/workflows/test.yml:1-145`
- **BlazeCode**: 3 parallel jobs (unit linux, unit windows, e2e). Blacksmith 4-vCPU runners. Turbo cache for task outputs. E2E tests with Playwright. Separate typecheck workflow.
- **BlazeCode**: 4 serial jobs (fmt, clippy, test×3 OS matrix, deny). No parallelism within jobs. Single `cargo build && cargo test` per OS.
- **Gap**: CI jobs run sequentially at workflow level (except test matrix). No separate typecheck job. No e2e tests.
- **Consequence**: Full CI run takes 30-60min. fmt/clippy failures waste matrix test slots.
- **Recommendation**: Reorder jobs to fail-fast: run fmt+clippy first, then matrix test+deny in parallel with those results. Use `cargo nextest` for faster test execution. Add `cargo check` as a separate fast job.
- **Severity**: **Medium**

### 3.2 Test Execution

- **Location**: `blazecode/.github/workflows/ci.yml:49-62`, `blazecode/.github/workflows/test.yml:66-75`
- **BlazeCode**: `bun turbo test` with `--output-logs=errors-only`. Separate unit/e2e jobs. 20min timeout for unit tests, 30min for e2e with Playwright.
- **BlazeCode**: `cargo test --all --verbose` with no parallelism flags. Matrix runs: ubuntu, macos, windows. Windows uses `skip_unix_tests` cfg flag.
- **Gap**: No `cargo nextest` for parallel test execution. No test result reporting (JUnit/xml). No flaky test detection.
- **Consequence**: Test suite grows linearly with module count. No test analytics or history. Flaky tests go undetected.
- **Recommendation**: Switch to `cargo nextest`. Publish test results with `dorny/test-reporter`. Add `RUST_TEST_THREADS=4` for controlled parallelism. Add test categorization (unit/integration/e2e).
- **Severity**: **Medium**

### 3.3 Artifact Caching

- **Location**: `blazecode/.github/workflows/ci.yml:36`
- **BlazeCode**: Turbo remote caching. Bun dependency caching in `setup-bun` composite action with keyed restore/save.
- **BlazeCode**: `Swatinem/rust-cache@v2` (single line). No sccache. Cache key uses default prefix only.
- **Gap**: Basic caching only. No distributed cache for matrix builds. Cache miss on any change triggers full rebuild per OS.
- **Consequence**: Windows and macOS runners cannot share Linux build cache. Each OS compiles all dependencies independently.
- **Recommendation**: Add `sccache` with S3/GCS backend for cross-OS cache sharing. Configure custom cache keys per OS. Add `CARGO_INCREMENTAL=1`.
- **Severity**: **Medium**

### 3.4 Release Automation

- **Location**: `blazecode/.github/workflows/release.yml:1-276`, `blazecode/scripts/version.sh:1-212`, `blazecode/`
- **BlazeCode**: SST deploy workflow (`sst deploy`). Infrastructure-as-code deployments to AWS. Multiple deploy environments (dev/production). Desktop app packaging via Electron. `SDK` generation pipeline.
- **BlazeCode**: Comprehensive release workflow: 5 targets (x86_64/aarch64 Linux, x86_64/aarch64 macOS, x86_64 Windows), tarball/zip packaging, SHA256 checksums, GPG signing, auto-generated release notes from git log, `scripts/version.sh` for version management (bump, tag, changelog, full release).
- **Gap**: BlazeCode release is better automated than BlazeCode's multi-channel deployment. However, BlazeCode has no `CHANGELOG.md` file yet (script generates it but file is gitignored/never committed). No `cargo publish` to crates.io. No npm/pkg equivalent.
- **Consequence**: Versioning script works but `CHANGELOG.md` is ephemeral. No registries consume releases — only GitHub Releases.
- **Recommendation**: Commit `CHANGELOG.md` after each release. Add `cargo publish` for crates.io distribution. Consider Homebrew tap for macOS. Automate `scripts/version.sh release` as a CI workflow step.
- **Severity**: **Low**

### 3.5 Security Auditing

- **Location**: `blazecode/.github/workflows/audit.yml:1-98`, `blazecode/deny.toml:1-29`
- **BlazeCode**: No equivalent security audit workflow found. `.gitleaksignore` present for secret scanning.
- **BlazeCode**: Weekly scheduled `cargo-audit` with auto-created GitHub issues on vulnerability detection. `cargo-deny` in CI with license allowlist (12 licenses), ban config, and advisory ignore list.
- **Gap**: BlazeCode has stronger automated security auditing than BlazeCode. However, `RUSTSEC-2024-0436` is ignored without documented reason. `wildcards = "allow"` in bans may allow duplicate dependencies.
- **Consequence**: Better security posture than upstream in this dimension, but suppressed advisory is undocumented risk.
- **Recommendation**: Document reason for `RUSTSEC-2024-0436` ignore with a comment. Set `wildcards = "deny"` gradually. Add Dependabot or Renovate for dependency updates.
- **Severity**: **Low**

---

## 4. Documentation

### 4.1 README & Getting Started

- **Location**: `blazecode/README.md` (missing), `blazecode/README.md` + 26 translations
- **BlazeCode**: Comprehensive `README.md` with 26 locale translations (`README.ar.md` through `README.zht.md`). `screenshot-uk.png`. `SECURITY.md`. `LICENSE`.
- **BlazeCode**: **No `README.md`**. `CLAUDE.md` serves as primary documentation but is AI-context-oriented, not user-facing. `docs/` contains only `plugin-system.md`.
- **Gap**: No user-facing README at all. New users have zero entry point.
- **Consequence**: Users cannot learn what BlazeCode is, how to build it, or how to contribute without reading `CLAUDE.md` (which is intended for AI agents).
- **Recommendation**: Write a proper `README.md` with: badge row, description, quick-start, build instructions, configuration, and link to plugin docs. Consider at least English-only; translation is a future concern.
- **Severity**: **Critical**

### 4.2 Technical Documentation

- **Location**: `blazecode/docs/plugin-system.md:1-293`, `blazecode/specs/`
- **BlazeCode**: 14 specification documents in `specs/` covering V2 architecture (session, config, provider, tools, instructions, schema), TUI extraction plan, storage/DB design. `CONTEXT.md` defines Domain Language for Session Runtime. `AGENTS.md` documents style guide and conventions.
- **BlazeCode**: Single documentation file (`docs/plugin-system.md`). `CLAUDE.md` doubles as architecture doc + developer workflow. No design specs, no migration guides.
- **Gap**: BlazeCode has 1 doc file vs BlazeCode's 14+ specs. No architecture decision records (ADRs). No migration guide.
- **Consequence**: New contributors have no architectural context beyond CLAUDE.md. Design decisions are tribal knowledge. V2 session semantics documented in BlazeCode's CONTEXT.md are not ported.
- **Recommendation**: Port key specs from BlazeCode: session architecture, provider model, config schema. Add ADR process for design decisions. Document the porting mapping between TS and Rust modules.
- **Severity**: **High**

---

## 5. Onboarding

### 5.1 Time to First Build

- **Location**: `blazecode/Cargo.toml:12-64`
- **BlazeCode**: `bun install` (5-15s), `bun dev` (instant). No compilation. Bun 1.3+ required. Nix flake available for reproducible environments (`flake.nix`).
- **BlazeCode**: `cargo build` requires LLVM toolchain, Rust nightly or stable. First build: 87 workspace dependencies, estimated 15-30min. CLAUDE.md Rule #1 prohibits local build entirely.
- **Gap**: No prebuilt binaries for quick start. No `rust-toolchain.toml` for automatic version pinning. Install script is for users, not developers.
- **Consequence**: Developer cannot build locally. Only CI can compile. Zero local iteration capability.
- **Recommendation**: Add `rust-toolchain.toml` to pin toolchain version. Provide prebuilt binaries via CI artifacts for quick download. Allow `cargo check` locally (fast, skips codegen). Add Dockerfile for reproducible build environment.
- **Severity**: **Critical**

### 5.2 Dependencies

- **Location**: `blazecode/deny.toml:6-21`
- **BlazeCode**: Bun manages JavaScript dependencies. `bun install` single command. Nix flake for system dependencies (bun, nodejs, openssl, pkg-config, git). `bun.lock` for reproducible installs.
- **BlazeCode**: Cargo fetches 87+ transitive dependencies across 6 workspace crates. License allowlist of 12 licenses. `deny.toml` bans check. System dependencies include OpenSSL and pkg-config.
- **Gap**: Rust compilation requires more system dependencies than Bun/Node. No Nix flake for BlazeCode. No cross-platform CI for musl (Alpine).
- **Consequence**: Developers need Rust toolchain + OpenSSL + pkg-config. No reproducible environment declaration.
- **Recommendation**: Add `rust-toolchain.toml`. Add Nix flake (or devenv.sh) for reproducible Rust dev environment. Document system dependencies in CONTRIBUTING.md.
- **Severity**: **Medium**

### 5.3 Platform Support

- **Location**: `blazecode/install:93-136`
- **BlazeCode**: Linux (x86_64, aarch64), macOS (x86_64, aarch64 via Rosetta detection), Windows (x86_64). Nix flake supports all 4 platform combinations.
- **BlazeCode**: Install script supports Linux (gnu+musl), macOS, Windows. CI tests on ubuntu, macos, windows. Release workflow builds 5 targets.
- **Gap**: No official Nix flake for BlazeCode dev environment. No `aarch64-pc-windows-msvc` target. No musl-specific CI testing despite detection in install script.
- **Consequence**: Platform parity with BlazeCode for users, but no reproducible dev environment via Nix.
- **Recommendation**: Add Nix flake for development. Test musl builds in CI. Consider `aarch64-pc-windows-msvc` for ARM Windows.
- **Severity**: **Low**

---

## 6. IDE Support

- **Location**: `blazecode/` (no IDE config), `blazecode/.vscode/`
- **BlazeCode**: VSCode settings + launch configs (`.vscode/settings.example.json`, `.vscode/launch.example.json`). Zed editor config (`.zed/` directory). Comprehensive debugger setup guide in `CONTRIBUTING.md` (Bun `--inspect`, `--inspect-wait`, `--inspect-brk`).
- **BlazeCode**: No IDE configuration files. No `.vscode/`. No `.zed/`. No `.helix/`. No `rust-analyzer` config for workspace features.
- **Gap**: Zero IDE support configuration.
- **Consequence**: Every developer must configure rust-analyzer from scratch. No debug launch configuration. No shared editor settings.
- **Recommendation**: Add `.vscode/settings.json` with rust-analyzer configuration for workspace. Add `.vscode/launch.json` with debug targets. Document VS Code and JetBrains Rust setup in CONTRIBUTING.md. Add editorconfig for cross-editor consistency.
- **Severity**: **High**

---

## 7. Debugging & Observability

### 7.1 Logging & Tracing

- **Location**: `blazecode/crates/blazecode-core/src/observability.rs:1-1652`, `blazecode/Cargo.toml:18-20`
- **BlazeCode**: Effect-based structured logging with OTLP export. File logging to `$XDG_DATA_HOME/blazecode/log/`. Stderr logging via `BLAZECODE_PRINT_LOGS`. Honeycomb observability integration (`HONEYCOMB_API_KEY` in deploy.yml:42).
- **BlazeCode**: Comprehensive observability module (1652 lines) with: structured key=value logging, JSON output, OTLP export, tracing-subscriber layers, token usage tracking, performance metrics, span helpers. Dependencies: `tracing`, `tracing-subscriber` (env-filter, json, registry), `tracing-appender`.
- **Gap**: BlazeCode observability is well-implemented and arguably more detailed than BlazeCode at module level. However, no Sentry/Honeycomb integration in CI/deploy. No correlation with deploy workflow.
- **Consequence**: Good internal tracing but no production observability pipeline connected.
- **Recommendation**: Wire OTLP exporter to Honeycomb or similar in production deployment. Add Sentry for crash reporting (matching BlazeCode's `SENTRY_AUTH_TOKEN` in deploy).
- **Severity**: **Medium**

### 7.2 Debug Configuration

- **Location**: `blazecode/CLAUDE.md:8`
- **BlazeCode**: Full debug guide in CONTRIBUTING.md: VSCode launch configurations, `--inspect` flags, worker thread debugging, `spawn` mode, `BUN_OPTIONS` env var.
- **BlazeCode**: No debug documentation. No VSCode launch configs. CLI explicitly prohibits `cargo build` locally.
- **Gap**: Completely absent debug workflow.
- **Consequence**: Developer cannot debug locally. Bug diagnosis requires adding `eprintln!`/`tracing::debug!` and deploying to CI.
- **Recommendation**: Provide VSCode launch configurations for debugging with `rust-gdb`/`lldb`. Document debug logging with `RUST_LOG`/`BLAZECODE_LOG_LEVEL`. Add `--verbose` flag to CLI for debug output.
- **Severity**: **High**

---

## 8. Code Generation

- **Location**: `blazecode/` (no codegen), `blazecode/packages/sdk/`
- **BlazeCode**: OpenAPI → JavaScript SDK codegen (`./packages/sdk/js/script/build.ts`). Hono OpenAPI middleware generates OpenAPI spec from server routes. SDK is regenerated when server API changes. Codegen is a documented step in CONTRIBUTING.md.
- **BlazeCode**: No code generation. All provider implementations, tool definitions, and serialization types are manually written Rust code.
- **Gap**: No OpenAPI spec generation. No client SDK generation. No automated type synchronization between server and client.
- **Consequence**: Every API change requires manual updates to all consumers. No type-safe client library. Higher maintenance burden for protocol changes.
- **Recommendation**: Add `utoipa` for OpenAPI spec generation from axum routes. Generate Rust client types from OpenAPI spec using `progenitor` or `oapi-codegen`. Consider sharing model types between server and SDK crates.
- **Severity**: **Medium**

---

## 9. Release Process

- **Location**: `blazecode/scripts/version.sh:1-212`, `blazecode/.github/workflows/release.yml:1-276`, `blazecode/`
- **BlazeCode**: SST-based deployment to AWS (dev/production). Electron desktop app packaging. Separate `deploy.yml` workflow. No version management script found.
- **BlazeCode**: Comprehensive release process: `scripts/version.sh` supports bump (major/minor/patch), set, tag, changelog, and full release. `release.yml` builds 5 cross-platform targets, signs with GPG, generates SHA256 checksums, creates GitHub Release with auto-generated notes from git log.
- **Gap**: BlazeCode release tooling is actually **more mature** than BlazeCode's in this specific area. However, no `cargo publish` to crates.io. No Homebrew tap. No `npm`-equivalent package registry integration. `CHANGELOG.md` is generated but never committed.
- **Consequence**: Great binary distribution but no ecosystem package presence.
- **Recommendation**: Add `cargo publish` step to release workflow. Create Homebrew formula for macOS. Publish `CHANGELOG.md` alongside releases. Add cargo-dist for modern Rust release automation.
- **Severity**: **Low**

---

## 10. Plugin Development

### 10.1 Plugin SDK & Documentation

- **Location**: `blazecode/docs/plugin-system.md:1-293`, `blazecode/crates/blazecode-core/src/plugin.rs`, `blazecode/packages/plugin/`
- **BlazeCode**: Dedicated `@blazecode-ai/plugin` npm package. TUI plugin presentation slots in `packages/tui`. Plugin system integrated with server API. Plugin discovery in `.blazecode/skills/`.
- **BlazeCode**: Well-documented plugin system with single `docs/plugin-system.md` (293 lines). Three plugin tiers: config-based, closure plugins, trait plugins. Provider plugin architecture with 3 hooks (transform_catalog, discover_models, load_auth). 14 built-in OpenAI-compatible provider profiles. `ProviderPluginRegistry` for plugin lifecycle.
- **Gap**: BlazeCode's plugin documentation is actually excellent — better structured than BlazeCode's scattered plugin docs. However, only covers LLM provider plugins, not tool plugins, UI plugins, or MCP plugins. No plugin example repository. No plugin packaging format defined.
- **Consequence**: Good foundation but limited scope. Plugin developers can only extend providers, not add tools or TUI components.
- **Recommendation**: Expand plugin system to cover tool plugins and context source plugins following BlazeCode's architecture. Add plugin packaging format (`.wasm` plugins?). Add example plugin repository. Document plugin development workflow in CONTRIBUTING.md.
- **Severity**: **Medium**

### 10.2 Skill System

- **Location**: `blazecode/crates/blazecode-core/src/skill.rs`
- **BlazeCode**: Skill system documented in BlazeCode specifications. Skills loaded from `.blazecode/skills/*.md`. Skill content exposed to agents via permission-checked tool.
- **BlazeCode**: Skill module exists as scaffold. `discover()` function from `.blazecode/skills/*.md`. Integrated with instruction context and system context registry.
- **Gap**: Skill module exists but is scaffold-only. No usage examples. No tests.
- **Consequence**: Skill discovery and integration is unimplemented despite module presence.
- **Recommendation**: Complete skill module implementation. Add integration tests for skill discovery. Document skill authoring workflow.
- **Severity**: **Medium**

---

## 11. Testing Experience

### 11.1 Test Framework & Running

- **Location**: `blazecode/.github/workflows/ci.yml:51-61`, `blazecode/.github/workflows/test.yml:66-75`
- **BlazeCode**: Bun test runner (Jest-compatible). `bun turbo test` from package dirs. Tests cannot run from repo root (guard: `do-not-run-tests-from-root`). E2E tests with Playwright in `packages/app`.
- **BlazeCode**: Standard `#[cfg(test)]` unit tests. `cargo test --all` in CI. Rust test framework. Windows tests use `skip_unix_tests` cfg flag.
- **Gap**: No `cargo nextest` (faster, parallel). No doc tests enforced. No integration test crate. No performance/benchmark tests (`#[bench]` is nightly-only). No fuzz testing. No property-based testing (proptest/fuzzcheck).
- **Consequence**: Basic unit test coverage only. No regression test suite for LLM provider behavior, tool execution, or session management.
- **Recommendation**: Add `cargo nextest` for CI. Create `tests/` integration test crate. Add property-based testing with `proptest` for core logic. Add regression tests for provider protocols. Consider `insta` for snapshot testing.
- **Severity**: **High**

### 11.2 Test Fixtures & Data

- **Location**: `blazecode/` (no test fixtures directory visible)
- **BlazeCode**: Playwright browser testing with Chromium. E2E test results and reports uploaded as CI artifacts. Test fixtures in package directories.
- **BlazeCode**: No dedicated test fixtures directory. No test data files for provider responses, config files, or session data.
- **Gap**: No shared test utilities or mock data.
- **Consequence**: Tests must recreate data inline. No easy way to test against realistic provider responses.
- **Recommendation**: Create `test-fixtures/` with sample config files, provider responses (JSON), and session snapshots. Use `serde_json::json!` macros for inline test data. Add fixture loading utilities.
- **Severity**: **Medium**

---

## 12. Error Messages

### 12.1 Compiler Errors

- **Location**: `blazecode/crates/blazecode-core/src/error.rs`, `blazecode/CLAUDE.md:10`
- **BlazeCode**: TypeScript runtime errors with stack traces. Effect library provides structured error types. `thisError` pattern for typed errors.
- **BlazeCode**: `thiserror` derive macro for `Error` enum (14 variants). `anyhow` for convenience error handling. `#![forbid(unsafe_code)]` for safety. No `.unwrap()` in library code — enforced via CLAUDE.md Rule #3.
- **Gap**: Rust compiler errors are inherently superior to TypeScript runtime errors (compile-time vs runtime). BlazeCode enforces good error hygiene. However, no user-facing error display formatting.
- **Consequence**: Good internal error handling but no user-friendly error messages for CLI output.
- **Recommendation**: Implement `Display` traits with user-facing messages for all error variants. Add error context chain formatting for CLI output (`color-eyre` or `eyre`). Add suggestion messages for common errors.
- **Severity**: **Medium**

### 12.2 CLI Error Output

- **Location**: `blazecode/src/main.rs` (not read)
- **BlazeCode**: Rich terminal output with color, spinners (indicatif), formatted error display.
- **BlazeCode**: Uses `dialoguer` and `indicatif` for terminal interaction (Cargo.toml:55-56). Error formatting not yet analyzed.
- **Gap**: BlazeCode has the right dependencies for good CLI UX but actual error formatting quality is unknown.
- **Consequence**: Depends on implementation quality.
- **Recommendation**: Audit CLI error output paths. Use `color-eyre` for panic/error formatting with span traces. Ensure consistent error style across all commands.
- **Severity**: **Low**

---

## 13. Hot Reload & Dev Workflow

### 13.1 Watch Mode

- **Location**: `blazecode/Cargo.toml:57` (notify dependency), `blazecode/turbo.json:1-25`
- **BlazeCode**: Turborepo watch mode (`turbo dev`). `bun --watch` for file change detection. Near-instant restart (Bun is a JS runtime, no compilation). Desktop app hot reload via Electron + Vite.
- **BlazeCode**: `notify` crate listed in workspace dependencies (v6, no features). No `cargo-watch` configured. No `watchexec` in dev scripts. CLAUDE.md Rule #1 prohibits local builds entirely.
- **Gap**: No watch/reload mechanism whatsoever. Not even `cargo watch`.
- **Consequence**: Zero hot-reload capability. Every code change requires CI run to validate.
- **Recommendation**: Add `cargo watch` to dev scripts for auto-`cargo check` on file changes. Add `watchexec` for test watch mode. Remove or relax CLAUDE.md Rule #1 to allow local `cargo check`. Consider `cargo-msrv` for caching.
- **Severity**: **Critical**

### 13.2 Development Server

- **Location**: `blazecode/crates/blazecode-server/` (stub), `blazecode/CONTRIBUTING.md:42-48`
- **BlazeCode**: `bun dev` starts development server with worker threads. Supports spawning against custom directories (`bun dev <directory>`). API server with `bun dev serve`. Web app with `bun dev web`.
- **BlazeCode**: `blazecode-server` is a scaffold crate. No documented way to run development server.
- **Gap**: Server implementation is stub-only. No dev server workflow.
- **Consequence**: Cannot test server functionality locally. MCP, LSP, and HTTP features are inaccessible.
- **Recommendation**: Implement server crate with axum. Add `cargo run -- serve` subcommand. Document `BLAZECODE_PORT`, `BLAZECODE_HOST` environment variables.
- **Severity**: **High**

---

## 14. Contribution Guide

### 14.1 CONTRIBUTING.md

- **Location**: `blazecode/CONTRIBUTING.md` (missing), `blazecode/CONTRIBUTING.md:1-299`
- **BlazeCode**: Comprehensive CONTRIBUTING.md (299 lines) covering: PR expectations (issue-first policy, AI-generated content policy, UI change screenshots), style preferences, debug setup, feature request process, trust/vouch system, issue templates. `AGENTS.md` extends with coding conventions.
- **BlazeCode**: No `CONTRIBUTING.md`. All developer guidance is in `CLAUDE.md` (which targets AI agents, not human contributors).
- **Gap**: Complete absence of human-facing contribution documentation.
- **Consequence**: Humans have no documented process for contributing. The only guide tells AI agents not to run `cargo build` locally.
- **Recommendation**: Write `CONTRIBUTING.md` with: local setup instructions, PR workflow, coding standards, test expectations, CI pipeline explanation, and code of conduct. Separate AI-agent instructions (CLAUDE.md) from human contributor docs.
- **Severity**: **Critical**

### 14.2 Issue Templates

- **Location**: `blazecode/.github/` (no templates visible), `blazecode/CONTRIBUTING.md:282-299`
- **BlazeCode**: Mandatory issue templates (bug report, feature request, question). Automated template compliance check. 2-hour editing window before auto-close. Vouch/denounce system for contributor trust management.
- **BlazeCode**: No issue templates. No PR template. No code of conduct.
- **Gap**: Zero contribution infrastructure.
- **Consequence**: Issues and PRs will lack structure, increasing maintainer burden.
- **Recommendation**: Add GitHub issue templates (bug report, feature request). Add PR template with checklist. Add CODE_OF_CONDUCT.md (e.g., Contributor Covenant).
- **Severity**: **High**

---

## 15. Migration Tooling

### 15.1 Database Migration

- **Location**: `blazecode/Cargo.toml:21` (sqlx), `blazecode/specs/v2/schema-changelog.md`
- **BlazeCode**: 35 SQLite migrations via Drizzle ORM. Database schema defined in `packages/core/src/database/`. 18 SQLite tables. V2 schema migration documented in specs.
- **BlazeCode**: `sqlx` dependency with SQLite feature. No migration infrastructure visible. `storage.rs` uses JSON storage with SQLite placeholder.
- **Gap**: No migration system. No schema definitions. No type-safe queries via `sqlx::query!` macros.
- **Consequence**: Database schema cannot evolve safely. No compile-time query verification.
- **Recommendation**: Add `sqlx::migrate!` for SQLite migrations. Port BlazeCode's Drizzle schema to SQL. Add `cargo sqlx prepare` for compile-time query checking. Use `sqlx::query_as!` for type-safe row mapping.
- **Severity**: **High**

### 15.2 BlazeCode V1→V2 Migration

- **Location**: `blazecode/` (no migration context), `blazecode/specs/v2/`
- **BlazeCode**: V2 specifications cover session model, provider model, config, tools, instructions, catalog/plugin lifecycle, schema changelog. Migration from V1 to V2 session architecture is a documented concern.
- **BlazeCode**: BlazeCode is a ground-up port. No V1 codebase exists. Some V2 session architecture is ported (session_epoch, session_execution, system_context modules).
- **Gap**: No migration needed (greenfield). But also no guarantee of source compatibility with BlazeCode V2 API.
- **Consequence**: API divergence risk if BlazeCode V2 spec changes. BlazeCode may become incompatible with upstream BlazeCode.
- **Recommendation**: Pin to specific BlazeCode commit (`5d0f86606ac30690f79f0a6a9f41a1f49fe95d0b` per CLAUDE.md). Establish sync cadence for upstream changes. Document API compatibility expectations.
- **Severity**: **Medium**

---

## Summary of Findings

| # | Area | Severity | Key Gap |
|---|------|----------|---------|
| 1 | **Build System** | **Critical** | No local compilation allowed; 15-30min CI loop |
| 2 | **Onboarding** | **Critical** | No README, no CONTRIBUTING.md, cannot build locally |
| 3 | **Hot Reload** | **Critical** | Zero watch/reload capability |
| 4 | **Contribution Guide** | **Critical** | No human-facing contributing docs |
| 5 | **Tooling** | **High** | No pre-commit hooks, no IDE config, no editorconfig |
| 6 | **IDE Support** | **High** | No VSCode/rust-analyzer config, no debug launch configs |
| 7 | **Debugging** | **High** | Complete absence of debug workflow |
| 8 | **Testing** | **High** | No nextest, no integration tests, no fixtures |
| 9 | **Migration** | **High** | No database migration infrastructure |
| 10 | **Documentation** | **High** | Only 1 doc file vs BlazeCode's 14+ specs |
| 11 | **Code Generation** | **Medium** | No OpenAPI/SDK codegen |
| 12 | **Plugin Dev** | **Medium** | Provider-only; no tool/UI/MCP plugin support |
| 13 | **Observability** | **Medium** | Good tracing but no production pipeline |
| 14 | **CI/CD** | **Medium** | No test parallelism, no nextest |
| 15 | **Release Process** | **Low** | Actually better than BlazeCode in this area |

## Quick Wins (High Impact, Low Effort)

1. **Allow `cargo check` locally** — Modify CLAUDE.md Rule #1 to permit `cargo check --workspace`
2. **Write README.md** — 50-line single-file getting-started guide
3. **Write CONTRIBUTING.md** — Port key sections from BlazeCode's version
4. **Add `rust-toolchain.toml`** — Pin toolchain for reproducibility
5. **Add `.editorconfig`** — Cross-editor consistency
6. **Add `.vscode/settings.json`** — rust-analyzer configuration
7. **Add issue templates** — GitHub bug report + feature request templates
8. **Add `cargo watch` dev script** — Auto-check on file changes

## Structural Recommendations (Multi-Sprint)

1. **Remove CLAUDE.md Rule #1** — Replace with CI-only deployment restriction but allow local dev tooling
2. **Add CI pre-check job** — `cargo check` + `cargo fmt --check` as fast first CI step
3. **Port BlazeCode specs** — Session architecture, provider model, config schema docs
4. **Add database migrations** — `sqlx::migrate!` for SQLite schema evolution
5. **Add sccache** — Distributed compilation caching for CI matrix
6. **Switch to cargo nextest** — Parallel test execution
7. **Implement server crate** — Complete axum HTTP/SSE server implementation
8. **Add OpenAPI codegen** — `utoipa` + generate Rust client types
9. **Expand plugin system** — Tool plugins, UI plugins, MCP plugins
10. **Add Nix flake** — Reproducible development environment
