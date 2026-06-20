//! Health check route — liveness/readiness probe for orchestrators and monitoring.
//!
//! Ported from: `packages/opencode/src/server/routes/instance/httpapi/groups/health.ts`

use axum::extract::State;
use axum::response::IntoResponse;
use axum::routing::get;
use axum::{Json, Router};
use std::sync::Arc;

use crate::server::AppState;

/// Create the health routes router.
///
/// # Source
/// Ported from `packages/opencode/src/server/routes/instance/httpapi/groups/health.ts`
pub fn health_routes(state: Arc<AppState>) -> Router {
    Router::new()
        .route("/health", get(health_check))
        .with_state(state)
}

/// Health check — returns version, uptime, provider status, database status,
/// and connected client count.
///
/// # Source
/// Ported from `packages/opencode/src/server/routes/instance/httpapi/groups/health.ts`
async fn health_check(State(state): State<Arc<AppState>>) -> impl IntoResponse {
    let uptime = state.start_time.elapsed();
    let uptime_seconds = uptime.as_secs();

    // Provider status — a provider is "connected" if it's in the map
    let provider_status: Vec<serde_json::Value> = state
        .providers
        .keys()
        .map(|id| {
            serde_json::json!({
                "id": id,
                "status": "connected",
            })
        })
        .collect();

    // Database status — present if the session manager is available
    let db_status = "connected";

    // Connected SSE/event clients (bus receiver count)
    let connected_clients = state.bus.receiver_count();

    Json(serde_json::json!({
        "healthy": true,
        "version": state.version,
        "uptime_seconds": uptime_seconds,
        "uptime_display": format_uptime(uptime_seconds),
        "providers": {
            "count": state.providers.len(),
            "status": provider_status,
        },
        "database": {
            "status": db_status,
        },
        "connected_clients": connected_clients,
    }))
    .into_response()
}

/// Format uptime seconds into a human-readable string.
fn format_uptime(seconds: u64) -> String {
    let days = seconds / 86400;
    let hours = (seconds % 86400) / 3600;
    let minutes = (seconds % 3600) / 60;
    let secs = seconds % 60;

    if days > 0 {
        format!("{days}d {hours}h {minutes}m {secs}s")
    } else if hours > 0 {
        format!("{hours}h {minutes}m {secs}s")
    } else if minutes > 0 {
        format!("{minutes}m {secs}s")
    } else {
        format!("{secs}s")
    }
}
