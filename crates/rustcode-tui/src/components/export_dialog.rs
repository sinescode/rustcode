//! Session export dialog — choose format, toggle sanitization, preview, and write file.
//!
//! Ported from: `packages/tui/src/component/dialog-session-export.tsx`
//!
//! The export dialog lets users export the current session to a file in
//! JSON, Markdown, or HTML format, with optional sanitization of sensitive
//! data (API keys, tokens).
//!
//! ## Key bindings
//!
//! | Key | Action |
//! |-----|--------|
//! | `Up` / `Down` | Navigate options |
//! | `Enter` | Confirm / write file |
//! | `Tab` | Switch between fields |
//! | `Esc` | Close dialog |
//! | `s` | Toggle sanitize |
//! | Any printable | Type in filename field |

use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
use ratatui::{
    layout::{Constraint, Direction, Layout, Rect},
    style::{Color, Modifier, Style},
    text::{Line, Span, Text},
    widgets::{Block, Borders, Clear, Paragraph, Wrap},
    Frame,
};

/// Export format options.
#[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
pub enum ExportFormat {
    /// JSON with message tree structure.
    #[default]
    Json,
    /// Human-readable Markdown.
    Markdown,
    /// Styled HTML document.
    Html,
}

impl ExportFormat {
    pub fn label(&self) -> &'static str {
        match self {
            ExportFormat::Json => "JSON",
            ExportFormat::Markdown => "Markdown",
            ExportFormat::Html => "HTML",
        }
    }

    pub fn extension(&self) -> &'static str {
        match self {
            ExportFormat::Json => ".json",
            ExportFormat::Markdown => ".md",
            ExportFormat::Html => ".html",
        }
    }

    pub fn description(&self) -> &'static str {
        match self {
            ExportFormat::Json => {
                "Full message tree with metadata, tool calls, and reasoning — machine-readable."
            }
            ExportFormat::Markdown => {
                "Conversation logs with code blocks — readable in any editor."
            }
            ExportFormat::Html => "Styled standalone HTML page with syntax highlighting.",
        }
    }

    pub fn all() -> [ExportFormat; 3] {
        [
            ExportFormat::Json,
            ExportFormat::Markdown,
            ExportFormat::Html,
        ]
    }
}

/// Where the input focus is in the export dialog.
#[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
pub enum ExportFocus {
    /// Navigating format list.
    #[default]
    FormatList,
    /// Editing the filename.
    Filename,
    /// Confirming the export.
    Confirm,
}

/// State for the session export dialog.
#[derive(Debug, Default)]
pub struct ExportState {
    /// Whether the dialog is visible.
    pub visible: bool,
    /// Selected export format.
    pub format: ExportFormat,
    /// Selection index in the format list (0=JSON, 1=MD, 2=HTML).
    pub format_selection: usize,
    /// Whether to sanitize sensitive data.
    pub sanitize: bool,
    /// User-entered filename (without extension).
    pub filename: String,
    /// Generated preview text.
    pub preview: String,
    /// Which field has focus.
    pub focus: ExportFocus,
    /// Whether export was confirmed (reset after handling).
    pub confirmed: bool,
    /// Approximate size estimate of the export.
    pub estimated_size: String,
}

impl ExportState {
    pub fn new() -> Self {
        Self {
            visible: false,
            format: ExportFormat::Markdown,
            format_selection: 1, // Default to Markdown
            sanitize: true,
            filename: String::new(),
            preview: String::new(),
            focus: ExportFocus::FormatList,
            confirmed: false,
            estimated_size: String::new(),
        }
    }

    /// Show the dialog with the given session data.
    pub fn show(&mut self, session_id: Option<&str>, message_count: usize) {
        self.visible = true;
        self.confirmed = false;
        self.focus = ExportFocus::FormatList;

        // Default filename from session ID
        self.filename = session_id
            .map(|sid| format!("session_{}", &sid[..sid.len().min(8)]))
            .unwrap_or_else(|| "export".to_string());

        // Compute estimated size
        let est_bytes = message_count * 2048; // Rough: 2KB per message
        self.estimated_size = if est_bytes < 1024 {
            format!("{} B", est_bytes)
        } else if est_bytes < 1024 * 1024 {
            format!("{:.1} KB", est_bytes as f64 / 1024.0)
        } else {
            format!("{:.1} MB", est_bytes as f64 / (1024.0 * 1024.0))
        };

        // Generate preview
        self.generate_preview();
    }

    /// Hide the dialog.
    pub fn hide(&mut self) {
        self.visible = false;
    }

    /// Generate a preview sample based on current settings.
    fn generate_preview(&mut self) {
        let sanitize_note = if self.sanitize {
            "// [SANITIZED: API keys and tokens will be redacted]\n"
        } else {
            ""
        };

        self.preview = match self.format {
            ExportFormat::Json => format!(
                r#"{sanitize_note}{{
  "sessionID": "ses_abc123...",
  "title": "Example session",
  "model": "claude-sonnet-4-6",
  "messages": [
    {{
      "role": "user",
      "text": "Hello, can you help me write a Rust function?"
    }},
    {{
      "role": "assistant",
      "text": "Sure! Here is a function that...",
      "toolCalls": [...]
    }}
  ],
  "cost": 0.0042,
  "tokens": {{ "input": 1234, "output": 567 }}
}}
..."#
            ),
            ExportFormat::Markdown => format!(
                r#"{sanitize_note}# Session Export — ses_abc123

## Messages

### User (2026-06-18 10:30)
Hello, can you help me write a Rust function?

### Assistant (2026-06-18 10:30)
Sure! Here is a function that computes the factorial...

```rust
fn factorial(n: u64) -> u64 {{
    match n {{
        0 | 1 => 1,
        _ => n * factorial(n - 1),
    }}
}}
```

**Cost**: $0.0042 | **Tokens**: 1,234 in / 567 out
..."#
            ),
            ExportFormat::Html => format!(
                r#"{}<!DOCTYPE html>
<html><head><title>Session Export</title></head>
<body>
<h1>Session Export — ses_abc123</h1>
<div class="message user">
  <strong>User</strong>
  <p>Hello, can you help me write a Rust function?</p>
</div>
<div class="message assistant">
  <strong>Assistant</strong>
  <pre><code>fn factorial(n: u64) -> u64 {{ ... }}</code></pre>
</div>
</body></html>
..."#,
                sanitize_note
            ),
        };
    }

    /// Cycle to the next format.
    pub fn next_format(&mut self) {
        let all = ExportFormat::all();
        let idx = all.iter().position(|f| *f == self.format).unwrap_or(0);
        let next = (idx + 1) % all.len();
        self.format = all[next];
        self.format_selection = next;
        self.generate_preview();
    }

    /// Cycle to the previous format.
    pub fn prev_format(&mut self) {
        let all = ExportFormat::all();
        let idx = all.iter().position(|f| *f == self.format).unwrap_or(0);
        let prev = if idx == 0 { all.len() - 1 } else { idx - 1 };
        self.format = all[prev];
        self.format_selection = prev;
        self.generate_preview();
    }

    /// Get the full output filename (with extension).
    pub fn full_filename(&self) -> String {
        if self.filename.is_empty() {
            format!("export{}", self.format.extension())
        } else {
            format!("{}{}", self.filename, self.format.extension())
        }
    }

    /// Handle a key event. Returns the action to take.
    pub fn handle_key(&mut self, key: KeyEvent) -> Option<ExportAction> {
        if !self.visible {
            return None;
        }

        match key {
            KeyEvent {
                code: KeyCode::Esc, ..
            } => {
                self.hide();
                Some(ExportAction::Close)
            }

            KeyEvent {
                code: KeyCode::Tab,
                modifiers: KeyModifiers::NONE,
                ..
            } => {
                // Cycle focus: FormatList -> Filename -> Confirm -> FormatList
                self.focus = match self.focus {
                    ExportFocus::FormatList => ExportFocus::Filename,
                    ExportFocus::Filename => ExportFocus::Confirm,
                    ExportFocus::Confirm => ExportFocus::FormatList,
                };
                Some(ExportAction::Navigate)
            }

            KeyEvent {
                code: KeyCode::BackTab,
                modifiers: KeyModifiers::SHIFT,
                ..
            } => {
                self.focus = match self.focus {
                    ExportFocus::FormatList => ExportFocus::Confirm,
                    ExportFocus::Filename => ExportFocus::FormatList,
                    ExportFocus::Confirm => ExportFocus::Filename,
                };
                Some(ExportAction::Navigate)
            }

            KeyEvent {
                code: KeyCode::Enter,
                modifiers: KeyModifiers::NONE,
                ..
            } => {
                match self.focus {
                    ExportFocus::Confirm => {
                        self.confirmed = true;
                        Some(ExportAction::Export {
                            filename: self.full_filename(),
                            format: self.format,
                            sanitize: self.sanitize,
                        })
                    }
                    _ => {
                        // Enter on format or filename advances focus
                        self.handle_key(KeyEvent::new(KeyCode::Tab, KeyModifiers::NONE))
                    }
                }
            }

            // Navigation in format list
            KeyEvent {
                code: KeyCode::Up, ..
            } if self.focus == ExportFocus::FormatList => {
                self.prev_format();
                Some(ExportAction::Navigate)
            }

            KeyEvent {
                code: KeyCode::Down,
                ..
            } if self.focus == ExportFocus::FormatList => {
                self.next_format();
                Some(ExportAction::Navigate)
            }

            // Toggle sanitize
            KeyEvent {
                code: KeyCode::Char('s'),
                modifiers: KeyModifiers::NONE,
                ..
            } if self.focus == ExportFocus::FormatList => {
                self.sanitize = !self.sanitize;
                self.generate_preview();
                Some(ExportAction::Navigate)
            }

            // Filename editing
            KeyEvent {
                code: KeyCode::Char(ch),
                modifiers: KeyModifiers::NONE | KeyModifiers::SHIFT,
                ..
            } if self.focus == ExportFocus::Filename => {
                self.filename.push(ch);
                Some(ExportAction::Navigate)
            }

            KeyEvent {
                code: KeyCode::Backspace,
                modifiers: KeyModifiers::NONE,
                ..
            } if self.focus == ExportFocus::Filename => {
                self.filename.pop();
                Some(ExportAction::Navigate)
            }

            // Confirm: Y/N or Enter
            KeyEvent {
                code: KeyCode::Char('y'),
                modifiers: KeyModifiers::NONE,
                ..
            } if self.focus == ExportFocus::Confirm => {
                self.confirmed = true;
                Some(ExportAction::Export {
                    filename: self.full_filename(),
                    format: self.format,
                    sanitize: self.sanitize,
                })
            }

            KeyEvent {
                code: KeyCode::Char('n'),
                modifiers: KeyModifiers::NONE,
                ..
            } if self.focus == ExportFocus::Confirm => {
                self.focus = ExportFocus::FormatList;
                Some(ExportAction::Navigate)
            }

            _ => Some(ExportAction::Navigate),
        }
    }
}

/// Actions returned by the export dialog key handler.
#[derive(Debug, Clone)]
pub enum ExportAction {
    /// Close the dialog.
    Close,
    /// Navigation occurred (redraw needed).
    Navigate,
    /// Write the export file with these parameters.
    Export {
        filename: String,
        format: ExportFormat,
        sanitize: bool,
    },
}

/// Render the export dialog as a modal dialog.
pub fn render_export_dialog(f: &mut Frame, area: Rect, state: &ExportState) {
    if !state.visible {
        return;
    }

    let dialog_width = (area.width as f64 * 0.65).min(90.0).max(50.0) as u16;
    let dialog_height = (area.height as f64 * 0.75).min(35.0).max(20.0) as u16;
    let dialog_x = (area.width.saturating_sub(dialog_width)) / 2;
    let dialog_y = (area.height.saturating_sub(dialog_height)) / 4;

    let dialog_area = Rect::new(
        area.x + dialog_x,
        area.y + dialog_y,
        dialog_width,
        dialog_height,
    );

    f.render_widget(Clear, dialog_area);

    let block = Block::default()
        .borders(Borders::ALL)
        .title(" Export Session ")
        .title_bottom(" Tab:switch  Enter:confirm  s:sanitize  Esc:close ")
        .border_style(Style::default().fg(Color::Cyan))
        .style(Style::default().bg(Color::Black));

    let inner = block.inner(dialog_area);
    f.render_widget(block, dialog_area);

    // ── Layout ─────────────────────────────────────────────────
    // 1. Format list + filename (top section)
    // 2. Sanitize + estimated size
    // 3. Preview
    // 4. Confirm button
    let rows = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(7), // Format + filename
            Constraint::Length(2), // Sanitize + size
            Constraint::Min(4),    // Preview
            Constraint::Length(3), // Confirm
        ])
        .split(inner);

    // ── Row 1: Format selector + filename ──────────────────────
    let top_cols = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([Constraint::Percentage(50), Constraint::Percentage(50)])
        .split(rows[0]);

    // Left: format list
    let format_block = Block::default()
        .borders(Borders::NONE)
        .style(
            Style::default().fg(if state.focus == ExportFocus::FormatList {
                Color::Yellow
            } else {
                Color::White
            }),
        );

    let format_inner = format_block.inner(top_cols[0]);
    f.render_widget(format_block, top_cols[0]);

    let format_text: Vec<Line> = ExportFormat::all()
        .iter()
        .enumerate()
        .map(|(i, fmt)| {
            let selected = i == state.format_selection;
            let prefix = if selected { " > " } else { "   " };
            let style = if selected {
                Style::default()
                    .fg(Color::Black)
                    .bg(if state.focus == ExportFocus::FormatList {
                        Color::Cyan
                    } else {
                        Color::Gray
                    })
            } else {
                Style::default().fg(Color::Gray)
            };
            Line::from(vec![
                Span::styled(prefix, style),
                Span::styled(fmt.label(), style.add_modifier(Modifier::BOLD)),
                Span::raw("  "),
                Span::styled(
                    format!("({})", fmt.extension()),
                    Style::default().fg(Color::DarkGray),
                ),
            ])
        })
        .collect();

    f.render_widget(Paragraph::new(Text::from(format_text)), format_inner);

    // Right: filename + format description
    let right_cols = Layout::default()
        .direction(Direction::Vertical)
        .constraints([Constraint::Length(3), Constraint::Min(0)])
        .split(top_cols[1]);

    // Filename input
    let filename_label = if state.focus == ExportFocus::Filename {
        " Filename ▶ "
    } else {
        " Filename   "
    };

    let filename_block = Block::default()
        .borders(Borders::BOTTOM)
        .border_style(
            Style::default().fg(if state.focus == ExportFocus::Filename {
                Color::Yellow
            } else {
                Color::DarkGray
            }),
        )
        .title(filename_label);

    let filename_inner = filename_block.inner(right_cols[0]);
    f.render_widget(filename_block, right_cols[0]);

    let display_name = format!(
        "{}{}",
        if state.filename.is_empty() {
            "export"
        } else {
            &state.filename
        },
        state.format.extension()
    );

    let cursor = if state.focus == ExportFocus::Filename {
        "█"
    } else {
        ""
    };

    let filename_span = Line::from(vec![
        Span::styled(
            &display_name,
            Style::default()
                .fg(Color::White)
                .add_modifier(Modifier::BOLD),
        ),
        Span::styled(cursor, Style::default().fg(Color::Yellow)),
    ]);

    f.render_widget(Paragraph::new(filename_span), filename_inner);

    // Format description
    let desc = state.format.description();
    f.render_widget(
        Paragraph::new(desc)
            .style(Style::default().fg(Color::DarkGray))
            .wrap(Wrap { trim: true }),
        right_cols[1],
    );

    // ── Row 2: Sanitize + Estimated size ───────────────────────
    let sanitize_marker = if state.sanitize {
        "[x] Sanitize sensitive data"
    } else {
        "[ ] Sanitize sensitive data"
    };

    let sanitize_style = if state.sanitize {
        Style::default().fg(Color::Green)
    } else {
        Style::default().fg(Color::DarkGray)
    };

    let size_text = format!("Est. size: {}", state.estimated_size);
    let row2_line = Line::from(vec![
        Span::styled(sanitize_marker, sanitize_style.add_modifier(Modifier::BOLD)),
        Span::raw("     "),
        Span::styled(size_text, Style::default().fg(Color::DarkGray)),
    ]);

    f.render_widget(Paragraph::new(row2_line), rows[1]);

    // ── Row 3: Preview ────────────────────────────────────────
    let preview_block = Block::default()
        .borders(Borders::ALL)
        .title(format!(" Preview ({}) ", state.format.label()))
        .border_style(Style::default().fg(Color::Blue));

    let preview_inner = preview_block.inner(rows[2]);
    f.render_widget(preview_block, rows[2]);

    f.render_widget(
        Paragraph::new(state.preview.as_str())
            .style(Style::default().fg(Color::White))
            .wrap(Wrap { trim: true }),
        preview_inner,
    );

    // ── Row 4: Confirm button ──────────────────────────────────
    let confirm_is_focused = state.focus == ExportFocus::Confirm;
    let confirm_style = if confirm_is_focused {
        Style::default()
            .fg(Color::Black)
            .bg(Color::Green)
            .add_modifier(Modifier::BOLD)
    } else {
        Style::default().fg(Color::Green)
    };

    let confirm_text = format!(
        "[ {} Export as {} ]",
        if confirm_is_focused { "ENTER" } else { "Tab" },
        state.full_filename()
    );

    let confirm_line = Line::from(vec![
        Span::styled(" ".repeat(2), Style::default()),
        Span::styled(confirm_text, confirm_style),
    ]);

    f.render_widget(Paragraph::new(confirm_line), rows[3]);
}
