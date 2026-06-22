//! Fence middleware — reads EventSequenceTable before mutation endpoints,
//! computes diff after mutation, and emits `x-blazecode-sync` response header.
//!
//! Ported from: `packages/blazecode/src/server/routes/instance/httpapi/middleware/fence.ts`
//! and `packages/blazecode/src/server/shared/fence.ts`

use axum::extract::{Request, State};
use axum::middleware::Next;
use axum::response::{IntoResponse, Response};
use blazecode_core::database::EventSequenceRow;
use std::collections::HashMap;
use std::sync::Arc;

use crate::error::ServerError;
use crate::server::AppState;

/// Header name for fence sync state.
pub const FENCE_HEADER: &str = "x-blazecode-sync";

/// Fence state: aggregate_id → sequence number.
pub type FenceState = HashMap<String, i64>;

/// HTTP methods that are ignored by the fence (read-only methods).
const IGNORED_METHODS: &[&str] = &["GET", "HEAD", "OPTIONS"];

/// Load the current fence state from the database.
///
/// Returns a map of `aggregate_id → seq` for all event_sequence rows.
///
/// # Source
/// Ported from `packages/blazecode/src/server/shared/fence.ts` `load()`.
async fn load_fence_state(
    db: &blazecode_core::database::DatabaseService,
) -> Result<FenceState, ServerError> {
    let rows = db
        .list_all_event_sequences()
        .await
        .map_err(|e| ServerError::unknown(format!("fence load failed: {e}")))?;

    Ok(rows
        .into_iter()
        .map(|row: EventSequenceRow| (row.aggregate_id, row.seq))
        .collect())
}

/// Compute the diff between two fence states.
///
/// Returns entries where the sequence number changed, or entries that exist
/// only in `next` (new aggregates).
///
/// # Source
/// Ported from `packages/blazecode/src/server/shared/fence.ts` `diff()`.
pub fn fence_diff(prev: &FenceState, next: &FenceState) -> FenceState {
    let mut result = FenceState::new();
    let mut all_keys: Vec<&String> = Vec::new();

    for key in prev.keys() {
        all_keys.push(key);
    }
    for key in next.keys() {
        if !prev.contains_key(key) {
            all_keys.push(key);
        }
    }

    // Deduplicate
    all_keys.sort();
    all_keys.dedup();

    for key in all_keys {
        let prev_seq = prev.get(key).copied().unwrap_or(-1);
        let next_seq = next.get(key).copied().unwrap_or(-1);
        if prev_seq != next_seq {
            result.insert(key.clone(), next_seq);
        }
    }

    result
}

/// Parse a fence state from a `x-blazecode-sync` header value.
///
/// # Source
/// Ported from `packages/blazecode/src/server/shared/fence.ts` `parse()`.
pub fn parse_fence_header(raw: &str) -> Option<FenceState> {
    let data: serde_json::Value = serde_json::from_str(raw).ok()?;

    match data {
        serde_json::Value::Object(map) => {
            let mut state = FenceState::new();
            for (key, value) in map {
                if let Some(n) = value.as_i64() {
                    state.insert(key, n);
                }
            }
            Some(state)
        }
        _ => None,
    }
}

/// Wait for a fence state to be reached on a workspace (blocking call).
///
/// In the TS, this calls `Workspace.Service.use((svc) => svc.waitForSync(...))`.
/// In the Rust port, this is a simple poll-based wait for now.
///
/// # Source
/// Ported from `packages/blazecode/src/server/shared/fence.ts` `wait()`.
pub async fn wait_for_fence_state(
    db: &blazecode_core::database::DatabaseService,
    target: &FenceState,
    max_retries: u32,
) -> Result<(), ServerError> {
    for _ in 0..max_retries {
        let current = load_fence_state(db).await?;
        let diff = fence_diff(&current, target);
        if diff.is_empty() {
            return Ok(());
        }
        tokio::time::sleep(std::time::Duration::from_millis(50)).await;
    }
    Err(ServerError::timeout(
        "fence state not reached within retry limit",
    ))
}

/// Fence middleware for axum.
///
/// Captures the event_sequence state before and after mutation endpoints,
/// computes the diff, and emits it as the `x-blazecode-sync` response header.
///
/// Only applies to mutation methods (POST, PUT, PATCH, DELETE).
///
/// # Source
/// Ported from `packages/blazecode/src/server/routes/instance/httpapi/middleware/fence.ts`
pub async fn fence_middleware(
    State(state): State<Arc<AppState>>,
    req: Request,
    next: Next,
) -> Response {
    let method = req.method().to_string();

    // Skip read-only methods
    if IGNORED_METHODS.contains(&method.as_str()) {
        return next.run(req).await;
    }

    // Check if BLAZECODE_WORKSPACE_ID env var is set (matches TS `Flag.BLAZECODE_WORKSPACE_ID`)
    let has_workspace_id = std::env::var("BLAZECODE_WORKSPACE_ID")
        .ok()
        .is_some_and(|v| !v.is_empty());
    if !has_workspace_id {
        return next.run(req).await;
    }

    let db = state.sessions.db();

    // Capture state before the handler
    let previous = match load_fence_state(db).await {
        Ok(s) => s,
        Err(e) => return e.into_response(),
    };

    // Run the handler
    let response = next.run(req).await;

    // Capture state after the handler
    let after = match load_fence_state(db).await {
        Ok(s) => s,
        Err(_) => return response, // Can't compute diff, return original response
    };

    // Compute diff
    let diff = fence_diff(&previous, &after);
    if diff.is_empty() {
        return response;
    }

    // Serialize diff to JSON and add header
    let header_value = match serde_json::to_string(&diff) {
        Ok(v) => v,
        Err(_) => return response,
    };

    let (mut parts, body) = response.into_parts();
    parts
        .headers
        .insert(FENCE_HEADER, header_value.parse().unwrap());
    Response::from_parts(parts, body)
}
