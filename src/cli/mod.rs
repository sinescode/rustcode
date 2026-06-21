//! CLI interactive prompts and ANSI style constants.
//!
//! Ported from: `packages/opencode/src/cli/ui.ts`

pub mod prompts;
pub mod styles;

pub use prompts::{
    prompt_autocomplete, prompt_confirm, prompt_multiselect, prompt_password, prompt_select,
    prompt_text, show_spinner, SelectItem, SpinnerGuard,
};
pub use styles::*;
