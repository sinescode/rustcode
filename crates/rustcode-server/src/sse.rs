//! Server-Sent Events (SSE) streaming utilities.
//!
//! Ported from: `packages/opencode/src/server/routes/instance/httpapi/handlers/event.ts`
//! and `packages/opencode/src/server/routes/instance/httpapi/groups/event.ts`
//!
//! The TS source uses `HttpApiSchema.asText({ contentType: "text/event-stream" })`
//! for the SSE endpoint. This module provides the Rust equivalent.

use axum::response::sse::{Event as SseEvent, KeepAlive, Sse};
use futures::stream::Stream;
use std::convert::Infallible;
use std::time::Duration;

/// Default keep-alive interval for SSE connections.
///
/// # Source
/// Matches the default effect/platform `HttpServer.sse` keep-alive (15 seconds).
const KEEP_ALIVE_INTERVAL_SECS: u64 = 15;

/// Build an SSE stream from an event stream.
///
/// Each item in the input stream is serialized as an SSE event.
/// The `event:` field is set from the `type` property of the JSON payload.
///
/// # Source
/// Ported from `packages/opencode/src/server/routes/instance/httpapi/handlers/event.ts`
/// which transforms bus events into SSE format with correct `event:` type names.
pub fn sse_stream<S, E>(
    stream: S,
) -> Sse<impl Stream<Item = Result<SseEvent, Infallible>>>
where
    S: Stream<Item = Result<serde_json::Value, E>> + Send + 'static,
    E: std::error::Error + Send + Sync + 'static,
{
    let event_stream = stream.map(|item| match item {
        Ok(value) => {
            let event_type = value
                .get("type")
                .and_then(|t| t.as_str())
                .unwrap_or("message");

            let data = serde_json::to_string(&value).unwrap_or_default();

            Ok(SseEvent::default()
                .event(event_type)
                .data(data))
        }
        Err(e) => {
            tracing::warn!("SSE stream error: {e}");
            Ok(SseEvent::default()
                .event("error")
                .data(format!(r#"{{"error":"{}"}}"#, e)))
        }
    });

    Sse::new(event_stream).keep_alive(
        KeepAlive::new()
            .interval(Duration::from_secs(KEEP_ALIVE_INTERVAL_SECS))
            .text("ping"),
    )
}
