# Filesystem/Process/PTY/Shell/Utility — Gap Analysis

## Filesystem

| Feature | TS | Rust | Status |
|---------|----|------|--------|
| Read/List/Find/Glob/Grep schemas | Full | Full | ✅ |
| Path escape protection | `resolve()` | `resolve_safe()` | ✅ |
| Read file with UTF-8/base64 fallback | Full | Full | ✅ |
| List directory with sorting | Full | Full | ✅ |
| Fuzzy file search | `fuzzysort` | Custom substring | ⚠️ Weaker |
| Glob search | `ripgrep`-based | Custom minimal | ⚠️ Weaker |
| Grep search | `ripgrep` external | Built-in | ⚠️ Different |
| **Ignored folders** | 32 entries | 28 entries (missing 4) | ⚠️ |
| MIME type map | `mime-types` npm (1000+) | ~27 entries | ❌ **Weaker** |
| **Watcher** | Full `@parcel/watcher` with event bus | **Types only** | ❌ **CRITICAL** |
| **Search engine (FFF)** | `@ff-labs/fff-bun` native | Custom `walk_for_entries` | ❌ **CRITICAL** |

## fs_util

| Feature | TS | Rust | Status |
|---------|----|------|--------|
| mimeType | Full (npm mime-types) | ~27 entries | ❌ Weaker |
| normalizePath | Full | Full | ✅ |
| resolve (realpath) | Full | Full | ✅ |
| windowsPath | Full | Full | ✅ |
| overlaps/contains | Full | Full | ✅ |
| `writeWithDirs` | ✅ | ❌ Missing | ❌ |
| `findUp` | ✅ | ❌ Missing | ❌ |
| `up` (multi-target) | ✅ | ❌ Missing | ❌ |
| `globUp` | ✅ | ❌ Missing | ❌ |

## Process

| Feature | TS | Rust | Status |
|---------|----|------|--------|
| RunOptions | Full | Full | ✅ |
| RunResult | Full | Full | ✅ |
| Interface | spawner + run + runStream | Full | ✅ |
| Output truncation | ✅ | ✅ | ✅ |
| Timeout | Effect.timeoutOrElse | tokio::time::timeout | ✅ |
| Cancellation | AbortSignal | CancellationToken | ✅ |
| Kill process tree | SIGTERM→SIGKILL + taskkill | ✅ | ✅ |

## PTY

| Feature | TS | Rust | Status |
|---------|----|------|--------|
| Types | Full | Full | ✅ |
| Protocol (metaFrame/chunks/decodeInput) | Full | Full | ✅ |
| Ticket (ConnectToken/Scope/issue/consume) | Full | Full | ✅ |
| **Runtime layer** (session map, spawn, lifecycle, events) | Full (pty.ts:122-343) | **Types/trait only** | ❌ **CRITICAL** |
| **PTY spawn backend** | 2 (bun-pty + node-pty) | **None** | ❌ **CRITICAL** |

## Shell

| Feature | TS | Rust | Status |
|---------|----|------|--------|
| META record (9 shells) | Full | Full | ✅ |
| killTree | Full | Full | ✅ |
| gitbash detection | Full | Full | ✅ |
| args() per shell | Full | Full | ✅ |
| preferred() with caching | Full | Full | ✅ |
| acceptable() with caching | Full | Full | ✅ |
| list()/select() | Full | Full | ✅ |
| ShellService with execute() | — | ✅ Extra | Rust extra |

## Ripgrep

| Feature | TS | Rust | Status |
|---------|----|------|--------|
| RawMatch schema | Full | Full | ✅ |
| Error types | Full | Full | ✅ |
| FindInput/GlobInput/GrepInput | Full | Full | ✅ |
| Interface (find/glob/grep) | Full | Full | ✅ |
| Service layer | Full | Full | ✅ |
| Platform config | 7 platforms | Same 7 | ✅ |
| **Binary download** | Full tar.gz extraction | Simplified (hardcoded URL) | ⚠️ |

## State

| Feature | TS | Rust | Status |
|---------|----|------|--------|
| Transform type | `Transform<Editor>` | Matching | ✅ |
| Interface (get/transform/update/mutate) | Full | Full | ✅ |
| Rebuild with transform replay | Full | Full | ✅ |
| Semaphore-based mutual exclusion | Effect Semaphore | Mutex | ⚠️ Different |
| Immer draft pattern | ✅ `MakeEditor` | ❌ No draft abstraction | ❌ |

## Utility Functions

**~95+ utility functions missing** across:
- `packages/core/src/util/` (18 files): `array.ts`, `binary.ts`, `effect-flock.ts`, `encode.ts`, `error.ts`, `flock.ts` (358L — file locking), `glob.ts`, `hash.ts`, `path.ts`, `retry.ts`, `slug.ts`, `token.ts`, `which.ts`, `wildcard.ts`
- `packages/opencode/src/util/` (23 files): `archive.ts`, `bom.ts`, `data-url.ts`, `defer.ts`, `filesystem.ts` (21 functions), `html.ts`, `local-context.ts`, `media.ts`, `process.ts`, `proxy-env.ts`, `queue.ts`, `repository.ts`, `rpc.ts`, `signal.ts`, `timeout.ts`

## 5 Most Critical Gaps

### 1. PTY Runtime Layer Is Missing
TS: Complete PTY lifecycle: session map, spawn, attach/detach, buffer management, event publishing, exit cleanup.
Rust: Only types and trait — no `spawn()`, no session registry, no runtime.

**TS**: `pty.ts:122-343`
**Rust**: `pty.rs:646-687` (trait only)

### 2. Filesystem Watcher Is Types-Only
TS: Full `@parcel/watcher` integration with event bus, VCS watching, config-driven ignore.
Rust: Types only — no subscription or runtime.

**TS**: `watcher.ts:32-142`
**Rust**: `filesystem.rs:526-594`

### 3. No FFF / Fuzzy File Search Engine
TS: `@ff-labs/fff-bun` native file finder for fast fuzzy search.
Rust: Custom synchronous `walk_for_entries` with minimal glob matching.

**TS**: `search.ts:126-233`
**Rust**: `filesystem.rs:811-876`

### 4. File Locking System Absent
TS: `flock.ts:1-358` + `effect-flock.ts:1-285` — distributed file locking with heartbeat, breaker pattern, jittered retry.
Rust: Nothing.

### 5. 95+ Utility Functions Missing
Every feature branch depends on these. Key missing: encoding, hashing, glob scanning, which, retry, path manipulation, lazy, defer, signals, queues, RPC, repository parsing, proxy detection, HTML escaping, media sniffing, BOM handling, archive extraction.
