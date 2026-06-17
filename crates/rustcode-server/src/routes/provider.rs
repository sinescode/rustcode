//! Provider routes — list, auth methods, OAuth authorize/callback.
//!
//! Ported from: `packages/opencode/src/server/routes/instance/httpapi/groups/provider.ts`

use axum::extract::{Path, State};
use axum::response::IntoResponse;
use axum::{Json, Router};
use axum::routing::{get, post};
use serde::Deserialize;
use std::sync::Arc;

use crate::server::AppState;

#[derive(Debug, Deserialize)]
pub struct AuthorizeInput {
    #[serde(default)]
    pub redirect_uri: Option<String>,
}

#[derive(Debug, Deserialize)]
pub struct CallbackInput {
    pub code: String,
    #[serde(default)]
    pub state: Option<String>,
    #[serde(default)]
    pub redirect_uri: Option<String>,
}

pub fn provider_routes(state: Arc<AppState>) -> Router {
    Router::new()
        .route("/provider", get(list_providers))
        .route("/provider/auth", get(provider_auth_methods))
        .route("/provider/{providerID}/oauth/authorize", post(oauth_authorize))
        .route("/provider/{providerID}/oauth/callback", post(oauth_callback))
        .with_state(state)
}

async fn list_providers(State(_): State<Arc<AppState>>) -> impl IntoResponse {
    Json(serde_json::json!([]))
}

async fn provider_auth_methods(State(_): State<Arc<AppState>>) -> impl IntoResponse {
    Json(serde_json::json!([]))
}

async fn oauth_authorize(
    State(_): State<Arc<AppState>>,
    Path(provider_id): Path<String>,
    Json(_payload): Json<AuthorizeInput>,
) -> impl IntoResponse {
    Json(serde_json::json!({ "provider_id": provider_id, "authorization_url": null }))
}

async fn oauth_callback(
    State(_): State<Arc<AppState>>,
    Path(provider_id): Path<String>,
    Json(_payload): Json<CallbackInput>,
) -> impl IntoResponse {
    Json(serde_json::json!({ "provider_id": provider_id, "success": true }))
}
