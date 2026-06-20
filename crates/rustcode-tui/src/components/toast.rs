//! Toast notification system — transient popup messages in the top-right corner.
//!
//! Ported from: `packages/tui/src/ui/toast.tsx`
//!
//! Toasts are short-lived notifications that appear in the top-right corner
//! and automatically expire. They stack vertically, with newer toasts at the
//! bottom. Maximum 5 visible at a time.
//!
//! ## Usage
//!
//! ```ignore
//! toast_state.show(Some("Success"), "File saved.", ToastVariant::Success);
//! // Each frame: toast_state.tick();
//! // Render: render_toast(f, area, &toast_state);
//! ```

use ratatui::{
    layout::{Constraint, Direction, Layout, Rect},
    style::{Color, Style},
    text::{Line, Span},
    widgets::{Block, Borders, Clear, Paragraph},
    Frame,
};
use std::collections::VecDeque;
use std::time::{Duration, Instant};

/// Maximum number of toasts visible at once.
const MAX_VISIBLE_TOASTS: usize = 5;

/// Default toast duration (5 seconds).
const DEFAULT_DURATION_MS: u64 = 5000;

/// Maximum width of a toast in characters.
const MAX_TOAST_WIDTH: u16 = 50;

/// Minimum width of a toast in characters.
const MIN_TOAST_WIDTH: u16 = 30;

/// Toast variant — determines color and icon.
///
/// # Source
/// Ported from `packages/tui/src/ui/toast.tsx` variant mapping.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ToastVariant {
    Info,
    Success,
    Warning,
    Error,
}

impl ToastVariant {
    /// Get the color associated with this variant.
    fn color(self) -> Color {
        match self {
            ToastVariant::Info => Color::Cyan,
            ToastVariant::Success => Color::Green,
            ToastVariant::Warning => Color::Yellow,
            ToastVariant::Error => Color::Red,
        }
    }

    /// Get the icon for this variant.
    fn icon(self) -> &'static str {
        match self {
            ToastVariant::Info => "ℹ",
            ToastVariant::Success => "✓",
            ToastVariant::Warning => "△",
            ToastVariant::Error => "✗",
        }
    }
}

impl std::fmt::Display for ToastVariant {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ToastVariant::Info => write!(f, "info"),
            ToastVariant::Success => write!(f, "success"),
            ToastVariant::Warning => write!(f, "warning"),
            ToastVariant::Error => write!(f, "error"),
        }
    }
}

/// A single toast notification.
#[derive(Debug, Clone)]
pub struct ToastMessage {
    /// Optional title displayed in bold at the top.
    pub title: Option<String>,
    /// Main message text.
    pub message: String,
    /// Variant determining color scheme.
    pub variant: ToastVariant,
    /// How long this toast lives (in milliseconds).
    pub duration_ms: u64,
    /// When this toast was created.
    pub created_at: Instant,
}

impl ToastMessage {
    /// Create a new toast with the given variant and default duration.
    pub fn new(title: Option<String>, message: String, variant: ToastVariant) -> Self {
        Self {
            title,
            message,
            variant,
            duration_ms: DEFAULT_DURATION_MS,
            created_at: Instant::now(),
        }
    }

    /// Create a new toast with a custom duration.
    pub fn with_duration(
        title: Option<String>,
        message: String,
        variant: ToastVariant,
        duration_ms: u64,
    ) -> Self {
        Self {
            title,
            message,
            variant,
            duration_ms,
            created_at: Instant::now(),
        }
    }

    /// Whether this toast has expired.
    fn is_expired(&self) -> bool {
        self.created_at.elapsed() >= Duration::from_millis(self.duration_ms)
    }

    /// Remaining duration as a percentage (0.0 to 1.0).
    pub fn remaining_pct(&self) -> f64 {
        let elapsed = self.created_at.elapsed().as_millis() as f64;
        let total = self.duration_ms as f64;
        if elapsed >= total {
            0.0
        } else {
            1.0 - (elapsed / total)
        }
    }
}

/// State for the toast notification system.
///
/// # Source
/// Ported from `packages/tui/src/ui/toast.tsx` `ToastState`.
#[derive(Debug, Default)]
pub struct ToastState {
    /// Active toasts (not yet expired). Newer toasts at the back.
    pub toasts: VecDeque<ToastMessage>,
}

impl ToastState {
    pub fn new() -> Self {
        Self {
            toasts: VecDeque::new(),
        }
    }

    /// Show a toast notification.
    ///
    /// Title is optional. The toast will auto-expire after the default
    /// duration (5 seconds).
    pub fn show(&mut self, title: Option<&str>, message: &str, variant: ToastVariant) {
        let toast = ToastMessage::new(title.map(|t| t.to_string()), message.to_string(), variant);
        self.toasts.push_back(toast);
    }

    /// Show a toast with a custom duration in milliseconds.
    pub fn show_with_duration(
        &mut self,
        title: Option<&str>,
        message: &str,
        variant: ToastVariant,
        duration_ms: u64,
    ) {
        let toast = ToastMessage::with_duration(
            title.map(|t| t.to_string()),
            message.to_string(),
            variant,
            duration_ms,
        );
        self.toasts.push_back(toast);
    }

    /// Remove expired toasts. Call this every frame.
    pub fn tick(&mut self) {
        // Remove toasts from the front (oldest first) until we find
        // one that hasn't expired.
        while let Some(toast) = self.toasts.front() {
            if toast.is_expired() {
                self.toasts.pop_front();
            } else {
                break;
            }
        }

        // If we have more than MAX_VISIBLE_TOASTS, drop the oldest.
        while self.toasts.len() > MAX_VISIBLE_TOASTS {
            self.toasts.pop_front();
        }
    }

    /// Whether there are any active toasts.
    pub fn has_active(&self) -> bool {
        !self.toasts.is_empty()
    }

    /// Number of active toasts.
    pub fn len(&self) -> usize {
        self.toasts.len()
    }

    /// Clear all toasts immediately.
    pub fn clear(&mut self) {
        self.toasts.clear();
    }
}

/// Render toast notifications in the top-right corner of the screen.
///
/// Toasts are stacked vertically, with the newest at the bottom.
/// Each toast is color-coded by variant and shows an icon, optional title,
/// and message text.
pub fn render_toast(f: &mut Frame, area: Rect, state: &ToastState) {
    if state.toasts.is_empty() {
        return;
    }

    // Only render the most recent MAX_VISIBLE_TOASTS (newest at back of deque)
    let visible_toasts: Vec<&ToastMessage> =
        state.toasts.iter().rev().take(MAX_VISIBLE_TOASTS).collect();

    // Calculate toast dimensions
    let max_msg_len = visible_toasts
        .iter()
        .map(|t| {
            let title_len = t.title.as_ref().map(|s| s.len()).unwrap_or(0);
            t.message.len().max(title_len)
        })
        .max()
        .unwrap_or(MIN_TOAST_WIDTH as usize);

    let toast_width = (max_msg_len as u16 + 6) // icon + padding + borders
        .clamp(MIN_TOAST_WIDTH, MAX_TOAST_WIDTH)
        .min(area.width.saturating_sub(4));

    // Each toast: 1 line title (optional) + wrapped message lines + borders
    // Estimate: title line + wrapped message + 2 border lines
    let toast_height: u16 = 3; // border top/bottom + 1 line of content (at minimum)

    let total_height = toast_height * visible_toasts.len() as u16;

    // Position in top-right corner
    let x = area.x + area.width.saturating_sub(toast_width + 2);
    let y = area.y + 1;
    let toast_area = Rect::new(x, y, toast_width, total_height);

    // Render each toast
    let rows = Layout::default()
        .direction(Direction::Vertical)
        .constraints(
            visible_toasts
                .iter()
                .map(|_| Constraint::Length(toast_height))
                .collect::<Vec<_>>(),
        )
        .split(toast_area);

    for (i, toast) in visible_toasts.iter().enumerate() {
        if i >= rows.len() {
            break;
        }

        let row = rows[i];
        let color = toast.variant.color();
        let icon = toast.variant.icon();

        // Slightly fade older toasts
        let opacity = toast.remaining_pct();
        let bg_color = if opacity > 0.5 {
            Color::Black
        } else {
            Color::Rgb(20, 20, 20)
        };

        // Clear the area first
        f.render_widget(Clear, row);

        let title_str = match &toast.title {
            Some(t) => format!(" {icon} {t} "),
            None => format!(" {icon} "),
        };

        let block = Block::default()
            .borders(Borders::ALL)
            .title(title_str)
            .border_style(Style::default().fg(color))
            .style(Style::default().bg(bg_color));

        let inner = block.inner(row);
        f.render_widget(block, row);

        // Render message text inside
        let msg_style = Style::default().fg(color);
        let paragraph = Paragraph::new(Line::from(Span::styled(&toast.message, msg_style)));
        f.render_widget(paragraph, inner);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::thread;

    #[test]
    fn test_toast_creation() {
        let toast = ToastMessage::new(
            Some("Test".into()),
            "Hello world".into(),
            ToastVariant::Info,
        );
        assert_eq!(toast.title, Some("Test".into()));
        assert_eq!(toast.message, "Hello world");
        assert_eq!(toast.variant, ToastVariant::Info);
        assert_eq!(toast.duration_ms, DEFAULT_DURATION_MS);
        assert!(!toast.is_expired());
    }

    #[test]
    fn test_toast_expiry() {
        let toast = ToastMessage::with_duration(
            None,
            "Quick".into(),
            ToastVariant::Warning,
            1, // 1ms
        );
        thread::sleep(Duration::from_millis(2));
        assert!(toast.is_expired());
    }

    #[test]
    fn test_toast_variant_colors() {
        assert_eq!(ToastVariant::Info.color(), Color::Cyan);
        assert_eq!(ToastVariant::Success.color(), Color::Green);
        assert_eq!(ToastVariant::Warning.color(), Color::Yellow);
        assert_eq!(ToastVariant::Error.color(), Color::Red);
    }

    #[test]
    fn test_toast_state_show_and_tick() {
        let mut state = ToastState::new();
        assert!(!state.has_active());
        assert_eq!(state.len(), 0);

        state.show(Some("A"), "msg", ToastVariant::Info);
        assert_eq!(state.len(), 1);
        assert!(state.has_active());

        // Tick should not expire a fresh toast
        state.tick();
        assert_eq!(state.len(), 1);
    }

    #[test]
    fn test_toast_state_max_visible() {
        let mut state = ToastState::new();
        for i in 0..7 {
            state.show(None, &format!("toast {i}"), ToastVariant::Info);
        }
        // Should only keep 5
        state.tick();
        assert_eq!(state.len(), MAX_VISIBLE_TOASTS);
    }

    #[test]
    fn test_toast_state_expiry_on_tick() {
        let mut state = ToastState::new();
        state.show_with_duration(None, "fast", ToastVariant::Error, 1);
        thread::sleep(Duration::from_millis(5));
        state.tick();
        assert_eq!(state.len(), 0);
    }

    #[test]
    fn test_toast_state_clear() {
        let mut state = ToastState::new();
        state.show(None, "a", ToastVariant::Info);
        state.show(None, "b", ToastVariant::Success);
        assert_eq!(state.len(), 2);
        state.clear();
        assert_eq!(state.len(), 0);
    }
}
