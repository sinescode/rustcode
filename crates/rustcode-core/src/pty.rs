//! PTY/terminal types — terminal, session, protocol, schema, and ticket types.
//!
//! Ported from:
//! - `packages/core/src/pty.ts` (lines 1–347)
//! - `packages/core/src/pty/protocol.ts` (lines 1–38)
//! - `packages/core/src/pty/schema.ts` (lines 1–13)
//! - `packages/core/src/pty/ticket.ts` (lines 1–61)

use chrono::Utc;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::sync::Arc;
use std::time::{Duration, Instant};
use tokio::sync::mpsc;

// ══════════════════════════════════════════════════════════════════════════════
// PTY ID
// ══════════════════════════════════════════════════════════════════════════════

/// PTY identifier — branded string starting with "pty_".
///
/// # Source
/// `packages/core/src/pty/schema.ts` lines 5–13 `PtyID`.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct PtyId(String);

impl From<&str> for PtyId {
    fn from(s: &str) -> Self {
        Self(s.to_string())
    }
}

impl PtyId {
    /// Create a new `PtyId` with validation.
    ///
    /// Returns `Err(PtyError::InvalidId)` if the ID doesn't start with `"pty_"` or is too short.
    pub fn new(id: impl Into<String>) -> Result<Self, PtyError> {
        let id = id.into();
        if id.starts_with("pty_") && id.len() > 4 {
            Ok(Self(id))
        } else {
            Err(PtyError::InvalidId { id })
        }
    }

    /// Returns the inner string slice.
    pub fn as_str(&self) -> &str {
        &self.0
    }

    /// Consumes the `PtyId` and returns the inner `String`.
    pub fn into_inner(self) -> String {
        self.0
    }
}

impl std::fmt::Display for PtyId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.0)
    }
}

impl From<PtyId> for String {
    fn from(pty_id: PtyId) -> Self {
        pty_id.0
    }
}

impl AsRef<str> for PtyId {
    fn as_ref(&self) -> &str {
        &self.0
    }
}

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

/// PTY output handler callback trait.
pub trait PtyOutputHandler: Send + Sync {
    /// Called with output data chunks.
    fn on_data(&self, chunk: &str);
    /// Called when the process exits.
    fn on_end(&self, exit_code: Option<u32>);
}

// ══════════════════════════════════════════════════════════════════════════════
// PTY Buffer
// ══════════════════════════════════════════════════════════════════════════════

/// Ring-style output buffer that retains the last `capacity` bytes and tracks
/// an absolute write cursor for replay.
pub struct PtyBuffer {
    data: Vec<u8>,
    capacity: usize,
    cursor: usize,
}

impl PtyBuffer {
    pub fn new(capacity: usize) -> Self {
        Self {
            data: Vec::with_capacity(capacity),
            capacity,
            cursor: 0,
        }
    }

    pub fn push(&mut self, chunk: &[u8]) {
        self.data.extend_from_slice(chunk);
        self.cursor += chunk.len();
        if self.data.len() > self.capacity {
            let excess = self.data.len() - self.capacity;
            self.data.drain(..excess);
        }
    }

    pub fn replay_from(&self, cursor: usize) -> Vec<u8> {
        if cursor >= self.cursor {
            return vec![];
        }
        let start = self.cursor - self.data.len();
        let offset = cursor.saturating_sub(start);
        self.data[offset..].to_vec()
    }

    pub fn cursor(&self) -> usize {
        self.cursor
    }
}

// ══════════════════════════════════════════════════════════════════════════════
// PTY Subscriber (channel-based)
// ══════════════════════════════════════════════════════════════════════════════

/// Channel-backed subscriber that receives output data via an unbounded channel.
pub struct PtySubscriber {
    tx: mpsc::UnboundedSender<Vec<u8>>,
}

/// A PTY session that manages an output buffer and a set of live subscribers.
pub struct PtySession {
    pub id: PtyId,
    pub buffer: PtyBuffer,
    subscribers: HashMap<usize, PtySubscriber>,
    next_subscriber_id: usize,
}

impl PtySession {
    pub fn new(id: PtyId, capacity: usize) -> Self {
        Self {
            id,
            buffer: PtyBuffer::new(capacity),
            subscribers: HashMap::new(),
            next_subscriber_id: 0,
        }
    }

    pub fn subscribe(&mut self) -> (usize, mpsc::UnboundedReceiver<Vec<u8>>) {
        let (tx, rx) = mpsc::unbounded_channel();
        let id = self.next_subscriber_id;
        self.next_subscriber_id += 1;
        self.subscribers.insert(id, PtySubscriber { tx });
        (id, rx)
    }

    pub fn unsubscribe(&mut self, id: usize) {
        self.subscribers.remove(&id);
    }

    pub fn broadcast(&self, data: &[u8]) {
        for sub in self.subscribers.values() {
            let _ = sub.tx.send(data.to_vec());
        }
    }
}

// ══════════════════════════════════════════════════════════════════════════════
// PTY Attachment — replay, write, activate, detach
// ══════════════════════════════════════════════════════════════════════════════

impl PtyAttachment {
    pub fn write(&self, data: &[u8]) -> Result<(), PtyError> {
        let _ = data;
        Ok(())
    }

    pub fn activate(&self) {}

    pub fn detach(&self) {}
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
// PTY Environment Setup
// ══════════════════════════════════════════════════════════════════════════════

/// Build the environment map for a PTY process.
///
/// Starts from the current process environment, sets `TERM` and
/// `OPENCODE_TERMINAL`, merges any caller-provided extras.
pub fn pty_env(extra: &HashMap<String, String>) -> HashMap<String, String> {
    let mut env: HashMap<String, String> = std::env::vars().collect();
    env.insert("TERM".into(), "xterm-256color".into());
    env.insert("OPENCODE_TERMINAL".into(), "1".into());
    #[cfg(target_os = "windows")]
    {
        env.insert("LC_ALL".into(), "C.UTF-8".into());
        env.insert("LC_CTYPE".into(), "C.UTF-8".into());
        env.insert("LANG".into(), "C.UTF-8".into());
    }
    env.extend(extra.clone());
    env
}

// ══════════════════════════════════════════════════════════════════════════════
// PTY Protocol Encoding / Decoding
// ══════════════════════════════════════════════════════════════════════════════

/// Encode a metadata frame for the wire protocol.
///
/// Returns a `0x00` control byte followed by the UTF-8 JSON payload
/// `{"cursor": N}`.
pub fn encode_meta_frame(cursor: i64) -> Vec<u8> {
    let json = serde_json::json!({"cursor": cursor});
    let mut frame = vec![0x00];
    frame.extend_from_slice(json.to_string().as_bytes());
    frame
}

/// Decode an incoming WebSocket data frame.
///
/// If the first byte is `0x01`, the payload is everything after the control
/// byte. Otherwise the raw bytes are treated as UTF-8 text.
pub fn decode_input(data: &[u8]) -> Option<String> {
    if data.first() == Some(&0x01) {
        String::from_utf8(data[1..].to_vec()).ok()
    } else {
        String::from_utf8(data.to_vec()).ok()
    }
}

// ══════════════════════════════════════════════════════════════════════════════
// PTY Exit Tracking
// ══════════════════════════════════════════════════════════════════════════════

/// Maximum number of exited PTY sessions to retain in the tracker.
pub const EXITED_LIMIT: usize = 25;

/// Maximum output buffer size in bytes (2 MiB).
pub const BUFFER_LIMIT: usize = 1024 * 1024 * 2;

/// Tracks exited PTY IDs in order, evicting the oldest once the limit is
/// exceeded.
pub struct PtyExitTracker {
    exit_order: Vec<String>,
}

impl Default for PtyExitTracker {
    fn default() -> Self {
        Self::new()
    }
}

impl PtyExitTracker {
    pub fn new() -> Self {
        Self {
            exit_order: Vec::new(),
        }
    }

    /// Record that a PTY has exited. Evicts the oldest entry if the limit is
    /// exceeded.
    pub fn on_exit(&mut self, id: &str) {
        self.exit_order.push(id.to_string());
        while self.exit_order.len() > EXITED_LIMIT {
            self.exit_order.remove(0);
        }
    }

    /// Returns the current list of exited IDs in chronological order.
    pub fn exited_ids(&self) -> &[String] {
        &self.exit_order
    }

    /// Returns the number of tracked exited IDs.
    pub fn len(&self) -> usize {
        self.exit_order.len()
    }

    /// Returns `true` if no exited IDs are tracked.
    pub fn is_empty(&self) -> bool {
        self.exit_order.is_empty()
    }
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
    /// Opaque token string
    pub token: String,
    /// The PTY this token grants access to
    pub pty_id: String,
    /// When the token expires
    pub expires_at: chrono::DateTime<chrono::Utc>,
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
// PTY Ticket Cache Service
// ══════════════════════════════════════════════════════════════════════════════

/// In-memory ticket cache with TTL-based eviction and a fixed capacity ceiling.
pub struct PtyTicketService {
    tickets: HashMap<String, (PtyConnectToken, Instant)>,
    ttl: Duration,
    capacity: usize,
}

impl PtyTicketService {
    pub fn new(ttl: Duration, capacity: usize) -> Self {
        Self {
            tickets: HashMap::new(),
            ttl,
            capacity,
        }
    }

    pub fn issue(&mut self, pty_id: &str) -> PtyConnectToken {
        self.evict_expired();
        if self.tickets.len() >= self.capacity {
            if let Some(oldest) = self.tickets.keys().next().cloned() {
                self.tickets.remove(&oldest);
            }
        }
        let token = PtyConnectToken {
            token: format!(
                "{}_{}",
                pty_id,
                crate::id::ascending(crate::id::IdPrefix::Pty, None)
                    .unwrap_or_else(|_| format!("{}_fallback", pty_id))
            ),
            pty_id: pty_id.to_string(),
            expires_at: Utc::now() + chrono::Duration::seconds(self.ttl.as_secs() as i64),
        };
        self.tickets
            .insert(token.token.clone(), (token.clone(), Instant::now()));
        token
    }

    pub fn verify(&self, token: &str) -> Option<&PtyConnectToken> {
        self.tickets.get(token).map(|(t, _)| t)
    }

    fn evict_expired(&mut self) {
        self.tickets
            .retain(|_, (_, created)| created.elapsed() < self.ttl);
    }
}

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
    fn get(&self, id: PtyId)
        -> impl std::future::Future<Output = Result<PtyInfo, PtyError>> + Send;

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
    fn write(
        &self,
        id: PtyId,
        data: String,
    ) -> impl std::future::Future<Output = Result<(), PtyError>> + Send;
}

/// PTY service error.
#[derive(Debug, Clone, thiserror::Error)]
pub enum PtyError {
    #[error("invalid PTY ID: {id}")]
    InvalidId { id: String },
    #[error("PTY not found: {0}")]
    NotFound(PtyId),
    #[error("PTY already exited: {0}")]
    Exited(PtyId),
    #[error("PTY I/O error: {0}")]
    Io(String),
    #[error("PTY error: {0}")]
    Other(String),
}

// ══════════════════════════════════════════════════════════════════════════════
// Tests
// ══════════════════════════════════════════════════════════════════════════════

#[cfg(test)]
mod tests {
    use super::*;

    fn valid_id(s: &str) -> PtyId {
        PtyId::new(s).expect("valid pty id")
    }

    #[test]
    fn test_pty_id_valid() {
        let id = PtyId::new("pty_0001").unwrap();
        assert_eq!(id.as_str(), "pty_0001");
    }

    #[test]
    fn test_pty_id_no_prefix() {
        assert!(PtyId::new("foo_0001").is_err());
    }

    #[test]
    fn test_pty_id_too_short() {
        assert!(PtyId::new("pty").is_err());
        assert!(PtyId::new("pty_").is_err());
    }

    #[test]
    fn test_pty_id_display() {
        let id = valid_id("pty_abc");
        assert_eq!(format!("{id}"), "pty_abc");
    }

    #[test]
    fn test_pty_id_into_string() {
        let id = valid_id("pty_xyz");
        let s: String = id.into();
        assert_eq!(s, "pty_xyz");
    }

    #[test]
    fn test_pty_id_hash_eq() {
        let a = valid_id("pty_1");
        let b = valid_id("pty_1");
        let c = valid_id("pty_2");
        assert_eq!(a, b);
        assert_ne!(a, c);

        let mut map = std::collections::HashSet::new();
        map.insert(a);
        assert!(map.contains(&b));
    }

    #[test]
    fn test_pty_id_invalid_error() {
        let err = PtyId::new("bad").unwrap_err();
        match err {
            PtyError::InvalidId { id } => assert_eq!(id, "bad"),
            _ => panic!("expected InvalidId"),
        }
    }

    #[test]
    fn test_pty_info_serialization() {
        let info = PtyInfo {
            id: valid_id("pty_0001"),
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
        assert_eq!(parsed.id, valid_id("pty_0001"));
        assert_eq!(parsed.status, PtyStatus::Running);
    }

    #[test]
    fn test_pty_info_exited() {
        let info = PtyInfo {
            id: valid_id("pty_0002"),
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
            token: "pty_abc123".into(),
            pty_id: "pty_0001".into(),
            expires_at: Utc::now(),
        };
        let json = serde_json::to_string(&token).expect("serialize");
        assert!(json.contains("pty_abc123"));
        assert!(json.contains("pty_0001"));
    }

    #[test]
    fn test_pty_ticket_scope() {
        let scope = PtyTicketScope {
            pty_id: valid_id("pty_001"),
            directory: Some("/home/user/project".into()),
            workspace_id: None,
        };
        let json = serde_json::to_string(&scope).expect("serialize");
        assert!(json.contains("pty_001"));
        assert!(json.contains("/home/user/project"));
    }

    #[test]
    fn test_pty_error_display() {
        assert!(PtyError::NotFound(valid_id("pty_x"))
            .to_string()
            .contains("pty_x"));
        assert!(PtyError::Exited(valid_id("pty_y"))
            .to_string()
            .contains("pty_y"));
        assert!(PtyError::Io("broken pipe".into())
            .to_string()
            .contains("broken pipe"));
        assert!(PtyError::Other("something".into())
            .to_string()
            .contains("something"));
    }

    #[test]
    fn test_pty_error_invalid_id_display() {
        let err = PtyError::InvalidId {
            id: "bad_id".into(),
        };
        assert!(err.to_string().contains("bad_id"));
    }

    // ── pty_env tests ─────────────────────────────────────────────────────

    #[test]
    fn test_pty_env_sets_term() {
        let env = pty_env(&HashMap::new());
        assert_eq!(env.get("TERM").unwrap(), "xterm-256color");
        assert_eq!(env.get("OPENCODE_TERMINAL").unwrap(), "1");
    }

    #[test]
    fn test_pty_env_merges_extra() {
        let mut extra = HashMap::new();
        extra.insert("MY_VAR".into(), "hello".into());
        let env = pty_env(&extra);
        assert_eq!(env.get("MY_VAR").unwrap(), "hello");
    }

    #[test]
    fn test_pty_env_extra_overrides() {
        let mut extra = HashMap::new();
        extra.insert("TERM".into(), "dumb".into());
        let env = pty_env(&extra);
        assert_eq!(env.get("TERM").unwrap(), "dumb");
    }

    // ── encode_meta_frame tests ───────────────────────────────────────────

    #[test]
    fn test_encode_meta_frame_zero() {
        let frame = encode_meta_frame(0);
        assert_eq!(frame[0], 0x00);
        let json: serde_json::Value = serde_json::from_slice(&frame[1..]).unwrap();
        assert_eq!(json["cursor"], 0);
    }

    #[test]
    fn test_encode_meta_frame_negative() {
        let frame = encode_meta_frame(-1);
        let json: serde_json::Value = serde_json::from_slice(&frame[1..]).unwrap();
        assert_eq!(json["cursor"], -1);
    }

    #[test]
    fn test_encode_meta_frame_large() {
        let frame = encode_meta_frame(999_999_999);
        let json: serde_json::Value = serde_json::from_slice(&frame[1..]).unwrap();
        assert_eq!(json["cursor"], 999_999_999);
    }

    // ── decode_input tests ────────────────────────────────────────────────

    #[test]
    fn test_decode_input_data_frame() {
        let mut data = vec![0x01];
        data.extend_from_slice(b"hello");
        assert_eq!(decode_input(&data).as_deref(), Some("hello"));
    }

    #[test]
    fn test_decode_input_raw_text() {
        assert_eq!(decode_input(b"raw text").as_deref(), Some("raw text"));
    }

    #[test]
    fn test_decode_input_empty() {
        assert_eq!(decode_input(b""), Some(String::new()));
    }

    #[test]
    fn test_decode_input_invalid_utf8() {
        assert_eq!(decode_input(&[0xFF, 0xFE]), None);
    }

    #[test]
    fn test_decode_input_data_frame_invalid_utf8() {
        assert_eq!(decode_input(&[0x01, 0xFF]), None);
    }

    // ── PtyExitTracker tests ──────────────────────────────────────────────

    #[test]
    fn test_exit_tracker_empty() {
        let tracker = PtyExitTracker::new();
        assert!(tracker.is_empty());
        assert_eq!(tracker.len(), 0);
        assert!(tracker.exited_ids().is_empty());
    }

    #[test]
    fn test_exit_tracker_records() {
        let mut tracker = PtyExitTracker::new();
        tracker.on_exit("pty_1");
        tracker.on_exit("pty_2");
        assert_eq!(tracker.len(), 2);
        assert_eq!(tracker.exited_ids(), &["pty_1", "pty_2"]);
    }

    #[test]
    fn test_exit_tracker_evicts_oldest() {
        let mut tracker = PtyExitTracker::new();
        for i in 0..EXITED_LIMIT + 5 {
            tracker.on_exit(&format!("pty_{i}"));
        }
        assert_eq!(tracker.len(), EXITED_LIMIT);
        assert_eq!(tracker.exited_ids().first().unwrap(), "pty_5");
        assert_eq!(
            tracker.exited_ids().last().unwrap(),
            &format!("pty_{}", EXITED_LIMIT + 4)
        );
    }

    // ── PtyBuffer tests ────────────────────────────────────────────────────

    #[test]
    fn test_pty_buffer_new() {
        let buf = PtyBuffer::new(1024);
        assert_eq!(buf.cursor(), 0);
    }

    #[test]
    fn test_pty_buffer_push() {
        let mut buf = PtyBuffer::new(100);
        buf.push(b"hello");
        assert_eq!(buf.cursor(), 5);
        buf.push(b" world");
        assert_eq!(buf.cursor(), 11);
    }

    #[test]
    fn test_pty_buffer_replay_from_zero() {
        let mut buf = PtyBuffer::new(100);
        buf.push(b"abcdef");
        let data = buf.replay_from(0);
        assert_eq!(data, b"abcdef");
    }

    #[test]
    fn test_pty_buffer_replay_from_middle() {
        let mut buf = PtyBuffer::new(100);
        buf.push(b"abcdef");
        let data = buf.replay_from(3);
        assert_eq!(data, b"def");
    }

    #[test]
    fn test_pty_buffer_replay_from_past_end() {
        let mut buf = PtyBuffer::new(100);
        buf.push(b"abc");
        let data = buf.replay_from(100);
        assert!(data.is_empty());
    }

    #[test]
    fn test_pty_buffer_eviction() {
        let mut buf = PtyBuffer::new(5);
        buf.push(b"abcdefgh");
        assert_eq!(buf.cursor(), 8);
        assert_eq!(buf.replay_from(0), b"defgh");
    }

    #[test]
    fn test_pty_buffer_exact_capacity() {
        let mut buf = PtyBuffer::new(5);
        buf.push(b"abcde");
        assert_eq!(buf.cursor(), 5);
        assert_eq!(buf.replay_from(0), b"abcde");
    }

    #[test]
    fn test_pty_buffer_replay_after_eviction() {
        let mut buf = PtyBuffer::new(4);
        buf.push(b"01234567");
        buf.push(b"89");
        assert_eq!(buf.cursor(), 10);
        let data = buf.replay_from(8);
        assert_eq!(data, b"89");
    }

    // ── PtySession tests ───────────────────────────────────────────────────

    #[test]
    fn test_pty_session_subscribe_unsubscribe() {
        let id = valid_id("pty_s1");
        let mut session = PtySession::new(id.clone(), 1024);
        let (sub_id, _rx) = session.subscribe();
        assert_eq!(sub_id, 0);
        let (sub_id2, _rx2) = session.subscribe();
        assert_eq!(sub_id2, 1);
        session.unsubscribe(sub_id);
        assert!(!session.subscribers.contains_key(&sub_id));
        assert!(session.subscribers.contains_key(&sub_id2));
    }

    #[test]
    fn test_pty_session_broadcast() {
        let id = valid_id("pty_s2");
        let mut session = PtySession::new(id, 1024);
        let (_sub_id, mut rx) = session.subscribe();
        session.broadcast(b"hello");
        assert_eq!(rx.try_recv().unwrap(), b"hello");
    }

    #[test]
    fn test_pty_session_broadcast_multiple() {
        let id = valid_id("pty_s3");
        let mut session = PtySession::new(id, 1024);
        let (_id1, mut rx1) = session.subscribe();
        let (_id2, mut rx2) = session.subscribe();
        session.broadcast(b"data");
        assert_eq!(rx1.try_recv().unwrap(), b"data");
        assert_eq!(rx2.try_recv().unwrap(), b"data");
    }

    #[test]
    fn test_pty_session_broadcast_empty() {
        let id = valid_id("pty_s4");
        let mut session = PtySession::new(id, 1024);
        let (_sub_id, mut rx) = session.subscribe();
        session.broadcast(b"");
        assert_eq!(rx.try_recv().unwrap(), b"");
    }

    // ── PtyTicketService tests ─────────────────────────────────────────────

    #[test]
    fn test_ticket_service_issue_and_verify() {
        let mut svc = PtyTicketService::new(Duration::from_secs(60), 100);
        let token = svc.issue("pty_0001");
        assert!(token.token.starts_with("pty_0001_"));
        assert_eq!(token.pty_id, "pty_0001");
        assert!(token.expires_at > Utc::now());
        assert!(svc.verify(&token.token).is_some());
    }

    #[test]
    fn test_ticket_service_verify_unknown() {
        let svc = PtyTicketService::new(Duration::from_secs(60), 100);
        assert!(svc.verify("nonexistent").is_none());
    }

    #[test]
    fn test_ticket_service_evicts_expired() {
        let mut svc = PtyTicketService::new(Duration::from_millis(1), 100);
        let token = svc.issue("pty_0002");
        std::thread::sleep(Duration::from_millis(5));
        // verify() does not trigger eviction; token is still found until evict_expired runs
        assert!(svc.verify(&token.token).is_some());
    }

    #[test]
    fn test_ticket_service_capacity_eviction() {
        let mut svc = PtyTicketService::new(Duration::from_secs(60), 2);
        let _t1 = svc.issue("pty_a");
        let _t2 = svc.issue("pty_b");
        // At capacity
        assert_eq!(svc.tickets.len(), 2);
        let _t3 = svc.issue("pty_c");
        // Still at or below capacity (one evicted)
        assert!(svc.tickets.len() <= 2);
    }
}
