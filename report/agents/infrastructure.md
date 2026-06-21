# Infrastructure Analysis Report — Agent 14

**Date**: 2026-06-21
**Scope**: OpenCode (TypeScript/Bun) vs RustCode (Rust port)

---

## 1. Containerization

| Aspect | OpenCode | RustCode |
|--------|----------|----------|
| Dockerfiles | 5 (base, bun-node, rust, tauri-linux, publish) | 0 |
| Registry | `ghcr.io/anomalyco/build/*:24.04` | N/A |
| Multi-stage | Yes — layered: `base → bun-node → rust/tauri-linux/publish` | N/A |
| Multi-arch | `linux/amd64,linux/arm64` via Docker Buildx | N/A |

- **Location**: `packages/containers/base/Dockerfile:1` (OpenCode)
- **OpenCode**: Full container strategy with 5 Dockerfiles forming a dependency chain. `base` (Ubuntu 24.04 + build tools) → `bun-node` (+ Bun + Node.js 24) → `rust` (+ Rust stable) / `tauri-linux` (+ Tauri deps) / `publish` (+ Docker CLI + AUR tooling). All built via `docker buildx` for multi-arch.
- **RustCode**: No Dockerfiles exist. No containerization whatsoever.
- **Gap**: RustCode has zero containerization. Cannot run in containerized CI, cannot deploy as container image.
- **Consequence**: RustCode CI must install Rust toolchain from scratch on every run (~2-3 min overhead). Cannot leverage pre-baked CI containers for faster builds.
- **Recommendation**: Create a `ci.Dockerfile` with Rust toolchain + dependencies for use as `job.container` in CI. This would cut CI build time significantly.
- **Severity**: Medium

---

## 2. Deployment Strategies

| Aspect | OpenCode | RustCode |
|--------|----------|----------|
| Deployment model | SST (IAC) → Cloudflare Workers + AWS ECS Fargate | GitHub Releases (binary download) |
| Target platform | Serverless (Workers) + Containerized (ECS) | End-user machine (CLI) |
| IAC | SST (sst.config.ts + infra/*.ts) | None |

- **Location**: `sst.config.ts:1-53` (OpenCode), `.github/workflows/release.yml:1-276` (RustCode)
- **OpenCode**: Uses SST as infrastructure-as-code to deploy to Cloudflare Workers (API, Auth, Web, Console, Stats, Teams apps) and AWS ECS Fargate (Lake ingest, Stats sync services). PlanetScale for MySQL database. Stripe for billing.
- **RustCode**: Pure CLI tool. Deployment = GitHub Release with binary archives. No server component to deploy.
- **Gap**: This is an architectural difference — RustCode is a CLI tool while OpenCode has SaaS backend. Not a gap per se, but RustCode will need a server component if it wants to offer cloud features.
- **Consequence**: RustCode cannot provide cloud/SaaS features (team sync, persisted history, analytics). All data is local SQLite.
- **Recommendation**: If cloud features are desired, design a stateless REST API that the CLI communicates with, deployable via Docker/cloud.
- **Severity**: Info

---

## 3. Kubernetes

| Aspect | OpenCode | RustCode |
|--------|----------|----------|
| K8s manifests | None | None |
| Orchestration | AWS ECS Fargate (not K8s) | N/A |
| Serverless | Cloudflare Workers | N/A |

- **Location**: `infra/lake.ts:194-195` (OpenCode ECS cluster)
- **OpenCode**: Uses AWS ECS Fargate (not raw K8s) for stateful services (Lake ingest, Stats sync). No Kubernetes manifests exist. Serverless Cloudflare Workers for all user-facing APIs.
- **RustCode**: No orchestration. Runs locally.
- **Gap**: Neither project uses Kubernetes. This is a deliberate choice by OpenCode (serverless-first).
- **Consequence**: None. Choosing ECS Fargate over K8s is a valid architectural decision that reduces operational complexity.
- **Recommendation**: If RustCode ever needs server-side components, consider same serverless approach (Cloudflare Workers) for API surface to avoid K8s operational burden.
- **Severity**: Info

---

## 4. Observability

| Aspect | OpenCode | RustCode |
|--------|----------|----------|
| Tracing | OpenTelemetry → Honeycomb | `tracing` crate (stdout) |
| Error tracking | Sentry (web + desktop) | None |
| Metrics | Honeycomb (custom calculated fields, SLOs) | None |
| Alerting | Honeycomb Triggers → Discord webhook | None |

- **Location**: `infra/monitoring.ts:1-287` (OpenCode), `Cargo.toml:18-20` (RustCode tracing deps)
- **OpenCode**: Full observability stack:
  - Honeycomb for distributed tracing + metrics with custom calculated fields for failure detection
  - 6 Honeycomb Triggers: Increased Model HTTP Errors (Go/Zen), Low Model TPS (Go/Zen), Increased Provider HTTP Errors, Increased Free Tier Requests
  - Discord webhook receiver for alerts with structured JSON payloads
  - Sentry for web + desktop error tracking (DSN, org, project, release tracking)
  - Cloudflare Worker `tailConsumers` for log processing pipeline
  - Log processor worker sends logs to Honeycomb
- **RustCode**: Has `tracing` + `tracing-subscriber` (with `env-filter`, `json`, `registry` features) + `tracing-appender` in dependencies. Configured at the binary level (not done yet in scaffold). No error tracking, no metrics, no alerting.
- **Gap**: RustCode has the foundation (tracing crate) but no:
  - Structured log shipping (no Honeycomb/Otel exporter)
  - Error tracking (no Sentry equivalent)
  - Metrics collection or alerting
  - Log processor/filter pipeline
- **Consequence**: RustCode users get stdout/stderr logs only. No visibility into crashes, no performance metrics, no proactive alerting. Hard to debug issues in the wild.
- **Recommendation**: Implement OpenTelemetry tracing with `opentelemetry-otlp` crate for Honeycomb compatibility. Add `sentry` crate for crash reporting. Export key metrics as traces with span attributes.
- **Severity**: High

---

## 5. Logging

| Aspect | OpenCode | RustCode |
|--------|----------|----------|
| Structured logging | Yes (Honeycomb events) | Planned (tracing-subscriber JSON) |
| Log levels | Via OpenTelemetry | tracing/env-filter |
| Log rotation | N/A (Cloudflare Workers) | tracing-appender (file rotation) |
| Log shipping | Cloudflare tail consumer → Honeycomb | None (stdout only) |

- **Location**: `infra/console.ts:243-246` (OpenCode LogProcessor), `Cargo.toml:19` (RustCode tracing-subscriber)
- **OpenCode**: All logs are structured events sent to Honeycomb. Cloudflare Workers have `logpush: true` for firehose. Log processor worker consumes worker logs via `tailConsumers` and forwards to Honeycomb.
- **RustCode**: Has `tracing-subscriber` with `env-filter`, `json`, `registry` features + `tracing-appender` for file-based logging with rotation. Currently scaffold — no production logging configuration.
- **Gap**: RustCode has the deps but no configured log pipeline. No log shipping infrastructure exists (no Otel exporter, no log aggregation). No consensus on log format or levels.
- **Consequence**: Currently no useful logging in RustCode beyond `println!` / `eprintln!`. When tracing is implemented, logs stay local to the machine — no remote debugging capability.
- **Recommendation**: Configure tracing-subscriber early with JSON formatting + file appender. Add an optional `--log-format` flag and `--log-file` flag. Consider `opentelemetry-appender-tracing` for future shipping.
- **Severity**: Medium

---

## 6. Monitoring

| Aspect | OpenCode | RustCode |
|--------|----------|----------|
| Uptime monitoring | Cloudflare + Honeycomb | None |
| Performance monitoring | Honeycomb (TPS, latency percentiles) | None |
| Error tracking | Sentry (web + desktop) | None |
| SLOs/Alerts | 6 Honeycomb Triggers with Discord notify | None |

- **Location**: `infra/monitoring.ts:160-287` (OpenCode triggers)
- **OpenCode**:
  - Model HTTP Error Rate trigger (threshold: ≥70% error rate, 5-min window)
  - Low TPS trigger (threshold: ≤10 TPS for Go/Zen tiers, 30-min window)
  - Provider HTTP Error Rate trigger
  - Free Tier Request anomaly detection (day-over-day baseline comparison)
  - All alerts route through Honeycomb → Discord webhook with structured payload
- **RustCode**: No monitoring whatsoever. No crash reporting, no performance tracking, no telemetry.
- **Gap**: RustCode has zero monitoring infrastructure. A crash in the field is invisible to maintainers.
- **Consequence**: Critical bugs go undetected until users report them. No data on which platforms/features have issues. Adoption analytics nonexistent.
- **Recommendation**: Add opt-in telemetry (disabled by default for privacy) using OpenTelemetry. Add `sentry` crate for crash reporting. Instrument key operations (LLM calls, tool executions) with tracing spans.
- **Severity**: Critical

---

## 7. Secret Management

| Aspect | OpenCode | RustCode |
|--------|----------|----------|
| CI/CD secrets | GitHub Environments, SST Secrets | GitHub Secrets |
| Production secrets | SST Secrets (Cloudflare Workers) + SSM Parameter Store | N/A |
| Secret rotation | Manual (SST console/CLI) | N/A |

- **Location**: `infra/secret.ts:7-13` (OpenCode), `infra/app.ts:3-10` (secret declarations)
- **OpenCode**:
  - SST `Secret` resources for app secrets (GITHUB_APP_ID, STRIPE_SECRET_KEY, etc.) — encrypted at rest in Cloudflare Workers
  - AWS SSM Parameter Store for lake ingest secret (`SecureString`)
  - `random.RandomPassword` for generated secrets (Honeycomb webhook secret, lake ingest secret)
  - CI/CD secrets passed via `${{ secrets.* }}` from GitHub Environments (`dev` and `production`)
  - Conditional secrets per stage (`STRIPE_SECRET_KEY_PROD` vs `STRIPE_SECRET_KEY_DEV`)
  - 30+ ZEN_MODELS secrets for model configuration
- **RustCode**: Uses `${{ secrets.GPG_SIGNING_KEY }}` etc. in CI for release signing. No production secret management (local CLI tool).
- **Gap**: RustCode has minimal secret needs (just signing keys for releases). No gap for current architecture.
- **Consequence**: None for current scope. If RustCode adds telemetry or server features, secret management will need to be designed.
- **Recommendation**: If telemetry is added, use environment variables for configuration. Never hardcode API keys. Consider a `.env` file or config directory (e.g., `~/.config/rustcode/`).
- **Severity**: Low

---

## 8. Environment Management

| Aspect | OpenCode | RustCode |
|--------|----------|----------|
| Environments | production, dev, stage-specific (thdxr, vimtor, adam) | N/A |
| Domain isolation | opencode.ai / dev.opencode.ai / *.dev.opencode.ai | N/A |
| DB isolation | PlanetScale branches per stage | N/A |
| Protection | Production: `protect: true`, `removal: retain` | N/A |

- **Location**: `infra/stage.ts:1-21` (OpenCode)
- **OpenCode**:
  - Stage-based infrastructure with SST: `$app.stage` drives all decisions
  - Production: `opencode.ai`, protected, resources retained on destroy
  - Dev: `dev.opencode.ai`, resources removed on destroy
  - Personal stages: `{name}.dev.opencode.ai` for developers
  - PlanetScale branching: each stage gets its own branch from `production`
  - AWS stage mapped: `production → production`, everything else → `dev`
  - Conditional AWS deployment: only `production` and `dev` stages deploy AWS resources
  - Monitoring only enabled for `production` and `vimtor` stages
  - Regional hostname set to US
- **RustCode**: No environment concept. Single binary, single config file per user.
- **Gap**: Architectural difference — RustCode is a local tool. No environment management needed.
- **Consequence**: None.
- **Recommendation**: N/A
- **Severity**: Info

---

## 9. Configuration Management

| Aspect | OpenCode | RustCode |
|--------|----------|----------|
| Config files | N/A (SST infra) | TOML config |
| Env vars | SST environment + Cloudflare Worker bindings | Environment variables |
| Feature flags | Branch-based logic in SST | Not yet implemented |

- **Location**: `infra/app.ts:16-19` (OpenCode Worker env vars), `Cargo.toml:28` (RustCode toml dep)
- **OpenCode**: Configuration is managed through SST's `environment` blocks on Workers, `link` for secret references. Each Worker's environment is defined in infrastructure code. Feature branching via stage checks (e.g., `$dev` flag, tail consumers only for non-personal stages).
- **RustCode**: Has `toml` crate in deps for config file parsing. Planned: `~/.config/rustcode/config.toml` pattern. Environment variable overrides for API keys, model config, etc. Not yet implemented in scaffold.
- **Gap**: RustCode has the dependency but no config file implementation. No config schema or validation.
- **Consequence**: Currently all configuration must be hard-coded or passed via env vars. No persistent user config.
- **Recommendation**: Implement `~/.config/rustcode/config.toml` with serde deserialization. Support env var overrides (e.g., `RUSTCODE_API_KEY`). Define a `Config` struct in `rustcode-core` matching OpenCode's config schema.
- **Severity**: Medium

---

## 10. Backup & Restore

| Aspect | OpenCode | RustCode |
|--------|----------|----------|
| Database | PlanetScale (MySQL) — automatic backups | SQLite (local file) |
| Backup strategy | PlanetScale branching + point-in-time recovery | None |
| Disaster recovery | Multi-region (Workers global, AWS us-east-1/us-east-2) | None |
| RPO/RTO | Unknown (PlanetScale managed) | N/A |

- **Location**: `infra/console.ts:11-43` (OpenCode PlanetScale DB)
- **OpenCode**: Uses PlanetScale for MySQL database with:
  - Automatic backups (PlanetScale managed)
  - Branch-based workflow (production, dev, stage branches)
  - Point-in-time recovery (PlanetScale feature)
  - Development branches parented from `production` for safe schema changes
  - S3 Tables + AWS Glue as data lake for analytics (long-term retention)
  - Production resources have `forceDestroy: false` to prevent accidental deletion
- **RustCode**: Uses SQLite via `sqlx`. Database is a single local file. No backup mechanism whatsoever.
- **Gap**: RustCode has zero backup/disaster recovery for user data. If the SQLite file is corrupted or deleted, all session history is lost.
- **Consequence**: Risk of permanent data loss. No migration path between versions. No way to recover from corruption.
- **Recommendation**: Implement periodic SQLite backups (WAL mode + `VACUUM INTO`). Add an `export`/`import` command for session data. Document backup locations (`~/.local/share/rustcode/`).
- **Severity**: High

---

## 11. CI/CD Infrastructure

| Aspect | OpenCode | RustCode |
|--------|----------|----------|
| Runners | Blacksmith CI (4vcpu custom) | GitHub-hosted (ubuntu/macos/windows-latest) |
| Caching | apt cache, bun install cache | Swatinem/rust-cache |
| Parallelism | Matrix builds (6 targets × Electron) | Matrix builds (5 targets for release) |
| CI speed (est.) | ~5-10 min (pre-baked containers) | ~15-25 min (Rust compile from scratch) |

- **Location**: `.github/workflows/publish.yml:36` (OpenCode Blacksmith), `.github/workflows/ci.yml:20` (RustCode GitHub-hosted)
- **OpenCode**:
  - Uses **Blacksmith CI** (self-hosted, 4vcpu) for most jobs — faster, more reliable than GitHub-hosted
  - Pre-baked container images (`ghcr.io/anomalyco/build/*:24.04`) for Linux jobs
  - Extensive caching: apt packages, bun install, Electron dependencies
  - Matrix builds across 6 targets for Electron desktop app
  - Dedicated Windows signing job on `blacksmith-4vcpu-windows-2025`
  - QEMU + Buildx setup for multi-arch container builds
- **RustCode**:
  - Uses **GitHub-hosted runners** exclusively (ubuntu-latest, macos-latest, windows-latest)
  - `Swatinem/rust-cache` for incremental compilation caching
  - Matrix builds: 4 jobs in CI (fmt, clippy, test matrix on 3 OS, deny), 5 targets for release
  - No pre-baked containers — installs Rust toolchain fresh each run via `dtolnay/rust-toolchain`
  - CI is gated on `cargo build` + `cargo test` for every commit
- **Gap**: RustCode CI is slower due to:
  1. No pre-baked CI containers (must install Rust + deps each time)
  2. GitHub-hosted runners (vs Blacksmith's faster hardware)
  3. Full `cargo build` on every CI run (no `--check` shortcut for test)
- **Consequence**: RustCode CI takes longer. Developer feedback loop is 15-25 min vs OpenCode's 5-10 min.
- **Recommendation**: Create a Docker image with pre-compiled Rust toolchain for CI. Consider caching `target/` directory more aggressively. Use `sccache` for distributed compilation. Evaluate Blacksmith or similar.
- **Severity**: Medium

---

## 12. Release Infrastructure

| Aspect | OpenCode | RustCode |
|--------|----------|----------|
| Package registries | npm, Homebrew, AUR, GHCR, GitHub Releases | GitHub Releases, crates.io |
| Binary signing | Azure Trusted Signing (Windows), Apple codesign (macOS) | GPG signing (optional) |
| SBOM | Not observed | Not observed |
| Channel management | prod, beta channels with separate npm tags | npm-like prerelease flag on GitHub |

- **Location**: `.github/workflows/publish.yml:120-210` (OpenCode signing), `.github/workflows/release.yml:166-180` (RustCode signing)
- **OpenCode**:
  - npm package (`opencode-ai`) published to npmjs.org
  - Homebrew formula maintained (via AUR equivalent or brew tap)
  - AUR (Arch Linux) package via SSH key + pacman
  - Desktop app: Electron-builder produces `.dmg` (macOS), `.exe` (Windows), `.deb`/`.rpm`/`.AppImage` (Linux)
  - **Azure Trusted Signing** for Windows binaries (Authenticode)
  - **Apple codesign** for macOS binaries (Apple Developer cert + notarization API key)
  - Tauri signing private key for Tauri builds
  - GitHub Container Registry for CI container images
  - Channel management: `prod` vs `beta` channel (npm dist-tags, AUR)
  - Windows builds: 3 variants (x64, arm64, x64-baseline)
- **RustCode**:
  - GitHub Releases with SHA256 checksums for each binary
  - Optional GPG signing for archives (if GPG key configured)
  - Simple 5-target matrix (Linux x86_64 + aarch64, macOS x86_64 + aarch64, Windows x86_64)
  - Install script (`install`) does the download + verify + install flow
  - No Windows code signing
  - No macOS code signing
  - No package manager integration (no Homebrew, no apt, no AUR)
  - No npm/native binary distribution
  - **No crates.io publishing yet** (although Cargo.toml defines metadata for it)
- **Gap**: RustCode is missing:
  1. Windows Authenticode signing (all Windows users get "unknown publisher" warnings)
  2. macOS codesign + notarization (Gatekeeper blocks unsigned binaries)
  3. Homebrew/apt/scoop package manager support
  4. No aarch64 Windows build
  5. No SBOM generation
- **Consequence**: Users on Windows and macOS will have installation friction. Without codesign, OS security dialogs scare away casual users. Narrower platform reach.
- **Recommendation**: 
  1. Add Azure Trusted Signing (or equivalent) for Windows EXEs
  2. Add Apple Developer ID signing for macOS binaries
  3. Create Homebrew tap formula
  4. Add `aarch64-pc-windows-msvc` target
  5. Generate SBOM with `cargo cyclonedx` or `cargo sbom`
  6. Actually publish to crates.io for `cargo install` support
- **Severity**: High

---

## 13. Platform Support

| Aspect | OpenCode | RustCode |
|--------|----------|----------|
| Linux x86_64 | ✅ | ✅ |
| Linux aarch64 | ✅ | ✅ |
| macOS x86_64 | ✅ | ✅ |
| macOS aarch64 | ✅ | ✅ |
| Windows x86_64 | ✅ | ✅ (2 variants: MSVC) |
| Windows aarch64 | ✅ | ❌ |

- **Location**: `.github/workflows/release.yml:78-97` (RustCode targets)
- **OpenCode**: All 6 platform targets supported (Linux x86_64/arm64, macOS x86_64/arm64, Windows x86_64/arm64). Windows aarch64 built via `windows-2025` runner.
- **RustCode**: 5 of 6 targets. Missing `aarch64-pc-windows-msvc`. Only `x86_64-pc-windows-msvc` for Windows.
- **Gap**: No Windows on ARM support for RustCode. This affects Surface Pro X, Mac M-series via Parallels/VMware, and future ARM Windows devices.
- **Consequence**: Growing segment of Windows ARM users (~15% of new Windows devices by some estimates) cannot run RustCode natively.
- **Recommendation**: Add `aarch64-pc-windows-msvc` to release matrix. Install `llvm` tools on the Windows ARM runner for cross-compilation. Use `dtolnay/rust-toolchain` with appropriate target.
- **Severity**: Medium

---

## 14. Nix Support

| Aspect | OpenCode | RustCode |
|--------|----------|----------|
| Flake | ✅ flake.nix + flake.lock | ❌ |
| Nix package | ✅ (nixpkgs overlay with opencode + opencode-desktop) | ❌ |
| Dev shell | ✅ (bun, nodejs, openssl, pkg-config, git) | ❌ |
| Platforms | ✅ aarch64/x86_64 Linux + Darwin | ❌ |

- **Location**: `flake.nix:1-73` (OpenCode), `nix/opencode.nix:1-102`, `nix/desktop.nix:1-110`, `nix/node_modules.nix:1-85`
- **OpenCode**: Full Nix support:
  - `flake.nix` with `nixpkgs-unstable` input, 4 system platforms
  - `overlays.default` — provides `opencode` and `opencode-desktop` packages
  - `packages` flake output for both packages
  - `devShells.default` — development environment with bun, nodejs, openssl, pkg-config, git
  - `nix/opencode.nix`: Builds CLI binary from source via `bun build`. Wraps binary with `ripgrep` in PATH. Shell completions (bash + zsh). Version check hook.
  - `nix/desktop.nix`: Builds Electron desktop app. Ad-hoc signing on Darwin, autoPatchelf on Linux. Handles macOS `.app` bundle and Linux unpacked directory.
  - `nix/node_modules.nix`: Fixed-output derivation for deterministic node_modules. Uses `bun install --frozen-lockfile` with per-platform hashes in `hashes.json`. Canonicalizes symlinks and normalizes bun binaries.
  - `hashes.json`: Contains SHA256 hashes for node_modules on all 4 platforms.
- **RustCode**: No Nix support whatsoever. No `flake.nix`, no `shell.nix`, no `default.nix`.
- **Gap**: Full gap — Nix users cannot build/run RustCode without manual setup.
- **Consequence**: NixOS users and Nix enthusiasts are excluded. No reproducible development environment.
- **Recommendation**: Add a `flake.nix` with:
  - `nixpkgs` input (nixpkgs-unstable)
  - `devShell` with `rustup`, `openssl`, `pkg-config`, `sqlite`
  - `packages.default` using `rustPlatform.buildRustPackage` or `cargoBuild`
  - Flake check integration
  - Optionally publish to nixpkgs
- **Severity**: Medium

---

## 15. Installation Methods

| Method | OpenCode | RustCode |
|--------|----------|----------|
| npm | ✅ `npm i -g opencode-ai` | ❌ |
| Homebrew | ✅ `brew install opencode` | ❌ |
| AUR | ✅ `yay -S opencode` | ❌ |
| Nix | ✅ `nix run github:sst/opencode` | ❌ |
| curl \| sh | ✅ install script | ✅ install script |
| Direct download | ✅ GitHub Releases | ✅ GitHub Releases |
| cargo install | ❌ (TypeScript project) | ❌ (not on crates.io yet) |
| Windows (scoop) | ❌ | ❌ |

- **Location**: `install` (both repos) — RustCode install script at `rustcode/install:1-400`
- **OpenCode**: Wide distribution — npm (primary), Homebrew, AUR, Nix, direct download. npm `postinstall` script handles native binary download per platform.
- **RustCode**: Two methods:
  1. `curl -fsSL https://raw.githubusercontent.com/sinescode/rustcode/main/install | bash` — custom install script (400 lines, feature-rich)
  2. Direct download from GitHub Releases
  - The install script supports: version pinning (`--version`), local binary (`--binary`), custom directory (`--dir`), skip PATH modification (`--no-modify-path`), skip checksum (`--skip-checksum`), platform detection (including musl detection on Linux, Rosetta detection on macOS), SHA256 verification, shell config file PATH update (bash, zsh, fish, ash/sh), version check (skips reinstall if same version)
- **Gap**: RustCode is missing:
  1. `cargo install rustcode` (not on crates.io)
  2. Homebrew formula
  3. Scoop (Windows) manifest
  4. Chocolatey (Windows) package
  5. Platform package managers (apt/yum)
- **Consequence**: Limited discovery. Users must find the GitHub repo to install. No `apt install rustcode` or `brew install rustcode` convenience.
- **Recommendation**:
  1. Publish to crates.io (`cargo publish`) for `cargo install rustcode`
  2. Create Homebrew tap with a formula
  3. Create Scoop manifest for Windows users
  4. Consider GitHub Action to auto-publish to package managers on release
- **Severity**: Medium

---

## Summary of Severities

| Severity | Count | Key Findings |
|----------|-------|-------------|
| **Critical** | 1 | No monitoring/telemetry in RustCode (#6) |
| **High** | 3 | No observability pipeline (#4), no backup strategy (#10), missing code signing + package managers (#12) |
| **Medium** | 5 | No containerization (#1), no logging pipeline (#5), slow CI (#11), missing Windows ARM (#13), no Nix support (#14), missing package managers (#15) |
| **Low** | 1 | Minimal secret management needs (#7) |
| **Info** | 3 | Architecture difference in deployment (#2), no K8s (#3), no env management (#8) |

## Top 5 Recommendations

1. **Add OpenTelemetry + Sentry to RustCode** (#4, #6) — Instrument key operations with tracing spans, export to Honeycomb via OTLP, add Sentry for crash reporting. This is critical for understanding real-world usage and catching regressions.

2. **Add Windows + macOS code signing** (#12) — Use Azure Trusted Signing (Windows) and Apple Developer ID (macOS) to avoid OS security warnings. Without this, adoption on these platforms will suffer.

3. **Create CI container image** (#1, #11) — Pre-bake a Docker image with Rust toolchain to cut CI times by 50%+. This directly improves the developer experience.

4. **Add Nix flake** (#14) — Low effort (one file), high impact for Nix users. Enables reproducible dev environments and simplifies contribution.

5. **Publish to crates.io + package managers** (#12, #15) — Enables `cargo install rustcode`, the most natural install path for Rust users. Add Homebrew + Scoop for broader reach.
