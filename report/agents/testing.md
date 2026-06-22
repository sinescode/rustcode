# Testing Agent Report — BlazeCode vs BlazeCode

**Date**: 2026-06-21  
**BlazeCode**: 2,386 test functions, 112 `#[cfg(test)]` modules, 3,023 total test attributes  
**BlazeCode**: 532 test files, 27 CI workflows, Playwright e2e, http-recorder, 140 core package tests

---

## 1. Unit Tests

- **Location**: BlazeCode — inline `mod tests` in every `.rs` file across `crates/blazecode-core/src/`.  
- **BlazeCode**: 140 test files in `packages/core/test/`, organized by module (e.g., `agent.test.ts`, `config/`, `provider/`, `skill/`). Uses `bun test` with `describe`/`it`/`expect`.  
- **BlazeCode**: 2,386 test functions across 112 modules. Top modules by test function count: `config.rs` (82), `catalog.rs` (80), `tool_impls.rs` (75), `location.rs` (72), `repository.rs` (70), `ripgrep.rs` (67), `session.rs` (66), `permission.rs` (63), `pty.rs` (59), `plugin.rs` (56), `provider.rs` (55), `integration.rs` (30).  
- **Gap**: BlazeCode separates tests into a `test/` directory with fixtures; BlazeCode tests are inline only. BlazeCode has no test helpers, no test utility modules, and no shared test infrastructure.  
- **Consequence**: Test setup code is duplicated across modules. No standardized way to create test databases, mock providers, or configure temp directories.  
- **Recommendation**: Extract shared test utilities into a `test_helpers.rs` or `test/` module. Create a `TestContext` builder for database-backed tests.  
- **Severity**: Medium

---

## 2. Integration Tests

- **Location**: BlazeCode — `crates/blazecode-core/src/integration.rs:885` has 30 tests. `database.rs:3304` has 20 tests covering SQL migrations and queries.  
- **BlazeCode**: `integration.test.ts` (Effect-based test layer composition), `database-migration.test.ts` (18 SQLite tables, 35 migrations), `session-runner-recorded.test.ts` (recorded provider responses).  
- **Gap**: BlazeCode's integration tests only cover business logic composition — no database migration tests, no provider integration tests, no network-recorded session tests. BlazeCode's `http-recorder` package enables deterministic replay of LLM provider API calls; BlazeCode has no equivalent.  
- **Consequence**: BlazeCode integration tests cannot safely exercise LLM provider code paths. Session runner tests lack recorded cassettes for deterministic replay.  
- **Recommendation**: Port `http-recorder` pattern to Rust using `reqwest::Middleware` or a custom `HttpClient` trait with recording/replay capabilities.  
- **Severity**: Critical

---

## 3. E2E Tests

- **Location**: BlazeCode — none.  
- **BlazeCode**: Playwright-based e2e in `packages/app/e2e/` (smoke, regression, performance). `bun run test:e2e:local` in CI.  
- **Gap**: BlazeCode has zero end-to-end tests. No CLI invocation tests, no TUI integration tests, no full workflow tests.  
- **Consequence**: Release quality depends entirely on unit tests. Regression in CLI argument parsing, TUI rendering, or cross-crate interaction goes undetected.  
- **Recommendation**: Add `trycmd` or `assert_cmd` for CLI binary testing. Add `ratatui` rendering snapshot tests. Create a test that starts the server, connects via SSE, sends a prompt, and verifies response.  
- **Severity**: Critical

---

## 4. Test Coverage

- **Location**: BlazeCode — no coverage tool configured.  
- **BlazeCode**: No explicit coverage tool mentioned in workflows, but `bun test` provides built-in coverage via `--coverage`.  
- **BlazeCode**: Estimated coverage ~60-70%. Well-tested core modules (`permission`, `config`, `catalog`, `location`). Untested paths: providers (many have 0–3 tests), CLI entry points, TUI components, LSP handlers, MCP handlers, error recovery paths, concurrent session execution.  
- **Critical untested paths**:  
  - `providers/openai.rs`, `providers/openai_compatible.rs`, `providers/gemini.rs` — 0 test functions.  
  - `providers/openrouter.rs` — 1 test. `providers/xai.rs` — 2 tests. `providers/github_copilot.rs` — 3 tests.  
  - `credential.rs` — 0 test functions.  
  - `bus.rs` — 0 test functions.  
  - `system_context.rs` — 0 test functions.  
  - `model.rs` — 0 test functions.  
  - `policy.rs` — 0 test functions.  
  - `event.rs` — 0 test functions.  
  - `v2_schema.rs` — 0 test functions.  
- **Gap**: 11 modules with zero test functions despite having `mod tests {}` blocks. No coverage tool means coverage cannot be measured or gated.  
- **Recommendation**: Add `cargo-tarpaulin` or `grcov` to CI. Set a coverage threshold (e.g., 60% minimum). Add tests for modules with 0 coverage.  
- **Severity**: High

---

## 5. Test Quality

- **Location**: BlazeCode — tests use `assert!` and `assert_eq!` primarily.  
- **BlazeCode**: Tests use `expect()` matchers with `bun:test`. Pattern: `describe` blocks grouping related tests, `beforeAll`/`afterAll` setup/teardown.  
- **BlazeCode**: Tests are predominantly data-structure serialization roundtrips, rule evaluation matrices, and edge-case enumerations. Example: `permission.rs` has 63 tests covering wildcard matching exhaustively. `image.rs` has 47 tests covering MIME detection for every format. Tests are well-structured with clear names.  
- **Gap**: No property-based testing (no `proptest`/`quickcheck` in dependencies). No fuzzing. Tests only check known inputs — no random or generated inputs.  
- **Consequence**: Edge cases involving unexpected data shapes (e.g., malformed JSON, boundary values in IDs, race conditions in concurrent maps) are untested.  
- **Recommendation**: Add `proptest` or `quickcheck` for property-based testing of core algorithms (ID generation, wildcard matching, serialization roundtrips).  
- **Severity**: Medium

---

## 6. Mock/Stub Strategy

- **Location**: BlazeCode — only 56 references to `mock`/`stub`/`fake` across entire codebase.  
- **BlazeCode**: Uses Effect's `Layer` system for dependency injection. `http-recorder` package records/replays HTTP traffic deterministically. No explicit mocking — tests use real implementations with recorded responses.  
- **BlazeCode**: No mocking framework (no `mockall`, `mockito`, `wiremock` in dependencies). Providers are traits but tests don't mock them — they construct real provider instances in tests. No recorded HTTP cassettes.  
- **Gap**: Without a mock strategy, provider tests require real API keys. Tests that touch the network are skipped or fragile. No HTTP recording infrastructure exists.  
- **Consequence**: Provider integration tests cannot run in CI. Session runner tests cannot safely execute provider calls. PTY tests use real shell processes, making them platform-dependent.  
- **Recommendation**: Implement an `HttpClient` trait that can be swapped between real/replay/record modes. Add `wiremock` or `mockito` for HTTP-level mocking. Add `mockall` for trait mocking in unit tests.  
- **Severity**: Critical

---

## 7. Test Infrastructure

- **Location**: BlazeCode — no dedicated test infrastructure.  
- **BlazeCode**: `packages/core/test/fixture/` provides `tmpdir.ts` (temp directory creation with `Symbol.asyncDispose`), `git.ts`, `location.ts`, `recordings/` directory. `packages/http-recorder/` provides deterministic HTTP recording/replay.  
- **BlazeCode**: `tempfile` crate is in dependencies but used ad-hoc. No shared test fixtures, no test database setup helpers, no standardized temp directory management.  
- **Gap**: Each test module reimplements setup boilerplate. No consistent pattern for database test setup (SQLite in-memory vs file-based). No shared builder pattern for complex test objects.  
- **Consequence**: Test maintainability decreases as the codebase grows. Adding a new module requires reinventing test scaffolding.  
- **Recommendation**: Create `crates/blazecode-core/src/test_support.rs` with: `TestDb::new()`, `TempDir::new()`, `TestConfigBuilder`, `MockProviderBuilder`. Add `#[cfg(test)]` module in `lib.rs` that re-exports test utilities.  
- **Severity**: High

---

## 8. Documentation Testing

- **Location**: BlazeCode — 58 code blocks in doc comments, but only 2 explicitly marked ````rust``.  
- **BlazeCode**: Not applicable (TypeScript — `bun test` doesn't run doc tests).  
- **BlazeCode**: Most doc comments have examples, but few are runnable doctests. `env.rs:143` and `npm.rs:270` have verified doctests; the rest are plain code fences.  
- **Gap**: 56 documentation examples are not compile-checked or tested. They will silently drift from the actual API.  
- **Consequence**: Documentation examples may be incorrect or out of date. Users (and AI agents) relying on docs get broken examples.  
- **Recommendation**: Convert documentation examples to runnable doctests. Add `rust` annotations to code fences. Set `#![deny(rustdoc::broken_intra_doc_links)]`.  
- **Severity**: Medium

---

## 9. Performance Testing

- **Location**: BlazeCode — none.  
- **BlazeCode**: `packages/app/e2e/performance/` directory with Playwright performance benchmarks.  
- **BlazeCode**: No benchmarks, no `criterion` or `divan` in dependencies, no `iai` for instruction counting. No load tests for the SSE server or session processing pipeline.  
- **Gap**: No way to detect performance regressions. Session processing latency, provider streaming throughput, and database query performance are unmeasured.  
- **Consequence**: Performance regressions will ship unnoticed. The async streaming architecture (broadcast channels, tokio tasks) is not validated under load.  
- **Recommendation**: Add `criterion` benchmarks for critical paths (ID generation, wildcard matching, serialization). Add a `#[bench]` module in `session.rs` for session processing throughput. Add k6 or similar for server load testing.  
- **Severity**: Medium

---

## 10. Security Testing

- **Location**: BlazeCode — `audit.yml` runs `cargo-audit` weekly. `ci.yml` runs `cargo-deny` on every commit.  
- **BlazeCode**: No dedicated security workflow visible in the CI files examined.  
- **BlazeCode**: `cargo-audit` checks for known vulnerabilities in dependency tree. `cargo-deny` checks license allowlist and advisories. No fuzzing (`cargo-fuzz` / `cargo-afl` not configured). No SAST (static analysis beyond clippy). No DAST for the HTTP server.  
- **Gap**: No fuzz testing for input parsing (shell commands, config files, JSON payloads). No security unit tests for permission evaluation or MCP authorization. No red-team testing.  
- **Consequence**: Input validation vulnerabilities (e.g., shell injection via crafted prompts, MCP parameter injection) are not proactively discovered.  
- **Recommendation**: Add `cargo-fuzz` harnesses for config file parsing, shell command parsing, and MCP parameter deserialization. Add security-focused unit tests for permission boundary enforcement.  
- **Severity**: Medium

---

## 11. CI Integration

- **Location**: BlazeCode — 1 CI workflow.  
- **BlazeCode**: 27 CI workflows including test.yml, typecheck.yml, review.yml, triage.yml, and many others.  
- **BlazeCode**: 4 jobs in `ci.yml`: `fmt` (ubuntu), `clippy` (ubuntu), `test` (ubuntu+macos+windows), `deny` (ubuntu). `audit.yml` runs weekly. `release.yml` builds on tag. No typecheck job in CI. Test matrix: 3 OS, no feature flag combinations, no MSRV check.  
- **Gap**: No `cargo check` or type checking in CI (relies on clippy). No feature-flag combinatorial testing. No MSRV validation. No test result reporting (JUnit/TAP).  
- **Consequence**: Compilation errors that pass clippy but fail type checking won't be caught until test job. Feature-gated code may break when features are toggled.  
- **Recommendation**: Add a `cargo check --all-targets` job. Test with `--no-default-features` and `--all-features`. Add MSRV check. Upload test results as JUnit XML.  
- **Severity**: Medium

---

## 12. Test Reliability

- **Location**: BlazeCode — generally deterministic.  
- **BlazeCode**: Unknown from examined files.  
- **BlazeCode**: SQLite in-memory database tests are deterministic. PTY tests rely on real process execution (potential flakiness). File system tests create/clean up temp directories (potential race conditions under concurrent test execution). No test retry mechanism.  
- **Gap**: CI runs tests with `--skip_unix_tests` on Windows via `RUSTFLAGS` cfg flag — a manual, error-prone approach. No `#[serial]` annotation for tests that share state.  
- **Consequence**: Windows CI may skip tests accidentally (cfg flag typo or incorrect logic). Concurrent test execution may cause temp directory conflicts.  
- **Recommendation**: Use `serial_test` crate for tests that share global state. Replace RUSTFLAGS-based platform gating with `#[cfg(not(windows))]`. Add `--test-threads=1` for database tests.  
- **Severity**: Medium

---

## 13. Mutation Testing

- **Location**: BlazeCode — none.  
- **BlazeCode**: none.  
- **BlazeCode**: No `cargo-mutants`, no `mutagen`, no mutation testing infrastructure.  
- **Gap**: Without mutation testing, the quality of the test suite cannot be objectively measured. Tests may pass despite having weak assertions.  
- **Consequence**: False confidence in test coverage. Tests may pass after source mutations that should change behavior.  
- **Recommendation**: Run `cargo-mutants` on critical modules (permission, config, session). Set a mutant survival threshold (e.g., <15% survival).  
- **Severity**: Low

---

## 14. Property-Based Testing

- **Location**: BlazeCode — none.  
- **BlazeCode**: none (TypeScript lacks built-in PBT support; `fast-check` not in dependencies).  
- **BlazeCode**: No `proptest` or `quickcheck` in dependencies. All tests use hand-written example inputs.  
- **Gap**: Functions with complex invariants (ID ordering, wildcard pattern matching, serialization roundtrips, state machine transitions) are not tested with generated inputs.  
- **Consequence**: Edge cases in combinatorial logic are missed. For example, wildcard matching with unusual character combinations, or ID generation under concurrency.  
- **Recommendation**: Add `proptest` and write generative tests for: `permission::wildcard_match` (pattern + input generation), `id::ascending`/`descending` (ordering invariants), `serde_json` roundtrips for all public types.  
- **Severity**: Medium

---

## 15. Comparison with BlazeCode

| Dimension | BlazeCode | BlazeCode | Gap |
|---|---|---|---|
| Test file count | 532 | 112 modules (inline) | Separate test directory vs inline |
| Test runner | `bun test` | `cargo test` | Comparable |
| E2E tests | Playwright (app e2e) | None | BlazeCode missing entire category |
| HTTP recording | `http-recorder` package | None | Cannot test providers deterministically |
| Dependency injection | Effect Layers | Trait + struct fields | BlazeCode DI is simpler but harder to mock |
| CI workflows | 27 | 3 (ci, audit, release) | BlazeCode has richer CI pipeline |
| Test fixtures | `tmpdir.ts`, `recordings/`, `git.ts` | None | BlazeCode lacks shared test infrastructure |
| Provider tests | 30+ provider-specific test files | 0 for 3 providers, sparse for others | Large gap |
| Database tests | Migration tests + recorded sessions | 20 database tests | Adequate but no migration tests |
| TUI tests | Not visible (app e2e covers this) | Minimal (editor, theme, clipboard) | Both need improvement |
| Performance tests | Playwright benchmarks | None | BlazeCode gap |
| Security tests | Not visible | cargo-audit + cargo-deny | BlazeCode slightly ahead |
| Property-based tests | None | None | Parity (both absent) |
| Mutation tests | None | None | Parity (both absent) |

---

## Summary

**Strengths** (BlazeCode):
- Extensive unit test coverage: 2,386 test functions across 112 modules
- Thorough edge-case testing in core modules (permission wildcard: 63 tests, image MIME: 47 tests)
- Deterministic tests (SQLite in-memory, no network)
- Security scanning in CI (cargo-audit weekly, cargo-deny per commit)
- Cross-platform test matrix (ubuntu, macos, windows)

**Critical Gaps**:
1. **No provider testing strategy** — 3 provider modules have zero tests, none have recorded cassettes
2. **No E2E tests** — CLI binary, server, TUI, MCP server all untested at integration level
3. **No HTTP recording/replay** — cannot safely replay LLM provider conversations
4. **No coverage tooling** — coverage is invisible and cannot be gated
5. **No test infrastructure** — no shared fixtures, test builders, or standardized setup patterns

**High-Priority Recommendations**:
1. Implement `HttpClient` trait with recording/replay middleware (port `http-recorder` pattern)
2. Add `trycmd`/`assert_cmd` for CLI binary testing
3. Add `cargo-tarpaulin` to CI with minimum coverage threshold
4. Create `test_helpers.rs` module with `TestDb`, `TempDir`, `TestConfig`
5. Add `proptest` for property-based testing of core algorithms
6. Add `mockall` or similar for trait mocking
7. Test all provider modules with recorded cassettes
