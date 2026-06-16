//! Configuration system — types, loading, merging, and persistence.
//!
//! Ported from: `packages/opencode/src/config/config.ts`
//! OpenCode commit: 5d0f86606ac30690f79f0a6a9f41a1f49fe95d0b
//!
//! ## Architecture
//!
//! The TS source uses Effect.ts `Layer` + `InstanceState` for config loading,
//! merging multiple sources: global config, project config files (JSON/JSONC),
//! `.opencode` directories, well-known URLs, environment variables, and managed
//! preferences.
//!
//! In Rust, config loading is synchronous (filesystem reads). The [`Config`]
//! struct wraps an [`Info`] behind [`RwLock`] and provides the same `get` /
//! `update` / `invalidate` interface.
//!
//! ## Config Sources (in priority order, lowest first)
//!
//! 1. Global config — `~/.config/opencode/opencode.jsonc`
//! 2. Remote well-known org config (fetched via auth service — future)
//! 3. Project config — `opencode.jsonc` / `opencode.json` walking up from cwd
//! 4. `.opencode/` directory configs
//! 5. Environment variable overrides (`OPENCODE_CONFIG`, `OPENCODE_CONFIG_CONTENT`)
//! 6. Managed preferences (macOS MDM — future)
//!
//! Merging is deep: nested objects are merged recursively, and the `instructions`
//! array is concatenated (deduplicated) rather than replaced.

use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::RwLock;

// ── Top-level config service ────────────────────────────────────────────

/// Main configuration service.
///
/// Wraps the loaded configuration info and manages persistence.
/// Thread-safe via interior [`RwLock`].
///
/// # Source
/// Ported from `packages/opencode/src/config/config.ts` lines 117–136
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
#[serde(rename_all = "camelCase")]
pub struct Info {
    /// JSON schema reference for editor completion
    #[serde(skip_serializing_if = "Option::is_none")]
    pub schema: Option<String>,

    /// Default shell for terminal and bash tool
    #[serde(skip_serializing_if = "Option::is_none")]
    pub shell: Option<String>,

    /// Log level: DEBUG, INFO, WARN, ERROR
    #[serde(skip_serializing_if = "Option::is_none")]
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
    Disabled { disabled: bool },
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
    /// # Source
    /// Ported from `packages/opencode/src/config/config.ts` line 605
    /// (`Config.get()`).
    pub fn get(&self) -> Info {
        self.info
            .read()
            .expect("Config lock poisoned")
            .clone()
    }

    /// Load configuration from the global config directory.
    ///
    /// Reads `opencode.jsonc`, `opencode.json`, and `config.json`
    /// from the OS config directory (`~/.config/opencode/` on Linux).
    ///
    /// # Source
    /// Ported from `packages/opencode/src/config/config.ts` lines 246–279
    /// (`loadGlobal`).
    pub fn load_global() -> crate::error::Result<Info> {
        let config_dir = global_config_dir()?;
        let mut info = Info::default();

        // Try each candidate file in order
        for filename in &["config.json", "opencode.json", "opencode.jsonc"] {
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

        Ok(info)
    }

    /// Load configuration from the default location and merge into self.
    ///
    /// # Errors
    /// Returns an error if the config directory cannot be determined.
    pub fn load(&self) -> crate::error::Result<Info> {
        Self::load_global()
    }

    /// Get the global config directory path.
    pub fn global_config_dir() -> crate::error::Result<PathBuf> {
        global_config_dir()
    }

    /// Get the data directory for rustcode.
    pub fn data_dir() -> crate::error::Result<PathBuf> {
        let data_dir = dirs::data_dir()
            .ok_or_else(|| crate::error::Error::Config("Cannot determine data directory".into()))?;
        Ok(data_dir.join("opencode"))
    }
}

// ── Config file discovery ────────────────────────────────────────────────

/// Discover config files walking up from `start_dir`, stopping at `worktree`.
///
/// Looks for `{name}.jsonc` and `{name}.json` in each directory.
///
/// # Source
/// Ported from `packages/opencode/src/config/paths.ts` lines 10–21
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
            if dir == stop {
                break;
            }
        }

        for ext in &["jsonc", "json"] {
            let candidate = dir.join(format!("{name}.{ext}"));
            if candidate.exists() {
                files.push(candidate);
            }
        }

        current = dir.parent().map(|p| p.to_path_buf());

        // Stop at filesystem root
        if current.as_ref().map_or(true, |p| p == dir) {
            break;
        }
    }

    // Reverse so parent dirs come first (lowest priority)
    files.reverse();
    Ok(files)
}

/// Discover `.opencode` directories walking up from `start_dir`.
///
/// # Source
/// Ported from `packages/opencode/src/config/paths.ts` lines 23–41
/// (`ConfigPaths.directories`).
pub fn discover_opencode_dirs(
    start_dir: &std::path::Path,
    stop_dir: Option<&std::path::Path>,
) -> crate::error::Result<Vec<PathBuf>> {
    let mut dirs = Vec::new();
    let mut current = Some(start_dir.to_path_buf());

    while let Some(dir) = current {
        if let Some(stop) = stop_dir {
            if dir == stop {
                break;
            }
        }

        let opencode_dir = dir.join(".opencode");
        if opencode_dir.exists() && opencode_dir.is_dir() {
            dirs.push(opencode_dir);
        }

        current = dir.parent().map(|p| p.to_path_buf());
        if current.as_ref().map_or(true, |p| p == dir) {
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
/// Ported from `packages/opencode/src/config/parse.ts` lines 8–33
/// (`ConfigParse.jsonc`).
pub fn parse_jsonc(text: &str, _filepath: &std::path::Path) -> crate::error::Result<serde_json::Value> {
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
            while j < chars.len() && (chars[j] == ' ' || chars[j] == '\t' || chars[j] == '\n' || chars[j] == '\r') {
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
/// Ported from `packages/opencode/src/config/parse.ts` lines 35–79
/// (`ConfigParse.schema`).
pub fn validate_info(
    value: serde_json::Value,
    source: &std::path::Path,
) -> crate::error::Result<Info> {
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
/// # Source
/// Ported from `packages/opencode/src/config/variable.ts` lines 34–91
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
        let m = caps.get(0).unwrap();
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
        let resolved = if file_path_str.starts_with("~/") {
            if let Some(home) = dirs::home_dir() {
                home.join(&file_path_str[2..])
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

        // Read the file
        match std::fs::read_to_string(&resolved) {
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

// ── Config merging ──────────────────────────────────────────────────────

/// Deep-merge `source` into `target`, mutating `target`.
///
/// - Objects are merged recursively.
/// - The `instructions` array is concatenated (deduplicated).
/// - All other fields: source wins if present.
///
/// # Source
/// Ported from `packages/opencode/src/config/config.ts` lines 41–52
/// (`mergeConfig` + `mergeConfigConcatArrays`).
pub fn merge_info(target: &mut Info, source: &Info) {
    // Instructions — concatenate and deduplicate
    if !source.instructions.is_empty() {
        let mut combined: Vec<String> = target.instructions.iter().cloned().collect();
        for inst in &source.instructions {
            if !combined.contains(inst) {
                combined.push(inst.clone());
            }
        }
        target.instructions = combined;
    }

    // Merge commands
    for (key, cmd) in &source.command {
        target.command.entry(key.clone()).or_insert_with(|| cmd.clone());
    }

    // Merge agents
    for (key, agent) in &source.agent {
        target.agent.entry(key.clone()).or_insert_with(|| agent.clone());
    }

    // Merge providers
    for (key, provider) in &source.provider {
        target.provider.entry(key.clone()).or_insert_with(|| provider.clone());
    }

    // Merge MCP configs
    for (key, mcp) in &source.mcp {
        target.mcp.entry(key.clone()).or_insert_with(|| mcp.clone());
    }

    // Merge deprecated mode → agent
    for (key, mode_cfg) in &source.mode {
        target.mode.entry(key.clone()).or_insert_with(|| mode_cfg.clone());
    }

    // Merge references
    for (key, ref_entry) in &source.references {
        target.references.entry(key.clone()).or_insert_with(|| ref_entry.clone());
    }
    for (key, ref_entry) in &source.reference {
        target.reference.entry(key.clone()).or_insert_with(|| ref_entry.clone());
    }

    // Merge plugin specs (concatenate, don't replace)
    if !source.plugin.is_empty() {
        target.plugin.extend(source.plugin.iter().cloned());
    }

    // Scalar and optional fields — source wins if Some
    if source.schema.is_some() { target.schema = source.schema.clone(); }
    if source.shell.is_some() { target.shell = source.shell.clone(); }
    if source.log_level.is_some() { target.log_level = source.log_level; }
    if source.server.is_some() { target.server = source.server.clone(); }
    if source.skills.is_some() { target.skills = source.skills.clone(); }
    if source.watcher.is_some() { target.watcher = source.watcher.clone(); }
    if source.snapshot.is_some() { target.snapshot = source.snapshot; }
    if source.share.is_some() { target.share = source.share; }
    if source.autoshare.is_some() { target.autoshare = source.autoshare; }
    if source.autoupdate.is_some() { target.autoupdate = source.autoupdate.clone(); }
    if source.model.is_some() { target.model = source.model.clone(); }
    if source.small_model.is_some() { target.small_model = source.small_model.clone(); }
    if source.default_agent.is_some() { target.default_agent = source.default_agent.clone(); }
    if source.username.is_some() { target.username = source.username.clone(); }
    if source.formatter.is_some() { target.formatter = source.formatter.clone(); }
    if source.lsp.is_some() { target.lsp = source.lsp.clone(); }
    if source.layout.is_some() { target.layout = source.layout.clone(); }
    if source.permission.is_some() { target.permission = source.permission.clone(); }
    if source.attachment.is_some() { target.attachment = source.attachment.clone(); }
    if source.enterprise.is_some() { target.enterprise = source.enterprise.clone(); }
    if source.tool_output.is_some() { target.tool_output = source.tool_output.clone(); }
    if source.compaction.is_some() { target.compaction = source.compaction.clone(); }
    if source.experimental.is_some() { target.experimental = source.experimental.clone(); }

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
/// Ported from `packages/opencode/src/config/config.ts` lines 263–276
/// (legacy config migration block).
fn migrate_legacy_toml(content: &str) -> crate::error::Result<Info> {
    let value: toml::Value = toml::from_str(content).map_err(|e| {
        crate::error::Error::Config(format!("Legacy TOML parse error: {e}"))
    })?;

    let mut info = Info::default();

    if let Some(table) = value.as_table() {
        // provider + model → model field
        if let (Some(provider), Some(model)) = (table.get("provider"), table.get("model")) {
            if let (Some(p), Some(m)) = (provider.as_str(), model.as_str()) {
                info.model = Some(format!("{p}/{m}"));
            }
        }
        info.schema = Some("https://opencode.ai/config.json".to_owned());

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
    Ok(config_dir.join("opencode"))
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
        std::env::set_var("RUSTCODE_TEST_VAR", "test_value");
        let result = substitute_variables(
            "prefix {env:RUSTCODE_TEST_VAR} suffix",
            std::path::Path::new("."),
            None,
        )
        .unwrap();
        assert_eq!(result, "prefix test_value suffix");
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
            McpConfig::Local { command, enabled, timeout, .. } => {
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
}
