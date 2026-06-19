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
//!
//! ## Contextual info
//!
//! The dialog shows additional context based on permission type:
//! - `bash`: the command being run (from metadata.command)
//! - `edit` / `write`: the file path and diff preview (from metadata)
//! - `read` / `grep` / `glob`: the path or pattern

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

/// Extract contextual info from a permission request's metadata.
fn extract_context(request: &PermissionRequest) -> Vec<String> {
    let mut context: Vec<String> = Vec::new();
    let meta = &request.metadata;

    match request.permission.as_str() {
        "bash" => {
            // Show the command from metadata
            if let Some(cmd) = meta.get("command").and_then(|v| v.as_str()) {
                context.push(format!("Command: {cmd}"));
            } else if let Some(cmd) = meta.get("cmd").and_then(|v| v.as_str()) {
                context.push(format!("Command: {cmd}"));
            }
            if let Some(cwd) = meta.get("cwd").and_then(|v| v.as_str()) {
                context.push(format!("In: {cwd}"));
            }
        }
        "edit" | "write" => {
            // Show the file path and diff if available
            if let Some(path) = meta.get("file").and_then(|v| v.as_str()) {
                context.push(format!("File: {path}"));
            } else if let Some(path) = meta.get("path").and_then(|v| v.as_str()) {
                context.push(format!("File: {path}"));
            }
            if let Some(diff) = meta.get("diff").and_then(|v| v.as_str()) {
                // Show first 5 lines of diff as preview
                let preview: String = diff.lines().take(5).collect::<Vec<_>>().join("\n");
                context.push(format!("Diff preview:\n{preview}"));
                let remaining = diff.lines().count().saturating_sub(5);
                if remaining > 0 {
                    context.push(format!("  ... and {remaining} more lines"));
                }
            }
        }
        "read" | "glob" | "grep" => {
            if let Some(path) = meta.get("path").and_then(|v| v.as_str()) {
                context.push(format!("Path: {path}"));
            } else if let Some(pattern) = meta.get("pattern").and_then(|v| v.as_str()) {
                context.push(format!("Pattern: {pattern}"));
            }
        }
        "delete" => {
            if let Some(path) = meta.get("path").and_then(|v| v.as_str()) {
                context.push(format!("Delete: {path}"));
            }
        }
        "web" | "url" | "fetch" => {
            if let Some(url) = meta.get("url").and_then(|v| v.as_str()) {
                // Truncate long URLs
                let display = if url.len() > 60 {
                    format!("{}...", &url[..57])
                } else {
                    url.to_string()
                };
                context.push(format!("URL: {display}"));
            }
        }
        _ => {
            // Generic: show any string metadata values
            for (key, value) in meta.as_object().into_iter().flat_map(|o| o.iter()) {
                if let Some(s) = value.as_str() {
                    if key != "id" && key != "sessionID" && key != "session_id" {
                        let truncated = if s.len() > 80 {
                            format!("{}...", &s[..77])
                        } else {
                            s.to_string()
                        };
                        context.push(format!("{key}: {truncated}"));
                    }
                }
            }
        }
    }

    context
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

    // Calculate dialog area — height depends on content
    let context_lines = extract_context(request);
    let patterns_section = if request.patterns.is_empty() { 0 } else { 2 + request.patterns.len() };
    let context_height = context_lines.len();
    let base_height = 10;
    let extra = patterns_section + context_height;
    let dialog_height = (base_height + extra) as u16;

    let dialog_width = (area.width as f64 * 0.65).min(90.0) as u16;
    let dialog_x = (area.width.saturating_sub(dialog_width)) / 2;
    let dialog_y = (area.height.saturating_sub(dialog_height)) / 2;

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

    // Permission type header
    lines.push(Line::from(vec![
        Span::styled("△ ", Style::default().fg(Color::Yellow)),
        Span::styled(
            &request.permission,
            Style::default().fg(Color::White).add_modifier(Modifier::BOLD),
        ),
    ]));

    // Contextual info (bash command, file path, etc.)
    if !context_lines.is_empty() {
        lines.push(Line::from(""));
        for ctx_line in &context_lines {
            for subline in ctx_line.lines() {
                lines.push(Line::from(Span::styled(
                    format!("  {subline}"),
                    Style::default().fg(Color::Gray),
                )));
            }
        }
    }

    // Patterns
    if !request.patterns.is_empty() {
        lines.push(Line::from(""));
        lines.push(Line::from(Span::styled(
            "Patterns:",
            Style::default().fg(Color::DarkGray),
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
            // Show which patterns will be saved
            if !request.always.is_empty() {
                lines.push(Line::from(Span::styled(
                    "Will allow these patterns:",
                    Style::default().fg(Color::Gray),
                )));
                for pattern in &request.always {
                    lines.push(Line::from(Span::styled(
                        format!("  ✓ {pattern}"),
                        Style::default().fg(Color::Green),
                    )));
                }
                lines.push(Line::from(""));
            }
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
            let display_text = if state.reject_message.is_empty() {
                "(type a message, Enter to confirm, Esc to cancel)"
            } else {
                &state.reject_message
            };
            lines.push(Line::from(Span::styled(
                display_text,
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
