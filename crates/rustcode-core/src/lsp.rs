//! LSP integration.
//!
//! Ported from: `packages/opencode/src/lsp/*.ts`

/// LSP client placeholder.
pub struct LspClient;

impl LspClient {
    /// Create a new LSP client.
    pub fn new() -> Self {
        Self
    }
}

impl Default for LspClient {
    fn default() -> Self {
        Self::new()
    }
}
