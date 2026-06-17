//! Session runner — integrates prompt building, provider resolution,
//! and the session processing loop into a single callable entry point.
//!
//! This is the glue that makes `rustcode run "hello"` work end-to-end.
//!
//! Ported from:
//! - `packages/opencode/src/session/prompt.ts` (1594-1723 lines)
//! - `packages/opencode/src/session/index.ts`
//! - `packages/core/src/session/runner/index.ts`

use std::sync::Arc;

use crate::error::Error;
use crate::provider::{ChatMessage, ContentPart, LlmEvent, MessageContent, Model, Provider, ToolDefinition};
use crate::session_prompt::{PromptPart, SessionPromptBuilder, SessionPromptInput};
use crate::tool::ToolRegistry;

/// Result of running a session prompt.
#[derive(Debug)]
pub struct SessionRunResult {
    /// The assistant's text response (concatenated from deltas)
    pub text: String,
    /// All LLM events that occurred during the run
    pub events: Vec<LlmEvent>,
    /// Whether the run completed successfully
    pub success: bool,
    /// Error message if the run failed
    pub error: Option<String>,
}

/// Session runner — wires together prompt building, provider resolution,
/// and the session processing loop.
///
/// This is the main entry point for executing a user prompt against an LLM.
pub struct SessionRunner {
    /// Tool registry for tool definitions
    tool_registry: Arc<ToolRegistry>,
}

impl SessionRunner {
    /// Create a new session runner.
    pub fn new(tool_registry: Arc<ToolRegistry>) -> Self {
        Self { tool_registry }
    }

    /// Run a session prompt with the given provider and model.
    ///
    /// This orchestrates the full pipeline:
    /// 1. Build the system prompt from instructions + tool descriptions
    /// 2. Convert the prompt input + session history → `ChatMessage[]`
    /// 3. Resolve tool definitions from the tool registry
    /// 4. Call `provider.stream()` to get the LLM event stream
    /// 5. Process events through the `SessionProcessor`
    /// 6. Return the final result
    pub async fn run(
        &self,
        provider: &dyn Provider,
        model: &Model,
        input: &SessionPromptInput,
        instructions: &[String],
    ) -> Result<SessionRunResult, Error> {
        // ── 1. Build system prompt ─────────────────────────────────
        let mut prompt_builder = SessionPromptBuilder::new();
        for instr in instructions {
            prompt_builder.add_instruction(instr);
        }

        // Add tool descriptions
        let tool_defs = self.tool_registry.to_definitions();
        let tool_descriptions: std::collections::HashMap<String, String> = self
            .tool_registry
            .list_tools()
            .into_iter()
            .map(|t| (t.id, t.description))
            .collect();
        prompt_builder.assemble_tool_descriptions(&tool_descriptions);

        let system_prompt = prompt_builder.build_system_prompt();

        // ── 2. Build ChatMessage[] ─────────────────────────────────
        let chat_messages = build_chat_messages(input, &system_prompt).await?;

        // ── 3. Call Provider.stream() and collect events ──────────
        let mut stream = provider.stream(model, &chat_messages, &tool_defs).await?;

        use futures::StreamExt;
        let mut events: Vec<LlmEvent> = Vec::new();
        let mut text = String::new();
        let mut error: Option<String> = None;
        let mut success = true;

        while let Some(result) = stream.next().await {
            match result {
                Ok(event) => {
                    // Accumulate text from TextDelta events
                    if let LlmEvent::TextDelta { text: ref delta, .. } = &event {
                        text.push_str(delta);
                    }
                    events.push(event);
                }
                Err(e) => {
                    error = Some(e.to_string());
                    success = false;
                    events.push(LlmEvent::ProviderErrorEvent {
                        message: e.to_string(),
                        classification: Some("stream-error".into()),
                        retryable: Some(false),
                        provider_metadata: None,
                    });
                }
            }
        }

        Ok(SessionRunResult {
            text,
            events,
            success,
            error,
        })
    }
}

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
                // File parts: we include a text reference to the file
                // and any inline content if available
                let filename = file_part.filename.as_deref().unwrap_or("unnamed");
                let mime = &file_part.mime;

                user_parts.push(ContentPart::Text {
                    text: format!("[Attached file: {filename} ({mime})]"),
                });

                // If the file URL is a data URL, include it as an image/file part
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
                // Single text part — optimize to simple string content
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

            let messages = build_chat_messages(&input, "You are helpful.").await.unwrap();

            // Should have: system message + user message
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

            let messages = build_chat_messages(&input, "Default system").await.unwrap();

            // Should have 2 system messages: default + custom
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
            // No system prompt, no parts — should be empty
            assert_eq!(messages.len(), 0);
        });
    }
}
