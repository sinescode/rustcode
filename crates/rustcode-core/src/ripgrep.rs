//! ripgrep search integration types.
//!
//! Ported from: `packages/core/src/ripgrep.ts` (289 lines),
//!               `packages/core/src/ripgrep/binary.ts` (134 lines)
//! OpenCode commit: 5d0f86606ac30690f79f0a6a9f41a1f49fe95d0b

use serde::{Deserialize, Serialize};

// ---------------------------------------------------------------------------
// Constants
// ---------------------------------------------------------------------------

/// Maximum stderr bytes to capture from ripgrep.
///
/// Ported from: `ripgrep.ts` — `ERROR_BYTES`
pub const RG_ERROR_BYTES: usize = 8 * 1024;

/// Maximum bytes per JSON record from ripgrep.
///
/// Ported from: `ripgrep.ts` — `MAX_RECORD_BYTES`
pub const RG_MAX_RECORD_BYTES: usize = 64 * 1024;

/// Maximum number of submatches to report per match.
///
/// Ported from: `ripgrep.ts` — `MAX_SUBMATCHES`
pub const RG_MAX_SUBMATCHES: usize = 100;

// ---------------------------------------------------------------------------
// Raw ripgrep output types (JSON mode)
// ---------------------------------------------------------------------------

/// Raw match record from ripgrep's `--json` output.
///
/// Ported from: `ripgrep.ts` — `RawMatch`
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RawMatch {
    #[serde(rename = "type")]
    pub match_type: RawMatchType,
    pub data: RawMatchData,
}

/// Type discriminator for ripgrep JSON records.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum RawMatchType {
    #[serde(rename = "match")]
    Match,
    #[serde(rename = "begin")]
    Begin,
    #[serde(rename = "end")]
    End,
    #[serde(rename = "summary")]
    Summary,
    #[serde(rename = "context")]
    Context,
}

/// Data payload for a `match`-type ripgrep record.
///
/// Ported from: `ripgrep.ts` — `RawMatch.data`
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RawMatchData {
    /// File path information
    pub path: RawPathInfo,
    /// Line information
    pub lines: RawLineInfo,
    /// Line number (1-based)
    pub line_number: u64,
    /// Absolute byte offset in file
    pub absolute_offset: u64,
    /// Submatches within the line
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub submatches: Vec<RawSubmatch>,
}

/// File path in ripgrep output.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RawPathInfo {
    pub text: String,
}

/// Line text in ripgrep output.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RawLineInfo {
    pub text: String,
}

/// A submatch within a line.
///
/// Ported from: `ripgrep.ts` — submatches array item
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RawSubmatch {
    /// The matched text
    #[serde(rename = "match")]
    pub submatch_match: RawMatchText,
    /// Start offset (bytes from line start)
    pub start: u64,
    /// End offset (bytes from line start)
    pub end: u64,
}

/// Matched text wrapper.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RawMatchText {
    pub text: String,
}

// ---------------------------------------------------------------------------
// Ripgrep search configuration
// ---------------------------------------------------------------------------

/// Input for a ripgrep search.
///
/// Ported from: `ripgrep.ts` — search options
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RipgrepSearchInput {
    /// The search pattern (regex)
    pub pattern: String,
    /// Path to search in (directory or file)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub path: Option<String>,
    /// File glob pattern to filter (e.g., "*.rs")
    #[serde(skip_serializing_if = "Option::is_none")]
    pub glob: Option<String>,
    /// File type filter (e.g., "rust", "python")
    #[serde(skip_serializing_if = "Option::is_none")]
    pub file_type: Option<String>,
    /// Maximum results to return
    #[serde(skip_serializing_if = "Option::is_none")]
    pub limit: Option<u64>,
    /// Case insensitive search
    #[serde(skip_serializing_if = "Option::is_none")]
    pub case_insensitive: Option<bool>,
    /// Follow symlinks
    #[serde(skip_serializing_if = "Option::is_none")]
    pub follow: Option<bool>,
    /// Search hidden files/directories
    #[serde(skip_serializing_if = "Option::is_none")]
    pub hidden: Option<bool>,
    /// Include file content in results
    #[serde(skip_serializing_if = "Option::is_none")]
    pub with_filename: Option<bool>,
}

/// Output mode for ripgrep.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum RipgrepOutputMode {
    /// JSON output (structured)
    Json,
    /// Standard text output
    Lines,
    /// Files-with-matches only
    FilesWithMatches,
    /// Count of matches per file
    Count,
}

// ---------------------------------------------------------------------------
// Ripgrep binary management types (from ripgrep/binary.ts)
// ---------------------------------------------------------------------------

/// Information about the ripgrep binary.
///
/// Ported from: `ripgrep/binary.ts`
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RipgrepBinary {
    /// Path to the ripgrep binary
    pub path: String,
    /// Version string (e.g., "13.0.0")
    pub version: String,
    /// Whether the binary is bundled or system-installed
    pub source: RipgrepBinarySource,
}

/// Source of the ripgrep binary.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum RipgrepBinarySource {
    /// Bundled with the application
    Bundled,
    /// Found on system PATH
    System,
}

/// Platform-specific binary name.
pub const RG_BINARY_NAME: &str = if cfg!(windows) { "rg.exe" } else { "rg" };

// ---------------------------------------------------------------------------
// Error types
// ---------------------------------------------------------------------------

/// Error from ripgrep operations.
///
/// Ported from: `ripgrep.ts` — `Error`
#[derive(Debug, Clone, Serialize, Deserialize, thiserror::Error)]
#[error("ripgrep error: {message}")]
pub struct RipgrepError {
    pub message: String,
    #[serde(skip)]
    pub cause: Option<String>,
}

/// Error for invalid regex patterns.
///
/// Ported from: `ripgrep.ts` — `InvalidPatternError`
#[derive(Debug, Clone, Serialize, Deserialize, thiserror::Error)]
#[error("invalid ripgrep pattern '{pattern}': {message}")]
pub struct RipgrepInvalidPatternError {
    pub pattern: String,
    pub message: String,
}

/// Error when ripgrep binary is not found.
#[derive(Debug, Clone, thiserror::Error)]
#[error("ripgrep binary not found at {path}")]
pub struct RipgrepNotFoundError {
    pub path: String,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_constants() {
        assert_eq!(RG_ERROR_BYTES, 8 * 1024);
        assert_eq!(RG_MAX_RECORD_BYTES, 64 * 1024);
        assert_eq!(RG_MAX_SUBMATCHES, 100);
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
                    {"match": {"text": "main"}, "start": 3, "end": 7}
                ]
            }
        }"#;
        let rm: RawMatch = serde_json::from_str(json).expect("deserialize");
        assert_eq!(rm.match_type, RawMatchType::Match);
        assert_eq!(rm.data.path.text, "src/main.rs");
        assert_eq!(rm.data.line_number, 1);
        assert_eq!(rm.data.submatches.len(), 1);
    }

    #[test]
    fn test_search_input_serde() {
        let input = RipgrepSearchInput {
            pattern: r"fn\s+\w+".into(),
            path: Some("src".into()),
            glob: Some("*.rs".into()),
            file_type: Some("rust".into()),
            limit: Some(100),
            case_insensitive: None,
            follow: None,
            hidden: Some(true),
            with_filename: Some(true),
        };
        let json = serde_json::to_string(&input).expect("serialize");
        let parsed: RipgrepSearchInput =
            serde_json::from_str(&json).expect("deserialize");
        assert_eq!(parsed.pattern, r"fn\s+\w+");
        assert_eq!(parsed.path, Some("src".into()));
        assert_eq!(parsed.glob, Some("*.rs".into()));
    }

    #[test]
    fn test_search_input_defaults() {
        let input = RipgrepSearchInput {
            pattern: "test".into(),
            path: None,
            glob: None,
            file_type: None,
            limit: None,
            case_insensitive: None,
            follow: None,
            hidden: None,
            with_filename: None,
        };
        let json = serde_json::to_string(&input).expect("serialize");
        assert!(!json.contains("path"));
        assert!(!json.contains("glob"));
    }

    #[test]
    fn test_binary_source_serde() {
        assert_eq!(
            serde_json::to_string(&RipgrepBinarySource::Bundled).expect("serialize"),
            r#""bundled""#
        );
        assert_eq!(
            serde_json::to_string(&RipgrepBinarySource::System).expect("serialize"),
            r#""system""#
        );
    }

    #[test]
    fn test_binary_info_serde() {
        let bin = RipgrepBinary {
            path: "/usr/bin/rg".into(),
            version: "14.1.0".into(),
            source: RipgrepBinarySource::System,
        };
        let json = serde_json::to_string(&bin).expect("serialize");
        let parsed: RipgrepBinary = serde_json::from_str(&json).expect("deserialize");
        assert_eq!(parsed.version, "14.1.0");
    }

    #[test]
    fn test_ripgrep_error_display() {
        let err = RipgrepError {
            message: "process exited with code 2".into(),
            cause: Some("stderr output".into()),
        };
        assert!(err.to_string().contains("ripgrep error"));
    }

    #[test]
    fn test_invalid_pattern_error_display() {
        let err = RipgrepInvalidPatternError {
            pattern: "[invalid".into(),
            message: "unclosed character class".into(),
        };
        assert!(err.to_string().contains("[invalid"));
    }

    #[test]
    fn test_binary_name() {
        if cfg!(windows) {
            assert_eq!(RG_BINARY_NAME, "rg.exe");
        } else {
            assert_eq!(RG_BINARY_NAME, "rg");
        }
    }

    #[test]
    fn test_raw_submatch_serde() {
        let sub = RawSubmatch {
            submatch_match: RawMatchText {
                text: "hello".into(),
            },
            start: 5,
            end: 10,
        };
        let json = serde_json::to_string(&sub).expect("serialize");
        let parsed: RawSubmatch = serde_json::from_str(&json).expect("deserialize");
        assert_eq!(parsed.submatch_match.text, "hello");
        assert_eq!(parsed.start, 5);
        assert_eq!(parsed.end, 10);
    }
}
