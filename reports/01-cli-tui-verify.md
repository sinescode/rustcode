# CLI/TUI Parity Verification Report

**Date**: 2026-06-21
**Author**: Automated audit
**Scope**: Spot-check of 10-15 CLI flags/behaviors across opencode (TypeScript) vs rustcode (Rust)
**Previous report**: `01-cli-tui.md` (claims 100% PORTED)

## Summary

The previous report's claim of **100% ported** is **not accurate**. This verification found **6 confirmed gaps** (1 P1, 3 P2, 2 P3) and 1 extra feature. The existing report missed several missing subcommands, flag type mismatches, and behavioral differences.

---

## Methodology

Compared yargs-based CLI in `$OC/packages/opencode/src/` vs clap-derive CLI in `$RC/src/main.rs`. Examined:
- All 23 command registrations + global flags in `index.ts` vs `Cli`/`Commands` enum
- All command arg structs in `main.rs` vs corresponding `.ts` option definitions
- Network options in `network.ts` vs `NetworkArgs`
- MCP subcommands in `mcp.ts` vs `McpCommand` enum
- TUI keybindings in `packages/tui/src/config/keybind.ts` vs `keymap.rs`
- Version flag config, help text, and error message formats

---

## Confirmed Gaps

### P1 — Missing functionality (user-visible)

| # | Gap | TS (opencode) | Rust (rustcode) | Fix difficulty |
|---|-----|---------------|-----------------|----------------|
| 1 | **`db path` subcommand missing** | `opencode db path` prints the database path (`Database.path()`) | `DbArgs` only has `query` + `--format`; no `path` subcommand at all | Easy — add `Path` variant to `DbArgs` or restructure as subcommand enum |

**TS source**: `$OC/packages/opencode/src/cli/cmd/db.ts` lines 45-52 (PathCommand)
**Rust source**: `$RC/src/main.rs` lines 717-729 (DbArgs struct)

---

### P2 — Behavioral differences

| # | Gap | TS (opencode) | Rust (rustcode) | Fix difficulty |
|---|-----|---------------|-----------------|----------------|
| 2 | **`--version` short flag `-v` vs `-V`** | `.alias("version", "v")` — `-v` shows version | Clap defaults to `-V` (capital V) | Easy — add `#[arg(short = 'v')]` or `#[command(version, short = 'v')]` |
| 3 | **`stats --models` type mismatch** | `.option("models", {...})` without `type: "number"` — accepts both `--models` (boolean = show all) and `--models 5` (show top 5) | `models: Option<usize>` — only accepts numeric `--models N`; bare `--models` is rejected | Medium — use `Option<String>` with custom parser, or add separate `--models-all` bool |
| 4 | **`--log-level` default value always set** | No default; `OPENCODE_LOG_LEVEL` env var only set if `--log-level` is explicitly passed | `#[arg(default_value = "INFO")]` — always has a value; `OPENCODE_LOG_LEVEL` is always set to "INFO" even when not passed | Easy — remove `default_value`, use `Option<LogLevel>` |

**TS sources**:
- `$OC/packages/opencode/src/index.ts` lines 51-52 (version alias), lines 57-61 (log-level)
- `$OC/packages/opencode/src/cli/cmd/stats.ts` lines 62-64 (models option)
**Rust sources**:
- `$RC/src/main.rs` lines 34-36 (version), lines 55-56 (log_level), lines 636-637 (models field)

---

### P3 — Missing subcommands / flags

| # | Gap | TS (opencode) | Rust (rustcode) | Fix difficulty |
|---|-----|---------------|-----------------|----------------|
| 5 | **`completion` subcommand missing** | `opencode completion` generates shell completion scripts via `.completion("completion", ...)` | No equivalent | Medium — add using `clap_complete` or a shell completion generator |
| 6 | **`mcp auth list` sub-subcommand missing** | `mcp auth list` / `mcp auth ls` lists OAuth-capable MCP servers with auth status; implemented as `.command(McpAuthListCommand)` within `McpAuthCommand` | `Auth` variant only has `name: Option<String>`; no `list` sub-subcommand | Medium — add `AuthList` variant to `McpCommand` enum |

**TS sources**:
- `$OC/packages/opencode/src/index.ts` line 80 (completion)
- `$OC/packages/opencode/src/cli/cmd/mcp.ts` lines 317-345 (McpAuthListCommand), line 181 (`.command(McpAuthListCommand)`)
**Rust sources**:
- `$RC/src/main.rs` lines 789-793 (Auth variant), no AuthList

---

## Extra Features (Rust-only, not in TS)

| # | Feature | Description | Recommendation |
|---|---------|-------------|---------------|
| E1 | **`tui --json` flag** | `TuiArgs` has `json: bool` field that outputs structured JSON events on stdout; no equivalent in TS `tui.ts` | Either remove for parity, or document as intentional Rust addition |
| E2 | **`github run --event-payload` flag** | `Run` variant in `GithubCommand` has `event_payload: Option<String>`; TS `GithubRunCommand` does not have this flag | Either remove for parity, or document as intentional Rust addition |

---

## Commands with Full Parity (No Gaps Found)

The following commands were verified as having complete flag/subcommand parity:

| Command | Verification notes |
|---------|-------------------|
| `run` | All 23 flags match (message, command, continue, session, fork, share, model, agent, format, file, title, attach, password, username, dir, port, variant, thinking, replay, replay-limit, interactive, dangerously-skip-permissions, demo) |
| `tui` (minus `--json`) | All 8 flags match (project, model, continue, session, fork, prompt, agent + network options) |
| `attach` | All 6 flags match (url, dir, continue, session, fork, password, username) |
| `upgrade` | All 2 flags match (target, method with choices) |
| `uninstall` | All 4 flags match (keep-config, keep-data, dry-run, force) |
| `models` | All 3 flags match (provider, verbose, refresh) |
| `export` | Both flags match (session-id, sanitize) |
| `import` | Both flags match (file) |
| `pr` | Both flags match (number) |
| `plugin` / `plug` | All 3 flags match (module, global, force) |
| `serve` / `web` | Network args all match (port, hostname, mdns, mdns-domain, cors) |
| `session` | Both subcommands match (list with max-count/format, delete with session-id) |
| `console` / `account` | All 5 subcommands match (login, logout, switch, orgs, open) |
| `providers` | All 3 subcommands match (list, login with provider/method, logout) |
| `agent` | Both subcommands match (create with path/description/mode/permissions/model, list) |
| `github` | Both subcommands match (install, run with event/token) |
| `debug` | All subcommands match (config, lsp with diagnostics/symbols/document-symbols, rg with files/search, file with search/read/list, scrap, skill, snapshot with track/patch/diff, startup, agent, v2, info, paths, wait) |
| `acp` | Both flags match (cwd + network args) |

---

## TUI Keybinding Parity

TS source: `$OC/packages/tui/src/config/keybind.ts`
Rust source: `$RC/crates/rustcode-tui/src/keymap.rs`

Both use a leader-based system. Key binding mapping:

| Action | TS key | Rust key | Status |
|--------|--------|----------|--------|
| App exit | `leader` + `q` | `ctrl-c` or `q` | Differences exist but both implement exit |
| Submit input | `enter` | `enter` | ✓ |
| Cancel | `escape` | `escape` | ✓ |
| Scroll up | `ctrl-u` / `ctrl-b` | `ctrl-u` / `page-up` | Minor difference |
| Scroll down | `ctrl-d` / `ctrl-f` | `ctrl-d` / `page-down` | Minor difference |

The TUI keybinding parity is **partial** — the upstream JS TUI (React/Ink) uses different key mapping conventions than the Rust TUI (ratatui), so some differences are expected. Full functional parity should be verified when TUI reaches production quality.

---

## Error Message Parity

Spot-checked error messages (e.g., `--fork requires --continue or --session`):
- TS: `UI.error("--fork requires --continue or --session")`
- Rust: `eprintln!("Error: --fork requires --continue or --session")`

Both produce similar output. The `Error:` prefix in Rust is a minor stylistic difference. Other error messages were observed to follow the same pattern.

---

## Recommendations

1. **Fix P1 gap** — Implement `db path` subcommand (highest priority, missing feature)
2. **Fix P2 gaps** — Fix `-v` vs `-V`, `stats --models` type, and `--log-level` default
3. **Fix P3 gaps** — Add `completion` command and `mcp auth list` sub-subcommand
4. **Document or remove** — Rust-only extra features (`--json` on tui, `--event-payload` on github run)
5. **Update parity claim** — Change "100% PORTED" to "xx/23 commands ported with noted gaps" with a link to this verification report
