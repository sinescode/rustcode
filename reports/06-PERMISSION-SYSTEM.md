# Permission System — Gap Analysis

## Architecture

| Aspect | TS | Rust |
|--------|----|------|
| Files | 7 (V1+V2 types, service, schema, saved, SQL, arity, config) | 1 monolithic file (2154L) |
| Evaluation | V2 `evaluateInput()` multi-stage | Simple `evaluate()` + `ask()` |
| Storage | DB-backed `SavedPermissions` via Drizzle | In-memory `approved` list + DB CRUD |
| Agent integration | Agent permissions resolved per-session | No agent/session dependency |

## Feature Gap Table

| Feature | TS | Rust | Severity |
|---------|----|------|----------|
| PermissionRule format | Full | Full | ✅ PARITY |
| V1 `evaluate()` | Full | Full | ✅ PARITY |
| Wildcard matching | Full | Full (no `s` flag in regex) | ⚠️ Minor |
| Arity checking | 137-entry dictionary | Same 137 entries | ✅ PARITY |
| Config rule conversion | Full | Full | ✅ PARITY |
| **V2 `evaluateInput()` multi-stage** | Full (agent+DB+deny-first) | **Missing** — no DB load | **CRITICAL** |
| **`ask()` creates pending entry** | Full | **Does not** — fire-and-forget | **CRITICAL** |
| **Saved permissions project association** | Full (`location.project.id`) | **Empty `project_id: ""`** | **CRITICAL** |
| **`configured()` agent resolution** | Full | **Missing** | **HIGH** |
| **`reply()` cascade uses DB re-fetch** | Full | **Stale in-memory only** | **HIGH** |
| V2 event types | `permission.v2.asked/replied` | `permission.replied` (V1 style) | MEDIUM |
| `get()` / `for_session()` | Full | Full | ✅ PARITY |
| `reply()` | Full | Full | ✅ PARITY |
| `assert()` | Full | Full (double eval bug) | LOW |
| `PermissionV2.denied()` | Full | **Missing** | MEDIUM |
| `PermissionV2.relevant()` | Full | **Missing** | MEDIUM |
| Test coverage | 0 test files | 40+ test functions | Rust BETTER |

## 5 Most Critical Gaps

### 1. `evaluateInput()` — Multi-stage V2 evaluation not ported
Rust `ask()`/`assert()` never load agent-configured permissions or saved DB permissions. Only uses caller-supplied ruleset + in-memory `approved` list.

**TS**: `core/src/permission.rs:181-188`
**Rust**: `permission.rs:968`

### 2. `ask()` creates no pending entry
Rust `ask()` evaluates and returns `Ask` but never inserts into pending. No oneshot channel, no deferred, no way for `reply()` to resolve the request.

**TS**: `opencode/src/permission/index.ts:78-118`
**Rust**: `permission.rs:968-1018`

### 3. Saved permissions have no project association
`reply()` passes `project_id: String::new()`. TS uses `location.project.id`. All saved permissions stored under empty project ID.

**Rust**: `permission.rs:1150-1159`

### 4. Missing `configured()` — agent/session permission resolution
No `SessionStore.get()`, no `AgentService.resolve()`, no `missingAgentPermissions` fallback.

**TS**: `core/src/permission.ts:163-171`

### 5. V2 `reply()` cascade uses stale in-memory state
Rust cascade never consults database or agent permissions. `cascade_always()` can race with concurrent DashMap operations.

**TS**: `core/src/permission.ts:275-308`
**Rust**: `permission.rs:1138-1165`
