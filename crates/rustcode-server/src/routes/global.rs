//! Global routes — health, config, dispose, upgrade, global events.
//!
//! Ported from: `packages/opencode/src/server/routes/instance/httpapi/groups/global.ts`
//!
//! Route paths:
//! - `GET  /global/health`   — health check
//! - `GET  /global/event`    — global event stream (SSE)
//! - `GET  /global/config`   — get global config
//! - `PATCH /global/config`  — update global config
//! - `POST /global/dispose`  — dispose instance
//! - `POST /global/upgrade`  — upgrade opencode

use axum::extract::State;
use axum::response::IntoResponse;
use axum::{Json, Router};
use axum::routing::{get, patch, post};
use serde::{Deserialize, Serialize};
use std::sync::Arc;

use crate::server::AppState;

// ── Types ────────────────────────────────────────────────────────────────────

/// Health check response.
///
/// # Source
/// `GlobalHealth` in `global.ts` line 11.
#[derive(Debug, Serialize)]
pub struct HealthResponse {
    pub healthy: bool,
    pub version: String,
}

/// Global config update payload.
///
/// # Source
/// `ConfigV1.Info` used as payload in `global.ts` line 106.
#[derive(Debug, Deserialize)]
pub struct ConfigUpdatePayload {
    #[serde(default)]
    pub shell: Option<String>,
    #[serde(default)]
    pub log_level: Option<String>,
    #[serde(default)]
    pub default_agent: Option<String>,
    #[serde(default)]
    pub username: Option<String>,
}

/// Upgrade input.
///
/// # Source
/// `GlobalUpgradeInput` in `global.ts` line 52.
#[derive(Debug, Deserialize)]
pub struct UpgradeInput {
    #[serde(default)]
    pub target: Option<String>,
}

/// Upgrade result.
///
/// # Source
/// `GlobalUpgradeResult` in `global.ts` line 56.
#[derive(Debug, Serialize)]
#[serde(untagged)]
pub enum UpgradeResult {
    Success { success: bool, version: String },
    Failure { success: bool, error: String },
}

// ── Routes ────────────────────────────────────────────────────────────────────

/// Create the global routes router.
pub fn global_routes(state: Arc<AppState>) -> Router {
    Router::new()
        .route("/global/health", get(health))
        .route("/global/event", get(global_event))
        .route("/global/config", get(global_config_get).patch(global_config_update))
        .route("/global/dispose", post(global_dispose))
        .route("/global/upgrade", post(global_upgrade))
        .with_state(state)
}

// ── Handlers ──────────────────────────────────────────────────────────────────

/// `GET /global/health` — health check.
///
/// # Source
/// `global.ts` line 78 — `HttpApiEndpoint.get("health", GlobalPaths.health, ...)`.
async fn health(State(state): State<Arc<AppState>>) -> impl IntoResponse {
    Json(HealthResponse {
        healthy: true,
        version: state.version.clone(),
    })
}

/// `GET /global/event` — global event stream (SSE placeholder).
///
/// # Source
/// `global.ts` line 87 — `HttpApiEndpoint.get("event", GlobalPaths.event, ...)`.
async fn global_event(State(_): State<Arc<AppState>>) -> impl IntoResponse {
    // The TS source returns `GlobalEventSchema` for the schema definition.
    // In runtime, this is an SSE endpoint. For now, return empty OK.
    // Full SSE implementation connects to the bus and streams events.
    Json(serde_json::json!({"message": "SSE endpoint — use GET /event for instance events"}))
}

/// `GET /global/config` — get global config.
///
/// # Source
/// `global.ts` line 96 — `HttpApiEndpoint.get("configGet", GlobalPaths.config, ...)`.
async fn global_config_get(State(_): State<Arc<AppState>>) -> impl IntoResponse {
    Json(serde_json::json!({
        "schema": "opencode.json",
        "version": env!("CARGO_PKG_VERSION"),
    }))
}

/// `PATCH /global/config` — update global config.
///
/// # Source
/// `global.ts` line 105 — `HttpApiEndpoint.patch("configUpdate", GlobalPaths.config, ...)`.
async fn global_config_update(
    State(_): State<Arc<AppState>>,
    Json(_payload): Json<ConfigUpdatePayload>,
) -> impl IntoResponse {
    Json(serde_json::json!({
        "schema": "opencode.json",
        "version": env!("CARGO_PKG_VERSION"),
    }))
}

/// `POST /global/dispose` — dispose all instances.
///
/// # Source
/// `global.ts` line 116 — `HttpApiEndpoint.post("dispose", GlobalPaths.dispose, ...)`.
async fn global_dispose(State(_): State<Arc<AppState>>) -> impl IntoResponse {
    Json(serde_json::json!(true))
}

/// `POST /global/upgrade` — upgrade opencode.
///
/// # Source
/// `global.ts` line 125 — `HttpApiEndpoint.post("upgrade", GlobalPaths.upgrade, ...)`.
async fn global_upgrade(
    State(_): State<Arc<AppState>>,
    Json(payload): Json<UpgradeInput>,
) -> impl IntoResponse {
    let target = payload.target.unwrap_or_else(|| "latest".to_string());
    Json(serde_json::json!({
        "success": false,
        "error": format!("upgrade to {target} not yet implemented in rustcode-server"),
    }))
}
