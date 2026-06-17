//! Keybinding definitions for the TUI.
//!
//! Ported from: `packages/tui/src/config/keybind.ts` and `packages/tui/src/keymap.tsx`
//!
//! ## Keybindings (matching TS defaults)
//!
//! | Key | Action | Source |
//! |-----|--------|--------|
//! | `Enter` | Submit prompt | `input_submit: "return"` |
//! | `Ctrl+C` | Cancel / exit | `app_exit: "ctrl+c,ctrl+d"` |
//! | `Ctrl+D` | Delete / exit | `app_exit: "ctrl+c,ctrl+d"` |
//! | `Escape` | Interrupt session / dismiss | `session_interrupt: "escape"` |
//! | `Ctrl+Z` | Suspend terminal | `terminal_suspend: "ctrl+z"` |
//! | `Ctrl+R` | Rename session | `session_rename: "ctrl+r"` |
//! | `Ctrl+P` | Command palette | `command_list: "ctrl+p"` |
//! | `Ctrl+B` | Background subagents | `session_background: "ctrl+b"` |
//! | `Ctrl+T` | Cycle variants | `variant_cycle: "ctrl+t"` |
//! | `Ctrl+F` | Pin session | `session_pin_toggle: "ctrl+f"` |
//! | `Ctrl+G` | First message | `messages_first: "ctrl+g"` |
//! | `Ctrl+A` | Cycle agent / line home | varies |
//! | `Tab` / `Shift+Tab` | Cycle agent | `agent_cycle: "tab"` |
//! | `PageUp` / `PageDown` | Scroll messages | `messages_page_up/page_down` |
//! | `Home` / `End` | First/last message | `messages_first/last` |
//! | `Left` / `Right` / `h` / `l` | Permission option prev/next | `permission.tsx` |
//! | `Up` / `Down` / `k` / `j` | Permission option select | `permission.tsx` |

use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};

/// Represents a keybinding in the TUI.
#[derive(Debug, Clone)]
pub struct KeyBinding {
    /// The key combination.
    pub key: KeyEvent,
    /// Human-readable description.
    pub description: &'static str,
    /// Group name for organization.
    pub group: &'static str,
}

/// Represents an action triggered by a keybinding.
///
/// # Source
/// Ported from the TS `CommandMap` in `keybind.ts` (lines 253–414).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum TuiAction {
    // App-level
    Quit,
    CommandPalette,
    Help,
    Status,
    Suspend,

    // Session
    SessionList,
    SessionNew,
    SessionRename,
    SessionShare,
    SessionFork,
    SessionCompact,
    SessionInterrupt,
    SessionUndo,
    SessionRedo,
    SessionExport,
    SessionTimeline,
    SessionDelete,
    SessionBackground,

    // Navigation
    ScrollUp,
    ScrollDown,
    ScrollPageUp,
    ScrollPageDown,
    ScrollHalfPageUp,
    ScrollHalfPageDown,
    ScrollFirst,
    ScrollLast,
    ScrollNextMessage,
    ScrollPrevMessage,

    // Child sessions
    ChildFirst,
    ChildNext,
    ChildPrev,
    Parent,

    // Agent/Model
    AgentCycle,
    AgentCycleReverse,
    AgentList,
    ModelList,
    ModelCycleRecent,
    ModelCycleRecentReverse,
    VariantCycle,
    VariantList,

    // Provider
    ProviderConnect,

    // Theme
    ThemeSwitch,
    ThemeSwitchMode,

    // Toggles
    ToggleSidebar,
    ToggleTimestamps,
    ToggleThinking,
    ToggleToolDetails,
    ToggleConceal,
    ToggleScrollbar,
    ToggleGenericToolOutput,
    ToggleTerminalTitle,
    ToggleAnimations,
    ToggleFileContext,
    ToggleDiffWrap,
    TogglePasteSummary,

    // Input
    InputSubmit,
    InputClear,
    InputNewline,

    // Permission
    PermissionOnce,
    PermissionAlways,
    PermissionReject,
    PermissionPrevOption,
    PermissionNextOption,

    // Question
    QuestionSelect(u8),
    QuestionPrevOption,
    QuestionNextOption,
    QuestionPrevTab,
    QuestionNextTab,
    QuestionSubmit,
    QuestionReject,

    // Quick switch (1-9)
    QuickSwitch(u8),

    /// Custom command string (from server events).
    CustomCommand(String),
}

/// Map a crossterm `KeyEvent` to a `TuiAction`, if there is a binding.
///
/// # Source
/// Ported from `packages/tui/src/config/keybind.ts` `Definitions`.
pub fn key_to_action(key: KeyEvent) -> Option<TuiAction> {
    match key {
        // ── Input submit ──────────────────────────────────────────
        KeyEvent {
            code: KeyCode::Enter,
            modifiers: KeyModifiers::NONE,
            ..
        } => Some(TuiAction::InputSubmit),

        // ── Exit / Cancel ─────────────────────────────────────────
        KeyEvent {
            code: KeyCode::Char('c'),
            modifiers: KeyModifiers::CONTROL,
            ..
        }
        | KeyEvent {
            code: KeyCode::Char('d'),
            modifiers: KeyModifiers::CONTROL,
            ..
        } => Some(TuiAction::Quit),

        // ── Escape → interrupt ────────────────────────────────────
        KeyEvent {
            code: KeyCode::Esc, ..
        } => Some(TuiAction::SessionInterrupt),

        // ── Command palette ───────────────────────────────────────
        KeyEvent {
            code: KeyCode::Char('p'),
            modifiers: KeyModifiers::CONTROL,
            ..
        } => Some(TuiAction::CommandPalette),

        // ── Status ────────────────────────────────────────────────
        // <leader>s is implemented in the app layer; raw binding:

        // ── Session rename ────────────────────────────────────────
        KeyEvent {
            code: KeyCode::Char('r'),
            modifiers: KeyModifiers::CONTROL,
            ..
        } => Some(TuiAction::SessionRename),

        // ── Background subagents ──────────────────────────────────
        KeyEvent {
            code: KeyCode::Char('b'),
            modifiers: KeyModifiers::CONTROL,
            ..
        } => Some(TuiAction::SessionBackground),

        // ── Agent cycle ───────────────────────────────────────────
        KeyEvent {
            code: KeyCode::Tab,
            modifiers: KeyModifiers::NONE,
            ..
        } => Some(TuiAction::AgentCycle),

        KeyEvent {
            code: KeyCode::BackTab,
            modifiers: KeyModifiers::SHIFT,
            ..
        } => Some(TuiAction::AgentCycleReverse),

        // ── Variant cycle ─────────────────────────────────────────
        KeyEvent {
            code: KeyCode::Char('t'),
            modifiers: KeyModifiers::CONTROL,
            ..
        } => Some(TuiAction::VariantCycle),

        // ── Scroll navigation ─────────────────────────────────────
        KeyEvent {
            code: KeyCode::PageUp,
            modifiers: KeyModifiers::NONE,
            ..
        } => Some(TuiAction::ScrollPageUp),

        KeyEvent {
            code: KeyCode::PageDown,
            modifiers: KeyModifiers::NONE,
            ..
        } => Some(TuiAction::ScrollPageDown),

        KeyEvent {
            code: KeyCode::Home,
            modifiers: KeyModifiers::NONE,
            ..
        } => Some(TuiAction::ScrollFirst),

        KeyEvent {
            code: KeyCode::End,
            modifiers: KeyModifiers::NONE,
            ..
        } => Some(TuiAction::ScrollLast),

        KeyEvent {
            code: KeyCode::Char('g'),
            modifiers: KeyModifiers::CONTROL,
            ..
        } => Some(TuiAction::ScrollFirst),

        KeyEvent {
            code: KeyCode::Up,
            modifiers: KeyModifiers::NONE,
            ..
        } => Some(TuiAction::ScrollUp),

        KeyEvent {
            code: KeyCode::Down,
            modifiers: KeyModifiers::NONE,
            ..
        } => Some(TuiAction::ScrollDown),

        // ── Suspend terminal ──────────────────────────────────────
        KeyEvent {
            code: KeyCode::Char('z'),
            modifiers: KeyModifiers::CONTROL,
            ..
        } => Some(TuiAction::Suspend),

        // ── Agent list ────────────────────────────────────────────
        // <leader>a

        // ── Model list ────────────────────────────────────────────
        // <leader>m

        // ── Session list ──────────────────────────────────────────
        // <leader>l

        // ── Session new ───────────────────────────────────────────
        // <leader>n

        // ── Session compact ───────────────────────────────────────
        // <leader>c

        // ── Sidebar toggle ────────────────────────────────────────
        // <leader>b

        // ── Session export ────────────────────────────────────────
        // <leader>x

        // ── Session undo ──────────────────────────────────────────
        // <leader>u

        // ── Session redo ──────────────────────────────────────────
        // <leader>r

        // ── Copy message ──────────────────────────────────────────
        // <leader>y

        // ── Toggle conceal ────────────────────────────────────────
        // <leader>h

        // ── Session fork ──────────────────────────────────────────
        // <leader>g  (timeline)

        // ── Session delete ────────────────────────────────────────
        KeyEvent {
            code: KeyCode::Delete,
            modifiers: KeyModifiers::NONE,
            ..
        } => Some(TuiAction::SessionDelete),

        // ── Theme switch ──────────────────────────────────────────
        // <leader>t

        _ => None,
    }
}

/// Check if a key event is a "leader" chord starter.
///
/// # Source
/// `LeaderDefault = "ctrl+x"` in `keybind.ts` line 41.
pub fn is_leader_prefix(key: KeyEvent) -> bool {
    key.code == KeyCode::Char('x') && key.modifiers == KeyModifiers::CONTROL
}

/// Map leader + key chords to actions.
pub fn leader_chord_to_action(chord: KeyEvent) -> Option<TuiAction> {
    match chord {
        // <leader> + s → status
        KeyEvent {
            code: KeyCode::Char('s'),
            modifiers: KeyModifiers::NONE,
            ..
        } => Some(TuiAction::Status),

        // <leader> + e → external editor
        KeyEvent {
            code: KeyCode::Char('e'),
            modifiers: KeyModifiers::NONE,
            ..
        } => Some(TuiAction::SessionExport),

        // <leader> + t → theme list
        KeyEvent {
            code: KeyCode::Char('t'),
            modifiers: KeyModifiers::NONE,
            ..
        } => Some(TuiAction::ThemeSwitch),

        // <leader> + b → toggle sidebar
        KeyEvent {
            code: KeyCode::Char('b'),
            modifiers: KeyModifiers::NONE,
            ..
        } => Some(TuiAction::ToggleSidebar),

        // <leader> + n → new session
        KeyEvent {
            code: KeyCode::Char('n'),
            modifiers: KeyModifiers::NONE,
            ..
        } => Some(TuiAction::SessionNew),

        // <leader> + l → session list
        KeyEvent {
            code: KeyCode::Char('l'),
            modifiers: KeyModifiers::NONE,
            ..
        } => Some(TuiAction::SessionList),

        // <leader> + a → agent list
        KeyEvent {
            code: KeyCode::Char('a'),
            modifiers: KeyModifiers::NONE,
            ..
        } => Some(TuiAction::AgentList),

        // <leader> + m → model list
        KeyEvent {
            code: KeyCode::Char('m'),
            modifiers: KeyModifiers::NONE,
            ..
        } => Some(TuiAction::ModelList),

        // <leader> + c → compact
        KeyEvent {
            code: KeyCode::Char('c'),
            modifiers: KeyModifiers::NONE,
            ..
        } => Some(TuiAction::SessionCompact),

        // <leader> + q → session list (queued)
        KeyEvent {
            code: KeyCode::Char('q'),
            modifiers: KeyModifiers::NONE,
            ..
        } => Some(TuiAction::Quit),

        // <leader> + u → undo
        KeyEvent {
            code: KeyCode::Char('u'),
            modifiers: KeyModifiers::NONE,
            ..
        } => Some(TuiAction::SessionUndo),

        // <leader> + r → redo
        KeyEvent {
            code: KeyCode::Char('r'),
            modifiers: KeyModifiers::NONE,
            ..
        } => Some(TuiAction::SessionRedo),

        // <leader> + y → copy
        // Returns an action that can be handled by the app

        // <leader> + h → toggle conceal
        KeyEvent {
            code: KeyCode::Char('h'),
            modifiers: KeyModifiers::NONE,
            ..
        } => Some(TuiAction::ToggleConceal),

        // <leader> + g → session timeline
        KeyEvent {
            code: KeyCode::Char('g'),
            modifiers: KeyModifiers::NONE,
            ..
        } => Some(TuiAction::SessionTimeline),

        // <leader> + x → session export
        KeyEvent {
            code: KeyCode::Char('x'),
            modifiers: KeyModifiers::NONE,
            ..
        } => Some(TuiAction::SessionExport),

        // <leader> + 1-9 → quick switch
        KeyEvent {
            code: KeyCode::Char(c @ '1'..='9'),
            modifiers: KeyModifiers::NONE,
            ..
        } => Some(TuiAction::QuickSwitch(
            c.to_digit(10).expect("digit char '1'..='9' always yields a valid digit") as u8,
        )),

        // <leader> + down → first child
        KeyEvent {
            code: KeyCode::Down,
            modifiers: KeyModifiers::NONE,
            ..
        } => Some(TuiAction::ChildFirst),

        _ => None,
    }
}
