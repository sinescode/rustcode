//! Config routes — get/update config, list providers.
//!
//! Ported from: `packages/blazecode/src/server/routes/instance/httpapi/groups/config.ts`
//!
//! Route paths:
//! - `GET   /config`           — get config
//! - `PATCH /config`           — update config
//! - `GET   /config/providers` — list config providers

use axum::extract::State;
use axum::response::IntoResponse;
use axum::routing::get;
use axum::{Json, Router};
use std::sync::Arc;
use tracing::info;

use crate::server::AppState;

/// Create the config routes router.
pub fn config_routes(state: Arc<AppState>) -> Router {
    Router::new()
        .route("/config", get(get_config).patch(update_config))
        .route("/config/providers", get(list_providers))
        .with_state(state)
}

async fn get_config(State(state): State<Arc<AppState>>) -> impl IntoResponse {
    // Return the current config — version + schema
    let cwd = std::env::current_dir()
        .map(|p| p.to_string_lossy().to_string())
        .unwrap_or_default();
    let home = std::env::var("HOME").unwrap_or_else(|_| "/tmp".to_string());

    Json(serde_json::json!({
        "schema": "blazecode.json",
        "version": state.version,
        "directory": cwd,
        "home": home,
        "shell": std::env::var("SHELL").unwrap_or_else(|_| "/bin/sh".to_string()),
    }))
}

async fn update_config(
    State(_state): State<Arc<AppState>>,
    Json(payload): Json<serde_json::Value>,
) -> impl IntoResponse {
    // Parse the JSON body into ConfigV1.Info
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

    // Determine the config file path: prefer blazecode.json in current dir,
    // fall back to blazecode.jsonc
    let cwd = std::env::current_dir().unwrap_or_else(|_| std::path::PathBuf::from("."));
    let json_path = cwd.join("blazecode.json");
    let jsonc_path = cwd.join("blazecode.jsonc");
    let path = if json_path.exists() {
        json_path
    } else {
        jsonc_path
    };

    // Load existing config and deep-merge the incoming patch
    let mut merged = match blazecode_core::config::Config::load_from_file(&path) {
        Ok(existing) => existing,
        Err(e) => {
            tracing::warn!("Could not load existing project config (will start fresh): {e}");
            blazecode_core::config::Info::default()
        }
    };
    blazecode_core::config::merge_info(&mut merged, &incoming);

    let key_count = serde_json::to_value(&merged)
        .ok()
        .and_then(|v| v.as_object().map(|o| o.len()))
        .unwrap_or(0);
    info!(
        "Writing project config to `{}` ({} top-level keys after merge)",
        path.display(),
        key_count,
    );

    match blazecode_core::config::Config::save_to_file(&path, &merged) {
        Ok(()) => Json(serde_json::json!({
            "updated": true,
            "path": path.to_string_lossy(),
        }))
        .into_response(),
        Err(e) => (
            axum::http::StatusCode::INTERNAL_SERVER_ERROR,
            Json(serde_json::json!({
                "error": "Failed to write config file",
                "detail": e.to_string(),
            })),
        )
            .into_response(),
    }
}

async fn list_providers(State(state): State<Arc<AppState>>) -> impl IntoResponse {
    // List providers registered in AppState
    let providers: Vec<serde_json::Value> = state
        .providers
        .iter()
        .map(|(id, provider)| {
            serde_json::json!({
                "id": id,
                "name": provider.provider_id(),
                "npm": provider.npm(),
            })
        })
        .collect();
    Json(serde_json::json!({
        "providers": providers,
        "default": state.version,
    }))
}
