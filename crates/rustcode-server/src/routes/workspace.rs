//! Workspace routes — adapters, list, create, sync, status, remove, warp.
//!
//! Ported from: `packages/opencode/src/server/routes/instance/httpapi/groups/workspace.ts`

use axum::extract::{Path, State};
use axum::response::IntoResponse;
use axum::{Json, Router};
use axum::routing::{delete, get, post};
use serde::Deserialize;
use std::sync::Arc;

use crate::server::AppState;

#[derive(Debug, Deserialize)]
pub struct CreateWorkspacePayload {
    pub name: String,
    #[serde(default)] pub r#type: Option<String>,
}
#[derive(Debug, Deserialize)]
pub struct WarpPayload {
    pub id: Option<String>,
    #[serde(rename = "sessionID")] pub session_id: String,
    #[serde(default, rename = "copyChanges")] pub copy_changes: Option<bool>,
}

pub fn workspace_routes(state: Arc<AppState>) -> Router {
    Router::new()
        .route("/experimental/workspace/adapter", get(list_adapters))
        .route("/experimental/workspace", get(list_workspaces).post(create_workspace))
        .route("/experimental/workspace/sync-list", post(sync_list))
        .route("/experimental/workspace/status", get(workspace_status))
        .route("/experimental/workspace/{id}", delete(remove_workspace))
        .route("/experimental/workspace/warp", post(warp_session))
        .with_state(state)
}

async fn list_adapters(State(_): State<Arc<AppState>>) -> impl IntoResponse { Json(serde_json::json!([])) }
async fn list_workspaces(State(_): State<Arc<AppState>>) -> impl IntoResponse { Json(serde_json::json!([])) }
async fn create_workspace(
    State(_): State<Arc<AppState>>,
    Json(payload): Json<CreateWorkspacePayload>,
) -> impl IntoResponse {
    Json(serde_json::json!({ "id": uuid::Uuid::new_v4().to_string(), "name": payload.name, "type": payload.r#type }))
}
async fn sync_list(State(_): State<Arc<AppState>>) -> impl IntoResponse { Json(serde_json::json!(null)) }
async fn workspace_status(State(_): State<Arc<AppState>>) -> impl IntoResponse { Json(serde_json::json!([])) }
async fn remove_workspace(
    State(_): State<Arc<AppState>>,
    Path(id): Path<String>,
) -> impl IntoResponse { Json(serde_json::json!({ "removed": true, "id": id })) }
async fn warp_session(
    State(_): State<Arc<AppState>>,
    Json(payload): Json<WarpPayload>,
) -> impl IntoResponse {
    Json(serde_json::json!({ "warped": true, "session_id": payload.session_id }))
}
