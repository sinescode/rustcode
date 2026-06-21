# Dependency Analysis: RustCode vs OpenCode

**Agent**: Agent 11 — Dependency Agent
**Date**: 2026-06-21
**Scope**: All direct + transitive deps in RustCode workspace (6 crates) vs OpenCode workspace (10+ packages)

---

## 1. Crate Inventory — All Direct Rust Dependencies

Total unique direct dependencies: **52** (48 workspace-level + 4 crate-local)

### Async Runtime & Concurrency (7)
| Crate | Version | Purpose |
|---|---|---|
| `tokio` | 1.52 (full) | Async runtime — the backbone |
| `futures` | 0.3 | Async primitives (StreamExt, FutureExt) |
| `tokio-stream` | 0.1 | Stream wrappers for tokio channels |
| `tokio-util` | 0.7 | Cancellation, delayed, poll helpers |
| `tokio-tungstenite` | 0.24 | WebSocket client (SSE streaming) |
| `async-trait` | 0.1 | Async trait methods |
| `pin-project-lite` | 0.2 | Safe pin projections |

### Serialization (6)
| Crate | Version | Purpose |
|---|---|---|
| `serde` | 1.0 (derive) | Serialization framework |
| `serde_json` | 1.0 | JSON format |
| `serde_yaml` | 0.9 | YAML format (config) |
| `toml` | 0.8 | TOML format (config) |
| `schemars` | 0.8 | JSON Schema generation |
| `bytes` | 1.0 | Byte buffer abstraction |

### Error Handling (2)
| Crate | Version | Purpose |
|---|---|---|
| `thiserror` | 2.0 | Derive Error trait |
| `anyhow` | 1.0 | Contextful error propagation |

### HTTP / Networking (4)
| Crate | Version | Purpose |
|---|---|---|
| `axum` | 0.8 | HTTP server framework |
| `reqwest` | 0.12 (rustls-tls) | HTTP client (LLM API calls) |
| `tower` | 0.5 | Service middleware |
| `tower-http` | 0.6 | CORS, compression middleware |

### Database (1)
| Crate | Version | Purpose |
|---|---|---|
| `sqlx` | 0.8 (sqlite) | Async SQL database |

### CLI & Terminal (6)
| Crate | Version | Purpose |
|---|---|---|
| `clap` | 4.6 (derive) | CLI argument parsing |
| `clap_complete` | 4.6 | Shell completion generation |
| `dialoguer` | 0.11 | Interactive prompts |
| `indicatif` | 0.17 | Progress bars/spinners |
| `ratatui` | 0.26 | Terminal UI framework (TUI) |
| `crossterm` | 0.27 | Terminal backend for ratatui |

### Telemetry (3)
| Crate | Version | Purpose |
|---|---|---|
| `tracing` | 0.1 | Structured logging |
| `tracing-subscriber` | 0.3 | Log subscriber (env-filter, json) |
| `tracing-appender` | 0.2 | File log appender |

### Cryptography / Encoding (7)
| Crate | Version | Purpose |
|---|---|---|
| `uuid` | 1 (v4) | Session/message IDs |
| `base64` | 0.22 | Base64 encode/decode |
| `sha2` | 0.10 | SHA-256 hashing |
| `hmac` | 0.12 | HMAC signing |
| `hex` | 0.4 | Hex encode/decode |
| `rand` | 0.8 | Random generation |
| `regex` | 1.0 | Regular expressions |

### Date/Time (1)
| Crate | Version | Purpose |
|---|---|---|
| `chrono` | 0.4 (serde) | Timestamps, formatting |

### File System (6)
| Crate | Version | Purpose |
|---|---|---|
| `dirs` | 6 | Platform config/data dirs |
| `glob` | 0.3 | Glob pattern matching |
| `ignore` | 0.4 | Gitignore-aware file walking |
| `walkdir` | 2 | Recursive directory walk |
| `tempfile` | 3 | Temporary files/dirs |
| `notify` | 6 | File system watcher |

### Parsing / Diff (6)
| Crate | Version | Purpose |
|---|---|---|
| `tree-sitter` | 0.24 | AST parsing framework |
| `tree-sitter-bash` | 0.23 | Bash grammar |
| `similar` | 2 | Text diff engine |
| `shlex` | 1 | Shell lexing |
| `url` | 2 | URL parsing |
| `urlencoding` | 2 | URL percent-encoding |

### Data Structures (2)
| Crate | Version | Purpose |
|---|---|---|
| `dashmap` | 6 | Concurrent hash map |
| `image` | 0.25 | Image processing |

---

## 2. Version Freshness

### Current locked versions vs latest (June 2026)

| Crate | Locked | Latest | Status |
|---|---|---|---|
| `tokio` | 1.52.3 | 1.52.x | ✅ Fresh |
| `serde` | 1.0.228 | 1.0.x | ✅ Fresh |
| `sqlx` | 0.8.6 | 0.8.x | ✅ Fresh |
| `axum` | 0.8.9 | 0.8.x | ✅ Fresh |
| `reqwest` | 0.12.28 | 0.12.x | ✅ Fresh |
| `hyper` | 1.10.1 | 1.10.x | ✅ Fresh |
| `rustls` | 0.23.40 | 0.23.x | ✅ Fresh |
| `clap` | 4.6.1 | 4.x | ⚠️ Stale (4.5.x lock; 4.6+ available) |
| `chrono` | 0.4.45 | 0.4.x | ✅ Fresh |
| `time` | 0.3.49 | 0.3.x | ✅ Fresh |
| `ring` | 0.17.14 | 0.17.x | ✅ Fresh |
| `openssl` | 0.10.81 | 0.10.x | ✅ Fresh |
| `libsqlite3-sys` | 0.30.1 | 0.30.x | ✅ Fresh |
| `ratatui` | 0.26.x | 0.28+ | ⚠️ Ratatui moves fast; 0.26 may be outdated |
| `crossterm` | 0.27.x | 0.28+ | ⚠️ Possibly outdated |
| `tree-sitter` | not locked | 0.25 | ⚠️ Specified 0.24 but not in lock (stub crate) |
| `tree-sitter-bash` | not locked | 0.25 | ⚠️ Specified 0.23 but not in lock |
| `image` | not locked | 0.25 | ✅ Specified 0.25 but not in lock (stub) |

**Verdict**: Most core deps are fresh. `clap`, `ratatui`, `crossterm`, `tree-sitter` lag behind latest.

---

## 3. Security Risks

### Known Advisory: RUSTSEC-2024-0436 (IGNORED)
- **Crate**: `paste` (transitive via unmaintained dep)
- **Severity**: Info (unmaintained, not vulnerable)
- **Action**: Currently ignored in `deny.toml`. Acceptable as it's just a proc-macro helper.
- **Risk**: Low. `paste` is a transitive proc-macro, no runtime risk.

### Supply Chain Concerns
| Risk | Details | Severity |
|---|---|---|
| `openssl-sys` 0.9.117 | C bindings to OpenSSL — CVE surface via C code | Medium |
| `libsqlite3-sys` 0.30.1 | Bundled C SQLite — CVEs in SQLite itself | Medium |
| `ring` 0.17.14 | Crypto in C/asm — ~10 RUSTSEC advisories historically | Low |
| `native-tls` 0.2.18 | Platform TLS — pulls in openssl on Linux | Low |
| `tokio-tungstenite` 0.24 | WebSocket — depends on `tungstenite` which has had DoS CVEs | Low |

### Enforced mitigations
- `forbid(unsafe_code)` in every crate — zero `unsafe` allowed
- TLS via `rustls` (Rust-native) for `reqwest`, `hyper-rustls` also present
- No `native-tls` in reqwest (uses `rustls-tls` feature)

**Note**: `reqwest` pulls in `native-tls` for its default `default-tls` feature, but RustCode uses `rustls-tls` feature. However, `native-tls` is still a transitive dep of `sqlx` (for MySQL/Postgres). Since RustCode uses SQLite only, consider disabling sqlx's default features to drop `native-tls`.

---

## 4. Outdated Packages

| Crate | Version | Issue | Severity |
|---|---|---|---|
| `tree-sitter` | 0.24 | Latest is 0.25+; 0.24 has known parser issues | Medium |
| `tree-sitter-bash` | 0.23 | Latest is 0.25+ | Medium |
| `clap_builder` | 4.6.0 | Newer 4.6.x patch exists | Low |
| `anstream` | 1.0.0 | Newer versions available | Low |
| `paste` (transitive) | unmaintained | Archived by author, no patches | Info |

---

## 5. Redundant Packages

### Overlapping functionality

| Crate | Why redundant | Recommendation |
|---|---|---|
| `serde_yaml` + `toml` | Both for config parsing. OpenCode uses TOML only. | Keep both — YAML useful for MCP/tool configs |
| `glob` + `ignore` + `walkdir` | Three file walking crates. `ignore` subsumes `glob` + `walkdir`. | ✅ Actually needed: `ignore` is gitignore-aware, `walkdir` is simpler, `glob` for pattern matching |
| `sha2` + `hmac` + `hex` | All crypto primitives — could be `ring` alone | Keep — fine-grained is fine |
| `url` + `urlencoding` | URL handling overlap | Keep — separate concerns |
| `shlex` + `regex` | Both for string processing | Keep — different use cases |

### Unnecessary transitive deps
- `sqlx` pulls in `sqlx-mysql`, `sqlx-postgres` even though only SQLite is used
- `openssl`/`native-tls` pulled in despite using `rustls` — source: sqlx's default features

---

## 6. Missing Dependencies — What RustCode Needs From OpenCode

### Critical gaps

| Category | OpenCode | RustCode | Gap | Consequence |
|---|---|---|---|---|
| **AI Provider SDKs** | 20+ `@ai-sdk/*` packages (Anthropic, OpenAI, Google, Bedrock, Azure, Groq, Mistral, Cohere, Perplexity, XAI, DeepInfra, Together, Cerebras, Alibaba, Gateway, Vercel, OpenRouter) | 0 provider-specific crates | **CRITICAL** — RustCode has no provider protocol adapters | Must implement 15+ HTTP protocol adapters from scratch |
| **Schema validation** | `zod` 4.x | `schemars` 0.8 | `schemars` generates JSON Schema but doesn't validate. Need `jsonschema` or `valico` for runtime validation | Runtime validation must be custom-built |
| **Effect system** | `effect` (full functional effects, layers, `Effect.gen`) | `thiserror` + `anyhow` (basic error handling) | No structured concurrency, no dependency injection, no `Context` service system | Manual DI via struct fields, no tracing/AOP |
| **Drizzle ORM** | `drizzle-orm` + `drizzle-kit` (migrations, schema) | `sqlx` raw SQL | No migration framework, no type-safe query builder | Raw SQL + manual migrations |
| **OpenTUI** | `@opentui/core/solid/keymap` | `ratatui` + `crossterm` | Ratatui is lower-level; no widget library comparable | More TUI implementation work |
| **SolidJS reactivity** | `solid-js` (signals, effects, JSX) | Nothing comparable | No reactive UI framework | Ratatui is immediate-mode, not reactive |
| **OpenAuth** | `@openauthjs/openauth` | Nothing | No auth library for server mode | Must implement auth from scratch |

### Moderate gaps

| Category | OpenCode | RustCode | Gap |
|---|---|---|---|
| **Git operations** | `@octokit/rest`, `@octokit/graphql` | `git2` (not listed yet) | No Git library dep — must add `git2` |
| **Package management** | `@npmcli/arborist`, `npm-package-arg` | Nothing | Plugin system needs npm interaction |
| **OpenTelemetry** | `@opentelemetry/api`, exporter, SDK | `tracing` only | No OpenTelemetry export (traces to OTLP) |
| **Diff viewer** | `diff` + `@pierre/diffs` | `similar` 2 | `similar` generates diffs but no rendering |
| **Fuzzy search** | `fuzzysort` 3.1 | Nothing | Missing for command palette/fuzzy matching |
| **Markdown parsing** | `marked` + `marked-shiki` | Nothing | No markdown rendering for chat messages |
| **HTML sanitization** | `dompurify` | Nothing | HTML output from LLM needs sanitization |
| **Shiki syntax highlight** | `@shikijs/stream` + `shiki` | `tree-sitter` only | Tree-sitter for parsing but no highlighting |
| **MCP SDK** | `@modelcontextprotocol/sdk` | None | MCP protocol from scratch |
| **CLI spinner** | `opentui-spinner` + `@clack/prompts` | `indicatif` | Adequate — not a gap |
| **AWS SDK** | `@aws-sdk/client-s3`, `@aws-sdk/credential-providers` | Nothing | Bedrock auth needs SigV4 |
| **ZIP** | `@zip.js/zip.js` | `zip` (not listed) | Need to add `zip` crate |

---

## 7. Dependency Tree Depth

### RustCode
- **Total transitive packages in lock**: 395
- **Direct dependencies**: 52
- **Lock file size**: 3984 lines
- **Tree depth**: max ~8-10 levels (tokio → mio, hyper → h2 → rustls → ring)

### Transitives by category
- Async/IO: ~80 packages (tokio, mio, socket2, etc.)
- HTTP: ~30 packages (hyper, http, h2, etc.)
- TLS/Crypto: ~25 packages (rustls, ring, webpki, etc.)
- SQL: ~25 packages (sqlx-core, sqlx-sqlite, libsqlite3-sys, etc.)
- Serialization: ~15 packages (serde, serde_derive, etc.)
- Windows compat: ~40 packages (windows-sys × 4 variants)
- TUI: ~15 packages (ratatui, crossterm, etc.)
- TOML: ~10 packages (toml, toml_edit, winnow)
- Unicode: ~20 packages (icu4x stack)
- WebAssembly: ~10 packages (wasm-bindgen, wasm-streams, etc.)
- Tracing: ~12 packages

### OpenCode Comparison
OpenCode's JS/TS dependency tree is much larger per-package due to npm's flat-ish structure:
- `packages/core`: ~200+ transitive deps (AI SDK, Effect, Drizzle, OpenTelemetry, etc.)
- `packages/opencode`: ~300+ transitive deps (opencode packages, 20+ AI SDK packages, openauth, modelcontextprotocol, etc.)
- `packages/tui`: ~100+ transitives (SolidJS, OpenTUI, etc.)

**RustCode has fewer transitive deps but each does more heavy lifting.**

---

## 8. License Compliance

### Current allowlist (from `deny.toml`)
MIT, Apache-2.0, BSD-2-Clause, BSD-3-Clause, ISC, Unicode-3.0, Unicode-DFS-2016, Zlib, MPL-2.0, OpenSSL, CC0-1.0, BSL-1.0, CDLA-Permissive-2.0

### Analysis
| License | Crate | Risk |
|---|---|---|
| MIT | Most crates (tokio, serde, clap, axum, ...) | ✅ Safe |
| Apache-2.0 | reqwest, hyper, ring, rustls | ✅ Safe |
| ISC | regex, glob | ✅ Safe |
| MPL-2.0 | `dashmap` | ✅ Allowed, weak copyleft — acceptable for CLI tool |
| OpenSSL | `openssl-sys` | ✅ Allowed (license of linked C lib) |
| CC0-1.0 | Minor crates | ✅ Public domain equivalent |
| BSL-1.0 | Minor crates | ✅ Boost license, very permissive |
| CDLA-Permissive-2.0 | Community Data License | ✅ Allowed |

### Risks
- **MPL-2.0** (`dashmap`): File-level copyleft. If we modify `dashmap` sources (unlikely), must release those changes. Low risk.
- **OpenSSL license**: GPL-incompatible — but RustCode is MIT, not GPL. No conflict.
- **Unicode-DFS-2016** / **Unicode-3.0**: Unicode data files. Safe.

**Verdict**: No license conflicts. Allowlist is comprehensive and reasonable.

---

## 9. Build Time Impact

### Heavy compilation dependencies

| Crate | Impact | Why |
|---|---|---|
| `sqlx` (with sqlite) | **Very High** | Compiles `libsqlite3-sys` (C SQLite), proc macros for query checking |
| `axum` | **High** | Large generic HTTP stack, tower service tree |
| `reqwest` | **High** | Pulls in hyper, h2, rustls, quinn (HTTP/3) |
| `rustls` | **High** | Rust TLS implementation, ring (C/asm crypto) |
| `ring` | **High** | C/asm compiled crypto — slowest single crate |
| `tree-sitter` | **High** | C library compilation, run-time grammar generation |
| `ratatui` | **Medium** | Pure Rust but many generic constraint combinations |
| `image` | **Medium** | Codec implementations (png, jpeg, gif) |
| `tracing-subscriber` | **Medium** | JSON + env-filter features add complexity |
| `schemars` | **Medium** | Proc-macro heavy derive expansion |
| `syn` / `proc-macro2` / `quote` | **Medium** | ~20+ packages use these for derives |
| `clap` | **Low-Medium** | Derive proc-macro + builder tree |

### Estimated build times (cold cache, 8-core)
- `cargo build` (debug): **~8-12 minutes**
- `cargo build --release`: **~20-30 minutes**
- Largest single crate: likely `libsqlite3-sys` or `ring` (C compilation)
- 395 packages in lock → significant resolution/linking time

### Optimization opportunities
- Switch `sqlx` to `runtime-tokio`, `sqlite` only (`no-default-features`) — already done
- Use `sqlx` compile-time checking optionally (feature flag)
- Pre-build `ring` via system package or vendor
- Consider `aws-lc-rs` instead of `ring` (faster compilation, FIPS)

---

## 10. OpenCode Comparison — Dependency Strategy

### Strategy divergence

| Aspect | OpenCode | RustCode |
|---|---|---|
| **Package count** | ~200 direct dependencies across workspace | 52 direct dependencies |
| **AI providers** | 20+ small `@ai-sdk/*` npm packages (each 100-500 LOC) | 0 — must implement providers as modules |
| **Philosophy** | Many small, composable packages (JS micro-packages) | Fewer, larger, integrated crates |
| **Update frequency** | Constant churn — catalog entries change daily | Stable — semver-compatible versions |
| **Tree shaking** | Tree-shaken by bundler (unused code excluded) | Cargo excludes unused code at crate level (dead code elimination) |
| **Transitives** | 2000+ transitive npm packages across workspace | 395 transitive crates |
| **Build system** | Turbo (multi-process, cached) | Cargo (multi-crate, cached) |
| **Type safety** | TypeScript (structural types) | Rust (nominal types + borrow checker) |

### OpenCode's approach: Many small packages
**Pros**: Fine-grained versioning, tree-shakeable, easy to swap providers
**Cons**: Churn in lock file, dependency confusion risk, npm supply chain surface

### RustCode's approach: Fewer integrated crates
**Pros**: Single-source provider impls, smaller supply chain, compile-time checks
**Cons**: More code to write per provider, harder to add new providers

### Provider implementation gap
RustCode must implement what OpenCode gets from `@ai-sdk/*`:
- 15+ provider protocol adapters (Anthropic Messages, OpenAI Chat/Responses, Google Gemini, Bedrock Converse, etc.)
- Streaming SSE parsing, tool call streaming, content block delta handling
- Each ~300-1000 lines of Rust

---

## 11. Consolidated Recommendations

### Critical
1. **Add provider protocol crates** — rustcode-core needs at minimum Anthropic Messages + OpenAI Chat/Responses protocol adapters. OpenCode has 20+; start with 3 most used.
2. **Add `git2` crate** — required for OpenCode parity (git diff, status, worktree operations). Missing entirely.
3. **Add runtime schema validation** — `schemars` generates schemas but doesn't validate. Add `jsonschema` or `valico` crate for LLM tool call validation.

### High
4. **Strip `native-tls`/`openssl`** — sqlx pulls them in for MySQL/Postgres. Use `sqlx` with `no-default-features` + `runtime-tokio` + `sqlite` features only. Tree drops by ~20 packages.
5. **Update `tree-sitter` to 0.25** — OpenCode uses `web-tree-sitter@0.25.10` and `tree-sitter-bash@0.25.0`. RustCode lags at 0.24/0.23.
6. **Update `clap` to latest 4.x patch** — minor but easy hygiene.
7. **Add `zip` crate** — OpenCode uses `@zip.js/zip.js` for JAR/SARIF handling.

### Medium
8. **Add `fuzzysort` equivalent** — `fuzzy-matcher` or `skim` crate for command palette.
9. **Add OpenTelemetry tracing** — `opentelemetry` + `opentelemetry-otlp` crates to match OpenCode's OTLP export.
10. **Lock `tree-sitter-bash` at 0.25** — current 0.23 may have grammar regressions.
11. **Update `ratatui` and `crossterm`** — 0.26 is several releases behind. Ratatui 0.28+ has breaking API changes but important fixes.
12. **Vendor or system-dep `ring`** — slowest compile-time dep; consider `openssl` vendored or `aws-lc-rs`.

### Low
13. **Add `lnbg`-like progress** — `indicatif` is already there (good).
14. **Consider `similar` for unified diff output** — already present; ensure it renders to string for LLM context.
15. **Lock `time` crate vs `chrono`** — both present transitively. Consider `time` sub (dep of `tracing-subscriber` via `matchers`).

---

## 12. Summary Statistics

| Metric | RustCode | OpenCode |
|---|---|---|
| Direct deps | 52 | ~200+ |
| Transitive packages | 395 | ~2000+ |
| AI provider SDKs | 0 | 20+ |
| ORM/database | 1 (sqlx) | 2 (drizzle-orm + sqlite) |
| Auth libraries | 0 | 3+ |
| TLS backends | 2 (rustls + native-tls) | N/A |
| File watcher | 1 (notify) | 2 (chokidar + parcel-watcher) |
| Markdown parsers | 0 | 2 (marked + turndown) |
| Schema validation | 0 (schemars gen only) | 1 (zod) |
| Effect system | 0 (thiserror + anyhow) | 1 (effect) |
| Lock file lines | 3984 | ~10000+ (bun.lock) |
| Build time (cold) | ~8-12 min | ~30-60s (Bun/JIT) |
| Binary size | ~15-25 MB (est) | ~60-100 MB (Bun + JS) |
| Supply chain risk | Low (395 crates, semver) | High (2000+ npm packages) |

---

## 13. Key Findings Summary

- **Biggest gap**: 0 AI provider SDKs vs 20+. RustCode needs custom protocol implementations for every LLM provider.
- **Biggest win**: Rust supply chain is ~1/5 the size of npm's. 395 crates vs 2000+ packages.
- **Biggest risk**: `RUSTSEC-2024-0436` (paste) is ignored. Acceptable (proc-macro only).
- **Biggest bloat**: `ring` (C crypto), `libsqlite3-sys` (C SQLite), `openssl-sys` (C OpenSSL) — 3 C compilation deps. Consider `aws-lc-rs` to replace `ring`.
- **Quickest win**: Drop `native-tls`/`openssl` by configuring sqlx correctly. Save ~20 packages from tree.
- **License**: Clean. Allowlist is comprehensive, no GPL/AGPL conflicts.
