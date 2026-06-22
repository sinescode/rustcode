//! TUI routes — prompt append, dialogs, command execution, toast, publish, control.
//!
//! Ported from: `packages/blazecode/src/server/routes/instance/httpapi/groups/tui.ts`

use axum::extract::State;
use axum::response::IntoResponse;
use axum::routing::{get, post};
use axum::{Json, Router};
use serde::Deserialize;
use std::sync::Arc;
use tracing::{info, warn};

use crate::server::AppState;

#[derive(Debug, Deserialize)]
pub struct AppendPromptPayload {
    pub text: String,
}

#[derive(Debug, Deserialize)]
pub struct ExecuteCommandPayload {
    pub command: String,
}

#[derive(Debug, Deserialize)]
pub struct ToastPayload {
    #[serde(default)]
    pub title: Option<String>,
    pub message: String,
    pub variant: String,
    #[serde(default)]
    pub duration: Option<u64>,
}

#[derive(Debug, Deserialize)]
pub struct SessionSelectPayload {
    #[serde(rename = "sessionID")]
    pub session_id: String,
}

#[derive(Debug, Deserialize)]
#[serde(tag = "type")]
pub enum TuiPublishPayload {
    #[serde(rename = "tui.prompt.append")]
    PromptAppend { properties: AppendPromptPayload },
    #[serde(rename = "tui.command.execute")]
    CommandExecute { properties: ExecuteCommandPayload },
    #[serde(rename = "tui.toast.show")]
    ToastShow { properties: ToastPayload },
    #[serde(rename = "tui.session.select")]
    SessionSelect { properties: SessionSelectPayload },
}

pub fn tui_routes(state: Arc<AppState>) -> Router {
    Router::new()
        .route("/tui/append-prompt", post(append_prompt))
        .route("/tui/open-help", post(open_help))
        .route("/tui/open-sessions", post(open_sessions))
        .route("/tui/open-themes", post(open_themes))
        .route("/tui/open-models", post(open_models))
        .route("/tui/submit-prompt", post(submit_prompt))
        .route("/tui/clear-prompt", post(clear_prompt))
        .route("/tui/execute-command", post(execute_command))
        .route("/tui/show-toast", post(show_toast))
        .route("/tui/publish", post(publish_event))
        .route("/tui/select-session", post(select_session))
        .route("/tui/control/next", get(control_next))
        .route("/tui/control/response", post(control_response))
        .with_state(state)
}

async fn append_prompt(
    State(state): State<Arc<AppState>>,
    Json(payload): Json<AppendPromptPayload>,
) -> impl IntoResponse {
    info!(
        "TUI: append prompt: {}",
        &payload.text[..payload.text.len().min(80)]
    );
    let event = blazecode_core::bus::GlobalEvent::from_tui(
        &blazecode_core::bus::TuiBusEvent::TuiPromptAppend {
            text: payload.text.clone(),
        },
    )
    .expect("TuiBusEvent serialization must succeed");
    let _ = state.bus.publish(event);
    Json(serde_json::json!({ "appended": true, "text": payload.text }))
}

async fn open_help(State(state): State<Arc<AppState>>) -> impl IntoResponse {
    info!("TUI: open help requested");
    let event =
        blazecode_core::bus::GlobalEvent::from_tui(&blazecode_core::bus::TuiBusEvent::TuiHelpOpen)
            .expect("TuiBusEvent serialization must succeed");
    let _ = state.bus.publish(event);
    Json(serde_json::json!(true))
}

async fn open_sessions(State(state): State<Arc<AppState>>) -> impl IntoResponse {
    info!("TUI: open sessions requested");
    let event = blazecode_core::bus::GlobalEvent::from_tui(
        &blazecode_core::bus::TuiBusEvent::TuiSessionsOpen,
    )
    .expect("TuiBusEvent serialization must succeed");
    let _ = state.bus.publish(event);
    Json(serde_json::json!(true))
}

async fn open_themes(State(state): State<Arc<AppState>>) -> impl IntoResponse {
    info!("TUI: open themes requested");
    let event =
        blazecode_core::bus::GlobalEvent::from_tui(&blazecode_core::bus::TuiBusEvent::TuiThemesOpen)
            .expect("TuiBusEvent serialization must succeed");
    let _ = state.bus.publish(event);
    Json(serde_json::json!(true))
}

async fn open_models(State(state): State<Arc<AppState>>) -> impl IntoResponse {
    info!("TUI: open models requested");
    let event =
        blazecode_core::bus::GlobalEvent::from_tui(&blazecode_core::bus::TuiBusEvent::TuiModelsOpen)
            .expect("TuiBusEvent serialization must succeed");
    let _ = state.bus.publish(event);
    Json(serde_json::json!(true))
}

async fn submit_prompt(State(state): State<Arc<AppState>>) -> impl IntoResponse {
    info!("TUI: submit prompt requested");
    let event = blazecode_core::bus::GlobalEvent::from_tui(
        &blazecode_core::bus::TuiBusEvent::TuiPromptSubmit,
    )
    .expect("TuiBusEvent serialization must succeed");
    let _ = state.bus.publish(event);
    Json(serde_json::json!(true))
}

async fn clear_prompt(State(state): State<Arc<AppState>>) -> impl IntoResponse {
    info!("TUI: clear prompt requested");
    let event =
        blazecode_core::bus::GlobalEvent::from_tui(&blazecode_core::bus::TuiBusEvent::TuiPromptClear)
            .expect("TuiBusEvent serialization must succeed");
    let _ = state.bus.publish(event);
    Json(serde_json::json!(true))
}

async fn execute_command(
    State(state): State<Arc<AppState>>,
    Json(payload): Json<ExecuteCommandPayload>,
) -> impl IntoResponse {
    info!("TUI: execute command '{}'", payload.command);
    let event = blazecode_core::bus::GlobalEvent::from_tui(
        &blazecode_core::bus::TuiBusEvent::TuiCommandExecute {
            command: payload.command.clone(),
        },
    )
    .expect("TuiBusEvent serialization must succeed");
    let _ = state.bus.publish(event);
    Json(serde_json::json!({ "executed": true, "command": payload.command }))
}

async fn show_toast(
    State(state): State<Arc<AppState>>,
    Json(payload): Json<ToastPayload>,
) -> impl IntoResponse {
    info!(
        "TUI: show toast '{}' (variant: {})",
        payload.message, payload.variant
    );
    let event =
        blazecode_core::bus::GlobalEvent::from_tui(&blazecode_core::bus::TuiBusEvent::TuiToastShow {
            title: payload.title.clone(),
            message: payload.message.clone(),
            variant: payload.variant.clone(),
            duration: payload.duration,
        })
        .expect("TuiBusEvent serialization must succeed");
    let _ = state.bus.publish(event);
    Json(serde_json::json!({ "shown": true, "message": payload.message }))
}

async fn publish_event(
    State(state): State<Arc<AppState>>,
    Json(payload): Json<TuiPublishPayload>,
) -> impl IntoResponse {
    let tui_event = match &payload {
        TuiPublishPayload::PromptAppend { properties } => {
            blazecode_core::bus::TuiBusEvent::TuiPromptAppend {
                text: properties.text.clone(),
            }
        }
        TuiPublishPayload::CommandExecute { properties } => {
            blazecode_core::bus::TuiBusEvent::TuiCommandExecute {
                command: properties.command.clone(),
            }
        }
        TuiPublishPayload::ToastShow { properties } => {
            blazecode_core::bus::TuiBusEvent::TuiToastShow {
                title: properties.title.clone(),
                message: properties.message.clone(),
                variant: properties.variant.clone(),
                duration: None,
            }
        }
        TuiPublishPayload::SessionSelect { properties } => {
            blazecode_core::bus::TuiBusEvent::TuiSessionSelect {
                session_id: properties.session_id.clone(),
            }
        }
    };

    let event = blazecode_core::bus::GlobalEvent::from_tui(&tui_event)
        .expect("TuiBusEvent serialization must succeed");
    let event_type = event.event_type().unwrap_or("unknown").to_string();
    info!("TUI: publish event type={event_type}");
    let _ = state.bus.publish(event);
    Json(serde_json::json!({ "published": true, "type": event_type }))
}

async fn select_session(
    State(state): State<Arc<AppState>>,
    Json(payload): Json<SessionSelectPayload>,
) -> impl IntoResponse {
    info!("TUI: select session {}", payload.session_id);
    // Verify session exists
    match state.sessions.get(&payload.session_id).await {
        Ok(_) => {
            let event = blazecode_core::bus::GlobalEvent::from_tui(
                &blazecode_core::bus::TuiBusEvent::TuiSessionSelect {
                    session_id: payload.session_id.clone(),
                },
            )
            .expect("TuiBusEvent serialization must succeed");
            let _ = state.bus.publish(event);
            Json(serde_json::json!({ "selected": true, "session_id": payload.session_id }))
                .into_response()
        }
        Err(_) => (
            axum::http::StatusCode::NOT_FOUND,
            Json(
                serde_json::json!({"error": "session not found", "session_id": payload.session_id}),
            ),
        )
            .into_response(),
    }
}

async fn control_next(State(state): State<Arc<AppState>>) -> impl IntoResponse {
    // Check for pending permission requests first
    let permissions: Vec<_> = state.permissions.list();
    if let Some(req) = permissions.into_iter().next() {
        return Json(serde_json::json!({
            "type": "permission",
            "request": req,
        }));
    }

    // Check for pending questions
    let questions: Vec<_> = state.questions.list().await;
    if let Some(req) = questions.into_iter().next() {
        return Json(serde_json::json!({
            "type": "question",
            "request": req,
        }));
    }

    // No pending items
    Json(serde_json::json!({
        "type": "none",
        "request": null,
    }))
}

async fn control_response(
    State(state): State<Arc<AppState>>,
    Json(payload): Json<serde_json::Value>,
) -> impl IntoResponse {
    // Handle a control response — could be a permission reply, question answer, etc.
    if let Some(control_type) = payload.get("type").and_then(|v| v.as_str()) {
        info!("TUI: control response type={control_type}");
        match control_type {
            "permission" => {
                if let (Some(request_id), Some(reply)) = (
                    payload.get("request_id").and_then(|v| v.as_str()),
                    payload.get("reply").and_then(|v| v.as_str()),
                ) {
                    let perm_reply = match reply {
                        "once" => blazecode_core::permission::PermissionReply::Once,
                        "always" => blazecode_core::permission::PermissionReply::Always,
                        "reject" => blazecode_core::permission::PermissionReply::Reject,
                        _ => blazecode_core::permission::PermissionReply::Once,
                    };
                    let input = blazecode_core::permission::ReplyInput {
                        request_id: request_id.to_string(),
                        reply: perm_reply,
                        message: payload
                            .get("message")
                            .and_then(|v| v.as_str())
                            .map(String::from),
                    };
                    let _ = state.permissions.reply(input).await;
                }
            }
            "question" => {
                if let Some(request_id) = payload.get("request_id").and_then(|v| v.as_str()) {
                    let request_id_obj =
                        blazecode_core::question::QuestionId::new_unchecked(request_id);
                    if let Some(answers) = payload.get("answers").and_then(|v| v.as_array()) {
                        let question_answers: Vec<blazecode_core::question::QuestionAnswer> =
                            answers
                                .iter()
                                .filter_map(|a| {
                                    a.as_array()
                                        .map(|arr| {
                                            arr.iter()
                                                .filter_map(|v| v.as_str().map(String::from))
                                                .collect::<Vec<_>>()
                                        })
                                        .map(blazecode_core::question::QuestionAnswer::new)
                                })
                                .collect();
                        let _ = state
                            .questions
                            .reply(&request_id_obj, question_answers)
                            .await;
                    } else {
                        let _ = state.questions.reject(&request_id_obj).await;
                    }
                }
            }
            _ => {
                warn!("Unknown control response type: {control_type}");
            }
        }
    }
    Json(serde_json::json!({ "submitted": true }))
}
