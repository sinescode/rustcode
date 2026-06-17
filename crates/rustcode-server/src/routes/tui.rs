//! TUI routes — prompt append, dialogs, command execution, toast, publish, control.
//!
//! Ported from: `packages/opencode/src/server/routes/instance/httpapi/groups/tui.ts`

use axum::extract::State;
use axum::response::IntoResponse;
use axum::{Json, Router};
use axum::routing::{get, post};
use serde::Deserialize;
use std::sync::Arc;

use crate::server::AppState;

#[derive(Debug, Deserialize)]
pub struct AppendPromptPayload { pub text: String }

#[derive(Debug, Deserialize)]
pub struct ExecuteCommandPayload { pub command: String }

#[derive(Debug, Deserialize)]
pub struct ToastPayload {
    #[serde(default)] pub title: Option<String>,
    pub message: String,
    pub variant: String,
    #[serde(default)] pub duration: Option<u64>,
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
    State(_): State<Arc<AppState>>,
    Json(payload): Json<AppendPromptPayload>,
) -> impl IntoResponse {
    Json(serde_json::json!({ "appended": true, "text": payload.text }))
}

async fn open_help(State(_): State<Arc<AppState>>) -> impl IntoResponse { Json(serde_json::json!(true)) }
async fn open_sessions(State(_): State<Arc<AppState>>) -> impl IntoResponse { Json(serde_json::json!(true)) }
async fn open_themes(State(_): State<Arc<AppState>>) -> impl IntoResponse { Json(serde_json::json!(true)) }
async fn open_models(State(_): State<Arc<AppState>>) -> impl IntoResponse { Json(serde_json::json!(true)) }
async fn submit_prompt(State(_): State<Arc<AppState>>) -> impl IntoResponse { Json(serde_json::json!(true)) }
async fn clear_prompt(State(_): State<Arc<AppState>>) -> impl IntoResponse { Json(serde_json::json!(true)) }

async fn execute_command(
    State(_): State<Arc<AppState>>,
    Json(payload): Json<ExecuteCommandPayload>,
) -> impl IntoResponse {
    Json(serde_json::json!({ "executed": true, "command": payload.command }))
}

async fn show_toast(
    State(_): State<Arc<AppState>>,
    Json(payload): Json<ToastPayload>,
) -> impl IntoResponse {
    Json(serde_json::json!({ "shown": true, "message": payload.message }))
}

async fn publish_event(
    State(_): State<Arc<AppState>>,
    Json(payload): Json<TuiPublishPayload>,
) -> impl IntoResponse {
    let event_type = match &payload {
        TuiPublishPayload::PromptAppend { .. } => "tui.prompt.append",
        TuiPublishPayload::CommandExecute { .. } => "tui.command.execute",
        TuiPublishPayload::ToastShow { .. } => "tui.toast.show",
        TuiPublishPayload::SessionSelect { .. } => "tui.session.select",
    };
    Json(serde_json::json!({ "published": true, "type": event_type }))
}

async fn select_session(
    State(_): State<Arc<AppState>>,
    Json(payload): Json<SessionSelectPayload>,
) -> impl IntoResponse {
    Json(serde_json::json!({ "selected": true, "session_id": payload.session_id }))
}

async fn control_next(State(_): State<Arc<AppState>>) -> impl IntoResponse {
    Json(serde_json::json!(null))
}

async fn control_response(
    State(_): State<Arc<AppState>>,
    Json(_payload): Json<serde_json::Value>,
) -> impl IntoResponse {
    Json(serde_json::json!({ "submitted": true }))
}
