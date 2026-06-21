# Server System — Gap Analysis

## Architecture

| Aspect | TS | Rust |
|--------|----|------|
| Framework | Effect HttpApi | axum 0.8 |
| DI | Effect Layer | `Arc<AppState>` |
| Routes | 3-tier: RootHttpApi + InstanceHttpApi + Api | Single flat Router |
| Error model | 22 typed errors (Schema.TaggedErrorClass) | 18 enum variants |
| OpenAPI | `/doc` with legacy transform | **None** |
| mDNS | Bonjour publish/unpublish | **Missing** |
| Files | ~65 | 37 |

## Route Group Parity

| Group | TS Routes | Rust Routes | Parity |
|-------|-----------|-------------|--------|
| Health/Global | 6 | 6 | 90% |
| Config | 3 | 3 | 90% |
| Control | 3 | 3 | 90% |
| Event SSE | 2 | 2 | 70% |
| Instance | 12 | 11 | 65% |
| File | 6 | 6 | 70% |
| Experimental | 11 | 10 | 65% |
| Session | 27 | 30 | 80% |
| PTY | 8 | 8 | **40%** (most 501 stubs) |
| MCP | 8 | 7 | 70% |
| Permission | 2 | 2 | 80% |
| Project | 5 | 5 | 70% |
| Provider | 4 | 4 | 70% |
| Question | 3 | 3 | 80% |
| TUI | 15 | 15 | 80% |
| Workspace | 7 | 7 | 70% |
| V2 API | ~25 | 30+ | 50% |
| **Middleware** | **8** | **3** | **30%** |

## Middleware Parity

| Middleware | TS | Rust |
|------------|----|------|
| Authorization | Full | Full |
| CORS | Full | Partial (missing Vary:Origin) |
| Compression | Full (SSE bypass, threshold) | Partial (default tower-http) |
| Error | Full (UUID refs, ConfigError) | Partial (no UUID refs) |
| **Fence (sync)** | Full | **MISSING** |
| **Instance Context** | Full | **MISSING** |
| **Proxy** | Full (WS + HTTP) | **MISSING** |
| **Schema Error** | Full | **MISSING** |
| **Workspace Routing** | Full (250L) | **MISSING** |
| **Lifecycle/Dispose** | Full | **MISSING** |

## 5 Most Critical Gaps

### 1. Workspace Routing Middleware
250 lines handling local vs remote workspace resolution, session lookup, proxy delegation, sync fence wait.

**TS**: `middleware/workspace-routing.ts:237`
**Rust**: **NOT IMPLEMENTED**

### 2. Fence (Sync Barrier)
Reads EventSequenceTable before/after mutations, computes diff, emits `x-opencode-sync` header.

**TS**: `middleware/fence.ts:9`
**Rust**: **MISSING**

### 3. Instance Context and Lifecycle
Loads instance by directory, provisions InstanceRef/WorkspaceRef, handles deferred disposal.

**TS**: `instance-context.ts:37`, `lifecycle.ts:43`
**Rust**: **NOT IMPLEMENTED**

### 4. Proxy Middleware (HTTP + WebSocket)
Bidirectional WebSocket proxy and HTTP proxy with header sanitization.

**TS**: `middleware/proxy.ts:14`
**Rust**: **NOT IMPLEMENTED**

### 5. Schema Validation / Error Formatting
Truncated validation messages, UUID error refs, ConfigV1 detection.

**TS**: `schema-error.ts:25`, `error.ts:28`
**Rust**: Generic 500s, no error refs, no ConfigV1 handling
