//! Which-key popup — shows available keybindings when a leader key is pressed.
//!
//! Ported from: `packages/tui/src/feature-plugins/system/which-key.tsx`

use ratatui::{
    layout::{Alignment, Rect},
    style::{Modifier, Style},
    text::{Line, Span, Text},
    widgets::{Block, Borders, Paragraph, Wrap},
    Frame,
};

use crate::theme::Theme;

/// State for the which-key popup.
#[derive(Debug, Clone)]
pub struct WhichKeyState {
    /// Whether the popup is currently visible.
    pub visible: bool,
    /// The leader key that was pressed (e.g., "Ctrl+P").
    pub leader: String,
    /// Available keybinding groups to display.
    pub groups: Vec<KeyGroup>,
    /// Currently selected group index (for tab navigation).
    pub selected_group: usize,
}

/// A group of related keybindings.
#[derive(Debug, Clone)]
pub struct KeyGroup {
    /// Group title (e.g., "Navigation", "Editing").
    pub title: String,
    /// Keybinding entries in this group.
    pub items: Vec<KeyItem>,
}

/// A single keybinding entry.
#[derive(Debug, Clone)]
pub struct KeyItem {
    /// Key combination (e.g., "Ctrl+O", "j/k").
    pub keys: String,
    /// Description of what this key does.
    pub description: String,
    /// Optional category hint (e.g., "file", "edit").
    pub category: Option<String>,
}

impl Default for WhichKeyState {
    fn default() -> Self {
        Self {
            visible: false,
            leader: String::new(),
            groups: Vec::new(),
            selected_group: 0,
        }
    }
}

impl WhichKeyState {
    /// Create which-key state with default keybindings.
    pub fn new() -> Self {
        let mut state = Self::default();
        state.groups = default_keybindings();
        state
    }

    /// Toggle visibility.
    pub fn toggle(&mut self) {
        self.visible = !self.visible;
        if !self.visible {
            self.leader.clear();
            self.selected_group = 0;
        }
    }

    /// Show the which-key popup with a specific leader.
    pub fn show(&mut self, leader: &str) {
        self.visible = true;
        self.leader = leader.to_string();
        self.selected_group = 0;
    }

    /// Hide the which-key popup.
    pub fn hide(&mut self) {
        self.visible = false;
        self.leader.clear();
        self.selected_group = 0;
    }

    /// Navigate to the next group.
    pub fn next_group(&mut self) {
        if !self.groups.is_empty() {
            self.selected_group = (self.selected_group + 1) % self.groups.len();
        }
    }

    /// Navigate to the previous group.
    pub fn prev_group(&mut self) {
        if !self.groups.is_empty() {
            self.selected_group = if self.selected_group == 0 {
                self.groups.len() - 1
            } else {
                self.selected_group - 1
            };
        }
    }
}

/// Default keybinding groups shown in which-key.
pub fn default_keybindings() -> Vec<KeyGroup> {
    vec![
        KeyGroup {
            title: "Navigation".into(),
            items: vec![
                KeyItem { keys: "Ctrl+P".into(), description: "Command palette".into(), category: None },
                KeyItem { keys: "Ctrl+B".into(), description: "Toggle sidebar".into(), category: None },
                KeyItem { keys: "Ctrl+L".into(), description: "Cycle models".into(), category: None },
                KeyItem { keys: "Ctrl+S".into(), description: "Toggle enhanced sidebar".into(), category: None },
                KeyItem { keys: "Ctrl+N".into(), description: "New session".into(), category: None },
                KeyItem { keys: "Ctrl+Tab".into(), description: "Next session".into(), category: None },
            ],
        },
        KeyGroup {
            title: "Conversation".into(),
            items: vec![
                KeyItem { keys: "Enter".into(), description: "Send message".into(), category: None },
                KeyItem { keys: "Shift+Enter".into(), description: "New line".into(), category: None },
                KeyItem { keys: "Esc".into(), description: "Cancel / close".into(), category: None },
                KeyItem { keys: "Ctrl+O".into(), description: "Open in editor".into(), category: None },
                KeyItem { keys: "Ctrl+Y".into(), description: "Accept diff".into(), category: None },
                KeyItem { keys: "Ctrl+N".into(), description: "Reject diff".into(), category: None },
            ],
        },
        KeyGroup {
            title: "Messages".into(),
            items: vec![
                KeyItem { keys: "↑/↓".into(), description: "Navigate messages".into(), category: None },
                KeyItem { keys: "PageUp/Down".into(), description: "Scroll conversation".into(), category: None },
                KeyItem { keys: "Home/End".into(), description: "Jump to top/bottom".into(), category: None },
                KeyItem { keys: "Ctrl+R".into(), description: "Reply to message".into(), category: None },
                KeyItem { keys: "Ctrl+E".into(), description: "Export conversation".into(), category: None },
            ],
        },
        KeyGroup {
            title: "Editing".into(),
            items: vec![
                KeyItem { keys: "Ctrl+A".into(), description: "Select all".into(), category: None },
                KeyItem { keys: "Ctrl+C".into(), description: "Copy".into(), category: None },
                KeyItem { keys: "Ctrl+V".into(), description: "Paste".into(), category: None },
                KeyItem { keys: "Ctrl+Z".into(), description: "Undo".into(), category: None },
                KeyItem { keys: "Ctrl+X".into(), description: "Cut".into(), category: None },
            ],
        },
        KeyGroup {
            title: "Dialogs".into(),
            items: vec![
                KeyItem { keys: "Ctrl+/".into(), description: "Help / keybindings".into(), category: None },
                KeyItem { keys: "Ctrl+T".into(), description: "Timeline".into(), category: None },
                KeyItem { keys: "Ctrl+D".into(), description: "Diff view".into(), category: None },
                KeyItem { keys: "Ctrl+K".into(), description: "Sub-agents".into(), category: None },
                KeyItem { keys: "Ctrl+Shift+S".into(), description: "Session list".into(), category: None },
                KeyItem { keys: "Ctrl+Shift+T".into(), description: "Theme picker".into(), category: None },
            ],
        },
    ]
}

/// Render the which-key popup.
pub fn render_which_key(f: &mut Frame, area: Rect, state: &WhichKeyState, theme: &Theme) {
    if !state.visible || state.groups.is_empty() {
        return;
    }

    // Popup dimensions
    let popup_width = 60u16.min(area.width.saturating_sub(4));
    let popup_height = 30u16.min(area.height.saturating_sub(4));

    let popup_area = Rect::new(
        area.x + (area.width.saturating_sub(popup_width)) / 2,
        area.y + (area.height.saturating_sub(popup_height)) / 2,
        popup_width,
        popup_height,
    );

    // ── Group tabs ─────────────────────────────────────────────
    let mut tab_spans: Vec<Span<'static>> = Vec::new();
    for (i, group) in state.groups.iter().enumerate() {
        if i > 0 {
            tab_spans.push(Span::styled(" │ ", Style::default().fg(theme.text_muted)));
        }
        let style = if i == state.selected_group {
            Style::default()
                .fg(theme.background)
                .bg(theme.accent)
                .add_modifier(Modifier::BOLD)
        } else {
            Style::default().fg(theme.text_muted)
        };
        tab_spans.push(Span::styled(format!(" {} ", group.title), style));
    }

    // ── Content lines ─────────────────────────────────────────
    let mut lines: Vec<Line<'static>> = Vec::new();
    lines.push(Line::from(tab_spans));

    // Separator
    lines.push(Line::from(Span::styled(
        "─".repeat(popup_width.saturating_sub(2) as usize),
        Style::default().fg(theme.border),
    )));

    // Keybinding items for selected group
    if let Some(group) = state.groups.get(state.selected_group) {
        let max_key_len = group.items.iter().map(|i| i.keys.len()).max().unwrap_or(0);

        for item in &group.items {
            let padding = " ".repeat(max_key_len.saturating_sub(item.keys.len()) + 2);
            lines.push(Line::from(vec![
                Span::styled(
                    item.keys.clone(),
                    Style::default()
                        .fg(theme.warning)
                        .add_modifier(Modifier::BOLD),
                ),
                Span::styled(padding, Style::default().fg(theme.text_muted)),
                Span::styled(
                    item.description.clone(),
                    Style::default().fg(theme.text),
                ),
            ]));
        }
    }

    // Footer help
    lines.push(Line::from(Span::styled(
        "─".repeat(popup_width.saturating_sub(2) as usize),
        Style::default().fg(theme.border),
    )));
    lines.push(Line::from(Span::styled(
        "  ←/→ to navigate tabs  ·  Esc to close",
        Style::default().fg(theme.text_muted),
    )));

    // ── Render ────────────────────────────────────────────────
    let block = Block::default()
        .borders(Borders::ALL)
        .title(" Keyboard Shortcuts ")
        .title_alignment(Alignment::Center)
        .border_style(Style::default().fg(theme.border));

    let inner = block.inner(popup_area);
    f.render_widget(block, popup_area);

    let paragraph = Paragraph::new(Text::from(lines))
        .wrap(Wrap { trim: false });
    f.render_widget(paragraph, inner);
}
