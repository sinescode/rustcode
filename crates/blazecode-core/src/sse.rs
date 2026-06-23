//! Server-Sent Events (SSE) stream parser.
//!
//! Parses an HTTP response body into a stream of SSE events. Used by
//! Anthropic, OpenAI, Gemini, and OpenAI-compatible providers.
//!
//! Ported from:
//! - `packages/llm/src/route/framing.ts` (28 lines)
//! - `packages/llm/src/route/transport/http.ts` (108 lines)
//! - `packages/llm/src/protocols/shared.ts` (349 lines)
//!
//! BlazeCode commit: 5d0f86606ac30690f79f0a6a9f41a1f49fe95d0b

use bytes::Bytes;
use futures::{Stream, StreamExt};
use std::collections::VecDeque;
use tokio_util::io::StreamReader;

/// An SSE event parsed from the stream.
#[derive(Debug, Clone)]
pub struct SseEvent {
    /// Event type (the `event:` field), if present
    pub event_type: Option<String>,
    /// Event data (the `data:` field concatenated across lines)
    pub data: String,
    /// Event ID (the `id:` field), if present
    pub id: Option<String>,
    /// Retry timeout in milliseconds (the `retry:` field), if present
    pub retry_ms: Option<u64>,
}

impl SseEvent {
    /// Returns true if this is a `[DONE]` sentinel event (OpenAI convention).
    pub fn is_done(&self) -> bool {
        self.data.trim() == "[DONE]"
    }

    /// Returns true if this event has meaningful data.
    pub fn has_data(&self) -> bool {
        !self.data.is_empty()
    }
}

/// Error from SSE parsing.
#[derive(Debug, thiserror::Error)]
pub enum SseError {
    /// I/O error reading the stream
    #[error("SSE read error: {0}")]
    Io(#[from] std::io::Error),

    /// Stream ended unexpectedly
    #[error("SSE stream ended unexpectedly")]
    UnexpectedEnd,

    /// The event data field exceeded the maximum allowed size
    #[error("SSE event data too large: {0} bytes")]
    DataTooLarge(usize),
}

/// Maximum size of a single SSE event's data field (1 MiB).
pub const MAX_SSE_EVENT_SIZE: usize = 1024 * 1024;

/// Parse a response body into a stream of SSE events.
///
/// This function reads the body as a byte stream, splits on double-newline
/// boundaries (SSE event separators), and parses `event:`, `data:`, `id:`,
/// and `retry:` fields from each event block.
///
/// # Source
/// Ported from `packages/llm/src/route/transport/http.ts` and
/// `packages/llm/src/protocols/shared.ts` (SSE framing).
pub fn parse_sse_stream(
    body: reqwest::Response,
) -> impl Stream<Item = Result<SseEvent, SseError>> + Send + Unpin {
    let byte_stream = body
        .bytes_stream()
        .map(|result| result.map_err(std::io::Error::other));

    let reader = StreamReader::new(byte_stream);
    let lines = tokio_util::io::ReaderStream::new(reader);

    // We need to accumulate lines within an SSE event block.
    // The framing algorithm: accumulate lines until we hit an empty line
    // (just "\n"), then parse the accumulated block.
    SseEventStream {
        lines: Box::pin(lines),
        buffer: Vec::new(),
        current_event_type: None,
        current_data: String::new(),
        current_id: None,
        current_retry: None,
        data_size: 0,
        done: false,
        pending: VecDeque::new(),
    }
}

// Inner stream implementation
struct SseEventStream<S> {
    lines: std::pin::Pin<Box<S>>,
    buffer: Vec<u8>,
    current_event_type: Option<String>,
    current_data: String,
    current_id: Option<String>,
    current_retry: Option<u64>,
    data_size: usize,
    done: bool,
    /// Buffer of fully-parsed events ready to yield (framing workaround).
    /// When a single HTTP chunk contains multiple SSE events, we parse
    /// them all at once and buffer them here to avoid losing events on
    /// stream-end flush.
    pending: VecDeque<Result<SseEvent, SseError>>,
}

impl<S> Stream for SseEventStream<S>
where
    S: Stream<Item = Result<Bytes, std::io::Error>> + Unpin,
{
    type Item = Result<SseEvent, SseError>;

    fn poll_next(
        mut self: std::pin::Pin<&mut Self>,
        cx: &mut std::task::Context<'_>,
    ) -> std::task::Poll<Option<Self::Item>> {
        // Drain pending events from previous chunk processing first
        if let Some(event) = self.pending.pop_front() {
            return std::task::Poll::Ready(Some(event));
        }

        loop {
            if self.done {
                return std::task::Poll::Ready(None);
            }

            // Read next chunk
            match self.lines.as_mut().poll_next(cx) {
                std::task::Poll::Ready(Some(Ok(bytes))) => {
                    self.buffer.extend_from_slice(&bytes);

                    // Process ALL complete event boundaries in the buffer
                    loop {
                        if let Some(pos) = find_double_newline(&self.buffer) {
                            let block_bytes = self.buffer[..pos].to_vec();
                            let remainder = self.buffer[pos + 2..].to_vec();

                            // Parse the event block (lines joined by \n)
                            if let Ok(block_str) = std::str::from_utf8(&block_bytes) {
                                self.parse_event_block(block_str);
                            }

                            self.buffer = remainder;

                            // Check for done sentinel
                            if self.current_data.trim().eq_ignore_ascii_case("[DONE]") {
                                self.done = true;
                                let event = self.take_event().unwrap_or(SseEvent {
                                    event_type: None,
                                    data: "[DONE]".into(),
                                    id: None,
                                    retry_ms: None,
                                });
                                self.pending.push_back(Ok(event));
                                break;
                            }

                            // Yield complete event or continue to next boundary
                            if let Some(event) = self.take_event() {
                                self.pending.push_back(Ok(event));
                            } else {
                                continue;
                            }
                        } else {
                            break;
                        }
                    }

                    // Check for buffer overflow
                    if self.buffer.len() > MAX_SSE_EVENT_SIZE * 2 {
                        return std::task::Poll::Ready(Some(Err(SseError::DataTooLarge(
                            self.buffer.len(),
                        ))));
                    }

                    // Yield pending events (at least one should exist from the loop above)
                    if let Some(event) = self.pending.pop_front() {
                        return std::task::Poll::Ready(Some(event));
                    }
                }
                std::task::Poll::Ready(Some(Err(e))) => {
                    return std::task::Poll::Ready(Some(Err(SseError::Io(e))));
                }
                std::task::Poll::Ready(None) => {
                    // Stream ended — flush remaining buffer, processing all boundaries
                    if !self.buffer.is_empty() || !self.current_data.is_empty() {
                        let remaining = std::mem::take(&mut self.buffer);
                        let mut pos = 0usize;

                        // Walk through remaining bytes, splitting on \n\n
                        loop {
                            let sub = &remaining[pos..];
                            if let Some(dnl) = sub.windows(2).position(|w| w == b"\n\n") {
                                let block_end = pos + dnl;
                                if block_end > pos {
                                    let block_bytes = &remaining[pos..block_end];
                                    if let Ok(block_str) = std::str::from_utf8(block_bytes) {
                                        if !block_str.trim().is_empty() {
                                            self.parse_event_block(block_str);
                                            if let Some(event) = self.take_event() {
                                                self.pending.push_back(Ok(event));
                                            }
                                        }
                                    }
                                }
                                pos = block_end + 2;
                            } else {
                                // No more \n\n — treat remaining as one block
                                let block_bytes = &remaining[pos..];
                                if !block_bytes.is_empty() {
                                    if let Ok(block_str) = std::str::from_utf8(block_bytes) {
                                        if !block_str.trim().is_empty() {
                                            self.parse_event_block(block_str);
                                            if let Some(event) = self.take_event() {
                                                self.pending.push_back(Ok(event));
                                            }
                                        }
                                    }
                                }
                                break;
                            }
                        }

                        if let Some(event) = self.pending.pop_front() {
                            self.done = true;
                            return std::task::Poll::Ready(Some(event));
                        }
                    }
                    self.done = true;
                    return std::task::Poll::Ready(None);
                }
                std::task::Poll::Pending => return std::task::Poll::Pending,
            }
        }
    }
}

impl<S> SseEventStream<S>
where
    S: Stream<Item = Result<Bytes, std::io::Error>> + Unpin,
{
    fn parse_event_block(&mut self, block: &str) {
        // Fields accumulate across lines within a block.
        // The block may contain multiple events separated by blank lines,
        // but we process one block at a time.

        // We only accumulate. The final event is emitted when a double-newline
        // boundary is hit. So each block may add to the current in-progress event.
        for line in block.lines() {
            let line = line.trim_end_matches('\r');

            if line.is_empty() {
                // Empty line within a block — separator between event parts
                continue;
            }

            if let Some(value) = line.strip_prefix("event:") {
                // Only keep the last event type per block
                self.current_event_type = Some(value.trim().to_string());
            } else if let Some(value) = line.strip_prefix("data:") {
                let value = value.strip_prefix(' ').unwrap_or(value);
                if !self.current_data.is_empty() {
                    self.current_data.push('\n');
                }
                self.current_data.push_str(value);
                self.data_size += value.len();
            } else if let Some(value) = line.strip_prefix("id:") {
                let value = value.strip_prefix(' ').unwrap_or(value);
                self.current_id = Some(value.trim().to_string());
            } else if let Some(value) = line.strip_prefix("retry:") {
                let value = value.strip_prefix(' ').unwrap_or(value);
                if let Ok(ms) = value.trim().parse() {
                    self.current_retry = Some(ms);
                }
            } else if line.starts_with(':') {
                // Comment line — ignore
            }
            // Unknown field — ignore
        }
    }

    fn take_event(&mut self) -> Option<SseEvent> {
        // An SSE event is "complete" when we hit a double-newline boundary.
        // At that point, if we have data, we yield the event.
        // The parse_event_block was called before this, so current_data may be populated.

        if self.current_data.is_empty() && self.current_event_type.is_none() {
            return None;
        }

        let event = SseEvent {
            event_type: self.current_event_type.take(),
            data: std::mem::take(&mut self.current_data),
            id: self.current_id.take(),
            retry_ms: self.current_retry.take(),
        };
        self.data_size = 0;
        Some(event)
    }
}

/// Find the position of the first double-newline (b"\n\n") in the buffer.
fn find_double_newline(buf: &[u8]) -> Option<usize> {
    buf.windows(2).position(|w| w == b"\n\n")
}

#[cfg(test)]
mod tests {
    use super::*;
    use bytes::Bytes;
    use futures::stream;

    fn sse_chunks(data: &str) -> impl Stream<Item = Result<Bytes, std::io::Error>> + Unpin {
        let chunks: Vec<Result<Bytes, std::io::Error>> = data
            .as_bytes()
            .chunks(1)
            .map(|c| Ok(Bytes::copy_from_slice(c)))
            .collect();
        Box::pin(stream::iter(chunks))
    }

    #[tokio::test]
    async fn test_simple_sse_event() {
        let input = "data: hello world\n\ndata: goodbye\n\n";
        let stream = SseEventStream {
            lines: Box::pin(sse_chunks(input)),
            buffer: Vec::new(),
            current_event_type: None,
            current_data: String::new(),
            current_id: None,
            current_retry: None,
            data_size: 0,
            done: false,
        };

        let events: Vec<SseEvent> =
            futures::StreamExt::collect::<Vec<Result<SseEvent, SseError>>>(stream)
                .await
                .into_iter()
                .filter_map(|r| r.ok())
                .collect();

        assert_eq!(events.len(), 2);
        assert_eq!(events[0].data, "hello world");
        assert_eq!(events[1].data, "goodbye");
    }

    #[tokio::test]
    async fn test_sse_with_event_type() {
        let input =
            "event: message_start\ndata: {\"type\":\"start\"}\n\nevent: content_block_delta\ndata: {\"text\":\"hi\"}\n\n";
        let stream = SseEventStream {
            lines: Box::pin(sse_chunks(input)),
            buffer: Vec::new(),
            current_event_type: None,
            current_data: String::new(),
            current_id: None,
            current_retry: None,
            data_size: 0,
            done: false,
        };

        let events: Vec<SseEvent> =
            futures::StreamExt::collect::<Vec<Result<SseEvent, SseError>>>(stream)
                .await
                .into_iter()
                .filter_map(|r| r.ok())
                .collect();

        assert_eq!(events.len(), 2);
        assert_eq!(events[0].event_type.as_deref(), Some("message_start"));
        assert_eq!(events[0].data, r#"{"type":"start"}"#);
        assert_eq!(events[1].event_type.as_deref(), Some("content_block_delta"));
        assert_eq!(events[1].data, r#"{"text":"hi"}"#);
    }

    #[tokio::test]
    async fn test_sse_multiline_data() {
        let input = "data: line1\ndata: line2\ndata: line3\n\n";
        let stream = SseEventStream {
            lines: Box::pin(sse_chunks(input)),
            buffer: Vec::new(),
            current_event_type: None,
            current_data: String::new(),
            current_id: None,
            current_retry: None,
            data_size: 0,
            done: false,
        };

        let events: Vec<SseEvent> =
            futures::StreamExt::collect::<Vec<Result<SseEvent, SseError>>>(stream)
                .await
                .into_iter()
                .filter_map(|r| r.ok())
                .collect();

        assert_eq!(events.len(), 1);
        assert_eq!(events[0].data, "line1\nline2\nline3");
    }

    #[tokio::test]
    async fn test_sse_event_is_done() {
        let event = SseEvent {
            event_type: None,
            data: "[DONE]".into(),
            id: None,
            retry_ms: None,
        };
        assert!(event.is_done());

        let event2 = SseEvent {
            event_type: None,
            data: "something else".into(),
            id: None,
            retry_ms: None,
        };
        assert!(!event2.is_done());
    }

    #[tokio::test]
    async fn test_find_double_newline() {
        assert_eq!(find_double_newline(b"hello\n\nworld"), Some(5));
        assert_eq!(find_double_newline(b"no double newline"), None);
        assert_eq!(find_double_newline(b"\n\nstart"), Some(0));
    }
}
