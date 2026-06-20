//! Sync routes — start, replay, steal, history.
//!
//! Ported from: `packages/opencode/src/server/routes/instance/httpapi/groups/sync.ts`

use axum::extract::State;
use axum::response::IntoResponse;
use axum::routing::post;
use axum::{Json, Router};
use serde::Deserialize;
use std::collections::HashMap;
use std::sync::Arc;
use tracing::info;

use crate::server::AppState;

#[derive(Debug, Deserialize)]
pub struct ReplayPayload {
    pub directory: String,
    pub events: Vec<ReplayEventItem>,
}
#[derive(Debug, Deserialize)]
pub struct ReplayEventItem {
    pub id: String,
    #[serde(rename = "aggregateID")]
    pub aggregate_id: String,
    pub seq: u64,
    #[serde(rename = "type")]
    pub event_type: String,
    pub data: serde_json::Value,
}
#[derive(Debug, Deserialize)]
pub struct StealPayload {
    #[serde(rename = "sessionID")]
    pub session_id: String,
}

pub fn sync_routes(state: Arc<AppState>) -> Router {
    Router::new()
        .route("/sync/start", post(sync_start))
        .route("/sync/replay", post(sync_replay))
        .route("/sync/steal", post(sync_steal))
        .route("/sync/history", post(sync_history))
        .with_state(state)
}

async fn sync_start(State(state): State<Arc<AppState>>) -> impl IntoResponse {
    info!("Sync: start requested");
    let event = rustcode_core::bus::GlobalEvent::new(serde_json::json!({
        "type": "sync.started",
        "version": state.version,
    }));
    let _ = state.bus.publish(event);
    Json(serde_json::json!(true))
}

async fn sync_replay(
    State(state): State<Arc<AppState>>,
    Json(payload): Json<ReplayPayload>,
) -> impl IntoResponse {
    info!(
        "Sync: replaying {} events for directory {}",
        payload.events.len(),
        payload.directory
    );
    // Replay events: process each event against the bus
    let mut replayed = 0u64;
    let mut errors = Vec::new();

    for event_item in &payload.events {
        let event = rustcode_core::bus::GlobalEvent::new(serde_json::json!({
            "type": format!("sync.replay.{}", event_item.event_type),
            "aggregate_id": event_item.aggregate_id,
            "seq": event_item.seq,
            "data": event_item.data,
        }));
        if let Err(e) = state.bus.publish(event) {
            errors.push(format!("event {}: {e}", event_item.id));
        } else {
            replayed += 1;
        }
    }

    // Find or create a session for the replay
    let session_id = if let Ok(sessions) = state
        .sessions
        .list(Some(rustcode_core::session::ListSessionsInput {
            directory: Some(payload.directory.clone()),
            limit: Some(1),
            ..Default::default()
        }))
        .await
    {
        sessions.first().map(|s| s.id.clone()).unwrap_or_default()
    } else {
        String::new()
    };

    Json(serde_json::json!({
        "sessionID": session_id,
        "replayed_count": replayed,
        "total_count": payload.events.len(),
        "errors": errors,
    }))
}

async fn sync_steal(
    State(state): State<Arc<AppState>>,
    Json(payload): Json<StealPayload>,
) -> impl IntoResponse {
    info!("Sync: steal session {}", payload.session_id);
    // "Stealing" a session means claiming it for the current instance
    match state.sessions.get(&payload.session_id).await {
        Ok(session) => {
            let event = rustcode_core::bus::GlobalEvent::new(serde_json::json!({
                "type": "sync.stolen",
                "session_id": payload.session_id,
                "title": session.title,
            }));
            let _ = state.bus.publish(event);
            Json(serde_json::json!({
                "sessionID": payload.session_id,
                "title": session.title,
            }))
            .into_response()
        }
        Err(e) => (
            axum::http::StatusCode::NOT_FOUND,
            Json(serde_json::json!({"error": e.to_string(), "sessionID": payload.session_id})),
        )
            .into_response(),
    }
}

async fn sync_history(
    State(state): State<Arc<AppState>>,
    Json(payload): Json<HashMap<String, u64>>,
) -> impl IntoResponse {
    // payload maps aggregate IDs to their highest known sequence
    info!("Sync: history request for {} aggregates", payload.len());

    // Return sessions as sync events
    let sessions = state.sessions.list(None).await.unwrap_or_default();

    let events: Vec<serde_json::Value> = sessions
        .iter()
        .map(|s| {
            serde_json::json!({
                "id": s.id,
                "aggregateID": s.project_id,
                "seq": s.time.updated,
                "type": "session.updated",
                "data": {
                    "id": s.id,
                    "title": s.title,
                    "directory": s.directory,
                    "updated": s.time.updated,
                },
            })
        })
        .collect();

    info!("Sync: returning {} history events", events.len());
    Json(serde_json::to_value(events).unwrap_or_default())
}
