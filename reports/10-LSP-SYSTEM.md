# LSP System — Gap Analysis

## Architecture

| Aspect | TS | Rust |
|--------|----|------|
| JSON-RPC | `vscode-jsonrpc` (full protocol) | Manual Content-Length framing |
| State | Effect InstanceState + Context.Service | `Arc<RwLock>` + `OnceLock` singleton |
| Files | 6 (client, diagnostic, language, launch, lsp, server) | 2 (lib.rs + core/lsp.rs) |
| Server catalog | 28 with custom root/spawn/install logic | 34 static entries |

## Feature Parity

| LSP Method | TS | Rust | Status |
|-----------|-----|------|--------|
| initialize | ✅ | ✅ | ✅ |
| textDocument/hover | ✅ | ✅ | ✅ |
| textDocument/definition | ✅ | ✅ | ✅ |
| textDocument/references | ✅ | ✅ | ✅ |
| textDocument/implementation | ✅ | ✅ | ✅ |
| textDocument/documentSymbol | ✅ | ✅ | ✅ |
| workspace/symbol | ✅ | ✅ (no filtering/limit) | ⚠️ |
| callHierarchy (prepare/incoming/outgoing) | ✅ | ✅ | ✅ |
| textDocument/completion | ❌ (not exposed) | ✅ | Rust has extra |
| **textDocument/diagnostic (pull)** | ✅ **Full** | ❌ | **CRITICAL** |
| **workspace/diagnostic (pull)** | ✅ **Full** | ❌ | **CRITICAL** |
| textDocument/publishDiagnostics | ✅ | ✅ | ✅ |
| textDocument/didChange | ✅ | ✅ | ✅ |
| textDocument/didClose | ✅ | ❌ | Missing |
| textDocument/didSave | ✅ | ✅ | ✅ |
| workspace/didChangeWatchedFiles | ✅ | ✅ | ✅ |
| workspace/didChangeConfiguration | ✅ | ✅ | ✅ |
| client/registerCapability | ✅ | ✅ | ✅ |

## Diagnostics Pipeline

| Feature | TS | Rust |
|---------|----|------|
| Push diagnostics | ✅ Cached with listeners | ✅ Stored in Vec |
| **Pull diagnostics** | ✅ Full (document + workspace) | ❌ **Entirely absent** |
| Debounced wait | ✅ `waitForFreshPush` with 150ms | ❌ |
| Event-driven retry | ✅ `Promise.race` push/registration | ❌ |
| Deduplication | ✅ JSON-key dedup | ❌ |

## Server Lifecycle

| Feature | TS | Rust |
|---------|----|------|
| Spawn deduplication | ✅ `spawning: Map` | ❌ **Missing** — race condition |
| Broken server tracking | ✅ `broken: Set` | ❌ **Missing** — retries failed servers |
| Process ID in initialize | ✅ `processId: process.pid` | ❌ `"processId": null` |
| Graceful shutdown | ✅ `connection.end()` + `dispose()` | ✅ `shutdown` + `exit` |
| User config override | ✅ `cfg.lsp` | ❌ **Missing** |
| Dynamic root detection | ✅ Walk-up search | ❌ Static only |

## 5 Most Critical Gaps

### 1. Pull Diagnostics (no `textDocument/diagnostic` / `workspace/diagnostic`)
**TS**: `client.ts:293-444` — Full pull pipeline with registration tracking, previousResultIds, retry.
**Rust**: ❌ Only push diagnostics handled. Many modern servers use pull primarily.

### 2. Wait-for-Diagnostics with Debounce
**TS**: `client.ts:464-541` — Event-driven wait with 150ms debounce.
**Rust**: Returns immediately after `didOpen` — no diagnostic waiting.

### 3. Dynamic Root Detection and Server Auto-Install
**TS**: 28 server definitions with custom root-finding, downloading, installing.
**Rust**: Static catalog — servers must be on `$PATH`, no auto-install.

**TS**: `server.ts:1-1983`
**Rust**: `lib.rs:294-425` (static), `456-540` (simple config matching)

### 4. Spawn Deduplication + Broken Server Tracking
**TS**: `lsp.ts:117-118` — dedup spawns, skip broken servers.
**Rust**: Race condition on concurrent spawns, infinite retry on failure.

### 5. User Configuration Override
**TS**: `lsp.ts:162-184` — users disable/enable/configure servers.
**Rust**: Static catalog — no user configuration path.
