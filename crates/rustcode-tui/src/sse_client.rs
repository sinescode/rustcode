//! SSE client for connecting the TUI to a remote rustcode server.
//!
//! Ported from: `packages/tui/src/context/sync.ts` (the SSE subscription pattern)
//! and `packages/opencode/src/server/routes/instance/httpapi/handlers/event.ts`
//!
//! ## Architecture
//!
//! The `SseClient` connects to `GET {base_url}/event` on the remote server,
//! receives Server-Sent Events, parses them into `TuiEvent` variants, and
//! broadcasts them to all subscribers via a `tokio::sync::broadcast` channel.
//!
//! Auto-reconnect with exponential backoff ensures the TUI stays connected
//! even when the server restarts or the network is unstable.

use reqwest::header::HeaderMap;
use std::time::Duration;
use tokio::sync::broadcast;
use tracing::{debug, error, info, warn};

use crate::event::{
    CommandExecuteProperties, PromptAppendProperties, SessionSelectProperties, ToastProperties,
    TuiEvent,
};

/// Initial reconnect delay (milliseconds).
const INITIAL_RECONNECT_DELAY_MS: u64 = 500;

/// Maximum reconnect delay (milliseconds).
const MAX_RECONNECT_DELAY_MS: u64 = 30_000;

/// Multiplier applied to the delay after each failed reconnect.
const BACKOFF_MULTIPLIER: f64 = 2.0;

/// Fuzz factor applied to each delay to avoid thundering herd (±20%).
/// Set to 0 to disable fuzz.
const BACKOFF_FUZZ: f64 = 0.2;

/// SSE client that connects to a remote rustcode server's event stream.
///
/// Events are received via SSE and broadcast to all subscribers through
/// a [`tokio::sync::broadcast`] channel.
///
/// # Source
/// Ported from `packages/tui/src/context/sync.ts` — the SSE subscription
/// that feeds the TUI event loop.
pub struct SseClient {
    /// Base URL of the remote server (e.g. "http://localhost:4096").
    base_url: String,
    /// HTTP headers (for auth, etc.).
    headers: HeaderMap,
    /// Broadcast sender for TUI events.
    events_tx: broadcast::Sender<TuiEvent>,
}

impl SseClient {
    /// Create a new SSE client for the given server URL.
    ///
    /// The `base_url` should be the server root, e.g. `"http://localhost:4096"`.
    /// The client connects to `{base_url}/event` for the SSE stream.
    pub fn new(base_url: &str) -> Self {
        let (events_tx, _) = broadcast::channel(256);
        Self {
            base_url: base_url.trim_end_matches('/').to_string(),
            headers: HeaderMap::new(),
            events_tx,
        }
    }

    /// Set the auth headers (Basic auth with username/password).
    ///
    /// If `password` is `None`, no auth header is set.
    pub fn set_auth(&mut self, username: &str, password: Option<&str>) {
        if let Some(pw) = password {
            let auth = format!("{username}:{pw}");
            let encoded = base64_encode(&auth);
            if let Ok(value) = reqwest::header::HeaderValue::from_str(&format!("Basic {encoded}")) {
                self.headers.insert(reqwest::header::AUTHORIZATION, value);
            }
        }
    }

    /// Set arbitrary HTTP headers.
    pub fn set_headers(&mut self, headers: HeaderMap) {
        self.headers = headers;
    }

    /// Subscribe to TUI events broadcast by this client.
    ///
    /// Returns a receiver that yields events as they arrive from the server.
    /// Multiple subscribers can exist simultaneously (fan-out).
    pub fn subscribe(&self) -> broadcast::Receiver<TuiEvent> {
        self.events_tx.subscribe()
    }

    /// Connect to the remote server and start streaming events.
    ///
    /// This method runs indefinitely, reconnecting on disconnect with
    /// exponential backoff. It should be spawned in a background tokio task.
    ///
    /// # Errors
    /// Returns an error only if the initial connection setup fails fatally.
    /// Transient errors trigger automatic reconnection.
    pub async fn connect(&self) -> anyhow::Result<()> {
        let event_url = format!("{}/event", self.base_url);
        info!("SSE client connecting to {event_url}");

        let client = reqwest::Client::builder()
            .timeout(Duration::from_secs(0)) // no overall timeout for SSE
            .connect_timeout(Duration::from_secs(10))
            .build()
            .map_err(|e| anyhow::anyhow!("Failed to build HTTP client: {e}"))?;

        let mut reconnect_delay = INITIAL_RECONNECT_DELAY_MS;

        loop {
            let request = client
                .get(&event_url)
                .headers(self.headers.clone())
                .header("Accept", "text/event-stream")
                .header("Cache-Control", "no-cache");

            match request.send().await {
                Ok(response) => {
                    if !response.status().is_success() {
                        let status = response.status();
                        if status.as_u16() == 401 {
                            error!(
                                "SSE: authentication failed (HTTP 401) — check username/password"
                            );
                            return Err(anyhow::anyhow!(
                                "SSE authentication failed: server returned 401"
                            ));
                        }
                        warn!("SSE: server returned HTTP {status}, retrying...");
                        sleep_with_jitter(reconnect_delay, BACKOFF_FUZZ).await;
                        reconnect_delay = next_backoff(
                            reconnect_delay,
                            MAX_RECONNECT_DELAY_MS,
                            BACKOFF_MULTIPLIER,
                        );
                        continue;
                    }

                    // Reset backoff on successful connection
                    reconnect_delay = INITIAL_RECONNECT_DELAY_MS;
                    info!("SSE: connected to {event_url}");

                    // Stream the response body as SSE
                    if let Err(e) = self.stream_events(response).await {
                        warn!("SSE: stream ended: {e} — reconnecting...");
                    }
                }
                Err(e) => {
                    warn!("SSE: connection failed: {e} — retrying in {reconnect_delay}ms");
                }
            }

            // Exponential backoff before reconnecting
            sleep_with_jitter(reconnect_delay, BACKOFF_FUZZ).await;
            reconnect_delay =
                next_backoff(reconnect_delay, MAX_RECONNECT_DELAY_MS, BACKOFF_MULTIPLIER);
        }
    }

    /// Stream SSE events from the response, parsing and broadcasting each one.
    async fn stream_events(&self, response: reqwest::Response) -> anyhow::Result<()> {
        use futures::StreamExt;

        let mut stream = response.bytes_stream();
        let mut buffer = String::new();
        let mut current_event: Option<String> = None;
        let mut current_data = String::new();

        while let Some(chunk_result) = stream.next().await {
            let chunk = match chunk_result {
                Ok(c) => c,
                Err(e) => {
                    warn!("SSE: chunk error: {e}");
                    return Err(anyhow::anyhow!("SSE chunk error: {e}"));
                }
            };

            // Convert chunk to string, handling partial UTF-8
            let chunk_str = String::from_utf8_lossy(&chunk);
            buffer.push_str(&chunk_str);

            // Process complete lines
            while let Some(line_end) = buffer.find('\n') {
                let mut line = buffer[..=line_end].to_string();
                buffer = buffer[line_end + 1..].to_string();

                // Trim trailing \r
                line = line.trim_end_matches(['\r', '\n']).to_string();

                if line.is_empty() {
                    // Empty line = end of event
                    if !current_data.is_empty() {
                        let event_type = current_event
                            .take()
                            .unwrap_or_else(|| "message".to_string());
                        self.dispatch_sse_event(&event_type, &current_data);
                        current_data.clear();
                    }
                } else if let Some(field_value) = line.strip_prefix("event:") {
                    current_event = Some(field_value.trim().to_string());
                } else if let Some(field_value) = line.strip_prefix("data:") {
                    if !current_data.is_empty() {
                        current_data.push('\n');
                    }
                    current_data.push_str(field_value.trim());
                } else if line.starts_with(':') {
                    // Comment line — ignore
                }
                // Ignore other fields (id:, retry:)
            }
        }

        // Flush any partial event at end of stream
        if !current_data.is_empty() {
            let event_type = current_event.unwrap_or_else(|| "message".to_string());
            self.dispatch_sse_event(&event_type, &current_data);
        }

        Ok(())
    }

    /// Parse the SSE event data and dispatch to the appropriate TuiEvent variant.
    fn dispatch_sse_event(&self, event_type: &str, data: &str) {
        // Parse the JSON payload from data
        let payload: serde_json::Value = match serde_json::from_str(data) {
            Ok(v) => v,
            Err(e) => {
                debug!("SSE: failed to parse event data as JSON: {e}");
                return;
            }
        };

        let tui_event = match event_type {
            "tui.prompt.append" => {
                let text = payload.get("text").and_then(|v| v.as_str()).unwrap_or("");
                TuiEvent::PromptAppend {
                    properties: PromptAppendProperties {
                        text: text.to_string(),
                    },
                }
            }
            "tui.command.execute" => {
                let command = payload
                    .get("command")
                    .and_then(|v| v.as_str())
                    .unwrap_or("");
                TuiEvent::CommandExecute {
                    properties: CommandExecuteProperties {
                        command: command.to_string(),
                    },
                }
            }
            "tui.toast.show" => {
                let title = payload
                    .get("title")
                    .and_then(|v| v.as_str())
                    .map(String::from);
                let message = payload
                    .get("message")
                    .and_then(|v| v.as_str())
                    .unwrap_or("")
                    .to_string();
                let variant = payload
                    .get("variant")
                    .and_then(|v| v.as_str())
                    .unwrap_or("info")
                    .to_string();
                let duration = payload
                    .get("duration")
                    .and_then(|v| v.as_u64())
                    .unwrap_or(5000);
                TuiEvent::ToastShow {
                    properties: ToastProperties {
                        title,
                        message,
                        variant,
                        duration,
                    },
                }
            }
            "tui.session.select" => {
                let session_id = payload
                    .get("session_id")
                    .and_then(|v| v.as_str())
                    .unwrap_or("")
                    .to_string();
                TuiEvent::SessionSelect {
                    properties: SessionSelectProperties { session_id },
                }
            }
            // Session-related events from the bus
            "session.created" | "session.updated" | "session.deleted" => {
                // These are informational — the TUI handles them via bus events
                let _ = payload;
                return;
            }
            "message.updated" | "message.deleted" | "part.updated" => {
                // Handled via bus events
                return;
            }
            // Unknown event types — log and skip
            _ => {
                debug!("SSE: unknown event type: {event_type}");
                return;
            }
        };

        if let Err(e) = self.events_tx.send(tui_event) {
            debug!("SSE: no active TUI subscribers for event {event_type}: {e}");
        }
    }
}

// ── Helpers ──────────────────────────────────────────────────────────────────

/// Compute the next backoff delay.
fn next_backoff(current_ms: u64, max_ms: u64, multiplier: f64) -> u64 {
    let next = (current_ms as f64 * multiplier) as u64;
    next.min(max_ms).max(500)
}

/// Sleep for `base_ms` ± `fuzz` percentage jitter.
async fn sleep_with_jitter(base_ms: u64, fuzz: f64) {
    let jitter = if fuzz > 0.0 {
        let range = (base_ms as f64 * fuzz) as i64;
        if range > 0 {
            // Deterministic-ish jitter based on current time
            let seed = std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap_or_default()
                .subsec_nanos() as i64
                % (range * 2 + 1);
            seed - range
        } else {
            0
        }
    } else {
        0
    };
    let delay_ms = (base_ms as i64 + jitter).max(100) as u64;
    tokio::time::sleep(Duration::from_millis(delay_ms)).await;
}

/// Simple base64 encoder matching the one in main.rs.
fn base64_encode(input: &str) -> String {
    const CHARS: &[u8] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
    let bytes = input.as_bytes();
    let mut result = String::new();

    for chunk in bytes.chunks(3) {
        let b0 = chunk[0] as u32;
        let b1 = if chunk.len() > 1 { chunk[1] as u32 } else { 0 };
        let b2 = if chunk.len() > 2 { chunk[2] as u32 } else { 0 };
        let combined = (b0 << 16) | (b1 << 8) | b2;

        result.push(CHARS[((combined >> 18) & 0x3F) as usize] as char);
        result.push(CHARS[((combined >> 12) & 0x3F) as usize] as char);

        if chunk.len() > 1 {
            result.push(CHARS[((combined >> 6) & 0x3F) as usize] as char);
        } else {
            result.push('=');
        }

        if chunk.len() > 2 {
            result.push(CHARS[(combined & 0x3F) as usize] as char);
        } else {
            result.push('=');
        }
    }

    result
}
