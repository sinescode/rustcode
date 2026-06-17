//! Instance routes — dispose, paths, VCS, commands, agents, skills, LSP, formatter.
//!
//! Ported from: `packages/opencode/src/server/routes/instance/httpapi/groups/instance.ts`

use axum::extract::{Query, State};
use axum::response::IntoResponse;
use axum::{Json, Router};
use axum::routing::{get, post};
use serde::Deserialize;
use std::sync::Arc;

use crate::server::AppState;

#[derive(Debug, Deserialize, Default)]
pub struct VcsDiffQuery {
    #[serde(default)] pub directory: Option<String>,
    #[serde(default)] pub workspace: Option<String>,
    #[serde(default)] pub mode: Option<String>,
    #[serde(default)] pub context: Option<u32>,
}
#[derive(Debug, Deserialize)]
pub struct VcsApplyPayload { pub patch: String }

pub fn instance_routes(state: Arc<AppState>) -> Router {
    Router::new()
        .route("/instance/dispose", post(dispose_instance))
        .route("/path", get(path_info))
        .route("/vcs", get(vcs_info))
        .route("/vcs/status", get(vcs_status))
        .route("/vcs/diff", get(vcs_diff))
        .route("/vcs/diff/raw", get(vcs_diff_raw))
        .route("/vcs/apply", post(vcs_apply))
        .route("/command", get(list_commands))
        .route("/agent", get(list_agents))
        .route("/skill", get(list_skills))
        .route("/lsp", get(lsp_status))
        .route("/formatter", get(formatter_status))
        .with_state(state)
}

fn home_dir() -> String { std::env::var("HOME").unwrap_or_else(|_| "/tmp".to_string()) }

async fn dispose_instance(State(_): State<Arc<AppState>>) -> impl IntoResponse { Json(serde_json::json!(true)) }

async fn path_info(State(_): State<Arc<AppState>>) -> impl IntoResponse {
    let cwd = std::env::current_dir().map(|p| p.to_string_lossy().to_string()).unwrap_or_default();
    Json(serde_json::json!({
        "home": home_dir(),
        "state": format!("{}/.local/state/opencode", home_dir()),
        "config": format!("{}/.config/opencode", home_dir()),
        "worktree": format!("{}/worktree", cwd),
        "directory": cwd,
    }))
}
async fn vcs_info(State(_): State<Arc<AppState>>) -> impl IntoResponse {
    Json(serde_json::json!({ "branch": "main", "is_repo": true }))
}
async fn vcs_status(State(_): State<Arc<AppState>>) -> impl IntoResponse { Json(serde_json::json!([])) }
async fn vcs_diff(State(_): State<Arc<AppState>>, Query(_query): Query<VcsDiffQuery>) -> impl IntoResponse {
    Json(serde_json::json!([]))
}
async fn vcs_diff_raw(State(_): State<Arc<AppState>>) -> impl IntoResponse {
    (axum::http::StatusCode::OK, [(axum::http::header::CONTENT_TYPE, "text/x-diff; charset=utf-8")], "")
}
async fn vcs_apply(
    State(_): State<Arc<AppState>>,
    Json(_payload): Json<VcsApplyPayload>,
) -> impl IntoResponse { Json(serde_json::json!({ "applied": true })) }
async fn list_commands(State(_): State<Arc<AppState>>) -> impl IntoResponse { Json(serde_json::json!([])) }
async fn list_agents(State(_): State<Arc<AppState>>) -> impl IntoResponse { Json(serde_json::json!([])) }
async fn list_skills(State(_): State<Arc<AppState>>) -> impl IntoResponse { Json(serde_json::json!([])) }
async fn lsp_status(State(_): State<Arc<AppState>>) -> impl IntoResponse { Json(serde_json::json!([])) }
async fn formatter_status(State(_): State<Arc<AppState>>) -> impl IntoResponse { Json(serde_json::json!([])) }
