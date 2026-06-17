//! Question prompt dialog — modal dialog for question requests from the AI.
//!
//! Ported from: `packages/tui/src/routes/session/question.tsx` (515 lines)
//!
//! ## Behavior
//!
//! - For single questions without `multiple`, auto-submit on selection.
//! - For multiple questions: tab between questions, Enter to toggle multi-select,
//!   Enter on the confirm tab to submit all answers.
//! - Custom text answers supported via `custom: true` flag.

use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
use ratatui::{
    layout::Rect,
    style::{Color, Modifier, Style},
    text::{Line, Span, Text},
    widgets::{Block, Borders, Clear, Paragraph, Wrap},
    Frame,
};

/// State for the question prompt dialog.
#[derive(Debug)]
pub struct QuestionState {
    /// Whether the dialog is visible.
    pub visible: bool,
    /// The question items to present.
    pub questions: Vec<super::super::event::QuestionItem>,
    /// Current question tab index.
    pub tab: usize,
    /// Current selected option index within the tab.
    pub selected_option: usize,
    /// Answers per question (labels for multi-select, single-element for single-select).
    pub answers: Vec<Vec<String>>,
    /// Custom text answers per question.
    pub custom: Vec<String>,
    /// Whether in editing mode (for custom text input).
    pub editing: bool,
    /// The request ID being answered.
    pub request_id: Option<String>,
}

impl QuestionState {
    pub fn new() -> Self {
        Self {
            visible: false,
            questions: Vec::new(),
            tab: 0,
            selected_option: 0,
            answers: Vec::new(),
            custom: Vec::new(),
            editing: false,
            request_id: None,
        }
    }

    /// Show a question request.
    pub fn show(
        &mut self,
        request_id: String,
        questions: Vec<super::super::event::QuestionItem>,
    ) {
        self.request_id = Some(request_id);
        self.questions = questions;
        self.answers = vec![Vec::new(); questions.len()];
        self.custom = vec![String::new(); questions.len()];
        self.tab = 0;
        self.selected_option = 0;
        self.editing = false;
        self.visible = true;
    }

    /// Dismiss the dialog.
    pub fn dismiss(&mut self) {
        self.visible = false;
        self.questions.clear();
        self.answers.clear();
        self.custom.clear();
    }

    /// Whether this is a single-question non-multi-select (auto-submit on pick).
    pub fn is_single(&self) -> bool {
        self.questions.len() == 1 && !self.questions[0].multiple
    }

    /// Whether on the confirm tab.
    pub fn is_confirm_tab(&self) -> bool {
        !self.is_single() && self.tab >= self.questions.len()
    }

    /// Total number of tabs (questions + optional confirm).
    pub fn tab_count(&self) -> usize {
        if self.is_single() {
            1
        } else {
            self.questions.len() + 1 // + confirm tab
        }
    }

    /// Current question, if not on confirm tab.
    pub fn current_question(&self) -> Option<&super::super::event::QuestionItem> {
        if self.is_confirm_tab() {
            None
        } else {
            self.questions.get(self.tab)
        }
    }

    /// Options for the current question (including custom).
    pub fn option_count(&self) -> usize {
        match self.current_question() {
            Some(q) => {
                let base = q.options.len();
                if q.custom { base + 1 } else { base }
            }
            None => 0,
        }
    }

    /// Handle a key event. Returns:
    /// - `Some(questions, answers)` if the dialog should submit
    /// - `Some(questions, vec![])` if the dialog should reject
    /// - `None` if still interacting
    pub fn handle_key(
        &mut self,
        key: KeyEvent,
    ) -> Option<(String, Vec<Vec<String>>)> {
        if !self.visible {
            return None;
        }

        if self.editing {
            return self.handle_editing_key(key);
        }

        if self.is_confirm_tab() {
            return self.handle_confirm_key(key);
        }

        self.handle_selection_key(key)
    }

    fn handle_selection_key(
        &mut self,
        key: KeyEvent,
    ) -> Option<(String, Vec<Vec<String>>)> {
        let total = self.option_count();

        match key {
            // Escape → reject
            KeyEvent {
                code: KeyCode::Esc, ..
            } => {
                let request_id = self.request_id.clone().unwrap_or_default();
                Some((request_id, Vec::new()))
            }

            // Enter → select option
            KeyEvent {
                code: KeyCode::Enter,
                modifiers: KeyModifiers::NONE,
                ..
            } => {
                let q = match self.current_question() {
                    Some(q) => q,
                    None => return None,
                };

                let is_custom = self.selected_option >= q.options.len();

                if is_custom {
                    // Enter editing mode for custom text
                    if q.multiple {
                        // Toggle custom value
                        self.editing = true;
                        return None;
                    } else {
                        self.editing = true;
                        return None;
                    }
                }

                // Regular option
                if let Some(opt) = q.options.get(self.selected_option) {
                    if q.multiple {
                        // Toggle
                        let answers = &mut self.answers[self.tab];
                        if let Some(pos) = answers.iter().position(|a| a == &opt.label) {
                            answers.remove(pos);
                        } else {
                            answers.push(opt.label.clone());
                        }
                    } else {
                        // Single select → pick and advance
                        self.answers[self.tab] = vec![opt.label.clone()];
                        if self.is_single() {
                            // Auto-submit
                            let request_id = self.request_id.clone().unwrap_or_default();
                            let answers = std::mem::take(&mut self.answers);
                            return Some((request_id, answers));
                        }
                        // Advance to next tab
                        self.next_tab();
                    }
                }
                None
            }

            // Up / k → prev option
            KeyEvent {
                code: KeyCode::Up,
                modifiers: KeyModifiers::NONE,
                ..
            }
            | KeyEvent {
                code: KeyCode::Char('k'),
                modifiers: KeyModifiers::NONE,
                ..
            } => {
                self.selected_option = if self.selected_option == 0 {
                    total.saturating_sub(1)
                } else {
                    self.selected_option - 1
                };
                None
            }

            // Down / j → next option
            KeyEvent {
                code: KeyCode::Down,
                modifiers: KeyModifiers::NONE,
                ..
            }
            | KeyEvent {
                code: KeyCode::Char('j'),
                modifiers: KeyModifiers::NONE,
                ..
            } => {
                self.selected_option = (self.selected_option + 1) % total;
                None
            }

            // Tab / Right → next tab
            KeyEvent {
                code: KeyCode::Tab,
                modifiers: KeyModifiers::NONE,
                ..
            }
            | KeyEvent {
                code: KeyCode::Right,
                modifiers: KeyModifiers::NONE,
                ..
            }
            | KeyEvent {
                code: KeyCode::Char('l'),
                modifiers: KeyModifiers::NONE,
                ..
            } => {
                self.next_tab();
                None
            }

            // Shift+Tab / Left → prev tab
            KeyEvent {
                code: KeyCode::BackTab,
                modifiers: KeyModifiers::SHIFT,
                ..
            }
            | KeyEvent {
                code: KeyCode::Left,
                modifiers: KeyModifiers::NONE,
                ..
            }
            | KeyEvent {
                code: KeyCode::Char('h'),
                modifiers: KeyModifiers::NONE,
                ..
            } => {
                self.prev_tab();
                None
            }

            // 1-9 → direct option selection
            KeyEvent {
                code: KeyCode::Char(ch @ '1'..='9'),
                modifiers: KeyModifiers::NONE,
                ..
            } => {
                let idx =
                    ch.to_digit(10).expect("digit char '1'..='9' always yields a valid digit") as usize
                    - 1;
                if idx < total {
                    self.selected_option = idx;
                    // Simulate Enter
                    return self.handle_selection_key(KeyEvent::new(
                        KeyCode::Enter,
                        KeyModifiers::NONE,
                    ));
                }
                None
            }

            _ => None,
        }
    }

    fn handle_confirm_key(
        &mut self,
        key: KeyEvent,
    ) -> Option<(String, Vec<Vec<String>>)> {
        match key {
            KeyEvent {
                code: KeyCode::Enter,
                modifiers: KeyModifiers::NONE,
                ..
            } => {
                let request_id = self.request_id.clone().unwrap_or_default();
                let answers = std::mem::take(&mut self.answers);
                Some((request_id, answers))
            }
            KeyEvent {
                code: KeyCode::Esc, ..
            } => {
                let request_id = self.request_id.clone().unwrap_or_default();
                Some((request_id, Vec::new()))
            }
            KeyEvent {
                code: KeyCode::Tab,
                modifiers: KeyModifiers::NONE,
                ..
            }
            | KeyEvent {
                code: KeyCode::Right,
                modifiers: KeyModifiers::NONE,
                ..
            } => {
                self.tab = 0;
                self.selected_option = 0;
                None
            }
            KeyEvent {
                code: KeyCode::BackTab,
                modifiers: KeyModifiers::SHIFT,
                ..
            }
            | KeyEvent {
                code: KeyCode::Left,
                modifiers: KeyModifiers::NONE,
                ..
            } => {
                self.tab = self.questions.len().saturating_sub(1);
                self.selected_option = 0;
                None
            }
            _ => None,
        }
    }

    fn handle_editing_key(
        &mut self,
        key: KeyEvent,
    ) -> Option<(String, Vec<Vec<String>>)> {
        match key {
            KeyEvent {
                code: KeyCode::Esc, ..
            } => {
                self.editing = false;
                None
            }
            KeyEvent {
                code: KeyCode::Enter,
                modifiers: KeyModifiers::NONE,
                ..
            } => {
                let text = std::mem::take(&mut self.custom[self.tab]).trim().to_string();
                if !text.is_empty() {
                    let q = match self.questions.get(self.tab) {
                        Some(q) => q,
                        None => return None,
                    };
                    if q.multiple {
                        let answers = &mut self.answers[self.tab];
                        if !answers.contains(&text) {
                            answers.push(text);
                        }
                    } else {
                        self.answers[self.tab] = vec![text];
                        if self.is_single() {
                            let request_id = self.request_id.clone().unwrap_or_default();
                            let answers = std::mem::take(&mut self.answers);
                            self.editing = false;
                            return Some((request_id, answers));
                        }
                        self.next_tab();
                    }
                }
                self.editing = false;
                None
            }
            KeyEvent {
                code: KeyCode::Backspace,
                modifiers: KeyModifiers::NONE,
                ..
            } => {
                self.custom[self.tab].pop();
                None
            }
            KeyEvent {
                code: KeyCode::Char(ch),
                modifiers: KeyModifiers::NONE | KeyModifiers::SHIFT,
                ..
            } => {
                self.custom[self.tab].push(ch);
                None
            }
            _ => None,
        }
    }

    fn next_tab(&mut self) {
        let count = self.tab_count();
        self.tab = (self.tab + 1) % count;
        self.selected_option = 0;
    }

    fn prev_tab(&mut self) {
        let count = self.tab_count();
        self.tab = if self.tab == 0 {
            count.saturating_sub(1)
        } else {
            self.tab - 1
        };
        self.selected_option = 0;
    }
}

impl Default for QuestionState {
    fn default() -> Self {
        Self::new()
    }
}

/// Render the question prompt dialog as a centered overlay.
pub fn render_question(f: &mut Frame, area: Rect, state: &QuestionState) {
    if !state.visible {
        return;
    }

    // Calculate dialog area (centered, 60% width)
    let dialog_width = (area.width as f64 * 0.6) as u16;
    let dialog_height = 12;
    let dialog_x = (area.width - dialog_width) / 2;
    let dialog_y = (area.height - dialog_height) / 2;

    let dialog_area = Rect::new(
        area.x + dialog_x,
        area.y + dialog_y,
        dialog_width,
        dialog_height,
    );

    f.render_widget(Clear, dialog_area);

    let block = Block::default()
        .borders(Borders::ALL)
        .title(" Question ")
        .border_style(Style::default().fg(Color::Cyan))
        .style(Style::default().bg(Color::Black));

    let inner = block.inner(dialog_area);
    f.render_widget(block, dialog_area);

    let mut lines: Vec<Line> = Vec::new();

    // Tab bar (for multi-question)
    if !state.is_single() {
        let mut tab_spans: Vec<Span> = Vec::new();
        for (i, q) in state.questions.iter().enumerate() {
            let is_active = i == state.tab;
            let is_answered = !state.answers[i].is_empty();
            let header = q.header.as_deref().unwrap_or(&q.question);

            let style = if is_active {
                Style::default()
                    .fg(Color::Black)
                    .bg(Color::Cyan)
            } else if is_answered {
                Style::default().fg(Color::White)
            } else {
                Style::default().fg(Color::Gray)
            };
            tab_spans.push(Span::styled(format!(" {header} "), style));
        }
        // Confirm tab
        let confirm_style = if state.is_confirm_tab() {
            Style::default()
                .fg(Color::Black)
                .bg(Color::Cyan)
        } else {
            Style::default().fg(Color::Gray)
        };
        tab_spans.push(Span::styled(" Confirm ", confirm_style));

        lines.push(Line::from(tab_spans));
        lines.push(Line::from(""));
    }

    if state.is_confirm_tab() {
        // Review screen
        lines.push(Line::from(Span::styled(
            "Review your answers:",
            Style::default().fg(Color::White),
        )));
        for (i, q) in state.questions.iter().enumerate() {
            let value = if state.answers[i].is_empty() {
                "(not answered)".to_string()
            } else {
                state.answers[i].join(", ")
            };
            let header = q.header.as_deref().unwrap_or(&q.question);
            lines.push(Line::from(vec![
                Span::styled(format!("{header}: "), Style::default().fg(Color::Gray)),
                Span::styled(&value, Style::default().fg(Color::White)),
            ]));
        }
        lines.push(Line::from(""));
        lines.push(Line::from(Span::styled(
            "Enter submit  ·  Esc dismiss",
            Style::default().fg(Color::DarkGray),
        )));
    } else if let Some(q) = state.current_question() {
        // Question text
        lines.push(Line::from(Span::styled(
            &q.question,
            Style::default().fg(Color::White).add_modifier(Modifier::BOLD),
        )));
        if q.multiple {
            lines.push(Line::from(Span::styled(
                "(select all that apply)",
                Style::default().fg(Color::Gray),
            )));
        }
        lines.push(Line::from(""));

        // Options
        for (i, opt) in q.options.iter().enumerate() {
            let is_selected = i == state.selected_option;
            let is_picked = state.answers[state.tab].contains(&opt.label);

            let prefix = if q.multiple {
                format!("[{}] ", if is_picked { "✓" } else { " " })
            } else {
                format!("{}. ", i + 1)
            };

            let style = if is_selected {
                Style::default().fg(Color::Cyan)
            } else if is_picked {
                Style::default().fg(Color::Green)
            } else {
                Style::default().fg(Color::White)
            };

            lines.push(Line::from(vec![
                Span::styled(&prefix, style),
                Span::styled(&opt.label, style),
            ]));

            if let Some(ref desc) = opt.description {
                lines.push(Line::from(Span::styled(
                    format!("    {desc}"),
                    Style::default().fg(Color::DarkGray),
                )));
            }
        }

        // Custom option
        if q.custom {
            let custom_idx = q.options.len();
            let is_selected = state.selected_option == custom_idx;
            let is_picked = !state.custom[state.tab].is_empty();

            let prefix = if q.multiple {
                format!("[{}] ", if is_picked { "✓" } else { " " })
            } else {
                format!("{}. ", custom_idx + 1)
            };

            let style = if is_selected {
                Style::default().fg(Color::Cyan)
            } else if is_picked {
                Style::default().fg(Color::Green)
            } else {
                Style::default().fg(Color::White)
            };

            lines.push(Line::from(Span::styled(
                format!("{prefix}Type your own answer"),
                style,
            )));

            if state.editing {
                let custom_text = if state.custom[state.tab].is_empty() {
                    "(type your answer...)"
                } else {
                    &state.custom[state.tab]
                };
                lines.push(Line::from(Span::styled(
                    format!("    > {custom_text}"),
                    Style::default().fg(Color::White),
                )));
            }
        }

        lines.push(Line::from(""));
        lines.push(Line::from(Span::styled(
            "↑↓ select  ·  Enter confirm",
            Style::default().fg(Color::DarkGray),
        )));

        if !state.is_single() {
            lines.push(Line::from(Span::styled(
                "⇆ tab  ·  Esc dismiss",
                Style::default().fg(Color::DarkGray),
            )));
        } else {
            lines.push(Line::from(Span::styled(
                "Esc dismiss",
                Style::default().fg(Color::DarkGray),
            )));
        }
    }

    let content = Text::from(lines);
    let paragraph = Paragraph::new(content).wrap(Wrap { trim: true });
    f.render_widget(paragraph, inner);
}
