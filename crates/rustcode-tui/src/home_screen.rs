//! Home screen — landing page shown when no session is active.
//!
//! Ported from: `packages/tui/src/routes/home/session-destination.tsx`
//! and `packages/tui/src/component/logo.tsx`

use ratatui::{
    layout::{Alignment, Rect},
    style::{Modifier, Style},
    text::{Line, Span, Text},
    widgets::{Block, Borders, Paragraph, Wrap},
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
    // ── Layout ──────────────────────────────────────────────────────
    let total_height = area.height;
    let content_y_start = if total_height > 25 {
        (total_height / 5) as u16
    } else {
        1
    };

    let logo_area = Rect::new(area.x, area.y + content_y_start, area.width, 7);

    // ── Logo ────────────────────────────────────────────────────────
    let logo_lines = build_rustcode_logo(theme);
    let logo_paragraph = Paragraph::new(logo_lines)
        .alignment(Alignment::Center)
        .style(Style::default().fg(theme.accent));
    f.render_widget(logo_paragraph, logo_area);

    // ── Subtitle / version ──────────────────────────────────────────
    let subtitle_y = content_y_start + 7;
    let subtitle_area = Rect::new(area.x, area.y + subtitle_y, area.width, 1);

    let mut subtitle_spans = vec![
        Span::styled(
            format!("rustcode TUI v{version}"),
            Style::default().fg(theme.text_muted),
        ),
    ];

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

    let subtitle = Paragraph::new(Line::from(subtitle_spans))
        .alignment(Alignment::Center);
    f.render_widget(subtitle, subtitle_area);

    // ── Features panel ──────────────────────────────────────────────
    let features_y = subtitle_y + 2;
    let features = [
        ("⌨", "  Type a message and press Enter to start"),
        ("📂", "  Ctrl+O to open in editor"),
        ("⌘", "  Ctrl+P for command palette"),
        ("🔄", "  Ctrl+L to cycle providers"),
        ("❓", "  Ctrl+/ for help & keybindings"),
        ("💾", "  Ctrl+S to toggle sidebar"),
    ];

    let mut feature_lines: Vec<Line<'static>> = Vec::new();
    feature_lines.push(Line::from(Span::styled(
        " Quick Start ",
        Style::default()
            .fg(theme.accent)
            .add_modifier(Modifier::BOLD),
    )));
    feature_lines.push(Line::from(""));

    for (icon_text, desc) in &features {
        feature_lines.push(Line::from(vec![
            Span::styled(*icon_text, Style::default().fg(theme.text)),
            Span::styled(*desc, Style::default().fg(theme.text_muted)),
        ]));
    }

    // Recent models section
    if !recent_models.is_empty() {
        feature_lines.push(Line::from(""));
        feature_lines.push(Line::from(Span::styled(
            " Recent Models ",
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

    // Status line
    feature_lines.push(Line::from(""));
    let status_text = if is_streaming {
        "  ⟳  Streaming in progress..."
    } else if connected {
        "  ●  Connected — ready to code"
    } else {
        "  ○  Disconnected — type a message to start in local mode"
    };
    let status_color = if is_streaming {
        theme.accent
    } else if connected {
        theme.success
    } else {
        theme.warning
    };

    feature_lines.push(Line::from(Span::styled(
        status_text,
        Style::default().fg(status_color),
    )));

    let panel_width = 50u16.min(area.width.saturating_sub(4));
    let features_area = Rect::new(
        area.x + (area.width.saturating_sub(panel_width)) / 2,
        area.y + features_y,
        panel_width,
        feature_lines.len() as u16 + 1,
    );

    let features_paragraph = Paragraph::new(Text::from(feature_lines))
        .block(Block::default().borders(Borders::NONE))
        .alignment(Alignment::Left)
        .wrap(Wrap { trim: false });

    let block = Block::default()
        .borders(Borders::ALL)
        .border_style(Style::default().fg(theme.border));
    let block_area = features_area;
    let inner = block.inner(block_area);
    f.render_widget(block, block_area);
    f.render_widget(features_paragraph, inner);

    // ── Bottom tip ──────────────────────────────────────────────────
    let tip_y = area.height.saturating_sub(2);
    let tip_area = Rect::new(area.x, area.y + tip_y, area.width, 1);
    let tip_text = Line::from(vec![
        Span::styled(
            " Ctrl+P commands · ",
            Style::default().fg(theme.text_muted),
        ),
        Span::styled(
            "Ctrl+/ help · ",
            Style::default().fg(theme.text_muted),
        ),
        Span::styled(
            "Type /help in chat · ",
            Style::default().fg(theme.text_muted),
        ),
        Span::styled(
            "Ctrl+Q / :q to quit",
            Style::default().fg(theme.text_muted),
        ),
    ]);
    let tip = Paragraph::new(tip_text).alignment(Alignment::Center);
    f.render_widget(tip, tip_area);
}

/// Build the ASCII art logo — rustcode with a crab.
fn build_rustcode_logo(theme: &Theme) -> Vec<Line<'static>> {
    let color = theme.accent;
    let muted = theme.text_muted;

    vec![
        Line::from(vec![
            Span::styled("  ██████╗ ██╗   ██╗███████╗████████╗ ██████╗ ██████╗ ██████╗ ███████╗", Style::default().fg(color)),
        ]),
        Line::from(vec![
            Span::styled("  ██╔══██╗██║   ██║██╔════╝╚══██╔══╝██╔════╝██╔═══██╗██╔══██╗██╔════╝", Style::default().fg(color)),
        ]),
        Line::from(vec![
            Span::styled("  ██████╔╝██║   ██║███████╗   ██║   ██║     ██║   ██║██║  ██║█████╗  ", Style::default().fg(color)),
        ]),
        Line::from(vec![
            Span::styled("  ██╔══██╗██║   ██║╚════██║   ██║   ██║     ██║   ██║██║  ██║██╔══╝  ", Style::default().fg(color)),
        ]),
        Line::from(vec![
            Span::styled("  ██║  ██║╚██████╔╝███████║   ██║   ╚██████╗╚██████╔╝██████╔╝███████╗", Style::default().fg(color)),
        ]),
        Line::from(vec![
            Span::styled("  ╚═╝  ╚═╝ ╚═════╝ ╚══════╝   ╚═╝    ╚═════╝ ╚═════╝ ╚═════╝ ╚══════╝", Style::default().fg(color)),
        ]),
        Line::from(vec![
            Span::styled("                                                                   🦀", Style::default().fg(muted)),
        ]),
    ]
}
