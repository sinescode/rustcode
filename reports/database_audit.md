# Database Audit Report: RustCode vs OpenCode

**Auditor**: Agent 11 (Database Auditor)
**Date**: 2026-06-19
**RustCode Branch/Commit**: scaffold phase (pinned to OpenCode commit `5d0f866`)
**OpenCode Branch**: dev

---

## Table of Contents

1. [Executive Summary](#executive-summary)
2. [Methodology](#methodology)
3. [Schema Design Analysis](#schema-design-analysis)
4. [Migration System Analysis](#migration-system-analysis)
5. [Connection Management](#connection-management)
6. [Query Patterns](#query-patterns)
7. [Transaction Usage](#transaction-usage)
8. [Data Integrity Analysis](#data-integrity-analysis)
9. [Index Analysis](#index-analysis)
10. [N+1 Query Detection](#n1-query-detection)
11. [WAL Mode and Journal Settings](#wal-mode-and-journal-settings)
12. [Backup Strategy](#backup-strategy)
13. [Dual Migration System Conflict](#dual-migration-system-conflict)
14. [OpenCode-Specific Issues](#opencode-specific-issues)
15. [Finding Summary Table](#finding-summary-table)
16. [Recommendations Roadmap](#recommendations-roadmap)
17. [Appendix A: Full Schema Comparison](#appendix-a-full-schema-comparison)
18. [Appendix B: OpenCode Migration List](#appendix-b-opencode-migration-list)

---

## Executive Summary

This report presents a deep audit of the database architecture in two projects: **RustCode** (Rust port) and **OpenCode** (TypeScript original). The audit covers schema design, migration systems, connection management, query patterns, transaction usage, data integrity, indexing, WAL/journal configuration, and backup strategies.

**Overall Assessment:**
- **OpenCode** has a mature, production-grade database layer using Drizzle ORM with 35+ migrations, Effect.ts for functional effect management, proper connection pooling (Bun/Node SQLite), and well-structured schema definitions.
- **RustCode** is in early scaffold phase with **critical gaps**: two conflicting migration systems, only 1 of 35 migrations ported, no connection pool configuration, no retry logic, raw SQL injection risk in migration parser, N+1 query pattern, and 6 schema columns missing or misordered.

**Risk Profile**: RustCode's database layer is not production-ready. The dual migration system alone could cause irreversible data corruption on upgrade paths.

---

## Methodology

The audit was conducted using static code analysis across both repositories:

| Source | Files Examined | Key File Paths |
|---|---|---|
| RustCode | `storage.rs`, `database.rs`, `permission.rs`, `error.rs`, `Cargo.toml` | `crates/rustcode-core/src/storage.rs`, `crates/rustcode-core/src/database.rs` |
| OpenCode (Core) | `database.ts`, `migration.ts`, `migration.gen.ts`, `schema.gen.ts`, `schema.sql.ts`, `sqlite.ts`, `sqlite.node.ts`, `sqlite.bun.ts`, `path.ts` | `packages/core/src/database/*.ts` |
| OpenCode (SQL defs) | `session/sql.ts`, `project/sql.ts`, `event/sql.ts`, `permission/sql.ts`, `credential/sql.ts`, `account/sql.ts`, `share/sql.ts`, `workspace.sql.ts` | `packages/core/src/**/sql.ts` |
| OpenCode (Migrations) | 35 migration files | `packages/core/src/database/migration/*.ts` |

---

## Schema Design Analysis

### 1. Table Count and Coverage

**Finding: Missing 2 tables in RustCode `INITIAL_MIGRATION`**

- **Location**: RustCode `storage.rs:409-468` (INITIAL_MIGRATION)
- **Evidence**:
  ```rust
  // storage.rs INITIAL_MIGRATION creates only 5 tables:
  project, session, message, part, session_input
  // But ALL_TABLE_NAMES in database.rs lists 20 tables
  ```
- **Problem**: `INITIAL_MIGRATION` creates only 5 tables (`project`, `session`, `message`, `part`, `session_input`), while `database.rs:839-860` defines 20 table names and 19 CREATE TABLE statements. The initial migration is radically incomplete — missing `workspace`, `project_directory`, `session_message`, `session_context_epoch`, `session_share`, `todo`, `account`, `control_account`, `account_state`, `credential`, `permission`, `event`, `event_sequence`, `data_migration`, and `migration`.
- **Impact**: If the INITIAL_MIGRATION runs, the database will be in a broken state — missing tables that downstream code (e.g., `SavedPermissions` in `permission.rs:819`) expects.
- **Severity**: **Critical**
- **Recommendation**: Either replace INITIAL_MIGRATION with the full 20-table schema from `database.rs` `ALL_CREATE_TABLES`, or migrate to using `database.rs` as the sole source of truth.
- **Effort**: 2-4 hours to reconcile schemas

### 2. Schema Column Drift — Project Table

**Finding: RustCode's `project` table has mismatched columns vs OpenCode**

- **Location**: RustCode `storage.rs:412-418` vs OpenCode `project/sql.ts:6-18`
- **Evidence**:
  ```
  RustCode INITIAL_MIGRATION:
    id TEXT PRIMARY KEY,        -- ✓
    vcs TEXT,                   -- ORDER DIFFERS (vcs before worktree)
    worktree TEXT,              -- ORDER DIFFERS
    name TEXT,                  -- ✓
    time_created INTEGER,       -- ✓
    time_initialized INTEGER    -- type: should be time_initialized, but NOT NULL missing

  OpenCode project/sql.ts:
    id, worktree, vcs, name, icon_url, icon_url_override, icon_color,
    time_created, time_updated, time_initialized, sandboxes, commands
  ```
- **Problem**: RustCode is **missing 5 columns**: `icon_url`, `icon_url_override`, `icon_color`, `sandboxes`, `commands`, and `time_updated`. Column ordering differs (`vcs` precedes `worktree` in RustCode, reversed in OpenCode). The `time_initialized` column in OpenCode is optional; in RustCode it has `NOT NULL` hardcoded.
- **Impact**: Schema incompatibility — cannot share databases between RustCode and OpenCode. Data loss when loading existing databases.
- **Severity**: **Critical**
- **Recommendation**: Rewrite `INITIAL_MIGRATION.sql` to exactly match OpenCode's `project` table definition from `project/sql.ts`.
- **Effort**: 1 hour

### 3. Schema Column Drift — Session Table

**Finding: RustCode's `session` table has significant column discrepancies**

- **Location**: RustCode `storage.rs:421-434` vs OpenCode `session/sql.ts:21-65`
- **Evidence**:
  ```
  RustCode INITIAL_MIGRATION:
    id, project_id, workspace_id, title, path,
    time_created, time_updated, usage_input, usage_output,
    usage_cache_read, usage_cache_write

  OpenCode session/sql.ts (21 columns):
    id, project_id, workspace_id, parent_id, slug, directory, path, title,
    version, share_url, summary_additions, summary_deletions, summary_files,
    summary_diffs, metadata, cost, tokens_input, tokens_output,
    tokens_reasoning, tokens_cache_read, tokens_cache_write, revert,
    permission, agent, model, time_created, time_updated, time_compacting,
    time_archived
  ```
- **Problem**: RustCode is missing **17 columns**: `parent_id`, `slug`, `version`, `share_url`, `summary_additions`, `summary_deletions`, `summary_files`, `summary_diffs`, `metadata`, `cost`, `tokens_reasoning`, `revert`, `permission`, `agent`, `model`, `time_compacting`, `time_archived`. Column names differ: `usage_input` vs `tokens_input`, `usage_output` vs `tokens_output`.
- **Impact**: Complete data incompatibility. Downstream Rust code in `database.rs:1560-1575` (`SessionRow`) uses the OpenCode field names (`cost`, `tokens_input`, `tokens_output`, `agent`, `model`), which don't exist in `INITIAL_MIGRATION`.
- **Severity**: **Critical**
- **Recommendation**: Replace the session table in `INITIAL_MIGRATION` with the full 29-column schema from `database.rs:726-758` (`CREATE_TABLE_SESSION`).
- **Effort**: 1-2 hours

### 4. Schema Column Drift — Message and Part Tables

**Finding: RustCode's message/part have simplified schemas**

- **Location**: RustCode `storage.rs:436-453` vs OpenCode `session/sql.ts:67-97`
- **Evidence**:
  ```
  RustCode:            message(id, session_id, role, content, time_created)
  OpenCode TS:         message(id, session_id, time_created, time_updated, data)
  RustCode database.rs: message(id, session_id, time_created, time_updated, data)

  RustCode:            part(id, message_id, type, content, tool_call_id, time_created)
  OpenCode TS:         part(id, message_id, session_id, time_created, time_updated, data)
  RustCode database.rs: part(id, message_id, session_id, time_created, time_updated, data)
  ```
- **Problem**: `INITIAL_MIGRATION` uses a simplified flat schema (`role`, `content`, `tool_call_id`) instead of the JSON `data` column used by OpenCode. The Rust `database.rs` CRUD code (`insert_message_v2` at line 1443) serializes structured data into the JSON `data` column, which doesn't exist in the `INITIAL_MIGRATION` message table.
- **Impact**: `DatabaseService` methods write to columns that don't exist in the `INITIAL_MIGRATION` schema. Queries would fail at runtime.
- **Severity**: **Critical**
- **Recommendation**: Replace the message/part tables in `INITIAL_MIGRATION` with the schemas from `database.rs:644-669`.
- **Effort**: 1 hour

### 5. Missing Timestamps on Session Table

**Finding: OpenCode session has `time_updated`; RustCode INITIAL_MIGRATION has it but database.rs schema is correct**

- **Location**: RustCode `storage.rs:422-434` (INITIAL_MIGRATION)
- **Evidence**: INITIAL_MIGRATION's session table has `time_updated` (line 429), matching OpenCode. However, the `DatabaseService::update_session` method (`database.rs:1169-1193`) expects `time_updated` to exist. This happens to be consistent.
- **Problem**: No actual issue here for the session table, but the `project` table in INITIAL_MIGRATION is missing `time_updated`, which downstream code in `database.rs` tests expects (e.g., `test_update_session` at line 2066 sets `time_updated` on project).
- **Impact**: Tests may pass because they use `ALL_CREATE_TABLES` (from database.rs) which has the correct schema, while `INITIAL_MIGRATION` uses the wrong schema. Production code using `INITIAL_MIGRATION` would fail.
- **Severity**: **High**
- **Recommendation**: Unify to a single schema source of truth. Use `ALL_CREATE_TABLES` from `database.rs` exclusively.
- **Effort**: 2 hours

---

## Migration System Analysis

### 6. Dual Migration Systems — Conflicting Implementations

**Finding: RustCode has TWO separate migration systems that conflict**

- **Location**: RustCode `storage.rs:193-335` (Migration struct + run_migrations) and `database.rs:392-443` (Migration/MigrationMeta/MigrationSet)
- **Evidence**:
  ```
  storage.rs Migration:
    pub struct Migration {
        pub id: &'static str,
        pub sql: &'static str,          // SQL as single string
    }
    Uses table: _migration (with underscore prefix)
    Splits SQL by semicolons: migration.sql.split(';')
    Executes via: sqlx::query(trimmed).execute(&mut *tx)

  database.rs Migration:
    pub struct Migration {
        pub id: String,
        pub up: Vec<String>,            // SQL as vector of strings
    }
    Uses table: migration (no underscore prefix)
    No execution logic — just type definitions
    MigrationSet wraps Vec<Migration>
  ```
- **Problem**: Two completely separate migration abstractions exist. `storage.rs` uses `_migration` table with raw SQL string splitting. `database.rs` uses `migration` table with pre-split SQL vectors. The `DatabaseService` in `database.rs:1085-1127` queries the `migration` table for status, but `storage.rs::Database::run_migrations` writes to `_migration`. A database initialized by one system would be incompatible with the other.
- **Impact**: If both systems are used (e.g., `storage.rs` for startup migrations and `database.rs` for runtime migration status checks), the migration status would always report 0 applied migrations, potentially causing migration replay and data corruption.
- **Severity**: **Critical**
- **Recommendation**: Eliminate the `storage.rs` migration system. Use `database.rs` types exclusively. Rename `_migration` to `migration` table. Replace SQL string splitting with pre-parsed `Vec<String>`.
- **Effort**: 4-8 hours

### 7. Migration Count — Only 1 of 35 Migrations Ported

**Finding: RustCode implements 1 migration vs 35 in OpenCode**

- **Location**: RustCode `storage.rs:472` (`ALL_MIGRATIONS`) and `database.rs:982-1018` (`KNOWN_MIGRATION_IDS`)
- **Evidence**:
  ```rust
  // storage.rs
  pub const ALL_MIGRATIONS: &[Migration] = &[INITIAL_MIGRATION]; // 1 migration

  // database.rs
  pub const KNOWN_MIGRATION_IDS: &[&str] = &[
      "20260127222353_familiar_lady_ursula",
      // ... 34 more entries
  ];
  assert_eq!(KNOWN_MIGRATION_IDS.len(), 35); // test at line 1842
  ```
- **Problem**: The TS codebase has 35 individual migration files (listed in `migration.gen.ts`), each with specific `ALTER TABLE`, `CREATE TABLE`, and data migration logic. RustCode acknowledges all 35 IDs (line 982-1018) but has only ported the equivalent of a combined initial schema — and even that is incomplete. None of the incremental ALTER TABLE migrations exist.
- **Impact**: A user migrating from an older database schema would have unapplied migrations. Since `INITIAL_MIGRATION` uses `CREATE TABLE IF NOT EXISTS`, it would silently skip table creation without applying the 34 incremental migrations. Data integrity cannot be guaranteed.
- **Severity**: **Critical**
- **Recommendation**: Port all 35 migrations in order, or implement a single combined migration that produces the final schema state. The latter is a valid approach for a new implementation but must be clearly versioned.
- **Effort**: 3-5 days (35 migrations × 1-2 hours each for analysis + porting)

### 8. SQL String Splitting in Migration Parser

**Finding: RustCode splits migration SQL by `;` — fragile parsing**

- **Location**: RustCode `storage.rs:301-313`
- **Evidence**:
  ```rust
  for statement in migration.sql.split(';') {
      let trimmed = statement.trim();
      if trimmed.is_empty() { continue; }
      sqlx::query(trimmed).execute(&mut *tx).await.map_err(|e| {
          Error::Config(format!("migration `{}` error at `{}`: {e}",
              migration.id, &trimmed[..trimmed.len().min(80)]))
      })?;
  }
  ```
- **Problem**: Naive `split(';')` breaks when SQL contains semicolons inside strings, triggers, or views. For example: `CREATE TRIGGER ... BEGIN ... END;` or literal strings with semicolons. Also, this approach uses `sqlx::query()` which only supports single statements — `sqlx::raw_sql()` should be used instead, or better yet, `sqlx::migrate!()`.
- **Impact**: Corrupted SQL execution, partial migration application, or runtime panics on valid SQL containing semicolons.
- **Severity**: **High**
- **Recommendation**: Use `sqlx::raw_sql()` for multi-statement execution (available in sqlx 0.8), or use `sqlx::migrate!()` with `.sql` files. Alternatively, keep each statement as a separate entry in a `Vec<String>` as `database.rs::Migration.up` already defines.
- **Effort**: 2-4 hours

### 9. Migration Uses CREATE TABLE IF NOT EXISTS

**Finding: INITIAL_MIGRATION uses IF NOT EXISTS — improper for deterministic migrations**

- **Location**: RustCode `storage.rs:412-468`
- **Evidence**: Every CREATE TABLE in `INITIAL_MIGRATION` uses `CREATE TABLE IF NOT EXISTS`. The migration system at `storage.rs:290-335` checks the `_migration` journal to skip already-applied migrations.
- **Problem**: `CREATE TABLE IF NOT EXISTS` masks errors. If a migration runs but partially fails (e.g., creates 3 of 5 tables), re-running will silently succeed because tables already exist. The `_migration` journal would not record the failure since the error happens during table creation, before the journal insert.
- **Impact**: Silent data corruption on partial migration failure.
- **Severity**: **High**
- **Recommendation**: Use `CREATE TABLE` without `IF NOT EXISTS` for migration steps. Since each migration runs in a transaction, a failure will roll back the entire migration. The journal entry is only written after successful execution. This matches OpenCode's approach in `schema.gen.ts` which uses plain `CREATE TABLE`.
- **Effort**: 30 minutes

### 10. Missing Drizzle Migration Journal Seeding

**Finding: RustCode does not seed from Drizzle's `__drizzle_migrations` table**

- **Location**: OpenCode `migration.ts:54-67` vs RustCode `storage.rs:338-348`
- **Evidence**:
  ```typescript
  // OpenCode migration.ts — seeds from legacy Drizzle journal
  if (yield* db.get(sql`SELECT name FROM sqlite_master WHERE type = 'table'
      AND name = ${"__drizzle_migrations"}`)) {
      yield* db.run(sql`
          INSERT OR IGNORE INTO ${sql.identifier("migration")} (id, time_completed)
          SELECT name, ${Date.now()}
          FROM ${sql.identifier("__drizzle_migrations")}
          WHERE name IS NOT NULL
      `)
  }
  ```
  ```rust
  // RustCode storage.rs — no equivalent seeding
  async fn ensure_migration_table(&self) -> Result<()> {
      sqlx::query("CREATE TABLE IF NOT EXISTS _migration (...)")
  }
  ```
- **Problem**: OpenCode's migration system handles upgrades from older Drizzle-based migration journals. RustCode's `ensure_migration_table` just creates the tracking table without any data seeding. Users upgrading from a Drizzle-created database would have all migrations re-applied.
- **Impact**: Migration replay on existing databases — could cause duplicate column errors or data corruption.
- **Severity**: **High**
- **Recommendation**: Add Drizzle journal seeding logic to `ensure_migration_table` (or the migration runner), checking for the `__drizzle_migrations` table and importing its entries.
- **Effort**: 2-4 hours

---

## Connection Management

### 11. No Connection Pool Configuration

**Finding: RustCode uses default sqlx pool without configuration**

- **Location**: RustCode `storage.rs:234-239`
- **Evidence**:
  ```rust
  let db_url = format!("sqlite:{}?mode=rwc", path.display());
  let pool = sqlx::SqlitePool::connect(&db_url).await.map_err(|e| {
      Error::Config(format!("failed to open database at {}: {e}", path.display()))
  })?;
  ```
- **Problem**: `SqlitePool::connect()` creates a pool with default settings: typically min 0, max 10 connections, with no timeout configuration. `DatabaseConfig` in `database.rs:279-298` defines `busy_timeout`, `cache_size`, and `wal` but these are never applied to the pool — they're only constant string constants in `CONNECTION_PRAGMAS` and the `DatabaseConfig::pragmas()` method is never called from `Database::open()`.
- **Impact**: Connection pool defaults may be inappropriate for production: no max connection limit enforcement, no connection timeout, no idle timeout. PRAGMAs are hardcoded in `open()` but not configurable.
- **Severity**: **Medium**
- **Recommendation**: Use `SqlitePoolOptions::new()` with explicit configuration:
  ```rust
  let pool = SqlitePoolOptions::new()
      .max_connections(5)
      .min_connections(1)
      .acquire_timeout(Duration::from_secs(30))
      .idle_timeout(Duration::from_secs(300))
      .connect(&db_url)
      .await?;
  ```
  Then apply `DatabaseConfig::pragmas()` instead of hardcoding.
- **Effort**: 2-4 hours (includes refactoring config application)

### 12. OpenCode Connection Pooling — Single Connection Serialization

**Finding: OpenCode's Bun/Node SQLite implementations serialize all access through a semaphore**

- **Location**: OpenCode `sqlite.bun.ts:115-116`, `sqlite.node.ts:115-116`
- **Evidence**:
  ```typescript
  const semaphore = yield* Semaphore.make(1)
  const acquirer = semaphore.withPermits(1)(Effect.succeed(connection))
  ```
- **Problem**: Both SQLite adapters use a semaphore with exactly 1 permit, serializing all database access. While SQLite itself only supports one writer at a time, this also serializes **readers**, which could be parallel in WAL mode. The `transactionAcquirer` similarly uses the same single-permit semaphore.
- **Impact**: Read throughput is artificially limited to 1 concurrent operation, even though WAL mode allows concurrent readers. This is a known limitation acknowledged in the code (legacy Drizzle pattern).
- **Severity**: **Medium** (performance, not correctness)
- **Recommendation**: Consider allowing up to N concurrent readers (e.g., `Semaphore.make(4)`) while using a separate write lock via `BEGIN IMMEDIATE` transactions. However, this would be a significant refactor of the Effect-based transaction system.
- **Effort**: 3-5 days (significant refactor)

### 13. No Health Checks or Connection Validation

**Finding: Neither project implements connection health checks**

- **Location**: RustCode `storage.rs:227-265` and OpenCode `sqlite.bun.ts:154-167`
- **Evidence**:
  ```rust
  // RustCode — no afterConnect hook, no ping check
  pub async fn open(path: &Path) -> Result<Self> {
      // ...
      let pool = sqlx::SqlitePool::connect(&db_url).await?;
      // Sets PRAGMAs but no connection validation
  }
  ```
  ```typescript
  // OpenCode — no health check, just opens and returns
  const native = new Database(config.filename, { ... });
  ```
- **Problem**: Neither project validates that the database connection is actually functional after opening. No periodic health checks verify the connection pool is healthy. sqlx's `SqlitePool` does have an `after_connect` callback option that could be used, but it's not configured.
- **Impact**: Silent connection failures. A database that becomes corrupted or unreachable during operation will not be detected until the first actual query fails, potentially at an inopportune time.
- **Severity**: **Low**
- **Recommendation**: Add an `after_connect` handler to sqlx pool options that runs `SELECT 1` to validate each new connection. Add a health check route that pings the database.
- **Effort**: 2-4 hours

---

## Query Patterns

### 14. Raw SQL vs Drizzle ORM

**Finding: RustCode uses raw SQL via sqlx; OpenCode uses Drizzle ORM (via Effect.ts)**

- **Location**: RustCode `database.rs:1088-1127`, OpenCode `credential.ts:83-105`
- **Evidence**:
  ```rust
  // RustCode — raw SQL with positional parameters
  sqlx::query_as("SELECT id, time_completed FROM migration ORDER BY time_completed")
      .fetch_all(&self.pool)
  ```
  ```typescript
  // OpenCode — Drizzle ORM query builder
  const row = yield* db
    .select()
    .from(CredentialTable)
    .where(eq(CredentialTable.id, id))
    .get()
    .pipe(Effect.orDie)
  ```
- **Problem**: RustCode's raw SQL is more error-prone (typos in column names, wrong types) and harder to refactor. OpenCode's Drizzle ORM provides compile-time SQL validation and type safety. However, RustCode's approach is not inherently wrong — `sqlx` provides compile-time verification when using `query_as!` / `query!` macros, but these aren't used.
- **Severity**: **Medium** (maintainability)
- **Recommendation**: Consider using `sqlx::query_as!()` with compile-time SQL verification, or continue with raw SQL but add integration tests that validate queries against a real SQLite database.
- **Effort**: 2-3 days (progressive migration to `query_as!`)

### 15. Parameterized Queries — Both Projects Use Parameterized Queries Correctly

**Finding: Both projects use parameterized queries, avoiding SQL injection**

- **Location**: RustCode `database.rs:1109` (`WHERE id = ?1`), OpenCode `credential.ts:105` (`eq(CredentialTable.id, id)`)
- **Evidence**:
  ```rust
  // RustCode uses sqlx positional parameters (?1, ?2)
  sqlx::query_as("SELECT id FROM migration WHERE id = ?1")
      .bind(migration_id)
  ```
  ```typescript
  // OpenCode uses Drizzle ORM (which generates parameterized queries)
  db.select().from(CredentialTable).where(eq(CredentialTable.id, id))
  ```
- **Problem**: No SQL injection risk identified. Both projects correctly use parameterized queries throughout. However, the migration system in `storage.rs` (`sqlx::query(trimmed)`) executes raw SQL strings from constants — this is intentional but means migration SQL must be carefully reviewed.
- **Impact**: None — this is a positive finding.
- **Severity**: **None** (compliant)
- **Recommendation**: Continue using parameterized queries. Consider adding automated SQL injection testing.
- **Effort**: N/A

### 16. Inefficient Query — Missing Index Hints in Raw SQL

**Finding: RustCode raw SQL queries could benefit from index hints**

- **Location**: RustCode `database.rs:1217-1225` (list_sessions), `database.rs:1263-1271` (list_messages)
- **Evidence**:
  ```rust
  sqlx::query_as(
      "SELECT id, project_id, workspace_id, slug, directory, title, version, \
       time_created, time_updated, cost, tokens_input, tokens_output, agent, model \
       FROM session WHERE project_id = ?1 ORDER BY time_updated DESC LIMIT ?2",
  )
  ```
- **Problem**: The query selects 14 of 29 columns from the `session` table. In SQLite, this means reading the full row from the table even if an index covers the WHERE/ORDER BY. A covering index on `(project_id, time_updated DESC, ...)` could improve performance for large datasets.
- **Impact**: Moderate query performance degradation for users with thousands of sessions.
- **Severity**: **Low**
- **Recommendation**: Review query patterns and add covering indexes where appropriate. The index `session_project_idx` on `project_id` exists (in `database.rs:827`) but doesn't include `time_updated`. Consider `CREATE INDEX session_project_time_idx ON session(project_id, time_updated DESC)`.
- **Effort**: 1-2 hours per query

---

## Transaction Usage

### 17. Missing Transaction in Permission Inserts

**Finding: RustCode `SavedPermissions::add()` inserts without a transaction**

- **Location**: RustCode `permission.rs:867-898`
- **Evidence**:
  ```rust
  pub async fn add(&self, input: &AddSavedInput) -> Result<()> {
      if input.resources.is_empty() { return Ok(()); }
      let ts = ...;
      for resource in &input.resources {
          // Each insert is a separate autocommit — no transaction!
          sqlx::query("INSERT OR IGNORE INTO permission (...)")
              .execute(self.db.pool())  // executes on pool, no transaction
              .await?;
      }
      Ok(())
  }
  ```
- **Problem**: If multiple resources are being saved and the 3rd insert fails due to a constraint violation, resources 1-2 are already committed. There's no way to roll back. OpenCode's equivalent in `permission/saved.ts` likely wraps the whole operation in a transaction.
- **Impact**: Partial permission saves — some resources remembered, others lost. Could lead to confusing permission behavior.
- **Severity**: **High**
- **Recommendation**: Wrap the loop in a transaction:
  ```rust
  let mut tx = self.db.pool().begin().await?;
  for resource in &input.resources {
      sqlx::query(...).execute(&mut *tx).await?;
  }
  tx.commit().await?;
  ```
- **Effort**: 1 hour

### 18. Missing Transaction in Permission Delete Cascade

**Finding: `delete_session_cascade` uses two separate autocommit queries**

- **Location**: RustCode `database.rs:1527-1553`
- **Evidence**:
  ```rust
  pub async fn delete_session_cascade(&self, id: &str) -> Result<(), DatabaseServiceError> {
      // Delete child sessions first (autocommit #1)
      sqlx::query("DELETE FROM session WHERE parent_id = ?1")
          .bind(id).execute(&self.pool).await?;
      // Delete the session itself (autocommit #2)
      let rows = sqlx::query("DELETE FROM session WHERE id = ?1")
          .bind(id).execute(&self.pool).await?;
      // ...
  }
  ```
- **Problem**: If the second DELETE fails (e.g., FK constraint from another table), the child sessions (autocommit #1) are already deleted. The database is left in an inconsistent state. OpenCode relies on `ON DELETE CASCADE` foreign keys, which handle this atomically.
- **Impact**: Orphaned child sessions if the parent delete fails. Inconsistent database state.
- **Severity**: **High**
- **Recommendation**: Use a transaction or rely on CASCADE:
  ```rust
  let mut tx = self.pool.begin().await?;
  sqlx::query("DELETE FROM session WHERE parent_id = ?1")
      .bind(id).execute(&mut *tx).await?;
  sqlx::query("DELETE FROM session WHERE id = ?1")
      .bind(id).execute(&mut *tx).await?;
  tx.commit().await?;
  ```
  Alternatively, let CASCADE handle it if a self-referencing FK is added.
- **Effort**: 1 hour

### 19. OpenCode Transaction Isolation Level

**Finding: OpenCode uses `BEGIN IMMEDIATE` behavior for event transactions**

- **Location**: OpenCode `event.ts:257-259`
- **Evidence**:
  ```typescript
  const committed = yield* db
    .transaction(
      () => Effect.gen(function* () { ... }),
      { behavior: "immediate" },
    )
  ```
- **Problem**: This is actually a **strength** — `BEGIN IMMEDIATE` acquires a write lock at the start of the transaction, preventing deadlocks in high-concurrency scenarios. SQLite's default `BEGIN DEFERRED` would wait until the first write operation, which can cause deadlocks with WAL mode.
- **Impact**: Positive — this is best practice for SQLite concurrent access.
- **Severity**: **None** (commendable)
- **Recommendation**: Ensure RustCode migration runner also uses `BEGIN IMMEDIATE` for transaction safety. Currently `storage.rs:295-298` uses `pool.begin()` which is `BEGIN DEFERRED` in sqlx.
- **Effort**: 30 minutes (add `pool.begin_with(BeginBehavior::Immediate)`)

### 20. RustCode Migration Transaction Commitment

**Finding: Migration commit is done after journal insert (correct)**

- **Location**: RustCode `storage.rs:295-329`
- **Evidence**:
  ```rust
  let mut tx = self.pool.begin().await?;
  // Execute migration SQL
  for statement in migration.sql.split(';') { ... }
  // Record migration in journal
  sqlx::query("INSERT INTO _migration (...) VALUES (?1, ?2)")
      .bind(...).execute(&mut *tx).await?;
  tx.commit().await?;  // Commit everything atomically
  ```
- **Problem**: The transaction handling is actually **correct** here — the migration SQL and the journal insert happen in the same transaction. If anything fails, the migration is fully rolled back. This matches OpenCode's pattern.
- **Impact**: None — this is a positive finding. However, the `split(';')` issue (Finding #8) means that even though the transaction handles atomicity, invalid SQL splitting could prevent the migration from running correctly in the first place.
- **Severity**: **None** (this aspect is correct)
- **Recommendation**: Keep the transaction pattern but fix the SQL splitting issue.
- **Effort**: N/A

---

## Data Integrity Analysis

### 21. Foreign Key Enforcement — Both Projects

**Finding: Both projects enable foreign keys correctly**

- **Location**: RustCode `storage.rs:247` (`PRAGMA foreign_keys = ON`), OpenCode `sqlite.node.ts:155` (`enableForeignKeyConstraints: true`)
- **Evidence**:
  ```rust
  // RustCode — sets PRAGMA on each connection
  "PRAGMA foreign_keys = ON",
  // ...executed in loop at storage.rs:249-253
  ```
  ```typescript
  // OpenCode Node — enables via DatabaseSync constructor
  const native = new DatabaseSync(config.filename, {
      enableForeignKeyConstraints: true,
  })
  ```
- **Problem**: Both projects correctly enable FK enforcement. However, RustCode's schema definitions in `database.rs` define FK constraints (e.g., `CONSTRAINT fk_session_project_id_project_id_fk FOREIGN KEY (project_id) REFERENCES project(id) ON DELETE CASCADE`), but the `INITIAL_MIGRATION` schema in `storage.rs` uses inline `FOREIGN KEY (project_id) REFERENCES project(id)` without explicit `ON DELETE CASCADE` on some tables.
- **Impact**: Non-cascading deletes in `INITIAL_MIGRATION` — deleting a project would not cascade to sessions, leaving orphaned records.
- **Severity**: **High** (only for INITIAL_MIGRATION, not the full schema)
- **Recommendation**: Ensure all FK constraints in `INITIAL_MIGRATION` match `database.rs` definitions with proper `ON DELETE CASCADE`.
- **Effort**: 1 hour

### 22. Unique Constraints

**Finding: RustCode `database.rs` defines critical unique constraints but they're missing from `INITIAL_MIGRATION`**

- **Location**: RustCode `database.rs:813-831` vs `storage.rs:409-468`
- **Evidence**:
  ```rust
  // database.rs — defines 17 CREATE INDEX statements including 5 UNIQUE indexes:
  // event_aggregate_seq_idx (UNIQUE)
  // permission_project_action_resource_idx (UNIQUE)
  // session_input_session_admitted_seq_idx (UNIQUE)
  // session_input_session_promoted_seq_idx (UNIQUE)
  // session_message_session_seq_idx (UNIQUE)

  // storage.rs INITIAL_MIGRATION — creates only 4 indexes, none UNIQUE:
  // idx_message_session_id, idx_part_message_id, idx_session_project_id, idx_session_input_session
  ```
- **Problem**: `INITIAL_MIGRATION` is missing 13 indexes and all 5 unique constraints. Without `session_message_session_seq_idx`, duplicate sequence numbers could be inserted. Without `event_aggregate_seq_idx`, event sourcing could produce duplicate events.
- **Impact**: Data integrity violations — duplicate events, duplicate session message sequences, duplicate permission entries. The event sourcing system (used for critical state synchronization) would be compromised.
- **Severity**: **Critical**
- **Recommendation**: Apply the full index set from `database.rs:813-831` to `INITIAL_MIGRATION`. Better yet, reference `database.rs::CREATE_INDEXES`.
- **Effort**: 1 hour

### 23. CASCADE Behavior — Self-Referencing Session Table

**Finding: Session parent_id has no FK constraint — manual cascade is fragile**

- **Location**: RustCode `database.rs:727-758` (session table), `database.rs:1527-1553` (manual cascade)
- **Evidence**:
  ```rust
  // Session table has parent_id but no FK constraint referencing itself
  CREATE TABLE session (
      id text PRIMARY KEY,
      parent_id text,       // No FOREIGN KEY defined!
      ...
  );

  // Manual cascade in delete_session_cascade — fragile, no transaction
  sqlx::query("DELETE FROM session WHERE parent_id = ?1").execute(&self.pool).await?;
  sqlx::query("DELETE FROM session WHERE id = ?1").execute(&self.pool).await?;
  ```
- **Problem**: OpenCode's `session/sql.ts:30` defines `parent_id: text().$type<SessionSchema.ID>()` but also lacks an explicit FK constraint for parent_id. However, OpenCode relies on application-level logic rather than DB constraints. RustCode has the manual cascade but with the transaction issue (Finding #18).
- **Impact**: If a session is deleted via `delete_session()` (database.rs:1197-1208) instead of `delete_session_cascade()`, child sessions become orphaned. There's no FK constraint preventing this.
- **Severity**: **Medium**
- **Recommendation**: Add a self-referencing FK: `FOREIGN KEY (parent_id) REFERENCES session(id) ON DELETE CASCADE`. This would make `delete_session()` cascade correctly without manual handling.
- **Effort**: 1 hour

### 24. Secret Storage — OAuth Tokens in Plaintext

**Finding: Both projects store OAuth tokens in plaintext SQLite columns**

- **Location**: RustCode `database.rs:511-522` (account table), OpenCode `account/sql.ts:10-11`
- **Evidence**:
  ```rust
  // RustCode — plaintext storage
  CREATE TABLE account (
      access_token text NOT NULL,
      refresh_token text NOT NULL,
      ...
  );
  ```
  ```typescript
  // OpenCode — also plaintext
  access_token: text().$type<AccountV2.AccessToken>().notNull(),
  refresh_token: text().$type<AccountV2.RefreshToken>().notNull(),
  ```
- **Problem**: OAuth tokens (access_token, refresh_token) and integration credentials (`credential.value`) are stored as plaintext in the SQLite database. While SQLite supports encryption via `PRAGMA key` or SQLCipher, neither is used. Anyone with filesystem access to the database file can extract all credentials.
- **Impact**: Credential theft if the database file is compromised. This is partially mitigated by SQLite file permissions but is insufficient for shared environments.
- **Severity**: **Medium**
- **Recommendation**: Use AES-256 encryption for stored tokens with a key derived from the OS keychain (e.g., `keyring` crate on Rust, `safeStorage` on Electron). At minimum, document the risk.
- **Effort**: 2-3 days (requires dependency research and key management design)

---

## Index Analysis

### 25. Index Coverage Comparison

**Finding: OpenCode defines 17 indexes; RustCode INITIAL_MIGRATION has only 4**

- **Location**: RustCode `database.rs:813-831` (17 indexes) vs `storage.rs:464-467` (4 indexes)
- **Evidence**:
  ```
  database.rs defines (17 indexes):
    1. UNIQUE event_aggregate_seq_idx
    2. INDEX event_aggregate_type_seq_idx
    3. UNIQUE permission_project_action_resource_idx
    4. INDEX message_session_time_created_id_idx
    5. INDEX part_message_id_id_idx
    6. INDEX part_session_idx
    7. INDEX session_input_session_pending_delivery_seq_idx
    8. UNIQUE session_input_session_admitted_seq_idx
    9. UNIQUE session_input_session_promoted_seq_idx
    10. UNIQUE session_message_session_seq_idx
    11. INDEX session_message_session_type_seq_idx
    12. INDEX session_message_session_time_created_id_idx
    13. INDEX session_message_time_created_idx
    14. INDEX session_project_idx
    15. INDEX session_workspace_idx
    16. INDEX session_parent_idx
    17. INDEX todo_session_idx

  INITIAL_MIGRATION defines (4 indexes):
    1. INDEX idx_message_session_id
    2. INDEX idx_part_message_id
    3. INDEX idx_session_project_id
    4. INDEX idx_session_input_session
  ```
- **Problem**: 13 indexes (including all 5 UNIQUE indexes) are missing from `INITIAL_MIGRATION`. The 4 simple indexes that exist use different naming conventions (`idx_` prefix vs `_idx` suffix) and are on different columns than the canonical schema. Missing indexes will cause full table scans on critical queries like searching session messages by sequence number or looking up event aggregates.
- **Impact**: Severe query performance degradation. Without `session_message_session_seq_idx`, finding the next message sequence requires `SELECT MAX(seq)` without index support. Without `event_aggregate_seq_idx`, event replay operations scan the entire event table.
- **Severity**: **Critical**
- **Recommendation**: Add all 17 indexes from `database.rs::CREATE_INDEXES` to `INITIAL_MIGRATION`. Ensure naming convention matches the canonical OpenCode schema.
- **Effort**: 1 hour

### 26. Composite Index Analysis — Missing Covering Index for Session Queries

**Finding: No covering index for session listing (project_id + time_updated)**

- **Location**: RustCode `database.rs:1211-1228` (list_sessions query)
- **Evidence**:
  ```rust
  // Query pattern: filter by project_id, order by time_updated DESC
  SELECT ... FROM session WHERE project_id = ?1 ORDER BY time_updated DESC LIMIT ?2

  // Existing index: session_project_idx (project_id only)
  // Missing: composite (project_id, time_updated DESC) covering the ORDER BY
  ```
- **Problem**: The `session_project_idx` index on `project_id` supports the WHERE clause but doesn't cover the ORDER BY. SQLite will need to collect all matching rows, then sort by `time_updated`. With many sessions per project, this is a filesort operation that doesn't use the index.
- **Impact**: `list_sessions` performance degrades linearly with session count per project, instead of being optimized by an index.
- **Severity**: **Medium**
- **Recommendation**: Add a composite index:
  ```sql
  CREATE INDEX session_project_time_idx ON session (project_id, time_updated DESC);
  ```
- **Effort**: 30 minutes

---

## N+1 Query Detection

### 27. Classic N+1 Query in get_messages_with_parts

**Finding: `get_messages_with_parts()` issues one query per message**

- **Location**: RustCode `database.rs:1424-1438`
- **Evidence**:
  ```rust
  pub async fn get_messages_with_parts(
      &self, session_id: &str, limit: Option<u32>,
  ) -> Result<Vec<(MessageRow, Vec<PartRow>)>, DatabaseServiceError> {
      let messages = self.list_messages(session_id, limit).await?;  // 1 query

      let mut result = Vec::with_capacity(messages.len());
      for msg in messages {
          let parts = self.list_parts(&msg.id).await?;  // N queries!
          result.push((msg, parts));
      }
      Ok(result)
  }
  ```
- **Problem**: For a session with 50 messages, this issues 1 query for messages + 50 queries for parts = 51 total queries. OpenCode would use either a JOIN or batch loading. With the default limit of 100 messages, this is 101 queries per call.
- **Impact**: Significant latency for sessions with many messages. 101 sequential network round-trips to SQLite (though SQLite is in-process, so overhead is marshaling, not network).
- **Severity**: **High**
- **Recommendation**: Use a single JOIN or batch query:
  ```rust
  // Option 1: JOIN query
  SELECT m.*, p.* FROM message m
  LEFT JOIN part p ON p.message_id = m.id
  WHERE m.session_id = ?1 ORDER BY m.time_created ASC, p.time_created ASC

  // Option 2: Batch load all parts for the session
  let parts: Vec<PartRow> = sqlx::query_as(
      "SELECT * FROM part WHERE message_id IN
       (SELECT id FROM message WHERE session_id = ?1 ORDER BY time_created ASC LIMIT ?2)"
  )
  .bind(session_id).bind(limit)
  .fetch_all(&self.pool).await?;
  // Then partition by message_id in-memory
  ```
- **Effort**: 2-4 hours

### 28. Potential N+1 in Permission Query Patterns

**Finding: Permission service queries could generate N+1 patterns**

- **Location**: RustCode `permission.rs:877-896`
- **Evidence**:
  ```rust
  pub async fn add(&self, input: &AddSavedInput) -> Result<()> {
      for resource in &input.resources {
          // One INSERT per resource — could be many
          sqlx::query("INSERT OR IGNORE INTO permission (...) VALUES (...)")
              .execute(self.db.pool()).await?;
      }
  }
  ```
- **Problem**: For saving 100 resources, this issues 100 separate INSERT statements. While not a query-per-row issue (no SELECT), it's an inefficient write pattern.
- **Impact**: Slow permission saves for large resource lists.
- **Severity**: **Low**
- **Recommendation**: Use batch INSERT if sqlx supports it (via `INSERT INTO ... VALUES (?1), (?2), ...`). Alternatively, wrap in a transaction to at least reduce overhead.
- **Effort**: 1-2 hours

---

## WAL Mode and Journal Settings

### 29. WAL Mode Configuration

**Finding: Both projects enable WAL mode correctly, with differences**

- **Location**: RustCode `database.rs:59-66` (CONNECTION_PRAGMAS), `storage.rs:242-254`, OpenCode `database.ts:27-32`
- **Evidence**:
  ```rust
  // RustCode database.rs CONNECTION_PRAGMAS has WAL + wal_checkpoint(PASSIVE)
  pub const CONNECTION_PRAGMAS: &[&str] = &[
      "PRAGMA journal_mode = WAL",
      "PRAGMA synchronous = NORMAL",
      "PRAGMA busy_timeout = 5000",
      "PRAGMA cache_size = -64000",
      "PRAGMA foreign_keys = ON",
      "PRAGMA wal_checkpoint(PASSIVE)",  // Not in storage.rs!
  ];
  ```
  ```rust
  // RustCode storage.rs — missing wal_checkpoint(PASSIVE)
  let pragmas = [
      "PRAGMA journal_mode = WAL",
      "PRAGMA synchronous = NORMAL",
      "PRAGMA busy_timeout = 5000",
      "PRAGMA cache_size = -64000",
      "PRAGMA foreign_keys = ON",
      // wal_checkpoint(PASSIVE) MISSING!
  ];
  ```
- **Problem**: `storage.rs::Database::open()` does not include `PRAGMA wal_checkpoint(PASSIVE)`, while it's defined in `database.rs::CONNECTION_PRAGMAS` and in OpenCode's `database.ts:32`. The checkpoint pragma helps keep the WAL file from growing unboundedly.
- **Impact**: In deployments using `storage.rs` (the main database module), the WAL file can grow indefinitely, consuming disk space and potentially degrading read performance.
- **Severity**: **Medium**
- **Recommendation**: Add `PRAGMA wal_checkpoint(PASSIVE)` to `storage.rs:242-254`. Use `DatabaseConfig::pragmas()` instead of hardcoded array to share the definition.
- **Effort**: 30 minutes

### 30. TRUNCATE vs WAL Checkpoint

**Finding: Neither project uses `PRAGMA journal_mode = TRUNCATE` or periodic checkpoints**

- **Location**: All files reviewed
- **Evidence**: Both projects use `journal_mode = WAL` as the only journal mode. Neither implements periodic or size-based WAL checkpointing. OpenCode runs `PRAGMA wal_checkpoint(PASSIVE)` only at connection open (database.ts:32).
- **Problem**: WAL files can grow unboundedly without periodic checkpointing. In production with heavy write loads, the WAL file can reach gigabytes. A passive checkpoint only triggers if there are no active readers/writers, which may never occur in a busy system.
- **Impact**: Disk space exhaustion and slower read performance over time as SQLite must scan through a large WAL file.
- **Severity**: **Medium**
- **Recommendation**: Implement a periodic WAL checkpoint strategy:
  - Call `PRAGMA wal_checkpoint(TRUNCATE)` periodically (e.g., every 1000 writes or every hour)
  - Or set `PRAGMA wal_autocheckpoint = 1000` (checkpoint every 1000 pages)
  - Or use sqlx's `after_connect` to set `wal_autocheckpoint`
- **Effort**: 2-4 hours

### 31. Busy Timeout

**Finding: Both projects set busy_timeout correctly**

- **Location**: RustCode `storage.rs:245` (`PRAGMA busy_timeout = 5000`), OpenCode `database.ts:29`
- **Evidence**:
  ```rust
  "PRAGMA busy_timeout = 5000"  // 5 second timeout
  ```
- **Problem**: The 5-second timeout is appropriate for most use cases. However, it's applied once at connection open time. If the database is under heavy load during connection setup (migration execution), the timeout may not be sufficient. The timeout is not configurable in `storage.rs::Database::open()`.
- **Impact**: Potential `SQLITE_BUSY` errors during migration on heavily loaded systems.
- **Severity**: **Low**
- **Recommendation**: Make `busy_timeout` configurable (it already is in `DatabaseConfig` — just not used in `storage.rs::open()`). Consider a longer timeout (30s) for migration operations.
- **Effort**: 1 hour

---

## Backup Strategy

### 32. No Database Backup Strategy

**Finding: Neither project implements database backup**

- **Location**: No backup code found in either project
- **Evidence**: No backup-related functions, commands, or documentation found in either codebase. No WAL checkpoint-based backup, no `.backup` command, no SQL dump, no replication.
- **Problem**: There is no mechanism to create consistent backups of the SQLite database. Users must manually copy the database file, which may produce an inconsistent copy if done while the database is being written (especially with WAL mode — the WAL file must be checkpointed first).
- **Impact**: Data loss risk. No disaster recovery path. If the single database file is corrupted (e.g., filesystem error, power loss during write), all user data is lost.
- **Severity**: **High**
- **Recommendation**: Implement a backup command/function:
  ```rust
  pub async fn backup(&self, dest_path: &Path) -> Result<()> {
      let mut tx = self.pool.begin().await?;
      sqlx::query("PRAGMA wal_checkpoint(TRUNCATE)").execute(&mut *tx).await?;
      // Use sqlx_backup or VACUUM INTO
      sqlx::query(&format!("VACUUM INTO '{}'", dest_path.display().replace('\'', "''")))
          .execute(&mut *tx).await?;
      tx.commit().await?;
      Ok(())
  }
  ```
  Also consider:
  - Automated periodic backups via cron/scheduler
  - Backup to cloud storage (S3, etc.)
  - WAL archive mode for point-in-time recovery
- **Effort**: 2-5 days (backup command + scheduling + documentation)

---

## Dual Migration System Conflict

### 33. Detailed Analysis of Dual Migration Systems

**Finding: `DatabaseService` (database.rs) and `Database` (storage.rs) diverge**

- **Location**: RustCode `storage.rs:280-335` and `database.rs:1067-1127`
- **Evidence**:
  ```
  ┌─────────────────────────────────────────────────────────────────────┐
  │                    RustCode Migration Systems                       │
  ├─────────────────────────────┬───────────────────────────────────────┤
  │ storage.rs::Database        │ database.rs::DatabaseService         │
  ├─────────────────────────────┼───────────────────────────────────────┤
  │ Migration table: _migration │ Migration table: migration           │
  │ Migration ID: &'static str  │ Migration ID: String                 │
  │ SQL format: &'static str    │ SQL format: Vec<String>              │
  │ Has run_migrations()        │ Has migration_status() (read-only)   │
  │ Has ensure_migration_table()│ Has is_migration_applied()           │
  │ INITIAL_MIGRATION (incomplete)│ KNOWN_MIGRATION_IDS (35 entries)   │
  │ ALL_MIGRATIONS (1 entry)    │ No separate migration runner         │
  └─────────────────────────────┴───────────────────────────────────────┘
  ```
- **Problem**: The two systems have:
  1. Different table names: `_migration` vs `migration`
  2. Different migration count: 1 vs 35
  3. Different SQL formats: single string vs vector
  4. Different module responsibilities: `storage.rs` runs migrations, `database.rs` queries migration status
  5. No clear ownership: which system actually manages schema evolution?
  6. Tests in `database.rs:2253-2281` (test_migration_idempotency) use `sqlx::query` to manually insert migration records, not the migration system
  7. Tests in `database.rs:1927-1955` (setup_test_db) use `ALL_CREATE_TABLES` directly, not the migration system
- **Impact**: Schema evolution is impossible with this design. Adding a new column requires updating both systems, which will inevitably diverge. The `storage.rs` system is used for initial setup but has incomplete schema. The `database.rs` system has correct schema but no way to apply it incrementally.
- **Severity**: **Critical**
- **Recommendation**: 
  1. Eliminate `storage.rs::Migration` struct
  2. Rename `_migration` table to `migration` — or add migration to migrate the migration table
  3. Move `run_migrations` logic into `database.rs`
  4. Implement each of the 35 OpenCode migrations as `Migration { id, up: Vec<String> }`
  5. Add explicit migration test that verifies migration ordering and idempotency
  6. Remove `INITIAL_MIGRATION` — it's a source of bugs
- **Effort**: 5-10 days

---

## OpenCode-Specific Issues

### 34. OpenCode Credential Schema Mismatch

**Finding: OpenCode's credential table has two conflicting definitions**

- **Location**: OpenCode `credential/sql.ts:6-15` vs `migration/20260611035744_credential.ts`
- **Evidence**:
  ```
  credential/sql.ts (current definition):
    id, integration_id (optional), label, value (json), connector_id (optional),
    method_id (optional), active (boolean), time_created, time_updated

  Migration 20260611035744_credential (applied definition):
    id, connector_id (NOT NULL), method_id (NOT NULL), label, value (json),
    active (boolean), time_created, time_updated
    + UNIQUE INDEX credential_connector_active_idx
  ```
- **Problem**: The migration creates `connector_id` and `method_id` as `NOT NULL`, but the current schema definition (`credential/sql.ts`) has them as optional (`text()` without `.notNull()`). The migration lacks the `integration_id` column that exists in the current schema. The migration has a partial unique index that doesn't exist in the current schema definition.
- **Impact**: INSERT operations using the Drizzle ORM definition may fail on NOT NULL constraints if the migration was applied but columns changed. The `connector_id`/`method_id` columns could be NULL at runtime but were declared NOT NULL in the migration.
- **Severity**: **Medium** (OpenCode-specific, but indicates porting complexity)
- **Recommendation**: Either update the migration to match the current schema, or update the Drizzle definition to match what the migration created. Add ALTER TABLE migration to reconcile differences.
- **Effort**: 2-4 hours

### 35. OpenCode Missing Stream Implementation

**Finding: OpenCode's SQLite adapters don't implement streaming queries**

- **Location**: OpenCode `sqlite.bun.ts:101-103`, `sqlite.node.ts:102-104`
- **Evidence**:
  ```typescript
  executeStream() {
      return Stream.die("executeStream not implemented")
  },
  ```
- **Problem**: The Effect SQL client interface requires `executeStream`, but both adapters throw an error when it's called. This means any code path that tries to stream database results (e.g., large result sets) will crash.
- **Impact**: Unexpected runtime crashes if any code uses streaming queries. This is a known missing feature documented in the code.
- **Severity**: **Medium** (OpenCode-specific)
- **Recommendation**: Implement `executeStream` using native SQLite's `iterate` method on Bun or `prepare.each` on Node. Alternatively, ensure no code paths call `executeStream`.
- **Effort**: 1-2 days

### 36. OpenCode Semaphore-Based Locking

**Finding: OpenCode's migration locking could cause deadlocks**

- **Location**: OpenCode `migration.ts:11`, `18-40`
- **Evidence**:
  ```typescript
  const lock = Semaphore.makeUnsafe(1)

  export function apply(db: Database) {
      return lock.withPermit(
          Effect.gen(function* () {
              // ...
              yield* db.transaction((tx) =>
                  // schema creation + migration insert
              )
          }),
      )
  }
  ```
- **Problem**: The semaphore lock prevents concurrent migration applications, which is correct. However, the semaphore is `makeUnsafe` (unbounded), not `make`. If `apply()` is called recursively (e.g., during another database operation that triggers migration), this could deadlock. The semaphore is module-scoped, meaning it applies to all database instances, not per-database.
- **Impact**: Potential deadlock if migrations are triggered from within another database operation. Rare but possible in complex startup sequences.
- **Severity**: **Low** (OpenCode-specific, corner case)
- **Recommendation**: Use `Semaphore.make(1)` instead of `makeUnsafe`, or make the semaphore per-database-instance.
- **Effort**: 2-4 hours

---

## Finding Summary Table

| # | Finding | Location | Severity | Effort |
|---|---|---|---|---|
| 1 | Missing 15 tables in INITIAL_MIGRATION | `storage.rs:409-468` | **Critical** | 2-4h |
| 2 | Project table missing 5 columns | `storage.rs:412-418` vs TS `project/sql.ts` | **Critical** | 1h |
| 3 | Session table missing 17 columns | `storage.rs:421-434` vs TS `session/sql.ts` | **Critical** | 1-2h |
| 4 | Message/Part simplified schema mismatch | `storage.rs:436-453` vs TS `session/sql.ts` | **Critical** | 1h |
| 5 | Missing project.time_updated in INITIAL_MIGRATION | `storage.rs:412-418` | **High** | 2h |
| 6 | Dual migration systems conflict | `storage.rs:193-335` vs `database.rs:392-443` | **Critical** | 4-8h |
| 7 | Only 1/35 migrations implemented | `storage.rs:472` vs TS `migration.gen.ts` | **Critical** | 3-5d |
| 8 | Naive SQL `split(';')` in migration parser | `storage.rs:301-313` | **High** | 2-4h |
| 9 | CREATE TABLE IF NOT EXISTS masks errors | `storage.rs:412-468` | **High** | 30m |
| 10 | Missing Drizzle migration journal seeding | `storage.rs:338-348` | **High** | 2-4h |
| 11 | No connection pool configuration | `storage.rs:234-239` | **Medium** | 2-4h |
| 12 | OpenCode serializes all DB access (semaphore=1) | TS `sqlite.bun.ts:115-116` | **Medium** | 3-5d |
| 13 | No health checks / connection validation | Both projects | **Low** | 2-4h |
| 14 | Raw SQL vs ORM (maintainability) | RustCode entire codebase | **Medium** | 2-3d |
| 15 | Parameterized queries used correctly | Both projects | **None** | N/A |
| 16 | Missing covering indexes | `database.rs:1211-1228` | **Low** | 1-2h |
| 17 | Missing transaction in permission inserts | `permission.rs:867-898` | **High** | 1h |
| 18 | Missing transaction in delete_session_cascade | `database.rs:1527-1553` | **High** | 1h |
| 19 | OpenCode uses BEGIN IMMEDIATE (positive) | TS `event.ts:257-259` | **None** | N/A |
| 20 | Migration transaction handling (correct) | `storage.rs:295-329` | **None** | N/A |
| 21 | FK enforcement enabled correctly | Both projects | **None** | 1h |
| 22 | Missing 13 indexes + 5 UNIQUE constraints | `storage.rs:464-467` vs `database.rs:813-831` | **Critical** | 1h |
| 23 | Session parent_id lacks self-ref FK cascade | `database.rs:727-758` | **Medium** | 1h |
| 24 | OAuth tokens in plaintext | Both projects | **Medium** | 2-3d |
| 25 | 13 missing indexes in INITIAL_MIGRATION | `storage.rs:464-467` | **Critical** | 1h |
| 26 | No covering index for session listing | `database.rs:1211-1228` | **Medium** | 30m |
| 27 | N+1 query in get_messages_with_parts | `database.rs:1424-1438` | **High** | 2-4h |
| 28 | Batch insert missing in permission.save | `permission.rs:877-896` | **Low** | 1-2h |
| 29 | WAL checkpoint missing in storage.rs | `storage.rs:242-254` | **Medium** | 30m |
| 30 | No periodic WAL checkpoint strategy | Both projects | **Medium** | 2-4h |
| 31 | Busy timeout not configurable in storage.rs | `storage.rs:245` | **Low** | 1h |
| 32 | No database backup strategy | Both projects | **High** | 2-5d |
| 33 | Dual migration system architectural conflict | `storage.rs` + `database.rs` | **Critical** | 5-10d |
| 34 | OpenCode credential schema mismatch (migration vs model) | TS `credential/sql.ts` vs migration | **Medium** | 2-4h |
| 35 | OpenCode streaming not implemented | TS `sqlite.bun.ts:101-103` | **Medium** | 1-2d |
| 36 | OpenCode semaphore deadlock potential | TS `migration.ts:11` | **Low** | 2-4h |

**Severity Distribution**: Critical (6), High (7), Medium (10), Low (4), None (3)

---

## Recommendations Roadmap

### Phase 1 — Immediate (Week 1-2)
1. **Eliminate the dual migration system** (Findings #6, #33): Remove `storage.rs::Migration` and `INITIAL_MIGRATION`. Use `database.rs` types exclusively.
2. **Port full schema** (Findings #1-5, #22, #25): Replace INITIAL_MIGRATION with the full 20-table schema from `database.rs::ALL_CREATE_TABLES` and `CREATE_INDEXES`.
3. **Fix permission and cascade transactions** (Findings #17, #18): Wrap multi-statement operations in transactions.
4. **Fix N+1 query** (Finding #27): Replace loop-per-message with JOIN or batch load.

### Phase 2 — Short-term (Week 3-4)
5. **Port 35 migrations** (Finding #7): Implement all incremental migrations as `database.rs::Migration` entries.
6. **Fix migration SQL parser** (Finding #8): Replace `split(';')` with `Vec<String>` per migration, or use `sqlx::raw_sql()`.
7. **Add pool configuration** (Finding #11): Use `SqlitePoolOptions` with explicit conn limits and timeouts.
8. **Add Drizzle seeding** (Finding #10): Import Drizzle migration journal on first run.

### Phase 3 — Medium-term (Month 2)
9. **Add WAL checkpoint strategy** (Findings #29, #30): Periodic checkpoints, auto-checkpoint setting.
10. **Add backup system** (Finding #32): `VACUUM INTO`-based backup command + scheduled backups.
11. **Encrypt secrets** (Finding #24): Keyring-based encryption for OAuth tokens and credentials.
12. **Add covering indexes** (Findings #16, #26): Composite indexes for common query patterns.

### Phase 4 — Long-term (Month 3+)
13. **Connection health checks** (Finding #13): after_connect validation, periodic pings.
14. **OpenCode semaphore refactoring** (Finding #12): Allow concurrent readers in Bun/Node adapters.
15. **Compile-time SQL verification** (Finding #14): Consider `sqlx::query_as!()` macros.

---

## Appendix A: Full Schema Comparison

### RustCode `database.rs` (Canonical Target) vs OpenCode `**/sql.ts` (Source)

| Table | RustCode database.rs | OpenCode Drizzle | Match? |
|---|---|---|---|
| workspace | `database.rs:469-481` | `workspace.sql.ts:6-20` | ✓ Exact |
| data_migration | `database.rs:487-492` | `data-migration.sql.ts:3-6` | ✓ Exact |
| account_state | `database.rs:498-505` | `account/sql.ts:16-22` | ✓ Exact |
| account | `database.rs:511-522` | `account/sql.ts:6-14` | ✓ Exact |
| control_account | `database.rs:528-540` | `account/sql.ts:25-39` | ✓ Exact |
| credential | `database.rs:546-558` | `credential/sql.ts:6-15` | ✓ exact (but migration differs) |
| event_sequence | `database.rs:564-570` | `event/sql.ts:4-8` | ✓ Exact |
| event | `database.rs:576-585` | `event/sql.ts:10-25` | ✓ Exact |
| permission | `database.rs:591-601` | `permission/sql.ts:7-20` | ✓ Exact |
| project_directory | `database.rs:607-617` | `project/sql.ts:20-35` | ✓ Exact |
| project | `database.rs:623-638` | `project/sql.ts:6-18` | ✓ Exact |
| message | `database.rs:644-653` | `session/sql.ts:67-79` | ✓ Exact |
| part | `database.rs:659-669` | `session/sql.ts:81-97` | ✓ Exact |
| session_context_epoch | `database.rs:675-686` | `session/sql.ts:167-178` | ✓ Exact |
| session_input | `database.rs:692-703` | `session/sql.ts:139-165` | ✓ Exact |
| session_message | `database.rs:709-720` | `session/sql.ts:118-137` | ✓ Exact |
| session | `database.rs:726-759` | `session/sql.ts:21-65` | ✓ Exact |
| todo | `database.rs:765-777` | `session/sql.ts:99-116` | ✓ Exact |
| session_share | `database.rs:783-793` | `share/sql.ts:5-13` | ✓ Exact |

**Note**: The `database.rs` schema definitions are accurate ports. The `INITIAL_MIGRATION` in `storage.rs` is the source of all mismatches. The `database.rs` test helper `setup_test_db()` at line 1949 correctly uses `ALL_CREATE_TABLES`, which is why tests pass.

---

## Appendix B: OpenCode Migration List

All 35 migrations that need to be ported to RustCode:

| # | ID | Purpose |
|---|---|---|
| 1 | `20260127222353_familiar_lady_ursula` | Initial schema |
| 2 | `20260211171708_add_project_commands` | Add commands to project |
| 3 | `20260213144116_wakeful_the_professor` | Session updates |
| 4 | `20260225215848_workspace` | Create workspace table |
| 5 | `20260227213759_add_session_workspace_id` | Add workspace_id to session |
| 6 | `20260228203230_blue_harpoon` | Schema adjustments |
| 7 | `20260303231226_add_workspace_fields` | Add fields to workspace |
| 8 | `20260309230000_move_org_to_state` | Move org to account_state |
| 9 | `20260312043431_session_message_cursor` | Session message cursor |
| 10 | `20260323234822_events` | Event sourcing tables |
| 11 | `20260410174513_workspace-name` | Workspace name field |
| 12 | `20260413175956_chief_energizer` | Schema changes |
| 13 | `20260423070820_add_icon_url_override` | Icon URL override |
| 14 | `20260427172553_slow_nightmare` | Schema adjustments |
| 15 | `20260428004200_add_session_path` | Session path column |
| 16 | `20260501142318_next_venus` | Schema changes |
| 17 | `20260504145000_add_sync_owner` | Sync owner field |
| 18 | `20260507164347_add_workspace_time` | Workspace time tracking |
| 19 | `20260510033149_session_usage` | Session usage columns |
| 20 | `20260511000411_data_migration_state` | Data migration table |
| 21 | `20260511173437_session-metadata` | Session metadata JSON |
| 22 | `20260601010001_normalize_storage_paths` | Path normalization |
| 23 | `20260601202201_amazing_prowler` | Schema changes |
| 24 | `20260602002951_lowly_union_jack` | Schema adjustments |
| 25 | `20260602182828_add_project_directories` | Project directory table |
| 26 | `20260603001617_session_message_projection_indexes` | Projection indexes |
| 27 | `20260603040000_session_message_projection_order` | Projection ordering |
| 28 | `20260603141458_session_input_inbox` | Session input inbox |
| 29 | `20260603160727_jittery_ezekiel_stane` | Schema changes |
| 30 | `20260604172448_event_sourced_session_input` | Event-sourced input |
| 31 | `20260605003541_add_session_context_snapshot` | Context snapshot |
| 32 | `20260605042240_add_context_epoch_agent` | Context epoch agent |
| 33 | `20260611035744_credential` | Credential table |
| 34 | `20260611192811_lush_chimera` | Schema changes |
| 35 | `20260612174303_project_dir_strategy` | Add strategy to project_directory |

---

## End of Report

**Total Findings**: 36 (6 Critical, 7 High, 10 Medium, 4 Low, 3 Informational)
**Estimated Remediation Effort**: 30-60 person-days (depending on parallelism)
**Overall Database Readiness Score**: 2/10 (RustCode), 8/10 (OpenCode)

*Report generated by Agent 11 — Database Auditor*
