//! MCP (Model Context Protocol) integration — core types and configuration.
//!
//! Ported from:
//! - `packages/opencode/src/mcp/index.ts`
//! - `packages/opencode/src/mcp/catalog.ts`
//! - `packages/opencode/src/mcp/auth.ts`
//! - `packages/opencode/src/mcp/oauth-provider.ts`
//!
//! This module defines the core MCP types used across the system. The actual
//! MCP transport and protocol implementation (stdio/SSE/StreamableHTTP client,
//! OAuth callback server, tool execution) lives in the `rustcode-mcp` crate.
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
use thiserror::Error;

// ---------------------------------------------------------------------------
// Server type
// ---------------------------------------------------------------------------

/// The transport type for an MCP server.
///
/// # Source
/// Ported from `packages/opencode/src/mcp/index.ts` —
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
// OAuth configuration
// ---------------------------------------------------------------------------

/// OAuth configuration for a remote MCP server.
///
/// # Source
/// Ported from `packages/opencode/src/mcp/oauth-provider.ts`
/// `McpOAuthConfig` interface.
#[derive(Debug, Clone, Serialize, Deserialize)]
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
/// Ported from `packages/opencode/src/mcp/index.ts` —
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
    #[serde(default = "default_timeout", skip_serializing_if = "is_default_timeout")]
    pub timeout: u64,
    /// Whether this server is enabled.
    #[serde(default = "default_enabled")]
    pub enabled: bool,
    /// OAuth configuration for remote servers. Set to `null`/`None` to
    /// disable OAuth (auto-detection is used when unspecified).
    #[serde(default, skip_serializing_if = "Option::is_none", deserialize_with = "deserialize_oauth")]
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

impl Default for McpOAuthConfig {
    fn default() -> Self {
        Self {
            client_id: None,
            client_secret: None,
            scope: None,
            callback_port: None,
            redirect_uri: None,
        }
    }
}

impl McpServerConfig {
    /// Create a new local (stdio) MCP server config.
    ///
    /// # Source
    /// Ported from `packages/opencode/src/mcp/index.ts` `connectLocal()`.
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
    /// Ported from `packages/opencode/src/mcp/index.ts` `connectRemote()`.
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
/// Ported from `packages/opencode/src/mcp/index.ts` — `Tool as MCPToolDef`
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
/// Ported from `packages/opencode/src/mcp/catalog.ts` `sanitize()` and
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
/// Ported from `packages/opencode/src/mcp/catalog.ts` `sanitize()`.
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

// ---------------------------------------------------------------------------
// Resource definition
// ---------------------------------------------------------------------------

/// A resource discovered from an MCP server.
///
/// # Source
/// Ported from `packages/opencode/src/mcp/index.ts` `Resource` type (lines 53–59).
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
/// Ported from `packages/opencode/src/mcp/index.ts` — `PromptInfo` type.
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
/// Ported from `packages/opencode/src/mcp/index.ts` `Status` union type
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
/// Ported from `packages/opencode/src/mcp/index.ts` `AuthStatus` type (line 939).
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
/// Ported from `packages/opencode/src/mcp/index.ts` `ToolsChanged` (lines 62–67)
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
/// Ported from `packages/opencode/src/mcp/index.ts` `NotFoundError`
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
/// Ported from `packages/opencode/src/mcp/index.ts` `Failed` error
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
/// Ported from `packages/opencode/src/mcp/oauth-callback.ts` and
/// `packages/opencode/src/mcp/index.ts` OAuth flow errors.
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
/// Ported from `packages/opencode/src/mcp/index.ts` `DEFAULT_TIMEOUT` (line 39).
pub const DEFAULT_MCP_TIMEOUT_MS: u64 = 30_000;

/// Default OAuth callback port.
///
/// # Source
/// Ported from `packages/opencode/src/mcp/oauth-provider.ts` `OAUTH_CALLBACK_PORT`.
pub const OAUTH_CALLBACK_PORT: u16 = 19876;

/// OAuth callback URL path.
///
/// # Source
/// Ported from `packages/opencode/src/mcp/oauth-provider.ts` `OAUTH_CALLBACK_PATH`.
pub const OAUTH_CALLBACK_PATH: &str = "/mcp/oauth/callback";

/// Maximum pages when paginating through MCP server results.
///
/// # Source
/// Ported from `packages/opencode/src/mcp/catalog.ts` `MAX_LIST_PAGES`.
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
        let config = McpServerConfig::local(vec![
            "node".into(),
            "server.js".into(),
        ]);
        assert!(config.is_local());
        assert!(!config.is_remote());
        assert_eq!(config.command, Some(vec!["node".into(), "server.js".into()]));
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
        assert_eq!(
            config.full_args(),
            vec!["run", "-A", "server.ts"]
        );

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
