//! Plugin system.
//!
//! Ported from: `packages/opencode/src/plugin/*.ts`

use serde::{Deserialize, Serialize};

/// Plugin information.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Plugin {
    /// Plugin name
    pub name: String,
    /// Plugin description
    #[serde(skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
    /// Plugin version
    #[serde(skip_serializing_if = "Option::is_none")]
    pub version: Option<String>,
}

/// Plugin manager placeholder.
pub struct PluginManager {
    plugins: Vec<Plugin>,
}

impl PluginManager {
    /// Create a new plugin manager.
    pub fn new() -> Self {
        Self { plugins: Vec::new() }
    }

    /// List all loaded plugins.
    pub fn list(&self) -> &[Plugin] {
        &self.plugins
    }
}

impl Default for PluginManager {
    fn default() -> Self {
        Self::new()
    }
}
