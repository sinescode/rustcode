//! File routes — find text, find files, find symbols, list, read, status.
//!
//! Ported from: `packages/blazecode/src/server/routes/instance/httpapi/groups/file.ts`

use axum::extract::{Query, State};
use axum::response::IntoResponse;
use axum::routing::get;
use axum::{Json, Router};
use serde::Deserialize;
use std::path::PathBuf;
use std::sync::Arc;
use tracing::{error, info};

use crate::server::AppState;

#[derive(Debug, Deserialize)]
pub struct FindTextQuery {
    pub pattern: String,
    #[serde(default)]
    pub directory: Option<String>,
    #[serde(default)]
    pub workspace: Option<String>,
}

#[derive(Debug, Deserialize)]
pub struct FindFileQuery {
    pub query: String,
    #[serde(default)]
    pub directory: Option<String>,
    #[serde(default)]
    pub workspace: Option<String>,
    #[serde(default)]
    pub dirs: Option<String>,
    #[serde(default)]
    pub r#type: Option<String>,
    #[serde(default)]
    pub limit: Option<u32>,
}

#[derive(Debug, Deserialize)]
pub struct FindSymbolQuery {
    pub query: String,
    #[serde(default)]
    pub directory: Option<String>,
    #[serde(default)]
    pub workspace: Option<String>,
}

#[derive(Debug, Deserialize)]
pub struct FileQuery {
    pub path: String,
    #[serde(default)]
    pub directory: Option<String>,
    #[serde(default)]
    pub workspace: Option<String>,
}

pub fn file_routes(state: Arc<AppState>) -> Router {
    Router::new()
        .route("/find", get(find_text))
        .route("/find/file", get(find_file))
        .route("/find/symbol", get(find_symbol))
        .route("/file", get(list_files))
        .route("/file/content", get(read_file))
        .route("/file/status", get(file_status))
        .with_state(state)
}

fn resolve_directory(directory: Option<&str>) -> PathBuf {
    directory
        .map(PathBuf::from)
        .unwrap_or_else(|| std::env::current_dir().unwrap_or_else(|_| PathBuf::from(".")))
}

async fn find_text(
    State(_state): State<Arc<AppState>>,
    Query(query): Query<FindTextQuery>,
) -> impl IntoResponse {
    let dir = resolve_directory(query.directory.as_deref());
    let mut matches = Vec::new();
    let limit = 50usize;

    info!("File: find_text '{}' in {}", query.pattern, dir.display());

    if let Err(e) = search_files_recursive(&dir, &query.pattern, &mut matches, limit) {
        error!("find_text error: {e}");
    }

    Json(serde_json::json!({
        "pattern": query.pattern,
        "directory": dir.to_string_lossy(),
        "matches": matches,
        "count": matches.len(),
    }))
}

fn search_files_recursive(
    dir: &PathBuf,
    pattern: &str,
    results: &mut Vec<serde_json::Value>,
    limit: usize,
) -> std::io::Result<()> {
    if results.len() >= limit || !dir.exists() {
        return Ok(());
    }
    let entries = std::fs::read_dir(dir)?;
    for entry in entries.flatten() {
        if results.len() >= limit {
            break;
        }
        let path = entry.path();
        let name = path.file_name().unwrap_or_default().to_string_lossy();
        // Skip hidden directories and common non-search dirs
        if path.is_dir() {
            if name.starts_with('.') || name == "target" || name == "node_modules" {
                continue;
            }
            search_files_recursive(&path, pattern, results, limit)?;
        } else if path.is_file() {
            if let Ok(content) = std::fs::read_to_string(&path) {
                for (line_num, line) in content.lines().enumerate() {
                    if line.to_lowercase().contains(&pattern.to_lowercase()) {
                        results.push(serde_json::json!({
                            "file": path.to_string_lossy(),
                            "line": line_num + 1,
                            "content": line.trim().to_string(),
                        }));
                        if results.len() >= limit {
                            break;
                        }
                    }
                }
            }
        }
    }
    Ok(())
}

async fn find_file(
    State(_state): State<Arc<AppState>>,
    Query(query): Query<FindFileQuery>,
) -> impl IntoResponse {
    let dir = resolve_directory(query.directory.as_deref());
    let limit = query.limit.unwrap_or(20) as usize;
    let mut files = Vec::new();

    info!("File: find_file '{}' in {}", query.query, dir.display());

    if let Err(e) = find_files_recursive(&dir, &query.query.to_lowercase(), &mut files, limit) {
        error!("find_file error: {e}");
    }

    Json(serde_json::json!({
        "query": query.query,
        "directory": dir.to_string_lossy(),
        "files": files,
        "count": files.len(),
    }))
}

fn find_files_recursive(
    dir: &PathBuf,
    query: &str,
    results: &mut Vec<serde_json::Value>,
    limit: usize,
) -> std::io::Result<()> {
    if results.len() >= limit || !dir.exists() {
        return Ok(());
    }
    let entries = std::fs::read_dir(dir)?;
    for entry in entries.flatten() {
        if results.len() >= limit {
            break;
        }
        let path = entry.path();
        let name = path.file_name().unwrap_or_default().to_string_lossy();
        if name.starts_with('.') || name == "target" || name == "node_modules" {
            continue;
        }
        if path.is_dir() {
            find_files_recursive(&path, query, results, limit)?;
        } else if name.to_lowercase().contains(query) {
            if let Ok(meta) = path.metadata() {
                results.push(serde_json::json!({
                    "path": path.to_string_lossy(),
                    "name": name,
                    "size": meta.len(),
                    "is_dir": false,
                }));
            }
        }
    }
    Ok(())
}

async fn find_symbol(
    State(_state): State<Arc<AppState>>,
    Query(query): Query<FindSymbolQuery>,
) -> impl IntoResponse {
    let dir = resolve_directory(query.directory.as_deref());
    let mut symbols = Vec::new();
    let limit = 30usize;

    info!("File: find_symbol '{}' in {}", query.query, dir.display());

    // Simple symbol search: grep for fn, struct, enum, impl, trait, mod declarations
    let patterns = [
        ("fn", "function"),
        ("pub fn", "public_function"),
        ("struct", "struct"),
        ("pub struct", "public_struct"),
        ("enum", "enum"),
        ("pub enum", "public_enum"),
        ("trait", "trait"),
        ("pub trait", "public_trait"),
        ("impl", "impl"),
        ("mod", "module"),
    ];

    if let Err(e) = find_symbols_recursive(&dir, &query.query, &patterns, &mut symbols, limit) {
        error!("find_symbol error: {e}");
    }

    Json(serde_json::json!({
        "query": query.query,
        "directory": dir.to_string_lossy(),
        "symbols": symbols,
        "count": symbols.len(),
    }))
}

fn find_symbols_recursive(
    dir: &PathBuf,
    query: &str,
    patterns: &[(&str, &str)],
    results: &mut Vec<serde_json::Value>,
    limit: usize,
) -> std::io::Result<()> {
    if results.len() >= limit || !dir.exists() {
        return Ok(());
    }
    let entries = std::fs::read_dir(dir)?;
    for entry in entries.flatten() {
        if results.len() >= limit {
            break;
        }
        let path = entry.path();
        let name = path.file_name().unwrap_or_default().to_string_lossy();
        if name.starts_with('.') || name == "target" || name == "node_modules" {
            continue;
        }
        if path.is_dir() {
            find_symbols_recursive(&path, query, patterns, results, limit)?;
        } else if path
            .extension()
            .is_some_and(|ext| ext == "rs" || ext == "py" || ext == "ts" || ext == "js")
        {
            if let Ok(content) = std::fs::read_to_string(&path) {
                let query_lower = query.to_lowercase();
                for line in content.lines() {
                    let trimmed = line.trim();
                    let lower = trimmed.to_lowercase();
                    if !lower.contains(&query_lower) {
                        continue;
                    }
                    for (keyword, kind) in patterns {
                        if trimmed.starts_with(keyword) {
                            results.push(serde_json::json!({
                                "file": path.to_string_lossy(),
                                "kind": kind,
                                "signature": trimmed.to_string(),
                            }));
                            break;
                        }
                    }
                    if results.len() >= limit {
                        break;
                    }
                }
            }
        }
    }
    Ok(())
}

async fn list_files(
    State(_state): State<Arc<AppState>>,
    Query(query): Query<FileQuery>,
) -> impl IntoResponse {
    let dir = resolve_directory(query.directory.as_deref());
    let target = dir.join(&query.path);
    let mut entries_json = Vec::new();

    if target.exists() {
        if target.is_dir() {
            if let Ok(entries) = std::fs::read_dir(&target) {
                for entry in entries.flatten() {
                    let path = entry.path();
                    let name = path.file_name().unwrap_or_default().to_string_lossy();
                    let is_dir = path.is_dir();
                    let size = path.metadata().ok().map(|m| m.len()).unwrap_or(0);
                    entries_json.push(serde_json::json!({
                        "name": name,
                        "path": path.to_string_lossy(),
                        "is_dir": is_dir,
                        "size": size,
                    }));
                }
                entries_json.sort_by_key(|e| {
                    (
                        if e["is_dir"].as_bool().unwrap_or(false) {
                            0
                        } else {
                            1
                        },
                        e["name"].as_str().unwrap_or("").to_string(),
                    )
                });
            }
        } else {
            entries_json.push(serde_json::json!({
                "name": target.file_name().unwrap_or_default().to_string_lossy(),
                "path": target.to_string_lossy(),
                "is_dir": false,
                "size": target.metadata().ok().map(|m| m.len()).unwrap_or(0),
            }));
        }
    }

    Json(serde_json::json!({
        "path": query.path,
        "directory": dir.to_string_lossy(),
        "entries": entries_json,
        "count": entries_json.len(),
    }))
}

async fn read_file(
    State(_state): State<Arc<AppState>>,
    Query(query): Query<FileQuery>,
) -> impl IntoResponse {
    let dir = resolve_directory(query.directory.as_deref());
    let file_path = dir.join(&query.path);

    match std::fs::read_to_string(&file_path) {
        Ok(content) => {
            let ext = file_path.extension().and_then(|e| e.to_str()).unwrap_or("");
            let mime = match ext {
                "rs" => "text/x-rust",
                "toml" => "text/x-toml",
                "json" => "application/json",
                "md" => "text/markdown",
                "yaml" | "yml" => "text/yaml",
                "py" => "text/x-python",
                "ts" | "js" => "application/javascript",
                "html" => "text/html",
                "css" => "text/css",
                "sh" | "bash" => "text/x-shellscript",
                "lock" => "text/plain",
                "png" => "image/png",
                "jpg" | "jpeg" => "image/jpeg",
                "gif" => "image/gif",
                "svg" => "image/svg+xml",
                "pdf" => "application/pdf",
                _ => "application/octet-stream",
            };
            let is_text = mime.starts_with("text/")
                || mime.starts_with("application/json")
                || mime.starts_with("application/javascript");
            let lines = content.lines().count();
            let size = content.len();
            info!(
                "Read file {}: {} lines, {size} bytes",
                file_path.display(),
                lines
            );

            Json(serde_json::json!({
                "path": query.path,
                "directory": dir.to_string_lossy(),
                "type": if is_text { "text" } else { "binary" },
                "mime": mime,
                "content": content,
                "lines": lines,
                "size": size,
            }))
            .into_response()
        }
        Err(e) => (
            axum::http::StatusCode::NOT_FOUND,
            Json(serde_json::json!({
                "error": format!("cannot read {}: {e}", file_path.display()),
                "path": query.path,
            })),
        )
            .into_response(),
    }
}

async fn file_status(State(_state): State<Arc<AppState>>) -> impl IntoResponse {
    // Return file status from git (if available) or filesystem
    let cwd = std::env::current_dir().unwrap_or_else(|_| PathBuf::from("."));
    let git = blazecode_core::git::Git::new(&cwd);
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
                Json(serde_json::to_value(statuses).unwrap_or_default()).into_response()
            }
            Err(_) => Json(serde_json::json!([])).into_response(),
        }
    } else {
        Json(serde_json::json!([])).into_response()
    }
}
