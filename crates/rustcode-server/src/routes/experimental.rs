//! Experimental routes — console, tools, worktree, global sessions, resources.
//!
//! Ported from: `packages/opencode/src/server/routes/instance/httpapi/groups/experimental.ts`

use axum::extract::{Path, Query, State};
use axum::response::IntoResponse;
use axum::{Json, Router};
use axum::routing::{delete, get, post};
use serde::Deserialize;
use std::sync::Arc;
use tracing::info;

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

async fn console_state(State(_state): State<Arc<AppState>>) -> impl IntoResponse {
    Json(serde_json::json!({ "consoleManagedProviders": [], "switchableOrgCount": 0 }))
}

async fn console_orgs(State(_state): State<Arc<AppState>>) -> impl IntoResponse {
    Json(serde_json::json!({ "orgs": [] }))
}

async fn console_switch(
    State(_state): State<Arc<AppState>>,
    Json(payload): Json<ConsoleSwitchPayload>,
) -> impl IntoResponse {
    info!(
        "Console switch: account={} org={}",
        payload.account_id, payload.org_id
    );
    Json(serde_json::json!({
        "switched": true,
        "account_id": payload.account_id,
        "org_id": payload.org_id,
    }))
}

async fn list_tools(
    State(state): State<Arc<AppState>>,
    Query(query): Query<ToolListQuery>,
) -> impl IntoResponse {
    // Return tool definitions formatted for LLM consumption
    let defs = state.tools.llm_definitions();
    let tools: Vec<serde_json::Value> = defs
        .iter()
        .map(|td| {
            serde_json::json!({
                "name": td.name,
                "description": td.description,
                "parameters": td.parameters,
            })
        })
        .collect();
    info!(
        "List tools for {}/{}: {} tools",
        query.provider,
        query.model,
        tools.len()
    );
    Json(serde_json::json!({
        "provider": query.provider,
        "model": query.model,
        "tools": tools,
    }))
}

async fn list_tool_ids(State(state): State<Arc<AppState>>) -> impl IntoResponse {
    let ids = state.tools.ids();
    Json(serde_json::json!(ids))
}

async fn list_worktrees(State(_state): State<Arc<AppState>>) -> impl IntoResponse {
    let cwd = std::env::current_dir().unwrap_or_else(|_| std::path::PathBuf::from("."));
    let git = rustcode_core::git::Git::new(&cwd);
    let worktrees = if git.is_repo() {
        git.worktree_list()
            .unwrap_or_default()
            .into_iter()
            .map(|wt| {
                serde_json::json!({
                    "path": wt.to_string_lossy(),
                })
            })
            .collect::<Vec<_>>()
    } else {
        Vec::new()
    };
    info!("List worktrees: {} found", worktrees.len());
    Json(serde_json::to_value(worktrees).unwrap_or_default())
}

async fn create_worktree(
    State(_state): State<Arc<AppState>>,
    Json(_payload): Json<serde_json::Value>,
) -> impl IntoResponse {
    let cwd = std::env::current_dir().unwrap_or_else(|_| std::path::PathBuf::from("."));
    let git = rustcode_core::git::Git::new(&cwd);
    if git.is_repo() {
        let wt_dir = cwd.join("..").join(format!(".worktree-{}", uuid::Uuid::new_v4()));
        match git.worktree_create(&wt_dir) {
            Ok(()) => {
                info!("Worktree created at {}", wt_dir.display());
                Json(serde_json::json!({
                    "created": true,
                    "directory": wt_dir.to_string_lossy(),
                }))
            }
            Err(e) => (
                axum::http::StatusCode::INTERNAL_SERVER_ERROR,
                Json(serde_json::json!({"error": e.to_string()})),
            )
                .into_response(),
        }
    } else {
        (
            axum::http::StatusCode::BAD_REQUEST,
            Json(serde_json::json!({"error": "not a git repository"})),
        )
            .into_response()
    }
}

async fn remove_worktree(
    State(_state): State<Arc<AppState>>,
    Json(payload): Json<serde_json::Value>,
) -> impl IntoResponse {
    let directory = payload
        .get("directory")
        .and_then(|v| v.as_str())
        .map(std::path::PathBuf::from);
    match directory {
        Some(dir) => {
            let cwd = std::env::current_dir().unwrap_or_else(|_| std::path::PathBuf::from("."));
            let git = rustcode_core::git::Git::new(&cwd);
            let force = payload.get("force").and_then(|v| v.as_bool()).unwrap_or(false);
            match git.worktree_remove(&dir, force) {
                Ok(()) => {
                    info!("Worktree removed: {}", dir.display());
                    Json(serde_json::json!({ "removed": true }))
                }
                Err(e) => (
                    axum::http::StatusCode::INTERNAL_SERVER_ERROR,
                    Json(serde_json::json!({"error": e.to_string()})),
                )
                    .into_response(),
            }
        }
        None => (
            axum::http::StatusCode::BAD_REQUEST,
            Json(serde_json::json!({"error": "directory field required"})),
        )
            .into_response(),
    }
}

async fn reset_worktree(
    State(_state): State<Arc<AppState>>,
    Json(_payload): Json<serde_json::Value>,
) -> impl IntoResponse {
    let cwd = std::env::current_dir().unwrap_or_else(|_| std::path::PathBuf::from("."));
    let git = rustcode_core::git::Git::new(&cwd);
    if git.is_repo() {
        match git.reset_changes() {
            Ok(()) => {
                info!("Worktree changes reset");
                Json(serde_json::json!({ "reset": true }))
            }
            Err(e) => (
                axum::http::StatusCode::INTERNAL_SERVER_ERROR,
                Json(serde_json::json!({"error": e.to_string()})),
            )
                .into_response(),
        }
    } else {
        (
            axum::http::StatusCode::BAD_REQUEST,
            Json(serde_json::json!({"error": "not a git repository"})),
        )
            .into_response()
    }
}

async fn global_session_list(
    State(state): State<Arc<AppState>>,
    Query(query): Query<SessionListQuery>,
) -> impl IntoResponse {
    let input = rustcode_core::session::ListSessionsInput {
        directory: query.directory,
        path: None,
        workspace_id: query.workspace,
        roots: query.roots,
        search: query.search,
        limit: query.limit,
    };
    match state.sessions.list(Some(input)).await {
        Ok(sessions) => {
            let session_list: Vec<serde_json::Value> = sessions
                .into_iter()
                .map(|s| {
                    serde_json::json!({
                        "id": s.id,
                        "title": s.title,
                        "directory": s.directory,
                        "agent": s.agent,
                        "updated": s.time.updated,
                        "archived": s.time.archived,
                    })
                })
                .collect();
            Json(serde_json::to_value(session_list).unwrap_or_default()).into_response()
        }
        Err(e) => (
            axum::http::StatusCode::INTERNAL_SERVER_ERROR,
            Json(serde_json::json!({"error": e.to_string()})),
        )
            .into_response(),
    }
}

async fn background_subagents(
    State(_state): State<Arc<AppState>>,
    Path(session_id): Path<String>,
) -> impl IntoResponse {
    info!("Backgrounding subagents for session {session_id}");
    Json(serde_json::json!({ "backgrounded": true, "session_id": session_id }))
}

async fn list_resources(State(_state): State<Arc<AppState>>) -> impl IntoResponse {
    // Return available resources (MCP, tool providers, etc.)
    Json(serde_json::json!({
        "resources": [],
    }))
}
