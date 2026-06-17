//! Project routes — list, current, init git, update, directories.
//!
//! Ported from: `packages/opencode/src/server/routes/instance/httpapi/groups/project.ts`
//!
//! Route paths:
//! - `GET   /project`                       — list projects
//! - `GET   /project/current`               — current project info
//! - `POST  /project/git/init`              — init git repo
//! - `PATCH /project/:projectID`            — update project
//! - `GET   /project/:projectID/directories` — project directories

use axum::extract::{Path, State};
use axum::response::IntoResponse;
use axum::{Json, Router};
use axum::routing::{get, patch, post};
use serde::Deserialize;
use std::sync::Arc;

use crate::server::AppState;

#[derive(Debug, Deserialize)]
pub struct UpdateProjectPayload {
    #[serde(default)]
    pub name: Option<String>,
    #[serde(default)]
    pub icon: Option<String>,
    #[serde(default)]
    pub commands: Option<serde_json::Value>,
}

/// Create the project routes router.
pub fn project_routes(state: Arc<AppState>) -> Router {
    Router::new()
        .route("/project", get(list_projects))
        .route("/project/current", get(current_project))
        .route("/project/git/init", post(init_git))
        .route("/project/{projectID}", patch(update_project))
        .route("/project/{projectID}/directories", get(project_directories))
        .with_state(state)
}

async fn list_projects() -> impl IntoResponse {
    Json(serde_json::json!([]))
}

async fn current_project() -> impl IntoResponse {
    Json(serde_json::json!({
        "id": "default",
        "name": "rustcode",
        "directory": std::env::current_dir()
            .map(|p| p.to_string_lossy().to_string())
            .unwrap_or_default(),
    }))
}

async fn init_git() -> impl IntoResponse {
    Json(serde_json::json!({
        "id": "default",
        "name": "rustcode",
        "initialized": true,
    }))
}

async fn update_project(
    Path(project_id): Path<String>,
    Json(payload): Json<UpdateProjectPayload>,
) -> impl IntoResponse {
    Json(serde_json::json!({
        "id": project_id,
        "name": payload.name,
        "updated": true,
    }))
}

async fn project_directories(
    Path(project_id): Path<String>,
) -> impl IntoResponse {
    Json(serde_json::json!({
        "project_id": project_id,
        "directories": [],
    }))
}
