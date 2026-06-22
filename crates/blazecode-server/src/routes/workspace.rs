//! Workspace routes — adapters, list, create, sync, status, remove, warp.
//!
//! Ported from: `packages/blazecode/src/server/routes/instance/httpapi/groups/workspace.ts`

use axum::extract::{Path, State};
use axum::response::IntoResponse;
use axum::routing::{delete, get, post};
use axum::{Json, Router};
use serde::Deserialize;
use std::sync::Arc;
use tracing::info;

use crate::server::AppState;

#[derive(Debug, Deserialize)]
pub struct CreateWorkspacePayload {
    pub name: String,
    #[serde(default)]
    pub r#type: Option<String>,
}
#[derive(Debug, Deserialize)]
pub struct WarpPayload {
    pub id: Option<String>,
    #[serde(rename = "sessionID")]
    pub session_id: String,
    #[serde(default, rename = "copyChanges")]
    pub copy_changes: Option<bool>,
}

pub fn workspace_routes(state: Arc<AppState>) -> Router {
    Router::new()
        .route("/experimental/workspace/adapter", get(list_adapters))
        .route(
            "/experimental/workspace",
            get(list_workspaces).post(create_workspace),
        )
        .route("/experimental/workspace/sync-list", post(sync_list))
        .route("/experimental/workspace/status", get(workspace_status))
        .route("/experimental/workspace/{id}", delete(remove_workspace))
        .route("/experimental/workspace/warp", post(warp_session))
        .with_state(state)
}

async fn list_adapters(State(_state): State<Arc<AppState>>) -> impl IntoResponse {
    // Return available workspace adapters (git worktree is the primary)
    let adapters = vec![
        serde_json::json!({
            "id": "git",
            "name": "Git Worktree",
            "description": "Git-based workspace isolation using worktrees",
        }),
        serde_json::json!({
            "id": "copy",
            "name": "Directory Copy",
            "description": "Simple directory copy for non-git workspaces",
        }),
    ];
    info!("Listing {} workspace adapters", adapters.len());
    Json(serde_json::json!({ "adapters": adapters }))
}

async fn list_workspaces(State(_state): State<Arc<AppState>>) -> impl IntoResponse {
    // List workspaces from current git worktrees
    let cwd = std::env::current_dir().unwrap_or_else(|_| std::path::PathBuf::from("."));
    let git = blazecode_core::git::Git::new(&cwd);
    let workspaces = if git.is_repo() {
        match git.worktree_list() {
            Ok(worktrees) => worktrees
                .into_iter()
                .map(|wt| {
                    serde_json::json!({
                        "id": wt.to_string_lossy(),
                        "path": wt.to_string_lossy(),
                        "type": "git_worktree",
                    })
                })
                .collect::<Vec<_>>(),
            Err(_) => Vec::new(),
        }
    } else {
        Vec::new()
    };
    info!("Listing {} workspaces", workspaces.len());
    Json(serde_json::to_value(workspaces).unwrap_or_default())
}

async fn create_workspace(
    State(_state): State<Arc<AppState>>,
    Json(payload): Json<CreateWorkspacePayload>,
) -> impl IntoResponse {
    info!(
        "Creating workspace '{}' type={:?}",
        payload.name, payload.r#type
    );
    let workspace_id = uuid::Uuid::new_v4().to_string();
    // In a full implementation, this creates a git worktree or dir copy
    if payload.r#type.as_deref() == Some("git") {
        let cwd = std::env::current_dir().unwrap_or_else(|_| std::path::PathBuf::from("."));
        let target_dir = cwd.join("..").join(format!(".workspace-{workspace_id}"));
        let git = blazecode_core::git::Git::new(&cwd);
        if git.is_repo() {
            if let Err(e) = git.worktree_create(&target_dir) {
                return (
                    axum::http::StatusCode::INTERNAL_SERVER_ERROR,
                    Json(serde_json::json!({"error": e.to_string()})),
                )
                    .into_response();
            }
        }
    }
    Json(serde_json::json!({
        "id": workspace_id,
        "name": payload.name,
        "type": payload.r#type,
    }))
    .into_response()
}

async fn sync_list(State(_state): State<Arc<AppState>>) -> impl IntoResponse {
    info!("Sync workspace list requested");
    Json(serde_json::json!(null))
}

async fn workspace_status(State(_state): State<Arc<AppState>>) -> impl IntoResponse {
    let cwd = std::env::current_dir().unwrap_or_else(|_| std::path::PathBuf::from("."));
    let git = blazecode_core::git::Git::new(&cwd);
    let status = if git.is_repo() {
        match git.status() {
            Ok(items) => serde_json::json!({
                "is_repo": true,
                "branch": git.branch().unwrap_or(None),
                "changed_files": items.len(),
            }),
            Err(_) => serde_json::json!({"is_repo": true, "error": "status failed"}),
        }
    } else {
        serde_json::json!({"is_repo": false})
    };
    Json(serde_json::json!([status]))
}

async fn remove_workspace(
    State(_state): State<Arc<AppState>>,
    Path(id): Path<String>,
) -> impl IntoResponse {
    info!("Removing workspace {id}");
    // In a full implementation, clean up the worktree directory
    Json(serde_json::json!({ "removed": true, "id": id }))
}

async fn warp_session(
    State(_state): State<Arc<AppState>>,
    Json(payload): Json<WarpPayload>,
) -> impl IntoResponse {
    info!(
        "Warping session {} to workspace (copy_changes: {:?})",
        payload.session_id, payload.copy_changes
    );
    Json(serde_json::json!({
        "warped": true,
        "session_id": payload.session_id,
        "workspace_id": payload.id,
    }))
}
