# Scalability Analysis: BlazeCode vs BlazeCode

**Agent 07 - Scalability Agent** | Date: 2026-06-21

---

## 1. Distributed Readiness

- **Location**: `blazecode-crates/blazecode-core/src/event.rs:775-799`, `blazecode/infra/app.ts:13-50`, `blazecode/packages/function/src/api.ts:15-114`
- **BlazeCode**: Cloud-native from day one. API runs as Cloudflare Workers (edge-deployed, 300+ locations), Durable Objects (`SyncServer`) provide distributed coordination with WebSocket-based sync, PlanetScale (MySQL) for multi-region database, R2 for blob storage, per-stage database branching.
- **BlazeCode**: Single-process, single-node. No distributed primitives — no service discovery, no leader election, no cross-node coordination. All state lives in one SQLite file.
- **Gap**: **Critical gap.** BlazeCode has zero distributed infrastructure. BlazeCode's `SyncServer` Durable Object (`packages/function/src/api.ts:15`) handles cross-node session sync via Cloudflare's global network. BlazeCode would need a complete distributed systems layer.
- **Consequence**: BlazeCode cannot run as a multi-instance service. Any clustering attempt immediately encounters split-brain for write operations.
- **Recommendation**: For single-user local use, this is acceptable. For multi-instance, adopt an external KV store (Redis/FoundationDB) and implement a distributed coordination layer. At minimum, make the bus and database pluggable.
- **Severity**: Critical

---

## 2. Horizontal Scaling

- **Location**: `blazecode/crates/blazecode-core/src/database.rs:59-66`, `blazecode/infra/console.ts:11-44`
- **BlazeCode**: PlanetScale (MySQL-compatible Vitess) provides horizontal read replicas, sharding-ready, connection pooling. Database branching per stage (`console.ts:23-28`). Cloudflare Workers scale to thousands of instances automatically. Upstash Redis for session caching.
- **BlazeCode**: SQLite with WAL mode (`database.rs:60`) - single-writer bottleneck. `PRAGMA busy_timeout = 5000` (5s) is the only concurrency mechanism. `dashmap` (concurrent HashMap) for in-memory state, but all writes serialize on SQLite.
- **Gap**: **Critical.** SQLite is fundamentally single-writer. Adding instances increases read capacity slightly but write capacity stays at 1. BlazeCode's PlanetScale supports read replicas and Vitess-based sharding.
- **Consequence**: At ~100+ concurrent sessions writing to DB, SQLite contention will dominate. WAL helps reads but all mutations (`insert_session`, `update_session`, `insert_message`) serialize per-transaction.
- **Recommendation**: Keep SQLite for local/dev. For production multi-instance: swap to PostgreSQL via `sqlx` (already abstracted), add PgBouncer for pooling. The `DatabaseService` struct is well-factored and could be trait-abstracted.
- **Severity**: Critical

---

## 3. Vertical Scaling

- **Location**: `blazecode/Cargo.toml:13` (tokio multi-thread), `blazecode/crates/blazecode-core/src/database.rs:301-302`
- **BlazeCode**: Bun/Node.js (single-threaded event loop), PlanetScale handles connection scaling independently. Cloudflare Workers have per-worker memory limits (128MB default) but scale horizontally instead.
- **BlazeCode**: Multi-threaded tokio runtime - benefits from more CPU cores. More memory → larger SQLite `cache_size` (`-64000` = 64MB page cache by default). Each session is an async task on tokio's work-stealing scheduler.
- **Gap**: **Medium.** BlazeCode benefits more from vertical scaling than BlazeCode due to native multi-threading. But SQLite is the ceiling — beyond ~4 CPU cores, SQLite contention (single-writer) negates extra cores.
- **Consequence**: BlazeCode's primary bottleneck is SQLite, not CPU. With tokio work-stealing, more cores help parallel I/O-bound sessions (LLM streaming is network I/O), but all database writes serialize.
- **Recommendation**: Increase SQLite `cache_size` for larger contexts. Consider `PRAGMA mmap_size` for read-heavy workloads. Add configurable thread count for the tokio runtime. The real ceiling is SQLite — estimate ~50K operations/sec on modern hardware.
- **Severity**: Medium

---

## 4. Fault Tolerance

- **Location**: `blazecode/crates/blazecode-core/src/flock.rs:23-31`, `blazecode/crates/blazecode-core/src/event.rs:1359-1422`, `blazecode/packages/function/src/api.ts:15-114`
- **BlazeCode**: Cloudflare Workers are stateless — crash means the next request hits another worker. Durable Objects persist state to CF storage. PlanetScale has automatic failover, point-in-time recovery, and cross-region replication. `SyncServer` Durable Object (`function/src/api.ts:15`) uses `ctx.storage` for durable state. Stripe integration for payments with webhook recovery.
- **BlazeCode**: No cross-node fault tolerance. Process crash loses all in-memory state: `SharedBus` subscribers, `AppState` transforms, `EpochManager` cache, in-flight LLM streams. Flock (`flock.rs:23`) provides stale detection (default 60s timeout, 300s acquire timeout) but only for local file-based locks.
- **Gap**: **Critical.** BlazeCode has no node-level fault tolerance. A crash loses:
  - All pending SSE connections (no reconnection logic in `sse.rs`)
  - All in-flight LLM streams (tokens already sent to client are lost)
  - Event bus subscribers (in-memory broadcast channel state)
  - Session runner state (in-progress tool calls)
- **Consequence**: Any process restart forcibly disconnects all clients. Sessions survive only if persisted to SQLite (via `session_runner`'s epoch manager).
- **Recommendation**: Implement SSE reconnect with event replay (`Last-Event-ID` header). Persist bus events to SQLite (EventV2 already supports this in `event.rs:855-1063` but not yet wired to the SSE layer). Add health check + watchdog auto-restart.
- **Severity**: Critical

---

## 5. Recovery

- **Location**: `blazecode/crates/blazecode-core/src/event.rs:1359-1422`, `blazecode/crates/blazecode-core/src/session_runner.rs:467-517`, `blazecode/packages/core/src/event.ts:60-71`
- **BlazeCode**: EventV2 system (`event.ts:60-71`) provides `SerializedEvent` with sequence numbers, replay idempotency checks (`ReplayDiverged`), aggregate ownership claims, and `CursorEvent` for stream position tracking. Commit guards and projectors run in transaction scopes. Session events support full replay from event store.
- **BlazeCode**: EventV2 port is structurally complete (`event.rs:1359-1422`) with `replay()`, `replay_all()`, `claim()` owner tracking, and idempotency checks (event ID uniqueness, sequence divergence, owner mismatch). However, the `SessionRunner::run_turn_attempt()` (`session_runner.rs:578-800`) has overflow recovery via compaction (`TurnControl::ContinueAfterOverflowCompaction`) but no crash recovery — there's no mechanism to resume an interrupted turn.
- **Gap**: **High.** EventV2 replay infrastructure exists but is not wired into session recovery. BlazeCode can replay events but cannot resume a crashed session mid-turn. BlazeCode's `run-coordinator.ts` and `execution/local.ts` provide the `resume` API for exactly this.
- **Consequence**: Crashed sessions must fully restart from the last persisted epoch. All in-flight work (tool results already computed but not persisted) is lost.
- **Recommendation**: Wire EventV2 replay into session initialization. Implement SessionRunCoordinator's `resume()` function in BlazeCode. Persist tool results as events, not just in-memory state. Use event sourcing for all session state mutations.
- **Severity**: High

---

## 6. Backpressure

- **Location**: `blazecode/crates/blazecode-core/src/bus.rs:208-258`, `blazecode/crates/blazecode-server/src/sse.rs:29-58`, `blazecode/crates/blazecode-core/src/provider.rs` (if accessible)
- **BlazeCode**: Effect-TS provides structured concurrency with interruption propagation. SSE streams have backpressure via the `Stream` type. The V2 run loop uses explicit demand signals (`Demand` in `run-coordinator.ts`). Provider streaming uses async iterables with backpressure built into the protocol.
- **BlazeCode**: `tokio::sync::broadcast` channel in the bus (`bus.rs:214`) has fixed capacity (default 1024). Lagged receivers skip events (`bus.rs:349: "bus subscriber lagged — {skipped} events skipped"`). SSE keepalive is 15s (`sse.rs:19`) but no per-client backpressure — if a client is slow, the broadcast channel drops events. Provider streaming in `session_runner.rs:639` uses `StreamExt::next()` which provides backpressure from the async channel.
- **Gap**: **High.** The broadcast channel has no per-subscriber buffering — one slow consumer causes event loss for all consumers. The `BusSubscription` handles `Lagged` errors by logging and continuing, but this means silent data loss. No backpressure between the LLM provider stream and the client SSE stream.
- **Consequence**: Under high throughput (50+ events/sec), slow SSE consumers will lose events. The gap between producer (LLM stream) and consumer (SSE client) is unbounded.
- **Recommendation**: Replace `broadcast::channel` with a per-subscriber `tokio::sync::mpsc` channel for SSE connections. Implement a bounded buffer with rejection or rate-limiting for the event bus. Add per-client flow control using SSE stream buffering with explicit `RECONNECT` handling.
- **Severity**: High

---

## 7. Database Scaling

- **Location**: `blazecode/crates/blazecode-core/src/database.rs:59-66,269-275`, `blazecode/infra/console.ts:11-44`, `blazecode/packages/core/src/database/database.ts:22-37`
- **BlazeCode**: PlanetScale (MySQL-compatible Vitess) with dedicated cluster per environment (`console.ts:11-14`). Database branching per stage (`console.ts:23-28`: `parentBranch: "production"`). Connection pooling via Vitess proxy. Supports read replicas, sharding, point-in-time recovery. Also uses Upstash Redis for caching (`monitoring.ts` references Upstash).
- **BlazeCode**: SQLite with WAL (`database.rs:60`), synchronous=NORMAL (`database.rs:64`), busy_timeout=5000ms (`database.rs:62`), cache_size=-64000KB (`database.rs:63`). Single connection pool (`sqlx::SqlitePool`). 18 tables, 35 migrations ported from BlazeCode. Serialized via `COALESCE`-based UPDATE pattern (`database.rs:1308-1324`).
- **Gap**: **Critical.** SQLite vs PlanetScale is the biggest architectural divergence. SQLite supports one writer; PlanetScale supports thousands. SQLite max ~281TB but practical limit far lower. BlazeCode's `DatabaseService` uses drizzle-orm with a typed query builder; BlazeCode's `DatabaseService` uses raw `sqlx::query` strings.
- **Consequence**: Hard wall at ~1 writer. Read contention grows with reader count but WAL helps. The `COALESCE`-based UPDATE pattern (`database.rs:1308`) prevents concurrent field-level updates — only one writer can update a session row at a time. At ~100 concurrent sessions, expect SQLITE_BUSY errors.
- **Recommendation**: For the port's local-first goal, SQLite is appropriate. For any multi-instance deployment: abstract `DatabaseService` behind a trait (`#[async_trait] pub trait DatabaseBackend`) and implement a PostgreSQL variant. Use `sqlx::PoolOptions` with max_connections config. Consider `r2d2` connection pooling.
- **Severity**: Critical

---

## 8. State Management

- **Location**: `blazecode/crates/blazecode-core/src/state.rs:145-280`, `blazecode/crates/blazecode-core/src/flock.rs:296-358`, `blazecode/crates/blazecode-core/src/session_epoch.rs` (referenced), `blazecode/packages/core/src/state.ts:55-112`
- **BlazeCode**: `State` module (`state.ts:55-112`) provides replayable transforms over initial values. Transforms are scoped closures, registered via `transform()`, replayed in order on any change. Mutations are one-shot non-replayable edits. Finalize hook runs after every commit. Used for config and UI state, not session state (session uses EventV2).
- **BlazeCode**: `AppState<S>` port (`state.rs:145-280`) with identical semantics — `Transform` closures, `TransformSlot` for updates, `mutate()` for one-shot edits, `rebuild()` for replay. Flock (`flock.rs:296-358`) provides directory-based advisory locks with heartbeat (stale=60s, acquire timeout=300s) and breaker-based stale recovery. Epoch manager (`session_epoch.rs` referenced) manages context epoch state with SQLite persistence.
- **Gap**: **Medium.** `AppState` is in-memory only — crash loses all transforms. No persistence mechanism for transforms (BlazeCode uses Effect's Layer system which provides transactional state). Flock's stale detection (`flock.rs:213: is_stale`) uses filesystem mtime which is unreliable across NFS or distributed filesystems.
- **Consequence**: `AppState` state is ephemeral. Flock locks work across processes on the same machine but not across nodes. Epoch state in SQLite is durable but the in-memory `AppState` transforms are not.
- **Recommendation**: Add optional SQLite persistence for AppState transforms. For flock: document the single-node limitation. Swap `tokio::sync::Mutex` for `std::sync::Mutex` where contention is low (reduces overhead).
- **Severity**: Medium

---

## 9. Event Bus

- **Location**: `blazecode/crates/blazecode-core/src/bus.rs:196-258`, `blazecode/crates/blazecode-core/src/event.rs:632-697,855-1063`, `blazecode/packages/core/src/event.ts:40-51,185-187`
- **BlazeCode**: EventV2 (`event.ts:185-187`) uses typed `PubSub` per event type with `Effect` concurrency. Synchronized events persist to SQLite via drizzle-orm with aggregate sequence tracking. Unsynchronized events are in-memory only. `PublishOptions` supports commit hooks, metadata, location context. `Listener` functions with error isolation. Global channel + per-type channels.
- **BlazeCode**: Two bus systems:
  1. `SharedBus` (`bus.rs:272-307`): `tokio::sync::broadcast` channel, capacity 1024, auto-ID injection, RAII subscription via `BusSubscription` drop.
  2. `EventV2` (`event.rs:775-799`): Full port — typed channels per event type, global channel, database-backed sync events with `event_sequence` table, projectors, listeners, sync handlers, commit guards, aggregate event streaming, replay with idempotency checks, ownership claims.
- **Gap**: **High.** Both bus systems exist but are not unified. `SharedBus` is used by `SessionManager` for CRUD events (`session.created`, `session.updated`). `EventV2` is a separate system with database-backed events. They don't interoperate — events published on `SharedBus` are not persisted; events on `EventV2` are not sent to `SharedBus` subscribers. The `broadcast::channel` in `SharedBus` has no persistence at all.
- **Consequence**: Dual bus creates confusion about where to publish. CRUD events on `SharedBus` are lost on crash. `EventV2` sync events survive. No single event pipeline end-to-end.
- **Recommendation**: Merge into one event bus. Route all events through `EventV2` with a lightweight in-memory shortcut for ephemeral events. Remove `SharedBus` or make it a thin wrapper over `EventV2`'s global channel. This would give all events persistence guarantees.
- **Severity**: High

---

## 10. Session Isolation

- **Location**: `blazecode/crates/blazecode-core/src/session_runner.rs:338-451`, `blazecode/crates/blazecode-core/src/session.rs:605-608`, `blazecode/packages/core/src/session/execution/local.ts:10-33`
- **BlazeCode**: `SessionRunCoordinator` (`execution/local.ts:16`) joins explicit same-session resumes, coalesces prompt wakeups, and allows different sessions to run concurrently. The `drain` function accepts a session ID and runs isolated per session. V2 design docs explicitly state: "Keep local Session drains process-local until clustering is implemented. Different Sessions can run concurrently."
- **BlazeCode**: `SessionRunner` (`session_runner.rs:338-451`) processes one session at a time via the V2 orchestration loop. The `SessionManager` (`session.rs:605-608`) is shared via `Arc` — multiple sessions can be listed/read concurrently, but writes to the same session serialize at SQLite level. Each session runner invocation is independent.
- **Gap**: **Low.** Session isolation is naturally good — each session is a separate async task with its own `SessionRunner` instance. Sessions share only the database pool (serialized by SQLite) and the event bus (serialized by `broadcast::channel`). No session-level locking beyond SQLite row-level locking.
- **Consequence**: Sessions are well-isolated. The only cross-session contention is at the database and event bus level. SQLite WAL mode allows concurrent reads.
- **Recommendation**: Add per-session write lock in `SessionManager` to prevent concurrent writes to the same session. Document that session-level parallelism is limited by SQLite write throughput. Add a configurable max-concurrent-sessions limit.
- **Severity**: Low

---

## 11. Resource Limits

- **Location**: `blazecode/crates/blazecode-core/src/session_runner.rs:37-43`, `blazecode/infra/console.ts:270` (ZEN_LIMITS secret)
- **BlazeCode**: Cloudflare Workers have hard memory limits (128MB default, configurable). BlazeCode Enterprise (`console.ts:270`: `ZEN_LIMITS` secret) has usage-based tier limiting (model access, request counts, concurrent sessions). Stripe integration for billing limits. Stats pipeline (`infra/stats.ts`) tracks token usage, cost, and error rates per model/provider/product tier.
- **BlazeCode**: Step limit (`session_runner.rs:37: MAX_STEPS = 25`), iteration limit (`session_runner.rs:43: DEFAULT_MAX_ITERATIONS = 25`), doom-loop detection (`session_runner.rs:43: DOOM_LOOP_THRESHOLD = 3`). No per-session resource budgeting (memory, tokens, cost). No global resource caps. No billing/rate tier system.
- **Gap**: **High.** BlazeCode has no memory limits per session. An LLM can generate unlimited context until the provider or the OOM killer stops it. No token budgeting. No cost tracking per session (beyond what's in the DB schema). BlazeCode has enterprise-grade usage tracking and limits.
- **Consequence**: A single runaway session can exhaust system memory (especially with large context windows after compaction). No cost control for LLM API usage.
- **Recommendation**: Add per-session token budget with overflow handling (already partially in `SessionCompaction`). Add memory monitoring via `tokio::task::ConsumePermissions` or `jemalloc` stats. Implement per-session cost tracking with hard limits. Wire the existing `SessionInfo.cost` field into a limit enforcement system.
- **Severity**: High

---

## 12. Multi-Tenant Architecture

- **Location**: `blazecode/infra/console.ts:7-44`, `blazecode/infra/enterprise.ts:6-17`
- **BlazeCode**: Full multi-tenant SaaS. PlanetScale database per stage (shared). Enterprise mode (`enterprise.ts:6-17`) uses Cloudflare R2 for isolated storage, custom domain per tenant (`shortDomain`), storage adapter abstraction (`BLAZECODE_STORAGE_ADAPTER: "r2"`). Auth via GitHub OAuth + Google OAuth (`console.ts:59-68`). Stripe billing with tiered plans: BlazeCode Go ($10/mo), BlazeCode Black ($20-$200/mo). Control plane with auth API, organization management, team support.
- **BlazeCode**: Single-user, single-workspace. No tenant concept. `SessionInfo.workspace_id` exists in schema but is nullable. No auth middleware (though `auth.rs` is referenced in `server.rs:217`). No account, org, or billing tables implemented in the port (tables exist in schema).
- **Gap**: **Critical for enterprise.** BlazeCode has no multi-tenant infrastructure. The database schema includes `account`, `control_account`, `account_state`, `workspace` tables but they are scaffold-only — no CRUD operations, no auth flow, no tenant isolation.
- **Consequence**: BlazeCode cannot serve multiple users or organizations. No authentication, no authorization, no billing. Suitable only as a local CLI tool.
- **Recommendation**: Document as single-user only. For multi-tenant: implement workspace isolation via database-level filtering (already has `workspace_id` column). Implement the auth middleware referenced in `server.rs`. Add JWT verification (analogous to BlazeCode's `jwtVerify` in `function/src/api.ts:4`). Wire up the `account` and `account_state` tables.
- **Severity**: Critical

---

## 13. Caching Strategy

- **Location**: `blazecode/crates/blazecode-core/src/database.rs:63`, `blazecode/infra/secret.ts:12-13`
- **BlazeCode**: Upstash Redis for caching (`secret.ts:12-13: UpstashRedisRestUrl, UpstashRedisRestToken`). Redis is used for session cache, rate limiting, and KV storage. Cloudflare's edge network provides CDN caching for static assets and API responses. R2 for blob storage (session shares, file attachments). PlanetScale's built-in query cache for frequent queries.
- **BlazeCode**: No caching layer. SQLite `cache_size` PRAGMA (`database.rs:63: cache_size = -64000` i.e. 64MB page cache) is the only cache. No in-memory session cache (each `get_session()` hits SQLite). No query result caching. No CDN. No blob storage cache.
- **Gap**: **High.** BlazeCode has zero application-level caching. Every session list, every message read, every part query goes to SQLite. With SQLite already being the bottleneck, this compounds the problem.
- **Consequence**: Repeated `get_messages()` calls on the same session re-query SQLite each time. Session listing (sorted by `time_updated DESC`) requires a full table scan at scale. No cache invalidation because there's no cache.
- **Recommendation**: Implement an in-memory session cache using `dashmap` with TTL (already a dependency). Cache the most recent N sessions in memory. Add SQLite query result caching for frequently accessed patterns. Add a `lru` cache for part deserialization (JSON parse is expensive).
- **Severity**: High

---

## 14. Rate Limiting

- **Location**: `blazecode/crates/blazecode-server/src/server.rs:217`, `blazecode/infra/console.ts:270` (ZEN_LIMITS), `blazecode/packages/function/src/api.ts:4` (jose JWT)
- **BlazeCode**: API rate limiting via Upstash Redis + Cloudflare's built-in rate limiting. Tier-based limits (Go/Black plans). Authentication-based rate tiers via JWT. Provider rate limiting handled at the LLM proxy layer. Usage tracking via stats pipeline (`infra/stats.ts`) feeds into billing limits.
- **BlazeCode**: No rate limiting. The `auth` middleware is registered in `server.rs:217` (`axum::middleware::from_fn(crate::auth::auth_middleware)`) but authentication is basic password-only. No token bucket, no leaky bucket, no per-user rate limits. No LLM provider rate limit handling (provider errors are surfaced as-is).
- **Gap**: **High.** No rate limiting means a single client can exhaust LLM API budget, saturate SQLite, or consume all available SSE connections. Provider rate limits (e.g., Anthropic's 429s) are not handled gracefully — they're just errors.
- **Consequence**: No protection against abuse or accidental runaway usage. Provider API costs are unbounded. No fair scheduling between concurrent sessions.
- **Recommendation**: Implement a token bucket rate limiter using `Arc<Mutex<...>>` or `dashmap`. Add per-route rate limiting middleware. Implement exponential backoff for 429 responses from LLM providers. Add daily/monthly token usage caps per session.
- **Severity**: High

---

## 15. Connection Limits

- **Location**: `blazecode/crates/blazecode-server/src/sse.rs:29-58`, `blazecode/Cargo.toml:13` (tokio features)
- **BlazeCode**: Cloudflare Workers handle 1,000+ concurrent connections per worker. Durable Objects support WebSocket state management across connections. PlanetScale handles 10,000+ concurrent database connections. The enterprise tier (`infra/enterprise.ts`) uses dedicated infrastructure.
- **BlazeCode**: SSE connections (`sse.rs:29-58`) are unbounded — no max connections, no per-IP limits. Each SSE client gets a `bus.subscribe()` which creates a new `broadcast::Receiver`. tokio's async I/O handles connections efficiently but there's no upper bound. SQLite pool size is default (`sqlx::SqlitePoolOptions::default()` = ~10-20 connections). No WebSocket support.
- **Gap**: **Medium.** No explicit connection limits. While tokio can handle 10K+ connections, each SSE subscriber adds overhead (broadcast receiver memory, keepalive timer task). SQLite pool exhaustion blocks reads.
- **Consequence**: The system will degrade gracefully under high connection counts (tokio is efficient) but eventually hit SQLite connection pool limits. No rejection policy — all connections are accepted until resource exhaustion.
- **Recommendation**: Add configurable `max_sse_connections` to `ServerConfig`. Implement connection counting in `AppState`. Add per-IP connection rate limiting. Use `tokio::sync::Semaphore` to limit concurrent SSE subscribers. Add WebSocket support for the TUI connection type.
- **Severity**: Medium

---

## Summary

| Dimension | BlazeCode | BlazeCode | Gap Severity |
|---|---|---|---|
| Distributed Readiness | Cloudflare Workers + Durable Objects + PlanetScale | Single-process SQLite | **Critical** |
| Horizontal Scaling | PlanetScale Vitess (sharding-ready) | SQLite single-writer | **Critical** |
| Vertical Scaling | Bun single-threaded, limited benefit | Tokio multi-threaded, SQLite-bottlenecked | Medium |
| Fault Tolerance | Stateless Workers + durable DO state | No cross-node FT, crash loses in-memory state | **Critical** |
| Recovery | EventV2 replay + SessionRunCoordinator.resume() | EventV2 port exists but not wired into session recovery | **High** |
| Backpressure | Effect Stream + per-type PubSub | broadcast::channel drops on lag, no per-client backpressure | **High** |
| Database Scaling | PlanetScale MySQL (Vitess) | SQLite single-file (WAL) | **Critical** |
| State Management | Replayable State + Layer system | AppState in-memory only, Flock single-node | Medium |
| Event Bus | EventV2 unified (sync + async) | Dual bus (SharedBus in-memory, EventV2 DB-backed), not unified | **High** |
| Session Isolation | SessionRunCoordinator per-session | Independent async tasks, SQLite serialization | Low |
| Resource Limits | Tier-based (ZenLite/ZenBlack), stats pipeline | Step limits only, no token/memory/cost budgets | **High** |
| Multi-Tenant | SaaS with orgs, teams, billing | Single-user, tables scaffolded only | **Critical** |
| Caching Strategy | Upstash Redis + CF CDN | SQLite page cache only (64MB) | **High** |
| Rate Limiting | Upstash + CF + tier-based | None | **High** |
| Connection Limits | 1000+ per worker, limited by design | Unbounded, no rejection policy | Medium |

### Overall Assessment

**BlazeCode's architecture is optimized for local-first, single-user CLI usage.** It inherits BlazeCode's schema and feature set but substitutes planet-scale infrastructure for local SQLite. This is appropriate for the target use case but represents a fundamentally different scalability profile.

### Priority Recommendations

1. **Database abstraction** — trait-ify `DatabaseService` so PostgreSQL can be swapped in (paves the way for all infra scaling)
2. **Unify event bus** — route all events through EventV2; make SharedBus a thin wrapper
3. **Implement session recovery** — wire EventV2 replay into `SessionRunner::run_v2()` for crash recovery
4. **Add per-client SSE backpressure** — replace broadcast channel with per-subscriber mpsc channels
5. **Implement resource limits** — token budgets, memory caps, cost tracking per session
6. **Add application-level caching** — in-memory session cache via dashmap with TTL
7. **Add rate limiting middleware** — token bucket per route, per IP
8. **Implement auth and multi-tenant** — at minimum the auth middleware referenced in server.rs
