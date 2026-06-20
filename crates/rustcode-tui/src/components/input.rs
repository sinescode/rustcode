//! Input prompt area — text entry with keybindings.
//!
//! Ported from: `packages/tui/src/component/prompt/index.tsx`

use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
use ratatui::{
    layout::Rect,
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Borders, Paragraph},
    Frame,
};

use crate::theme::Theme;

/// Maximum number of history entries.
const MAX_HISTORY: usize = 50;

/// Paste detection threshold (characters). Pastes larger than this show a summary.
const PASTE_THRESHOLD: usize = 500;

/// Cycle through these placeholder texts.
const PLACEHOLDERS: &[&str] = &[
    "Type a message... (Enter to send, Shift+Enter for newline)",
    "Ask me anything... (Ctrl+P for commands)",
    "Describe what you want to build or fix...",
    "Paste code or describe your problem...",
    "Type /help for available commands...",
];

/// State for the input area.
#[derive(Debug)]
pub struct InputState {
    /// Current input text.
    pub text: String,
    /// Cursor position within the text.
    pub cursor: usize,
    /// Whether the input is focused.
    pub focused: bool,
    /// Current placeholder text.
    pub placeholder: String,
    /// History of submitted prompts (newest first).
    history: Vec<String>,
    /// Current position in history navigation (-1 = not navigating).
    history_index: i32,
    /// Saved current input when navigating history (to restore on Escape or at end).
    saved_input: String,
    /// Saved cursor position when navigating history.
    saved_cursor: usize,
    /// Index into the placeholder cycle.
    placeholder_index: usize,
    /// Number of frames since last placeholder change.
    placeholder_tick: u64,
    /// Current agent name to display in the input.
    pub agent_name: String,
}

impl Default for InputState {
    fn default() -> Self {
        Self {
            text: String::new(),
            cursor: 0,
            focused: true,
            placeholder: PLACEHOLDERS[0].to_string(),
            history: Vec::new(),
            history_index: -1,
            saved_input: String::new(),
            saved_cursor: 0,
            placeholder_index: 0,
            placeholder_tick: 0,
            agent_name: String::from("build"),
        }
    }
}

impl InputState {
    pub fn new() -> Self {
        Self::default()
    }

    // ── Text manipulation ────────────────────────────────────────────

    /// Insert a character at the cursor position.
    pub fn insert_char(&mut self, ch: char) {
        self.text.insert(self.cursor, ch);
        self.cursor += 1;
    }

    /// Insert a string at the cursor position.
    pub fn insert_str(&mut self, s: &str) {
        self.text.insert_str(self.cursor, s);
        self.cursor += s.len();
    }

    /// Delete the character before the cursor.
    pub fn backspace(&mut self) {
        if self.cursor > 0 {
            self.cursor -= 1;
            self.text.remove(self.cursor);
        }
    }

    /// Delete the character at the cursor.
    pub fn delete(&mut self) {
        if self.cursor < self.text.len() {
            self.text.remove(self.cursor);
        }
    }

    /// Move cursor left.
    pub fn cursor_left(&mut self) {
        if self.cursor > 0 {
            self.cursor -= 1;
        }
    }

    /// Move cursor right.
    pub fn cursor_right(&mut self) {
        if self.cursor < self.text.len() {
            self.cursor += 1;
        }
    }

    /// Move cursor to start of text.
    pub fn cursor_home(&mut self) {
        self.cursor = 0;
    }

    /// Move cursor to end of text.
    pub fn cursor_end(&mut self) {
        self.cursor = self.text.len();
    }

    /// Delete from cursor to end of text.
    pub fn delete_to_end(&mut self) {
        self.text.truncate(self.cursor);
    }

    /// Delete from start of text to cursor.
    pub fn delete_to_start(&mut self) {
        if self.cursor > 0 {
            self.text.drain(..self.cursor);
            self.cursor = 0;
        }
    }

    /// Clear all text.
    pub fn clear(&mut self) {
        self.text.clear();
        self.cursor = 0;
    }

    /// Take the current text and clear the input (for submit).
    pub fn take(&mut self) -> String {
        let text = std::mem::take(&mut self.text);
        self.cursor = 0;
        // Reset history navigation
        self.history_index = -1;
        self.saved_input.clear();
        text
    }

    /// Set the text (from external event, e.g. server append).
    pub fn set_text(&mut self, text: &str) {
        self.text = text.to_string();
        self.cursor = self.text.len();
    }

    /// Append text (from server event).
    pub fn append(&mut self, text: &str) {
        self.text.push_str(text);
        self.cursor = self.text.len();
    }

    // ── History management ───────────────────────────────────────────

    /// Add a prompt to history. Duplicates are not added.
    pub fn add_to_history(&mut self, text: &str) {
        if text.trim().is_empty() {
            return;
        }
        // Remove duplicate if exists
        self.history.retain(|h| h != text);
        self.history.insert(0, text.to_string());
        if self.history.len() > MAX_HISTORY {
            self.history.truncate(MAX_HISTORY);
        }
    }

    /// Navigate to previous history entry.
    fn history_prev(&mut self) {
        if self.history.is_empty() {
            return;
        }
        if self.history_index == -1 {
            // Save current input before navigating
            self.saved_input = std::mem::take(&mut self.text);
            self.saved_cursor = self.cursor;
            self.history_index = 0;
        } else if (self.history_index as usize) < self.history.len().saturating_sub(1) {
            self.history_index += 1;
        }
        let idx = self.history_index as usize;
        self.text = self.history[idx].clone();
        self.cursor = self.text.len();
    }

    /// Navigate to next history entry.
    fn history_next(&mut self) {
        if self.history_index <= 0 {
            // Restore saved input
            self.history_index = -1;
            self.text = std::mem::take(&mut self.saved_input);
            self.cursor = self.saved_cursor;
        } else {
            self.history_index -= 1;
            let idx = self.history_index as usize;
            self.text = self.history[idx].clone();
            self.cursor = self.text.len();
        }
    }

    // ── Placeholder cycling ──────────────────────────────────────────

    /// Advance the placeholder to the next in the cycle.
    /// Call this on a timer (e.g., every 120 frames at 50ms = every 6 seconds).
    pub fn tick_placeholder(&mut self) {
        self.placeholder_tick = self.placeholder_tick.wrapping_add(1);
        if self.placeholder_tick.is_multiple_of(120) {
            self.placeholder_index = (self.placeholder_index + 1) % PLACEHOLDERS.len();
            self.placeholder = PLACEHOLDERS[self.placeholder_index].to_string();
        }
    }

    // ── Key handling ─────────────────────────────────────────────────

    /// Handle a key event. Returns `true` if the key was consumed.
    ///
    /// # Source
    /// Ported from `packages/tui/src/config/keybind.ts` input bindings.
    pub fn handle_key(&mut self, key: KeyEvent) -> bool {
        match key {
            // Enter → submit (handled by caller)
            KeyEvent {
                code: KeyCode::Enter,
                modifiers: KeyModifiers::NONE,
                ..
            } => false, // Caller captures this

            // Shift+Enter / Ctrl+Enter → newline
            KeyEvent {
                code: KeyCode::Enter,
                modifiers: KeyModifiers::SHIFT,
                ..
            }
            | KeyEvent {
                code: KeyCode::Enter,
                modifiers: KeyModifiers::CONTROL,
                ..
            } => {
                self.insert_char('\n');
                true
            }

            // Ctrl+J → newline (alternative)
            KeyEvent {
                code: KeyCode::Char('j'),
                modifiers: KeyModifiers::CONTROL,
                ..
            } => {
                self.insert_char('\n');
                true
            }

            // Up arrow → history previous
            KeyEvent {
                code: KeyCode::Up,
                modifiers: KeyModifiers::NONE,
                ..
            } => {
                self.history_prev();
                true
            }

            // Down arrow → history next
            KeyEvent {
                code: KeyCode::Down,
                modifiers: KeyModifiers::NONE,
                ..
            } => {
                self.history_next();
                true
            }

            // Escape → cancel history navigation
            KeyEvent {
                code: KeyCode::Esc,
                modifiers: KeyModifiers::NONE,
                ..
            } => {
                if self.history_index >= 0 {
                    self.history_next(); // restores saved input
                }
                true
            }

            // Backspace
            KeyEvent {
                code: KeyCode::Backspace,
                modifiers: KeyModifiers::NONE,
                ..
            } => {
                self.backspace();
                true
            }

            // Delete
            KeyEvent {
                code: KeyCode::Delete,
                modifiers: KeyModifiers::NONE,
                ..
            } => {
                self.delete();
                true
            }

            // Left arrow
            KeyEvent {
                code: KeyCode::Left,
                modifiers: KeyModifiers::NONE,
                ..
            } => {
                self.cursor_left();
                true
            }

            // Ctrl+B → left (emacs-style)
            KeyEvent {
                code: KeyCode::Char('b'),
                modifiers: KeyModifiers::CONTROL,
                ..
            } => {
                self.cursor_left();
                true
            }

            // Right arrow
            KeyEvent {
                code: KeyCode::Right,
                modifiers: KeyModifiers::NONE,
                ..
            } => {
                self.cursor_right();
                true
            }

            // Ctrl+F → right (emacs-style)
            KeyEvent {
                code: KeyCode::Char('f'),
                modifiers: KeyModifiers::CONTROL,
                ..
            } => {
                self.cursor_right();
                true
            }

            // Ctrl+A → home (emacs-style)
            KeyEvent {
                code: KeyCode::Char('a'),
                modifiers: KeyModifiers::CONTROL,
                ..
            } => {
                self.cursor_home();
                true
            }

            // Ctrl+E → end (emacs-style)
            KeyEvent {
                code: KeyCode::Char('e'),
                modifiers: KeyModifiers::CONTROL,
                ..
            } => {
                self.cursor_end();
                true
            }

            // Ctrl+K → delete to end
            KeyEvent {
                code: KeyCode::Char('k'),
                modifiers: KeyModifiers::CONTROL,
                ..
            } => {
                self.delete_to_end();
                true
            }

            // Ctrl+U → delete to start
            KeyEvent {
                code: KeyCode::Char('u'),
                modifiers: KeyModifiers::CONTROL,
                ..
            } => {
                self.delete_to_start();
                true
            }

            // Ctrl+W → delete word backward
            KeyEvent {
                code: KeyCode::Char('w'),
                modifiers: KeyModifiers::CONTROL,
                ..
            } => {
                self.delete_word_backward();
                true
            }

            // Printable characters
            KeyEvent {
                code: KeyCode::Char(ch),
                modifiers: KeyModifiers::NONE | KeyModifiers::SHIFT,
                ..
            } => {
                self.insert_char(ch);
                true
            }

            _ => false,
        }
    }

    /// Handle a paste event — a large text insertion.
    /// Returns a summary string if the paste exceeds the threshold.
    pub fn handle_paste(&mut self, text: &str) -> Option<String> {
        if text.len() > PASTE_THRESHOLD {
            let line_count = text.lines().count();
            let char_count = text.chars().count();
            self.insert_str(text);
            Some(format!("Pasted {} chars, {} lines", char_count, line_count))
        } else {
            self.insert_str(text);
            None
        }
    }

    // ── Private helpers ──────────────────────────────────────────────

    /// Delete the word before the cursor.
    fn delete_word_backward(&mut self) {
        // Delete the character at cursor position (if cursor points to a char)
        if self.cursor < self.text.len() {
            self.text.remove(self.cursor);
        } else if self.cursor > 0 {
            // cursor is past the end, delete last char
            self.cursor -= 1;
            self.text.remove(self.cursor);
        }
        // Delete preceding word characters (alphanumeric + underscores)
        while self.cursor > 0 {
            let prev = self.text.as_bytes().get(self.cursor - 1).copied();
            match prev {
                Some(b) if b.is_ascii_alphanumeric() || b == b'_' => {
                    self.cursor -= 1;
                    self.text.remove(self.cursor);
                }
                _ => break,
            }
        }
    }

    // ── Accessors ────────────────────────────────────────────────────

    /// Character count of the current text.
    pub fn char_count(&self) -> usize {
        self.text.chars().count()
    }

    /// Line count of the current text.
    pub fn line_count(&self) -> usize {
        self.text.lines().count().max(1)
    }

    /// Whether the user is currently navigating history.
    pub fn is_navigating_history(&self) -> bool {
        self.history_index >= 0
    }
}

/// Render the input area.
pub fn render_input(f: &mut Frame, area: Rect, state: &InputState, theme: &Theme) {
    let display_text = if state.text.is_empty() && !state.focused {
        state.placeholder.clone()
    } else {
        state.text.clone()
    };

    let cursor_style = if state.focused {
        Style::default().fg(theme.background).bg(theme.accent)
    } else {
        Style::default()
    };

    // Build a line with cursor highlighting
    let content = if state.focused && state.cursor <= display_text.len() {
        let mut line = Line::default();
        let chars: Vec<char> = display_text.chars().collect();

        for (i, ch) in chars.iter().enumerate() {
            if i == state.cursor {
                line.push_span(Span::styled(ch.to_string(), cursor_style));
            } else if *ch == '\n' {
                line.push_span(Span::styled("↵ ", Style::default().fg(theme.dim)));
            } else {
                line.push_span(Span::styled(
                    ch.to_string(),
                    Style::default().fg(theme.foreground),
                ));
            }
        }

        // Cursor at end
        if state.cursor >= chars.len() {
            line.push_span(Span::styled(" ", cursor_style));
        }

        line
    } else if display_text.is_empty() {
        Line::from(Span::styled(
            &state.placeholder,
            Style::default().fg(theme.dim),
        ))
    } else {
        // Replace newlines with visible markers when not focused
        let visible = display_text.replace('\n', "↵ ");
        Line::from(Span::styled(visible, Style::default().fg(theme.foreground)))
    };

    let input_widget =
        Paragraph::new(content).block(Block::default().borders(Borders::TOP).border_style(
            if state.focused {
                Style::default().fg(theme.accent)
            } else {
                Style::default().fg(theme.dim)
            },
        ));

    f.render_widget(input_widget, area);

    // ── Overlay: agent indicator + character counter ────────────────
    if area.width > 20 {
        let agent_span = Span::styled(
            format!("[{}]", state.agent_name),
            Style::default()
                .fg(Color::Black)
                .bg(agent_color(&state.agent_name))
                .add_modifier(Modifier::BOLD),
        );

        let counter_text = if state.text.is_empty() {
            String::from("0")
        } else {
            format!(
                "{} char{} · {} line{} · hist:{}",
                state.char_count(),
                if state.char_count() == 1 { "" } else { "s" },
                state.line_count(),
                if state.line_count() == 1 { "" } else { "s" },
                state.history.len(),
            )
        };

        let counter_span = Span::styled(counter_text, Style::default().fg(Color::DarkGray));

        let status_line = Line::from(vec![
            Span::raw(" "),
            agent_span,
            Span::raw(" "),
            counter_span,
        ]);

        // Render at the bottom of the input area (the last row)
        if area.height > 1 {
            let status_area = Rect {
                x: area.x,
                y: area.y + area.height.saturating_sub(1),
                width: area.width,
                height: 1,
            };
            let widget = Paragraph::new(status_line);
            f.render_widget(widget, status_area);
        }
    }
}

/// Pick a background color based on agent name.
fn agent_color(agent: &str) -> Color {
    match agent {
        "build" => Color::Rgb(0, 140, 186),
        "plan" => Color::Rgb(186, 120, 0),
        "general" => Color::Rgb(140, 100, 186),
        "explore" => Color::Rgb(0, 160, 100),
        _ => Color::Rgb(100, 100, 140),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_insert_and_backspace() {
        let mut state = InputState::new();
        state.insert_char('h');
        state.insert_char('i');
        assert_eq!(state.text, "hi");
        assert_eq!(state.cursor, 2);

        state.backspace();
        assert_eq!(state.text, "h");
        assert_eq!(state.cursor, 1);
    }

    #[test]
    fn test_cursor_movement() {
        let mut state = InputState::new();
        state.insert_str("hello");
        state.cursor_left();
        assert_eq!(state.cursor, 4);
        state.cursor_left();
        assert_eq!(state.cursor, 3);
        state.cursor_right();
        assert_eq!(state.cursor, 4);
        state.cursor_end();
        assert_eq!(state.cursor, 5);
        state.cursor_home();
        assert_eq!(state.cursor, 0);
    }

    #[test]
    fn test_delete_word_backward() {
        let mut state = InputState::new();
        state.insert_str("hello world foo");
        state.cursor = 14; // end
        state.delete_word_backward();
        assert_eq!(state.text, "hello world ");
        state.delete_word_backward();
        assert_eq!(state.text, "hello ");
    }

    #[test]
    fn test_take_clears_input() {
        let mut state = InputState::new();
        state.insert_str("some prompt");
        let taken = state.take();
        assert_eq!(taken, "some prompt");
        assert!(state.text.is_empty());
        assert_eq!(state.cursor, 0);
    }

    #[test]
    fn test_history_navigation() {
        let mut state = InputState::new();
        state.add_to_history("first");
        state.add_to_history("second");
        state.add_to_history("third");
        assert_eq!(state.history.len(), 3);

        // Navigate up (prev) through history
        state.history_prev();
        assert_eq!(state.text, "third");

        state.history_prev();
        assert_eq!(state.text, "second");

        // Navigate down (next)
        state.history_next();
        assert_eq!(state.text, "third");

        // Past the newest → restores saved input
        state.history_next();
        assert_eq!(state.text, "");
        assert_eq!(state.history_index, -1);
    }

    #[test]
    fn test_history_duplicate_avoidance() {
        let mut state = InputState::new();
        state.add_to_history("hello");
        state.add_to_history("hello");
        state.add_to_history("world");
        assert_eq!(state.history.len(), 2);
        assert_eq!(state.history[0], "world");
        assert_eq!(state.history[1], "hello");
    }

    #[test]
    fn test_placeholder_cycle() {
        let mut state = InputState::new();
        let initial = state.placeholder.clone();
        // Tick 120 times to cycle
        for _ in 0..120 {
            state.tick_placeholder();
        }
        assert_ne!(state.placeholder, initial);
        // Should be at index 1
        assert_eq!(state.placeholder, PLACEHOLDERS[1]);
    }

    #[test]
    fn test_char_count() {
        let mut state = InputState::new();
        assert_eq!(state.char_count(), 0);
        state.insert_str("hello");
        assert_eq!(state.char_count(), 5);
        state.insert_str(" world");
        assert_eq!(state.char_count(), 11);
    }

    #[test]
    fn test_line_count() {
        let mut state = InputState::new();
        assert_eq!(state.line_count(), 1); // empty = 1 line
        state.insert_str("line1\nline2\nline3");
        assert_eq!(state.line_count(), 3);
    }

    #[test]
    fn test_shift_enter_adds_newline() {
        let mut state = InputState::new();
        let consumed = state.handle_key(KeyEvent::new(KeyCode::Enter, KeyModifiers::SHIFT));
        assert!(consumed);
        assert_eq!(state.text, "\n");
    }

    #[test]
    fn test_ctrl_enter_adds_newline() {
        let mut state = InputState::new();
        let consumed = state.handle_key(KeyEvent::new(KeyCode::Enter, KeyModifiers::CONTROL));
        assert!(consumed);
        assert_eq!(state.text, "\n");
    }

    #[test]
    fn test_plain_enter_not_consumed() {
        let mut state = InputState::new();
        let consumed = state.handle_key(KeyEvent::new(KeyCode::Enter, KeyModifiers::NONE));
        assert!(!consumed); // caller handles submit
    }
}
