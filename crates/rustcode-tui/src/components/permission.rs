//! Permission prompt dialog — modal dialog for permission requests.
//!
//! Ported from: `packages/tui/src/routes/session/permission.tsx` (721 lines)
//!
//! ## State machine
//!
//! The TS source implements a 3-stage state machine:
//! 1. `"permission"` — Show the request with options: Allow / Allow always / Reject
//! 2. `"always"` — Show patterns that will be always-allowed, Confirm / Cancel
//! 3. `"reject"` — Optional rejection message textarea, Confirm / Cancel
//!
//! In Rust, this is implemented as a modal dialog overlay with arrow key navigation.

use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
use ratatui::{
    layout::{Constraint, Direction, Layout, Rect},
    style::{Color, Modifier, Style},
    text::{Line, Span, Text},
    widgets::{Block, Borders, Clear, Paragraph, Wrap},
    Frame,
};
use rustcode_core::permission::PermissionRequest;

/// Stage of the permission prompt.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PermissionStage {
    /// Showing the permission request with options.
    Permission,
    /// Showing always-allow confirmation.
    AlwaysAllow,
    /// Showing rejection message input.
    Reject,
}

/// State for the permission prompt dialog.
#[derive(Debug)]
pub struct PermissionState {
    /// The permission request being handled.
    pub request: Option<PermissionRequest>,
    /// Current stage of the dialog.
    pub stage: PermissionStage,
    /// Currently selected option index (0 = once, 1 = always, 2 = reject in Permission stage).
    pub selected_option: usize,
    /// Rejection message text.
    pub reject_message: String,
    /// Whether the dialog is visible.
    pub visible: bool,
}

impl PermissionState {
    pub fn new() -> Self {
        Self {
            request: None,
            stage: PermissionStage::Permission,
            selected_option: 0,
            reject_message: String::new(),
            visible: false,
        }
    }

    /// Show a permission request.
    pub fn show(&mut self, request: PermissionRequest) {
        self.request = Some(request);
        self.stage = PermissionStage::Permission;
        self.selected_option = 0;
        self.reject_message.clear();
        self.visible = true;
    }

    /// Dismiss the dialog.
    pub fn dismiss(&mut self) {
        self.visible = false;
        self.request = None;
        self.stage = PermissionStage::Permission;
        self.selected_option = 0;
    }

    /// Move to the next option.
    pub fn next_option(&mut self) {
        let max = match self.stage {
            PermissionStage::Permission => 2, // once, always, reject
            PermissionStage::AlwaysAllow | PermissionStage::Reject => 1, // confirm, cancel
        };
        self.selected_option = (self.selected_option + 1) % (max + 1);
    }

    /// Move to the previous option.
    pub fn prev_option(&mut self) {
        let max = match self.stage {
            PermissionStage::Permission => 2,
            PermissionStage::AlwaysAllow | PermissionStage::Reject => 1,
        };
        self.selected_option = if self.selected_option == 0 {
            max
        } else {
            self.selected_option - 1
        };
    }

    /// Select the current option (Enter key).
    pub fn select(&mut self) -> Option<PermissionReply> {
        match self.stage {
            PermissionStage::Permission => match self.selected_option {
                0 => Some(PermissionReply::Once),
                1 => {
                    self.stage = PermissionStage::AlwaysAllow;
                    self.selected_option = 0;
                    None
                }
                2 => {
                    self.stage = PermissionStage::Reject;
                    self.selected_option = 0;
                    None
                }
                _ => None,
            },
            PermissionStage::AlwaysAllow => match self.selected_option {
                0 => Some(PermissionReply::Always),
                1 => {
                    self.stage = PermissionStage::Permission;
                    self.selected_option = 0;
                    None
                }
                _ => None,
            },
            PermissionStage::Reject => match self.selected_option {
                0 => Some(PermissionReply::Reject {
                    message: if self.reject_message.is_empty() {
                        None
                    } else {
                        Some(std::mem::take(&mut self.reject_message))
                    },
                }),
                1 => {
                    self.stage = PermissionStage::Permission;
                    self.selected_option = 0;
                    None
                }
                _ => None,
            },
        }
    }

    /// Handle a key event. Returns the permission reply if the dialog was confirmed.
    pub fn handle_key(&mut self, key: KeyEvent) -> Option<PermissionReply> {
        if !self.visible {
            return None;
        }

        match key {
            // Escape → reject/cancel
            KeyEvent {
                code: KeyCode::Esc, ..
            } => match self.stage {
                PermissionStage::Permission => Some(PermissionReply::Reject { message: None }),
                _ => {
                    self.stage = PermissionStage::Permission;
                    self.selected_option = 0;
                    None
                }
            },

            // Enter → select current option
            KeyEvent {
                code: KeyCode::Enter,
                modifiers: KeyModifiers::NONE,
                ..
            } => self.select(),

            // Left / h → prev option
            KeyEvent {
                code: KeyCode::Left,
                modifiers: KeyModifiers::NONE,
                ..
            }
            | KeyEvent {
                code: KeyCode::Char('h'),
                modifiers: KeyModifiers::NONE,
                ..
            } => {
                self.prev_option();
                None
            }

            // Right / l → next option
            KeyEvent {
                code: KeyCode::Right,
                modifiers: KeyModifiers::NONE,
                ..
            }
            | KeyEvent {
                code: KeyCode::Char('l'),
                modifiers: KeyModifiers::NONE,
                ..
            } => {
                self.next_option();
                None
            }

            // In reject stage: type message
            KeyEvent {
                code: KeyCode::Backspace,
                modifiers: KeyModifiers::NONE,
                ..
            } if self.stage == PermissionStage::Reject => {
                self.reject_message.pop();
                None
            }
            KeyEvent {
                code: KeyCode::Char(ch),
                modifiers: KeyModifiers::NONE | KeyModifiers::SHIFT,
                ..
            } if self.stage == PermissionStage::Reject => {
                self.reject_message.push(ch);
                None
            }

            _ => None,
        }
    }
}

impl Default for PermissionState {
    fn default() -> Self {
        Self::new()
    }
}

/// The reply to a permission request.
///
/// # Source
/// `PermissionV1.Reply` union in `permission.tsx`.
#[derive(Debug, Clone)]
pub enum PermissionReply {
    /// Allow this one time.
    Once,
    /// Always allow (with patterns).
    Always,
    /// Reject (optionally with a message).
    Reject { message: Option<String> },
}

/// Render the permission prompt dialog as a centered overlay.
pub fn render_permission(f: &mut Frame, area: Rect, state: &PermissionState) {
    if !state.visible {
        return;
    }

    let request = match &state.request {
        Some(req) => req,
        None => return,
    };

    // Calculate dialog area (centered, 60% width, auto height)
    let dialog_width = (area.width as f64 * 0.6) as u16;
    let dialog_height = 10;
    let dialog_x = (area.width - dialog_width) / 2;
    let dialog_y = (area.height - dialog_height) / 2;

    let dialog_area = Rect::new(
        area.x + dialog_x,
        area.y + dialog_y,
        dialog_width,
        dialog_height,
    );

    // Clear the background
    f.render_widget(Clear, dialog_area);

    let title = match state.stage {
        PermissionStage::Permission => " Permission Required ",
        PermissionStage::AlwaysAllow => " Always Allow ",
        PermissionStage::Reject => " Reject Permission ",
    };

    let border_color = match state.stage {
        PermissionStage::Permission => Color::Yellow,
        PermissionStage::AlwaysAllow => Color::Cyan,
        PermissionStage::Reject => Color::Red,
    };

    let block = Block::default()
        .borders(Borders::ALL)
        .title(title)
        .border_style(Style::default().fg(border_color))
        .style(Style::default().bg(Color::Black));

    let inner = block.inner(dialog_area);
    f.render_widget(block, dialog_area);

    // Build content
    let mut lines: Vec<Line> = Vec::new();

    // Permission type
    lines.push(Line::from(vec![
        Span::styled("△ ", Style::default().fg(Color::Yellow)),
        Span::styled(
            &request.permission,
            Style::default().fg(Color::White).add_modifier(Modifier::BOLD),
        ),
    ]));

    // Patterns (if any)
    if !request.patterns.is_empty() {
        lines.push(Line::from(""));
        lines.push(Line::from(Span::styled(
            "Patterns:",
            Style::default().fg(Color::Gray),
        )));
        for pattern in &request.patterns {
            lines.push(Line::from(Span::styled(
                format!("  - {pattern}"),
                Style::default().fg(Color::White),
            )));
        }
    }

    lines.push(Line::from(""));

    // Options
    match state.stage {
        PermissionStage::Permission => {
            let options = ["Allow once", "Allow always", "Reject"];
            let mut option_spans: Vec<Span> = Vec::new();
            for (i, opt) in options.iter().enumerate() {
                let style = if i == state.selected_option {
                    Style::default()
                        .fg(Color::Black)
                        .bg(Color::Yellow)
                } else {
                    Style::default()
                        .fg(Color::Gray)
                        .bg(Color::DarkGray)
                };
                option_spans.push(Span::styled(format!(" {opt} "), style));
                option_spans.push(Span::raw(" "));
            }
            lines.push(Line::from(option_spans));
            lines.push(Line::from(""));
            lines.push(Line::from(Span::styled(
                "← → select  ·  Enter confirm  ·  Esc reject",
                Style::default().fg(Color::DarkGray),
            )));
        }
        PermissionStage::AlwaysAllow => {
            let options = ["Confirm", "Cancel"];
            let mut option_spans: Vec<Span> = Vec::new();
            for (i, opt) in options.iter().enumerate() {
                let style = if i == state.selected_option {
                    Style::default()
                        .fg(Color::Black)
                        .bg(Color::Cyan)
                } else {
                    Style::default()
                        .fg(Color::Gray)
                        .bg(Color::DarkGray)
                };
                option_spans.push(Span::styled(format!(" {opt} "), style));
                option_spans.push(Span::raw(" "));
            }
            lines.push(Line::from(option_spans));
            lines.push(Line::from(""));
            lines.push(Line::from(Span::styled(
                "This will always allow matching patterns.",
                Style::default().fg(Color::Gray),
            )));
        }
        PermissionStage::Reject => {
            // Show rejection message input
            lines.push(Line::from(Span::styled(
                "Tell OpenCode what to do differently:",
                Style::default().fg(Color::Gray),
            )));
            lines.push(Line::from(Span::styled(
                if state.reject_message.is_empty() {
                    "(type a message, Enter to confirm, Esc to cancel)"
                } else {
                    &state.reject_message
                },
                Style::default().fg(Color::White),
            )));
            lines.push(Line::from(""));
            let options = ["Confirm", "Cancel"];
            let mut option_spans: Vec<Span> = Vec::new();
            for (i, opt) in options.iter().enumerate() {
                let style = if i == state.selected_option {
                    Style::default()
                        .fg(Color::Black)
                        .bg(Color::Red)
                } else {
                    Style::default()
                        .fg(Color::Gray)
                        .bg(Color::DarkGray)
                };
                option_spans.push(Span::styled(format!(" {opt} "), style));
                option_spans.push(Span::raw(" "));
            }
            lines.push(Line::from(option_spans));
            lines.push(Line::from(""));
            lines.push(Line::from(Span::styled(
                "Enter confirm  ·  Esc cancel",
                Style::default().fg(Color::DarkGray),
            )));
        }
    }

    let content = Text::from(lines);
    let paragraph = Paragraph::new(content).wrap(Wrap { trim: true });
    f.render_widget(paragraph, inner);
}
