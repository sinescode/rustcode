# Config System — Gap Analysis

## Config Structure Parity

Rust `Info` struct closely mirrors TS `ConfigV1.Info` at the top-level field level. **Full field parity** across:
- `AgentConfig` (14 fields)
- `ProviderConfig` (8 fields)
- `ProviderOptions` (8 fields)  
- `MCP Config` (Local: 6, Remote: 6)
- `CompactionConfig` (5 fields)
- `ExperimentalConfig` (7 fields)

## Missing Config Modules

| Module | TS Source | LOC | Status |
|--------|-----------|-----|--------|
| **TUI Config System** | `config/tui.ts` + `tui-cwd.ts` + `tui-host-attention.ts` + `tui-migrate.ts` | ~432 | **Entirely Missing** |
| **Markdown Frontmatter Parsing** | `config/markdown.ts` + `core/config/markdown.ts` | ~72 | **Missing** |
| **Remote Well-Known Config** | `config/config.ts:355-394` | ~75 | **Missing** |
| **Remote Auth/Console Config** | `config/config.ts:477-513` | ~36 | **Missing** |
| **NPM Dependency Installation** | `config/config.ts:437-456` | ~36 | **Missing** |
| **InstanceState Caching** | `config/config.ts:281-289` | ~15 | **Missing** |
| **Flag System** | `core/flag/flag.ts` | ~78 | **Partly missing** |
| **V2 Config Migrate** | `core/v1/config/migrate.ts` | ~258 | **Missing** |

## Config Format Support

| Format | TS | Rust | Status |
|--------|----|------|--------|
| JSON | ✅ jsonc-parser | ✅ serde_json | ✅ |
| JSONC (comments+trailing commas) | ✅ jsonc-parser | ✅ Custom `strip_jsonc_comments` | ⚠️ May differ |
| TOML (legacy) | ✅ | ✅ toml crate | ✅ |
| **YAML frontmatter (.md files)** | ✅ gray-matter | ❌ | **CRITICAL** |
| PLIST (macOS MDM) | ✅ plutil | ✅ plutil | ✅ |

## Config Path Resolution Gaps

- **Priority order REVERSED**: Rust loads `config.json` first, TS loads it last (highest priority)
- **Home `.opencode` directory** support missing in Rust
- **TUI config paths** (`tui.json`/`tui.jsonc`) entirely missing

## Variable Substitution Gaps

- `missing: "empty"` mode missing — Rust hard-errors on missing file refs where TS silently substitutes `""`

## 5 Most Critical Gaps

### 1. Markdown/YAML Frontmatter Parsing
Rust has discovery functions but **no markdown parsing**. Agents and commands defined as `.md` files are silently ignored.

**TS**: `config/agent.ts:19-30`, `config/markdown.ts:20-34`
**Rust**: `discover_agent_files:2475` — finds files but never parses

### 2. Remote Well-Known and Console Config
Auth-dependent config provisioning from well-known endpoints and console API.

**TS**: `config/config.ts:355-394`, `477-513`

### 3. TUI Config System (Entirely Missing)
No `tui.json`/`tui.jsonc` reading, no theme/keybind management, no attention sounds.

**TS**: `config/tui.ts` (274L), `tui-migrate.ts` (132L)

### 4. NPM Plugin Dependency Installation
`@opencode-ai/plugin` never auto-installed in `.opencode` directories.

**TS**: `config/config.ts:437-456`

### 5. Structured Validation & Agent Normalization
Rust uses plain `serde_json::from_value` — no structured error messages, no V1 backward compat (tools→permission, options extraction, maxSteps→steps).

**TS**: `config/parse.ts:55-71`, `v1/config/agent.ts:62-81`
