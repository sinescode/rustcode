//! Session runner — integrates prompt building, provider resolution,
//! and the **multi-turn tool-execution loop** into a single callable entry point.
//!
//! This is the glue that makes `rustcode run "prompt"` a full agentic invocation
//! with tools.  The runner loops:  stream LLM → collect tool calls → execute →
//! feed results back → repeat until the LLM is done (or max iterations / doom-loop).
//!
//! Ported from:
//! - `packages/opencode/src/session/prompt.ts` (1594-1723 lines)
//! - `packages/opencode/src/session/index.ts`
//! - `packages/core/src/session/runner/index.ts`
//! - `packages/opencode/src/session/processor.ts` (tool-execution loop)

use std::collections::HashMap;
use std::sync::Arc;

use crate::error::Error;
use crate::provider::{
    ChatMessage, ContentPart, LlmEvent, MessageContent, Model, Provider, ToolDefinition,
    ToolResultPart,
};
use crate::session_prompt::{PromptPart, SessionPromptBuilder, SessionPromptInput};
use crate::tool::{ToolContext, ToolRegistry};

/// Default maximum number of LLM-tool round-trips before we abort (doom-loop guard).
const DEFAULT_MAX_ITERATIONS: usize = 25;

/// How many identical (tool, input) calls before we consider it a doom-loop.
const DOOM_LOOP_THRESHOLD: usize = 3;

// ══════════════════════════════════════════════════════════════════════════
// Public types
// ══════════════════════════════════════════════════════════════════════════

/// Record of a single tool call made during a session run.
#[derive(Debug, Clone)]
pub struct ToolCallRecord {
    /// Tool name (e.g. "bash", "read")
    pub name: String,
    /// Input arguments as received from the LLM
    pub input: serde_json::Value,
    /// Whether the tool execution succeeded
    pub success: bool,
    /// Error message if execution failed
    pub error: Option<String>,
}

/// Result of running a session prompt.
#[derive(Debug)]
pub struct SessionRunResult {
    /// The assistant's text response (concatenated from deltas)
    pub text: String,
    /// All LLM events that occurred during the run (across all iterations)
    pub events: Vec<LlmEvent>,
    /// Whether the run completed successfully
    pub success: bool,
    /// Tool calls that were executed
    pub tool_calls: Vec<ToolCallRecord>,
    /// Number of LLM iterations (stream calls) used
    pub iterations: usize,
    /// Error message if the run was aborted or failed
    pub error: Option<String>,
}

/// Pending tool call accumulated during a single stream iteration.
#[derive(Debug, Clone)]
struct PendingToolCall {
    call_id: String,
    name: String,
    input: serde_json::Value,
}

// ══════════════════════════════════════════════════════════════════════════
// SessionRunner
// ══════════════════════════════════════════════════════════════════════════

/// Session runner — wires together prompt building, provider resolution,
/// and the **multi-turn tool-execution loop**.
///
/// This is the main entry point for executing a user prompt against an LLM
/// with full tool support.
pub struct SessionRunner {
    /// Tool registry for tool definitions and execution
    tool_registry: Arc<ToolRegistry>,
    /// Maximum number of LLM→tool round-trips (doom-loop guard)
    max_iterations: usize,
}

impl SessionRunner {
    /// Create a new session runner.
    pub fn new(tool_registry: Arc<ToolRegistry>) -> Self {
        Self {
            tool_registry,
            max_iterations: DEFAULT_MAX_ITERATIONS,
        }
    }

    /// Create a new session runner with a custom max-iterations cap.
    pub fn with_max_iterations(tool_registry: Arc<ToolRegistry>, max_iterations: usize) -> Self {
        Self {
            tool_registry,
            max_iterations,
        }
    }

    /// Return the configured maximum number of LLM→tool iterations.
    pub fn max_iterations(&self) -> usize {
        self.max_iterations
    }

    /// Run a session prompt with the given provider and model.
    ///
    /// This orchestrates the full **multi-turn tool-execution** pipeline.
    /// Builds messages from the input, then runs the streaming tool loop.
    pub async fn run(
        &self,
        provider: &dyn Provider,
        model: &Model,
        input: &SessionPromptInput,
        instructions: &[String],
    ) -> Result<SessionRunResult, Error> {
        let system_prompt = self.build_system_prompt(instructions);
        let tool_defs = self.tool_registry.to_definitions();
        let mut messages = build_chat_messages(input, &system_prompt).await?;
        let input_clone = input.clone();
        self.run_loop(provider, model, &mut messages, &tool_defs, &input_clone).await
    }

    /// Run the tool loop starting from pre-built messages.
    ///
    /// Useful for interactive / conversational mode where the caller maintains
    /// message history across multiple user inputs.  The system prompt and any
    /// prior turns must already be in `messages`.
    pub async fn run_with_messages(
        &self,
        provider: &dyn Provider,
        model: &Model,
        messages: &mut Vec<ChatMessage>,
    ) -> Result<SessionRunResult, Error> {
        let tool_defs = self.tool_registry.to_definitions();
        let dummy_input = SessionPromptInput {
            session_id: String::new(),
            message_id: None,
            model: None,
            agent: None,
            no_reply: false,
            tools: None,
            format: None,
            system: None,
            variant: None,
            parts: vec![],
        };
        self.run_loop(provider, model, messages, &tool_defs, &dummy_input).await
    }

    /// Build the system prompt from instructions + tool descriptions.
    pub fn build_system_prompt(&self, instructions: &[String]) -> String {
        let mut prompt_builder = SessionPromptBuilder::new();
        for instr in instructions {
            prompt_builder.add_instruction(instr);
        }
        let tool_info_briefs = self.tool_registry.list_tools_info();
        let tool_descriptions: HashMap<String, String> = tool_info_briefs
            .into_iter()
            .map(|t| (t.id, t.description))
            .collect();
        prompt_builder.assemble_tool_descriptions(&tool_descriptions);
        prompt_builder.build_system_prompt()
    }

    /// Core multi-turn streaming tool loop.
    ///
    /// Repeatedly: stream from provider → collect tool calls → execute tools →
    /// feed results back → repeat until the LLM is done (or limits hit).
    async fn run_loop(
        &self,
        provider: &dyn Provider,
        model: &Model,
        messages: &mut Vec<ChatMessage>,
        tool_defs: &[ToolDefinition],
        input: &SessionPromptInput,
    ) -> Result<SessionRunResult, Error> {
        let mut final_text = String::new();
        let mut all_events: Vec<LlmEvent> = Vec::new();
        let mut tool_calls_made: Vec<ToolCallRecord> = Vec::new();
        let mut iterations: usize = 0;
        let mut aborted = false;
        let mut abort_reason: Option<String> = None;

        loop {
            iterations += 1;

            if iterations > self.max_iterations {
                aborted = true;
                abort_reason = Some(format!(
                    "exceeded max iterations ({})",
                    self.max_iterations
                ));
                break;
            }

            if let Some((tool, count)) = detect_doom_loop(&tool_calls_made) {
                aborted = true;
                abort_reason =
                    Some(format!("doom loop: tool '{tool}' called {count}x with same input"));
                break;
            }

            use futures::StreamExt;
            let mut stream = match provider.stream(model, messages, tool_defs).await {
                Ok(s) => s,
                Err(e) => {
                    let msg = e.to_string();
                    if is_context_overflow(&msg) {
                        abort_reason = Some("context overflow during stream".to_string());
                    }
                    return Err(e);
                }
            };

            let mut step_text = String::new();
            let mut pending_tool_calls: HashMap<String, PendingToolCall> = HashMap::new();
            let mut has_tool_calls = false;
            let mut stream_error: Option<String> = None;

            while let Some(result) = stream.next().await {
                match result {
                    Ok(event) => {
                        if let LlmEvent::TextDelta { text: ref delta, .. } = &event {
                            step_text.push_str(delta);
                            final_text.push_str(delta);
                        }
                        if let LlmEvent::ToolCall { ref id, ref name, ref input, .. } = &event {
                            has_tool_calls = true;
                            pending_tool_calls.insert(
                                id.clone(),
                                PendingToolCall {
                                    call_id: id.clone(),
                                    name: name.clone(),
                                    input: input.clone(),
                                },
                            );
                        }
                        if let LlmEvent::StepFinish { ref reason, .. } = &event {
                            let _ = reason;
                        }
                        all_events.push(event);
                    }
                    Err(e) => {
                        let msg = e.to_string();
                        if is_context_overflow(&msg) {
                            abort_reason = Some("context overflow during stream".to_string());
                            aborted = true;
                            stream_error = Some(msg);
                        } else {
                            all_events.push(LlmEvent::ProviderErrorEvent {
                                message: msg.clone(),
                                classification: Some("stream-error".into()),
                                retryable: Some(false),
                                provider_metadata: None,
                            });
                            stream_error = Some(msg);
                        }
                    }
                }
                if aborted {
                    break;
                }
            }

            if let Some(err) = stream_error {
                if aborted {
                    break;
                }
                return Err(Error::Tool(err));
            }

            if !has_tool_calls {
                break;
            }

            let mut assistant_parts: Vec<ContentPart> = Vec::new();
            if !step_text.is_empty() {
                assistant_parts.push(ContentPart::Text { text: step_text.clone() });
            }
            for tc in pending_tool_calls.values() {
                assistant_parts.push(ContentPart::ToolCallPart {
                    tool_call_id: tc.call_id.clone(),
                    tool_name: tc.name.clone(),
                });
            }
            messages.push(ChatMessage::Assistant {
                content: MessageContent::Parts(assistant_parts),
            });

            let mut tool_result_parts: Vec<ToolResultPart> = Vec::new();
            for (_key, tc) in &pending_tool_calls {
                let ctx = ToolContext {
                    session_id: input.session_id.clone(),
                    message_id: String::new(),
                    agent: input.agent.clone().unwrap_or_else(|| "cli".into()),
                    abort: tokio_util::sync::CancellationToken::new(),
                    call_id: Some(tc.call_id.clone()),
                    extra: HashMap::new(),
                    messages: messages.clone(),
                };
                let result = self
                    .tool_registry
                    .execute_by_name(&tc.name, tc.input.clone(), &ctx)
                    .await;
                match result {
                    Ok(exec_result) => {
                        tool_calls_made.push(ToolCallRecord {
                            name: tc.name.clone(),
                            input: tc.input.clone(),
                            success: true,
                            error: None,
                        });
                        tool_result_parts.push(ToolResultPart::ToolResult {
                            tool_call_id: tc.call_id.clone(),
                            tool_name: tc.name.clone(),
                            output: serde_json::json!({"result": exec_result.output}),
                            is_error: false,
                        });
                    }
                    Err(e) => {
                        let err_msg = e.to_string();
                        tool_calls_made.push(ToolCallRecord {
                            name: tc.name.clone(),
                            input: tc.input.clone(),
                            success: false,
                            error: Some(err_msg.clone()),
                        });
                        tool_result_parts.push(ToolResultPart::ToolResult {
                            tool_call_id: tc.call_id.clone(),
                            tool_name: tc.name.clone(),
                            output: serde_json::json!({"error": err_msg}),
                            is_error: true,
                        });
                    }
                }
            }

            if !tool_result_parts.is_empty() {
                messages.push(ChatMessage::Tool {
                    content: tool_result_parts,
                });
            }

            if check_context_overflow(messages, model) {
                aborted = true;
                abort_reason = Some("context overflow after tool results".to_string());
                break;
            }
        }

        Ok(SessionRunResult {
            text: final_text,
            events: all_events,
            success: !aborted,
            tool_calls: tool_calls_made,
            iterations,
            error: abort_reason,
        })
    }
}

// ══════════════════════════════════════════════════════════════════════════
// Helpers
// ══════════════════════════════════════════════════════════════════════════

/// Build `ChatMessage` array from a prompt input and system prompt.
///
/// Converts the high-level `SessionPromptInput` into the canonical
/// `ChatMessage` format that providers consume.
async fn build_chat_messages(
    input: &SessionPromptInput,
    system_prompt: &str,
) -> Result<Vec<ChatMessage>, Error> {
    let mut messages: Vec<ChatMessage> = Vec::new();

    // System message (always first)
    if !system_prompt.is_empty() {
        messages.push(ChatMessage::System {
            content: MessageContent::Text(system_prompt.to_string()),
        });
    }

    // Context from the system field on the input
    if let Some(ref sys) = input.system {
        if !sys.is_empty() {
            messages.push(ChatMessage::System {
                content: MessageContent::Text(sys.clone()),
            });
        }
    }

    // User parts → user message
    let mut user_parts: Vec<ContentPart> = Vec::new();

    for part in &input.parts {
        match part {
            PromptPart::Text(text_part) => {
                user_parts.push(ContentPart::Text {
                    text: text_part.text.clone(),
                });
            }
            PromptPart::File(file_part) => {
                let filename = file_part.filename.as_deref().unwrap_or("unnamed");
                let mime = &file_part.mime;

                user_parts.push(ContentPart::Text {
                    text: format!("[Attached file: {filename} ({mime})]"),
                });

                if file_part.url.starts_with("data:") {
                    if mime.starts_with("image/") {
                        let data = if let Some(comma_pos) = file_part.url.find(',') {
                            file_part.url[comma_pos + 1..].to_string()
                        } else {
                            file_part.url.clone()
                        };
                        user_parts.push(ContentPart::Image { image: data });
                    } else {
                        user_parts.push(ContentPart::File {
                            data: file_part.url.clone(),
                            media_type: mime.clone(),
                            filename: file_part.filename.clone(),
                        });
                    }
                } else if let Some(ref source) = file_part.source {
                    if let Some(ref value) = source.value {
                        user_parts.push(ContentPart::Text {
                            text: format!("\n--- File: {filename} ---\n{value}\n--- End file ---"),
                        });
                    }
                }
            }
            PromptPart::Agent(agent_part) => {
                user_parts.push(ContentPart::Text {
                    text: format!("[Agent: {}]", agent_part.name),
                });
            }
            PromptPart::Subtask(subtask) => {
                user_parts.push(ContentPart::Text {
                    text: format!(
                        "[Subtask: {} — {}]\n{}",
                        subtask.agent, subtask.description, subtask.prompt
                    ),
                });
            }
        }
    }

    if !user_parts.is_empty() {
        if user_parts.len() == 1 {
            if let ContentPart::Text { text } = &user_parts[0] {
                messages.push(ChatMessage::User {
                    content: MessageContent::Text(text.clone()),
                });
            } else {
                messages.push(ChatMessage::User {
                    content: MessageContent::Parts(user_parts),
                });
            }
        } else {
            messages.push(ChatMessage::User {
                content: MessageContent::Parts(user_parts),
            });
        }
    }

    Ok(messages)
}

// ── Doom-loop detection ───────────────────────────────────────────────────

/// Check the most recent tool calls for a repeating pattern.
///
/// A "doom loop" is when the LLM calls the exact same tool with the exact
/// same arguments three times in a row — it is stuck.
///
/// Returns `Some((tool_name, count))` if doom loop detected.
fn detect_doom_loop(tool_calls: &[ToolCallRecord]) -> Option<(&str, usize)> {
    if tool_calls.len() < DOOM_LOOP_THRESHOLD {
        return None;
    }

    let last = &tool_calls[tool_calls.len() - 1];
    let input_str = serde_json::to_string(&last.input).unwrap_or_default();
    let mut count = 1;

    for tc in tool_calls.iter().rev().skip(1) {
        if tc.name == last.name {
            let tc_input = serde_json::to_string(&tc.input).unwrap_or_default();
            if tc_input == input_str {
                count += 1;
            } else {
                break;
            }
        } else {
            break;
        }
    }

    if count >= DOOM_LOOP_THRESHOLD {
        Some((last.name.as_str(), count))
    } else {
        None
    }
}

// ── Context-overflow detection ────────────────────────────────────────────

/// Roughly estimate whether the message list has grown beyond the model's
/// context window.
///
/// This is a best-effort heuristic — real token counting would require
/// a tokeniser per model.  We count characters as a proxy.
fn check_context_overflow(messages: &[ChatMessage], model: &Model) -> bool {
    let context_limit = model.limit.context;
    if context_limit == 0 {
        return false; // unlimited
    }

    // Very rough: 1 token ≈ 4 chars for English text
    let estimated_tokens: u64 = messages
        .iter()
        .map(|m| {
            let json = serde_json::to_string(m).unwrap_or_default();
            json.len() as u64 / 4
        })
        .sum();

    // Reserve ~20% for output
    let usable = (context_limit as f64 * 0.8) as u64;
    estimated_tokens > usable
}

/// Heuristic check: does the error message indicate a context-window overflow?
fn is_context_overflow(error_message: &str) -> bool {
    let lower = error_message.to_lowercase();
    lower.contains("context")
        || lower.contains("token limit")
        || lower.contains("too long")
        || lower.contains("maximum context")
        || lower.contains("reduce the length")
}

// ══════════════════════════════════════════════════════════════════════════
// Tests
// ══════════════════════════════════════════════════════════════════════════

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_build_chat_messages_simple_text() {
        let rt = tokio::runtime::Runtime::new().unwrap();
        rt.block_on(async {
            let input = SessionPromptInput {
                session_id: "ses_test".into(),
                message_id: None,
                model: None,
                agent: Some("build".into()),
                no_reply: false,
                tools: None,
                format: None,
                system: None,
                variant: None,
                parts: vec![PromptPart::Text(
                    crate::session_prompt::PromptTextPart {
                        id: None,
                        text: "Hello, can you help me?".into(),
                        synthetic: false,
                    },
                )],
            };

            let messages = build_chat_messages(&input, "You are helpful.")
                .await
                .unwrap();

            assert_eq!(messages.len(), 2);
            match &messages[0] {
                ChatMessage::System { content } => match content {
                    MessageContent::Text(t) => assert_eq!(t, "You are helpful."),
                    _ => panic!("Expected text system message"),
                },
                _ => panic!("Expected system message first"),
            }
            match &messages[1] {
                ChatMessage::User { content } => match content {
                    MessageContent::Text(t) => assert_eq!(t, "Hello, can you help me?"),
                    _ => panic!("Expected text user message"),
                },
                _ => panic!("Expected user message second"),
            }
        });
    }

    #[test]
    fn test_build_chat_messages_with_system_override() {
        let rt = tokio::runtime::Runtime::new().unwrap();
        rt.block_on(async {
            let input = SessionPromptInput {
                session_id: "ses_test".into(),
                message_id: None,
                model: None,
                agent: Some("build".into()),
                no_reply: false,
                tools: None,
                format: None,
                system: Some("Custom system instructions".into()),
                variant: None,
                parts: vec![],
            };

            let messages = build_chat_messages(&input, "Default system")
                .await
                .unwrap();

            assert_eq!(messages.len(), 2);
            match &messages[0] {
                ChatMessage::System { content } => match content {
                    MessageContent::Text(t) => assert_eq!(t, "Default system"),
                    _ => panic!("Expected text"),
                },
                _ => panic!("Expected system"),
            }
            match &messages[1] {
                ChatMessage::System { content } => match content {
                    MessageContent::Text(t) => assert_eq!(t, "Custom system instructions"),
                    _ => panic!("Expected text"),
                },
                _ => panic!("Expected system"),
            }
        });
    }

    #[test]
    fn test_build_chat_messages_empty() {
        let rt = tokio::runtime::Runtime::new().unwrap();
        rt.block_on(async {
            let input = SessionPromptInput {
                session_id: "ses_test".into(),
                message_id: None,
                model: None,
                agent: None,
                no_reply: false,
                tools: None,
                format: None,
                system: None,
                variant: None,
                parts: vec![],
            };

            let messages = build_chat_messages(&input, "").await.unwrap();
            assert_eq!(messages.len(), 0);
        });
    }

    // ── Doom-loop detection tests ──────────────────────────────────

    #[test]
    fn test_detect_doom_loop_no_calls() {
        let calls: Vec<ToolCallRecord> = vec![];
        assert!(detect_doom_loop(&calls).is_none());
    }

    #[test]
    fn test_detect_doom_loop_fewer_than_threshold() {
        let calls = vec![
            ToolCallRecord {
                name: "search".into(),
                input: serde_json::json!({"query": "test"}),
                success: true,
                error: None,
            },
            ToolCallRecord {
                name: "search".into(),
                input: serde_json::json!({"query": "test"}),
                success: true,
                error: None,
            },
        ];
        assert!(detect_doom_loop(&calls).is_none());
    }

    #[test]
    fn test_detect_doom_loop_exactly_threshold() {
        let calls = vec![
            ToolCallRecord {
                name: "search".into(),
                input: serde_json::json!({"query": "test"}),
                success: true,
                error: None,
            },
            ToolCallRecord {
                name: "search".into(),
                input: serde_json::json!({"query": "test"}),
                success: true,
                error: None,
            },
            ToolCallRecord {
                name: "search".into(),
                input: serde_json::json!({"query": "test"}),
                success: true,
                error: None,
            },
        ];
        let detected = detect_doom_loop(&calls);
        assert!(detected.is_some());
        let (name, count) = detected.unwrap();
        assert_eq!(name, "search");
        assert_eq!(count, 3);
    }

    #[test]
    fn test_detect_doom_loop_different_inputs_not_loop() {
        let calls = vec![
            ToolCallRecord {
                name: "search".into(),
                input: serde_json::json!({"query": "a"}),
                success: true,
                error: None,
            },
            ToolCallRecord {
                name: "search".into(),
                input: serde_json::json!({"query": "b"}),
                success: true,
                error: None,
            },
            ToolCallRecord {
                name: "search".into(),
                input: serde_json::json!({"query": "a"}),
                success: true,
                error: None,
            },
        ];
        assert!(detect_doom_loop(&calls).is_none());
    }

    #[test]
    fn test_detect_doom_loop_different_tools_not_loop() {
        let calls = vec![
            ToolCallRecord {
                name: "search".into(),
                input: serde_json::json!({"query": "test"}),
                success: true,
                error: None,
            },
            ToolCallRecord {
                name: "read".into(),
                input: serde_json::json!({"query": "test"}),
                success: true,
                error: None,
            },
            ToolCallRecord {
                name: "search".into(),
                input: serde_json::json!({"query": "test"}),
                success: true,
                error: None,
            },
        ];
        assert!(detect_doom_loop(&calls).is_none());
    }

    // ── Context-overflow tests ────────────────────────────────────

    #[test]
    fn test_is_context_overflow() {
        assert!(is_context_overflow("context window exceeded"));
        assert!(is_context_overflow("token limit reached"));
        assert!(is_context_overflow("input is too long"));
        assert!(is_context_overflow("maximum context length exceeded"));
        assert!(is_context_overflow("please reduce the length"));
        assert!(!is_context_overflow("invalid api key"));
    }
}
