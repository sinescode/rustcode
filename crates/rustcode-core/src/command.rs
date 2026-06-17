//! Command types — command definitions, configuration, and state management.
//!
//! Ported from:
//! - `packages/core/src/command.ts` — CommandV2 namespace, Info, Data, Editor, Interface
//!
//! OpenCode commit: 5d0f86606ac30690f79f0a6a9f41a1f49fe95d0b

use serde::{Deserialize, Serialize};

// ══════════════════════════════════════════════════════════════════════════════
// Command Info
// ══════════════════════════════════════════════════════════════════════════════

/// Definition of a named command that the agent can execute.
///
/// # Source
/// `packages/core/src/command.ts` lines 8–16.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CommandInfo {
    /// Unique command name (used as the key in the command map).
    pub name: String,

    /// Template string — the prompt or instruction executed when this command runs.
    pub template: String,

    /// Optional human-readable description shown in autocomplete.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,

    /// Optional agent override — which agent to use when executing this command.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub agent: Option<String>,

    /// Optional model override reference (provider ID + model ID).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub model: Option<CommandModelRef>,

    /// Whether this command should be executed as a subtask.
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub subtask: bool,
}

impl CommandInfo {
    /// Create a new command with required fields.
    #[must_use]
    pub fn new(name: impl Into<String>, template: impl Into<String>) -> Self {
        Self {
            name: name.into(),
            template: template.into(),
            description: None,
            agent: None,
            model: None,
            subtask: false,
        }
    }
}

/// A reference to a model — used in command and other contexts.
///
/// # Source
/// `packages/core/src/command.ts` line 13 — `ModelV2.Ref`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CommandModelRef {
    /// The provider identifier (e.g., `"anthropic"`, `"openai"`).
    pub provider_id: String,

    /// The model identifier within the provider (e.g., `"claude-sonnet-4-20250514"`).
    pub model_id: String,
}

impl CommandModelRef {
    /// Create a new model reference.
    #[must_use]
    pub fn new(provider_id: impl Into<String>, model_id: impl Into<String>) -> Self {
        Self {
            provider_id: provider_id.into(),
            model_id: model_id.into(),
        }
    }

    /// Parse a colon-separated string like `"anthropic:claude-sonnet-4"`.
    #[must_use]
    pub fn parse(input: &str) -> Option<Self> {
        let (provider, model) = input.split_once(':')?;
        Some(Self::new(provider, model))
    }
}

impl std::fmt::Display for CommandModelRef {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}:{}", self.provider_id, self.model_id)
    }
}

// ══════════════════════════════════════════════════════════════════════════════
// Command state types
// ══════════════════════════════════════════════════════════════════════════════

/// The mutable command state — a map of command names to their definitions.
///
/// # Source
/// `packages/core/src/command.ts` lines 17–19.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct CommandData {
    pub commands: std::collections::HashMap<String, CommandInfo>,
}

/// Input for creating or updating a command.
///
/// # Source
/// Derived from the `Editor.update` closure in `packages/core/src/command.ts`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CommandUpdateInput {
    /// The command name (key).
    pub name: String,

    /// The template/prompt to execute.
    pub template: String,

    /// Optional description.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,

    /// Optional agent override.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub agent: Option<String>,

    /// Optional model override.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub model: Option<CommandModelRef>,

    /// Whether to run as a subtask.
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub subtask: bool,
}

impl CommandData {
    /// List all commands as a sorted vector.
    #[must_use]
    pub fn list(&self) -> Vec<&CommandInfo> {
        let mut cmds: Vec<&CommandInfo> = self.commands.values().collect();
        cmds.sort_by(|a, b| a.name.cmp(&b.name));
        cmds
    }

    /// Get a command by name.
    #[must_use]
    pub fn get(&self, name: &str) -> Option<&CommandInfo> {
        self.commands.get(name)
    }

    /// Upsert a command (create or update).
    pub fn upsert(&mut self, input: CommandUpdateInput) {
        let entry = self.commands.entry(input.name.clone()).or_insert_with(|| {
            CommandInfo::new(&input.name, &input.template)
        });
        entry.template = input.template;
        if input.description.is_some() {
            entry.description = input.description;
        }
        if input.agent.is_some() {
            entry.agent = input.agent;
        }
        if input.model.is_some() {
            entry.model = input.model;
        }
        entry.subtask = input.subtask;
        entry.name = input.name;
    }

    /// Remove a command by name.
    pub fn remove(&mut self, name: &str) -> bool {
        self.commands.remove(name).is_some()
    }
}

// ══════════════════════════════════════════════════════════════════════════════
// Command execution types
// ══════════════════════════════════════════════════════════════════════════════

/// Input for executing a named command.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CommandExecuteInput {
    /// The command name to execute.
    pub name: String,

    /// Optional arguments interpolated into the template.
    #[serde(default, skip_serializing_if = "std::collections::HashMap::is_empty")]
    pub args: std::collections::HashMap<String, String>,
}

/// Result of a command execution — the resolved prompt with applied args.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CommandExecuteResult {
    /// The command name that was executed.
    pub name: String,

    /// The final prompt after template interpolation.
    pub prompt: String,

    /// The agent that will handle execution.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub agent: Option<String>,

    /// The model to use for execution.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub model: Option<CommandModelRef>,

    /// Whether to run as a subtask.
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub subtask: bool,
}

// ══════════════════════════════════════════════════════════════════════════════
// Tests
// ══════════════════════════════════════════════════════════════════════════════

#[cfg(test)]
mod tests {
    use super::*;

    // ── CommandInfo ────────────────────────────────────────────────

    #[test]
    fn test_command_info_new() {
        let cmd = CommandInfo::new("greet", "Hello, {{name}}!");
        assert_eq!(cmd.name, "greet");
        assert_eq!(cmd.template, "Hello, {{name}}!");
        assert!(!cmd.subtask);
    }

    #[test]
    fn test_command_info_full() {
        let cmd = CommandInfo {
            name: "deploy".into(),
            template: "Deploy to {{env}}".into(),
            description: Some("Deploy the application".into()),
            agent: Some("build".into()),
            model: Some(CommandModelRef::new("anthropic", "claude-sonnet-4")),
            subtask: true,
        };
        let json = serde_json::to_string(&cmd).expect("serialize");
        assert!(json.contains("deploy"));
        assert!(json.contains("claude-sonnet-4"));
        assert!(json.contains("subtask"));
    }

    #[test]
    fn test_command_info_serde_minimal() {
        let cmd = CommandInfo::new("test", "echo hi");
        let json = serde_json::to_string(&cmd).expect("serialize");
        let parsed: CommandInfo = serde_json::from_str(&json).expect("deserialize");
        assert_eq!(parsed.name, "test");
        assert_eq!(parsed.template, "echo hi");
    }

    // ── CommandModelRef ────────────────────────────────────────────

    #[test]
    fn test_model_ref_new() {
        let m = CommandModelRef::new("openai", "gpt-5");
        assert_eq!(m.provider_id, "openai");
        assert_eq!(m.model_id, "gpt-5");
    }

    #[test]
    fn test_model_ref_parse() {
        let m = CommandModelRef::parse("anthropic:claude-opus-4").expect("parse");
        assert_eq!(m.provider_id, "anthropic");
        assert_eq!(m.model_id, "claude-opus-4");
    }

    #[test]
    fn test_model_ref_parse_no_colon() {
        assert!(CommandModelRef::parse("just-a-model").is_none());
    }

    #[test]
    fn test_model_ref_display() {
        let m = CommandModelRef::new("prov", "mod");
        assert_eq!(m.to_string(), "prov:mod");
    }

    #[test]
    fn test_model_ref_serde() {
        let m = CommandModelRef::new("anthropic", "claude");
        let json = serde_json::to_string(&m).expect("serialize");
        let parsed: CommandModelRef = serde_json::from_str(&json).expect("deserialize");
        assert_eq!(parsed.provider_id, "anthropic");
        assert_eq!(parsed.model_id, "claude");
    }

    // ── CommandData ────────────────────────────────────────────────

    #[test]
    fn test_command_data_empty() {
        let data = CommandData::default();
        assert!(data.commands.is_empty());
        assert!(data.list().is_empty());
    }

    #[test]
    fn test_command_data_upsert_new() {
        let mut data = CommandData::default();
        data.upsert(CommandUpdateInput {
            name: "build".into(),
            template: "cargo build".into(),
            description: Some("Build the project".into()),
            agent: None,
            model: None,
            subtask: false,
        });
        assert_eq!(data.commands.len(), 1);
        assert!(data.get("build").is_some());
    }

    #[test]
    fn test_command_data_upsert_update() {
        let mut data = CommandData::default();
        data.upsert(CommandUpdateInput {
            name: "test".into(),
            template: "old".into(),
            description: None,
            agent: None,
            model: None,
            subtask: false,
        });
        data.upsert(CommandUpdateInput {
            name: "test".into(),
            template: "new template".into(),
            description: Some("updated".into()),
            agent: None,
            model: None,
            subtask: true,
        });
        let cmd = data.get("test").expect("exists");
        assert_eq!(cmd.template, "new template");
        assert!(cmd.subtask);
    }

    #[test]
    fn test_command_data_remove() {
        let mut data = CommandData::default();
        data.upsert(CommandUpdateInput {
            name: "temp".into(),
            template: "x".into(),
            description: None,
            agent: None,
            model: None,
            subtask: false,
        });
        assert!(data.remove("temp"));
        assert!(!data.remove("temp"));
        assert!(data.get("temp").is_none());
    }

    #[test]
    fn test_command_data_list_sorted() {
        let mut data = CommandData::default();
        data.upsert(CommandUpdateInput {
            name: "zebra".into(),
            template: "z".into(),
            description: None,
            agent: None,
            model: None,
            subtask: false,
        });
        data.upsert(CommandUpdateInput {
            name: "alpha".into(),
            template: "a".into(),
            description: None,
            agent: None,
            model: None,
            subtask: false,
        });
        let list = data.list();
        assert_eq!(list[0].name, "alpha");
        assert_eq!(list[1].name, "zebra");
    }

    // ── CommandExecuteInput / CommandExecuteResult ─────────────────

    #[test]
    fn test_execute_input_serde() {
        let input = CommandExecuteInput {
            name: "deploy".into(),
            args: {
                let mut m = std::collections::HashMap::new();
                m.insert("env".into(), "production".into());
                m
            },
        };
        let json = serde_json::to_string(&input).expect("serialize");
        assert!(json.contains("deploy"));
        assert!(json.contains("production"));
    }

    #[test]
    fn test_execute_result_serde() {
        let result = CommandExecuteResult {
            name: "greet".into(),
            prompt: "Hello, World!".into(),
            agent: None,
            model: None,
            subtask: false,
        };
        let json = serde_json::to_string(&result).expect("serialize");
        let parsed: CommandExecuteResult = serde_json::from_str(&json).expect("deserialize");
        assert_eq!(parsed.name, "greet");
        assert_eq!(parsed.prompt, "Hello, World!");
    }

    #[test]
    fn test_execute_result_with_model() {
        let result = CommandExecuteResult {
            name: "analyze".into(),
            prompt: "Analyze this".into(),
            agent: Some("plan".into()),
            model: Some(CommandModelRef::new("anthropic", "claude-opus")),
            subtask: true,
        };
        let json = serde_json::to_string(&result).expect("serialize");
        let parsed: CommandExecuteResult = serde_json::from_str(&json).expect("deserialize");
        assert_eq!(parsed.agent.as_deref(), Some("plan"));
        assert!(parsed.subtask);
    }

    // ── CommandUpdateInput ─────────────────────────────────────────

    #[test]
    fn test_update_input_serde() {
        let input = CommandUpdateInput {
            name: "cmd".into(),
            template: "tpl".into(),
            description: Some("desc".into()),
            agent: Some("build".into()),
            model: Some(CommandModelRef::new("p", "m")),
            subtask: false,
        };
        let json = serde_json::to_string(&input).expect("serialize");
        let parsed: CommandUpdateInput = serde_json::from_str(&json).expect("deserialize");
        assert_eq!(parsed.name, "cmd");
        assert_eq!(parsed.agent.as_deref(), Some("build"));
    }
}
