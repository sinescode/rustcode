//! TUI components — conversation view, input area, status line, permission dialog,
//! toast notifications, dialog stack, session sidebar, diff viewer, session list,
//! tool-specific rendering, timeline view, export dialog, subagent dialog, and
//! model selector.
//!
//! Ported from:
//! - `packages/tui/src/routes/session/index.tsx` (conversation view)
//! - `packages/tui/src/component/prompt/index.tsx` (input area)
//! - `packages/tui/src/routes/session/footer.tsx` (status line)
//! - `packages/tui/src/routes/session/permission.tsx` (permission prompt)
//! - `packages/tui/src/routes/session/question.tsx` (question prompt)
//! - `packages/tui/src/ui/toast.tsx` (toast notifications)
//! - `packages/tui/src/ui/dialog.tsx` (dialog stack)
//! - `packages/tui/src/routes/session/sidebar.tsx` (session sidebar)
//! - `packages/tui/src/feature-plugins/system/diff-viewer.tsx` (diff viewer)
//! - `packages/tui/src/component/dialog-session-list.tsx` (session list)
//! - `packages/tui/src/routes/session/index.tsx` (tool rendering)
//! - `packages/tui/src/component/dialog-session-timeline.tsx` (timeline tree view)
//! - `packages/tui/src/component/dialog-session-export.tsx` (export dialog)
//! - `packages/tui/src/component/dialog-subagent.tsx` (subagent management)
//! - `packages/tui/src/component/dialog-model-selector.tsx` (model selector)

pub mod conversation;
pub mod dialog;
pub mod diff;
pub mod export_dialog;
pub mod input;
pub mod model_selector;
pub mod permission;
pub mod question;
pub mod session_list;
pub mod sidebar;
pub mod status;
pub mod subagent;
pub mod timeline;
pub mod toast;
pub mod tool_render;
