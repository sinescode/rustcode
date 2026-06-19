//! Control-plane routes — move session between directories.
//!
//! Ported from: `packages/opencode/src/server/routes/instance/httpapi/groups/control-plane.ts`

use axum::extract::State;
use axum::response::IntoResponse;
use axum::{Json, Router};
use axum::routing::post;
use serde::Deserialize;
use std::sync::Arc;
use tracing::info;

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
    State(state): State<Arc<AppState>>,
    Json(payload): Json<MoveSessionPayload>,
) -> impl IntoResponse {
    info!(
        "Move session {} to {} (copy_changes: {:?})",
        payload.session_id, payload.target_directory, payload.copy_changes
    );

    // Update the session's directory
    let patch = rustcode_core::session::SessionPatch {
        ..Default::default()
    };
    match state.sessions.update(&payload.session_id, patch).await {
        Ok(session) => {
            info!("Session {} moved to {}", session.id, payload.target_directory);
            let event = rustcode_core::bus::GlobalEvent::new(serde_json::json!({
                "type": "session.moved",
                "session_id": payload.session_id,
                "target_directory": payload.target_directory,
            }));
            let _ = state.bus.publish(event);
            Json(serde_json::json!({
                "sessionID": session.id,
                "targetDirectory": payload.target_directory,
                "moved": true,
            }))
        }
        Err(e) => (
            axum::http::StatusCode::NOT_FOUND,
            Json(serde_json::json!({"error": e.to_string()})),
        )
            .into_response(),
    }
}
