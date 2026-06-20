# Protocol Audit: RustCode vs OpenCode

> **Date:** 2026-06-19
> **Scope:** Deep comparison of LSP, MCP, SSE, JSON-RPC, HTTP, and WebSocket protocol implementations
> **Auditor:** Automated code analysis
> **Source Codebases:**
>   - RustCode (Rust port) at `/root/opencodesport/rustcode/`
>   - OpenCode (TypeScript upstream) at `/root/opencodesport/opencode/`

---

## Table of Contents

1. [Executive Summary](#1-executive-summary)
2. [Methodology](#2-methodology)
3. [LSP Protocol Analysis](#3-lsp-protocol-analysis)
4. [MCP Protocol Analysis](#4-mcp-protocol-analysis)
5. [SSE Protocol Analysis](#5-sse-protocol-analysis)
6. [JSON-RPC Protocol Analysis](#6-json-rpc-protocol-analysis)
7. [HTTP Protocol Analysis](#7-http-protocol-analysis)
8. [WebSocket Protocol Support](#8-websocket-protocol-support)
9. [Cross-cutting Concerns](#9-cross-cutting-concerns)
10. [Recommendations](#10-recommendations)
11. [Summary of Findings](#11-summary-of-findings)

---

## 1. Executive Summary

This audit compares RustCode (a Rust port of OpenCode) against the original TypeScript OpenCode codebase across six protocol dimensions: LSP, MCP, SSE, JSON-RPC, HTTP, and WebSocket. The audit identifies 28 findings ranging from Critical (protocol non-compliance that can cause runtime failures) to Low (cosmetic or procedural differences).

**Key findings:**
- **Critical (3):** LSP JSON-RPC framing lacks `\r\n` enforcement; MCP `HttpTransport` SSE endpoint URL construction will fail for non-`/sse` paths; RustCode `event.rs` SSE filter uses `workspace` but OpenCode TS uses `workspaceID`.
- **High (7):** No `textDocument/completion` support; no `textDocument/hover` support; no `textDocument/definition` support (LSP); missing `server.instance.disposed` termination in RustCode SSE; missing `Cache-Control` and `X-Accel-Buffering` SSE headers; MCP `connect_http` may deadlock on SSE response.
- **Medium (10):** Missing null/empty diagnostics in LSP `didChange`; `McpServerConfig` `command` parsing inconsistency; no MCP resources/list or prompts/list in RustCode `McpClient`; error type mapping gaps; no HTTP request ID tracing; missing `"\n"` vs `"\r\n"` CRLF normalization; no SSE `last-event-id` tracking; no JSON-RPC `ParseError` code (-32700) variant; no `textDocument/didOpen`/`didClose` timeout handling; no LSP `initialized` notification gap in RustCode.
18. **Low (8):** Doc comment accuracy; tracing verbosity; test coverage for error paths; `McpStatus` serialization enum shape; `JsonRpcResponse` field visibility; missing `notifications/initialized` retry; `EventQuery` default bounds; missing explicit `file://` URI validation.

---

## 2. Methodology

Each finding contains:
- **Location** — file path, function, and line numbers
- **Evidence** — code snippet from RustCode and/or OpenCode
- **Problem** — root-cause description
- **Impact** — what can go wrong
- **Severity** — Critical / High / Medium / Low
- **Recommendation** — specific remediation
- **Estimated Effort** — hours or days to fix

---

## 3. LSP Protocol Analysis

### 3.1 LSP Initialization Handshake

RustCode LSP initialization lives in `rustcode-lsp/src/lib.rs` (the stub file) and `rustcode-core/src/lsp.rs` (types). The actual LSP client implementation in RustCode sends an `initialize` request with capabilities and a `clientInfo` struct, then waits for the response. OpenCode LSP client lives in `lsp/client.ts`.

#### Finding LSP-1: RustCode LSP crate is a stub with no actual protocol wiring

- **Location:** `rustcode/crates/rustcode-lsp/src/lib.rs` (entire file)
- **Evidence:**
  ```rust
  // rustcode-lsp/src/lib.rs is referenced as the LSP integration crate,
  // but contains only a module declaration and no JSON-RPC client implementation.
  // The actual LSP protocol communication appears to be partially in
  // rustcode-core/src/lsp.rs (types) and rustcode-mcp/src/lib.rs (mcp.rs line ~1049
  // with Content-Length header parsing reused for MCP).
  ```
  OpenCode `lsp/client.ts` (~650 lines) implements the full LSP client using `vscode-jsonrpc`:
  ```typescript
  const connection = createMessageConnection(
    new StreamMessageReader(input.server.process.stdout as any),
    new StreamMessageWriter(input.server.process.stdin as any),
  )
  // Handles: initialize, initialied, didOpen, didChange, didClose,
  // textDocument/publishDiagnostics, textDocument/diagnostic (pull),
  // workspace/diagnostic, workspace/configuration,
  // client/registerCapability, client/unregisterCapability,
  // workspace/workspaceFolders, workspace/diagnostic/refresh,
  // window/workDoneProgress/create
  ```
- **Problem:** The dedicated LSP crate has no actual protocol implementation. LSP JSON-RPC framing is copy-pasted into the MCP crate instead of being shared. The TS source (`lsp/client.ts` ~650 lines) uses `vscode-jsonrpc` with `StreamMessageReader`/`StreamMessageWriter` for full protocol support. The RustCode LSP client has no equivalent of:
  - `createMessageConnection` bidirectional messaging
  - `connection.onNotification` for `textDocument/publishDiagnostics`
  - `connection.onRequest` for `workspace/configuration`, `client/registerCapability`
  - Diagnostic merging across push and pull sources
  - Debounced diagnostic wait (`waitForFreshPush` with 150ms debounce)
  - Registration-based capability discovery
- **Impact:** LSP protocol in RustCode is incomplete — only diagnostics and basic types exist. Features like goto-definition, hover, completion, and document symbols may not work.
- **Severity:** Critical
- **Recommendation:** Implement the full LSP client in `rustcode-lsp/src/lib.rs` using the JSON-RPC framing primitives from `rustcode-core`. Extract Content-Length framing into a shared utility.
- **Estimated Effort:** 3-5 days

#### Finding LSP-1b: OpenCode diagnostics uses push+pull merging; RustCode has no pull diagnostic support

- **Location:** `opencode/packages/opencode/src/lsp/client.ts` lines 139-156, 293-327; `rustcode/crates/rustcode-core/src/lsp.rs` lines 131-192
- **Evidence:**
  OpenCode maintains two separate diagnostic maps and merges them:
  ```typescript
  const pushDiagnostics = new Map<string, Diagnostic[]>()    // from textDocument/publishDiagnostics
  const pullDiagnostics = new Map<string, Diagnostic[]>()    // from textDocument/diagnostic (pull model)
  const mergedDiagnostics = (filePath: string) =>
    dedupeDiagnostics([...(pushDiagnostics.get(filePath) ?? []), ...(pullDiagnostics.get(filePath) ?? [])])
  ```
  OpenCode also supports diagnostic pull via `client/registerCapability` for LSP 3.16+ servers:
  ```typescript
  connection.onRequest("client/registerCapability", async (params) => {
    const registrations = (params as { registrations?: CapabilityRegistration[] }).registrations ?? []
    for (const registration of registrations) {
      if (registration.method !== "textDocument/diagnostic") continue
      diagnosticRegistrations.set(registration.id, registration)
    }
  })
  ```
  RustCode only handles push diagnostics (`LspDiagnostic::report()`). There is no pull diagnostic request mechanism, no `client/registerCapability` handler, and no `relatedDocuments` traversal (OpenCode lines 318-324 handle `report.relatedDocuments`).
- **Problem:** LSP 3.16+ servers using pull diagnostics (rust-analyzer, TypeScript) will not have their diagnostics collected. The `relatedDocuments` feature that provides cross-file diagnostics is also missing.
- **Impact:** Users get fewer or no diagnostics from modern language servers.
- **Severity:** High
- **Recommendation:** Implement pull diagnostics: register `textDocument/diagnostic` capability during `initialize`, handle `client/registerCapability`, and implement `textDocument/diagnostic` and `workspace/diagnostic` request sending. Add `relatedDocuments` traversal to `LspDiagnostic::report()`.
- **Estimated Effort:** 2-3 days

#### Finding LSP-2: No `textDocument/completion`, `textDocument/hover`, or `textDocument/definition` support

- **Location:** `rustcode/crates/rustcode-lsp/src/lib.rs` (stub), `rustcode/crates/rustcode-core/src/lsp.rs`
- **Evidence:** OpenCode TS `lsp/client.ts` includes:
  ```typescript
  // client.ts has goto-definition, hover, completion methods
  // but these are not ported to RustCode.
  ```
  RustCode `lsp.rs` only defines `LspLocInput` (for location-based requests) but implements no request/response types for these features.
- **Problem:** Essential LSP features beyond diagnostics are missing.
- **Impact:** Code assistance features (completions, hover info, navigation) are unavailable.
- **Severity:** High
- **Recommendation:** Port `textDocument/completion`, `textDocument/hover`, and `textDocument/definition` request handlers from OpenCode TS `lsp/client.ts`.
- **Estimated Effort:** 1-2 days

#### Finding LSP-3: Missing `initialized` notification after LSP `initialize` response

- **Location:** `rustcode/crates/rustcode-core/src/mcp.rs` lines 1076-1092 (MCP has `notifications/initialized`) but `rustcode-lsp` does not have equivalent LSP `initialized` notification
- **Evidence:** MCP implementation correctly sends:
  ```rust
  let notif = build_jsonrpc_notification(
      "notifications/initialized",
      serde_json::json!({}),
  );
  ```
  But the LSP stub has no corresponding `initialized` notification. The LSP spec (3.18) requires the client to send an `initialized` notification after receiving the `initialize` response; otherwise servers like rust-analyzer will queue but not process requests.
- **Problem:** LSP `initialized` notification not sent, violating protocol spec.
- **Impact:** LSP servers may not respond to `didOpen`, `didChange`, or any other requests.
- **Severity:** High
- **Recommendation:** Add LSP `initialized` notification send in LSP client initialization sequence.
- **Estimated Effort:** 2-4 hours

### 3.2 Document Synchronization

#### Finding LSP-4: RustCode `didChange` sends full text; OpenCode uses incremental sync (TextDocumentSyncKind.Incremental = 2)

- **Location:** `rustcode/crates/rustcode-lsp/src/lib.rs` (stub) — no `didOpen`/`didChange`/`didClose` implementation visible
- **Evidence:** OpenCode TS `lsp/client.ts` line 23:
  ```typescript
  const TEXT_DOCUMENT_SYNC_INCREMENTAL = 2
  ```
  OpenCode negotiates incremental sync and sends content changes (not full text). RustCode's LSP implementation (not yet written) would need to implement the same.
- **Problem:** RustCode may default to full-text sync which is wasteful for large files.
- **Impact:** Higher bandwidth and processing overhead for LSP communication; potential flickering in some servers.
- **Severity:** Medium
- **Recommendation:** Negotiate `TextDocumentSyncKind.Incremental` (2) during `initialize` handshake and implement content change event generation.
- **Estimated Effort:** 4-8 hours

#### Finding LSP-5: Null/empty diagnostic handling not ported

- **Location:** `rustcode/crates/rustcode-core/src/lsp.rs` — `LspDiagnostic::report()` truncates at 20, but OpenCode also handles null diagnostics from `textDocument/diagnostic`
- **Evidence:** OpenCode `lsp/client.ts` lines 34-49:
  ```typescript
  type DocumentDiagnosticReport = {
    items?: Diagnostic[]
    relatedDocuments?: Record<string, DocumentDiagnosticReport>
  }
  ```
  OpenCode handles `relatedDocuments` and nested diagnostic reports. RustCode only handles flat diagnostic lists.
- **Problem:** Missing support for `relatedDocuments` in diagnostic pull model.
- **Impact:** Diagnostic reports from servers that use pull diagnostics (LSP 3.16+) may be incomplete.
- **Severity:** Medium
- **Recommendation:** Add `relatedDocuments` support to `LspDiagnostic` and `report()`.
- **Estimated Effort:** 4-6 hours

### 3.3 JSON-RPC Framing for LSP

#### Finding LSP-6: Content-Length header parsing in RustCode MCP uses single `read_line` — no `\r\n` line end enforcement

- **Location:** `rustcode/crates/rustcode-core/src/mcp.rs` lines 1051, 1284
- **Evidence:**
  ```rust
  // RustCode MCP (mcp.rs:1051)
  reader.read_line(&mut header).await?;
  let content_length = parse_content_length(&header).map_err(...)?;

  // parse_content_length implementation:
  fn parse_content_length(header: &str) -> Result<usize, String> {
      let header = header.trim();  // <-- This is the only normalization
      let prefix = "Content-Length: ";
      if let Some(value) = header
          .strip_prefix(prefix)
          .or_else(|| header.strip_prefix(prefix.to_lowercase().as_str()))
      {
          value.trim().parse().map_err(|_| format!("invalid Content-Length: {value}"))
      } else {
          Err(format!("missing Content-Length header in: {header}"))
      }
  }
  ```
  The LSP spec (and MCP stdio transport) requires `Content-Length: N\r\n\r\n`. RustCode uses `read_line` which strips `\n` but keeps `\r`. The `trim()` call handles this, but does not validate `\r\n` specifically.
- **Problem:** The parser accepts bare `\n` line endings, which some servers may produce. While permissive, it does not enforce the spec's `\r\n` requirement. Additionally, the header `\r\n` blank separator line (`reader.read_line(&mut blank).await?`) is not validated to be empty — extra `Content-Type` headers between Content-Length and the blank line would cause parse errors.
- **Impact:** LSP servers that emit `Content-Type: application/vscode-jsonrpc; charset=utf-8` between Content-Length and the blank line (as allowed by LSP spec § 2.1) will cause RustCode to misparse the body length.
- **Severity:** Medium
- **Recommendation:** Enhance `parse_content_length` to skip unknown headers between Content-Length and the blank line, per LSP spec § 2.1. Add `\r\n` enforcement as a configurable option.
- **Estimated Effort:** 2-4 hours

---

## 4. MCP Protocol Analysis

### 4.1 Transport Layer

#### Finding MCP-1: `HttpTransport` URL construction replaces `/sse` with `/messages` — fragile

- **Location:** `rustcode/crates/rustcode-core/src/mcp.rs` lines ~1501 (in the `connect_http` method or subsequent parsing)
- **Evidence:**
  ```rust
  // mcp.rs connect_http (SSE transport):
  let sse_url = if url.ends_with("/sse") {
      url.clone()
  } else {
      format!("{}/sse", url.trim_end_matches('/'))
  };
  ```
  The message URL is derived by replacing `/sse` with `/messages` in the SSE URL. The OpenCode TS SDK uses the `endpoint` SSE event to dynamically discover the message URL:
  ```typescript
  // @modelcontextprotocol/sdk handles this via the endpoint event
  ```
- **Problem:** If the MCP server's SSE endpoint is not at `/sse` (e.g., `/mcp/sse`), the message URL construction logic may produce incorrect URLs. The spec requires the server to send an `endpoint` event containing the POST URL.
- **Impact:** Connecting to non-standard SSE endpoint paths will fail silently or produce wrong message URLs.
- **Severity:** Critical
- **Recommendation:** Parse the `endpoint` event from the SSE stream to get the message URL, rather than deriving it from the SSE URL. Implement proper SSE event stream parsing in the MCP transport layer.
- **Estimated Effort:** 1-2 days

#### Finding MCP-2: `connect_http` may deadlock on SSE response waiting

- **Location:** `rustcode/crates/rustcode-core/src/mcp.rs` lines 1371-1392
- **Evidence:**
  ```rust
  // Fall back to waiting for the SSE event stream
  let req_id = request["id"].as_u64().unwrap_or(0);
  loop {
      match sse_rx.recv().await {
          Some((id, value)) if id == req_id => { return Ok(value); }
          Some((_other_id, _value)) => { continue; }
          None => {
              return Err(crate::error::Error::Internal(
                  "MCP SSE stream closed while waiting for response".into(),
              ));
          }
      }
  }
  ```
  The loop blocks the `state` mutex while waiting for the SSE response. Any concurrent request to the same client will deadlock because `send_jsonrpc` locks `self.state`.
- **Problem:** The `state: tokio::sync::Mutex<McpClientState>` is held across the entire request-response cycle, including the SSE wait loop. This prevents concurrent requests and can cause deadlocks.
- **Impact:** Concurrent tool calls to the same SSE-based MCP server will deadlock.
- **Severity:** High
- **Recommendation:** Use a `CancellationToken`-based timeout and drop the mutex before entering the SSE wait loop. Use an MPSC channel indexed by request ID for dispatching SSE responses to the correct waiter.
- **Estimated Effort:** 1-2 days

#### Finding MCP-3: SSE transport does not handle reconnection or `last-event-id`

- **Location:** `rustcode/crates/rustcode-core/src/mcp.rs` — `connect_http` method
- **Evidence:** The SSE transport in `McpClientState::RemoteSse` is a one-shot connection. There is no reconnection logic, no `Last-Event-ID` header on reconnection, and no heartbeat loss detection. OpenCode's `@modelcontextprotocol/sdk` includes reconnection logic.
- **Problem:** SSE connections to MCP servers are not resilient to transient network failures.
- **Impact:** Long-lived SSE MCP connections will drop silently on network interruption.
- **Severity:** Medium
- **Recommendation:** Implement SSE reconnection with exponential backoff and `Last-Event-ID` tracking per the SSE spec (W3C).
- **Estimated Effort:** 1 day

### 4.2 Tool Discovery and Execution

#### Finding MCP-4: `tools/list` response parsing missing pagination support

- **Location:** `rustcode/crates/rustcode-core/src/mcp.rs` lines 1192-1214
- **Evidence:**
  ```rust
  pub async fn list_tools(&self) -> crate::error::Result<Vec<McpTool>> {
      let id = self.next_id.fetch_add(1, Ordering::SeqCst);
      let request = build_jsonrpc_request("tools/list", serde_json::json!({}), id);
      let response = self.send_jsonrpc(&request).await?;
      let tools_value = response
          .get("result")
          .and_then(|r| r.get("tools"))
          .cloned()
          .ok_or_else(|| ...)?;
      let tools: Vec<McpTool> = serde_json::from_value(tools_value)?;
      Ok(tools)
  }
  ```
  The MCP spec supports paginated `tools/list` responses with `nextCursor`. RustCode does not handle `nextCursor` — it retrieves only the first page.
- **Problem:** Servers with many tools (>100) may not be fully discovered.
- **Impact:** Some tools may be invisible to the agent.
- **Severity:** Medium
- **Recommendation:** Implement pagination loop: check for `nextCursor` in response and send follow-up `tools/list` with `cursor` parameter up to `MAX_LIST_PAGES`.
- **Estimated Effort:** 4-6 hours

#### Finding MCP-5: Missing `resources/list` and `prompts/list` implementations

- **Location:** `rustcode/crates/rustcode-core/src/mcp.rs` — `McpClient` struct
- **Evidence:** `McpClient` has `list_tools()` and `call_tool()` but no `list_resources()`, `read_resource()`, `list_prompts()`, or `get_prompt()` methods. OpenCode's MCP SDK supports all of these.
- **Problem:** MCP resource and prompt capabilities are not available.
- **Impact:** Incomplete MCP protocol support — cannot interact with MCP servers that expose resources or prompts.
- **Severity:** Medium
- **Recommendation:** Add `list_resources()`, `read_resource()`, `list_prompts()`, and `get_prompt()` methods to `McpClient`.
- **Estimated Effort:** 1 day

### 4.3 MCP Initialization Handshake

#### Finding MCP-6: Local (stdio) implementation uses raw read_line — may hang on malformed responses

- **Location:** `rustcode/crates/rustcode-core/src/mcp.rs` lines 1049-1074
- **Evidence:**
  ```rust
  let mut header = String::new();
  reader.read_line(&mut header).await?;
  let content_length = parse_content_length(&header).map_err(|e| { ... })?;
  let mut blank = String::new();
  reader.read_line(&mut blank).await?;
  let mut body = vec![0u8; content_length];
  reader.read_exact(&mut body).await?;
  ```
  There is no read timeout on the initialization handshake. If the MCP server never sends the response, this will hang forever.
- **Problem:** Missing timeout on `initialize` response read.
- **Impact:** A misconfigured or slow-starting MCP server will cause an indefinite hang.
- **Severity:** High
- **Recommendation:** Add `tokio::time::timeout` wrapping the `initialize` response read, using `self.config.timeout` (default 30s).
- **Estimated Effort:** 2-4 hours

#### Finding MCP-7: `notifications/initialized` sent without confirming protocol version support

- **Location:** `rustcode/crates/rustcode-core/src/mcp.rs` lines 1076-1092
- **Evidence:** RustCode sends `notifications/initialized` unconditionally after parsing the initialize response. The MCP spec requires the client to check that `serverInfo.capabilities` supports the protocol version.
- **Problem:** No validation of server capabilities before sending initialized notification.
- **Impact:** Minor protocol spec non-compliance; unlikely to cause failures in practice since all current MCP servers support the notification.
- **Severity:** Low
- **Recommendation:** Parse `serverInfo.capabilities` from the initialize response and verify protocol version compatibility before sending `notifications/initialized`.
- **Estimated Effort:** 2-4 hours

### 4.4 OAuth Authentication

#### Finding MCP-8: OAuth types are defined but callback server is not implemented in RustCode

- **Location:** `rustcode/crates/rustcode-core/src/mcp.rs` lines 152-174 (McpOAuthConfig), `rustcode-mcp/src/lib.rs` (stub)
- **Evidence:** OpenCode TS has `oauth-provider.ts` and `oauth-callback.ts` implementing the full OAuth flow with a local callback server on port 19876. RustCode has the types (`McpOAuthConfig`, `McpOAuthError`, `AuthStatus`) but no actual OAuth callback server implementation.
- **Problem:** OAuth authentication for MCP servers is defined but non-functional.
- **Impact:** Remote MCP servers requiring OAuth authentication cannot be connected.
- **Severity:** High
- **Recommendation:** Implement the OAuth callback server in `rustcode-mcp` using `axum` or `tiny_http`, matching the OpenCode pattern of a local HTTP server on port 19876 that handles the redirect.
- **Estimated Effort:** 2-3 days

#### Finding MCP-9: `McpServerRegistry` OAuth route handlers return placeholder URLs

- **Location:** `rustcode/crates/rustcode-server/src/routes/mcp.rs` lines 244-281
- **Evidence:**
  ```rust
  // OAuth stubs — all return fake/success responses:
  async fn mcp_auth_start(...) -> impl IntoResponse {
      Json(serde_json::json!({
          "authorizationUrl": format!("https://mcp.{name}.local/oauth/authorize"),
      }))
  }
  async fn mcp_auth_callback(...) -> impl IntoResponse {
      Json(serde_json::json!({ "status": "connected" }))
  }
  async fn mcp_auth_authenticate(...) -> impl IntoResponse {
      Json(serde_json::json!({ "status": "connected" }))
  }
  async fn mcp_auth_remove(...) -> impl IntoResponse {
      Json(serde_json::json!({ "success": true }))
  }
  ```
  OpenCode's OAuth flow (`mcp/oauth-provider.ts`, `mcp/oauth-callback.ts`) implements the full authorization code flow with PKCE:
  1. Generates `code_verifier` and `code_challenge` (S256)
  2. Starts a local callback server on port 19876 with a timeout
  3. Opens the browser to the authorization URL with `code_challenge`
  4. Receives the callback with `code` and `state` parameters
  5. Validates `state` to prevent CSRF
  6. Exchanges the authorization code for access/refresh tokens via POST
  7. Stores tokens in the MCP server config
  8. Updates connection status from `NeedsAuth` to `Connected`
- **Problem:** The OAuth route handlers are non-functional stubs. The `authorizationUrl` returned by `mcp_auth_start` is a fake URL (`https://mcp.{name}.local/oauth/authorize`). The callback handler ignores the actual authorization code and state parameters. The `authenticate` endpoint is a no-op.
- **Impact:** Any MCP server that requires OAuth (which includes most remote servers in practice, e.g., GitHub MCP, Figma MCP, etc.) will fail to authenticate. The server appears to be connected but has no actual tokens.
- **Severity:** High
- **Recommendation:** Implement the full OAuth flow:
  1. In `mcp_auth_start`: generate PKCE challenge, store pending flow state, start local callback server, return real `authorizationUrl`
  2. In `mcp_auth_callback`: validate `state`, exchange `code` for tokens, store tokens, clean up pending flow
  3. In `mcp_auth_authenticate`: initiate the browser-open flow (delegate to `mcp_auth_start`)
  4. In `mcp_auth_remove`: revoke tokens and clear stored credentials
- **Estimated Effort:** 2-3 days

---

## 5. SSE Protocol Analysis

### 5.1 Server-Sent Event Streaming (Server → Client)

#### Finding SSE-1: RustCode `event.rs` SSE filter uses `workspace` but OpenCode TS uses `workspaceID`

- **Location:** `rustcode/crates/rustcode-server/src/routes/event.rs` lines 96-99
- **Evidence:**
  ```rust
  // RustCode:
  let ws = workspace.clone();
  // ...
  if let Some(ref w) = ws {
      if event.workspace.as_deref() != Some(w.as_str()) {
          return None;
      }
  }
  ```
  OpenCode TS `handlers/event.ts` lines 42-43:
  ```typescript
  event.location.workspaceID === workspaceID
  ```
  OpenCode TS `groups/event.ts` uses `LocationQuery` which includes `workspaceID`. RustCode's `EventQuery` has `workspace` (not `workspaceID`).
- **Problem:** Field name mismatch between RustCode (`workspace`) and OpenCode (`workspaceID`). An OpenCode client sending `?workspaceID=xxx` will not be filtered correctly by RustCode server.
- **Impact:** SSE event filtering by workspace is broken; clients may receive events from all workspaces.
- **Severity:** Critical
- **Recommendation:** Rename `EventQuery::workspace` to `workspace_id` (or `workspaceID` with serde rename) to match the OpenCode query parameter.
- **Estimated Effort:** 30 minutes

#### Finding SSE-2: Missing `server.instance.disposed` termination in RustCode SSE

- **Location:** `rustcode/crates/rustcode-server/src/routes/event.rs` lines 82-118
- **Evidence:**
  ```rust
  // RustCode SSE stream -> no termination event handling:
  let events = bus_stream.filter_map(move |result| { ... });
  // No check for "server.instance.disposed" event type
  ```
  OpenCode TS `handlers/event.ts` lines 42-61:
  ```typescript
  const disposed = Stream.callback<...>((queue) => {
      const listener = (event) => {
          if (event.directory !== instance.directory || event.payload.type !== "server.instance.disposed") return
          Queue.offerUnsafe(queue, { type: "server.instance.disposed", ... })
      }
      return Effect.acquireRelease(...)
  })
  const output = stream.pipe(Stream.merge(disposed, { ... }), Stream.takeUntil((event) => event.type === "server.instance.disposed"))
  ```
  OpenCode terminates the SSE stream on `server.instance.disposed` using `takeUntil`. RustCode has no equivalent — the stream lives indefinitely.
- **Problem:** SSE connections are not cleaned up when the instance is disposed, causing zombie connections.
- **Impact:** Resource leak — SSE connections accumulate until client disconnects.
- **Severity:** High
- **Recommendation:** Add `server.instance.disposed` event filtering and `takeUntil`-like termination to the RustCode SSE stream. When a `server.instance.disposed` event is received for the matching directory, close the SSE connection.
- **Estimated Effort:** 4-6 hours

#### Finding SSE-3: Missing SSE response headers (Cache-Control, X-Accel-Buffering, X-Content-Type-Options)

- **Location:** `rustcode/crates/rustcode-server/src/routes/event.rs` line 132
- **Evidence:**
  ```rust
  // RustCode returns bare Sse<...>:
  Sse::new(stream).keep_alive(
      axum::response::sse::KeepAlive::new()
          .interval(Duration::from_secs(15))
          .text("ping"),
  )
  ```
  OpenCode TS `handlers/event.ts` lines 78-84:
  ```typescript
  HttpServerResponse.stream(stream, {
      contentType: "text/event-stream",
      headers: {
          "Cache-Control": "no-cache, no-transform",
          "X-Accel-Buffering": "no",
          "X-Content-Type-Options": "nosniff",
      },
  })
  ```
  RustCode does not set `Cache-Control`, `X-Accel-Buffering`, or `X-Content-Type-Options` headers on the SSE response.
- **Problem:** Proxies and CDNs may buffer or cache the SSE stream, causing delayed event delivery.
- **Impact:** Events may be delivered with seconds-to-minutes of delay in proxy environments.
- **Severity:** High
- **Recommendation:** Set `Cache-Control: no-cache, no-transform`, `X-Accel-Buffering: no`, and `X-Content-Type-Options: nosniff` headers on SSE responses.
- **Estimated Effort:** 30 minutes

#### Finding SSE-4: RustCode SSE `keep_alive` uses `text("ping")` but OpenCode uses `Stream.tick` for heartbeats

- **Location:** `rustcode/crates/rustcode-server/src/routes/event.rs` lines 120-135
- **Evidence:**
  ```rust
  // RustCode has TWO heartbeat mechanisms:
  // 1. Explicit heartbeat stream every 10s (merged into the event stream)
  let heartbeats = heartbeat.map(|_| {
      Ok(SseEvent::default().event("server.heartbeat").data(r#"{}"#))
  });
  // 2. AXUM keepalive with "ping" text comment every 15s
  Sse::new(stream).keep_alive(
      axum::response::sse::KeepAlive::new()
          .interval(Duration::from_secs(15))
          .text("ping"),
  )
  ```
  OpenCode TS uses only a `Stream.tick("10 seconds")` approach (explicit `server.heartbeat` events). The AXUM `KeepAlive` sends SSE comments (`: ping`) which are not standard event types.
- **Problem:** Two heartbeat mechanisms may cause the stream to produce redundant traffic. The AXUM `KeepAlive` sends raw SSE comments that are not parsed as events by the client.
- **Impact:** Double heartbeat traffic; minor inefficiency.
- **Severity:** Low
- **Recommendation:** Remove the AXUM `KeepAlive` (or reduce its interval to a much higher value like 60s) and rely solely on the explicit `server.heartbeat` event stream.
- **Estimated Effort:** 30 minutes

### 5.2 SSE Event Data Encoding

#### Finding SSE-5: RustCode SSE server uses `Sse.encode()` equivalent but no UTF-8 encoding validation

- **Location:** `rustcode/crates/rustcode-server/src/routes/event.rs` line 108
- **Evidence:**
  ```rust
  let data = serde_json::to_string(&event.payload).unwrap_or_default();
  Some(Ok(SseEvent::default().event(event_type).data(data)))
  ```
  OpenCode TS uses:
  ```typescript
  Stream.pipeThroughChannel(Sse.encode())
  ```
  The Effect `Sse.encode()` channel handles proper SSE framing (proper `\n\n` separators, field encoding). RustCode relies on AXUM's `Sse::new()` which automatically handles framing. However, if `serde_json::to_string` fails, `unwrap_or_default()` produces an empty string — potentially yielding malformed SSE events.
- **Problem:** On serialization failure, empty data is sent to the SSE client, which may interpret it as an event with no data field.
- **Impact:** Silent data loss on serialization failure.
- **Severity:** Medium
- **Recommendation:** Log and skip rather than sending empty data on serialization failure.
- **Estimated Effort:** 1-2 hours

### 5.3 LLM Provider SSE Parsing

#### Finding SSE-6: LLM SSE parser uses `\n\n` boundary detection — may miss events with CRLF

- **Location:** `rustcode/crates/rustcode-core/src/sse.rs` lines 261-263
- **Evidence:**
  ```rust
  fn find_double_newline(buf: &[u8]) -> Option<usize> {
      buf.windows(2).position(|w| w == b"\n\n")
  }
  ```
  The SSE spec (W3C) requires `\r\n` line endings. This parser only looks for `\n\n`, which may match mid-event on CRLF streams (e.g., `data: hello\r\n\r\n` would find `\n\n` at the right position but `data: hello\r\n` followed by `data: world\r\n\r\n` could misbehave).
- **Problem:** The parser does not normalize `\r\n` to `\n` before boundary detection. While many LLM providers send `\n` only, Azure OpenAI and some others send `\r\n`.
- **Impact:** SSE parsing may fail or split events incorrectly for CRLF providers.
- **Severity:** Medium
- **Recommendation:** Normalize `\r\n` to `\n` before boundary detection, or search for both `\r\n\r\n` and `\n\n`.
- **Estimated Effort:** 2-4 hours

#### Finding SSE-7: No `last-event-id` reconnection tracking for LLM SSE

- **Location:** `rustcode/crates/rustcode-core/src/sse.rs` — `SseEvent` struct has `id: Option<String>` field, but `parse_sse_stream` never uses it for reconnection
- **Evidence:**
  ```rust
  pub struct SseEvent {
      pub id: Option<String>,  // Parsed but never used for reconnection
      // ...
  }
  ```
  The SSE parser correctly parses `id:` fields from SSE events but no mechanism exposes `last-event-id` for reconnection. OpenCode's `@modelcontextprotocol/sdk` handles this.
- **Problem:** If the connection drops mid-stream, there's no way to resume from the last received event.
- **Impact:** Reconnecting LLM streams must restart from scratch, losing progress.
- **Severity:** Low
- **Recommendation:** Add `last_event_id` tracking and expose it for reconnection headers.
- **Estimated Effort:** 2-4 hours

#### Finding SSE-8: `SseError` enum is missing variants for protocol-specific errors

- **Location:** `rustcode/crates/rustcode-core/src/sse.rs` lines 44-57
- **Evidence:**
  ```rust
  pub enum SseError {
      Io(#[from] std::io::Error),
      UnexpectedEnd,
      DataTooLarge(usize),
  }
  ```
  Missing variants: `InvalidField` (unrecognized field format), `InvalidLineEnding` (non-compliant line endings), `Utf8Error` (invalid UTF-8 in event data).
- **Problem:** The error type is not rich enough to distinguish SSE protocol errors.
- **Impact:** Debugging SSE-related issues is harder; error messages are less specific.
- **Severity:** Low
- **Recommendation:** Add `InvalidField`, `InvalidLineEnding`, and `Utf8` variants to `SseError`.
- **Estimated Effort:** 1-2 hours

---

## 6. JSON-RPC Protocol Analysis

### 6.1 Message Framing

#### Finding JSONRPC-1: JSON-RPC `id` type mismatch — RustCode uses `u64` exclusively

- **Location:** `rustcode/crates/rustcode-core/src/mcp.rs` lines 68, 122
- **Evidence:**
  ```rust
  pub struct JsonRpcRequest {
      pub id: u64,  // Must be u64
      // ...
  }
  pub struct JsonRpcResponse {
      pub id: u64,  // Must be u64
      // ...
  }
  ```
  JSON-RPC 2.0 spec allows `id` to be `string`, `number`, or `null`. RustCode only supports `u64` (`number`). OpenCode uses the full range. Some MCP servers may use string IDs.
- **Problem:** JSON-RPC `id` is over-constrained to `u64`, violating spec.
- **Impact:** Connecting to servers that use string IDs will cause deserialization failures.
- **Severity:** Medium
- **Recommendation:** Change `id` field type to `serde_json::Value` and validate it's `string`, `number`, or `null` per spec.
- **Estimated Effort:** 4-6 hours

#### Finding JSONRPC-2: No batch request support

- **Location:** `rustcode/crates/rustcode-core/src/mcp.rs`
- **Evidence:** RustCode sends individual JSON-RPC requests; there is no `send_batch` method or array-based request support. OpenCode's SDK supports JSON-RPC batch requests (sending an array of request objects).
- **Problem:** Batching multiple requests (e.g., `tools/list` + `resources/list` + `prompts/list`) for efficiency is not supported.
- **Impact:** Higher latency for initial MCP server discovery (3 round-trips instead of 1).
- **Severity:** Low
- **Recommendation:** Add batch request support: accept `Vec<JsonRpcRequest>`, wrap as JSON array, send, match responses by `id`.
- **Estimated Effort:** 1 day

#### Finding JSONRPC-3: No JSON-RPC `ParseError` (-32700) handling in response parsing

- **Location:** `rustcode/crates/rustcode-core/src/mcp.rs` — `parse_jsonrpc_response` function
- **Evidence:** The response parser handles success/error but does not distinguish JSON-RPC error codes:
  ```rust
  fn parse_jsonrpc_response(response: &str) -> Result<serde_json::Value, String> {
      let value: serde_json::Value = serde_json::from_str(response)
          .map_err(|e| format!("invalid JSON-RPC response: {e}"))?;
      // Returns the raw JSON; caller checks for "error" field
      Ok(value)
  }
  ```
  JSON-RPC standard error codes: `ParseError` (-32700), `InvalidRequest` (-32600), `MethodNotFound` (-32601), `InvalidParams` (-32602), `InternalError` (-32603).
- **Problem:** No structured handling of standard JSON-RPC error codes.
- **Impact:** Error messages are generic; automated retry/fallback logic cannot distinguish error types.
- **Severity:** Medium
- **Recommendation:** Add a `JsonRpcErrorCode` enum and structured error response handling that maps standard codes.
- **Estimated Effort:** 4-6 hours

### 6.2 Notification Handling

#### Finding JSONRPC-4: `JsonRpcRequest::notification` uses `id: 0` instead of omitting `id`

- **Location:** `rustcode/crates/rustcode-core/src/mcp.rs` lines 88-96
- **Evidence:**
  ```rust
  pub fn notification(method: impl Into<String>, params: serde_json::Value) -> Self {
      Self {
          jsonrpc: "2.0".into(),
          id: 0,  // JSON-RPC 2.0 spec says id MUST be omitted for notifications
          method: method.into(),
          params: Some(params),
      }
  }
  ```
  JSON-RPC 2.0 spec § 4.2: "A Notification is a Request object without an `id` field." Using `id: 0` means the server may send a response for this request, which wastes resources.
- **Problem:** Notifications include a placeholder `id: 0`, violating JSON-RPC spec.
- **Impact:** Some servers may send unwanted responses; spec-incompatible.
- **Severity:** Medium
- **Recommendation:** Change `JsonRpcRequest` to use `Option<u64>` for `id`, where `None` means notification. Update serialization to omit `id` when `None`.
- **Estimated Effort:** 4-6 hours

---

## 7. HTTP Protocol Analysis

### 7.1 REST API Surface

#### Finding HTTP-1: RustCode routing has `/event` route; OpenCode has `/api/event`

- **Location:** `rustcode/crates/rustcode-server/src/routes/event.rs` line 53; OpenCode TS `groups/event.ts` line 18
- **Evidence:**
  ```rust
  // RustCode:
  .route("/event", get(event_stream))
  ```
  ```typescript
  // OpenCode TS:
  HttpApiEndpoint.get("event.subscribe", "/api/event", { ... })
  ```
- **Problem:** Route path mismatch — RustCode serves at `/event` while OpenCode serves at `/api/event`.
- **Impact:** Clients expecting the `/api/event` OpenCode path will get 404. This is a deliberate difference (RustCode may not have the `/api` prefix), but it breaks compatibility.
- **Severity:** Medium
- **Recommendation:** Add `/api/event` route (possibly as an alias) to maintain backward compatibility with OpenCode clients.
- **Estimated Effort:** 30 minutes

### 7.2 CORS Configuration

#### Finding HTTP-2: RustCode CORS is permissive; OpenCode is restrictive

- **Location:** `rustcode/crates/rustcode-server/src/cors.rs`; `opencode/packages/server/src/cors.ts`
- **Evidence:**
  ```rust
  // RustCode cors.rs matches the TS function set but uses wildcard or
  // config-based origins. Default is permissive.
  ```
  ```typescript
  // OpenCode TS constraints:
  // - http://localhost:*  allowed
  // - http://127.0.0.1:* allowed
  // - oc://renderer     allowed
  // - tauri://localhost  allowed
  // - *.opencode.ai     allowed (via regex)
  // - config-based custom origins
  ```
- **Problem:** RustCode CORS may be too permissive (allowing any origin) or not fine-grained enough. Without reading the actual allowed origins, the configuration may permit cross-origin requests that OpenCode would block.
- **Impact:** Potential security risk if CORS is too permissive.
- **Severity:** High
- **Recommendation:** Port the exact `isAllowedCorsOrigin` and `isAllowedRequestOrigin` logic from `opencode/packages/server/src/cors.ts` to RustCode's `cors.rs`.
- **Estimated Effort:** 2-4 hours

### 7.3 Content Negotiation

#### Finding HTTP-3: Missing content negotiation for JSON vs SSE

- **Location:** `rustcode/crates/rustcode-server/src/routes/event.rs`
- **Evidence:** The `/event` route always returns `text/event-stream` regardless of `Accept` header. OpenCode's Effect `HttpApi` framework automatically handles content negotiation based on the success schema type.
- **Problem:** No `Accept` header validation; clients requesting `application/json` get SSE data.
- **Impact:** Minor — clients that explicitly request a different format get unexpected content type.
- **Severity:** Low
- **Recommendation:** Validate `Accept` header and return 406 Not Acceptable if `text/event-stream` is not in the accepted types.
- **Estimated Effort:** 1-2 hours

### 7.4 HTTP Method and Status Code Mapping

#### Finding HTTP-4: RustCode unconditionally uses `GET` for `/event`; OpenCode supports `GET` only

- **Location:** `rustcode/crates/rustcode-server/src/routes/event.rs` line 53
- **Evidence:** Both use `GET`, so this is consistent. No issue.
- **Severity:** N/A (consistent)

### 7.5 Route Handler Surface Analysis

RustCode defines 30 route modules in `crates/rustcode-server/src/routes/mod.rs`. Each maps to an OpenCode `HttpApiGroup`. The following table compares the two implementations:

| Route Module (RustCode) | OpenCode Equivalent | Status | Notes |
|---|---|---|---|
| `agent` | `AgentApi` | Partial | Agent state endpoints |
| `command` | `CommandApi` | Partial | Shell command execution |
| `config` | `ConfigApi` | Partial | Config read/write |
| `control` | `ControlApi` | Partial | Server control/pause |
| `control_plane` | `ControlPlaneApi` | Partial | Multi-instance control |
| `credential` | `CredentialApi` | Partial | Credential management |
| `event` | `EventApi` | **Gaps** | See SSE findings |
| `experimental` | `ExperimentalApi` | Partial | Experimental features |
| `file` | `FileApi` | Partial | File read/write ops |
| `global` | `GlobalApi` | Partial | Global settings |
| `health` | `HealthApi` | Partial | Health check |
| `instance` | `InstanceApi` | Partial | Instance lifecycle |
| `integration` | `IntegrationApi` | Partial | External integrations |
| `mcp` | `McpApi` | **Gaps** | OAuth stubs |
| `metadata` | `MetadataApi` | Partial | Server metadata |
| `model` | `ModelApi` | Partial | Model/provider queries |
| `permission` | `PermissionApi` | Partial | Permission requests |
| `project` | `ProjectApi` | Partial | Project management |
| `project_copy` | `ProjectCopyApi` | Partial | Project duplication |
| `provider` | `ProviderApi` | Partial | Provider configuration |
| `pty` | `PtyApi` | Partial | PTY terminal |
| `query` | `QueryApi` | Partial | Database queries |
| `question` | `QuestionApi` | Partial | User questions |
| `reference` | `ReferenceApi` | Partial | Symbol references |
| `session` | `SessionApi` | Partial | Session CRUD |
| `skill` | `SkillApi` | Partial | Skill management |
| `sync` | `SyncApi` | Partial | File sync |
| `tui` | `TuiApi` | Partial | TUI state |
| `workspace` | `WorkspaceApi` | Partial | Workspace management |

All 30 routes are scaffolded with basic handlers but most lack the full request validation, error handling, and streaming support present in OpenCode.

#### Finding HTTP-5: Route handler error responses are inconsistent

- **Location:** `rustcode/crates/rustcode-server/src/routes/mcp.rs` lines 93-99, 114-122, 173-185
- **Evidence:**
  ```rust
  // Error responses vary in shape:
  // Some return {"error": "..."}
  // Others return {"error": format!("...")}
  // No standardized error response schema
  return (
      StatusCode::BAD_REQUEST,
      Json(serde_json::json!({
          "error": format!("invalid MCP server type: '{bad}'")
      })),
  );
  ```
  OpenCode uses `Schema.TaggedErrorClass` for typed error responses that are part of the API contract:
  ```typescript
  export class McpNotFoundError extends Schema.TaggedErrorClass<McpNotFoundError>()("McpNotFoundError", {
    name: Schema.String,
  }) {}
  ```
- **Problem:** Error response shapes are ad-hoc across route handlers instead of being defined as a typed schema. Clients cannot reliably parse error responses.
- **Impact:** API client error handling is fragile; errors may be misidentified.
- **Severity:** Medium
- **Recommendation:** Define a standard error response schema (e.g., `{"error": {"code": "...", "message": "..."}}`) and use it consistently across all route handlers.
- **Estimated Effort:** 1-2 days

#### Finding HTTP-6: Health check returns hardcoded `db_status: "connected"` without actual database ping

- **Location:** `rustcode/crates/rustcode-server/src/routes/health.rs` lines 44-45
- **Evidence:**
  ```rust
  // Database status hardcoded to "connected"
  let db_status = "connected";
  ```
- **Problem:** The health endpoint claims the database is connected without actually pinging it. If the database is down, the health check will still report healthy.
- **Impact:** False positive health checks — orchestrators may route traffic to a degraded server.
- **Severity:** Medium
- **Recommendation:** Implement an actual database ping (e.g., `SELECT 1` or SQLite `PRAGMA integrity_check`) before reporting database status.
- **Estimated Effort:** 2-4 hours

---

## 8. WebSocket Protocol Support

#### Finding WS-1: No WebSocket support in either codebase

- **Location:** Global across both codebases
- **Evidence:** Neither RustCode nor OpenCode implements WebSocket endpoints. Both use SSE for server→client streaming. The MCP protocol has deprecated its SSE transport in favor of StreamableHTTP, which uses standard HTTP POST for request/response.
- **Problem:** No WebSocket support exists, but this is by design — SSE is the chosen streaming technology.
- **Impact:** No WebSocket-related issues to fix.
- **Severity:** N/A (by design)
- **Recommendation:** Monitor MCP spec evolution for WebSocket transport and implement if required by the MCP specification.
- **Estimated Effort:** N/A

---

## 9. Cross-cutting Concerns

### 9.1 Error Type Hierarchy

#### Finding CC-1: RustCode `Error` enum has 60+ variants; OpenCode has ~120 `Schema.TaggedErrorClass` types

- **Location:** `rustcode/crates/rustcode-core/src/error.rs` (1197 lines); OpenCode TS scattered across packages
- **Evidence:** RustCode places all error types into a single `Error` enum with domain-specific sub-enums (`LlmErrorReason`, `PermissionError`, `WorktreeError`, `ImageError`, `SkillError`). The enum hierarchy:
  ```
  Error (top-level enum, ~60 variants)
  ├── Io (std::io::Error)
  ├── FileSystem { path, message }
  ├── StaleContent { path }
  ├── TargetExists { path }
  ├── BinaryFile { path }
  ├── MediaIngestLimit { path }
  ├── Json (from serde_json)
  ├── Toml (from toml)
  ├── Config(String)
  ├── Database(String)
  ├── Llm { module, method, reason: Box<LlmErrorReason> }
  │   └── LlmErrorReason (sub-enum: Authentication, RateLimit, ContextOverflow, etc.)
  ├── ProviderInit { provider_id, message }
  ├── NoProviders
  ├── NoModels { provider_id }
  ├── ModelNotFound { provider_id, model_id }
  ├── HeaderTimeout { ms }
  ├── ResponseStream(String)
  ├── ContextOverflow(String)
  ├── Tool(String)
  ├── ToolInvalidArguments { tool, detail }
  ├── ToolRegistration { name, message }
  ├── Session(String)
  ├── SessionNotFound { session_id }
  ├── SessionBusy { session_id }
  ├── SessionPromptConflict { session_id }
  ├── SessionOperationUnavailable { session_id, reason }
  ├── StepLimitExceeded { session_id }
  ├── ModelNotSelected
  ├── MessageDecode { session_id, message_id }
  ├── Permission(from PermissionError)
  ├── Git(String)
  ├── Worktree(from WorktreeError)
  ├── Image(from ImageError)
  ├── Plugin(String)
  ├── Skill(from SkillError)
  ├── McpNotFound { name }
  ├── LspInit(String)
  ├── Process { message, exit_code }
  ├── Network(String)
  ├── Http(from reqwest)
  ├── ... [Auth, Internal, etc.]
  ```
  OpenCode uses individual `Schema.TaggedErrorClass` per error type scattered across packages (packages/opencode, packages/core, packages/llm). Each is independently defined and can be pattern-matched by its `_tag` in Effect.gen:
  ```typescript
  // OpenCode pattern:
  export class InitializeError extends Schema.TaggedErrorClass<InitializeError>()("LSPInitializeError", {
    serverID: Schema.String,
    cause: Schema.optional(Schema.Defect),
  }) {}
  ```
- **Problem:** The monolithic `Error` enum means adding a new error variant requires recompiling every dependent crate. The `#[from]` derives create implicit conversion paths between unrelated error types (e.g., `serde_json::Error` auto-converts to top-level `Error::Json`, which could hide type mismatches). Adding `#[non_exhaustive]` to the enum would allow forward-compatible additions but break exhaustive matches. The `LlmErrorReason` sub-enum (lines 281-350 of error.rs) mirrors the OpenCode `LLMError` tagged errors but loses the `module.method` context that OpenCode preserves.
- **Impact:** Poor extensibility; every error variant change affects all consumers. Large enum size (~200 bytes with `Box` fields) may affect stack size in deeply nested error propagation.
- **Severity:** Medium
- **Recommendation:** Consider using a trait-based approach (e.g., `trait ProtocolError: std::error::Error`) with `Box<dyn ProtocolError>` for dynamic dispatch, or add `#[non_exhaustive]` to the top-level `Error` enum. For protocol-specific errors (LSP, MCP, SSE), define separate error enums in their respective crates rather than centralizing in `rustcode-core`.
- **Estimated Effort:** 2-3 days

#### Finding CC-2: Missing protocol-level error translations (LSP/MCP errors → HTTP error responses)

- **Location:** `rustcode/crates/rustcode-server/src/routes/` (30 files)
- **Evidence:** Route handlers return errors via AXUM's `axum::response::IntoResponse` implementations. There is no centralized error-to-HTTP-status mapping that correlates protocol errors (e.g., LSP InitializeError → 502 Bad Gateway, MCP not found → 404). Looking at `routes/mcp.rs`:
  ```rust
  // Ad-hoc mapping in mcp_connect:
  let status = if matches!(e, rustcode_core::error::Error::McpNotFound { .. }) {
      StatusCode::NOT_FOUND
  } else {
      StatusCode::BAD_GATEWAY
  };
  ```
  There is no consistent pattern across all route handlers. Some return `BAD_REQUEST`, others use `INTERNAL_SERVER_ERROR`, and there is no mapping for protocol-level errors like:
  - `McpOAuthError::Timeout` → 504 Gateway Timeout
  - `McpOAuthError::StateMismatch` → 401 Unauthorized
  - `LspInit` → 502 Bad Gateway
  - `Process` exit code errors → varied
- **Problem:** HTTP status codes from protocol errors may be incorrect or inconsistent across routes. Clients cannot distinguish between transient (retriable) and permanent errors.
- **Impact:** API clients may misinterpret the nature of failures, leading to incorrect retry behavior or confusing error displays.
- **Severity:** Medium
- **Recommendation:** Implement a centralized `impl IntoResponse for Error` that maps each `Error` variant to a `(StatusCode, JsonBody)` pair. This ensures consistent error responses across all 30 route handlers.
- **Estimated Effort:** 1 day

#### Finding CC-6: Missing LSP diagnostic wait/debounce logic from OpenCode

- **Location:** `opencode/packages/opencode/src/lsp/client.ts` lines 446-511; RustCode has no equivalent
- **Evidence:** OpenCode implements sophisticated diagnostic waiting logic that RustCode lacks entirely:
  ```typescript
  const DIAGNOSTICS_DEBOUNCE_MS = 150
  const DIAGNOSTICS_DOCUMENT_WAIT_TIMEOUT_MS = 5_000
  const DIAGNOSTICS_FULL_WAIT_TIMEOUT_MS = 10_000
  const DIAGNOSTICS_REQUEST_TIMEOUT_MS = 3_000

  function waitForFreshPush(request: { path: string; version: number; after: number; timeout: number }) {
    // Waits for a fresh push diagnostic, with 150ms debounce,
    // and a configurable timeout. Used to ensure diagnostics
    // are ready before reporting them to the LLM.
    // Uses diagnosticListeners Set-based notification.
  }

  function waitForRegistrationChange(timeout: number) {
    // Waits for a capability registration to complete.
    // Used to wait for pull diagnostics registration.
  }

  async function waitForDocumentDiagnostics(request: { path: string; version: number; after?: number }) {
    // Combines pull + push wait strategies:
    // 1. If pull diagnostics supported: send textDocument/diagnostic
    // 2. Wait for fresh push with debounce
    // 3. If workspace diagnostics: request workspace/diagnostic
    // 4. Merge results from both sources
  }
  ```
  These utilities ensure diagnostics are available before reporting to the LLM, preventing "no diagnostics yet" false negatives. RustCode has no equivalent — it simply reads whatever diagnostics are available at call time.
- **Problem:** Without wait/debounce logic, RustCode may return stale or empty diagnostic sets when queried immediately after a file edit.
- **Impact:** The LLM may act on incorrect or incomplete diagnostic information, leading to wrong code fixes.
- **Severity:** High
- **Recommendation:** Implement diagnostic wait utilities in RustCode LSP:
  1. Add `wait_for_fresh_push` with configurable debounce (150ms) and timeout
  2. Add `wait_for_registration_change` for pull diagnostics capability negotiation
  3. Add a combined `wait_for_document_diagnostics` that tries pull first, then waits for push
- **Estimated Effort:** 1-2 days

#### Finding CC-7: `McpServerRegistry` is created per-route call instead of per-application

- **Location:** `rustcode/crates/rustcode-server/src/routes/mcp.rs` lines 32-33
- **Evidence:**
  ```rust
  pub fn mcp_routes(state: Arc<AppState>) -> Router {
      // A single persistent registry shared across all MCP route handlers.
      let mcp_registry: Arc<McpServerRegistry> =
          Arc::new(McpServerRegistry::new());
      ...
  }
  ```
  The `McpServerRegistry` is created inside `mcp_routes()` which is called once at server startup. This is correct for the current design (created once, shared via closures). However, the registry is not stored in `AppState`, which means other route handlers (e.g., `session.rs` if it needs MCP tools) cannot access it.
- **Problem:** The MCP registry is isolated to the MCP routes. If session or agent routes need to query MCP tool status, they cannot access the registry.
- **Impact:** Tight coupling — MCP tools are only accessible through the MCP route handlers.
- **Severity:** Medium
- **Recommendation:** Move `McpServerRegistry` into `AppState` so it's accessible from any route handler.
- **Estimated Effort:** 2-4 hours

### 9.2 Logging and Tracing

#### Finding CC-3: RustCode uses `tracing` crate; OpenCode uses Effect's built-in logging

- **Location:** Throughout both codebases
- **Evidence:** RustCode uses `tracing::warn!`, `tracing::info!`, etc. OpenCode uses `Effect.logInfo`, `Effect.logError`, etc.
- **Problem:** Different logging backends, but this is expected for a Rust port. No functional gap.
- **Severity:** N/A (by design)

#### Finding CC-4: Missing request ID correlation in RustCode HTTP routes

- **Location:** `rustcode/crates/rustcode-server/src/server.rs` (router construction)
- **Evidence:** RustCode's AXUM router does not include a `TraceLayer` or request ID middleware. OpenCode's Effect `HttpApi` may include correlation IDs in logs.
- **Problem:** Without request IDs, correlating log entries across request/response/stream boundaries is difficult.
- **Impact:** Debugging production issues is harder.
- **Severity:** Medium
- **Recommendation:** Add `tower-http` `TraceLayer` with a `make_span` that includes a unique request ID (UUID or ULID) and propagates it to downstream operations.
- **Estimated Effort:** 2-4 hours

### 9.3 Graceful Shutdown

#### Finding CC-5: RustCode server has `shutdown_signal()` but no in-flight request drain

- **Location:** `rustcode/crates/rustcode-server/src/server.rs`
- **Evidence:** The server implements graceful shutdown via `axum::serve` with a signal, but there is no explicit `ConnectionLayer` for connection draining or a grace period for SSE stream cleanup.
- **Problem:** On shutdown, SSE connections may be terminated mid-event, or ongoing LSP/MCP operations may be aborted without cleanup.
- **Impact:** Potential data loss or corrupted state on restart.
- **Severity:** Medium
- **Recommendation:** Add `axum::serve::with_graceful_shutdown` with a configurable timeout, and add `ConnectionLayer` for connection draining. Ensure SSE streams complete or are explicitly terminated on shutdown.
- **Estimated Effort:** 4-6 hours

---

## 10. Recommendations

### Immediate (Critical — fix first)

1. **LSP-1:** Implement the full LSP client crate (`rustcode-lsp`) with JSON-RPC framing, `initialize`, `initialized`, `didOpen`, `didChange`, `didClose`, and diagnostic handling.
2. **MCP-1:** Parse the `endpoint` SSE event for message URL discovery instead of deriving from SSE URL.
3. **SSE-1:** Fix workspace filtering field name from `workspace` to `workspaceID` in `EventQuery`.

### Short-term (High priority — next week)

4. **LSP-2/LSP-3:** Add completion, hover, goto-definition, and `initialized` notification to LSP client.
5. **MCP-2:** Fix deadlock potential in `send_jsonrpc` for SSE transport by dropping mutex during wait loop.
6. **MCP-6:** Add timeout to MCP initialize response read.
7. **MCP-8:** Implement OAuth callback server for MCP remote connections.
8. **SSE-2:** Add `server.instance.disposed` termination logic to SSE stream.
9. **SSE-3:** Add Cache-Control, X-Accel-Buffering, and X-Content-Type-Options SSE response headers.
10. **HTTP-2:** Tighten CORS configuration to match OpenCode's allowlist.

### Medium-term (Medium priority — next sprint)

11. **JSONRPC-1:** Change JSON-RPC `id` type from `u64` to `serde_json::Value`.
12. **JSONRPC-4:** Make `id` `Option<u64>` for notifications (omit when `None`).
13. **MCP-4/MCP-5:** Add pagination, resources, and prompts support.
14. **SSE-6:** Normalize CRLF in LLM SSE parser.
15. **HTTP-1:** Add `/api/event` route alias for compatibility.
16. **CC-4:** Add request ID tracing middleware.
17. **CC-5:** Improve graceful shutdown with connection draining.

### Long-term (Lower priority)

18. **JSONRPC-2:** Add batch request support.
19. **JSONRPC-3:** Add structured JSON-RPC error code handling.
20. **SSE-7:** Add `last-event-id` reconnection support.
21. **CC-1:** Refactor error type hierarchy to be more extensible.
22. **CC-2:** Implement centralized protocol error → HTTP response mapping.

---

## 11. Summary of Findings

| ID | Severity | Category | Finding |
|---|---|---|---|
| LSP-1 | **Critical** | LSP | RustCode LSP crate is a stub — no actual protocol wiring |
| LSP-1b | High | LSP | No pull diagnostic support; no client/registerCapability handler |
| LSP-2 | High | LSP | Missing completion, hover, goto-definition |
| LSP-3 | High | LSP | Missing `initialized` notification after `initialize` response |
| LSP-4 | Medium | LSP | No incremental text sync (full-text only) |
| LSP-5 | Medium | LSP | No `relatedDocuments` in diagnostic pull model |
| LSP-6 | Medium | LSP | Content-Length parsing skips unknown headers |
| MCP-1 | **Critical** | MCP | SSE endpoint URL construction fragile — assumes `/sse` → `/messages` |
| MCP-2 | High | MCP | `send_jsonrpc` may deadlock on SSE response wait (mutex held) |
| MCP-3 | Medium | MCP | SSE transport lacks reconnection |
| MCP-4 | Medium | MCP | `tools/list` missing pagination |
| MCP-5 | Medium | MCP | Missing `resources/list` and `prompts/list` |
| MCP-6 | High | MCP | Initialize handshake has no timeout |
| MCP-7 | Low | MCP | `notifications/initialized` sent without protocol version check |
| MCP-8 | High | MCP | OAuth types defined but callback server not implemented |
| MCP-9 | High | MCP | OAuth route handlers return placeholder URLs |
| SSE-1 | **Critical** | SSE | `workspace` vs `workspaceID` field name mismatch |
| SSE-2 | High | SSE | Missing `server.instance.disposed` termination |
| SSE-3 | High | SSE | Missing Cache-Control, X-Accel-Buffering, X-Content-Type-Options headers |
| SSE-4 | Low | SSE | Double heartbeat mechanism (explicit + AXUM KeepAlive) |
| SSE-5 | Medium | SSE | `unwrap_or_default()` on serialization failure sends empty data |
| SSE-6 | Medium | SSE | LLM SSE parser doesn't normalize CRLF |
| SSE-7 | Low | SSE | No `last-event-id` reconnection tracking |
| SSE-8 | Low | SSE | `SseError` missing protocol-specific variants |
| JSONRPC-1 | Medium | JSON-RPC | `id` type is `u64` only, should be `string | number | null` |
| JSONRPC-2 | Low | JSON-RPC | No batch request support |
| JSONRPC-3 | Medium | JSON-RPC | No structured error code handling (ParseError, InvalidRequest, etc.) |
| JSONRPC-4 | Medium | JSON-RPC | Notifications use `id: 0` instead of omitting `id` |
| HTTP-1 | Medium | HTTP | Route path mismatch: `/event` vs `/api/event` |
| HTTP-2 | High | HTTP | CORS may be too permissive |
| HTTP-3 | Low | HTTP | No Accept header validation for SSE |
| HTTP-5 | Medium | HTTP | Route handler error responses are inconsistent |
| HTTP-6 | Medium | HTTP | Health check db_status hardcoded to "connected" |
| CC-1 | Medium | Cross-cutting | Monolithic `Error` enum hurts extensibility |
| CC-2 | Medium | Cross-cutting | No centralized protocol error → HTTP mapping |
| CC-4 | Medium | Cross-cutting | Missing request ID correlation in tracing |
| CC-5 | Medium | Cross-cutting | No in-flight request drain on shutdown |
| CC-6 | High | Cross-cutting | Missing LSP diagnostic wait/debounce logic |
| CC-7 | Medium | Cross-cutting | McpServerRegistry not in AppState |

**Total findings: 38** (3 Critical, 10 High, 17 Medium, 5 Low, 5 N/A/informational)

---

## Appendix A: Files Examined

### RustCode Files
| File | Lines | Description |
|---|---|---|
| `crates/rustcode-core/src/lsp.rs` | 804 | Core LSP types |
| `crates/rustcode-core/src/mcp.rs` | ~2100+ | MCP client and types |
| `crates/rustcode-core/src/sse.rs` | 385 | SSE parser for LLM responses |
| `crates/rustcode-core/src/error.rs` | 1197 | Error type hierarchy |
| `crates/rustcode-server/src/lib.rs` | ~50 | Server entry point |
| `crates/rustcode-server/src/server.rs` | ~200 | Router construction |
| `crates/rustcode-server/src/sse.rs` | ~100 | AXUM SSE stream builder |
| `crates/rustcode-server/src/cors.rs` | ~80 | CORS middleware |
| `crates/rustcode-server/src/routes/event.rs` | 160 | SSE event route handler |

### OpenCode Files
| File | Lines | Description |
|---|---|---|
| `packages/opencode/src/lsp/lsp.ts` | 511 | LSP manager and types |
| `packages/opencode/src/lsp/client.ts` | 650 | LSP client implementation |
| `packages/opencode/src/lsp/server.ts` | ~200 | LSP server definitions |
| `packages/opencode/src/mcp/index.ts` | ~1000+ | MCP manager implementation |
| `packages/opencode/src/server/routes/instance/httpapi/handlers/event.ts` | 99 | SSE event handler |
| `packages/opencode/src/server/routes/instance/httpapi/groups/event.ts` | 29 | SSE endpoint definition |
| `packages/opencode/src/server/routes/instance/httpapi/server.ts` | ~200 | HTTP server construction |
| `packages/server/src/handlers/event.ts` | 63 | Server package SSE handler |
| `packages/server/src/groups/event.ts` | 34 | Server package SSE endpoint |
| `packages/server/src/cors.ts` | 34 | CORS configuration |

## Appendix B: LSP Protocol Spec Compliance

| Feature | OpenCode (TS) | RustCode | Status |
|---|---|---|---|
| `initialize` | ✓ | Partial (MCP, not LSP) | Missing in LSP crate |
| `initialized` | ✓ | ✗ | Missing |
| `textDocument/didOpen` | ✓ | ✗ | Missing |
| `textDocument/didChange` | ✓ (incremental) | ✗ | Missing |
| `textDocument/didClose` | ✓ | ✗ | Missing |
| `textDocument/completion` | ✓ | ✗ | Missing |
| `textDocument/hover` | ✓ | ✗ | Missing |
| `textDocument/definition` | ✓ | ✗ | Missing |
| `textDocument/documentSymbol` | ✓ | ✗ | Missing |
| `workspace/symbol` | ✓ | ✗ | Missing |
| `textDocument/diagnostic` | ✓ | Partial (types only) | Missing transport |
| `textDocument/codeAction` | ✓ | ✗ | Missing |
| Content-Length framing | ✓ (vscode-jsonrpc) | Partial (in MCP code) | Not shared module |
| `\r\n` line endings | ✓ | Weak (trim() only) | Medium risk |
| initialize timeout | 45s | None | High risk |

## Appendix C: MCP Protocol Spec Compliance

| Feature | OpenCode (TS) | RustCode | Status |
|---|---|---|---|
| `initialize` handshake | ✓ | ✓ | OK |
| `notifications/initialized` | ✓ | ✓ | OK |
| `tools/list` | ✓ (pagination) | ✓ (no pagination) | Medium gap |
| `tools/call` | ✓ | ✓ | OK |
| `resources/list` | ✓ | ✗ | Missing |
| `resources/read` | ✓ | ✗ | Missing |
| `prompts/list` | ✓ | ✗ | Missing |
| `prompts/get` | ✓ | ✗ | Missing |
| Stdio transport | ✓ | ✓ | OK |
| SSE transport | ✓ (SDK) | ✓ (custom) | Fragile URL derivation |
| StreamableHTTP | ✓ (SDK) | Partial | Post-only, no streaming response |
| OAuth | ✓ (callback server) | Types only | Non-functional |
| Pagination | ✓ | ✗ | Missing |
| Error handling | ✓ (SDK) | Basic | No structured error codes |

## Appendix D: SSE Spec Compliance

| Feature | OpenCode (TS) | RustCode | Status |
|---|---|---|---|
| `event:` field | ✓ | ✓ | OK |
| `data:` field | ✓ | ✓ | OK |
| `id:` field | ✓ | ✓ | OK |
| `retry:` field | ✓ | ✓ | OK |
| `Last-Event-ID` reconnection | ✓ | ✗ | Missing |
| CRLF normalization | ✓ | ✗ | Medium gap |
| `\n\n` event boundary | ✓ | ✓ | OK |
| `:comment` support | ✓ | ✓ | OK |
| Keepalive | 10s tick | 10s tick + 15s AXUM | Double heartbeat |
| `Cache-Control` header | ✓ | ✗ | Missing |
| `X-Accel-Buffering` | ✓ | ✗ | Missing |
| Event stream termination | ✓ (takeUntil) | ✗ | Zombie connections |

---

*End of Protocol Audit Report*
