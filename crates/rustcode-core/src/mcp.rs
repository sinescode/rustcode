//! MCP (Model Context Protocol) integration.
//!
//! Ported from: `packages/opencode/src/mcp/*.ts`

/// MCP client placeholder.
pub struct McpClient;

impl McpClient {
    /// Create a new MCP client.
    pub fn new() -> Self {
        Self
    }
}

impl Default for McpClient {
    fn default() -> Self {
        Self::new()
    }
}
