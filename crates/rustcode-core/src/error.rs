//! Error types for rustcode-core.
//!
//! Ported from: cross-cutting error handling in OpenCode.

use thiserror::Error;

/// Top-level error for the rustcode-core crate.
#[derive(Debug, Error)]
pub enum Error {
    /// I/O error
    #[error("I/O error: {0}")]
    Io(#[from] std::io::Error),

    /// Serialization/deserialization error
    #[error("JSON error: {0}")]
    Json(#[from] serde_json::Error),

    /// Configuration error
    #[error("Config error: {0}")]
    Config(String),

    /// Database error
    #[error("Database error: {0}")]
    Database(String),

    /// Provider/LLM error
    #[error("Provider error: {0}")]
    Provider(String),

    /// Tool execution error
    #[error("Tool error: {0}")]
    Tool(String),

    /// Session error
    #[error("Session error: {0}")]
    Session(String),

    /// Permission denied
    #[error("Permission denied: {0}")]
    PermissionDenied(String),

    /// Not found
    #[error("Not found: {0}")]
    NotFound(String),

    /// Plugin error
    #[error("Plugin error: {0}")]
    Plugin(String),

    /// Git error
    #[error("Git error: {0}")]
    Git(String),

    /// Network error
    #[error("Network error: {0}")]
    Network(String),

    /// Context overflow
    #[error("Context overflow: {0}")]
    ContextOverflow(String),

    /// Aborted by user
    #[error("Aborted")]
    Aborted,

    /// TOML parsing error
    #[error("TOML error: {0}")]
    Toml(#[from] toml::de::Error),

    /// Internal error (should never happen)
    #[error("Internal error: {0}")]
    Internal(String),
}

/// Result type alias for rustcode-core.
pub type Result<T> = std::result::Result<T, Error>;
