//! Home screen — landing page shown when no session is active.
//!
//! Ported from: `packages/tui/src/routes/home.tsx`
//! and `packages/tui/src/component/logo.tsx`
//!
//! ## Visual Design (Opencode Match)
//!
//! Vertically centered layout:
//! ```text
//!                                 (blank space)
//!                             ┌─────────────────────┐
//!                             │   Blazecode Logo    │
//!                             │   v0.3.0 · model    │
//!                             └─────────────────────┘
//!                                 (blank space)
//!                          Ask anything... "Fix a TODO..."
//!                      ╹▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀
//!                        build · deepseek-v4-flash
//!                      ╹▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀
//!
//!                         ~/project:main  ⊙ 2 MCP  v0.3.0
//! ```

use ratatui::{
    layout::{Alignment, Rect},
    style::{Modifier, Style},
    text::{Line, Span, Text},
    widgets::{Paragraph},
    Frame,
};

use crate::theme::Theme;

/// Render the home screen — shown when no session is active.
pub fn render_home_screen(
    f: &mut Frame,
    area: Rect,
    theme: &Theme,
    version: &str,
    recent_models: &[String],
    connected: bool,
    is_streaming: bool,
    provider_name: Option<&str>,
    model_name: Option<&str>,
) {
    if area.width < 30 || area.height < 15 {
        // Terminal too small — just show a minimal welcome
        let minimal = Paragraph::new(Text::from(vec![
            Line::from(Span::styled(
                "blazecode TUI",
                Style::default()
                    .fg(theme.accent)
                    .add_modifier(Modifier::BOLD),
            )),
            Line::from(""),
            Line::from(Span::styled(
                "Terminal too small — resize to at least 30x15",
                Style::default().fg(theme.text_muted),
            )),
        ]))
        .alignment(Alignment::Center);
        f.render_widget(minimal, area);
        return;
    }

    // ── Layout ──────────────────────────────────────────────────────
    // Opencode style: vertically centered, single column
    let total_height = area.height;
    let total_width = area.width;

    // Calculate content height
    let logo_height: u16 = 8; // ASCII logo is 7 lines + 1 spacing
    let subtitle_height: u16 = 1;
    let gap1: u16 = 1;
    let features_height: u16 = 2; // spacing + "Quick Start"
    let features_count = 6u16;
    let features_list_height = features_count;
    let gap2: u16 = 1;
    let tip_height: u16 = 1;

    let content_height = logo_height
        + subtitle_height
        + gap1
        + features_height
        + features_list_height
        + gap2
        + tip_height;

    // Vertical centering
    let start_y = if total_height > content_height {
        area.y + (total_height - content_height) / 2
    } else {
        area.y + 2
    };

    let center_x = area.x + total_width / 2;

    // ── 1. Logo ──────────────────────────────────────────────────
    let logo_lines = build_blazecode_logo(theme);

    // Calculate the actual logo width so we can center it
    let max_logo_width = logo_lines
        .iter()
        .map(|l| {
            l.spans
                .iter()
                .map(|s| s.content.len() as u16)
                .sum::<u16>()
        })
        .max()
        .unwrap_or(68);

    let logo_x = center_x.saturating_sub(max_logo_width / 2);
    let logo_area = Rect::new(logo_x, start_y, max_logo_width, logo_height);
    let logo_paragraph = Paragraph::new(logo_lines).style(Style::default().fg(theme.accent));
    f.render_widget(logo_paragraph, logo_area);

    // ── 2. Subtitle line ──────────────────────────────────────────
    let subtitle_y = start_y + logo_height;
    let subtitle_area = Rect::new(area.x, subtitle_y, total_width, subtitle_height);

    let mut subtitle_spans = vec![Span::styled(
        format!("blazecode TUI v{version}"),
        Style::default().fg(theme.text_muted),
    )];

    if connected {
        let provider = provider_name.unwrap_or("?");
        let model = model_name.unwrap_or("auto");
        subtitle_spans.push(Span::styled(
            format!("  ·  {provider}/{model}"),
            Style::default().fg(theme.success),
        ));
    } else {
        subtitle_spans.push(Span::styled(
            "  ·  offline mode",
            Style::default().fg(theme.warning),
        ));
    }

    let subtitle = Paragraph::new(Line::from(subtitle_spans)).alignment(Alignment::Center);
    f.render_widget(subtitle, subtitle_area);

    // ── 3. Prompt area hint ─────────────────────────────────────
    let prompt_y = subtitle_y + gap1 + features_height; // "Quick Start" header
    let prompt_area = Rect::new(area.x, prompt_y, total_width, features_list_height);

    let mut feature_lines = Vec::new();

    // Quick start header
    feature_lines.push(Line::from(Span::styled(
        "  Quick Start",
        Style::default()
            .fg(theme.accent)
            .add_modifier(Modifier::BOLD),
    )));

    // Features
    let features = [
        ("  ⌨", "  Type a message and press Enter to start"),
        ("  📂", "  Ctrl+O to open in editor"),
        ("  ⌘", "  Ctrl+P for command palette"),
        ("  🔄", "  Ctrl+L to cycle providers"),
        ("  ❓", "  Ctrl+/ for help & keybindings"),
        ("  💾", "  Ctrl+S to toggle sidebar"),
    ];

    for (icon, desc) in &features {
        feature_lines.push(Line::from(vec![
            Span::styled(*icon, Style::default().fg(theme.text)),
            Span::styled(*desc, Style::default().fg(theme.text_muted)),
        ]));
    }

    // Recent models (if any)
    if !recent_models.is_empty() {
        feature_lines.push(Line::from(""));
        feature_lines.push(Line::from(Span::styled(
            "  Recent Models",
            Style::default()
                .fg(theme.accent)
                .add_modifier(Modifier::BOLD),
        )));
        for model in recent_models.iter().take(5) {
            feature_lines.push(Line::from(vec![
                Span::styled("  ▲ ", Style::default().fg(theme.success)),
                Span::styled(model.clone(), Style::default().fg(theme.text)),
            ]));
        }
    }

    // Streaming indicator
    if is_streaming {
        feature_lines.push(Line::from(""));
        feature_lines.push(Line::from(Span::styled(
            "  ⟳  Streaming in progress...",
            Style::default().fg(theme.warning),
        )));
    } else if connected {
        feature_lines.push(Line::from(""));
        feature_lines.push(Line::from(Span::styled(
            "  ●  Connected & ready",
            Style::default().fg(theme.success),
        )));
    }

    let features_widget = Paragraph::new(Text::from(feature_lines));
    f.render_widget(features_widget, prompt_area);

    // ── 4. Tip at bottom (rotating, Opencode style) ──────────────
    let tip_y = prompt_y + features_list_height + gap2;
    let tip_area = Rect::new(area.x, tip_y, total_width, tip_height + 1);

    const TIPS: &[&str] = &[
        "  Set \"formatter\": true for auto-formatting",
        "  Ctrl+Q / :q to quit  |  Ctrl+P for commands",
        "  Ctrl+X then S for status  |  Alt+B for sidebar",
        "  Ctrl+L to cycle providers  |  Ctrl+O for editor",
        "  Ask anything and press Enter to start",
        "  Type /help in chat for more info",
    ];
    let tip_idx = (area.height as usize) % TIPS.len();
    let tip_text = Text::from(vec![
        Line::from(Span::styled(TIPS[tip_idx], Style::default().fg(theme.text_muted))),
    ]);
    let tip_widget = Paragraph::new(tip_text).alignment(Alignment::Center);
    f.render_widget(tip_widget, tip_area);
}

/// Build the ASCII art logo — blazecode with a crab.
fn build_blazecode_logo(theme: &Theme) -> Vec<Line<'static>> {
    let color = theme.accent;
    let muted = theme.text_muted;

    vec![
        Line::from(vec![
            Span::styled(
                "  ██████╗ ██╗   ██╗███████╗████████╗ ██████╗ ██████╗ ██████╗ ███████╗",
                Style::default().fg(color),
            ),
        ]),
        Line::from(vec![
            Span::styled(
                "  ██╔══██╗██║   ██║██╔════╝╚══██╔══╝██╔════╝██╔═══██╗██╔══██╗██╔════╝",
                Style::default().fg(color),
            ),
        ]),
        Line::from(vec![
            Span::styled(
                "  ██████╔╝██║   ██║███████╗   ██║   ██║     ██║   ██║██║  ██║█████╗  ",
                Style::default().fg(color),
            ),
        ]),
        Line::from(vec![
            Span::styled(
                "  ██╔══██╗██║   ██║╚════██║   ██║   ██║     ██║   ██║██║  ██║██╔══╝  ",
                Style::default().fg(color),
            ),
        ]),
        Line::from(vec![
            Span::styled(
                "  ██║  ██║╚██████╔╝███████║   ██║   ╚██████╗╚██████╔╝██████╔╝███████╗",
                Style::default().fg(color),
            ),
        ]),
        Line::from(vec![
            Span::styled(
                "  ╚═╝  ╚═╝ ╚═════╝ ╚══════╝   ╚═╝    ╚═════╝ ╚═════╝ ╚═════╝ ╚══════╝",
                Style::default().fg(color),
            ),
        ]),
        Line::from(vec![
            Span::styled("                                                                   🦀", Style::default().fg(muted)),
        ]),
    ]
}
