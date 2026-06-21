# Report 09: Server/API Subsystem — Gap Analysis & Fix Plan

## Overview

This report compares opencode's HTTP server/API subsystem (two packages:
`packages/opencode/src/server/` and `packages/server/src/`) against rustcode's
implementation (`crates/rustcode-server/`). The goal is to identify every gap so
they can be closed systematically.

**Reading this report:**
- "opencode" = the TypeScript reference at `/root/opencodesport/opencode/`
- "rustcode" = the Rust port at `/root/opencodesport/rustcode/`
- Each gap is numbered and categorized as **CRITICAL** (blocking functionality),
  **HIGH** (important for feature parity), **MEDIUM** (polish), or **LOW** (nice-to-have).

---

## 1. Architecture Overview

### opencode Server Stack (from server.ts)

```
[Client Request]
    |
    v
HttpRouter.serve() ─┬─ RootHttpApi  (/global/*, /auth/*, /log)
                     ├─ EventApi      (/event SSE)
                     ├─ PtyConnectApi (/pty/:ptyID/connect WebSocket)
                     ├─ InstanceHttpApi (all /session/*, /file/*, etc.)
                     ├─ Api           (/api/* v2 routes)
                     ├─ docRoute      (GET /doc OpenAPI)
                     └─ uiRoute       (catch-all UI proxy)
                          |
                          ├─ errorLayer         (global error handler)
                          ├─ compressionLayer   (gzip/deflate)
                          ├─ corsVaryFix        (Vary header)
                          ├─ fenceLayer         (state-change tracking)
                          ├─ cors(corsOptions)  (CORS)
                          └─ HttpServer.layerServices
```

Auth is applied **per route group** via Layer composition:
- `httpApiAuthLayer` → Instance API + Root API + Event API
- `serverHttpApiAuthLayer` → v2 API
- `ptyConnectHttpApiAuthLayer` → PTY connect
- `authOnlyRouterMiddleware` → doc + UI catch-all (public paths bypass)

### rustcode Server Stack (from server.rs)

```
build_router(state)
    |
    └─ Router::new()
         ├─ global_routes
         ├─ health_routes
         ├─ control_routes
         ├─ control_plane_routes
         ├─ agent | command | config | credential | experimental | file
         ├─ instance | integration | mcp | model | permission | project
         ├─ project_copy | provider | pty | question | reference
         ├─ session | skill | sync | tui | workspace
         ├─ event_routes
         ├─ metadata_routes
         ├─ query_routes
         └─ .layer(cors)    ← cors_layer(&[]) hardcoded!
```

**No auth middleware. No compression. No error standardization. No v2 API. No UI proxy.**

---

## 2. Route Completeness

### Legend
| Symbol | Meaning |
|--------|---------|
| ✓ | Implemented |
| ~ | Partial / stub |
| ✗ | Missing |

### Instance API (Root routes + /global/*)

| Route | opencode | rustcode | Notes |
|-------|----------|----------|-------|
| `GET /health` | ✓ | ✓ | health.rs |
| `GET /global/health` | ✓ | ✓ | global.rs |
| `GET /global/event` (SSE) | ✓ | ✗ | **Gap 1** — stub missing, no global SSE |
| `GET /global/config` | ✓ | ✓ | global.rs — GET works |
| `PATCH /global/config` | ✓ | ✓ | global.rs — PATCH works |
| `POST /global/dispose` | ✓ | ✓ | global.rs — stub |
| `POST /global/upgrade` | ✓ | ~ | global.rs — returns success stub |
| `PUT /auth/:providerID` | ✓ | ✓ | control.rs |
| `DELETE /auth/:providerID` | ✓ | ✓ | control.rs |
| `POST /log` | ✓ | ✓ | control.rs — writes log events |
| `POST /experimental/control-plane/move-session` | ✓ | ~ | control_plane.rs — stub |

### Session Routes

| Route | opencode | rustcode | Notes |
|-------|----------|----------|-------|
| `GET /session` | ✓ | ✓ | session.rs |
| `GET /session/status` | ✓ | ✗ | **Gap 2** |
| `GET /session/:sessionID` | ✓ | ✓ | |
| `GET /session/:sessionID/children` | ✓ | ✗ | **Gap 3** |
| `GET /session/:sessionID/todo` | ✓ | ✗ | **Gap 4** |
| `GET /session/:sessionID/diff` | ✓ | ✗ | **Gap 5** |
| `GET /session/:sessionID/message` | ✓ | ~ | session.rs — basic list |
| `GET /session/:sessionID/message/:messageID` | ✓ | ~ | |
| `POST /session` | ✓ | ✓ | |
| `DELETE /session/:sessionID` | ✓ | ✓ | |
| `PATCH /session/:sessionID` | ✓ | ✓ | |
| `POST /session/:sessionID/fork` | ✓ | ✗ | **Gap 6** |
| `POST /session/:sessionID/abort` | ✓ | ✗ | **Gap 7** |
| `POST /session/:sessionID/share` | ✓ | ✗ | **Gap 8** |
| `DELETE /session/:sessionID/share` | ✓ | ✗ | |
| `POST /session/:sessionID/init` | ✓ | ✗ | **Gap 9** |
| `POST /session/:sessionID/summarize` | ✓ | ✗ | **Gap 10** |
| `POST /session/:sessionID/message` (prompt) | ✓ | ✗ | **Gap 11** |
| `POST /session/:sessionID/prompt_async` | ✓ | ✗ | **Gap 12** |
| `POST /session/:sessionID/command` | ✓ | ✗ | **Gap 13** |
| `POST /session/:sessionID/shell` | ✓ | ✗ | **Gap 14** |
| `POST /session/:sessionID/revert` | ✓ | ✗ | **Gap 15** |
| `POST /session/:sessionID/unrevert` | ✓ | ✗ | |
| `POST /session/:sessionID/permissions/:permissionID` | ✓ | ✗ | **Gap 16** |
| `DELETE /session/:sessionID/message/:messageID` | ✓ | ✗ | **Gap 17** |
| `DELETE /session/:sessionID/message/:messageID/part/:partID` | ✓ | ✗ | |
| `PATCH /session/:sessionID/message/:messageID/part/:partID` | ✓ | ✗ | |

### File Routes

| Route | opencode | rustcode | Notes |
|-------|----------|----------|-------|
| `GET /find` | ✓ | ✗ | **Gap 18** — find text in files |
| `GET /find/file` | ✓ | ✗ | **Gap 19** — find files |
| `GET /find/symbol` | ✓ | ✗ | **Gap 20** — find symbols |
| `GET /file` | ✓ | ~ | file.rs — stub listing |
| `GET /file/content` | ✓ | ✗ | **Gap 21** |
| `GET /file/status` | ✓ | ✗ | **Gap 22** |

### MCP Routes

| Route | opencode | rustcode | Notes |
|-------|----------|----------|-------|
| `GET /mcp` | ✓ | ~ | mcp.rs — basic status |
| `POST /mcp` | ✓ | ✗ | **Gap 23** — add MCP server |
| `POST /mcp/:name/auth` | ✓ | ✗ | **Gap 24** |
| `POST /mcp/:name/auth/callback` | ✓ | ✗ | |
| `POST /mcp/:name/auth/authenticate` | ✓ | ✗ | |
| `DELETE /mcp/:name/auth` | ✓ | ✗ | |
| `POST /mcp/:name/connect` | ✓ | ✗ | **Gap 25** |
| `POST /mcp/:name/disconnect` | ✓ | ✗ | |

### VCS Routes

| Route | opencode | rustcode | Notes |
|-------|----------|----------|-------|
| `GET /vcs` | ✓ | ✗ | **Gap 26** |
| `GET /vcs/status` | ✓ | ✗ | **Gap 27** |
| `GET /vcs/diff` | ✓ | ✗ | **Gap 28** |
| `GET /vcs/diff/raw` | ✓ | ✗ | |
| `POST /vcs/apply` | ✓ | ✗ | **Gap 29** |

### Instance Routes

| Route | opencode | rustcode | Notes |
|-------|----------|----------|-------|
| `POST /instance/dispose` | ✓ | ~ | instance.rs — stub |
| `GET /path` | ✓ | ✗ | **Gap 30** |

### Provider Routes

| Route | opencode | rustcode | Notes |
|-------|----------|----------|-------|
| `GET /config` | ✓ | ✓ | config.rs |
| `PATCH /config` | ✓ | ✓ | |
| `GET /config/providers` | ✓ | ✗ | **Gap 31** |
| `GET /provider` | ✓ | ✓ | provider.rs |
| `GET /provider/auth` | ✓ | ✗ | **Gap 32** |
| `POST /provider/:providerID/oauth/authorize` | ✓ | ✗ | **Gap 33** |
| `POST /provider/:providerID/oauth/callback` | ✓ | ✗ | |

### Question Routes

| Route | opencode | rustcode | Notes |
|-------|----------|----------|-------|
| `GET /question` | ✓ | ✓ | question.rs |
| `POST /question/:requestID/reply` | ✓ | ✓ | |
| `POST /question/:requestID/reject` | ✓ | ✗ | **Gap 34** |

### Permission Routes

| Route | opencode | rustcode | Notes |
|-------|----------|----------|-------|
| `GET /permission` | ✓ | ✓ | permission.rs |
| `POST /permission/:requestID/reply` | ✓ | ✓ | |

### Project Routes

| Route | opencode | rustcode | Notes |
|-------|----------|----------|-------|
| `GET /project` | ✓ | ~ | project.rs — stub |
| `GET /project/current` | ✓ | ✗ | **Gap 35** |
| `POST /project/git/init` | ✓ | ✗ | **Gap 36** |
| `PATCH /project/:projectID` | ✓ | ✗ | |
| `GET /project/:projectID/directories` | ✓ | ✗ | **Gap 37** |

### PTY Routes

| Route | opencode | rustcode | Notes |
|-------|----------|----------|-------|
| `GET /pty/shells` | ✓ | ✓ | pty.rs — works |
| `GET /pty` | ✓ | ~ | returns empty list |
| `POST /pty` | ✓ | ~ | returns 501 Not Implemented |
| `GET /pty/:ptyID` | ✓ | ~ | returns 404 |
| `PUT /pty/:ptyID` | ✓ | ~ | returns 501 |
| `DELETE /pty/:ptyID` | ✓ | ✓ | returns success stub |
| `POST /pty/:ptyID/connect-token` | ✓ | ~ | returns 501 |
| `GET /pty/:ptyID/connect` (WebSocket) | ✓ | ~ | WS handler exists but echoed only |
| `WS /pty/:ptyID/ws` | ✓ | ~ | WS handler but no real PTY bridge |

### Agent / Command / Skill Routes

| Route | opencode | rustcode | Notes |
|-------|----------|----------|-------|
| `GET /agent` | ✓ | ~ | agent.rs — basic stub |
| `GET /command` | ✓ | ~ | command.rs — basic stub |
| `GET /skill` | ✓ | ~ | skill.rs — basic stub |
| `GET /lsp` | ✓ | ✗ | **Gap 38** |
| `GET /formatter` | ✓ | ✗ | **Gap 39** |

### Sync Routes

| Route | opencode | rustcode | Notes |
|-------|----------|----------|-------|
| `GET /sync/start` | ✓ | ✗ | **Gap 40** |
| `POST /sync/replay` | ✓ | ✗ | **Gap 41** |
| `POST /sync/steal` | ✓ | ✗ | **Gap 42** |
| `POST /sync/history` | ✓ | ✗ | **Gap 43** |

### TUI Routes

| Route | opencode | rustcode | Notes |
|-------|----------|----------|-------|
| `GET /tui/*` (11 routes) | ✓ | ✗ | **Gap 44** |

### Workspace / Experimental Routes

| Route | opencode | rustcode | Notes |
|-------|----------|----------|-------|
| `GET /experimental/workspace/*` (8 routes) | ✓ | ~ | workspace.rs — basic |
| `GET /experimental/capabilities` | ✓ | ✗ | **Gap 45** |
| `GET /experimental/console` | ✓ | ✗ | **Gap 46** |
| `GET /experimental/console/orgs` | ✓ | ✗ | |
| `POST /experimental/console/switch` | ✓ | ✗ | |
| `GET /experimental/tool` | ✓ | ✗ | **Gap 47** |
| `GET /experimental/tool/ids` | ✓ | ✗ | |
| `GET /experimental/worktree` | ✓ | ✗ | **Gap 48** |
| `POST /experimental/worktree` | ✓ | ✗ | |
| `DELETE /experimental/worktree` | ✓ | ✗ | |
| `POST /experimental/worktree/reset` | ✓ | ✗ | |
| `GET /experimental/session` | ✓ | ✗ | **Gap 49** |
| `POST /experimental/session/:sessionID/background` | ✓ | ✗ | |
| `GET /experimental/resource` | ✓ | ✗ | **Gap 50** |

### V2 API Routes (under /api/)

| Route | opencode | rustcode | Notes |
|-------|----------|----------|-------|
| `GET /api/health` | ✓ | ✗ | **Gap 51** |
| `GET /api/location` | ✓ | ✗ | **Gap 52** |
| `GET /api/agent` | ✓ | ✗ | |
| `GET /api/session` | ✓ | ✗ | **Gap 53** |
| `POST /api/session` | ✓ | ✗ | |
| `GET /api/session/:sessionID` | ✓ | ✗ | |
| `POST /api/session/:sessionID/prompt` | ✓ | ✗ | |
| `POST /api/session/:sessionID/compact` | ✓ | ✗ | |
| `POST /api/session/:sessionID/wait` | ✓ | ✗ | |
| `GET /api/session/:sessionID/context` | ✓ | ✗ | |
| `GET /api/session/:sessionID/message` | ✓ | ✗ | |
| `GET /api/session/:sessionID/message/:messageID` | ✓ | ✗ | |
| `GET /api/model` | ✓ | ✗ | |
| `GET /api/provider` | ✓ | ✗ | |
| `POST /api/integration` | ✓ | ✗ | |
| `POST /api/credential` | ✓ | ✗ | |
| `GET /api/permission` | ✓ | ✗ | |
| `POST /api/permission/:requestID/reply` | ✓ | ✗ | |
| `GET /api/fs/**` | ✓ | ✗ | |
| `GET /api/command` | ✓ | ✗ | |
| `GET /api/skill` | ✓ | ✗ | |
| `GET /api/event` (SSE) | ✓ | ✗ | |
| `GET /api/pty` | ✓ | ✗ | |
| `GET /api/question` | ✓ | ✗ | |
| `POST /api/question/:requestID/reply` | ✓ | ✗ | |
| `POST /api/question/:requestID/reject` | ✓ | ✗ | |
| `GET /api/reference` | ✓ | ✗ | |
| `POST /api/project-copy/:projectID/generate-name` | ✓ | ✗ | |

### Extra Endpoints (not in Instance API)

| Route | opencode | rustcode | Notes |
|-------|----------|----------|-------|
| `GET /doc` (OpenAPI) | ✓ | ✗ | **Gap 54** |
| `GET /metadata` | ✓ | ✓ | metadata.rs — works |
| `GET /event` (SSE) | ✓ | ✓ | event.rs — good impl |
| `GET /query` | ✓ | ✓ | query.rs — structured query |
| Catch-all `/*` UI proxy | ✓ | ✗ | **Gap 55** |

---

## 3. Middleware / Infrastructure Gaps

### Gap A: Auth Middleware — **CRITICAL**

**opencode** (`authorization.ts` in both packages):
- Checks `OPENCODE_SERVER_PASSWORD` env var
- Supports Basic auth via `Authorization: Basic <base64>` header
- Supports `auth_token` query parameter as bearer token alternative
- Public UI paths bypass auth (e.g., `/assets/*`, `/favicon.ico`)
- PTY connect ticket URLs bypass auth
- Adds `WWW-Authenticate: Basic realm="Secure Area"` header on 401
- Two variants: `Authorization` (HttpApiMiddleware) and `authorizationRouterMiddleware` (HttpRouter middleware)

**rustcode**: **No auth middleware at all.** Any request to any endpoint succeeds.

### Gap B: Compression Middleware — **MEDIUM**

**opencode** (`compression.ts`):
- `HttpMiddleware.compression` with content-type filtering
- Gzip/deflate for compressible MIME types
- Bypasses streaming SSE responses (they are already chunked)

**rustcode**: No compression middleware.

### Gap C: Error Response Standardization — **HIGH**

**opencode** (`errors.ts`):
- Uses `Schema.ErrorClass` with `{name: string, data: {message: string, ...}}` format
- HTTP status codes map to specific error classes:
  - 400: `InvalidRequestError`, `InvalidCursorError`
  - 401: `UnauthorizedError`
  - 403: `ForbiddenError`, `PtyForbiddenError`
  - 404: `ProviderNotFoundError`, `ModelNotFoundError`, `SessionNotFoundError`, `MessageNotFoundError`, `QuestionNotFoundError`, `PermissionNotFoundError`, `McpServerNotFoundError`, `PtyNotFoundError`, `ProjectNotFoundError`, `ApiNotFoundError`
  - 409: `ConflictError`, `SessionBusyError`
  - 500: `UnknownError`
  - 502: `UpstreamError`
  - 503: `ServiceUnavailableError`
  - 504: `TimeoutError`
- v2 API has `ApiNotFoundError` with `{name: "NotFoundError", data: {message: string}}`

**rustcode**: Ad-hoc error responses — some return `{"error": "..."}`, some return `{"message": "..."}`, some use `StatusCode + Json(...)`. No error enum.

### Gap D: Fence Middleware — **LOW**

**opencode** (`fence.ts`):
- Tracks workspace state changes during request processing
- Used to prevent concurrent modifications

**rustcode**: Not present. axum's state management may handle this differently.

### Gap E: Instance Context Middleware — **MEDIUM**

**opencode** (`instance-context.ts`):
- Provides `InstanceRef` to request handlers
- Attaches per-instance context (directory, workspace ID)

**rustcode**: Not present. State is passed explicitly.

### Gap F: Workspace Routing Middleware — **MEDIUM**

**opencode** (`workspace-routing.ts`):
- Routes requests to the correct workspace based on `directory` query param
- Extracts workspace context from the request

**rustcode**: Workspace context not automatically extracted.

### Gap G: Schema Error Middleware — **MEDIUM**

**opencode** (`schema-error.ts`):
- Transforms schema validation errors into `InvalidRequestError`
- Truncates reasons to 1024 chars

**rustcode**: No schema validation error middleware. Axum's built-in error handling is used.

### Gap H: Cors Vary Fix — **LOW**

**opencode** (`cors-vary.ts`):
- Ensures `Vary: Origin` header is set on responses

**rustcode**: Not explicitly handled (tower-http CorsLayer may handle this).

### Gap I: WebSocket Tracker — **LOW**

**opencode** (`websocket-tracker.ts`):
- Tracks active WebSocket connections for PTY sessions
- Allows force-closing all WebSockets during shutdown

**rustcode**: No WebSocket connection tracking.

### Gap J: PTY Connect Ticket Auth — **MEDIUM**

**opencode** (`pty.ts` + `ticket.ts`):
- PTY connect uses a signed ticket URL that bypasses normal auth
- `POST /pty/:ptyID/connect-token` generates a short-lived ticket
- `GET /pty/:ptyID/connect` validates the ticket and upgrades to WebSocket

**rustcode**: `connect-token` returns 501. WS handler exists but is echo-only.

### Gap K: CORS Config Not Plumbed — **HIGH**

**opencode**: `createRoutes(corsOptions?)` accepts CORS options and passes them to the cors middleware.

**rustcode** (`server.rs` line 132):
```rust
let cors = cors_layer(&[]);  // HARDCODED empty — always allows all origins
```
The `cors_origins` from `ServerConfig` is never passed to `cors_layer`.

### Gap L: OpenAPI Doc Endpoint — **MEDIUM**

**opencode**: `GET /doc` returns `OpenApi.fromApi(PublicApi)` — the auto-generated OpenAPI spec. Lazy-loaded so CLI processes don't pay the cost.

**rustcode**: No `/doc` endpoint.

### Gap M: Static File Serving / UI Proxy — **HIGH**

**opencode** (`ui.ts`):
- Serves embedded web UI from the package bundle
- Falls back to upstream proxy (https://opencode.ai) if embedded UI is disabled
- Bypasses auth for public UI paths

**rustcode**: No UI serving at all. The server has no way to serve a web interface.

### Gap N: Global Event SSE — **MEDIUM**

**opencode**: `GET /global/event` — SSE stream of global events (not scoped to an instance). Uses the global bus.

**rustcode**: Only `/event` exists (instance-scoped). `/global/event` route is declared in `global_routes` but likely not implemented or returns a stub.

---

## 4. Implementation Details Comparison

### Auth Logic (from opencode)

```
ServerAuth.required(config)  = config.password.isSome() && config.password.value !== ""
ServerAuth.authorized(creds, config) = creds.username === config.username && creds.password === config.password.value
```

open reads:
- `OPENCODE_SERVER_PASSWORD` (required, no default)
- `OPENCODE_SERVER_USERNAME` (optional, default "opencode")

Credential extraction:
1. Parse `auth_token` query param → decode as Base64 → split on `:` → `{username, password}`
2. Parse `Authorization: Basic <base64>` header → same decode
3. If neither, use empty credentials (will fail auth if password is required)

Public paths bypass:
- Checks path prefix against: `/`, `/assets`, `/favicon.ico`, `/api` (some), etc.

### Error Response Wire Format

**opencode** (Schema.ErrorClass serialization):
```json
// 401 Unauthorized
{ "name": "UnauthorizedError", "data": { "message": "Authentication required" } }

// 404 Session Not Found
{ "name": "SessionNotFoundError", "data": { "sessionID": "abc", "message": "Session not found" } }

// 400 Invalid Request
{ "name": "InvalidRequestError", "data": { "message": "Invalid cursor", "kind": "syntax" } }
```

The HttpApi system serializes TaggedErrorClass instances with this format automatically.

**rustcode** (current, inconsistent):
```json
// pty.rs (404)
{ "error": "PTY 'xxx' not found" }

// pty.rs (403)
{ "error": "Forbidden: message" }

// permission.rs (400)
{ "error": "invalid reply: 'foo'. Use once/always/reject" }
```

### Event SSE Format

**opencode**:
```
event: server.connected
data: {}

event: <event type from bus>
data: <JSON payload>

event: server.heartbeat
data: {}
```

**rustcode** (event.rs): Follows the same format — good.

---

## 5. Priority Action Items

### Phase 1: Critical (security + core functionality)

1. **Auth middleware** — Add `Authorization` middleware checking `OPENCODE_SERVER_PASSWORD`
   - Support Basic auth header
   - Support `auth_token` query param
   - Add public path bypass
   - Add PTY ticket URL bypass
   - Default username "opencode"

2. **Plumb CORS config** — Pass `cors_origins` from `ServerConfig` to `cors_layer()`
   - Currently hardcoded `&[]` on line 132 of server.rs

3. **Error standardization** — Create a unified error enum with proper status codes
   - Match opencode's error classes
   - Uniform JSON response format

### Phase 2: High (API parity)

4. **Implement missing route handlers** (in priority order):
   - Session sub-routes: status, children, todo, diff, fork, abort, share, init, summarize, send prompt, command, shell, revert, unrevert, delete message
   - File routes: find, find/file, find/symbol, file/content, file/status
   - VCS routes: /vcs, /vcs/status, /vcs/diff, /vcs/diff/raw, /vcs/apply
   - Provider auth: /provider/auth, OAuth endpoints
   - MCP management: POST /mcp, connect, disconnect, OAuth
   - LSP, formatter status
   - Question reject endpoint

5. **Add Global Event SSE** — Implement `/global/event` using the global bus

6. **Add v2 API routes** — Create `/api/*` route tree matching packages/server

### Phase 3: Medium (completeness)

7. **Compression middleware** — Add tower-http compression layer

8. **Static file serving / UI proxy** — Add catch-all route that proxies to upstream UI

9. **OpenAPI doc endpoint** — Add `GET /doc` with `utoipa` or manual spec

10. **PTY WebSocket bridging** — Connect WS handler to real PTY process via `portable-pty` or similar

11. **Workspace routing middleware** — Add automatic workspace context extraction

### Phase 4: Low (polish)

12. **Fence middleware** — Add workspace state tracking

13. **WebSocket tracker** — Track active WS connections for shutdown

14. **CORS Vary fix** — Ensure Vary: Origin header

15. **Schema error middleware** — Structured schema validation errors

---

## 6. opencode Key Reference Files

| File | Purpose |
|------|---------|
| `packages/opencode/src/server/routes/instance/httpapi/server.ts` | Route assembly, middleware layering |
| `packages/opencode/src/server/server.ts` | Server lifecycle, listen, stop |
| `packages/opencode/src/server/routes/instance/httpapi/middleware/authorization.ts` | Auth logic |
| `packages/opencode/src/server/auth.ts` | ServerAuth config, credential validation |
| `packages/opencode/src/server/routes/instance/httpapi/errors.ts` | Error classes |
| `packages/opencode/src/server/routes/instance/httpapi/api.ts` | API type definitions (InstanceHttpApi, RootHttpApi) |
| `packages/opencode/src/server/routes/instance/httpapi/groups/*.ts` | Route group definitions (21 files) |
| `packages/opencode/src/server/routes/instance/httpapi/handlers/*.ts` | Route handlers (one per group) |
| `packages/opencode/src/server/shared/ui.ts` | Embedded UI + proxy serving |
| `packages/opencode/src/cli/cmd/serve.ts` | CLI serve command |
| `packages/opencode/src/cli/network.ts` | Network options (port, hostname, cors, mdns) |
| `packages/server/src/` | v2 API: Api definition, handlers, middleware, groups |
| `packages/server/src/middleware/authorization.ts` | v2 API auth middleware |
| `packages/server/src/middleware/schema-error.ts` | v2 API schema error handler |
| `packages/server/src/errors.ts` | v2 API error classes |

## 7. rustcode Key Implementation Files

| File | Purpose | Status |
|------|---------|--------|
| `crates/rustcode-server/src/server.rs` | Router, AppState, serve, shutdown | Good structure, missing middleware |
| `crates/rustcode-server/src/cors.rs` | CORS layer factory | Good but unused config |
| `crates/rustcode-server/src/routes/mod.rs` | Route module declarations | 30 modules declared |
| `crates/rustcode-server/src/routes/session.rs` | Session CRUD | Most complete route file |
| `crates/rustcode-server/src/routes/event.rs` | SSE event stream | Good implementation |
| `crates/rustcode-server/src/routes/pty.rs` | PTY + WebSocket | Stub handlers, WS echo only |
| `crates/rustcode-server/src/routes/global.rs` | Global routes | Partial (missing global/event SSE) |
| `crates/rustcode-server/src/routes/control.rs` | Control routes | Good (auth set/remove, log) |
| `crates/rustcode-server/src/routes/health.rs` | Health check | Good |
| `crates/rustcode-server/src/routes/*.rs` | Other route modules | Various states of completion |
| `src/main.rs` (cmd_serve) | Server initialization | Passes config, builds state |

---

## 8. Summary Statistics

| Category | Total Routes | opencode | rustcode | Gap |
|----------|-------------|----------|----------|-----|
| Global / Control | 10 | 10 | ~8 | 2 missing |
| Session | 26 | 26 | ~8 | 18 missing |
| File / Find | 6 | 6 | ~1 | 5 missing |
| MCP | 8 | 8 | ~1 | 7 missing |
| VCS | 5 | 5 | 0 | 5 missing |
| Provider / Config | 7 | 7 | ~4 | 3 missing |
| Question / Permission | 6 | 6 | ~5 | 1 missing |
| Project | 5 | 5 | ~1 | 4 missing |
| PTY | 9 | 9 | ~6 | 3 stubs |
| Agent / Command / Skill | 3 | 3 | ~3 | 0 (all stubs) |
| Sync | 4 | 4 | 0 | 4 missing |
| TUI | 11 | 11 | 0 | 11 missing |
| Workspace / Experimental | 17 | 17 | ~1 | 16 missing |
| v2 API | 28 | 28 | 0 | 28 missing |
| Other (event, metadata, query, doc) | 4 | 4 | ~3 | 1 (doc) |
| **Total** | **149** | **149** | **~41** | **108 missing/stub** |

### Middleware Gaps

| Middleware | opencode | rustcode | Priority |
|-----------|----------|----------|----------|
| Auth | ✓ | ✗ | CRITICAL |
| CORS (plumbed) | ✓ | ✗ | HIGH |
| Error standardization | ✓ | ✗ | HIGH |
| Compression | ✓ | ✗ | MEDIUM |
| Static UI | ✓ | ✗ | HIGH |
| Global event SSE | ✓ | ✗ | MEDIUM |
| v2 API routes | ✓ | ✗ | HIGH |
| Workspace routing | ✓ | ✗ | MEDIUM |
| Instance context | ✓ | ✗ | MEDIUM |
| Schema error | ✓ | ✗ | MEDIUM |
| Fence | ✓ | ✗ | LOW |
| WebSocket tracker | ✓ | ✗ | LOW |
| CORS Vary fix | ✓ | ✗ | LOW |
| OpenAPI doc | ✓ | ✗ | MEDIUM |
| PTY ticket auth | ✓ | ✗ | MEDIUM |
