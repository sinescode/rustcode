//! Reference routes — list available code references and context items.
//!
//! Ported from: `packages/opencode/src/server/routes/instance/httpapi/groups/reference.ts`

use axum::extract::{Query, State};
use axum::response::IntoResponse;
use axum::routing::get;
use axum::{Json, Router};
use serde::Deserialize;
use std::sync::Arc;

use crate::server::AppState;

/// Query parameters for reference listing.
#[derive(Debug, Deserialize, Default)]
pub struct ReferenceQuery {
    /// Scope references to a specific directory.
    #[serde(default)]
    pub directory: Option<String>,

    /// When true, include hidden references.
    #[serde(default)]
    pub include_hidden: Option<bool>,
}

/// Create the reference routes router.
///
/// # Source
/// Ported from `packages/opencode/src/server/routes/instance/httpapi/groups/reference.ts`
pub fn reference_routes(state: Arc<AppState>) -> Router {
    Router::new()
        .route("/reference", get(list_references))
        .with_state(state)
}

/// List all available code references.
///
/// Returns references from the `ReferenceService`, including local directories
/// and git repositories. Supports `?directory=<path>` scoping and
/// `?include_hidden=true` to show hidden references.
///
/// # Source
/// Ported from `packages/opencode/src/server/routes/instance/httpapi/groups/reference.ts`
async fn list_references(
    State(state): State<Arc<AppState>>,
    Query(query): Query<ReferenceQuery>,
) -> impl IntoResponse {
    let include_hidden = query.include_hidden.unwrap_or(false);

    let references: Vec<serde_json::Value> = state
        .reference_service
        .list()
        .into_iter()
        .filter(|(_, source)| include_hidden || !source.is_hidden())
        .filter(|(_, source)| {
            if let Some(ref dir) = query.directory {
                // Only include references whose path contains the directory filter
                source.path_hint().contains(dir.as_str())
            } else {
                true
            }
        })
        .map(|(name, source)| {
            serde_json::json!({
                "name": name,
                "type": source.source_type(),
                "path": source.path_hint(),
                "description": source.description(),
                "hidden": source.is_hidden(),
            })
        })
        .collect();

    Json(serde_json::to_value(references).unwrap_or_default()).into_response()
}
