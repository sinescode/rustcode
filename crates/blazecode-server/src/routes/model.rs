//! Model routes — list available LLM models from registered providers.
//!
//! Ported from: `packages/blazecode/src/server/routes/instance/httpapi/groups/model.rs`

use axum::extract::{Query, State};
use axum::response::IntoResponse;
use axum::routing::get;
use axum::{Json, Router};
use serde::Deserialize;
use std::sync::Arc;
use tracing::{info, warn};

use crate::server::AppState;

/// Query parameters for model listing.
#[derive(Debug, Deserialize, Default)]
pub struct ModelQuery {
    /// Filter models by provider ID (e.g. "anthropic", "openai").
    #[serde(default)]
    pub provider: Option<String>,

    /// When true, include full metadata (cost, token limits, capabilities).
    #[serde(default)]
    pub verbose: Option<bool>,
}

/// Create the model routes router.
///
/// # Source
/// Ported from `packages/blazecode/src/server/routes/instance/httpapi/groups/model.ts`
pub fn model_routes(state: Arc<AppState>) -> Router {
    Router::new()
        .route("/model", get(list_models))
        .with_state(state)
}

/// List every model from every registered provider.
///
/// Supports `?provider=<id>` and `?verbose=true` query parameters.
///
/// # Source
/// Ported from `packages/blazecode/src/server/routes/instance/httpapi/groups/model.ts`
async fn list_models(
    State(state): State<Arc<AppState>>,
    Query(query): Query<ModelQuery>,
) -> impl IntoResponse {
    let verbose = query.verbose.unwrap_or(false);
    let mut models: Vec<serde_json::Value> = Vec::new();

    for (provider_id, provider) in &state.providers {
        // Skip providers that don't match the filter
        if let Some(ref filter) = query.provider {
            if provider_id != filter {
                continue;
            }
        }

        match provider.list_models().await {
            Ok(provider_models) => {
                for model in provider_models {
                    let mut entry = serde_json::json!({
                        "id": model.id,
                        "provider_id": provider_id,
                        "name": model.name,
                    });

                    if verbose {
                        entry["capabilities"] = serde_json::json!({
                            "temperature": model.capabilities.temperature,
                            "reasoning": model.capabilities.reasoning,
                            "attachment": model.capabilities.attachment,
                            "toolcall": model.capabilities.toolcall,
                            "input": {
                                "text": model.capabilities.input.text,
                                "image": model.capabilities.input.image,
                                "audio": model.capabilities.input.audio,
                                "video": model.capabilities.input.video,
                                "pdf": model.capabilities.input.pdf,
                            },
                            "output": {
                                "text": model.capabilities.output.text,
                                "image": model.capabilities.output.image,
                                "audio": model.capabilities.output.audio,
                            },
                        });
                        entry["context_window"] = serde_json::json!({
                            "context": model.limit.context,
                            "input": model.limit.input,
                            "output": model.limit.output,
                        });
                        entry["cost"] = serde_json::json!({
                            "input": model.cost.input,
                            "output": model.cost.output,
                            "cache_read": model.cost.cache.read,
                            "cache_write": model.cost.cache.write,
                        });
                        entry["status"] =
                            serde_json::json!(format!("{:?}", model.status).to_lowercase());
                        entry["family"] =
                            serde_json::Value::String(model.family.unwrap_or_default());
                        entry["api_id"] = serde_json::Value::String(model.api.id);
                        entry["release_date"] = serde_json::Value::String(model.release_date);
                    }

                    models.push(entry);
                }
            }
            Err(e) => {
                warn!("Failed to list models for provider '{}': {e}", provider_id);
            }
        }
    }

    info!(
        "Listed {} models across {} providers",
        models.len(),
        state.providers.len()
    );
    Json(serde_json::to_value(models).unwrap_or_default()).into_response()
}
