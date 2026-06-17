#![forbid(unsafe_code)]
#![warn(clippy::all)]

//! HTTP server and SSE event streaming for rustcode.
//!
//! Ported from: `packages/opencode/src/server/`
//!
//! ## Architecture
//!
//! The server exposes two API surfaces:
//! - **Global/Control** — unauthenticated health, global config, control-plane operations
//! - **Instance** — workspace-scoped session, question, permission, tool, provider, etc.
//!
//! SSE events are streamed on `GET /event` with `text/event-stream` content type.
//! The server supports graceful shutdown via `tokio::signal`.

pub mod cors;
pub mod routes;
pub mod server;
pub mod sse;

pub use server::{AppState, ServerConfig, build_router, serve};
