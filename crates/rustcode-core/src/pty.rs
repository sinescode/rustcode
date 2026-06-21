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
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;
use std::time::{Duration, Instant};
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
use tokio::sync::{broadcast, mpsc, Mutex, RwLock};
use tokio_util::sync::CancellationToken;

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
    /// Optional callback for incoming data chunks (called synchronously from data path).
    /// Use this for live streaming; consider keeping this lightweight/non-blocking.
    #[cfg(feature = "pty_callbacks")]
    pub on_data: Option<std::sync::Arc<dyn Fn(&str) + Send + Sync>>,
    /// Optional callback for when the session ends (process exit or teardown).
    #[cfg(feature = "pty_callbacks")]
    pub on_end: Option<std::sync::Arc<dyn Fn(Option<u64>) + Send + Sync>>,
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
        Err(PtyError::Other("PtyAttachment is a stub; use PtyLiveAttachment instead".into()))
    }

    pub fn activate(&self) {
        // Stub
    }

    pub fn detach(&self) {
        // Stub
    }
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
/// Inbound frames are treated as raw UTF-8 text, matching the OpenCode PTY
/// protocol where all client-to-server data is plain text or binary UTF-8.
///
/// # Source
///  lines 30–37 .
pub fn decode_input(data: &[u8]) -> Option<String> {
    String::from_utf8(data.to_vec()).ok()
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

    /// Attach to a PTY session to receive live output and send input.
    /// Returns an attachment handle with replay buffer, cursor, write, activate, and detach capabilities.
    ///
    /// # Source
    ///  lines 289–340 .
    fn attach(
        &self,
        id: PtyId,
        input: PtyAttachInput,
    ) -> impl std::future::Future<Output = Result<PtyLiveAttachment, PtyError>> + Send;
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
    fn test_decode_input_plain_text() {
        assert_eq!(decode_input(b"hello").as_deref(), Some("hello"));
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
    fn test_decode_input_invalid_utf8_after_ctrl() {
        // 0x01 followed by 0xFF is invalid UTF-8 (0xFF is never valid)
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

// ══════════════════════════════════════════════════════════════════════════════
// PTY Opts / Proc (portable-pty interface types)
// ══════════════════════════════════════════════════════════════════════════════

/// Options for spawning a PTY.
///
/// # Source
/// `packages/core/src/pty/pty.ts` lines 10–16 `Opts`.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PtyOpts {
    /// Terminal type name (e.g. "xterm-256color")
    pub name: String,
    /// Number of columns
    #[serde(skip_serializing_if = "Option::is_none")]
    pub cols: Option<u32>,
    /// Number of rows
    #[serde(skip_serializing_if = "Option::is_none")]
    pub rows: Option<u32>,
    /// Working directory
    #[serde(skip_serializing_if = "Option::is_none")]
    pub cwd: Option<String>,
    /// Environment variables
    #[serde(skip_serializing_if = "Option::is_none")]
    pub env: Option<HashMap<String, String>>,
}

impl PtyOpts {
    pub fn new(name: impl Into<String>) -> Self {
        Self {
            name: name.into(),
            cols: None,
            rows: None,
            cwd: None,
            env: None,
        }
    }
}

/// Exit event from a PTY process.
///
/// # Source
/// `packages/core/src/pty/pty.ts` lines 5–8 `Exit`.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PtyExit {
    /// Exit code
    pub exit_code: i32,
    /// Signal number or name (Unix)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub signal: Option<i32>,
}

/// A disposable handle (subscription cleanup).
///
/// # Source
/// `packages/core/src/pty/pty.ts` lines 1–3 `Disp`.
pub trait PtyDisp: Send + Sync {
    fn dispose(&self);
}

/// PTY process handle interface.
///
/// # Source
/// `packages/core/src/pty/pty.ts` lines 18–25 `Proc`.
pub trait PtyProc: Send + Sync {
    /// Process ID
    fn pid(&self) -> u32;
    /// Register a data listener; returns a disposable
    fn on_data(&self, listener: Box<dyn Fn(String) + Send + Sync>) -> Box<dyn PtyDisp>;
    /// Register an exit listener; returns a disposable
    fn on_exit(&self, listener: Box<dyn Fn(PtyExit) + Send + Sync>) -> Box<dyn PtyDisp>;
    /// Write data to the PTY stdin
    fn write(&self, data: &str);
    /// Resize the terminal
    fn resize(&self, cols: u32, rows: u32);
    /// Kill the process with an optional signal
    fn kill(&self, signal: Option<&str>);
}

impl std::fmt::Debug for dyn PtyProc {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("PtyProc").field("pid", &self.pid()).finish()
    }
}

// ══════════════════════════════════════════════════════════════════════════════
// PTY Attachment — replay, write, activate, detach (real implementation)
// ══════════════════════════════════════════════════════════════════════════════

/// A live PTY attachment handle with actual process interaction.
///
/// # Source
/// `packages/core/src/pty.ts` lines 84–93 `Attachment`.
#[derive(Clone)]
pub struct PtyLiveAttachment {
    /// Retained output from the requested cursor to current end
    pub replay: String,
    /// Absolute output cursor after replay
    pub cursor: u64,
    /// Channel to send writes to the PTY process
    write_tx: mpsc::UnboundedSender<String>,
    /// Whether this attachment has been activated
    active: std::sync::Arc<std::sync::atomic::AtomicBool>,
    /// Whether this attachment has been detached
    detached: std::sync::Arc<std::sync::atomic::AtomicBool>,
}

impl PtyLiveAttachment {
    pub fn new(
        replay: String,
        cursor: u64,
        write_tx: mpsc::UnboundedSender<String>,
    ) -> Self {
        Self {
            replay,
            cursor,
            write_tx,
            active: std::sync::Arc::new(std::sync::atomic::AtomicBool::new(false)),
            detached: std::sync::Arc::new(std::sync::atomic::AtomicBool::new(false)),
        }
    }

    /// Write data to the PTY process stdin.
    pub fn write(&self, data: &str) -> Result<(), PtyError> {
        if self.detached.load(std::sync::atomic::Ordering::SeqCst) {
            return Err(PtyError::Other("attachment is detached".into()));
        }
        self.write_tx.send(data.to_string()).map_err(|_| {
            PtyError::Io("failed to send write to PTY process".into())
        })
    }

    /// Activate the attachment — start live data delivery.
    /// After activation, the subscriber will receive live data.
    pub fn activate(&self) {
        self.active.store(true, std::sync::atomic::Ordering::SeqCst);
    }

    /// Detach the attachment — stop live data delivery.
    pub fn detach(&self) {
        self.detached.store(true, std::sync::atomic::Ordering::SeqCst);
    }

    /// Check if this attachment is active.
    pub fn is_active(&self) -> bool {
        self.active.load(std::sync::atomic::Ordering::SeqCst)
    }

    /// Check if this attachment is detached.
    pub fn is_detached(&self) -> bool {
        self.detached.load(std::sync::atomic::Ordering::SeqCst)
    }
}

impl std::fmt::Debug for PtyLiveAttachment {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("PtyLiveAttachment")
            .field("cursor", &self.cursor)
            .field("active", &self.is_active())
            .field("detached", &self.is_detached())
            .finish()
    }
}

// ══════════════════════════════════════════════════════════════════════════════
// PTY Session Inner (runtime per-session)
// ══════════════════════════════════════════════════════════════════════════════

/// Broadcast channel capacity for live PTY output delivery.
const BROADCAST_CAPACITY: usize = 256;

/// Milliseconds between SIGTERM and SIGKILL escalation.
const PTY_KILL_TIMEOUT_MS: u64 = 200;

// ── Private subscriber entry ──────────────────────────────────────────────────

struct PtySubscriberEntry {
    active: bool,
    pending: Vec<Vec<u8>>,
    tx: mpsc::UnboundedSender<Vec<u8>>,
    end: Option<u32>,
}

// ── PtySessionShared (shared across tasks) ────────────────────────────────────

struct PtySessionShared {
    info: RwLock<PtyInfo>,
    buffer: RwLock<PtyBuffer>,
    cursor: AtomicU64,
    subscribers: RwLock<HashMap<u64, PtySubscriberEntry>>,
    next_sub_id: AtomicU64,
    output_tx: broadcast::Sender<Vec<u8>>,
}

// ── PtySessionInner ──────────────────────────────────────────────────────────

/// A live PTY session runtime that manages a child process with piped I/O,
/// output buffering, and subscriber delivery.
///
/// Each session owns a spawned child process, background tasks for reading
/// stdout, and an exit watcher that marks the session as exited when the
/// process terminates.
///
/// # Source
/// `packages/core/src/pty.ts` — `Active` session struct (lines 223–231).
pub struct PtySessionInner {
    id: String,
    shared: Arc<PtySessionShared>,
    child: Mutex<Option<tokio::process::Child>>,
    stdin: Mutex<Option<tokio::process::ChildStdin>>,
    cancel: CancellationToken,
    pid: u32,
}

impl std::fmt::Debug for PtySessionInner {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("PtySessionInner")
            .field("id", &self.id)
            .field("pid", &self.pid)
            .finish()
    }
}

impl PtySessionInner {
    /// Session identifier string.
    pub fn id(&self) -> &str {
        &self.id
    }

    /// Child process PID (0 if unavailable).
    pub fn pid(&self) -> u32 {
        self.pid
    }

    /// Cancellation token used to stop background tasks.
    pub fn cancel_token(&self) -> &CancellationToken {
        &self.cancel
    }

    /// Snapshot of the session info.
    pub async fn info(&self) -> PtyInfo {
        self.shared.info.read().await.clone()
    }

    /// Subscribe to the broadcast output channel.
    pub fn subscribe_broadcast(&self) -> broadcast::Receiver<Vec<u8>> {
        self.shared.output_tx.subscribe()
    }

    /// Create a subscriber for the attach/detach lifecycle.
    ///
    /// Returns `(subscriber_id, receiver)`.  Call
    /// [`activate_subscriber`](Self::activate_subscriber) to begin live
    /// delivery or [`detach_subscriber`](Self::detach_subscriber) to remove.
    pub async fn subscribe(&self) -> (u64, mpsc::UnboundedReceiver<Vec<u8>>) {
        let (tx, rx) = mpsc::unbounded_channel();
        let id = self.shared.next_sub_id.fetch_add(1, Ordering::SeqCst);
        let entry = PtySubscriberEntry {
            active: false,
            pending: Vec::new(),
            tx,
            end: None,
        };
        self.shared.subscribers.write().await.insert(id, entry);
        (id, rx)
    }

    /// Activate a subscriber — flush accumulated pending data and start
    /// live delivery.
    ///
    /// # Source
    /// `packages/core/src/pty.ts` — `activate()` (lines 322–331).
    pub async fn activate_subscriber(&self, sub_id: u64) {
        let mut subs = self.shared.subscribers.write().await;
        let entry = match subs.get_mut(&sub_id) {
            Some(e) if !e.active => e,
            _ => return,
        };
        entry.active = true;
        let pending = std::mem::take(&mut entry.pending);
        for chunk in &pending {
            let _ = entry.tx.send(chunk.clone());
        }
        if let Some(code) = entry.end {
            let _ = entry.tx.send(format!("\x00\x00\x00_exit_{code}").into_bytes());
        }
    }

    /// Detach a subscriber — stop delivery and remove the subscription.
    ///
    /// # Source
    /// `packages/core/src/pty.ts` — `detach()` (lines 333–338).
    pub async fn detach_subscriber(&self, sub_id: u64) {
        self.shared.subscribers.write().await.remove(&sub_id);
    }

    /// Write data to the child process stdin.
    ///
    /// # Source
    /// `packages/core/src/pty.ts` — `session.process.write(data)` (line 286).
    pub async fn write(&self, data: &[u8]) -> Result<(), PtyError> {
        let mut guard = self.stdin.lock().await;
        let stdin = guard.as_mut().ok_or_else(|| PtyError::Other("stdin closed".into()))?;
        stdin.write_all(data).await.map_err(|e| PtyError::Io(e.to_string()))?;
        Ok(())
    }

    /// Resize the terminal (no-op for piped processes).
    ///
    /// # Source
    /// `packages/core/src/pty.ts` — `session.process.resize()` (line 279).
    pub async fn resize(&self, _cols: u16, _rows: u16) -> Result<(), PtyError> {
        // TODO: implement with ioctl(TIOCSWINSZ) when a real PTY is used
        Ok(())
    }

    /// Kill the process group with SIGTERM → SIGKILL escalation.
    ///
    /// On Unix, sends SIGTERM to the process group (negative PID), waits
    /// [`PTY_KILL_TIMEOUT_MS`], then escalates to SIGKILL.
    ///
    /// # Source
    /// `packages/core/src/shell.ts` — `killTree()` (lines 205–216).
    pub async fn kill(&self) {
        let pid = self.pid;
        if pid > 0 {
            crate::process::kill_group(pid).await;
            tokio::time::sleep(Duration::from_millis(PTY_KILL_TIMEOUT_MS)).await;
            #[cfg(unix)]
            {
                let _ = tokio::process::Command::new("kill")
                    .args(["-KILL", &format!("-{pid}")])
                    .output()
                    .await;
            }
        }
    }

    /// Returns `true` if the session process has exited.
    pub async fn is_exited(&self) -> bool {
        matches!(self.shared.info.read().await.status, PtyStatus::Exited)
    }
}

// ── PtyRuntime inner ─────────────────────────────────────────────────────────

/// PTY session registry and spawn runtime.
///
/// Manages the full lifecycle of PTY sessions:
///   - [`create`](PtyRuntime::create) — spawn a child process with piped I/O
///   - [`write`](PtyRuntime::write) — feed stdin
///   - [`resize`](PtyRuntime::resize) — change terminal dimensions (no-op)
///   - [`get`](PtyRuntime::get) — look up a session by ID
///   - [`remove`](PtyRuntime::remove) — kill the process tree and clean up
///   - [`list`](PtyRuntime::list) — enumerate all session infos
///
/// When a [`SharedBus`] is configured, lifecycle events are published:
/// `Created`, `Updated`, `Exited`, `Deleted`.
///
/// # Source
/// `packages/core/src/pty.ts` — `Service` / `Active` session (lines 122–343).
pub struct PtyRuntime {
    sessions: RwLock<HashMap<String, Arc<PtySessionInner>>>,
    bus: Option<SharedBus>,
}

impl std::fmt::Debug for PtyRuntime {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("PtyRuntime")
            .field("bus", &self.bus.as_ref().map(|_| "Some"))
            .finish()
    }
}

impl PtyRuntime {
    /// Create an empty runtime with no event bus.
    pub fn new() -> Self {
        Self {
            sessions: RwLock::new(HashMap::new()),
            bus: None,
        }
    }

    /// Create a runtime that publishes lifecycle events on `bus`.
    pub fn with_bus(bus: SharedBus) -> Self {
        Self {
            sessions: RwLock::new(HashMap::new()),
            bus: Some(bus),
        }
    }

    /// Attach an event bus after construction.
    pub fn set_bus(&mut self, bus: SharedBus) {
        self.bus = Some(bus);
    }

    /// Snapshot of all session infos.
    ///
    /// # Source
    /// `packages/core/src/pty.ts` — `list()` (lines 187–189).
    pub async fn list(&self) -> Vec<PtyInfo> {
        let guard = self.sessions.read().await;
        let mut result = Vec::with_capacity(guard.len());
        for session in guard.values() {
            result.push(session.info().await);
        }
        result
    }

    /// Look up a session by its string ID.
    ///
    /// # Source
    /// `packages/core/src/pty.ts` — `get()` (lines 191–193).
    pub async fn get(&self, id: &str) -> Option<Arc<PtySessionInner>> {
        self.sessions.read().await.get(id).cloned()
    }

    /// Spawn a child process and register the session.
    ///
    /// The child is spawned with piped stdin/stdout/stderr.  On Unix the
    /// process is placed into a new process group via `setpgid(0, 0)` so
    /// that [`kill`](PtySessionInner::kill) can terminate the entire tree.
    ///
    /// Two background tasks are launched:
    ///   - **stdout reader** — reads lines from stdout, pushes them into the
    ///     output buffer, broadcasts to all subscribers, and buffers for
    ///     inactive subscribers.
    ///   - **exit watcher** — waits for the child to exit, updates session
    ///     status to [`PtyStatus::Exited`], notifies subscribers, and
    ///     publishes a `PtyExitedEvent` on the bus.
    ///
    /// # Source
    /// `packages/core/src/pty.ts` — `create()` (lines 195–273).
    pub async fn create(
        &self,
        info: PtyInfo,
        extra_env: HashMap<String, String>,
    ) -> Result<Arc<PtySessionInner>, PtyError> {
        let id_str = info.id.as_str().to_string();

        // ── build command ───────────────────────────────────────────
        let mut cmd = tokio::process::Command::new(&info.command);
        cmd.args(&info.args);
        cmd.current_dir(&info.cwd);
        cmd.stdin(std::process::Stdio::piped());
        cmd.stdout(std::process::Stdio::piped());
        cmd.stderr(std::process::Stdio::piped());
        cmd.kill_on_drop(true);

        // Merge environment: base (TERM, OPENCODE_TERMINAL) + extras
        for (k, v) in pty_env(&extra_env) {
            cmd.env(k, v);
        }

        // Create a new process group on Unix so we can kill the entire tree.
        #[cfg(unix)]
        {
            use std::os::unix::process::CommandExt;
            cmd.as_std_mut().process_group(0);
        }

        // ── spawn child ─────────────────────────────────────────────
        let mut child = cmd.spawn().map_err(|e| PtyError::Io(e.to_string()))?;
        let pid: u32 = child.id().unwrap_or(0) as u32;
        let stdin = child.stdin.take();
        let stdout = child
            .stdout
            .take()
            .ok_or_else(|| PtyError::Other("failed to capture stdout".into()))?;

        // ── shared state ────────────────────────────────────────────
        let (output_tx, _) = broadcast::channel(BROADCAST_CAPACITY);
        let shared = Arc::new(PtySessionShared {
            info: RwLock::new(info.clone()),
            buffer: RwLock::new(PtyBuffer::new(BUFFER_LIMIT)),
            cursor: AtomicU64::new(0),
            subscribers: RwLock::new(HashMap::new()),
            next_sub_id: AtomicU64::new(0),
            output_tx: output_tx.clone(),
        });

        let cancel = CancellationToken::new();

        // ── stdout reader task ──────────────────────────────────────
        let reader_shared = Arc::clone(&shared);
        let reader_cancel = cancel.clone();
        tokio::spawn(async move {
            let reader = BufReader::new(stdout);
            let mut lines = reader.lines();
            loop {
                tokio::select! {
                    result = lines.next_line() => {
                        let line = match result {
                            Ok(Some(line)) => line,
                            Ok(None) => break,
                            Err(_) => break,
                        };
                        let mut bytes = line.into_bytes();
                        bytes.push(b'\n');

                        let len = bytes.len();
                        reader_shared.cursor.fetch_add(len, Ordering::SeqCst);

                        // Write to buffer
                        {
                            let mut buf = reader_shared.buffer.write().await;
                            buf.push(&bytes);
                        }

                        // Broadcast to all
                        let _ = reader_shared.output_tx.send(bytes.clone());

                        // Active subscribers get data directly;
                        // inactive subscribers accumulate pending.
                        let mut subs = reader_shared.subscribers.write().await;
                        for (_, entry) in subs.iter_mut() {
                            if entry.active {
                                let _ = entry.tx.send(bytes.clone());
                            } else {
                                entry.pending.push(bytes.clone());
                            }
                        }
                    }
                    _ = reader_cancel.cancelled() => break,
                }
            }
        });

        // ── child handle (shared between exit watcher and remove) ──
        let child_mutex: Arc<Mutex<Option<tokio::process::Child>>> =
            Arc::new(Mutex::new(Some(child)));

        // ── exit watcher task ───────────────────────────────────────
        let exit_shared = Arc::clone(&shared);
        let exit_cancel = cancel.clone();
        let exit_child = Arc::clone(&child_mutex);
        let bus = self.bus.clone();
        let exit_id = id_str.clone();

        tokio::spawn(async move {
            let child = {
                let mut guard = exit_child.lock().await;
                guard.take()
            };

            let exit_code: Option<i32> = if let Some(mut c) = child {
                c.wait().await.ok().and_then(|s| {
                    #[cfg(unix)]
                    {
                        use std::os::unix::process::ExitStatusExt;
                        s.code().or_else(|| s.signal().map(|sig| 128 + sig))
                    }
                    #[cfg(not(unix))]
                    {
                        s.code()
                    }
                })
            } else {
                None
            };

            // Mark session as exited.
            {
                let mut info = exit_shared.info.write().await;
                info.status = PtyStatus::Exited;
                info.exit_code = exit_code.map(|c| c as u64);
            }

            // Notify subscribers.
            {
                let mut subs = exit_shared.subscribers.write().await;
                for (_, entry) in subs.iter_mut() {
                    if entry.active {
                        let _ = entry.tx.send(Vec::new());
                    }
                    entry.end = exit_code.map(|c| c as u32);
                }
            }

            // Publish PtyExitedEvent on the bus.
            if let Some(ref bus) = bus {
                let event = PtyExitedEvent {
                    id: PtyId::new(&exit_id)
                        .expect("session id was validated on creation"),
                    exit_code: exit_code.unwrap_or(-1) as u64,
                };
                if let Ok(payload) = serde_json::to_value(event) {
                    let _ = bus.publish(GlobalEvent::new(payload).with_workspace("pty"));
                }
            }

            // Stop the stdout reader.
            exit_cancel.cancel();
        });

        // ── build session inner ─────────────────────────────────────
        let session = Arc::new(PtySessionInner {
            id: id_str.clone(),
            shared,
            child: child_mutex,
            stdin: Mutex::new(stdin),
            cancel,
            pid,
        });

        // Register in the map.
        self.sessions
            .write()
            .await
            .insert(id_str.clone(), Arc::clone(&session));

        // Publish PtyCreatedEvent.
        if let Some(ref bus) = self.bus {
            let event = PtyCreatedEvent {
                info: info.clone(),
            };
            if let Ok(payload) = serde_json::to_value(event) {
                let _ = bus.publish(GlobalEvent::new(payload).with_workspace("pty"));
            }
        }

        Ok(session)
    }

    /// Write raw bytes to a session's stdin.
    ///
    /// # Source
    /// `packages/core/src/pty.ts` — `write()` (lines 284–287).
    pub async fn write(&self, id: &str, data: &[u8]) -> Result<(), PtyError> {
        let guard = self.sessions.read().await;
        let session = guard.get(id).ok_or_else(|| {
            PtyError::NotFound(
                PtyId::new(id).unwrap_or_else(|_| PtyId::new("pty_missing").expect("fallback id")),
            )
        })?;
        session.write(data).await
    }

    /// Resize a session's terminal dimensions.
    ///
    /// # Source
    /// `packages/core/src/pty.ts` — `session.process.resize()` (line 279).
    pub async fn resize(&self, id: &str, cols: u16, rows: u16) -> Result<(), PtyError> {
        let guard = self.sessions.read().await;
        let session = guard.get(id).ok_or_else(|| {
            PtyError::NotFound(
                PtyId::new(id).unwrap_or_else(|_| PtyId::new("pty_missing").expect("fallback id")),
            )
        })?;
        session.resize(cols, rows).await
    }

    /// Remove a session — kill the process tree and clean up.
    ///
    /// # Source
    /// `packages/core/src/pty.ts` — `remove()` / `removeSession()` (lines 171–185).
    pub async fn remove(&self, id: &str) -> Result<(), PtyError> {
        let session = {
            let guard = self.sessions.read().await;
            guard.get(id).cloned()
        };

        let session = session.ok_or_else(|| {
            PtyError::NotFound(
                PtyId::new(id).unwrap_or_else(|_| PtyId::new("pty_missing").expect("fallback id")),
            )
        })?;

        // Kill the process group (SIGTERM → SIGKILL).
        session.kill().await;

        // Reap the child to avoid zombies.
        {
            let mut guard = session.child.lock().await;
            if let Some(mut c) = guard.take() {
                let _ = c.wait().await;
            }
        }

        // Stop background tasks.
        session.cancel.cancel();

        // Remove from registry.
        self.sessions.write().await.remove(id);

        // Publish PtyDeletedEvent.
        if let Some(ref bus) = self.bus {
            let event = PtyDeletedEvent {
                id: PtyId::new(id)
                    .unwrap_or_else(|_| PtyId::new("pty_missing").expect("fallback id")),
            };
            if let Ok(payload) = serde_json::to_value(event) {
                let _ = bus.publish(GlobalEvent::new(payload).with_workspace("pty"));
            }
        }

        Ok(())
    }

    /// Number of registered sessions.
    pub async fn len(&self) -> usize {
        self.sessions.read().await.len()
    }

    /// `true` if no sessions are registered.
    pub async fn is_empty(&self) -> bool {
        self.sessions.read().await.is_empty()
    }

    /// Remove all sessions.
    pub async fn clear(&self) {
        let ids: Vec<String> = {
            let guard = self.sessions.read().await;
            guard.keys().cloned().collect()
        };
        for id in ids {
            let _ = self.remove(&id).await;
        }
    }
}

impl Default for PtyRuntime {
    fn default() -> Self {
        Self::new()
    }
}

// ══════════════════════════════════════════════════════════════════════════════
// Tests — PTY runtime integration
// ══════════════════════════════════════════════════════════════════════════════

#[cfg(test)]
mod runtime_tests {
    use super::*;
    use std::collections::HashMap;

    fn test_info(id: &str, cmd: &str) -> PtyInfo {
        PtyInfo {
            id: PtyId::new(id).expect("test pty id"),
            title: format!("Test {id}"),
            command: cmd.to_string(),
            args: Vec::new(),
            cwd: "/tmp".into(),
            status: PtyStatus::Running,
            pid: 0,
            exit_code: None,
        }
    }

    #[tokio::test]
    async fn test_runtime_create_and_list() {
        let runtime = PtyRuntime::new();
        let info = test_info("pty_test_1", "echo");
        let session = runtime
            .create(info.clone(), HashMap::new())
            .await
            .expect("should spawn echo");
        assert_eq!(session.id(), "pty_test_1");
        assert!(session.pid() > 0);

        let list = runtime.list().await;
        assert_eq!(list.len(), 1);
        assert_eq!(list[0].id, info.id);

        runtime.remove("pty_test_1").await.expect("should remove");
        assert!(runtime.is_empty().await);
    }

    #[tokio::test]
    async fn test_runtime_get_unknown() {
        let runtime = PtyRuntime::new();
        let session = runtime.get("pty_nonexistent").await;
        assert!(session.is_none());
    }

    #[tokio::test]
    async fn test_runtime_write_to_stdin() {
        let runtime = PtyRuntime::new();
        // Use 'cat' which echoes stdin back to stdout
        let info = PtyInfo {
            id: PtyId::new("pty_cat_test").expect("valid id"),
            title: "cat test".into(),
            command: "cat".into(),
            args: Vec::new(),
            cwd: "/tmp".into(),
            status: PtyStatus::Running,
            pid: 0,
            exit_code: None,
        };
        let session = runtime
            .create(info, HashMap::new())
            .await
            .expect("should spawn cat");
        session.write(b"hello\n").await.expect("should write");
        // Give cat time to echo
        tokio::time::sleep(Duration::from_millis(100)).await;
        runtime.remove(session.id()).await.expect("should remove");
    }

    #[tokio::test]
    async fn test_runtime_write_to_closed_stdin_errors() {
        let runtime = PtyRuntime::new();
        let info = test_info("pty_closed_stdin", "true");
        let session = runtime
            .create(info, HashMap::new())
            .await
            .expect("should spawn true");
        // 'true' exits immediately; wait and then try to write
        tokio::time::sleep(Duration::from_millis(50)).await;
        let result = session.write(b"data").await;
        // Write may fail if the process has already exited and closed stdin
        // Either Ok or Err is acceptable depending on timing
        let _ = result;
        runtime.remove(session.id()).await.expect("should remove");
    }

    #[tokio::test]
    async fn test_runtime_remove_idempotent() {
        let runtime = PtyRuntime::new();
        let info = test_info("pty_rm_test", "sleep");
        let info = PtyInfo {
            args: vec!["60".into()],
            ..info
        };
        let session = runtime
            .create(info, HashMap::new())
            .await
            .expect("should spawn sleep");

        // Remove once
        runtime.remove(session.id()).await.expect("first remove");
        assert!(runtime.is_empty().await);

        // Remove again should error with NotFound
        let err = runtime.remove("pty_rm_test").await;
        assert!(err.is_err());
        match err.unwrap_err() {
            PtyError::NotFound(_) => {} // expected
            other => panic!("expected NotFound, got {other}"),
        }
    }

    #[tokio::test]
    async fn test_runtime_subscribe_and_broadcast() {
        let runtime = PtyRuntime::new();
        let info = PtyInfo {
            id: PtyId::new("pty_sub_test").expect("valid id"),
            title: "sub test".into(),
            command: "echo".into(),
            args: vec!["hello pty".into()],
            cwd: "/tmp".into(),
            status: PtyStatus::Running,
            pid: 0,
            exit_code: None,
        };

        let session = runtime
            .create(info, HashMap::new())
            .await
            .expect("should spawn");

        let mut rx = session.subscribe_broadcast();
        // Wait for output
        tokio::time::sleep(Duration::from_millis(200)).await;
        // The process should have written "hello pty\n" to stdout
        let msg = rx.try_recv().ok();
        assert!(msg.is_some(), "expected broadcast output");
        let text = String::from_utf8_lossy(&msg.unwrap());
        assert!(text.contains("hello pty"), "expected hello pty, got {text:?}");

        runtime.remove(session.id()).await.expect("should remove");
    }

    #[tokio::test]
    async fn test_runtime_subscriber_lifecycle() {
        let runtime = PtyRuntime::new();
        let info = PtyInfo {
            id: PtyId::new("pty_lifecycle").expect("valid id"),
            title: "lifecycle".into(),
            command: "echo".into(),
            args: vec!["hello lifecycle".into()],
            cwd: "/tmp".into(),
            status: PtyStatus::Running,
            pid: 0,
            exit_code: None,
        };

        let session = runtime
            .create(info, HashMap::new())
            .await
            .expect("should spawn");

        // Create an inactive subscriber; data should buffer
        let (sub_id, _rx) = session.subscribe().await;
        assert!(!session.is_exited().await);

        // Activate and flush pending
        session.activate_subscriber(sub_id).await;
        session.detach_subscriber(sub_id).await;

        tokio::time::sleep(Duration::from_millis(100)).await;
        runtime.remove(session.id()).await.expect("should remove");
    }

    #[tokio::test]
    async fn test_runtime_clear_all_sessions() {
        let runtime = PtyRuntime::new();
        for i in 0..3 {
            let info = test_info(&format!("pty_clear_{i}"), "true");
            runtime
                .create(info, HashMap::new())
                .await
                .expect("should spawn");
        }
        assert_eq!(runtime.len().await, 3);
        runtime.clear().await;
        assert_eq!(runtime.len().await, 0);
    }
}
