//! MCP routes — status, add, connect/disconnect, OAuth flow.
//!
//! Ported from: `packages/opencode/src/server/routes/instance/httpapi/groups/mcp.ts`

use axum::extract::{Path, Query, State};
use axum::http::StatusCode;
use axum::response::IntoResponse;
use axum::routing::{get, post};
use axum::{Json, Router};
use serde::Deserialize;
use std::collections::HashMap;
use std::sync::Arc;
use tracing::{info, warn};

use crate::server::AppState;
use rustcode_core::mcp::{sanitize_name, McpServerConfig, McpServerRegistry};

#[derive(Debug, Deserialize)]
pub struct AddMcpPayload {
    pub name: String,
    pub config: serde_json::Value,
}

#[derive(Debug, Deserialize)]
pub struct AuthCallbackPayload {
    pub code: String,
}

/// Build the MCP routes, wiring the persistent registry into the router.
pub fn mcp_routes(state: Arc<AppState>) -> Router {
    // A single persistent registry shared across all MCP route handlers.
    let mcp_registry: Arc<McpServerRegistry> = Arc::new(McpServerRegistry::new());

    Router::new()
        .route(
            "/mcp",
            get({
                let store = mcp_registry.clone();
                move |state, query| mcp_status(store, state, query)
            })
            .post({
                let store = mcp_registry.clone();
                move |state, payload| add_mcp(store, state, payload)
            }),
        )
        .route(
            "/mcp/{name}/auth",
            post({
                let store = mcp_registry.clone();
                move |path, state| mcp_auth_start(store, path, state)
            })
            .delete({
                let store = mcp_registry.clone();
                move |path, state| mcp_auth_remove(store, path, state)
            }),
        )
        .route(
            "/mcp/{name}/auth/callback",
            post({
                let store = mcp_registry.clone();
                move |path, state, payload| mcp_auth_callback(store, path, state, payload)
            }),
        )
        .route("/mcp/{name}/auth/authenticate", post(mcp_auth_authenticate))
        .route(
            "/mcp/{name}/connect",
            post({
                let store = mcp_registry.clone();
                move |path, state| mcp_connect(store, path, state)
            }),
        )
        .route(
            "/mcp/{name}/disconnect",
            post({
                let store = mcp_registry;
                move |path, state| mcp_disconnect(store, path, state)
            }),
        )
        .with_state(state)
}

/// `GET /mcp` — list all MCP servers with their actual connection status
/// and discovered tools.
async fn mcp_status(
    store: Arc<McpServerRegistry>,
    State(_state): State<Arc<AppState>>,
    Query(_query): Query<HashMap<String, String>>,
) -> impl IntoResponse {
    let summaries = store.list_servers().await;
    info!("MCP status: {} servers", summaries.len());
    Json(serde_json::json!({ "servers": summaries }))
}

/// `POST /mcp` — add (register) a new MCP server configuration.
///
/// After adding, the server is available for `POST /mcp/{name}/connect`.
async fn add_mcp(
    store: Arc<McpServerRegistry>,
    State(_state): State<Arc<AppState>>,
    Json(payload): Json<AddMcpPayload>,
) -> impl IntoResponse {
    info!("MCP: adding server '{}'", payload.name);

    // Validate: config must have a "type" field
    let config_type = match payload.config.get("type").and_then(|v| v.as_str()) {
        Some(t @ ("local" | "remote")) => t.to_string(),
        Some(bad) => {
            return (
                StatusCode::BAD_REQUEST,
                Json(serde_json::json!({
                    "error": format!("invalid MCP server type: '{bad}' — expected 'local' or 'remote'")
                })),
            ).into_response();
        }
        None => {
            return (
                StatusCode::BAD_REQUEST,
                Json(serde_json::json!({
                    "error": "MCP server config must have a 'type' field"
                })),
            )
                .into_response();
        }
    };

    // Deserialize the full config into McpServerConfig
    let config: McpServerConfig = match serde_json::from_value(payload.config.clone()) {
        Ok(cfg) => cfg,
        Err(e) => {
            return (
                StatusCode::BAD_REQUEST,
                Json(serde_json::json!({
                    "error": format!("invalid MCP server config: {e}")
                })),
            )
                .into_response();
        }
    };

    // Additional validation based on type
    match config_type.as_str() {
        "local" => {
            if config.command.is_none() || config.command.as_ref().is_some_and(|c| c.is_empty()) {
                return (
                    StatusCode::BAD_REQUEST,
                    Json(serde_json::json!({
                        "error": "local MCP server config must have a non-empty 'command' field"
                    })),
                )
                    .into_response();
            }
        }
        "remote" => {
            if config.url.is_none() {
                return (
                    StatusCode::BAD_REQUEST,
                    Json(serde_json::json!({
                        "error": "remote MCP server config must have a 'url' field"
                    })),
                )
                    .into_response();
            }
        }
        _ => unreachable!("type already validated"),
    }

    let name = payload.name.clone();
    store.add_config(name.clone(), config).await;

    Json(serde_json::json!({
        "added": true,
        "name": name,
    }))
    .into_response()
}

/// `POST /mcp/{name}/connect` — connect to an MCP server, discover its
/// tools, and register them in the ToolRegistry so they are available
/// to the LLM.
async fn mcp_connect(
    store: Arc<McpServerRegistry>,
    State(state): State<Arc<AppState>>,
    Path(name): Path<String>,
) -> impl IntoResponse {
    info!("MCP: connecting to '{name}'");

    // 1. Connect via the registry
    let client = match store.connect(&name).await {
        Ok(c) => c,
        Err(e) => {
            warn!("MCP: failed to connect to '{name}': {e}");
            let status = if matches!(e, rustcode_core::error::Error::McpNotFound { .. }) {
                StatusCode::NOT_FOUND
            } else {
                StatusCode::BAD_GATEWAY
            };
            return (
                status,
                Json(serde_json::json!({
                    "error": format!("failed to connect to MCP server '{name}': {e}")
                })),
            )
                .into_response();
        }
    };

    // 2. Register discovered tools in the ToolRegistry
    let tools = client.cached_tools().await;
    let tools_registered = register_mcp_tools(&state.tools, Arc::clone(&client)).await;

    info!(
        "MCP: connected to '{name}' — discovered {} tools ({} registered in ToolRegistry)",
        tools.len(),
        tools_registered,
    );

    Json(serde_json::json!({
        "connected": true,
        "name": name,
        "tools": tools,
        "tools_registered": tools_registered,
    }))
    .into_response()
}

/// `POST /mcp/{name}/disconnect` — disconnect from an MCP server, kill
/// its subprocess, and unregister its tools from the ToolRegistry.
async fn mcp_disconnect(
    store: Arc<McpServerRegistry>,
    State(state): State<Arc<AppState>>,
    Path(name): Path<String>,
) -> impl IntoResponse {
    info!("MCP: disconnecting from '{name}'");

    // 1. Unregister tools from the ToolRegistry
    let unregistered = unregister_mcp_tools(&state.tools, &name).await;

    // 2. Disconnect via the registry (kills subprocess, cleans up state)
    match store.disconnect(&name).await {
        Ok(()) => {
            info!("MCP: disconnected from '{name}' — unregistered {unregistered} tools");
            Json(serde_json::json!({
                "disconnected": true,
                "name": name,
                "tools_unregistered": unregistered,
            }))
            .into_response()
        }
        Err(e) => {
            warn!("MCP: error disconnecting from '{name}': {e}");
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(serde_json::json!({
                    "error": format!("failed to disconnect MCP server '{name}': {e}")
                })),
            )
                .into_response()
        }
    }
}

// ── OAuth stubs (not yet implemented in core) ──────────────────────────────

async fn mcp_auth_start(
    _store: Arc<McpServerRegistry>,
    State(_state): State<Arc<AppState>>,
    Path(name): Path<String>,
) -> impl IntoResponse {
    info!("MCP: auth start for '{name}'");
    Json(serde_json::json!({
        "name": name,
        "authorizationUrl": format!("https://mcp.{name}.local/oauth/authorize"),
    }))
}

async fn mcp_auth_callback(
    _store: Arc<McpServerRegistry>,
    State(_state): State<Arc<AppState>>,
    Path(name): Path<String>,
    Json(_payload): Json<AuthCallbackPayload>,
) -> impl IntoResponse {
    info!("MCP: auth callback for '{name}'");
    Json(serde_json::json!({ "name": name, "status": "connected" }))
}

async fn mcp_auth_authenticate(
    State(_state): State<Arc<AppState>>,
    Path(name): Path<String>,
) -> impl IntoResponse {
    info!("MCP: authenticate for '{name}'");
    Json(serde_json::json!({ "name": name, "status": "connected" }))
}

async fn mcp_auth_remove(
    _store: Arc<McpServerRegistry>,
    State(_state): State<Arc<AppState>>,
    Path(name): Path<String>,
) -> impl IntoResponse {
    info!("MCP: auth removed for '{name}'");
    Json(serde_json::json!({ "success": true }))
}

// ── ToolRegistry integration helpers ───────────────────────────────────────

/// Register MCP-discovered tools in the global [`ToolRegistry`] so they
/// are available for LLM function calling.
///
/// Uses [`McpClient::to_plugin_defs()`] to convert MCP tools to the
/// registry format, then registers each one.
///
/// Returns the number of tools successfully registered.
async fn register_mcp_tools(
    tool_registry: &rustcode_core::tool::ToolRegistry,
    client: std::sync::Arc<rustcode_core::mcp::McpClient>,
) -> usize {
    let plugin_defs = client.to_plugin_defs().await;
    let count = plugin_defs.len();
    for plugin_def in plugin_defs {
        tool_registry.register_plugin(plugin_def);
    }
    count
}

/// Unregister all tools associated with an MCP server from the global
/// [`ToolRegistry`].
///
/// Returns the number of tools that were removed.
async fn unregister_mcp_tools(
    tool_registry: &rustcode_core::tool::ToolRegistry,
    server_name: &str,
) -> usize {
    let prefix = format!("{}_", sanitize_name(server_name));
    let all_tool_ids = tool_registry.ids();
    let mut count = 0;

    for id in &all_tool_ids {
        if id.starts_with(&prefix) {
            tool_registry.unregister_plugin(id);
            count += 1;
        }
    }

    count
}
