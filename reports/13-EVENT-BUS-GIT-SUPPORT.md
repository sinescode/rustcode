# Event/Bus/Git/Supporting Modules — Gap Analysis

## Per-Module Summary

| Module | TS LOC | Rust LOC | Status | Critical Gaps |
|--------|--------|----------|--------|---------------|
| Bus | 22 | 941 | ✅ Complete + Enhanced | 0 |
| ID | 160 | 539 | ✅ Complete | 1 minor |
| Env | 43 | 718 | ✅ Complete + Enhanced | 0 |
| **Event** | 705 | 1951 | ⚠️ **Partial** | **4 critical** |
| Git | 795 | 1436 | ✅ Complete | 0 |
| **Snapshot** | 817 | 1260 | ⚠️ Partial | **2 critical** |
| Skill | 718 | ~1440 | ✅ Complete | 0 |
| Sync | 11 | 72 | ✅ Complete | 0 |
| **Image** | 268 | 622 | ❌ **Very Partial** | **5 critical** |
| **Question** | 437 | 1255 | ⚠️ Partial | **2 critical** |
| **Share** | 446 | 147 | ❌ **Stub Only** | **6 critical** |
| **Worktree** | 654 | 893 | ⚠️ Partial | **3 critical** |

## Event System — 4 Critical Gaps

### 1. No DB Persistence on Publish
**TS**: `event.ts:396-407` — Writes to EventSequenceTable + EventTable in SQLite transaction.
**Rust**: `event.rs:874-923` — Entirely in-memory. All events lost on restart.

### 2. `aggregateEvents()` Has No Historical Replay
**TS**: `event.ts:606-628` — First reads historical events from DB, then subscribes to live.
**Rust**: `event.rs:1060-1078` — Live subscription only. No historical events replayed.

### 3. `replay()` Skips Idempotency Checks
**TS**: `event.ts:453-482` — Full commitSyncEvent pipeline with DB validation.
**Rust**: `event.rs:1112-1140` — No duplicate detection, no sequence check, no owner check.

### 4. `claim()`/`remove()` Are No-Ops
**TS**: `event.ts:518-536` — Updates/deletes from DB.
**Rust**: `event.rs:1084-1101` — In-memory only.

## Image System — 5 Critical Gaps

### 1. No Image Resizer
**TS**: `image/image.ts:63-164` — Uses Photon WASM to resize images.
**Rust**: No `image` crate integration.

### 2. No `normalize()` Function
TS transforms `FilePart` → `FilePart` with resize. Rust only validates.

### 3. No Base64 Decode/Resize/Re-encode Pipeline
TS progressive resize: 32 step-down sizes × 5 JPEG qualities.

### 4. No Progressive Resize Algorithm
TS tries increasingly smaller sizes until fitting limits.

### 5. MIME Detection Only
Only `detect_mime`, `is_image_mime`, `is_media`, `is_pdf_mime` ported. Core normalization absent.

## Share System — 6 Critical Gaps

Share module is a **skeleton** — `ShareNextInterface` and `SessionShareInterface` traits with **no implementations**:
- No HTTP client
- No queue/batch sync mechanism
- No event watching
- No full sync on share creation
- No DB storage
- No high-level service impl

## Worktree — 3 Critical Gaps

- **No EventBus/EventV2 integration** (TS publishes worktree.ready/failed)
- **No database integration** (TS looks up project start commands)
- **No start command execution** after worktree creation

## Snapshot — 2 Critical Gaps

- **No `cat-file --batch` optimization** — Rust calls `git show` per file (O(n) processes vs O(1))
- **No background GC cleanup loop** — TS runs `git gc --prune=7.days` every hour

## Question — 2 Critical Gaps

- **No event publishing** on ask/reply/reject
- **No lifecycle cleanup** — pending questions left on shutdown
