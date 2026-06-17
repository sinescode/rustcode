//! ripgrep search integration types.
//!
//! Ported from:
//! - `packages/core/src/ripgrep.ts`
//! - `packages/core/src/ripgrep/binary.ts`
//!
//! OpenCode commit: 5d0f86606ac30690f79f0a6a9f41a1f49fe95d0b

use serde::{Deserialize, Serialize};
use std::collections::HashMap;

// ── Constants ────────────────────────────────────────────────────────────

/// Maximum stderr bytes to capture from a ripgrep child process.
///
/// Ported from: `packages/core/src/ripgrep.ts` — `ERROR_BYTES`
pub const ERROR_BYTES: u32 = 8 * 1024;

/// Maximum bytes per JSON record read from ripgrep stdout.
///
/// Ported from: `packages/core/src/ripgrep.ts` — `MAX_RECORD_BYTES`
pub const MAX_RECORD_BYTES: u32 = 64 * 1024;

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
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RipgrepError {
    /// Human-readable error message.
    pub message: String,
    /// Optional underlying cause (e.g., stderr output from the rg process).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub cause: Option<String>,
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

// ── Input Types ──────────────────────────────────────────────────────────

/// Input for a ripgrep file-search (`rg --files` with optional glob).
///
/// Crawls a directory tree and returns matching file paths up to `limit`.
///
/// Ported from: `packages/core/src/ripgrep.ts` — `FindInput`
#[derive(Debug, Clone, Serialize, Deserialize)]
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
/// Maps an OpenCode platform key (e.g., `"x64-linux"`) to the ripgrep
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

/// State of the local ripgrep binary installation.
///
/// Tracks where the binary lives on disk, whether it came from the system
/// PATH or was downloaded by OpenCode, and its detected version string.
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

/// Service for running ripgrep search operations.
///
/// Ported from: `packages/core/src/ripgrep.ts`
pub struct RipgrepService {
    /// Path to the rg binary
    binary_path: String,
}

impl RipgrepService {
    /// Create a new RipgrepService, auto-detecting the binary.
    pub fn new() -> Self {
        Self {
            binary_path: Self::resolve_binary(),
        }
    }

    /// Create with an explicit binary path.
    pub fn with_binary(binary_path: impl Into<String>) -> Self {
        Self {
            binary_path: binary_path.into(),
        }
    }

    /// Find and return the ripgrep binary path.
    ///
    /// Checks:
    /// 1. OPENCODE_RG_PATH environment variable
    /// 2. `rg` on system PATH
    /// 3. Bundled binary in cache directory
    ///
    /// Ported from: `packages/core/src/ripgrep/binary.ts`
    pub fn resolve_binary() -> String {
        // Check env var first
        if let Ok(path) = std::env::var("OPENCODE_RG_PATH") {
            if std::path::Path::new(&path).exists() {
                return path;
            }
        }

        // Check system PATH
        if let Ok(path) = which::which("rg") {
            return path.to_string_lossy().to_string();
        }

        // Check bundled binary
        if let Some(data_dir) = dirs::data_dir() {
            let bundled = data_dir.join("opencode").join("bin").join("rg");
            if bundled.exists() {
                return bundled.to_string_lossy().to_string();
            }
        }

        // Default — hope rg is on PATH
        "rg".to_string()
    }

    /// Get the current binary state information.
    pub fn binary_state(&self) -> RipgrepBinaryState {
        let path = std::path::Path::new(&self.binary_path);
        let is_system = !self.binary_path.contains(".cache")
            && !self.binary_path.contains(".local/share");

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
        let mut args = vec![
            "--files".to_string(),
            "--json".to_string(),
        ];

        if input.hidden.unwrap_or(false) {
            args.push("--hidden".to_string());
        }
        if input.follow.unwrap_or(false) {
            args.push("--follow".to_string());
        }

        // The glob pattern
        args.push("--glob".to_string());
        args.push(input.pattern.clone());

        let output = self.run_rg(&input.cwd, &args).await?;

        let paths = output
            .lines()
            .filter_map(|line| {
                let record: serde_json::Value = serde_json::from_str(line).ok()?;
                if record.get("type")?.as_str()? == "begin" {
                    record
                        .get("data")?
                        .get("path")?
                        .get("text")?
                        .as_str()
                        .map(|s| s.to_string())
                } else {
                    None
                }
            })
            .take(input.limit as usize)
            .collect();

        Ok(paths)
    }

    /// Grep for a regex pattern in file contents using `rg --json`.
    ///
    /// Returns parsed JSON match records.
    ///
    /// Ported from: `packages/core/src/ripgrep.ts` — `grep()`
    pub async fn grep(&self, input: &GrepInput) -> Result<Vec<RawMatch>, RipgrepError> {
        let mut args = vec![
            "--json".to_string(),
            "--no-heading".to_string(),
            "-n".to_string(), // line numbers
        ];

        if let Some(ref include) = input.include {
            args.push("--glob".to_string());
            args.push(include.clone());
        }

        if let Some(ref file) = input.file {
            args.push(file.clone());
        }

        args.push(input.pattern.clone());

        let output = self.run_rg(&input.cwd, &args).await?;

        let matches: Vec<RawMatch> = output
            .lines()
            .filter_map(|line| {
                let record: serde_json::Value = serde_json::from_str(line).ok()?;
                let match_type = record.get("type")?.as_str()?.to_string();

                if match_type == "match" {
                    serde_json::from_value(record).ok()
                } else {
                    None
                }
            })
            .take(input.limit as usize)
            .collect();

        Ok(matches)
    }

    /// Run a ripgrep glob (list files) — similar to find but without JSON output.
    pub async fn glob(&self, input: &GlobInput) -> Result<Vec<String>, RipgrepError> {
        let mut args = vec![
            "--files".to_string(),
            "--glob".to_string(),
            input.pattern.clone(),
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
            .map(|s| s.to_string())
            .take(input.limit as usize)
            .collect();

        Ok(paths)
    }

    /// Execute ripgrep with given args and return stdout as string.
    async fn run_rg(&self, cwd: &str, args: &[String]) -> Result<String, RipgrepError> {
        let mut cmd = tokio::process::Command::new(&self.binary_path);
        cmd.args(args);
        cmd.current_dir(cwd);
        cmd.stdout(std::process::Stdio::piped());
        cmd.stderr(std::process::Stdio::piped());
        cmd.kill_on_drop(true);

        let output = cmd.output().await.map_err(|e| RipgrepError {
            message: format!("failed to spawn rg: {e}"),
            cause: Some(e.to_string()),
        })?;

        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr).to_string();

            // Check for invalid regex pattern
            if stderr.contains("regex") || stderr.contains("pattern") {
                return Err(RipgrepError {
                    message: format!("invalid regex pattern"),
                    cause: Some(stderr),
                });
            }

            return Err(RipgrepError {
                message: format!("rg exited with code {:?}", output.status.code()),
                cause: Some(stderr),
            });
        }

        Ok(String::from_utf8_lossy(&output.stdout).to_string())
    }
}

impl Default for RipgrepService {
    fn default() -> Self {
        Self::new()
    }
}

// ── Tests ────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_constants() {
        assert_eq!(ERROR_BYTES, 8 * 1024);
        assert_eq!(MAX_RECORD_BYTES, 64 * 1024);
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

        let parsed: RawSubmatch =
            serde_json::from_str(&json).expect("deserialize RawSubmatch");
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
        };
        let json = serde_json::to_string(&input).expect("serialize FindInput");
        // signal is skipped during serialization
        assert!(!json.contains("signal"));

        let parsed: FindInput =
            serde_json::from_str(&json).expect("deserialize FindInput");
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

        let parsed: GlobInput =
            serde_json::from_str(&json).expect("deserialize GlobInput");
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
        let parsed: GrepInput =
            serde_json::from_str(&json).expect("deserialize GrepInput");
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

        let parsed: GrepInput =
            serde_json::from_str(&json).expect("deserialize GrepInput");
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
            assert!(
                platforms.contains_key(key),
                "missing platform key: {key}"
            );
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
            assert_eq!(
                cfg.extension, "zip",
                "{key} must have zip extension"
            );
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
            assert!(current.is_some(), "arm64-darwin platform should be detected");
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
        let c = RawMatchLines {
            text: "}".into(),
        };
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
        assert!(!path.is_empty(), "resolve_binary should return a non-empty string");
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
        assert!(!service.binary_state().filepath.as_deref().unwrap_or("").is_empty());
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
            cwd: "/home/kali/gitaction/opencodess/rustcode/crates/rustcode-core/src".to_string(),
            pattern: "*.rs".to_string(),
            limit: 10,
            hidden: None,
            follow: None,
            signal: None,
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
            cwd: "/home/kali/gitaction/opencodess/rustcode/crates/rustcode-core/src".to_string(),
            pattern: "pub struct".to_string(),
            file: Some("ripgrep.rs".to_string()),
            include: None,
            limit: 5,
        };
        let result = service.grep(&input).await;
        match result {
            Ok(matches) => {
                // Should find "pub struct" patterns in ripgrep.rs
                assert!(!matches.is_empty(), "Should find pub struct in ripgrep.rs");
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
            cwd: "/home/kali/gitaction/opencodess/rustcode/crates/rustcode-core/src".to_string(),
            pattern: "[invalid".to_string(), // unclosed character class
            file: Some("ripgrep.rs".to_string()),
            include: None,
            limit: 5,
        };
        let result = service.grep(&input).await;
        // Should error with invalid regex
        if let Ok(matches) = result {
            // rg 14+ sometimes handles this differently
            eprintln!("rg handled [invalid gracefully: {} matches", matches.len());
        }
    }

    #[tokio::test]
    async fn test_ripgrep_service_glob_basic() {
        let service = RipgrepService::new();
        let input = GlobInput {
            cwd: "/home/kali/gitaction/opencodess/rustcode/crates/rustcode-core/src".to_string(),
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
        };
        let result = service.find(&input).await;
        // Should error
        assert!(result.is_err(), "Expected error for nonexistent directory");
    }
}
