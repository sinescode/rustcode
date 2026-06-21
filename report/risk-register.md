# Risk Register — RustCode Transformation Program

**Generated:** 2026-06-21  
**Source:** 20 agent reports across RustCode and OpenCode codebases  
**Total Risks Identified:** 87  

---

## Summary Tables

### Risk Distribution by Category

| Category | Critical | High | Medium | Low | Total |
|----------|----------|------|--------|-----|-------|
| Data Loss (DL) | 2 | 1 | 2 | 0 | 5 |
| Security (SEC) | 1 | 7 | 5 | 0 | 13 |
| Performance (PERF) | 5 | 5 | 2 | 0 | 12 |
| Reliability (REL) | 6 | 8 | 1 | 0 | 15 |
| Architecture (ARCH) | 10 | 5 | 2 | 0 | 17 |
| Business (BIZ) | 0 | 4 | 2 | 0 | 6 |
| Technical Debt (DEBT) | 5 | 7 | 3 | 0 | 15 |
| Scalability (SCALE) | 4 | 4 | 1 | 0 | 9 |
| **Total** | **33** | **41** | **18** | **0** | **92** |

### Risk Distribution by Level

| Risk Level | Score Range | Count |
|------------|-------------|-------|
| Critical | 20–25 | 33 |
| High | 12–19 | 41 |
| Medium | 6–11 | 18 |
| Low | 1–5 | 0 |

### Top 10 Risks by Score

| Rank | Risk ID | Title | Score | Level |
|------|---------|-------|-------|-------|
| 1 | RISK-DL-001 | SQL NULL corruption via clear_revert | 25 | Critical |
| 2 | RISK-DL-002 | Epoch snapshot corruption via Some() wrapping | 25 | Critical |
| 3 | RISK-REL-001 | No provider retry mechanism | 25 | Critical |
| 4 | RISK-REL-002 | No timeouts on provider calls | 25 | Critical |
| 5 | RISK-REL-003 | No signal handling / graceful shutdown | 25 | Critical |
| 6 | RISK-REL-005 | File lock TOCTOU race | 25 | Critical |
| 7 | RISK-REL-007 | Error context silently discarded (i32 exit codes) | 25 | Critical |
| 8 | RISK-ARCH-010 | Fragmented error hierarchy (5 error types) | 25 | Critical |
| 9 | RISK-DEBT-001 | 300+ panic!() calls in production paths | 25 | Critical |
| 10 | RISK-DEBT-002 | 500+ .unwrap() calls in library code | 25 | Critical |

### Risk Heat Map

```
Impact →
  5 | DL-001 DL-002  REL-001/002/003/005/007  ARCH-001/002/005/006/007/010  DEBT-001/002/003/004
    | REL-006 REL-011 PERF-001/002/003/004/007 REL-012 SEC-001 ARCH-013 ARCH-014 ARCH-015
    | ARCH-017 SCALE-001/002/003/008 DEBT-012
  4 | PERF-005 PERF-009 PERF-011 REL-004 REL-009 REL-010 REL-013 BIZ-001
    | ARCH-003 ARCH-004 ARCH-008 ARCH-009 ARCH-011 BIZ-003 SCALE-004 SCALE-005
    | SCALE-007 DEBT-005 DEBT-006 DEBT-007 DEBT-011 DEBT-013
  3 | SEC-002 SEC-003 SEC-004 SEC-006 SEC-007 SEC-010 BIZ-002 BIZ-005
    | MEDIUM entries...
  2 |
  1 |
     1    2    3    4    5
                    Probability →
```

---

## DATA LOSS RISKS

---

### RISK-DL-001: SQL NULL Corruption via clear_revert

- **Title**: clear_revert writes literal string "null" instead of SQL NULL
- **Description**: `session.rs:1208-1209` passes `Some("null")` to `update_session`, which writes the 4-character text `"null"` into the SQLite column instead of SQL `NULL`. The comment says "empty string" but the code writes the literal string "null". Any `WHERE revert IS NULL` query will miss this row. Deserialization incidentally treats it as `None` but the column is physically corrupted.
- **Category**: Data Loss
- **Probability**: 5 (almost certain — happens on every clear_revert call)
- **Impact**: 5 (catastrophic — corrupts session data integrity)
- **Risk Score**: 25
- **Risk Level**: Critical
- **Affected Component**: `rustcode-core/src/session.rs:1208-1209`
- **Agent Source**: Agent 04 — Logic Verification (C-1)
- **Current Mitigation**: Deserialization `serde_json::from_str::<RevertInfo>("null").ok()` incidentally returns `None`, masking the bug in some code paths
- **Recommended Mitigation**: Pass `None` instead of `Some("null")` to set the column to SQL `NULL`. Fix the single parameter value.
- **Owner**: Session Team Lead
- **Timeline**: Immediate (0.1 person-days)
- **Status**: Open

---

### RISK-DL-002: Epoch Snapshot Corruption via Some() Wrapping

- **Title**: Epoch snapshot corrupted by redundant Option::Some() wrapping in JSON
- **Description**: `session_runner.rs:703-717` uses `compact_result.as_ref().map(|r| r.summary.clone())` inside `serde_json::json!({...})`. The `.map()` produces `Option<String>` inside the `json!` macro, resulting in JSON like `{"summary": Some("compacted_text")}` with literal `Some(...)` wrappers. Downstream consumers trying to deserialize the snapshot fail.
- **Category**: Data Loss
- **Probability**: 5 (almost certain — triggered on every compaction)
- **Impact**: 5 (catastrophic — corrupts epoch snapshot storage)
- **Risk Score**: 25
- **Risk Level**: Critical
- **Affected Component**: `rustcode-core/src/session_runner.rs:703-717`
- **Agent Source**: Agent 04 — Logic Verification (C-3)
- **Current Mitigation**: None
- **Recommended Mitigation**: Replace with `if let Some(ref result) = compact_result { ... result.summary ... }` to avoid double-wrapping. Also fix the `unwrap()` on line 710 and eliminate the `prepare_epoch` double-call.
- **Owner**: Session Team Lead
- **Timeline**: Immediate (0.5 person-days)
- **Status**: Open

---

### RISK-DL-003: Session State Corruption on Crash

- **Title**: No incremental persistence during session execution — crash loses all progress
- **Description**: `session_runner.rs:957` does not persist intermediate events or tool results during execution. Only the final `SessionRunResult` is returned. If the process crashes mid-tool-loop, all progress for the entire turn is lost. The user must restart from scratch.
- **Category**: Data Loss
- **Probability**: 3 (possible — crashes happen)
- **Impact**: 5 (catastrophic — all in-flight work lost)
- **Risk Score**: 15
- **Risk Level**: High
- **Affected Component**: `rustcode-core/src/session_runner.rs:957-1155`
- **Agent Source**: Agent 16 — Reliability (2.2), Agent 07 — Scalability (5)
- **Current Mitigation**: None
- **Recommended Mitigation**: Persist each tool call result incrementally to the database. Use event sourcing for LLM events as they arrive. Wire EventV2 replay into session initialization.
- **Owner**: Session Team Lead
- **Timeline**: 2–4 weeks
- **Status**: Open

---

### RISK-DL-004: Event Replay Inconsistency

- **Title**: Catch-up projection not atomic — partial replay leaves mixed state
- **Description**: `EventProjector::catch_up()` at `event_projector.rs:144` processes events sequentially without atomic batch semantics. If the 50th of 500 events fails, the first 49 have already been processed. The next catch-up may double-process or skip events.
- **Category**: Data Loss
- **Probability**: 2 (unlikely — requires mid-replay failure)
- **Impact**: 4 (major — inconsistent projection state)
- **Risk Score**: 8
- **Risk Level**: Medium
- **Affected Component**: `rustcode-core/src/event_projector.rs:144-221`
- **Agent Source**: Agent 16 — Reliability (13.1)
- **Current Mitigation**: None
- **Recommended Mitigation**: Add a transaction around catch-up projection. If any event fails, roll back to the last known-good checkpoint.
- **Owner**: Data Team Lead
- **Timeline**: 2–4 weeks
- **Status**: Open

---

### RISK-DL-005: Database Corruption on Power Loss

- **Title**: No fsync on JSON storage writes — data loss on crash
- **Description**: `Storage::write()` at `storage.rs:454` uses `std::fs::write()` which does not guarantee data is flushed to disk. No explicit `fsync()` call. A crash after `write()` returns but before data reaches disk loses the written data.
- **Category**: Data Loss
- **Probability**: 2 (unlikely — requires crash during write)
- **Impact**: 4 (major — data loss for in-flight writes)
- **Risk Score**: 8
- **Risk Level**: Medium
- **Affected Component**: `rustcode-core/src/storage.rs:454`
- **Agent Source**: Agent 16 — Reliability (6.2)
- **Current Mitigation**: SQLite WAL mode protects database writes; JSON file storage is unprotected
- **Recommended Mitigation**: Use `File::create()` + `write_all()` + `sync_all()` for all JSON storage writes.
- **Owner**: Data Team Lead
- **Timeline**: 1–2 weeks
- **Status**: Open

---

## SECURITY RISKS

---

### RISK-SEC-001: No Encryption at Rest for Credentials

- **Title**: All stored credentials and tokens in plaintext SQLite and JSON files
- **Description**: `account.access_token`, `account.refresh_token`, `credential.value`, `mcp-auth.json`, and `auth.json` are all stored in plaintext. The encryption module (`encryption/hmac.rs`) referenced in OpenCode does not exist in RustCode. Anyone with filesystem access to the SQLite database file can read API tokens, OAuth tokens, and credentials.
- **Category**: Security
- **Probability**: 3 (possible — filesystem access may be compromised)
- **Impact**: 5 (catastrophic — full credential exposure)
- **Risk Score**: 15
- **Risk Level**: Critical
- **Affected Component**: `rustcode-core/src/credential.rs:352-353`, `rustcode-core/src/auth.rs:195-206`, `rustcode-core/src/mcp.rs:2263-2276`
- **Agent Source**: Agent 05 — Security (1.1, 3.1, 3.6, 4.1)
- **Current Mitigation**: File permissions set to `0o600` for auth files
- **Recommended Mitigation**: Implement credential encryption using platform keychain (macOS Keychain, Linux Secret Service, Windows Credential Manager). At minimum, use AES-256-GCM with a device-derived key.
- **Owner**: Security Lead
- **Timeline**: 2–4 weeks
- **Status**: Open

---

### RISK-SEC-002: Path Traversal via {file:} Substitution

- **Title**: Config {file:path} variable substitution reads arbitrary files
- **Description**: `config.rs:2796-2789` implements `{file:path}` substitution that reads arbitrary files from the filesystem. An attacker who can trick a user into loading a crafted config can exfiltrate local files (SSH keys, `/etc/passwd`, etc.). No path canonicalization or project-directory boundary check.
- **Category**: Security
- **Probability**: 3 (possible — requires malicious config)
- **Impact**: 4 (major — arbitrary file read)
- **Risk Score**: 12
- **Risk Level**: High
- **Affected Component**: `rustcode-core/src/config.rs:2796-2789`
- **Agent Source**: Agent 05 — Security (12.3, 12.4)
- **Current Mitigation**: None
- **Recommended Mitigation**: Use `std::fs::canonicalize()` and verify the resolved path is within the project directory. Restrict `{file:}` to the project tree.
- **Owner**: Security Lead
- **Timeline**: 1–2 weeks
- **Status**: Open

---

### RISK-SEC-003: MCP Sandbox Escape via Local Server Execution

- **Title**: MCP local servers run with full user privileges — no sandbox
- **Description**: `McpClient::connect()` at `mcp.rs:1044-1051` spawns subprocesses with the command and args from configuration. MCP servers run with full user permissions. An attacker who can modify config can execute arbitrary commands with the user's privileges. OpenCode's SECURITY.md explicitly states "No Sandbox."
- **Category**: Security
- **Probability**: 3 (possible — requires config modification)
- **Impact**: 4 (major — arbitrary command execution)
- **Risk Score**: 12
- **Risk Level**: High
- **Affected Component**: `rustcode-core/src/mcp.rs:1044-1051`
- **Agent Source**: Agent 05 — Security (9.1)
- **Current Mitigation**: Permission system gates tool access but does not sandbox MCP processes
- **Recommended Mitigation**: Consider running MCP servers in restricted contexts (containers, landlock, seccomp). Validate command paths. Document plugin trust model.
- **Owner**: Security Lead
- **Timeline**: 1–2 months
- **Status**: Open

---

### RISK-SEC-004: V1 Run Loop Permission Bypass

- **Title**: V1 run_loop executes tools with zero permission enforcement
- **Description**: `session_runner.rs:1086-1096` — V1 `run_loop` sets `ask_fn: None` and `permission_source: None`, then calls `execute_by_name` which performs zero permission checks. Every code path using `run_loop` (including `run()` and `run_with_messages()`) executes tools with no allow/deny/ask gate.
- **Category**: Security
- **Probability**: 5 (almost certain — every V1 execution path)
- **Impact**: 4 (major — LLM can call any tool without permission)
- **Risk Score**: 20
- **Risk Level**: Critical
- **Affected Component**: `rustcode-core/src/session_runner.rs:1086-1096`
- **Agent Source**: Agent 04 — Logic Verification (C-2)
- **Current Mitigation**: None — permission system exists but is intentionally bypassed in V1 paths
- **Recommended Mitigation**: Wire `ask_fn` from the caller into the V1 `run_loop` path. Switch V1 from `execute_by_name` to `execute_with_pipeline`. Ensure `permission_source` is populated.
- **Owner**: Security Lead
- **Timeline**: Immediate (1 person-day)
- **Status**: Open

---

### RISK-SEC-005: Supply Chain — Ignored Advisory

- **Title**: RUSTSEC-2024-0436 ignored in deny.toml without documented rationale
- **Description**: `deny.toml` ignores `RUSTSEC-2024-0436` advisory. The affected crate and exploitation context are unknown. The `wildcards = "allow"` setting permits imprecise version specs. `unknown-registry = "warn"` and `unknown-git = "warn"` are lenient.
- **Category**: Security
- **Probability**: 2 (unlikely — advisory may not be exploitable)
- **Impact**: 3 (moderate — supply chain vulnerability)
- **Risk Score**: 6
- **Risk Level**: Medium
- **Affected Component**: `deny.toml:3`
- **Agent Source**: Agent 05 — Security (8.1, 8.2, 8.3), Agent 11 — Dependencies
- **Current Mitigation**: Advisory is ignored; `cargo-deny` runs in CI
- **Recommended Mitigation**: Investigate RUSTSEC-2024-0436 and document rationale. Set `wildcards = "deny"`, `unknown-registry = "deny"`, `unknown-git = "deny"`.
- **Owner**: Security Lead
- **Timeline**: 2–4 weeks
- **Status**: Open

---

### RISK-SEC-006: No TLS in Server

- **Title**: rustcode-server runs HTTP only — credentials transmitted in plaintext
- **Description**: Server crate has no TLS support. Authentication credentials (`auth_token` query parameter, `OPENCODE_SERVER_PASSWORD`) are transmitted in plaintext over HTTP. URLs logged by proxies, visible in browser history, leaked via Referer headers.
- **Category**: Security
- **Probability**: 3 (possible — server mode is used)
- **Impact**: 4 (major — credential interception)
- **Risk Score**: 12
- **Risk Level**: High
- **Affected Component**: `rustcode-server/src/auth.rs:81-87`
- **Agent Source**: Agent 05 — Security (1.3), Agent 20 — Production Readiness
- **Current Mitigation**: None
- **Recommended Mitigation**: Add `--tls-cert` / `--tls-key` CLI flags for HTTPS. Remove query-param auth support or add warning log.
- **Owner**: Server Team Lead
- **Timeline**: 2–4 weeks
- **Status**: Open

---

### RISK-SEC-007: No CSRF Protection

- **Title**: Server endpoints are vulnerable to CSRF attacks
- **Description**: State-changing server endpoints (`POST /session`, `DELETE /session`, etc.) have no CSRF protection. A malicious website could trigger state changes if a user has the server running locally.
- **Category**: Security
- **Probability**: 2 (unlikely — local server only)
- **Impact**: 4 (major — unauthorized state changes)
- **Risk Score**: 8
- **Risk Level**: High
- **Affected Component**: `rustcode-server/src/routes/`
- **Agent Source**: Agent 20 — Production Readiness
- **Current Mitigation**: Server is local-only by default
- **Recommended Mitigation**: Implement CSRF tokens for state-changing endpoints. Add `SameSite` cookie attributes.
- **Owner**: Server Team Lead
- **Timeline**: 2–4 weeks
- **Status**: Open

---

### RISK-SEC-008: API Keys in Process Memory Not Zeroed

- **Title**: API keys live in heap memory until process exit; no zeroing
- **Description**: All provider implementations read API keys from env vars into `String`. These strings remain in heap memory until the process exits. A memory-dump attack or core dump could expose API keys.
- **Category**: Security
- **Probability**: 2 (unlikely — requires memory access)
- **Impact**: 3 (moderate — API key exposure)
- **Risk Score**: 6
- **Risk Level**: Medium
- **Affected Component**: All provider modules (`providers/*`)
- **Agent Source**: Agent 05 — Security (3.3)
- **Current Mitigation**: None
- **Recommended Mitigation**: Use `secrecy::SecretString` for all API key and token fields. Implement zeroing on drop.
- **Owner**: Security Lead
- **Timeline**: 1–2 months
- **Status**: Open

---

### RISK-SEC-009: No API Rate Limiting

- **Title**: No rate limiting — single client can exhaust resources
- **Description**: Server has no rate limiting middleware. A single client can exhaust LLM API budget, saturate SQLite, or consume all available SSE connections. Provider rate limits (e.g., Anthropic 429s) are surfaced as errors, not handled gracefully.
- **Category**: Security
- **Probability**: 3 (possible — server mode)
- **Impact**: 3 (moderate — resource exhaustion)
- **Risk Score**: 9
- **Risk Level**: Medium
- **Affected Component**: `rustcode-server/src/`
- **Agent Source**: Agent 07 — Scalability (14)
- **Current Mitigation**: None
- **Recommended Mitigation**: Implement token bucket rate limiter. Add per-route rate limiting middleware. Add daily/monthly token usage caps.
- **Owner**: Server Team Lead
- **Timeline**: 1–2 months
- **Status**: Open

---

### RISK-SEC-010: MCP OAuth Token in Plaintext File

- **Title**: MCP OAuth tokens stored in plaintext `mcp-auth.json`
- **Description**: `mcp.rs:2263-2276` stores OAuth access and refresh tokens in `mcp-auth.json` in plaintext. PKCE verifier persisted during OAuth flow. An attacker with filesystem access can steal OAuth tokens.
- **Category**: Security
- **Probability**: 3 (possible — filesystem compromise)
- **Impact**: 4 (major — OAuth token theft)
- **Risk Score**: 12
- **Risk Level**: High
- **Affected Component**: `rustcode-core/src/mcp.rs:2263-2276`, `rustcode-core/src/mcp_oauth.rs:1003-1013`
- **Agent Source**: Agent 05 — Security (3.1, 3.5)
- **Current Mitigation**: PKCE verifier deleted after token exchange
- **Recommended Mitigation**: Encrypt mcp-auth.json at rest. Delete PKCE verifier immediately after token exchange (already done).
- **Owner**: MCP Team Lead
- **Timeline**: 2–4 weeks
- **Status**: Open

---

### RISK-SEC-011: Plugin Auto-Install Without Integrity Check

- **Title**: Plugin auto-installs npm/bun dependencies without integrity verification
- **Description**: `config.rs:1836-1886` auto-installs npm/bun dependencies from config-specified directories. No code signing or integrity verification. An attacker can supply malicious packages via config manipulation.
- **Category**: Security
- **Probability**: 2 (unlikely — requires config access)
- **Impact**: 3 (moderate — malicious code execution)
- **Risk Score**: 6
- **Risk Level**: Medium
- **Affected Component**: `rustcode-core/src/config.rs:1836-1886`
- **Agent Source**: Agent 05 — Security (10.1, 10.2)
- **Current Mitigation**: None
- **Recommended Mitigation**: Add package integrity verification (lockfile, hash checking). Validate package names before install.
- **Owner**: Plugin Team Lead
- **Timeline**: 1–2 months
- **Status**: Open

---

### RISK-SEC-012: Permission Cascade — Reject Fails All Pending

- **Title**: User rejecting one permission cascades to fail ALL pending requests
- **Description**: `permission.rs:1126-1128` — rejecting one permission cascades to fail all pending permission requests in the same session. "Always" cascade auto-approves all pending. This broad-brush approach can cause unexpected denials or approvals.
- **Category**: Security
- **Probability**: 2 (unlikely — specific UX pattern)
- **Impact**: 3 (moderate — scope-of-permission confusion)
- **Risk Score**: 6
- **Risk Level**: Medium
- **Affected Component**: `rustcode-core/src/permission.rs:1099-1166`
- **Agent Source**: Agent 05 — Security (7.2, 7.3)
- **Current Mitigation**: Matches OpenCode behavior (by design)
- **Recommended Mitigation**: Only cascade for same permission+pattern pair. Document the cascade behavior explicitly.
- **Owner**: Permission Team Lead
- **Timeline**: 1–2 months
- **Status**: Open

---

### RISK-SEC-013: File Write Tool Permission Resource Wildcard

- **Title**: Permission check always passes "*" as resource — defeats pattern granularity
- **Description**: `tool.rs:502` passes `"*"` as the resource pattern in `ctx.ask(name, "*")`, regardless of the tool being called. For file tools (`read`, `write`, `edit`), the actual file path should be passed for fine-grained permission evaluation. With `"*"`, all permission rules match all resources.
- **Category**: Security
- **Probability**: 5 (almost certain — every tool call)
- **Impact**: 3 (moderate — permission granularity defeated)
- **Risk Score**: 15
- **Risk Level**: High
- **Affected Component**: `rustcode-core/src/tool.rs:502`
- **Agent Source**: Agent 04 — Logic Verification (H-8)
- **Current Mitigation**: Permission system exists but resource-level granularity is disabled
- **Recommended Mitigation**: Extract the resource from tool arguments. For file tools, extract `filePath`/`path` from `args`. For bash, extract the command. Pass the real resource to `ctx.ask()`.
- **Owner**: Permission Team Lead
- **Timeline**: 1–2 weeks
- **Status**: Open

---

## PERFORMANCE RISKS

---

### RISK-PERF-001: Blocking Synchronous I/O on Async Runtime

- **Title**: Synchronous std::fs operations block tokio worker threads
- **Description**: All filesystem operations in tool implementations (ReadTool, WriteTool, EditTool, grep_search) use synchronous `std::fs` APIs on the async runtime. `std::fs::read_to_string` blocks the tokio worker thread for the entire file read duration. Large files block for milliseconds. In a server context, this blocks all connected clients.
- **Category**: Performance
- **Probability**: 5 (almost certain — every file operation)
- **Impact**: 4 (major — blocks async runtime, cascading latency)
- **Risk Score**: 20
- **Risk Level**: Critical
- **Affected Component**: `rustcode-core/src/tool_impls.rs:1065-1238`, `rustcode-core/src/filesystem.rs:1281`
- **Agent Source**: Agent 06 — Performance (7.3.1), Agent 20 — Production Readiness (3)
- **Current Mitigation**: None
- **Recommended Mitigation**: Use `tokio::fs` versions or wrap blocking I/O in `tokio::task::spawn_blocking`. Filesystem operations longer than 50µs should be offloaded.
- **Owner**: Performance Team Lead
- **Timeline**: 2–4 weeks
- **Status**: Open

---

### RISK-PERF-002: Full Message History Clone Per Tool Call

- **Title**: ToolContext clones entire Vec<ChatMessage> history per tool call
- **Description**: `session_runner.rs:749` performs `ctx.messages = messages.clone()` which deep-clones the entire `Vec<ChatMessage>` history into every tool invocation. For 50 messages at ~2KB each, that's ~100KB per tool call. With 25 tool calls per session, that's 2.5MB of cloned message data per turn.
- **Category**: Performance
- **Probability**: 5 (almost certain — every tool call)
- **Impact**: 4 (major — 2.5MB+ deep clones per turn)
- **Risk Score**: 20
- **Risk Level**: Critical
- **Affected Component**: `rustcode-core/src/session_runner.rs:749`, `rustcode-core/src/tool.rs:47`
- **Agent Source**: Agent 06 — Performance (4.1.2), Agent 03 — Rust Expert (1.4)
- **Current Mitigation**: None
- **Recommended Mitigation**: Store `Arc<Vec<ChatMessage>>` or `Arc<[ChatMessage]>` in `ToolContext` instead of `Vec<ChatMessage>`. Eliminates 2.5MB+ of clones per session.
- **Owner**: Performance Team Lead
- **Timeline**: 1–2 weeks (0.5 person-days)
- **Status**: Open

---

### RISK-PERF-003: EventPayload Clone Storm on Sync Events

- **Title**: EventPayload cloned 8+ times per sync event publication
- **Description**: `event.rs:936,945,1025,1035` — each sync event publication clones `EventPayload` for commit guards, projectors, sync handlers, aggregate subscribers, listeners, typed channel, and global channel. `EventPayload` contains `EventId` (String), `event_type` (String), `data` (Value), `location` (Option), `metadata` (Option) — easily 500+ bytes per clone. 8+ clones per event = 4KB+ cloned per sync event.
- **Category**: Performance
- **Probability**: 5 (almost certain — every sync event)
- **Impact**: 4 (major — unnecessary copies in hot path)
- **Risk Score**: 20
- **Risk Level**: Critical
- **Affected Component**: `rustcode-core/src/event.rs:936,945,1025,1035`
- **Agent Source**: Agent 06 — Performance (4.1.4)
- **Current Mitigation**: Some clones are necessary for broadcast channels
- **Recommended Mitigation**: Pass `&EventPayload` where possible. Use `Arc<EventPayload>` for broadcast paths.
- **Owner**: Event System Team Lead
- **Timeline**: 2–4 weeks
- **Status**: Open

---

### RISK-PERF-004: DB Transaction Held During Async Work

- **Title**: Database transaction held open during async projectors and commit hooks
- **Description**: `event.rs:899-984` — the event publish transaction holds the SQLite transaction open while running async commit guards, projectors, and commit hooks. If these take 100ms, the transaction blocks other writers for 100ms. This increases contention and risk of `SQLITE_BUSY`.
- **Category**: Performance
- **Probability**: 4 (likely — every sync event)
- **Impact**: 5 (catastrophic — blocks all other DB writers)
- **Risk Score**: 20
- **Risk Level**: Critical
- **Affected Component**: `rustcode-core/src/event.rs:899-984`
- **Agent Source**: Agent 15 — Database (5), Agent 06 — Performance (8.3)
- **Current Mitigation**: None
- **Recommended Mitigation**: Move projectors and commit hooks outside the transaction. Only the seq UPSERT + event INSERT need to be in a transaction.
- **Owner**: Data Team Lead
- **Timeline**: 2–4 weeks
- **Status**: Open

---

### RISK-PERF-005: SQLite Single-Writer Bottleneck

- **Title**: SQLite single-writer limits concurrent session throughput
- **Description**: SQLite is inherently single-writer. At ~100+ concurrent sessions writing to DB, SQLite contention dominates. WAL mode helps reads but all mutations (`insert_session`, `update_session`, `insert_message`) serialize per-transaction. The sqlx pool default size may create too many idle connections.
- **Category**: Performance
- **Probability**: 4 (likely — multi-session usage)
- **Impact**: 3 (moderate — performance degradation under load)
- **Risk Score**: 12
- **Risk Level**: High
- **Affected Component**: `rustcode-core/src/database.rs:59-66`
- **Agent Source**: Agent 07 — Scalability (2, 7), Agent 15 — Database (6)
- **Current Mitigation**: WAL mode, `busy_timeout = 5000`
- **Recommended Mitigation**: Set max SQLite pool size to 3 connections. Abstract `DatabaseService` behind a trait for future PostgreSQL swap. Use `BEGIN IMMEDIATE` for write transactions.
- **Owner**: Data Team Lead
- **Timeline**: 2–4 weeks
- **Status**: Open

---

### RISK-PERF-006: grep_search Reads Full Files Into Memory

- **Title**: grep_search reads entire files into memory instead of streaming
- **Description**: `filesystem.rs:1281` uses `std::fs::read_to_string` which reads each matching file entirely into a `String`. A single grep search matching 50 files of 5MB each would allocate 250MB simultaneously. No memory-mapped I/O or ripgrep delegation.
- **Category**: Performance
- **Probability**: 3 (possible — large repo search)
- **Impact**: 4 (major — potential OOM on large repos)
- **Risk Score**: 12
- **Risk Level**: High
- **Affected Component**: `rustcode-core/src/filesystem.rs:1281`
- **Agent Source**: Agent 06 — Performance (2.1, 1.1)
- **Current Mitigation**: None
- **Recommended Mitigation**: Delegate to ripgrep subprocess after initial file listing. Add `regex::Regex` LRU cache for compiled patterns.
- **Owner**: Performance Team Lead
- **Timeline**: 2–4 weeks
- **Status**: Open

---

### RISK-PERF-007: ReadTool Reads Full File Before Truncation

- **Title**: ReadTool reads entire file (even 500MB) before applying 50KB cap
- **Description**: `tool_impls.rs:1225-1230` — `MAX_READ_BYTES = 51200` but this cap is applied AFTER reading the full file. A 500MB log file reads 500MB into memory then discards all but 50KB. No `take(MAX_READ_BYTES)` on the file handle.
- **Category**: Performance
- **Probability**: 4 (likely — large files exist in repos)
- **Impact**: 4 (major — 500MB read for 50KB output)
- **Risk Score**: 16
- **Risk Level**: Critical
- **Affected Component**: `rustcode-core/src/tool_impls.rs:1225-1230`
- **Agent Source**: Agent 06 — Performance (9.2)
- **Current Mitigation**: 50KB truncation limit exists but after full read
- **Recommended Mitigation**: Use `tokio::io::AsyncReadExt::take(MAX_READ_BYTES)` on the file handle to cap reads at the limit.
- **Owner**: Performance Team Lead
- **Timeline**: 1–2 weeks
- **Status**: Open

---

### RISK-PERF-008: No HTTP Timeout on Provider Requests

- **Title**: Provider API calls have no timeout — hanging requests block sessions indefinitely
- **Description**: `provider.stream()` and `provider.complete()` are called with no timeout in `session_runner.rs:625-634`. The `reqwest` client has no per-request timeout set. A hanging HTTP connection to an LLM provider can block a session indefinitely. Only the bash tool has explicit timeout enforcement.
- **Category**: Performance
- **Probability**: 3 (possible — network issues)
- **Impact**: 5 (catastrophic — session hangs forever)
- **Risk Score**: 15
- **Risk Level**: Critical
- **Affected Component**: `rustcode-core/src/session_runner.rs:625-634`, All provider modules
- **Agent Source**: Agent 16 — Reliability (3.4, 7.1), Agent 06 — Performance (10.3)
- **Current Mitigation**: Bash tool has explicit timeouts (2 min default, 10 min max)
- **Recommended Mitigation**: Add mandatory timeouts to all provider calls. Default to 60s for streaming, 30s for completion. Set `reqwest::Client::builder().timeout(Duration::from_secs(120))`.
- **Owner**: Provider Team Lead
- **Timeline**: 1–2 weeks
- **Status**: Open

---

### RISK-PERF-009: N+1 Query for Messages and Parts Loading

- **Title**: Session message loading performs 1+N queries instead of JOIN
- **Description**: `database.rs:1728-1744` — `get_messages_with_parts` calls `list_messages(session_id)` then for each message calls `list_parts(msg.id)`. A session with 200 messages + 400 parts would require 201 SQL queries instead of 1 JOIN.
- **Category**: Performance
- **Probability**: 5 (almost certain — every session load)
- **Impact**: 3 (moderate — 201 queries vs 1)
- **Risk Score**: 15
- **Risk Level**: High
- **Affected Component**: `rustcode-core/src/database.rs:1728-1744`
- **Agent Source**: Agent 15 — Database (2), Agent 06 — Performance (8.1)
- **Current Mitigation**: None
- **Recommended Mitigation**: Replace with a `LEFT JOIN` query that fetches all messages and their parts in one round trip.
- **Owner**: Data Team Lead
- **Timeline**: 1–2 weeks
- **Status**: Open

---

### RISK-PERF-010: Synchronous Git Operations on Async Runtime

- **Title**: git_in_dir uses blocking std::process::Command in async context
- **Description**: `worktree.rs:421-433` calls `std::process::Command::output()` which blocks the calling thread until the git process exits. Git operations (clone, fetch, reset) can take seconds, blocking the async runtime thread pool.
- **Category**: Performance
- **Probability**: 4 (likely — git operations in normal workflow)
- **Impact**: 3 (moderate — blocks async runtime for seconds)
- **Risk Score**: 12
- **Risk Level**: High
- **Affected Component**: `rustcode-core/src/worktree.rs:421-433`, `rustcode-core/src/snapshot.rs:825`
- **Agent Source**: Agent 06 — Performance (7.3.2), Agent 16 — Reliability (1.4)
- **Current Mitigation**: None
- **Recommended Mitigation**: Use `tokio::process::Command` for all git operations. Add configurable timeouts.
- **Owner**: Performance Team Lead
- **Timeline**: 2–4 weeks
- **Status**: Open

---

### RISK-PERF-011: Regex Compiled on Every Grep — No Caching

- **Title**: Regex::Regex compiled on every grep call with no LRU cache
- **Description**: `filesystem.rs:1208` compiles `regex::Regex::new(&input.pattern)` on every `grep_search()` call. No caching of compiled patterns. Adds ~2-50µs overhead per search call. Also reads entire files into memory instead of using ripgrep.
- **Category**: Performance
- **Probability**: 5 (almost certain — every grep call)
- **Impact**: 2 (minor — microsecond overhead per call)
- **Risk Score**: 10
- **Risk Level**: High
- **Affected Component**: `rustcode-core/src/filesystem.rs:1208`
- **Agent Source**: Agent 06 — Performance (1.1)
- **Current Mitigation**: None
- **Recommended Mitigation**: Add `regex::Regex` LRU cache (e.g., `lru` crate) keyed by pattern string. For large files, delegate to ripgrep subprocess.
- **Owner**: Performance Team Lead
- **Timeline**: 2–4 weeks
- **Status**: Open

---

### RISK-PERF-012: Tree-Sitter Parsing Every Bash Command

- **Title**: Tree-sitter-bash AST parsing on every bash command imposes disproportionate overhead
- **Description**: `tool_impls.rs:633-634` — every bash tool invocation pays ~500µs-5ms tree-sitter parsing cost regardless of command complexity. For simple commands like `ls -la`, this is disproportionate overhead. Tree-sitter is optimized for editor incremental parsing, not one-shot parsing.
- **Category**: Performance
- **Probability**: 5 (almost certain — every bash command)
- **Impact**: 2 (minor — 500µs-5ms per command)
- **Risk Score**: 10
- **Risk Level**: Medium
- **Affected Component**: `rustcode-core/src/tool_impls.rs:633-634`
- **Agent Source**: Agent 06 — Performance (1.3)
- **Current Mitigation**: None
- **Recommended Mitigation**: Use a fast regex pre-check for known-dangerous patterns first. Only invoke tree-sitter for commands that pass the regex filter.
- **Owner**: Tool Team Lead
- **Timeline**: 2–4 weeks
- **Status**: Open

---

## RELIABILITY RISKS

---

### RISK-REL-001: No Provider Retry Mechanism

- **Title**: LLM provider retry is non-functional — is_retryable() is dead code
- **Description**: `error.rs:456` defines `LlmErrorReason::is_retryable()` which correctly identifies rate limits and provider internal errors as retryable. However, this method is **never called**. The `run_loop` and `run_turn_attempt` methods never implement automatic retry. Transient provider errors (rate limits, 503s) immediately fail the turn instead of being retried.
- **Category**: Reliability
- **Probability**: 5 (almost certain — transient provider errors are common)
- **Impact**: 5 (catastrophic — every transient error fails the turn)
- **Risk Score**: 25
- **Risk Level**: Critical
- **Affected Component**: `rustcode-core/src/error.rs:456`, `rustcode-core/src/session_runner.rs:960-1155`
- **Agent Source**: Agent 16 — Reliability (3.1, 8.1)
- **Current Mitigation**: `is_retryable()` method defined but never called
- **Recommended Mitigation**: Wire `is_retryable()` into the turn execution flow. Add exponential backoff with jitter for retryable provider errors, matching OpenCode's `retryPolicy`.
- **Owner**: Provider Team Lead
- **Timeline**: 1–2 weeks
- **Status**: Open

---

### RISK-REL-002: No Timeouts on Provider Calls

- **Title**: Provider API calls have no configurable timeout — indefinite hangs
- **Description**: `session_runner.rs:625` calls `provider.stream()` with no timeout. `provider.complete()` in compaction also has no timeout. If the provider never responds (TCP half-open, DNS hang), the session thread hangs indefinitely. Only recovery is process restart.
- **Category**: Reliability
- **Probability**: 3 (possible — network failures happen)
- **Impact**: 5 (catastrophic — session hangs forever)
- **Risk Score**: 15
- **Risk Level**: Critical
- **Affected Component**: `rustcode-core/src/session_runner.rs:625-634,990-994`
- **Agent Source**: Agent 16 — Reliability (7.1, 3.4)
- **Current Mitigation**: None
- **Recommended Mitigation**: Add timeout parameter to `Provider` trait. Default to 60s for streaming, 30s for completion. Make timeouts configurable in provider config.
- **Owner**: Provider Team Lead
- **Timeline**: 1–2 weeks
- **Status**: Open

---

### RISK-REL-003: No Signal Handling / Graceful Shutdown

- **Title**: No SIGINT/SIGTERM handlers — Ctrl+C causes immediate ungraceful termination
- **Description**: `main.rs:1233` creates a tokio runtime and calls `block_on` with no signal handling. `SIGINT` (Ctrl+C) immediately terminates the process, possibly with partial writes. No graceful shutdown sequence: no cancel of in-flight operations, no finalize hooks, no state persistence.
- **Category**: Reliability
- **Probability**: 4 (likely — users press Ctrl+C)
- **Impact**: 5 (catastrophic — data loss, incomplete writes)
- **Risk Score**: 20
- **Risk Level**: Critical
- **Affected Component**: `src/main.rs:1233-1278`
- **Agent Source**: Agent 16 — Reliability (9.1)
- **Current Mitigation**: None
- **Recommended Mitigation**: Use `tokio::signal::ctrl_c()` and `tokio::signal::unix::Signal` for SIGTERM. Implement graceful shutdown: cancel in-flight operations, run finalize hooks, persist state, close connections.
- **Owner**: CLI Team Lead
- **Timeline**: 1–2 weeks
- **Status**: Open

---

### RISK-REL-004: Sequential Tool Failure Propagation

- **Title**: Sequential tool execution with early exit on first failure — loses partial results
- **Description**: `session_runner.rs:740` processes tool calls sequentially in a `for` loop. If the third of five tool calls fails, the error is returned immediately and remaining tool calls are never executed. OpenCode uses `Promise.allSettled()` — if one tool fails, others still complete.
- **Category**: Reliability
- **Probability**: 3 (possible — tool failures happen)
- **Impact**: 4 (major — lost potentially successful work)
- **Risk Score**: 12
- **Risk Level**: High
- **Affected Component**: `rustcode-core/src/session_runner.rs:740-797`
- **Agent Source**: Agent 16 — Reliability (13.2)
- **Current Mitigation**: None
- **Recommended Mitigation**: Execute independent tool calls concurrently using `FuturesUnordered` or `join_all`. Report partial failures alongside successful results.
- **Owner**: Session Team Lead
- **Timeline**: 2–4 weeks
- **Status**: Open

---

### RISK-REL-005: File Lock TOCTOU Race

- **Title**: Flock staleness check has TOCTOU race allowing concurrent lock ownership
- **Description**: `flock.rs:222-227` — `try_acquire_lock_dir` checks `is_stale()`, creates a `.breaker` directory, then re-checks staleness. If the original lock holder renews its heartbeat between the two staleness checks, the breaker logic can incorrectly delete a live lock. Two processes could simultaneously believe they hold the same lock.
- **Category**: Reliability
- **Probability**: 3 (possible — concurrent process access)
- **Impact**: 5 (catastrophic — concurrent writes, data corruption)
- **Risk Score**: 15
- **Risk Level**: Critical
- **Affected Component**: `rustcode-core/src/flock.rs:222-227`
- **Agent Source**: Agent 16 — Reliability (1.6)
- **Current Mitigation**: Breaker protocol with heartbeat
- **Recommended Mitigation**: Use the breaker directory itself as the authoritative lock. Acquire breaker with `mkdir` atomicity; only remove original lock while holding breaker.
- **Owner**: Infrastructure Team Lead
- **Timeline**: 1–2 weeks
- **Status**: Open

---

### RISK-REL-006: No Circuit Breaker for Providers

- **Title**: No circuit breaker pattern — failing providers called on every turn
- **Description**: After N consecutive failures, the system should open the circuit and fail fast. No circuit breaker implementation exists. A failing provider (e.g., returning 429 or 503) will be called on every turn, wasting time and potentially exacerbating the provider's load.
- **Category**: Reliability
- **Probability**: 3 (possible — provider outages happen)
- **Impact**: 4 (major — retry storms, slow degradation)
- **Risk Score**: 12
- **Risk Level**: High
- **Affected Component**: `rustcode-core/src/` (no circuit breaker found)
- **Agent Source**: Agent 16 — Reliability (3.2)
- **Current Mitigation**: None
- **Recommended Mitigation**: Implement circuit breaker for provider calls. Track per-provider failure counts with configurable thresholds. Use `tokio::sync::watch` for state notification.
- **Owner**: Provider Team Lead
- **Timeline**: 1–2 months
- **Status**: Open

---

### RISK-REL-007: Error Context Silently Discarded

- **Title**: Command handlers return i32 exit codes, discarding all error context
- **Description**: `dispatch_inner` at `main.rs:1337` returns raw `i32` exit codes. The actual error is discarded. `CliErrorFormatter::format_error` is never called — the user never sees the actual error message. All runtime errors from command handlers are silently swallowed.
- **Category**: Reliability
- **Probability**: 5 (almost certain — every error path)
- **Impact**: 5 (catastrophic — users see no error messages)
- **Risk Score**: 25
- **Risk Level**: Critical
- **Affected Component**: `src/main.rs:1337-1372`, `src/cli_error.rs:50-111`
- **Agent Source**: Agent 16 — Reliability (4.1)
- **Current Mitigation**: Exit codes returned but error messages lost
- **Recommended Mitigation**: Change command handlers to return `Result<(), anyhow::Error>` and propagate errors through `dispatch` for proper formatting.
- **Owner**: CLI Team Lead
- **Timeline**: 1–2 weeks
- **Status**: Open

---

### RISK-REL-008: Control Flow via String Parsing in Errors

- **Title**: Overflow recovery encoded as string inside Error::Internal — fragile
- **Description**: `session_runner.rs:928-947` — `TurnControl` is serialized as a string inside `Error::Internal`, then parsed via substring matching (`msg.contains("(steer)")`). Any change to the encoding format silently breaks overflow recovery. `Error::Internal` could legitimately contain these strings.
- **Category**: Reliability
- **Probability**: 3 (possible — format changes or provider error messages)
- **Impact**: 4 (major — overflow recovery silently stops working)
- **Risk Score**: 12
- **Risk Level**: High
- **Affected Component**: `rustcode-core/src/session_runner.rs:928-947`
- **Agent Source**: Agent 04 — Logic Verification (H-7), Agent 16 — Reliability (1.5)
- **Current Mitigation**: None
- **Recommended Mitigation**: Use a dedicated `TurnControl` variant in `Error` enum instead of encoding in strings. Add exhaustive test coverage for all turn control paths.
- **Owner**: Session Team Lead
- **Timeline**: 1–2 weeks
- **Status**: Open

---

### RISK-REL-009: No Provider Fallback Chain

- **Title**: No provider fallback mechanism — single provider of failure
- **Description**: The `SessionRunner` is initialized with a specific `Arc<dyn Provider>` and `Model`. If this provider fails, the entire session fails. OpenCode implements fallback chains that try the next configured provider on failure.
- **Category**: Reliability
- **Probability**: 3 (possible — provider outage)
- **Impact**: 4 (major — session cannot proceed)
- **Risk Score**: 12
- **Risk Level**: High
- **Affected Component**: `rustcode-core/src/session_runner.rs`
- **Agent Source**: Agent 16 — Reliability (5.1)
- **Current Mitigation**: None
- **Recommended Mitigation**: Implement provider fallback chain. On provider error, attempt the fallback provider before failing the turn.
- **Owner**: Provider Team Lead
- **Timeline**: 1–2 months
- **Status**: Open

---

### RISK-REL-010: Non-Transactional Session Revert Cleanup

- **Title**: Session revert cleanup deletes messages without transaction — partial corruption
- **Description**: `session_revert.rs:244-250` performs individual `DELETE FROM session_message` queries without a wrapping transaction. If the process crashes mid-cleanup, some messages are deleted and others remain, leaving the session in an unrecoverable state with inconsistent message ordering and dangling references.
- **Category**: Reliability
- **Probability**: 2 (unlikely — crash during cleanup)
- **Impact**: 4 (major — session corruption)
- **Risk Score**: 8
- **Risk Level**: High
- **Affected Component**: `rustcode-core/src/session_revert.rs:244-250`
- **Agent Source**: Agent 16 — Reliability (1.7)
- **Current Mitigation**: None
- **Recommended Mitigation**: Wrap all revert cleanup operations in a SQLite transaction.
- **Owner**: Session Team Lead
- **Timeline**: 1–2 weeks
- **Status**: Open

---

### RISK-REL-011: Database Schema Drift Between SQL and Migrations

- **Title**: Raw SQL strings not validated against actual schema — runtime crashes
- **Description**: `database.rs` uses `sqlx::query` (unchecked) instead of `sqlx::query!` (compile-time checked). SQL column names and types are plain strings. Any schema migration that adds a column without updating the corresponding INSERT causes a hard panic at runtime. No compile-time query validation.
- **Category**: Reliability
- **Probability**: 3 (possible — schema changes)
- **Impact**: 5 (catastrophic — runtime crash on DB write)
- **Risk Score**: 15
- **Risk Level**: Critical
- **Affected Component**: `rustcode-core/src/database.rs:1254-1278`
- **Agent Source**: Agent 16 — Reliability (1.1), Agent 15 — Database (1)
- **Current Mitigation**: None
- **Recommended Mitigation**: Use `sqlx::query!` with compile-time checking or add integration tests that verify all hand-written SQL against the actual schema.
- **Owner**: Data Team Lead
- **Timeline**: 2–4 weeks
- **Status**: Open

---

### RISK-REL-012: Blocking Mutex in Async Snapshot Code

- **Title**: std::sync::Mutex used in async context — tokio thread starvation
- **Description**: `snapshot.rs:138` uses `std::sync::Mutex<()>` for per-operation mutual exclusion. This is a blocking mutex used inside async code. If the lock is held while awaiting a future (e.g., in `snapshot_git`), it blocks the entire tokio worker thread, starving all other async tasks on that thread.
- **Category**: Reliability
- **Probability**: 3 (possible — snapshot operations)
- **Impact**: 4 (major — thread starvation)
- **Risk Score**: 12
- **Risk Level**: High
- **Affected Component**: `rustcode-core/src/snapshot.rs:138`
- **Agent Source**: Agent 16 — Reliability (12.2)
- **Current Mitigation**: Global `StdMutex<()>` lock serializes all snapshot operations
- **Recommended Mitigation**: Replace `std::sync::Mutex` with `tokio::sync::Mutex` in all async code paths. The snapshot service lock should be async-compatible.
- **Owner**: Snapshot Team Lead
- **Timeline**: 1–2 weeks
- **Status**: Open

---

### RISK-REL-013: Session State Not Persisted Incrementally

- **Title**: No incremental persistence during tool loop — crash loses all progress
- **Description**: (Same as RISK-DL-003) `session_runner.rs:957` does not persist intermediate events or tool results during execution. Crash during long-running tool sequence loses all work.
- **Category**: Reliability
- **Probability**: 3 (possible — crashes happen)
- **Impact**: 4 (major — total progress loss on crash)
- **Risk Score**: 12
- **Risk Level**: High
- **Affected Component**: `rustcode-core/src/session_runner.rs:957-1155`
- **Agent Source**: Agent 16 — Reliability (2.2), Agent 07 — Scalability (5)
- **Current Mitigation**: None
- **Recommended Mitigation**: Persist each tool call result incrementally to the database. Use event sourcing for all session state mutations.
- **Owner**: Session Team Lead
- **Timeline**: 2–4 weeks
- **Status**: Open

---

### RISK-REL-014: Missing In-Flight Request Draining on Shutdown

- **Title**: No mechanism to interrupt all active sessions on shutdown
- **Description**: `RunCoordinator` has `interrupt()` at `session_execution.rs:786` but there's no global `shutdown()` method that interrupts all lanes. On process termination, active drains for other sessions continue running, potentially executing tool commands and writing to the database post-shutdown.
- **Category**: Reliability
- **Probability**: 3 (possible — process shutdown)
- **Impact**: 3 (moderate — post-shutdown tool execution)
- **Risk Score**: 9
- **Risk Level**: High
- **Affected Component**: `rustcode-core/src/session_execution.rs:786-821`
- **Agent Source**: Agent 16 — Reliability (9.2)
- **Current Mitigation**: None
- **Recommended Mitigation**: Add a `shutdown()` method to `RunCoordinator` that interrupts all active lanes and waits for them to settle.
- **Owner**: Session Team Lead
- **Timeline**: 2–4 weeks
- **Status**: Open

---

### RISK-REL-015: Snapshot Global Mutex Serializes All Operations

- **Title**: Single global StdMutex serializes all snapshot operations across sessions
- **Description**: `snapshot.rs:138` — a single `StdMutex<()>` lock serializes all snapshot operations (track, restore, revert, diff) across all sessions/projects. Taking a snapshot for session A blocks restoring a snapshot for session B. This is a scalability bottleneck for multi-session workflows.
- **Category**: Reliability
- **Probability**: 4 (likely — multi-session usage)
- **Impact**: 2 (minor — serialization delay)
- **Risk Score**: 8
- **Risk Level**: Medium
- **Affected Component**: `rustcode-core/src/snapshot.rs:138`
- **Agent Source**: Agent 15 — Database (9), Agent 06 — Performance (6.1)
- **Current Mitigation**: Global mutex prevents concurrent corruption
- **Recommended Mitigation**: Replace global `Mutex<()>` with per-snapshot-repo locking (keyed by `gitdir` path). Use `tokio::sync::Mutex`.
- **Owner**: Snapshot Team Lead
- **Timeline**: 2–4 weeks
- **Status**: Open

---

## ARCHITECTURE RISKS

---

### RISK-ARCH-001: Monolithic Core Crate with 95 Flat Public Modules

- **Title**: Single rustcode-core crate with 95 flat public modules — no bounded contexts
- **Description**: `rustcode-core/src/lib.rs:11-95` — all 95 modules are `pub mod` with no visibility filtering. No sub-module hierarchy. Every module is world-visible. This makes it impossible to distinguish public API from internal implementation details. The crate is a single point of failure for all business logic.
- **Category**: Architecture
- **Probability**: 5 (almost certain — structural property)
- **Impact**: 4 (major — prevents modular development)
- **Risk Score**: 20
- **Risk Level**: Critical
- **Affected Component**: `rustcode-core/src/lib.rs:11-95`
- **Agent Source**: Agent 02 — Architecture (1, 2), Agent 12 — Maintainability (9)
- **Current Mitigation**: None
- **Recommended Mitigation**: Use `pub(crate)` for internal modules. Define a clean `lib.rs` re-export surface. Split into sub-crates (rustcode-session, rustcode-provider, rustcode-config, etc.).
- **Owner**: Architecture Lead
- **Timeline**: 2–4 weeks (5 person-days for visibility discipline)
- **Status**: Open

---

### RISK-ARCH-002: Extreme Coupling Across All Modules

- **Title**: All 95 modules flat-scoped with no dependency inversion — changes ripple everywhere
- **Description**: All modules in `rustcode-core` import each other directly. No dependency injection pattern. `config.rs` changes can ripple through all 94 other modules. Testing any module in isolation requires importing the entire core crate. The provider module references `database.rs`, `config.rs`, `tool.rs`.
- **Category**: Architecture
- **Probability**: 5 (almost certain — structural property)
- **Impact**: 4 (major — brittle, hard to refactor)
- **Risk Score**: 20
- **Risk Level**: Critical
- **Affected Component**: `rustcode-core/src/` (all modules)
- **Agent Source**: Agent 02 — Architecture (3)
- **Current Mitigation**: None
- **Recommended Mitigation**: Introduce trait-based dependency inversion within `rustcode-core`. Split into multiple crates. Use constructor injection, not global state.
- **Owner**: Architecture Lead
- **Timeline**: 1–2 months
- **Status**: Open

---

### RISK-ARCH-003: No Domain Boundaries — 14 Session_* Modules Flat

- **Title**: 14 session-related modules flat in crate with no sub-module grouping
- **Description**: `session_runner.rs`, `session_prompt.rs`, `session_projector.rs`, `session_history.rs`, `session_input_inbox.rs`, `session_epoch.rs`, `session_compaction.rs`, `session_message.rs`, `session_execution.rs`, `session_model.rs`, `session_reminders.rs`, `session_revert.rs`, `session_todo.rs`, `session_info.rs` — 14 flat files with no sub-module hierarchy. Developers scan 95 flat names to understand the module structure.
- **Category**: Architecture
- **Probability**: 5 (almost certain — structural property)
- **Impact**: 3 (moderate — heavy cognitive load)
- **Risk Score**: 15
- **Risk Level**: High
- **Affected Component**: `rustcode-core/src/session_*.rs`
- **Agent Source**: Agent 02 — Architecture (4)
- **Current Mitigation**: None
- **Recommended Mitigation**: Group into `session/` sub-module with `mod.rs` and sub-files. Use `pub(crate)` within groups.
- **Owner**: Architecture Lead
- **Timeline**: 2–4 weeks
- **Status**: Open

---

### RISK-ARCH-004: 8,575-Line main.rs with Business Logic

- **Title**: Binary entry point is a monolith with mixed concerns
- **Description**: `src/main.rs` at 8,575 lines contains CLI argument parsing, inline business logic, database initialization, SSE handling, provider resolution, permission handling, and command implementations. OpenCode's entry point is ~200 lines of thin CLI dispatch. RustCode's main.rs is a thick monolith.
- **Category**: Architecture
- **Probability**: 5 (almost certain — structural property)
- **Impact**: 3 (moderate — hard to test, swap, or parallelize)
- **Risk Score**: 15
- **Risk Level**: High
- **Affected Component**: `src/main.rs` (8,575 lines)
- **Agent Source**: Agent 02 — Architecture (1), Agent 18 — Refactoring (AR-2)
- **Current Mitigation**: None
- **Recommended Mitigation**: Create `rustcode-cli` library crate. Move all `cmd_*` functions. Reduce `main.rs` to ~30 lines: parse CLI args, call `rustcode_cli::dispatch(cli)`.
- **Owner**: CLI Team Lead
- **Timeline**: 1–2 months
- **Status**: Open

---

### RISK-ARCH-005: Infrastructure Dependency in Core (DIP Violation)

- **Title**: Core code directly imports sqlx, reqwest, axum — violates Dependency Inversion
- **Description**: `rustcode-core` (inner layer) imports `sqlx`, `reqwest`, `serde_json`, `tracing` — infrastructure concerns. Core has `pub mod database` with SQLite schema definitions and queries inline. Core directly constructs HTTP clients and makes network requests. Clean Architecture dependency rule is violated: dependencies should point inward, but core depends on infrastructure.
- **Category**: Architecture
- **Probability**: 5 (almost certain — structural property)
- **Impact**: 5 (catastrophic — cannot swap infrastructure without modifying core)
- **Risk Score**: 25
- **Risk Level**: Critical
- **Affected Component**: `rustcode-core/src/` (all infrastructure imports)
- **Agent Source**: Agent 02 — Architecture (7)
- **Current Mitigation**: Provider trait is a clean port/adapter; everything else is directly coupled
- **Recommended Mitigation**: Define `Database` trait in core, implement in `rustcode-database-sqlite`. Define `HttpClient` trait in core, implement with `reqwest` in `rustcode-http`. Define `FileSystem` trait in core.
- **Owner**: Architecture Lead
- **Timeline**: 1–3 months
- **Status**: Open

---

### RISK-ARCH-006: Insufficient Modularization — 5 Crates vs 26 Packages

- **Title**: Only 5 crates vs OpenCode's 26 packages — insufficient separation
- **Description**: RustCode has 5 crates in workspace. 4 of 5 are stubs re-exporting from core. No infrastructure crates (database, http, filesystem). No event-store crate. No CLI crate. No plugin SDK crate. Build times degrade as core grows. No reuse path.
- **Category**: Architecture
- **Probability**: 5 (almost certain — structural property)
- **Impact**: 4 (major — build times, no reuse, hard to contribute)
- **Risk Score**: 20
- **Risk Level**: Critical
- **Affected Component**: `Cargo.toml` (workspace), all crate definitions
- **Agent Source**: Agent 02 — Architecture (8)
- **Current Mitigation**: Workspace structure exists with 5 crates
- **Recommended Mitigation**: Extract into more granular crates: `rustcode-core-types`, `rustcode-provider`, `rustcode-session`, `rustcode-database-sqlite`, `rustcode-http`, `rustcode-plugin-sdk`, `rustcode-cli`.
- **Owner**: Architecture Lead
- **Timeline**: 1–3 months (30 person-days)
- **Status**: Open

---

### RISK-ARCH-007: No API Firewall — Everything is pub

- **Title**: Zero API firewall — all 95 modules world-public, no internal module hiding
- **Description**: Every module in `rustcode-core` is `pub mod`. No `pub(crate)` discipline. Internal implementation details (database, flock, ripgrep) are part of the public API. Impossible to refactor internals without breaking external consumers. Breaking changes can impact downstream consumers of any module.
- **Category**: Architecture
- **Probability**: 5 (almost certain — structural property)
- **Impact**: 4 (major — prevents refactoring, leaks internals)
- **Risk Score**: 20
- **Risk Level**: Critical
- **Affected Component**: `rustcore-core/src/lib.rs:11-95`
- **Agent Source**: Agent 09 — API (1.1, 4.1), Agent 02 — Architecture (2)
- **Current Mitigation**: None
- **Recommended Mitigation**: Audit each module. Change internal modules to `pub(crate)`. Define explicit `pub use` re-exports for the public API surface.
- **Owner**: Architecture Lead
- **Timeline**: 2–4 weeks (5 person-days)
- **Status**: Open

---

### RISK-ARCH-008: No Hexagonal Architecture Outside Provider Trait

- **Title**: Port/adapter pattern only used for LLM providers — not for database, filesystem, event store
- **Description**: The `Provider` trait is a clean port/adapter pattern. But the database (`sqlx`), HTTP server (`axum`), and filesystem access are imported directly. There is no infrastructure abstraction layer. Testing requires real infrastructure (SQLite files, real filesystem, network).
- **Category**: Architecture
- **Probability**: 5 (almost certain — structural property)
- **Impact**: 3 (moderate — hard to test, swap infrastructure)
- **Risk Score**: 15
- **Risk Level**: High
- **Affected Component**: `rustcode-core/src/` (database.rs, filesystem.rs, etc.)
- **Agent Source**: Agent 02 — Architecture (6)
- **Current Mitigation**: Provider trait is clean; everything else is coupled
- **Recommended Mitigation**: Generalize the port/adapter pattern. Define traits for `Database`, `FileSystem`, `EventStore`, `SessionStore`. Move infrastructure implementations to adapter crates.
- **Owner**: Architecture Lead
- **Timeline**: 1–3 months
- **Status**: Open

---

### RISK-ARCH-009: No Database/Filesystem/Event-Store Port Abstractions

- **Title**: Missing trait abstractions for all infrastructure concerns
- **Description**: No `Database` trait — core calls `sqlx` directly. No `FileSystem` trait — core calls `std::fs` directly. No `EventStore` trait — core calls `sqlx` directly for events. Cannot swap implementations without modifying core code.
- **Category**: Architecture
- **Probability**: 5 (almost certain — structural property)
- **Impact**: 3 (moderate — infrastructure coupling)
- **Risk Score**: 15
- **Risk Level**: High
- **Affected Component**: `rustcode-core/src/` (database.rs, filesystem.rs, event.rs)
- **Agent Source**: Agent 02 — Architecture (6, 7), Agent 18 — Refactoring (MR-3, MR-4, MR-6)
- **Current Mitigation**: None
- **Recommended Mitigation**: Extract `Database`, `FileSystem`, `HttpClient` traits. Implement adapters in separate crates. Move core to depend only on traits.
- **Owner**: Architecture Lead
- **Timeline**: 1–3 months
- **Status**: Open

---

### RISK-ARCH-010: Fragmented Error Hierarchy (5 Error Types)

- **Title**: Five separate error types with no From conversions between them
- **Description**: `crate::error::Error` (34 variants), `SessionError` (12 variants), `DatabaseServiceError` (3 variants), `LspError` (10 variants), `McpError`. Downstream code must match on 5+ enums. `crate::error::Error` does NOT have `#[from] SessionError` — callers must `map_err`. Error information is lost through `SessionError::Other(String)` conversions.
- **Category**: Architecture
- **Probability**: 5 (almost certain — structural property)
- **Impact**: 5 (catastrophic — error information loss, impossible to match)
- **Risk Score**: 25
- **Risk Level**: Critical
- **Affected Component**: `rustcode-core/src/error.rs`, `session.rs`, `database.rs`, `rustcode-lsp/src/lib.rs`
- **Agent Source**: Agent 03 — Rust Expert (7.1), Agent 18 — Refactoring (QW-9)
- **Current Mitigation**: Some `#[from]` derives exist but are incomplete
- **Recommended Mitigation**: Merge all into `crate::error::Error` with variant nesting. Add `#[from]` for all sub-error types. Remove `SessionError::Other(String)`.
- **Owner**: Architecture Lead
- **Timeline**: 2–4 weeks (1 person-day for From impls)
- **Status**: Open

---

### RISK-ARCH-011: Stringly-Typed IDs — No Type Safety

- **Title**: All IDs are String type aliases — no compile-time type safety
- **Description**: `pub type SessionId = String;` — type aliases provide zero type safety. `fn foo(id: SessionId)` accepts any `String`. IDs are interchangeable with no compiler enforcement. Can pass `ModelId` where `ProviderId` is expected with no error.
- **Category**: Architecture
- **Probability**: 5 (almost certain — structural property)
- **Impact**: 3 (moderate — runtime errors instead of compile-time)
- **Risk Score**: 15
- **Risk Level**: High
- **Affected Component**: `rustcode-core/src/provider.rs:24-48`, `session.rs:83-90`
- **Agent Source**: Agent 09 — API (7.1), Agent 03 — Rust Expert (10.3)
- **Current Mitigation**: `TaggedString` exists but is unused
- **Recommended Mitigation**: Convert each ID alias to a newtype wrapper: `struct SessionId(String)`, `struct MessageId(String)`, etc. with `new()`, `as_str()`, `Serialize`/`Deserialize`.
- **Owner**: Architecture Lead
- **Timeline**: 1–2 months (5 person-days)
- **Status**: Open

---

### RISK-ARCH-012: No Semver Policy or API Versioning

- **Title**: All crates at version 0.1.0 with no versioning policy
- **Description**: All crates in `Cargo.toml` are at version `0.1.0`. No `#[deprecated]` annotations. No changelog or migration guide. No `cargo-semver-checks` in CI. Any change could be breaking. Downstream consumers cannot safely depend on any specific API surface.
- **Category**: Architecture
- **Probability**: 5 (almost certain — structural property)
- **Impact**: 3 (moderate — no compatibility guarantees)
- **Risk Score**: 15
- **Risk Level**: High
- **Affected Component**: All `Cargo.toml` files
- **Agent Source**: Agent 09 — API (3.1)
- **Current Mitigation**: None
- **Recommended Mitigation**: Establish semver policy. Add `#![warn(deprecated_safe)]`. Create API compat test suite. Use `cargo-semver-checks` in CI.
- **Owner**: Architecture Lead
- **Timeline**: 1–2 months
- **Status**: Open

---

### RISK-ARCH-013: No SDK/Client Crate for Consumers

- **Title**: No programmatic client API — consumers must embed the entire core crate
- **Description**: OpenCode publishes `@opencode-ai/sdk` with typed client, lifecycle helpers, and server launcher. RustCode has no equivalent SDK crate. Rust consumers must either use the server crate directly (embedding axum state), shell out to the binary, or write their own HTTP client.
- **Category**: Architecture
- **Probability**: 5 (almost certain — gap in API surface)
- **Impact**: 4 (major — no "nice" programmatic API)
- **Risk Score**: 20
- **Risk Level**: Critical
- **Affected Component**: No crate exists (missing)
- **Agent Source**: Agent 09 — API (14.1)
- **Current Mitigation**: None
- **Recommended Mitigation**: Create `rustcode-client` crate with typed async HTTP client for the REST API, similar to `@opencode-ai/sdk/client`.
- **Owner**: SDK Team Lead
- **Timeline**: 1–2 months
- **Status**: Open

---

### RISK-ARCH-014: Hand-Written Types Drift from OpenCode

- **Title**: All types are hand-written ports — no code generation — inevitable drift
- **Description**: OpenCode SDK types and client are auto-generated from OpenAPI spec. RustCode all types are hand-written ports from TypeScript. No code generation. No shared spec. As OpenCode evolves, RustCode types must be manually updated. Type mismatches will accumulate.
- **Category**: Architecture
- **Probability**: 5 (almost certain — manual sync will fall behind)
- **Impact**: 4 (major — type mismatches, silent incompatibility)
- **Risk Score**: 20
- **Risk Level**: Critical
- **Affected Component**: All `rustcode-core/src/*.rs` type definitions
- **Agent Source**: Agent 09 — API (14.2)
- **Current Mitigation**: Pinned to a specific commit (`5d0f866`)
- **Recommended Mitigation**: Generate Rust types from OpenAPI spec or TypeScript source. At minimum, add a schema validation test that fetches the latest OpenCode SDK types and compares them.
- **Owner**: Architecture Lead
- **Timeline**: 1–3 months
- **Status**: Open

---

### RISK-ARCH-015: Server Route Handlers Are Stubs

- **Title**: Critical server routes return placeholder data — cannot serve as real backend
- **Description**: `rustcode-server/src/routes/api.rs:116-181` — 25+ routes exist but handlers are stubs or simplified. `api_session_prompt`, `api_session_compact`, `api_session_wait` return minimal placeholder data. No authentication middleware. Session prompt execution is stub-only.
- **Category**: Architecture
- **Probability**: 5 (almost certain — every server request)
- **Impact**: 4 (major — server cannot serve as real backend)
- **Risk Score**: 20
- **Risk Level**: Critical
- **Affected Component**: `rustcode-server/src/routes/api.rs:116-181`
- **Agent Source**: Agent 09 — API (10.1)
- **Current Mitigation**: None
- **Recommended Mitigation**: Document which routes are real vs stubs. Implement session CRUD + prompt execution paths. Add auth middleware.
- **Owner**: Server Team Lead
- **Timeline**: 1–2 months
- **Status**: Open

---

### RISK-ARCH-016: Dual Event Bus System — Not Unified

- **Title**: SharedBus (in-memory) and EventV2 (DB-backed) coexist without interoperability
- **Description**: Two bus systems exist but are not unified. `SharedBus` uses `tokio::sync::broadcast` with no persistence. `EventV2` has database-backed events. Events published on `SharedBus` are not persisted; events on `EventV2` are not sent to `SharedBus` subscribers. CRUD events on `SharedBus` are lost on crash.
- **Category**: Architecture
- **Probability**: 5 (almost certain — structural property)
- **Impact**: 3 (moderate — confusion about where to publish)
- **Risk Score**: 15
- **Risk Level**: High
- **Affected Component**: `rustcode-core/src/bus.rs`, `rustcode-core/src/event.rs`
- **Agent Source**: Agent 07 — Scalability (9)
- **Current Mitigation**: Both systems exist independently
- **Recommended Mitigation**: Route all events through `EventV2` with a lightweight in-memory shortcut for ephemeral events. Remove `SharedBus` or make it a thin wrapper over `EventV2`.
- **Owner**: Event System Team Lead
- **Timeline**: 1–2 months
- **Status**: Open

---

### RISK-ARCH-017: No OpenAPI Specification for Server

- **Title**: No contract-first API documentation — no client SDK generation
- **Description**: `rustcode-server/` has no OpenAPI spec file. OpenCode has `packages/sdk/openapi.json` that drives SDK generation. RustCode has no spec, no SDK generation, no auto-generated client. Consumers must reverse-engineer the API from handler code.
- **Category**: Architecture
- **Probability**: 5 (almost certain — gap in tooling)
- **Impact**: 4 (major — no automated client generation, poor documentation)
- **Risk Score**: 20
- **Risk Level**: Critical
- **Affected Component**: `rustcode-server/src/` (no spec file)
- **Agent Source**: Agent 09 — API (10.3), Agent 13 — DevEx (8)
- **Current Mitigation**: None
- **Recommended Mitigation**: Generate OpenAPI 3.0 spec from axum routes using `utoipa` or `aide`. Generate Rust client types from OpenAPI spec.
- **Owner**: Server Team Lead
- **Timeline**: 2–4 weeks
- **Status**: Open

---

## BUSINESS RISKS

---

### RISK-BIZ-001: OpenCode Surpasses RustCode Further

- **Title**: OpenCode continues to evolve — RustCode cannot catch up by porting alone
- **Description**: OpenCode has 20k+ GitHub stars, 2M+ downloads, 25 packages, active community, and cloud infrastructure. RustCode is in scaffold phase with 5 crates. By the time RustCode reaches current parity, OpenCode will have moved further. Porting alone is a losing strategy.
- **Category**: Business
- **Probability**: 4 (likely — OpenCode is actively developed)
- **Impact**: 4 (major — RustCode becomes permanently obsolete)
- **Risk Score**: 16
- **Risk Level**: High
- **Affected Component**: Strategic positioning
- **Agent Source**: Agent 17 — Competitive Intelligence
- **Current Mitigation**: Pinned to specific commit for baseline parity
- **Recommended Mitigation**: Exploit Rust's unique advantages: proc macros, single binary, WASM sandboxing, local AI inference. Build "Rust-native AI terminal" — not a clone.
- **Owner**: Product Lead
- **Timeline**: Ongoing
- **Status**: Open

---

### RISK-BIZ-002: Community Fragmentation

- **Title**: RustCode ecosystem splits OpenCode community — dilutes contributions
- **Description**: Two competing open-source codebases for the same product fragment the community. Contributors must choose between TypeScript and Rust. Bug reports and features must be ported between both. Users are confused about which version to use.
- **Category**: Business
- **Probability**: 3 (possible — if both projects are maintained)
- **Impact**: 3 (moderate — diluted community effort)
- **Risk Score**: 9
- **Risk Level**: Medium
- **Affected Component**: Community management
- **Agent Source**: Agent 17 — Competitive Intelligence
- **Current Mitigation**: None
- **Recommended Mitigation**: Establish clear relationship between RustCode and OpenCode. Consider upstream contribution strategy. Document which version is canonical for which use case.
- **Owner**: Community Lead
- **Timeline**: 3–6 months
- **Status**: Open

---

### RISK-BIZ-003: Maintenance Burden of Dual Codebase

- **Title**: Maintaining two codebases (TS + Rust) doubles maintenance cost
- **Description**: Every feature, bug fix, and API change must be implemented twice (once in TS OpenCode, once in Rust RustCode). Without automated porting or code generation, the maintenance burden is roughly 2x for all changes. Schema drift accumulates.
- **Category**: Business
- **Probability**: 5 (almost certain — dual maintenance)
- **Impact**: 3 (moderate — 2x maintenance cost)
- **Risk Score**: 15
- **Risk Level**: High
- **Affected Component**: All modules
- **Agent Source**: Agent 09 — API (14.2), Agent 17 — Competitive Intelligence
- **Current Mitigation**: Pinned to specific commit
- **Recommended Mitigation**: Generate Rust types from TS/OpenAPI specs. Consider discontinuing one codebase or establishing clear sync cadence. Automate porting where possible.
- **Owner**: Program Lead
- **Timeline**: 3–6 months
- **Status**: Open

---

### RISK-BIZ-004: No Open-Source Community

- **Title**: No community to drive adoption, contributions, or bug reports
- **Description**: RustCode has <10 GitHub stars (internal/pinned fork). No community. No Discord. No release cadence. Without community, RustCode cannot sustain development. No feedback loop. No third-party contributions. No organic growth.
- **Category**: Business
- **Probability**: 4 (likely — no community-building effort)
- **Impact**: 4 (major — development cannot be sustained)
- **Risk Score**: 16
- **Risk Level**: High
- **Affected Component**: Community/outreach
- **Agent Source**: Agent 17 — Competitive Intelligence (10)
- **Current Mitigation**: None
- **Recommended Mitigation**: Open-source as soon as MVP works. Publish to crates.io. Set up Discord. Encourage contributions via good first issues. Build contributing guide.
- **Owner**: Community Lead
- **Timeline**: 1–3 months
- **Status**: Open

---

### RISK-BIZ-005: No Package Manager Presence

- **Title**: No cargo install, Homebrew, or other package manager distribution
- **Description**: RustCode is not on crates.io. No Homebrew formula. No Scoop manifest. The only install methods are direct GitHub download and the install script. Users cannot `cargo install rustcode` or `brew install rustcode`. Discoverability is severely limited.
- **Category**: Business
- **Probability**: 4 (likely — not published)
- **Impact**: 2 (minor — limited distribution)
- **Risk Score**: 8
- **Risk Level**: Medium
- **Affected Component**: Release/Infrastructure
- **Agent Source**: Agent 14 — Infrastructure (12, 15)
- **Current Mitigation**: Install script and GitHub Releases
- **Recommended Mitigation**: Publish to crates.io. Create Homebrew tap. Create Scoop manifest. Consider `cargo-dist` for modern release automation.
- **Owner**: Release Team Lead
- **Timeline**: 2–4 weeks
- **Status**: Open

---

### RISK-BIZ-006: No Code Signing — OS Security Warnings

- **Title**: Windows and macOS binaries are not code-signed — OS blocks installation
- **Description**: RustCode has no Windows Authenticode signing or macOS code signing/notarization. Windows users get "unknown publisher" warnings. macOS Gatekeeper blocks unsigned binaries. This creates installation friction and scares away casual users.
- **Category**: Business
- **Probability**: 4 (likely — every install on Windows/macOS)
- **Impact**: 3 (moderate — installation friction)
- **Risk Score**: 12
- **Risk Level**: High
- **Affected Component**: Release pipeline
- **Agent Source**: Agent 14 — Infrastructure (12)
- **Current Mitigation**: GPG signing for archives
- **Recommended Mitigation**: Add Azure Trusted Signing for Windows EXEs. Add Apple Developer ID signing for macOS binaries. Add `aarch64-pc-windows-msvc` target.
- **Owner**: Release Team Lead
- **Timeline**: 2–4 weeks
- **Status**: Open

---

## TECHNICAL DEBT RISKS

---

### RISK-DEBT-001: 300+ panic!() Calls in Production Paths

- **Title**: Widespread panic!() in non-test code — any hit crashes the process
- **Description**: 300+ `panic!()` calls exist in production (non-test) code across all crates. Used for enum variant extraction, JSON parsing, and error handling. Any unexpected enum variant triggers a process crash. In an AI coding agent, this means mid-session data loss and a completely broken experience.
- **Category**: Technical Debt
- **Probability**: 5 (almost certain — structurally prevalent)
- **Impact**: 5 (catastrophic — process crash, data loss)
- **Risk Score**: 25
- **Risk Level**: Critical
- **Affected Component**: All crates (100+ production panic!() calls)
- **Agent Source**: Agent 19 — Technical Debt (CRIT-1), Agent 04 — Logic Verification (I-1)
- **Current Mitigation**: None
- **Recommended Mitigation**: Replace every `panic!()` in non-test code with `return Err(...)` or proper error propagation. Use `#[cfg(test)]` to gate test-only panics.
- **Owner**: Engineering Lead
- **Timeline**: 4–6 weeks (80–120 person-hours)
- **Status**: Open

---

### RISK-DEBT-002: 500+ .unwrap() Calls in Library Code

- **Title**: Project rule "No .unwrap() in library code" systematically violated
- **Description**: CLAUDE.md Rule #3 prohibits `.unwrap()` in library code. 500+ `.unwrap()` + `.expect()` calls exist in non-test production code. Each `unwrap()` on `None`/`Err` crashes the process. Includes regex unwraps, JSON unwraps in replay, lock poisoned panics, I/O unwraps, ID generation unwraps.
- **Category**: Technical Debt
- **Probability**: 5 (almost certain — structurally prevalent)
- **Impact**: 5 (catastrophic — process crash on any unexpected state)
- **Risk Score**: 25
- **Risk Level**: Critical
- **Affected Component**: All production modules across all crates
- **Agent Source**: Agent 19 — Technical Debt (CRIT-4), Agent 04 — Logic Verification (C-3, I-1)
- **Current Mitigation**: Rule documented in CLAUDE.md but not enforced
- **Recommended Mitigation**: Enforce via `clippy::unwrap_used` lint (deny in CI). Replace all `.unwrap()` with `?`, `.context()`, or `.expect("reason")`.
- **Owner**: Engineering Lead
- **Timeline**: 4–6 weeks (40–60 person-hours)
- **Status**: Open

---

### RISK-DEBT-003: Dead Code Hidden by Crate-Wide Allow Lints

- **Title**: `#![allow(dead_code, unused_imports, unused_variables)]` masks 50+ dead items
- **Description**: Crate-wide `#![allow(dead_code, unused_imports, unused_variables)]` in `lib.rs:2` and `main.rs:2` suppresses compiler detection of dead code. 50+ dead items silently accumulate. Modules may or may not be used — the compiler cannot verify. CI passes despite broken references.
- **Category**: Technical Debt
- **Probability**: 5 (almost certain — structurally prevalent)
- **Impact**: 4 (major — dead code rots silently, masks real bugs)
- **Risk Score**: 20
- **Risk Level**: Critical
- **Affected Component**: `rustcode-core/src/lib.rs:2`, `src/main.rs:2`
- **Agent Source**: Agent 19 — Technical Debt (CRIT-2), Agent 12 — Maintainability (1)
- **Current Mitigation**: `#![allow(dead_code)]` suppresses warnings
- **Recommended Mitigation**: Remove crate-wide `allow` attributes. Add `#[expect(dead_code)]` on individual items with FIXME comments. Gate scaffold-phase items behind `#[cfg(scaffold)]`.
- **Owner**: Engineering Lead
- **Timeline**: 2–4 weeks (20–30 person-hours)
- **Status**: Open

---

### RISK-DEBT-004: NotImplemented Stubs in Production Paths

- **Title**: Core functionality returns `Error::NotImplemented` at runtime
- **Description**: `agent.rs:1293,1304` and 10+ other locations return `Err(Error::NotImplemented("mock".into()))` for methods that should be implemented. Users hit "not implemented" errors during normal operation. Core agent functionality is incomplete.
- **Category**: Technical Debt
- **Probability**: 5 (almost certain — stubs exist in production paths)
- **Impact**: 4 (major — users cannot use core features)
- **Risk Score**: 20
- **Risk Level**: Critical
- **Affected Component**: `agent.rs`, `session_runner.rs`, and 10+ other modules
- **Agent Source**: Agent 19 — Technical Debt (CRIT-3)
- **Current Mitigation**: None
- **Recommended Mitigation**: Replace each `NotImplemented` with a real implementation or a proper feature-gated `todo!()` with a tracking issue.
- **Owner**: Engineering Lead
- **Timeline**: 1–3 months (30–50 person-hours)
- **Status**: Open

---

### RISK-DEBT-005: 19-Parameter update_session Function

- **Title**: update_session has 19 positional parameters — fragile and unreadable
- **Description**: `database.rs:1284-1350` — `update_session` accepts 19 positional `Option` parameters. Every call site (10+ locations) passes 14–17 `None` values. Argument ordering bugs are invisible to the compiler. Adding a new column requires editing every call site.
- **Category**: Technical Debt
- **Probability**: 5 (almost certain — every session update)
- **Impact**: 3 (moderate — fragile, error-prone)
- **Risk Score**: 15
- **Risk Level**: High
- **Affected Component**: `rustcode-core/src/database.rs:1284-1350`
- **Agent Source**: Agent 12 — Maintainability (4), Agent 04 — Logic Verification (L-1), Agent 19 — Technical Debt (MED-5)
- **Current Mitigation**: None
- **Recommended Mitigation**: Replace with `SessionUpdate` struct with `#[derive(Default)]` and named fields. Use `..Default::default()` at call sites.
- **Owner**: Data Team Lead
- **Timeline**: 1–2 weeks (0.5 person-days)
- **Status**: Open

---

### RISK-DEBT-006: No Provider Protocol Implementations Exist

- **Title**: Provider trait defined but no real LLM provider implementations
- **Description**: The `Provider` trait is fully defined with `stream()`, `complete()`, `list_models()`, `get_model()` methods, but no real provider implementations exist. The TS source has 20+ `@ai-sdk/*` packages covering Anthropic, OpenAI, Gemini, Bedrock, Azure, etc. RustCode cannot call any LLM.
- **Category**: Technical Debt
- **Probability**: 5 (almost certain — critical missing feature)
- **Impact**: 5 (catastrophic — agent cannot call any LLM)
- **Risk Score**: 25
- **Risk Level**: Critical
- **Affected Component**: `rustcode-core/src/provider.rs`, All provider modules
- **Agent Source**: Agent 19 — Technical Debt (HIGH-2), Agent 08 — Feature Gap
- **Current Mitigation**: None
- **Recommended Mitigation**: Implement Anthropic and OpenAI providers first (cover ~80% of users). Use `reqwest` + SSE streaming.
- **Owner**: Provider Team Lead
- **Timeline**: 4–8 weeks (80–120 person-hours)
- **Status**: Open

---

### RISK-DEBT-007: Session Compaction Incomplete

- **Title**: Session compaction (context window management) missing — long sessions fail
- **Description**: `session_compaction.rs` has core types but the actual compaction logic — summarization, pruning, context window management — is missing or stubbed. `SessionManager::diff()` returns empty. Long sessions will overflow context windows, causing provider errors.
- **Category**: Technical Debt
- **Probability**: 4 (likely — long sessions are common)
- **Impact**: 4 (major — sessions break after N turns)
- **Risk Score**: 16
- **Risk Level**: High
- **Affected Component**: `rustcode-core/src/session_compaction.rs`
- **Agent Source**: Agent 19 — Technical Debt (HIGH-3)
- **Current Mitigation**: Core types exist, logic is stub
- **Recommended Mitigation**: Implement `SessionCompactionService` with tail-turns preservation, summary generation, and overflow detection.
- **Owner**: Session Team Lead
- **Timeline**: 4–8 weeks (40–60 person-hours)
- **Status**: Open

---

### RISK-DEBT-008: Config Lock Poisoned Panic

- **Title**: Config RwLock expect() panics on lock poison — crashes all threads
- **Description**: `config.rs:1166` — `self.info.read().expect("Config lock poisoned")` panics on lock poison. A panic in one thread taking the lock poisons it for all threads. Subsequent reads/writes crash the process.
- **Category**: Technical Debt
- **Probability**: 2 (unlikely — rare but catastrophic)
- **Impact**: 4 (major — process crash)
- **Risk Score**: 8
- **Risk Level**: High
- **Affected Component**: `rustcode-core/src/config.rs:1166`
- **Agent Source**: Agent 19 — Technical Debt (HIGH-4)
- **Current Mitigation**: None
- **Recommended Mitigation**: Replace `.expect()` with `lock().map_err(|_| Error::Internal(...))?` or use `Mutex` with clear ownership.
- **Owner**: Config Team Lead
- **Timeline**: 1–2 weeks (2–4 person-hours)
- **Status**: Open

---

### RISK-DEBT-009: No SQLite Pool in Production

- **Title**: SQLite pool not wired — persistence falls back to JSON file storage
- **Description**: `storage.rs` and `database.rs` define 20 CREATE TABLE statements and 35 migration IDs, but the `Database` struct uses JSON file-based storage as primary implementation. SQLite pool creation is deferred. Data is lost on restart.
- **Category**: Technical Debt
- **Probability**: 5 (almost certain — critical missing feature)
- **Impact**: 4 (major — no persistent storage)
- **Risk Score**: 20
- **Risk Level**: Critical
- **Affected Component**: `rustcode-core/src/storage.rs`, `database.rs`
- **Agent Source**: Agent 19 — Technical Debt (HIGH-5)
- **Current Mitigation**: JSON file-based storage as fallback
- **Recommended Mitigation**: Wire up `sqlx::SqlitePool` in `runtime.rs` init, run migrations on startup, switch `DatabaseService` to real pool.
- **Owner**: Data Team Lead
- **Timeline**: 4–8 weeks (30–50 person-hours)
- **Status**: Open

---

### RISK-DEBT-010: No Integration Tests — <2% Coverage

- **Title**: Less than 2% test coverage on business logic — no integration tests
- **Description**: ~54 total test functions, mostly trivial unit tests. No integration tests for database, session, event, provider, tool, plugin, filesystem, or server modules. No mocking infrastructure. Cannot test most modules because they require real infrastructure. Estimated <2% code coverage.
- **Category**: Technical Debt
- **Probability**: 5 (almost certain — structural property)
- **Impact**: 4 (major — regressions undetected)
- **Risk Score**: 20
- **Risk Level**: Critical
- **Affected Component**: All modules (lack of test infrastructure)
- **Agent Source**: Agent 10 — Testing, Agent 12 — Maintainability (12)
- **Current Mitigation**: 54 trivial unit tests in core modules
- **Recommended Mitigation**: Add mock implementations. Create integration test suite. Add `cargo-tarpaulin` to CI with minimum coverage threshold.
- **Owner**: Engineering Lead
- **Timeline**: 2–4 months (40 person-days)
- **Status**: Open

---

### RISK-DEBT-011: 14 Files Over 1,000 Lines Each

- **Title**: Monolithic files violate Single Responsibility Principle
- **Description**: 14 files are >1,000 lines each: `database.rs` (4,758), `config.rs` (4,861), `main.rs` (8,575), `plugin.rs` (6,236), `session.rs` (4,133), `event.rs` (2,905), `provider.rs` (3,018), `tool_impls.rs` (7,235), `filesystem.rs` (2,383), `permission.rs` (2,154), `filesystem.rs` (2,383), `rustcode-lsp/src/lib.rs` (3,099), `rustcode-mcp/src/lib.rs` (1,774), `TuiApp` (1,270). Merge conflicts, cognitive load, hard to navigate.
- **Category**: Technical Debt
- **Probability**: 5 (almost certain — structural property)
- **Impact**: 3 (moderate — hard to navigate, merge conflicts)
- **Risk Score**: 15
- **Risk Level**: High
- **Affected Component**: 14 files across all crates
- **Agent Source**: Agent 12 — Maintainability (5), Agent 18 — Refactoring (MR-2)
- **Current Mitigation**: None
- **Recommended Mitigation**: Split each >1,000-line file into directory-based sub-modules (aim for <300 lines each).
- **Owner**: Engineering Lead
- **Timeline**: 1–2 months (10 person-days)
- **Status**: Open

---

### RISK-DEBT-012: TuiApp God Struct with 50+ Fields

- **Title**: TuiApp struct has ~50 fields — violates Single Responsibility
- **Description**: `rustcode-tui/src/app.rs:96-200` — `TuiApp` struct with ~50 fields covering component states, app state, backend services, streaming state, toggle flags, overlay states, dialog states, LLM streaming sender, tool definitions, terminal geometry, recent models, pinned sessions, theme, plugins, audio. Knows about everything.
- **Category**: Technical Debt
- **Probability**: 5 (almost certain — structural property)
- **Impact**: 4 (major — hard to test, every feature touches TuiApp)
- **Risk Score**: 20
- **Risk Level**: Critical
- **Affected Component**: `rustcode-tui/src/app.rs:96-200`
- **Agent Source**: Agent 12 — Maintainability (4)
- **Current Mitigation**: None
- **Recommended Mitigation**: Split into focused sub-structs (`AppCore`, `StreamingState`, `UIOptions`, `PluginHost`) composed as fields.
- **Owner**: TUI Team Lead
- **Timeline**: 2–4 weeks (12 person-hours)
- **Status**: Open

---

### RISK-DEBT-013: V1/V2 Code Duplication in Session Runner

- **Title**: LLM streaming loop, tool call collection, and execution duplicated between V1 and V2
- **Description**: `session_runner.rs:957-1155` (V1 `run_loop`, ~200 lines) and `session_runner.rs:578-800` (V2 `run_turn_attempt`, ~222 lines) both iterate stream events, accumulate text deltas, collect tool calls, build tool contexts, and execute tools. ~420 lines of duplicated logic.
- **Category**: Technical Debt
- **Probability**: 5 (almost certain — structural property)
- **Impact**: 3 (moderate — bug fixes must be applied twice)
- **Risk Score**: 15
- **Risk Level**: High
- **Affected Component**: `rustcode-core/src/session_runner.rs:578-800, 957-1155`
- **Agent Source**: Agent 04 — Logic Verification (I-3)
- **Current Mitigation**: None
- **Recommended Mitigation**: Extract shared stream processing and tool execution pipeline into a shared helper function.
- **Owner**: Session Team Lead
- **Timeline**: 2–4 weeks
- **Status**: Open

---

### RISK-DEBT-014: Server Route Error Handling Boilerplate

- **Title**: 25+ route handlers duplicate identical match/error pattern
- **Description**: Every server handler repeats the same 6-10 line pattern: `match result { Ok(v) => Json(serde_json::to_value(v).unwrap_or_default()).into_response(), Err(e) => (StatusCode::INTERNAL_SERVER_ERROR, Json(json!({"error": e.to_string()}))).into_response() }`. ~200 lines of identical boilerplate. Uses `unwrap_or_default()` on serialization, silently swallowing errors.
- **Category**: Technical Debt
- **Probability**: 5 (almost certain — structural property)
- **Impact**: 2 (minor — boilerplate, silent serialization failures)
- **Risk Score**: 10
- **Risk Level**: Medium
- **Affected Component**: `rustcode-server/src/routes/session.rs`
- **Agent Source**: Agent 12 — Maintainability (4), Agent 18 — Refactoring (QW-10)
- **Current Mitigation**: None
- **Recommended Mitigation**: Extract `fn ok_or_500<T: Serialize>(result: Result<T>) -> impl IntoResponse` helper. Return 500 on serialization failure instead of silent null.
- **Owner**: Server Team Lead
- **Timeline**: 1–2 weeks (0.5 person-days)
- **Status**: Open

---

### RISK-DEBT-015: Replacer Strategy Duplication

- **Title**: 9 Replacer structs duplicate candidate-lookup and offset arithmetic
- **Description**: `tool_impls.rs:56-430` — 9 Replacer structs (`SimpleReplacer`, `LineTrimmedReplacer`, `BlockAnchorReplacer`, `WhitespaceNormalizedReplacer`, `IndentationFlexibleReplacer`, `EscapeNormalizedReplacer`, `MultiOccurrenceReplacer`, `TrimmedBoundaryReplacer`, `ContextAwareReplacer`) each implement `fn search(content, find) -> Vec<String>` with substantial algorithmic overlap in index computation and line-offset arithmetic.
- **Category**: Technical Debt
- **Probability**: 5 (almost certain — structural property)
- **Impact**: 2 (minor — ~200 lines of near-identical boilerplate)
- **Risk Score**: 10
- **Risk Level**: Medium
- **Affected Component**: `rustcode-core/src/tool_impls.rs:56-430`
- **Agent Source**: Agent 12 — Maintainability (2)
- **Current Mitigation**: None
- **Recommended Mitigation**: Extract `fn find_block_indices()` and `fn extract_span()` helpers. Consider a macro for common line-offset arithmetic pattern.
- **Owner**: Tool Team Lead
- **Timeline**: 2–4 weeks (3 person-hours)
- **Status**: Open

---

## SCALABILITY RISKS

---

### RISK-SCALE-001: No Distributed Infrastructure

- **Title**: Zero distributed infrastructure — single process, single node
- **Description**: RustCode has no distributed primitives — no service discovery, no leader election, no cross-node coordination. All state lives in one SQLite file. OpenCode runs on Cloudflare Workers (300+ locations) with Durable Objects for distributed coordination. RustCode cannot run as a multi-instance service.
- **Category**: Scalability
- **Probability**: 5 (almost certain — structural limitation)
- **Impact**: 4 (major — cannot scale beyond single node)
- **Risk Score**: 20
- **Risk Level**: Critical
- **Affected Component**: Entire architecture
- **Agent Source**: Agent 07 — Scalability (1)
- **Current Mitigation**: Single-user local-only design
- **Recommended Mitigation**: Document as single-user only. For multi-instance: adopt external KV store (Redis/FoundationDB) and implement distributed coordination layer.
- **Owner**: Architecture Lead
- **Timeline**: 3–6 months (if needed)
- **Status**: Open

---

### RISK-SCALE-002: SQLite Single-Writer Bottleneck

- **Title**: SQLite fundamentally limits write throughput — ~1K writes/sec ceiling
- **Description**: SQLite supports one writer at a time. Adding instances increases read capacity but write capacity stays at 1. At ~100+ concurrent sessions writing to DB, SQLite contention dominates. WAL helps reads but all mutations serialize per-transaction. Hard ceiling at ~50K operations/sec on modern hardware.
- **Category**: Scalability
- **Probability**: 5 (almost certain — architectural ceiling)
- **Impact**: 4 (major — hard write throughput ceiling)
- **Risk Score**: 20
- **Risk Level**: Critical
- **Affected Component**: `rustcode-core/src/database.rs:59-66`
- **Agent Source**: Agent 07 — Scalability (2, 7)
- **Current Mitigation**: WAL mode, busy_timeout
- **Recommended Mitigation**: Keep SQLite for local/dev. Abstract `DatabaseService` behind a trait for future PostgreSQL swap. Add PgBouncer for pooling.
- **Owner**: Data Team Lead
- **Timeline**: 3–6 months (if needed)
- **Status**: Open

---

### RISK-SCALE-003: No Fault Tolerance — Crash Loses All State

- **Title**: Process crash loses all in-memory state — no cross-node fault tolerance
- **Description**: Crash loses: all pending SSE connections (no reconnection), all in-flight LLM streams, event bus subscribers (in-memory broadcast channel state), session runner state (in-progress tool calls). No health check + watchdog. No SSE reconnect with event replay.
- **Category**: Scalability
- **Probability**: 3 (possible — crashes happen)
- **Impact**: 5 (catastrophic — total state loss)
- **Risk Score**: 15
- **Risk Level**: Critical
- **Affected Component**: All in-memory state (bus, session, event systems)
- **Agent Source**: Agent 07 — Scalability (4)
- **Current Mitigation**: EventV2 replay infrastructure exists but not wired
- **Recommended Mitigation**: Implement SSE reconnect with event replay (`Last-Event-ID`). Persist bus events to SQLite. Add health check + watchdog auto-restart.
- **Owner**: Infrastructure Team Lead
- **Timeline**: 2–4 months
- **Status**: Open

---

### RISK-SCALE-004: Session Recovery Not Wired — EventV2 Not Connected

- **Title**: EventV2 replay infrastructure exists but is not wired into session recovery
- **Description**: EventV2 replay infrastructure is structurally complete (`event.rs:1359-1422`) with `replay()`, `replay_all()`, `claim()` owner tracking, and idempotency checks. However, it is not wired into session recovery. There is no mechanism to resume an interrupted session turn.
- **Category**: Scalability
- **Probability**: 3 (possible — crashes happen)
- **Impact**: 4 (major — crashed sessions must fully restart)
- **Risk Score**: 12
- **Risk Level**: High
- **Affected Component**: `rustcode-core/src/event.rs:1359-1422`, `session_runner.rs`
- **Agent Source**: Agent 07 — Scalability (5)
- **Current Mitigation**: EventV2 port exists but not connected
- **Recommended Mitigation**: Wire EventV2 replay into session initialization. Implement `resume()` function. Persist tool results as events.
- **Owner**: Session Team Lead
- **Timeline**: 1–2 months
- **Status**: Open

---

### RISK-SCALE-005: Bus Backpressure Inadequate

- **Title**: broadcast::channel drops events for slow consumers — silent data loss
- **Description**: `bus.rs:214` — `tokio::sync::broadcast` channel with fixed capacity (default 1024). One slow consumer causes event loss for all consumers. `BusSubscription` handles `Lagged` errors by logging and continuing — silent data loss. No per-subscriber buffering.
- **Category**: Scalability
- **Probability**: 4 (likely — under high throughput)
- **Impact**: 3 (moderate — event loss for slow consumers)
- **Risk Score**: 12
- **Risk Level**: High
- **Affected Component**: `rustcode-core/src/bus.rs:208-258`
- **Agent Source**: Agent 07 — Scalability (6)
- **Current Mitigation**: Capacity of 1024 buffers ~1 second of events
- **Recommended Mitigation**: Replace `broadcast::channel` with per-subscriber `tokio::sync::mpsc` channel for SSE connections. Add per-client flow control using SSE stream buffering.
- **Owner**: Event System Team Lead
- **Timeline**: 2–4 weeks
- **Status**: Open

---

### RISK-SCALE-006: No Application-Level Caching

- **Title**: Zero application-level caching — every query hits SQLite
- **Description**: No in-memory session cache. Every `get_session()` hits SQLite. Every `get_messages()` re-queries. Session listing requires full table scan. No query result caching. SQLite page cache (64MB) is the only cache.
- **Category**: Scalability
- **Probability**: 5 (almost certain — structural gap)
- **Impact**: 3 (moderate — repeated queries hit DB)
- **Risk Score**: 15
- **Risk Level**: High
- **Affected Component**: `rustcode-core/src/database.rs`
- **Agent Source**: Agent 07 — Scalability (13)
- **Current Mitigation**: SQLite page cache (64MB)
- **Recommended Mitigation**: Implement in-memory session cache using `dashmap` with TTL. Cache most recent N sessions. Add `lru` cache for part deserialization.
- **Owner**: Performance Team Lead
- **Timeline**: 2–4 weeks
- **Status**: Open

---

### RISK-SCALE-007: No Per-Session Resource Limits

- **Title**: No token budgets, memory caps, or cost tracking per session
- **Description**: Step limit (25) and iteration limit (25) are the only limits. No per-session resource budgeting (memory, tokens, cost). No global resource caps. A single runaway session can exhaust system memory. No cost control for LLM API usage.
- **Category**: Scalability
- **Probability**: 3 (possible — run-away session)
- **Impact**: 4 (major — memory exhaustion, unbounded costs)
- **Risk Score**: 12
- **Risk Level**: High
- **Affected Component**: `rustcode-core/src/session_runner.rs:37-43`
- **Agent Source**: Agent 07 — Scalability (11)
- **Current Mitigation**: Step limit (25) and iteration limit (25)
- **Recommended Mitigation**: Add per-session token budget with overflow handling. Add memory monitoring. Implement per-session cost tracking with hard limits.
- **Owner**: Session Team Lead
- **Timeline**: 1–2 months
- **Status**: Open

---

### RISK-SCALE-008: No Multi-Tenant Architecture

- **Title**: Single-user only — cannot serve multiple users or organizations
- **Description**: No tenant concept. Auth middleware is basic password-only. `account`, `control_account`, `account_state`, `workspace` tables exist but are scaffold-only. No CRUD operations, no auth flow, no tenant isolation. Cannot serve multiple users.
- **Category**: Scalability
- **Probability**: 5 (almost certain — structural limitation)
- **Impact**: 4 (major — cannot serve multiple users)
- **Risk Score**: 20
- **Risk Level**: Critical
- **Affected Component**: Auth, account, workspace modules
- **Agent Source**: Agent 07 — Scalability (12)
- **Current Mitigation**: Documented as single-user only
- **Recommended Mitigation**: For multi-tenant: implement workspace isolation, auth middleware, JWT verification, wire up account tables.
- **Owner**: Infrastructure Team Lead
- **Timeline**: 3–6 months (if needed)
- **Status**: Open

---

### RISK-SCALE-009: No Connection Limits

- **Title**: SSE connections are unbounded — no max connections or per-IP limits
- **Description**: SSE connections at `sse.rs:29-58` are unbounded. No max connections, no per-IP limits. Each SSE client creates a new broadcast receiver. tokio handles connections efficiently but there's no upper bound. SQLite pool size default may be too large for WAL.
- **Category**: Scalability
- **Probability**: 3 (possible — high connection count)
- **Impact**: 2 (minor — resource exhaustion)
- **Risk Score**: 6
- **Risk Level**: Medium
- **Affected Component**: `rustcode-server/src/sse.rs:29-58`
- **Agent Source**: Agent 07 — Scalability (15)
- **Current Mitigation**: None
- **Recommended Mitigation**: Add configurable `max_sse_connections` to `ServerConfig`. Implement connection counting. Use `tokio::sync::Semaphore` to limit concurrent SSE subscribers.
- **Owner**: Server Team Lead
- **Timeline**: 2–4 weeks
- **Status**: Open

---

## Appendix: Cross-Reference to Agent Reports

| Risk ID | Primary Agent | Agent Finding ID |
|---------|---------------|------------------|
| RISK-DL-001 | Agent 04 — Logic Verification | C-1 |
| RISK-DL-002 | Agent 04 — Logic Verification | C-3 |
| RISK-DL-003 | Agent 16 — Reliability | 2.2 |
| RISK-DL-004 | Agent 16 — Reliability | 13.1 |
| RISK-DL-005 | Agent 16 — Reliability | 6.2 |
| RISK-SEC-001 | Agent 05 — Security | 1.1, 3.1, 3.6, 4.1 |
| RISK-SEC-002 | Agent 05 — Security | 12.3, 12.4 |
| RISK-SEC-003 | Agent 05 — Security | 9.1 |
| RISK-SEC-004 | Agent 04 — Logic Verification | C-2 |
| RISK-SEC-005 | Agent 05 — Security | 8.1, 8.2, 8.3 |
| RISK-SEC-006 | Agent 05 — Security | 1.3 |
| RISK-SEC-007 | Agent 20 — Production Readiness | 2 |
| RISK-SEC-008 | Agent 05 — Security | 3.3 |
| RISK-SEC-009 | Agent 07 — Scalability | 14 |
| RISK-SEC-010 | Agent 05 — Security | 3.1, 3.5 |
| RISK-SEC-011 | Agent 05 — Security | 10.1, 10.2 |
| RISK-SEC-012 | Agent 05 — Security | 7.2, 7.3 |
| RISK-SEC-013 | Agent 04 — Logic Verification | H-8 |
| RISK-PERF-001 | Agent 06 — Performance | 7.3.1 |
| RISK-PERF-002 | Agent 06 — Performance | 4.1.2 |
| RISK-PERF-003 | Agent 06 — Performance | 4.1.4 |
| RISK-PERF-004 | Agent 15 — Database | 5 |
| RISK-PERF-005 | Agent 07 — Scalability | 2, 7 |
| RISK-PERF-006 | Agent 06 — Performance | 2.1, 1.1 |
| RISK-PERF-007 | Agent 06 — Performance | 9.2 |
| RISK-PERF-008 | Agent 16 — Reliability | 3.4, 7.1 |
| RISK-PERF-009 | Agent 15 — Database | 2 |
| RISK-PERF-010 | Agent 06 — Performance | 7.3.2 |
| RISK-PERF-011 | Agent 06 — Performance | 1.1 |
| RISK-PERF-012 | Agent 06 — Performance | 1.3 |
| RISK-REL-001 | Agent 16 — Reliability | 3.1, 8.1 |
| RISK-REL-002 | Agent 16 — Reliability | 7.1, 3.4 |
| RISK-REL-003 | Agent 16 — Reliability | 9.1 |
| RISK-REL-004 | Agent 16 — Reliability | 13.2 |
| RISK-REL-005 | Agent 16 — Reliability | 1.6 |
| RISK-REL-006 | Agent 16 — Reliability | 3.2 |
| RISK-REL-007 | Agent 16 — Reliability | 4.1 |
| RISK-REL-008 | Agent 04 — Logic Verification | H-7 |
| RISK-REL-009 | Agent 16 — Reliability | 5.1 |
| RISK-REL-010 | Agent 16 — Reliability | 1.7 |
| RISK-REL-011 | Agent 16 — Reliability | 1.1 |
| RISK-REL-012 | Agent 16 — Reliability | 12.2 |
| RISK-REL-013 | Agent 16 — Reliability | 2.2 |
| RISK-REL-014 | Agent 16 — Reliability | 9.2 |
| RISK-REL-015 | Agent 15 — Database | 9 |
| RISK-ARCH-001 | Agent 02 — Architecture | 2 |
| RISK-ARCH-002 | Agent 02 — Architecture | 3 |
| RISK-ARCH-003 | Agent 02 — Architecture | 4 |
| RISK-ARCH-004 | Agent 02 — Architecture | 1 |
| RISK-ARCH-005 | Agent 02 — Architecture | 7 |
| RISK-ARCH-006 | Agent 02 — Architecture | 8 |
| RISK-ARCH-007 | Agent 09 — API | 1.1, 4.1 |
| RISK-ARCH-008 | Agent 02 — Architecture | 6 |
| RISK-ARCH-009 | Agent 02 — Architecture | 6, 7 |
| RISK-ARCH-010 | Agent 03 — Rust Expert | 7.1 |
| RISK-ARCH-011 | Agent 09 — API | 7.1 |
| RISK-ARCH-012 | Agent 09 — API | 3.1 |
| RISK-ARCH-013 | Agent 09 — API | 14.1 |
| RISK-ARCH-014 | Agent 09 — API | 14.2 |
| RISK-ARCH-015 | Agent 09 — API | 10.1 |
| RISK-ARCH-016 | Agent 07 — Scalability | 9 |
| RISK-ARCH-017 | Agent 09 — API | 10.3 |
| RISK-BIZ-001 | Agent 17 — Competitive Intelligence | 1 |
| RISK-BIZ-002 | Agent 17 — Competitive Intelligence | 10 |
| RISK-BIZ-003 | Agent 09 — API | 14.2 |
| RISK-BIZ-004 | Agent 17 — Competitive Intelligence | 10 |
| RISK-BIZ-005 | Agent 14 — Infrastructure | 12, 15 |
| RISK-BIZ-006 | Agent 14 — Infrastructure | 12 |
| RISK-DEBT-001 | Agent 19 — Technical Debt | CRIT-1 |
| RISK-DEBT-002 | Agent 19 — Technical Debt | CRIT-4 |
| RISK-DEBT-003 | Agent 19 — Technical Debt | CRIT-2 |
| RISK-DEBT-004 | Agent 19 — Technical Debt | CRIT-3 |
| RISK-DEBT-005 | Agent 12 — Maintainability | 4 |
| RISK-DEBT-006 | Agent 19 — Technical Debt | HIGH-2 |
| RISK-DEBT-007 | Agent 19 — Technical Debt | HIGH-3 |
| RISK-DEBT-008 | Agent 19 — Technical Debt | HIGH-4 |
| RISK-DEBT-009 | Agent 19 — Technical Debt | HIGH-5 |
| RISK-DEBT-010 | Agent 10 — Testing | 1 |
| RISK-DEBT-011 | Agent 12 — Maintainability | 5 |
| RISK-DEBT-012 | Agent 12 — Maintainability | 4 |
| RISK-DEBT-013 | Agent 04 — Logic Verification | I-3 |
| RISK-DEBT-014 | Agent 12 — Maintainability | 4 |
| RISK-DEBT-015 | Agent 12 — Maintainability | 2 |
| RISK-SCALE-001 | Agent 07 — Scalability | 1 |
| RISK-SCALE-002 | Agent 07 — Scalability | 2, 7 |
| RISK-SCALE-003 | Agent 07 — Scalability | 4 |
| RISK-SCALE-004 | Agent 07 — Scalability | 5 |
| RISK-SCALE-005 | Agent 07 — Scalability | 6 |
| RISK-SCALE-006 | Agent 07 — Scalability | 13 |
| RISK-SCALE-007 | Agent 07 — Scalability | 11 |
| RISK-SCALE-008 | Agent 07 — Scalability | 12 |
| RISK-SCALE-009 | Agent 07 — Scalability | 15 |

---

*Generated by Risk Register Writer Agent | 2026-06-21 | Based on 20 agent reports covering RustCode vs OpenCode analysis*
