//! Environment variable management.
//!
//! Ported from: `packages/opencode/src/env/index.ts`

use std::collections::HashMap;

/// Environment variable manager.
pub struct Env {
    vars: HashMap<String, String>,
}

impl Env {
    /// Create a new environment manager.
    pub fn new() -> Self {
        Self {
            vars: std::env::vars().collect(),
        }
    }

    /// Get an environment variable.
    pub fn get(&self, key: &str) -> Option<&str> {
        self.vars.get(key).map(|s| s.as_str())
    }

    /// Get an environment variable with a default.
    pub fn get_or(&self, key: &str, default: &str) -> &str {
        self.get(key).unwrap_or(default)
    }

    /// Check if an environment variable is set.
    pub fn has(&self, key: &str) -> bool {
        self.vars.contains_key(key)
    }
}

impl Default for Env {
    fn default() -> Self {
        Self::new()
    }
}
