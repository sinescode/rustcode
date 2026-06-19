//! PTY routes — CRUD for pseudo-terminal sessions and WebSocket connect tokens.
//!
//! Ported from: `packages/opencode/src/server/routes/instance/httpapi/groups/pty.ts`
//!
//! Route paths (all under `/pty`):
//! - `GET    /pty/shells`              — list available shells
//! - `GET    /pty`                     — list PTY sessions
//! - `POST   /pty`                     — create a PTY session
//! - `GET    /pty/{ptyID}`             — get PTY info
//! - `PUT    /pty/{ptyID}`             — update PTY (resize)
//! - `DELETE /pty/{ptyID}`             — remove PTY
//! - `POST   /pty/{ptyID}/connect`     — get connect token
//! - `WS     /pty/{ptyID}/ws`          — WebSocket connection

use axum::extract::{Path, State, WebSocketUpgrade};
use axum::extract::ws::{Message, WebSocket};
use axum::response::{IntoResponse, Response};
use axum::{Json, Router};
use axum::routing::{get, post, put, delete};
use serde::Deserialize;
use std::sync::Arc;
use tracing::{info, warn};

use crate::server::AppState;

// ══════════════════════════════════════════════════════════════════════════════
// Error Types
// ══════════════════════════════════════════════════════════════════════════════

/// Error type for PTY route handlers.
///
/// # Source
/// `packages/opencode/src/server/routes/instance/httpapi/groups/pty.ts`
#[derive(Debug, thiserror::Error)]
pub enum PtyRouteError {
    /// PTY session not found.
    #[error("PTY not found: {id}")]
    NotFound { id: String },

    /// Access denied.
    #[error("Forbidden: {message}")]
    Forbidden { message: String },

    /// Invalid input payload.
    #[error("Bad request: {message}")]
    BadRequest { message: String },

    /// PTY creation failed.
    #[error("Internal error: {message}")]
    Internal { message: String },
}

impl IntoResponse for PtyRouteError {
    fn into_response(self) -> Response {
        let (status, body) = match &self {
            PtyRouteError::NotFound { id } => (
                axum::http::StatusCode::NOT_FOUND,
                serde_json::json!({ "error": format!("PTY not found: {id}") }),
            ),
            PtyRouteError::Forbidden { message } => (
                axum::http::StatusCode::FORBIDDEN,
                serde_json::json!({ "error": format!("Forbidden: {message}") }),
            ),
            PtyRouteError::BadRequest { message } => (
                axum::http::StatusCode::BAD_REQUEST,
                serde_json::json!({ "error": format!("Bad request: {message}") }),
            ),
            PtyRouteError::Internal { message } => (
                axum::http::StatusCode::INTERNAL_SERVER_ERROR,
                serde_json::json!({ "error": format!("Internal error: {message}") }),
            ),
        };
        (status, Json(body)).into_response()
    }
}

// ══════════════════════════════════════════════════════════════════════════════
// Types
// ══════════════════════════════════════════════════════════════════════════════

/// Payload for creating or updating a PTY session.
///
/// # Source
/// `packages/core/src/pty.ts` `CreateInput` and `UpdateInput`
#[derive(Debug, Deserialize)]
pub struct PtyInput {
    pub command: Option<String>,
    pub cwd: Option<String>,
    #[serde(default)]
    pub env: Option<std::collections::HashMap<String, String>>,
    #[serde(default)]
    pub cols: Option<u16>,
    #[serde(default)]
    pub rows: Option<u16>,
}

// ══════════════════════════════════════════════════════════════════════════════
// Router
// ══════════════════════════════════════════════════════════════════════════════

/// Create the PTY routes router.
///
/// # Source
/// Ported from `packages/opencode/src/server/routes/instance/httpapi/groups/pty.ts`
pub fn pty_routes(state: Arc<AppState>) -> Router {
    Router::new()
        // Shell listing (must be before /pty/{ptyID} to avoid path conflict)
        .route("/pty/shells", get(list_shells))
        // Collection
        .route("/pty", get(list_ptys).post(create_pty))
        // Single PTY
        .route(
            "/pty/{ptyID}",
            get(get_pty).put(update_pty).delete(remove_pty),
        )
        // Connect token
        .route("/pty/{ptyID}/connect", post(connect_token_pty))
        // WebSocket
        .route("/pty/{ptyID}/ws", get(ws_handler))
        .with_state(state)
}

// ══════════════════════════════════════════════════════════════════════════════
// Handlers
// ══════════════════════════════════════════════════════════════════════════════

/// List available shells on the system.
///
/// # Source
/// `packages/opencode/src/server/routes/instance/httpapi/groups/pty.ts`
/// `shells` endpoint.
async fn list_shells() -> impl IntoResponse {
    let config = rustcode_core::shell::ShellConfig::default();
    let service = rustcode_core::shell::ShellService::new(config);
    let shells = service.detect();
    Json(serde_json::json!({ "shells": shells }))
}

/// List all active PTY sessions.
///
/// # Source
/// `packages/opencode/src/server/routes/instance/httpapi/groups/pty.ts`
/// `list` endpoint.
async fn list_ptys(State(_state): State<Arc<AppState>>) -> impl IntoResponse {
    // TODO: integrate with PTY manager service
    Json(serde_json::json!({ "ptys": [] }))
}

/// Create a new PTY session.
///
/// # Source
/// `packages/opencode/src/server/routes/instance/httpapi/groups/pty.ts`
/// `create` endpoint.
async fn create_pty(
    State(_state): State<Arc<AppState>>,
    Json(payload): Json<PtyInput>,
) -> impl IntoResponse {
    info!(
        "PTY create requested: command={:?}, cwd={:?}",
        payload.command, payload.cwd
    );
    // TODO: spawn PTY process via PTY manager
    (
        axum::http::StatusCode::NOT_IMPLEMENTED,
        Json(serde_json::json!({
            "error": "PTY creation not yet implemented",
            "command": payload.command,
            "cwd": payload.cwd,
        })),
    )
        .into_response()
}

/// Get details for a single PTY session.
///
/// # Source
/// `packages/opencode/src/server/routes/instance/httpapi/groups/pty.ts`
/// `get` endpoint.
async fn get_pty(
    State(_state): State<Arc<AppState>>,
    Path(pty_id): Path<String>,
) -> impl IntoResponse {
    info!("PTY get requested for '{pty_id}'");
    // TODO: look up PTY by id from PTY manager
    (
        axum::http::StatusCode::NOT_FOUND,
        Json(serde_json::json!({
            "error": format!("PTY '{pty_id}' not found"),
        })),
    )
        .into_response()
}

/// Update an existing PTY session (e.g. resize).
///
/// # Source
/// `packages/opencode/src/server/routes/instance/httpapi/groups/pty.ts`
/// `update` endpoint.
async fn update_pty(
    State(_state): State<Arc<AppState>>,
    Path(pty_id): Path<String>,
    Json(payload): Json<PtyInput>,
) -> impl IntoResponse {
    info!(
        "PTY update requested for '{pty_id}': cols={:?}, rows={:?}",
        payload.cols, payload.rows
    );
    // TODO: resize or reconfigure PTY via PTY manager
    (
        axum::http::StatusCode::NOT_IMPLEMENTED,
        Json(serde_json::json!({
            "error": "PTY update not yet implemented",
            "pty_id": pty_id,
            "cols": payload.cols,
            "rows": payload.rows,
        })),
    )
        .into_response()
}

/// Remove a PTY session.
///
/// # Source
/// `packages/opencode/src/server/routes/instance/httpapi/groups/pty.ts`
/// `remove` endpoint.
async fn remove_pty(
    State(_state): State<Arc<AppState>>,
    Path(pty_id): Path<String>,
) -> impl IntoResponse {
    info!("PTY remove requested for '{pty_id}'");
    // TODO: kill PTY process and clean up via PTY manager
    Json(serde_json::json!({
        "removed": true,
        "pty_id": pty_id,
    }))
}

/// Get a WebSocket connect token for an existing PTY.
///
/// # Source
/// `packages/opencode/src/server/routes/instance/httpapi/groups/pty.ts`
/// `connectToken` endpoint.
async fn connect_token_pty(
    State(_state): State<Arc<AppState>>,
    Path(pty_id): Path<String>,
) -> impl IntoResponse {
    info!("PTY connect-token requested for '{pty_id}'");
    // TODO: generate a short-lived token for WebSocket attachment
    (
        axum::http::StatusCode::NOT_IMPLEMENTED,
        Json(serde_json::json!({
            "error": "PTY connect-token not yet implemented",
            "pty_id": pty_id,
        })),
    )
        .into_response()
}

/// Handle WebSocket upgrade for PTY connection.
///
/// # Source
/// `packages/opencode/src/server/routes/instance/httpapi/groups/pty.ts`
/// WebSocket handler.
async fn ws_handler(
    ws: WebSocketUpgrade,
    Path(pty_id): Path<String>,
    State(_state): State<Arc<AppState>>,
) -> Response {
    info!("WebSocket connection requested for PTY '{pty_id}'");
    ws.on_upgrade(move |socket| handle_ws(socket, pty_id))
}

/// Handle an individual WebSocket connection for a PTY session.
///
/// This is a stub that accepts connections but doesn't bridge to a real PTY.
async fn handle_ws(mut socket: WebSocket, pty_id: String) {
    info!("WebSocket connected for PTY '{pty_id}'");

    // Stub: echo messages back and log them
    while let Some(msg_result) = socket.recv().await {
        match msg_result {
            Ok(Message::Text(text)) => {
                info!("PTY '{pty_id}' received text: {} bytes", text.len());
                // Echo back for now — real impl will write to PTY stdin
                if let Err(e) = socket.send(Message::Text(text)).await {
                    warn!("WebSocket send error for PTY '{pty_id}': {e}");
                    break;
                }
            }
            Ok(Message::Binary(data)) => {
                info!("PTY '{pty_id}' received binary: {} bytes", data.len());
                // Echo back for now — real impl will write to PTY stdin
                if let Err(e) = socket.send(Message::Binary(data)).await {
                    warn!("WebSocket send error for PTY '{pty_id}': {e}");
                    break;
                }
            }
            Ok(Message::Close(_)) => {
                info!("WebSocket closed for PTY '{pty_id}'");
                break;
            }
            Ok(_) => {
                // Ping/pong handled automatically by axum
            }
            Err(e) => {
                warn!("WebSocket error for PTY '{pty_id}': {e}");
                break;
            }
        }
    }

    info!("WebSocket handler finished for PTY '{pty_id}'");
}
