//! Scrolling conversation view — displays session messages.
//!
//! ## Visual Design (Opencode Match)
//!
//! User messages with `┃` left border:
//! ```text
//!  ┃
//!  ┃  say hello
//!  ┃
//!
//!     + Thought: 671ms
//!
//!     → Read Cargo.toml
//!  ┌─────────────────────────────┐
//!  │ [package]                   │
//!  │ name = "blazecode"          │
//!  └─────────────────────────────┘
//!
//!     Some response text...
//!
//!     → WebSearch result
//!  % Result content...
//! ```
//!
//! Ported from: `packages/tui/src/routes/session/index.tsx`

use ratatui::{
    layout::Rect,
    style::{Color, Modifier, Style},
    text::{Line, Span, Text},
    widgets::{Block, Borders, List, ListItem, Paragraph, Wrap},
    Frame,
};
use blazecode_core::session::{Message, MessageInfo, Part, ToolState};
use std::collections::HashMap;

use crate::theme::Theme;

/// Maximum number of system messages to retain.
const MAX_SYSTEM_MESSAGES: usize = 100;

/// State for the conversation view.
#[derive(Debug, Default)]
pub struct ConversationState {
    /// Scroll offset in lines from the bottom (0 = bottom, increasing = scrolling up).
    pub scroll_offset: u16,
    /// Whether auto-scroll is enabled (follows new messages).
    pub auto_scroll: bool,
    /// Messages to display.
    pub messages: Vec<Message>,
    /// Parts keyed by message ID.
    pub parts: HashMap<String, Vec<Part>>,
    /// System/info messages rendered as gray lines.
    pub system_messages: Vec<String>,
}

impl ConversationState {
    pub fn new() -> Self {
        Self {
            scroll_offset: 0,
            auto_scroll: true,
            messages: Vec::new(),
            parts: HashMap::new(),
            system_messages: Vec::new(),
        }
    }

    /// Set messages and their parts.
    pub fn set_messages(&mut self, messages: Vec<Message>, parts_map: HashMap<String, Vec<Part>>) {
        self.messages = messages;
        self.parts = parts_map;
    }

    /// Add a system/status message.
    pub fn add_system_message(&mut self, text: String) {
        self.system_messages.push(text);
        if self.system_messages.len() > MAX_SYSTEM_MESSAGES {
            self.system_messages.remove(0);
        }
    }

    /// Scroll up by `amount` lines.
    pub fn scroll_up(&mut self, amount: u16) {
        self.scroll_offset = self.scroll_offset.saturating_add(amount);
        self.auto_scroll = false;
    }

    /// Scroll down by `amount` lines.
    pub fn scroll_down(&mut self, amount: u16) {
        self.scroll_offset = self.scroll_offset.saturating_sub(amount);
        if self.scroll_offset == 0 {
            self.auto_scroll = true;
        }
    }

    /// Scroll to the bottom (latest message).
    pub fn scroll_to_bottom(&mut self) {
        self.scroll_offset = 0;
        self.auto_scroll = true;
    }

    /// Scroll to the top (first message).
    pub fn scroll_to_top(&mut self) {
        self.scroll_offset = u16::MAX;
        self.auto_scroll = false;
    }

    #[allow(dead_code)]
    fn is_at_bottom(&self) -> bool {
        self.scroll_offset == 0
    }
}

/// Pick a color based on agent name — matches Opencode agent colors.
fn agent_color(agent: &str) -> Color {
    match agent {
        "build" => Color::Rgb(0x5c, 0x9c, 0xf5),   // blue
        "plan" => Color::Rgb(0xf5, 0xa7, 0x42),     // orange
        "general" => Color::Rgb(0x9d, 0x7c, 0xd8),  // purple
        "explore" => Color::Rgb(0x7f, 0xd8, 0x8f),  // green
        _ => Color::Rgb(0xfa, 0xb2, 0x83),          // default warm
    }
}

/// Render the conversation view.
pub fn render_conversation(f: &mut Frame, area: Rect, state: &ConversationState, theme: &Theme) {
    let messages = &state.messages;
    let system_msgs = &state.system_messages;

    if messages.is_empty() && system_msgs.is_empty() {
        let welcome = Paragraph::new(Text::from(vec![
            Line::from(Span::styled(
                "Welcome to BlazeCode!",
                Style::default()
                    .fg(theme.accent)
                    .add_modifier(Modifier::BOLD),
            )),
            Line::from(""),
            Line::from(Span::styled(
                "Type a message below to get started.",
                Style::default().fg(theme.text),
            )),
            Line::from(Span::styled(
                "Ctrl+C to exit, Ctrl+P for commands.",
                Style::default().fg(theme.text_muted),
            )),
        ]))
        .block(Block::default().borders(Borders::NONE))
        .wrap(Wrap { trim: true });

        f.render_widget(welcome, area);
        return;
    }

    let viewport_width = area.width.max(20);

    // Build all renderable lines as ListItems
    let mut items: Vec<ListItem> = Vec::new();

    // System messages first (oldest at top)
    for sys_msg in system_msgs {
        items.push(ListItem::new(Line::from(Span::styled(
            sys_msg.as_str(),
            Style::default().fg(theme.text_muted),
        ))));
    }

    // Build message items — matching Opencode's exact design
    for msg in messages {
        let parts = state
            .parts
            .get(msg.info.id())
            .cloned()
            .or_else(|| Some(msg.parts.clone()))
            .unwrap_or_default();
        let msg_items = build_message_items(msg, &parts, viewport_width, theme);
        items.extend(msg_items);
    }

    // Calculate visible items with scroll offset
    let visible_height = area.height.saturating_sub(1) as usize;
    let total_items = items.len();

    let max_scroll = total_items.saturating_sub(visible_height);
    let effective_offset = if state.auto_scroll {
        max_scroll
    } else {
        (state.scroll_offset as usize).min(max_scroll)
    };

    let visible_items: Vec<ListItem> = items
        .into_iter()
        .skip(effective_offset)
        .take(visible_height)
        .collect();

    let list = List::new(visible_items).block(Block::default().borders(Borders::NONE));

    f.render_widget(list, area);
}

/// Build renderable list items from a message and its parts, matching Opencode's design.
fn build_message_items(msg: &Message, parts: &[Part], width: u16, theme: &Theme) -> Vec<ListItem<'static>> {
    let mut items = Vec::new();

    match &msg.info {
        MessageInfo::User(user_info) => {
            let agent = user_info.agent.as_deref().unwrap_or("build");
            let color = agent_color(agent);
            let border_char = "\u{2503}"; // ┃

            // ── User message: Opencode style ────────────────────────
            // Format:
            //  ┃  <message text>

            // Text parts
            for part in parts {
                match part {
                    Part::Text(tp) => {
                        let raw = &tp.text;
                        if raw.trim().is_empty() {
                            continue;
                        }
                        let wrapped = wrap_text(raw, width.saturating_sub(8));
                        for line_text in wrapped {
                            items.push(ListItem::new(Line::from(vec![
                                Span::raw(format!("  {}  ", border_char)),
                                Span::styled(line_text, Style::default().fg(theme.text)),
                            ])).style(Style::default().bg(theme.background_panel)));
                        }
                    }
                    Part::File(fp) => {
                        let badge = fp.mime.split_once('/').map(|(_, ext)| ext).unwrap_or("file");
                        items.push(ListItem::new(Line::from(vec![
                            Span::raw("  "),
                            Span::styled(border_char, Style::default().fg(color)),
                            Span::raw("  "),
                            Span::styled(
                                format!(" {} ", badge),
                                Style::default()
                                    .fg(Color::Black)
                                    .bg(agent_color(agent)),
                            ),
                            Span::raw(" "),
                            Span::styled(
                                fp.filename.clone().unwrap_or_default(),
                                Style::default()
                                    .fg(theme.text_muted)
                                    .bg(theme.background_element),
                            ),
                        ])).style(Style::default().bg(theme.background_panel)));
                    }
                    _ => {}
                }
            }

            // Closing blank line
            items.push(ListItem::new(Line::from(vec![
                Span::raw("  "),
                Span::styled(border_char, Style::default().fg(color)),
            ])).style(Style::default().bg(theme.background_panel)));
        }

        MessageInfo::Assistant(assistant_info) => {
            // ── Assistant message parts (Opencode design) ───────────
            // Format:
            //     + Thought: Xms
            //     → ToolCall ...
            //  ┌─────────────────┐
            //  │ tool result ... │
            //  └─────────────────┘
            //     Response text...

            // Sort parts so Reasoning → Tool → Text → rest (Opencode ordering)
            let mut sorted: Vec<&Part> = parts.iter().collect();
            sorted.sort_by_key(|p| part_render_order(p));

            for part in sorted {
                match part {
                    Part::Text(tp) => {
                        let raw = &tp.text;
                        if raw.trim().is_empty() {
                            continue;
                        }
                        // Opencode: text with paddingLeft=3, no grouping info
                        let rendered = render_text_with_codeblocks(raw, width.saturating_sub(3), theme);
                        for rline in rendered {
                            items.push(ListItem::new(rline));
                        }
                    }

                    Part::Reasoning(rp) => {
                        // Opencode: "+ Thought: Xms" — compact single line
                        // Show first 60 chars of reasoning as a summary preview
                        let first_line = rp.text.lines().next()
                            .unwrap_or("")
                            .trim();

                        if first_line.is_empty() {
                            continue;
                        }

                        let preview: String = first_line.chars()
                            .take(60)
                            .collect();

                        let label = if first_line.len() > 60 {
                            format!("+ Thinking: {}...", preview)
                        } else {
                            format!("+ Thought: {preview}")
                        };

                        let is_complete = first_line.len() <= 60;

                        items.push(ListItem::new(Line::from(vec![
                            Span::raw("    "),
                            if is_complete {
                                Span::styled(label, Style::default().fg(theme.text_muted))
                            } else {
                                Span::styled(label, Style::default().fg(theme.warning))
                            },
                        ])));
                    }

                    Part::Tool(tool_part) => {
                        // Opencode tool rendering:
                        //   → Read ...
                        // or % WebSearch ...
                        // or code in a bordered panel
                        let tool_name = tool_part.tool.as_str();
                        let display_name = capitalize_first(tool_name);

                        match &tool_part.state {
                            ToolState::Pending { .. } => {
                                let icon = tool_icon(tool_name);
                                items.push(ListItem::new(Line::from(vec![
                                    Span::raw("    "),
                                    Span::styled(
                                        format!("{icon} {display_name} ..."),
                                        Style::default().fg(theme.text_muted),
                                    ),
                                ])));
                            }
                            ToolState::Running { .. } => {
                                let icon = tool_icon(tool_name);
                                items.push(ListItem::new(Line::from(vec![
                                    Span::raw("    "),
                                    Span::styled(
                                        format!("{icon} {display_name} ..."),
                                        Style::default().fg(theme.warning),
                                    ),
                                ])));
                            }
                            ToolState::Completed { ref output, ref title, .. } => {
                                let icon = tool_icon(tool_name);
                                let label = format!("{} {} {}", icon, display_name, title);
                                items.push(ListItem::new(Line::from(vec![
                                    Span::raw("    "),
                                    Span::styled(label, Style::default().fg(theme.text_muted)),
                                ])));

                                // Show output in a bordered panel (Opencode style)
                                if !output.is_empty() {
                                    let output_preview: Vec<&str> = output.lines().collect();
                                    let show_lines = output_preview.len().min(5);
                                    let max_output_width = width.saturating_sub(4) as usize;

                                    // Top border
                                    items.push(ListItem::new(Line::from(vec![
                                        Span::raw("  "),
                                        Span::styled(
                                            format!(
                                                "┌{}┐",
                                                "─".repeat(max_output_width.saturating_sub(2))
                                            ),
                                            Style::default().fg(theme.text_muted),
                                        ),
                                    ])).style(Style::default().bg(theme.background_panel)));

                                    for oline in &output_preview[..show_lines] {
                                        let display: String = oline
                                            .chars()
                                            .take(max_output_width.saturating_sub(4))
                                            .collect();
                                        items.push(ListItem::new(Line::from(vec![
                                            Span::raw("  "),
                                            Span::styled(
                                                format!("│ {} │", display),
                                                Style::default().fg(Color::Rgb(180, 190, 200)),
                                            ),
                                        ])).style(Style::default().bg(theme.background_panel)));
                                    }

                                    // Bottom border
                                    items.push(ListItem::new(Line::from(vec![
                                        Span::raw("  "),
                                        Span::styled(
                                            format!(
                                                "└{}┘",
                                                "─".repeat(max_output_width.saturating_sub(2))
                                            ),
                                            Style::default().fg(theme.text_muted),
                                        ),
                                    ])).style(Style::default().bg(theme.background_panel)));

                                    if output_preview.len() > 5 {
                                        items.push(ListItem::new(Line::from(vec![
                                            Span::raw("  "),
                                            Span::styled(
                                                format!("({} lines)", output_preview.len()),
                                                Style::default().fg(theme.text_muted),
                                            ),
                                        ])));
                                    }
                                }
                            }
                            ToolState::Error { ref error, .. } => {
                                items.push(ListItem::new(Line::from(vec![
                                    Span::raw("    "),
                                    Span::styled("✗ ", Style::default().fg(theme.error)),
                                    Span::styled(
                                        format!("{} — {}", display_name, error),
                                        Style::default().fg(theme.error),
                                    ),
                                ])));
                            }
                        }
                    }

                    Part::StepStart(_) => {
                        // Opencode: no step marks
                    }

                    Part::StepFinish(_sf) => {
                        // Opencode: no step finish marks inline
                        // Token counts are shown in the sidebar only
                    }

                    Part::Patch(pp) => {
                        let file_list: Vec<String> =
                            pp.files.iter().map(|f| f.path.clone()).collect();
                        items.push(ListItem::new(Line::from(vec![
                            Span::raw("    "),
                            Span::styled("%", Style::default().fg(theme.info)),
                            Span::raw(" "),
                            Span::styled(
                                format!("Patched: {}", file_list.join(", ")),
                                Style::default().fg(theme.text_muted),
                            ),
                        ])));
                    }

                    Part::Compaction(cp) => {
                        let label = if cp.auto { "auto-compacted" } else { "compacted" };
                        items.push(ListItem::new(Line::from(vec![
                            Span::raw("    "),
                            Span::styled(
                                format!("── Context {label} ──"),
                                Style::default().fg(theme.text_muted),
                            ),
                        ])));
                    }

                    Part::Subtask(_) | Part::Agent(_) | Part::Retry(_) => {
                        items.push(ListItem::new(Line::from(vec![
                            Span::raw("    "),
                            Span::styled(
                                format!("→ ..."),
                                Style::default().fg(theme.text_muted),
                            ),
                        ])));
                    }

                    _ => {}
                }
            }

            // ── Summary line (Opencode style) ────────────────────────
            // ▣  Build · 2.3s  or  ▣  build · deepseek-v4-flash openmodel
            let agent_name = &assistant_info.agent;
            let duration = assistant_info
                .time
                .completed
                .map(|c| c.saturating_sub(assistant_info.time.created));
            let duration_str = duration.map(|ms| {
                if ms >= 1000 {
                    format!("{:.1}s", ms as f64 / 1000.0)
                } else {
                    format!("{ms}ms")
                }
            });
            let display_agent = capitalize_first(agent_name);
            let summary = match duration_str {
                Some(d) => format!("▣  {display_agent} · {d}"),
                None => format!("▣  {display_agent}"),
            };
            items.push(ListItem::new(Line::from(vec![
                Span::raw("    "),
                Span::styled(summary, Style::default().fg(theme.text_muted)),
            ])));
        }
    }

    items
}

/// Capitalize the first letter of a word.
/// Sort key for parts: Reasoning → Tool → Text → rest
fn part_render_order(part: &Part) -> u8 {
    match part {
        Part::Reasoning(_) => 0,
        Part::Tool(_) => 1,
        Part::Text(_) => 2,
        _ => 99,
    }
}

fn capitalize_first(s: &str) -> String {
    let mut chars = s.chars();
    match chars.next() {
        None => String::new(),
        Some(first) => first.to_uppercase().to_string() + chars.as_str(),
    }
}

/// Icon for tool type — matches Opencode.
fn tool_icon(tool: &str) -> &'static str {
    match tool {
        "read" => "\u{2192}",         // →
        "write" => "\u{270F}",        // ✏
        "edit" => "\u{2190}",         // ←
        "bash" => "$",
        "grep" => "\u{1F50D}",        // 🔍
        "glob" => "*",
        "webfetch" => "%",
        "websearch" => "%",
        "apply_patch" => "%",
        "task" => "\u{25B6}",         // ▶
        "todowrite" => "\u{2611}",    // ☑
        "question" => "?",
        "skill" => "\u{2699}",        // ⚙
        _ => "\u{2699}",              // ⚙ default
    }
}

// ── Code block rendering ────────────────────────────────────────────────────

/// Render text content with code block highlighting.
fn render_text_with_codeblocks(raw: &str, wrap_width: u16, theme: &Theme) -> Vec<Line<'static>> {
    let mut lines: Vec<Line<'static>> = Vec::new();
    let mut in_code_block = false;
    let mut code_block_lines: Vec<String> = Vec::new();
    let mut code_block_lang: Option<String> = None;

    for line in raw.lines() {
        let trimmed = line.trim();
        if trimmed.starts_with("```") {
            if in_code_block {
                // End of code block — render with border
                let source = code_block_lines.join("\n");
                let code_border_style = Style::default().fg(theme.text_muted);

                // Top border
                lines.push(Line::from(vec![
                    Span::raw("   "),
                    Span::styled(
                        format!(
                            "┌{}┐",
                            "─".repeat(wrap_width.saturating_sub(5) as usize)
                        ),
                        code_border_style,
                    ),
                ]));

                if let Some(highlighted) =
                    crate::syntax::highlight_code(&source, code_block_lang.as_deref())
                {
                    for hl_line in split_spans_into_lines(highlighted) {
                        let mut bordered: Vec<Span> = vec![
                            Span::raw("   │ "),
                        ];
                        bordered.extend(hl_line);
                        bordered.push(Span::raw(" │"));
                        lines.push(Line::from(bordered));
                    }
                } else {
                    for cline in &code_block_lines {
                        let display: String = cline
                            .chars()
                            .take(wrap_width.saturating_sub(6) as usize)
                            .collect();
                        lines.push(Line::from(vec![
                            Span::raw("   │ "),
                            Span::styled(display, Color::Rgb(180, 190, 200)),
                        ]));
                    }
                }

                // Bottom border
                lines.push(Line::from(vec![
                    Span::raw("   "),
                    Span::styled(
                        format!(
                            "└{}┘",
                            "─".repeat(wrap_width.saturating_sub(5) as usize)
                        ),
                        code_border_style,
                    ),
                ]));

                code_block_lines.clear();
                code_block_lang = None;
            } else {
                let rest = trimmed.trim_start_matches("```");
                let lang = if rest.is_empty() {
                    None
                } else {
                    Some(rest.split_whitespace().next().unwrap_or("").to_string())
                };
                code_block_lang = lang;
            }
            in_code_block = !in_code_block;
            continue;
        }

        if in_code_block {
            code_block_lines.push(line.to_string());
        } else {
            // Normal text line — Opencode: paddingLeft=3
            let wrapped = wrap_text(line, wrap_width.saturating_sub(3));
            for wline in wrapped {
                lines.push(Line::from(vec![
                    Span::raw("   "),
                    Span::styled(wline, Style::default().fg(theme.text)),
                ]));
            }
        }
    }

    // Handle unclosed code block at EOF
    if in_code_block && !code_block_lines.is_empty() {
        let source = code_block_lines.join("\n");
        let code_border_style = Style::default().fg(theme.text_muted);

        lines.push(Line::from(vec![
            Span::raw("   "),
            Span::styled(
                format!(
                    "┌{}┐",
                    "─".repeat(wrap_width.saturating_sub(5) as usize)
                ),
                code_border_style,
            ),
        ]));

        if let Some(highlighted) =
            crate::syntax::highlight_code(&source, code_block_lang.as_deref())
        {
            for hl_line in split_spans_into_lines(highlighted) {
                let mut bordered: Vec<Span> = vec![Span::raw("   │ ")];
                bordered.extend(hl_line);
                bordered.push(Span::raw(" │"));
                lines.push(Line::from(bordered));
            }
        } else {
            for cline in &code_block_lines {
                let display: String = cline
                    .chars()
                    .take(wrap_width.saturating_sub(6) as usize)
                    .collect();
                lines.push(Line::from(vec![
                    Span::raw("   │ "),
                    Span::styled(display, Color::Rgb(180, 190, 200)),
                ]));
            }
        }

        lines.push(Line::from(vec![
            Span::raw("   "),
            Span::styled(
                format!(
                    "└{}┘",
                    "─".repeat(wrap_width.saturating_sub(5) as usize)
                ),
                code_border_style,
            ),
        ]));
    }

    lines
}

/// Split a flat vec of spans into lines separated by newlines.
fn split_spans_into_lines(spans: Vec<ratatui::text::Span<'static>>) -> Vec<Vec<ratatui::text::Span<'static>>> {
    let mut result_lines: Vec<Vec<ratatui::text::Span<'static>>> = Vec::new();
    let mut current_line: Vec<ratatui::text::Span<'static>> = Vec::new();

    for span in spans {
        if span.content == "\n" {
            result_lines.push(std::mem::take(&mut current_line));
        } else {
            current_line.push(span);
        }
    }

    if !current_line.is_empty() {
        result_lines.push(current_line);
    }

    result_lines
}

/// Word-wrap text to fit within `width` columns.
fn wrap_text(text: &str, width: u16) -> Vec<String> {
    let width = width.max(1) as usize;
    let mut result: Vec<String> = Vec::new();

    for paragraph in text.split('\n') {
        if paragraph.is_empty() {
            result.push(String::new());
            continue;
        }

        let mut current_line = String::with_capacity(width);
        for word in paragraph.split(' ') {
            let word_len = word.chars().count();

            if current_line.is_empty() {
                if word_len <= width {
                    current_line.push_str(word);
                } else {
                    for ch in word.chars() {
                        if current_line.chars().count() >= width {
                            result.push(std::mem::take(&mut current_line));
                        }
                        current_line.push(ch);
                    }
                }
            } else if current_line.chars().count() + 1 + word_len <= width {
                current_line.push(' ');
                current_line.push_str(word);
            } else {
                result.push(std::mem::take(&mut current_line));
                if word_len <= width {
                    current_line.push_str(word);
                } else {
                    for ch in word.chars() {
                        if current_line.chars().count() >= width {
                            result.push(std::mem::take(&mut current_line));
                        }
                        current_line.push(ch);
                    }
                }
            }
        }

        if !current_line.is_empty() {
            result.push(current_line);
        }
    }

    result
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_capitalize_first() {
        assert_eq!(capitalize_first("read"), "Read");
        assert_eq!(capitalize_first("websearch"), "Websearch");
        assert_eq!(capitalize_first(""), "");
        assert_eq!(capitalize_first("a"), "A");
    }

    #[test]
    fn test_wrap_text_short() {
        let result = wrap_text("hello world", 80);
        assert_eq!(result, vec!["hello world"]);
    }

    #[test]
    fn test_wrap_text_narrow() {
        let result = wrap_text("hello world", 6);
        assert_eq!(result, vec!["hello", "world"]);
    }

    #[test]
    fn test_wrap_text_newlines_preserved() {
        let result = wrap_text("line1\n\nline2", 80);
        assert_eq!(result, vec!["line1", "", "line2"]);
    }

    #[test]
    fn test_conversation_state_scroll() {
        let mut state = ConversationState::new();
        assert!(state.auto_scroll);
        assert_eq!(state.scroll_offset, 0);

        state.scroll_up(5);
        assert_eq!(state.scroll_offset, 5);
        assert!(!state.auto_scroll);

        state.scroll_down(3);
        assert_eq!(state.scroll_offset, 2);

        state.scroll_down(3);
        assert_eq!(state.scroll_offset, 0);
        assert!(state.auto_scroll);
    }

    #[test]
    fn test_agent_color() {
        assert_eq!(agent_color("build"), Color::Rgb(0x5c, 0x9c, 0xf5));
        assert_eq!(agent_color("plan"), Color::Rgb(0xf5, 0xa7, 0x42));
        assert_eq!(agent_color("unknown"), Color::Rgb(0xfa, 0xb2, 0x83));
    }
}
