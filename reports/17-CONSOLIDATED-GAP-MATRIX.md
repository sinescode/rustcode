# Consolidated Gap Matrix — All 16 Module Areas

## Legend

| Severity | Code | Meaning |
|----------|------|---------|
| CRITICAL | 🔴 | Blocks functionality entirely |
| HIGH | 🟠 | Significant feature degradation |
| MEDIUM | 🟡 | Missing convenience/edge cases |
| LOW | 🔵 | Minor differences, workarounds exist |

---

## Complete Gap Inventory (80 gaps across 16 domains)

### 1. Session System (11 gaps)

| # | Gap | Severity | TS Reference | Rust Reference |
|---|-----|----------|-------------|----------------|
| S1 | EventV2 architecture (no DB persistence, no replay) | 🔴 CRITICAL | `core/src/session/event.ts` | `event.rs:874-923` |
| S2 | SessionRunner V2 orchestration | 🔴 CRITICAL | `runner/llm.ts:86-401` | `session_runner.rs:177-379` |
| S3 | Message pipeline (event→projection) | 🔴 CRITICAL | `projector.ts` | **Missing** |
| S4 | Context Epoch reconciliation algebra | 🟠 HIGH | `context-epoch.ts` | `session_epoch.rs` (simple CRUD) |
| S5 | SessionInputInbox admit/promote lifecycle | 🟠 HIGH | `core/src/session/input.ts` | `session_input_inbox.rs` |
| S6 | Compaction strategy (LLM-based) | 🟠 HIGH | `core/src/session/compaction.ts` | `session_compaction.rs` (types only) |
| S7 | RunCoordinator (demand coalescing) | 🟠 HIGH | `session/execution/` | `session_execution.rs` (types only) |
| S8 | Revert system | 🟠 HIGH | `opencode/src/session/revert.ts` | **Missing** |
| S9 | Reminders system | 🟠 HIGH | `opencode/src/session/reminders.ts` | **Missing** |
| S10 | Model resolution (catalog-aware) | 🟠 HIGH | `runner/model.ts` | `runtime.rs` (hardcoded) |
| S11 | System context assembly | 🟠 HIGH | `system.ts` + `runner/llm.ts` | **Missing** |

### 2. Agent System (11 gaps)

| # | Gap | Severity | TS Reference | Rust Reference |
|---|-----|----------|-------------|----------------|
| A1 | V2 AgentV2 Service (branded ID, Info, Selection) | 🔴 CRITICAL | `core/src/agent.ts:11-44,64-73` | **Missing** |
| A2 | Agent generation (LLM call with provider pipeline) | 🔴 CRITICAL | `opencode/src/agent/agent.ts:366-434` | Stub `NotImplemented` |
| A3 | V2 agentic loop (turn transitions, epochs, input delivery) | 🔴 CRITICAL | `runner/llm.ts:86-401` | V1-style `run_loop` only |
| A4 | LLM event publishing (15+ event types) | 🔴 CRITICAL | `runner/publish-llm-event.ts:1-411` | Basic `Vec<LlmEvent>` |
| A5 | V2 runner model resolution | 🔴 CRITICAL | `runner/model.ts:42-166` | **Missing** |
| A6 | Agent generation — plugin hook / OAuth | 🟠 HIGH | `agent.ts:379,384,416-431` | **Missing** |
| A7 | Context Epoch / AgentMismatch handling | 🟠 HIGH | `llm.ts:162-167,184-190` | **Missing** |
| A8 | Tool materialization with permissions | 🟠 HIGH | `llm.ts:217` | **Missing** |
| A9 | Overflow compaction recovery | 🟠 HIGH | `llm.ts:228-229,291-297` | Heuristic only |
| A10 | Tool fiber management (FiberSet) | 🟠 HIGH | `llm.ts:137-138,311-329` | Sequential only |
| A11 | Question rejection handling | 🟡 MEDIUM | `llm.ts:141-142,313-317` | **Missing** |

### 3. Tool System (13 gaps)

| # | Gap | Severity | TS Reference | Rust Reference |
|---|-----|----------|-------------|----------------|
| T1 | ShellTool with Tree-sitter AST parsing | 🔴 CRITICAL | `shell.ts:1-657` + `shell/prompt.ts` | Simple `BashTool` |
| T2 | WebSearch is placeholder stub | 🔴 CRITICAL | `websearch.ts:1-246` + `mcp-websearch.ts` | Returns hardcoded text |
| T3 | Task tool (subagent delegation) | 🔴 CRITICAL | `task.ts:1-346` | Returns placeholder text |
| T4 | Question tool (real user interaction) | 🔴 CRITICAL | `question.ts:1-86` | Event-bus only, not wired |
| T5 | LSP tool (9 operations) | 🟠 HIGH | `lsp.ts:1-113` | **Missing entirely** |
| T6 | Apply_patch add/delete/move missing | 🟠 HIGH | `apply-patch.ts:1-177` | Update only |
| T7 | Tool prompt templates (.txt files) | 🟡 MEDIUM | 14 `.txt` files | Inline `&str` (shorter) |
| T8 | JSON schema generation (normalize/inline) | 🟡 MEDIUM | `json-schema.ts:1-164` | Hand-written `json!()` macros |
| T9 | HTML→Markdown (turndown vs custom) | 🟡 MEDIUM | `webfetch.ts` | Custom ~250-line converter |
| T10 | BOM preservation in edit | 🟡 MEDIUM | `core/edit.ts` | **Missing** |
| T11 | `writeIfUnchanged` stale-content check | 🟡 MEDIUM | `core/file-mutation.ts` | Direct `fs::write()` |
| T12 | Invalid tool | 🟢 LOW | `invalid.ts` (21L) | **Missing** |
| T13 | Tool output streaming (real-time) | 🟠 HIGH | `shell.ts` streaming | Block after completion |

### 4. Provider/LLM System (13 gaps)

| # | Gap | Severity | TS Reference | Rust Reference |
|---|-----|----------|-------------|----------------|
| P1 | OpenAI Responses API protocol missing | 🔴 CRITICAL | `protocols/openai-responses.ts` (1004L) | Chat Completions only |
| P2 | Route composition system absent | 🔴 CRITICAL | `route/` (12 files, ~1500L) | ~5000L duplicated logic |
| P3 | Models from hardcoded catalogs vs remote | 🔴 CRITICAL | `models-dev.ts` | Per-provider hardcoded |
| P4 | Reasoning effort variants (37 branches) | 🔴 CRITICAL | `transform.ts:665-1043` (534L) | `variants` field never populated |
| P5 | Google Vertex / OAuth auth missing | 🟠 HIGH | `provider.ts:485-556` | Bare profile only |
| P6 | Snowflake Cortex custom fetch transform | 🟠 HIGH | `provider.ts:849-948` | **Missing** |
| P7 | Cloudflare AI Gateway provider | 🟠 HIGH | `provider.ts:754-829` | **Missing** |
| P8 | GitLab workflow model discovery | 🟠 HIGH | `provider.ts:591-715` | **Missing** |
| P9 | SSE per-chunk timeout | 🟠 HIGH | `wrapSSE()` in provider.ts | **Missing** |
| P10 | Error classification centralized | 🟡 MEDIUM | `error.ts` | 8 duplicated copies |
| P11 | Bedrock native Converse protocol | 🟡 MEDIUM | `bedrock-converse.ts` + `bedrock-event-stream.ts` | Chat bridge only |
| P12 | Tool schema sanitization | 🟡 MEDIUM | `transform.ts:1296-1377` | **Missing** |
| P13 | Provider options routing | 🟡 MEDIUM | `transform.ts:1045-1283` | **Missing** |

### 5. Config System (11 gaps)

| # | Gap | Severity | TS Reference | Rust Reference |
|---|-----|----------|-------------|----------------|
| C1 | YAML frontmatter parsing for agent/command .md files | 🔴 CRITICAL | `config/markdown.ts` + `gray-matter` | Discovery functions find files but never parse |
| C2 | Remote well-known and console config | 🟠 HIGH | `config/config.ts:355-394,477-513` | **Missing** |
| C3 | TUI config system (entirely missing) | 🔴 CRITICAL | `config/tui.ts` + `tui-cwd.ts` + `tui-host-attention.ts` + `tui-migrate.ts` (~432L) | **Missing** |
| C4 | NPM plugin dependency installation | 🟠 HIGH | `config/config.ts:437-456` | **Missing** |
| C5 | Structured validation errors & agent normalization | 🟠 HIGH | `config/parse.ts:55-71`, `v1/config/agent.ts:62-81` | Plain `serde_json::from_value` |
| C6 | V2 Config migrate | 🟠 HIGH | `core/v1/config/migrate.ts` (258L) | **Missing** |
| C7 | Entry name extraction from paths | 🟡 MEDIUM | `config/entry-name.ts` | **Missing** |
| C8 | Variable `missing: "empty"` mode | 🟡 MEDIUM | `variable.ts` | Hard-error only |
| C9 | Flag system (30+ env vars) | 🟡 MEDIUM | `core/flag/flag.ts` (78L) | ~5 handled |
| C10 | Config file priority order reversed | 🟡 MEDIUM | `paths.ts` | `config.json` first vs last |
| C11 | Home `.opencode` directory support | 🟡 MEDIUM | `paths.ts:34-38` | **Missing** |

### 6. Permission System (8 gaps)

| # | Gap | Severity | TS Reference | Rust Reference |
|---|-----|----------|-------------|----------------|
| PM1 | V2 `evaluateInput()` multi-stage evaluation | 🔴 CRITICAL | `core/src/permission.ts:181-188` | Never loads agent/DB rules |
| PM2 | `ask()` creates no pending entry | 🔴 CRITICAL | `index.ts:78-118` | Fire-and-forget, no deferred |
| PM3 | Saved permissions empty project ID | 🔴 CRITICAL | `core/src/permission.ts:277` | `project_id: String::new()` |
| PM4 | Missing `configured()` agent resolution | 🟠 HIGH | `core/src/permission.ts:163-171` | **Missing** |
| PM5 | `reply()` cascade uses stale in-memory state | 🟠 HIGH | `core/src/permission.ts:275-308` | Never re-fetches DB |
| PM6 | V2 event type names | 🟢 LOW | `permission.v2.asked/replied` | `permission.replied` (V1 style) |
| PM7 | Wildcard regex missing `s` flag | 🟢 LOW | `wildcard.ts` | No `(?s)` — `.` won't match `\n` |
| PM8 | `get()` in V1 not ported to use DB | 🟡 MEDIUM | `core/src/permission.ts:317-323` | In-memory only |

### 7. Database/Storage (8 gaps)

| # | Gap | Severity | TS Reference | Rust Reference |
|---|-----|----------|-------------|----------------|
| DB1 | Migration system (static SQL vs Effect functions) | 🔴 CRITICAL | `migration.ts:13-16` | Static `&str` split by `;` |
| DB2 | JSON storage file locking | 🔴 CRITICAL | `storage.ts:218-221` | **None** — data corruption risk |
| DB3 | JSON storage data migrations | 🟠 HIGH | `storage.ts:82-84` | **None** |
| DB4 | SessionStore context-epoch-aware loading | 🟠 HIGH | `store.ts:13-23` | Raw CRUD only |
| DB5 | Missing CRUD for 12 tables | 🟠 HIGH | Per-table `sql.ts` files | Only 6 tables have CRUD |
| DB6 | Drizzle journal import for TS→Rust migration | 🟠 HIGH | `migration.ts:54-66` | **Missing** |
| DB7 | Path validation/normalization not integrated | 🟡 MEDIUM | `path.ts:27-91` | Standalone functions, not enforced |
| DB8 | Typed JSON columns (14 → 0) | 🟡 MEDIUM | All `text({mode:"json"})` columns | All stored as plain `text` |

### 8. Server System (9 gaps)

| # | Gap | Severity | TS Reference | Rust Reference |
|---|-----|----------|-------------|----------------|
| SR1 | Workspace Routing middleware | 🔴 CRITICAL | `workspace-routing.ts:237` (250L) | **NOT IMPLEMENTED** |
| SR2 | Fence (sync barrier) middleware | 🔴 CRITICAL | `fence.ts:9` | **MISSING** |
| SR3 | Instance Context middleware | 🔴 CRITICAL | `instance-context.ts:37` | **NOT IMPLEMENTED** |
| SR4 | Proxy middleware (WS + HTTP) | 🔴 CRITICAL | `proxy.ts:14` (108L) | **NOT IMPLEMENTED** |
| SR5 | Schema validation / error formatting (UUID refs) | 🟠 HIGH | `schema-error.ts:25`, `error.ts:28` | Generic 500s |
| SR6 | CORS Vary:Origin merging | 🟢 LOW | `cors-vary.ts` | **Missing** |
| SR7 | Compression SSE bypass | 🟡 MEDIUM | `compression.ts` | Default tower-http |
| SR8 | Lifecycle/dispose middleware | 🟡 MEDIUM | `lifecycle.ts:43` | **MISSING** |
| SR9 | mDNS / Bonjour | 🟢 LOW | `mdns.ts` | **Missing** |

### 9. TUI System (11 gaps)

| # | Gap | Severity | TS Reference | Rust Reference |
|---|-----|----------|-------------|----------------|
| TU1 | Plugin system (slots, routes, API) | 🔴 CRITICAL | `tui/src/plugin/` + `plugin/src/tui.ts` (~2200L) | **None** |
| TU2 | Autocomplete / Frecency / Slash commands | 🔴 CRITICAL | `component/prompt/` (7 modules, ~1200L) | Basic up/down history |
| TU3 | Theme system (35→8 themes, 50→9 properties) | 🟠 HIGH | `theme/` (35 themes, ~50 props each) | 8 themes, 9 props |
| TU4 | Command palette / Which-key / Help overlay | 🟠 HIGH | `command-palette.tsx`, `which-key.tsx` | Static/stub only |
| TU5 | Monolithic vs composable architecture | 🟠 HIGH | 20+ context providers | Single 3726L `TuiApp` |
| TU6 | Home screen (session picker, tips, footer) | 🟠 HIGH | `routes/home.tsx` + 5 files | **Not implemented** |
| TU7 | Missing 6 dialog types | 🟡 MEDIUM | Alert, Confirm, Prompt, Select, etc. | 10 of 16 types |
| TU8 | Keybinding system (230→50 bindings) | 🟡 MEDIUM | `@opentui/keymap` | Flat match statements |
| TU9 | Missing diff viewer features | 🟡 MEDIUM | Hunk navigation, source switching | Basic unified/split |
| TU10 | Audio/Attention system | 🟡 MEDIUM | `audio.ts`, `attention.ts` | Stub bell only |
| TU11 | Startup loading screen | 🟢 LOW | `startup-loading.tsx` | **Not implemented** |

### 10. LSP System (7 gaps)

| # | Gap | Severity | TS Reference | Rust Reference |
|---|-----|----------|-------------|----------------|
| L1 | Pull diagnostics (document + workspace) | 🔴 CRITICAL | `client.ts:293-444` | **Entirely absent** |
| L2 | Wait-for-diagnostics with debounce/retry | 🔴 CRITICAL | `client.ts:464-541` | Returns immediately |
| L3 | Dynamic root detection / server auto-install | 🟠 HIGH | `server.ts:1-1983` | Static catalog, no install |
| L4 | Spawn dedup + broken server tracking | 🟠 HIGH | `lsp.ts:117-118` | Race on concurrent spawns |
| L5 | User configuration override for LSP | 🟡 MEDIUM | `lsp.ts:162-184` | Static catalog |
| L6 | `textDocument/didClose` notification | 🟢 LOW | `lsp.ts` | **Missing** |
| L7 | Process ID in initialize request | 🟢 LOW | `processId: process.pid` | `"processId": null` |

### 11. MCP System (7 gaps)

| # | Gap | Severity | TS Reference | Rust Reference |
|---|-----|----------|-------------|----------------|
| M1 | OAuth flow (provider + callback server) | 🔴 CRITICAL | `oauth-provider.ts` (206L) + `oauth-callback.ts` (233L) | **Missing** |
| M2 | Tool execution is placeholder stub | 🔴 CRITICAL | `catalog.ts:42-82` | Static text placeholder |
| M3 | CLI commands (list/add/auth/logout/debug) | 🔴 CRITICAL | `cli/cmd/mcp.ts` (849L) | **All missing** |
| M4 | Tolerant schema fallback for `tools/list` | 🟠 HIGH | `catalog.ts:14-16,128-151` | No error recovery |
| M5 | Tool progress / timeout reset | 🟠 HIGH | `catalog.ts:61-65` | No `onprogress` callback |
| M6 | Working directory (cwd) for stdio transport | 🟡 MEDIUM | `index.ts:333` | cwd field exists but ignored |
| M7 | Child process tree cleanup | 🟡 MEDIUM | `index.ts:400-422` | Simple `child.kill()` only |

### 12. Plugin System (9 gaps)

| # | Gap | Severity | TS Reference | Rust Reference |
|---|-----|----------|-------------|----------------|
| PL1 | V2 Effect-based plugin system | 🔴 CRITICAL | `core/src/plugin.ts` (130L) | **Entirely absent** |
| PL2 | 24/33 provider plugin configs missing | 🔴 CRITICAL | `core/src/plugin/provider/` | Only 9, all shallow stubs |
| PL3 | TUI plugin system (entirely) | 🔴 CRITICAL | `tui/src/plugin/` + `plugin/src/tui.ts` (~2200L) | **None** |
| PL4 | Auth plugin system (7+ plugins) | 🔴 CRITICAL | `opencode/src/plugin/` (7 auth plugins) | **None** |
| PL5 | Boot phase orchestrator | 🔴 CRITICAL | `core/src/plugin/boot.ts` (135L) | Only registers 9 providers |
| PL6 | Config plugins (5 from `.opencode/`) | 🟠 HIGH | `core/src/config/plugin/` | **Missing** |
| PL7 | V1 plugin detection (legacy compatibility) | 🟡 MEDIUM | `shared.ts` | **Missing** |
| PL8 | Plugin theme support | 🟡 MEDIUM | `meta.ts` + `runtime.tsx` | **Missing** |
| PL9 | 17 of 20 V1 plugin hooks | 🟠 HIGH | `plugin/src/index.ts` | Only 3 hooks (`ProviderPlugin` trait) |

### 13. Event/Bus/Git/Support (12 gaps)

| # | Gap | Severity | TS Reference | Rust Reference |
|---|-----|----------|-------------|----------------|
| E1 | Event publish has no DB persistence | 🔴 CRITICAL | `event.ts:396-407` | In-memory only |
| E2 | `aggregateEvents()` has no historical replay | 🟠 HIGH | `event.ts:606-628` | Live subscription only |
| E3 | `replay()` skips idempotency checks | 🟠 HIGH | `event.ts:453-482` | No DB validation |
| E4 | `claim()`/`remove()` are no-ops | 🟠 HIGH | `event.ts:518-536` | In-memory only |
| E5 | Image normalization pipeline absent | 🔴 CRITICAL | `image/image.ts:63-164` | MIME detection only |
| E6 | Share module is skeleton (no HTTP, no queue, no DB) | 🔴 CRITICAL | `share-next.ts` + `session.ts` | Traits only, no impl |
| E7 | Snapshot no `cat-file --batch` optimization | 🟠 HIGH | `snapshot/index.ts:603-678` | O(n) `git show` per file |
| E8 | Worktree no EventBus/EventV2 integration | 🟠 HIGH | `worktree/index.ts:257-292` | Silent creation |
| E9 | Worktree no DB/project integration | 🟠 HIGH | `worktree/index.ts:500-512` | Static path only |
| E10 | Question no event publishing | 🟠 HIGH | `question.ts:140-196` | No events on ask/reply |
| E11 | Snapshot no background GC loop | 🟡 MEDIUM | `snapshot/index.ts:760-765` | Manual only |
| E12 | Question no lifecycle cleanup | 🟡 MEDIUM | `question.ts:128-138` | Pending on shutdown |

### 14. CLI/Commands (9 gaps)

| # | Gap | Severity | TS Reference | Rust Reference |
|---|-----|----------|-------------|----------------|
| CL1 | Interactive run mode (full split-footer TUI) | 🟠 HIGH | `cmd/run.ts:811-833` | Basic REPL loop |
| CL2 | Interactive prompts (select/text/password) | 🔴 CRITICAL | `@clack/prompts` across all commands | **None** |
| CL3 | Error formatting (12 typed formatters) | 🟠 HIGH | `cli/error.ts:35-126` | Plain `eprintln!()` |
| CL4 | Plugin install (npm + manifest + config patch) | 🟠 HIGH | `cmd/plug.ts:70-176` | Prints instructions |
| CL5 | Agent creation (LLM generation) | 🟠 HIGH | `cmd/agent.ts:61-231` | Prints instructions |
| CL6 | `completion` subcommand | 🟢 LOW | `index.ts:80` | **Missing** |
| CL7 | Auto-upgrade check | 🟡 MEDIUM | `cli/upgrade.ts:8-53` | **Missing** |
| CL8 | ANSI logo display | 🟢 LOW | `cli/logo.ts` | Box-drawing only |
| CL9 | Heap/memory monitoring | 🟢 LOW | `cli/heap.ts` | **Missing** |

### 15. Auth/Credentials/Identity (10 gaps)

| # | Gap | Severity | TS Reference | Rust Reference |
|---|-----|----------|-------------|----------------|
| AC1 | Credential CRUD service layer | 🔴 CRITICAL | `credential.ts:44-150` | Types only, no queries |
| AC2 | OAuth device flow incomplete (paths, format, persistence) | 🔴 CRITICAL | `account.ts:378-438` | Wrong URLs, no persist |
| AC3 | Token refresh dedup with eager threshold | 🟠 HIGH | `account.ts:248-265` | No caching, after-expiry |
| AC4 | Installation/upgrade service | 🔴 CRITICAL | `installation/index.ts` (350L) | Types only |
| AC5 | Account service missing features | 🟠 HIGH | `account.ts:329-373` | No orgsByAccount, no config |
| AC6 | Method detection for installation | 🟡 MEDIUM | `installation/index.ts:186-219` | **Missing** |
| AC7 | `latest()` version check (6 sources) | 🟡 MEDIUM | `installation/index.ts:220-276` | **Missing** |
| AC8 | `upgrade()` (8 strategies) | 🟡 MEDIUM | `installation/index.ts:277-333` | **Missing** |
| AC9 | Brand type safety | 🟢 LOW | Effect/Schema brands | Type aliases |
| AC10 | Directory creation on startup | 🟢 LOW | `global.ts:35-43` | **Missing** |

### 16. Filesystem/Process/PTY/Util (11 gaps)

| # | Gap | Severity | TS Reference | Rust Reference |
|---|-----|----------|-------------|----------------|
| F1 | PTY runtime layer (spawn, session map, events) | 🔴 CRITICAL | `pty.ts:122-343` | Types/trait only |
| F2 | PTY spawn backend | 🔴 CRITICAL | `pty.bun.ts` + `pty.node.ts` | **None** |
| F3 | Filesystem watcher | 🔴 CRITICAL | `watcher.ts:32-142` | Types only |
| F4 | FFF search engine | 🟠 HIGH | `search.ts:126-233` | Custom `walk_for_entries` |
| F5 | File locking system | 🟠 HIGH | `flock.ts:358L` + `effect-flock.ts:285L` | **Absent** |
| F6 | ~95+ utility functions | 🟠 HIGH | `core/src/util/` (18 files) + `opencode/src/util/` (23 files) | Mostly missing |
| F7 | MIME type map (~27 vs 1000+) | 🟡 MEDIUM | `mime-types` npm | Built-in map |
| F8 | File system utils (writeWithDirs, findUp, etc.) | 🟡 MEDIUM | `fs-util.ts` | Missing async methods |
| F9 | 4 ignored folders missing | 🟢 LOW | `ignore.ts` (32 entries) | 28 entries |
| F10 | Ripgrep binary download simplified | 🟢 LOW | `binary.ts` | Hardcoded URL |
| F11 | Immer draft pattern (state management) | 🟡 MEDIUM | `MakeEditor` | Missing abstraction |

---

## Summary Statistics

| Severity | Count |
|----------|-------|
| 🔴 CRITICAL | **31** |
| 🟠 HIGH | **29** |
| 🟡 MEDIUM | **15** |
| 🔵 LOW | **9** |
| **Total** | **84** |

## Most Cross-Cutting Critical Gaps

These gaps affect multiple systems simultaneously:

1. **EventV2 Architecture** — affects Session, Agent, Tool, and all event-driven features
2. **V2 Session Runner** — affects Session, Agent, and all runtime orchestration
3. **Provider/LLM Route Composition** — affects all LLM interactions, plugin system
4. **TUI Plugin System** — affects all TUI features, extensibility
5. **Config/YAML Frontmatter** — affects Config, Agent, Command, Skill discovery
6. **Database Migration/Storage** — affects data durability, cross-session features
7. **Auth OAuth Flows** — affects all provider authentication, account management
8. **PTY Runtime** — affects shell tool, server PTY routes
