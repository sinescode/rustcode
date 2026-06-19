//! Instance routes — dispose, paths, VCS, commands, agents, skills, LSP, formatter.
//!
//! Ported from: `packages/opencode/src/server/routes/instance/httpapi/groups/instance.ts`

use axum::extract::{Query, State};
use axum::response::IntoResponse;
use axum::{Json, Router};
use axum::routing::{get, post};
use serde::Deserialize;
use std::sync::Arc;
use tracing::{error, info};

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

async fn dispose_instance(State(state): State<Arc<AppState>>) -> impl IntoResponse {
    info!("Dispose instance requested");
    let event = rustcode_core::bus::GlobalEvent::new(serde_json::json!({
        "type": "instance.disposed",
        "version": state.version,
    }));
    let _ = state.bus.publish(event);
    Json(serde_json::json!(true))
}

async fn path_info(State(_): State<Arc<AppState>>) -> impl IntoResponse {
    let cwd = std::env::current_dir()
        .map(|p| p.to_string_lossy().to_string())
        .unwrap_or_default();
    Json(serde_json::json!({
        "home": home_dir(),
        "state": format!("{}/.local/state/opencode", home_dir()),
        "config": format!("{}/.config/opencode", home_dir()),
        "worktree": format!("{}/worktree", cwd),
        "directory": cwd,
    }))
}

async fn vcs_info(State(_state): State<Arc<AppState>>) -> impl IntoResponse {
    let cwd = std::env::current_dir().unwrap_or_else(|_| std::path::PathBuf::from("."));
    let git = rustcode_core::git::Git::new(&cwd);
    if git.is_repo() {
        let branch = git.branch().unwrap_or(None).unwrap_or_else(|| "HEAD".into());
        let has_head = git.has_head().unwrap_or(false);
        let prefix = git.prefix().unwrap_or_default();
        let default_branch = git.default_branch().ok().flatten();
        Json(serde_json::json!({
            "branch": branch,
            "is_repo": true,
            "has_head": has_head,
            "prefix": prefix,
            "default_branch": default_branch,
        }))
    } else {
        Json(serde_json::json!({ "branch": null, "is_repo": false }))
    }
}

async fn vcs_status(State(_state): State<Arc<AppState>>) -> impl IntoResponse {
    let cwd = std::env::current_dir().unwrap_or_else(|_| std::path::PathBuf::from("."));
    let git = rustcode_core::git::Git::new(&cwd);
    if git.is_repo() {
        match git.status() {
            Ok(items) => {
                let statuses: Vec<serde_json::Value> = items
                    .into_iter()
                    .map(|item| {
                        serde_json::json!({
                            "file": item.file,
                            "code": item.code,
                            "status": item.status,
                        })
                    })
                    .collect();
                info!("VCS status: {} changed files", statuses.len());
                Json(serde_json::to_value(statuses).unwrap_or_default()).into_response()
            }
            Err(e) => {
                error!("VCS status failed: {e}");
                Json(serde_json::json!([])).into_response()
            }
        }
    } else {
        Json(serde_json::json!([])).into_response()
    }
}

async fn vcs_diff(
    State(_state): State<Arc<AppState>>,
    Query(query): Query<VcsDiffQuery>,
) -> impl IntoResponse {
    let dir = query.directory.unwrap_or_else(|| {
        std::env::current_dir()
            .map(|p| p.to_string_lossy().to_string())
            .unwrap_or_default()
    });
    let git = rustcode_core::git::Git::new(&dir);
    if git.is_repo() {
        let opts = rustcode_core::git::PatchOptions {
            context: query.context,
            max_output_bytes: None,
        };
        match git.patch_all("HEAD", Some(&opts)) {
            Ok(patch) => Json(serde_json::json!({
                "text": patch.text,
                "truncated": patch.truncated,
            }))
            .into_response(),
            Err(e) => {
                error!("VCS diff failed: {e}");
                Json(serde_json::json!({"text": "", "error": e.to_string()})).into_response()
            }
        }
    } else {
        Json(serde_json::json!({ "text": "", "is_repo": false })).into_response()
    }
}

async fn vcs_diff_raw(State(_state): State<Arc<AppState>>) -> impl IntoResponse {
    let cwd = std::env::current_dir().unwrap_or_else(|_| std::path::PathBuf::from("."));
    let git = rustcode_core::git::Git::new(&cwd);
    if git.is_repo() {
        match git.patch_all("HEAD", None) {
            Ok(patch) => (
                axum::http::StatusCode::OK,
                [(axum::http::header::CONTENT_TYPE, "text/x-diff; charset=utf-8")],
                patch.text,
            )
                .into_response(),
            Err(e) => (
                axum::http::StatusCode::INTERNAL_SERVER_ERROR,
                [(axum::http::header::CONTENT_TYPE, "text/plain; charset=utf-8")],
                e.to_string(),
            )
                .into_response(),
        }
    } else {
        (
            axum::http::StatusCode::OK,
            [(axum::http::header::CONTENT_TYPE, "text/x-diff; charset=utf-8")],
            String::new(),
        )
            .into_response()
    }
}

async fn vcs_apply(
    State(_state): State<Arc<AppState>>,
    Json(payload): Json<VcsApplyPayload>,
) -> impl IntoResponse {
    let cwd = std::env::current_dir().unwrap_or_else(|_| std::path::PathBuf::from("."));
    let git = rustcode_core::git::Git::new(&cwd);
    if !git.is_repo() {
        return (
            axum::http::StatusCode::BAD_REQUEST,
            Json(serde_json::json!({"error": "not a git repository"})),
        )
            .into_response();
    }
    match git.apply_patch(&payload.patch) {
        Ok(result) => {
            info!(
                "Patch applied: exit={}, truncated={}",
                result.exit_code, result.truncated
            );
            Json(serde_json::json!({
                "applied": result.exit_code == 0,
                "exit_code": result.exit_code,
                "output": result.text(),
            }))
            .into_response()
        }
        Err(e) => (
            axum::http::StatusCode::INTERNAL_SERVER_ERROR,
            Json(serde_json::json!({"error": e.to_string()})),
        )
            .into_response(),
    }
}

async fn list_commands(State(state): State<Arc<AppState>>) -> impl IntoResponse {
    // Return all registered tool IDs as available commands
    let tool_ids = state.tools.ids();
    info!("Listing {} commands (tools)", tool_ids.len());
    Json(serde_json::json!({
        "commands": tool_ids,
        "count": tool_ids.len(),
    }))
}

async fn list_agents(State(_state): State<Arc<AppState>>) -> impl IntoResponse {
    // Return known agent configurations
    // In a full implementation, this reads from config agents
    let agents = vec![
        serde_json::json!({"name": "build", "description": "Build/code generation agent", "mode": "primary"}),
        serde_json::json!({"name": "plan", "description": "Planning and architecture agent", "mode": "subagent"}),
        serde_json::json!({"name": "code-explorer", "description": "Code search and exploration subagent", "mode": "subagent"}),
    ];
    info!("Listing {} agents", agents.len());
    Json(serde_json::to_value(agents).unwrap_or_default())
}

async fn list_skills(State(_state): State<Arc<AppState>>) -> impl IntoResponse {
    // Scan .opencode/skills/ directory for skill definitions
    let cwd = std::env::current_dir().unwrap_or_else(|_| std::path::PathBuf::from("."));
    let skills_dir = cwd.join(".opencode").join("skills");
    let mut skills = Vec::new();
    if skills_dir.exists() {
        if let Ok(entries) = std::fs::read_dir(&skills_dir) {
            for entry in entries.flatten() {
                let path = entry.path();
                if path.extension().map_or(false, |ext| ext == "md") {
                    if let Some(name) = path.file_stem().and_then(|n| n.to_str()) {
                        skills.push(serde_json::json!({
                            "name": name,
                            "path": path.to_string_lossy(),
                        }));
                    }
                }
            }
        }
    }
    info!("Found {} skills", skills.len());
    Json(serde_json::json!({"skills": skills}))
}

async fn lsp_status(State(_state): State<Arc<AppState>>) -> impl IntoResponse {
    // LSP integration status — placeholder until rustcode-lsp is implemented
    Json(serde_json::json!({
        "status": "not_available",
        "message": "LSP integration is planned for the rustcode-lsp crate",
    }))
}

async fn formatter_status(State(_state): State<Arc<AppState>>) -> impl IntoResponse {
    // Formatter status — report available formatters
    let formatters = vec![
        serde_json::json!({"name": "rustfmt", "available": true, "command": "rustfmt"}),
        serde_json::json!({"name": "prettier", "available": false, "command": "prettier"}),
    ];
    Json(serde_json::json!({
        "formatters": formatters,
    }))
}
