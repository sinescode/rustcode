//! Session sidebar — right-side panel with Context, Todo, Files, LSP, MCP tabs.
//!
//! Ported from: `packages/tui/src/routes/session/sidebar.tsx` and feature-plugins.
//!
//! The sidebar is a right-aligned panel (40 chars wide) with a tab bar at the top
//! for selecting between panels: Context, Todo, Files, LSP, MCP. Toggled via
//! `Alt+B` or `Ctrl+X b` (leader chord).
//!
//! ## Panels
//!
//! - **Context**: Shows token count, percentage used, cost estimate.
//! - **Todo**: Collapsible todo list from the current session.
//! - **Files**: Modified files with +/- change counts.
//! - **LSP**: Connection dots (green = active, red = error, gray = idle).
//! - **MCP**: Server names with connection status dots.

use ratatui::{
    layout::{Constraint, Direction, Layout, Rect},
    style::{Color, Modifier, Style},
    text::{Line, Span, Text},
    widgets::{Block, Borders, Paragraph, Tabs, Wrap},
    Frame,
};

use crate::theme::Theme;

/// Available sidebar panels.
///
/// # Source
/// Ported from `packages/tui/src/routes/session/sidebar.tsx` panel definitions.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SidebarPanel {
    Context,
    Todo,
    Files,
    Lsp,
    Mcp,
}

impl SidebarPanel {
    /// All panels in tab order.
    pub fn all() -> &'static [SidebarPanel] {
        &[
            SidebarPanel::Context,
            SidebarPanel::Todo,
            SidebarPanel::Files,
            SidebarPanel::Lsp,
            SidebarPanel::Mcp,
        ]
    }

    /// Human-readable tab label.
    pub fn label(&self) -> &'static str {
        match self {
            SidebarPanel::Context => " Context ",
            SidebarPanel::Todo => " Todo ",
            SidebarPanel::Files => " Files ",
            SidebarPanel::Lsp => " LSP ",
            SidebarPanel::Mcp => " MCP ",
        }
    }

    /// Index in the tab order.
    pub fn index(&self) -> usize {
        match self {
            SidebarPanel::Context => 0,
            SidebarPanel::Todo => 1,
            SidebarPanel::Files => 2,
            SidebarPanel::Lsp => 3,
            SidebarPanel::Mcp => 4,
        }
    }

    /// Get panel from index.
    pub fn from_index(idx: usize) -> Option<Self> {
        match idx {
            0 => Some(SidebarPanel::Context),
            1 => Some(SidebarPanel::Todo),
            2 => Some(SidebarPanel::Files),
            3 => Some(SidebarPanel::Lsp),
            4 => Some(SidebarPanel::Mcp),
            _ => None,
        }
    }
}

/// Information about a modified file shown in the Files panel.
#[derive(Debug, Clone)]
pub struct FileChange {
    /// File path relative to workspace root.
    pub path: String,
    /// Number of lines added.
    pub additions: usize,
    /// Number of lines deleted.
    pub deletions: usize,
    /// Whether the file is staged.
    pub staged: bool,
}

/// Information about an LSP server connection.
#[derive(Debug, Clone)]
pub struct LspConnection {
    /// Server name (e.g., "rust-analyzer", "typescript-language-server").
    pub name: String,
    /// Connection status.
    pub status: LspStatus,
}

/// Status of an LSP server connection.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LspStatus {
    /// Server is running and connected.
    Connected,
    /// Server is starting up.
    Starting,
    /// Server has an error.
    Error,
    /// Server is idle / not needed.
    Idle,
}

impl LspStatus {
    fn color(&self) -> Color {
        match self {
            LspStatus::Connected => Color::Green,
            LspStatus::Starting => Color::Yellow,
            LspStatus::Error => Color::Red,
            LspStatus::Idle => Color::DarkGray,
        }
    }

    fn icon(&self) -> &'static str {
        match self {
            LspStatus::Connected => "●",
            LspStatus::Starting => "◐",
            LspStatus::Error => "✗",
            LspStatus::Idle => "○",
        }
    }
}

/// Information about an MCP server connection.
#[derive(Debug, Clone)]
pub struct McpConnection {
    /// Server name (e.g., "filesystem", "github").
    pub name: String,
    /// Connection status.
    pub connected: bool,
    /// Whether there was an error.
    pub has_error: bool,
    /// Tool count exposed by this MCP server.
    pub tool_count: usize,
}

/// A todo item from the session.
#[derive(Debug, Clone)]
pub struct TodoItem {
    /// The todo text.
    pub text: String,
    /// Whether the todo is complete.
    pub done: bool,
    /// Whether this todo section is collapsed (for parent items).
    pub collapsed: bool,
    /// Child items (for nested todos).
    pub children: Vec<TodoItem>,
    /// Indentation level.
    pub level: usize,
}

/// State for the session sidebar.
///
/// # Source
/// Ported from `packages/tui/src/routes/session/sidebar.tsx`.
#[derive(Debug)]
pub struct SidebarState {
    /// Whether the sidebar is visible.
    pub visible: bool,
    /// Currently active panel tab.
    pub active_panel: SidebarPanel,
    /// Width of the sidebar in characters.
    pub width: u16,

    // ── Context panel data ──────────────────────────────────────
    /// Total token count.
    pub token_count: u64,
    /// Context window limit (tokens).
    pub token_limit: u64,
    /// Percentage of context window used (0.0–100.0).
    pub context_used_pct: f64,
    /// Total cost in USD.
    pub cost: f64,
    /// Number of messages in context.
    pub message_count: usize,

    // ── Todo panel data ─────────────────────────────────────────
    /// Todo items from the session.
    pub todos: Vec<TodoItem>,

    // ── Files panel data ────────────────────────────────────────
    /// Modified files with change counts.
    pub changed_files: Vec<FileChange>,

    // ── LSP panel data ──────────────────────────────────────────
    /// LSP server connections.
    pub lsp_connections: Vec<LspConnection>,

    // ── MCP panel data ──────────────────────────────────────────
    /// MCP server connections.
    pub mcp_connections: Vec<McpConnection>,
}

impl Default for SidebarState {
    fn default() -> Self {
        Self {
            visible: false,
            active_panel: SidebarPanel::Context,
            width: 40,
            token_count: 0,
            token_limit: 200_000,
            context_used_pct: 0.0,
            cost: 0.0,
            message_count: 0,
            todos: Vec::new(),
            changed_files: Vec::new(),
            lsp_connections: Vec::new(),
            mcp_connections: Vec::new(),
        }
    }
}

impl SidebarState {
    pub fn new() -> Self {
        Self::default()
    }

    /// Toggle sidebar visibility.
    pub fn toggle(&mut self) {
        self.visible = !self.visible;
    }

    /// Show the sidebar.
    pub fn show(&mut self) {
        self.visible = true;
    }

    /// Hide the sidebar.
    pub fn hide(&mut self) {
        self.visible = false;
    }

    /// Cycle to the next panel tab.
    pub fn next_panel(&mut self) {
        let panels = SidebarPanel::all();
        let idx = self.active_panel.index();
        let next_idx = (idx + 1) % panels.len();
        self.active_panel = SidebarPanel::from_index(next_idx).unwrap_or(SidebarPanel::Context);
    }

    /// Cycle to the previous panel tab.
    pub fn prev_panel(&mut self) {
        let panels = SidebarPanel::all();
        let idx = self.active_panel.index();
        let prev_idx = if idx == 0 { panels.len() - 1 } else { idx - 1 };
        self.active_panel = SidebarPanel::from_index(prev_idx).unwrap_or(SidebarPanel::Context);
    }
}

/// Render the sidebar as a right-aligned panel.
///
/// The sidebar consists of a tab bar at the top and the active panel content
/// below it. The panel is rendered with a left border to separate it from
/// the conversation view.
pub fn render_sidebar(f: &mut Frame, area: Rect, state: &SidebarState, theme: &Theme) {
    if !state.visible || area.width < 20 {
        return;
    }

    let block = Block::default()
        .borders(Borders::LEFT)
        .border_style(Style::default().fg(theme.border))
        .style(Style::default().bg(theme.background));
    let inner = block.inner(area);
    f.render_widget(block, area);

    // Split into tab bar + content
    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([Constraint::Length(2), Constraint::Min(0)])
        .split(inner);

    // ── Render tab bar ──────────────────────────────────────────
    render_tab_bar(f, chunks[0], state);

    // ── Render active panel content ────────────────────────────
    match state.active_panel {
        SidebarPanel::Context => render_context_panel(f, chunks[1], state),
        SidebarPanel::Todo => render_todo_panel(f, chunks[1], state),
        SidebarPanel::Files => render_files_panel(f, chunks[1], state),
        SidebarPanel::Lsp => render_lsp_panel(f, chunks[1], state),
        SidebarPanel::Mcp => render_mcp_panel(f, chunks[1], state),
    }
}

/// Render the tab bar at the top of the sidebar.
fn render_tab_bar(f: &mut Frame, area: Rect, state: &SidebarState) {
    let titles: Vec<Line> = SidebarPanel::all()
        .iter()
        .map(|panel| {
            let style = if *panel == state.active_panel {
                Style::default()
                    .fg(Color::Black)
                    .bg(Color::Cyan)
                    .add_modifier(Modifier::BOLD)
            } else {
                Style::default().fg(Color::DarkGray)
            };
            Line::from(Span::styled(panel.label(), style))
        })
        .collect();

    let tabs = Tabs::new(titles)
        .block(Block::default())
        .highlight_style(Style::default().fg(Color::Cyan));

    f.render_widget(tabs, area);
}

/// Render the Context panel — token usage and cost.
fn render_context_panel(f: &mut Frame, area: Rect, state: &SidebarState) {
    let mut lines: Vec<Line> = Vec::new();

    lines.push(Line::from(Span::styled(
        " Context ",
        Style::default()
            .fg(Color::Cyan)
            .add_modifier(Modifier::BOLD),
    )));
    lines.push(Line::from(""));

    // Token bar
    let pct = state.context_used_pct;
    let bar_width = 30usize;
    let filled = ((pct / 100.0) * bar_width as f64) as usize;
    let empty = bar_width.saturating_sub(filled);

    let bar_color = if pct > 90.0 {
        Color::Red
    } else if pct > 70.0 {
        Color::Yellow
    } else {
        Color::Green
    };

    let bar = format!("[{}{}]", "█".repeat(filled), "░".repeat(empty),);
    lines.push(Line::from(Span::styled(
        bar,
        Style::default().fg(bar_color),
    )));

    lines.push(Line::from(vec![
        Span::styled(
            format_tokens_human(state.token_count),
            Style::default().fg(Color::White),
        ),
        Span::raw(" / "),
        Span::styled(
            format_tokens_human(state.token_limit),
            Style::default().fg(Color::DarkGray),
        ),
        Span::styled(
            format!(" ({pct:.1}%)"),
            Style::default().fg(if pct > 90.0 {
                Color::Red
            } else {
                Color::DarkGray
            }),
        ),
    ]));

    lines.push(Line::from(""));
    lines.push(Line::from(Span::styled(
        format!("Messages: {}", state.message_count),
        Style::default().fg(Color::White),
    )));

    if state.cost > 0.0 {
        lines.push(Line::from(Span::styled(
            format!("Session cost: ${:.4}", state.cost),
            Style::default().fg(if state.cost > 1.0 {
                Color::Yellow
            } else {
                Color::Green
            }),
        )));
    } else {
        lines.push(Line::from(Span::styled(
            "Session cost: $0.0000",
            Style::default().fg(Color::DarkGray),
        )));
    }

    // Estimate remaining
    if state.token_count > 0 {
        let remaining = state.token_limit.saturating_sub(state.token_count);
        lines.push(Line::from(Span::styled(
            format!("Remaining: {}", format_tokens_human(remaining)),
            Style::default().fg(Color::DarkGray),
        )));
    }

    let text = Text::from(lines);
    f.render_widget(Paragraph::new(text).wrap(Wrap { trim: true }), area);
}

/// Render the Todo panel — collapsible todo list.
fn render_todo_panel(f: &mut Frame, area: Rect, state: &SidebarState) {
    let mut lines: Vec<Line> = Vec::new();

    lines.push(Line::from(Span::styled(
        " Todo ",
        Style::default()
            .fg(Color::Cyan)
            .add_modifier(Modifier::BOLD),
    )));
    lines.push(Line::from(""));

    if state.todos.is_empty() {
        lines.push(Line::from(Span::styled(
            "No active todos.",
            Style::default().fg(Color::DarkGray),
        )));
    } else {
        let done_count = count_done(&state.todos);
        let total_count = count_total(&state.todos);
        lines.push(Line::from(Span::styled(
            format!("{done_count}/{total_count} completed"),
            Style::default().fg(if done_count == total_count && total_count > 0 {
                Color::Green
            } else {
                Color::Gray
            }),
        )));
        lines.push(Line::from(""));
        render_todo_items(&mut lines, &state.todos, area.width);
    }

    let text = Text::from(lines);
    f.render_widget(Paragraph::new(text).wrap(Wrap { trim: true }), area);
}

/// Recursively render todo items with indentation.
fn render_todo_items(lines: &mut Vec<Line>, items: &[TodoItem], _max_width: u16) {
    for item in items {
        let indent = "  ".repeat(item.level);
        let checkbox = if item.done { "[x]" } else { "[ ]" };
        let collapse_icon = if !item.children.is_empty() {
            if item.collapsed {
                "▸"
            } else {
                "▾"
            }
        } else {
            " "
        };

        let style = if item.done {
            Style::default().fg(Color::DarkGray)
        } else {
            Style::default().fg(Color::White)
        };

        lines.push(Line::from(vec![
            Span::raw(indent),
            Span::styled(collapse_icon, Style::default().fg(Color::DarkGray)),
            Span::raw(" "),
            Span::styled(checkbox, style),
            Span::raw(" "),
            Span::styled(item.text.clone(), style),
        ]));

        // Render children if not collapsed
        if !item.collapsed && !item.children.is_empty() {
            render_todo_items(lines, &item.children, _max_width);
        }
    }
}

fn count_done(items: &[TodoItem]) -> usize {
    let mut count = 0;
    for item in items {
        if item.done {
            count += 1;
        }
        count += count_done(&item.children);
    }
    count
}

fn count_total(items: &[TodoItem]) -> usize {
    let mut count = items.len();
    for item in items {
        count += count_total(&item.children);
    }
    count
}

/// Render the Files panel — modified files with +/- change counts.
fn render_files_panel(f: &mut Frame, area: Rect, state: &SidebarState) {
    let mut lines: Vec<Line> = Vec::new();

    lines.push(Line::from(Span::styled(
        " Files ",
        Style::default()
            .fg(Color::Cyan)
            .add_modifier(Modifier::BOLD),
    )));
    lines.push(Line::from(""));

    if state.changed_files.is_empty() {
        lines.push(Line::from(Span::styled(
            "No changed files.",
            Style::default().fg(Color::DarkGray),
        )));
    } else {
        let total_adds: usize = state.changed_files.iter().map(|f| f.additions).sum();
        let total_dels: usize = state.changed_files.iter().map(|f| f.deletions).sum();
        lines.push(Line::from(vec![
            Span::styled(format!("+{total_adds}"), Style::default().fg(Color::Green)),
            Span::raw(" "),
            Span::styled(format!("-{total_dels}"), Style::default().fg(Color::Red)),
            Span::styled(
                format!(" in {} files", state.changed_files.len()),
                Style::default().fg(Color::Gray),
            ),
        ]));
        lines.push(Line::from(""));

        for file in &state.changed_files {
            // Truncate path to fit
            let max_path = (area.width as usize).saturating_sub(18);
            let display_path = if file.path.len() > max_path {
                format!(
                    "...{}",
                    &file.path[file.path.len().saturating_sub(max_path - 3)..]
                )
            } else {
                file.path.clone()
            };

            let staged_marker = if file.staged { "S " } else { "  " };
            let staged_style = if file.staged {
                Style::default().fg(Color::Yellow)
            } else {
                Style::default().fg(Color::DarkGray)
            };

            lines.push(Line::from(vec![
                Span::styled(staged_marker, staged_style),
                Span::styled(display_path, Style::default().fg(Color::White)),
                Span::raw("  "),
                Span::styled(
                    format!("+{}", file.additions),
                    Style::default().fg(Color::Green),
                ),
                Span::raw(" "),
                Span::styled(
                    format!("-{}", file.deletions),
                    Style::default().fg(Color::Red),
                ),
            ]));
        }
    }

    let text = Text::from(lines);
    f.render_widget(Paragraph::new(text).wrap(Wrap { trim: true }), area);
}

/// Render the LSP panel — connection status dots.
fn render_lsp_panel(f: &mut Frame, area: Rect, state: &SidebarState) {
    let mut lines: Vec<Line> = Vec::new();

    lines.push(Line::from(Span::styled(
        " LSP ",
        Style::default()
            .fg(Color::Cyan)
            .add_modifier(Modifier::BOLD),
    )));
    lines.push(Line::from(""));

    if state.lsp_connections.is_empty() {
        lines.push(Line::from(Span::styled(
            "No LSP servers.",
            Style::default().fg(Color::DarkGray),
        )));
    } else {
        let connected = state
            .lsp_connections
            .iter()
            .filter(|c| c.status == LspStatus::Connected)
            .count();
        let total = state.lsp_connections.len();
        lines.push(Line::from(Span::styled(
            format!("{connected}/{total} connected"),
            Style::default().fg(if connected == total {
                Color::Green
            } else {
                Color::Yellow
            }),
        )));
        lines.push(Line::from(""));

        for conn in &state.lsp_connections {
            let icon = conn.status.icon();
            let color = conn.status.color();
            lines.push(Line::from(vec![
                Span::styled(format!(" {icon} "), Style::default().fg(color)),
                Span::styled(&conn.name, Style::default().fg(Color::White)),
            ]));
        }
    }

    let text = Text::from(lines);
    f.render_widget(Paragraph::new(text).wrap(Wrap { trim: true }), area);
}

/// Render the MCP panel — server names with connection dots.
fn render_mcp_panel(f: &mut Frame, area: Rect, state: &SidebarState) {
    let mut lines: Vec<Line> = Vec::new();

    lines.push(Line::from(Span::styled(
        " MCP ",
        Style::default()
            .fg(Color::Cyan)
            .add_modifier(Modifier::BOLD),
    )));
    lines.push(Line::from(""));

    if state.mcp_connections.is_empty() {
        lines.push(Line::from(Span::styled(
            "No MCP servers.",
            Style::default().fg(Color::DarkGray),
        )));
    } else {
        let connected = state.mcp_connections.iter().filter(|c| c.connected).count();
        let total = state.mcp_connections.len();
        let error_count = state.mcp_connections.iter().filter(|c| c.has_error).count();

        lines.push(Line::from(vec![Span::styled(
            format!("{connected}/{total} connected"),
            Style::default().fg(if connected == total {
                Color::Green
            } else if error_count > 0 {
                Color::Red
            } else {
                Color::Yellow
            }),
        )]));

        if error_count > 0 {
            lines.push(Line::from(Span::styled(
                format!("{error_count} with errors"),
                Style::default().fg(Color::Red),
            )));
        }

        lines.push(Line::from(""));

        for conn in &state.mcp_connections {
            let (icon, color) = if conn.has_error {
                ("✗", Color::Red)
            } else if conn.connected {
                ("●", Color::Green)
            } else {
                ("○", Color::DarkGray)
            };

            lines.push(Line::from(vec![
                Span::styled(format!(" {icon} "), Style::default().fg(color)),
                Span::styled(&conn.name, Style::default().fg(Color::White)),
                Span::styled(
                    format!(" ({} tools)", conn.tool_count),
                    Style::default().fg(Color::DarkGray),
                ),
            ]));
        }
    }

    let text = Text::from(lines);
    f.render_widget(Paragraph::new(text).wrap(Wrap { trim: true }), area);
}

/// Format a token count in human-readable form (e.g., "12.5K", "1.2M").
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
    fn test_sidebar_toggle() {
        let mut state = SidebarState::new();
        assert!(!state.visible);

        state.toggle();
        assert!(state.visible);

        state.toggle();
        assert!(!state.visible);
    }

    #[test]
    fn test_sidebar_panel_cycle() {
        let mut state = SidebarState::new();
        assert_eq!(state.active_panel, SidebarPanel::Context);

        state.next_panel();
        assert_eq!(state.active_panel, SidebarPanel::Todo);

        state.next_panel();
        assert_eq!(state.active_panel, SidebarPanel::Files);

        state.prev_panel();
        assert_eq!(state.active_panel, SidebarPanel::Todo);

        // Wrap around
        state.active_panel = SidebarPanel::Mcp;
        state.next_panel();
        assert_eq!(state.active_panel, SidebarPanel::Context);

        state.prev_panel();
        assert_eq!(state.active_panel, SidebarPanel::Mcp);
    }

    #[test]
    fn test_sidebar_show_hide() {
        let mut state = SidebarState::new();
        state.show();
        assert!(state.visible);
        state.hide();
        assert!(!state.visible);
    }

    #[test]
    fn test_format_tokens_human() {
        assert_eq!(format_tokens_human(500), "500");
        assert_eq!(format_tokens_human(1_500), "1.5K");
        assert_eq!(format_tokens_human(1_500_000), "1.5M");
        assert_eq!(format_tokens_human(0), "0");
    }

    #[test]
    fn test_todo_count() {
        let items = vec![
            TodoItem {
                text: "parent".into(),
                done: false,
                collapsed: false,
                level: 0,
                children: vec![TodoItem {
                    text: "child".into(),
                    done: true,
                    collapsed: false,
                    level: 1,
                    children: vec![],
                }],
            },
            TodoItem {
                text: "done".into(),
                done: true,
                collapsed: false,
                level: 0,
                children: vec![],
            },
        ];
        assert_eq!(count_total(&items), 3);
        assert_eq!(count_done(&items), 2);
    }
}
