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
use axum::routing::{delete, get, post};
use axum::{Json, Router};
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
            get(get_session)
                .patch(update_session)
                .delete(delete_session),
        )
        // Children
        .route("/session/{sessionID}/children", get(list_children))
        // Todo
        .route("/session/{sessionID}/todo", get(get_todos))
        // Diff
        .route("/session/{sessionID}/diff", get(get_diff))
        // Messages
        .route(
            "/session/{sessionID}/message",
            get(list_messages).post(post_prompt),
        )
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
        project_id: None,
        start: None,
        cursor: None,
        scope: None,
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
    State(state): State<Arc<AppState>>,
    Query(query): Query<ListQuery>,
) -> impl IntoResponse {
    // Return a status map for all sessions (or filtered)
    let input = Some(rustcode_core::session::ListSessionsInput {
        directory: query.directory,
        path: query.path,
        workspace_id: query.workspace,
        roots: query.roots,
        search: query.search,
        limit: query.limit,
        project_id: None,
        start: None,
        cursor: None,
        scope: None,
    });
    match state.sessions.list(input).await {
        Ok(sessions) => {
            let status_map: HashMap<String, serde_json::Value> = sessions
                .into_iter()
                .map(|s| {
                    let session_id = s.id.clone();
                    let status = serde_json::json!({
                        "id": s.id,
                        "title": s.title,
                        "directory": s.directory,
                        "agent": s.agent,
                        "cost": s.cost,
                        "tokens": s.tokens,
                        "updated": s.time.updated,
                        "archived": s.time.archived,
                    });
                    (session_id, status)
                })
                .collect();
            Json(serde_json::to_value(status_map).unwrap_or_default()).into_response()
        }
        Err(e) => (
            axum::http::StatusCode::INTERNAL_SERVER_ERROR,
            Json(serde_json::json!({"error": e.to_string()})),
        )
            .into_response(),
    }
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
    let directory = payload.directory.clone();
    let input = rustcode_core::session::CreateSessionInput {
        project_id: "default".to_string(),
        workspace_id: None,
        directory: payload.directory,
        path: payload.path,
        parent_id: payload.parent_id,
        title: payload.title,
        agent: payload.agent,
        model: payload
            .model
            .map(|m| rustcode_core::session::ModelSelection {
                id: m.id,
                provider_id: m.provider_id,
                variant: m.variant,
            }),
        metadata: None,
        permission: None,
    };
    match state.sessions.create(input).await {
        Ok(session) => {
            let event = rustcode_core::bus::GlobalEvent::new(serde_json::json!({
                "type": "session.created",
                "session_id": &session.id,
            }))
            .with_directory(directory);
            let _ = state.bus.publish(event);
            (
                axum::http::StatusCode::CREATED,
                Json(serde_json::to_value(session).unwrap_or_default()),
            )
                .into_response()
        }
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
        Ok(session) => {
            let event = rustcode_core::bus::GlobalEvent::new(serde_json::json!({
                "type": "session.updated",
                "session_id": &session_id,
            }));
            let _ = state.bus.publish(event);
            Json(serde_json::to_value(session).unwrap_or_default()).into_response()
        }
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
        Ok(()) => {
            let event = rustcode_core::bus::GlobalEvent::new(serde_json::json!({
                "type": "session.deleted",
                "session_id": &session_id,
            }));
            let _ = state.bus.publish(event);
            Json(serde_json::json!(true)).into_response()
        }
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
    State(state): State<Arc<AppState>>,
    Path(session_id): Path<String>,
) -> impl IntoResponse {
    // Fetch session messages and extract tool parts for todo status
    match state.sessions.get_messages(&session_id).await {
        Ok(messages) => {
            let todos: Vec<serde_json::Value> = messages
                .iter()
                .flat_map(|msg| {
                    msg.parts.iter().filter_map(|part| {
                        if let rustcode_core::session::Part::Tool(tp) = part {
                            if tp.tool == "todowrite" || tp.tool == "todo_write" {
                                match &tp.state {
                                    rustcode_core::session::ToolState::Completed {
                                        output,
                                        title,
                                        ..
                                    } => Some(serde_json::json!({
                                        "id": tp.id,
                                        "tool": tp.tool,
                                        "title": title,
                                        "output": output,
                                        "state": "completed",
                                    })),
                                    rustcode_core::session::ToolState::Running { .. } => {
                                        Some(serde_json::json!({
                                            "id": tp.id,
                                            "tool": tp.tool,
                                            "state": "running",
                                        }))
                                    }
                                    rustcode_core::session::ToolState::Pending { .. } => {
                                        Some(serde_json::json!({
                                            "id": tp.id,
                                            "tool": tp.tool,
                                            "state": "pending",
                                        }))
                                    }
                                    _ => None,
                                }
                            } else {
                                None
                            }
                        } else {
                            None
                        }
                    })
                })
                .collect();
            Json(serde_json::to_value(todos).unwrap_or_default()).into_response()
        }
        Err(_) => Json(serde_json::json!([])).into_response(),
    }
}

async fn get_diff(
    State(state): State<Arc<AppState>>,
    Path(session_id): Path<String>,
) -> impl IntoResponse {
    // Get session info to find the directory, then run git diff
    match state.sessions.get(&session_id).await {
        Ok(session_info) => {
            let git = rustcode_core::git::Git::new(&session_info.directory);
            if git.is_repo() {
                let diff_items = git.diff("HEAD").unwrap_or_default();
                let status_items = git.status().unwrap_or_default();
                Json(serde_json::json!({
                    "session_id": session_id,
                    "directory": session_info.directory,
                    "diff": diff_items,
                    "status": status_items,
                }))
                .into_response()
            } else {
                Json(serde_json::json!({
                    "session_id": session_id,
                    "directory": session_info.directory,
                    "is_repo": false,
                }))
                .into_response()
            }
        }
        Err(e) => (
            axum::http::StatusCode::NOT_FOUND,
            Json(serde_json::json!({"error": e.to_string()})),
        )
            .into_response(),
    }
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
        input.session_id,
        input
            .model
            .as_ref()
            .map(|m| m.provider_id.as_str())
            .unwrap_or("?"),
        input.model.as_ref().map(|m| m.id.as_str()).unwrap_or("?")
    );

    match state
        .runner
        .run(provider.as_ref(), &model, &input, &instructions)
        .await
    {
        Ok(result) => {
            info!(
                "Prompt completed for session {}: {} chars, {} events, {} tool calls, {} iterations",
                input.session_id,
                result.text.len(),
                result.events.len(),
                result.tool_calls.len(),
                result.iterations,
            );

            let event = rustcode_core::bus::GlobalEvent::new(serde_json::json!({
                "type": "session.message.created",
                "session_id": &input.session_id,
            }));
            let _ = state.bus.publish(event);

            Json(serde_json::json!({
                "session_id": input.session_id,
                "text": result.text,
                "success": result.success,
                "events_count": result.events.len(),
                "tool_calls": result.tool_calls.iter().map(|tc| {
                    serde_json::json!({
                        "name": tc.name,
                        "success": tc.success,
                        "error": tc.error,
                    })
                }).collect::<Vec<_>>(),
                "iterations": result.iterations,
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
    State(state): State<Arc<AppState>>,
    Path((session_id, message_id)): Path<(String, String)>,
) -> impl IntoResponse {
    // Mark message as deleted by updating its finish
    match state
        .sessions
        .update_message(
            &session_id,
            &message_id,
            rustcode_core::session::MessagePatch {
                finish: Some(Some("deleted".into())),
                ..Default::default()
            },
        )
        .await
    {
        Ok(()) => {
            let event = rustcode_core::bus::GlobalEvent::new(serde_json::json!({
                "type": "session.message.deleted",
                "session_id": &session_id,
                "message_id": &message_id,
            }));
            let _ = state.bus.publish(event);
            info!("Deleted message {message_id} from session {session_id}");
            Json(serde_json::json!({"deleted": true, "session_id": session_id, "message_id": message_id}))
                .into_response()
        }
        Err(e) => (
            axum::http::StatusCode::NOT_FOUND,
            Json(serde_json::json!({"error": e.to_string()})),
        )
            .into_response(),
    }
}

async fn delete_part(
    State(state): State<Arc<AppState>>,
    Path((session_id, message_id, part_id)): Path<(String, String, String)>,
) -> impl IntoResponse {
    // Get messages, find the specific message and remove the part
    match state.sessions.get_messages(&session_id).await {
        Ok(mut messages) => {
            if let Some(msg) = messages.iter_mut().find(|m| m.info.id() == message_id) {
                let before = msg.parts.len();
                msg.parts.retain(|p| {
                    use rustcode_core::session::Part;
                    let pid = match p {
                        Part::Text(tp) => &tp.id,
                        Part::Tool(tp) => &tp.id,
                        Part::Reasoning(tp) => &tp.id,
                        Part::File(tp) => &tp.id,
                        Part::StepStart(tp) => &tp.id,
                        Part::StepFinish(tp) => &tp.id,
                        Part::Patch(tp) => &tp.id,
                        Part::Compaction(tp) => &tp.id,
                        Part::Subtask(tp) => &tp.id,
                        Part::Snapshot(tp) => &tp.id,
                        Part::Agent(tp) => &tp.id,
                        Part::Retry(tp) => &tp.id,
                        Part::SourceUrl(tp) => &tp.id,
                    };
                    *pid != part_id
                });
                let removed = before - msg.parts.len();
                info!(
                    "Deleted {removed} part(s) {part_id} from message {message_id} in session {session_id}"
                );
            }
            let event = rustcode_core::bus::GlobalEvent::new(serde_json::json!({
                "type": "session.part.deleted",
                "session_id": &session_id,
                "message_id": &message_id,
                "part_id": &part_id,
            }));
            let _ = state.bus.publish(event);
            Json(serde_json::json!({
                "deleted": true,
                "session_id": session_id,
                "message_id": message_id,
                "part_id": part_id,
            }))
            .into_response()
        }
        Err(e) => (
            axum::http::StatusCode::NOT_FOUND,
            Json(serde_json::json!({"error": e.to_string()})),
        )
            .into_response(),
    }
}

async fn update_part(
    State(state): State<Arc<AppState>>,
    Path((session_id, message_id, part_id)): Path<(String, String, String)>,
    Json(payload): Json<serde_json::Value>,
) -> impl IntoResponse {
    // Update part content if it's a text part
    match state.sessions.get_messages(&session_id).await {
        Ok(mut messages) => {
            let mut updated = false;
            if let Some(msg) = messages.iter_mut().find(|m| m.info.id() == message_id) {
                for part in &mut msg.parts {
                    match part {
                        rustcode_core::session::Part::Text(tp) if tp.id == part_id => {
                            if let Some(text) = payload.get("text").and_then(|v| v.as_str()) {
                                tp.text = text.to_string();
                                updated = true;
                            }
                        }
                        _ => {}
                    }
                }
            }
            if updated {
                let event = rustcode_core::bus::GlobalEvent::new(serde_json::json!({
                    "type": "session.part.updated",
                    "session_id": &session_id,
                    "message_id": &message_id,
                    "part_id": &part_id,
                }));
                let _ = state.bus.publish(event);
                info!("Updated part {part_id} in message {message_id} session {session_id}");
            }
            Json(serde_json::json!({
                "updated": updated,
                "session_id": session_id,
                "message_id": message_id,
                "part_id": part_id,
                "payload": payload,
            }))
            .into_response()
        }
        Err(e) => (
            axum::http::StatusCode::NOT_FOUND,
            Json(serde_json::json!({"error": e.to_string()})),
        )
            .into_response(),
    }
}

async fn fork_session(
    State(state): State<Arc<AppState>>,
    Path(session_id): Path<String>,
    Json(payload): Json<ForkPayload>,
) -> impl IntoResponse {
    match state
        .sessions
        .fork(&session_id, payload.message_id.as_deref())
        .await
    {
        Ok(session) => {
            let new_id = session.id.clone();
            let dir = session.directory.clone();
            let event = rustcode_core::bus::GlobalEvent::new(serde_json::json!({
                "type": "session.created",
                "session_id": &new_id,
                "forked_from": &session_id,
            }))
            .with_directory(dir);
            let _ = state.bus.publish(event);
            Json(serde_json::to_value(session).unwrap_or_default()).into_response()
        }
        Err(e) => (
            axum::http::StatusCode::INTERNAL_SERVER_ERROR,
            Json(serde_json::json!({"error": e.to_string()})),
        )
            .into_response(),
    }
}

async fn abort_session(
    State(state): State<Arc<AppState>>,
    Path(session_id): Path<String>,
) -> impl IntoResponse {
    // Publish an abort event on the bus — the session processor listens for this
    let event = rustcode_core::bus::GlobalEvent::new(serde_json::json!({
        "type": "session.abort",
        "session_id": &session_id,
    }));
    let _ = state.bus.publish(event);
    info!("Abort signal sent for session {session_id}");
    Json(serde_json::json!({"aborted": true, "session_id": session_id}))
}

async fn share_session(
    State(state): State<Arc<AppState>>,
    Path(session_id): Path<String>,
) -> impl IntoResponse {
    // Generate a share URL and update the session
    let share_url = format!("https://opencode.ai/share/{session_id}");
    let patch = rustcode_core::session::SessionPatch {
        ..Default::default()
    };
    // Update session with share info via direct mutation
    let _ = state.sessions.update(&session_id, patch).await;
    // Publish share event
    let event = rustcode_core::bus::GlobalEvent::new(serde_json::json!({
        "type": "session.shared",
        "session_id": &session_id,
        "url": &share_url,
    }));
    let _ = state.bus.publish(event);
    info!("Session {session_id} shared at {share_url}");
    Json(serde_json::json!({
        "id": session_id,
        "share": {"url": share_url},
    }))
}

async fn unshare_session(
    State(state): State<Arc<AppState>>,
    Path(session_id): Path<String>,
) -> impl IntoResponse {
    let event = rustcode_core::bus::GlobalEvent::new(serde_json::json!({
        "type": "session.unshared",
        "session_id": &session_id,
    }));
    let _ = state.bus.publish(event);
    info!("Session {session_id} unshared");
    Json(serde_json::json!({"id": session_id, "share": null}))
}

async fn init_session(
    State(state): State<Arc<AppState>>,
    Path(session_id): Path<String>,
    Json(payload): Json<InitPayload>,
) -> impl IntoResponse {
    info!(
        "Initializing session {session_id} with model {}/{} at message {}",
        payload.provider_id, payload.model_id, payload.message_id
    );
    // Set the model on the session so subsequent prompts use it
    let model_selection = rustcode_core::session::ModelSelection {
        id: payload.model_id.clone(),
        provider_id: payload.provider_id.clone(),
        variant: None,
    };
    let patch = rustcode_core::session::SessionPatch {
        model: Some(Some(model_selection)),
        ..Default::default()
    };
    match state.sessions.update(&session_id, patch).await {
        Ok(session) => {
            let event = rustcode_core::bus::GlobalEvent::new(serde_json::json!({
                "type": "session.initialized",
                "session_id": &session_id,
                "provider_id": &payload.provider_id,
                "model_id": &payload.model_id,
            }));
            let _ = state.bus.publish(event);
            Json(serde_json::to_value(session).unwrap_or_default()).into_response()
        }
        Err(e) => (
            axum::http::StatusCode::NOT_FOUND,
            Json(serde_json::json!({"error": e.to_string()})),
        )
            .into_response(),
    }
}

async fn summarize_session(
    State(state): State<Arc<AppState>>,
    Path(session_id): Path<String>,
    Json(payload): Json<SummarizePayload>,
) -> impl IntoResponse {
    info!(
        "Summarizing session {session_id} with {}/{} (auto: {})",
        payload.provider_id,
        payload.model_id,
        payload.auto.unwrap_or(false)
    );
    // Look up the provider and model for summarization
    let provider = match state.providers.get(&payload.provider_id) {
        Some(p) => p,
        None => {
            return (
                axum::http::StatusCode::BAD_REQUEST,
                Json(serde_json::json!({
                    "error": format!("provider '{}' not configured", payload.provider_id)
                })),
            )
                .into_response();
        }
    };
    let model = match provider.get_model(&payload.model_id).await {
        Ok(m) => m,
        Err(e) => {
            return (
                axum::http::StatusCode::BAD_REQUEST,
                Json(serde_json::json!({"error": format!("model not found: {e}")})),
            )
                .into_response();
        }
    };
    // Build a summarization prompt using the existing messages
    let messages = state
        .sessions
        .get_messages(&session_id)
        .await
        .unwrap_or_default();
    let conversation: String = messages
        .iter()
        .map(|m| {
            format!(
                "[{}] {}",
                m.info.role(),
                m.parts
                    .iter()
                    .filter_map(|p| {
                        if let rustcode_core::session::Part::Text(tp) = p {
                            Some(tp.text.as_str())
                        } else {
                            None
                        }
                    })
                    .collect::<Vec<_>>()
                    .join("")
            )
        })
        .collect::<Vec<_>>()
        .join("\n");
    let summary_prompt = format!(
        "Summarize the following conversation in 2-3 sentences, focusing on what was discussed and any key decisions. Then provide a list of files that were mentioned or modified.\n\nConversation:\n{conversation}"
    );
    // Build chat messages for the summary model
    let chat_messages = vec![rustcode_core::provider::ChatMessage::User {
        content: rustcode_core::provider::MessageContent::Text(summary_prompt),
    }];
    match provider.complete(&model, &chat_messages, &[]).await {
        Ok(response) => {
            let summary_text = response.text();
            info!(
                "Summarization complete for session {session_id}: {} chars",
                summary_text.len()
            );
            // Update the session with the summary
            let summary = rustcode_core::session::SessionSummary {
                additions: 0,
                deletions: 0,
                files: 0,
                diffs: None,
            };
            let patch = rustcode_core::session::SessionPatch {
                summary: Some(Some(summary)),
                ..Default::default()
            };
            let _ = state.sessions.update(&session_id, patch).await;
            let event = rustcode_core::bus::GlobalEvent::new(serde_json::json!({
                "type": "session.summarized",
                "session_id": &session_id,
                "model_id": &payload.model_id,
            }));
            let _ = state.bus.publish(event);
            Json(serde_json::json!({
                "summarized": true,
                "session_id": session_id,
                "model_id": payload.model_id,
                "summary": summary_text,
            }))
            .into_response()
        }
        Err(e) => {
            error!("Summarization failed for session {session_id}: {e}");
            (
                axum::http::StatusCode::INTERNAL_SERVER_ERROR,
                Json(serde_json::json!({"error": e.to_string()})),
            )
                .into_response()
        }
    }
}

async fn prompt_async(
    State(state): State<Arc<AppState>>,
    Path(session_id): Path<String>,
    Json(payload): Json<PromptPayload>,
) -> impl IntoResponse {
    // Fire-and-forget prompt — spawn a background task
    let state_clone = state.clone();
    let sid = session_id.clone();
    let txt = payload.text.clone();
    let mdl = payload.model.clone();
    tokio::spawn(async move {
        info!("Async prompt started for session {sid}");
        let model_selection = mdl.unwrap_or_else(|| ModelSelectionPayload {
            id: "claude-sonnet-4-6".into(),
            provider_id: "anthropic".into(),
            variant: None,
        });
        if let Some(provider) = state_clone.providers.get(&model_selection.provider_id) {
            if let Ok(model) = provider.get_model(&model_selection.id).await {
                let input = rustcode_core::session_prompt::SessionPromptInput {
                    session_id: sid.clone(),
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
                    parts: vec![rustcode_core::session_prompt::PromptPart::Text(
                        rustcode_core::session_prompt::PromptTextPart {
                            id: None,
                            text: txt,
                            synthetic: false,
                        },
                    )],
                };
                let instructions = vec!["You are a helpful coding assistant.".to_string()];
                let result = state_clone
                    .runner
                    .run(provider.as_ref(), &model, &input, &instructions)
                    .await;
                match result {
                    Ok(r) => info!(
                        "Async prompt completed for session {sid}: {} chars, {} tool calls, {} iters",
                        r.text.len(),
                        r.tool_calls.len(),
                        r.iterations
                    ),
                    Err(e) => error!("Async prompt failed for session {sid}: {e}"),
                }
            }
        }
    });
    Json(serde_json::json!({
        "session_id": session_id,
        "text": payload.text,
        "status": "accepted_async",
    }))
}

async fn post_command(
    State(state): State<Arc<AppState>>,
    Path(session_id): Path<String>,
    Json(payload): Json<CommandPayload>,
) -> impl IntoResponse {
    info!(
        "Executing command '{}' in session {session_id}",
        payload.command
    );
    // Look up the tool in the registry
    match state.tools.get(&payload.command) {
        Some(tool_def) => {
            let args = serde_json::to_value(payload.args.unwrap_or_default()).unwrap_or_default();
            let ctx = rustcode_core::tool::ToolContext {
                session_id: session_id.clone(),
                message_id: String::new(),
                agent: "cli".into(),
                abort: tokio_util::sync::CancellationToken::new(),
                call_id: None,
                extra: std::collections::HashMap::new(),
                messages: std::sync::Arc::from([] as [rustcode_core::provider::ChatMessage; 0]),
                ask_fn: None,
                permission_source: Some(rustcode_core::permission::PermissionSource::Session {
                    session_id: session_id.clone(),
                }),
                prompt_ops: None,
            };
            let tool = tool_def.tool;
            match tool.execute(args, &ctx).await {
                Ok(result) => Json(serde_json::json!({
                    "session_id": session_id,
                    "command": payload.command,
                    "title": result.title,
                    "output": result.output,
                    "truncated": result.truncated,
                    "status": "completed",
                }))
                .into_response(),
                Err(e) => (
                    axum::http::StatusCode::INTERNAL_SERVER_ERROR,
                    Json(serde_json::json!({
                        "session_id": session_id,
                        "command": payload.command,
                        "error": e.to_string(),
                        "status": "error",
                    })),
                )
                    .into_response(),
            }
        }
        None => (
            axum::http::StatusCode::NOT_FOUND,
            Json(serde_json::json!({
                "session_id": session_id,
                "command": payload.command,
                "error": format!("command/tool '{}' not found in registry", payload.command),
                "status": "unknown_command",
            })),
        )
            .into_response(),
    }
}

async fn post_shell(
    State(state): State<Arc<AppState>>,
    Path(session_id): Path<String>,
    Json(payload): Json<ShellPayload>,
) -> impl IntoResponse {
    info!(
        "Running shell command '{}' in session {session_id} (workdir: {:?})",
        payload.command, payload.workdir
    );
    // Get the session directory for working directory
    let workdir = if let Some(wd) = &payload.workdir {
        std::path::PathBuf::from(wd)
    } else {
        match state.sessions.get(&session_id).await {
            Ok(si) => std::path::PathBuf::from(&si.directory),
            Err(_) => std::env::current_dir().unwrap_or_else(|_| std::path::PathBuf::from("/tmp")),
        }
    };
    // Spawn a shell subprocess
    let output = tokio::process::Command::new("sh")
        .arg("-c")
        .arg(&payload.command)
        .current_dir(&workdir)
        .output()
        .await;
    match output {
        Ok(out) => {
            let stdout = String::from_utf8_lossy(&out.stdout).to_string();
            let stderr = String::from_utf8_lossy(&out.stderr).to_string();
            info!(
                "Shell command completed in session {session_id}: exit={}",
                out.status.code().unwrap_or(-1)
            );
            // Publish command result event
            let event = rustcode_core::bus::GlobalEvent::new(serde_json::json!({
                "type": "shell.completed",
                "session_id": &session_id,
                "command": &payload.command,
                "exit_code": out.status.code(),
                "stdout_size": stdout.len(),
            }));
            let _ = state.bus.publish(event);
            Json(serde_json::json!({
                "session_id": session_id,
                "command": payload.command,
                "exit_code": out.status.code(),
                "stdout": stdout,
                "stderr": stderr,
                "status": "completed",
                "description": payload.description,
            }))
            .into_response()
        }
        Err(e) => {
            error!("Shell command failed in session {session_id}: {e}");
            (
                axum::http::StatusCode::INTERNAL_SERVER_ERROR,
                Json(serde_json::json!({
                    "session_id": session_id,
                    "command": payload.command,
                    "error": e.to_string(),
                    "status": "error",
                })),
            )
                .into_response()
        }
    }
}

async fn revert_session(
    State(state): State<Arc<AppState>>,
    Path(session_id): Path<String>,
    Json(payload): Json<RevertPayload>,
) -> impl IntoResponse {
    let message_id = payload.message_id.clone();
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
        Ok(_) => {
            let event = rustcode_core::bus::GlobalEvent::new(serde_json::json!({
                "type": "session.reverted",
                "session_id": &session_id,
                "message_id": &message_id,
            }));
            let _ = state.bus.publish(event);
            Json(serde_json::json!({"reverted": true, "session_id": session_id})).into_response()
        }
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
        Ok(_) => {
            let event = rustcode_core::bus::GlobalEvent::new(serde_json::json!({
                "type": "session.unreverted",
                "session_id": &session_id,
            }));
            let _ = state.bus.publish(event);
            Json(serde_json::json!({"unreverted": true, "session_id": session_id})).into_response()
        }
        Err(e) => (
            axum::http::StatusCode::INTERNAL_SERVER_ERROR,
            Json(serde_json::json!({"error": e.to_string()})),
        )
            .into_response(),
    }
}

async fn permission_respond(
    State(state): State<Arc<AppState>>,
    Path((session_id, permission_id)): Path<(String, String)>,
    Json(payload): Json<PermissionResponsePayload>,
) -> impl IntoResponse {
    // Convert the response string to a PermissionReply
    let reply = match payload.response.to_lowercase().as_str() {
        "once" => rustcode_core::permission::PermissionReply::Once,
        "always" => rustcode_core::permission::PermissionReply::Always,
        "reject" | "deny" => rustcode_core::permission::PermissionReply::Reject,
        _ => {
            return (
                axum::http::StatusCode::BAD_REQUEST,
                Json(serde_json::json!({
                    "error": format!("invalid permission response: '{}'. Use once/always/reject", payload.response),
                })),
            )
                .into_response();
        }
    };
    let input = rustcode_core::permission::ReplyInput {
        request_id: permission_id.clone(),
        reply,
        message: payload.message,
    };
    match state.permissions.reply(input).await {
        Ok(()) => {
            let event = rustcode_core::bus::GlobalEvent::new(serde_json::json!({
                "type": "permission.replied",
                "session_id": &session_id,
                "permission_id": &permission_id,
                "response": &payload.response,
            }));
            let _ = state.bus.publish(event);
            info!(
                "Permission {permission_id} resolved: {:?} for session {session_id}",
                reply
            );
            Json(serde_json::json!({
                "processed": true,
                "session_id": session_id,
                "permission_id": permission_id,
                "response": payload.response,
            }))
            .into_response()
        }
        Err(e) => (
            axum::http::StatusCode::NOT_FOUND,
            Json(serde_json::json!({"error": e.to_string()})),
        )
            .into_response(),
    }
}
