//! Input prompt area — text entry with keybindings.
//!
//! Ported from: `packages/tui/src/component/prompt/index.tsx`
//!
//! ## Visual Design (Opencode Match)
//!
//! The input area uses Opencode's signature border style:
//! ```text
//! ╹▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀
//! Build · deepseek-v4-flash openmodel
//! ╹▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀
//! ```

use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
use ratatui::{
    layout::{Rect},
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::Paragraph,
    Frame,
};

use crate::theme::Theme;

/// Maximum number of history entries.
const MAX_HISTORY: usize = 50;

/// Paste detection threshold (characters). Pastes larger than this show a summary.
const PASTE_THRESHOLD: usize = 500;

/// Cycle through these placeholder texts.
const PLACEHOLDERS: &[&str] = &[
    "Ask anything... \"Fix a TODO in the codebase\"",
    "Ask anything... \"Refactor this function\"",
    "Ask anything... \"Explain this code\"",
    "Ask anything... \"Add tests for this module\"",
    "Ask anything... \"Write documentation\"",
    "Ask anything... \"Debug this issue\"",
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
    /// Current model name to display.
    pub model_name: Option<String>,
    /// Current provider name to display.
    pub provider_name: Option<String>,
    /// Whether a session is active (not home screen).
    pub session_active: bool,
    /// Token count to display in footer.
    pub token_count: Option<u64>,
    /// Token limit to display in footer.
    pub token_limit: Option<u64>,
    /// Cost to display in footer.
    pub cost: f64,
    /// Whether streaming is in progress.
    pub is_streaming: bool,
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
            model_name: None,
            provider_name: None,
            session_active: false,
            token_count: None,
            token_limit: Some(200_000), // match sidebar default
            cost: 0.0,
            is_streaming: false,
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
    pub fn tick_placeholder(&mut self) {
        self.placeholder_tick = self.placeholder_tick.wrapping_add(1);
        if self.placeholder_tick.is_multiple_of(120) {
            self.placeholder_index = (self.placeholder_index + 1) % PLACEHOLDERS.len();
            self.placeholder = PLACEHOLDERS[self.placeholder_index].to_string();
        }
    }

    // ── Key handling ─────────────────────────────────────────────────

    /// Handle a key event. Returns `true` if the key was consumed.
    pub fn handle_key(&mut self, key: KeyEvent) -> bool {
        match key {
            KeyEvent {
                code: KeyCode::Enter,
                modifiers: KeyModifiers::NONE,
                ..
            } => false, // Caller captures this

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

            KeyEvent {
                code: KeyCode::Char('j'),
                modifiers: KeyModifiers::CONTROL,
                ..
            } => {
                self.insert_char('\n');
                true
            }

            KeyEvent {
                code: KeyCode::Up,
                modifiers: KeyModifiers::NONE,
                ..
            } => {
                self.history_prev();
                true
            }

            KeyEvent {
                code: KeyCode::Down,
                modifiers: KeyModifiers::NONE,
                ..
            } => {
                self.history_next();
                true
            }

            KeyEvent {
                code: KeyCode::Esc,
                modifiers: KeyModifiers::NONE,
                ..
            } => {
                if self.history_index >= 0 {
                    self.history_next();
                }
                true
            }

            KeyEvent {
                code: KeyCode::Backspace,
                modifiers: KeyModifiers::NONE,
                ..
            } => {
                self.backspace();
                true
            }

            KeyEvent {
                code: KeyCode::Delete,
                modifiers: KeyModifiers::NONE,
                ..
            } => {
                self.delete();
                true
            }

            KeyEvent {
                code: KeyCode::Left,
                modifiers: KeyModifiers::NONE,
                ..
            } => {
                self.cursor_left();
                true
            }

            KeyEvent {
                code: KeyCode::Char('b'),
                modifiers: KeyModifiers::CONTROL,
                ..
            } => {
                self.cursor_left();
                true
            }

            KeyEvent {
                code: KeyCode::Right,
                modifiers: KeyModifiers::NONE,
                ..
            } => {
                self.cursor_right();
                true
            }

            KeyEvent {
                code: KeyCode::Char('f'),
                modifiers: KeyModifiers::CONTROL,
                ..
            } => {
                self.cursor_right();
                true
            }

            KeyEvent {
                code: KeyCode::Char('a'),
                modifiers: KeyModifiers::CONTROL,
                ..
            } => {
                self.cursor_home();
                true
            }

            KeyEvent {
                code: KeyCode::Char('e'),
                modifiers: KeyModifiers::CONTROL,
                ..
            } => {
                self.cursor_end();
                true
            }

            KeyEvent {
                code: KeyCode::Char('k'),
                modifiers: KeyModifiers::CONTROL,
                ..
            } => {
                self.delete_to_end();
                true
            }

            KeyEvent {
                code: KeyCode::Char('u'),
                modifiers: KeyModifiers::CONTROL,
                ..
            } => {
                self.delete_to_start();
                true
            }

            KeyEvent {
                code: KeyCode::Char('w'),
                modifiers: KeyModifiers::CONTROL,
                ..
            } => {
                self.delete_word_backward();
                true
            }

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

    /// Handle a paste event.
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

    fn delete_word_backward(&mut self) {
        if self.cursor < self.text.len() {
            self.text.remove(self.cursor);
        } else if self.cursor > 0 {
            self.cursor -= 1;
            self.text.remove(self.cursor);
        }
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

    pub fn char_count(&self) -> usize {
        self.text.chars().count()
    }

    pub fn line_count(&self) -> usize {
        self.text.lines().count().max(1)
    }

    pub fn is_navigating_history(&self) -> bool {
        self.history_index >= 0
    }

    /// Get the highlight color for the agent name.
    pub fn agent_color(&self) -> Color {
        match self.agent_name.as_str() {
            "build" => Color::Rgb(0, 140, 186),
            "plan" => Color::Rgb(186, 120, 0),
            "general" => Color::Rgb(140, 100, 186),
            "explore" => Color::Rgb(0, 160, 100),
            _ => Color::Rgb(100, 100, 140),
        }
    }
}

// ── Rendering ─────────────────────────────────────────────────────────────────

/// Render the input area in Opencode's style.
///
/// Visual layout:
/// ```text
/// ╹▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀
///  Build · deepseek-v4-flash openmodel
/// ╹▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀
/// ```
pub fn render_input(f: &mut Frame, area: Rect, state: &InputState, theme: &Theme) {
    if area.width < 20 {
        return;
    }

    let agent_color = state.agent_color();
    let highlight_color = if state.focused {
        agent_color
    } else {
        theme.text_muted
    };
    let bc = "\u{2503}"; // ┃

    // ── Layout (Opencode Match, 4 rows) ──────────────────────────────
    // Row 0: ┃  Ask anything... or user text
    // Row 1: ┃  (blank)
    // Row 2: ┃  Build · deepseek-v4-flash openmodel
    // Row 3: ╹▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀  tab agents  ctrl+p commands

    // ── Row 0: Input text with ┃ border ──────────────────────────────
    let display_text = if state.text.is_empty() {
        state.placeholder.clone()
    } else {
        state.text.clone()
    };

    let input_spans = if state.focused && !state.text.is_empty() {
        let mut spans: Vec<Span> = vec![
            Span::styled(format!("  {}  ", bc), Style::default().fg(highlight_color)),
        ];
        for (i, c) in display_text.chars().enumerate() {
            if i == state.cursor {
                spans.push(Span::styled(c.to_string(), Style::default().fg(theme.background).bg(theme.text)));
            } else {
                spans.push(Span::styled(c.to_string(), Style::default().fg(theme.text)));
            }
        }
        if state.cursor >= display_text.chars().count() {
            spans.push(Span::styled(" ", Style::default().fg(theme.background).bg(theme.text)));
        }
        spans
    } else {
        vec![Span::styled(
            format!("  {}  {}", bc, display_text),
            Style::default().fg(if state.text.is_empty() { theme.text_muted } else { theme.text }),
        )]
    };

    f.render_widget(
        Paragraph::new(Line::from(input_spans)),
        Rect::new(area.x, area.y, area.width, 1),
    );

    // ── Row 1: Blank ┃ line ──────────────────────────────────────────
    f.render_widget(
        Paragraph::new(Line::from(Span::styled(
            format!("  {}", bc),
            Style::default().fg(highlight_color),
        ))),
        Rect::new(area.x, area.y + 1, area.width, 1),
    );

    // ── Row 2: Agent/model label with ┃ border ───────────────────────
    let mut lbl: Vec<Span> = vec![
        Span::styled(format!("  {}  ", bc), Style::default().fg(highlight_color)),
    ];

    if state.session_active {
        lbl.push(Span::styled(
            &state.agent_name,
            Style::default().fg(Color::Black).bg(agent_color).add_modifier(Modifier::BOLD),
        ));
        if let Some(ref model) = state.model_name {
            lbl.push(Span::raw(" · "));
            lbl.push(Span::styled(model.clone(), Style::default().fg(theme.text)));
        }
        if let Some(ref prov) = state.provider_name {
            lbl.push(Span::raw(" "));
            lbl.push(Span::styled(prov.clone(), Style::default().fg(theme.text_muted)));
        }
    } else {
        lbl.push(Span::styled(
            "blazecode TUI",
            Style::default().fg(theme.accent).add_modifier(Modifier::BOLD),
        ));
    }

    if state.is_streaming {
        lbl.push(Span::styled("  \u{25CF} Streaming...", Style::default().fg(theme.success)));
    }

    f.render_widget(Paragraph::new(Line::from(lbl)), Rect::new(area.x, area.y + 2, area.width, 1));

    // ── Row 3: ╹▀▀▀▀ border with right-aligned footer ──────────────
    // Opencode: always show "tab agents  ctrl+p commands" even on home screen
    let mut footer = String::new();
    if state.is_streaming {
        footer.push_str("esc interrupt");
    } else if let Some(tok) = state.token_count {
        // Token count replaces shortcuts when available
        let pct_str = state.token_limit.filter(|l| *l > 0).map(|l| format!(" ({:.0}%)", (tok as f64 / l as f64) * 100.0)).unwrap_or_default();
        footer.push_str(&format!("{}{}", format_tokens_human(tok), pct_str));
        footer.push_str("  ctrl+p commands");
    } else {
        // Opencode always shows these shortcuts
        footer.push_str("tab agents  ctrl+p commands");
    }

    let footer_len = if footer.is_empty() { 0 } else { footer.len() + 3 }; // 3 for "   " separator
    let fill_w = (area.width as usize).saturating_sub(1).saturating_sub(footer_len);
    let border_fill = "\u{2580}".repeat(fill_w); // ▀

    let combined = if !footer.is_empty() {
        format!("\u{2579}{}   {}", border_fill, footer)
    } else {
        format!("\u{2579}{}", border_fill)
    };

    f.render_widget(
        Paragraph::new(Line::from(Span::styled(combined, Style::default().fg(highlight_color)))),
        Rect::new(area.x, area.y + 3, area.width, 1),
    );
}

/// Format a token count in human-readable form.
fn format_tokens_human(count: u64) -> String {
    if count >= 1_000_000 {
        format!("{:.1}M", count as f64 / 1_000_000.0)
    } else if count >= 1_000 {
        format!("{:.1}K", count as f64 / 1_000.0)
    } else {
        count.to_string()
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

        state.history_prev();
        assert_eq!(state.text, "third");

        state.history_prev();
        assert_eq!(state.text, "second");

        state.history_next();
        assert_eq!(state.text, "third");

        state.history_next();
        assert_eq!(state.text, "");
        assert_eq!(state.history_index, -1);
    }

    #[test]
    fn test_shift_enter_adds_newline() {
        let mut state = InputState::new();
        let consumed = state.handle_key(KeyEvent::new(KeyCode::Enter, KeyModifiers::SHIFT));
        assert!(consumed);
        assert_eq!(state.text, "\n");
    }

    #[test]
    fn test_plain_enter_not_consumed() {
        let mut state = InputState::new();
        let consumed = state.handle_key(KeyEvent::new(KeyCode::Enter, KeyModifiers::NONE));
        assert!(!consumed);
    }

    #[test]
    fn test_format_tokens() {
        assert_eq!(format_tokens_human(500), "500");
        assert_eq!(format_tokens_human(1_500), "1.5K");
        assert_eq!(format_tokens_human(1_500_000), "1.5M");
    }
}
