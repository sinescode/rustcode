//! Query route — structured data queries for web UI data fetching.
//!
//! Supports query params: `type` (sessions/models/stats), `limit`, `offset`,
//! and `filters` (JSON object).

use axum::extract::{Query, State};
use axum::response::IntoResponse;
use axum::routing::get;
use axum::{Json, Router};
use serde::Deserialize;
use std::collections::HashMap;
use std::sync::Arc;
use tracing::warn;

use crate::server::AppState;

/// Query parameters for structured data queries.
#[derive(Debug, Deserialize)]
pub struct DataQuery {
    /// The type of data to query: "sessions", "models", "stats", or "projects".
    #[serde(rename = "type")]
    pub query_type: String,

    /// Maximum number of results to return (default 20, max 100).
    #[serde(default = "default_limit")]
    pub limit: usize,

    /// Number of results to skip for pagination.
    #[serde(default)]
    pub offset: usize,

    /// Optional JSON-encoded filter object.
    #[serde(default)]
    pub filters: Option<String>,
}

fn default_limit() -> usize {
    20
}

/// Create the query routes router.
pub fn query_routes(state: Arc<AppState>) -> Router {
    Router::new()
        .route("/query", get(query_data))
        .with_state(state)
}

/// Execute a structured data query based on the `type` parameter.
///
/// Supported types:
/// - `sessions` — List recent sessions with optional filtering.
/// - `models` — List available models with counts per provider.
/// - `stats` — Aggregate statistics (session counts, token usage, costs).
/// - `projects` — List known projects with session counts.
async fn query_data(
    State(state): State<Arc<AppState>>,
    Query(query): Query<DataQuery>,
) -> impl IntoResponse {
    let limit = query.limit.min(100);
    let offset = query.offset;
    let filters: HashMap<String, serde_json::Value> = query
        .filters
        .as_deref()
        .and_then(|s| serde_json::from_str(s).ok())
        .unwrap_or_default();

    match query.query_type.as_str() {
        "sessions" => query_sessions(&state, limit, offset, &filters).await.into_response(),
        "models" => query_models(&state).await.into_response(),
        "stats" => query_stats(&state).await.into_response(),
        "projects" => query_projects(&state, limit, offset).await.into_response(),
        _ => Json(serde_json::json!({
            "error": format!("unknown query type: '{}'. Supported: sessions, models, stats, projects", query.query_type),
            "supported_types": ["sessions", "models", "stats", "projects"],
        }))
        .into_response(),
    }
}

/// Query sessions with optional filtering.
async fn query_sessions(
    state: &AppState,
    limit: usize,
    offset: usize,
    filters: &HashMap<String, serde_json::Value>,
) -> impl IntoResponse {
    let directory = filters
        .get("directory")
        .and_then(|v| v.as_str())
        .map(|s| s.to_string());

    let search = filters
        .get("search")
        .and_then(|v| v.as_str())
        .map(|s| s.to_string());

    let project_id = filters
        .get("project_id")
        .and_then(|v| v.as_str())
        .map(|s| s.to_string());

    let list_input = blazecode_core::session::ListSessionsInput {
        project_id,
        directory,
        search,
        limit: Some(limit + offset),
        ..Default::default()
    };

    match state.sessions.list(Some(list_input)).await {
        Ok(all_sessions) => {
            let total = all_sessions.len();
            let sessions: Vec<serde_json::Value> = all_sessions
                .into_iter()
                .skip(offset)
                .take(limit)
                .map(|s| {
                    serde_json::json!({
                        "id": s.id,
                        "title": s.title,
                        "directory": s.directory,
                        "agent": s.agent,
                        "model": s.model,
                        "cost": s.cost,
                        "tokens": s.tokens,
                        "created": s.time.created,
                        "updated": s.time.updated,
                    })
                })
                .collect();

            Json(serde_json::json!({
                "type": "sessions",
                "total": total,
                "limit": limit,
                "offset": offset,
                "results": sessions,
            }))
            .into_response()
        }
        Err(e) => {
            warn!("Failed to list sessions: {e}");
            Json(serde_json::json!({
                "type": "sessions",
                "error": format!("Failed to list sessions: {e}"),
                "results": [],
            }))
            .into_response()
        }
    }
}

/// Query models grouped by provider.
async fn query_models(state: &AppState) -> impl IntoResponse {
    let mut provider_models: Vec<serde_json::Value> = Vec::new();

    for (provider_id, provider) in &state.providers {
        match provider.list_models().await {
            Ok(models) => {
                let model_ids: Vec<String> = models.iter().map(|m| m.id.clone()).collect();
                provider_models.push(serde_json::json!({
                    "provider_id": provider_id,
                    "model_count": models.len(),
                    "models": model_ids,
                }));
            }
            Err(e) => {
                warn!("Failed to list models for provider '{}': {e}", provider_id);
            }
        }
    }

    Json(serde_json::json!({
        "type": "models",
        "provider_count": state.providers.len(),
        "providers": provider_models,
    }))
    .into_response()
}

/// Query aggregate statistics.
async fn query_stats(state: &AppState) -> impl IntoResponse {
    // Count sessions (approximate — list all)
    let session_count = state
        .sessions
        .list(Some(blazecode_core::session::ListSessionsInput {
            limit: Some(1),
            ..Default::default()
        }))
        .await
        .map(|s| s.len())
        .unwrap_or(0);

    let tool_count = state.tools.list_tools_info().len();
    let provider_count = state.providers.len();
    let uptime_seconds = state.start_time.elapsed().as_secs();
    let connected_clients = state.bus.receiver_count();

    Json(serde_json::json!({
        "type": "stats",
        "session_count": session_count,
        "tool_count": tool_count,
        "provider_count": provider_count,
        "connected_clients": connected_clients,
        "uptime_seconds": uptime_seconds,
        "version": state.version,
    }))
    .into_response()
}

/// Query projects with session counts.
async fn query_projects(_state: &AppState, limit: usize, offset: usize) -> impl IntoResponse {
    // In a full implementation, this would query a projects table.
    // For now, return a placeholder.
    Json(serde_json::json!({
        "type": "projects",
        "total": 0,
        "limit": limit,
        "offset": offset,
        "results": [],
    }))
    .into_response()
}
