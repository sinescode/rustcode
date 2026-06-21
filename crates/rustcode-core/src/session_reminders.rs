//! Reminders system — load `.opencode/reminders/*.md` files and inject
//! their content into the system prompt as synthetic parts.
//!
//! Ported from: `packages/opencode/src/session/reminders.ts` (lines 1–92)

use std::path::PathBuf;

/// Result from applying reminders — modified messages with injected parts.
///
/// # Source
/// Ported from `packages/opencode/src/session/reminders.ts`.
#[derive(Debug, Clone)]
pub struct ReminderResult {
    /// The reminder text that was applied, if any.
    pub reminder_text: Option<String>,
    /// Whether any changes were made to the messages.
    pub applied: bool,
}

/// Load and apply session reminders.
///
/// Scans `.opencode/reminders/` and `.claude/reminders/` for `.md` files,
/// concatenates their content, and returns it for injection into the
/// system prompt.
///
/// # Source
/// Ported from `packages/opencode/src/session/reminders.ts` — the `apply` function
/// and the pattern of loading reminder files.
pub struct SessionReminders;

impl SessionReminders {
    /// Discover reminder directories relative to the given working directory.
    fn reminder_dirs(cwd: &std::path::Path) -> Vec<PathBuf> {
        vec![
            cwd.join(".opencode").join("reminders"),
            cwd.join(".claude").join("reminders"),
        ]
    }

    /// Load all reminder markdown files from all discoverable directories.
    ///
    /// Returns the concatenated text of all reminder files, separated by
    /// blank lines. Returns `None` if no reminders exist.
    ///
    /// # Source
    /// Ported from `packages/opencode/src/session/reminders.ts` — the file
    /// scanning pattern.
    pub fn load_reminders(cwd: &std::path::Path) -> Result<Option<String>, std::io::Error> {
        let mut all_text = String::new();

        for dir in Self::reminder_dirs(cwd) {
            if !dir.exists() || !dir.is_dir() {
                continue;
            }

            let mut entries: Vec<std::fs::DirEntry> = std::fs::read_dir(&dir)?
                .filter_map(|e| e.ok())
                .filter(|e| {
                    e.path()
                        .extension()
                        .map(|ext| ext == "md")
                        .unwrap_or(false)
                })
                .collect();

            entries.sort_by_key(|e| e.file_name());

            for entry in &entries {
                let content = std::fs::read_to_string(entry.path())?;
                if !content.trim().is_empty() {
                    if !all_text.is_empty() {
                        all_text.push_str("\n\n");
                    }
                    all_text.push_str(&content);
                }
            }
        }

        if all_text.is_empty() {
            Ok(None)
        } else {
            Ok(Some(all_text))
        }
    }

    /// Apply reminders to the messages for a session.
    ///
    /// Returns the reminder text that should be injected into the system
    /// prompt context, or `None` if no reminders exist.
    ///
    /// # Source
    /// Ported from `packages/opencode/src/session/reminders.ts` `apply` function.
    pub fn apply(cwd: &std::path::Path) -> ReminderResult {
        match Self::load_reminders(cwd) {
            Ok(Some(text)) => ReminderResult {
                reminder_text: Some(text),
                applied: true,
            },
            Ok(None) => ReminderResult {
                reminder_text: None,
                applied: false,
            },
            Err(e) => {
                tracing::warn!("Failed to load reminders: {e}");
                ReminderResult {
                    reminder_text: None,
                    applied: false,
                }
            }
        }
    }

    /// Format the reminders for inclusion in a system prompt.
    ///
    /// Wraps the reminder text in a `<reminders>` XML tag.
    pub fn format_for_prompt(reminders: &str) -> String {
        format!(
            "<reminders>\n{}\n</reminders>",
            reminders.trim()
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;

    fn setup_test_reminders() -> tempfile::TempDir {
        let dir = tempfile::tempdir().expect("create temp dir");
        let reminders_dir = dir.path().join(".opencode").join("reminders");
        std::fs::create_dir_all(&reminders_dir).expect("create reminders dir");

        let mut f1 = std::fs::File::create(reminders_dir.join("001-important.md")).expect("create file");
        write!(f1, "Remember to run `cargo test` before committing.").expect("write");

        let mut f2 = std::fs::File::create(reminders_dir.join("002-notes.md")).expect("create file");
        write!(f2, "Keep the CLAUDE.md rules in mind.").expect("write");

        dir
    }

    #[test]
    fn test_load_reminders_finds_files() {
        let dir = setup_test_reminders();
        let result = SessionReminders::load_reminders(dir.path()).expect("load reminders");
        assert!(result.is_some());
        let text = result.unwrap();
        assert!(text.contains("cargo test"));
        assert!(text.contains("CLAUDE.md"));
    }

    #[test]
    fn test_load_reminders_no_dir() {
        let dir = tempfile::tempdir().expect("create temp dir");
        // No reminders directory exists
        let result = SessionReminders::load_reminders(dir.path()).expect("load reminders");
        assert!(result.is_none());
    }

    #[test]
    fn test_apply_with_reminders() {
        let dir = setup_test_reminders();
        let result = SessionReminders::apply(dir.path());
        assert!(result.applied);
        assert!(result.reminder_text.is_some());
    }

    #[test]
    fn test_apply_without_reminders() {
        let dir = tempfile::tempdir().expect("create temp dir");
        let result = SessionReminders::apply(dir.path());
        assert!(!result.applied);
        assert!(result.reminder_text.is_none());
    }

    #[test]
    fn test_format_for_prompt() {
        let formatted = SessionReminders::format_for_prompt("Test reminder");
        assert!(formatted.contains("<reminders>"));
        assert!(formatted.contains("Test reminder"));
        assert!(formatted.contains("</reminders>"));
    }

    #[test]
    fn test_claude_reminders_dir() {
        let dir = tempfile::tempdir().expect("create temp dir");
        let claude_dir = dir.path().join(".claude").join("reminders");
        std::fs::create_dir_all(&claude_dir).expect("create dir");
        let mut f = std::fs::File::create(claude_dir.join("note.md")).expect("create file");
        write!(f, "Claude reminder").expect("write");

        let result = SessionReminders::load_reminders(dir.path()).expect("load");
        assert!(result.is_some());
        assert!(result.unwrap().contains("Claude reminder"));
    }
}
