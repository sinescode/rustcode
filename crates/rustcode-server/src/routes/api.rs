//! V2 API routes — `/api/*` endpoints mirroring `packages/server/src/`.
//!
//! Ported from: `packages/server/src/api.ts` and all groups in `packages/server/src/groups/`.
//!
//! Route paths (all under `/api`):
//! - `GET    /api/health`                  — health check
//! - `GET    /api/location`                — location info
//! - `GET    /api/agent`                   — list agents
//! - `GET    /api/session`                 — list sessions
//! - `POST   /api/session`                 — create session
//! - `GET    /api/session/:sessionID`      — get session
//! - `POST   /api/session/:sessionID/prompt` — send prompt
//! - `POST   /api/session/:sessionID/compact` — compact session
//! - `POST   /api/session/:sessionID/wait` — wait for session
//! - `GET    /api/session/:sessionID/context` — session context
//! - `GET    /api/session/:sessionID/message` — list messages
//! - `GET    /api/session/:sessionID/message/:messageID` — get message
//! - `GET    /api/model`                   — list models
//! - `GET    /api/provider`                — list providers
//! - `POST   /api/integration`             — create integration
//! - `POST   /api/credential`              — set credential
//! - `GET    /api/permission`              — list permissions
//! - `POST   /api/permission/:requestID/reply` — reply to permission
//! - `GET    /api/fs/**`                   — filesystem access
//! - `GET    /api/command`                 — list commands
//! - `GET    /api/skill`                   — list skills
//! - `GET    /api/event` (SSE)             — event stream
//! - `GET    /api/pty`                     — list PTYs
//! - `GET    /api/question`                — list questions
//! - `POST   /api/question/:requestID/reply` — reply to question
//! - `POST   /api/question/:requestID/reject` — reject question
//! - `GET    /api/reference`               — list references
//! - `POST   /api/project-copy/:projectID/generate-name` — generate copy name

use axum::extract::{Path, Query, State};
use axum::response::sse::Event as SseEvent;
use axum::response::{IntoResponse, Sse};
use axum::routing::{get, post};
use axum::{Json, Router};
use futures::stream::Stream;
use serde::Deserialize;
use std::convert::Infallible;
use std::sync::Arc;
use std::time::Duration;
use tokio_stream::wrappers::BroadcastStream;
use tokio_stream::StreamExt;
use tracing::info;

use crate::error::ServerError;
use crate::server::AppState;

// ── Query types ────────────────────────────────────────────────────────────

#[derive(Debug, Deserialize, Default)]
pub struct ApiListQuery {
    pub directory: Option<String>,
    pub workspace: Option<String>,
    pub limit: Option<usize>,
    pub start: Option<u64>,
    pub search: Option<String>,
}

#[derive(Debug, Deserialize)]
pub struct ApiPromptPayload {
    pub text: String,
    #[serde(default)]
    pub agent: Option<String>,
    #[serde(default)]
    pub model: Option<String>,
}

#[derive(Debug, Deserialize)]
pub struct ApiPermissionReplyPayload {
    pub reply: String,
    #[serde(default)]
    pub message: Option<String>,
}

#[derive(Debug, Deserialize)]
pub struct ApiQuestionReplyPayload {
    pub answers: Vec<Vec<String>>,
}

#[derive(Debug, Deserialize)]
pub struct ApiCredentialPayload {
    pub provider_id: String,
    pub key: String,
    #[serde(default)]
    pub base_url: Option<String>,
}

#[derive(Debug, Deserialize)]
pub struct ApiIntegrationPayload {
    pub id: String,
    pub config: serde_json::Value,
}

#[derive(Debug, Deserialize)]
pub struct ApiFsQuery {
    pub path: String,
    #[serde(default)]
    pub directory: Option<String>,
    #[serde(default)]
    pub workspace: Option<String>,
}

#[derive(Debug, Deserialize)]
pub struct ApiEventQuery {
    pub directory: Option<String>,
    pub workspace: Option<String>,
}

// ── Router ─────────────────────────────────────────────────────────────────

/// Create the v2 API routes router.
pub fn api_routes(state: Arc<AppState>) -> Router {
    Router::new()
        // Health
        .route("/api/health", get(api_health))
        // Location
        .route("/api/location", get(api_location))
        // Agent
        .route("/api/agent", get(api_agent))
        // Session
        .route("/api/session", get(api_list_sessions).post(api_create_session))
        .route("/api/session/{sessionID}", get(api_get_session))
        .route("/api/session/{sessionID}/prompt", post(api_session_prompt))
        .route("/api/session/{sessionID}/compact", post(api_session_compact))
        .route("/api/session/{sessionID}/wait", post(api_session_wait))
        .route("/api/session/{sessionID}/context", get(api_session_context))
        .route(
            "/api/session/{sessionID}/message",
            get(api_list_messages),
        )
        .route(
            "/api/session/{sessionID}/message/{messageID}",
            get(api_get_message),
        )
        // Model
        .route("/api/model", get(api_model))
        // Provider
        .route("/api/provider", get(api_provider))
        // Integration
        .route("/api/integration", post(api_integration))
        // Credential
        .route("/api/credential", post(api_credential))
        // Permission
        .route("/api/permission", get(api_permission))
        .route(
            "/api/permission/{requestID}/reply",
            post(api_permission_reply),
        )
        // Filesystem
        .route("/api/fs/{*path}", get(api_fs))
        // Command
        .route("/api/command", get(api_command))
        // Skill
        .route("/api/skill", get(api_skill))
        // Event (SSE)
        .route("/api/event", get(api_event))
        // PTY
        .route("/api/pty", get(api_pty))
        // Question
        .route("/api/question", get(api_question))
        .route(
            "/api/question/{requestID}/reply",
            post(api_question_reply),
        )
        .route(
            "/api/question/{requestID}/reject",
            post(api_question_reject),
        )
        // Reference
        .route("/api/reference", get(api_reference))
        // Project copy
        .route(
            "/api/project-copy/{projectID}/generate-name",
            post(api_project_copy_generate_name),
        )
        .with_state(state)
}

// ── Handlers ───────────────────────────────────────────────────────────────

/// `GET /api/health` — health check.
async fn api_health(State(state): State<Arc<AppState>>) -> impl IntoResponse {
    Json(serde_json::json!({
        "healthy": true,
        "version": state.version,
        "uptime_seconds": state.start_time.elapsed().as_secs(),
        "provider_count": state.providers.len(),
    }))
}

/// `GET /api/location` — location info (current directory, workspace, project).
async fn api_location() -> impl IntoResponse {
    let cwd = std::env::current_dir()
        .map(|p| p.to_string_lossy().to_string())
        .unwrap_or_default();
    Json(serde_json::json!({
        "directory": cwd,
        "workspace_id": null,
        "project_id": "default",
    }))
}

/// `GET /api/agent` — list agents.
async fn api_agent(State(state): State<Arc<AppState>>) -> impl IntoResponse {
    if let Some(ref agent_svc) = state.agent_service {
        let agents = agent_svc.list(None);
        let result: Vec<serde_json::Value> = agents
            .iter()
            .map(|a| {
                serde_json::json!({
                    "id": a.name,
                    "name": a.name,
                    "mode": format!("{:?}", a.mode).to_lowercase(),
                })
            })
            .collect();
        return Json(serde_json::to_value(result).unwrap_or_default()).into_response();
    }
    // Fallback built-in list
    Json(serde_json::json!([
        {"id": "build", "name": "Build", "mode": "primary"},
        {"id": "plan", "name": "Plan", "mode": "primary"},
    ]))
    .into_response()
}

/// `GET /api/session` — list sessions.
async fn api_list_sessions(
    State(state): State<Arc<AppState>>,
    Query(query): Query<ApiListQuery>,
) -> impl IntoResponse {
    let input = rustcode_core::session::ListSessionsInput {
        directory: query.directory,
        search: query.search,
        limit: query.limit,
        ..Default::default()
    };
    match state.sessions.list(Some(input)).await {
        Ok(sessions) => Json(serde_json::to_value(sessions).unwrap_or_default()).into_response(),
        Err(e) => ServerError::unknown(e.to_string()).into_response(),
    }
}

/// `POST /api/session` — create session.
async fn api_create_session(
    State(state): State<Arc<AppState>>,
    Json(payload): Json<serde_json::Value>,
) -> impl IntoResponse {
    let directory = payload
        .get("directory")
        .and_then(|v| v.as_str())
        .unwrap_or(".")
        .to_string();
    let input = rustcode_core::session::CreateSessionInput {
        project_id: "default".to_string(),
        directory,
        workspace_id: None,
        path: None,
        parent_id: None,
        title: None,
        agent: None,
        model: None,
        metadata: None,
        permission: None,
    };
    match state.sessions.create(input).await {
        Ok(session) => {
            (axum::http::StatusCode::CREATED, Json(serde_json::to_value(session).unwrap_or_default()))
                .into_response()
        }
        Err(e) => ServerError::unknown(e.to_string()).into_response(),
    }
}

/// `GET /api/session/:sessionID` — get session.
async fn api_get_session(
    State(state): State<Arc<AppState>>,
    Path(session_id): Path<String>,
) -> impl IntoResponse {
    match state.sessions.get(&session_id).await {
        Ok(session) => Json(serde_json::to_value(session).unwrap_or_default()).into_response(),
        Err(_) => ServerError::session_not_found(&session_id).into_response(),
    }
}

/// `POST /api/session/:sessionID/prompt` — send prompt.
async fn api_session_prompt(
    State(state): State<Arc<AppState>>,
    Path(session_id): Path<String>,
    Json(payload): Json<ApiPromptPayload>,
) -> impl IntoResponse {
    // Simple prompt — add text as a message
    let parts = vec![rustcode_core::session_prompt::PromptPart::Text(
        rustcode_core::session_prompt::PromptTextPart {
            id: None,
            text: payload.text,
            synthetic: false,
        },
    )];
    let input = rustcode_core::session_prompt::SessionPromptInput {
        session_id,
        message_id: None,
        model: None,
        agent: payload.agent.or(Some("build".into())),
        no_reply: false,
        tools: None,
        format: None,
        system: None,
        variant: None,
        parts,
    };
    let instructions = vec![
        "You are a helpful coding assistant.".to_string(),
    ];
    // Find any provider to handle the prompt
    if let Some((_pid, provider)) = state.providers.iter().next() {
        if let Ok(model) = provider.list_models().await.and_then(|models| {
            models.into_iter().next().ok_or_else(|| rustcode_core::error::Error::Internal("no models".to_string()))
        }) {
            match state.runner.run(provider.as_ref(), &model, &input, &instructions).await {
                Ok(result) => {
                    return Json(serde_json::json!({
                        "session_id": input.session_id,
                        "text": result.text,
                        "success": result.success,
                    }))
                    .into_response();
                }
                Err(e) => {
                    return ServerError::unknown(e.to_string()).into_response();
                }
            }
        }
    }
    // If no provider or model, just acknowledge
    Json(serde_json::json!({
        "session_id": input.session_id,
        "status": "accepted",
    }))
    .into_response()
}

/// `POST /api/session/:sessionID/compact` — compact session.
async fn api_session_compact(
    State(_state): State<Arc<AppState>>,
    Path(session_id): Path<String>,
) -> impl IntoResponse {
    info!("API: compact session {session_id} (not yet implemented)");
    ServerError::NotImplemented {
        message: format!("session compaction for {session_id} is not yet implemented"),
    }.into_response()
}

/// `POST /api/session/:sessionID/wait` — wait for session.
async fn api_session_wait(
    State(_state): State<Arc<AppState>>,
    Path(session_id): Path<String>,
) -> impl IntoResponse {
    info!("API: wait for session {session_id} (not yet implemented)");
    ServerError::NotImplemented {
        message: format!("session wait for {session_id} is not yet implemented"),
    }.into_response()
}

/// `GET /api/session/:sessionID/context` — session context.
async fn api_session_context(
    State(state): State<Arc<AppState>>,
    Path(session_id): Path<String>,
) -> impl IntoResponse {
    match state.sessions.get(&session_id).await {
        Ok(session) => {
            let context = serde_json::json!({
                "session_id": session.id,
                "directory": session.directory,
                "project_id": session.project_id,
                "agent": session.agent,
                "model": session.model,
            });
            Json(context).into_response()
        }
        Err(_) => ServerError::session_not_found(&session_id).into_response(),
    }
}

/// `GET /api/session/:sessionID/message` — list messages.
async fn api_list_messages(
    State(state): State<Arc<AppState>>,
    Path(session_id): Path<String>,
) -> impl IntoResponse {
    match state.sessions.get_messages(&session_id).await {
        Ok(messages) => Json(serde_json::to_value(messages).unwrap_or_default()).into_response(),
        Err(_) => Json(serde_json::json!([])).into_response(),
    }
}

/// `GET /api/session/:sessionID/message/:messageID` — get message.
async fn api_get_message(
    State(state): State<Arc<AppState>>,
    Path((session_id, message_id)): Path<(String, String)>,
) -> impl IntoResponse {
    match state.sessions.get_messages(&session_id).await {
        Ok(messages) => {
            if let Some(msg) = messages.into_iter().find(|m| m.info.id() == message_id) {
                Json(serde_json::to_value(msg).unwrap_or_default()).into_response()
            } else {
                ServerError::message_not_found(format!("Message '{}' not found in session '{}'", message_id, session_id)).into_response()
            }
        }
        Err(_) => ServerError::session_not_found(&session_id).into_response(),
    }
}

/// `GET /api/model` — list models.
async fn api_model(State(state): State<Arc<AppState>>) -> impl IntoResponse {
    let mut models: Vec<serde_json::Value> = Vec::new();
    for (provider_id, provider) in &state.providers {
        if let Ok(provider_models) = provider.list_models().await {
            for model in provider_models {
                models.push(serde_json::json!({
                    "id": model.id,
                    "provider_id": provider_id,
                    "name": model.name,
                }));
            }
        }
    }
    Json(serde_json::to_value(models).unwrap_or_default()).into_response()
}

/// `GET /api/provider` — list providers.
async fn api_provider(State(state): State<Arc<AppState>>) -> impl IntoResponse {
    let providers: Vec<serde_json::Value> = state
        .providers
        .iter()
        .map(|(id, provider)| {
            serde_json::json!({
                "id": id,
                "name": provider.provider_id(),
            })
        })
        .collect();
    Json(serde_json::to_value(providers).unwrap_or_default()).into_response()
}

/// `POST /api/integration` — create/register an integration.
async fn api_integration(
    State(state): State<Arc<AppState>>,
    Json(payload): Json<ApiIntegrationPayload>,
) -> impl IntoResponse {
    info!("API: integration '{}' with config", payload.id);
    let event = rustcode_core::bus::GlobalEvent::new(serde_json::json!({
        "type": "integration.created",
        "id": payload.id,
    }));
    let _ = state.bus.publish(event);
    Json(serde_json::json!({
        "id": payload.id,
        "created": true,
    }))
    .into_response()
}

/// `POST /api/credential` — set credential for a provider.
async fn api_credential(
    State(_state): State<Arc<AppState>>,
    Json(payload): Json<ApiCredentialPayload>,
) -> impl IntoResponse {
    info!("API: credential set for provider '{}'", payload.provider_id);
    let mut cred = serde_json::json!({
        "type": "api_key",
        "key": payload.key,
    });
    if let Some(ref base_url) = payload.base_url {
        cred["base_url"] = serde_json::Value::String(base_url.clone());
    }
    match rustcode_core::config::Config::save_auth(&payload.provider_id, &cred) {
        Ok(()) => Json(serde_json::json!({
            "provider_id": payload.provider_id,
            "set": true,
        }))
        .into_response(),
        Err(e) => ServerError::unknown(e.to_string()).into_response(),
    }
}

/// `GET /api/permission` — list pending permissions.
async fn api_permission(State(state): State<Arc<AppState>>) -> impl IntoResponse {
    let pending = state.permissions.list();
    Json(serde_json::to_value(pending).unwrap_or_default()).into_response()
}

/// `POST /api/permission/:requestID/reply` — reply to permission request.
async fn api_permission_reply(
    State(state): State<Arc<AppState>>,
    Path(request_id): Path<String>,
    Json(payload): Json<ApiPermissionReplyPayload>,
) -> impl IntoResponse {
    let reply = match payload.reply.to_lowercase().as_str() {
        "once" => rustcode_core::permission::PermissionReply::Once,
        "always" => rustcode_core::permission::PermissionReply::Always,
        "reject" | "deny" => rustcode_core::permission::PermissionReply::Reject,
        _ => {
            return ServerError::invalid_request(format!(
                "invalid permission reply: '{}'. Use once/always/reject",
                payload.reply
            ))
            .into_response();
        }
    };
    let input = rustcode_core::permission::ReplyInput {
        request_id: request_id.clone(),
        reply,
        message: payload.message,
    };
    match state.permissions.reply(input).await {
        Ok(()) => Json(serde_json::json!({
            "processed": true,
            "request_id": request_id,
        }))
        .into_response(),
        Err(e) => ServerError::unknown(e.to_string()).into_response(),
    }
}

/// `GET /api/fs/**` — filesystem access.
async fn api_fs(
    State(_state): State<Arc<AppState>>,
    Path(path): Path<String>,
    Query(query): Query<ApiFsQuery>,
) -> impl IntoResponse {
    let base_dir = query.directory.unwrap_or_else(|| {
        std::env::current_dir()
            .map(|p| p.to_string_lossy().to_string())
            .unwrap_or_default()
    });
    let full_path = std::path::PathBuf::from(&base_dir).join(&path);

    if !full_path.exists() {
        return ServerError::invalid_request(format!("path '{}' not found", full_path.display()))
            .into_response();
    }

    if full_path.is_dir() {
        let mut entries = Vec::new();
        if let Ok(dir_entries) = std::fs::read_dir(&full_path) {
            for entry in dir_entries.flatten() {
                let name = entry.file_name().to_string_lossy().to_string();
                let is_dir = entry.path().is_dir();
                let size = entry.path().metadata().ok().map(|m| m.len()).unwrap_or(0);
                entries.push(serde_json::json!({
                    "name": name,
                    "is_dir": is_dir,
                    "size": size,
                }));
            }
        }
        Json(serde_json::json!({
            "path": path,
            "directory": base_dir,
            "type": "directory",
            "entries": entries,
        }))
        .into_response()
    } else {
        match std::fs::read_to_string(&full_path) {
            Ok(content) => Json(serde_json::json!({
                "path": path,
                "directory": base_dir,
                "type": "file",
                "content": content,
                "size": content.len(),
            }))
            .into_response(),
            Err(e) => ServerError::invalid_request(format!("cannot read file: {e}")).into_response(),
        }
    }
}

/// `GET /api/command` — list commands.
async fn api_command(State(state): State<Arc<AppState>>) -> impl IntoResponse {
    let tool_ids = state.tools.ids();
    let mut commands: Vec<serde_json::Value> = tool_ids
        .into_iter()
        .map(|id| serde_json::json!({ "id": id, "source": "tool" }))
        .collect();
    // Add config commands
    for cmd in state.command_data.list() {
        let already = commands.iter().any(|c| c["id"].as_str() == Some(cmd.name.as_str()));
        if !already {
            commands.push(serde_json::json!({
                "id": cmd.name,
                "source": "command",
                "description": cmd.description,
            }));
        }
    }
    Json(serde_json::to_value(commands).unwrap_or_default()).into_response()
}

/// `GET /api/skill` — list skills.
async fn api_skill(State(_state): State<Arc<AppState>>) -> impl IntoResponse {
    let cwd = std::env::current_dir().unwrap_or_else(|_| std::path::PathBuf::from("."));
    let skills_dir = cwd.join(".opencode").join("skills");
    let mut skills = Vec::new();
    if skills_dir.exists() {
        if let Ok(entries) = std::fs::read_dir(&skills_dir) {
            for entry in entries.flatten() {
                let path = entry.path();
                if path.extension().is_some_and(|ext| ext == "md") {
                    if let Some(name) = path.file_stem().and_then(|n| n.to_str()) {
                        skills.push(serde_json::json!({
                            "name": name,
                            "path": path.to_string_lossy(),
                        }));
                    }
                }
            }
        }
    }
    Json(serde_json::to_value(skills).unwrap_or_default()).into_response()
}

/// `GET /api/event` — SSE event stream (v2). Mirrors `/event` but under `/api/event`.
async fn api_event(
    State(state): State<Arc<AppState>>,
    Query(query): Query<ApiEventQuery>,
) -> Sse<impl Stream<Item = Result<SseEvent, Infallible>>> {
    let directory = query.directory;
    let mut bus_rx = state.bus.subscribe();

    // Bridge bus subscription to broadcast receiver
    let (tx, rx) = tokio::sync::broadcast::channel(256);
    tokio::spawn(async move {
        while let Some(event) = bus_rx.recv().await {
            if tx.send(event).is_err() {
                break;
            }
        }
    });

    let bus_stream = BroadcastStream::new(rx);

    let heartbeat = tokio_stream::wrappers::IntervalStream::new(
        tokio::time::interval(Duration::from_secs(10)),
    );

    let connected = tokio_stream::once(Ok(SseEvent::default()
        .event("server.connected")
        .data(r#"{}"#)));

    let events = bus_stream.filter_map(move |result| match result {
        Ok(event) => {
            if let Some(ref d) = directory {
                if event.directory.as_deref() != Some(d.as_str()) {
                    return None;
                }
            }
            let event_type = event
                .payload
                .get("type")
                .and_then(|t| t.as_str())
                .unwrap_or("message");
            let data = serde_json::to_string(&event.payload).unwrap_or_default();
            Some(Ok(SseEvent::default().event(event_type).data(data)))
        }
        Err(_) => None,
    });

    let heartbeats =
        heartbeat.map(|_| Ok(SseEvent::default().event("server.heartbeat").data(r#"{}"#)));

    let stream = connected.chain(events).merge(heartbeats);

    Sse::new(stream).keep_alive(
        axum::response::sse::KeepAlive::new()
            .interval(Duration::from_secs(15))
            .text("ping"),
    )
}

/// `GET /api/pty` — list PTYs.
async fn api_pty() -> impl IntoResponse {
    Json(serde_json::json!({ "ptys": [] }))
}

/// `GET /api/question` — list questions.
async fn api_question(State(state): State<Arc<AppState>>) -> impl IntoResponse {
    let pending = state.questions.list().await;
    Json(serde_json::to_value(pending).unwrap_or_default()).into_response()
}

/// `POST /api/question/:requestID/reply` — reply to question.
async fn api_question_reply(
    State(state): State<Arc<AppState>>,
    Path(request_id): Path<String>,
    Json(payload): Json<ApiQuestionReplyPayload>,
) -> impl IntoResponse {
    let answers: Vec<rustcode_core::question::QuestionAnswer> = payload
        .answers
        .into_iter()
        .map(rustcode_core::question::QuestionAnswer::new)
        .collect();
    let request_id_obj = rustcode_core::question::QuestionId::new_unchecked(&request_id);
    match state.questions.reply(&request_id_obj, answers).await {
        Ok(()) => Json(serde_json::json!({
            "replied": true,
            "request_id": request_id,
        }))
        .into_response(),
        Err(e) => ServerError::unknown(e.to_string()).into_response(),
    }
}

/// `POST /api/question/:requestID/reject` — reject question.
async fn api_question_reject(
    State(state): State<Arc<AppState>>,
    Path(request_id): Path<String>,
) -> impl IntoResponse {
    let request_id_obj = rustcode_core::question::QuestionId::new_unchecked(&request_id);
    match state.questions.reject(&request_id_obj).await {
        Ok(()) => Json(serde_json::json!({
            "rejected": true,
            "request_id": request_id,
        }))
        .into_response(),
        Err(e) => ServerError::unknown(e.to_string()).into_response(),
    }
}

/// `GET /api/reference` — list references.
async fn api_reference(State(state): State<Arc<AppState>>) -> impl IntoResponse {
    let references = state.reference_service.lookup_all();
    Json(serde_json::to_value(references).unwrap_or_default()).into_response()
}

/// `POST /api/project-copy/:projectID/generate-name` — generate a name for a project copy.
async fn api_project_copy_generate_name(
    State(_state): State<Arc<AppState>>,
    Path(project_id): Path<String>,
    Json(payload): Json<serde_json::Value>,
) -> impl IntoResponse {
    let context = payload.get("context").and_then(|v| v.as_str());
    let name = context
        .map(|ctx| {
            ctx.split_whitespace()
                .take(3)
                .collect::<Vec<_>>()
                .join("-")
                .to_lowercase()
        })
        .unwrap_or_else(|| "project-copy".to_string());
    Json(serde_json::json!({
        "name": name,
        "project_id": project_id,
    }))
    .into_response()
}
