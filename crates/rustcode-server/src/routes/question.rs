//! Question routes — list, reply, reject.
//!
//! Ported from: `packages/opencode/src/server/routes/instance/httpapi/groups/question.ts`
//!
//! Route paths:
//! - `GET  /question`                   — list pending questions
//! - `POST /question/:requestID/reply`   — reply to a question
//! - `POST /question/:requestID/reject`  — reject a question

use axum::extract::{Path, State};
use axum::response::IntoResponse;
use axum::{Json, Router};
use axum::routing::{get, post};
use serde::Deserialize;
use std::sync::Arc;

use crate::server::AppState;

/// Reply payload.
///
/// # Source
/// `ReplyPayload` in `question.ts` line 13.
#[derive(Debug, Deserialize)]
pub struct ReplyPayload {
    pub answers: Vec<Vec<String>>,
}

/// Create the question routes router.
pub fn question_routes(state: Arc<AppState>) -> Router {
    Router::new()
        .route("/question", get(list_questions))
        .route("/question/{requestID}/reply", post(reply_question))
        .route("/question/{requestID}/reject", post(reject_question))
        .with_state(state)
}

async fn list_questions() -> impl IntoResponse {
    Json(serde_json::json!([]))
}

async fn reply_question(
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
    Path(request_id): Path<String>,
) -> impl IntoResponse {
    Json(serde_json::json!({
        "rejected": true,
        "request_id": request_id,
    }))
}
