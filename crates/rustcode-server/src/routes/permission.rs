//! Permission routes — list pending, reply.
//!
//! Ported from: `packages/opencode/src/server/routes/instance/httpapi/groups/permission.ts`

use axum::extract::{Path, State};
use axum::response::IntoResponse;
use axum::{Json, Router};
use axum::routing::{get, post};
use serde::Deserialize;
use std::sync::Arc;

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

async fn list_permissions(State(_): State<Arc<AppState>>) -> impl IntoResponse {
    Json(serde_json::json!([]))
}

async fn reply_permission(
    State(_): State<Arc<AppState>>,
    Path(request_id): Path<String>,
    Json(payload): Json<PermissionReplyPayload>,
) -> impl IntoResponse {
    Json(serde_json::json!({ "processed": true, "request_id": request_id, "reply": payload.reply }))
}
