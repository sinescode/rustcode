//! TUI theme system — 35 built-in color themes with 50+ properties each.
//!
//! Ported from: `packages/tui/src/context/theme.tsx` and `packages/tui/src/theme/`
//!
//! ## Built-in Themes (35 total)
//!
//! | Name               | Style    | Background | Accent   |
//! |--------------------|----------|------------|----------|
//! | dark               | dark     | #1a1b26    | #7aa2f7  |
//! | light              | light    | #e0e0e0    | #3366cc  |
//! | dracula            | dark     | #282a36    | #bd93f9  |
//! | monokai            | dark     | #272822    | #a6e22e  |
//! | nord               | dark     | #2e3440    | #88c0d0  |
//! | solarized          | dark     | #002b36    | #268bd2  |
//! | github             | light    | #ffffff    | #0969da  |
//! | tokyonight         | dark     | #1a1b26    | #7dcfff  |
//! | aura               | dark     | #21202e    | #a277ff  |
//! | ayu                | dark     | #1a1b24    | #ffb454  |
//! | carbonfox          | dark     | #161616    | #78a9ff  |
//! | catppuccin         | dark     | #1e1e2e    | #cba6f7  |
//! | catppuccin-frappe  | dark     | #303446    | #ca9ee6  |
//! | catppuccin-macchiato| dark    | #24273a    | #c6a0f6  |
//! | cobalt2            | dark     | #122637    | #ffc600  |
//! | cursor             | dark     | #1a1a1a    | #8a7ef7  |
//! | everforest         | dark     | #2d353b    | #a7c080  |
//! | flexoki            | dark     | #1c1b1a    | #ce5d03  |
//! | gruvbox            | dark     | #282828    | #d79921  |
//! | kanagawa           | dark     | #1f1f28    | #7fb4ca  |
//! | lucent-orng        | dark     | #1c1c1c    | #ff6b35  |
//! | material           | dark     | #1e1e2e    | #89b4fa  |
//! | matrix             | dark     | #000000    | #00ff41  |
//! | mercury            | dark     | #1a1a1a    | #6c8fbf  |
//! | nightowl           | light    | #f8f8f2    | #7e57c2  |
//! | one-dark           | dark     | #282c34    | #61afef  |
//! | blazecode           | dark     | #0d1117    | #58a6ff  |
//! | orng               | dark     | #1a1a1a    | #ff8c00  |
//! | osaka-jade         | dark     | #1b1e2b    | #7bc99d  |
//! | palenight          | dark     | #292d3e    | #82aaff  |
//! | rosepine           | dark     | #191724    | #ebbcba  |
//! | synthwave84        | dark     | #262335    | #ff7edb  |
//! | vercel             | light    | #ffffff    | #000000  |
//! | vesper             | dark     | #101010    | #c0a36e  |
//! | zenburn            | dark     | #3f3f3f    | #dcdccc  |

use ratatui::style::Color;
use std::collections::HashMap;
use std::path::PathBuf;

// ── Theme Definition ────────────────────────────────────────────────────────

/// A color theme for the TUI with ~50 color properties matching TS parity.
///
/// # Source
/// Ported from `packages/tui/src/theme/index.ts` `Theme` type (50 fields).
#[derive(Debug, Clone)]
pub struct Theme {
    /// Theme name (e.g. "dark", "dracula").
    pub name: &'static str,
    /// Whether this is a dark or light theme.
    pub mode: ThemeMode,

    // ── Core identity colors ──────────────────────────────────────────
    pub primary: Color,
    pub secondary: Color,
    pub accent: Color,

    // ── Status colors ─────────────────────────────────────────────────
    pub error: Color,
    pub warning: Color,
    pub success: Color,
    pub info: Color,

    // ── Text colors ───────────────────────────────────────────────────
    pub text: Color,
    pub text_muted: Color,
    pub selected_list_item_text: Color,

    // ── Background colors ─────────────────────────────────────────────
    pub background: Color,
    pub background_panel: Color,
    pub background_element: Color,
    pub background_menu: Color,

    // ── Border colors ─────────────────────────────────────────────────
    pub border: Color,
    pub border_active: Color,
    pub border_subtle: Color,

    // ── Diff colors ───────────────────────────────────────────────────
    pub diff_added: Color,
    pub diff_removed: Color,
    pub diff_context: Color,
    pub diff_hunk_header: Color,
    pub diff_highlight_added: Color,
    pub diff_highlight_removed: Color,
    pub diff_added_bg: Color,
    pub diff_removed_bg: Color,
    pub diff_context_bg: Color,
    pub diff_line_number: Color,
    pub diff_added_line_number_bg: Color,
    pub diff_removed_line_number_bg: Color,

    // ── Markdown colors ───────────────────────────────────────────────
    pub markdown_text: Color,
    pub markdown_heading: Color,
    pub markdown_link: Color,
    pub markdown_link_text: Color,
    pub markdown_code: Color,
    pub markdown_block_quote: Color,
    pub markdown_emph: Color,
    pub markdown_strong: Color,
    pub markdown_horizontal_rule: Color,
    pub markdown_list_item: Color,
    pub markdown_list_enumeration: Color,
    pub markdown_image: Color,
    pub markdown_image_text: Color,
    pub markdown_code_block: Color,

    // ── Syntax highlight colors ───────────────────────────────────────
    pub syntax_comment: Color,
    pub syntax_keyword: Color,
    pub syntax_function: Color,
    pub syntax_variable: Color,
    pub syntax_string: Color,
    pub syntax_number: Color,
    pub syntax_type: Color,
    pub syntax_operator: Color,
    pub syntax_punctuation: Color,

    // ── Misc ──────────────────────────────────────────────────────────
    /// Opacity for thinking/reasoning overlay (0.0–1.0).
    pub thinking_opacity: f64,
    /// Whether selectedListItemText was explicitly defined.
    pub has_selected_list_item_text: bool,
}

impl Theme {
    /// Derive a full 50-field theme from 10 core colors plus name/mode.
    ///
    /// Most diff, markdown, and syntax fields are populated with sensible
    /// defaults from the core palette. Use the builder-style `with_*`
    /// methods to override specific fields.
    pub const fn derive(
        name: &'static str,
        mode: ThemeMode,
        background: Color,
        foreground: Color,
        accent: Color,
        dim: Color,
        border: Color,
        success: Color,
        warning: Color,
        error: Color,
        info: Color,
    ) -> Self {
        let text = foreground;
        let text_muted = dim;
        let primary = accent;
        let secondary = dim;
        let background_panel = background;
        let background_element = background;
        let background_menu = background;
        let border_active = accent;
        let border_subtle = dim;
        let diff_added = success;
        let diff_removed = error;
        let diff_context = dim;
        let diff_hunk_header = dim;
        let diff_highlight_added = success;
        let diff_highlight_removed = error;
        let diff_added_bg = background;
        let diff_removed_bg = background;
        let diff_context_bg = background;
        let diff_line_number = dim;
        let diff_added_line_number_bg = background;
        let diff_removed_line_number_bg = background;
        let markdown_text = text;
        let markdown_heading = primary;
        let markdown_link = info;
        let markdown_link_text = accent;
        let markdown_code = success;
        let markdown_block_quote = text_muted;
        let markdown_emph = warning;
        let markdown_strong = warning;
        let markdown_horizontal_rule = border;
        let markdown_list_item = primary;
        let markdown_list_enumeration = info;
        let markdown_image = info;
        let markdown_image_text = accent;
        let markdown_code_block = text;
        let syntax_comment = text_muted;
        let syntax_keyword = secondary;
        let syntax_function = primary;
        let syntax_variable = text;
        let syntax_string = success;
        let syntax_number = warning;
        let syntax_type = info;
        let syntax_operator = info;
        let syntax_punctuation = text;

        Self {
            name,
            mode,
            primary,
            secondary,
            accent,
            error,
            warning,
            success,
            info,
            text,
            text_muted,
            selected_list_item_text: background,
            background,
            background_panel,
            background_element,
            background_menu,
            border,
            border_active,
            border_subtle,
            diff_added,
            diff_removed,
            diff_context,
            diff_hunk_header,
            diff_highlight_added,
            diff_highlight_removed,
            diff_added_bg,
            diff_removed_bg,
            diff_context_bg,
            diff_line_number,
            diff_added_line_number_bg,
            diff_removed_line_number_bg,
            markdown_text,
            markdown_heading,
            markdown_link,
            markdown_link_text,
            markdown_code,
            markdown_block_quote,
            markdown_emph,
            markdown_strong,
            markdown_horizontal_rule,
            markdown_list_item,
            markdown_list_enumeration,
            markdown_image,
            markdown_image_text,
            markdown_code_block,
            syntax_comment,
            syntax_keyword,
            syntax_function,
            syntax_variable,
            syntax_string,
            syntax_number,
            syntax_type,
            syntax_operator,
            syntax_punctuation,
            thinking_opacity: 0.6,
            has_selected_list_item_text: false,
        }
    }

    /// Override the primary color.
    pub const fn with_primary(mut self, color: Color) -> Self {
        self.primary = color;
        self
    }

    /// Override the secondary color.
    pub const fn with_secondary(mut self, color: Color) -> Self {
        self.secondary = color;
        self
    }

    /// Override the background panel color.
    pub const fn with_background_panel(mut self, color: Color) -> Self {
        self.background_panel = color;
        self
    }

    /// Override the background element color.
    pub const fn with_background_element(mut self, color: Color) -> Self {
        self.background_element = color;
        self
    }

    /// Override the background menu color.
    pub const fn with_background_menu(mut self, color: Color) -> Self {
        self.background_menu = color;
        self
    }

    /// Override the border color.
    pub const fn with_border(mut self, color: Color) -> Self {
        self.border = color;
        self
    }

    /// Override the border active color.
    pub const fn with_border_active(mut self, color: Color) -> Self {
        self.border_active = color;
        self
    }

    /// Override the border subtle color.
    pub const fn with_border_subtle(mut self, color: Color) -> Self {
        self.border_subtle = color;
        self
    }

    /// Override diff colors.
    pub const fn with_diff(mut self, added: Color, removed: Color, context: Color) -> Self {
        self.diff_added = added;
        self.diff_removed = removed;
        self.diff_context = context;
        self
    }

    /// Override diff background colors.
    pub const fn with_diff_bg(mut self, added: Color, removed: Color, context: Color) -> Self {
        self.diff_added_bg = added;
        self.diff_removed_bg = removed;
        self.diff_context_bg = context;
        self
    }

    /// Override markdown colors.
    pub const fn with_markdown(mut self, heading: Color, link: Color, code: Color) -> Self {
        self.markdown_heading = heading;
        self.markdown_link = link;
        self.markdown_code = code;
        self
    }

    /// Override syntax colors.
    pub const fn with_syntax(mut self, comment: Color, keyword: Color, func: Color, var: Color, str: Color, num: Color) -> Self {
        self.syntax_comment = comment;
        self.syntax_keyword = keyword;
        self.syntax_function = func;
        self.syntax_variable = var;
        self.syntax_string = str;
        self.syntax_number = num;
        self
    }

    /// Override thinking opacity.
    pub const fn with_thinking_opacity(mut self, opacity: f64) -> Self {
        self.thinking_opacity = opacity;
        self
    }

    /// Override the text muted color.
    pub const fn with_text_muted(mut self, color: Color) -> Self {
        self.text_muted = color;
        self
    }

    /// Override selected list item text color.
    pub const fn with_selected_list_item_text(mut self, color: Color) -> Self {
        self.selected_list_item_text = color;
        self.has_selected_list_item_text = true;
        self
    }

    /// Create a fully custom theme specifying all fields.
    #[allow(clippy::too_many_arguments)]
    pub const fn full(
        name: &'static str,
        mode: ThemeMode,
        primary: Color,
        secondary: Color,
        accent: Color,
        error: Color,
        warning: Color,
        success: Color,
        info: Color,
        text: Color,
        text_muted: Color,
        selected_list_item_text: Color,
        background: Color,
        background_panel: Color,
        background_element: Color,
        background_menu: Color,
        border: Color,
        border_active: Color,
        border_subtle: Color,
        diff_added: Color,
        diff_removed: Color,
        diff_context: Color,
        diff_hunk_header: Color,
        diff_highlight_added: Color,
        diff_highlight_removed: Color,
        diff_added_bg: Color,
        diff_removed_bg: Color,
        diff_context_bg: Color,
        diff_line_number: Color,
        diff_added_line_number_bg: Color,
        diff_removed_line_number_bg: Color,
        markdown_text: Color,
        markdown_heading: Color,
        markdown_link: Color,
        markdown_link_text: Color,
        markdown_code: Color,
        markdown_block_quote: Color,
        markdown_emph: Color,
        markdown_strong: Color,
        markdown_horizontal_rule: Color,
        markdown_list_item: Color,
        markdown_list_enumeration: Color,
        markdown_image: Color,
        markdown_image_text: Color,
        markdown_code_block: Color,
        syntax_comment: Color,
        syntax_keyword: Color,
        syntax_function: Color,
        syntax_variable: Color,
        syntax_string: Color,
        syntax_number: Color,
        syntax_type: Color,
        syntax_operator: Color,
        syntax_punctuation: Color,
        thinking_opacity: f64,
        has_selected_list_item_text: bool,
    ) -> Self {
        Self {
            name, mode, primary, secondary, accent,
            error, warning, success, info,
            text, text_muted, selected_list_item_text,
            background, background_panel, background_element, background_menu,
            border, border_active, border_subtle,
            diff_added, diff_removed, diff_context, diff_hunk_header,
            diff_highlight_added, diff_highlight_removed,
            diff_added_bg, diff_removed_bg, diff_context_bg,
            diff_line_number, diff_added_line_number_bg, diff_removed_line_number_bg,
            markdown_text, markdown_heading, markdown_link, markdown_link_text,
            markdown_code, markdown_block_quote, markdown_emph, markdown_strong,
            markdown_horizontal_rule, markdown_list_item, markdown_list_enumeration,
            markdown_image, markdown_image_text, markdown_code_block,
            syntax_comment, syntax_keyword, syntax_function, syntax_variable,
            syntax_string, syntax_number, syntax_type, syntax_operator, syntax_punctuation,
            thinking_opacity, has_selected_list_item_text,
        }
    }
}

// ── Theme Mode ──────────────────────────────────────────────────────────────

/// Light or dark mode.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ThemeMode {
    Dark,
    Light,
}

impl ThemeMode {
    /// Return the opposite mode.
    pub fn invert(self) -> Self {
        match self {
            ThemeMode::Dark => ThemeMode::Light,
            ThemeMode::Light => ThemeMode::Dark,
        }
    }

    /// Human-readable name.
    pub fn as_str(self) -> &'static str {
        match self {
            ThemeMode::Dark => "dark",
            ThemeMode::Light => "light",
        }
    }
}

// ── Helper: make an RGB color ──────────────────────────────────────────────

const fn rgb(r: u8, g: u8, b: u8) -> Color {
    Color::Rgb(r, g, b)
}

// ── Built-in Themes ─────────────────────────────────────────────────────────

/// Dark (default dark theme) — background: #1a1b26, accent: #7aa2f7
pub const THEME_DARK: Theme = Theme::derive(
    "dark", ThemeMode::Dark,
    rgb(0x1a, 0x1b, 0x26), // background
    rgb(0xc0, 0xca, 0xf5), // foreground
    rgb(0x7a, 0xa2, 0xf7), // accent
    rgb(0x56, 0x5f, 0x89), // dim
    rgb(0x29, 0x2e, 0x42), // border
    rgb(0x9e, 0xce, 0x6a), // success
    rgb(0xe0, 0xaf, 0x68), // warning
    rgb(0xf7, 0x76, 0x8e), // error
    rgb(0x7d, 0xcf, 0xff), // info
);

/// Light — background: #e0e0e0, accent: #3366cc
pub const THEME_LIGHT: Theme = Theme::derive(
    "light", ThemeMode::Light,
    rgb(0xe0, 0xe0, 0xe0),
    rgb(0x1a, 0x1a, 0x1a),
    rgb(0x33, 0x66, 0xcc),
    rgb(0x88, 0x88, 0x88),
    rgb(0xbb, 0xbb, 0xbb),
    rgb(0x1a, 0x7f, 0x37),
    rgb(0x9a, 0x67, 0x00),
    rgb(0xcf, 0x22, 0x2e),
    rgb(0x09, 0x69, 0xda),
);

/// Dracula — background: #282a36, accent: #bd93f9
pub const THEME_DRACULA: Theme = Theme::derive(
    "dracula", ThemeMode::Dark,
    rgb(0x28, 0x2a, 0x36),
    rgb(0xf8, 0xf8, 0xf2),
    rgb(0xbd, 0x93, 0xf9),
    rgb(0x62, 0x72, 0xa4),
    rgb(0x44, 0x47, 0x5a),
    rgb(0x50, 0xfa, 0x7b),
    rgb(0xff, 0xb8, 0x6c),
    rgb(0xff, 0x55, 0x55),
    rgb(0x8b, 0xe9, 0xfd),
).with_secondary(rgb(0xff, 0x79, 0xc6))
 .with_background_panel(rgb(0x21, 0x22, 0x2c))
 .with_background_element(rgb(0x44, 0x47, 0x5a))
 .with_border_active(rgb(0xbd, 0x93, 0xf9))
 .with_border_subtle(rgb(0x19, 0x1a, 0x21))
 .with_diff_bg(rgb(0x1a, 0x3a, 0x1a), rgb(0x3a, 0x1a, 0x1a), rgb(0x21, 0x22, 0x2c))
 .with_diff(rgb(0x50, 0xfa, 0x7b), rgb(0xff, 0x55, 0x55), rgb(0x62, 0x72, 0xa4))
 .with_diff_hunk_header_set(rgb(0x62, 0x72, 0xa4))
 .with_diff_line_number(rgb(0x98, 0x9a, 0xa4))
 .with_diff_line_number_bg(rgb(0x1a, 0x3a, 0x1a), rgb(0x3a, 0x1a, 0x1a))
 .with_markdown(rgb(0xbd, 0x93, 0xf9), rgb(0x8b, 0xe9, 0xfd), rgb(0x50, 0xfa, 0x7b))
 .with_syntax(
    rgb(0x62, 0x72, 0xa4), // comment
    rgb(0xff, 0x79, 0xc6), // keyword
    rgb(0x50, 0xfa, 0x7b), // function
    rgb(0xf8, 0xf8, 0xf2), // variable
    rgb(0xf1, 0xfa, 0x8c), // string
    rgb(0xbd, 0x93, 0xf9), // number
);

// ── Helper methods only used for Dracula's advanced overrides ─────────
// (These are defined at the end of the file in the Theme impl block)

/// Monokai — background: #272822, accent: #a6e22e
pub const THEME_MONOKAI: Theme = Theme::derive(
    "monokai", ThemeMode::Dark,
    rgb(0x27, 0x28, 0x22),
    rgb(0xf8, 0xf8, 0xf2),
    rgb(0xa6, 0xe2, 0x2e),
    rgb(0x75, 0x71, 0x5e),
    rgb(0x3e, 0x3d, 0x32),
    rgb(0xa6, 0xe2, 0x2e),
    rgb(0xfd, 0x97, 0x1f),
    rgb(0xf9, 0x26, 0x72),
    rgb(0x66, 0xd9, 0xef),
).with_secondary(rgb(0x66, 0xd9, 0xef));

/// Nord — background: #2e3440, accent: #88c0d0
pub const THEME_NORD: Theme = Theme::derive(
    "nord", ThemeMode::Dark,
    rgb(0x2e, 0x34, 0x40),
    rgb(0xec, 0xef, 0xf4),
    rgb(0x88, 0xc0, 0xd0),
    rgb(0x4c, 0x56, 0x6a),
    rgb(0x3b, 0x42, 0x52),
    rgb(0xa3, 0xbe, 0x8c),
    rgb(0xeb, 0xcb, 0x8b),
    rgb(0xbf, 0x61, 0x6a),
    rgb(0x81, 0xa1, 0xc1),
);

/// Solarized — background: #002b36, accent: #268bd2
pub const THEME_SOLARIZED: Theme = Theme::derive(
    "solarized", ThemeMode::Dark,
    rgb(0x00, 0x2b, 0x36),
    rgb(0x83, 0x94, 0x96),
    rgb(0x26, 0x8b, 0xd2),
    rgb(0x58, 0x6e, 0x75),
    rgb(0x07, 0x36, 0x42),
    rgb(0x85, 0x99, 0x00),
    rgb(0xb5, 0x89, 0x00),
    rgb(0xdc, 0x32, 0x2f),
    rgb(0x2a, 0xa1, 0x98),
);

/// GitHub — background: #ffffff, accent: #0969da
pub const THEME_GITHUB: Theme = Theme::derive(
    "github", ThemeMode::Light,
    rgb(0xff, 0xff, 0xff),
    rgb(0x1f, 0x23, 0x28),
    rgb(0x09, 0x69, 0xda),
    rgb(0x65, 0x6d, 0x76),
    rgb(0xd0, 0xd7, 0xde),
    rgb(0x1a, 0x7f, 0x37),
    rgb(0x9a, 0x67, 0x00),
    rgb(0xcf, 0x22, 0x2e),
    rgb(0x09, 0x69, 0xda),
);

/// TokyoNight — background: #1a1b26, accent: #7dcfff
pub const THEME_TOKYONIGHT: Theme = Theme::derive(
    "tokyonight", ThemeMode::Dark,
    rgb(0x1a, 0x1b, 0x26),
    rgb(0xc0, 0xca, 0xf5),
    rgb(0x7d, 0xcf, 0xff),
    rgb(0x56, 0x5f, 0x89),
    rgb(0x29, 0x2e, 0x42),
    rgb(0x9e, 0xce, 0x6a),
    rgb(0xe0, 0xaf, 0x68),
    rgb(0xf7, 0x76, 0x8e),
    rgb(0x7d, 0xcf, 0xff),
).with_primary(rgb(0x82, 0xaa, 0xff))
 .with_secondary(rgb(0xc0, 0x99, 0xff))
 .with_background_panel(rgb(0x1e, 0x20, 0x30))
 .with_background_element(rgb(0x22, 0x24, 0x36))
 .with_background_menu(rgb(0x22, 0x24, 0x36))
 .with_border(rgb(0x54, 0x5c, 0x7e))
 .with_border_active(rgb(0x73, 0x7a, 0xa2))
 .with_border_subtle(rgb(0x3b, 0x42, 0x61))
 .with_text_muted(rgb(0x82, 0x8b, 0xb8))
 .with_diff(rgb(0x4f, 0xd6, 0xbe), rgb(0xc5, 0x3b, 0x53), rgb(0x82, 0x8b, 0xb8))
 .with_diff_bg(rgb(0x20, 0x30, 0x3b), rgb(0x37, 0x22, 0x2c), rgb(0x1e, 0x20, 0x30))
 .with_diff_hunk_header_set(rgb(0x82, 0x8b, 0xb8))
 .with_diff_line_number(rgb(0x8f, 0x90, 0x9a))
 .with_diff_line_number_bg(rgb(0x1b, 0x2b, 0x34), rgb(0x2d, 0x1f, 0x26))
 .with_syntax(
    rgb(0x82, 0x8b, 0xb8), // comment
    rgb(0xc0, 0x99, 0xff), // keyword
    rgb(0x82, 0xaa, 0xff), // function
    rgb(0xff, 0x75, 0x7f), // variable
    rgb(0xc3, 0xe8, 0x8d), // string
    rgb(0xff, 0x96, 0x6c), // number
);

// ══════════════════════════════════════════════════════════════════════════
// Remaining 27 Themes
// ══════════════════════════════════════════════════════════════════════════

/// Aura — background: #21202e, accent: #a277ff
pub const THEME_AURA: Theme = Theme::derive(
    "aura", ThemeMode::Dark,
    rgb(0x21, 0x20, 0x2e),
    rgb(0xec, 0xec, 0xf0),
    rgb(0xa2, 0x77, 0xff),
    rgb(0x6d, 0x6c, 0x7a),
    rgb(0x2e, 0x2d, 0x3e),
    rgb(0x61, 0xff, 0xca),
    rgb(0xff, 0xca, 0x85),
    rgb(0xff, 0x67, 0x67),
    rgb(0x82, 0xca, 0xff),
).with_secondary(rgb(0xff, 0x67, 0x67))
 .with_background_panel(rgb(0x1a, 0x19, 0x27))
 .with_background_element(rgb(0x2e, 0x2d, 0x3e))
 .with_syntax(
    rgb(0x6d, 0x6c, 0x7a),
    rgb(0xff, 0x67, 0x67),
    rgb(0xa2, 0x77, 0xff),
    rgb(0xec, 0xec, 0xf0),
    rgb(0x61, 0xff, 0xca),
    rgb(0xff, 0xca, 0x85),
);

/// Ayu — background: #1a1b24, accent: #ffb454
pub const THEME_AYU: Theme = Theme::derive(
    "ayu", ThemeMode::Dark,
    rgb(0x1a, 0x1b, 0x24),
    rgb(0xcb, 0xcc, 0xc6),
    rgb(0xff, 0xb4, 0x54),
    rgb(0x5c, 0x5f, 0x66),
    rgb(0x2e, 0x30, 0x3a),
    rgb(0x7b, 0xd1, 0x8d),
    rgb(0xff, 0xb4, 0x54),
    rgb(0xf0, 0x71, 0x78),
    rgb(0x59, 0xc2, 0xff),
).with_primary(rgb(0x59, 0xc2, 0xff))
 .with_secondary(rgb(0xd4, 0xa0, 0xff))
 .with_background_panel(rgb(0x14, 0x15, 0x1e))
 .with_background_element(rgb(0x21, 0x23, 0x2d))
 .with_syntax(
    rgb(0x5c, 0x5f, 0x66),
    rgb(0xd4, 0xa0, 0xff),
    rgb(0x59, 0xc2, 0xff),
    rgb(0xcb, 0xcc, 0xc6),
    rgb(0x7b, 0xd1, 0x8d),
    rgb(0xff, 0xb4, 0x54),
);

/// Carbonfox — background: #161616, accent: #78a9ff
pub const THEME_CARBONFOX: Theme = Theme::derive(
    "carbonfox", ThemeMode::Dark,
    rgb(0x16, 0x16, 0x16),
    rgb(0xf2, 0xf4, 0xf8),
    rgb(0x78, 0xa9, 0xff),
    rgb(0x6f, 0x6f, 0x6f),
    rgb(0x39, 0x39, 0x39),
    rgb(0x42, 0xbe, 0x65),
    rgb(0xfe, 0xc8, 0x44),
    rgb(0xfa, 0x4d, 0x56),
    rgb(0x33, 0xbb, 0xff),
).with_background_panel(rgb(0x21, 0x21, 0x21))
 .with_background_element(rgb(0x2b, 0x2b, 0x2b))
 .with_syntax(
    rgb(0x6f, 0x6f, 0x6f),
    rgb(0xbe, 0x95, 0xff),
    rgb(0x78, 0xa9, 0xff),
    rgb(0xf2, 0xf4, 0xf8),
    rgb(0x42, 0xbe, 0x65),
    rgb(0xfe, 0xc8, 0x44),
);

/// Catppuccin — background: #1e1e2e, accent: #cba6f7
pub const THEME_CATPPUCCIN: Theme = Theme::derive(
    "catppuccin", ThemeMode::Dark,
    rgb(0x1e, 0x1e, 0x2e),
    rgb(0xcd, 0xd6, 0xf4),
    rgb(0xcb, 0xa6, 0xf7),
    rgb(0x6c, 0x70, 0x86),
    rgb(0x31, 0x32, 0x44),
    rgb(0xa6, 0xe3, 0xa1),
    rgb(0xfa, 0xe3, 0xb0),
    rgb(0xf3, 0x8b, 0xa8),
    rgb(0x89, 0xbe, 0xbf),
).with_primary(rgb(0x89, 0xbe, 0xbf))
 .with_secondary(rgb(0xf3, 0x8b, 0xa8))
 .with_background_panel(rgb(0x18, 0x18, 0x26))
 .with_background_element(rgb(0x25, 0x26, 0x37))
 .with_syntax(
    rgb(0x6c, 0x70, 0x86),
    rgb(0xcb, 0xa6, 0xf7),
    rgb(0x89, 0xbe, 0xbf),
    rgb(0xcd, 0xd6, 0xf4),
    rgb(0xa6, 0xe3, 0xa1),
    rgb(0xfa, 0xe3, 0xb0),
);

/// Catppuccin Frappé — background: #303446, accent: #ca9ee6
pub const THEME_CATPPUCCIN_FRAPPE: Theme = Theme::derive(
    "catppuccin-frappe", ThemeMode::Dark,
    rgb(0x30, 0x34, 0x46),
    rgb(0xc6, 0xd0, 0xf5),
    rgb(0xca, 0x9e, 0xe6),
    rgb(0x73, 0x78, 0x8f),
    rgb(0x41, 0x45, 0x59),
    rgb(0xa6, 0xd1, 0x89),
    rgb(0xe5, 0xc8, 0x90),
    rgb(0xe7, 0x82, 0x84),
    rgb(0x85, 0xc1, 0xdc),
).with_background_panel(rgb(0x29, 0x2c, 0x3c))
 .with_background_element(rgb(0x35, 0x38, 0x4f))
 .with_syntax(
    rgb(0x73, 0x78, 0x8f),
    rgb(0xca, 0x9e, 0xe6),
    rgb(0x85, 0xc1, 0xdc),
    rgb(0xc6, 0xd0, 0xf5),
    rgb(0xa6, 0xd1, 0x89),
    rgb(0xe5, 0xc8, 0x90),
);

/// Catppuccin Macchiato — background: #24273a, accent: #c6a0f6
pub const THEME_CATPPUCCIN_MACCHIATO: Theme = Theme::derive(
    "catppuccin-macchiato", ThemeMode::Dark,
    rgb(0x24, 0x27, 0x3a),
    rgb(0xca, 0xd3, 0xf5),
    rgb(0xc6, 0xa0, 0xf6),
    rgb(0x6e, 0x73, 0x8c),
    rgb(0x36, 0x39, 0x4f),
    rgb(0xa6, 0xda, 0x95),
    rgb(0xee, 0xd4, 0x9f),
    rgb(0xed, 0x87, 0x96),
    rgb(0x8b, 0xd5, 0xca),
).with_background_panel(rgb(0x1e, 0x20, 0x31))
 .with_background_element(rgb(0x2a, 0x2d, 0x43))
 .with_syntax(
    rgb(0x6e, 0x73, 0x8c),
    rgb(0xc6, 0xa0, 0xf6),
    rgb(0x8b, 0xd5, 0xca),
    rgb(0xca, 0xd3, 0xf5),
    rgb(0xa6, 0xda, 0x95),
    rgb(0xee, 0xd4, 0x9f),
);

/// Cobalt2 — background: #122637, accent: #ffc600
pub const THEME_COBALT2: Theme = Theme::derive(
    "cobalt2", ThemeMode::Dark,
    rgb(0x12, 0x26, 0x37),
    rgb(0xdd, 0xe5, 0xf0),
    rgb(0xff, 0xc6, 0x00),
    rgb(0x55, 0x66, 0x77),
    rgb(0x1e, 0x3a, 0x4d),
    rgb(0x3e, 0xb3, 0x7a),
    rgb(0xff, 0xc6, 0x00),
    rgb(0xff, 0x4a, 0x4a),
    rgb(0x89, 0xbe, 0xff),
).with_primary(rgb(0x89, 0xbe, 0xff))
 .with_secondary(rgb(0x89, 0xbe, 0xff))
 .with_background_panel(rgb(0x0e, 0x1d, 0x2b))
 .with_background_element(rgb(0x15, 0x2b, 0x3f))
 .with_syntax(
    rgb(0x55, 0x66, 0x77),
    rgb(0x89, 0xbe, 0xff),
    rgb(0xff, 0x9d, 0x00),
    rgb(0xdd, 0xe5, 0xf0),
    rgb(0x3e, 0xb3, 0x7a),
    rgb(0xff, 0x9d, 0x00),
);

/// Cursor — background: #1a1a1a, accent: #8a7ef7
pub const THEME_CURSOR: Theme = Theme::derive(
    "cursor", ThemeMode::Dark,
    rgb(0x1a, 0x1a, 0x1a),
    rgb(0xdd, 0xdd, 0xdd),
    rgb(0x8a, 0x7e, 0xf7),
    rgb(0x66, 0x66, 0x66),
    rgb(0x2a, 0x2a, 0x2a),
    rgb(0x4a, 0xcc, 0x7a),
    rgb(0xd4, 0xa5, 0x3a),
    rgb(0xf7, 0x5a, 0x5a),
    rgb(0x6a, 0x9a, 0xf7),
).with_background_panel(rgb(0x22, 0x22, 0x22))
 .with_background_element(rgb(0x2d, 0x2d, 0x2d))
 .with_syntax(
    rgb(0x66, 0x66, 0x66),
    rgb(0x8a, 0x7e, 0xf7),
    rgb(0x6a, 0x9a, 0xf7),
    rgb(0xdd, 0xdd, 0xdd),
    rgb(0x4a, 0xcc, 0x7a),
    rgb(0xd4, 0xa5, 0x3a),
);

/// Everforest — background: #2d353b, accent: #a7c080
pub const THEME_EVERFOREST: Theme = Theme::derive(
    "everforest", ThemeMode::Dark,
    rgb(0x2d, 0x35, 0x3b),
    rgb(0xd3, 0xc6, 0xaa),
    rgb(0xa7, 0xc0, 0x80),
    rgb(0x75, 0x82, 0x70),
    rgb(0x3d, 0x48, 0x4d),
    rgb(0xa7, 0xc0, 0x80),
    rgb(0xd6, 0x9b, 0x5c),
    rgb(0xe6, 0x7e, 0x80),
    rgb(0x83, 0xc0, 0x92),
).with_background_panel(rgb(0x27, 0x2e, 0x33))
 .with_background_element(rgb(0x34, 0x3f, 0x44))
 .with_syntax(
    rgb(0x75, 0x82, 0x70),
    rgb(0xe6, 0x7e, 0x80),
    rgb(0xa7, 0xc0, 0x80),
    rgb(0xd3, 0xc6, 0xaa),
    rgb(0xa7, 0xc0, 0x80),
    rgb(0xd6, 0x9b, 0x5c),
);

/// Flexoki — background: #1c1b1a, accent: #ce5d03
pub const THEME_FLEXOKI: Theme = Theme::derive(
    "flexoki", ThemeMode::Dark,
    rgb(0x1c, 0x1b, 0x1a),
    rgb(0xb7, 0xb5, 0xac),
    rgb(0xce, 0x5d, 0x03),
    rgb(0x69, 0x68, 0x62),
    rgb(0x2e, 0x2d, 0x2c),
    rgb(0x66, 0x80, 0x3b),
    rgb(0xce, 0x5d, 0x03),
    rgb(0xce, 0x3e, 0x3e),
    rgb(0x28, 0x5f, 0x8f),
).with_background_panel(rgb(0x14, 0x14, 0x13))
 .with_background_element(rgb(0x24, 0x23, 0x22))
 .with_syntax(
    rgb(0x69, 0x68, 0x62),
    rgb(0xce, 0x3e, 0x3e),
    rgb(0x28, 0x5f, 0x8f),
    rgb(0xb7, 0xb5, 0xac),
    rgb(0x66, 0x80, 0x3b),
    rgb(0xce, 0x5d, 0x03),
);

/// Gruvbox — background: #282828, accent: #d79921
pub const THEME_GRUVBOX: Theme = Theme::derive(
    "gruvbox", ThemeMode::Dark,
    rgb(0x28, 0x28, 0x28),
    rgb(0xeb, 0xdb, 0xb2),
    rgb(0xd7, 0x99, 0x21),
    rgb(0x92, 0x83, 0x74),
    rgb(0x3c, 0x38, 0x36),
    rgb(0x98, 0x9a, 0x1a),
    rgb(0xd7, 0x99, 0x21),
    rgb(0xcc, 0x24, 0x1d),
    rgb(0x45, 0x85, 0x88),
).with_background_panel(rgb(0x1d, 0x20, 0x21))
 .with_background_element(rgb(0x32, 0x30, 0x2f))
 .with_syntax(
    rgb(0x92, 0x83, 0x74),
    rgb(0xd3, 0x86, 0x9b),
    rgb(0x45, 0x85, 0x88),
    rgb(0xeb, 0xdb, 0xb2),
    rgb(0x98, 0x9a, 0x1a),
    rgb(0xd3, 0x86, 0x9b),
);

/// Kanagawa — background: #1f1f28, accent: #7fb4ca
pub const THEME_KANAGAWA: Theme = Theme::derive(
    "kanagawa", ThemeMode::Dark,
    rgb(0x1f, 0x1f, 0x28),
    rgb(0xd5, 0xcd, 0xb2),
    rgb(0x7f, 0xb4, 0xca),
    rgb(0x59, 0x5e, 0x6f),
    rgb(0x2d, 0x2e, 0x3b),
    rgb(0x76, 0x9a, 0x6c),
    rgb(0xe6, 0xa5, 0x5e),
    rgb(0xe4, 0x68, 0x6c),
    rgb(0x7f, 0xb4, 0xca),
).with_primary(rgb(0xd5, 0xcd, 0xb2))
 .with_secondary(rgb(0xe4, 0x68, 0x6c))
 .with_background_panel(rgb(0x18, 0x18, 0x20))
 .with_background_element(rgb(0x26, 0x26, 0x33))
 .with_syntax(
    rgb(0x59, 0x5e, 0x6f),
    rgb(0xe4, 0x68, 0x6c),
    rgb(0x7f, 0xb4, 0xca),
    rgb(0xd5, 0xcd, 0xb2),
    rgb(0x76, 0x9a, 0x6c),
    rgb(0xe6, 0xa5, 0x5e),
);

/// Lucent Orng — background: #1c1c1c, accent: #ff6b35
pub const THEME_LUCENT_ORNG: Theme = Theme::derive(
    "lucent-orng", ThemeMode::Dark,
    rgb(0x1c, 0x1c, 0x1c),
    rgb(0xf0, 0xf0, 0xf0),
    rgb(0xff, 0x6b, 0x35),
    rgb(0x88, 0x88, 0x88),
    rgb(0x33, 0x33, 0x33),
    rgb(0x4c, 0xaf, 0x50),
    rgb(0xff, 0xab, 0x35),
    rgb(0xf4, 0x43, 0x36),
    rgb(0x42, 0xa5, 0xf5),
).with_background_panel(rgb(0x25, 0x25, 0x25))
 .with_background_element(rgb(0x30, 0x30, 0x30))
 .with_syntax(
    rgb(0x88, 0x88, 0x88),
    rgb(0xff, 0x6b, 0x35),
    rgb(0x42, 0xa5, 0xf5),
    rgb(0xf0, 0xf0, 0xf0),
    rgb(0x4c, 0xaf, 0x50),
    rgb(0xff, 0xab, 0x35),
);

/// Material — background: #1e1e2e, accent: #89b4fa
pub const THEME_MATERIAL: Theme = Theme::derive(
    "material", ThemeMode::Dark,
    rgb(0x1e, 0x1e, 0x2e),
    rgb(0xcd, 0xd6, 0xf4),
    rgb(0x89, 0xb4, 0xfa),
    rgb(0x6c, 0x70, 0x86),
    rgb(0x31, 0x32, 0x44),
    rgb(0xa6, 0xe3, 0xa1),
    rgb(0xfa, 0xe3, 0xb0),
    rgb(0xf3, 0x8b, 0xa8),
    rgb(0x89, 0xbe, 0xbf),
).with_background_panel(rgb(0x18, 0x18, 0x26))
 .with_background_element(rgb(0x25, 0x26, 0x37))
 .with_syntax(
    rgb(0x6c, 0x70, 0x86),
    rgb(0xcb, 0xa6, 0xf7),
    rgb(0x89, 0xb4, 0xfa),
    rgb(0xcd, 0xd6, 0xf4),
    rgb(0xa6, 0xe3, 0xa1),
    rgb(0xfa, 0xe3, 0xb0),
);

/// Matrix — background: #000000, accent: #00ff41
pub const THEME_MATRIX: Theme = Theme::derive(
    "matrix", ThemeMode::Dark,
    rgb(0x00, 0x00, 0x00),
    rgb(0x00, 0xff, 0x41),
    rgb(0x00, 0xff, 0x41),
    rgb(0x00, 0x80, 0x20),
    rgb(0x00, 0x33, 0x0d),
    rgb(0x00, 0xff, 0x41),
    rgb(0x00, 0xcc, 0x33),
    rgb(0xff, 0x00, 0x00),
    rgb(0x00, 0xff, 0x41),
).with_primary(rgb(0x00, 0xff, 0x41))
 .with_secondary(rgb(0x00, 0x80, 0x20))
 .with_background_panel(rgb(0x00, 0x0a, 0x00))
 .with_background_element(rgb(0x00, 0x15, 0x00))
 .with_border(rgb(0x00, 0x80, 0x20))
 .with_border_subtle(rgb(0x00, 0x33, 0x0d))
 .with_text_muted(rgb(0x00, 0x80, 0x20))
 .with_diff(rgb(0x00, 0xff, 0x41), rgb(0xff, 0x00, 0x00), rgb(0x00, 0x80, 0x20))
 .with_syntax(
    rgb(0x00, 0x80, 0x20),
    rgb(0x00, 0xff, 0x41),
    rgb(0x00, 0xff, 0x41),
    rgb(0x00, 0xff, 0x41),
    rgb(0x00, 0xff, 0x41),
    rgb(0x00, 0xff, 0x41),
);

/// Mercury — background: #1a1a1a, accent: #6c8fbf
pub const THEME_MERCURY: Theme = Theme::derive(
    "mercury", ThemeMode::Dark,
    rgb(0x1a, 0x1a, 0x1a),
    rgb(0xcd, 0xcd, 0xcd),
    rgb(0x6c, 0x8f, 0xbf),
    rgb(0x80, 0x80, 0x80),
    rgb(0x2a, 0x2a, 0x2a),
    rgb(0x6c, 0xbf, 0x8f),
    rgb(0xbf, 0x9f, 0x6c),
    rgb(0xbf, 0x6c, 0x6c),
    rgb(0x6c, 0x9f, 0xbf),
).with_background_panel(rgb(0x22, 0x22, 0x22))
 .with_background_element(rgb(0x2d, 0x2d, 0x2d))
 .with_syntax(
    rgb(0x80, 0x80, 0x80),
    rgb(0xbf, 0x6c, 0xbf),
    rgb(0x6c, 0x8f, 0xbf),
    rgb(0xcd, 0xcd, 0xcd),
    rgb(0x6c, 0xbf, 0x8f),
    rgb(0xbf, 0x9f, 0x6c),
);

/// Night Owl — background: #f8f8f2, accent: #7e57c2 (light theme)
pub const THEME_NIGHTOWL: Theme = Theme::derive(
    "nightowl", ThemeMode::Light,
    rgb(0xf8, 0xf8, 0xf2),
    rgb(0x40, 0x3e, 0x53),
    rgb(0x7e, 0x57, 0xc2),
    rgb(0xa0, 0x9b, 0xbb),
    rgb(0xe0, 0xde, 0xd6),
    rgb(0x5f, 0x9e, 0x6f),
    rgb(0xc9, 0x6b, 0x2e),
    rgb(0xc0, 0x4d, 0x4d),
    rgb(0x3a, 0x7e, 0xbf),
).with_primary(rgb(0x7e, 0x57, 0xc2))
 .with_secondary(rgb(0xc0, 0x4d, 0x4d))
 .with_background_panel(rgb(0xef, 0xed, 0xe6))
 .with_background_element(rgb(0xe6, 0xe4, 0xda))
 .with_syntax(
    rgb(0xa0, 0x9b, 0xbb),
    rgb(0x7e, 0x57, 0xc2),
    rgb(0x3a, 0x7e, 0xbf),
    rgb(0x40, 0x3e, 0x53),
    rgb(0x5f, 0x9e, 0x6f),
    rgb(0xc9, 0x6b, 0x2e),
);

/// One Dark — background: #282c34, accent: #61afef
pub const THEME_ONE_DARK: Theme = Theme::derive(
    "one-dark", ThemeMode::Dark,
    rgb(0x28, 0x2c, 0x34),
    rgb(0xab, 0xb2, 0xbf),
    rgb(0x61, 0xaf, 0xef),
    rgb(0x5c, 0x63, 0x70),
    rgb(0x3e, 0x43, 0x4a),
    rgb(0x98, 0xc3, 0x79),
    rgb(0xe5, 0xc0, 0x7b),
    rgb(0xe0, 0x6c, 0x75),
    rgb(0x61, 0xaf, 0xef),
).with_primary(rgb(0x61, 0xaf, 0xef))
 .with_secondary(rgb(0xc6, 0x78, 0xdd))
 .with_background_panel(rgb(0x21, 0x25, 0x2b))
 .with_background_element(rgb(0x35, 0x39, 0x40))
 .with_syntax(
    rgb(0x5c, 0x63, 0x70),
    rgb(0xc6, 0x78, 0xdd),
    rgb(0x61, 0xaf, 0xef),
    rgb(0xab, 0xb2, 0xbf),
    rgb(0x98, 0xc3, 0x79),
    rgb(0xe5, 0xc0, 0x7b),
);

/// Blazecode — Opencode-matching theme
/// background: #0a0a0a, accent: #fab283, text: #eeeeee
pub const THEME_BLAZECODE: Theme = Theme::derive(
    "blazecode", ThemeMode::Dark,
    rgb(0x0a, 0x0a, 0x0a),  // background
    rgb(0xee, 0xee, 0xee),  // foreground / text
    rgb(0xfa, 0xb2, 0x83),  // accent / primary
    rgb(0x80, 0x80, 0x80),  // dim / text_muted
    rgb(0x48, 0x48, 0x48),  // border
    rgb(0x7f, 0xd8, 0x8f),  // success
    rgb(0xf5, 0xa7, 0x42),  // warning
    rgb(0xe0, 0x6c, 0x75),  // error
    rgb(0x56, 0xb6, 0xc2),  // info
).with_background_panel(rgb(0x14, 0x14, 0x14))
 .with_background_element(rgb(0x1e, 0x1e, 0x1e))
 .with_background_menu(rgb(0x14, 0x14, 0x14))
 .with_border_active(rgb(0xfa, 0xb2, 0x83))
 .with_border_subtle(rgb(0x60, 0x60, 0x60))
 .with_primary(rgb(0xfa, 0xb2, 0x83))
 .with_secondary(rgb(0x5c, 0x9c, 0xf5))
 .with_syntax(
    rgb(0x80, 0x80, 0x80),  // comment
    rgb(0x9d, 0x7c, 0xd8),  // keyword (purple)
    rgb(0xfa, 0xb2, 0x83),  // function (orange)
    rgb(0xee, 0xee, 0xee),  // variable (white)
    rgb(0x7f, 0xd8, 0x8f),  // string (green)
    rgb(0xf5, 0xa7, 0x42),  // number (orange)
);

/// Orng — background: #1a1a1a, accent: #ff8c00
pub const THEME_ORNG: Theme = Theme::derive(
    "orng", ThemeMode::Dark,
    rgb(0x1a, 0x1a, 0x1a),
    rgb(0xf0, 0xf0, 0xf0),
    rgb(0xff, 0x8c, 0x00),
    rgb(0x80, 0x80, 0x80),
    rgb(0x33, 0x33, 0x33),
    rgb(0x4c, 0xaf, 0x50),
    rgb(0xff, 0xaa, 0x33),
    rgb(0xf4, 0x43, 0x36),
    rgb(0x42, 0xa5, 0xf5),
).with_background_panel(rgb(0x25, 0x25, 0x25))
 .with_background_element(rgb(0x30, 0x30, 0x30))
 .with_syntax(
    rgb(0x80, 0x80, 0x80),
    rgb(0xff, 0x8c, 0x00),
    rgb(0x42, 0xa5, 0xf5),
    rgb(0xf0, 0xf0, 0xf0),
    rgb(0x4c, 0xaf, 0x50),
    rgb(0xff, 0xaa, 0x33),
);

/// Osaka Jade — background: #1b1e2b, accent: #7bc99d
pub const THEME_OSAKA_JADE: Theme = Theme::derive(
    "osaka-jade", ThemeMode::Dark,
    rgb(0x1b, 0x1e, 0x2b),
    rgb(0xd8, 0xde, 0xe9),
    rgb(0x7b, 0xc9, 0x9d),
    rgb(0x66, 0x6e, 0x80),
    rgb(0x2a, 0x2e, 0x3e),
    rgb(0x7b, 0xc9, 0x9d),
    rgb(0xe0, 0xaf, 0x68),
    rgb(0xe6, 0x6a, 0x7a),
    rgb(0x6f, 0xaa, 0xd3),
).with_primary(rgb(0x6f, 0xaa, 0xd3))
 .with_background_panel(rgb(0x15, 0x17, 0x22))
 .with_background_element(rgb(0x22, 0x25, 0x35))
 .with_syntax(
    rgb(0x66, 0x6e, 0x80),
    rgb(0xe6, 0x6a, 0x7a),
    rgb(0x6f, 0xaa, 0xd3),
    rgb(0xd8, 0xde, 0xe9),
    rgb(0x7b, 0xc9, 0x9d),
    rgb(0xe0, 0xaf, 0x68),
);

/// Palenight — background: #292d3e, accent: #82aaff
pub const THEME_PALENIGHT: Theme = Theme::derive(
    "palenight", ThemeMode::Dark,
    rgb(0x29, 0x2d, 0x3e),
    rgb(0x95, 0x9d, 0xc8),
    rgb(0x82, 0xaa, 0xff),
    rgb(0x67, 0x6e, 0x95),
    rgb(0x34, 0x38, 0x4d),
    rgb(0x89, 0xdd, 0x78),
    rgb(0xff, 0xc7, 0x77),
    rgb(0xf0, 0x7f, 0x7f),
    rgb(0x82, 0xaa, 0xff),
).with_primary(rgb(0x95, 0x9d, 0xc8))
 .with_secondary(rgb(0xc0, 0x92, 0xd5))
 .with_background_panel(rgb(0x22, 0x25, 0x35))
 .with_background_element(rgb(0x32, 0x36, 0x48))
 .with_syntax(
    rgb(0x67, 0x6e, 0x95),
    rgb(0xc0, 0x92, 0xd5),
    rgb(0x82, 0xaa, 0xff),
    rgb(0x95, 0x9d, 0xc8),
    rgb(0x89, 0xdd, 0x78),
    rgb(0xff, 0xc7, 0x77),
);

/// Rosé Pine — background: #191724, accent: #ebbcba
pub const THEME_ROSEPINE: Theme = Theme::derive(
    "rosepine", ThemeMode::Dark,
    rgb(0x19, 0x17, 0x24),
    rgb(0xe0, 0xde, 0xf4),
    rgb(0xeb, 0xbc, 0xba),
    rgb(0x6e, 0x6a, 0x86),
    rgb(0x2a, 0x27, 0x3e),
    rgb(0x31, 0x7f, 0x6f),
    rgb(0xf6, 0xc1, 0x77),
    rgb(0xeb, 0x6f, 0x92),
    rgb(0x9c, 0xcf, 0xd8),
).with_primary(rgb(0x9c, 0xcf, 0xd8))
 .with_secondary(rgb(0xc4, 0xa7, 0xe7))
 .with_background_panel(rgb(0x1f, 0x1d, 0x2e))
 .with_background_element(rgb(0x26, 0x24, 0x3a))
 .with_syntax(
    rgb(0x6e, 0x6a, 0x86),
    rgb(0xeb, 0x6f, 0x92),
    rgb(0x9c, 0xcf, 0xd8),
    rgb(0xe0, 0xde, 0xf4),
    rgb(0x31, 0x7f, 0x6f),
    rgb(0xf6, 0xc1, 0x77),
);

/// Synthwave '84 — background: #262335, accent: #ff7edb
pub const THEME_SYNTHWAVE84: Theme = Theme::derive(
    "synthwave84", ThemeMode::Dark,
    rgb(0x26, 0x23, 0x35),
    rgb(0xbf, 0xb9, 0xdb),
    rgb(0xff, 0x7e, 0xdb),
    rgb(0x6b, 0x63, 0x7d),
    rgb(0x32, 0x2d, 0x45),
    rgb(0x16, 0xe3, 0xc0),
    rgb(0xff, 0x7e, 0xdb),
    rgb(0xfe, 0x44, 0x50),
    rgb(0x36, 0xf9, 0xf6),
).with_primary(rgb(0x36, 0xf9, 0xf6))
 .with_background_panel(rgb(0x1e, 0x1c, 0x2c))
 .with_background_element(rgb(0x2e, 0x29, 0x40))
 .with_syntax(
    rgb(0x6b, 0x63, 0x7d),
    rgb(0xff, 0x7e, 0xdb),
    rgb(0x36, 0xf9, 0xf6),
    rgb(0xbf, 0xb9, 0xdb),
    rgb(0x16, 0xe3, 0xc0),
    rgb(0xfe, 0x44, 0x50),
);

/// Vercel — background: #ffffff, accent: #000000 (light theme)
pub const THEME_VERCEL: Theme = Theme::derive(
    "vercel", ThemeMode::Light,
    rgb(0xff, 0xff, 0xff),
    rgb(0x00, 0x00, 0x00),
    rgb(0x00, 0x00, 0x00),
    rgb(0x99, 0x99, 0x99),
    rgb(0xe0, 0xe0, 0xe0),
    rgb(0x00, 0x77, 0x3b),
    rgb(0xbf, 0x77, 0x00),
    rgb(0xe0, 0x00, 0x00),
    rgb(0x00, 0x55, 0xcc),
).with_background_panel(rgb(0xf8, 0xf8, 0xf8))
 .with_background_element(rgb(0xf0, 0xf0, 0xf0))
 .with_syntax(
    rgb(0x99, 0x99, 0x99),
    rgb(0x00, 0x00, 0x00),
    rgb(0x00, 0x55, 0xcc),
    rgb(0x00, 0x00, 0x00),
    rgb(0x00, 0x77, 0x3b),
    rgb(0xbf, 0x77, 0x00),
);

/// Vesper — background: #101010, accent: #c0a36e
pub const THEME_VESPER: Theme = Theme::derive(
    "vesper", ThemeMode::Dark,
    rgb(0x10, 0x10, 0x10),
    rgb(0xc0, 0xbf, 0xb5),
    rgb(0xc0, 0xa3, 0x6e),
    rgb(0x58, 0x57, 0x51),
    rgb(0x22, 0x22, 0x20),
    rgb(0x7c, 0x8c, 0x58),
    rgb(0xc0, 0xa3, 0x6e),
    rgb(0xa3, 0x48, 0x3a),
    rgb(0x5e, 0x82, 0x9c),
).with_background_panel(rgb(0x18, 0x18, 0x18))
 .with_background_element(rgb(0x25, 0x25, 0x25))
 .with_syntax(
    rgb(0x58, 0x57, 0x51),
    rgb(0xc0, 0xa3, 0x6e),
    rgb(0x5e, 0x82, 0x9c),
    rgb(0xc0, 0xbf, 0xb5),
    rgb(0x7c, 0x8c, 0x58),
    rgb(0xc0, 0xa3, 0x6e),
);

/// Zenburn — background: #3f3f3f, accent: #dcdccc
pub const THEME_ZENBURN: Theme = Theme::derive(
    "zenburn", ThemeMode::Dark,
    rgb(0x3f, 0x3f, 0x3f),
    rgb(0xd5, 0xd5, 0xbc),
    rgb(0xdc, 0xdc, 0xcc),
    rgb(0x70, 0x70, 0x70),
    rgb(0x50, 0x50, 0x50),
    rgb(0x7f, 0x9f, 0x7f),
    rgb(0xef, 0xef, 0xaf),
    rgb(0xd0, 0x5b, 0x5b),
    rgb(0x8c, 0xd0, 0xd3),
).with_background_panel(rgb(0x37, 0x37, 0x37))
 .with_background_element(rgb(0x48, 0x48, 0x48))
 .with_syntax(
    rgb(0x70, 0x70, 0x70),
    rgb(0xd0, 0x5b, 0x5b),
    rgb(0x8c, 0xd0, 0xd3),
    rgb(0xd5, 0xd5, 0xbc),
    rgb(0x7f, 0x9f, 0x7f),
    rgb(0xef, 0xef, 0xaf),
);

// ── All built-in themes ────────────────────────────────────────────────────

/// All 35 built-in themes in display order.
pub const ALL_THEMES: &[&Theme] = &[
    &THEME_DARK,
    &THEME_LIGHT,
    &THEME_DRACULA,
    &THEME_MONOKAI,
    &THEME_NORD,
    &THEME_SOLARIZED,
    &THEME_GITHUB,
    &THEME_TOKYONIGHT,
    &THEME_AURA,
    &THEME_AYU,
    &THEME_CARBONFOX,
    &THEME_CATPPUCCIN,
    &THEME_CATPPUCCIN_FRAPPE,
    &THEME_CATPPUCCIN_MACCHIATO,
    &THEME_COBALT2,
    &THEME_CURSOR,
    &THEME_EVERFOREST,
    &THEME_FLEXOKI,
    &THEME_GRUVBOX,
    &THEME_KANAGAWA,
    &THEME_LUCENT_ORNG,
    &THEME_MATERIAL,
    &THEME_MATRIX,
    &THEME_MERCURY,
    &THEME_NIGHTOWL,
    &THEME_ONE_DARK,
    &THEME_BLAZECODE,
    &THEME_ORNG,
    &THEME_OSAKA_JADE,
    &THEME_PALENIGHT,
    &THEME_ROSEPINE,
    &THEME_SYNTHWAVE84,
    &THEME_VERCEL,
    &THEME_VESPER,
    &THEME_ZENBURN,
];

// ── Theme lookup by name (including custom loaded themes) ─────────────────

/// A collection of themes: built-in + custom loaded from disk.
#[derive(Debug, Clone)]
pub struct ThemeCollection {
    /// Built-in themes keyed by name.
    builtins: HashMap<&'static str, &'static Theme>,
    /// Custom themes loaded from disk, keyed by name.
    custom: HashMap<String, Theme>,
    /// Ordered list of all theme names.
    names_order: Vec<String>,
}

impl Default for ThemeCollection {
    fn default() -> Self {
        Self::new()
    }
}

impl ThemeCollection {
    /// Create a new collection with all built-in themes.
    pub fn new() -> Self {
        let mut builtins = HashMap::new();
        let mut names_order = Vec::new();
        for theme in ALL_THEMES {
            builtins.insert(theme.name, *theme);
            names_order.push(theme.name.to_string());
        }
        Self {
            builtins,
            custom: HashMap::new(),
            names_order,
        }
    }

    /// Get a theme by name. Checks custom themes first, then built-ins.
    pub fn get(&self, name: &str) -> Option<&Theme> {
        if let Some(custom) = self.custom.get(name) {
            return Some(custom);
        }
        self.builtins.get(name).copied()
    }

    /// Get a theme by index (for cycling).
    pub fn get_by_index(&self, index: usize) -> Option<&Theme> {
        let name = self.names_order.get(index)?;
        self.get(name)
    }

    /// Number of themes (built-in + custom).
    pub fn len(&self) -> usize {
        self.names_order.len()
    }

    pub fn is_empty(&self) -> bool {
        self.names_order.is_empty()
    }

    /// Add a custom theme.
    pub fn add_custom(&mut self, name: String, theme: Theme) {
        if !self.names_order.contains(&name) {
            self.names_order.push(name.clone());
        }
        self.custom.insert(name, theme);
    }

    /// List all theme names in order.
    pub fn names(&self) -> &[String] {
        &self.names_order
    }

    /// Check if a theme exists.
    pub fn has(&self, name: &str) -> bool {
        self.builtins.contains_key(name) || self.custom.contains_key(name)
    }

    /// Find the index of a theme by name.
    pub fn index_of(&self, name: &str) -> Option<usize> {
        self.names_order.iter().position(|n| n == name)
    }
}

// ── Custom theme loading ─────────────────────────────────────────────────

/// Load custom themes from `~/.config/blazecode/themes/*.json`.
///
/// Each JSON file should follow the same schema as the built-in theme
/// assets (see `packages/tui/src/theme/assets/`).
pub fn load_custom_themes() -> Vec<(String, Theme)> {
    let config_dir = dirs_or_default();
    let themes_dir = config_dir.join("themes");

    if !themes_dir.is_dir() {
        return Vec::new();
    }

    let mut themes = Vec::new();
    if let Ok(entries) = std::fs::read_dir(&themes_dir) {
        for entry in entries.flatten() {
            let path = entry.path();
            if path.extension().and_then(|e| e.to_str()) != Some("json") {
                continue;
            }
            let path_str = path.to_string_lossy().to_string();
            match load_theme_from_file(std::path::Path::new(&path_str)) {
                Ok(Some(theme)) => {
                    let name = path
                        .file_stem()
                        .and_then(|s| s.to_str())
                        .unwrap_or("unknown")
                        .to_string();
                    themes.push((name, theme));
                }
                Ok(None) => {} // skipped
                Err(e) => {
                    tracing::warn!("failed to load custom theme {path_str}: {e}");
                }
            }
        }
    }
    themes
}

fn dirs_or_default() -> PathBuf {
    std::env::var("HOME")
        .ok()
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from("~"))
        .join(".config")
        .join("blazecode")
}

fn load_theme_from_file(path: &std::path::Path) -> Result<Option<Theme>, Box<dyn std::error::Error>> {
    let content = std::fs::read_to_string(path)?;
    let json: serde_json::Value = serde_json::from_str(&content)?;

    let theme_obj = json.get("theme").and_then(|v| v.as_object());
    let theme_obj = match theme_obj {
        Some(o) => o,
        None => return Ok(None),
    };

    let defs = json
        .get("defs")
        .and_then(|v| v.as_object())
        .map(|d| {
            d.iter()
                .map(|(k, v)| (k.clone(), parse_hex_color(v)))
                .filter(|(_, c)| c.is_some())
                .map(|(k, c)| (k, c.unwrap()))
                .collect::<HashMap<_, _>>()
        })
        .unwrap_or_default();

    let resolve = |key: &str| -> Option<Color> {
        let val = theme_obj.get(key)?;
        resolve_color_value(val, &defs)
    };

    let name = path
        .file_stem()
        .and_then(|s| s.to_str())
        .map(|s| s.to_string())
        .unwrap_or_else(|| "custom".to_string());
    let name: &'static str = Box::leak(name.into_boxed_str());
    let bg = resolve("background").unwrap_or(rgb(0x1a, 0x1b, 0x26));
    let fg = resolve("text").or_else(|| resolve("foreground")).unwrap_or(rgb(0xc0, 0xca, 0xf5));
    let accent = resolve("accent").unwrap_or(rgb(0x7a, 0xa2, 0xf7));

    // Detect mode from background luminance
    let luminance = color_luminance(&bg);
    let mode = if luminance > 0.5 { ThemeMode::Light } else { ThemeMode::Dark };

    let mut theme = Theme::derive(name, mode, bg, fg, accent,
        resolve("textMuted").or_else(|| resolve("dim")).unwrap_or(rgb(0x56, 0x5f, 0x89)),
        resolve("border").unwrap_or(rgb(0x29, 0x2e, 0x42)),
        resolve("success").unwrap_or(rgb(0x9e, 0xce, 0x6a)),
        resolve("warning").unwrap_or(rgb(0xe0, 0xaf, 0x68)),
        resolve("error").unwrap_or(rgb(0xf7, 0x76, 0x8e)),
        resolve("info").unwrap_or(rgb(0x7d, 0xcf, 0xff)),
    );

    if let Some(c) = resolve("primary") { theme = theme.with_primary(c); }
    if let Some(c) = resolve("secondary") { theme = theme.with_secondary(c); }
    if let Some(c) = resolve("backgroundPanel") { theme = theme.with_background_panel(c); }
    if let Some(c) = resolve("backgroundElement") { theme = theme.with_background_element(c); }
    if let Some(c) = resolve("backgroundMenu") { theme = theme.with_background_menu(c); }
    if let Some(c) = resolve("borderActive") { theme = theme.with_border_active(c); }
    if let Some(c) = resolve("borderSubtle") { theme = theme.with_border_subtle(c); }
    if let Some(c) = resolve("textMuted") { theme = theme.with_text_muted(c); }
    if let Some(c) = resolve("selectedListItemText") { theme = theme.with_selected_list_item_text(c); }

    if let Some(v) = theme_obj.get("thinkingOpacity").and_then(|v| v.as_f64()) {
        theme = theme.with_thinking_opacity(v);
    }

    Ok(Some(theme))
}

fn parse_hex_color(v: &serde_json::Value) -> Option<Color> {
    let s = v.as_str()?;
    let s = s.trim_start_matches('#');
    if s.len() == 6 {
        let r = u8::from_str_radix(&s[0..2], 16).ok()?;
        let g = u8::from_str_radix(&s[2..4], 16).ok()?;
        let b = u8::from_str_radix(&s[4..6], 16).ok()?;
        Some(rgb(r, g, b))
    } else {
        None
    }
}

fn resolve_color_value(val: &serde_json::Value, defs: &HashMap<String, Color>) -> Option<Color> {
    match val {
        serde_json::Value::String(s) => {
            if s.starts_with('#') {
                parse_hex_color(val)
            } else {
                defs.get(s.as_str()).copied()
            }
        }
        serde_json::Value::Object(map) => {
            // Dark/light variant object: { "dark": "...", "light": "..." }
            // We use "dark" by default
            map.get("dark")
                .or_else(|| map.get("light"))
                .and_then(|v| resolve_color_value(v, defs))
        }
        _ => None,
    }
}

fn color_luminance(c: &Color) -> f64 {
    match c {
        Color::Rgb(r, g, b) => {
            0.299 * *r as f64 / 255.0 + 0.587 * *g as f64 / 255.0 + 0.114 * *b as f64 / 255.0
        }
        _ => 0.5,
    }
}

// ── Theme State ──────────────────────────────────────────────────────────────

/// Persistent theme state managed by the TUI app.
#[derive(Debug, Clone)]
pub struct ThemeState {
    /// Index into the theme collection for the currently active theme.
    theme_index: usize,
    /// Whether the theme has been locked (prevents cycling/switching).
    locked: bool,
    /// Collection of all available themes.
    collection: ThemeCollection,
}

impl Default for ThemeState {
    fn default() -> Self {
        let collection = ThemeCollection::new();
        Self {
            theme_index: 0,
            locked: false,
            collection,
        }
    }
}

impl ThemeState {
    /// Create a new ThemeState with the default (dark) theme.
    pub fn new() -> Self {
        let mut state = Self::default();
        // Load custom themes from disk
        let custom = load_custom_themes();
        for (name, theme) in custom {
            state.collection.add_custom(name, theme);
        }
        state
    }

    /// Get the current theme.
    pub fn current(&self) -> &Theme {
        self.collection
            .get_by_index(self.theme_index)
            .unwrap_or_else(|| &THEME_DARK)
    }

    /// Get the current theme name.
    pub fn name(&self) -> &str {
        self.current().name
    }

    /// Get the current theme mode (dark/light).
    pub fn mode(&self) -> ThemeMode {
        self.current().mode
    }

    /// Whether the theme is locked.
    pub fn is_locked(&self) -> bool {
        self.locked
    }

    /// Lock the theme (prevents switching).
    pub fn lock(&mut self) {
        self.locked = true;
    }

    /// Unlock the theme (allows switching).
    pub fn unlock(&mut self) {
        self.locked = false;
    }

    /// Toggle the lock state.
    pub fn toggle_lock(&mut self) {
        self.locked = !self.locked;
    }

    /// Switch to a theme by name. Returns true if the theme was found and applied.
    pub fn switch_theme(&mut self, name: &str) -> bool {
        if self.locked {
            return false;
        }
        if let Some(idx) = self.collection.index_of(name) {
            self.theme_index = idx;
            true
        } else {
            false
        }
    }

    /// Cycle themes forward (+1) or backward (-1).
    /// Returns the name of the new theme.
    pub fn cycle_theme(&mut self, direction: i32) -> Option<&str> {
        if self.locked {
            return None;
        }
        let len = self.collection.len() as i32;
        let new_index = (self.theme_index as i32 + direction).rem_euclid(len) as usize;
        self.theme_index = new_index;
        Some(self.current().name)
    }

    /// Toggle between dark and light mode.
    /// Finds the next theme of the opposite mode.
    /// Returns the name of the new theme.
    pub fn toggle_mode(&mut self) -> Option<&str> {
        if self.locked {
            return None;
        }
        let target_mode = self.current().mode.invert();
        let len = self.collection.len();
        for offset in 1..len {
            let idx = (self.theme_index + offset) % len;
            if let Some(theme) = self.collection.get_by_index(idx) {
                if theme.mode == target_mode {
                    self.theme_index = idx;
                    return Some(self.current().name);
                }
            }
        }
        // Fallback: find first theme of target mode
        for idx in 0..len {
            if let Some(theme) = self.collection.get_by_index(idx) {
                if theme.mode == target_mode {
                    self.theme_index = idx;
                    return Some(self.current().name);
                }
            }
        }
        None
    }

    /// Get the list of all theme names.
    pub fn theme_names(&self) -> Vec<&str> {
        self.collection.names().iter().map(|s| s.as_str()).collect()
    }

    /// Get a reference to the theme collection.
    pub fn collection(&self) -> &ThemeCollection {
        &self.collection
    }

    /// Get a mutable reference to the theme collection.
    pub fn collection_mut(&mut self) -> &mut ThemeCollection {
        &mut self.collection
    }

    /// Add a theme from a plugin.
    pub fn add_plugin_theme(&mut self, name: String, theme: Theme) {
        self.collection.add_custom(name, theme);
    }
}

// ── Theme impl helpers (used by const declarations above) ───────────────

impl Theme {
    pub const fn with_diff_hunk_header_set(mut self, color: Color) -> Self {
        self.diff_hunk_header = color;
        self
    }

    pub const fn with_diff_line_number(mut self, color: Color) -> Self {
        self.diff_line_number = color;
        self
    }

    pub const fn with_diff_line_number_bg(mut self, added: Color, removed: Color) -> Self {
        self.diff_added_line_number_bg = added;
        self.diff_removed_line_number_bg = removed;
        self
    }
}

// ── Tests ────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_default_is_dark() {
        let state = ThemeState::new();
        assert_eq!(state.name(), "dark");
        assert_eq!(state.mode(), ThemeMode::Dark);
    }

    #[test]
    fn test_switch_theme() {
        let mut state = ThemeState::new();
        assert!(state.switch_theme("dracula"));
        assert_eq!(state.name(), "dracula");
        assert!(!state.switch_theme("nonexistent"));
        assert_eq!(state.name(), "dracula");
    }

    #[test]
    fn test_cycle_forward() {
        let mut state = ThemeState::new();
        assert_eq!(state.name(), "dark");
        state.cycle_theme(1);
        assert_eq!(state.name(), "light");
        state.cycle_theme(1);
        assert_eq!(state.name(), "dracula");
    }

    #[test]
    fn test_cycle_wraps() {
        let mut state = ThemeState::new();
        assert_eq!(state.cycle_theme(-1), Some("zenburn"));
    }

    #[test]
    fn test_toggle_mode() {
        let mut state = ThemeState::new();
        assert_eq!(state.mode(), ThemeMode::Dark);
        let new_name = state.toggle_mode();
        assert!(new_name.is_some());
        assert_eq!(state.mode(), ThemeMode::Light);
    }

    #[test]
    fn test_lock_prevents_switch() {
        let mut state = ThemeState::new();
        state.lock();
        assert!(!state.switch_theme("dracula"));
        assert_eq!(state.name(), "dark");
        assert_eq!(state.cycle_theme(1), None);
        assert_eq!(state.toggle_mode(), None);
    }

    #[test]
    fn test_unlock_allows_switch() {
        let mut state = ThemeState::new();
        state.lock();
        state.unlock();
        assert!(state.switch_theme("nord"));
        assert_eq!(state.name(), "nord");
    }

    #[test]
    fn test_all_themes_have_names() {
        for theme in ALL_THEMES {
            assert!(!theme.name.is_empty());
        }
        let names: Vec<&str> = ALL_THEMES.iter().map(|t| t.name).collect();
        let mut unique = names.clone();
        unique.sort();
        unique.dedup();
        assert_eq!(unique.len(), names.len());
    }

    #[test]
    fn test_thirty_five_themes() {
        assert_eq!(ALL_THEMES.len(), 35);
    }

    #[test]
    fn test_dark_and_light_modes_present() {
        let has_dark = ALL_THEMES.iter().any(|t| t.mode == ThemeMode::Dark);
        let has_light = ALL_THEMES.iter().any(|t| t.mode == ThemeMode::Light);
        assert!(has_dark);
        assert!(has_light);
    }

    #[test]
    fn test_theme_names() {
        let state = ThemeState::new();
        let names = state.theme_names();
        assert_eq!(names.len(), 35);
        assert!(names.contains(&"dark"));
        assert!(names.contains(&"dracula"));
        assert!(names.contains(&"tokyonight"));
        assert!(names.contains(&"catppuccin"));
        assert!(names.contains(&"gruvbox"));
        assert!(names.contains(&"rosepine"));
    }

    #[test]
    fn test_toggle_lock() {
        let mut state = ThemeState::new();
        assert!(!state.is_locked());
        state.toggle_lock();
        assert!(state.is_locked());
        state.toggle_lock();
        assert!(!state.is_locked());
    }

    #[test]
    fn test_theme_collection() {
        let coll = ThemeCollection::new();
        assert_eq!(coll.len(), 35);
        assert!(coll.has("dark"));
        assert!(coll.has("tokyonight"));
        assert!(!coll.has("nonexistent"));
    }

    #[test]
    fn test_theme_collection_add_custom() {
        let mut coll = ThemeCollection::new();
        let len_before = coll.len();
        coll.add_custom("my-theme".into(), THEME_DARK.clone());
        assert_eq!(coll.len(), len_before + 1);
        assert!(coll.has("my-theme"));
    }

    #[test]
    fn test_theme_derive_has_all_fields() {
        let theme = THEME_DARK.clone();
        assert_eq!(theme.primary, theme.accent);
        assert_eq!(theme.markdown_text, theme.text);
        assert_eq!(theme.syntax_comment, theme.text_muted);
    }

    #[test]
    fn test_each_theme_has_correct_mode() {
        let light_themes = ["light", "github", "nightowl", "vercel"];
        for name in &light_themes {
            let theme = ALL_THEMES.iter().find(|t| t.name == *name).unwrap();
            assert_eq!(theme.mode, ThemeMode::Light, "{} should be light", name);
        }
    }

    #[test]
    fn test_add_plugin_theme() {
        let mut state = ThemeState::new();
        let len_before = state.theme_names().len();
        state.add_plugin_theme("plugin-theme".into(), THEME_DRACULA.clone());
        assert_eq!(state.theme_names().len(), len_before + 1);
        assert!(state.switch_theme("plugin-theme"));
    }

    #[test]
    fn test_tokyonight_has_specific_colors() {
        let theme = &THEME_TOKYONIGHT;
        assert_eq!(theme.background, rgb(0x1a, 0x1b, 0x26));
        assert_eq!(theme.primary, rgb(0x82, 0xaa, 0xff));
        assert_eq!(theme.syntax_keyword, rgb(0xc0, 0x99, 0xff));
    }

    #[test]
    fn test_dracula_has_specific_colors() {
        let theme = &THEME_DRACULA;
        assert_eq!(theme.background, rgb(0x28, 0x2a, 0x36));
        assert_eq!(theme.secondary, rgb(0xff, 0x79, 0xc6));
        assert_eq!(theme.syntax_function, rgb(0x50, 0xfa, 0x7b));
    }
}
