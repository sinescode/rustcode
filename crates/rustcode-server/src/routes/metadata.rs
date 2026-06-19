//! Metadata route — server version, capabilities, OpenAPI schema path,
//! and supported features.
//!
//! Ported from: `packages/opencode/src/server/routes/instance/httpapi/server.ts`
//! (the root-level metadata endpoint).

use axum::extract::State;
use axum::response::IntoResponse;
use axum::{Json, Router};
use axum::routing::get;
use std::sync::Arc;

use crate::server::AppState;

/// Create the metadata routes router.
///
/// # Source
/// Ported from `packages/opencode/src/server/routes/instance/httpapi/server.ts`
pub fn metadata_routes(state: Arc<AppState>) -> Router {
    Router::new()
        .route("/metadata", get(server_metadata))
        .with_state(state)
}

/// Return server metadata: version, capabilities, OpenAPI schema path,
/// and supported features.
///
/// # Source
/// Ported from `packages/opencode/src/server/routes/instance/httpapi/server.ts`
async fn server_metadata(State(state): State<Arc<AppState>>) -> impl IntoResponse {
    let capabilities = vec![
        "agents",
        "commands",
        "skills",
        "integrations",
        "references",
        "models",
        "providers",
        "sessions",
        "events",
        "mcp",
        "lsp",
        "tools",
        "permissions",
        "questions",
        "projects",
        "worktree",
        "pty",
        "file",
        "credential",
        "config",
        "sync",
        "tui",
    ];

    Json(serde_json::json!({
        "server": "rustcode-server",
        "version": state.version,
        "api_version": "v1",
        "openapi_schema": "/openapi.json",
        "capabilities": capabilities,
        "features": state.server_features,
        "provider_count": state.providers.len(),
        "provider_ids": state.providers.keys().collect::<Vec<_>>(),
        "endpoints": {
            "health": "/health",
            "metadata": "/metadata",
            "agent": "/agent",
            "command": "/command",
            "model": "/model",
            "skill": "/skill",
            "integration": "/integration",
            "reference": "/reference",
            "session": "/session",
            "project": "/experimental/project",
            "config": "/config",
            "credential": "/credential",
            "event": "/event",
            "file": "/file",
            "mcp": "/mcp",
            "permission": "/permission",
            "question": "/question",
            "provider": "/provider",
            "pty": "/pty",
            "query": "/query",
        },
    }))
    .into_response()
}
