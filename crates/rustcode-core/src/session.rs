//! Session management — messages, processor, prompt construction.
//!
//! Ported from: `packages/opencode/src/session/session.ts` and `processor.ts`

use crate::id;
use crate::provider::Usage;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

/// Session information.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Session {
    /// Unique session ID
    pub id: String,
    /// URL-friendly slug
    pub slug: String,
    /// Project ID
    pub project_id: String,
    /// Working directory
    pub directory: String,
    /// Title
    pub title: String,
    /// Agent name
    #[serde(skip_serializing_if = "Option::is_none")]
    pub agent: Option<String>,
    /// Model selection
    #[serde(skip_serializing_if = "Option::is_none")]
    pub model: Option<ModelSelection>,
    /// Version that created this session
    pub version: String,
    /// Cost so far
    #[serde(default)]
    pub cost: f64,
    /// Token usage
    #[serde(default)]
    pub tokens: TokenUsage,
    /// Timestamps
    pub time: Timestamps,
}

/// Model selection for a session.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ModelSelection {
    pub id: String,
    pub provider_id: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub variant: Option<String>,
}

/// Token usage tracking.
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
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Timestamps {
    pub created: u64,
    pub updated: u64,
}

/// Message in a session.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "role")]
pub enum Message {
    #[serde(rename = "user")]
    User(UserMessage),
    #[serde(rename = "assistant")]
    Assistant(AssistantMessage),
    #[serde(rename = "tool")]
    Tool(ToolMessage),
}

/// User message.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UserMessage {
    pub id: String,
    pub session_id: String,
    pub content: String,
    pub time: u64,
}

/// Assistant message.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AssistantMessage {
    pub id: String,
    pub session_id: String,
    pub content: String,
    pub agent: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub model: Option<String>,
    #[serde(default)]
    pub cost: f64,
    #[serde(default)]
    pub tokens: TokenUsage,
    #[serde(default)]
    pub error: Option<String>,
    pub time: MessageTime,
}

/// Tool message (result of a tool call).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ToolMessage {
    pub id: String,
    pub session_id: String,
    pub message_id: String,
    pub tool: String,
    pub call_id: String,
    pub state: ToolState,
}

/// Tool execution state.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "status")]
pub enum ToolState {
    #[serde(rename = "pending")]
    Pending { input: serde_json::Value },
    #[serde(rename = "running")]
    Running { input: serde_json::Value, start: u64 },
    #[serde(rename = "completed")]
    Completed {
        input: serde_json::Value,
        output: String,
        title: String,
        metadata: HashMap<String, serde_json::Value>,
        start: u64,
        end: u64,
    },
    #[serde(rename = "error")]
    Error {
        input: serde_json::Value,
        error: String,
        start: u64,
        end: u64,
    },
}

/// Message timestamps.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MessageTime {
    pub created: u64,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub completed: Option<u64>,
}

/// Session processor — manages the LLM interaction loop.
///
/// # Source
/// Ported from `packages/opencode/src/session/processor.ts`.
pub struct SessionProcessor {
    session: Session,
    messages: Vec<Message>,
}

impl SessionProcessor {
    /// Create a new session processor.
    pub fn new(session: Session) -> Self {
        Self {
            session,
            messages: Vec::new(),
        }
    }

    /// Add a user message.
    pub fn add_user_message(&mut self, content: String) -> &mut Self {
        self.messages.push(Message::User(UserMessage {
            id: id::ascending("msg"),
            session_id: self.session.id.clone(),
            content,
            time: chrono::Utc::now().timestamp_millis() as u64,
        }));
        self
    }

    /// Get all messages for LLM input.
    pub fn messages(&self) -> &[Message] {
        &self.messages
    }

    /// Get the session.
    pub fn session(&self) -> &Session {
        &self.session
    }
}
