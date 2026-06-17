//! Experimental routes — console, tools, worktree, global sessions, resources.
//!
//! Ported from: `packages/opencode/src/server/routes/instance/httpapi/groups/experimental.ts`

use axum::extract::{Path, Query, State};
use axum::response::IntoResponse;
use axum::{Json, Router};
use axum::routing::{delete, get, post};
use serde::Deserialize;
use std::sync::Arc;

use crate::server::AppState;

#[derive(Debug, Deserialize)]
pub struct ConsoleSwitchPayload {
    #[serde(rename = "accountID")] pub account_id: String,
    #[serde(rename = "orgID")] pub org_id: String,
}
#[derive(Debug, Deserialize)]
pub struct ToolListQuery {
    pub provider: String,
    pub model: String,
    #[serde(default)] pub directory: Option<String>,
    #[serde(default)] pub workspace: Option<String>,
}
#[derive(Debug, Deserialize, Default)]
pub struct SessionListQuery {
    #[serde(default)] pub directory: Option<String>,
    #[serde(default)] pub workspace: Option<String>,
    #[serde(default)] pub roots: Option<bool>,
    #[serde(default)] pub start: Option<u64>,
    #[serde(default)] pub cursor: Option<u64>,
    #[serde(default)] pub search: Option<String>,
    #[serde(default)] pub limit: Option<usize>,
    #[serde(default)] pub archived: Option<bool>,
}

pub fn experimental_routes(state: Arc<AppState>) -> Router {
    Router::new()
        .route("/experimental/console", get(console_state))
        .route("/experimental/console/orgs", get(console_orgs))
        .route("/experimental/console/switch", post(console_switch))
        .route("/experimental/tool", get(list_tools))
        .route("/experimental/tool/ids", get(list_tool_ids))
        .route("/experimental/worktree", get(list_worktrees).post(create_worktree).delete(remove_worktree))
        .route("/experimental/worktree/reset", post(reset_worktree))
        .route("/experimental/session", get(global_session_list))
        .route("/experimental/session/{sessionID}/background", post(background_subagents))
        .route("/experimental/resource", get(list_resources))
        .with_state(state)
}

macro_rules! stub {
    ($name:ident, $state:ident, $($arg:ident: $ty:ty),*) => {
        async fn $name(State(_): State<Arc<AppState>>, $($arg: $ty),*) -> impl IntoResponse { Json(serde_json::json!({})) }
    };
}

async fn console_state(State(_): State<Arc<AppState>>) -> impl IntoResponse {
    Json(serde_json::json!({ "consoleManagedProviders": [], "switchableOrgCount": 0 }))
}
async fn console_orgs(State(_): State<Arc<AppState>>) -> impl IntoResponse {
    Json(serde_json::json!({ "orgs": [] }))
}
async fn console_switch(
    State(_): State<Arc<AppState>>,
    Json(payload): Json<ConsoleSwitchPayload>,
) -> impl IntoResponse {
    Json(serde_json::json!({ "switched": true, "account_id": payload.account_id, "org_id": payload.org_id }))
}
async fn list_tools(
    State(_): State<Arc<AppState>>,
    Query(query): Query<ToolListQuery>,
) -> impl IntoResponse {
    Json(serde_json::json!({ "provider": query.provider, "model": query.model, "tools": [] }))
}
async fn list_tool_ids(State(_): State<Arc<AppState>>) -> impl IntoResponse { Json(serde_json::json!([])) }
async fn list_worktrees(State(_): State<Arc<AppState>>) -> impl IntoResponse { Json(serde_json::json!([])) }
async fn create_worktree(
    State(_): State<Arc<AppState>>,
    Json(_payload): Json<serde_json::Value>,
) -> impl IntoResponse { Json(serde_json::json!({ "created": true, "directory": "" })) }
async fn remove_worktree(
    State(_): State<Arc<AppState>>,
    Json(_payload): Json<serde_json::Value>,
) -> impl IntoResponse { Json(serde_json::json!({ "removed": true })) }
async fn reset_worktree(
    State(_): State<Arc<AppState>>,
    Json(_payload): Json<serde_json::Value>,
) -> impl IntoResponse { Json(serde_json::json!({ "reset": true })) }
async fn global_session_list(
    State(_): State<Arc<AppState>>,
    Query(_query): Query<SessionListQuery>,
) -> impl IntoResponse { Json(serde_json::json!([])) }
async fn background_subagents(
    State(_): State<Arc<AppState>>,
    Path(session_id): Path<String>,
) -> impl IntoResponse { Json(serde_json::json!({ "backgrounded": true, "session_id": session_id })) }
async fn list_resources(State(_): State<Arc<AppState>>) -> impl IntoResponse { Json(serde_json::json!({})) }
