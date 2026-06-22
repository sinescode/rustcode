//! Configuration system — types, loading, merging, and persistence.
//!
//! Ported from: `packages/blazecode/src/config/config.ts`
//! BlazeCode commit: 5d0f86606ac30690f79f0a6a9f41a1f49fe95d0b
//!
//! ## Architecture
//!
//! The TS source uses Effect.ts `Layer` + `InstanceState` for config loading,
//! merging multiple sources: global config, project config files (JSON/JSONC),
//! `.blazecode` directories, well-known URLs, environment variables, and managed
//! preferences.
//!
//! In Rust, config loading is synchronous (filesystem reads). The [`Config`]
//! struct wraps an [`Info`] behind [`RwLock`] and provides the same `get` /
//! `update` / `invalidate` interface.
//!
//! ## Config Sources (in priority order, lowest first)
//!
//! 1. Global config — `~/.config/blazecode/blazecode.jsonc`
//! 2. Remote well-known org config (fetched via auth service — future)
//! 3. Project config — `blazecode.jsonc` / `blazecode.json` walking up from cwd
//! 4. `.blazecode/` directory configs
//! 5. Environment variable overrides (`BLAZECODE_CONFIG`, `BLAZECODE_CONFIG_CONTENT`)
//! 6. Managed preferences (macOS MDM — future)
//!
//! Merging is deep: nested objects are merged recursively, and the `instructions`
//! array is concatenated (deduplicated) rather than replaced.

use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::RwLock;

// ── Top-level config service ────────────────────────────────────────────

/// Main configuration service.
///
/// Wraps the loaded configuration info and manages persistence.
/// Thread-safe via interior [`RwLock`].
///
/// # Source
/// Ported from `packages/blazecode/src/config/config.ts` lines 117–136
/// (`State` type + `Service` context).
#[derive(Debug)]
pub struct Config {
    /// The merged configuration info
    info: RwLock<Info>,
    /// Config file directories for this instance
    directories: Vec<PathBuf>,
    /// Project root directory
    project_dir: PathBuf,
    /// Worktree boundary (stop walking up past this)
    worktree: Option<PathBuf>,
}

/// Result of updating global config.
#[derive(Debug, Clone)]
pub struct UpdateResult {
    /// The merged config after the update
    pub info: Info,
    /// Whether the config actually changed
    pub changed: bool,
}

/// Console state (subset for UI display).
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct ConsoleState {
    /// Providers managed by the console
    #[serde(default)]
    pub console_managed_providers: Vec<String>,
    /// Active organization name
    #[serde(skip_serializing_if = "Option::is_none")]
    pub active_org_name: Option<String>,
    /// Number of switchable orgs
    #[serde(default)]
    pub switchable_org_count: u32,
}

// ── Main configuration info (ConfigV1.Info) ─────────────────────────────

/// Complete configuration schema.
///
/// All fields are optional — absent keys inherit defaults.
///
/// # Source
/// Ported from `packages/core/src/v1/config/config.ts` lines 32–189
/// (`ConfigV1.Info` schema).
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct Info {
    /// JSON schema reference for editor completion
    #[serde(skip_serializing_if = "Option::is_none", alias = "$schema")]
    pub schema: Option<String>,

    /// Default shell for terminal and bash tool
    #[serde(skip_serializing_if = "Option::is_none")]
    pub shell: Option<String>,

    /// Log level: DEBUG, INFO, WARN, ERROR
    #[serde(skip_serializing_if = "Option::is_none", rename = "logLevel")]
    pub log_level: Option<LogLevel>,

    /// HTTP server configuration
    #[serde(skip_serializing_if = "Option::is_none")]
    pub server: Option<ServerConfig>,

    /// Custom command definitions
    #[serde(default, skip_serializing_if = "HashMap::is_empty")]
    pub command: HashMap<String, CommandConfig>,

    /// Additional skill folder paths and URLs
    #[serde(skip_serializing_if = "Option::is_none")]
    pub skills: Option<SkillsConfig>,

    /// Named git or local directory references
    #[serde(default, skip_serializing_if = "HashMap::is_empty")]
    pub references: HashMap<String, ReferenceEntry>,

    /// @deprecated — use `references` instead
    #[serde(default, skip_serializing_if = "HashMap::is_empty")]
    pub reference: HashMap<String, ReferenceEntry>,

    /// File watcher ignore patterns
    #[serde(skip_serializing_if = "Option::is_none")]
    pub watcher: Option<WatcherConfig>,

    /// Enable or disable filesystem snapshots
    #[serde(skip_serializing_if = "Option::is_none")]
    pub snapshot: Option<bool>,

    /// Plugin specifications
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub plugin: Vec<PluginSpec>,

    /// Share behavior: manual, auto, or disabled
    #[serde(skip_serializing_if = "Option::is_none")]
    pub share: Option<ShareMode>,

    /// @deprecated — use `share` instead
    #[serde(skip_serializing_if = "Option::is_none")]
    pub autoshare: Option<bool>,

    /// Auto-update setting: true, false, or "notify"
    #[serde(skip_serializing_if = "Option::is_none")]
    pub autoupdate: Option<AutoUpdate>,

    /// Providers to disable
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub disabled_providers: Vec<String>,

    /// When set, ONLY these providers are enabled
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub enabled_providers: Vec<String>,

    /// Default model in `provider/model` format
    #[serde(skip_serializing_if = "Option::is_none")]
    pub model: Option<String>,

    /// Small model for tasks like title generation
    #[serde(skip_serializing_if = "Option::is_none")]
    pub small_model: Option<String>,

    /// Default agent name (must be a primary agent)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub default_agent: Option<String>,

    /// Custom username to display
    #[serde(skip_serializing_if = "Option::is_none")]
    pub username: Option<String>,

    /// @deprecated — use `agent` instead
    #[serde(default, skip_serializing_if = "HashMap::is_empty")]
    pub mode: HashMap<String, AgentConfig>,

    /// Agent configurations (build, plan, general, explore, title, summary,
    /// compaction, plus custom)
    #[serde(default, skip_serializing_if = "HashMap::is_empty")]
    pub agent: HashMap<String, AgentConfig>,

    /// Provider configurations and model overrides
    #[serde(default, skip_serializing_if = "HashMap::is_empty")]
    pub provider: HashMap<String, ProviderConfig>,

    /// MCP server configurations
    #[serde(default, skip_serializing_if = "HashMap::is_empty")]
    pub mcp: HashMap<String, McpEntry>,

    /// Formatter configuration
    #[serde(skip_serializing_if = "Option::is_none")]
    pub formatter: Option<FormatterConfig>,

    /// LSP configuration
    #[serde(skip_serializing_if = "Option::is_none")]
    pub lsp: Option<LspConfig>,

    /// Additional instruction files or patterns to include.
    ///
    /// **Merge behavior**: arrays from multiple sources are concatenated and
    /// deduplicated, not replaced.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub instructions: Vec<String>,

    /// @deprecated — always uses stretch layout
    #[serde(skip_serializing_if = "Option::is_none")]
    pub layout: Option<serde_json::Value>,

    /// Permission rules
    #[serde(skip_serializing_if = "Option::is_none")]
    pub permission: Option<PermissionConfig>,

    /// Tool enable/disable shortcuts
    #[serde(default, skip_serializing_if = "HashMap::is_empty")]
    pub tools: HashMap<String, bool>,

    /// Attachment processing configuration
    #[serde(skip_serializing_if = "Option::is_none")]
    pub attachment: Option<AttachmentConfig>,

    /// Enterprise URL
    #[serde(skip_serializing_if = "Option::is_none")]
    pub enterprise: Option<EnterpriseConfig>,

    /// Tool output truncation thresholds
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tool_output: Option<ToolOutputConfig>,

    /// Compaction settings
    #[serde(skip_serializing_if = "Option::is_none")]
    pub compaction: Option<CompactionConfig>,

    /// Experimental flags
    #[serde(skip_serializing_if = "Option::is_none")]
    pub experimental: Option<ExperimentalConfig>,

    /// Derived plugin provenance — not persisted, not serialized.
    /// Keeps each winning plugin spec together with the file and scope it came
    /// from so that downstream runtime code can make location-sensitive decisions.
    /// Populated during config loading, not read from files.
    #[serde(skip)]
    pub plugin_origins: Vec<PluginOrigin>,
}

// ── TUI config (tui.json / tui.jsonc) ───────────────────────────────────

/// TUI configuration — theme, keybinds, display settings.
///
/// Ported from: `packages/tui/src/config/config.ts` — `TuiConfig.Info`.
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct TuiConfigInfo {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub theme: Option<String>,
    #[serde(default, skip_serializing_if = "HashMap::is_empty")]
    pub keybinds: HashMap<String, String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub colors: Option<HashMap<String, String>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub font_size: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub font_family: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub show_status_bar: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub show_minimap: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub cursor_style: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub scrollbar: Option<ScrollbarConfig>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub host_attention: Option<HostAttentionConfig>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub plugin: Vec<PluginSpec>,
    #[serde(flatten)]
    pub extra: HashMap<String, serde_json::Value>,
}

/// Scrollbar display configuration.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ScrollbarConfig {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub visible: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub style: Option<String>,
}

/// Host attention notification configuration.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HostAttentionConfig {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub sounds: Option<Vec<String>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub flash: Option<bool>,
}

// ── V2 config schema (migration target) ─────────────────────────────────

/// V2 configuration info — the target format for V1→V2 migration.
///
/// Ported from: `packages/core/src/v1/config/migrate.ts` lines 36–73.
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct V2ConfigInfo {
    #[serde(skip_serializing_if = "Option::is_none", alias = "$schema")]
    pub schema: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub shell: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub model: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub default_agent: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub autoupdate: Option<AutoUpdate>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub share: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub enterprise: Option<EnterpriseConfig>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub username: Option<String>,
    #[serde(default)]
    pub permissions: Vec<V2PermissionRule>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub agents: Option<HashMap<String, V2AgentEntry>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub snapshots: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub watcher: Option<WatcherConfig>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub formatter: Option<FormatterConfig>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub lsp: Option<LspConfig>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub attachments: Option<AttachmentConfig>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tool_output: Option<ToolOutputConfig>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub mcp: Option<V2McpConfig>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub compaction: Option<V2CompactionConfig>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub skills: Option<Vec<String>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub commands: Option<HashMap<String, V2CommandEntry>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub instructions: Option<Vec<String>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub references: Option<HashMap<String, String>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub plugins: Option<Vec<String>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub experimental: Option<V2ExperimentalConfig>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub providers: Option<HashMap<String, V2ProviderEntry>>,
}

/// V2 permission rule.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct V2PermissionRule {
    pub action: String,
    pub resource: String,
    pub effect: String,
}

/// V2 agent entry.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct V2AgentEntry {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub model: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub variant: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub mode: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub hidden: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub color: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub steps: Option<u32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub disabled: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub system: Option<String>,
}

/// V2 MCP config.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct V2McpConfig {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub timeout: Option<u64>,
    pub servers: HashMap<String, V2McpServer>,
}

/// V2 MCP server.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct V2McpServer {
    pub r#type: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub command: Option<Vec<String>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub cwd: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub environment: Option<HashMap<String, String>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub disabled: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub timeout: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub url: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub headers: Option<HashMap<String, String>>,
}

/// V2 compaction config.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct V2CompactionConfig {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub auto: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub prune: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub keep_tokens: Option<u32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub buffer: Option<u32>,
}

/// V2 command entry.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct V2CommandEntry {
    pub template: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub agent: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub model: Option<String>,
}

/// V2 experimental config.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct V2ExperimentalConfig {
    #[serde(default)]
    pub policies: Vec<serde_json::Value>,
}

/// V2 provider entry.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct V2ProviderEntry {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub env: Vec<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub api: Option<V2ApiEntry>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub models: Option<HashMap<String, V2ModelEntry>>,
}

/// V2 API entry.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct V2ApiEntry {
    pub r#type: String,
    #[serde(rename = "package")]
    pub package_name: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub url: Option<String>,
}

/// V2 model entry.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct V2ModelEntry {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub family: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub id: Option<String>,
}

// ── Plugin origin tracking ──────────────────────────────────────────────

/// Plugin origin metadata — tracks where a plugin spec was declared.
///
/// After multiple config files are merged, callers still need to know which
/// config file declared the plugin and whether it should behave like a
/// global or project-local plugin.
///
/// # Source
/// Ported from `packages/blazecode/src/config/plugin.ts` — `Origin` type.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PluginOrigin {
    /// The plugin specifier (npm package name or file URL).
    pub spec: PluginSpec,
    /// Config file path that declared this plugin.
    pub source: String,
    /// Whether this plugin is global or project-local.
    pub scope: PluginScope,
}

/// Plugin scope — global or local.
///
/// # Source
/// Ported from `packages/blazecode/src/config/plugin.ts` — `Scope` type.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum PluginScope {
    Global,
    Local,
}

// ── Enums ────────────────────────────────────────────────────────────────

/// Log level.
///
/// # Source
/// Ported from `packages/core/src/v1/config/config.ts` line 27.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "UPPERCASE")]
pub enum LogLevel {
    Debug,
    Info,
    Warn,
    Error,
}

/// Share mode for sessions.
///
/// # Source
/// Ported from `packages/core/src/v1/config/config.ts` line 56.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum ShareMode {
    Manual,
    Auto,
    Disabled,
}

/// Auto-update behavior.
///
/// # Source
/// Ported from `packages/core/src/v1/config/config.ts` line 64.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(untagged)]
pub enum AutoUpdate {
    Bool(bool),
    Notify(String),
}

/// MCP server configuration entry — either a full config or just enabled/disabled.
///
/// # Source
/// Ported from `packages/core/src/v1/config/config.ts` line 111.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(untagged)]
pub enum McpEntry {
    Full(McpConfig),
    Toggle { enabled: bool },
}

/// Plugin specification — either a plain string or a [string, options] tuple.
///
/// # Source
/// Ported from `packages/core/src/v1/config/plugin.ts` line 8.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(untagged)]
pub enum PluginSpec {
    Simple(String),
    WithOptions(String, HashMap<String, serde_json::Value>),
}

// ── Sub-config structs ──────────────────────────────────────────────────

/// HTTP server configuration.
///
/// # Source
/// Ported from `packages/core/src/v1/config/server.ts` lines 6–18.
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct ServerConfig {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub port: Option<u16>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub hostname: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub mdns: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub mdns_domain: Option<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub cors: Vec<String>,
}

/// Custom command definition.
///
/// # Source
/// Ported from `packages/core/src/v1/config/command.ts` lines 5–12.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CommandConfig {
    pub template: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub agent: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub model: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub variant: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub subtask: Option<bool>,
}

/// Skills configuration.
///
/// # Source
/// Ported from `packages/core/src/v1/config/skills.ts` lines 5–12.
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct SkillsConfig {
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub paths: Vec<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub urls: Vec<String>,
}

/// Reference entry — git repository, local directory, or plain path string.
///
/// # Source
/// Ported from `packages/core/src/config/reference.ts` lines 5–19.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(untagged)]
pub enum ReferenceEntry {
    Simple(String),
    Git {
        repository: String,
        #[serde(skip_serializing_if = "Option::is_none")]
        branch: Option<String>,
        #[serde(skip_serializing_if = "Option::is_none")]
        description: Option<String>,
        #[serde(skip_serializing_if = "Option::is_none")]
        hidden: Option<bool>,
    },
    Local {
        path: String,
        #[serde(skip_serializing_if = "Option::is_none")]
        description: Option<String>,
        #[serde(skip_serializing_if = "Option::is_none")]
        hidden: Option<bool>,
    },
}

/// File watcher configuration.
///
/// # Source
/// Ported from `packages/core/src/v1/config/config.ts` line 51.
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct WatcherConfig {
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub ignore: Vec<String>,
}

/// Provider configuration.
///
/// # Source
/// Ported from `packages/core/src/v1/config/provider.ts` lines 76–121.
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct ProviderConfig {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub env: Vec<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub api: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub npm: Option<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub whitelist: Vec<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub blacklist: Vec<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub options: Option<ProviderOptions>,
    #[serde(default, skip_serializing_if = "HashMap::is_empty")]
    pub models: HashMap<String, ModelConfig>,
}

/// Provider-specific options.
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct ProviderOptions {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub api_key: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub base_url: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub enterprise_url: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub set_cache_key: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub timeout: Option<TimeoutValue>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub header_timeout: Option<TimeoutValue>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub chunk_timeout: Option<u64>,
    /// Catch-all for provider-specific options
    #[serde(flatten)]
    pub extra: HashMap<String, serde_json::Value>,
}

/// Timeout value — either a positive integer (milliseconds) or `false` (disabled).
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(untagged)]
pub enum TimeoutValue {
    Millis(u64),
    Disabled(bool),
}

/// Model configuration override within a provider.
///
/// # Source
/// Ported from `packages/core/src/v1/config/provider.ts` lines 8–74.
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct ModelConfig {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub family: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub release_date: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub attachment: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub reasoning: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub temperature: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tool_call: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub interleaved: Option<serde_json::Value>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub cost: Option<ModelCost>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub limit: Option<ModelLimit>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub modalities: Option<ModelModalities>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub experimental: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub status: Option<ModelStatus>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub provider: Option<ModelProviderRef>,
    #[serde(default, skip_serializing_if = "HashMap::is_empty")]
    pub options: HashMap<String, serde_json::Value>,
    #[serde(default, skip_serializing_if = "HashMap::is_empty")]
    pub headers: HashMap<String, String>,
    #[serde(default, skip_serializing_if = "HashMap::is_empty")]
    pub variants: HashMap<String, serde_json::Value>,
}

/// Model cost structure.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ModelCost {
    pub input: f64,
    pub output: f64,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub cache_read: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub cache_write: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub context_over_200k: Option<Box<ContextOver200kCost>>,
}

/// Cost for context windows over 200k tokens.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ContextOver200kCost {
    pub input: f64,
    pub output: f64,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub cache_read: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub cache_write: Option<f64>,
}

/// Model context/input/output limits.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ModelLimit {
    pub context: f64,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub input: Option<f64>,
    pub output: f64,
}

/// Model input/output modalities.
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct ModelModalities {
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub input: Vec<Modality>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub output: Vec<Modality>,
}

/// A content modality.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Modality {
    Text,
    Audio,
    Image,
    Video,
    Pdf,
}

/// Model status.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum ModelStatus {
    Alpha,
    Beta,
    Deprecated,
    Active,
}

/// Model provider reference (for cross-provider model config).
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct ModelProviderRef {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub npm: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub api: Option<String>,
}

/// Agent configuration.
///
/// # Source
/// Ported from `packages/core/src/v1/config/agent.ts` lines 12–41.
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct AgentConfig {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub model: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub variant: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub temperature: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub top_p: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub prompt: Option<String>,
    /// @deprecated — use `permission` instead
    #[serde(default, skip_serializing_if = "HashMap::is_empty")]
    pub tools: HashMap<String, bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub disable: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub mode: Option<AgentMode>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub hidden: Option<bool>,
    #[serde(default, skip_serializing_if = "HashMap::is_empty")]
    pub options: HashMap<String, serde_json::Value>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub color: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub steps: Option<u32>,
    /// @deprecated — use `steps` instead
    #[serde(skip_serializing_if = "Option::is_none")]
    pub max_steps: Option<u32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub permission: Option<PermissionConfig>,
}

/// Agent mode.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum AgentMode {
    Subagent,
    Primary,
    All,
}

/// MCP server configuration.
///
/// # Source
/// Ported from `packages/core/src/v1/config/mcp.ts` lines 6–63.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type")]
pub enum McpConfig {
    #[serde(rename = "local")]
    Local {
        /// Command and arguments
        command: Vec<String>,
        /// Working directory
        #[serde(skip_serializing_if = "Option::is_none")]
        cwd: Option<String>,
        /// Environment variables
        #[serde(default, skip_serializing_if = "HashMap::is_empty")]
        environment: HashMap<String, String>,
        /// Enable or disable
        #[serde(skip_serializing_if = "Option::is_none")]
        enabled: Option<bool>,
        /// Request timeout in milliseconds
        #[serde(skip_serializing_if = "Option::is_none")]
        timeout: Option<u64>,
    },
    #[serde(rename = "remote")]
    Remote {
        /// Server URL
        url: String,
        /// Enable or disable
        #[serde(skip_serializing_if = "Option::is_none")]
        enabled: Option<bool>,
        /// Request headers
        #[serde(default, skip_serializing_if = "HashMap::is_empty")]
        headers: HashMap<String, String>,
        /// OAuth configuration
        #[serde(skip_serializing_if = "Option::is_none")]
        oauth: Option<McpOAuth>,
        /// Request timeout in milliseconds
        #[serde(skip_serializing_if = "Option::is_none")]
        timeout: Option<u64>,
    },
}

/// MCP OAuth configuration.
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct McpOAuth {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub client_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub client_secret: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub scope: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub callback_port: Option<u16>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub redirect_uri: Option<String>,
}

/// Permission configuration.
///
/// # Source
/// Ported from `packages/core/src/v1/config/permission.ts` lines 5–49.
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub struct PermissionConfig {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub read: Option<PermissionRule>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub edit: Option<PermissionRule>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub glob: Option<PermissionRule>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub grep: Option<PermissionRule>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub list: Option<PermissionRule>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub bash: Option<PermissionRule>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub task: Option<PermissionRule>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub external_directory: Option<PermissionRule>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub todowrite: Option<PermissionAction>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub question: Option<PermissionAction>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub webfetch: Option<PermissionAction>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub websearch: Option<PermissionAction>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub lsp: Option<PermissionRule>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub doom_loop: Option<PermissionAction>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub skill: Option<PermissionRule>,
    /// Catch-all or wildcard entry (*)
    #[serde(skip_serializing_if = "Option::is_none")]
    #[serde(rename = "*")]
    pub wildcard: Option<PermissionAction>,
    /// Additional tool-specific rules
    #[serde(flatten)]
    pub extra: HashMap<String, PermissionRule>,
}

/// Permission action.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum PermissionAction {
    Ask,
    Allow,
    Deny,
}

/// Permission rule — either a simple action or an object mapping patterns to actions.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(untagged)]
pub enum PermissionRule {
    Action(PermissionAction),
    Object(HashMap<String, PermissionAction>),
}

/// Formatter configuration.
///
/// # Source
/// Ported from `packages/core/src/v1/config/formatter.ts` lines 5–12.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(untagged)]
pub enum FormatterConfig {
    Bool(bool),
    Map(HashMap<String, FormatterEntry>),
}

/// Formatter entry for a language.
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct FormatterEntry {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub disabled: Option<bool>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub command: Vec<String>,
    #[serde(default, skip_serializing_if = "HashMap::is_empty")]
    pub environment: HashMap<String, String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub extensions: Vec<String>,
}

/// LSP configuration — boolean or per-server map.
///
/// # Source
/// Ported from `packages/core/src/v1/config/lsp.ts` lines 9–17, 76.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(untagged)]
pub enum LspConfig {
    Bool(bool),
    Map(HashMap<String, LspEntry>),
}

/// LSP server entry.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(untagged)]
pub enum LspEntry {
    Disabled {
        disabled: bool,
    },
    Config {
        command: Vec<String>,
        #[serde(default, skip_serializing_if = "Vec::is_empty")]
        extensions: Vec<String>,
        #[serde(skip_serializing_if = "Option::is_none")]
        disabled: Option<bool>,
        #[serde(default, skip_serializing_if = "HashMap::is_empty")]
        env: HashMap<String, String>,
        #[serde(default, skip_serializing_if = "HashMap::is_empty")]
        initialization: HashMap<String, serde_json::Value>,
    },
}

/// Attachment processing configuration.
///
/// # Source
/// Ported from `packages/core/src/v1/config/attachment.ts` lines 6–24.
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct AttachmentConfig {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub image: Option<ImageAttachmentConfig>,
}

/// Image attachment settings.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ImageAttachmentConfig {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub auto_resize: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub max_width: Option<u32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub max_height: Option<u32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub max_base64_bytes: Option<u64>,
}

/// Enterprise configuration.
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct EnterpriseConfig {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub url: Option<String>,
}

/// Tool output truncation thresholds.
///
/// # Source
/// Ported from `packages/core/src/v1/config/config.ts` lines 132–145.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ToolOutputConfig {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub max_lines: Option<u32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub max_bytes: Option<u64>,
}

/// Compaction configuration.
///
/// # Source
/// Ported from `packages/core/src/v1/config/config.ts` lines 146–165.
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct CompactionConfig {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub auto: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub prune: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tail_turns: Option<u32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub preserve_recent_tokens: Option<u32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub reserved: Option<u32>,
}

/// Experimental configuration flags.
///
/// # Source
/// Ported from `packages/core/src/v1/config/config.ts` lines 166–186.
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct ExperimentalConfig {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub disable_paste_summary: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub batch_tool: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub open_telemetry: Option<bool>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub primary_tools: Vec<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub continue_loop_on_deny: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub mcp_timeout: Option<u64>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub policies: Vec<serde_json::Value>,
}

// ── Config implementation ───────────────────────────────────────────────

impl Config {
    /// Create a new `Config` for the given project directory.
    #[must_use]
    pub fn new(project_dir: PathBuf, worktree: Option<PathBuf>) -> Self {
        Self {
            info: RwLock::new(Info::default()),
            directories: Vec::new(),
            project_dir,
            worktree,
        }
    }

    /// Get the current merged configuration info.
    ///
    /// Returns default config if the lock is poisoned (recovery, not panic).
    ///
    /// # Source
    /// Ported from `packages/blazecode/src/config/config.ts` line 605
    /// (`Config.get()`).
    pub fn get(&self) -> Info {
        self.info.read()
            .unwrap_or_else(|poisoned| {
                tracing::warn!("Config lock poisoned, recovering with default");
                poisoned.into_inner()
            })
            .clone()
    }

    /// Load configuration from the global config directory.
    ///
    /// Reads `blazecode.jsonc`, `blazecode.json`, and `config.json`
    /// from the OS config directory (`~/.config/blazecode/` on Linux).
    ///
    /// # Source
    /// Ported from `packages/blazecode/src/config/config.ts` lines 246–279
    /// (`loadGlobal`).
    pub fn load_global() -> crate::error::Result<Info> {
        let config_dir = global_config_dir()?;
        let mut info = Info::default();

        // Try each candidate file in order
        for filename in &["config.json", "blazecode.json", "blazecode.jsonc"] {
            let path = config_dir.join(filename);
            if path.exists() {
                if let Ok(text) = std::fs::read_to_string(&path) {
                    if let Ok(parsed) = parse_jsonc(&text, &path) {
                        if let Ok(loaded) = validate_info(parsed, &path) {
                            merge_info(&mut info, &loaded);
                        }
                    }
                }
            }
        }

        // Legacy TOML config migration
        let legacy = config_dir.join("config");
        if legacy.exists() {
            if let Ok(content) = std::fs::read_to_string(&legacy) {
                if let Ok(toml_info) = migrate_legacy_toml(&content) {
                    merge_info(&mut info, &toml_info);
                    // Write migrated config as JSON and remove legacy
                    let json_path = config_dir.join("config.json");
                    if let Ok(json) = serde_json::to_string_pretty(&info) {
                        let _ = std::fs::write(&json_path, json);
                        let _ = std::fs::remove_file(&legacy);
                    }
                }
            }
        }

        // Seed schema if no config file exists yet
        let _ = seed_global_config_schema();

        // Post-processing
        normalize_config(&mut info);

        Ok(info)
    }

    /// Load configuration from the default location.
    ///
    /// Loads global config, then applies env var overrides
    /// (`BLAZECODE_CONFIG_CONTENT`, `BLAZECODE_CONFIG_DIR`, `BLAZECODE_CONFIG`),
    /// managed config, and CLI flags.
    ///
    /// # Source
    /// Ported from `packages/blazecode/src/config/config.ts` — `loadInstanceState`.
    ///
    /// # Errors
    /// Returns an error if the config directory cannot be determined
    /// or the config file cannot be read or parsed.
    pub fn load() -> crate::error::Result<Info> {
        let mut info = Self::load_global()?;

        // BLAZECODE_CONFIG env var — specific config file path
        if let Ok(config_path) = std::env::var("BLAZECODE_CONFIG") {
            let path = std::path::Path::new(&config_path);
            if let Ok(loaded) = Self::load_from_file(path) {
                merge_info(&mut info, &loaded);
            }
        }

        // Project config files (walk up from cwd)
        if std::env::var("BLAZECODE_DISABLE_PROJECT_CONFIG").is_err() {
            if let Ok(cwd) = std::env::current_dir() {
                if let Ok(files) = discover_config_files("blazecode", &cwd, None) {
                    for file in files {
                        if let Ok(loaded) = Self::load_from_file(&file) {
                            merge_info(&mut info, &loaded);
                        }
                    }
                }
                // .blazecode directories
                if let Ok(dirs) = discover_blazecode_dirs(&cwd, None) {
                    for dir in dirs {
                        for name in &["blazecode.json", "blazecode.jsonc"] {
                            let path = dir.join(name);
                            if let Ok(loaded) = Self::load_from_file(&path) {
                                merge_info(&mut info, &loaded);
                            }
                        }
                    }
                }
                // BLAZECODE_CONFIG_DIR — treated like an additional .blazecode directory
                if let Ok(config_dir) = std::env::var("BLAZECODE_CONFIG_DIR") {
                    let config_dir_path = std::path::PathBuf::from(&config_dir);
                    for name in &["blazecode.json", "blazecode.jsonc"] {
                        let path = config_dir_path.join(name);
                        if let Ok(loaded) = Self::load_from_file(&path) {
                            merge_info(&mut info, &loaded);
                        }
                    }
                }
            }
        }

        // Managed config (system-wide + macOS MDM)
        if let Ok(managed) = Self::load_managed() {
            merge_info(&mut info, &managed);
        }

        // BLAZECODE_CONFIG_CONTENT env var
        if let Ok(Some(from_env)) = Self::load_from_env() {
            merge_info(&mut info, &from_env);
        }

        // BLAZECODE_PERMISSION env var — JSON permission override
        if let Ok(perm_json) = std::env::var("BLAZECODE_PERMISSION") {
            if let Ok(perm_value) = serde_json::from_str::<PermissionConfig>(&perm_json) {
                let mut perm_info = Info::default();
                perm_info.permission = Some(perm_value);
                merge_info(&mut info, &perm_info);
            }
        }

        // Disable autocompact from CLI flag
        if std::env::var("BLAZECODE_DISABLE_AUTOCOMPACT").is_ok() {
            let mut comp = info.compaction.clone().unwrap_or_default();
            comp.auto = Some(false);
            info.compaction = Some(comp);
        }

        // Disable prune from CLI flag
        if std::env::var("BLAZECODE_DISABLE_PRUNE").is_ok() {
            let mut comp = info.compaction.clone().unwrap_or_default();
            comp.prune = Some(false);
            info.compaction = Some(comp);
        }

        // Post-processing (mode→agent, tools→permission, etc.)
        normalize_config(&mut info);

        Ok(info)
    }

    /// Load configuration from the default location into this instance.
    ///
    /// # Errors
    /// Returns an error if the config directory cannot be determined.
    pub fn refresh(&self) -> crate::error::Result<()> {
        let info = Self::load_global()?;
        *self.info.write().expect("Config lock poisoned") = info;
        Ok(())
    }

    /// Get the global config directory path.
    pub fn global_config_dir() -> crate::error::Result<PathBuf> {
        global_config_dir()
    }

    /// Get the data directory for blazecode.
    pub fn data_dir() -> crate::error::Result<PathBuf> {
        let data_dir = dirs::data_dir()
            .ok_or_else(|| crate::error::Error::Config("Cannot determine data directory".into()))?;
        Ok(data_dir.join("blazecode"))
    }

    /// Load a single config file (JSON or JSONC) and validate as [`Info`].
    ///
    /// Returns `Info::default()` if the file does not exist.
    ///
    /// # Errors
    /// Returns an error if the file exists but cannot be read or parsed.
    pub fn load_from_file(path: &std::path::Path) -> crate::error::Result<Info> {
        if !path.exists() {
            return Ok(Info::default());
        }
        let text = std::fs::read_to_string(path)?;
        let parsed = parse_jsonc(&text, path)?;
        validate_info(parsed, path)
    }

    /// Load project-level config from `{dir}/blazecode.json` or `{dir}/blazecode.jsonc`.
    ///
    /// Returns `Info::default()` if neither file exists.
    ///
    /// # Errors
    /// Returns an error if a file exists but cannot be read or parsed.
    pub fn load_project(dir: &std::path::Path) -> crate::error::Result<Info> {
        let json_path = dir.join("blazecode.json");
        let jsonc_path = dir.join("blazecode.jsonc");
        let path = if json_path.exists() {
            json_path
        } else {
            jsonc_path
        };
        Self::load_from_file(&path)
    }

    /// Save config to `{dir}/blazecode.json`, creating parent directories.
    ///
    /// # Errors
    /// Returns an error if serialization fails, the parent directory cannot be
    /// created, or the file cannot be written.
    pub fn save_project(info: &Info, dir: &std::path::Path) -> crate::error::Result<()> {
        let path = dir.join("blazecode.json");
        Self::save_to_file(&path, info)
    }

    /// Load config from `BLAZECODE_CONFIG_CONTENT` environment variable.
    ///
    /// Returns `None` if the env var is not set.
    ///
    /// # Source
    /// Ported from `packages/blazecode/src/config/config.ts` lines 467–475
    /// (`BLAZECODE_CONFIG_CONTENT` block).
    pub fn load_from_env() -> crate::error::Result<Option<Info>> {
        let content = match std::env::var("BLAZECODE_CONFIG_CONTENT") {
            Ok(v) => v,
            Err(_) => return Ok(None),
        };
        let expanded = match substitute_variables(&content, &std::path::Path::new("."), None) {
            Ok(s) => s,
            Err(e) => {
                return Err(crate::error::Error::Config(format!(
                    "BLAZECODE_CONFIG_CONTENT variable substitution failed: {e}"
                )));
            }
        };
        let parsed = parse_jsonc(&expanded, std::path::Path::new("BLAZECODE_CONFIG_CONTENT"))?;
        let info = validate_info(parsed, std::path::Path::new("BLAZECODE_CONFIG_CONTENT"))?;
        Ok(Some(info))
    }

    /// Load config from the managed config directory (system-wide).
    ///
    /// Reads `blazecode.json` and `blazecode.jsonc` from the managed config
    /// directory (`/etc/blazecode/`, `/Library/Application Support/blazecode/`,
    /// etc.) and also attempts macOS MDM managed preferences.
    ///
    /// # Source
    /// Ported from `packages/blazecode/src/config/config.ts` lines 516–533
    /// (managed config and managed preferences blocks).
    pub fn load_managed() -> crate::error::Result<Info> {
        let mut info = Info::default();

        // Managed config directory
        let managed_dir = managed_config_dir();
        if managed_dir.exists() {
            for filename in &["blazecode.json", "blazecode.jsonc"] {
                let path = managed_dir.join(filename);
                if path.exists() {
                    if let Ok(text) = std::fs::read_to_string(&path) {
                        if let Ok(parsed) = parse_jsonc(&text, &path) {
                            if let Ok(loaded) = validate_info(parsed, &path) {
                                merge_info(&mut info, &loaded);
                            }
                        }
                    }
                }
            }
        }

        // macOS managed preferences (MDM)
        if let Some((source, text)) = read_managed_preferences() {
            let expanded = substitute_variables(&text, &std::path::Path::new("."), None).unwrap_or(text);
            let parsed = parse_jsonc(&expanded, std::path::Path::new(&source))
                .unwrap_or(serde_json::Value::Object(serde_json::Map::new()));
            if let Ok(loaded) = validate_info(parsed, std::path::Path::new(&source)) {
                merge_info(&mut info, &loaded);
            }
        }

        Ok(info)
    }

    /// Load config from the `BLAZECODE_CONFIG_DIR` directory.
    ///
    /// Returns `Ok(Info::default())` if the directory does not exist.
    ///
    /// # Source
    /// Ported from `packages/blazecode/src/config/config.ts` lines 417–419
    /// (`BLAZECODE_CONFIG_DIR` block).
    pub fn load_from_config_dir() -> crate::error::Result<Info> {
        let config_dir = match std::env::var("BLAZECODE_CONFIG_DIR") {
            Ok(dir) => std::path::PathBuf::from(dir),
            Err(_) => return Ok(Info::default()),
        };
        let mut info = Info::default();
        for filename in &["blazecode.json", "blazecode.jsonc"] {
            let path = config_dir.join(filename);
            if path.exists() {
                if let Ok(text) = std::fs::read_to_string(&path) {
                    if let Ok(parsed) = parse_jsonc(&text, &path) {
                        if let Ok(loaded) = validate_info(parsed, &path) {
                            merge_info(&mut info, &loaded);
                        }
                    }
                }
            }
        }
        Ok(info)
    }

    /// Serialize `config` to pretty JSON and write to `path`.
    ///
    /// Creates parent directories if they do not exist.
    /// If the file extension is `.jsonc`, a comment header is prepended.
    ///
    /// # Source
    /// Ported from `packages/blazecode/src/config/config.ts` — `Config.save()`.
    ///
    /// # Errors
    /// Returns an error if serialization fails, the parent directory cannot be
    /// created, or the file cannot be written.
    pub fn save_to_file(path: &std::path::Path, config: &Info) -> crate::error::Result<()> {
        let mut json = serde_json::to_string_pretty(config)?;

        // Prepend comment header for .jsonc files
        if path.extension().is_some_and(|ext| ext == "jsonc") {
            json.insert_str(0, "// This file is auto-generated by blazecode.\n// Edit with care — comments and trailing commas are supported.\n\n");
        }

        // Create parent directories
        if let Some(parent) = path.parent() {
            if !parent.as_os_str().is_empty() {
                std::fs::create_dir_all(parent)?;
            }
        }

        std::fs::write(path, json)?;
        Ok(())
    }

    /// Save credentials for a provider to `auth.json`.
    ///
    /// Reads the existing `auth.json` (if it exists), merges in the new
    /// credentials for the given `provider_id`, and writes back.
    /// Creates parent directories if needed.
    ///
    /// # Source
    /// Ported from `packages/blazecode/src/auth/index.ts` — `put()`.
    ///
    /// # Errors
    /// Returns an error if the data directory cannot be determined,
    /// the existing file cannot be read or parsed, or the file cannot be written.
    pub fn save_auth(
        provider_id: &str,
        credentials: &serde_json::Value,
    ) -> crate::error::Result<()> {
        let data_dir = Self::data_dir()?;
        let auth_path = data_dir.join("auth.json");

        // Read existing auth, if any
        let mut providers: HashMap<String, serde_json::Value> = if auth_path.exists() {
            let content = std::fs::read_to_string(&auth_path)?;
            if content.trim().is_empty() {
                HashMap::new()
            } else {
                serde_json::from_str(&content).unwrap_or_default()
            }
        } else {
            HashMap::new()
        };

        // Merge in the new credentials
        providers.insert(provider_id.to_string(), credentials.clone());

        // Create parent dirs and write
        std::fs::create_dir_all(&data_dir)?;
        let json = serde_json::to_string_pretty(&providers)?;
        std::fs::write(&auth_path, json)?;
        Ok(())
    }

    /// Remove a provider's credentials from `auth.json`.
    ///
    /// Reads the existing `auth.json`, removes the entry for `provider_id`,
    /// and writes back. If the resulting map is empty, the file is deleted.
    ///
    /// # Source
    /// Ported from `packages/blazecode/src/auth/index.ts` — `remove()`.
    ///
    /// # Errors
    /// Returns an error if the data directory cannot be determined,
    /// the existing file cannot be read, or the file cannot be written.
    pub fn remove_auth(provider_id: &str) -> crate::error::Result<()> {
        let data_dir = Self::data_dir()?;
        let auth_path = data_dir.join("auth.json");

        // Read existing auth
        let mut providers: HashMap<String, serde_json::Value> = if auth_path.exists() {
            let content = std::fs::read_to_string(&auth_path)?;
            if content.trim().is_empty() {
                HashMap::new()
            } else {
                serde_json::from_str(&content).unwrap_or_default()
            }
        } else {
            // Nothing to remove
            return Ok(());
        };

        providers.remove(provider_id);

        if providers.is_empty() {
            // Remove the file if empty
            if auth_path.exists() {
                std::fs::remove_file(&auth_path)?;
            }
        } else {
            // Write back
            let json = serde_json::to_string_pretty(&providers)?;
            std::fs::write(&auth_path, json)?;
        }
        Ok(())
    }

    /// Load all provider credentials from `auth.json`.
    ///
    /// Returns an empty map if the file does not exist or is empty.
    ///
    /// # Source
    /// Ported from `packages/blazecode/src/auth/index.ts` — `get()`.
    ///
    /// # Errors
    /// Returns an error if the data directory cannot be determined
    /// or the file exists but cannot be read or parsed.
    pub fn load_auth() -> crate::error::Result<HashMap<String, serde_json::Value>> {
        let data_dir = Self::data_dir()?;
        let auth_path = data_dir.join("auth.json");

        if !auth_path.exists() {
            return Ok(HashMap::new());
        }

        let content = std::fs::read_to_string(&auth_path)?;
        if content.trim().is_empty() {
            return Ok(HashMap::new());
        }

        let providers: HashMap<String, serde_json::Value> = serde_json::from_str(&content)?;
        Ok(providers)
    }

    /// Update the project config by merging `config` into the existing project config.
    ///
    /// Reads the current `config.json` from `project_dir`, deep-merges the incoming
    /// config (stripping derived state via [`writable`]), and writes back.
    ///
    /// # Source
    /// Ported from `packages/blazecode/src/config/config.ts` lines 623–630
    /// (`Config.update`).
    pub fn update(&self, config: &Info) -> crate::error::Result<()> {
        let file = self.project_dir.join("config.json");
        let existing = Self::load_from_file(&file).unwrap_or_default();
        let merged = merge_writable(&existing, &writable(config));
        Self::save_to_file(&file, &merged)
    }

    /// Update the global config file with the given patch.
    ///
    /// Reads the global config file, deep-merges the patch (stripping derived
    /// state), writes back, and invalidates the in-memory cache.
    ///
    /// For `.jsonc` files, uses [`patch_jsonc`] to preserve comments.
    /// For `.json` files, full re-serialization.
    ///
    /// Returns the merged config and whether anything actually changed.
    ///
    /// # Source
    /// Ported from `packages/blazecode/src/config/config.ts` lines 636–659
    /// (`Config.updateGlobal`).
    pub fn update_global(config: &Info) -> crate::error::Result<UpdateResult> {
        let file = global_config_file()?;
        let before = std::fs::read_to_string(&file).unwrap_or_else(|_| "{}".to_owned());
        let patch = writable_global(config);

        let (next, changed) = if file.extension().is_some_and(|ext| ext == "jsonc") {
            let updated = patch_jsonc(&before, &serde_json::to_value(&patch)?);
            let parsed = parse_jsonc(&updated, &file)
                .unwrap_or(serde_json::Value::Object(serde_json::Map::new()));
            let info = validate_info(parsed, &file).unwrap_or_default();
            (info, updated != before)
        } else {
            let existing = parse_jsonc(&before, &file)
                .and_then(|v| validate_info(v, &file))
                .unwrap_or_default();
            let merged = merge_writable(&writable(&existing), &patch);
            let serialized = serde_json::to_string_pretty(&merged)?;
            let changed = serialized != before;
            if changed {
                std::fs::write(&file, serialized)?;
            }
            (merged, changed)
        };

        Ok(UpdateResult { info: next, changed })
    }

    /// Ensure a `.gitignore` file exists in the given directory.
    ///
    /// Creates a default `.gitignore` with common entries if none exists.
    ///
    /// # Source
    /// Ported from `packages/blazecode/src/config/config.ts` lines 295–311
    /// (`ensureGitignore`).
    pub fn ensure_gitignore(dir: &std::path::Path) -> crate::error::Result<()> {
        let gitignore = dir.join(".gitignore");
        if !gitignore.exists() {
            let content = [
                "node_modules",
                "package.json",
                "package-lock.json",
                "bun.lock",
                ".gitignore",
            ]
            .join("\n");
            let _ = std::fs::write(&gitignore, content);
        }
        Ok(())
    }

    /// Fetch remote well-known config from `{server_url}/.well-known/blazecode`.
    ///
    /// Performs an HTTP GET to the well-known URL, parses the response, and
    /// merges the returned config into the provided `Info`.
    ///
    /// Ported from: `packages/blazecode/src/config/config.ts` lines 355–394.
    pub async fn fetch_well_known(
        server_url: &str,
        info: &mut Info,
    ) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
        let wellknown_url = format!("{}/.well-known/blazecode", server_url.trim_end_matches('/'));
        let client = reqwest::Client::new();
        let resp = client
            .get(&wellknown_url)
            .header("Accept", "application/json")
            .send()
            .await?;
        let wellknown: serde_json::Value = resp.json().await?;

        let remote_config_url = wellknown["remote_config"]["url"]
            .as_str()
            .map(String::from);
        let remote_config_headers = wellknown["remote_config"]["headers"]
            .as_object()
            .map(|m| {
                m.iter()
                    .filter_map(|(k, v)| v.as_str().map(|v| (k.clone(), v.to_string())))
                    .collect::<HashMap<String, String>>()
            });

        let mut fetched = serde_json::Map::new();
        if let Some(url) = remote_config_url {
            let mut req = client.get(&url).header("Accept", "application/json");
            if let Some(ref headers) = remote_config_headers {
                for (k, v) in headers {
                    req = req.header(k.as_str(), v.as_str());
                }
            }
            let data: serde_json::Value = req.send().await?.json().await?;
            if let Some(cfg) = data.get("config").and_then(|v| v.as_object()) {
                fetched = cfg.clone();
            } else if let Some(obj) = data.as_object() {
                fetched = obj.clone();
            }
        }

        let local = wellknown["config"].as_object().cloned().unwrap_or_default();
        let merged = deep_merge_json_map(local, &fetched);

        if let Ok(parsed) =
            serde_json::from_value::<Info>(serde_json::Value::Object(merged))
        {
            merge_info(info, &parsed);
        }

        Ok(())
    }

    /// Load TUI configuration from `tui.json`/`tui.jsonc` files in config dirs.
    ///
    /// Reads global config first, then BLAZECODE_TUI_CONFIG override, then
    /// project-level files, then `.blazecode` directory files.
    ///
    /// Ported from: `packages/blazecode/src/config/tui.ts` lines 81–224.
    pub fn load_tui_config(start_dir: &std::path::Path) -> TuiConfigInfo {
        let mut result = TuiConfigInfo::default();

        // 1. Global tui config (lowest precedence)
        if let Ok(config_dir) = global_config_dir() {
            for file in &[config_dir.join("tui.json"), config_dir.join("tui.jsonc")] {
                if let Ok(text) = std::fs::read_to_string(&file) {
                    if let Ok(parsed) = parse_jsonc(&text, &file) {
                        if let Ok(tui) = serde_json::from_value::<TuiConfigInfo>(parsed) {
                            result = deep_merge_tui(result, tui);
                        }
                    }
                }
            }
        }

        // 2. BLAZECODE_TUI_CONFIG override
        if let Ok(path) = std::env::var("BLAZECODE_TUI_CONFIG") {
            let file = std::path::Path::new(&path).to_path_buf();
            if let Ok(text) = std::fs::read_to_string(&file) {
                if let Ok(parsed) = parse_jsonc(&text, &file) {
                    if let Ok(tui) = serde_json::from_value::<TuiConfigInfo>(parsed) {
                        result = deep_merge_tui(result, tui);
                    }
                }
            }
        }

        // 3. Project tui files (walk up from start_dir)
        if let Ok(files) = discover_config_files("tui", start_dir, None) {
            for file in files {
                if let Ok(text) = std::fs::read_to_string(&file) {
                    if let Ok(parsed) = parse_jsonc(&text, &file) {
                        if let Ok(tui) = serde_json::from_value::<TuiConfigInfo>(parsed) {
                            result = deep_merge_tui(result, tui);
                        }
                    }
                }
            }
        }

        // 4. .blazecode directories
        if let Ok(dirs) = discover_blazecode_dirs(start_dir, None) {
            for dir in dirs {
                for file in &[dir.join("tui.json"), dir.join("tui.jsonc")] {
                    if let Ok(text) = std::fs::read_to_string(&file) {
                        if let Ok(parsed) = parse_jsonc(&text, &file) {
                            if let Ok(tui) = serde_json::from_value::<TuiConfigInfo>(parsed) {
                                result = deep_merge_tui(result, tui);
                            }
                        }
                    }
                }
            }
        }

        // 5. BLAZECODE_CONFIG_DIR (treated like an additional .blazecode directory)
        if let Ok(config_dir) = std::env::var("BLAZECODE_CONFIG_DIR") {
            let config_path = std::path::PathBuf::from(&config_dir);
            for file in &[config_path.join("tui.json"), config_path.join("tui.jsonc")] {
                if let Ok(text) = std::fs::read_to_string(&file) {
                    if let Ok(parsed) = parse_jsonc(&text, &file) {
                        if let Ok(tui) = serde_json::from_value::<TuiConfigInfo>(parsed) {
                            result = deep_merge_tui(result, tui);
                        }
                    }
                }
            }
        }

        result
    }

    /// Ensure `@blazecode-ai/plugin` npm dependency is installed in each
    /// `.blazecode` directory. Spawns `npm install` or `bun add` in the
    /// background if the package is not already present.
    ///
    /// Ported from: `packages/blazecode/src/config/config.ts` lines 437–456.
    pub fn ensure_npm_deps(dirs: &[std::path::PathBuf]) -> Vec<std::process::Child> {
        let mut children = Vec::new();

        for dir in dirs {
            let pkg_json = dir.join("package.json");
            let has_pkg = pkg_json.exists();
            let has_plugin = if has_pkg {
                if let Ok(content) = std::fs::read_to_string(&pkg_json) {
                    if let Ok(v) =
                        serde_json::from_str::<serde_json::Value>(&content)
                    {
                        let deps = v["dependencies"].as_object();
                        let dev = v["devDependencies"].as_object();
                        deps.and_then(|d| d.get("@blazecode-ai/plugin")).is_some()
                            || dev.and_then(|d| d.get("@blazecode-ai/plugin")).is_some()
                    } else {
                        false
                    }
                } else {
                    false
                }
            } else {
                false
            };

            if has_plugin {
                continue;
            }

            // Pick the package manager: bun, then npm
            let (cmd, add_arg) = if which("bun").is_some() {
                ("bun", "add")
            } else {
                ("npm", "install --save-prod")
            };

            match std::process::Command::new(cmd)
                .args(add_arg.split_whitespace().collect::<Vec<_>>())
                .arg("@blazecode-ai/plugin")
                .current_dir(dir)
                .stdout(std::process::Stdio::null())
                .stderr(std::process::Stdio::null())
                .spawn()
            {
                Ok(child) => children.push(child),
                Err(_) => continue,
            }
        }

        children
    }

    /// Migrate a V1 config `Info` to V2 config format.
    ///
    /// Transforms fields, renames keys, and restructures providers and agents.
    ///
    /// Ported from: `packages/core/src/v1/config/migrate.ts` lines 36–73.
    pub fn migrate_v1_to_v2(info: &Info) -> V2ConfigInfo {
        let share = info.share.or_else(|| {
            if info.autoshare == Some(true) {
                Some(crate::config::ShareMode::Auto)
            } else {
                None
            }
        });
        let share_str = share.map(|s| match s {
            ShareMode::Manual => "manual".to_string(),
            ShareMode::Auto => "auto".to_string(),
            ShareMode::Disabled => "disabled".to_string(),
        });

        let mut v2 = V2ConfigInfo {
            schema: info.schema.clone(),
            shell: info.shell.clone(),
            model: info.model.clone(),
            default_agent: info.default_agent.clone(),
            autoupdate: info.autoupdate.clone(),
            share: share_str,
            enterprise: info.enterprise.clone(),
            username: info.username.clone(),
            permissions: Vec::new(),
            agents: None,
            snapshots: info.snapshot,
            watcher: info.watcher.clone(),
            formatter: info.formatter.clone(),
            lsp: info.lsp.clone(),
            attachments: info.attachment.clone(),
            tool_output: info.tool_output.clone(),
            mcp: None,
            compaction: None,
            skills: None,
            commands: None,
            instructions: if info.instructions.is_empty() {
                None
            } else {
                Some(info.instructions.clone())
            },
            references: if info.references.is_empty() && info.reference.is_empty() {
                None
            } else {
                let mut refs: HashMap<String, String> = HashMap::new();
                for (k, v) in &info.references {
                    if let ReferenceEntry::Simple(s) = v {
                        refs.insert(k.clone(), s.clone());
                    }
                }
                for (k, v) in &info.reference {
                    if let ReferenceEntry::Simple(s) = v {
                        refs.entry(k.clone()).or_insert_with(|| s.clone());
                    }
                }
                Some(refs)
            },
            plugins: None,
            experimental: info
                .experimental
                .as_ref()
                .and_then(|e| Some(e.policies.as_ref()))
                .filter(|p: &&[serde_json::Value]| !p.is_empty())
                .map(|p| V2ExperimentalConfig {
                    policies: p.to_vec(),
                }),
            providers: None,
        };

        // Permissions
        for (action, enabled) in &info.tools {
            let normalized_action = if action == "write" || action == "patch" {
                "edit"
            } else {
                action.as_str()
            };
            v2.permissions.push(V2PermissionRule {
                action: normalized_action.to_string(),
                resource: "*".to_string(),
                effect: if *enabled { "allow" } else { "deny" }.to_string(),
            });
        }
        if let Some(ref perm) = info.permission {
            serialize_permission_rules(perm, &mut v2.permissions);
        }
        if v2.permissions.is_empty() {
            v2.permissions = Vec::new();
        }

        // Agents
        let mut agent_entries: HashMap<String, V2AgentEntry> = HashMap::new();
        for (name, agent) in &info.agent {
            agent_entries.insert(name.clone(), V2AgentEntry {
                model: agent.model.clone(),
                variant: agent.variant.clone(),
                description: agent.description.clone(),
                mode: agent.mode.clone().map(|m| match m {
                    AgentMode::Subagent => "subagent".to_string(),
                    AgentMode::Primary => "primary".to_string(),
                    AgentMode::All => "all".to_string(),
                }),
                hidden: agent.hidden,
                color: agent.color.clone(),
                steps: agent.steps,
                disabled: agent.disable,
                system: agent.prompt.clone(),
            });
        }
        for (name, mode) in &info.mode {
            agent_entries.entry(name.clone()).or_insert(V2AgentEntry {
                model: mode.model.clone(),
                variant: mode.variant.clone(),
                description: mode.description.clone(),
                mode: mode.mode.clone().map(|m| match m {
                    AgentMode::Subagent => "subagent".to_string(),
                    AgentMode::Primary => "primary".to_string(),
                    AgentMode::All => "all".to_string(),
                }),
                hidden: mode.hidden,
                color: mode.color.clone(),
                steps: mode.steps,
                disabled: mode.disable,
                system: mode.prompt.clone(),
            });
        }
        if !agent_entries.is_empty() {
            v2.agents = Some(agent_entries);
        }

        // MCP
        if !info.mcp.is_empty() {
            let mut servers = HashMap::new();
            for (name, entry) in &info.mcp {
                match entry {
                    McpEntry::Full(cfg) => match cfg {
                        McpConfig::Local { command, cwd, environment, enabled, timeout, .. } => {
                            servers.insert(name.clone(), V2McpServer {
                                r#type: "local".to_string(),
                                command: Some(command.clone()),
                                cwd: cwd.clone(),
                                environment: Some(environment.clone()),
                                disabled: enabled.map(|e| !e),
                                timeout: *timeout,
                                url: None,
                                headers: None,
                            });
                        }
                        McpConfig::Remote { url, enabled, headers, timeout, .. } => {
                            servers.insert(name.clone(), V2McpServer {
                                r#type: "remote".to_string(),
                                command: None,
                                cwd: None,
                                environment: None,
                                disabled: enabled.map(|e| !e),
                                timeout: *timeout,
                                url: Some(url.clone()),
                                headers: Some(headers.clone()),
                            });
                        }
                    },
                    McpEntry::Toggle { enabled } => {
                        servers.insert(name.clone(), V2McpServer {
                            r#type: "local".to_string(),
                            command: None,
                            cwd: None,
                            environment: None,
                            disabled: Some(!*enabled),
                            timeout: None,
                            url: None,
                            headers: None,
                        });
                    }
                }
            }
            v2.mcp = Some(V2McpConfig {
                timeout: info.experimental.as_ref().and_then(|e| e.mcp_timeout),
                servers,
            });
        }

        // Compaction
        if let Some(ref comp) = info.compaction {
            v2.compaction = Some(V2CompactionConfig {
                auto: comp.auto,
                prune: comp.prune,
                keep_tokens: comp.preserve_recent_tokens,
                buffer: comp.reserved,
            });
        }

        // Skills
        if let Some(ref skills) = info.skills {
            let mut all = Vec::new();
            all.extend(skills.paths.iter().cloned());
            all.extend(skills.urls.iter().cloned());
            if !all.is_empty() {
                v2.skills = Some(all);
            }
        }

        // Commands
        if !info.command.is_empty() {
            let mut cmds = HashMap::new();
            for (name, cmd) in &info.command {
                cmds.insert(name.clone(), V2CommandEntry {
                    template: cmd.template.clone(),
                    description: cmd.description.clone(),
                    agent: cmd.agent.clone(),
                    model: cmd.model.clone(),
                });
            }
            v2.commands = Some(cmds);
        }

        // Plugins
        if !info.plugin.is_empty() {
            let mut plist = Vec::new();
            for plugin in &info.plugin {
                match plugin {
                    PluginSpec::Simple(s) => plist.push(s.clone()),
                    PluginSpec::WithOptions(s, opts) => {
                        plist.push(serde_json::json!({
                            "package": s,
                            "options": opts,
                        })
                        .to_string());
                    }
                }
            }
            v2.plugins = Some(plist);
        }

        // Providers
        if !info.provider.is_empty() {
            let mut provs = HashMap::new();
            for (name, prov) in &info.provider {
                provs.insert(name.clone(), V2ProviderEntry {
                    name: prov.name.clone(),
                    env: prov.env.clone(),
                    api: prov.npm.as_ref().map(|npm| V2ApiEntry {
                        r#type: "aisdk".to_string(),
                        package_name: npm.clone(),
                        url: prov.api.clone(),
                    }),
                    models: if prov.models.is_empty() {
                        None
                    } else {
                        Some(
                            prov.models
                                .iter()
                                .map(|(k, v)| (k.clone(), V2ModelEntry {
                                    family: v.family.clone(),
                                    name: v.name.clone(),
                                    id: v.id.clone(),
                                }))
                                .collect(),
                        )
                    },
                });
            }
            v2.providers = Some(provs);
        }

        v2
    }
}

/// Deep-merge two JSON maps recursively.
fn deep_merge_json_map(
    base: serde_json::Map<String, serde_json::Value>,
    patch: &serde_json::Map<String, serde_json::Value>,
) -> serde_json::Map<String, serde_json::Value> {
    let mut result = base;
    for (key, patch_val) in patch {
        match (result.remove(key), patch_val) {
            (Some(serde_json::Value::Object(base_obj)), serde_json::Value::Object(patch_obj)) => {
                result.insert(key.clone(), serde_json::Value::Object(deep_merge_json_map(base_obj, patch_obj)));
            }
            (_, val) => {
                result.insert(key.clone(), val.clone());
            }
        }
    }
    result
}

/// Deep-merge two TuiConfigInfo values (simple field-level merge).
fn deep_merge_tui(base: TuiConfigInfo, patch: TuiConfigInfo) -> TuiConfigInfo {
    TuiConfigInfo {
        theme: patch.theme.or(base.theme),
        keybinds: if patch.keybinds.is_empty() { base.keybinds } else { patch.keybinds },
        colors: patch.colors.or(base.colors),
        font_size: patch.font_size.or(base.font_size),
        font_family: patch.font_family.or(base.font_family),
        show_status_bar: patch.show_status_bar.or(base.show_status_bar),
        show_minimap: patch.show_minimap.or(base.show_minimap),
        cursor_style: patch.cursor_style.or(base.cursor_style),
        scrollbar: patch.scrollbar.or(base.scrollbar),
        host_attention: {
            match (base.host_attention, patch.host_attention) {
                (Some(b), Some(p)) => {
                    let mut merged = b;
                    if p.sounds.is_some() {
                        merged.sounds = p.sounds;
                    }
                    if p.flash.is_some() {
                        merged.flash = p.flash;
                    }
                    Some(merged)
                }
                (Some(b), None) => Some(b),
                (None, Some(p)) => Some(p),
                (None, None) => None,
            }
        },
        plugin: if !patch.plugin.is_empty() {
            patch.plugin
        } else {
            base.plugin
        },
        extra: {
            let mut m = base.extra;
            m.extend(patch.extra);
            m
        },
    }
}

fn serialize_permission_rules(
    config: &PermissionConfig,
    rules: &mut Vec<V2PermissionRule>,
) {
    macro_rules! push_rule {
        ($field:expr, $action:expr) => {
            if let Some(ref rule) = $field {
                match rule {
                    PermissionRule::Action(a) => {
                        let effect = match a {
                            PermissionAction::Ask => "ask",
                            PermissionAction::Allow => "allow",
                            PermissionAction::Deny => "deny",
                        };
                        rules.push(V2PermissionRule {
                            action: $action.to_string(),
                            resource: "*".to_string(),
                            effect: effect.to_string(),
                        });
                    }
                    PermissionRule::Object(map) => {
                        for (resource, a) in map {
                            let effect = match a {
                                PermissionAction::Ask => "ask",
                                PermissionAction::Allow => "allow",
                                PermissionAction::Deny => "deny",
                            };
                            rules.push(V2PermissionRule {
                                action: $action.to_string(),
                                resource: resource.clone(),
                                effect: effect.to_string(),
                            });
                        }
                    }
                }
            }
        };
    }

    macro_rules! push_action_rule {
        ($field:expr, $action:expr) => {
            if let Some(ref a) = $field {
                let effect = match a {
                    PermissionAction::Ask => "ask",
                    PermissionAction::Allow => "allow",
                    PermissionAction::Deny => "deny",
                };
                rules.push(V2PermissionRule {
                    action: $action.to_string(),
                    resource: "*".to_string(),
                    effect: effect.to_string(),
                });
            }
        };
    }

    push_rule!(config.read, "read");
    push_rule!(config.edit, "edit");
    push_rule!(config.glob, "glob");
    push_rule!(config.grep, "grep");
    push_rule!(config.bash, "bash");
    push_rule!(config.list, "list");
    push_rule!(config.task, "task");
    push_rule!(config.lsp, "lsp");
    push_rule!(config.skill, "skill");
    push_rule!(config.external_directory, "external_directory");
    push_action_rule!(config.wildcard, "*");
    push_action_rule!(config.todowrite, "todowrite");
    push_action_rule!(config.question, "question");
    push_action_rule!(config.webfetch, "webfetch");
    push_action_rule!(config.websearch, "websearch");
    push_action_rule!(config.doom_loop, "doom_loop");

    for (action, rule) in &config.extra {
        push_rule!(Some(rule), action);
    }
}

/// Extract the entry name from a file path by stripping known prefixes
/// and the file extension.
///
/// Ported from: `packages/blazecode/src/config/entry-name.ts` lines 8–18.
pub fn config_entry_name_from_path(relative_path: &str, prefixes: &[&str]) -> String {
    let normalized = relative_path.replace('\\', "/");
    let candidate = prefixes
        .iter()
        .find(|p| normalized.starts_with(*p))
        .map(|p| normalized[p.len()..].to_string())
        .unwrap_or_else(|| {
            std::path::Path::new(&normalized)
                .file_name()
                .map(|s| s.to_string_lossy().to_string())
                .unwrap_or_else(|| normalized.clone())
        });
    let path = std::path::Path::new(&candidate);
    let ext = path.extension().and_then(|e| e.to_str()).unwrap_or("");
    if ext.is_empty() {
        candidate
    } else {
        let dot_ext = format!(".{ext}");
        candidate.strip_suffix(&dot_ext).unwrap_or(&candidate).to_string()
    }
}

fn which(name: &str) -> Option<String> {
    std::env::var_os("PATH").and_then(|path_var| {
        std::env::split_paths(&path_var).find_map(|dir| {
            let path = dir.join(name);
            if path.is_file() {
                Some(path.to_string_lossy().to_string())
            } else {
                None
            }
        })
    })
}

// ── Config writable helpers ──────────────────────────────────────────────

/// Strip derived/non-persistable state from an `Info` before writing to disk.
///
/// Removes `plugin_origins` (which is derived, not a user config field).
///
/// # Source
/// Ported from `packages/blazecode/src/config/config.ts` lines 163–166
/// (`writable`).
pub fn writable(config: &Info) -> Info {
    let mut out = config.clone();
    out.plugin_origins.clear();
    out
}

/// Strip derived state and clean up empty values for global config writes.
///
/// Extends [`writable`] by also clearing empty `shell` (avoids persisting
/// `""` back to the global config).
///
/// # Source
/// Ported from `packages/blazecode/src/config/config.ts` lines 168–173
/// (`writableGlobal`).
pub fn writable_global(config: &Info) -> Info {
    let mut out = writable(config);
    if out.shell.as_deref() == Some("") {
        out.shell = None;
    }
    out
}

/// Merge two configs using writable-strip semantics.
///
/// Deep-merges `patch` into `source`, both stripped via [`writable`].
///
/// # Source
/// Ported from `packages/blazecode/src/config/config.ts` line 628
/// (`mergeDeep(writable(existing), writable(config))`).
pub fn merge_writable(source: &Info, patch: &Info) -> Info {
    let mut target = writable(source);
    merge_info(&mut target, patch);
    target
}

// ── JSONC patching ──────────────────────────────────────────────────────

/// Patch a JSONC string with a JSON value, preserving comments.
///
/// This is a simplified port that serializes the patch, deep-merges it into
/// the existing parsed JSON, and re-serializes with the existing key ordering
/// where possible. For complex cases, falls back to full re-serialization.
///
/// # Source
/// Ported from `packages/blazecode/src/config/config.ts` lines 149–161
/// (`patchJsonc`).
pub fn patch_jsonc(existing: &str, patch: &serde_json::Value) -> String {
    // If the patch is not an object, just serialize it
    let patch_obj = match patch.as_object() {
        Some(o) => o,
        None => return serde_json::to_string_pretty(patch).unwrap_or_default(),
    };

    // Parse the existing JSONC (strip comments first)
    let cleaned = strip_jsonc_comments(existing);
    let existing_value: serde_json::Value =
        serde_json::from_str(&cleaned).unwrap_or(serde_json::Value::Object(serde_json::Map::new()));

    // Deep merge: patch into existing
    let merged = deep_merge_json(existing_value, patch);

    // Re-serialize with pretty formatting
    serde_json::to_string_pretty(&merged).unwrap_or_else(|_| merged.to_string())
}

/// Deep-merge a JSON patch into a base value.
///
/// Objects are merged recursively. Arrays and scalars from `patch` replace `base`.
fn deep_merge_json(base: serde_json::Value, patch: &serde_json::Value) -> serde_json::Value {
    match (base, patch) {
        (serde_json::Value::Object(mut base_map), serde_json::Value::Object(patch_map)) => {
            for (key, patch_val) in patch_map {
                let entry = base_map
                    .entry(key.clone())
                    .or_insert(serde_json::Value::Null);
                *entry = deep_merge_json(entry.take(), patch_val);
            }
            serde_json::Value::Object(base_map)
        }
        (_, patch @ serde_json::Value::Object(_)) => patch.clone(),
        (_, patch) => patch.clone(),
    }
}

// ── Config file discovery ────────────────────────────────────────────────

/// Discover config files walking up from `start_dir`, stopping at `worktree`.
///
/// Looks for `{name}.jsonc` and `{name}.json` in each directory.
///
/// # Source
/// Ported from `packages/blazecode/src/config/paths.ts` lines 10–21
/// (`ConfigPaths.files`).
pub fn discover_config_files(
    name: &str,
    start_dir: &std::path::Path,
    stop_dir: Option<&std::path::Path>,
) -> crate::error::Result<Vec<PathBuf>> {
    let mut files = Vec::new();
    let mut current = Some(start_dir.to_path_buf());

    while let Some(dir) = current {
        // Stop at the worktree boundary
        if let Some(stop) = stop_dir {
            if dir.as_path() == stop {
                break;
            }
        }

        for ext in &["jsonc", "json"] {
            let candidate = dir.join(format!("{name}.{ext}"));
            if candidate.exists() {
                files.push(candidate);
            }
        }

        let dir_path = dir.as_path();
        current = dir.parent().map(|p| p.to_path_buf());

        // Stop at filesystem root
        if current.as_ref().is_none_or(|p| p.as_path() == dir_path) {
            break;
        }
    }

    // Reverse so parent dirs come first (lowest priority)
    files.reverse();
    Ok(files)
}

/// Discover `.blazecode` directories walking up from `start_dir`.
///
/// # Source
/// Ported from `packages/blazecode/src/config/paths.ts` lines 23–41
/// (`ConfigPaths.directories`).
pub fn discover_blazecode_dirs(
    start_dir: &std::path::Path,
    stop_dir: Option<&std::path::Path>,
) -> crate::error::Result<Vec<PathBuf>> {
    let mut dirs = Vec::new();
    let mut current = Some(start_dir.to_path_buf());

    while let Some(dir) = current {
        if let Some(stop) = stop_dir {
            if dir.as_path() == stop {
                break;
            }
        }

        let blazecode_dir = dir.join(".blazecode");
        if blazecode_dir.exists() && blazecode_dir.is_dir() {
            dirs.push(blazecode_dir);
        }

        let dir_path = dir.as_path();
        current = dir.parent().map(|p| p.to_path_buf());
        if current.as_ref().is_none_or(|p| p.as_path() == dir_path) {
            break;
        }
    }

    dirs.reverse();
    Ok(dirs)
}

// ── JSONC parsing ───────────────────────────────────────────────────────

/// Parse JSONC (JSON with comments and trailing commas) into a [`serde_json::Value`].
///
/// Strips `//` line comments, `/* */` block comments, and trailing commas
/// before delegating to `serde_json`.
///
/// # Source
/// Ported from `packages/blazecode/src/config/parse.ts` lines 8–33
/// (`ConfigParse.jsonc`).
pub fn parse_jsonc(
    text: &str,
    _filepath: &std::path::Path,
) -> crate::error::Result<serde_json::Value> {
    let cleaned = strip_jsonc_comments(text);
    serde_json::from_str(&cleaned).map_err(|e| {
        crate::error::Error::Config(format!(
            "JSON parse error in `{}`: {e}",
            _filepath.display()
        ))
    })
}

/// Strip `//` line comments and `/* */` block comments from JSONC text.
///
/// Also removes trailing commas before `]` and `}`.
fn strip_jsonc_comments(text: &str) -> String {
    let mut result = String::with_capacity(text.len());
    let chars: Vec<char> = text.chars().collect();
    let mut i = 0;

    while i < chars.len() {
        // String literal — copy verbatim
        if chars[i] == '"' {
            result.push('"');
            i += 1;
            while i < chars.len() {
                if chars[i] == '\\' && i + 1 < chars.len() {
                    result.push('\\');
                    result.push(chars[i + 1]);
                    i += 2;
                } else if chars[i] == '"' {
                    result.push('"');
                    i += 1;
                    break;
                } else {
                    result.push(chars[i]);
                    i += 1;
                }
            }
            continue;
        }

        // Line comment
        if chars[i] == '/' && i + 1 < chars.len() && chars[i + 1] == '/' {
            result.push(' '); // replace with whitespace
            i += 2;
            while i < chars.len() && chars[i] != '\n' {
                result.push(' ');
                i += 1;
            }
            continue;
        }

        // Block comment
        if chars[i] == '/' && i + 1 < chars.len() && chars[i + 1] == '*' {
            result.push(' '); // replace with whitespace
            i += 2;
            while i < chars.len() {
                if chars[i] == '*' && i + 1 < chars.len() && chars[i + 1] == '/' {
                    result.push(' ');
                    result.push(' ');
                    i += 2;
                    break;
                }
                // Preserve newlines for line-count accuracy
                if chars[i] == '\n' {
                    result.push('\n');
                } else {
                    result.push(' ');
                }
                i += 1;
            }
            continue;
        }

        // Trailing comma before ] or }
        if chars[i] == ',' {
            // Look ahead past whitespace for ] or }
            let mut j = i + 1;
            while j < chars.len()
                && (chars[j] == ' ' || chars[j] == '\t' || chars[j] == '\n' || chars[j] == '\r')
            {
                j += 1;
            }
            if j < chars.len() && (chars[j] == ']' || chars[j] == '}') {
                result.push(' '); // replace comma with space
                i += 1;
                continue;
            }
        }

        result.push(chars[i]);
        i += 1;
    }

    result
}

// ── Schema validation ────────────────────────────────────────────────────

/// Known top-level keys in ConfigV1.Info.
///
/// Used to detect unrecognised keys during validation.
/// Keys must match the blazecode ConfigV1.Info schema exactly:
/// <https://github.com/sst/blazecode/blob/dev/packages/core/src/v1/config/config.ts>
const KNOWN_INFO_KEYS: &[&str] = &[
    "$schema",
    "shell",
    "logLevel",
    "server",
    "command",
    "skills",
    "references",
    "reference",
    "watcher",
    "snapshot",
    "plugin",
    "share",
    "autoshare",
    "autoupdate",
    "disabled_providers",
    "enabled_providers",
    "model",
    "small_model",
    "default_agent",
    "username",
    "mode",
    "agent",
    "provider",
    "mcp",
    "formatter",
    "lsp",
    "instructions",
    "layout",
    "permission",
    "tools",
    "attachment",
    "enterprise",
    "tool_output",
    "compaction",
    "experimental",
];

/// Validate parsed JSON against the ConfigV1.Info schema.
///
/// Rejects unrecognised top-level keys and type mismatches.
///
/// # Source
/// Ported from `packages/blazecode/src/config/parse.ts` lines 35–79
/// (`ConfigParse.schema`).
pub fn validate_info(
    mut value: serde_json::Value,
    source: &std::path::Path,
) -> crate::error::Result<Info> {
    // Strip legacy TUI keys (theme, keybinds, tui) before validation
    normalize_loaded_config(&mut value);

    // Check for unrecognised keys
    if let Some(obj) = value.as_object() {
        let known: std::collections::HashSet<&str> = KNOWN_INFO_KEYS.iter().copied().collect();
        let unknown: Vec<String> = obj
            .keys()
            .filter(|k| !known.contains(k.as_str()))
            .cloned()
            .collect();
        if !unknown.is_empty() {
            let s = if unknown.len() == 1 { "" } else { "s" };
            return Err(crate::error::Error::Config(format!(
                "Unrecognised key{s} in `{}`: {}",
                source.display(),
                unknown.join(", ")
            )));
        }
    }

    // Deserialize into Info
    serde_json::from_value(value).map_err(|e| {
        crate::error::Error::Config(format!(
            "Config validation error in `{}`: {e}",
            source.display()
        ))
    })
}

// ── Variable substitution ────────────────────────────────────────────────

/// Substitute `{env:VAR}` and `{file:path}` placeholders in config text.
///
/// This function performs synchronous I/O and should only be called during
/// application initialization (Config::load). For async contexts, use
/// [`substitute_variables_async`] instead.
///
/// # Source
/// Ported from `packages/blazecode/src/config/variable.ts` lines 34–91
/// (`ConfigVariable.substitute`).
pub fn substitute_variables(
    text: &str,
    dir: &std::path::Path,
    env: Option<&HashMap<String, String>>,
) -> crate::error::Result<String> {
    let mut result = text.to_owned();

    // {env:VAR} — replace with env var value or empty string
    let env_re = regex::Regex::new(r"\{env:([^}]+)\}")
        .map_err(|e| crate::error::Error::Config(format!("regex error: {e}")))?;
    result = env_re
        .replace_all(&result, |caps: &regex::Captures| {
            let var_name = &caps[1];
            if let Some(env_map) = env {
                if let Some(val) = env_map.get(var_name) {
                    return val.clone();
                }
            }
            std::env::var(var_name).unwrap_or_default()
        })
        .to_string();

    // {file:path} — read file contents and substitute
    let file_re = regex::Regex::new(r"\{file:([^}]+)\}")
        .map_err(|e| crate::error::Error::Config(format!("regex error: {e}")))?;

    // Check if there are file references before processing
    if !file_re.is_match(&result) {
        return Ok(result);
    }

    let mut out = String::with_capacity(result.len());
    let mut cursor = 0;

    for caps in file_re.captures_iter(&result) {
        let m = caps.get(0).expect("capture group 0 always exists for a regex match");
        let file_path_str = &caps[1];

        // Copy text before this match
        out.push_str(&result[cursor..m.start()]);

        // Check if this line is commented out (starts with // after whitespace)
        let line_start = result[..m.start()].rfind('\n').map_or(0, |i| i + 1);
        let prefix = result[line_start..m.start()].trim_start();
        if prefix.starts_with("//") {
            // Keep the token verbatim if on a commented line
            out.push_str(m.as_str());
            cursor = m.end();
            continue;
        }

        // Resolve the file path
        let resolved = if let Some(stripped) = file_path_str.strip_prefix("~/") {
            if let Some(home) = dirs::home_dir() {
                home.join(stripped)
            } else {
                return Err(crate::error::Error::Config(
                    "Cannot determine home directory for {file:...} substitution".into(),
                ));
            }
        } else if std::path::Path::new(file_path_str).is_absolute() {
            std::path::PathBuf::from(file_path_str)
        } else {
            dir.join(file_path_str)
        };

        // Security check: canonicalize and verify the resolved path
        // is within the config/project directory to prevent path traversal.
        let canonical = resolved.canonicalize().map_err(|e| {
            crate::error::Error::Config(format!(
                "Failed to resolve path '{file_path_str}': {e}"
            ))
        })?;
        let canonical_dir = dir.canonicalize().unwrap_or_else(|_| dir.to_path_buf());
        if !canonical.starts_with(&canonical_dir) && !canonical.starts_with(dirs::home_dir().as_ref().unwrap_or(&canonical_dir)) {
            return Err(crate::error::Error::Config(format!(
                "Path traversal blocked: {file_path_str} resolves to {canonical:?} outside config directory {canonical_dir:?}"
            )));
        }

        // Read the file
        match std::fs::read_to_string(&canonical) {
            Ok(content) => {
                // JSON-escape the content and strip surrounding quotes
                let escaped = serde_json::to_string(&content.trim())
                    .unwrap_or_else(|_| content.trim().to_owned());
                // Remove surrounding quotes from JSON string
                if escaped.starts_with('"') && escaped.ends_with('"') {
                    out.push_str(&escaped[1..escaped.len() - 1]);
                } else {
                    out.push_str(&escaped);
                }
            }
            Err(e) => {
                return Err(crate::error::Error::Config(format!(
                    "bad file reference: `{{{}}}` — {} does not exist or cannot be read: {e}",
                    m.as_str(),
                    resolved.display()
                )));
            }
        }

        cursor = m.end();
    }

    out.push_str(&result[cursor..]);
    Ok(out)
}

/// Async version of [`substitute_variables`] — uses tokio::task::spawn_blocking
/// to avoid blocking the async runtime on file I/O.
///
/// # Source
/// Ported from `packages/blazecode/src/config/variable.ts` lines 34–91
/// (`ConfigVariable.substitute`).
pub async fn substitute_variables_async(
    text: &str,
    dir: &std::path::Path,
    env: Option<&HashMap<String, String>>,
) -> crate::error::Result<String> {
    let text = text.to_owned();
    let dir = dir.to_owned();
    let env = env.map(|e| e.clone());
    tokio::task::spawn_blocking(move || {
        substitute_variables(&text, &dir, env.as_ref())
    })
    .await
    .map_err(|e| crate::error::Error::Internal(format!("spawn_blocking failed: {e}")))?
}

// ── Config merging ──────────────────────────────────────────────────────

/// Deep-merge `source` into `target`, mutating `target`.
///
/// - Objects are merged recursively.
/// - The `instructions` array is concatenated (deduplicated).
/// - All other fields: source wins if present.
///
/// # Source
/// Ported from `packages/blazecode/src/config/config.ts` lines 41–52
/// (`mergeConfig` + `mergeConfigConcatArrays`).
pub fn merge_info(target: &mut Info, source: &Info) {
    // Instructions — concatenate and deduplicate
    if !source.instructions.is_empty() {
        let mut combined: Vec<String> = target.instructions.to_vec();
        for inst in &source.instructions {
            if !combined.contains(inst) {
                combined.push(inst.clone());
            }
        }
        target.instructions = combined;
    }

    // Merge commands
    for (key, cmd) in &source.command {
        target
            .command
            .entry(key.clone())
            .or_insert_with(|| cmd.clone());
    }

    // Merge agents
    for (key, agent) in &source.agent {
        target
            .agent
            .entry(key.clone())
            .or_insert_with(|| agent.clone());
    }

    // Merge providers
    for (key, provider) in &source.provider {
        target
            .provider
            .entry(key.clone())
            .or_insert_with(|| provider.clone());
    }

    // Merge MCP configs
    for (key, mcp) in &source.mcp {
        target.mcp.entry(key.clone()).or_insert_with(|| mcp.clone());
    }

    // Merge deprecated mode → agent
    for (key, mode_cfg) in &source.mode {
        target
            .mode
            .entry(key.clone())
            .or_insert_with(|| mode_cfg.clone());
    }

    // Merge references
    for (key, ref_entry) in &source.references {
        target
            .references
            .entry(key.clone())
            .or_insert_with(|| ref_entry.clone());
    }
    for (key, ref_entry) in &source.reference {
        target
            .reference
            .entry(key.clone())
            .or_insert_with(|| ref_entry.clone());
    }

    // Merge plugin specs (concatenate, don't replace)
    if !source.plugin.is_empty() {
        target.plugin.extend(source.plugin.iter().cloned());
    }

    // Scalar and optional fields — source wins if Some
    if source.schema.is_some() {
        target.schema = source.schema.clone();
    }
    if source.shell.is_some() {
        target.shell = source.shell.clone();
    }
    if source.log_level.is_some() {
        target.log_level = source.log_level;
    }
    if source.server.is_some() {
        target.server = source.server.clone();
    }
    if source.skills.is_some() {
        target.skills = source.skills.clone();
    }
    if source.watcher.is_some() {
        target.watcher = source.watcher.clone();
    }
    if source.snapshot.is_some() {
        target.snapshot = source.snapshot;
    }
    if source.share.is_some() {
        target.share = source.share;
    }
    if source.autoshare.is_some() {
        target.autoshare = source.autoshare;
    }
    if source.autoupdate.is_some() {
        target.autoupdate = source.autoupdate.clone();
    }
    if source.model.is_some() {
        target.model = source.model.clone();
    }
    if source.small_model.is_some() {
        target.small_model = source.small_model.clone();
    }
    if source.default_agent.is_some() {
        target.default_agent = source.default_agent.clone();
    }
    if source.username.is_some() {
        target.username = source.username.clone();
    }
    if source.formatter.is_some() {
        target.formatter = source.formatter.clone();
    }
    if source.lsp.is_some() {
        target.lsp = source.lsp.clone();
    }
    if source.layout.is_some() {
        target.layout = source.layout.clone();
    }
    if source.permission.is_some() {
        target.permission = source.permission.clone();
    }
    if source.attachment.is_some() {
        target.attachment = source.attachment.clone();
    }
    if source.enterprise.is_some() {
        target.enterprise = source.enterprise.clone();
    }
    if source.tool_output.is_some() {
        target.tool_output = source.tool_output.clone();
    }
    if source.compaction.is_some() {
        target.compaction = source.compaction.clone();
    }
    if source.experimental.is_some() {
        target.experimental = source.experimental.clone();
    }

    // Merge tools maps
    for (key, val) in &source.tools {
        target.tools.insert(key.clone(), *val);
    }

    // Merge disabled/enabled providers
    if !source.disabled_providers.is_empty() {
        target.disabled_providers = source.disabled_providers.clone();
    }
    if !source.enabled_providers.is_empty() {
        target.enabled_providers = source.enabled_providers.clone();
    }
}

// ── Legacy TOML migration ────────────────────────────────────────────────

/// Attempt to migrate a legacy TOML config file.
///
/// # Source
/// Ported from `packages/blazecode/src/config/config.ts` lines 263–276
/// (legacy config migration block).
fn migrate_legacy_toml(content: &str) -> crate::error::Result<Info> {
    let value: toml::Value = toml::from_str(content)
        .map_err(|e| crate::error::Error::Config(format!("Legacy TOML parse error: {e}")))?;

    let mut info = Info::default();

    if let Some(table) = value.as_table() {
        // provider + model → model field
        if let (Some(provider), Some(model)) = (table.get("provider"), table.get("model")) {
            if let (Some(p), Some(m)) = (provider.as_str(), model.as_str()) {
                info.model = Some(format!("{p}/{m}"));
            }
        }
        info.schema = Some("https://blazecode.ai/config.json".to_owned());

        // Convert remaining TOML keys to JSON and deserialize into Info
        let json_val = toml_to_json_value(&value);
        if let Ok(parsed) = serde_json::from_value(json_val) {
            merge_info(&mut info, &parsed);
        }
    }

    Ok(info)
}

/// Convert a TOML value to a `serde_json::Value`.
fn toml_to_json_value(toml_val: &toml::Value) -> serde_json::Value {
    match toml_val {
        toml::Value::String(s) => serde_json::Value::String(s.clone()),
        toml::Value::Integer(i) => (*i).into(),
        toml::Value::Float(f) => (*f).into(),
        toml::Value::Boolean(b) => (*b).into(),
        toml::Value::Datetime(dt) => serde_json::Value::String(dt.to_string()),
        toml::Value::Array(arr) => {
            serde_json::Value::Array(arr.iter().map(toml_to_json_value).collect())
        }
        toml::Value::Table(tbl) => {
            let mut map = serde_json::Map::new();
            for (k, v) in tbl {
                // Skip provider and model — already handled
                if k == "provider" || k == "model" {
                    continue;
                }
                map.insert(k.clone(), toml_to_json_value(v));
            }
            serde_json::Value::Object(map)
        }
    }
}

// ── Helpers ─────────────────────────────────────────────────────────────

/// Get the OS-specific global config directory.
fn global_config_dir() -> crate::error::Result<PathBuf> {
    let config_dir = dirs::config_dir()
        .ok_or_else(|| crate::error::Error::Config("Cannot determine config directory".into()))?;
    Ok(config_dir.join("blazecode"))
}

/// Find the first existing global config file from the candidate list.
///
/// Candidates: `blazecode.jsonc`, `blazecode.json`, `config.json`
/// Returns the path of the first existing file, or the first candidate if none exist.
///
/// # Source
/// Ported from `packages/blazecode/src/config/config.ts` lines 139–147
/// (`globalConfigFile`).
pub fn global_config_file() -> crate::error::Result<std::path::PathBuf> {
    let config_dir = global_config_dir()?;
    let candidates = ["blazecode.jsonc", "blazecode.json", "config.json"];
    for file in &candidates {
        let path = config_dir.join(file);
        if path.exists() {
            return Ok(path);
        }
    }
    Ok(config_dir.join(candidates[0]))
}

/// Write the schema URL to a new global config file if one doesn't exist.
///
/// # Source
/// Ported from `packages/blazecode/src/config/config.ts` lines 250–257
/// (schema seeding block).
pub fn seed_global_config_schema() -> crate::error::Result<()> {
    let config_dir = global_config_dir()?;
    std::fs::create_dir_all(&config_dir)?;
    let file = config_dir.join("blazecode.jsonc");
    if !file.exists() {
        let content = serde_json::json!({
            "$schema": "https://blazecode.ai/config.json"
        });
        std::fs::write(&file, serde_json::to_string_pretty(&content)?)?;
    }
    Ok(())
}


// ── Config post-processing / normalization ───────────────────────────────

/// Strip legacy TUI keys from parsed config data before deserialization.
///
/// When a user has `theme`, `keybinds`, or `tui` keys in their `blazecode.json`
/// (left over from the TUI migration), these must be removed before the data is
/// deserialized into `Info`, because `Info` no longer has those fields.
///
/// # Source
/// Ported from `packages/blazecode/src/config/config.ts` lines 53–62
/// (`normalizeLoadedConfig`).
pub fn normalize_loaded_config(value: &mut serde_json::Value) {
    if let Some(obj) = value.as_object_mut() {
        let had_legacy = obj.contains_key("theme") || obj.contains_key("keybinds") || obj.contains_key("tui");
        if had_legacy {
            obj.remove("theme");
            obj.remove("keybinds");
            obj.remove("tui");
        }
    }
}

/// Normalize a fully-merged `Info` by applying post-processing rules.
///
/// This mimics the logic in `packages/blazecode/src/config/config.ts`:
///   - `mode` entries are merged into `agent` with `mode: Some(AgentMode::Primary)`
///   - `tools` (deprecated boolean map) is converted to `permission` rules
///   - `autoshare: true` is converted to `share: Some(ShareMode::Auto)`
///   - `username` is set to the system username if not already set
///
/// # Source
/// Ported from `packages/blazecode/src/config/config.ts` lines 411–576.
pub fn normalize_config(config: &mut Info) {
    // Mode → agent flattening (deprecated `mode` field)
    for (name, mode_cfg) in config.mode.clone().iter() {
        let mut entry = mode_cfg.clone();
        if entry.mode.is_none() {
            entry.mode = Some(AgentMode::Primary);
        }
        config
            .agent
            .entry(name.clone())
            .or_insert(entry);
    }
    config.mode.clear();

    // Tools → permission conversion (deprecated `tools` field)
    if !config.tools.is_empty() {
        let mut perm = PermissionConfig::default();
        for (tool, enabled) in config.tools.clone() {
            let action = if enabled {
                PermissionAction::Allow
            } else {
                PermissionAction::Deny
            };
            let rule = PermissionRule::Action(action);
            // Map well-known tool names to permission fields
            match tool.as_str() {
                "write" | "edit" | "patch" => {
                    perm.edit = Some(rule);
                }
                "read" => perm.read = Some(rule),
                "glob" => perm.glob = Some(rule),
                "grep" => perm.grep = Some(rule),
                "list" => perm.list = Some(rule),
                "bash" => perm.bash = Some(rule),
                "task" => perm.task = Some(rule),
                "lsp" => perm.lsp = Some(rule),
                "skill" => perm.skill = Some(rule),
                "external_directory" => perm.external_directory = Some(rule),
                _ => {
                    // Unknown tools go into extra
                    perm.extra.insert(tool, rule);
                }
            }
        }
        // Merge with existing permission — explicit user permission overrides tools-derived rules
        match &mut config.permission {
            Some(existing) => {
                // Override tools-derived defaults with any explicitly-set fields
                if let Some(ref existing_rule) = existing.edit {
                    perm.edit = Some(existing_rule.clone());
                }
                if let Some(ref existing_rule) = existing.read {
                    perm.read = Some(existing_rule.clone());
                }
                if let Some(ref existing_rule) = existing.bash {
                    perm.bash = Some(existing_rule.clone());
                }
                if let Some(ref existing_rule) = existing.glob {
                    perm.glob = Some(existing_rule.clone());
                }
                if let Some(ref existing_rule) = existing.grep {
                    perm.grep = Some(existing_rule.clone());
                }
                if let Some(ref existing_rule) = existing.list {
                    perm.list = Some(existing_rule.clone());
                }
                if let Some(ref existing_rule) = existing.task {
                    perm.task = Some(existing_rule.clone());
                }
                if let Some(ref existing_rule) = existing.lsp {
                    perm.lsp = Some(existing_rule.clone());
                }
                if let Some(ref existing_rule) = existing.skill {
                    perm.skill = Some(existing_rule.clone());
                }
                if let Some(ref existing_rule) = existing.external_directory {
                    perm.external_directory = Some(existing_rule.clone());
                }
                // Copy over extra rules from existing permission
                for (k, v) in existing.extra.clone() {
                    perm.extra.entry(k).or_insert(v);
                }
                config.permission = Some(perm);
            }
            None => {
                config.permission = Some(perm);
            }
        }
        config.tools.clear();
    }

    // autoshare → share conversion
    if config.autoshare == Some(true) && config.share.is_none() {
        config.share = Some(ShareMode::Auto);
    }

    // Username fallback
    if config.username.is_none() {
        config.username = Some(
            std::env::var("USER")
                .or_else(|_| std::env::var("USERNAME"))
                .unwrap_or_else(|_| "user".to_string()),
        );
    }
}

// ── ConfigPaths helpers ──────────────────────────────────────────────────

/// Return the candidate file paths for a named config in a directory.
///
/// Returns `[dir/{name}.json, dir/{name}.jsonc]`.
///
/// # Source
/// Ported from `packages/blazecode/src/config/paths.ts` line 43
/// (`fileInDirectory`).
pub fn config_file_in_directory(dir: &std::path::Path, name: &str) -> Vec<std::path::PathBuf> {
    vec![
        dir.join(format!("{name}.json")),
        dir.join(format!("{name}.jsonc")),
    ]
}

// ── Plugin discovery ─────────────────────────────────────────────────────

/// Discover plugin files under `.blazecode/plugin/` or `.blazecode/plugins/`.
///
/// Scans for `.ts` and `.js` files in those directories.
///
/// # Source
/// Ported from `packages/blazecode/src/config/plugin.ts` lines 18–30
/// (`ConfigPlugin.load`).
pub fn discover_plugin_files(dir: &std::path::Path) -> std::io::Result<Vec<String>> {
    let mut plugins = Vec::new();
    for sub in &["plugin", "plugins"] {
        let plugin_dir = dir.join(sub);
        if plugin_dir.is_dir() {
            if let Ok(entries) = std::fs::read_dir(&plugin_dir) {
                for entry in entries.flatten() {
                    let path = entry.path();
                    let ext = path.extension().and_then(|e| e.to_str());
                    if ext == Some("ts") || ext == Some("js") {
                        // Convert to file:// URL
                        let abs = if path.is_absolute() {
                            path
                        } else {
                            dir.join(&path)
                        };
                        if let Ok(canon) = abs.canonicalize() {
                            let url = format!("file://{}", canon.display());
                            plugins.push(url);
                        } else {
                            plugins.push(format!("file://{}", abs.display()));
                        }
                    }
                }
            }
        }
    }
    Ok(plugins)
}

/// Resolve a path-like plugin spec relative to a config file path.
///
/// # Source
/// Ported from `packages/blazecode/src/config/plugin.ts` lines 42–60
/// (`ConfigPlugin.resolvePluginSpec`).
pub fn resolve_plugin_spec(spec: &PluginSpec, config_filepath: &std::path::Path) -> PluginSpec {
    let spec_str = plugin_specifier(spec);
    // Only path-like specs need resolution
    if !spec_str.starts_with('.') && !spec_str.starts_with('/') && !spec_str.starts_with("file://")
    {
        return spec.clone();
    }
    let base = config_filepath.parent().unwrap_or(std::path::Path::new("."));
    let resolved = if spec_str.starts_with("file://") {
        std::path::PathBuf::from(spec_str.trim_start_matches("file://"))
    } else if std::path::Path::new(&spec_str).is_absolute() {
        std::path::PathBuf::from(&spec_str)
    } else {
        base.join(&spec_str)
    };
    let resolved_str = format!("file://{}", resolved.display());
    match spec {
        PluginSpec::WithOptions(_, opts) => PluginSpec::WithOptions(resolved_str, opts.clone()),
        PluginSpec::Simple(_) => PluginSpec::Simple(resolved_str),
    }
}

/// Extract the string specifier from a `PluginSpec`.
///
/// # Source
/// Ported from `packages/blazecode/src/config/plugin.ts` lines 32–34
/// (`pluginSpecifier`).
pub fn plugin_specifier(spec: &PluginSpec) -> String {
    match spec {
        PluginSpec::Simple(s) => s.clone(),
        PluginSpec::WithOptions(s, _) => s.clone(),
    }
}

/// Extract the options from a `PluginSpec`, if any.
///
/// # Source
/// Ported from `packages/blazecode/src/config/plugin.ts` lines 36–38
/// (`pluginOptions`).
pub fn plugin_options(spec: &PluginSpec) -> Option<&HashMap<String, serde_json::Value>> {
    match spec {
        PluginSpec::Simple(_) => None,
        PluginSpec::WithOptions(_, opts) => Some(opts),
    }
}

/// Deduplicate a list of plugin origins by identity.
///
/// Later entries win (overridden by higher-priority config). Returns
/// origins in their original order with duplicates removed.
///
/// # Source
/// Ported from `packages/blazecode/src/config/plugin.ts` lines 64–77
/// (`deduplicatePluginOrigins`).
pub fn deduplicate_plugin_origins(plugins: Vec<PluginOrigin>) -> Vec<PluginOrigin> {
    let mut seen = std::collections::HashSet::new();
    let mut result = Vec::new();
    for origin in plugins.into_iter().rev() {
        let ident = plugin_specifier(&origin.spec);
        if seen.insert(ident) {
            result.push(origin);
        }
    }
    result.reverse();
    result
}

// ── Frontmatter parsing for markdown config files ────────────────────

/// Parse YAML frontmatter from markdown text.
///
/// Looks for `---\n` delimiters at the start of the content.
/// Returns the parsed frontmatter as a HashMap and the body text.
///
/// Falls back to sanitizing frontmatter for unquoted colons in values
/// (common in other coding agent configs) before retrying YAML parse.
///
/// # Source
/// Ported from `packages/core/src/config/markdown.ts` `ConfigMarkdown.parse`
/// and `packages/blazecode/src/config/markdown.ts` `ConfigMarkdown.parse`.
pub fn parse_frontmatter(text: &str) -> Option<(HashMap<String, serde_json::Value>, String)> {
    let text = text.trim();
    if !text.starts_with("---") {
        return None;
    }

    // Skip past opening ---
    let rest = &text[3..];
    let rest = rest.strip_prefix('\n').or_else(|| rest.strip_prefix("\r\n"))?;

    // Find the closing ---
    let closing = rest.find("\n---")?;
    let frontmatter_str = &rest[..closing];
    let after_closing = &rest[closing + 4..];
    let body = after_closing.trim().to_string();

    // Try to parse YAML; fallback to sanitization for unquoted colons
    match serde_yaml::from_str::<HashMap<String, serde_json::Value>>(frontmatter_str) {
        Ok(data) => Some((data, body)),
        Err(_) => sanitize_frontmatter(frontmatter_str)
            .and_then(|s| serde_yaml::from_str::<HashMap<String, serde_json::Value>>(&s).ok())
            .map(|data| (data, body)),
    }
}

/// Sanitize frontmatter to handle unquoted colons in values
/// (common in other coding agent configs).
///
/// Converts lines like `description: foo: bar` to block scalar syntax:
/// ```yaml
/// description: |-
///   foo: bar
/// ```
///
/// # Source
/// Ported from `packages/core/src/config/markdown.ts` `ConfigMarkdown.sanitize`.
fn sanitize_frontmatter(frontmatter: &str) -> Option<String> {
    let mut result: Vec<String> = Vec::new();
    for line in frontmatter.lines() {
        let trimmed = line.trim();
        if trimmed.starts_with('#') || trimmed.is_empty() || line.starts_with(' ') || line.starts_with('\t') {
            result.push(line.to_string());
            continue;
        }
        if let Some(colon_pos) = line.find(':') {
            let key_part = &line[..colon_pos];
            let key_trimmed = key_part.trim();
            if key_trimmed.is_empty() || !key_trimmed.chars().next().map_or(false, |c| c.is_alphabetic() || c == '_') {
                result.push(line.to_string());
                continue;
            }
            let value = line[colon_pos + 1..].trim();
            if value.is_empty()
                || value == ">"
                || value == "|"
                || value.starts_with('"')
                || value.starts_with('\'')
                || !value.contains(':')
            {
                result.push(line.to_string());
            } else {
                result.push(format!("{}: |-", key_trimmed));
                result.push(format!("  {}", value));
            }
        } else {
            result.push(line.to_string());
        }
    }
    Some(result.join("\n"))
}

/// Parse an agent markdown file into an [`AgentConfig`].
///
/// Reads the file, extracts YAML frontmatter, and uses body as prompt.
/// The config name is derived from the filename if not in frontmatter.
///
/// # Source
/// Ported from `packages/blazecode/src/config/agent.ts` `ConfigAgent.load`
/// (lines 11–32).
pub fn parse_agent_md(path: &Path) -> crate::error::Result<AgentConfig> {
    let text = std::fs::read_to_string(path).map_err(crate::error::Error::Io)?;

    let (frontmatter, body) = parse_frontmatter(&text).ok_or_else(|| {
        crate::error::Error::Config(format!("Missing or invalid frontmatter in {:?}", path))
    })?;

    let map: serde_json::Map<String, serde_json::Value> = frontmatter.into_iter().collect();
    let mut config: AgentConfig =
        serde_json::from_value(serde_json::Value::Object(map)).map_err(|e| {
            crate::error::Error::Config(format!(
                "Failed to deserialize agent config from {:?}: {}",
                path, e
            ))
        })?;

    if config.name.is_none() {
        config.name = path
            .file_stem()
            .and_then(|s| s.to_str())
            .map(|s| s.to_string());
    }

    config.prompt = Some(body);
    Ok(config)
}

/// Parse a command markdown file into a [`CommandConfig`].
///
/// Reads the file, extracts YAML frontmatter, and uses body as template.
///
/// # Source
/// Ported from `packages/blazecode/src/config/command.ts` `ConfigCommand.load`
/// (lines 13–39).
pub fn parse_command_md(path: &Path) -> crate::error::Result<CommandConfig> {
    let text = std::fs::read_to_string(path).map_err(crate::error::Error::Io)?;

    let (frontmatter, body) = parse_frontmatter(&text).ok_or_else(|| {
        crate::error::Error::Config(format!("Missing or invalid frontmatter in {:?}", path))
    })?;

    let map: serde_json::Map<String, serde_json::Value> = frontmatter.into_iter().collect();
    let mut config: CommandConfig =
        serde_json::from_value(serde_json::Value::Object(map)).map_err(|e| {
            crate::error::Error::Config(format!(
                "Failed to deserialize command config from {:?}: {}",
                path, e
            ))
        })?;

    config.template = body;
    Ok(config)
}

// ── Agent discovery from .blazecode/agent(s) directories ─────────────────

/// Discover agent markdown files under `.blazecode/agent/` or `.blazecode/agents/`.
///
/// Returns parsed [`AgentConfig`] entries.
///
/// # Source
/// Ported from `packages/blazecode/src/config/agent.ts` lines 11–32
/// (`ConfigAgent.load`).
pub fn discover_agent_files(dir: &Path) -> crate::error::Result<Vec<AgentConfig>> {
    let mut agents = Vec::new();
    for sub in &["agent", "agents"] {
        let agent_dir = dir.join(sub);
        if agent_dir.is_dir() {
            if let Ok(entries) = find_files_recursive(&agent_dir, "md") {
                for path_str in entries {
                    let path = Path::new(&path_str);
                    if let Ok(config) = parse_agent_md(path) {
                        agents.push(config);
                    }
                }
            }
        }
    }
    Ok(agents)
}

/// Discover mode markdown files under `.blazecode/mode/` or `.blazecode/modes/`.
///
/// # Source
/// Ported from `packages/blazecode/src/config/agent.ts` lines 34–59
/// (`ConfigAgent.loadMode`).
pub fn discover_mode_files(dir: &Path) -> crate::error::Result<Vec<String>> {
    let mut files = Vec::new();
    for sub in &["mode", "modes"] {
        let mode_dir = dir.join(sub);
        if mode_dir.is_dir() {
            if let Ok(entries) = find_files_recursive(&mode_dir, "md") {
                files.extend(entries);
            }
        }
    }
    Ok(files)
}

/// Discover command markdown files under `.blazecode/command/` or `.blazecode/commands/`.
///
/// Returns parsed [`CommandConfig`] entries.
///
/// # Source
/// Ported from `packages/blazecode/src/config/command.ts` lines 13–39
/// (`ConfigCommand.load`).
pub fn discover_command_files(dir: &Path) -> crate::error::Result<Vec<CommandConfig>> {
    let mut commands = Vec::new();
    for sub in &["command", "commands"] {
        let cmd_dir = dir.join(sub);
        if cmd_dir.is_dir() {
            if let Ok(entries) = find_files_recursive(&cmd_dir, "md") {
                for path_str in entries {
                    let path = Path::new(&path_str);
                    if let Ok(config) = parse_command_md(path) {
                        commands.push(config);
                    }
                }
            }
        }
    }
    Ok(commands)
}

/// Recursively find files with a given extension under a directory.
fn find_files_recursive(dir: &std::path::Path, ext: &str) -> std::io::Result<Vec<String>> {
    let mut files = Vec::new();
    if dir.is_dir() {
        let mut stack = vec![dir.to_path_buf()];
        while let Some(current) = stack.pop() {
            if let Ok(entries) = std::fs::read_dir(&current) {
                for entry in entries.flatten() {
                    let path = entry.path();
                    if path.is_dir() {
                        if stack.len() < 100 {
                            stack.push(path);
                        }
                    } else if path.is_file() {
                        if let Some(e) = path.extension() {
                            if e == ext {
                                files.push(path.to_string_lossy().to_string());
                            }
                        }
                    }
                }
            }
        }
    }
    Ok(files)
}

// ── Managed config support ──────────────────────────────────────────────

/// Return the system-level managed config directory.
///
/// On macOS: `/Library/Application Support/blazecode`
/// On Windows: `%ProgramData%/blazecode`
/// On Linux: `/etc/blazecode`
///
/// # Source
/// Ported from `packages/blazecode/src/config/managed.ts` lines 20–29
/// (`systemManagedConfigDir`).
pub fn system_managed_config_dir() -> std::path::PathBuf {
    #[cfg(target_os = "macos")]
    {
        std::path::PathBuf::from("/Library/Application Support/blazecode")
    }
    #[cfg(target_os = "windows")]
    {
        let pd = std::env::var("ProgramData").unwrap_or_else(|_| r"C:\ProgramData".to_string());
        std::path::PathBuf::from(pd).join("blazecode")
    }
    #[cfg(not(any(target_os = "macos", target_os = "windows")))]
    {
        std::path::PathBuf::from("/etc/blazecode")
    }
}

/// Return the managed config directory (overridable via env var).
///
/// # Source
/// Ported from `packages/blazecode/src/config/managed.ts` lines 31–33
/// (`managedConfigDir`).
pub fn managed_config_dir() -> std::path::PathBuf {
    std::env::var("BLAZECODE_TEST_MANAGED_CONFIG_DIR")
        .map(std::path::PathBuf::from)
        .unwrap_or_else(|_| system_managed_config_dir())
}

/// Parse a managed plist JSON string to strip plist meta keys.
///
/// # Source
/// Ported from `packages/blazecode/src/config/managed.ts` lines 35–41
/// (`parseManagedPlist`).
pub fn parse_managed_plist(json: &str) -> crate::error::Result<String> {
    let mut value: serde_json::Value = serde_json::from_str(json)?;
    if let Some(obj) = value.as_object_mut() {
        let meta_keys: Vec<String> = [
            "PayloadDisplayName",
            "PayloadIdentifier",
            "PayloadType",
            "PayloadUUID",
            "PayloadVersion",
            "_manualProfile",
        ]
        .iter()
        .map(|s| s.to_string())
        .collect();
        for key in meta_keys {
            obj.remove(&key);
        }
    }
    serde_json::to_string(&value).map_err(crate::error::Error::Json)
}

/// Read macOS managed preferences (MDM-deployed `.mobileconfig`).
///
/// Returns the config text if a managed plist was found, or `None`.
/// Only supported on macOS; returns `None` on other platforms.
///
/// # Source
/// Ported from `packages/blazecode/src/config/managed.ts` lines 43–69
/// (`readManagedPreferences`).
pub fn read_managed_preferences() -> Option<(String, String)> {
    #[cfg(target_os = "macos")]
    {
        let domain = "ai.blazecode.managed";
        let user = std::env::var("USER").unwrap_or_else(|_| "user".to_string());
        let plist_paths = vec![
            std::path::PathBuf::from(format!("/Library/Managed Preferences/{user}/{domain}.plist")),
            std::path::PathBuf::from(format!("/Library/Managed Preferences/{domain}.plist")),
        ];
        for plist in plist_paths {
            if !plist.exists() {
                continue;
            }
            // Attempt to convert plist to JSON using plutil
            let output = std::process::Command::new("plutil")
                .args(["-convert", "json", "-o", "-", &plist.to_string_lossy()])
                .output()
                .ok()?;
            if !output.status.success() {
                continue;
            }
            let stdout = String::from_utf8_lossy(&output.stdout);
            if let Ok(parsed) = parse_managed_plist(&stdout) {
                return Some((format!("mobileconfig:{}", plist.display()), parsed));
            }
        }
        None
    }
    #[cfg(not(target_os = "macos"))]
    {
        None
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // -- JSONC parsing tests -------------------------------------------------

    #[test]
    fn test_parse_jsonc_with_line_comments() {
        let input = r#"{
            // This is a comment
            "key": "value"
        }"#;
        let result = parse_jsonc(input, std::path::Path::new("test.jsonc")).unwrap();
        assert_eq!(result["key"], "value");
    }

    #[test]
    fn test_parse_jsonc_with_block_comments() {
        let input = r#"{
            /* multi-line
               block comment */
            "key": "value"
        }"#;
        let result = parse_jsonc(input, std::path::Path::new("test.jsonc")).unwrap();
        assert_eq!(result["key"], "value");
    }

    #[test]
    fn test_parse_jsonc_trailing_comma() {
        let input = r#"{
            "a": 1,
            "b": 2,
        }"#;
        let result = parse_jsonc(input, std::path::Path::new("test.jsonc")).unwrap();
        assert_eq!(result["a"], 1);
        assert_eq!(result["b"], 2);
    }

    #[test]
    fn test_parse_jsonc_trailing_comma_in_array() {
        let input = r#"{"arr": [1, 2, 3,]}"#;
        let result = parse_jsonc(input, std::path::Path::new("test.jsonc")).unwrap();
        assert_eq!(result["arr"].as_array().unwrap().len(), 3);
    }

    #[test]
    fn test_parse_jsonc_comment_in_string() {
        let input = r#"{"url": "https://example.com/path"}"#;
        let result = parse_jsonc(input, std::path::Path::new("test.jsonc")).unwrap();
        assert_eq!(result["url"], "https://example.com/path");
    }

    // -- Schema validation tests ---------------------------------------------

    #[test]
    fn test_validate_info_basic() {
        let input = serde_json::json!({"model": "anthropic/claude-sonnet-4-6"});
        let info = validate_info(input, std::path::Path::new("test.json")).unwrap();
        assert_eq!(info.model.unwrap(), "anthropic/claude-sonnet-4-6");
    }

    #[test]
    fn test_validate_info_unrecognized_key() {
        let input = serde_json::json!({"nonexistent_key": true});
        let result = validate_info(input, std::path::Path::new("test.json"));
        assert!(result.is_err());
        let err = result.unwrap_err().to_string();
        assert!(err.contains("Unrecognised key"));
        assert!(err.contains("nonexistent_key"));
    }

    #[test]
    fn test_validate_info_empty() {
        let input = serde_json::json!({});
        let info = validate_info(input, std::path::Path::new("test.json")).unwrap();
        assert!(info.model.is_none());
        assert!(info.shell.is_none());
    }

    // -- Variable substitution tests -----------------------------------------

    #[test]
    fn test_substitute_env_var() {
        std::env::set_var("BLAZECODE_CFG_TEST_SUBST_VAR", "test_value");
        let result = substitute_variables(
            "prefix {env:BLAZECODE_CFG_TEST_SUBST_VAR} suffix",
            std::path::Path::new("."),
            None,
        )
        .unwrap();
        assert_eq!(result, "prefix test_value suffix");
        std::env::remove_var("BLAZECODE_CFG_TEST_SUBST_VAR");
    }

    #[test]
    fn test_substitute_missing_env_var() {
        let result = substitute_variables(
            "{env:NONEXISTENT_VAR_12345}",
            std::path::Path::new("."),
            None,
        )
        .unwrap();
        assert_eq!(result, "");
    }

    #[test]
    fn test_substitute_no_placeholders() {
        let result = substitute_variables("plain text", std::path::Path::new("."), None).unwrap();
        assert_eq!(result, "plain text");
    }

    // -- Config merging tests ------------------------------------------------

    #[test]
    fn test_merge_instructions_concatenates() {
        let mut target = Info {
            instructions: vec!["a".into(), "b".into()],
            ..Default::default()
        };
        let source = Info {
            instructions: vec!["b".into(), "c".into()],
            ..Default::default()
        };
        merge_info(&mut target, &source);
        assert_eq!(target.instructions, vec!["a", "b", "c"]);
    }

    #[test]
    fn test_merge_scalar_source_wins() {
        let mut target = Info {
            model: Some("old/model".into()),
            ..Default::default()
        };
        let source = Info {
            model: Some("new/model".into()),
            ..Default::default()
        };
        merge_info(&mut target, &source);
        assert_eq!(target.model.unwrap(), "new/model");
    }

    #[test]
    fn test_merge_scalar_target_kept() {
        let mut target = Info {
            shell: Some("/bin/zsh".into()),
            ..Default::default()
        };
        let source = Info::default();
        merge_info(&mut target, &source);
        assert_eq!(target.shell.unwrap(), "/bin/zsh");
    }

    // -- MCP config tests ----------------------------------------------------

    #[test]
    fn test_mcp_config_local() {
        let json = serde_json::json!({
            "type": "local",
            "command": ["node", "server.js"],
            "enabled": true,
            "timeout": 10000
        });
        let cfg: McpConfig = serde_json::from_value(json).unwrap();
        match cfg {
            McpConfig::Local {
                command,
                enabled,
                timeout,
                ..
            } => {
                assert_eq!(command, vec!["node", "server.js"]);
                assert_eq!(enabled, Some(true));
                assert_eq!(timeout, Some(10000));
            }
            _ => panic!("expected Local"),
        }
    }

    #[test]
    fn test_mcp_config_remote() {
        let json = serde_json::json!({
            "type": "remote",
            "url": "https://mcp.example.com",
            "headers": {"Authorization": "Bearer token"}
        });
        let cfg: McpConfig = serde_json::from_value(json).unwrap();
        match cfg {
            McpConfig::Remote { url, headers, .. } => {
                assert_eq!(url, "https://mcp.example.com");
                assert_eq!(headers.get("Authorization").unwrap(), "Bearer token");
            }
            _ => panic!("expected Remote"),
        }
    }

    // -- Provider config tests -----------------------------------------------

    #[test]
    fn test_provider_config_with_models() {
        let json = serde_json::json!({
            "api": "anthropic",
            "env": ["ANTHROPIC_API_KEY"],
            "models": {
                "claude-sonnet-4-6": {
                    "cost": {
                        "input": 3.0,
                        "output": 15.0
                    }
                }
            }
        });
        let cfg: ProviderConfig = serde_json::from_value(json).unwrap();
        assert_eq!(cfg.api.unwrap(), "anthropic");
        assert_eq!(cfg.env, vec!["ANTHROPIC_API_KEY"]);
        assert!(cfg.models.contains_key("claude-sonnet-4-6"));
    }

    // -- Agent config normalization test -------------------------------------

    #[test]
    fn test_agent_config_tools_to_permission() {
        let json = serde_json::json!({
            "name": "test-agent",
            "mode": "primary",
            "tools": {"bash": true, "write": false},
            "permission": {"read": "allow"}
        });
        let cfg: AgentConfig = serde_json::from_value(json).unwrap();
        assert_eq!(cfg.name.unwrap(), "test-agent");
        assert_eq!(cfg.mode.unwrap(), AgentMode::Primary);
        assert_eq!(cfg.tools.get("bash"), Some(&true));
        assert_eq!(cfg.tools.get("write"), Some(&false));
        assert!(cfg.permission.is_some());
    }

    // -- Permission config tests ---------------------------------------------

    #[test]
    fn test_permission_config_action() {
        let json = serde_json::json!({"read": "allow", "edit": "ask"});
        let cfg: PermissionConfig = serde_json::from_value(json).unwrap();
        match cfg.read.unwrap() {
            PermissionRule::Action(PermissionAction::Allow) => {}
            _ => panic!("expected Allow"),
        }
    }

    #[test]
    fn test_permission_config_wildcard() {
        let json = serde_json::json!({"*": "deny"});
        let cfg: PermissionConfig = serde_json::from_value(json).unwrap();
        assert_eq!(cfg.wildcard, Some(PermissionAction::Deny));
    }

    #[test]
    fn test_autoupdate_untagged() {
        let json_bool = serde_json::json!(true);
        let v: AutoUpdate = serde_json::from_value(json_bool).unwrap();
        match v {
            AutoUpdate::Bool(true) => {}
            _ => panic!("expected Bool(true)"),
        }

        let json_str = serde_json::json!("notify");
        let v: AutoUpdate = serde_json::from_value(json_str).unwrap();
        match v {
            AutoUpdate::Notify(s) => assert_eq!(s, "notify"),
            _ => panic!("expected Notify"),
        }
    }

    // ── JSONC edge cases ─────────────────────────────────────────────────

    #[test]
    fn test_parse_jsonc_nested_comments() {
        // Block comment containing what looks like a string with slashes
        let input = r#"{
            /* comment with "quotes" and // slashes */
            "key": "value"
        }"#;
        let result = parse_jsonc(input, std::path::Path::new("test.jsonc")).unwrap();
        assert_eq!(result["key"], "value");
    }

    #[test]
    fn test_parse_jsonc_escaped_quotes_in_string() {
        let input = r#"{"message": "She said \"hello\" to me"}"#;
        let result = parse_jsonc(input, std::path::Path::new("test.jsonc")).unwrap();
        assert_eq!(result["message"], "She said \"hello\" to me");
    }

    #[test]
    fn test_parse_jsonc_unicode_in_strings() {
        let input = r#"{"greeting": "こんにちは", "emoji": "🚀"}"#;
        let result = parse_jsonc(input, std::path::Path::new("test.jsonc")).unwrap();
        assert_eq!(result["greeting"], "こんにちは");
        assert_eq!(result["emoji"], "🚀");
    }

    #[test]
    fn test_parse_jsonc_line_comment_after_value() {
        let input = r#"{
            "key": "value" // inline comment
        }"#;
        let result = parse_jsonc(input, std::path::Path::new("test.jsonc")).unwrap();
        assert_eq!(result["key"], "value");
    }

    #[test]
    fn test_parse_jsonc_multiple_trailing_commas() {
        let input = r#"{
            "a": 1,
            "b": 2,
            "c": 3,
        }"#;
        let result = parse_jsonc(input, std::path::Path::new("test.jsonc")).unwrap();
        assert_eq!(result["a"], 1);
        assert_eq!(result["b"], 2);
        assert_eq!(result["c"], 3);
    }

    #[test]
    fn test_parse_jsonc_empty_object_with_comments() {
        let input = r#"{
            // nothing here
            /* really */
        }"#;
        let result = parse_jsonc(input, std::path::Path::new("test.jsonc")).unwrap();
        assert!(result.as_object().unwrap().is_empty());
    }

    #[test]
    fn test_parse_jsonc_block_comment_between_fields() {
        let input = r#"{
            "a": 1,
            /* separator */
            "b": 2
        }"#;
        let result = parse_jsonc(input, std::path::Path::new("test.jsonc")).unwrap();
        assert_eq!(result["a"], 1);
        assert_eq!(result["b"], 2);
    }

    #[test]
    fn test_parse_jsonc_single_line_block_comment() {
        let input = r#"{/* short */ "key": true}"#;
        let result = parse_jsonc(input, std::path::Path::new("test.jsonc")).unwrap();
        assert_eq!(result["key"], true);
    }

    // ── Variable substitution edge cases ──────────────────────────────────

    #[test]
    fn test_substitute_with_custom_env_map() {
        let mut env = HashMap::new();
        env.insert("CUSTOM_KEY".into(), "custom_value".into());
        let result =
            substitute_variables("{env:CUSTOM_KEY}", std::path::Path::new("."), Some(&env))
                .unwrap();
        assert_eq!(result, "custom_value");
    }

    #[test]
    fn test_substitute_multiple_env_vars() {
        std::env::set_var("BLAZECODE_VAR_A", "alpha");
        std::env::set_var("BLAZECODE_VAR_B", "beta");
        let result = substitute_variables(
            "{env:BLAZECODE_VAR_A} and {env:BLAZECODE_VAR_B}",
            std::path::Path::new("."),
            None,
        )
        .unwrap();
        assert_eq!(result, "alpha and beta");
        std::env::remove_var("BLAZECODE_VAR_A");
        std::env::remove_var("BLAZECODE_VAR_B");
    }

    #[test]
    fn test_substitute_env_var_in_json_context() {
        std::env::set_var("BLAZECODE_MODEL", "claude-sonnet");
        let result = substitute_variables(
            r#"{"model": "{env:BLAZECODE_MODEL}"}"#,
            std::path::Path::new("."),
            None,
        )
        .unwrap();
        assert_eq!(result, r#"{"model": "claude-sonnet"}"#);
        std::env::remove_var("BLAZECODE_MODEL");
    }

    #[test]
    fn test_substitute_comment_line_keeps_file_token() {
        // When a {file:...} token appears on a commented line, it should be kept verbatim
        let result = substitute_variables(
            "// {file:./example.txt}\n{\"key\": \"value\"}",
            std::path::Path::new("."),
            None,
        )
        .unwrap();
        assert!(result.contains("{file:./example.txt}"));
        assert!(result.contains("\"key\""));
    }

    // ── Info field coverage tests ────────────────────────────────────────

    #[test]
    fn test_info_all_fields_deserialize() {
        let json = serde_json::json!({
            "$schema": "https://blazecode.ai/config.json",
            "shell": "/bin/bash",
            "logLevel": "INFO",
            "server": {
                "port": 3000,
                "hostname": "localhost"
            },
            "command": {
                "test": {
                    "template": "echo hello"
                }
            },
            "skills": {
                "paths": ["./skills"],
                "urls": ["https://example.com/skills"]
            },
            "references": {
                "mylib": {
                    "repository": "https://github.com/example/repo",
                    "branch": "main"
                }
            },
            "watcher": {
                "ignore": ["node_modules", ".git"]
            },
            "snapshot": true,
            "plugin": ["my-plugin"],
            "share": "manual",
            "autoupdate": "notify",
            "disabledProviders": ["bedrock"],
            "enabledProviders": ["anthropic"],
            "model": "anthropic/claude-sonnet-4-6",
            "smallModel": "anthropic/claude-haiku",
            "defaultAgent": "build",
            "username": "test-user",
            "agent": {
                "build": {
                    "name": "build-agent",
                    "mode": "primary",
                    "steps": 25
                }
            },
            "provider": {
                "anthropic": {
                    "api": "anthropic",
                    "env": ["ANTHROPIC_API_KEY"]
                }
            },
            "mcp": {
                "filesystem": {
                    "type": "local",
                    "command": ["npx", "-y", "@modelcontextprotocol/server-filesystem"]
                }
            },
            "formatter": true,
            "lsp": true,
            "instructions": ["Always use TypeScript", "No console.log"],
            "permission": {
                "bash": "ask",
                "read": { "*.env": "deny", "*.ts": "allow" }
            },
            "tools": {
                "bash": true,
                "python": false
            },
            "attachment": {
                "image": {
                    "maxWidth": 1920,
                    "maxHeight": 1080
                }
            },
            "enterprise": {
                "url": "https://enterprise.example.com"
            },
            "toolOutput": {
                "maxLines": 1000,
                "maxBytes": 50000
            },
            "compaction": {
                "auto": true,
                "prune": false
            },
            "experimental": {
                "batchTool": true
            }
        });

        let info: Info = serde_json::from_value(json).unwrap();
        assert_eq!(info.schema.unwrap(), "https://blazecode.ai/config.json");
        assert_eq!(info.shell.unwrap(), "/bin/bash");
        assert_eq!(info.log_level.unwrap(), LogLevel::Info);
        assert!(info.server.is_some());
        assert_eq!(info.command.len(), 1);
        assert!(info.skills.is_some());
        assert_eq!(info.references.len(), 1);
        assert!(info.watcher.is_some());
        assert_eq!(info.snapshot, Some(true));
        assert_eq!(info.plugin.len(), 1);
        assert!(matches!(info.share.unwrap(), ShareMode::Manual));
        assert_eq!(info.disabled_providers, vec!["bedrock"]);
        assert_eq!(info.enabled_providers, vec!["anthropic"]);
        assert_eq!(info.model.unwrap(), "anthropic/claude-sonnet-4-6");
        assert_eq!(info.small_model.unwrap(), "anthropic/claude-haiku");
        assert_eq!(info.default_agent.unwrap(), "build");
        assert_eq!(info.username.unwrap(), "test-user");
        assert_eq!(info.agent.len(), 1);
        assert_eq!(info.provider.len(), 1);
        assert_eq!(info.mcp.len(), 1);
        assert!(info.formatter.is_some());
        assert!(info.lsp.is_some());
        assert_eq!(info.instructions.len(), 2);
        assert!(info.permission.is_some());
        assert_eq!(info.tools.len(), 2);
        assert!(info.attachment.is_some());
        assert!(info.enterprise.is_some());
        assert!(info.tool_output.is_some());
        assert!(info.compaction.is_some());
        assert!(info.experimental.is_some());
    }

    #[test]
    fn test_info_default_is_empty() {
        let info = Info::default();
        assert!(info.schema.is_none());
        assert!(info.shell.is_none());
        assert!(info.log_level.is_none());
        assert!(info.server.is_none());
        assert!(info.command.is_empty());
        assert!(info.skills.is_none());
        assert!(info.references.is_empty());
        assert!(info.watcher.is_none());
        assert!(info.snapshot.is_none());
        assert!(info.plugin.is_empty());
        assert!(info.share.is_none());
        assert!(info.model.is_none());
        assert!(info.agent.is_empty());
        assert!(info.provider.is_empty());
        assert!(info.mcp.is_empty());
        assert!(info.instructions.is_empty());
        assert!(info.permission.is_none());
        assert!(info.tools.is_empty());
    }

    #[test]
    fn test_parse_jsonc_boolean_and_number_values() {
        let input = r#"{
            "enabled": true,
            "disabled": false,
            "count": 42,
            "price": 9.99,
            "null_field": null
        }"#;
        let result = parse_jsonc(input, std::path::Path::new("test.jsonc")).unwrap();
        assert_eq!(result["enabled"], true);
        assert_eq!(result["disabled"], false);
        assert_eq!(result["count"], 42);
        assert_eq!(result["price"], 9.99);
        assert!(result["null_field"].is_null());
    }

    #[test]
    fn test_substitute_variables_empty_template() {
        let result = substitute_variables("", std::path::Path::new("."), None).unwrap();
        assert_eq!(result, "");
    }

    // ── Config discovery integration test ────────────────────────────────

    #[test]
    fn test_discover_config_files_current_dir() {
        // Discover from the current directory should not error
        let files = discover_config_files("blazecode", std::path::Path::new("."), None);
        // May or may not find files, but should not error
        assert!(files.is_ok());
    }

    #[test]
    fn test_discover_blazecode_dirs_current_dir() {
        let dirs = discover_blazecode_dirs(std::path::Path::new("."), None);
        assert!(dirs.is_ok());
    }

    // ── Config merging: all fields ────────────────────────────────────────

    #[test]
    fn test_merge_all_scalar_fields() {
        let mut target = Info::default();
        let source = Info {
            schema: Some("https://schema.test".into()),
            shell: Some("/bin/fish".into()),
            log_level: Some(LogLevel::Debug),
            snapshot: Some(false),
            share: Some(ShareMode::Disabled),
            model: Some("test/model".into()),
            small_model: Some("test/small".into()),
            default_agent: Some("test-agent".into()),
            username: Some("tester".into()),
            ..Default::default()
        };
        merge_info(&mut target, &source);
        assert_eq!(target.schema.unwrap(), "https://schema.test");
        assert_eq!(target.shell.unwrap(), "/bin/fish");
        assert_eq!(target.log_level.unwrap(), LogLevel::Debug);
        assert!(!target.snapshot.unwrap());
        assert!(matches!(target.share.unwrap(), ShareMode::Disabled));
        assert_eq!(target.model.unwrap(), "test/model");
        assert_eq!(target.small_model.unwrap(), "test/small");
        assert_eq!(target.default_agent.unwrap(), "test-agent");
        assert_eq!(target.username.unwrap(), "tester");
    }

    #[test]
    fn test_merge_providers_and_agents() {
        let mut target = Info {
            agent: {
                let mut m = HashMap::new();
                m.insert(
                    "existing".into(),
                    AgentConfig {
                        name: Some("existing".into()),
                        ..Default::default()
                    },
                );
                m
            },
            provider: {
                let mut m = HashMap::new();
                m.insert("old-prov".into(), ProviderConfig::default());
                m
            },
            ..Default::default()
        };
        let source = Info {
            agent: {
                let mut m = HashMap::new();
                m.insert(
                    "new-agent".into(),
                    AgentConfig {
                        name: Some("new-agent".into()),
                        ..Default::default()
                    },
                );
                m
            },
            provider: {
                let mut m = HashMap::new();
                m.insert("new-prov".into(), ProviderConfig::default());
                m
            },
            ..Default::default()
        };

        merge_info(&mut target, &source);

        // Existing entries should be preserved
        assert!(target.agent.contains_key("existing"));
        assert!(target.provider.contains_key("old-prov"));
        // New entries should be added
        assert!(target.agent.contains_key("new-agent"));
        assert!(target.provider.contains_key("new-prov"));
    }

    #[test]
    fn test_merge_disabled_enabled_providers() {
        let mut target = Info {
            disabled_providers: vec!["a".into()],
            ..Default::default()
        };
        let source = Info {
            disabled_providers: vec!["b".into(), "c".into()],
            enabled_providers: vec!["d".into()],
            ..Default::default()
        };
        merge_info(&mut target, &source);
        // Source overrides disabled_providers (full replacement)
        assert_eq!(target.disabled_providers, vec!["b", "c"]);
        assert_eq!(target.enabled_providers, vec!["d"]);
    }

    // ── Normalize config tests ──────────────────────────────────────────

    #[test]
    fn test_normalize_mode_to_agent() {
        let mut info = Info {
            mode: {
                let mut m = std::collections::HashMap::new();
                m.insert("build".into(), AgentConfig {
                    name: Some("build-agent".into()),
                    ..Default::default()
                });
                m
            },
            ..Default::default()
        };
        normalize_config(&mut info);
        // mode should be empty after normalization
        assert!(info.mode.is_empty());
        // agent should have the entry with mode = primary
        assert!(info.agent.contains_key("build"));
        assert_eq!(info.agent["build"].mode, Some(AgentMode::Primary));
    }

    #[test]
    fn test_normalize_tools_to_permission() {
        let mut info = Info {
            tools: {
                let mut m = std::collections::HashMap::new();
                m.insert("bash".into(), true);
                m.insert("write".into(), false);
                m.insert("read".into(), true);
                m
            },
            ..Default::default()
        };
        normalize_config(&mut info);
        // tools should be empty after normalization
        assert!(info.tools.is_empty());
        // permission should be set
        let perm = info.permission.expect("permission should be set");
        match perm.bash {
            Some(crate::config::PermissionRule::Action(crate::config::PermissionAction::Allow)) => {}
            _ => panic!("bash should be Allow"),
        }
        match perm.edit {
            Some(crate::config::PermissionRule::Action(crate::config::PermissionAction::Deny)) => {}
            _ => panic!("edit (from write) should be Deny"),
        }
    }

    #[test]
    fn test_normalize_autoshare() {
        let mut info = Info {
            autoshare: Some(true),
            ..Default::default()
        };
        normalize_config(&mut info);
        assert_eq!(info.share, Some(crate::config::ShareMode::Auto));
    }

    #[test]
    fn test_normalize_username_fallback() {
        let mut info = Info::default();
        normalize_config(&mut info);
        // username should be set (either system user or "user")
        assert!(info.username.is_some());
    }

    // ── Plugin origin tests ─────────────────────────────────────────────

    #[test]
    fn test_plugin_specifier_simple() {
        let spec = PluginSpec::Simple("my-plugin".into());
        assert_eq!(plugin_specifier(&spec), "my-plugin");
    }

    #[test]
    fn test_plugin_specifier_with_options() {
        let opts = std::collections::HashMap::new();
        let spec = PluginSpec::WithOptions("my-plugin".into(), opts);
        assert_eq!(plugin_specifier(&spec), "my-plugin");
    }

    #[test]
    fn test_deduplicate_plugin_origins() {
        let origins = vec![
            PluginOrigin {
                spec: PluginSpec::Simple("plugin-a".into()),
                source: "global/config.json".into(),
                scope: PluginScope::Global,
            },
            PluginOrigin {
                spec: PluginSpec::Simple("plugin-b".into()),
                source: "local/.blazecode/config.json".into(),
                scope: PluginScope::Local,
            },
            PluginOrigin {
                spec: PluginSpec::Simple("plugin-a".into()),
                source: "other/config.json".into(),
                scope: PluginScope::Global,
            },
        ];
        let deduped = deduplicate_plugin_origins(origins);
        // Should be 2 entries (plugin-a deduplicated, later entry wins)
        assert_eq!(deduped.len(), 2);
        // plugin-a should be the second source (later wins)
        let plugin_a = deduped.iter().find(|o| plugin_specifier(&o.spec) == "plugin-a").unwrap();
        assert_eq!(plugin_a.source, "other/config.json");
    }

    // ── Config paths tests ──────────────────────────────────────────────

    #[test]
    fn test_config_file_in_directory() {
        let dir = std::path::Path::new("/tmp/test");
        let files = config_file_in_directory(dir, "blazecode");
        assert_eq!(files.len(), 2);
        assert!(files[0].ends_with("blazecode.json"));
        assert!(files[1].ends_with("blazecode.jsonc"));
    }

    // ── Managed config tests ────────────────────────────────────────────

    #[test]
    fn test_parse_managed_plist_strips_meta() {
        let plist = r#"{"PayloadDisplayName": "Test", "shell": "/bin/bash", "PayloadUUID": "abc123"}"#;
        let result = parse_managed_plist(plist).unwrap();
        let v: serde_json::Value = serde_json::from_str(&result).unwrap();
        let obj = v.as_object().unwrap();
        assert!(!obj.contains_key("PayloadDisplayName"));
        assert!(!obj.contains_key("PayloadUUID"));
        assert_eq!(obj["shell"], "/bin/bash");
    }

    // ── Global config file tests ───────────────────────────────────────

    #[test]
    fn test_global_config_file_returns_first() {
        // This is hard to test without filesystem side effects, but at least
        // verify the function compiles and runs without panicking
        let _ = global_config_dir();
    }

    #[test]
    fn test_load_from_env_not_set() {
        // Should return Ok(None) when env var is not set
        let result = Config::load_from_env();
        assert!(result.is_ok());
        assert!(result.unwrap().is_none());
    }

    // ── Writable helpers tests ─────────────────────────────────────────

    #[test]
    fn test_writable_strips_plugin_origins() {
        let info = Info {
            schema: Some("test".into()),
            plugin_origins: vec![PluginOrigin {
                spec: PluginSpec::Simple("p".into()),
                source: "src".into(),
                scope: PluginScope::Global,
            }],
            ..Default::default()
        };
        let w = writable(&info);
        assert!(w.plugin_origins.is_empty());
        assert_eq!(w.schema.as_deref(), Some("test"));
    }

    #[test]
    fn test_writable_global_clears_empty_shell() {
        let info = Info {
            shell: Some("".into()),
            ..Default::default()
        };
        let w = writable_global(&info);
        assert!(w.shell.is_none());
    }

    #[test]
    fn test_writable_global_keeps_nonempty_shell() {
        let info = Info {
            shell: Some("/bin/zsh".into()),
            ..Default::default()
        };
        let w = writable_global(&info);
        assert_eq!(w.shell.as_deref(), Some("/bin/zsh"));
    }

    // ── JSONC patching tests ──────────────────────────────────────────

    #[test]
    fn test_patch_jsonc_adds_key() {
        let existing = r#"{
            // existing config
            "model": "anthropic/claude"
        }"#;
        let patch = serde_json::json!({"shell": "/bin/bash"});
        let result = patch_jsonc(existing, &patch);
        let parsed: serde_json::Value = serde_json::from_str(&result).unwrap();
        assert_eq!(parsed["model"], "anthropic/claude");
        assert_eq!(parsed["shell"], "/bin/bash");
    }

    #[test]
    fn test_patch_jsonc_deep_merge() {
        let existing = r#"{"server": {"port": 3000}}"#;
        let patch = serde_json::json!({"server": {"hostname": "localhost"}});
        let result = patch_jsonc(existing, &patch);
        let parsed: serde_json::Value = serde_json::from_str(&result).unwrap();
        assert_eq!(parsed["server"]["port"], 3000);
        assert_eq!(parsed["server"]["hostname"], "localhost");
    }

    #[test]
    fn test_deep_merge_json_objects() {
        let base = serde_json::json!({"a": 1, "b": {"c": 2}});
        let patch = serde_json::json!({"b": {"d": 3}, "e": 4});
        let merged = deep_merge_json(base, &patch);
        assert_eq!(merged["a"], 1);
        assert_eq!(merged["b"]["c"], 2);
        assert_eq!(merged["b"]["d"], 3);
        assert_eq!(merged["e"], 4);
    }

    // ── merge_writable tests ──────────────────────────────────────────

    #[test]
    fn test_merge_writable_strips_and_merges() {
        let source = Info {
            model: Some("old/model".into()),
            plugin_origins: vec![PluginOrigin {
                spec: PluginSpec::Simple("p".into()),
                source: "s".into(),
                scope: PluginScope::Global,
            }],
            ..Default::default()
        };
        let patch = Info {
            model: Some("new/model".into()),
            ..Default::default()
        };
        let result = merge_writable(&source, &patch);
        assert_eq!(result.model.as_deref(), Some("new/model"));
        assert!(result.plugin_origins.is_empty());
    }

    // ── config_entry_name_from_path tests ────────────────────────────

    #[test]
    fn test_entry_name_from_path_simple() {
        let name = config_entry_name_from_path("agent.md", &[]);
        assert_eq!(name, "agent");
    }

    #[test]
    fn test_entry_name_from_path_with_prefix() {
        let name = config_entry_name_from_path(".blazecode/agent/build.md", &[".blazecode/agent/"]);
        assert_eq!(name, "build");
    }

    #[test]
    fn test_entry_name_from_path_with_dir_prefix() {
        let name = config_entry_name_from_path("agents/deploy.md", &["agents/"]);
        assert_eq!(name, "deploy");
    }

    #[test]
    fn test_entry_name_from_path_no_extension() {
        let name = config_entry_name_from_path("myfile", &[]);
        assert_eq!(name, "myfile");
    }

    #[test]
    fn test_entry_name_from_path_windows_sep() {
        let name = config_entry_name_from_path(".blazecode\\agent\\test.md", &[".blazecode/agent/"]);
        assert_eq!(name, "test");
    }

    #[test]
    fn test_entry_name_from_path_prefix_not_matched() {
        let name = config_entry_name_from_path("some/deep/path/entry.ts", &["other/"]);
        assert_eq!(name, "entry");
    }

    // ── TuiConfigInfo round-trip ──────────────────────────────────────

    #[test]
    fn test_tui_config_info_round_trip() {
        let info = TuiConfigInfo {
            theme: Some("dark".into()),
            font_size: Some(14.0),
            ..Default::default()
        };
        let json = serde_json::to_string(&info).unwrap();
        let parsed: TuiConfigInfo = serde_json::from_str(&json).unwrap();
        assert_eq!(parsed.theme.unwrap(), "dark");
        assert_eq!(parsed.font_size.unwrap(), 14.0);
        assert!(parsed.keybinds.is_empty());
    }

    #[test]
    fn test_tui_config_info_with_keybinds() {
        let mut info = TuiConfigInfo::default();
        info.keybinds.insert("ctrl+q".into(), "quit".into());
        info.keybinds.insert("ctrl+s".into(), "save".into());
        let json = serde_json::to_string(&info).unwrap();
        let parsed: TuiConfigInfo = serde_json::from_str(&json).unwrap();
        assert_eq!(parsed.keybinds.get("ctrl+q").unwrap(), "quit");
        assert_eq!(parsed.keybinds.len(), 2);
    }

    // ── V2ConfigInfo migration tests ──────────────────────────────────

    #[test]
    fn test_v2_config_info_default() {
        let v2 = V2ConfigInfo::default();
        assert!(v2.shell.is_none());
        assert!(v2.permissions.is_empty());
    }

    #[test]
    fn test_v2_serialization_round_trip() {
        let v2 = V2ConfigInfo {
            schema: Some("https://blazecode.ai/config-v2.json".into()),
            shell: Some("/bin/bash".into()),
            model: Some("anthropic/claude".into()),
            ..Default::default()
        };
        let json = serde_json::to_string(&v2).unwrap();
        let parsed: V2ConfigInfo = serde_json::from_str(&json).unwrap();
        assert_eq!(parsed.schema.unwrap(), "https://blazecode.ai/config-v2.json");
        assert_eq!(parsed.shell.unwrap(), "/bin/bash");
    }

    #[test]
    fn test_v2_permission_rule_serialize() {
        let rule = V2PermissionRule {
            action: "read".into(),
            resource: "*.env".into(),
            effect: "deny".into(),
        };
        let json = serde_json::to_value(&rule).unwrap();
        assert_eq!(json["action"], "read");
        assert_eq!(json["resource"], "*.env");
        assert_eq!(json["effect"], "deny");
    }

    #[test]
    fn test_migrate_v1_to_v2_basic() {
        let mut info = Info::default();
        info.model = Some("anthropic/claude-sonnet-4-6".into());
        info.shell = Some("/bin/zsh".into());
        info.snapshot = Some(true);

        let v2 = Config::migrate_v1_to_v2(&info);
        assert_eq!(v2.model.unwrap(), "anthropic/claude-sonnet-4-6");
        assert_eq!(v2.shell.unwrap(), "/bin/zsh");
        assert_eq!(v2.snapshots, Some(true));
    }

    #[test]
    fn test_migrate_v1_to_v2_autoshare_conversion() {
        let mut info = Info::default();
        info.autoshare = Some(true);

        let v2 = Config::migrate_v1_to_v2(&info);
        assert_eq!(v2.share.unwrap(), "auto");
    }

    #[test]
    fn test_migrate_v1_to_v2_tools_to_permissions() {
        let mut info = Info::default();
        info.tools.insert("bash".into(), true);
        info.tools.insert("write".into(), false);

        let v2 = Config::migrate_v1_to_v2(&info);
        assert_eq!(v2.permissions.len(), 2);
        let bash_perm = v2.permissions.iter().find(|p| p.action == "bash").unwrap();
        assert_eq!(bash_perm.effect, "allow");
        let edit_perm = v2.permissions.iter().find(|p| p.action == "edit").unwrap();
        assert_eq!(edit_perm.effect, "deny");
    }

    #[test]
    fn test_migrate_v1_to_v2_agents() {
        let mut info = Info::default();
        info.agent.insert("build".into(), AgentConfig {
            model: Some("claude-sonnet".into()),
            description: Some("Build agent".into()),
            mode: Some(AgentMode::Primary),
            steps: Some(25),
            ..Default::default()
        });

        let v2 = Config::migrate_v1_to_v2(&info);
        let agents = v2.agents.unwrap();
        assert!(agents.contains_key("build"));
        let build = agents.get("build").unwrap();
        assert_eq!(build.model.as_deref(), Some("claude-sonnet"));
        assert_eq!(build.mode.as_deref(), Some("primary"));
        assert_eq!(build.steps, Some(25));
    }

    #[test]
    fn test_migrate_v1_to_v2_mcp_servers() {
        let mut info = Info::default();
        let mut mcp_map = HashMap::new();
        mcp_map.insert("fs".into(), McpEntry::Full(McpConfig::Local {
            command: vec!["npx".into(), "mcp-server".into()],
            cwd: None,
            environment: HashMap::new(),
            enabled: Some(true),
            timeout: Some(5000),
        }));
        info.mcp = mcp_map;

        let v2 = Config::migrate_v1_to_v2(&info);
        let mcp = v2.mcp.unwrap();
        assert!(mcp.servers.contains_key("fs"));
        let server = mcp.servers.get("fs").unwrap();
        assert_eq!(server.r#type, "local");
        assert_eq!(server.command.as_ref().unwrap(), &["npx", "mcp-server"]);
    }

    // ── load_tui_config edge cases ────────────────────────────────────

    #[test]
    fn test_tui_config_info_empty() {
        let info = TuiConfigInfo::default();
        let json = serde_json::to_string(&info).unwrap();
        assert_eq!(json, "{}");
    }

    #[test]
    fn test_tui_config_info_host_attention() {
        let mut info = TuiConfigInfo::default();
        info.host_attention = Some(HostAttentionConfig {
            sounds: Some(vec!["bell.wav".into()]),
            flash: Some(true),
        });
        let json = serde_json::to_string(&info).unwrap();
        let parsed: TuiConfigInfo = serde_json::from_str(&json).unwrap();
        let ha = parsed.host_attention.unwrap();
        assert_eq!(ha.sounds.unwrap(), vec!["bell.wav"]);
        assert_eq!(ha.flash, Some(true));
    }

    #[test]
    fn test_v2_compaction_config() {
        let comp = V2CompactionConfig {
            auto: Some(true),
            prune: Some(false),
            keep_tokens: Some(32000),
            buffer: Some(4000),
        };
        let json = serde_json::to_value(&comp).unwrap();
        assert_eq!(json["auto"], true);
        assert_eq!(json["prune"], false);
        assert_eq!(json["keep_tokens"], 32000);
        assert_eq!(json["buffer"], 4000);
    }

    // ── ensure_npm_deps test ──────────────────────────────────────────

    #[test]
    fn test_ensure_npm_deps_empty_dirs() {
        let children = Config::ensure_npm_deps(&[]);
        assert!(children.is_empty());
    }

    // ── deep_merge_json_map test ──────────────────────────────────────

    #[test]
    fn test_deep_merge_json_map_objects() {
        let mut base = serde_json::Map::new();
        base.insert("a".into(), serde_json::json!(1));
        let mut patch = serde_json::Map::new();
        patch.insert("b".into(), serde_json::json!(2));
        let result = deep_merge_json_map(base, &patch);
        assert_eq!(result["a"], 1);
        assert_eq!(result["b"], 2);
    }

    #[test]
    fn test_deep_merge_json_map_nested() {
        let mut base = serde_json::Map::new();
        let mut inner = serde_json::Map::new();
        inner.insert("x".into(), serde_json::json!(10));
        base.insert("nested".into(), serde_json::Value::Object(inner));

        let mut patch = serde_json::Map::new();
        let mut patch_inner = serde_json::Map::new();
        patch_inner.insert("y".into(), serde_json::json!(20));
        patch.insert("nested".into(), serde_json::Value::Object(patch_inner));

        let result = deep_merge_json_map(base, &patch);
        assert_eq!(result["nested"]["x"], 10);
        assert_eq!(result["nested"]["y"], 20);
    }
}
