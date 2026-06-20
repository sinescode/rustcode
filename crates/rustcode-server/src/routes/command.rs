//! Command routes — list available commands (slash commands) from config and tools.
//!
//! Ported from: `packages/opencode/src/server/routes/instance/httpapi/groups/command.ts`

use axum::extract::{Query, State};
use axum::response::IntoResponse;
use axum::routing::get;
use axum::{Json, Router};
use serde::Deserialize;
use std::sync::Arc;

use crate::server::AppState;

/// Query parameters for command listing.
#[derive(Debug, Deserialize, Default)]
pub struct CommandQuery {
    /// Filter by agent type (e.g. "build", "plan").
    #[serde(default)]
    pub agent: Option<String>,
}

/// Create the command routes router.
///
/// # Source
/// Ported from `packages/opencode/src/server/routes/instance/httpapi/groups/command.ts`
pub fn command_routes(state: Arc<AppState>) -> Router {
    Router::new()
        .route("/command", get(list_commands))
        .with_state(state)
}

/// List commands from user config and tool registry.
///
/// Commands from `command_data` (user-defined slash commands) take priority.
/// Tools from the `ToolRegistry` are also exposed as available commands.
/// Supports `?agent=<name>` to filter.
///
/// # Source
/// Ported from `packages/opencode/src/server/routes/instance/httpapi/groups/command.ts`
async fn list_commands(
    State(state): State<Arc<AppState>>,
    Query(query): Query<CommandQuery>,
) -> impl IntoResponse {
    let mut commands: Vec<serde_json::Value> = Vec::new();

    // -- User-defined commands from config ──────────────────────────────
    for cmd in state.command_data.list() {
        // Filter by agent if requested
        if let Some(ref agent_filter) = query.agent {
            if let Some(ref cmd_agent) = cmd.agent {
                if cmd_agent != agent_filter {
                    continue;
                }
            } else if agent_filter != "build" {
                // Commands without an explicit agent default to "build"
                continue;
            }
        }
        commands.push(serde_json::json!({
            "id": cmd.name,
            "name": cmd.name,
            "description": cmd.description,
            "template": cmd.template,
            "agent": cmd.agent,
            "model": cmd.model.as_ref().map(|m| serde_json::json!({
                "provider_id": m.provider_id,
                "model_id": m.model_id,
            })),
            "subtask": cmd.subtask,
            "source": "config",
        }));
    }

    // -- Tool-based commands from registry ─────────────────────────────
    for tool in state.tools.list_tools_info() {
        commands.push(serde_json::json!({
            "id": tool.id,
            "name": tool.id,
            "description": tool.description,
            "agent": null,
            "model": null,
            "subtask": false,
            "source": "tool",
        }));
    }

    Json(serde_json::to_value(commands).unwrap_or_default()).into_response()
}
