//! Scrolling conversation view — displays session messages.
//!
//! Ported from: `packages/tui/src/routes/session/index.tsx` (lines 176–1350)
//!
//! The conversation view renders a scrollable list of messages (user and
//! assistant), with parts (text, tool, reasoning) rendered inline.

use ratatui::{
    layout::Rect,
    style::{Color, Modifier, Style},
    text::{Line, Span, Text},
    widgets::{Block, Borders, List, ListItem, Paragraph, Wrap},
    Frame,
};
use rustcode_core::session::{
    FilePart, Message, MessageInfo, Part, TextPart, ToolState,
};
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

    /// Check if we're at the bottom (auto-scroll position).
    fn is_at_bottom(&self) -> bool {
        self.scroll_offset == 0
    }
}

/// Render the conversation view into the given frame area.
pub fn render_conversation(f: &mut Frame, area: Rect, state: &ConversationState, theme: &Theme) {
    let messages = &state.messages;
    let system_msgs = &state.system_messages;

    if messages.is_empty() && system_msgs.is_empty() {
        let welcome = Paragraph::new(Text::from(vec![
            Line::from(Span::styled(
                "Welcome to rustcode!",
                Style::default().fg(theme.accent).add_modifier(Modifier::BOLD),
            )),
            Line::from(""),
            Line::from(Span::styled(
                "Type a message below to get started.",
                Style::default().fg(theme.foreground),
            )),
            Line::from(Span::styled(
                "Ctrl+C to exit, Ctrl+P for commands.",
                Style::default().fg(theme.dim),
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

    // System messages first (newest at bottom, so render oldest first)
    for sys_msg in system_msgs {
        items.push(ListItem::new(Line::from(Span::styled(
            sys_msg.as_str(),
            Style::default().fg(theme.dim),
        ))));
    }

    // Build message items
    for msg in messages {
        let parts = state.parts.get(msg.info.id()).cloned().unwrap_or_default();
        let msg_items = build_message_items(msg, &parts, viewport_width);
        items.extend(msg_items);
    }

    // Calculate how many items fit in the viewport and apply scroll offset
    let visible_height = area.height.saturating_sub(1) as usize; // reserve 1 for potential border
    let total_items = items.len();

    // Compute the effective scroll offset (clamped)
    let max_scroll = total_items.saturating_sub(visible_height);
    let effective_offset = if state.auto_scroll {
        // When auto-scrolling, show the bottom
        max_scroll
    } else {
        (state.scroll_offset as usize).min(max_scroll)
    };

    // Take only items that fit in the viewport
    let visible_items: Vec<ListItem> = items
        .into_iter()
        .skip(effective_offset)
        .take(visible_height)
        .collect();

    let list = List::new(visible_items).block(Block::default().borders(Borders::NONE));

    f.render_widget(list, area);
}

/// Build renderable list items from a message and its parts.
fn build_message_items(msg: &Message, parts: &[Part], width: u16) -> Vec<ListItem> {
    let mut items = Vec::new();

    match &msg.info {
        MessageInfo::User(user_info) => {
            // ── User message ──────────────────────────────────────────
            let agent_name = user_info.agent.as_deref().unwrap_or("You");

            // Header badge
            items.push(ListItem::new(Line::from(vec![
                Span::styled(
                    format!(" {} ", agent_name),
                    Style::default()
                        .fg(Color::Black)
                        .bg(Color::Cyan)
                        .add_modifier(Modifier::BOLD),
                ),
                if let Some(ref model) = user_info.model {
                    Span::styled(
                        format!(" via {}/{}", model.provider_id, model.id),
                        Style::default().fg(Color::DarkGray),
                    )
                } else {
                    Span::raw("")
                },
            ])));

            // Render parts: text parts and file attachments
            for part in parts {
                match part {
                    Part::Text(tp) => {
                        let wrapped = wrap_text(&tp.text, width.saturating_sub(4));
                        for line in wrapped {
                            items.push(ListItem::new(Line::from(Span::styled(
                                format!("  {line}"),
                                Style::default().fg(Color::White),
                            ))));
                        }
                    }
                    Part::File(fp) => {
                        items.push(ListItem::new(Line::from(vec![
                            Span::styled("  📎 ", Style::default().fg(Color::Cyan)),
                            Span::styled(
                                fp.filename.as_deref().unwrap_or("unnamed"),
                                Style::default()
                                    .fg(Color::Cyan)
                                    .add_modifier(Modifier::UNDERLINED),
                            ),
                            Span::styled(
                                format!(" ({})", fp.mime),
                                Style::default().fg(Color::DarkGray),
                            ),
                        ])));
                    }
                    _ => {}
                }
            }

            // If no text parts were rendered, show placeholder
            let has_text = parts
                .iter()
                .any(|p| matches!(p, Part::Text(tp) if !tp.text.trim().is_empty()));
            if !has_text {
                items.push(ListItem::new(Line::from(Span::styled(
                    "  (empty message)",
                    Style::default().fg(Color::DarkGray),
                ))));
            }
        }

        MessageInfo::Assistant(assistant_info) => {
            // ── Assistant header ─────────────────────────────────────
            let agent = if assistant_info.agent.is_empty() {
                "assistant"
            } else {
                &assistant_info.agent
            };
            let model = assistant_info
                .model_id
                .as_deref()
                .unwrap_or("unknown");

            let mut header = vec![
                Span::styled(
                    format!(" {} ", agent),
                    Style::default()
                        .fg(Color::Black)
                        .bg(Color::Magenta)
                        .add_modifier(Modifier::BOLD),
                ),
                Span::raw(" "),
                Span::styled(model, Style::default().fg(Color::Gray)),
            ];

            // Cost/tokens summary
            let cost = assistant_info.cost;
            let tokens = &assistant_info.tokens;
            let has_usage = tokens.input > 0 || tokens.output > 0;
            if has_usage {
                header.push(Span::styled(
                    format!(
                        " · {}↑ {}↓ ${:.4}",
                        tokens.input, tokens.output, cost
                    ),
                    Style::default().fg(Color::DarkGray),
                ));
            }

            items.push(ListItem::new(Line::from(header)));

            // ── Render parts ─────────────────────────────────────────
            for part in parts {
                match part {
                    Part::Text(tp) => {
                        let raw = tp.text.as_str();
                        if raw.trim().is_empty() {
                            continue;
                        }
                        let rendered = render_text_with_codeblocks(raw, width.saturating_sub(4));
                        items.extend(rendered);
                    }

                    Part::Reasoning(rp) => {
                        // Collapsible "Thought:" block — show first line as header
                        let first_line = rp
                            .text
                            .lines()
                            .next()
                            .unwrap_or("(thinking)")
                            .chars()
                            .take(width.saturating_sub(12) as usize)
                            .collect::<String>();

                        items.push(ListItem::new(Line::from(vec![
                            Span::styled(
                                " Thought: ",
                                Style::default()
                                    .fg(Color::Yellow)
                                    .add_modifier(Modifier::BOLD),
                            ),
                            Span::styled(&first_line, Style::default().fg(Color::Yellow)),
                        ])));

                        // Show remaining reasoning lines indented
                        let remaining_lines: Vec<&str> = rp.text.lines().skip(1).collect();
                        for line in remaining_lines.iter().take(5) {
                            let truncated: String = line
                                .chars()
                                .take(width.saturating_sub(8) as usize)
                                .collect();
                            items.push(ListItem::new(Line::from(vec![
                                Span::raw("  "),
                                Span::styled(&truncated, Style::default().fg(Color::Yellow)),
                            ])));
                        }
                        if remaining_lines.len() > 5 {
                            items.push(ListItem::new(Line::from(Span::styled(
                                format!("  ... ({} more lines)", remaining_lines.len() - 5),
                                Style::default().fg(Color::DarkGray),
                            ))));
                        }
                    }

                    Part::Tool(tool) => match &tool.state {
                        ToolState::Pending { .. } => {
                            items.push(ListItem::new(Line::from(vec![
                                Span::styled(" ⏳ ", Style::default().fg(Color::Yellow)),
                                Span::styled(
                                    format!("{} ...", tool.tool),
                                    Style::default().fg(Color::Gray),
                                ),
                            ])));
                        }
                        ToolState::Running { .. } => {
                            items.push(ListItem::new(Line::from(vec![
                                Span::styled(" ⟳ ", Style::default().fg(Color::Yellow)),
                                Span::styled(
                                    format!("{} running...", tool.tool),
                                    Style::default().fg(Color::Gray),
                                ),
                            ])));
                        }
                        ToolState::Completed { ref title, ref output, .. } => {
                            items.push(ListItem::new(Line::from(vec![
                                Span::styled(" ✓ ", Style::default().fg(Color::Green)),
                                Span::styled(
                                    format!("{} — {}", tool.tool, title),
                                    Style::default().fg(Color::White),
                                ),
                            ])));
                            // Show truncated output
                            if !output.is_empty() {
                                let preview: String = output
                                    .lines()
                                    .take(3)
                                    .flat_map(|l| {
                                        let truncated: String =
                                            l.chars().take(width.saturating_sub(6) as usize)
                                                .collect();
                                        vec![truncated]
                                    })
                                    .collect::<Vec<_>>()
                                    .join("\n");
                                if !preview.is_empty() {
                                    items.push(ListItem::new(Line::from(vec![
                                        Span::raw("   "),
                                        Span::styled(&preview, Style::default().fg(Color::DarkGray)),
                                    ])));
                                }
                                let line_count = output.lines().count();
                                if line_count > 3 {
                                    items.push(ListItem::new(Line::from(Span::styled(
                                        format!("   ... ({} lines)", line_count),
                                        Style::default().fg(Color::DarkGray),
                                    ))));
                                }
                            }
                        }
                        ToolState::Error { ref error, .. } => {
                            items.push(ListItem::new(Line::from(vec![
                                Span::styled(" ✗ ", Style::default().fg(Color::Red)),
                                Span::styled(
                                    format!("{} — {}", tool.tool, error),
                                    Style::default().fg(Color::Red),
                                ),
                            ])));
                        }
                    },

                    Part::StepStart(_) => {
                        items.push(ListItem::new(Line::from(
                            Span::styled(
                                " ── Step started ──",
                                Style::default().fg(Color::DarkGray),
                            ),
                        )));
                    }

                    Part::StepFinish(sf) => {
                        let reason = &sf.reason;
                        let input_tokens = sf.tokens.input;
                        let output_tokens = sf.tokens.output;
                        let cost = sf.cost;
                        items.push(ListItem::new(Line::from(vec![
                            Span::styled(
                                " ── Step finished: ",
                                Style::default().fg(Color::DarkGray),
                            ),
                            Span::styled(reason, Style::default().fg(Color::Gray)),
                            Span::styled(
                                format!(
                                    " ({}↑ {}↓ ${:.4})",
                                    input_tokens, output_tokens, cost
                                ),
                                Style::default().fg(Color::DarkGray),
                            ),
                        ])));
                    }

                    Part::File(fp) => {
                        items.push(ListItem::new(Line::from(vec![
                            Span::styled(" 📎 ", Style::default().fg(Color::Cyan)),
                            Span::styled(
                                fp.filename.as_deref().unwrap_or("attachment"),
                                Style::default()
                                    .fg(Color::Cyan)
                                    .add_modifier(Modifier::UNDERLINED),
                            ),
                            Span::styled(
                                format!(" ({})", fp.mime),
                                Style::default().fg(Color::DarkGray),
                            ),
                        ])));
                    }

                    Part::Patch(pp) => {
                        let file_list: Vec<String> = pp
                            .files
                            .iter()
                            .map(|f| f.path.clone())
                            .collect();
                        items.push(ListItem::new(Line::from(vec![
                            Span::styled(" Patch: ", Style::default().fg(Color::Green)),
                            Span::styled(
                                file_list.join(", "),
                                Style::default().fg(Color::Gray),
                            ),
                        ])));
                    }

                    Part::Compaction(cp) => {
                        let label = if cp.auto { "auto-compacted" } else { "compacted" };
                        items.push(ListItem::new(Line::from(Span::styled(
                            format!(" ── Context {label} ──"),
                            Style::default().fg(Color::DarkGray),
                        ))));
                    }

                    Part::Subtask(_) => {
                        items.push(ListItem::new(Line::from(Span::styled(
                            " ── Subtask dispatched ──",
                            Style::default().fg(Color::DarkGray),
                        ))));
                    }
                }
            }

            // ── Error display ─────────────────────────────────────────
            if let Some(ref error) = assistant_info.error {
                let err_msg = error
                    .get("message")
                    .and_then(|m| m.as_str())
                    .unwrap_or("unknown error");
                items.push(ListItem::new(Line::from(vec![
                    Span::styled(" Error: ", Style::default().fg(Color::Red).add_modifier(Modifier::BOLD)),
                    Span::styled(err_msg, Style::default().fg(Color::Red)),
                ])));
            }

            // ── Finish reason ─────────────────────────────────────────
            if let Some(ref finish) = assistant_info.finish {
                if !finish.is_empty() {
                    items.push(ListItem::new(Line::from(Span::styled(
                        format!(" ── {finish} ──"),
                        Style::default().fg(Color::DarkGray),
                    ))));
                }
            }

            // ── Separator after assistant message ─────────────────────
            items.push(ListItem::new(Line::from("")));
        }
    }

    items
}

/// Render text content with basic code block highlighting.
///
/// Detects ``` fences and applies a dimmed background to code blocks.
/// Returns list items suitable for a ratatui `List`.
fn render_text_with_codeblocks(raw: &str, wrap_width: u16) -> Vec<ListItem> {
    let mut items: Vec<ListItem> = Vec::new();
    let mut in_code_block = false;
    let code_style = Style::default().bg(Color::Rgb(30, 30, 40));

    for line in raw.lines() {
        let trimmed = line.trim();
        if trimmed.starts_with("```") {
            in_code_block = !in_code_block;
            // Render the fence itself
            items.push(ListItem::new(Line::from(Span::styled(
                format!("  {line}"),
                Style::default().fg(Color::DarkGray),
            ))));
            continue;
        }

        let wrapped = wrap_text(line, wrap_width);
        for wline in wrapped {
            if in_code_block {
                items.push(ListItem::new(Line::from(Span::styled(
                    format!("  {wline}"),
                    code_style.fg(Color::Rgb(180, 190, 200)),
                ))));
            } else {
                items.push(ListItem::new(Line::from(Span::styled(
                    format!("  {wline}"),
                    Style::default().fg(Color::White),
                ))));
            }
        }
    }

    items
}

/// Word-wrap text to fit within `width` columns.
///
/// Splits on word boundaries when possible, falls back to character-level
/// splitting for long words. Preserves existing newlines.
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
                // First word on line — if it fits, add it; if too long, chunk it
                if word_len <= width {
                    current_line.push_str(word);
                } else {
                    // Word is longer than width: character-split
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
                // Current line is full, push it and start new line with word
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
    fn test_wrap_text_long_word() {
        let result = wrap_text("supercalifragilisticexpialidocious", 10);
        assert!(result.len() > 1);
        // Reconstructed should match original
        let joined: String = result.join("");
        assert_eq!(joined, "supercalifragilisticexpialidocious");
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
}
