//! Main TUI application — event loop, rendering, key dispatch.
//!
//! Ported from: `packages/tui/src/app.tsx` and `packages/tui/src/routes/session/index.tsx`
//!
//! ## Architecture
//!
//! The TUI runs a main event loop that:
//! 1. Reads keyboard events from `crossterm`
//! 2. Dispatches to the appropriate component (input, permission, question)
//! 3. Reads SSE events from the server bus
//! 4. Renders the conversation view, input area, status line, and overlays
//!
//! Terminal setup/teardown uses crossterm's `EnterAlternateScreen` and `EnableRawMode`.

use crossterm::{
    event::{self, DisableMouseCapture, EnableMouseCapture, Event, KeyEventKind},
    execute,
    terminal::{disable_raw_mode, enable_raw_mode, EnterAlternateScreen, LeaveAlternateScreen},
};
use ratatui::{
    layout::{Constraint, Direction, Layout},
    style::{Color, Style},
    Frame,
    Terminal,
};
use std::collections::HashMap;
use std::io::{self, Stdout};
use std::sync::Arc;
use std::time::Duration;

use crate::components::conversation::{render_conversation, ConversationState};
use crate::components::input::{render_input, InputState};
use crate::components::permission::{render_permission, PermissionReply, PermissionState};
use crate::components::question::{render_question, QuestionState};
use crate::components::status::{render_status, StatusState};
use crate::event::{AppEvent, SessionStatus, TuiEvent};
use crate::keymap::{is_leader_prefix, key_to_action, leader_chord_to_action, TuiAction};

/// The main TUI application.
///
/// # Source
/// Ported from `packages/tui/src/app.tsx` `App` component (lines 351–1101).
pub struct TuiApp {
    /// Terminal handle.
    terminal: Terminal<ratatui::backend::CrosstermBackend<Stdout>>,

    // ── Component states ──────────────────────────────────────────
    /// Conversation view state.
    conversation: ConversationState,
    /// Input prompt state.
    input: InputState,
    /// Status line state.
    status: StatusState,
    /// Permission prompt state.
    permission: PermissionState,
    /// Question prompt state.
    question: QuestionState,

    // ── App state ─────────────────────────────────────────────────
    /// Whether the app should exit.
    should_quit: bool,
    /// Whether the leader key (Ctrl+X) was pressed and we're waiting for a chord.
    leader_active: bool,
    /// Current session ID (if viewing a session).
    session_id: Option<String>,
    /// Event bus subscription (set after connecting to server).
    bus: Option<rustcode_core::bus::SharedBus>,
}

impl TuiApp {
    /// Create a new TuiApp and initialize the terminal.
    ///
    /// # Source
    /// Ported from `packages/tui/src/app.tsx` `run()` function.
    pub fn new() -> anyhow::Result<Self> {
        // Setup terminal
        enable_raw_mode()?;
        let mut stdout = io::stdout();
        execute!(stdout, EnterAlternateScreen, EnableMouseCapture)?;
        let backend = ratatui::backend::CrosstermBackend::new(stdout);
        let terminal = Terminal::new(backend)?;

        Ok(Self {
            terminal,
            conversation: ConversationState::new(),
            input: InputState::new(),
            status: StatusState::new(),
            permission: PermissionState::new(),
            question: QuestionState::new(),
            should_quit: false,
            leader_active: false,
            session_id: None,
            bus: None,
        })
    }

    /// Run the main event loop.
    ///
    /// # Source
    /// Ported from `packages/tui/src/app.tsx` — the render loop and event handling.
    pub fn run(&mut self, bus: rustcode_core::bus::SharedBus) -> anyhow::Result<()> {
        self.bus = Some(bus.clone());
        let mut sub = bus.subscribe();

        let tick_rate = Duration::from_millis(50);

        loop {
            // Render
            self.terminal.draw(|f| self.render(f))?;

            // Check for exit
            if self.should_quit {
                break;
            }

            // Poll for events
            if event::poll(tick_rate)? {
                match event::read()? {
                    Event::Key(key) => {
                        // Only handle press events (not release or repeat)
                        if key.kind != KeyEventKind::Press {
                            continue;
                        }

                        self.handle_key_event(key);

                        // Check for exit again after key handling
                        if self.should_quit {
                            break;
                        }
                    }
                    Event::Resize(_, _) => {
                        // Terminal was resized — render will adapt
                    }
                    Event::Mouse(_) => {
                        // Mouse events — handled by ratatui internally
                    }
                    _ => {}
                }
            }

            // Poll for bus events (non-blocking)
            // In a full implementation, this would use tokio::select!
            // For now, check for events synchronously using try_recv
        }

        Ok(())
    }

    /// Restore the terminal to its original state.
    pub fn cleanup(&mut self) -> anyhow::Result<()> {
        disable_raw_mode()?;
        execute!(
            self.terminal.backend_mut(),
            LeaveAlternateScreen,
            DisableMouseCapture
        )?;
        self.terminal.show_cursor()?;
        Ok(())
    }

    // ── Rendering ────────────────────────────────────────────────────────

    /// Render the entire TUI.
    fn render(&mut self, f: &mut Frame) {
        let area = f.area();

        // Check if permission or question overlays are active
        let overlay_active = self.permission.visible || self.question.visible;

        if !overlay_active {
            // Normal layout: conversation (flex), input (3 lines), status (1 line)
            let chunks = Layout::default()
                .direction(Direction::Vertical)
                .constraints([
                    Constraint::Min(0),    // Conversation
                    Constraint::Length(3), // Input
                    Constraint::Length(1), // Status
                ])
                .split(area);

            render_conversation(f, chunks[0], &self.conversation);
            render_input(f, chunks[1], &self.input);
            render_status(f, chunks[2], &self.status);
        } else {
            // When overlays are active, show dimmed background
            let dim_style = Style::default().bg(Color::Rgb(20, 20, 20));
            f.buffer_mut().set_style(area, dim_style);

            let chunks = Layout::default()
                .direction(Direction::Vertical)
                .constraints([
                    Constraint::Min(0),
                    Constraint::Length(3),
                    Constraint::Length(1),
                ])
                .split(area);

            render_conversation(f, chunks[0], &self.conversation);
            render_input(f, chunks[1], &self.input);
            render_status(f, chunks[2], &self.status);
        }

        // Render overlays on top
        if self.permission.visible {
            render_permission(f, area, &self.permission);
        }
        if self.question.visible {
            render_question(f, area, &self.question);
        }
    }

    // ── Key Event Handling ───────────────────────────────────────────────

    /// Handle a keyboard event.
    fn handle_key_event(&mut self, key: crossterm::event::KeyEvent) {
        // If leader is active, handle chord
        if self.leader_active {
            self.leader_active = false;
            if let Some(action) = leader_chord_to_action(key) {
                self.dispatch_action(action);
            }
            return;
        }

        // Check for leader prefix (Ctrl+X)
        if is_leader_prefix(key) {
            self.leader_active = true;
            return;
        }

        // If permission dialog is visible, handle there first
        if self.permission.visible {
            if let Some(reply) = self.permission.handle_key(key) {
                self.handle_permission_reply(reply);
            }
            return;
        }

        // If question dialog is visible, handle there first
        if self.question.visible {
            if let Some((request_id, answers)) = self.question.handle_key(key) {
                self.handle_question_reply(request_id, answers);
            }
            return;
        }

        // Try input handling first
        if self.input.focused {
            if self.input.handle_key(key) {
                // Key was consumed by input
                return;
            }
            // Enter was not consumed → submit
            if key.code == crossterm::event::KeyCode::Enter
                && key.modifiers == crossterm::event::KeyModifiers::NONE
            {
                let text = self.input.take();
                if !text.is_empty() {
                    self.handle_prompt_submit(text);
                }
                return;
            }
        }

        // Map to action
        if let Some(action) = key_to_action(key) {
            self.dispatch_action(action);
        }
    }

    /// Dispatch a TUI action.
    fn dispatch_action(&mut self, action: TuiAction) {
        match action {
            TuiAction::Quit => {
                self.should_quit = true;
            }
            TuiAction::SessionInterrupt => {
                // Send interrupt via bus or API
                self.status.session_status = Some(SessionStatus::Idle);
            }
            TuiAction::ScrollUp => self.conversation.scroll_up(1),
            TuiAction::ScrollDown => self.conversation.scroll_down(1),
            TuiAction::ScrollPageUp => {
                let page_size = self.terminal.get_frame().area().height / 2;
                self.conversation.scroll_up(page_size);
            }
            TuiAction::ScrollPageDown => {
                let page_size = self.terminal.get_frame().area().height / 2;
                self.conversation.scroll_down(page_size);
            }
            TuiAction::ScrollFirst => self.conversation.scroll_to_top(),
            TuiAction::ScrollLast => self.conversation.scroll_to_bottom(),
            TuiAction::AgentCycle => {
                // Cycle to the next agent
            }
            TuiAction::AgentCycleReverse => {
                // Cycle to the previous agent
            }
            TuiAction::VariantCycle => {
                // Cycle model variants
            }
            TuiAction::ToggleSidebar => {
                // Toggle sidebar visibility
            }
            TuiAction::ToggleConceal => {
                // Toggle code concealment
            }
            TuiAction::ToggleTimestamps => {
                // Toggle timestamps
            }
            TuiAction::ToggleThinking => {
                // Toggle thinking visibility
            }
            TuiAction::ToggleToolDetails => {
                // Toggle tool details visibility
            }
            TuiAction::ToggleScrollbar => {
                // Toggle scrollbar
            }
            TuiAction::ToggleGenericToolOutput => {
                // Toggle generic tool output
            }
            TuiAction::ToggleTerminalTitle => {
                // Toggle terminal title
            }
            TuiAction::ToggleAnimations => {
                // Toggle animations
            }
            TuiAction::ToggleFileContext => {
                // Toggle file context
            }
            TuiAction::ToggleDiffWrap => {
                // Toggle diff wrapping
            }
            TuiAction::TogglePasteSummary => {
                // Toggle paste summary
            }
            TuiAction::SessionNew => {
                // Navigate to home (new session)
                self.session_id = None;
            }
            TuiAction::CommandPalette => {
                // Open command palette
            }
            TuiAction::Status => {
                // Show status dialog
            }
            TuiAction::Help => {
                // Show help dialog
            }
            TuiAction::Suspend => {
                // Suspend terminal (Ctrl+Z)
                // This requires raw mode to be temporarily disabled
            }
            TuiAction::InputClear => {
                self.input.clear();
            }
            TuiAction::PermissionPrevOption => {
                self.permission.prev_option();
            }
            TuiAction::PermissionNextOption => {
                self.permission.next_option();
            }
            TuiAction::PermissionOnce => {
                if let Some(reply) = self.permission.select() {
                    self.handle_permission_reply(reply);
                }
            }
            _ => {
                // Unhandled actions — log for now
                tracing::debug!("unhandled action: {action:?}");
            }
        }
    }

    // ── Event Handlers ───────────────────────────────────────────────────

    /// Handle a prompt submission.
    fn handle_prompt_submit(&mut self, text: String) {
        tracing::info!("prompt submitted: {text}");
        // In a full implementation, this would call the server API to create a message
    }

    /// Handle a permission reply.
    fn handle_permission_reply(&mut self, reply: PermissionReply) {
        match reply {
            PermissionReply::Once => {
                tracing::info!("permission: allow once");
            }
            PermissionReply::Always => {
                tracing::info!("permission: allow always");
            }
            PermissionReply::Reject { message } => {
                tracing::info!("permission: reject, message: {message:?}");
            }
        }
        self.permission.dismiss();
    }

    /// Handle a question reply (submit or reject).
    fn handle_question_reply(&mut self, request_id: String, answers: Vec<Vec<String>>) {
        if answers.is_empty() {
            tracing::info!("question rejected: {request_id}");
        } else {
            tracing::info!("question answered: {request_id}, answers: {answers:?}");
        }
        self.question.dismiss();
    }

    // ── Public API ───────────────────────────────────────────────────────

    /// Set the current session messages.
    pub fn set_messages(
        &mut self,
        session_id: &str,
        messages: Vec<rustcode_core::session::Message>,
        parts: HashMap<String, Vec<rustcode_core::session::Part>>,
    ) {
        self.session_id = Some(session_id.to_string());
        self.conversation.set_messages(messages, parts);
    }

    /// Handle a TUI event from the server.
    pub fn handle_tui_event(&mut self, event: TuiEvent) {
        match event {
            TuiEvent::PromptAppend { properties } => {
                self.input.append(&properties.text);
            }
            TuiEvent::CommandExecute { properties } => {
                // Dispatch the command
                if let Some(action) = key_to_action(
                    crossterm::event::KeyEvent::new(
                        crossterm::event::KeyCode::Enter,
                        crossterm::event::KeyModifiers::NONE,
                    ),
                ) {
                    self.dispatch_action(action);
                }
            }
            TuiEvent::ToastShow { properties } => {
                tracing::info!(
                    "toast [{}]: {}",
                    properties.variant,
                    properties.message
                );
            }
            TuiEvent::SessionSelect { properties } => {
                self.session_id = Some(properties.session_id);
            }
        }
    }

    /// Update session status.
    pub fn set_session_status(&mut self, status: SessionStatus) {
        self.status.session_status = Some(status);
    }

    /// Set whether connected to a provider.
    pub fn set_connected(&mut self, connected: bool) {
        self.status.connected = connected;
        if connected {
            self.status.show_welcome = false;
        }
    }

    /// Update LSP/MCP counts.
    pub fn set_service_counts(&mut self, lsp: usize, mcp: usize, mcp_error: bool) {
        self.status.lsp_count = lsp;
        self.status.mcp_count = mcp;
        self.status.mcp_error = mcp_error;
    }

    /// Update permission count.
    pub fn set_permission_count(&mut self, count: usize) {
        self.status.permission_count = count;
    }

    /// Show a permission request dialog.
    pub fn show_permission(&mut self, request: rustcode_core::permission::PermissionRequest) {
        self.permission.show(request);
    }

    /// Show a question request dialog.
    pub fn show_question(&mut self, request_id: String, questions: Vec<crate::event::QuestionItem>) {
        self.question.show(request_id, questions);
    }
}
