//! OpenRouter provider — OpenAI Chat with extra OpenRouter fields.
//!
//! Ported from: `packages/llm/src/providers/openrouter.ts` (98 lines)

use async_trait::async_trait;
use futures::StreamExt;
use std::collections::{HashMap, VecDeque};
use std::pin::Pin;

use crate::error::{Error, LlmErrorReason};
use crate::provider::{
    ChatMessage, ContentPart, FinishReason, LlmEvent, MessageContent, Model, Provider,
    ToolDefinition, Usage,
};
use crate::tool_stream::ToolStreamAccumulator;

const DEFAULT_BASE_URL: &str = "https://openrouter.ai/api/v1";

pub struct OpenRouterProvider {
    api_key: String,
    base_url: String,
    http_client: reqwest::Client,
    models: Vec<Model>,
}

impl OpenRouterProvider {
    pub fn new() -> Result<Self, Error> {
        let api_key = std::env::var("OPENROUTER_API_KEY")
            .ok()
            .filter(|k| !k.is_empty())
            .ok_or_else(|| Error::Auth("OPENROUTER_API_KEY not set".into()))?;
        Self::with_key(api_key, DEFAULT_BASE_URL.into())
    }

    pub fn with_key(api_key: String, base_url: String) -> Result<Self, Error> {
        let client = reqwest::Client::builder()
            .user_agent(format!("rustcode/{}", env!("CARGO_PKG_VERSION")))
            .timeout(std::time::Duration::from_secs(300))
            .build()
            .map_err(|e| Error::Network(format!("HTTP: {e}")))?;
        Ok(Self {
            api_key,
            base_url,
            http_client: client,
            models: vec![
                mk("openai/gpt-5.2", "GPT-5.2", "gpt"),
                mk("anthropic/claude-sonnet-4-6", "Claude Sonnet 4.6", "claude"),
                mk("google/gemini-3.0-pro", "Gemini 3.0 Pro", "gemini"),
                mk("meta-llama/llama-4-maverick", "Llama 4 Maverick", "llama"),
            ],
        })
    }

    fn chat_url(&self) -> String {
        format!("{}/chat/completions", self.base_url.trim_end_matches('/'))
    }

    fn build_body(
        &self,
        model: &Model,
        messages: &[ChatMessage],
        tools: &[ToolDefinition],
    ) -> serde_json::Value {
        let msgs: Vec<serde_json::Value> = messages.iter().map(|m| match m {
            ChatMessage::System { content } => serde_json::json!({"role":"system","content":text_of(content)}),
            ChatMessage::User { content } => serde_json::json!({"role":"user","content":text_of(content)}),
            ChatMessage::Assistant { content } => {
                let t = text_of(content);
                serde_json::json!({"role":"assistant","content":if t.is_empty(){serde_json::Value::Null}else{serde_json::Value::String(t)}})
            }
            ChatMessage::Tool { content } => {
                if let Some(p) = content.first() {
                    let crate::provider::ToolResultPart::ToolResult { tool_call_id, output, .. } = p;
                    serde_json::json!({"role":"tool","tool_call_id":tool_call_id,"content":output.to_string()})
                } else { serde_json::json!({"role":"tool","tool_call_id":"","content":""}) }
            }
        }).collect();

        let tools_arr: Vec<serde_json::Value> = tools.iter().map(|t| serde_json::json!({
            "type":"function","function":{"name":t.name,"description":t.description,"parameters":t.parameters}
        })).collect();

        let mut body = serde_json::json!({
            "model": model.api.id, "messages": msgs, "stream": true,
            "stream_options": {"include_usage": true},
            "max_tokens": crate::provider::max_output_tokens(model, crate::provider::OUTPUT_TOKEN_MAX),
        });
        if !tools_arr.is_empty() {
            body["tools"] = serde_json::Value::Array(tools_arr);
        }
        body
    }
}

fn text_of(c: &MessageContent) -> String {
    match c {
        MessageContent::Text(t) => t.clone(),
        MessageContent::Parts(p) => p
            .iter()
            .filter_map(|p| {
                if let ContentPart::Text { text } = p {
                    Some(text.as_str())
                } else {
                    None
                }
            })
            .collect::<Vec<_>>()
            .join(""),
    }
}

fn mk(id: &str, name: &str, family: &str) -> Model {
    Model {
        id: id.into(),
        provider_id: "openrouter".into(),
        name: name.into(),
        api: crate::provider::ApiInfo {
            id: id.into(),
            url: DEFAULT_BASE_URL.into(),
            npm: "@openrouter/ai-sdk-provider".into(),
        },
        family: Some(family.into()),
        capabilities: crate::provider::Capabilities {
            temperature: true,
            reasoning: true,
            attachment: true,
            toolcall: true,
            ..Default::default()
        },
        cost: Default::default(),
        limit: crate::provider::TokenLimit {
            context: 200_000,
            input: None,
            output: 16_384,
        },
        status: crate::provider::ModelStatus::Active,
        options: HashMap::new(),
        headers: HashMap::new(),
        release_date: "2026".into(),
        variants: None,
    }
}

// ── SSE Event Types ─────────────────────────────────────────────────────

#[derive(Debug, serde::Deserialize)]
struct OpenRouterChatEvent {
    choices: Vec<OpenRouterChoice>,
    #[serde(default)]
    usage: Option<OpenRouterUsage>,
}

#[derive(Debug, serde::Deserialize)]
struct OpenRouterChoice {
    delta: Option<OpenRouterDelta>,
    finish_reason: Option<String>,
}

#[derive(Debug, serde::Deserialize)]
struct OpenRouterDelta {
    content: Option<String>,
    reasoning_content: Option<String>,
    tool_calls: Option<Vec<OpenRouterToolCallDelta>>,
}

#[derive(Debug, serde::Deserialize)]
struct OpenRouterToolCallDelta {
    index: u64,
    id: Option<String>,
    function: Option<OpenRouterToolCallDeltaFn>,
}

#[derive(Debug, serde::Deserialize)]
struct OpenRouterToolCallDeltaFn {
    name: Option<String>,
    arguments: Option<String>,
}

#[derive(Debug, serde::Deserialize)]
struct OpenRouterUsage {
    prompt_tokens: Option<u64>,
    completion_tokens: Option<u64>,
    total_tokens: Option<u64>,
    prompt_tokens_details: Option<OpenRouterPromptTokenDetails>,
    completion_tokens_details: Option<OpenRouterCompletionTokenDetails>,
}

#[derive(Debug, serde::Deserialize)]
struct OpenRouterPromptTokenDetails {
    cached_tokens: Option<u64>,
}

#[derive(Debug, serde::Deserialize)]
struct OpenRouterCompletionTokenDetails {
    reasoning_tokens: Option<u64>,
}

// ── Event Mapping ──────────────────────────────────────────────────────

struct OpenRouterStreamState {
    tool_stream: ToolStreamAccumulator,
    text_started: bool,
    reasoning_started: bool,
    step_started: bool,
    usage: Option<Usage>,
    finished: bool,
}

fn events_from_chat(event: OpenRouterChatEvent, state: &mut OpenRouterStreamState) -> Vec<LlmEvent> {
    let mut events = Vec::new();
    let usage = event.usage.as_ref().map(map_usage).or(state.usage.clone());
    let choice = event.choices.first();

    if let Some(delta) = choice.and_then(|c| c.delta.as_ref()) {
        if let Some(ref rc) = delta.reasoning_content {
            if !rc.is_empty() {
                if !state.reasoning_started {
                    state.reasoning_started = true;
                    events.push(LlmEvent::ReasoningStart {
                        id: "reasoning-0".into(),
                        provider_metadata: None,
                    });
                }
                events.push(LlmEvent::ReasoningDelta {
                    id: "reasoning-0".into(),
                    text: rc.clone(),
                    provider_metadata: None,
                });
            }
        }

        if let Some(ref content) = delta.content {
            if !content.is_empty() {
                if !state.text_started {
                    state.text_started = true;
                    events.push(LlmEvent::TextStart {
                        id: "text-0".into(),
                        provider_metadata: None,
                    });
                }
                events.push(LlmEvent::TextDelta {
                    id: "text-0".into(),
                    text: content.clone(),
                    provider_metadata: None,
                });
            }
        }

        if let Some(tool_deltas) = &delta.tool_calls {
            for td in tool_deltas {
                if let Some(name) = td.function.as_ref().and_then(|f| f.name.as_ref()) {
                    state.tool_stream.set_identity(
                        td.index,
                        name,
                        td.id.clone().unwrap_or_default(),
                    );
                }
                if let Some(args) = td.function.as_ref().and_then(|f| f.arguments.as_ref()) {
                    if let Some(ev) = state.tool_stream.append(td.index, args) {
                        if !state.step_started {
                            events.push(LlmEvent::StepStart { index: 0 });
                            state.step_started = true;
                        }
                        events.push(ev);
                    }
                }
            }
        }
    }

    if let Some(finish_reason) = choice.and_then(|c| c.finish_reason.as_ref()) {
        for tool_ev in state.tool_stream.finish_all() {
            events.push(tool_ev);
        }

        let reason = map_finish_reason(finish_reason);

        if state.text_started {
            events.push(LlmEvent::TextEnd {
                id: "text-0".into(),
                provider_metadata: None,
            });
        }
        if state.reasoning_started {
            events.push(LlmEvent::ReasoningEnd {
                id: "reasoning-0".into(),
                provider_metadata: None,
            });
        }

        events.push(LlmEvent::StepFinish {
            index: 0,
            reason: reason.clone(),
            usage: usage.clone(),
            provider_metadata: None,
        });
        events.push(LlmEvent::Finish {
            reason,
            usage: usage.clone(),
            provider_metadata: None,
        });
        state.finished = true;
    }

    state.usage = usage;
    events
}

fn map_finish_reason(reason: &str) -> FinishReason {
    match reason {
        "stop" => FinishReason::Stop,
        "length" => FinishReason::Length,
        "content_filter" => FinishReason::ContentFilter,
        "function_call" | "tool_calls" => FinishReason::ToolCalls,
        _ => FinishReason::Unknown,
    }
}

fn map_usage(u: &OpenRouterUsage) -> Usage {
    let cached = u
        .prompt_tokens_details
        .as_ref()
        .and_then(|d| d.cached_tokens);
    let reasoning = u
        .completion_tokens_details
        .as_ref()
        .and_then(|d| d.reasoning_tokens);
    let non_cached = u
        .prompt_tokens
        .map(|p| p.saturating_sub(cached.unwrap_or(0)));

    Usage {
        input_tokens: u.prompt_tokens,
        output_tokens: u.completion_tokens,
        non_cached_input_tokens: non_cached,
        cache_read_input_tokens: cached,
        cache_write_input_tokens: None,
        reasoning_tokens: reasoning,
        total_tokens: u.total_tokens,
        provider_metadata: None,
    }
}

fn classify_error(status: u16, body: &str) -> LlmErrorReason {
    let msg = || body.to_string();
    match status {
        401 | 403 => LlmErrorReason::Authentication {
            message: msg(),
            kind: crate::error::AuthErrorKind::Invalid,
        },
        429 => LlmErrorReason::RateLimit {
            message: msg(),
            retry_after_ms: None,
        },
        400 | 413 => {
            if crate::error::is_context_overflow(body) {
                LlmErrorReason::InvalidRequest {
                    message: msg(),
                    parameter: None,
                    classification: Some("context-overflow".into()),
                }
            } else {
                LlmErrorReason::InvalidRequest {
                    message: msg(),
                    parameter: None,
                    classification: None,
                }
            }
        }
        500..=599 => LlmErrorReason::ProviderInternal {
            message: msg(),
            status,
            retry_after_ms: None,
        },
        _ => LlmErrorReason::UnknownProvider {
            message: msg(),
            status: Some(status),
        },
    }
}

// ── Provider impl ──────────────────────────────────────────────────────

#[async_trait]
impl Provider for OpenRouterProvider {
    fn provider_id(&self) -> &str {
        "openrouter"
    }
    fn npm(&self) -> &str {
        "@openrouter/ai-sdk-provider"
    }
    async fn list_models(&self) -> crate::error::Result<Vec<Model>> {
        Ok(self.models.clone())
    }
    async fn get_model(&self, id: &str) -> crate::error::Result<Model> {
        self.models
            .iter()
            .find(|m| m.id == id)
            .cloned()
            .ok_or_else(|| Error::ModelNotFound {
                provider_id: "openrouter".into(),
                model_id: id.into(),
            })
    }

    async fn stream(
        &self,
        model: &Model,
        messages: &[ChatMessage],
        tools: &[ToolDefinition],
    ) -> crate::error::Result<
        Box<dyn futures::Stream<Item = crate::error::Result<LlmEvent>> + Send + Unpin>,
    > {
        let body = self.build_body(model, messages, tools);
        let resp = self
            .http_client
            .post(self.chat_url())
            .header("Authorization", format!("Bearer {}", self.api_key))
            .header("Content-Type", "application/json")
            .header("HTTP-Referer", "https://github.com/sinescode/rustcode")
            .json(&body)
            .send()
            .await
            .map_err(|e| Error::Network(format!("OpenRouter: {e}")))?;

        if !resp.status().is_success() {
            let status = resp.status().as_u16();
            let text = resp.text().await.unwrap_or_default();
            return Err(Error::Llm {
                module: "openrouter".into(),
                method: "stream".into(),
                reason: Box::new(classify_error(status, &text)),
            });
        }

        let sse_stream = crate::sse::parse_sse_stream(resp);
        let state = OpenRouterStreamState {
            tool_stream: ToolStreamAccumulator::new(),
            text_started: false,
            reasoning_started: false,
            step_started: false,
            usage: None,
            finished: false,
        };

        let llm_stream = futures::stream::unfold(
            (
                Box::pin(sse_stream)
                    as Pin<
                        Box<
                            dyn futures::Stream<
                                    Item = Result<crate::sse::SseEvent, crate::sse::SseError>,
                                > + Send
                                + Unpin,
                        >,
                    >,
                state,
                VecDeque::new(),
            ),
            |(mut sse, mut state, mut buffer)| {
                Box::pin(async move {
                    loop {
                        if let Some(ev) = buffer.pop_front() {
                            return Some((ev, (sse, state, buffer)));
                        }
                        if state.finished {
                            return None;
                        }
                        match sse.next().await {
                            Some(Ok(se)) if !se.is_done() && se.has_data() => {
                                if let Ok(event) =
                                    serde_json::from_str::<OpenRouterChatEvent>(&se.data)
                                {
                                    for ev in events_from_chat(event, &mut state) {
                                        buffer.push_back(Ok(ev));
                                    }
                                    if let Some(ev) = buffer.pop_front() {
                                        return Some((ev, (sse, state, buffer)));
                                    }
                                }
                            }
                            Some(Err(e)) => {
                                return Some((
                                    Err(Error::ResponseStream(format!("OpenRouter SSE: {e}"))),
                                    (sse, state, buffer),
                                ))
                            }
                            None => return None,
                            _ => continue,
                        }
                    }
                })
            },
        );
        Ok(Box::new(llm_stream))
    }

    async fn complete(
        &self,
        model: &Model,
        messages: &[ChatMessage],
        tools: &[ToolDefinition],
    ) -> crate::error::Result<crate::provider::LlmResponse> {
        let mut stream = self.stream(model, messages, tools).await?;
        let mut events = Vec::new();
        let mut usage = None;
        while let Some(r) = stream.next().await {
            if let Ok(ev) = r {
                if let Some(u) = ev.usage() {
                    usage = Some(u.clone());
                }
                events.push(ev);
            }
        }
        Ok(crate::provider::LlmResponse { events, usage })
    }
}

// ── Tests ──────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_map_finish_reason() {
        assert_eq!(map_finish_reason("stop"), FinishReason::Stop);
        assert_eq!(map_finish_reason("length"), FinishReason::Length);
        assert_eq!(map_finish_reason("tool_calls"), FinishReason::ToolCalls);
        assert_eq!(map_finish_reason("content_filter"), FinishReason::ContentFilter);
        assert_eq!(map_finish_reason("unknown"), FinishReason::Unknown);
    }

    #[test]
    fn test_classify_error() {
        let reason = classify_error(401, "bad key");
        assert!(matches!(reason, LlmErrorReason::Authentication { .. }));

        let reason = classify_error(429, "rate limit");
        assert!(matches!(reason, LlmErrorReason::RateLimit { .. }));

        let reason = classify_error(500, "internal");
        assert!(matches!(reason, LlmErrorReason::ProviderInternal { .. }));
    }

    #[test]
    fn test_model_catalog() {
        let models = OpenRouterProvider::with_key("test-key".into(), DEFAULT_BASE_URL.into())
            .unwrap()
            .models;
        assert!(models.len() >= 4);
        let gpt = models.iter().find(|m| m.id == "openai/gpt-5.2").unwrap();
        assert_eq!(gpt.provider_id, "openrouter");
        assert!(gpt.capabilities.toolcall);

        let claude = models.iter().find(|m| m.id == "anthropic/claude-sonnet-4-6").unwrap();
        assert_eq!(claude.family.as_deref(), Some("claude"));
        assert_eq!(claude.limit.context, 200_000);
    }

    #[test]
    fn test_map_usage() {
        let u = OpenRouterUsage {
            prompt_tokens: Some(100),
            completion_tokens: Some(50),
            total_tokens: Some(150),
            prompt_tokens_details: Some(OpenRouterPromptTokenDetails {
                cached_tokens: Some(20),
            }),
            completion_tokens_details: Some(OpenRouterCompletionTokenDetails {
                reasoning_tokens: Some(10),
            }),
        };
        let usage = map_usage(&u);
        assert_eq!(usage.input_tokens, Some(100));
        assert_eq!(usage.output_tokens, Some(50));
        assert_eq!(usage.total_tokens, Some(150));
        assert_eq!(usage.cache_read_input_tokens, Some(20));
        assert_eq!(usage.non_cached_input_tokens, Some(80));
        assert_eq!(usage.reasoning_tokens, Some(10));
    }

    #[test]
    fn test_text_of() {
        let content = MessageContent::Text("hello world".into());
        assert_eq!(text_of(&content), "hello world");

        let content = MessageContent::Parts(vec![
            ContentPart::Text { text: "hello ".into() },
            ContentPart::Text { text: "world".into() },
        ]);
        assert_eq!(text_of(&content), "hello world");
    }

    #[test]
    fn test_events_finish() {
        let mut state = OpenRouterStreamState {
            tool_stream: ToolStreamAccumulator::new(),
            text_started: false,
            reasoning_started: false,
            step_started: false,
            usage: None,
            finished: false,
        };
        let event = OpenRouterChatEvent {
            choices: vec![OpenRouterChoice {
                delta: None,
                finish_reason: Some("stop".into()),
            }],
            usage: None,
        };
        let events = events_from_chat(event, &mut state);
        assert!(events.iter().any(|e| matches!(e, LlmEvent::Finish { reason, .. } if *reason == FinishReason::Stop)));
        assert!(state.finished);
    }
}
