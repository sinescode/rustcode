//! File routes — find text, find files, find symbols, list, read, status.
//!
//! Ported from: `packages/opencode/src/server/routes/instance/httpapi/groups/file.ts`

use axum::extract::{Query, State};
use axum::response::IntoResponse;
use axum::{Json, Router};
use axum::routing::get;
use serde::Deserialize;
use std::sync::Arc;

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

async fn find_text(
    State(_): State<Arc<AppState>>,
    Query(query): Query<FindTextQuery>,
) -> impl IntoResponse {
    Json(serde_json::json!({ "pattern": query.pattern, "matches": [] }))
}

async fn find_file(
    State(_): State<Arc<AppState>>,
    Query(query): Query<FindFileQuery>,
) -> impl IntoResponse {
    Json(serde_json::json!({ "query": query.query, "files": [] }))
}

async fn find_symbol(
    State(_): State<Arc<AppState>>,
    Query(query): Query<FindSymbolQuery>,
) -> impl IntoResponse {
    Json(serde_json::json!({ "query": query.query, "symbols": [] }))
}

async fn list_files(
    State(_): State<Arc<AppState>>,
    Query(query): Query<FileQuery>,
) -> impl IntoResponse {
    Json(serde_json::json!({ "path": query.path, "entries": [] }))
}

async fn read_file(
    State(_): State<Arc<AppState>>,
    Query(query): Query<FileQuery>,
) -> impl IntoResponse {
    Json(serde_json::json!({ "path": query.path, "type": "text", "content": "" }))
}

async fn file_status(State(_): State<Arc<AppState>>) -> impl IntoResponse {
    Json(serde_json::json!([]))
}
