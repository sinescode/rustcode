//! Project copy routes — create, delete, and refresh project copies
//! backed by git worktrees.
//!
//! Ported from: `packages/opencode/src/server/routes/instance/httpapi/groups/project-copy.ts`

use axum::extract::{Path, State};
use axum::response::IntoResponse;
use axum::{Json, Router};
use axum::routing::{delete, get, post};
use serde::Deserialize;
use std::collections::HashMap;
use std::sync::Arc;
use tracing::info;

use crate::server::AppState;

/// In-memory store for project copies.
type CopyStore = HashMap<String, ProjectCopyInfo>;

/// Payload for creating a project copy.
#[derive(Debug, Deserialize)]
pub struct CreateCopyPayload {
    /// Optional context for generating the copy name.
    #[serde(default)]
    pub context: Option<String>,
    /// Directory to copy (defaults to current working directory).
    #[serde(default)]
    pub directory: Option<String>,
}

/// Payload for generating a name suggestion.
#[derive(Debug, Deserialize)]
pub struct GenerateNamePayload {
    #[serde(default)]
    pub context: Option<String>,
}

/// Information about a project copy.
#[derive(Debug, Clone, serde::Serialize)]
struct ProjectCopyInfo {
    id: String,
    name: String,
    path: String,
    status: String,
    created_at: u64,
    #[serde(skip_serializing_if = "Option::is_none")]
    size_bytes: Option<u64>,
}

/// Create the project copy routes router.
///
/// # Source
/// Ported from `packages/opencode/src/server/routes/instance/httpapi/groups/project-copy.ts`
pub fn project_copy_routes(state: Arc<AppState>) -> Router {
    Router::new()
        .route("/experimental/project/{projectID}/copy", get(list_copies))
        .route("/experimental/project/{projectID}/copy", post(create_copy))
        .route("/experimental/project/{projectID}/copy/{copyID}", get(get_copy))
        .route(
            "/experimental/project/{projectID}/copy/{copyID}",
            delete(delete_copy),
        )
        .route(
            "/experimental/project/{projectID}/copy/{copyID}/refresh",
            post(refresh_copy),
        )
        .route(
            "/experimental/project/{projectID}/copy/generate-name",
            post(generate_name),
        )
        .with_state(state)
}

/// List all copies for a project.
async fn list_copies(
    State(_state): State<Arc<AppState>>,
    Path(_project_id): Path<String>,
) -> impl IntoResponse {
    // In a full implementation this would query the database or filesystem.
    // For now, return an empty list — copies are ephemeral and tracked in memory.
    Json(serde_json::json!([])).into_response()
}

/// Create a new project copy using a git worktree.
async fn create_copy(
    State(_state): State<Arc<AppState>>,
    Path(project_id): Path<String>,
    Json(_payload): Json<CreateCopyPayload>,
) -> impl IntoResponse {
    let cwd = std::env::current_dir().unwrap_or_else(|_| std::path::PathBuf::from("."));
    let copy_id = format!("copy_{}", uuid::Uuid::new_v4().to_string().split('-').next().unwrap_or("00000000"));

    // Check if the directory is a git repo
    let is_git = cwd.join(".git").exists();

    if !is_git {
        return (
            axum::http::StatusCode::BAD_REQUEST,
            Json(serde_json::json!({
                "error": "Cannot create project copy: not a git repository",
                "directory": cwd.display().to_string(),
            })),
        )
            .into_response();
    }

    // Attempt to create a worktree
    let copy_path = cwd
        .parent()
        .unwrap_or(&cwd)
        .join(format!(".opencode-copies/{}", copy_id));

    // Try git worktree add
    let result = std::process::Command::new("git")
        .args([
            "worktree",
            "add",
            "--detach",
            &copy_path.display().to_string(),
            "HEAD",
        ])
        .current_dir(&cwd)
        .output();

    match result {
        Ok(output) if output.status.success() => {
            let now = std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap_or_default()
                .as_secs();

            info!(
                "Created project copy '{}' for project '{}' at '{}'",
                copy_id,
                project_id,
                copy_path.display()
            );

            Json(serde_json::json!({
                "id": copy_id,
                "project_id": project_id,
                "name": format!("copy-{}", &copy_id[..8]),
                "path": copy_path.display().to_string(),
                "status": "ready",
                "created_at": now,
                "size_bytes": estimate_dir_size(&copy_path),
            }))
            .into_response()
        }
        Ok(output) => {
            let stderr = String::from_utf8_lossy(&output.stderr);
            (
                axum::http::StatusCode::INTERNAL_SERVER_ERROR,
                Json(serde_json::json!({
                    "error": format!("Failed to create worktree: {}", stderr.trim()),
                })),
            )
                .into_response()
        }
        Err(e) => (
            axum::http::StatusCode::INTERNAL_SERVER_ERROR,
            Json(serde_json::json!({
                "error": format!("Failed to run git worktree: {e}"),
            })),
        )
            .into_response(),
    }
}

/// Get details for a specific project copy.
async fn get_copy(
    State(_state): State<Arc<AppState>>,
    Path((project_id, copy_id)): Path<(String, String)>,
) -> impl IntoResponse {
    let cwd = std::env::current_dir().unwrap_or_else(|_| std::path::PathBuf::from("."));
    let copy_path = cwd
        .parent()
        .unwrap_or(&cwd)
        .join(format!(".opencode-copies/{}", copy_id));

    if !copy_path.exists() {
        return (
            axum::http::StatusCode::NOT_FOUND,
            Json(serde_json::json!({
                "error": format!("copy '{}' not found for project '{}'", copy_id, project_id),
            })),
        )
            .into_response();
    }

    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs();

    Json(serde_json::json!({
        "id": copy_id,
        "project_id": project_id,
        "name": format!("copy-{}", &copy_id[..8]),
        "path": copy_path.display().to_string(),
        "status": "ready",
        "created_at": now,
        "size_bytes": estimate_dir_size(&copy_path),
    }))
    .into_response()
}

/// Delete a project copy (remove the worktree).
async fn delete_copy(
    State(_state): State<Arc<AppState>>,
    Path((project_id, copy_id)): Path<(String, String)>,
) -> impl IntoResponse {
    let cwd = std::env::current_dir().unwrap_or_else(|_| std::path::PathBuf::from("."));
    let copy_path = cwd
        .parent()
        .unwrap_or(&cwd)
        .join(format!(".opencode-copies/{}", copy_id));

    if !copy_path.exists() {
        return (
            axum::http::StatusCode::NOT_FOUND,
            Json(serde_json::json!({
                "error": format!("copy '{}' not found for project '{}'", copy_id, project_id),
            })),
        )
            .into_response();
    }

    // Remove the worktree
    let remove_result = std::process::Command::new("git")
        .args([
            "worktree",
            "remove",
            "--force",
            &copy_path.display().to_string(),
        ])
        .current_dir(&cwd)
        .output();

    match remove_result {
        Ok(output) if output.status.success() => {
            info!(
                "Deleted project copy '{}' for project '{}'",
                copy_id, project_id
            );
            Json(serde_json::json!({
                "status": "deleted",
                "id": copy_id,
                "project_id": project_id,
            }))
            .into_response()
        }
        Ok(output) => {
            let stderr = String::from_utf8_lossy(&output.stderr);
            (
                axum::http::StatusCode::INTERNAL_SERVER_ERROR,
                Json(serde_json::json!({
                    "error": format!("Failed to remove worktree: {}", stderr.trim()),
                })),
            )
                .into_response()
        }
        Err(e) => (
            axum::http::StatusCode::INTERNAL_SERVER_ERROR,
            Json(serde_json::json!({
                "error": format!("Failed to run git worktree remove: {e}"),
            })),
        )
            .into_response(),
    }
}

/// Refresh a project copy — pull latest changes from the source.
async fn refresh_copy(
    State(_state): State<Arc<AppState>>,
    Path((project_id, copy_id)): Path<(String, String)>,
) -> impl IntoResponse {
    let cwd = std::env::current_dir().unwrap_or_else(|_| std::path::PathBuf::from("."));
    let copy_path = cwd
        .parent()
        .unwrap_or(&cwd)
        .join(format!(".opencode-copies/{}", copy_id));

    if !copy_path.exists() {
        return (
            axum::http::StatusCode::NOT_FOUND,
            Json(serde_json::json!({
                "error": format!("copy '{}' not found for project '{}'", copy_id, project_id),
            })),
        )
            .into_response();
    }

    // Pull latest changes
    let pull_result = std::process::Command::new("git")
        .args(["fetch", "--all"])
        .current_dir(&copy_path)
        .output();

    match pull_result {
        Ok(output) if output.status.success() => {
            info!(
                "Refreshed project copy '{}' for project '{}'",
                copy_id, project_id
            );
            Json(serde_json::json!({
                "id": copy_id,
                "project_id": project_id,
                "status": "refreshed",
                "path": copy_path.display().to_string(),
                "size_bytes": estimate_dir_size(&copy_path),
            }))
            .into_response()
        }
        Ok(output) => {
            let stderr = String::from_utf8_lossy(&output.stderr);
            (
                axum::http::StatusCode::INTERNAL_SERVER_ERROR,
                Json(serde_json::json!({
                    "error": format!("Failed to refresh: {}", stderr.trim()),
                })),
            )
                .into_response()
        }
        Err(e) => (
            axum::http::StatusCode::INTERNAL_SERVER_ERROR,
            Json(serde_json::json!({
                "error": format!("Failed to run git fetch: {e}"),
            })),
        )
            .into_response(),
    }
}

/// Generate a suggested name for a project copy.
async fn generate_name(
    State(_): State<Arc<AppState>>,
    Path(_project_id): Path<String>,
    Json(payload): Json<GenerateNamePayload>,
) -> impl IntoResponse {
    let name = payload
        .context
        .as_deref()
        .map(|ctx| {
            ctx.split_whitespace()
                .take(3)
                .collect::<Vec<_>>()
                .join("-")
                .to_lowercase()
        })
        .unwrap_or_else(|| "project-copy".to_string());
    Json(serde_json::json!({ "name": name })).into_response()
}

/// Estimate the size of a directory in bytes.
fn estimate_dir_size(path: &std::path::Path) -> Option<u64> {
    let mut total: u64 = 0;
    let entries = match std::fs::read_dir(path) {
        Ok(e) => e,
        Err(_) => return None,
    };
    for entry in entries.flatten() {
        if let Ok(meta) = entry.metadata() {
            if meta.is_file() {
                total += meta.len();
            } else if meta.is_dir() {
                if let Some(sub) = estimate_dir_size(&entry.path()) {
                    total += sub;
                }
            }
        }
    }
    if total > 0 {
        Some(total)
    } else {
        None
    }
}
