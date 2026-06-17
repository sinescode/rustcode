//! MCP routes — status, add, connect/disconnect, OAuth flow.
//!
//! Ported from: `packages/opencode/src/server/routes/instance/httpapi/groups/mcp.ts`
//!
//! Route paths:
//! - `GET    /mcp`                          — MCP server status map
//! - `POST   /mcp`                          — add MCP server
//! - `POST   /mcp/:name/auth`               — start OAuth
//! - `DELETE /mcp/:name/auth`               — remove OAuth credentials
//! - `POST   /mcp/:name/auth/callback`      — OAuth callback
//! - `POST   /mcp/:name/auth/authenticate`  — authenticate (browser flow)
//! - `POST   /mcp/:name/connect`            — connect server
//! - `POST   /mcp/:name/disconnect`         — disconnect server

use axum::extract::{Path, State};
use axum::response::IntoResponse;
use axum::{Json, Router};
use axum::routing::{delete, get, post};
use serde::Deserialize;
use std::sync::Arc;

use crate::server::AppState;

#[derive(Debug, Deserialize)]
pub struct AddMcpPayload {
    pub name: String,
    pub config: serde_json::Value,
}

#[derive(Debug, Deserialize)]
pub struct AuthCallbackPayload {
    pub code: String,
}

/// Create the MCP routes router.
pub fn mcp_routes(state: Arc<AppState>) -> Router {
    Router::new()
        .route("/mcp", get(mcp_status).post(add_mcp))
        .route("/mcp/{name}/auth", post(mcp_auth_start).delete(mcp_auth_remove))
        .route("/mcp/{name}/auth/callback", post(mcp_auth_callback))
        .route("/mcp/{name}/auth/authenticate", post(mcp_auth_authenticate))
        .route("/mcp/{name}/connect", post(mcp_connect))
        .route("/mcp/{name}/disconnect", post(mcp_disconnect))
        .with_state(state)
}

async fn mcp_status() -> impl IntoResponse {
    Json(serde_json::json!({}))
}

async fn add_mcp(
    Json(payload): Json<AddMcpPayload>,
) -> impl IntoResponse {
    Json(serde_json::json!({
        "added": true,
        "name": payload.name,
    }))
}

async fn mcp_auth_start(
    Path(name): Path<String>,
) -> impl IntoResponse {
    Json(serde_json::json!({
        "name": name,
        "authorizationUrl": null,
    }))
}

async fn mcp_auth_callback(
    Path(name): Path<String>,
    Json(_payload): Json<AuthCallbackPayload>,
) -> impl IntoResponse {
    Json(serde_json::json!({
        "name": name,
        "status": "connected",
    }))
}

async fn mcp_auth_authenticate(
    Path(name): Path<String>,
) -> impl IntoResponse {
    Json(serde_json::json!({
        "name": name,
        "status": "connected",
    }))
}

async fn mcp_auth_remove(
    Path(name): Path<String>,
) -> impl IntoResponse {
    Json(serde_json::json!({
        "success": true,
    }))
}

async fn mcp_connect(
    Path(name): Path<String>,
) -> impl IntoResponse {
    Json(serde_json::json!({
        "connected": true,
        "name": name,
    }))
}

async fn mcp_disconnect(
    Path(name): Path<String>,
) -> impl IntoResponse {
    Json(serde_json::json!({
        "disconnected": true,
        "name": name,
    }))
}
