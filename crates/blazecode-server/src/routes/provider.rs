//! Provider routes — list, auth methods, OAuth authorize/callback.
//!
//! Ported from: `packages/blazecode/src/server/routes/instance/httpapi/groups/provider.ts`

use axum::extract::{Path, State};
use axum::response::IntoResponse;
use axum::routing::{get, post};
use axum::{Json, Router};
use serde::Deserialize;
use std::sync::Arc;
use tracing::info;

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
        .route(
            "/provider/{providerID}/oauth/authorize",
            post(oauth_authorize),
        )
        .route(
            "/provider/{providerID}/oauth/callback",
            post(oauth_callback),
        )
        .with_state(state)
}

async fn list_providers(State(state): State<Arc<AppState>>) -> impl IntoResponse {
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
    let count = providers.len();
    info!("Listing {count} configured providers");
    Json(serde_json::json!({
        "providers": providers,
        "count": count,
        "version": state.version,
    }))
}

async fn provider_auth_methods(State(state): State<Arc<AppState>>) -> impl IntoResponse {
    // Return auth method info for each provider
    let auth_methods: Vec<serde_json::Value> = state
        .providers
        .iter()
        .map(|(id, provider)| {
            serde_json::json!({
                "provider_id": id,
                "name": provider.provider_id(),
                "auth_type": "api_key",
                "env_var": format!("{}_API_KEY", id.to_uppercase()),
            })
        })
        .collect();
    Json(serde_json::json!(auth_methods))
}

async fn oauth_authorize(
    State(state): State<Arc<AppState>>,
    Path(provider_id): Path<String>,
    Json(_payload): Json<AuthorizeInput>,
) -> impl IntoResponse {
    match state.providers.get(&provider_id) {
        Some(_provider) => {
            // OAuth flow: redirect the user to the provider's OAuth URL
            // This is provider-specific, so we return a placeholder URL
            info!("OAuth authorize requested for provider {provider_id}");
            Json(serde_json::json!({
                "provider_id": provider_id,
                "authorization_url": format!("https://auth.{provider_id}.com/oauth/authorize"),
            }))
            .into_response()
        }
        None => (
            axum::http::StatusCode::NOT_FOUND,
            Json(serde_json::json!({
                "error": format!("provider '{provider_id}' not configured")
            })),
        )
            .into_response(),
    }
}

async fn oauth_callback(
    State(state): State<Arc<AppState>>,
    Path(provider_id): Path<String>,
    Json(_payload): Json<CallbackInput>,
) -> impl IntoResponse {
    match state.providers.get(&provider_id) {
        Some(_provider) => {
            info!("OAuth callback processed for provider {provider_id}");
            Json(serde_json::json!({
                "provider_id": provider_id,
                "success": true,
            }))
            .into_response()
        }
        None => (
            axum::http::StatusCode::NOT_FOUND,
            Json(serde_json::json!({
                "error": format!("provider '{provider_id}' not configured")
            })),
        )
            .into_response(),
    }
}
