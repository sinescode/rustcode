//! Session message projection pipeline — projects session events into
//! session message rows and manages the session, message, and input tables.
//!
//! Ported from: `packages/core/src/session/projector.ts` (lines 1–451)

use crate::database::DatabaseService;
use crate::event::{
    EventPayload, EventV2, ProjectorFn,
    session_event_types::{
        AGENT_SWITCHED, COMPACTION_ENDED, CONTEXT_UPDATED, INTERRUPT_REQUESTED,
        MODEL_SWITCHED, PROMPT_ADMITTED, PROMPT_PROMOTED, PROMPTED,
        SHELL_ENDED, SHELL_STARTED, STEP_ENDED, STEP_FAILED, STEP_STARTED,
        SYNTHETIC, TEXT_ENDED, TEXT_STARTED, TOOL_CALLED, TOOL_FAILED,
        TOOL_INPUT_ENDED, TOOL_INPUT_STARTED, TOOL_PROGRESS, TOOL_SUCCESS,
        REASONING_ENDED, REASONING_STARTED,
    },
};
use crate::session_message::{
    AssistantMessage, AssistantTime, MessageTime, Prompt, SessionMessage,
    ShellMessage, ShellTime, SyntheticMessage, SystemMessage, UserMessage,
};
use crate::session_epoch::EpochManager;
use std::sync::Arc;

/// Error type for session projection.
#[derive(Debug, thiserror::Error)]
pub enum ProjectionError {
    #[error("Session already projected: {0}")]
    SessionAlreadyProjected(String),
    #[error("Prompt already projected: {0}")]
    PromptAlreadyProjected(String),
    #[error("Missing aggregate sequence for event type {0}")]
    MissingSequence(String),
    #[error("{0}")]
    Other(String),
}

/// Register all standard session event projectors on an EventV2 instance.
///
/// Each projector handles a specific event type, updating the session_message
/// table, session table, session_input table, and session_context_epoch table
/// as appropriate.
///
/// # Source
/// Ported from `packages/core/src/session/projector.ts` lines 212–448.
pub async fn register_all_projectors(
    events: &EventV2,
    db: Arc<DatabaseService>,
    epoch_mgr: Arc<EpochManager>,
) -> Result<(), ProjectionError> {
    // Helper: extract session_id from event data
    let session_id = |data: &serde_json::Value| -> Option<String> {
        data.get("sessionID")
            .and_then(|v| v.as_str())
            .map(String::from)
    };

    // Helper: check event has seq
    let require_seq = |payload: &EventPayload| -> Result<u64, ProjectionError> {
        payload.seq.ok_or_else(|| {
            ProjectionError::MissingSequence(payload.event_type.clone())
        })
    };

    // Helper: insert a session_message row
    let insert_message = |db: &DatabaseService, sid: &str, msg_type: &str, seq: u64, msg_id: &str, data: &serde_json::Value| {
        let db2 = db.clone();
        let sid = sid.to_string();
        let msg_type = msg_type.to_string();
        let msg_id = msg_id.to_string();
        let data_str = serde_json::to_string(data).unwrap_or_default();
        async move {
            sqlx::query(
                "INSERT INTO session_message (id, session_id, type, seq, time_created, data) \
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6) \
                 ON CONFLICT(id) DO UPDATE SET type=excluded.type, seq=excluded.seq, data=excluded.data",
            )
            .bind(&msg_id)
            .bind(&sid)
            .bind(&msg_type)
            .bind(seq as i64)
            .bind(chrono::Utc::now().timestamp_millis())
            .bind(&data_str)
            .execute(db2.pool())
            .await
            .ok();
        }
    };

    // Helper: update session table
    let update_session = |db: &DatabaseService, sid: &str, updates: serde_json::Value| {
        let db2 = db.clone();
        let sid = sid.to_string();
        async move {
            if let Some(agent) = updates.get("agent").and_then(|v| v.as_str()) {
                sqlx::query("UPDATE session SET agent = ?1, time_updated = ?2 WHERE id = ?3")
                    .bind(agent)
                    .bind(chrono::Utc::now().timestamp_millis())
                    .bind(&sid)
                    .execute(db2.pool())
                    .await
                    .ok();
            }
            if let Some(model) = updates.get("model") {
                let model_str = serde_json::to_string(model).unwrap_or_default();
                sqlx::query("UPDATE session SET model = ?1, time_updated = ?2 WHERE id = ?3")
                    .bind(&model_str)
                    .bind(chrono::Utc::now().timestamp_millis())
                    .bind(&sid)
                    .execute(db2.pool())
                    .await
                    .ok();
            }
        }
    };

    // ── AgentSwitched ────────────────────────────────────────────────
    events
        .project(
            AGENT_SWITCHED,
            mk_projector_fn({
                let db = Arc::clone(&db);
                let epoch = Arc::clone(&epoch_mgr);
                move |payload| {
                    let db = db.clone();
                    let epoch = epoch.clone();
                    Box::pin(async move {
                        let sid = session_id(&payload.data).ok_or_else(|| {
                            crate::event::EventError::Internal("missing sessionID".into())
                        })?;
                        let seq = payload.seq.ok_or_else(|| {
                            crate::event::EventError::Internal("missing seq".into())
                        })?;
                        let agent = payload.data["agent"].as_str().unwrap_or("build");
                        let msg_id = payload.data["messageID"].as_str().unwrap_or("");

                        // Update session agent
                        sqlx::query(
                            "UPDATE session SET agent = ?1, time_updated = ?2 WHERE id = ?3",
                        )
                        .bind(agent)
                        .bind(chrono::Utc::now().timestamp_millis())
                        .bind(&sid)
                        .execute(db.pool())
                        .await
                        .map_err(|e| {
                            crate::event::EventError::Internal(format!("update session: {e}"))
                        })?;

                        // Insert agent-switched message
                        let msg_data = serde_json::json!({
                            "agent": agent,
                            "time": {"created": chrono::Utc::now().timestamp_millis()}
                        });
                        let data_str = serde_json::to_string(&msg_data).unwrap_or_default();
                        sqlx::query(
                            "INSERT INTO session_message (id, session_id, type, seq, time_created, data) \
                             VALUES (?1, ?2, 'agent-switched', ?3, ?4, ?5) \
                             ON CONFLICT(id) DO NOTHING",
                        )
                        .bind(msg_id)
                        .bind(&sid)
                        .bind(seq as i64)
                        .bind(chrono::Utc::now().timestamp_millis())
                        .bind(&data_str)
                        .execute(db.pool())
                        .await
                        .map_err(|e| {
                            crate::event::EventError::Internal(format!("insert msg: {e}"))
                        })?;

                        // Request replacement
                        epoch.request_replacement(&sid).await.map_err(|e| {
                            crate::event::EventError::Internal(format!("request replace: {e}"))
                        })?;

                        Ok(())
                    })
                }
            }),
        )
        .await;

    // ── ModelSwitched ────────────────────────────────────────────────
    events
        .project(
            MODEL_SWITCHED,
            mk_projector_fn({
                let db = Arc::clone(&db);
                let epoch = Arc::clone(&epoch_mgr);
                move |payload| {
                    let db = db.clone();
                    let epoch = epoch.clone();
                    Box::pin(async move {
                        let sid = session_id(&payload.data).ok_or_else(|| {
                            crate::event::EventError::Internal("missing sessionID".into())
                        })?;
                        let seq = payload.seq.ok_or_else(|| {
                            crate::event::EventError::Internal("missing seq".into())
                        })?;
                        let msg_id = payload.data["messageID"].as_str().unwrap_or("");
                        let model_val = &payload.data["model"];

                        // Update session model
                        let model_str = serde_json::to_string(model_val).unwrap_or_default();
                        sqlx::query(
                            "UPDATE session SET model = ?1, time_updated = ?2 WHERE id = ?3",
                        )
                        .bind(&model_str)
                        .bind(chrono::Utc::now().timestamp_millis())
                        .bind(&sid)
                        .execute(db.pool())
                        .await
                        .map_err(|e| {
                            crate::event::EventError::Internal(format!("update session: {e}"))
                        })?;

                        // Insert model-switched message
                        let msg_data = serde_json::json!({
                            "model": model_val,
                            "time": {"created": chrono::Utc::now().timestamp_millis()}
                        });
                        let data_str = serde_json::to_string(&msg_data).unwrap_or_default();
                        sqlx::query(
                            "INSERT INTO session_message (id, session_id, type, seq, time_created, data) \
                             VALUES (?1, ?2, 'model-switched', ?3, ?4, ?5) \
                             ON CONFLICT(id) DO NOTHING",
                        )
                        .bind(msg_id)
                        .bind(&sid)
                        .bind(seq as i64)
                        .bind(chrono::Utc::now().timestamp_millis())
                        .bind(&data_str)
                        .execute(db.pool())
                        .await
                        .map_err(|e| {
                            crate::event::EventError::Internal(format!("insert msg: {e}"))
                        })?;

                        // Request replacement
                        epoch.request_replacement(&sid).await.map_err(|e| {
                            crate::event::EventError::Internal(format!("request replace: {e}"))
                        })?;

                        Ok(())
                    })
                }
            }),
        )
        .await;

    // ── Prompted (legacy) ─────────────────────────────────────────────
    events
        .project(
            PROMPTED,
            mk_projector_fn({
                let db = Arc::clone(&db);
                move |payload| {
                    let db = db.clone();
                    Box::pin(async move {
                        let sid = session_id(&payload.data).ok_or_else(|| {
                            crate::event::EventError::Internal("missing sessionID".into())
                        })?;
                        let msg_id = payload.data["messageID"].as_str().unwrap_or("");
                        let seq = payload.seq.ok_or_else(|| {
                            crate::event::EventError::Internal("missing seq".into())
                        })?;

                        // Check for existing message
                        let exists: bool = sqlx::query_scalar::<_, i64>(
                            "SELECT COUNT(*) FROM session_message WHERE id = ?1",
                        )
                        .bind(msg_id)
                        .fetch_one(db.pool())
                        .await
                        .unwrap_or(0)
                            > 0;

                        if exists {
                            return Err(crate::event::EventError::Internal(
                                "PromptAlreadyProjected".into(),
                            ));
                        }

                        let prompt_val = &payload.data["prompt"];
                        let delivery = payload.data["delivery"]
                            .as_str()
                            .unwrap_or("steer");
                        let prompt: Prompt =
                            serde_json::from_value(prompt_val.clone()).unwrap_or_else(|_| {
                                Prompt {
                                    text: prompt_val
                                        .as_str()
                                        .unwrap_or("")
                                        .to_string(),
                                    files: None,
                                    agents: None,
                                }
                            });

                        let user_msg = UserMessage {
                            id: msg_id.to_string(),
                            session_id: Some(sid.clone()),
                            text: prompt.text,
                            format: None,
                            summary: None,
                            agent: None,
                            model: None,
                            system: None,
                            tools: None,
                            files: prompt.files,
                            agents: prompt.agents,
                            metadata: None,
                            time: MessageTime {
                                created: chrono::Utc::now().timestamp_millis() as u64,
                            },
                        };

                        let msg = SessionMessage::User(user_msg);
                        let encoded =
                            serde_json::to_value(&msg).map_err(|e| {
                                crate::event::EventError::Internal(format!(
                                    "encode: {e}"
                                ))
                            })?;
                        let data_str =
                            serde_json::to_string(&encoded).unwrap_or_default();
                        sqlx::query(
                            "INSERT INTO session_message (id, session_id, type, seq, time_created, data) \
                             VALUES (?1, ?2, 'user', ?3, ?4, ?5)",
                        )
                        .bind(msg_id)
                        .bind(&sid)
                        .bind(seq as i64)
                        .bind(chrono::Utc::now().timestamp_millis())
                        .bind(&data_str)
                        .execute(db.pool())
                        .await
                        .map_err(|e| {
                            crate::event::EventError::Internal(format!(
                                "insert msg: {e}"
                            ))
                        })?;

                        // Also insert into session_input as legacy prompted
                        let prompt_str =
                            serde_json::to_string(&prompt).unwrap_or_default();
                        sqlx::query(
                            "INSERT INTO session_input (id, session_id, prompt, delivery, admitted_seq, promoted_seq, time_created) \
                             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7) \
                             ON CONFLICT(id) DO NOTHING",
                        )
                        .bind(msg_id)
                        .bind(&sid)
                        .bind(&prompt_str)
                        .bind(delivery)
                        .bind(seq as i64)
                        .bind(seq as i64)
                        .bind(chrono::Utc::now().timestamp_millis())
                        .execute(db.pool())
                        .await
                        .map_err(|e| {
                            crate::event::EventError::Internal(format!(
                                "insert session_input: {e}"
                            ))
                        })?;

                        Ok(())
                    })
                }
            }),
        )
        .await;

    // ── PromptLifecycle.Promoted ──────────────────────────────────────
    events
        .project(
            PROMPT_PROMOTED,
            mk_projector_fn({
                let db = Arc::clone(&db);
                move |payload| {
                    let db = db.clone();
                    Box::pin(async move {
                        let sid = session_id(&payload.data).ok_or_else(|| {
                            crate::event::EventError::Internal("missing sessionID".into())
                        })?;
                        let msg_id = payload.data["messageID"].as_str().unwrap_or("");
                        let seq = payload.seq.ok_or_else(|| {
                            crate::event::EventError::Internal("missing seq".into())
                        })?;
                        let prompt_val = &payload.data["prompt"];

                        // Update session_input promoted_seq
                        sqlx::query(
                            "UPDATE session_input SET promoted_seq = ?1 \
                             WHERE id = ?2 AND session_id = ?3 AND promoted_seq IS NULL",
                        )
                        .bind(seq as i64)
                        .bind(msg_id)
                        .bind(&sid)
                        .execute(db.pool())
                        .await
                        .map_err(|e| {
                            crate::event::EventError::Internal(format!(
                                "update promoted: {e}"
                            ))
                        })?;

                        // Insert user message
                        let prompt: Prompt =
                            serde_json::from_value(prompt_val.clone()).unwrap_or_else(
                                |_| Prompt {
                                    text: prompt_val
                                        .as_str()
                                        .unwrap_or("")
                                        .to_string(),
                                    files: None,
                                    agents: None,
                                },
                            );

                        let created = payload.data["timeCreated"]
                            .as_u64()
                            .unwrap_or_else(|| chrono::Utc::now().timestamp_millis() as u64);

                        let user_msg = UserMessage {
                            id: msg_id.to_string(),
                            session_id: Some(sid.clone()),
                            text: prompt.text,
                            format: None,
                            summary: None,
                            agent: None,
                            model: None,
                            system: None,
                            tools: None,
                            files: prompt.files,
                            agents: prompt.agents,
                            metadata: None,
                            time: MessageTime { created },
                        };

                        let msg = SessionMessage::User(user_msg);
                        let encoded =
                            serde_json::to_value(&msg).map_err(|e| {
                                crate::event::EventError::Internal(format!(
                                    "encode: {e}"
                                ))
                            })?;
                        let data_str =
                            serde_json::to_string(&encoded).unwrap_or_default();
                        sqlx::query(
                            "INSERT INTO session_message (id, session_id, type, seq, time_created, data) \
                             VALUES (?1, ?2, 'user', ?3, ?4, ?5)",
                        )
                        .bind(msg_id)
                        .bind(&sid)
                        .bind(seq as i64)
                        .bind(created as i64)
                        .bind(&data_str)
                        .execute(db.pool())
                        .await
                        .map_err(|e| {
                            crate::event::EventError::Internal(format!(
                                "insert msg: {e}"
                            ))
                        })?;

                        Ok(())
                    })
                }
            }),
        )
        .await;

    // ── Synthetic ─────────────────────────────────────────────────────
    events
        .project(
            SYNTHETIC,
            mk_projector_fn({
                let db = Arc::clone(&db);
                move |payload| {
                    let db = db.clone();
                    Box::pin(async move {
                        let sid = session_id(&payload.data).ok_or_else(|| {
                            crate::event::EventError::Internal("missing sessionID".into())
                        })?;
                        let msg_id = payload.data["messageID"].as_str().unwrap_or("");
                        let seq = payload.seq.ok_or_else(|| {
                            crate::event::EventError::Internal("missing seq".into())
                        })?;
                        let text = payload.data["text"].as_str().unwrap_or("");

                        let msg = SessionMessage::Synthetic(SyntheticMessage {
                            id: msg_id.to_string(),
                            session_id: sid.clone(),
                            text: text.to_string(),
                            metadata: None,
                            time: MessageTime {
                                created: chrono::Utc::now().timestamp_millis() as u64,
                            },
                        });

                        let encoded = serde_json::to_value(&msg).unwrap_or_default();
                        let data_str = serde_json::to_string(&encoded).unwrap_or_default();
                        sqlx::query(
                            "INSERT INTO session_message (id, session_id, type, seq, time_created, data) \
                             VALUES (?1, ?2, 'synthetic', ?3, ?4, ?5)",
                        )
                        .bind(msg_id)
                        .bind(&sid)
                        .bind(seq as i64)
                        .bind(chrono::Utc::now().timestamp_millis())
                        .bind(&data_str)
                        .execute(db.pool())
                        .await
                        .map_err(|e| {
                            crate::event::EventError::Internal(format!("insert msg: {e}"))
                        })?;

                        Ok(())
                    })
                }
            }),
        )
        .await;

    // ── ContextUpdated ────────────────────────────────────────────────
    events
        .project(
            CONTEXT_UPDATED,
            mk_projector_fn({
                let db = Arc::clone(&db);
                move |payload| {
                    let db = db.clone();
                    Box::pin(async move {
                        let sid = session_id(&payload.data).ok_or_else(|| {
                            crate::event::EventError::Internal("missing sessionID".into())
                        })?;
                        let msg_id = payload.data["messageID"].as_str().unwrap_or("");
                        let seq = payload.seq.ok_or_else(|| {
                            crate::event::EventError::Internal("missing seq".into())
                        })?;
                        let text = payload.data["text"].as_str().unwrap_or("");

                        let msg = SessionMessage::System(SystemMessage {
                            id: msg_id.to_string(),
                            text: text.to_string(),
                            metadata: None,
                            time: MessageTime {
                                created: chrono::Utc::now().timestamp_millis() as u64,
                            },
                        });

                        let encoded = serde_json::to_value(&msg).unwrap_or_default();
                        let data_str = serde_json::to_string(&encoded).unwrap_or_default();
                        sqlx::query(
                            "INSERT INTO session_message (id, session_id, type, seq, time_created, data) \
                             VALUES (?1, ?2, 'system', ?3, ?4, ?5)",
                        )
                        .bind(msg_id)
                        .bind(&sid)
                        .bind(seq as i64)
                        .bind(chrono::Utc::now().timestamp_millis())
                        .bind(&data_str)
                        .execute(db.pool())
                        .await
                        .map_err(|e| {
                            crate::event::EventError::Internal(format!("insert msg: {e}"))
                        })?;

                        Ok(())
                    })
                }
            }),
        )
        .await;

    // ── Shell.Started — insert a shell message with pending output ────
    events
        .project(
            SHELL_STARTED,
            mk_projector_fn({
                let db = Arc::clone(&db);
                move |payload| {
                    let db = db.clone();
                    Box::pin(async move {
                        let sid = session_id(&payload.data).ok_or_else(|| {
                            crate::event::EventError::Internal("missing sessionID".into())
                        })?;
                        let msg_id = payload.data["messageID"].as_str().unwrap_or("");
                        let call_id = payload.data["callID"].as_str().unwrap_or("");
                        let command = payload.data["command"].as_str().unwrap_or("");
                        let seq = payload.seq.ok_or_else(|| {
                            crate::event::EventError::Internal("missing seq".into())
                        })?;

                        let msg = SessionMessage::Shell(ShellMessage {
                            id: msg_id.to_string(),
                            call_id: call_id.to_string(),
                            command: command.to_string(),
                            output: String::new(),
                            metadata: None,
                            time: ShellTime {
                                created: chrono::Utc::now().timestamp_millis() as u64,
                                completed: None,
                            },
                        });

                        let encoded = serde_json::to_value(&msg).unwrap_or_default();
                        let data_str = serde_json::to_string(&encoded).unwrap_or_default();
                        sqlx::query(
                            "INSERT INTO session_message (id, session_id, type, seq, time_created, data) \
                             VALUES (?1, ?2, 'shell', ?3, ?4, ?5)",
                        )
                        .bind(msg_id)
                        .bind(&sid)
                        .bind(seq as i64)
                        .bind(chrono::Utc::now().timestamp_millis())
                        .bind(&data_str)
                        .execute(db.pool())
                        .await
                        .map_err(|e| {
                            crate::event::EventError::Internal(format!("insert msg: {e}"))
                        })?;

                        Ok(())
                    })
                }
            }),
        )
        .await;

    // ── Shell.Ended — update the existing shell message with output ───
    events
        .project(
            SHELL_ENDED,
            mk_projector_fn({
                let db = Arc::clone(&db);
                move |payload| {
                    let db = db.clone();
                    Box::pin(async move {
                        let call_id = payload.data["callID"].as_str().unwrap_or("");
                        let output = payload.data["output"].as_str().unwrap_or("");

                        // Find the shell message by call_id and update it
                        let rows: Vec<(String,)> = sqlx::query_as(
                            "SELECT id FROM session_message \
                             WHERE session_id = ?1 AND type = 'shell' \
                             ORDER BY seq DESC",
                        )
                        .bind(
                            session_id(&payload.data).unwrap_or_default(),
                        )
                        .fetch_all(db.pool())
                        .await
                        .unwrap_or_default();

                        for (msg_id,) in rows {
                            let existing: Option<serde_json::Value> =
                                sqlx::query_scalar::<_, serde_json::Value>(
                                    "SELECT data FROM session_message WHERE id = ?1",
                                )
                                .bind(&msg_id)
                                .fetch_optional(db.pool())
                                .await
                                .unwrap_or(None);

                            if let Some(mut data) = existing {
                                if data.get("call_id").and_then(|v| v.as_str())
                                    == Some(call_id)
                                {
                                    data["output"] =
                                        serde_json::Value::String(output.to_string());
                                    if let Some(time) =
                                        data.get_mut("time")
                                    {
                                        if let Some(obj) = time.as_object_mut() {
                                            obj.insert(
                                                "completed".into(),
                                                serde_json::json!(
                                                    chrono::Utc::now()
                                                        .timestamp_millis()
                                                ),
                                            );
                                        }
                                    }
                                    let data_str =
                                        serde_json::to_string(&data).unwrap_or_default();
                                    sqlx::query(
                                        "UPDATE session_message SET data = ?1 WHERE id = ?2",
                                    )
                                    .bind(&data_str)
                                    .bind(&msg_id)
                                    .execute(db.pool())
                                    .await
                                    .ok();
                                    break;
                                }
                            }
                        }

                        Ok(())
                    })
                }
            }),
        )
        .await;

    // ── Step.Started ──────────────────────────────────────────────────
    events
        .project(
            STEP_STARTED,
            mk_projector_fn({
                let db = Arc::clone(&db);
                move |payload| {
                    let db = db.clone();
                    Box::pin(async move {
                        let sid = session_id(&payload.data).ok_or_else(|| {
                            crate::event::EventError::Internal("missing sessionID".into())
                        })?;
                        let assistant_msg_id = payload.data["assistantMessageID"]
                            .as_str()
                            .unwrap_or("");
                        let agent = payload.data["agent"].as_str().unwrap_or("");
                        let seq = payload.seq.ok_or_else(|| {
                            crate::event::EventError::Internal("missing seq".into())
                        })?;

                        let model_val = &payload.data["model"];
                        let msg = SessionMessage::Assistant(AssistantMessage {
                            id: assistant_msg_id.to_string(),
                            session_id: Some(sid.clone()),
                            agent: agent.to_string(),
                            model: crate::session_info::ModelRef {
                                id: model_val.get("id")
                                    .and_then(|v| v.as_str())
                                    .unwrap_or("")
                                    .to_string(),
                                provider_id: model_val
                                    .get("providerID")
                                    .and_then(|v| v.as_str())
                                    .unwrap_or("")
                                    .to_string(),
                                variant: None,
                            },
                            parent_id: None,
                            provider_id: None,
                            model_id: None,
                            mode: None,
                            path: None,
                            summary: false,
                            variant: None,
                            content: Vec::new(),
                            snapshot: None,
                            finish: None,
                            cost: None,
                            tokens: None,
                            error: None,
                            metadata: None,
                            time: AssistantTime {
                                created: chrono::Utc::now().timestamp_millis() as u64,
                                completed: None,
                            },
                        });

                        let encoded = serde_json::to_value(&msg).unwrap_or_default();
                        let data_str = serde_json::to_string(&encoded).unwrap_or_default();
                        sqlx::query(
                            "INSERT INTO session_message (id, session_id, type, seq, time_created, data) \
                             VALUES (?1, ?2, 'assistant', ?3, ?4, ?5)",
                        )
                        .bind(assistant_msg_id)
                        .bind(&sid)
                        .bind(seq as i64)
                        .bind(chrono::Utc::now().timestamp_millis())
                        .bind(&data_str)
                        .execute(db.pool())
                        .await
                        .map_err(|e| {
                            crate::event::EventError::Internal(format!("insert msg: {e}"))
                        })?;

                        Ok(())
                    })
                }
            }),
        )
        .await;

    // ── Compaction.Ended ──────────────────────────────────────────────
    events
        .project(
            COMPACTION_ENDED,
            mk_projector_fn({
                let db = Arc::clone(&db);
                let epoch = Arc::clone(&epoch_mgr);
                move |payload| {
                    let db = db.clone();
                    let epoch = epoch.clone();
                    Box::pin(async move {
                        let sid = session_id(&payload.data).ok_or_else(|| {
                            crate::event::EventError::Internal("missing sessionID".into())
                        })?;
                        let msg_id = payload.data["messageID"].as_str().unwrap_or("");
                        let seq = payload.seq.ok_or_else(|| {
                            crate::event::EventError::Internal("missing seq".into())
                        })?;
                        let reason = payload.data["reason"].as_str().unwrap_or("auto");
                        let summary = payload.data["text"].as_str().unwrap_or("");
                        let recent = payload.data["recent"].as_str().unwrap_or("");

                        let msg = SessionMessage::Compaction(
                            crate::session_message::CompactionMessage {
                                id: msg_id.to_string(),
                                reason: reason.to_string(),
                                summary: summary.to_string(),
                                recent: recent.to_string(),
                                metadata: None,
                                time: MessageTime {
                                    created: chrono::Utc::now().timestamp_millis() as u64,
                                },
                            },
                        );

                        let encoded = serde_json::to_value(&msg).unwrap_or_default();
                        let data_str = serde_json::to_string(&encoded).unwrap_or_default();
                        sqlx::query(
                            "INSERT INTO session_message (id, session_id, type, seq, time_created, data) \
                             VALUES (?1, ?2, 'compaction', ?3, ?4, ?5)",
                        )
                        .bind(msg_id)
                        .bind(&sid)
                        .bind(seq as i64)
                        .bind(chrono::Utc::now().timestamp_millis())
                        .bind(&data_str)
                        .execute(db.pool())
                        .await
                        .map_err(|e| {
                            crate::event::EventError::Internal(format!("insert msg: {e}"))
                        })?;

                        epoch.request_replacement(&sid).await.ok();

                        Ok(())
                    })
                }
            }),
        )
        .await;

    // ── Placeholder projectors for remaining event types ──────────────
    // These are registered as no-ops (they still trigger session_message updates
    // via the `run(db, event)` function in the TS code). For now they just pass
    // through to ensure the event pipeline doesn't stall.

    let noop = mk_projector_fn(|_payload| Box::pin(async { Ok(()) }));

    events.project(STEP_ENDED, noop.clone()).await;
    events.project(STEP_FAILED, noop.clone()).await;
    events.project(TEXT_STARTED, noop.clone()).await;
    events.project(TEXT_ENDED, noop.clone()).await;
    events.project(TOOL_INPUT_STARTED, noop.clone()).await;
    events.project(TOOL_INPUT_ENDED, noop.clone()).await;
    events.project(TOOL_CALLED, noop.clone()).await;
    events.project(TOOL_PROGRESS, noop.clone()).await;
    events.project(TOOL_SUCCESS, noop.clone()).await;
    events.project(TOOL_FAILED, noop.clone()).await;
    events.project(REASONING_STARTED, noop.clone()).await;
    events.project(REASONING_ENDED, noop).await;
    events.project(INTERRUPT_REQUESTED, mk_projector_fn(|_payload| Box::pin(async { Ok(()) }))).await;

    Ok(())
}
