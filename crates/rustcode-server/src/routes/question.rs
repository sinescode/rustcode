//! Question routes — list, reply, reject.
//!
//! Ported from: `packages/opencode/src/server/routes/instance/httpapi/groups/question.ts`

use axum::extract::{Path, State};
use axum::response::IntoResponse;
use axum::{Json, Router};
use axum::routing::{get, post};
use serde::Deserialize;
use std::sync::Arc;

use crate::server::AppState;

#[derive(Debug, Deserialize)]
pub struct ReplyPayload {
    pub answers: Vec<Vec<String>>,
}

pub fn question_routes(state: Arc<AppState>) -> Router {
    Router::new()
        .route("/question", get(list_questions))
        .route("/question/{requestID}/reply", post(reply_question))
        .route("/question/{requestID}/reject", post(reject_question))
        .with_state(state)
}

async fn list_questions(State(_): State<Arc<AppState>>) -> impl IntoResponse {
    Json(serde_json::json!([]))
}

async fn reply_question(
    State(_): State<Arc<AppState>>,
    Path(request_id): Path<String>,
    Json(payload): Json<ReplyPayload>,
) -> impl IntoResponse {
    Json(serde_json::json!({
        "replied": true,
        "request_id": request_id,
        "answer_count": payload.answers.len(),
    }))
}

async fn reject_question(
    State(_): State<Arc<AppState>>,
    Path(request_id): Path<String>,
) -> impl IntoResponse {
    Json(serde_json::json!({
        "rejected": true,
        "request_id": request_id,
    }))
}
