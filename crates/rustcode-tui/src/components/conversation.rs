//! Scrolling conversation view — displays session messages.
//!
//! Ported from: `packages/tui/src/routes/session/index.tsx` (lines 176–1350)
//!
//! The conversation view renders a scrollable list of messages (user and
//! assistant), with parts (text, tool, reasoning) rendered inline.

use ratatui::{
    layout::{Constraint, Layout, Rect},
    style::{Color, Modifier, Style},
    text::{Line, Span, Text},
    widgets::{Block, Borders, List, ListItem, Paragraph, Wrap},
    Frame,
};
use rustcode_core::session::{Message, MessageInfo, Part};
use std::collections::HashMap;

/// State for the conversation view.
#[derive(Debug, Default)]
pub struct ConversationState {
    /// Scroll offset (0 = bottom, increasing = scrolling up).
    pub scroll_offset: u16,
    /// Whether auto-scroll is enabled (follows new messages).
    pub auto_scroll: bool,
    /// Messages to display.
    pub messages: Vec<Message>,
    /// Parts keyed by message ID.
    pub parts: HashMap<String, Vec<Part>>,
}

impl ConversationState {
    pub fn new() -> Self {
        Self {
            scroll_offset: 0,
            auto_scroll: true,
            messages: Vec::new(),
            parts: HashMap::new(),
        }
    }

    /// Set messages and their parts.
    pub fn set_messages(&mut self, messages: Vec<Message>, parts_map: HashMap<String, Vec<Part>>) {
        self.messages = messages;
        self.parts = parts_map;
    }

    /// Scroll up by one line.
    pub fn scroll_up(&mut self, amount: u16) {
        self.scroll_offset = self.scroll_offset.saturating_add(amount);
        self.auto_scroll = false;
    }

    /// Scroll down by one line.
    pub fn scroll_down(&mut self, amount: u16) {
        self.scroll_offset = self.scroll_offset.saturating_sub(amount);
        if self.scroll_offset == 0 {
            self.auto_scroll = true;
        }
    }

    /// Scroll to the bottom (latest message).
    pub fn scroll_to_bottom(&mut self) {
        self.scroll_offset = 0;
        self.auto_scroll = true;
    }

    /// Scroll to the top (first message).
    pub fn scroll_to_top(&mut self) {
        self.scroll_offset = u16::MAX;
        self.auto_scroll = false;
    }
}

/// Render the conversation view into the given frame area.
pub fn render_conversation(f: &mut Frame, area: Rect, state: &ConversationState) {
    let messages = &state.messages;
    if messages.is_empty() {
        let welcome = Paragraph::new(Text::from(vec![
            Line::from(Span::styled(
                "Welcome to rustcode!",
                Style::default().fg(Color::Cyan).add_modifier(Modifier::BOLD),
            )),
            Line::from(""),
            Line::from(Span::styled(
                "Type a message below to get started.",
                Style::default().fg(Color::Gray),
            )),
            Line::from(Span::styled(
                "Ctrl+C to exit, Ctrl+P for commands.",
                Style::default().fg(Color::DarkGray),
            )),
        ]))
        .block(Block::default().borders(Borders::NONE))
        .wrap(Wrap { trim: true });

        f.render_widget(welcome, area);
        return;
    }

    // Build list items from messages
    let items: Vec<ListItem> = messages
        .iter()
        .flat_map(|msg| {
            let parts = state.parts.get(msg.info.id()).cloned().unwrap_or_default();
            build_message_items(msg, &parts)
        })
        .collect();

    let list = List::new(items)
        .block(Block::default().borders(Borders::NONE));

    f.render_widget(list, area);
}

/// Build renderable list items from a message and its parts.
fn build_message_items(msg: &Message, parts: &[Part]) -> Vec<ListItem> {
    let mut items = Vec::new();

    match &msg.info {
        MessageInfo::User(user_info) => {
            // User message — render text parts
            let texts: Vec<String> = parts
                .iter()
                .filter_map(|p| {
                    if let Part::Text(tp) = p {
                        Some(tp.text.clone())
                    } else {
                        None
                    }
                })
                .collect();

            let content = if texts.is_empty() {
                "(empty message)".to_string()
            } else {
                texts.join("\n")
            };

            let line = Line::from(vec![
                Span::styled(
                    " You ",
                    Style::default()
                        .fg(Color::Black)
                        .bg(Color::Cyan)
                        .add_modifier(Modifier::BOLD),
                ),
                Span::raw(" "),
                Span::styled(&content, Style::default().fg(Color::White)),
            ]);

            items.push(ListItem::new(line));
        }

        MessageInfo::Assistant(assistant_info) => {
            // Assistant header
            let agent = &assistant_info.agent;
            let model = assistant_info
                .model_id
                .as_deref()
                .unwrap_or("unknown");

            let header = vec![
                Span::styled(
                    format!(" {} ", agent),
                    Style::default()
                        .fg(Color::Black)
                        .bg(Color::Magenta)
                        .add_modifier(Modifier::BOLD),
                ),
                Span::raw(" "),
                Span::styled(model, Style::default().fg(Color::Gray)),
            ];

            if let Some(finish) = &assistant_info.finish {
                items.push(ListItem::new(Line::from(vec![
                    Span::styled(
                        format!(" {} ", agent),
                        Style::default()
                            .fg(Color::Black)
                            .bg(Color::Magenta)
                            .add_modifier(Modifier::BOLD),
                    ),
                    Span::raw(" "),
                    Span::styled(model, Style::default().fg(Color::Gray)),
                    Span::raw(" · "),
                    Span::styled(finish, Style::default().fg(Color::DarkGray)),
                ])));
            } else {
                items.push(ListItem::new(Line::from(header)));
            }

            // Render parts
            for part in parts {
                match part {
                    Part::Text(tp) => {
                        let text = tp.text.trim();
                        if !text.is_empty() {
                            items.push(ListItem::new(Line::from(
                                Span::styled(text, Style::default().fg(Color::White)),
                            )));
                        }
                    }
                    Part::Tool(tool) => {
                        let status_icon = match &tool.state {
                            rustcode_core::session::ToolState::Pending { .. } => "⏳",
                            rustcode_core::session::ToolState::Running { .. } => "⟳",
                            rustcode_core::session::ToolState::Completed { ref title, .. } => {
                                items.push(ListItem::new(Line::from(vec![
                                    Span::styled(" ✓ ", Style::default().fg(Color::Green)),
                                    Span::styled(
                                        format!("{} — {}", tool.tool, title),
                                        Style::default().fg(Color::Gray),
                                    ),
                                ])));
                                continue;
                            }
                            rustcode_core::session::ToolState::Error { ref error, .. } => {
                                items.push(ListItem::new(Line::from(vec![
                                    Span::styled(" ✗ ", Style::default().fg(Color::Red)),
                                    Span::styled(
                                        format!("{} — {}", tool.tool, error),
                                        Style::default().fg(Color::Red),
                                    ),
                                ])));
                                continue;
                            }
                        };
                        items.push(ListItem::new(Line::from(vec![
                            Span::styled(
                                format!(" {} {}...", status_icon, tool.tool),
                                Style::default().fg(Color::Gray),
                            ),
                        ])));
                    }
                    Part::Reasoning(rp) => {
                        let summary: String = rp
                            .text
                            .lines()
                            .take(1)
                            .map(|l| l.chars().take(80).collect())
                            .collect();
                        items.push(ListItem::new(Line::from(vec![
                            Span::styled(" Thought: ", Style::default().fg(Color::Yellow)),
                            Span::styled(&summary, Style::default().fg(Color::DarkGray)),
                        ])));
                    }
                    Part::StepStart(_) => {
                        items.push(ListItem::new(Line::from(
                            Span::styled(" ── Step started ──", Style::default().fg(Color::DarkGray)),
                        )));
                    }
                    Part::StepFinish(sf) => {
                        items.push(ListItem::new(Line::from(vec![
                            Span::styled(" ── Step finished: ", Style::default().fg(Color::DarkGray)),
                            Span::styled(&sf.reason, Style::default().fg(Color::Gray)),
                            Span::styled(
                                format!(
                                    " ({} in / {} out tokens, ${:.4})",
                                    sf.tokens.input, sf.tokens.output, sf.cost
                                ),
                                Style::default().fg(Color::DarkGray),
                            ),
                        ])));
                    }
                    _ => {}
                }
            }

            // Error display
            if let Some(ref error) = assistant_info.error {
                let err_msg = error
                    .get("message")
                    .and_then(|m| m.as_str())
                    .unwrap_or("unknown error");
                items.push(ListItem::new(Line::from(
                    Span::styled(format!(" Error: {err_msg}"), Style::default().fg(Color::Red)),
                )));
            }

            // Separator
            items.push(ListItem::new(Line::from("")));
        }
    }

    items
}
