//! Error types for rustcode-core.
//!
//! Ported from: cross-cutting error handling across `OpenCode`.
//! OpenCode commit: 5d0f86606ac30690f79f0a6a9f41a1f49fe95d0b
//!
//! The TS codebase uses Effect.ts `Schema.TaggedErrorClass` for ~120+ error
//! types. This module consolidates them into a hierarchical `thiserror` enum:
//! a top-level [`Error`] for cross-crate propagation, plus domain-specific
//! sub-enums ([`LlmErrorReason`], [`PermissionError`], etc.) that mirror the
//! TS structure and preserve match-ability.

use std::collections::HashMap;
use thiserror::Error;

// ── Top-level error ─────────────────────────────────────────────────

/// Top-level error for the rustcode-core crate.
///
/// # Source
/// Aggregated from all `Schema.TaggedErrorClass` definitions across
/// `packages/opencode/src/` and `packages/core/src/`.
#[derive(Debug, Error)]
pub enum Error {
    // -- I/O & filesystem --
    /// I/O error (file read/write, directory operations).
    #[error("I/O error: {0}")]
    Io(#[from] std::io::Error),

    /// File system operation error.
    ///
    /// Ported from `packages/core/src/fs-util.ts` `FileSystemError`.
    #[error("filesystem error in `{path}`: {message}")]
    FileSystem { path: String, message: String },

    /// Attempted to write a file whose content has changed since it was read.
    ///
    /// Ported from `packages/core/src/file-mutation.ts` `StaleContentError`.
    #[error("stale content: file `{path}` was modified externally")]
    StaleContent { path: String },

    /// Target path already exists when it should not.
    ///
    /// Ported from `packages/core/src/file-mutation.ts` `TargetExistsError`.
    #[error("target already exists: `{path}`")]
    TargetExists { path: String },

    /// File is binary and cannot be processed as text.
    ///
    /// Ported from `packages/core/src/tool/read-filesystem.ts` `BinaryFileError`.
    #[error("binary file: `{path}`")]
    BinaryFile { path: String },

    /// File exceeds the media ingest size limit.
    ///
    /// Ported from `packages/core/src/tool/read-filesystem.ts` `MediaIngestLimitError`.
    #[error("media ingest limit exceeded for `{path}`")]
    MediaIngestLimit { path: String },

    // -- Serialization --
    /// JSON serialization/deserialization error.
    #[error("JSON error: {0}")]
    Json(#[from] serde_json::Error),

    /// TOML parsing error.
    #[error("TOML error: {0}")]
    Toml(#[from] toml::de::Error),

    // -- Config --
    /// Configuration error.
    #[error("config error: {0}")]
    Config(String),

    // -- Database --
    /// Database error.
    #[error("database error: {0}")]
    Database(String),

    // -- LLM / Provider --
    /// LLM provider error with structured reason.
    ///
    /// Ported from `packages/llm/src/schema/errors.ts` `LLMError`.
    #[error("{module}.{method}: {reason}")]
    Llm {
        module: String,
        method: String,
        reason: Box<LlmErrorReason>,
    },

    /// Provider initialization error.
    ///
    /// Ported from `packages/opencode/src/provider/provider.ts` `InitError`.
    #[error("provider init error ({provider_id}): {message}")]
    ProviderInit {
        provider_id: String,
        message: String,
    },

    /// No providers are configured or available.
    ///
    /// Ported from `packages/opencode/src/provider/provider.ts` `NoProvidersError`.
    #[error("no providers available")]
    NoProviders,

    /// No models available for the given provider.
    ///
    /// Ported from `packages/opencode/src/provider/provider.ts` `NoModelsError`.
    #[error("no models available for provider `{provider_id}`")]
    NoModels { provider_id: String },

    /// Model not found.
    ///
    /// Ported from `packages/opencode/src/provider/provider.ts` `ModelNotFoundError`
    /// and `packages/core/src/catalog.ts` `ModelNotFoundError`.
    #[error("model `{model_id}` not found (provider: `{provider_id}`)")]
    ModelNotFound {
        provider_id: String,
        model_id: String,
    },

    /// Provider response headers timed out.
    ///
    /// Ported from `packages/opencode/src/provider/error.ts` `HeaderTimeoutError`.
    #[error("provider response headers timed out after {ms}ms")]
    HeaderTimeout { ms: u64 },

    /// Provider response stream error.
    ///
    /// Ported from `packages/opencode/src/provider/error.ts` `ResponseStreamError`.
    #[error("provider response stream error: {0}")]
    ResponseStream(String),

    /// Context window overflow — input exceeds model's context length.
    ///
    /// Ported from `packages/llm/src/provider-error.ts` `isContextOverflow`.
    #[error("context overflow: {0}")]
    ContextOverflow(String),

    // -- Tool --
    /// Tool execution error.
    ///
    /// Ported from `packages/llm/src/schema/errors.ts` `ToolFailure`.
    #[error("tool error: {0}")]
    Tool(String),

    /// Tool called with invalid arguments.
    ///
    /// Ported from `packages/opencode/src/tool/tool.ts` `InvalidArgumentsError`.
    #[error("the `{tool}` tool was called with invalid arguments: {detail}")]
    ToolInvalidArguments { tool: String, detail: String },

    /// Tool registration error.
    ///
    /// Ported from `packages/core/src/tool/tool.ts` `RegistrationError`.
    #[error("tool registration error for `{name}`: {message}")]
    ToolRegistration { name: String, message: String },

    // -- Session --
    /// Session error.
    #[error("session error: {0}")]
    Session(String),

    /// Session not found.
    ///
    /// Ported from `packages/core/src/session.ts` `NotFoundError`.
    #[error("session `{session_id}` not found")]
    SessionNotFound { session_id: String },

    /// Session is busy (another operation in progress).
    ///
    /// Ported from `packages/opencode/src/session/session.ts` `BusyError`.
    #[error("session `{session_id}` is busy")]
    SessionBusy { session_id: String },

    /// Session prompt conflict.
    ///
    /// Ported from `packages/core/src/session.ts` `PromptConflictError`.
    #[error("session prompt conflict in `{session_id}`")]
    SessionPromptConflict { session_id: String },

    /// Session operation unavailable.
    ///
    /// Ported from `packages/core/src/session.ts` `OperationUnavailableError`.
    #[error("operation unavailable for session `{session_id}`: {reason}")]
    SessionOperationUnavailable { session_id: String, reason: String },

    /// Step limit exceeded in session runner.
    ///
    /// Ported from `packages/core/src/session/runner/index.ts` `StepLimitExceededError`.
    #[error("step limit exceeded in session `{session_id}`")]
    StepLimitExceeded { session_id: String },

    /// Model not selected for session.
    ///
    /// Ported from `packages/core/src/session/runner/model.ts` `ModelNotSelectedError`.
    #[error("no model selected for session")]
    ModelNotSelected,

    /// Message decode error.
    ///
    /// Ported from `packages/core/src/session/error.ts` `MessageDecodeError`.
    #[error("failed to decode message `{message_id}` in session `{session_id}`")]
    MessageDecode {
        session_id: String,
        message_id: String,
    },

    // -- Permission --
    /// Permission error.
    #[error("{0}")]
    Permission(#[from] PermissionError),

    // -- Git / Worktree --
    /// Git operation error.
    ///
    /// Ported from `packages/core/src/git.ts` `WorktreeError`, `PatchError`.
    #[error("git error: {0}")]
    Git(String),

    /// Worktree error.
    ///
    /// Ported from `packages/opencode/src/worktree/index.ts`.
    #[error("{0}")]
    Worktree(#[from] WorktreeError),

    // -- Image --
    /// Image processing error.
    ///
    /// Ported from `packages/core/src/image.ts` and `packages/opencode/src/image/image.ts`.
    #[error("{0}")]
    Image(#[from] ImageError),

    // -- Plugin --
    /// Plugin error.
    #[error("plugin error: {0}")]
    Plugin(String),

    // -- Skill --
    /// Skill error.
    ///
    /// Ported from `packages/opencode/src/skill/index.ts`.
    #[error("{0}")]
    Skill(#[from] SkillError),

    // -- MCP --
    /// MCP server not found.
    ///
    /// Ported from `packages/opencode/src/mcp/index.ts` `NotFoundError`.
    #[error("MCP server `{name}` not found")]
    McpNotFound { name: String },

    // -- LSP --
    /// LSP initialization error.
    ///
    /// Ported from `packages/opencode/src/lsp/client.ts` `InitializeError`.
    #[error("LSP initialization error: {0}")]
    LspInit(String),

    // -- Process --
    /// Subprocess execution error.
    ///
    /// Ported from `packages/core/src/process.ts` `AppProcessError`
    /// and `packages/opencode/src/util/process.ts` `RunFailedError`.
    #[error("process error: {message} (exit code: {exit_code:?})")]
    Process {
        message: String,
        exit_code: Option<i32>,
    },

    // -- Network --
    /// Network / HTTP error.
    #[error("network error: {0}")]
    Network(String),

    /// HTTP request error (from reqwest).
    #[error("HTTP error: {0}")]
    Http(#[from] reqwest::Error),

    // -- Auth --
    /// Authentication error.
    ///
    /// Ported from `packages/opencode/src/auth/index.ts` `AuthError`.
    #[error("auth error: {0}")]
    Auth(String),

    // -- Question --
    /// Question rejected by user.
    ///
    /// Ported from `packages/core/src/question.ts` `RejectedError`.
    #[error("question rejected")]
    QuestionRejected,

    /// Question not found.
    ///
    /// Ported from `packages/core/src/question.ts` `NotFoundError`.
    #[error("question `{question_id}` not found")]
    QuestionNotFound { question_id: String },

    // -- Project --
    /// Project not found.
    ///
    /// Ported from `packages/opencode/src/project/project.ts` `NotFoundError`.
    #[error("project `{project_id}` not found")]
    ProjectNotFound { project_id: String },

    // -- Storage --
    /// Item not found in storage.
    ///
    /// Ported from `packages/opencode/src/storage/storage.ts` `NotFoundError`.
    #[error("not found: {entity} `{id}`")]
    NotFound { entity: String, id: String },

    // -- Search --
    /// Ripgrep / search error.
    ///
    /// Ported from `packages/core/src/ripgrep.ts` `Error`, `InvalidPatternError`.
    #[error("search error: {0}")]
    Search(String),

    /// Invalid search pattern.
    #[error("invalid search pattern: {0}")]
    InvalidSearchPattern(String),

    // -- Aborted --
    /// Operation aborted by user or signal.
    #[error("aborted")]
    Aborted,

    // -- Not implemented --
    /// Feature not yet implemented.
    ///
    /// Used as a stub for modules still under construction.
    #[error("not implemented: {0}")]
    NotImplemented(String),

    // -- Internal --
    /// Internal error (should never happen in normal operation).
    #[error("internal error: {0}")]
    Internal(String),
}

// ── LLM error reason (mirrors TS LLMErrorReason union) ─────────────

/// Structured reason for an LLM error.
///
/// # Source
/// Ported from `packages/llm/src/schema/errors.ts` — the `LLMErrorReason`
/// tagged union with 10 variants.
#[derive(Debug, Error)]
pub enum LlmErrorReason {
    /// Invalid request (bad prompt, unsupported parameter).
    ///
    /// Ported from `LLM.Error.InvalidRequest`.
    #[error("invalid request: {message}")]
    InvalidRequest {
        message: String,
        parameter: Option<String>,
        classification: Option<String>,
    },

    /// No route available for the given provider/model combination.
    ///
    /// Ported from `LLM.Error.NoRoute`.
    #[error("no route for {provider}/{model} using {route}")]
    NoRoute {
        route: String,
        provider: String,
        model: String,
    },

    /// Authentication failure (missing, invalid, or expired credentials).
    ///
    /// Ported from `LLM.Error.Authentication`.
    #[error("authentication error: {message}")]
    Authentication {
        message: String,
        kind: AuthErrorKind,
    },

    /// Rate limited by the provider.
    ///
    /// Ported from `LLM.Error.RateLimit`.
    #[error("rate limited: {message}")]
    RateLimit {
        message: String,
        retry_after_ms: Option<u64>,
    },

    /// Quota exceeded (billing/plan limit).
    ///
    /// Ported from `LLM.Error.QuotaExceeded`.
    #[error("quota exceeded: {message}")]
    QuotaExceeded { message: String },

    /// Content policy violation.
    ///
    /// Ported from `LLM.Error.ContentPolicy`.
    #[error("content policy violation: {message}")]
    ContentPolicy { message: String },

    /// Provider internal error (500-level).
    ///
    /// Ported from `LLM.Error.ProviderInternal`.
    #[error("provider internal error (status {status}): {message}")]
    ProviderInternal {
        message: String,
        status: u16,
        retry_after_ms: Option<u64>,
    },

    /// Transport/network error.
    ///
    /// Ported from `LLM.Error.Transport`.
    #[error("transport error: {message}")]
    Transport {
        message: String,
        kind: Option<String>,
        url: Option<String>,
    },

    /// Invalid output from the provider (unparseable response).
    ///
    /// Ported from `LLM.Error.InvalidProviderOutput`.
    #[error("invalid provider output: {message}")]
    InvalidProviderOutput {
        message: String,
        raw: Option<String>,
    },

    /// Unknown/unclassified provider error.
    ///
    /// Ported from `LLM.Error.UnknownProvider`.
    #[error("unknown provider error: {message}")]
    UnknownProvider {
        message: String,
        status: Option<u16>,
    },
}

impl LlmErrorReason {
    /// Whether this error reason is retryable.
    ///
    /// Mirrors the `retryable` getter on each TS error reason class.
    pub fn is_retryable(&self) -> bool {
        matches!(self, Self::RateLimit { .. } | Self::ProviderInternal { .. })
    }

    /// Retry-after hint in milliseconds, if available.
    pub fn retry_after_ms(&self) -> Option<u64> {
        match self {
            Self::RateLimit { retry_after_ms, .. }
            | Self::ProviderInternal { retry_after_ms, .. } => *retry_after_ms,
            _ => None,
        }
    }
}

// ── Auth error kind ─────────────────────────────────────────────────

/// Authentication error classification.
///
/// # Source
/// Ported from `packages/llm/src/schema/errors.ts` `AuthenticationReason.kind`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum AuthErrorKind {
    Missing,
    Invalid,
    Expired,
    InsufficientPermissions,
    Unknown,
}

impl std::fmt::Display for AuthErrorKind {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Missing => write!(f, "missing"),
            Self::Invalid => write!(f, "invalid"),
            Self::Expired => write!(f, "expired"),
            Self::InsufficientPermissions => write!(f, "insufficient-permissions"),
            Self::Unknown => write!(f, "unknown"),
        }
    }
}

// ── Permission errors ───────────────────────────────────────────────

/// Permission system errors.
///
/// # Source
/// Ported from `packages/core/src/permission.ts` — `RejectedError`,
/// `CorrectedError`, `DeniedError`, `NotFoundError`.
#[derive(Debug, Error)]
pub enum PermissionError {
    /// Permission rejected by user.
    #[error("permission rejected")]
    Rejected,

    /// Permission corrected by user (with feedback).
    #[error("permission corrected: {feedback}")]
    Corrected { feedback: String },

    /// Permission denied by ruleset.
    #[error("permission denied")]
    Denied,

    /// Permission request not found.
    #[error("permission request `{request_id}` not found")]
    NotFound { request_id: String },
}

// ── Worktree errors ─────────────────────────────────────────────────

/// Worktree management errors.
///
/// # Source
/// Ported from `packages/opencode/src/worktree/index.ts`.
#[derive(Debug, Error)]
pub enum WorktreeError {
    /// Not in a git repository.
    #[error("not a git repository")]
    NotGit,

    /// Failed to generate a worktree name.
    #[error("failed to generate worktree name")]
    NameGenerationFailed,

    /// Failed to create a worktree.
    #[error("failed to create worktree: {0}")]
    CreateFailed(String),

    /// Failed to start command in worktree.
    #[error("failed to start command in worktree: {0}")]
    StartCommandFailed(String),

    /// Failed to remove a worktree.
    #[error("failed to remove worktree: {0}")]
    RemoveFailed(String),

    /// Failed to reset a worktree.
    #[error("failed to reset worktree: {0}")]
    ResetFailed(String),

    /// Failed to list worktrees.
    #[error("failed to list worktrees: {0}")]
    ListFailed(String),
}

// ── Image errors ────────────────────────────────────────────────────

/// Image processing errors.
///
/// # Source
/// Ported from `packages/core/src/image.ts` and
/// `packages/opencode/src/image/image.ts`.
#[derive(Debug, Error)]
pub enum ImageError {
    /// Image resizer is not available.
    #[error("image resizer unavailable")]
    ResizerUnavailable,

    /// Invalid data URL format.
    #[error("invalid data URL")]
    InvalidDataUrl,

    /// Failed to decode image data.
    #[error("image decode error")]
    Decode,

    /// Image exceeds size limits.
    #[error("image too large: {width}x{height} exceeds limit")]
    Size { width: u32, height: u32 },
}

// ── Skill errors ────────────────────────────────────────────────────

/// Skill-related errors.
///
/// # Source
/// Ported from `packages/opencode/src/skill/index.ts`.
#[derive(Debug, Error)]
pub enum SkillError {
    /// Skill definition is invalid.
    #[error("invalid skill: {0}")]
    Invalid(String),

    /// Skill name does not match expected name.
    #[error("skill name mismatch: expected `{expected}`, got `{actual}`")]
    NameMismatch { expected: String, actual: String },

    /// Skill not found.
    #[error("skill `{name}` not found")]
    NotFound { name: String },
}

// ── HTTP API errors (server layer) ──────────────────────────────────

/// HTTP API error codes for the server layer.
///
/// # Source
/// Ported from `packages/opencode/src/server/routes/instance/httpapi/errors.ts`.
#[derive(Debug, Error)]
pub enum ApiError {
    /// 400 Bad Request.
    #[error("invalid request: {0}")]
    InvalidRequest(String),

    /// 401 Unauthorized.
    #[error("unauthorized")]
    Unauthorized,

    /// 403 Forbidden.
    #[error("forbidden")]
    Forbidden,

    /// 404 Not Found.
    #[error("not found: {entity}")]
    NotFound { entity: String },

    /// 409 Conflict.
    #[error("conflict: {0}")]
    Conflict(String),

    /// 408 Timeout.
    #[error("timeout: {0}")]
    Timeout(String),

    /// 502 Upstream error.
    #[error("upstream error: {0}")]
    Upstream(String),

    /// 503 Service unavailable.
    #[error("service unavailable: {0}")]
    ServiceUnavailable(String),

    /// 500 Unknown/internal error.
    #[error("unknown error: {0}")]
    Unknown(String),
}

// ── Context overflow detection ──────────────────────────────────────

/// Check whether an error message indicates a context window overflow.
///
/// # Source
/// Ported from `packages/llm/src/provider-error.ts` `isContextOverflow`.
/// Uses the same regex patterns as the TS source.
pub fn is_context_overflow(message: &str) -> bool {
    let lower = message.to_lowercase();
    let patterns = [
        "prompt is too long",
        "input is too long for requested model",
        "exceeds the context window",
        "input token count",
        "maximum prompt length is",
        "reduce the length of the messages",
        "maximum context length is",
        "exceeds the limit of",
        "exceeds the available context size",
        "greater than the context length",
        "context window exceeds limit",
        "exceeded model token limit",
        "context_length_exceeded",
        "context length exceeded",
        "request entity too large",
        "context length is only",
        "input length",
        "prompt too long",
        "model_context_window_exceeded",
    ];
    patterns.iter().any(|p| lower.contains(p))
}

// ── Result alias ────────────────────────────────────────────────────

/// Result type alias for rustcode-core.
pub type Result<T> = std::result::Result<T, Error>;

// ── HTTP context for LLM errors ─────────────────────────────────────

/// HTTP request/response context attached to LLM errors.
///
/// # Source
/// Ported from `packages/llm/src/schema/errors.ts` `HttpContext`.
#[derive(Debug, Clone, Default)]
pub struct HttpContext {
    pub method: String,
    pub url: String,
    pub request_headers: HashMap<String, String>,
    pub response_status: Option<u16>,
    pub response_headers: Option<HashMap<String, String>>,
    pub body: Option<String>,
    pub body_truncated: bool,
    pub request_id: Option<String>,
    pub retry_after_ms: Option<u64>,
}

// ── Tests ───────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_result_alias() {
        let ok: Result<i32> = Ok(42);
        assert_eq!(ok.unwrap(), 42);

        let err: Result<i32> = Err(Error::Aborted);
        assert!(err.is_err());
    }

    #[test]
    fn test_io_error_conversion() {
        let io_err = std::io::Error::new(std::io::ErrorKind::NotFound, "file not found");
        let err: Error = io_err.into();
        assert!(matches!(err, Error::Io(_)));
        assert!(err.to_string().contains("file not found"));
    }

    #[test]
    fn test_json_error_conversion() {
        let json_err = serde_json::from_str::<serde_json::Value>("not json").unwrap_err();
        let err: Error = json_err.into();
        assert!(matches!(err, Error::Json(_)));
    }

    #[test]
    fn test_permission_error_conversion() {
        let perm_err = PermissionError::Rejected;
        let err: Error = perm_err.into();
        assert!(matches!(err, Error::Permission(PermissionError::Rejected)));
        assert_eq!(err.to_string(), "permission rejected");
    }

    #[test]
    fn test_worktree_error_conversion() {
        let wt_err = WorktreeError::NotGit;
        let err: Error = wt_err.into();
        assert!(matches!(err, Error::Worktree(WorktreeError::NotGit)));
    }

    #[test]
    fn test_image_error_display() {
        let err = ImageError::Size {
            width: 4096,
            height: 4096,
        };
        assert_eq!(err.to_string(), "image too large: 4096x4096 exceeds limit");
    }

    #[test]
    fn test_skill_error_display() {
        let err = SkillError::NameMismatch {
            expected: "foo".into(),
            actual: "bar".into(),
        };
        assert_eq!(
            err.to_string(),
            "skill name mismatch: expected `foo`, got `bar`"
        );
    }

    #[test]
    fn test_llm_error_reason_retryable() {
        let rate_limit = LlmErrorReason::RateLimit {
            message: "slow down".into(),
            retry_after_ms: Some(5000),
        };
        assert!(rate_limit.is_retryable());
        assert_eq!(rate_limit.retry_after_ms(), Some(5000));

        let auth = LlmErrorReason::Authentication {
            message: "bad key".into(),
            kind: AuthErrorKind::Invalid,
        };
        assert!(!auth.is_retryable());
        assert_eq!(auth.retry_after_ms(), None);

        let internal = LlmErrorReason::ProviderInternal {
            message: "overloaded".into(),
            status: 503,
            retry_after_ms: Some(10000),
        };
        assert!(internal.is_retryable());
        assert_eq!(internal.retry_after_ms(), Some(10000));
    }

    #[test]
    fn test_llm_error_display() {
        let reason = LlmErrorReason::RateLimit {
            message: "too many requests".into(),
            retry_after_ms: None,
        };
        let err = Error::Llm {
            module: "anthropic".into(),
            method: "stream".into(),
            reason: Box::new(reason),
        };
        assert_eq!(
            err.to_string(),
            "anthropic.stream: rate limited: too many requests"
        );
    }

    #[test]
    fn test_context_overflow_detection() {
        assert!(is_context_overflow("prompt is too long"));
        assert!(is_context_overflow(
            "This input exceeds the context window of the model"
        ));
        assert!(is_context_overflow("context_length_exceeded"));
        assert!(is_context_overflow(
            "Maximum context length is 128000 tokens"
        ));
        assert!(is_context_overflow("Request entity too large"));
        assert!(is_context_overflow("model_context_window_exceeded"));
        assert!(!is_context_overflow("everything is fine"));
        assert!(!is_context_overflow(""));
    }

    #[test]
    fn test_auth_error_kind_display() {
        assert_eq!(AuthErrorKind::Missing.to_string(), "missing");
        assert_eq!(AuthErrorKind::Invalid.to_string(), "invalid");
        assert_eq!(AuthErrorKind::Expired.to_string(), "expired");
        assert_eq!(
            AuthErrorKind::InsufficientPermissions.to_string(),
            "insufficient-permissions"
        );
        assert_eq!(AuthErrorKind::Unknown.to_string(), "unknown");
    }

    #[test]
    fn test_api_error_variants() {
        let err = ApiError::InvalidRequest("missing field".into());
        assert_eq!(err.to_string(), "invalid request: missing field");

        let err = ApiError::NotFound {
            entity: "session".into(),
        };
        assert_eq!(err.to_string(), "not found: session");
    }

    #[test]
    fn test_tool_invalid_arguments() {
        let err = Error::ToolInvalidArguments {
            tool: "bash".into(),
            detail: "missing command field".into(),
        };
        assert!(err
            .to_string()
            .contains("the `bash` tool was called with invalid arguments"));
    }

    #[test]
    fn test_session_errors() {
        let err = Error::SessionNotFound {
            session_id: "ses_123".into(),
        };
        assert_eq!(err.to_string(), "session `ses_123` not found");

        let err = Error::SessionBusy {
            session_id: "ses_456".into(),
        };
        assert_eq!(err.to_string(), "session `ses_456` is busy");
    }

    #[test]
    fn test_process_error() {
        let err = Error::Process {
            message: "command failed".into(),
            exit_code: Some(1),
        };
        assert!(err.to_string().contains("exit code: Some(1)"));
    }

    #[test]
    fn test_http_context_default() {
        let ctx = HttpContext::default();
        assert!(ctx.method.is_empty());
        assert!(ctx.response_status.is_none());
        assert!(!ctx.body_truncated);
    }
}
