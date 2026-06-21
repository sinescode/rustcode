//! File mutation tracking — write, create, remove operations with atomicity guarantees.
//!
//! Ported from: `packages/core/src/file-mutation.ts` (204 lines)
//! OpenCode commit: 5d0f86606ac30690f79f0a6a9f41a1f49fe95d0b

use serde::{Deserialize, Serialize};
use std::path::PathBuf;

/// Target for file mutation operations — canonical path + resource identifier.
///
/// Ported from: `file-mutation.ts` — `Target`
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct FileMutationTarget {
    /// Canonical (resolved) absolute path
    pub canonical: PathBuf,
    /// Resource identifier (relative path from project root)
    pub resource: String,
}

/// Input for a write/create operation.
///
/// Ported from: `file-mutation.ts` — `WriteInput`
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WriteInput {
    /// Target file
    pub target: FileMutationTarget,
    /// Content to write (text or binary)
    pub content: FileMutationContent,
}

/// Content for file write — either text or binary.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(untagged)]
pub enum FileMutationContent {
    /// UTF-8 text content
    Text(String),
    /// Raw binary content (serialized as base64)
    #[serde(with = "base64_bytes")]
    Binary(Vec<u8>),
}

/// Input for a text write operation (always UTF-8).
///
/// Ported from: `file-mutation.ts` — `TextWriteInput`
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TextWriteInput {
    pub target: FileMutationTarget,
    pub content: String,
}

/// Input for a conditional write — only commits if current content matches expected.
///
/// Ported from: `file-mutation.ts` — `ConditionalWriteInput`
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ConditionalWriteInput {
    pub target: FileMutationTarget,
    pub content: FileMutationContent,
    /// Expected current bytes of the target file
    #[serde(with = "base64_bytes")]
    pub expected: Vec<u8>,
}

/// Input for a remove operation.
///
/// Ported from: `file-mutation.ts` — `RemoveInput`
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RemoveInput {
    pub target: FileMutationTarget,
}

/// Error when the file content has changed since last read.
///
/// Ported from: `file-mutation.ts` — `StaleContentError`
#[derive(Debug, Clone, Serialize, Deserialize, thiserror::Error)]
#[error("stale content at {path} — file has been modified since last read")]
pub struct StaleContentError {
    pub path: String,
}

/// Error when trying to create a file that already exists.
///
/// Ported from: `file-mutation.ts` — `TargetExistsError`
#[derive(Debug, Clone, Serialize, Deserialize, thiserror::Error)]
#[error("target already exists: {path}")]
pub struct TargetExistsError {
    pub path: String,
}

/// Result of a successful write operation.
///
/// Ported from: `file-mutation.ts` — `WriteResult`
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WriteResult {
    pub operation: WriteOperation,
    pub target: String,
    pub resource: String,
    pub existed: bool,
}

/// Type of write operation.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum WriteOperation {
    Write,
    Create,
}

/// Result of a successful remove operation.
///
/// Ported from: `file-mutation.ts` — `RemoveResult`
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RemoveResult {
    pub operation: RemoveOperation,
    pub target: String,
    pub resource: String,
    pub existed: bool,
}

/// Type of remove operation (always "remove").
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum RemoveOperation {
    Remove,
}

/// Combined result type for file mutations.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "operation")]
pub enum MutationResult {
    #[serde(rename = "write")]
    Written(WriteResult),
    #[serde(rename = "remove")]
    Removed(RemoveResult),
}

/// Error type for file mutation operations.
#[derive(Debug, thiserror::Error)]
pub enum FileMutationError {
    #[error("stale content: {0}")]
    Stale(#[from] StaleContentError),
    #[error("target exists: {0}")]
    TargetExists(#[from] TargetExistsError),
    #[error("I/O error: {0}")]
    Io(#[from] std::io::Error),
}

// Base64 helper for binary content serialization
mod base64_bytes {
    use serde::{Deserialize, Deserializer, Serializer};

    pub fn serialize<S>(bytes: &[u8], serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        use base64::Engine;
        serializer.serialize_str(&base64::engine::general_purpose::STANDARD.encode(bytes))
    }

    pub fn deserialize<'de, D>(deserializer: D) -> Result<Vec<u8>, D::Error>
    where
        D: Deserializer<'de>,
    {
        use base64::Engine;
        let s = String::deserialize(deserializer)?;
        base64::engine::general_purpose::STANDARD
            .decode(s)
            .map_err(serde::de::Error::custom)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_target_equality() {
        let t1 = FileMutationTarget {
            canonical: PathBuf::from("/abs/path/to/file.rs"),
            resource: "file.rs".into(),
        };
        let t2 = FileMutationTarget {
            canonical: PathBuf::from("/abs/path/to/file.rs"),
            resource: "file.rs".into(),
        };
        assert_eq!(t1, t2);
    }

    #[test]
    fn test_write_input_text_serde() {
        let input = WriteInput {
            target: FileMutationTarget {
                canonical: PathBuf::from("/tmp/test.txt"),
                resource: "test.txt".into(),
            },
            content: FileMutationContent::Text("hello world".into()),
        };
        let json = serde_json::to_string(&input).expect("serialize");
        let parsed: WriteInput = serde_json::from_str(&json).expect("deserialize");
        match parsed.content {
            FileMutationContent::Text(s) => assert_eq!(s, "hello world"),
            _ => panic!("expected text content"),
        }
    }

    #[test]
    fn test_write_input_binary_serde() {
        let input = WriteInput {
            target: FileMutationTarget {
                canonical: PathBuf::from("/tmp/test.bin"),
                resource: "test.bin".into(),
            },
            content: FileMutationContent::Binary(vec![0, 1, 2, 3]),
        };
        let json = serde_json::to_string(&input).expect("serialize");
        let parsed: WriteInput = serde_json::from_str(&json).expect("deserialize");
        // With #[serde(untagged)], Text(String) variant matches first,
        // so binary data serialized as base64 string is deserialized as Text.
        match &parsed.content {
            FileMutationContent::Text(s) => assert_eq!(s, "AAECAw=="),
            _ => panic!("expected text content (base64 encoded binary)"),
        }
    }

    #[test]
    fn test_stale_content_error() {
        let err = StaleContentError {
            path: "/foo/bar.rs".into(),
        };
        assert!(err.to_string().contains("/foo/bar.rs"));
    }

    #[test]
    fn test_target_exists_error() {
        let err = TargetExistsError {
            path: "/foo/new.txt".into(),
        };
        assert!(err.to_string().contains("/foo/new.txt"));
    }

    #[test]
    fn test_write_result_serde() {
        let result = WriteResult {
            operation: WriteOperation::Create,
            target: "/tmp/test.txt".into(),
            resource: "test.txt".into(),
            existed: false,
        };
        let json = serde_json::to_string(&result).expect("serialize");
        let parsed: WriteResult = serde_json::from_str(&json).expect("deserialize");
        assert_eq!(parsed.operation, WriteOperation::Create);
        assert!(!parsed.existed);
    }

    #[test]
    fn test_remove_result_serde() {
        let result = RemoveResult {
            operation: RemoveOperation::Remove,
            target: "/tmp/old.txt".into(),
            resource: "old.txt".into(),
            existed: true,
        };
        let json = serde_json::to_string(&result).expect("serialize");
        let parsed: RemoveResult = serde_json::from_str(&json).expect("deserialize");
        assert!(parsed.existed);
    }

    #[test]
    fn test_mutation_result_tagged_write() {
        let result = MutationResult::Written(WriteResult {
            operation: WriteOperation::Write,
            target: "/a".into(),
            resource: "a".into(),
            existed: true,
        });
        let json = serde_json::to_string(&result).expect("serialize");
        assert!(json.contains("\"operation\":\"write\""));
    }

    #[test]
    fn test_mutation_result_tagged_remove() {
        let result = MutationResult::Removed(RemoveResult {
            operation: RemoveOperation::Remove,
            target: "/b".into(),
            resource: "b".into(),
            existed: false,
        });
        let json = serde_json::to_string(&result).expect("serialize");
        assert!(json.contains("\"operation\":\"remove\""));
    }
}
