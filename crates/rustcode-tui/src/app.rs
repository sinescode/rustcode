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
    Frame, Terminal,
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

use rustcode_core::bus::SharedBus;
use rustcode_core::provider::Provider;
use rustcode_core::session::SessionManager;

/// The main TUI application.
pub struct TuiApp {
    terminal: Terminal<ratatui::backend::CrosstermBackend<Stdout>>,

    // Component states
    conversation: ConversationState,
    input: InputState,
    status: StatusState,
    permission: PermissionState,
    question: QuestionState,

    // App state
    should_quit: bool,
    leader_active: bool,
    session_id: Option<String>,

    // Backend services
    bus: Option<SharedBus>,
    sessions: Option<Arc<SessionManager>>,
    runner: Option<Arc<rustcode_core::session_runner::SessionRunner>>,
    providers: HashMap<String, Box<dyn Provider>>,
    default_provider: Option<String>,
    default_model: Option<String>,

    // Message accumulation during streaming
    current_agent: String,
    current_model_name: String,
}

impl TuiApp {
    /// Create a new TuiApp with backend services.
    pub fn new(
        sessions: Arc<SessionManager>,
        runner: Arc<rustcode_core::session_runner::SessionRunner>,
        providers: HashMap<String, Box<dyn Provider>>,
        bus: SharedBus,
    ) -> anyhow::Result<Self> {
        enable_raw_mode()?;
        let mut stdout = io::stdout();
        execute!(stdout, EnterAlternateScreen, EnableMouseCapture)?;
        let backend = ratatui::backend::CrosstermBackend::new(stdout);
        let terminal = Terminal::new(backend)?;

        let default_provider = providers.keys().next().cloned();
        let default_model = default_provider.as_ref().and_then(|pid| {
            providers.get(pid).and_then(|_p| {
                if pid == "anthropic" { Some("claude-sonnet-4-6".into()) }
                else if pid == "openai" { Some("gpt-5.2".into()) }
                else if pid == "google" { Some("gemini-3.0-flash".into()) }
                else { None }
            })
        });

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
            bus: Some(bus),
            sessions: Some(sessions),
            runner: Some(runner),
            providers,
            default_provider,
            default_model,
            current_agent: "build".into(),
            current_model_name: String::new(),
        })
    }

    /// Run the main event loop — async with tokio.
    pub async fn run_async(&mut self) -> anyhow::Result<()> {
        let bus = self.bus.clone().expect("bus not set");
        let tick_rate = Duration::from_millis(50);

        // Welcome message
        self.status.connected = true;
        self.status.show_welcome = false;
        self.add_system_message("Welcome to rustcode! Type a message and press Enter to start.");
        if let Some(ref pid) = self.default_provider {
            self.add_system_message(&format!("Provider: {pid} | Model: {}", self.default_model.as_deref().unwrap_or("auto")));
        }

        loop {
            // Render
            self.terminal.draw(|f| self.render(f))?;

            if self.should_quit { break; }

            // Poll for crossterm events with timeout
            if event::poll(tick_rate)? {
                match event::read()? {
                    Event::Key(key) if key.kind == KeyEventKind::Press => {
                        self.handle_key_event(key);
                        if self.should_quit { break; }
                    }
                    Event::Resize(_, _) => {}
                    Event::Mouse(_) => {}
                    _ => {}
                }
            }

            // Drain bus events (non-blocking)
            if let Some(ref sessions) = self.sessions {
                // Check for session status changes
            }
        }
        Ok(())
    }

    /// Restore terminal state.
    pub fn cleanup(&mut self) -> anyhow::Result<()> {
        disable_raw_mode()?;
        execute!(self.terminal.backend_mut(), LeaveAlternateScreen, DisableMouseCapture)?;
        self.terminal.show_cursor()?;
        Ok(())
    }

    // ── Rendering ────────────────────────────────────────────────────

    fn render(&mut self, f: &mut Frame) {
        let area = f.area();
        let overlay_active = self.permission.visible || self.question.visible;

        let bg = if overlay_active { Style::default().bg(Color::Rgb(20, 20, 20)) } else { Style::default() };
        if overlay_active { f.buffer_mut().set_style(area, bg); }

        let chunks = Layout::default()
            .direction(Direction::Vertical)
            .constraints([Constraint::Min(0), Constraint::Length(3), Constraint::Length(1)])
            .split(area);

        render_conversation(f, chunks[0], &self.conversation);
        render_input(f, chunks[1], &self.input);
        render_status(f, chunks[2], &self.status);

        if self.permission.visible { render_permission(f, area, &self.permission); }
        if self.question.visible { render_question(f, area, &self.question); }
    }

    // ── Key Handling ─────────────────────────────────────────────────

    fn handle_key_event(&mut self, key: crossterm::event::KeyEvent) {
        if self.leader_active {
            self.leader_active = false;
            if let Some(action) = leader_chord_to_action(key) { self.dispatch_action(action); }
            return;
        }
        if is_leader_prefix(key) { self.leader_active = true; return; }

        if self.permission.visible {
            if let Some(reply) = self.permission.handle_key(key) { self.handle_permission_reply(reply); }
            return;
        }
        if self.question.visible {
            if let Some((rid, answers)) = self.question.handle_key(key) { self.handle_question_reply(rid, answers); }
            return;
        }

        // Input handling
        if self.input.focused {
            if self.input.handle_key(key) { return; }
            if key.code == crossterm::event::KeyCode::Enter && key.modifiers == crossterm::event::KeyModifiers::NONE {
                let text = self.input.take();
                if !text.is_empty() { self.handle_prompt_submit(text); }
                return;
            }
        }

        if let Some(action) = key_to_action(key) { self.dispatch_action(action); }
    }

    fn dispatch_action(&mut self, action: TuiAction) {
        match action {
            TuiAction::Quit => { self.should_quit = true; tracing::info!("TUI quitting"); }
            TuiAction::SessionInterrupt => {
                self.status.session_status = Some(SessionStatus::Idle);
                self.add_system_message("Interrupted.");
            }
            TuiAction::ScrollUp => self.conversation.scroll_up(1),
            TuiAction::ScrollDown => self.conversation.scroll_down(1),
            TuiAction::ScrollPageUp => {
                let h = self.terminal.get_frame().area().height / 2;
                self.conversation.scroll_up(h);
            }
            TuiAction::ScrollPageDown => {
                let h = self.terminal.get_frame().area().height / 2;
                self.conversation.scroll_down(h);
            }
            TuiAction::ScrollFirst => self.conversation.scroll_to_top(),
            TuiAction::ScrollLast => self.conversation.scroll_to_bottom(),
            TuiAction::InputClear => self.input.clear(),
            TuiAction::SessionNew => { self.session_id = None; self.conversation.messages.clear(); self.conversation.parts.clear(); self.add_system_message("New session."); }
            TuiAction::AgentCycle => {
                let agents = ["build", "plan", "general"];
                let idx = agents.iter().position(|a| *a == self.current_agent).unwrap_or(0);
                self.current_agent = agents[(idx + 1) % agents.len()].into();
                self.add_system_message(&format!("Agent: {}", self.current_agent));
            }
            TuiAction::AgentCycleReverse => {
                let agents = ["build", "plan", "general"];
                let idx = agents.iter().position(|a| *a == self.current_agent).unwrap_or(0);
                self.current_agent = agents[(idx + agents.len() - 1) % agents.len()].into();
                self.add_system_message(&format!("Agent: {}", self.current_agent));
            }
            _ => { tracing::debug!("unhandled action: {action:?}"); }
        }
    }

    // ── Prompt Submit — the core LLM call ───────────────────────────

    /// Handle prompt submission — build the prompt, call the LLM, stream results.
    fn handle_prompt_submit(&mut self, text: String) {
        tracing::info!("prompt submitted: {text}");
        self.status.session_status = Some(SessionStatus::Busy);

        // Add user message to conversation
        self.add_user_message(&text);

        // Clone what we need for the async task
        let session_id = self.session_id.clone().unwrap_or_else(|| "tui_session".into());
        let provider_id = self.default_provider.clone().unwrap_or_else(|| "anthropic".into());
        let model_name = self.default_model.clone().unwrap_or_else(|| "claude-sonnet-4-6".into());
        let agent = self.current_agent.clone();

        // Try to get provider + model, if not available just show error
        let provider_exists = self.providers.contains_key(&provider_id);
        if !provider_exists {
            self.add_error_message(&format!("No provider '{provider_id}' configured. Set the appropriate API key environment variable."));
            self.status.session_status = Some(SessionStatus::Idle);
            return;
        }

        // Build the prompt input
        let parts = vec![rustcode_core::session_prompt::PromptPart::Text(
            rustcode_core::session_prompt::PromptTextPart { id: None, text: text.clone(), synthetic: false },
        )];

        let prompt_input = rustcode_core::session_prompt::SessionPromptInput {
            session_id: session_id.clone(),
            message_id: None,
            model: Some(rustcode_core::session_info::ModelRef { id: model_name.clone(), provider_id: provider_id.clone(), variant: None }),
            agent: Some(agent.clone()),
            no_reply: false, tools: None, format: None, system: None, variant: None,
            parts,
        };

        let instructions = vec![
            "You are a helpful coding assistant running in a terminal (rustcode).".into(),
            "You have tools for reading, writing, editing, and searching code.".into(),
            "Use tools when you need to interact with the filesystem.".into(),
            "Keep responses concise. Prefer showing code over describing it.".into(),
        ];

        // Grab what we need
        let runner = self.runner.clone();
        let provider = self.providers.get(&provider_id).map(|p| {
            // We need to get an owned reference, this is tricky with Box<dyn Provider>
            // For now, we signal that the async call will happen
            provider_id.clone()
        });
        let pmodel = model_name.clone();

        // Queue the LLM call via tokio spawn
        let bus = self.bus.clone();
        let sid = session_id.clone();

        // We need to run the prompt asynchronously. Since the TUI event loop is
        // synchronous, we spawn a task and use a channel for results.
        if let Some(runner) = runner {
            let prov_id = provider_id.clone();
            let model_id = model_name.clone();

            // Clone the provider by re-fetching — we need &dyn Provider
            // For now, add a "thinking..." message and note that real streaming
            // requires the async event loop integration
            self.add_assistant_thinking();
            self.status.session_status = Some(SessionStatus::Busy);

            // Spawn the actual LLM call
            let runner_clone = runner.clone();
            let text_clone = text.clone();
            tokio::spawn(async move {
                tracing::info!("starting LLM call: {prov_id}/{model_id}");
                // The actual call requires provider access — in a full implementation,
                // this would be wired through the session runner
                tracing::info!("LLM call would be: {text_clone}");
            });

            self.add_system_message("(Full async TUI<->Provider integration requires tokio::select! in the event loop)");
        } else {
            self.add_error_message("Session runner not configured.");
        }

        self.status.session_status = Some(SessionStatus::Idle);
    }

    fn handle_permission_reply(&mut self, reply: PermissionReply) {
        match reply {
            PermissionReply::Once => tracing::info!("permission: allow once"),
            PermissionReply::Always => tracing::info!("permission: allow always"),
            PermissionReply::Reject { message } => tracing::info!("permission: reject {message:?}"),
        }
        self.permission.dismiss();
    }

    fn handle_question_reply(&mut self, request_id: String, answers: Vec<Vec<String>>) {
        if answers.is_empty() { tracing::info!("question rejected: {request_id}"); }
        else { tracing::info!("question answered: {request_id}"); }
        self.question.dismiss();
    }

    // ── Conversation helpers ─────────────────────────────────────────

    fn add_system_message(&mut self, text: &str) {
        use rustcode_core::session::{Message, MessageInfo};
        let msg = Message {
            info: MessageInfo::System(rustcode_core::session::SystemMessage { text: text.into() }),
            parts: vec![],
        };
        self.conversation.messages.push(msg);
    }

    fn add_user_message(&mut self, text: &str) {
        use rustcode_core::session::{Message, MessageInfo, UserMessage};
        let msg = Message {
            info: MessageInfo::User(UserMessage {
                model: self.default_model.clone().map(|m| rustcode_core::session_info::ModelRef {
                    id: m, provider_id: self.default_provider.clone().unwrap_or_default(), variant: None,
                }),
                agent: self.current_agent.clone(),
                text: text.into(),
                synthetic: false,
            }),
            parts: vec![rustcode_core::session::Part::UserPrompt(text.into())],
        };
        self.conversation.messages.push(msg);
    }

    fn add_assistant_thinking(&mut self) {
        use rustcode_core::session::{AssistantContent, AssistantInfo, Message, MessageInfo, MessageTime};
        let msg = Message {
            info: MessageInfo::Assistant(AssistantInfo {
                id: "thinking".into(),
                session_id: self.session_id.clone().unwrap_or_default(),
                parent_id: "user".into(),
                agent: self.current_agent.clone(),
                model_id: self.default_model.clone(),
                provider_id: None,
                variant: None,
                summary: false,
                cost: 0.0,
                tokens: Default::default(),
                finish: None,
                error: None,
                time: MessageTime { created: 0, completed: None },
            }),
            parts: vec![rustcode_core::session::Part::Text("Thinking...".into())],
        };
        self.conversation.messages.push(msg);
    }

    fn add_error_message(&mut self, text: &str) {
        use rustcode_core::session::{Message, MessageInfo, SystemMessage};
        let msg = Message {
            info: MessageInfo::System(SystemMessage { text: format!("Error: {text}") }),
            parts: vec![],
        };
        self.conversation.messages.push(msg);
    }

    // ── Public API ───────────────────────────────────────────────────

    pub fn set_messages(&mut self, session_id: &str, messages: Vec<rustcode_core::session::Message>, parts: HashMap<String, Vec<rustcode_core::session::Part>>) {
        self.session_id = Some(session_id.into());
        self.conversation.set_messages(messages, parts);
    }

    pub fn handle_tui_event(&mut self, event: TuiEvent) {
        match event {
            TuiEvent::PromptAppend { properties } => { self.input.append(&properties.text); }
            TuiEvent::CommandExecute { .. } => {}
            TuiEvent::ToastShow { properties } => {
                tracing::info!("toast [{}]: {}", properties.variant, properties.message);
            }
            TuiEvent::SessionSelect { properties } => { self.session_id = Some(properties.session_id); }
        }
    }

    pub fn set_session_status(&mut self, status: SessionStatus) { self.status.session_status = Some(status); }
    pub fn set_connected(&mut self, connected: bool) { self.status.connected = connected; if connected { self.status.show_welcome = false; } }
    pub fn set_service_counts(&mut self, lsp: usize, mcp: usize, mcp_err: bool) {
        self.status.lsp_count = lsp; self.status.mcp_count = mcp; self.status.mcp_error = mcp_err;
    }
    pub fn set_permission_count(&mut self, count: usize) { self.status.permission_count = count; }
    pub fn show_permission(&mut self, req: rustcode_core::permission::PermissionRequest) { self.permission.show(req); }
    pub fn show_question(&mut self, rid: String, qs: Vec<crate::event::QuestionItem>) { self.question.show(rid, qs); }

    pub fn get_session_id(&self) -> Option<&str> { self.session_id.as_deref() }
}
