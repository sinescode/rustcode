//! Health check endpoint — returns server status and DB connectivity.

use crate::AppState;
use axum::{extract::State, http::StatusCode, Json, Router};
use std::sync::Arc;

/// Build health check routes.
pub fn health_routes() -> Router<Arc<AppState>> {
    Router::new()
        .route("/health", axum::routing::get(health_check))
}

/// `GET /health` — returns server health status.
///
/// Checks:
/// - Server is running (always true if this responds)
/// - Database connectivity (pings the SQLite pool)
///
/// # Source
/// Ported from `packages/server/src/routes/health.ts`
pub async fn health_check(
    State(state): State<Arc<AppState>>,
) -> impl axum::response::IntoResponse {
    let db_ok = state.db.pool().acquire().await.is_ok();
    let status = if db_ok { "healthy" } else { "degraded" };
    let status_code = if db_ok {
        StatusCode::OK
    } else {
        StatusCode::SERVICE_UNAVAILABLE
    };
    let response = serde_json::json!({
        "status": status,
        "version": state.version,
        "uptime_secs": state.start_time.elapsed().as_secs(),
        "checks": {
            "database": if db_ok { "connected" } else { "unreachable" },
        },
    });
    (status_code, Json(response))
}
