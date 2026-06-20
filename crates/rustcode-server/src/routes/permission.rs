//! Permission routes — list pending, reply.
//!
//! Ported from: `packages/opencode/src/server/routes/instance/httpapi/groups/permission.ts`

use axum::extract::{Path, State};
use axum::response::IntoResponse;
use axum::routing::{get, post};
use axum::{Json, Router};
use serde::Deserialize;
use std::sync::Arc;
use tracing::info;

use crate::server::AppState;

#[derive(Debug, Deserialize)]
pub struct PermissionReplyPayload {
    pub reply: String,
    #[serde(default)]
    pub message: Option<String>,
}

pub fn permission_routes(state: Arc<AppState>) -> Router {
    Router::new()
        .route("/permission", get(list_permissions))
        .route("/permission/{requestID}/reply", post(reply_permission))
        .with_state(state)
}

async fn list_permissions(State(state): State<Arc<AppState>>) -> impl IntoResponse {
    let pending = state.permissions.list();
    info!("Listing {} pending permission requests", pending.len());
    Json(serde_json::to_value(pending).unwrap_or_default())
}

async fn reply_permission(
    State(state): State<Arc<AppState>>,
    Path(request_id): Path<String>,
    Json(payload): Json<PermissionReplyPayload>,
) -> impl IntoResponse {
    let reply = match payload.reply.to_lowercase().as_str() {
        "once" => rustcode_core::permission::PermissionReply::Once,
        "always" => rustcode_core::permission::PermissionReply::Always,
        "reject" | "deny" => rustcode_core::permission::PermissionReply::Reject,
        _ => {
            return (
                axum::http::StatusCode::BAD_REQUEST,
                Json(serde_json::json!({
                    "error": format!("invalid reply: '{}'. Use once/always/reject", payload.reply),
                })),
            )
                .into_response();
        }
    };
    let input = rustcode_core::permission::ReplyInput {
        request_id: request_id.clone(),
        reply,
        message: payload.message,
    };
    match state.permissions.reply(input).await {
        Ok(()) => {
            info!("Permission request {request_id} replied: {:?}", reply);
            Json(serde_json::json!({
                "processed": true,
                "request_id": request_id,
                "reply": payload.reply,
            }))
            .into_response()
        }
        Err(e) => (
            axum::http::StatusCode::NOT_FOUND,
            Json(serde_json::json!({"error": e.to_string()})),
        )
            .into_response(),
    }
}
