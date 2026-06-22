//! Clipboard integration for the TUI.
//!
//! Provides cross-platform clipboard copy via OS-specific tools:
//! - Linux: `xclip -selection clipboard` (X11) or `wl-copy` (Wayland)
//! - macOS: `pbcopy`
//! - Fallback: returns false, caller should show a message
//!
//! Ported from: `packages/tui/src/util/clipboard.ts`

use std::io::Write;
use std::process::{Command, Stdio};

/// Copy text to the system clipboard.
///
/// Returns `true` if the text was successfully copied, `false` if no clipboard
/// tool is available.
pub fn copy_to_clipboard(text: &str) -> bool {
    // Try each clipboard tool in priority order.
    // Each tool receives text via stdin.

    #[cfg(target_os = "linux")]
    {
        // Try wl-copy first (Wayland), then xclip (X11)
        if try_clipboard_tool("wl-copy", &["--"], text) {
            return true;
        }
        if try_clipboard_tool("xclip", &["-selection", "clipboard"], text) {
            return true;
        }
        // Also try xsel as a fallback
        if try_clipboard_tool("xsel", &["--clipboard", "--input"], text) {
            return true;
        }
    }

    #[cfg(target_os = "macos")]
    {
        if try_clipboard_tool("pbcopy", &[], text) {
            return true;
        }
    }

    // On other platforms (Windows, etc.), no clipboard tool available
    false
}

/// Try to pipe text to a clipboard tool. Returns true on success.
fn try_clipboard_tool(cmd: &str, args: &[&str], text: &str) -> bool {
    match Command::new(cmd)
        .args(args)
        .stdin(Stdio::piped())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()
    {
        Ok(mut child) => {
            // Write text to stdin
            if let Some(ref mut stdin) = child.stdin {
                if stdin.write_all(text.as_bytes()).is_err() {
                    let _ = child.wait();
                    return false;
                }
            }
            // Wait for the process to finish
            match child.wait() {
                Ok(status) => status.success(),
                Err(_) => false,
            }
        }
        Err(_) => false,
    }
}

/// Check whether clipboard support is likely available on this system.
pub fn clipboard_available() -> bool {
    #[cfg(target_os = "linux")]
    {
        let has_wl_copy = Command::new("which")
            .arg("wl-copy")
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .status()
            .map(|s| s.success())
            .unwrap_or(false);

        let has_xclip = Command::new("which")
            .arg("xclip")
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .status()
            .map(|s| s.success())
            .unwrap_or(false);

        let has_xsel = Command::new("which")
            .arg("xsel")
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .status()
            .map(|s| s.success())
            .unwrap_or(false);

        has_wl_copy || has_xclip || has_xsel
    }

    #[cfg(target_os = "macos")]
    {
        Command::new("which")
            .arg("pbcopy")
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .status()
            .map(|s| s.success())
            .unwrap_or(true) // macOS always has pbcopy
    }

    #[cfg(not(any(target_os = "linux", target_os = "macos")))]
    {
        false
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_copy_empty_string() {
        // Copying empty text should still work if the tool exists
        let result = copy_to_clipboard("");
        // We can't assert true because clipboard tools may not be installed in CI,
        // but it should at least not panic.
        let _ = result;
    }

    #[test]
    fn test_clipboard_available_does_not_panic() {
        let _available = clipboard_available();
    }
}
