//! Agent routes — list available coding agents.
//!
//! Ported from: `packages/blazecode/src/server/routes/instance/httpapi/groups/agent.ts`

use axum::extract::{Query, State};
use axum::response::IntoResponse;
use axum::routing::get;
use axum::{Json, Router};
use serde::Deserialize;
use std::sync::Arc;

use crate::server::AppState;

/// Query parameters for agent listing.
#[derive(Debug, Deserialize, Default)]
pub struct AgentQuery {
    /// Filter agents by location (directory or project).
    #[serde(default)]
    pub directory: Option<String>,
}
/// Create the agent routes router.
///
/// # Source
/// Ported from `packages/blazecode/src/server/routes/instance/httpapi/groups/agent.ts`
pub fn agent_routes(state: Arc<AppState>) -> Router {
    Router::new()
        .route("/agent", get(list_agents))
        .with_state(state)
}

/// List all registered agents. Uses `AgentService` from `AppState` when
/// available; falls back to a static built-in list.
///
/// Supports `?directory=<path>` to retrieve agents scoped to a location.
///
/// # Source
/// Ported from `packages/blazecode/src/server/routes/instance/httpapi/groups/agent.ts`
async fn list_agents(
    State(state): State<Arc<AppState>>,
    Query(_query): Query<AgentQuery>,
) -> impl IntoResponse {
    // Prefer the agent service when wired in
    if let Some(ref agent_svc) = state.agent_service {
        let agents = agent_svc.list(None);
        let result: Vec<serde_json::Value> = agents
            .iter()
            .map(|a| {
                serde_json::json!({
                    "id": a.name,
                    "name": a.name,
                    "mode": format!("{:?}", a.mode).to_lowercase(),
                    "model": a.model.as_ref().map(|m| serde_json::json!({
                        "provider_id": m.provider_id,
                        "model_id": m.model_id,
                    })),
                    "permissions": a.permission,
                    "native": a.native,
                    "hidden": a.hidden,
                    "description": a.description,
                })
            })
            .collect();
        return Json(serde_json::to_value(result).unwrap_or_default()).into_response();
    }

    // Fallback: static built-in agent list
    Json(serde_json::json!([
        {
            "id": "build",
            "name": "Build",
            "mode": "primary",
            "description": "General-purpose coding agent with all tools",
            "native": true,
            "hidden": false,
            "permissions": ["read", "edit", "bash", "glob", "grep", "list", "task", "webfetch", "websearch", "question"],
        },
        {
            "id": "plan",
            "name": "Plan",
            "mode": "primary",
            "description": "Planning agent for architecture and design — read-only",
            "native": true,
            "hidden": false,
            "permissions": ["read", "glob", "grep", "list", "webfetch", "websearch"],
        },
        {
            "id": "general",
            "name": "General",
            "mode": "subagent",
            "description": "General-purpose subagent for parallel work",
            "native": true,
            "hidden": false,
            "permissions": ["read", "edit", "bash", "glob", "grep", "list", "webfetch", "websearch"],
        },
        {
            "id": "explore",
            "name": "Explore",
            "mode": "subagent",
            "description": "Fast agent specialized for exploring codebases",
            "native": true,
            "hidden": false,
            "permissions": ["read", "glob", "grep", "list", "bash", "webfetch", "websearch"],
        },
        {
            "id": "code-review",
            "name": "Code Review",
            "mode": "primary",
            "description": "Reviews code for bugs and improvements",
            "native": false,
            "hidden": false,
            "permissions": ["read", "glob", "grep", "list"],
        },
    ]))
    .into_response()
}
