//! CLI error formatting with styled output.
//!
//! Ported from: `packages/opencode/src/cli/error.ts` — `FormatError()` function
//! handling 12 error types with styled messages.

// ── ANSI style constants ────────────────────────────────────────────────

pub const TEXT_DANGER_BOLD: &str = "\x1b[1;31m";
pub const TEXT_DIM: &str = "\x1b[2m";
pub const TEXT_BOLD: &str = "\x1b[1m";
pub const TEXT_WARNING: &str = "\x1b[33m";
pub const TEXT_INFO: &str = "\x1b[36m";
pub const TEXT_GREEN: &str = "\x1b[32m";
pub const TEXT_RESET: &str = "\x1b[0m";

const PREFIX: &str = "\x1b[1;31mError:\x1b[0m";

// ── CliErrorFormatter ──────────────────────────────────────────────────

/// Structured error formatter for the CLI.
///
/// Ported from: `packages/opencode/src/cli/error.ts` — `FormatError()`.
/// Prints styled error messages to stderr with actionable suggestions.
pub struct CliErrorFormatter {
    pub has_errors: bool,
}

impl CliErrorFormatter {
    pub fn new() -> Self {
        Self { has_errors: false }
    }

    // ── Main dispatch ────────────────────────────────────────────────

    /// Format and print an error with appropriate styling.
    ///
    /// Attempts to downcast to known rustcode-core error types. If the
    /// error is not a recognized core error, formats it as a generic
    /// CLI error.
    pub fn format_error(&mut self, err: &anyhow::Error, _cmd: &str) {
        self.has_errors = true;
        if let Some(core_err) = err.downcast_ref::<rustcode_core::error::Error>() {
            self.format_core_error(core_err);
        } else {
            self.fmt_cli_error(&err.to_string());
        }
    }

    /// Dispatch a core `Error` variant to the appropriate formatter.
    fn format_core_error(&self, err: &rustcode_core::error::Error) {
        use rustcode_core::error::Error;
        match err {
            Error::Config(msg) => self.fmt_config_error(msg, ""),
            Error::ProviderInit { provider_id, message } => {
                self.fmt_provider_init_error(provider_id, message);
            }
            Error::NoProviders => {
                self.fmt_provider_init_error(
                    "unknown",
                    "No LLM providers detected. Set an API key environment variable.",
                );
            }
            Error::NoModels { provider_id } => {
                self.fmt_provider_model_not_found("", provider_id);
            }
            Error::ModelNotFound { provider_id, model_id } => {
                self.fmt_provider_model_not_found(model_id, provider_id);
            }
            Error::McpNotFound { name } => {
                self.fmt_mcp_failed_error(name, "MCP server not found");
            }
            Error::Auth(msg) => self.fmt_account_service_error("auth", msg),
            Error::Json(e) => self.fmt_config_json_error(&e.to_string(), "config"),
            Error::Io(e) => self.fmt_cli_error(&format!("I/O error: {e}")),
            Error::Session(msg) => self.fmt_cli_error(&format!("Session error: {msg}")),
            Error::SessionNotFound { session_id } => {
                self.fmt_cli_error(&format!("Session `{session_id}` not found"));
            }
            Error::Git(msg) => self.fmt_cli_error(&format!("Git error: {msg}")),
            Error::Network(msg) => self.fmt_account_service_error("network", msg),
            Error::Aborted => {}
            Error::Permission(e) => self.fmt_cli_error(&format!("Permission error: {e}")),
            Error::Tool(msg) => self.fmt_cli_error(&format!("Tool error: {msg}")),
            Error::NotImplemented(feature) => {
                self.fmt_cli_error(&format!("Not implemented: {feature}"));
            }
            Error::LspInit(msg) => self.fmt_cli_error(&format!("LSP initialization error: {msg}")),
            Error::Plugin(msg) => self.fmt_cli_error(&format!("Plugin error: {msg}")),
            Error::Database(msg) => self.fmt_cli_error(&format!("Database error: {msg}")),
            Error::Search(msg) => self.fmt_cli_error(&format!("Search error: {msg}")),
            Error::InvalidSearchPattern(msg) => {
                self.fmt_cli_error(&format!("Invalid search pattern: {msg}"));
            }
            Error::QuestionRejected => {}
            Error::ContextOverflow(msg) => {
                eprintln!("{} Context window exceeded: {msg}", PREFIX);
                eprintln!("{}Try enabling compression or reducing input size.{}", TEXT_DIM, TEXT_RESET);
            }
            Error::HeaderTimeout { ms } => {
                eprintln!("{} Provider response headers timed out after {ms}ms", PREFIX);
                eprintln!("{}Consider increasing timeout or checking provider status.{}", TEXT_DIM, TEXT_RESET);
            }
            Error::ResponseStream(msg) => {
                eprintln!("{} Provider response stream error: {msg}", PREFIX);
            }
            Error::Internal(msg) => {
                eprintln!("{} Internal error: {msg}", PREFIX);
                eprintln!("{}This is a bug. Please report it.{}", TEXT_DIM, TEXT_RESET);
            }
            _ => self.fmt_cli_error(&err.to_string()),
        }
    }

    // ── Instance methods (delegate to free functions) ───────────────
    // These match the TS FormatError dispatch and can be called from
    // command handlers that have access to a CliErrorFormatter instance.

    /// Generic CLI error.
    pub fn fmt_cli_error(&self, msg: &str) {
        format_cli_error(msg);
    }

    /// Config error with optional detail.
    pub fn fmt_config_error(&self, msg: &str, detail: &str) {
        format_config_error(msg, detail);
    }

    /// Config JSON parse error.
    pub fn fmt_config_json_error(&self, msg: &str, path: &str) {
        format_config_json_error(msg, path);
    }

    /// Directory typo suggestion.
    pub fn fmt_config_directory_typo(&self, dir: &str, suggestion: &str) {
        format_config_directory_typo(dir, suggestion);
    }

    /// Frontmatter parse error.
    pub fn fmt_config_frontmatter_error(&self, file: &str, msg: &str) {
        format_config_frontmatter_error(file, msg);
    }

    /// Remote config auth error.
    pub fn fmt_config_remote_auth_error(&self, url: &str) {
        format_config_remote_auth_error(url);
    }

    /// Invalid config key.
    pub fn fmt_config_invalid_error(&self, key: &str, msg: &str) {
        format_config_invalid_error(key, msg);
    }

    /// Model not found.
    pub fn fmt_provider_model_not_found(&self, model: &str, provider: &str) {
        format_provider_model_not_found(model, provider);
    }

    /// Provider init error.
    pub fn fmt_provider_init_error(&self, provider: &str, msg: &str) {
        format_provider_init_error(provider, msg);
    }

    /// MCP server failure.
    pub fn fmt_mcp_failed_error(&self, server: &str, msg: &str) {
        format_mcp_failed_error(server, msg);
    }

    /// Account service error.
    pub fn fmt_account_service_error(&self, op: &str, msg: &str) {
        format_account_service_error(op, msg);
    }

    /// User cancelled (no output).
    pub fn fmt_ui_cancelled(&self) {}
}

impl Default for CliErrorFormatter {
    fn default() -> Self {
        Self::new()
    }
}

// ── Free-form helper functions ─────────────────────────────────────────
// These allow command handlers to print styled error output without
// needing access to a CliErrorFormatter instance.

/// Print a styled error message to stderr.
pub fn format_cli_error(msg: &str) {
    eprintln!("{} {msg}", PREFIX);
}

/// Pretty-print a JSON parse error for a config file path.
///
/// Ported from TS `ConfigJsonError` handler.
pub fn format_config_json_error(msg: &str, path: &str) {
    eprintln!("{} Config file at {path}{} is not valid JSON(C)", PREFIX, TEXT_RESET);
    if !msg.is_empty() {
        eprintln!("{}  {msg}{}", TEXT_DIM, TEXT_RESET);
    }
}

/// Directory name typo with rename suggestion.
///
/// Ported from TS `ConfigDirectoryTypoError` handler.
pub fn format_config_directory_typo(dir: &str, suggestion: &str) {
    eprintln!("{} Directory {dir}{} is not valid", PREFIX, TEXT_RESET);
    eprintln!("{}Rename the directory to {suggestion}{} or remove it. This is a common typo.{}",
        TEXT_DIM, TEXT_RESET, TEXT_RESET);
}

/// YAML/TOML frontmatter parse error.
///
/// Ported from TS `ConfigFrontmatterError` handler.
pub fn format_config_frontmatter_error(file: &str, msg: &str) {
    eprintln!("{} Frontmatter error in {file}{}", PREFIX, TEXT_RESET);
    if !msg.is_empty() {
        eprintln!("{}  {msg}{}", TEXT_DIM, TEXT_RESET);
    }
}

/// Remote config authentication failure.
///
/// Ported from TS `ConfigRemoteAuthError` handler.
pub fn format_config_remote_auth_error(url: &str) {
    eprintln!("{} Failed to load remote config: the server returned a login page instead of JSON", PREFIX);
    eprintln!("{}Authentication is missing or has expired (the endpoint is likely behind an SSO or identity-aware proxy).{}",
        TEXT_DIM, TEXT_RESET);
    if !url.is_empty() {
        eprintln!("{}Run `rustcode auth login {url}`{} to re-authenticate.{}",
            TEXT_DIM, TEXT_RESET, TEXT_RESET);
    }
}

/// Invalid configuration key/value.
///
/// Ported from TS `ConfigInvalidError` handler (path-specific branch).
pub fn format_config_invalid_error(key: &str, msg: &str) {
    eprintln!("{} Invalid configuration key {key}{}", PREFIX, TEXT_RESET);
    if !msg.is_empty() {
        eprintln!("{}  {msg}{}", TEXT_DIM, TEXT_RESET);
    }
    eprintln!("{}Check your opencode.json or opencode.jsonc file for typos.{}", TEXT_DIM, TEXT_RESET);
}

/// Configuration error with optional detail.
///
/// Ported from TS `ConfigInvalidError` handler.
pub fn format_config_error(msg: &str, detail: &str) {
    if detail.is_empty() {
        eprintln!("{} Configuration: {msg}", PREFIX);
    } else {
        eprintln!("{} Configuration: {msg}", PREFIX);
        eprintln!("{}  {detail}{}", TEXT_DIM, TEXT_RESET);
    }
}

/// Model not found for a provider.
///
/// Ported from TS `ProviderModelNotFoundError` handler.
pub fn format_provider_model_not_found(model: &str, provider: &str) {
    if model.is_empty() {
        eprintln!("{} No models available for provider {provider}{}", PREFIX, TEXT_RESET);
    } else {
        eprintln!("{} Model {model}{} not found for provider {provider}{}",
            PREFIX, TEXT_RESET, TEXT_RESET);
    }
    eprintln!("{}Try: `rustcode models`{} to list available models{}",
        TEXT_DIM, TEXT_RESET, TEXT_RESET);
    eprintln!("{}Or check your config (opencode.json) provider/model names{}",
        TEXT_DIM, TEXT_RESET);
}

/// Provider initialization failure.
///
/// Ported from TS `ProviderInitError` handler.
pub fn format_provider_init_error(provider: &str, msg: &str) {
    eprintln!("{} Failed to initialize provider {provider}{}", PREFIX, TEXT_RESET);
    if !msg.is_empty() {
        eprintln!("{}  {msg}{}", TEXT_DIM, TEXT_RESET);
    }
    eprintln!("{}Check credentials and configuration.{}", TEXT_DIM, TEXT_RESET);
}

/// MCP server failure.
///
/// Ported from TS `MCPFailed` handler.
pub fn format_mcp_failed_error(server: &str, msg: &str) {
    eprintln!("{} MCP server {server}{} failed", PREFIX, TEXT_RESET);
    if !msg.is_empty() {
        eprintln!("{}  {msg}{}", TEXT_DIM, TEXT_RESET);
    }
    eprintln!("{}Note: opencode does not support MCP authentication yet.{}", TEXT_DIM, TEXT_RESET);
}

/// Account service or transport error.
///
/// Ported from TS `AccountServiceError` / `AccountTransportError` handler.
pub fn format_account_service_error(op: &str, msg: &str) {
    eprintln!("{} Account {op} failed", PREFIX);
    if !msg.is_empty() {
        eprintln!("{}  {msg}{}", TEXT_DIM, TEXT_RESET);
    }
}
