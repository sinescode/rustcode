//! Diff viewer — side-by-side or unified diff display with file tree sidebar.
//!
//! Ported from: `packages/tui/src/feature-plugins/system/diff-viewer.tsx`
//!
//! The diff viewer displays file changes with color-coded additions (green),
//! deletions (red), context lines (gray), and line numbers. It supports both
//! unified and split view modes and provides keyboard navigation between files,
//! hunks, and lines.
//!
//! ## Key bindings
//!
//! | Key | Action |
//! |-----|--------|
//! | `j` / `Down` | Next line |
//! | `k` / `Up` | Previous line |
//! | `n` | Next file |
//! | `p` | Previous file |
//! | `h` | Previous hunk |
//! | `l` | Next hunk |
//! | `Space` | Mark file as reviewed |
//! | `v` | Toggle unified/split view |
//! | `Esc` / `q` | Close diff viewer |

use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
use ratatui::{
    layout::{Constraint, Direction, Layout, Rect},
    style::{Color, Modifier, Style},
    text::{Line, Span, Text},
    widgets::{Block, Borders, Clear, List, ListItem, Paragraph, Wrap},
    Frame,
};

/// View mode for diff rendering.
///
/// # Source
/// Ported from `packages/tui/src/feature-plugins/system/diff-viewer.tsx`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DiffViewMode {
    /// Single-column unified diff (like `git diff` output).
    Unified,
    /// Side-by-side split diff (additions left, deletions right).
    Split,
}

/// A single line in a diff hunk.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum DiffLine {
    /// A context line (unchanged).
    Context(String),
    /// An added line.
    Addition(String),
    /// A deleted line.
    Deletion(String),
    /// A hunk header line (e.g., "@@ -1,5 +1,7 @@").
    Header(String),
}

impl DiffLine {
    /// Prefix character for this line type.
    pub fn prefix(&self) -> char {
        match self {
            DiffLine::Context(_) => ' ',
            DiffLine::Addition(_) => '+',
            DiffLine::Deletion(_) => '-',
            DiffLine::Header(_) => '@',
        }
    }

    /// Color for this line type.
    pub fn color(&self) -> Color {
        match self {
            DiffLine::Context(_) => Color::Gray,
            DiffLine::Addition(_) => Color::Green,
            DiffLine::Deletion(_) => Color::Red,
            DiffLine::Header(_) => Color::Cyan,
        }
    }

    /// Background color for this line type.
    pub fn bg_color(&self) -> Color {
        match self {
            DiffLine::Context(_) => Color::Black,
            DiffLine::Addition(_) => Color::Rgb(0, 40, 0),
            DiffLine::Deletion(_) => Color::Rgb(40, 0, 0),
            DiffLine::Header(_) => Color::Rgb(0, 30, 40),
        }
    }

    /// The text content of this line.
    pub fn text(&self) -> &str {
        match self {
            DiffLine::Context(s)
            | DiffLine::Addition(s)
            | DiffLine::Deletion(s)
            | DiffLine::Header(s) => s.as_str(),
        }
    }
}

/// A single hunk in a diff (a contiguous block of changes).
#[derive(Debug, Clone)]
pub struct DiffHunk {
    /// Starting line number in the old file.
    pub old_start: u64,
    /// Number of lines in the old file's hunk.
    pub old_count: u64,
    /// Starting line number in the new file.
    pub new_start: u64,
    /// Number of lines in the new file's hunk.
    pub new_count: u64,
    /// The lines in this hunk.
    pub lines: Vec<DiffLine>,
}

/// A file in the diff.
#[derive(Debug, Clone)]
pub struct DiffFile {
    /// File path relative to workspace root.
    pub path: String,
    /// Hunks of changes within this file.
    pub hunks: Vec<DiffHunk>,
    /// Number of added lines.
    pub additions: usize,
    /// Number of deleted lines.
    pub deletions: usize,
    /// Whether this file has been reviewed.
    pub reviewed: bool,
    /// Old file path (for renamed files).
    pub old_path: Option<String>,
    /// File mode change (e.g., "new file", "deleted", "renamed").
    pub status: Option<String>,
}

impl DiffFile {
    /// Total number of lines in all hunks.
    pub fn total_lines(&self) -> usize {
        self.hunks.iter().map(|h| h.lines.len()).sum()
    }
}

/// State for the diff viewer.
///
/// # Source
/// Ported from `packages/tui/src/feature-plugins/system/diff-viewer.tsx`.
#[derive(Debug, Default)]
pub struct DiffState {
    /// Whether the diff viewer is visible.
    pub visible: bool,
    /// Files being diffed.
    pub files: Vec<DiffFile>,
    /// Currently selected file index.
    pub selected_index: usize,
    /// Scroll offset within the current file (in lines).
    pub scroll_offset: u16,
    /// Current view mode.
    pub view_mode: DiffViewMode,
    /// Sidebar width as a fraction (0.0–1.0).
    pub sidebar_ratio: f64,
}

impl DiffState {
    pub fn new() -> Self {
        Self {
            visible: false,
            files: Vec::new(),
            selected_index: 0,
            scroll_offset: 0,
            view_mode: DiffViewMode::Unified,
            sidebar_ratio: 0.25,
        }
    }

    /// Toggle visibility.
    pub fn toggle(&mut self) {
        self.visible = !self.visible;
    }

    /// Show the diff viewer with the given files.
    pub fn show(&mut self, files: Vec<DiffFile>) {
        self.files = files;
        self.selected_index = 0;
        self.scroll_offset = 0;
        self.visible = true;
    }

    /// Hide the diff viewer.
    pub fn hide(&mut self) {
        self.visible = false;
    }

    /// Get the currently selected file.
    pub fn selected_file(&self) -> Option<&DiffFile> {
        self.files.get(self.selected_index)
    }

    /// Select the next file.
    pub fn next_file(&mut self) {
        if !self.files.is_empty() {
            self.selected_index = (self.selected_index + 1) % self.files.len();
            self.scroll_offset = 0;
        }
    }

    /// Select the previous file.
    pub fn prev_file(&mut self) {
        if !self.files.is_empty() {
            self.selected_index = if self.selected_index == 0 {
                self.files.len().saturating_sub(1)
            } else {
                self.selected_index - 1
            };
            self.scroll_offset = 0;
        }
    }

    /// Scroll up by `amount` lines.
    pub fn scroll_up(&mut self, amount: u16) {
        self.scroll_offset = self.scroll_offset.saturating_add(amount);
    }

    /// Scroll down by `amount` lines.
    pub fn scroll_down(&mut self, amount: u16) {
        self.scroll_offset = self.scroll_offset.saturating_sub(amount);
    }

    /// Jump to the next hunk in the current file.
    pub fn next_hunk(&mut self) {
        // This advances scroll position to the next hunk header
        // For now, increment scroll to approximate next hunk position
        self.scroll_up(10);
    }

    /// Jump to the previous hunk in the current file.
    pub fn prev_hunk(&mut self) {
        self.scroll_down(10);
    }

    /// Toggle the reviewed status of the current file.
    pub fn toggle_reviewed(&mut self) {
        if let Some(file) = self.files.get_mut(self.selected_index) {
            file.reviewed = !file.reviewed;
        }
    }

    /// Toggle the view mode between Unified and Split.
    pub fn toggle_view_mode(&mut self) {
        self.view_mode = match self.view_mode {
            DiffViewMode::Unified => DiffViewMode::Split,
            DiffViewMode::Split => DiffViewMode::Unified,
        };
    }

    /// Handle a key event. Returns `true` if the key was consumed.
    pub fn handle_key(&mut self, key: KeyEvent) -> bool {
        if !self.visible {
            return false;
        }

        match key {
            // Close
            KeyEvent {
                code: KeyCode::Esc, ..
            }
            | KeyEvent {
                code: KeyCode::Char('q'),
                modifiers: KeyModifiers::NONE,
                ..
            } => {
                self.hide();
                return true;
            }

            // Navigation
            KeyEvent {
                code: KeyCode::Down, ..
            }
            | KeyEvent {
                code: KeyCode::Char('j'),
                modifiers: KeyModifiers::NONE,
                ..
            } => {
                self.scroll_down(1);
                return true;
            }

            KeyEvent {
                code: KeyCode::Up, ..
            }
            | KeyEvent {
                code: KeyCode::Char('k'),
                modifiers: KeyModifiers::NONE,
                ..
            } => {
                self.scroll_up(1);
                return true;
            }

            // Next/prev file
            KeyEvent {
                code: KeyCode::Char('n'),
                modifiers: KeyModifiers::NONE,
                ..
            } => {
                self.next_file();
                return true;
            }

            KeyEvent {
                code: KeyCode::Char('p'),
                modifiers: KeyModifiers::NONE,
                ..
            } => {
                self.prev_file();
                return true;
            }

            // Next/prev hunk
            KeyEvent {
                code: KeyCode::Char('l'),
                modifiers: KeyModifiers::NONE,
                ..
            } => {
                self.next_hunk();
                return true;
            }

            KeyEvent {
                code: KeyCode::Char('h'),
                modifiers: KeyModifiers::NONE,
                ..
            } => {
                self.prev_hunk();
                return true;
            }

            // Mark reviewed
            KeyEvent {
                code: KeyCode::Char(' '),
                modifiers: KeyModifiers::NONE,
                ..
            } => {
                self.toggle_reviewed();
                return true;
            }

            // Toggle view mode
            KeyEvent {
                code: KeyCode::Char('v'),
                modifiers: KeyModifiers::NONE,
                ..
            } => {
                self.toggle_view_mode();
                return true;
            }

            _ => true, // Consume all other keys when visible
        }
    }
}

/// Render the diff viewer as a full-screen overlay.
///
/// Layout: sidebar (file tree) | diff content.
pub fn render_diff(f: &mut Frame, area: Rect, state: &DiffState) {
    if !state.visible {
        return;
    }

    // Dim the background
    f.render_widget(Clear, area);

    let block = Block::default()
        .borders(Borders::ALL)
        .title(format!(
            " Diff Viewer — {} mode (j/k:nav n/p:file Space:review q:close) ",
            match state.view_mode {
                DiffViewMode::Unified => "Unified",
                DiffViewMode::Split => "Split",
            }
        ))
        .border_style(Style::default().fg(Color::Cyan))
        .style(Style::default().bg(Color::Black));

    let inner = block.inner(area);
    f.render_widget(block, area);

    if state.files.is_empty() {
        let msg = Paragraph::new("No changes to display.")
            .style(Style::default().fg(Color::Gray));
        f.render_widget(msg, inner);
        return;
    }

    // Split into sidebar + content
    let sidebar_width = (inner.width as f64 * state.sidebar_ratio) as u16;
    let sidebar_width = sidebar_width.clamp(15, 40);

    let columns = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([Constraint::Length(sidebar_width), Constraint::Min(0)])
        .split(inner);

    // Render file tree sidebar
    render_diff_sidebar(f, columns[0], state);

    // Render diff content
    render_diff_content(f, columns[1], state);
}

/// Render the file tree sidebar.
fn render_diff_sidebar(f: &mut Frame, area: Rect, state: &DiffState) {
    let block = Block::default()
        .borders(Borders::RIGHT)
        .border_style(Style::default().fg(Color::DarkGray));

    let inner = block.inner(area);
    f.render_widget(block, area);

    let items: Vec<ListItem> = state
        .files
        .iter()
        .enumerate()
        .map(|(i, file)| {
            let is_selected = i == state.selected_index;
            let style = if is_selected {
                Style::default()
                    .fg(Color::Black)
                    .bg(Color::Cyan)
            } else if file.reviewed {
                Style::default().fg(Color::DarkGray)
            } else {
                Style::default().fg(Color::White)
            };

            // Truncate path to fit
            let max_path = (area.width as usize).saturating_sub(14);
            let display_path = if file.path.len() > max_path {
                format!("...{}", &file.path[file.path.len().saturating_sub(max_path - 3)..])
            } else {
                file.path.clone()
            };

            let reviewed_marker = if file.reviewed { " ✓" } else { "" };

            ListItem::new(Line::from(vec![
                Span::styled(
                    format!(
                        " +{:<4} -{:<4}",
                        file.additions, file.deletions,
                    ),
                    if is_selected {
                        Style::default()
                            .fg(Color::Green)
                            .bg(Color::Cyan)
                    } else {
                        Style::default().fg(Color::Green)
                    },
                ),
                Span::styled(&display_path, style),
                Span::styled(reviewed_marker, Style::default().fg(Color::DarkGray)),
            ]))
        })
        .collect();

    let list = List::new(items);
    f.render_widget(list, inner);
}

/// Render diff content for the selected file.
fn render_diff_content(f: &mut Frame, area: Rect, state: &DiffState) {
    let file = match state.selected_file() {
        Some(f) => f,
        None => return,
    };

    // File header with status
    let mut header_lines: Vec<Line> = Vec::new();

    let status_text = file.status.as_deref().unwrap_or("modified");
    let status_color = match status_text {
        "new file" | "added" => Color::Green,
        "deleted" => Color::Red,
        "renamed" => Color::Yellow,
        _ => Color::Cyan,
    };

    header_lines.push(Line::from(vec![
        Span::styled(format!(" {status_text} "), Style::default().fg(Color::Black).bg(status_color).add_modifier(Modifier::BOLD)),
        Span::raw(" "),
        Span::styled(&file.path, Style::default().fg(Color::White).add_modifier(Modifier::BOLD)),
    ]));

    if let Some(ref old_path) = file.old_path {
        header_lines.push(Line::from(vec![
            Span::raw("  → "),
            Span::styled(old_path, Style::default().fg(Color::DarkGray)),
        ]));
    }

    header_lines.push(Line::from(Span::styled(
        format!(
            "  +{} -{} in {} hunk{}",
            file.additions,
            file.deletions,
            file.hunks.len(),
            if file.hunks.len() == 1 { "" } else { "s" },
        ),
        Style::default().fg(Color::DarkGray),
    )));
    header_lines.push(Line::from(""));

    let header_height = header_lines.len() as u16;

    // Split area into header + diff body
    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([Constraint::Length(header_height), Constraint::Min(0)])
        .split(area);

    // Render header
    let header_text = Text::from(header_lines);
    f.render_widget(Paragraph::new(header_text), chunks[0]);

    // Render diff body
    match state.view_mode {
        DiffViewMode::Unified => render_unified_diff(f, chunks[1], file, state.scroll_offset),
        DiffViewMode::Split => render_split_diff(f, chunks[1], file, state.scroll_offset),
    }
}

/// Render unified diff (like `git diff`).
fn render_unified_diff(f: &mut Frame, area: Rect, file: &DiffFile, scroll_offset: u16) {
    let mut lines: Vec<Line> = Vec::new();

    for hunk in &file.hunks {
        // Hunk header
        let header = format!(
            "@@ -{},{} +{},{} @@",
            hunk.old_start, hunk.old_count, hunk.new_start, hunk.new_count
        );
        lines.push(Line::from(Span::styled(header, Style::default().fg(Color::Cyan))));

        // Hunk lines
        for line in &hunk.lines {
            let prefix = line.prefix();
            let color = line.color();
            let bg = line.bg_color();

            // Line number tracking (old/new)
            let old_line = if matches!(line, DiffLine::Addition(_)) {
                String::new()
            } else {
                format!("{:>4} ", hunk.old_start) // approximate
            };
            let new_line = if matches!(line, DiffLine::Deletion(_)) {
                String::new()
            } else {
                format!("{:>4} ", hunk.new_start) // approximate
            };

            // Truncate content to fit area
            let max_content = (area.width as usize).saturating_sub(12);
            let content = line.text();
            let display_content: String = content
                .chars()
                .take(max_content)
                .collect();

            lines.push(Line::from(vec![
                Span::styled(old_line, Style::default().fg(Color::DarkGray)),
                Span::styled(new_line, Style::default().fg(Color::DarkGray)),
                Span::styled(
                    format!("{prefix} {display_content}"),
                    Style::default().fg(color).bg(bg),
                ),
            ]));
        }
    }

    // Apply scroll offset
    let visible_height = area.height as usize;
    let max_scroll = lines.len().saturating_sub(visible_height);
    let effective_offset = (scroll_offset as usize).min(max_scroll);

    let visible_lines: Vec<Line> = lines
        .into_iter()
        .skip(effective_offset)
        .take(visible_height)
        .collect();

    let text = Text::from(visible_lines);
    f.render_widget(
        Paragraph::new(text).wrap(Wrap { trim: false }),
        area,
    );
}

/// Render split diff (additions right, deletions left).
fn render_split_diff(f: &mut Frame, area: Rect, file: &DiffFile, scroll_offset: u16) {
    let half_width = area.width / 2;

    let columns = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([Constraint::Ratio(1, 2), Constraint::Ratio(1, 2)])
        .split(area);

    // Left column: deletions + context
    let mut left_lines: Vec<Line> = Vec::new();
    // Right column: additions + context
    let mut right_lines: Vec<Line> = Vec::new();

    for hunk in &file.hunks {
        // Hunk header spans both sides
        let header = format!(
            "@@ -{},{} +{},{} @@",
            hunk.old_start, hunk.old_count, hunk.new_start, hunk.new_count
        );
        left_lines.push(Line::from(Span::styled(&header, Style::default().fg(Color::Cyan))));
        right_lines.push(Line::from(Span::styled(&header, Style::default().fg(Color::Cyan))));

        for line in &hunk.lines {
            let content: String = line
                .text()
                .chars()
                .take(half_width.saturating_sub(4) as usize)
                .collect();

            match line {
                DiffLine::Context(s) => {
                    left_lines.push(Line::from(Span::styled(
                        format!("  {s}"),
                        Style::default().fg(Color::Gray),
                    )));
                    right_lines.push(Line::from(Span::styled(
                        format!("  {s}"),
                        Style::default().fg(Color::Gray),
                    )));
                }
                DiffLine::Deletion(s) => {
                    left_lines.push(Line::from(Span::styled(
                        format!("- {s}"),
                        Style::default().fg(Color::Red).bg(Color::Rgb(40, 0, 0)),
                    )));
                    // Empty placeholder on the right side
                    right_lines.push(Line::from(""));
                }
                DiffLine::Addition(s) => {
                    // Empty placeholder on the left side
                    left_lines.push(Line::from(""));
                    right_lines.push(Line::from(Span::styled(
                        format!("+ {s}"),
                        Style::default().fg(Color::Green).bg(Color::Rgb(0, 40, 0)),
                    )));
                }
                DiffLine::Header(_) => {
                    // Already handled above for hunk header
                    let truncated: String = content
                        .chars()
                        .take(half_width.saturating_sub(4) as usize)
                        .collect();
                    left_lines.push(Line::from(Span::styled(
                        format!("  {truncated}"),
                        Style::default().fg(Color::Cyan),
                    )));
                    right_lines.push(Line::from(Span::styled(
                        format!("  {truncated}"),
                        Style::default().fg(Color::Cyan),
                    )));
                }
            }
        }
    }

    // Apply scroll offset to both sides
    let visible_height = area.height as usize;
    let max_scroll_left = left_lines.len().saturating_sub(visible_height);
    let max_scroll_right = right_lines.len().saturating_sub(visible_height);
    let effective_offset = (scroll_offset as usize)
        .min(max_scroll_left)
        .min(max_scroll_right);

    // Left pane
    let left_block = Block::default()
        .title(" Old ")
        .borders(Borders::ALL)
        .border_style(Style::default().fg(Color::Red));
    let left_inner = left_block.inner(columns[0]);
    f.render_widget(left_block, columns[0]);

    let left_visible: Vec<Line> = left_lines
        .into_iter()
        .skip(effective_offset)
        .take(visible_height.saturating_sub(2))
        .collect();
    f.render_widget(Paragraph::new(Text::from(left_visible)), left_inner);

    // Right pane
    let right_block = Block::default()
        .title(" New ")
        .borders(Borders::ALL)
        .border_style(Style::default().fg(Color::Green));
    let right_inner = right_block.inner(columns[1]);
    f.render_widget(right_block, columns[1]);

    let right_visible: Vec<Line> = right_lines
        .into_iter()
        .skip(effective_offset)
        .take(visible_height.saturating_sub(2))
        .collect();
    f.render_widget(Paragraph::new(Text::from(right_visible)), right_inner);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_diff_line_types() {
        let ctx = DiffLine::Context("unchanged".into());
        let add = DiffLine::Addition("new line".into());
        let del = DiffLine::Deletion("old line".into());
        let hdr = DiffLine::Header("@@ -1,3 +1,4 @@".into());

        assert_eq!(ctx.prefix(), ' ');
        assert_eq!(add.prefix(), '+');
        assert_eq!(del.prefix(), '-');
        assert_eq!(hdr.prefix(), '@');

        assert_eq!(ctx.color(), Color::Gray);
        assert_eq!(add.color(), Color::Green);
        assert_eq!(del.color(), Color::Red);
        assert_eq!(hdr.color(), Color::Cyan);

        assert_eq!(ctx.text(), "unchanged");
        assert_eq!(add.text(), "new line");
        assert_eq!(del.text(), "old line");
    }

    #[test]
    fn test_diff_state_navigation() {
        let mut state = DiffState::new();
        assert!(!state.visible);

        let files = vec![
            DiffFile {
                path: "src/a.rs".into(),
                hunks: vec![],
                additions: 5,
                deletions: 2,
                reviewed: false,
                old_path: None,
                status: None,
            },
            DiffFile {
                path: "src/b.rs".into(),
                hunks: vec![],
                additions: 0,
                deletions: 10,
                reviewed: false,
                old_path: None,
                status: Some("deleted".into()),
            },
        ];

        state.show(files);
        assert!(state.visible);
        assert_eq!(state.selected_index, 0);

        state.next_file();
        assert_eq!(state.selected_index, 1);

        state.next_file();
        assert_eq!(state.selected_index, 0); // wrap

        state.prev_file();
        assert_eq!(state.selected_index, 1); // wrap

        state.toggle_reviewed();
        assert!(state.files[1].reviewed);

        state.hide();
        assert!(!state.visible);
    }

    #[test]
    fn test_diff_scroll() {
        let mut state = DiffState::new();
        assert_eq!(state.scroll_offset, 0);

        state.scroll_up(5);
        assert_eq!(state.scroll_offset, 5);

        state.scroll_down(2);
        assert_eq!(state.scroll_offset, 3);

        state.scroll_down(10);
        assert_eq!(state.scroll_offset, 0); // clamped
    }

    #[test]
    fn test_diff_view_mode_toggle() {
        let mut state = DiffState::new();
        assert_eq!(state.view_mode, DiffViewMode::Unified);

        state.toggle_view_mode();
        assert_eq!(state.view_mode, DiffViewMode::Split);

        state.toggle_view_mode();
        assert_eq!(state.view_mode, DiffViewMode::Unified);
    }

    #[test]
    fn test_diff_file_total_lines() {
        let file = DiffFile {
            path: "test.rs".into(),
            hunks: vec![
                DiffHunk {
                    old_start: 1,
                    old_count: 3,
                    new_start: 1,
                    new_count: 4,
                    lines: vec![
                        DiffLine::Context("line1".into()),
                        DiffLine::Addition("line2".into()),
                        DiffLine::Context("line3".into()),
                    ],
                },
            ],
            additions: 1,
            deletions: 0,
            reviewed: false,
            old_path: None,
            status: None,
        };
        assert_eq!(file.total_lines(), 3);
    }
}
