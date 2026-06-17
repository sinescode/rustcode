//! Question / user prompt system — core types for interactive Q&A.
//!
//! Ported from:
//! - `packages/opencode/src/question/index.ts`
//! - `packages/opencode/src/question/schema.ts`
//! - `packages/core/src/question.ts`
//! - `packages/core/src/tool/question.ts`
//!
//! This module defines the data types for the question/answer system that
//! allows the AI agent to ask the user questions during execution. Questions
//! are published as events on the bus, and answers (or rejections) flow back
//! through deferred futures.
//!
//! ## Architecture
//!
//! In the TS source, questions are asked via `Question.ask()` which generates
//! an ascending ID (prefixed `que`), stores a `Deferred` in a pending map,
//! publishes an `Asked` event, and awaits the deferred. The user (via TUI
//! or CLI) replies via `Question.reply()` or rejects via `Question.reject()`.
//!
//! This module provides the Rust types for:
//! - [`QuestionId`] — branded string ID with ascending generation
//! - [`QuestionOption`] — a labeled choice the user can pick
//! - [`QuestionInfo`] — a complete question with options
//! - [`QuestionPrompt`] — a prompt for the LLM to generate questions
//! - [`QuestionRequest`] — an active question request (pending)
//! - [`QuestionAnswer`] — user's selected labels
//! - [`QuestionReply`] — full reply with all answers
//! - [`QuestionEvent`] — events published on the bus
//! - Error types for rejection and not-found conditions

use serde::{Deserialize, Serialize};
use thiserror::Error;

// ---------------------------------------------------------------------------
// Question ID
// ---------------------------------------------------------------------------

/// A unique question ID, prefixed with `que_`.
///
/// # Source
/// Ported from `packages/opencode/src/question/schema.ts` `QuestionID`
/// and `packages/core/src/question.ts` `ID`.
///
/// The ID format is `que_` + ascending identifier similar to other IDs
/// in the system (e.g., `evt_`, `ses_`).
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct QuestionId(String);

impl QuestionId {
    /// Create a `QuestionId` from a string, validating the `que_` prefix.
    ///
    /// Returns `None` if the string does not start with `que_`.
    pub fn new(id: impl Into<String>) -> Option<Self> {
        let s = id.into();
        if s.starts_with("que") {
            Some(Self(s))
        } else {
            None
        }
    }

    /// Create a `QuestionId` without validation (for internal use when
    /// the prefix is known to be correct).
    pub fn new_unchecked(id: impl Into<String>) -> Self {
        Self(id.into())
    }

    /// Generate an ascending question ID.
    ///
    /// Uses the same format as the TS `Identifier.ascending("question", id)`.
    pub fn ascending(seed: Option<&str>) -> Self {
        // Generate a format-matching ascending ID: question_ + hex timestamp + random hex
        let id = if let Some(s) = seed {
            if s.starts_with("que") {
                s.to_string()
            } else {
                format!("que_{s}")
            }
        } else {
            // Generate a unique ascending ID using timestamp + random suffix
            let timestamp = std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .map(|d| d.as_millis())
                .unwrap_or(0);
            let random: u64 = rand::random();
            format!("que_{:012x}_{:08x}", timestamp, random as u32)
        };
        Self(id)
    }

    /// Returns the inner string.
    pub fn as_str(&self) -> &str {
        &self.0
    }

    /// Returns true if this is a valid question ID (starts with `que`).
    pub fn is_valid(&self) -> bool {
        self.0.starts_with("que")
    }
}

impl std::fmt::Display for QuestionId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.0)
    }
}

impl From<QuestionId> for String {
    fn from(id: QuestionId) -> Self {
        id.0
    }
}

// ---------------------------------------------------------------------------
// Question option
// ---------------------------------------------------------------------------

/// An option the user can select to answer a question.
///
/// # Source
/// Ported from `packages/opencode/src/question/index.ts` `Option` type
/// and `packages/core/src/question.ts` `Option` type.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct QuestionOption {
    /// Display text — concise, 1-5 words.
    pub label: String,
    /// Explanation of what choosing this option means.
    pub description: String,
}

impl QuestionOption {
    /// Create a new question option.
    pub fn new(label: impl Into<String>, description: impl Into<String>) -> Self {
        Self {
            label: label.into(),
            description: description.into(),
        }
    }
}

// ---------------------------------------------------------------------------
// Question info (full question with metadata)
// ---------------------------------------------------------------------------

/// A complete question to present to the user.
///
/// # Source
/// Ported from `packages/opencode/src/question/index.ts` `Info` type
/// and `packages/core/src/question.ts` `Info` type.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct QuestionInfo {
    /// The complete question text.
    pub question: String,
    /// Very short label (max 30 chars) for the question header.
    pub header: String,
    /// Available choices for the user.
    pub options: Vec<QuestionOption>,
    /// Whether the user can select multiple options.
    #[serde(default, skip_serializing_if = "is_false")]
    pub multiple: bool,
    /// Whether the user can type a custom answer (default: true).
    #[serde(default = "default_true", skip_serializing_if = "is_true")]
    pub custom: bool,
}

fn default_true() -> bool {
    true
}

fn is_true(v: &bool) -> bool {
    *v
}

fn is_false(v: &bool) -> bool {
    !*v
}

impl QuestionInfo {
    /// Create a new question with the given text and header.
    pub fn new(question: impl Into<String>, header: impl Into<String>) -> Self {
        Self {
            question: question.into(),
            header: header.into(),
            options: Vec::new(),
            multiple: false,
            custom: true,
        }
    }

    /// Add options to the question.
    pub fn with_options(mut self, options: Vec<QuestionOption>) -> Self {
        self.options = options;
        self
    }

    /// Allow selecting multiple options.
    pub fn with_multiple(mut self, multiple: bool) -> Self {
        self.multiple = multiple;
        self
    }

    /// Allow/disable custom answer typing.
    pub fn with_custom(mut self, custom: bool) -> Self {
        self.custom = custom;
        self
    }
}

// ---------------------------------------------------------------------------
// Question prompt (simplified, LLM-facing)
// ---------------------------------------------------------------------------

/// A simplified question format used by the LLM to generate questions.
///
/// # Source
/// Ported from `packages/opencode/src/question/index.ts` `Prompt` type
/// and `packages/core/src/question.ts` `Prompt` type.
///
/// Unlike [`QuestionInfo`], `QuestionPrompt` does not include the `custom`
/// field — this matches the TS source where `Prompt = Schema.Struct(base)`
/// excludes the `custom` property.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct QuestionPrompt {
    /// The complete question text.
    pub question: String,
    /// Very short label (max 30 chars).
    pub header: String,
    /// Available choices.
    pub options: Vec<QuestionOption>,
    /// Whether the user can select multiple options.
    #[serde(default, skip_serializing_if = "is_false")]
    pub multiple: bool,
}

impl From<QuestionInfo> for QuestionPrompt {
    fn from(info: QuestionInfo) -> Self {
        Self {
            question: info.question,
            header: info.header,
            options: info.options,
            multiple: info.multiple,
        }
    }
}

// ---------------------------------------------------------------------------
// Question tool context
// ---------------------------------------------------------------------------

/// Tool context for a question — identifies which tool call spawned the
/// question request.
///
/// # Source
/// Ported from `packages/opencode/src/question/index.ts` `Tool` type
/// and `packages/core/src/question.ts` `Tool` type.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct QuestionTool {
    /// The message ID of the assistant message that triggered the tool call.
    #[serde(rename = "messageID")]
    pub message_id: String,
    /// The tool call ID.
    #[serde(rename = "callID")]
    pub call_id: String,
}

// ---------------------------------------------------------------------------
// Question request
// ---------------------------------------------------------------------------

/// A pending question request — sent to the user and awaiting a reply.
///
/// # Source
/// Ported from `packages/opencode/src/question/index.ts` `Request` type
/// and `packages/core/src/question.ts` `Request` type.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct QuestionRequest {
    /// Unique question ID (prefixed `que_`).
    pub id: QuestionId,
    /// Session ID this question belongs to.
    #[serde(rename = "sessionID")]
    pub session_id: String,
    /// The questions to ask the user.
    pub questions: Vec<QuestionInfo>,
    /// Optional tool context (if the question was triggered by a tool call).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub tool: Option<QuestionTool>,
}

// ---------------------------------------------------------------------------
// Question answer
// ---------------------------------------------------------------------------

/// An answer to a single question — an array of selected option labels.
///
/// # Source
/// Ported from `packages/opencode/src/question/index.ts` `Answer` type
/// and `packages/core/src/question.ts` `Answer` type.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct QuestionAnswer(Vec<String>);

impl QuestionAnswer {
    /// Create a new answer with the given selected labels.
    pub fn new(labels: Vec<String>) -> Self {
        Self(labels)
    }

    /// Returns the selected labels.
    pub fn labels(&self) -> &[String] {
        &self.0
    }

    /// Whether the user selected any options.
    pub fn is_empty(&self) -> bool {
        self.0.is_empty()
    }

    /// Number of selected options.
    pub fn len(&self) -> usize {
        self.0.len()
    }

    /// Join labels with a separator for display purposes.
    pub fn joined(&self, sep: &str) -> String {
        self.0.join(sep)
    }
}

impl From<Vec<String>> for QuestionAnswer {
    fn from(labels: Vec<String>) -> Self {
        Self(labels)
    }
}

impl std::ops::Deref for QuestionAnswer {
    type Target = [String];

    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

// ---------------------------------------------------------------------------
// Question reply
// ---------------------------------------------------------------------------

/// A full reply from the user, containing answers for all questions in order.
///
/// # Source
/// Ported from `packages/opencode/src/question/index.ts` `Reply` type
/// and `packages/core/src/question.ts` `Reply` type.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct QuestionReply {
    /// User answers in the same order as the questions were asked.
    /// Each answer is an array of selected labels.
    pub answers: Vec<QuestionAnswer>,
}

// ---------------------------------------------------------------------------
// Input types
// ---------------------------------------------------------------------------

/// Input for the `ask` operation — the data needed to post a question.
///
/// # Source
/// Ported from `packages/core/src/question.ts` `AskInput` interface
/// and `packages/opencode/src/question/index.ts` `ask()` parameter.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct QuestionAskInput {
    /// Session ID.
    #[serde(rename = "sessionID")]
    pub session_id: String,
    /// The questions to ask.
    pub questions: Vec<QuestionInfo>,
    /// Optional tool context.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub tool: Option<QuestionTool>,
}

/// Input for the `reply` operation.
///
/// # Source
/// Ported from `packages/core/src/question.ts` `ReplyInput` interface.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct QuestionReplyInput {
    /// The question request ID to reply to.
    #[serde(rename = "requestID")]
    pub request_id: QuestionId,
    /// The user's answers.
    pub answers: Vec<QuestionAnswer>,
}

// ---------------------------------------------------------------------------
// Events
// ---------------------------------------------------------------------------

/// Question-related event types published on the event bus.
///
/// # Source
/// Ported from:
/// - `packages/opencode/src/question/index.ts` `Event` object (lines 87–91)
/// - `packages/core/src/question.ts` `Event` object (lines 63–79)
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum QuestionEvent {
    /// A question has been asked — the UI should present it to the user.
    #[serde(rename = "question.asked")]
    Asked {
        /// The question request that was created.
        #[serde(flatten)]
        request: QuestionRequest,
    },
    /// The user has replied to a question.
    #[serde(rename = "question.replied")]
    Replied {
        /// Session ID.
        #[serde(rename = "sessionID")]
        session_id: String,
        /// The question request ID.
        #[serde(rename = "requestID")]
        request_id: QuestionId,
        /// The user's answers.
        answers: Vec<QuestionAnswer>,
    },
    /// The user has rejected/dismissed a question.
    #[serde(rename = "question.rejected")]
    Rejected {
        /// Session ID.
        #[serde(rename = "sessionID")]
        session_id: String,
        /// The question request ID.
        #[serde(rename = "requestID")]
        request_id: QuestionId,
    },
}

impl QuestionEvent {
    /// Event type string for asked.
    pub const ASKED: &str = "question.asked";
    /// Event type string for replied.
    pub const REPLIED: &str = "question.replied";
    /// Event type string for rejected.
    pub const REJECTED: &str = "question.rejected";
}

// ---------------------------------------------------------------------------
// Model output formatting
// ---------------------------------------------------------------------------

/// Format question-answer pairs for the LLM as a model output string.
///
/// # Source
/// Ported from `packages/core/src/tool/question.ts` `toModelOutput()`.
///
/// Produces a string like:
/// `User has answered your questions: "What color?"="red, blue", ...`
pub fn format_model_output(
    questions: &[QuestionPrompt],
    answers: &[QuestionAnswer],
) -> String {
    let formatted: Vec<String> = questions
        .iter()
        .enumerate()
        .map(|(i, q)| {
            let answer = answers
                .get(i)
                .map(|a| a.joined(", "))
                .filter(|s| !s.is_empty())
                .unwrap_or_else(|| "Unanswered".to_string());
            format!("\"{}\"=\"{}\"", q.question, answer)
        })
        .collect();
    format!(
        "User has answered your questions: {}. You can now continue with the user's answers in mind.",
        formatted.join(", ")
    )
}

// ---------------------------------------------------------------------------
// Question description text
// ---------------------------------------------------------------------------

/// Description of the question tool, matching the TS source.
///
/// # Source
/// Ported from `packages/core/src/tool/question.ts` `description` constant.
pub const QUESTION_TOOL_DESCRIPTION: &str = concat!(
    "Use this tool when you need to ask the user questions during execution. ",
    "This allows you to:\n",
    "1. Gather user preferences or requirements\n",
    "2. Clarify ambiguous instructions\n",
    "3. Get decisions on implementation choices as you work\n",
    "4. Offer choices to the user about what direction to take.\n\n",
    "Usage notes:\n",
    "- When `custom` is enabled (default), a \"Type your own answer\" option is added automatically; ",
    "don't include \"Other\" or catch-all options\n",
    "- Answers are returned as arrays of labels; set `multiple: true` to allow selecting more than one\n",
    "- If you recommend a specific option, make that the first option in the list and add \"(Recommended)\" ",
    "at the end of the label",
);

// ---------------------------------------------------------------------------
// Errors
// ---------------------------------------------------------------------------

/// Question was rejected/dismissed by the user.
///
/// # Source
/// Ported from `packages/opencode/src/question/index.ts` `RejectedError`
/// and `packages/core/src/question.ts` `RejectedError`.
#[derive(Debug, Error)]
#[error("the user dismissed this question")]
pub struct QuestionRejectedError;

/// Question request was not found (stale or invalid ID).
///
/// # Source
/// Ported from `packages/opencode/src/question/index.ts` `NotFoundError`
/// and `packages/core/src/question.ts` `NotFoundError`.
#[derive(Debug, Error)]
#[error("question `{request_id}` not found")]
pub struct QuestionNotFoundError {
    /// The question request ID that was not found.
    pub request_id: QuestionId,
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    // -- QuestionId ---------------------------------------------------------

    #[test]
    fn test_question_id_new_valid() {
        let id = QuestionId::new("que_abc123").expect("should parse valid ID");
        assert!(id.is_valid());
        assert_eq!(id.as_str(), "que_abc123");
    }

    #[test]
    fn test_question_id_new_invalid() {
        let id = QuestionId::new("bad_abc123");
        assert!(id.is_none());
    }

    #[test]
    fn test_question_id_display() {
        let id = QuestionId::new_unchecked("que_test");
        assert_eq!(id.to_string(), "que_test");
    }

    #[test]
    fn test_question_id_ascending_with_seed() {
        let id = QuestionId::ascending(Some("que_mytest"));
        assert!(id.as_str().starts_with("que_mytest"));
    }

    #[test]
    fn test_question_id_ascending_random() {
        let id = QuestionId::ascending(None);
        assert!(id.is_valid());
        assert!(id.as_str().starts_with("que_"));
        // The generated ID should be at least "que_" + some hex chars
        assert!(id.as_str().len() > 4);
    }

    #[test]
    fn test_question_id_roundtrip_through_string() {
        let id = QuestionId::new_unchecked("que_hello");
        let s: String = id.into();
        let id2 = QuestionId::new(s).expect("should roundtrip");
        assert_eq!(id2.as_str(), "que_hello");
    }

    // -- QuestionOption -----------------------------------------------------

    #[test]
    fn test_question_option_new() {
        let opt = QuestionOption::new("Fix all", "Apply all suggested fixes automatically");
        assert_eq!(opt.label, "Fix all");
        assert_eq!(opt.description, "Apply all suggested fixes automatically");
    }

    #[test]
    fn test_option_serialization() {
        let opt = QuestionOption::new("Yes", "Proceed with the change");
        let json = serde_json::to_string(&opt).expect("serialize");
        let deser: QuestionOption = serde_json::from_str(&json).expect("deserialize");
        assert_eq!(deser.label, "Yes");
        assert_eq!(deser.description, "Proceed with the change");
    }

    // -- QuestionInfo -------------------------------------------------------

    #[test]
    fn test_question_info_defaults() {
        let info = QuestionInfo::new("What should we do?", "Action Required");
        assert_eq!(info.question, "What should we do?");
        assert_eq!(info.header, "Action Required");
        assert!(info.options.is_empty());
        assert!(!info.multiple);
        assert!(info.custom);
    }

    #[test]
    fn test_question_info_builder() {
        let info = QuestionInfo::new("Pick a color", "Color Choice")
            .with_options(vec![
                QuestionOption::new("Red", "Crimson red"),
                QuestionOption::new("Blue", "Ocean blue"),
            ])
            .with_multiple(true)
            .with_custom(false);

        assert_eq!(info.options.len(), 2);
        assert!(info.multiple);
        assert!(!info.custom);
    }

    #[test]
    fn test_question_info_serialization_skips_defaults() {
        let info = QuestionInfo::new("Test?", "Test");
        let json = serde_json::to_string(&info).expect("serialize");
        // custom = true is skipped (default), multiple = false is skipped
        assert!(!json.contains("custom"));
        assert!(!json.contains("multiple"));
    }

    // -- QuestionPrompt -----------------------------------------------------

    #[test]
    fn test_prompt_from_info() {
        let info = QuestionInfo::new("Q?", "H")
            .with_options(vec![QuestionOption::new("A", "desc")])
            .with_multiple(true);
        let prompt: QuestionPrompt = info.into();
        assert_eq!(prompt.question, "Q?");
        assert_eq!(prompt.header, "H");
        assert_eq!(prompt.options.len(), 1);
        assert!(prompt.multiple);
        // Prompt should NOT have `custom` field
        let json = serde_json::to_string(&prompt).expect("serialize");
        assert!(!json.contains("custom"));
    }

    // -- QuestionAnswer -----------------------------------------------------

    #[test]
    fn test_answer_new() {
        let answer = QuestionAnswer::new(vec!["Red".into(), "Blue".into()]);
        assert_eq!(answer.len(), 2);
        assert!(!answer.is_empty());
        assert_eq!(answer.labels(), &["Red", "Blue"]);
    }

    #[test]
    fn test_answer_empty() {
        let answer = QuestionAnswer::new(vec![]);
        assert!(answer.is_empty());
        assert_eq!(answer.len(), 0);
    }

    #[test]
    fn test_answer_joined() {
        let answer = QuestionAnswer::new(vec!["A".into(), "B".into()]);
        assert_eq!(answer.joined(", "), "A, B");
    }

    #[test]
    fn test_answer_from_vec() {
        let answer: QuestionAnswer = vec!["x".to_string(), "y".to_string()].into();
        assert_eq!(answer.labels(), &["x", "y"]);
    }

    // -- QuestionReply ------------------------------------------------------

    #[test]
    fn test_reply() {
        let reply = QuestionReply {
            answers: vec![
                QuestionAnswer::new(vec!["Red".into()]),
                QuestionAnswer::new(vec!["Blue".into(), "Green".into()]),
            ],
        };
        assert_eq!(reply.answers.len(), 2);
        assert_eq!(reply.answers[0].joined(", "), "Red");
        assert_eq!(reply.answers[1].joined(", "), "Blue, Green");
    }

    // -- QuestionEvent ------------------------------------------------------

    #[test]
    fn test_event_asked_serialization() {
        let event = QuestionEvent::Asked {
            request: QuestionRequest {
                id: QuestionId::new_unchecked("que_001"),
                session_id: "ses_abc".into(),
                questions: vec![QuestionInfo::new("Test?", "Test")],
                tool: None,
            },
        };
        let json = serde_json::to_string(&event).expect("serialize");
        assert!(json.contains("question.asked"));
        assert!(json.contains("que_001"));
        assert!(json.contains("ses_abc"));
    }

    #[test]
    fn test_event_rejected_serialization() {
        let event = QuestionEvent::Rejected {
            session_id: "ses_xyz".into(),
            request_id: QuestionId::new_unchecked("que_002"),
        };
        let json = serde_json::to_string(&event).expect("serialize");
        assert!(json.contains("question.rejected"));
        assert!(json.contains("ses_xyz"));
        assert!(json.contains("que_002"));
    }

    #[test]
    fn test_event_constants() {
        assert_eq!(QuestionEvent::ASKED, "question.asked");
        assert_eq!(QuestionEvent::REPLIED, "question.replied");
        assert_eq!(QuestionEvent::REJECTED, "question.rejected");
    }

    // -- format_model_output ------------------------------------------------

    #[test]
    fn test_format_model_output() {
        let questions = vec![QuestionPrompt {
            question: "What color?".into(),
            header: "Color".into(),
            options: vec![],
            multiple: false,
        }];
        let answers = vec![QuestionAnswer::new(vec!["Blue".into()])];
        let output = format_model_output(&questions, &answers);
        assert!(output.contains("\"What color?\"=\"Blue\""));
        assert!(output.contains("User has answered your questions"));
    }

    #[test]
    fn test_format_model_output_unanswered() {
        let questions = vec![QuestionPrompt {
            question: "What?".into(),
            header: "H".into(),
            options: vec![],
            multiple: false,
        }];
        let answers: Vec<QuestionAnswer> = vec![];
        let output = format_model_output(&questions, &answers);
        assert!(output.contains("Unanswered"));
    }

    #[test]
    fn test_format_model_output_empty_answer() {
        let questions = vec![QuestionPrompt {
            question: "Q?".into(),
            header: "H".into(),
            options: vec![],
            multiple: false,
        }];
        let answers = vec![QuestionAnswer::new(vec![])];
        let output = format_model_output(&questions, &answers);
        // Empty answer vector should become "Unanswered" since joined("") is empty
        assert!(output.contains("Unanswered"));
    }

    // -- QuestionTool -------------------------------------------------------

    #[test]
    fn test_tool_serialization() {
        let tool = QuestionTool {
            message_id: "msg_001".into(),
            call_id: "call_abc".into(),
        };
        let json = serde_json::to_string(&tool).expect("serialize");
        assert!(json.contains("messageID"));
        assert!(json.contains("callID"));
        let deser: QuestionTool = serde_json::from_str(&json).expect("deserialize");
        assert_eq!(deser.message_id, "msg_001");
        assert_eq!(deser.call_id, "call_abc");
    }

    // -- QuestionRequest ----------------------------------------------------

    #[test]
    fn test_request_serialization() {
        let request = QuestionRequest {
            id: QuestionId::new_unchecked("que_req"),
            session_id: "ses_1".into(),
            questions: vec![QuestionInfo::new("Q", "H")],
            tool: Some(QuestionTool {
                message_id: "msg_1".into(),
                call_id: "call_1".into(),
            }),
        };
        let json = serde_json::to_string(&request).expect("serialize");
        assert!(json.contains("sessionID"));
        assert!(json.contains("messageID"));
        assert!(json.contains("callID"));
    }

    // -- Errors -------------------------------------------------------------

    #[test]
    fn test_rejected_error() {
        let err = QuestionRejectedError;
        assert_eq!(err.to_string(), "the user dismissed this question");
    }

    #[test]
    fn test_not_found_error() {
        let err = QuestionNotFoundError {
            request_id: QuestionId::new_unchecked("que_missing"),
        };
        assert!(err.to_string().contains("que_missing"));
        assert!(err.to_string().contains("not found"));
    }

    // -- Description constant -----------------------------------------------

    #[test]
    fn test_tool_description_is_set() {
        assert!(QUESTION_TOOL_DESCRIPTION.contains("ask the user questions"));
        assert!(QUESTION_TOOL_DESCRIPTION.contains("Usage notes"));
    }
}
