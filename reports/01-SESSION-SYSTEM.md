# Session System — Gap Analysis

## Architecture

| Aspect | TS | Rust |
|--------|----|------|
| Files | 49 across 2 packages | 11 files |
| Key style | EventV2-driven, Effect.gen orchestration | Raw `GlobalEvent` bus + async fn |
| State management | Event-sourced (26 durable event types) | In-memory + DB CRUD |
| Runner | V2 `runTurnAttempt` with turn transitions | V1-style `run_loop` |

## Feature Gap Table

| Feature | TS | Rust | Severity |
|---------|----|------|----------|
| Event-sourced architecture (EventV2) | Full | **Missing** — no DB persistence | **CRITICAL** |
| SessionRunner V2 (orchestration) | Full | **Stub** — simple loop | **CRITICAL** |
| Message pipeline (event→projection) | Full | **Missing** | **CRITICAL** |
| Context Epoch system | Full (reconciliation algebra) | **Stub** — simple CRUD | **HIGH** |
| SessionInputInbox admit/promote | Full (EventV2-driven) | **Basic** — plain DB ops | **HIGH** |
| Compaction strategy (LLM-based) | Full | **Type-level only** — no LLM call | **HIGH** |
| RunCoordinator (demand coalescing) | Full (FiberSet) | **Types only** | **HIGH** |
| Revert | Full | **Missing** | **HIGH** |
| Reminders | Full | **Missing** | **HIGH** |
| Model resolution | Full (catalog-aware) | **Missing** — hardcoded names | **HIGH** |
| System context assembly | Full | **Missing** | **HIGH** |
| LLM streaming with state persistence | Full (EventV2) | Basic (direct bus events) | **HIGH** |
| Run-state management | Full | **Missing** | **HIGH** |
| Interrupt handling | Full (coordinator-level) | **Stub** — CancellationToken only | **HIGH** |
| Summary | Full (diff stats) | Type-level only | **MEDIUM** |
| Instruction system | Full | Type-level only | **MEDIUM** |
| Status tracking | Full | Type-level only | **MEDIUM** |
| Auto-title / session-naming | Present | **Missing** | **MEDIUM** |
| Doom-loop detection | Basic | Implemented | LOW |
| Overflow detection | Full | Implemented | LOW |
| Retry logic | Full (exponential backoff) | Implemented | LOW |
| Message types | 9 variants | All 8 variants present | LOW |
| Session CRUD | Full | Full | LOW |
| Todo system | Full (EventV2) | Type/trait only | MEDIUM |
| Share URL management | Present | Present | LOW |
| Agent attachment/handoff | Present | Type present | MEDIUM |

## 5 Most Critical Gaps

### 1. Entire EventV2 Architecture Missing
The Rust port has **no event-sourced architecture**. The TS codebase builds on 26+ durable event types projected through `projector.ts`. Rust uses raw `bus.publish(GlobalEvent::new(json!({...})))` with no schema, sequencing, replay, or projectors.

**Impact**: No crash recovery, replay, exact-retry reconciliation, or durable input admission pipeline.

**TS**: `core/src/session/event.ts:50-499` + `projector.ts:1-451`
**Rust**: `event.rs:874-923` — no DB write

### 2. Missing SessionRunner Orchestration
TS implements sophisticated orchestration with turn transitions, overflow recovery, context epoch management, tool fiber concurrency, and step limiting.

**Rust**: Simple `run_loop` with no context epoch integration, compaction, prompt promotion, or overflow recovery.

**TS**: `core/src/session/runner/llm.ts:86-401`
**Rust**: `session_runner.rs:177-379`

### 3. Context Epoch System Incomplete
TS implements full reconciliation algebra (`SystemContext.initialize/reconcile/replace`) with optimistic concurrency.

**Rust**: Simple CRUD wrapper with no reconciliation logic or SystemContext integration.

**TS**: `core/src/session/context-epoch.ts:1-343`
**Rust**: `session_epoch.rs` — basic only

### 4. No Event-Driven Input Inbox Lifecycle
TS implements full admit/promote lifecycle with event-driven conflict detection.

**Rust**: Direct DB operations with no event publication or lifecycle conflict detection.

**TS**: `core/src/session/input.ts:1-353`
**Rust**: `session_input_inbox.rs`

### 5. Missing LLM Model Resolution and System Context Assembly
TS resolves models through catalog, credentials, and integrations with variant support.

**Rust**: Hardcoded model names with no catalog, variant support, credential lookup, or system context assembly.

**TS**: `core/src/session/runner/model.ts:42-166` + `llm.ts:170-173`
**Rust**: `runtime.rs:default_model_for()`
