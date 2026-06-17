//! Project copy routes — generate name.
//!
//! Ported from: `packages/opencode/src/server/routes/instance/httpapi/groups/project-copy.ts`

use axum::extract::{Path, State};
use axum::response::IntoResponse;
use axum::{Json, Router};
use axum::routing::post;
use serde::Deserialize;
use std::sync::Arc;

use crate::server::AppState;

#[derive(Debug, Deserialize)]
pub struct GenerateNamePayload {
    #[serde(default)] pub context: Option<String>,
}

pub fn project_copy_routes(state: Arc<AppState>) -> Router {
    Router::new()
        .route("/experimental/project/{projectID}/copy/generate-name", post(generate_name))
        .with_state(state)
}

async fn generate_name(
    State(_): State<Arc<AppState>>,
    Path(_project_id): Path<String>,
    Json(payload): Json<GenerateNamePayload>,
) -> impl IntoResponse {
    let name = payload.context.as_deref().map(|ctx| {
        ctx.split_whitespace().take(3).collect::<Vec<_>>().join("-").to_lowercase()
    }).unwrap_or_else(|| "project-copy".to_string());
    Json(serde_json::json!({ "name": name }))
}
