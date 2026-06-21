//! Amazon Bedrock Converse API — native protocol implementation.
//!
//! Uses the Bedrock Converse API directly instead of the OpenAI-compatible
//! Chat Completions bridge (`/chat/completions`). Benefits:
//! - Native content block types (text, toolUse, toolResult, reasoningContent)
//! - Proper tool call start/delta/stop lifecycle via content block indices
//! - Correct stop reason mapping (end_turn, max_tokens, tool_use, content_filter)
//! - Native cache point support for prompt caching
//! - Usage includes cache read/write breakdown
//!
//! ## Binary Event-Stream Framing
//!
//! Bedrock Converse streams use the AWS event-stream binary protocol. Each
//! frame is `[total_length:4][headers_length:4][prelude_crc:4][headers][payload][crc:4]`.
//! The `:event-type` header identifies the JSON payload variant.
//!
//! Ported from:
//! - `packages/llm/src/protocols/bedrock-converse.ts` (664 lines)
//! - `packages/llm/src/protocols/bedrock-event-stream.ts` (87 lines)
//! - `packages/llm/src/protocols/utils/bedrock-auth.ts` (70 lines)
//! - `packages/llm/src/protocols/utils/bedrock-media.ts` (90 lines)
//! - `packages/llm/src/protocols/utils/bedrock-cache.ts` (37 lines)

use async_trait::async_trait;
use chrono::Utc;
use futures::StreamExt;
use hmac::{Hmac, Mac};
use sha2::{Digest, Sha256};
use std::collections::HashMap;
use std::collections::VecDeque;
use std::pin::Pin;
use std::sync::OnceLock;

use crate::error::{Error, LlmErrorReason};
use crate::provider::{
    ChatMessage, ContentPart, FinishReason, LlmEvent, MessageContent, Model, Provider,
    ToolDefinition, Usage,
};
use crate::tool_stream::ToolStreamAccumulator;

// ── CRC32 (for AWS event-stream framing) ───────────────────────────────

static CRC32_TABLE: OnceLock<[u32; 256]> = OnceLock::new();

fn crc32_table() -> &'static [u32; 256] {
    CRC32_TABLE.get_or_init(|| {
        let mut table = [0u32; 256];
        for i in 0..256 {
            let mut crc = i as u32;
            for _ in 0..8 {
                if crc & 1 != 0 {
                    crc = 0xedb88320 ^ (crc >> 1);
                } else {
                    crc >>= 1;
                }
            }
            table[i as usize] = crc;
        }
        table
    })
}

fn crc32(buf: &[u8]) -> u32 {
    let table = crc32_table();
    let mut crc = !0u32;
    for &byte in buf {
        crc = table[((crc as u8) ^ byte) as usize] ^ (crc >> 8);
    }
    !crc
}

// ── AWS SigV4 Signing ──────────────────────────────────────────────────

type HmacSha256 = Hmac<Sha256>;

/// AWS SigV4 signer for Bedrock requests. Duplicated from `bedrock.rs` to
/// keep bedrock_converse self-contained.
struct SigV4Signer {
    access_key: String,
    secret_key: String,
    region: String,
    service: &'static str,
}

impl SigV4Signer {
    fn new(access_key: &str, secret_key: &str, region: &str) -> Self {
        Self {
            access_key: access_key.into(),
            secret_key: secret_key.into(),
            region: region.into(),
            service: "bedrock",
        }
    }

    fn sign(
        &self,
        method: &str,
        host: &str,
        path: &str,
        body: &[u8],
        date: &str,
    ) -> Result<String, Error> {
        let payload_hash = hex::encode(Sha256::digest(body));
        let signed_headers = "host;x-amz-content-sha256;x-amz-date";
        let canonical_headers =
            format!("host:{host}\nx-amz-content-sha256:{payload_hash}\nx-amz-date:{date}\n");
        let canonical_request =
            format!("{method}\n{path}\n\n{canonical_headers}\n{signed_headers}\n{payload_hash}");

        let credential_scope = format!("{date}/{}/{}/aws4_request", self.region, self.service);
        let canonical_request_hash = hex::encode(Sha256::digest(canonical_request.as_bytes()));
        let string_to_sign =
            format!("AWS4-HMAC-SHA256\n{date}\n{credential_scope}\n{canonical_request_hash}");

        let k_date = self.hmac_raw(
            format!("AWS4{}", self.secret_key).as_bytes(),
            date.as_bytes(),
        );
        let k_region = self.hmac_raw(&k_date, self.region.as_bytes());
        let k_service = self.hmac_raw(&k_region, self.service.as_bytes());
        let k_signing = self.hmac_raw(&k_service, b"aws4_request");

        let signature = self.hmac_raw(&k_signing, string_to_sign.as_bytes());

        Ok(format!(
            "AWS4-HMAC-SHA256 Credential={}/{}, SignedHeaders={signed_headers}, Signature={}",
            self.access_key,
            credential_scope,
            hex::encode(signature),
        ))
    }

    fn hmac_raw(&self, key: &[u8], data: &[u8]) -> Vec<u8> {
        let mut mac = HmacSha256::new_from_slice(key).expect("HMAC can take key of any size");
        mac.update(data);
        mac.finalize().into_bytes().to_vec()
    }
}

// ── Env Helpers ────────────────────────────────────────────────────────

fn resolve_api_key() -> Result<String, Error> {
    std::env::var("AWS_ACCESS_KEY_ID")
        .ok()
        .filter(|k| !k.is_empty())
        .ok_or_else(|| Error::Auth("AWS_ACCESS_KEY_ID environment variable not set".into()))
}

fn resolve_secret_key() -> Result<String, Error> {
    std::env::var("AWS_SECRET_ACCESS_KEY")
        .ok()
        .filter(|k| !k.is_empty())
        .ok_or_else(|| Error::Auth("AWS_SECRET_ACCESS_KEY environment variable not set".into()))
}

fn resolve_region() -> String {
    std::env::var("AWS_REGION")
        .ok()
        .filter(|r| !r.is_empty())
        .unwrap_or_else(|| "us-east-1".into())
}

fn bedrock_base_url(region: &str) -> String {
    format!("https://bedrock-runtime.{}.amazonaws.com", region)
}

// ═════════════════════════════════════════════════════════════════════════
// 1. AWS Event-Stream Binary Framing Decoder
// ═════════════════════════════════════════════════════════════════════════

/// Decoded AWS event-stream frame with its `:event-type` and JSON payload.
struct AwsFrame {
    event_type: String,
    payload: serde_json::Value,
}

/// Decode one AWS event-stream frame from a byte slice.
///
/// Returns `None` if more bytes are needed. Returns `Some(Err(...))` on
/// decode failure.
///
/// Frame format (big-endian):
/// - `[0..4]`: total_length (u32, includes all fields)
/// - `[4..8]`: headers_length (u32)
/// - `[8..12]`: prelude_crc (u32, CRC32 of bytes 0..8)
/// - `[12..12+headers_length]`: headers (variable-length TLV)
/// - `[12+headers_length..total_length-4]`: payload (JSON UTF-8)
/// - `[total_length-4..total_length]`: message_crc (u32, CRC32 of bytes 0..total_length-4)
fn decode_aws_frame(bytes: &[u8]) -> Result<Option<AwsFrame>, Error> {
    if bytes.len() < 12 {
        return Ok(None);
    }

    let total_length = u32::from_be_bytes([bytes[0], bytes[1], bytes[2], bytes[3]]) as usize;
    let headers_length = u32::from_be_bytes([bytes[4], bytes[5], bytes[6], bytes[7]]) as usize;
    let prelude_crc =
        u32::from_be_bytes([bytes[8], bytes[9], bytes[10], bytes[11]]);

    if total_length < 12 {
        return Err(Error::ResponseStream(
            "AWS event-stream: total_length < 12".into(),
        ));
    }
    if headers_length + 16 > total_length {
        return Err(Error::ResponseStream(
            "AWS event-stream: headers_length exceeds frame".into(),
        ));
    }

    // Validate prelude CRC
    let expected_prelude_crc = crc32(&bytes[..8]);
    if prelude_crc != expected_prelude_crc {
        return Err(Error::ResponseStream(format!(
            "AWS event-stream: prelude CRC mismatch (got {prelude_crc:#x}, expected {expected_prelude_crc:#x})"
        )));
    }

    if bytes.len() < total_length {
        return Ok(None);
    }

    // Validate message CRC
    let message_crc = u32::from_be_bytes([
        bytes[total_length - 4],
        bytes[total_length - 3],
        bytes[total_length - 2],
        bytes[total_length - 1],
    ]);
    let expected_message_crc = crc32(&bytes[..total_length - 4]);
    if message_crc != expected_message_crc {
        return Err(Error::ResponseStream(format!(
            "AWS event-stream: message CRC mismatch (got {message_crc:#x}, expected {expected_message_crc:#x})"
        )));
    }

    // Parse headers to find `:event-type`
    let mut offset = 12;
    let headers_end = 12 + headers_length;
    let mut event_type: Option<String> = None;

    while offset < headers_end {
        if offset >= bytes.len() {
            break;
        }
        let name_len = bytes[offset] as usize;
        offset += 1;
        if offset + name_len > bytes.len() {
            break;
        }
        let name = std::str::from_utf8(&bytes[offset..offset + name_len])
            .map_err(|e| {
                Error::ResponseStream(format!("AWS event-stream header name UTF-8: {e}"))
            })?;
        offset += name_len;
        if offset >= bytes.len() {
            break;
        }
        let header_type = bytes[offset];
        offset += 1;

        // Decode value based on type
        match header_type {
            0 | 1 => { /* bool true/false, skip */ }
            2 => {
                // byte
                if offset + 1 <= bytes.len() {
                    offset += 1;
                }
            }
            3 => {
                // int16
                if offset + 2 <= bytes.len() {
                    offset += 2;
                }
            }
            4 => {
                // int32
                if offset + 4 <= bytes.len() {
                    offset += 4;
                }
            }
            5 | 8 => {
                // int64 or timestamp
                if offset + 8 <= bytes.len() {
                    offset += 8;
                }
            }
            6 | 7 => {
                // byte array or string
                if offset + 2 > bytes.len() {
                    break;
                }
                let val_len =
                    u16::from_be_bytes([bytes[offset], bytes[offset + 1]]) as usize;
                offset += 2;
                if offset + val_len > bytes.len() {
                    break;
                }
                if header_type == 7 && name == ":event-type" {
                    let val = std::str::from_utf8(&bytes[offset..offset + val_len])
                        .map_err(|e| {
                            Error::ResponseStream(format!(
                                "AWS event-stream :event-type UTF-8: {e}"
                            ))
                        })?;
                    event_type = Some(val.to_string());
                }
                offset += val_len;
            }
            9 => {
                // UUID (16 bytes)
                if offset + 16 <= bytes.len() {
                    offset += 16;
                }
            }
            _ => {
                return Err(Error::ResponseStream(format!(
                    "AWS event-stream unknown header type {header_type}"
                )));
            }
        }
    }

    let event_type = event_type.ok_or_else(|| {
        Error::ResponseStream("AWS event-stream missing :event-type header".into())
    })?;

    // Parse payload
    let payload_start = 12 + headers_length;
    let payload_end = total_length - 4;
    if payload_end <= payload_start {
        return Ok(Some(AwsFrame {
            event_type,
            payload: serde_json::Value::Object(serde_json::Map::new()),
        }));
    }
    let payload_bytes = &bytes[payload_start..payload_end];

    // Remove trailing null bytes / padding (Bedrock pads short payloads)
    let trimmed: &[u8] = payload_bytes
        .iter()
        .rposition(|&b| b != 0)
        .map(|pos| &payload_bytes[..=pos])
        .unwrap_or(payload_bytes);

    let mut payload: serde_json::Value = if trimmed.is_empty() {
        serde_json::Value::Object(serde_json::Map::new())
    } else {
        serde_json::from_slice(trimmed).map_err(|e| {
            Error::ResponseStream(format!(
                "AWS event-stream payload JSON: {e}: {}",
                String::from_utf8_lossy(trimmed)
            ))
        })?
    };

    // Remove `p` padding field if present (AWS event-stream padding artifact)
    if let serde_json::Value::Object(ref mut map) = payload {
        map.remove("p");
    }

    Ok(Some(AwsFrame {
        event_type,
        payload,
    }))
}

// ═════════════════════════════════════════════════════════════════════════
// 2. Request Body Construction
// ═════════════════════════════════════════════════════════════════════════

/// Build the Converse API request body from model, messages, and tools.
///
/// Returns a `serde_json::Value` matching the `Converse` or `ConverseStream`
/// API request shape:
/// ```json
/// {
///   "modelId": "claude-sonnet-4-20250514",
///   "messages": [{"role": "user", "content": [{"text": "..."}]}],
///   "system": [{"text": "..."}],
///   "inferenceConfig": {"maxTokens": 8192, "temperature": 1.0},
///   "toolConfig": {"tools": [...]}
/// }
/// ```
pub fn build_converse_body(
    model: &Model,
    messages: &[ChatMessage],
    tools: &[ToolDefinition],
) -> serde_json::Value {
    let (system_blocks, converse_messages) = lower_messages(messages);
    let tool_config = lower_tools_to_converse(tools);

    let mut body = serde_json::json!({
        "modelId": model.api.id,
        "messages": converse_messages,
    });

    if !system_blocks.is_empty() {
        body["system"] = serde_json::Value::Array(system_blocks);
    }

    let mut inference_config = serde_json::Map::new();
    let max_tokens =
        crate::provider::max_output_tokens(model, crate::provider::OUTPUT_TOKEN_MAX);
    if max_tokens > 0 {
        inference_config.insert("maxTokens".into(), serde_json::json!(max_tokens));
    }
    let temp = crate::provider::default_temperature(&model.api.id);
    if let Some(t) = temp {
        inference_config.insert("temperature".into(), serde_json::json!(t));
    }
    let top_p = crate::provider::default_top_p(&model.api.id);
    if let Some(p) = top_p {
        inference_config.insert("topP".into(), serde_json::json!(p));
    }
    if !inference_config.is_empty() {
        body["inferenceConfig"] = serde_json::Value::Object(inference_config);
    }

    if let Some(tc) = tool_config {
        body["toolConfig"] = tc;
    }

    body
}

/// Lower `ChatMessage`s to Converse system blocks and messages array.
///
/// Converse API rules:
/// - System messages stack into the top-level `system` field
/// - User messages with text/image/tool-result content blocks
/// - Assistant messages with text/reasoning/tool-use content blocks
/// - Tool messages are folded into user messages with `toolResult` blocks
/// - Any `ChatMessage::System` NOT at the start (chronological update) is
///   wrapped in `<system-update>` and sent as a user text block
fn lower_messages(messages: &[ChatMessage]) -> (Vec<serde_json::Value>, Vec<serde_json::Value>) {
    let mut system_blocks: Vec<serde_json::Value> = Vec::new();
    let mut converse_messages: Vec<serde_json::Value> = Vec::new();
    let mut seen_non_system = false;

    for msg in messages {
        match msg {
            ChatMessage::System { content } => {
                if !seen_non_system {
                    // Initial system messages → top-level system field
                    accumulate_system_text(content, &mut system_blocks);
                } else {
                    // Chronological system update → wrapped as user message
                    let text = extract_message_text(content);
                    let wrapped = format!("<system-update>\n{text}\n</system-update>");
                    let content_block = serde_json::json!({"text": wrapped});
                    let last = converse_messages.last_mut();
                    if let Some(prev) = last {
                        if prev["role"] == "user" {
                            if let Some(arr) = prev["content"].as_array_mut() {
                                arr.push(content_block);
                                continue;
                            }
                        }
                    }
                    converse_messages.push(serde_json::json!({
                        "role": "user",
                        "content": [content_block]
                    }));
                }
            }
            ChatMessage::User { content } => {
                seen_non_system = true;
                let blocks = lower_user_content(content);
                if !blocks.is_empty() {
                    converse_messages.push(serde_json::json!({
                        "role": "user",
                        "content": blocks
                    }));
                }
            }
            ChatMessage::Assistant { content } => {
                seen_non_system = true;
                let blocks = lower_assistant_content(content);
                if !blocks.is_empty() {
                    converse_messages.push(serde_json::json!({
                        "role": "assistant",
                        "content": blocks
                    }));
                }
            }
            ChatMessage::Tool { content } => {
                seen_non_system = true;
                let blocks = lower_tool_result_content(content);
                if !blocks.is_empty() {
                    converse_messages.push(serde_json::json!({
                        "role": "user",
                        "content": blocks
                    }));
                }
            }
        }
    }

    (system_blocks, converse_messages)
}

fn extract_message_text(content: &MessageContent) -> String {
    match content {
        MessageContent::Text(t) => t.clone(),
        MessageContent::Parts(parts) => parts
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

fn accumulate_system_text(content: &MessageContent, blocks: &mut Vec<serde_json::Value>) {
    let text = extract_message_text(content);
    if !text.is_empty() {
        blocks.push(serde_json::json!({"text": text}));
    }
}

fn lower_user_content(content: &MessageContent) -> Vec<serde_json::Value> {
    match content {
        MessageContent::Text(t) => {
            if t.is_empty() {
                vec![]
            } else {
                vec![serde_json::json!({"text": t})]
            }
        }
        MessageContent::Parts(parts) => {
            let mut blocks = Vec::new();
            for part in parts {
                match part {
                    ContentPart::Text { text } => {
                        blocks.push(serde_json::json!({"text": text}));
                    }
                    ContentPart::Image { image } => {
                        let (fmt, b64) = if image.starts_with("data:") {
                            // data:image/png;base64,...
                            if let Some(comma) = image.find(',') {
                                let header = &image[..comma];
                                let data = &image[comma + 1..];
                                let fmt = if header.contains("png") {
                                    "png"
                                } else if header.contains("jpeg") || header.contains("jpg") {
                                    "jpeg"
                                } else if header.contains("gif") {
                                    "gif"
                                } else if header.contains("webp") {
                                    "webp"
                                } else {
                                    "png"
                                };
                                (fmt, data)
                            } else {
                                ("png", image.as_str())
                            }
                        } else {
                            ("png", image.as_str())
                        };
                        blocks.push(serde_json::json!({
                            "image": {
                                "format": fmt,
                                "source": {"bytes": b64}
                            }
                        }));
                    }
                    _ => {}
                }
            }
            blocks
        }
    }
}

fn lower_assistant_content(content: &MessageContent) -> Vec<serde_json::Value> {
    match content {
        MessageContent::Text(t) => {
            if t.is_empty() {
                vec![]
            } else {
                vec![serde_json::json!({"text": t})]
            }
        }
        MessageContent::Parts(parts) => {
            let mut blocks = Vec::new();
            for part in parts {
                match part {
                    ContentPart::Text { text } => {
                        blocks.push(serde_json::json!({"text": text}));
                    }
                    ContentPart::Reasoning {
                        text,
                        provider_options,
                    } => {
                        let signature = provider_options
                            .as_ref()
                            .and_then(|o| o.get("bedrock"))
                            .and_then(|b| b.get("signature"))
                            .and_then(|s| s.as_str());
                        let mut rc = serde_json::json!({
                            "reasoningContent": {
                                "reasoningText": {
                                    "text": text
                                }
                            }
                        });
                        if let Some(sig) = signature {
                            if let Some(obj) = rc.pointer_mut("/reasoningContent/reasoningText") {
                                if let Some(map) = obj.as_object_mut() {
                                    let sig_map = serde_json::json!({"text": text, "signature": sig});
                                    if let serde_json::Value::Object(m) = sig_map {
                                        *obj = serde_json::Value::Object(m);
                                    }
                                }
                            }
                        }
                        blocks.push(rc);
                    }
                    ContentPart::ToolCallPart {
                        tool_call_id,
                        tool_name,
                        arguments,
                    } => {
                        blocks.push(serde_json::json!({
                            "toolUse": {
                                "toolUseId": tool_call_id,
                                "name": tool_name,
                                "input": arguments
                            }
                        }));
                    }
                    _ => {}
                }
            }
            blocks
        }
    }
}

fn lower_tool_result_content(content: &[crate::provider::ToolResultPart]) -> Vec<serde_json::Value> {
    let mut blocks = Vec::new();
    for part in content {
        let crate::provider::ToolResultPart::ToolResult {
            tool_call_id,
            output,
            is_error,
            ..
        } = part;
        let status = if *is_error { "error" } else { "success" };
        let result_content = match output {
            serde_json::Value::String(s) => {
                vec![serde_json::json!({"text": s})]
            }
            other => {
                vec![serde_json::json!({"json": other})]
            }
        };
        blocks.push(serde_json::json!({
            "toolResult": {
                "toolUseId": tool_call_id,
                "content": result_content,
                "status": status
            }
        }));
    }
    blocks
}

fn lower_tools_to_converse(tools: &[ToolDefinition]) -> Option<serde_json::Value> {
    if tools.is_empty() {
        return None;
    }

    let tools_arr: Vec<serde_json::Value> = tools
        .iter()
        .map(|t| {
            serde_json::json!({
                "toolSpec": {
                    "name": t.name,
                    "description": t.description,
                    "inputSchema": {
                        "json": t.parameters
                    }
                }
            })
        })
        .collect();

    Some(serde_json::json!({
        "tools": tools_arr
    }))
}

// ═════════════════════════════════════════════════════════════════════════
// 3. Stream Parsing — ConverseStreamParser
// ═════════════════════════════════════════════════════════════════════════

/// Map a Converse stop reason string to `FinishReason`.
fn map_finish_reason(reason: &str) -> FinishReason {
    match reason {
        "end_turn" | "stop_sequence" => FinishReason::Stop,
        "max_tokens" => FinishReason::Length,
        "tool_use" => FinishReason::ToolCalls,
        "content_filtered" | "guardrail_intervened" => FinishReason::ContentFilter,
        _ => FinishReason::Unknown,
    }
}

/// Parse usage from a Converse metadata usage object.
fn map_converse_usage(usage: &serde_json::Value) -> Usage {
    let input_tokens = usage.get("inputTokens").and_then(|v| v.as_u64());
    let output_tokens = usage.get("outputTokens").and_then(|v| v.as_u64());
    let total_tokens = usage.get("totalTokens").and_then(|v| v.as_u64());
    let cache_read = usage.get("cacheReadInputTokens").and_then(|v| v.as_u64());
    let cache_write = usage.get("cacheWriteInputTokens").and_then(|v| v.as_u64());

    let cache_total = cache_read.unwrap_or(0) + cache_write.unwrap_or(0);
    let non_cached = input_tokens.map(|i| i.saturating_sub(cache_total));

    Usage {
        input_tokens,
        output_tokens,
        non_cached_input_tokens: non_cached,
        cache_read_input_tokens: cache_read,
        cache_write_input_tokens: cache_write,
        reasoning_tokens: None,
        total_tokens,
        provider_metadata: Some({
            let mut m = HashMap::new();
            m.insert("bedrock".into(), usage.clone());
            m
        }),
    }
}

/// Parser state for the Converse streaming event state machine.
///
/// Tracks content blocks (text, reasoning, tool calls), pending finish
/// reason/usage (since `messageStop` and `metadata` arrive in separate
/// events), and reasoning signatures.
#[derive(Debug)]
pub struct ConverseStreamState {
    /// Accumulator for streaming tool call JSON arguments (keyed by
    /// content block index).
    pub tool_stream: ToolStreamAccumulator,
    /// Whether a text block has been started.
    pub text_started: bool,
    /// Whether a reasoning block has been started.
    pub reasoning_started: bool,
    /// Whether a step has started (for StepStart).
    pub step_started: bool,
    /// Whether any tool calls were made in this response.
    pub has_tool_calls: bool,
    /// Pending finish from `messageStop`; paired with usage from `metadata`.
    pub pending_finish: Option<FinishReason>,
    /// Pending usage from `metadata`.
    pub pending_usage: Option<Usage>,
    /// Whether we've seen a `messageStop` event.
    pub seen_message_stop: bool,
    /// Whether we've seen a `metadata` event.
    pub seen_metadata: bool,
    /// Reasoning signatures per content block index.
    pub reasoning_signatures: HashMap<u64, String>,
}

impl ConverseStreamState {
    pub fn new() -> Self {
        Self {
            tool_stream: ToolStreamAccumulator::new(),
            text_started: false,
            reasoning_started: false,
            step_started: false,
            has_tool_calls: false,
            pending_finish: None,
            pending_usage: None,
            seen_message_stop: false,
            seen_metadata: false,
            reasoning_signatures: HashMap::new(),
        }
    }
}

impl Default for ConverseStreamState {
    fn default() -> Self {
        Self::new()
    }
}

/// Process one Converse streaming event and produce zero or more `LlmEvent`s.
///
/// # Events handled
/// - `messageStart`: role notification (tracked, no direct events)
/// - `contentBlockStart` with `toolUse.start`: emits `ToolInputStart`
/// - `contentBlockDelta` with `delta.text`: emits `TextStart`/`TextDelta`
/// - `contentBlockDelta` with `delta.reasoningContent`: emits `ReasoningStart`/`ReasoningDelta`
/// - `contentBlockDelta` with `delta.toolUse.input`: emits `ToolInputDelta`
/// - `contentBlockStop`: closes text/reasoning/tool blocks
/// - `messageStop`: records finish reason, may emit terminal
/// - `metadata`: records usage, may emit terminal
/// - Error events: `ProviderError`
pub fn events_from_converse(
    event_type: &str,
    payload: &serde_json::Value,
    state: &mut ConverseStreamState,
) -> Vec<LlmEvent> {
    let mut events = Vec::new();

    match event_type {
        "messageStart" => {
            // Role notification — we don't emit events for this,
            // but track it for potential future use.
        }

        "contentBlockStart" => {
            let index = payload
                .get("contentBlockIndex")
                .and_then(|v| v.as_u64())
                .unwrap_or(0);
            let start = payload.get("start");

            if let Some(start) = start {
                if let Some(tool_use) = start.get("toolUse") {
                    let tool_use_id = tool_use
                        .get("toolUseId")
                        .and_then(|v| v.as_str())
                        .unwrap_or("")
                        .to_string();
                    let name = tool_use
                        .get("name")
                        .and_then(|v| v.as_str())
                        .unwrap_or("")
                        .to_string();

                    if !state.step_started {
                        events.push(LlmEvent::StepStart { index: 0 });
                        state.step_started = true;
                    }

                    state.tool_stream.start(index, &name, tool_use_id.clone());
                    events.push(LlmEvent::ToolInputStart {
                        id: tool_use_id,
                        name,
                        provider_metadata: None,
                    });
                }
            }
        }

        "contentBlockDelta" => {
            let index = payload
                .get("contentBlockIndex")
                .and_then(|v| v.as_u64())
                .unwrap_or(0);
            let delta = payload.get("delta");

            if let Some(delta) = delta {
                // Text delta
                if let Some(text) = delta.get("text").and_then(|v| v.as_str()) {
                    if !text.is_empty() {
                        if !state.text_started {
                            events.push(LlmEvent::TextStart {
                                id: format!("text-{index}"),
                                provider_metadata: None,
                            });
                            state.text_started = true;
                        }
                        events.push(LlmEvent::TextDelta {
                            id: format!("text-{index}"),
                            text: text.to_string(),
                            provider_metadata: None,
                        });
                    }
                }

                // Reasoning content delta
                if let Some(rc) = delta.get("reasoningContent") {
                    if let Some(text) = rc.get("text").and_then(|v| v.as_str()) {
                        if !text.is_empty() {
                            if !state.reasoning_started {
                                events.push(LlmEvent::ReasoningStart {
                                    id: format!("reasoning-{index}"),
                                    provider_metadata: None,
                                });
                                state.reasoning_started = true;
                            }
                            events.push(LlmEvent::ReasoningDelta {
                                id: format!("reasoning-{index}"),
                                text: text.to_string(),
                                provider_metadata: None,
                            });
                        }
                    }
                    if let Some(sig) = rc.get("signature").and_then(|v| v.as_str()) {
                        state
                            .reasoning_signatures
                            .insert(index, sig.to_string());
                    }
                }

                // Tool use delta (streaming JSON input)
                if let Some(tool_use) = delta.get("toolUse") {
                    if let Some(input) = tool_use.get("input").and_then(|v| v.as_str()) {
                        if !state.step_started {
                            events.push(LlmEvent::StepStart { index: 0 });
                            state.step_started = true;
                        }
                        if let Some(ev) = state.tool_stream.append(index, input) {
                            events.push(ev);
                        }
                    }
                }
            }
        }

        "contentBlockStop" => {
            let index = payload
                .get("contentBlockIndex")
                .and_then(|v| v.as_u64())
                .unwrap_or(0);

            // If this content block has a pending tool, finish it
            if state.tool_stream.name(index).is_some() {
                if let Some(tool_ev) = state.tool_stream.finish(index) {
                    state.has_tool_calls = true;
                    events.push(tool_ev);
                }
                return events;
            }

            // Close text block
            if state.text_started {
                events.push(LlmEvent::TextEnd {
                    id: format!("text-{index}"),
                    provider_metadata: None,
                });
                state.text_started = false;
            }

            // Close reasoning block with optional signature
            if state.reasoning_started {
                let metadata = state
                    .reasoning_signatures
                    .remove(&index)
                    .map(|sig| {
                        let mut m = HashMap::new();
                        m.insert(
                            "bedrock".into(),
                            serde_json::json!({"signature": sig}),
                        );
                        m
                    });

                events.push(LlmEvent::ReasoningEnd {
                    id: format!("reasoning-{index}"),
                    provider_metadata: metadata,
                });
                state.reasoning_started = false;
            }
        }

        "messageStop" => {
            let stop_reason = payload
                .get("stopReason")
                .and_then(|v| v.as_str())
                .unwrap_or("end_turn");
            let reason = map_finish_reason(stop_reason);
            state.pending_finish = Some(reason);
            state.seen_message_stop = true;

            // If we already have usage, emit terminal events
            if state.seen_metadata {
                events.extend(emit_terminal(state));
            }
        }

        "metadata" => {
            if let Some(usage_val) = payload.get("usage") {
                let usage = map_converse_usage(usage_val);
                state.pending_usage = Some(usage);
            }
            state.seen_metadata = true;

            // If we already have a stop reason, emit terminal events
            if state.seen_message_stop {
                events.extend(emit_terminal(state));
            }
        }

        // Error events
        "internalServerException" | "modelStreamErrorException"
        | "serviceUnavailableException" => {
            let msg = payload
                .get("message")
                .and_then(|v| v.as_str())
                .unwrap_or("Bedrock Converse stream error")
                .to_string();
            events.push(LlmEvent::ProviderErrorEvent {
                message: msg,
                classification: None,
                retryable: Some(true),
                provider_metadata: None,
            });
        }

        "validationException" => {
            let msg = payload
                .get("message")
                .and_then(|v| v.as_str())
                .unwrap_or("Bedrock Converse validation error")
                .to_string();
            let classification = if crate::error::is_context_overflow(&msg) {
                Some("context-overflow".to_string())
            } else {
                None
            };
            events.push(LlmEvent::ProviderErrorEvent {
                message: msg,
                classification,
                retryable: Some(false),
                provider_metadata: None,
            });
        }

        "throttlingException" => {
            let msg = payload
                .get("message")
                .and_then(|v| v.as_str())
                .unwrap_or("Bedrock Converse throttling error")
                .to_string();
            events.push(LlmEvent::ProviderErrorEvent {
                message: msg,
                classification: None,
                retryable: Some(true),
                provider_metadata: None,
            });
        }

        _ => {
            // Unknown event type — ignore
        }
    }

    events
}

/// Emit terminal events (StepFinish + Finish) from accumulated state.
///
/// Called when both `messageStop` (carries reason) and `metadata`
/// (carries usage) have arrived.
fn emit_terminal(state: &mut ConverseStreamState) -> Vec<LlmEvent> {
    let mut events = Vec::new();

    let reason = state.pending_finish.clone().unwrap_or(FinishReason::Stop);
    let usage = state.pending_usage.clone();

    // If stop reason is "stop" but we have tool calls, upgrade to "tool-calls"
    let effective_reason = if reason == FinishReason::Stop && state.has_tool_calls {
        FinishReason::ToolCalls
    } else {
        reason
    };

    // Close any remaining text/reasoning blocks
    if state.text_started {
        events.push(LlmEvent::TextEnd {
            id: "text-0".into(),
            provider_metadata: None,
        });
        state.text_started = false;
    }
    if state.reasoning_started {
        events.push(LlmEvent::ReasoningEnd {
            id: "reasoning-0".into(),
            provider_metadata: None,
        });
        state.reasoning_started = false;
    }

    // Finish any pending tool calls
    for tool_ev in state.tool_stream.finish_all() {
        state.has_tool_calls = true;
        events.push(tool_ev);
    }

    events.push(LlmEvent::StepFinish {
        index: 0,
        reason: effective_reason.clone(),
        usage: usage.clone(),
        provider_metadata: None,
    });
    events.push(LlmEvent::Finish {
        reason: effective_reason,
        usage,
        provider_metadata: None,
    });

    events
}

// ═════════════════════════════════════════════════════════════════════════
// 4. Streaming Response
// ═════════════════════════════════════════════════════════════════════════

/// Create a streaming LLM response from a Bedrock Converse HTTP response.
///
/// Decodes the AWS event-stream binary frames and drives the
/// `ConverseStreamState` machine to produce `LlmEvent`s.
pub fn create_converse_stream(
    response: reqwest::Response,
) -> Box<dyn futures::Stream<Item = Result<LlmEvent, Error>> + Send + Unpin> {
    let byte_stream = response.bytes_stream();

    let stream = futures::stream::unfold(
        (
            Box::pin(byte_stream)
                as Pin<
                    Box<
                        dyn futures::Stream<Item = Result<bytes::Bytes, reqwest::Error>>
                            + Send
                            + Unpin,
                    >,
                >,
            ConverseStreamState::new(),
            Vec::new(), // accumulated buffer bytes
            VecDeque::<Result<LlmEvent, Error>>::new(),
        ),
        |(mut stream, mut state, mut buffer, mut events_queue)| {
            Box::pin(async move {
                loop {
                    // Drain queued events
                    if let Some(ev) = events_queue.pop_front() {
                        return Some((ev, (stream, state, buffer, events_queue)));
                    }

                    // Try to decode frames from buffer
                    if !buffer.is_empty() {
                        match decode_aws_frame(&buffer) {
                            Ok(Some(frame)) => {
                                // Consume the frame from buffer
                                let total_length =
                                    u32::from_be_bytes([
                                        buffer[0], buffer[1], buffer[2], buffer[3],
                                    ]) as usize;
                                buffer.drain(..total_length);

                                // Skip non-event messages
                                if frame.event_type == "messageStart"
                                    || frame.event_type == "contentBlockStart"
                                    || frame.event_type == "contentBlockDelta"
                                    || frame.event_type == "contentBlockStop"
                                    || frame.event_type == "messageStop"
                                    || frame.event_type == "metadata"
                                    || frame.event_type == "internalServerException"
                                    || frame.event_type == "modelStreamErrorException"
                                    || frame.event_type == "validationException"
                                    || frame.event_type == "throttlingException"
                                    || frame.event_type == "serviceUnavailableException"
                                {
                                    let evs = events_from_converse(
                                        &frame.event_type,
                                        &frame.payload,
                                        &mut state,
                                    );
                                    for ev in evs {
                                        events_queue.push_back(Ok(ev));
                                    }
                                }

                                // If we got both messageStop and metadata, we're done
                                if state.seen_message_stop && state.seen_metadata {
                                    // Drain remaining events
                                    if let Some(ev) = events_queue.pop_front() {
                                        return Some((
                                            ev,
                                            (stream, state, buffer, events_queue),
                                        ));
                                    }
                                    return None;
                                }

                                // Loop to try next frame
                                continue;
                            }
                            Ok(None) => {
                                // Need more bytes — break to fetch from stream
                            }
                            Err(e) => {
                                buffer.clear();
                                return Some((
                                    Err(e),
                                    (stream, state, buffer, events_queue),
                                ));
                            }
                        }
                    }

                    // Fetch more bytes from the HTTP stream
                    match stream.next().await {
                        Some(Ok(chunk)) => {
                            buffer.extend_from_slice(&chunk);
                            continue;
                        }
                        Some(Err(e)) => {
                            return Some((
                                Err(Error::Network(format!(
                                    "Bedrock Converse stream read: {e}"
                                ))),
                                (stream, state, buffer, events_queue),
                            ));
                        }
                        None => {
                            // Stream ended — if we have pending finish, emit it
                            if state.seen_message_stop || state.seen_metadata {
                                let evs = emit_terminal(&mut state);
                                for ev in evs {
                                    events_queue.push_back(Ok(ev));
                                }
                                if let Some(ev) = events_queue.pop_front() {
                                    return Some((
                                        ev,
                                        (stream, state, buffer, events_queue),
                                    ));
                                }
                            }
                            return None;
                        }
                    }
                }
            })
        },
    );

    Box::new(stream)
}

// ═════════════════════════════════════════════════════════════════════════
// 5. BedrockConverseProvider
// ═════════════════════════════════════════════════════════════════════════

const CONVERSE_STREAM_PATH: &str = "/model";

/// Check whether the Converse API is available for a given model.
///
/// Bedrock Converse is available for Claude, Llama, and Titan models.
/// This is a heuristic based on model ID patterns.
pub fn supports_converse(model: &Model) -> bool {
    let id = model.api.id.to_lowercase();
    id.contains("claude")
        || id.contains("llama")
        || id.contains("titan")
        || id.contains("mistral")
        || id.contains("nova")
        || id.contains("jurassic")
        || id.contains("cohere")
}

#[derive(Debug)]
pub struct BedrockConverseProvider {
    api_key: String,
    secret_key: String,
    region: String,
    base_url: String,
    http_client: reqwest::Client,
    models: Vec<Model>,
}

impl BedrockConverseProvider {
    /// Create from `AWS_ACCESS_KEY_ID`, `AWS_SECRET_ACCESS_KEY`, and `AWS_REGION`.
    pub fn new() -> Result<Self, Error> {
        let api_key = resolve_api_key()?;
        let secret_key = resolve_secret_key()?;
        let region = resolve_region();
        let base_url = bedrock_base_url(&region);
        Self::with_base_url(api_key, secret_key, region, base_url)
    }

    /// Create with explicit credentials and base URL.
    pub fn with_base_url(
        api_key: String,
        secret_key: String,
        region: String,
        base_url: String,
    ) -> Result<Self, Error> {
        let http_client = reqwest::Client::builder()
            .user_agent(format!("rustcode/{}", env!("CARGO_PKG_VERSION")))
            .timeout(std::time::Duration::from_secs(300))
            .build()
            .map_err(|e| Error::Network(format!("HTTP client: {e}")))?;
        Ok(Self {
            api_key,
            secret_key,
            region,
            base_url,
            http_client,
            models: build_model_catalog(),
        })
    }

    fn converse_url(&self, model_id: &str) -> String {
        let encoded = urlencoding::encode(model_id);
        format!(
            "{}{}/{}",
            self.base_url.trim_end_matches('/'),
            CONVERSE_STREAM_PATH,
            encoded
        )
    }
}

fn build_model_catalog() -> Vec<Model> {
    vec![
        make_model(
            "claude-sonnet-4-20250514",
            "Claude Sonnet 4",
            200_000,
            8_192,
            "claude",
            true,
            true,
        ),
        make_model(
            "claude-opus-4-20250514",
            "Claude Opus 4",
            200_000,
            8_192,
            "claude",
            true,
            true,
        ),
        make_model(
            "claude-haiku-4-20250514",
            "Claude Haiku 4",
            200_000,
            8_192,
            "claude",
            true,
            false,
        ),
        make_model(
            "llama-4-maverick",
            "Llama 4 Maverick",
            128_000,
            4_096,
            "llama",
            true,
            false,
        ),
        make_model(
            "titan-text",
            "Titan Text",
            8_000,
            4_096,
            "titan",
            true,
            false,
        ),
    ]
}

fn make_model(
    id: &str,
    name: &str,
    ctx: u64,
    out: u64,
    family: &str,
    temperature: bool,
    reasoning: bool,
) -> Model {
    Model {
        id: id.into(),
        provider_id: "bedrock".into(),
        name: name.into(),
        api: crate::provider::ApiInfo {
            id: id.into(),
            url: "https://bedrock-runtime.us-east-1.amazonaws.com".into(),
            npm: "@ai-sdk/amazon-bedrock".into(),
        },
        family: Some(family.into()),
        capabilities: crate::provider::Capabilities {
            temperature,
            reasoning,
            attachment: false,
            toolcall: true,
            input: crate::provider::Modalities {
                text: true,
                ..Default::default()
            },
            output: crate::provider::Modalities {
                text: true,
                ..Default::default()
            },
            interleaved: Default::default(),
        },
        cost: crate::provider::Cost {
            input: 0.0,
            output: 0.0,
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
        release_date: "2025".into(),
        variants: None,
    }
}

#[async_trait]
impl Provider for BedrockConverseProvider {
    fn provider_id(&self) -> &str {
        "bedrock"
    }

    fn npm(&self) -> &str {
        "@ai-sdk/amazon-bedrock"
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
                provider_id: "bedrock".into(),
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
        let messages = crate::provider::normalize_messages(messages, model);
        let body = build_converse_body(model, &messages, tools);

        let body_json = serde_json::to_vec(&body)
            .map_err(|e| Error::Network(format!("Bedrock Converse body serialization: {e}")))?;

        let signer = SigV4Signer::new(&self.api_key, &self.secret_key, &self.region);
        let now = Utc::now().format("%Y%m%dT%H%M%SZ").to_string();
        let url_str = self.converse_url(&model.api.id);
        let url =
            url::Url::parse(&url_str).map_err(|e| Error::Network(format!("Bedrock URL parse: {e}")))?;
        let host = url.host_str().unwrap_or("");
        let path = url.path();

        let authorization = signer
            .sign("POST", host, path, &body_json, &now)
            .map_err(|e| Error::Network(format!("Bedrock SigV4: {e}")))?;

        let response = self
            .http_client
            .post(&url_str)
            .header("Content-Type", "application/json")
            .header("Authorization", authorization)
            .header("X-Amz-Date", &now)
            .header(
                "X-Amz-Content-Sha256",
                hex::encode(Sha256::digest(&body_json)),
            )
            .body(body_json)
            .send()
            .await
            .map_err(|e| Error::Network(format!("Bedrock Converse request: {e}")))?;

        if !response.status().is_success() {
            let status = response.status().as_u16();
            let text = response.text().await.unwrap_or_default();
            return Err(Error::Llm { http_context: None, 
                module: "bedrock-converse".into(),
                method: "stream".into(),
                reason: Box::new(classify_converse_error(status, &text)),
            });
        }

        Ok(create_converse_stream(response))
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

// ── Error Classification ───────────────────────────────────────────────

/// Classify a Converse HTTP error into a structured `LlmErrorReason`.
fn classify_converse_error(status: u16, body: &str) -> LlmErrorReason {
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

// ═════════════════════════════════════════════════════════════════════════
// Tests
// ═════════════════════════════════════════════════════════════════════════

#[cfg(test)]
mod tests {
    use super::*;
    use crate::provider::{ContentPart, MessageContent};

    // ── Model catalog ────────────────────────────────────────────

    #[test]
    fn test_model_catalog_count() {
        let models = build_model_catalog();
        assert_eq!(models.len(), 5);
    }

    #[test]
    fn test_model_catalog_ids() {
        let models = build_model_catalog();
        let ids: Vec<&str> = models.iter().map(|m| m.id.as_str()).collect();
        assert!(ids.contains(&"claude-sonnet-4-20250514"));
        assert!(ids.contains(&"claude-opus-4-20250514"));
        assert!(ids.contains(&"claude-haiku-4-20250514"));
        assert!(ids.contains(&"llama-4-maverick"));
        assert!(ids.contains(&"titan-text"));
    }

    // ── CRC32 ────────────────────────────────────────────────────

    #[test]
    fn test_crc32_known_values() {
        assert_eq!(crc32(b""), 0);
        assert_eq!(crc32(b"hello"), 0x3610a686);
        assert_eq!(crc32(b"123456789"), 0xcbf43926);
    }

    // ── map_finish_reason ────────────────────────────────────────

    #[test]
    fn test_map_finish_reason_end_turn() {
        assert_eq!(map_finish_reason("end_turn"), FinishReason::Stop);
    }

    #[test]
    fn test_map_finish_reason_stop_sequence() {
        assert_eq!(map_finish_reason("stop_sequence"), FinishReason::Stop);
    }

    #[test]
    fn test_map_finish_reason_max_tokens() {
        assert_eq!(map_finish_reason("max_tokens"), FinishReason::Length);
    }

    #[test]
    fn test_map_finish_reason_tool_use() {
        assert_eq!(map_finish_reason("tool_use"), FinishReason::ToolCalls);
    }

    #[test]
    fn test_map_finish_reason_content_filtered() {
        assert_eq!(
            map_finish_reason("content_filtered"),
            FinishReason::ContentFilter
        );
    }

    #[test]
    fn test_map_finish_reason_guardrail_intervened() {
        assert_eq!(
            map_finish_reason("guardrail_intervened"),
            FinishReason::ContentFilter
        );
    }

    #[test]
    fn test_map_finish_reason_unknown() {
        assert_eq!(
            map_finish_reason("some_other_reason"),
            FinishReason::Unknown
        );
    }

    // ── map_converse_usage ───────────────────────────────────────

    #[test]
    fn test_map_converse_usage_basic() {
        let usage_val = serde_json::json!({
            "inputTokens": 100,
            "outputTokens": 50,
            "totalTokens": 150
        });
        let usage = map_converse_usage(&usage_val);
        assert_eq!(usage.input_tokens, Some(100));
        assert_eq!(usage.output_tokens, Some(50));
        assert_eq!(usage.total_tokens, Some(150));
        assert_eq!(usage.non_cached_input_tokens, Some(100));
        assert!(usage.provider_metadata.is_some());
    }

    #[test]
    fn test_map_converse_usage_with_cache() {
        let usage_val = serde_json::json!({
            "inputTokens": 1000,
            "outputTokens": 500,
            "totalTokens": 1500,
            "cacheReadInputTokens": 300,
            "cacheWriteInputTokens": 100
        });
        let usage = map_converse_usage(&usage_val);
        assert_eq!(usage.input_tokens, Some(1000));
        assert_eq!(usage.cache_read_input_tokens, Some(300));
        assert_eq!(usage.cache_write_input_tokens, Some(100));
        assert_eq!(usage.non_cached_input_tokens, Some(600));
    }

    #[test]
    fn test_map_converse_usage_empty() {
        let usage_val = serde_json::json!({});
        let usage = map_converse_usage(&usage_val);
        assert_eq!(usage.input_tokens, None);
        assert_eq!(usage.output_tokens, None);
    }

    // ── build_converse_body ──────────────────────────────────────

    #[test]
    fn test_build_converse_body_basic() {
        let model = Model {
            id: "claude-sonnet-4-20250514".into(),
            provider_id: "bedrock".into(),
            name: "Claude Sonnet 4".into(),
            api: crate::provider::ApiInfo {
                id: "claude-sonnet-4-20250514".into(),
                url: "https://bedrock-runtime.us-east-1.amazonaws.com".into(),
                ..Default::default()
            },
            ..Default::default()
        };
        let msgs = vec![ChatMessage::User {
            content: MessageContent::Text("Hello".into()),
        }];
        let body = build_converse_body(&model, &msgs, &[]);
        assert_eq!(body["modelId"], "claude-sonnet-4-20250514");
        assert_eq!(body["messages"][0]["role"], "user");
        assert_eq!(body["messages"][0]["content"][0]["text"], "Hello");
    }

    #[test]
    fn test_build_converse_body_with_system() {
        let model = Model {
            id: "claude-sonnet-4-20250514".into(),
            ..Default::default()
        };
        let msgs = vec![
            ChatMessage::System {
                content: MessageContent::Text("You are helpful.".into()),
            },
            ChatMessage::User {
                content: MessageContent::Text("Hello".into()),
            },
        ];
        let body = build_converse_body(&model, &msgs, &[]);
        assert_eq!(body["system"][0]["text"], "You are helpful.");
        assert_eq!(body["messages"].as_array().unwrap().len(), 1);
    }

    #[test]
    fn test_build_converse_body_with_tools() {
        let model = Model {
            id: "claude-sonnet-4-20250514".into(),
            ..Default::default()
        };
        let msgs = vec![ChatMessage::User {
            content: MessageContent::Text("What's the weather?".into()),
        }];
        let tools = vec![ToolDefinition {
            name: "get_weather".into(),
            description: "Get weather for a city".into(),
            parameters: serde_json::json!({
                "type": "object",
                "properties": {
                    "city": {"type": "string"}
                }
            }),
        }];
        let body = build_converse_body(&model, &msgs, &tools);
        let tool_config = body.get("toolConfig").unwrap();
        assert_eq!(tool_config["tools"][0]["toolSpec"]["name"], "get_weather");
    }

    #[test]
    fn test_build_converse_body_with_assistant_tool_call() {
        let model = Model {
            id: "claude-sonnet-4-20250514".into(),
            ..Default::default()
        };
        let msgs = vec![ChatMessage::Assistant {
            content: MessageContent::Parts(vec![
                ContentPart::Text {
                    text: "Let me check.".into(),
                },
                ContentPart::ToolCallPart {
                    tool_call_id: "tc_01".into(),
                    tool_name: "get_weather".into(),
                    arguments: serde_json::json!({"city": "London"}),
                },
            ]),
        }];
        let body = build_converse_body(&model, &msgs, &[]);
        let content = body["messages"][0]["content"].as_array().unwrap();
        assert_eq!(content.len(), 2);
        assert_eq!(content[0]["text"], "Let me check.");
        assert_eq!(content[1]["toolUse"]["toolUseId"], "tc_01");
        assert_eq!(content[1]["toolUse"]["name"], "get_weather");
        assert_eq!(content[1]["toolUse"]["input"]["city"], "London");
    }

    #[test]
    fn test_build_converse_body_with_tool_result() {
        let model = Model {
            id: "claude-sonnet-4-20250514".into(),
            ..Default::default()
        };
        let msgs = vec![ChatMessage::Tool {
            content: vec![crate::provider::ToolResultPart::ToolResult {
                tool_call_id: "tc_01".into(),
                tool_name: "get_weather".into(),
                output: serde_json::json!({"temp": 72}),
                is_error: false,
            }],
        }];
        let body = build_converse_body(&model, &msgs, &[]);
        let content = body["messages"][0]["content"].as_array().unwrap();
        assert_eq!(content[0]["toolResult"]["toolUseId"], "tc_01");
        assert_eq!(content[0]["toolResult"]["status"], "success");
    }

    #[test]
    fn test_build_converse_body_with_reasoning() {
        let model = Model {
            id: "claude-sonnet-4-20250514".into(),
            ..Default::default()
        };
        let msgs = vec![ChatMessage::Assistant {
            content: MessageContent::Parts(vec![
                ContentPart::Reasoning {
                    text: "Let me think...".into(),
                    provider_options: None,
                },
                ContentPart::Text {
                    text: "The answer is 42.".into(),
                },
            ]),
        }];
        let body = build_converse_body(&model, &msgs, &[]);
        let content = body["messages"][0]["content"].as_array().unwrap();
        assert_eq!(content.len(), 2);
        assert!(content[0].get("reasoningContent").is_some());
        assert_eq!(
            content[0]["reasoningContent"]["reasoningText"]["text"],
            "Let me think..."
        );
        assert_eq!(content[1]["text"], "The answer is 42.");
    }

    // ── events_from_converse ─────────────────────────────────────

    #[test]
    fn test_events_from_converse_message_start() {
        let mut state = ConverseStreamState::new();
        let payload = serde_json::json!({"role": "assistant"});
        let events = events_from_converse("messageStart", &payload, &mut state);
        assert!(events.is_empty());
    }

    #[test]
    fn test_events_from_converse_text_delta() {
        let mut state = ConverseStreamState::new();
        let payload = serde_json::json!({
            "contentBlockIndex": 0,
            "delta": {"text": "Hello"}
        });
        let events = events_from_converse("contentBlockDelta", &payload, &mut state);
        assert!(events.iter().any(|e| matches!(e, LlmEvent::TextStart { .. })));
        assert!(events.iter().any(|e| matches!(e, LlmEvent::TextDelta { .. })));
        assert!(state.text_started);
    }

    #[test]
    fn test_events_from_converse_reasoning_delta() {
        let mut state = ConverseStreamState::new();
        let payload = serde_json::json!({
            "contentBlockIndex": 0,
            "delta": {
                "reasoningContent": {
                    "text": "Let me think..."
                }
            }
        });
        let events = events_from_converse("contentBlockDelta", &payload, &mut state);
        assert!(events
            .iter()
            .any(|e| matches!(e, LlmEvent::ReasoningStart { .. })));
        assert!(events
            .iter()
            .any(|e| matches!(e, LlmEvent::ReasoningDelta { .. })));
        assert!(state.reasoning_started);
    }

    #[test]
    fn test_events_from_converse_text_delta_multiple_chunks() {
        let mut state = ConverseStreamState::new();
        let p1 = serde_json::json!({
            "contentBlockIndex": 0,
            "delta": {"text": "Hello "}
        });
        let _ = events_from_converse("contentBlockDelta", &p1, &mut state);
        let p2 = serde_json::json!({
            "contentBlockIndex": 0,
            "delta": {"text": "World"}
        });
        let events = events_from_converse("contentBlockDelta", &p2, &mut state);
        assert!(!state.text_started); // Already started from p1
        assert!(events.iter().any(|e| matches!(e, LlmEvent::TextDelta { .. })));
    }

    #[test]
    fn test_events_from_converse_content_block_stop_text() {
        let mut state = ConverseStreamState::new();
        state.text_started = true;
        let payload = serde_json::json!({"contentBlockIndex": 0});
        let events = events_from_converse("contentBlockStop", &payload, &mut state);
        assert!(events.iter().any(|e| matches!(e, LlmEvent::TextEnd { .. })));
        assert!(!state.text_started);
    }

    #[test]
    fn test_events_from_converse_tool_use_start() {
        let mut state = ConverseStreamState::new();
        let payload = serde_json::json!({
            "contentBlockIndex": 0,
            "start": {
                "toolUse": {
                    "toolUseId": "toolu_01",
                    "name": "get_weather"
                }
            }
        });
        let events = events_from_converse("contentBlockStart", &payload, &mut state);
        assert!(events
            .iter()
            .any(|e| matches!(e, LlmEvent::ToolInputStart { .. })));
        assert!(state.step_started);
    }

    #[test]
    fn test_events_from_converse_tool_use_delta() {
        let mut state = ConverseStreamState::new();
        state
            .tool_stream
            .start(0, "get_weather", "toolu_01".to_string());

        let partial: serde_json::Value = serde_json::from_str(r#"{"city":""#).unwrap();
        let payload = serde_json::json!({
            "contentBlockIndex": 0,
            "delta": {
                "toolUse": {
                    "input": partial
                }
            }
        });
        let events = events_from_converse("contentBlockDelta", &payload, &mut state);
        assert!(events
            .iter()
            .any(|e| matches!(e, LlmEvent::ToolInputDelta { .. })));
    }

    #[test]
    fn test_events_from_converse_tool_use_stop() {
        let mut state = ConverseStreamState::new();
        state.tool_stream.start(0, "get_weather", "toolu_01");
        state.tool_stream.append(0, r#"{"city":"London"}"#);

        let payload = serde_json::json!({"contentBlockIndex": 0});
        let events = events_from_converse("contentBlockStop", &payload, &mut state);

        let tool_calls: Vec<_> = events
            .iter()
            .filter(|e| matches!(e, LlmEvent::ToolCall { .. }))
            .collect();
        assert_eq!(tool_calls.len(), 1);
        if let LlmEvent::ToolCall { name, input, .. } = tool_calls[0] {
            assert_eq!(name, "get_weather");
            assert_eq!(input["city"], "London");
        }
    }

    #[test]
    fn test_events_from_converse_message_stop() {
        let mut state = ConverseStreamState::new();
        let payload = serde_json::json!({"stopReason": "end_turn"});
        let events = events_from_converse("messageStop", &payload, &mut state);
        assert!(events.is_empty()); // Waiting for metadata
        assert!(state.seen_message_stop);
        assert_eq!(state.pending_finish, Some(FinishReason::Stop));
    }

    #[test]
    fn test_events_from_converse_metadata_triggers_finish() {
        let mut state = ConverseStreamState::new();
        state.seen_message_stop = true;
        state.pending_finish = Some(FinishReason::Stop);

        let payload = serde_json::json!({
            "usage": {
                "inputTokens": 10,
                "outputTokens": 5,
                "totalTokens": 15
            }
        });
        let events = events_from_converse("metadata", &payload, &mut state);
        assert!(events.iter().any(|e| matches!(e, LlmEvent::StepFinish { .. })));
        assert!(events.iter().any(|e| matches!(e, LlmEvent::Finish { .. })));
    }

    #[test]
    fn test_events_from_converse_metadata_then_message_stop() {
        let mut state = ConverseStreamState::new();
        let meta = serde_json::json!({
            "usage": {"inputTokens": 10, "outputTokens": 5}
        });
        let _ = events_from_converse("metadata", &meta, &mut state);
        assert!(state.seen_metadata);

        let stop = serde_json::json!({"stopReason": "end_turn"});
        let events = events_from_converse("messageStop", &stop, &mut state);
        assert!(events.iter().any(|e| matches!(e, LlmEvent::Finish { .. })));
    }

    #[test]
    fn test_events_from_converse_provider_error() {
        let mut state = ConverseStreamState::new();
        let payload = serde_json::json!({"message": "Internal failure"});
        let events =
            events_from_converse("internalServerException", &payload, &mut state);
        assert!(
            events
                .iter()
                .any(|e| matches!(e, LlmEvent::ProviderErrorEvent { .. }))
        );
    }

    #[test]
    fn test_events_from_converse_validation_error() {
        let mut state = ConverseStreamState::new();
        let payload =
            serde_json::json!({"message": "Input exceeds the context window"});
        let events = events_from_converse("validationException", &payload, &mut state);
        assert!(
            events
                .iter()
                .any(|e| matches!(e, LlmEvent::ProviderErrorEvent { .. }))
        );
    }

    #[test]
    fn test_events_from_converse_throttling() {
        let mut state = ConverseStreamState::new();
        let payload = serde_json::json!({"message": "Throttled"});
        let events = events_from_converse("throttlingException", &payload, &mut state);
        assert!(
            events
                .iter()
                .any(|e| matches!(e, LlmEvent::ProviderErrorEvent { .. }))
        );
    }

    // ── supports_converse ────────────────────────────────────────

    #[test]
    fn test_supports_converse_claude() {
        let model = Model {
            api: crate::provider::ApiInfo {
                id: "claude-sonnet-4-20250514".into(),
                ..Default::default()
            },
            ..Default::default()
        };
        assert!(supports_converse(&model));
    }

    #[test]
    fn test_supports_converse_llama() {
        let model = Model {
            api: crate::provider::ApiInfo {
                id: "llama-4-maverick".into(),
                ..Default::default()
            },
            ..Default::default()
        };
        assert!(supports_converse(&model));
    }

    #[test]
    fn test_supports_converse_titan() {
        let model = Model {
            api: crate::provider::ApiInfo {
                id: "titan-text".into(),
                ..Default::default()
            },
            ..Default::default()
        };
        assert!(supports_converse(&model));
    }

    // ── lower_messages helpers ────────────────────────────────────

    #[test]
    fn test_lower_messages_system_only() {
        let msgs = vec![ChatMessage::System {
            content: MessageContent::Text("Be helpful.".into()),
        }];
        let (system, converse) = lower_messages(&msgs);
        assert_eq!(system.len(), 1);
        assert_eq!(system[0]["text"], "Be helpful.");
        assert!(converse.is_empty());
    }

    #[test]
    fn test_lower_messages_user_with_image() {
        let msgs = vec![ChatMessage::User {
            content: MessageContent::Parts(vec![
                ContentPart::Text {
                    text: "What's this?".into(),
                },
                ContentPart::Image {
                    image: "data:image/png;base64,iVBOR".into(),
                },
            ]),
        }];
        let (_, converse) = lower_messages(&msgs);
        let content = converse[0]["content"].as_array().unwrap();
        assert_eq!(content.len(), 2);
        assert_eq!(content[0]["text"], "What's this?");
        assert_eq!(content[1]["image"]["format"], "png");
        assert_eq!(content[1]["image"]["source"]["bytes"], "iVBOR");
    }

    #[test]
    fn test_lower_messages_system_chronological() {
        let msgs = vec![
            ChatMessage::System {
                content: MessageContent::Text("Initial system.".into()),
            },
            ChatMessage::User {
                content: MessageContent::Text("Hi".into()),
            },
            ChatMessage::System {
                content: MessageContent::Text("Update.".into()),
            },
        ];
        let (system, converse) = lower_messages(&msgs);
        assert_eq!(system.len(), 1);
        assert_eq!(system[0]["text"], "Initial system.");
        // The chronological system update should be a user message with wrapping
        let last = converse.last().unwrap();
        assert_eq!(last["role"], "user");
        let text = last["content"][0]["text"].as_str().unwrap();
        assert!(text.contains("<system-update>"));
        assert!(text.contains("Update."));
        assert!(text.contains("</system-update>"));
    }

    // ── Provider trait ───────────────────────────────────────────

    #[test]
    fn test_provider_trait_provider_id() {
        let provider = BedrockConverseProvider::with_base_url(
            "test-ak".into(),
            "test-sk".into(),
            "us-east-1".into(),
            "https://bedrock-runtime.us-east-1.amazonaws.com".into(),
        )
        .unwrap();
        assert_eq!(provider.provider_id(), "bedrock");
        assert_eq!(provider.npm(), "@ai-sdk/amazon-bedrock");
    }

    #[test]
    fn test_converse_url_format() {
        let provider = BedrockConverseProvider::with_base_url(
            "ak".into(),
            "sk".into(),
            "us-west-2".into(),
            "https://bedrock-runtime.us-west-2.amazonaws.com".into(),
        )
        .unwrap();
        let url = provider.converse_url("claude-sonnet-4-20250514");
        assert_eq!(
            url,
            "https://bedrock-runtime.us-west-2.amazonaws.com/model/claude-sonnet-4-20250514"
        );
    }

    #[test]
    fn test_converse_url_with_slash_in_model() {
        let provider = BedrockConverseProvider::with_base_url(
            "ak".into(),
            "sk".into(),
            "us-east-1".into(),
            "https://bedrock-runtime.us-east-1.amazonaws.com".into(),
        )
        .unwrap();
        // Models with inference profiles like "anthropic.claude-sonnet-4-v1:0"
        let url = provider.converse_url("anthropic.claude-sonnet-4-v1:0");
        assert!(url.contains("/model/anthropic.claude-sonnet-4-v1%3A0"));
    }

    // ── Error classification ─────────────────────────────────────

    #[test]
    fn test_classify_converse_error_auth() {
        let reason = classify_converse_error(401, "Unauthorized");
        assert!(matches!(
            reason,
            LlmErrorReason::Authentication { .. }
        ));
    }

    #[test]
    fn test_classify_converse_error_rate_limit() {
        let reason = classify_converse_error(429, "Too fast");
        assert!(matches!(reason, LlmErrorReason::RateLimit { .. }));
    }

    #[test]
    fn test_classify_converse_error_context_overflow() {
        let reason =
            classify_converse_error(400, "Input exceeds the context window");
        assert!(matches!(
            reason,
            LlmErrorReason::InvalidRequest {
                classification: Some(ref c),
                ..
            } if c == "context-overflow"
        ));
    }

    #[test]
    fn test_classify_converse_error_server() {
        let reason = classify_converse_error(503, "Service unavailable");
        assert!(matches!(
            reason,
            LlmErrorReason::ProviderInternal { status: 503, .. }
        ));
    }
}
