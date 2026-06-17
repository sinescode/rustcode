//! Event SSE route — server-sent event stream for session events.
//!
//! Ported from: `packages/opencode/src/server/routes/instance/httpapi/groups/event.ts`
//! and `packages/opencode/src/server/routes/instance/httpapi/handlers/event.ts`
//!
//! Route: `GET /event` → `text/event-stream`
//!
//! Events emitted:
//! - `server.connected` — sent immediately on connect
//! - `server.heartbeat` — every 10 seconds
//! - `<EventV2 type>` — all bus events filtered by directory/workspace
//! - `server.instance.disposed` — terminates stream
//!
//! # Source
//! The TS handler (event.ts lines 28–86):
//! 1. Emits `server.connected` immediately
//! 2. Subscribes to bus events
//! 3. Filters by directory and optional workspaceID from query params
//! 4. Merges with a heartbeat stream (10s interval)
//! 5. Terminates on `server.instance.disposed` event

use axum::extract::{Query, State};
use axum::response::sse::Event as SseEvent;
use axum::response::Sse;
use axum::Router;
use axum::routing::get;
use futures::stream::Stream;
use serde::Deserialize;
use std::convert::Infallible;
use std::sync::Arc;
use std::time::Duration;
use tokio_stream::wrappers::BroadcastStream;
use tokio_stream::StreamExt;

use crate::server::AppState;

/// Query parameters for the SSE event stream.
///
/// # Source
/// `WorkspaceRoutingQuery` — `directory` filters events to a specific project
/// directory, `workspace` further scopes to a workspace ID.
#[derive(Debug, Deserialize, Default)]
pub struct EventQuery {
    #[serde(default)]
    pub directory: Option<String>,
    #[serde(default)]
    pub workspace: Option<String>,
}

/// Create the event SSE route.
pub fn event_routes(state: Arc<AppState>) -> Router {
    Router::new()
        .route("/event", get(event_stream))
        .with_state(state)
}

/// `GET /event` — SSE event stream.
///
/// # Source
/// `packages/opencode/src/server/routes/instance/httpapi/handlers/event.ts` lines 28–86.
async fn event_stream(
    State(state): State<Arc<AppState>>,
    Query(query): Query<EventQuery>,
) -> Sse<impl Stream<Item = Result<SseEvent, Infallible>>> {
    let directory = query.directory;
    let workspace = query.workspace;

    // Subscribe to the bus
    let bus_rx = state.bus.subscribe();
    let bus_stream = BroadcastStream::new(bus_rx_to_receiver(bus_rx));

    // Create heartbeat stream (every 10 seconds, matching TS)
    let heartbeat = tokio_stream::wrappers::IntervalStream::new(
        tokio::time::interval(Duration::from_secs(10)),
    );

    // Initial connected event
    let connected = tokio_stream::once(Ok(SseEvent::default()
        .event("server.connected")
        .data(r#"{}"#)));

    // Filter bus events by directory/workspace, then map to SSE
    let events = bus_stream.filter_map(move |result| {
        let dir = directory.clone();
        let ws = workspace.clone();
        async move {
            match result {
                Ok(event) => {
                    // Filter by directory
                    if let Some(ref d) = dir {
                        if event.directory.as_deref() != Some(d.as_str()) {
                            return None;
                        }
                    }
                    // Filter by workspace
                    if let Some(ref w) = ws {
                        if event.workspace.as_deref() != Some(w.as_str()) {
                            return None;
                        }
                    }

                    let event_type = event
                        .payload
                        .get("type")
                        .and_then(|t| t.as_str())
                        .unwrap_or("message");

                    let data = serde_json::to_string(&event.payload).unwrap_or_default();

                    Some(Ok(SseEvent::default().event(event_type).data(data)))
                }
                Err(e) => {
                    tracing::warn!("SSE bus stream lagged: {e}");
                    None
                }
            }
        }
    });

    // Heartbeat events
    let heartbeats = heartbeat.map(|_| {
        Ok(SseEvent::default()
            .event("server.heartbeat")
            .data(r#"{}"#))
    });

    // Merge: connected → events → heartbeats
    let stream = connected
        .chain(events)
        .merge(heartbeats);

    Sse::new(stream).keep_alive(
        axum::response::sse::KeepAlive::new()
            .interval(Duration::from_secs(15))
            .text("ping"),
    )
}

/// Convert a `BusSubscription` into a `tokio::sync::broadcast::Receiver` for
/// use with `BroadcastStream`. This uses an internal channel to bridge the
/// subscription's async recv() into a broadcast receiver.
fn bus_rx_to_receiver(
    mut sub: rustcode_core::bus::BusSubscription,
) -> tokio::sync::broadcast::Receiver<rustcode_core::bus::GlobalEvent> {
    let (tx, rx) = tokio::sync::broadcast::channel(256);
    tokio::spawn(async move {
        loop {
            match sub.recv().await {
                Some(event) => {
                    if tx.send(event).is_err() {
                        // All receivers dropped
                        break;
                    }
                }
                None => break, // Bus closed
            }
        }
    });
    rx
}
