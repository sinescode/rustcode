//! Question / user input system.
//!
//! Ported from: `packages/opencode/src/question/*.ts`

/// Question to ask the user.
#[derive(Debug, Clone)]
pub struct Question {
    /// Question text
    pub text: String,
    /// Available options
    pub options: Vec<String>,
}

/// Answer from the user.
#[derive(Debug, Clone)]
pub struct Answer {
    /// Selected option index
    pub index: usize,
}
