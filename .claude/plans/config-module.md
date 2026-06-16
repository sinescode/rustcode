# Config Module — Phase 1 Plan

## STEP 1 — Source Summary

Read 15 TS files:
- `packages/opencode/src/config/config.ts` — main Config service (684 lines, Effect.ts Layer)
- `packages/opencode/src/config/paths.ts` — config file path resolution
- `packages/opencode/src/config/parse.ts` — JSONC parsing + schema validation
- `packages/opencode/src/config/variable.ts` — {env:VAR}, {file:path} substitution
- `packages/opencode/src/config/agent.ts` — agent config from markdown files
- `packages/core/src/v1/config/config.ts` — ConfigV1.Info schema (40+ fields)
- `packages/core/src/v1/config/provider.ts` — Provider config schema
- `packages/core/src/v1/config/agent.ts` — Agent config schema
- `packages/core/src/v1/config/mcp.ts` — MCP config schema (Local + Remote)
- `packages/core/src/v1/config/permission.ts` — Permission config schema
- `packages/core/src/v1/config/plugin.ts` — Plugin spec schema
- `packages/core/src/v1/config/skills.ts` — Skills config schema
- `packages/core/src/config/provider.ts` — Provider v2 config
- `packages/core/src/config/experimental.ts` — Experimental config
- `packages/core/src/config/mcp.ts` — MCP v2 config

## STEP 2 — Interface Contract

### 2a. Public API Surface

| TS Function/Type | Location | Rust Equivalent |
|---|---|---|
| `Config.Interface.get()` | config.ts:125 | `Config::get() -> Result<Info>` |
| `Config.Interface.getGlobal()` | config.ts:126 | `Config::get_global() -> Result<Info>` |
| `Config.Interface.update(config)` | config.ts:128 | `Config::update(info: Info) -> Result<()>` |
| `Config.Interface.updateGlobal(config)` | config.ts:129 | `Config::update_global(info: Info) -> Result<UpdateResult>` |
| `Config.Interface.invalidate()` | config.ts:130 | `Config::invalidate() -> Result<()>` |
| `Config.Interface.directories()` | config.ts:131 | `Config::directories() -> Result<Vec<String>>` |
| `Config.Interface.getConsoleState()` | config.ts:127 | `Config::console_state() -> Result<ConsoleState>` |
| `ConfigPaths.files(name, dir, worktree)` | paths.ts:10 | `ConfigPaths::files(name, dir, worktree) -> Result<Vec<PathBuf>>` |
| `ConfigPaths.directories(dir, worktree)` | paths.ts:23 | `ConfigPaths::directories(dir, worktree) -> Result<Vec<PathBuf>>` |
| `ConfigParse.jsonc(text, filepath)` | parse.ts:8 | `ConfigParse::jsonc(text, filepath) -> Result<Value>` |
| `ConfigParse.schema(schema, data, source)` | parse.ts:35 | `ConfigParse::validate(data, source) -> Result<Info>` |
| `ConfigVariable.substitute(input)` | variable.ts:34 | `ConfigVariable::substitute(text, dir, env) -> Result<String>` |

### 2b. Config Schema Fields (ConfigV1.Info)

All optional unless noted:
- `$schema: Option<String>` — JSON schema URL
- `shell: Option<String>` — default shell
- `logLevel: Option<LogLevel>` — DEBUG|INFO|WARN|ERROR
- `server: Option<ServerConfig>` — HTTP server config
- `command: Option<HashMap<String, CommandConfig>>` — custom commands
- `skills: Option<SkillsConfig>` — additional skill paths
- `references: Option<ReferenceConfig>` — named git/local references
- `watcher: Option<WatcherConfig>` — file watcher ignore patterns
- `snapshot: Option<bool>` — enable/disable snapshots
- `plugin: Option<Vec<PluginSpec>>` — plugin specs
- `share: Option<ShareMode>` — manual|auto|disabled
- `autoshare: Option<bool>` — deprecated, use share
- `autoupdate: Option<AutoUpdate>` — bool or "notify"
- `disabled_providers: Option<Vec<String>>` — providers to disable
- `enabled_providers: Option<Vec<String>>` — only these providers
- `model: Option<String>` — default model (provider/model)
- `small_model: Option<String>` — small model for titles
- `default_agent: Option<String>` — default agent name
- `username: Option<String>` — custom username
- `mode: Option<HashMap<String, AgentConfig>>` — deprecated
- `agent: Option<AgentMap>` — agent configs (build, plan, general, explore, title, summary, compaction + custom)
- `provider: Option<HashMap<String, ProviderConfig>>` — provider configs
- `mcp: Option<HashMap<String, McpEntry>>` — MCP server configs
- `formatter: Option<FormatterConfig>` — formatter settings
- `lsp: Option<LspConfig>` — LSP settings
- `instructions: Option<Vec<String>>` — instruction files/patterns (merged via array concat)
- `layout: Option<LayoutConfig>` — deprecated
- `permission: Option<PermissionConfig>` — permission rules
- `tools: Option<HashMap<String, bool>>` — tool enable/disable
- `attachment: Option<AttachmentConfig>` — attachment processing
- `enterprise: Option<EnterpriseConfig>` — enterprise URL
- `tool_output: Option<ToolOutputConfig>` — output truncation thresholds
- `compaction: Option<CompactionConfig>` — compaction settings
- `experimental: Option<ExperimentalConfig>` — experimental flags

### 2d. Dependencies
- `error` crate (Error::Config variant, Error::Io)
- `serde` + `serde_json` for serialization
- `toml` for legacy config migration
- `dirs` for OS-standard paths
- `glob` for file discovery

### 2e. Error Conditions
- `ConfigNotFound` — no config file at path
- `ConfigParseError` — invalid JSON/JSONC
- `ConfigValidationError` — schema validation failure (unrecognized keys, type mismatch)
- `ConfigSubstitutionError` — bad {file:path} reference (ENOENT, permissions)
- `ConfigWriteError` — cannot write config file
- `Io` — general filesystem error

## STEP 3 — Rust Design

### 3a. File Layout
Single file expansion: `crates/rustcode-core/src/config.rs` — estimated 600-800 lines

### 3b. Key Types

```rust
/// Main config service — wraps the loaded Info + manages persistence.
pub struct Config {
    info: RwLock<Info>,
    global_info: RwLock<Info>,
    project_dir: PathBuf,
    worktree: Option<PathBuf>,
    config_paths: Vec<PathBuf>,
}

pub struct ConsoleState {
    pub console_managed_providers: Vec<String>,
    pub active_org_name: Option<String>,
    pub switchable_org_count: u32,
}

pub struct UpdateResult {
    pub info: Info,
    pub changed: bool,
}
```

### 3c. Required Crates
Already in workspace: `serde`, `serde_json`, `toml`, `dirs`, `glob`, `thiserror`, `tracing`

### 3d. Concurrency Model
- `Config` holds `RwLock<Info>` for interior mutability
- Config loading is sync (filesystem reads) — no tokio needed for core config
- Path traversal uses `glob` crate or manual walkdir logic

### 3e. Error Variants (add to existing error.rs or config-local)
```rust
Config(String),           // general config error
ConfigParse { path: String, message: String },
ConfigValidation { path: String, issues: Vec<String> },
ConfigSubstitution { path: String, message: String },
```

### 3f. No SQLite changes needed for config module

### 3g. Streaming — not applicable to config (file-based)

### 3h. Permission — not applicable (config is loaded before permission system)

### 3i. Testing Strategy
- `test_parse_jsonc_with_comments` — JSONC parsing
- `test_parse_jsonc_trailing_comma` — trailing comma support
- `test_parse_invalid_jsonc` — error on bad input
- `test_variable_substitution_env` — {env:VAR} replacement
- `test_variable_substitution_file` — {file:path} replacement
- `test_variable_substitution_missing_file_error` — error on missing file
- `test_config_merge_deep` — deep merge of nested objects
- `test_config_merge_instructions_concat` — array concatenation for instructions
- `test_config_load_defaults` — default config when no file exists
- `test_config_load_from_file` — full load from valid file
- `test_config_validation_unrecognized_keys` — reject unknown top-level keys
- `test_config_paths_resolution` — directory traversal for config files
- `test_config_update_and_persist` — round-trip update
- `test_mcp_config_local` — parse local MCP config
- `test_mcp_config_remote` — parse remote MCP config
- `test_provider_config_with_models` — provider config with model overrides
- `test_agent_config_normalization` — tools→permission normalization

## STEP 4 — Behavioral Parity Checklist
- [ ] Config loaded from global + project + .opencode dirs
- [ ] JSONC supports comments and trailing commas
- [ ] Schema validates and rejects unrecognized keys
- [ ] {env:VAR} substituted from process env
- [ ] {file:path} substituted from file contents
- [ ] Deep merge with array concatenation for instructions
- [ ] Legacy TOML config migration
- [ ] Default values applied when keys missing
- [ ] Config can be updated and persisted
- [ ] Global config invalidation

## STEP 5 — Readiness
✅ READY TO IMPLEMENT (depends on: error, id, env — all DONE)
