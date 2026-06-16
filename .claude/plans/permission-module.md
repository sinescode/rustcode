# Permission Module [07] — Phase 1 Deep Read + Plan

## STEP 1 — Source Summary

Read 9 TS files across 3 packages:

| File | Lines | Purpose |
|------|-------|---------|
| `packages/opencode/src/permission/index.ts` | 231 | Main V1 permission service (ask/reply/list) + evaluate + fromConfig + merge + disabled |
| `packages/opencode/src/permission/evaluate.ts` | 2 | Re-exports evaluate |
| `packages/opencode/src/permission/arity.ts` | 163 | Bash command prefix → arity dictionary (160+ entries) |
| `packages/core/src/permission.ts` | 330 | V2 permission service (ask/assert/reply/get/forSession/list) with agent/session integration |
| `packages/core/src/permission/saved.ts` | 88 | PermissionSaved — SQLite-backed saved rules (add/list/remove) |
| `packages/core/src/permission/schema.ts` | 15 | V2 schema types: Effect, Rule, Ruleset |
| `packages/core/src/permission/sql.ts` | 20 | drizzle SQLite table definition for saved permissions |
| `packages/core/src/v1/config/permission.ts` | 50 | Config permission schema (Action | Object union, known keys) |
| `packages/core/src/v1/permission.ts` | 97 | V1 types: ID, Rule, Ruleset, Request, Reply, errors |
| `packages/core/src/util/wildcard.ts` | 14 | Regex-based wildcard matching |

## STEP 2 — Interface Contract

### 2a. Public API Surface

| TS Function/Type | Location | Rust Equivalent |
|---|---|---|
| `PermissionV1.ID` | v1/permission.ts:9 | `PermissionId::create(Option<&str>)` |
| `PermissionV1.Rule` | v1/permission.ts:18 | `PermissionRule { permission, pattern, action }` |
| `PermissionV1.Ruleset` | v1/permission.ts:25 | `pub type PermissionRuleset = Vec<PermissionRule>` |
| `PermissionV1.Request` | v1/permission.ts:28 | `PermissionRequest { id, session_id, permission, patterns, metadata, always, tool }` |
| `PermissionV1.Reply` | v1/permission.ts:42 | `PermissionReply { Once, Always, Reject }` |
| `PermissionV1.AskInput` | v1/permission.ts:57 | `AskInput { id?, session_id, permission, patterns, metadata, always, tool?, ruleset }` |
| `PermissionV1.ReplyInput` | v1/permission.ts:64 | `ReplyInput { request_id, reply, message? }` |
| `PermissionV1.RejectedError` | v1/permission.ts:70 | `PermissionError::Rejected` (already in error.rs) |
| `PermissionV1.CorrectedError` | v1/permission.ts:76 | `PermissionError::Corrected { feedback }` (already in error.rs) |
| `PermissionV1.DeniedError` | v1/permission.ts:84 | `PermissionError::Denied` (already in error.rs) |
| `PermissionV1.NotFoundError` | v1/permission.ts:92 | `PermissionError::NotFound { request_id }` (already in error.rs) |
| `PermissionSchema.Effect` | permission/schema.ts:5 | `PermissionEffect { Allow, Deny, Ask }` |
| `PermissionSchema.Rule` (V2) | permission/schema.ts:8 | `PermissionRuleV2 { action, resource, effect }` |
| `PermissionSaved.Info` | permission/saved.ts:17 | `SavedPermission { id, project_id, action, resource }` |
| `PermissionSaved.Interface` | permission/saved.ts:37 | `SavedPermissions` struct with `list`, `add`, `remove` methods |
| `ConfigPermissionV1.Info` | v1/config/permission.ts:43 | `ConfigPermission` — maps tool name → action or pattern→action |
| `BashArity.prefix(tokens)` | arity.ts:1 | `bash_arity_prefix(tokens: &[&str]) -> &[&str]` |
| `Wildcard.match(input, pattern)` | util/wildcard.ts:3 | `wildcard_match(input: &str, pattern: &str) -> bool` |
| `evaluate(permission, pattern, ...rulesets)` | index.ts:39 | `evaluate(permission, pattern, rulesets) -> PermissionRule` |
| `evaluate(action, resource, ...rulesets)` (V2) | core/permission.ts:102 | `evaluate_v2(action, resource, rulesets) -> PermissionRuleV2` |
| `fromConfig(permission)` | index.ts:197 | `rules_from_config(config: &ConfigPermission) -> PermissionRuleset` |
| `merge(...rulesets)` | index.ts:211 | `merge_rulesets(rulesets: &[PermissionRuleset]) -> PermissionRuleset` |
| `disabled(tools, ruleset)` | index.ts:215 | `disabled_tools(tools: &[String], ruleset) -> HashSet<String>` |
| `Permission.Service.ask()` | index.ts:78 (V1) + permission.ts:216 (V2) | `PermissionService::ask()` |
| `Permission.Service.reply()` | index.ts:120 (V1) + permission.ts:245 (V2) | `PermissionService::reply()` |
| `Permission.Service.list()` | index.ts:180 (V1) + permission.ts:313 (V2) | `PermissionService::list()` |
| `Permission.Service.assert()` (V2 only) | permission.ts:223 | `PermissionService::assert()` — blocks until resolved |
| `Permission.Service.get()` (V2 only) | permission.ts:317 | `PermissionService::get(id)` |
| `Permission.Service.forSession()` (V2 only) | permission.ts:321 | `PermissionService::for_session(session_id)` |

### 2b. Events Emitted (bus messages)

| Event Type | Payload | When |
|---|---|---|
| `permission.asked` | `PermissionRequest` (id, sessionID, permission, patterns, metadata, always, tool?) | When `ask()` creates a pending request (needs user input) |
| `permission.replied` | `{ sessionID, requestID, reply }` | When `reply()` resolves a pending request |
| `permission.v2.asked` | V2 Request fields | When V2 `ask()`/`assert()` creates pending |
| `permission.v2.replied` | `{ sessionID, requestID, reply }` | When V2 `reply()` resolves |

### 2c. Events Consumed

Permission does not directly subscribe to events. It only publishes. Replies come through the `reply()` method called externally (e.g., from the server or TUI).

### 2d. Dependencies

- **error** — `Error::Permission(PermissionError)` with Rejected/Corrected/Denied/NotFound variants (already exists)
- **id** — `id::ascending(IdPrefix::Permission, None)` for `per_` prefix IDs (already exists)
- **bus** — `SharedBus` for publishing asked/replied events (already exists)
- **config** — `ConfigPermissionV1.Info` schema for `fromConfig()` conversion (config.rs already has the permission field in ConfigV1)
- **storage** — `Database` for PermissionSaved SQLite table (already exists)
- **session** — SessionID type (module not yet implemented)
- **agent** — AgentID type (module not yet implemented)

### 2e. All Error Conditions

| Error | TS Source | When |
|-------|-----------|------|
| `DeniedError` | v1/permission.ts:84 | Rule evaluates to "deny" for a requested pattern |
| `RejectedError` | v1/permission.ts:70 | User rejects permission request (reply = "reject") |
| `CorrectedError` | v1/permission.ts:76 | User rejects with feedback message |
| `NotFoundError` | v1/permission.ts:92 | reply() called for non-existent request ID |
| Duplicate pending ID | permission.ts:207 (die) | Two requests with same ID → panic-equivalent |

### 2f. Performance-Sensitive Paths

- `evaluate()` is called in hot loops (once per `pattern` in `ask()`, once per pending item in `reply()`)
- Wildcard matching uses regex — compile patterns lazily / cache
- `fromConfig()` is called once at config load (not hot)

### 2g. Permission Check Flow

1. Tool calls `permission.ask()` or `permission.assert()`
2. For each `pattern` in the request:
   - Evaluate against `ruleset` (from config) + `approved` (remembered rules)
   - If any pattern → "deny" → error (or "ask" result)
   - If all patterns → "allow" → return immediately
   - Otherwise → needs user input → create pending entry
3. Publish `permission.asked` event on bus
4. External system (server/TUI) receives event, presents to user
5. User calls `permission.reply()` with `once`/`always`/`reject`
6. Service resolves the pending Deferred, publishes `permission.replied`
7. If "always": save to approved rules + optionally persist to database

### 2h. Config Dependencies

- `ConfigPermissionV1.Info` is `Record<string, Action | Record<string, Action>>`
- Known keys: read, edit, glob, grep, list, bash, task, external_directory, todowrite, question, webfetch, websearch, lsp, doom_loop, skill
- `*` key acts as catch-all default
- `fromConfig()` converts: `{"bash": "allow"}` → `[{permission: "bash", pattern: "*", action: "allow"}]`
- Object values: `{"bash": {"*.ts": "deny", "*": "allow"}}` → two rules
- `expand()` handles `~/` and `$HOME` in patterns

### 2i. Database Interactions (PermissionSaved)

**Table**: `permission` (SQLite)
```sql
CREATE TABLE permission (
    id          TEXT PRIMARY KEY,
    project_id  TEXT NOT NULL REFERENCES project(id) ON DELETE CASCADE,
    action      TEXT NOT NULL,
    resource    TEXT NOT NULL,
    time_created INTEGER NOT NULL,
    time_updated INTEGER NOT NULL,
    UNIQUE(project_id, action, resource)
);
```

**Operations:**
- `list({ project_id? })` — `SELECT * FROM permission [WHERE project_id = ?]`
- `add({ project_id, action, resources[] })` — `INSERT OR IGNORE INTO permission ...` (one row per resource)
- `remove(id)` — `DELETE FROM permission WHERE id = ?`

### 2j-2l. No external process, network, or non-trivial filesystem interactions

## STEP 3 — Rust Design

### 3a. File Layout

Single file: `crates/rustcode-core/src/permission.rs` (~800-1000 lines expected)

Logical sections:
1. `PermissionEffect`, `PermissionAction` enums
2. `PermissionRule` (V1 style: permission/pattern/action)
3. `PermissionRequest`, `PermissionReply`, `AskInput`, `ReplyInput`
4. `PermissionService` — state, ask, reply, list, assert
5. `SavedPermissions` — database-backed CRUD
6. `wildcard_match()` — regex-based matching
7. `bash_arity_prefix()` — command arity lookup
8. `rules_from_config()`, `merge_rulesets()`, `disabled_tools()`
9. Bus event type constants
10. Tests

### 3b. Key Rust Types

```rust
/// Permission effect/action.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum PermissionAction {
    Allow,
    Deny,
    Ask,
}

impl std::fmt::Display for PermissionAction {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Allow => write!(f, "allow"),
            Self::Deny => write!(f, "deny"),
            Self::Ask => write!(f, "ask"),
        }
    }
}

/// A single permission rule (V1 terminology: permission + pattern + action).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PermissionRule {
    pub permission: String,
    pub pattern: String,
    pub action: PermissionAction,
}

pub type PermissionRuleset = Vec<PermissionRule>;

/// Permission request state.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PermissionRequest {
    pub id: String,            // per_ prefix
    pub session_id: String,    // ses_ prefix
    pub permission: String,
    pub patterns: Vec<String>,
    pub metadata: serde_json::Value,
    pub always: Vec<String>,
    pub tool: Option<ToolSource>,
}

/// Reply to a permission request.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum PermissionReply {
    Once,
    Always,
    Reject,
}

/// Permission service — manages pending requests and evaluation.
pub struct PermissionService {
    bus: SharedBus,
    db: Option<Database>,  // for saved permissions
    pending: DashMap<String, PendingEntry>,
}

struct PendingEntry {
    request: PermissionRequest,
    tx: tokio::sync::oneshot::Sender<Result<(), PermissionError>>,
}

impl PermissionService {
    pub fn new(bus: SharedBus, db: Option<Database>) -> Self;
    
    /// Evaluate and potentially create a pending request.
    pub async fn ask(&self, input: AskInput) -> Result<PermissionAction, Error>;
    
    /// Block until permission is granted or denied.
    pub async fn assert(&self, input: AskInput) -> Result<(), Error>;
    
    /// Reply to a pending permission request.
    pub async fn reply(&self, input: ReplyInput) -> Result<(), Error>;
    
    /// List all pending permission requests.
    pub fn list(&self) -> Vec<PermissionRequest>;
}

/// Saved/remembered permissions (database-backed).
pub struct SavedPermissions {
    db: Database,
}

impl SavedPermissions {
    pub async fn list(&self, project_id: Option<&str>) -> Result<Vec<SavedPermission>, Error>;
    pub async fn add(&self, input: AddSavedInput) -> Result<(), Error>;
    pub async fn remove(&self, id: &str) -> Result<(), Error>;
}
```

### 3c. Required Crates

All already in workspace dependencies:
- `tokio` — async runtime, oneshot channels for pending
- `serde` / `serde_json` — serialization
- `dashmap` — concurrent pending map
- `regex` — wildcard matching (Wildcard.match uses RegExp)
- `sqlx` — saved permissions queries

### 3d. Concurrency Model

- `DashMap<String, PendingEntry>` for concurrent access to pending requests
- `tokio::sync::oneshot` per pending request for the assert() blocking pattern
- `SharedBus` (Arc<EventBus>) for publishing events
- No long-lived locks — pending map operations are O(1) DashMap access

### 3e. Error Handling

Uses existing `PermissionError` enum from error.rs:
- `PermissionError::Rejected`
- `PermissionError::Corrected { feedback }`
- `PermissionError::Denied`
- `PermissionError::NotFound { request_id }`

No new error variants needed.

### 3f. SQLite Schema (PermissionSaved migration)

```sql
CREATE TABLE IF NOT EXISTS permission (
    id TEXT PRIMARY KEY,
    project_id TEXT NOT NULL,
    action TEXT NOT NULL,
    resource TEXT NOT NULL,
    time_created INTEGER NOT NULL,
    time_updated INTEGER NOT NULL
);
CREATE UNIQUE INDEX IF NOT EXISTS permission_project_action_resource_idx
    ON permission(project_id, action, resource);
```

Migration ID: `"20260616_permission_saved"`

### 3g. Streaming Design

No streaming paths. Permission evaluation is synchronous (CPU-bound) and reply is a single async operation. Events are fire-and-forget publish.

### 3h. Permission Integration

This IS the permission system — it gates tool execution. Downstream: every tool calls `permission.ask()` or `permission.assert()` before executing.

### 3i. Testing Strategy

| Test | Type | Description |
|------|------|-------------|
| `test_wildcard_exact` | unit | Exact string match |
| `test_wildcard_star` | unit | `*` matches everything |
| `test_wildcard_prefix_suffix` | unit | `foo*bar` pattern |
| `test_wildcard_path_backslash` | unit | Backslash normalization |
| `test_wildcard_question` | unit | `?` single char |
| `test_evaluate_exact_match` | unit | Rule matches exactly |
| `test_evaluate_wildcard` | unit | `*` permission matches any tool |
| `test_evaluate_no_match_asks` | unit | No matching rule → Ask |
| `test_evaluate_last_wins` | unit | Last matching rule takes precedence |
| `test_evaluate_with_patterns` | unit | Pattern-level matching |
| `test_evaluate_v2_semantics` | unit | V2 action/resource/effect |
| `test_bash_arity_simple` | unit | `cat file.txt` → 1 token |
| `test_bash_arity_git` | unit | `git checkout main` → 2 tokens |
| `test_bash_arity_npm_run` | unit | `npm run dev` → 3 tokens |
| `test_bash_arity_unknown` | unit | Unknown command → 1 token |
| `test_from_config_string` | unit | String value → wildcard rule |
| `test_from_config_object` | unit | Object value → per-pattern rules |
| `test_from_config_expand_home` | unit | `~/` expansion |
| `test_from_config_expand_env` | unit | `$HOME` expansion |
| `test_merge_rulesets` | unit | Multiple rulesets flattened |
| `test_disabled_tools_edit` | unit | edit/write/apply_patch → "edit" |
| `test_disabled_tools_denied` | unit | Tool with deny `*` rule marked disabled |
| `test_service_ask_allow` | integration | All patterns allow → immediate return |
| `test_service_ask_deny` | integration | Any pattern deny → DeniedError |
| `test_service_ask_pending` | integration | Pattern needs ask → pending + event |
| `test_service_assert_blocks` | integration | assert() blocks until reply |
| `test_service_reply_once` | integration | reply("once") resolves pending |
| `test_service_reply_always` | integration | reply("always") saves rule |
| `test_service_reply_reject` | integration | reply("reject") → RejectedError |
| `test_service_reply_corrected` | integration | reply("reject", message) → CorrectedError |
| `test_service_reply_not_found` | integration | reply to bad ID → NotFoundError |
| `test_saved_list` | integration | List saved permissions from DB |
| `test_saved_add` | integration | Add and verify saved permission |
| `test_saved_remove` | integration | Remove saved permission |
| `test_saved_duplicate_ignored` | integration | Duplicate add → ignored |
| `test_permission_request_id_prefix` | unit | ID starts with "per_" |

## STEP 4 — Behavioral Parity Checklist

### V1 evaluate() parity
- [x] Happy path: last matching rule wins
- [x] Error: no match → default `{action: "ask"}`
- [x] Edge: empty ruleset → ask for everything
- [x] Edge: multiple rulesets → flattened, last match wins
- [x] Edge: patterns in rules vs no patterns in rules

### Wildcard matching parity
- [x] Exact match: `match("bash", "bash")` → true
- [x] `*` matches everything
- [x] `*` in middle: `match("foo/bar/baz", "foo/*/baz")` → true
- [x] `?` single char: `match("cat", "c?t")` → true
- [x] Backslash normalization: `\` → `/`
- [x] Special regex chars escaped: `.`, `+`, `^`, `$`, `{}`, `()`, `|`, `[]`, `\`
- [x] Trailing ` .*` → `( .*)?`

### BashArity parity
- [x] Empty tokens → `[]`
- [x] Single unknown token → `[token]`
- [x] Known single-token command → `[cmd]` (1 element)
- [x] `git checkout main` → `["git", "checkout"]` (2 elements)
- [x] `npm run dev` → `["npm", "run", "dev"]` (3 elements)
- [x] `docker compose up` → `["docker", "compose"]` (2 elements, docker has arity 2)

### Service (ask/reply/list) parity
- [x] Happy path: ask → pending → reply("once") → resolved
- [x] Error: ask with deny rule → DeniedError
- [x] Error: reply to non-existent ID → NotFoundError
- [x] Edge: reply("always") saves rule for future auto-approval
- [x] Edge: reply("reject") cascades to all pending for same session
- [x] Edge: publish asked/replied events on bus
- [x] Edge: cleanup on service drop (reject all pending)

### assert() parity (V2)
- [x] Happy path: allow rule → returns immediately
- [x] Error: deny rule → DeniedError
- [x] Blocking: ask rule → blocks until reply

### fromConfig parity
- [x] String value: `{"bash": "allow"}` → one rule with pattern `*`
- [x] Object value: `{"bash": {"*.ts": "deny"}}` → per-pattern rules
- [x] Expand: `~/` → homedir, `$HOME/` → homedir
- [x] Known keys get typed entries; unknown keys use Record<string, Rule>

### PermissionSaved parity
- [x] list() with projectID filter
- [x] list() without filter (all projects)
- [x] add() with multiple resources → one row per resource
- [x] add() with empty resources → no-op
- [x] remove() by ID
- [x] ON CONFLICT DO NOTHING for duplicates

## STEP 5 — Blockers & Readiness

### Dependencies (by module ID)
- [00] scaffold — ✅ DONE
- [01] error — ✅ DONE (PermissionError enum already defined)
- [02] id — ✅ DONE (IdPrefix::Permission = "per")
- [03] env — ✅ DONE (needed for $HOME expansion)
- [04] bus — ✅ DONE (SharedBus for events)
- [05] config — ✅ DONE (ConfigPermissionV1.Info schema for fromConfig)
- [06] storage — ✅ DONE (Database + Migration for PermissionSaved)
- [10] agent — ⛔ NOT STARTED (AgentID only used in V2 assert; soft dep)
- [11] session — ⛔ NOT STARTED (SessionID needed for requests; soft dep)

**Soft dependencies**: AgentID and SessionID are used as type parameters (String IDs). We can use `String` as placeholder types for now and tighten with newtype wrappers when those modules are built.

### ✅ READY TO IMPLEMENT
All hard dependencies are DONE. Soft dependencies can be handled with String-based IDs.
