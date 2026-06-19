#![forbid(unsafe_code)]
#![warn(clippy::all)]

//! LSP integration for rustcode.
//!
//! Ported from: `packages/opencode/src/lsp/`
//!
//! ## Architecture
//!
//! - [`LspManager`] — orchestrates multiple language server connections,
//!   auto-detects which servers a workspace needs, and starts/stops them.
//! - [`LspClient`] — a connection to a single language server, providing
//!   diagnostics, document symbols, and workspace symbols.
//! - `LspClientState` (internal) — owns the child process, the
//!   JSON-RPC request/response engine, and the background read loop.
//!
//! ## JSON-RPC framing (LSP base protocol)
//!
//! ```text
//! Content-Length: <byte-count>\r\n
//! \r\n
//! <json-body>
//! ```
//!
//! See [`frame_lsp_message`] and [`parse_lsp_message`].

use std::collections::{HashMap, HashSet};
use std::fmt;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::Arc;
use std::time::Duration;

use rustcode_core::lsp::{
    language_id_for_extension, LspClientInfo, LspConnectionStatus, LspDiagnostic,
    LspDocumentSymbol, LspServerInfo, LspStatus, LspSymbol,
};
use serde_json::Value;
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
use tokio::process::{Child, ChildStdin, Command};
use tokio::sync::{Mutex, RwLock, oneshot};
use tracing::{debug, error, info, warn};

// =============================================================================
// Error type
// =============================================================================

/// An error that occurred during LSP operations.
#[derive(Debug)]
pub enum LspError {
    /// I/O error communicating with the language server process.
    Io(std::io::Error),
    /// JSON serialization or deserialization error.
    Json(serde_json::Error),
    /// Failed to spawn the language server process.
    Spawn(String),
    /// The `initialize` handshake failed or timed out.
    Initialize(String),
    /// A JSON-RPC request timed out.
    Timeout(String),
    /// The requested server ID is not active.
    ServerNotFound(String),
    /// The server is not connected (already shut down or never started).
    NotConnected(String),
    /// An error occurred during the shutdown sequence.
    Shutdown(String),
    /// No launch command is configured for this server.
    NoCommand(String),
    /// The server process exited unexpectedly.
    ServerExited(String),
}

impl fmt::Display for LspError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Io(e) => write!(f, "LSP I/O error: {e}"),
            Self::Json(e) => write!(f, "LSP JSON error: {e}"),
            Self::Spawn(m) => write!(f, "LSP spawn error: {m}"),
            Self::Initialize(m) => write!(f, "LSP initialize error: {m}"),
            Self::Timeout(m) => write!(f, "LSP timeout: {m}"),
            Self::ServerNotFound(id) => write!(f, "LSP server not found: '{id}'"),
            Self::NotConnected(m) => write!(f, "LSP not connected: {m}"),
            Self::Shutdown(m) => write!(f, "LSP shutdown error: {m}"),
            Self::NoCommand(id) => write!(f, "LSP no command configured for server: '{id}'"),
            Self::ServerExited(m) => write!(f, "LSP server exited: {m}"),
        }
    }
}

impl std::error::Error for LspError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Self::Io(e) => Some(e),
            Self::Json(e) => Some(e),
            _ => None,
        }
    }
}

impl From<std::io::Error> for LspError {
    fn from(e: std::io::Error) -> Self {
        Self::Io(e)
    }
}

impl From<serde_json::Error> for LspError {
    fn from(e: serde_json::Error) -> Self {
        Self::Json(e)
    }
}

/// Convenience alias for results from this crate.
pub type Result<T> = std::result::Result<T, LspError>;

// =============================================================================
// Constants
// =============================================================================

/// Timeout for the `initialize` handshake (matches upstream TS).
const INITIALIZE_TIMEOUT: Duration = Duration::from_secs(45);

/// Default timeout for general LSP requests.
const REQUEST_TIMEOUT: Duration = Duration::from_secs(30);

/// Grace period between sending `exit` and force-killing the process.
const SHUTDOWN_GRACE_MS: u64 = 500;

// =============================================================================
// JSON-RPC framing helpers
// =============================================================================

/// Wrap a JSON string with an LSP Content-Length header.
///
/// The LSP base protocol uses a simple framing where each message is
/// prefixed with `Content-Length: <N>\r\n\r\n`.
///
/// # Example
///
/// ```
/// # use rustcode_lsp::frame_lsp_message;
/// let framed = frame_lsp_message(r#"{"jsonrpc":"2.0","method":"test"}"#);
/// assert!(framed.starts_with("Content-Length: "));
/// assert!(framed.contains("\r\n\r\n"));
/// ```
pub fn frame_lsp_message(json: &str) -> String {
    let len = json.as_bytes().len();
    format!("Content-Length: {len}\r\n\r\n{json}")
}

/// Extract the JSON body from a framed LSP message.
///
/// Parses the Content-Length header, then extracts and deserializes
/// the JSON body of the given length.
///
/// # Example
///
/// ```
/// # use rustcode_lsp::{frame_lsp_message, parse_lsp_message};
/// let original = serde_json::json!({"jsonrpc":"2.0","result":"ok"});
/// let framed = frame_lsp_message(&original.to_string());
/// let parsed = parse_lsp_message(&framed).expect("parse");
/// assert_eq!(parsed, original);
/// ```
pub fn parse_lsp_message(data: &str) -> std::result::Result<Value, String> {
    let (content_length, body_start) = parse_header(data)?;
    let body = &data[body_start..];
    if body.len() < content_length {
        return Err(format!(
            "incomplete message: expected {content_length} bytes, got {}",
            body.len()
        ));
    }
    let json_str = &body[..content_length];
    serde_json::from_str(json_str).map_err(|e| format!("invalid JSON in LSP message: {e}"))
}

/// Parse the Content-Length header and return `(length, body_start_index)`.
fn parse_header(data: &str) -> std::result::Result<(usize, usize), String> {
    let header_end = data
        .find("\r\n\r\n")
        .ok_or_else(|| "missing LSP header terminator".to_string())?;

    let header = &data[..header_end];
    let content_length = header
        .lines()
        .find_map(|line| {
            if line.to_lowercase().starts_with("content-length:") {
                line.split(':')
                    .nth(1)
                    .and_then(|s| s.trim().parse::<usize>().ok())
            } else {
                None
            }
        })
        .ok_or_else(|| "missing Content-Length header".to_string())?;

    Ok((content_length, header_end + 4)) // skip "\r\n\r\n"
}

/// Extract zero or more complete LSP messages from a byte buffer.
///
/// Returns the parsed JSON values and the number of bytes consumed.
/// Partial messages (incomplete body) are left in the buffer.
fn extract_messages(buf: &[u8]) -> (Vec<Value>, usize) {
    let mut messages = Vec::new();
    let data = match std::str::from_utf8(buf) {
        Ok(s) => s,
        Err(_) => return (messages, 0),
    };

    let mut offset = 0;
    while offset < data.len() {
        let remaining = &data[offset..];
        let header_end = match remaining.find("\r\n\r\n") {
            Some(p) => p,
            None => break,
        };

        let header = &remaining[..header_end];
        let content_length = match header.lines().find_map(|line| {
            if line.to_lowercase().starts_with("content-length:") {
                line.split(':')
                    .nth(1)
                    .and_then(|s| s.trim().parse::<usize>().ok())
            } else {
                None
            }
        }) {
            Some(len) => len,
            None => {
                offset += header_end + 4;
                continue;
            }
        };

        let body_start = offset + header_end + 4;
        let body_end = body_start + content_length;
        if body_end > data.len() {
            break;
        }

        let json_str = &data[body_start..body_end];
        if let Ok(value) = serde_json::from_str::<Value>(json_str) {
            messages.push(value);
        }
        offset = body_end;
    }

    (messages, offset)
}

// =============================================================================
// Known LSP server catalog
// =============================================================================

/// Return the built-in catalog of known language servers.
///
/// Each entry maps file extensions to a well-known language server
/// executable. Used by [`get_server_for_file`] and the auto-detection
/// logic in [`detect_servers_for_workspace`].
///
/// Ported from: `packages/opencode/src/lsp/server.ts`
pub fn known_servers() -> Vec<LspServerInfo> {
    vec![
        // --- Rust ---
        server("rust", &[".rs"], &["rust-analyzer"]),
        // --- TypeScript / JavaScript ---
        server(
            "typescript",
            &[".ts", ".tsx", ".js", ".jsx", ".mjs", ".cjs", ".mts", ".cts"],
            &["typescript-language-server", "--stdio"],
        ),
        // --- Python ---
        server(
            "pyright",
            &[".py", ".pyi"],
            &["pyright-langserver", "--stdio"],
        ),
        // --- Go ---
        server("gopls", &[".go"], &["gopls"]),
        // --- C / C++ ---
        server(
            "clangd",
            &[".c", ".cpp", ".cc", ".cxx", ".h", ".hpp", ".hxx", ".hh"],
            &["clangd"],
        ),
        // --- C# ---
        server("csharp", &[".cs", ".csx"], &["roslyn-language-server"]),
        // --- Java ---
        server("java", &[".java"], &["jdtls"]),
        // --- Kotlin ---
        server("kotlin", &[".kt", ".kts"], &["kotlin-language-server"]),
        // --- Swift ---
        server("swift", &[".swift"], &["sourcekit-lsp"]),
        // --- Elixir ---
        server("elixir", &[".ex", ".exs"], &["elixir-ls"]),
        // --- Erlang ---
        server("erlang", &[".erl", ".hrl"], &["erlang_ls"]),
        // --- Haskell ---
        server(
            "haskell",
            &[".hs", ".lhs"],
            &["haskell-language-server-wrapper"],
        ),
        // --- Dart ---
        server("dart", &[".dart"], &["dart", "language-server"]),
        // --- Lua ---
        server("lua", &[".lua"], &["lua-language-server"]),
        // --- Zig ---
        server("zig", &[".zig", ".zon"], &["zls"]),
        // --- Scala ---
        server("scala", &[".scala"], &["metals"]),
        // --- Ruby ---
        server(
            "ruby",
            &[".rb", ".rake", ".gemspec", ".ru", ".erb"],
            &["solargraph", "stdio"],
        ),
        // --- PHP ---
        server("php", &[".php"], &["intelephense", "--stdio"]),
        // --- OCaml ---
        server("ocaml", &[".ml", ".mli"], &["ocamllsp"]),
        // --- Terraform ---
        server(
            "terraform",
            &[".tf", ".tfvars", ".hcl"],
            &["terraform-ls", "serve"],
        ),
        // --- Nix ---
        server("nix", &[".nix"], &["nixd"]),
        // --- Dockerfile ---
        server(
            "dockerfile",
            &[".dockerfile"],
            &["docker-langserver", "--stdio"],
        ),
        // --- Bash ---
        server(
            "bash",
            &[".sh", ".bash", ".zsh", ".ksh"],
            &["bash-language-server", "start"],
        ),
        // --- YAML ---
        server(
            "yaml",
            &[".yaml", ".yml"],
            &["yaml-language-server", "--stdio"],
        ),
        // --- JSON ---
        server(
            "json",
            &[".json"],
            &["vscode-json-languageserver", "--stdio"],
        ),
        // --- Vue ---
        server("vue", &[".vue"], &["vue-language-server", "--stdio"]),
        // --- Svelte ---
        server("svelte", &[".svelte"], &["svelteserver", "--stdio"]),
        // --- Astro ---
        server("astro", &[".astro"], &["astro-ls", "--stdio"]),
        // --- Gleam ---
        server("gleam", &[".gleam"], &["gleam", "lsp"]),
        // --- Typst ---
        server("typst", &[".typ", ".typc"], &["tinymist"]),
        // --- LaTeX ---
        server("latex", &[".tex", ".latex"], &["texlab"]),
        // --- Markdown ---
        server("markdown", &[".md", ".markdown"], &["marksman"]),
        // --- CSS ---
        server(
            "css",
            &[".css", ".scss", ".sass", ".less"],
            &["vscode-css-language-server", "--stdio"],
        ),
        // --- HTML ---
        server(
            "html",
            &[".html", ".htm"],
            &["vscode-html-language-server", "--stdio"],
        ),
    ]
}

/// Convenience helper to build an [`LspServerInfo`].
fn server(id: &str, extensions: &[&str], command: &[&str]) -> LspServerInfo {
    LspServerInfo {
        id: id.to_string(),
        extensions: extensions.iter().map(|e| e.to_string()).collect(),
        command: Some(command.iter().map(|s| s.to_string()).collect()),
        env: None,
        initialization: None,
    }
}

/// Return every known server that supports the given file extension.
///
/// The extension can be given with or without a leading dot.
///
/// # Example
///
/// ```
/// # use rustcode_lsp::get_server_for_file;
/// let servers = get_server_for_file(".rs");
/// assert_eq!(servers.len(), 1);
/// assert_eq!(&servers[0].id, "rust");
/// ```
pub fn get_server_for_file(ext: &str) -> Vec<LspServerInfo> {
    let needle = if ext.starts_with('.') {
        ext.to_string()
    } else {
        format!(".{ext}")
    };
    known_servers()
        .into_iter()
        .filter(|s| s.extensions.iter().any(|e| e == &needle))
        .collect()
}

// =============================================================================
// Workspace auto-detection
// =============================================================================

/// Mapping from project config file names to the server IDs they imply.
static CONFIG_FILE_TO_SERVER: &[(&str, &str)] = &[
    ("Cargo.toml", "rust"),
    ("package.json", "typescript"),
    ("tsconfig.json", "typescript"),
    ("go.mod", "gopls"),
    ("pyproject.toml", "pyright"),
    ("setup.py", "pyright"),
    ("setup.cfg", "pyright"),
    ("CMakeLists.txt", "clangd"),
    ("compile_commands.json", "clangd"),
    ("pom.xml", "java"),
    ("build.gradle", "java"),
    ("build.gradle.kts", "kotlin"),
    ("mix.exs", "elixir"),
    ("rebar.config", "erlang"),
    ("stack.yaml", "haskell"),
    ("package.yaml", "haskell"),
    ("pubspec.yaml", "dart"),
    ("build.zig", "zig"),
    ("build.zig.zon", "zig"),
    ("composer.json", "php"),
    ("dune-project", "ocaml"),
    ("Gemfile", "ruby"),
    ("flake.nix", "nix"),
    ("shell.nix", "nix"),
    ("Dockerfile", "dockerfile"),
    ("gleam.toml", "gleam"),
    ("svelte.config.js", "svelte"),
    ("astro.config.mjs", "astro"),
    ("vue.config.js", "vue"),
    ("*.csproj", "csharp"),
    ("*.sln", "csharp"),
];

/// Scan the given workspace directory for known project config files and
/// return the set of language servers that should be started.
///
/// This looks for files like `Cargo.toml`, `package.json`, `go.mod`, etc.
///
/// # Example
///
/// ```
/// # use std::path::Path;
/// # use rustcode_lsp::detect_servers_for_workspace;
/// let servers = detect_servers_for_workspace(Path::new("/nonexistent"));
/// assert!(servers.is_empty());
/// ```
pub fn detect_servers_for_workspace(root: &Path) -> Vec<LspServerInfo> {
    let catalog = known_servers();
    let mut seen = HashSet::new();
    let mut result = Vec::new();

    for (config_file, server_id) in CONFIG_FILE_TO_SERVER {
        // Handle glob-style markers (e.g. "*.csproj")
        let candidate = if config_file.starts_with("*.") {
            let ext = &config_file[1..]; // ".csproj"
            let mut found = None;
            if let Ok(entries) = std::fs::read_dir(root) {
                for entry in entries.flatten() {
                    let name = entry.file_name();
                    if name.to_string_lossy().ends_with(ext) {
                        found = Some(entry.path());
                        break;
                    }
                }
            }
            found
        } else {
            let p = root.join(config_file);
            if p.exists() {
                Some(p)
            } else {
                None
            }
        };

        if candidate.is_some() && seen.insert(*server_id) {
            if let Some(s) = catalog.iter().find(|s| s.id == *server_id) {
                result.push(s.clone());
            }
        }
    }

    result
}

// =============================================================================
// LspClientState — internal request/response engine
// =============================================================================

/// Internal state for a single language server connection.
///
/// Owns the child process handle and stdin writer, manages the
/// JSON-RPC request/response correlation, and caches incoming
/// diagnostics.
struct LspClientState {
    /// The child process (set to `None` after shutdown).
    child: Mutex<Option<Child>>,
    /// Handle to the child's stdin (set to `None` after shutdown).
    stdin: Mutex<Option<ChildStdin>>,
    /// Monotonically increasing JSON-RPC request ID.
    next_request_id: AtomicU64,
    /// Maps in-flight request IDs to their response channels.
    pending_requests: Mutex<HashMap<u64, oneshot::Sender<std::result::Result<Value, LspError>>>>,
    /// Cached diagnostics (appended to by `textDocument/publishDiagnostics`).
    diagnostics: RwLock<Vec<LspDiagnostic>>,
    /// Set to `false` when the server shuts down or crashes.
    alive: AtomicBool,
    /// Server ID for diagnostics and error messages.
    server_id: String,
}

impl LspClientState {
    /// Spawn the language server, perform the initialize handshake, and
    /// start the background I/O loop.
    async fn new(server_info: &LspServerInfo, root_dir: &Path) -> Result<Arc<Self>> {
        let command = server_info
            .command
            .as_ref()
            .ok_or_else(|| LspError::NoCommand(server_info.id.clone()))?;

        let (program, args) = command.split_first().ok_or_else(|| {
            LspError::NoCommand(format!("empty command for '{}'", server_info.id))
        })?;

        // --- Spawn child process ---
        let mut cmd = Command::new(program);
        cmd.args(args);
        cmd.stdin(std::process::Stdio::piped());
        cmd.stdout(std::process::Stdio::piped());
        cmd.stderr(std::process::Stdio::piped());
        cmd.kill_on_drop(true);
        cmd.current_dir(root_dir);

        if let Some(env) = &server_info.env {
            for (k, v) in env {
                cmd.env(k, v);
            }
        }

        let mut child = cmd
            .spawn()
            .map_err(|e| LspError::Spawn(format!("failed to spawn '{program}': {e}")))?;

        let stdin = child
            .stdin
            .take()
            .ok_or_else(|| LspError::Spawn("failed to capture stdin".into()))?;
        let stdout = child
            .stdout
            .take()
            .ok_or_else(|| LspError::Spawn("failed to capture stdout".into()))?;
        let stderr = child
            .stderr
            .take()
            .ok_or_else(|| LspError::Spawn("failed to capture stderr".into()))?;

        // --- Build shared state ---
        let state = Arc::new(Self {
            child: Mutex::new(Some(child)),
            stdin: Mutex::new(Some(stdin)),
            next_request_id: AtomicU64::new(1),
            pending_requests: Mutex::new(HashMap::new()),
            diagnostics: RwLock::new(Vec::new()),
            alive: AtomicBool::new(true),
            server_id: server_info.id.clone(),
        });

        // --- Spawn stderr logger ---
        tokio::spawn(read_stderr(stderr, server_info.id.clone()));

        // --- Spawn stdout reader ---
        let bg_state = Arc::clone(&state);
        tokio::spawn(read_stdout_loop(bg_state, stdout));

        // --- Perform the initialize handshake ---
        let root_uri = path_to_uri(root_dir);
        let workspace_name = root_dir
            .file_name()
            .map(|n| n.to_string_lossy().to_string())
            .unwrap_or_else(|| "workspace".into());

        let mut init_params = serde_json::json!({
            "processId": null,
            "rootUri": root_uri,
            "capabilities": {
                "textDocument": {
                    "publishDiagnostics": { "versionSupport": false },
                    "synchronization": {
                        "didOpen": true,
                        "didChange": true,
                        "didSave": true,
                        "didClose": true
                    }
                },
                "workspace": {
                    "workspaceFolders": true,
                    "symbol": {
                        "symbolKind": {
                            "valueSet": [1,2,3,4,5,6,7,8,9,10,11,12,13,14,15,16,17,18,19,20,21,22,23,24,25,26]
                        }
                    }
                }
            },
            "workspaceFolders": [{ "uri": root_uri, "name": workspace_name }]
        });

        // Merge server-specific initialization options
        if let Some(opts) = &server_info.initialization {
            if let Some(obj) = init_params.as_object_mut() {
                obj.insert("initializationOptions".into(), opts.clone());
            }
        }

        state
            .send_request_timeout("initialize", init_params, INITIALIZE_TIMEOUT)
            .await
            .map_err(|e| {
                LspError::Initialize(format!(
                    "failed to initialize '{}': {e}",
                    server_info.id
                ))
            })?;

        // Send `initialized` notification
        state
            .send_notification("initialized", serde_json::json!({}))
            .await?;

        // Send workspace/didChangeConfiguration if the server has settings
        if let Some(opts) = &server_info.initialization {
            let _ = state
                .send_notification(
                    "workspace/didChangeConfiguration",
                    serde_json::json!({ "settings": opts }),
                )
                .await;
        }

        info!(
            server_id = %server_info.id,
            root = %root_dir.display(),
            "LSP server initialized"
        );

        Ok(state)
    }

    /// Send a JSON-RPC request and await the response.
    async fn send_request(&self, method: &str, params: Value) -> Result<Value> {
        self.send_request_timeout(method, params, REQUEST_TIMEOUT)
            .await
    }

    /// Send a JSON-RPC request with a custom timeout.
    async fn send_request_timeout(
        &self,
        method: &str,
        params: Value,
        timeout: Duration,
    ) -> Result<Value> {
        if !self.alive.load(Ordering::SeqCst) {
            return Err(LspError::NotConnected(format!(
                "server '{}' is not alive",
                self.server_id
            )));
        }

        let id = self.next_request_id.fetch_add(1, Ordering::SeqCst);
        let (tx, rx) = oneshot::channel();

        self.pending_requests.lock().await.insert(id, tx);

        let request = serde_json::json!({
            "jsonrpc": "2.0",
            "id": id,
            "method": method,
            "params": params,
        });

        let framed = frame_lsp_message(&request.to_string());

        {
            let mut stdin = self.stdin.lock().await;
            if let Some(stdin) = stdin.as_mut() {
                stdin.write_all(framed.as_bytes()).await?;
                stdin.flush().await?;
            } else {
                self.pending_requests.lock().await.remove(&id);
                return Err(LspError::NotConnected("stdin closed".into()));
            }
        }

        match tokio::time::timeout(timeout, rx).await {
            Ok(Ok(result)) => result,
            Ok(Err(_recv_err)) => Err(LspError::ServerExited(format!(
                "server '{}' exited during '{}'",
                self.server_id, method
            ))),
            Err(_elapsed) => {
                self.pending_requests.lock().await.remove(&id);
                Err(LspError::Timeout(format!(
                    "'{}' timed out after {}s",
                    method,
                    timeout.as_secs()
                )))
            }
        }
    }

    /// Send a JSON-RPC notification (no response expected).
    async fn send_notification(&self, method: &str, params: Value) -> Result<()> {
        if !self.alive.load(Ordering::SeqCst) {
            return Err(LspError::NotConnected(format!(
                "server '{}' is not alive",
                self.server_id
            )));
        }

        let notification = serde_json::json!({
            "jsonrpc": "2.0",
            "method": method,
            "params": params,
        });

        let framed = frame_lsp_message(&notification.to_string());

        let mut stdin = self.stdin.lock().await;
        if let Some(stdin) = stdin.as_mut() {
            stdin.write_all(framed.as_bytes()).await?;
            stdin.flush().await?;
        } else {
            return Err(LspError::NotConnected("stdin closed".into()));
        }

        Ok(())
    }

    /// Gracefully shut down the server: send `shutdown`, then `exit`,
    /// then force-kill if it hasn't exited on its own.
    async fn shutdown(&self) -> Result<()> {
        // Guard against double-shutdown (but keep alive=true so the
        // shutdown handshake can actually be sent).
        if !self.alive.load(Ordering::SeqCst) {
            return Ok(());
        }

        // Best-effort graceful shutdown — the server is still alive here.
        let _ = self
            .send_request_timeout("shutdown", serde_json::json!({}), Duration::from_secs(5))
            .await;
        let _ = self
            .send_notification("exit", serde_json::json!({}))
            .await;

        // Mark as dead so any concurrent senders fail fast from now on.
        self.alive.store(false, Ordering::SeqCst);

        tokio::time::sleep(Duration::from_millis(SHUTDOWN_GRACE_MS)).await;

        let mut child_guard = self.child.lock().await;
        if let Some(mut child) = child_guard.take() {
            if let Err(e) = child.start_kill() {
                warn!(
                    server_id = %self.server_id,
                    error = %e,
                    "Failed to kill LSP child process"
                );
            }
            let _ = child.wait().await;
        }

        // Close stdin → triggers stdout reader to exit
        self.stdin.lock().await.take();

        // Drain pending response channels
        self.pending_requests.lock().await.clear();

        info!(
            server_id = %self.server_id,
            "LSP server shut down"
        );

        Ok(())
    }
}

// ---------------------------------------------------------------------------
// Background I/O tasks
// ---------------------------------------------------------------------------

/// Pipe the server's stderr into tracing.
async fn read_stderr(stderr: tokio::process::ChildStderr, server_id: String) {
    let reader = BufReader::new(stderr);
    let mut lines = reader.lines();
    loop {
        match lines.next_line().await {
            Ok(Some(line)) => {
                debug!(server_id = %server_id, stderr = %line, "LSP stderr");
            }
            Ok(None) => break,
            Err(e) => {
                warn!(
                    server_id = %server_id,
                    error = %e,
                    "Error reading LSP stderr"
                );
                break;
            }
        }
    }
}

/// Continuously read framed JSON-RPC messages from stdout and dispatch
/// them as responses or notifications.
async fn read_stdout_loop(state: Arc<LspClientState>, stdout: tokio::process::ChildStdout) {
    let mut reader = BufReader::new(stdout);
    let mut buf = Vec::new();
    let mut line = String::new();

    loop {
        line.clear();
        match reader.read_line(&mut line).await {
            Ok(0) => {
                debug!(
                    server_id = %state.server_id,
                    "LSP stdout closed (server exited)"
                );
                break;
            }
            Ok(_) => {}
            Err(e) => {
                error!(
                    server_id = %state.server_id,
                    error = %e,
                    "Error reading LSP stdout"
                );
                break;
            }
        }

        buf.extend_from_slice(line.as_bytes());

        let (messages, consumed) = extract_messages(&buf);
        if consumed > 0 {
            let remaining = buf.len() - consumed;
            if remaining > 0 {
                buf.copy_within(consumed.., 0);
            }
            buf.truncate(remaining);
        }

        for message in messages {
            dispatch_message(&state, message).await;
        }
    }

    // Mark as dead and drain any pending response channels
    state.alive.store(false, Ordering::SeqCst);
    state.pending_requests.lock().await.clear();
}

/// Route a single JSON-RPC message from the server.
async fn dispatch_message(state: &LspClientState, message: Value) {
    // --- Response (has "id") ---
    if let Some(id) = message.get("id").and_then(|i| i.as_u64()) {
        let mut pending = state.pending_requests.lock().await;
        if let Some(tx) = pending.remove(&id) {
            if let Some(err) = message.get("error") {
                let msg = err
                    .get("message")
                    .and_then(|m| m.as_str())
                    .unwrap_or("unknown LSP error");
                let _ = tx.send(Err(LspError::Shutdown(msg.into())));
            } else {
                let result = message.get("result").cloned().unwrap_or(Value::Null);
                let _ = tx.send(Ok(result));
            }
        }
        return;
    }

    // --- Notification (has "method", no "id") ---
    if let Some(method) = message.get("method").and_then(|m| m.as_str()) {
        match method {
            "textDocument/publishDiagnostics" => {
                if let Some(params) = message.get("params") {
                    let uri = params
                        .get("uri")
                        .and_then(|u| u.as_str())
                        .unwrap_or("")
                        .to_string();

                    let raw_diags = params
                        .get("diagnostics")
                        .and_then(|d| d.as_array())
                        .cloned()
                        .unwrap_or_default();

                    let new_diags: Vec<LspDiagnostic> =
                        serde_json::from_value(Value::Array(raw_diags)).unwrap_or_default();

                    let mut cache = state.diagnostics.write().await;
                    cache.retain(|d| d.uri != uri);
                    cache.extend(new_diags);

                    debug!(
                        server_id = %state.server_id,
                        uri = %uri,
                        count = cache.iter().filter(|d| d.uri == uri).count(),
                        "Received diagnostics"
                    );
                }
            }
            "window/logMessage" => {
                if let Some(params) = message.get("params") {
                    let msg = params
                        .get("message")
                        .and_then(|m| m.as_str())
                        .unwrap_or("");
                    let level = params
                        .get("type")
                        .and_then(|t| t.as_u64())
                        .unwrap_or(4);
                    match level {
                        1 => error!(server_id = %state.server_id, "[LSP] {msg}"),
                        2 => warn!(server_id = %state.server_id, "[LSP] {msg}"),
                        _ => debug!(server_id = %state.server_id, "[LSP] {msg}"),
                    }
                }
            }
            "telemetry/event"
            | "$/progress"
            | "window/workDoneProgress/create"
            | "$/cancelRequest" => {
                // Progress / telemetry — silently ignored
            }
            other => {
                debug!(
                    server_id = %state.server_id,
                    method = %other,
                    "Unhandled LSP notification"
                );
            }
        }
    }
}

// =============================================================================
// LspClient
// =============================================================================

/// A connection to a single language server process.
///
/// Created by [`LspManager::connect`]. Provides access to live
/// diagnostics, document symbols, and workspace-wide symbol search.
pub struct LspClient {
    /// Unique server ID (e.g. `"rust"`, `"typescript"`).
    pub server_id: String,
    /// Project root directory for this server.
    pub root: String,
    /// Working directory of the server process.
    pub directory: String,
    /// Internal connection state.
    state: Arc<LspClientState>,
}

impl LspClient {
    /// Create and connect to a language server. Called by [`LspManager::connect`].
    pub(crate) async fn new(
        server_info: &LspServerInfo,
        root_dir: &Path,
    ) -> Result<Self> {
        let root = root_dir
            .canonicalize()
            .unwrap_or_else(|_| root_dir.to_path_buf());
        let root_str = root.to_string_lossy().to_string();
        let state = LspClientState::new(server_info, &root).await?;

        Ok(Self {
            server_id: server_info.id.clone(),
            root: root_str.clone(),
            directory: root_str,
            state,
        })
    }

    /// Return the cached diagnostics.
    ///
    /// All diagnostics received via `textDocument/publishDiagnostics` are
    /// appended here. Each [`LspDiagnostic`] carries a `uri` field so
    /// callers can filter by file.
    pub fn diagnostics(&self) -> Arc<RwLock<Vec<LspDiagnostic>>> {
        Arc::clone(&self.state.diagnostics)
    }

    /// Return metadata describing this client connection.
    pub fn info(&self) -> LspClientInfo {
        LspClientInfo {
            server_id: self.server_id.clone(),
            root: self.root.clone(),
            directory: self.directory.clone(),
        }
    }

    /// Request document symbols for a file from the language server.
    ///
    /// Sends `textDocument/documentSymbol` and returns the parsed
    /// [`LspDocumentSymbol`] list.
    pub async fn document_symbols(
        &self,
        file: &str,
    ) -> Result<Vec<LspDocumentSymbol>> {
        let uri = path_to_uri(Path::new(file));
        let params = serde_json::json!({ "textDocument": { "uri": uri } });

        let result = self
            .state
            .send_request("textDocument/documentSymbol", params)
            .await?;

        if let Some(arr) = result.as_array() {
            if arr
                .first()
                .map(|s| s.get("selectionRange").is_some())
                .unwrap_or(false)
            {
                Ok(serde_json::from_value(result)?)
            } else {
                Ok(Vec::new())
            }
        } else {
            Ok(Vec::new())
        }
    }

    /// Search for workspace-wide symbols matching a query string.
    ///
    /// Sends `workspace/symbol` and returns the parsed [`LspSymbol`] list.
    pub async fn workspace_symbols(&self, query: &str) -> Result<Vec<LspSymbol>> {
        let params = serde_json::json!({ "query": query });

        let result = self
            .state
            .send_request("workspace/symbol", params)
            .await?;

        Ok(serde_json::from_value(result)?)
    }

    /// Gracefully shut down the language server.
    pub async fn shutdown(&self) -> Result<()> {
        self.state.shutdown().await
    }
}

// =============================================================================
// LspManager
// =============================================================================

/// Manages multiple language server connections for a workspace.
///
/// Auto-detects which servers are needed by scanning for known config
/// files, then starts and stops them on demand.
///
/// # Example (pseudo-code)
///
/// ```ignore
/// let manager = LspManager::new();
/// manager.update(Path::new("/path/to/project")).await?;
/// for c in manager.list_clients() {
///     println!("{} -> {}", c.server_id, c.root);
/// }
/// ```
pub struct LspManager {
    clients: std::sync::RwLock<HashMap<String, Arc<LspClient>>>,
}

impl LspManager {
    /// Create a new, empty manager.
    pub fn new() -> Self {
        Self {
            clients: std::sync::RwLock::new(HashMap::new()),
        }
    }

    /// Connect to a language server for the given root directory.
    ///
    /// Spawns the process, performs the init handshake, and stores the
    /// client. If a client with the same `server_info.id` is already
    /// connected the existing one is returned unchanged.
    pub async fn connect(
        &self,
        server_info: LspServerInfo,
        root_dir: &Path,
    ) -> Result<Arc<LspClient>> {
        // Fast path: already connected
        {
            let clients = self.clients.read().expect("lock poisoned");
            if let Some(existing) = clients.get(&server_info.id) {
                return Ok(Arc::clone(existing));
            }
        }

        let client = Arc::new(LspClient::new(&server_info, root_dir).await?);

        {
            let mut clients = self.clients.write().expect("lock poisoned");
            // Double-check after acquiring write lock
            if let Some(existing) = clients.get(&server_info.id) {
                return Ok(Arc::clone(existing));
            }
            clients.insert(server_info.id.clone(), Arc::clone(&client));
        }

        Ok(client)
    }

    /// Disconnect a language server by its ID.
    ///
    /// Sends the shutdown/exit sequence and removes the client.
    pub async fn disconnect(&self, server_id: &str) -> Result<()> {
        let client = {
            let mut clients = self.clients.write().expect("lock poisoned");
            clients.remove(server_id)
        };

        match client {
            Some(c) => {
                c.shutdown().await?;
                Ok(())
            }
            None => Err(LspError::ServerNotFound(server_id.to_string())),
        }
    }

    /// Return a reference to an active client by server ID, if connected.
    pub fn get_client(&self, server_id: &str) -> Option<Arc<LspClient>> {
        self.clients
            .read()
            .expect("lock poisoned")
            .get(server_id)
            .cloned()
    }

    /// List metadata for all active clients.
    pub fn list_clients(&self) -> Vec<LspClientInfo> {
        self.clients
            .read()
            .expect("lock poisoned")
            .values()
            .map(|c| c.info())
            .collect()
    }

    /// Scan the workspace and update the server fleet.
    ///
    /// Detects required servers, starts missing ones, and stops servers
    /// whose toolchains are no longer present. Returns the current status
    /// of every detected server.
    pub async fn update(&self, workspace_root: &Path) -> Vec<LspStatus> {
        let needed = detect_servers_for_workspace(workspace_root);

        // Start new servers
        for info in &needed {
            if self.get_client(&info.id).is_none() {
                if let Err(e) = self.connect(info.clone(), workspace_root).await {
                    warn!(
                        server_id = %info.id,
                        error = %e,
                        "Failed to auto-start LSP server"
                    );
                }
            }
        }

        // Collect needed IDs
        let needed_ids: HashSet<&str> = needed.iter().map(|s| s.id.as_str()).collect();

        // Stop stale servers
        let current_ids: Vec<String> = {
            self.clients
                .read()
                .expect("lock poisoned")
                .keys()
                .cloned()
                .collect()
        };

        for id in &current_ids {
            if !needed_ids.contains(id.as_str()) {
                let _ = self.disconnect(id).await;
            }
        }

        self.build_status_list(workspace_root)
    }

    /// Build a status list using the workspace root to relativize paths.
    fn build_status_list(&self, workspace_root: &Path) -> Vec<LspStatus> {
        let ws = workspace_root.to_string_lossy().to_string();

        self.clients
            .read()
            .expect("lock poisoned")
            .values()
            .map(|c| {
                let relative_root = if c.root.starts_with(&ws) {
                    let rel = &c.root[ws.len()..];
                    rel.trim_start_matches('/')
                        .trim_start_matches('\\')
                        .to_string()
                } else {
                    c.root.clone()
                };

                LspStatus {
                    id: c.server_id.clone(),
                    name: c.server_id.clone(),
                    root: if relative_root.is_empty() {
                        ".".into()
                    } else {
                        relative_root
                    },
                    status: LspConnectionStatus::Connected,
                }
            })
            .collect()
    }
}

impl Default for LspManager {
    fn default() -> Self {
        Self::new()
    }
}

// =============================================================================
// Helpers
// =============================================================================

/// Convert a filesystem path to a `file://` URI.
fn path_to_uri(path: &Path) -> String {
    let abs = path
        .canonicalize()
        .unwrap_or_else(|_| path.to_path_buf());
    format!("file://{}", abs.display())
}

// =============================================================================
// Tests
// =============================================================================

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::Path;

    // ------------------------------------------------------------------
    // JSON-RPC framing
    // ------------------------------------------------------------------

    #[test]
    fn frame_contains_length_and_body() {
        let json = r#"{"jsonrpc":"2.0","method":"test"}"#;
        let framed = frame_lsp_message(json);
        assert!(framed.starts_with("Content-Length: "));
        assert!(framed.contains("\r\n\r\n"));
        assert!(framed.ends_with(json));
    }

    #[test]
    fn frame_content_length_matches_byte_count() {
        let json = r#"{"key":"value"}"#;
        let framed = frame_lsp_message(json);
        let expected_len = json.as_bytes().len();
        let header_line = framed.lines().next().expect("first line");
        let parsed: usize = header_line
            .strip_prefix("Content-Length: ")
            .expect("prefix")
            .parse()
            .expect("number");
        assert_eq!(parsed, expected_len);
    }

    #[test]
    fn parse_roundtrip() {
        let original = serde_json::json!({"jsonrpc":"2.0","id":1,"result":{}});
        let framed = frame_lsp_message(&original.to_string());
        let parsed = parse_lsp_message(&framed).expect("parse");
        assert_eq!(parsed, original);
    }

    #[test]
    fn parse_notification_missing_id() {
        let notif = serde_json::json!({
            "jsonrpc": "2.0",
            "method": "textDocument/publishDiagnostics",
            "params": { "uri": "file:///test.rs", "diagnostics": [] }
        });
        let framed = frame_lsp_message(&notif.to_string());
        let parsed = parse_lsp_message(&framed).expect("parse");
        assert_eq!(parsed["method"], "textDocument/publishDiagnostics");
        assert!(parsed.get("id").is_none());
    }

    #[test]
    fn parse_missing_header() {
        assert!(parse_lsp_message("garbage without header").is_err());
    }

    #[test]
    fn parse_incomplete_body() {
        // Content-Length says 100 but body is much shorter
        let data = "Content-Length: 100\r\n\r\n{\"short\"}";
        assert!(parse_lsp_message(data).is_err());
    }

    #[test]
    fn parse_missing_content_length() {
        let data = "X-Foo: bar\r\n\r\n{\"test\":1}";
        assert!(parse_lsp_message(data).is_err());
    }

    #[test]
    fn parse_header_case_insensitive() {
        let data = "content-length: 15\r\n\r\n000000000000000";
        let (len, body_start) = parse_header(data).expect("parse");
        assert_eq!(len, 15);
        assert_eq!(&data[body_start..body_start + 15], "000000000000000");
    }

    #[test]
    fn parse_header_whitespace_insensitive() {
        let data = "Content-Length:  42  \r\n\r\nAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAA";
        let (len, _) = parse_header(data).expect("parse");
        assert_eq!(len, 42);
    }

    // ------------------------------------------------------------------
    // extract_messages
    // ------------------------------------------------------------------

    #[test]
    fn extract_single_message() {
        let msg = serde_json::json!({"jsonrpc":"2.0","id":1,"result":"ok"});
        let framed = frame_lsp_message(&msg.to_string());
        let (msgs, consumed) = extract_messages(framed.as_bytes());
        assert_eq!(msgs.len(), 1);
        assert_eq!(consumed, framed.len());
    }

    #[test]
    fn extract_multiple_messages() {
        let m1 = frame_lsp_message(r#"{"jsonrpc":"2.0","id":1,"result":"a"}"#);
        let m2 = frame_lsp_message(r#"{"jsonrpc":"2.0","method":"n"}"#);
        let combined = format!("{m1}{m2}");
        let (msgs, consumed) = extract_messages(combined.as_bytes());
        assert_eq!(msgs.len(), 2);
        assert_eq!(consumed, combined.len());
    }

    #[test]
    fn extract_partial_message_waits() {
        let full = frame_lsp_message(r#"{"jsonrpc":"2.0","id":1,"result":"ok"}"#);
        let split = full.len() - 5;
        let (msgs, consumed) = extract_messages(full[..split].as_bytes());
        assert_eq!(msgs.len(), 0);
        assert_eq!(consumed, 0);
    }

    #[test]
    fn extract_complete_then_partial() {
        let m1 = frame_lsp_message(r#"{"jsonrpc":"2.0","id":1,"result":"first"}"#);
        let m2 = frame_lsp_message(r#"{"jsonrpc":"2.0","id":2,"result":"second"}"#);
        let combined = format!("{m1}{m2}");
        let split = combined.len() - 3;
        let (msgs, _) = extract_messages(combined[..split].as_bytes());
        assert_eq!(msgs.len(), 1);
        assert_eq!(msgs[0]["id"], 1);
    }

    #[test]
    fn extract_empty_input() {
        let (msgs, consumed) = extract_messages(b"");
        assert_eq!(msgs.len(), 0);
        assert_eq!(consumed, 0);
    }

    #[test]
    fn extract_non_utf8_returns_empty() {
        let (msgs, consumed) = extract_messages(&[0xFF, 0xFE, 0xFD]);
        assert_eq!(msgs.len(), 0);
        assert_eq!(consumed, 0);
    }

    // ------------------------------------------------------------------
    // frame_lsp_message edge cases
    // ------------------------------------------------------------------

    #[test]
    fn frame_empty_json() {
        let framed = frame_lsp_message("{}");
        assert!(framed.contains("Content-Length: 2"));
        assert!(framed.ends_with("{}"));
    }

    #[test]
    fn frame_unicode_preserved() {
        let json = r#"{"msg":"héllo wörld 🌍"}"#;
        let framed = frame_lsp_message(json);
        let parsed = parse_lsp_message(&framed).expect("parse");
        assert_eq!(parsed["msg"], "héllo wörld 🌍");
    }

    // ------------------------------------------------------------------
    // Server-for-file detection
    // ------------------------------------------------------------------

    #[test]
    fn rust_extension() {
        let s = get_server_for_file(".rs");
        assert_eq!(s.len(), 1);
        assert_eq!(s[0].id, "rust");
    }

    #[test]
    fn typescript_extension() {
        let s = get_server_for_file(".ts");
        assert!(s.iter().any(|i| i.id == "typescript"));
    }

    #[test]
    fn python_extension() {
        let s = get_server_for_file(".py");
        assert!(s.iter().any(|i| i.id == "pyright"));
    }

    #[test]
    fn go_extension() {
        let s = get_server_for_file(".go");
        assert!(s.iter().any(|i| i.id == "gopls"));
    }

    #[test]
    fn without_leading_dot() {
        let s = get_server_for_file("rs");
        assert_eq!(s.len(), 1);
        assert_eq!(s[0].id, "rust");
    }

    #[test]
    fn unknown_extension() {
        assert!(get_server_for_file(".fakelang").is_empty());
    }

    #[test]
    fn every_extension_mapped_to_at_least_one_server() {
        // Each server's extensions should appear in at least that server's list
        for server in known_servers() {
            for ext in &server.extensions {
                let found = get_server_for_file(ext);
                assert!(
                    found.iter().any(|s| s.id == server.id),
                    "extension '{ext}' should resolve to server '{}'",
                    server.id
                );
            }
        }
    }

    // ------------------------------------------------------------------
    // Known server catalog integrity
    // ------------------------------------------------------------------

    #[test]
    fn all_servers_have_command() {
        for s in known_servers() {
            assert!(s.command.is_some(), "server '{}' must have a command", s.id);
            assert!(
                !s.command.as_ref().unwrap().is_empty(),
                "server '{}' command must be non-empty",
                s.id
            );
        }
    }

    #[test]
    fn all_servers_have_extensions() {
        for s in known_servers() {
            assert!(
                !s.extensions.is_empty(),
                "server '{}' must list extensions",
                s.id
            );
        }
    }

    #[test]
    fn all_server_ids_unique() {
        let mut ids = HashSet::new();
        for s in known_servers() {
            assert!(ids.insert(s.id.clone()), "duplicate server ID '{}'", s.id);
        }
    }

    // ------------------------------------------------------------------
    // Config file to server coverage
    // ------------------------------------------------------------------

    #[test]
    fn config_files_map_to_known_servers() {
        let all_ids: HashSet<&str> = known_servers().iter().map(|s| s.id.as_str()).collect();
        for (file, id) in CONFIG_FILE_TO_SERVER {
            assert!(
                all_ids.contains(id),
                "config file '{file}' maps to unknown server '{id}'"
            );
        }
    }

    // ------------------------------------------------------------------
    // Auto-detection (non-existent directory → empty)
    // ------------------------------------------------------------------

    #[test]
    fn detect_empty_workspace() {
        assert!(detect_servers_for_workspace(Path::new("/nonexistent_xyz")).is_empty());
    }

    // ------------------------------------------------------------------
    // LspManager (no live servers needed)
    // ------------------------------------------------------------------

    #[test]
    fn manager_new_is_empty() {
        let m = LspManager::new();
        assert!(m.list_clients().is_empty());
    }

    #[test]
    fn manager_get_client_missing() {
        let m = LspManager::new();
        assert!(m.get_client("nope").is_none());
    }

    #[test]
    fn manager_default_is_empty() {
        let m = LspManager::default();
        assert!(m.list_clients().is_empty());
    }

    // ------------------------------------------------------------------
    // LspClientInfo / LspStatus struct integrity
    // ------------------------------------------------------------------

    #[test]
    fn client_info_fields() {
        let info = LspClientInfo {
            server_id: "rust".into(),
            root: "/home/project".into(),
            directory: "/home/project".into(),
        };
        assert_eq!(info.server_id, "rust");
        assert_eq!(info.root, "/home/project");
    }

    #[test]
    fn lsp_status_connected() {
        let s = LspStatus {
            id: "rust".into(),
            name: "rust-analyzer".into(),
            root: ".".into(),
            status: LspConnectionStatus::Connected,
        };
        assert_eq!(s.status, LspConnectionStatus::Connected);
    }

    // ------------------------------------------------------------------
    // LspError
    // ------------------------------------------------------------------

    #[test]
    fn error_display() {
        let e = LspError::ServerNotFound("foo".into());
        assert!(e.to_string().contains("foo"));
        assert!(e.to_string().contains("not found"));
    }

    #[test]
    fn error_from_io() {
        let io = std::io::Error::new(std::io::ErrorKind::NotFound, "gone");
        let e: LspError = io.into();
        assert!(matches!(e, LspError::Io(_)));
    }

    #[test]
    fn error_from_json() {
        let je = serde_json::from_str::<Value>("{").unwrap_err();
        let e: LspError = je.into();
        assert!(matches!(e, LspError::Json(_)));
    }

    #[test]
    fn error_source_io() {
        let io = std::io::Error::new(std::io::ErrorKind::Other, "test");
        let e = LspError::Io(io);
        assert!(e.source().is_some());
    }

    #[test]
    fn error_source_json() {
        let je = serde_json::from_str::<Value>("{").unwrap_err();
        let e = LspError::Json(je);
        assert!(e.source().is_some());
    }

    #[test]
    fn error_source_none_for_other_variants() {
        assert!(LspError::Timeout("t".into()).source().is_none());
        assert!(LspError::NoCommand("s".into()).source().is_none());
    }

    // ------------------------------------------------------------------
    // path_to_uri
    // ------------------------------------------------------------------

    #[test]
    fn path_to_uri_starts_with_file() {
        let uri = path_to_uri(Path::new("/tmp/test.rs"));
        assert!(uri.starts_with("file:///"));
        assert!(uri.ends_with("test.rs"));
    }

    // ------------------------------------------------------------------
    // Language ID integration
    // ------------------------------------------------------------------

    #[test]
    fn language_id_for_rust() {
        assert_eq!(language_id_for_extension(".rs"), "rust");
    }

    #[test]
    fn language_id_for_typescript() {
        assert_eq!(language_id_for_extension(".ts"), "typescript");
    }

    #[test]
    fn language_id_fallback_plaintext() {
        assert_eq!(language_id_for_extension(".zzz"), "plaintext");
    }

    // ------------------------------------------------------------------
    // detect_servers_for_workspace with real temp dir
    // ------------------------------------------------------------------

    // ------------------------------------------------------------------
    // Helper: create a temporary directory that cleans up on drop.
    // ------------------------------------------------------------------

    struct TempDir {
        path: PathBuf,
    }

    impl TempDir {
        fn new(prefix: &str) -> std::io::Result<Self> {
            let mut path = std::env::temp_dir();
            // Use process ID + prefix to avoid collisions between test runs
            let dirname = format!(
                "rustcode_lsp_test_{}_{}_{}",
                prefix,
                std::process::id(),
                std::time::SystemTime::now()
                    .duration_since(std::time::UNIX_EPOCH)
                    .unwrap_or_default()
                    .as_nanos()
            );
            path.push(dirname);
            std::fs::create_dir_all(&path)?;
            Ok(Self { path })
        }

        fn path(&self) -> &Path {
            &self.path
        }
    }

    impl Drop for TempDir {
        fn drop(&mut self) {
            let _ = std::fs::remove_dir_all(&self.path);
        }
    }

    #[test]
    fn detect_with_cargo_toml() {
        let dir = TempDir::new("cargo").expect("tempdir");
        std::fs::write(dir.path().join("Cargo.toml"), "[package]\nname = \"test\"\n")
            .expect("write");
        let servers = detect_servers_for_workspace(dir.path());
        assert!(servers.iter().any(|s| s.id == "rust"));
    }

    #[test]
    fn detect_with_package_json() {
        let dir = TempDir::new("npm").expect("tempdir");
        std::fs::write(dir.path().join("package.json"), "{}").expect("write");
        let servers = detect_servers_for_workspace(dir.path());
        assert!(servers.iter().any(|s| s.id == "typescript"));
    }

    #[test]
    fn detect_with_go_mod() {
        let dir = TempDir::new("go").expect("tempdir");
        std::fs::write(dir.path().join("go.mod"), "module test\n").expect("write");
        let servers = detect_servers_for_workspace(dir.path());
        assert!(servers.iter().any(|s| s.id == "gopls"));
    }

    #[test]
    fn detect_with_pyproject_toml() {
        let dir = TempDir::new("py").expect("tempdir");
        std::fs::write(dir.path().join("pyproject.toml"), "[project]\nname = \"test\"\n")
            .expect("write");
        let servers = detect_servers_for_workspace(dir.path());
        assert!(servers.iter().any(|s| s.id == "pyright"));
    }

    #[test]
    fn detect_multiple_configs() {
        let dir = TempDir::new("multi").expect("tempdir");
        std::fs::write(dir.path().join("Cargo.toml"), "[package]\nname = \"a\"\n")
            .expect("write");
        std::fs::write(dir.path().join("package.json"), "{}").expect("write");
        let servers = detect_servers_for_workspace(dir.path());
        let ids: HashSet<&str> = servers.iter().map(|s| s.id.as_str()).collect();
        assert!(ids.contains("rust"));
        assert!(ids.contains("typescript"));
    }

    #[test]
    fn detect_no_server_without_config() {
        let dir = TempDir::new("empty").expect("tempdir");
        // Create only a random file, no known config
        std::fs::write(dir.path().join("README.md"), "# hello").expect("write");
        let servers = detect_servers_for_workspace(dir.path());
        assert!(servers.is_empty());
    }

    // ------------------------------------------------------------------
    // LspManager::update with temp dir (integration-level)
    // ------------------------------------------------------------------

    #[tokio::test]
    async fn update_detects_servers() {
        let dir = TempDir::new("update_rs").expect("tempdir");
        std::fs::write(dir.path().join("Cargo.toml"), "[package]\nname = \"t\"\n")
            .expect("write");
        let manager = LspManager::new();
        // update() will try to spawn rust-analyzer, which likely isn't
        // installed — the call should not panic, just return statuses.
        let statuses = manager.update(dir.path()).await;
        // We should get at least one status entry (the detected rust server)
        // regardless of whether the actual process could start.
        assert!(!statuses.is_empty(), "update should have detected servers");
    }

    #[tokio::test]
    async fn update_with_no_config_yields_empty() {
        let dir = TempDir::new("update_empty").expect("tempdir");
        std::fs::write(dir.path().join("hello.txt"), "world").expect("write");
        let manager = LspManager::new();
        let statuses = manager.update(dir.path()).await;
        assert!(statuses.is_empty());
    }
}
