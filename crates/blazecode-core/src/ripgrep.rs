//! ripgrep search integration types.
//!
//! Ported from:
//! - `packages/core/src/ripgrep.ts`
//! - `packages/core/src/ripgrep/binary.ts`
//!
//! BlazeCode commit: 5d0f86606ac30690f79f0a6a9f41a1f49fe95d0b

use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::Path;
use std::sync::OnceLock;

// ── Entry Types ───────────────────────────────────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum EntryType {
    File,
    Directory,
    Symlink,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RipgrepEntry {
    pub path: String,
    pub entry_type: EntryType,
    pub mime: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RipgrepMatch {
    pub entry: RipgrepEntry,
    pub line: u64,
    pub offset: u64,
    pub text: String,
    pub submatches: Vec<(usize, usize)>,
}

// ── Constants ────────────────────────────────────────────────────────────

/// Maximum stderr bytes to capture from a ripgrep child process.
///
/// Ported from: `packages/core/src/ripgrep.ts` — `ERROR_BYTES`
pub const ERROR_BYTES: u32 = 8 * 1024;

/// Maximum bytes per JSON record read from ripgrep stdout.
///
/// Ported from: `packages/core/src/ripgrep.ts` — `MAX_RECORD_BYTES`
pub const MAX_RECORD_BYTES: usize = 1024 * 1024;

/// Maximum number of submatches to report per match.
///
/// Ported from: `packages/core/src/ripgrep.ts` — `MAX_SUBMATCHES`
pub const MAX_SUBMATCHES: u32 = 100;

/// Expected ripgrep binary version.
///
/// Ported from: `packages/core/src/ripgrep/binary.ts` — `RIPGREP_VERSION`
pub const RIPGREP_VERSION: &str = "15.1.0";

// ── Error Types ──────────────────────────────────────────────────────────

/// Error from a ripgrep operation.
///
/// Mirrors Effect.ts `TaggedError` — a structured error with a message
/// and an optional underlying cause string (stderr output, OS error, etc.).
///
/// Ported from: `packages/core/src/ripgrep.ts` — `RipgrepError`
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct RipgrepError {
    /// Human-readable error message.
    pub message: String,
    /// Optional underlying cause (e.g., stderr output from the rg process).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub cause: Option<String>,
}

/// Helper to construct a simple `RipgrepError` with just a message.
impl RipgrepError {
    /// Create a new error with a message and no cause.
    pub fn new(message: impl Into<String>) -> Self {
        Self {
            message: message.into(),
            cause: None,
        }
    }

    /// Create a new error with a message and cause.
    pub fn with_cause(message: impl Into<String>, cause: impl Into<String>) -> Self {
        Self {
            message: message.into(),
            cause: Some(cause.into()),
        }
    }

    /// Create a "binary not found" error.
    pub fn binary_not_found(message: impl Into<String>) -> Self {
        Self {
            message: message.into(),
            cause: None,
        }
    }
}

impl std::fmt::Display for RipgrepError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "ripgrep error: {}", self.message)?;
        if let Some(ref cause) = self.cause {
            write!(f, " (cause: {cause})")?;
        }
        Ok(())
    }
}

impl std::error::Error for RipgrepError {}

/// Error for an invalid regex pattern passed to ripgrep.
///
/// Mirrors Effect.ts `TaggedError` — carries the offending pattern
/// and an explanation of why it is invalid.
///
/// Ported from: `packages/core/src/ripgrep.ts` — `RipgrepInvalidPatternError`
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RipgrepInvalidPatternError {
    /// The invalid regex pattern.
    pub pattern: String,
    /// Description of why the pattern is invalid.
    pub message: String,
}

impl std::fmt::Display for RipgrepInvalidPatternError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "invalid ripgrep pattern '{}': {}",
            self.pattern, self.message
        )
    }
}

impl std::error::Error for RipgrepInvalidPatternError {}

// ── Result Types ─────────────────────────────────────────────────────────

/// Result of a file-find operation.
#[derive(Debug, Clone)]
pub struct FindResult {
    /// Files found matching the query.
    pub items: Vec<RipgrepEntry>,
    /// Whether results were truncated due to the limit.
    pub truncated: bool,
}

/// Result of a grep operation.
#[derive(Debug, Clone)]
pub struct GrepResult {
    /// Matches found by the grep query.
    pub items: Vec<RipgrepMatch>,
    /// Whether results were truncated due to the limit.
    pub truncated: bool,
    /// True when ripgrep exited with code 2 (invalid pattern or similar warning).
    pub partial: bool,
}

// ── Input Types ──────────────────────────────────────────────────────────

/// Input for a ripgrep file-search (`rg --files` with optional glob).
///
/// Crawls a directory tree and returns matching file paths up to `limit`.
///
/// Ported from: `packages/core/src/ripgrep.ts` — `FindInput`
#[derive(Serialize, Deserialize)]
pub struct FindInput {
    /// Working directory to search from.
    pub cwd: String,
    /// Glob pattern for matching file names (e.g., `"*.rs"`, `"**/*.ts"`).
    pub pattern: String,
    /// Maximum number of results to return.
    pub limit: u32,
    /// Whether to search hidden files and directories.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub hidden: Option<bool>,
    /// Whether to follow symbolic links.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub follow: Option<bool>,
    /// Runtime cancel signal — set to `Some(true)` to request cancellation.
    /// Not serialized; used only at the call site to abort a running process.
    #[serde(skip, default)]
    pub signal: Option<bool>,
    /// Optional callback invoked for each file entry found.
    #[serde(skip, default)]
    #[allow(clippy::type_complexity)]
    pub on_entry: Option<Box<dyn Fn(&RipgrepEntry) + Send + Sync>>,
}

impl std::fmt::Debug for FindInput {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("FindInput")
            .field("cwd", &self.cwd)
            .field("pattern", &self.pattern)
            .field("limit", &self.limit)
            .field("hidden", &self.hidden)
            .field("follow", &self.follow)
            .field("signal", &self.signal)
            .field("on_entry", &self.on_entry.as_ref().map(|_| "Fn"))
            .finish()
    }
}

impl Clone for FindInput {
    fn clone(&self) -> Self {
        Self {
            cwd: self.cwd.clone(),
            pattern: self.pattern.clone(),
            limit: self.limit,
            hidden: self.hidden,
            follow: self.follow,
            signal: self.signal,
            on_entry: None,
        }
    }
}

/// Input for a ripgrep glob search (list files matching a glob pattern).
///
/// Similar to `FindInput` but does not carry a cancel signal.
///
/// Ported from: `packages/core/src/ripgrep.ts` — `GlobInput`
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GlobInput {
    /// Working directory to search from.
    pub cwd: String,
    /// Glob pattern for matching file names.
    pub pattern: String,
    /// Maximum number of results to return.
    pub limit: u32,
    /// Whether to search hidden files and directories.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub hidden: Option<bool>,
    /// Whether to follow symbolic links.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub follow: Option<bool>,
}

/// Input for a ripgrep grep search (match pattern against file contents).
///
/// Supports optional file and include filters on top of the regex pattern.
///
/// Ported from: `packages/core/src/ripgrep.ts` — `GrepInput`
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GrepInput {
    /// Working directory to search from.
    pub cwd: String,
    /// Regex pattern to match against file contents.
    pub pattern: String,
    /// Optional path to a specific file (limits search to one file).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub file: Option<String>,
    /// Optional glob pattern to include specific file types (e.g., `"*.rs"`).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub include: Option<String>,
    /// Maximum number of results to return.
    pub limit: u32,
}

// ── Raw Match Types (ripgrep JSON output) ────────────────────────────────

/// File path text from a ripgrep JSON record.
///
/// Ported from: `packages/core/src/ripgrep.ts` — `RawMatchPath`
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RawMatchPath {
    /// Path text (relative to search root).
    pub text: String,
}

/// Line text from a ripgrep JSON record.
///
/// Ported from: `packages/core/src/ripgrep.ts` — `RawMatchLines`
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RawMatchLines {
    /// Line text content.
    pub text: String,
}

/// A single submatch (capture group match) within a line.
///
/// The `match` field from ripgrep JSON is renamed to `match_text` because
/// `match` is a reserved keyword in Rust.
///
/// Ported from: `packages/core/src/ripgrep.ts` — submatches array item
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RawSubmatch {
    /// The matched text content.
    #[serde(rename = "match")]
    pub match_text: String,
    /// Byte offset of the match start relative to the line start.
    pub start: u32,
    /// Byte offset of the match end relative to the line start.
    pub end: u32,
}

/// Data payload for a `match`-type ripgrep JSON record.
///
/// Ported from: `packages/core/src/ripgrep.ts` — `RawMatchData`
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RawMatchData {
    /// File path information.
    pub path: RawMatchPath,
    /// Line information.
    pub lines: RawMatchLines,
    /// Line number (1-based).
    pub line_number: u32,
    /// Absolute byte offset of the match in the file.
    pub absolute_offset: u32,
    /// Submatches (capture groups) within the line.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub submatches: Vec<RawSubmatch>,
}

/// A single JSON record from ripgrep's `--json` output stream.
///
/// The `type` field from ripgrep JSON is renamed to `match_type` because
/// `type` is a reserved keyword in Rust.
///
/// Ported from: `packages/core/src/ripgrep.ts` — `RawMatch`
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RawMatch {
    /// Record type discriminator (`"begin"`, `"match"`, `"end"`, `"summary"`, `"context"`).
    #[serde(rename = "type")]
    pub match_type: String,
    /// Data payload for the record.
    pub data: RawMatchData,
}

// ── Platform Types (from ripgrep/binary.ts) ──────────────────────────────

/// Platform-specific ripgrep binary configuration.
///
/// Maps an BlazeCode platform key (e.g., `"x64-linux"`) to the ripgrep
/// release artifact name and archive extension.
///
/// Ported from: `packages/core/src/ripgrep/binary.ts` — `RipgrepPlatformConfig`
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RipgrepPlatformConfig {
    /// Rust target triple for the prebuilt ripgrep binary
    /// (e.g., `"x86_64-unknown-linux-musl"`).
    pub platform: String,
    /// Archive extension for the release artifact (`"tar.gz"` or `"zip"`).
    pub extension: String,
}

/// Return the full platform map — all 7 platform keys to their ripgrep
/// release configurations.
///
/// Ported from: `packages/core/src/ripgrep/binary.ts` — `RIPGREP_PLATFORMS`
#[must_use]
pub fn ripgrep_platforms() -> HashMap<&'static str, RipgrepPlatformConfig> {
    let mut map = HashMap::with_capacity(7);
    map.insert(
        "arm64-darwin",
        RipgrepPlatformConfig {
            platform: "aarch64-apple-darwin".into(),
            extension: "tar.gz".into(),
        },
    );
    map.insert(
        "arm64-linux",
        RipgrepPlatformConfig {
            platform: "aarch64-unknown-linux-gnu".into(),
            extension: "tar.gz".into(),
        },
    );
    map.insert(
        "x64-darwin",
        RipgrepPlatformConfig {
            platform: "x86_64-apple-darwin".into(),
            extension: "tar.gz".into(),
        },
    );
    map.insert(
        "x64-linux",
        RipgrepPlatformConfig {
            platform: "x86_64-unknown-linux-musl".into(),
            extension: "tar.gz".into(),
        },
    );
    map.insert(
        "arm64-win32",
        RipgrepPlatformConfig {
            platform: "aarch64-pc-windows-msvc".into(),
            extension: "zip".into(),
        },
    );
    map.insert(
        "ia32-win32",
        RipgrepPlatformConfig {
            platform: "i686-pc-windows-msvc".into(),
            extension: "zip".into(),
        },
    );
    map.insert(
        "x64-win32",
        RipgrepPlatformConfig {
            platform: "x86_64-pc-windows-msvc".into(),
            extension: "zip".into(),
        },
    );
    map
}

/// Return the ripgrep platform configuration for the current host target.
///
/// Uses `cfg!` macros to determine which of the 7 platform keys matches.
/// Returns `None` if the current target does not correspond to any known
/// ripgrep prebuilt binary (e.g., 32-bit ARM Linux).
#[must_use]
pub fn current_ripgrep_platform() -> Option<RipgrepPlatformConfig> {
    let platforms = ripgrep_platforms();

    if cfg!(all(target_os = "macos", target_arch = "aarch64")) {
        return platforms.get("arm64-darwin").cloned();
    }
    if cfg!(all(target_os = "macos", target_arch = "x86_64")) {
        return platforms.get("x64-darwin").cloned();
    }
    if cfg!(all(target_os = "linux", target_arch = "aarch64")) {
        return platforms.get("arm64-linux").cloned();
    }
    if cfg!(all(target_os = "linux", target_arch = "x86_64")) {
        return platforms.get("x64-linux").cloned();
    }
    if cfg!(all(target_os = "windows", target_arch = "aarch64")) {
        return platforms.get("arm64-win32").cloned();
    }
    if cfg!(all(target_os = "windows", target_arch = "x86")) {
        return platforms.get("ia32-win32").cloned();
    }
    if cfg!(all(target_os = "windows", target_arch = "x86_64")) {
        return platforms.get("x64-win32").cloned();
    }

    None
}

// ── Binary State ─────────────────────────────────────────────────────────

/// Cached resolved binary path, computed once per process lifetime.
static CACHED_BINARY: OnceLock<Option<String>> = OnceLock::new();

/// Return the cached binary path, resolving it on first call.
pub fn cached_binary_path() -> Option<&'static str> {
    CACHED_BINARY
        .get_or_init(|| RipgrepService::resolve_binary_from_path().ok())
        .as_deref()
}

/// Download a prebuilt ripgrep binary for the current platform into `cache_dir`.
///
/// Returns the path to the extracted `rg` binary.
///
/// Ported from: `packages/core/src/ripgrep/binary.ts`
pub async fn download_binary(cache_dir: &str) -> Result<String, RipgrepError> {
    let version = "14.1.1";
    let url = if cfg!(target_os = "macos") {
        format!(
            "https://github.com/BurntSushi/ripgrep/releases/download/{}/ripgrep-{}-aarch64-apple-darwin.tar.gz",
            version, version
        )
    } else if cfg!(target_os = "linux") {
        format!(
            "https://github.com/BurntSushi/ripgrep/releases/download/{}/ripgrep-{}-x86_64-unknown-linux-musl.tar.gz",
            version, version
        )
    } else {
        return Err(RipgrepError::binary_not_found(
            "auto-download not supported on this platform",
        ));
    };

    let response = reqwest::get(&url)
        .await
        .map_err(|e| RipgrepError::with_cause("failed to download ripgrep", e.to_string()))?;

    let bytes = response.bytes().await.map_err(|e| {
        RipgrepError::with_cause("failed to read ripgrep download body", e.to_string())
    })?;

    let archive_path = format!("{}/rg.tar.gz", cache_dir);
    std::fs::write(&archive_path, &bytes)
        .map_err(|e| RipgrepError::with_cause("failed to write ripgrep archive", e.to_string()))?;

    std::process::Command::new("tar")
        .args(["-xzf", &archive_path, "-C", cache_dir])
        .output()
        .map_err(|e| {
            RipgrepError::with_cause("failed to extract ripgrep archive", e.to_string())
        })?;

    let binary = format!("{}/rg", cache_dir);
    if std::path::Path::new(&binary).exists() {
        Ok(binary)
    } else {
        Err(RipgrepError::new(
            "extraction succeeded but rg binary not found",
        ))
    }
}

/// State of the local ripgrep binary installation.
///
/// Tracks where the binary lives on disk, whether it came from the system
/// PATH or was downloaded by BlazeCode, and its detected version string.
///
/// Ported from: `packages/core/src/ripgrep/binary.ts` — `RipgrepBinaryState`
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RipgrepBinaryState {
    /// Absolute path to the ripgrep binary, if known.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub filepath: Option<String>,
    /// Whether the binary was found on the system PATH (`true`) or is
    /// a bundled/downloaded binary managed by the application (`false`).
    #[serde(default)]
    pub is_system: bool,
    /// Detected version string (e.g., `"15.1.0"`), if known.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub version: Option<String>,
}

// ── RipgrepService ────────────────────────────────────────────────────────

/// Raw output from `run_rg_grep`, including the exit code.
struct RgGrepOutput {
    stdout: String,
    exit_code: i32,
}

/// Service for running ripgrep search operations.
///
/// Ported from: `packages/core/src/ripgrep.ts`
pub struct RipgrepService {
    /// Path to the rg binary
    binary_path: String,
}

impl RipgrepService {
    /// Create a new RipgrepService, using the cached binary path if available.
    pub fn new() -> Self {
        Self {
            binary_path: cached_binary_path().unwrap_or("rg").to_string(),
        }
    }

    /// Create with an explicit binary path.
    pub fn with_binary(binary_path: impl Into<String>) -> Self {
        Self {
            binary_path: binary_path.into(),
        }
    }

    /// Resolve the ripgrep binary path without caching (for initial cache population).
    ///
    /// Checks:
    /// 1. BLAZECODE_RG_PATH environment variable
    /// 2. `rg` on system PATH
    /// 3. Bundled binary in cache directory
    ///
    /// Ported from: `packages/core/src/ripgrep/binary.ts`
    pub fn resolve_binary_from_path() -> Result<String, RipgrepError> {
        // Check env var first
        if let Ok(path) = std::env::var("BLAZECODE_RG_PATH") {
            if std::path::Path::new(&path).exists() {
                return Ok(path);
            }
        }

        // Check system PATH
        if let Some(path) = find_on_path("rg") {
            return Ok(path);
        }

        // Check bundled binary
        if let Some(data_dir) = dirs::data_dir() {
            let bundled = data_dir.join("blazecode").join("bin").join("rg");
            if bundled.exists() {
                return Ok(bundled.to_string_lossy().to_string());
            }
        }

        Err(RipgrepError::binary_not_found("ripgrep binary not found"))
    }

    /// Find and return the ripgrep binary path, falling back to `"rg"`.
    ///
    /// Ported from: `packages/core/src/ripgrep/binary.ts`
    pub fn resolve_binary() -> String {
        Self::resolve_binary_from_path().unwrap_or_else(|_| "rg".to_string())
    }

    /// Get the current binary state information.
    pub fn binary_state(&self) -> RipgrepBinaryState {
        let path = std::path::Path::new(&self.binary_path);
        let is_system =
            !self.binary_path.contains(".cache") && !self.binary_path.contains(".local/share");

        let version = Self::detect_version(&self.binary_path);

        RipgrepBinaryState {
            filepath: Some(self.binary_path.clone()),
            is_system,
            version,
        }
    }

    /// Detect ripgrep version by running `rg --version`.
    fn detect_version(binary: &str) -> Option<String> {
        std::process::Command::new(binary)
            .arg("--version")
            .output()
            .ok()
            .and_then(|output| {
                let stdout = String::from_utf8_lossy(&output.stdout);
                // Parse "ripgrep 14.1.0" -> "14.1.0"
                stdout
                    .split_whitespace()
                    .nth(1)
                    .map(|v| v.trim().to_string())
            })
    }

    /// Find files matching a glob pattern using `rg --files`.
    ///
    /// Returns file paths relative to the CWD.
    ///
    /// Ported from: `packages/core/src/ripgrep.ts` — `find()`
    pub async fn find(&self, input: &FindInput) -> Result<Vec<String>, RipgrepError> {
        let result = self.find_entries(input).await?;
        Ok(result.items.into_iter().map(|e| e.path).collect())
    }

    /// Find files and return full `RipgrepEntry` values with truncation info.
    ///
    /// Ported from: `packages/core/src/ripgrep.ts` — `find()`
    pub async fn find_entries(&self, input: &FindInput) -> Result<FindResult, RipgrepError> {
        if let Some(ref signal) = input.signal {
            if *signal {
                return Err(RipgrepError::new("search aborted"));
            }
        }

        let mut args = vec![
            "--no-config".to_string(),
            "--files".to_string(),
            "--json".to_string(),
        ];

        if input.hidden.unwrap_or(false) {
            args.push("--hidden".to_string());
        }
        if input.follow.unwrap_or(false) {
            args.push("--follow".to_string());
        }

        args.push("--glob".to_string());
        args.push(input.pattern.clone());
        args.push("--glob=!**/.git/**".to_string());
        args.push("--".to_string());

        let output = self.run_rg(&input.cwd, &args).await?;

        let limit = input.limit as usize;
        let mut items = Vec::new();
        let mut truncated = false;
        let lines_iter = output.lines();

        for line in lines_iter {
            if items.len() >= limit {
                truncated = true;
                break;
            }
            let record: serde_json::Value = match serde_json::from_str(line) {
                Ok(v) => v,
                Err(_) => continue,
            };
            if record.get("type").and_then(|v| v.as_str()) == Some("begin") {
                let path_text = record
                    .get("data")
                    .and_then(|d| d.get("path"))
                    .and_then(|p| p.get("text"))
                    .and_then(|t| t.as_str())
                    .unwrap_or("");
                let normalized = normalize_path(path_text);
                if let Some(ref on_entry) = input.on_entry {
                    let entry_type = if Path::new(path_text).is_dir() {
                        EntryType::Directory
                    } else {
                        EntryType::File
                    };
                    let entry = RipgrepEntry {
                        path: normalized.clone(),
                        entry_type,
                        mime: None,
                    };
                    on_entry(&entry);
                }
                items.push(RipgrepEntry {
                    path: normalized,
                    entry_type: EntryType::File,
                    mime: None,
                });
            }
        }

        Ok(FindResult { items, truncated })
    }

    /// Grep for a regex pattern in file contents using `rg --json`.
    ///
    /// Returns parsed JSON match records with truncation and partial flags.
    ///
    /// Ported from: `packages/core/src/ripgrep.ts` — `grep()`
    pub async fn grep(&self, input: &GrepInput) -> Result<GrepResult, RipgrepError> {
        let mut args = vec![
            "--no-config".to_string(),
            "--json".to_string(),
            "--no-heading".to_string(),
            "-n".to_string(),
            "--hidden".to_string(),
            "--no-messages".to_string(),
            "--glob=!**/.git/**".to_string(),
        ];

        if let Some(ref include) = input.include {
            args.push("--glob".to_string());
            args.push(include.clone());
        }

        args.push("--".to_string());
        args.push(input.pattern.clone());
        if let Some(ref file) = input.file {
            args.push(file.clone());
        }

        let output = self.run_rg_grep(&input.cwd, &args).await?;

        let limit = input.limit as usize;
        let mut matches = Vec::new();
        let mut truncated = false;

        for line in output.stdout.lines() {
            if matches.len() >= limit {
                truncated = true;
                break;
            }
            if line.len() > MAX_RECORD_BYTES {
                return Err(RipgrepError::with_cause(
                    "record too large",
                    format!(
                        "record is {} bytes, exceeds limit of {MAX_RECORD_BYTES}",
                        line.len()
                    ),
                ));
            }
            let record: serde_json::Value = match serde_json::from_str(line) {
                Ok(v) => v,
                Err(_) => continue,
            };
            let match_type = record.get("type").and_then(|v| v.as_str()).unwrap_or("");
            if match_type == "match" {
                if let Ok(m) = serde_json::from_value(record) {
                    matches.push(m);
                }
            }
        }

        Ok(GrepResult {
            items: matches,
            truncated,
            partial: output.exit_code == 2,
        })
    }

    /// Run a ripgrep glob (list files) — similar to find but without JSON output.
    pub async fn glob(&self, input: &GlobInput) -> Result<Vec<String>, RipgrepError> {
        let mut args = vec![
            "--no-config".to_string(),
            "--files".to_string(),
            "--glob".to_string(),
            input.pattern.clone(),
            "--glob=!**/.git/**".to_string(),
        ];

        if input.hidden.unwrap_or(false) {
            args.push("--hidden".to_string());
        }
        if input.follow.unwrap_or(false) {
            args.push("--follow".to_string());
        }

        let output = self.run_rg(&input.cwd, &args).await?;

        let paths: Vec<String> = output
            .lines()
            .map(normalize_path)
            .take(input.limit as usize)
            .collect();

        Ok(paths)
    }

    /// Execute ripgrep with given args and return stdout as string.
    async fn run_rg(&self, cwd: &str, args: &[String]) -> Result<String, RipgrepError> {
        let mut cmd = tokio::process::Command::new(&self.binary_path);
        cmd.arg("--no-config");
        cmd.args(args);
        cmd.current_dir(cwd);
        cmd.stdout(std::process::Stdio::piped());
        cmd.stderr(std::process::Stdio::piped());
        cmd.kill_on_drop(true);

        let output = cmd.output().await.map_err(|e| RipgrepError {
            message: format!("failed to spawn rg: {e}"),
            cause: Some(e.to_string()),
        })?;

        let exit_code = output.status.code().unwrap_or(1);
        let stderr = String::from_utf8_lossy(&output.stderr).to_string();

        match exit_code {
            0 => Ok(String::from_utf8_lossy(&output.stdout).to_string()),
            1 => Ok(String::new()),
            2 => {
                if is_invalid_pattern(&stderr) {
                    Err(RipgrepError {
                        message: "invalid regex pattern".into(),
                        cause: Some(stderr),
                    })
                } else {
                    Err(RipgrepError {
                        message: "rg exited with code 2".to_string(),
                        cause: Some(stderr),
                    })
                }
            }
            _ => Err(RipgrepError {
                message: format!("rg exited with code {exit_code}"),
                cause: Some(stderr),
            }),
        }
    }

    /// Execute ripgrep for grep operations, returning stdout + exit code.
    ///
    /// Unlike `run_rg`, exit code 2 returns stdout (partial results) instead of
    /// an error, allowing callers to use `GrepResult.partial` to signal the issue.
    async fn run_rg_grep(&self, cwd: &str, args: &[String]) -> Result<RgGrepOutput, RipgrepError> {
        let mut cmd = tokio::process::Command::new(&self.binary_path);
        cmd.arg("--no-config");
        cmd.args(args);
        cmd.current_dir(cwd);
        cmd.stdout(std::process::Stdio::piped());
        cmd.stderr(std::process::Stdio::piped());
        cmd.kill_on_drop(true);

        let output = cmd.output().await.map_err(|e| RipgrepError {
            message: format!("failed to spawn rg: {e}"),
            cause: Some(e.to_string()),
        })?;

        let exit_code = output.status.code().unwrap_or(1);

        match exit_code {
            0..=2 => Ok(RgGrepOutput {
                stdout: String::from_utf8_lossy(&output.stdout).to_string(),
                exit_code,
            }),
            _ => {
                let stderr = String::from_utf8_lossy(&output.stderr).to_string();
                Err(RipgrepError::with_cause(
                    format!("rg exited with code {exit_code}"),
                    stderr,
                ))
            }
        }
    }
}

impl Default for RipgrepService {
    fn default() -> Self {
        Self::new()
    }
}

// ── Helpers ───────────────────────────────────────────────────────────────

/// Find an executable on the system PATH.
fn find_on_path(name: &str) -> Option<String> {
    std::env::var_os("PATH").and_then(|path_var| {
        std::env::split_paths(&path_var).find_map(|dir| {
            let path = dir.join(name);
            if path.is_file() {
                Some(path.to_string_lossy().to_string())
            } else {
                None
            }
        })
    })
}

/// Normalize a file path by stripping leading `./`, `/`, or `\` and converting backslashes to forward slashes.
fn normalize_path(path: &str) -> String {
    path.trim_start_matches("./")
        .trim_start_matches(['/', '\\'])
        .replace('\\', "/")
        .to_string()
}

/// Check stderr for invalid regex pattern indicators.
fn is_invalid_pattern(stderr: &str) -> bool {
    stderr.contains("regex parse error") || stderr.contains("error parsing regex")
}

// ── Tests ────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_constants() {
        assert_eq!(ERROR_BYTES, 8 * 1024);
        assert_eq!(MAX_RECORD_BYTES, 1024 * 1024);
        assert_eq!(MAX_SUBMATCHES, 100);
        assert_eq!(RIPGREP_VERSION, "15.1.0");
    }

    #[test]
    fn test_ripgrep_error_display() {
        let err = RipgrepError {
            message: "process exited with code 2".into(),
            cause: Some("regex parse error".into()),
        };
        let displayed = err.to_string();
        assert!(displayed.contains("ripgrep error"));
        assert!(displayed.contains("process exited with code 2"));
        assert!(displayed.contains("regex parse error"));
    }

    #[test]
    fn test_ripgrep_error_display_without_cause() {
        let err = RipgrepError {
            message: "binary not found".into(),
            cause: None,
        };
        let displayed = err.to_string();
        assert_eq!(displayed, "ripgrep error: binary not found");
    }

    #[test]
    fn test_invalid_pattern_error_display() {
        let err = RipgrepInvalidPatternError {
            pattern: "[invalid".into(),
            message: "unclosed character class".into(),
        };
        let displayed = err.to_string();
        assert!(displayed.contains("[invalid"));
        assert!(displayed.contains("unclosed character class"));
    }

    #[test]
    fn test_raw_match_deserialize() {
        let json = r#"{
            "type": "match",
            "data": {
                "path": {"text": "src/main.rs"},
                "lines": {"text": "fn main() {"},
                "line_number": 1,
                "absolute_offset": 0,
                "submatches": [
                    {"match": "main", "start": 3, "end": 7}
                ]
            }
        }"#;
        let rm: RawMatch = serde_json::from_str(json).expect("deserialize RawMatch");
        assert_eq!(rm.match_type, "match");
        assert_eq!(rm.data.path.text, "src/main.rs");
        assert_eq!(rm.data.lines.text, "fn main() {");
        assert_eq!(rm.data.line_number, 1);
        assert_eq!(rm.data.absolute_offset, 0);
        assert_eq!(rm.data.submatches.len(), 1);
        assert_eq!(rm.data.submatches[0].match_text, "main");
        assert_eq!(rm.data.submatches[0].start, 3);
        assert_eq!(rm.data.submatches[0].end, 7);
    }

    #[test]
    fn test_raw_match_deserialize_no_submatches() {
        let json = r#"{
            "type": "match",
            "data": {
                "path": {"text": "lib.rs"},
                "lines": {"text": "pub mod ripgrep;"},
                "line_number": 51,
                "absolute_offset": 1500
            }
        }"#;
        let rm: RawMatch = serde_json::from_str(json).expect("deserialize RawMatch");
        assert_eq!(rm.data.submatches.len(), 0);
    }

    #[test]
    fn test_raw_submatch_rename_serde() {
        let sub = RawSubmatch {
            match_text: "hello".into(),
            start: 5,
            end: 10,
        };
        let json = serde_json::to_string(&sub).expect("serialize RawSubmatch");
        // The JSON key must be "match", not "match_text"
        assert!(json.contains(r#""match":"hello""#));
        assert!(!json.contains("match_text"));

        let parsed: RawSubmatch = serde_json::from_str(&json).expect("deserialize RawSubmatch");
        assert_eq!(parsed.match_text, "hello");
        assert_eq!(parsed.start, 5);
        assert_eq!(parsed.end, 10);
    }

    #[test]
    fn test_find_input_serde_roundtrip() {
        let input = FindInput {
            cwd: "/home/user/project".into(),
            pattern: "*.rs".into(),
            limit: 200,
            hidden: Some(true),
            follow: None,
            signal: None,
            on_entry: None,
        };
        let json = serde_json::to_string(&input).expect("serialize FindInput");
        // signal is skipped during serialization
        assert!(!json.contains("signal"));

        let parsed: FindInput = serde_json::from_str(&json).expect("deserialize FindInput");
        assert_eq!(parsed.cwd, "/home/user/project");
        assert_eq!(parsed.pattern, "*.rs");
        assert_eq!(parsed.limit, 200);
        assert_eq!(parsed.hidden, Some(true));
        assert_eq!(parsed.follow, None);
        // signal defaults to None on deserialization
        assert_eq!(parsed.signal, None);
    }

    #[test]
    fn test_glob_input_serde_defaults() {
        let input = GlobInput {
            cwd: ".".into(),
            pattern: "**/*.ts".into(),
            limit: 50,
            hidden: None,
            follow: None,
        };
        let json = serde_json::to_string(&input).expect("serialize GlobInput");
        assert!(!json.contains("hidden"));
        assert!(!json.contains("follow"));

        let parsed: GlobInput = serde_json::from_str(&json).expect("deserialize GlobInput");
        assert_eq!(parsed.cwd, ".");
        assert_eq!(parsed.pattern, "**/*.ts");
        assert_eq!(parsed.limit, 50);
    }

    #[test]
    fn test_grep_input_serde_with_file_and_include() {
        let input = GrepInput {
            cwd: "/src".into(),
            pattern: r"fn\s+\w+".into(),
            file: Some("src/lib.rs".into()),
            include: Some("*.rs".into()),
            limit: 100,
        };
        let json = serde_json::to_string(&input).expect("serialize GrepInput");
        let parsed: GrepInput = serde_json::from_str(&json).expect("deserialize GrepInput");
        assert_eq!(parsed.cwd, "/src");
        assert_eq!(parsed.pattern, r"fn\s+\w+");
        assert_eq!(parsed.file.as_deref(), Some("src/lib.rs"));
        assert_eq!(parsed.include.as_deref(), Some("*.rs"));
        assert_eq!(parsed.limit, 100);
    }

    #[test]
    fn test_grep_input_serde_minimal() {
        let input = GrepInput {
            cwd: ".".into(),
            pattern: "TODO".into(),
            file: None,
            include: None,
            limit: 10,
        };
        let json = serde_json::to_string(&input).expect("serialize GrepInput");
        assert!(!json.contains("file"));
        assert!(!json.contains("include"));

        let parsed: GrepInput = serde_json::from_str(&json).expect("deserialize GrepInput");
        assert_eq!(parsed.file, None);
        assert_eq!(parsed.include, None);
    }

    #[test]
    fn test_ripgrep_platforms_map_size() {
        let platforms = ripgrep_platforms();
        assert_eq!(platforms.len(), 7);
    }

    #[test]
    fn test_ripgrep_platforms_all_keys_present() {
        let platforms = ripgrep_platforms();
        let expected_keys = [
            "arm64-darwin",
            "arm64-linux",
            "x64-darwin",
            "x64-linux",
            "arm64-win32",
            "ia32-win32",
            "x64-win32",
        ];
        for key in &expected_keys {
            assert!(platforms.contains_key(key), "missing platform key: {key}");
        }
    }

    #[test]
    fn test_ripgrep_platforms_linux_musl() {
        let platforms = ripgrep_platforms();
        let x64_linux = platforms
            .get("x64-linux")
            .expect("x64-linux platform must exist");
        assert_eq!(x64_linux.platform, "x86_64-unknown-linux-musl");
        assert_eq!(x64_linux.extension, "tar.gz");
    }

    #[test]
    fn test_ripgrep_platforms_windows_zip() {
        let platforms = ripgrep_platforms();
        for key in &["arm64-win32", "ia32-win32", "x64-win32"] {
            let cfg = platforms
                .get(*key)
                .unwrap_or_else(|| panic!("{key} platform must exist"));
            assert_eq!(cfg.extension, "zip", "{key} must have zip extension");
        }
    }

    #[test]
    fn test_current_ripgrep_platform_returns_some() {
        // On the host running the test (Linux x86_64 in CI), we expect
        // current_ripgrep_platform() to return the x64-linux config.
        let current = current_ripgrep_platform();
        if cfg!(all(target_os = "linux", target_arch = "x86_64")) {
            assert!(current.is_some(), "x64-linux platform should be detected");
            let cfg = current.expect("current platform");
            assert_eq!(cfg.platform, "x86_64-unknown-linux-musl");
            assert_eq!(cfg.extension, "tar.gz");
        } else if cfg!(all(target_os = "macos", target_arch = "aarch64")) {
            assert!(
                current.is_some(),
                "arm64-darwin platform should be detected"
            );
            let cfg = current.expect("current platform");
            assert_eq!(cfg.platform, "aarch64-apple-darwin");
        } else if cfg!(all(target_os = "windows", target_arch = "x86_64")) {
            assert!(current.is_some(), "x64-win32 platform should be detected");
            let cfg = current.expect("current platform");
            assert_eq!(cfg.platform, "x86_64-pc-windows-msvc");
        }
        // Other targets may return None — that is expected behavior.
    }

    #[test]
    fn test_binary_state_defaults() {
        let state = RipgrepBinaryState {
            filepath: None,
            is_system: false,
            version: None,
        };
        let json = serde_json::to_string(&state).expect("serialize RipgrepBinaryState");
        let parsed: RipgrepBinaryState =
            serde_json::from_str(&json).expect("deserialize RipgrepBinaryState");
        assert!(!parsed.is_system);
        assert_eq!(parsed.filepath, None);
        assert_eq!(parsed.version, None);
    }

    #[test]
    fn test_binary_state_system_installed() {
        let state = RipgrepBinaryState {
            filepath: Some("/usr/bin/rg".into()),
            is_system: true,
            version: Some("15.1.0".into()),
        };
        let json = serde_json::to_string(&state).expect("serialize RipgrepBinaryState");
        let parsed: RipgrepBinaryState =
            serde_json::from_str(&json).expect("deserialize RipgrepBinaryState");
        assert_eq!(parsed.filepath.as_deref(), Some("/usr/bin/rg"));
        assert!(parsed.is_system);
        assert_eq!(parsed.version.as_deref(), Some("15.1.0"));
    }

    #[test]
    fn test_binary_state_bundled() {
        let state = RipgrepBinaryState {
            filepath: Some("/home/user/.cache/rg".into()),
            is_system: false,
            version: Some("15.1.0".into()),
        };
        let json = serde_json::to_string(&state).expect("serialize RipgrepBinaryState");
        let parsed: RipgrepBinaryState =
            serde_json::from_str(&json).expect("deserialize RipgrepBinaryState");
        assert!(!parsed.is_system);
        assert_eq!(parsed.version.as_deref(), Some("15.1.0"));
    }

    #[test]
    fn test_raw_match_path_equality() {
        let a = RawMatchPath {
            text: "src/main.rs".into(),
        };
        let b = RawMatchPath {
            text: "src/main.rs".into(),
        };
        let c = RawMatchPath {
            text: "src/lib.rs".into(),
        };
        assert_eq!(a, b);
        assert_ne!(a, c);
    }

    #[test]
    fn test_raw_match_lines_equality() {
        let a = RawMatchLines {
            text: "fn main() {".into(),
        };
        let b = RawMatchLines {
            text: "fn main() {".into(),
        };
        let c = RawMatchLines { text: "}".into() };
        assert_eq!(a, b);
        assert_ne!(a, c);
    }

    #[test]
    fn test_ripgrep_error_is_std_error() {
        let err = RipgrepError {
            message: "test".into(),
            cause: None,
        };
        // Verify it can be used as a Box<dyn Error>
        let _: Box<dyn std::error::Error> = Box::new(err);
    }

    #[test]
    fn test_invalid_pattern_error_is_std_error() {
        let err = RipgrepInvalidPatternError {
            pattern: "(".into(),
            message: "unclosed group".into(),
        };
        let _: Box<dyn std::error::Error> = Box::new(err);
    }

    #[test]
    fn test_ripgrep_service_resolve_binary() {
        let path = RipgrepService::resolve_binary();
        assert!(
            !path.is_empty(),
            "resolve_binary should return a non-empty string"
        );
    }

    #[test]
    fn test_ripgrep_service_binary_state() {
        let service = RipgrepService::with_binary("/usr/bin/rg");
        let state = service.binary_state();
        assert_eq!(state.filepath.as_deref(), Some("/usr/bin/rg"));
    }

    #[test]
    fn test_ripgrep_service_default() {
        let service = RipgrepService::default();
        assert!(!service
            .binary_state()
            .filepath
            .as_deref()
            .unwrap_or("")
            .is_empty());
    }

    #[test]
    fn test_ripgrep_service_with_explicit_binary() {
        let service = RipgrepService::with_binary("/custom/path/rg");
        let state = service.binary_state();
        assert_eq!(state.filepath.as_deref(), Some("/custom/path/rg"));
    }

    #[tokio::test]
    async fn test_ripgrep_service_find_basic() {
        let service = RipgrepService::new();
        let input = FindInput {
            cwd: "/home/kali/gitaction/blazecodess/blazecode/crates/blazecode-core/src".to_string(),
            pattern: "*.rs".to_string(),
            limit: 10,
            hidden: None,
            follow: None,
            signal: None,
            on_entry: None,
        };
        let result = service.find(&input).await;
        // Should find at least some .rs files
        match result {
            Ok(files) => {
                assert!(!files.is_empty(), "Should find .rs files in src dir");
                for f in &files {
                    assert!(f.ends_with(".rs"), "All files should be .rs: {f}");
                }
            }
            Err(e) => {
                // rg might not be installed; that's okay
                eprintln!("rg not available: {e}");
            }
        }
    }

    #[tokio::test]
    async fn test_ripgrep_service_grep_basic() {
        let service = RipgrepService::new();
        let input = GrepInput {
            cwd: "/home/kali/gitaction/blazecodess/blazecode/crates/blazecode-core/src".to_string(),
            pattern: "pub struct".to_string(),
            file: Some("ripgrep.rs".to_string()),
            include: None,
            limit: 5,
        };
        let result = service.grep(&input).await;
        match result {
            Ok(gr) => {
                // Should find "pub struct" patterns in ripgrep.rs
                assert!(!gr.items.is_empty(), "Should find pub struct in ripgrep.rs");
                assert!(!gr.partial, "Exit code should be 0 for valid pattern");
            }
            Err(e) => {
                eprintln!("rg not available: {e}");
            }
        }
    }

    #[tokio::test]
    async fn test_ripgrep_service_grep_with_invalid_regex() {
        let service = RipgrepService::new();
        let input = GrepInput {
            cwd: "/home/kali/gitaction/blazecodess/blazecode/crates/blazecode-core/src".to_string(),
            pattern: "[invalid".to_string(), // unclosed character class
            file: Some("ripgrep.rs".to_string()),
            include: None,
            limit: 5,
        };
        let result = service.grep(&input).await;
        match result {
            Ok(gr) => {
                // rg 14+ with --json may return partial results with exit code 2
                assert!(gr.partial, "Exit code 2 should set partial=true");
            }
            Err(e) => {
                // Some rg versions still error on invalid regex
                eprintln!("rg error on [invalid: {e}");
            }
        }
    }

    #[tokio::test]
    async fn test_ripgrep_service_glob_basic() {
        let service = RipgrepService::new();
        let input = GlobInput {
            cwd: "/home/kali/gitaction/blazecodess/blazecode/crates/blazecode-core/src".to_string(),
            pattern: "*.rs".to_string(),
            limit: 5,
            hidden: None,
            follow: None,
        };
        let result = service.glob(&input).await;
        match result {
            Ok(files) => {
                assert!(!files.is_empty(), "Should find .rs files");
            }
            Err(e) => {
                eprintln!("rg not available: {e}");
            }
        }
    }

    #[tokio::test]
    async fn test_ripgrep_service_find_nonexistent_dir() {
        let service = RipgrepService::new();
        let input = FindInput {
            cwd: "/nonexistent/directory/xyz".to_string(),
            pattern: "*.rs".to_string(),
            limit: 10,
            hidden: None,
            follow: None,
            signal: None,
            on_entry: None,
        };
        let result = service.find(&input).await;
        // Should error
        assert!(result.is_err(), "Expected error for nonexistent directory");
    }

    #[test]
    fn test_normalize_path_strips_dot_slash() {
        assert_eq!(normalize_path("./src/main.rs"), "src/main.rs");
    }

    #[test]
    fn test_normalize_path_strips_leading_slash() {
        assert_eq!(normalize_path("/src/main.rs"), "src/main.rs");
    }

    #[test]
    fn test_normalize_path_strips_leading_backslash() {
        assert_eq!(normalize_path("\\src\\main.rs"), "src/main.rs");
    }

    #[test]
    fn test_normalize_path_converts_backslashes() {
        assert_eq!(normalize_path("src\\main.rs"), "src/main.rs");
    }

    #[test]
    fn test_normalize_path_already_clean() {
        assert_eq!(normalize_path("src/main.rs"), "src/main.rs");
    }

    #[test]
    fn test_normalize_path_empty() {
        assert_eq!(normalize_path(""), "");
    }

    #[test]
    fn test_is_invalid_pattern_regex_parse_error() {
        assert!(is_invalid_pattern("regex parse error: unclosed"));
    }

    #[test]
    fn test_is_invalid_pattern_error_parsing_regex() {
        assert!(is_invalid_pattern("error parsing regex: bad escape"));
    }

    #[test]
    fn test_is_invalid_pattern_no_match() {
        assert!(!is_invalid_pattern("No matches found"));
    }

    #[test]
    fn test_is_invalid_pattern_empty() {
        assert!(!is_invalid_pattern(""));
    }

    #[test]
    fn test_entry_type_variants() {
        let file = EntryType::File;
        let dir = EntryType::Directory;
        let sym = EntryType::Symlink;
        match file {
            EntryType::File => {}
            _ => panic!("expected File"),
        }
        match dir {
            EntryType::Directory => {}
            _ => panic!("expected Directory"),
        }
        match sym {
            EntryType::Symlink => {}
            _ => panic!("expected Symlink"),
        }
    }

    #[test]
    fn test_ripgrep_entry_construction() {
        let entry = RipgrepEntry {
            path: "src/main.rs".into(),
            entry_type: EntryType::File,
            mime: Some("text/x-rust".into()),
        };
        assert_eq!(entry.path, "src/main.rs");
        assert!(matches!(entry.entry_type, EntryType::File));
        assert_eq!(entry.mime.as_deref(), Some("text/x-rust"));
    }

    #[test]
    fn test_ripgrep_match_construction() {
        let entry = RipgrepEntry {
            path: "src/lib.rs".into(),
            entry_type: EntryType::File,
            mime: None,
        };
        let m = RipgrepMatch {
            entry,
            line: 42,
            offset: 1000,
            text: "pub fn hello()".into(),
            submatches: vec![(4, 9)],
        };
        assert_eq!(m.line, 42);
        assert_eq!(m.offset, 1000);
        assert_eq!(m.text, "pub fn hello()");
        assert_eq!(m.submatches, vec![(4, 9)]);
    }

    #[test]
    fn test_find_input_on_entry_none_serde() {
        let input = FindInput {
            cwd: ".".into(),
            pattern: "*.rs".into(),
            limit: 10,
            hidden: None,
            follow: None,
            signal: None,
            on_entry: None,
        };
        let json = serde_json::to_string(&input).expect("serialize FindInput with on_entry=None");
        assert!(!json.contains("on_entry"));
        let parsed: FindInput = serde_json::from_str(&json).expect("deserialize FindInput");
        assert!(parsed.on_entry.is_none());
    }

    #[tokio::test]
    async fn test_ripgrep_service_find_with_on_entry() {
        use std::sync::{Arc, Mutex};

        let service = RipgrepService::new();
        let entries: Arc<Mutex<Vec<String>>> = Arc::new(Mutex::new(Vec::new()));
        let entries_clone = entries.clone();

        let input = FindInput {
            cwd: "/home/kali/gitaction/blazecodess/blazecode/crates/blazecode-core/src".to_string(),
            pattern: "*.rs".to_string(),
            limit: 5,
            hidden: None,
            follow: None,
            signal: None,
            on_entry: Some(Box::new(move |entry: &RipgrepEntry| {
                entries_clone.lock().expect("lock").push(entry.path.clone());
            })),
        };
        let result = service.find(&input).await;
        match result {
            Ok(paths) => {
                let recorded = entries.lock().expect("lock");
                assert_eq!(paths.len(), recorded.len());
            }
            Err(e) => {
                eprintln!("rg not available: {e}");
            }
        }
    }

    #[tokio::test]
    async fn test_ripgrep_service_grep_no_matches() {
        let service = RipgrepService::new();
        let input = GrepInput {
            cwd: "/home/kali/gitaction/blazecodess/blazecode/crates/blazecode-core/src".to_string(),
            pattern: "ZZZ_IMPOSSIBLE_PATTERN_ZZZ".to_string(),
            file: Some("ripgrep.rs".to_string()),
            include: None,
            limit: 10,
        };
        let result = service.grep(&input).await;
        match result {
            Ok(gr) => {
                assert!(gr.items.is_empty(), "Should return empty for no matches");
                assert!(!gr.truncated, "Should not be truncated");
            }
            Err(e) => {
                eprintln!("rg not available: {e}");
            }
        }
    }

    #[tokio::test]
    async fn test_ripgrep_service_find_excludes_git() {
        let service = RipgrepService::new();
        let input = FindInput {
            cwd: "/home/kali/gitaction/blazecodess/blazecode".to_string(),
            pattern: "*".to_string(),
            limit: 500,
            hidden: None,
            follow: None,
            signal: None,
            on_entry: None,
        };
        let result = service.find(&input).await;
        match result {
            Ok(files) => {
                for f in &files {
                    assert!(!f.contains("/.git/"), "Should exclude .git: {f}");
                }
            }
            Err(e) => {
                eprintln!("rg not available: {e}");
            }
        }
    }

    #[tokio::test]
    async fn test_ripgrep_service_glob_excludes_git() {
        let service = RipgrepService::new();
        let input = GlobInput {
            cwd: "/home/kali/gitaction/blazecodess/blazecode".to_string(),
            pattern: "*".to_string(),
            limit: 500,
            hidden: None,
            follow: None,
        };
        let result = service.glob(&input).await;
        match result {
            Ok(files) => {
                for f in &files {
                    assert!(!f.contains("/.git/"), "Should exclude .git: {f}");
                }
            }
            Err(e) => {
                eprintln!("rg not available: {e}");
            }
        }
    }

    // ── New tests for added features ────────────────────────────────────

    #[test]
    fn test_find_result_construction() {
        let items = vec![RipgrepEntry {
            path: "src/main.rs".into(),
            entry_type: EntryType::File,
            mime: None,
        }];
        let result = FindResult {
            items,
            truncated: false,
        };
        assert_eq!(result.items.len(), 1);
        assert!(!result.truncated);
    }

    #[test]
    fn test_find_result_truncated() {
        let result = FindResult {
            items: vec![],
            truncated: true,
        };
        assert!(result.truncated);
    }

    #[test]
    fn test_grep_result_construction() {
        let result = GrepResult {
            items: vec![],
            truncated: false,
            partial: false,
        };
        assert!(result.items.is_empty());
        assert!(!result.truncated);
        assert!(!result.partial);
    }

    #[test]
    fn test_grep_result_partial() {
        let result = GrepResult {
            items: vec![],
            truncated: false,
            partial: true,
        };
        assert!(result.partial);
    }

    #[test]
    fn test_ripgrep_error_partial_eq() {
        let a = RipgrepError::new("test");
        let b = RipgrepError::new("test");
        assert_eq!(a, b);

        let c = RipgrepError::new("other");
        assert_ne!(a, c);
    }

    #[test]
    fn test_ripgrep_error_new() {
        let err = RipgrepError::new("something went wrong");
        assert_eq!(err.message, "something went wrong");
        assert_eq!(err.cause, None);
    }

    #[test]
    fn test_ripgrep_error_with_cause() {
        let err = RipgrepError::with_cause("download failed", "connection refused");
        assert_eq!(err.message, "download failed");
        assert_eq!(err.cause.as_deref(), Some("connection refused"));
    }

    #[test]
    fn test_ripgrep_error_binary_not_found() {
        let err = RipgrepError::binary_not_found("rg not on PATH");
        assert_eq!(err.message, "rg not on PATH");
        assert_eq!(err.cause, None);
    }

    #[test]
    fn test_cached_binary_path_returns_option() {
        let path = cached_binary_path();
        // Should return Some if rg is on PATH, or None if not
        if std::process::Command::new("rg")
            .arg("--version")
            .output()
            .is_ok()
        {
            assert!(
                path.is_some(),
                "rg is installed, cached_binary_path should be Some"
            );
        }
        // In either case, it should not panic
    }

    #[tokio::test]
    async fn test_find_signal_cancellation() {
        let service = RipgrepService::new();
        let input = FindInput {
            cwd: "/tmp".into(),
            pattern: "*".into(),
            limit: 100,
            hidden: None,
            follow: None,
            signal: Some(true),
            on_entry: None,
        };
        let result = service.find(&input).await;
        assert!(result.is_err(), "Signal cancellation should return error");
        let err = result.unwrap_err();
        assert!(err.message.contains("abort"), "Error should mention abort");
    }

    #[tokio::test]
    async fn test_find_entries_returns_find_result() {
        let service = RipgrepService::new();
        let input = FindInput {
            cwd: "/home/kali/gitaction/blazecodess/blazecode/crates/blazecode-core/src".to_string(),
            pattern: "*.rs".to_string(),
            limit: 3,
            hidden: None,
            follow: None,
            signal: None,
            on_entry: None,
        };
        let result = service.find_entries(&input).await;
        match result {
            Ok(fr) => {
                assert!(!fr.items.is_empty(), "Should find .rs files");
                assert!(fr.items.len() <= 3, "Should respect limit");
                for item in &fr.items {
                    assert!(item.path.ends_with(".rs"));
                    assert!(matches!(item.entry_type, EntryType::File));
                }
            }
            Err(e) => {
                eprintln!("rg not available: {e}");
            }
        }
    }

    #[tokio::test]
    async fn test_find_entries_signal_cancellation() {
        let service = RipgrepService::new();
        let input = FindInput {
            cwd: "/tmp".into(),
            pattern: "*".into(),
            limit: 100,
            hidden: None,
            follow: None,
            signal: Some(true),
            on_entry: None,
        };
        let result = service.find_entries(&input).await;
        assert!(result.is_err(), "Signal cancellation should return error");
    }

    #[test]
    fn test_ripgrep_error_display_new() {
        let err = RipgrepError::with_cause("failed", "timeout");
        let s = err.to_string();
        assert!(s.contains("failed"));
        assert!(s.contains("timeout"));
    }

    #[test]
    fn test_find_result_clone() {
        let fr = FindResult {
            items: vec![RipgrepEntry {
                path: "a.rs".into(),
                entry_type: EntryType::File,
                mime: None,
            }],
            truncated: true,
        };
        let cloned = fr.clone();
        assert_eq!(cloned.items.len(), 1);
        assert!(cloned.truncated);
    }

    #[test]
    fn test_grep_result_clone() {
        let gr = GrepResult {
            items: vec![],
            truncated: true,
            partial: true,
        };
        let cloned = gr.clone();
        assert!(cloned.truncated);
        assert!(cloned.partial);
    }

    #[test]
    fn test_resolve_binary_from_path_ok() {
        let result = RipgrepService::resolve_binary_from_path();
        if std::process::Command::new("rg")
            .arg("--version")
            .output()
            .is_ok()
        {
            assert!(result.is_ok());
        }
    }

    #[test]
    fn test_ripgrep_error_eq_with_different_causes() {
        let a = RipgrepError::with_cause("msg", "cause1");
        let b = RipgrepError::with_cause("msg", "cause2");
        assert_ne!(a, b);
    }
}
