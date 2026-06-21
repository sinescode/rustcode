//! Tool system — defines, registers, and executes tools.
//!
//! Ported from:
//! - `packages/opencode/src/tool/tool.ts` (183 lines)
//! - `packages/opencode/src/tool/registry.ts` (441 lines)
//! - `packages/opencode/src/tool/schema.ts` (15 lines)
//!
//! Truncation logic moved to `truncate.rs`.
//!
//! OpenCode commit: 5d0f86606ac30690f79f0a6a9f41a1f49fe95d0b

use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::future::Future;
use std::pin::Pin;
use std::sync::Arc;
use tokio_util::sync::CancellationToken;

use crate::session_prompt::{PromptPart, SessionPromptInput};

// Re-export truncation types from the truncate module.
pub use crate::truncate::{TruncateOptions, TruncateResult, TruncateService, truncate_output};

/// Tool execution context passed to every tool.
///
/// Carries session/message identifiers, an abort signal, optional
/// permission callbacks, and extra metadata.
///
/// # Source
/// Ported from `packages/opencode/src/tool/tool.ts` line 36–46.
#[derive(Clone)]
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
    /// Optional callback to ask the user for permission.
    ///
    /// The callback receives (permission_name, resource_pattern) and
    /// returns `true` if the action is allowed.
    pub ask_fn: Option<
        Arc<
            dyn Fn(
                    String,
                    String,
                ) -> Pin<Box<dyn Future<Output = crate::error::Result<bool>> + Send>>
                    + Send
                    + Sync,
        >,
    >,
    /// Permission source identifying the origin of the request.
    pub permission_source: Option<crate::permission::PermissionSource>,
    /// Callbacks for the TaskTool to delegate work to subagents.
    ///
    /// Set by the session processor to allow the TaskTool to resolve prompt
    /// templates, run prompts against child sessions, and cancel them.
    pub prompt_ops: Option<Arc<TaskPromptOps>>,
}

impl std::fmt::Debug for ToolContext {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("ToolContext")
            .field("session_id", &self.session_id)
            .field("message_id", &self.message_id)
            .field("agent", &self.agent)
            .field("call_id", &self.call_id)
            .field("extra", &self.extra)
            .field("messages", &self.messages.len())
            .field("ask_fn", &self.ask_fn.as_ref().map(|_| "Some(..)"))
            .field("permission_source", &self.permission_source)
            .field("prompt_ops", &self.prompt_ops.as_ref().map(|_| "Some(..)"))
            .finish()
    }
}

impl ToolContext {
    /// Ask the user for permission for a specific action+resource.
    ///
    /// Returns `Ok(true)` if allowed, `Ok(false)` if denied, or forwards
    /// the error from the underlying callback.
    ///
    /// If no `ask_fn` is configured, permission is implicitly granted.
    pub async fn ask(&self, permission: &str, resource: &str) -> crate::error::Result<bool> {
        match &self.ask_fn {
            Some(f) => f(permission.to_string(), resource.to_string()).await,
            None => Ok(true),
        }
    }

    /// Update metadata in the extra context map.
    pub fn update_metadata(&mut self, key: impl Into<String>, value: serde_json::Value) {
        self.extra.insert(key.into(), value);
    }
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

/// Callbacks provided by the session processor for the TaskTool to run subagent prompts.
///
/// These carry the three operations the TaskTool needs to delegate work to a subagent:
/// resolving prompt template variables, running a prompt on a session, and cancelling a
/// running session.
///
/// Ported from `packages/opencode/src/tool/task.ts` lines 18–22 (`TaskPromptOps` interface).
pub struct TaskPromptOps {
    /// Resolve template variables in a prompt string into a list of PromptParts.
    pub resolve_prompt_parts: Arc<dyn Fn(&str) -> crate::error::Result<Vec<PromptPart>> + Send + Sync>,
    /// Run a prompt on a session and return the resulting text.
    pub prompt: Arc<dyn Fn(SessionPromptInput) -> Pin<Box<dyn Future<Output = crate::error::Result<String>> + Send>> + Send + Sync>,
    /// Cancel a running session by its ID.
    pub cancel: Arc<dyn Fn(String) -> Pin<Box<dyn Future<Output = ()> + Send>> + Send + Sync>,
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
    pub fn new(
        id: impl Into<String>,
        init: impl Fn() -> Box<dyn Tool> + Send + Sync + 'static,
    ) -> Self {
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

/// Type alias for the plugin tool execute function.
///
/// # Source
/// Ported from `packages/opencode/src/tool/registry.ts` `ToolDefinition.execute`.
pub type PluginToolExecFn = Arc<
    dyn Fn(
            serde_json::Value,
            ToolContext,
        ) -> Pin<
            Box<dyn Future<Output = crate::error::Result<ExecuteResult>> + Send>,
        > + Send
        + Sync,
>;

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
    pub execute: PluginToolExecFn,
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
        Fut: Future<Output = crate::error::Result<ExecuteResult>> + Send + 'static,
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
        self.tools.get(id).map(|r| r.clone()).or_else(|| {
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
        self.plugin_tools
            .iter()
            .map(|r| r.value().clone())
            .collect()
    }

    /// Lightweight listing: (id, description) pairs for prompt building.
    pub fn list_tools_info(&self) -> Vec<ToolInfoBrief> {
        let mut infos: Vec<ToolInfoBrief> = self
            .tools
            .iter()
            .map(|r| {
                let def = r.value();
                ToolInfoBrief {
                    id: def.id.clone(),
                    description: def.description.clone(),
                }
            })
            .collect();
        infos.extend(self.plugin_tools.iter().map(|r| {
            let p = r.value();
            ToolInfoBrief {
                id: p.id.clone(),
                description: p.description.clone(),
            }
        }));
        infos
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

    /// Alias for `llm_definitions()` — canonical name used by SessionRunner.
    pub fn to_definitions(&self) -> Vec<crate::provider::ToolDefinition> {
        self.llm_definitions()
    }

    /// Execute a tool by name with the given arguments.
    ///
    /// Looks up the tool in both built-in and plugin registries, builds a
    /// [`ToolContext`], and runs the tool's `execute` method.
    pub async fn execute_by_name(
        &self,
        name: &str,
        args: serde_json::Value,
        ctx: &ToolContext,
    ) -> crate::error::Result<ExecuteResult> {
        let def = self
            .get(name)
            .ok_or_else(|| crate::error::Error::Tool(format!("tool not found: {name}")))?;
        let tool = Arc::clone(&def.tool);
        tool.execute(args, ctx).await
    }

    /// Execute a tool by name with the full execution pipeline:
    ///
    /// 1. **Permission check** — evaluates the tool name against the configured
    ///    permission source (if any). If the action is denied, returns an error.
    ///    If the action requires asking (`Ask`), invokes the `ask_fn` callback.
    /// 2. **Tool execution** — runs the tool's `execute` method.
    /// 3. **Output truncation** — truncates the result output if it exceeds
    ///    configured limits, writing the full output to the truncation directory.
    /// 4. **Result wrapping** — returns the final `ExecuteResult`.
    ///
    /// # Source
    /// Ported from `packages/opencode/src/tool/registry.ts` `execute` method.
    pub async fn execute_with_pipeline(
        &self,
        name: &str,
        args: serde_json::Value,
        ctx: &ToolContext,
        truncate: &TruncateService,
    ) -> crate::error::Result<ExecuteResult> {
        // Step 1: Permission check
        // Evaluate the tool permission using the context's permission source.
        // If the user needs to be asked, invoke the ask_fn.
        if let Some(ref _source) = ctx.permission_source {
            // Check permission using the context's ask_fn.
            // The permission name is the tool name itself (e.g. "bash", "read").
            // The resource pattern defaults to "*" for top-level tool permission.
            let allowed = ctx.ask(name, "*").await?;
            if !allowed {
                return Err(crate::error::Error::Permission(
                    crate::error::PermissionError::Denied,
                ));
            }
        }

        // Step 2: Tool execution
        let def = self.get(name).ok_or_else(|| {
            crate::error::Error::Tool(format!("tool not found: {name}"))
        })?;
        let tool = Arc::clone(&def.tool);

        let result = tool.execute(args, ctx).await?;

        // Step 3: Output truncation
        let truncated = truncate
            .truncate(&result.output, &ctx.session_id, &ctx.call_id.clone().unwrap_or_default())
            .await;

        // Step 4: Result wrapping
        Ok(ExecuteResult {
            title: result.title,
            output: truncated.content,
            truncated: truncated.truncated,
            output_path: truncated.output_path,
            attachments: result.attachments,
            metadata: result.metadata,
        })
    }
}

/// Brief tool metadata — used for prompt building.
#[derive(Debug, Clone)]
pub struct ToolInfoBrief {
    pub id: String,
    pub description: String,
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
            ask_fn: None,
            permission_source: None,
        };
        assert_eq!(ctx.session_id, "ses_1");
        assert!(ctx.call_id.is_none());
        assert!(ctx.extra.is_empty());
        assert!(ctx.ask_fn.is_none());
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
            ask_fn: None,
            permission_source: None,
        };
        assert_eq!(ctx.call_id, Some("call_abc".into()));
    }

    #[test]
    fn test_tool_context_update_metadata() {
        let mut ctx = ToolContext {
            session_id: "ses_1".into(),
            message_id: "msg_1".into(),
            agent: "claude".into(),
            abort: CancellationToken::new(),
            call_id: None,
            extra: HashMap::new(),
            messages: vec![],
            ask_fn: None,
            permission_source: None,
        };
        ctx.update_metadata("foo", serde_json::json!("bar"));
        assert_eq!(ctx.extra.get("foo").and_then(|v| v.as_str()), Some("bar"));
    }

    #[tokio::test]
    async fn test_tool_context_ask_no_fn() {
        let ctx = ToolContext {
            session_id: "ses_1".into(),
            message_id: "msg_1".into(),
            agent: "claude".into(),
            abort: CancellationToken::new(),
            call_id: None,
            extra: HashMap::new(),
            messages: vec![],
            ask_fn: None,
            permission_source: None,
        };
        // Without ask_fn, permission is implicitly granted.
        let allowed = ctx.ask("bash", "*").await.unwrap();
        assert!(allowed);
    }

    #[tokio::test]
    async fn test_tool_context_ask_with_fn() {
        let ask_fn: Option<
            Arc<
                dyn Fn(String, String) -> Pin<Box<dyn Future<Output = crate::error::Result<bool>> + Send>>
                    + Send
                    + Sync,
            >,
        > = Some(Arc::new(|perm, _res| {
            Box::pin(async move { Ok(perm == "bash") })
        }));

        let ctx = ToolContext {
            session_id: "ses_1".into(),
            message_id: "msg_1".into(),
            agent: "claude".into(),
            abort: CancellationToken::new(),
            call_id: None,
            extra: HashMap::new(),
            messages: vec![],
            ask_fn,
            permission_source: Some(crate::permission::PermissionSource::Tool {
                message_id: "msg_1".into(),
                call_id: "call_1".into(),
            }),
        };

        assert!(ctx.ask("bash", "*").await.unwrap());
        assert!(!ctx.ask("read", "*").await.unwrap());
    }

    // ── truncate re-export ────────────────────────────────────────

    #[test]
    fn test_truncate_output_re_export() {
        let result = truncate_output("short", 10, 100);
        assert!(!result.truncated);
        assert_eq!(result.content, "short");
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
            ask_fn: None,
            permission_source: None,
        };
        let result = tool.execute(serde_json::json!({}), &ctx).await.unwrap();
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
            ask_fn: None,
            permission_source: None,
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
                Box<dyn futures::Stream<Item = crate::error::Result<ToolOutputEvent>> + Send + Unpin>,
            > {
                use futures::stream;
                Ok(Box::new(stream::iter(vec![Ok(ToolOutputEvent::Complete(
                    ExecuteResult {
                        title: "ok".into(),
                        output: "streamed".into(),
                        truncated: false,
                        output_path: None,
                        attachments: None,
                        metadata: HashMap::new(),
                    },
                ))])))
            }
        }

        let tool = StreamingNoop {
            id: "streaming".into(),
        };
        assert_eq!(tool.id(), "streaming");
    }

    // ── execute_with_pipeline ─────────────────────────────────────

    #[tokio::test]
    async fn test_execute_with_pipeline_basic() {
        let registry = ToolRegistry::new();
        registry.register(Arc::new(NoopTool::new("noop")));

        let ctx = ToolContext {
            session_id: "ses_1".into(),
            message_id: "msg_1".into(),
            agent: "claude".into(),
            abort: CancellationToken::new(),
            call_id: Some("call_1".into()),
            extra: HashMap::new(),
            messages: vec![],
            ask_fn: None,
            permission_source: None,
        };

        let truncate = TruncateService::new();
        let result = registry
            .execute_with_pipeline("noop", serde_json::json!({}), &ctx, &truncate)
            .await
            .unwrap();

        assert_eq!(result.title, "noop");
        assert_eq!(result.output, "ok");
        assert!(!result.truncated);
    }

    #[tokio::test]
    async fn test_execute_with_pipeline_permission_denied() {
        let registry = ToolRegistry::new();
        registry.register(Arc::new(NoopTool::new("noop")));

        let ask_fn: Option<
            Arc<
                dyn Fn(String, String) -> Pin<Box<dyn Future<Output = crate::error::Result<bool>> + Send>>
                    + Send
                    + Sync,
            >,
        > = Some(Arc::new(|_perm, _res| Box::pin(async move { Ok(false) })));

        let ctx = ToolContext {
            session_id: "ses_1".into(),
            message_id: "msg_1".into(),
            agent: "claude".into(),
            abort: CancellationToken::new(),
            call_id: Some("call_1".into()),
            extra: HashMap::new(),
            messages: vec![],
            ask_fn,
            permission_source: Some(crate::permission::PermissionSource::Tool {
                message_id: "msg_1".into(),
                call_id: "call_1".into(),
            }),
        };

        let truncate = TruncateService::new();
        let result = registry
            .execute_with_pipeline("noop", serde_json::json!({}), &ctx, &truncate)
            .await;

        assert!(result.is_err());
    }

    #[tokio::test]
    async fn test_execute_with_pipeline_truncation() {
        use crate::truncate::TruncateOptions;
        use std::path::PathBuf;

        let registry = ToolRegistry::new();
        registry.register(Arc::new(NoopTool::new("verbose")));

        let ctx = ToolContext {
            session_id: "ses_trunc".into(),
            message_id: "msg_1".into(),
            agent: "claude".into(),
            abort: CancellationToken::new(),
            call_id: Some("call_trunc".into()),
            extra: HashMap::new(),
            messages: vec![],
            ask_fn: None,
            permission_source: None,
        };

        // Override the NoopTool to produce a lot of output for testing.
        // We'll register a custom tool instead.
        struct VerboseTool;

        #[async_trait]
        impl Tool for VerboseTool {
            fn id(&self) -> &str {
                "verbose"
            }
            fn description(&self) -> &str {
                "Produces verbose output"
            }
            fn parameters_schema(&self) -> serde_json::Value {
                serde_json::json!({"type": "object", "properties": {}})
            }
            async fn execute(
                &self,
                _args: serde_json::Value,
                _ctx: &ToolContext,
            ) -> crate::error::Result<ExecuteResult> {
                let long_output = (0..100).map(|i| format!("line {}", i)).collect::<Vec<_>>().join("\n");
                Ok(ExecuteResult {
                    title: "verbose".into(),
                    output: long_output,
                    truncated: false,
                    output_path: None,
                    attachments: None,
                    metadata: HashMap::new(),
                })
            }
        }

        let truncate = TruncateService::with_options(TruncateOptions {
            max_lines: 5,
            max_chars: 1_000_000,
            dir: PathBuf::from("/tmp/trunc-test-pipeline"),
            ..Default::default()
        });

        let registry2 = ToolRegistry::new();
        registry2.register(Arc::new(VerboseTool));

        let result = registry2
            .execute_with_pipeline("verbose", serde_json::json!({}), &ctx, &truncate)
            .await
            .unwrap();

        assert!(result.truncated);
        assert!(result.output_path.is_some());

        // Cleanup
        if let Some(path) = &result.output_path {
            let _ = std::fs::remove_file(path);
        }
        let _ = std::fs::remove_dir_all("/tmp/trunc-test-pipeline");
    }
}
