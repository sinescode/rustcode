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
    ToolDefinition,
};

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
            let s = resp.status().as_u16();
            let t = resp.text().await.unwrap_or_default();
            return Err(Error::Llm {
                module: "openrouter".into(),
                method: "stream".into(),
                reason: Box::new(LlmErrorReason::UnknownProvider {
                    message: format!("HTTP {s}: {t}"),
                    status: Some(s),
                }),
            });
        }

        let sse = crate::sse::parse_sse_stream(resp);
        let llm = futures::stream::unfold(
            (
                Box::pin(sse)
                    as Pin<
                        Box<
                            dyn futures::Stream<
                                    Item = Result<crate::sse::SseEvent, crate::sse::SseError>,
                                > + Send
                                + Unpin,
                        >,
                    >,
                false,
                VecDeque::new(),
            ),
            |(mut s, mut done, mut buf)| {
                Box::pin(async move {
                    loop {
                        if let Some(ev) = buf.pop_front() {
                            return Some((ev, (s, done, buf)));
                        }
                        if done {
                            return None;
                        }
                        match s.next().await {
                            Some(Ok(e)) if !e.is_done() && e.has_data() => {
                                if let Ok(v) = serde_json::from_str::<serde_json::Value>(&e.data) {
                                    if let Some(ch) = v["choices"][0].as_object() {
                                        if let Some(dc) = ch["delta"]["content"].as_str() {
                                            buf.push_back(Ok(LlmEvent::TextDelta {
                                                id: "text-0".into(),
                                                text: dc.into(),
                                                provider_metadata: None,
                                            }));
                                        }
                                        if let Some(fr) = ch["finish_reason"].as_str() {
                                            let reason = match fr {
                                                "stop" => FinishReason::Stop,
                                                "length" => FinishReason::Length,
                                                "tool_calls" | "function_call" => {
                                                    FinishReason::ToolCalls
                                                }
                                                "content_filter" => FinishReason::ContentFilter,
                                                _ => FinishReason::Unknown,
                                            };
                                            buf.push_back(Ok(LlmEvent::Finish {
                                                reason,
                                                usage: None,
                                                provider_metadata: None,
                                            }));
                                            done = true;
                                        }
                                    }
                                }
                                if let Some(ev) = buf.pop_front() {
                                    return Some((ev, (s, done, buf)));
                                }
                            }
                            Some(Err(e)) => {
                                return Some((
                                    Err(Error::ResponseStream(format!("OpenRouter SSE: {e}"))),
                                    (s, done, buf),
                                ))
                            }
                            None => return None,
                            _ => continue,
                        }
                    }
                })
            },
        );
        Ok(Box::new(llm))
    }

    async fn complete(
        &self,
        m: &Model,
        msgs: &[ChatMessage],
        t: &[ToolDefinition],
    ) -> crate::error::Result<crate::provider::LlmResponse> {
        let mut s = self.stream(m, msgs, t).await?;
        let mut evs = vec![];
        while let Some(r) = s.next().await {
            if let Ok(e) = r {
                evs.push(e);
            }
        }
        Ok(crate::provider::LlmResponse {
            events: evs,
            usage: None,
        })
    }
}
