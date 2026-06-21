//! MCP OAuth flow — PKCE, callback server, token exchange, and browser launch.
//!
//! Ported from:
//! - `packages/opencode/src/mcp/oauth-provider.ts`
//! - `packages/opencode/src/mcp/oauth-callback.ts`
//! - `packages/opencode/src/mcp/index.ts` (auth flow integration lines 748–898)

use base64::{engine::general_purpose::URL_SAFE_NO_PAD, Engine};
use rand::Rng;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::collections::HashMap;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::time::{Duration, Instant};
use tokio::io::{AsyncBufReadExt, AsyncWriteExt};
use tokio::net::TcpListener;
use tokio::sync::Mutex;
use tracing::debug;

use crate::mcp::{McpAuthStore, McpOAuthConfig};

// ---------------------------------------------------------------------------
// Constants
// ---------------------------------------------------------------------------

/// Default OAuth callback port.
pub const OAUTH_CALLBACK_PORT: u16 = 19876;

/// Default OAuth callback path.
pub const OAUTH_CALLBACK_PATH: &str = "/mcp/oauth/callback";

/// OAuth callback timeout (5 minutes).
const CALLBACK_TIMEOUT: Duration = Duration::from_secs(5 * 60);

/// Maximum port retries when binding the callback server.
const MAX_PORT_RETRIES: u16 = 100;

/// Timeout for HTTP requests to the OAuth server.
const HTTP_TIMEOUT: Duration = Duration::from_secs(30);

// ---------------------------------------------------------------------------
// Errors
// ---------------------------------------------------------------------------

/// Errors that can occur during the MCP OAuth flow.
///
/// # Source
/// Ported from `packages/opencode/src/mcp/oauth-callback.ts`
/// and `packages/opencode/src/mcp/index.ts` auth flow errors.
#[derive(Debug, thiserror::Error)]
pub enum OAuthError {
    /// OAuth callback timed out — user did not complete authorization.
    #[error("OAuth callback timeout — authorization took too long")]
    Timeout,

    /// OAuth state parameter did not match — potential CSRF attack.
    #[error("OAuth state mismatch — potential CSRF attack")]
    StateMismatch,

    /// No pending OAuth flow found for the given server URL.
    #[error("no pending OAuth flow for server URL `{server_url}`")]
    NoPendingFlow { server_url: String },

    /// OAuth completion failed at the server.
    #[error("OAuth completion failed: {0}")]
    CompletionFailed(String),

    /// Authorization was cancelled by the user.
    #[error("authorization cancelled")]
    Cancelled,

    /// Failed to fetch OAuth metadata from the server.
    #[error("OAuth metadata fetch failed: {0}")]
    MetadataError(String),

    /// Token exchange with the authorization server failed.
    #[error("token exchange failed: {0}")]
    TokenExchangeError(String),

    /// HTTP request failed.
    #[error("HTTP error: {0}")]
    HttpError(String),

    /// No authorization code was returned from the callback.
    #[error("no authorization code received")]
    NoCode,

    /// The OAuth callback server encountered an error.
    #[error("callback server error: {0}")]
    ServerError(String),

    /// Failed to open the browser.
    #[error("failed to open browser: {0}")]
    BrowserOpenError(String),
}

// ---------------------------------------------------------------------------
// PKCE helpers
// ---------------------------------------------------------------------------

/// Generate a PKCE code verifier (random unreserved string, 64–96 chars).
///
/// Uses characters from the RFC 3986 unreserved set: `A-Z`, `a-z`, `0-9`,
/// `-`, `.`, `_`, `~`.
///
/// # Source
/// Ported from `packages/opencode/src/mcp/index.ts` the `generateCodeVerifier`
/// call (inlined at line 766).
pub fn generate_code_verifier() -> String {
    let mut rng = rand::thread_rng();
    let length = rng.gen_range(64..=96);
    let charset = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789-._~";
    (0..length)
        .map(|_| {
            let idx = rng.gen_range(0..charset.len());
            charset[idx] as char
        })
        .collect()
}

/// Generate a PKCE code challenge: `BASE64URL-ENCODE(SHA256(code_verifier))`.
///
/// # Source
/// Ported from `packages/opencode/src/mcp/index.ts` (the PKCE challenge
/// computation done by the MCP SDK).
pub fn generate_code_challenge(verifier: &str) -> String {
    let hash = Sha256::digest(verifier.as_bytes());
    URL_SAFE_NO_PAD.encode(hash)
}

/// Generate a cryptographically random state string (32 random bytes as hex).
///
/// # Source
/// Ported from `packages/opencode/src/mcp/oauth-provider.ts` `state()` method
/// (lines 154–162).
pub fn generate_random_state() -> String {
    let bytes: [u8; 32] = rand::random();
    hex::encode(bytes)
}

// ---------------------------------------------------------------------------
// OAuth metadata types
// ---------------------------------------------------------------------------

/// OAuth 2.0 Authorization Server metadata (RFC 8414).
///
/// # Source
/// Ported from the MCP SDK `@modelcontextprotocol/sdk/shared/auth` types.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OAuthServerMetadata {
    /// The authorization endpoint URL.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub authorization_endpoint: Option<String>,
    /// The token endpoint URL.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub token_endpoint: Option<String>,
    /// PKCE challenge methods supported (e.g. `S256`).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub code_challenge_methods_supported: Option<Vec<String>>,
    /// Scopes supported by the server.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub scopes_supported: Option<Vec<String>>,
    /// Token endpoint auth methods supported.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub token_endpoint_auth_methods_supported: Option<Vec<String>>,
    /// Registration endpoint for dynamic client registration.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub registration_endpoint: Option<String>,
    /// Issuer identifier.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub issuer: Option<String>,
    /// Any extra fields returned by the server.
    #[serde(flatten)]
    pub extra: HashMap<String, serde_json::Value>,
}

// ---------------------------------------------------------------------------
// Token types
// ---------------------------------------------------------------------------

/// OAuth tokens returned from the token endpoint.
///
/// # Source
/// Ported from `@modelcontextprotocol/sdk/shared/auth` `OAuthTokens`.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OAuthTokenResponse {
    /// Access token.
    pub access_token: String,
    /// Token type (typically "Bearer").
    pub token_type: String,
    /// Optional refresh token.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub refresh_token: Option<String>,
    /// Expiry duration in seconds from issuance.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub expires_in: Option<u64>,
    /// Scope of the token.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub scope: Option<String>,
}

// ---------------------------------------------------------------------------
// OAuthStateManager
// ---------------------------------------------------------------------------

/// Tracks pending OAuth state → (server_url, code_verifier) for callback
/// correlation. Thread-safe with automatic timeout-based cleanup.
/// Each pending entry carries a oneshot channel so the HTTP callback
/// handler can deliver the authorization code back to the waiting caller.
///
/// # Source
/// Ported from `packages/opencode/src/mcp/oauth-callback.ts` — the
/// `pendingAuths` Map and timeout / cleanup logic (lines 51–72).
pub struct OAuthStateManager {
    states: Arc<Mutex<HashMap<String, PendingEntry>>>,
}

struct PendingEntry {
    server_url: String,
    code_verifier: String,
    created_at: Instant,
    /// Oneshot sender for delivering the authorization code (or error)
    /// back to the `wait_for_callback` caller.
    result_tx: tokio::sync::oneshot::Sender<Result<String, OAuthError>>,
}

impl OAuthStateManager {
    /// Create a new empty state manager.
    pub fn new() -> Self {
        Self {
            states: Arc::new(Mutex::new(HashMap::new())),
        }
    }

    /// Insert a new pending OAuth state with a channel for the result.
    ///
    /// Returns the receiver end of the channel that will deliver the
    /// authorization code when the callback arrives.
    pub async fn insert(
        &self,
        state: String,
        server_url: String,
        code_verifier: String,
    ) -> tokio::sync::oneshot::Receiver<Result<String, OAuthError>> {
        let (tx, rx) = tokio::sync::oneshot::channel();
        let mut states = self.states.lock().await;
        states.insert(
            state,
            PendingEntry {
                server_url,
                code_verifier,
                created_at: Instant::now(),
                result_tx: tx,
            },
        );
        rx
    }

    /// Resolve a pending state with an authorization code.
    ///
    /// Sends the code through the oneshot channel and removes the entry.
    /// Returns `true` if a pending entry was found and resolved.
    pub async fn resolve(&self, state: &str, code: String) -> bool {
        let mut states = self.states.lock().await;
        if let Some(entry) = states.remove(state) {
            if entry.created_at.elapsed() <= CALLBACK_TIMEOUT {
                let _ = entry.result_tx.send(Ok(code));
                return true;
            }
        }
        false
    }

    /// Reject a pending state with an error.
    ///
    /// Sends the error through the oneshot channel and removes the entry.
    /// Returns `true` if a pending entry was found and rejected.
    pub async fn reject(&self, state: &str, error: OAuthError) -> bool {
        let mut states = self.states.lock().await;
        if let Some(entry) = states.remove(state) {
            let _ = entry.result_tx.send(Err(error));
            return true;
        }
        false
    }

    /// Cancel all pending states for a given server URL.
    pub async fn cancel_by_server_url(&self, server_url: &str) {
        let mut states = self.states.lock().await;
        let to_remove: Vec<String> = states
            .iter()
            .filter(|(_, v)| v.server_url == server_url)
            .map(|(k, _)| k.clone())
            .collect();
        for key in to_remove {
            if let Some(entry) = states.remove(&key) {
                let _ = entry
                    .result_tx
                    .send(Err(OAuthError::Cancelled));
            }
        }
    }

    /// Cancel a specific pending state.
    pub async fn cancel(&self, state: &str) {
        let mut states = self.states.lock().await;
        if let Some(entry) = states.remove(state) {
            let _ = entry
                .result_tx
                .send(Err(OAuthError::Cancelled));
        }
    }

    /// Remove expired entries.
    pub async fn cleanup_expired(&self) {
        let mut states = self.states.lock().await;
        states.retain(|_, v| v.created_at.elapsed() <= CALLBACK_TIMEOUT);
    }

    /// Number of currently pending states.
    pub async fn pending_count(&self) -> usize {
        self.states.lock().await.len()
    }
}

impl Default for OAuthStateManager {
    fn default() -> Self {
        Self::new()
    }
}

impl Clone for OAuthStateManager {
    fn clone(&self) -> Self {
        Self {
            states: self.states.clone(),
        }
    }
}

// ---------------------------------------------------------------------------
// OAuthCallbackServer
// ---------------------------------------------------------------------------

const HTML_SUCCESS: &str = r#"<!DOCTYPE html>
<html>
<head>
  <title>OpenCode - Authorization Successful</title>
  <style>
    body { font-family: system-ui, -apple-system, sans-serif; display: flex; justify-content: center; align-items: center; height: 100vh; margin: 0; background: #1a1a2e; color: #eee; }
    .container { text-align: center; padding: 2rem; }
    h1 { color: #4ade80; margin-bottom: 1rem; }
    p { color: #aaa; }
  </style>
</head>
<body>
  <div class="container">
    <h1>Authorization Successful</h1>
    <p>You can close this window and return to the terminal.</p>
  </div>
  <script>setTimeout(() => window.close(), 2000);</script>
</body>
</html>"#;

fn html_error(error: &str) -> String {
    format!(
        r#"<!DOCTYPE html>
<html>
<head>
  <title>OpenCode - Authorization Failed</title>
  <style>
    body {{ font-family: system-ui, -apple-system, sans-serif; display: flex; justify-content: center; align-items: center; height: 100vh; margin: 0; background: #1a1a2e; color: #eee; }}
    .container {{ text-align: center; padding: 2rem; }}
    h1 {{ color: #f87171; margin-bottom: 1rem; }}
    p {{ color: #aaa; }}
    .error {{ color: #fca5a5; font-family: monospace; margin-top: 1rem; padding: 1rem; background: rgba(248,113,113,0.1); border-radius: 0.5rem; }}
  </style>
</head>
<body>
  <div class="container">
    <h1>Authorization Failed</h1>
    <p>An error occurred during authorization.</p>
    <div class="error">{error}</div>
  </div>
</body>
</html>"#
    )
}

/// Local HTTP server that captures the OAuth authorization callback.
///
/// Listens on `127.0.0.1:<port><path>` and handles incoming callbacks
/// with `?code=...&state=...`. Matches the state against
/// [`OAuthStateManager`] and resolves the pending authorization via
/// the oneshot channel stored in the pending entry.
///
/// # Source
/// Ported from `packages/opencode/src/mcp/oauth-callback.ts` — the
/// `createServer`, `handleRequest`, `waitForCallback` functions.
pub struct OAuthCallbackServer {
    running: Arc<AtomicBool>,
    port: u16,
    path: String,
}

impl OAuthCallbackServer {
    /// Start a callback server on a given port and path.
    ///
    /// If the port is busy, tries the next available port up to
    /// `MAX_PORT_RETRIES` attempts.
    ///
    /// Returns the server handle and the actual port number.
    pub async fn start(
        state_manager: OAuthStateManager,
        requested_port: u16,
        path: &str,
    ) -> Result<(Self, u16), OAuthError> {
        let mut actual_port = requested_port;
        let listener = loop {
            let addr = format!("127.0.0.1:{actual_port}");
            match TcpListener::bind(&addr).await {
                Ok(l) => break l,
                Err(_) if actual_port < requested_port + MAX_PORT_RETRIES => {
                    actual_port += 1;
                    continue;
                }
                Err(e) => {
                    return Err(OAuthError::ServerError(format!(
                        "failed to bind callback server on port range {requested_port}–{}: {e}",
                        requested_port + MAX_PORT_RETRIES
                    )));
                }
            }
        };

        let running = Arc::new(AtomicBool::new(true));
        let path_owned = path.to_string();
        let sm = state_manager;
        let r = running.clone();

        // Spawn the accept loop
        tokio::spawn(async move {
            loop {
                if !r.load(Ordering::Relaxed) {
                    break;
                }

                match tokio::time::timeout(Duration::from_secs(1), listener.accept()).await {
                    Ok(Ok((stream, _))) => {
                        let sm = sm.clone();
                        let path = path_owned.clone();
                        tokio::spawn(async move {
                            handle_callback_connection(stream, &sm, &path).await;
                        });
                    }
                    Ok(Err(e)) => {
                        debug!("OAuth callback accept error: {e}");
                        break;
                    }
                    Err(_) => {
                        // accept timeout, check running flag
                    }
                }
            }
            debug!("OAuth callback server stopped");
        });

        debug!("OAuth callback server started on port {actual_port}");
        Ok((
            Self {
                running,
                port: actual_port,
                path: path.to_string(),
            },
            actual_port,
        ))
    }

    /// Start with default port (19876) and path (`/mcp/oauth/callback`).
    pub async fn start_default(
        state_manager: OAuthStateManager,
    ) -> Result<(Self, u16), OAuthError> {
        Self::start(state_manager, OAUTH_CALLBACK_PORT, OAUTH_CALLBACK_PATH).await
    }

    /// Stop the callback server.
    pub fn stop(&self) {
        self.running.store(false, Ordering::Relaxed);
    }

    /// Get the port the server is listening on.
    pub fn port(&self) -> u16 {
        self.port
    }

    /// Get the path the server is serving.
    pub fn path(&self) -> &str {
        &self.path
    }

    /// Build the redirect URI for this server.
    pub fn redirect_uri(&self) -> String {
        format!("http://127.0.0.1:{}{}", self.port, self.path)
    }
}

/// Handle a single HTTP connection from the OAuth callback.
///
/// Parses the HTTP GET request, extracts `code` and `state` query params,
/// resolves the pending state via [`OAuthStateManager::resolve`] or
/// [`OAuthStateManager::reject`], and sends back an HTML response.
async fn handle_callback_connection(
    mut stream: tokio::net::TcpStream,
    state_manager: &OAuthStateManager,
    path: &str,
) {
    let (reader, mut writer) = stream.split();
    let mut buf_reader = tokio::io::BufReader::new(reader);
    let mut request_line = String::new();
    if let Err(e) = buf_reader.read_line(&mut request_line).await {
        debug!("OAuth callback: failed to read request line: {e}");
        return;
    }

    // Read headers (stop at blank line)
    loop {
        let mut line = String::new();
        if let Err(e) = buf_reader.read_line(&mut line).await {
            debug!("OAuth callback: failed to read header: {e}");
            return;
        }
        if line.trim().is_empty() {
            break;
        }
    }

    // Parse request line: "GET /path?code=...&state=... HTTP/1.1"
    let request_path = match request_line.split_whitespace().nth(1) {
        Some(p) => p,
        None => {
            send_http_response(&mut writer, "400 Bad Request", "text/plain", "Bad Request").await;
            return;
        }
    };

    // Strip query string
    let (req_path, query) = match request_path.find('?') {
        Some(idx) => (&request_path[..idx], &request_path[idx + 1..]),
        None => (request_path, ""),
    };

    // Path must match
    if req_path != path {
        send_http_response(&mut writer, "404 Not Found", "text/plain", "Not Found").await;
        return;
    }

    // Parse query string
    let params: HashMap<String, String> = query
        .split('&')
        .filter_map(|pair| {
            let mut parts = pair.splitn(2, '=');
            match (parts.next(), parts.next()) {
                (Some(k), Some(v)) => Some((
                    urlencoding::decode(k).unwrap_or_default().to_string(),
                    urlencoding::decode(v).unwrap_or_default().to_string(),
                )),
                _ => None,
            }
        })
        .collect();

    let state = match params.get("state") {
        Some(s) => s,
        None => {
            send_http_response(
                &mut writer,
                "400 Bad Request",
                "text/html; charset=utf-8",
                "Missing required state parameter",
            )
            .await;
            return;
        }
    };

    // Check for error parameter
    if let Some(err) = params.get("error") {
        let error_desc = params
            .get("error_description")
            .cloned()
            .unwrap_or_else(|| err.clone());
        state_manager
            .reject(state, OAuthError::CompletionFailed(error_desc.clone()))
            .await;
        let body = html_error(&error_desc);
        send_http_response(
            &mut writer,
            "200 OK",
            "text/html; charset=utf-8",
            &body,
        )
        .await;
        return;
    }

    let code = match params.get("code") {
        Some(c) => c,
        None => {
            state_manager
                .reject(state, OAuthError::NoCode)
                .await;
            send_http_response(
                &mut writer,
                "400 Bad Request",
                "text/html; charset=utf-8",
                "No authorization code provided",
            )
            .await;
            return;
        }
    };

    // Resolve the pending state with the authorization code
    let resolved = state_manager.resolve(state, code.clone()).await;
    if !resolved {
        // Unknown or expired state
        send_http_response(
            &mut writer,
            "400 Bad Request",
            "text/html; charset=utf-8",
            &html_error("Invalid or expired state parameter"),
        )
        .await;
        return;
    }

    // Send success response
    send_http_response(&mut writer, "200 OK", "text/html; charset=utf-8", HTML_SUCCESS).await;
}

async fn send_http_response(
    writer: &mut tokio::net::tcp::WriteHalf<'_>,
    status: &str,
    content_type: &str,
    body: &str,
) {
    let response = format!(
        "HTTP/1.1 {status}\r\nContent-Type: {content_type}\r\nContent-Length: {}\r\n\r\n{body}",
        body.len()
    );
    let _ = writer.write_all(response.as_bytes()).await;
}

// ---------------------------------------------------------------------------
// OAuth metadata fetching
// ---------------------------------------------------------------------------

/// Well-known OAuth authorization server configuration path.
const WELL_KNOWN_OAUTH_PATH: &str = "/.well-known/oauth-authorization-server";

/// Fetch OAuth metadata from a server's well-known endpoint.
///
/// Tries `{base_url}/.well-known/oauth-authorization-server`.
///
/// # Source
/// Ported from the MCP SDK's metadata discovery logic.
pub async fn fetch_oauth_metadata(
    server_url: &str,
    client: &reqwest::Client,
) -> Result<OAuthServerMetadata, OAuthError> {
    let url = url::Url::parse(server_url).map_err(|e| {
        OAuthError::MetadataError(format!("invalid server URL: {e}"))
    })?;

    let well_known_url = format!(
        "{}://{}{}{}",
        url.scheme(),
        url.host_str().unwrap_or("localhost"),
        url.port().map(|p| format!(":{p}")).unwrap_or_default(),
        WELL_KNOWN_OAUTH_PATH
    );

    let response = client
        .get(&well_known_url)
        .timeout(HTTP_TIMEOUT)
        .send()
        .await
        .map_err(|e| {
            OAuthError::MetadataError(format!("failed to fetch OAuth metadata: {e}"))
        })?;

    if !response.status().is_success() {
        return Err(OAuthError::MetadataError(format!(
            "OAuth metadata endpoint returned {}",
            response.status()
        )));
    }

    let metadata: OAuthServerMetadata = response.json().await.map_err(|e| {
        OAuthError::MetadataError(format!("failed to parse OAuth metadata: {e}"))
    })?;

    Ok(metadata)
}

// ---------------------------------------------------------------------------
// Token exchange
// ---------------------------------------------------------------------------

/// Exchange an authorization code for tokens via the token endpoint.
///
/// # Source
/// Ported from `packages/opencode/src/mcp/index.ts` — the token exchange
/// in `finishAuth()` (line 870), and the MCP SDK's token endpoint call.
pub async fn exchange_code(
    token_endpoint: &str,
    client_id: &str,
    code_verifier: &str,
    code: &str,
    redirect_uri: &str,
    client: &reqwest::Client,
) -> Result<OAuthTokenResponse, OAuthError> {
    let params = [
        ("grant_type", "authorization_code"),
        ("code", code),
        ("redirect_uri", redirect_uri),
        ("client_id", client_id),
        ("code_verifier", code_verifier),
    ];

    let response = client
        .post(token_endpoint)
        .form(&params)
        .timeout(HTTP_TIMEOUT)
        .send()
        .await
        .map_err(|e| OAuthError::TokenExchangeError(format!("request failed: {e}")))?;

    if !response.status().is_success() {
        let status = response.status();
        let body = response.text().await.unwrap_or_default();
        return Err(OAuthError::TokenExchangeError(format!(
            "token endpoint returned {status}: {body}"
        )));
    }

    let tokens: OAuthTokenResponse = response.json().await.map_err(|e| {
        OAuthError::TokenExchangeError(format!("failed to parse token response: {e}"))
    })?;

    Ok(tokens)
}

/// Refresh an access token using a refresh token.
///
/// # Source
/// Ported from the MCP SDK's token refresh logic.
pub async fn refresh_token(
    token_endpoint: &str,
    client_id: &str,
    refresh_token: &str,
    client: &reqwest::Client,
) -> Result<OAuthTokenResponse, OAuthError> {
    let params = [
        ("grant_type", "refresh_token"),
        ("refresh_token", refresh_token),
        ("client_id", client_id),
    ];

    let response = client
        .post(token_endpoint)
        .form(&params)
        .timeout(HTTP_TIMEOUT)
        .send()
        .await
        .map_err(|e| OAuthError::TokenExchangeError(format!("refresh failed: {e}")))?;

    if !response.status().is_success() {
        let status = response.status();
        let body = response.text().await.unwrap_or_default();
        return Err(OAuthError::TokenExchangeError(format!(
            "token refresh endpoint returned {status}: {body}"
        )));
    }

    let tokens: OAuthTokenResponse = response.json().await.map_err(|e| {
        OAuthError::TokenExchangeError(format!("failed to parse refresh response: {e}"))
    })?;

    Ok(tokens)
}

/// Build the authorization URL with PKCE challenge parameters.
///
/// # Source
/// Ported from the MCP SDK's authorization URL construction
/// and `packages/opencode/src/mcp/index.ts` (the capture of
/// `authorizationUrl` at line 770).
pub fn build_authorization_url(
    authorization_endpoint: &str,
    client_id: &str,
    redirect_uri: &str,
    code_challenge: &str,
    state: &str,
    scope: Option<&str>,
) -> Result<String, OAuthError> {
    let mut url = url::Url::parse(authorization_endpoint).map_err(|e| {
        OAuthError::MetadataError(format!("invalid authorization endpoint: {e}"))
    })?;

    url.query_pairs_mut()
        .append_pair("response_type", "code")
        .append_pair("client_id", client_id)
        .append_pair("redirect_uri", redirect_uri)
        .append_pair("code_challenge", code_challenge)
        .append_pair("code_challenge_method", "S256")
        .append_pair("state", state);

    if let Some(s) = scope {
        url.query_pairs_mut().append_pair("scope", s);
    }

    Ok(url.to_string())
}

// ---------------------------------------------------------------------------
// Browser opening
// ---------------------------------------------------------------------------

/// Open a URL in the user's default browser.
///
/// Uses `xdg-open` on Linux, `open` on macOS, and `start` on Windows.
/// Returns the child process handle.
///
/// # Source
/// Ported from `packages/opencode/src/mcp/index.ts` — `open()` (line 838)
/// and the `BrowserOpenFailed` event handling.
pub fn open_browser(url: &str) -> Result<std::process::Child, OAuthError> {
    #[cfg(target_os = "macos")]
    let result = std::process::Command::new("open")
        .arg(url)
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .spawn();

    #[cfg(target_os = "linux")]
    let result = std::process::Command::new("xdg-open")
        .arg(url)
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .spawn();

    #[cfg(target_os = "windows")]
    let result = std::process::Command::new("cmd")
        .args(["/c", "start", url])
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .spawn();

    #[cfg(not(any(target_os = "macos", target_os = "linux", target_os = "windows")))]
    let result = Err(std::io::Error::new(
        std::io::ErrorKind::Unsupported,
        "unsupported platform for browser opening",
    ));

    result.map_err(|e| OAuthError::BrowserOpenError(format!("{e}")))
}

// ---------------------------------------------------------------------------
// McpOAuthClient — high-level OAuth flow orchestration
// ---------------------------------------------------------------------------

/// High-level OAuth client for MCP servers.
///
/// Orchestrates the full OAuth flow:
/// 1. Fetch OAuth metadata (PKCE, endpoints)
/// 2. Build authorization URL with PKCE challenge
/// 3. Start callback server
/// 4. Open browser for user authorization
/// 5. Exchange authorization code for tokens
/// 6. Persist tokens via [`McpAuthStore`]
///
/// # Source
/// Ported from `packages/opencode/src/mcp/index.ts` — `startAuth()`,
/// `authenticate()`, `finishAuth()` functions (lines 748–892) combined
/// with `McpOAuthProvider` from `oauth-provider.ts`.
pub struct McpOAuthClient {
    /// HTTP client for API calls.
    http_client: reqwest::Client,
    /// Persistent auth storage.
    auth_store: McpAuthStore,
    /// State manager for callback correlation.
    state_manager: OAuthStateManager,
    /// MCP server name (for storage keys).
    mcp_name: String,
    /// Server URL.
    server_url: String,
    /// OAuth configuration.
    oauth_config: McpOAuthConfig,
    /// Oneshot receiver for the current pending OAuth flow.
    /// Set by [`initiate_oauth`](Self::initiate_oauth) and consumed
    /// by [`wait_for_callback`](Self::wait_for_callback).
    pending_rx: tokio::sync::Mutex<Option<tokio::sync::oneshot::Receiver<Result<String, OAuthError>>>>,
}

impl McpOAuthClient {
    /// Create a new OAuth client for the given MCP server.
    pub fn new(
        mcp_name: String,
        server_url: String,
        oauth_config: McpOAuthConfig,
        auth_store: McpAuthStore,
    ) -> Self {
        Self {
            http_client: reqwest::Client::new(),
            auth_store,
            state_manager: OAuthStateManager::new(),
            mcp_name,
            server_url,
            oauth_config,
            pending_rx: tokio::sync::Mutex::new(None),
        }
    }

    /// Get the effective redirect URI based on config.
    pub fn redirect_uri(&self, callback_port: u16) -> String {
        if let Some(ref explicit_uri) = self.oauth_config.redirect_uri {
            return explicit_uri.clone();
        }
        format!("http://127.0.0.1:{callback_port}{OAUTH_CALLBACK_PATH}")
    }

    /// Initiate the OAuth flow:
    /// 1. Fetch OAuth metadata from the server
    /// 2. Generate PKCE verifier + challenge
    /// 3. Generate state
    /// 4. Start the callback server
    /// 5. Build the authorization URL
    /// 6. Register the pending state in [`OAuthStateManager`]
    ///
    /// Returns `(authorization_url, state, code_verifier, actual_port)`.
    /// After calling this, open the browser with the authorization URL
    /// then call [`wait_for_callback`](Self::wait_for_callback).
    ///
    /// # Source
    /// Ported from `packages/opencode/src/mcp/index.ts` — `startAuth()` (line 748).
    pub async fn initiate_oauth(
        &self,
    ) -> Result<(String, String, String, u16), OAuthError> {
        // Step 1: Fetch OAuth metadata
        let metadata = fetch_oauth_metadata(&self.server_url, &self.http_client).await?;

        let authorization_endpoint = metadata.authorization_endpoint.as_deref().ok_or_else(|| {
            OAuthError::MetadataError(
                "server did not advertise an authorization endpoint".into(),
            )
        })?;

        // Step 2: Generate PKCE
        let code_verifier = generate_code_verifier();
        let code_challenge = generate_code_challenge(&code_verifier);

        // Step 3: Generate state
        let state = generate_random_state();

        // Step 4: Start the callback server
        let callback_port = self
            .oauth_config
            .callback_port
            .unwrap_or(OAUTH_CALLBACK_PORT);
        let (_server, actual_port) =
            OAuthCallbackServer::start(self.state_manager.clone(), callback_port, OAUTH_CALLBACK_PATH)
                .await?;

        let redirect_uri = self.redirect_uri(actual_port);

        // Step 5: Build authorization URL
        let client_id = self
            .oauth_config
            .client_id
            .as_deref()
            .unwrap_or("opencode");
        let scope = self.oauth_config.scope.as_deref();

        let auth_url = build_authorization_url(
            authorization_endpoint,
            client_id,
            &redirect_uri,
            &code_challenge,
            &state,
            scope,
        )?;

        // Step 6: Register pending state and store the receiver for wait_for_callback.
        let rx = self
            .state_manager
            .insert(state.clone(), self.server_url.clone(), code_verifier.clone())
            .await;
        *self.pending_rx.lock().await = Some(rx);

        // Persist state and verifier to the auth store for recovery
        self.auth_store
            .update_code_verifier(&self.mcp_name, code_verifier.clone())
            .await
            .map_err(|e| {
                OAuthError::ServerError(format!("failed to save code verifier: {e}"))
            })?;

        self.auth_store
            .update_oauth_state(&self.mcp_name, state.clone())
            .await
            .map_err(|e| OAuthError::ServerError(format!("failed to save state: {e}")))?;

        Ok((auth_url, state, code_verifier, actual_port))
    }

    /// Wait for the authorization callback and return the code.
    ///
    /// Blocks until the callback server receives the authorization code
    /// via the oneshot channel set up by [`initiate_oauth`](Self::initiate_oauth),
    /// or until the 5-minute timeout elapses.
    ///
    /// # Source
    /// Ported from `packages/opencode/src/mcp/index.ts` — the
    /// `McpOAuthCallback.waitForCallback()` call (line 836).
    pub async fn wait_for_callback(&self) -> Result<String, OAuthError> {
        let rx = self
            .pending_rx
            .lock()
            .await
            .take()
            .ok_or_else(|| OAuthError::NoPendingFlow {
                server_url: self.server_url.clone(),
            })?;

        match tokio::time::timeout(CALLBACK_TIMEOUT, rx).await {
            Ok(Ok(Ok(code))) => Ok(code),
            Ok(Ok(Err(e))) => Err(e),
            Ok(Err(_)) => {
                // Oneshot sender was dropped without sending (server stopped)
                Err(OAuthError::Cancelled)
            }
            Err(_) => {
                // Timeout
                self.state_manager
                    .cancel_by_server_url(&self.server_url)
                    .await;
                Err(OAuthError::Timeout)
            }
        }
    }

    /// Exchange the authorization code for tokens and persist them.
    ///
    /// Fetches OAuth metadata to find the token endpoint, then exchanges
    /// the code. Persists the resulting tokens via [`McpAuthStore`].
    ///
    /// # Source
    /// Ported from `packages/opencode/src/mcp/index.ts` — `finishAuth()`
    /// token exchange (line 870) and the `McpOAuthProvider.saveTokens()`.
    pub async fn exchange_and_save(
        &self,
        code: &str,
        code_verifier: &str,
        redirect_uri: &str,
    ) -> Result<OAuthTokenResponse, OAuthError> {
        let metadata = fetch_oauth_metadata(&self.server_url, &self.http_client).await?;
        let token_endpoint = metadata.token_endpoint.as_deref().ok_or_else(|| {
            OAuthError::MetadataError("server did not advertise a token endpoint".into())
        })?;

        let client_id = self
            .oauth_config
            .client_id
            .as_deref()
            .unwrap_or("opencode");

        let tokens = exchange_code(
            token_endpoint,
            client_id,
            code_verifier,
            code,
            redirect_uri,
            &self.http_client,
        )
        .await?;

        let expires_at = tokens
            .expires_in
            .map(|secs| chrono::Utc::now().timestamp() as f64 + secs as f64);

        let auth_tokens = crate::mcp::McpAuthTokens {
            access_token: tokens.access_token.clone(),
            refresh_token: tokens.refresh_token.clone(),
            expires_at,
            scope: tokens.scope.clone(),
        };

        self.auth_store
            .update_tokens(&self.mcp_name, auth_tokens, Some(&self.server_url))
            .await
            .map_err(|e| OAuthError::ServerError(format!("failed to save tokens: {e}")))?;

        // Clean up
        self.auth_store.clear_code_verifier(&self.mcp_name).await.ok();
        self.auth_store.clear_oauth_state(&self.mcp_name).await.ok();
        self.state_manager.cancel_by_server_url(&self.server_url).await;

        Ok(tokens)
    }

    /// Refresh the stored access token using the refresh token.
    ///
    /// # Source
    /// Ported from the MCP SDK's token refresh logic.
    pub async fn refresh_stored_token(&self) -> Result<OAuthTokenResponse, OAuthError> {
        let entry = self.auth_store.get(&self.mcp_name).await.ok_or_else(|| {
            OAuthError::NoPendingFlow {
                server_url: self.server_url.clone(),
            }
        })?;

        let tokens = entry.tokens.ok_or_else(|| OAuthError::NoPendingFlow {
            server_url: self.server_url.clone(),
        })?;

        let refresh_token_str = tokens.refresh_token.as_deref().ok_or_else(|| {
            OAuthError::TokenExchangeError("no refresh token available".into())
        })?;

        let metadata = fetch_oauth_metadata(&self.server_url, &self.http_client).await?;
        let token_endpoint = metadata.token_endpoint.as_deref().ok_or_else(|| {
            OAuthError::MetadataError("server did not advertise a token endpoint".into())
        })?;

        let client_id = self
            .oauth_config
            .client_id
            .as_deref()
            .unwrap_or("opencode");

        let new_tokens =
            refresh_token(token_endpoint, client_id, refresh_token_str, &self.http_client).await?;

        let expires_at = new_tokens
            .expires_in
            .map(|secs| chrono::Utc::now().timestamp() as f64 + secs as f64);

        let auth_tokens = crate::mcp::McpAuthTokens {
            access_token: new_tokens.access_token.clone(),
            refresh_token: new_tokens.refresh_token.or(tokens.refresh_token),
            expires_at,
            scope: new_tokens.scope.or(tokens.scope),
        };

        self.auth_store
            .update_tokens(&self.mcp_name, auth_tokens, Some(&self.server_url))
            .await
            .map_err(|e| OAuthError::ServerError(format!("failed to save refreshed tokens: {e}")))?;

        Ok(new_tokens)
    }

    /// Check whether the stored tokens are expired.
    pub async fn is_token_expired(&self) -> Option<bool> {
        self.auth_store.is_token_expired(&self.mcp_name).await
    }

    /// Get the auth status for this server.
    pub async fn auth_status(&self) -> crate::mcp::AuthStatus {
        let entry = self.auth_store.get(&self.mcp_name).await;
        match entry {
            Some(e) if e.tokens.is_some() => {
                let is_expired = e
                    .tokens
                    .as_ref()
                    .and_then(|t| t.expires_at)
                    .map(|exp| {
                        let now = chrono::Utc::now().timestamp() as f64;
                        now >= exp
                    })
                    .unwrap_or(false);
                if is_expired {
                    crate::mcp::AuthStatus::Expired
                } else {
                    crate::mcp::AuthStatus::Authenticated
                }
            }
            _ => crate::mcp::AuthStatus::NotAuthenticated,
        }
    }

    /// Remove stored auth data for this server and cancel any pending flow.
    pub async fn remove_auth(&self) -> Result<(), OAuthError> {
        self.auth_store
            .remove(&self.mcp_name)
            .await
            .map_err(|e| OAuthError::ServerError(format!("failed to remove auth: {e}")))?;
        self.state_manager
            .cancel_by_server_url(&self.server_url)
            .await;
        Ok(())
    }

    /// Get a reference to the state manager.
    pub fn state_manager(&self) -> &OAuthStateManager {
        &self.state_manager
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    // ── PKCE helpers ────────────────────────────────────────────────

    #[test]
    fn test_generate_code_verifier_length() {
        let verifier = generate_code_verifier();
        assert!(verifier.len() >= 43);
        assert!(verifier.len() <= 128);
    }

    #[test]
    fn test_generate_code_verifier_charset() {
        let verifier = generate_code_verifier();
        let valid_chars: &[u8] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789-._~";
        for c in verifier.chars() {
            assert!(valid_chars.contains(&(c as u8)), "invalid char: {c}");
        }
    }

    #[test]
    fn test_generate_code_verifier_unique() {
        let v1 = generate_code_verifier();
        let v2 = generate_code_verifier();
        assert_ne!(v1, v2);
    }

    #[test]
    fn test_generate_code_challenge_format() {
        let verifier = generate_code_verifier();
        let challenge = generate_code_challenge(&verifier);
        // BASE64URL(SHA256(...)) is always 43 chars with no padding
        assert_eq!(challenge.len(), 43);
        assert!(!challenge.contains('+'));
        assert!(!challenge.contains('/'));
        assert!(!challenge.contains('='));
    }

    #[test]
    fn test_generate_code_challenge_deterministic() {
        let verifier = "test-verifier-12345-abcdef-ghijklm-nopqrst-uvwxyz";
        let c1 = generate_code_challenge(verifier);
        let c2 = generate_code_challenge(verifier);
        assert_eq!(c1, c2);
    }

    #[test]
    fn test_generate_random_state_length() {
        let state = generate_random_state();
        assert_eq!(state.len(), 64);
        assert!(state.chars().all(|c| c.is_ascii_hexdigit()));
    }

    // ── OAuthStateManager ────────────────────────────────────────────

    #[tokio::test]
    async fn test_state_manager_insert_and_resolve() {
        let sm = OAuthStateManager::new();
        let rx = sm
            .insert("s1".into(), "https://example.com".into(), "v1".into())
            .await;
        let resolved = sm.resolve("s1", "authcode123".into()).await;
        assert!(resolved);
        let result = rx.await.expect("channel should resolve");
        assert!(result.is_ok());
        assert_eq!(result.unwrap(), "authcode123");
    }

    #[tokio::test]
    async fn test_state_manager_reject() {
        let sm = OAuthStateManager::new();
        let rx = sm
            .insert("s1".into(), "url".into(), "v1".into())
            .await;
        let rejected = sm
            .reject("s1", OAuthError::CompletionFailed("access denied".into()))
            .await;
        assert!(rejected);
        let result = rx.await.expect("channel should resolve");
        assert!(result.is_err());
    }

    #[tokio::test]
    async fn test_state_manager_unknown_state() {
        let sm = OAuthStateManager::new();
        let resolved = sm.resolve("nonexistent", "code".into()).await;
        assert!(!resolved);
    }

    #[tokio::test]
    async fn test_state_manager_cancel_by_server_url() {
        let sm = OAuthStateManager::new();
        let rx = sm
            .insert("s1".into(), "https://srv.example.com".into(), "v1".into())
            .await;
        sm.cancel_by_server_url("https://srv.example.com").await;
        let result = rx.await.expect("channel should resolve");
        assert!(matches!(result, Err(OAuthError::Cancelled)));
    }

    #[tokio::test]
    async fn test_state_manager_pending_count() {
        let sm = OAuthStateManager::new();
        assert_eq!(sm.pending_count().await, 0);
        sm.insert("a".into(), "u1".into(), "v1".into()).await;
        sm.insert("b".into(), "u2".into(), "v2".into()).await;
        assert_eq!(sm.pending_count().await, 2);
    }

    #[tokio::test]
    async fn test_state_manager_resolve_once() {
        let sm = OAuthStateManager::new();
        sm.insert("s1".into(), "url".into(), "v1".into()).await;
        let r1 = sm.resolve("s1", "code1".into()).await;
        assert!(r1);
        let r2 = sm.resolve("s1", "code2".into()).await;
        assert!(!r2, "second resolve should fail — entry removed");
    }

    // ── build_authorization_url ──────────────────────────────────────

    #[test]
    fn test_build_authorization_url_basic() {
        let url = build_authorization_url(
            "https://auth.example.com/authorize",
            "my-client",
            "http://127.0.0.1:19876/mcp/oauth/callback",
            "challenge123",
            "state456",
            None,
        )
        .unwrap();
        assert!(url.contains("response_type=code"));
        assert!(url.contains("client_id=my-client"));
        assert!(url.contains("code_challenge=challenge123"));
        assert!(url.contains("code_challenge_method=S256"));
        assert!(url.contains("state=state456"));
        assert!(url.contains("redirect_uri=http%3A%2F%2F127.0.0.1%3A19876"));
    }

    #[test]
    fn test_build_authorization_url_with_scope() {
        let url = build_authorization_url(
            "https://auth.example.com/authorize",
            "cid",
            "http://localhost:9876/cb",
            "ch",
            "st",
            Some("openid profile"),
        )
        .unwrap();
        assert!(url.contains("scope=openid+profile"));
    }

    #[test]
    fn test_build_authorization_url_invalid_endpoint() {
        let result = build_authorization_url("not-a-url", "c", "r", "ch", "st", None);
        assert!(result.is_err());
    }

    // ── OAuthTokenResponse deserialization ───────────────────────────

    #[test]
    fn test_oauth_token_response_deserialize() {
        let json = r#"{
            "access_token": "abc123",
            "token_type": "Bearer",
            "refresh_token": "ref456",
            "expires_in": 3600,
            "scope": "read write"
        }"#;
        let tokens: OAuthTokenResponse = serde_json::from_str(json).unwrap();
        assert_eq!(tokens.access_token, "abc123");
        assert_eq!(tokens.token_type, "Bearer");
        assert_eq!(tokens.refresh_token, Some("ref456".into()));
        assert_eq!(tokens.expires_in, Some(3600));
        assert_eq!(tokens.scope, Some("read write".into()));
    }

    #[test]
    fn test_oauth_token_response_minimal() {
        let json = r#"{
            "access_token": "tok",
            "token_type": "Bearer"
        }"#;
        let tokens: OAuthTokenResponse = serde_json::from_str(json).unwrap();
        assert_eq!(tokens.access_token, "tok");
        assert!(tokens.refresh_token.is_none());
        assert!(tokens.expires_in.is_none());
        assert!(tokens.scope.is_none());
    }

    // ── OAuthServerMetadata deserialization ──────────────────────────

    #[test]
    fn test_oauth_server_metadata_deserialize() {
        let json = r#"{
            "issuer": "https://auth.example.com",
            "authorization_endpoint": "https://auth.example.com/authorize",
            "token_endpoint": "https://auth.example.com/token",
            "code_challenge_methods_supported": ["S256"],
            "scopes_supported": ["openid", "profile", "email"],
            "token_endpoint_auth_methods_supported": ["client_secret_post", "none"],
            "registration_endpoint": "https://auth.example.com/register"
        }"#;
        let meta: OAuthServerMetadata = serde_json::from_str(json).unwrap();
        assert_eq!(meta.issuer, Some("https://auth.example.com".into()));
        assert_eq!(
            meta.authorization_endpoint,
            Some("https://auth.example.com/authorize".into())
        );
        assert_eq!(
            meta.token_endpoint,
            Some("https://auth.example.com/token".into())
        );
        assert!(meta
            .code_challenge_methods_supported
            .unwrap()
            .contains(&"S256".into()));
    }

    // ── open_browser is tested for compilation only ──────────────────

    #[test]
    fn test_open_browser_smoke() {
        // On CI without xdg-open/open/cmd this will error, but should
        // not panic. We just verify the function exists and type-checks.
        let _result = open_browser("http://localhost:0/nonexistent");
    }
}
