//! Sync routes — start, replay, steal, history.
//!
//! Ported from: `packages/opencode/src/server/routes/instance/httpapi/groups/sync.ts`

use axum::extract::State;
use axum::response::IntoResponse;
use axum::{Json, Router};
use axum::routing::post;
use serde::Deserialize;
use std::collections::HashMap;
use std::sync::Arc;

use crate::server::AppState;

#[derive(Debug, Deserialize)]
pub struct ReplayPayload {
    pub directory: String,
    pub events: Vec<ReplayEventItem>,
}
#[derive(Debug, Deserialize)]
pub struct ReplayEventItem {
    pub id: String,
    #[serde(rename = "aggregateID")] pub aggregate_id: String,
    pub seq: u64,
    #[serde(rename = "type")] pub event_type: String,
    pub data: serde_json::Value,
}
#[derive(Debug, Deserialize)]
pub struct StealPayload {
    #[serde(rename = "sessionID")] pub session_id: String,
}

pub fn sync_routes(state: Arc<AppState>) -> Router {
    Router::new()
        .route("/sync/start", post(sync_start))
        .route("/sync/replay", post(sync_replay))
        .route("/sync/steal", post(sync_steal))
        .route("/sync/history", post(sync_history))
        .with_state(state)
}

async fn sync_start(State(_): State<Arc<AppState>>) -> impl IntoResponse {
    Json(serde_json::json!(true))
}
async fn sync_replay(
    State(_): State<Arc<AppState>>,
    Json(payload): Json<ReplayPayload>,
) -> impl IntoResponse {
    Json(serde_json::json!({ "sessionID": "", "replayed_count": payload.events.len() }))
}
async fn sync_steal(
    State(_): State<Arc<AppState>>,
    Json(payload): Json<StealPayload>,
) -> impl IntoResponse {
    Json(serde_json::json!({ "sessionID": payload.session_id }))
}
async fn sync_history(
    State(_): State<Arc<AppState>>,
    Json(_payload): Json<HashMap<String, u64>>,
) -> impl IntoResponse {
    Json(serde_json::json!([]))
}
