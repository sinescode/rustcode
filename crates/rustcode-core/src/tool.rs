//! Tool system — defines, registers, and executes tools.
//!
//! Ported from:
//! - `packages/opencode/src/tool/tool.ts` (183 lines)
//! - `packages/opencode/src/tool/registry.ts` (441 lines)
//! - `packages/opencode/src/tool/schema.ts` (15 lines)
//! - `packages/opencode/src/tool/truncate.ts`
//! OpenCode commit: 5d0f86606ac30690f79f0a6a9f41a1f49fe95d0b

use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::sync::Arc;
use tokio_util::sync::CancellationToken;

/// Tool execution context passed to every tool.
///
/// # Source
/// Ported from `packages/opencode/src/tool/tool.ts` line 36–46.
#[derive(Debug, Clone)]
pub struct ToolContext {
    /// Current session ID
    pub session_id: String,
    /// Current message ID
    pub message_id: String,
    /// Agent name
    pub agent: String,
    /// Abort signal for cancellation
    pub abort: CancellationToken,
    /// Tool call ID (provider-assigned)
    pub call_id: Option<String>,
    /// Extra context data
    pub extra: HashMap<String, serde_json::Value>,
    /// Current message history
    pub messages: Vec<crate::provider::ChatMessage>,
}

/// Result of a tool execution.
///
/// # Source
/// Ported from `packages/opencode/src/tool/tool.ts` line 48–53.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ExecuteResult {
    /// Title for display
    pub title: String,
    /// Output text
    pub output: String,
    /// Whether the output was truncated
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub truncated: bool,
    /// Path to full output (if truncated)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub output_path: Option<String>,
    /// Attachments (e.g. images)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub attachments: Option<Vec<FileAttachment>>,
    /// Metadata
    #[serde(default)]
    pub metadata: HashMap<String, serde_json::Value>,
}

/// A file attachment returned with a tool result.
///
/// # Source
/// Ported from `packages/core/src/v1/session.ts` `FilePart`.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FileAttachment {
    /// MIME type
    pub mime: String,
    /// Data URL or file URL
    pub url: String,
}

/// Tool definition trait.
///
/// # Source
/// Ported from `packages/opencode/src/tool/tool.ts` line 55–65.
#[async_trait]
pub trait Tool: Send + Sync {
    /// Tool identifier.
    fn id(&self) -> &str;

    /// Human-readable description.
    fn description(&self) -> &str;

    /// Optional JSON Schema for the tool's parameters.
    /// When Some, this is what the LLM sees.
    fn json_schema(&self) -> Option<serde_json::Value> {
        None
    }

    /// Effect-style parameter schema decoder.
    /// Returns a schema that can validate tool arguments.
    fn parameters_schema(&self) -> serde_json::Value;

    /// Execute the tool with the given arguments.
    ///
    /// # Errors
    /// Returns `Error::ToolInvalidArguments` if the arguments don't match the schema.
    /// Returns `Error::Tool` for execution failures.
    async fn execute(
        &self,
        args: serde_json::Value,
        ctx: &ToolContext,
    ) -> crate::error::Result<ExecuteResult>;

    /// Format a validation error into a human-readable message.
    fn format_validation_error(&self, error: &str) -> String {
        format!(
            "The {} tool was called with invalid arguments: {}.\n\
             Please rewrite the input so it satisfies the expected schema.",
            self.id(),
            error
        )
    }
}

/// Concrete tool definition — combines a Tool with metadata.
///
/// This is the struct that gets registered in the ToolRegistry.
///
/// # Source
/// Ported from `packages/opencode/src/tool/tool.ts` `Def` type.
#[derive(Clone)]
pub struct ToolDef {
    /// Tool ID
    pub id: String,
    /// Human-readable description
    pub description: String,
    /// JSON Schema for LLM
    pub json_schema: Option<serde_json::Value>,
    /// The actual tool implementation
    pub tool: Arc<dyn Tool>,
}

impl ToolDef {
    /// Create a new ToolDef from a Tool implementation.
    pub fn new(tool: Arc<dyn Tool>) -> Self {
        let id = tool.id().to_string();
        let description = tool.description().to_string();
        let json_schema = tool.json_schema();
        Self {
            id,
            description,
            json_schema,
            tool,
        }
    }

    /// Get parameter definitions for LLM function calling.
    pub fn to_llm_definition(&self) -> crate::provider::ToolDefinition {
        crate::provider::ToolDefinition {
            name: self.id.clone(),
            description: self.description.clone(),
            parameters: self.tool.parameters_schema(),
        }
    }
}

/// Lightweight tool metadata — holds the ID and an init function.
///
/// Used for deferred tool initialization (e.g. MCP tools loaded at runtime).
///
/// # Source
/// Ported from `packages/opencode/src/tool/tool.ts` `Info` type.
pub struct ToolInfo {
    /// Tool ID
    pub id: String,
    /// Initialization function
    pub init: Arc<dyn Fn() -> Box<dyn Tool> + Send + Sync>,
}

impl ToolInfo {
    /// Create a ToolInfo for a tool that can be instantiated on demand.
    pub fn new(id: impl Into<String>, init: impl Fn() -> Box<dyn Tool> + Send + Sync + 'static) -> Self {
        Self {
            id: id.into(),
            init: Arc::new(init),
        }
    }

    /// Initialize the tool.
    pub fn instantiate(&self) -> Box<dyn Tool> {
        (self.init)()
    }
}

/// A plugin-provided tool (external or MCP).
///
/// Plugin tools don't implement the Tool trait directly; they have
/// execute handlers that receive raw args and return string or structured results.
///
/// # Source
/// Ported from `packages/opencode/src/tool/registry.ts` `ToolDefinition` from plugin.
#[derive(Clone)]
pub struct PluginToolDef {
    /// Tool ID
    pub id: String,
    /// Description
    pub description: String,
    /// JSON Schema for the tool's input arguments
    pub json_schema: serde_json::Value,
    /// Execute function — takes raw args JSON + context, returns output or structured result
    pub execute: Arc<
        dyn Fn(
                serde_json::Value,
                ToolContext,
            )
                -> std::pin::Pin<
                    Box<dyn std::future::Future<Output = crate::error::Result<ExecuteResult>> + Send>,
                > + Send
            + Sync,
    >,
}

impl PluginToolDef {
    /// Create a new PluginToolDef.
    pub fn new<F, Fut>(
        id: impl Into<String>,
        description: impl Into<String>,
        json_schema: serde_json::Value,
        execute_fn: F,
    ) -> Self
    where
        F: Fn(serde_json::Value, ToolContext) -> Fut + Send + Sync + 'static,
        Fut: std::future::Future<Output = crate::error::Result<ExecuteResult>> + Send + 'static,
    {
        Self {
            id: id.into(),
            description: description.into(),
            json_schema,
            execute: Arc::new(move |args, ctx| Box::pin(execute_fn(args, ctx))),
        }
    }
}

// ═══════════════════════════════════════════════════════════════════
// Tool Registry
// ═══════════════════════════════════════════════════════════════════

/// Thread-safe registry of available tools.
///
/// Uses DashMap for concurrent access — tools can be registered
/// at runtime (MCP, plugins).
///
/// # Source
/// Ported from `packages/opencode/src/tool/registry.ts`.
pub struct ToolRegistry {
    tools: dashmap::DashMap<String, ToolDef>,
    plugin_tools: dashmap::DashMap<String, PluginToolDef>,
}

impl ToolRegistry {
    /// Create an empty tool registry.
    pub fn new() -> Self {
        Self {
            tools: dashmap::DashMap::new(),
            plugin_tools: dashmap::DashMap::new(),
        }
    }

    /// Register a built-in tool.
    pub fn register(&self, tool: Arc<dyn Tool>) {
        let def = ToolDef::new(tool);
        self.tools.insert(def.id.clone(), def);
    }

    /// Register a tool directly as a ToolDef.
    pub fn register_def(&self, def: ToolDef) {
        self.tools.insert(def.id.clone(), def);
    }

    /// Register a plugin/external tool.
    pub fn register_plugin(&self, def: PluginToolDef) {
        self.plugin_tools.insert(def.id.clone(), def);
    }

    /// Remove a plugin tool (when MCP server disconnects).
    pub fn unregister_plugin(&self, id: &str) {
        self.plugin_tools.remove(id);
    }

    /// Get a tool by ID.
    pub fn get(&self, id: &str) -> Option<ToolDef> {
        self.tools
            .get(id)
            .map(|r| r.clone())
            .or_else(|| {
                // Plugin tools don't implement Tool trait — wrap in adapter
                self.plugin_tools.get(id).map(|r| {
                    let plugin = r.clone();
                    let adapter: Arc<dyn Tool> = Arc::new(PluginToolAdapter { def: plugin });
                    ToolDef::new(adapter)
                })
            })
    }

    /// Check if a tool exists.
    pub fn has(&self, id: &str) -> bool {
        self.tools.contains_key(id) || self.plugin_tools.contains_key(id)
    }

    /// List all tool IDs (built-in + plugin).
    pub fn ids(&self) -> Vec<String> {
        let mut ids: Vec<String> = self.tools.iter().map(|r| r.key().clone()).collect();
        ids.extend(self.plugin_tools.iter().map(|r| r.key().clone()));
        ids.sort();
        ids
    }

    /// List all built-in tool defs.
    pub fn builtin_defs(&self) -> Vec<ToolDef> {
        self.tools.iter().map(|r| r.value().clone()).collect()
    }

    /// List all plugin tool defs.
    pub fn plugin_defs(&self) -> Vec<PluginToolDef> {
        self.plugin_tools.iter().map(|r| r.value().clone()).collect()
    }

    /// Get all LLM tool definitions for function calling.
    pub fn llm_definitions(&self) -> Vec<crate::provider::ToolDefinition> {
        let mut defs: Vec<_> = self
            .tools
            .iter()
            .map(|r| r.value().to_llm_definition())
            .collect();
        for entry in &self.plugin_tools {
            let p = entry.value();
            defs.push(crate::provider::ToolDefinition {
                name: p.id.clone(),
                description: p.description.clone(),
                parameters: p.json_schema.clone(),
            });
        }
        defs
    }
}

impl Default for ToolRegistry {
    fn default() -> Self {
        Self::new()
    }
}

/// Adapter that wraps a PluginToolDef behind the Tool trait.
struct PluginToolAdapter {
    def: PluginToolDef,
}

#[async_trait]
impl Tool for PluginToolAdapter {
    fn id(&self) -> &str {
        &self.def.id
    }

    fn description(&self) -> &str {
        &self.def.description
    }

    fn json_schema(&self) -> Option<serde_json::Value> {
        Some(self.def.json_schema.clone())
    }

    fn parameters_schema(&self) -> serde_json::Value {
        self.def.json_schema.clone()
    }

    async fn execute(
        &self,
        args: serde_json::Value,
        ctx: &ToolContext,
    ) -> crate::error::Result<ExecuteResult> {
        (self.def.execute)(args, ctx.clone()).await
    }
}

// ═══════════════════════════════════════════════════════════════════
// Output Truncation
// ═══════════════════════════════════════════════════════════════════

/// Output truncation configuration.
///
/// # Source
/// Ported from `packages/opencode/src/tool/truncate.ts`.
#[derive(Debug, Clone)]
pub struct TruncateConfig {
    /// Maximum number of characters (bytes in TS) in tool output
    pub max_chars: usize,
    /// Maximum number of lines
    pub max_lines: usize,
}

impl Default for TruncateConfig {
    fn default() -> Self {
        Self {
            max_chars: 100_000,
            max_lines: 5_000,
        }
    }
}

/// Result of truncating tool output.
#[derive(Debug, Clone)]
pub struct TruncateResult {
    /// The (possibly truncated) content
    pub content: String,
    /// Whether the output was truncated
    pub truncated: bool,
}

/// Truncate tool output to fit within configured limits.
///
/// # Source
/// Ported from `packages/opencode/src/tool/truncate.ts`.
#[must_use]
pub fn truncate_output(output: &str, config: &TruncateConfig) -> TruncateResult {
    let lines: Vec<&str> = output.lines().collect();
    let total_chars: usize = output.chars().count();

    if total_chars <= config.max_chars && lines.len() <= config.max_lines {
        return TruncateResult {
            content: output.to_string(),
            truncated: false,
        };
    }

    // Truncate by lines first
    let truncated_lines = std::cmp::min(lines.len(), config.max_lines);
    let mut result = String::new();
    let mut char_count = 0;

    for (i, line) in lines.iter().enumerate().take(truncated_lines) {
        let line_chars = line.chars().count() + 1; // +1 for newline
        if char_count + line_chars > config.max_chars {
            let remaining = config.max_chars - char_count;
            if remaining > 20 {
                // Partial line included
                result.push_str(&line.chars().take(remaining).collect::<String>());
                result.push_str("\n... (truncated)");
            } else {
                result.push_str("... (truncated)");
            }
            return TruncateResult {
                content: result,
                truncated: true,
            };
        }
        result.push_str(line);
        if i < truncated_lines - 1 {
            result.push('\n');
        }
        char_count += line_chars;
    }

    if lines.len() > truncated_lines {
        result.push_str(&format!(
            "\n... (truncated: {} lines > {} limit)",
            lines.len(),
            config.max_lines
        ));
    }

    TruncateResult {
        content: result,
        truncated: true,
    }
}

// ═══════════════════════════════════════════════════════════════════
// Streaming tool output
// ═══════════════════════════════════════════════════════════════════

/// Streaming tool output — events emitted during tool execution.
///
/// # Source
/// Ported from `packages/opencode/src/tool/tool.ts` streaming pattern.
#[derive(Debug, Clone)]
pub enum ToolOutputEvent {
    /// Text chunk produced by a running tool
    Text(String),
    /// Tool execution completed
    Complete(ExecuteResult),
    /// Tool encountered an error
    Error(String),
}

/// A tool whose output is streamed in real time.
///
/// # Source
/// Ported from the streaming tool pattern in OpenCode.
#[async_trait]
pub trait StreamingTool: Tool {
    /// Execute the tool with streaming output.
    ///
    /// The returned stream yields output events as they become available,
    /// with a final Complete or Error event.
    async fn execute_streaming(
        &self,
        args: serde_json::Value,
        ctx: &ToolContext,
    ) -> crate::error::Result<
        Box<dyn futures::Stream<Item = crate::error::Result<ToolOutputEvent>> + Send + Unpin>,
    >;
}

// ═══════════════════════════════════════════════════════════════════
// Stub tools for testing
// ═══════════════════════════════════════════════════════════════════

/// A no-op tool that always succeeds.
#[derive(Debug)]
pub struct NoopTool {
    id: String,
}

impl NoopTool {
    /// Create a no-op tool stub.
    pub fn new(id: impl Into<String>) -> Self {
        Self { id: id.into() }
    }
}

#[async_trait]
impl Tool for NoopTool {
    fn id(&self) -> &str {
        &self.id
    }

    fn description(&self) -> &str {
        "A no-op tool for testing"
    }

    fn parameters_schema(&self) -> serde_json::Value {
        serde_json::json!({
            "type": "object",
            "properties": {}
        })
    }

    async fn execute(
        &self,
        _args: serde_json::Value,
        _ctx: &ToolContext,
    ) -> crate::error::Result<ExecuteResult> {
        Ok(ExecuteResult {
            title: "noop".into(),
            output: "ok".into(),
            truncated: false,
            output_path: None,
            attachments: None,
            metadata: HashMap::new(),
        })
    }
}

// ═══════════════════════════════════════════════════════════════════
// Tests
// ═══════════════════════════════════════════════════════════════════

#[cfg(test)]
mod tests {
    use super::*;

    // ── ToolRegistry ──────────────────────────────────────────────

    #[test]
    fn test_registry_register_and_get() {
        let registry = ToolRegistry::new();
        let tool: Arc<dyn Tool> = Arc::new(NoopTool::new("test_tool"));
        registry.register(tool);

        let found = registry.get("test_tool");
        assert!(found.is_some());
        assert_eq!(found.unwrap().id, "test_tool");
    }

    #[test]
    fn test_registry_has() {
        let registry = ToolRegistry::new();
        assert!(!registry.has("missing"));

        let tool: Arc<dyn Tool> = Arc::new(NoopTool::new("exists"));
        registry.register(tool);
        assert!(registry.has("exists"));
    }

    #[test]
    fn test_registry_ids() {
        let registry = ToolRegistry::new();
        registry.register(Arc::new(NoopTool::new("tool_a")));
        registry.register(Arc::new(NoopTool::new("tool_b")));

        let ids = registry.ids();
        assert_eq!(ids.len(), 2);
        assert!(ids.contains(&"tool_a".to_string()));
        assert!(ids.contains(&"tool_b".to_string()));
    }

    #[test]
    fn test_registry_llm_definitions() {
        let registry = ToolRegistry::new();
        let tool: Arc<dyn Tool> = Arc::new(NoopTool::new("test_tool"));
        registry.register(tool);

        let defs = registry.llm_definitions();
        assert_eq!(defs.len(), 1);
        assert_eq!(defs[0].name, "test_tool");
    }

    #[test]
    fn test_registry_plugin_tool() {
        let registry = ToolRegistry::new();
        let plugin = PluginToolDef::new(
            "plugin_echo",
            "Echoes input",
            serde_json::json!({"type": "object", "properties": {"msg": {"type": "string"}}}),
            |args, _ctx| async move {
                Ok(ExecuteResult {
                    title: "echo".into(),
                    output: args["msg"].as_str().unwrap_or("").to_string(),
                    truncated: false,
                    output_path: None,
                    attachments: None,
                    metadata: HashMap::new(),
                })
            },
        );
        registry.register_plugin(plugin);
        assert!(registry.has("plugin_echo"));

        let defs = registry.llm_definitions();
        assert_eq!(defs.len(), 1);
        assert_eq!(defs[0].name, "plugin_echo");
    }

    #[test]
    fn test_registry_unregister_plugin() {
        let registry = ToolRegistry::new();
        registry.register_plugin(PluginToolDef::new(
            "temp",
            "temp",
            serde_json::json!({}),
            |_, _| async {
                Ok(ExecuteResult {
                    title: "".into(),
                    output: "".into(),
                    truncated: false,
                    output_path: None,
                    attachments: None,
                    metadata: HashMap::new(),
                })
            },
        ));
        assert!(registry.has("temp"));
        registry.unregister_plugin("temp");
        assert!(!registry.has("temp"));
    }

    // ── ToolDef ───────────────────────────────────────────────────

    #[test]
    fn test_tool_def_new() {
        let tool: Arc<dyn Tool> = Arc::new(NoopTool::new("my_tool"));
        let def = ToolDef::new(tool);
        assert_eq!(def.id, "my_tool");
        assert!(def.description.contains("no-op"));
    }

    #[test]
    fn test_tool_def_to_llm_definition() {
        let tool: Arc<dyn Tool> = Arc::new(NoopTool::new("my_tool"));
        let def = ToolDef::new(tool);
        let llm_def = def.to_llm_definition();
        assert_eq!(llm_def.name, "my_tool");
        assert!(llm_def.description.contains("no-op"));
    }

    // ── ExecuteResult ─────────────────────────────────────────────

    #[test]
    fn test_execute_result_serialize() {
        let result = ExecuteResult {
            title: "Test".into(),
            output: "hello".into(),
            truncated: false,
            output_path: None,
            attachments: None,
            metadata: HashMap::new(),
        };
        let json = serde_json::to_value(&result).unwrap();
        assert_eq!(json["title"], "Test");
        assert_eq!(json["output"], "hello");
    }

    #[test]
    fn test_execute_result_truncated() {
        let result = ExecuteResult {
            title: "Test".into(),
            output: "partial...".into(),
            truncated: true,
            output_path: Some("/tmp/full_output.txt".into()),
            attachments: None,
            metadata: HashMap::new(),
        };
        let json = serde_json::to_string(&result).unwrap();
        assert!(json.contains("truncated"));
        assert!(json.contains("output_path"));
    }

    // ── ToolContext ───────────────────────────────────────────────

    #[test]
    fn test_tool_context_default() {
        let ctx = ToolContext {
            session_id: "ses_1".into(),
            message_id: "msg_1".into(),
            agent: "claude".into(),
            abort: CancellationToken::new(),
            call_id: None,
            extra: HashMap::new(),
            messages: vec![],
        };
        assert_eq!(ctx.session_id, "ses_1");
        assert!(ctx.call_id.is_none());
        assert!(ctx.extra.is_empty());
    }

    #[test]
    fn test_tool_context_with_call_id() {
        let ctx = ToolContext {
            session_id: "ses_1".into(),
            message_id: "msg_1".into(),
            agent: "claude".into(),
            abort: CancellationToken::new(),
            call_id: Some("call_abc".into()),
            extra: HashMap::new(),
            messages: vec![],
        };
        assert_eq!(ctx.call_id, Some("call_abc".into()));
    }

    // ── truncate_output ──────────────────────────────────────────

    #[test]
    fn test_truncate_no_truncation_needed() {
        let result = truncate_output("short output", &TruncateConfig::default());
        assert!(!result.truncated);
        assert_eq!(result.content, "short output");
    }

    #[test]
    fn test_truncate_by_lines() {
        let config = TruncateConfig {
            max_chars: 1_000_000,
            max_lines: 3,
        };
        let output = "line1\nline2\nline3\nline4\nline5";
        let result = truncate_output(output, &config);
        assert!(result.truncated);
        // Should contain first 3 lines
        assert!(result.content.contains("line1"));
        assert!(result.content.contains("line2"));
        assert!(result.content.contains("line3"));
        assert!(!result.content.contains("line5"));
    }

    #[test]
    fn test_truncate_by_chars() {
        let config = TruncateConfig {
            max_chars: 10,
            max_lines: 1_000_000,
        };
        let output = "0123456789ABCDEF";
        let result = truncate_output(output, &config);
        assert!(result.truncated);
        assert!(result.content.len() <= 25); // 10 chars + truncation message
    }

    #[test]
    fn test_truncate_empty() {
        let result = truncate_output("", &TruncateConfig::default());
        assert!(!result.truncated);
        assert_eq!(result.content, "");
    }

    // ── ToolInfo ──────────────────────────────────────────────────

    #[test]
    fn test_tool_info_instantiate() {
        let info = ToolInfo::new("test_tool", || Box::new(NoopTool::new("test_tool")));
        let tool = info.instantiate();
        assert_eq!(tool.id(), "test_tool");
    }

    // ── NoopTool ──────────────────────────────────────────────────

    #[tokio::test]
    async fn test_noop_tool_execute() {
        let tool = NoopTool::new("noop");
        let ctx = ToolContext {
            session_id: "s".into(),
            message_id: "m".into(),
            agent: "claude".into(),
            abort: CancellationToken::new(),
            call_id: None,
            extra: HashMap::new(),
            messages: vec![],
        };
        let result = tool
            .execute(serde_json::json!({}), &ctx)
            .await
            .unwrap();
        assert_eq!(result.title, "noop");
        assert_eq!(result.output, "ok");
    }

    // ── Plugin tool execute ───────────────────────────────────────

    #[tokio::test]
    async fn test_plugin_tool_execute() {
        let plugin = PluginToolDef::new(
            "echo",
            "Echoes",
            serde_json::json!({"type": "object", "properties": {"msg": {"type": "string"}}}),
            |args, _ctx| async move {
                Ok(ExecuteResult {
                    title: "echo".into(),
                    output: format!("got: {}", args["msg"].as_str().unwrap_or("")),
                    truncated: false,
                    output_path: None,
                    attachments: None,
                    metadata: HashMap::new(),
                })
            },
        );

        let ctx = ToolContext {
            session_id: "ses_1".into(),
            message_id: "msg_1".into(),
            agent: "claude".into(),
            abort: CancellationToken::new(),
            call_id: None,
            extra: HashMap::new(),
            messages: vec![],
        };

        let adapter = PluginToolAdapter { def: plugin };
        let result = adapter
            .execute(serde_json::json!({"msg": "hello"}), &ctx)
            .await
            .unwrap();
        assert_eq!(result.output, "got: hello");
    }

    // ── format_validation_error ───────────────────────────────────

    #[test]
    fn test_format_validation_error() {
        let tool = NoopTool::new("bash");
        let err = tool.format_validation_error("missing command field");
        assert!(err.contains("bash"));
        assert!(err.contains("invalid arguments"));
        assert!(err.contains("missing command field"));
        assert!(err.contains("rewrite"));
    }

    // ── StreamingTool trait ───────────────────────────────────────

    /// Test that StreamingTool can be used through the Tool trait
    #[test]
    fn test_streaming_tool_is_tool() {
        struct StreamingNoop {
            id: String,
        }

        #[async_trait]
        impl Tool for StreamingNoop {
            fn id(&self) -> &str {
                &self.id
            }
            fn description(&self) -> &str {
                "streaming noop"
            }
            fn parameters_schema(&self) -> serde_json::Value {
                serde_json::json!({})
            }
            async fn execute(
                &self,
                _args: serde_json::Value,
                _ctx: &ToolContext,
            ) -> crate::error::Result<ExecuteResult> {
                Ok(ExecuteResult {
                    title: "ok".into(),
                    output: "ok".into(),
                    truncated: false,
                    output_path: None,
                    attachments: None,
                    metadata: HashMap::new(),
                })
            }
        }

        #[async_trait]
        impl StreamingTool for StreamingNoop {
            async fn execute_streaming(
                &self,
                _args: serde_json::Value,
                _ctx: &ToolContext,
            ) -> crate::error::Result<
                Box<
                    dyn futures::Stream<Item = crate::error::Result<ToolOutputEvent>>
                        + Send
                        + Unpin,
                >,
            > {
                use futures::stream;
                Ok(Box::new(stream::once(async {
                    Ok(ToolOutputEvent::Complete(ExecuteResult {
                        title: "ok".into(),
                        output: "streamed".into(),
                        truncated: false,
                        output_path: None,
                        attachments: None,
                        metadata: HashMap::new(),
                    }))
                })))
            }
        }

        let tool = StreamingNoop {
            id: "streaming".into(),
        };
        assert_eq!(tool.id(), "streaming");
    }
}
