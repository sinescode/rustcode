//! Session list dialog — browse, filter, and manage sessions.
//!
//! Ported from: `packages/tui/src/component/dialog-session-list.tsx`
//!
//! The session list is a modal dialog that displays all sessions with fuzzy
//! search filtering. Each row shows the session title, agent icon, model,
//! timestamp, and message count.
//!
//! ## Key bindings
//!
//! | Key | Action |
//! |-----|--------|
//! | `Up` / `k` | Previous session |
//! | `Down` / `j` | Next session |
//! | `Enter` | Select session |
//! | `Ctrl+D` / `Delete` | Delete session |
//! | `Ctrl+X` | Pin/unpin session |
//! | `Esc` | Close dialog |
//! | `/` | Focus search bar |
//! | Any printable | Filter sessions |

use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
use ratatui::{
    layout::{Constraint, Direction, Layout, Rect},
    style::{Color, Modifier, Style},
    text::{Line, Span, Text},
    widgets::{Block, Borders, Clear, List, ListItem, Paragraph, Wrap},
    Frame,
};

/// Maximum number of sessions to show at once.
const MAX_DISPLAY: usize = 50;

/// A session entry in the list.
///
/// # Source
/// Ported from `packages/tui/src/component/dialog-session-list.tsx`.
#[derive(Debug, Clone)]
pub struct SessionEntry {
    /// Unique session ID.
    pub id: String,
    /// Display title.
    pub title: String,
    /// Agent type (e.g., "build", "plan", "general").
    pub agent: Option<String>,
    /// Model used (e.g., "claude-sonnet-4-6").
    pub model: Option<String>,
    /// Unix timestamp of last activity.
    pub timestamp: u64,
    /// Number of messages in the session.
    pub message_count: usize,
    /// Whether this session is pinned.
    pub pinned: bool,
    /// Whether this is the currently active session.
    pub active: bool,
}

impl SessionEntry {
    /// Agent display icon.
    pub fn agent_icon(&self) -> &'static str {
        match self.agent.as_deref() {
            Some("build") => "🔨",
            Some("plan") => "📋",
            Some("general") => "💬",
            Some("explore") => "🔍",
            Some("docs") => "📖",
            Some("review") => "🔍",
            _ => "🤖",
        }
    }

    /// Human-readable time ago string.
    pub fn time_ago(&self) -> String {
        let now = chrono::Utc::now().timestamp_millis() as u64;
        let diff_ms = now.saturating_sub(self.timestamp);
        let diff_secs = diff_ms / 1000;

        if diff_secs < 60 {
            "just now".into()
        } else if diff_secs < 3600 {
            format!("{}m ago", diff_secs / 60)
        } else if diff_secs < 86400 {
            format!("{}h ago", diff_secs / 3600)
        } else if diff_secs < 604800 {
            format!("{}d ago", diff_secs / 86400)
        } else {
            format!("{}w ago", diff_secs / 604800)
        }
    }
}

/// State for the session list dialog.
///
/// # Source
/// Ported from `packages/tui/src/component/dialog-session-list.tsx`.
#[derive(Debug, Default)]
pub struct SessionListState {
    /// Whether the dialog is visible.
    pub visible: bool,
    /// All available sessions.
    pub sessions: Vec<SessionEntry>,
    /// Selected session index in the filtered list.
    pub selected: usize,
    /// Search/filter query string.
    pub query: String,
    /// Whether the search bar is focused.
    pub search_focused: bool,
    /// Filtered session indices (indexes into `self.sessions`).
    filtered_indices: Vec<usize>,
}

impl SessionListState {
    pub fn new() -> Self {
        Self {
            visible: false,
            sessions: Vec::new(),
            selected: 0,
            query: String::new(),
            search_focused: true,
            filtered_indices: Vec::new(),
        }
    }

    /// Show the dialog with the given sessions.
    pub fn show(&mut self, sessions: Vec<SessionEntry>) {
        self.sessions = sessions;
        self.visible = true;
        self.selected = 0;
        self.query.clear();
        self.search_focused = true;
        self.update_filter();
    }

    /// Hide the dialog.
    pub fn hide(&mut self) {
        self.visible = false;
    }

    /// Toggle visibility.
    pub fn toggle(&mut self, sessions: Vec<SessionEntry>) {
        if self.visible {
            self.hide();
        } else {
            self.show(sessions);
        }
    }

    /// Get the currently selected session.
    pub fn selected_session(&self) -> Option<&SessionEntry> {
        self.filtered_indices
            .get(self.selected)
            .and_then(|&idx| self.sessions.get(idx))
    }

    /// Get the ID of the currently selected session.
    pub fn selected_id(&self) -> Option<String> {
        self.selected_session().map(|s| s.id.clone())
    }

    /// Number of filtered sessions.
    pub fn filtered_count(&self) -> usize {
        self.filtered_indices.len()
    }

    /// Select the next session.
    pub fn next(&mut self) {
        if self.filtered_count() > 0 {
            self.selected = (self.selected + 1) % self.filtered_count();
        }
    }

    /// Select the previous session.
    pub fn prev(&mut self) {
        if self.filtered_count() > 0 {
            self.selected = if self.selected == 0 {
                self.filtered_count().saturating_sub(1)
            } else {
                self.selected - 1
            };
        }
    }

    /// Update the filter based on the current query.
    fn update_filter(&mut self) {
        let query_lower = self.query.to_lowercase();
        self.filtered_indices = self
            .sessions
            .iter()
            .enumerate()
            .filter(|(_, s)| {
                if query_lower.is_empty() {
                    return true;
                }
                s.title.to_lowercase().contains(&query_lower)
                    || s.agent
                        .as_ref()
                        .map(|a| a.to_lowercase().contains(&query_lower))
                        .unwrap_or(false)
                    || s.model
                        .as_ref()
                        .map(|m| m.to_lowercase().contains(&query_lower))
                        .unwrap_or(false)
                    || s.id.to_lowercase().contains(&query_lower)
            })
            .map(|(i, _)| i)
            .collect();

        // Clamp selected
        if self.filtered_indices.is_empty() {
            self.selected = 0;
        } else if self.selected >= self.filtered_indices.len() {
            self.selected = self.filtered_indices.len() - 1;
        }
    }

    /// Handle a key event. Returns the action to take.
    pub fn handle_key(&mut self, key: KeyEvent) -> Option<SessionListAction> {
        if !self.visible {
            return None;
        }

        match key {
            // Close
            KeyEvent {
                code: KeyCode::Esc, ..
            } => {
                self.hide();
                return Some(SessionListAction::Close);
            }

            // Navigate
            KeyEvent {
                code: KeyCode::Up, ..
            }
            | KeyEvent {
                code: KeyCode::Char('k'),
                modifiers: KeyModifiers::NONE,
                ..
            } => {
                self.prev();
                return Some(SessionListAction::Navigate);
            }

            KeyEvent {
                code: KeyCode::Down, ..
            }
            | KeyEvent {
                code: KeyCode::Char('j'),
                modifiers: KeyModifiers::NONE,
                ..
            } => {
                self.next();
                return Some(SessionListAction::Navigate);
            }

            // Select
            KeyEvent {
                code: KeyCode::Enter,
                modifiers: KeyModifiers::NONE,
                ..
            } => {
                let id = self.selected_id();
                self.hide();
                return Some(SessionListAction::Select(id));
            }

            // Delete
            KeyEvent {
                code: KeyCode::Delete, ..
            }
            | KeyEvent {
                code: KeyCode::Char('d'),
                modifiers: KeyModifiers::CONTROL,
                ..
            } => {
                let id = self.selected_id();
                if let Some(ref sid) = id {
                    // Remove from internal list
                    self.sessions.retain(|s| s.id != *sid);
                    self.update_filter();
                }
                return Some(SessionListAction::Delete(id));
            }

            // Pin/unpin
            KeyEvent {
                code: KeyCode::Char('x'),
                modifiers: KeyModifiers::CONTROL,
                ..
            } => {
                let id = self.selected_id();
                if let Some(ref sid) = id {
                    if let Some(session) = self.sessions.iter_mut().find(|s| s.id == *sid) {
                        session.pinned = !session.pinned;
                    }
                }
                return Some(SessionListAction::Pin(id));
            }

            // Toggle search focus
            KeyEvent {
                code: KeyCode::Char('/'),
                modifiers: KeyModifiers::NONE,
                ..
            } => {
                self.search_focused = true;
                self.query.clear();
                self.update_filter();
                return Some(SessionListAction::Navigate);
            }

            // Backspace in search
            KeyEvent {
                code: KeyCode::Backspace,
                modifiers: KeyModifiers::NONE,
                ..
            } => {
                if self.search_focused {
                    self.query.pop();
                    self.update_filter();
                    self.selected = 0;
                }
                return Some(SessionListAction::Navigate);
            }

            // Printable characters in search
            KeyEvent {
                code: KeyCode::Char(ch),
                modifiers: KeyModifiers::NONE | KeyModifiers::SHIFT,
                ..
            } if self.search_focused => {
                self.query.push(ch);
                self.update_filter();
                self.selected = 0;
                return Some(SessionListAction::Navigate);
            }

            _ => None,
        }
    }

    /// Sort pinned sessions to the top, then by timestamp descending.
    pub fn sort_default(&mut self) {
        self.sessions.sort_by(|a, b| {
            b.pinned
                .cmp(&a.pinned) // Pinned first
                .then_with(|| b.timestamp.cmp(&a.timestamp)) // Newest first
        });
        self.update_filter();
    }
}

/// Actions returned by the session list key handler.
#[derive(Debug, Clone)]
pub enum SessionListAction {
    /// Close the dialog.
    Close,
    /// Navigation occurred (redraw needed).
    Navigate,
    /// Select the session with this ID.
    Select(Option<String>),
    /// Delete the session with this ID.
    Delete(Option<String>),
    /// Pin/unpin the session with this ID.
    Pin(Option<String>),
}

/// Render the session list as a modal dialog.
pub fn render_session_list(f: &mut Frame, area: Rect, state: &SessionListState) {
    if !state.visible {
        return;
    }

    let dialog_width = (area.width as f64 * 0.65).min(80.0).max(40.0) as u16;
    let dialog_height = (area.height as f64 * 0.7).min(30.0).max(15.0) as u16;
    let dialog_x = (area.width.saturating_sub(dialog_width)) / 2;
    let dialog_y = (area.height.saturating_sub(dialog_height)) / 4;

    let dialog_area = Rect::new(area.x + dialog_x, area.y + dialog_y, dialog_width, dialog_height);

    f.render_widget(Clear, dialog_area);

    let pin_count = state.sessions.iter().filter(|s| s.pinned).count();
    let title = if pin_count > 0 {
        format!(" Sessions ({} pinned, {} total) ", pin_count, state.sessions.len())
    } else {
        format!(" Sessions ({} total) ", state.sessions.len())
    };

    let block = Block::default()
        .borders(Borders::ALL)
        .title(title)
        .title_bottom(format!(
            " j/k:nav  Enter:select  Ctrl+D:delete  Ctrl+X:pin  /:search  Esc:close "
        ))
        .border_style(Style::default().fg(Color::Cyan))
        .style(Style::default().bg(Color::Black));

    let inner = block.inner(dialog_area);
    f.render_widget(block, dialog_area);

    // Split into search bar + list
    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([Constraint::Length(2), Constraint::Min(0)])
        .split(inner);

    // ── Search bar ─────────────────────────────────────────────
    let search_style = if state.search_focused {
        Style::default().fg(Color::Yellow).bg(Color::Black)
    } else {
        Style::default().fg(Color::DarkGray)
    };

    let search_text = if state.query.is_empty() {
        if state.search_focused {
            Span::styled(" Type to filter sessions... ", search_style.add_modifier(Modifier::BOLD))
        } else {
            Span::styled(" Press / to search... ", search_style)
        }
    } else {
        Span::styled(
            format!(" /{} ", state.query),
            search_style,
        )
    };

    let search_block = Block::default()
        .borders(Borders::BOTTOM)
        .border_style(Style::default().fg(if state.search_focused { Color::Yellow } else { Color::DarkGray }));
    let search_inner = search_block.inner(chunks[0]);
    f.render_widget(search_block, chunks[0]);
    f.render_widget(Paragraph::new(Line::from(search_text)), search_inner);

    // ── Session list ───────────────────────────────────────────
    let max_display = MAX_DISPLAY.min(state.filtered_count());
    let visible_indices: Vec<usize> = state.filtered_indices.iter().take(max_display).copied().collect();

    if visible_indices.is_empty() {
        let no_results = if state.query.is_empty() {
            "No sessions. Press Enter to create one."
        } else {
            "No matching sessions."
        };
        f.render_widget(
            Paragraph::new(no_results)
                .style(Style::default().fg(Color::DarkGray))
                .wrap(Wrap { trim: true }),
            chunks[1],
        );
        return;
    }

    let items: Vec<ListItem> = visible_indices
        .iter()
        .enumerate()
        .map(|(i, &sess_idx)| {
            let session = &state.sessions[sess_idx];
            let is_selected = i == state.selected;

            let row_style = if is_selected {
                Style::default().fg(Color::Black).bg(Color::Cyan)
            } else if session.active {
                Style::default().fg(Color::White).bg(Color::Rgb(20, 20, 40))
            } else {
                Style::default().fg(Color::White)
            };

            // Pin icon
            let pin_icon = if session.pinned { "📌" } else { "  " };

            // Active indicator
            let active_marker = if session.active { " * " } else { "   " };

            // Truncate title to fit
            let max_title = 30;
            let title = if session.title.len() > max_title {
                format!("{}...", &session.title[..max_title - 3])
            } else {
                session.title.clone()
            };

            let model_str = session.model.as_deref().unwrap_or("auto");
            let agent_icon = session.agent_icon();
            let time_ago = session.time_ago();

            let line = Line::from(vec![
                Span::styled(pin_icon, if is_selected { row_style } else { Style::default().fg(Color::Yellow) }),
                Span::styled(active_marker, row_style),
                Span::styled(agent_icon, row_style),
                Span::raw(" "),
                Span::styled(&title, row_style.add_modifier(Modifier::BOLD)),
                Span::raw("  "),
                Span::styled(model_str, if is_selected { Style::default().fg(Color::Black) } else { Style::default().fg(Color::DarkGray) }),
                Span::raw("  "),
                Span::styled(
                    format!("{} msgs", session.message_count),
                    if is_selected { Style::default().fg(Color::Black) } else { Style::default().fg(Color::Gray) },
                ),
                Span::raw("  "),
                Span::styled(time_ago, if is_selected { Style::default().fg(Color::Black) } else { Style::default().fg(Color::DarkGray) }),
            ]);

            ListItem::new(line)
        })
        .collect();

    let list = List::new(items);
    f.render_widget(list, chunks[1]);
}

#[cfg(test)]
mod tests {
    use super::*;

    fn make_session(title: &str, pinned: bool, active: bool) -> SessionEntry {
        SessionEntry {
            id: format!("ses_{}", title),
            title: title.to_string(),
            agent: Some("build".into()),
            model: Some("claude-sonnet-4-6".into()),
            timestamp: chrono::Utc::now().timestamp_millis() as u64,
            message_count: 5,
            pinned,
            active,
        }
    }

    #[test]
    fn test_session_list_navigation() {
        let sessions = vec![
            make_session("Alpha", false, false),
            make_session("Beta", true, false),
            make_session("Gamma", false, true),
        ];

        let mut state = SessionListState::new();
        state.show(sessions);
        assert!(state.visible);
        assert_eq!(state.filtered_count(), 3);
        assert_eq!(state.selected, 0);

        state.next();
        assert_eq!(state.selected, 1);

        state.next();
        assert_eq!(state.selected, 2);

        state.next();
        assert_eq!(state.selected, 0); // wrap

        state.prev();
        assert_eq!(state.selected, 2); // wrap back
    }

    #[test]
    fn test_session_list_filter() {
        let sessions = vec![
            make_session("Alpha project", false, false),
            make_session("Beta feature", false, false),
            make_session("Gamma refactor", false, false),
        ];

        let mut state = SessionListState::new();
        state.show(sessions);
        assert_eq!(state.filtered_count(), 3);

        // Type query
        state.query = "beta".into();
        state.update_filter();
        assert_eq!(state.filtered_count(), 1);
        assert_eq!(state.selected, 0);

        // No results
        state.query = "xyzzy".into();
        state.update_filter();
        assert_eq!(state.filtered_count(), 0);

        // Clear
        state.query.clear();
        state.update_filter();
        assert_eq!(state.filtered_count(), 3);
    }

    #[test]
    fn test_session_list_sort() {
        let old = SessionEntry {
            id: "old".into(),
            title: "Old".into(),
            agent: None,
            model: None,
            timestamp: 1000,
            message_count: 0,
            pinned: false,
            active: false,
        };
        let pinned = SessionEntry {
            id: "pinned".into(),
            title: "Pinned".into(),
            agent: None,
            model: None,
            timestamp: 2000,
            message_count: 0,
            pinned: true,
            active: false,
        };
        let new = SessionEntry {
            id: "new".into(),
            title: "New".into(),
            agent: None,
            model: None,
            timestamp: 3000,
            message_count: 0,
            pinned: false,
            active: false,
        };

        let mut state = SessionListState::new();
        state.sessions = vec![old.clone(), pinned.clone(), new.clone()];
        state.sort_default();

        // Pinned should be first
        assert!(state.sessions[0].pinned);
        // Then newest first among unpinned
        assert!(!state.sessions[1].pinned);
        assert!(state.sessions[1].timestamp >= state.sessions[2].timestamp);
    }

    #[test]
    fn test_session_entry_time_ago() {
        let now = chrono::Utc::now().timestamp_millis() as u64;
        let just_now = SessionEntry {
            id: "now".into(),
            title: "Now".into(),
            agent: None,
            model: None,
            timestamp: now,
            message_count: 0,
            pinned: false,
            active: false,
        };
        assert_eq!(just_now.time_ago(), "just now");

        let mins_ago = SessionEntry {
            id: "mins".into(),
            title: "Mins".into(),
            agent: None,
            model: None,
            timestamp: now.saturating_sub(5 * 60 * 1000), // 5 min ago
            message_count: 0,
            pinned: false,
            active: false,
        };
        assert_eq!(mins_ago.time_ago(), "5m ago");
    }
}
