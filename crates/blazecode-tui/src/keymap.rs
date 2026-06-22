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

    // ── New component actions ────────────────────────────────────
    /// Toggle the diff viewer overlay.
    DiffView,
    /// Show session list as a dialog (vs logging-only SessionList).
    SessionListDialog,
    /// Push a dialog onto the dialog stack.
    DialogPush(DialogTarget),
    /// Toggle sidebar panel (cycle tabs).
    SidebarNextPanel,
    SidebarPrevPanel,

    // ── New integrations ──────────────────────────────────────────
    /// Toggle audio notification on stream completion.
    AudioToggle,
    /// Open the current file in an external editor.
    OpenInEditor,
}

/// Target dialog type for the dialog stack.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum DialogTarget {
    ModelSelector,
    AgentSelector,
    SessionList,
    ThemePicker,
    Export,
    Timeline,
    Subagent,
    Stash,
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

        // ── Help ──────────────────────────────────────────────────
        KeyEvent {
            code: KeyCode::Char('h'),
            modifiers: KeyModifiers::CONTROL,
            ..
        } if false => None, // Ctrl+H is used for backspace in terminals
        // Help is accessed via leader+h or F1
        KeyEvent {
            code: KeyCode::F(1),
            modifiers: KeyModifiers::NONE,
            ..
        } => Some(TuiAction::Help),

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

        // ── Half-page scroll ──────────────────────────────────────
        KeyEvent {
            code: KeyCode::Char('u'),
            modifiers: KeyModifiers::CONTROL,
            ..
        } => Some(TuiAction::ScrollHalfPageUp),

        // ── Suspend terminal ──────────────────────────────────────
        KeyEvent {
            code: KeyCode::Char('z'),
            modifiers: KeyModifiers::CONTROL,
            ..
        } => Some(TuiAction::Suspend),

        // ── Toggle timestamps ─────────────────────────────────────
        KeyEvent {
            code: KeyCode::Char('s'),
            modifiers: KeyModifiers::CONTROL,
            ..
        } => Some(TuiAction::ToggleTimestamps),

        // ── Toggle thinking ───────────────────────────────────────
        KeyEvent {
            code: KeyCode::Char('y'),
            modifiers: KeyModifiers::CONTROL,
            ..
        } => Some(TuiAction::ToggleThinking),

        // ── Session delete ────────────────────────────────────────
        KeyEvent {
            code: KeyCode::Delete,
            modifiers: KeyModifiers::NONE,
            ..
        } => Some(TuiAction::SessionDelete),

        // ── Session fork ──────────────────────────────────────────
        KeyEvent {
            code: KeyCode::Char('f'),
            modifiers: KeyModifiers::CONTROL,
            ..
        } => Some(TuiAction::SessionFork),

        // ── Next/prev message ─────────────────────────────────────
        KeyEvent {
            code: KeyCode::Char('n'),
            modifiers: KeyModifiers::CONTROL,
            ..
        } => Some(TuiAction::ScrollNextMessage),

        // ── Toggle sidebar (direct) ───────────────────────────────
        KeyEvent {
            code: KeyCode::Char('b'),
            modifiers: KeyModifiers::ALT,
            ..
        } => Some(TuiAction::ToggleSidebar),

        // ── Diff viewer ───────────────────────────────────────────
        KeyEvent {
            code: KeyCode::Char('o'),
            modifiers: KeyModifiers::CONTROL,
            ..
        } => Some(TuiAction::DiffView),

        // ── Session list dialog ────────────────────────────────────
        KeyEvent {
            code: KeyCode::Char('l'),
            modifiers: KeyModifiers::CONTROL,
            ..
        } => Some(TuiAction::SessionListDialog),

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

        // <leader> + e → external editor / export
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

        // <leader> + q → quit
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

        // <leader> + y → copy message
        KeyEvent {
            code: KeyCode::Char('y'),
            modifiers: KeyModifiers::NONE,
            ..
        } => Some(TuiAction::CustomCommand("copy".into())),

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

        // <leader> + w → toggle tool details
        KeyEvent {
            code: KeyCode::Char('w'),
            modifiers: KeyModifiers::NONE,
            ..
        } => Some(TuiAction::ToggleToolDetails),

        // <leader> + i → toggle timestamps
        KeyEvent {
            code: KeyCode::Char('i'),
            modifiers: KeyModifiers::NONE,
            ..
        } => Some(TuiAction::ToggleTimestamps),

        // <leader> + k → toggle thinking
        KeyEvent {
            code: KeyCode::Char('k'),
            modifiers: KeyModifiers::NONE,
            ..
        } => Some(TuiAction::ToggleThinking),

        // <leader> + v → toggle scrollbar
        KeyEvent {
            code: KeyCode::Char('v'),
            modifiers: KeyModifiers::NONE,
            ..
        } => Some(TuiAction::ToggleScrollbar),

        // <leader> + p → command palette (alternative)
        KeyEvent {
            code: KeyCode::Char('p'),
            modifiers: KeyModifiers::NONE,
            ..
        } => Some(TuiAction::CommandPalette),

        // <leader> + f → session fork
        KeyEvent {
            code: KeyCode::Char('f'),
            modifiers: KeyModifiers::NONE,
            ..
        } => Some(TuiAction::SessionFork),

        // <leader> + d → session delete
        KeyEvent {
            code: KeyCode::Char('d'),
            modifiers: KeyModifiers::NONE,
            ..
        } => Some(TuiAction::SessionDelete),

        // <leader> + / → help
        KeyEvent {
            code: KeyCode::Char('?'),
            modifiers: KeyModifiers::SHIFT,
            ..
        } => Some(TuiAction::Help),

        // <leader> + j → toggle animations
        KeyEvent {
            code: KeyCode::Char('j'),
            modifiers: KeyModifiers::NONE,
            ..
        } => Some(TuiAction::ToggleAnimations),

        // <leader> + o → toggle file context
        KeyEvent {
            code: KeyCode::Char('o'),
            modifiers: KeyModifiers::NONE,
            ..
        } => Some(TuiAction::ToggleFileContext),

        // <leader> + z → toggle diff wrap
        KeyEvent {
            code: KeyCode::Char('z'),
            modifiers: KeyModifiers::NONE,
            ..
        } => Some(TuiAction::ToggleDiffWrap),

        // <leader> + E → open in editor
        KeyEvent {
            code: KeyCode::Char('E'),
            modifiers: KeyModifiers::NONE,
            ..
        } => Some(TuiAction::OpenInEditor),

        // <leader> + A → toggle audio notifications
        KeyEvent {
            code: KeyCode::Char('A'),
            modifiers: KeyModifiers::NONE,
            ..
        } => Some(TuiAction::AudioToggle),

        // <leader> + . → model selector dialog
        KeyEvent {
            code: KeyCode::Char('.'),
            modifiers: KeyModifiers::NONE,
            ..
        } => Some(TuiAction::DialogPush(DialogTarget::ModelSelector)),

        // <leader> + , → agent selector dialog
        KeyEvent {
            code: KeyCode::Char(','),
            modifiers: KeyModifiers::NONE,
            ..
        } => Some(TuiAction::DialogPush(DialogTarget::AgentSelector)),

        // <leader> + tab → next sidebar panel
        KeyEvent {
            code: KeyCode::Tab,
            modifiers: KeyModifiers::NONE,
            ..
        } => Some(TuiAction::SidebarNextPanel),

        // <leader> + S-Tab → prev sidebar panel
        KeyEvent {
            code: KeyCode::BackTab,
            modifiers: KeyModifiers::SHIFT,
            ..
        } => Some(TuiAction::SidebarPrevPanel),

        // <leader> + 1-9 → quick switch
        KeyEvent {
            code: KeyCode::Char(c @ '1'..='9'),
            modifiers: KeyModifiers::NONE,
            ..
        } => Some(TuiAction::QuickSwitch(
            c.to_digit(10)
                .expect("digit char '1'..='9' always yields a valid digit") as u8,
        )),

        // <leader> + down → first child
        KeyEvent {
            code: KeyCode::Down,
            modifiers: KeyModifiers::NONE,
            ..
        } => Some(TuiAction::ChildFirst),

        // <leader> + right → next child
        KeyEvent {
            code: KeyCode::Right,
            modifiers: KeyModifiers::NONE,
            ..
        } => Some(TuiAction::ChildNext),

        // <leader> + left → prev child
        KeyEvent {
            code: KeyCode::Left,
            modifiers: KeyModifiers::NONE,
            ..
        } => Some(TuiAction::ChildPrev),

        // <leader> + up → parent session
        KeyEvent {
            code: KeyCode::Up,
            modifiers: KeyModifiers::NONE,
            ..
        } => Some(TuiAction::Parent),

        _ => None,
    }
}

/// Returns all known keybindings for display in the help overlay.
pub fn all_bindings() -> Vec<KeyBinding> {
    vec![
        // App-level
        KeyBinding {
            key: KeyEvent::new(KeyCode::Char('c'), KeyModifiers::CONTROL),
            description: "Quit",
            group: "App",
        },
        KeyBinding {
            key: KeyEvent::new(KeyCode::Char('d'), KeyModifiers::CONTROL),
            description: "Quit",
            group: "App",
        },
        KeyBinding {
            key: KeyEvent::new(KeyCode::Char('p'), KeyModifiers::CONTROL),
            description: "Command palette",
            group: "App",
        },
        KeyBinding {
            key: KeyEvent::new(KeyCode::F(1), KeyModifiers::NONE),
            description: "Help",
            group: "App",
        },
        KeyBinding {
            key: KeyEvent::new(KeyCode::Esc, KeyModifiers::NONE),
            description: "Interrupt / dismiss",
            group: "App",
        },
        KeyBinding {
            key: KeyEvent::new(KeyCode::Char('z'), KeyModifiers::CONTROL),
            description: "Suspend",
            group: "App",
        },
        // Session
        KeyBinding {
            key: KeyEvent::new(KeyCode::Char('r'), KeyModifiers::CONTROL),
            description: "Rename session",
            group: "Session",
        },
        KeyBinding {
            key: KeyEvent::new(KeyCode::Char('b'), KeyModifiers::CONTROL),
            description: "Background subagents",
            group: "Session",
        },
        KeyBinding {
            key: KeyEvent::new(KeyCode::Char('f'), KeyModifiers::CONTROL),
            description: "Fork session",
            group: "Session",
        },
        KeyBinding {
            key: KeyEvent::new(KeyCode::Delete, KeyModifiers::NONE),
            description: "Delete session",
            group: "Session",
        },
        // Navigation
        KeyBinding {
            key: KeyEvent::new(KeyCode::Up, KeyModifiers::NONE),
            description: "Scroll up",
            group: "Navigation",
        },
        KeyBinding {
            key: KeyEvent::new(KeyCode::Down, KeyModifiers::NONE),
            description: "Scroll down",
            group: "Navigation",
        },
        KeyBinding {
            key: KeyEvent::new(KeyCode::PageUp, KeyModifiers::NONE),
            description: "Page up",
            group: "Navigation",
        },
        KeyBinding {
            key: KeyEvent::new(KeyCode::PageDown, KeyModifiers::NONE),
            description: "Page down",
            group: "Navigation",
        },
        KeyBinding {
            key: KeyEvent::new(KeyCode::Home, KeyModifiers::NONE),
            description: "First message",
            group: "Navigation",
        },
        KeyBinding {
            key: KeyEvent::new(KeyCode::End, KeyModifiers::NONE),
            description: "Last message",
            group: "Navigation",
        },
        KeyBinding {
            key: KeyEvent::new(KeyCode::Char('g'), KeyModifiers::CONTROL),
            description: "First message",
            group: "Navigation",
        },
        KeyBinding {
            key: KeyEvent::new(KeyCode::Char('n'), KeyModifiers::CONTROL),
            description: "Next message",
            group: "Navigation",
        },
        // Agent/Model
        KeyBinding {
            key: KeyEvent::new(KeyCode::Tab, KeyModifiers::NONE),
            description: "Next agent",
            group: "Agent",
        },
        KeyBinding {
            key: KeyEvent::new(KeyCode::BackTab, KeyModifiers::SHIFT),
            description: "Prev agent",
            group: "Agent",
        },
        KeyBinding {
            key: KeyEvent::new(KeyCode::Char('t'), KeyModifiers::CONTROL),
            description: "Cycle variant",
            group: "Agent",
        },
        // Toggles
        KeyBinding {
            key: KeyEvent::new(KeyCode::Char('s'), KeyModifiers::CONTROL),
            description: "Toggle timestamps",
            group: "Toggles",
        },
        KeyBinding {
            key: KeyEvent::new(KeyCode::Char('y'), KeyModifiers::CONTROL),
            description: "Toggle thinking",
            group: "Toggles",
        },
    ]
}

/// Returns all leader-chord bindings for display in the help overlay.
pub fn all_leader_bindings() -> Vec<KeyBinding> {
    vec![
        KeyBinding {
            key: KeyEvent::new(KeyCode::Char('s'), KeyModifiers::NONE),
            description: "Status",
            group: "Leader",
        },
        KeyBinding {
            key: KeyEvent::new(KeyCode::Char('e'), KeyModifiers::NONE),
            description: "Export session",
            group: "Leader",
        },
        KeyBinding {
            key: KeyEvent::new(KeyCode::Char('t'), KeyModifiers::NONE),
            description: "Theme switch",
            group: "Leader",
        },
        KeyBinding {
            key: KeyEvent::new(KeyCode::Char('b'), KeyModifiers::NONE),
            description: "Toggle sidebar",
            group: "Leader",
        },
        KeyBinding {
            key: KeyEvent::new(KeyCode::Char('n'), KeyModifiers::NONE),
            description: "New session",
            group: "Leader",
        },
        KeyBinding {
            key: KeyEvent::new(KeyCode::Char('l'), KeyModifiers::NONE),
            description: "Session list",
            group: "Leader",
        },
        KeyBinding {
            key: KeyEvent::new(KeyCode::Char('a'), KeyModifiers::NONE),
            description: "Agent list",
            group: "Leader",
        },
        KeyBinding {
            key: KeyEvent::new(KeyCode::Char('m'), KeyModifiers::NONE),
            description: "Model list",
            group: "Leader",
        },
        KeyBinding {
            key: KeyEvent::new(KeyCode::Char('c'), KeyModifiers::NONE),
            description: "Compact session",
            group: "Leader",
        },
        KeyBinding {
            key: KeyEvent::new(KeyCode::Char('q'), KeyModifiers::NONE),
            description: "Quit",
            group: "Leader",
        },
        KeyBinding {
            key: KeyEvent::new(KeyCode::Char('u'), KeyModifiers::NONE),
            description: "Undo",
            group: "Leader",
        },
        KeyBinding {
            key: KeyEvent::new(KeyCode::Char('r'), KeyModifiers::NONE),
            description: "Redo",
            group: "Leader",
        },
        KeyBinding {
            key: KeyEvent::new(KeyCode::Char('y'), KeyModifiers::NONE),
            description: "Copy message",
            group: "Leader",
        },
        KeyBinding {
            key: KeyEvent::new(KeyCode::Char('h'), KeyModifiers::NONE),
            description: "Toggle conceal",
            group: "Leader",
        },
        KeyBinding {
            key: KeyEvent::new(KeyCode::Char('g'), KeyModifiers::NONE),
            description: "Session timeline",
            group: "Leader",
        },
        KeyBinding {
            key: KeyEvent::new(KeyCode::Char('x'), KeyModifiers::NONE),
            description: "Export session",
            group: "Leader",
        },
        KeyBinding {
            key: KeyEvent::new(KeyCode::Char('w'), KeyModifiers::NONE),
            description: "Toggle tool details",
            group: "Leader",
        },
        KeyBinding {
            key: KeyEvent::new(KeyCode::Char('i'), KeyModifiers::NONE),
            description: "Toggle timestamps",
            group: "Leader",
        },
        KeyBinding {
            key: KeyEvent::new(KeyCode::Char('k'), KeyModifiers::NONE),
            description: "Toggle thinking",
            group: "Leader",
        },
        KeyBinding {
            key: KeyEvent::new(KeyCode::Char('v'), KeyModifiers::NONE),
            description: "Toggle scrollbar",
            group: "Leader",
        },
        KeyBinding {
            key: KeyEvent::new(KeyCode::Char('p'), KeyModifiers::NONE),
            description: "Command palette",
            group: "Leader",
        },
        KeyBinding {
            key: KeyEvent::new(KeyCode::Char('f'), KeyModifiers::NONE),
            description: "Fork session",
            group: "Leader",
        },
        KeyBinding {
            key: KeyEvent::new(KeyCode::Char('d'), KeyModifiers::NONE),
            description: "Delete session",
            group: "Leader",
        },
        KeyBinding {
            key: KeyEvent::new(KeyCode::Char('?'), KeyModifiers::SHIFT),
            description: "Help",
            group: "Leader",
        },
        KeyBinding {
            key: KeyEvent::new(KeyCode::Char('j'), KeyModifiers::NONE),
            description: "Toggle animations",
            group: "Leader",
        },
        KeyBinding {
            key: KeyEvent::new(KeyCode::Char('o'), KeyModifiers::NONE),
            description: "Toggle file context",
            group: "Leader",
        },
        KeyBinding {
            key: KeyEvent::new(KeyCode::Char('z'), KeyModifiers::NONE),
            description: "Toggle diff wrap",
            group: "Leader",
        },
        KeyBinding {
            key: KeyEvent::new(KeyCode::Char('1'), KeyModifiers::NONE),
            description: "Quick switch 1",
            group: "Leader",
        },
        KeyBinding {
            key: KeyEvent::new(KeyCode::Char('9'), KeyModifiers::NONE),
            description: "Quick switch N",
            group: "Leader",
        },
        KeyBinding {
            key: KeyEvent::new(KeyCode::Down, KeyModifiers::NONE),
            description: "First child",
            group: "Leader",
        },
        KeyBinding {
            key: KeyEvent::new(KeyCode::Right, KeyModifiers::NONE),
            description: "Next child",
            group: "Leader",
        },
        KeyBinding {
            key: KeyEvent::new(KeyCode::Left, KeyModifiers::NONE),
            description: "Prev child",
            group: "Leader",
        },
        KeyBinding {
            key: KeyEvent::new(KeyCode::Up, KeyModifiers::NONE),
            description: "Parent session",
            group: "Leader",
        },
        KeyBinding {
            key: KeyEvent::new(KeyCode::Char('E'), KeyModifiers::NONE),
            description: "Open in editor",
            group: "Leader",
        },
        KeyBinding {
            key: KeyEvent::new(KeyCode::Char('A'), KeyModifiers::NONE),
            description: "Toggle audio",
            group: "Leader",
        },
    ]
}
