# Database/Storage System — Gap Analysis

## Table Schema Parity

**20 tables defined in both implementations** ✅ — all columns match by name.

However, **14 typed JSON columns** in TS use `text({mode:"json"})` with auto-serialize/deserialize. Rust stores all as plain `text` — **loss of type safety**.

## Migration System

| Aspect | TS | Rust |
|--------|----|------|
| Format | Arbitrary Effect functions | Static `&str` SQL split by `;` |
| Files | 35 separate `.ts` files | 1 inlined array |
| Locking | Global `Semaphore` | **None** |
| Drizzle journal import | Yes | **No** |
| Fresh install fast path | Detects first-run | **Simplified** |
| Conditional logic | Full Effect expressions | **Impossible** |

## JSON Storage

| Feature | TS | Rust |
|---------|----|------|
| File locking | `TxReentrantLock` per file | **None** |
| Data migrations | 2 built-in | **None** |
| Schema validation | `Schema.decodeUnknownOption` | Generic serde |
| Async | Effect-based | Synchronous `std::fs` |

## SessionStore

| Feature | TS | Rust |
|---------|----|------|
| `context(sessionID)` | Context-epoch-aware loading | **Missing** |
| `runnerContext(sessionID, baselineSeq)` | Runner-filtered loading | **Missing** |
| Schema-decoded messages | Full | Raw JSON strings |

## Missing CRUD for 12 Tables

Rust only has CRUD for 6 tables. Missing:
- `project`, `project_directory`, `account`, `account_state`, `control_account`
- `credential`, `event`, `event_sequence`, `permission`, `workspace`
- `session_share`, `todo`

## 5 Most Critical Gaps

### 1. Migration System Fundamentally Weaker
TS migrations are arbitrary Effect functions with conditional logic and data backfills. Rust uses static SQL strings split by `;` — fragile and inflexible.

**TS**: `migration.ts:13-16`
**Rust**: `storage.rs:190-195`

### 2. JSON Storage Lacks Locking, Migrations, and Validation
Concurrent access causes data corruption. Old JSON formats never migrated. No schema validation.

**TS**: `storage.ts:63-65`, `218-221`
**Rust**: `storage.rs:42-185`

### 3. No Platform-Aware Path Validation
TS uses `DatabasePath` column types that validate/normalize paths. Rust has standalone functions not integrated into CRUD.

**TS**: `path.ts:27-91`
**Rust**: `database.rs:891-973`

### 4. No Equivalent of SessionStore with Context-Epoch-Aware Loading
TS SessionStore provides context-epoch-aware message loading. Rust only has raw CRUD.

**TS**: `store.ts:13-23`
**Rust**: `database.rs:1060-1916`

### 5. Missing Domain-Specific CRUD for 12 Tables
No CRUD wrappers for project, account, credential, event, permission, workspace, share, todo tables.

**Rust**: Only 6 of 18 data tables have CRUD methods.
