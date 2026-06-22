#![forbid(unsafe_code)]
#![warn(clippy::all)]

//! Terminal UI for blazecode.
//!
//! Ported from: `packages/tui/src/`
//!
//! ## Architecture
//!
//! The TUI uses `ratatui` + `crossterm` to render a terminal interface with:
//! - **Conversation view** — scrolling message display with user/assistant messages
//! - **Input area** — prompt input with keybindings
//! - **Status line** — busy/idle/retry status, LSP/MCP counts, directory
//! - **Permission prompt** — modal dialog for permission requests
//! - **Question prompt** — modal dialog for question requests
//!
//! Events are received from the server via SSE and dispatched to the UI.

pub mod syntax;
pub mod which_key;
pub mod app;
pub mod clipboard;
pub mod components;
pub mod editor;
pub mod event;
pub mod home_screen;
pub mod keymap;
pub mod plugin;
pub mod sse_client;
pub mod theme;

pub use app::TuiApp;
pub use plugin::TuiPluginManager;
pub use sse_client::SseClient;
