#![forbid(unsafe_code)]
// Relaxed for scaffolding phase — tighten as modules are implemented
#![allow(dead_code, unused_imports, unused_variables)]
#![warn(clippy::all, clippy::pedantic, clippy::nursery)]

//! Core library for rustcode — AI-powered development tool.
//!
//! Ported from the OpenCode TypeScript monorepo.
//! Source: `packages/opencode/src/` and `packages/core/src/`
//! OpenCode commit: 5d0f86606ac30690f79f0a6a9f41a1f49fe95d0b

pub mod agent;
pub mod bus;
pub mod config;
pub mod env;
pub mod error;
pub mod format;
pub mod git;
pub mod id;
pub mod image;
pub mod lsp;
pub mod mcp;
pub mod permission;
pub mod plugin;
pub mod provider;
pub mod question;
pub mod session;
pub mod skill;
pub mod snapshot;
pub mod storage;
pub mod tool;
pub mod worktree;
