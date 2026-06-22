//! Subagent dialog — manage child/parent session relationships and spawn subagents.
//!
//! Ported from: `packages/tui/src/component/dialog-subagent.tsx`
//!
//! The subagent dialog shows parent-child session relationships, lets users
//! spawn new subagents (child sessions), shows subagent execution status, and
//! allows navigation to subagent sessions.
//!
//! ## Key bindings
//!
//! | Key | Action |
//! |-----|--------|
//! | `Up` / `k` | Previous subagent |
//! | `Down` / `j` | Next subagent |
//! | `Enter` | Navigate to selected subagent |
//! | `n` | Spawn new subagent |
//! | `Esc` | Close dialog |
//! | `Tab` | Switch between subagent list and spawn form |

use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
use ratatui::{
    layout::{Constraint, Direction, Layout, Rect},
    style::{Color, Modifier, Style},
    text::{Line, Span, Text},
    widgets::{Block, Borders, Clear, List, ListItem, Paragraph, Wrap},
    Frame,
};

/// Status of a subagent session.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SubagentStatus {
    /// Currently running / streaming.
    Running,
    /// Completed successfully.
    Completed,
    /// Completed with an error.
    Error,
    /// Idle (created but not yet started).
    Idle,
    /// Unknown status.
    Unknown,
}

impl SubagentStatus {
    pub fn icon(&self) -> &'static str {
        match self {
            SubagentStatus::Running => "●",
            SubagentStatus::Completed => "✓",
            SubagentStatus::Error => "✗",
            SubagentStatus::Idle => "○",
            SubagentStatus::Unknown => "?",
        }
    }

    pub fn color(&self) -> Color {
        match self {
            SubagentStatus::Running => Color::Yellow,
            SubagentStatus::Completed => Color::Green,
            SubagentStatus::Error => Color::Red,
            SubagentStatus::Idle => Color::DarkGray,
            SubagentStatus::Unknown => Color::Gray,
        }
    }

    pub fn label(&self) -> &'static str {
        match self {
            SubagentStatus::Running => "running",
            SubagentStatus::Completed => "completed",
            SubagentStatus::Error => "error",
            SubagentStatus::Idle => "idle",
            SubagentStatus::Unknown => "unknown",
        }
    }
}

/// An entry in the subagent list.
#[derive(Debug, Clone)]
pub struct SubagentEntry {
    /// Session ID of the subagent.
    pub session_id: String,
    /// Display title.
    pub title: String,
    /// Agent type (build, plan, general, etc.)
    pub agent: String,
    /// Current status.
    pub status: SubagentStatus,
    /// Number of messages.
    pub message_count: usize,
    /// Whether this is a direct child of the current session.
    pub is_direct_child: bool,
    /// Depth in the session tree (0 = direct child).
    pub depth: usize,
}

/// Which panel has focus in the subagent dialog.
#[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
pub enum SubagentFocus {
    /// Navigating the subagent list.
    #[default]
    SubagentList,
    /// Filling in the spawn form.
    SpawnForm,
    /// Selecting the agent type in the spawn form.
    AgentSelect,
}

/// State for the subagent dialog.
#[derive(Debug, Default)]
pub struct SubagentState {
    /// Whether the dialog is visible.
    pub visible: bool,
    /// List of subagent entries.
    pub subagents: Vec<SubagentEntry>,
    /// Currently selected subagent index.
    pub selected: usize,
    /// Which panel has focus.
    pub focus: SubagentFocus,
    /// Spawn form: selected agent type.
    pub spawn_agent: String,
    /// Spawn form: task description / prompt.
    pub spawn_task: String,
    /// Spawn form: model override (None = use parent's model).
    pub spawn_model: Option<String>,
    /// Available agent types.
    pub available_agents: Vec<String>,
    /// Agent selection index within the spawn form.
    pub agent_select_idx: usize,
    /// Parent session ID.
    pub parent_session_id: Option<String>,
    /// Parent session title.
    pub parent_title: Option<String>,
}

impl SubagentState {
    pub fn new() -> Self {
        Self {
            visible: false,
            subagents: Vec::new(),
            selected: 0,
            focus: SubagentFocus::SubagentList,
            spawn_agent: "build".to_string(),
            spawn_task: String::new(),
            spawn_model: None,
            available_agents: vec![
                "build".to_string(),
                "plan".to_string(),
                "general".to_string(),
                "explore".to_string(),
                "review".to_string(),
            ],
            agent_select_idx: 0,
            parent_session_id: None,
            parent_title: None,
        }
    }

    /// Show the dialog with subagent data.
    pub fn show(
        &mut self,
        subagents: Vec<SubagentEntry>,
        parent_session_id: Option<String>,
        parent_title: Option<String>,
    ) {
        self.visible = true;
        self.subagents = subagents;
        self.selected = 0;
        self.focus = SubagentFocus::SubagentList;
        self.spawn_task.clear();
        self.spawn_model = None;
        self.parent_session_id = parent_session_id;
        self.parent_title = parent_title;
    }

    /// Hide the dialog.
    pub fn hide(&mut self) {
        self.visible = false;
    }

    /// Select the next subagent.
    pub fn next(&mut self) {
        if !self.subagents.is_empty() {
            self.selected = (self.selected + 1) % self.subagents.len();
        }
    }

    /// Select the previous subagent.
    pub fn prev(&mut self) {
        if !self.subagents.is_empty() {
            self.selected = if self.selected == 0 {
                self.subagents.len().saturating_sub(1)
            } else {
                self.selected - 1
            };
        }
    }

    /// Get the selected subagent.
    pub fn selected_subagent(&self) -> Option<&SubagentEntry> {
        self.subagents.get(self.selected)
    }

    /// Cycle to the next available agent type in the spawn form.
    pub fn next_agent(&mut self) {
        if !self.available_agents.is_empty() {
            self.agent_select_idx = (self.agent_select_idx + 1) % self.available_agents.len();
            self.spawn_agent = self.available_agents[self.agent_select_idx].clone();
        }
    }

    /// Handle a key event. Returns the action to take.
    pub fn handle_key(&mut self, key: KeyEvent) -> Option<SubagentAction> {
        if !self.visible {
            return None;
        }

        match key {
            KeyEvent {
                code: KeyCode::Esc, ..
            } => {
                // If in spawn form, go back to list; else close
                if self.focus != SubagentFocus::SubagentList {
                    self.focus = SubagentFocus::SubagentList;
                    Some(SubagentAction::Navigate)
                } else {
                    self.hide();
                    Some(SubagentAction::Close)
                }
            }

            KeyEvent {
                code: KeyCode::Tab,
                modifiers: KeyModifiers::NONE,
                ..
            } => {
                self.focus = match self.focus {
                    SubagentFocus::SubagentList => SubagentFocus::SpawnForm,
                    SubagentFocus::SpawnForm => SubagentFocus::AgentSelect,
                    SubagentFocus::AgentSelect => SubagentFocus::SubagentList,
                };
                Some(SubagentAction::Navigate)
            }

            // Navigation in subagent list
            KeyEvent {
                code: KeyCode::Up, ..
            }
            | KeyEvent {
                code: KeyCode::Char('k'),
                modifiers: KeyModifiers::NONE,
                ..
            } if self.focus == SubagentFocus::SubagentList => {
                self.prev();
                Some(SubagentAction::Navigate)
            }

            KeyEvent {
                code: KeyCode::Down,
                ..
            }
            | KeyEvent {
                code: KeyCode::Char('j'),
                modifiers: KeyModifiers::NONE,
                ..
            } if self.focus == SubagentFocus::SubagentList => {
                self.next();
                Some(SubagentAction::Navigate)
            }

            // Select subagent → navigate to it
            KeyEvent {
                code: KeyCode::Enter,
                modifiers: KeyModifiers::NONE,
                ..
            } if self.focus == SubagentFocus::SubagentList => {
                let sid = self
                    .subagents
                    .get(self.selected)
                    .map(|s| s.session_id.clone());
                if let Some(ref sid) = sid {
                    self.hide();
                    Some(SubagentAction::NavigateTo(sid.clone()))
                } else {
                    Some(SubagentAction::Navigate)
                }
            }

            // Spawn new subagent
            KeyEvent {
                code: KeyCode::Char('n'),
                modifiers: KeyModifiers::NONE,
                ..
            } if self.focus == SubagentFocus::SubagentList => {
                self.focus = SubagentFocus::SpawnForm;
                self.spawn_task.clear();
                Some(SubagentAction::Navigate)
            }

            // Agent selection
            KeyEvent {
                code: KeyCode::Left,
                ..
            }
            | KeyEvent {
                code: KeyCode::Up, ..
            } if self.focus == SubagentFocus::AgentSelect => {
                if !self.available_agents.is_empty() {
                    self.agent_select_idx = if self.agent_select_idx == 0 {
                        self.available_agents.len().saturating_sub(1)
                    } else {
                        self.agent_select_idx - 1
                    };
                    self.spawn_agent = self.available_agents[self.agent_select_idx].clone();
                }
                Some(SubagentAction::Navigate)
            }

            KeyEvent {
                code: KeyCode::Right,
                ..
            }
            | KeyEvent {
                code: KeyCode::Down,
                ..
            } if self.focus == SubagentFocus::AgentSelect => {
                self.next_agent();
                Some(SubagentAction::Navigate)
            }

            // Spawn form: text input
            KeyEvent {
                code: KeyCode::Char(ch),
                modifiers: KeyModifiers::NONE | KeyModifiers::SHIFT,
                ..
            } if self.focus == SubagentFocus::SpawnForm => {
                self.spawn_task.push(ch);
                Some(SubagentAction::Navigate)
            }

            KeyEvent {
                code: KeyCode::Backspace,
                modifiers: KeyModifiers::NONE,
                ..
            } if self.focus == SubagentFocus::SpawnForm => {
                self.spawn_task.pop();
                Some(SubagentAction::Navigate)
            }

            // Submit spawn form
            KeyEvent {
                code: KeyCode::Enter,
                modifiers: KeyModifiers::NONE,
                ..
            } if self.focus == SubagentFocus::SpawnForm
                || self.focus == SubagentFocus::AgentSelect =>
            {
                if !self.spawn_task.is_empty() {
                    let task = self.spawn_task.clone();
                    self.spawn_task.clear();
                    self.focus = SubagentFocus::SubagentList;
                    Some(SubagentAction::Spawn {
                        agent: self.spawn_agent.clone(),
                        task,
                        model: self.spawn_model.clone(),
                    })
                } else {
                    Some(SubagentAction::Navigate)
                }
            }

            _ => Some(SubagentAction::Navigate),
        }
    }
}

/// Actions returned by the subagent dialog key handler.
#[derive(Debug, Clone)]
pub enum SubagentAction {
    /// Close the dialog.
    Close,
    /// Navigation occurred (redraw needed).
    Navigate,
    /// Navigate to a subagent session.
    NavigateTo(String),
    /// Spawn a new subagent.
    Spawn {
        /// Agent type.
        agent: String,
        /// Task description / prompt.
        task: String,
        /// Optional model override.
        model: Option<String>,
    },
}

/// Render the subagent dialog as a modal dialog.
pub fn render_subagent_dialog(f: &mut Frame, area: Rect, state: &SubagentState) {
    if !state.visible {
        return;
    }

    let dialog_width = (area.width as f64 * 0.65).clamp(50.0, 90.0) as u16;
    let dialog_height = (area.height as f64 * 0.65).clamp(18.0, 35.0) as u16;
    let dialog_x = (area.width.saturating_sub(dialog_width)) / 2;
    let dialog_y = (area.height.saturating_sub(dialog_height)) / 4;

    let dialog_area = Rect::new(
        area.x + dialog_x,
        area.y + dialog_y,
        dialog_width,
        dialog_height,
    );

    f.render_widget(Clear, dialog_area);

    let parent_label = state
        .parent_title
        .as_deref()
        .unwrap_or("(current)")
        .to_string();

    let title = if state.parent_session_id.is_some() {
        format!(" Subagents of \"{}\" ", parent_label)
    } else {
        " Subagents ".to_string()
    };

    let block = Block::default()
        .borders(Borders::ALL)
        .title(title)
        .title_bottom(" j/k:nav  Enter:select  n:new  Tab:switch  Esc:back/close ")
        .border_style(Style::default().fg(Color::Cyan))
        .style(Style::default().bg(Color::Black));

    let inner = block.inner(dialog_area);
    f.render_widget(block, dialog_area);

    // ── Layout ─────────────────────────────────────────────────
    let columns = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([Constraint::Percentage(50), Constraint::Percentage(50)])
        .split(inner);

    // ── Left: Subagent list ────────────────────────────────────
    let list_block_style = if state.focus == SubagentFocus::SubagentList {
        Style::default().fg(Color::Yellow)
    } else {
        Style::default().fg(Color::White)
    };

    let list_block = Block::default()
        .borders(Borders::RIGHT)
        .border_style(list_block_style)
        .title(format!(" Subagents ({}) ", state.subagents.len()));

    let list_inner = list_block.inner(columns[0]);
    f.render_widget(list_block, columns[0]);

    if state.subagents.is_empty() {
        f.render_widget(
            Paragraph::new("No subagents yet.\n\nPress 'n' to spawn one.")
                .style(Style::default().fg(Color::DarkGray))
                .wrap(Wrap { trim: true }),
            list_inner,
        );
    } else {
        let items: Vec<ListItem> = state
            .subagents
            .iter()
            .enumerate()
            .map(|(i, sub)| {
                let is_selected = i == state.selected && state.focus == SubagentFocus::SubagentList;
                let status_color = sub.status.color();
                let status_icon = sub.status.icon();

                let row_style = if is_selected {
                    Style::default().fg(Color::Black).bg(Color::Cyan)
                } else {
                    Style::default().fg(Color::White)
                };

                // Depth indentation
                let indent = "  ".repeat(sub.depth);

                // Truncate title
                let max_title = 28usize.saturating_sub(sub.depth * 2);
                let title = if sub.title.len() > max_title {
                    format!("{}...", &sub.title[..max_title - 3])
                } else {
                    sub.title.clone()
                };

                let line = Line::from(vec![
                    Span::styled(
                        indent,
                        if is_selected {
                            row_style
                        } else {
                            Style::default().fg(Color::DarkGray)
                        },
                    ),
                    Span::styled(
                        format!("{} ", status_icon),
                        if is_selected {
                            row_style
                        } else {
                            Style::default().fg(status_color)
                        },
                    ),
                    Span::styled(title, row_style.add_modifier(Modifier::BOLD)),
                    Span::raw("  "),
                    Span::styled(
                        sub.agent.clone(),
                        if is_selected {
                            Style::default().fg(Color::Black)
                        } else {
                            Style::default().fg(Color::Gray)
                        },
                    ),
                    Span::raw("  "),
                    Span::styled(
                        format!("{}m", sub.message_count),
                        if is_selected {
                            Style::default().fg(Color::Black)
                        } else {
                            Style::default().fg(Color::DarkGray)
                        },
                    ),
                ]);

                ListItem::new(line)
            })
            .collect();

        let list = List::new(items);
        f.render_widget(list, list_inner);
    }

    // ── Right: Spawn form ──────────────────────────────────────
    let spawn_cols = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(2), // Agent selector
            Constraint::Length(3), // Task input
            Constraint::Min(2),    // Info / submit
        ])
        .split(columns[1]);

    // Agent selector
    let agent_block_style = if state.focus == SubagentFocus::AgentSelect {
        Style::default().fg(Color::Yellow)
    } else {
        Style::default().fg(Color::DarkGray)
    };

    let agent_block = Block::default()
        .borders(Borders::BOTTOM)
        .border_style(agent_block_style)
        .title(" Agent ");

    let agent_inner = agent_block.inner(spawn_cols[0]);
    f.render_widget(agent_block, spawn_cols[0]);

    let agent_line = Line::from(vec![
        Span::styled(
            if state.focus == SubagentFocus::AgentSelect {
                "< "
            } else {
                "  "
            },
            agent_block_style,
        ),
        Span::styled(
            &state.spawn_agent,
            Style::default()
                .fg(Color::White)
                .add_modifier(Modifier::BOLD),
        ),
        Span::styled(
            if state.focus == SubagentFocus::AgentSelect {
                " >"
            } else {
                "  "
            },
            agent_block_style,
        ),
    ]);

    f.render_widget(Paragraph::new(agent_line), agent_inner);

    // Task input
    let task_block_style = if state.focus == SubagentFocus::SpawnForm {
        Style::default().fg(Color::Yellow)
    } else {
        Style::default().fg(Color::DarkGray)
    };

    let task_label = if state.spawn_task.is_empty() {
        "Describe the subagent's task..."
    } else {
        &state.spawn_task
    };

    let cursor = if state.focus == SubagentFocus::SpawnForm {
        " █"
    } else {
        ""
    };

    let task_line = Line::from(vec![Span::styled(
        format!("{task_label}{cursor}"),
        if state.focus == SubagentFocus::SpawnForm {
            Style::default().fg(Color::White)
        } else {
            Style::default().fg(Color::Gray)
        },
    )]);

    let task_block = Block::default()
        .borders(Borders::ALL)
        .border_style(task_block_style)
        .title(" Task ");

    let task_inner = task_block.inner(spawn_cols[1]);
    f.render_widget(task_block, spawn_cols[1]);
    f.render_widget(
        Paragraph::new(task_line).wrap(Wrap { trim: true }),
        task_inner,
    );

    // Info / status legend
    let mut info_lines: Vec<Line> = Vec::new();
    info_lines.push(Line::from(vec![
        Span::styled("● ", Style::default().fg(Color::Yellow)),
        Span::styled("running  ", Style::default().fg(Color::Gray)),
        Span::styled("✓ ", Style::default().fg(Color::Green)),
        Span::styled("completed  ", Style::default().fg(Color::Gray)),
        Span::styled("✗ ", Style::default().fg(Color::Red)),
        Span::styled("error", Style::default().fg(Color::Gray)),
    ]));
    info_lines.push(Line::from(""));
    info_lines.push(Line::from(vec![Span::styled(
        "Enter: submit  |  Esc: cancel",
        Style::default().fg(Color::DarkGray),
    )]));

    if let Some(ref mid) = state.spawn_model {
        info_lines.push(Line::from(vec![
            Span::styled("Model override: ", Style::default().fg(Color::DarkGray)),
            Span::styled(mid, Style::default().fg(Color::Yellow)),
        ]));
    }

    f.render_widget(Paragraph::new(Text::from(info_lines)), spawn_cols[2]);
}
