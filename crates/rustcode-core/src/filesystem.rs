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
    let extra: &[String] = opts
        .and_then(|o| o.extra.as_deref())
        .unwrap_or(&[]);
    for pattern in IGNORE_FILES.iter().map(|s| *s).chain(extra.iter().map(|s| s.as_str())) {
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
            whitelist: Some(vec!["node_modules/my-package/**".to_string()]),
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
            extra: Some(vec!["**/*.gen.rs".to_string()]),
            whitelist: None,
        };
        assert!(is_ignored("src/types.gen.rs", Some(&opts)));
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
        assert!(glob_matches(
            "**/logs/**",
            "logs/2024/01/error.txt"
        ));
        assert!(glob_matches(
            "**/logs/**",
            "deep/path/logs/something"
        ));
    }
}
