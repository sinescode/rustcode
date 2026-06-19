//! Route modules for the HTTP server.
//!
//! Each module corresponds to a route group in the TS source:
//! `packages/opencode/src/server/routes/instance/httpapi/groups/`

pub mod agent;
pub mod command;
pub mod config;
pub mod control;
pub mod control_plane;
pub mod credential;
pub mod event;
pub mod experimental;
pub mod file;
pub mod global;
pub mod health;
pub mod instance;
pub mod integration;
pub mod mcp;
pub mod metadata;
pub mod model;
pub mod permission;
pub mod project;
pub mod project_copy;
pub mod provider;
pub mod pty;
pub mod query;
pub mod question;
pub mod reference;
pub mod session;
pub mod skill;
pub mod sync;
pub mod tui;
pub mod workspace;
