//! Credential routes — manage per-provider credentials at runtime.
//!
//! Ported from: `packages/opencode/src/server/routes/instance/httpapi/groups/credential.ts`

use axum::extract::{Path, State};
use axum::response::IntoResponse;
use axum::{Json, Router};
use axum::routing::{delete, patch};
use serde::Deserialize;
use std::sync::Arc;
use tracing::info;

use crate::server::AppState;

/// Payload for updating a credential.
#[derive(Debug, Deserialize)]
pub struct CredentialUpdate {
    pub key: String,
    #[serde(default)]
    pub base_url: Option<String>,
}

/// Create the credential routes router.
///
/// # Source
/// Ported from `packages/opencode/src/server/routes/instance/httpapi/groups/credential.ts`
pub fn credential_routes(state: Arc<AppState>) -> Router {
    Router::new()
        .route(
            "/credential/{credentialID}",
            patch(update_credential).delete(remove_credential),
        )
        .with_state(state)
}

/// Update an existing credential (PATCH).
///
/// # Source
/// Ported from `packages/opencode/src/server/routes/instance/httpapi/groups/credential.ts`
async fn update_credential(
    State(_state): State<Arc<AppState>>,
    Path(credential_id): Path<String>,
    Json(payload): Json<CredentialUpdate>,
) -> impl IntoResponse {
    info!("Credential update for '{credential_id}'");

    // Persist the credential via the config layer
    let mut cred = serde_json::json!({
        "type": "api_key",
        "key": payload.key,
    });
    if let Some(ref base_url) = payload.base_url {
        cred["base_url"] = serde_json::Value::String(base_url.clone());
    }

    match rustcode_core::config::Config::save_auth(&credential_id, &cred) {
        Ok(()) => {
            info!("Persisted credential for {credential_id}");
            Json(serde_json::json!({
                "updated": true,
                "credential_id": credential_id,
            }))
            .into_response()
        }
        Err(e) => (
            axum::http::StatusCode::INTERNAL_SERVER_ERROR,
            Json(serde_json::json!({
                "error": "Failed to persist credential",
                "detail": e.to_string(),
            })),
        )
            .into_response(),
    }
}

/// Remove a credential (DELETE).
///
/// # Source
/// Ported from `packages/opencode/src/server/routes/instance/httpapi/groups/credential.ts`
async fn remove_credential(
    State(_state): State<Arc<AppState>>,
    Path(credential_id): Path<String>,
) -> impl IntoResponse {
    info!("Credential removal for '{credential_id}'");

    match rustcode_core::config::Config::remove_auth(&credential_id) {
        Ok(()) => {
            info!("Removed persisted credential for {credential_id}");
            Json(serde_json::json!({
                "removed": true,
                "credential_id": credential_id,
            }))
            .into_response()
        }
        Err(e) => (
            axum::http::StatusCode::INTERNAL_SERVER_ERROR,
            Json(serde_json::json!({
                "error": "Failed to remove credential",
                "detail": e.to_string(),
            })),
        )
            .into_response(),
    }
}
