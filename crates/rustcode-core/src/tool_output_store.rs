//! Tool output storage — persistence and retrieval of LLM tool call outputs.
//!
//! Ported from: `packages/core/src/tool-output-store.ts` (199 lines)
//! OpenCode commit: 5d0f86606ac30690f79f0a6a9f41a1f49fe95d0b

use serde::{Deserialize, Serialize};

/// Maximum lines retained per tool output.
///
/// Ported from: `tool-output-store.ts` — `MAX_LINES`
pub const TOOL_OUTPUT_MAX_LINES: usize = 2_000;

/// Maximum bytes retained per tool output.
///
/// Ported from: `tool-output-store.ts` — `MAX_BYTES`
pub const TOOL_OUTPUT_MAX_BYTES: usize = 50 * 1024;

/// Retention period for stored tool outputs (7 days).
///
/// Ported from: `tool-output-store.ts` — `RETENTION`
pub const TOOL_OUTPUT_RETENTION_DAYS: u32 = 7;

/// Managed directory name for tool output storage.
///
/// Ported from: `tool-output-store.ts` — `MANAGED_DIRECTORY`
pub const TOOL_OUTPUT_DIRECTORY: &str = "tool-output";

/// Input for binding a tool output to a session.
///
/// Ported from: `tool-output-store.ts` — `BoundInput`
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BoundInput {
    /// Session identifier
    pub session_id: String,
    /// Tool call identifier
    pub tool_call_id: String,
    /// The tool output to store
    pub output: ToolOutputData,
}

/// Tool output data — the result of a tool execution.
///
/// Ported from: `tool-output-store.ts` + `@opencode-ai/llm` ToolOutput
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type")]
pub enum ToolOutputData {
    /// Text output
    #[serde(rename = "text")]
    Text(ToolOutputText),
    /// Image output (base64-encoded)
    #[serde(rename = "image")]
    Image(ToolOutputImage),
    /// File output
    #[serde(rename = "file")]
    File(ToolOutputFile),
}

/// Text tool output.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ToolOutputText {
    /// The text content
    pub text: String,
}

/// Image tool output.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ToolOutputImage {
    /// Base64-encoded image data
    pub data: String,
    /// MIME type (e.g., "image/png")
    pub mime_type: String,
}

/// File tool output.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ToolOutputFile {
    /// File path
    pub path: String,
    /// File content (base64 for binary)
    pub data: Option<String>,
    /// MIME type
    pub mime_type: Option<String>,
}

/// Result of binding tool output.
///
/// Ported from: `tool-output-store.ts` — `BoundResult`
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BoundResult {
    /// The stored output (possibly truncated)
    pub output: ToolOutputData,
    /// Paths where the output was written
    pub output_paths: Vec<String>,
}

/// Error during tool output storage.
///
/// Ported from: `tool-output-store.ts` — `StorageError`
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ToolOutputStorageError {
    /// The operation that failed
    pub operation: ToolOutputOperation,
    /// Underlying cause message
    pub cause: String,
}

impl std::fmt::Display for ToolOutputStorageError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "tool output storage error in {}: {}",
            self.operation, self.cause
        )
    }
}

impl std::error::Error for ToolOutputStorageError {}

/// Operations that can fail in tool output storage.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum ToolOutputOperation {
    Encode,
    Write,
}

impl std::fmt::Display for ToolOutputOperation {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Encode => write!(f, "encode"),
            Self::Write => write!(f, "write"),
        }
    }
}

/// Storage limits configuration.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ToolOutputLimits {
    pub max_lines: usize,
    pub max_bytes: usize,
}

impl Default for ToolOutputLimits {
    fn default() -> Self {
        Self {
            max_lines: TOOL_OUTPUT_MAX_LINES,
            max_bytes: TOOL_OUTPUT_MAX_BYTES,
        }
    }
}

/// Take a prefix of text up to `max_bytes` UTF-8 bytes.
///
/// Ported from: `tool-output-store.ts` — `takePrefix()`
pub fn take_prefix(input: &str, max_bytes: usize) -> String {
    let mut bytes = 0usize;
    let mut result = String::with_capacity(input.len().min(max_bytes));
    for ch in input.chars() {
        let char_len = ch.len_utf8();
        if bytes + char_len > max_bytes {
            break;
        }
        result.push(ch);
        bytes += char_len;
    }
    result
}

/// Take a suffix of text up to `max_bytes` UTF-8 bytes.
///
/// Ported from: `tool-output-store.ts` — `takeSuffix()`
pub fn take_suffix(input: &str, max_bytes: usize) -> String {
    let chars: Vec<char> = input.chars().collect();
    let mut bytes = 0usize;
    let mut result_chars: Vec<char> = Vec::new();
    for ch in chars.iter().rev() {
        let char_len = ch.len_utf8();
        if bytes + char_len > max_bytes {
            break;
        }
        result_chars.push(*ch);
        bytes += char_len;
    }
    result_chars.iter().rev().collect()
}

/// Build an output path from session ID and tool call ID.
pub fn output_path(session_id: &str, tool_call_id: &str) -> String {
    format!("{TOOL_OUTPUT_DIRECTORY}/{session_id}/{tool_call_id}")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_constants() {
        assert_eq!(TOOL_OUTPUT_MAX_LINES, 2_000);
        assert_eq!(TOOL_OUTPUT_MAX_BYTES, 50 * 1024);
        assert_eq!(TOOL_OUTPUT_RETENTION_DAYS, 7);
        assert_eq!(TOOL_OUTPUT_DIRECTORY, "tool-output");
    }

    #[test]
    fn test_default_limits() {
        let limits = ToolOutputLimits::default();
        assert_eq!(limits.max_lines, 2_000);
        assert_eq!(limits.max_bytes, 50 * 1024);
    }

    #[test]
    fn test_take_prefix_empty() {
        assert_eq!(take_prefix("", 100), "");
    }

    #[test]
    fn test_take_prefix_within_limit() {
        assert_eq!(take_prefix("hello", 100), "hello");
    }

    #[test]
    fn test_take_prefix_exact_limit() {
        assert_eq!(take_prefix("abc", 3), "abc");
    }

    #[test]
    fn test_take_prefix_truncates() {
        let result = take_prefix("abcdefghij", 5);
        assert!(result.len() <= 5);
    }

    #[test]
    fn test_take_suffix_empty() {
        assert_eq!(take_suffix("", 100), "");
    }

    #[test]
    fn test_take_suffix_within_limit() {
        assert_eq!(take_suffix("hello", 100), "hello");
    }

    #[test]
    fn test_take_suffix_truncates_from_front() {
        let input = "abcdefghij";
        let result = take_suffix(input, 5);
        assert!(result.len() <= 5);
        // Should be the last chars
        assert!(input.ends_with(&result));
    }

    #[test]
    fn test_output_path() {
        let path = output_path("sess_123", "tc_456");
        assert!(path.contains("tool-output"));
        assert!(path.contains("sess_123"));
        assert!(path.contains("tc_456"));
    }

    #[test]
    fn test_bound_input_serde() {
        let input = BoundInput {
            session_id: "sess_abc".into(),
            tool_call_id: "tc_def".into(),
            output: ToolOutputData::Text(ToolOutputText {
                text: "result".into(),
            }),
        };
        let json = serde_json::to_string(&input).expect("serialize");
        let parsed: BoundInput = serde_json::from_str(&json).expect("deserialize");
        assert_eq!(parsed.session_id, "sess_abc");
        match parsed.output {
            ToolOutputData::Text(t) => assert_eq!(t.text, "result"),
            _ => panic!("expected text output"),
        }
    }

    #[test]
    fn test_tool_output_image_serde() {
        let output = ToolOutputData::Image(ToolOutputImage {
            data: "base64data".into(),
            mime_type: "image/png".into(),
        });
        let json = serde_json::to_string(&output).expect("serialize");
        assert!(json.contains("image/png"));
    }

    #[test]
    fn test_storage_error_display() {
        let err = ToolOutputStorageError {
            operation: ToolOutputOperation::Write,
            cause: "disk full".into(),
        };
        assert!(err.to_string().contains("write"));
        assert!(err.to_string().contains("disk full"));
    }
}
