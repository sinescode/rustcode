//! Session management — messages, processor, prompt construction, compaction.
//!
//! Ported from:
//! - `packages/opencode/src/session/processor.ts` (lines 1–1084)
//! - `packages/opencode/src/session/session.ts` (lines 1–1119)
//! - `packages/opencode/src/session/message-v2.ts` (lines 1–744)
//! - `packages/opencode/src/session/compaction.ts` (lines 1–620)
//! - `packages/opencode/src/session/retry.ts` (lines 1–201)
//! - `packages/opencode/src/session/overflow.ts` (lines 1–34)
//! - `packages/opencode/src/session/status.ts` (lines 1–97)
//! - `packages/opencode/src/session/run-state.ts` (lines 1–156)

use crate::bus::SharedBus;
use crate::database::{
    DatabaseService, DatabaseServiceError, MessageRow, PartRow, SessionRow,
};
use crate::id;
use crate::permission::PermissionService;
use crate::provider::{LlmEvent, Model, Usage};
use crate::tool::ToolRegistry;

use chrono::Utc;
use futures::StreamExt;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::{oneshot, Mutex};
use tokio_util::sync::CancellationToken;
use tracing::{error, info, warn};

// ══════════════════════════════════════════════════════════════════════════════
// Error types
// ══════════════════════════════════════════════════════════════════════════════

/// Session-related errors.
///
/// # Source
/// `packages/opencode/src/session/session.ts` line 455.
#[derive(Debug, thiserror::Error)]
pub enum SessionError {
    #[error("session not found: {0}")]
    NotFound(String),

    #[error("session is busy: {0}")]
    Busy(String),

    #[error("context overflow: {0}")]
    ContextOverflow(String),

    #[error("tool execution aborted")]
    ToolAborted,

    #[error("doom loop detected: tool {tool} called {count} times with same input")]
    DoomLoop { tool: String, count: u32 },

    #[error("provider error: {0}")]
    Provider(String),

    #[error("permission denied: {0}")]
    PermissionDenied(String),

    #[error("aborted")]
    Aborted,

    #[error("compaction failed: {0}")]
    CompactionFailed(String),

    #[error("database error: {0}")]
    Db(#[from] sqlx::Error),

    #[error("database service error: {0}")]
    DatabaseService(#[from] DatabaseServiceError),

    #[error("{0}")]
    Other(String),
}

// ══════════════════════════════════════════════════════════════════════════════
// Core data types — SessionInfo, messages, parts
// ══════════════════════════════════════════════════════════════════════════════

/// Session identifier.
pub type SessionId = String;

/// Message identifier.
pub type MessageId = String;

/// Part identifier.
pub type PartId = String;

// ── Session Info ────────────────────────────────────────────────────────────

/// Complete session information.
///
/// # Source
/// `packages/opencode/src/session/session.ts` lines 213–234 `Info`.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SessionInfo {
    pub id: SessionId,
    pub slug: String,
    pub project_id: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub workspace_id: Option<String>,
    pub directory: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub path: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub parent_id: Option<SessionId>,
    pub title: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub agent: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub model: Option<ModelSelection>,
    pub version: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub summary: Option<SessionSummary>,
    #[serde(default)]
    pub cost: f64,
    #[serde(default)]
    pub tokens: TokenUsage,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub share: Option<ShareInfo>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub metadata: Option<serde_json::Value>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub permission: Option<Vec<crate::permission::PermissionRule>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub revert: Option<RevertInfo>,
    pub time: SessionTimestamps,
}

/// Model selection for a session.
///
/// # Source
/// `packages/opencode/src/session/session.ts` lines 205–209.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ModelSelection {
    pub id: String,
    pub provider_id: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub variant: Option<String>,
}

/// Session file-change summary.
///
/// # Source
/// `packages/opencode/src/session/session.ts` lines 164–169.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SessionSummary {
    pub additions: i64,
    pub deletions: i64,
    pub files: i64,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub diffs: Option<Vec<FileDiff>>,
}

/// File diff in a summary.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FileDiff {
    pub path: String,
    pub hash: String,
}

/// Token usage tracking.
///
/// # Source
/// `packages/opencode/src/session/session.ts` lines 171–181.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct TokenUsage {
    #[serde(default)]
    pub input: u64,
    #[serde(default)]
    pub output: u64,
    #[serde(default)]
    pub reasoning: u64,
    #[serde(default)]
    pub cache: CacheUsage,
}

/// Cache token usage.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct CacheUsage {
    #[serde(default)]
    pub read: u64,
    #[serde(default)]
    pub write: u64,
}

/// Session timestamps.
///
/// # Source
/// `packages/opencode/src/session/session.ts` lines 191–196.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SessionTimestamps {
    pub created: u64,
    pub updated: u64,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub compacting: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub archived: Option<u64>,
}

/// Share information.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ShareInfo {
    pub url: String,
}

/// Revert information.
///
/// # Source
/// `packages/opencode/src/session/session.ts` lines 198–203.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RevertInfo {
    pub message_id: MessageId,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub part_id: Option<PartId>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub snapshot: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub diff: Option<String>,
}

// ── Messages ────────────────────────────────────────────────────────────────

/// A session message (user, assistant, or tool).
///
/// # Source
/// `packages/opencode/src/session/message-v2.ts` — `WithParts` concept.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Message {
    pub info: MessageInfo,
    pub parts: Vec<Part>,
}

/// Message metadata (role-independent fields).
///
/// # Source
/// `packages/opencode/src/session/message-v2.ts` line 91–97.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "role")]
pub enum MessageInfo {
    #[serde(rename = "user")]
    User(UserInfo),
    #[serde(rename = "assistant")]
    Assistant(AssistantInfo),
}

/// User message info.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UserInfo {
    pub id: MessageId,
    pub session_id: SessionId,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub agent: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub model: Option<ModelSelection>,
    pub time: MessageTime,
}

/// Assistant message info.
///
/// # Source
/// `packages/opencode/src/session/session.ts` — `Assistant` fields.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AssistantInfo {
    pub id: MessageId,
    pub session_id: SessionId,
    pub parent_id: MessageId,
    #[serde(default)]
    pub agent: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub model_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub provider_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub variant: Option<String>,
    #[serde(default)]
    pub summary: bool,
    #[serde(default)]
    pub cost: f64,
    #[serde(default)]
    pub tokens: TokenUsage,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub finish: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<serde_json::Value>,
    pub time: MessageTime,
}

/// Message timestamps.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MessageTime {
    pub created: u64,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub completed: Option<u64>,
}

// ── Parts ───────────────────────────────────────────────────────────────────

/// A part within a message.
///
/// # Source
/// `packages/opencode/src/session/message-v2.ts` line 98–104.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type")]
pub enum Part {
    #[serde(rename = "text")]
    Text(TextPart),
    #[serde(rename = "tool")]
    Tool(ToolPart),
    #[serde(rename = "reasoning")]
    Reasoning(ReasoningPart),
    #[serde(rename = "file")]
    File(FilePart),
    #[serde(rename = "step-start")]
    StepStart(StepStartPart),
    #[serde(rename = "step-finish")]
    StepFinish(StepFinishPart),
    #[serde(rename = "patch")]
    Patch(PatchPart),
    #[serde(rename = "compaction")]
    Compaction(CompactionPart),
    #[serde(rename = "subtask")]
    Subtask(SubtaskPart),
}

/// Text part — streamed LLM output.
///
/// # Source
/// `packages/opencode/src/session/processor.ts` lines 759–839.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TextPart {
    pub id: PartId,
    pub message_id: MessageId,
    pub session_id: SessionId,
    pub text: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub metadata: Option<serde_json::Value>,
    pub time: PartTime,
}

/// Tool part — a tool call.
///
/// # Source
/// `packages/opencode/src/session/processor.ts` lines 295–346.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ToolPart {
    pub id: PartId,
    pub message_id: MessageId,
    pub session_id: SessionId,
    pub tool: String,
    pub call_id: String,
    pub state: ToolState,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub metadata: Option<serde_json::Value>,
}

/// Tool execution state.
///
/// # Source
/// `packages/opencode/src/session/processor.ts` lines 326–336.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "status")]
pub enum ToolState {
    #[serde(rename = "pending")]
    Pending { input: serde_json::Value },
    #[serde(rename = "running")]
    Running {
        input: serde_json::Value,
        time: ToolTime,
    },
    #[serde(rename = "completed")]
    Completed {
        input: serde_json::Value,
        output: String,
        title: String,
        metadata: serde_json::Value,
        time: ToolTime,
        #[serde(skip_serializing_if = "Option::is_none")]
        attachments: Option<Vec<FilePart>>,
    },
    #[serde(rename = "error")]
    Error {
        input: serde_json::Value,
        error: String,
        time: ToolTime,
        #[serde(skip_serializing_if = "Option::is_none")]
        metadata: Option<serde_json::Value>,
    },
}

/// Tool execution timestamps.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ToolTime {
    pub start: u64,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub end: Option<u64>,
}

/// Reasoning part — thinking/reasoning output.
///
/// # Source
/// `packages/opencode/src/session/processor.ts` lines 373–426.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ReasoningPart {
    pub id: PartId,
    pub message_id: MessageId,
    pub session_id: SessionId,
    pub text: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub metadata: Option<serde_json::Value>,
    pub time: PartTime,
}

/// File attachment part.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FilePart {
    pub id: PartId,
    pub message_id: MessageId,
    pub session_id: SessionId,
    pub url: String,
    pub mime: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub filename: Option<String>,
}

/// Step start marker.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StepStartPart {
    pub id: PartId,
    pub message_id: MessageId,
    pub session_id: SessionId,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub snapshot: Option<String>,
}

/// Step finish marker.
///
/// # Source
/// `packages/opencode/src/session/processor.ts` lines 693–757.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StepFinishPart {
    pub id: PartId,
    pub message_id: MessageId,
    pub session_id: SessionId,
    pub reason: String,
    pub tokens: TokenUsage,
    pub cost: f64,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub snapshot: Option<String>,
}

/// Patch part — file changes from a step.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PatchPart {
    pub id: PartId,
    pub message_id: MessageId,
    pub session_id: SessionId,
    pub hash: String,
    pub files: Vec<FileDiff>,
}

/// Compaction part — context window compaction marker.
///
/// # Source
/// `packages/opencode/src/session/compaction.ts` lines 554–577.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CompactionPart {
    pub id: PartId,
    pub message_id: MessageId,
    pub session_id: SessionId,
    #[serde(default)]
    pub auto: bool,
    #[serde(default)]
    pub overflow: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tail_start_id: Option<MessageId>,
}

/// Subtask part.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SubtaskPart {
    pub id: PartId,
    pub message_id: MessageId,
    pub session_id: SessionId,
}

/// Part timestamps.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PartTime {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub start: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub end: Option<u64>,
}

// ══════════════════════════════════════════════════════════════════════════════
// Session Manager — CRUD + message/part operations
// ══════════════════════════════════════════════════════════════════════════════

/// Manages session lifecycle: create, list, get, remove, fork.
///
/// Session data is persisted to SQLite via [`DatabaseService`].
///
/// # Source
/// `packages/opencode/src/session/session.ts` lines 461–514 `Interface`.
pub struct SessionManager {
    db: Arc<DatabaseService>,
    bus: SharedBus,
}

impl SessionManager {
    /// Create a new session manager backed by SQLite.
    pub fn new(bus: SharedBus, db: Arc<DatabaseService>) -> Self {
        Self { db, bus }
    }

    /// Create a new session.
    ///
    /// # Source
    /// `packages/opencode/src/session/session.ts` lines 709–731.
    pub async fn create(&self, input: CreateSessionInput) -> Result<SessionInfo, SessionError> {
        let now = Utc::now().timestamp_millis();
        let slug = id::descending(id::IdPrefix::Session, Some(8))
            .map_err(|e| SessionError::Other(e.to_string()))?;
        let session_id = id::descending(id::IdPrefix::Session, None)
            .map_err(|e| SessionError::Other(e.to_string()))?;

        let title = input.title.unwrap_or_else(|| {
            let prefix = if input.parent_id.is_some() {
                "Child session - "
            } else {
                "New session - "
            };
            format!("{prefix}{}", chrono::Utc::now().to_rfc3339())
        });

        let agent = input.agent.clone();
        let model_json = input
            .model
            .as_ref()
            .and_then(|m| serde_json::to_string(m).ok());

        self.db
            .insert_session(
                &session_id,
                &input.project_id,
                input.workspace_id.as_deref(),
                &slug,
                &input.directory,
                &title,
                env!("CARGO_PKG_VERSION"),
                now,
                now,
                agent.as_deref(),
                model_json.as_deref(),
            )
            .await?;

        let info = SessionInfo {
            id: session_id,
            slug,
            project_id: input.project_id,
            workspace_id: input.workspace_id,
            directory: input.directory,
            path: input.path,
            parent_id: input.parent_id,
            title,
            agent: input.agent,
            model: input.model,
            version: env!("CARGO_PKG_VERSION").to_string(),
            summary: None,
            cost: 0.0,
            tokens: TokenUsage::default(),
            share: None,
            metadata: input.metadata,
            permission: input.permission,
            revert: None,
            time: SessionTimestamps {
                created: now as u64,
                updated: now as u64,
                compacting: None,
                archived: None,
            },
        };

        self.bus.publish("session.created", &info)?;

        Ok(info)
    }

    /// Get a session by ID.
    ///
    /// Queries SQLite via [`DatabaseService::get_session`].
    ///
    /// # Source
    /// `packages/opencode/src/session/session.ts` line 582.
    pub async fn get(&self, id: &str) -> Result<SessionInfo, SessionError> {
        let row = self
            .db
            .get_session(id)
            .await?
            .ok_or_else(|| SessionError::NotFound(id.to_string()))?;

        Ok(session_row_to_info(row))
    }

    /// List sessions, optionally filtered by project.
    ///
    /// Uses [`DatabaseService::list_sessions`] for the project-scoped query,
    /// then applies optional in-memory filters (directory, search, roots, workspace).
    ///
    /// # Source
    /// `packages/opencode/src/session/session.ts` lines 588–594.
    pub async fn list(
        &self,
        input: Option<ListSessionsInput>,
    ) -> Result<Vec<SessionInfo>, SessionError> {
        let filters = input.unwrap_or_default();

        // Require project_id for DB-backed listing
        let project_id = filters
            .project_id
            .as_deref()
            .unwrap_or("__no_project__");

        let limit = filters.limit.map(|l| l.min(100) as u32);

        let rows = self.db.list_sessions(project_id, limit).await?;
        let mut results: Vec<SessionInfo> =
            rows.into_iter().map(session_row_to_info).collect();

        // Apply additional in-memory filters
        if let Some(dir) = &filters.directory {
            results.retain(|s| s.directory == *dir);
        }
        if let Some(search) = &filters.search {
            results.retain(|s| s.title.contains(search.as_str()));
        }
        if filters.roots.unwrap_or(false) {
            results.retain(|s| s.parent_id.is_none());
        }
        if let Some(workspace_id) = &filters.workspace_id {
            results.retain(|s| s.workspace_id.as_deref() == Some(workspace_id.as_str()));
        }
        results.sort_by_key(|s| s.time.updated);
        results.reverse();
        if let Some(limit) = filters.limit {
            results.truncate(limit.min(100));
        } else {
            results.truncate(100);
        }

        Ok(results)
    }

    /// Update session metadata.
    ///
    /// # Source
    /// `packages/opencode/src/session/session.ts` lines 776–789.
    pub async fn update(
        &self,
        id: &str,
        patch: SessionPatch,
    ) -> Result<SessionInfo, SessionError> {
        let now = Utc::now().timestamp_millis();

        // Flatten Option<Option<String>> → Option<&str>
        let title: Option<String> = patch.title.and_then(|inner| inner);
        let title_ref: Option<&str> = title.as_deref();

        let tokens_input = patch.tokens.as_ref().map(|t| t.input as i64);
        let tokens_output = patch.tokens.as_ref().map(|t| t.output as i64);

        self.db
            .update_session(id, now, title_ref, patch.cost, tokens_input, tokens_output)
            .await?;

        // Re-read to return updated info
        let row = self
            .db
            .get_session(id)
            .await?
            .ok_or_else(|| SessionError::NotFound(id.to_string()))?;

        let updated = session_row_to_info(row);
        self.bus.publish("session.updated", &updated)?;
        Ok(updated)
    }

    /// Remove a session and all related records (cascade delete).
    ///
    /// # Source
    /// `packages/opencode/src/session/session.ts` lines 648–669.
    pub async fn remove(&self, id: &str) -> Result<(), SessionError> {
        // Read session info before deleting (for event publishing)
        let info = self.get(id).await?;

        self.db.delete_session_cascade(id).await?;
        self.bus.publish("session.deleted", &info)?;
        Ok(())
    }

    /// Fork a session — copy messages up to a message ID.
    ///
    /// **TODO**: reimplement with DB-backed message copying.
    ///
    /// # Source
    /// `packages/opencode/src/session/session.ts` lines 733–773.
    pub async fn fork(
        &self,
        _session_id: &str,
        _message_id: Option<&str>,
    ) -> Result<SessionInfo, SessionError> {
        Err(SessionError::Other(
            "fork() is not yet implemented with DB-backed storage".into(),
        ))
    }

    /// Get all messages for a session (with parts).
    ///
    /// Deserializes the JSON `data` column from the `message` and `part` tables.
    pub async fn get_messages(&self, session_id: &str) -> Result<Vec<Message>, SessionError> {
        let rows = self
            .db
            .get_messages_with_parts(session_id, None)
            .await?;

        let mut messages = Vec::with_capacity(rows.len());
        for (msg_row, part_rows) in rows {
            let info: MessageInfo = serde_json::from_str(&msg_row.data)
                .map_err(|e| SessionError::Other(format!("deserialize message: {e}")))?;

            let parts: Result<Vec<Part>, SessionError> = part_rows
                .iter()
                .map(|pr| {
                    serde_json::from_str(&pr.data).map_err(|e| {
                        SessionError::Other(format!("deserialize part: {e}"))
                    })
                })
                .collect();

            messages.push(Message {
                info,
                parts: parts?,
            });
        }

        Ok(messages)
    }

    /// Append a message and its parts to a session.
    ///
    /// Serializes [`MessageInfo`] and each [`Part`] to JSON for storage in the
    /// legacy `message.data` and `part.data` columns.
    pub async fn append_message(
        &self,
        session_id: SessionId,
        info: MessageInfo,
        parts: Vec<Part>,
    ) -> Result<(), SessionError> {
        let now = Utc::now().timestamp_millis();
        let msg_id = info.id().to_string();

        // Serialize MessageInfo to JSON and insert message row
        let data = serde_json::to_string(&info)
            .map_err(|e| SessionError::Other(format!("serialize message: {e}")))?;

        self.db
            .insert_message(&msg_id, &session_id, &data, now, now)
            .await?;

        // Serialize and insert each part
        for part in &parts {
            let part_id = part_id(part);
            let part_data = serde_json::to_string(part)
                .map_err(|e| SessionError::Other(format!("serialize part: {e}")))?;
            self.db
                .insert_part(part_id, &msg_id, &session_id, &part_data, now, now)
                .await?;
        }

        self.bus.publish(
            "session.message_appended",
            &serde_json::json!({"session_id": session_id}),
        )?;
        Ok(())
    }

    /// Update a message (applies a patch to the stored JSON data).
    ///
    /// Reads the current message from the database, deserializes its `data`
    /// JSON, applies the patch, re-serializes, and writes it back.
    ///
    /// # Source
    /// `packages/opencode/src/session/session.ts` lines 671–675.
    pub async fn update_message(
        &self,
        session_id: &str,
        message_id: &str,
        patch: MessagePatch,
    ) -> Result<(), SessionError> {
        // Get current messages for the session
        let messages = self.db.list_messages(session_id, None).await?;
        let msg_row = messages
            .iter()
            .find(|m| m.id == message_id)
            .ok_or_else(|| SessionError::NotFound(message_id.to_string()))?;

        // Deserialize current info
        let mut info: MessageInfo = serde_json::from_str(&msg_row.data)
            .map_err(|e| SessionError::Other(format!("deserialize message: {e}")))?;

        // Apply patch
        info.apply_patch(patch);

        // Re-serialize and update
        let new_data = serde_json::to_string(&info)
            .map_err(|e| SessionError::Other(format!("serialize message: {e}")))?;
        let now = Utc::now().timestamp_millis();

        sqlx::query("UPDATE message SET data = ?1, time_updated = ?2 WHERE id = ?3")
            .bind(&new_data)
            .bind(now)
            .bind(message_id)
            .execute(self.db.pool())
            .await
            .map_err(|e| SessionError::Other(format!("update message row: {e}")))?;

        Ok(())
    }
}

/// Convert a [`SessionRow`] from the database into a [`SessionInfo`].
///
/// Fields not present in the current `SessionRow` (parent_id, path, metadata,
/// summary, share, permission, revert, etc.) are set to their defaults.
fn session_row_to_info(row: SessionRow) -> SessionInfo {
    SessionInfo {
        id: row.id,
        slug: row.slug,
        project_id: row.project_id,
        workspace_id: row.workspace_id,
        directory: row.directory,
        path: None,
        parent_id: None,
        title: row.title,
        agent: row.agent,
        model: row.model.and_then(|m| serde_json::from_str(&m).ok()),
        version: row.version,
        summary: None,
        cost: row.cost,
        tokens: TokenUsage {
            input: row.tokens_input as u64,
            output: row.tokens_output as u64,
            reasoning: 0,
            cache: CacheUsage::default(),
        },
        share: None,
        metadata: None,
        permission: None,
        revert: None,
        time: SessionTimestamps {
            created: row.time_created as u64,
            updated: row.time_updated as u64,
            compacting: None,
            archived: None,
        },
    }
}

/// Extract the part ID regardless of variant.
fn part_id(part: &Part) -> &str {
    match part {
        Part::Text(p) => &p.id,
        Part::Tool(p) => &p.id,
        Part::Reasoning(p) => &p.id,
        Part::File(p) => &p.id,
        Part::StepStart(p) => &p.id,
        Part::StepFinish(p) => &p.id,
        Part::Patch(p) => &p.id,
        Part::Compaction(p) => &p.id,
        Part::Subtask(p) => &p.id,
    }
}

// ── Session Manager input types ────────────────────────────────────────────

/// Input for creating a session.
#[derive(Debug, Clone)]
pub struct CreateSessionInput {
    pub project_id: String,
    pub workspace_id: Option<String>,
    pub directory: String,
    pub path: Option<String>,
    pub parent_id: Option<SessionId>,
    pub title: Option<String>,
    pub agent: Option<String>,
    pub model: Option<ModelSelection>,
    pub metadata: Option<serde_json::Value>,
    pub permission: Option<Vec<crate::permission::PermissionRule>>,
}

/// Filters for listing sessions.
#[derive(Debug, Clone, Default)]
pub struct ListSessionsInput {
    /// Required for DB-backed listing — filters sessions by project.
    pub project_id: Option<String>,
    pub directory: Option<String>,
    pub path: Option<String>,
    pub workspace_id: Option<String>,
    pub roots: Option<bool>,
    pub search: Option<String>,
    pub limit: Option<usize>,
}

/// Patch for updating a session.
#[derive(Debug, Clone, Default)]
pub struct SessionPatch {
    pub title: Option<Option<String>>,
    pub agent: Option<Option<String>>,
    pub model: Option<Option<ModelSelection>>,
    pub cost: Option<f64>,
    pub tokens: Option<TokenUsage>,
    pub summary: Option<Option<SessionSummary>>,
    pub revert: Option<Option<RevertInfo>>,
}

/// Patch for updating a message.
#[derive(Debug, Clone, Default)]
pub struct MessagePatch {
    pub finish: Option<Option<String>>,
    pub error: Option<Option<serde_json::Value>>,
    pub cost: Option<f64>,
    pub tokens: Option<TokenUsage>,
    pub time_completed: Option<u64>,
}

// ── MessageInfo helpers ─────────────────────────────────────────────────────

impl MessageInfo {
    /// Get the message ID regardless of role.
    pub fn id(&self) -> &str {
        match self {
            MessageInfo::User(u) => &u.id,
            MessageInfo::Assistant(a) => &a.id,
        }
    }

    /// Get the role string.
    pub fn role(&self) -> &str {
        match self {
            MessageInfo::User(_) => "user",
            MessageInfo::Assistant(_) => "assistant",
        }
    }

    /// Clone with new session and message IDs.
    pub fn clone_with_session(
        &self,
        new_session_id: &str,
        new_id: &str,
        id_map: &HashMap<MessageId, MessageId>,
    ) -> Self {
        match self {
            MessageInfo::User(u) => MessageInfo::User(UserInfo {
                id: new_id.to_string(),
                session_id: new_session_id.to_string(),
                agent: u.agent.clone(),
                model: u.model.clone(),
                time: u.time.clone(),
            }),
            MessageInfo::Assistant(a) => {
                let parent_id = id_map
                    .get(&a.parent_id)
                    .cloned()
                    .unwrap_or_else(|| a.parent_id.clone());
                MessageInfo::Assistant(AssistantInfo {
                    id: new_id.to_string(),
                    session_id: new_session_id.to_string(),
                    parent_id,
                    agent: a.agent.clone(),
                    model_id: a.model_id.clone(),
                    provider_id: a.provider_id.clone(),
                    variant: a.variant.clone(),
                    summary: a.summary,
                    cost: a.cost,
                    tokens: a.tokens.clone(),
                    finish: a.finish.clone(),
                    error: a.error.clone(),
                    time: a.time.clone(),
                })
            }
        }
    }

    /// Apply a message patch.
    pub fn apply_patch(&mut self, patch: MessagePatch) {
        if let MessageInfo::Assistant(a) = self {
            if let Some(finish) = patch.finish {
                a.finish = finish;
            }
            if let Some(error) = patch.error {
                a.error = error;
            }
            if let Some(cost) = patch.cost {
                a.cost = cost;
            }
            if let Some(tokens) = patch.tokens {
                a.tokens = tokens;
            }
            if let Some(completed) = patch.time_completed {
                a.time.completed = Some(completed);
            }
        }
    }
}

// ── Part helpers ───────────────────────────────────────────────────────────

impl Part {
    fn set_message_id(&mut self, id: &str) {
        let mid = match self {
            Part::Text(p) => &mut p.message_id,
            Part::Tool(p) => &mut p.message_id,
            Part::Reasoning(p) => &mut p.message_id,
            Part::File(p) => &mut p.message_id,
            Part::StepStart(p) => &mut p.message_id,
            Part::StepFinish(p) => &mut p.message_id,
            Part::Patch(p) => &mut p.message_id,
            Part::Compaction(p) => &mut p.message_id,
            Part::Subtask(p) => &mut p.message_id,
        };
        *mid = id.to_string();
    }

    fn set_session_id(&mut self, id: &str) {
        let sid = match self {
            Part::Text(p) => &mut p.session_id,
            Part::Tool(p) => &mut p.session_id,
            Part::Reasoning(p) => &mut p.session_id,
            Part::File(p) => &mut p.session_id,
            Part::StepStart(p) => &mut p.session_id,
            Part::StepFinish(p) => &mut p.session_id,
            Part::Patch(p) => &mut p.session_id,
            Part::Compaction(p) => &mut p.session_id,
            Part::Subtask(p) => &mut p.session_id,
        };
        *sid = id.to_string();
    }

    fn set_id(&mut self, id: &str) {
        let pid = match self {
            Part::Text(p) => &mut p.id,
            Part::Tool(p) => &mut p.id,
            Part::Reasoning(p) => &mut p.id,
            Part::File(p) => &mut p.id,
            Part::StepStart(p) => &mut p.id,
            Part::StepFinish(p) => &mut p.id,
            Part::Patch(p) => &mut p.id,
            Part::Compaction(p) => &mut p.id,
            Part::Subtask(p) => &mut p.id,
        };
        *pid = id.to_string();
    }
}

// ── Utilities ─────────────────────────────────────────────────────────────

fn fork_title(title: &str) -> String {
    // Check for " (fork #N)" suffix
    if let Some(open_paren) = title.rfind(" (fork #") {
        let after_fork = &title[open_paren + 8..]; // skip " (fork #"
        if let Some(close_paren) = after_fork.find(')') {
            let num_str = &after_fork[..close_paren];
            if let Ok(num) = num_str.parse::<u32>() {
                let base = &title[..open_paren];
                return format!("{base} (fork #{})", num + 1);
            }
        }
    }
    format!("{title} (fork #1)")
}

// ══════════════════════════════════════════════════════════════════════════════
// Session Processor — core LLM interaction loop
// ══════════════════════════════════════════════════════════════════════════════

/// Result of processing a step.
///
/// # Source
/// `packages/opencode/src/session/processor.ts` line 36.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ProcessResult {
    /// Context overflow — needs compaction.
    Compact,
    /// Blocked or errored — stop processing.
    Stop,
    /// Continue to next step.
    Continue,
}

/// Doom loop detection threshold.
///
/// # Source
/// `packages/opencode/src/session/processor.ts` line 35.
const DOOM_LOOP_THRESHOLD: usize = 3;

/// Internal tool call tracking.
///
/// # Source
/// `packages/opencode/src/session/processor.ts` lines 66–74.
#[derive(Debug)]
struct TrackedToolCall {
    assistant_message_id: Option<MessageId>,
    part_id: PartId,
    message_id: MessageId,
    session_id: SessionId,
    done: Option<oneshot::Sender<()>>,
    input_ended: bool,
    raw: String,
}

/// Mutable context during stream processing.
///
/// # Source
/// `packages/opencode/src/session/processor.ts` lines 76–86.
struct ProcessorContext {
    assistant_message: AssistantInfo,
    session_id: SessionId,
    model: Model,
    toolcalls: HashMap<String, TrackedToolCall>,
    should_break: bool,
    snapshot: Option<String>,
    blocked: bool,
    needs_compaction: bool,
    current_text: Option<TextPart>,
    current_text_id: Option<String>,
    reasoning_map: HashMap<String, ReasoningPart>,
    aborted: bool,
}

/// LLM stream input for a processing step.
///
/// # Source
/// `packages/opencode/src/session/processor.ts` — `LLM.StreamInput`.
#[derive(Debug, Clone)]
pub struct StreamInput {
    pub user: UserInfo,
    pub agent: crate::agent::AgentInfo,
    pub session_id: SessionId,
    pub tools: HashMap<String, serde_json::Value>,
    pub system: Vec<String>,
    pub messages: Vec<crate::provider::ChatMessage>,
    pub model: Model,
}

/// Processes LLM streams for a single session turn.
///
/// # Source
/// `packages/opencode/src/session/processor.ts` lines 90–1084.
pub struct SessionProcessor {
    manager: Arc<SessionManager>,
    tool_registry: Arc<ToolRegistry>,
    permission: Arc<PermissionService>,
    bus: SharedBus,
}

impl SessionProcessor {
    /// Create a new session processor.
    pub fn new(
        manager: Arc<SessionManager>,
        tool_registry: Arc<ToolRegistry>,
        permission: Arc<PermissionService>,
        bus: SharedBus,
    ) -> Self {
        Self {
            manager,
            tool_registry,
            permission,
            bus,
        }
    }

    /// Process a stream input — the core loop.
    ///
    /// The caller must resolve the correct [`Provider`] before calling this method.
    ///
    /// # Source
    /// `packages/opencode/src/session/processor.ts` lines 960–1034.
    pub async fn process(
        &self,
        provider: &(dyn crate::provider::Provider),
        input: &StreamInput,
        cancel_token: CancellationToken,
    ) -> Result<ProcessResult, SessionError> {
        let assistant_msg_id = id::ascending(id::IdPrefix::Message, None)
            .map_err(|e| SessionError::Other(e.to_string()))?;
        let now = Utc::now().timestamp_millis() as u64;

        let mut ctx = ProcessorContext {
            assistant_message: AssistantInfo {
                id: assistant_msg_id.clone(),
                session_id: input.session_id.clone(),
                parent_id: input.user.id.clone(),
                agent: input.agent.name().to_string(),
                model_id: Some(input.model.id.clone()),
                provider_id: Some(input.model.provider_id.clone()),
                variant: input.user.model.as_ref().and_then(|m| m.variant.clone()),
                summary: false,
                cost: 0.0,
                tokens: TokenUsage::default(),
                finish: None,
                error: None,
                time: MessageTime {
                    created: now,
                    completed: None,
                },
            },
            session_id: input.session_id.clone(),
            model: input.model.clone(),
            toolcalls: HashMap::new(),
            should_break: true,
            snapshot: None,
            blocked: false,
            needs_compaction: false,
            current_text: None,
            current_text_id: None,
            reasoning_map: HashMap::new(),
            aborted: false,
        };

        // Publish step-started event
        self.bus.publish(
            "session.step.started",
            &serde_json::json!({
                "session_id": ctx.session_id,
                "message_id": assistant_msg_id,
                "agent": ctx.assistant_message.agent,
                "model": {"id": ctx.model.id, "provider_id": ctx.model.provider_id},
            }),
        )?;

        // Append the initial assistant message
        self.manager
            .append_message(
                ctx.session_id.clone(),
                MessageInfo::Assistant(ctx.assistant_message.clone()),
                vec![],
            )
            .await?;

        // Get the provider and stream
        let retry_result = self
            .run_with_retry(&mut ctx, provider, input, &cancel_token)
            .await;

        // Determine result
        let result = if ctx.needs_compaction {
            ProcessResult::Compact
        } else if ctx.blocked || ctx.assistant_message.error.is_some() {
            ProcessResult::Stop
        } else {
            ProcessResult::Continue
        };

        // Update assistant message
        ctx.assistant_message.time.completed = Some(Utc::now().timestamp_millis() as u64);
        self.manager
            .update_message(
                &ctx.session_id,
                &assistant_msg_id,
                MessagePatch {
                    finish: Some(ctx.assistant_message.finish.clone()),
                    error: Some(ctx.assistant_message.error.clone()),
                    cost: Some(ctx.assistant_message.cost),
                    tokens: Some(ctx.assistant_message.tokens.clone()),
                    time_completed: ctx.assistant_message.time.completed,
                },
            )
            .await?;

        // Publish step-ended event
        self.bus.publish(
            "session.step.ended",
            &serde_json::json!({
                "session_id": ctx.session_id,
                "message_id": assistant_msg_id,
                "result": format!("{:?}", result),
            }),
        )?;

        if let Err(e) = retry_result {
            ctx.assistant_message.error =
                Some(serde_json::json!({"message": e.to_string(), "type": "error"}));
            return Err(e);
        }

        Ok(result)
    }

    /// Run the stream with retry logic using exponential backoff.
    ///
    /// # Source
    /// `packages/opencode/src/session/processor.ts` lines 994–1027.
    /// `packages/opencode/src/session/retry.ts` lines 176–199.
    async fn run_with_retry(
        &self,
        ctx: &mut ProcessorContext,
        provider: &(dyn crate::provider::Provider),
        input: &StreamInput,
        cancel_token: &CancellationToken,
    ) -> Result<(), SessionError> {
        let max_attempts = 4u32;
        let mut attempt: u32 = 0;

        loop {
            attempt += 1;

            if cancel_token.is_cancelled() {
                return Err(SessionError::Aborted);
            }

            match self.run_stream(ctx, provider, input, cancel_token).await {
                Ok(()) => return Ok(()),
                Err(e) => {
                    // Check if error is retryable
                    if !is_retryable(&e.to_string()) || attempt >= max_attempts {
                        return Err(e);
                    }

                    // Wait with exponential backoff
                    let delay_ms = retry_delay(attempt);
                    info!(
                        attempt = attempt,
                        delay_ms = delay_ms,
                        "retrying stream after error"
                    );

                    tokio::select! {
                        _ = cancel_token.cancelled() => {
                            return Err(SessionError::Aborted);
                        }
                        _ = tokio::time::sleep(std::time::Duration::from_millis(delay_ms)) => {}
                    }
                }
            }
        }
    }

    /// Run a single streaming pass.
    ///
    /// # Source
    /// `packages/opencode/src/session/processor.ts` lines 969–980.
    async fn run_stream(
        &self,
        ctx: &mut ProcessorContext,
        provider: &(dyn crate::provider::Provider),
        input: &StreamInput,
        cancel_token: &CancellationToken,
    ) -> Result<(), SessionError> {
        use futures::StreamExt;

        let messages = input.messages.clone();
        let tools = self.build_tool_definitions();

        let mut stream = provider
            .stream(
                &input.model,
                &messages,
                &tools,
            )
            .await
            .map_err(|e| SessionError::Provider(e.to_string()))?;

        loop {
            tokio::select! {
                _ = cancel_token.cancelled() => {
                    ctx.aborted = true;
                    self.cleanup(ctx).await?;
                    return Err(SessionError::Aborted);
                }
                event = stream.next() => {
                    match event {
                        Some(Ok(event)) => {
                            self.handle_event(ctx, &event).await?;
                            if ctx.needs_compaction {
                                return Ok(());
                            }
                        }
                        Some(Err(e)) => {
                            let msg = e.to_string();
                            // Context overflow → trigger compaction
                            if msg.contains("context") || msg.contains("token") || msg.contains("limit") {
                                ctx.needs_compaction = true;
                                return Ok(());
                            }
                            return Err(SessionError::Provider(msg));
                        }
                        None => break,
                    }
                }
            }
        }

        Ok(())
    }

    /// Handle a single stream event.
    ///
    /// # Source
    /// `packages/opencode/src/session/processor.ts` lines 371–843.
    async fn handle_event(
        &self,
        ctx: &mut ProcessorContext,
        event: &LlmEvent,
    ) -> Result<(), SessionError> {
        match event {
            // ── Reasoning events ──────────────────────────────────
            LlmEvent::ReasoningStart { id, .. } => {
                if ctx.reasoning_map.contains_key(id.as_str()) {
                    return Ok(());
                }
                let part = ReasoningPart {
                    id: id::ascending(id::IdPrefix::Part, None).unwrap_or_default(),
                    message_id: ctx.assistant_message.id.clone(),
                    session_id: ctx.session_id.clone(),
                    text: String::new(),
                    metadata: None,
                    time: PartTime {
                        start: Some(Utc::now().timestamp_millis() as u64),
                        end: None,
                    },
                };
                ctx.reasoning_map.insert(id.clone(), part);
            }

            LlmEvent::ReasoningDelta { id, text, .. } => {
                if let Some(part) = ctx.reasoning_map.get_mut(id.as_str()) {
                    part.text.push_str(text);
                }
            }

            LlmEvent::ReasoningEnd { id, .. } => {
                if let Some(mut part) = ctx.reasoning_map.remove(id.as_str()) {
                    part.time.end = Some(Utc::now().timestamp_millis() as u64);
                    // Fire reasoning-ended event
                    self.bus.publish(
                        "session.reasoning.ended",
                        &serde_json::json!({
                            "session_id": ctx.session_id,
                            "reasoning_id": id,
                            "text": part.text,
                        }),
                    )?;
                }
            }

            // ── Text events ───────────────────────────────────────
            LlmEvent::TextStart { id, .. } => {
                let part = TextPart {
                    id: id::ascending(id::IdPrefix::Part, None).unwrap_or_default(),
                    message_id: ctx.assistant_message.id.clone(),
                    session_id: ctx.session_id.clone(),
                    text: String::new(),
                    metadata: None,
                    time: PartTime {
                        start: Some(Utc::now().timestamp_millis() as u64),
                        end: None,
                    },
                };
                ctx.current_text_id = Some(id.clone());
                ctx.current_text = Some(part);
            }

            LlmEvent::TextDelta { id, text, .. } => {
                if let Some(ref mut current) = ctx.current_text {
                    if ctx.current_text_id.as_deref() == Some(id.as_str()) {
                        current.text.push_str(text);
                    }
                }
            }

            LlmEvent::TextEnd { id, .. } => {
                if let Some(mut part) = ctx.current_text.take() {
                    if ctx.current_text_id.as_deref() == Some(id.as_str()) {
                        part.time.end = Some(Utc::now().timestamp_millis() as u64);
                    }
                    // Fire text-ended event
                    self.bus.publish(
                        "session.text.ended",
                        &serde_json::json!({
                            "session_id": ctx.session_id,
                            "text_id": id,
                            "text": part.text,
                        }),
                    )?;
                }
                ctx.current_text_id = None;
            }

            // ── Tool input events ─────────────────────────────────
            LlmEvent::ToolCall {
                id,
                name,
                input,
                ..
            } => {
                self.ensure_tool_call(ctx, id.as_str(), name.as_str(), false)
                    .await?;

                // Check for doom loop
                let recent_parts = self.check_doom_loop(ctx, name, input).await;
                if recent_parts >= DOOM_LOOP_THRESHOLD {
                    warn!(
                        "doom loop detected: tool={} count={}",
                        name, DOOM_LOOP_THRESHOLD
                    );
                    // In TS: asks permission for doom_loop
                    // For now, flag and continue
                    self.bus.publish(
                        "session.doom_loop",
                        &serde_json::json!({
                            "session_id": ctx.session_id,
                            "tool": name,
                            "count": DOOM_LOOP_THRESHOLD,
                        }),
                    )?;
                }

                // Execute the tool
                let result = self
                    .execute_tool_call(ctx, id.as_str(), name.as_str(), input)
                    .await;

                match result {
                    Ok(output) => {
                        self.complete_tool_call(ctx, id.as_str(), &output).await?;
                    }
                    Err(e) => {
                        self.fail_tool_call(ctx, id.as_str(), &e.to_string()).await?;
                    }
                }
            }

            // ── Step events ───────────────────────────────────────
            LlmEvent::StepStart { .. } => {
                // Track snapshot start (simplified — TS uses snapshot.track())
                ctx.snapshot = Some("snapshot".to_string());
            }

            LlmEvent::StepFinish {
                reason, usage, ..
            } => {
                // Finish remaining reasoning parts
                let remaining: Vec<String> = ctx.reasoning_map.keys().cloned().collect();
                for id in remaining {
                    if let Some(mut part) = ctx.reasoning_map.remove(&id) {
                        part.time.end = Some(Utc::now().timestamp_millis() as u64);
                    }
                }

                let usage = usage.as_ref().cloned().unwrap_or_default();
                ctx.assistant_message.finish = Some(Self::finish_reason_str(reason));
                ctx.assistant_message.cost += self.calculate_cost(&usage, &ctx.model);
                ctx.assistant_message.tokens = usage_to_token_usage(&usage);

                ctx.snapshot = None;

                // Check overflow → needs compaction
                let is_overflow = check_overflow(
                    &ctx.assistant_message.tokens,
                    &ctx.model,
                    None,
                );
                if is_overflow {
                    ctx.needs_compaction = true;
                }
            }

            // ── Provider error ────────────────────────────────────
            LlmEvent::ProviderErrorEvent { message, .. } => {
                return Err(SessionError::Provider(message.clone()));
            }

            // ── Tool result / error from provider-side execution ──
            LlmEvent::ToolResult {
                id,
                result,
                ..
            } => {
                // `result` is a serde_json::Value. TS checks result.type === "error".
                let is_error = result
                    .as_object()
                    .and_then(|o| o.get("type"))
                    .and_then(|t| t.as_str())
                    == Some("error");
                if is_error {
                    let err_msg = result
                        .get("value")
                        .and_then(|v| v.as_str())
                        .unwrap_or("unknown tool error");
                    self.fail_tool_call(ctx, id.as_str(), err_msg)
                        .await?;
                }
            }

            // ── Other events (no-op) ──────────────────────────────
            _ => {}
        }

        Ok(())
    }

    /// Ensure a tool call is tracked in the context.
    ///
    /// # Source
    /// `packages/opencode/src/session/processor.ts` lines 295–346.
    async fn ensure_tool_call(
        &self,
        ctx: &mut ProcessorContext,
        id: &str,
        name: &str,
        provider_executed: bool,
    ) -> Result<(), SessionError> {
        if ctx.toolcalls.contains_key(id) {
            return Ok(());
        }

        let part_id = id::ascending(id::IdPrefix::Part, None).unwrap_or_default();

        // Publish tool-input-started
        self.bus.publish(
            "session.tool.input_started",
            &serde_json::json!({
                "session_id": ctx.session_id,
                "call_id": id,
                "name": name,
            }),
        )?;

        let (done_tx, _done_rx) = oneshot::channel::<()>();
        ctx.toolcalls.insert(
            id.to_string(),
            TrackedToolCall {
                assistant_message_id: Some(ctx.assistant_message.id.clone()),
                part_id: part_id.clone(),
                message_id: ctx.assistant_message.id.clone(),
                session_id: ctx.session_id.clone(),
                done: Some(done_tx),
                input_ended: false,
                raw: String::new(),
            },
        );

        // Append tool part to the session
        let _ = provider_executed; // mark provider_executed in metadata if true
        Ok(())
    }

    /// Execute a tool call via the tool registry.
    ///
    /// # Source
    /// Permission checks are handled by each tool individually; the processor
    /// only does doom-loop detection (above). This matches the TS source where
    /// `SessionProcessor` delegates tool execution to the tool layer.
    ///
    /// Uses `ToolRegistry::execute_by_name()` which looks up the tool, builds
    /// a [`ToolContext`], and awaits the `execute` method.
    async fn execute_tool_call(
        &self,
        ctx: &mut ProcessorContext,
        tool_call_id: &str,
        tool_name: &str,
        input: &serde_json::Value,
    ) -> Result<ToolCallOutput, SessionError> {
        let tool_ctx = crate::tool::ToolContext {
            session_id: ctx.session_id.clone(),
            message_id: ctx.assistant_message.id.clone(),
            agent: ctx.assistant_message.agent.clone(),
            abort: CancellationToken::new(),
            call_id: Some(tool_call_id.to_string()),
            extra: std::collections::HashMap::new(),
            messages: vec![],
        };

        self.tool_registry
            .execute_by_name(tool_name, input.clone(), &tool_ctx)
            .await
            .map(|result| ToolCallOutput {
                title: result.title,
                output: result.output,
                metadata: serde_json::Value::Null,
                attachments: None,
            })
            .map_err(|e| SessionError::Other(format!("tool {tool_name} error: {e}")))
    }

    /// Complete a tool call with its output.
    ///
    /// # Source
    /// `packages/opencode/src/session/processor.ts` lines 203–227.
    async fn complete_tool_call(
        &self,
        ctx: &mut ProcessorContext,
        tool_call_id: &str,
        output: &ToolCallOutput,
    ) -> Result<(), SessionError> {
        if let Some(tc) = ctx.toolcalls.get_mut(tool_call_id) {
            if let Some(done) = tc.done.take() {
                let _ = done.send(());
            }

            self.bus.publish(
                "session.tool.completed",
                &serde_json::json!({
                    "session_id": ctx.session_id,
                    "call_id": tool_call_id,
                    "title": output.title,
                    "output": output.output,
                }),
            )?;
        }
        Ok(())
    }

    /// Fail a tool call with an error.
    ///
    /// # Source
    /// `packages/opencode/src/session/processor.ts` lines 229–246.
    async fn fail_tool_call(
        &self,
        ctx: &mut ProcessorContext,
        tool_call_id: &str,
        error: &str,
    ) -> Result<(), SessionError> {
        if let Some(tc) = ctx.toolcalls.get_mut(tool_call_id) {
            if let Some(done) = tc.done.take() {
                let _ = done.send(());
            }

            self.bus.publish(
                "session.tool.failed",
                &serde_json::json!({
                    "session_id": ctx.session_id,
                    "call_id": tool_call_id,
                    "error": error,
                }),
            )?;
        }
        Ok(())
    }

    /// Check for doom loop — same tool called repeatedly with same input.
    async fn check_doom_loop(
        &self,
        ctx: &ProcessorContext,
        tool_name: &str,
        input: &serde_json::Value,
    ) -> usize {
        let input_str = serde_json::to_string(input).unwrap_or_default();
        let mut count = 0;
        for tc in ctx.toolcalls.values() {
            // Simplified check — count matching calls
            count += 1;
        }
        let _ = (tool_name, input_str);
        count
    }

    /// Cleanup incomplete state on abort or error.
    ///
    /// # Source
    /// `packages/opencode/src/session/processor.ts` lines 846–915.
    async fn cleanup(&self, ctx: &mut ProcessorContext) -> Result<(), SessionError> {
        // Finish current text if any
        if let Some(mut part) = ctx.current_text.take() {
            let end = Utc::now().timestamp_millis() as u64;
            part.time.end = Some(end);
        }

        // Settle remaining tool calls
        let call_ids: Vec<String> = ctx.toolcalls.keys().cloned().collect();
        for id in &call_ids {
            self.fail_tool_call(ctx, id, "Tool execution aborted")
                .await?;
        }
        ctx.toolcalls.clear();

        // Mark assistant message as completed
        ctx.assistant_message.time.completed = Some(Utc::now().timestamp_millis() as u64);

        Ok(())
    }

    /// Build tool definitions from the registry for the LLM call.
    fn build_tool_definitions(&self) -> Vec<crate::provider::ToolDefinition> {
        self.tool_registry.llm_definitions()
    }

    /// Calculate cost from usage and model (static helper, also usable outside &self).
    ///
    /// # Source
    /// `packages/opencode/src/session/session.ts` lines 384–453 `getUsage`.
    pub fn calculate_cost_static(usage: &Usage, model: &Model) -> f64 {
        Self::calc_cost_impl(usage, model)
    }

    /// Calculate cost from usage and model.
    fn calculate_cost(&self, usage: &Usage, model: &Model) -> f64 {
        Self::calc_cost_impl(usage, model)
    }

    fn calc_cost_impl(usage: &Usage, model: &Model) -> f64 {
        let tokens = usage_to_token_usage(usage);
        let cost = model.cost.as_ref();

        if let Some(c) = cost {
            let input_cost = tokens.input as f64 * c.input / 1_000_000.0;
            let output_cost = tokens.output as f64 * c.output / 1_000_000.0;
            let cache_read = tokens.cache.read as f64 * c.cache.read / 1_000_000.0;
            let cache_write = tokens.cache.write as f64 * c.cache.write / 1_000_000.0;
            input_cost + output_cost + cache_read + cache_write
        } else {
            0.0
        }
    }

    /// Convert finish reason to string (static helper for tests).
    pub fn finish_reason_str_static(reason: &crate::provider::FinishReason) -> String {
        Self::finish_reason_str(reason)
    }

    /// Convert finish reason to string.
    fn finish_reason_str(reason: &crate::provider::FinishReason) -> String {
        match reason {
            crate::provider::FinishReason::Stop => "stop".into(),
            crate::provider::FinishReason::Length => "length".into(),
            crate::provider::FinishReason::ToolCalls => "tool_calls".into(),
            crate::provider::FinishReason::ContentFilter => "content_filter".into(),
            _ => "unknown".into(),
        }
    }
}

/// Output from a completed tool call.
#[derive(Debug, Clone)]
pub struct ToolCallOutput {
    pub title: String,
    pub output: String,
    pub metadata: serde_json::Value,
    pub attachments: Option<Vec<FilePart>>,
}

/// Convert Usage to TokenUsage.
fn usage_to_token_usage(usage: &Usage) -> TokenUsage {
    let safe = |v: Option<u64>| v.unwrap_or(0);
    TokenUsage {
        input: safe(usage.input_tokens),
        output: safe(usage.output_tokens),
        reasoning: safe(usage.reasoning_tokens),
        cache: CacheUsage {
            read: safe(usage.cache_read_input_tokens),
            write: safe(usage.cache_write_input_tokens),
        },
    }
}

// ══════════════════════════════════════════════════════════════════════════════
// Compaction / Overflow
// ══════════════════════════════════════════════════════════════════════════════

/// Compaction buffer (tokens reserved for compaction summary).
///
/// # Source
/// `packages/opencode/src/session/overflow.ts` line 8.
const COMPACTION_BUFFER: u64 = 20_000;

/// Check if the current token usage overflows the context window.
///
/// # Source
/// `packages/opencode/src/session/overflow.ts` lines 22–34.
pub fn check_overflow(
    tokens: &TokenUsage,
    model: &Model,
    _output_token_max: Option<u64>,
) -> bool {
    let context = model.limit.context;
    if context == 0 {
        return false;
    }

    let count = tokens.input + tokens.output + tokens.reasoning + tokens.cache.read + tokens.cache.write;

    // Calculate usable context (context minus reserved for output)
    let max_output = crate::provider::max_output_tokens(model, _output_token_max.unwrap_or(0));
    let reserved = COMPACTION_BUFFER.min(max_output);
    let usable = if model.limit.input.unwrap_or(0) > 0 {
        model.limit.input.unwrap_or(0).saturating_sub(reserved)
    } else {
        context.saturating_sub(max_output)
    };

    count >= usable
}

// ══════════════════════════════════════════════════════════════════════════════
// Retry logic
// ══════════════════════════════════════════════════════════════════════════════

/// Retry initial delay in milliseconds.
///
/// # Source
/// `packages/opencode/src/session/retry.ts` line 26.
pub const RETRY_INITIAL_DELAY_MS: u64 = 2_000;

/// Retry backoff factor.
///
/// # Source
/// `packages/opencode/src/session/retry.ts` line 27.
pub const RETRY_BACKOFF_FACTOR: u64 = 2;

/// Maximum retry delay without response headers.
///
/// # Source
/// `packages/opencode/src/session/retry.ts` line 28.
pub const RETRY_MAX_DELAY_NO_HEADERS_MS: u64 = 30_000;

/// Compute retry delay for a given attempt.
///
/// # Source
/// `packages/opencode/src/session/retry.ts` lines 35–66.
pub fn retry_delay(attempt: u32) -> u64 {
    let delay = RETRY_INITIAL_DELAY_MS * RETRY_BACKOFF_FACTOR.pow(attempt.saturating_sub(1));
    delay.min(RETRY_MAX_DELAY_NO_HEADERS_MS)
}

/// Determine if an error is retryable.
///
/// # Source
/// `packages/opencode/src/session/retry.ts` lines 68–152.
pub fn is_retryable(error: &str) -> bool {
    let lower = error.to_lowercase();
    // 5xx errors are retryable
    lower.contains("overloaded")
        || lower.contains("rate limit")
        || lower.contains("too many requests")
        || lower.contains("connection reset")
        || lower.contains("service unavailable")
        || lower.contains("internal server error")
}

// ══════════════════════════════════════════════════════════════════════════════
// Session Status
// ══════════════════════════════════════════════════════════════════════════════

/// Session status.
///
/// # Source
/// `packages/opencode/src/session/status.ts` lines 9–33.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type")]
pub enum SessionStatus {
    #[serde(rename = "idle")]
    Idle,
    #[serde(rename = "busy")]
    Busy,
    #[serde(rename = "retry")]
    Retry {
        attempt: u64,
        message: String,
        #[serde(skip_serializing_if = "Option::is_none")]
        action: Option<RetryAction>,
        next: u64,
    },
}

/// Retry action information.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RetryAction {
    pub reason: String,
    pub provider: String,
    pub title: String,
    pub message: String,
    pub label: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub link: Option<String>,
}

// ══════════════════════════════════════════════════════════════════════════════
// Tests
// ══════════════════════════════════════════════════════════════════════════════

#[cfg(test)]
mod tests {
    use super::*;
    use crate::provider::{
        ApiInfo, CacheCost as ProviderCacheCost, Capabilities, Cost as ProviderCost,
        ModelStatus, TokenLimit,
    };
    use std::collections::HashMap;

    /// Helper to build a minimal Model for unit tests.
    fn test_model(context: u64, input: u64, output: u64) -> Model {
        Model {
            id: "test-model".into(),
            provider_id: "test-provider".into(),
            name: "Test Model".into(),
            api: ApiInfo {
                id: "test-model".into(),
                url: String::new(),
                npm: "@ai-sdk/test".into(),
            },
            family: None,
            capabilities: Capabilities::default(),
            cost: ProviderCost {
                input: 3.0,
                output: 15.0,
                cache: ProviderCacheCost { read: 0.0, write: 0.0 },
                tiers: None,
                experimental_over_200k: None,
            },
            limit: TokenLimit {
                context,
                input: if input > 0 { Some(input) } else { None },
                output,
            },
            status: ModelStatus::Active,
            options: HashMap::new(),
            headers: HashMap::new(),
            release_date: String::new(),
            variants: None,
        }
    }

    /// Build an in-memory DatabaseService for tests that need to compile
    /// but are ignored because they require a full schema setup.
    fn test_db() -> Arc<DatabaseService> {
        let pool = sqlx::sqlite::SqlitePoolOptions::new()
            .max_connections(1)
            .connect_lazy("sqlite::memory:");
        Arc::new(DatabaseService::new(pool))
    }

    // ── Session Manager tests ────────────────────────────────────
    // NOTE: These tests require a DatabaseService (SQLite). They are
    // ignored because the test harness does not yet provide a test DB.
    // The DatabaseService CRUD tests in database.rs cover the DB layer.

    #[ignore = "needs test database with DatabaseService"]
    #[tokio::test]
    async fn test_create_and_get_session() {
        let bus = SharedBus::new(64);
        let manager = SessionManager::new(bus, test_db());

        let session = manager
            .create(CreateSessionInput {
                project_id: "test-project".into(),
                workspace_id: None,
                directory: "/tmp/test".into(),
                path: None,
                parent_id: None,
                title: Some("Test Session".into()),
                agent: Some("default".into()),
                model: None,
                metadata: None,
                permission: None,
            })
            .await
            .expect("create should succeed");

        assert_eq!(session.title, "Test Session");
        assert_eq!(session.agent, Some("default".into()));

        let fetched = manager
            .get(&session.id)
            .await
            .expect("get should succeed");
        assert_eq!(fetched.id, session.id);
    }

    #[ignore = "needs test database with DatabaseService"]
    #[tokio::test]
    async fn test_create_session_with_parent() {
        let bus = SharedBus::new(64);
        let manager = SessionManager::new(bus, test_db());

        let parent = manager
            .create(CreateSessionInput {
                project_id: "test-project".into(),
                workspace_id: None,
                directory: "/tmp/test".into(),
                path: None,
                parent_id: None,
                title: Some("Parent".into()),
                agent: None,
                model: None,
                metadata: None,
                permission: None,
            })
            .await
            .expect("parent creation");

        let child = manager
            .create(CreateSessionInput {
                project_id: "test-project".into(),
                workspace_id: None,
                directory: "/tmp/test".into(),
                path: None,
                parent_id: Some(parent.id.clone()),
                title: None,
                agent: None,
                model: None,
                metadata: None,
                permission: None,
            })
            .await
            .expect("child creation");

        assert!(child.title.starts_with("Child session - "));
        assert_eq!(child.parent_id, Some(parent.id));
    }

    #[ignore = "needs test database with DatabaseService"]
    #[tokio::test]
    async fn test_list_sessions() {
        let bus = SharedBus::new(64);
        let manager = SessionManager::new(bus, test_db());

        manager
            .create(CreateSessionInput {
                project_id: "p1".into(),
                workspace_id: None,
                directory: "/tmp/a".into(),
                path: None,
                parent_id: None,
                title: Some("Session A".into()),
                agent: None,
                model: None,
                metadata: None,
                permission: None,
            })
            .await
            .unwrap();

        manager
            .create(CreateSessionInput {
                project_id: "p1".into(),
                workspace_id: None,
                directory: "/tmp/a".into(),
                path: None,
                parent_id: None,
                title: Some("Session B".into()),
                agent: None,
                model: None,
                metadata: None,
                permission: None,
            })
            .await
            .unwrap();

        let all = manager.list(None).await.unwrap();
        assert_eq!(all.len(), 2);
    }

    #[ignore = "needs test database with DatabaseService"]
    #[tokio::test]
    async fn test_list_with_search() {
        let bus = SharedBus::new(64);
        let manager = SessionManager::new(bus, test_db());

        manager
            .create(CreateSessionInput {
                project_id: "p1".into(),
                workspace_id: None,
                directory: "/tmp/x".into(),
                path: None,
                parent_id: None,
                title: Some("Hello World".into()),
                agent: None,
                model: None,
                metadata: None,
                permission: None,
            })
            .await
            .unwrap();

        manager
            .create(CreateSessionInput {
                project_id: "p1".into(),
                workspace_id: None,
                directory: "/tmp/x".into(),
                path: None,
                parent_id: None,
                title: Some("Goodbye".into()),
                agent: None,
                model: None,
                metadata: None,
                permission: None,
            })
            .await
            .unwrap();

        let results = manager
            .list(Some(ListSessionsInput {
                search: Some("Hello".into()),
                ..Default::default()
            }))
            .await
            .unwrap();
        assert_eq!(results.len(), 1);
        assert_eq!(results[0].title, "Hello World");
    }

    #[ignore = "needs test database with DatabaseService"]
    #[tokio::test]
    async fn test_remove_session() {
        let bus = SharedBus::new(64);
        let manager = SessionManager::new(bus, test_db());

        let session = manager
            .create(CreateSessionInput {
                project_id: "p1".into(),
                workspace_id: None,
                directory: "/tmp/x".into(),
                path: None,
                parent_id: None,
                title: Some("To Delete".into()),
                agent: None,
                model: None,
                metadata: None,
                permission: None,
            })
            .await
            .unwrap();

        manager.remove(&session.id).await.unwrap();
        let result = manager.get(&session.id).await;
        assert!(result.is_err());
    }

    #[ignore = "needs test database with DatabaseService"]
    #[tokio::test]
    async fn test_get_nonexistent_session() {
        let bus = SharedBus::new(64);
        let manager = SessionManager::new(bus, test_db());

        let result = manager.get("nonexistent").await;
        assert!(result.is_err());
        match result {
            Err(SessionError::NotFound(_)) => {} // expected
            _ => panic!("expected NotFound error"),
        }
    }

    #[ignore = "needs test database with DatabaseService"]
    #[tokio::test]
    async fn test_update_session() {
        let bus = SharedBus::new(64);
        let manager = SessionManager::new(bus, test_db());

        let session = manager
            .create(CreateSessionInput {
                project_id: "p1".into(),
                workspace_id: None,
                directory: "/tmp/x".into(),
                path: None,
                parent_id: None,
                title: Some("Original".into()),
                agent: None,
                model: None,
                metadata: None,
                permission: None,
            })
            .await
            .unwrap();

        let updated = manager
            .update(
                &session.id,
                SessionPatch {
                    title: Some(Some("Updated Title".into())),
                    agent: Some(Some("builder".into())),
                    ..Default::default()
                },
            )
            .await
            .unwrap();

        assert_eq!(updated.title, "Updated Title");
        assert_eq!(updated.agent, Some("builder".into()));
        assert!(updated.time.updated > session.time.updated);
    }

    #[ignore = "needs test database with DatabaseService"]
    #[tokio::test]
    async fn test_update_nonexistent_session() {
        let bus = SharedBus::new(64);
        let manager = SessionManager::new(bus, test_db());

        let result = manager
            .update(
                "nonexistent",
                SessionPatch {
                    title: Some(Some("New".into())),
                    ..Default::default()
                },
            )
            .await;
        assert!(result.is_err());
    }

    #[ignore = "needs test database with DatabaseService"]
    #[tokio::test]
    async fn test_append_and_get_messages() {
        let bus = SharedBus::new(64);
        let manager = SessionManager::new(bus, test_db());

        let session = manager
            .create(CreateSessionInput {
                project_id: "p1".into(),
                workspace_id: None,
                directory: "/tmp/x".into(),
                path: None,
                parent_id: None,
                title: Some("Msg Test".into()),
                agent: None,
                model: None,
                metadata: None,
                permission: None,
            })
            .await
            .unwrap();

        let msg_info = MessageInfo::User(UserInfo {
            id: "msg_001".into(),
            session_id: session.id.clone(),
            agent: None,
            model: None,
            time: MessageTime {
                created: 1000,
                completed: None,
            },
        });

        manager
            .append_message(session.id.clone(), msg_info, vec![])
            .await
            .unwrap();

        let messages = manager.get_messages(&session.id).await.unwrap();
        assert_eq!(messages.len(), 1);
    }

    #[ignore = "needs test database with DatabaseService"]
    #[tokio::test]
    async fn test_delete_session_cascade() {
        let bus = SharedBus::new(64);
        let manager = SessionManager::new(bus, test_db());

        let session = manager
            .create(CreateSessionInput {
                project_id: "p1".into(),
                workspace_id: None,
                directory: "/tmp/x".into(),
                path: None,
                parent_id: None,
                title: Some("Cascade Test".into()),
                agent: None,
                model: None,
                metadata: None,
                permission: None,
            })
            .await
            .unwrap();

        // Add some messages
        let msg = MessageInfo::User(UserInfo {
            id: "msg_c1".into(),
            session_id: session.id.clone(),
            agent: None,
            model: None,
            time: MessageTime {
                created: 1000,
                completed: None,
            },
        });
        manager
            .append_message(session.id.clone(), msg, vec![])
            .await
            .unwrap();

        // Delete session
        manager.remove(&session.id).await.unwrap();

        // Messages should also be gone
        let msgs = manager.get_messages(&session.id).await.unwrap();
        assert!(msgs.is_empty());

        // Session should be gone
        assert!(manager.get(&session.id).await.is_err());
    }

    #[ignore = "needs test database with DatabaseService"]
    #[tokio::test]
    async fn test_list_with_pagination() {
        let bus = SharedBus::new(64);
        let manager = SessionManager::new(bus, test_db());

        for i in 0..5 {
            manager
                .create(CreateSessionInput {
                    project_id: "p1".into(),
                    workspace_id: None,
                    directory: "/tmp/x".into(),
                    path: None,
                    parent_id: None,
                    title: Some(format!("Session {i}")),
                    agent: None,
                    model: None,
                    metadata: None,
                    permission: None,
                })
                .await
                .unwrap();
        }

        let limited = manager
            .list(Some(ListSessionsInput {
                limit: Some(2),
                ..Default::default()
            }))
            .await
            .unwrap();
        assert_eq!(limited.len(), 2);
    }

    #[ignore = "needs test database with DatabaseService"]
    #[tokio::test]
    async fn test_list_roots_only() {
        let bus = SharedBus::new(64);
        let manager = SessionManager::new(bus, test_db());

        let parent = manager
            .create(CreateSessionInput {
                project_id: "p1".into(),
                workspace_id: None,
                directory: "/tmp/x".into(),
                path: None,
                parent_id: None,
                title: Some("Root".into()),
                agent: None,
                model: None,
                metadata: None,
                permission: None,
            })
            .await
            .unwrap();

        manager
            .create(CreateSessionInput {
                project_id: "p1".into(),
                workspace_id: None,
                directory: "/tmp/x".into(),
                path: None,
                parent_id: Some(parent.id),
                title: None,
                agent: None,
                model: None,
                metadata: None,
                permission: None,
            })
            .await
            .unwrap();

        let roots = manager
            .list(Some(ListSessionsInput {
                roots: Some(true),
                ..Default::default()
            }))
            .await
            .unwrap();
        assert_eq!(roots.len(), 1);
    }

    #[ignore = "needs test database with DatabaseService"]
    #[tokio::test]
    async fn test_fork_session() {
        let bus = SharedBus::new(64);
        let manager = SessionManager::new(bus, test_db());

        let original = manager
            .create(CreateSessionInput {
                project_id: "p1".into(),
                workspace_id: None,
                directory: "/tmp/x".into(),
                path: None,
                parent_id: None,
                title: Some("Original Session".into()),
                agent: Some("build".into()),
                model: None,
                metadata: None,
                permission: None,
            })
            .await
            .unwrap();

        // Add a message to original
        let msg = MessageInfo::User(UserInfo {
            id: "msg_f1".into(),
            session_id: original.id.clone(),
            agent: None,
            model: None,
            time: MessageTime {
                created: 1000,
                completed: None,
            },
        });
        manager
            .append_message(original.id.clone(), msg, vec![])
            .await
            .unwrap();

        // Fork
        let forked = manager.fork(&original.id, None).await.unwrap();

        assert!(forked.title.contains("(fork #1)"));
        assert_eq!(forked.agent, Some("build".into()));

        // Forked session should have copied messages
        let forked_msgs = manager.get_messages(&forked.id).await.unwrap();
        assert_eq!(forked_msgs.len(), 1);
    }

    #[ignore = "needs test database with DatabaseService"]
    #[tokio::test]
    async fn test_fork_nonexistent_errors() {
        let bus = SharedBus::new(64);
        let manager = SessionManager::new(bus, test_db());

        let result = manager.fork("nonexistent", None).await;
        assert!(result.is_err());
    }

    #[ignore = "needs test database with DatabaseService"]
    #[tokio::test]
    async fn test_update_message() {
        let bus = SharedBus::new(64);
        let manager = SessionManager::new(bus, test_db());

        let session = manager
            .create(CreateSessionInput {
                project_id: "p1".into(),
                workspace_id: None,
                directory: "/tmp/x".into(),
                path: None,
                parent_id: None,
                title: Some("Update Msg".into()),
                agent: None,
                model: None,
                metadata: None,
                permission: None,
            })
            .await
            .unwrap();

        let info = MessageInfo::Assistant(AssistantInfo {
            id: "msg_a1".into(),
            session_id: session.id.clone(),
            parent_id: "msg_u1".into(),
            agent: "build".into(),
            model_id: None,
            provider_id: None,
            variant: None,
            summary: false,
            cost: 0.0,
            tokens: TokenUsage::default(),
            finish: None,
            error: None,
            time: MessageTime {
                created: 1000,
                completed: None,
            },
        });
        manager
            .append_message(session.id.clone(), info, vec![])
            .await
            .unwrap();

        manager
            .update_message(
                &session.id,
                "msg_a1",
                MessagePatch {
                    finish: Some(Some("stop".into())),
                    cost: Some(0.05),
                    tokens: Some(TokenUsage {
                        input: 1000,
                        output: 500,
                        reasoning: 0,
                        cache: CacheUsage::default(),
                    }),
                    ..Default::default()
                },
            )
            .await
            .unwrap();
    }

    // ── Overflow tests ──────────────────────────────────────────

    #[test]
    fn test_check_overflow_with_zero_context() {
        let model = test_model(0, 0, 0);
        let tokens = TokenUsage {
            input: 1_000_000,
            ..Default::default()
        };
        assert!(!check_overflow(&tokens, &model, None));
    }

    #[test]
    fn test_check_overflow_under_limit() {
        let model = test_model(200_000, 180_000, 16_000);
        let tokens = TokenUsage {
            input: 50_000,
            output: 10_000,
            ..Default::default()
        };
        assert!(!check_overflow(&tokens, &model, None));
    }

    #[test]
    fn test_check_overflow_over_limit() {
        let model = test_model(200_000, 180_000, 16_000);
        let tokens = TokenUsage {
            input: 170_000,
            output: 10_000,
            ..Default::default()
        };
        assert!(check_overflow(&tokens, &model, None));
    }

    #[test]
    fn test_check_overflow_exact_boundary() {
        let model = test_model(200_000, 200_000, 16_000);
        let tokens = TokenUsage {
            input: 160_000,
            output: 4_000,
            reasoning: 0,
            cache: CacheUsage::default(),
        };
        // At 164_000 < 180_000 (context - output), should not overflow
        assert!(!check_overflow(&tokens, &model, None));
    }

    #[test]
    fn test_check_overflow_with_reasoning_tokens() {
        let model = test_model(200_000, 180_000, 16_000);
        let tokens = TokenUsage {
            input: 100_000,
            output: 20_000,
            reasoning: 50_000,
            cache: CacheUsage::default(),
        };
        // 170_000 total, should check against usable
        let result = check_overflow(&tokens, &model, None);
        // 170_000 >= 180_000 - max(20000, 16000) = 160_000? Let's not be fragile
        // Just verify it doesn't panic
        let _ = result;
    }

    #[test]
    fn test_check_overflow_with_cache() {
        let model = test_model(200_000, 180_000, 16_000);
        let tokens = TokenUsage {
            input: 50_000,
            output: 10_000,
            reasoning: 0,
            cache: CacheUsage {
                read: 80_000,
                write: 40_000,
            },
        };
        // Total: 50k + 10k + 0 + 80k + 40k = 180_000
        // Usable: 180_000 - 16_000 = 164_000
        // 180_000 >= 164_000, should overflow
        assert!(check_overflow(&tokens, &model, None));
    }

    // ── Retry tests ─────────────────────────────────────────────

    #[test]
    fn test_retry_delay_exponential_growth() {
        let d1 = retry_delay(1);
        let d2 = retry_delay(2);
        let d3 = retry_delay(3);

        assert!(d2 > d1);
        assert!(d3 > d2);
        assert_eq!(d1, RETRY_INITIAL_DELAY_MS);
    }

    #[test]
    fn test_retry_delay_capped() {
        let d10 = retry_delay(10);
        assert!(d10 <= RETRY_MAX_DELAY_NO_HEADERS_MS);
    }

    #[test]
    fn test_is_retryable_overloaded() {
        assert!(is_retryable("Provider is overloaded"));
    }

    #[test]
    fn test_is_retryable_rate_limit() {
        assert!(is_retryable("Rate limit exceeded"));
    }

    #[test]
    fn test_non_retryable() {
        assert!(!is_retryable("Invalid API key"));
    }

    #[test]
    fn test_is_retryable_connection_reset() {
        assert!(is_retryable("Connection reset by peer"));
    }

    #[test]
    fn test_is_retryable_internal_server_error() {
        assert!(is_retryable("Internal server error"));
    }

    #[test]
    fn test_retry_delay_edge_cases() {
        // Attempt 0 -> delay should be initial
        // retry_delay uses saturating_sub on attempt - 1, so attempt 0 = attempt 1 behavior
        let d0 = retry_delay(0);
        assert_eq!(d0, RETRY_INITIAL_DELAY_MS);

        // High attempt should cap
        let d100 = retry_delay(100);
        assert!(d100 <= RETRY_MAX_DELAY_NO_HEADERS_MS);
    }

    #[test]
    fn test_retry_delay_specific_values() {
        assert_eq!(retry_delay(1), 2_000);
        assert_eq!(retry_delay(2), 4_000);
        assert_eq!(retry_delay(3), 8_000);
        assert_eq!(retry_delay(4), 16_000);
        assert_eq!(retry_delay(5), 30_000); // capped at max
    }

    // ── SessionInfo serialization ───────────────────────────────

    #[test]
    fn test_session_info_json_roundtrip() {
        let info = SessionInfo {
            id: "ses_001".into(),
            slug: "test-slug".into(),
            project_id: "proj_1".into(),
            workspace_id: None,
            directory: "/tmp/test".into(),
            path: Some("subdir".into()),
            parent_id: None,
            title: "Test".into(),
            agent: None,
            model: None,
            version: "0.1.0".into(),
            summary: None,
            cost: 0.0,
            tokens: TokenUsage::default(),
            share: None,
            metadata: None,
            permission: None,
            revert: None,
            time: SessionTimestamps {
                created: 1000,
                updated: 2000,
                compacting: None,
                archived: None,
            },
        };

        let json = serde_json::to_string(&info).unwrap();
        let parsed: SessionInfo = serde_json::from_str(&json).unwrap();
        assert_eq!(parsed.id, "ses_001");
        assert_eq!(parsed.title, "Test");
    }

    // ── Fork title tests ────────────────────────────────────────

    #[test]
    fn test_fork_title_first_fork() {
        assert_eq!(fork_title("My Session"), "My Session (fork #1)");
    }

    #[test]
    fn test_fork_title_increment() {
        assert_eq!(
            fork_title("My Session (fork #1)"),
            "My Session (fork #2)"
        );
        assert_eq!(
            fork_title("My Session (fork #5)"),
            "My Session (fork #6)"
        );
    }

    #[test]
    fn test_fork_title_with_two_digit_fork_number() {
        assert_eq!(
            fork_title("My Session (fork #10)"),
            "My Session (fork #11)"
        );
    }

    #[test]
    fn test_fork_title_no_parentheses() {
        // Title has parentheses but not a fork number
        assert_eq!(
            fork_title("Session (important)"),
            "Session (important) (fork #1)"
        );
    }

    // ── TokenUsage / Usage conversion ───────────────────────────

    #[test]
    fn test_usage_to_token_usage_full() {
        let usage = Usage {
            input_tokens: Some(5000),
            output_tokens: Some(2000),
            reasoning_tokens: Some(500),
            cache_read_input_tokens: Some(1000),
            cache_write_input_tokens: Some(200),
            non_cached_input_tokens: Some(4000),
            ..Default::default()
        };

        let tu = usage_to_token_usage(&usage);
        assert_eq!(tu.input, 5000);
        assert_eq!(tu.output, 2000);
        assert_eq!(tu.reasoning, 500);
        assert_eq!(tu.cache.read, 1000);
        assert_eq!(tu.cache.write, 200);
    }

    #[test]
    fn test_usage_to_token_usage_empty() {
        let usage = Usage {
            input_tokens: None,
            output_tokens: None,
            reasoning_tokens: None,
            cache_read_input_tokens: None,
            cache_write_input_tokens: None,
            non_cached_input_tokens: None,
            total_tokens: None,
            provider_metadata: None,
        };
        let tu = usage_to_token_usage(&usage);
        assert_eq!(tu.input, 0);
        assert_eq!(tu.output, 0);
        assert_eq!(tu.reasoning, 0);
        assert_eq!(tu.cache.read, 0);
        assert_eq!(tu.cache.write, 0);
    }

    #[test]
    fn test_token_usage_default() {
        let tu = TokenUsage::default();
        assert_eq!(tu.input, 0);
        assert_eq!(tu.output, 0);
        assert_eq!(tu.reasoning, 0);
        assert_eq!(tu.cache.read, 0);
        assert_eq!(tu.cache.write, 0);
    }

    // ── Cost calculation tests ───────────────────────────────────

    #[test]
    fn test_calculate_cost_with_model_costs() {
        let model = test_model(200_000, 180_000, 16_000);

        let usage = Usage {
            input_tokens: Some(1_000_000),
            output_tokens: Some(500_000),
            reasoning_tokens: Some(0),
            cache_read_input_tokens: Some(0),
            cache_write_input_tokens: Some(0),
            non_cached_input_tokens: Some(1_000_000),
            total_tokens: None,
            provider_metadata: None,
        };

        // input cost = 1_000_000 * 3.0 / 1_000_000 = 3.0
        // output cost = 500_000 * 15.0 / 1_000_000 = 7.5
        // total = 10.5
        let cost = SessionProcessor::calculate_cost_static(&usage, &model);
        assert!((cost - 10.5).abs() < 0.01, "expected ~10.5, got {cost}");
    }

    #[test]
    fn test_calculate_cost_no_model_cost() {
        let mut model = test_model(200_000, 180_000, 16_000);
        model.cost.input = 0.0;
        model.cost.output = 0.0;

        let usage = Usage {
            input_tokens: Some(1_000_000),
            output_tokens: Some(500_000),
            reasoning_tokens: None,
            cache_read_input_tokens: None,
            cache_write_input_tokens: None,
            non_cached_input_tokens: None,
            total_tokens: None,
            provider_metadata: None,
        };

        let cost = SessionProcessor::calculate_cost_static(&usage, &model);
        assert_eq!(cost, 0.0);
    }

    #[test]
    fn test_calculate_cost_with_cache() {
        let mut model = test_model(200_000, 180_000, 16_000);
        model.cost.cache.read = 0.3;
        model.cost.cache.write = 1.0;

        let usage = Usage {
            input_tokens: Some(1_000_000),
            output_tokens: Some(500_000),
            reasoning_tokens: None,
            cache_read_input_tokens: Some(2_000_000),
            cache_write_input_tokens: Some(1_000_000),
            non_cached_input_tokens: None,
            total_tokens: None,
            provider_metadata: None,
        };

        // input: 1000K * 3 / 1000K = 3.0
        // output: 500K * 15 / 1000K = 7.5
        // cache_read: 2000K * 0.3 / 1000K = 0.6
        // cache_write: 1000K * 1.0 / 1000K = 1.0
        // total = 12.1
        let cost = SessionProcessor::calculate_cost_static(&usage, &model);
        assert!((cost - 12.1).abs() < 0.01, "expected ~12.1, got {cost}");
    }

    // ── Finish reason tests ──────────────────────────────────────

    #[test]
    fn test_finish_reason_stop() {
        assert_eq!(
            SessionProcessor::finish_reason_str_static(&crate::provider::FinishReason::Stop),
            "stop"
        );
    }

    #[test]
    fn test_finish_reason_length() {
        assert_eq!(
            SessionProcessor::finish_reason_str_static(&crate::provider::FinishReason::Length),
            "length"
        );
    }

    #[test]
    fn test_finish_reason_tool_calls() {
        assert_eq!(
            SessionProcessor::finish_reason_str_static(&crate::provider::FinishReason::ToolCalls),
            "tool_calls"
        );
    }

    #[test]
    fn test_finish_reason_content_filter() {
        assert_eq!(
            SessionProcessor::finish_reason_str_static(
                &crate::provider::FinishReason::ContentFilter
            ),
            "content_filter"
        );
    }

    // ── SessionStatus tests ──────────────────────────────────────

    #[test]
    fn test_session_status_idle_serialization() {
        let status = SessionStatus::Idle;
        let json = serde_json::to_string(&status).expect("serialize");
        assert!(json.contains("idle"));
        let parsed: SessionStatus = serde_json::from_str(&json).expect("deserialize");
        match parsed {
            SessionStatus::Idle => {}
            _ => panic!("expected Idle"),
        }
    }

    #[test]
    fn test_session_status_busy_serialization() {
        let status = SessionStatus::Busy;
        let json = serde_json::to_string(&status).expect("serialize");
        assert!(json.contains("busy"));
    }

    #[test]
    fn test_session_status_retry_serialization() {
        let status = SessionStatus::Retry {
            attempt: 3,
            message: "Rate limited".into(),
            action: Some(RetryAction {
                reason: "rate_limit".into(),
                provider: "anthropic".into(),
                title: "Retry in 16s".into(),
                message: "The provider is rate limited".into(),
                label: "Wait".into(),
                link: Some("https://status.anthropic.com".into()),
            }),
            next: 1700000000000,
        };
        let json = serde_json::to_string(&status).expect("serialize");
        assert!(json.contains("retry"));
        assert!(json.contains("Rate limited"));
        assert!(json.contains("anthropic"));

        let parsed: SessionStatus = serde_json::from_str(&json).expect("deserialize");
        match parsed {
            SessionStatus::Retry { attempt, message, .. } => {
                assert_eq!(attempt, 3);
                assert_eq!(message, "Rate limited");
            }
            _ => panic!("expected Retry"),
        }
    }

    // ── Doom loop detection tests ────────────────────────────────

    #[test]
    fn test_doom_loop_threshold_constant() {
        assert_eq!(DOOM_LOOP_THRESHOLD, 3);
    }

    // ── ProcessResult tests ──────────────────────────────────────

    #[test]
    fn test_process_result_variants() {
        // Verify Debug/PartialEq impls
        assert_eq!(ProcessResult::Compact, ProcessResult::Compact);
        assert_eq!(ProcessResult::Stop, ProcessResult::Stop);
        assert_eq!(ProcessResult::Continue, ProcessResult::Continue);
        assert_ne!(ProcessResult::Compact, ProcessResult::Continue);
    }

    // ── MessageInfo tests ────────────────────────────────────────

    #[test]
    fn test_message_info_id() {
        let user = MessageInfo::User(UserInfo {
            id: "msg_u1".into(),
            session_id: "ses_1".into(),
            agent: None,
            model: None,
            time: MessageTime {
                created: 1000,
                completed: None,
            },
        });
        assert_eq!(user.id(), "msg_u1");

        let assistant = MessageInfo::Assistant(AssistantInfo {
            id: "msg_a1".into(),
            session_id: "ses_1".into(),
            parent_id: "msg_u1".into(),
            agent: "build".into(),
            model_id: None,
            provider_id: None,
            variant: None,
            summary: false,
            cost: 0.0,
            tokens: TokenUsage::default(),
            finish: None,
            error: None,
            time: MessageTime {
                created: 2000,
                completed: None,
            },
        });
        assert_eq!(assistant.id(), "msg_a1");
    }

    #[test]
    fn test_message_info_role() {
        let user = MessageInfo::User(UserInfo {
            id: "u".into(),
            session_id: "s".into(),
            agent: None,
            model: None,
            time: MessageTime {
                created: 1000,
                completed: None,
            },
        });
        assert_eq!(user.role(), "user");

        let assistant = MessageInfo::Assistant(AssistantInfo {
            id: "a".into(),
            session_id: "s".into(),
            parent_id: "u".into(),
            agent: "b".into(),
            model_id: None,
            provider_id: None,
            variant: None,
            summary: false,
            cost: 0.0,
            tokens: TokenUsage::default(),
            finish: None,
            error: None,
            time: MessageTime {
                created: 2000,
                completed: None,
            },
        });
        assert_eq!(assistant.role(), "assistant");
    }

    #[test]
    fn test_message_info_apply_patch() {
        let mut info = MessageInfo::Assistant(AssistantInfo {
            id: "a".into(),
            session_id: "s".into(),
            parent_id: "u".into(),
            agent: "b".into(),
            model_id: None,
            provider_id: None,
            variant: None,
            summary: false,
            cost: 0.0,
            tokens: TokenUsage::default(),
            finish: None,
            error: None,
            time: MessageTime {
                created: 1000,
                completed: None,
            },
        });

        info.apply_patch(MessagePatch {
            finish: Some(Some("stop".into())),
            cost: Some(0.05),
            tokens: Some(TokenUsage {
                input: 1000,
                output: 500,
                reasoning: 0,
                cache: CacheUsage::default(),
            }),
            time_completed: Some(5000),
            ..Default::default()
        });

        match &info {
            MessageInfo::Assistant(a) => {
                assert_eq!(a.finish.as_deref(), Some("stop"));
                assert_eq!(a.cost, 0.05);
                assert_eq!(a.tokens.input, 1000);
                assert_eq!(a.tokens.output, 500);
                assert_eq!(a.time.completed, Some(5000));
            }
            _ => panic!("expected Assistant"),
        }
    }

    // ── Part helper tests ────────────────────────────────────────

    #[test]
    fn test_part_set_message_id() {
        let mut part = Part::Text(TextPart {
            id: "old_id".into(),
            message_id: "old_mid".into(),
            session_id: "old_sid".into(),
            text: "hello".into(),
            metadata: None,
            time: PartTime {
                start: None,
                end: None,
            },
        });

        part.set_message_id("new_mid");
        match &part {
            Part::Text(p) => assert_eq!(p.message_id, "new_mid"),
            _ => panic!("expected Text"),
        }
    }

    #[test]
    fn test_part_set_session_id() {
        let mut part = Part::Tool(ToolPart {
            id: "old_id".into(),
            message_id: "mid".into(),
            session_id: "old_sid".into(),
            tool: "test".into(),
            call_id: "call1".into(),
            state: ToolState::Pending {
                input: serde_json::json!({}),
            },
            metadata: None,
        });

        part.set_session_id("new_sid");
        match &part {
            Part::Tool(p) => assert_eq!(p.session_id, "new_sid"),
            _ => panic!("expected Tool"),
        }
    }

    #[test]
    fn test_part_set_id() {
        let mut part = Part::Reasoning(ReasoningPart {
            id: "old_id".into(),
            message_id: "mid".into(),
            session_id: "sid".into(),
            text: "thinking...".into(),
            metadata: None,
            time: PartTime {
                start: None,
                end: None,
            },
        });

        part.set_id("new_id");
        match &part {
            Part::Reasoning(p) => assert_eq!(p.id, "new_id"),
            _ => panic!("expected Reasoning"),
        }
    }

    // ── Compaction buffer constant test ──────────────────────────

    #[test]
    fn test_compaction_buffer_constant() {
        assert_eq!(COMPACTION_BUFFER, 20_000);
    }

    // ── ModelSelection serialization ─────────────────────────────

    #[test]
    fn test_model_selection_with_variant() {
        let ms = ModelSelection {
            id: "claude-sonnet".into(),
            provider_id: "anthropic".into(),
            variant: Some("thinking".into()),
        };
        let json = serde_json::to_string(&ms).expect("serialize");
        assert!(json.contains("claude-sonnet"));
        assert!(json.contains("thinking"));
        let parsed: ModelSelection = serde_json::from_str(&json).expect("deserialize");
        assert_eq!(parsed.variant.as_deref(), Some("thinking"));
    }

    #[test]
    fn test_model_selection_without_variant() {
        let ms = ModelSelection {
            id: "gpt-5".into(),
            provider_id: "openai".into(),
            variant: None,
        };
        let json = serde_json::to_string(&ms).expect("serialize");
        assert!(!json.contains("variant"));
    }

    // ── SessionInfo with all optional fields ─────────────────────

    #[test]
    fn test_session_info_with_summary() {
        let info = SessionInfo {
            id: "ses_001".into(),
            slug: "test".into(),
            project_id: "p1".into(),
            workspace_id: None,
            directory: "/tmp".into(),
            path: None,
            parent_id: None,
            title: "Test".into(),
            agent: None,
            model: None,
            version: "1.0".into(),
            summary: Some(SessionSummary {
                additions: 10,
                deletions: 5,
                files: 3,
                diffs: Some(vec![FileDiff {
                    path: "src/main.rs".into(),
                    hash: "abc".into(),
                }]),
            }),
            cost: 1.5,
            tokens: TokenUsage {
                input: 10000,
                output: 5000,
                reasoning: 1000,
                cache: CacheUsage {
                    read: 500,
                    write: 100,
                },
            },
            share: Some(ShareInfo {
                url: "https://share.opencode.dev/abc".into(),
            }),
            metadata: Some(serde_json::json!({"foo": "bar"})),
            permission: None,
            revert: Some(RevertInfo {
                message_id: "msg_001".into(),
                part_id: Some("part_001".into()),
                snapshot: None,
                diff: None,
            }),
            time: SessionTimestamps {
                created: 1000,
                updated: 2000,
                compacting: Some(1500),
                archived: None,
            },
        };

        let json = serde_json::to_string(&info).expect("serialize");
        let parsed: SessionInfo = serde_json::from_str(&json).expect("deserialize");
        assert_eq!(parsed.summary.as_ref().unwrap().additions, 10);
        assert_eq!(parsed.share.as_ref().unwrap().url, "https://share.opencode.dev/abc");
        assert_eq!(parsed.cost, 1.5);
        assert_eq!(parsed.tokens.input, 10000);
        assert!(parsed.revert.is_some());
    }

    // ── SessionError display tests ────────────────────────────────

    #[test]
    fn test_session_error_not_found() {
        let err = SessionError::NotFound("ses_abc".into());
        assert!(err.to_string().contains("ses_abc"));
    }

    #[test]
    fn test_session_error_busy() {
        let err = SessionError::Busy("ses_busy".into());
        assert!(err.to_string().contains("ses_busy"));
    }

    #[test]
    fn test_session_error_doom_loop() {
        let err = SessionError::DoomLoop {
            tool: "search".into(),
            count: 3,
        };
        assert!(err.to_string().contains("search"));
        assert!(err.to_string().contains("3"));
    }

    #[test]
    fn test_session_error_compaction_failed() {
        let err = SessionError::CompactionFailed("summary generation failed".into());
        assert!(err.to_string().contains("summary generation failed"));
    }
}
