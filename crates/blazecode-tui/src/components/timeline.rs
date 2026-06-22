//! Session timeline dialog — message tree view with parent-child relationships.
//!
//! Ported from: `packages/tui/src/component/dialog-session-timeline.tsx`
//!
//! The timeline shows the session message tree (undoscope), displaying
//! parent-child relationships between messages. Users can navigate nodes,
//! preview message content on selection, and trigger fork-from-timeline.
//!
//! ## Key bindings
//!
//! | Key | Action |
//! |-----|--------|
//! | `Up` / `k` | Previous node |
//! | `Down` / `j` | Next node |
//! | `Enter` | Expand/collapse or fork at node |
//! | `Esc` | Close dialog |
//! | `Ctrl+F` | Fork session at selected node |

use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
use ratatui::{
    layout::{Constraint, Direction, Layout, Rect},
    style::{Color, Modifier, Style},
    text::{Line, Span, Text},
    widgets::{Block, Borders, Clear, List, ListItem, Paragraph, Wrap},
    Frame,
};
use std::collections::HashMap;

/// A node in the timeline tree.
#[derive(Debug, Clone)]
pub struct TimelineNode {
    /// Display label (role + truncated text or "Fork N")
    pub label: String,
    /// Depth in the tree (0 = root).
    pub depth: usize,
    /// Message ID this node represents.
    pub message_id: Option<String>,
    /// Role: "user", "assistant", or "fork".
    pub role: String,
    /// Truncated preview of the message content.
    pub preview: String,
    /// Whether this node has children (expandable).
    pub has_children: bool,
    /// Whether this node's children are currently expanded.
    pub expanded: bool,
    /// Index of this node in the flat list.
    pub index: usize,
    /// Parent index in the flat list (None for root).
    pub parent_index: Option<usize>,
}

/// State for the session timeline dialog.
#[derive(Debug, Default)]
pub struct TimelineState {
    /// Whether the dialog is visible.
    pub visible: bool,
    /// Flat list of visible nodes in the tree.
    pub nodes: Vec<TimelineNode>,
    /// Currently selected node index.
    pub selected: usize,
    /// Currently hovered node (for preview).
    pub preview_index: Option<usize>,
    /// The session ID being shown.
    pub session_id: Option<String>,
    /// Message preview panel width ratio.
    pub preview_visible: bool,
}

impl TimelineState {
    pub fn new() -> Self {
        Self {
            visible: false,
            nodes: Vec::new(),
            selected: 0,
            preview_index: None,
            session_id: None,
            preview_visible: false,
        }
    }

    /// Build the timeline from messages.
    ///
    /// `messages` should include parent_id info for each assistant message.
    /// AssistantInfo has `parent_id` pointing to the user message that spawned it.
    pub fn build_from_messages(&mut self, messages: &[blazecode_core::session::Message]) {
        self.nodes.clear();

        if messages.is_empty() {
            return;
        }

        // Collect message info into a flat list preserving encounter order.
        #[derive(Debug, Clone)]
        struct MsgMeta {
            id: String,
            role: String,
            preview: String,
            parent_id: Option<String>,
            children: Vec<usize>,
            expanded: bool,
        }

        let mut metas: Vec<MsgMeta> = Vec::new();

        for msg in messages.iter() {
            let (id, role, parent_id, preview) = match &msg.info {
                blazecode_core::session::MessageInfo::User(info) => {
                    let preview = Self::preview_from_parts(&msg.parts, 60);
                    (info.id.clone(), "user".to_string(), None::<String>, preview)
                }
                blazecode_core::session::MessageInfo::Assistant(info) => {
                    let preview = Self::preview_from_parts(&msg.parts, 60);
                    (
                        info.id.clone(),
                        "assistant".to_string(),
                        Some(info.parent_id.clone()),
                        preview,
                    )
                }
            };

            let meta = MsgMeta {
                id,
                role,
                preview,
                parent_id,
                children: Vec::new(),
                expanded: true, // Start expanded
            };
            metas.push(meta);
        }

        // Build child references
        let id_to_idx: HashMap<String, usize> = metas
            .iter()
            .enumerate()
            .map(|(i, m)| (m.id.clone(), i))
            .collect();

        for i in 0..metas.len() {
            if let Some(ref parent_id) = metas[i].parent_id {
                if let Some(&p_idx) = id_to_idx.get(parent_id) {
                    if p_idx < metas.len() {
                        metas[p_idx].children.push(i);
                    }
                }
            }
        }

        // Flatten tree with indentation (pre-order traversal)
        let mut flat_nodes: Vec<TimelineNode> = Vec::new();

        // Find roots: messages with either no parent_id or parent_id not in our list
        let root_indices: Vec<usize> = metas
            .iter()
            .enumerate()
            .filter(|(_, m)| {
                m.parent_id
                    .as_ref()
                    .map(|pid| !id_to_idx.contains_key(pid))
                    .unwrap_or(true)
            })
            .map(|(i, _)| i)
            .collect();

        fn flatten_recursive(
            metas: &[MsgMeta],
            node_idx: usize,
            depth: usize,
            flat_nodes: &mut Vec<TimelineNode>,
            parent_flat_idx: Option<usize>,
        ) {
            let meta = &metas[node_idx];

            let label = format!(
                "{} {}",
                TimelineState::role_icon(&meta.role),
                &meta.preview[..meta.preview.len().min(50)]
            );

            let node = TimelineNode {
                label,
                depth,
                message_id: Some(meta.id.clone()),
                role: meta.role.clone(),
                preview: meta.preview.clone(),
                has_children: !meta.children.is_empty(),
                expanded: meta.expanded,
                index: flat_nodes.len(),
                parent_index: parent_flat_idx,
            };

            let my_flat_idx = flat_nodes.len();
            flat_nodes.push(node);

            if meta.expanded {
                for &child_idx in &meta.children {
                    flatten_recursive(metas, child_idx, depth + 1, flat_nodes, Some(my_flat_idx));
                }
            }
        }

        for &root_idx in &root_indices {
            flatten_recursive(&metas, root_idx, 0, &mut flat_nodes, None);
        }

        self.nodes = flat_nodes;
        if self.selected >= self.nodes.len() {
            self.selected = self.nodes.len().saturating_sub(1);
        }
    }

    /// Extract a short text preview from message parts.
    fn preview_from_parts(parts: &[blazecode_core::session::Part], max_len: usize) -> String {
        for part in parts {
            if let blazecode_core::session::Part::Text(tp) = part {
                let text = tp.text.trim();
                if text.len() <= max_len {
                    return text.to_string();
                }
                return format!("{}...", &text[..text.len().min(max_len - 3)]);
            }
        }
        "(no text)".to_string()
    }

    /// Show the dialog.
    pub fn show(&mut self) {
        self.visible = true;
        self.selected = 0;
        self.preview_visible = false;
    }

    /// Hide the dialog.
    pub fn hide(&mut self) {
        self.visible = false;
    }

    /// Select the next node.
    pub fn next(&mut self) {
        if !self.nodes.is_empty() {
            self.selected = (self.selected + 1) % self.nodes.len();
            self.preview_index = Some(self.selected);
        }
    }

    /// Select the previous node.
    pub fn prev(&mut self) {
        if !self.nodes.is_empty() {
            self.selected = if self.selected == 0 {
                self.nodes.len().saturating_sub(1)
            } else {
                self.selected - 1
            };
            self.preview_index = Some(self.selected);
        }
    }

    /// Toggle expand/collapse at the selected node.
    pub fn toggle_expand(&mut self) {
        if let Some(node) = self.nodes.get(self.selected) {
            if node.has_children {
                // Need message reference to toggle. In full impl we'd mutate the source metas.
                // For now, we signal that the expansion changed.
            }
        }
    }

    /// Get the selected message ID, if any.
    pub fn selected_message_id(&self) -> Option<String> {
        self.nodes
            .get(self.selected)
            .and_then(|n| n.message_id.clone())
    }

    /// Handle a key event. Returns the action to take.
    pub fn handle_key(&mut self, key: KeyEvent) -> Option<TimelineAction> {
        if !self.visible {
            return None;
        }

        match key {
            KeyEvent {
                code: KeyCode::Esc, ..
            } => {
                self.hide();
                Some(TimelineAction::Close)
            }

            KeyEvent {
                code: KeyCode::Up, ..
            }
            | KeyEvent {
                code: KeyCode::Char('k'),
                modifiers: KeyModifiers::NONE,
                ..
            } => {
                self.prev();
                Some(TimelineAction::Navigate)
            }

            KeyEvent {
                code: KeyCode::Down,
                ..
            }
            | KeyEvent {
                code: KeyCode::Char('j'),
                modifiers: KeyModifiers::NONE,
                ..
            } => {
                self.next();
                Some(TimelineAction::Navigate)
            }

            KeyEvent {
                code: KeyCode::Enter,
                modifiers: KeyModifiers::NONE,
                ..
            } => {
                self.toggle_expand();
                if let Some(msg_id) = self.selected_message_id() {
                    Some(TimelineAction::Select(msg_id))
                } else {
                    Some(TimelineAction::Navigate)
                }
            }

            KeyEvent {
                code: KeyCode::Char('f'),
                modifiers: KeyModifiers::CONTROL,
                ..
            } => {
                let msg_id = self.selected_message_id();
                // Fork at the selected message
                Some(TimelineAction::Fork(msg_id))
            }

            KeyEvent {
                code: KeyCode::Char('p'),
                modifiers: KeyModifiers::NONE,
                ..
            } => {
                self.preview_visible = !self.preview_visible;
                Some(TimelineAction::Navigate)
            }

            _ => Some(TimelineAction::Navigate),
        }
    }

    /// Get display icon for a message role.
    pub fn role_icon(role: &str) -> &'static str {
        match role {
            "user" => "U",
            "assistant" => "A",
            "fork" => "F",
            _ => "?",
        }
    }

    /// Get display color for a message role.
    pub fn role_color(role: &str) -> Color {
        match role {
            "user" => Color::Green,
            "assistant" => Color::Cyan,
            "fork" => Color::Yellow,
            _ => Color::Gray,
        }
    }
}

/// Actions returned by the timeline key handler.
#[derive(Debug, Clone)]
pub enum TimelineAction {
    /// Close the dialog.
    Close,
    /// Navigation occurred (redraw needed).
    Navigate,
    /// Select a message by ID.
    Select(String),
    /// Fork the session at this message.
    Fork(Option<String>),
}

/// Render the timeline as a modal dialog.
pub fn render_timeline(f: &mut Frame, area: Rect, state: &TimelineState) {
    if !state.visible {
        return;
    }

    let dialog_width = (area.width as f64 * 0.72).clamp(50.0, 100.0) as u16;
    let dialog_height = (area.height as f64 * 0.75).clamp(15.0, 40.0) as u16;
    let dialog_x = (area.width.saturating_sub(dialog_width)) / 2;
    let dialog_y = (area.height.saturating_sub(dialog_height)) / 4;

    let dialog_area = Rect::new(
        area.x + dialog_x,
        area.y + dialog_y,
        dialog_width,
        dialog_height,
    );

    f.render_widget(Clear, dialog_area);

    let node_count = state.nodes.len();
    let title = format!(" Session Timeline ({} messages) ", node_count);

    let block = Block::default()
        .borders(Borders::ALL)
        .title(title)
        .title_bottom(" j/k:nav  Enter:select  Ctrl+F:fork  p:preview  Esc:close ")
        .border_style(Style::default().fg(Color::Cyan))
        .style(Style::default().bg(Color::Black));

    let inner = block.inner(dialog_area);
    f.render_widget(block, dialog_area);

    // Split: tree view (left) and optional preview (right)
    let has_preview = state.preview_visible && state.selected < state.nodes.len();
    let constraints: Vec<Constraint> = if has_preview {
        vec![Constraint::Percentage(50), Constraint::Percentage(50)]
    } else {
        vec![Constraint::Percentage(100)]
    };

    let columns = Layout::default()
        .direction(Direction::Horizontal)
        .constraints(constraints)
        .split(inner);

    // ── Tree view (left) ───────────────────────────────────────
    if state.nodes.is_empty() {
        let msg = Paragraph::new("No messages in this session.")
            .style(Style::default().fg(Color::DarkGray))
            .wrap(Wrap { trim: true });
        f.render_widget(msg, columns[0]);
    } else {
        let items: Vec<ListItem> = state
            .nodes
            .iter()
            .enumerate()
            .map(|(i, node)| {
                let is_selected = i == state.selected;
                let role_color = TimelineState::role_color(&node.role);

                let row_style = if is_selected {
                    Style::default().fg(Color::Black).bg(role_color)
                } else {
                    Style::default().fg(role_color)
                };

                // Indentation with tree-drawing characters
                let mut prefix = String::new();
                if node.depth > 0 {
                    for d in 0..node.depth {
                        if d == node.depth - 1 {
                            if node.has_children {
                                if node.expanded {
                                    prefix.push_str(" |-");
                                } else {
                                    prefix.push_str(" |+");
                                }
                            } else {
                                prefix.push_str(" `-");
                            }
                        } else {
                            prefix.push_str(" | ");
                        }
                    }
                }

                let expander = if node.has_children {
                    if node.expanded {
                        "[-] "
                    } else {
                        "[+] "
                    }
                } else {
                    "    "
                };

                let role_tag = format!("[{}]", TimelineState::role_icon(&node.role));
                let preview = &node.preview;

                // Truncate preview to fit
                let max_preview = 45usize.saturating_sub(node.depth * 2);
                let preview_text = if preview.len() > max_preview {
                    format!("{}...", &preview[..max_preview.saturating_sub(3)])
                } else {
                    preview.to_string()
                };

                let line = Line::from(vec![
                    Span::styled(
                        prefix,
                        if is_selected {
                            row_style
                        } else {
                            Style::default().fg(Color::DarkGray)
                        },
                    ),
                    Span::styled(expander, row_style),
                    Span::styled(role_tag, row_style.add_modifier(Modifier::BOLD)),
                    Span::raw(" "),
                    Span::styled(preview_text, row_style),
                ]);

                ListItem::new(line)
            })
            .collect();

        let list = List::new(items);
        f.render_widget(list, columns[0]);
    }

    // ── Preview pane (right) ──────────────────────────────────
    if has_preview {
        let preview_area = columns[1];
        let preview_block = Block::default()
            .borders(Borders::LEFT)
            .title(" Message Preview ")
            .border_style(Style::default().fg(Color::Blue));

        let preview_inner = preview_block.inner(preview_area);
        f.render_widget(preview_block, preview_area);

        if let Some(node) = state.nodes.get(state.selected) {
            let mut preview_lines: Vec<Line> = Vec::new();
            preview_lines.push(Line::from(vec![
                Span::styled("Role: ", Style::default().fg(Color::Gray)),
                Span::styled(
                    &node.role,
                    Style::default()
                        .fg(TimelineState::role_color(&node.role))
                        .add_modifier(Modifier::BOLD),
                ),
            ]));
            if let Some(ref mid) = node.message_id {
                preview_lines.push(Line::from(vec![
                    Span::styled("ID:   ", Style::default().fg(Color::Gray)),
                    Span::styled(mid, Style::default().fg(Color::DarkGray)),
                ]));
            }
            preview_lines.push(Line::from(vec![
                Span::styled("Depth:", Style::default().fg(Color::Gray)),
                Span::styled(
                    format!(" {}", node.depth),
                    Style::default().fg(Color::White),
                ),
            ]));
            preview_lines.push(Line::from(vec![Span::styled(
                if node.has_children {
                    "Children: yes"
                } else {
                    "Children: no"
                },
                Style::default().fg(Color::Gray),
            )]));
            preview_lines.push(Line::from(""));
            preview_lines.push(Line::from(Span::styled(
                "─".repeat(preview_inner.width.max(10) as usize),
                Style::default().fg(Color::DarkGray),
            )));
            preview_lines.push(Line::from(Span::styled(
                &node.preview,
                Style::default().fg(Color::White),
            )));

            let text = Text::from(preview_lines);
            f.render_widget(
                Paragraph::new(text).wrap(Wrap { trim: true }),
                preview_inner,
            );
        }
    }
}
