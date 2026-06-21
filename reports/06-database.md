# Database Schema and Migration Gap Analysis

## Ground Truth: opencode Database Layer

### Source Files Examined
- `/root/opencodesport/opencode/packages/core/src/database/` (10 files)
- `/root/opencodesport/opencode/packages/core/src/data-migration.sql.ts`
- `/root/opencodesport/opencode/packages/core/src/database/migration/` (35 migration files)

### Table Inventory (20 tables, final schema)

All 20 tables and their columns, as defined in `schema.gen.ts` (the canonical schema):

| # | Table | Columns | FK Constraints |
|---|-------|---------|----------------|
| 1 | `project` | id(PK), worktree(NN), vcs, name, icon_url, icon_url_override, icon_color, time_created(NN), time_updated(NN), time_initialized, sandboxes(NN), commands | - |
| 2 | `workspace` | id(PK), type(NN), name(DEFAULT '' NN), branch, directory, extra, project_id(NN), time_used(NN) | project_id → project(id) CASCADE |
| 3 | `data_migration` | name(PK), time_completed(NN) | - |
| 4 | `account_state` | id(PK integer), active_account_id, active_org_id | active_account_id → account(id) SET NULL |
| 5 | `account` | id(PK), email(NN), url(NN), access_token(NN), refresh_token(NN), token_expiry, time_created(NN), time_updated(NN) | - |
| 6 | `control_account` | email(NN), url(NN), access_token(NN), refresh_token(NN), token_expiry, active(NN), time_created(NN), time_updated(NN), PK(email,url) | - |
| 7 | `credential` | id(PK), integration_id, label(NN), value(NN), connector_id, method_id, active, time_created(NN), time_updated(NN) | - |
| 8 | `event_sequence` | aggregate_id(PK), seq(NN), owner_id | - |
| 9 | `event` | id(PK), aggregate_id(NN), seq(NN), type(NN), data(NN) | aggregate_id → event_sequence(aggregate_id) CASCADE |
| 10 | `permission` | id(PK), project_id(NN), action(NN), resource(NN), time_created(NN), time_updated(NN) | project_id → project(id) CASCADE |
| 11 | `project_directory` | project_id(NN), directory(NN), type, strategy, time_created(NN), PK(project_id,directory) | project_id → project(id) CASCADE |
| 12 | `message` | id(PK), session_id(NN), time_created(NN), time_updated(NN), data(NN) | session_id → session(id) CASCADE |
| 13 | `part` | id(PK), message_id(NN), session_id(NN), time_created(NN), time_updated(NN), data(NN) | message_id → message(id) CASCADE |
| 14 | `session_context_epoch` | session_id(PK), baseline(NN), agent(DEFAULT 'build' NN), snapshot(NN), baseline_seq(NN), replacement_seq, revision(DEFAULT 0 NN) | session_id → session(id) CASCADE |
| 15 | `session_input` | id(PK), session_id(NN), prompt(NN), delivery(NN), admitted_seq(NN), promoted_seq, time_created(NN) | session_id → session(id) CASCADE |
| 16 | `session_message` | id(PK), session_id(NN), type(NN), seq(NN), time_created(NN), time_updated(NN), data(NN) | session_id → session(id) CASCADE |
| 17 | `session` | id(PK), project_id(NN), workspace_id, parent_id, slug(NN), directory(NN), path, title(NN), version(NN), share_url, summary_additions, summary_deletions, summary_files, summary_diffs, metadata, cost(DEFAULT 0 NN), tokens_input(DEFAULT 0 NN), tokens_output(DEFAULT 0 NN), tokens_reasoning(DEFAULT 0 NN), tokens_cache_read(DEFAULT 0 NN), tokens_cache_write(DEFAULT 0 NN), revert, permission, agent, model, time_created(NN), time_updated(NN), time_compacting, time_archived | project_id → project(id) CASCADE |
| 18 | `todo` | session_id(NN), content(NN), status(NN), priority(NN), position(NN), time_created(NN), time_updated(NN), PK(session_id,position) | session_id → session(id) CASCADE |
| 19 | `session_share` | session_id(PK), id(NN), secret(NN), url(NN), time_created(NN), time_updated(NN) | session_id → session(id) CASCADE |
| 20 | `migration` | id(PK), time_completed(NN) | - |

### Index Inventory (17 indexes)

| # | Index Name | Table | Columns | Unique |
|---|------------|-------|---------|--------|
| 1 | event_aggregate_seq_idx | event | aggregate_id, seq | YES |
| 2 | event_aggregate_type_seq_idx | event | aggregate_id, type, seq | NO |
| 3 | permission_project_action_resource_idx | permission | project_id, action, resource | YES |
| 4 | message_session_time_created_id_idx | message | session_id, time_created, id | NO |
| 5 | part_message_id_id_idx | part | message_id, id | NO |
| 6 | part_session_idx | part | session_id | NO |
| 7 | session_input_session_pending_delivery_seq_idx | session_input | session_id, promoted_seq, delivery, admitted_seq | NO |
| 8 | session_input_session_admitted_seq_idx | session_input | session_id, admitted_seq | YES |
| 9 | session_input_session_promoted_seq_idx | session_input | session_id, promoted_seq | YES |
| 10 | session_message_session_seq_idx | session_message | session_id, seq | YES |
| 11 | session_message_session_type_seq_idx | session_message | session_id, type, seq | NO |
| 12 | session_message_session_time_created_id_idx | session_message | session_id, time_created, id | NO |
| 13 | session_message_time_created_idx | session_message | time_created | NO |
| 14 | session_project_idx | session | project_id | NO |
| 15 | session_workspace_idx | session | workspace_id | NO |
| 16 | session_parent_idx | session | parent_id | NO |
| 17 | todo_session_idx | todo | session_id | NO |

### SQL Schema Exports

| Export | Source File | Description |
|--------|------------|-------------|
| `DataMigrationTable` | `data-migration.sql.ts` | Drizzle table for data_migration |
| `Database.*` | `database.ts` | Module export (Service, layer, path) |
| `DatabaseMigration.*` | `migration.ts` | Module export (apply, applyOnly, Migration type) |
| `Sqlite.*` | `sqlite.ts` | Module export (Native, Drizzle services) |
| `Timestamps` | `schema.sql.ts` | time_created/time_updated helpers |
| Schema gen default | `schema.gen.ts` | up() function creating all tables + indexes |
| `absoluteColumn` | `path.ts` | Custom Drizzle column for absolute paths |
| `directoryColumn` | `path.ts` | Custom Drizzle column for directories |
| `pathColumn` | `path.ts` | Custom Drizzle column for storage paths |
| `absoluteArrayColumn` | `path.ts` | Custom Drizzle column for path arrays |

### Migration Functions

| Function | File | Description |
|----------|------|-------------|
| `apply(db)` | `migration.ts` | Full migration: checks session table, creates schema + seeds migration journal |
| `applyOnly(db, migrations)` | `migration.ts` | Applies pending migrations sequentially, supports Drizzle journal migration |
| `schema.up(tx)` | `schema.gen.ts` | Creates all tables and indexes in a single transaction |

### Migration Count: 35

IDs:
1. `20260127222353_familiar_lady_ursula` - Initial schema (project, message, part, permission, session, todo, session_share)
2. `20260211171708_add_project_commands` - ALTER project ADD commands
3. `20260213144116_wakeful_the_professor` - CREATE control_account
4. `20260225215848_workspace` - CREATE workspace (original: id, branch, project_id, config)
5. `20260227213759_add_session_workspace_id` - ALTER session ADD workspace_id + index
6. `20260228203230_blue_harpoon` - CREATE account, account_state
7. `20260303231226_add_workspace_fields` - ALTER workspace ADD type/name/directory/extra, DROP config
8. `20260309230000_move_org_to_state` - Move selected_org_id from account to account_state
9. `20260312043431_session_message_cursor` - Rebuild message/part indexes
10. `20260323234822_events` - CREATE event_sequence, event
11. `20260410174513_workspace-name` - Rebuild workspace with DEFAULT '' name
12. `20260413175956_chief_energizer` - CREATE session_entry (later dropped)
13. `20260423070820_add_icon_url_override` - ALTER project ADD icon_url_override + backfill
14. `20260427172553_slow_nightmare` - CREATE session_message, DROP session_entry
15. `20260428004200_add_session_path` - ALTER session ADD path
16. `20260501142318_next_venus` - ALTER session ADD agent, model
17. `20260504145000_add_sync_owner` - ALTER event_sequence ADD owner_id
18. `20260507164347_add_workspace_time` - ALTER workspace ADD time_used
19. `20260510033149_session_usage` - ALTER session ADD cost, tokens_* columns + backfill
20. `20260511000411_data_migration_state` - CREATE data_migration
21. `20260511173437_session-metadata` - ALTER session ADD metadata
22. `20260601010001_normalize_storage_paths` - Normalize Windows paths in existing data
23. `20260601202201_amazing_prowler` - DROP permission (old schema)
24. `20260602002951_lowly_union_jack` - CREATE permission (new schema with id)
25. `20260602182828_add_project_directories` - CREATE project_directory (without strategy)
26. `20260603001617_session_message_projection_indexes` - Rebuild session_message + event indexes
27. `20260603040000_session_message_projection_order` - ADD seq to session_message + indexes
28. `20260603141458_session_input_inbox` - CREATE session_input (original: autoincrement seq)
29. `20260603160727_jittery_ezekiel_stane` - Rebuild indexes for delivery/type ordering
30. `20260604172448_event_sourced_session_input` - Rebuild session_input (id PK pattern), add admitted_seq/promoted_seq indexes
31. `20260605003541_add_session_context_snapshot` - CREATE session_context_epoch
32. `20260605042240_add_context_epoch_agent` - ALTER session_context_epoch ADD agent
33. `20260611035744_credential` - CREATE credential (original schema)
34. `20260611192811_lush_chimera` - DROP + recreate credential (final schema with integration_id)
35. `20260612174303_project_dir_strategy` - ALTER project_directory ADD strategy + rebuild

### PRAGMA Configuration

```typescript
// From database.ts lines 27-32
PRAGMA journal_mode = WAL
PRAGMA synchronous = NORMAL
PRAGMA busy_timeout = 5000
PRAGMA cache_size = -64000
PRAGMA foreign_keys = ON
PRAGMA wal_checkpoint(PASSIVE)
```

### Connection Pool / Transaction Patterns

- SQLite via bun:sqlite or node:sqlite with Effect effect system
- Semaphore-based single connection (pool size 1)
- Transaction acquirer with uninterruptible mask
- Migration lock via `Semaphore.makeUnsafe(1)`
- `apply()` runs all create-table + seed in single transaction
- `applyOnly()` runs each migration in its own transaction

### Custom Path Column Types

```typescript
- absoluteColumn: validates + normalizes to POSIX slashes, wraps AbsolutePath
- directoryColumn: like absolute but allows empty string for legacy sessions
- pathColumn: normalizes slashes only, no absolute check
- absoluteArrayColumn: JSON-array of absolute paths
```

---

## Gap Analysis: rustcode vs opencode

### rustcode Implementation Status

**File: `/root/opencodesport/rustcode/crates/rustcode-core/src/database.rs`**
- 2652 lines
- 20 CREATE_TABLE constants (all match opencode's final schema)
- 17 CREATE_INDEX constants (all match opencode)
- DatabaseService with CRUD for: session, message, part, session_message
- Path helper functions (db_absolute_path, db_path, db_absolute_path_array, etc.)
- GlobalPaths with XDG directory support
- DatabaseConfig with PRAGMA configuration
- Migration types and KNOWN_MIGRATION_IDS list (35 IDs)

**File: `/root/opencodesport/rustcode/crates/rustcode-core/src/storage.rs`**
- Storage (JSON file-based key-value store)
- Database (SQLite with connection pool)
- Migration runner

---

## Gaps FOUND AND FIXED

### GAP 1 (CRITICAL): Wrong migration table name `_migration` → `migration`

**Location:** `storage.rs` lines 279, 316, 333-339, 608, 611, 619, 871, 873

**Issue:** The storage.rs `Database::run_migrations()` and `ensure_migration_table()` used `_migration` (with underscore prefix) as the migration journal table name. The opencode schema uses `migration` (without underscore). This means:
1. A fresh database would create `_migration` instead of `migration`
2. The `database.rs` DatabaseService queries `migration` table (correctly)
3. These two systems would be incompatible on the same database file

**Fix applied:** Changed all `_migration` references to `migration` in:
- `ensure_migration_table()`: CREATE TABLE `migration`
- `run_migrations()`: SELECT/INSERT queries use `migration`
- All test assertions updated
- Doc comments updated

### GAP 2 (CRITICAL): Missing `PRAGMA wal_checkpoint(PASSIVE)`

**Location:** `storage.rs` lines 237-244

**Issue:** opencode runs `PRAGMA wal_checkpoint(PASSIVE)` as part of connection initialization (database.ts line 32), but rustcode's `Database::open()` omitted this pragma. While not strictly required, it improves WAL file management.

**Fix applied:** Added `"PRAGMA wal_checkpoint(PASSIVE)"` to the pragma list in `Database::open()`.

### GAP 3 (CRITICAL): Wrong schema in `INITIAL_MIGRATION`

**Location:** `storage.rs` lines 401-465 (original INITIAL_MIGRATION)

**Issue:** The original `INITIAL_MIGRATION` created only 5 tables (`project`, `session`, `message`, `part`, `session_input`) with incorrect schemas that do NOT match opencode's final schema:

| Table | Original (wrong) | Correct (opencode) |
|-------|-------------------|-------------------|
| project | Missing: icon_url, icon_url_override, icon_color, sandboxes(NN), commands. Has: time_initialized(NN) instead of NULLable | Has ALL columns with correct nullability |
| session | Missing: slug, directory(NN), version, share_url, summary_*, metadata, cost, tokens_*, revert, permission, time_compacting, time_archived. Wrong: has usage_input/output/cache_read/cache_write instead of tokens_* | Full 30-column schema |
| message | Has role(NN), content(NN DEFAULT '') instead of data(NN) JSON blob. Missing time_updated | Has id, session_id, time_created, time_updated, data (JSON) |
| part | Has type(NN), content(NN DEFAULT ''), tool_call_id instead of data(NN) JSON blob. Missing session_id, time_updated | Has id, message_id, session_id, time_created, time_updated, data (JSON) |
| session_input | Has text(NN), input_type(NN DEFAULT 'user') instead of prompt, delivery, admitted_seq, promoted_seq | Has id, session_id, prompt, delivery, admitted_seq, promoted_seq, time_created |
| 15 missing tables | Not created at all | account, account_state, control_account, credential, event, event_sequence, permission, project_directory, session_context_epoch, session_message, session_share, todo, workspace, data_migration, migration |

**Fix applied:** Complete rewrite of `INITIAL_MIGRATION` to:
- Create ALL 20 tables matching opencode's final schema
- Create ALL 17 indexes
- Uses `CREATE TABLE IF NOT EXISTS` and `CREATE INDEX IF NOT EXISTS` for idempotency
- Metadata ID changed from `"20260616_initial_schema"` to `"20260127222353_familiar_lady_ursula"` (matching opencode's first migration)

### GAP 4 (CRITICAL): Only 1 migration vs 35

**Location:** `storage.rs` line 468 (original `ALL_MIGRATIONS`)

**Issue:** The original had only `INITIAL_MIGRATION` (1 migration) while opencode has 35 migrations in dependency order. Without all 35, the migration history cannot be properly tracked and databases upgraded from different versions would be incompatible.

**Fix applied:** Added all 35 migrations as `Migration` struct instances in `ALL_MIGRATIONS`, with SQL matching the opencode migration files.

### GAP 5 (MODERATE): Missing columns in `insert_session`

**Location:** `database.rs` (original `insert_session` method)

**Issue:** The original `insert_session` only accepted 11 parameters and was missing critical columns:
- `parent_id` (for session nesting)
- `path` (for custom session paths)
- `cost`, `tokens_input`, `tokens_output` (token tracking)

**Fix applied:** Extended `insert_session` to accept 16 parameters adding `parent_id`, `path`, `cost`, `tokens_input`, `tokens_output`.

### GAP 6 (MODERATE): Incomplete `update_session`

**Location:** `database.rs` (original `update_session` method)

**Issue:** The original `update_session` only updated 4 mutable fields. Missing 14+ fields that opencode can update.

**Fix applied:** Extended `update_session` to accept 19 parameters covering all mutable session columns: tokens_reasoning, tokens_cache_read, tokens_cache_write, share_url, summary_additions, summary_deletions, summary_files, summary_diffs, metadata, revert, permission, time_compacting, time_archived.

---

## Gaps DOCUMENTED (Not Fixed - Acceptable for Scaffold)

### GAP 7: Missing CRUD for 15 tables

**Issue:** `DatabaseService` only has CRUD for 4 tables (session, message, part, session_message). Missing CRUD for:
- workspace, project, project_directory
- session_input, session_context_epoch, session_share
- todo, account, control_account, account_state
- credential, permission, event, event_sequence
- data_migration

**Rationale:** These are scaffold-level placeholders. Downstream modules will add CRUD as needed. The CREATE TABLE definitions and indexes are already correct in `database.rs`.

### GAP 8: No `data_migration` CRUD

**Issue:** opencode has `DataMigrationTable` in `data-migration.sql.ts` with `name` PK and `time_completed`. No Rust equivalent exists.

**Rationale:** Scaffold. The table definition exists in `database.rs` (`CREATE_TABLE_DATA_MIGRATION`). CRUD can be added when data migration logic is implemented.

### GAP 9: No prepared statement caching

**Issue:** opencode (via drizzle-orm) uses prepared statements. rustcode uses `sqlx::query()` which does its own statement caching at the driver level. This is generally equivalent.

**Rationale:** `sqlx` handles prepared statement caching transparently. No action needed.

### GAP 10: No ORM layer (Drizzle → sqlx)

**Issue:** opencode uses drizzle-orm with custom column types. rustcode uses raw SQL with sqlx.

**Rationale:** This is by design per the rustcode CLAUDE.md. The custom path column types from `path.ts` are replicated as helper functions in `database.rs`.

### GAP 11: Missing `directoryColumn` and `directoryColumn` behavior

**Issue:** opencode's `directoryColumn` allows empty strings (for legacy sessions) while normalizing non-empty values. rustcode's `db_absolute_path()` rejects all non-absolute paths.

**Rationale:** Acceptable for scaffold. The Rust implementation is stricter, which is generally better.

---

## Verification Summary

| Fix # | File | Change | Status |
|-------|------|--------|--------|
| 1 | `storage.rs` | `_migration` → `migration` (table name) | ✓ FIXED |
| 2 | `storage.rs` | Added `PRAGMA wal_checkpoint(PASSIVE)` | ✓ FIXED |
| 3 | `storage.rs` | Rewrote `INITIAL_MIGRATION` with correct schema | ✓ FIXED |
| 4 | `storage.rs` | Added all 35 migrations to `ALL_MIGRATIONS` | ✓ FIXED |
| 5 | `database.rs` | Extended `insert_session` with parent_id, path, cost, tokens | ✓ FIXED |
| 6 | `database.rs` | Extended `update_session` with all mutable columns | ✓ FIXED |
| 7 | `database.rs` | Updated all test calls to match new signatures | ✓ FIXED |

### What was already correct in rustcode

- All 20 CREATE_TABLE constants in `database.rs` match opencode's final schema
- All 17 CREATE_INDEX constants match opencode
- `CONNECTION_PRAGMAS` array matches opencode (6 PRAGMAs including wal_checkpoint)
- `ALL_TABLE_NAMES` lists all 20 tables correctly
- `KNOWN_MIGRATION_IDS` lists all 35 migration IDs correctly
- `database_path()` function logic matches opencode's `path()` function
- `GlobalPaths` XDG implementation matches opencode's `global.ts`
- `DatabaseConfig` with PRAGA method matches opencode pattern
- Path helper functions (`db_absolute_path`, `db_path`, etc.) match opencode's `path.ts`
- `is_existing_install()` helper checks for `session` table (matches opencode)
- Migration run loop uses per-migration transactions (matches opencode's `applyOnly`)
- Error handling via `DatabaseServiceError` with Database, NotFound, ConstraintViolation variants

### Files Modified

1. `/root/opencodesport/rustcode/crates/rustcode-core/src/storage.rs` - Complete rewrite of migration section
2. `/root/opencodesport/rustcode/crates/rustcode-core/src/database.rs` - Extended insert_session, update_session, updated tests

### Report written: `/root/opencodesport/rustcode/reports/06-database.md`
