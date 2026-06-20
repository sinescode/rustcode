//! Dialog stack system — stackable modal dialogs with backdrop dimming.
//!
//! Ported from: `packages/tui/src/ui/dialog.tsx`
//!
//! The dialog system supports a stack of overlay dialogs. Each dialog is
//! rendered centered on a dimmed background. Only the top-most dialog
//! receives input. Pressing `Esc` pops the top dialog; when the stack is
//! empty, the dialog system is inactive.
//!
//! ## Dialog types
//!
//! | Type | Purpose |
//! |------|---------|
//! | `ModelSelector` | Pick a model from the provider list |
//! | `AgentSelector` | Pick an agent type (build/plan/general) |
//! | `SessionList` | Browse and switch sessions |
//! | `ThemePicker` | Select a color theme |
//! | `Status` | Detailed status info |
//! | `Export` | Export session to file |
//! | `Timeline` | Session message tree / undoscope |
//! | `Subagent` | Subagent configuration |
//! | `Message` | View a message in detail |
//! | `Stash` | Git stash management |

use crossterm::event::{KeyCode, KeyEvent};
use ratatui::{
    layout::Rect,
    style::{Color, Style},
    widgets::{Block, Borders, Clear},
    Frame,
};

/// The type of dialog currently being shown.
///
/// # Source
/// Ported from `packages/tui/src/ui/dialog.tsx` dialog components.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DialogType {
    /// Model selector dialog.
    ModelSelector,
    /// Agent selector dialog.
    AgentSelector,
    /// Session list dialog.
    SessionList,
    /// Theme picker dialog.
    ThemePicker,
    /// Detailed status dialog.
    Status,
    /// Session export dialog.
    Export,
    /// Session timeline / undoscope dialog.
    Timeline,
    /// Subagent configuration dialog.
    Subagent,
    /// Detailed message view.
    Message,
    /// Git stash management dialog.
    Stash,
}

impl DialogType {
    /// Human-readable title for the dialog.
    pub fn title(&self) -> &'static str {
        match self {
            DialogType::ModelSelector => " Select Model ",
            DialogType::AgentSelector => " Select Agent ",
            DialogType::SessionList => " Sessions ",
            DialogType::ThemePicker => " Choose Theme ",
            DialogType::Status => " Status ",
            DialogType::Export => " Export Session ",
            DialogType::Timeline => " Session Timeline ",
            DialogType::Subagent => " Subagent ",
            DialogType::Message => " Message Detail ",
            DialogType::Stash => " Git Stash ",
        }
    }

    /// Default dialog width as a fraction of terminal width (0.0–1.0).
    pub fn width_ratio(&self) -> f64 {
        match self {
            DialogType::ModelSelector | DialogType::AgentSelector | DialogType::SessionList => 0.5,
            DialogType::ThemePicker => 0.4,
            DialogType::Status => 0.5,
            DialogType::Export => 0.6,
            DialogType::Timeline => 0.7,
            DialogType::Subagent => 0.5,
            DialogType::Message => 0.7,
            DialogType::Stash => 0.6,
        }
    }

    /// Default dialog height as a fraction of terminal height (0.0–1.0).
    pub fn height_ratio(&self) -> f64 {
        match self {
            DialogType::ModelSelector | DialogType::AgentSelector | DialogType::SessionList => 0.6,
            DialogType::ThemePicker => 0.5,
            DialogType::Status => 0.5,
            DialogType::Export => 0.4,
            DialogType::Timeline => 0.7,
            DialogType::Subagent => 0.4,
            DialogType::Message => 0.7,
            DialogType::Stash => 0.5,
        }
    }
}

/// A single entry on the dialog stack.
///
/// Each entry records the dialog type and an opaque identifier string
/// that the renderer can use to determine what content to draw.
#[derive(Debug, Clone)]
pub struct DialogEntry {
    /// Type of dialog.
    pub dialog_type: DialogType,
    /// Optional contextual ID (e.g., session ID, model ID).
    pub context_id: Option<String>,
}

impl DialogEntry {
    /// Create a new dialog entry.
    pub fn new(dialog_type: DialogType) -> Self {
        Self {
            dialog_type,
            context_id: None,
        }
    }

    /// Create a new dialog entry with a context ID.
    pub fn with_context(dialog_type: DialogType, context_id: String) -> Self {
        Self {
            dialog_type,
            context_id: Some(context_id),
        }
    }
}

/// State for the dialog stack system.
///
/// # Source
/// Ported from `packages/tui/src/ui/dialog.tsx` `DialogState`.
#[derive(Debug, Default)]
pub struct DialogState {
    /// Stack of active dialogs (top = last element).
    pub stack: Vec<DialogEntry>,
    /// Whether the dialog system is active (has dialogs).
    pub active: bool,
}

impl DialogState {
    pub fn new() -> Self {
        Self {
            stack: Vec::new(),
            active: false,
        }
    }

    /// Push a dialog onto the stack (makes it the active top dialog).
    pub fn push(&mut self, dialog_type: DialogType) {
        self.stack.push(DialogEntry::new(dialog_type));
        self.active = true;
    }

    /// Push a dialog with a context ID.
    pub fn push_with_context(&mut self, dialog_type: DialogType, context_id: String) {
        self.stack
            .push(DialogEntry::with_context(dialog_type, context_id));
        self.active = true;
    }

    /// Pop the top dialog from the stack.
    ///
    /// Returns the popped dialog, or `None` if the stack was empty.
    pub fn pop(&mut self) -> Option<DialogEntry> {
        let entry = self.stack.pop();
        if self.stack.is_empty() {
            self.active = false;
        }
        entry
    }

    /// Clear all dialogs.
    pub fn clear(&mut self) {
        self.stack.clear();
        self.active = false;
    }

    /// Get the top dialog (the one receiving input).
    pub fn top(&self) -> Option<&DialogEntry> {
        self.stack.last()
    }

    /// Whether there are any dialogs in the stack.
    pub fn is_active(&self) -> bool {
        self.active && !self.stack.is_empty()
    }

    /// Number of dialogs in the stack.
    pub fn len(&self) -> usize {
        self.stack.len()
    }

    /// Handle a key event for the topmost dialog.
    ///
    /// Returns `true` if the key was consumed by the dialog system.
    /// The caller should check this before dispatching to other handlers.
    pub fn handle_key(&mut self, key: KeyEvent) -> bool {
        if !self.is_active() {
            return false;
        }

        // Esc always pops the top dialog (from any dialog type)
        if key.code == KeyCode::Esc {
            self.pop();
            return true;
        }

        // Other keys are consumed by the dialog overlay (prevents leaking
        // to the main input when a dialog is open).
        // Specific dialog types may handle additional keys in their own
        // renderer/handler.

        // Consume all printable keys to prevent input leaking
        true
    }
}

/// Render the dialog backdrop (dimmed overlay behind dialog).
///
/// Dims the entire screen to indicate that dialogs are modal.
pub fn render_backdrop(f: &mut Frame, area: Rect) {
    // Render a semi-transparent overlay by filling with dark characters
    let dim_style = Style::default().bg(Color::Rgb(20, 20, 20));
    let block = Block::default().style(dim_style);
    f.render_widget(Clear, area);
    f.render_widget(block, area);
}

/// Render the dialog box frame (borders + clear interior).
///
/// Returns the inner area where dialog content should be rendered.
pub fn render_dialog_frame(
    f: &mut Frame,
    area: Rect,
    dialog_type: DialogType,
    extra_height: u16,
) -> Rect {
    let dialog_width = (area.width as f64 * dialog_type.width_ratio())
        .min(100.0)
        .max(30.0) as u16;
    let base_height = (area.height as f64 * dialog_type.height_ratio())
        .min(40.0)
        .max(10.0) as u16;
    let dialog_height = base_height + extra_height;

    let dialog_x = (area.width.saturating_sub(dialog_width)) / 2;
    let dialog_y = (area.height.saturating_sub(dialog_height)) / 3;

    let dialog_area = Rect::new(
        area.x + dialog_x,
        area.y + dialog_y,
        dialog_width,
        dialog_height,
    );

    // Clear the dialog area
    f.render_widget(Clear, dialog_area);

    let block = Block::default()
        .borders(Borders::ALL)
        .title(dialog_type.title())
        .border_style(Style::default().fg(Color::Cyan))
        .style(Style::default().bg(Color::Black));

    let inner = block.inner(dialog_area);
    f.render_widget(block, dialog_area);

    inner
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_dialog_stack_push_pop() {
        let mut state = DialogState::new();
        assert!(!state.is_active());
        assert_eq!(state.len(), 0);

        state.push(DialogType::ModelSelector);
        assert!(state.is_active());
        assert_eq!(state.len(), 1);
        assert_eq!(
            state.top().expect("should have top").dialog_type,
            DialogType::ModelSelector
        );

        state.push(DialogType::Status);
        assert_eq!(state.len(), 2);
        assert_eq!(
            state.top().expect("should have top").dialog_type,
            DialogType::Status
        );

        let popped = state.pop();
        assert!(popped.is_some());
        assert_eq!(
            popped.expect("popped dialog").dialog_type,
            DialogType::Status
        );
        assert_eq!(state.len(), 1);
        assert!(state.is_active());

        // Pop last dialog
        let popped = state.pop();
        assert!(popped.is_some());
        assert!(!state.is_active());
        assert_eq!(state.len(), 0);
    }

    #[test]
    fn test_dialog_stack_clear() {
        let mut state = DialogState::new();
        state.push(DialogType::ModelSelector);
        state.push(DialogType::SessionList);
        state.push(DialogType::Export);
        assert_eq!(state.len(), 3);

        state.clear();
        assert_eq!(state.len(), 0);
        assert!(!state.is_active());
    }

    #[test]
    fn test_dialog_pop_empty() {
        let mut state = DialogState::new();
        let popped = state.pop();
        assert!(popped.is_none());
        assert!(!state.is_active());
    }

    #[test]
    fn test_dialog_esc_handling() {
        let mut state = DialogState::new();
        state.push(DialogType::ThemePicker);
        assert!(state.is_active());

        let consumed = state.handle_key(KeyEvent::new(
            KeyCode::Esc,
            crossterm::event::KeyModifiers::NONE,
        ));
        assert!(consumed);
        assert!(!state.is_active()); // Esc should pop the dialog
    }

    #[test]
    fn test_dialog_key_consumption_when_active() {
        let mut state = DialogState::new();
        state.push(DialogType::Status);

        // Any key should be consumed when a dialog is active
        let consumed = state.handle_key(KeyEvent::new(
            KeyCode::Char('x'),
            crossterm::event::KeyModifiers::NONE,
        ));
        assert!(consumed);
        assert!(state.is_active()); // Dialog still active
    }

    #[test]
    fn test_dialog_no_key_consumption_when_inactive() {
        let mut state = DialogState::new();
        let consumed = state.handle_key(KeyEvent::new(
            KeyCode::Char('x'),
            crossterm::event::KeyModifiers::NONE,
        ));
        assert!(!consumed);
    }

    #[test]
    fn test_dialog_with_context() {
        let mut state = DialogState::new();
        state.push_with_context(DialogType::SessionList, "ses_abc123".into());
        let top = state.top().expect("should have top");
        assert_eq!(top.dialog_type, DialogType::SessionList);
        assert_eq!(top.context_id, Some("ses_abc123".into()));
    }
}
