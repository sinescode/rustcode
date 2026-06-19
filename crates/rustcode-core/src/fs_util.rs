//! Filesystem utility types — path manipulation, MIME type detection, glob helpers.
//!
//! Ported from: `packages/core/src/fs-util.ts` (252 lines)
//! OpenCode commit: 5d0f86606ac30690f79f0a6a9f41a1f49fe95d0b

use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};

/// Error type for filesystem operations.
///
/// Ported from: `fs-util.ts` — `FSUtil.FileSystemError`
#[derive(Debug, Clone, Serialize, Deserialize, thiserror::Error)]
#[error("filesystem error in {method}: {cause}")]
pub struct FileSystemError {
    /// The method that failed (e.g., "readDirectoryEntries", "glob")
    pub method: String,
    /// Optional underlying cause message
    pub cause: Option<String>,
}

/// A directory entry returned by read_directory.
///
/// Ported from: `fs-util.ts` — `FSUtil.DirEntry`
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct DirEntry {
    /// The name of the entry (not full path)
    pub name: String,
    /// The type of entry
    #[serde(rename = "type")]
    pub entry_type: DirEntryType,
}

/// Type of a directory entry.
///
/// Ported from: `fs-util.ts` — `DirEntry.type`
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum DirEntryType {
    /// Regular file
    File,
    /// Directory
    Directory,
    /// Symbolic link
    Symlink,
    /// Other (socket, FIFO, etc.)
    Other,
}

/// Options for glob pattern matching.
///
/// Ported from: `fs-util.ts` + `util/glob.ts`
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct GlobOptions {
    /// Working directory for relative patterns
    pub cwd: Option<PathBuf>,
    /// If true, return absolute paths
    pub absolute: bool,
    /// What to include in results
    pub include: Option<GlobInclude>,
    /// If true, include dotfiles
    pub dot: bool,
    /// Maximum depth to search
    pub max_depth: Option<usize>,
    /// Patterns to ignore
    pub ignore: Vec<String>,
}

/// What types of entries to include in glob results.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum GlobInclude {
    /// Only files
    File,
    /// Only directories
    Directory,
    /// Both files and directories
    All,
}

/// Options for the `find_up`/`up` functions.
///
/// Ported from: `fs-util.ts` — `up()` options
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FindUpOptions {
    /// Files/directories to search for
    pub targets: Vec<String>,
    /// Directory to start searching from
    pub start: PathBuf,
    /// Optional directory to stop at (inclusive)
    pub stop: Option<PathBuf>,
}

// ---------------------------------------------------------------------------
// Pure path manipulation helpers (no Effect needed)
// Ported from: fs-util.ts — bottom section
// ---------------------------------------------------------------------------

/// Guess MIME type from a file extension.
///
/// Ported from: `fs-util.ts` — `mimeType()`
///
/// Falls back to `"application/octet-stream"` for unknown extensions.
pub fn mime_type(path: &Path) -> String {
    let ext = path
        .extension()
        .and_then(|e| e.to_str())
        .unwrap_or("");
    match ext {
        "txt" => "text/plain".into(),
        "html" | "htm" => "text/html".into(),
        "css" => "text/css".into(),
        "js" => "application/javascript".into(),
        "json" => "application/json".into(),
        "xml" => "application/xml".into(),
        "png" => "image/png".into(),
        "jpg" | "jpeg" => "image/jpeg".into(),
        "gif" => "image/gif".into(),
        "svg" => "image/svg+xml".into(),
        "webp" => "image/webp".into(),
        "ico" => "image/x-icon".into(),
        "pdf" => "application/pdf".into(),
        "zip" => "application/zip".into(),
        "tar" => "application/x-tar".into(),
        "gz" => "application/gzip".into(),
        "mp3" => "audio/mpeg".into(),
        "mp4" => "video/mp4".into(),
        "wav" => "audio/wav".into(),
        "woff" => "font/woff".into(),
        "woff2" => "font/woff2".into(),
        "ttf" => "font/ttf".into(),
        "otf" => "font/otf".into(),
        "md" => "text/markdown".into(),
        "yaml" | "yml" => "application/yaml".into(),
        "toml" => "application/toml".into(),
        "rs" => "text/x-rust".into(),
        "ts" => "text/typescript".into(),
        "tsx" => "text/typescript-jsx".into(),
        "py" => "text/x-python".into(),
        "sh" | "bash" => "text/x-shellscript".into(),
        "sql" => "application/sql".into(),
        "wasm" => "application/wasm".into(),
        _ => "application/octet-stream".into(),
    }
}

/// Normalize a path for the current platform.
///
/// Ported from: `fs-util.ts` — `normalizePath()`
///
/// On Unix: identity (path separators are already correct).
/// On Windows: resolves WSL/cygdrive-style paths to Windows paths.
pub fn normalize_path(p: &Path) -> PathBuf {
    // On Unix, paths are already normalized (canonicalize if needed)
    if cfg!(windows) {
        let resolved = windows_path(p);
        match std::fs::canonicalize(&resolved) {
            Ok(canon) => canon,
            Err(_) => resolved,
        }
    } else {
        p.to_path_buf()
    }
}

/// Convert a Unix/WSL-style path to a Windows path.
///
/// Ported from: `fs-util.ts` — `windowsPath()`
///
/// Handles:
/// - `/c:/...` → `C:/...`
/// - `/c/...` → `C:/...`
/// - `/cygdrive/c/...` → `C:/...`
/// - `/mnt/c/...` → `C:/...`
pub fn windows_path(p: &Path) -> PathBuf {
    if !cfg!(windows) {
        return p.to_path_buf();
    }
    let s = p.to_string_lossy();

    // Helper: check if a char is a drive letter (a-z, A-Z)
    fn is_drive(c: char) -> bool {
        c.is_ascii_alphabetic()
    }

    // Pattern: /X:/... or /X:\... → X:/...
    if s.len() >= 3 && s.starts_with('/') && is_drive(s.as_bytes()[1] as char) && s.as_bytes()[2] == b':' {
        let drive = s[1..2].to_uppercase();
        let rest = &s[3..];
        return PathBuf::from(format!("{drive}:/{rest}"));
    }
    // Pattern: /X/... (single drive letter followed by /) → X:/...
    if s.len() >= 3
        && s.starts_with('/')
        && is_drive(s.as_bytes()[1] as char)
        && s.as_bytes()[2] == b'/'
    {
        let drive = s[1..2].to_uppercase();
        let rest = &s[3..];
        return PathBuf::from(format!("{drive}:/{rest}"));
    }
    // Pattern: /cygdrive/X/... → X:/...
    if s.starts_with("/cygdrive/") && s.len() >= 12 && is_drive(s.as_bytes()[10] as char) {
        let drive = s[10..11].to_uppercase();
        let rest = if s.len() > 11 { &s[11..].trim_start_matches('/') } else { "" };
        return PathBuf::from(format!("{drive}:/{rest}"));
    }
    // Pattern: /mnt/X/... → X:/...
    if s.starts_with("/mnt/") && s.len() >= 7 && is_drive(s.as_bytes()[5] as char) {
        let drive = s[5..6].to_uppercase();
        let rest = if s.len() > 6 { &s[6..].trim_start_matches('/') } else { "" };
        return PathBuf::from(format!("{drive}:/{rest}"));
    }
    PathBuf::from(s.into_owned())
}

/// Resolve a path, following symlinks (like `realpath`).
///
/// Ported from: `fs-util.ts` — `resolve()`
pub fn resolve_path(p: &Path) -> PathBuf {
    match std::fs::canonicalize(p) {
        Ok(canon) => normalize_path(&canon),
        Err(_) => normalize_path(p),
    }
}

/// Normalize a glob pattern for the current platform.
///
/// Ported from: `fs-util.ts` — `normalizePathPattern()`
pub fn normalize_path_pattern(p: &str) -> String {
    if !cfg!(windows) {
        return p.to_string();
    }
    if p == "*" {
        return "*".to_string();
    }
    // Check if ends with \*/ or /* pattern
    if p.ends_with("\\*") || p.ends_with("/*") {
        let dir = &p[..p.len() - 2];
        format!("{}{}*", normalize_path(Path::new(dir)).display(), std::path::MAIN_SEPARATOR)
    } else {
        normalize_path(Path::new(p)).display().to_string()
    }
}

/// Check if two paths overlap (one contains the other).
///
/// Ported from: `fs-util.ts` — `overlaps()`
pub fn overlaps(a: &Path, b: &Path) -> bool {
    contains(a, b) || contains(b, a)
}

/// Ensure a directory exists, creating it recursively if necessary.
pub fn ensure_dir(path: &Path) -> std::io::Result<()> {
    std::fs::create_dir_all(path)
}

/// Check if `parent` path contains `child` path.
///
/// Ported from: `fs-util.ts` — `contains()`
pub fn contains(parent: &Path, child: &Path) -> bool {
    let Ok(rel) = child.strip_prefix(parent) else {
        return false;
    };
    // If relative path is empty → same directory
    // If relative path starts with .. → not contained
    if rel.as_os_str().is_empty() {
        return true;
    }
    // Check that it doesn't start with ..
    !rel.starts_with("..")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_mime_type_known_extensions() {
        assert_eq!(mime_type(Path::new("test.txt")), "text/plain");
        assert_eq!(mime_type(Path::new("test.html")), "text/html");
        assert_eq!(mime_type(Path::new("test.json")), "application/json");
        assert_eq!(mime_type(Path::new("test.png")), "image/png");
        assert_eq!(mime_type(Path::new("test.jpg")), "image/jpeg");
        assert_eq!(mime_type(Path::new("test.jpeg")), "image/jpeg");
        assert_eq!(mime_type(Path::new("test.pdf")), "application/pdf");
    }

    #[test]
    fn test_mime_type_unknown_extension() {
        assert_eq!(
            mime_type(Path::new("test.unknown_ext")),
            "application/octet-stream"
        );
    }

    #[test]
    fn test_mime_type_no_extension() {
        assert_eq!(mime_type(Path::new("test")), "application/octet-stream");
    }

    #[test]
    fn test_contains_same_path() {
        assert!(contains(Path::new("/foo"), Path::new("/foo")));
    }

    #[test]
    fn test_contains_child() {
        assert!(contains(Path::new("/foo"), Path::new("/foo/bar")));
    }

    #[test]
    fn test_contains_deep_child() {
        assert!(contains(Path::new("/foo"), Path::new("/foo/bar/baz")));
    }

    #[test]
    fn test_contains_not_parent() {
        assert!(!contains(Path::new("/foo"), Path::new("/other")));
    }

    #[test]
    fn test_contains_sibling_not_contained() {
        assert!(!contains(Path::new("/foo/bar"), Path::new("/foo/baz")));
    }

    #[test]
    fn test_overlaps_same() {
        assert!(overlaps(Path::new("/foo"), Path::new("/foo")));
    }

    #[test]
    fn test_overlaps_parent_child() {
        assert!(overlaps(Path::new("/foo"), Path::new("/foo/bar")));
        assert!(overlaps(Path::new("/foo/bar"), Path::new("/foo")));
    }

    #[test]
    fn test_overlaps_no_overlap() {
        assert!(!overlaps(Path::new("/foo"), Path::new("/bar")));
    }

    #[test]
    fn test_dir_entry_type_serde() {
        let file_entry = DirEntry {
            name: "test.rs".into(),
            entry_type: DirEntryType::File,
        };
        let json = serde_json::to_string(&file_entry).expect("serialize");
        let parsed: DirEntry = serde_json::from_str(&json).expect("deserialize");
        assert_eq!(parsed.entry_type, DirEntryType::File);
        assert_eq!(parsed.name, "test.rs");
    }

    #[test]
    fn test_glob_options_default() {
        let opts = GlobOptions::default();
        assert!(!opts.absolute);
        assert!(!opts.dot);
        assert!(opts.cwd.is_none());
        assert!(opts.ignore.is_empty());
    }

    #[test]
    fn test_glob_include_serde() {
        assert_eq!(
            serde_json::to_string(&GlobInclude::File).expect("serialize"),
            r#""file""#
        );
        assert_eq!(
            serde_json::to_string(&GlobInclude::Directory).expect("serialize"),
            r#""directory""#
        );
        let parsed: GlobInclude =
            serde_json::from_str(r#""all""#).expect("deserialize");
        assert_eq!(parsed, GlobInclude::All);
    }

    #[test]
    fn test_filesystem_error_display() {
        let err = FileSystemError {
            method: "readDirectoryEntries".into(),
            cause: Some("permission denied".into()),
        };
        assert!(err.to_string().contains("readDirectoryEntries"));
        assert!(err.to_string().contains("permission denied"));
    }

    #[test]
    fn test_normalize_path_pattern_star() {
        assert_eq!(normalize_path_pattern("*"), "*");
    }

    #[test]
    fn test_dir_entry_serde_roundtrip() {
        for (name, entry_type) in [
            ("file.rs", DirEntryType::File),
            ("src", DirEntryType::Directory),
            ("link", DirEntryType::Symlink),
            ("socket", DirEntryType::Other),
        ] {
            let entry = DirEntry {
                name: name.into(),
                entry_type,
            };
            let json = serde_json::to_string(&entry).expect("serialize");
            let back: DirEntry = serde_json::from_str(&json).expect("deserialize");
            assert_eq!(back.name, name);
            assert_eq!(back.entry_type, entry.entry_type);
        }
    }

    #[test]
    fn test_ensure_dir_creates_nested() {
        let tmp = std::env::temp_dir().join("rustcode_test_ensure_dir_nested");
        let nested = tmp.join("a").join("b").join("c");
        ensure_dir(&nested).expect("create nested dir");
        assert!(nested.is_dir());
        std::fs::remove_dir_all(&tmp).ok();
    }

    #[test]
    fn test_ensure_dir_existing() {
        let tmp = std::env::temp_dir().join("rustcode_test_ensure_dir_existing");
        ensure_dir(&tmp).expect("create dir");
        // Second call should succeed (already exists)
        ensure_dir(&tmp).expect("ensure existing dir");
        std::fs::remove_dir_all(&tmp).ok();
    }
}
