//! Event types for the TUI application.
//!
//! Ported from: `packages/tui/src/context/event.ts` and
//! `packages/opencode/src/server/tui-event.ts`
//!
//! Events flow from the server (via SSE) into the TUI's event loop, where they
//! are dispatched to the appropriate UI components.

use serde::{Deserialize, Serialize};

// ── TUI Events (from server) ──────────────────────────────────────────────────

/// TUI event received from the server via SSE.
///
/// # Source
/// `packages/opencode/src/server/tui-event.ts` — `TuiEvent` definitions.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type")]
pub enum TuiEvent {
    /// Append text to the TUI prompt.
    ///
    /// # Source
    /// `TuiEvent.PromptAppend` in `tui-event.ts` line 9.
    #[serde(rename = "tui.prompt.append")]
    PromptAppend {
        #[serde(default)]
        properties: PromptAppendProperties,
    },

    /// Execute a TUI command.
    ///
    /// # Source
    /// `TuiEvent.CommandExecute` in `tui-event.ts` line 10.
    #[serde(rename = "tui.command.execute")]
    CommandExecute {
        #[serde(default)]
        properties: CommandExecuteProperties,
    },

    /// Show a toast notification.
    ///
    /// # Source
    /// `TuiEvent.ToastShow` in `tui-event.ts` line 36.
    #[serde(rename = "tui.toast.show")]
    ToastShow {
        #[serde(default)]
        properties: ToastProperties,
    },

    /// Navigate to a session.
    ///
    /// # Source
    /// `TuiEvent.SessionSelect` in `tui-event.ts` line 47.
    #[serde(rename = "tui.session.select")]
    SessionSelect {
        #[serde(default)]
        properties: SessionSelectProperties,
    },
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct PromptAppendProperties {
    #[serde(default)]
    pub text: String,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct CommandExecuteProperties {
    #[serde(default)]
    pub command: String,
}

/// Toast notification properties.
///
/// # Source
/// `TuiEvent.ToastShow.data` in `tui-event.ts` line 38.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct ToastProperties {
    #[serde(default)]
    pub title: Option<String>,
    #[serde(default)]
    pub message: String,
    /// Toast variant: "info", "success", "warning", "error".
    #[serde(default)]
    pub variant: String,
    /// Duration in milliseconds.
    #[serde(default = "default_toast_duration")]
    pub duration: u64,
}

fn default_toast_duration() -> u64 {
    5000
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct SessionSelectProperties {
    /// Session ID to navigate to.
    #[serde(default, rename = "sessionID")]
    pub session_id: String,
}

// ── Session Status (for status line) ──────────────────────────────────────────

/// Session status used in the status line.
///
/// # Source
/// `packages/opencode/src/session/status.ts` lines 9–33.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type")]
pub enum SessionStatus {
    /// Session is idle — no processing active.
    #[serde(rename = "idle")]
    Idle,
    /// Session is busy — LLM streaming or tool execution in progress.
    #[serde(rename = "busy")]
    Busy,
    /// Session is in retry state — error occurred, waiting for retry.
    #[serde(rename = "retry")]
    Retry {
        attempt: u64,
        message: String,
        #[serde(default)]
        action: Option<RetryAction>,
        next: u64,
    },
}

/// Retry action information shown in the status line.
///
/// # Source
/// `RetryAction` in `session/status.ts`.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RetryAction {
    pub reason: String,
    pub provider: String,
    pub title: String,
    pub message: String,
    pub label: String,
    #[serde(default)]
    pub link: Option<String>,
}

// ── App Events (local UI events) ──────────────────────────────────────────────

/// Application-level event used internally by the TUI.
#[derive(Debug, Clone)]
pub enum AppEvent {
    /// A TUI event received from the server.
    TuiEvent(TuiEvent),
    /// Session status changed.
    SessionStatusChanged {
        session_id: String,
        status: SessionStatus,
    },
    /// The user submitted a prompt.
    PromptSubmit(String),
    /// The user wants to quit.
    Quit,
    /// A permission request needs user input.
    PermissionAsk {
        request: rustcode_core::permission::PermissionRequest,
    },
    /// A question request needs user input.
    QuestionAsk {
        question_id: String,
        questions: Vec<QuestionItem>,
    },
}

/// A question item presented to the user.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct QuestionItem {
    pub question: String,
    pub header: Option<String>,
    #[serde(default)]
    pub options: Vec<QuestionOption>,
    #[serde(default)]
    pub multiple: bool,
    #[serde(default = "default_true")]
    pub custom: bool,
}

fn default_true() -> bool {
    true
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct QuestionOption {
    pub label: String,
    #[serde(default)]
    pub description: Option<String>,
}
