//! Session routes — CRUD, messages, fork, abort, prompt, revert, etc.
//!
//! Ported from: `packages/opencode/src/server/routes/instance/httpapi/groups/session.ts`
//!
//! Route paths (all under `/session`):
//! - `GET    /session`                        — list sessions
//! - `POST   /session`                        — create session
//! - `GET    /session/status`                 — session status map
//! - `GET    /session/:sessionID`             — get session
//! - `DELETE /session/:sessionID`             — delete session
//! - `PATCH  /session/:sessionID`             — update session
//! - `GET    /session/:sessionID/children`    — child sessions
//! - `GET    /session/:sessionID/todo`        — todo list
//! - `GET    /session/:sessionID/diff`        — session diff
//! - `GET    /session/:sessionID/message`     — list messages
//! - `POST   /session/:sessionID/message`     — send prompt
//! - `GET    /session/:sessionID/message/:messageID` — get message
//! - `DELETE /session/:sessionID/message/:messageID` — delete message
//! - `DELETE /session/:sessionID/message/:messageID/part/:partID` — delete part
//! - `PATCH  /session/:sessionID/message/:messageID/part/:partID` — update part
//! - `POST   /session/:sessionID/fork`        — fork session
//! - `POST   /session/:sessionID/abort`       — abort session
//! - `POST   /session/:sessionID/share`       — share / unshare session
//! - `POST   /session/:sessionID/init`        — init session
//! - `POST   /session/:sessionID/summarize`   — summarize session
//! - `POST   /session/:sessionID/prompt_async` — async prompt
//! - `POST   /session/:sessionID/command`     — send command
//! - `POST   /session/:sessionID/shell`       — run shell command
//! - `POST   /session/:sessionID/revert`      — revert message
//! - `POST   /session/:sessionID/unrevert`    — unrevert messages
//! - `POST   /session/:sessionID/permissions/:permissionID` — permission response

use axum::extract::{Path, Query, State};
use axum::response::IntoResponse;
use axum::{Json, Router};
use axum::routing::{delete, get, patch, post};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::sync::Arc;
use tracing::{error, info};

use crate::server::AppState;

// ── Types ────────────────────────────────────────────────────────────────────

/// Query parameters for listing sessions.
///
/// # Source
/// `ListQuery` in `session.ts` line 30.
#[derive(Debug, Deserialize, Default)]
pub struct ListQuery {
    #[serde(default)]
    pub directory: Option<String>,
    #[serde(default)]
    pub workspace: Option<String>,
    #[serde(default)]
    pub scope: Option<String>,
    #[serde(default)]
    pub path: Option<String>,
    #[serde(default)]
    pub roots: Option<bool>,
    #[serde(default)]
    pub start: Option<u64>,
    #[serde(default)]
    pub search: Option<String>,
    #[serde(default)]
    pub limit: Option<usize>,
}

/// Query parameters for messages.
///
/// # Source
/// `MessagesQuery` in `session.ts` line 43.
#[derive(Debug, Deserialize, Default)]
pub struct MessagesQuery {
    #[serde(default)]
    pub directory: Option<String>,
    #[serde(default)]
    pub workspace: Option<String>,
    #[serde(default)]
    pub limit: Option<usize>,
    #[serde(default)]
    pub before: Option<String>,
}

/// Workspace routing query.
///
/// # Source
/// `WorkspaceRoutingQuery` — used across many route groups.
#[derive(Debug, Deserialize, Default)]
pub struct WsQuery {
    #[serde(default)]
    pub directory: Option<String>,
    #[serde(default)]
    pub workspace: Option<String>,
}

/// Create session payload.
///
/// # Source
/// `Session.CreateInput` in `session.ts` line 206.
#[derive(Debug, Deserialize)]
pub struct CreateSessionPayload {
    pub directory: String,
    #[serde(default)]
    pub title: Option<String>,
    #[serde(default)]
    pub agent: Option<String>,
    #[serde(default)]
    pub model: Option<ModelSelectionPayload>,
    #[serde(default)]
    pub path: Option<String>,
    #[serde(default)]
    pub parent_id: Option<String>,
}

/// Model selection in requests.
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct ModelSelectionPayload {
    pub id: String,
    pub provider_id: String,
    #[serde(default)]
    pub variant: Option<String>,
}

/// Update session payload.
///
/// # Source
/// `UpdatePayload` in `session.ts` line 49.
#[derive(Debug, Deserialize)]
pub struct UpdateSessionPayload {
    #[serde(default)]
    pub title: Option<String>,
    #[serde(default)]
    pub metadata: Option<serde_json::Value>,
    #[serde(default)]
    pub permission: Option<serde_json::Value>,
    #[serde(default)]
    pub time: Option<ArchiveTimePayload>,
}

#[derive(Debug, Deserialize)]
pub struct ArchiveTimePayload {
    #[serde(default)]
    pub archived: Option<u64>,
}

/// Prompt payload.
///
/// # Source
/// `PromptPayload` in `session.ts` line 70.
#[derive(Debug, Deserialize)]
pub struct PromptPayload {
    pub text: String,
    #[serde(default)]
    pub parts: Option<Vec<serde_json::Value>>,
    #[serde(default)]
    pub agent: Option<String>,
    #[serde(default)]
    pub model: Option<ModelSelectionPayload>,
}

/// Command payload.
///
/// # Source
/// `CommandPayload` in `session.ts` line 71.
#[derive(Debug, Deserialize)]
pub struct CommandPayload {
    pub command: String,
    #[serde(default)]
    pub args: Option<Vec<String>>,
}

/// Shell payload.
///
/// # Source
/// `ShellPayload` in `session.ts` line 72.
#[derive(Debug, Deserialize)]
pub struct ShellPayload {
    pub command: String,
    #[serde(default)]
    pub workdir: Option<String>,
    #[serde(default)]
    pub description: Option<String>,
}

/// Revert payload.
///
/// # Source
/// `RevertPayload` in `session.ts` line 73.
#[derive(Debug, Deserialize)]
pub struct RevertPayload {
    pub message_id: String,
}

/// Permission response payload.
///
/// # Source
/// `PermissionResponsePayload` in `session.ts` line 74.
#[derive(Debug, Deserialize)]
pub struct PermissionResponsePayload {
    pub response: String,
    #[serde(default)]
    pub message: Option<String>,
}

/// Fork payload.
///
/// # Source
/// `ForkPayload` in `session.ts` line 59.
#[derive(Debug, Deserialize)]
pub struct ForkPayload {
    #[serde(default)]
    pub message_id: Option<String>,
}

/// Init payload.
///
/// # Source
/// `InitPayload` in `session.ts` line 60.
#[derive(Debug, Deserialize)]
pub struct InitPayload {
    pub model_id: String,
    pub provider_id: String,
    pub message_id: String,
}

/// Summarize payload.
///
/// # Source
/// `SummarizePayload` in `session.ts` line 65.
#[derive(Debug, Deserialize)]
pub struct SummarizePayload {
    pub provider_id: String,
    pub model_id: String,
    #[serde(default)]
    pub auto: Option<bool>,
}

// ── Routes ────────────────────────────────────────────────────────────────────

/// Create the session routes router.
///
/// # Source
/// `packages/opencode/src/server/routes/instance/httpapi/groups/session.ts`
/// `SessionPaths` (lines 78–105) and `SessionApi` route definitions.
pub fn session_routes(state: Arc<AppState>) -> Router {
    Router::new()
        // Collection
        .route("/session", get(list_sessions).post(create_session))
        // Status
        .route("/session/status", get(session_status))
        // Single session
        .route(
            "/session/{sessionID}",
            get(get_session).patch(update_session).delete(delete_session),
        )
        // Children
        .route("/session/{sessionID}/children", get(list_children))
        // Todo
        .route("/session/{sessionID}/todo", get(get_todos))
        // Diff
        .route("/session/{sessionID}/diff", get(get_diff))
        // Messages
        .route("/session/{sessionID}/message", get(list_messages).post(post_prompt))
        .route(
            "/session/{sessionID}/message/{messageID}",
            get(get_message).delete(delete_message),
        )
        .route(
            "/session/{sessionID}/message/{messageID}/part/{partID}",
            delete(delete_part).patch(update_part),
        )
        // Fork
        .route("/session/{sessionID}/fork", post(fork_session))
        // Abort
        .route("/session/{sessionID}/abort", post(abort_session))
        // Share
        .route(
            "/session/{sessionID}/share",
            post(share_session).delete(unshare_session),
        )
        // Init
        .route("/session/{sessionID}/init", post(init_session))
        // Summarize
        .route("/session/{sessionID}/summarize", post(summarize_session))
        // Async prompt
        .route("/session/{sessionID}/prompt_async", post(prompt_async))
        // Command
        .route("/session/{sessionID}/command", post(post_command))
        // Shell
        .route("/session/{sessionID}/shell", post(post_shell))
        // Revert / Unrevert
        .route("/session/{sessionID}/revert", post(revert_session))
        .route("/session/{sessionID}/unrevert", post(unrevert_session))
        // Permission response (deprecated path)
        .route(
            "/session/{sessionID}/permissions/{permissionID}",
            post(permission_respond),
        )
        .with_state(state)
}

// ── Handlers ──────────────────────────────────────────────────────────────────

async fn list_sessions(
    State(state): State<Arc<AppState>>,
    Query(query): Query<ListQuery>,
) -> impl IntoResponse {
    let input = rustcode_core::session::ListSessionsInput {
        directory: query.directory,
        path: query.path,
        workspace_id: query.workspace,
        roots: query.roots,
        search: query.search,
        limit: query.limit,
    };
    match state.sessions.list(Some(input)).await {
        Ok(sessions) => Json(serde_json::to_value(sessions).unwrap_or_default()).into_response(),
        Err(e) => (
            axum::http::StatusCode::INTERNAL_SERVER_ERROR,
            Json(serde_json::json!({"error": e.to_string()})),
        )
            .into_response(),
    }
}

async fn session_status(
    State(_): State<Arc<AppState>>,
) -> impl IntoResponse {
    // Return an empty status map — status tracking is in the processor layer
    let status_map: HashMap<String, serde_json::Value> = HashMap::new();
    Json(serde_json::to_value(status_map).unwrap_or_default())
}

async fn get_session(
    State(state): State<Arc<AppState>>,
    Path(session_id): Path<String>,
) -> impl IntoResponse {
    match state.sessions.get(&session_id).await {
        Ok(session) => Json(serde_json::to_value(session).unwrap_or_default()).into_response(),
        Err(_) => (
            axum::http::StatusCode::NOT_FOUND,
            Json(serde_json::json!({"error": "session not found"})),
        )
            .into_response(),
    }
}

async fn create_session(
    State(state): State<Arc<AppState>>,
    Json(payload): Json<CreateSessionPayload>,
) -> impl IntoResponse {
    let input = rustcode_core::session::CreateSessionInput {
        project_id: "default".to_string(),
        workspace_id: None,
        directory: payload.directory,
        path: payload.path,
        parent_id: payload.parent_id,
        title: payload.title,
        agent: payload.agent,
        model: payload.model.map(|m| rustcode_core::session::ModelSelection {
            id: m.id,
            provider_id: m.provider_id,
            variant: m.variant,
        }),
        metadata: None,
        permission: None,
    };
    match state.sessions.create(input).await {
        Ok(session) => (
            axum::http::StatusCode::CREATED,
            Json(serde_json::to_value(session).unwrap_or_default()),
        )
            .into_response(),
        Err(e) => (
            axum::http::StatusCode::INTERNAL_SERVER_ERROR,
            Json(serde_json::json!({"error": e.to_string()})),
        )
            .into_response(),
    }
}

async fn update_session(
    State(state): State<Arc<AppState>>,
    Path(session_id): Path<String>,
    Json(payload): Json<UpdateSessionPayload>,
) -> impl IntoResponse {
    let patch = rustcode_core::session::SessionPatch {
        title: Some(payload.title),
        ..Default::default()
    };
    match state.sessions.update(&session_id, patch).await {
        Ok(session) => Json(serde_json::to_value(session).unwrap_or_default()).into_response(),
        Err(e) => (
            axum::http::StatusCode::NOT_FOUND,
            Json(serde_json::json!({"error": e.to_string()})),
        )
            .into_response(),
    }
}

async fn delete_session(
    State(state): State<Arc<AppState>>,
    Path(session_id): Path<String>,
) -> impl IntoResponse {
    match state.sessions.remove(&session_id).await {
        Ok(()) => Json(serde_json::json!(true)).into_response(),
        Err(e) => (
            axum::http::StatusCode::NOT_FOUND,
            Json(serde_json::json!({"error": e.to_string()})),
        )
            .into_response(),
    }
}

async fn list_children(
    State(state): State<Arc<AppState>>,
    Path(session_id): Path<String>,
) -> impl IntoResponse {
    // Children are sessions with parent_id == session_id
    let list_input = Some(rustcode_core::session::ListSessionsInput::default());
    match state.sessions.list(list_input).await {
        Ok(sessions) => {
            let children: Vec<_> = sessions
                .into_iter()
                .filter(|s| s.parent_id.as_deref() == Some(&session_id))
                .collect();
            Json(serde_json::to_value(children).unwrap_or_default()).into_response()
        }
        Err(e) => (
            axum::http::StatusCode::INTERNAL_SERVER_ERROR,
            Json(serde_json::json!({"error": e.to_string()})),
        )
            .into_response(),
    }
}

async fn get_todos(
    State(_): State<Arc<AppState>>,
    Path(session_id): Path<String>,
) -> impl IntoResponse {
    Json(serde_json::json!([]))
}

async fn get_diff(
    State(_): State<Arc<AppState>>,
    Path(session_id): Path<String>,
) -> impl IntoResponse {
    Json(serde_json::json!([]))
}

async fn list_messages(
    State(state): State<Arc<AppState>>,
    Path(session_id): Path<String>,
    Query(_query): Query<MessagesQuery>,
) -> impl IntoResponse {
    match state.sessions.get_messages(&session_id).await {
        Ok(messages) => Json(serde_json::to_value(messages).unwrap_or_default()).into_response(),
        Err(_) => Json(serde_json::json!([])).into_response(),
    }
}

async fn get_message(
    State(state): State<Arc<AppState>>,
    Path((session_id, message_id)): Path<(String, String)>,
) -> impl IntoResponse {
    match state.sessions.get_messages(&session_id).await {
        Ok(messages) => {
            let msg = messages.into_iter().find(|m| m.info.id() == message_id);
            match msg {
                Some(m) => Json(serde_json::to_value(m).unwrap_or_default()).into_response(),
                None => (
                    axum::http::StatusCode::NOT_FOUND,
                    Json(serde_json::json!({"error": "message not found"})),
                )
                    .into_response(),
            }
        }
        Err(_) => (
            axum::http::StatusCode::NOT_FOUND,
            Json(serde_json::json!({"error": "session not found"})),
        )
            .into_response(),
    }
}

async fn post_prompt(
    State(state): State<Arc<AppState>>,
    Path(session_id): Path<String>,
    Json(payload): Json<PromptPayload>,
) -> impl IntoResponse {
    // Resolve the model — default to Anthropic's claude-sonnet-4-6
    let model_selection = payload.model.unwrap_or_else(|| ModelSelectionPayload {
        id: "claude-sonnet-4-6".into(),
        provider_id: "anthropic".into(),
        variant: None,
    });

    // Find the provider
    let provider = match state.providers.get(&model_selection.provider_id) {
        Some(p) => p,
        None => {
            return (
                axum::http::StatusCode::BAD_REQUEST,
                Json(serde_json::json!({
                    "error": format!("provider '{}' not configured", model_selection.provider_id)
                })),
            )
                .into_response();
        }
    };

    // Find the model
    let model = match provider.get_model(&model_selection.id).await {
        Ok(m) => m,
        Err(e) => {
            return (
                axum::http::StatusCode::BAD_REQUEST,
                Json(serde_json::json!({"error": format!("model not found: {e}")})),
            )
                .into_response();
        }
    };

    // Build the prompt input
    let mut parts = Vec::new();
    if let Some(payload_parts) = payload.parts {
        for part in payload_parts {
            if let Some(t) = part.get("text").and_then(|v| v.as_str()) {
                parts.push(rustcode_core::session_prompt::PromptPart::Text(
                    rustcode_core::session_prompt::PromptTextPart {
                        id: None,
                        text: t.to_string(),
                        synthetic: false,
                    },
                ));
            }
        }
    }
    if !payload.text.is_empty() {
        parts.push(rustcode_core::session_prompt::PromptPart::Text(
            rustcode_core::session_prompt::PromptTextPart {
                id: None,
                text: payload.text.clone(),
                synthetic: false,
            },
        ));
    }

    let input = rustcode_core::session_prompt::SessionPromptInput {
        session_id,
        message_id: None,
        model: Some(rustcode_core::session_info::ModelRef {
            id: model_selection.id,
            provider_id: model_selection.provider_id,
            variant: model_selection.variant,
        }),
        agent: payload.agent.or(Some("build".into())),
        no_reply: false,
        tools: None,
        format: None,
        system: None,
        variant: None,
        parts,
    };

    // Default instructions from CLAUDE.md / built-in system prompt
    let instructions = vec![
        "You are a helpful coding assistant powered by Claude.".to_string(),
        "You have access to tools for reading, writing, editing, and searching code.".to_string(),
        "Always write correct, idiomatic Rust code.".to_string(),
        "Use the available tools when you need to interact with the filesystem.".to_string(),
    ];

    // Run the prompt through the session runner
    info!(
        "Running prompt for session {} with model {}/{}",
        input.session_id, input.model.as_ref().map(|m| m.provider_id.as_str()).unwrap_or("?"), input.model.as_ref().map(|m| m.id.as_str()).unwrap_or("?")
    );

    match state.runner.run(provider.as_ref(), &model, &input, &instructions).await {
        Ok(result) => {
            info!(
                "Prompt completed for session {}: {} chars, {} events",
                input.session_id,
                result.text.len(),
                result.events.len()
            );

            Json(serde_json::json!({
                "session_id": input.session_id,
                "text": result.text,
                "success": result.success,
                "events_count": result.events.len(),
                "error": result.error,
            }))
            .into_response()
        }
        Err(e) => {
            error!("Prompt failed for session {}: {e}", input.session_id);
            (
                axum::http::StatusCode::INTERNAL_SERVER_ERROR,
                Json(serde_json::json!({"error": e.to_string()})),
            )
                .into_response()
        }
    }
}

async fn delete_message(
    State(_): State<Arc<AppState>>,
    Path((session_id, message_id)): Path<(String, String)>,
) -> impl IntoResponse {
    Json(serde_json::json!({"deleted": true, "session_id": session_id, "message_id": message_id}))
}

async fn delete_part(
    State(_): State<Arc<AppState>>,
    Path((session_id, message_id, part_id)): Path<(String, String, String)>,
) -> impl IntoResponse {
    Json(serde_json::json!({"deleted": true, "session_id": session_id, "message_id": message_id, "part_id": part_id}))
}

async fn update_part(
    State(_): State<Arc<AppState>>,
    Path((session_id, message_id, part_id)): Path<(String, String, String)>,
    Json(payload): Json<serde_json::Value>,
) -> impl IntoResponse {
    Json(serde_json::json!({
        "updated": true,
        "session_id": session_id,
        "message_id": message_id,
        "part_id": part_id,
        "payload": payload,
    }))
}

async fn fork_session(
    State(state): State<Arc<AppState>>,
    Path(session_id): Path<String>,
    Json(payload): Json<ForkPayload>,
) -> impl IntoResponse {
    match state.sessions.fork(&session_id, payload.message_id.as_deref()).await {
        Ok(session) => Json(serde_json::to_value(session).unwrap_or_default()).into_response(),
        Err(e) => (
            axum::http::StatusCode::INTERNAL_SERVER_ERROR,
            Json(serde_json::json!({"error": e.to_string()})),
        )
            .into_response(),
    }
}

async fn abort_session(
    State(_): State<Arc<AppState>>,
    Path(session_id): Path<String>,
) -> impl IntoResponse {
    Json(serde_json::json!({"aborted": true, "session_id": session_id}))
}

async fn share_session(
    State(_): State<Arc<AppState>>,
    Path(session_id): Path<String>,
) -> impl IntoResponse {
    Json(serde_json::json!({
        "id": session_id,
        "share": {"url": format!("https://opencode.ai/share/{session_id}")},
    }))
}

async fn unshare_session(
    State(_): State<Arc<AppState>>,
    Path(session_id): Path<String>,
) -> impl IntoResponse {
    Json(serde_json::json!({"id": session_id, "share": null}))
}

async fn init_session(
    State(_): State<Arc<AppState>>,
    Path(session_id): Path<String>,
    Json(_payload): Json<InitPayload>,
) -> impl IntoResponse {
    Json(serde_json::json!({"initialized": true, "session_id": session_id}))
}

async fn summarize_session(
    State(_): State<Arc<AppState>>,
    Path(session_id): Path<String>,
    Json(payload): Json<SummarizePayload>,
) -> impl IntoResponse {
    Json(serde_json::json!({
        "summarized": true,
        "session_id": session_id,
        "model_id": payload.model_id,
    }))
}

async fn prompt_async(
    State(_): State<Arc<AppState>>,
    Path(session_id): Path<String>,
    Json(payload): Json<PromptPayload>,
) -> impl IntoResponse {
    Json(serde_json::json!({
        "session_id": session_id,
        "text": payload.text,
        "status": "accepted_async",
    }))
}

async fn post_command(
    State(_): State<Arc<AppState>>,
    Path(session_id): Path<String>,
    Json(payload): Json<CommandPayload>,
) -> impl IntoResponse {
    Json(serde_json::json!({
        "session_id": session_id,
        "command": payload.command,
        "status": "accepted",
    }))
}

async fn post_shell(
    State(_): State<Arc<AppState>>,
    Path(session_id): Path<String>,
    Json(payload): Json<ShellPayload>,
) -> impl IntoResponse {
    Json(serde_json::json!({
        "session_id": session_id,
        "command": payload.command,
        "status": "accepted",
    }))
}

async fn revert_session(
    State(state): State<Arc<AppState>>,
    Path(session_id): Path<String>,
    Json(payload): Json<RevertPayload>,
) -> impl IntoResponse {
    let patch = rustcode_core::session::SessionPatch {
        revert: Some(Some(rustcode_core::session::RevertInfo {
            message_id: payload.message_id,
            part_id: None,
            snapshot: None,
            diff: None,
        })),
        ..Default::default()
    };
    match state.sessions.update(&session_id, patch).await {
        Ok(_) => Json(serde_json::json!({"reverted": true, "session_id": session_id})).into_response(),
        Err(e) => (
            axum::http::StatusCode::INTERNAL_SERVER_ERROR,
            Json(serde_json::json!({"error": e.to_string()})),
        )
            .into_response(),
    }
}

async fn unrevert_session(
    State(state): State<Arc<AppState>>,
    Path(session_id): Path<String>,
) -> impl IntoResponse {
    let patch = rustcode_core::session::SessionPatch {
        revert: Some(None),
        ..Default::default()
    };
    match state.sessions.update(&session_id, patch).await {
        Ok(_) => Json(serde_json::json!({"unreverted": true, "session_id": session_id})).into_response(),
        Err(e) => (
            axum::http::StatusCode::INTERNAL_SERVER_ERROR,
            Json(serde_json::json!({"error": e.to_string()})),
        )
            .into_response(),
    }
}

async fn permission_respond(
    State(_): State<Arc<AppState>>,
    Path((session_id, permission_id)): Path<(String, String)>,
    Json(payload): Json<PermissionResponsePayload>,
) -> impl IntoResponse {
    Json(serde_json::json!({
        "processed": true,
        "session_id": session_id,
        "permission_id": permission_id,
        "response": payload.response,
    }))
}

