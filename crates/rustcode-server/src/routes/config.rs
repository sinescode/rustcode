//! Config routes — get/update config, list providers.
//!
//! Ported from: `packages/opencode/src/server/routes/instance/httpapi/groups/config.ts`
//!
//! Route paths:
//! - `GET   /config`           — get config
//! - `PATCH /config`           — update config
//! - `GET   /config/providers` — list config providers

use axum::extract::{Path, State};
use axum::response::IntoResponse;
use axum::{Json, Router};
use axum::routing::{get, patch};
use serde::Deserialize;
use std::sync::Arc;

use crate::server::AppState;

/// Create the config routes router.
pub fn config_routes(state: Arc<AppState>) -> Router {
    Router::new()
        .route("/config", get(get_config).patch(update_config))
        .route("/config/providers", get(list_providers))
        .with_state(state)
}

async fn get_config() -> impl IntoResponse {
    Json(serde_json::json!({
        "schema": "opencode.json",
        "version": env!("CARGO_PKG_VERSION"),
    }))
}

async fn update_config(
    Json(payload): Json<serde_json::Value>,
) -> impl IntoResponse {
    Json(serde_json::json!({
        "updated": true,
        "payload": payload,
    }))
}

async fn list_providers() -> impl IntoResponse {
    Json(serde_json::json!([]))
}
