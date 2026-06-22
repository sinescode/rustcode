//! Google Gemini provider (generateContent API).
//!
//! Ported from:
//! - `packages/llm/src/protocols/gemini.ts` (487 lines)
//! - `packages/llm/src/providers/google.ts` (35 lines)

use async_trait::async_trait;
use futures::{Stream, StreamExt};
use serde::{Deserialize, Serialize};
use std::collections::{HashMap, VecDeque};
use std::pin::Pin;

use crate::error::{Error, LlmErrorReason};
use crate::provider::{
    ChatMessage, ContentPart, FinishReason, LlmEvent, MessageContent, Model, Provider,
    ToolDefinition, Usage,
};
use crate::sse::parse_sse_stream;

const DEFAULT_BASE_URL: &str = "https://generativelanguage.googleapis.com/v1beta";

fn resolve_api_key() -> Result<String, Error> {
    std::env::var("GOOGLE_GENERATIVE_AI_API_KEY")
        .ok()
        .filter(|k| !k.is_empty())
        .or_else(|| {
            std::env::var("GEMINI_API_KEY")
                .ok()
                .filter(|k| !k.is_empty())
        })
        .ok_or_else(|| Error::Auth("GOOGLE_GENERATIVE_AI_API_KEY or GEMINI_API_KEY not set".into()))
}

// ── Gemini API Types ───────────────────────────────────────────────────

#[derive(Debug, Serialize)]
struct GeminiBody {
    contents: Vec<GeminiContent>,
    #[serde(skip_serializing_if = "Option::is_none")]
    system_instruction: Option<GeminiSystemInstruction>,
    #[serde(skip_serializing_if = "Option::is_none")]
    tools: Option<Vec<GeminiTool>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    tool_config: Option<GeminiToolConfig>,
    #[serde(skip_serializing_if = "Option::is_none")]
    generation_config: Option<GeminiGenerationConfig>,
}

#[derive(Debug, Serialize, Deserialize)]
struct GeminiContent {
    role: String,
    parts: Vec<GeminiPart>,
}

#[derive(Debug, Serialize, Deserialize)]
#[serde(untagged)]
enum GeminiPart {
    Text {
        text: String,
        #[serde(skip_serializing_if = "Option::is_none")]
        thought: Option<bool>,
        #[serde(skip_serializing_if = "Option::is_none")]
        thought_signature: Option<String>,
    },
    InlineData {
        inline_data: GeminiInlineData,
    },
    FunctionCall {
        function_call: GeminiFunctionCall,
        #[serde(skip_serializing_if = "Option::is_none")]
        thought_signature: Option<String>,
    },
    FunctionResponse {
        function_response: GeminiFunctionResponse,
    },
}

#[derive(Debug, Serialize, Deserialize)]
struct GeminiInlineData {
    mime_type: String,
    data: String,
}

#[derive(Debug, Serialize, Deserialize)]
struct GeminiFunctionCall {
    name: String,
    args: serde_json::Value,
}

#[derive(Debug, Serialize, Deserialize)]
struct GeminiFunctionResponse {
    name: String,
    response: serde_json::Value,
}

#[derive(Debug, Serialize)]
struct GeminiSystemInstruction {
    parts: Vec<GeminiTextPart>,
}

#[derive(Debug, Serialize)]
struct GeminiTextPart {
    text: String,
}

#[derive(Debug, Serialize)]
struct GeminiTool {
    function_declarations: Vec<GeminiFunctionDeclaration>,
}

#[derive(Debug, Serialize)]
struct GeminiFunctionDeclaration {
    name: String,
    description: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    parameters: Option<serde_json::Value>,
}

#[derive(Debug, Serialize)]
struct GeminiToolConfig {
    function_calling_config: GeminiFunctionCallingConfig,
}

#[derive(Debug, Serialize)]
struct GeminiFunctionCallingConfig {
    mode: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    allowed_function_names: Option<Vec<String>>,
}

#[derive(Debug, Serialize)]
struct GeminiGenerationConfig {
    #[serde(skip_serializing_if = "Option::is_none")]
    max_output_tokens: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    temperature: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    top_p: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    top_k: Option<u32>,
}

// ── Gemini SSE Event ───────────────────────────────────────────────────

#[derive(Debug, Deserialize)]
struct GeminiEvent {
    #[serde(default)]
    candidates: Vec<GeminiCandidate>,
    #[serde(rename = "usageMetadata")]
    usage_metadata: Option<GeminiUsage>,
}

#[derive(Debug, Deserialize)]
struct GeminiCandidate {
    content: Option<GeminiContent>,
    #[serde(rename = "finishReason")]
    finish_reason: Option<String>,
}

#[derive(Debug, Deserialize)]
struct GeminiUsage {
    #[serde(rename = "promptTokenCount")]
    prompt_token_count: Option<u64>,
    #[serde(rename = "candidatesTokenCount")]
    candidates_token_count: Option<u64>,
    #[serde(rename = "totalTokenCount")]
    total_token_count: Option<u64>,
    #[serde(rename = "cachedContentTokenCount")]
    cached_content_token_count: Option<u64>,
    #[serde(rename = "thoughtsTokenCount")]
    thoughts_token_count: Option<u64>,
}

// ── Provider ───────────────────────────────────────────────────────────

pub struct GeminiProvider {
    api_key: String,
    base_url: String,
    http_client: reqwest::Client,
    models: Vec<Model>,
}

impl GeminiProvider {
    pub fn new() -> Result<Self, Error> {
        let key = resolve_api_key()?;
        Self::with_key(key, DEFAULT_BASE_URL.into())
    }

    pub fn with_key(api_key: String, base_url: String) -> Result<Self, Error> {
        let http_client = reqwest::Client::builder()
            .user_agent(format!("blazecode/{}", env!("CARGO_PKG_VERSION")))
            .timeout(std::time::Duration::from_secs(300))
            .build()
            .map_err(|e| Error::Network(format!("HTTP client: {e}")))?;
        Ok(Self {
            api_key,
            base_url,
            http_client,
            models: build_model_catalog(),
        })
    }

    fn stream_url(&self, model_id: &str) -> String {
        format!(
            "{}/models/{}:streamGenerateContent?alt=sse",
            self.base_url.trim_end_matches('/'),
            model_id
        )
    }

    fn build_contents(messages: &[ChatMessage]) -> Vec<GeminiContent> {
        let mut contents: Vec<GeminiContent> = Vec::new();
        for msg in messages {
            match msg {
                ChatMessage::System { content } => {
                    let text = extract_text(content);
                    if text.is_empty() {
                        continue;
                    }
                    if let Some(last) = contents.last_mut() {
                        if last.role == "user" {
                            last.parts.push(GeminiPart::Text {
                                text,
                                thought: None,
                                thought_signature: None,
                            });
                            continue;
                        }
                    }
                    contents.push(GeminiContent {
                        role: "user".into(),
                        parts: vec![GeminiPart::Text {
                            text,
                            thought: None,
                            thought_signature: None,
                        }],
                    });
                }
                ChatMessage::User { content } => {
                    let mut parts = Vec::new();
                    match content {
                        MessageContent::Text(t) => parts.push(GeminiPart::Text {
                            text: t.clone(),
                            thought: None,
                            thought_signature: None,
                        }),
                        MessageContent::Parts(p) => {
                            for part in p {
                                match part {
                                    ContentPart::Text { text } => parts.push(GeminiPart::Text {
                                        text: text.clone(),
                                        thought: None,
                                        thought_signature: None,
                                    }),
                                    ContentPart::Image { image } => {
                                        let (mime, data) = parse_data_url(image);
                                        parts.push(GeminiPart::InlineData {
                                            inline_data: GeminiInlineData {
                                                mime_type: mime,
                                                data,
                                            },
                                        });
                                    }
                                    _ => {}
                                }
                            }
                        }
                    }
                    contents.push(GeminiContent {
                        role: "user".into(),
                        parts,
                    });
                }
                ChatMessage::Assistant { content } => {
                    let mut parts = Vec::new();
                    match content {
                        MessageContent::Text(t) => parts.push(GeminiPart::Text {
                            text: t.clone(),
                            thought: None,
                            thought_signature: None,
                        }),
                        MessageContent::Parts(p) => {
                            for part in p {
                                match part {
                                    ContentPart::Text { text } => parts.push(GeminiPart::Text {
                                        text: text.clone(),
                                        thought: None,
                                        thought_signature: None,
                                    }),
                                    ContentPart::Reasoning { text, .. } => {
                                        parts.push(GeminiPart::Text {
                                            text: text.clone(),
                                            thought: Some(true),
                                            thought_signature: None,
                                        })
                                    }
                                    ContentPart::ToolCallPart {
                                        tool_call_id,
                                        tool_name,
                                        arguments,
                                    } => parts.push(GeminiPart::FunctionCall {
                                        function_call: GeminiFunctionCall {
                                            name: tool_name.clone(),
                                            args: arguments.clone(),
                                        },
                                        thought_signature: None,
                                    }),
                                    _ => {}
                                }
                            }
                        }
                    }
                    contents.push(GeminiContent {
                        role: "model".into(),
                        parts,
                    });
                }
                ChatMessage::Tool { content } => {
                    let mut parts = Vec::new();
                    for part in content {
                        let crate::provider::ToolResultPart::ToolResult {
                            tool_name, output, ..
                        } = part;
                        parts.push(GeminiPart::FunctionResponse {
                            function_response: GeminiFunctionResponse {
                                name: tool_name.clone(),
                                response: output.clone(),
                            },
                        });
                    }
                    contents.push(GeminiContent {
                        role: "user".into(),
                        parts,
                    });
                }
            }
        }
        contents
    }
}

fn extract_text(content: &MessageContent) -> String {
    match content {
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

fn parse_data_url(data: &str) -> (String, String) {
    if let Some(rest) = data.strip_prefix("data:") {
        if let Some(comma) = rest.find(',') {
            let mime = rest[..comma]
                .split(';')
                .next()
                .unwrap_or("image/png")
                .to_string();
            return (mime, rest[comma + 1..].to_string());
        }
    }
    ("image/png".into(), data.to_string())
}

fn map_finish(reason: &str, has_tools: bool) -> FinishReason {
    match reason {
        "STOP" => {
            if has_tools {
                FinishReason::ToolCalls
            } else {
                FinishReason::Stop
            }
        }
        "MAX_TOKENS" => FinishReason::Length,
        "SAFETY" | "RECITATION" | "IMAGE_SAFETY" | "BLOCKLIST" | "PROHIBITED_CONTENT" | "SPII" => {
            FinishReason::ContentFilter
        }
        "MALFORMED_FUNCTION_CALL" => FinishReason::Error,
        _ => FinishReason::Unknown,
    }
}

fn map_usage(u: &GeminiUsage) -> Usage {
    let cached = u.cached_content_token_count;
    let non_cached = u
        .prompt_token_count
        .map(|p| p.saturating_sub(cached.unwrap_or(0)));
    let output = u
        .candidates_token_count
        .map(|c| c + u.thoughts_token_count.unwrap_or(0));
    Usage {
        input_tokens: u.prompt_token_count,
        output_tokens: output,
        non_cached_input_tokens: non_cached,
        cache_read_input_tokens: cached,
        cache_write_input_tokens: None,
        reasoning_tokens: u.thoughts_token_count,
        total_tokens: u.total_token_count,
        provider_metadata: None,
    }
}

struct GeminiStreamState {
    has_tool_calls: bool,
    next_tool_id: u64,
    text_started: bool,
    reasoning_started: bool,
    step_started: bool,
    reasoning_sig: Option<String>,
    usage: Option<Usage>,
    finish_reason: Option<String>,
    finished: bool,
}

fn events_from_gemini(ev: GeminiEvent, state: &mut GeminiStreamState) -> Vec<LlmEvent> {
    let mut events = Vec::new();
    if let Some(ref u) = ev.usage_metadata {
        state.usage = Some(map_usage(u));
    }
    let candidate = &ev.candidates.first();
    state.finish_reason = candidate
        .and_then(|c| c.finish_reason.clone())
        .or(state.finish_reason.clone());

    if let Some(content) = candidate.and_then(|c| c.content.as_ref()) {
        for part in &content.parts {
            match part {
                GeminiPart::Text {
                    text,
                    thought,
                    thought_signature,
                } if !text.is_empty() => {
                    if *thought == Some(true) {
                        if !state.reasoning_started {
                            state.reasoning_started = true;
                            events.push(LlmEvent::ReasoningStart {
                                id: "reasoning-0".into(),
                                provider_metadata: None,
                            });
                        }
                        events.push(LlmEvent::ReasoningDelta {
                            id: "reasoning-0".into(),
                            text: text.clone(),
                            provider_metadata: None,
                        });
                    } else {
                        if !state.step_started {
                            state.step_started = true;
                            events.push(LlmEvent::StepStart { index: 0 });
                        }
                        if !state.text_started {
                            state.text_started = true;
                            events.push(LlmEvent::TextStart {
                                id: "text-0".into(),
                                provider_metadata: None,
                            });
                        }
                        events.push(LlmEvent::TextDelta {
                            id: "text-0".into(),
                            text: text.clone(),
                            provider_metadata: None,
                        });
                    }
                }
                GeminiPart::FunctionCall { function_call, .. } => {
                    if !state.step_started {
                        state.step_started = true;
                        events.push(LlmEvent::StepStart { index: 0 });
                    }
                    let id = format!("tool_{}", state.next_tool_id);
                    state.next_tool_id += 1;
                    state.has_tool_calls = true;
                    events.push(LlmEvent::ToolCall {
                        id,
                        name: function_call.name.clone(),
                        input: function_call.args.clone(),
                        provider_executed: None,
                        provider_metadata: None,
                    });
                }
                _ => {}
            }
        }
    }

    // Emit finish on stream end (candidates empty and finishReason set)
    if candidate.is_none_or(|c| c.content.is_none())
        && state.finish_reason.is_some()
        && !state.finished
    {
        let reason = map_finish(
            state.finish_reason.as_deref().unwrap_or("STOP"),
            state.has_tool_calls,
        );
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
            usage: state.usage.clone(),
            provider_metadata: None,
        });
        events.push(LlmEvent::Finish {
            reason,
            usage: state.usage.clone(),
            provider_metadata: None,
        });
        state.finished = true;
    }
    events
}

#[async_trait]
impl Provider for GeminiProvider {
    fn provider_id(&self) -> &str {
        "google"
    }
    fn npm(&self) -> &str {
        "@ai-sdk/google"
    }

    async fn list_models(&self) -> crate::error::Result<Vec<Model>> {
        Ok(self.models.clone())
    }
    async fn get_model(&self, model_id: &str) -> crate::error::Result<Model> {
        self.models
            .iter()
            .find(|m| m.id == model_id)
            .cloned()
            .ok_or_else(|| Error::ModelNotFound {
                provider_id: "google".into(),
                model_id: model_id.into(),
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
        let system_text = messages
            .iter()
            .filter_map(|m| {
                if let ChatMessage::System { content } = m {
                    Some(extract_text(content))
                } else {
                    None
                }
            })
            .collect::<Vec<_>>()
            .join("\n");
        let messages = crate::provider::normalize_messages(messages, model);
        let body = GeminiBody {
            contents: Self::build_contents(&messages),
            system_instruction: if system_text.is_empty() {
                None
            } else {
                Some(GeminiSystemInstruction {
                    parts: vec![GeminiTextPart { text: system_text }],
                })
            },
            tools: if tools.is_empty() {
                None
            } else {
                Some(vec![GeminiTool {
                    function_declarations: tools
                        .iter()
                        .map(|t| GeminiFunctionDeclaration {
                            name: t.name.clone(),
                            description: t.description.clone(),
                            parameters: Some(t.parameters.clone()),
                        })
                        .collect(),
                }])
            },
            tool_config: None,
            generation_config: Some(GeminiGenerationConfig {
                max_output_tokens: Some(crate::provider::max_output_tokens(
                    model,
                    crate::provider::OUTPUT_TOKEN_MAX,
                )),
                temperature: crate::provider::default_temperature(&model.api.id),
                top_p: crate::provider::default_top_p(&model.api.id),
                top_k: crate::provider::default_top_k(&model.api.id),
            }),
        };

        let response = self
            .http_client
            .post(self.stream_url(&model.api.id))
            .header("x-goog-api-key", &self.api_key)
            .header("Content-Type", "application/json")
            .json(&body)
            .send()
            .await
            .map_err(|e| Error::Network(format!("Gemini request: {e}")))?;

        if !response.status().is_success() {
            let status = response.status().as_u16();
            let text = response.text().await.unwrap_or_default();
            return Err(Error::Llm { http_context: None, 
                module: "gemini".into(),
                method: "stream".into(),
                reason: Box::new(classify_error(status, &text)),
            });
        }

        let sse_stream = parse_sse_stream(response);
        let state = GeminiStreamState {
            has_tool_calls: false,
            next_tool_id: 0,
            text_started: false,
            reasoning_started: false,
            step_started: false,
            reasoning_sig: None,
            usage: None,
            finish_reason: None,
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
            |(mut sse, mut st, mut buf)| {
                Box::pin(async move {
                    loop {
                        if let Some(ev) = buf.pop_front() {
                            return Some((ev, (sse, st, buf)));
                        }
                        if st.finished {
                            return None;
                        }
                        match sse.next().await {
                            Some(Ok(se)) if se.has_data() => {
                                if let Ok(ge) = serde_json::from_str::<GeminiEvent>(&se.data) {
                                    for ev in events_from_gemini(ge, &mut st) {
                                        buf.push_back(Ok(ev));
                                    }
                                    if let Some(ev) = buf.pop_front() {
                                        return Some((ev, (sse, st, buf)));
                                    }
                                }
                            }
                            Some(Err(e)) => {
                                return Some((
                                    Err(Error::ResponseStream(format!("Gemini SSE: {e}"))),
                                    (sse, st, buf),
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

fn classify_error(status: u16, body: &str) -> LlmErrorReason {
    match status {
        401 | 403 => LlmErrorReason::Authentication {
            message: body.into(),
            kind: crate::error::AuthErrorKind::Invalid,
        },
        429 => LlmErrorReason::RateLimit {
            message: body.into(),
            retry_after_ms: None,
        },
        400 | 413 => LlmErrorReason::InvalidRequest {
            message: body.into(),
            parameter: None,
            classification: if crate::error::is_context_overflow(body) {
                Some("context-overflow".into())
            } else {
                None
            },
        },
        500..=599 => LlmErrorReason::ProviderInternal {
            message: body.into(),
            status,
            retry_after_ms: None,
        },
        _ => LlmErrorReason::UnknownProvider {
            message: body.into(),
            status: Some(status),
        },
    }
}

fn build_model_catalog() -> Vec<Model> {
    let mk = |id: &str, name: &str, ctx: u64, out: u64, inp: f64, outp: f64| Model {
        id: id.into(),
        provider_id: "google".into(),
        name: name.into(),
        api: crate::provider::ApiInfo {
            id: id.into(),
            url: DEFAULT_BASE_URL.into(),
            npm: "@ai-sdk/google".into(),
        },
        family: Some("gemini".into()),
        capabilities: crate::provider::Capabilities {
            temperature: true,
            reasoning: true,
            attachment: true,
            toolcall: true,
            input: crate::provider::Modalities {
                text: true,
                image: true,
                audio: true,
                video: true,
                ..Default::default()
            },
            output: crate::provider::Modalities {
                text: true,
                ..Default::default()
            },
            interleaved: Default::default(),
        },
        cost: crate::provider::Cost {
            input: inp,
            output: outp,
            cache: Default::default(),
            tiers: None,
            experimental_over_200k: None,
        },
        limit: crate::provider::TokenLimit {
            context: ctx,
            input: None,
            output: out,
        },
        status: crate::provider::ModelStatus::Active,
        options: HashMap::new(),
        headers: HashMap::new(),
        release_date: "2026".into(),
        variants: None,
    };
    vec![
        mk(
            "gemini-3.0-pro",
            "Gemini 3.0 Pro",
            1_000_000,
            65_536,
            1.25,
            5.0,
        ),
        mk(
            "gemini-3.0-flash",
            "Gemini 3.0 Flash",
            1_000_000,
            65_536,
            0.15,
            0.60,
        ),
        mk(
            "gemini-2.5-pro",
            "Gemini 2.5 Pro",
            1_000_000,
            65_536,
            1.25,
            10.0,
        ),
        mk(
            "gemini-2.5-flash",
            "Gemini 2.5 Flash",
            1_000_000,
            65_536,
            0.15,
            0.60,
        ),
    ]
}
