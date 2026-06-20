# MCP Integration Gap Analysis: Rust vs TypeScript OpenCode

## Executive Summary

The TypeScript MCP implementation is a production-grade, feature-complete integration built on the official `@modelcontextprotocol/sdk`, with full OAuth authentication, paginated discovery, dynamic tool/resource/prompt listing, server notification handling, and Effect.ts-based service composition. The Rust port covers the foundational types, JSON-RPC framing, stdio/HTTP transport, and basic tool discovery/execution, but lacks OAuth auth flow, paginated discovery, resource/prompt operations, notification handlers, capability negotiation, and graceful process tree cleanup.

**Overall parity: ~40% (types + transport + basic tool ops only)**

---

## 1. Architecture

| Aspect | TypeScript | Rust | Gap |
|--------|-----------|------|-----|
| **SDK** | `@modelcontextprotocol/sdk` — official MCP client | Hand-rolled JSON-RPC 2.0 over stdio/HTTP | **HIGH** — Rust reimplements what the SDK provides; no access to SDK's bug fixes, protocol updates |
| **Service model** | Effect.ts `Service` + `Layer` + `InstanceState` (per-directory scoped) | `McpServerRegistry` + `McpClient` (manual struct-based DI) | **MEDIUM** — functional but less composable |
| **Concurrency** | `Effect.forEach({ concurrency: "unbounded" })` for parallel connection | Sequential `connect()` calls in registry | **MEDIUM** — Rust connects servers sequentially |
| **State management** | `InstanceState` with scoped cleanup on directory close | `DashMap` + `RwLock` (global, no per-directory scope) | **MEDIUM** — no per-project isolation |
| **File layout** | 5 files: `index.ts`, `catalog.ts`, `auth.ts`, `oauth-callback.ts`, `oauth-provider.ts` | 2 files: `mcp.rs` (core), `lib.rs` (mcp crate) | **LOW** — Rust is simpler but incomplete |

---

## 2. MCP Client

| Feature | TypeScript | Rust | Gap |
|---------|-----------|------|-----|
| **Client class** | `Client` from SDK (`@modelcontextprotocol/sdk/client/index.js`) | `McpClient` (custom) | **HIGH** — SDK handles protocol versioning, capability negotiation, reconnection |
| **Roots capability** | Sends `capabilities: { roots: {} }`, handles `ListRootsRequestSchema` | Sends `capabilities: {}` (empty), no roots handler | **MEDIUM** — servers requesting roots get no response |
| **Capability check** | `client.getServerCapabilities()?.tools` before tool listing | No capability checking — blindly calls `tools/list` | **MEDIUM** — may fail on servers without tool capability |
| **Protocol version** | `"2024-11-05"` (hardcoded in SDK) | `"2024-11-05"` (hardcoded) | **OK** — matches |
| **Client name** | `"opencode"` + `InstallationVersion` | `"rustcode"` + `CARGO_PKG_VERSION` | **OK** — matches pattern |
| **Initialize handshake** | Via SDK `client.connect(transport)` | Manual JSON-RPC `initialize` → response → `notifications/initialized` | **OK** — same flow, manual vs SDK |
| **Close/cleanup** | `client.close()` + descendant process killing (`pgrep -P`) | `child.kill_on_drop(true)` + `disconnect()` kills direct child only | **HIGH** — no descendant process tree cleanup |

---

## 3. Transport

| Feature | TypeScript | Rust | Gap |
|---------|-----------|------|-----|
| **Stdio transport** | `StdioClientTransport` from SDK | `StdioTransport` (custom) + inline in `McpClient` | **OK** — both implement Content-Length framing |
| **StreamableHTTP** | `StreamableHTTPClientTransport` from SDK | `HttpTransport` (custom HTTP POST) | **OK** — basic functionality present |
| **SSE transport** | `SSEClientTransport` from SDK | `RemoteSse` variant in `McpClient` + `SseEventStream` parser | **OK** — both handle SSE, Rust has more manual SSE parsing |
| **Transport fallback** | Tries StreamableHTTP first, falls back to SSE automatically | `connect()` uses HTTP (StreamableHTTP), `connect_http()` uses SSE — separate entry points | **MEDIUM** — TS auto-fallback, Rust requires manual selection |
| **Auth integration** | Transports accept `authProvider` parameter (McpOAuthProvider) | No auth integration in transports | **HIGH** — Rust transports cannot authenticate |
| **Abort signal** | `client.callTool(..., { signal: options.abortSignal })` | No abort/cancellation support | **MEDIUM** — no way to cancel in-flight tool calls |
| **Timeout** | `withTimeout(client.connect(t), timeout)` + per-call `timeout` | `std::time::Duration::from_millis(config.timeout)` on HTTP requests | **OK** — both support timeouts |

---

## 4. OAuth Authentication

| Feature | TypeScript | Rust | Gap |
|---------|-----------|------|-----|
| **OAuth provider** | `McpOAuthProvider` implementing `OAuthClientProvider` from SDK | Only `McpOAuthConfig` struct (config only) | **CRITICAL** — no OAuth implementation |
| **Callback server** | `McpOAuthCallback` — HTTP server on port 19876, handles redirect, state validation, CSRF protection | None | **CRITICAL** — no OAuth redirect handling |
| **Auth persistence** | `McpAuth` service — JSON file at `Global.Path.data/mcp-auth.json` with file locking | None | **CRITICAL** — no token persistence |
| **Dynamic registration** | SDK handles via `OAuthClientProvider.saveClientInformation()` | None | **CRITICAL** — can't auto-register with servers |
| **PKCE** | `saveCodeVerifier()` / `codeVerifier()` on auth provider | None | **CRITICAL** — OAuth PKCE not supported |
| **Token refresh** | `saveTokens()` / `tokens()` with expiry check | None | **CRITICAL** — no token refresh |
| **State management** | `updateOAuthState()` / `getOAuthState()` / CSRF validation in callback | `McpOAuthError::StateMismatch` variant defined but never triggered | **CRITICAL** — state validation types exist but no implementation |
| **Browser opening** | `open` package to launch browser, `BrowserOpenFailed` event on failure | `McpEvent::BrowserOpenFailed` variant defined | **MEDIUM** — event type defined but no browser opening |
| **OAuth status** | `getAuthStatus()` returns `authenticated`/`expired`/`not_authenticated` | `AuthStatus` enum defined | **OK** — types match |
| **Config options** | `clientId`, `clientSecret`, `scope`, `callbackPort`, `redirectUri` | Same fields in `McpOAuthConfig` | **OK** — config fields match |
| **Auto-detection** | OAuth attempted automatically on 401/unauthorized, falls back to `needs_auth` status | `oauth` field defaults to `None` (auto), but no auto-detection logic | **HIGH** — no automatic OAuth trigger |

---

## 5. Tool Discovery

| Feature | TypeScript | Rust | Gap |
|---------|-----------|------|-----|
| **List tools** | `McpCatalog.listTools()` with paginated cursor support | `McpClient::list_tools()` — single call, no pagination | **HIGH** — servers with >1000 tools will be truncated |
| **Pagination** | `paginate()` function: cursor-based, duplicate cursor detection, `MAX_LIST_PAGES = 1000` | None — assumes all tools in one response | **HIGH** — MCP spec supports pagination |
| **Tolerant parsing** | Catches `outputSchema` validation errors, falls back to `TolerantListToolsResultSchema` | No fallback — strict JSON parsing only | **MEDIUM** — servers with invalid outputSchema will fail |
| **Capability check** | `getServerCapabilities()?.tools` before listing | None — always tries to list | **MEDIUM** — may error on servers without tool support |
| **Tool caching** | `s.defs[name] = listed` stored in state, refreshed on `ToolListChangedNotification` | `tools: RwLock<Vec<McpTool>>` cached, `refresh_tools()` available | **OK** — both cache tools |
| **Tool key** | `sanitize(clientName) + "_" + sanitize(mcpTool.name)` | `tool_key(server_name, tool_name)` — same format | **OK** — matches |
| **Sanitize** | `value.replace(/[^a-zA-Z0-9_-]/g, "_")` | Same regex logic in `sanitize_name()` | **OK** — matches |
| **Tool definition** | `convertTool()` creates `dynamicTool` with `jsonSchema` input, progress reset, abort signal | `to_plugin_defs()` creates `PluginToolDef` with stub execute function | **MEDIUM** — Rust stubs route through session runner, less direct |

---

## 6. Tool Execution

| Feature | TypeScript | Rust | Gap |
|---------|-----------|------|-----|
| **callTool** | `client.callTool({ name, arguments }, CallToolResultSchema, { signal, timeout, onprogress })` | `McpClient::call_tool(tool_name, arguments)` — simple JSON-RPC call | **HIGH** — no abort signal, no progress reset, no structured result schema |
| **Error handling** | Checks `result.isError`, extracts text content, throws meaningful error | Checks `result.error` in JSON-RPC response | **MEDIUM** — Rust doesn't handle MCP-level `isError` flag |
| **Structured content** | Handles `result.structuredContent` — serializes to JSON text | No structured content handling | **MEDIUM** — MCP 2025+ feature not supported |
| **Progress** | `resetTimeoutOnProgress: true`, `onprogress: () => {}` to keep SDK sending progress tokens | None | **MEDIUM** — long-running tools may time out |
| **McpToolExecutor** | N/A (tools registered via catalog) | `McpToolExecutor` wraps `McpClient` for single tool | **OK** — Rust has cleaner per-tool abstraction |

---

## 7. Resource Discovery & Access

| Feature | TypeScript | Rust | Gap |
|---------|-----------|------|-----|
| **List resources** | `McpCatalog.resources()` — paginated, capability-gated | `McpResource` struct defined, no `list_resources()` | **CRITICAL** — resource listing not implemented |
| **Read resource** | `client.readResource({ uri })` via service interface | None | **CRITICAL** — resource reading not implemented |
| **Pagination** | Uses `paginate()` for resources | None | **HIGH** — same pagination gap as tools |
| **Capability check** | `getServerCapabilities()?.resources` before listing | None | **MEDIUM** |
| **Resource type** | `ResourceInfo & { client: string }` with `name`, `uri`, `description`, `mimeType` | `McpResource` with `name`, `uri`, `description`, `mime_type` | **OK** — fields match |

---

## 8. Prompt Discovery & Access

| Feature | TypeScript | Rust | Gap |
|---------|-----------|------|-----|
| **List prompts** | `McpCatalog.prompts()` — paginated, capability-gated | `McpPrompt` struct defined, no `list_prompts()` | **CRITICAL** — prompt listing not implemented |
| **Get prompt** | `client.getPrompt({ name, arguments })` via service interface | None | **CRITICAL** — prompt retrieval not implemented |
| **Pagination** | Uses `paginate()` for prompts | None | **HIGH** |
| **Capability check** | `getServerCapabilities()?.prompts` before listing | None | **MEDIUM** |
| **Prompt type** | `PromptInfo & { client: string }` with `name`, `description`, `arguments` | `McpPrompt` with `name`, `description`, `arguments: Vec<McpPromptArgument>` | **OK** — fields match |

---

## 9. Event Handling & Notifications

| Feature | TypeScript | Rust | Gap |
|---------|-----------|------|-----|
| **Tool list changed** | `ToolListChangedNotificationSchema` handler → re-discovers tools, publishes `ToolsChanged` event | `McpEvent::ToolsChanged` defined but no notification handler registered | **HIGH** — dynamic tool changes not detected |
| **Logging** | `LoggingMessageNotificationSchema` handler → routes to `Effect.logDebug/Info/Warning/Error` by level | None | **MEDIUM** — server log messages lost |
| **Connection close** | `client.onclose` handler → updates status, publishes `ToolsChanged` | `connected: AtomicBool` flag, no callback | **MEDIUM** — no event on disconnect |
| **Browser open failed** | `BrowserOpenFailed` event published when browser launch fails | `McpEvent::BrowserOpenFailed` defined | **OK** — event type exists |
| **Toast notifications** | `TuiEvent.ToastShow` for auth prompts and errors | None | **MEDIUM** — no user-facing notifications |

---

## 10. Server Registry & State

| Feature | TypeScript | Rust | Gap |
|---------|-----------|------|-----|
| **State structure** | `State { config, status, clients, defs }` per-directory | `McpServerRegistry { clients: DashMap, configs: RwLock }` | **OK** — similar structure |
| **Config source** | `Config.Service.get().mcp` (from config file) | `McpDiscovery` from Claude Desktop, OpenCode, env vars | **OK** — Rust has more discovery sources |
| **Status tracking** | `Record<string, Status>` with connected/disabled/failed/needs_auth/needs_client_registration | `McpStatus` enum with same variants | **OK** — matches |
| **Add server** | `add(name, mcp)` → creates, stores, watches | `add_config(name, config)` → stores config only, no auto-connect | **MEDIUM** — Rust requires explicit `connect()` |
| **Connect** | `connect(name)` → looks up config, creates, stores, watches | `connect(name)` → looks up config, creates `McpClient`, stores | **OK** — similar flow |
| **Disconnect** | `disconnect(name)` → closes client, removes, publishes event | `disconnect(name)` → kills process, removes from map | **OK** — similar flow |
| **Watch** | `watch()` sets up `onclose`, `ToolListChangedNotification`, `LoggingMessageNotification` handlers | None | **HIGH** — no live monitoring |
| **Clear** | Finalizer closes all clients, kills descendants, clears pending OAuth | `clear()` disconnects all, clears configs | **OK** — Rust simpler but less thorough |
| **Summary** | `status()` returns all server statuses | `list_servers()` returns `Vec<McpServerSummary>` with name, config, connected, tools | **OK** — Rust has richer summary |

---

## 11. Configuration

| Feature | TypeScript | Rust | Gap |
|---------|-----------|------|-----|
| **Config format** | `ConfigMCPV1.Info` — `type: "local" | "remote"`, `command`, `url`, `headers`, `timeout`, `enabled`, `oauth`, `cwd`, `environment` | `McpServerConfig` — same fields plus `args` (separate from command) | **OK** — Rust has more granular args handling |
| **Claude Desktop** | Not directly supported (uses own config) | `McpDiscovery::from_claude_desktop_config()` — parses Claude Desktop format | **ADVANTAGE** — Rust has better multi-source discovery |
| **OpenCode config** | Via `Config.Service` | `McpDiscovery::from_opencode_config()` — parses `.opencode/config.json` | **OK** |
| **Environment** | Not directly supported | `McpDiscovery::from_env()` — `MCP_SERVERS` JSON or `MCP_SERVER_*` prefix vars | **ADVANTAGE** — Rust has env var discovery |
| **OAuth config** | `oauth: false | McpOAuthConfig` in config | `oauth: Option<McpOAuthConfig>` with custom deserializer for `false`/`null`/config | **OK** — both handle the three states |
| **Experimental** | `cfg.experimental?.mcp_timeout` for global timeout override | None | **LOW** — no experimental overrides |

---

## 12. Process Management

| Feature | TypeScript | Rust | Gap |
|---------|-----------|------|-----|
| **Child process** | `StdioClientTransport` from SDK with `stderr: "pipe"` | `tokio::process::Command` with `kill_on_drop(true)` | **OK** — both spawn subprocesses |
| **Descendant killing** | `descendants(pid)` via `pgrep -P` recursive traversal, sends SIGTERM to all | None — only kills direct child | **HIGH** — orphaned MCP server processes possible |
| **CWD resolution** | `path.resolve(baseDir, mcp.cwd)` with `InstanceState.directory` | `cwd` field in config (relative path) | **MEDIUM** — Rust doesn't resolve relative to project root |
| **Env inheritance** | `process.env` + MCP-specific env + `BUN_BE_BUN` for opencode subprocess | `config.env` only (no base env inheritance) | **HIGH** — Rust MCP servers won't see parent env vars like `PATH` |
| **stderr** | Piped and available via SDK | Piped but not read or logged | **LOW** — stderr output lost in Rust |

---

## 13. Test Coverage

| Area | TypeScript | Rust | Gap |
|------|-----------|------|-----|
| **Unit tests** | Inline via Effect test patterns | 80+ unit tests in `mcp.rs` and `lib.rs` covering types, serialization, JSON-RPC helpers, discovery parsing | **OK** — Rust has good unit test coverage |
| **Integration tests** | Implicit via Effect runtime tests | None (requires real MCP servers) | **MEDIUM** — no integration tests in either codebase |
| **Edge cases** | Tolerant tool listing, duplicate cursor detection, OAuth state mismatch | Content-Length parsing edge cases, stream parsing, config deserialization | **OK** — both cover edge cases in their domains |

---

## 14. Missing Features (Severity Ratings)

### CRITICAL — Blocks core MCP functionality

| # | Feature | Impact |
|---|---------|--------|
| C1 | **OAuth authentication flow** | Cannot connect to any MCP server requiring OAuth |
| C2 | **OAuth callback server** | Cannot receive authorization redirects |
| C3 | **OAuth token persistence** | Tokens lost on restart; must re-authenticate every session |
| C4 | **Dynamic client registration** | Cannot auto-register with OAuth servers |
| C5 | **PKCE code verifier** | OAuth security compromise for servers requiring PKCE |
| C6 | **Resource listing** (`resources/list`) | Cannot discover or use MCP resources |
| C7 | **Resource reading** (`resources/read`) | Cannot read MCP resources |
| C8 | **Prompt listing** (`prompts/list`) | Cannot discover or use MCP prompts |
| C9 | **Prompt retrieval** (`prompts/get`) | Cannot retrieve MCP prompts |

### HIGH — Significant functionality gap

| # | Feature | Impact |
|---|---------|--------|
| H1 | **Pagination** for tools/resources/prompts | Large tool sets truncated; MCP spec compliance issue |
| H2 | **Tool list changed notification** | Dynamic tool additions/removals not detected |
| H3 | **Transport auto-fallback** (StreamableHTTP → SSE) | Must manually choose transport |
| H4 | **Automatic OAuth trigger** on 401 | No automatic auth flow on unauthorized |
| H5 | **Process tree cleanup** (descendant killing) | Orphaned MCP server processes |
| H6 | **Environment variable inheritance** | MCP servers may fail due to missing `PATH` etc. |
| H7 | **Abort/cancellation** for tool calls | Cannot cancel long-running tools |
| H8 | **SDK dependency** | Rust re-implements what `@modelcontextprotocol/sdk` provides; misses future SDK updates |

### MEDIUM — Reduced functionality

| # | Feature | Impact |
|---|---------|--------|
| M1 | **Capability negotiation check** | May error on servers without tool/resource/prompt support |
| M2 | **Server logging** (`LoggingMessageNotification`) | Server diagnostic logs lost |
| M3 | **Connection close monitoring** (`onclose`) | No event-driven disconnect handling |
| M4 | **Tolerant tool listing** (outputSchema fallback) | Servers with invalid outputSchema fail |
| M5 | **Per-directory state scoping** | Global state shared across projects |
| M6 | **Structured content handling** (`structuredContent`) | MCP 2025+ feature unsupported |
| M7 | **Progress/reset timeout** for long tool calls | Long-running tools may time out |
| M8 | **CWD resolution** relative to project root | Relative `cwd` paths may not resolve correctly |
| M9 | **Toast notifications** for auth prompts | No user-facing auth guidance |
| H9 | **Roots capability** | Servers requesting roots get no response |

### LOW — Minor gaps

| # | Feature | Impact |
|---|---------|--------|
| L1 | **Experimental MCP timeout override** | No global timeout config |
| L2 | **stderr logging** for MCP processes | Debug output lost |
| L3 | **Browser opening** for OAuth | Event type defined but no browser launch |

---

## 15. What Rust Does Better

| Feature | Detail |
|---------|--------|
| **Multi-source discovery** | `McpDiscovery` supports Claude Desktop config, OpenCode config, `MCP_SERVERS` JSON env var, and `MCP_SERVER_*` prefix env vars — more sources than TS |
| **Transport trait abstraction** | `McpTransport` trait enables pluggable transports; TS uses concrete SDK classes |
| **Typed JSON-RPC structs** | `JsonRpcRequest`, `JsonRpcResponse`, `JsonRpcError` with proper serde derive; TS uses SDK types or raw JSON |
| **Server registry** | `McpServerRegistry` with `DashMap` for concurrent access; TS uses plain `Record` |
| **Test coverage** | 80+ unit tests covering types, serialization, framing, discovery parsing; TS tests are sparse for MCP |
| **SSE parser** | Standalone `SseEventStream` in `rustcode-core/src/sse.rs` — reusable for LLM providers too |
| **McpToolExecutor** | Clean per-tool abstraction with `to_plugin_def()` for registry integration |
| **Builder pattern** | `McpServerConfig::local().with_env().with_timeout().with_headers()` — ergonomic config construction |

---

## 16. Recommended Implementation Priority

### Phase 1 — Critical gaps (blocks production use)
1. OAuth callback HTTP server (port 19876)
2. OAuth token persistence (JSON file with file locking)
3. OAuth provider implementing the MCP SDK's `OAuthClientProvider` interface
4. PKCE support (code verifier generation/storage)
5. Dynamic client registration support

### Phase 2 — Core features (completes MCP spec compliance)
6. Paginated tool/resource/prompt listing
7. `resources/list` + `resources/read` operations
8. `prompts/list` + `prompts/get` operations
9. Tool list changed notification handler
10. Transport auto-fallback (StreamableHTTP → SSE)

### Phase 3 — Production hardening
11. Capability negotiation checks before operations
12. Abort/cancellation support for tool calls
13. Environment variable inheritance for child processes
14. Descendant process tree cleanup
15. Server logging notification handler
16. Connection close monitoring + event dispatch

### Phase 4 — Polish
17. Tolerant tool listing (outputSchema fallback)
18. Structured content handling
19. Per-directory state scoping
20. Toast notification integration
21. Progress/reset timeout for long-running tools
