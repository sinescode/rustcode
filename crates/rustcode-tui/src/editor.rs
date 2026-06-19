//! External editor integration for the TUI.
//!
//! Opens files in the user's preferred editor ($EDITOR or $VISUAL env var,
//! falling back to "vim" or "nano").
//!
//! Ported from: `packages/tui/src/util/editor.ts`

use std::env;
use std::process::{Command, Stdio};

/// Resolve the editor command to use.
///
/// Checks `$EDITOR`, then `$VISUAL`, falls back to "vim", then "nano".
pub fn resolve_editor() -> String {
    // Check $EDITOR first
    if let Ok(editor) = env::var("EDITOR") {
        if !editor.is_empty() {
            return editor;
        }
    }
    // Then $VISUAL
    if let Ok(editor) = env::var("VISUAL") {
        if !editor.is_empty() {
            return editor;
        }
    }
    // Fallback: check common editors
    for candidate in &["vim", "nvim", "nano", "vi"] {
        if editor_exists(candidate) {
            return candidate.to_string();
        }
    }
    // Absolute last resort
    "nano".to_string()
}

/// Check whether an editor binary exists on the system PATH.
fn editor_exists(name: &str) -> bool {
    Command::new("which")
        .arg(name)
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status()
        .map(|s| s.success())
        .unwrap_or(false)
}

/// Open a file in the user's preferred editor.
///
/// Blocks until the editor exits. Returns `true` if the editor was launched
/// successfully, `false` on error.
///
/// The editor process inherits stdin/stdout/stderr so the user can interact
/// with it directly in the terminal. The TUI should restore the terminal
/// to cooked mode before calling this and re-enable raw mode after.
pub fn open_in_editor(file_path: &str) -> bool {
    let editor = resolve_editor();

    match Command::new(&editor)
        .arg(file_path)
        .stdin(Stdio::inherit())
        .stdout(Stdio::inherit())
        .stderr(Stdio::inherit())
        .spawn()
    {
        Ok(mut child) => {
            // Wait for the editor to exit
            match child.wait() {
                Ok(status) => {
                    if !status.success() {
                        tracing::warn!(
                            "editor '{editor}' exited with status: {status}"
                        );
                    }
                    true
                }
                Err(e) => {
                    tracing::error!("failed to wait for editor '{editor}': {e}");
                    false
                }
            }
        }
        Err(e) => {
            tracing::error!("failed to launch editor '{editor}': {e}");
            false
        }
    }
}

/// Check if an editor is available (at least the fallback exists).
pub fn editor_available() -> bool {
    let editor = resolve_editor();
    editor_exists(&editor)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_resolve_editor_returns_something() {
        let editor = resolve_editor();
        assert!(!editor.is_empty());
    }

    #[test]
    fn test_editor_available_does_not_panic() {
        let _available = editor_available();
    }
}
