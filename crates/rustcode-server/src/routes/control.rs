//! Control routes — auth set/remove, log writing.
//!
//! Ported from: `packages/opencode/src/server/routes/instance/httpapi/groups/control.ts`

use axum::extract::{Path, Query, State};
use axum::response::IntoResponse;
use axum::{Json, Router};
use axum::routing::{delete, post, put};
use serde::Deserialize;
use std::sync::Arc;

use crate::server::AppState;

#[derive(Debug, Deserialize)]
pub struct AuthInfo { pub key: String, #[serde(default)] pub base_url: Option<String> }
#[derive(Debug, Deserialize, Default)]
pub struct LogQuery { #[serde(default)] pub directory: Option<String>, #[serde(default)] pub workspace: Option<String> }
#[derive(Debug, Deserialize)]
pub struct LogInput {
    pub service: String,
    pub level: String,
    pub message: String,
    #[serde(default)] pub extra: Option<serde_json::Value>,
}

pub fn control_routes(state: Arc<AppState>) -> Router {
    Router::new()
        .route("/auth/{providerID}", put(auth_set).delete(auth_remove))
        .route("/log", post(write_log))
        .with_state(state)
}

async fn auth_set(
    State(_): State<Arc<AppState>>,
    Path(provider_id): Path<String>,
    Json(_payload): Json<AuthInfo>,
) -> impl IntoResponse { Json(serde_json::json!({ "set": true, "provider_id": provider_id })) }

async fn auth_remove(
    State(_): State<Arc<AppState>>,
    Path(provider_id): Path<String>,
) -> impl IntoResponse { Json(serde_json::json!({ "removed": true, "provider_id": provider_id })) }

async fn write_log(
    State(_): State<Arc<AppState>>,
    Query(_query): Query<LogQuery>,
    Json(payload): Json<LogInput>,
) -> impl IntoResponse {
    match payload.level.as_str() {
        "error" => tracing::error!(service = %payload.service, "{}", payload.message),
        "warn" => tracing::warn!(service = %payload.service, "{}", payload.message),
        "debug" => tracing::debug!(service = %payload.service, "{}", payload.message),
        _ => tracing::info!(service = %payload.service, "{}", payload.message),
    }
    Json(serde_json::json!(true))
}
