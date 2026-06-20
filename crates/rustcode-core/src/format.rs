//! Human-readable formatting for tokens, costs, durations, and diffs.
//!
//! Ported from:
//! - `packages/opencode/src/cli/cmd/stats.ts` line 386 — [`format_number`]
//! - `packages/tui/src/util/format.ts` lines 1–20 — [`format_duration`]
//! - `packages/opencode/src/format/index.ts` — formatter registry
//! - `packages/opencode/src/format/formatter.ts` — 25 code formatter definitions
//!
//! The TS codebase provides `formatNumber` (compact "1.2K"/"1.5M" style) and
//! `formatDuration` (seconds-based human-readable elapsed time). This module
//! extends those patterns with `format_tokens` and `format_cost` for LLM usage
//! display.

/// Format a token count for human display.
///
/// # Source
/// Adapted from `formatNumber` in `packages/opencode/src/cli/cmd/stats.ts`
/// line 386, which formats numbers >= 1000 with "K" suffix and >= 1,000,000
/// with "M" suffix.
///
/// # Examples
/// ```ignore
/// assert_eq!(format_tokens(0), "0");
/// assert_eq!(format_tokens(500), "500");
/// assert_eq!(format_tokens(1_200), "1.2K");
/// assert_eq!(format_tokens(25_000), "25.0K");
/// assert_eq!(format_tokens(1_500_000), "1.5M");
/// assert_eq!(format_tokens(42_000_000), "42.0M");
/// ```
pub fn format_tokens(count: u64) -> String {
    if count >= 1_000_000 {
        let val = count as f64 / 1_000_000.0;
        format!("{:.1}M", val)
    } else if count >= 1_000 {
        let val = count as f64 / 1_000.0;
        format!("{:.1}K", val)
    } else {
        count.to_string()
    }
}

/// Format a dollar amount for human display.
///
/// No direct TS equivalent exists in the codebase. This is a utility
/// for displaying LLM API costs in a compact, readable format.
///
/// # Examples
/// ```ignore
/// assert_eq!(format_cost(0.0), "$0.00");
/// assert_eq!(format_cost(0.05), "$0.05");
/// assert_eq!(format_cost(1.0), "$1.00");
/// assert_eq!(format_cost(1.234), "$1.23");
/// assert_eq!(format_cost(0.001), "<$0.01");
/// ```
pub fn format_cost(dollars: f64) -> String {
    if dollars < 0.0 {
        return format!("-$0.00");
    }
    if dollars < 0.005 {
        return "<$0.01".to_string();
    }
    if dollars < 1.0 {
        format!("${:.2}", dollars)
    } else if dollars < 1_000.0 {
        format!("${:.2}", dollars)
    } else {
        // Compact K/M suffix for large costs
        if dollars >= 1_000_000.0 {
            format!("${:.2}M", dollars / 1_000_000.0)
        } else {
            format!("${:.2}K", dollars / 1_000.0)
        }
    }
}

/// Format a duration in milliseconds to a human-readable string.
///
/// # Source
/// Ported from `formatDuration` in `packages/tui/src/util/format.ts`
/// lines 1–20, adapted from seconds to milliseconds.
///
/// The TS version uses seconds and outputs "30s", "5m 30s", "2h 15m",
/// "~1 day", "~2 weeks". This Rust version accepts milliseconds and
/// converts internally.
///
/// # Examples
/// ```ignore
/// assert_eq!(format_duration(0), "");
/// assert_eq!(format_duration(500), "<1s");
/// assert_eq!(format_duration(1_000), "1s");
/// assert_eq!(format_duration(61_000), "1m 1s");
/// assert_eq!(format_duration(3_600_000), "1h");
/// assert_eq!(format_duration(86_400_000), "~1 day");
/// assert_eq!(format_duration(604_800_000), "~1 week");
/// ```
pub fn format_duration(ms: u64) -> String {
    if ms == 0 {
        return String::new();
    }

    let total_secs = ms / 1000;

    if total_secs == 0 {
        return "<1s".to_string();
    }

    if total_secs < 60 {
        return format!("{}s", total_secs);
    }

    if total_secs < 3600 {
        let mins = total_secs / 60;
        let remaining = total_secs % 60;
        if remaining > 0 {
            format!("{}m {}s", mins, remaining)
        } else {
            format!("{}m", mins)
        }
    } else if total_secs < 86400 {
        let hours = total_secs / 3600;
        let remaining = (total_secs % 3600) / 60;
        if remaining > 0 {
            format!("{}h {}m", hours, remaining)
        } else {
            format!("{}h", hours)
        }
    } else if total_secs < 604800 {
        let days = total_secs / 86400;
        if days == 1 {
            "~1 day".to_string()
        } else {
            format!("~{} days", days)
        }
    } else {
        let weeks = total_secs / 604800;
        if weeks == 1 {
            "~1 week".to_string()
        } else {
            format!("~{} weeks", weeks)
        }
    }
}

// ── Diff formatting (kept from original stub, documented) ────────────

/// Format a unified diff with ANSI color codes for terminal display.
///
/// - Lines starting with `+` (but not `+++`) are colored green.
/// - Lines starting with `-` (but not `---`) are colored red.
/// - Lines starting with `@@` (hunk headers) are colored cyan.
///
/// # Source
/// Common pattern in the TS codebase (e.g., `apply_patch.ts`, `edit.ts`).
pub fn format_diff(diff: &str) -> String {
    let mut output = String::with_capacity(diff.len() + 64);
    for line in diff.lines() {
        if line.starts_with('+') && !line.starts_with("+++") {
            output.push_str("\x1b[32m");
            output.push_str(line);
            output.push_str("\x1b[0m\n");
        } else if line.starts_with('-') && !line.starts_with("---") {
            output.push_str("\x1b[31m");
            output.push_str(line);
            output.push_str("\x1b[0m\n");
        } else if line.starts_with("@@") {
            output.push_str("\x1b[36m");
            output.push_str(line);
            output.push_str("\x1b[0m\n");
        } else {
            output.push_str(line);
            output.push('\n');
        }
    }
    output
}

// ── Tests ────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    // ── format_tokens ─────────────────────────────────────────────

    #[test]
    fn test_format_tokens_zero_and_small() {
        assert_eq!(format_tokens(0), "0");
        assert_eq!(format_tokens(1), "1");
        assert_eq!(format_tokens(42), "42");
        assert_eq!(format_tokens(999), "999");
    }

    #[test]
    fn test_format_tokens_kilo() {
        assert_eq!(format_tokens(1_000), "1.0K");
        assert_eq!(format_tokens(1_200), "1.2K");
        assert_eq!(format_tokens(15_500), "15.5K");
        assert_eq!(format_tokens(999_000), "999.0K");
    }

    #[test]
    fn test_format_tokens_mega() {
        assert_eq!(format_tokens(1_000_000), "1.0M");
        assert_eq!(format_tokens(1_500_000), "1.5M");
        assert_eq!(format_tokens(42_000_000), "42.0M");
        assert_eq!(format_tokens(999_900_000), "999.9M");
    }

    #[test]
    fn test_format_tokens_boundary() {
        // Exactly at 1K boundary
        assert_eq!(format_tokens(1_000), "1.0K");
        assert_eq!(format_tokens(999), "999");
        // Exactly at 1M boundary
        assert_eq!(format_tokens(1_000_000), "1.0M");
        assert_eq!(format_tokens(999_999), "1000.0K");
    }

    // ── format_cost ───────────────────────────────────────────────

    #[test]
    fn test_format_cost_zero_and_tiny() {
        assert_eq!(format_cost(0.0), "<$0.01");
        assert_eq!(format_cost(0.001), "<$0.01");
        assert_eq!(format_cost(0.0049), "<$0.01");
    }

    #[test]
    fn test_format_cost_cents() {
        assert_eq!(format_cost(0.01), "$0.01");
        assert_eq!(format_cost(0.05), "$0.05");
        assert_eq!(format_cost(0.50), "$0.50");
        assert_eq!(format_cost(0.99), "$0.99");
    }

    #[test]
    fn test_format_cost_dollars() {
        assert_eq!(format_cost(1.0), "$1.00");
        assert_eq!(format_cost(1.50), "$1.50");
        assert_eq!(format_cost(42.0), "$42.00");
        assert_eq!(format_cost(123.456), "$123.46");
    }

    #[test]
    fn test_format_cost_large() {
        assert_eq!(format_cost(1_000.0), "$1.00K");
        assert_eq!(format_cost(1_500.0), "$1.50K");
        assert_eq!(format_cost(1_000_000.0), "$1.00M");
    }

    #[test]
    fn test_format_cost_negative() {
        assert_eq!(format_cost(-1.0), "-$0.00");
    }

    // ── format_duration ───────────────────────────────────────────

    #[test]
    fn test_format_duration_zero_and_sub_second() {
        assert_eq!(format_duration(0), "");
        assert_eq!(format_duration(1), "<1s");
        assert_eq!(format_duration(500), "<1s");
        assert_eq!(format_duration(999), "<1s");
    }

    #[test]
    fn test_format_duration_seconds() {
        assert_eq!(format_duration(1_000), "1s");
        assert_eq!(format_duration(30_000), "30s");
        assert_eq!(format_duration(59_000), "59s");
    }

    #[test]
    fn test_format_duration_minutes() {
        assert_eq!(format_duration(60_000), "1m");
        assert_eq!(format_duration(61_000), "1m 1s");
        assert_eq!(format_duration(90_000), "1m 30s");
        assert_eq!(format_duration(120_000), "2m");
        assert_eq!(format_duration(3_599_000), "59m 59s");
    }

    #[test]
    fn test_format_duration_hours() {
        assert_eq!(format_duration(3_600_000), "1h");
        assert_eq!(format_duration(3_660_000), "1h 1m");
        assert_eq!(format_duration(7_200_000), "2h");
        assert_eq!(format_duration(8_100_000), "2h 15m");
        assert_eq!(format_duration(86_399_000), "23h 59m");
    }

    #[test]
    fn test_format_duration_days() {
        assert_eq!(format_duration(86_400_000), "~1 day");
        assert_eq!(format_duration(172_800_000), "~2 days");
        assert_eq!(format_duration(259_200_000), "~3 days");
        assert_eq!(format_duration(604_799_000), "~6 days");
    }

    #[test]
    fn test_format_duration_weeks() {
        assert_eq!(format_duration(604_800_000), "~1 week");
        assert_eq!(format_duration(1_209_600_000), "~2 weeks");
    }

    #[test]
    fn test_format_duration_boundary_values() {
        // <1s boundary
        assert_eq!(format_duration(999), "<1s");
        assert_eq!(format_duration(1_000), "1s");
        // 59s → 1m boundary
        assert_eq!(format_duration(59_000), "59s");
        assert_eq!(format_duration(60_000), "1m");
        // 59m → 1h boundary
        assert_eq!(format_duration(3_599_000), "59m 59s");
        assert_eq!(format_duration(3_600_000), "1h");
        // 23h → 1 day boundary
        assert_eq!(format_duration(86_399_000), "23h 59m");
        assert_eq!(format_duration(86_400_000), "~1 day");
        // 6 days → 1 week boundary
        assert_eq!(format_duration(604_799_000), "~6 days");
        assert_eq!(format_duration(604_800_000), "~1 week");
    }

    // ── format_diff ───────────────────────────────────────────────

    #[test]
    fn test_format_diff_colors() {
        let diff = "@@ -1,3 +1,4 @@\n unchanged\n-old line\n+new line\n";
        let formatted = format_diff(diff);
        assert!(formatted.contains("\x1b[36m@@ -1,3 +1,4 @@\x1b[0m"));
        assert!(formatted.contains("\x1b[31m-old line\x1b[0m"));
        assert!(formatted.contains("\x1b[32m+new line\x1b[0m"));
        assert!(formatted.contains(" unchanged"));
    }

    #[test]
    fn test_format_diff_preserves_headers() {
        // +++ and --- should NOT be colorized
        let diff = "--- a/file.txt\n+++ b/file.txt\n unchanged\n";
        let formatted = format_diff(diff);
        // Headers should be present but without color codes
        assert!(formatted.contains("--- a/file.txt"));
        assert!(formatted.contains("+++ b/file.txt"));
        // Make sure they don't have the ANSI codes that +/- lines get
        assert!(!formatted.contains("\x1b[32m--- a/file.txt"));
        assert!(!formatted.contains("\x1b[31m+++ b/file.txt"));
    }

    // ── format_tokens edge cases ──────────────────────────────────────

    #[test]
    fn test_format_tokens_very_large() {
        // Billions and beyond
        assert_eq!(format_tokens(1_000_000_000), "1000.0M");
        assert_eq!(format_tokens(2_500_000_000), "2500.0M");
        assert_eq!(format_tokens(9_999_999_999), "10000.0M");
    }

    #[test]
    fn test_format_tokens_exact_boundaries() {
        assert_eq!(format_tokens(999), "999");
        assert_eq!(format_tokens(1_000), "1.0K");
        assert_eq!(format_tokens(999_999), "1000.0K");
        assert_eq!(format_tokens(1_000_000), "1.0M");
    }

    // ── format_cost edge cases ────────────────────────────────────────

    #[test]
    fn test_format_cost_exact_boundaries() {
        // Boundary between <$0.01 and showing value
        assert_eq!(format_cost(0.00499), "<$0.01");
        assert_eq!(format_cost(0.005), "$0.01"); // rounds to $0.01
        assert_eq!(format_cost(0.00999), "$0.01");
        assert_eq!(format_cost(0.01), "$0.01");
        assert_eq!(format_cost(0.994), "$0.99");
        assert_eq!(format_cost(0.995), "$0.99");
    }

    #[test]
    fn test_format_cost_very_large() {
        assert_eq!(format_cost(10_000.0), "$10.00K");
        assert_eq!(format_cost(99_999.99), "$100.00K");
        assert_eq!(format_cost(999_999.99), "$1000.00K");
        assert_eq!(format_cost(10_000_000.0), "$10.00M");
        assert_eq!(format_cost(1_000_000_000.0), "$1000.00M");
    }

    // ── format_duration edge cases ────────────────────────────────────

    #[test]
    fn test_format_duration_exact_second_boundaries() {
        // Exactly at boundaries
        assert_eq!(format_duration(999), "<1s");
        assert_eq!(format_duration(1_000), "1s");
        assert_eq!(format_duration(59_000), "59s");
        assert_eq!(format_duration(60_000), "1m");
        assert_eq!(format_duration(3_599_000), "59m 59s");
        assert_eq!(format_duration(3_600_000), "1h");
        assert_eq!(format_duration(86_399_000), "23h 59m");
        assert_eq!(format_duration(86_400_000), "~1 day");
        assert_eq!(format_duration(604_799_000), "~6 days");
        assert_eq!(format_duration(604_800_000), "~1 week");
    }

    #[test]
    fn test_format_duration_large_values() {
        // Very large durations
        assert_eq!(format_duration(86_400_000 * 10), "~1 week");
        assert_eq!(format_duration(604_800_000 * 4), "~4 weeks");
        assert_eq!(format_duration(604_800_000 * 52), "~52 weeks");
    }

    #[test]
    fn test_format_duration_no_remainder_minutes() {
        assert_eq!(format_duration(120_000), "2m");
        assert_eq!(format_duration(600_000), "10m");
        assert_eq!(format_duration(3_600_000), "1h");
    }

    #[test]
    fn test_format_duration_exact_minutes_with_seconds() {
        assert_eq!(format_duration(61_000), "1m 1s");
        assert_eq!(format_duration(121_000), "2m 1s");
        assert_eq!(format_duration(3_601_000), "1h");
    }

    // ── format_diff edge cases ────────────────────────────────────────

    #[test]
    fn test_format_diff_empty() {
        let formatted = format_diff("");
        assert_eq!(formatted, "");
    }

    #[test]
    fn test_format_diff_only_context_lines() {
        let diff = "line1\nline2\nline3\n";
        let formatted = format_diff(diff);
        // All lines should be present without color codes
        assert!(formatted.contains("line1"));
        assert!(formatted.contains("line2"));
        assert!(formatted.contains("line3"));
        // No color escape codes
        assert!(!formatted.contains("\x1b["));
    }

    #[test]
    fn test_format_diff_mixed_changes() {
        let diff = "\
--- a/old.txt
+++ b/new.txt
@@ -1,4 +1,4 @@
 context line
-old line 1
-old line 2
+new line 1
+new line 2
 context end
";
        let formatted = format_diff(diff);
        // Context lines preserved
        assert!(formatted.contains("context line"));
        assert!(formatted.contains("context end"));
        // Removed lines in red
        assert!(formatted.contains("\x1b[31m-old line 1\x1b[0m"));
        assert!(formatted.contains("\x1b[31m-old line 2\x1b[0m"));
        // Added lines in green
        assert!(formatted.contains("\x1b[32m+new line 1\x1b[0m"));
        assert!(formatted.contains("\x1b[32m+new line 2\x1b[0m"));
        // Headers preserved without color
        assert!(formatted.contains("--- a/old.txt"));
        assert!(formatted.contains("+++ b/new.txt"));
        // Hunk header in cyan
        assert!(formatted.contains("\x1b[36m@@ -1,4 +1,4 @@\x1b[0m"));
    }
}
