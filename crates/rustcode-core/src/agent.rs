//! Agent definitions.
//!
//! Ported from: `packages/opencode/src/agent/agent.ts`

use crate::permission::PermissionRuleset;
use serde::{Deserialize, Serialize};

/// Agent information.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Agent {
    /// Agent name (e.g., "coder", "task")
    pub name: String,
    /// Agent description
    #[serde(skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
    /// Agent mode
    #[serde(default)]
    pub mode: AgentMode,
    /// Permission rules
    #[serde(default)]
    pub permission: PermissionRuleset,
    /// System prompt additions
    #[serde(skip_serializing_if = "Option::is_none")]
    pub system_prompt: Option<String>,
}

/// Agent mode.
#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum AgentMode {
    #[default]
    Primary,
    Subagent,
}

/// Default agents.
pub fn default_agents() -> Vec<Agent> {
    vec![
        Agent {
            name: "coder".into(),
            description: Some("Primary coding agent".into()),
            mode: AgentMode::Primary,
            permission: Vec::new(),
            system_prompt: None,
        },
        Agent {
            name: "task".into(),
            description: Some("Subagent for delegated tasks".into()),
            mode: AgentMode::Subagent,
            permission: Vec::new(),
            system_prompt: None,
        },
    ]
}
