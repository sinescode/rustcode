//! Interactive prompt helpers (select, text, password, confirm, autocomplete, spinner).
//!
//! Ported from: `packages/opencode/src/cli/ui.ts` — `@clack/prompts` usage
//!
//! Uses `dialoguer` for all interactive prompts and `indicatif` for spinners.
//! Every function writes to stderr so stdout remains clean for piping.

use std::io;

use dialoguer::{
    theme::ColorfulTheme,
    Confirm, FuzzySelect, Input, MultiSelect, Password, Select,
};
use indicatif::{ProgressBar, ProgressStyle};

// ── Types ─────────────────────────────────────────────────────────────────────

/// A selectable item with a display label, a return value, and an optional hint.
///
/// The `value` field is returned by [`prompt_select`] and [`prompt_multiselect`];
/// it need not correspond to the index in the slice.
pub struct SelectItem {
    pub label: String,
    pub value: usize,
    pub hint: Option<String>,
}

// ── SpinnerGuard ──────────────────────────────────────────────────────────────

/// A RAII guard that shows an animated spinner while alive and clears it on drop.
///
/// Create one via [`show_spinner`]. The spinner ticks in a background thread
/// until the guard is dropped (or `finish()` is called manually).
pub struct SpinnerGuard {
    spinner: Option<ProgressBar>,
}

impl Drop for SpinnerGuard {
    fn drop(&mut self) {
        if let Some(spinner) = &self.spinner {
            spinner.finish_and_clear();
        }
    }
}

impl SpinnerGuard {
    /// Set a new message on the running spinner.
    pub fn set_message(&self, msg: String) {
        if let Some(spinner) = &self.spinner {
            spinner.set_message(msg);
        }
    }

    /// Finish the spinner immediately (keeps the final message visible).
    pub fn finish(&self) {
        if let Some(spinner) = &self.spinner {
            spinner.finish_with_message("done");
        }
    }
}

// ── Helpers ───────────────────────────────────────────────────────────────────

fn theme() -> ColorfulTheme {
    ColorfulTheme::default()
}

/// Build display labels from [`SelectItem`] slice, appending hints in parens.
fn item_labels(items: &[SelectItem]) -> Vec<String> {
    items
        .iter()
        .map(|item| {
            let mut label = item.label.clone();
            if let Some(hint) = &item.hint {
                label.push_str(&format!(" ({hint})"));
            }
            label
        })
        .collect()
}

// ── Prompt functions ──────────────────────────────────────────────────────────

/// Interactive select with arrow keys.
///
/// Returns the `value` of the chosen [`SelectItem`].
pub fn prompt_select(items: &[SelectItem]) -> io::Result<usize> {
    let labels = item_labels(items);
    let selection = Select::with_theme(&theme())
        .items(&labels)
        .default(0)
        .interact()?;
    Ok(items[selection].value)
}

/// Text input with an optional default value.
///
/// If `default` is `Some`, the user can press Enter to accept it.
pub fn prompt_text(prompt: &str, default: Option<&str>) -> io::Result<String> {
    let mut input = Input::<String>::with_theme(&theme());
    input.with_prompt(prompt);
    if let Some(d) = default {
        input.default(d.to_string());
    }
    input.interact_text()
}

/// Password input (masked).
pub fn prompt_password(prompt: &str) -> io::Result<String> {
    Password::with_theme(&theme())
        .with_prompt(prompt)
        .interact()
}

/// Yes / no confirmation.
pub fn prompt_confirm(prompt: &str, default: bool) -> io::Result<bool> {
    Confirm::with_theme(&theme())
        .with_prompt(prompt)
        .default(default)
        .interact()
}

/// Autocomplete / fuzzy select from a list of strings.
///
/// The user types to filter items; pressing Enter selects the highlighted one.
pub fn prompt_autocomplete(items: &[String], prompt: &str) -> io::Result<String> {
    let selection = FuzzySelect::with_theme(&theme())
        .with_prompt(prompt)
        .items(items)
        .interact()?;
    Ok(items[selection].clone())
}

/// Multi-select with checkboxes.
///
/// Returns the `value` of every selected [`SelectItem`].
pub fn prompt_multiselect(items: &[SelectItem]) -> io::Result<Vec<usize>> {
    let labels = item_labels(items);
    let selections = MultiSelect::with_theme(&theme())
        .items(&labels)
        .interact()?;
    Ok(selections.iter().map(|&i| items[i].value).collect())
}

/// Show an animated spinner with the given message.
///
/// Returns a [`SpinnerGuard`] that auto-clears the spinner when dropped.
pub fn show_spinner(message: &str) -> SpinnerGuard {
    let spinner = ProgressBar::new_spinner();
    spinner.set_style(
        ProgressStyle::default_spinner()
            .tick_strings(&["⠋", "⠙", "⠹", "⠸", "⠼", "⠴", "⠦", "⠧", "⠇", "⠏"])
            .template("{spinner} {msg}")
            .expect("valid spinner template"),
    );
    spinner.set_message(message.to_string());
    spinner.enable_steady_tick(std::time::Duration::from_millis(80));
    SpinnerGuard {
        spinner: Some(spinner),
    }
}

// ── Print helpers ─────────────────────────────────────────────────────────────

/// Write a formatted message to stderr followed by a newline and ANSI reset.
///
/// This matches the TS `println()` in `packages/opencode/src/cli/ui.ts`:
/// it writes to stderr (not stdout) and resets attributes so trailing text
/// from other tools isn't accidentally styled.
///
/// # Example
///
/// ```ignore
/// println_stderr!("{TEXT_SUCCESS}✓{TEXT_NORMAL} Done");
/// ```
#[macro_export]
macro_rules! println_stderr {
    ($($arg:tt)*) => {{
        use std::io::Write;
        let _ = writeln!(&mut std::io::stderr(), "{}\x1b[0m", format!($($arg)*));
    }};
}
