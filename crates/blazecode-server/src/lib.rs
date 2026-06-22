#![forbid(unsafe_code)]
#![warn(clippy::all)]

//! HTTP server and SSE event streaming for blazecode.
//!
//! Ported from: `packages/blazecode/src/server/`
//!
//! ## Architecture
//!
//! The server exposes two API surfaces:
//! - **Global/Control** — unauthenticated health, global config, control-plane operations
//! - **Instance** — workspace-scoped session, question, permission, tool, provider, etc.
//!
//! Auth is implemented via axum middleware that checks `BLAZECODE_SERVER_PASSWORD`.
//! Error responses follow the blazecode JSON format `{"name": "...", "data": {...}}`.
//! SSE events are streamed on `GET /event` with `text/event-stream` content type.
//! The server supports graceful shutdown via `tokio::signal`.

pub mod auth;
pub mod cors;
pub mod error;
pub mod fence;
pub mod instance_context;
pub mod proxy;
pub mod routes;
pub mod schema_error;
pub mod server;
pub mod sse;
pub mod workspace_routing;

pub use auth::AuthConfig;
pub use error::ServerError;
pub use server::{build_router, serve, AppState, ServerConfig};
