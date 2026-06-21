# Database Architecture Analysis: RustCode vs OpenCode

**Agent 15 — Database Agent**  
**Date**: 2026-06-21  
**Scope**: Full-stack database comparison — schema, queries, migrations, indexing, transactions, event store, snapshots, concurrency, and infrastructure.

---

## 1. Schema Design

### Table Inventory

| # | Table | OpenCode (Drizzle ORM) | RustCode (sqlx raw SQL) | Parity |
|---|-------|----------------------|------------------------|--------|
| 1 | `workspace` | ✅ | ✅ `CREATE_TABLE_WORKSPACE` | Full |
| 2 | `project` | ✅ | ✅ `CREATE_TABLE_PROJECT` | Full |
| 3 | `project_directory` | ✅ | ✅ `CREATE_TABLE_PROJECT_DIRECTORY` | Full |
| 4 | `session` | ✅ | ✅ `CREATE_TABLE_SESSION` | Full |
| 5 | `session_message` | ✅ | ✅ `CREATE_TABLE_SESSION_MESSAGE` | Full |
| 6 | `session_input` | ✅ | ✅ `CREATE_TABLE_SESSION_INPUT` | Full |
| 7 | `session_context_epoch` | ✅ | ✅ `CREATE_TABLE_SESSION_CONTEXT_EPOCH` | Full |
| 8 | `session_share` | ✅ | ✅ `CREATE_TABLE_SESSION_SHARE` | Full |
| 9 | `message` (legacy) | ✅ | ✅ `CREATE_TABLE_MESSAGE` | Full |
| 10 | `part` (legacy) | ✅ | ✅ `CREATE_TABLE_PART` | Full |
| 11 | `todo` | ✅ | ✅ `CREATE_TABLE_TODO` | Full |
| 12 | `account` | ✅ | ✅ `CREATE_TABLE_ACCOUNT` | Full |
| 13 | `control_account` | ✅ | ✅ `CREATE_TABLE_CONTROL_ACCOUNT` | Full |
| 14 | `account_state` | ✅ | ✅ `CREATE_TABLE_ACCOUNT_STATE` | Full |
| 15 | `credential` | ✅ | ✅ `CREATE_TABLE_CREDENTIAL` | Full |
| 16 | `permission` | ✅ | ✅ `CREATE_TABLE_PERMISSION` | Full |
| 17 | `event` | ✅ | ✅ `CREATE_TABLE_EVENT` | Full |
| 18 | `event_sequence` | ✅ | ✅ `CREATE_TABLE_EVENT_SEQUENCE` | Full |
| 19 | `data_migration` | ✅ | ✅ `CREATE_TABLE_DATA_MIGRATION` | Full |
| 20 | `migration` | ✅ | ✅ `CREATE_TABLE_MIGRATION` | Full |

- **Location**: `database.rs:472-807` (RustCode); `schema.gen.ts` (OpenCode)
- **OpenCode**: Uses Drizzle ORM `sqliteTable("name", {...})` with snake_case column names. Schema defined as TypeScript objects with Drizzle type system.
- **RustCode**: Duplicates all 20 table schemas as `const &str` SQL literals. `CREATE_TABLE_*` constants mirror the exact Drizzle-generated SQL.
- **Gap**: **No compile-time schema verification.** RustCode's raw SQL strings are not validated against the actual SQLite schema until runtime. A typo in a column name or constraint will not be caught by `cargo build` or `cargo test` (unless a test runs migration against an actual database).
- **Consequence**: Schema drift between RustCode and OpenCode is silent. If OpenCode adds a column via migration, RustCode's `INITIAL_MIGRATION` will create the table *without* that column, and the subsequent ALTER TABLE migration will fail.
- **Recommendation**: Add a compile-time macro or build script that compares Rust `const` SQL strings against TypeScript's Drizzle schema definitions. Alternatively, add an integration test that runs all 35 migrations against a fresh SQLite database and verifies table/column parity.
- **Severity**: **High**

### Column Type Mapping

- **Location**: `database.rs:356-391`, `schema.gen.ts`
- **OpenCode**: Drizzle types: `text()`, `integer()`, `real()`, `text().primaryKey()`, `integer().notNull()`, with `$default` and `$onUpdate` hooks.
- **RustCode**: Raw SQL types: `text PRIMARY KEY`, `integer NOT NULL`, `real DEFAULT 0 NOT NULL`. Uses `chrono::Utc::now().timestamp_millis()` for timestamps.
- **Gap**: RustCode lacks Drizzle's `$default` / `$onUpdate` automatic timestamp handling. Every INSERT must explicitly pass `time_created` and `time_updated`.
- **Consequence**: Callers can forget to set timestamps. The `update_session` and similar methods manually compute `now` but there is no guard enforcing it.
- **Recommendation**: Add a Rust macro or helper that automatically populates timestamp columns on INSERT/UPDATE, mirroring Drizzle's `$default` / `$onUpdate`.
- **Severity**: **Medium**

### JSON Columns

- **Location**: `database.rs:889-1063`
- **OpenCode**: Drizzle stores JSON as `text()` with `$type<T>()` for type safety at the TS level. Deserialization is manual via `JSON.parse`.
- **RustCode**: Provides `JsonColumn<T>`, `json_column_serialize`, `json_column_deserialize` wrappers using `serde_json`. Also has path-specific helpers: `db_absolute_path`, `db_path`, `db_absolute_path_array`, `json_absolute_path_array_column`.
- **Gap**: RustCode adds path normalization (POSIX `/` slashes, absolute path validation) that OpenCode does at the application layer, not the database layer. This is a **strict superset** of OpenCode's functionality.
- **Consequence**: RustCode's path column helpers are more strict than the TS equivalent — they reject relative paths at the column level, which could cause silent failures if a caller passes a non-absolute path.
- **Recommendation**: Keep the validation but add clear error messages. Consider making path validation configurable.
- **Severity**: **Low**

---

## 2. Query Patterns

### Prepared Statements

- **Location**: `database.rs:1253-1278` (example: `insert_session`)
- **OpenCode**: Uses Drizzle's type-safe query builder: `yield* db.insert(schema.session).values({...})`. Drizzle generates parameterized SQL with `?` placeholders.
- **RustCode**: All queries use `sqlx::query("INSERT ... VALUES (?1, ?2, ...)").bind(...).execute()`. Every parameter is explicitly numbered (`?1`, `?2`).
- **Gap**: RustCode uses `?1` (1-indexed, sqlite-specific) while Drizzle generates `?` (positional). Both are correct SQLite but the numbering convention differs.
- **Consequence**: No functional impact. Both use parameterized queries (no SQL injection risk).
- **Severity**: **Info**

### JOIN Patterns

- **Location**: `database.rs:1728-1744`
- **OpenCode**: Drizzle supports eager loading and JOINs via `eq(session_message.session_id, session.id)`.
- **RustCode**: Does **not use SQL JOINs**. Instead, the `get_messages_with_parts` method performs **N+1 queries**: it calls `list_messages(session_id)` then for each message calls `list_parts(msg.id)`.
- **Gap**: **N+1 query problem** — loading all messages for a session requires 1 query for messages + N queries for parts (where N = number of messages). OpenCode likely uses JOINs or Drizzle's `relations` for eager loading.
- **Consequence**: Significant performance degradation for sessions with many messages. A session with 200 messages + 400 parts would require 201 SQL queries instead of 1.
- **Recommendation**: Replace with a `LEFT JOIN` query:
  ```sql
  SELECT m.*, p.* FROM message m LEFT JOIN part p ON m.id = p.message_id WHERE m.session_id = ?1 ORDER BY m.time_created, p.time_created
  ```
  Then group the results in Rust.
- **Severity**: **High**

### Dynamic Query Building

- **Location**: `database.rs:1356-1421` (`list_sessions_global`)
- **OpenCode**: Drizzle's query builder can conditionally add `.where()` clauses: `db.select().from(session).where(and(...optional filters...))`.
- **RustCode**: Builds SQL strings dynamically via `format!()` with `next_bind` tracking. String interpolation of column names (`directory = ?{next_bind}`).
- **Gap**: **String interpolation of SQL fragments** — while parameters are bound via `?`, the WHERE clause structure is built via string formatting. This is safe (conditions are parameterized) but fragile.
- **Consequence**: Minor maintenance burden. Adding a new filter requires updating the bind counter, the condition string, the SQL column list, the ORDER BY, and the bind calls.
- **Recommendation**: Extract into a small query builder pattern or use `sqlx::QueryBuilder` which supports dynamic query construction with proper binding.
- **Severity**: **Medium**

---

## 3. Migrations

### Migration System

- **Location**: `database.rs:1098-1135` (drizzle journal import), `storage.rs:621-1363` (RustCode migrations)
- **OpenCode**: 35 individual migration files under `packages/core/src/database/migration/`. Each is a `.ts` file exporting `{ id, up }` where `up` is an Effect-returning function receiving a Drizzle transaction. Uses a semaphore (`Semaphore.makeUnsafe(1)`) to ensure serial migration application. Supports: fresh install (creates all tables + marks all migrations done), existing install (runs pending `applyOnly`), Drizzle journal import (seeds from `__drizzle_migrations`).
- **RustCode**: All 35 migrations are defined as `Migration { id, sql }` structs in `storage.rs:1226-1364`. A single `INITIAL_MIGRATION` (id `20260127222353_familiar_lady_ursula`) creates all 20 tables at once via `CREATE TABLE IF NOT EXISTS`. Subsequent migrations are ALTER TABLE / CREATE INDEX / DROP TABLE statements. Rollback tested via `FAILING_MIGRATION`.
- **Gap 1**: **RustCode skips the fresh-install optimization.** OpenCode detects fresh installs (no tables) and creates the full schema + marks all 35 migrations as complete in one transaction. RustCode runs all 35 migrations sequentially, even on a fresh database. Each migration's `CREATE TABLE IF NOT EXISTS` is a no-op after the first, but this adds needless overhead.
- **Gap 2**: **RustCode's Drizzle journal import is in `database.rs:1098-1135`** (a separate function) while the TS version is integrated into `migration.ts:apply()`. The Rust import is a `pub async fn` rather than being called automatically during migration — callers must explicitly invoke it.
- **Gap 3**: **No rollback support.** OpenCode migrations are forward-only (no `down`). RustCode follows the same pattern but has a test-only `FAILING_MIGRATION` that proves transaction rollback works during migration failure.
- **Consequence**: Fresh install startup is ~35x slower than necessary. Migrations run outside any semaphore — concurrent startup could attempt duplicate migrations (though SQLite will serialize them).
- **Recommendation**: Add the fresh-install shortcut: if no tables exist, create all at once and mark all migrations complete. Wrap migration application in a semaphore or advisory lock to prevent concurrent migration attempts.
- **Severity**: **Medium**

### Migration ID Convention

- **Location**: `database.rs:982-1018`, `migration.gen.ts`
- **OpenCode**: Migration IDs are timestamp-based strings like `20260127222353_familiar_lady_ursula` (timestamp + random words).
- **RustCode**: Mirrors exactly with `KNOWN_MIGRATION_IDS` showing all 35 IDs. Chronological order verified by tests.
- **Gap**: None — exact match.
- **Severity**: **Info**

---

## 4. Indexes

### Index Coverage

- **Location**: `database.rs:816-834`, `storage.rs:1125-1143`
- **OpenCode**: 17 indexes defined in `schema.gen.ts` lines 242-274.
- **RustCode**: Same 17 indexes duplicated as `CREATE_INDEXES` and `ALL_INDEX_SQL`.

| Index | Columns | Unique? | Present? |
|-------|---------|---------|----------|
| `event_aggregate_seq_idx` | `(aggregate_id, seq)` | YES | ✅ |
| `event_aggregate_type_seq_idx` | `(aggregate_id, type, seq)` | NO | ✅ |
| `permission_project_action_resource_idx` | `(project_id, action, resource)` | YES | ✅ |
| `message_session_time_created_id_idx` | `(session_id, time_created, id)` | NO | ✅ |
| `part_message_id_id_idx` | `(message_id, id)` | NO | ✅ |
| `part_session_idx` | `(session_id)` | NO | ✅ |
| `session_input_session_pending_delivery_seq_idx` | `(session_id, promoted_seq, delivery, admitted_seq)` | NO | ✅ |
| `session_input_session_admitted_seq_idx` | `(session_id, admitted_seq)` | YES | ✅ |
| `session_input_session_promoted_seq_idx` | `(session_id, promoted_seq)` | YES | ✅ |
| `session_message_session_seq_idx` | `(session_id, seq)` | YES | ✅ |
| `session_message_session_type_seq_idx` | `(session_id, type, seq)` | NO | ✅ |
| `session_message_session_time_created_id_idx` | `(session_id, time_created, id)` | NO | ✅ |
| `session_message_time_created_idx` | `(time_created)` | NO | ✅ |
| `session_project_idx` | `(project_id)` | NO | ✅ |
| `session_workspace_idx` | `(workspace_id)` | NO | ✅ |
| `session_parent_idx` | `(parent_id)` | NO | ✅ |
| `todo_session_idx` | `(session_id)` | NO | ✅ |

- **Gap**: None — full index coverage parity.
- **Missing Index (both)**:
  1. `event` table has no index on `(aggregate_id, ...)` that matches the `list_events_by_aggregate` query (only `type` + `seq` combined). The `(aggregate_id, seq)` index covers this partially but not optimally.
  2. `session` table could benefit from `(project_id, time_updated DESC, id DESC)` composite index matching the `list_sessions` query pattern.
  3. `session` table has no index on `(directory, time_updated)` matching the `list_sessions_global` directory filter.
- **Recommendation**: Add `(project_id, time_updated DESC, id DESC)` index on `session` and `(directory, time_updated)` index on `session`.
- **Severity**: **Low** (SQLite can use partial index matching for simple queries)

---

## 5. Transactions

### Transaction Boundaries

- **Location**: `storage.rs:727-728` (migrations), `event.rs:899-986` (event publish), `event.rs:1285-1303` (aggregate removal)
- **OpenCode**: Drizzle's `db.transaction(tx => Effect.gen(...))` with `{ behavior: "immediate" }`. Transaction per migration, per event publish, per aggregate removal. Supports nested transactions (reuses parent tx).
- **RustCode**: Uses `sqlx::SqlitePool::begin()` / `tx.commit()` / `tx.rollback()`. Transaction per:
  - Migration step (`storage.rs:727`)
  - Sync event publish: read seq → check event → run guards → run projectors → commit hook → UPSERT sequence → INSERT event → commit (`event.rs:899-986`)
  - Aggregate removal: delete events → delete sequence → commit (`event.rs:1285-1303`)
  - Replay: UPSERT sequence → INSERT event → commit (`event.rs:1470-1499`)
- **Gap 1**: **RustCode runs projectors inside the database transaction** (`event.rs:943-948`). If a projector fails, the entire event write is rolled back. OpenCode runs projectors after the transaction commits (post-commit hooks). Running projectors inside the tx keeps the transaction open longer and can trigger deadlocks.
- **Gap 2**: **No nested transaction support.** OpenCode's `Database.use` inside `Database.transaction` reuses the parent tx. RustCode always opens a new `begin()`, which would create a savepoint in SQLite but the code doesn't account for this.
- **Gap 3**: **No `immediate` mode.** OpenCode uses `{ behavior: "immediate" }` to avoid SQLITE_BUSY errors in concurrent scenarios. RustCode uses default deferred transaction mode.
- **Consequence**: Running projectors inside the transaction increases contention and rollback scope. Lack of `immediate` mode risks serialization failures under concurrent writes.
- **Recommendation**: Move projectors to post-commit hooks (after `tx.commit().await`). Add `BEGIN IMMEDIATE` for event publish transactions. Propagate a `Transaction` type through the call chain to support nested tx reuse.
- **Severity**: **Critical** (projectors inside tx is a correctness issue for event sourcing)

### Transaction in `commit_sync_event`

- **Location**: `event_projector.rs:276-331`
- **OpenCode**: Single transaction that writes event + updates sequence.
- **RustCode**: `commit_sync_event` does **not use a transaction** at all. It calls `db.insert_event()` then `db.upsert_event_sequence()` as separate non-transactional queries.
- **Gap**: **Missing atomicity** — if `db.insert_event` succeeds but `db.upsert_event_sequence` fails, the database has an orphan event with no sequence tracking, and a subsequent write will produce a duplicate sequence number.
- **Consequence**: Potential data corruption — orphan events or duplicate sequence numbers that break event sourcing invariant (strictly increasing contiguous seq per aggregate).
- **Recommendation**: Wrap `insert_event` + `upsert_event_sequence` in a SQLite transaction using `pool.begin()`. Ensure both operations succeed or roll back together.
- **Severity**: **Critical**

---

## 6. Connection Management

### Pool Configuration

- **Location**: `storage.rs:658-670`, `database.rs:59-66`, `Cargo.toml:13`
- **OpenCode**: Uses `@effect/sql-sqlite-bun` or `@effect/sql-sqlite-node` as the underlying driver. Effect manages the connection lifecycle via `Layer`. Single connection per process (Bun SQLite is single-connection by default).
- **RustCode**: Uses `sqlx::SqlitePool` with `sqlx::SqlitePool::connect()` via `sqlite:...?mode=rwc` URL. Pool is created with default settings (sqlx pool defaults: max 10 connections for SQLite, but SQLite only supports one writer at a time).
- **Gap 1**: **`sqlx::SqlitePool` is designed for multi-connection.** SQLite in WAL mode supports multiple readers but only one writer. A pool of 10 connections to SQLite is wasteful — most connections will be idle, waiting on the WAL write lock.
- **Gap 2**: **No max connection limit is explicitly set.** The default pool size for sqlx SQLite is `num_cpus * 2`, which could be 16+ connections for a machine with many cores, all contending for the same SQLite file.
- **Consequence**: Higher than necessary memory usage from idle pool connections. Potential for SQLITE_BUSY errors under high write concurrency as many connections compete for the write lock.
- **Recommendation**: Set `pool_options.max_connections = 3` (1 writer + 2 readers) or use a single connection with `sqlx::sqlite::SqlitePoolOptions::new().max_connections(3)`. The `busy_timeout = 5000` already provides reasonable contention handling.
- **Severity**: **Medium**

### Connection URI

- **Location**: `storage.rs:664`
- **OpenCode**: Uses Effect's `SqliteClient.layer({ filename })`.
- **RustCode**: URI format `sqlite:{path}?mode=rwc` — `rwc` means read-write-create. This implicitly creates the database if it doesn't exist.
- **Gap**: RustCode does not distinguish between "database doesn't exist, create it" and "database does exist, open it". If the path points to the wrong location, a new empty database is silently created instead of returning an error.
- **Consequence**: Silent data loss — running the app with a wrong `OPENCODE_DB` path creates a new empty database, migrations succeed, but there is no user data.
- **Recommendation**: Validate the database path: if `mode != :memory:`, check that the file exists OR the user explicitly expects creation. Consider using `mode=rw` (read-write, no create) in production with a separate `create` command.
- **Severity**: **Medium**

---

## 7. ORM vs Raw SQL

### Layer Comparison

| Aspect | OpenCode (Drizzle ORM) | RustCode (sqlx raw SQL) |
|--------|----------------------|------------------------|
| **Query building** | Type-safe `db.select().from(table).where(eq(...))` | Raw `"SELECT ... FROM ... WHERE ..."` strings |
| **Type safety** | Compile-time table/column validation | `sqlx::query_as::<_, RowType>()` — row type checked at compile time, SQL strings not validated |
| **Migrations** | Effect-returning functions with Drizzle schema | `&str` SQL literals |
| **Transaction** | `tx.insert(table).values(...)` | `sqlx::query("INSERT ...").execute(&mut tx)` |
| **Schema definition** | TypeScript `sqliteTable("name", {...})` | Raw `"CREATE TABLE ..."` `const &str` |
| **IDE support** | Autocomplete on columns, tables, joins | None for SQL strings |

- **Gap**: **`sqlx` does not validate SQL syntax at compile time for SQLite.** For PostgreSQL, sqlx can connect to a DB at compile time and verify queries. For SQLite, it does not have this capability. Every raw SQL string is unchecked until runtime.
- **Consequence**: A typo in a column name (e.g., `UPDATE session SET tokns_input = ...`) passes `cargo build` and `cargo test` (if no test exercises that path) and only fails at runtime.
- **Recommendation**: Add integration tests that run every query pattern against an in-memory SQLite database. Consider adding a `sqlx::migrate!` or compile-time SQL validation macro. Use `cargo sqlx prepare` for PostgreSQL-like checking if feasible.
- **Severity**: **High**

### Row Mapping

- **Location**: `database.rs:3163-3300`, `database.rs:3196-3230`
- **OpenCode**: Drizzle returns typed JavaScript objects directly from queries.
- **RustCode**: Requires a two-tier mapping: `RowRaw` (with `#[derive(sqlx::FromRow)]`) → `into_row()` → `Row`. This is because Drizzle column names like `type` conflict with Rust keywords, requiring `#[sqlx(rename = "type")]`.
- **Gap**: The `RowRaw` → `Row` pattern adds ~10 lines of boilerplate per table. There are 20 tables with ~400 total lines of mapping code.
- **Consequence**: Maintainability burden. Adding a column requires updating: the table SQL constant, the migration SQL, the `Row` struct, the `RowRaw` struct, and the `into_row()` method.
- **Recommendation**: Use `#[serde(rename = "...")]` directly on the public `Row` structs and derive `Deserialize` + `sqlx::FromRow` on them, eliminating the `RowRaw` intermediate type. Where keyword conflicts exist (e.g., `type`), rename the field in the query with `AS`.
- **Severity**: **Low**

---

## 8. Event Store

### Event Sourcing Pattern

- **Location**: `event.rs:554-668`, `event.rs:770-1063`, `database.rs:567-588`
- **OpenCode**: `EventV2` system with typed definitions (`EventV2.define()`), sync/async events, pub/sub, projectors, commit guards, replay, and aggregate ownership tracking. Events stored in `event` table with `event_sequence` table for aggregate tracking.
- **RustCode**: Faithful port of the entire EventV2 system. Implements:
  - `EventDefinition` with type tag, optional SyncConfig, and data schema
  - `EventPayload` runtime envelope with id, type, version, seq, location, metadata
  - `EventRegistry` for typed event definitions
  - `EventPubSub` per-type + global broadcast channels
  - `EventProjector` for catch-up projection and rebuild
  - `commit_sync_event` helper
  - `ReplayOptions` with strict owner checks, sequence divergence detection
- **Gap 1**: **`commit_sync_event` is not atomic** (see Section 5).
- **Gap 2**: **Event uniqueness check is an extra query** (`event.rs:917-931`). The `event_aggregate_seq_idx` UNIQUE index on `(aggregate_id, seq)` would catch duplicate seq violations without the explicit SELECT query. OpenCode relies on the DB constraint.
- **Gap 3**: **Projectors run inside DB transaction** (see Section 5).
- **Gap 4**: **No event version upgrade path.** OpenCode supports `version` in `SyncConfig` and `versioned_type()` for forward compatibility (e.g., `session.next.prompted.1` → `session.next.prompted.2`). RustCode implements the versioning types but does not handle event data migration during version upgrade.
- **Consequence**: The extra uniqueness query before INSERT is a minor performance issue (an index lookup on a hot path). Missing version upgrade handling means stored events with an old version cannot be transparently migrated to a new schema.
- **Recommendation**: Remove the explicit SELECT for event ID uniqueness check — rely on the UNIQUE index constraint and catch the constraint violation error. Add a version migration mechanism that transforms stored event data when the schema version changes.
- **Severity**: **Medium**

### Event Types (Session Events)

- **Location**: `event.rs:1648-2290`
- **OpenCode**: ~40+ session event types defined as TypeScript interfaces with `define()`.
- **RustCode**: All types ported as Rust structs with `Serialize`/`Deserialize`. Well-known event type constants in `session_event_types` module.
- **Gap**: None — full type parity. RustCode even adds `session_event_types` constants module that OpenCode lacks as a single import.
- **Severity**: **Info**

---

## 9. Snapshot Strategy

### Snapshot Service

- **Location**: `snapshot.rs:1-1443`
- **OpenCode**: Uses a sideband git repository (`~/.local/share/opencode/snapshot/<project>/<hash>`) separate from the user's repo. Tracks filesystem state with `git write-tree`, `git read-tree`, `git checkout-index`. Supports: `track()`, `patch()`, `restore()`, `revert()`, `diff()`, `diff_full()`, `cleanup()`.
- **RustCode**: Full port of the snapshot service. Uses `std::process::Command` to run git with `--git-dir` and `--work-tree` flags. Supports:
  - `init()` with config (core.autocrlf, core.longpaths, feature.manyFiles, etc.)
  - `track()` — stages changes, filters excluded/large files, writes tree, returns hash
  - `patch()` — list changed files since snapshot
  - `restore()` — full restore via read-tree + checkout-index
  - `revert()` — selective file revert with dedup
  - `diff()` — unified diff between snapshot and current state
  - `diff_full()` — full diff with file contents via `cat-file --batch`
  - `cleanup()` — `git gc --prune=7.days`
- **Gap 1**: **`StdMutex<()>` lock** (`snapshot.rs:138`). A single global lock serializes all snapshot operations (track, restore, revert, diff) across all sessions/projects. This means taking a snapshot for session A blocks restoring a snapshot for session B.
- **Gap 2**: **`blocking_write()` call in `cat-file --batch` fallback path** — if `batch_cat_file` fails, it falls back to per-file `git show` calls in a loop. This is not async.
- **Gap 3**: **Heartbeat / stale detection only in `flock.rs`** — the snapshot service has no equivalent of OpenCode's `SnapshotService.timer` heartbeat for long-running operations.
- **Consequence**: The global mutex is a scalability bottleneck for multi-session workflows. Concurrent tool calls across different sessions will serialize on snapshot operations.
- **Recommendation**: Replace global `Mutex<()>` with per-snapshot-repo locking (keyed by the `gitdir` path). Use `tokio::sync::Mutex` to avoid blocking the async runtime during snapshot operations.
- **Severity**: **High**

### Context Epoch / Compaction

- **Location**: `persistence.rs:1200-1218` (compaction), `database.rs:1856-1894` (context epoch CRUD), `session_compaction.rs:1-1045`
- **OpenCode**: Context epochs track the compaction state of a session. Each epoch has a `baseline` (compaction summary text), `snapshot` (git tree hash of the workspace at epoch start), `baseline_seq` / `replacement_seq` (event sequence numbers), and `revision` (optimistic lock counter for concurrent updates).
- **RustCode**: Full port including:
  - `session_context_epoch` table with `revision` guard
  - `upsert_context_epoch`, `get_context_epoch`, `delete_context_epoch`
  - `update_context_epoch_snapshot` (with revision guard)
  - `replace_context_epoch` (with revision guard)
  - `SessionCompaction` service: `should_compact`, `select`, `compact`, `compact_if_needed`
  - `CompactionSelector` with turn detection and token budget
  - `CompactionSerializer` with LLM prompt building
- **Gap**: **Context epoch revision guard is the only optimistic concurrency control.** OpenCode also uses event sourcing for the compaction state (compaction events are durable sync events). RustCode's `replace_context_epoch` update is guarded by `WHERE revision = ?7`, which prevents lost updates but does not guarantee ordering with respect to the event stream.
- **Consequence**: If two processes compact the same session simultaneously, both will read the same revision, but only one will succeed (revision mismatch). The other will fail and retry. This is correct but retry logic is not implemented — the caller just gets `false` back.
- **Recommendation**: Add retry logic around `replace_context_epoch` failures. Consider using the event sequence number as the definitive ordering mechanism rather than a separate revision counter.
- **Severity**: **Medium**

---

## 10. Read/Write Patterns

### Workload Profile

- **OpenCode**: Read-heavy during session browsing (listing sessions, loading messages). Write-heavy during LLM streaming (inserting messages, parts, events). Balanced during compaction (reads all messages, writes summary + context epoch).
- **RustCode**: Same workload profile. SQLite WAL mode allows concurrent reads during writes.
- **Gap**: **No read replica or CQRS.** Both use a single SQLite database for read and write. OpenCode's architecture spec mentions PlanetScale + stats DB, but the core app uses a single SQLite file.
- **Severity**: **Info** (acceptable for single-user CLI tool)

### CQRS via Event Sourcing

- **Location**: `event.rs:700-764`, `event_projector.rs:48-264`
- **OpenCode**: EventV2 uses a CQRS-like pattern where:
  - Commands produce events (write path via `publish`)
  - Events are projected to update read models (read path via `project`/`sync`)
  - `EventProjector` replays events to rebuild state
- **RustCode**: Same pattern with `EventProjector` supporting `catch_up` and `rebuild`.
- **Gap**: **Projection state is in-memory only** (`event_projector.rs:66`). The `state: RwLock<HashMap<String, ProjectionState>>` is lost on restart. OpenCode persists projection checkpoints to the `event_sequence` table (which the `db_last_seq` fallback reads). But the in-memory state is the primary source and the DB is a fallback — this is inverted.
- **Consequence**: On restart, the projector reads from seq 0 (or the DB checkpoint) and re-projects all events. This is correct but wasteful — all events from the beginning of time are replayed on every restart.
- **Recommendation**: Make DB the checkpoint authority. Initialize in-memory state from DB on startup. Only use in-memory state as a write-through cache during the session lifetime.
- **Severity**: **Medium**

---

## 11. Data Integrity

### Foreign Keys

- **Location**: `database.rs:472-807`
- **OpenCode**: Foreign keys defined in Drizzle schema with `references(() => table.columns, { onDelete: "cascade" })`.
- **RustCode**: Foreign keys defined as inline constraints in CREATE TABLE SQL:
  - `session.project_id` → `project(id) ON DELETE CASCADE`
  - `workspace.project_id` → `project(id) ON DELETE CASCADE`
  - `message.session_id` → `session(id) ON DELETE CASCADE`
  - `part.message_id` → `message(id) ON DELETE CASCADE`
  - `event.aggregate_id` → `event_sequence(aggregate_id) ON DELETE CASCADE`
  - `session_input.session_id` → `session(id) ON DELETE CASCADE`
  - etc.
- **Gap**: None — all 15+ foreign key constraints are present in both.
- **Severity**: **Info**

### Cascade Behavior

- **Location**: `database.rs:1829-1848`
- **OpenCode**: Relies on DB-level CASCADE for most deletes. `delete_session_cascade` handles child sessions explicitly (no self-referencing FK CASCADE).
- **RustCode**: Same pattern. `delete_session_cascade` explicitly deletes child sessions, then the cascade handles messages → parts.
- **Gap**: **No explicit cascade for `delete_project`** in RustCode. Deleting a project should cascade to sessions → messages → parts → todos → permissions → workspaces → context_epochs → session_shares. The FK constraints handle this if `PRAGMA foreign_keys = ON` is set, which it is.
- **Severity**: **Low** (covered by FK constraints)

### Token Storage

- **Location**: `database.rs:514-525` (account table)
- **OpenCode**: Account tokens stored in `account.access_token` / `account.refresh_token` as `text` columns.
- **RustCode**: Same — plaintext storage.
- **Gap**: **No encryption at rest for access tokens or refresh tokens.** Both OpenCode and RustCode store tokens as plain text in SQLite.
- **Consequence**: Anyone with filesystem access to the SQLite database file can read the user's API tokens.
- **Recommendation**: Use OS keychain integration (macOS Keychain, Linux Secret Service, Windows Credential Manager) or at minimum encrypt token columns with a device-derived key.
- **Severity**: **High**

---

## 12. Performance

### N+1 Queries

| Query Pattern | OpenCode | RustCode | Issue |
|-------------|----------|----------|-------|
| `get_messages_with_parts` | Likely JOIN | 1 + N queries | **Critical** |
| `list_sessions + project lookup` | Likely JOIN | Separate queries | Low |
| `get_session + workspace lookup` | Likely JOIN | Separate queries | Low |

### Hot Path Analysis

1. **Session listing** (`database.rs:1515-1536`): `SELECT ... FROM session WHERE project_id = ?1 ORDER BY time_updated DESC LIMIT ?2`. Well-indexed by `session_project_idx`.
2. **Message listing** (`database.rs:1565-1581`): `SELECT ... FROM message WHERE session_id = ?1 ORDER BY time_created ASC LIMIT ?2`. Indexed by `message_session_time_created_id_idx`.
3. **Event streaming** (`database.rs:2594-2617`): `SELECT ... FROM event WHERE aggregate_id = ?1 AND seq > ?2 ORDER BY seq ASC LIMIT ?3`. Indexed by `event_aggregate_seq_idx` (UNIQUE).
4. **Session search** (`database.rs:1356-1421`): Dynamic query with `directory = ?`, `title LIKE ?`, `parent_id IS NULL`, `time_archived IS NULL`. No composite index for combined filters.
- **Gap 1**: No composite index on `session(directory, time_updated)` for `list_sessions_global` directory filter.
- **Gap 2**: No composite index on `session(project_id, time_updated DESC, id DESC)` for `list_sessions`.
- **Recommendation**: Add composite indexes for the most common query patterns.
- **Severity**: **Medium**

---

## 13. Concurrent Access

### SQLite WAL Mode

- **Location**: `database.rs:59-66`, `storage.rs:673-680`
- **OpenCode**: PRAGMAs: `journal_mode=WAL`, `synchronous=NORMAL`, `busy_timeout=5000`, `cache_size=-64000`, `foreign_keys=ON`, `wal_checkpoint(PASSIVE)`.
- **RustCode**: Same 6 PRAGMAs in `CONNECTION_PRAGMAS` and `Database::open()`. Additionally, the `DatabaseConfig.pragmas()` method allows per-config customization.
- **Gap**: None — identical PRAGMA configuration.
- **Severity**: **Info**

### File Locking (Flock)

- **Location**: `flock.rs:1-514`
- **OpenCode**: Directory-based advisory locking using `mkdir` as atomic acquire, with heartbeat files, stale detection, breaker pattern, and token-verified release.
- **RustCode**: Full port of the flock system. Uses `tokio::fs::create_dir` for atomic acquire, `uuid::Uuid::new_v4()` for tokens, heartbeat background task, exponential backoff with jitter, stale detection via mtime.
- **Gap**: **Heartbeat runs on the async runtime** (`flock.rs:329-338`) using `tokio::spawn`. If the runtime is under heavy load, the heartbeat may not fire in time, causing the lock to be considered stale and stolen by another process.
- **Consequence**: Rare (but possible) premature lock release under heavy async load.
- **Recommendation**: Consider spawning the heartbeat on a dedicated low-priority runtime or using a `tokio::time::interval` with a larger safety margin (current: `stale_ms / 3`, recommend: `stale_ms / 5`).
- **Severity**: **Low**

### Concurrent Writes

- **Location**: `event.rs:899-986` (event publish tx), `database.rs:4013-4125` (concurrent test)
- **OpenCode**: Serialized via SQLite WAL mode + `busy_timeout` + `{ behavior: "immediate" }`.
- **RustCode**: Tested with concurrent pool connections. WAL mode enables concurrent readers but only one writer.
- **Gap**: **RustCode does not use `BEGIN IMMEDIATE` for write transactions.** SQLite's default `BEGIN DEFERRED` starts in read mode and upgrades to write on the first mutation. If two concurrent transactions both start in deferred mode, one will get `SQLITE_BUSY` when trying to upgrade. The `busy_timeout=5000` handles this by retrying, but it adds latency.
- **Consequence**: Under concurrent write load (e.g., two sessions writing events simultaneously), one writer may experience 5 second delays while the busy timeout retries the upgrade.
- **Recommendation**: Use `BEGIN IMMEDIATE` for all write transactions. For sqlx, this can be done via `PRAGMA sqlite3_exec("BEGIN IMMEDIATE")` before the transaction or by using `sqlx::Sqlite::begin_with_behavior()`.
- **Severity**: **Medium**

---

## 14. Backup Strategy

### Current State

- **OpenCode**: No built-in backup. Relies on filesystem backup of `~/.local/share/opencode/opencode.db`. The snapshot git repository provides a form of filesystem-level point-in-time recovery but not for the database itself.
- **RustCode**: Same — no backup feature.
- **Gap**: **Neither system implements database backups.** If the SQLite file is corrupted (power loss, disk full, bug), all session data, events, accounts, and credentials are lost.
- **Recommendation**: Add a `rustcode db backup` command that:
  1. Runs `PRAGMA wal_checkpoint(TRUNCATE)` to flush WAL
  2. Copies the database file with `.backup` suffix
  3. Adds backup metadata to a `backup` table (path, timestamp, checksum)
  4. Cleans up backups older than N days
  Alternatively, use SQLite's `.backup` API via `sqlx` (incremental backup to a separate file).
- **Severity**: **High**

---

## 15. Multi-Database Architecture

### OpenCode Infrastructure

- **Location**: `infra/lake.ts:1-327`
- **OpenCode** operates on multiple database tiers:

| Database | Purpose | Technology |
|----------|---------|------------|
| Local SQLite | Session data, events, accounts, projects | Bun SQLite / Drizzle ORM |
| PlanetScale (MySQL) | Cross-session sync, sharing | PlanetScale (Vitess) |
| S3 Tables / Athena | Analytics lake (usage stats, telemetry) | AWS S3 Tables, Glue, Athena |
| Stats Server | Service-side stats ingestion | Firehose → S3 Tables |

- **RustCode**: **Only implements the local SQLite tier.** The PlanetScale and AWS infrastructure are not ported.
- **Gap**: **RustCode has no multi-database architecture.** Features that depend on PlanetScale (session sharing, sync) and the analytics lake (usage statistics, telemetry) are not available.
- **Consequence**: RustCode is a single-user, offline-only tool. It cannot participate in OpenCode's cloud features.
- **Recommendation**: This is by design (RustCode is a port of the core client). Document this limitation explicitly. If cloud features are needed, add a `rustcode-server` that connects to PostgreSQL or the OpenCode API.
- **Severity**: **Info** (by design)

---

## Summary of Findings

| Severity | Count | Key Issues |
|----------|-------|------------|
| **Critical** | 2 | Projectors inside DB transactions; `commit_sync_event` missing atomicity |
| **High** | 4 | No compile-time schema validation; N+1 query for messages+parts; plaintext token storage; no database backup |
| **Medium** | 7 | Dynamic query building; missing fresh-install migration opt; pool size; version upgrade; projection state in-memory only; missing indexes; BEGIN IMMEDIATE |
| **Low** | 3 | Path validation strictness; RowRaw -> Row boilerplate; heartbeat jitter |
| **Info** | 6 | Query parameter convention; index parity; FK parity; PRAGMA parity; event type parity; multi-DB architecture |

### Top 5 Recommendations

1. **Move projectors outside DB transactions** (`event.rs:943-948`): Run projectors after `tx.commit()`, not inside. Prevents long-lived write transactions and guards against rollback of valid events due to projector failures. **Severity: Critical**

2. **Wrap `commit_sync_event` in a transaction** (`event_projector.rs:276-331`): The current implementation of `insert_event` + `upsert_event_sequence` as separate queries can produce orphan events. Add a SQLite transaction around both operations. **Severity: Critical**

3. **Fix N+1 query for messages + parts** (`database.rs:1728-1744`): Replace the 1+N loop with a `LEFT JOIN` query. A single query can fetch all messages and their parts in one round trip. **Severity: High**

4. **Add compile-time schema validation**: Create a build script or integration test that compares Rust SQL table definitions against the canonical OpenCode schema to detect drift. **Severity: High**

5. **Secure token storage**: Encrypt `access_token` and `refresh_token` columns or use OS keychain APIs. Currently anyone with filesystem access can read API tokens. **Severity: High**
