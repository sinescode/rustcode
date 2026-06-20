# FILESYSTEM/FILES Gap Analysis: Rust vs TypeScript

**Rust source**: `crates/rustcode-core/src/filesystem.rs`, `file_mutation.rs`, `fs_util.rs`
**TypeScript source**: `packages/core/src/filesystem.ts`, `filesystem/`, `fs-util.ts`, `file-mutation.ts`, `packages/opencode/src/tool/read.ts`, `write.ts`, `edit.ts`
**OpenCode commit**: `5d0f86606ac30690f79f0a6a9f41a1f49fe95d0b`

---

## 1. Schema Types (Entry, Submatch, Match)

| Feature | Rust | TypeScript | Gap |
|---|---|---|---|
| `Entry` struct | `path`, `entry_type`, `mime` | `path`, `type`, `mime` | ✅ Parity (field naming differs: `entry_type` vs `type`) |
| `Submatch` struct | `text`, `start`, `end` (u32) | `text`, `start`, `end` (NonNegativeInt) | ✅ Parity |
| `Match` struct | `entry`, `line`, `offset`, `text`, `submatches` | Same fields | ✅ Parity |
| `FileType` enum | `File`, `Directory` | Literals `["file", "directory"]` | ✅ Parity |
| Serde serialization | `rename_all = "lowercase"` | Effect Schema literals | ✅ Parity |

**Severity**: ✅ No gap. All schema types are correctly ported.

---

## 2. Input Types (ReadInput, FindInput, GlobInput, GrepInput)

| Feature | Rust | TypeScript | Gap |
|---|---|---|---|
| `ReadInput` | `{ path: RelativePath }` | Same | ✅ Parity |
| `Content` struct | `uri`, `name?`, `content`, `encoding`, `mime` | Same | ✅ Parity |
| `ContentEncoding` | `Utf8`, `Base64` | Literals `["utf8", "base64"]` | ✅ Parity |
| `ListInput` | `{ path: Option<RelativePath> }` | Same | ✅ Parity |
| `FindInput` | `query`, `type?`, `limit?` | Same | ✅ Parity |
| `GlobInput` | `pattern`, `path?`, `limit?` | Same | ✅ Parity |
| `GrepInput` | `pattern`, `path?`, `include?`, `limit?` | Same | ✅ Parity |

**Severity**: ✅ No gap. All input types match.

---

## 3. Ignore Patterns

| Feature | Rust | TypeScript | Gap |
|---|---|---|---|
| `FOLDERS` / `IGNORE_FOLDERS` | 28 folder names | 28 folder names (Set) | ✅ Parity (identical content) |
| `FILES` / `IGNORE_FILES` | 11 glob patterns | 11 glob patterns (Array) | ✅ Parity (identical content) |
| `PATTERNS` / `IGNORE_PATTERNS` | Empty static, `ignore_patterns()` function | `[...FILES, ...FOLDERS]` | ⚠️ Minor: TS `PATTERNS` is a static array; Rust `IGNORE_PATTERNS` is empty, must use `ignore_patterns()` function |
| `match()` / `is_ignored()` | Same logic: whitelist check → folder check → file glob | Same logic | ✅ Parity |
| Whitelist support | `IgnoreMatchOptions { extra, whitelist }` | `match(filepath, { extra?, whitelist? })` | ✅ Parity |
| Extra patterns | Supported via `opts.extra` | Supported via `opts.extra` | ✅ Parity |
| Glob matching backend | Custom `glob_matches()` (minimal) | `Glob.match()` from `util/glob.ts` | ⚠️ **Gap**: TS uses `Glob.match()` (likely `picomatch`-style); Rust has a minimal custom `glob_matches()` with limited pattern support |

**Severity**: ⚠️ Minor gap — the Rust glob matcher is a simplified implementation that may not handle all patterns the same way as the TS `Glob.match()`.

---

## 4. Protected Paths

| Feature | Rust | TypeScript | Gap |
|---|---|---|---|
| macOS home dirs | 9 names (Music, Pictures, etc.) | Same 9 names | ✅ Parity |
| macOS Library dirs | 10 names | Same 10 names | ✅ Parity |
| macOS root paths | 4 paths | Same 4 paths | ✅ Parity |
| Windows home dirs | 8 names | Same 8 names | ✅ Parity |
| Linux protected names | Empty `[]` | Empty `Set()` | ✅ Parity |
| `protected_names()` | Returns `&'static [&'static str]` | Returns `ReadonlySet<string>` | ✅ Parity |
| `protected_paths()` | Returns `Vec<String>` with home dir join | Returns `string[]` with `path.join(home, name)` | ✅ Parity |
| Platform gating | `#[cfg(target_os)]` | `process.platform` checks | ✅ Parity |

**Severity**: ✅ No gap. Protected paths are correctly ported.

---

## 5. File Watcher

| Feature | Rust | TypeScript | Gap |
|---|---|---|---|
| `WatcherEventKind` | `Add`, `Change`, `Unlink` | Literals `["add", "change", "unlink"]` | ✅ Parity |
| `WatcherEvent` | `{ file, event }` | Same | ✅ Parity |
| `WatcherBackend` | `Windows`, `FsEvents`, `Inotify` | String literals `"windows"`, `"fs-events"`, `"inotify"` | ✅ Parity |
| `watcher_backend()` | Platform detection via `cfg!()` | `process.platform` check | ✅ Parity |
| `SUBSCRIBE_TIMEOUT_MS` | `10_000` | `10_000` | ✅ Parity |
| `FileEditedEvent` | `{ file: String }` | `{ file: Schema.String }` in Event.Edited | ✅ Parity |
| `hasNativeBinding()` | ❌ Not ported | `!!watcher()` | ❌ **Gap**: No native binding check |
| Watcher `layer` / subscription | ❌ Types only, no runtime watcher | Full runtime with `@parcel/watcher` integration, callbacks, ignore patterns, protected path exclusion, `.git` handling | ❌ **Gap**: Rust has only type definitions, no actual watcher implementation |
| Watcher finalizer/cleanup | ❌ Not ported | `Effect.addFinalizer` to unsubscribe all | ❌ **Gap**: No cleanup mechanism |
| Config-driven ignore | ❌ Not ported | Reads `Config.Document` watcher ignore patterns | ❌ **Gap**: No config integration |
| Git directory handling | ❌ Not ported | Subscribes to `.git` dir with ignore filtering | ❌ **Gap**: No git integration |

**Severity**: 🔴 Major gap — The watcher is types-only in Rust. The full runtime watcher with `@parcel/watcher` (or Rust equivalent like `notify`) is not implemented.

---

## 6. File Read Operations

| Feature | Rust | TypeScript | Gap |
|---|---|---|---|
| `read_file()` | Reads file, UTF-8 or base64 fallback | `FileSystem.Service.read()` returns `Uint8Array` + mime | ⚠️ **Gap**: TS returns raw bytes; Rust auto-detects encoding |
| `list_directory()` | Reads entries, filters ignored, sorts dirs-first | Same logic with `readDirectoryEntries` + sort | ✅ Parity |
| Read tool constants | ❌ Not ported | `DEFAULT_READ_LIMIT=2000`, `MAX_LINE_LENGTH=2000`, `MAX_BYTES=50KB`, `SAMPLE_BYTES=4096` | ❌ **Gap**: No read tool constants |
| Line truncation | ❌ Not ported | Lines >2000 chars truncated with `... (line truncated)` suffix | ❌ **Gap**: No line truncation |
| Byte cap (50KB) | ❌ Not ported | Stops reading at 50KB with `Output capped` message | ❌ **Gap**: No byte cap |
| Binary file detection | ❌ Not ported | Extension-based + non-printable byte ratio detection (>0.3) | ❌ **Gap**: No binary file detection |
| Image/PDF handling | ❌ Not ported | Detects image/PDF MIME, returns as base64 attachment | ❌ **Gap**: No image/PDF handling |
| File miss suggestions | ❌ Not ported | On file not found, suggests similar filenames via fuzzy match | ❌ **Gap**: No miss suggestions |
| Directory listing with offset | ❌ Not ported | Supports `offset`/`limit` pagination for directory entries | ❌ **Gap**: No pagination |
| LSP warm-up | ❌ Not ported | Calls `lsp.touchFile()` after read | ❌ **Gap**: No LSP integration |
| Instruction resolution | ❌ Not ported | Resolves file-specific instructions for system context | ❌ **Gap**: No instruction service |
| Symlink resolution in list | ❌ Not ported | Resolves symlinks to determine if target is directory | ❌ **Gap**: No symlink resolution |
| `FileMetadata` | `entry`, `size`, `modified_ms`, `readable`, `writable` | Not exposed as separate type (stat inline) | ✅ Rust has extra metadata type (good) |

**Severity**: 🔴 Major gap — The Rust `read_file` is minimal vs the TS `ReadTool` which has extensive features (byte caps, binary detection, image handling, pagination, LSP integration, etc.).

---

## 7. File Find/Search Operations

| Feature | Rust | TypeScript | Gap |
|---|---|---|---|
| `find_files()` | Custom `walk_for_entries` + `fuzzy_match` | `fuzzysort` library or `fff.fileSearch()` | 🔴 **Gap**: Rust uses custom substring/fuzzy; TS uses `fuzzysort` (much better scoring) |
| `glob_search()` | Custom `walk_for_entries` + `glob_matches` | `ripgrep.glob()` or `fff.glob()` | 🔴 **Gap**: Rust walks manually; TS uses ripgrep (much faster) |
| `grep_search()` | Custom `walk_for_entries` + `regex` crate | `ripgrep.grep()` or `fff.grep()` | 🔴 **Gap**: Rust walks manually; TS uses ripgrep |
| `fuzzysort` | ❌ Not available | `fuzzysort.go()` with proper scoring | ❌ **Gap**: No fuzzy search library |
| `ripgrep` integration | ❌ Not ported | `Ripgrep.Service` with `find`, `glob`, `grep` methods | ❌ **Gap**: No ripgrep integration |
| `fff` (FileFinder) | ❌ Not ported | Native Bun binding for fast file search/glob/grep | ❌ **Gap**: No fff integration |
| Search backend selection | ❌ Not ported | `Flag.OPENCODE_DISABLE_FFF` flag selects ripgrep vs fff | ❌ **Gap**: No backend selection |
| VCS-aware search | ❌ Not ported | Uses `location.vcs` to set scan limits | ❌ **Gap**: No VCS awareness |
| Frecency tracking | ❌ Not ported | `fff.trackQuery()`, `getHistoricalQuery()` | ❌ **Gap**: No frecency |
| Mixed search | ❌ Not ported | `fff.mixedSearch()` returns files + directories together | ❌ **Gap**: No mixed search |
| Directory search | ❌ Not ported | `fff.directorySearch()` | ❌ **Gap**: No directory search |
| Time budget for grep | ❌ Not ported | `timeBudgetMs: 1_500` in fff grep | ❌ **Gap**: No time budget |

**Severity**: 🔴 Major gap — The Rust search is a naive directory walker vs TS which uses ripgrep/fff for high-performance search. This is a critical performance gap.

---

## 8. File Mutation (Write/Edit/Remove)

| Feature | Rust | TypeScript | Gap |
|---|---|---|---|
| `WriteInput` | `{ target, content: Text\|Binary }` | Same | ✅ Parity |
| `TextWriteInput` | `{ target, content: String }` | Same | ✅ Parity |
| `ConditionalWriteInput` | `{ target, content, expected: Vec<u8> }` | Same | ✅ Parity |
| `RemoveInput` | `{ target }` | Same | ✅ Parity |
| `StaleContentError` | `{ path: String }` | Same | ✅ Parity |
| `TargetExistsError` | `{ path: String }` | Same | ✅ Parity |
| `WriteResult` | `operation`, `target`, `resource`, `existed` | Same | ✅ Parity |
| `RemoveResult` | `operation`, `target`, `resource`, `existed` | Same | ✅ Parity |
| `MutationResult` | Tagged union `Written \| Removed` | Not present (separate types) | ✅ Rust adds tagged union (good) |
| Base64 serialization | `base64_bytes` module | N/A (uses `Uint8Array` directly) | ✅ Parity (Rust approach appropriate) |
| **Runtime operations** | ❌ Types only, no `create()`/`write()`/`writeIfUnchanged()`/`remove()` impl | Full `Service` with `create`, `write`, `writeTextPreservingBom`, `writeIfUnchanged`, `remove` | 🔴 **Gap**: No runtime implementation |
| **KeyedMutex** | ❌ Not ported | `KeyedMutex` serializes mutations per canonical path | 🔴 **Gap**: No serialization |
| **BOM handling** | ❌ Not ported | `splitBom`, `joinBom`, `hasUtf8Bom` | ❌ **Gap**: No BOM support |
| **`flag: "wx"` (create-only)** | ❌ Not ported | Uses `O_EXCL` flag for atomic create | ❌ **Gap**: No atomic create |
| `writeTextPreservingBom` | ❌ Not ported | Reads existing BOM, preserves on write | ❌ **Gap**: No BOM-preserving write |

**Severity**: 🔴 Major gap — All file mutation types are ported but the runtime implementation is missing. The TS has a full `FileMutation.Service` with mutex-based serialization, BOM handling, and atomic create operations.

---

## 9. Tool-Level Operations (read.ts, write.ts, edit.ts)

### Read Tool

| Feature | Rust | TypeScript | Gap |
|---|---|---|---|
| Tool definition | ❌ Not ported | `Tool.define("read", ...)` with parameters | ❌ **Gap** |
| Permission check | ❌ Not ported | `ctx.ask({ permission: "read" })` | ❌ **Gap** |
| External directory check | ❌ Not ported | `assertExternalDirectoryEffect()` | ❌ **Gap** |
| `ReadStop` error | ❌ Not ported | Tagged error for early stream termination | ❌ **Gap** |
| Stream-based reading | ❌ Not ported | `fs.stream()` → `Stream.splitLines` → `Stream.runForEach` | ❌ **Gap** |
| TextDecoder | ❌ Not ported | Manual `TextDecoder("utf-8")` for streaming decode | ❌ **Gap** |
| Instruction loading | ❌ Not ported | `instruction.resolve()` for file-specific context | ❌ **Gap** |

### Write Tool

| Feature | Rust | TypeScript | Gap |
|---|---|---|---|
| Tool definition | ❌ Not ported | `Tool.define("write", ...)` with parameters | ❌ **Gap** |
| Permission check | ❌ Not ported | `ctx.ask({ permission: "edit" })` | ❌ **Gap** |
| Diff generation | ❌ Not ported | `createTwoFilesPatch()` from `diff` library | ❌ **Gap** |
| `trimDiff()` | ❌ Not ported | Strips common leading whitespace from diffs | ❌ **Gap** |
| BOM handling | ❌ Not ported | `Bom.readFile()`, `Bom.split()`, `Bom.join()`, `Bom.syncFile()` | ❌ **Gap** |
| Formatter integration | ❌ Not ported | `format.file(filepath)` for auto-formatting after write | ❌ **Gap** |
| Event publishing | ❌ Not ported | `FileSystem.Event.Edited` + `Watcher.Event.Updated` | ❌ **Gap** |
| LSP diagnostics | ❌ Not ported | Reports LSP errors after write, up to 5 other files | ❌ **Gap** |
| External directory check | ❌ Not ported | `assertExternalDirectoryEffect()` | ❌ **Gap** |

### Edit Tool

| Feature | Rust | TypeScript | Gap |
|---|---|---|---|
| Tool definition | ❌ Not ported | `Tool.define("edit", ...)` with `oldString`/`newString`/`replaceAll` | ❌ **Gap** |
| **Replacer chain** | ❌ Not ported | 9 replacers: Simple, LineTrimmed, BlockAnchor, WhitespaceNormalized, IndentationFlexible, EscapeNormalized, MultiOccurrence, TrimmedBoundary, ContextAware | 🔴 **Gap**: This is the core edit intelligence |
| **BlockAnchorReplacer** | ❌ Not ported | First/last line anchor matching with Levenshtein similarity | 🔴 **Gap** |
| **Levenshtein distance** | ❌ Not ported | Used for similarity-based block matching | ❌ **Gap** |
| Similarity thresholds | ❌ Not ported | `SINGLE_CANDIDATE_SIMILARITY_THRESHOLD=0.65`, `MULTIPLE_CANDIDATES_SIMILARITY_THRESHOLD=0.65` | ❌ **Gap** |
| `isDisproportionateMatch` | ❌ Not ported | Rejects matches where span is much larger than oldString | ❌ **Gap** |
| Line ending detection | ❌ Not ported | `detectLineEnding()`, `convertToLineEnding()`, `normalizeLineEndings()` | ❌ **Gap** |
| Per-file semaphore lock | ❌ Not ported | `Semaphore.makeUnsafe(1)` per resolved filepath | ❌ **Gap** |
| Snapshot integration | ❌ Not ported | `Snapshot.FileDiff` with additions/deletions count | ❌ **Gap** |
| `replaceAll` mode | ❌ Not ported | `content.replaceAll(search, newString)` | ❌ **Gap** |

**Severity**: 🔴 Critical gap — None of the tool-level operations (read/write/edit tools) are ported. The edit tool's replacer chain is particularly important for LLM-driven code editing.

---

## 10. Filesystem Utilities (fs_util.rs)

| Feature | Rust | TypeScript | Gap |
|---|---|---|---|
| `mime_type()` | Manual extension match (~30 types) | `lookup()` from `mime-types` library | ⚠️ **Gap**: Rust is less comprehensive; TS uses `mime-types` for full MIME database |
| `normalize_path()` | Platform check + canonicalize | Same | ✅ Parity |
| `windows_path()` | Handles `/X:/`, `/X/`, `/cygdrive/`, `/mnt/` | Same regex patterns | ✅ Parity |
| `resolve_path()` | `canonicalize` + normalize | `realpathSync` + normalize | ✅ Parity |
| `normalize_path_pattern()` | Handles `\*` and `/*` patterns | Same | ✅ Parity |
| `overlaps()` | `contains(a,b) \|\| contains(b,a)` | Same | ✅ Parity |
| `contains()` | `strip_prefix` + check for `..` | `relative()` + check | ✅ Parity |
| `ensure_dir()` | `create_dir_all` | `fs.makeDirectory({ recursive })` | ✅ Parity |
| `DirEntry` type | `{ name, entry_type: File\|Directory\|Symlink\|Other }` | `{ name, type: "file"\|"directory"\|"symlink"\|"other" }` | ✅ Parity |
| `GlobOptions` | `cwd`, `absolute`, `include`, `dot`, `max_depth`, `ignore` | Same concept via `Glob.Options` | ✅ Parity |
| `GlobInclude` | `File`, `Directory`, `All` | Same | ✅ Parity |
| `FindUpOptions` | `targets`, `start`, `stop?` | Inline in `findUp`/`up` | ✅ Parity |
| `findUp()` | ❌ Not ported (only options type) | Walks up directory tree looking for target | ⚠️ Gap: options type exists but no function |
| `up()` | ❌ Not ported (only options type) | Walks up looking for multiple targets | ⚠️ Gap: options type exists but no function |
| `globUp()` | ❌ Not ported | Walks up running glob at each level | ⚠️ Gap: no function |
| `readJson()` | ❌ Not ported | `JSON.parse(fs.readFileString(path))` | ❌ **Gap** |
| `writeJson()` | ❌ Not ported | `JSON.stringify(data, null, 2)` + write | ❌ **Gap** |
| `readFileStringSafe()` | ❌ Not ported | Returns `undefined` on NotFound | ❌ **Gap** |
| `existsSafe()` | ❌ Not ported | Returns `false` on error | ❌ **Gap** |
| `writeWithDirs()` | ❌ Not ported | Auto-creates parent directories on NotFound | ❌ **Gap** |
| `globMatch()` | ❌ Not ported | `Glob.match(pattern, filepath)` | ❌ **Gap** |
| `isDir()` / `isFile()` | Not in fs_util (in filesystem.rs) | `FSUtil.Service.isDir/isFile` | ⚠️ Different location |

**Severity**: ⚠️ Moderate gap — Core path utilities are ported, but several helper functions (`findUp`, `up`, `globUp`, `readJson`, `writeJson`, `writeWithDirs`, `existsSafe`) are missing.

---

## 11. FFF (FileFinder) Integration

| Feature | Rust | TypeScript | Gap |
|---|---|---|---|
| `fff.bun.ts` | ❌ Not ported | Bun native binding to `@ff-labs/fff-bun` | ❌ **Gap** |
| `fff.node.ts` | ❌ Not ported | Node.js stub (always returns unavailable) | ❌ **Gap** |
| `FileFinder.create()` | ❌ Not ported | Creates picker with `basePath`, `aiMode`, scanning options | ❌ **Gap** |
| `Picker.fileSearch()` | ❌ Not ported | Fuzzy file search with scores | ❌ **Gap** |
| `Picker.glob()` | ❌ Not ported | Glob with pagination | ❌ **Gap** |
| `Picker.grep()` | ❌ Not ported | Regex/fuzzy grep with context lines, time budget | ❌ **Gap** |
| `Picker.directorySearch()` | ❌ Not ported | Directory-only search | ❌ **Gap** |
| `Picker.mixedSearch()` | ❌ Not ported | Combined file+directory search | ❌ **Gap** |
| `Picker.trackQuery()` | ❌ Not ported | Frecency tracking | ❌ **Gap** |
| `Picker.getHistoricalQuery()` | ❌ Not ported | Query history | ❌ **Gap** |
| `Picker.isScanning()` | ❌ Not ported | Background scan status | ❌ **Gap** |
| `Picker.waitForScan()` | ❌ Not ported | Wait for scan completion | ❌ **Gap** |
| `Picker.refreshGitStatus()` | ❌ Not ported | Git status refresh | ❌ **Gap** |
| `Picker.destroy()` | ❌ Not ported | Cleanup | ❌ **Gap** |

**Severity**: 🔴 Major gap — The FFF integration provides high-performance file search. Rust has no equivalent.

---

## 12. Search Backend Architecture

| Feature | Rust | TypeScript | Gap |
|---|---|---|---|
| `FileSystemSearch.Service` | ❌ Not ported | Context.Service with `find`, `glob`, `grep` | ❌ **Gap** |
| `ripgrepLayer` | ❌ Not ported | Uses `Ripgrep.Service` for find/glob/grep | ❌ **Gap** |
| `fffLayer` | ❌ Not ported | Uses `Fff` for find/glob/grep | ❌ **Gap** |
| `defaultLayer` | ❌ Not ported | Selects ripgrep or fff based on flag | ❌ **Gap** |
| `Flag.OPENCODE_DISABLE_FFF` | ❌ Not ported | Forces ripgrep backend | ❌ **Gap** |
| `Flag.OPENCODE_EXPERIMENTAL_DISABLE_FILEWATCHER` | ❌ Not ported | Disables file watcher | ❌ **Gap** |
| `Flag.OPENCODE_EXPERIMENTAL_FILEWATCHER` | ❌ Not ported | Enables file watcher | ❌ **Gap** |

**Severity**: 🔴 Major gap — The entire search backend architecture (service layers, backend selection, flag-based configuration) is not ported.

---

## 13. Content Encoding & Binary Handling

| Feature | Rust | TypeScript | Gap |
|---|---|---|---|
| UTF-8 detection | `std::str::from_utf8` | `TextDecoder("utf-8")` | ✅ Parity |
| Base64 fallback | `base64::engine::general_purpose::STANDARD.encode` | `Buffer.from(bytes).toString("base64")` | ✅ Parity |
| Binary detection | ❌ Not ported | Extension list + non-printable byte ratio | ❌ **Gap** |
| BOM handling | ❌ Not ported | `Bom.split()`, `Bom.join()`, `Bom.syncFile()` | ❌ **Gap** |
| Image MIME sniffing | ❌ Not ported | `sniffAttachmentMime()` from `util/media.ts` | ❌ **Gap** |
| PDF attachment detection | ❌ Not ported | `isPdfAttachment()` from `util/media.ts` | ❌ **Gap** |

**Severity**: ⚠️ Moderate gap — Basic encoding works, but binary detection, BOM handling, and media sniffing are missing.

---

## 14. Glob Patterns

| Feature | Rust | TypeScript | Gap |
|---|---|---|---|
| `glob_matches()` implementation | Custom: handles `**/`, `*`, `**/*.ext`, `**/*suffix` | Delegates to `Glob.match()` (picomatch-style) | ⚠️ **Gap**: Rust's custom matcher has limited pattern support |
| `Glob.scan()` | ❌ Not ported | Full glob with `cwd`, `absolute`, `include`, `dot`, `maxDepth`, `ignore` | ❌ **Gap**: No glob scan function |
| `Glob.match()` | ❌ Not ported | Pattern matching against file paths | ❌ **Gap** |
| `normalize_path_pattern()` | Present | Same | ✅ Parity |

**Severity**: ⚠️ Moderate gap — The custom glob matcher works for basic patterns but lacks the full power of picomatch.

---

## 15. Path Safety & Escape Prevention

| Feature | Rust | TypeScript | Gap |
|---|---|---|---|
| `resolve_safe()` | Rejects `..` that escapes root | `FSUtil.contains()` check | ✅ Parity |
| Canonical path resolution | Uses `canonicalize()` | Uses `fs.realPath()` | ✅ Parity |
| Lexical fallback | Checks `candidate.starts_with(root)` when path doesn't exist | Same concept | ✅ Parity |
| `assertExternalDirectoryEffect()` | ❌ Not ported | Validates path is within allowed directory | ❌ **Gap** |
| `Location.Service` integration | ❌ Not ported | Uses `location.directory` and `location.worktree` | ❌ **Gap** |

**Severity**: ⚠️ Moderate gap — Core path safety works, but external directory assertion and location service integration are missing.

---

## 16. Severity Summary

| Category | Severity | Gap Description |
|---|---|---|
| **Tool implementations (read/write/edit)** | 🔴 Critical | No tool definitions, parameter schemas, permission checks, or execution logic |
| **Edit replacer chain** | 🔴 Critical | 9 sophisticated replacers for robust LLM-driven editing not ported |
| **Search backend (ripgrep/fff)** | 🔴 Major | No ripgrep or FFF integration; uses naive directory walker |
| **File watcher runtime** | 🔴 Major | Types only, no actual watcher with `@parcel/watcher` or equivalent |
| **File mutation runtime** | 🔴 Major | Types only, no `Service` with mutex, BOM, atomic create |
| **Search backend architecture** | 🔴 Major | No service layers, backend selection, flag configuration |
| **FFF (FileFinder)** | 🔴 Major | No native file finder integration |
| **Ignore patterns** | ⚠️ Minor | `PATTERNS` export is empty; custom glob matcher is simplified |
| **MIME type detection** | ⚠️ Minor | Manual extension list vs `mime-types` library |
| **Glob patterns** | ⚠️ Moderate | Custom `glob_matches()` is limited; no `Glob.scan()` or `Glob.match()` |
| **Filesystem utilities** | ⚠️ Moderate | Missing `findUp`, `up`, `globUp`, `readJson`, `writeJson`, `writeWithDirs`, `existsSafe` |
| **Content encoding** | ⚠️ Moderate | Missing binary detection, BOM handling, media sniffing |
| **Path safety** | ⚠️ Moderate | Missing external directory assertion, location service integration |
| **Schema types** | ✅ Complete | All schema types correctly ported |
| **Input types** | ✅ Complete | All input types match |
| **Protected paths** | ✅ Complete | All platform-specific protected paths ported |
| **Basic file ops** | ✅ Complete | `read_file`, `list_directory`, `find_files`, `glob_search`, `grep_search` work |

---

## 17. Recommendations

### Priority 1 (Critical)
1. **Port the edit tool replacer chain** — This is the core intelligence for LLM code editing. The 9 replacers (Simple, LineTrimmed, BlockAnchor, WhitespaceNormalized, IndentationFlexible, EscapeNormalized, MultiOccurrence, TrimmedBoundary, ContextAware) plus Levenshtein distance and `isDisproportionateMatch` are essential.
2. **Port read/write/edit tool definitions** — Parameter schemas, permission checks, and execution logic.
3. **Integrate ripgrep** — Use the `grep` crate or shell out to `rg` for search operations. The naive directory walker is too slow for real codebases.

### Priority 2 (Major)
4. **Implement file watcher** — Use the `notify` crate (Rust equivalent of `@parcel/watcher`).
5. **Implement FileMutation.Service** — Runtime operations with `KeyedMutex`-like serialization, BOM handling, and atomic create.
6. **Port FFF or equivalent** — Consider using `ignore` crate + `WalkDir` for fast file discovery, or integrate with `ripgrep` for search.

### Priority 3 (Moderate)
7. **Enhance glob matching** — Use the `glob` or `globset` crate instead of custom `glob_matches()`.
8. **Port filesystem utilities** — `findUp`, `up`, `globUp`, `readJson`, `writeJson`, `writeWithDirs`.
9. **Add binary detection** — Extension list + non-printable byte ratio heuristic.
10. **Integrate `mime-types` crate** — Replace manual extension matching.

---

*Report generated: 2026-06-20*
*Source commit: 5d0f86606ac30690f79f0a6a9f41a1f49fe95d0b*
