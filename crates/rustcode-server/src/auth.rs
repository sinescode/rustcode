//! Auth middleware — checks `OPENCODE_SERVER_PASSWORD` env var.
//!
//! Clients authenticate via:
//! - `Authorization: Basic <base64>` header (username: `opencode` by default)
//! - `auth_token` query parameter (base64-encoded `username:password`)
//!
//! Public endpoints (`/health`, `/version`, `/global/health`) bypass auth.
//!
//! Ported from: `packages/opencode/src/server/routes/instance/httpapi/middleware/authorization.ts`
//! and `packages/server/src/middleware/authorization.ts`

use axum::extract::Request;
use axum::http::{header, StatusCode};
use axum::middleware::Next;
use axum::response::{IntoResponse, Response};

/// Set of public path prefixes that bypass auth.
const PUBLIC_PATH_PREFIXES: &[&str] = &["/health", "/version", "/global/health"];

/// Set of exact public paths that bypass auth.
const PUBLIC_EXACT_PATHS: &[&str] = &["/", "/favicon.ico"];

/// WWW-Authenticate header value returned on 401.
const WWW_AUTHENTICATE: &str = r#"Basic realm="Secure Area""#;

/// Auth configuration derived from environment variables at startup.
#[derive(Debug, Clone)]
pub struct AuthConfig {
    /// The expected password. If `None` or empty, auth is disabled.
    pub password: Option<String>,
    /// The expected username (defaults to `"opencode"`).
    pub username: String,
}

impl AuthConfig {
    /// Load auth config from environment variables or file.
    ///
    /// Reads:
    /// - `OPENCODE_SERVER_PASSWORD` (env var, less secure — visible in /proc)
    /// - `OPENCODE_SERVER_PASSWORD_FILE` (file path, more secure)
    /// - `OPENCODE_SERVER_USERNAME` (defaults to `"opencode"`)
    pub fn from_env() -> Self {
        let password = std::env::var("OPENCODE_SERVER_PASSWORD")
            .ok()
            .or_else(|| {
                std::env::var("OPENCODE_SERVER_PASSWORD_FILE").ok().and_then(|path| {
                    std::fs::read_to_string(&path).ok().map(|s| s.trim().to_string())
                })
            });
        let username = std::env::var("OPENCODE_SERVER_USERNAME")
            .unwrap_or_else(|_| "opencode".to_string());
        Self { password, username }
    }

    /// Returns `true` if auth is required (password is set and non-empty).
    pub fn required(&self) -> bool {
        self.password
            .as_ref()
            .is_some_and(|p| !p.is_empty())
    }

    /// Returns `true` if the given credentials are authorized.
    pub fn authorized(&self, username: &str, password: &str) -> bool {
        if let Some(ref expected_password) = self.password {
            username == self.username && password == expected_password
        } else {
            true // No password configured — everyone is authorized
        }
    }
}

/// Decoded credentials from a request.
#[derive(Debug, Clone, Default)]
pub struct Credentials {
    pub username: String,
    pub password: String,
}

impl Credentials {
    /// Extract credentials from a request.
    ///
    /// Priority:
    /// 1. `auth_token` query parameter (base64-encoded `username:password`)
    /// 2. `Authorization: Basic <base64>` header
    pub fn from_request(req: &Request) -> Self {
        // Try query param first (less secure — credentials can leak in logs/Referer)
        let uri = req.uri();
        if let Some(query) = uri.query() {
            for pair in query.split('&') {
                if let Some((key, value)) = pair.split_once('=') {
                    if key == "auth_token" && !value.is_empty() {
                        tracing::warn!("auth_token query parameter used — less secure than Authorization header");
                        let decoded = url_decode(value).unwrap_or_default();
                        return Self::from_basic(&decoded);
                    }
                }
            }
        }

        // Try Authorization header
        if let Some(auth_header) = req.headers().get(header::AUTHORIZATION) {
            if let Ok(auth_str) = auth_header.to_str() {
                if let Some(b64) = auth_str.strip_prefix("Basic ") {
                    return Self::from_basic(b64);
                }
            }
        }

        Credentials::default()
    }

    /// Parse a base64-encoded `username:password` string.
    fn from_basic(encoded: &str) -> Self {
        use base64::Engine;
        let engine = base64::engine::general_purpose::STANDARD;
        match engine.decode(encoded) {
            Ok(decoded) => {
                let decoded_str = String::from_utf8_lossy(&decoded);
                if let Some((username, password)) = decoded_str.split_once(':') {
                    Credentials {
                        username: username.to_string(),
                        password: password.to_string(),
                    }
                } else {
                    Credentials::default()
                }
            }
            Err(_) => Credentials::default(),
        }
    }
}

/// Simple URL-decoding for the `auth_token` query parameter value.
fn url_decode(input: &str) -> Option<String> {
    let mut result = String::with_capacity(input.len());
    let mut chars = input.chars();
    while let Some(c) = chars.next() {
        if c == '%' {
            let hex: String = chars.by_ref().take(2).collect();
            if hex.len() == 2 {
                let byte = u8::from_str_radix(&hex, 16).ok()?;
                result.push(byte as char);
            } else {
                return None;
            }
        } else if c == '+' {
            result.push(' ');
        } else {
            result.push(c);
        }
    }
    Some(result)
}

/// Check whether the given request path is a public endpoint.
fn is_public_path(req: &Request) -> bool {
    let path = req.uri().path();

    // Check exact public paths
    if PUBLIC_EXACT_PATHS.contains(&path) {
        return true;
    }

    // Check public path prefixes
    for prefix in PUBLIC_PATH_PREFIXES {
        if path.starts_with(prefix) {
            return true;
        }
    }

    false
}

/// Auth middleware for axum.
///
/// Checks `OPENCODE_SERVER_PASSWORD` and authenticates requests.
/// Public endpoints bypass auth.
pub async fn auth_middleware(
    req: Request,
    next: Next,
) -> Response {
    // Load config once at startup, cached for subsequent requests
    let config = AUTH_CONFIG.get_or_init(|| AuthConfig::from_env());

    // Skip auth if not required or if path is public
    if !config.required() || is_public_path(&req) {
        return next.run(req).await;
    }

    // Extract credentials
    let creds = Credentials::from_request(&req);

    // Validate
    if config.authorized(&creds.username, &creds.password) {
        return next.run(req).await;
    }

    // Unauthorized — return 401 with JSON body and WWW-Authenticate header
    let body = serde_json::json!({
        "error": "unauthorized"
    });

    let mut response = (StatusCode::UNAUTHORIZED, axum::Json(body)).into_response();
    response.headers_mut().insert(
        header::WWW_AUTHENTICATE,
        WWW_AUTHENTICATE.parse().unwrap(),
    );
    response
}
