//! Control routes — auth set/remove, log writing.
//!
//! Ported from: `packages/blazecode/src/server/routes/instance/httpapi/groups/control.ts`

use axum::extract::{Path, Query, State};
use axum::response::IntoResponse;
use axum::routing::{post, put};
use axum::{Json, Router};
use serde::Deserialize;
use std::sync::Arc;
use tracing::{info, warn};

use crate::server::AppState;

#[derive(Debug, Deserialize)]
pub struct AuthInfo {
    pub key: String,
    #[serde(default)]
    pub base_url: Option<String>,
}
#[derive(Debug, Deserialize, Default)]
pub struct LogQuery {
    #[serde(default)]
    pub directory: Option<String>,
    #[serde(default)]
    pub workspace: Option<String>,
}
#[derive(Debug, Deserialize)]
pub struct LogInput {
    pub service: String,
    pub level: String,
    pub message: String,
    #[serde(default)]
    pub extra: Option<serde_json::Value>,
}

pub fn control_routes(state: Arc<AppState>) -> Router {
    Router::new()
        .route("/auth/{providerID}", put(auth_set).delete(auth_remove))
        .route("/log", post(write_log))
        .with_state(state)
}

async fn auth_set(
    State(state): State<Arc<AppState>>,
    Path(provider_id): Path<String>,
    Json(payload): Json<AuthInfo>,
) -> impl IntoResponse {
    info!(
        "Auth set for provider '{provider_id}' (base_url: {:?})",
        payload.base_url
    );
    // Publish auth event — the provider layer picks this up to set API keys
    let event = blazecode_core::bus::GlobalEvent::new(serde_json::json!({
        "type": "auth.set",
        "provider_id": &provider_id,
        "has_base_url": payload.base_url.is_some(),
    }));
    let _ = state.bus.publish(event);

    // Set the env var for this provider
    let key_env = format!("{}_API_KEY", provider_id.to_uppercase());
    std::env::set_var(&key_env, &payload.key);
    info!("Set env var {key_env} for provider {provider_id}");

    // Persist credentials to disk at ~/.local/share/blazecode/auth.json
    let mut creds = serde_json::json!({
        "type": "api_key",
        "key": payload.key,
    });
    if let Some(ref base_url) = payload.base_url {
        creds["base_url"] = serde_json::Value::String(base_url.clone());
    }
    match blazecode_core::config::Config::save_auth(&provider_id, &creds) {
        Ok(()) => info!("Persisted auth for provider {provider_id}"),
        Err(e) => warn!("Failed to persist auth for provider {provider_id}: {e}"),
    }

    Json(serde_json::json!({
        "set": true,
        "provider_id": provider_id,
        "base_url": payload.base_url,
    }))
}

async fn auth_remove(
    State(state): State<Arc<AppState>>,
    Path(provider_id): Path<String>,
) -> impl IntoResponse {
    info!("Auth removed for provider '{provider_id}'");
    // Publish auth removal event
    let event = blazecode_core::bus::GlobalEvent::new(serde_json::json!({
        "type": "auth.removed",
        "provider_id": &provider_id,
    }));
    let _ = state.bus.publish(event);

    // Unset the env var
    let key_env = format!("{}_API_KEY", provider_id.to_uppercase());
    std::env::remove_var(&key_env);
    info!("Removed env var {key_env} for provider {provider_id}");

    // Remove credentials from disk
    match blazecode_core::config::Config::remove_auth(&provider_id) {
        Ok(()) => info!("Removed persisted auth for provider {provider_id}"),
        Err(e) => warn!("Failed to remove persisted auth for provider {provider_id}: {e}"),
    }

    Json(serde_json::json!({
        "removed": true,
        "provider_id": provider_id,
    }))
}

async fn write_log(
    State(state): State<Arc<AppState>>,
    Query(query): Query<LogQuery>,
    Json(payload): Json<LogInput>,
) -> impl IntoResponse {
    // Emit log at the appropriate tracing level
    let msg = if let Some(dir) = &query.directory {
        format!("[{}/{}] {}", payload.service, dir, payload.message)
    } else {
        format!("[{}] {}", payload.service, payload.message)
    };

    match payload.level.as_str() {
        "error" => tracing::error!(service = %payload.service, "{msg}"),
        "warn" => tracing::warn!(service = %payload.service, "{msg}"),
        "debug" => tracing::debug!(service = %payload.service, "{msg}"),
        _ => tracing::info!(service = %payload.service, "{msg}"),
    }

    // Also publish as a bus event for log streaming
    let event = blazecode_core::bus::GlobalEvent::new(serde_json::json!({
        "type": "log",
        "service": payload.service,
        "level": payload.level,
        "message": payload.message,
        "extra": payload.extra,
    }));
    let _ = state.bus.publish(event);

    Json(serde_json::json!(true))
}
