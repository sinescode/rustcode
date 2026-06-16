//! Configuration system.
//!
//! Ported from: `packages/opencode/src/config/config.ts`

use serde::{Deserialize, Serialize};
use std::path::PathBuf;

/// Main configuration for rustcode.
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct Config {
    /// Provider configurations
    #[serde(default)]
    pub provider: std::collections::HashMap<String, ProviderConfig>,
    /// Disabled providers
    #[serde(default)]
    pub disabled_providers: Vec<String>,
    /// Enabled providers (if set, only these are used)
    #[serde(default)]
    pub enabled_providers: Option<Vec<String>>,
    /// Agent configurations
    #[serde(default)]
    pub agent: std::collections::HashMap<String, AgentConfig>,
    /// Compaction settings
    #[serde(default)]
    pub compaction: Option<CompactionConfig>,
    /// Experimental flags
    #[serde(default)]
    pub experimental: Option<ExperimentalConfig>,
    /// MCP server configurations
    #[serde(default)]
    pub mcp: std::collections::HashMap<String, McpConfig>,
}

/// Provider configuration.
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct ProviderConfig {
    /// Provider display name
    #[serde(skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,
    /// Environment variable names for API keys
    #[serde(default)]
    pub env: Vec<String>,
    /// API key override
    #[serde(skip_serializing_if = "Option::is_none")]
    pub api_key: Option<String>,
    /// Base URL override
    #[serde(skip_serializing_if = "Option::is_none")]
    pub base_url: Option<String>,
    /// NPM package for provider SDK
    #[serde(skip_serializing_if = "Option::is_none")]
    pub npm: Option<String>,
    /// Model overrides
    #[serde(default)]
    pub models: std::collections::HashMap<String, ModelConfig>,
    /// Additional options
    #[serde(default)]
    pub options: std::collections::HashMap<String, serde_json::Value>,
}

/// Model configuration override.
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct ModelConfig {
    /// Model ID override
    #[serde(skip_serializing_if = "Option::is_none")]
    pub id: Option<String>,
    /// Provider settings
    #[serde(default)]
    pub provider: std::collections::HashMap<String, serde_json::Value>,
}

/// Agent configuration.
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct AgentConfig {
    /// Agent description
    #[serde(skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
    /// Agent mode (primary/subagent)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub mode: Option<String>,
    /// System prompt additions
    #[serde(skip_serializing_if = "Option::is_none")]
    pub system_prompt: Option<String>,
}

/// Compaction configuration.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CompactionConfig {
    /// Auto-compact on context overflow
    #[serde(default = "default_true")]
    pub auto: bool,
}

fn default_true() -> bool {
    true
}

/// Experimental configuration.
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct ExperimentalConfig {
    /// Continue loop on permission deny
    #[serde(default)]
    pub continue_loop_on_deny: bool,
}

/// MCP server configuration.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct McpConfig {
    /// Command to run
    pub command: String,
    /// Arguments
    #[serde(default)]
    pub args: Vec<String>,
    /// Environment variables
    #[serde(default)]
    pub env: std::collections::HashMap<String, String>,
}

impl Config {
    /// Load configuration from the default location.
    ///
    /// # Errors
    /// Returns an error if the config file cannot be read or parsed.
    pub fn load() -> crate::error::Result<Self> {
        let config_path = Self::config_path()?;
        if config_path.exists() {
            let content = std::fs::read_to_string(&config_path)?;
            let config: Config = toml::from_str(&content)?;
            Ok(config)
        } else {
            Ok(Config::default())
        }
    }

    /// Get the path to the config file.
    fn config_path() -> crate::error::Result<PathBuf> {
        let config_dir = dirs::config_dir()
            .ok_or_else(|| crate::error::Error::Config("Cannot determine config directory".into()))?;
        Ok(config_dir.join("opencode").join("config.toml"))
    }

    /// Get the data directory for rustcode.
    ///
    /// # Errors
    /// Returns an error if the data directory cannot be determined.
    pub fn data_dir() -> crate::error::Result<PathBuf> {
        let data_dir = dirs::data_dir()
            .ok_or_else(|| crate::error::Error::Config("Cannot determine data directory".into()))?;
        Ok(data_dir.join("opencode"))
    }
}
