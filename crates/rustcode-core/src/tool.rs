//! Tool system — defines, registers, and executes tools.
//!
//! Ported from: `packages/opencode/src/tool/tool.ts` and `registry.ts`

use async_trait::async_trait;
use serde::{Deserialize, Serialize};

/// Tool execution context passed to every tool.
#[derive(Debug)]
pub struct ToolContext {
    /// Current session ID
    pub session_id: String,
    /// Current message ID
    pub message_id: String,
    /// Agent name
    pub agent: String,
    /// Abort signal for cancellation
    pub abort: tokio_util::sync::CancellationToken,
    /// Current message history
    pub messages: Vec<crate::provider::ChatMessage>,
}

/// Result of a tool execution.
#[derive(Debug, Clone)]
pub struct ToolResult {
    /// Title for display
    pub title: String,
    /// Output text
    pub output: String,
    /// Metadata
    pub metadata: std::collections::HashMap<String, serde_json::Value>,
}

/// Tool definition trait.
///
/// # Source
/// Ported from `packages/opencode/src/tool/tool.ts`.
#[async_trait]
pub trait Tool: Send + Sync {
    /// Tool identifier.
    fn id(&self) -> &str;

    /// Human-readable description.
    fn description(&self) -> &str;

    /// JSON Schema for the tool's parameters.
    fn parameters_schema(&self) -> serde_json::Value;

    /// Execute the tool with the given arguments.
    ///
    /// # Errors
    /// Returns an error if the tool execution fails.
    async fn execute(
        &self,
        args: serde_json::Value,
        ctx: &ToolContext,
    ) -> crate::error::Result<ToolResult>;
}

/// Registry of available tools.
pub struct ToolRegistry {
    tools: std::collections::HashMap<String, Box<dyn Tool>>,
}

impl ToolRegistry {
    /// Create an empty tool registry.
    pub fn new() -> Self {
        Self {
            tools: std::collections::HashMap::new(),
        }
    }

    /// Register a tool.
    pub fn register(&mut self, tool: Box<dyn Tool>) {
        self.tools.insert(tool.id().to_string(), tool);
    }

    /// Get a tool by ID.
    pub fn get(&self, id: &str) -> Option<&dyn Tool> {
        self.tools.get(id).map(|t| t.as_ref())
    }

    /// List all tool IDs.
    pub fn ids(&self) -> Vec<&str> {
        self.tools.keys().map(|s| s.as_str()).collect()
    }

    /// List all tools.
    pub fn all(&self) -> Vec<&dyn Tool> {
        self.tools.values().map(|t| t.as_ref()).collect()
    }

    /// Get tool definitions for LLM function calling.
    pub fn definitions(&self) -> Vec<crate::provider::ToolDefinition> {
        self.tools
            .values()
            .map(|t| crate::provider::ToolDefinition {
                name: t.id().to_string(),
                description: t.description().to_string(),
                parameters: t.parameters_schema(),
            })
            .collect()
    }
}

impl Default for ToolRegistry {
    fn default() -> Self {
        Self::new()
    }
}
