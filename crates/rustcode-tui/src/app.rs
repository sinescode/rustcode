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
    layout::{Constraint, Direction, Layout, Rect},
    style::{Modifier, Style},
    text::{Line, Span, Text},
    widgets::{Block, Borders, Clear, List, ListItem, Paragraph, Wrap},
    Frame, Terminal,
};
use std::collections::HashMap;
use std::io::{self, Stdout};
use std::sync::Arc;
use std::time::Duration;

use crate::clipboard::copy_to_clipboard;
use crate::components::conversation::{render_conversation, ConversationState};
use crate::components::dialog::{render_backdrop, render_dialog_frame, DialogState, DialogType};
use crate::components::diff::{render_diff, DiffState};
use crate::components::export_dialog::{render_export_dialog, ExportAction, ExportState};
use crate::components::input::{render_input, InputState};
use crate::components::model_selector::{
    render_model_selector, ModelSelectorAction, ModelSelectorState,
};
use crate::components::permission::{render_permission, PermissionReply, PermissionState};
use crate::components::question::{render_question, QuestionState};
use crate::components::session_list::{
    render_session_list, SessionEntry, SessionListAction, SessionListState,
};
use crate::components::sidebar::{render_sidebar, SidebarState};
use crate::components::status::{render_status, StatusState};
use crate::components::subagent::{render_subagent_dialog, SubagentAction, SubagentState};
use crate::components::timeline::{render_timeline, TimelineAction, TimelineState};
use crate::components::toast::{render_toast, ToastState};
use crate::editor::open_in_editor;
use crate::event::{QuestionItem, TuiEvent};
use crate::keymap::{
    all_bindings, all_leader_bindings, is_leader_prefix, key_to_action, leader_chord_to_action,
    DialogTarget, TuiAction,
};
use crate::theme::ThemeState;

use rustcode_core::bus::SharedBus;
use rustcode_core::git::Git;
use rustcode_core::permission::{PermissionService, ReplyInput};
use rustcode_core::provider::{ChatMessage, LlmEvent, MessageContent, Provider, ToolDefinition};
use rustcode_core::session::{
    self, Message, MessageInfo, MessageTime, Part, PartTime, SessionManager, SessionStatus,
    TextPart,
};

// ── TUI Operating Mode ───────────────────────────────────────────────────────

/// Operating mode of the TUI.
///
/// In `Local` mode, the TUI runs against in-process backend services
/// (session manager, runner, providers, bus).
///
/// In `Remote` mode, the TUI connects to a remote rustcode server via HTTP+SSE.
/// Prompts, permissions, and questions are sent via HTTP POST; events are
/// received via the SseClient.
pub enum TuiMode {
    /// Local mode — direct access to backend services.
    Local,
    /// Remote mode — TUI connects to a remote server via HTTP+SSE.
    Remote {
        /// SSE client for receiving events from the server.
        client: Arc<crate::sse_client::SseClient>,
        /// Base URL of the remote server.
        base_url: String,
        /// HTTP client for sending commands to the server.
        http_client: reqwest::Client,
    },
}

// ── TuiApp ────────────────────────────────────────────────────────────────────

/// The main TUI application.
pub struct TuiApp {
    terminal: Option<Terminal<ratatui::backend::CrosstermBackend<Stdout>>>,

    // Component states
    conversation: ConversationState,
    input: InputState,
    status: StatusState,
    permission: PermissionState,
    question: QuestionState,
    toast: ToastState,
    dialog: DialogState,
    sidebar_state: SidebarState,
    diff: DiffState,
    session_list_state: SessionListState,

    // App state
    mode: TuiMode,
    should_quit: bool,
    leader_active: bool,
    session_id: Option<String>,

    // Backend services
    bus: Option<SharedBus>,
    sessions: Option<Arc<SessionManager>>,
    #[allow(dead_code)]
    runner: Option<Arc<rustcode_core::session_runner::SessionRunner>>,
    providers: HashMap<String, Arc<dyn Provider>>,
    default_provider: Option<String>,
    default_model: Option<String>,
    permission_service: Option<Arc<PermissionService>>,

    // Message accumulation during streaming
    current_agent: String,
    current_model_name: String,

    // ── LLM streaming state ───────────────────────────────────────
    /// Whether an LLM stream is currently active.
    is_streaming: bool,
    /// The ID of the assistant message being streamed into.
    current_assistant_msg_id: Option<String>,
    /// Accumulated text from TextDelta events.
    stream_text_buf: String,
    /// Accumulated reasoning per content block ID.
    stream_reasoning_buf: HashMap<String, String>,

    // ── Toggle flags ──────────────────────────────────────────────
    show_sidebar: bool,
    show_timestamps: bool,
    show_thinking: bool,
    show_tool_details: bool,
    conceal_enabled: bool,
    show_scrollbar: bool,
    animations_enabled: bool,
    file_context_enabled: bool,
    diff_wrap: bool,
    paste_summary: bool,
    generic_tool_output: bool,
    terminal_title: bool,

    // ── Overlay states ────────────────────────────────────────────
    command_palette_visible: bool,
    command_palette_query: String,
    command_palette_selection: usize,
    help_visible: bool,
    status_dialog_visible: bool,

    // ── New dialog states ───────────────────────────────────────
    /// Session timeline tree view.
    timeline: TimelineState,
    /// Session export dialog.
    export: ExportState,
    /// Subagent management dialog.
    subagent: SubagentState,
    /// Model selector dialog.
    model_selector: ModelSelectorState,

    // ── LLM streaming sender ─────────────────────────────────────
    /// Sender into the main loop's LLM event channel.
    /// Set in run_async() after the channel is created.
    llm_tx: Option<Arc<tokio::sync::mpsc::UnboundedSender<(String, LlmEvent)>>>,

    // ── Tool definitions (sent to LLM on each request) ────────────
    tool_definitions: Vec<ToolDefinition>,

    // ── Terminal geometry cache (fixes Bug 5) ────────────────────
    last_term_size: Rect,

    // ── Recent models list (for ModelCycleRecent) ────────────────
    recent_models: Vec<String>,

    // ── Pinned session slots (for QuickSwitch 1-9) ───────────────
    pinned_sessions: Vec<Option<String>>,

    // ── Theme system ─────────────────────────────────────────────
    /// Current theme state (8 built-in themes, dark/light mode).
    theme: ThemeState,

    // ── Audio notification ───────────────────────────────────────
    /// Whether to emit the terminal bell on stream completion.
    audio_enabled: bool,
}

impl TuiApp {
    /// Create a new TuiApp with backend services.
    pub fn new(
        _sessions: Arc<SessionManager>,
        _runner: Arc<rustcode_core::session_runner::SessionRunner>,
        providers: HashMap<String, Arc<dyn Provider>>,
        bus: SharedBus,
        tool_definitions: Vec<ToolDefinition>,
    ) -> anyhow::Result<Self> {
        enable_raw_mode()?;
        let mut stdout = io::stdout();
        execute!(stdout, EnterAlternateScreen, EnableMouseCapture)?;
        let backend = ratatui::backend::CrosstermBackend::new(stdout);
        let terminal = Terminal::new(backend)?;

        let default_provider = providers.keys().next().cloned();
        let _default_model = default_provider.as_ref().and_then(|pid| {
            providers.get(pid).and_then(|_p| {
                if pid == "anthropic" {
                    Some(String::from("claude-sonnet-4-6"))
                } else if pid == "openai" {
                    Some(String::from("gpt-5.2"))
                } else if pid == "google" {
                    Some(String::from("gemini-3.0-flash"))
                } else {
                    None
                }
            })
        });

        // Detect git branch
        let git_branch = std::env::current_dir().ok().and_then(|dir| {
            let git = Git::new(dir);
            git.branch().ok().flatten()
        });

        // Set up status with git branch
        let mut status_state = StatusState::new();
        status_state.git_branch = git_branch;

        let permission_service = Arc::new(PermissionService::new(bus.clone()));

        Ok(Self {
            terminal: Some(terminal),
            conversation: ConversationState::new(),
            input: InputState::new(),
            status: status_state,
            permission: PermissionState::new(),
            question: QuestionState::new(),
            toast: ToastState::new(),
            dialog: DialogState::new(),
            sidebar_state: SidebarState::new(),
            diff: DiffState::new(),
            session_list_state: SessionListState::new(),
            mode: TuiMode::Local,
            should_quit: false,
            leader_active: false,
            session_id: None,
            bus: None,
            sessions: None,
            runner: None,
            providers,
            default_provider: None,
            default_model: None,
            permission_service: Some(permission_service),
            current_agent: "build".into(),
            current_model_name: String::new(),
            // Streaming state
            is_streaming: false,
            current_assistant_msg_id: None,
            stream_text_buf: String::new(),
            stream_reasoning_buf: HashMap::new(),
            // Toggles
            show_sidebar: false,
            show_timestamps: true,
            show_thinking: true,
            show_tool_details: true,
            conceal_enabled: false,
            show_scrollbar: true,
            animations_enabled: true,
            file_context_enabled: false,
            diff_wrap: true,
            paste_summary: true,
            generic_tool_output: false,
            terminal_title: true,
            // Overlays
            command_palette_visible: false,
            command_palette_query: String::new(),
            command_palette_selection: 0,
            help_visible: false,
            status_dialog_visible: false,
            // New dialog states
            timeline: TimelineState::new(),
            export: ExportState::new(),
            subagent: SubagentState::new(),
            model_selector: ModelSelectorState::new(),
            // LLM streaming
            llm_tx: None,
            // Tool definitions
            tool_definitions,
            // Terminal geometry cache
            last_term_size: Rect::default(),
            // Recent models
            recent_models: Vec::new(),
            // Pinned sessions (slots 0-8 for keys 1-9)
            pinned_sessions: vec![None; 9],
            // Theme system
            theme: ThemeState::new(),
            // Audio notification
            audio_enabled: true,
        })
    }

    /// Create a new TuiApp in Remote mode, connected to a server via HTTP+SSE.
    ///
    /// Events are received through the SseClient; prompts and replies are sent
    /// via HTTP POST to the remote server.
    pub fn new_remote(
        sse_client: Arc<crate::sse_client::SseClient>,
        base_url: String,
        http_client: reqwest::Client,
    ) -> anyhow::Result<Self> {
        enable_raw_mode()?;
        let mut stdout = io::stdout();
        execute!(stdout, EnterAlternateScreen, EnableMouseCapture)?;
        let backend = ratatui::backend::CrosstermBackend::new(stdout);
        let terminal = Terminal::new(backend)?;

        let mut status_state = StatusState::new();
        status_state.connected = false; // Will update once SSE connects

        Ok(Self {
            terminal: Some(terminal),
            conversation: ConversationState::new(),
            input: InputState::new(),
            status: status_state,
            permission: PermissionState::new(),
            question: QuestionState::new(),
            toast: ToastState::new(),
            dialog: DialogState::new(),
            sidebar_state: SidebarState::new(),
            diff: DiffState::new(),
            session_list_state: SessionListState::new(),
            mode: TuiMode::Remote {
                client: sse_client,
                base_url,
                http_client,
            },
            should_quit: false,
            leader_active: false,
            session_id: None,
            bus: None,
            sessions: None,
            runner: None,
            providers: HashMap::new(),
            default_provider: None,
            default_model: None,
            permission_service: None,
            current_agent: "build".into(),
            current_model_name: String::new(),
            // Streaming state
            is_streaming: false,
            current_assistant_msg_id: None,
            stream_text_buf: String::new(),
            stream_reasoning_buf: HashMap::new(),
            // Toggles
            show_sidebar: false,
            show_timestamps: true,
            show_thinking: true,
            show_tool_details: true,
            conceal_enabled: false,
            show_scrollbar: true,
            animations_enabled: true,
            file_context_enabled: false,
            diff_wrap: true,
            paste_summary: true,
            generic_tool_output: false,
            terminal_title: true,
            // Overlays
            command_palette_visible: false,
            command_palette_query: String::new(),
            command_palette_selection: 0,
            help_visible: false,
            status_dialog_visible: false,
            // New dialog states
            timeline: TimelineState::new(),
            export: ExportState::new(),
            subagent: SubagentState::new(),
            model_selector: ModelSelectorState::new(),
            // LLM streaming
            llm_tx: None,
            // Tool definitions
            tool_definitions: Vec::new(),
            // Terminal geometry cache
            last_term_size: Rect::default(),
            // Recent models
            recent_models: Vec::new(),
            // Pinned sessions (slots 0-8 for keys 1-9)
            pinned_sessions: vec![None; 9],
            // Theme system
            theme: ThemeState::new(),
            // Audio notification
            audio_enabled: true,
        })
    }

    /// Run the main event loop — async with tokio::select!.
    ///
    /// Handles three concurrent event sources:
    /// 1. crossterm keyboard events (from a spawn_blocking task)
    /// 2. bus events (from the SharedBus subscription) — local mode only
    /// 3. LLM stream events (from Provider::stream(), when active) — local mode only
    /// 4. SSE client events — remote mode only
    pub async fn run_async(&mut self) -> anyhow::Result<()> {
        // Set initial status
        self.status.connected = true;
        self.status.show_welcome = false;
        if let Some(ref provider) = self.default_provider {
            self.status.provider_name = Some(provider.clone());
        }
        if let Some(ref model) = self.default_model {
            self.status.model_name = Some(model.clone());
        }
        self.input.agent_name = self.current_agent.clone();

        // ── Channel for crossterm events ──────────────────────────
        let (event_tx, mut event_rx) =
            tokio::sync::mpsc::unbounded_channel::<crossterm::event::Event>();

        tokio::task::spawn_blocking(move || loop {
            if let Ok(true) = event::poll(Duration::from_millis(10)) {
                match event::read() {
                    Ok(Event::Key(key))
                        if key.kind == KeyEventKind::Press
                            && event_tx.send(Event::Key(key)).is_err() =>
                    {
                        break;
                    }
                    Ok(Event::Resize(w, h)) if event_tx.send(Event::Resize(w, h)).is_err() => {
                        break;
                    }
                    Ok(Event::Mouse(mouse)) => {
                        let _ = event_tx.send(Event::Mouse(mouse));
                    }
                    _ => {}
                }
            }
        });

        // ── Event sources: branch on mode ──────────────────────────────
        // Local: bus + LLM stream channels
        // Remote: SSE client subscription
        let mut bus_rx: Option<
            tokio::sync::mpsc::UnboundedReceiver<rustcode_core::bus::GlobalEvent>,
        > = None;
        let mut llm_rx: Option<
            tokio::sync::mpsc::UnboundedReceiver<(String, rustcode_core::provider::LlmEvent)>,
        > = None;
        let mut sse_rx: Option<tokio::sync::broadcast::Receiver<TuiEvent>> = None;

        let is_remote = matches!(self.mode, TuiMode::Remote { .. });

        if is_remote {
            // ── Remote mode: subscribe to SseClient ─────────────────
            if let TuiMode::Remote { ref client, .. } = self.mode {
                sse_rx = Some(client.subscribe());
            }
            self.status.connected = true;
            self.status.provider_name = Some("remote".into());
            self.status.model_name = Some("remote".into());
        } else {
            // ── Local mode: bus + LLM stream channels ──────────────
            let bus = self.bus.clone().expect("bus not set");
            let mut bus_sub = bus.subscribe();
            let (bus_tx, local_bus_rx) =
                tokio::sync::mpsc::unbounded_channel::<rustcode_core::bus::GlobalEvent>();
            bus_rx = Some(local_bus_rx);

            tokio::spawn(async move {
                while let Some(event) = bus_sub.recv().await {
                    if bus_tx.send(event).is_err() {
                        break;
                    }
                }
            });

            let (llm_tx, local_llm_rx) = tokio::sync::mpsc::unbounded_channel::<(
                String,
                rustcode_core::provider::LlmEvent,
            )>();
            llm_rx = Some(local_llm_rx);
            let llm_tx = Arc::new(llm_tx);
            self.llm_tx = Some(llm_tx);
        }

        // ── Main loop ─────────────────────────────────────────────
        let render_interval = Duration::from_millis(50);
        let mut last_draw = tokio::time::Instant::now();

        // Take terminal out of self to avoid borrow conflict in the draw closure
        let mut terminal = self.terminal.take().expect("terminal not initialized");

        loop {
            terminal
                .draw(|f| self.render(f))
                .expect("terminal draw failed");

            if self.should_quit {
                break;
            }

            // Update permission count (local mode only)
            if let Some(ref perm_svc) = self.permission_service {
                let pending = perm_svc.list();
                self.status.permission_count = pending.len();
            }

            // Tick placeholder cycling
            self.input.tick_placeholder();

            // Compute sleep until next scheduled render
            let elapsed = last_draw.elapsed();
            let sleep_dur = if elapsed < render_interval {
                render_interval - elapsed
            } else {
                Duration::from_millis(0)
            };

            let next_draw = tokio::time::sleep(sleep_dur);
            tokio::pin!(next_draw);

            // Remote mode event loop
            if is_remote {
                if let Some(ref mut sse) = sse_rx {
                    tokio::select! {
                        biased;

                        Some(evt) = event_rx.recv() => {
                            if let Event::Key(key) = evt {
                                self.handle_key_event(key);
                            }
                        }

                        Ok(tui_evt) = sse.recv() => {
                            self.handle_tui_event(tui_evt);
                        }

                        _ = &mut next_draw => {}
                    }
                } else {
                    // No SSE subscription — just keyboard
                    tokio::select! {
                        biased;

                        Some(evt) = event_rx.recv() => {
                            if let Event::Key(key) = evt {
                                self.handle_key_event(key);
                            }
                        }

                        _ = &mut next_draw => {}
                    }
                }
            } else if self.is_streaming {
                tokio::select! {
                    biased;

                    Some(evt) = event_rx.recv() => {
                        if let Event::Key(key) = evt {
                            self.handle_key_event(key);
                        }
                    }

                    Some(bus_evt) = bus_rx.as_mut().expect("bus_rx set").recv() => {
                        self.handle_bus_event(bus_evt);
                    }

                    opt = llm_rx.as_mut().expect("llm_rx set").recv() => {
                        match opt {
                            Some((msg_id, llm_evt)) => {
                                self.apply_llm_event(&msg_id, llm_evt);
                            }
                            None => {
                                self.finalize_stream();
                            }
                        }
                    }

                    _ = &mut next_draw => {}
                }
            } else {
                tokio::select! {
                    biased;

                    Some(evt) = event_rx.recv() => {
                        if let Event::Key(key) = evt {
                            self.handle_key_event(key);
                        }
                    }

                    Some(bus_evt) = bus_rx.as_mut().expect("bus_rx set").recv() => {
                        self.handle_bus_event(bus_evt);
                    }

                    _ = &mut next_draw => {}
                }
            }

            last_draw = tokio::time::Instant::now();
        }

        // Put the terminal back before cleanup
        self.terminal = Some(terminal);

        Ok(())
    }

    /// Restore terminal state.
    pub fn cleanup(&mut self) -> anyhow::Result<()> {
        disable_raw_mode()?;
        let terminal = self.terminal.as_mut().expect("terminal not initialized");
        execute!(
            terminal.backend_mut(),
            LeaveAlternateScreen,
            DisableMouseCapture
        )?;
        terminal.show_cursor()?;
        Ok(())
    }

    // ── LLM Streaming ─────────────────────────────────────────────

    /// Spawn the Provider::stream() call in a tokio task.
    ///
    /// Builds chat messages from the user prompt, creates a placeholder
    /// assistant message, then streams LLM events through the stored `llm_tx`.
    /// The main loop picks them up via `apply_llm_event()`.
    fn spawn_llm_stream(&mut self, text: String) {
        let provider_id = self
            .default_provider
            .clone()
            .unwrap_or_else(|| "anthropic".into());
        let model_id = self
            .default_model
            .clone()
            .unwrap_or_else(|| "claude-sonnet-4-20250514".into());
        let agent = self.current_agent.clone();
        let session_id = self
            .session_id
            .clone()
            .unwrap_or_else(|| format!("ses_tui_{}", chrono::Utc::now().timestamp_millis()));

        self.session_id = Some(session_id.clone());
        self.is_streaming = true;

        let provider = match self.providers.get(&provider_id) {
            Some(p) => Arc::clone(p),
            None => {
                self.conversation
                    .add_system_message(format!("Error: provider '{provider_id}' not found"));
                self.is_streaming = false;
                self.current_assistant_msg_id = None;
                self.status.session_status = Some(SessionStatus::Idle);
                return;
            }
        };

        let llm_tx = match self.llm_tx.clone() {
            Some(tx) => tx,
            None => {
                self.conversation
                    .add_system_message("Error: LLM channel not initialized".into());
                self.is_streaming = false;
                self.status.session_status = Some(SessionStatus::Idle);
                return;
            }
        };

        // Build chat messages from the conversation state plus new user prompt.
        // We always include a system instruction and the user's message.
        let instructions = [
            "You are a helpful coding assistant running in a terminal (rustcode).".to_string(),
            "You have tools for reading, writing, editing, and searching code.".to_string(),
            "Use tools when you need to interact with the filesystem.".to_string(),
            "Keep responses concise. Prefer showing code over describing it.".to_string(),
        ];
        let system_prompt = instructions.join("\n");

        let mut chat_messages: Vec<ChatMessage> = vec![ChatMessage::System {
            content: MessageContent::Text(system_prompt),
        }];

        // Include recent conversation context (last few messages) for multi-turn
        for msg in self.conversation.messages.iter().rev().take(6).rev() {
            match &msg.info {
                MessageInfo::User(_) => {
                    let text = msg
                        .parts
                        .iter()
                        .filter_map(|p| {
                            if let Part::Text(t) = p {
                                Some(t.text.as_str())
                            } else {
                                None
                            }
                        })
                        .collect::<Vec<_>>()
                        .join("\n");
                    if !text.is_empty() {
                        chat_messages.push(ChatMessage::User {
                            content: MessageContent::Text(text),
                        });
                    }
                }
                MessageInfo::Assistant(_) => {
                    let text = msg
                        .parts
                        .iter()
                        .filter_map(|p| {
                            if let Part::Text(t) = p {
                                Some(t.text.as_str())
                            } else {
                                None
                            }
                        })
                        .collect::<Vec<_>>()
                        .join("\n");
                    if !text.is_empty() {
                        chat_messages.push(ChatMessage::Assistant {
                            content: MessageContent::Text(text),
                        });
                    }
                }
            }
        }

        // Ensure the current prompt is the last user message
        chat_messages.push(ChatMessage::User {
            content: MessageContent::Text(text.clone()),
        });

        // Use stored tool definitions
        let tool_definitions = self.tool_definitions.clone();

        // Create placeholder assistant message
        let now = chrono::Utc::now().timestamp_millis() as u64;
        let assistant_msg_id = format!("msg_asst_{now}");
        self.current_assistant_msg_id = Some(assistant_msg_id.clone());
        self.stream_text_buf.clear();
        self.stream_reasoning_buf.clear();

        self.conversation.messages.push(Message {
            info: MessageInfo::Assistant(session::AssistantInfo {
                id: assistant_msg_id.clone(),
                session_id: session_id.clone(),
                parent_id: format!("msg_user_{now}"),
                agent: agent.clone(),
                model_id: Some(model_id.clone()),
                provider_id: Some(provider_id.clone()),
                variant: None,
                summary: false,
                cost: 0.0,
                tokens: Default::default(),
                finish: None,
                error: None,
                time: MessageTime {
                    created: now,
                    completed: None,
                },
            }),
            parts: vec![Part::Text(TextPart {
                id: format!("part_text_{now}"),
                message_id: assistant_msg_id.clone(),
                session_id: session_id.clone(),
                text: String::new(),
                metadata: None,
                time: PartTime {
                    start: Some(now),
                    end: None,
                },
            })],
        });

        // Spawn the streaming task
        tokio::spawn(async move {
            let model = match provider.get_model(&model_id).await {
                Ok(m) => m,
                Err(e) => {
                    let _ = llm_tx.send((
                        assistant_msg_id,
                        LlmEvent::ProviderErrorEvent {
                            message: format!("Failed to get model: {e}"),
                            classification: Some("model-error".into()),
                            retryable: Some(false),
                            provider_metadata: None,
                        },
                    ));
                    return;
                }
            };

            let stream_result = provider
                .stream(&model, &chat_messages, &tool_definitions)
                .await;
            let mut stream = match stream_result {
                Ok(s) => s,
                Err(e) => {
                    let _ = llm_tx.send((
                        assistant_msg_id,
                        LlmEvent::ProviderErrorEvent {
                            message: format!("Stream error: {e}"),
                            classification: Some("stream-error".into()),
                            retryable: Some(false),
                            provider_metadata: None,
                        },
                    ));
                    return;
                }
            };

            use futures::StreamExt;
            while let Some(result) = stream.next().await {
                let event = match result {
                    Ok(evt) => evt,
                    Err(e) => LlmEvent::ProviderErrorEvent {
                        message: format!("Stream error: {e}"),
                        classification: Some("stream-error".into()),
                        retryable: Some(false),
                        provider_metadata: None,
                    },
                };
                if llm_tx.send((assistant_msg_id.clone(), event)).is_err() {
                    break;
                }
            }
        });
    }

    /// Apply an LLM streaming event to the conversation in real time.
    fn apply_llm_event(&mut self, msg_id: &str, event: LlmEvent) {
        match event {
            LlmEvent::TextDelta { text, .. } => {
                self.stream_text_buf.push_str(&text);
                let acc = self.stream_text_buf.clone();
                if let Some(msg) = self
                    .conversation
                    .messages
                    .iter_mut()
                    .rev()
                    .find(|m| m.info.id() == msg_id)
                {
                    for part in &mut msg.parts {
                        if let Part::Text(ref mut tp) = part {
                            tp.text = acc;
                            return;
                        }
                    }
                    let now = chrono::Utc::now().timestamp_millis() as u64;
                    let sid = self.session_id.clone().unwrap_or_default();
                    msg.parts.push(Part::Text(TextPart {
                        id: format!("part_text_{now}"),
                        message_id: msg_id.to_string(),
                        session_id: sid,
                        text: acc,
                        metadata: None,
                        time: PartTime {
                            start: Some(now),
                            end: None,
                        },
                    }));
                }
            }

            LlmEvent::ReasoningDelta { id, text, .. } => {
                let acc = self.stream_reasoning_buf.entry(id.clone()).or_default();
                acc.push_str(&text);
                let reasoning_text = acc.clone();
                if let Some(msg) = self
                    .conversation
                    .messages
                    .iter_mut()
                    .rev()
                    .find(|m| m.info.id() == msg_id)
                {
                    for part in &mut msg.parts {
                        if let Part::Reasoning(ref mut rp) = part {
                            if rp.id == id {
                                rp.text = reasoning_text;
                                return;
                            }
                        }
                    }
                    let now = chrono::Utc::now().timestamp_millis() as u64;
                    let sid = self.session_id.clone().unwrap_or_default();
                    msg.parts.push(Part::Reasoning(session::ReasoningPart {
                        id: id.clone(),
                        message_id: msg_id.to_string(),
                        session_id: sid,
                        text: reasoning_text,
                        metadata: None,
                        time: PartTime {
                            start: Some(now),
                            end: None,
                        },
                    }));
                }
            }

            LlmEvent::ToolCall {
                id, name, input, ..
            } => {
                let sid = self.session_id.clone().unwrap_or_default();
                let tool_part = Part::Tool(session::ToolPart {
                    id: id.clone(),
                    message_id: msg_id.to_string(),
                    session_id: sid,
                    tool: name.clone(),
                    call_id: id.clone(),
                    state: session::ToolState::Pending {
                        input: input.clone(),
                    },
                    metadata: None,
                });
                if let Some(msg) = self
                    .conversation
                    .messages
                    .iter_mut()
                    .rev()
                    .find(|m| m.info.id() == msg_id)
                {
                    msg.parts.push(tool_part);
                }
                self.conversation
                    .add_system_message(format!("Tool call: {name}"));
            }

            LlmEvent::ToolResult {
                id, name, result, ..
            } => {
                let now = chrono::Utc::now().timestamp_millis() as u64;
                if let Some(msg) = self
                    .conversation
                    .messages
                    .iter_mut()
                    .rev()
                    .find(|m| m.info.id() == msg_id)
                {
                    for part in &mut msg.parts {
                        if let Part::Tool(ref mut tp) = part {
                            if tp.call_id == id {
                                let output = result
                                    .as_str()
                                    .map(|s| s.to_string())
                                    .unwrap_or_else(|| result.to_string());
                                let title = result
                                    .get("title")
                                    .and_then(|t| t.as_str())
                                    .unwrap_or(&name)
                                    .to_string();
                                tp.state = session::ToolState::Completed {
                                    input: serde_json::Value::Null,
                                    output,
                                    title,
                                    metadata: serde_json::Value::Null,
                                    time: session::ToolTime {
                                        start: now,
                                        end: Some(now),
                                    },
                                    attachments: None,
                                };
                                break;
                            }
                        }
                    }
                }
            }

            LlmEvent::ToolError {
                id,
                name: _,
                message,
                ..
            } => {
                let now = chrono::Utc::now().timestamp_millis() as u64;
                if let Some(msg) = self
                    .conversation
                    .messages
                    .iter_mut()
                    .rev()
                    .find(|m| m.info.id() == msg_id)
                {
                    for part in &mut msg.parts {
                        if let Part::Tool(ref mut tp) = part {
                            if tp.call_id == id {
                                tp.state = session::ToolState::Error {
                                    input: serde_json::Value::Null,
                                    error: message.clone(),
                                    time: session::ToolTime {
                                        start: now,
                                        end: Some(now),
                                    },
                                    metadata: None,
                                };
                                break;
                            }
                        }
                    }
                }
            }

            LlmEvent::StepStart { index } => {
                let now = chrono::Utc::now().timestamp_millis() as u64;
                if let Some(msg) = self
                    .conversation
                    .messages
                    .iter_mut()
                    .rev()
                    .find(|m| m.info.id() == msg_id)
                {
                    let sid = self.session_id.clone().unwrap_or_default();
                    msg.parts.push(Part::StepStart(session::StepStartPart {
                        id: format!("step_start_{index}_{now}"),
                        message_id: msg_id.to_string(),
                        session_id: sid,
                        snapshot: None,
                    }));
                }
            }

            LlmEvent::StepFinish {
                index,
                reason,
                usage,
                ..
            } => {
                let now = chrono::Utc::now().timestamp_millis() as u64;
                let tokens = usage.map(|u| session::TokenUsage {
                    input: u.input_tokens.unwrap_or(0),
                    output: u.output_tokens.unwrap_or(0),
                    reasoning: u.reasoning_tokens.unwrap_or(0),
                    cache: session::CacheUsage {
                        read: u.cache_read_input_tokens.unwrap_or(0),
                        write: u.cache_write_input_tokens.unwrap_or(0),
                    },
                });
                if let Some(msg) = self
                    .conversation
                    .messages
                    .iter_mut()
                    .rev()
                    .find(|m| m.info.id() == msg_id)
                {
                    let sid = self.session_id.clone().unwrap_or_default();
                    msg.parts.push(Part::StepFinish(session::StepFinishPart {
                        id: format!("step_finish_{index}_{now}"),
                        message_id: msg_id.to_string(),
                        session_id: sid,
                        reason: format!("{reason:?}"),
                        tokens: tokens.unwrap_or_default(),
                        cost: 0.0,
                        snapshot: None,
                    }));
                }
            }

            LlmEvent::Finish { reason, usage, .. } => {
                let now = chrono::Utc::now().timestamp_millis() as u64;
                if let Some(msg) = self
                    .conversation
                    .messages
                    .iter_mut()
                    .rev()
                    .find(|m| m.info.id() == msg_id)
                {
                    if let MessageInfo::Assistant(ref mut info) = msg.info {
                        info.finish = Some(format!("{reason:?}"));
                        info.time.completed = Some(now);
                        if let Some(ref u) = usage {
                            info.tokens = session::TokenUsage {
                                input: u.input_tokens.unwrap_or(0),
                                output: u.output_tokens.unwrap_or(0),
                                reasoning: u.reasoning_tokens.unwrap_or(0),
                                cache: session::CacheUsage {
                                    read: u.cache_read_input_tokens.unwrap_or(0),
                                    write: u.cache_write_input_tokens.unwrap_or(0),
                                },
                            };
                        }
                    }
                }
            }

            LlmEvent::ProviderErrorEvent {
                message,
                classification,
                ..
            } => {
                let class = classification.unwrap_or_else(|| "error".into());
                self.conversation
                    .add_system_message(format!("Provider error [{class}]: {message}"));
                if let Some(msg) = self
                    .conversation
                    .messages
                    .iter_mut()
                    .rev()
                    .find(|m| m.info.id() == msg_id)
                {
                    if let MessageInfo::Assistant(ref mut info) = msg.info {
                        info.error =
                            Some(serde_json::json!({"message": message, "classification": class}));
                    }
                }
            }

            // Tool input streaming events: intermediate — ToolCall has assembled input
            LlmEvent::TextStart { .. }
            | LlmEvent::TextEnd { .. }
            | LlmEvent::ReasoningStart { .. }
            | LlmEvent::ReasoningEnd { .. }
            | LlmEvent::ToolInputStart { .. }
            | LlmEvent::ToolInputDelta { .. }
            | LlmEvent::ToolInputEnd { .. } => {}
        }
    }

    /// Finalize the streaming session when the LLM channel closes.
    fn finalize_stream(&mut self) {
        self.is_streaming = false;
        self.current_assistant_msg_id = None;
        self.stream_text_buf.clear();
        self.stream_reasoning_buf.clear();
        self.status.session_status = Some(SessionStatus::Idle);
        self.conversation
            .add_system_message("Response complete.".into());

        // Emit terminal bell for audio notification on stream completion.
        if self.audio_enabled {
            // Print ASCII bell character (\x07) — most terminals play a sound.
            // We write directly to stderr to avoid interfering with the TUI
            // rendering on stdout.
            use std::io::Write;
            let _ = std::io::stderr().write_all(b"\x07");
            let _ = std::io::stderr().flush();
            tracing::debug!("audio notification: bell emitted");
        }
    }

    /// Handle a bus event from the SharedBus subscription.
    ///
    /// Dispatches based on payload shape:
    /// - PermissionRequest (has `permission` + `patterns` fields)
    /// - `session.*` type-tagged events
    /// - `question.*` type-tagged events
    /// - `toast.*` type-tagged events
    /// - `prompt.append` type-tagged events
    fn handle_bus_event(&mut self, event: rustcode_core::bus::GlobalEvent) {
        let payload = &event.payload;

        // ── Permission request (published by PermissionService) ─────
        // PermissionRequest has: id, sessionID, permission, patterns, metadata
        if payload.get("permission").and_then(|v| v.as_str()).is_some()
            && payload.get("patterns").is_some()
        {
            match serde_json::from_value::<rustcode_core::permission::PermissionRequest>(
                payload.clone(),
            ) {
                Ok(request) => {
                    tracing::info!(
                        id = %request.id,
                        permission = %request.permission,
                        "permission asked via bus"
                    );
                    self.show_permission(request);
                    return;
                }
                Err(e) => {
                    tracing::warn!("failed to parse bus PermissionRequest: {e}");
                }
            }
        }

        // ── Type-tagged events ──────────────────────────────────────
        let event_type = payload.get("type").and_then(|v| v.as_str()).unwrap_or("");

        match event_type {
            // ── Session events ──────────────────────────────────
            "session.deleted" => {
                let sid = payload
                    .get("sessionID")
                    .and_then(|v| v.as_str())
                    .unwrap_or("");
                tracing::info!("session deleted via bus: {sid}");
                // If our current session was deleted, reset to home
                if self.session_id.as_deref() == Some(sid) || sid.is_empty() {
                    self.session_id = None;
                    self.conversation.messages.clear();
                    self.conversation.parts.clear();
                    self.status.cost = 0.0;
                    self.status.token_count = None;
                    self.add_system_message("Session was deleted. New session.");
                }
            }

            "session.status.changed" => {
                if let Some(status_val) = payload.get("status") {
                    if let Ok(status) = serde_json::from_value::<SessionStatus>(status_val.clone())
                    {
                        tracing::info!(?status, "session status changed via bus");
                        self.status.session_status = Some(status);
                    }
                }
            }

            // ── Question events ─────────────────────────────────
            "question.asked" => {
                if let Some(rid) = payload.get("requestID").and_then(|v| v.as_str()) {
                    if let Some(qs) = payload.get("questions") {
                        if let Ok(questions) =
                            serde_json::from_value::<Vec<QuestionItem>>(qs.clone())
                        {
                            tracing::info!(%rid, "question asked via bus");
                            self.show_question(rid.to_string(), questions);
                        }
                    }
                }
            }

            // ── Toast events ────────────────────────────────────
            "toast.show" | "tui.toast.show" => {
                let title = payload
                    .get("title")
                    .and_then(|v| v.as_str())
                    .map(String::from);
                let message = payload
                    .get("message")
                    .and_then(|v| v.as_str())
                    .unwrap_or("")
                    .to_string();
                let variant = payload
                    .get("variant")
                    .and_then(|v| v.as_str())
                    .unwrap_or("info")
                    .to_string();
                tracing::info!("toast [{}]: {}", variant, message);
                // In a full impl, we'd show a toast overlay.
                // For now, add a system message.
                let prefix = match variant.as_str() {
                    "error" => "[ERROR]",
                    "warning" => "[WARN]",
                    "success" => "[OK]",
                    _ => "[INFO]",
                };
                if let Some(t) = title {
                    self.add_system_message(&format!("{prefix} {t}: {message}"));
                } else {
                    self.add_system_message(&format!("{prefix} {message}"));
                }
            }

            // ── Prompt append ───────────────────────────────────
            "prompt.append" | "tui.prompt.append" => {
                if let Some(text) = payload.get("text").and_then(|v| v.as_str()) {
                    tracing::info!("prompt append via bus: {text}");
                    self.input.append(text);
                }
            }

            // ── TUI command execution ────────────────────────────
            "tui.command.execute" => {
                if let Some(cmd) = payload.get("command").and_then(|v| v.as_str()) {
                    tracing::info!("tui command execute via bus: {cmd}");
                    match cmd {
                        "quit" => self.should_quit = true,
                        "help" => self.help_visible = !self.help_visible,
                        "sidebar" => self.show_sidebar = !self.show_sidebar,
                        "interrupt" => {
                            self.status.session_status = Some(SessionStatus::Idle);
                            self.is_streaming = false;
                            self.add_system_message("Interrupted via bus.");
                        }
                        _ => {
                            self.add_system_message(&format!("Command from server: {cmd}"));
                        }
                    }
                }
            }

            // ── TUI session navigation ───────────────────────────
            "tui.session.select" => {
                if let Some(sid) = payload.get("session_id").and_then(|v| v.as_str()) {
                    tracing::info!("tui session select via bus: {sid}");
                    self.session_id = Some(sid.to_string());
                    self.add_system_message(&format!("Navigated to session {sid}."));
                }
            }

            // ── TUI overlay open events ──────────────────────────
            "tui.open.help" => {
                tracing::info!("tui open help via bus");
                self.help_visible = true;
            }
            "tui.open.sessions" => {
                tracing::info!("tui open sessions via bus");
                self.add_system_message("Sessions list requested from server.");
            }
            "tui.open.themes" => {
                tracing::info!("tui open themes via bus");
                self.add_system_message("Themes picker requested from server.");
            }
            "tui.open.models" => {
                tracing::info!("tui open models via bus");
                self.add_system_message("Models list requested from server.");
            }
            "tui.prompt.submit" => {
                tracing::info!("tui prompt submit via bus");
                let text = self.input.take();
                if !text.is_empty() {
                    self.input.add_to_history(&text);
                    self.handle_prompt_submit(text);
                }
            }
            "tui.prompt.clear" => {
                tracing::info!("tui prompt clear via bus");
                self.input.clear();
            }

            // ── Session updated ─────────────────────────────────
            "session.updated" => {
                let sid = payload
                    .get("sessionID")
                    .and_then(|v| v.as_str())
                    .unwrap_or("");
                tracing::info!("session updated via bus: {sid}");
            }

            // ── Message/part streaming events ────────────────────
            "message.updated" => {
                let sid = payload
                    .get("sessionID")
                    .and_then(|v| v.as_str())
                    .unwrap_or("");
                tracing::info!("message updated via bus: session={sid}");
            }
            "part.updated" => {
                tracing::info!("part updated via bus");
            }
            "part.delta" => {
                if let Some(delta) = payload.get("delta").and_then(|v| v.as_str()) {
                    let msg_id = payload
                        .get("messageID")
                        .and_then(|v| v.as_str())
                        .unwrap_or("");
                    if let Some(msg) = self
                        .conversation
                        .messages
                        .iter_mut()
                        .rev()
                        .find(|m| m.info.id() == msg_id)
                    {
                        for part in &mut msg.parts {
                            if let Part::Text(ref mut tp) = part {
                                tp.text.push_str(delta);
                                break;
                            }
                        }
                    }
                }
            }
            "todo.updated" => {
                let sid = payload
                    .get("sessionID")
                    .and_then(|v| v.as_str())
                    .unwrap_or("");
                tracing::info!("todo updated via bus: session={sid}");
            }

            // ── Unknown / unhandled ─────────────────────────────
            other => {
                if !other.is_empty() {
                    tracing::debug!(%other, "bus event received (unhandled type)");
                } else {
                    tracing::debug!("bus event received (no type tag, not a permission request)");
                }
            }
        }
    }

    // ── Rendering ────────────────────────────────────────────────────

    fn render(&mut self, f: &mut Frame) {
        let area = f.size();
        // Cache terminal size for scroll handlers that need it outside draw
        self.last_term_size = area;

        // Tick toast expiry and update sidebar data
        self.toast.tick();
        self.sync_sidebar_state();

        let theme_bg = self.theme.current().background;
        let bg = Style::default().bg(theme_bg);
        f.buffer_mut().set_style(area, bg);

        // Main layout: enhanced sidebar? + [conversation | input | status]
        let main_area = if self.sidebar_state.visible {
            let sidebar_width = self.sidebar_state.width.min(area.width / 3);
            let cols = Layout::default()
                .direction(Direction::Horizontal)
                .constraints([Constraint::Length(sidebar_width), Constraint::Min(0)])
                .split(area);
            render_sidebar(f, cols[0], &self.sidebar_state, self.theme.current());
            cols[1]
        } else if self.show_sidebar {
            // Legacy simple sidebar (fallback)
            let cols = Layout::default()
                .direction(Direction::Horizontal)
                .constraints([Constraint::Percentage(20), Constraint::Percentage(80)])
                .split(area);
            self.render_legacy_sidebar(f, cols[0]);
            cols[1]
        } else {
            area
        };

        let chunks = Layout::default()
            .direction(Direction::Vertical)
            .constraints([
                Constraint::Min(0),
                Constraint::Length(3),
                Constraint::Length(1),
            ])
            .split(main_area);

        render_conversation(f, chunks[0], &self.conversation, self.theme.current());
        render_input(f, chunks[1], &self.input, self.theme.current());
        render_status(f, chunks[2], &self.status, self.theme.current());

        // Overlays (render order matters — later = on top)
        if self.permission.visible {
            render_permission(f, area, &self.permission);
        }
        if self.question.visible {
            render_question(f, area, &self.question);
        }
        if self.command_palette_visible {
            self.render_command_palette(f, area);
        }
        if self.help_visible {
            self.render_help(f, area);
        }
        if self.status_dialog_visible {
            self.render_status_dialog(f, area);
        }

        // Dialog stack overlay
        if self.dialog.is_active() {
            render_backdrop(f, area);
            if let Some(top) = self.dialog.top() {
                let dialog_type = top.dialog_type;
                // For dialogs driven by the dialog stack (not their own state),
                // we push the corresponding state into visible mode and render.
                // Dialog types with their own full state management (timeline, export,
                // subagent, model_selector) are rendered separately above.
                let _inner = render_dialog_frame(f, area, dialog_type, 0);

                // For dialog types that need content, delegate to state-based renderers
                // but only if they aren't already showing via their own visibility flag.
                match dialog_type {
                    DialogType::Timeline => {
                        // Timeline uses its own state, but can also be triggered via stack
                        if !self.timeline.visible {
                            self.timeline.visible = true;
                        }
                    }
                    DialogType::Export => {
                        if !self.export.visible {
                            self.export.visible = true;
                        }
                    }
                    DialogType::Subagent => {
                        if !self.subagent.visible {
                            self.subagent.visible = true;
                        }
                    }
                    DialogType::ModelSelector => {
                        if !self.model_selector.visible {
                            // Populate model selector from providers
                            let provider_map: std::collections::HashMap<
                                String,
                                rustcode_core::provider::ProviderInfo,
                            > = std::collections::HashMap::new();
                            self.model_selector.show(
                                provider_map,
                                self.default_provider.clone(),
                                self.default_model.clone(),
                            );
                        }
                    }
                    DialogType::SessionList => {
                        if !self.session_list_state.visible {
                            let entries: Vec<SessionEntry> = Vec::new();
                            self.session_list_state.show(entries);
                        }
                    }
                    DialogType::AgentSelector => {
                        // Agent selector is handled inline via the dialog frame
                    }
                    DialogType::ThemePicker => {
                        // Theme picker placeholder
                    }
                    DialogType::Status => {
                        self.status_dialog_visible = true;
                    }
                    DialogType::Message => {
                        // Message detail view placeholder
                    }
                    DialogType::Stash => {
                        // Stash management placeholder
                    }
                }
            }
        }

        // Diff viewer overlay (full screen)
        if self.diff.visible {
            render_diff(f, area, &self.diff);
        }

        // Session list dialog
        if self.session_list_state.visible {
            render_session_list(f, area, &self.session_list_state);
        }

        // Timeline dialog (extends dialog stack when Timeline is pushed)
        if self.timeline.visible {
            render_timeline(f, area, &self.timeline);
        }

        // Export dialog
        if self.export.visible {
            render_export_dialog(f, area, &self.export);
        }

        // Subagent dialog
        if self.subagent.visible {
            render_subagent_dialog(f, area, &self.subagent);
        }

        // Model selector dialog
        if self.model_selector.visible {
            render_model_selector(f, area, &self.model_selector);
        }

        // Toast notifications (always on top, non-blocking)
        render_toast(f, area, &self.toast);
    }

    /// Sync sidebar state from TUI app state.
    fn sync_sidebar_state(&mut self) {
        if self.show_sidebar {
            self.sidebar_state.visible = true;
            self.sidebar_state.token_count = self.status.token_count.unwrap_or(0);
            self.sidebar_state.cost = self.status.cost;
            self.sidebar_state.message_count = self.conversation.messages.len();
            // Sync LSP/MCP counts
            if !self.sidebar_state.lsp_connections.is_empty() && self.status.lsp_count == 0 {
                // Keep existing connections
            }
            if !self.sidebar_state.mcp_connections.is_empty() && self.status.mcp_count == 0 {
                // Keep existing connections
            }
        }
    }

    /// Render the legacy simple sidebar (fallback when SidebarState not visible).
    fn render_legacy_sidebar(&self, f: &mut Frame, area: Rect) {
        let theme = self.theme.current();
        let block = Block::default()
            .borders(Borders::RIGHT)
            .border_style(Style::default().fg(theme.border))
            .style(Style::default().bg(theme.background));
        let inner = block.inner(area);
        f.render_widget(block, area);

        let mut lines: Vec<Line> = Vec::new();
        lines.push(Line::from(Span::styled(
            " Sessions ",
            Style::default()
                .fg(theme.accent)
                .add_modifier(Modifier::BOLD),
        )));
        lines.push(Line::from(""));
        lines.push(Line::from(Span::styled(
            self.session_id.as_deref().unwrap_or("(new)"),
            Style::default().fg(theme.foreground),
        )));
        lines.push(Line::from(""));
        lines.push(Line::from(Span::styled(
            format!("Agent: {}", self.current_agent),
            Style::default().fg(theme.dim),
        )));

        let text = Text::from(lines);
        f.render_widget(Paragraph::new(text), inner);
    }

    /// Render the command palette overlay.
    fn render_command_palette(&self, f: &mut Frame, area: Rect) {
        let theme = self.theme.current();
        let commands = self.build_command_list();
        let query_lower = self.command_palette_query.to_lowercase();
        let filtered: Vec<(usize, &str, &str)> = commands
            .iter()
            .enumerate()
            .map(|(i, cmd)| (i, cmd.0, cmd.1))
            .filter(|(_, name, desc)| {
                query_lower.is_empty()
                    || name.to_lowercase().contains(&query_lower)
                    || desc.to_lowercase().contains(&query_lower)
            })
            .collect();

        let dialog_width = (area.width as f64 * 0.5).min(70.0) as u16;
        let dialog_height = (filtered.len() as u16 + 4).min(16);
        let dialog_x = (area.width.saturating_sub(dialog_width)) / 2;
        let dialog_y = (area.height.saturating_sub(dialog_height)) / 3;

        let dialog_area = Rect::new(
            area.x + dialog_x,
            area.y + dialog_y,
            dialog_width,
            dialog_height,
        );
        f.render_widget(Clear, dialog_area);

        let block = Block::default()
            .borders(Borders::ALL)
            .title(format!(
                " Command Palette {} ",
                if self.command_palette_query.is_empty() {
                    String::new()
                } else {
                    format!("— \"{}\"", self.command_palette_query)
                }
            ))
            .border_style(Style::default().fg(theme.accent))
            .style(Style::default().bg(theme.background));

        let inner = block.inner(dialog_area);
        f.render_widget(block, dialog_area);

        let items: Vec<ListItem> = filtered
            .iter()
            .enumerate()
            .map(|(i, (_orig_idx, name, desc))| {
                let style = if i == self.command_palette_selection {
                    Style::default().fg(theme.background).bg(theme.accent)
                } else {
                    Style::default().fg(theme.foreground)
                };
                ListItem::new(Line::from(vec![
                    Span::styled(format!(" {name} "), style),
                    Span::styled(format!("  {desc}"), Style::default().fg(theme.dim)),
                ]))
            })
            .collect();

        if items.is_empty() {
            let no_results =
                Paragraph::new("No matching commands").style(Style::default().fg(theme.dim));
            f.render_widget(no_results, inner);
        } else {
            let list = List::new(items);
            f.render_widget(list, inner);
        }
    }

    /// Build the command list for the palette.
    fn build_command_list(&self) -> Vec<(&'static str, &'static str)> {
        vec![
            ("SessionNew", "Create a new session"),
            ("SessionList", "List all sessions"),
            ("SessionRename", "Rename current session"),
            ("SessionFork", "Fork session at current message"),
            ("SessionCompact", "Compact/compress session context"),
            ("SessionExport", "Export session to file"),
            ("SessionTimeline", "Show session timeline/undoscope"),
            ("SessionUndo", "Undo last change"),
            ("SessionRedo", "Redo last undone change"),
            ("SessionDelete", "Delete current session"),
            ("SessionBackground", "Toggle background subagent mode"),
            ("SessionInterrupt", "Interrupt current LLM call"),
            ("AgentCycle", "Cycle to next agent"),
            ("AgentList", "List available agents"),
            ("ModelList", "List available models"),
            ("VariantCycle", "Cycle model variant"),
            ("ProviderConnect", "Connect to provider"),
            ("CommandPalette", "Show this command palette"),
            ("Help", "Show keybinding reference"),
            ("Status", "Show detailed status"),
            ("Suspend", "Suspend terminal (Ctrl+Z)"),
            ("Quit", "Exit rustcode"),
            ("ToggleSidebar", "Toggle session sidebar"),
            ("ToggleTimestamps", "Toggle message timestamps"),
            ("ToggleThinking", "Toggle thinking/reasoning display"),
            ("ToggleToolDetails", "Toggle tool call details"),
            ("ToggleConceal", "Toggle conceal mode"),
            ("ToggleScrollbar", "Toggle scrollbar"),
            ("ToggleAnimations", "Toggle animations"),
            ("ToggleFileContext", "Toggle file context display"),
            ("ToggleDiffWrap", "Toggle diff wrapping"),
            ("TogglePasteSummary", "Toggle paste summary"),
            ("ThemeSwitch", "Switch color theme"),
            ("ThemeSwitchMode", "Toggle dark/light mode"),
            ("OpenInEditor", "Open current file in external editor"),
            ("AudioToggle", "Toggle audio notification on completion"),
        ]
    }

    /// Render the help overlay (keybinding reference).
    fn render_help(&self, f: &mut Frame, area: Rect) {
        let theme = self.theme.current();
        let dialog_width = (area.width as f64 * 0.7).min(80.0) as u16;
        let dialog_height = 20;
        let dialog_x = (area.width.saturating_sub(dialog_width)) / 2;
        let dialog_y = (area.height.saturating_sub(dialog_height)) / 3;

        let dialog_area = Rect::new(
            area.x + dialog_x,
            area.y + dialog_y,
            dialog_width,
            dialog_height,
        );
        f.render_widget(Clear, dialog_area);

        let block = Block::default()
            .borders(Borders::ALL)
            .title(" Keybindings (Esc to close) ")
            .border_style(Style::default().fg(theme.accent))
            .style(Style::default().bg(theme.background));

        let inner = block.inner(dialog_area);
        f.render_widget(block, dialog_area);

        let mut lines: Vec<Line> = Vec::new();

        // Section: Global
        lines.push(Line::from(Span::styled(
            " Global ",
            Style::default()
                .fg(theme.accent)
                .add_modifier(Modifier::BOLD),
        )));
        for binding in all_bindings() {
            let key_str = key_event_to_string(&binding.key);
            lines.push(Line::from(vec![
                Span::styled(
                    format!("  {key_str:<20}"),
                    Style::default().fg(theme.warning),
                ),
                Span::styled(binding.description, Style::default().fg(theme.dim)),
            ]));
        }

        // Section: Leader (Ctrl+X)
        lines.push(Line::from(""));
        lines.push(Line::from(Span::styled(
            " Leader (Ctrl+X then...) ",
            Style::default()
                .fg(theme.accent)
                .add_modifier(Modifier::BOLD),
        )));
        for binding in all_leader_bindings() {
            let key_str = key_event_to_string(&binding.key);
            lines.push(Line::from(vec![
                Span::styled(
                    format!("  C-x {key_str:<17}"),
                    Style::default().fg(theme.warning),
                ),
                Span::styled(binding.description, Style::default().fg(theme.dim)),
            ]));
        }

        // Toggles section
        lines.push(Line::from(""));
        lines.push(Line::from(Span::styled(
            " Toggles ",
            Style::default()
                .fg(theme.accent)
                .add_modifier(Modifier::BOLD),
        )));
        let toggles = [
            ("timestamps", self.show_timestamps),
            ("thinking", self.show_thinking),
            ("tool details", self.show_tool_details),
            ("conceal", self.conceal_enabled),
            ("sidebar", self.show_sidebar),
            ("scrollbar", self.show_scrollbar),
            ("animations", self.animations_enabled),
            ("file context", self.file_context_enabled),
            ("diff wrap", self.diff_wrap),
            ("paste summary", self.paste_summary),
            ("audio bell", self.audio_enabled),
        ];
        for (name, state) in &toggles {
            let icon = if *state { "✓" } else { "✗" };
            let color = if *state { theme.success } else { theme.error };
            lines.push(Line::from(vec![
                Span::styled(format!("  [{icon}] "), Style::default().fg(color)),
                Span::styled(*name, Style::default().fg(theme.dim)),
            ]));
        }

        let text = Text::from(lines);
        f.render_widget(Paragraph::new(text).wrap(Wrap { trim: true }), inner);
    }

    /// Render the status/info dialog.
    fn render_status_dialog(&self, f: &mut Frame, area: Rect) {
        let theme = self.theme.current();
        let dialog_width = (area.width as f64 * 0.5).min(60.0) as u16;
        let dialog_height = 14;
        let dialog_x = (area.width.saturating_sub(dialog_width)) / 2;
        let dialog_y = (area.height.saturating_sub(dialog_height)) / 3;

        let dialog_area = Rect::new(
            area.x + dialog_x,
            area.y + dialog_y,
            dialog_width,
            dialog_height,
        );
        f.render_widget(Clear, dialog_area);

        let block = Block::default()
            .borders(Borders::ALL)
            .title(" Status (Esc to close) ")
            .border_style(Style::default().fg(theme.accent))
            .style(Style::default().bg(theme.background));

        let inner = block.inner(dialog_area);
        f.render_widget(block, dialog_area);

        let mut lines: Vec<Line> = Vec::new();

        lines.push(Line::from(Span::styled(
            " Session ",
            Style::default()
                .fg(theme.accent)
                .add_modifier(Modifier::BOLD),
        )));
        lines.push(Line::from(Span::styled(
            format!(
                "  ID:    {}",
                self.session_id.as_deref().unwrap_or("(none)")
            ),
            Style::default().fg(theme.foreground),
        )));
        lines.push(Line::from(Span::styled(
            format!("  Agent: {}", self.current_agent),
            Style::default().fg(theme.foreground),
        )));

        lines.push(Line::from(""));
        lines.push(Line::from(Span::styled(
            " Provider ",
            Style::default()
                .fg(theme.accent)
                .add_modifier(Modifier::BOLD),
        )));
        lines.push(Line::from(Span::styled(
            format!(
                "  Name:  {}",
                self.status.provider_name.as_deref().unwrap_or("none")
            ),
            Style::default().fg(theme.foreground),
        )));
        lines.push(Line::from(Span::styled(
            format!(
                "  Model: {}",
                self.status.model_name.as_deref().unwrap_or("none")
            ),
            Style::default().fg(theme.foreground),
        )));
        if self.status.cost > 0.0 {
            lines.push(Line::from(Span::styled(
                format!("  Cost:  ${:.4}", self.status.cost),
                Style::default().fg(theme.warning),
            )));
        }
        if let Some(tokens) = self.status.token_count {
            lines.push(Line::from(Span::styled(
                format!("  Tokens: {tokens}"),
                Style::default().fg(theme.foreground),
            )));
        }

        lines.push(Line::from(""));
        lines.push(Line::from(Span::styled(
            format!(
                "  Theme: {} ({} mode)",
                self.theme.name(),
                self.theme.mode().as_str()
            ),
            Style::default().fg(theme.foreground),
        )));
        lines.push(Line::from(""));
        lines.push(Line::from(Span::styled(
            " Services ",
            Style::default()
                .fg(theme.accent)
                .add_modifier(Modifier::BOLD),
        )));
        lines.push(Line::from(Span::styled(
            format!("  LSP: {} active", self.status.lsp_count),
            Style::default().fg(theme.success),
        )));
        let mcp_color = if self.status.mcp_error {
            theme.error
        } else {
            theme.success
        };
        lines.push(Line::from(Span::styled(
            format!("  MCP: {} connected", self.status.mcp_count),
            Style::default().fg(mcp_color),
        )));
        lines.push(Line::from(Span::styled(
            format!("  Permissions: {} pending", self.status.permission_count),
            Style::default().fg(if self.status.permission_count > 0 {
                theme.warning
            } else {
                theme.success
            }),
        )));

        let text = Text::from(lines);
        f.render_widget(Paragraph::new(text), inner);
    }

    // ── Key Handling ─────────────────────────────────────────────────

    fn handle_key_event(&mut self, key: crossterm::event::KeyEvent) {
        // Handle overlays first — priority order
        if self.command_palette_visible {
            self.handle_command_palette_key(key);
            return;
        }
        if self.help_visible || self.status_dialog_visible {
            // Any key dismisses help/status except when leader is active
            if key.code == crossterm::event::KeyCode::Esc
                || key.code == crossterm::event::KeyCode::Enter
                || key.code == crossterm::event::KeyCode::Char('q')
            {
                self.help_visible = false;
                self.status_dialog_visible = false;
            }
            // Still handle quit keys
            if matches!(
                key.code,
                crossterm::event::KeyCode::Char('c') | crossterm::event::KeyCode::Char('d')
            ) && key.modifiers == crossterm::event::KeyModifiers::CONTROL
            {
                self.should_quit = true;
            }
            return;
        }

        if self.leader_active {
            self.leader_active = false;
            if let Some(action) = leader_chord_to_action(key) {
                self.dispatch_action(action);
            }
            return;
        }
        if is_leader_prefix(key) {
            self.leader_active = true;
            return;
        }

        // Diff viewer overlay (handles its own keys)
        if self.diff.visible && self.diff.handle_key(key) {
            return;
        }

        // Dialog stack overlay (blocks most keys)
        if self.dialog.is_active() && self.dialog.handle_key(key) {
            return;
        }

        // Session list dialog (handles its own keys)
        if self.session_list_state.visible {
            if let Some(action) = self.session_list_state.handle_key(key) {
                match action {
                    SessionListAction::Close => {}
                    SessionListAction::Navigate => {}
                    SessionListAction::Select(Some(sid)) => {
                        self.session_id = Some(sid.clone());
                        self.add_system_message(&format!("Switched to session {sid}"));
                    }
                    SessionListAction::Select(None) => {}
                    SessionListAction::Delete(Some(sid)) => {
                        self.add_system_message(&format!("Deleted session {sid}"));
                    }
                    SessionListAction::Delete(None) => {}
                    SessionListAction::Pin(_) => {
                        self.add_system_message("Session pin toggled.");
                    }
                }
            }
            return;
        }

        // Timeline dialog
        if self.timeline.visible {
            if let Some(action) = self.timeline.handle_key(key) {
                match action {
                    TimelineAction::Close => {}
                    TimelineAction::Navigate => {}
                    TimelineAction::Select(msg_id) => {
                        self.add_system_message(&format!("Timeline node selected: {msg_id}"));
                    }
                    TimelineAction::Fork(msg_id) => {
                        if let Some(ref sessions) = self.sessions {
                            if let Some(ref sid) = self.session_id {
                                let sessions = sessions.clone();
                                let sid = sid.clone();
                                let mid = msg_id.unwrap_or_default();
                                tokio::spawn(async move {
                                    match sessions.fork(&sid, Some(&mid)).await {
                                        Ok(new_session) => {
                                            tracing::info!(
                                                "session forked from timeline: {} -> {}",
                                                sid,
                                                new_session.id
                                            );
                                        }
                                        Err(e) => tracing::error!("timeline fork failed: {e}"),
                                    }
                                });
                            }
                        }
                        self.add_system_message("Forking session from timeline...");
                    }
                }
            }
            return;
        }

        // Export dialog
        if self.export.visible {
            if let Some(action) = self.export.handle_key(key) {
                match action {
                    ExportAction::Close => {}
                    ExportAction::Navigate => {}
                    ExportAction::Export {
                        filename,
                        format,
                        sanitize,
                    } => {
                        let ext = format.extension();
                        self.add_system_message(&format!(
                            "Exporting session as {} (sanitize: {}) -> {}",
                            ext, sanitize, filename
                        ));
                        // In full impl, write the file using session data
                        if let Some(ref sessions) = self.sessions {
                            if let Some(ref sid) = self.session_id {
                                let sessions = sessions.clone();
                                let sid = sid.clone();
                                let fname = filename.clone();
                                let sanitize_flag = sanitize;
                                tokio::spawn(async move {
                                    match sessions.get(&sid).await {
                                        Ok(info) => {
                                            tracing::info!(
                                                "export requested for session {} -> {} (format={:?}, sanitize={})",
                                                sid, fname, format, sanitize_flag
                                            );
                                            let _ = info; // Use session info for export
                                        }
                                        Err(e) => tracing::error!("export failed: {e}"),
                                    }
                                });
                            }
                        }
                        self.export.confirmed = false;
                    }
                }
            }
            return;
        }

        // Subagent dialog
        if self.subagent.visible {
            if let Some(action) = self.subagent.handle_key(key) {
                match action {
                    SubagentAction::Close => {}
                    SubagentAction::Navigate => {}
                    SubagentAction::NavigateTo(sid) => {
                        if let Some(ref bus) = self.bus {
                            let event = rustcode_core::bus::GlobalEvent::new(serde_json::json!({
                                "type": "session.select",
                                "sessionID": sid,
                            }));
                            let _ = bus.publish(event);
                        }
                        self.session_id = Some(sid.clone());
                        self.add_system_message(&format!("Navigated to subagent session {sid}"));
                    }
                    SubagentAction::Spawn { agent, task, model } => {
                        self.add_system_message(&format!(
                            "Spawning subagent '{}' with task: {}...",
                            agent,
                            &task[..task.len().min(40)]
                        ));
                        // Spawn a background task to create the subagent session
                        if let Some(ref sessions) = self.sessions {
                            if let Some(ref sid) = self.session_id {
                                let sessions = sessions.clone();
                                let parent_id = sid.clone();
                                let agent_clone = agent.clone();
                                let task_clone = task.clone();
                                let model_clone = model.clone();
                                tokio::spawn(async move {
                                    // Create the child session
                                    let title = format!(
                                        "{}: {}",
                                        agent_clone,
                                        &task_clone[..task_clone.len().min(50)]
                                    );
                                    let input = rustcode_core::session::CreateSessionInput {
                                        project_id: String::new(),
                                        workspace_id: None,
                                        directory: String::new(),
                                        path: None,
                                        parent_id: Some(parent_id),
                                        title: Some(title),
                                        agent: Some(agent_clone),
                                        model: model_clone.map(|m| {
                                            rustcode_core::session::ModelSelection {
                                                id: m,
                                                provider_id: String::new(),
                                                variant: None,
                                            }
                                        }),
                                        metadata: None,
                                        permission: None,
                                    };
                                    match sessions.create(input).await {
                                        Ok(info) => {
                                            tracing::info!(
                                                "subagent created: {} (parent: {})",
                                                info.id,
                                                info.parent_id.as_deref().unwrap_or("none")
                                            );
                                        }
                                        Err(e) => tracing::error!("subagent creation failed: {e}"),
                                    }
                                });
                            }
                        }
                    }
                }
            }
            return;
        }

        // Model selector dialog
        if self.model_selector.visible {
            if let Some(action) = self.model_selector.handle_key(key) {
                match action {
                    ModelSelectorAction::Close => {}
                    ModelSelectorAction::Navigate => {}
                    ModelSelectorAction::Select {
                        provider_id,
                        model_id,
                    } => {
                        self.default_provider = Some(provider_id.clone());
                        self.default_model = Some(model_id.clone());
                        self.status.provider_name = Some(provider_id.clone());
                        self.status.model_name = Some(model_id.clone());
                        // Track in recent models
                        let model_key = format!("{provider_id}/{model_id}");
                        self.recent_models.retain(|m| m != &model_key);
                        self.recent_models.insert(0, model_key);
                        self.recent_models.truncate(20);
                        self.add_system_message(&format!("Model: {provider_id}/{model_id}"));
                    }
                }
            }
            return;
        }

        if self.permission.visible {
            if let Some(reply) = self.permission.handle_key(key) {
                self.handle_permission_reply(reply);
            }
            return;
        }
        if self.question.visible {
            if let Some((rid, answers)) = self.question.handle_key(key) {
                self.handle_question_reply(rid, answers);
            }
            return;
        }

        // Input handling
        if self.input.focused {
            if self.input.handle_key(key) {
                return;
            }
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

        if let Some(action) = key_to_action(key) {
            self.dispatch_action(action);
        }
    }

    /// Handle keys while the command palette is visible.
    fn handle_command_palette_key(&mut self, key: crossterm::event::KeyEvent) {
        // Allow toggling palette closed with Ctrl+P (the same binding that opens it)
        if key.code == crossterm::event::KeyCode::Char('p')
            && key.modifiers == crossterm::event::KeyModifiers::CONTROL
        {
            self.command_palette_visible = false;
            self.command_palette_query.clear();
            self.command_palette_selection = 0;
            self.input.focused = true;
            return;
        }

        match key.code {
            crossterm::event::KeyCode::Esc => {
                self.command_palette_visible = false;
                self.command_palette_query.clear();
                self.command_palette_selection = 0;
                self.input.focused = true;
            }
            crossterm::event::KeyCode::Enter => {
                // Execute the selected command
                let commands = self.build_command_list();
                let query_lower = self.command_palette_query.to_lowercase();
                let filtered: Vec<&str> = commands
                    .iter()
                    .filter(|(name, desc)| {
                        query_lower.is_empty()
                            || name.to_lowercase().contains(&query_lower)
                            || desc.to_lowercase().contains(&query_lower)
                    })
                    .map(|(name, _)| *name)
                    .collect();

                if let Some(&cmd_name) = filtered.get(self.command_palette_selection) {
                    let action = match cmd_name {
                        "SessionNew" => TuiAction::SessionNew,
                        "SessionList" => TuiAction::SessionList,
                        "SessionRename" => TuiAction::SessionRename,
                        "SessionFork" => TuiAction::SessionFork,
                        "SessionCompact" => TuiAction::SessionCompact,
                        "SessionExport" => TuiAction::SessionExport,
                        "SessionTimeline" => TuiAction::SessionTimeline,
                        "SessionUndo" => TuiAction::SessionUndo,
                        "SessionRedo" => TuiAction::SessionRedo,
                        "SessionDelete" => TuiAction::SessionDelete,
                        "SessionBackground" => TuiAction::SessionBackground,
                        "SessionInterrupt" => TuiAction::SessionInterrupt,
                        "AgentCycle" => TuiAction::AgentCycle,
                        "AgentList" => TuiAction::AgentList,
                        "ModelList" => TuiAction::ModelList,
                        "VariantCycle" => TuiAction::VariantCycle,
                        "ProviderConnect" => TuiAction::ProviderConnect,
                        "CommandPalette" => TuiAction::CommandPalette,
                        "Help" => TuiAction::Help,
                        "Status" => TuiAction::Status,
                        "Suspend" => TuiAction::Suspend,
                        "Quit" => TuiAction::Quit,
                        "ToggleSidebar" => TuiAction::ToggleSidebar,
                        "ToggleTimestamps" => TuiAction::ToggleTimestamps,
                        "ToggleThinking" => TuiAction::ToggleThinking,
                        "ToggleToolDetails" => TuiAction::ToggleToolDetails,
                        "ToggleConceal" => TuiAction::ToggleConceal,
                        "ToggleScrollbar" => TuiAction::ToggleScrollbar,
                        "ToggleAnimations" => TuiAction::ToggleAnimations,
                        "ToggleFileContext" => TuiAction::ToggleFileContext,
                        "ToggleDiffWrap" => TuiAction::ToggleDiffWrap,
                        "TogglePasteSummary" => TuiAction::TogglePasteSummary,
                        "ThemeSwitch" => TuiAction::ThemeSwitch,
                        "ThemeSwitchMode" => TuiAction::ThemeSwitchMode,
                        "OpenInEditor" => TuiAction::OpenInEditor,
                        "AudioToggle" => TuiAction::AudioToggle,
                        _ => return,
                    };
                    self.command_palette_visible = false;
                    self.command_palette_query.clear();
                    self.command_palette_selection = 0;
                    self.dispatch_action(action);
                }
            }
            crossterm::event::KeyCode::Up => {
                let commands = self.build_command_list();
                let query_lower = self.command_palette_query.to_lowercase();
                let count = commands
                    .iter()
                    .filter(|(name, desc)| {
                        query_lower.is_empty()
                            || name.to_lowercase().contains(&query_lower)
                            || desc.to_lowercase().contains(&query_lower)
                    })
                    .count();
                if count > 0 {
                    self.command_palette_selection = if self.command_palette_selection == 0 {
                        count.saturating_sub(1)
                    } else {
                        self.command_palette_selection - 1
                    };
                }
            }
            crossterm::event::KeyCode::Down => {
                let commands = self.build_command_list();
                let query_lower = self.command_palette_query.to_lowercase();
                let count = commands
                    .iter()
                    .filter(|(name, desc)| {
                        query_lower.is_empty()
                            || name.to_lowercase().contains(&query_lower)
                            || desc.to_lowercase().contains(&query_lower)
                    })
                    .count();
                if count > 0 {
                    self.command_palette_selection = (self.command_palette_selection + 1) % count;
                }
            }
            crossterm::event::KeyCode::Backspace => {
                self.command_palette_query.pop();
                self.command_palette_selection = 0;
            }
            crossterm::event::KeyCode::Char(ch) => {
                self.command_palette_query.push(ch);
                self.command_palette_selection = 0;
            }
            _ => {}
        }
    }

    // ── Action Dispatch ──────────────────────────────────────────────

    fn dispatch_action(&mut self, action: TuiAction) {
        match action {
            // ── App-level ───────────────────────────────────────
            TuiAction::Quit => {
                self.should_quit = true;
                tracing::info!("TUI quitting");
            }
            TuiAction::CommandPalette => {
                self.command_palette_visible = !self.command_palette_visible;
                self.command_palette_query.clear();
                self.command_palette_selection = 0;
                self.input.focused = !self.command_palette_visible;
            }
            TuiAction::Help => {
                self.help_visible = !self.help_visible;
                self.input.focused = !self.help_visible;
            }
            TuiAction::Status => {
                self.status_dialog_visible = !self.status_dialog_visible;
                self.input.focused = !self.status_dialog_visible;
            }
            TuiAction::Suspend => {
                // Restore terminal state before suspending
                let _ = disable_raw_mode();
                let _ = execute!(io::stdout(), LeaveAlternateScreen);

                // Send SIGTSTP to our own process to suspend
                // Uses the kill command as a portable way to send signals without libc/nix
                let pid = std::process::id().to_string();
                let _ = std::process::Command::new("kill")
                    .args(["-s", "TSTP", &pid])
                    .status();

                // When resumed (SIGCONT), reinitialize terminal
                let _ = enable_raw_mode();
                let _ = execute!(io::stdout(), EnterAlternateScreen);
            }

            // ── Session ─────────────────────────────────────────
            TuiAction::SessionInterrupt => {
                self.status.session_status = Some(SessionStatus::Idle);
                self.is_streaming = false;
                self.current_assistant_msg_id = None;
                if let Some(ref bus) = self.bus {
                    let event = rustcode_core::bus::GlobalEvent::new(serde_json::json!({
                        "type": "session.interrupted",
                        "sessionID": self.session_id.as_deref().unwrap_or(""),
                    }));
                    let _ = bus.publish(event);
                }
                self.add_system_message("Interrupted.");
            }
            TuiAction::SessionNew => {
                self.session_id = None;
                self.conversation.messages.clear();
                self.conversation.parts.clear();
                self.status.cost = 0.0;
                self.status.token_count = None;
                self.add_system_message("New session.");
                tracing::info!("new session created");
            }
            TuiAction::SessionList => {
                // Spawn async task to list sessions
                if let Some(ref sessions) = self.sessions {
                    let sessions = sessions.clone();
                    let bus = self.bus.clone();
                    let current_id = self.session_id.clone();
                    tokio::spawn(async move {
                        match sessions.list(None).await {
                            Ok(list) => {
                                tracing::info!("sessions listed: {} found", list.len());
                                for s in &list {
                                    tracing::info!(
                                        "  {} — {} [{}]",
                                        s.id,
                                        s.title,
                                        s.agent.as_deref().unwrap_or("none")
                                    );
                                }
                            }
                            Err(e) => tracing::error!("failed to list sessions: {e}"),
                        }
                        let _ = current_id;
                        let _ = bus;
                    });
                }
                self.add_system_message("Session list requested (see logs).");
            }
            TuiAction::SessionRename => {
                self.add_system_message("Session rename: type a new name and press Enter.");
                self.input.clear();
                self.input.focused = true;
                // In a full impl, we'd enter a rename mode
            }
            TuiAction::SessionFork => {
                if let Some(ref sessions) = self.sessions {
                    if let Some(ref sid) = self.session_id {
                        let sessions = sessions.clone();
                        let sid = sid.clone();
                        tokio::spawn(async move {
                            match sessions.fork(&sid, None).await {
                                Ok(new_session) => {
                                    tracing::info!("session forked: {} -> {}", sid, new_session.id);
                                }
                                Err(e) => tracing::error!("fork failed: {e}"),
                            }
                        });
                    }
                }
                self.add_system_message("Session forked.");
            }
            TuiAction::SessionCompact => {
                if let Some(ref bus) = self.bus {
                    let event = rustcode_core::bus::GlobalEvent::new(serde_json::json!({
                        "type": "session.compact",
                        "sessionID": self.session_id.as_deref().unwrap_or(""),
                    }));
                    let _ = bus.publish(event);
                }
                self.add_system_message("Session compaction requested.");
            }
            TuiAction::SessionExport => {
                // Open the export dialog with session data
                let msg_count = self.conversation.messages.len();
                self.export.show(self.session_id.as_deref(), msg_count);
                self.input.focused = false;
                // Also publish bus event for any backend listeners
                if let Some(ref bus) = self.bus {
                    let event = rustcode_core::bus::GlobalEvent::new(serde_json::json!({
                        "type": "session.export",
                        "sessionID": self.session_id.as_deref().unwrap_or(""),
                    }));
                    let _ = bus.publish(event);
                }
            }
            TuiAction::SessionTimeline => {
                // Build timeline from conversation messages and open dialog
                self.timeline
                    .build_from_messages(&self.conversation.messages);
                self.timeline.show();
                self.input.focused = false;
                // Also publish bus event for any backend listeners
                if let Some(ref bus) = self.bus {
                    let event = rustcode_core::bus::GlobalEvent::new(serde_json::json!({
                        "type": "session.timeline",
                        "sessionID": self.session_id.as_deref().unwrap_or(""),
                    }));
                    let _ = bus.publish(event);
                }
            }
            TuiAction::SessionUndo => {
                if let Some(ref bus) = self.bus {
                    let event = rustcode_core::bus::GlobalEvent::new(serde_json::json!({
                        "type": "session.undo",
                        "sessionID": self.session_id.as_deref().unwrap_or(""),
                    }));
                    let _ = bus.publish(event);
                }
                self.add_system_message("Undo requested.");
            }
            TuiAction::SessionRedo => {
                if let Some(ref bus) = self.bus {
                    let event = rustcode_core::bus::GlobalEvent::new(serde_json::json!({
                        "type": "session.redo",
                        "sessionID": self.session_id.as_deref().unwrap_or(""),
                    }));
                    let _ = bus.publish(event);
                }
                self.add_system_message("Redo requested.");
            }
            TuiAction::SessionDelete => {
                if let Some(ref sessions) = self.sessions {
                    if let Some(ref sid) = self.session_id {
                        let sessions = sessions.clone();
                        let sid = sid.clone();
                        tokio::spawn(async move {
                            match sessions.remove(&sid).await {
                                Ok(()) => tracing::info!("session deleted: {sid}"),
                                Err(e) => tracing::error!("delete failed: {e}"),
                            }
                        });
                    }
                }
                self.session_id = None;
                self.conversation.messages.clear();
                self.conversation.parts.clear();
                self.add_system_message("Session deleted.");
            }
            TuiAction::SessionBackground => {
                self.add_system_message("Background subagent mode toggled.");
            }
            TuiAction::SessionShare => {
                if let Some(ref bus) = self.bus {
                    let event = rustcode_core::bus::GlobalEvent::new(serde_json::json!({
                        "type": "session.share",
                        "sessionID": self.session_id.as_deref().unwrap_or(""),
                    }));
                    let _ = bus.publish(event);
                }
                if let Some(ref sid) = self.session_id {
                    // Try to create a share URL via the session manager
                    if let Some(ref sessions) = self.sessions {
                        let sessions = sessions.clone();
                        let sid = sid.clone();
                        tokio::spawn(async move {
                            match sessions.get(&sid).await {
                                Ok(info) => {
                                    if let Some(share) = info.share {
                                        tracing::info!("session share URL: {}", share.url);
                                    } else {
                                        tracing::info!("session {} not yet shared — publish share event to create URL", sid);
                                    }
                                }
                                Err(e) => tracing::error!("failed to get session for share: {e}"),
                            }
                        });
                    }
                }
                self.add_system_message("Session share requested.");
            }

            // ── Navigation ───────────────────────────────────────
            TuiAction::ScrollUp => self.conversation.scroll_up(1),
            TuiAction::ScrollDown => self.conversation.scroll_down(1),
            TuiAction::ScrollPageUp => {
                let h = (self.last_term_size.height / 2).max(1);
                self.conversation.scroll_up(h);
            }
            TuiAction::ScrollPageDown => {
                let h = (self.last_term_size.height / 2).max(1);
                self.conversation.scroll_down(h);
            }
            TuiAction::ScrollHalfPageUp => {
                let h = (self.last_term_size.height / 4).max(1);
                self.conversation.scroll_up(h);
            }
            TuiAction::ScrollHalfPageDown => {
                let h = (self.last_term_size.height / 4).max(1);
                self.conversation.scroll_down(h);
            }
            TuiAction::ScrollFirst => self.conversation.scroll_to_top(),
            TuiAction::ScrollLast => self.conversation.scroll_to_bottom(),
            TuiAction::ScrollNextMessage => {
                // Scroll to next assistant message
                self.conversation.scroll_down(3);
            }
            TuiAction::ScrollPrevMessage => {
                // Scroll to previous assistant message
                self.conversation.scroll_up(3);
            }

            // ── Child sessions ───────────────────────────────────
            TuiAction::ChildFirst => {
                // Navigate to the first child session of the current session
                if let (Some(ref sessions), Some(ref sid)) = (&self.sessions, &self.session_id) {
                    let sessions = sessions.clone();
                    let sid = sid.clone();
                    let bus = self.bus.clone();
                    tokio::spawn(async move {
                        match sessions.list(None).await {
                            Ok(all) => {
                                let children: Vec<_> = all
                                    .iter()
                                    .filter(|s| s.parent_id.as_deref() == Some(&sid))
                                    .collect();
                                if let Some(child) = children.first() {
                                    tracing::info!("navigating to first child: {}", child.id);
                                    if let Some(bus) = bus {
                                        let _ = bus.publish(rustcode_core::bus::GlobalEvent::new(
                                            serde_json::json!({
                                                "type": "session.select",
                                                "sessionID": child.id,
                                            }),
                                        ));
                                    }
                                } else {
                                    tracing::info!("no child sessions found for {sid}");
                                }
                            }
                            Err(e) => tracing::error!("child session lookup failed: {e}"),
                        }
                    });
                    self.add_system_message("Navigating to first child session...");
                } else {
                    self.add_system_message("No active session.");
                }
            }
            TuiAction::ChildNext => {
                if let (Some(ref sessions), Some(ref sid)) = (&self.sessions, &self.session_id) {
                    let sessions = sessions.clone();
                    let sid = sid.clone();
                    let bus = self.bus.clone();
                    tokio::spawn(async move {
                        match sessions.list(None).await {
                            Ok(all) => {
                                let children: Vec<_> = all
                                    .iter()
                                    .filter(|s| s.parent_id.as_deref() == Some(&sid))
                                    .collect();
                                if children.len() > 1 {
                                    let second = &children[1];
                                    tracing::info!("navigating to next child: {}", second.id);
                                    if let Some(bus) = bus {
                                        let _ = bus.publish(rustcode_core::bus::GlobalEvent::new(
                                            serde_json::json!({
                                                "type": "session.select",
                                                "sessionID": second.id,
                                            }),
                                        ));
                                    }
                                } else {
                                    tracing::info!("only {} child(ren), no next", children.len());
                                }
                            }
                            Err(e) => tracing::error!("child session lookup failed: {e}"),
                        }
                    });
                    self.add_system_message("Navigating to next child session...");
                } else {
                    self.add_system_message("No active session.");
                }
            }
            TuiAction::ChildPrev => {
                if let (Some(ref sessions), Some(ref sid)) = (&self.sessions, &self.session_id) {
                    let sessions = sessions.clone();
                    let sid = sid.clone();
                    let bus = self.bus.clone();
                    tokio::spawn(async move {
                        match sessions.list(None).await {
                            Ok(all) => {
                                let children: Vec<_> = all
                                    .iter()
                                    .filter(|s| s.parent_id.as_deref() == Some(&sid))
                                    .collect();
                                let len = children.len();
                                if len > 1 {
                                    let last = &children[len - 1];
                                    tracing::info!("navigating to prev child: {}", last.id);
                                    if let Some(bus) = bus {
                                        let _ = bus.publish(rustcode_core::bus::GlobalEvent::new(
                                            serde_json::json!({
                                                "type": "session.select",
                                                "sessionID": last.id,
                                            }),
                                        ));
                                    }
                                } else {
                                    tracing::info!("only {} child(ren), no previous", len);
                                }
                            }
                            Err(e) => tracing::error!("child session lookup failed: {e}"),
                        }
                    });
                    self.add_system_message("Navigating to previous child session...");
                } else {
                    self.add_system_message("No active session.");
                }
            }
            TuiAction::Parent => {
                if let (Some(ref sessions), Some(ref sid)) = (&self.sessions, &self.session_id) {
                    let sessions = sessions.clone();
                    let sid = sid.clone();
                    let bus = self.bus.clone();
                    tokio::spawn(async move {
                        match sessions.get(&sid).await {
                            Ok(info) => {
                                if let Some(parent_id) = info.parent_id {
                                    tracing::info!("navigating to parent: {parent_id}");
                                    if let Some(bus) = bus {
                                        let _ = bus.publish(rustcode_core::bus::GlobalEvent::new(
                                            serde_json::json!({
                                                "type": "session.select",
                                                "sessionID": parent_id,
                                            }),
                                        ));
                                    }
                                } else {
                                    tracing::info!("session {sid} has no parent");
                                }
                            }
                            Err(e) => tracing::error!("parent lookup failed: {e}"),
                        }
                    });
                    self.add_system_message("Navigating to parent session...");
                } else {
                    self.add_system_message("No active session.");
                }
            }

            // ── Agent/Model ──────────────────────────────────────
            TuiAction::AgentCycle => {
                // Use the keys from the providers HashMap for cycling
                let provider_keys: Vec<&String> = self.providers.keys().collect();
                if !provider_keys.is_empty() {
                    let current_provider = self.default_provider.as_deref().unwrap_or("");
                    let idx = provider_keys
                        .iter()
                        .position(|k| k.as_str() == current_provider)
                        .unwrap_or(0);
                    let next_idx = (idx + 1) % provider_keys.len();
                    let next_provider = provider_keys[next_idx].clone();
                    self.default_provider = Some(next_provider.clone());
                    self.status.provider_name = Some(next_provider.clone());

                    // Reset model to default for this provider
                    self.default_model = if next_provider == "anthropic" {
                        Some("claude-sonnet-4-6".into())
                    } else if next_provider == "openai" {
                        Some("gpt-5.2".into())
                    } else if next_provider == "google" {
                        Some("gemini-3.0-flash".into())
                    } else {
                        None
                    };
                    self.status.model_name = self.default_model.clone();

                    // Track in recent models
                    let model_key = format!(
                        "{}/{}",
                        next_provider,
                        self.default_model.as_deref().unwrap_or("auto")
                    );
                    self.recent_models.retain(|m| m != &model_key);
                    self.recent_models.insert(0, model_key);
                    self.recent_models.truncate(20);

                    self.add_system_message(&format!(
                        "Provider: {next_provider} | Model: {}",
                        self.default_model.as_deref().unwrap_or("auto")
                    ));
                } else {
                    // Fallback: cycle agents
                    let agents = ["build", "plan", "general"];
                    let idx = agents
                        .iter()
                        .position(|a| *a == self.current_agent)
                        .unwrap_or(0);
                    self.current_agent = agents[(idx + 1) % agents.len()].into();
                    self.add_system_message(&format!("Agent: {}", self.current_agent));
                }
            }
            TuiAction::AgentCycleReverse => {
                let provider_keys: Vec<&String> = self.providers.keys().collect();
                if !provider_keys.is_empty() {
                    let current_provider = self.default_provider.as_deref().unwrap_or("");
                    let idx = provider_keys
                        .iter()
                        .position(|k| k.as_str() == current_provider)
                        .unwrap_or(0);
                    let prev_idx = if idx == 0 {
                        provider_keys.len() - 1
                    } else {
                        idx - 1
                    };
                    let prev_provider = provider_keys[prev_idx].clone();
                    self.default_provider = Some(prev_provider.clone());
                    self.status.provider_name = Some(prev_provider.clone());

                    self.default_model = if prev_provider == "anthropic" {
                        Some("claude-sonnet-4-6".into())
                    } else if prev_provider == "openai" {
                        Some("gpt-5.2".into())
                    } else if prev_provider == "google" {
                        Some("gemini-3.0-flash".into())
                    } else {
                        None
                    };
                    self.status.model_name = self.default_model.clone();

                    // Track in recent models
                    let model_key = format!(
                        "{}/{}",
                        prev_provider,
                        self.default_model.as_deref().unwrap_or("auto")
                    );
                    self.recent_models.retain(|m| m != &model_key);
                    self.recent_models.insert(0, model_key);
                    self.recent_models.truncate(20);

                    self.add_system_message(&format!(
                        "Provider: {prev_provider} | Model: {}",
                        self.default_model.as_deref().unwrap_or("auto")
                    ));
                } else {
                    let agents = ["build", "plan", "general"];
                    let idx = agents
                        .iter()
                        .position(|a| *a == self.current_agent)
                        .unwrap_or(0);
                    self.current_agent = agents[(idx + agents.len() - 1) % agents.len()].into();
                    self.add_system_message(&format!("Agent: {}", self.current_agent));
                }
            }
            TuiAction::AgentList => {
                // Open agent selector via dialog stack
                self.dialog.push(DialogType::AgentSelector);
                self.input.focused = false;
            }
            TuiAction::ModelList => {
                // Open model selector dialog with provider data
                let provider_map: std::collections::HashMap<
                    String,
                    rustcode_core::provider::ProviderInfo,
                > = std::collections::HashMap::new();
                self.model_selector.show(
                    provider_map,
                    self.default_provider.clone(),
                    self.default_model.clone(),
                );
                self.input.focused = false;
            }
            TuiAction::ModelCycleRecent => {
                // Cycle through recently used models
                if self.recent_models.is_empty() {
                    self.add_system_message("No recent models. Use a model first.");
                } else {
                    // Move the first model to the end (cycle forward)
                    let first = self.recent_models.remove(0);
                    self.recent_models.push(first.clone());
                    // Apply the new first model
                    let parts: Vec<&str> = first.splitn(2, '/').collect();
                    if parts.len() == 2 {
                        self.default_provider = Some(parts[0].to_string());
                        self.default_model = Some(parts[1].to_string());
                        self.status.provider_name = Some(parts[0].to_string());
                        self.status.model_name = Some(parts[1].to_string());
                        self.add_system_message(&format!("Model: {}/{}", parts[0], parts[1]));
                    }
                }
            }
            TuiAction::ModelCycleRecentReverse => {
                if self.recent_models.is_empty() {
                    self.add_system_message("No recent models. Use a model first.");
                } else {
                    // Move the last model to the front (cycle backward)
                    let last = self.recent_models.pop().expect("vec is non-empty");
                    self.recent_models.insert(0, last.clone());
                    let parts: Vec<&str> = last.splitn(2, '/').collect();
                    if parts.len() == 2 {
                        self.default_provider = Some(parts[0].to_string());
                        self.default_model = Some(parts[1].to_string());
                        self.status.provider_name = Some(parts[0].to_string());
                        self.status.model_name = Some(parts[1].to_string());
                        self.add_system_message(&format!("Model: {}/{}", parts[0], parts[1]));
                    }
                }
            }
            TuiAction::VariantCycle => {
                let variants = ["default", "thinking", "long-context"];
                let current = self.current_model_name.as_str();
                let idx = variants.iter().position(|v| *v == current).unwrap_or(0);
                let next = variants[(idx + 1) % variants.len()];
                self.current_model_name = next.to_string();
                self.status.model_name = Some(format!(
                    "{}:{}",
                    self.default_model.as_deref().unwrap_or("auto"),
                    next
                ));
                self.add_system_message(&format!("Variant: {next}"));
            }
            TuiAction::VariantList => {
                self.add_system_message("Variants: default, thinking, long-context");
            }

            // ── Provider ─────────────────────────────────────────
            TuiAction::ProviderConnect => {
                self.status.connected = true;
                self.status.show_welcome = false;
                self.add_system_message("Provider connected.");
            }

            // ── Theme ────────────────────────────────────────────
            TuiAction::ThemeSwitch => {
                if let Some(name) = self.theme.cycle_theme(1) {
                    self.add_system_message(&format!("Theme: {name}"));
                    tracing::info!(name, "theme switched");
                } else {
                    self.add_system_message("Theme is locked. Unlock to switch.");
                }
            }
            TuiAction::ThemeSwitchMode => {
                if let Some(name) = self.theme.toggle_mode() {
                    let mode = self.theme.mode().as_str();
                    self.add_system_message(&format!("Theme: {name} ({mode} mode)"));
                    tracing::info!(name, mode, "theme mode toggled");
                } else {
                    self.add_system_message("Theme is locked. Unlock to switch mode.");
                }
            }

            // ── Toggles ──────────────────────────────────────────
            TuiAction::ToggleSidebar => {
                self.sidebar_state.toggle();
                self.show_sidebar = self.sidebar_state.visible;
                self.add_system_message(&format!(
                    "Sidebar: {}",
                    if self.sidebar_state.visible {
                        "shown"
                    } else {
                        "hidden"
                    }
                ));
            }
            TuiAction::ToggleTimestamps => {
                self.show_timestamps = !self.show_timestamps;
                self.add_system_message(&format!(
                    "Timestamps: {}",
                    if self.show_timestamps {
                        "shown"
                    } else {
                        "hidden"
                    }
                ));
            }
            TuiAction::ToggleThinking => {
                self.show_thinking = !self.show_thinking;
                self.add_system_message(&format!(
                    "Thinking: {}",
                    if self.show_thinking {
                        "visible"
                    } else {
                        "hidden"
                    }
                ));
            }
            TuiAction::ToggleToolDetails => {
                self.show_tool_details = !self.show_tool_details;
                self.add_system_message(&format!(
                    "Tool details: {}",
                    if self.show_tool_details {
                        "shown"
                    } else {
                        "hidden"
                    }
                ));
            }
            TuiAction::ToggleConceal => {
                self.conceal_enabled = !self.conceal_enabled;
                self.add_system_message(&format!(
                    "Conceal: {}",
                    if self.conceal_enabled {
                        "enabled"
                    } else {
                        "disabled"
                    }
                ));
            }
            TuiAction::ToggleScrollbar => {
                self.show_scrollbar = !self.show_scrollbar;
                self.add_system_message(&format!(
                    "Scrollbar: {}",
                    if self.show_scrollbar {
                        "shown"
                    } else {
                        "hidden"
                    }
                ));
            }
            TuiAction::ToggleGenericToolOutput => {
                self.generic_tool_output = !self.generic_tool_output;
                self.add_system_message(&format!(
                    "Generic tool output: {}",
                    if self.generic_tool_output {
                        "shown"
                    } else {
                        "hidden"
                    }
                ));
            }
            TuiAction::ToggleTerminalTitle => {
                self.terminal_title = !self.terminal_title;
                self.add_system_message(&format!(
                    "Terminal title: {}",
                    if self.terminal_title {
                        "enabled"
                    } else {
                        "disabled"
                    }
                ));
            }
            TuiAction::ToggleAnimations => {
                self.animations_enabled = !self.animations_enabled;
                self.add_system_message(&format!(
                    "Animations: {}",
                    if self.animations_enabled {
                        "enabled"
                    } else {
                        "disabled"
                    }
                ));
            }
            TuiAction::ToggleFileContext => {
                self.file_context_enabled = !self.file_context_enabled;
                self.add_system_message(&format!(
                    "File context: {}",
                    if self.file_context_enabled {
                        "shown"
                    } else {
                        "hidden"
                    }
                ));
            }
            TuiAction::ToggleDiffWrap => {
                self.diff_wrap = !self.diff_wrap;
                self.add_system_message(&format!(
                    "Diff wrap: {}",
                    if self.diff_wrap {
                        "enabled"
                    } else {
                        "disabled"
                    }
                ));
            }
            TuiAction::TogglePasteSummary => {
                self.paste_summary = !self.paste_summary;
                self.add_system_message(&format!(
                    "Paste summary: {}",
                    if self.paste_summary {
                        "enabled"
                    } else {
                        "disabled"
                    }
                ));
            }

            // ── Input ────────────────────────────────────────────
            TuiAction::InputSubmit => {
                let text = self.input.take();
                if !text.is_empty() {
                    self.input.add_to_history(&text);
                    self.handle_prompt_submit(text);
                }
            }
            TuiAction::InputClear => self.input.clear(),
            TuiAction::InputNewline => {
                self.input.insert_char('\n');
            }

            // ── Permission (direct key handling in dialog) ──────
            TuiAction::PermissionOnce => {
                if let Some(reply) = self.permission.select() {
                    self.handle_permission_reply(reply);
                }
            }
            TuiAction::PermissionAlways => {
                self.permission.selected_option = 1;
                if let Some(reply) = self.permission.select() {
                    self.handle_permission_reply(reply);
                }
            }
            TuiAction::PermissionReject => {
                self.permission.selected_option = 2;
                if let Some(reply) = self.permission.select() {
                    self.handle_permission_reply(reply);
                }
            }
            TuiAction::PermissionPrevOption => {
                self.permission.prev_option();
            }
            TuiAction::PermissionNextOption => {
                self.permission.next_option();
            }

            // ── Question ─────────────────────────────────────────
            TuiAction::QuestionSelect(n) => {
                self.question.selected_option = n as usize;
                // Simulate enter
                let key = crossterm::event::KeyEvent::new(
                    crossterm::event::KeyCode::Enter,
                    crossterm::event::KeyModifiers::NONE,
                );
                if let Some((rid, answers)) = self.question.handle_key(key) {
                    self.handle_question_reply(rid, answers);
                }
            }
            TuiAction::QuestionPrevOption => {
                let total = self.question.option_count();
                if total > 0 {
                    self.question.selected_option = if self.question.selected_option == 0 {
                        total.saturating_sub(1)
                    } else {
                        self.question.selected_option - 1
                    };
                }
            }
            TuiAction::QuestionNextOption => {
                let total = self.question.option_count();
                if total > 0 {
                    self.question.selected_option = (self.question.selected_option + 1) % total;
                }
            }
            TuiAction::QuestionPrevTab => {
                let count = self.question.tab_count();
                self.question.tab = if self.question.tab == 0 {
                    count.saturating_sub(1)
                } else {
                    self.question.tab - 1
                };
                self.question.selected_option = 0;
            }
            TuiAction::QuestionNextTab => {
                let count = self.question.tab_count();
                self.question.tab = (self.question.tab + 1) % count;
                self.question.selected_option = 0;
            }
            TuiAction::QuestionSubmit => {
                let key = crossterm::event::KeyEvent::new(
                    crossterm::event::KeyCode::Enter,
                    crossterm::event::KeyModifiers::NONE,
                );
                if let Some((rid, answers)) = self.question.handle_key(key) {
                    self.handle_question_reply(rid, answers);
                }
            }
            TuiAction::QuestionReject => {
                let key = crossterm::event::KeyEvent::new(
                    crossterm::event::KeyCode::Esc,
                    crossterm::event::KeyModifiers::NONE,
                );
                if let Some((rid, answers)) = self.question.handle_key(key) {
                    self.handle_question_reply(rid, answers);
                }
            }

            // ── Quick switch ─────────────────────────────────────
            TuiAction::QuickSwitch(n) => {
                let idx = (n as usize).saturating_sub(1); // keys 1-9 → indices 0-8
                if idx < self.pinned_sessions.len() {
                    if let Some(ref sid) = self.pinned_sessions[idx] {
                        // Publish session select event to navigate to pinned session
                        if let Some(ref bus) = self.bus {
                            let event = rustcode_core::bus::GlobalEvent::new(serde_json::json!({
                                "type": "session.select",
                                "sessionID": sid,
                            }));
                            let _ = bus.publish(event);
                        }
                        self.session_id = Some(sid.clone());
                        self.add_system_message(&format!("Switched to pinned session (slot {n})"));
                    } else {
                        // Pin the current session to this slot
                        if let Some(ref sid) = self.session_id {
                            self.pinned_sessions[idx] = Some(sid.clone());
                            self.add_system_message(&format!("Pinned current session to slot {n}"));
                        } else {
                            self.add_system_message(&format!(
                                "Slot {n} is empty. Use a session first to pin it."
                            ));
                        }
                    }
                }
            }

            // ── Dialog stack actions ──────────────────────────────
            TuiAction::DialogPush(target) => {
                match target {
                    DialogTarget::ModelSelector => {
                        let provider_map: std::collections::HashMap<
                            String,
                            rustcode_core::provider::ProviderInfo,
                        > = std::collections::HashMap::new();
                        self.model_selector.show(
                            provider_map,
                            self.default_provider.clone(),
                            self.default_model.clone(),
                        );
                    }
                    DialogTarget::AgentSelector => {
                        self.dialog.push(DialogType::AgentSelector);
                    }
                    DialogTarget::SessionList => {
                        let entries: Vec<SessionEntry> = Vec::new();
                        self.session_list_state.show(entries);
                    }
                    DialogTarget::ThemePicker => {
                        self.dialog.push(DialogType::ThemePicker);
                    }
                    DialogTarget::Export => {
                        let msg_count = self.conversation.messages.len();
                        self.export.show(self.session_id.as_deref(), msg_count);
                    }
                    DialogTarget::Timeline => {
                        self.timeline
                            .build_from_messages(&self.conversation.messages);
                        self.timeline.show();
                    }
                    DialogTarget::Subagent => {
                        // Populate subagent list from session children
                        self.subagent.show(
                            Vec::new(),
                            self.session_id.clone(),
                            Some(
                                self.session_id
                                    .as_deref()
                                    .unwrap_or("(current)")
                                    .to_string(),
                            ),
                        );
                    }
                    DialogTarget::Stash => {
                        self.dialog.push(DialogType::Stash);
                    }
                }
                self.input.focused = false;
            }
            TuiAction::SessionListDialog => {
                // Populate session list from SessionManager
                if let Some(ref sessions) = self.sessions {
                    let sessions = sessions.clone();
                    let current_id = self.session_id.clone();
                    tokio::spawn(async move {
                        match sessions.list(None).await {
                            Ok(list) => {
                                tracing::info!("session list dialog: {} sessions", list.len());
                                let _ = current_id;
                                // In full impl we'd send data back to the TUI
                            }
                            Err(e) => tracing::error!("session list failed: {e}"),
                        }
                    });
                }
                let entries: Vec<SessionEntry> = Vec::new();
                self.session_list_state.show(entries);
                self.input.focused = false;
            }
            TuiAction::DiffView => {
                self.diff.visible = !self.diff.visible;
                self.add_system_message(&format!(
                    "Diff viewer: {}",
                    if self.diff.visible { "shown" } else { "hidden" }
                ));
            }
            TuiAction::SidebarNextPanel => {
                self.sidebar_state.next_panel();
                // Sync sidebar visibility
                if !self.sidebar_state.visible {
                    self.sidebar_state.visible = true;
                    self.show_sidebar = true;
                }
            }
            TuiAction::SidebarPrevPanel => {
                self.sidebar_state.prev_panel();
                if !self.sidebar_state.visible {
                    self.sidebar_state.visible = true;
                    self.show_sidebar = true;
                }
            }

            // ── Custom command ───────────────────────────────────
            TuiAction::CustomCommand(cmd) => {
                tracing::info!("custom command: {cmd}");
                match cmd.as_str() {
                    "copy" => {
                        // Extract text from the last assistant message
                        let last_text = self
                            .conversation
                            .messages
                            .iter()
                            .rev()
                            .find(|m| matches!(m.info, MessageInfo::Assistant(_)))
                            .map(|m| {
                                m.parts
                                    .iter()
                                    .filter_map(|p| {
                                        if let Part::Text(tp) = p {
                                            Some(tp.text.as_str())
                                        } else {
                                            None
                                        }
                                    })
                                    .collect::<Vec<_>>()
                                    .join("\n")
                            })
                            .unwrap_or_default();

                        if last_text.is_empty() {
                            self.add_system_message("No assistant message to copy.");
                        } else if copy_to_clipboard(&last_text) {
                            let preview: String = last_text.chars().take(60).collect();
                            let suffix = if last_text.len() > 60 { "..." } else { "" };
                            self.add_system_message(&format!(
                                "Copied to clipboard: {preview}{suffix}"
                            ));
                        } else {
                            self.add_system_message("Clipboard tool not available. Install xclip, wl-clipboard, or xsel.");
                        }
                    }
                    _ => {
                        self.add_system_message(&format!("Custom command: {cmd}"));
                    }
                }
            }

            // ── Audio toggle ──────────────────────────────────────
            TuiAction::AudioToggle => {
                self.audio_enabled = !self.audio_enabled;
                self.add_system_message(&format!(
                    "Audio notifications: {}",
                    if self.audio_enabled {
                        "enabled"
                    } else {
                        "disabled"
                    }
                ));
            }

            // ── Open in editor ─────────────────────────────────────
            TuiAction::OpenInEditor => {
                // Restore terminal to cooked mode so the editor can use it
                let _ = disable_raw_mode();
                let _ = execute!(io::stdout(), LeaveAlternateScreen);

                // Open the current working directory or a specific file
                // For now, open the current directory — the user can navigate
                // in their editor from there. In the future, this could track
                // the "current file" from tool calls.
                let path = std::env::current_dir()
                    .ok()
                    .and_then(|d| d.to_str().map(String::from))
                    .unwrap_or_else(|| ".".to_string());

                let success = open_in_editor(&path);
                if success {
                    self.add_system_message(&format!("Editor session ended for: {path}"));
                } else {
                    self.add_system_message("Failed to launch editor. Set $EDITOR or $VISUAL.");
                }

                // Reinitialize the terminal for the TUI
                let _ = enable_raw_mode();
                let _ = execute!(io::stdout(), EnterAlternateScreen);
            }
        }
    }

    // ── Prompt Submit — the core LLM call ───────────────────────────

    /// Handle prompt submission — add user message, start LLM stream.
    fn handle_prompt_submit(&mut self, text: String) {
        tracing::info!("prompt submitted: {text}");
        self.status.session_status = Some(SessionStatus::Busy);

        // Add user message to conversation
        let now = chrono::Utc::now().timestamp_millis() as u64;
        let sid = self
            .session_id
            .clone()
            .unwrap_or_else(|| format!("ses_tui_{now}"));
        self.session_id = Some(sid.clone());

        let user_msg_id = format!("msg_user_{now}");
        self.conversation.messages.push(Message {
            info: MessageInfo::User(session::UserInfo {
                id: user_msg_id.clone(),
                session_id: sid.clone(),
                agent: Some(self.current_agent.clone()),
                model: self
                    .default_model
                    .as_ref()
                    .map(|m| session::ModelSelection {
                        id: m.clone(),
                        provider_id: self.default_provider.clone().unwrap_or_default(),
                        variant: None,
                    }),
                time: MessageTime {
                    created: now,
                    completed: Some(now),
                },
            }),
            parts: vec![Part::Text(TextPart {
                id: format!("part_text_{now}"),
                message_id: user_msg_id,
                session_id: sid.clone(),
                text: text.clone(),
                metadata: None,
                time: PartTime {
                    start: Some(now),
                    end: Some(now),
                },
            })],
        });

        // Branch on mode
        if let TuiMode::Remote {
            ref base_url,
            ref http_client,
            ..
        } = self.mode
        {
            // Remote mode: POST prompt to server
            let submit_url = format!("{base_url}/tui/submit-prompt");
            let client = http_client.clone();
            let text_clone = text.clone();

            tokio::spawn(async move {
                match client
                    .post(&submit_url)
                    .json(&serde_json::json!({ "text": text_clone }))
                    .send()
                    .await
                {
                    Ok(resp) => {
                        if !resp.status().is_success() {
                            tracing::warn!("Remote prompt submit failed: HTTP {}", resp.status());
                        }
                    }
                    Err(e) => {
                        tracing::error!("Remote prompt submit error: {e}");
                    }
                }
            });

            self.conversation
                .add_system_message("Prompt sent to remote server. Awaiting response...".into());
            // Remote mode: the SSE client will deliver events back
            return;
        }

        // Local mode: check provider and start streaming
        let provider_id = self
            .default_provider
            .clone()
            .unwrap_or_else(|| "anthropic".into());
        if !self.providers.contains_key(&provider_id) {
            self.conversation.add_system_message(format!(
                "No provider '{provider_id}' configured. Set API key env var."
            ));
            self.status.session_status = Some(SessionStatus::Idle);
            return;
        }

        // Start streaming through the main loop's LLM channel.
        self.spawn_llm_stream(text);
    }

    fn handle_permission_reply(&mut self, reply: PermissionReply) {
        let request_id = self.permission.request.as_ref().map(|r| r.id.clone());

        match &reply {
            PermissionReply::Once => tracing::info!("permission: allow once"),
            PermissionReply::Always => tracing::info!("permission: allow always"),
            PermissionReply::Reject { message } => tracing::info!("permission: reject {message:?}"),
        }

        // Remote mode: send reply via HTTP POST
        if let TuiMode::Remote {
            ref base_url,
            ref http_client,
            ..
        } = self.mode
        {
            if let Some(ref rid) = request_id {
                let reply_str = match &reply {
                    PermissionReply::Once => "once",
                    PermissionReply::Always => "always",
                    PermissionReply::Reject { .. } => "reject",
                };
                let message = match &reply {
                    PermissionReply::Reject { message } => message.clone(),
                    _ => None,
                };
                let url = format!("{base_url}/tui/control/response");
                let client = http_client.clone();
                let rid = rid.clone();

                tokio::spawn(async move {
                    let mut body = serde_json::json!({
                        "type": "permission",
                        "request_id": rid,
                        "reply": reply_str,
                    });
                    if let Some(msg) = message {
                        body["message"] = serde_json::Value::String(msg);
                    }
                    if let Err(e) = client.post(&url).json(&body).send().await {
                        tracing::error!("Remote permission reply failed: {e}");
                    }
                });
            }
            self.permission.dismiss();
            return;
        }

        // Local mode: call the permission service
        if let Some(ref perm_svc) = self.permission_service {
            if let Some(ref rid) = request_id {
                let perm_svc = perm_svc.clone();
                let rid = rid.clone();
                let core_reply = match &reply {
                    PermissionReply::Once => rustcode_core::permission::PermissionReply::Once,
                    PermissionReply::Always => rustcode_core::permission::PermissionReply::Always,
                    PermissionReply::Reject { .. } => {
                        rustcode_core::permission::PermissionReply::Reject
                    }
                };
                let message = match &reply {
                    PermissionReply::Reject { message } => message.clone(),
                    _ => None,
                };

                tokio::spawn(async move {
                    let input = ReplyInput {
                        request_id: rid,
                        reply: core_reply,
                        message,
                    };
                    if let Err(e) = perm_svc.reply(input).await {
                        tracing::error!("permission reply failed: {e}");
                    }
                });
            }
        }

        self.permission.dismiss();
    }

    fn handle_question_reply(&mut self, request_id: String, answers: Vec<Vec<String>>) {
        if answers.is_empty() {
            tracing::info!("question rejected: {request_id}");
        } else {
            tracing::info!(
                "question answered: {request_id} with {} answer groups",
                answers.len()
            );
        }

        // Remote mode: send reply via HTTP POST
        if let TuiMode::Remote {
            ref base_url,
            ref http_client,
            ..
        } = self.mode
        {
            let url = format!("{base_url}/tui/control/response");
            let client = http_client.clone();
            let rid = request_id.clone();
            let answers_clone = answers.clone();

            tokio::spawn(async move {
                let body = serde_json::json!({
                    "type": "question",
                    "request_id": rid,
                    "answers": answers_clone,
                });
                if let Err(e) = client.post(&url).json(&body).send().await {
                    tracing::error!("Remote question reply failed: {e}");
                }
            });
            self.question.dismiss();
            return;
        }

        // Local mode: publish question reply on the bus
        if let Some(ref bus) = self.bus {
            let event = rustcode_core::bus::GlobalEvent::new(serde_json::json!({
                "type": "question.replied",
                "requestID": request_id,
                "answers": answers,
            }));
            let _ = bus.publish(event);
        }

        self.question.dismiss();
    }

    // ── Conversation helpers ─────────────────────────────────────────

    /// Add a system/status message to the conversation.
    fn add_system_message(&mut self, text: &str) {
        self.conversation.add_system_message(text.to_string());
    }

    // ── Public API ───────────────────────────────────────────────────

    pub fn set_messages(
        &mut self,
        session_id: &str,
        messages: Vec<rustcode_core::session::Message>,
        parts: HashMap<String, Vec<rustcode_core::session::Part>>,
    ) {
        self.session_id = Some(session_id.into());
        self.conversation.set_messages(messages, parts);
    }

    pub fn handle_tui_event(&mut self, event: TuiEvent) {
        match event {
            TuiEvent::PromptAppend { properties } => {
                self.input.append(&properties.text);
            }
            TuiEvent::CommandExecute { .. } => {}
            TuiEvent::ToastShow { properties } => {
                tracing::info!("toast [{}]: {}", properties.variant, properties.message);
            }
            TuiEvent::SessionSelect { properties } => {
                self.session_id = Some(properties.session_id);
            }
        }
    }

    pub fn set_session_status(&mut self, status: SessionStatus) {
        self.status.session_status = Some(status);
    }
    pub fn set_connected(&mut self, connected: bool) {
        self.status.connected = connected;
        if connected {
            self.status.show_welcome = false;
        }
    }
    pub fn set_service_counts(&mut self, lsp: usize, mcp: usize, mcp_err: bool) {
        self.status.lsp_count = lsp;
        self.status.mcp_count = mcp;
        self.status.mcp_error = mcp_err;
    }
    pub fn set_permission_count(&mut self, count: usize) {
        self.status.permission_count = count;
    }
    pub fn show_permission(&mut self, req: rustcode_core::permission::PermissionRequest) {
        self.permission.show(req);
    }
    pub fn show_question(&mut self, rid: String, qs: Vec<crate::event::QuestionItem>) {
        self.question.show(rid, qs);
    }
    /// Update the tool definitions sent to the LLM on each request.
    pub fn set_tool_definitions(&mut self, defs: Vec<ToolDefinition>) {
        self.tool_definitions = defs;
    }

    pub fn get_session_id(&self) -> Option<&str> {
        self.session_id.as_deref()
    }
}

/// Convert a KeyEvent to a human-readable string.
fn key_event_to_string(key: &crossterm::event::KeyEvent) -> String {
    use crossterm::event::KeyCode;
    use crossterm::event::KeyModifiers;

    let mut parts: Vec<String> = Vec::new();

    if key.modifiers.contains(KeyModifiers::CONTROL) {
        parts.push("C-".into());
    }
    if key.modifiers.contains(KeyModifiers::ALT) {
        parts.push("M-".into());
    }
    if key.modifiers.contains(KeyModifiers::SHIFT) {
        // Only add shift for non-char keys
        match key.code {
            KeyCode::Char(_) => {}
            _ => {
                parts.push("S-".into());
            }
        }
    }

    match key.code {
        KeyCode::Char(c) => parts.push(c.to_string()),
        KeyCode::Enter => parts.push("Enter".into()),
        KeyCode::Esc => parts.push("Esc".into()),
        KeyCode::Backspace => parts.push("Backspace".into()),
        KeyCode::Tab => parts.push("Tab".into()),
        KeyCode::BackTab => parts.push("S-Tab".into()),
        KeyCode::Delete => parts.push("Delete".into()),
        KeyCode::Up => parts.push("Up".into()),
        KeyCode::Down => parts.push("Down".into()),
        KeyCode::Left => parts.push("Left".into()),
        KeyCode::Right => parts.push("Right".into()),
        KeyCode::Home => parts.push("Home".into()),
        KeyCode::End => parts.push("End".into()),
        KeyCode::PageUp => parts.push("PageUp".into()),
        KeyCode::PageDown => parts.push("PageDown".into()),
        KeyCode::F(n) => parts.push(format!("F{n}")),
        _ => parts.push("?".into()),
    }

    parts.join("")
}
