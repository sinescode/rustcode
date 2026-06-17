//! TUI components — conversation view, input area, status line, permission dialog.
//!
//! Ported from:
//! - `packages/tui/src/routes/session/index.tsx` (conversation view)
//! - `packages/tui/src/component/prompt/index.tsx` (input area)
//! - `packages/tui/src/routes/session/footer.tsx` (status line)
//! - `packages/tui/src/routes/session/permission.tsx` (permission prompt)
//! - `packages/tui/src/routes/session/question.tsx` (question prompt)

pub mod conversation;
pub mod input;
pub mod permission;
pub mod question;
pub mod status;
