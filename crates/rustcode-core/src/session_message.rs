//! Session message types — MessageV2, message IDs, and related enums.
//!
//! Ported from:
//! - `packages/core/src/session/message.ts` (lines 1–194)
//! - `packages/core/src/session/message-id.ts` (lines 1–13)
//! - `packages/opencode/src/session/message-v2.ts` (lines 1–744)

use crate::session_info::{MessageId, SessionId};
use serde::{Deserialize, Serialize};

// ══════════════════════════════════════════════════════════════════════════════
// Session Message ID
// ══════════════════════════════════════════════════════════════════════════════

/// Session message identifier — branded string starting with "msg_".
///
/// # Source
/// `packages/core/src/session/message-id.ts` lines 7–13 `SessionMessageID.ID`.
pub type SessionMessageId = String;

// ══════════════════════════════════════════════════════════════════════════════
// Prompt types (core, used by messages)
// ══════════════════════════════════════════════════════════════════════════════

/// A source range within text.
///
/// # Source
/// `packages/core/src/session/prompt.ts` lines 3–7 `Source`.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PromptSource {
    pub start: f64,
    pub end: f64,
    pub text: String,
}

/// A file attachment within a prompt.
///
/// # Source
/// `packages/core/src/session/prompt.ts` lines 9–25 `FileAttachment`.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PromptFileAttachment {
    pub uri: String,
    pub mime: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub source: Option<PromptSource>,
}

/// An agent attachment within a prompt.
///
/// # Source
/// `packages/core/src/session/prompt.ts` lines 27–30 `AgentAttachment`.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PromptAgentAttachment {
    pub name: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub source: Option<PromptSource>,
}

/// The user prompt structure.
///
/// # Source
/// `packages/core/src/session/prompt.ts` lines 32–46 `Prompt`.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Prompt {
    pub text: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub files: Option<Vec<PromptFileAttachment>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub agents: Option<Vec<PromptAgentAttachment>>,
}

// ══════════════════════════════════════════════════════════════════════════════
// V2 API Error types (from packages/sdk/js/src/v2/gen/types.gen.ts)
// ══════════════════════════════════════════════════════════════════════════════

/// Provider authentication error.
///
/// # Source
/// `packages/sdk/js/src/v2/gen/types.gen.ts` lines 262–268 `ProviderAuthError`.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProviderAuthError {
    pub provider_id: String,
    pub message: String,
}

/// Unknown error with optional reference.
///
/// # Source
/// `packages/sdk/js/src/v2/gen/types.gen.ts` lines 270–276 `UnknownError`.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UnknownError {
    pub message: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub ref_: Option<String>,
}

/// Message output exceeded maximum length.
///
/// # Source
/// `packages/sdk/js/src/v2/gen/types.gen.ts` lines 278–283 `MessageOutputLengthError`.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MessageOutputLengthError {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub extra: Option<serde_json::Value>,
}

/// Message was aborted.
///
/// # Source
/// `packages/sdk/js/src/v2/gen/types.gen.ts` lines 285–290 `MessageAbortedError`.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MessageAbortedError {
    pub message: String,
}

/// Structured output parsing failed.
///
/// # Source
/// `packages/sdk/js/src/v2/gen/types.gen.ts` lines 292–298 `StructuredOutputError`.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StructuredOutputError {
    pub message: String,
    pub retries: f64,
}

/// Context window overflow.
///
/// # Source
/// `packages/sdk/js/src/v2/gen/types.gen.ts` lines 300–306 `ContextOverflowError`.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ContextOverflowError {
    pub message: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub response_body: Option<String>,
}

/// Content was filtered by the provider.
///
/// # Source
/// `packages/sdk/js/src/v2/gen/types.gen.ts` lines 308–313 `ContentFilterError`.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ContentFilterError {
    pub message: String,
}

/// Generic API error from a provider.
///
/// # Source
/// `packages/sdk/js/src/v2/gen/types.gen.ts` lines 315–329 `ApiError`.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ApiError {
    pub message: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub status_code: Option<f64>,
    #[serde(default)]
    pub is_retryable: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub response_headers: Option<std::collections::HashMap<String, String>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub response_body: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub metadata: Option<std::collections::HashMap<String, String>>,
}

/// Union of all possible assistant message errors.
///
/// # Source
/// `packages/sdk/js/src/v2/gen/types.gen.ts` lines 339–347 `AssistantMessage.error`.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "name")]
pub enum AssistantError {
    #[serde(rename = "ProviderAuthError")]
    ProviderAuth(ProviderAuthError),
    #[serde(rename = "UnknownError")]
    Unknown(UnknownError),
    #[serde(rename = "MessageOutputLengthError")]
    OutputLength(MessageOutputLengthError),
    #[serde(rename = "MessageAbortedError")]
    Aborted(MessageAbortedError),
    #[serde(rename = "StructuredOutputError")]
    StructuredOutput(StructuredOutputError),
    #[serde(rename = "ContextOverflowError")]
    ContextOverflow(ContextOverflowError),
    #[serde(rename = "ContentFilterError")]
    ContentFilter(ContentFilterError),
    #[serde(rename = "APIError")]
    Api(ApiError),
}

// ══════════════════════════════════════════════════════════════════════════════
// Session Message — the tagged union of all message types
// ══════════════════════════════════════════════════════════════════════════════

/// Session message — discriminated union of all message variants.
///
/// # Source
/// `packages/core/src/session/message.ts` lines 178–191 `Message`.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type")]
#[allow(clippy::large_enum_variant)]
pub enum SessionMessage {
    /// Agent was switched
    #[serde(rename = "agent-switched")]
    AgentSwitched(AgentSwitchedMessage),

    /// Model was switched
    #[serde(rename = "model-switched")]
    ModelSwitched(ModelSwitchedMessage),

    /// User message
    #[serde(rename = "user")]
    User(UserMessage),

    /// Synthetic/system-generated message
    #[serde(rename = "synthetic")]
    Synthetic(SyntheticMessage),

    /// System update message
    #[serde(rename = "system")]
    System(SystemMessage),

    /// Shell command message
    #[serde(rename = "shell")]
    Shell(ShellMessage),

    /// Assistant response message
    #[serde(rename = "assistant")]
    Assistant(AssistantMessage),

    /// Compaction marker message
    #[serde(rename = "compaction")]
    Compaction(CompactionMessage),
}

/// The message type discriminant.
///
/// # Source
/// `packages/core/src/session/message.ts` line 193 `Type`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum MessageType {
    AgentSwitched,
    ModelSwitched,
    User,
    Synthetic,
    System,
    Shell,
    Assistant,
    Compaction,
}

// ── Agent Switched ───────────────────────────────────────────────────────────

/// Message emitted when the agent changes for a session.
///
/// # Source
/// `packages/core/src/session/message.ts` lines 22–26 `AgentSwitched`.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AgentSwitchedMessage {
    pub id: SessionMessageId,
    pub agent: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub metadata: Option<serde_json::Value>,
    pub time: MessageTime,
}

// ── Model Switched ───────────────────────────────────────────────────────────

/// Message emitted when the model changes for a session.
///
/// # Source
/// `packages/core/src/session/message.ts` lines 28–31 `ModelSwitched`.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ModelSwitchedMessage {
    pub id: SessionMessageId,
    pub model: crate::session_info::ModelRef,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub metadata: Option<serde_json::Value>,
    pub time: MessageTime,
}

// ── User Message ─────────────────────────────────────────────────────────────

/// A user-initiated message.
///
/// # Source
/// `packages/core/src/session/message.ts` lines 34–43 `User`.
/// `packages/sdk/js/src/v2/gen/types.gen.ts` lines 237–260 `UserMessage`.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UserMessage {
    pub id: SessionMessageId,
    /// Session identifier this message belongs to
    #[serde(skip_serializing_if = "Option::is_none")]
    pub session_id: Option<SessionId>,
    pub text: String,
    /// Output format (text or json_schema)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub format: Option<OutputFormat>,
    /// Summary of file changes
    #[serde(skip_serializing_if = "Option::is_none")]
    pub summary: Option<MessageSummary>,
    /// Agent name if specified
    #[serde(skip_serializing_if = "Option::is_none")]
    pub agent: Option<String>,
    /// Model selection if specified
    #[serde(skip_serializing_if = "Option::is_none")]
    pub model: Option<MessageModelRef>,
    /// System prompt override
    #[serde(skip_serializing_if = "Option::is_none")]
    pub system: Option<String>,
    /// Tool enablement map
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tools: Option<std::collections::HashMap<String, bool>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub files: Option<Vec<PromptFileAttachment>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub agents: Option<Vec<PromptAgentAttachment>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub metadata: Option<serde_json::Value>,
    pub time: MessageTime,
}

/// Output format — text or json_schema.
///
/// # Source
/// `packages/sdk/js/src/v2/gen/types.gen.ts` lines 221–235 `OutputFormat`.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type")]
pub enum OutputFormat {
    #[serde(rename = "text")]
    Text,
    #[serde(rename = "json_schema")]
    JsonSchema {
        schema: serde_json::Value,
        #[serde(skip_serializing_if = "Option::is_none")]
        retry_count: Option<f64>,
    },
}

/// Summary for a message (title, body, diffs).
///
/// # Source
/// `packages/sdk/js/src/v2/gen/types.gen.ts` lines 245–249 `UserMessage.summary`.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MessageSummary {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub title: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub body: Option<String>,
    #[serde(default)]
    pub diffs: Vec<SnapshotFileDiff>,
}

/// File diff in a message summary.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SnapshotFileDiff {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub file: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub patch: Option<String>,
    #[serde(default)]
    pub additions: f64,
    #[serde(default)]
    pub deletions: f64,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub status: Option<String>,
}

/// Model reference used in messages.
///
/// # Source
/// `packages/sdk/js/src/v2/gen/types.gen.ts` lines 251–255 `UserMessage.model`.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MessageModelRef {
    #[serde(rename = "providerID")]
    pub provider_id: String,
    #[serde(rename = "modelID")]
    pub model_id: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub variant: Option<String>,
}

// ── Synthetic Message ────────────────────────────────────────────────────────

/// A system-generated context message.
///
/// # Source
/// `packages/core/src/session/message.ts` lines 45–49 `Synthetic`.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SyntheticMessage {
    pub id: SessionMessageId,
    pub session_id: SessionId,
    pub text: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub metadata: Option<serde_json::Value>,
    pub time: MessageTime,
}

// ── System Message ───────────────────────────────────────────────────────────

/// A system context update message.
///
/// # Source
/// `packages/core/src/session/message.ts` lines 52–56 `System`.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SystemMessage {
    pub id: SessionMessageId,
    pub text: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub metadata: Option<serde_json::Value>,
    pub time: MessageTime,
}

// ── Shell Message ────────────────────────────────────────────────────────────

/// A shell command execution message.
///
/// # Source
/// `packages/core/src/session/message.ts` lines 58–68 `Shell`.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ShellMessage {
    pub id: SessionMessageId,
    pub call_id: String,
    pub command: String,
    pub output: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub metadata: Option<serde_json::Value>,
    pub time: ShellTime,
}

/// Timestamps for shell messages (includes completion).
///
/// # Source
/// `packages/core/src/session/message.ts` lines 64–68 `time`.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ShellTime {
    pub created: u64,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub completed: Option<u64>,
}

// ── Assistant Message ────────────────────────────────────────────────────────

/// An assistant (LLM) response message.
///
/// # Source
/// `packages/core/src/session/message.ts` lines 142–168 `Assistant`.
/// `packages/sdk/js/src/v2/gen/types.gen.ts` lines 331–372 `AssistantMessage`.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AssistantMessage {
    pub id: SessionMessageId,
    /// Session identifier this message belongs to
    #[serde(skip_serializing_if = "Option::is_none")]
    pub session_id: Option<SessionId>,
    /// Agent name
    pub agent: String,
    /// Model reference
    pub model: crate::session_info::ModelRef,
    /// Parent message ID
    #[serde(skip_serializing_if = "Option::is_none")]
    pub parent_id: Option<SessionMessageId>,
    /// Provider ID (e.g. "anthropic", "openai")
    #[serde(skip_serializing_if = "Option::is_none")]
    pub provider_id: Option<String>,
    /// Model ID that generated this response
    #[serde(skip_serializing_if = "Option::is_none")]
    pub model_id: Option<String>,
    /// Mode (e.g. "subagent", "primary")
    #[serde(skip_serializing_if = "Option::is_none")]
    pub mode: Option<String>,
    /// Working directory path info
    #[serde(skip_serializing_if = "Option::is_none")]
    pub path: Option<AssistantPath>,
    /// Whether this is a summary message
    #[serde(default)]
    pub summary: bool,
    /// Model variant
    #[serde(skip_serializing_if = "Option::is_none")]
    pub variant: Option<String>,
    /// Assistant content blocks
    pub content: Vec<AssistantContent>,
    /// Snapshot tracking
    #[serde(skip_serializing_if = "Option::is_none")]
    pub snapshot: Option<AssistantSnapshot>,
    /// Finish reason
    #[serde(skip_serializing_if = "Option::is_none")]
    pub finish: Option<String>,
    /// Accumulated cost
    #[serde(skip_serializing_if = "Option::is_none")]
    pub cost: Option<f64>,
    /// Token usage
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tokens: Option<crate::session_info::TokenUsage>,
    /// Error (typed)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<AssistantError>,
    /// Opaque metadata
    #[serde(skip_serializing_if = "Option::is_none")]
    pub metadata: Option<serde_json::Value>,
    /// Timestamps
    pub time: AssistantTime,
}

/// Working directory path for an assistant message.
///
/// # Source
/// `packages/sdk/js/src/v2/gen/types.gen.ts` lines 353–356 `AssistantMessage.path`.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AssistantPath {
    pub cwd: String,
    pub root: String,
}

/// Snapshot tracking within an assistant message.
///
/// # Source
/// `packages/core/src/session/message.ts` lines 148–151 `snapshot`.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AssistantSnapshot {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub start: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub end: Option<String>,
}

/// Timestamps for assistant messages.
///
/// # Source
/// `packages/core/src/session/message.ts` lines 164–167 `time`.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AssistantTime {
    pub created: u64,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub completed: Option<u64>,
}

// ── Assistant Content ────────────────────────────────────────────────────────

/// Content within an assistant message — text, reasoning, or tool call.
///
/// # Source
/// `packages/core/src/session/message.ts` lines 137–140 `AssistantContent`.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type")]
#[allow(clippy::large_enum_variant)]
pub enum AssistantContent {
    /// Plain text output
    #[serde(rename = "text")]
    Text(AssistantText),

    /// Reasoning / thinking output
    #[serde(rename = "reasoning")]
    Reasoning(AssistantReasoning),

    /// Tool call invocation
    #[serde(rename = "tool")]
    Tool(AssistantTool),
}

/// Assistant text content.
///
/// # Source
/// `packages/core/src/session/message.ts` lines 124–128 `AssistantText`.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AssistantText {
    pub id: String,
    pub text: String,
}

/// Assistant reasoning content.
///
/// # Source
/// `packages/core/src/session/message.ts` lines 130–135 `AssistantReasoning`.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AssistantReasoning {
    pub id: String,
    pub text: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub provider_metadata: Option<serde_json::Value>,
}

/// Assistant tool call content.
///
/// # Source
/// `packages/core/src/session/message.ts` lines 106–122 `AssistantTool`.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AssistantTool {
    pub id: String,
    pub name: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub provider: Option<ToolProviderMeta>,
    pub state: ToolState,
    pub time: ToolCallTime,
}

/// Provider-side execution metadata for a tool call.
///
/// # Source
/// `packages/core/src/session/message.ts` lines 109–114 `provider`.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ToolProviderMeta {
    #[serde(default)]
    pub executed: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub metadata: Option<serde_json::Value>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub result_metadata: Option<serde_json::Value>,
}

/// Tool call lifecycle timestamps.
///
/// # Source
/// `packages/core/src/session/message.ts` lines 117–122 `time`.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ToolCallTime {
    pub created: u64,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub ran: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub completed: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub pruned: Option<u64>,
}

// ── Tool State ───────────────────────────────────────────────────────────────

/// The state of a tool execution.
///
/// # Source
/// `packages/core/src/session/message.ts` lines 70–104 `ToolState`.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "status")]
pub enum ToolState {
    /// Tool is pending execution
    #[serde(rename = "pending")]
    Pending { input: String },

    /// Tool is running
    #[serde(rename = "running")]
    Running {
        input: serde_json::Value,
        structured: serde_json::Value,
        content: Vec<ToolContentItem>,
    },

    /// Tool completed successfully
    #[serde(rename = "completed")]
    Completed {
        input: serde_json::Value,
        #[serde(skip_serializing_if = "Option::is_none")]
        attachments: Option<serde_json::Value>,
        content: Vec<ToolContentItem>,
        #[serde(default)]
        output_paths: Vec<String>,
        structured: serde_json::Value,
        result: serde_json::Value,
    },

    /// Tool execution failed with an error
    #[serde(rename = "error")]
    Error {
        input: serde_json::Value,
        content: Vec<ToolContentItem>,
        structured: serde_json::Value,
        error: serde_json::Value,
        result: serde_json::Value,
    },
}

/// An item in tool output content — text or media.
///
/// # Source
/// `packages/core/src/session/message.ts` — `ToolContent` item.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type")]
pub enum ToolContentItem {
    /// Text output
    #[serde(rename = "text")]
    Text { text: String },
    /// Media / binary output
    #[serde(rename = "media")]
    Media {
        #[serde(rename = "mediaType")]
        media_type: String,
        data: String,
    },
}

// ── Compaction Message ───────────────────────────────────────────────────────

/// A compaction (context summarization) message.
///
/// # Source
/// `packages/core/src/session/message.ts` lines 170–176 `Compaction`.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CompactionMessage {
    pub id: SessionMessageId,
    pub reason: String,
    pub summary: String,
    pub recent: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub metadata: Option<serde_json::Value>,
    #[serde(default)]
    pub time: MessageTime,
}

// ── Common Message Time ──────────────────────────────────────────────────────

/// Common message timestamp (created only).
///
/// # Source
/// `packages/core/src/session/message.ts` lines 17–20 `Base.time`.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct MessageTime {
    pub created: u64,
}

// ══════════════════════════════════════════════════════════════════════════════
// Tests
// ══════════════════════════════════════════════════════════════════════════════

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_user_message_serialization() {
        let msg = SessionMessage::User(UserMessage {
            id: "msg_001".into(),
            session_id: None,
            text: "Hello, world!".into(),
            format: None,
            summary: None,
            agent: None,
            model: None,
            system: None,
            tools: None,
            files: None,
            agents: None,
            metadata: None,
            time: MessageTime {
                created: 1700000000000,
            },
        });

        let json = serde_json::to_string(&msg).expect("serialize");
        assert!(json.contains(r#""type":"user""#));
        assert!(json.contains("Hello, world!"));
        let parsed: SessionMessage = serde_json::from_str(&json).expect("deserialize");
        match parsed {
            SessionMessage::User(u) => assert_eq!(u.text, "Hello, world!"),
            _ => panic!("expected User variant"),
        }
    }

    #[test]
    fn test_assistant_message_full() {
        let msg = SessionMessage::Assistant(AssistantMessage {
            id: "msg_002".into(),
            session_id: None,
            agent: "build".into(),
            model: crate::session_info::ModelRef {
                id: "claude-sonnet-4-20250514".into(),
                provider_id: "anthropic".into(),
                variant: None,
            },
            parent_id: None,
            provider_id: Some("anthropic".into()),
            model_id: Some("claude-sonnet-4-20250514".into()),
            mode: None,
            path: None,
            summary: false,
            variant: None,
            content: vec![
                AssistantContent::Text(AssistantText {
                    id: "text_1".into(),
                    text: "I will help you with that.".into(),
                }),
                AssistantContent::Reasoning(AssistantReasoning {
                    id: "reason_1".into(),
                    text: "Let me think about this first.".into(),
                    provider_metadata: None,
                }),
            ],
            snapshot: Some(AssistantSnapshot {
                start: Some("snap_a".into()),
                end: None,
            }),
            finish: Some("stop".into()),
            cost: Some(0.15),
            tokens: Some(crate::session_info::TokenUsage {
                input: 2000,
                output: 500,
                reasoning: 300,
                cache: crate::session_info::CacheUsage { read: 0, write: 0 },
            }),
            error: None,
            metadata: None,
            time: AssistantTime {
                created: 1700000001000,
                completed: Some(1700000003000),
            },
        });

        let json = serde_json::to_string(&msg).expect("serialize");
        assert!(json.contains(r#""type":"assistant""#));
        assert!(json.contains(r#""type":"text""#));
        assert!(json.contains(r#""type":"reasoning""#));
        let parsed: SessionMessage = serde_json::from_str(&json).expect("deserialize");
        match parsed {
            SessionMessage::Assistant(a) => {
                assert_eq!(a.agent, "build");
                assert_eq!(a.content.len(), 2);
                assert_eq!(a.cost, Some(0.15));
            }
            _ => panic!("expected Assistant variant"),
        }
    }

    #[test]
    fn test_tool_state_pending() {
        let state = ToolState::Pending {
            input: "query database".into(),
        };
        let json = serde_json::to_string(&state).expect("serialize");
        assert!(json.contains(r#""status":"pending""#));
        let parsed: ToolState = serde_json::from_str(&json).expect("deserialize");
        match parsed {
            ToolState::Pending { input } => assert_eq!(input, "query database"),
            _ => panic!("expected Pending variant"),
        }
    }

    #[test]
    fn test_tool_state_completed() {
        let state = ToolState::Completed {
            input: serde_json::json!({"query": "SELECT 1"}),
            attachments: None,
            content: vec![ToolContentItem::Text {
                text: "Result: 1 row".into(),
            }],
            output_paths: vec!["/tmp/output.txt".into()],
            structured: serde_json::json!({}),
            result: serde_json::json!({"rows": 1}),
        };
        let json = serde_json::to_string(&state).expect("serialize");
        assert!(json.contains(r#""status":"completed""#));
        let parsed: ToolState = serde_json::from_str(&json).expect("deserialize");
        match parsed {
            ToolState::Completed { output_paths, .. } => {
                assert_eq!(output_paths.len(), 1);
            }
            _ => panic!("expected Completed variant"),
        }
    }

    #[test]
    fn test_tool_state_error() {
        let state = ToolState::Error {
            input: serde_json::json!({"file": "nonexistent.txt"}),
            content: vec![],
            structured: serde_json::json!({}),
            error: serde_json::json!({"message": "File not found"}),
            result: serde_json::json!(null),
        };
        let json = serde_json::to_string(&state).expect("serialize");
        assert!(json.contains(r#""status":"error""#));
        let parsed: ToolState = serde_json::from_str(&json).expect("deserialize");
        match parsed {
            ToolState::Error { error, .. } => {
                assert_eq!(error["message"], "File not found");
            }
            _ => panic!("expected Error variant"),
        }
    }

    #[test]
    fn test_shell_message() {
        let msg = SessionMessage::Shell(ShellMessage {
            id: "msg_sh_1".into(),
            call_id: "call_001".into(),
            command: "ls -la".into(),
            output: "total 4\ndrwxr-xr-x ...".into(),
            metadata: None,
            time: ShellTime {
                created: 1000,
                completed: Some(1500),
            },
        });
        let json = serde_json::to_string(&msg).expect("serialize");
        assert!(json.contains(r#""type":"shell""#));
        assert!(json.contains("ls -la"));
    }

    #[test]
    fn test_compaction_message() {
        let msg = SessionMessage::Compaction(CompactionMessage {
            id: "msg_cmp_1".into(),
            reason: "auto".into(),
            summary: "Worked on auth module".into(),
            recent: "Recent conversation...".into(),
            metadata: None,
            time: MessageTime {
                created: 1700000000000,
            },
        });
        let json = serde_json::to_string(&msg).expect("serialize");
        assert!(json.contains(r#""type":"compaction""#));
        assert!(json.contains("auto"));
        let parsed: SessionMessage = serde_json::from_str(&json).expect("deserialize");
        match parsed {
            SessionMessage::Compaction(c) => assert_eq!(c.reason, "auto"),
            _ => panic!("expected Compaction variant"),
        }
    }

    #[test]
    fn test_prompt_serialization() {
        let prompt = Prompt {
            text: "Fix the bug".into(),
            files: Some(vec![PromptFileAttachment {
                uri: "file:///tmp/test.rs".into(),
                mime: "text/rust".into(),
                name: Some("test.rs".into()),
                description: None,
                source: None,
            }]),
            agents: Some(vec![PromptAgentAttachment {
                name: "build".into(),
                source: None,
            }]),
        };
        let json = serde_json::to_string(&prompt).expect("serialize");
        assert!(json.contains("Fix the bug"));
        assert!(json.contains("test.rs"));
    }

    #[test]
    fn test_message_type_enum() {
        assert_eq!(
            serde_json::to_string(&MessageType::User).expect("serialize"),
            r#""user""#
        );
        assert_eq!(
            serde_json::to_string(&MessageType::Assistant).expect("serialize"),
            r#""assistant""#
        );
        assert_eq!(
            serde_json::to_string(&MessageType::AgentSwitched).expect("serialize"),
            r#""agent-switched""#
        );
    }
}
