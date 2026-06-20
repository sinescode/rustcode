//! Question routes — list, reply, reject.
//!
//! Ported from: `packages/opencode/src/server/routes/instance/httpapi/groups/question.ts`

use axum::extract::{Path, State};
use axum::response::IntoResponse;
use axum::routing::{get, post};
use axum::{Json, Router};
use serde::Deserialize;
use std::sync::Arc;
use tracing::info;

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

async fn list_questions(State(state): State<Arc<AppState>>) -> impl IntoResponse {
    let pending = state.questions.list().await;
    info!("Listing {} pending questions", pending.len());
    Json(serde_json::to_value(pending).unwrap_or_default())
}

async fn reply_question(
    State(state): State<Arc<AppState>>,
    Path(request_id): Path<String>,
    Json(payload): Json<ReplyPayload>,
) -> impl IntoResponse {
    let answers: Vec<rustcode_core::question::QuestionAnswer> = payload
        .answers
        .into_iter()
        .map(rustcode_core::question::QuestionAnswer::new)
        .collect();
    let answer_count = answers.len();
    let request_id_obj = rustcode_core::question::QuestionId::new_unchecked(&request_id);
    match state.questions.reply(&request_id_obj, answers).await {
        Ok(()) => {
            info!("Question {request_id} replied with {answer_count} answers");
            Json(serde_json::json!({
                "replied": true,
                "request_id": request_id,
                "answer_count": answer_count,
            }))
            .into_response()
        }
        Err(e) => (
            axum::http::StatusCode::NOT_FOUND,
            Json(serde_json::json!({"error": e.to_string(), "request_id": request_id})),
        )
            .into_response(),
    }
}

async fn reject_question(
    State(state): State<Arc<AppState>>,
    Path(request_id): Path<String>,
) -> impl IntoResponse {
    let request_id_obj = rustcode_core::question::QuestionId::new_unchecked(&request_id);
    match state.questions.reject(&request_id_obj).await {
        Ok(()) => {
            info!("Question {request_id} rejected");
            Json(serde_json::json!({
                "rejected": true,
                "request_id": request_id,
            }))
            .into_response()
        }
        Err(e) => (
            axum::http::StatusCode::NOT_FOUND,
            Json(serde_json::json!({"error": e.to_string(), "request_id": request_id})),
        )
            .into_response(),
    }
}
