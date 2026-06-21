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
// Tool prompt templates — ported from packages/opencode/src/tool/*.txt
// ═══════════════════════════════════════════════════════════════════

/// Prompt template for the `edit` tool.
pub const PROMPT_EDIT: &str = r"Performs exact string replacements in files. 

Usage:
- You must use your `Read` tool at least once in the conversation before editing. This tool will error if you attempt an edit without reading the file. 
- When editing text from Read tool output, ensure you preserve the exact indentation (tabs/spaces) as it appears AFTER the line number prefix. The line number prefix format is: line number + colon + space (e.g., `1: `). Everything after that space is the actual file content to match. Never include any part of the line number prefix in the oldString or newString.
- ALWAYS prefer editing existing files in the codebase. NEVER write new files unless explicitly required.
- Only use emojis if the user explicitly requests it. Avoid adding emojis to files unless asked.
- The edit will FAIL if `oldString` is not found in the file with an error ""oldString not found in content"".
- The edit will FAIL if `oldString` is found multiple times in the file with an error ""Found multiple matches for oldString. Provide more surrounding lines in oldString to identify the correct match."" Either provide a larger string with more surrounding context to make it unique or use `replaceAll` to change every instance of `oldString`. 
- Use `replaceAll` for replacing and renaming strings across the file. This parameter is useful if you want to rename a variable for instance.";

/// Prompt template for the `read` tool.
pub const PROMPT_READ: &str = r"Read a file or directory from the local filesystem. If the path does not exist, an error is returned.

Usage:
- The filePath parameter should be an absolute path.
- By default, this tool returns up to 2000 lines from the start of the file.
- The offset parameter is the line number to start from (1-indexed).
- To read later sections, call this tool again with a larger offset.
- Use the grep tool to find specific content in large files or files with long lines.
- If you are unsure of the correct file path, use the glob tool to look up filenames by glob pattern.
- Contents are returned with each line prefixed by its line number as `<line>: <content>`. For example, if a file has contents ""foo\n"", you will receive ""1: foo\n"". For directories, entries are returned one per line (without line numbers) with a trailing `/` for subdirectories.
- Any line longer than 2000 characters is truncated.
- Call this tool in parallel when you know there are multiple files you want to read.
- Avoid tiny repeated slices (30 line chunks). If you need more context, read a larger window.
- This tool can read image files and PDFs and return them as file attachments.";

/// Prompt template for the `write` tool.
pub const PROMPT_WRITE: &str = r"Writes a file to the local filesystem.

Usage:
- This tool will overwrite the existing file if there is one at the provided path.
- If this is an existing file, you MUST use the Read tool first to read the file's contents. This tool will fail if you did not read the file first.
- ALWAYS prefer editing existing files in the codebase. NEVER write new files unless explicitly required.
- NEVER proactively create documentation files (*.md) or README files. Only create documentation files if explicitly requested by the User.
- Only use emojis if the user explicitly requests it. Avoid writing emojis to files unless asked.";

/// Prompt template for the `glob` tool.
pub const PROMPT_GLOB: &str = r"- Fast file pattern matching tool that works with any codebase size
- Supports glob patterns like ""**/*.js"" or ""src/**/*.ts""
- Returns matching file paths
- Use this tool when you need to find files by name patterns
- When you are doing an open-ended search that may require multiple rounds of globbing and grepping, use the Task tool instead
- You have the capability to call multiple tools in a single response. It is always better to speculatively perform multiple searches as a batch that are potentially useful.";

/// Prompt template for the `grep` tool.
pub const PROMPT_GREP: &str = r"- Fast content search tool that works with any codebase size
- Searches file contents using regular expressions
- Supports full regex syntax (eg. ""log.*Error"", ""function\s+\w+"", etc.)
- Filter files by pattern with the include parameter (eg. ""*.js"", ""*.{ts,tsx}"")
- Returns file paths and line numbers with matching lines
- Use this tool when you need to find files containing specific patterns
- If you need to identify/count the number of matches within files, use the Bash tool with `rg` (ripgrep) directly. Do NOT use `grep`.
- When you are doing an open-ended search that may require multiple rounds of globbing and grepping, use the Task tool instead";

/// Prompt template for the `webfetch` tool.
pub const PROMPT_WEBFETCH: &str = r"- Fetches content from a specified URL
- Takes a URL and optional format as input
- Fetches the URL content, converts to requested format (markdown by default)
- Returns the content in the specified format
- Use this tool when you need to retrieve and analyze web content

Usage notes:
  - IMPORTANT: if another tool is present that offers better web fetching capabilities, is more targeted to the task, or has fewer restrictions, prefer using that tool instead of this one.
  - The URL must be a fully-formed valid URL
  - HTTP URLs will be automatically upgraded to HTTPS
  - Format options: ""markdown"" (default), ""text"", or ""html""
  - This tool is read-only and does not modify any files
  - Results may be summarized if the content is very large";

/// Prompt template for the `websearch` tool.
pub const PROMPT_WEBSEARCH: &str = r"- Search the web using the session's web search provider - performs real-time web searches and can scrape content from specific URLs
- Provides up-to-date information for current events and recent data
- Supports configurable result counts and returns the content from the most relevant websites
- Use this tool for accessing information beyond knowledge cutoff
- Searches are performed automatically within a single API call

Usage notes:
  - Supports live crawling modes when available: 'fallback' (backup if cached unavailable) or 'preferred' (prioritize live crawling)
  - Search types when available: 'auto' (balanced), 'fast' (quick results), 'deep' (comprehensive search)
  - Configurable context length for optimal LLM integration
  - Domain filtering and advanced search options available

The current year is {{year}}. You MUST use this year when searching for recent information or current events
- Example: If the current year is 2026 and the user asks for ""latest AI news"", search for ""AI news 2026"", NOT ""AI news 2025""";

/// Prompt template for the `question` tool.
pub const PROMPT_QUESTION: &str = r"Use this tool when you need to ask the user questions during execution. This allows you to:
1. Gather user preferences or requirements
2. Clarify ambiguous instructions
3. Get decisions on implementation choices as you work
4. Offer choices to the user about what direction to take.

Usage notes:
- When `custom` is enabled (default), a ""Type your own answer"" option is added automatically; don't include ""Other"" or catch-all options
- Answers are returned as arrays of labels; set `multiple: true` to allow selecting more than one
- If you recommend a specific option, make that the first option in the list and add ""(Recommended)"" at the end of the label";

/// Prompt template for the `skill` tool.
pub const PROMPT_SKILL: &str = r"Load a specialized skill when the task at hand matches one of the skills listed in the system prompt.

Use this tool to inject the skill's instructions and resources into current conversation. The output may contain detailed workflow guidance as well as references to scripts, files, etc in the same directory as the skill.

The skill name must match one of the skills listed in your system prompt.";

/// Prompt template for the `task` tool.
pub const PROMPT_TASK: &str = r"Launch a new agent to handle complex, multistep tasks autonomously.

When using the Task tool, you must specify a subagent_type parameter to select which agent type to use.

When NOT to use the Task tool:
- If you want to read a specific file path, use the Read or Glob tool instead of the Task tool, to find the match more quickly
- If you are searching for a specific class definition like ""class Foo"", use the Grep tool instead, to find the match more quickly
- If you are searching for code within a specific file or set of 2-3 files, use the Read tool instead of the Task tool, to find the match more quickly
- If no available agent is a good fit for the task, use other tools directly


Usage notes:
1. Launch multiple agents concurrently whenever possible, to maximize performance; to do that, use a single message with multiple tool uses
2. Once you have delegated work to an agent, do not duplicate that work yourself. Continue with non-overlapping tasks, or wait for the result. For background tasks, you will be notified automatically when the result is ready.
3. When the agent is done, it will return a single message back to you. The result returned by the agent is not visible to the user. To show the user the result, you should send a text message back to the user with a concise summary of the result. The output includes a task_id you can reuse later to continue the same subagent session.
4. Each agent invocation starts with a fresh context unless you provide task_id to resume the same subagent session (which continues with its previous messages and tool outputs). When starting fresh, your prompt should contain a highly detailed task description for the agent to perform autonomously and you should specify exactly what information the agent should return back to you in its final and only message to you.
5. The agent's outputs should generally be trusted
6. Clearly tell the agent whether you expect it to write code or just to do research (search, file reads, web fetches, etc.), since it is not aware of the user's intent. Tell it how to verify its work if possible (e.g., relevant test commands).
7. If the agent description mentions that it should be used proactively, then you should try your best to use it without the user having to ask for it first. Use your judgement.";

/// Prompt template for the `todowrite` tool.
pub const PROMPT_TODOWRITE: &str = r"Create and maintain a structured task list for the current coding session. Tracks progress, organizes multi-step work, and surfaces status to the user.

## When to use
Use proactively when:
- The task requires 3+ distinct steps or actions (not just 3 tool calls for a single conceptual step)
- The work is non-trivial and benefits from planning
- The user provides multiple tasks (numbered or comma-separated) or explicitly asks for a todo list
- New instructions arrive - capture them as todos
- You start a task - mark it `in_progress` (only one at a time) before working
- You finish a task - mark it `completed` and add any follow-ups discovered during the work

## When NOT to use
Skip when:
- The work is a single, straightforward task (or <3 trivial steps)
- The request is purely informational or conversational
- Tracking adds no organizational value

## States
- `pending` - not started
- `in_progress` - actively working (exactly ONE at a time)
- `completed` - finished successfully
- `cancelled` - no longer needed

## Rules
- Update status in real time; don't batch completions
- Mark `completed` only after the required work is actually done, including any required verification. Never based on intent.
- Keep exactly one `in_progress` while work remains
- If blocked or partial, keep it `in_progress` and add a follow-up todo describing the blocker
- Preserve user-provided commands verbatim (flags, args, order)
- Items should be specific and actionable; break large work into smaller steps

## Examples

Use it:
- ""Add a dark mode toggle and run the tests"" -> multi-step feature + explicit verification
- ""Rename getCwd -> getCurrentWorkingDirectory across the repo"" -> grep reveals 15 occurrences in 8 files
- ""Implement registration, catalog, cart, checkout"" -> multiple complex features

Skip it:
- ""How do I print Hello World in Python?"" -> informational
- ""Add a comment to calculateTotal"" -> single edit
- ""Run npm install and tell me what happened"" -> one command

When in doubt, use it.";

/// Prompt template for the `apply_patch` tool.
pub const PROMPT_APPLY_PATCH: &str = r"Use the `apply_patch` tool to edit files. Your patch language is a stripped‑down, file‑oriented diff format designed to be easy to parse and safe to apply. You can think of it as a high‑level envelope:

*** Begin Patch
[ one or more file sections ]
*** End Patch

Within that envelope, you get a sequence of file operations.
You MUST include a header to specify the action you are taking.
Each operation starts with one of three headers:

*** Add File: <path> - create a new file. Every following line is a + line (the initial contents).
*** Delete File: <path> - remove an existing file. Nothing follows.
*** Update File: <path> - patch an existing file in place (optionally with a rename).

Example patch:

```
*** Begin Patch
*** Add File: hello.txt
+Hello world
*** Update File: src/app.py
*** Move to: src/main.py
@@ def greet():
-print(""Hi"")
+print(""Hello, world!"")
*** Delete File: obsolete.txt
*** End Patch
```

It is important to remember:

- You must include a header with your intended action (Add/Delete/Update)
- You must prefix new lines with `+` even when creating a new file";

/// Prompt template for the `lsp` tool.
pub const PROMPT_LSP: &str = r"Interact with Language Server Protocol (LSP) servers to get code intelligence features.

Supported operations:
- goToDefinition: Find where a symbol is defined
- findReferences: Find all references to a symbol
- hover: Get hover information (documentation, type info) for a symbol
- documentSymbol: Get all symbols (functions, classes, variables) in a document
- workspaceSymbol: List project-wide symbols matching a query string
- goToImplementation: Find implementations of an interface or abstract method
- prepareCallHierarchy: Get call hierarchy item at a position (functions/methods)
- incomingCalls: Find all functions/methods that call the function at a position
- outgoingCalls: Find all functions/methods called by the function at a position

All operations require:
- filePath: The file to operate on
- line: The line number (1-based, as shown in editors)
- character: The character offset (1-based, as shown in editors)

workspaceSymbol also accepts:
- query: A query string to filter symbols by. Empty string requests all symbols.

For workspaceSymbol, filePath is not sent in the LSP workspace/symbol request. It is used by opencode to select and start the matching LSP server.

Note: LSP servers must be configured for the file type. If no server is available, an error will be returned.";

/// Prompt template for the `shell` tool.
pub const PROMPT_SHELL: &str = r"${intro}

Be aware: OS: ${os}, Shell: ${shell}

${workdirSection}

Use `${tmp}` for temporary work outside the workspace. This directory has already been created, already exists, and is pre-approved for external directory access.

IMPORTANT: This tool is for terminal operations like git, npm, docker, etc. DO NOT use it for file operations (reading, writing, editing, searching, finding files) - use the specialized tools for this instead.

${commandSection}

# Git and GitHub
- Only commit, amend, push, or create PRs when explicitly requested.
- Before committing, inspect `git status`, `git diff`, and `git log --oneline -10`; stage only intended files and never commit secrets.
- Write a concise commit message that matches the repo style.
- Do not update git config, skip hooks, use interactive `-i`, force-push, or create empty commits unless explicitly requested.
- If a commit fails or hooks reject it, fix the issue and create a new commit; do not amend the failed commit.
- Before creating a PR, inspect status, diff, remote tracking, recent commits, and the diff from the base branch.
- Review all commits included in the PR, not just the latest commit.
- Use `gh` for GitHub tasks, including PRs, issues, checks, and releases; return the PR URL when done.";

/// All tool prompt templates keyed by tool ID.
pub const TOOL_PROMPTS: &[(&str, &str)] = &[
    ("edit", PROMPT_EDIT),
    ("read", PROMPT_READ),
    ("write", PROMPT_WRITE),
    ("glob", PROMPT_GLOB),
    ("grep", PROMPT_GREP),
    ("webfetch", PROMPT_WEBFETCH),
    ("websearch", PROMPT_WEBSEARCH),
    ("question", PROMPT_QUESTION),
    ("skill", PROMPT_SKILL),
    ("task", PROMPT_TASK),
    ("todowrite", PROMPT_TODOWRITE),
    ("apply_patch", PROMPT_APPLY_PATCH),
    ("lsp", PROMPT_LSP),
    ("shell", PROMPT_SHELL),
];

/// Get the prompt template for a tool by its ID.
pub fn get_tool_prompt(id: &str) -> Option<&'static str> {
    TOOL_PROMPTS.iter().find(|(key, _)| *key == id).map(|(_, val)| *val)
}

// ═══════════════════════════════════════════════════════════════════
// JSON Schema helpers — ported from packages/opencode/src/tool/json-schema.ts
// ═══════════════════════════════════════════════════════════════════

/// Normalize a JSON Schema value — strips null from anyOf, unwraps single-element
/// anyOf, flattens allOf, and adds safe bounds for integer type.
///
/// Ported from `packages/opencode/src/tool/json-schema.ts` lines 28–88.
pub fn normalize_json_schema(value: &serde_json::Value) -> serde_json::Value {
    normalize_json(value, &NormalizeOptions { strip_null: false })
}

#[derive(Clone, Copy)]
struct NormalizeOptions {
    strip_null: bool,
}

fn normalize_json(value: &serde_json::Value, options: &NormalizeOptions) -> serde_json::Value {
    match value {
        serde_json::Value::Array(arr) => {
            serde_json::Value::Array(arr.iter().map(|v| normalize_json(v, options)).collect())
        }
        serde_json::Value::Object(obj) => {
            let is_record = true;
            let required: Option<Vec<String>> = obj.get("required").and_then(|r| {
                r.as_array().map(|a| {
                    a.iter()
                        .filter_map(|v| v.as_str().map(String::from))
                        .collect()
                })
            });

            let mut schema: serde_json::Map<String, serde_json::Value> = obj
                .iter()
                .map(|(key, val)| {
                    let new_val = if key == "properties" && val.is_object() {
                        serde_json::Value::Object(
                            val.as_object()
                                .unwrap()
                                .iter()
                                .map(|(name, prop)| {
                                    let opts = NormalizeOptions {
                                        strip_null: required
                                            .as_ref()
                                            .map(|r| !r.contains(name))
                                            .unwrap_or(false),
                                    };
                                    (name.clone(), normalize_json(prop, &opts))
                                })
                                .collect(),
                        )
                    } else {
                        normalize_json(val, &NormalizeOptions { strip_null: false })
                    };
                    (key.clone(), new_val)
                })
                .collect();

            // Strip `additionalProperties: true`
            if schema.get("additionalProperties") == Some(&serde_json::Value::Bool(true)) {
                schema.remove("additionalProperties");
            }

            // Handle stripNull
            if options.strip_null {
                if let Some(any_of) = schema.get("any_of").and_then(|v| v.as_array()) {
                    let filtered: Vec<&serde_json::Value> = any_of
                        .iter()
                        .filter(|item| match item {
                            serde_json::Value::Object(m) if m.get("type") == Some(&serde_json::Value::String("null".into())) => false,
                            _ => true,
                        })
                        .collect();
                    if filtered.len() != any_of.len() {
                        let mut rest = schema.clone();
                        rest.remove("any_of");
                        rest.insert("any_of".into(), serde_json::Value::Array(filtered.into_iter().cloned().collect()));
                        return normalize_json(&serde_json::Value::Object(rest), &NormalizeOptions { strip_null: false });
                    }
                }
            }

            // Handle anyOf unwrapping
            if let Some(any_of) = schema.get("any_of").and_then(|v| v.as_array()) {
                // Number + non-finite enum -> replace with number
                let number_item = any_of.iter().find(|item| {
                    item.as_object()
                        .and_then(|m| m.get("type"))
                        .and_then(|t| t.as_str())
                        == Some("number")
                });
                let non_finite_items: Vec<&serde_json::Value> = any_of
                    .iter()
                    .filter(|item| {
                        item.as_object()
                            .map(|m| m.get("enum").and_then(|e| e.as_array()))
                            .flatten()
                            .map(|arr| {
                                arr.iter().all(|entry| {
                                    entry.as_str().map(|s| s == "NaN" || s == "Infinity" || s == "-Infinity").unwrap_or(false)
                                })
                            })
                            .unwrap_or(false)
                    })
                    .collect();
                if number_item.is_some() && non_finite_items.len() == any_of.len() - 1 {
                    if let Some(num) = number_item {
                        let mut rest = schema.clone();
                        rest.remove("any_of");
                        if let serde_json::Value::Object(num_obj) = num {
                            for (k, v) in num_obj {
                                rest.insert(k.clone(), v.clone());
                            }
                        }
                        return normalize_json(&serde_json::Value::Object(rest), &NormalizeOptions { strip_null: false });
                    }
                }

                // Empty struct union -> { type: "object", properties: {} }
                let empties: Vec<&serde_json::Value> = any_of
                    .iter()
                    .filter(|item| {
                        matches!(item, serde_json::Value::Object(m) if m.get("type") == Some(&serde_json::Value::String("object".into())) && !m.contains_key("properties"))
                    })
                    .collect();
                if empties.len() == 2 && any_of.len() == 2 {
                    let mut rest = schema.clone();
                    rest.remove("any_of");
                    rest.insert("type".into(), serde_json::Value::String("object".into()));
                    rest.insert("properties".into(), serde_json::Value::Object(serde_json::Map::new()));
                    return normalize_json(&serde_json::Value::Object(rest), &NormalizeOptions { strip_null: false });
                }

                // Single-element anyOf
                if any_of.len() == 1 {
                    if let Some(single) = any_of.first() {
                        let mut rest = schema.clone();
                        rest.remove("any_of");
                        if let serde_json::Value::Object(single_obj) = single {
                            for (k, v) in single_obj {
                                rest.insert(k.clone(), v.clone());
                            }
                        }
                        return normalize_json(&serde_json::Value::Object(rest), &NormalizeOptions { strip_null: false });
                    }
                }
            }

            // Flatten allOf
            if let Some(all_of) = schema.get("all_of").and_then(|v| v.as_array()) {
                let all_objs: Vec<&serde_json::Map<String, serde_json::Value>> = all_of
                    .iter()
                    .filter_map(|v| v.as_object())
                    .collect();
                if all_objs.len() == all_of.len() && can_flatten_all_of_json(&all_objs, &schema) {
                    schema.remove("all_of");
                    for item_obj in all_objs {
                        for (k, v) in item_obj {
                            schema.insert(k.clone(), v.clone());
                        }
                    }
                    return normalize_json(&serde_json::Value::Object(schema), &NormalizeOptions { strip_null: false });
                }
            }

            // Integer bounds
            if schema.get("type") == Some(&serde_json::Value::String("integer".into())) && !schema.contains_key("maximum") {
                schema.insert("minimum".into(), serde_json::Value::Number(serde_json::Number::MIN));
                schema.insert("maximum".into(), serde_json::Value::Number(serde_json::Number::MAX));
            }

            serde_json::Value::Object(schema)
        }
        other => other.clone(),
    }
}

/// Check whether allOf items can be safely flattened into the parent.
fn can_flatten_all_of_json(
    all_of: &[&serde_json::Map<String, serde_json::Value>],
    parent: &serde_json::Map<String, serde_json::Value>,
) -> bool {
    let mut keys: std::collections::HashSet<&str> = parent
        .keys()
        .filter(|k| *k != "all_of")
        .map(|k| k.as_str())
        .collect();
    all_of.iter().all(|item| {
        item.keys().all(|k| {
            if keys.contains(k.as_str()) {
                return false;
            }
            keys.insert(k.as_str());
            true
        })
    })
}

/// Inline local `$ref` references (`#/$defs/name`) into the schema.
///
/// Ported from `packages/opencode/src/tool/json-schema.ts` lines 121–144.
pub fn inline_local_refs(value: &serde_json::Value) -> serde_json::Value {
    inline_local_refs_inner(value, true, &mut std::collections::HashSet::new())
}

fn inline_local_refs_inner(
    value: &serde_json::Value,
    top_level: bool,
    seen: &mut std::collections::HashSet<String>,
) -> serde_json::Value {
    match value {
        serde_json::Value::Array(arr) => {
            serde_json::Value::Array(arr.iter().map(|v| inline_local_refs_inner(v, false, seen)).collect())
        }
        serde_json::Value::Object(obj) => {
            // Extract $defs from top-level
            let defs: Option<serde_json::Map<String, serde_json::Value>> = if top_level {
                obj.get("$defs").and_then(|v| v.as_object().cloned())
            } else {
                None
            };

            // Handle $ref
            if let Some(ref_str) = obj.get("$ref").and_then(|v| v.as_str()) {
                let name = ref_str
                    .strip_prefix("#/$defs/")
                    .or_else(|| ref_str.strip_prefix("#/definitions/"));
                if let Some(name) = name {
                    if let Some(ref defs_map) = defs {
                        if let Some(target) = defs_map.get(name) {
                            if !seen.contains(name) {
                                seen.insert(name.to_string());
                                let mut rest: serde_json::Map<String, serde_json::Value> = obj
                                    .iter()
                                    .filter(|(k, _)| *k != "$ref")
                                    .map(|(k, v)| (k.clone(), v.clone()))
                                    .collect();
                                if let serde_json::Value::Object(target_obj) = target {
                                    for (k, v) in target_obj {
                                        rest.entry(k.clone()).or_insert_with(|| v.clone());
                                    }
                                }
                                return inline_local_refs_inner(&serde_json::Value::Object(rest), false, seen);
                            }
                        }
                    }
                }
            }

            let mut new_obj = serde_json::Map::new();
            for (key, val) in obj {
                new_obj.insert(key.clone(), inline_local_refs_inner(val, false, seen));
            }
            serde_json::Value::Object(new_obj)
        }
        other => other.clone(),
    }
}

/// Drop `$defs` / `definitions` if all references have been resolved.
///
/// Ported from `packages/opencode/src/tool/json-schema.ts` lines 146–150.
pub fn drop_defs_if_resolved(value: &serde_json::Value) -> serde_json::Value {
    if has_local_ref(value) {
        return value.clone();
    }
    match value {
        serde_json::Value::Object(obj) => {
            let mut new_obj = obj.clone();
            new_obj.remove("$defs");
            new_obj.remove("definitions");
            serde_json::Value::Object(new_obj)
        }
        other => other.clone(),
    }
}

fn has_local_ref(value: &serde_json::Value) -> bool {
    match value {
        serde_json::Value::Array(arr) => arr.iter().any(has_local_ref),
        serde_json::Value::Object(obj) => {
            if let Some(ref_str) = obj.get("$ref").and_then(|v| v.as_str()) {
                if ref_str.starts_with("#/$defs/") || ref_str.starts_with("#/definitions/") {
                    return true;
                }
            }
            obj.values().any(has_local_ref)
        }
        _ => false,
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
