//! Text formatting utilities.
//!
//! Ported from: `packages/opencode/src/format/*.ts`

/// Format a diff for display.
pub fn format_diff(diff: &str) -> String {
    // Simple ANSI-colored diff output
    let mut output = String::new();
    for line in diff.lines() {
        if line.starts_with('+') && !line.starts_with("+++") {
            output.push_str(&format!("\x1b[32m{line}\x1b[0m\n"));
        } else if line.starts_with('-') && !line.starts_with("---") {
            output.push_str(&format!("\x1b[31m{line}\x1b[0m\n"));
        } else if line.starts_with("@@") {
            output.push_str(&format!("\x1b[36m{line}\x1b[0m\n"));
        } else {
            output.push_str(line);
            output.push('\n');
        }
    }
    output
}
