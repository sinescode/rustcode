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

/// State for the input area.
#[derive(Debug, Default)]
pub struct InputState {
    /// Current input text.
    pub text: String,
    /// Cursor position within the text.
    pub cursor: usize,
    /// Whether the input is focused.
    pub focused: bool,
    /// Whether to show a placeholder.
    pub placeholder: String,
}

impl InputState {
    pub fn new() -> Self {
        Self {
            text: String::new(),
            cursor: 0,
            focused: true,
            placeholder: "Type a message... (Enter to send)".to_string(),
        }
    }

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

    /// Move cursor to start of line.
    pub fn cursor_home(&mut self) {
        self.cursor = 0;
    }

    /// Move cursor to end of line.
    pub fn cursor_end(&mut self) {
        self.cursor = self.text.len();
    }

    /// Delete from cursor to end of line.
    pub fn delete_to_end(&mut self) {
        self.text.truncate(self.cursor);
    }

    /// Delete from start of line to cursor.
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

            // Ctrl+J → newline
            KeyEvent {
                code: KeyCode::Char('j'),
                modifiers: KeyModifiers::CONTROL,
                ..
            } => {
                self.insert_char('\n');
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

            // Left
            KeyEvent {
                code: KeyCode::Left,
                modifiers: KeyModifiers::NONE,
                ..
            } => {
                self.cursor_left();
                true
            }

            // Ctrl+B → left
            KeyEvent {
                code: KeyCode::Char('b'),
                modifiers: KeyModifiers::CONTROL,
                ..
            } => {
                self.cursor_left();
                true
            }

            // Right
            KeyEvent {
                code: KeyCode::Right,
                modifiers: KeyModifiers::NONE,
                ..
            } => {
                self.cursor_right();
                true
            }

            // Ctrl+F → right
            KeyEvent {
                code: KeyCode::Char('f'),
                modifiers: KeyModifiers::CONTROL,
                ..
            } => {
                self.cursor_right();
                true
            }

            // Ctrl+A → home
            KeyEvent {
                code: KeyCode::Char('a'),
                modifiers: KeyModifiers::CONTROL,
                ..
            } => {
                self.cursor_home();
                true
            }

            // Ctrl+E → end
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

    /// Delete the word before the cursor.
    fn delete_word_backward(&mut self) {
        // Skip whitespace
        while self.cursor > 0 && self.text.as_bytes().get(self.cursor - 1) == Some(&b' ') {
            self.cursor -= 1;
            self.text.remove(self.cursor);
        }
        // Delete word characters
        while self.cursor > 0
            && self.text.as_bytes().get(self.cursor - 1).map_or(false, |b| *b != b' ')
        {
            self.cursor -= 1;
            self.text.remove(self.cursor);
        }
    }
}

/// Render the input area.
pub fn render_input(f: &mut Frame, area: Rect, state: &InputState) {
    let display_text = if state.text.is_empty() && !state.focused {
        state.placeholder.clone()
    } else {
        state.text.clone()
    };

    let cursor_style = if state.focused {
        Style::default()
            .fg(Color::Black)
            .bg(Color::White)
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
            } else {
                line.push_span(Span::styled(ch.to_string(), Style::default().fg(Color::White)));
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
            Style::default().fg(Color::DarkGray),
        ))
    } else {
        Line::from(Span::styled(&display_text, Style::default().fg(Color::White)))
    };

    let input_widget = Paragraph::new(content).block(
        Block::default()
            .borders(Borders::TOP)
            .border_style(if state.focused {
                Style::default().fg(Color::Cyan)
            } else {
                Style::default().fg(Color::DarkGray)
            }),
    );

    f.render_widget(input_widget, area);
}
