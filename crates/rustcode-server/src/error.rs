//! Error standardization — unified error types producing opencode-compatible JSON.
//!
//! Ported from: `packages/server/src/errors.ts`
//!
//! All errors serialize to the opencode wire format:
//! ```json
//! { "name": "ErrorName", "data": { "message": "error text" } }
//! ```

use axum::http::StatusCode;
use axum::response::{IntoResponse, Response};
use axum::Json;

/// A unified server error that maps to opencode error classes.
///
/// Each variant corresponds to a specific HTTP status code and
/// serializes to the opencode JSON error format.
#[derive(Debug, thiserror::Error)]
pub enum ServerError {
    // ── 400 Bad Request ──────────────────────────────────────────────
    #[error("Invalid request: {message}")]
    InvalidRequest {
        message: String,
        kind: Option<String>,
    },

    #[error("Invalid cursor: {message}")]
    InvalidCursor { message: String },

    // ── 401 Unauthorized ─────────────────────────────────────────────
    #[error("Authentication required")]
    Unauthorized { message: String },

    // ── 403 Forbidden ────────────────────────────────────────────────
    #[error("Forbidden: {message}")]
    Forbidden { message: String },

    #[error("PTY access forbidden: {message}")]
    PtyForbidden { message: String },

    // ── 404 Not Found ────────────────────────────────────────────────
    #[error("Provider not found: {provider_id}")]
    ProviderNotFound {
        provider_id: String,
        message: String,
    },

    #[error("Model not found: {message}")]
    ModelNotFound { message: String },

    #[error("Session not found: {session_id}")]
    SessionNotFound {
        session_id: String,
        message: String,
    },

    #[error("Message not found: {message}")]
    MessageNotFound { message: String },

    #[error("Question not found: {request_id}")]
    QuestionNotFound {
        request_id: String,
        message: String,
    },

    #[error("Permission request not found: {request_id}")]
    PermissionNotFound {
        request_id: String,
        message: String,
    },

    #[error("MCP server not found: {name}")]
    McpServerNotFound { name: String, message: String },

    #[error("PTY not found: {pty_id}")]
    PtyNotFound {
        pty_id: String,
        message: String,
    },

    #[error("Project not found: {project_id}")]
    ProjectNotFound {
        project_id: String,
        message: String,
    },

    #[error("API endpoint not found: {path}")]
    ApiNotFound { path: String, message: String },

    // ── 409 Conflict ─────────────────────────────────────────────────
    #[error("Conflict: {message}")]
    Conflict {
        message: String,
        resource: Option<String>,
    },

    #[error("Session busy: {message}")]
    SessionBusy { message: String },

    // ── 500 Internal Server Error ────────────────────────────────────
    #[error("Internal error: {message}")]
    Unknown { message: String },

    // ── 502 Bad Gateway ──────────────────────────────────────────────
    #[error("Upstream error: {message}")]
    Upstream { message: String },

    // ── 503 Service Unavailable ──────────────────────────────────────
    #[error("Service unavailable: {message}")]
    ServiceUnavailable {
        message: String,
        service: Option<String>,
    },

    // ── 504 Gateway Timeout ──────────────────────────────────────────
    #[error("Timeout: {message}")]
    Timeout { message: String },
}

impl ServerError {
    /// Return the opencode error class name.
    pub fn error_name(&self) -> &'static str {
        match self {
            ServerError::InvalidRequest { .. } => "InvalidRequestError",
            ServerError::InvalidCursor { .. } => "InvalidCursorError",
            ServerError::Unauthorized { .. } => "UnauthorizedError",
            ServerError::Forbidden { .. } => "ForbiddenError",
            ServerError::PtyForbidden { .. } => "PtyForbiddenError",
            ServerError::ProviderNotFound { .. } => "ProviderNotFoundError",
            ServerError::ModelNotFound { .. } => "ModelNotFoundError",
            ServerError::SessionNotFound { .. } => "SessionNotFoundError",
            ServerError::MessageNotFound { .. } => "MessageNotFoundError",
            ServerError::QuestionNotFound { .. } => "QuestionNotFoundError",
            ServerError::PermissionNotFound { .. } => "PermissionNotFoundError",
            ServerError::McpServerNotFound { .. } => "McpServerNotFoundError",
            ServerError::PtyNotFound { .. } => "PtyNotFoundError",
            ServerError::ProjectNotFound { .. } => "ProjectNotFoundError",
            ServerError::ApiNotFound { .. } => "ApiNotFoundError",
            ServerError::Conflict { .. } => "ConflictError",
            ServerError::SessionBusy { .. } => "SessionBusyError",
            ServerError::Unknown { .. } => "UnknownError",
            ServerError::Upstream { .. } => "UpstreamError",
            ServerError::ServiceUnavailable { .. } => "ServiceUnavailableError",
            ServerError::Timeout { .. } => "TimeoutError",
        }
    }

    /// Return the HTTP status code for this error.
    pub fn status_code(&self) -> StatusCode {
        match self {
            ServerError::InvalidRequest { .. }
            | ServerError::InvalidCursor { .. } => StatusCode::BAD_REQUEST,
            ServerError::Unauthorized { .. } => StatusCode::UNAUTHORIZED,
            ServerError::Forbidden { .. } | ServerError::PtyForbidden { .. } => {
                StatusCode::FORBIDDEN
            }
            ServerError::ProviderNotFound { .. }
            | ServerError::ModelNotFound { .. }
            | ServerError::SessionNotFound { .. }
            | ServerError::MessageNotFound { .. }
            | ServerError::QuestionNotFound { .. }
            | ServerError::PermissionNotFound { .. }
            | ServerError::McpServerNotFound { .. }
            | ServerError::PtyNotFound { .. }
            | ServerError::ProjectNotFound { .. }
            | ServerError::ApiNotFound { .. } => StatusCode::NOT_FOUND,
            ServerError::Conflict { .. } | ServerError::SessionBusy { .. } => {
                StatusCode::CONFLICT
            }
            ServerError::Unknown { .. } => StatusCode::INTERNAL_SERVER_ERROR,
            ServerError::Upstream { .. } => StatusCode::BAD_GATEWAY,
            ServerError::ServiceUnavailable { .. } => StatusCode::SERVICE_UNAVAILABLE,
            ServerError::Timeout { .. } => StatusCode::GATEWAY_TIMEOUT,
        }
    }

    /// Build the JSON body in opencode error format.
    pub fn json_body(&self) -> serde_json::Value {
        let name = self.error_name();
        let message = self.to_string();

        // Build data object with common fields
        let mut data = serde_json::json!({ "message": message });

        // Add extra fields for specific error types
        match self {
            ServerError::InvalidRequest { kind, .. } => {
                if let Some(k) = kind {
                    data["kind"] = serde_json::Value::String(k.clone());
                }
            }
            ServerError::ProviderNotFound { provider_id, .. } => {
                data["providerID"] = serde_json::Value::String(provider_id.clone());
            }
            ServerError::SessionNotFound { session_id, .. } => {
                data["sessionID"] = serde_json::Value::String(session_id.clone());
            }
            ServerError::QuestionNotFound { request_id, .. } => {
                data["requestID"] = serde_json::Value::String(request_id.clone());
            }
            ServerError::PermissionNotFound { request_id, .. } => {
                data["requestID"] = serde_json::Value::String(request_id.clone());
            }
            ServerError::McpServerNotFound { name, .. } => {
                data["name"] = serde_json::Value::String(name.clone());
            }
            ServerError::PtyNotFound { pty_id, .. } => {
                data["ptyID"] = serde_json::Value::String(pty_id.clone());
            }
            ServerError::ProjectNotFound { project_id, .. } => {
                data["projectID"] = serde_json::Value::String(project_id.clone());
            }
            ServerError::ApiNotFound { path, .. } => {
                data["path"] = serde_json::Value::String(path.clone());
            }
            ServerError::Conflict { resource, .. } => {
                if let Some(r) = resource {
                    data["resource"] = serde_json::Value::String(r.clone());
                }
            }
            ServerError::ServiceUnavailable { service, .. } => {
                if let Some(s) = service {
                    data["service"] = serde_json::Value::String(s.clone());
                }
            }
            _ => {}
        }

        serde_json::json!({
            "name": name,
            "data": data,
        })
    }
}

impl IntoResponse for ServerError {
    fn into_response(self) -> Response {
        let status = self.status_code();
        let body = self.json_body();
        (status, Json(body)).into_response()
    }
}

// ── Convenience constructors ────────────────────────────────────────────────

impl ServerError {
    pub fn invalid_request(message: impl Into<String>) -> Self {
        ServerError::InvalidRequest {
            message: message.into(),
            kind: None,
        }
    }

    pub fn invalid_cursor(message: impl Into<String>) -> Self {
        ServerError::InvalidCursor {
            message: message.into(),
        }
    }

    pub fn unauthorized(message: impl Into<String>) -> Self {
        ServerError::Unauthorized {
            message: message.into(),
        }
    }

    pub fn forbidden(message: impl Into<String>) -> Self {
        ServerError::Forbidden {
            message: message.into(),
        }
    }

    pub fn session_not_found(session_id: impl Into<String>) -> Self {
        let id = session_id.into();
        ServerError::SessionNotFound {
            message: format!("Session '{}' not found", id),
            session_id: id,
        }
    }

    pub fn message_not_found(message: impl Into<String>) -> Self {
        ServerError::MessageNotFound {
            message: message.into(),
        }
    }

    pub fn provider_not_found(provider_id: impl Into<String>) -> Self {
        let id = provider_id.into();
        ServerError::ProviderNotFound {
            message: format!("Provider '{}' not found", id),
            provider_id: id,
        }
    }

    pub fn pty_not_found(pty_id: impl Into<String>) -> Self {
        let id = pty_id.into();
        ServerError::PtyNotFound {
            message: format!("PTY '{}' not found", id),
            pty_id: id,
        }
    }

    pub fn conflict(message: impl Into<String>) -> Self {
        ServerError::Conflict {
            message: message.into(),
            resource: None,
        }
    }

    pub fn unknown(message: impl Into<String>) -> Self {
        ServerError::Unknown {
            message: message.into(),
        }
    }

    pub fn upstream(message: impl Into<String>) -> Self {
        ServerError::Upstream {
            message: message.into(),
        }
    }

    pub fn service_unavailable(message: impl Into<String>) -> Self {
        ServerError::ServiceUnavailable {
            message: message.into(),
            service: None,
        }
    }

    pub fn timeout(message: impl Into<String>) -> Self {
        ServerError::Timeout {
            message: message.into(),
        }
    }
}

/// Helper trait for converting domain errors into `ServerError`.
pub trait IntoServerError {
    fn into_server_error(self) -> ServerError;
}
