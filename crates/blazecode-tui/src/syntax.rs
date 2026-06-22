//! Syntax highlighting engine for code blocks in the TUI.
//!
//! Uses `syntect` to tokenize source code and produce ratatui `Span`
//! sequences with ANSI color mappings.  A lazy-static singleton caches
//! the syntax set and the base16-ocean.dark theme.

use std::sync::OnceLock;
use ratatui::style::{Color, Style};
use syntect::highlighting::ThemeSet;
use syntect::parsing::SyntaxSet;
use syntect::easy::HighlightLines;

/// Cached syntax set (all defaults).
static SYNTAX_SET: OnceLock<SyntaxSet> = OnceLock::new();

/// Cached theme set.
static THEME_SET: OnceLock<ThemeSet> = OnceLock::new();

/// Get (or initialise) the shared syntax set.
fn syntax_set() -> &'static SyntaxSet {
    SYNTAX_SET.get_or_init(SyntaxSet::load_defaults_newlines)
}

/// Get (or initialise) the shared theme set.
fn theme_set() -> &'static ThemeSet {
    THEME_SET.get_or_init(ThemeSet::load_defaults)
}

/// Highlight a code block and return ratatui `Span`s.
///
/// Returns `None` if highlighting fails or no matching syntax is found
/// (the caller should fall back to plain rendering).
pub fn highlight_code(
    code: &str,
    language: Option<&str>,
) -> Option<Vec<ratatui::text::Span<'static>>> {
    let ss = syntax_set();

    let syntax_ref = language
        .and_then(|lang| {
            ss.find_syntax_by_extension(lang)
                .or_else(|| ss.find_syntax_by_token(lang))
        })
        .or_else(|| ss.find_syntax_by_extension("rs"))
        .or_else(|| Some(ss.find_syntax_plain_text()))?;

    let theme = &theme_set().themes["base16-ocean.dark"];

    let mut highlighter = HighlightLines::new(syntax_ref, theme);
    let mut spans: Vec<ratatui::text::Span<'static>> = Vec::new();

    for line in code.lines() {
        let regions = highlighter.highlight_line(line, ss).ok()?;

        for (style, text) in regions {
            let color = syntect_fg_to_ratatui(&style);
            spans.push(ratatui::text::Span::styled(
                text.to_string(),
                color,
            ));
        }
        // Newline between lines
        spans.push(ratatui::text::Span::raw("\n"));
    }

    Some(spans)
}

/// Extract foreground color from a `syntect::highlighting::Style`.
fn syntect_fg_to_ratatui(s: &syntect::highlighting::Style) -> Style {
    let c = s.foreground;
    Style::default().fg(Color::Rgb(c.r, c.g, c.b))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_highlight_rust() {
        let result = highlight_code(
            r#"fn hello() -> &'static str { "world" }"#,
            Some("rs"),
        );
        assert!(result.is_some(), "should produce spans");
        let spans = result.unwrap();
        assert!(!spans.is_empty(), "should produce at least one span");
    }
}
