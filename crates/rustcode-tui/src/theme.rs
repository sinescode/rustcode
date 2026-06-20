//! TUI theme system — 8 built-in color themes with dark/light mode support.
//!
//! Ported from: `packages/tui/src/context/theme.tsx` and `packages/tui/src/theme/`
//!
//! ## Built-in Themes
//!
//! | Name       | Style    | Background | Accent   |
//! |------------|----------|------------|----------|
//! | dark       | dark     | #1a1b26    | #7aa2f7  |
//! | light      | light    | #e0e0e0    | #3366cc  |
//! | dracula    | dark     | #282a36    | #bd93f9  |
//! | monokai    | dark     | #272822    | #a6e22e  |
//! | nord       | dark     | #2e3440    | #88c0d0  |
//! | solarized  | dark     | #002b36    | #268bd2  |
//! | github     | light    | #ffffff    | #0969da  |
//! | tokyonight | dark     | #1a1b26    | #7dcfff  |

use ratatui::style::Color;

// ── Theme Definition ────────────────────────────────────────────────────────

/// A color theme for the TUI.
#[derive(Debug, Clone)]
pub struct Theme {
    /// Theme name (e.g. "dark", "dracula").
    pub name: &'static str,
    /// Whether this is a dark or light theme.
    pub mode: ThemeMode,
    /// Main background color.
    pub background: Color,
    /// Main foreground / text color.
    pub foreground: Color,
    /// Accent / highlight color (used for borders, titles, active elements).
    pub accent: Color,
    /// Dimmed / secondary text color.
    pub dim: Color,
    /// Border / separator color.
    pub border: Color,
    /// Success / positive status color.
    pub success: Color,
    /// Warning / caution status color.
    pub warning: Color,
    /// Error / negative status color.
    pub error: Color,
    /// Info / neutral status color.
    pub info: Color,
}

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

// ── Built-in Themes ─────────────────────────────────────────────────────────

/// Dark (default dark theme) — background: #1a1b26, accent: #7aa2f7
pub const THEME_DARK: Theme = Theme {
    name: "dark",
    mode: ThemeMode::Dark,
    background: Color::Rgb(0x1a, 0x1b, 0x26),
    foreground: Color::Rgb(0xc0, 0xca, 0xf5),
    accent: Color::Rgb(0x7a, 0xa2, 0xf7),
    dim: Color::Rgb(0x56, 0x5f, 0x89),
    border: Color::Rgb(0x29, 0x2e, 0x42),
    success: Color::Rgb(0x9e, 0xce, 0x6a),
    warning: Color::Rgb(0xe0, 0xaf, 0x68),
    error: Color::Rgb(0xf7, 0x76, 0x8e),
    info: Color::Rgb(0x7d, 0xcf, 0xff),
};

/// Light — background: #e0e0e0, accent: #3366cc
pub const THEME_LIGHT: Theme = Theme {
    name: "light",
    mode: ThemeMode::Light,
    background: Color::Rgb(0xe0, 0xe0, 0xe0),
    foreground: Color::Rgb(0x1a, 0x1a, 0x1a),
    accent: Color::Rgb(0x33, 0x66, 0xcc),
    dim: Color::Rgb(0x88, 0x88, 0x88),
    border: Color::Rgb(0xbb, 0xbb, 0xbb),
    success: Color::Rgb(0x1a, 0x7f, 0x37),
    warning: Color::Rgb(0x9a, 0x67, 0x00),
    error: Color::Rgb(0xcf, 0x22, 0x2e),
    info: Color::Rgb(0x09, 0x69, 0xda),
};

/// Dracula — background: #282a36, accent: #bd93f9
pub const THEME_DRACULA: Theme = Theme {
    name: "dracula",
    mode: ThemeMode::Dark,
    background: Color::Rgb(0x28, 0x2a, 0x36),
    foreground: Color::Rgb(0xf8, 0xf8, 0xf2),
    accent: Color::Rgb(0xbd, 0x93, 0xf9),
    dim: Color::Rgb(0x62, 0x72, 0xa4),
    border: Color::Rgb(0x44, 0x47, 0x5a),
    success: Color::Rgb(0x50, 0xfa, 0x7b),
    warning: Color::Rgb(0xff, 0xb8, 0x6c),
    error: Color::Rgb(0xff, 0x55, 0x55),
    info: Color::Rgb(0x8b, 0xe9, 0xfd),
};

/// Monokai — background: #272822, accent: #a6e22e
pub const THEME_MONOKAI: Theme = Theme {
    name: "monokai",
    mode: ThemeMode::Dark,
    background: Color::Rgb(0x27, 0x28, 0x22),
    foreground: Color::Rgb(0xf8, 0xf8, 0xf2),
    accent: Color::Rgb(0xa6, 0xe2, 0x2e),
    dim: Color::Rgb(0x75, 0x71, 0x5e),
    border: Color::Rgb(0x3e, 0x3d, 0x32),
    success: Color::Rgb(0xa6, 0xe2, 0x2e),
    warning: Color::Rgb(0xfd, 0x97, 0x1f),
    error: Color::Rgb(0xf9, 0x26, 0x72),
    info: Color::Rgb(0x66, 0xd9, 0xef),
};

/// Nord — background: #2e3440, accent: #88c0d0
pub const THEME_NORD: Theme = Theme {
    name: "nord",
    mode: ThemeMode::Dark,
    background: Color::Rgb(0x2e, 0x34, 0x40),
    foreground: Color::Rgb(0xec, 0xef, 0xf4),
    accent: Color::Rgb(0x88, 0xc0, 0xd0),
    dim: Color::Rgb(0x4c, 0x56, 0x6a),
    border: Color::Rgb(0x3b, 0x42, 0x52),
    success: Color::Rgb(0xa3, 0xbe, 0x8c),
    warning: Color::Rgb(0xeb, 0xcb, 0x8b),
    error: Color::Rgb(0xbf, 0x61, 0x6a),
    info: Color::Rgb(0x81, 0xa1, 0xc1),
};

/// Solarized — background: #002b36, accent: #268bd2
pub const THEME_SOLARIZED: Theme = Theme {
    name: "solarized",
    mode: ThemeMode::Dark,
    background: Color::Rgb(0x00, 0x2b, 0x36),
    foreground: Color::Rgb(0x83, 0x94, 0x96),
    accent: Color::Rgb(0x26, 0x8b, 0xd2),
    dim: Color::Rgb(0x58, 0x6e, 0x75),
    border: Color::Rgb(0x07, 0x36, 0x42),
    success: Color::Rgb(0x85, 0x99, 0x00),
    warning: Color::Rgb(0xb5, 0x89, 0x00),
    error: Color::Rgb(0xdc, 0x32, 0x2f),
    info: Color::Rgb(0x2a, 0xa1, 0x98),
};

/// GitHub — background: #ffffff, accent: #0969da
pub const THEME_GITHUB: Theme = Theme {
    name: "github",
    mode: ThemeMode::Light,
    background: Color::Rgb(0xff, 0xff, 0xff),
    foreground: Color::Rgb(0x1f, 0x23, 0x28),
    accent: Color::Rgb(0x09, 0x69, 0xda),
    dim: Color::Rgb(0x65, 0x6d, 0x76),
    border: Color::Rgb(0xd0, 0xd7, 0xde),
    success: Color::Rgb(0x1a, 0x7f, 0x37),
    warning: Color::Rgb(0x9a, 0x67, 0x00),
    error: Color::Rgb(0xcf, 0x22, 0x2e),
    info: Color::Rgb(0x09, 0x69, 0xda),
};

/// TokyoNight — background: #1a1b26, accent: #7dcfff
pub const THEME_TOKYONIGHT: Theme = Theme {
    name: "tokyonight",
    mode: ThemeMode::Dark,
    background: Color::Rgb(0x1a, 0x1b, 0x26),
    foreground: Color::Rgb(0xc0, 0xca, 0xf5),
    accent: Color::Rgb(0x7d, 0xcf, 0xff),
    dim: Color::Rgb(0x56, 0x5f, 0x89),
    border: Color::Rgb(0x29, 0x2e, 0x42),
    success: Color::Rgb(0x9e, 0xce, 0x6a),
    warning: Color::Rgb(0xe0, 0xaf, 0x68),
    error: Color::Rgb(0xf7, 0x76, 0x8e),
    info: Color::Rgb(0x7d, 0xcf, 0xff),
};

/// All built-in themes in display order.
pub const ALL_THEMES: &[&Theme] = &[
    &THEME_DARK,
    &THEME_LIGHT,
    &THEME_DRACULA,
    &THEME_MONOKAI,
    &THEME_NORD,
    &THEME_SOLARIZED,
    &THEME_GITHUB,
    &THEME_TOKYONIGHT,
];

// ── Theme State ──────────────────────────────────────────────────────────────

/// Persistent theme state managed by the TUI app.
#[derive(Debug, Clone)]
#[derive(Default)]
pub struct ThemeState {
    /// Index into `ALL_THEMES` for the currently active theme.
    theme_index: usize,
    /// Whether the theme has been locked (prevents cycling/switching).
    locked: bool,
}


impl ThemeState {
    /// Create a new ThemeState with the default (dark) theme.
    pub fn new() -> Self {
        Self::default()
    }

    /// Get the current theme.
    pub fn current(&self) -> &'static Theme {
        ALL_THEMES[self.theme_index]
    }

    /// Get the current theme name.
    pub fn name(&self) -> &'static str {
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
        if let Some(idx) = ALL_THEMES.iter().position(|t| t.name == name) {
            self.theme_index = idx;
            true
        } else {
            false
        }
    }

    /// Cycle themes forward (+1) or backward (-1).
    /// Returns the name of the new theme.
    pub fn cycle_theme(&mut self, direction: i32) -> Option<&'static str> {
        if self.locked {
            return None;
        }
        let len = ALL_THEMES.len() as i32;
        let new_index = (self.theme_index as i32 + direction).rem_euclid(len) as usize;
        self.theme_index = new_index;
        Some(self.current().name)
    }

    /// Toggle between dark and light mode.
    /// Finds the next theme of the opposite mode.
    /// Returns the name of the new theme.
    pub fn toggle_mode(&mut self) -> Option<&'static str> {
        if self.locked {
            return None;
        }
        let target_mode = self.current().mode.invert();
        // Find the first theme matching the target mode, starting from current + 1
        let len = ALL_THEMES.len();
        for offset in 1..len {
            let idx = (self.theme_index + offset) % len;
            if ALL_THEMES[idx].mode == target_mode {
                self.theme_index = idx;
                return Some(self.current().name);
            }
        }
        // If no other theme matches, just switch between dark and light directly
        if target_mode == ThemeMode::Light {
            self.theme_index = ALL_THEMES
                .iter()
                .position(|t| t.name == "light")
                .unwrap_or(0);
        } else {
            self.theme_index = ALL_THEMES
                .iter()
                .position(|t| t.name == "dark")
                .unwrap_or(0);
        }
        Some(self.current().name)
    }

    /// Get the list of all theme names.
    pub fn theme_names() -> Vec<&'static str> {
        ALL_THEMES.iter().map(|t| t.name).collect()
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
        // Cycle backward from first wraps to last
        assert_eq!(state.cycle_theme(-1), Some("tokyonight"));
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
        // No duplicate names
        let names: Vec<&str> = ALL_THEMES.iter().map(|t| t.name).collect();
        let mut unique = names.clone();
        unique.sort();
        unique.dedup();
        assert_eq!(unique.len(), names.len());
    }

    #[test]
    fn test_eight_themes() {
        assert_eq!(ALL_THEMES.len(), 8);
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
        let names = ThemeState::theme_names();
        assert_eq!(
            names,
            vec![
                "dark",
                "light",
                "dracula",
                "monokai",
                "nord",
                "solarized",
                "github",
                "tokyonight"
            ]
        );
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
}
