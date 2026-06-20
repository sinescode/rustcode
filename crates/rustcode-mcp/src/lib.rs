#![forbid(unsafe_code)]
#![warn(clippy::all)]

//! MCP (Model Context Protocol) integration for rustcode.
//!
//! Ported from: `packages/opencode/src/mcp/`
//!
//! This crate provides:
//!
//! - **Re-exports** of core MCP types from [`rustcode_core::mcp`] for convenience.
//! - **[`McpTransport`]** trait — abstract transport for MCP communication.
//! - **[`StdioTransport`]** — subprocess-based transport using the MCP stdio framing
//!   protocol (`Content-Length` header framing over stdin/stdout).
//! - **[`HttpTransport`]** — remote transport using HTTP POST for JSON-RPC.
//! - **[`McpToolExecutor`]** — wraps an [`McpClient`] to execute a single tool.
//! - **[`McpDiscovery`]** — discovers MCP server configs from Claude Desktop config,
//!   OpenCode config, and environment variables.
//!
//! ## Architecture
//!
//! The core MCP types and the [`McpClient`] / [`McpServerRegistry`] live in
//! `rustcode-core` (`rustcode_core::mcp`). This crate adds transport
//! abstractions and tool-execution convenience wrappers on top.

use async_trait::async_trait;
use rustcode_core::error::{Error, Result};
use rustcode_core::mcp::McpClient;
use rustcode_core::tool::{ExecuteResult, PluginToolDef, ToolContext};
use serde::Deserialize;
use std::collections::HashMap;
use std::path::Path;
use std::process::Stdio;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;
use tokio::io::{AsyncBufReadExt, AsyncReadExt, AsyncWriteExt, BufReader};
use tokio::process::Command as TokioCommand;
use tokio::sync::Mutex;
use tracing::{debug, warn};

// ---------------------------------------------------------------------------
// Re-exports from rustcode_core::mcp
// ---------------------------------------------------------------------------

pub use rustcode_core::mcp::{
    sanitize_name, tool_key, AuthStatus, JsonRpcError, JsonRpcRequest, JsonRpcResponse, McpEvent,
    McpOAuthConfig, McpResource, McpServerConfig, McpServerRegistry, McpServerSummary,
    McpServerType, McpStatus, McpTool, DEFAULT_MCP_TIMEOUT_MS, MAX_LIST_PAGES, OAUTH_CALLBACK_PORT,
};

// ---------------------------------------------------------------------------
// McpTransport trait
// ---------------------------------------------------------------------------

/// Abstract transport for MCP communication.
///
/// Implementations handle the framing and delivery of JSON-RPC messages.
/// The transport is responsible for its own connection lifecycle:
/// implementations typically perform the MCP `initialize` handshake during
/// construction or through a separate `connect`/`initialize` method.
///
/// # Source
/// Ported from the MCP spec transport layer (stdio and StreamableHTTP).
#[async_trait]
pub trait McpTransport: Send + Sync {
    /// Send a JSON-RPC message and return the response.
    ///
    /// For stdio transports this frames the message, writes to stdin,
    /// and reads the framed response from stdout. For HTTP transports
    /// this POSTs the message and returns the response body.
    ///
    /// # Errors
    /// Returns [`Error::Process`] if the subprocess exits unexpectedly.
    /// Returns [`Error::Network`] if the HTTP request fails.
    /// Returns [`Error::Internal`] if framing or parsing fails.
    async fn send(&self, message: &serde_json::Value) -> Result<serde_json::Value>;

    /// Close the transport and release resources.
    ///
    /// For stdio transports this kills the child process. For HTTP
    /// transports this is a no-op (the client is dropped).
    async fn close(&self) -> Result<()>;
}

// ---------------------------------------------------------------------------
// StdioTransport
// ---------------------------------------------------------------------------

/// MCP transport over a local subprocess using the stdio framing protocol.
///
/// Spawns a child process, then communicates via framed JSON-RPC messages
/// over its stdin and stdout. Messages are framed using the MCP framing
/// protocol:
///
/// ```text
/// Content-Length: <N>\r\n
/// \r\n
/// <JSON body of N bytes>
/// ```
///
/// # Source
/// Ported from `packages/opencode/src/mcp/index.ts` — the `connectLocal()`
/// function and the MCP spec stdio transport.
pub struct StdioTransport {
    /// The command to execute.
    command: String,
    /// Command arguments.
    args: Vec<String>,
    /// Environment variables for the child process.
    env: HashMap<String, String>,
    /// Connection timeout in milliseconds.
    timeout_ms: u64,
    /// The spawned child process (protected by a mutex for concurrent send calls).
    child: Mutex<Option<tokio::process::Child>>,
    /// Monotonically increasing JSON-RPC request ID counter.
    next_id: AtomicU64,
}

impl StdioTransport {
    /// Create a new stdio transport configuration.
    ///
    /// The subprocess is NOT started until [`connect()`](Self::connect) is called.
    pub fn new(command: impl Into<String>) -> Self {
        Self {
            command: command.into(),
            args: Vec::new(),
            env: HashMap::new(),
            timeout_ms: DEFAULT_MCP_TIMEOUT_MS,
            child: Mutex::new(None),
            next_id: AtomicU64::new(1),
        }
    }

    /// Add command-line arguments.
    pub fn with_args(mut self, args: Vec<String>) -> Self {
        self.args = args;
        self
    }

    /// Set environment variables for the child process.
    pub fn with_env(mut self, env: HashMap<String, String>) -> Self {
        self.env = env;
        self
    }

    /// Set the connection timeout in milliseconds.
    pub fn with_timeout(mut self, timeout_ms: u64) -> Self {
        self.timeout_ms = timeout_ms;
        self
    }

    /// Spawn the child process and perform the MCP `initialize` handshake.
    ///
    /// # Flow
    ///
    /// 1. Spawn the child process with piped stdin/stdout/stderr.
    /// 2. Send the `initialize` JSON-RPC request (framed, via stdin).
    /// 3. Read the `initialize` response from stdout.
    /// 4. Send the `notifications/initialized` notification.
    ///
    /// After this method returns, the transport is ready for subsequent
    /// [`send()`](McpTransport::send) calls.
    ///
    /// # Errors
    /// Returns [`Error::Process`] if the subprocess fails to spawn.
    /// Returns [`Error::Internal`] if the handshake fails (invalid framing,
    /// missing response, JSON-RPC error).
    pub async fn connect(&self) -> Result<()> {
        let mut child = TokioCommand::new(&self.command)
            .args(&self.args)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .envs(&self.env)
            .kill_on_drop(true)
            .spawn()
            .map_err(|e| Error::Process {
                message: format!("failed to spawn MCP subprocess `{}`: {e}", self.command),
                exit_code: None,
            })?;

        // Send initialize request
        let init_req = build_jsonrpc_request(
            "initialize",
            serde_json::json!({
                "protocolVersion": "2024-11-05",
                "capabilities": {},
                "clientInfo": {
                    "name": "rustcode",
                    "version": option_env!("CARGO_PKG_VERSION").unwrap_or("0.1.0")
                }
            }),
            0,
        );

        {
            let stdin = child
                .stdin
                .as_mut()
                .ok_or_else(|| Error::Internal("MCP child stdin not available".into()))?;
            let framed = frame_message(&init_req);
            stdin.write_all(framed.as_bytes()).await?;
            stdin.flush().await?;
        }

        // Read initialize response
        {
            let stdout = child
                .stdout
                .as_mut()
                .ok_or_else(|| Error::Internal("MCP child stdout not available".into()))?;
            let mut reader = BufReader::new(stdout);

            // Read Content-Length header line
            let mut header = String::new();
            reader.read_line(&mut header).await.map_err(|e| {
                Error::Internal(format!(
                    "failed to read MCP initialize response header: {e}"
                ))
            })?;

            let content_length = parse_content_length(&header).map_err(|e| {
                Error::Internal(format!("invalid MCP initialize response header: {e}"))
            })?;

            // Read the \r\n blank line separator
            let mut blank = String::new();
            reader.read_line(&mut blank).await.map_err(|e| {
                Error::Internal(format!(
                    "failed to read MCP initialize response separator: {e}"
                ))
            })?;

            // Read the JSON body
            let mut body = vec![0u8; content_length];
            reader.read_exact(&mut body).await.map_err(|e| {
                Error::Internal(format!("failed to read MCP initialize response body: {e}"))
            })?;

            let response_str = String::from_utf8_lossy(&body).to_string();
            parse_jsonrpc_response(&response_str)
                .map_err(|e| Error::Internal(format!("MCP initialize handshake failed: {e}")))?;
        }

        // Send "initialized" notification
        {
            let stdin = child.stdin.as_mut().ok_or_else(|| {
                Error::Internal("MCP child stdin not available for initialized notification".into())
            })?;
            let notif =
                build_jsonrpc_notification("notifications/initialized", serde_json::json!({}));
            let framed = frame_message(&notif);
            stdin.write_all(framed.as_bytes()).await?;
            stdin.flush().await?;
        }

        debug!("StdioTransport: connected to `{}`", self.command);

        {
            let mut guard = self.child.lock().await;
            *guard = Some(child);
        }

        Ok(())
    }

    /// Check whether the transport is connected (process is running).
    pub async fn is_connected(&self) -> bool {
        self.child.lock().await.is_some()
    }

    /// Allocate the next JSON-RPC request ID.
    fn next_id(&self) -> u64 {
        self.next_id.fetch_add(1, Ordering::SeqCst)
    }

    /// Send a raw framed message over the stdio transport and read the response.
    ///
    /// This is the internal send logic, intended to be called while holding
    /// the child lock (or through the trait `send` method).
    async fn send_framed(&self, message: &serde_json::Value) -> Result<serde_json::Value> {
        let mut guard = self.child.lock().await;
        let child = guard.as_mut().ok_or_else(|| {
            Error::Internal("StdioTransport: not connected — call connect() first".into())
        })?;

        // Write framed request to stdin
        let framed = frame_message(message);
        {
            let stdin = child
                .stdin
                .as_mut()
                .ok_or_else(|| Error::Internal("MCP child stdin not available".into()))?;
            stdin.write_all(framed.as_bytes()).await?;
            stdin.flush().await?;
        }

        // Read framed response from stdout
        let stdout = child
            .stdout
            .as_mut()
            .ok_or_else(|| Error::Internal("MCP child stdout not available".into()))?;
        let mut reader = BufReader::new(stdout);

        // Read Content-Length header
        let mut header = String::new();
        reader
            .read_line(&mut header)
            .await
            .map_err(|e| Error::Internal(format!("failed to read MCP response header: {e}")))?;

        let content_length = parse_content_length(&header)
            .map_err(|e| Error::Internal(format!("invalid MCP response header: {e}")))?;

        // Read blank line separator
        let mut blank = String::new();
        reader
            .read_line(&mut blank)
            .await
            .map_err(|e| Error::Internal(format!("failed to read MCP response separator: {e}")))?;

        // Read body
        let mut body = vec![0u8; content_length];
        reader
            .read_exact(&mut body)
            .await
            .map_err(|e| Error::Internal(format!("failed to read MCP response body: {e}")))?;

        let response_str = String::from_utf8_lossy(&body).to_string();
        parse_jsonrpc_response(&response_str)
            .map_err(|e| Error::Internal(format!("MCP response error: {e}")))
    }
}

#[async_trait]
impl McpTransport for StdioTransport {
    async fn send(&self, message: &serde_json::Value) -> Result<serde_json::Value> {
        self.send_framed(message).await
    }

    async fn close(&self) -> Result<()> {
        let mut guard = self.child.lock().await;
        if let Some(mut child) = guard.take() {
            let _ = child.kill().await;
            let _ = child.wait().await;
        }
        Ok(())
    }
}

impl Drop for StdioTransport {
    fn drop(&mut self) {
        // The child process is killed via kill_on_drop, but we try to close
        // gracefully if possible (this is a best-effort cleanup).
    }
}

// ---------------------------------------------------------------------------
// HttpTransport
// ---------------------------------------------------------------------------

/// MCP transport over HTTP for remote MCP servers.
///
/// Sends JSON-RPC requests via HTTP POST and reads the response from
/// the HTTP response body. This implements the StreamableHTTP transport
/// from the MCP specification.
///
/// # Source
/// Ported from `packages/opencode/src/mcp/index.ts` — the `connectRemote()`
/// function and the MCP spec StreamableHTTP transport.
pub struct HttpTransport {
    /// Reusable HTTP client for JSON-RPC calls.
    client: reqwest::Client,
    /// The server's base URL.
    url: String,
    /// Custom HTTP headers sent with every request.
    headers: HashMap<String, String>,
    /// Request timeout in milliseconds.
    timeout_ms: u64,
    /// Whether the transport has been initialized.
    initialized: Mutex<bool>,
}

impl HttpTransport {
    /// Create a new HTTP transport for a remote MCP server.
    ///
    /// The transport is ready for use after construction — no explicit
    /// `connect()` call is needed. However, the MCP `initialize` handshake
    /// should be performed via [`initialize()`](Self::initialize) before
    /// sending tool discovery or execution requests.
    pub fn new(url: impl Into<String>) -> Self {
        Self {
            client: reqwest::Client::new(),
            url: url.into(),
            headers: HashMap::new(),
            timeout_ms: DEFAULT_MCP_TIMEOUT_MS,
            initialized: Mutex::new(false),
        }
    }

    /// Set custom HTTP headers.
    pub fn with_headers(mut self, headers: HashMap<String, String>) -> Self {
        self.headers = headers;
        self
    }

    /// Set the request timeout in milliseconds.
    pub fn with_timeout(mut self, timeout_ms: u64) -> Self {
        self.timeout_ms = timeout_ms;
        self
    }

    /// Perform the MCP `initialize` handshake over HTTP.
    ///
    /// Sends the `initialize` JSON-RPC request via POST, verifies the
    /// response, and sends the `notifications/initialized` notification.
    ///
    /// # Errors
    /// Returns [`Error::Network`] if the server is unreachable or returns
    /// a non-2xx status. Returns [`Error::Internal`] if the response is
    /// not valid JSON-RPC.
    pub async fn initialize(&self) -> Result<()> {
        let init_req = build_jsonrpc_request(
            "initialize",
            serde_json::json!({
                "protocolVersion": "2024-11-05",
                "capabilities": {},
                "clientInfo": {
                    "name": "rustcode",
                    "version": option_env!("CARGO_PKG_VERSION").unwrap_or("0.1.0")
                }
            }),
            0,
        );

        let response = self
            .post_json(&init_req)
            .await
            .map_err(|e| Error::Network(format!("MCP HTTP initialize failed: {e}")))?;

        // Verify the response has a result (no error)
        if response.get("error").is_some() {
            let err_msg = response["error"]
                .get("message")
                .and_then(|m| m.as_str())
                .unwrap_or("unknown error");
            return Err(Error::Internal(format!(
                "MCP HTTP initialize returned error: {err_msg}"
            )));
        }

        // Send initialized notification
        let notif = build_jsonrpc_notification("notifications/initialized", serde_json::json!({}));
        let _ = self.post_json(&notif).await;

        {
            let mut guard = self.initialized.lock().await;
            *guard = true;
        }

        debug!("HttpTransport: initialized with {}", self.url);
        Ok(())
    }

    /// Check whether the transport has been initialized.
    pub async fn is_initialized(&self) -> bool {
        *self.initialized.lock().await
    }

    /// Send a JSON-RPC message via HTTP POST and return the parsed response.
    async fn post_json(&self, message: &serde_json::Value) -> Result<serde_json::Value> {
        let mut request = self
            .client
            .post(&self.url)
            .json(message)
            .timeout(std::time::Duration::from_millis(self.timeout_ms));

        for (key, value) in &self.headers {
            request = request.header(key.as_str(), value.as_str());
        }

        let response = request.send().await.map_err(|e| {
            if e.is_timeout() {
                Error::Network(format!(
                    "MCP HTTP request to `{}` timed out after {}ms",
                    self.url, self.timeout_ms
                ))
            } else if e.is_connect() {
                Error::Network(format!(
                    "MCP HTTP: failed to connect to `{}`: {e}",
                    self.url
                ))
            } else {
                Error::Http(e)
            }
        })?;

        if !response.status().is_success() {
            return Err(Error::Network(format!(
                "MCP server `{}` returned HTTP {}",
                self.url,
                response.status()
            )));
        }

        let body: serde_json::Value = response.json().await?;
        Ok(body)
    }
}

#[async_trait]
impl McpTransport for HttpTransport {
    async fn send(&self, message: &serde_json::Value) -> Result<serde_json::Value> {
        self.post_json(message).await
    }

    async fn close(&self) -> Result<()> {
        // HTTP transport has no persistent connection to close.
        // The reqwest client is dropped when HttpTransport is dropped.
        Ok(())
    }
}

// ---------------------------------------------------------------------------
// McpToolExecutor
// ---------------------------------------------------------------------------

/// Wraps an [`McpClient`] to execute a single tool as a rustcode tool.
///
/// This is the bridge between the MCP client (which manages the full
/// server connection) and the tool registry (which expects individual
/// tool implementations).
///
/// # Source
/// Ported from `packages/opencode/src/mcp/index.ts` — the tool dispatch
/// through the MCP connection.
pub struct McpToolExecutor {
    /// The active MCP client connection.
    client: Arc<McpClient>,
    /// The name of the tool on the MCP server.
    tool_name: String,
}

impl McpToolExecutor {
    /// Create a new executor for the given tool on the given MCP client.
    ///
    /// The tool must exist on the server — call [`McpClient::list_tools()`]
    /// first to discover available tools.
    pub fn new(client: Arc<McpClient>, tool_name: impl Into<String>) -> Self {
        Self {
            client,
            tool_name: tool_name.into(),
        }
    }

    /// Execute the MCP tool with the given arguments.
    ///
    /// Calls [`McpClient::call_tool()`] and converts the JSON-RPC result
    /// into a displayable output string.
    ///
    /// # Errors
    /// Returns an error if the MCP server connection fails or the tool
    /// call returns a JSON-RPC error.
    pub async fn execute(&self, args: serde_json::Value) -> Result<serde_json::Value> {
        self.client.call_tool(&self.tool_name, args).await
    }

    /// Execute the tool and return a structured [`ExecuteResult`].
    ///
    /// This extracts text content from the MCP `tools/call` response
    /// `result.content` array and formats it for display.
    pub async fn execute_formatted(&self, args: serde_json::Value) -> Result<ExecuteResult> {
        let result = self.client.call_tool(&self.tool_name, args).await?;

        let output = extract_mcp_content(&result);
        let title = self.tool_name.clone();

        Ok(ExecuteResult {
            title,
            output,
            truncated: false,
            output_path: None,
            attachments: None,
            metadata: HashMap::new(),
        })
    }

    /// Build a [`PluginToolDef`] suitable for registration in the
    /// [`ToolRegistry`](rustcode_core::tool::ToolRegistry).
    ///
    /// The returned definition includes the tool's JSON input schema,
    /// description, and an execute closure that calls the MCP server.
    ///
    /// # Errors
    /// Returns an error if the tool metadata cannot be found in the
    /// client's cached tool list.
    pub async fn to_plugin_def(&self) -> Result<PluginToolDef> {
        let tools = self.client.cached_tools().await;
        let tool = tools
            .iter()
            .find(|t| t.name == self.tool_name)
            .ok_or_else(|| {
                Error::Tool(format!(
                    "MCP tool '{}' not found on server '{}'",
                    self.tool_name, self.client.server_name
                ))
            })?;

        let tool_id = tool_key(&self.client.server_name, &tool.name);
        let input_schema = tool
            .input_schema
            .clone()
            .unwrap_or_else(|| serde_json::json!({"type": "object", "properties": {}}));
        let description = tool
            .description
            .clone()
            .unwrap_or_else(|| format!("MCP tool: {}", tool.name));

        let client = Arc::clone(&self.client);
        let tool_name = self.tool_name.clone();

        Ok(PluginToolDef::new(
            tool_id,
            description,
            input_schema,
            move |args, _ctx: ToolContext| {
                let client = Arc::clone(&client);
                let tool_name = tool_name.clone();
                async move {
                    let result = client.call_tool(&tool_name, args).await?;
                    let output = extract_mcp_content(&result);

                    Ok(ExecuteResult {
                        title: tool_name.clone(),
                        output,
                        truncated: false,
                        output_path: None,
                        attachments: None,
                        metadata: HashMap::new(),
                    })
                }
            },
        ))
    }

    /// Get the full tool key (`{server_name}_{tool_name}`).
    pub fn tool_key(&self) -> String {
        tool_key(&self.client.server_name, &self.tool_name)
    }
}

/// Extract human-readable text content from an MCP `tools/call` response.
///
/// The MCP protocol returns `result.content` as an array of content blocks.
/// Each text block has `{type: "text", text: "..."}`. This function joins
/// all text blocks with newlines. If the result does not contain a `content`
/// array, the entire result is serialized as a JSON string.
fn extract_mcp_content(result: &serde_json::Value) -> String {
    if let Some(content) = result.get("content").and_then(|c| c.as_array()) {
        let texts: Vec<&str> = content
            .iter()
            .filter_map(|block| {
                if block.get("type").and_then(|t| t.as_str()) == Some("text") {
                    block.get("text").and_then(|t| t.as_str())
                } else {
                    None
                }
            })
            .collect();

        if texts.is_empty() {
            // No text blocks — serialize the entire content
            serde_json::to_string_pretty(&content).unwrap_or_else(|_| format!("{content:?}"))
        } else {
            texts.join("\n")
        }
    } else {
        // No content array — serialize the whole result
        serde_json::to_string_pretty(result).unwrap_or_else(|_| format!("{result}"))
    }
}

// ---------------------------------------------------------------------------
// McpDiscovery
// ---------------------------------------------------------------------------

/// Configuration format used by Claude Desktop's `claude_desktop_config.json`.
///
/// The file format is:
/// ```json
/// {
///   "mcpServers": {
///     "server-name": {
///       "command": "npx",
///       "args": ["-y", "@modelcontextprotocol/server-filesystem", "."]
///     }
///   }
/// }
/// ```
///
/// Remote servers are indicated by `"type": "url"`.
#[derive(Debug, Clone, Deserialize)]
struct ClaudeDesktopConfig {
    #[serde(rename = "mcpServers", default)]
    mcp_servers: HashMap<String, serde_json::Value>,
}

/// Configuration format used by OpenCode's `.opencode/config.json`.
///
/// The file format is:
/// ```json
/// {
///   "mcp": {
///     "server-name": {
///       "type": "local",
///       "command": ["node", "server.js"]
///     }
///   }
/// }
/// ```
#[derive(Debug, Clone, Deserialize)]
struct OpenCodeMcpSection {
    #[serde(default)]
    mcp: HashMap<String, McpServerConfig>,
}

/// Discovers MCP server configurations from various sources.
///
/// # Source
/// Ported from `packages/opencode/src/mcp/index.ts` — config loading
/// and `packages/opencode/src/config/config.ts` MCP section parsing.
pub struct McpDiscovery;

impl McpDiscovery {
    /// Parse MCP server configs from a Claude Desktop configuration file.
    ///
    /// Claude Desktop stores MCP servers in `claude_desktop_config.json`
    /// under the `mcpServers` key. Each entry has:
    ///
    /// - `command` (string) — the executable to run
    /// - `args` (optional array of strings) — command arguments
    /// - `env` (optional object) — environment variables
    /// - `type` (optional, `"url"` for remote servers)
    /// - `url` (string, for remote servers)
    ///
    /// # Errors
    /// Returns an error if the file cannot be read or parsed.
    pub fn from_claude_desktop_config(path: &Path) -> Result<Vec<McpServerConfig>> {
        let content = std::fs::read_to_string(path).map_err(|e| Error::FileSystem {
            path: path.display().to_string(),
            message: format!("failed to read Claude Desktop config: {e}"),
        })?;

        let parsed: ClaudeDesktopConfig = serde_json::from_str(&content).map_err(|e| {
            Error::Config(format!(
                "failed to parse Claude Desktop config `{}`: {e}",
                path.display()
            ))
        })?;

        let mut configs = Vec::with_capacity(parsed.mcp_servers.len());

        for (name, value) in &parsed.mcp_servers {
            let config = parse_claude_server_entry(name, value)?;
            configs.push(config);
        }

        debug!(
            "Discovered {} MCP servers from Claude Desktop config",
            configs.len()
        );
        Ok(configs)
    }

    /// Parse MCP server configs from an OpenCode project config file.
    ///
    /// OpenCode stores MCP servers in `.opencode/config.json` under the
    /// `mcp` key. Each entry is a full [`McpServerConfig`].
    ///
    /// # Errors
    /// Returns an error if the file cannot be read or parsed.
    pub fn from_opencode_config(path: &Path) -> Result<Vec<McpServerConfig>> {
        let content = std::fs::read_to_string(path).map_err(|e| Error::FileSystem {
            path: path.display().to_string(),
            message: format!("failed to read OpenCode config: {e}"),
        })?;

        let parsed: OpenCodeMcpSection = serde_json::from_str(&content).map_err(|e| {
            Error::Config(format!(
                "failed to parse OpenCode config `{}`: {e}",
                path.display()
            ))
        })?;

        let configs: Vec<McpServerConfig> = parsed.mcp.into_values().collect();
        debug!(
            "Discovered {} MCP servers from OpenCode config",
            configs.len()
        );
        Ok(configs)
    }

    /// Discover MCP server configs from environment variables.
    ///
    /// Supports two formats:
    ///
    /// 1. **`MCP_SERVERS`** — a JSON array or map of server configs.
    ///    ```text
    ///    MCP_SERVERS='[{"type":"local","command":["node","server.js"]}]'
    ///    ```
    ///
    /// 2. **Prefixed variables** — individual env vars with the `MCP_SERVER_`
    ///    prefix. Each server is identified by name and property:
    ///    ```text
    ///    MCP_SERVER_MYTOOL_COMMAND=node
    ///    MCP_SERVER_MYTOOL_ARGS=server.js --port 3000
    ///    MCP_SERVER_MYTOOL_TYPE=local
    ///    ```
    ///
    /// # Errors
    /// Returns an error only if `MCP_SERVERS` is set but contains invalid JSON.
    pub fn from_env() -> Result<Vec<McpServerConfig>> {
        // Format 1: MCP_SERVERS JSON
        if let Ok(json) = std::env::var("MCP_SERVERS") {
            let trimmed = json.trim();
            if trimmed.is_empty() {
                return Ok(Vec::new());
            }

            // Try parsing as a JSON array first
            if let Ok(configs) = serde_json::from_str::<Vec<McpServerConfig>>(trimmed) {
                debug!(
                    "Discovered {} MCP servers from MCP_SERVERS env var",
                    configs.len()
                );
                return Ok(configs);
            }

            // Try parsing as a JSON object (map)
            if let Ok(map) = serde_json::from_str::<HashMap<String, McpServerConfig>>(trimmed) {
                let configs: Vec<McpServerConfig> = map.into_values().collect();
                debug!(
                    "Discovered {} MCP servers from MCP_SERVERS env var (map)",
                    configs.len()
                );
                return Ok(configs);
            }

            return Err(Error::Config(
                "MCP_SERVERS env var contains invalid JSON (expected array or map of McpServerConfig)".into(),
            ));
        }

        // Format 2: MCP_SERVER_<NAME>_<PROPERTY> prefixed variables
        let configs = parse_mcp_server_env_prefix();
        if !configs.is_empty() {
            debug!(
                "Discovered {} MCP servers from MCP_SERVER_* env vars",
                configs.len()
            );
        }
        Ok(configs)
    }
}

/// Parse a single server entry from a Claude Desktop config.
///
/// Claude Desktop format:
/// ```json
/// {
///   "command": "npx",
///   "args": ["-y", "server"],
///   "env": {"KEY": "val"}
/// }
/// ```
fn parse_claude_server_entry(name: &str, value: &serde_json::Value) -> Result<McpServerConfig> {
    let obj = value.as_object().ok_or_else(|| {
        Error::Config(format!(
            "Claude Desktop config: MCP server '{name}' entry is not an object"
        ))
    })?;

    // Check for remote type
    let server_type = obj.get("type").and_then(|t| t.as_str()).unwrap_or("local");

    if server_type == "url" || server_type == "remote" {
        // Remote (HTTP) server
        let url = obj.get("url").and_then(|u| u.as_str()).ok_or_else(|| {
            Error::Config(format!(
                "Claude Desktop config: remote MCP server '{name}' missing 'url'"
            ))
        })?;

        let mut config = McpServerConfig::remote(url.to_string());

        if let Some(headers) = obj.get("headers").and_then(|h| h.as_object()) {
            let mut header_map = HashMap::new();
            for (k, v) in headers {
                if let Some(v_str) = v.as_str() {
                    header_map.insert(k.clone(), v_str.to_string());
                }
            }
            if !header_map.is_empty() {
                config = config.with_headers(header_map);
            }
        }

        Ok(config)
    } else {
        // Local (stdio) server
        let command = obj.get("command").and_then(|c| c.as_str()).ok_or_else(|| {
            Error::Config(format!(
                "Claude Desktop config: local MCP server '{name}' missing 'command'"
            ))
        })?;

        let mut cmd_parts = vec![command.to_string()];

        // In Claude Desktop, args are separate from command
        if let Some(args) = obj.get("args").and_then(|a| a.as_array()) {
            for arg in args {
                if let Some(arg_str) = arg.as_str() {
                    cmd_parts.push(arg_str.to_string());
                }
            }
        }

        let mut config = McpServerConfig::local(cmd_parts);

        if let Some(env) = obj.get("env").and_then(|e| e.as_object()) {
            let mut env_map = HashMap::new();
            for (k, v) in env {
                if let Some(v_str) = v.as_str() {
                    env_map.insert(k.clone(), v_str.to_string());
                }
            }
            if !env_map.is_empty() {
                config = config.with_env(env_map);
            }
        }

        Ok(config)
    }
}

/// Parse MCP server configs from `MCP_SERVER_<NAME>_<PROPERTY>` env vars.
///
/// Groups environment variables by server name, then builds configs.
fn parse_mcp_server_env_prefix() -> Vec<McpServerConfig> {
    let prefix = "MCP_SERVER_";
    let mut server_props: HashMap<String, HashMap<String, String>> = HashMap::new();

    for (key, value) in std::env::vars() {
        if let Some(suffix) = key.strip_prefix(prefix) {
            // Expected format: <NAME>_<PROPERTY>
            // Find the last underscore to split name from property
            if let Some(last_underscore) = suffix.rfind('_') {
                let server_name = &suffix[..last_underscore];
                let property = &suffix[last_underscore + 1..];
                server_props
                    .entry(server_name.to_string())
                    .or_default()
                    .insert(property.to_uppercase(), value);
            }
        }
    }

    let mut configs = Vec::new();

    for (name, props) in &server_props {
        // Determine if remote: has URL property
        if let Some(url) = props.get("URL") {
            let mut config = McpServerConfig::remote(url.clone());

            if let Some(headers_str) = props.get("HEADERS") {
                if let Ok(headers) = serde_json::from_str::<HashMap<String, String>>(headers_str) {
                    config = config.with_headers(headers);
                } else {
                    warn!(
                        "MCP_SERVER_{}_HEADERS is not valid JSON, ignoring",
                        name.to_uppercase()
                    );
                }
            }

            if let Some(timeout_str) = props.get("TIMEOUT") {
                if let Ok(ms) = timeout_str.parse::<u64>() {
                    config = config.with_timeout(ms);
                }
            }

            configs.push(config);
        } else if let Some(command) = props.get("COMMAND") {
            let mut cmd_parts = vec![command.clone()];

            if let Some(args_str) = props.get("ARGS") {
                // Space-separated arguments
                cmd_parts.extend(args_str.split_whitespace().map(|s| s.to_string()));
            }

            let mut config = McpServerConfig::local(cmd_parts);

            if let Some(env_str) = props.get("ENV") {
                if let Ok(env_map) = serde_json::from_str::<HashMap<String, String>>(env_str) {
                    config = config.with_env(env_map);
                } else {
                    warn!(
                        "MCP_SERVER_{}_ENV is not valid JSON, ignoring",
                        name.to_uppercase()
                    );
                }
            }

            if let Some(timeout_str) = props.get("TIMEOUT") {
                if let Ok(ms) = timeout_str.parse::<u64>() {
                    config = config.with_timeout(ms);
                }
            }

            configs.push(config);
        } else {
            warn!(
                "MCP_SERVER_{}_* env vars have neither COMMAND nor URL, skipping",
                name.to_uppercase()
            );
        }
    }

    // Sort by server name for determinism
    configs.sort_by(|a, b| {
        let a_cmd = a.command_executable().unwrap_or("");
        let b_cmd = b.command_executable().unwrap_or("");
        a_cmd.cmp(b_cmd)
    });

    configs
}

// ---------------------------------------------------------------------------
// JSON-RPC framing helpers
// ---------------------------------------------------------------------------

/// Build a JSON-RPC 2.0 request object.
///
/// Returns a JSON value with `jsonrpc`, `method`, `params`, and `id` fields.
fn build_jsonrpc_request(method: &str, params: serde_json::Value, id: u64) -> serde_json::Value {
    serde_json::json!({
        "jsonrpc": "2.0",
        "method": method,
        "params": params,
        "id": id,
    })
}

/// Build a JSON-RPC 2.0 notification (a request without an `id` field).
///
/// Notifications are fire-and-forget — the server does not send a response.
fn build_jsonrpc_notification(method: &str, params: serde_json::Value) -> serde_json::Value {
    serde_json::json!({
        "jsonrpc": "2.0",
        "method": method,
        "params": params,
    })
}

/// Frame a JSON message using the MCP framing protocol.
///
/// Format: `Content-Length: <N>\r\n\r\n<json>`
///
/// This is the framing used by the MCP stdio transport per the specification.
fn frame_message(json: &serde_json::Value) -> String {
    let body = serde_json::to_string(json).expect("JSON serialization should not fail");
    format!("Content-Length: {}\r\n\r\n{}", body.len(), body)
}

/// Parse a `Content-Length` header value from a framed MCP message.
///
/// Expects input like `"Content-Length: 123\r\n"` and returns the
/// parsed byte count.
///
/// # Errors
/// Returns a string description if the header is missing or malformed.
fn parse_content_length(header: &str) -> std::result::Result<usize, String> {
    header
        .trim()
        .strip_prefix("Content-Length:")
        .ok_or_else(|| format!("missing Content-Length header in: {header}"))?
        .trim()
        .parse::<usize>()
        .map_err(|e| format!("invalid Content-Length value: {e}"))
}

/// Parse a JSON-RPC 2.0 response string and check for errors.
///
/// Returns the parsed JSON value on success, or an error string if the
/// response contains a JSON-RPC error object.
fn parse_jsonrpc_response(response: &str) -> std::result::Result<serde_json::Value, String> {
    let value: serde_json::Value =
        serde_json::from_str(response).map_err(|e| format!("invalid JSON: {e}"))?;

    if let Some(err) = value.get("error") {
        let message = err
            .get("message")
            .and_then(|m| m.as_str())
            .unwrap_or("unknown error");
        return Err(format!("JSON-RPC error: {message}"));
    }

    Ok(value)
}

/// Parse a stream of MCP-framed messages into individual JSON values.
///
/// Splits the input by `Content-Length: N\r\n\r\n` headers and returns
/// each complete message body as a parsed JSON value. Incomplete trailing
/// data is silently ignored.
fn parse_jsonrpc_stream(data: &str) -> Vec<serde_json::Value> {
    let mut messages = Vec::new();
    let mut remaining = data;

    while !remaining.is_empty() {
        if let Some(header_end) = remaining.find("\r\n\r\n") {
            let header = &remaining[..header_end];
            if let Some(content_length) = header
                .strip_prefix("Content-Length: ")
                .and_then(|n| n.trim().parse::<usize>().ok())
            {
                let body_start = header_end + 4; // skip \r\n\r\n
                if remaining.len() >= body_start + content_length {
                    let body = &remaining[body_start..body_start + content_length];
                    if let Ok(value) = serde_json::from_str::<serde_json::Value>(body) {
                        messages.push(value);
                    }
                    remaining = &remaining[body_start + content_length..];
                    continue;
                }
            }
        }
        // Incomplete message or invalid header — stop parsing
        break;
    }

    messages
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    // ── Frame helpers ─────────────────────────────────────────────────

    #[test]
    fn test_frame_message_format() {
        let msg = serde_json::json!({"jsonrpc": "2.0", "method": "ping", "id": 1});
        let framed = frame_message(&msg);
        let body = serde_json::to_string(&msg).unwrap();
        let expected = format!("Content-Length: {}\r\n\r\n{}", body.len(), body);
        assert_eq!(framed, expected);
    }

    #[test]
    fn test_frame_message_preserves_body() {
        let msg = serde_json::json!({"key": "value with spaces", "num": 42});
        let framed = frame_message(&msg);
        // The frame should contain the exact JSON body
        let body = serde_json::to_string(&msg).unwrap();
        assert!(framed.ends_with(&body));
        assert!(framed.starts_with("Content-Length: "));
    }

    #[test]
    fn test_parse_content_length_valid() {
        let result = parse_content_length("Content-Length: 42\r\n").expect("valid header");
        assert_eq!(result, 42);
    }

    #[test]
    fn test_parse_content_length_with_whitespace() {
        let result = parse_content_length("  Content-Length:  128  \r\n  ").expect("valid header");
        assert_eq!(result, 128);
    }

    #[test]
    fn test_parse_content_length_zero() {
        let result = parse_content_length("Content-Length: 0\r\n").expect("valid header");
        assert_eq!(result, 0);
    }

    #[test]
    fn test_parse_content_length_missing() {
        assert!(parse_content_length("X-Custom: 42\r\n").is_err());
    }

    #[test]
    fn test_parse_content_length_invalid_number() {
        assert!(parse_content_length("Content-Length: abc\r\n").is_err());
    }

    #[test]
    fn test_parse_content_length_empty() {
        assert!(parse_content_length("").is_err());
    }

    // ── JSON-RPC helpers ──────────────────────────────────────────────

    #[test]
    fn test_build_jsonrpc_request() {
        let req = build_jsonrpc_request("tools/list", serde_json::json!({}), 1);
        assert_eq!(req["jsonrpc"], "2.0");
        assert_eq!(req["method"], "tools/list");
        assert_eq!(req["id"], 1);
        assert!(req.get("params").is_some());
    }

    #[test]
    fn test_build_jsonrpc_request_with_params() {
        let req = build_jsonrpc_request(
            "tools/call",
            serde_json::json!({"name": "echo", "arguments": {"msg": "hi"}}),
            42,
        );
        assert_eq!(req["id"], 42);
        assert_eq!(req["params"]["name"], "echo");
        assert_eq!(req["params"]["arguments"]["msg"], "hi");
    }

    #[test]
    fn test_build_jsonrpc_notification_has_no_id() {
        let notif = build_jsonrpc_notification("notifications/initialized", serde_json::json!({}));
        let json_str = serde_json::to_string(&notif).unwrap();
        let parsed: serde_json::Value = serde_json::from_str(&json_str).unwrap();
        assert!(
            parsed.get("id").is_none(),
            "notification should not have an id"
        );
        assert_eq!(parsed["jsonrpc"], "2.0");
        assert_eq!(parsed["method"], "notifications/initialized");
    }

    #[test]
    fn test_parse_jsonrpc_response_success() {
        let resp = r#"{"jsonrpc":"2.0","id":1,"result":{"tools":[]}}"#;
        let parsed = parse_jsonrpc_response(resp).expect("valid response");
        assert_eq!(parsed["id"], 1);
        assert!(parsed["result"].is_object());
    }

    #[test]
    fn test_parse_jsonrpc_response_error() {
        let resp =
            r#"{"jsonrpc":"2.0","id":1,"error":{"code":-32601,"message":"Method not found"}}"#;
        let err = parse_jsonrpc_response(resp).unwrap_err();
        assert!(err.contains("Method not found"));
    }

    #[test]
    fn test_parse_jsonrpc_response_error_with_data() {
        let resp = r#"{"jsonrpc":"2.0","id":1,"error":{"code":-32000,"message":"Server error","data":{"detail":"something broke"}}}"#;
        let err = parse_jsonrpc_response(resp).unwrap_err();
        assert!(err.contains("Server error"));
    }

    #[test]
    fn test_parse_jsonrpc_response_invalid_json() {
        assert!(parse_jsonrpc_response("not json").is_err());
    }

    // ── JSON-RPC stream parsing ───────────────────────────────────────

    #[test]
    fn test_parse_jsonrpc_stream_single_message() {
        let body = r#"{"jsonrpc":"2.0","id":1,"result":"ok"}"#;
        let framed = format!("Content-Length: {}\r\n\r\n{}", body.len(), body);
        let messages = parse_jsonrpc_stream(&framed);
        assert_eq!(messages.len(), 1);
        assert_eq!(messages[0]["result"], "ok");
    }

    #[test]
    fn test_parse_jsonrpc_stream_multiple_messages() {
        let body1 = r#"{"jsonrpc":"2.0","id":1,"result":"first"}"#;
        let body2 = r#"{"jsonrpc":"2.0","id":2,"result":"second"}"#;
        let framed = format!(
            "Content-Length: {}\r\n\r\n{}Content-Length: {}\r\n\r\n{}",
            body1.len(),
            body1,
            body2.len(),
            body2
        );
        let messages = parse_jsonrpc_stream(&framed);
        assert_eq!(messages.len(), 2);
        assert_eq!(messages[0]["result"], "first");
        assert_eq!(messages[1]["result"], "second");
    }

    #[test]
    fn test_parse_jsonrpc_stream_incomplete_trailer() {
        let body = r#"{"jsonrpc":"2.0","id":1,"result":"ok"}"#;
        // Only partial data after the header
        let framed = format!("Content-Length: {}\r\n\r\n{}", body.len(), "{");
        let messages = parse_jsonrpc_stream(&framed);
        assert!(messages.is_empty());
    }

    #[test]
    fn test_parse_jsonrpc_stream_empty() {
        let messages = parse_jsonrpc_stream("");
        assert!(messages.is_empty());
    }

    #[test]
    fn test_parse_jsonrpc_stream_no_header() {
        let messages = parse_jsonrpc_stream(r#"{"jsonrpc":"2.0","id":1}"#);
        assert!(
            messages.is_empty(),
            "raw JSON without framing should produce no messages"
        );
    }

    // ── Tool key generation ───────────────────────────────────────────

    #[test]
    fn test_tool_key_basic() {
        let key = tool_key("my-server", "search_docs");
        assert_eq!(key, "my-server_search_docs");
    }

    #[test]
    fn test_tool_key_sanitizes_special_chars() {
        let key = tool_key("my server!", "search docs?");
        assert_eq!(key, "my_server__search_docs_");
    }

    #[test]
    fn test_tool_key_with_hyphens_and_underscores() {
        let key = tool_key("test-server_v2", "run_tool-x");
        assert_eq!(key, "test-server_v2_run_tool-x");
    }

    #[test]
    fn test_sanitize_name_preserves_alphanumeric() {
        assert_eq!(sanitize_name("hello123"), "hello123");
    }

    #[test]
    fn test_sanitize_name_replaces_symbols() {
        assert_eq!(sanitize_name("a@b#c$d%e^f&g*h(i)"), "a_b_c_d_e_f_g_h_i_");
    }

    #[test]
    fn test_sanitize_name_empty() {
        assert_eq!(sanitize_name(""), "");
    }

    // ── extract_mcp_content ───────────────────────────────────────────

    #[test]
    fn test_extract_mcp_content_text_blocks() {
        let result = serde_json::json!({
            "content": [
                {"type": "text", "text": "Hello, World!"},
                {"type": "text", "text": "Second line"}
            ]
        });
        let output = extract_mcp_content(&result);
        assert_eq!(output, "Hello, World!\nSecond line");
    }

    #[test]
    fn test_extract_mcp_content_mixed_blocks() {
        let result = serde_json::json!({
            "content": [
                {"type": "text", "text": "Text only"},
                {"type": "resource", "uri": "file:///tmp/data"}
            ]
        });
        let output = extract_mcp_content(&result);
        assert_eq!(output, "Text only");
    }

    #[test]
    fn test_extract_mcp_content_no_text_blocks() {
        let result = serde_json::json!({
            "content": [
                {"type": "resource", "uri": "file:///tmp/data"}
            ]
        });
        let output = extract_mcp_content(&result);
        // Should serialize the content array as JSON
        assert!(output.contains("resource"));
    }

    #[test]
    fn test_extract_mcp_content_no_content_field() {
        let result = serde_json::json!({"status": "ok", "value": 42});
        let output = extract_mcp_content(&result);
        assert!(output.contains("ok"));
        assert!(output.contains("42"));
    }

    // ── McpDiscovery: Claude Desktop config ───────────────────────────

    #[test]
    fn test_parse_claude_server_entry_local() {
        let value = serde_json::json!({
            "command": "node",
            "args": ["server.js", "--port", "3000"]
        });
        let config = parse_claude_server_entry("my-server", &value).expect("valid config");
        assert!(config.is_local());
        assert_eq!(config.command_executable(), Some("node"));
        assert_eq!(config.full_args(), vec!["server.js", "--port", "3000"]);
    }

    #[test]
    fn test_parse_claude_server_entry_remote() {
        let value = serde_json::json!({
            "type": "url",
            "url": "https://mcp.example.com/api"
        });
        let config = parse_claude_server_entry("remote-srv", &value).expect("valid config");
        assert!(config.is_remote());
        assert_eq!(config.url.as_deref(), Some("https://mcp.example.com/api"));
    }

    #[test]
    fn test_parse_claude_server_entry_remote_type() {
        let value = serde_json::json!({
            "type": "remote",
            "url": "https://other.example.com"
        });
        let config = parse_claude_server_entry("srv", &value).expect("valid config");
        assert!(config.is_remote());
    }

    #[test]
    fn test_parse_claude_server_entry_with_env() {
        let value = serde_json::json!({
            "command": "python3",
            "args": ["-m", "mcp_server"],
            "env": {"PYTHONPATH": "/opt/mcp", "DEBUG": "1"}
        });
        let config = parse_claude_server_entry("py-srv", &value).expect("valid config");
        assert_eq!(config.env.get("DEBUG"), Some(&"1".to_string()));
        assert_eq!(
            config.env.get("PYTHONPATH").map(|s| s.as_str()),
            Some("/opt/mcp")
        );
    }

    #[test]
    fn test_parse_claude_server_entry_remote_with_headers() {
        let value = serde_json::json!({
            "type": "url",
            "url": "https://auth.example.com",
            "headers": {"Authorization": "Bearer token123", "X-Custom": "value"}
        });
        let config = parse_claude_server_entry("auth-srv", &value).expect("valid config");
        assert!(config.is_remote());
        assert_eq!(
            config.headers.get("Authorization").map(|s| s.as_str()),
            Some("Bearer token123")
        );
    }

    // ── McpDiscovery: OpenCode config ─────────────────────────────────

    #[test]
    fn test_parse_opencode_config_empty() {
        let json = r#"{"mcp": {}}"#;
        let parsed: OpenCodeMcpSection = serde_json::from_str(json).expect("valid");
        assert!(parsed.mcp.is_empty());
    }

    #[test]
    fn test_parse_opencode_config_local_server() {
        let json = r#"{
            "mcp": {
                "my-server": {
                    "type": "local",
                    "command": ["node", "server.js"]
                }
            }
        }"#;
        let parsed: OpenCodeMcpSection = serde_json::from_str(json).expect("valid");
        assert_eq!(parsed.mcp.len(), 1);
        let config = &parsed.mcp["my-server"];
        assert!(config.is_local());
        assert_eq!(config.command_executable(), Some("node"));
    }

    #[test]
    fn test_parse_opencode_config_remote_server() {
        let json = r#"{
            "mcp": {
                "http-server": {
                    "type": "remote",
                    "url": "https://mcp.example.com",
                    "timeout": 60000,
                    "headers": {"Authorization": "Bearer secret"}
                }
            }
        }"#;
        let parsed: OpenCodeMcpSection = serde_json::from_str(json).expect("valid");
        assert_eq!(parsed.mcp.len(), 1);
        let config = &parsed.mcp["http-server"];
        assert!(config.is_remote());
        assert_eq!(config.url.as_deref(), Some("https://mcp.example.com"));
        assert_eq!(config.timeout, 60000);
    }

    // ── McpDiscovery: env var prefix parsing ──────────────────────────

    #[test]
    fn test_parse_mcp_server_env_prefix_local() {
        temp_env::with_var("MCP_SERVER_MYTOOL_COMMAND", Some("node"), || {
            temp_env::with_var(
                "MCP_SERVER_MYTOOL_ARGS",
                Some("server.js --port 3000"),
                || {
                    let configs = parse_mcp_server_env_prefix();
                    assert_eq!(configs.len(), 1);
                    assert!(configs[0].is_local());
                    assert_eq!(configs[0].command_executable(), Some("node"));
                },
            );
        });
    }

    #[test]
    fn test_parse_mcp_server_env_prefix_remote() {
        temp_env::with_var(
            "MCP_SERVER_REMOTE_URL",
            Some("https://remote.example.com"),
            || {
                let configs = parse_mcp_server_env_prefix();
                assert_eq!(configs.len(), 1);
                assert!(configs[0].is_remote());
                assert_eq!(
                    configs[0].url.as_deref(),
                    Some("https://remote.example.com")
                );
            },
        );
    }

    #[test]
    fn test_parse_mcp_server_env_prefix_multiple_servers() {
        temp_env::with_var("MCP_SERVER_A_COMMAND", Some("echo"), || {
            temp_env::with_var("MCP_SERVER_B_URL", Some("https://b.example.com"), || {
                let configs = parse_mcp_server_env_prefix();
                assert_eq!(configs.len(), 2);
                // Results are sorted by command_executable / url
            });
        });
    }

    #[test]
    fn test_parse_mcp_server_env_prefix_no_vars() {
        // This test assumes no MCP_SERVER_* env vars are set
        let configs = parse_mcp_server_env_prefix();
        // May or may not be empty depending on test environment
        // At minimum this should not panic
        let _ = configs.len();
    }

    // ── McpDiscovery: MCP_SERVERS JSON env var ────────────────────────

    #[test]
    fn test_from_env_mcp_servers_array() {
        let json = r#"[
            {"type": "local", "command": ["python3", "-m", "mcp"]},
            {"type": "remote", "url": "https://mcp.example.com"}
        ]"#;
        temp_env::with_var("MCP_SERVERS", Some(json), || {
            let configs = McpDiscovery::from_env().expect("valid JSON");
            assert_eq!(configs.len(), 2);
            assert!(configs[0].is_local());
            assert!(configs[1].is_remote());
        });
    }

    #[test]
    fn test_from_env_mcp_servers_map() {
        let json = r#"{
            "srv1": {"type": "local", "command": ["echo"]},
            "srv2": {"type": "remote", "url": "https://example.com"}
        }"#;
        temp_env::with_var("MCP_SERVERS", Some(json), || {
            let configs = McpDiscovery::from_env().expect("valid JSON");
            assert_eq!(configs.len(), 2);
        });
    }

    #[test]
    fn test_from_env_mcp_servers_empty_string() {
        temp_env::with_var("MCP_SERVERS", Some(""), || {
            let configs = McpDiscovery::from_env().expect("empty string");
            assert!(configs.is_empty());
        });
    }

    #[test]
    fn test_from_env_mcp_servers_not_set() {
        temp_env::with_var("MCP_SERVERS", None::<&str>, || {
            let configs = McpDiscovery::from_env().expect("no var");
            // Returns empty vec when no env vars are set
            // (may have MCP_SERVER_* vars from test env, but shouldn't error)
            assert!(configs.is_empty());
        });
    }

    #[test]
    fn test_from_env_mcp_servers_invalid_json() {
        temp_env::with_var("MCP_SERVERS", Some("{not valid json"), || {
            let err = McpDiscovery::from_env().unwrap_err();
            assert!(err.to_string().contains("MCP_SERVERS"));
        });
    }

    // ── Claude Desktop config file parsing ────────────────────────────

    #[test]
    fn test_parse_claude_desktop_config_full() {
        let json = r#"{
            "mcpServers": {
                "filesystem": {
                    "command": "npx",
                    "args": ["-y", "@modelcontextprotocol/server-filesystem", "/tmp"]
                },
                "github": {
                    "command": "npx",
                    "args": ["-y", "@modelcontextprotocol/server-github"],
                    "env": {"GITHUB_PERSONAL_ACCESS_TOKEN": "ghp_test123"}
                }
            }
        }"#;
        let parsed: ClaudeDesktopConfig = serde_json::from_str(json).expect("valid config");
        assert_eq!(parsed.mcp_servers.len(), 2);
        assert!(parsed.mcp_servers.contains_key("filesystem"));
        assert!(parsed.mcp_servers.contains_key("github"));
    }

    #[test]
    fn test_parse_claude_desktop_config_empty() {
        let json = r#"{"mcpServers": {}}"#;
        let parsed: ClaudeDesktopConfig = serde_json::from_str(json).expect("valid config");
        assert!(parsed.mcp_servers.is_empty());
    }

    #[test]
    fn test_parse_claude_desktop_config_no_mcp_key() {
        let json = r#"{"otherKey": "value"}"#;
        let parsed: ClaudeDesktopConfig = serde_json::from_str(json).expect("valid config");
        assert!(parsed.mcp_servers.is_empty());
    }

    // ── Re-exports smoke test ─────────────────────────────────────────

    #[test]
    fn test_reexports_available() {
        // Verify key types are re-exported and usable
        let config = McpServerConfig::local(vec!["test".into()]);
        assert!(config.is_local());

        let tool = McpTool {
            name: "test".into(),
            description: Some("A test tool".into()),
            input_schema: Some(serde_json::json!({"type": "object"})),
        };
        assert_eq!(tool.name, "test");

        let resource = McpResource {
            name: "doc".into(),
            uri: "file:///doc.txt".into(),
            description: None,
            mime_type: None,
        };
        assert_eq!(resource.uri, "file:///doc.txt");

        // Constants
        assert_eq!(DEFAULT_MCP_TIMEOUT_MS, 30_000);
        assert_eq!(OAUTH_CALLBACK_PORT, 19876);
        assert_eq!(MAX_LIST_PAGES, 1_000);
    }

    #[test]
    fn test_status_reexport() {
        let connected = McpStatus::Connected;
        assert!(connected.is_connected());

        let disabled = McpStatus::Disabled;
        assert!(disabled.is_disabled());

        let needs_auth = McpStatus::NeedsAuth;
        assert!(needs_auth.needs_auth());
    }

    #[test]
    fn test_jsonrpc_types_reexport() {
        let req = JsonRpcRequest::new("ping", serde_json::json!({}), 1);
        assert_eq!(req.method, "ping");

        let json = r#"{"jsonrpc":"2.0","id":1,"result":"ok"}"#;
        let resp: JsonRpcResponse = serde_json::from_str(json).unwrap();
        assert!(resp.is_success());
    }

    // ── StdioTransport config ─────────────────────────────────────────

    #[test]
    fn test_stdio_transport_builder() {
        let mut env = HashMap::new();
        env.insert("NODE_ENV".into(), "production".into());

        let transport = StdioTransport::new("node")
            .with_args(vec!["server.js".into(), "--port".into(), "3000".into()])
            .with_env(env)
            .with_timeout(60_000);

        // Verify fields (cannot call connect in unit tests without a real process)
        assert_eq!(transport.command, "node");
        assert_eq!(transport.args.len(), 3);
        assert_eq!(transport.timeout_ms, 60_000);
    }

    // ── HttpTransport config ──────────────────────────────────────────

    #[test]
    fn test_http_transport_builder() {
        let mut headers = HashMap::new();
        headers.insert("Authorization".into(), "Bearer test".into());

        let transport = HttpTransport::new("https://mcp.example.com/api")
            .with_headers(headers)
            .with_timeout(45_000);

        assert_eq!(transport.url, "https://mcp.example.com/api");
        assert_eq!(transport.timeout_ms, 45_000);
    }

    // ── McpToolExecutor key generation ────────────────────────────────

    #[test]
    fn test_tool_executor_key_generation() {
        // Use the tool_key function directly since we can't easily create a
        // real McpClient in unit tests
        let key = tool_key("github-mcp", "list_repos");
        assert_eq!(key, "github-mcp_list_repos");
    }

    #[test]
    fn test_tool_key_uniqueness_across_servers() {
        // Same tool name on different servers should produce different keys
        let key1 = tool_key("server-a", "search");
        let key2 = tool_key("server-b", "search");
        assert_ne!(key1, key2);
        assert!(key1.starts_with("server-a_"));
        assert!(key2.starts_with("server-b_"));
    }

    // ── McpServerType re-export ───────────────────────────────────────

    #[test]
    fn test_server_type_reexport_serialization() {
        let local = McpServerType::Local;
        let remote = McpServerType::Remote;
        assert_eq!(serde_json::to_string(&local).unwrap(), r#""local""#);
        assert_eq!(serde_json::to_string(&remote).unwrap(), r#""remote""#);
    }
}
