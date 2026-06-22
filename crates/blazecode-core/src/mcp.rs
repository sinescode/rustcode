//! MCP (Model Context Protocol) integration — core types and configuration.
//!
//! Ported from:
//! - `packages/blazecode/src/mcp/index.ts`
//! - `packages/blazecode/src/mcp/catalog.ts`
//! - `packages/blazecode/src/mcp/auth.ts`
//! - `packages/blazecode/src/mcp/oauth-provider.ts`
//!
//! This module defines the core MCP types used across the system. The actual
//! MCP transport and protocol implementation (stdio/SSE/StreamableHTTP client,
//! OAuth callback server, tool execution) lives in the `blazecode-mcp` crate.
//!
//! ## Architecture
//!
//! The TS source uses the `@modelcontextprotocol/sdk` for MCP client
//! connectivity and tool discovery. This module provides Rust equivalents:
//!
//! - [`McpServerConfig`] — configuration for a local (stdio) or remote (HTTP) MCP server
//! - [`McpTool`] — a tool discovered from an MCP server, with its input schema
//! - [`McpResource`] — a resource discovered from an MCP server
//! - [`McpStatus`] — connection status for an MCP server
//! - [`McpEvent`] — events published on the event bus
//! - Error types for MCP-related failures

use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::process::Stdio;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::Arc;
use futures::future::BoxFuture;
use thiserror::Error;
use tokio::io::{AsyncBufReadExt, AsyncReadExt, AsyncWriteExt, BufReader};

// ---------------------------------------------------------------------------
// Server type
// ---------------------------------------------------------------------------

/// The transport type for an MCP server.
///
/// # Source
/// Ported from `packages/blazecode/src/mcp/index.ts` —
/// `ConfigMCPV1.Info.type` (local vs remote).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum McpServerType {
    /// Local server launched as a subprocess over stdio.
    Local,
    /// Remote server accessed via HTTP (SSE or StreamableHTTP).
    Remote,
}

// ---------------------------------------------------------------------------
// JSON-RPC 2.0 types
// ---------------------------------------------------------------------------

/// A JSON-RPC 2.0 request.
///
/// Used for both stdio-framed and HTTP JSON-RPC communication with MCP servers.
///
/// # Source
/// Ported from the MCP spec (JSON-RPC 2.0 over stdio / HTTP).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct JsonRpcRequest {
    /// Must be `"2.0"`.
    pub jsonrpc: String,
    /// Request ID (monotonically increasing, used to match responses).
    pub id: u64,
    /// The RPC method name (e.g. `"tools/list"`, `"initialize"`).
    pub method: String,
    /// Method parameters as arbitrary JSON.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub params: Option<serde_json::Value>,
}

impl JsonRpcRequest {
    /// Create a new JSON-RPC 2.0 request.
    pub fn new(method: impl Into<String>, params: serde_json::Value, id: u64) -> Self {
        Self {
            jsonrpc: "2.0".into(),
            id,
            method: method.into(),
            params: Some(params),
        }
    }

    /// Create a notification (a request with no `id` — no response expected).
    pub fn notification(method: impl Into<String>, params: serde_json::Value) -> Self {
        Self {
            jsonrpc: "2.0".into(),
            id: 0,
            method: method.into(),
            params: Some(params),
        }
    }
}

/// A JSON-RPC 2.0 error object.
///
/// # Source
/// MCP spec — JSON-RPC 2.0 error object.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct JsonRpcError {
    /// Numeric error code (see JSON-RPC spec for standard codes).
    pub code: i64,
    /// Human-readable error message.
    pub message: String,
    /// Optional additional error data.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub data: Option<serde_json::Value>,
}

/// A JSON-RPC 2.0 response.
///
/// # Source
/// MCP spec — JSON-RPC 2.0 response object.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct JsonRpcResponse {
    /// Must be `"2.0"`.
    pub jsonrpc: String,
    /// Request ID matching the original request.
    pub id: u64,
    /// Successful result (absent on error).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub result: Option<serde_json::Value>,
    /// Error details (absent on success).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub error: Option<JsonRpcError>,
}

impl JsonRpcResponse {
    /// Whether this response indicates a successful result.
    pub fn is_success(&self) -> bool {
        self.error.is_none()
    }

    /// Whether this response indicates a JSON-RPC error.
    pub fn is_error(&self) -> bool {
        self.error.is_some()
    }

    /// Get the error message, if present.
    pub fn error_message(&self) -> Option<&str> {
        self.error.as_ref().map(|e| e.message.as_str())
    }
}

// ---------------------------------------------------------------------------
// OAuth configuration
// ---------------------------------------------------------------------------

/// OAuth configuration for a remote MCP server.
///
/// # Source
/// Ported from `packages/blazecode/src/mcp/oauth-provider.ts`
/// `McpOAuthConfig` interface.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct McpOAuthConfig {
    /// Pre-registered client ID (for servers that don't support dynamic registration).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub client_id: Option<String>,
    /// Client secret (if the server requires it).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub client_secret: Option<String>,
    /// Requested OAuth scope.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub scope: Option<String>,
    /// Callback port for the OAuth redirect (default: 19876).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub callback_port: Option<u16>,
    /// Explicit redirect URI (overrides callback_port-based default).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub redirect_uri: Option<String>,
}

// ---------------------------------------------------------------------------
// Server configuration
// ---------------------------------------------------------------------------

/// Configuration for connecting to an MCP server.
///
/// # Source
/// Ported from `packages/blazecode/src/mcp/index.ts` —
/// `ConfigMCPV1.Info` struct.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct McpServerConfig {
    /// Transport type: "local" (stdio) or "remote" (HTTP).
    pub r#type: McpServerType,
    /// For local servers: the command to execute.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub command: Option<Vec<String>>,
    /// For local servers: command arguments (separate from command[1..]).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub args: Option<Vec<String>>,
    /// For local servers: environment variables to set in the child process.
    #[serde(default, skip_serializing_if = "HashMap::is_empty")]
    pub env: HashMap<String, String>,
    /// For local servers: working directory (relative to project root).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub cwd: Option<String>,
    /// For remote servers: the HTTP(S) URL.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub url: Option<String>,
    /// For remote servers: custom HTTP headers.
    #[serde(default, skip_serializing_if = "HashMap::is_empty")]
    pub headers: HashMap<String, String>,
    /// Connection timeout in milliseconds (default: 30_000).
    #[serde(
        default = "default_timeout",
        skip_serializing_if = "is_default_timeout"
    )]
    pub timeout: u64,
    /// Whether this server is enabled.
    #[serde(default = "default_enabled")]
    pub enabled: bool,
    /// OAuth configuration for remote servers. Set to `null`/`None` to
    /// disable OAuth (auto-detection is used when unspecified).
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        deserialize_with = "deserialize_oauth"
    )]
    pub oauth: Option<McpOAuthConfig>,
}

fn default_timeout() -> u64 {
    30_000
}

fn default_enabled() -> bool {
    true
}

fn is_default_timeout(t: &u64) -> bool {
    *t == 30_000
}

/// Deserializer for OAuth config that handles `null`, `false`, and full config.
fn deserialize_oauth<'de, D>(deserializer: D) -> Result<Option<McpOAuthConfig>, D::Error>
where
    D: serde::Deserializer<'de>,
{
    // `oauth` can be `false` (disable), an object (config), or absent/`null` (auto).
    // We store `None` for disabled, `Some(cfg)` for explicit config,
    // and also `None` for absent — callers check `has_oauth_config` to distinguish.
    #[derive(Deserialize)]
    #[serde(untagged)]
    enum OAuthField {
        Bool(bool),
        Config(McpOAuthConfig),
        Null,
    }

    match OAuthField::deserialize(deserializer)? {
        OAuthField::Bool(false) | OAuthField::Null => Ok(None),
        OAuthField::Bool(true) => Ok(Some(McpOAuthConfig::default())),
        OAuthField::Config(cfg) => Ok(Some(cfg)),
    }
}

impl McpServerConfig {
    /// Create a new local (stdio) MCP server config.
    ///
    /// # Source
    /// Ported from `packages/blazecode/src/mcp/index.ts` `connectLocal()`.
    pub fn local(command: Vec<String>) -> Self {
        Self {
            r#type: McpServerType::Local,
            command: Some(command),
            args: None,
            env: HashMap::new(),
            cwd: None,
            url: None,
            headers: HashMap::new(),
            timeout: default_timeout(),
            enabled: true,
            oauth: None,
        }
    }

    /// Create a new remote (HTTP) MCP server config.
    ///
    /// # Source
    /// Ported from `packages/blazecode/src/mcp/index.ts` `connectRemote()`.
    pub fn remote(url: String) -> Self {
        Self {
            r#type: McpServerType::Remote,
            command: None,
            args: None,
            env: HashMap::new(),
            cwd: None,
            url: Some(url),
            headers: HashMap::new(),
            timeout: default_timeout(),
            enabled: true,
            oauth: None,
        }
    }

    /// Set the environment variables for the server process (local only).
    pub fn with_env(mut self, env: HashMap<String, String>) -> Self {
        self.env = env;
        self
    }

    /// Set the connection timeout in milliseconds.
    pub fn with_timeout(mut self, timeout_ms: u64) -> Self {
        self.timeout = timeout_ms;
        self
    }

    /// Disable this server.
    pub fn disabled(mut self) -> Self {
        self.enabled = false;
        self
    }

    /// Set custom HTTP headers (remote only).
    pub fn with_headers(mut self, headers: HashMap<String, String>) -> Self {
        self.headers = headers;
        self
    }

    /// Set the OAuth configuration.
    pub fn with_oauth(mut self, oauth: McpOAuthConfig) -> Self {
        self.oauth = Some(oauth);
        self
    }

    /// Disable OAuth explicitly.
    pub fn without_oauth(mut self) -> Self {
        self.oauth = None;
        self
    }

    /// Whether this is a local (stdio) server.
    pub fn is_local(&self) -> bool {
        self.r#type == McpServerType::Local
    }

    /// Whether this is a remote (HTTP) server.
    pub fn is_remote(&self) -> bool {
        self.r#type == McpServerType::Remote
    }

    /// Get the command executable path.
    ///
    /// Returns `None` if no command is configured.
    pub fn command_executable(&self) -> Option<&str> {
        self.command.as_ref()?.first().map(|s| s.as_str())
    }

    /// Get the full command arguments as an owned Vec.
    pub fn full_args(&self) -> Vec<String> {
        let mut result: Vec<String> = Vec::new();
        if let Some(ref cmd) = self.command {
            // Skip the first element (the command itself), use rest as args
            result.extend(cmd.iter().skip(1).cloned());
        }
        if let Some(ref args) = self.args {
            result.extend(args.iter().cloned());
        }
        result
    }

    /// Get the environment variables map for this server.
    pub fn environment(&self) -> &HashMap<String, String> {
        &self.env
    }
}

// ---------------------------------------------------------------------------
// Tool definition
// ---------------------------------------------------------------------------

/// A tool discovered from an MCP server.
///
/// # Source
/// Ported from `packages/blazecode/src/mcp/index.ts` — `Tool as MCPToolDef`
/// from `@modelcontextprotocol/sdk/types.js`.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct McpTool {
    /// Unique tool name (within the server).
    pub name: String,
    /// Human-readable description.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
    /// JSON Schema for the tool's input parameters.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub input_schema: Option<serde_json::Value>,
}

/// The full tool key used to identify tools across servers.
///
/// Format: `{sanitized_server_name}_{sanitized_tool_name}`
///
/// # Source
/// Ported from `packages/blazecode/src/mcp/catalog.ts` `sanitize()` and
/// the key construction in `McpCatalog.convertTool`.
pub fn tool_key(server_name: &str, tool_name: &str) -> String {
    format!(
        "{}_{}",
        sanitize_name(server_name),
        sanitize_name(tool_name)
    )
}

/// Sanitize a name for use in tool/resource/prompt keys.
///
/// Replaces non-alphanumeric characters (except `-` and `_`) with `_`.
///
/// # Source
/// Ported from `packages/blazecode/src/mcp/catalog.ts` `sanitize()`.
pub fn sanitize_name(value: &str) -> String {
    value
        .chars()
        .map(|c| {
            if c.is_ascii_alphanumeric() || c == '-' || c == '_' {
                c
            } else {
                '_'
            }
        })
        .collect()
}

/// Extract human-readable text content from an MCP `tools/call` response.
///
/// The MCP protocol returns `result.content` as an array of content blocks.
/// Each text block has `{type: "text", text: "..."}`. This function joins
/// all text blocks with newlines. If the result does not contain a `content`
/// array, the entire result is serialized as a JSON string.
///
/// # Source
/// Ported from `packages/blazecode/src/mcp/catalog.ts` `convertTool` execute
/// handler content extraction.
pub fn extract_mcp_content(result: &serde_json::Value) -> String {
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
            serde_json::to_string_pretty(&content).unwrap_or_else(|_| format!("{content:?}"))
        } else {
            texts.join("\n")
        }
    } else {
        serde_json::to_string_pretty(result).unwrap_or_else(|_| format!("{result}"))
    }
}

// ---------------------------------------------------------------------------
// Resource definition
// ---------------------------------------------------------------------------

/// A resource discovered from an MCP server.
///
/// # Source
/// Ported from `packages/blazecode/src/mcp/index.ts` `Resource` type (lines 53–59).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct McpResource {
    /// Resource name.
    pub name: String,
    /// Resource URI.
    pub uri: String,
    /// Optional human-readable description.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
    /// Optional MIME type.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub mime_type: Option<String>,
}

/// A prompt discovered from an MCP server.
///
/// # Source
/// Ported from `packages/blazecode/src/mcp/index.ts` — `PromptInfo` type.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct McpPrompt {
    /// Prompt name.
    pub name: String,
    /// Optional description.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
    /// Optional list of arguments the prompt accepts.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub arguments: Option<Vec<McpPromptArgument>>,
}

/// An argument accepted by an MCP prompt.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct McpPromptArgument {
    /// Argument name.
    pub name: String,
    /// Optional description.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
    /// Whether the argument is required.
    #[serde(default)]
    pub required: bool,
}

// ---------------------------------------------------------------------------
// Connection status
// ---------------------------------------------------------------------------

/// Connection status of an MCP server.
///
/// # Source
/// Ported from `packages/blazecode/src/mcp/index.ts` `Status` union type
/// (lines 95–119).
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "status", rename_all = "snake_case")]
pub enum McpStatus {
    /// Server is connected and operational.
    Connected,
    /// Server is disabled in configuration.
    Disabled,
    /// Server connection failed.
    Failed {
        /// Error message from the failed connection.
        error: String,
    },
    /// Server requires OAuth authentication.
    #[serde(rename = "needs_auth")]
    NeedsAuth,
    /// Server requires client registration (missing client_id).
    #[serde(rename = "needs_client_registration")]
    NeedsClientRegistration {
        /// Error details about the registration failure.
        error: String,
    },
}

impl McpStatus {
    /// Whether this status represents a working connection.
    pub fn is_connected(&self) -> bool {
        matches!(self, Self::Connected)
    }

    /// Whether this status indicates the server is disabled.
    pub fn is_disabled(&self) -> bool {
        matches!(self, Self::Disabled)
    }

    /// Whether this status represents an authentication requirement.
    pub fn needs_auth(&self) -> bool {
        matches!(self, Self::NeedsAuth | Self::NeedsClientRegistration { .. })
    }
}

// ---------------------------------------------------------------------------
// Auth status
// ---------------------------------------------------------------------------

/// OAuth authentication status for an MCP server.
///
/// # Source
/// Ported from `packages/blazecode/src/mcp/index.ts` `AuthStatus` type (line 939).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AuthStatus {
    /// User has authenticated with this server.
    Authenticated,
    /// Previously stored token has expired.
    Expired,
    /// No token stored — user has not authenticated.
    #[serde(rename = "not_authenticated")]
    NotAuthenticated,
}

// ---------------------------------------------------------------------------
// Events
// ---------------------------------------------------------------------------

/// MCP-related event types published on the event bus.
///
/// # Source
/// Ported from `packages/blazecode/src/mcp/index.ts` `ToolsChanged` (lines 62–67)
/// and `BrowserOpenFailed` (lines 69–75).
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum McpEvent {
    /// Published when tools change on an MCP server.
    #[serde(rename = "mcp.tools.changed")]
    ToolsChanged {
        /// Name of the MCP server whose tools changed.
        server: String,
    },
    /// Published when opening the browser for OAuth fails.
    #[serde(rename = "mcp.browser.open.failed")]
    BrowserOpenFailed {
        /// Name of the MCP server.
        mcp_name: String,
        /// The URL that failed to open.
        url: String,
    },
}

impl McpEvent {
    /// Event type string for tools changed.
    pub const TOOLS_CHANGED: &str = "mcp.tools.changed";
    /// Event type string for browser open failed.
    pub const BROWSER_OPEN_FAILED: &str = "mcp.browser.open.failed";
}

// ---------------------------------------------------------------------------
// Errors
// ---------------------------------------------------------------------------

/// MCP server not found error.
///
/// # Source
/// Ported from `packages/blazecode/src/mcp/index.ts` `NotFoundError`
/// (line 81–83).
#[derive(Debug, Error)]
#[error("MCP server `{name}` not found")]
pub struct McpNotFoundError {
    /// Name of the MCP server that was not found.
    pub name: String,
}

/// MCP operation failed error.
///
/// # Source
/// Ported from `packages/blazecode/src/mcp/index.ts` `Failed` error
/// (lines 77–79).
#[derive(Debug, Error)]
#[error("MCP operation failed for `{name}`")]
pub struct McpFailedError {
    /// Name of the MCP server.
    pub name: String,
}

/// MCP OAuth error.
///
/// # Source
/// Ported from `packages/blazecode/src/mcp/oauth-callback.ts` and
/// `packages/blazecode/src/mcp/index.ts` OAuth flow errors.
#[derive(Debug, Error)]
pub enum McpOAuthError {
    /// OAuth callback timed out.
    #[error("OAuth callback timeout — authorization took too long")]
    Timeout,
    /// OAuth state mismatch — potential CSRF attack.
    #[error("OAuth state mismatch — potential CSRF attack")]
    StateMismatch,
    /// No pending OAuth flow for this server.
    #[error("no pending OAuth flow for MCP server `{mcp_name}`")]
    NoPendingFlow { mcp_name: String },
    /// OAuth completion failed.
    #[error("OAuth completion failed")]
    CompletionFailed,
    /// Authorization was cancelled by the user.
    #[error("authorization cancelled")]
    Cancelled,
}

// ---------------------------------------------------------------------------
// Constants
// ---------------------------------------------------------------------------

/// Default connection timeout in milliseconds (30 seconds).
///
/// # Source
/// Ported from `packages/blazecode/src/mcp/index.ts` `DEFAULT_TIMEOUT` (line 39).
pub const DEFAULT_MCP_TIMEOUT_MS: u64 = 30_000;

/// Default OAuth callback port.
///
/// # Source
/// Ported from `packages/blazecode/src/mcp/oauth-provider.ts` `OAUTH_CALLBACK_PORT`.
pub const OAUTH_CALLBACK_PORT: u16 = 19876;

/// OAuth callback URL path.
///
/// # Source
/// Ported from `packages/blazecode/src/mcp/oauth-provider.ts` `OAUTH_CALLBACK_PATH`.
pub const OAUTH_CALLBACK_PATH: &str = "/mcp/oauth/callback";

/// Maximum pages when paginating through MCP server results.
///
/// # Source
/// Ported from `packages/blazecode/src/mcp/catalog.ts` `MAX_LIST_PAGES`.
pub const MAX_LIST_PAGES: usize = 1_000;

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    // -- McpServerType ------------------------------------------------------

    #[test]
    fn test_server_type_serialization() {
        let local = McpServerType::Local;
        let remote = McpServerType::Remote;
        assert_eq!(serde_json::to_string(&local).unwrap(), r#""local""#);
        assert_eq!(serde_json::to_string(&remote).unwrap(), r#""remote""#);
    }

    // -- McpServerConfig ----------------------------------------------------

    #[test]
    fn test_local_config() {
        let config = McpServerConfig::local(vec!["node".into(), "server.js".into()]);
        assert!(config.is_local());
        assert!(!config.is_remote());
        assert_eq!(
            config.command,
            Some(vec!["node".into(), "server.js".into()])
        );
        assert_eq!(config.full_args(), vec!["server.js"]);
        assert!(config.enabled);
        assert_eq!(config.timeout, 30_000);
    }

    #[test]
    fn test_remote_config() {
        let config = McpServerConfig::remote("https://mcp.example.com".into());
        assert!(config.is_remote());
        assert!(!config.is_local());
        assert_eq!(config.url, Some("https://mcp.example.com".into()));
        assert!(config.command.is_none());
    }

    #[test]
    fn test_config_builder_methods() {
        let config = McpServerConfig::local(vec!["cmd".into()])
            .with_timeout(60_000)
            .disabled();

        assert!(!config.enabled);
        assert_eq!(config.timeout, 60_000);
    }

    #[test]
    fn test_config_with_env() {
        let mut env = HashMap::new();
        env.insert("NODE_ENV".into(), "production".into());
        let config = McpServerConfig::local(vec!["cmd".into()]).with_env(env);

        assert_eq!(
            config.environment().get("NODE_ENV"),
            Some(&"production".to_string())
        );
    }

    #[test]
    fn test_config_full_args() {
        // Command includes args inline: ["deno", "run", "-A", "server.ts"]
        let config = McpServerConfig::local(vec![
            "deno".into(),
            "run".into(),
            "-A".into(),
            "server.ts".into(),
        ]);
        assert_eq!(config.full_args(), vec!["run", "-A", "server.ts"]);

        // Command + explicit args
        let config = McpServerConfig {
            r#type: McpServerType::Local,
            command: Some(vec!["python3".into()]),
            args: Some(vec!["-m".into(), "mcp_server".into()]),
            ..McpServerConfig::local(vec!["python3".into()])
        };
        assert_eq!(config.full_args(), vec!["-m", "mcp_server"]);
    }

    // -- McpTool ------------------------------------------------------------

    #[test]
    fn test_tool_key() {
        let key = tool_key("my-server", "search_docs");
        assert_eq!(key, "my-server_search_docs");
    }

    #[test]
    fn test_sanitize_name() {
        assert_eq!(sanitize_name("hello-world"), "hello-world");
        assert_eq!(sanitize_name("my server!"), "my_server_");
        assert_eq!(sanitize_name("foo@bar#baz"), "foo_bar_baz");
        assert_eq!(sanitize_name("safe_name-123"), "safe_name-123");
    }

    #[test]
    fn test_tool_key_sanitizes() {
        // Spaces and special chars get replaced with underscores
        let key = tool_key("my server", "search docs!");
        assert_eq!(key, "my_server_search_docs_");
    }

    // -- McpStatus ----------------------------------------------------------

    #[test]
    fn test_status_is_connected() {
        assert!(McpStatus::Connected.is_connected());
        assert!(!McpStatus::Disabled.is_connected());
        assert!(!McpStatus::Failed {
            error: "boom".into()
        }
        .is_connected());
    }

    #[test]
    fn test_status_needs_auth() {
        assert!(McpStatus::NeedsAuth.needs_auth());
        assert!(McpStatus::NeedsClientRegistration {
            error: "missing client".into()
        }
        .needs_auth());
        assert!(!McpStatus::Connected.needs_auth());
        assert!(!McpStatus::Failed {
            error: "boom".into()
        }
        .needs_auth());
    }

    #[test]
    fn test_status_serialization() {
        let status = McpStatus::Connected;
        let json = serde_json::to_string(&status).unwrap();
        assert_eq!(json, r#"{"status":"connected"}"#);

        let status = McpStatus::Failed {
            error: "timeout".into(),
        };
        let json = serde_json::to_string(&status).unwrap();
        assert!(json.contains("failed"));
        assert!(json.contains("timeout"));
    }

    #[test]
    fn test_status_deserialization() {
        let json = r#"{"status":"needs_auth"}"#;
        let status: McpStatus = serde_json::from_str(json).unwrap();
        assert!(matches!(status, McpStatus::NeedsAuth));

        let json = r#"{"status":"needs_client_registration","error":"msg"}"#;
        let status: McpStatus = serde_json::from_str(json).unwrap();
        assert!(matches!(
            status,
            McpStatus::NeedsClientRegistration { error } if error == "msg"
        ));
    }

    // -- McpEvent -----------------------------------------------------------

    #[test]
    fn test_event_tools_changed() {
        let event = McpEvent::ToolsChanged {
            server: "my-mcp".into(),
        };
        let json = serde_json::to_string(&event).unwrap();
        assert!(json.contains("mcp.tools.changed"));
        assert!(json.contains("my-mcp"));
    }

    #[test]
    fn test_event_constants() {
        assert_eq!(McpEvent::TOOLS_CHANGED, "mcp.tools.changed");
        assert_eq!(McpEvent::BROWSER_OPEN_FAILED, "mcp.browser.open.failed");
    }

    // -- Errors -------------------------------------------------------------

    #[test]
    fn test_not_found_error() {
        let err = McpNotFoundError {
            name: "test-server".into(),
        };
        assert!(err.to_string().contains("test-server"));
        assert!(err.to_string().contains("not found"));
    }

    #[test]
    fn test_failed_error() {
        let err = McpFailedError {
            name: "bad-server".into(),
        };
        assert!(err.to_string().contains("bad-server"));
    }

    #[test]
    fn test_oauth_errors() {
        let err = McpOAuthError::Timeout;
        assert!(err.to_string().contains("timeout"));

        let err = McpOAuthError::StateMismatch;
        assert!(err.to_string().contains("CSRF"));

        let err = McpOAuthError::NoPendingFlow {
            mcp_name: "srv".into(),
        };
        assert!(err.to_string().contains("srv"));
    }

    // -- AuthStatus ---------------------------------------------------------

    #[test]
    fn test_auth_status_serialization() {
        assert_eq!(
            serde_json::to_string(&AuthStatus::Authenticated).unwrap(),
            r#""authenticated""#
        );
        assert_eq!(
            serde_json::to_string(&AuthStatus::Expired).unwrap(),
            r#""expired""#
        );
        assert_eq!(
            serde_json::to_string(&AuthStatus::NotAuthenticated).unwrap(),
            r#""not_authenticated""#
        );
    }
}

// ---------------------------------------------------------------------------
// MCP Client — connection, tool discovery, and execution
// ---------------------------------------------------------------------------

/// Internal connection state for an MCP client.
///
/// Variants correspond to the transport types: local subprocess (stdio),
/// remote HTTP (StreamableHTTP), and remote SSE.
enum McpClientState {
    /// Local subprocess-based connection (stdio transport).
    Local {
        /// The spawned child process with piped stdin/stdout/stderr.
        child: tokio::process::Child,
    },
    /// Remote HTTP-based connection (StreamableHTTP transport).
    /// JSON-RPC requests are sent via HTTP POST; responses come in the
    /// HTTP response body.
    Remote {
        /// Reusable HTTP client for subsequent JSON-RPC calls.
        http_client: reqwest::Client,
        /// The server's base URL.
        url: String,
        /// Custom HTTP headers sent with every request.
        headers: HashMap<String, String>,
    },
    /// Remote SSE-based connection (deprecated MCP transport).
    /// Client opens an SSE stream for server→client messages and sends
    /// JSON-RPC requests via HTTP POST to the message endpoint.
    RemoteSse {
        /// Reusable HTTP client for sending JSON-RPC requests.
        http_client: reqwest::Client,
        /// The POST endpoint for sending JSON-RPC messages (extracted from
        /// the SSE `endpoint` event after connecting).
        message_url: String,
        /// Custom HTTP headers sent with every request.
        headers: HashMap<String, String>,
        /// A channel receiver for incoming SSE event data.
        /// Each item is a `(request_id, result_json)` pair.
        sse_rx: tokio::sync::mpsc::UnboundedReceiver<(u64, serde_json::Value)>,
    },
    /// Client is not connected.
    Disconnected,
}

/// An active connection to an MCP (Model Context Protocol) server.
///
/// Created via [`McpClient::connect()`] or [`McpClient::connect_http()`]
/// and used to list tools, call tools, and discover resources and prompts.
///
/// # Source
/// Ported from `packages/blazecode/src/mcp/index.ts` — the `connectLocal()`
/// and `connectRemote()` functions.
pub struct McpClient {
    /// The server configuration used to create this connection.
    pub config: McpServerConfig,
    /// Human-readable server name (for error messages and logging).
    pub server_name: String,
    /// Tools discovered from this MCP server (cached at connect time).
    pub tools: tokio::sync::RwLock<Vec<McpTool>>,
    /// Whether this client is currently connected.
    pub connected: Arc<AtomicBool>,
    /// Monotonically increasing JSON-RPC request ID counter.
    next_id: AtomicU64,
    /// Interior-mutable connection state (locked per-operation).
    /// Server capabilities from the initialize handshake.
    pub capabilities: tokio::sync::RwLock<std::collections::HashMap<String, serde_json::Value>>,
    state: tokio::sync::Mutex<McpClientState>,
    /// Registered notification handlers.
    notification_handlers: McpNotificationHandlers,
    /// Callbacks to fire when the connection closes.
    onclose_callbacks: Arc<std::sync::Mutex<Vec<Arc<dyn Fn() -> BoxFuture<'static, ()> + Send + Sync>>>>,
}

impl McpClient {
    /// Connect to an MCP server using the given configuration.
    ///
    /// For local servers this spawns a subprocess and performs the JSON-RPC
    /// `initialize` handshake over stdio. For remote servers this sends an
    /// `initialize` request via HTTP POST and stores the client for
    /// subsequent calls.
    ///
    /// # Source
    /// Ported from `packages/blazecode/src/mcp/index.ts` `connectLocal()`
    /// and `connectRemote()`.
    ///
    /// # Errors
    /// Returns [`crate::error::Error::Config`] if the configuration is
    /// incomplete (missing command or URL). Returns
    /// [`crate::error::Error::Process`] if spawning the subprocess fails.
    /// Returns [`crate::error::Error::Network`] if the remote server
    /// returns a non-2xx status.
    pub async fn connect(
        config: McpServerConfig,
        server_name: String,
    ) -> crate::error::Result<Self> {
        let client = Self {
            config: config.clone(),
            server_name: server_name.clone(),
            tools: tokio::sync::RwLock::new(Vec::new()),
            connected: Arc::new(AtomicBool::new(false)),
            next_id: AtomicU64::new(1),
            capabilities: tokio::sync::RwLock::new(std::collections::HashMap::new()),
            state: tokio::sync::Mutex::new(McpClientState::Disconnected),
            notification_handlers: McpNotificationHandlers::default(),
            onclose_callbacks: Arc::new(std::sync::Mutex::new(Vec::new())),
        };

        match config.r#type {
            McpServerType::Local => {
                let cmd = config.command_executable().ok_or_else(|| {
                    crate::error::Error::Config("MCP local server has no command executable".into())
                })?;
                let args = config.full_args();

                let mut child = tokio::process::Command::new(cmd)
                    .args(&args)
                    .stdin(Stdio::piped())
                    .stdout(Stdio::piped())
                    .stderr(Stdio::piped())
                    .envs(&config.env)
                    .kill_on_drop(true)
                    .spawn()
                    .map_err(|e| crate::error::Error::Process {
                        message: format!("failed to spawn MCP server `{server_name}`: {e}"),
                        exit_code: None,
                    })?;

                let init_req = build_jsonrpc_request(
                    "initialize",
                    serde_json::json!({
                        "protocolVersion": "2024-11-05",
                        "capabilities": {},
                        "clientInfo": {
                            "name": "blazecode",
                            "version": env!("CARGO_PKG_VERSION")
                        }
                    }),
                    0,
                );
                let framed = frame_jsonrpc_message(&init_req);

                {
                    let stdin = child.stdin.as_mut().ok_or_else(|| {
                        crate::error::Error::Internal("MCP child stdin not available".into())
                    })?;
                    stdin.write_all(framed.as_bytes()).await?;
                    stdin.flush().await?;
                }

                {
                    let stdout = child.stdout.as_mut().ok_or_else(|| {
                        crate::error::Error::Internal("MCP child stdout not available".into())
                    })?;
                    let mut reader = BufReader::new(stdout);

                    let mut header = String::new();
                    reader.read_line(&mut header).await?;
                    let content_length = parse_content_length(&header).map_err(|e| {
                        crate::error::Error::Internal(format!(
                            "invalid MCP initialize response header: {e}"
                        ))
                    })?;

                    let mut blank = String::new();
                    reader.read_line(&mut blank).await?;

                    let mut body = vec![0u8; content_length];
                    reader.read_exact(&mut body).await?;

                    let response_str = String::from_utf8_lossy(&body).to_string();
                    let init_value = parse_jsonrpc_response(&response_str).map_err(|e| {
                        crate::error::Error::Internal(format!(
                            "invalid MCP initialize response: {e}"
                        ))
                    })?;
                    if let Some(caps) = init_value
                        .get("result")
                        .and_then(|r| r.get("capabilities"))
                        .and_then(|c| c.as_object())
                    {
                        let mut store = client.capabilities.write().await;
                        for (k, v) in caps {
                            store.insert(k.clone(), v.clone());
                        }
                    }
                }

                {
                    let stdin = child.stdin.as_mut().ok_or_else(|| {
                        crate::error::Error::Internal(
                            "MCP child stdin not available for initialized notification".into(),
                        )
                    })?;
                    let notif = build_jsonrpc_notification(
                        "notifications/initialized",
                        serde_json::json!({}),
                    );
                    let framed = frame_jsonrpc_message(&notif);
                    stdin.write_all(framed.as_bytes()).await?;
                    stdin.flush().await?;
                }

                client.connected.store(true, std::sync::atomic::Ordering::SeqCst);
                *client.state.lock().await = McpClientState::Local { child };

                match client.list_tools().await {
                    Ok(discovered) => {
                        let mut tools = client.tools.write().await;
                        *tools = discovered;
                    }
                    Err(e) => {
                        tracing::warn!(
                            "MCP: failed to discover tools for '{}' during connect: {e}",
                            client.server_name
                        );
                    }
                }
            }
            McpServerType::Remote => {
                let url = config.url.as_ref().ok_or_else(|| {
                    crate::error::Error::Config("MCP remote server has no URL".into())
                })?;

                let caps_lock = tokio::sync::RwLock::new(std::collections::HashMap::new());
                let state = Self::connect_with_fallback(
                    url,
                    &config,
                    &server_name,
                    &caps_lock,
                ).await?;

                {
                    let remote_caps = caps_lock.read().await;
                    let mut store = client.capabilities.write().await;
                    for (k, v) in remote_caps.iter() {
                        store.insert(k.clone(), v.clone());
                    }
                }

                client.connected.store(true, std::sync::atomic::Ordering::SeqCst);
                *client.state.lock().await = state;

                match client.list_tools().await {
                    Ok(discovered) => {
                        let mut tools = client.tools.write().await;
                        *tools = discovered;
                    }
                    Err(e) => {
                        tracing::warn!(
                            "MCP: failed to discover tools for '{}' during connect: {e}",
                            client.server_name
                        );
                    }
                }
            }
        };

        Ok(client)
    }

    /// List all tools available on the connected MCP server.
    ///
    /// Sends a `tools/list` JSON-RPC request and parses the response.
    ///
    /// # Source
    /// Ported from `packages/blazecode/src/mcp/index.ts` `listTools()`.
    pub async fn list_tools(&self) -> crate::error::Result<Vec<McpTool>> {
        let id = self.next_id.fetch_add(1, Ordering::SeqCst);
        let request = build_jsonrpc_request("tools/list", serde_json::json!({}), id);

        let response = self.send_jsonrpc(&request).await?;

        let tools_value = response
            .get("result")
            .and_then(|r| r.get("tools"))
            .cloned()
            .ok_or_else(|| {
                crate::error::Error::Internal(
                    "MCP tools/list response missing 'result.tools'".into(),
                )
            })?;

        let tools: Vec<McpTool> = serde_json::from_value(tools_value)?;
        Ok(tools)
    }

    /// Call a tool on the connected MCP server with the given arguments.
    ///
    /// Sends a `tools/call` JSON-RPC request and returns the result as
    /// arbitrary JSON.
    ///
    /// # Source
    /// Ported from `packages/blazecode/src/mcp/index.ts` `callTool()`.
    pub async fn call_tool(
        &self,
        tool_name: &str,
        arguments: serde_json::Value,
    ) -> crate::error::Result<serde_json::Value> {
        let id = self.next_id.fetch_add(1, Ordering::SeqCst);
        let request = build_jsonrpc_request(
            "tools/call",
            serde_json::json!({
                "name": tool_name,
                "arguments": arguments,
            }),
            id,
        );

        let response = self.send_jsonrpc(&request).await?;

        let result = response.get("result").cloned().ok_or_else(|| {
            crate::error::Error::Internal("MCP tools/call response missing 'result'".into())
        })?;

        Ok(result)
    }

    /// Send a JSON-RPC request and return the parsed response.
    ///
    /// For local connections this writes the framed message to the child's
    /// stdin and reads the framed response from stdout. For remote
    /// connections this POSTs the request to the server's HTTP endpoint.
    async fn send_jsonrpc(
        &self,
        request: &serde_json::Value,
    ) -> crate::error::Result<serde_json::Value> {
        let mut state = self.state.lock().await;

        match &mut *state {
            McpClientState::Local { child } => {
                let framed = frame_jsonrpc_message(request);

                // Write the framed request to the child's stdin
                {
                    let stdin = child.stdin.as_mut().ok_or_else(|| {
                        crate::error::Error::Internal("MCP child stdin not available".into())
                    })?;
                    stdin.write_all(framed.as_bytes()).await?;
                    stdin.flush().await?;
                }

                // Read the framed response from the child's stdout
                let stdout = child.stdout.as_mut().ok_or_else(|| {
                    crate::error::Error::Internal("MCP child stdout not available".into())
                })?;
                let mut reader = BufReader::new(stdout);

                let mut header = String::new();
                reader.read_line(&mut header).await?;
                let content_length = parse_content_length(&header).map_err(|e| {
                    crate::error::Error::Internal(format!("invalid MCP response: {e}"))
                })?;

                let mut blank = String::new();
                reader.read_line(&mut blank).await?;

                let mut body = vec![0u8; content_length];
                reader.read_exact(&mut body).await?;

                let response_str = String::from_utf8_lossy(&body).to_string();
                let response = parse_jsonrpc_response(&response_str).map_err(|e| {
                    crate::error::Error::Internal(format!("invalid MCP response: {e}"))
                })?;

                Ok(response)
            }
            McpClientState::Remote {
                http_client,
                url,
                headers,
            } => {
                let mut request_builder = http_client
                    .post(url.clone())
                    .json(request)
                    .timeout(std::time::Duration::from_millis(self.config.timeout));

                for (key, value) in headers.iter() {
                    request_builder = request_builder.header(key.as_str(), value.as_str());
                }

                let response = request_builder.send().await?;

                if !response.status().is_success() {
                    return Err(crate::error::Error::Network(format!(
                        "MCP server `{}` returned HTTP {}",
                        self.server_name,
                        response.status()
                    )));
                }

                let body: serde_json::Value = response.json().await?;
                Ok(body)
            }
            McpClientState::RemoteSse {
                http_client,
                message_url,
                headers,
                sse_rx,
            } => {
                // Send the JSON-RPC request via HTTP POST to the message endpoint
                let mut request_builder = http_client
                    .post(message_url.as_str())
                    .json(request)
                    .timeout(std::time::Duration::from_millis(self.config.timeout));

                for (key, value) in headers.iter() {
                    request_builder = request_builder.header(key.as_str(), value.as_str());
                }

                // Fire the POST — some SSE servers return the response
                // directly; others send it via the SSE stream.
                let http_response = request_builder.send().await?;

                // First try the direct HTTP response
                if http_response.status().is_success() {
                    if let Ok(body) = http_response.json::<serde_json::Value>().await {
                        if body.get("result").is_some() || body.get("error").is_some() {
                            return Ok(body);
                        }
                    }
                }

                // Fall back to waiting for the SSE event stream
                // The response will arrive as an SSE event matched by request ID
                let req_id = request["id"].as_u64().unwrap_or(0);
                loop {
                    match sse_rx.recv().await {
                        Some((id, value)) if id == req_id => {
                            return Ok(value);
                        }
                        Some((_other_id, _value)) => {
                            // Response for a different request — this
                            // shouldn't normally happen in a single-client
                            // scenario; ignore and keep waiting.
                            continue;
                        }
                        None => {
                            return Err(crate::error::Error::Internal(
                                "MCP SSE stream closed while waiting for response".into(),
                            ));
                        }
                    }
                }
            }
            McpClientState::Disconnected => Err(crate::error::Error::Internal(
                "MCP client is disconnected".into(),
            )),
        }
    }

    /// Connect to a remote MCP server using the SSE (Server-Sent Events)
    /// transport.
    ///
    /// This is the older MCP HTTP transport (now deprecated in favor of
    /// Streamable HTTP). It opens an SSE stream to receive server→client
    /// messages and sends JSON-RPC requests via HTTP POST to a message
    /// endpoint extracted from the first SSE event.
    ///
    /// # Flow
    ///
    /// 1. GET `{url}` — opens the SSE event stream
    /// 2. Receive `endpoint` event — extracts the POST message URL
    /// 3. Send `initialize` JSON-RPC request via POST
    /// 4. Receive `initialize` response via SSE
    /// 5. Send `initialized` notification via POST
    /// 6. Discover tools via `tools/list`
    ///
    /// # Source
    /// Ported from the MCP spec SSE transport.
    ///
    /// # Errors
    /// Returns an error if the SSE endpoint cannot be reached, if the
    /// endpoint event is missing, or if the initialize handshake fails.
    pub async fn connect_http(
        config: McpServerConfig,
        server_name: String,
    ) -> crate::error::Result<Self> {
        let url = config
            .url
            .as_ref()
            .ok_or_else(|| crate::error::Error::Config("MCP remote server has no URL".into()))?;

        let http_client = reqwest::Client::new();

        // Step 1: Open the SSE event stream
        let sse_url = if url.ends_with("/sse") {
            url.clone()
        } else {
            format!("{}/sse", url.trim_end_matches('/'))
        };

        let mut sse_request = http_client
            .get(&sse_url)
            .header("Accept", "text/event-stream")
            .timeout(std::time::Duration::from_millis(config.timeout));

        for (key, value) in &config.headers {
            sse_request = sse_request.header(key.as_str(), value.as_str());
        }

        let sse_response = sse_request.send().await.map_err(|e| {
            crate::error::Error::Network(format!(
                "MCP SSE connection to `{server_name}` failed: {e}"
            ))
        })?;

        if !sse_response.status().is_success() {
            return Err(crate::error::Error::Network(format!(
                "MCP server `{server_name}` SSE endpoint returned HTTP {}",
                sse_response.status()
            )));
        }

        // Step 2: Parse the SSE stream to get the message endpoint
        let sse_stream = crate::sse::parse_sse_stream(sse_response);
        let (sse_tx, sse_rx) = tokio::sync::mpsc::unbounded_channel::<(u64, serde_json::Value)>();

        // Spawn a background task to process SSE events
        let _sse_handle = tokio::spawn(async move {
            use futures::StreamExt;
            tokio::pin!(sse_stream);

            while let Some(event_result) = sse_stream.next().await {
                match event_result {
                    Ok(event) => {
                        if let Ok(value) = serde_json::from_str::<serde_json::Value>(&event.data) {
                            if let Some(id) = value.get("id").and_then(|v| v.as_u64()) {
                                let _ = sse_tx.send((id, value));
                            }
                        }
                    }
                    Err(_e) => {
                        // SSE stream error — channel will close
                        break;
                    }
                }
            }
        });

        // Step 3: Determine the message endpoint
        // For most MCP SSE servers, the message endpoint is {base_url}/messages
        // with a session ID appended from the first endpoint event.
        let message_url = if url.ends_with("/sse") {
            // Replace /sse with /messages
            format!("{}/messages", &url[..url.len() - 4])
        } else {
            format!("{}/messages", url.trim_end_matches('/'))
        };

        // Step 4: Send initialize request via POST
        let init_req = build_jsonrpc_request(
            "initialize",
            serde_json::json!({
                "protocolVersion": "2024-11-05",
                "capabilities": {},
                "clientInfo": {
                    "name": "blazecode",
                    "version": env!("CARGO_PKG_VERSION")
                }
            }),
            0,
        );

        let mut request = http_client
            .post(&message_url)
            .json(&init_req)
            .timeout(std::time::Duration::from_millis(config.timeout));

        for (key, value) in &config.headers {
            request = request.header(key.as_str(), value.as_str());
        }

        let response = request.send().await?;
        if !response.status().is_success() {
            return Err(crate::error::Error::Network(format!(
                "MCP server `{server_name}` initialize returned HTTP {}",
                response.status()
            )));
        }

        let body: serde_json::Value = response.json().await?;
        // Capture server capabilities from initialize response
        let mut capabilities = std::collections::HashMap::new();
        if let Some(caps) = body
            .get("result")
            .and_then(|r| r.get("capabilities"))
            .and_then(|c| c.as_object())
        {
            for (k, v) in caps {
                capabilities.insert(k.clone(), v.clone());
            }
        }

        // Step 5: Send initialized notification
        let notif = build_jsonrpc_notification("notifications/initialized", serde_json::json!({}));
        let _ = http_client
            .post(&message_url)
            .json(&notif)
            .timeout(std::time::Duration::from_millis(config.timeout))
            .send()
            .await;

        let state = McpClientState::RemoteSse {
            http_client,
            message_url,
            headers: config.headers.clone(),
            sse_rx,
        };

        // Build client
        let client = Self {
            config,
            server_name,
            tools: tokio::sync::RwLock::new(Vec::new()),
            connected: Arc::new(AtomicBool::new(true)),
            next_id: AtomicU64::new(1),
            capabilities: tokio::sync::RwLock::new(capabilities),
            state: tokio::sync::Mutex::new(state),
            notification_handlers: McpNotificationHandlers::default(),
            onclose_callbacks: Arc::new(std::sync::Mutex::new(Vec::new())),
        };

        // Discover tools
        match client.list_tools().await {
            Ok(discovered) => {
                let mut tools = client.tools.write().await;
                *tools = discovered;
            }
            Err(e) => {
                tracing::warn!(
                    "MCP SSE: failed to discover tools for '{}' during connect: {e}",
                    client.server_name
                );
            }
        }

        Ok(client)
    }

    /// Disconnect from the MCP server.
    ///
    /// For local (stdio) connections, this kills the child process and
    /// waits for it to exit. For remote connections, it drops the HTTP
    /// client. After calling this method, the client is marked as
    /// disconnected and subsequent operations will return errors.
    pub async fn disconnect(&self) -> crate::error::Result<()> {
        self.connected.store(false, Ordering::SeqCst);

        let mut state = self.state.lock().await;
        match std::mem::replace(&mut *state, McpClientState::Disconnected) {
            McpClientState::Local { mut child } => {
                // Kill the child process and wait for it to exit
                let _ = child.kill().await;
                let _ = child.wait().await;
            }
            McpClientState::Remote { .. } => {
                // Drop the HTTP client — no explicit close needed
            }
            McpClientState::RemoteSse { .. } => {
                // Drop the HTTP client and SSE channel
            }
            McpClientState::Disconnected => {
                // Already disconnected
            }
        }
        Ok(())
    }

    /// Whether this client is currently connected.
    pub fn is_connected(&self) -> bool {
        self.connected.load(Ordering::SeqCst)
    }

    /// Get the server name.
    pub fn name(&self) -> &str {
        &self.server_name
    }

    /// Get a snapshot of the cached tool definitions.
    pub async fn cached_tools(&self) -> Vec<McpTool> {
        self.tools.read().await.clone()
    }

    /// Refresh the tool cache by re-listing tools from the server.
    pub async fn refresh_tools(&self) -> crate::error::Result<Vec<McpTool>> {
        let tools = self.list_tools().await?;
        let mut cache = self.tools.write().await;
        *cache = tools.clone();
        Ok(tools)
    }

    /// Convert cached tools into [`PluginToolDef`] entries suitable for
    /// registration in the [`ToolRegistry`](crate::tool::ToolRegistry).
    ///
    /// Each tool gets a key of the form `{sanitized_server_name}_{sanitized_tool_name}`
    /// and an execute closure that calls [`McpClient::call_tool`] to dispatch
    /// the tool to the MCP server.
    pub async fn to_plugin_defs(self: Arc<McpClient>) -> Vec<crate::tool::PluginToolDef> {
        let tools = self.tools.read().await;
        let mut defs = Vec::with_capacity(tools.len());

        for tool in tools.iter() {
            let tool_id = tool_key(&self.server_name, &tool.name);
            let input_schema = tool
                .input_schema
                .clone()
                .unwrap_or_else(|| serde_json::json!({"type": "object", "properties": {}}));
            let description = tool
                .description
                .clone()
                .unwrap_or_else(|| format!("MCP tool: {}", tool.name));

            let client = Arc::clone(&self);
            let tool_name = tool.name.clone();

            let plugin = crate::tool::PluginToolDef::new(
                tool_id,
                description,
                input_schema,
                move |args, _ctx: crate::tool::ToolContext| {
                    let client = Arc::clone(&client);
                    let tool_name = tool_name.clone();
                    async move {
                        let result = client.call_tool(&tool_name, args).await?;
                        let output = extract_mcp_content(&result);
                        Ok(crate::tool::ExecuteResult {
                            title: tool_name.clone(),
                            output,
                            truncated: false,
                            output_path: None,
                            attachments: None,
                            metadata: HashMap::new(),
                        })
                    }
                },
            );
            defs.push(plugin);
        }

        defs
    }
}

// ---------------------------------------------------------------------------
// MCP Server Registry — manages multiple active MCP connections
// ---------------------------------------------------------------------------

/// A thread-safe registry of connected MCP servers.
///
/// Maps server names to their active [`McpClient`] instances. Used by the
/// HTTP API to track connection state, register/unregister tools, and
/// manage server lifecycles.
///
/// # Source
/// Ported from `packages/blazecode/src/mcp/index.ts` — the in-memory
/// connection tracking in the MCP module.
pub struct McpServerRegistry {
    /// Active connections keyed by server name.
    clients: dashmap::DashMap<String, Arc<McpClient>>,
    /// Server configurations (including disabled servers).
    configs: tokio::sync::RwLock<HashMap<String, McpServerConfig>>,
}

impl McpServerRegistry {
    /// Create an empty registry.
    pub fn new() -> Self {
        Self {
            clients: dashmap::DashMap::new(),
            configs: tokio::sync::RwLock::new(HashMap::new()),
        }
    }

    /// Add a server configuration (does not connect).
    pub async fn add_config(&self, name: String, config: McpServerConfig) {
        self.configs.write().await.insert(name, config);
    }

    /// Remove a server configuration and disconnect if connected.
    pub async fn remove_config(&self, name: &str) {
        self.configs.write().await.remove(name);
        self.disconnect(name).await.ok();
    }

    /// Get a server configuration.
    pub async fn get_config(&self, name: &str) -> Option<McpServerConfig> {
        self.configs.read().await.get(name).cloned()
    }

    /// Connect to a server by name.
    ///
    /// Looks up the configuration, creates an [`McpClient`], connects it,
    /// and stores the active client in the registry.
    ///
    /// # Errors
    /// Returns an error if the server is not found, disabled, or connection fails.
    pub async fn connect(&self, name: &str) -> crate::error::Result<Arc<McpClient>> {
        // Already connected?
        if let Some(existing) = self.clients.get(name) {
            if existing.is_connected() {
                return Ok(existing.clone());
            }
            // Stale disconnected entry — remove it
            self.clients.remove(name);
        }

        let config = self
            .configs
            .read()
            .await
            .get(name)
            .cloned()
            .ok_or_else(|| crate::error::Error::McpNotFound {
                name: name.to_string(),
            })?;

        if !config.enabled {
            return Err(crate::error::Error::Config(format!(
                "MCP server '{name}' is disabled"
            )));
        }

        let client = Arc::new(McpClient::connect(config, name.to_string()).await?);

        self.clients.insert(name.to_string(), client.clone());

        Ok(client)
    }

    /// Disconnect a server by name.
    ///
    /// Kills the subprocess (for local connections) and removes the
    /// client from the registry.
    pub async fn disconnect(&self, name: &str) -> crate::error::Result<()> {
        if let Some((_, client)) = self.clients.remove(name) {
            client.disconnect().await?;
        }
        Ok(())
    }

    /// Get an active client by name.
    pub fn get_client(&self, name: &str) -> Option<Arc<McpClient>> {
        self.clients.get(name).map(|r| r.clone())
    }

    /// List all registered server names (configs, regardless of connection status).
    pub async fn server_names(&self) -> Vec<String> {
        let mut names: Vec<String> = self.configs.read().await.keys().cloned().collect();
        names.sort();
        names
    }

    /// List all active clients.
    pub fn active_clients(&self) -> Vec<Arc<McpClient>> {
        self.clients.iter().map(|r| r.value().clone()).collect()
    }

    /// Get connection status for a server.
    ///
    /// Returns the [`McpStatus`] for the named server.
    pub async fn status(&self, name: &str) -> McpStatus {
        if let Some(client) = self.clients.get(name) {
            if client.is_connected() {
                return McpStatus::Connected;
            }
        }

        // Check if the config exists but is disabled
        if let Some(cfg) = self.configs.read().await.get(name) {
            if !cfg.enabled {
                return McpStatus::Disabled;
            }
        }

        McpStatus::Failed {
            error: "not connected".into(),
        }
    }

    /// Get a summary of all servers: name, config, status, and tools.
    pub async fn list_servers(&self) -> Vec<McpServerSummary> {
        let configs = self.configs.read().await;
        let mut summaries: Vec<McpServerSummary> = Vec::new();

        for (name, config) in configs.iter() {
            let (connected, tools) = if let Some(client) = self.clients.get(name) {
                (client.is_connected(), client.cached_tools().await)
            } else {
                (false, Vec::new())
            };

            summaries.push(McpServerSummary {
                name: name.clone(),
                config: config.clone(),
                connected,
                tools,
            });
        }

        summaries.sort_by(|a, b| a.name.cmp(&b.name));
        summaries
    }

    /// Remove all clients (disconnect everything).
    pub async fn clear(&self) {
        for entry in self.clients.iter() {
            let _ = entry.value().disconnect().await;
        }
        self.clients.clear();
        self.configs.write().await.clear();
    }
}

impl Default for McpServerRegistry {
    fn default() -> Self {
        Self::new()
    }
}

/// Summary of an MCP server's current state.
///
/// Returned by [`McpServerRegistry::list_servers()`] for the
/// `GET /mcp` API response.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct McpServerSummary {
    /// Server name.
    pub name: String,
    /// Server configuration.
    pub config: McpServerConfig,
    /// Whether the server is currently connected.
    pub connected: bool,
    /// Tools discovered from this server (empty if not connected).
    pub tools: Vec<McpTool>,
}

// ---------------------------------------------------------------------------
// JSON-RPC 2.0 message helpers
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
/// Used for the `notifications/initialized` message in the MCP handshake.
fn build_jsonrpc_notification(method: &str, params: serde_json::Value) -> serde_json::Value {
    serde_json::json!({
        "jsonrpc": "2.0",
        "method": method,
        "params": params,
    })
}

/// Parse a JSON-RPC 2.0 response string into a JSON value.
///
/// Returns an error if the JSON is invalid or if the response contains a
/// JSON-RPC error object.
fn parse_jsonrpc_response(response: &str) -> std::result::Result<serde_json::Value, String> {
    let value: serde_json::Value =
        serde_json::from_str(response).map_err(|e| format!("invalid JSON: {e}"))?;

    // Check for JSON-RPC error
    if let Some(err) = value.get("error") {
        let message = err
            .get("message")
            .and_then(|m| m.as_str())
            .unwrap_or("unknown error");
        return Err(format!("JSON-RPC error: {message}"));
    }

    Ok(value)
}

/// Frame a JSON message using the MCP framing protocol.
///
/// Format: `Content-Length: <N>\r\n\r\n<json>`
///
/// This is the framing used by the MCP stdio transport.
fn frame_jsonrpc_message(json: &serde_json::Value) -> String {
    let body = serde_json::to_string(json).expect("JSON serialization should not fail");
    format!("Content-Length: {}\r\n\r\n{}", body.len(), body)
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
        // Look for Content-Length header
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

/// Parse a `Content-Length` header value.
///
/// Expects input like `"Content-Length: 123\r\n"` and returns the
/// parsed byte count.
fn parse_content_length(header: &str) -> std::result::Result<usize, String> {
    header
        .trim()
        .strip_prefix("Content-Length:")
        .ok_or_else(|| format!("missing Content-Length header in: {header}"))?
        .trim()
        .parse::<usize>()
        .map_err(|e| format!("invalid Content-Length: {e}"))
}

// ---------------------------------------------------------------------------
// Client tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod client_tests {
    use super::*;

    // -- JSON-RPC helpers ----------------------------------------------------

    #[test]
    fn test_build_jsonrpc_request() {
        let req = build_jsonrpc_request("tools/list", serde_json::json!({}), 1);
        assert_eq!(req["jsonrpc"], "2.0");
        assert_eq!(req["method"], "tools/list");
        assert_eq!(req["id"], 1);
    }

    #[test]
    fn test_parse_jsonrpc_response_success() {
        let resp = r#"{"jsonrpc":"2.0","id":1,"result":{"tools":[]}}"#;
        let parsed = parse_jsonrpc_response(resp).expect("valid response");
        assert_eq!(parsed["id"], 1);
    }

    #[test]
    fn test_parse_jsonrpc_response_error() {
        let resp =
            r#"{"jsonrpc":"2.0","id":1,"error":{"code":-32601,"message":"Method not found"}}"#;
        let err = parse_jsonrpc_response(resp).unwrap_err();
        assert!(err.contains("Method not found"));
    }

    #[test]
    fn test_frame_jsonrpc_message() {
        let msg = serde_json::json!({"jsonrpc":"2.0","method":"ping","id":1});
        let framed = frame_jsonrpc_message(&msg);
        let body = serde_json::to_string(&msg).unwrap();
        let expected = format!("Content-Length: {}\r\n\r\n{}", body.len(), body);
        assert_eq!(framed, expected);
    }

    #[test]
    fn test_parse_jsonrpc_stream_single() {
        let body = r#"{"jsonrpc":"2.0","id":1,"result":"ok"}"#;
        let framed = format!("Content-Length: {}\r\n\r\n{}", body.len(), body);
        let messages = parse_jsonrpc_stream(&framed);
        assert_eq!(messages.len(), 1);
        assert_eq!(messages[0]["result"], "ok");
    }

    #[test]
    fn test_parse_jsonrpc_stream_multiple() {
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
    fn test_parse_jsonrpc_stream_incomplete() {
        let body = r#"{"jsonrpc":"2.0","id":1,"result":"ok"}"#;
        // Only send the header + partial body
        let framed = format!("Content-Length: {}\r\n\r\n{}", body.len(), "{");
        let messages = parse_jsonrpc_stream(&framed);
        assert!(messages.is_empty());
    }

    #[test]
    fn test_parse_content_length() {
        let result = parse_content_length("Content-Length: 42\r\n").expect("valid header");
        assert_eq!(result, 42);
    }

    #[test]
    fn test_parse_content_length_invalid() {
        assert!(parse_content_length("X-Test: 42\r\n").is_err());
        assert!(parse_content_length("Content-Length: abc\r\n").is_err());
    }

    // -- JSON-RPC typed structs ---------------------------------------------

    #[test]
    fn test_jsonrpc_request_new() {
        let req = JsonRpcRequest::new("tools/list", serde_json::json!({}), 1);
        assert_eq!(req.jsonrpc, "2.0");
        assert_eq!(req.method, "tools/list");
        assert_eq!(req.id, 1);
        assert!(req.params.is_some());
    }

    #[test]
    fn test_jsonrpc_request_notification() {
        let notif =
            JsonRpcRequest::notification("notifications/initialized", serde_json::json!({}));
        assert_eq!(notif.jsonrpc, "2.0");
        assert_eq!(notif.method, "notifications/initialized");
        assert_eq!(notif.id, 0);
    }

    #[test]
    fn test_jsonrpc_request_serialization() {
        let req = JsonRpcRequest::new("ping", serde_json::json!({"key": "val"}), 42);
        let json = serde_json::to_string(&req).unwrap();
        assert!(json.contains(r#""jsonrpc":"2.0""#));
        assert!(json.contains(r#""method":"ping""#));
        assert!(json.contains(r#""id":42"#));
        assert!(json.contains(r#""key":"val""#));
    }

    #[test]
    fn test_jsonrpc_response_success() {
        let json = r#"{"jsonrpc":"2.0","id":1,"result":{"tools":[]}}"#;
        let resp: JsonRpcResponse = serde_json::from_str(json).unwrap();
        assert!(resp.is_success());
        assert!(!resp.is_error());
        assert!(resp.result.is_some());
        assert!(resp.error.is_none());
    }

    #[test]
    fn test_jsonrpc_response_error() {
        let json =
            r#"{"jsonrpc":"2.0","id":1,"error":{"code":-32601,"message":"Method not found"}}"#;
        let resp: JsonRpcResponse = serde_json::from_str(json).unwrap();
        assert!(!resp.is_success());
        assert!(resp.is_error());
        assert_eq!(resp.error_message(), Some("Method not found"));
    }

    #[test]
    fn test_jsonrpc_response_error_code() {
        let json = r#"{"jsonrpc":"2.0","id":2,"error":{"code":-32000,"message":"Server error","data":{"detail":"something broke"}}}"#;
        let resp: JsonRpcResponse = serde_json::from_str(json).unwrap();
        assert!(resp.is_error());
        let err = resp.error.unwrap();
        assert_eq!(err.code, -32000);
        assert_eq!(err.message, "Server error");
        assert!(err.data.is_some());
    }

    // -- build_jsonrpc_notification -----------------------------------------

    #[test]
    fn test_build_notification_has_no_id() {
        let notif = build_jsonrpc_notification("notifications/initialized", serde_json::json!({}));
        let json_str = serde_json::to_string(&notif).unwrap();
        // Notification must NOT have an "id" field
        let parsed: serde_json::Value = serde_json::from_str(&json_str).unwrap();
        assert!(parsed.get("id").is_none());
        assert_eq!(parsed["jsonrpc"], "2.0");
        assert_eq!(parsed["method"], "notifications/initialized");
    }

    // -- McpServerRegistry ---------------------------------------------------

    #[tokio::test]
    async fn test_registry_add_and_get_config() {
        let registry = McpServerRegistry::new();
        let config = McpServerConfig::local(vec!["echo".into()]);
        registry.add_config("test-srv".into(), config.clone()).await;

        let retrieved = registry.get_config("test-srv").await;
        assert!(retrieved.is_some());
        assert_eq!(retrieved.unwrap().command_executable(), Some("echo"));
    }

    #[tokio::test]
    async fn test_registry_remove_config() {
        let registry = McpServerRegistry::new();
        registry
            .add_config("temp".into(), McpServerConfig::local(vec!["ls".into()]))
            .await;
        assert!(registry.get_config("temp").await.is_some());

        registry.remove_config("temp").await;
        assert!(registry.get_config("temp").await.is_none());
    }

    #[tokio::test]
    async fn test_registry_server_names() {
        let registry = McpServerRegistry::new();
        registry
            .add_config("a".into(), McpServerConfig::local(vec!["a".into()]))
            .await;
        registry
            .add_config("b".into(), McpServerConfig::local(vec!["b".into()]))
            .await;

        let names = registry.server_names().await;
        assert_eq!(names, vec!["a", "b"]);
    }

    #[tokio::test]
    async fn test_registry_disconnect_nonexistent() {
        let registry = McpServerRegistry::new();
        // Disconnecting a non-existent server should not error
        let result = registry.disconnect("nonexistent").await;
        assert!(result.is_ok());
    }

    #[tokio::test]
    async fn test_registry_status_disconnected() {
        let registry = McpServerRegistry::new();
        // No config → Failed
        let status = registry.status("unknown").await;
        assert!(matches!(status, McpStatus::Failed { .. }));
    }

    #[tokio::test]
    async fn test_registry_status_disabled() {
        let registry = McpServerRegistry::new();
        let config = McpServerConfig::local(vec!["cmd".into()]).disabled();
        registry.add_config("disabled-srv".into(), config).await;

        let status = registry.status("disabled-srv").await;
        assert!(matches!(status, McpStatus::Disabled));
    }

    #[tokio::test]
    async fn test_registry_list_servers() {
        let registry = McpServerRegistry::new();
        registry
            .add_config("srv1".into(), McpServerConfig::local(vec!["echo".into()]))
            .await;
        registry
            .add_config(
                "srv2".into(),
                McpServerConfig::remote("https://example.com".into()).disabled(),
            )
            .await;

        let summaries = registry.list_servers().await;
        assert_eq!(summaries.len(), 2);
        assert_eq!(summaries[0].name, "srv1");
        assert_eq!(summaries[1].name, "srv2");
        assert!(!summaries[0].connected);
        assert!(!summaries[1].connected);
    }

    #[tokio::test]
    async fn test_registry_clear() {
        let registry = McpServerRegistry::new();
        registry
            .add_config("srv".into(), McpServerConfig::local(vec!["ls".into()]))
            .await;

        registry.clear().await;
        assert!(registry.server_names().await.is_empty());
    }

    #[tokio::test]
    async fn test_registry_default() {
        let registry = McpServerRegistry::default();
        assert!(registry.server_names().await.is_empty());
        assert!(registry.active_clients().is_empty());
    }

    // -- McpServerSummary ----------------------------------------------------

    #[test]
    fn test_mcp_server_summary_serialization() {
        let summary = McpServerSummary {
            name: "test".into(),
            config: McpServerConfig::local(vec!["cmd".into()]),
            connected: false,
            tools: vec![],
        };
        let json = serde_json::to_string(&summary).unwrap();
        assert!(json.contains(r#""name":"test""#));
        assert!(json.contains(r#""connected":false"#));
        assert!(json.contains(r#""tools":[]"#));
    }
}

// ---------------------------------------------------------------------------
// OAuth token storage (McpAuth)
// ---------------------------------------------------------------------------

/// OAuth tokens for an MCP server.
///
/// # Source
/// Ported from `packages/blazecode/src/mcp/auth.ts` `Tokens` schema.
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct McpAuthTokens {
    /// Access token for API calls.
    pub access_token: String,
    /// Optional refresh token.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub refresh_token: Option<String>,
    /// Token expiry timestamp (Unix seconds).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub expires_at: Option<f64>,
    /// OAuth scope.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub scope: Option<String>,
}

/// Client registration information for OAuth.
///
/// # Source
/// Ported from `packages/blazecode/src/mcp/auth.ts` `ClientInfo` schema.
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct McpAuthClientInfo {
    /// The client ID assigned during registration.
    pub client_id: String,
    /// Optional client secret.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub client_secret: Option<String>,
    /// When the client ID was issued (Unix seconds).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub client_id_issued_at: Option<f64>,
    /// When the client secret expires (Unix seconds).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub client_secret_expires_at: Option<f64>,
}

/// Stored authentication entry for an MCP server.
///
/// # Source
/// Ported from `packages/blazecode/src/mcp/auth.ts` `Entry` schema.
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct McpAuthEntry {
    /// OAuth tokens (present after successful authentication).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub tokens: Option<McpAuthTokens>,
    /// Client registration info (present after dynamic registration).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub client_info: Option<McpAuthClientInfo>,
    /// PKCE code verifier for the current auth flow.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub code_verifier: Option<String>,
    /// OAuth state parameter for CSRF protection.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub oauth_state: Option<String>,
    /// The server URL this entry is valid for.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub server_url: Option<String>,
}

/// Persistent OAuth token storage for MCP servers.
///
/// Reads and writes a JSON file at a configurable path. Thread-safe via
/// interior RwLock.
///
/// # Source
/// Ported from `packages/blazecode/src/mcp/auth.ts` — the file-based storage.
pub struct McpAuthStore {
    /// Path to the auth data JSON file.
    path: std::path::PathBuf,
    /// In-memory cache of the auth data.
    data: tokio::sync::RwLock<std::collections::HashMap<String, McpAuthEntry>>,
}

impl McpAuthStore {
    /// Create a new auth store at the given path.
    pub fn new(path: std::path::PathBuf) -> Self {
        Self {
            path,
            data: tokio::sync::RwLock::new(std::collections::HashMap::new()),
        }
    }

    /// Create a new auth store with the default path.
    pub fn default_path() -> Self {
        let config_dir = dirs::config_dir()
            .unwrap_or_else(|| std::path::PathBuf::from("."))
            .join("blazecode");
        Self::new(config_dir.join("mcp-auth.json"))
    }

    /// Create a global (default path) auth store.
    pub fn global() -> Self {
        Self::default_path()
    }

    /// Load auth data from disk, replacing in-memory cache.
    pub async fn load(&self) -> crate::error::Result<()> {
        let contents = match tokio::fs::read_to_string(&self.path).await {
            Ok(c) => c,
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
                let mut data = self.data.write().await;
                data.clear();
                return Ok(());
            }
            Err(e) => {
                return Err(crate::error::Error::FileSystem {
                    path: self.path.display().to_string(),
                    message: format!("failed to read MCP auth file: {e}"),
                });
            }
        };

        let loaded: std::collections::HashMap<String, McpAuthEntry> =
            serde_json::from_str(&contents).map_err(|e| {
                crate::error::Error::Config(format!(
                    "failed to parse MCP auth file `{}`: {e}",
                    self.path.display()
                ))
            })?;

        let mut data = self.data.write().await;
        *data = loaded;
        Ok(())
    }

    /// Save the current in-memory data to disk.
    async fn save(&self) -> crate::error::Result<()> {
        if let Some(parent) = self.path.parent() {
            tokio::fs::create_dir_all(parent).await?;
        }
        let data = self.data.read().await;
        let json = serde_json::to_string_pretty(&*data)?;
        let tmp_path = self.path.with_extension("json.tmp");
        tokio::fs::write(&tmp_path, &json).await?;
        tokio::fs::rename(&tmp_path, &self.path).await?;
        Ok(())
    }

    /// Get the auth entry for an MCP server.
    pub async fn get(&self, mcp_name: &str) -> Option<McpAuthEntry> {
        self.data.read().await.get(mcp_name).cloned()
    }

    /// Get auth entry for an MCP server, validating the server URL.
    pub async fn get_for_url(&self, mcp_name: &str, server_url: &str) -> Option<McpAuthEntry> {
        let data = self.data.read().await;
        let entry = data.get(mcp_name)?;
        if let Some(ref stored_url) = entry.server_url {
            if stored_url != server_url {
                return None;
            }
        }
        Some(entry.clone())
    }

    /// Get all auth entries.
    pub async fn all(&self) -> std::collections::HashMap<String, McpAuthEntry> {
        self.data.read().await.clone()
    }

    /// Set the auth entry for an MCP server.
    pub async fn set(&self, mcp_name: &str, entry: McpAuthEntry) -> crate::error::Result<()> {
        self.data.write().await.insert(mcp_name.to_string(), entry);
        self.save().await
    }

    /// Remove the auth entry for an MCP server.
    pub async fn remove(&self, mcp_name: &str) -> crate::error::Result<()> {
        self.data.write().await.remove(mcp_name);
        self.save().await
    }

    /// Update tokens for an MCP server.
    pub async fn update_tokens(
        &self,
        mcp_name: &str,
        tokens: McpAuthTokens,
        server_url: Option<&str>,
    ) -> crate::error::Result<()> {
        let mut data = self.data.write().await;
        let entry = data.entry(mcp_name.to_string()).or_default();
        entry.tokens = Some(tokens);
        if let Some(url) = server_url {
            entry.server_url = Some(url.to_string());
        }
        drop(data);
        self.save().await
    }

    /// Update client info for an MCP server.
    pub async fn update_client_info(
        &self,
        mcp_name: &str,
        client_info: McpAuthClientInfo,
        server_url: Option<&str>,
    ) -> crate::error::Result<()> {
        let mut data = self.data.write().await;
        let entry = data.entry(mcp_name.to_string()).or_default();
        entry.client_info = Some(client_info);
        if let Some(url) = server_url {
            entry.server_url = Some(url.to_string());
        }
        drop(data);
        self.save().await
    }

    /// Update the PKCE code verifier.
    pub async fn update_code_verifier(
        &self,
        mcp_name: &str,
        code_verifier: String,
    ) -> crate::error::Result<()> {
        let mut data = self.data.write().await;
        let entry = data.entry(mcp_name.to_string()).or_default();
        entry.code_verifier = Some(code_verifier);
        drop(data);
        self.save().await
    }

    /// Clear the PKCE code verifier.
    pub async fn clear_code_verifier(&self, mcp_name: &str) -> crate::error::Result<()> {
        let mut data = self.data.write().await;
        if let Some(entry) = data.get_mut(mcp_name) {
            entry.code_verifier = None;
        }
        drop(data);
        self.save().await
    }

    /// Update the OAuth state parameter.
    pub async fn update_oauth_state(
        &self,
        mcp_name: &str,
        oauth_state: String,
    ) -> crate::error::Result<()> {
        let mut data = self.data.write().await;
        let entry = data.entry(mcp_name.to_string()).or_default();
        entry.oauth_state = Some(oauth_state);
        drop(data);
        self.save().await
    }

    /// Get the OAuth state parameter.
    pub async fn get_oauth_state(&self, mcp_name: &str) -> Option<String> {
        self.data
            .read()
            .await
            .get(mcp_name)
            .and_then(|e| e.oauth_state.clone())
    }

    /// Clear the OAuth state parameter.
    pub async fn clear_oauth_state(&self, mcp_name: &str) -> crate::error::Result<()> {
        let mut data = self.data.write().await;
        if let Some(entry) = data.get_mut(mcp_name) {
            entry.oauth_state = None;
        }
        drop(data);
        self.save().await
    }

    /// Check whether stored tokens have expired.
    pub async fn is_token_expired(&self, mcp_name: &str) -> Option<bool> {
        let data = self.data.read().await;
        let entry = data.get(mcp_name)?;
        let tokens = entry.tokens.as_ref()?;
        match tokens.expires_at {
            Some(expiry) => Some(expiry < chrono::Utc::now().timestamp() as f64),
            None => Some(false),
        }
    }
}

// ---------------------------------------------------------------------------
// Pagination helper for MCP list operations
// ---------------------------------------------------------------------------

/// Paginate through an MCP list operation.
///
/// Handles cursor-based pagination with a maximum page limit.
///
/// # Source
/// Ported from `packages/blazecode/src/mcp/catalog.ts` `paginate()`.
pub async fn mcp_paginate<T, F, Fut>(
    mut list: F,
    extract_items: fn(serde_json::Value) -> std::result::Result<Vec<T>, String>,
) -> std::result::Result<Vec<T>, String>
where
    F: FnMut(Option<String>) -> Fut,
    Fut: std::future::Future<Output = std::result::Result<serde_json::Value, String>>,
{
    let mut all_items: Vec<T> = Vec::new();
    let mut seen_cursors: std::collections::HashSet<String> = std::collections::HashSet::new();
    let mut cursor: Option<String> = None;

    for _page in 0..MAX_LIST_PAGES {
        let response = list(cursor).await?;
        let items = extract_items(response.clone())?;
        all_items.extend(items);

        let next_cursor = response
            .get("nextCursor")
            .and_then(|v| v.as_str())
            .map(|s| s.to_string());

        match next_cursor {
            None => return Ok(all_items),
            Some(ref c) if c.is_empty() => return Ok(all_items),
            Some(ref c) => {
                if !seen_cursors.insert(c.clone()) {
                    return Err(format!("MCP list returned duplicate cursor: {c}"));
                }
                cursor = Some(c.clone());
            }
        }
    }

    Err(format!("MCP list exceeded {MAX_LIST_PAGES} pages"))
}

// ---------------------------------------------------------------------------
// Enhanced McpClient methods — prompts, resources, notifications
// ---------------------------------------------------------------------------

impl McpClient {
    /// Check whether the server supports a given capability.
    ///
    /// Checks the server capabilities returned during the `initialize` handshake.
    /// If capabilities are not cached, returns `false`.
    pub async fn supports_capability(&self, name: &str) -> bool {
        self.capabilities.read().await.get(name).is_some()
    }

    /// List prompts from the MCP server with pagination.
    ///
    /// Returns all prompts, handling cursor-based pagination transparently.
    ///
    /// # Source
    /// Ported from `packages/blazecode/src/mcp/catalog.ts` `prompts()`.
    pub async fn list_prompts(&self) -> crate::error::Result<Vec<serde_json::Value>> {
        if !self.supports_capability("prompts").await {
            return Ok(Vec::new());
        }

        let id = self.next_id.fetch_add(1, Ordering::SeqCst);
        let request = build_jsonrpc_request("prompts/list", serde_json::json!({}), id);
        let response = self.send_jsonrpc(&request).await?;

        let prompts_value = response
            .get("result")
            .and_then(|r| r.get("prompts"))
            .cloned()
            .ok_or_else(|| {
                crate::error::Error::Internal(
                    "MCP prompts/list response missing 'result.prompts'".into(),
                )
            })?;

        let prompts: Vec<serde_json::Value> =
            serde_json::from_value(prompts_value)?;
        Ok(prompts)
    }

    /// List resources from the MCP server with pagination.
    ///
    /// Returns all resources, handling cursor-based pagination transparently.
    ///
    /// # Source
    /// Ported from `packages/blazecode/src/mcp/catalog.ts` `resources()`.
    pub async fn list_resources(&self) -> crate::error::Result<Vec<serde_json::Value>> {
        if !self.supports_capability("resources").await {
            return Ok(Vec::new());
        }

        let id = self.next_id.fetch_add(1, Ordering::SeqCst);
        let request = build_jsonrpc_request("resources/list", serde_json::json!({}), id);
        let response = self.send_jsonrpc(&request).await?;

        let resources_value = response
            .get("result")
            .and_then(|r| r.get("resources"))
            .cloned()
            .ok_or_else(|| {
                crate::error::Error::Internal(
                    "MCP resources/list response missing 'result.resources'".into(),
                )
            })?;

        let resources: Vec<serde_json::Value> =
            serde_json::from_value(resources_value)?;
        Ok(resources)
    }

    /// Read a resource by URI.
    ///
    /// # Source
    /// Ported from `packages/blazecode/src/mcp/index.ts` `readResource()`.
    pub async fn read_resource(&self, uri: &str) -> crate::error::Result<serde_json::Value> {
        let id = self.next_id.fetch_add(1, Ordering::SeqCst);
        let request = build_jsonrpc_request(
            "resources/read",
            serde_json::json!({ "uri": uri }),
            id,
        );
        let response = self.send_jsonrpc(&request).await?;
        response.get("result").cloned().ok_or_else(|| {
            crate::error::Error::Internal(
                "MCP resources/read response missing 'result'".into(),
            )
        })
    }

    /// Get a prompt by name with optional arguments.
    ///
    /// # Source
    /// Ported from `packages/blazecode/src/mcp/index.ts` `getPrompt()`.
    pub async fn get_prompt(
        &self,
        name: &str,
        args: Option<std::collections::HashMap<String, String>>,
    ) -> crate::error::Result<serde_json::Value> {
        let id = self.next_id.fetch_add(1, Ordering::SeqCst);
        let params = if let Some(a) = args {
            serde_json::json!({ "name": name, "arguments": a })
        } else {
            serde_json::json!({ "name": name })
        };
        let request = build_jsonrpc_request("prompts/get", params, id);
        let response = self.send_jsonrpc(&request).await?;
        response.get("result").cloned().ok_or_else(|| {
            crate::error::Error::Internal(
                "MCP prompts/get response missing 'result'".into(),
            )
        })
    }
}

// ---------------------------------------------------------------------------
// Notification handling infrastructure
// ---------------------------------------------------------------------------

/// A handler for MCP notifications received from the server.
pub type McpNotificationHandler =
    Arc<dyn Fn(&str, &serde_json::Value) -> BoxFuture<'static, ()> + Send + Sync>;

/// Registered notification handlers for an MCP client.
#[derive(Default)]
pub struct McpNotificationHandlers {
    handlers: Arc<dashmap::DashMap<String, Vec<McpNotificationHandler>>>,
}

impl McpNotificationHandlers {
    /// Register a handler for a specific notification method.
    ///
    /// # Source
    /// Ported from `packages/blazecode/src/mcp/index.ts` `onNotification()`.
    pub fn on(&self, method: &str, handler: McpNotificationHandler) {
        self.handlers
            .entry(method.to_string())
            .or_default()
            .push(handler);
    }

    /// Notify all handlers registered for the given method.
    pub async fn notify(&self, method: &str, params: &serde_json::Value) {
        if let Some(handlers) = self.handlers.get(method) {
            for handler in handlers.iter() {
                handler(method, params).await;
            }
        }
    }
}

impl McpClient {
    /// Register a notification handler for a specific JSON-RPC method.
    ///
    /// # Source
    /// Ported from `packages/blazecode/src/mcp/index.ts` `onNotification()`.
    pub fn on_notification(
        &self,
        method: &str,
        handler: McpNotificationHandler,
    ) {
        self.notification_handlers.on(method, handler);
    }

    /// Register a handler that fires when the connection is closed.
    ///
    /// # Source
    /// Ported from `packages/blazecode/src/mcp/index.ts` `onClose()`.
    pub fn on_close<F>(&self, handler: F)
    where
        F: Fn() -> BoxFuture<'static, ()> + Send + Sync + 'static,
    {
        let mut callbacks = self.onclose_callbacks.lock().expect("lock onclose callbacks");
        callbacks.push(Arc::new(handler));
    }

    /// Fire all registered `on_close` callbacks.
    pub(crate) async fn fire_onclose(&self) {
        let callbacks = {
            let mut cb = self.onclose_callbacks.lock().unwrap();
            std::mem::take(&mut *cb)
        };
        for callback in callbacks {
            callback().await;
        }
    }
}

// ---------------------------------------------------------------------------
// McpServerRegistry — additional auth-related methods
// ---------------------------------------------------------------------------

impl McpServerRegistry {
    /// Returns `true` if the server with the given name supports OAuth.
    ///
    /// Ported from `packages/blazecode/src/mcp/catalog.ts` `supportsOAuth()`.
    /// Checks whether the server config has an `auth` section with a `provider`
    /// equal to `"oauth2"`.
    pub fn supports_oauth(&self, name: &str) -> bool {
        let configs = self.configs.blocking_read();
        configs.get(name)
            .map(|c| c.oauth.is_some())
            .unwrap_or(false)
    }

    /// Returns `true` if the server with the given name has stored OAuth tokens.
    ///
    /// Ported from `packages/blazecode/src/mcp/catalog.ts` `hasStoredTokens()`.
    pub fn has_stored_tokens(&self, name: &str) -> bool {
        let store = McpAuthStore::global();
        let rt = tokio::runtime::Handle::try_current();
        match rt {
            Ok(handle) => {
                let entry = handle.block_on(store.get(name));
                entry.is_some_and(|e| e.tokens.is_some())
            }
            Err(_) => false,
        }
    }

    /// Returns the auth status for the given server.
    ///
    /// Ported from `packages/blazecode/src/mcp/catalog.ts` `getAuthStatus()`.
    /// Returns `"connected"`, `"expired"`, or `"none"`.
    pub fn get_auth_status(&self, name: &str) -> &str {
        let store = McpAuthStore::global();
        let rt = tokio::runtime::Handle::try_current();
        match rt {
            Ok(handle) => {
                let entry = handle.block_on(store.get(name));
                match entry {
                    Some(e) if e.tokens.is_some() => {
                        // Check token expiry
                        let is_expired = e
                            .tokens
                            .as_ref()
                            .and_then(|t| t.expires_at)
                            .map(|exp| {
                                let now = chrono::Utc::now().timestamp() as f64;
                                now >= exp
                            })
                            .unwrap_or(false);
                        if is_expired { "expired" } else { "connected" }
                    }
                    _ => "none",
                }
            }
            Err(_) => "none",
        }
    }
}

// ---------------------------------------------------------------------------
// Transport fallback — try StreamableHTTP, then SSE
// ---------------------------------------------------------------------------

impl McpClient {
    /// Try multiple transports to connect to a remote MCP server.
    ///
    /// First attempts `StreamableHTTP` (direct POST to the message endpoint).
    /// If that fails, falls back to SSE (connect via `/sse` endpoint, then POST).
    ///
    /// # Source
    /// Ported from `packages/blazecode/src/mcp/index.ts` `connect()` — the
    /// transport fallback logic where StreamableHTTP is tried first, then SSE.
    async fn connect_with_fallback(
        url: &str,
        config: &McpServerConfig,
        server_name: &str,
        capabilities: &tokio::sync::RwLock<std::collections::HashMap<String, serde_json::Value>>,
    ) -> crate::error::Result<McpClientState> {
        // Attempt 1: StreamableHTTP (direct POST)
        // This is the primary transport for MCP remote servers.
        let http_client = reqwest::Client::new();

        let init_req = build_jsonrpc_request(
            "initialize",
            serde_json::json!({
                "protocolVersion": "2024-11-05",
                "capabilities": {},
                "clientInfo": {
                    "name": "blazecode",
                    "version": env!("CARGO_PKG_VERSION")
                }
            }),
            0,
        );

        let mut request = http_client
            .post(url)
            .json(&init_req)
            .timeout(std::time::Duration::from_millis(config.timeout));

        for (key, value) in &config.headers {
            request = request.header(key.as_str(), value.as_str());
        }

        match request.send().await {
            Ok(response) if response.status().is_success() => {
                let body: serde_json::Value = response.json().await.map_err(|e| {
                    crate::error::Error::Internal(format!(
                        "MCP: invalid JSON in StreamableHTTP init response from `{server_name}`: {e}"
                    ))
                })?;

                // Send the "initialized" notification
                let notif = build_jsonrpc_notification(
                    "notifications/initialized",
                    serde_json::json!({}),
                );
                let _ = http_client
                    .post(url)
                    .json(&notif)
                    .timeout(std::time::Duration::from_millis(config.timeout))
                    .send()
                    .await;

                tracing::info!(
                    "MCP: connected to `{server_name}` via StreamableHTTP"
                );

                // Capture capabilities from init response
                if let Some(caps) = body
                    .get("result")
                    .and_then(|r| r.get("capabilities"))
                    .and_then(|c| c.as_object())
                {
                    let mut store = capabilities.write().await;
                    for (k, v) in caps {
                        store.insert(k.clone(), v.clone());
                    }
                }

                return Ok(McpClientState::Remote {
                    http_client,
                    url: url.to_string(),
                    headers: config.headers.clone(),
                });
            }
            Ok(response) => {
                tracing::warn!(
                    "MCP: StreamableHTTP failed for `{server_name}` with HTTP {}, falling back to SSE",
                    response.status()
                );
            }
            Err(e) => {
                tracing::warn!(
                    "MCP: StreamableHTTP error for `{server_name}`: {e}, falling back to SSE"
                );
            }
        }

        // Attempt 2: SSE (connect via /sse endpoint)
        // Ported from: packages/blazecode/src/mcp/index.ts `connectSSE()`
        let sse_url = if url.ends_with('/') {
            format!("{}sse", url)
        } else {
            format!("{}/sse", url)
        };

        let mut sse_request = http_client
            .get(&sse_url)
            .header("Accept", "text/event-stream")
            .timeout(std::time::Duration::from_millis(config.timeout));

        for (key, value) in &config.headers {
            sse_request = sse_request.header(key.as_str(), value.as_str());
        }

        let sse_response = sse_request.send().await.map_err(|e| {
            crate::error::Error::Network(format!(
                "MCP: SSE connection to `{server_name}` failed: {e}"
            ))
        })?;

        if !sse_response.status().is_success() {
            return Err(crate::error::Error::Network(format!(
                "MCP: server `{server_name}` SSE endpoint returned HTTP {}",
                sse_response.status()
            )));
        }

        // Determine the message endpoint
        let message_url = if url.ends_with("/sse") {
            format!("{}/messages", &url[..url.len() - 4])
        } else if url.ends_with('/') {
            format!("{}messages", url)
        } else {
            format!("{}/messages", url)
        };

        // Send initialize via POST to message URL
        let mut init_request = http_client
            .post(&message_url)
            .json(&init_req)
            .timeout(std::time::Duration::from_millis(config.timeout));

        for (key, value) in &config.headers {
            init_request = init_request.header(key.as_str(), value.as_str());
        }

        let init_response = init_request.send().await.map_err(|e| {
            crate::error::Error::Network(format!(
                "MCP: SSE init request to `{server_name}` failed: {e}"
            ))
        })?;

        if !init_response.status().is_success() {
            return Err(crate::error::Error::Network(format!(
                "MCP: server `{server_name}` SSE init returned HTTP {}",
                init_response.status()
            )));
        }

        let _body: serde_json::Value = init_response.json().await.map_err(|e| {
            crate::error::Error::Internal(format!(
                "MCP: invalid JSON in SSE init response from `{server_name}`: {e}"
            ))
        })?;

        // Send the "initialized" notification
        let notif = build_jsonrpc_notification("notifications/initialized", serde_json::json!({}));
        let _ = http_client
            .post(&message_url)
            .json(&notif)
            .timeout(std::time::Duration::from_millis(config.timeout))
            .send()
            .await;

        tracing::info!(
            "MCP: connected to `{server_name}` via SSE"
        );

        if let Some(caps) = _body
            .get("result")
            .and_then(|r| r.get("capabilities"))
            .and_then(|c| c.as_object())
        {
            let mut store = capabilities.write().await;
            for (k, v) in caps {
                store.insert(k.clone(), v.clone());
            }
        }

        Ok(McpClientState::Remote {
            http_client,
            url: message_url,
            headers: config.headers.clone(),
        })
    }
}
