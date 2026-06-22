//! LLM event publishing pipeline — translates raw [`LlmEvent`] stream events
//! into durable [`SessionEvent`] publications via [`EventV2`].
//!
//! Ported from: `packages/core/src/session/runner/publish-llm-event.ts`
//! BlazeCode commit: 5d0f86606ac30690f79f0a6a9f41a1f49fe95d0b

use std::collections::HashMap;
use std::sync::Arc;

use chrono::Utc;
use serde_json::json;

use crate::event::session_event_types;
use crate::event::{
    CacheTokens, EventDefinition, EventV2, ModelRef, PublishOptions, SessionEventBase,
    StepEndedEvent, StepFailedEvent, StepTokens, TextDeltaEvent,
    TextStartedEvent, ToolCalledEvent, ToolEventBase, ToolFailedEvent,
    ToolInputDeltaEvent, ToolInputStartedEvent, ToolProviderInfo,
    ToolSuccessEvent, ReasoningDeltaEvent, ReasoningEndedEvent, ReasoningStartedEvent,
    UnknownError,
};
use crate::provider::{FinishReason, LlmEvent, ToolOutput, Usage};

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

fn safe(value: Option<u64>) -> f64 {
    match value {
        Some(v) => {
            let f = v as f64;
            if f.is_finite() && f >= 0.0 {
                f
            } else {
                0.0
            }
        }
        None => 0.0,
    }
}

fn tokens_from_usage(usage: Option<&Usage>) -> StepTokens {
    let u = match usage {
        Some(u) => u,
        None => {
            return StepTokens {
                input: 0.0,
                output: 0.0,
                reasoning: 0.0,
                cache: CacheTokens {
                    read: 0.0,
                    write: 0.0,
                },
            }
        }
    };
    StepTokens {
        input: safe(u.non_cached_input_tokens),
        output: safe(Some(u.visible_output_tokens())),
        reasoning: safe(u.reasoning_tokens),
        cache: CacheTokens {
            read: safe(u.cache_read_input_tokens),
            write: safe(u.cache_write_input_tokens),
        },
    }
}

fn record_value(value: &serde_json::Value) -> serde_json::Value {
    if let serde_json::Value::Object(_) = value {
        value.clone()
    } else {
        json!({ "value": value })
    }
}

#[allow(dead_code)]
fn message_value(value: &serde_json::Value) -> String {
    match value {
        serde_json::Value::String(s) => s.clone(),
        other => serde_json::to_string(other).unwrap_or_else(|_| format!("{other:?}")),
    }
}

// ---------------------------------------------------------------------------
// SettledOutput
// ---------------------------------------------------------------------------

enum SettledOutput {
    Success {
        structured: serde_json::Value,
        content: serde_json::Value,
    },
    Error {
        error_type: String,
        message: String,
    },
}

fn settled_output(output: &Option<ToolOutput>, result: &serde_json::Value) -> SettledOutput {
    if result.get("type").and_then(|t| t.as_str()) == Some("error") {
        let msg = result
            .get("value")
            .and_then(|v| v.as_str())
            .map(|s| s.to_string())
            .unwrap_or_else(|| message_value(result));
        return SettledOutput::Error {
            error_type: "unknown".to_string(),
            message: msg,
        };
    }
    if let Some(out) = output {
        SettledOutput::Success {
            structured: record_value(&out.content),
            content: out.content.clone(),
        }
    } else {
        SettledOutput::Success {
            structured: record_value(result),
            content: result.clone(),
        }
    }
}

// ---------------------------------------------------------------------------
// Tool tracking state
// ---------------------------------------------------------------------------

struct ToolEntry {
    assistant_message_id: String,
    name: String,
    input_ended: bool,
    called: bool,
    settled: bool,
    provider_executed: bool,
    provider_metadata: Option<serde_json::Value>,
}

// ---------------------------------------------------------------------------
// FragmentAccumulator
// ---------------------------------------------------------------------------

struct FragmentAccumulator {
    chunks: HashMap<String, Vec<String>>,
    name: &'static str,
}

impl FragmentAccumulator {
    fn new(name: &'static str) -> Self {
        Self {
            chunks: HashMap::new(),
            name,
        }
    }

    fn start(&mut self, id: &str) {
        if self.chunks.contains_key(id) {
            tracing::warn!("Duplicate {} start: {}", self.name, id);
            return;
        }
        self.chunks.insert(id.to_string(), Vec::new());
    }

    fn append(&mut self, id: &str, value: &str) {
        match self.chunks.get_mut(id) {
            Some(chunks) => chunks.push(value.to_string()),
            None => tracing::warn!("{} delta before start: {}", self.name, id),
        }
    }

    fn end(&mut self, id: &str) -> Option<String> {
        let current = self.chunks.remove(id);
        match current {
            Some(chunks) => Some(chunks.join("")),
            None => {
                tracing::warn!("{} end before start: {}", self.name, id);
                None
            }
        }
    }

    fn flush(&mut self) {
        let ids: Vec<String> = self.chunks.keys().cloned().collect();
        for id in ids {
            self.end(&id);
        }
    }
}

// ---------------------------------------------------------------------------
// LlmEventPublisher
// ---------------------------------------------------------------------------

pub struct LlmEventPublisher {
    events: Arc<EventV2>,
    session_id: String,
    _agent: String,
    _model: ModelRef,

    text: FragmentAccumulator,
    reasoning: FragmentAccumulator,
    tool_input: FragmentAccumulator,

    tools: HashMap<String, ToolEntry>,

    assistant_message_id: Option<String>,
    provider_failed: bool,
}

impl LlmEventPublisher {
    pub fn new(events: Arc<EventV2>, session_id: String, agent: String, model: ModelRef) -> Self {
        Self {
            events,
            session_id,
            _agent: agent,
            _model: model,
            text: FragmentAccumulator::new("text"),
            reasoning: FragmentAccumulator::new("reasoning"),
            tool_input: FragmentAccumulator::new("tool input"),
            tools: HashMap::new(),
            assistant_message_id: None,
            provider_failed: false,
        }
    }

    // ── Internal helpers ────────────────────────────────────────────────

    fn session_event_base(&self) -> SessionEventBase {
        SessionEventBase {
            timestamp: Utc::now().timestamp_millis() as u64,
            session_id: self.session_id.clone(),
        }
    }

    fn ensure_assistant_message_id(&mut self) -> String {
        if self.assistant_message_id.is_none() {
            self.assistant_message_id = Some(
                crate::id::ascending(crate::id::IdPrefix::Session, None)
                    .unwrap_or_default(),
            );
        }
        self.assistant_message_id
            .clone()
            .expect("assistant_message_id must be set")
    }

    fn current_assistant_message_id(&self) -> Result<&str, &'static str> {
        self.assistant_message_id
            .as_deref()
            .ok_or("Tool event before assistant step start")
    }

    fn sync_def(event_type: &str, version: u32) -> EventDefinition {
        EventDefinition::new(
            event_type,
            Some(crate::event::SyncConfig {
                version,
                aggregate: "sessionID".to_string(),
            }),
            serde_json::Value::Null,
        )
    }

    fn ephemeral_def(event_type: &str) -> EventDefinition {
        EventDefinition::new(event_type, None, serde_json::Value::Null)
    }

    async fn publish_data(&self, definition: &EventDefinition, data: serde_json::Value) {
        if let Err(e) = self
            .events
            .publish(definition, data, Some(PublishOptions::default()))
            .await
        {
            tracing::error!(
                event_type = %definition.event_type,
                error = %e,
                "failed to publish LLM event"
            );
        }
    }

    fn epoch_ms() -> u64 {
        Utc::now().timestamp_millis() as u64
    }

    // ── Public API ──────────────────────────────────────────────────────

    pub fn has_assistant_started(&self) -> bool {
        self.assistant_message_id.is_some()
    }

    pub fn has_provider_error(&self) -> bool {
        self.provider_failed
    }

    pub async fn publish(&mut self, event: &LlmEvent, output_paths: &[String]) {
        match event {
            LlmEvent::StepStart { .. } => {}

            LlmEvent::TextStart { id, .. } => {
                self.text.start(id.as_str());
                let session_id = self.session_id.clone();
                let assistant_message_id = self.ensure_assistant_message_id();
                let data = serde_json::to_value(TextStartedEvent {
                    base: SessionEventBase {
                        timestamp: Self::epoch_ms(),
                        session_id,
                    },
                    assistant_message_id,
                    text_id: id.clone(),
                })
                .unwrap_or_default();
                let def = Self::sync_def(session_event_types::TEXT_STARTED, 1);
                self.publish_data(&def, data).await;
            }

            LlmEvent::TextDelta { id, text, .. } => {
                self.text.append(id.as_str(), text.as_str());
                let assistant_message_id = match self.current_assistant_message_id() {
                    Ok(id) => id,
                    Err(_) => return,
                };
                let data = serde_json::to_value(TextDeltaEvent {
                    base: self.session_event_base(),
                    assistant_message_id: assistant_message_id.to_string(),
                    text_id: id.clone(),
                    delta: text.clone(),
                })
                .unwrap_or_default();
                let def = Self::ephemeral_def(session_event_types::TEXT_DELTA);
                self.publish_data(&def, data).await;
            }

            LlmEvent::TextEnd { id, .. } => {
                self.text.end(id.as_str());
            }

            LlmEvent::ReasoningStart { id, .. } => {
                self.reasoning.start(id.as_str());
                let session_id = self.session_id.clone();
                let assistant_message_id = self.ensure_assistant_message_id();
                let provider_metadata = event
                    .provider_metadata_ref()
                    .cloned()
                    .map(|m| serde_json::Value::Object(m.into_iter().collect()));
                let data = serde_json::to_value(ReasoningStartedEvent {
                    base: SessionEventBase {
                        timestamp: Self::epoch_ms(),
                        session_id,
                    },
                    assistant_message_id,
                    reasoning_id: id.clone(),
                    provider_metadata,
                })
                .unwrap_or_default();
                let def = Self::sync_def(session_event_types::REASONING_STARTED, 1);
                self.publish_data(&def, data).await;
            }

            LlmEvent::ReasoningDelta { id, text, .. } => {
                self.reasoning.append(id.as_str(), text.as_str());
                let assistant_message_id = match self.current_assistant_message_id() {
                    Ok(id) => id,
                    Err(_) => return,
                };
                let data = serde_json::to_value(ReasoningDeltaEvent {
                    base: self.session_event_base(),
                    assistant_message_id: assistant_message_id.to_string(),
                    reasoning_id: id.clone(),
                    delta: text.clone(),
                })
                .unwrap_or_default();
                let def = Self::ephemeral_def(session_event_types::REASONING_DELTA);
                self.publish_data(&def, data).await;
            }

            LlmEvent::ReasoningEnd {
                id,
                provider_metadata,
            } => {
                let joined = self.reasoning.end(id.as_str());
                let provider_metadata_val = provider_metadata
                    .clone()
                    .map(|m| serde_json::Value::Object(m.into_iter().collect()));
                let assistant_message_id = match self.current_assistant_message_id() {
                    Ok(id) => id.to_string(),
                    Err(_) => return,
                };
                let session_id = self.session_id.clone();
                let data = serde_json::to_value(ReasoningEndedEvent {
                    base: SessionEventBase {
                        timestamp: Self::epoch_ms(),
                        session_id,
                    },
                    assistant_message_id,
                    reasoning_id: id.clone(),
                    text: joined.unwrap_or_default(),
                    provider_metadata: provider_metadata_val,
                })
                .unwrap_or_default();
                let def = Self::sync_def(session_event_types::REASONING_ENDED, 1);
                self.publish_data(&def, data).await;
            }

            LlmEvent::ToolInputStart { id, name, .. } => {
                let assistant_message_id =
                    match self.start_tool_input(id.as_str(), name.as_str()) {
                        Ok(aid) => aid,
                        Err(e) => {
                            tracing::warn!("{e}: {id}");
                            return;
                        }
                    };
                let data = serde_json::to_value(ToolInputStartedEvent {
                    tool_base: ToolEventBase {
                        base: self.session_event_base(),
                        assistant_message_id,
                        call_id: id.clone(),
                    },
                    name: name.clone(),
                })
                .unwrap_or_default();
                let def = Self::sync_def(session_event_types::TOOL_INPUT_STARTED, 1);
                self.publish_data(&def, data).await;
            }

            LlmEvent::ToolInputDelta { id, name, text } => {
                let (valid, assistant_message_id) = {
                    let tool = match self.tools.get(id.as_str()) {
                        Some(t) => t,
                        None => {
                            tracing::warn!("Tool input delta before start: {id}");
                            return;
                        }
                    };
                    if tool.name != *name {
                        tracing::warn!(
                            "Tool input name changed for {id}: {} -> {name}",
                            tool.name
                        );
                        return;
                    }
                    if tool.input_ended {
                        tracing::warn!("Tool input delta after end: {id}");
                        return;
                    }
                    (true, tool.assistant_message_id.clone())
                };
                if !valid {
                    return;
                }
                self.tool_input.append(id.as_str(), text.as_str());
                let data = serde_json::to_value(ToolInputDeltaEvent {
                    tool_base: ToolEventBase {
                        base: self.session_event_base(),
                        assistant_message_id,
                        call_id: id.clone(),
                    },
                    delta: text.clone(),
                })
                .unwrap_or_default();
                let def = Self::ephemeral_def(session_event_types::TOOL_INPUT_DELTA);
                self.publish_data(&def, data).await;
            }

            LlmEvent::ToolInputEnd { id, name, .. } => {
                if let Err(e) = self.end_tool_input(id.as_str(), name.as_str()) {
                    tracing::warn!("{e}: {id}");
                }
            }

            LlmEvent::ToolCall {
                id,
                name,
                input,
                provider_executed,
                provider_metadata,
            } => {
                if !self.tools.contains_key(id.as_str()) {
                    if let Err(e) = self.start_tool_input(id.as_str(), name.as_str()) {
                        tracing::warn!("{e}: {id}");
                        return;
                    }
                }
                {
                    let should_end = self
                        .tools
                        .get(id.as_str())
                        .map(|t| !t.input_ended)
                        .unwrap_or(false);
                    if should_end {
                        if let Err(e) = self.end_tool_input(id.as_str(), name.as_str()) {
                            tracing::warn!("{e}: {id}");
                            return;
                        }
                    }
                }
                let (assistant_message_id, provider_executed_val, provider_metadata_val) = {
                    let entry = match self.tools.get_mut(id.as_str()) {
                        Some(e) => e,
                        None => return,
                    };
                    if entry.name != *name {
                        tracing::warn!(
                            "Tool call name changed for {id}: {} -> {name}",
                            entry.name
                        );
                        return;
                    }
                    if entry.called {
                        tracing::warn!("Duplicate tool call: {id}");
                        return;
                    }
                    entry.called = true;
                    entry.provider_executed = provider_executed.unwrap_or(false);
                    entry.provider_metadata = provider_metadata
                        .clone()
                        .map(|m| serde_json::Value::Object(m.into_iter().collect()));
                    (
                        entry.assistant_message_id.clone(),
                        entry.provider_executed,
                        entry.provider_metadata.clone(),
                    )
                };
                let provider = ToolProviderInfo {
                    executed: provider_executed_val,
                    metadata: provider_metadata_val,
                };
                let data = serde_json::to_value(ToolCalledEvent {
                    tool_base: ToolEventBase {
                        base: self.session_event_base(),
                        assistant_message_id,
                        call_id: id.clone(),
                    },
                    tool: name.clone(),
                    input: record_value(input),
                    provider,
                })
                .unwrap_or_default();
                let def = Self::sync_def(session_event_types::TOOL_CALLED, 1);
                self.publish_data(&def, data).await;
            }

            LlmEvent::ToolResult {
                id,
                name,
                result,
                output,
                provider_executed,
                provider_metadata,
            } => {
                let (
                    assistant_message_id,
                    provider_executed_final,
                    provider_metadata_final,
                    settled,
                ) = {
                    let entry = match self.tools.get_mut(id.as_str()) {
                        Some(e) if e.called => e,
                        _ => {
                            tracing::warn!("Tool result before call: {id}");
                            return;
                        }
                    };
                    if entry.name != *name {
                        tracing::warn!(
                            "Tool result name changed for {id}: {} -> {name}",
                            entry.name
                        );
                        return;
                    }
                    if entry.settled {
                        if result.get("type").and_then(|t| t.as_str()) != Some("error") {
                            tracing::warn!("Duplicate tool result: {id}");
                        }
                        return;
                    }
                    entry.settled = true;
                    let settled = settled_output(output, result);
                    let provider_executed_final =
                        provider_executed.unwrap_or(false) || entry.provider_executed;
                    let provider_metadata_final = provider_metadata
                        .clone()
                        .map(|m| serde_json::Value::Object(m.into_iter().collect()))
                        .or_else(|| entry.provider_metadata.clone());
                    (
                        entry.assistant_message_id.clone(),
                        provider_executed_final,
                        provider_metadata_final,
                        settled,
                    )
                };

                let provider = ToolProviderInfo {
                    executed: provider_executed_final,
                    metadata: provider_metadata_final,
                };

                match settled {
                    SettledOutput::Error {
                        error_type,
                        message,
                    } => {
                        let data = serde_json::to_value(ToolFailedEvent {
                            tool_base: ToolEventBase {
                                base: self.session_event_base(),
                                assistant_message_id,
                                call_id: id.clone(),
                            },
                            error: UnknownError {
                                error_type,
                                message,
                            },
                            result: Some(result.clone()),
                            provider,
                        })
                        .unwrap_or_default();
                        let def = Self::sync_def(session_event_types::TOOL_FAILED, 1);
                        self.publish_data(&def, data).await;
                    }
                    SettledOutput::Success {
                        structured,
                        content,
                    } => {
                        let data = serde_json::to_value(ToolSuccessEvent {
                            tool_base: ToolEventBase {
                                base: self.session_event_base(),
                                assistant_message_id,
                                call_id: id.clone(),
                            },
                            structured,
                            content: vec![content],
                            output_paths: if output_paths.is_empty() {
                                None
                            } else {
                                Some(output_paths.to_vec())
                            },
                            result: if provider_executed_final {
                                Some(result.clone())
                            } else {
                                None
                            },
                            provider,
                        })
                        .unwrap_or_default();
                        let def = Self::sync_def(session_event_types::TOOL_SUCCESS, 1);
                        self.publish_data(&def, data).await;
                    }
                }
            }

            LlmEvent::ToolError {
                id,
                name,
                message,
                ..
            } => {
                let assistant_message_id = {
                    let entry = match self.tools.get_mut(id.as_str()) {
                        Some(e) if e.called => e,
                        _ => {
                            tracing::warn!("Tool error before call: {id}");
                            return;
                        }
                    };
                    if entry.name != *name {
                        tracing::warn!(
                            "Tool error name changed for {id}: {} -> {name}",
                            entry.name
                        );
                        return;
                    }
                    if entry.settled {
                        tracing::warn!("Duplicate tool error: {id}");
                        return;
                    }
                    entry.settled = true;
                    (
                        entry.assistant_message_id.clone(),
                        entry.provider_executed,
                        entry.provider_metadata.clone(),
                    )
                };
                let provider = ToolProviderInfo {
                    executed: assistant_message_id.1,
                    metadata: assistant_message_id.2,
                };
                let data = serde_json::to_value(ToolFailedEvent {
                    tool_base: ToolEventBase {
                        base: self.session_event_base(),
                        assistant_message_id: assistant_message_id.0,
                        call_id: id.clone(),
                    },
                    error: UnknownError {
                        error_type: "unknown".to_string(),
                        message: message.clone(),
                    },
                    result: None,
                    provider,
                })
                .unwrap_or_default();
                let def = Self::sync_def(session_event_types::TOOL_FAILED, 1);
                self.publish_data(&def, data).await;
            }

            LlmEvent::StepFinish {
                reason, usage, ..
            } => {
                self.flush_fragments();
                let assistant_message_id = self.ensure_assistant_message_id();
                let finish_reason = match reason {
                    FinishReason::Stop => "stop",
                    FinishReason::Length => "length",
                    FinishReason::ToolCalls => "tool-calls",
                    FinishReason::ContentFilter => "content-filter",
                    FinishReason::Error => "error",
                    FinishReason::Unknown => "unknown",
                };
                let data = serde_json::to_value(StepEndedEvent {
                    base: self.session_event_base(),
                    assistant_message_id,
                    finish: finish_reason.to_string(),
                    cost: 0.0,
                    tokens: tokens_from_usage(usage.as_ref()),
                    snapshot: None,
                })
                .unwrap_or_default();
                let def = Self::sync_def(session_event_types::STEP_ENDED, 2);
                self.publish_data(&def, data).await;
            }

            LlmEvent::Finish { .. } => {}

            LlmEvent::ProviderErrorEvent { message, .. } => {
                self.provider_failed = true;
                self.flush_fragments();
                let assistant_message_id = self.ensure_assistant_message_id();
                let data = serde_json::to_value(StepFailedEvent {
                    base: self.session_event_base(),
                    assistant_message_id,
                    error: UnknownError {
                        error_type: "unknown".to_string(),
                        message: message.clone(),
                    },
                })
                .unwrap_or_default();
                let def = Self::sync_def(session_event_types::STEP_FAILED, 2);
                self.publish_data(&def, data).await;
            }
        }
    }

    pub fn flush(&mut self) {
        self.flush_fragments();
    }

    pub async fn fail_unsettled_tools(&mut self, message: &str, hosted_only: bool) {
        let call_ids: Vec<String> = self.tools.keys().cloned().collect();
        for call_id in &call_ids {
            let (assistant_message_id, provider_executed, provider_metadata) = {
                let entry = match self.tools.get_mut(call_id.as_str()) {
                    Some(e) if !e.settled => e,
                    _ => continue,
                };
                if hosted_only && !entry.provider_executed {
                    continue;
                }
                entry.settled = true;
                (
                    entry.assistant_message_id.clone(),
                    entry.provider_executed,
                    entry.provider_metadata.clone(),
                )
            };
            let provider = ToolProviderInfo {
                executed: provider_executed,
                metadata: provider_metadata,
            };
            let data = serde_json::to_value(ToolFailedEvent {
                tool_base: ToolEventBase {
                    base: self.session_event_base(),
                    assistant_message_id,
                    call_id: call_id.clone(),
                },
                error: UnknownError {
                    error_type: "unknown".to_string(),
                    message: message.to_string(),
                },
                result: None,
                provider,
            })
            .unwrap_or_default();
            let def = Self::sync_def(session_event_types::TOOL_FAILED, 1);
            self.publish_data(&def, data).await;
        }
    }

    pub fn assistant_message_id_for_tool(&self, call_id: &str) -> Option<&str> {
        self.tools
            .get(call_id)
            .map(|t| t.assistant_message_id.as_str())
    }

    // ── Private helpers ─────────────────────────────────────────────────

    fn start_tool_input(&mut self, id: &str, name: &str) -> Result<String, &'static str> {
        if self.tools.contains_key(id) {
            return Err("Duplicate tool input start");
        }
        let assistant_message_id = self.ensure_assistant_message_id();
        self.tools.insert(
            id.to_string(),
            ToolEntry {
                assistant_message_id: assistant_message_id.clone(),
                name: name.to_string(),
                input_ended: false,
                called: false,
                settled: false,
                provider_executed: false,
                provider_metadata: None,
            },
        );
        self.tool_input.start(id);
        Ok(assistant_message_id)
    }

    fn end_tool_input(&mut self, id: &str, name: &str) -> Result<(), &'static str> {
        let can_end = self
            .tools
            .get(id)
            .map_or(false, |t| t.name == name && !t.input_ended);
        if !can_end {
            match self.tools.get(id) {
                None => return Err("Tool input end before start"),
                Some(t) if t.name != name => return Err("Tool input name changed"),
                Some(_) => return Err("Duplicate tool input end"),
            }
        }
        self.tool_input.end(id);
        if let Some(t) = self.tools.get_mut(id) {
            t.input_ended = true;
        }
        Ok(())
    }

    fn flush_fragments(&mut self) {
        self.text.flush();
        self.reasoning.flush();
        self.tool_input.flush();
    }
}

// ---------------------------------------------------------------------------
// Helper to extract provider_metadata from LlmEvent variants
// ---------------------------------------------------------------------------

trait LlmEventProviderMetadata {
    fn provider_metadata_ref(&self) -> Option<&HashMap<String, serde_json::Value>>;
}

impl LlmEventProviderMetadata for LlmEvent {
    fn provider_metadata_ref(&self) -> Option<&HashMap<String, serde_json::Value>> {
        match self {
            Self::TextStart {
                provider_metadata, ..
            }
            | Self::TextDelta {
                provider_metadata, ..
            }
            | Self::TextEnd {
                provider_metadata, ..
            }
            | Self::ReasoningStart {
                provider_metadata, ..
            }
            | Self::ReasoningDelta {
                provider_metadata, ..
            }
            | Self::ReasoningEnd {
                provider_metadata, ..
            }
            | Self::ToolInputStart {
                provider_metadata, ..
            }
            | Self::ToolInputEnd {
                provider_metadata, ..
            }
            | Self::ToolCall {
                provider_metadata, ..
            }
            | Self::ToolResult {
                provider_metadata, ..
            }
            | Self::ToolError {
                provider_metadata, ..
            }
            | Self::StepFinish {
                provider_metadata, ..
            }
            | Self::Finish {
                provider_metadata, ..
            }
            | Self::ProviderErrorEvent {
                provider_metadata, ..
            } => provider_metadata.as_ref(),
            Self::ToolInputDelta { .. } | Self::StepStart { .. } => None,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::event::EventV2;
    use crate::provider::LlmEvent;
    use serde_json::json;

    fn make_publisher() -> (Arc<EventV2>, LlmEventPublisher) {
        let ev = Arc::new(EventV2::new(256, None));
        let model = ModelRef {
            id: "gpt-4".to_string(),
            provider_id: "openai".to_string(),
        };
        let pub_ = LlmEventPublisher::new(
            ev.clone(),
            "ses_test".to_string(),
            "default".to_string(),
            model,
        );
        (ev, pub_)
    }

    #[tokio::test]
    async fn test_text_start_delta_end() {
        let (ev, mut pub_) = make_publisher();
        let mut sub = ev.subscribe(session_event_types::TEXT_STARTED).await;
        let mut delta_sub = ev.subscribe(session_event_types::TEXT_DELTA).await;
        let mut ended_sub = ev.subscribe(session_event_types::TEXT_ENDED).await;

        pub_
            .publish(
                &LlmEvent::TextStart {
                    id: "txt_1".into(),
                    provider_metadata: None,
                },
                &[],
            )
            .await;

        let started = sub.recv().await.unwrap();
        assert_eq!(started.event_type, "session.next.text.started");
        assert_eq!(started.data["textID"], "txt_1");

        pub_
            .publish(
                &LlmEvent::TextDelta {
                    id: "txt_1".into(),
                    text: "Hello ".into(),
                    provider_metadata: None,
                },
                &[],
            )
            .await;

        let delta = delta_sub.recv().await.unwrap();
        assert_eq!(delta.data["delta"], "Hello ");

        pub_
            .publish(
                &LlmEvent::TextDelta {
                    id: "txt_1".into(),
                    text: "world".into(),
                    provider_metadata: None,
                },
                &[],
            )
            .await;

        let delta2 = delta_sub.recv().await.unwrap();
        assert_eq!(delta2.data["delta"], "world");

        pub_
            .publish(
                &LlmEvent::TextEnd {
                    id: "txt_1".into(),
                    provider_metadata: None,
                },
                &[],
            )
            .await;

        let ended = ended_sub.recv().await.unwrap();
        assert_eq!(ended.event_type, "session.next.text.ended");
        assert_eq!(ended.data["text"], "Hello world");
    }

    #[tokio::test]
    async fn test_tool_lifecycle() {
        let (ev, mut pub_) = make_publisher();
        let mut input_started = ev.subscribe(session_event_types::TOOL_INPUT_STARTED).await;
        let mut input_delta = ev.subscribe(session_event_types::TOOL_INPUT_DELTA).await;
        let mut input_ended = ev.subscribe(session_event_types::TOOL_INPUT_ENDED).await;
        let mut called = ev.subscribe(session_event_types::TOOL_CALLED).await;
        let mut success = ev.subscribe(session_event_types::TOOL_SUCCESS).await;

        pub_
            .publish(
                &LlmEvent::ToolInputStart {
                    id: "call_1".into(),
                    name: "bash".into(),
                    provider_metadata: None,
                },
                &[],
            )
            .await;

        let started = input_started.recv().await.unwrap();
        assert_eq!(started.data["name"], "bash");

        pub_
            .publish(
                &LlmEvent::ToolInputDelta {
                    id: "call_1".into(),
                    name: "bash".into(),
                    text: r#"{"cmd":"#.into(),
                },
                &[],
            )
            .await;

        let delta = input_delta.recv().await.unwrap();
        assert_eq!(delta.data["delta"], r#"{"cmd":"#);

        pub_
            .publish(
                &LlmEvent::ToolInputEnd {
                    id: "call_1".into(),
                    name: "bash".into(),
                    provider_metadata: None,
                },
                &[],
            )
            .await;

        let ended = input_ended.recv().await.unwrap();
        assert_eq!(ended.event_type, "session.next.tool.input.ended");
        assert_eq!(ended.data["text"], r#"{"cmd":"#);

        pub_
            .publish(
                &LlmEvent::ToolCall {
                    id: "call_1".into(),
                    name: "bash".into(),
                    input: json!({"cmd": "ls"}),
                    provider_executed: Some(false),
                    provider_metadata: None,
                },
                &[],
            )
            .await;

        let call = called.recv().await.unwrap();
        assert_eq!(call.data["tool"], "bash");
        assert_eq!(call.data["input"]["cmd"], "ls");

        pub_
            .publish(
                &LlmEvent::ToolResult {
                    id: "call_1".into(),
                    name: "bash".into(),
                    result: json!({"type": "value", "value": "file1\nfile2"}),
                    output: Some(ToolOutput {
                        structured: true,
                        content: json!({"result": "file1\nfile2"}),
                    }),
                    provider_executed: Some(false),
                    provider_metadata: None,
                },
                &["/tmp/out.txt".to_string()],
            )
            .await;

        let success_ev = success.recv().await.unwrap();
        assert_eq!(success_ev.data["tool_base"]["callID"], "call_1");
    }

    #[tokio::test]
    async fn test_step_lifecycle() {
        let (ev, mut pub_) = make_publisher();
        let mut step_started = ev.subscribe(session_event_types::STEP_STARTED).await;
        let mut step_ended = ev.subscribe(session_event_types::STEP_ENDED).await;

        pub_
            .publish(
                &LlmEvent::TextStart {
                    id: "txt_1".into(),
                    provider_metadata: None,
                },
                &[],
            )
            .await;

        let started = step_started.recv().await.unwrap();
        assert_eq!(started.event_type, "session.next.step.started");
        assert!(started.data["assistantMessageID"].is_string());

        pub_
            .publish(
                &LlmEvent::StepFinish {
                    index: 0,
                    reason: FinishReason::Stop,
                    usage: Some(crate::provider::Usage {
                        input_tokens: Some(100),
                        output_tokens: Some(50),
                        ..Default::default()
                    }),
                    provider_metadata: None,
                },
                &[],
            )
            .await;

        let ended = step_ended.recv().await.unwrap();
        assert_eq!(ended.event_type, "session.next.step.ended");
        assert_eq!(ended.data["finish"], "stop");
    }

    #[tokio::test]
    async fn test_provider_error() {
        let (ev, mut pub_) = make_publisher();
        let mut step_failed = ev.subscribe(session_event_types::STEP_FAILED).await;

        pub_
            .publish(
                &LlmEvent::TextStart {
                    id: "txt_1".into(),
                    provider_metadata: None,
                },
                &[],
            )
            .await;

        pub_
            .publish(
                &LlmEvent::ProviderErrorEvent {
                    message: "rate limited".into(),
                    classification: Some("rate_limit".into()),
                    retryable: Some(true),
                    provider_metadata: None,
                },
                &[],
            )
            .await;

        let failed = step_failed.recv().await.unwrap();
        assert_eq!(failed.event_type, "session.next.step.failed");
        assert_eq!(failed.data["error"]["message"], "rate limited");
    }

    #[tokio::test]
    async fn test_fail_unsettled_tools() {
        let (ev, mut pub_) = make_publisher();
        let mut failed = ev.subscribe(session_event_types::TOOL_FAILED).await;

        pub_
            .publish(
                &LlmEvent::ToolInputStart {
                    id: "call_1".into(),
                    name: "bash".into(),
                    provider_metadata: None,
                },
                &[],
            )
            .await;

        pub_
            .publish(
                &LlmEvent::ToolCall {
                    id: "call_1".into(),
                    name: "bash".into(),
                    input: json!({"cmd": "ls"}),
                    provider_executed: Some(false),
                    provider_metadata: None,
                },
                &[],
            )
            .await;

        pub_.fail_unsettled_tools("interrupted", false).await;

        let failed_ev = failed.recv().await.unwrap();
        assert_eq!(failed_ev.event_type, "session.next.tool.failed");
        assert_eq!(failed_ev.data["error"]["message"], "interrupted");
    }

    #[tokio::test]
    async fn test_has_assistant_started() {
        let (_, mut pub_) = make_publisher();
        assert!(!pub_.has_assistant_started());

        pub_
            .publish(
                &LlmEvent::TextStart {
                    id: "txt_1".into(),
                    provider_metadata: None,
                },
                &[],
            )
            .await;

        assert!(pub_.has_assistant_started());
    }

    #[tokio::test]
    async fn test_has_provider_error() {
        let (_, mut pub_) = make_publisher();
        assert!(!pub_.has_provider_error());

        pub_
            .publish(
                &LlmEvent::TextStart {
                    id: "txt_1".into(),
                    provider_metadata: None,
                },
                &[],
            )
            .await;

        pub_
            .publish(
                &LlmEvent::ProviderErrorEvent {
                    message: "error".into(),
                    classification: None,
                    retryable: None,
                    provider_metadata: None,
                },
                &[],
            )
            .await;

        assert!(pub_.has_provider_error());
    }

    #[tokio::test]
    async fn test_tool_result_error() {
        let (ev, mut pub_) = make_publisher();
        let mut failed = ev.subscribe(session_event_types::TOOL_FAILED).await;

        pub_
            .publish(
                &LlmEvent::ToolInputStart {
                    id: "call_1".into(),
                    name: "bash".into(),
                    provider_metadata: None,
                },
                &[],
            )
            .await;

        pub_
            .publish(
                &LlmEvent::ToolCall {
                    id: "call_1".into(),
                    name: "bash".into(),
                    input: json!({"cmd": "ls"}),
                    provider_executed: Some(false),
                    provider_metadata: None,
                },
                &[],
            )
            .await;

        pub_
            .publish(
                &LlmEvent::ToolResult {
                    id: "call_1".into(),
                    name: "bash".into(),
                    result: json!({"type": "error", "value": "command not found"}),
                    output: None,
                    provider_executed: Some(false),
                    provider_metadata: None,
                },
                &[],
            )
            .await;

        let failed_ev = failed.recv().await.unwrap();
        assert_eq!(failed_ev.data["error"]["message"], "command not found");
    }

    #[tokio::test]
    async fn test_tool_error_event() {
        let (ev, mut pub_) = make_publisher();
        let mut failed = ev.subscribe(session_event_types::TOOL_FAILED).await;

        pub_
            .publish(
                &LlmEvent::ToolInputStart {
                    id: "call_1".into(),
                    name: "bash".into(),
                    provider_metadata: None,
                },
                &[],
            )
            .await;

        pub_
            .publish(
                &LlmEvent::ToolCall {
                    id: "call_1".into(),
                    name: "bash".into(),
                    input: json!({"cmd": "ls"}),
                    provider_executed: Some(false),
                    provider_metadata: None,
                },
                &[],
            )
            .await;

        pub_
            .publish(
                &LlmEvent::ToolError {
                    id: "call_1".into(),
                    name: "bash".into(),
                    message: "permission denied".into(),
                    error: None,
                    provider_metadata: None,
                },
                &[],
            )
            .await;

        let failed_ev = failed.recv().await.unwrap();
        assert_eq!(failed_ev.data["error"]["message"], "permission denied");
    }

    #[tokio::test]
    async fn test_step_finish_with_tokens() {
        let (ev, mut pub_) = make_publisher();
        let mut step_ended = ev.subscribe(session_event_types::STEP_ENDED).await;

        pub_
            .publish(
                &LlmEvent::TextStart {
                    id: "txt_1".into(),
                    provider_metadata: None,
                },
                &[],
            )
            .await;

        pub_
            .publish(
                &LlmEvent::StepFinish {
                    index: 0,
                    reason: FinishReason::Stop,
                    usage: Some(crate::provider::Usage {
                        input_tokens: Some(150),
                        output_tokens: Some(75),
                        reasoning_tokens: Some(10),
                        non_cached_input_tokens: Some(100),
                        cache_read_input_tokens: Some(40),
                        cache_write_input_tokens: Some(10),
                        ..Default::default()
                    }),
                    provider_metadata: None,
                },
                &[],
            )
            .await;

        let ended = step_ended.recv().await.unwrap();
        let tokens = &ended.data["tokens"];
        assert_eq!(tokens["input"], 100.0);
        assert_eq!(tokens["output"], 65.0);
        assert_eq!(tokens["reasoning"], 10.0);
        assert_eq!(tokens["cache"]["read"], 40.0);
        assert_eq!(tokens["cache"]["write"], 10.0);
    }

    #[tokio::test]
    async fn test_reasoning_lifecycle() {
        let (ev, mut pub_) = make_publisher();
        let mut reasoning_started = ev.subscribe(session_event_types::REASONING_STARTED).await;
        let mut reasoning_delta = ev.subscribe(session_event_types::REASONING_DELTA).await;
        let mut reasoning_ended = ev.subscribe(session_event_types::REASONING_ENDED).await;

        pub_
            .publish(
                &LlmEvent::ReasoningStart {
                    id: "reason_1".into(),
                    provider_metadata: None,
                },
                &[],
            )
            .await;

        let started = reasoning_started.recv().await.unwrap();
        assert_eq!(started.data["reasoningID"], "reason_1");

        pub_
            .publish(
                &LlmEvent::ReasoningDelta {
                    id: "reason_1".into(),
                    text: "I think ".into(),
                    provider_metadata: None,
                },
                &[],
            )
            .await;

        let delta = reasoning_delta.recv().await.unwrap();
        assert_eq!(delta.data["delta"], "I think ");

        pub_
            .publish(
                &LlmEvent::ReasoningDelta {
                    id: "reason_1".into(),
                    text: "therefore I am".into(),
                    provider_metadata: None,
                },
                &[],
            )
            .await;

        pub_
            .publish(
                &LlmEvent::ReasoningEnd {
                    id: "reason_1".into(),
                    provider_metadata: Some(
                        [("model".to_string(), json!("claude"))]
                            .into_iter()
                            .collect(),
                    ),
                },
                &[],
            )
            .await;

        let ended = reasoning_ended.recv().await.unwrap();
        assert_eq!(ended.data["text"], "I think therefore I am");
        assert_eq!(ended.data["providerMetadata"]["model"], "claude");
    }

    #[tokio::test]
    async fn test_tool_call_with_auto_start_end() {
        let (ev, mut pub_) = make_publisher();
        let mut input_started = ev.subscribe(session_event_types::TOOL_INPUT_STARTED).await;
        let mut input_ended = ev.subscribe(session_event_types::TOOL_INPUT_ENDED).await;
        let mut called = ev.subscribe(session_event_types::TOOL_CALLED).await;

        pub_
            .publish(
                &LlmEvent::ToolCall {
                    id: "call_1".into(),
                    name: "read".into(),
                    input: json!({"path": "/tmp"}),
                    provider_executed: Some(true),
                    provider_metadata: None,
                },
                &[],
            )
            .await;

        let _started = input_started.recv().await.unwrap();
        let _ended = input_ended.recv().await.unwrap();

        let call = called.recv().await.unwrap();
        assert_eq!(call.data["tool"], "read");
        assert_eq!(call.data["input"]["path"], "/tmp");
    }
}
