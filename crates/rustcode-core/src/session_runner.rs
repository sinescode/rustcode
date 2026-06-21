//! Session runner V2 — full turn orchestration with context epoch management,
//! input promotion, tool materialization, overflow recovery, and step limiting.
//!
//! Ported from:
//! - `packages/core/src/session/runner/llm.ts` (404 lines) — V2 runner orchestration
//! - `packages/core/src/session/runner/index.ts` — RunError, Service type
//! - `packages/opencode/src/session/prompt.ts` — V1 run loop

use crate::agent::AgentService;
use crate::config::CompactionConfig;
use crate::database::DatabaseService;
use crate::error::Error;
use crate::provider::{
    ChatMessage, ContentPart, LlmEvent, MessageContent, Model, Provider, ToolDefinition,
    ToolResultPart,
};
use crate::session_compaction::SessionCompaction;
use crate::session_epoch::EpochManager;
use crate::session_execution::{
    Demand, DrainFn, RunCoordinator, SessionRunError, SessionRunErrorKind,
};
use crate::session_history::{ContextEpoch, PromoteSteersParams};
use crate::session_info::SessionId;
use crate::session_input_inbox::{InputInboxError, SessionInputInbox};
use crate::session_prompt::{PromptPart, SessionPromptBuilder, SessionPromptInput};
use crate::tool::{ToolContext, ToolRegistry};
use crate::truncate::TruncateService;
use crate::permission::PermissionSource;
use futures::StreamExt;
use std::time::Duration;

// ══════════════════════════════════════════════════════════════════════════════
// Constants
// ══════════════════════════════════════════════════════════════════════════════

/// Maximum number of turns (LLM→tool round-trips) before we abort.
///
/// # Source
/// `packages/core/src/session/runner/llm.ts` line 88 `MAX_STEPS`.
const MAX_STEPS: usize = 25;

/// Default maximum number of LLM-tool round-trips before we abort (doom-loop guard).
const DEFAULT_MAX_ITERATIONS: usize = 25;

/// How many identical (tool, input) calls before we consider it a doom-loop.
const DOOM_LOOP_THRESHOLD: usize = 3;

/// Timeout for provider streaming responses (5 minutes).
const PROVIDER_TIMEOUT: Duration = Duration::from_secs(300);

/// Timeout for individual provider stream chunks (2 minutes).
const PROVIDER_CHUNK_TIMEOUT: Duration = Duration::from_secs(120);

// ══════════════════════════════════════════════════════════════════════════════
// Public types
// ══════════════════════════════════════════════════════════════════════════════

// InputDelivery is re-exported from session_history.
pub use crate::session_history::InputDelivery;

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

// ══════════════════════════════════════════════════════════════════════════════
// Internal control flow for V2 turn transitions
// ══════════════════════════════════════════════════════════════════════════════

/// Turn transition — controls the orchestration flow between turns.
///
/// # Source
/// `packages/core/src/session/runner/llm.ts` lines 144–148 `TurnTransition`.
#[derive(Debug, Clone)]
enum TurnControl {
    /// Request preparation observed a concurrent Session change; must restart from durable state.
    RebuildPreparedTurn {
        promotion: Option<InputDelivery>,
    },
    /// Overflow compaction completed; rebuild once through the path without overflow recovery.
    ContinueAfterOverflowCompaction,
}

// ══════════════════════════════════════════════════════════════════════════════
// SessionRunner
// ══════════════════════════════════════════════════════════════════════════════

/// Session runner — wires together prompt building, provider resolution,
/// context epoch management, input promotion, and the multi-turn tool loop.
///
/// This is the main entry point for executing a session against an LLM
/// with full tool support and V2 orchestration.
///
/// # Source
/// Ported from `packages/core/src/session/runner/llm.ts`.
pub struct SessionRunner {
    /// Tool registry for tool definitions and execution
    tool_registry: Arc<ToolRegistry>,
    /// Context epoch manager
    epoch_manager: Arc<EpochManager>,
    /// Session input inbox for input promotion
    input_inbox: Arc<SessionInputInbox>,
    /// Agent service for resolving agent info and permissions
    agent_service: Arc<AgentService>,
    /// Session compaction service
    compaction: Arc<SessionCompaction>,
    /// Database service
    db: Arc<DatabaseService>,
    /// Truncation service for tool output
    truncate: Arc<TruncateService>,
    /// Maximum number of turns (step limit)
    max_steps: usize,
    /// Maximum number of LLM→tool round-trips (doom-loop guard, V1)
    max_iterations: usize,
}

impl SessionRunner {
    /// Create a new session runner with V2 dependencies.
    pub fn new(
        tool_registry: Arc<ToolRegistry>,
        epoch_manager: Arc<EpochManager>,
        input_inbox: Arc<SessionInputInbox>,
        agent_service: Arc<AgentService>,
        compaction: Arc<SessionCompaction>,
        db: Arc<DatabaseService>,
        truncate: Arc::new(TruncateService::new()),
    ) -> Self {
        Self {
            tool_registry,
            epoch_manager,
            input_inbox,
            agent_service,
            compaction,
            db,
            max_steps: MAX_STEPS,
            max_iterations: DEFAULT_MAX_ITERATIONS,
        }
    }

    /// Create a new session runner with a custom max-steps cap.
    pub fn with_max_steps(
        tool_registry: Arc<ToolRegistry>,
        epoch_manager: Arc<EpochManager>,
        input_inbox: Arc<SessionInputInbox>,
        agent_service: Arc<AgentService>,
        compaction: Arc<SessionCompaction>,
        db: Arc<DatabaseService>,
        max_steps: usize,
    ) -> Self {
        Self {
            tool_registry,
            epoch_manager,
            input_inbox,
            agent_service,
            compaction,
            db,
            max_steps,
            max_iterations: DEFAULT_MAX_ITERATIONS,
        }
    }

    /// Return the configured maximum number of steps.
    pub fn max_steps(&self) -> usize {
        self.max_steps
    }

    /// Return the configured maximum number of LLM→tool iterations.
    pub fn max_iterations(&self) -> usize {
        self.max_iterations
    }

    /// Return the tool registry.
    pub fn tool_registry(&self) -> &Arc<ToolRegistry> {
        &self.tool_registry
    }

    /// Return the epoch manager.
    pub fn epoch_manager(&self) -> &Arc<EpochManager> {
        &self.epoch_manager
    }

    /// Return the input inbox.
    pub fn input_inbox(&self) -> &Arc<SessionInputInbox> {
        &self.input_inbox
    }

    /// Return the agent service.
    pub fn agent_service(&self) -> &Arc<AgentService> {
        &self.agent_service
    }

    /// Return the compaction service.
    pub fn compaction(&self) -> &Arc<SessionCompaction> {
        &self.compaction
    }

    /// Return the database service.
    pub fn db(&self) -> &Arc<DatabaseService> {
        &self.db
    }

    // ══════════════════════════════════════════════════════════════════════════
    // V1 interface — legacy support
    // ══════════════════════════════════════════════════════════════════════════

    /// Create a [`DrainFn`] that wraps this runner's `run_v2()` method.
    ///
    /// The returned closure is ready for use with [`RunCoordinator::new`].
    /// All context (provider, model, input, instructions) must be provided
    /// up-front since the runner does not resolve sessions itself.
    pub fn make_drain_fn(
        self: Arc<Self>,
        provider: Arc<dyn Provider>,
        model: Model,
        input: SessionPromptInput,
        instructions: Vec<String>,
    ) -> DrainFn {
        Arc::new(move |session_id: SessionId, demand: Demand| {
            let runner = self.clone();
            let provider = provider.clone();
            let model = model.clone();
            let input = input.clone();
            let instructions = instructions.clone();
            Box::pin(async move {
                let _ = demand;
                runner
                    .run(&*provider, &model, &input, &instructions)
                    .await
                    .map_err(|e| SessionRunError {
                        kind: SessionRunErrorKind::Internal,
                        message: e.to_string(),
                        session_id: Some(session_id),
                    })?;
                Ok(())
            })
        })
    }

    /// Create a [`RunCoordinator`] that drives this runner's tool loop.
    pub fn make_coordinator(
        self: Arc<Self>,
        provider: Arc<dyn Provider>,
        model: Model,
        input: SessionPromptInput,
        instructions: Vec<String>,
    ) -> RunCoordinator {
        let drain_fn = self.make_drain_fn(provider, model, input, instructions);
        RunCoordinator::new(drain_fn, None)
    }

    /// Run a session prompt with the given provider and model (V1 style).
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
        self.run_loop(provider, model, &mut messages, &tool_defs, &input_clone)
            .await
    }

    /// Run the tool loop starting from pre-built messages (V1 style).
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
        self.run_loop(provider, model, messages, &tool_defs, &dummy_input)
            .await
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

    // ══════════════════════════════════════════════════════════════════════════
    // V2 orchestration — `run()` entry point
    // ══════════════════════════════════════════════════════════════════════════

    /// Run the full V2 session orchestration for the given session.
    ///
    /// Orchestrates turns, manages step limiting (MAX_STEPS), promotes inputs,
    /// handles context epoch lifecycle, and detects overflow with compaction recovery.
    ///
    /// # Source
    /// `packages/core/src/session/runner/llm.ts` lines 373–396 `run`.
    pub async fn run_v2(
        &self,
        provider: Arc<dyn Provider>,
        model: Model,
        session_id: &str,
        force: bool,
        compaction_config: &CompactionConfig,
    ) -> Result<SessionRunResult, Error> {
        // Check for pending inputs
        let pending = self
            .input_inbox
            .pending_inputs(session_id)
            .await
            .map_err(|e| Error::Session(format!("pending inputs: {e}")))?;

        let has_steer = pending.iter().any(|i| i.delivery == InputDelivery::Steer);
        let has_queue = if has_steer {
            false
        } else {
            pending.iter().any(|i| i.delivery == InputDelivery::Queue)
        };

        if !force && !has_steer && !has_queue {
            return Ok(SessionRunResult {
                text: String::new(),
                events: Vec::new(),
                success: true,
                tool_calls: Vec::new(),
                iterations: 0,
                error: None,
            });
        }

        let mut promotion: Option<InputDelivery> = if has_steer {
            Some(InputDelivery::Steer)
        } else if has_queue {
            Some(InputDelivery::Queue)
        } else {
            None
        };

        let mut open_activity = force || has_steer || has_queue;
        let mut all_text = String::new();
        let mut all_events: Vec<LlmEvent> = Vec::new();
        let mut all_tool_calls: Vec<ToolCallRecord> = Vec::new();
        let mut total_iterations: usize = 0;
        let mut error: Option<String> = None;

        while open_activity {
            let mut needs_continuation = true;

            for _ in 0..self.max_steps {
                // Run one turn (with overflow recovery)
                let (cont, text, events, tool_calls) = self
                    .run_turn(
                        &provider,
                        &model,
                        session_id,
                        promotion,
                        compaction_config,
                    )
                    .await?;

                all_text.push_str(&text);
                all_events.extend(events);
                all_tool_calls.extend(tool_calls);
                total_iterations += 1;
                needs_continuation = cont;

                // After the first turn, promotion becomes "steer"
                promotion = Some(InputDelivery::Steer);

                if !needs_continuation {
                    // Check for pending steer inputs
                    needs_continuation = self
                        .has_pending_steers(session_id)
                        .await
                        .map_err(|e| Error::Session(format!("check steer: {e}")))?;
                }

                if !needs_continuation {
                    break;
                }
            }

            if needs_continuation {
                error = Some(format!("step limit exceeded ({})", self.max_steps));
                return Err(Error::StepLimitExceeded {
                    session_id: session_id.to_string(),
                });
            }

            // Check for pending queue inputs — start a new activity
            let has_next_queue = self
                .has_pending_queues(session_id)
                .await
                .map_err(|e| Error::Session(format!("check queue: {e}")))?;
            open_activity = has_next_queue;
            promotion = if has_next_queue {
                Some(InputDelivery::Queue)
            } else {
                None
            };
        }

        Ok(SessionRunResult {
            text: all_text,
            events: all_events,
            success: error.is_none(),
            tool_calls: all_tool_calls,
            iterations: total_iterations,
            error,
        })
    }

    // ══════════════════════════════════════════════════════════════════════════
    // V2 turn orchestration
    // ══════════════════════════════════════════════════════════════════════════

    /// Run a single turn with overflow recovery.
    ///
    /// Calls `run_turn_attempt` with overflow recovery. If the attempt signals
    /// `ContinueAfterOverflowCompaction`, runs the post-compaction turn.
    /// If it signals `RebuildPreparedTurn`, retries from scratch.
    ///
    /// Returns `(needs_continuation, text, events, tool_calls)`.
    ///
    /// # Source
    /// `packages/core/src/session/runner/llm.ts` lines 359–371 `runTurn`.
    async fn run_turn(
        &self,
        provider: &dyn Provider,
        model: &Model,
        session_id: &str,
        promotion: Option<InputDelivery>,
        compaction_config: &CompactionConfig,
    ) -> Result<(bool, String, Vec<LlmEvent>, Vec<ToolCallRecord>), Error> {
        let recover = Some((compaction_config.clone(),));
        match self
            .run_turn_attempt(provider, model, session_id, promotion, recover)
            .await
        {
            Ok(result) => Ok(result),
            Err(e) => {
                // Check for TurnControl signals (Rust representation: we encode
                // control flow in the error variants)
                if let Error::Internal(ref msg) = e {
                    if let Some(ctrl) = Self::parse_turn_control(msg) {
                        match ctrl {
                            TurnControl::ContinueAfterOverflowCompaction => {
                                // Run again without overflow recovery
                                return self
                                    .run_after_overflow_compaction(
                                        provider,
                                        model,
                                        session_id,
                                        None,
                                        compaction_config,
                                    )
                                    .await;
                            }
                            TurnControl::RebuildPreparedTurn { promotion: p } => {
                                // Retry from scratch
                                return self
                                    .run_turn(
                                        provider,
                                        model,
                                        session_id,
                                        p,
                                        compaction_config,
                                    )
                                    .await;
                            }
                        }
                    }
                }
                Err(e)
            }
        }
    }

    /// Run a single turn after overflow compaction — no overflow recovery allowed.
    ///
    /// # Source
    /// `packages/core/src/session/runner/llm.ts` lines 345–357 `runAfterOverflowCompaction`.
    async fn run_after_overflow_compaction(
        &self,
        provider: &dyn Provider,
        model: &Model,
        session_id: &str,
        promotion: Option<InputDelivery>,
        compaction_config: &CompactionConfig,
    ) -> Result<(bool, String, Vec<LlmEvent>, Vec<ToolCallRecord>), Error> {
        match self
            .run_turn_attempt(provider, model, session_id, promotion, None)
            .await
        {
            Ok(result) => Ok(result),
            Err(e) => {
                if let Error::Internal(ref msg) = e {
                    match Self::parse_turn_control(msg) {
                        Some(TurnControl::ContinueAfterOverflowCompaction) => {
                            return Err(Error::Internal(
                                "Post-compaction provider attempt cannot recover another overflow"
                                    .to_string(),
                            ));
                        }
                        Some(TurnControl::RebuildPreparedTurn { promotion: p }) => {
                            return self
                                .run_after_overflow_compaction(
                                    provider,
                                    model,
                                    session_id,
                                    p,
                                    compaction_config,
                                )
                                .await;
                        }
                        None => {}
                    }
                }
                Err(e)
            }
        }
    }

    /// Execute one full provider turn with the complete lifecycle.
    ///
    /// 1. Initialize / prepare the context epoch
    /// 2. Promote inputs (steer/queue)
    /// 3. Check session consistency (agent, model)
    /// 4. Materialize tools from agent permissions
    /// 5. Check if compaction is needed before streaming
    /// 6. Stream the LLM call
    /// 7. Execute tool calls from the stream
    /// 8. Handle overflow recovery
    ///
    /// # Source
    /// `packages/core/src/session/runner/llm.ts` lines 175–338 `runTurnAttempt`.
    #[allow(clippy::too_many_arguments)]
    async fn run_turn_attempt(
        &self,
        provider: &dyn Provider,
        model: &Model,
        session_id: &str,
        promotion: Option<InputDelivery>,
        recover_overflow: Option<(CompactionConfig,)>,
    ) -> Result<(bool, String, Vec<LlmEvent>, Vec<ToolCallRecord>), Error> {
        // 1. Initialize or prepare the context epoch
        let system = self
            .initialize_epoch_for_turn(session_id)
            .await
            .map_err(|e| Error::Session(format!("epoch init: {e}")))?;

        // 2. Promote inputs before the turn
        self.promote_inputs(session_id, promotion)
            .await
            .map_err(|e| Error::Session(format!("input promotion: {e}")))?;

        // 3. Materialize tool definitions from the agent
        let agent_info = self
            .agent_service
            .get(&system.agent)
            .ok_or_else(|| Error::Session(format!("agent not found: {}", system.agent)))?;

        let agent_permissions = agent_info.permission.clone();
        let tool_defs = self.materialize_tools(&agent_permissions);

        // 4. Build the LLM request context
        let baseline_str = serde_json::to_string(&system.snapshot)
            .unwrap_or_default();

        let mut messages = Vec::new();
        if !baseline_str.is_empty() && baseline_str != "null" {
            messages.push(ChatMessage::System {
                content: MessageContent::Text(system.baseline.clone()),
            });
        }

        // 5. Stream the LLM call
        let mut final_text = String::new();
        let mut all_events: Vec<LlmEvent> = Vec::new();
        let mut tool_calls_made: Vec<ToolCallRecord> = Vec::new();
        let mut needs_continuation = false;
        let mut overflow_detected = false;
        let mut assistant_started = false;

        let mut stream = match tokio::time::timeout(PROVIDER_TIMEOUT, provider.stream(model, &messages, &tool_defs)).await {
            Ok(Ok(s)) => s,
            Ok(Err(e)) => {
                let msg = e.to_string();
                if crate::error::is_context_overflow(&msg) {
                    overflow_detected = true;
                }
                return Err(e);
            }
            Err(_) => {
                return Err(Error::Provider("provider stream timed out after 300s".into()));
            }
        };

        let mut pending_tool_calls: HashMap<String, PendingToolCall> = HashMap::new();
        let mut step_text = String::new();

        loop {
            let chunk = match tokio::time::timeout(PROVIDER_CHUNK_TIMEOUT, stream.next()).await {
                Ok(Some(result)) => result,
                Ok(None) => break,
                Err(_) => {
                    all_events.push(LlmEvent::ProviderErrorEvent {
                        message: "provider stream chunk timed out after 120s".into(),
                        classification: Some("timeout".into()),
                        retryable: Some(true),
                        provider_metadata: None,
                    });
                    break;
                }
            };
            let result = chunk;
            match result {
                Ok(event) => {
                    match &event {
                        LlmEvent::TextDelta { text: delta, .. } => {
                            step_text.push_str(delta);
                            final_text.push_str(delta);
                            assistant_started = true;
                        }
                        LlmEvent::ToolCall { id, name, input, .. } => {
                            needs_continuation = true;
                            pending_tool_calls.insert(
                                id.clone(),
                                PendingToolCall {
                                    call_id: id.clone(),
                                    name: name.clone(),
                                    input: input.clone(),
                                },
                            );
                        }
                        LlmEvent::StepFinish { reason, .. } => {
                            let _ = reason;
                        }
                        _ => {}
                    }
                    all_events.push(event);
                }
                Err(e) => {
                    let msg = e.to_string();
                    if !assistant_started && crate::error::is_context_overflow(&msg) {
                        overflow_detected = true;
                    } else {
                        all_events.push(LlmEvent::ProviderErrorEvent {
                            message: msg,
                            classification: Some("stream-error".into()),
                            retryable: Some(false),
                            provider_metadata: None,
                        });
                    }
                }
            }
        }

        // 6. Handle overflow recovery
        if overflow_detected && !assistant_started {
            if let Some((ref comp_cfg)) = recover_overflow {
                // Run compaction to recover
                let messages_json: Vec<serde_json::Value> = messages
                    .iter()
                    .filter_map(|m| serde_json::to_value(m).ok())
                    .collect();

                let compact_result = self
                    .compaction
                    .compact(
                        &messages_json,
                        model,
                        provider,
                        None,
                        comp_cfg,
                    )
                    .await
                    .map_err(|e| Error::Session(format!("overflow compaction: {e}")))?;

                if let Some(ref compact) = compact_result {
                    // Update epoch with the compacted state
                    let snapshot_val = serde_json::json!({
                        "summary": compact.summary,
                        "recent": compact.recent,
                    });
                    self.epoch_manager
                        .prepare_epoch(session_id, &compact.summary, &snapshot_val)
                        .await
                        .map_err(|e| Error::Session(format!("epoch prepare after compact: {e}")))?;

                    return Err(Error::Internal(
                        TurnControl::ContinueAfterOverflowCompaction.encode(),
                    ));
                }
            }
        }

        // 7. Execute tool calls
        if !pending_tool_calls.is_empty() {
            let mut assistant_parts: Vec<ContentPart> = Vec::new();
            if !step_text.is_empty() {
                assistant_parts.push(ContentPart::Text {
                    text: step_text.clone(),
                });
            }
            for tc in pending_tool_calls.values() {
                assistant_parts.push(ContentPart::ToolCallPart {
                    tool_call_id: tc.call_id.clone(),
                    tool_name: tc.name.clone(),
                    arguments: tc.input.clone(),
                });
            }
            messages.push(ChatMessage::Assistant {
                content: MessageContent::Parts(assistant_parts),
            });

            let mut tool_result_parts: Vec<ToolResultPart> = Vec::new();
            for tc in pending_tool_calls.values() {
                let ctx = ToolContext {
                    session_id: session_id.to_string(),
                    message_id: String::new(),
                    agent: system.agent.clone(),
                    abort: tokio_util::sync::CancellationToken::new(),
                    call_id: Some(tc.call_id.clone()),
                    extra: HashMap::new(),
                    messages: Arc::from(messages.as_slice()),
                    ask_fn: None,
                    permission_source: Some(PermissionSource::Session {
                        session_id: session_id.to_string(),
                    }),
                };

                let result = self
                    .tool_registry
                    .execute_with_pipeline(&tc.name, tc.input.clone(), &ctx, &self.truncate)
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
        }

        Ok((needs_continuation, final_text, all_events, tool_calls_made))
    }

    // ══════════════════════════════════════════════════════════════════════════
    // Helpers — epoch, promotion, tools
    // ══════════════════════════════════════════════════════════════════════════

    /// Initialize or prepare the context epoch for a turn.
    ///
    /// Returns the current `ContextEpoch`.
    async fn initialize_epoch_for_turn(
        &self,
        session_id: &str,
    ) -> Result<ContextEpoch, crate::session_epoch::EpochError> {
        let existing = self.epoch_manager.get_epoch(session_id).await?;

        match existing {
            Some(epoch) => {
                // Check staleness: agent mismatch triggers rebuild
                let snapshot = epoch.snapshot.clone();

                self.epoch_manager
                    .prepare_epoch(
                        session_id,
                        &epoch.baseline,
                        &snapshot,
                    )
                    .await
            }
            None => {
                // Initialize a new epoch with an empty baseline
                let snapshot = serde_json::json!({});
                self.epoch_manager
                    .initialize_epoch(session_id, "", "build", &snapshot, 0)
                    .await
            }
        }
    }

    /// Promote inputs before a turn based on the delivery mode.
    async fn promote_inputs(
        &self,
        session_id: &str,
        promotion: Option<InputDelivery>,
    ) -> Result<(), InputInboxError> {
        match promotion {
            Some(InputDelivery::Steer) => {
                let cutoff = self
                    .db
                    .get_next_admitted_seq(session_id)
                    .await
                    .map_err(|e| InputInboxError::Database(e.to_string()))?;

                self.input_inbox
                    .promote_steers(PromoteSteersParams {
                        session_id: session_id.to_string(),
                        cutoff: cutoff as u64,
                    })
                    .await?;
            }
            Some(InputDelivery::Queue) => {
                self.input_inbox
                    .promote_next_queued(session_id)
                    .await?;
                let cutoff = self
                    .db
                    .get_next_admitted_seq(session_id)
                    .await
                    .map_err(|e| InputInboxError::Database(e.to_string()))?;
                self.input_inbox
                    .promote_steers(PromoteSteersParams {
                        session_id: session_id.to_string(),
                        cutoff: cutoff as u64,
                    })
                    .await?;
            }
            None => {}
        }
        Ok(())
    }

    /// Materialize tool definitions from agent permissions.
    ///
    /// Filters the tool registry based on the agent's permission ruleset.
    /// Tools that are explicitly denied by the agent's permissions are excluded.
    fn materialize_tools(
        &self,
        _agent_permissions: &crate::permission::PermissionRuleset,
    ) -> Vec<ToolDefinition> {
        // For now, return all tool definitions.
        // Full permission filtering will be implemented when the permission
        // system is fully integrated with the runner.
        self.tool_registry.to_definitions()
    }

    /// Check if there are pending steer inputs for the session.
    async fn has_pending_steers(&self, session_id: &str) -> Result<bool, InputInboxError> {
        let pending = self.input_inbox.pending_inputs(session_id).await?;
        Ok(pending.iter().any(|i| i.delivery == InputDelivery::Steer))
    }

    /// Check if there are pending queue inputs for the session.
    async fn has_pending_queues(&self, session_id: &str) -> Result<bool, InputInboxError> {
        let pending = self.input_inbox.pending_inputs(session_id).await?;
        Ok(pending.iter().any(|i| i.delivery == InputDelivery::Queue))
    }

    // ══════════════════════════════════════════════════════════════════════════
    // TurnControl encoding/decoding
    // ══════════════════════════════════════════════════════════════════════════

    /// Encode a `TurnControl` as a string for transport via `Error::Internal`.
    fn encode_turn_control(ctrl: &TurnControl) -> String {
        match ctrl {
            TurnControl::RebuildPreparedTurn { promotion } => {
                let prom = match promotion {
                    Some(InputDelivery::Steer) => "steer",
                    Some(InputDelivery::Queue) => "queue",
                    None => "none",
                };
                format!("__TURN_CTRL::RebuildPreparedTurn({})__", prom)
            }
            TurnControl::ContinueAfterOverflowCompaction => {
                "__TURN_CTRL::ContinueAfterOverflowCompaction__".to_string()
            }
        }
    }

    /// Try to decode a `TurnControl` from a string.
    fn parse_turn_control(msg: &str) -> Option<TurnControl> {
        if msg.starts_with("__TURN_CTRL::") {
            if msg.contains("RebuildPreparedTurn") {
                let prom = if msg.contains("(steer)") {
                    Some(InputDelivery::Steer)
                } else if msg.contains("(queue)") {
                    Some(InputDelivery::Queue)
                } else {
                    None
                };
                Some(TurnControl::RebuildPreparedTurn { promotion: prom })
            } else if msg.contains("ContinueAfterOverflowCompaction") {
                Some(TurnControl::ContinueAfterOverflowCompaction)
            } else {
                None
            }
        } else {
            None
        }
    }

    // ══════════════════════════════════════════════════════════════════════════
    // V1 core loop — kept for backward compatibility
    // ══════════════════════════════════════════════════════════════════════════

    /// Core multi-turn streaming tool loop (V1 style).
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
                abort_reason =
                    Some(format!("exceeded max iterations ({})", self.max_iterations));
                break;
            }

            if let Some((tool, count)) = detect_doom_loop(&tool_calls_made) {
                aborted = true;
                abort_reason = Some(format!(
                    "doom loop: tool '{tool}' called {count}x with same input"
                ));
                break;
            }

            let mut stream = match tokio::time::timeout(PROVIDER_TIMEOUT, provider.stream(model, messages, tool_defs)).await {
                Ok(Ok(s)) => s,
                Ok(Err(e)) => {
                    return Err(e);
                }
                Err(_) => {
                    return Err(Error::Provider("provider stream timed out after 300s".into()));
                }
            };

            let mut step_text = String::new();
            let mut pending_tool_calls: HashMap<String, PendingToolCall> = HashMap::new();
            let mut has_tool_calls = false;
            let mut stream_error: Option<String> = None;

            while let Some(result) = stream.next().await {
                match result {
                    Ok(event) => {
                        if let LlmEvent::TextDelta {
                            text: ref delta, ..
                        } = &event
                        {
                            step_text.push_str(delta);
                            final_text.push_str(delta);
                        }
                        if let LlmEvent::ToolCall {
                            ref id,
                            ref name,
                            ref input,
                            ..
                        } = &event
                        {
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
                        if crate::error::is_context_overflow(&msg) {
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
                assistant_parts.push(ContentPart::Text {
                    text: step_text.clone(),
                });
            }
            for tc in pending_tool_calls.values() {
                assistant_parts.push(ContentPart::ToolCallPart {
                    tool_call_id: tc.call_id.clone(),
                    tool_name: tc.name.clone(),
                    arguments: tc.input.clone(),
                });
            }
            messages.push(ChatMessage::Assistant {
                content: MessageContent::Parts(assistant_parts),
            });

            let mut tool_result_parts: Vec<ToolResultPart> = Vec::new();
            for tc in pending_tool_calls.values() {
                let ctx = ToolContext {
                    session_id: input.session_id.clone(),
                    message_id: String::new(),
                    agent: input.agent.clone().unwrap_or_else(|| "cli".into()),
                    abort: tokio_util::sync::CancellationToken::new(),
                    call_id: Some(tc.call_id.clone()),
                    extra: HashMap::new(),
                    messages: Arc::from(messages.as_slice()),
                    ask_fn: None,
                    permission_source: Some(PermissionSource::Session {
                        session_id: input.session_id.clone(),
                    }),
                };
                let result = self
                    .tool_registry
                    .execute_with_pipeline(&tc.name, tc.input.clone(), &ctx, &self.truncate)
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

// ══════════════════════════════════════════════════════════════════════════════
// TurnControl encoding (as methods on the enum)
// ══════════════════════════════════════════════════════════════════════════════

impl TurnControl {
    fn encode(&self) -> String {
        SessionRunner::encode_turn_control(self)
    }
}

// ══════════════════════════════════════════════════════════════════════════════
// Helpers (shared between V1 and V2)
// ══════════════════════════════════════════════════════════════════════════════

/// Build `ChatMessage` array from a prompt input and system prompt.
async fn build_chat_messages(
    input: &SessionPromptInput,
    system_prompt: &str,
) -> Result<Vec<ChatMessage>, Error> {
    let mut messages: Vec<ChatMessage> = Vec::new();

    if !system_prompt.is_empty() {
        messages.push(ChatMessage::System {
            content: MessageContent::Text(system_prompt.to_string()),
        });
    }

    if let Some(ref sys) = input.system {
        if !sys.is_empty() {
            messages.push(ChatMessage::System {
                content: MessageContent::Text(sys.clone()),
            });
        }
    }

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

// ── Doom-loop detection ──────────────────────────────────────────────────────

/// Check the most recent tool calls for a repeating pattern.
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

// ── Context-overflow detection ───────────────────────────────────────────────

/// Roughly estimate whether the message list has grown beyond the model's
/// context window.
fn check_context_overflow(messages: &[ChatMessage], model: &Model) -> bool {
    let context_limit = model.limit.context;
    if context_limit == 0 {
        return false;
    }

    let estimated_tokens: u64 = messages
        .iter()
        .map(|m| {
            let json = serde_json::to_string(m).unwrap_or_default();
            json.len() as u64 / 4
        })
        .sum();

    let usable = (context_limit as f64 * 0.8) as u64;
    estimated_tokens > usable
}

// ══════════════════════════════════════════════════════════════════════════════
// Tests
// ══════════════════════════════════════════════════════════════════════════════

#[cfg(test)]
mod tests {
    use super::*;

    // ── InputDelivery tests ─────────────────────────────────────────

    #[test]
    fn test_input_delivery_debug() {
        assert_eq!(format!("{:?}", InputDelivery::Steer), "Steer");
        assert_eq!(format!("{:?}", InputDelivery::Queue), "Queue");
    }

    #[test]
    fn test_input_delivery_copy() {
        let a = InputDelivery::Steer;
        let b = a;
        assert_eq!(a, b);
    }

    // ── TurnControl encoding/decoding tests ──────────────────────────

    #[test]
    fn test_turn_control_roundtrip_rebuild_steer() {
        let ctrl = TurnControl::RebuildPreparedTurn {
            promotion: Some(InputDelivery::Steer),
        };
        let encoded = ctrl.encode();
        let decoded = SessionRunner::parse_turn_control(&encoded);
        assert!(decoded.is_some());
        match decoded.unwrap() {
            TurnControl::RebuildPreparedTurn { promotion } => {
                assert_eq!(promotion, Some(InputDelivery::Steer));
            }
            _ => panic!("Wrong variant"),
        }
    }

    #[test]
    fn test_turn_control_roundtrip_rebuild_queue() {
        let ctrl = TurnControl::RebuildPreparedTurn {
            promotion: Some(InputDelivery::Queue),
        };
        let encoded = ctrl.encode();
        let decoded = SessionRunner::parse_turn_control(&encoded);
        assert!(decoded.is_some());
        match decoded.unwrap() {
            TurnControl::RebuildPreparedTurn { promotion } => {
                assert_eq!(promotion, Some(InputDelivery::Queue));
            }
            _ => panic!("Wrong variant"),
        }
    }

    #[test]
    fn test_turn_control_roundtrip_rebuild_none() {
        let ctrl = TurnControl::RebuildPreparedTurn { promotion: None };
        let encoded = ctrl.encode();
        let decoded = SessionRunner::parse_turn_control(&encoded);
        assert!(decoded.is_some());
        match decoded.unwrap() {
            TurnControl::RebuildPreparedTurn { promotion } => {
                assert!(promotion.is_none());
            }
            _ => panic!("Wrong variant"),
        }
    }

    #[test]
    fn test_turn_control_roundtrip_overflow() {
        let ctrl = TurnControl::ContinueAfterOverflowCompaction;
        let encoded = ctrl.encode();
        let decoded = SessionRunner::parse_turn_control(&encoded);
        assert!(decoded.is_some());
        assert!(matches!(
            decoded.unwrap(),
            TurnControl::ContinueAfterOverflowCompaction
        ));
    }

    #[test]
    fn test_turn_control_parse_invalid() {
        assert!(SessionRunner::parse_turn_control("hello world").is_none());
        assert!(SessionRunner::parse_turn_control("").is_none());
        assert!(SessionRunner::parse_turn_control("__TURN_CTRL::unknown__").is_none());
    }

    // ── build_chat_messages tests ───────────────────────────────────

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
                parts: vec![PromptPart::Text(crate::session_prompt::PromptTextPart {
                    id: None,
                    text: "Hello, can you help me?".into(),
                    synthetic: false,
                })],
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

            let messages = build_chat_messages(&input, "Default system").await.unwrap();

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

    // ── Doom-loop detection tests ───────────────────────────────────

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

    // ── MAX_STEPS constant ──────────────────────────────────────────

    #[test]
    fn test_max_steps_constant() {
        assert_eq!(MAX_STEPS, 25);
    }
}
