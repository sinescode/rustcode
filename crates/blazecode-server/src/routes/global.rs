//! Global routes — health, config, dispose, upgrade, global events.
//!
//! Ported from: `packages/blazecode/src/server/routes/instance/httpapi/groups/global.ts`
//!
//! Route paths:
//! - `GET  /global/health`   — health check
//! - `GET  /global/event`    — global event stream (SSE)
//! - `GET  /global/config`   — get global config
//! - `PATCH /global/config`  — update global config
//! - `POST /global/dispose`  — dispose instance
//! - `POST /global/upgrade`  — upgrade blazecode

use axum::extract::{Path, State};
use axum::response::IntoResponse;
use axum::routing::{get, post, put};
use axum::{Json, Router};
use serde::{Deserialize, Serialize};
use std::sync::Arc;
use tracing::info;

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
        .route(
            "/global/config",
            get(global_config_get).patch(global_config_update),
        )
        .route("/global/dispose", post(global_dispose))
        .route("/global/upgrade", post(global_upgrade))
        .route("/auth/{provider_id}", put(put_auth))
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
        "schema": "blazecode.json",
        "version": env!("CARGO_PKG_VERSION"),
    }))
}

/// `PATCH /global/config` — update global config.
///
/// # Source
/// `global.ts` line 105 — `HttpApiEndpoint.patch("configUpdate", GlobalPaths.config, ...)`.
async fn global_config_update(
    State(state): State<Arc<AppState>>,
    Json(payload): Json<serde_json::Value>,
) -> impl IntoResponse {
    // Parse the full JSON body as ConfigV1.Info
    let incoming: blazecode_core::config::Info = match serde_json::from_value(payload) {
        Ok(info) => info,
        Err(e) => {
            return (
                axum::http::StatusCode::BAD_REQUEST,
                Json(serde_json::json!({
                    "error": "Invalid config payload",
                    "detail": e.to_string(),
                })),
            )
                .into_response();
        }
    };

    // Apply env-var side effects from key scalar fields
    if let Some(ref shell) = incoming.shell {
        info!(%shell, "Global config: setting shell");
        std::env::set_var("SHELL", shell);
    }
    if let Some(ref log_level) = incoming.log_level {
        let level_str = format!("{:?}", log_level).to_uppercase();
        info!(%level_str, "Global config: setting log_level");
        std::env::set_var("RUST_LOG", &level_str);
    }
    if let Some(ref agent) = incoming.default_agent {
        info!(%agent, "Global config: setting default_agent");
    }
    if let Some(ref username) = incoming.username {
        info!(%username, "Global config: setting username");
    }

    // Determine global config path:
    //   ~/.config/blazecode/config.json   (preferred)
    //   ~/.config/blazecode/blazecode.json (fallback)
    let config_dir = match blazecode_core::config::Config::global_config_dir() {
        Ok(dir) => dir,
        Err(e) => {
            return (
                axum::http::StatusCode::INTERNAL_SERVER_ERROR,
                Json(serde_json::json!({
                    "error": "Cannot determine config directory",
                    "detail": e.to_string(),
                })),
            )
                .into_response();
        }
    };
    let path = {
        let json = config_dir.join("config.json");
        let blazecode = config_dir.join("blazecode.json");
        if json.exists() {
            json
        } else {
            blazecode
        }
    };

    // Load existing global config from the file and deep-merge the incoming patch
    let mut merged = match blazecode_core::config::Config::load_from_file(&path) {
        Ok(existing) => existing,
        Err(e) => {
            tracing::warn!("Could not load existing global config (will start fresh): {e}");
            blazecode_core::config::Info::default()
        }
    };
    blazecode_core::config::merge_info(&mut merged, &incoming);

    let key_count = serde_json::to_value(&merged)
        .ok()
        .and_then(|v| v.as_object().map(|o| o.len()))
        .unwrap_or(0);
    info!(
        "Writing global config to `{}` ({} top-level keys after merge)",
        path.display(),
        key_count,
    );

    match blazecode_core::config::Config::save_to_file(&path, &merged) {
        Ok(()) => {
            let event = blazecode_core::bus::GlobalEvent::new(serde_json::json!({
                "type": "global.config.updated",
                "version": state.version,
            }));
            let _ = state.bus.publish(event);

            Json(serde_json::json!({
                "schema": "blazecode.json",
                "version": state.version,
                "updated": true,
                "path": path.to_string_lossy(),
            }))
            .into_response()
        }
        Err(e) => (
            axum::http::StatusCode::INTERNAL_SERVER_ERROR,
            Json(serde_json::json!({
                "error": "Failed to write global config",
                "detail": e.to_string(),
            })),
        )
            .into_response(),
    }
}

/// `POST /global/dispose` — dispose all instances.
///
/// # Source
/// `global.ts` line 116 — `HttpApiEndpoint.post("dispose", GlobalPaths.dispose, ...)`.
async fn global_dispose(State(state): State<Arc<AppState>>) -> impl IntoResponse {
    tracing::info!("Global dispose: shutting down all instances");
    let event = blazecode_core::bus::GlobalEvent::new(serde_json::json!({
        "type": "global.disposed",
        "version": state.version,
    }));
    let _ = state.bus.publish(event);
    Json(serde_json::json!(true))
}

/// `POST /global/upgrade` — upgrade blazecode.
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
        "error": format!("upgrade to {target} not yet implemented in blazecode-server"),
    }))
}

/// `PUT /auth/{provider_id}` — write auth credentials for a provider.
///
/// Accepts arbitrary JSON as the credential value and persists it to
/// `{data_dir}/blazecode/auth.json`.
///
/// # Source
/// Ported from `packages/blazecode/src/auth/index.ts` — `put()`.
async fn put_auth(
    Path(provider_id): Path<String>,
    Json(credentials): Json<serde_json::Value>,
) -> impl IntoResponse {
    info!("Writing auth credentials for provider `{provider_id}`");

    match blazecode_core::config::Config::save_auth(&provider_id, &credentials) {
        Ok(()) => Json(serde_json::json!({
            "saved": true,
            "provider_id": provider_id,
        }))
        .into_response(),
        Err(e) => (
            axum::http::StatusCode::INTERNAL_SERVER_ERROR,
            Json(serde_json::json!({
                "error": "Failed to save auth credentials",
                "detail": e.to_string(),
            })),
        )
            .into_response(),
    }
}
