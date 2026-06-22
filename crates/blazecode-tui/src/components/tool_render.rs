//! Tool-specific rendering — dispatches by tool name and renders output.
//!
//! Ported from: `packages/tui/src/routes/session/index.tsx` tool rendering.
//!
//! Each tool type has a dedicated renderer that formats the tool's input and
//! output appropriately (code blocks for bash, diff for edit, checkmarks for
//! todowrite, etc.).
//!
//! ## Supported tools
//!
//! | Tool | Renderer |
//! |------|----------|
//! | `bash` / `shell` | Command + expandable stdout/stderr |
//! | `write` / `edit` | File path + diff preview |
//! | `glob` / `grep` | Pattern + result list |
//! | `read` | File path + loaded lines |
//! | `webfetch` / `websearch` | URL/query + result summary |
//! | `task` | Subagent description + status |
//! | `todowrite` | Todo list with checkmarks |
//! | `question` | Question + answer |
//! | `skill` | Skill name + result |
//! | Generic | Fallback for unknown tools |

use ratatui::{
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::ListItem,
};

/// Maximum number of lines to show in a tool output preview.
const MAX_OUTPUT_LINES: usize = 15;

/// Maximum characters per line in tool output.
const MAX_LINE_WIDTH: usize = 120;

/// Maximum number of results to show in glob/grep.
const MAX_RESULTS: usize = 20;

/// Rendered output from a tool part.
///
/// Returns a list of ratatui `ListItem`s suitable for inclusion in the
/// conversation view.
pub type RenderedToolLines = Vec<ListItem<'static>>;

/// Render a tool part based on its name and state.
///
/// Dispatches to the appropriate renderer based on the tool name.
/// All renderers return `Vec<ListItem>` for uniform inclusion in the
/// conversation List widget.
///
/// # Source
/// Ported from `packages/tui/src/routes/session/index.tsx` lines 900–1100.
pub fn render_tool_part(
    tool_name: &str,
    input: &serde_json::Value,
    output: Option<&str>,
    state: ToolRenderState,
    width: u16,
) -> RenderedToolLines {
    let name_lower = tool_name.to_lowercase();

    match name_lower.as_str() {
        "bash" | "shell" | "exec" => render_bash(input, output, state, width),
        "write" | "edit" | "patch" => render_write(input, output, state, width),
        "glob" => render_glob(input, output, state, width),
        "grep" | "search" => render_grep(input, output, state, width),
        "read" => render_read(input, output, state, width),
        "webfetch" | "web_fetch" | "fetch" => render_webfetch(input, output, state, width),
        "websearch" | "web_search" => render_websearch(input, output, state, width),
        "task" | "subagent" => render_task(input, output, state, width),
        "todowrite" | "todo_write" | "todo" => render_todowrite(input, output, state, width),
        "question" | "ask" => render_question(input, output, state, width),
        "skill" => render_skill(input, output, state, width),
        "lsp" => render_lsp(input, output, state, width),
        "mcp" => render_mcp(input, output, state, width),
        "exit_plan_mode" | "exitplanmode" => render_exit_plan(input, output, state, width),
        _ => render_generic(tool_name, input, output, state, width),
    }
}

/// State flags controlling how tool output is rendered.
///
/// These map to the TUI toggle flags (show_tool_details, conceal, etc.).
#[derive(Debug, Clone, Copy, Default)]
pub struct ToolRenderState {
    /// Whether to expand full tool output (vs collapsed).
    pub show_details: bool,
    /// Whether conceal mode is active (hide file contents).
    pub conceal: bool,
    /// Whether to wrap long lines in diffs.
    pub diff_wrap: bool,
    /// Whether to show thinking/reasoning in tasks.
    pub show_thinking: bool,
    /// Whether generic tool output should be shown.
    pub generic_output: bool,
}

// ── Individual tool renderers ─────────────────────────────────────────────────

/// Render a bash/shell command — show command + expandable stdout/stderr.
fn render_bash(
    input: &serde_json::Value,
    output: Option<&str>,
    _state: ToolRenderState,
    width: u16,
) -> RenderedToolLines {
    let mut items: RenderedToolLines = Vec::new();

    let command = extract_str(input, "command")
        .or_else(|| extract_str(input, "cmd"))
        .unwrap_or("(no command)")
        .to_string();

    // Command header — styled as a code block
    items.push(make_dim_line("  $ ", width));
    items.push(make_code_line(
        &format!("  {command}"),
        width,
        Color::Yellow,
    ));

    if let Some(cwd) = extract_str(input, "cwd").or_else(|| extract_str(input, "workdir")) {
        items.push(make_dim_line(&format!("  in: {cwd}"), width));
    }

    // Output
    if let Some(out) = output {
        let out = out.to_string();
        if out.trim().is_empty() {
            items.push(make_dim_line("  (no output)", width));
        } else {
            let line_count = out.lines().count();
            let lines: Vec<String> = out
                .lines()
                .take(MAX_OUTPUT_LINES)
                .map(String::from)
                .collect();
            if lines.is_empty() {
                items.push(make_dim_line("  (empty)", width));
            } else {
                for line in &lines {
                    let truncated: String = line.chars().take(MAX_LINE_WIDTH).collect();
                    items.push(make_dim_line(&format!("    {truncated}"), width));
                }
                if line_count > MAX_OUTPUT_LINES {
                    items.push(make_dim_line(
                        &format!("    ... ({} more lines)", line_count - MAX_OUTPUT_LINES),
                        width,
                    ));
                }
            }
        }
    }

    items
}

/// Render a write/edit tool — file path + code block / diff.
fn render_write(
    input: &serde_json::Value,
    output: Option<&str>,
    _state: ToolRenderState,
    width: u16,
) -> RenderedToolLines {
    let mut items: RenderedToolLines = Vec::new();

    let file_path = extract_str(input, "file_path")
        .or_else(|| extract_str(input, "file"))
        .or_else(|| extract_str(input, "path"))
        .unwrap_or("(unknown file)")
        .to_string();

    items.push(ListItem::new(Line::from(vec![
        Span::styled("  File: ", Style::default().fg(Color::DarkGray)),
        Span::styled(
            file_path,
            Style::default()
                .fg(Color::Cyan)
                .add_modifier(Modifier::UNDERLINED),
        ),
    ])));

    // Show content or diff preview
    if let Some(content) = extract_str(input, "content").map(String::from) {
        let line_count = content.lines().count();
        let preview_lines: Vec<String> = content.lines().take(10).map(String::from).collect();
        for line in &preview_lines {
            let truncated: String = line
                .chars()
                .take(width.saturating_sub(6) as usize)
                .collect();
            items.push(make_code_line(
                &format!("  + {truncated}"),
                width,
                Color::Green,
            ));
        }
        if line_count > 10 {
            items.push(make_dim_line(
                &format!("  ... ({} more lines)", line_count - 10),
                width,
            ));
        }
    } else if let Some(diff) = extract_str(input, "diff").map(String::from) {
        // Show unified diff preview
        let preview_lines: Vec<String> = diff.lines().take(10).map(String::from).collect();
        for line in &preview_lines {
            let truncated: String = line
                .chars()
                .take(width.saturating_sub(6) as usize)
                .collect();
            let color = if line.starts_with('+') {
                Color::Green
            } else if line.starts_with('-') {
                Color::Red
            } else if line.starts_with("@@") {
                Color::Cyan
            } else {
                Color::Gray
            };
            items.push(make_colored_line(&format!("  {truncated}"), width, color));
        }
    }

    // Result summary
    if let Some(out) = output {
        let summary: String = out.chars().take(200).collect();
        if !summary.is_empty() {
            items.push(make_dim_line(&format!("  {summary}"), width));
        }
    }

    items
}

/// Render a glob tool — pattern + result list.
fn render_glob(
    input: &serde_json::Value,
    output: Option<&str>,
    _state: ToolRenderState,
    width: u16,
) -> RenderedToolLines {
    let mut items: RenderedToolLines = Vec::new();

    let pattern = extract_str(input, "pattern")
        .or_else(|| extract_str(input, "glob"))
        .unwrap_or("(no pattern)")
        .to_string();

    items.push(ListItem::new(Line::from(vec![
        Span::styled("  Pattern: ", Style::default().fg(Color::DarkGray)),
        Span::styled(pattern, Style::default().fg(Color::Yellow)),
    ])));

    if let Some(out) = output {
        let out = out.to_string();
        let files: Vec<String> = out.lines().take(MAX_RESULTS).map(String::from).collect();
        if files.is_empty() {
            items.push(make_dim_line("  No matches found.", width));
        } else {
            for file in &files {
                let truncated: String = file
                    .chars()
                    .take(width.saturating_sub(6) as usize)
                    .collect();
                items.push(make_colored_line(
                    &format!("  {truncated}"),
                    width,
                    Color::White,
                ));
            }
            if out.lines().count() > MAX_RESULTS {
                items.push(make_dim_line(
                    &format!("  ... and {} more", out.lines().count() - MAX_RESULTS),
                    width,
                ));
            }
        }
    }

    items
}

/// Render a grep/search tool — pattern + matched lines.
fn render_grep(
    input: &serde_json::Value,
    output: Option<&str>,
    _state: ToolRenderState,
    width: u16,
) -> RenderedToolLines {
    let mut items: RenderedToolLines = Vec::new();

    let pattern = extract_str(input, "pattern")
        .or_else(|| extract_str(input, "query"))
        .or_else(|| extract_str(input, "regex"))
        .map(|s| s.to_string())
        .unwrap_or_else(|| String::from("(no pattern)"));

    let path = extract_str(input, "path")
        .or_else(|| extract_str(input, "directory"))
        .map(|s| s.to_string())
        .unwrap_or_else(|| String::from("."));

    items.push(ListItem::new(Line::from(vec![
        Span::styled("  Grep: ", Style::default().fg(Color::DarkGray)),
        Span::styled(pattern, Style::default().fg(Color::Yellow)),
        Span::styled(format!(" in {path}"), Style::default().fg(Color::DarkGray)),
    ])));

    if let Some(out) = output {
        let owned_output = out.to_string();
        let matches: Vec<&str> = owned_output.lines().take(MAX_RESULTS).collect();
        if matches.is_empty() {
            items.push(make_dim_line("  No matches found.", width));
        } else {
            for m in &matches {
                let truncated: String = m.chars().take(width.saturating_sub(6) as usize).collect();
                items.push(make_colored_line(
                    &format!("  {truncated}"),
                    width,
                    Color::White,
                ));
            }
            if owned_output.lines().count() > MAX_RESULTS {
                items.push(make_dim_line(
                    &format!(
                        "  ... and {} more matches",
                        owned_output.lines().count() - MAX_RESULTS
                    ),
                    width,
                ));
            }
        }
    }

    items
}

/// Render a read tool — file path + loaded lines.
fn render_read(
    input: &serde_json::Value,
    output: Option<&str>,
    _state: ToolRenderState,
    width: u16,
) -> RenderedToolLines {
    let mut items: RenderedToolLines = Vec::new();

    let file_path = extract_str(input, "file_path")
        .or_else(|| extract_str(input, "file"))
        .or_else(|| extract_str(input, "path"))
        .unwrap_or("(unknown file)");

    let offset = extract_u64(input, "offset");
    let limit = extract_u64(input, "limit");

    let mut header = format!("  Read: {file_path}");
    if let (Some(off), Some(lim)) = (offset, limit) {
        header.push_str(&format!(" (lines {}-{})", off, off + lim));
    } else if let Some(off) = offset {
        header.push_str(&format!(" (from line {off})"));
    }

    items.push(ListItem::new(Line::from(Span::styled(
        header,
        Style::default().fg(Color::DarkGray),
    ))));

    if let Some(out) = output {
        let content_lines: Vec<&str> = out.lines().take(MAX_OUTPUT_LINES).collect();
        for line in &content_lines {
            let truncated: String = line
                .chars()
                .take(width.saturating_sub(6) as usize)
                .collect();
            items.push(make_code_line(
                &format!("  {truncated}"),
                width,
                Color::White,
            ));
        }
        if out.lines().count() > MAX_OUTPUT_LINES {
            items.push(make_dim_line(
                &format!("  ... ({} total lines)", out.lines().count()),
                width,
            ));
        }
    }

    items
}

/// Render a webfetch tool — URL + result summary.
fn render_webfetch(
    input: &serde_json::Value,
    output: Option<&str>,
    _state: ToolRenderState,
    width: u16,
) -> RenderedToolLines {
    let mut items: RenderedToolLines = Vec::new();

    let url = extract_str(input, "url")
        .or_else(|| extract_str(input, "link"))
        .unwrap_or("(no URL)");

    // Truncate long URLs
    let display_url = if url.len() > 80 {
        format!("{}...", &url[..77])
    } else {
        url.to_string()
    };

    items.push(ListItem::new(Line::from(vec![
        Span::styled("  Fetch: ", Style::default().fg(Color::DarkGray)),
        Span::styled(
            display_url,
            Style::default()
                .fg(Color::Cyan)
                .add_modifier(Modifier::UNDERLINED),
        ),
    ])));

    if let Some(out) = output {
        let summary: String = out.chars().take(500).collect();
        for line in summary.lines().take(8) {
            let truncated: String = line
                .chars()
                .take(width.saturating_sub(6) as usize)
                .collect();
            items.push(make_dim_line(&format!("  {truncated}"), width));
        }
        if out.lines().count() > 8 || out.len() > 500 {
            let remaining = out.len().saturating_sub(500);
            items.push(make_dim_line(
                &format!("  ... ({remaining} more chars)",),
                width,
            ));
        }
    }

    items
}

/// Render a websearch tool — query + result list.
fn render_websearch(
    input: &serde_json::Value,
    output: Option<&str>,
    _state: ToolRenderState,
    width: u16,
) -> RenderedToolLines {
    let mut items: RenderedToolLines = Vec::new();

    let query = extract_str(input, "query")
        .or_else(|| extract_str(input, "q"))
        .or_else(|| extract_str(input, "search"))
        .map(|s| s.to_string())
        .unwrap_or_else(|| String::from("(no query)"));

    items.push(ListItem::new(Line::from(vec![
        Span::styled("  Search: ", Style::default().fg(Color::DarkGray)),
        Span::styled(query, Style::default().fg(Color::Yellow)),
    ])));

    if let Some(out) = output {
        for line in out.lines().take(10) {
            let truncated: String = line
                .chars()
                .take(width.saturating_sub(8) as usize)
                .collect();
            items.push(ListItem::new(Line::from(vec![
                Span::styled(
                    format!("  {}. ", items.len()),
                    Style::default().fg(Color::DarkGray),
                ),
                Span::styled(truncated, Style::default().fg(Color::White)),
            ])));
        }
    }

    items
}

/// Render a task/subagent tool — description + status.
fn render_task(
    input: &serde_json::Value,
    _output: Option<&str>,
    _state: ToolRenderState,
    _width: u16,
) -> RenderedToolLines {
    let mut items: RenderedToolLines = Vec::new();

    let description = extract_str(input, "description")
        .or_else(|| extract_str(input, "prompt"))
        .or_else(|| extract_str(input, "task"))
        .map(|s| s.to_string())
        .unwrap_or_else(|| String::from("(no description)"));

    let subagent_type = extract_str(input, "subagent_type")
        .or_else(|| extract_str(input, "agent"))
        .map(|s| s.to_string())
        .unwrap_or_else(|| String::from("general"));

    items.push(ListItem::new(Line::from(vec![
        Span::styled(
            "  Subagent ",
            Style::default()
                .fg(Color::Magenta)
                .add_modifier(Modifier::BOLD),
        ),
        Span::styled(subagent_type, Style::default().fg(Color::Magenta)),
        Span::raw(": "),
        Span::styled(description, Style::default().fg(Color::White)),
    ])));

    items
}

/// Render a todowrite tool — todo list with checkmarks.
fn render_todowrite(
    input: &serde_json::Value,
    _output: Option<&str>,
    _state: ToolRenderState,
    width: u16,
) -> RenderedToolLines {
    let mut items: RenderedToolLines = Vec::new();

    items.push(ListItem::new(Line::from(Span::styled(
        "  Todo:",
        Style::default()
            .fg(Color::Yellow)
            .add_modifier(Modifier::BOLD),
    ))));

    // Parse todos from input
    if let Some(todos) = input.get("todos").and_then(|v| v.as_array()) {
        for todo in todos {
            let text = todo
                .get("content")
                .or_else(|| todo.get("text"))
                .or_else(|| todo.get("description"))
                .and_then(|v| v.as_str())
                .unwrap_or("(empty)");

            let status = todo
                .get("status")
                .or_else(|| todo.get("state"))
                .and_then(|v| v.as_str())
                .unwrap_or("pending");

            let (icon, color) = match status {
                "completed" | "done" => ("[x]", Color::Green),
                "in_progress" | "in-progress" | "active" => ("[~]", Color::Yellow),
                "cancelled" | "canceled" => ("[-]", Color::Red),
                _ => ("[ ]", Color::Gray),
            };

            let truncated: String = text
                .chars()
                .take(width.saturating_sub(8) as usize)
                .collect();
            items.push(ListItem::new(Line::from(vec![
                Span::raw("    "),
                Span::styled(icon, Style::default().fg(color)),
                Span::raw(" "),
                Span::styled(truncated, Style::default().fg(Color::White)),
            ])));
        }
    }

    items
}

/// Render a question tool — question + answer.
fn render_question(
    input: &serde_json::Value,
    output: Option<&str>,
    _state: ToolRenderState,
    _width: u16,
) -> RenderedToolLines {
    let mut items: RenderedToolLines = Vec::new();

    let question = extract_str(input, "question")
        .or_else(|| extract_str(input, "prompt"))
        .map(|s| s.to_string())
        .unwrap_or_else(|| String::from("(no question)"));

    let header_text = extract_str(input, "header").map(|s| s.to_string());

    if let Some(header) = header_text {
        items.push(ListItem::new(Line::from(Span::styled(
            format!("  {header}"),
            Style::default()
                .fg(Color::Magenta)
                .add_modifier(Modifier::BOLD),
        ))));
    }

    items.push(ListItem::new(Line::from(vec![
        Span::styled("  Q: ", Style::default().fg(Color::Yellow)),
        Span::styled(question, Style::default().fg(Color::White)),
    ])));

    if let Some(out) = output {
        let answer = String::from(out.trim());
        if !answer.is_empty() {
            items.push(ListItem::new(Line::from(vec![
                Span::styled("  A: ", Style::default().fg(Color::Green)),
                Span::styled(answer, Style::default().fg(Color::White)),
            ])));
        }
    }

    items
}

/// Render a skill tool — skill name + result.
fn render_skill(
    input: &serde_json::Value,
    output: Option<&str>,
    _state: ToolRenderState,
    width: u16,
) -> RenderedToolLines {
    let mut items: RenderedToolLines = Vec::new();

    let skill_name = extract_str(input, "skill")
        .or_else(|| extract_str(input, "name"))
        .map(|s| s.to_string())
        .unwrap_or_else(|| String::from("(unknown skill)"));

    items.push(ListItem::new(Line::from(vec![
        Span::styled("  Skill: ", Style::default().fg(Color::Cyan)),
        Span::styled(
            skill_name,
            Style::default()
                .fg(Color::Cyan)
                .add_modifier(Modifier::BOLD),
        ),
    ])));

    if let Some(out) = output {
        let preview: String = out.chars().take(300).collect();
        if !preview.is_empty() {
            for line in preview.lines().take(5) {
                let truncated: String = line
                    .chars()
                    .take(width.saturating_sub(6) as usize)
                    .collect();
                items.push(make_dim_line(&format!("  {truncated}"), width));
            }
        }
    }

    items
}

/// Render an LSP tool — diagnostics, references, etc.
fn render_lsp(
    input: &serde_json::Value,
    output: Option<&str>,
    _state: ToolRenderState,
    width: u16,
) -> RenderedToolLines {
    let mut items: RenderedToolLines = Vec::new();

    let operation = extract_str(input, "operation")
        .or_else(|| extract_str(input, "op"))
        .map(|s| s.to_string())
        .unwrap_or_else(|| String::from("diagnostics"));

    let file_path = extract_str(input, "file_path")
        .or_else(|| extract_str(input, "file"))
        .or_else(|| extract_str(input, "uri"))
        .map(|s| s.to_string())
        .unwrap_or_else(|| String::from("(unknown)"));

    items.push(ListItem::new(Line::from(vec![
        Span::styled("  LSP: ", Style::default().fg(Color::Cyan)),
        Span::styled(operation, Style::default().fg(Color::White)),
        Span::styled(
            format!(" on {file_path}"),
            Style::default().fg(Color::DarkGray),
        ),
    ])));

    if let Some(out) = output {
        let preview: String = out.chars().take(300).collect();
        if !preview.is_empty() {
            for line in preview.lines().take(8) {
                let truncated: String = line
                    .chars()
                    .take(width.saturating_sub(6) as usize)
                    .collect();
                items.push(make_dim_line(&format!("  {truncated}"), width));
            }
        }
    }

    items
}

/// Render an MCP tool — server name + tool invocation.
fn render_mcp(
    input: &serde_json::Value,
    output: Option<&str>,
    _state: ToolRenderState,
    width: u16,
) -> RenderedToolLines {
    let mut items: RenderedToolLines = Vec::new();

    let server = extract_str(input, "server")
        .or_else(|| extract_str(input, "server_name"))
        .map(|s| s.to_string())
        .unwrap_or_else(|| String::from("(unknown server)"));

    let mcp_tool = extract_str(input, "tool")
        .or_else(|| extract_str(input, "tool_name"))
        .map(|s| s.to_string())
        .unwrap_or_else(|| String::from("(unknown tool)"));

    items.push(ListItem::new(Line::from(vec![
        Span::styled("  MCP: ", Style::default().fg(Color::Magenta)),
        Span::styled(server, Style::default().fg(Color::White)),
        Span::raw(" → "),
        Span::styled(mcp_tool, Style::default().fg(Color::Cyan)),
    ])));

    if let Some(out) = output {
        let preview: String = out.chars().take(300).collect();
        if !preview.is_empty() {
            for line in preview.lines().take(5) {
                let truncated: String = line
                    .chars()
                    .take(width.saturating_sub(6) as usize)
                    .collect();
                items.push(make_dim_line(&format!("  {truncated}"), width));
            }
        }
    }

    items
}

/// Render an exit_plan_mode tool.
fn render_exit_plan(
    _input: &serde_json::Value,
    output: Option<&str>,
    _state: ToolRenderState,
    width: u16,
) -> RenderedToolLines {
    let mut items: RenderedToolLines = Vec::new();

    items.push(ListItem::new(Line::from(Span::styled(
        "  Exit Plan Mode",
        Style::default()
            .fg(Color::Cyan)
            .add_modifier(Modifier::BOLD),
    ))));

    if let Some(out) = output {
        let plan: String = out.chars().take(500).collect();
        if !plan.is_empty() {
            for line in plan.lines().take(10) {
                let truncated: String = line
                    .chars()
                    .take(width.saturating_sub(6) as usize)
                    .collect();
                items.push(make_colored_line(
                    &format!("  {truncated}"),
                    width,
                    Color::White,
                ));
            }
        }
    }

    items
}

/// Fallback renderer for unknown tools — shows tool name + input.
fn render_generic(
    tool_name: &str,
    input: &serde_json::Value,
    output: Option<&str>,
    _state: ToolRenderState,
    width: u16,
) -> RenderedToolLines {
    let mut items: RenderedToolLines = Vec::new();
    let tool_name = String::from(tool_name);

    items.push(ListItem::new(Line::from(vec![
        Span::styled("  Tool: ", Style::default().fg(Color::DarkGray)),
        Span::styled(
            tool_name,
            Style::default()
                .fg(Color::Yellow)
                .add_modifier(Modifier::BOLD),
        ),
    ])));

    // Show key input fields
    if let Some(obj) = input.as_object() {
        for (key, value) in obj.iter().take(5) {
            if key == "session_id" || key == "sessionID" || key == "id" {
                continue;
            }
            let val_str = match value {
                serde_json::Value::String(s) => {
                    let truncated: String = s.chars().take(60).collect();
                    truncated
                }
                serde_json::Value::Number(n) => n.to_string(),
                serde_json::Value::Bool(b) => b.to_string(),
                serde_json::Value::Array(a) => format!("[{} items]", a.len()),
                serde_json::Value::Object(_) => "{...}".to_string(),
                _ => String::new(),
            };
            if !val_str.is_empty() {
                items.push(ListItem::new(Line::from(vec![
                    Span::styled(format!("    {key}: "), Style::default().fg(Color::DarkGray)),
                    Span::styled(val_str, Style::default().fg(Color::Gray)),
                ])));
            }
        }
    }

    if let Some(out) = output {
        let preview: String = out.chars().take(200).collect();
        if !preview.is_empty() {
            items.push(make_dim_line(&format!("  Output: {preview}"), width));
        }
    }

    items
}

// ── Helpers for constructing ListItems ────────────────────────────────────────

/// Create a dim gray line.
fn make_dim_line(text: &str, _width: u16) -> ListItem<'static> {
    ListItem::new(Line::from(Span::styled(
        text.to_string(),
        Style::default().fg(Color::DarkGray),
    )))
}

/// Create a code-style line (dimmed background).
fn make_code_line(text: &str, _width: u16, color: Color) -> ListItem<'static> {
    ListItem::new(Line::from(Span::styled(
        text.to_string(),
        Style::default().fg(color).bg(Color::Rgb(30, 30, 40)),
    )))
}

/// Create a line with a specific foreground color.
fn make_colored_line(text: &str, _width: u16, color: Color) -> ListItem<'static> {
    ListItem::new(Line::from(Span::styled(
        text.to_string(),
        Style::default().fg(color),
    )))
}

// ── JSON input helpers ────────────────────────────────────────────────────────

/// Extract a string value from a JSON object.
fn extract_str<'a>(value: &'a serde_json::Value, key: &str) -> Option<&'a str> {
    value.get(key).and_then(|v| v.as_str())
}

/// Extract a u64 value from a JSON object.
fn extract_u64(value: &serde_json::Value, key: &str) -> Option<u64> {
    value.get(key).and_then(|v| v.as_u64())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_render_bash() {
        let input = serde_json::json!({
            "command": "cargo build --release",
            "cwd": "/home/user/project"
        });
        let state = ToolRenderState::default();
        let items = render_bash(
            &input,
            Some("   Compiling my-crate v0.1.0\n    Finished release [optimized]"),
            state,
            80,
        );
        assert!(!items.is_empty());
    }

    #[test]
    fn test_render_write() {
        let input = serde_json::json!({
            "file_path": "src/main.rs",
            "content": "fn main() {\n    println!(\"hello\");\n}"
        });
        let state = ToolRenderState::default();
        let items = render_write(&input, Some("Wrote 3 lines to src/main.rs"), state, 80);
        assert!(!items.is_empty());
    }

    #[test]
    fn test_render_glob() {
        let input = serde_json::json!({
            "pattern": "src/**/*.rs"
        });
        let state = ToolRenderState::default();
        let items = render_glob(
            &input,
            Some("src/main.rs\nsrc/lib.rs\nsrc/utils.rs"),
            state,
            80,
        );
        assert!(!items.is_empty());
    }

    #[test]
    fn test_render_todowrite() {
        let input = serde_json::json!({
            "todos": [
                {"content": "Add error handling", "status": "pending"},
                {"content": "Write tests", "status": "in_progress"},
                {"content": "Update docs", "status": "completed"}
            ]
        });
        let state = ToolRenderState::default();
        let items = render_todowrite(&input, None, state, 80);
        assert!(items.len() >= 4); // header + 3 todos
    }

    #[test]
    fn test_render_question() {
        let input = serde_json::json!({
            "question": "Which database should we use?",
            "header": "Architecture Decision"
        });
        let state = ToolRenderState::default();
        let items = render_question(
            &input,
            Some("SQLite for local, PostgreSQL for production"),
            state,
            80,
        );
        assert!(!items.is_empty());
    }

    #[test]
    fn test_render_skill() {
        let input = serde_json::json!({
            "skill": "svg-logo"
        });
        let state = ToolRenderState::default();
        let items = render_skill(&input, Some("Created logo.svg"), state, 80);
        assert!(!items.is_empty());
    }

    #[test]
    fn test_render_generic_fallback() {
        let input = serde_json::json!({
            "unknown_field": "some value",
            "count": 42
        });
        let state = ToolRenderState::default();
        let items = render_generic("unknown_tool", &input, Some("done"), state, 80);
        assert!(!items.is_empty());
    }

    #[test]
    fn test_extract_helpers() {
        let input = serde_json::json!({
            "name": "test",
            "count": 42,
            "enabled": true
        });
        assert_eq!(extract_str(&input, "name"), Some("test"));
        assert_eq!(extract_str(&input, "missing"), None);
        assert_eq!(extract_u64(&input, "count"), Some(42));
        assert_eq!(extract_u64(&input, "missing"), None);
    }
}
