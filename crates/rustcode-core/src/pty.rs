//! PTY/terminal types — terminal, session, protocol, schema, and ticket types.
//!
//! Ported from:
//! - `packages/core/src/pty.ts` (lines 1–347)
//! - `packages/core/src/pty/protocol.ts` (lines 1–38)
//! - `packages/core/src/pty/schema.ts` (lines 1–13)
//! - `packages/core/src/pty/ticket.ts` (lines 1–61)

use serde::{Deserialize, Serialize};
use std::sync::Arc;

// ══════════════════════════════════════════════════════════════════════════════
// PTY ID
// ══════════════════════════════════════════════════════════════════════════════

/// PTY identifier — branded string starting with "pty".
///
/// # Source
/// `packages/core/src/pty/schema.ts` lines 5–13 `PtyID`.
pub type PtyId = String;

// ══════════════════════════════════════════════════════════════════════════════
// PTY Info
// ══════════════════════════════════════════════════════════════════════════════

/// Information about a PTY session.
///
/// # Source
/// `packages/core/src/pty.ts` lines 38–49 `Info`.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PtyInfo {
    /// Unique PTY identifier
    pub id: PtyId,
    /// Display title
    pub title: String,
    /// Shell command
    pub command: String,
    /// Command arguments
    pub args: Vec<String>,
    /// Working directory
    pub cwd: String,
    /// Process status
    pub status: PtyStatus,
    /// Process ID (0 if not yet assigned on Windows)
    pub pid: u64,
    /// Exit code (present when status is "exited")
    #[serde(skip_serializing_if = "Option::is_none")]
    pub exit_code: Option<u64>,
}

/// PTY process status.
///
/// # Source
/// `packages/core/src/pty.ts` line 44 `status` literal union.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum PtyStatus {
    /// Process is running
    Running,
    /// Process has exited
    Exited,
}

// ══════════════════════════════════════════════════════════════════════════════
// PTY Create / Update Input
// ══════════════════════════════════════════════════════════════════════════════

/// Input for creating a new PTY.
///
/// # Source
/// `packages/core/src/pty.ts` lines 53–59 `CreateInput`.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PtyCreateInput {
    /// Shell command (defaults to user's preferred shell)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub command: Option<String>,
    /// Command arguments
    #[serde(skip_serializing_if = "Option::is_none")]
    pub args: Option<Vec<String>>,
    /// Working directory (defaults to current directory)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub cwd: Option<String>,
    /// Display title
    #[serde(skip_serializing_if = "Option::is_none")]
    pub title: Option<String>,
    /// Environment variables to merge
    #[serde(skip_serializing_if = "Option::is_none")]
    pub env: Option<std::collections::HashMap<String, String>>,
}

/// Input for updating an existing PTY.
///
/// # Source
/// `packages/core/src/pty.ts` lines 63–71 `UpdateInput`.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PtyUpdateInput {
    /// New display title
    #[serde(skip_serializing_if = "Option::is_none")]
    pub title: Option<String>,
    /// New terminal size
    #[serde(skip_serializing_if = "Option::is_none")]
    pub size: Option<TerminalSize>,
}

/// Terminal window size (rows × columns).
///
/// # Source
/// `packages/core/src/pty.ts` lines 66–69 `size` struct.
#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
pub struct TerminalSize {
    /// Number of rows
    pub rows: u32,
    /// Number of columns
    pub cols: u32,
}

// ══════════════════════════════════════════════════════════════════════════════
// PTY Attachment
// ══════════════════════════════════════════════════════════════════════════════

/// Input for attaching to a PTY.
///
/// # Source
/// `packages/core/src/pty.ts` lines 75–82 `AttachInput`.
#[derive(Debug, Clone)]
pub struct PtyAttachInput {
    /// Absolute output cursor to replay from.
    /// -1 tails from current end; omitted replays the full retained buffer.
    pub cursor: Option<i64>,
}

/// Attachment handle — replay, write, activate, detach.
///
/// # Source
/// `packages/core/src/pty.ts` lines 84–93 `Attachment`.
#[derive(Debug, Clone)]
pub struct PtyAttachment {
    /// Retained output from requested cursor to current end
    pub replay: String,
    /// Absolute output cursor after replay
    pub cursor: u64,
}

/// PTY subscriber callback traits.
pub trait PtySubscriber: Send + Sync {
    /// Called with output data chunks.
    fn on_data(&self, chunk: &str);
    /// Called when the process exits.
    fn on_end(&self, exit_code: Option<u32>);
}

// ══════════════════════════════════════════════════════════════════════════════
// PTY Errors
// ══════════════════════════════════════════════════════════════════════════════

/// PTY not found error.
///
/// # Source
/// `packages/core/src/pty.ts` lines 95–97 `NotFoundError`.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PtyNotFoundError {
    pub pty_id: PtyId,
}

impl std::fmt::Display for PtyNotFoundError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "PTY not found: {}", self.pty_id)
    }
}

impl std::error::Error for PtyNotFoundError {}

/// PTY already exited error.
///
/// # Source
/// `packages/core/src/pty.ts` lines 99–101 `ExitedError`.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PtyExitedError {
    pub pty_id: PtyId,
}

impl std::fmt::Display for PtyExitedError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "PTY already exited: {}", self.pty_id)
    }
}

impl std::error::Error for PtyExitedError {}

// ══════════════════════════════════════════════════════════════════════════════
// PTY Events
// ══════════════════════════════════════════════════════════════════════════════

/// Event payload for PTY creation.
///
/// # Source
/// `packages/core/src/pty.ts` line 104 `Event.Created`.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PtyCreatedEvent {
    pub info: PtyInfo,
}

/// Event payload for PTY update.
///
/// # Source
/// `packages/core/src/pty.ts` line 105 `Event.Updated`.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PtyUpdatedEvent {
    pub info: PtyInfo,
}

/// Event payload for PTY exit.
///
/// # Source
/// `packages/core/src/pty.ts` line 106 `Event.Exited`.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PtyExitedEvent {
    pub id: PtyId,
    pub exit_code: u64,
}

/// Event payload for PTY deletion.
///
/// # Source
/// `packages/core/src/pty.ts` line 107 `Event.Deleted`.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PtyDeletedEvent {
    pub id: PtyId,
}

// ══════════════════════════════════════════════════════════════════════════════
// PTY Protocol Types
// ══════════════════════════════════════════════════════════════════════════════

/// Maximum bytes in a single replay chunk.
///
/// # Source
/// `packages/core/src/pty/protocol.ts` line 13 `REPLAY_CHUNK`.
pub const REPLAY_CHUNK: usize = 64 * 1024;

/// Metadata frame sent after replay to communicate cursor position.
/// A 0x00 byte followed by UTF-8 JSON: `{"cursor": N}`.
///
/// # Source
/// `packages/core/src/pty/protocol.ts` lines 15–21 `metaFrame`.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PtyMetaFrame {
    pub cursor: u64,
}

/// Split data into replay chunks.
///
/// # Source
/// `packages/core/src/pty/protocol.ts` lines 23–27 `chunks`.
pub fn chunk_replay(data: &str) -> Vec<&str> {
    if data.is_empty() {
        return vec![];
    }
    let mut result = Vec::new();
    let mut start = 0;
    while start < data.len() {
        let end = std::cmp::min(start + REPLAY_CHUNK, data.len());
        result.push(&data[start..end]);
        start = end;
    }
    result
}

// ══════════════════════════════════════════════════════════════════════════════
// PTY Ticket Types
// ══════════════════════════════════════════════════════════════════════════════

/// Connection token for PTY websocket auth.
///
/// # Source
/// `packages/core/src/pty/ticket.ts` lines 12–15 `ConnectToken`.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PtyConnectToken {
    /// Opaque ticket string
    pub ticket: String,
    /// Expiration seconds
    pub expires_in: u64,
}

/// Scope for issuing/consuming a PTY ticket.
///
/// # Source
/// `packages/core/src/pty/ticket.ts` lines 17–21 `Scope`.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PtyTicketScope {
    /// PTY ID the ticket grants access to
    pub pty_id: PtyId,
    /// Optional directory constraint
    #[serde(skip_serializing_if = "Option::is_none")]
    pub directory: Option<String>,
    /// Optional workspace constraint
    #[serde(skip_serializing_if = "Option::is_none")]
    pub workspace_id: Option<String>,
}

/// Default ticket time-to-live (60 seconds).
///
/// # Source
/// `packages/core/src/pty/ticket.ts` line 7 `DEFAULT_TTL`.
pub const PTY_TICKET_TTL_SECS: u64 = 60;

/// Maximum ticket cache capacity.
///
/// # Source
/// `packages/core/src/pty/ticket.ts` line 8 `CAPACITY`.
pub const PTY_TICKET_CAPACITY: usize = 10_000;

// ══════════════════════════════════════════════════════════════════════════════
// PTY Trait (Service Interface)
// ══════════════════════════════════════════════════════════════════════════════

/// PTY service interface.
///
/// # Source
/// `packages/core/src/pty.ts` lines 110–118 `Interface`.
pub trait PtyService: Send + Sync {
    /// List all PTY sessions.
    fn list(&self) -> impl std::future::Future<Output = Result<Vec<PtyInfo>, PtyError>> + Send;

    /// Get a single PTY by ID.
    fn get(&self, id: PtyId) -> impl std::future::Future<Output = Result<PtyInfo, PtyError>> + Send;

    /// Create a new PTY session.
    fn create(
        &self,
        input: PtyCreateInput,
    ) -> impl std::future::Future<Output = Result<PtyInfo, PtyError>> + Send;

    /// Update a PTY (title, size).
    fn update(
        &self,
        id: PtyId,
        input: PtyUpdateInput,
    ) -> impl std::future::Future<Output = Result<PtyInfo, PtyError>> + Send;

    /// Remove a PTY session.
    fn remove(&self, id: PtyId) -> impl std::future::Future<Output = Result<(), PtyError>> + Send;

    /// Write data to the PTY process stdin.
    fn write(&self, id: PtyId, data: String) -> impl std::future::Future<Output = Result<(), PtyError>> + Send;
}

/// PTY service error.
#[derive(Debug, Clone)]
pub enum PtyError {
    NotFound(PtyId),
    Exited(PtyId),
    Io(String),
    Other(String),
}

impl std::fmt::Display for PtyError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::NotFound(id) => write!(f, "PTY not found: {id}"),
            Self::Exited(id) => write!(f, "PTY already exited: {id}"),
            Self::Io(msg) => write!(f, "PTY I/O error: {msg}"),
            Self::Other(msg) => write!(f, "PTY error: {msg}"),
        }
    }
}

impl std::error::Error for PtyError {}

// ══════════════════════════════════════════════════════════════════════════════
// Tests
// ══════════════════════════════════════════════════════════════════════════════

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_pty_info_serialization() {
        let info = PtyInfo {
            id: "pty_0001".into(),
            title: "Terminal 1".into(),
            command: "/bin/bash".into(),
            args: vec!["-l".into()],
            cwd: "/home/user".into(),
            status: PtyStatus::Running,
            pid: 12345,
            exit_code: None,
        };
        let json = serde_json::to_string(&info).expect("serialize");
        assert!(json.contains("pty_0001"));
        assert!(json.contains("running"));
        assert!(json.contains("/bin/bash"));
        let parsed: PtyInfo = serde_json::from_str(&json).expect("deserialize");
        assert_eq!(parsed.id, "pty_0001");
        assert_eq!(parsed.status, PtyStatus::Running);
    }

    #[test]
    fn test_pty_info_exited() {
        let info = PtyInfo {
            id: "pty_0002".into(),
            title: "Exited Terminal".into(),
            command: "zsh".into(),
            args: vec![],
            cwd: "/tmp".into(),
            status: PtyStatus::Exited,
            pid: 99999,
            exit_code: Some(0),
        };
        let json = serde_json::to_string(&info).expect("serialize");
        assert!(json.contains("exited"));
        assert!(json.contains("exit_code"));
        let parsed: PtyInfo = serde_json::from_str(&json).expect("deserialize");
        assert_eq!(parsed.exit_code, Some(0));
    }

    #[test]
    fn test_pty_status_roundtrip() {
        assert_eq!(
            serde_json::to_string(&PtyStatus::Running).expect("serialize"),
            r#""running""#
        );
        assert_eq!(
            serde_json::to_string(&PtyStatus::Exited).expect("serialize"),
            r#""exited""#
        );
        let parsed: PtyStatus = serde_json::from_str(r#""running""#).expect("deserialize");
        assert_eq!(parsed, PtyStatus::Running);
    }

    #[test]
    fn test_pty_create_input() {
        let input = PtyCreateInput {
            command: Some("/bin/fish".into()),
            args: Some(vec!["-C".into(), "echo hello".into()]),
            cwd: Some("/home/user".into()),
            title: Some("Fish Shell".into()),
            env: None,
        };
        let json = serde_json::to_string(&input).expect("serialize");
        assert!(json.contains("/bin/fish"));
        assert!(json.contains("echo hello"));
    }

    #[test]
    fn test_pty_update_input_resize() {
        let input = PtyUpdateInput {
            title: None,
            size: Some(TerminalSize {
                rows: 40,
                cols: 120,
            }),
        };
        let json = serde_json::to_string(&input).expect("serialize");
        assert!(json.contains("120"));
        assert!(json.contains("40"));
    }

    #[test]
    fn test_pty_not_found_error_display() {
        let err = PtyNotFoundError {
            pty_id: "pty_missing".into(),
        };
        assert_eq!(err.to_string(), "PTY not found: pty_missing");
    }

    #[test]
    fn test_chunk_replay_empty() {
        let chunks = chunk_replay("");
        assert!(chunks.is_empty());
    }

    #[test]
    fn test_chunk_replay_small() {
        let data = "hello world";
        let chunks = chunk_replay(data);
        assert_eq!(chunks, vec!["hello world"]);
    }

    #[test]
    fn test_chunk_replay_large() {
        let data = "A".repeat(REPLAY_CHUNK + 100);
        let chunks = chunk_replay(&data);
        assert_eq!(chunks.len(), 2);
        assert_eq!(chunks[0].len(), REPLAY_CHUNK);
        assert_eq!(chunks[1].len(), 100);
    }

    #[test]
    fn test_pty_ticket_token() {
        let token = PtyConnectToken {
            ticket: "abc123-def456".into(),
            expires_in: 60,
        };
        let json = serde_json::to_string(&token).expect("serialize");
        assert!(json.contains("abc123-def456"));
        assert!(json.contains("expires_in"));
    }

    #[test]
    fn test_pty_ticket_scope() {
        let scope = PtyTicketScope {
            pty_id: "pty_001".into(),
            directory: Some("/home/user/project".into()),
            workspace_id: None,
        };
        let json = serde_json::to_string(&scope).expect("serialize");
        assert!(json.contains("pty_001"));
        assert!(json.contains("/home/user/project"));
    }

    #[test]
    fn test_pty_error_display() {
        assert!(PtyError::NotFound("pty_x".into()).to_string().contains("pty_x"));
        assert!(PtyError::Exited("pty_y".into()).to_string().contains("pty_y"));
        assert!(PtyError::Io("broken pipe".into()).to_string().contains("broken pipe"));
        assert!(PtyError::Other("something".into()).to_string().contains("something"));
    }
}
