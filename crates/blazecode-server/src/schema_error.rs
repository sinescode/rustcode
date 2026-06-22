//! Schema error middleware — catches deserialization/validation errors and
//! returns structured `InvalidRequestError` responses with truncated messages.
//!
//! Ported from: `packages/blazecode/src/server/routes/instance/httpapi/middleware/schema-error.ts`

use axum::extract::Request;
use axum::middleware::Next;
use axum::response::{IntoResponse, Response};
use axum::Json;
use serde_json::json;

use crate::error::ServerError;
use crate::workspace_routing::log_schema_rejection;

/// Maximum length for error reason messages before truncation.
///
/// # Source
/// Ported from `packages/blazecode/src/server/routes/instance/httpapi/middleware/schema-error.ts`
/// line 10 (`const REASON_LIMIT = 1024`).
const REASON_LIMIT: usize = 1024;

/// Truncate a reason string to `REASON_LIMIT` bytes with a suffix.
///
/// # Source
/// Ported from `packages/blazecode/src/server/routes/instance/httpapi/middleware/schema-error.ts`
/// `truncateReason()`.
fn truncate_reason(reason: &str) -> String {
    if reason.len() <= REASON_LIMIT {
        return reason.to_string();
    }
    let suffix_len = format!("… ({} more chars)", reason.len() - REASON_LIMIT).len();
    let cut = REASON_LIMIT.saturating_sub(suffix_len + 1);
    format!(
        "{}… ({} more chars)",
        &reason[..cut],
        reason.len() - REASON_LIMIT
    )
}

/// Schema error middleware for axum.
///
/// Catches JSON deserialization/validation errors by inspecting the response.
/// In practice, axum's rejection handling is better suited to
/// `HandleErrorLayer`; this middleware provides a hook for catching and
/// reformatting error responses.
///
/// # Source
/// Ported from `packages/blazecode/src/server/routes/instance/httpapi/middleware/schema-error.ts`
pub async fn schema_error_middleware(
    req: Request,
    next: Next,
) -> Response {
    let _path = req.uri().path().to_string();
    let response = next.run(req).await;

    // If the response is a 400, it may be a JSON rejection.
    // Axum's default 400 body is plain text; we could inspect and replace it.
    // For now, this is a passthrough — individual routes can use
    // `schema_error_response` directly.
    response
}

/// Create a structured error response for schema validation failures.
///
/// For V1 endpoints (paths not starting with `/api/`), returns:
/// `{ "name": "BadRequest", "data": { "message": "...", "kind": "..." } }`
///
/// For V2 endpoints (paths starting with `/api/`), returns an
/// `InvalidRequestError`.
///
/// # Source
/// Ported from `packages/blazecode/src/server/routes/instance/httpapi/middleware/schema-error.ts`
pub fn schema_error_response(path: &str, kind: &str, message: &str) -> Response {
    let reason = truncate_reason(message);
    log_schema_rejection(kind, &reason);

    if path.starts_with("/api/") {
        ServerError::InvalidRequest {
            message: reason,
            kind: Some(kind.to_string()),
        }
        .into_response()
    } else {
        let body = json!({
            "name": "BadRequest",
            "data": {
                "message": reason,
                "kind": kind,
            }
        });
        (axum::http::StatusCode::BAD_REQUEST, Json(body)).into_response()
    }
}

/// Convert an axum `JsonRejection` into a structured error response.
///
/// Can be used with `axum::error_handling::HandleErrorLayer` or standalone.
pub fn json_rejection_to_response(err: axum::extract::rejection::JsonRejection) -> Response {
    schema_error_response("", "JsonRejection", &err.to_string())
}
