//! ANSI style constants matching `packages/blazecode/src/cli/ui.ts` → `Style` object.
//!
//! Ported from: `packages/blazecode/src/cli/ui.ts` lines 14–29

/// Bright cyan foreground — used for highlights / emphasis.
pub const TEXT_HIGHLIGHT: &str = "\x1b[96m";

/// Bright cyan + bold.
pub const TEXT_HIGHLIGHT_BOLD: &str = "\x1b[96m\x1b[1m";

/// Bright black (gray) foreground — used for dim / muted text.
pub const TEXT_DIM: &str = "\x1b[90m";

/// Bright black + bold.
pub const TEXT_DIM_BOLD: &str = "\x1b[90m\x1b[1m";

/// Reset all attributes.
pub const TEXT_NORMAL: &str = "\x1b[0m";

/// Bold (no color change).
pub const TEXT_NORMAL_BOLD: &str = "\x1b[1m";

/// Bright yellow foreground — used for warnings.
pub const TEXT_WARNING: &str = "\x1b[93m";

/// Bright yellow + bold.
pub const TEXT_WARNING_BOLD: &str = "\x1b[93m\x1b[1m";

/// Bright red foreground — used for errors / danger.
pub const TEXT_DANGER: &str = "\x1b[91m";

/// Bright red + bold.
pub const TEXT_DANGER_BOLD: &str = "\x1b[91m\x1b[1m";

/// Bright green foreground — used for success / confirmation.
pub const TEXT_SUCCESS: &str = "\x1b[92m";

/// Bright green + bold.
pub const TEXT_SUCCESS_BOLD: &str = "\x1b[92m\x1b[1m";

/// Bright blue foreground — used for informational messages.
pub const TEXT_INFO: &str = "\x1b[94m";

/// Bright blue + bold.
pub const TEXT_INFO_BOLD: &str = "\x1b[94m\x1b[1m";
