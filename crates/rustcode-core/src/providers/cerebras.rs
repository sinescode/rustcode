//! Cerebras provider — OpenAI-compatible Chat Completions protocol.
//!
//! Cerebras provides ultra-fast inference via their custom wafer-scale
//! hardware. Uses the standard OpenAI Chat Completions wire format with
//! SSE streaming.
//!
//! Base URL: https://api.cerebras.ai/v1
//! Auth: Bearer token via CEREBRAS_API_KEY env var.
//! npm: @ai-sdk/cerebras
//!
//! Ported from:
//! - `packages/llm/src/protocols/openai-chat.ts`
//! - `packages/llm/src/providers/openai-compatible.ts`
//! - `packages/llm/src/providers/openai-compatible-profile.ts`

use async_trait::async_trait;
use futures::StreamExt;
use std::collections::{HashMap, VecDeque};
use std::pin::Pin;

use crate::error::{Error, LlmErrorReason};
use crate::provider::{
    ChatMessage, FinishReason, LlmEvent, MessageContent, Model, Provider, ToolDefinition,
};
use crate::providers::openai_compatible::make_simple_model;

const BASE_URL: &str = "https://api.cerebras.ai/v1";
const PROVIDER_ID: &str = "cerebras";
const NPM: &str = "@ai-sdk/cerebras";

fn resolve_api_key() -> Result<String, Error> {
    std::env::var("CEREBRAS_API_KEY")
        .ok()
        .filter(|k| !k.is_empty())
        .ok_or_else(|| Error::Auth("CEREBRAS_API_KEY environment variable not set".into()))
}

// ── Cerebras Provider ──────────────────────────────────────────────────

pub struct CerebrasProvider {
    api_key: String,
    base_url: String,
    http_client: reqwest::Client,
    models: Vec<Model>,
}

impl CerebrasProvider {
    pub fn new() -> Result<Self, Error> {
        Self::with_base_url(resolve_api_key()?, BASE_URL.into())
    }

    pub fn with_base_url(api_key: String, base_url: String) -> Result<Self, Error> {
        let http_client = reqwest::Client::builder()
            .user_agent(format!("rustcode/{}", env!("CARGO_PKG_VERSION")))
            .timeout(std::time::Duration::from_secs(300))
            .build()
            .map_err(|e| Error::Network(format!("HTTP client: {e}")))?;
        Ok(Self { api_key, base_url, http_client, models: build_model_catalog() })
    }

    fn chat_url(&self) -> String {
        format!("{}/chat/completions", self.base_url.trim_end_matches('/'))
    }

    fn build_body(&self, model: &Model, messages: &[ChatMessage], tools: &[ToolDefinition]) -> serde_json::Value {
        let msgs: Vec<serde_json::Value> = messages.iter().map(|m| match m {
            ChatMessage::System { content } => serde_json::json!({"role":"system","content": extract_text(content)}),
            ChatMessage::User { content } => serde_json::json!({"role":"user","content": extract_text(content)}),
            ChatMessage::Assistant { content } => {
                let text = extract_text(content);
                let mut obj = serde_json::json!({"role":"assistant"});
                if !text.is_empty() { obj["content"] = serde_json::Value::String(text); }
                else { obj["content"] = serde_json::Value::Null; }
                obj
            }
            ChatMessage::Tool { content } => {
                let mut arr = Vec::new();
                for p in content {
                    if let crate::provider::ToolResultPart::ToolResult { tool_call_id, output, .. } = p {
                        arr.push(serde_json::json!({"role":"tool","tool_call_id":tool_call_id,"content":output.to_string()}));
                    }
                }
                arr.first().cloned().unwrap_or(serde_json::json!({"role":"tool","tool_call_id":"","content":""}))
            }
        }).collect();

        let tools_arr: Vec<serde_json::Value> = tools.iter().map(|t| serde_json::json!({
            "type":"function","function":{"name":t.name,"description":t.description,"parameters":t.parameters}
        })).collect();

        let mut body = serde_json::json!({
            "model": model.api.id,
            "messages": msgs,
            "stream": true,
            "stream_options": {"include_usage": true},
            "max_tokens": crate::provider::max_output_tokens(model, crate::provider::OUTPUT_TOKEN_MAX),
        });
        if !tools_arr.is_empty() { body["tools"] = serde_json::Value::Array(tools_arr); }
        body
    }
}

fn extract_text(content: &MessageContent) -> String {
    match content {
        MessageContent::Text(t) => t.clone(),
        MessageContent::Parts(p) => p.iter()
            .filter_map(|p| if let crate::provider::ContentPart::Text { text } = p { Some(text.as_str()) } else { None })
            .collect::<Vec<_>>().join(""),
    }
}

// ── Provider impl ──────────────────────────────────────────────────────

#[async_trait]
impl Provider for CerebrasProvider {
    fn provider_id(&self) -> &str { PROVIDER_ID }
    fn npm(&self) -> &str { NPM }

    async fn list_models(&self) -> crate::error::Result<Vec<Model>> { Ok(self.models.clone()) }

    async fn get_model(&self, model_id: &str) -> crate::error::Result<Model> {
        self.models.iter().find(|m| m.id == model_id).cloned()
            .ok_or_else(|| Error::ModelNotFound { provider_id: PROVIDER_ID.into(), model_id: model_id.into() })
    }

    async fn stream(&self, model: &Model, messages: &[ChatMessage], tools: &[ToolDefinition]) -> crate::error::Result<Box<dyn futures::Stream<Item = crate::error::Result<LlmEvent>> + Send + Unpin>> {
        let body = self.build_body(model, messages, tools);
        let response = self.http_client.post(self.chat_url())
            .header("Authorization", format!("Bearer {}", self.api_key))
            .header("Content-Type", "application/json")
            .json(&body).send().await
            .map_err(|e| Error::Network(format!("Cerebras request: {e}")))?;

        if !response.status().is_success() {
            let status = response.status().as_u16();
            let text = response.text().await.unwrap_or_default();
            return Err(Error::Llm { module: PROVIDER_ID.into(), method: "stream".into(),
                reason: Box::new(LlmErrorReason::UnknownProvider { message: format!("HTTP {status}: {text}"), status: Some(status) }) });
        }

        let sse_stream = crate::sse::parse_sse_stream(response);
        let llm_stream = futures::stream::unfold(
            (Box::pin(sse_stream) as Pin<Box<dyn futures::Stream<Item = Result<crate::sse::SseEvent, crate::sse::SseError>> + Send + Unpin>>, false, VecDeque::new()),
            |(mut sse, mut finished, mut buf)| async move {
                loop {
                    if let Some(ev) = buf.pop_front() { return Some((ev, (sse, finished, buf))); }
                    if finished { return None; }
                    match sse.next().await {
                        Some(Ok(se)) if !se.is_done() && se.has_data() => {
                            if let Ok(ce) = serde_json::from_str::<serde_json::Value>(&se.data) {
                                if let Some(choices) = ce["choices"].as_array() {
                                    if let Some(c) = choices.first() {
                                        if let Some(delta_content) = c["delta"]["content"].as_str() {
                                            buf.push_back(Ok(LlmEvent::TextDelta { id: "text-0".into(), text: delta_content.into(), provider_metadata: None }));
                                        }
                                        if let Some(fr) = c["finish_reason"].as_str() {
                                            let reason = match fr { "stop" => FinishReason::Stop, "length" => FinishReason::Length, "tool_calls"|"function_call" => FinishReason::ToolCalls, "content_filter" => FinishReason::ContentFilter, _ => FinishReason::Unknown };
                                            buf.push_back(Ok(LlmEvent::TextEnd { id: "text-0".into(), provider_metadata: None }));
                                            buf.push_back(Ok(LlmEvent::Finish { reason, usage: None, provider_metadata: None }));
                                            finished = true;
                                        }
                                    }
                                }
                            }
                            if let Some(ev) = buf.pop_front() { return Some((ev, (sse, finished, buf))); }
                        }
                        Some(Err(e)) => return Some((Err(Error::ResponseStream(format!("Cerebras SSE: {e}"))), (sse, finished, buf))),
                        None => return None,
                        _ => continue,
                    }
                }
            },
        );
        Ok(Box::new(llm_stream))
    }

    async fn complete(&self, model: &Model, messages: &[ChatMessage], tools: &[ToolDefinition]) -> crate::error::Result<crate::provider::LlmResponse> {
        let mut stream = self.stream(model, messages, tools).await?;
        let mut events = Vec::new();
        while let Some(r) = stream.next().await { if let Ok(ev) = r { events.push(ev); } }
        Ok(crate::provider::LlmResponse { events, usage: None })
    }
}

// ── Model Catalog ──────────────────────────────────────────────────────

fn build_model_catalog() -> Vec<Model> {
    vec![
        make_simple_model("llama-4-maverick", "Llama 4 Maverick", PROVIDER_ID, BASE_URL, "llama", 128_000, 16_384),
        make_simple_model("llama-3.3-70b", "Llama 3.3 70B", PROVIDER_ID, BASE_URL, "llama", 128_000, 8_192),
        make_simple_model("mixtral-8x7b", "Mixtral 8x7B", PROVIDER_ID, BASE_URL, "mixtral", 32_000, 4_096),
    ]
}

// ── Tests ──────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_model_catalog_count() {
        let models = build_model_catalog();
        assert_eq!(models.len(), 3);
    }

    #[test]
    fn test_model_catalog_has_llama4_maverick() {
        let models = build_model_catalog();
        let m = models.iter().find(|m| m.id == "llama-4-maverick").expect("llama-4-maverick not found");
        assert_eq!(m.name, "Llama 4 Maverick");
        assert_eq!(m.provider_id, PROVIDER_ID);
        assert_eq!(m.limit.context, 128_000);
        assert_eq!(m.family.as_deref(), Some("llama"));
    }

    #[test]
    fn test_model_catalog_has_llama33_70b() {
        let models = build_model_catalog();
        let m = models.iter().find(|m| m.id == "llama-3.3-70b").expect("llama-3.3-70b not found");
        assert_eq!(m.name, "Llama 3.3 70B");
        assert_eq!(m.limit.output, 8_192);
    }

    #[test]
    fn test_model_catalog_has_mixtral() {
        let models = build_model_catalog();
        let m = models.iter().find(|m| m.id == "mixtral-8x7b").expect("mixtral-8x7b not found");
        assert_eq!(m.name, "Mixtral 8x7B");
        assert_eq!(m.family.as_deref(), Some("mixtral"));
    }

    #[test]
    fn test_provider_id_and_npm() {
        let provider = CerebrasProvider::with_base_url("test-key".into(), BASE_URL.into()).expect("construct");
        assert_eq!(provider.provider_id(), PROVIDER_ID);
        assert_eq!(provider.npm(), NPM);
    }

    #[test]
    fn test_get_model_found() {
        let provider = CerebrasProvider::with_base_url("test-key".into(), BASE_URL.into()).expect("construct");
        let m = provider.get_model("llama-4-maverick");
        assert!(m.is_ok());
        assert_eq!(m.unwrap().id, "llama-4-maverick");
    }

    #[test]
    fn test_get_model_not_found() {
        let provider = CerebrasProvider::with_base_url("test-key".into(), BASE_URL.into()).expect("construct");
        let result = provider.get_model("nonexistent");
        assert!(result.is_err());
    }

    #[test]
    fn test_chat_url() {
        let provider = CerebrasProvider::with_base_url("test-key".into(), BASE_URL.into()).expect("construct");
        assert_eq!(provider.chat_url(), "https://api.cerebras.ai/v1/chat/completions");
    }

    #[test]
    fn test_all_models_active() {
        for m in &build_model_catalog() {
            assert_eq!(m.status, crate::provider::ModelStatus::Active, "model {} not active", m.id);
        }
    }
}
