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
//! - Working directory + git branch
//! - Provider + model name + token count + cost

use ratatui::{
    layout::{Alignment, Constraint, Direction, Layout, Rect},
    style::{Color, Style},
    text::{Line, Span},
    widgets::Paragraph,
    Frame,
};
use rustcode_core::format::{format_cost, format_tokens};
use rustcode_core::session::SessionStatus;

use crate::theme::Theme;

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
    /// Git branch name, if in a repo.
    pub git_branch: Option<String>,
    /// Current provider name.
    pub provider_name: Option<String>,
    /// Current model name.
    pub model_name: Option<String>,
    /// Total token count (input + output).
    pub token_count: Option<u64>,
    /// Total cost in USD.
    pub cost: f64,
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
///
/// Three-column layout:
/// - Left: directory + git branch
/// - Center: session status indicator
/// - Right: provider/model + tokens + cost + permission badge
pub fn render_status(f: &mut Frame, area: Rect, state: &StatusState, theme: &Theme) {
    if area.width < 20 {
        // Too narrow for a meaningful status line
        return;
    }

    // ── Left column: directory + git branch ─────────────────────────────
    let mut left_spans: Vec<Span> = Vec::new();

    // Directory (truncated to fit ~40% of width)
    let max_dir_len = (area.width as usize / 3).max(10);
    let dir_display = if state.directory.len() > max_dir_len {
        let home = std::env::var("HOME").unwrap_or_default();
        if !home.is_empty() && state.directory.starts_with(&home) {
            format!(
                "~/{}",
                &state.directory[home.len()..].trim_start_matches('/')
            )
        } else {
            state.directory.clone()
        }
    } else {
        state.directory.clone()
    };
    let dir_display = if dir_display.len() > max_dir_len {
        format!(
            "...{}",
            &dir_display[dir_display.len().saturating_sub(max_dir_len - 3)..]
        )
    } else {
        dir_display
    };

    left_spans.push(Span::styled(&dir_display, Style::default().fg(theme.dim)));

    // Git branch
    if let Some(ref branch) = state.git_branch {
        left_spans.push(Span::raw(" "));
        left_spans.push(Span::styled(
            format!("({branch})"),
            Style::default().fg(theme.accent),
        ));
    }

    let left_line = Line::from(left_spans);

    // ── Center column: session status ──────────────────────────────────
    let mut center_spans: Vec<Span> = Vec::new();

    if state.show_welcome && !state.connected {
        center_spans.push(Span::styled(
            "Get started /connect",
            Style::default().fg(Color::DarkGray),
        ));
    } else if state.connected {
        // LSP count
        center_spans.push(Span::styled(
            if state.lsp_count > 0 { "•" } else { "·" },
            if state.lsp_count > 0 {
                Style::default().fg(Color::Green)
            } else {
                Style::default().fg(Color::DarkGray)
            },
        ));
        center_spans.push(Span::styled(
            format!(" {} LSP", state.lsp_count),
            Style::default().fg(Color::White),
        ));
        center_spans.push(Span::raw("  "));

        // MCP count
        if state.mcp_count > 0 {
            center_spans.push(Span::styled(
                "⊙ ",
                if state.mcp_error {
                    Style::default().fg(Color::Red)
                } else {
                    Style::default().fg(Color::Green)
                },
            ));
            center_spans.push(Span::styled(
                format!("{} MCP", state.mcp_count),
                Style::default().fg(Color::White),
            ));
            center_spans.push(Span::raw("  "));
        }

        // Permission count badge
        if state.permission_count > 0 {
            center_spans.push(Span::styled("△ ", Style::default().fg(Color::Yellow)));
            center_spans.push(Span::styled(
                format!(
                    "{} Permission{}",
                    state.permission_count,
                    if state.permission_count > 1 { "s" } else { "" }
                ),
                Style::default().fg(Color::Yellow),
            ));
            center_spans.push(Span::raw("  "));
        }

        // Status indicator
        if let Some(ref status) = state.session_status {
            match status {
                SessionStatus::Idle => {
                    center_spans.push(Span::styled("• idle", Style::default().fg(Color::Green)));
                }
                SessionStatus::Busy => {
                    center_spans.push(Span::styled("⟳ busy", Style::default().fg(Color::Yellow)));
                }
                SessionStatus::Retry {
                    attempt, message, ..
                } => {
                    center_spans.push(Span::styled(
                        format!("△ retry (attempt {attempt}) — {message}"),
                        Style::default().fg(Color::Red),
                    ));
                }
            }
        }
    }

    let center_line = Line::from(center_spans);

    // ── Right column: provider/model + tokens + cost ───────────────────
    let mut right_spans: Vec<Span> = Vec::new();

    if state.connected {
        // Provider + model
        if let Some(ref provider) = state.provider_name {
            let model_display = state.model_name.as_deref().unwrap_or("auto");
            right_spans.push(Span::styled(
                format!("{provider}/{model_display}"),
                Style::default().fg(Color::DarkGray),
            ));
        }

        // Token count
        if let Some(tokens) = state.token_count {
            if !right_spans.is_empty() {
                right_spans.push(Span::raw(" · "));
            }
            right_spans.push(Span::styled(
                format_tokens(tokens),
                Style::default().fg(Color::DarkGray),
            ));
        }

        // Cost
        if state.cost > 0.0 {
            if !right_spans.is_empty() {
                right_spans.push(Span::raw(" · "));
            }
            right_spans.push(Span::styled(
                format_cost(state.cost),
                Style::default().fg(if state.cost > 0.50 {
                    Color::Yellow
                } else {
                    Color::DarkGray
                }),
            ));
        }

        // Status shortcut hint
        if !right_spans.is_empty() {
            right_spans.push(Span::raw("  "));
        }
        right_spans.push(Span::styled(
            "/status",
            Style::default().fg(Color::DarkGray),
        ));
    }

    let right_line = Line::from(right_spans);

    // Assemble: use ratatui layout for left/center/right
    let columns = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([
            Constraint::Percentage(34),
            Constraint::Percentage(33),
            Constraint::Percentage(33),
        ])
        .split(area);

    let left_widget = Paragraph::new(left_line);
    f.render_widget(left_widget, columns[0]);

    let center_widget = Paragraph::new(center_line);
    f.render_widget(center_widget, columns[1]);

    let right_widget = Paragraph::new(right_line).alignment(Alignment::Right);
    f.render_widget(right_widget, columns[2]);
}
