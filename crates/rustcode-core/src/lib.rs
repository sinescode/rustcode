#![forbid(unsafe_code)]
#![allow(dead_code, unused_imports, unused_variables)]
#![warn(clippy::all)]

//! Core library for rustcode — AI-powered development tool.
//!
//! Ported from the OpenCode TypeScript monorepo.
//! Source: `packages/opencode/src/` and `packages/core/src/`
//! OpenCode commit: 5d0f86606ac30690f79f0a6a9f41a1f49fe95d0b

pub mod account;
pub mod agent;
pub mod aisdk;
pub mod background_job;
pub mod bus;
pub mod catalog;
pub mod command;
pub mod config;
pub mod credential;
pub mod database;
pub mod env;
pub mod error;
pub mod flag;
pub mod event;
pub mod file_mutation;
pub mod filesystem;
pub mod format;
pub mod fs_util;
pub mod git;
pub mod global;
pub mod id;
pub mod image;
pub mod instruction_context;
pub mod integration;
pub mod location;
pub mod lsp;
pub mod mcp;
pub mod model;
pub mod npm;
pub mod observability;
pub mod patch;
pub mod permission;
pub mod plugin;
pub mod policy;
pub mod process;
pub mod project;
pub mod provider;
pub mod providers;
pub mod pty;
pub mod question;
pub mod reference;
pub mod repository;
pub mod ripgrep;
pub mod runtime;
pub mod schema;
pub mod session;
pub mod session_compaction;
pub mod session_execution;
pub mod session_history;
pub mod session_info;
pub mod session_message;
pub mod session_prompt;
pub mod session_runner;
pub mod session_todo;
pub mod shell;
pub mod skill;
pub mod snapshot;
pub mod sse;
pub mod system_context;
pub mod state;
pub mod tool_output_store;
pub mod storage;
pub mod tool;
pub mod tool_impls;
pub mod tool_stream;
pub mod v2_schema;
pub mod worktree;
pub mod workspace;
