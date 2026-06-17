//! Route modules for the HTTP server.
//!
//! Each module corresponds to a route group in the TS source:
//! `packages/opencode/src/server/routes/instance/httpapi/groups/`

pub mod config;
pub mod control;
pub mod control_plane;
pub mod event;
pub mod experimental;
pub mod file;
pub mod global;
pub mod instance;
pub mod mcp;
pub mod permission;
pub mod project;
pub mod project_copy;
pub mod provider;
pub mod question;
pub mod session;
pub mod sync;
pub mod tui;
pub mod workspace;
