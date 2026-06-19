//! Project routes — list, current, init git, update, directories.
//!
//! Ported from: `packages/opencode/src/server/routes/instance/httpapi/groups/project.ts`

use axum::extract::{Path, State};
use axum::response::IntoResponse;
use axum::{Json, Router};
use axum::routing::{get, patch, post};
use serde::Deserialize;
use std::path::PathBuf;
use std::sync::Arc;
use tracing::info;

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

pub fn project_routes(state: Arc<AppState>) -> Router {
    Router::new()
        .route("/project", get(list_projects))
        .route("/project/current", get(current_project))
        .route("/project/git/init", post(init_git))
        .route("/project/{projectID}", patch(update_project))
        .route("/project/{projectID}/directories", get(project_directories))
        .with_state(state)
}

async fn list_projects(State(_state): State<Arc<AppState>>) -> impl IntoResponse {
    // Return projects from known directories
    let cwd = std::env::current_dir()
        .map(|p| p.to_string_lossy().to_string())
        .unwrap_or_default();
    let cwd_path = std::env::current_dir().unwrap_or_else(|_| PathBuf::from("."));
    let project_name = cwd_path
        .file_name()
        .map(|n| n.to_string_lossy().to_string())
        .unwrap_or_else(|| "unknown".to_string());

    let projects = vec![serde_json::json!({
        "id": "default",
        "name": project_name,
        "directory": cwd,
        "icon": null,
        "commands": {},
    })];

    info!("Listing projects: {}", projects.len());
    Json(serde_json::to_value(projects).unwrap_or_default())
}

async fn current_project(State(_state): State<Arc<AppState>>) -> impl IntoResponse {
    let cwd = std::env::current_dir()
        .map(|p| p.to_string_lossy().to_string())
        .unwrap_or_default();
    let project_name = std::env::current_dir()
        .ok()
        .and_then(|p| p.file_name().map(|n| n.to_string_lossy().to_string()))
        .unwrap_or_else(|| "rustcode".to_string());

    // Check if it's a git repo
    let git = rustcode_core::git::Git::new(&cwd);
    let branch = git.branch().unwrap_or(None);
    let is_repo = git.is_repo();

    Json(serde_json::json!({
        "id": "default",
        "name": project_name,
        "directory": cwd,
        "is_repo": is_repo,
        "branch": branch,
    }))
}

async fn init_git(State(_state): State<Arc<AppState>>) -> impl IntoResponse {
    let cwd = std::env::current_dir().unwrap_or_else(|_| PathBuf::from("."));
    let git = rustcode_core::git::Git::new(&cwd);
    let already_repo = git.is_repo();

    if !already_repo {
        // Initialize a new git repository
        match std::process::Command::new("git")
            .args(["init"])
            .current_dir(&cwd)
            .output()
        {
            Ok(out) => {
                if out.status.success() {
                    info!("Git repository initialized in {}", cwd.display());
                } else {
                    let stderr = String::from_utf8_lossy(&out.stderr);
                    return (
                        axum::http::StatusCode::INTERNAL_SERVER_ERROR,
                        Json(serde_json::json!({"error": stderr})),
                    )
                        .into_response();
                }
            }
            Err(e) => {
                return (
                    axum::http::StatusCode::INTERNAL_SERVER_ERROR,
                    Json(serde_json::json!({"error": e.to_string()})),
                )
                    .into_response();
            }
        }
    }

    Json(serde_json::json!({
        "id": "default",
        "name": cwd.file_name().map(|n| n.to_string_lossy().to_string()).unwrap_or_default(),
        "directory": cwd.to_string_lossy(),
        "initialized": true,
        "was_already_repo": already_repo,
    }))
}

async fn update_project(
    State(_state): State<Arc<AppState>>,
    Path(project_id): Path<String>,
    Json(payload): Json<UpdateProjectPayload>,
) -> impl IntoResponse {
    info!(
        "Update project {project_id}: name={:?}, icon={:?}",
        payload.name, payload.icon
    );
    Json(serde_json::json!({
        "id": project_id,
        "name": payload.name,
        "icon": payload.icon,
        "commands": payload.commands,
        "updated": true,
    }))
}

async fn project_directories(
    State(_state): State<Arc<AppState>>,
    Path(project_id): Path<String>,
) -> impl IntoResponse {
    let cwd = std::env::current_dir().unwrap_or_else(|_| PathBuf::from("."));
    let mut directories = vec![serde_json::json!({
        "path": cwd.to_string_lossy(),
        "name": cwd.file_name().map(|n| n.to_string_lossy().to_string()).unwrap_or_default(),
    })];

    // Scan for workspace directories
    if let Ok(entries) = std::fs::read_dir(&cwd) {
        for entry in entries.flatten() {
            if entry.path().is_dir() {
                let name = entry.file_name().to_string_lossy().to_string();
                if name.starts_with('.') || name == "target" || name == "node_modules" {
                    continue;
                }
                // Check if it has a Cargo.toml or package.json to identify subprojects
                let has_cargo = entry.path().join("Cargo.toml").exists();
                let has_pkg = entry.path().join("package.json").exists();
                if has_cargo || has_pkg {
                    directories.push(serde_json::json!({
                        "path": entry.path().to_string_lossy(),
                        "name": name,
                        "has_cargo_toml": has_cargo,
                        "has_package_json": has_pkg,
                    }));
                }
            }
        }
    }

    info!("Project {project_id} has {} directories", directories.len());
    Json(serde_json::json!({
        "project_id": project_id,
        "directories": directories,
    }))
}
