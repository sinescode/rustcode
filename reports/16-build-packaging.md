# Report 16: Build, CI, and Release Infrastructure — OpenCode vs RustCode Gap Analysis

**Date**: 2026-06-21
**Scope**: Complete audit of build, CI/CD, release, packaging, and distribution infrastructure.
**Reference**: OpenCode source pinned at commit `5d0f86606ac30690f79f0a6a9f41a1f49fe95d0b`

---

## 1. Executive Summary

OpenCode has a mature, elaborate build and release infrastructure spanning **26 GitHub Actions workflows**, 14 build/release scripts, custom GitHub Actions, an install script, Docker containers, Nix support, and multi-platform release automation targeting Linux, macOS, and Windows across x64 and ARM64 architectures.

RustCode has a minimal CI pipeline consisting of a **single workflow** (`ci.yml`) with 4 jobs: format, clippy, test (ubuntu + macOS), and cargo-deny. There is no release pipeline, no binary distribution, no install mechanism, no Docker support, no cross-platform build matrix (Windows absent), no version management, no code signing, and no publishing infrastructure.

**Overall Gap**: RustCode's build/packaging infrastructure is at approximately **10% of OpenCode's capability**. The gap is severe across every dimension.

### Summary Scores

| Dimension | OpenCode | RustCode | Gap |
|-----------|----------|----------|-----|
| CI workflows | 26 | 1 | -25 |
| Build scripts | 14+ | 0 | -14+ |
| Custom actions | 2 | 0 | -2 |
| Release pipeline | Full | None | Critical |
| Cross-platform builds | 3 OS x 2 arch | 2 OS x 1 arch | Large |
| Binary distribution | curl, npm, brew, choco, scoop, AUR | None | Critical |
| Docker images | Built & published | None | Critical |
| Nix support | Full flake | None | Full |
| Code signing | macOS + Windows | None | Full |
| Install script | 460-line bash | None | Full |
| PR/issue automation | 10+ workflows | 0 | Full |

---

## 2. OpenCode CI/CD Inventory

### 2.1 Complete Workflow Listing

OpenCode has **26 active workflow files** in `.github/workflows/`:

| # | Workflow | Trigger | Purpose |
|---|----------|---------|---------|
| 1 | `test.yml` | Push (dev), PR, workflow_dispatch | Unit tests (Linux + Windows), E2E tests with Playwright |
| 2 | `typecheck.yml` | Push (dev), PR, workflow_dispatch | `bun typecheck` |
| 3 | `opencode.yml` | Issue/PR comment containing `/oc` or `/opencode` | Run opencode via GitHub Action |
| 4 | `review.yml` | Issue comment starting with `/review` | AI PR review |
| 5 | `pr-standards.yml` | PR opened/edited/synchronized | Check title format, linked issues, template compliance |
| 6 | `pr-management.yml` | PR opened | Duplicate detection, contributor labeling |
| 7 | `compliance-close.yml` | Scheduled every 30min | Auto-close non-compliant PRs after 2h |
| 8 | `close-prs.yml` | Scheduled daily, workflow_dispatch | Close old PRs without enough reactions |
| 9 | `close-issues.yml` | Scheduled daily | Close stale issues |
| 10 | `duplicate-issues.yml` | Issue opened/edited | Detect duplicate issues + compliance check |
| 11 | `triage.yml` | Issue opened | AI-powered issue triage |
| 12 | `publish.yml` | Push (ci/dev/beta), workflow_dispatch | Full release pipeline (see 2.2) |
| 13 | `beta.yml` | Workflow_dispatch, hourly schedule | Beta branch sync from beta-labeled PRs |
| 14 | `containers.yml` | Push (dev) modifying containers/, workflow_dispatch | Docker container build & push |
| 15 | `deploy.yml` | Push (dev/production), workflow_dispatch | SST cloud deployment |
| 16 | `generate.yml` | Push (dev) | Auto-generate code (SDK, types) + commit |
| 17 | `publish-github-action.yml` | Tag push `github-v*.*.*` | Publish GitHub Action |
| 18 | `publish-vscode.yml` | Tag push `vscode-v*.*.*` | Publish VS Code extension |
| 19 | `release-github-action.yml` | Push (dev) modifying `github/` | Auto-release GitHub Action updates |
| 20 | `publish-python-sdk.yml` | (disabled/placeholder) | Python SDK PyPI publishing |
| 21 | `nix-eval.yml` | Push (dev), PR, workflow_dispatch | Nix flake evaluation on 4 platforms |
| 22 | `nix-hashes.yml` | Push (dev/beta) modifying lockfiles, workflow_dispatch | Compute node_modules hashes for Nix |
| 23 | `stats.yml` | Scheduled daily, workflow_dispatch | Update download stats |
| 24 | `storybook.yml` | Push/PR (dev) modifying storybook/ui | Storybook build check |
| 25 | `notify-discord.yml` | Release published | Send release notification to Discord |
| 26 | `docs-update.yml` | Scheduled every 12h, workflow_dispatch | Auto-generate docs from recent commits |

### 2.2 Publish Pipeline (`publish.yml`) — Detailed Breakdown

The `publish.yml` workflow is OpenCode's most sophisticated pipeline. It has 5 job stages:

#### Stage 1: Version (1 job)
- Runs on `blacksmith-4vcpu-ubuntu-2404`
- Executes `script/version.ts` which:
  - Determines the next version (bump: major/minor/patch, or explicit override)
  - Generates changelog via `script/changelog.ts`
  - Creates a **draft GitHub release** with changelog
  - Outputs: `version`, `release` (databaseId), `tag`, `repo`
- Can operate on `beta` channel (uses different repo `anomalyco/opencode-beta`)

#### Stage 2: Build CLI (1 job)
- Builds native binaries via `packages/opencode/script/build.ts`
- Optionally includes sourcemaps for beta builds
- Uploads 3 artifact groups:
  - `opencode-cli` — macOS + Linux binaries (darwin-*, linux-*)
  - `opencode-cli-windows` — Windows binaries (windows-*)
  - `opencode-preview-cli` — Preview CLI from `packages/cli/`

#### Stage 3: Sign Windows CLI (1 job)
- Runs on Windows runner
- Downloads `opencode-cli-windows` artifact
- Signs executables using **Azure Trusted Signing** (Azure code signing service)
- Verifies signatures with `Get-AuthenticodeSignature`
- Repacks as ZIP archives
- Uploads signed artifacts to draft release (if release is non-draft)
- Uploads `opencode-cli-signed-windows` artifact

#### Stage 4: Build Electron Desktop (6 jobs — matrix)
- 6 platform targets:
  - `macOS x86_64` (macos-26-intel)
  - `macOS ARM64` (macos-26)
  - `Windows ARM64` (windows-2025, github-hosted)
  - `Windows x86_64` (blacksmith-4vcpu-windows-2025)
  - `Linux x86_64` (blacksmith-4vcpu-ubuntu-2404)
  - `Linux ARM64` (blacksmith-4vcpu-ubuntu-2404-arm)
- Each job:
  1. Imports code signing certs (macOS: Apple certs, Windows: Azure login)
  2. Builds Electron app
  3. Packages with `electron-builder` (DMG, `exe`, `AppImage`, `deb`, `rpm`)
  4. Creates `.app.tar.gz` for macOS
  5. Verifies Windows signatures
  6. Uploads artifacts: `opencode-desktop-<target>`, `latest-yml-<target>`

#### Stage 5: Publish (final job)
- Needs: version + build-cli + sign-cli-windows + build-electron
- Downloads all artifacts
- Steps:
  1. Login to GHCR (Docker registry)
  2. Set up QEMU + Docker Buildx for multi-arch
  3. Set up Node for npm publishing
  4. Download all build artifacts (CLI, signed Windows, preview CLI, desktop)
  5. Setup AUR SSH key for Arch Linux publishing
  6. Upload desktop release assets to GitHub release
  7. Run `script/publish.ts` which:
     - Updates all `package.json` version fields
     - Runs `bun install` to regenerate lockfile
     - Builds SDK, plugin packages
     - Publishes CLI, preview CLI, SDK, plugin to npm
     - Finalizes `latest.yml` / `latest.json` for auto-updater
     - Commits version bump to dev branch
     - Pushes git tag
     - Marks draft release as published

### 2.3 Custom GitHub Actions

OpenCode has 2 custom actions in `.github/actions/`:
- **`setup-bun/`** — Sets up Bun runtime with caching and platform-specific install flags
- **`setup-git-committer/`** — Configures git identity with GitHub App authentication for privileged operations

### 2.4 Build Scripts

Located in `script/`:

| Script | Language | Purpose |
|--------|----------|---------|
| `version.ts` | TypeScript/Bun | Version bump, changelog gen, draft release creation |
| `publish.ts` | TypeScript/Bun | Multi-package publish to npm, git tag/release |
| `beta.ts` | TypeScript/Bun | Beta branch management, conflict resolution |
| `changelog.ts` | TypeScript/Bun | Generate UPCOMING_CHANGELOG.md via opencode |
| `raw-changelog.ts` | TypeScript/Bun | Raw changelog output |
| `release` | Bash | Trigger publish workflow (`gh workflow run publish.yml`) |
| `format.ts` | TypeScript/Bun | Code formatting |
| `generate.ts` | TypeScript/Bun | SDK/code generation |
| `stats.ts` | TypeScript/Bun | Download statistics |
| `duplicate-pr.ts` | TypeScript/Bun | PR duplicate detection |
| `upgrade-opentui.ts` | TypeScript/Bun | OpenTUI dependency upgrade |
| `sign-windows.ps1` | PowerShell | Windows code signing |
| `github/close-issues.ts` | TypeScript/Bun | Stale issue closing |
| `github/close-prs.ts` | TypeScript/Bun | Old PR closing |

### 2.5 Install Script

`install` — a **460-line** bash script supporting:
- `curl -fsSL https://opencode.ai/install | bash`
- `--version` for specific version installation
- `--binary` for local binary installation
- `--no-modify-path` to skip shell config modification
- Automatic platform detection (OS + arch)
- GitHub release asset download
- Shell profile update (.zshrc, .bashrc, etc.)

### 2.6 Other Infrastructure

- **Docker**: `containers.yml` builds and pushes multi-arch containers to GHCR from `packages/containers/`
- **Nix**: Full Nix flake with `nix-eval.yml` and `nix-hashes.yml` — 4 platforms (x86_64-linux, aarch64-linux, x86_64-darwin, aarch64-darwin)
- **Code Signing**: Windows (Azure Trusted Signing), macOS (Apple Developer ID via `apple-actions/import-codesign-certs`)
- **Discord Integration**: Release notifications via `SethCohen/github-releases-to-discord`

---

## 3. RustCode Current CI Infrastructure

### 3.1 Single Workflow (`ci.yml`)

The entirety of RustCode's CI is a single file with 4 jobs:

```yaml
name: CI

on:
  push:
    branches: [main, dev, "feat/*", "fix/*"]
  pull_request:

jobs:
  fmt:
    name: Format
    runs-on: ubuntu-latest
    steps:
      - uses: actions/checkout@v4
      - uses: dtolnay/rust-toolchain@stable
        with: { components: rustfmt }
      - run: cargo fmt --all -- --check

  clippy:
    name: Clippy
    runs-on: ubuntu-latest
    steps:
      - uses: actions/checkout@v4
      - uses: dtolnay/rust-toolchain@stable
        with: { components: clippy }
      - uses: Swatinem/rust-cache@v2
      - run: cargo clippy --all-targets --all-features -- -D warnings

  test:
    name: Test (${{ matrix.os }})
    runs-on: ${{ matrix.os }}
    strategy:
      fail-fast: false
      matrix:
        os: [ubuntu-latest, macos-latest]
    steps:
      - uses: actions/checkout@v4
      - uses: dtolnay/rust-toolchain@stable
      - uses: Swatinem/rust-cache@v2
      - run: cargo build --all-targets --verbose
      - run: cargo test --all --verbose

  deny:
    name: Cargo Deny
    runs-on: ubuntu-latest
    steps:
      - uses: actions/checkout@v4
      - uses: EmbarkStudios/cargo-deny-action@v2
```

### 3.2 Key Observations

- **No Windows testing** — only `ubuntu-latest` and `macos-latest`
- **No release pipeline** — no way to create binaries, tag releases, or publish
- **No version management** — versions are static `0.1.0` in workspace
- **No artifact upload** — build artifacts are never saved
- **No install script** — users must build from source with `cargo build`
- **No Docker support** — no container image build
- **No code signing** — no mechanism for trusted binary distribution
- **No concurrency control** — no `concurrency` group to cancel stale runs
- **No PR/issue automation** — no standards checks, triage, or stale management
- **No scheduled workflows** — no nightly builds or maintenance tasks
- **No notification** — no Discord/Slack integration on build results
- **Basic caching** — only uses `Swatinem/rust-cache@v2`

---

## 4. Gap Analysis — Detailed

### 4.1 CI Configuration (Build, Test, Lint on Push/PR)

| Feature | OpenCode | RustCode | Priority | Notes |
|---------|----------|----------|----------|-------|
| Push triggers | Branch-specific (dev) | main, dev, feat/*, fix/* | Done | RustCode already has good trigger config |
| PR triggers | Full | Full | Done | Already present |
| Workflow dispatch | Common | None | Medium | Useful for manual CI triggers |
| Concurrency control | Per-branch with cancel-in-progress | None | **High** | Prevents wasted runner time |
| Matrix strategy | Linux + Windows | Linux + macOS | **High** | Windows is critical for cross-platform support |
| Build step | `bun turbo test` | `cargo build --all-targets` | Done | Already present |
| Test step | Unit + E2E (Playwright) | Unit only | Medium | E2E tests needed but may be early |
| Lint step | oxlint | clippy | Done | Already present |
| Format check | prettier | rustfmt | Done | Already present |
| Type check | `bun typecheck` | (compile-time) | Done | Rust's compiler is the typechecker |
| Fast failure | fail-fast: false | fail-fast: false | Done | Already configured |
| Timeout limits | 20-60 min | None | Low | Prevents runaway jobs |
| Env variables | FORCE_JAVASCRIPT_ACTIONS_TO_NODE24, CI | CARGO_TERM_COLOR, RUST_BACKTRACE | Done | Already present |

### 4.2 Release Pipeline

| Feature | OpenCode | RustCode | Priority | Notes |
|---------|----------|----------|----------|-------|
| Version management | `script/version.ts` with bump automation | Manual Cargo.toml edits | **Critical** | No release process exists |
| Changelog generation | `script/changelog.ts` with AI | None | **High** | Needed for release notes |
| GitHub Release creation | Automated draft release | None | **Critical** | No release artifact publishing |
| Release tagging | Automated git tag | None | **Critical** | No version history |
| Release publishing | `script/publish.ts` + `gh release edit` | None | **Critical** | No published releases |
| Multi-package publish | npm (CLI, SDK, plugin, preview) | None | Medium | Cargo publish could be analogous |
| Draft-to-published flow | Draft → review → publish | None | **High** | Standard release workflow |
| Release notes | Auto-generated from changelog | None | Medium | Important for users |
| Beta/nightly releases | `beta.yml` + beta branch | None | Medium | Useful for pre-release testing |
| Canary/preview builds | Preview CLI package | None | Low | Advanced feature |

### 4.3 Cross-Platform Builds

| Feature | OpenCode | RustCode | Priority | Notes |
|---------|----------|----------|----------|-------|
| Linux x86_64 | Yes | Yes (test only) | Done | Works but no artifact produced |
| Linux ARM64 | Yes (blacksmith ARM runner) | No | **High** | Needed for Raspberry Pi, AWS Graviton |
| macOS x86_64 | Yes (macos-26-intel) | Yes (test only) | Done | Already in test matrix |
| macOS ARM64 | Yes (macos-26) | No | **High** | Needed for Apple Silicon users |
| Windows x86_64 | Yes (blacksmith windows) | No | **Critical** | Windows users can't use rustcode |
| Windows ARM64 | Yes (windows-2025) | No | Medium | Niche but growing |
| Windows code signing | Yes Azure Trusted Signing | No | Low | Only for production releases |
| macOS code signing | Yes Apple Developer ID | No | Low | Only for production releases |
| Cross-compilation | Via build scripts | Possible via `--target` | Medium | Should be added for ARM targets |
| Artifact naming | Platform-specific | None | **High** | Needed for distribution |

### 4.4 Binary Distribution

| Feature | OpenCode | RustCode | Priority | Notes |
|---------|----------|----------|----------|-------|
| Install script | 460-line bash (`install`) | None | **Critical** | Primary user acquisition channel |
| Homebrew | Yes (via cask) | No | Medium | Popular on macOS |
| npm/pnpm/bun | Yes (opencode-ai package) | No | Medium | JS ecosystem distribution |
| AUR (Arch Linux) | Yes (automated publish) | No | Low | Niche Linux distribution |
| scoop (Windows) | Yes (in publish.ts) | No | Low | Windows package manager |
| choco (Windows) | Yes (in publish.ts) | No | Low | Windows package manager |
| curl pipe install | Yes (opencode.ai/install) | No | **Critical** | Most common install method |
| GitHub Release assets | Yes (auto-uploaded) | No | **Critical** | Manual download option |
| Auto-update | Yes (electron-builder + latest.yml) | No | Low | Built-in update mechanism |

### 4.5 Docker Image Build

| Feature | OpenCode | RustCode | Priority | Notes |
|---------|----------|----------|----------|-------|
| Dockerfile | Yes (packages/containers/) | None | Medium | Useful for server deployment |
| Multi-arch build | Yes (QEMU + Buildx) | None | Low | ARM64 + x86_64 |
| GHCR publish | Yes (ghcr.io/anomalyco/opencode) | None | Low | Container registry hosting |
| Automated build | Yes (containers.yml) | None | Low | On push to dev |
| Base images | bun-node, rust, tauri-linux | None | Low | Development environments |

### 4.6 Version Management

| Feature | OpenCode | RustCode | Priority | Notes |
|---------|----------|----------|----------|-------|
| Single version source | `package.json` + workspace | `Cargo.toml` workspace | Done | Already uses workspace versioning |
| Auto-bump (major/minor/patch) | Yes (workflow_dispatch input) | No | **Critical** | No version management at all |
| Version override | Yes (optional string input) | No | Medium | For emergency releases |
| Version propagation | Updates all package.json files | Single Cargo.toml | Done | Simpler in Rust (workspace) |
| Git tag creation | Yes (vX.Y.Z format) | No | **Critical** | No release tagging |
| Version in binary | Yes (`opencode --version`) | Yes (`rustcode --version`) | Done | Uses `env!("CARGO_PKG_VERSION")` |
| Version displayed in CLI | Yes | Yes (main.rs:36) | Done | Already present |

### 4.7 Dependency Auditing

| Feature | OpenCode | RustCode | Priority | Notes |
|---------|----------|----------|----------|-------|
| cargo-deny action | N/A (not applicable — JS) | Yes | Done | Already configured |
| License allowlist | N/A | Yes (in deny.toml) | Done | Already present |
| Advisory ignore | N/A | Yes (1 ignored) | Done | Already present |
| Dependency graph | Not applicable | `cargo metadata` | Low | Optional for SBOM |
| Dependabot | Not seen (likely via GitHub) | No | **High** | Automatic dependency updates |
| Bun release age check | Yes (minimumReleaseAge in bunfig.toml) | N/A | Low | JS-specific feature |

### 4.8 Linting Configuration

| Feature | OpenCode | RustCode | Priority | Notes |
|---------|----------|----------|----------|-------|
| Linter | oxlint (1.60.0) | clippy | Done | Already configured |
| Lint on CI | In publish pipeline | Yes (clippy job) | Done | Already present |
| Pedantic lints | Not applicable | `warn(clippy::all)` only | **High** | Should enable `pedantic` gradually |
| Security lints | Not applicable | No | **High** | Should add `clippy::cargo`/ `clippy::nursery` for relevant checks |
| Custom lint config | `.oxlintrc.json` | `#![warn(...)]` in code | Done | Already configured per-crate |
| Lint on all targets | Not applicable | `--all-targets` | Done | Already present |
| Lint on all features | Not applicable | `--all-features` | Done | Already present |
| Deny warnings in CI | N/A (fail on lint) | `-D warnings` | Done | Already configured |

### 4.9 Code Formatting

| Feature | OpenCode | RustCode | Priority | Notes |
|---------|----------|----------|----------|-------|
| Formatter | prettier (3.6.2) | rustfmt | Done | Already configured |
| CI check | In Storybook workflow (implicit) | Yes (`cargo fmt --check`) | Done | Already present |
| Editor config | `.editorconfig` + `.prettierignore` | No | Medium | Should add `.editorconfig` and `rustfmt.toml` |
| Custom config | `{"semi": false, "printWidth": 120}` | Default settings | Medium | Should add `rustfmt.toml` with custom settings |

### 4.10 Security Scanning

| Feature | OpenCode | RustCode | Priority | Notes |
|---------|----------|----------|----------|-------|
| gitleaks | Yes (`.gitleaksignore` exists) | No | **High** | Prevents secret leakage |
| cargo-deny | N/A (not applicable) | Yes | Done | Already present |
| Advisory DB | Not applicable | Yes (cargo-deny advisory) | Done | Already present |
| SAST scanning | Not applicable | No | Medium | Should consider `cargo-audit` |
| SBOM generation | Not applicable | No | Low | Software Bill of Materials |
| Dependency review | Not seen | No | Medium | GitHub Dependency Review action |
| CodeQL | Not seen | No | **High** | GitHub's built-in code analysis |

### 4.11 PR and Issue Automation

| Feature | OpenCode | RustCode | Priority | Notes |
|---------|----------|----------|----------|-------|
| PR title validation | Yes (conventional commit check) | No | **High** | Important for changelog generation |
| PR template compliance | Yes (full checklist) | No | Medium | Ensures quality |
| Issue templates | Yes (`.github/ISSUE_TEMPLATE/`) | No | Medium | Guides contributors |
| Duplicate detection | Yes (AI-powered) | No | Low | Advanced feature |
| Stale issue/PR management | Yes (close-prs, close-issues) | No | Medium | Prevents issue rot |
| Auto-labeling | Yes (contributor, needs:*) | No | Medium | Helps maintainers |
| AI triage | Yes (triage.yml) | No | Low | Advanced feature |
| AI review | Yes (review.yml) | No | Low | Advanced feature |
| Compliance enforcement | Yes (compliance-close.yml) | No | Low | Could be overkill for early project |

### 4.12 Infrastructure / Other

| Feature | OpenCode | RustCode | Priority | Notes |
|---------|----------|----------|----------|-------|
| Nix flake | Yes (flake.nix + flake.lock) | No | Low | Advanced dev environment |
| Code generation | Yes (generate.yml) | No | Low | Auto-generate from schemas |
| Storybook | Yes (storybook.yml) | No | N/A | UI component library |
| Stats tracking | Yes (stats.yml) | No | Low | Download/usage stats |
| Discord notification | Yes (notify-discord.yml) | No | Medium | Team awareness on releases |
| Docs automation | Yes (docs-update.yml) | No | Low | AI-powered docs |
| Locale sync | Yes (docs-locale-sync.yml) | No | N/A | i18n for docs |
| Cloud deployment | Yes (deploy.yml, SST) | No | N/A | Cloud-specific |

---

## 5. Recommended Fixes for RustCode

### 5.1 Critical Priority (Must Fix)

#### C-1: Add Windows to CI Test Matrix
**File**: `.github/workflows/ci.yml`
Add `windows-latest` to the `test` job matrix, with conditional build/test commands for Windows.

#### C-2: Add GitHub Release Pipeline
**File**: Create `.github/workflows/release.yml`
Implement a release workflow triggered by tag push (`v*.*.*`) that:
- Builds for Linux (x86_64 + ARM64), macOS (x86_64 + ARM64), Windows (x86_64)
- Creates GitHub Release with platform-specific binary artifacts
- Updates version from tag

Implementation approach:
```yaml
name: Release

on:
  push:
    tags: ["v*.*.*"]

jobs:
  build:
    strategy:
      matrix:
        include:
          - target: x86_64-unknown-linux-gnu
            os: ubuntu-latest
          - target: aarch64-unknown-linux-gnu
            os: ubuntu-latest
          - target: x86_64-apple-darwin
            os: macos-latest
          - target: aarch64-apple-darwin
            os: macos-latest
          - target: x86_64-pc-windows-msvc
            os: windows-latest
    steps:
      - uses: actions/checkout@v4
      - uses: dtolnay/rust-toolchain@stable
      - uses: Swatinem/rust-cache@v2
      - run: cargo build --release --target ${{ matrix.target }}
      - uses: actions/upload-artifact@v4
        with:
          name: rustcode-${{ matrix.target }}
          path: target/${{ matrix.target }}/release/rustcode*

  release:
    needs: build
    runs-on: ubuntu-latest
    steps:
      - uses: actions/download-artifact@v4
      - run: gh release create ${{ github.ref_name }} ./rustcode-*/*
```

#### C-3: Add Version Management Script
**File**: Create `script/bump-version.sh`
Automate version bumping in Cargo.toml workspace:
```bash
#!/usr/bin/env bash
set -euo pipefail
CURRENT=$(grep '^version' Cargo.toml | head -1 | sed 's/.*"\(.*\)"/\1/')
IFS='.' read -r major minor patch <<< "$CURRENT"
case "${1:-patch}" in
  major) major=$((major+1)); minor=0; patch=0 ;;
  minor) minor=$((minor+1)); patch=0 ;;
  patch|*) patch=$((patch+1)) ;;
esac
NEW="$major.$minor.$patch"
sed -i "s/^version = \"$CURRENT\"/version = \"$NEW\"/" Cargo.toml
echo "Bumped to $NEW"
```

#### C-4: Add Concurrency Control
**File**: `.github/workflows/ci.yml`
Add at the top level:
```yaml
concurrency:
  group: ${{ github.workflow }}-${{ github.event.pull_request.number || github.ref }}
  cancel-in-progress: true
```

#### C-5: Add cargo-audit Security Scanning
**File**: `.github/workflows/ci.yml` (add job)
```yaml
audit:
  name: Security Audit
  runs-on: ubuntu-latest
  steps:
    - uses: actions/checkout@v4
    - uses: dtolnay/rust-toolchain@stable
    - run: cargo install cargo-audit
    - run: cargo audit
```
Or use the `rustsec/audit-check` action.

### 5.2 High Priority (Should Fix)

#### H-1: Add Linux ARM64 Build Target
Add `aarch64-unknown-linux-gnu` to the test/build matrix using QEMU or an ARM runner.

#### H-2: Add macOS ARM64 Build Target
Add `aarch64-apple-darwin` to the test matrix (available as `macos-latest` on GitHub Actions).

#### H-3: Enable Clippy Pedantic Lints Gradually
In `src/main.rs` and each crate, progressively enable:
```rust
#![warn(clippy::pedantic)]
#![warn(clippy::cargo)]
```
Start with `clippy::all` (already enabled), then add `pedantic` with targeted allows for noisy false positives.

#### H-4: Add Dependabot Configuration
**File**: Create `.github/dependabot.yml`
```yaml
version: 2
updates:
  - package-ecosystem: cargo
    directory: "/"
    schedule:
      interval: weekly
    open-pull-requests-limit: 10
  - package-ecosystem: github-actions
    directory: "/"
    schedule:
      interval: weekly
```

#### H-5: Add CodeQL Analysis
**File**: Create `.github/workflows/codeql.yml`
```yaml
name: CodeQL
on:
  push:
    branches: [main, dev]
  pull_request:
    branches: [main]
jobs:
  analyze:
    runs-on: ubuntu-latest
    steps:
      - uses: actions/checkout@v4
      - uses: github/codeql-action/init@v3
        with:
          languages: rust
      - uses: github/codeql-action/analyze@v3
```

#### H-6: Add gitleaks Secret Scanning
**File**: `.github/workflows/ci.yml` (add job or separate workflow)
```yaml
secrets:
  name: Secrets Detection
  runs-on: ubuntu-latest
  steps:
    - uses: actions/checkout@v4
    - uses: gitleaks/gitleaks-action@v2
```

#### H-7: Add Install Script
**File**: Create `script/install.sh` (minimum viable version)
Create a bash install script that:
- Detects OS + arch
- Finds latest release from GitHub API
- Downloads the appropriate binary from GitHub Releases
- Places it in `/usr/local/bin` or similar
- Optionally adds to PATH

#### H-8: Add .editorconfig and rustfmt.toml
**File**: `.editorconfig` at repo root
**File**: `rustfmt.toml` at repo root
```toml
# rustfmt.toml
max_width = 120
edition = "2021"
```

### 5.3 Medium Priority

#### M-1: Add Release Workflow Dispatch
Allow manual triggering of release builds with version input (similar to OpenCode's `workflow_dispatch` with `bump` and `version` inputs).

#### M-2: Add Changelog Script
Create `script/changelog.sh` or `script/changelog.rs` that generates changelog from git log between tags. Can use `git-cliff` or `cargo-changelog` for this purpose.

#### M-3: Add Nightly Build Workflow
**File**: `.github/workflows/nightly.yml`
Triggered on schedule, builds all targets, uploads artifacts with date-based tags.

#### M-4: Add Issue Templates
**File**: Create `.github/ISSUE_TEMPLATE/bug_report.md` and `feature_request.md`
Basic templates with structured fields.

#### M-5: Add PR Template
**File**: Create `.github/pull_request_template.md`
Checklist-based template covering:
- What does this PR do?
- Type of change
- Testing performed
- Checklist (tests, docs, linting)

#### M-6: Add cargo deny Vulnerability Checks
Enhance `deny.toml` to enforce:
- `advisories.vulnerability = "deny"` (already default)
- Add `bans.multiple-versions = "deny"` instead of warn
- Consider adding `sources.unknown-registry = "deny"` for supply chain security

#### M-7: Add Concurrency to All Workflows
As new workflows are added, ensure each has appropriate concurrency settings.

### 5.4 Low Priority

#### L-1: Docker Support
Add a `Dockerfile` for server deployment and a `Docker` workflow for automated builds to GHCR.

#### L-2: Homebrew Formula
Create a Homebrew formula for `rustcode` and publish via GitHub Releases.

#### L-3: Nix Flake
Add a `flake.nix` for Nix-based builds and development environments.

#### L-4: Discord Notification
Add a workflow to post to Discord on release, similar to OpenCode's `notify-discord.yml`.

#### L-5: Auto-generated SBOM
Add `cargo sbom` or `cyclonedx-bom` generation to release pipeline.

---

## 6. Implementation Plan

### Phase 1: Critical (Week 1)

| # | Task | Effort | Dependencies |
|---|------|--------|-------------|
| C-1 | Add Windows to CI test matrix | 1h | None |
| C-2 | Add release pipeline (GitHub Release + binary artifacts) | 4h | C-1 (for Windows builds) |
| C-3 | Add version management script | 1h | None |
| C-4 | Add concurrency control to ci.yml | 15min | None |
| C-5 | Add cargo-audit to CI | 30min | None |

### Phase 2: High (Week 2)

| # | Task | Effort | Dependencies |
|---|------|--------|-------------|
| H-1 | Add Linux ARM64 target | 2h | C-1 |
| H-2 | Add macOS ARM64 target | 1h | C-1 |
| H-3 | Enable pedantic clippy lints | 2-4h | None |
| H-4 | Add Dependabot config | 15min | None |
| H-5 | Add CodeQL analysis | 30min | None |
| H-6 | Add gitleaks scanning | 30min | None |
| H-7 | Create install script | 3h | C-2 (for release artifacts) |
| H-8 | Add .editorconfig + rustfmt.toml | 30min | None |

### Phase 3: Medium (Weeks 3-4)

| # | Task | Effort | Dependencies |
|---|------|--------|-------------|
| M-1 | Manual release trigger | 1h | C-2 |
| M-2 | Changelog generation | 2h | None |
| M-3 | Nightly builds | 2h | C-2 |
| M-4 | Issue templates | 1h | None |
| M-5 | PR template | 1h | None |
| M-6 | Enhanced deny.toml | 30min | None |

### Phase 4: Low (Future)

| # | Task | Effort | Dependencies |
|---|------|--------|-------------|
| L-1 | Docker support | 4h | C-2 |
| L-2 | Homebrew formula | 2h | C-2 |
| L-3 | Nix flake | 4h | None |
| L-4 | Discord notifications | 1h | C-2 |
| L-5 | SBOM generation | 1h | C-2 |

---

## 7. Key Configuration Files to Create/Modify

### New Files to Create

```
.github/workflows/release.yml         # Release build + publish
.github/workflows/codeql.yml          # CodeQL security analysis
.github/workflows/nightly.yml         # Nightly builds
.github/dependabot.yml                # Automated dependency updates
.github/ISSUE_TEMPLATE/bug_report.md  # Bug report template
.github/ISSUE_TEMPLATE/feature_request.md  # Feature request template
.github/pull_request_template.md      # PR template
.editorconfig                         # Editor configuration
rustfmt.toml                          # Rust formatting configuration
script/bump-version.sh                # Version bump script
script/install.sh                     # Install script
script/changelog.sh                   # Changelog generation script
```

### Files to Modify

```
.github/workflows/ci.yml              # Add Windows, ARM, concurrency, audit
deny.toml                             # Enhanced security policies
Cargo.toml                            # Workspace version management (by script)
src/main.rs                           # Gradual clippy pedantic enablement
```

---

## 8. Conclusion

RustCode's build and release infrastructure is in a very early state compared to OpenCode. While the existing CI pipeline covers the essential basics (format, lint, test, dependency check), it lacks:

1. **Windows support** — Critical for user adoption on the most popular desktop OS
2. **Release pipeline** — No way to ship binaries to users
3. **Install mechanism** — No install script or package manager support
4. **Security scanning** — Missing SAST (CodeQL) and secret scanning (gitleaks)
5. **Version management** — No automated version bumping or release tagging
6. **PR/issue automation** — No quality enforcement on contributions
7. **Infrastructure** — No Docker, Nix, or notification tooling

The recommended fixes prioritize Windows support and a basic release pipeline as the two most critical gaps. Until these are addressed, RustCode cannot be distributed to end users.

**Total estimated effort to reach parity**: ~3-4 weeks for core CI/CD (phases 1-3), with an additional 1-2 weeks for "nice to have" infrastructure (phase 4). The total to match OpenCode's full 26-workflow infrastructure is approximately 5-6 weeks of focused work.

---

## Appendix A: OpenCode vs RustCode Workflow Comparison Table

| Category | OpenCode Workflows | RustCode Workflows | Gap |
|----------|-------------------|-------------------|-----|
| CI (build/test/lint) | `test.yml`, `typecheck.yml` | `ci.yml` | Partial |
| Release | `publish.yml`, `beta.yml` | None | Full |
| Docker | `containers.yml` | None | Full |
| Deployment | `deploy.yml` | None | Full |
| Nix | `nix-eval.yml`, `nix-hashes.yml` | None | Full |
| Security | (none dedicated, implicit in PR) | (cargo-deny only) | Partial |
| PR management | `pr-standards.yml`, `pr-management.yml`, `compliance-close.yml`, `close-prs.yml` | None | Full |
| Issue management | `close-issues.yml`, `duplicate-issues.yml`, `triage.yml` | None | Full |
| Publishing | `publish-github-action.yml`, `publish-vscode.yml`, `release-github-action.yml`, `publish-python-sdk.yml` | None | Full |
| Docs | `docs-update.yml`, `docs-locale-sync.yml`, `storybook.yml` | None | Full |
| Code gen | `generate.yml` | None | Full |
| Stats | `stats.yml` | None | Full |
| Notification | `notify-discord.yml` | None | Full |
| AI-powered | `opencode.yml`, `review.yml` | None | Full |

## Appendix B: Key Features Not Present in RustCode

- No concurrency groups (cancel-in-progress)
- No workflow_dispatch (manual trigger)
- No scheduled workflows (cron)
- No path-based triggers
- No environment-based deployment
- No artifact upload/persist
- No Docker registry integration
- No code signing infrastructure
- No install script
- No package manager distribution
- No release notes generation
- No CHANGELOG management
- No SBOM or dependency graph
- No CodeQL or gitleaks
- No editor configuration (.editorconfig)
- No custom lint configuration (rustfmt.toml)
- No issue/PR templates
