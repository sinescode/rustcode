//! HTTP and WebSocket proxy utilities for forwarding requests to remote workspace URLs.
//!
//! Ported from: `packages/opencode/src/server/routes/instance/httpapi/middleware/proxy.ts`
//! and `packages/opencode/src/server/proxy-util.ts`

use axum::body::Body;
use axum::extract::ws::{Message, WebSocket};
use axum::extract::{Request, WebSocketUpgrade};
use axum::http::{HeaderMap, Method, Uri};
use axum::response::{IntoResponse, Response};
use futures::{SinkExt, StreamExt};

use crate::error::ServerError;
use crate::workspace_routing::RemoteWorkspaceTarget;

/// Hop-by-hop headers that must be stripped from forwarded requests.
///
/// # Source
/// Ported from `packages/opencode/src/server/proxy-util.ts` `hop` set.
const HOP_BY_HOP: &[&str] = &[
    "connection",
    "keep-alive",
    "proxy-authenticate",
    "proxy-authorization",
    "proxy-connection",
    "te",
    "trailer",
    "transfer-encoding",
    "upgrade",
    "host",
];

/// Sanitize headers by removing hop-by-hop and internal headers.
///
/// # Source
/// Ported from `packages/opencode/src/server/proxy-util.ts` `sanitize()`.
fn sanitize_headers(headers: &mut HeaderMap) {
    for key in HOP_BY_HOP {
        headers.remove(*key);
    }
    headers.remove("accept-encoding");
    headers.remove("x-opencode-directory");
    headers.remove("x-opencode-workspace");
}

/// Merge extra headers into a header map after sanitization.
///
/// # Source
/// Ported from `packages/opencode/src/server/proxy-util.ts` `headers()`.
fn merge_headers(base: &HeaderMap, extra: Option<&HeaderMap>) -> HeaderMap {
    let mut out = base.clone();
    sanitize_headers(&mut out);
    if let Some(extra) = extra {
        for (key, value) in extra.iter() {
            out.insert(key, value.clone());
        }
    }
    out
}

/// Extract WebSocket protocols from request headers.
///
/// # Source
/// Ported from `packages/opencode/src/server/proxy-util.ts` `websocketProtocols()`.
pub fn websocket_protocols(headers: &HeaderMap) -> Vec<String> {
    headers
        .get("sec-websocket-protocol")
        .and_then(|v| v.to_str().ok())
        .map(|v| {
            v.split(',')
                .map(|s| s.trim().to_string())
                .filter(|s| !s.is_empty())
                .collect()
        })
        .unwrap_or_default()
}

/// Convert an HTTP URL to a WebSocket URL.
///
/// # Source
/// Ported from `packages/opencode/src/server/proxy-util.ts` `websocketTargetURL()`.
pub fn websocket_target_url(target: &str) -> String {
    if let Some(rest) = target.strip_prefix("http://") {
        format!("ws://{rest}")
    } else if let Some(rest) = target.strip_prefix("https://") {
        format!("wss://{rest}")
    } else {
        target.to_string()
    }
}

/// Proxy an HTTP request to a remote target using pre-collected body bytes.
///
/// Forwards method, headers (sanitized), query, and body to the target URL,
/// then returns the proxied response.
///
/// # Source
/// Ported from `packages/opencode/src/server/routes/instance/httpapi/middleware/proxy.ts` `http()`.
pub async fn proxy_http_request(
    method: Method,
    uri: &Uri,
    headers: HeaderMap,
    body_bytes: axum::body::Bytes,
    target_url: &str,
    extra_headers: Option<&HeaderMap>,
) -> Response {
    let client = match reqwest::Client::builder().no_proxy().build() {
        Ok(c) => c,
        Err(e) => {
            return ServerError::upstream(format!("proxy client build error: {e}"))
                .into_response();
        }
    };

    let proxy_headers = merge_headers(&headers, extra_headers);

    let mut proxy_req = client
        .request(method, target_url)
        .headers(proxy_headers)
        .body(body_bytes);

    // Append query parameters
    if let Some(query) = uri.query() {
        proxy_req = proxy_req.query(&query);
    }

    match proxy_req.send().await {
        Ok(resp) => {
            let status = resp.status();
            let resp_headers = resp.headers().clone();
            let resp_body = match resp.bytes().await {
                Ok(b) => b,
                Err(_) => {
                    return ServerError::upstream("failed to read proxy response")
                        .into_response();
                }
            };

            let mut response = Response::new(Body::from(resp_body));
            *response.status_mut() = status;

            for (key, value) in resp_headers.iter() {
                let key_lower = key.as_str().to_lowercase();
                if key_lower == "content-encoding" || key_lower == "content-length" {
                    continue;
                }
                response.headers_mut().insert(key.clone(), value.clone());
            }
            response
        }
        Err(e) => ServerError::upstream(format!("proxy error: {e}")).into_response(),
    }
}

/// Proxy a WebSocket connection.
///
/// Upgrades the inbound WebSocket, connects to the outbound target,
/// and bidirectionally bridges messages.
///
/// # Source
/// Ported from `packages/opencode/src/server/routes/instance/httpapi/middleware/proxy.ts` `websocket()`.
pub async fn proxy_websocket(
    ws: WebSocketUpgrade,
    target_url: &str,
) -> Response {
    let target = target_url.to_string();
    ws.on_upgrade(move |inbound_socket: WebSocket| async move {
        let ws_target = websocket_target_url(&target);
        match tokio_tungstenite::connect_async(&ws_target).await {
            Ok((outbound_socket, _)) => {
                let (mut inbound_tx, mut inbound_rx) = inbound_socket.split();
                let (mut outbound_tx, mut outbound_rx) = outbound_socket.split();

                // Forward outbound → inbound
                let task1 = tokio::spawn(async move {
                    while let Some(msg) = outbound_rx.next().await {
                        if let Ok(msg) = msg {
                            let ws_msg = match msg {
                                tokio_tungstenite::tungstenite::Message::Text(t) => {
                                    Message::Text(t.into())
                                }
                                tokio_tungstenite::tungstenite::Message::Binary(b) => {
                                    Message::Binary(b.into())
                                }
                                tokio_tungstenite::tungstenite::Message::Close(_) => {
                                    Message::Close(None)
                                }
                                _ => continue,
                            };
                            let _ = inbound_tx.send(ws_msg).await;
                        }
                    }
                });

                // Forward inbound → outbound
                let task2 = tokio::spawn(async move {
                    while let Some(msg) = inbound_rx.next().await {
                        if let Ok(msg) = msg {
                            let data = match msg {
                                Message::Text(t) => {
                                    tokio_tungstenite::tungstenite::Message::Text(t.to_string())
                                }
                                Message::Binary(b) => {
                                    tokio_tungstenite::tungstenite::Message::Binary(b.to_vec())
                                }
                                Message::Close(_) => {
                                    tokio_tungstenite::tungstenite::Message::Close(None)
                                }
                                _ => continue,
                            };
                            let _ = outbound_tx.send(data).await;
                        }
                    }
                });

                let _ = tokio::join!(task1, task2);
            }
            Err(e) => {
                tracing::error!("WebSocket proxy connection failed: {e}");
            }
        }
    })
}

/// Check a request for a [`RemoteWorkspaceTarget`] extension and return the proxy URL.
///
/// Used by handlers to determine if a request should be proxied.
pub fn get_proxy_target(req: &Request) -> Option<&RemoteWorkspaceTarget> {
    req.extensions().get::<RemoteWorkspaceTarget>()
}
