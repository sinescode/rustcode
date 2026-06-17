//! Control-plane routes — move session between directories.
//!
//! Ported from: `packages/opencode/src/server/routes/instance/httpapi/groups/control-plane.ts`

use axum::extract::State;
use axum::response::IntoResponse;
use axum::{Json, Router};
use axum::routing::post;
use serde::Deserialize;
use std::sync::Arc;

use crate::server::AppState;

#[derive(Debug, Deserialize)]
pub struct MoveSessionPayload {
    #[serde(rename = "sessionID")] pub session_id: String,
    #[serde(rename = "targetDirectory")] pub target_directory: String,
    #[serde(default, rename = "copyChanges")] pub copy_changes: Option<bool>,
}

pub fn control_plane_routes(state: Arc<AppState>) -> Router {
    Router::new()
        .route("/experimental/control-plane/move-session", post(move_session))
        .with_state(state)
}

async fn move_session(
    State(_): State<Arc<AppState>>,
    Json(payload): Json<MoveSessionPayload>,
) -> impl IntoResponse {
    let _ = tracing::info!("move session {} to {}", payload.session_id, payload.target_directory);
    Json(serde_json::json!(null))
}
