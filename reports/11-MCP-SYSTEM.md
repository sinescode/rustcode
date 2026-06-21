# MCP System — Gap Analysis

## Architecture

| Aspect | TS | Rust |
|--------|----|------|
| SDK | `@modelcontextprotocol/sdk` | Custom implementation |
| Transport | 3 SDK classes | `McpTransport` trait + 2 impls |
| Service | Effect-based | `McpServerRegistry` (DashMap) |
| OAuth | Full (provider + callback server) | **Missing OAuth flow** |

## Feature Parity

| Feature | TS | Rust | Status |
|---------|----|------|--------|
| Initialization handshake | SDK | `initialize` request/response | ✅ |
| Protocol version | SDK constant | Hardcoded `"2024-11-05"` | ⚠️ |
| `tools/list` | SDK | `list_tools()` | ✅ |
| `tools/call` | SDK | `call_tool()` | ✅ |
| Tool pagination | `paginate()` with cursor dedup | `mcp_paginate()` | ✅ |
| Prompt list/get | `catalog.ts:112-118` | `list_prompts()`/`get_prompt()` | ✅ |
| Resource list/read | `catalog.ts:120-126` | `list_resources()`/`read_resource()` | ✅ |
| Schema tolerance | `TolerantListToolsResultSchema` | **Missing** | **GAP** |
| Timeout reset on progress | `resetTimeoutOnProgress: true` | **Missing** | **GAP** |
| Tool result structuredContent | `catalog.ts:75-79` | **Missing** | **GAP** |

## Transport Comparison

| Transport | TS | Rust | Status |
|-----------|----|------|--------|
| Stdio | SDK `StdioClientTransport` | `StdioTransport` (lib.rs:103-355) | ✅ |
| StreamableHTTP | SDK class | `HttpTransport` (lib.rs:370-521) | ✅ |
| SSE | SDK `SSEClientTransport` | In `mcp.rs` | ✅ |
| Transport fallback | `connectRemote()` | `connect_with_fallback()` | ✅ |
| **Working directory (cwd)** | `path.resolve(baseDir, mcp.cwd)` | **cwd ignored** | **GAP** |
| **Child process cleanup** | Process tree via `descendants()` | Simple `child.kill()` | **GAP** |

## OAuth / Auth

| Feature | TS | Rust | Status |
|---------|----|------|--------|
| **OAuth Provider** | Implements SDK `OAuthClientProvider` | **Missing** | **CRITICAL** |
| **Dynamic client registration** | Full | **Missing** | **CRITICAL** |
| **PKCE flow** | codeVerifier/saveState | **Missing** | **CRITICAL** |
| **OAuth callback server** | Node HTTP server on port 19876 | **Missing** | **CRITICAL** |
| Token storage | McpAuth service | McpAuthStore | ✅ |
| Auth entry schema | Tokens, ClientInfo, Entry | McpAuthTokens, McpAuthEntry | ✅ |
| URL-validated token lookup | `getForUrl()` | `get_for_url()` | ✅ |

## Tool Integration

| Feature | TS | Rust | Status |
|---------|----|------|--------|
| Tool discovery | `create()→listTools()→defs()` | `connect()→list_tools()→to_plugin_defs()` | ✅ |
| **Tool execute** | Full via `client.callTool()` with progress/abort | **Stub** — placeholder text | **CRITICAL** |
| Tool key format | `sanitize(client) + "_" + toolName` | `tool_key()` | ✅ |
| Content extraction | `CallToolResultSchema` | `extract_mcp_content()` | ✅ |
| MCP web search tool | `mcp-websearch.ts:1-96` | **Missing** | **GAP** |

## CLI Commands

| Command | TS | Rust | Status |
|---------|----|------|--------|
| `mcp list` | Full (status, OAuth, type hints) | **Missing** | **GAP** |
| `mcp add` | Interactive + non-interactive | **Missing** | **GAP** |
| `mcp auth` | Start OAuth flow, browser, callback | **Missing** | **GAP** |
| `mcp auth list` | List OAuth status | **Missing** | **GAP** |
| `mcp logout` | Remove stored credentials | **Missing** | **GAP** |
| `mcp debug` | Connectivity, OAuth metadata | **Missing** | **GAP** |

## 5 Most Critical Gaps

### 1. OAuth Flow Entirely Missing
Remote MCP servers requiring OAuth (like `mcp.exa.ai`) cannot connect.

**TS**: `oauth-provider.ts` (206L), `oauth-callback.ts` (233L), `index.ts:748-898`

### 2. Tool Execution Is a Placeholder Stub
MCP tool execution returns a static string instead of actually calling the server.

**Rust**: `mcp.rs:1610-1623`

### 3. CLI Commands Missing
Users cannot manage MCP servers via CLI.

**TS**: `cli/cmd/mcp.ts` (849L)

### 4. No Tolerant Schema Fallback for `tools/list`
MCP servers with broken `outputSchema` fail entirely. TS gracefully degrades.

**TS**: `catalog.ts:14-16, 128-151`

### 5. No Tool Progress / Timeout Reset
Long-running MCP tool calls may time out unnecessarily.

**TS**: `catalog.ts:61-65`
