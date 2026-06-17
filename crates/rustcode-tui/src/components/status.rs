//! Status line — displays busy/idle/retry state, LSP/MCP counts, directory.
//!
//! Ported from: `packages/tui/src/routes/session/footer.tsx`
//!
//! ## Status indicators
//!
//! | State | Display |
//! |-------|---------|
//! | idle  | `• idle` in green |
//! | busy  | `⟳ busy` in yellow with spinner |
//! | retry | `△ retry (attempt N)` in red |
//!
//! Additional indicators:
//! - LSP count (e.g. `• 2 LSP`)
//! - MCP count (e.g. `⊙ 3 MCP`), red if any failed
//! - Permission count (e.g. `△ 1 Permission`), warning color
//! - Working directory

use ratatui::{
    layout::Rect,
    style::{Color, Style},
    text::{Line, Span},
    widgets::Paragraph,
    Frame,
};
use rustcode_core::session::SessionStatus;

/// State for the status line.
#[derive(Debug, Default, Clone)]
pub struct StatusState {
    /// Current session status.
    pub session_status: Option<SessionStatus>,
    /// Whether connected to a provider.
    pub connected: bool,
    /// Number of active LSP servers.
    pub lsp_count: usize,
    /// Number of connected MCP servers.
    pub mcp_count: usize,
    /// Whether any MCP server has failed.
    pub mcp_error: bool,
    /// Number of pending permissions.
    pub permission_count: usize,
    /// Working directory.
    pub directory: String,
    /// Whether to show the "welcome" message.
    pub show_welcome: bool,
}

impl StatusState {
    pub fn new() -> Self {
        Self {
            directory: std::env::current_dir()
                .map(|p| p.to_string_lossy().to_string())
                .unwrap_or_else(|_| "/".to_string()),
            connected: false,
            show_welcome: true,
            ..Default::default()
        }
    }
}

/// Render the status line at the bottom of the screen.
pub fn render_status(f: &mut Frame, area: Rect, state: &StatusState) {
    let mut spans: Vec<Span> = Vec::new();

    // Directory (left-aligned)
    spans.push(Span::styled(
        &state.directory,
        Style::default().fg(Color::Gray),
    ));

    // Separator
    spans.push(Span::raw("  "));

    // Welcome or connected state
    if state.show_welcome && !state.connected {
        spans.push(Span::styled(
            "Get started ",
            Style::default().fg(Color::White),
        ));
        spans.push(Span::styled(
            "/connect",
            Style::default().fg(Color::DarkGray),
        ));
    } else if state.connected {
        // Permission count
        if state.permission_count > 0 {
            spans.push(Span::styled(
                "△ ",
                Style::default().fg(Color::Yellow),
            ));
            spans.push(Span::styled(
                format!("{} Permission{}", state.permission_count, if state.permission_count > 1 { "s" } else { "" }),
                Style::default().fg(Color::Yellow),
            ));
            spans.push(Span::raw("  "));
        }

        // LSP count
        spans.push(Span::styled(
            if state.lsp_count > 0 { "•" } else { "·" },
            if state.lsp_count > 0 {
                Style::default().fg(Color::Green)
            } else {
                Style::default().fg(Color::DarkGray)
            },
        ));
        spans.push(Span::styled(
            format!(" {} LSP", state.lsp_count),
            Style::default().fg(Color::White),
        ));
        spans.push(Span::raw("  "));

        // MCP count
        if state.mcp_count > 0 {
            spans.push(Span::styled(
                "⊙ ",
                if state.mcp_error {
                    Style::default().fg(Color::Red)
                } else {
                    Style::default().fg(Color::Green)
                },
            ));
            spans.push(Span::styled(
                format!("{} MCP", state.mcp_count),
                Style::default().fg(Color::White),
            ));
            spans.push(Span::raw("  "));
        }

        // Status indicator
        if let Some(ref status) = state.session_status {
            match status {
                SessionStatus::Idle => {
                    spans.push(Span::styled("• idle", Style::default().fg(Color::Green)));
                }
                SessionStatus::Busy => {
                    spans.push(Span::styled("⟳ busy", Style::default().fg(Color::Yellow)));
                }
                SessionStatus::Retry { attempt, message, .. } => {
                    spans.push(Span::styled(
                        format!("△ retry (attempt {attempt}) — {message}"),
                        Style::default().fg(Color::Red),
                    ));
                }
            }
        }

        spans.push(Span::raw("  "));
        spans.push(Span::styled(
            "/status",
            Style::default().fg(Color::DarkGray),
        ));
    }

    let line = Line::from(spans);
    let status_widget = Paragraph::new(line);
    f.render_widget(status_widget, area);
}
