//! Status line — footer bar with directory, MCP status, and version.
//!
//! Ported from: `packages/tui/src/feature-plugins/home/footer.tsx`
//!
//! ## Visual Design (Opencode Match)
//!
//! ```text
//! ~/.openclaw/workspace:master  ⊙ 0 MCP  /status  ←→  0.3.0
//! ```

use ratatui::{
    layout::{Alignment, Rect},
    style::{Color, Style},
    text::{Line, Span},
    widgets::Paragraph,
    Frame,
};
use crate::theme::Theme;
use blazecode_core::session::SessionStatus;

/// State for the status line.
#[derive(Debug, Default, Clone)]
pub struct StatusState {
    /// Working directory.
    pub directory: String,
    /// Git branch name, if in a repo.
    pub git_branch: Option<String>,
    /// Number of connected MCP servers.
    pub mcp_count: usize,
    /// Whether any MCP server has failed.
    pub mcp_error: bool,
    /// Current provider name.
    pub provider_name: Option<String>,
    /// Current model name.
    pub model_name: Option<String>,
    /// Total token count (input + output).
    pub token_count: Option<u64>,
    /// Total cost in USD.
    pub cost: f64,
    /// Whether connected to a provider.
    pub connected: bool,
    /// Whether to show a welcome message.
    pub show_welcome: bool,
    /// Number of pending permission requests.
    pub permission_count: usize,
    /// Number of connected LSP servers.
    pub lsp_count: usize,
    /// Current session status.
    pub session_status: Option<SessionStatus>,
}

impl StatusState {
    pub fn new() -> Self {
        Self {
            directory: std::env::current_dir()
                .map(|p| p.to_string_lossy().to_string())
                .unwrap_or_else(|_| "/".to_string()),
            ..Default::default()
        }
    }
}

/// Render the status line — three zones: directory:branch, MCP, version
pub fn render_status(f: &mut Frame, area: Rect, state: &StatusState, theme: &Theme) {
    if area.width < 20 {
        return;
    }

    // ── Left: directory + git branch ─────────────────────────────────
    let max_dir_len = (area.width as usize / 3).max(10);
    let dir_display = abbreviate_home(&state.directory, max_dir_len);

    let mut left_spans: Vec<Span> = Vec::new();
    left_spans.push(Span::styled(&dir_display, Style::default().fg(theme.text_muted)));

    if let Some(ref branch) = state.git_branch {
        left_spans.push(Span::styled(":", Style::default().fg(theme.text_muted)));
        left_spans.push(Span::styled(branch.clone(), Style::default().fg(theme.text_muted)));
    }

    // ── Right: pure version only (Opencode match) ───────────────────
    // Opencode shows: ~/dir  ←→  v1.17.9
    let mut right_spans: Vec<Span> = Vec::new();

    if state.mcp_count > 0 {
        right_spans.push(Span::styled(
            if state.mcp_error { "⊙" } else { "●" },
            if state.mcp_error {
                Style::default().fg(Color::Red)
            } else {
                Style::default().fg(Color::Green)
            },
        ));
        right_spans.push(Span::raw(format!(" {}  ", state.mcp_count)));
    }

    // Version only — Opencode doesn't show /status hint
    right_spans.push(Span::styled(
        env!("CARGO_PKG_VERSION"),
        Style::default().fg(theme.text_muted),
    ));

    // ── Assemble ─────────────────────────────────────────────────────
    let left_line = Line::from(left_spans);
    let right_is_empty = right_spans.is_empty();
    let right_line = Line::from(right_spans);

    // Use flex-like layout: left text, push right
    let left_widget = Paragraph::new(left_line);
    f.render_widget(left_widget, area);

    if !right_is_empty {
        let right_widget = Paragraph::new(right_line).alignment(Alignment::Right);
        f.render_widget(right_widget, area);
    }
}

/// Abbreviate home directory to ~/ and truncate.
fn abbreviate_home(dir: &str, max_len: usize) -> String {
    let home = std::env::var("HOME").unwrap_or_default();
    let abbreviated = if !home.is_empty() && dir.starts_with(&home) {
        format!("~{}", &dir[home.len()..])
    } else {
        dir.to_string()
    };

    if abbreviated.len() > max_len {
        format!(
            "...{}",
            &abbreviated[abbreviated.len().saturating_sub(max_len - 3)..]
        )
    } else {
        abbreviated
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_abbreviate_home_works() {
        let result = abbreviate_home("/home/user/projects/myapp", "/home/user".len() + 10);
        assert!(result.starts_with("~"));
        assert!(result.len() <= "/home/user".len() + 10);
    }

    #[test]
    fn test_abbreviate_short_dir() {
        let result = abbreviate_home("/tmp", 50);
        assert_eq!(result, "/tmp");
    }
}
