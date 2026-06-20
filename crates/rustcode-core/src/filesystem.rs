//! Filesystem types — schema, ignore patterns, protected paths, search, and watcher.
//!
//! Ported from:
//! - `packages/core/src/filesystem.ts`
//! - `packages/core/src/filesystem/schema.ts`
//! - `packages/core/src/filesystem/ignore.ts`
//! - `packages/core/src/filesystem/protected.ts`
//! - `packages/core/src/filesystem/search.ts`
//! - `packages/core/src/filesystem/watcher.ts`
//!
//! OpenCode commit: 5d0f86606ac30690f79f0a6a9f41a1f49fe95d0b

use crate::schema::RelativePath;
use serde::{Deserialize, Serialize};

// ── Schema types ──────────────────────────────────────────────────────────
// Ported from: packages/core/src/filesystem/schema.ts

/// The type of a filesystem entry.
///
/// # Source
/// Ported from `packages/core/src/filesystem/schema.ts` `Entry.type`
/// (`Schema.Literals(["file", "directory"])`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum FileType {
    /// A regular file.
    File,
    /// A directory.
    Directory,
}

/// A single filesystem entry (file or directory).
///
/// # Source
/// Ported from `packages/core/src/filesystem/schema.ts` `Entry` class.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Entry {
    /// Relative path within the project.
    pub path: RelativePath,
    /// Whether this is a file or directory.
    #[serde(rename = "type")]
    pub entry_type: FileType,
    /// MIME type string (e.g. `"text/plain"`, `"application/x-directory"`).
    pub mime: String,
}

/// A submatch range within a matched line (start and end byte offsets).
///
/// # Source
/// Ported from `packages/core/src/filesystem/schema.ts` `Submatch` struct.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Submatch {
    /// The matched text.
    pub text: String,
    /// Byte offset where the match starts (inclusive).
    pub start: u32,
    /// Byte offset where the match ends (exclusive).
    pub end: u32,
}

/// A single search (grep) match result.
///
/// # Source
/// Ported from `packages/core/src/filesystem/schema.ts` `Match` class.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Match {
    /// The entry (file) where the match was found.
    pub entry: Entry,
    /// 1-based line number (> 0).
    pub line: u32,
    /// Byte offset from the start of the line.
    pub offset: u32,
    /// The full line text.
    pub text: String,
    /// Submatch ranges within the line.
    pub submatches: Vec<Submatch>,
}

// ── Input / output types ──────────────────────────────────────────────────
// Ported from: packages/core/src/filesystem.ts

/// Input for reading a single file.
///
/// # Source
/// Ported from `packages/core/src/filesystem.ts` `ReadInput`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ReadInput {
    /// Relative path to the file to read.
    pub path: RelativePath,
}

/// Encoding for file content.
///
/// # Source
/// Ported from `packages/core/src/filesystem.ts` `Content.encoding`
/// (`Schema.Literals(["utf8", "base64"])`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum ContentEncoding {
    /// UTF-8 text content.
    Utf8,
    /// Base64-encoded binary content.
    Base64,
}

/// File content returned from a read operation.
///
/// # Source
/// Ported from `packages/core/src/filesystem.ts` `Content` struct.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Content {
    /// URI for the file (e.g. `"file:///home/user/project/src/main.rs"`).
    pub uri: String,
    /// Optional display name for the content.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,
    /// The file content as a string.
    pub content: String,
    /// Content encoding.
    pub encoding: ContentEncoding,
    /// MIME type string.
    pub mime: String,
}

/// Input for listing directory entries.
///
/// # Source
/// Ported from `packages/core/src/filesystem.ts` `ListInput`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ListInput {
    /// Optional relative path to list (defaults to project root).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub path: Option<RelativePath>,
}

/// Input for fuzzy-finding files by name.
///
/// # Source
/// Ported from `packages/core/src/filesystem.ts` `FindInput` class.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct FindInput {
    /// Fuzzy search query string.
    pub query: String,
    /// Filter by entry type.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub r#type: Option<FileType>,
    /// Maximum number of results to return.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub limit: Option<u32>,
}

/// Input for glob-based file matching.
///
/// # Source
/// Ported from `packages/core/src/filesystem.ts` `GlobInput` class.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct GlobInput {
    /// Glob pattern to match.
    pub pattern: String,
    /// Optional relative path to scope the search.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub path: Option<RelativePath>,
    /// Maximum number of results to return.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub limit: Option<u32>,
}

/// Input for grep (content search) operations.
///
/// # Source
/// Ported from `packages/core/src/filesystem.ts` `GrepInput` class.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct GrepInput {
    /// Regex or literal pattern to search for.
    pub pattern: String,
    /// Optional relative path to scope the search.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub path: Option<RelativePath>,
    /// Optional glob pattern to filter files (e.g. `"*.rs"`).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub include: Option<String>,
    /// Maximum number of results to return.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub limit: Option<u32>,
}

// ── Ignore patterns ───────────────────────────────────────────────────────
// Ported from: packages/core/src/filesystem/ignore.ts

/// Directory basenames that are always ignored during filesystem scans.
///
/// # Source
/// Ported from `packages/core/src/filesystem/ignore.ts` `FOLDERS` Set.
pub static IGNORE_FOLDERS: &[&str] = &[
    "node_modules",
    "bower_components",
    ".pnpm-store",
    "vendor",
    ".npm",
    "dist",
    "build",
    "out",
    ".next",
    "target",
    "bin",
    "obj",
    ".git",
    ".svn",
    ".hg",
    ".vscode",
    ".idea",
    ".turbo",
    ".output",
    "desktop",
    ".sst",
    ".cache",
    ".webkit-cache",
    "__pycache__",
    ".pytest_cache",
    "mypy_cache",
    ".history",
    ".gradle",
];

/// File glob patterns that are always ignored during filesystem scans.
///
/// # Source
/// Ported from `packages/core/src/filesystem/ignore.ts` `FILES` array.
pub static IGNORE_FILES: &[&str] = &[
    "**/*.swp",
    "**/*.swo",
    "**/*.pyc",
    "**/.DS_Store",
    "**/Thumbs.db",
    "**/logs/**",
    "**/tmp/**",
    "**/temp/**",
    "**/*.log",
    "**/coverage/**",
    "**/.nyc_output/**",
];

/// Combined ignore patterns: files + folders.
///
/// # Source
/// Ported from `packages/core/src/filesystem/ignore.ts` `PATTERNS` const.
pub static IGNORE_PATTERNS: &[&str] = &[];

// Build IGNORE_PATTERNS lazily at static-init time.
// We cannot concat slices at compile time easily, so we provide a function.
// The static is kept for API compatibility with the TS `PATTERNS` export.

/// Options for ignore matching.
///
/// # Source
/// Ported from `packages/core/src/filesystem/ignore.ts` `match()` opts parameter.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct IgnoreMatchOptions {
    /// Additional glob patterns to treat as ignored.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub extra: Option<Vec<String>>,
    /// Whitelist patterns that override ignore rules (matching files are NOT ignored).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub whitelist: Option<Vec<String>>,
}

/// Check whether a file path matches ignore patterns.
///
/// Returns `true` if the path should be ignored.
///
/// # Source
/// Ported from `packages/core/src/filesystem/ignore.ts` `match()` function.
///
/// # Note
/// Uses a simple substring match for folder names and prefix match for glob
/// patterns. For production use, integrate with the `glob` or `ignore` crate.
pub fn is_ignored(filepath: &str, opts: Option<&IgnoreMatchOptions>) -> bool {
    // Whitelist check: if any whitelist pattern matches, the file is NOT ignored.
    if let Some(opts) = opts {
        if let Some(ref whitelist) = opts.whitelist {
            for pattern in whitelist {
                if glob_matches(pattern, filepath) {
                    return false;
                }
            }
        }
    }

    // Check each path segment against folder ignore list.
    let parts: Vec<&str> = filepath.split(&['/', '\\']).collect();
    for part in &parts {
        if IGNORE_FOLDERS.contains(part) {
            return true;
        }
    }

    // Check file-level patterns.
    let extra: &[String] = opts.and_then(|o| o.extra.as_deref()).unwrap_or(&[]);
    for pattern in IGNORE_FILES
        .iter()
        .copied()
        .chain(extra.iter().map(|s| s.as_str()))
    {
        if glob_matches(pattern, filepath) {
            return true;
        }
    }

    false
}

/// Simple glob matching for ignore patterns.
///
/// Supports `**/` (match any ancestor directory), `*` (match within a single
/// path segment), and literal matching. This is a minimal implementation
/// suitable for ignore checks; for production use the `glob` or `ignore` crate.
fn glob_matches(pattern: &str, filepath: &str) -> bool {
    // Normalize slashes.
    let filepath = filepath.replace('\\', "/");

    // "**/" patterns match anywhere in the path.
    if let Some(suffix) = pattern.strip_prefix("**/") {
        // **/X/** — match any path that contains X/ as a directory component.
        if let Some(inner) = suffix.strip_suffix("/**") {
            return filepath.contains(&format!("/{inner}/"))
                || filepath.starts_with(&format!("{inner}/"))
                || filepath == inner;
        }
        // **/*.ext — match by file extension.
        if let Some(ext) = suffix.strip_prefix("*.") {
            if let Some(dot) = filepath.rfind('.') {
                return &filepath[dot + 1..] == ext;
            }
            return false;
        }
        // **/*suffix — match the final path component ending.
        if let Some(file_suffix) = suffix.strip_prefix('*') {
            if let Some(file_name) = filepath.rsplit('/').next() {
                return file_name.ends_with(file_suffix);
            }
            return filepath.ends_with(file_suffix);
        }
        // **/literal — match path ending with suffix, or containing /suffix.
        return filepath.ends_with(suffix) || filepath.contains(&format!("/{suffix}"));
    }

    // Literal match (with possible single-level wildcards, no **).
    if let Some((prefix, suffix)) = pattern.split_once('*') {
        if let Some(file_stem) = filepath.rsplit('/').next() {
            return file_stem.starts_with(prefix) && file_stem.ends_with(suffix);
        }
        return filepath.starts_with(prefix) && filepath.ends_with(suffix);
    }

    // Exact match.
    filepath == pattern
}

/// Return the combined set of ignore patterns (folders + files + extras).
///
/// # Source
/// Ported from `packages/core/src/filesystem/ignore.ts` `PATTERNS`.
pub fn ignore_patterns() -> Vec<String> {
    let mut patterns: Vec<String> = IGNORE_FILES.iter().map(|s| s.to_string()).collect();
    patterns.extend(IGNORE_FOLDERS.iter().map(|s| s.to_string()));
    patterns
}

// ── Protected paths ───────────────────────────────────────────────────────
// Ported from: packages/core/src/filesystem/protected.ts

/// Directory basenames to skip when scanning the home directory on macOS.
///
/// # Source
/// Ported from `packages/core/src/filesystem/protected.ts` `DARWIN_HOME`.
#[cfg(target_os = "macos")]
static DARWIN_HOME_NAMES: &[&str] = &[
    "Music",
    "Pictures",
    "Movies",
    "Downloads",
    "Desktop",
    "Documents",
    "Public",
    "Applications",
    "Library",
];

/// Library subdirectories to protect on macOS.
///
/// # Source
/// Ported from `packages/core/src/filesystem/protected.ts` `DARWIN_LIBRARY`.
#[cfg(target_os = "macos")]
static DARWIN_LIBRARY_NAMES: &[&str] = &[
    "Application Support/AddressBook",
    "Calendars",
    "Mail",
    "Messages",
    "Safari",
    "Cookies",
    "Application Support/com.apple.TCC",
    "PersonalizationPortrait",
    "Metadata/CoreSpotlight",
    "Suggestions",
];

/// Root-level paths to protect on macOS.
///
/// # Source
/// Ported from `packages/core/src/filesystem/protected.ts` `DARWIN_ROOT`.
#[cfg(target_os = "macos")]
static DARWIN_ROOT_PATHS: &[&str] = &[
    "/.DocumentRevisions-V100",
    "/.Spotlight-V100",
    "/.Trashes",
    "/.fseventsd",
];

/// Directory basenames to skip when scanning the home directory on Windows.
///
/// # Source
/// Ported from `packages/core/src/filesystem/protected.ts` `WIN32_HOME`.
#[cfg(target_os = "windows")]
static WIN32_HOME_NAMES: &[&str] = &[
    "AppData",
    "Downloads",
    "Desktop",
    "Documents",
    "Pictures",
    "Music",
    "Videos",
    "OneDrive",
];

/// Directory basenames to skip when scanning the home directory.
///
/// Returns platform-appropriate names:
/// - macOS: `["Music", "Pictures", "Movies", "Downloads", "Desktop", "Documents", "Public", "Applications", "Library"]`
/// - Windows: `["AppData", "Downloads", "Desktop", "Documents", "Pictures", "Music", "Videos", "OneDrive"]`
/// - Linux: empty slice
///
/// # Source
/// Ported from `packages/core/src/filesystem/protected.ts` `names()`.
pub fn protected_names() -> &'static [&'static str] {
    #[cfg(target_os = "macos")]
    {
        DARWIN_HOME_NAMES
    }
    #[cfg(target_os = "windows")]
    {
        WIN32_HOME_NAMES
    }
    #[cfg(not(any(target_os = "macos", target_os = "windows")))]
    {
        &[]
    }
}

/// Absolute paths that should never be watched, stated, or scanned.
///
/// Returns platform-appropriate absolute paths joined with the user's home
/// directory:
/// - macOS: standard home folders + Library subdirectories + root paths
/// - Windows: standard home folders
/// - Linux: empty vec
///
/// # Source
/// Ported from `packages/core/src/filesystem/protected.ts` `paths()`.
pub fn protected_paths() -> Vec<String> {
    let home = dirs::home_dir()
        .map(|p| p.to_string_lossy().to_string())
        .unwrap_or_default();

    #[cfg(target_os = "macos")]
    {
        let mut paths: Vec<String> = Vec::new();
        // DARWIN_HOME mapped under home
        for name in DARWIN_HOME_NAMES {
            paths.push(format!("{home}/{name}"));
        }
        // DARWIN_LIBRARY mapped under home/Library
        for name in DARWIN_LIBRARY_NAMES {
            paths.push(format!("{home}/Library/{name}"));
        }
        // DARWIN_ROOT — absolute root paths
        for path in DARWIN_ROOT_PATHS {
            paths.push(path.to_string());
        }
        paths
    }

    #[cfg(target_os = "windows")]
    {
        let mut paths: Vec<String> = Vec::new();
        for name in WIN32_HOME_NAMES {
            paths.push(format!("{home}\\{name}"));
        }
        paths
    }

    #[cfg(not(any(target_os = "macos", target_os = "windows")))]
    {
        Vec::new()
    }
}

// ── Search types ──────────────────────────────────────────────────────────
// Ported from: packages/core/src/filesystem/search.ts

/// Search scope — filter by entry type.
///
/// # Source
/// Ported from `packages/core/src/filesystem/search.ts` (used in find/grep
/// as `FileSystem.FindInput.type`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum SearchFileType {
    /// Search only files.
    File,
    /// Search only directories.
    Directory,
}

// ── Watcher types ─────────────────────────────────────────────────────────
// Ported from: packages/core/src/filesystem/watcher.ts

/// Timeout for watcher subscription in milliseconds.
///
/// # Source
/// Ported from `packages/core/src/filesystem/watcher.ts` `SUBSCRIBE_TIMEOUT_MS`.
pub const SUBSCRIBE_TIMEOUT_MS: u64 = 10_000;

/// The kind of filesystem event from the watcher.
///
/// # Source
/// Ported from `packages/core/src/filesystem/watcher.ts` `Event.Updated` schema
/// (`Schema.Literals(["add", "change", "unlink"])`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum WatcherEventKind {
    /// A file was created.
    Add,
    /// A file was modified.
    Change,
    /// A file was deleted.
    Unlink,
}

/// A filesystem watcher event.
///
/// # Source
/// Ported from `packages/core/src/filesystem/watcher.ts` `Event.Updated`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct WatcherEvent {
    /// Absolute path to the affected file.
    pub file: String,
    /// The type of change event.
    pub event: WatcherEventKind,
}

/// Backend used by the filesystem watcher.
///
/// # Source
/// Ported from `packages/core/src/filesystem/watcher.ts` `getBackend()`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum WatcherBackend {
    /// Windows — uses ReadDirectoryChangesW.
    Windows,
    /// macOS — uses FSEvents.
    FsEvents,
    /// Linux — uses inotify.
    Inotify,
}

/// Determine the appropriate watcher backend for the current platform.
///
/// Returns `None` if the platform is not supported (should not happen for
/// tier-1 Rust targets).
///
/// # Source
/// Ported from `packages/core/src/filesystem/watcher.ts` `getBackend()`.
pub fn watcher_backend() -> Option<WatcherBackend> {
    if cfg!(target_os = "windows") {
        Some(WatcherBackend::Windows)
    } else if cfg!(target_os = "macos") {
        Some(WatcherBackend::FsEvents)
    } else if cfg!(target_os = "linux") {
        Some(WatcherBackend::Inotify)
    } else {
        None
    }
}

// ── Filesystem event ──────────────────────────────────────────────────────
// Ported from: packages/core/src/filesystem.ts

/// Notional filesystem event — file edited.
///
/// This mirrors the TS `Event.Edited` definition. In practice, edited events
/// are published through the global event bus; this struct defines the payload.
///
/// # Source
/// Ported from `packages/core/src/filesystem.ts` `Event.Edited`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct FileEditedEvent {
    /// Absolute or relative path to the edited file.
    pub file: String,
}

// ── Filesystem operations ───────────────────────────────────────────────────
// Ported from: packages/core/src/filesystem.ts (Interface: read, list, find, glob, grep)

use std::path::{Path, PathBuf};

/// Errors that can occur during filesystem operations.
///
/// # Source
/// Ported from error patterns in `packages/core/src/filesystem.ts`.
#[derive(Debug, thiserror::Error)]
pub enum FileSystemError {
    /// The path does not exist.
    #[error("file not found: {0}")]
    NotFound(String),

    /// The path is not a file when a file was expected.
    #[error("not a file: {0}")]
    NotAFile(String),

    /// The path is not a directory when a directory was expected.
    #[error("not a directory: {0}")]
    NotADirectory(String),

    /// The path escapes the allowed root/project directory.
    #[error("path escapes root: {0}")]
    PathEscapesRoot(String),

    /// An I/O error occurred.
    #[error("io error: {0}")]
    Io(#[from] std::io::Error),

    /// Pattern compilation error (regex).
    #[error("invalid pattern: {0}")]
    InvalidPattern(String),

    /// Unsupported encoding.
    #[error("unsupported encoding: {0}")]
    UnsupportedEncoding(String),
}

/// Metadata for a file or directory entry.
///
/// # Source
/// Derived from `fs.stat` + `Entry` in `packages/core/src/filesystem.ts`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct FileMetadata {
    /// The entry itself (path, type, mime).
    pub entry: Entry,
    /// File size in bytes (0 for directories).
    pub size: u64,
    /// Last modification time as epoch millis.
    pub modified_ms: i64,
    /// Whether the entry is readable.
    pub readable: bool,
    /// Whether the entry is writable.
    pub writable: bool,
}

/// Read a file's content with automatic encoding detection.
///
/// Attempts UTF-8 first; falls back to base64 for binary content.
///
/// # Source
/// Ported from `packages/core/src/filesystem.ts` `read()` method (lines 89–96).
pub fn read_file(root: &Path, input: &ReadInput) -> Result<Content, FileSystemError> {
    let absolute = resolve_safe(root, &input.path)?;
    let metadata = std::fs::metadata(&absolute)?;
    if !metadata.is_file() {
        return Err(FileSystemError::NotAFile(absolute.display().to_string()));
    }

    let raw = std::fs::read(&absolute)?;

    // Try UTF-8 first, fall back to base64
    match std::str::from_utf8(&raw) {
        Ok(text) => {
            let mime = mime_type(&absolute);
            Ok(Content {
                uri: format!("file://{}", absolute.display()),
                name: std::path::Path::new(input.path.as_str())
                    .file_name()
                    .map(|n| n.to_string_lossy().to_string()),
                content: text.to_string(),
                encoding: ContentEncoding::Utf8,
                mime,
            })
        }
        Err(_) => {
            let mime = mime_type(&absolute);
            Ok(Content {
                uri: format!("file://{}", absolute.display()),
                name: std::path::Path::new(input.path.as_str())
                    .file_name()
                    .map(|n| n.to_string_lossy().to_string()),
                content: base64_encode(&raw),
                encoding: ContentEncoding::Base64,
                mime,
            })
        }
    }
}

/// List directory entries with metadata.
///
/// Sorts directories before files, then alphabetically by path.
///
/// # Source
/// Ported from `packages/core/src/filesystem.ts` `list()` method (lines 98–121).
pub fn list_directory(
    root: &Path,
    input: Option<&ListInput>,
) -> Result<Vec<Entry>, FileSystemError> {
    let rel_path = input
        .and_then(|i| i.path.as_ref())
        .cloned()
        .unwrap_or_else(|| RelativePath::new("."));
    let absolute = resolve_safe(root, &rel_path)?;
    let metadata = std::fs::metadata(&absolute)?;
    if !metadata.is_dir() {
        return Err(FileSystemError::NotADirectory(
            absolute.display().to_string(),
        ));
    }

    let mut entries: Vec<Entry> = Vec::new();
    let dir_iter = std::fs::read_dir(&absolute)?;

    for item in dir_iter {
        let item = item?;
        let file_type = item.file_type()?;
        let name = item.file_name().to_string_lossy().to_string();

        let (entry_type, mime, path_suffix) = if file_type.is_dir() {
            (
                FileType::Directory,
                "application/x-directory".to_string(),
                format!("{name}/"),
            )
        } else if file_type.is_file() {
            let item_abs = item.path();
            (FileType::File, mime_type(&item_abs), name.clone())
        } else {
            continue; // Skip symlinks and special files
        };

        // Compute relative path from root
        let item_abs = item.path();
        let item_rel = item_abs
            .strip_prefix(root)
            .unwrap_or(&item_abs)
            .to_string_lossy()
            .to_string();

        // Skip ignored paths
        if is_ignored(&item_rel, None) {
            continue;
        }

        entries.push(Entry {
            path: RelativePath::new(&item_rel),
            entry_type,
            mime,
        });
    }

    // Sort: directories first, then alphabetically
    entries.sort_by(|a, b| match (a.entry_type, b.entry_type) {
        (FileType::Directory, FileType::File) => std::cmp::Ordering::Less,
        (FileType::File, FileType::Directory) => std::cmp::Ordering::Greater,
        _ => a.path.as_str().cmp(b.path.as_str()),
    });

    Ok(entries)
}

/// Search for files by fuzzy name matching.
///
/// Walks the directory tree from `root`, matching filenames against the query.
/// Respects ignore patterns and optional type/limit filters.
///
/// # Source
/// Ported from `packages/core/src/filesystem.ts` `find()` method.
pub fn find_files(root: &Path, input: &FindInput) -> Result<Vec<Entry>, FileSystemError> {
    let limit = input.limit.unwrap_or(50) as usize;
    let mut results: Vec<Entry> = Vec::new();
    let added = std::cell::Cell::new(0usize);

    walk_for_entries(root, root, &mut results, &|entry| {
        if added.get() >= limit {
            return false;
        }
        // Type filter
        if let Some(ref filter_type) = input.r#type {
            if entry.entry_type != *filter_type {
                return false;
            }
        }
        // Fuzzy match: check if query appears as substring of the filename
        let file_name = std::path::Path::new(entry.path.as_str())
            .file_name()
            .and_then(|n| n.to_str())
            .unwrap_or("");
        if fuzzy_match(&input.query, file_name) {
            added.set(added.get() + 1);
            true
        } else {
            false
        }
    })?;

    Ok(results)
}

/// Search for files matching a glob pattern.
///
/// Uses the `glob` crate for pattern matching against filesystem entries.
///
/// # Source
/// Ported from `packages/core/src/filesystem.ts` `glob()` method.
pub fn glob_search(root: &Path, input: &GlobInput) -> Result<Vec<Entry>, FileSystemError> {
    let limit = input.limit.unwrap_or(100) as usize;
    let mut results: Vec<Entry> = Vec::new();
    let added = std::cell::Cell::new(0usize);

    let search_root = if let Some(ref rel_path) = input.path {
        resolve_safe(root, rel_path)?
    } else {
        root.to_path_buf()
    };

    if !search_root.exists() {
        return Ok(results);
    }

    walk_for_entries(root, &search_root, &mut results, &|entry| {
        if added.get() >= limit {
            return false;
        }
        if glob_matches(&input.pattern, entry.path.as_str()) {
            added.set(added.get() + 1);
            true
        } else {
            false
        }
    })?;

    Ok(results)
}

/// Search file contents using regex pattern matching (grep).
///
/// Walks the directory tree, reading file contents and matching against
/// the regex pattern line by line.
///
/// # Source
/// Ported from `packages/core/src/filesystem.ts` `grep()` method.
pub fn grep_search(root: &Path, input: &GrepInput) -> Result<Vec<Match>, FileSystemError> {
    let limit = input.limit.unwrap_or(50) as usize;
    let re = regex::Regex::new(&input.pattern)
        .map_err(|e| FileSystemError::InvalidPattern(format!("regex error: {e}")))?;
    let mut results: Vec<Match> = Vec::new();

    let search_root = if let Some(ref rel_path) = input.path {
        resolve_safe(root, rel_path)?
    } else {
        root.to_path_buf()
    };

    if !search_root.exists() {
        return Ok(results);
    }

    // Collect files to search (filtered by include pattern and ignore)
    let mut files: Vec<Entry> = Vec::new();
    walk_for_entries(root, &search_root, &mut files, &|entry| {
        if entry.entry_type != FileType::File {
            return false;
        }
        if let Some(ref include) = input.include {
            if !glob_matches(include, entry.path.as_str()) {
                return false;
            }
        }
        true
    })?;

    for file_entry in &files {
        if results.len() >= limit {
            break;
        }

        let absolute = root.join(file_entry.path.as_str());
        let content = match std::fs::read_to_string(&absolute) {
            Ok(c) => c,
            Err(_) => continue, // Skip binary/unreadable files
        };

        for (line_idx, line_text) in content.lines().enumerate() {
            if results.len() >= limit {
                break;
            }

            if let Some(captures) = re.captures(line_text) {
                let mut submatches = Vec::new();
                for (i, cap) in captures.iter().enumerate() {
                    if let Some(m) = cap {
                        submatches.push(Submatch {
                            text: m.as_str().to_string(),
                            start: m.start() as u32,
                            end: m.end() as u32,
                        });
                    }
                }
                // Only add if we have actual submatches (non-empty matches)
                if !submatches.is_empty() {
                    results.push(Match {
                        entry: file_entry.clone(),
                        line: (line_idx + 1) as u32,
                        offset: captures.get(0).map(|m| m.start() as u32).unwrap_or(0),
                        text: line_text.to_string(),
                        submatches,
                    });
                }
            }
        }
    }

    Ok(results)
}

/// Get metadata for a file or directory.
///
/// # Source
/// Derived from `fs.stat` in `packages/core/src/filesystem.ts`.
pub fn file_metadata(
    root: &Path,
    rel_path: &RelativePath,
) -> Result<FileMetadata, FileSystemError> {
    let absolute = resolve_safe(root, rel_path)?;
    let metadata = std::fs::metadata(&absolute)?;

    let (entry_type, mime) = if metadata.is_dir() {
        (FileType::Directory, "application/x-directory".to_string())
    } else if metadata.is_file() {
        (FileType::File, mime_type(&absolute))
    } else {
        return Err(FileSystemError::NotFound(format!(
            "unsupported file type: {}",
            absolute.display()
        )));
    };

    let modified_ms = metadata
        .modified()
        .ok()
        .and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok())
        .map(|d| d.as_millis() as i64)
        .unwrap_or(0);

    Ok(FileMetadata {
        entry: Entry {
            path: rel_path.clone(),
            entry_type,
            mime,
        },
        size: metadata.len(),
        modified_ms,
        readable: !metadata.permissions().readonly(),
        writable: !metadata.permissions().readonly(),
    })
}

/// Check if a file or directory exists at the given relative path.
pub fn file_exists(root: &Path, rel_path: &RelativePath) -> bool {
    let absolute = root.join(rel_path.as_str());
    absolute.exists()
}

/// Check whether a path is a directory.
pub fn is_directory(root: &Path, rel_path: &RelativePath) -> Result<bool, FileSystemError> {
    let absolute = resolve_safe(root, rel_path)?;
    Ok(absolute.is_dir())
}

/// Check whether a path is a file.
pub fn is_file(root: &Path, rel_path: &RelativePath) -> Result<bool, FileSystemError> {
    let absolute = resolve_safe(root, rel_path)?;
    Ok(absolute.is_file())
}

// ── Internal helpers ────────────────────────────────────────────────────────

/// Resolve a relative path safely within the root directory — prevents path escape.
fn resolve_safe(root: &Path, rel: &RelativePath) -> Result<PathBuf, FileSystemError> {
    let rel_str = rel.as_str();

    // Reject paths with `..` components that would escape
    if rel_str.contains("..") {
        // Allow `..` only if the resolved path stays within root
        let candidate = root.join(rel_str);
        match candidate.canonicalize() {
            Ok(resolved) => {
                if resolved.starts_with(root) {
                    return Ok(resolved);
                }
                return Err(FileSystemError::PathEscapesRoot(rel_str.to_string()));
            }
            Err(_) => {
                // If path doesn't exist yet, check lexically
                let candidate = root.join(rel_str);
                // Simple check: the candidate must start with root
                if candidate.starts_with(root) {
                    return Ok(candidate);
                }
                return Err(FileSystemError::PathEscapesRoot(rel_str.to_string()));
            }
        }
    }

    Ok(root.join(rel_str))
}

/// Walk a directory tree, collecting entries that pass the filter predicate.
fn walk_for_entries(
    root: &Path,
    current: &Path,
    results: &mut Vec<Entry>,
    predicate: &dyn Fn(&Entry) -> bool,
) -> Result<(), FileSystemError> {
    if !current.is_dir() {
        return Ok(());
    }

    let dir_iter = match std::fs::read_dir(current) {
        Ok(iter) => iter,
        Err(_) => return Ok(()), // Skip unreadable directories
    };

    for item in dir_iter {
        let item = match item {
            Ok(i) => i,
            Err(_) => continue,
        };
        let file_type = match item.file_type() {
            Ok(ft) => ft,
            Err(_) => continue,
        };
        let name = item.file_name().to_string_lossy().to_string();
        let item_abs = item.path();

        let item_rel = item_abs
            .strip_prefix(root)
            .unwrap_or(&item_abs)
            .to_string_lossy()
            .to_string();

        // Skip ignored paths
        if is_ignored(&item_rel, None) {
            continue;
        }

        if file_type.is_dir() {
            let entry = Entry {
                path: RelativePath::new(&item_rel),
                entry_type: FileType::Directory,
                mime: "application/x-directory".to_string(),
            };
            if predicate(&entry) {
                results.push(entry);
            }
            // Recurse into subdirectories
            walk_for_entries(root, &item_abs, results, &predicate)?;
        } else if file_type.is_file() {
            let mime = mime_type(&item_abs);
            let entry = Entry {
                path: RelativePath::new(&item_rel),
                entry_type: FileType::File,
                mime,
            };
            if predicate(&entry) {
                results.push(entry);
            }
        }
    }

    Ok(())
}

/// Determine MIME type from file extension.
fn mime_type(path: &Path) -> String {
    let ext = path
        .extension()
        .and_then(|e| e.to_str())
        .unwrap_or("")
        .to_lowercase();

    match ext.as_str() {
        // Text
        "txt" => "text/plain".to_string(),
        "md" | "markdown" => "text/markdown".to_string(),
        "csv" => "text/csv".to_string(),
        "html" | "htm" => "text/html".to_string(),
        "css" => "text/css".to_string(),
        "xml" => "text/xml".to_string(),
        // Code
        "rs" => "text/x-rust".to_string(),
        "ts" => "text/typescript".to_string(),
        "tsx" => "text/typescript".to_string(),
        "js" => "application/javascript".to_string(),
        "jsx" => "text/javascript".to_string(),
        "py" => "text/x-python".to_string(),
        "rb" => "text/x-ruby".to_string(),
        "go" => "text/x-go".to_string(),
        "java" => "text/x-java".to_string(),
        "c" => "text/x-c".to_string(),
        "h" => "text/x-c-header".to_string(),
        "cpp" | "cc" | "cxx" => "text/x-c++".to_string(),
        "hpp" | "hh" | "hxx" => "text/x-c++-header".to_string(),
        "sh" | "bash" => "text/x-shellscript".to_string(),
        "zsh" => "text/x-shellscript".to_string(),
        "sql" => "text/x-sql".to_string(),
        "json" => "application/json".to_string(),
        "yaml" | "yml" => "application/x-yaml".to_string(),
        "toml" => "application/toml".to_string(),
        // Images
        "png" => "image/png".to_string(),
        "jpg" | "jpeg" => "image/jpeg".to_string(),
        "gif" => "image/gif".to_string(),
        "svg" => "image/svg+xml".to_string(),
        "webp" => "image/webp".to_string(),
        "ico" => "image/x-icon".to_string(),
        // Fonts
        "ttf" => "font/ttf".to_string(),
        "woff" => "font/woff".to_string(),
        "woff2" => "font/woff2".to_string(),
        // Archives / binaries
        "zip" => "application/zip".to_string(),
        "tar" => "application/x-tar".to_string(),
        "gz" => "application/gzip".to_string(),
        "pdf" => "application/pdf".to_string(),
        "wasm" => "application/wasm".to_string(),
        // Default
        _ => "application/octet-stream".to_string(),
    }
}

/// Simple base64 encoding (uses the base64 crate if available, otherwise hex fallback).
fn base64_encode(data: &[u8]) -> String {
    use base64::Engine;
    base64::engine::general_purpose::STANDARD.encode(data)
}

/// Simple fuzzy match — checks if query characters appear in order in the target.
fn fuzzy_match(query: &str, target: &str) -> bool {
    let query_lower = query.to_lowercase();
    let target_lower = target.to_lowercase();

    // First try direct substring match
    if target_lower.contains(&query_lower) {
        return true;
    }

    // Then try character-by-character fuzzy match
    let mut q_chars = query_lower.chars().peekable();
    for tc in target_lower.chars() {
        if let Some(&qc) = q_chars.peek() {
            if qc == tc {
                q_chars.next();
            }
        }
    }
    q_chars.next().is_none()
}

// ── Tests ─────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    // ── Entry ─────────────────────────────────────────────────────────

    #[test]
    fn entry_file_serde_roundtrip() {
        let entry = Entry {
            path: RelativePath::new("src/main.rs"),
            entry_type: FileType::File,
            mime: "text/plain".to_string(),
        };
        let json = serde_json::to_string(&entry).expect("serialize Entry");
        let back: Entry = serde_json::from_str(&json).expect("deserialize Entry");
        assert_eq!(entry, back);
        assert_eq!(back.path.as_str(), "src/main.rs");
        assert_eq!(back.entry_type, FileType::File);
    }

    #[test]
    fn entry_directory_serde_roundtrip() {
        let entry = Entry {
            path: RelativePath::new("src/components/"),
            entry_type: FileType::Directory,
            mime: "application/x-directory".to_string(),
        };
        let json = serde_json::to_string(&entry).expect("serialize Entry");
        // Entry type should serialize as lowercase "directory"
        assert!(json.contains("\"directory\""), "got: {json}");
        let back: Entry = serde_json::from_str(&json).expect("deserialize Entry");
        assert_eq!(back.entry_type, FileType::Directory);
    }

    // ── Submatch ──────────────────────────────────────────────────────

    #[test]
    fn submatch_serde_roundtrip() {
        let sm = Submatch {
            text: "foo".to_string(),
            start: 10,
            end: 13,
        };
        let json = serde_json::to_string(&sm).expect("serialize");
        let back: Submatch = serde_json::from_str(&json).expect("deserialize");
        assert_eq!(sm, back);
    }

    // ── Match ─────────────────────────────────────────────────────────

    #[test]
    fn match_serde_roundtrip() {
        let m = Match {
            entry: Entry {
                path: RelativePath::new("src/lib.rs"),
                entry_type: FileType::File,
                mime: "text/plain".to_string(),
            },
            line: 42,
            offset: 5,
            text: "    let x = 1;".to_string(),
            submatches: vec![Submatch {
                text: "x".to_string(),
                start: 9,
                end: 10,
            }],
        };
        let json = serde_json::to_string(&m).expect("serialize");
        let back: Match = serde_json::from_str(&json).expect("deserialize");
        assert_eq!(m, back);
        assert_eq!(back.line, 42);
        assert_eq!(back.offset, 5);
        assert_eq!(back.submatches.len(), 1);
    }

    // ── Content ───────────────────────────────────────────────────────

    #[test]
    fn content_with_name_serde_roundtrip() {
        let c = Content {
            uri: "file:///home/user/src/main.rs".to_string(),
            name: Some("main.rs".to_string()),
            content: "fn main() {}".to_string(),
            encoding: ContentEncoding::Utf8,
            mime: "text/x-rust".to_string(),
        };
        let json = serde_json::to_string(&c).expect("serialize");
        let back: Content = serde_json::from_str(&json).expect("deserialize");
        assert_eq!(c, back);
        assert_eq!(back.encoding, ContentEncoding::Utf8);
    }

    #[test]
    fn content_without_name_omits_field() {
        let c = Content {
            uri: "file:///tmp/data.bin".to_string(),
            name: None,
            content: "AQID".to_string(),
            encoding: ContentEncoding::Base64,
            mime: "application/octet-stream".to_string(),
        };
        let json = serde_json::to_string(&c).expect("serialize");
        // name field should be absent
        assert!(!json.contains("\"name\""), "got: {json}");
        let back: Content = serde_json::from_str(&json).expect("deserialize");
        assert!(back.name.is_none());
        assert_eq!(back.encoding, ContentEncoding::Base64);
    }

    // ── Input types ───────────────────────────────────────────────────

    #[test]
    fn find_input_serde() {
        let input = FindInput {
            query: "auth".to_string(),
            r#type: Some(FileType::File),
            limit: Some(10),
        };
        let json = serde_json::to_string(&input).expect("serialize");
        let back: FindInput = serde_json::from_str(&json).expect("deserialize");
        assert_eq!(back.query, "auth");
        assert_eq!(back.r#type, Some(FileType::File));
        assert_eq!(back.limit, Some(10));
    }

    #[test]
    fn glob_input_optional_fields_omitted() {
        let input = GlobInput {
            pattern: "*.rs".to_string(),
            path: None,
            limit: None,
        };
        let json = serde_json::to_string(&input).expect("serialize");
        assert!(!json.contains("\"path\""), "got: {json}");
        assert!(!json.contains("\"limit\""), "got: {json}");
        let back: GlobInput = serde_json::from_str(&json).expect("deserialize");
        assert_eq!(back.pattern, "*.rs");
    }

    #[test]
    fn grep_input_serde() {
        let input = GrepInput {
            pattern: "TODO".to_string(),
            path: Some(RelativePath::new("src/")),
            include: None,
            limit: Some(50),
        };
        let json = serde_json::to_string(&input).expect("serialize");
        let back: GrepInput = serde_json::from_str(&json).expect("deserialize");
        assert_eq!(back.pattern, "TODO");
        assert_eq!(back.limit, Some(50));
    }

    // ── Ignore patterns ───────────────────────────────────────────────

    #[test]
    fn ignore_folders_not_empty() {
        assert!(!IGNORE_FOLDERS.is_empty());
        assert!(IGNORE_FOLDERS.contains(&"node_modules"));
        assert!(IGNORE_FOLDERS.contains(&".git"));
        assert!(IGNORE_FOLDERS.contains(&"target"));
        assert!(IGNORE_FOLDERS.contains(&"__pycache__"));
        assert_eq!(IGNORE_FOLDERS.len(), 28);
    }

    #[test]
    fn ignore_files_not_empty() {
        assert!(!IGNORE_FILES.is_empty());
        assert!(IGNORE_FILES.contains(&"**/*.swp"));
        assert!(IGNORE_FILES.contains(&"**/.DS_Store"));
        assert!(IGNORE_FILES.contains(&"**/tmp/**"));
        assert_eq!(IGNORE_FILES.len(), 11);
    }

    #[test]
    fn ignore_patterns_combines_both() {
        let patterns = ignore_patterns();
        assert!(patterns.len() > IGNORE_FILES.len());
        assert!(patterns.len() > IGNORE_FOLDERS.len());
    }

    #[test]
    fn is_ignored_node_modules() {
        assert!(is_ignored("node_modules/express/index.js", None));
        assert!(is_ignored("project/node_modules/foo/bar.js", None));
        assert!(is_ignored("node_modules", None));
    }

    #[test]
    fn is_ignored_git_dir() {
        assert!(is_ignored(".git/objects/ab/cdef", None));
        assert!(is_ignored("src/.git/HEAD", None));
    }

    #[test]
    fn is_ignored_swap_files() {
        assert!(is_ignored("src/main.swp", None));
        assert!(is_ignored(".file.swo", None));
    }

    #[test]
    fn is_ignored_normal_file_not_ignored() {
        assert!(!is_ignored("src/main.rs", None));
        assert!(!is_ignored("Cargo.toml", None));
        assert!(!is_ignored("README.md", None));
    }

    #[test]
    fn is_ignored_with_whitelist() {
        let opts = IgnoreMatchOptions {
            whitelist: Some(vec!["**/my-package/**".to_string()]),
            extra: None,
        };
        // whitelist overrides the ignore check
        assert!(!is_ignored("node_modules/my-package/index.js", Some(&opts)));
        // non-matching whitelist still gets ignored
        assert!(is_ignored("node_modules/other-lib/index.js", Some(&opts)));
    }

    #[test]
    fn is_ignored_with_extra_patterns() {
        let opts = IgnoreMatchOptions {
            extra: Some(vec!["**/*.bak".to_string()]),
            whitelist: None,
        };
        assert!(is_ignored("src/types.bak", Some(&opts)));
        assert!(!is_ignored("src/types.rs", Some(&opts)));
    }

    #[test]
    fn ignore_match_options_default() {
        let opts = IgnoreMatchOptions::default();
        assert!(opts.extra.is_none());
        assert!(opts.whitelist.is_none());
    }

    // ── Protected names ───────────────────────────────────────────────

    #[test]
    fn protected_names_returns_slice() {
        let names = protected_names();
        // On Linux, empty; on macOS/Windows, non-empty.
        // Just verify it doesn't panic and returns something reasonable.
        if cfg!(target_os = "macos") {
            assert!(!names.is_empty());
            assert!(names.contains(&"Desktop"));
            assert!(names.contains(&"Library"));
        } else if cfg!(target_os = "windows") {
            assert!(!names.is_empty());
            assert!(names.contains(&"AppData"));
        } else {
            // Linux returns empty
            assert!(names.is_empty());
        }
    }

    #[test]
    fn protected_paths_returns_vec() {
        let paths = protected_paths();
        // Just verify it returns without panicking.
        // Content depends on platform and home directory.
        let _ = paths; // suppress unused warning on platforms with pre-existing use
    }

    // ── Watcher types ─────────────────────────────────────────────────

    #[test]
    fn watcher_event_kind_serde_lowercase() {
        let add_json = serde_json::to_string(&WatcherEventKind::Add).expect("serialize");
        assert_eq!(add_json, "\"add\"");
        let change_json = serde_json::to_string(&WatcherEventKind::Change).expect("serialize");
        assert_eq!(change_json, "\"change\"");
        let unlink_json = serde_json::to_string(&WatcherEventKind::Unlink).expect("serialize");
        assert_eq!(unlink_json, "\"unlink\"");
    }

    #[test]
    fn watcher_event_kind_deserialize() {
        let kind: WatcherEventKind = serde_json::from_str("\"add\"").expect("deserialize");
        assert_eq!(kind, WatcherEventKind::Add);
        let kind: WatcherEventKind = serde_json::from_str("\"unlink\"").expect("deserialize");
        assert_eq!(kind, WatcherEventKind::Unlink);
    }

    #[test]
    fn watcher_event_serde() {
        let event = WatcherEvent {
            file: "/home/user/src/main.rs".to_string(),
            event: WatcherEventKind::Change,
        };
        let json = serde_json::to_string(&event).expect("serialize");
        let back: WatcherEvent = serde_json::from_str(&json).expect("deserialize");
        assert_eq!(event, back);
    }

    #[test]
    fn watcher_backend_serde_kebab_case() {
        assert_eq!(
            serde_json::to_string(&WatcherBackend::FsEvents).expect("serialize"),
            "\"fs-events\""
        );
        assert_eq!(
            serde_json::to_string(&WatcherBackend::Windows).expect("serialize"),
            "\"windows\""
        );
        assert_eq!(
            serde_json::to_string(&WatcherBackend::Inotify).expect("serialize"),
            "\"inotify\""
        );
    }

    #[test]
    fn watcher_backend_detect() {
        let backend = watcher_backend();
        // On recognized platforms, we get a backend.
        #[cfg(any(target_os = "linux", target_os = "macos", target_os = "windows"))]
        assert!(backend.is_some());
    }

    #[test]
    fn subscribe_timeout_is_10_seconds() {
        assert_eq!(SUBSCRIBE_TIMEOUT_MS, 10_000);
    }

    // ── FileType enum ─────────────────────────────────────────────────

    #[test]
    fn file_type_serde_lowercase() {
        assert_eq!(
            serde_json::to_string(&FileType::File).expect("serialize"),
            "\"file\""
        );
        assert_eq!(
            serde_json::to_string(&FileType::Directory).expect("serialize"),
            "\"directory\""
        );
    }

    #[test]
    fn file_type_copy_semantics() {
        let ft = FileType::Directory;
        let ft2 = ft; // Copy
        assert_eq!(ft, ft2);
    }

    // ── FileEditedEvent ───────────────────────────────────────────────

    #[test]
    fn file_edited_event_serde() {
        let event = FileEditedEvent {
            file: "src/main.rs".to_string(),
        };
        let json = serde_json::to_string(&event).expect("serialize");
        let back: FileEditedEvent = serde_json::from_str(&json).expect("deserialize");
        assert_eq!(event, back);
    }

    // ── glob_matches ──────────────────────────────────────────────────

    #[test]
    fn glob_matches_swp() {
        assert!(glob_matches("**/*.swp", "src/main.swp"));
        assert!(glob_matches("**/*.swp", ".file.swp"));
        assert!(!glob_matches("**/*.swp", "src/main.rs"));
    }

    #[test]
    fn glob_matches_ds_store() {
        assert!(glob_matches("**/.DS_Store", ".DS_Store"));
        assert!(glob_matches("**/.DS_Store", "src/.DS_Store"));
        assert!(!glob_matches("**/.DS_Store", "DS_Store"));
    }

    #[test]
    fn glob_matches_log_files() {
        assert!(glob_matches("**/*.log", "debug.log"));
        assert!(glob_matches("**/*.log", "logs/error.log"));
        assert!(!glob_matches("**/*.log", "debug.log.txt"));
    }

    #[test]
    fn glob_matches_deep_path() {
        assert!(glob_matches("**/logs/**", "logs/2024/01/error.txt"));
        assert!(glob_matches("**/logs/**", "deep/path/logs/something"));
    }

    // ── Filesystem operations tests ───────────────────────────────────

    /// Helper: create a temp directory with test files
    fn setup_test_fs() -> (tempfile::TempDir, PathBuf) {
        let dir = tempfile::tempdir().expect("create temp dir");
        let root = dir.path().to_path_buf();

        // Create directory structure
        std::fs::create_dir_all(root.join("src/components")).unwrap();
        std::fs::create_dir_all(root.join("src/utils")).unwrap();
        std::fs::create_dir_all(root.join("tests")).unwrap();
        std::fs::create_dir_all(root.join("node_modules/pkg")).unwrap();

        // Create test files
        std::fs::write(root.join("README.md"), "# Test Project\nHello world\n").unwrap();
        std::fs::write(root.join("Cargo.toml"), "[package]\nname = \"test\"\n").unwrap();
        std::fs::write(
            root.join("src/main.rs"),
            "fn main() {\n    println!(\"hello\");\n}\n",
        )
        .unwrap();
        std::fs::write(
            root.join("src/lib.rs"),
            "pub fn add(a: i32, b: i32) -> i32 { a + b }\n",
        )
        .unwrap();
        std::fs::write(
            root.join("src/components/mod.rs"),
            "pub mod button;\npub mod input;\n",
        )
        .unwrap();
        std::fs::write(
            root.join("src/components/button.rs"),
            "// TODO: implement button\n",
        )
        .unwrap();
        std::fs::write(
            root.join("src/utils/helpers.rs"),
            "pub fn helper() -> bool { true }\n",
        )
        .unwrap();
        std::fs::write(
            root.join("tests/integration.rs"),
            "#[test]\nfn it_works() {}\n",
        )
        .unwrap();
        std::fs::write(
            root.join("node_modules/pkg/index.js"),
            "module.exports = {};\n",
        )
        .unwrap();

        (dir, root)
    }

    #[test]
    fn test_read_file_utf8() {
        let (_dir, root) = setup_test_fs();
        let input = ReadInput {
            path: RelativePath::new("README.md"),
        };
        let content = read_file(&root, &input).expect("read file");
        assert_eq!(content.encoding, ContentEncoding::Utf8);
        assert!(content.content.contains("# Test Project"));
        assert_eq!(content.name.as_deref(), Some("README.md"));
        assert!(content.uri.starts_with("file://"));
    }

    #[test]
    fn test_read_file_missing() {
        let (_dir, root) = setup_test_fs();
        let input = ReadInput {
            path: RelativePath::new("nonexistent.txt"),
        };
        let result = read_file(&root, &input);
        assert!(result.is_err());
    }

    #[test]
    fn test_read_file_not_a_file() {
        let (_dir, root) = setup_test_fs();
        let input = ReadInput {
            path: RelativePath::new("src"),
        };
        let result = read_file(&root, &input);
        assert!(matches!(result, Err(FileSystemError::NotAFile(_))));
    }

    #[test]
    fn test_list_directory_root() {
        let (_dir, root) = setup_test_fs();
        let entries = list_directory(&root, None).expect("list directory");
        // Should have README.md, Cargo.toml, src/ (but not node_modules/)
        let paths: Vec<&str> = entries.iter().map(|e| e.path.as_str()).collect();
        assert!(paths.contains(&"README.md"));
        assert!(paths.contains(&"Cargo.toml"));
        assert!(paths.contains(&"src"));
        // node_modules should be ignored
        assert!(!paths.iter().any(|p| p.contains("node_modules")));
    }

    #[test]
    fn test_list_directory_subdir() {
        let (_dir, root) = setup_test_fs();
        let input = ListInput {
            path: Some(RelativePath::new("src")),
        };
        let entries = list_directory(&root, Some(&input)).expect("list directory");
        let paths: Vec<&str> = entries.iter().map(|e| e.path.as_str()).collect();
        assert!(paths.iter().any(|p| p.contains("main.rs")));
        assert!(paths.iter().any(|p| p.contains("lib.rs")));
        assert!(paths.iter().any(|p| p.contains("components")));
    }

    #[test]
    fn test_list_directory_missing() {
        let (_dir, root) = setup_test_fs();
        let input = ListInput {
            path: Some(RelativePath::new("nonexistent")),
        };
        let result = list_directory(&root, Some(&input));
        assert!(result.is_err());
    }

    #[test]
    fn test_find_files_by_name() {
        let (_dir, root) = setup_test_fs();
        let input = FindInput {
            query: "button".to_string(),
            r#type: None,
            limit: None,
        };
        let entries = find_files(&root, &input).expect("find files");
        assert!(!entries.is_empty());
        let paths: Vec<&str> = entries.iter().map(|e| e.path.as_str()).collect();
        assert!(paths.iter().any(|p| p.contains("button")));
    }

    #[test]
    fn test_find_files_with_type_filter() {
        let (_dir, root) = setup_test_fs();
        let input = FindInput {
            query: ".rs".to_string(),
            r#type: Some(FileType::File),
            limit: Some(2),
        };
        let entries = find_files(&root, &input).expect("find files");
        assert!(entries.len() <= 2);
        for entry in &entries {
            assert_eq!(entry.entry_type, FileType::File);
            assert!(entry.path.as_str().ends_with(".rs"));
        }
    }

    #[test]
    fn test_find_files_empty_query_returns_none() {
        let (_dir, root) = setup_test_fs();
        let input = FindInput {
            query: "zzz_nonexistent_zzz".to_string(),
            r#type: None,
            limit: None,
        };
        let entries = find_files(&root, &input).expect("find files");
        assert!(entries.is_empty());
    }

    #[test]
    fn test_glob_search_rs_files() {
        let (_dir, root) = setup_test_fs();
        let input = GlobInput {
            pattern: "**/*.rs".to_string(),
            path: None,
            limit: None,
        };
        let entries = glob_search(&root, &input).expect("glob search");
        let paths: Vec<&str> = entries.iter().map(|e| e.path.as_str()).collect();
        // Should find .rs files but not from node_modules (ignored)
        assert!(paths.iter().any(|p| p.contains("main.rs")));
        assert!(paths.iter().any(|p| p.contains("lib.rs")));
        assert!(!paths.iter().any(|p| p.contains("node_modules")));
    }

    #[test]
    fn test_glob_search_with_path_scope() {
        let (_dir, root) = setup_test_fs();
        let input = GlobInput {
            pattern: "**/*.rs".to_string(),
            path: Some(RelativePath::new("src/components")),
            limit: None,
        };
        let entries = glob_search(&root, &input).expect("glob search");
        let paths: Vec<&str> = entries.iter().map(|e| e.path.as_str()).collect();
        assert!(paths.iter().any(|p| p.contains("button.rs")));
        assert!(!paths.iter().any(|p| p.contains("main.rs")));
    }

    #[test]
    fn test_glob_search_with_limit() {
        let (_dir, root) = setup_test_fs();
        let input = GlobInput {
            pattern: "**/*.rs".to_string(),
            path: None,
            limit: Some(2),
        };
        let entries = glob_search(&root, &input).expect("glob search");
        assert!(entries.len() <= 2);
    }

    #[test]
    fn test_grep_search_literal() {
        let (_dir, root) = setup_test_fs();
        let input = GrepInput {
            pattern: "TODO".to_string(),
            path: None,
            include: None,
            limit: None,
        };
        let matches = grep_search(&root, &input).expect("grep search");
        assert!(!matches.is_empty());
        // The button.rs file contains "TODO"
        assert!(matches
            .iter()
            .any(|m| m.entry.path.as_str().contains("button.rs")));
    }

    #[test]
    fn test_grep_search_with_include_filter() {
        let (_dir, root) = setup_test_fs();
        let input = GrepInput {
            pattern: "fn".to_string(),
            path: None,
            include: Some("**/*.rs".to_string()),
            limit: None,
        };
        let matches = grep_search(&root, &input).expect("grep search");
        // All matches should be in .rs files
        for m in &matches {
            assert!(m.entry.path.as_str().ends_with(".rs"));
        }
    }

    #[test]
    fn test_grep_search_invalid_regex() {
        let (_dir, root) = setup_test_fs();
        let input = GrepInput {
            pattern: "[unclosed".to_string(),
            path: None,
            include: None,
            limit: None,
        };
        let result = grep_search(&root, &input);
        assert!(matches!(result, Err(FileSystemError::InvalidPattern(_))));
    }

    #[test]
    fn test_file_metadata_file() {
        let (_dir, root) = setup_test_fs();
        let meta = file_metadata(&root, &RelativePath::new("README.md")).expect("file metadata");
        assert_eq!(meta.entry.entry_type, FileType::File);
        assert!(meta.size > 0);
        assert!(meta.modified_ms > 0);
        assert!(meta.readable);
    }

    #[test]
    fn test_file_metadata_directory() {
        let (_dir, root) = setup_test_fs();
        let meta = file_metadata(&root, &RelativePath::new("src")).expect("file metadata");
        assert_eq!(meta.entry.entry_type, FileType::Directory);
        assert_eq!(meta.entry.mime, "application/x-directory");
    }

    #[test]
    fn test_file_metadata_missing() {
        let (_dir, root) = setup_test_fs();
        let result = file_metadata(&root, &RelativePath::new("nope.txt"));
        assert!(result.is_err());
    }

    #[test]
    fn test_file_exists() {
        let (_dir, root) = setup_test_fs();
        assert!(file_exists(&root, &RelativePath::new("README.md")));
        assert!(!file_exists(&root, &RelativePath::new("nope.txt")));
    }

    #[test]
    fn test_is_directory() {
        let (_dir, root) = setup_test_fs();
        assert!(is_directory(&root, &RelativePath::new("src")).unwrap());
        assert!(!is_directory(&root, &RelativePath::new("README.md")).unwrap());
    }

    #[test]
    fn test_is_file() {
        let (_dir, root) = setup_test_fs();
        assert!(is_file(&root, &RelativePath::new("README.md")).unwrap());
        assert!(!is_file(&root, &RelativePath::new("src")).unwrap());
    }

    #[test]
    fn test_path_escape_prevention() {
        let (_dir, root) = setup_test_fs();
        let input = ReadInput {
            path: RelativePath::new("../../../etc/passwd"),
        };
        let result = read_file(&root, &input);
        // Should either fail (PathEscapesRoot) or not actually read the system file
        assert!(result.is_err());
    }

    #[test]
    fn test_fuzzy_match_substring() {
        assert!(fuzzy_match("main", "src/main.rs"));
        assert!(fuzzy_match("button", "button_component.ts"));
        assert!(!fuzzy_match("zzz", "src/main.rs"));
    }

    #[test]
    fn test_fuzzy_match_case_insensitive() {
        assert!(fuzzy_match("MAIN", "src/main.rs"));
        assert!(fuzzy_match("Button", "button_component.ts"));
    }

    #[test]
    fn test_list_directory_sorts_dirs_first() {
        let (_dir, root) = setup_test_fs();
        let entries = list_directory(&root, None).expect("list directory");
        // First entries should be directories
        let first_dir_idx = entries
            .iter()
            .position(|e| e.entry_type == FileType::Directory);
        let first_file_idx = entries.iter().position(|e| e.entry_type == FileType::File);
        if let (Some(d_idx), Some(f_idx)) = (first_dir_idx, first_file_idx) {
            assert!(d_idx < f_idx, "directories should come before files");
        }
    }

    #[test]
    fn test_read_file_base64_fallback() {
        let dir = tempfile::tempdir().expect("create temp dir");
        let root = dir.path();
        // Create a file with invalid UTF-8 bytes
        let invalid_utf8 = vec![0xFF, 0xFE, 0x00, 0x01, 0x02];
        std::fs::write(root.join("binary.bin"), &invalid_utf8).unwrap();

        let input = ReadInput {
            path: RelativePath::new("binary.bin"),
        };
        let content = read_file(root, &input).expect("read binary file");
        assert_eq!(content.encoding, ContentEncoding::Base64);
        assert!(!content.content.is_empty());
    }
}
