//! Workspace routing middleware — resolves `x-opencode-directory` to a workspace,
//! then routes locally or signals proxy forwarding.
//!
//! Ported from: `packages/opencode/src/server/routes/instance/httpapi/middleware/workspace-routing.ts`

use axum::extract::{Request, State};
use axum::middleware::Next;
use axum::response::Response;
use std::sync::Arc;

use crate::server::AppState;

/// Context injected into request extensions by the workspace routing middleware.
///
/// Downstream handlers and middleware read this to determine the workspace
/// directory and optional workspace ID for local processing.
#[derive(Debug, Clone)]
pub struct WorkspaceRouteContext {
    /// Resolved directory for the current request.
    pub directory: String,
    /// Optional workspace ID if a workspace was matched.
    pub workspace_id: Option<String>,
}

/// Injected into extensions when the workspace is remote and should be proxied.
#[derive(Debug, Clone)]
pub struct RemoteWorkspaceTarget {
    /// The proxy URL (remote workspace target).
    pub url: String,
}

// ── Routing rules matching the TS `RULES` array ──────────────────────────

const ROUTING_RULES: &[(Option<&str>, &str, RoutingAction)] = &[
    (None, "/experimental/workspace", RoutingAction::Local),
    (None, "/session/status", RoutingAction::Forward),
    (Some("GET"), "/session", RoutingAction::Local),
];

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum RoutingAction {
    Local,
    Forward,
}

/// Returns true when the request method+path should bypass workspace routing
/// and stay on the control plane (local processing).
///
/// # Source
/// Ported from `packages/opencode/src/server/shared/workspace-routing.ts` `isLocalWorkspaceRoute`.
pub fn is_local_workspace_route(method: &str, path: &str) -> bool {
    for (method_match, rule_path, action) in ROUTING_RULES {
        if let Some(m) = method_match {
            if *m != method {
                continue;
            }
        }
        let matched = path == *rule_path || path.starts_with(&format!("{}/", rule_path));
        if matched {
            return *action == RoutingAction::Local;
        }
    }
    false
}

/// Extract a session ID from workspace-routed URL paths.
///
/// # Source
/// Ported from `packages/opencode/src/server/shared/workspace-routing.ts` `getWorkspaceRouteSessionID`.
pub fn get_workspace_route_session_id(path: &str) -> Option<String> {
    if path == "/session/status" {
        return None;
    }
    if let Some(id) = path
        .strip_prefix("/session/")
        .and_then(|s| s.split('/').next())
    {
        if !id.is_empty() {
            return Some(id.to_string());
        }
    }
    if let Some(id) = path
        .strip_prefix("/experimental/session/")
        .and_then(|s| s.split('/').next())
    {
        if !id.is_empty() {
            return Some(id.to_string());
        }
    }
    None
}

/// Build a proxy URL for a remote workspace, stripping the `workspace` param.
///
/// # Source
/// Ported from `packages/opencode/src/server/shared/workspace-routing.ts` `workspaceProxyURL`.
pub fn workspace_proxy_url(target: &str, request_path: &str, request_query: &str) -> String {
    let base = target.trim_end_matches('/');
    let mut url = format!("{}{}", base, request_path);
    if !request_query.is_empty() {
        let params: Vec<&str> = request_query.split('&').collect();
        let filtered: Vec<&str> = params
            .into_iter()
            .filter(|p| !p.starts_with("workspace="))
            .collect();
        if !filtered.is_empty() {
            url.push('?');
            url.push_str(&filtered.join("&"));
        }
    }
    url
}

/// Simple URL-decode for query param values.
fn urldecode(input: &str) -> String {
    let mut result = String::with_capacity(input.len());
    let mut chars = input.chars();
    while let Some(c) = chars.next() {
        if c == '%' {
            let hex: String = chars.by_ref().take(2).collect();
            if let Ok(byte) = u8::from_str_radix(&hex, 16) {
                result.push(byte as char);
            }
        } else if c == '+' {
            result.push(' ');
        } else {
            result.push(c);
        }
    }
    result
}

/// Workspace routing middleware for axum.
///
/// Extracts `x-opencode-directory` (header or query param), looks up the
/// workspace from the database, and injects a [`WorkspaceRouteContext`]
/// extension. For remote workspaces, also injects [`RemoteWorkspaceTarget`].
///
/// # Source
/// Ported from `packages/opencode/src/server/routes/instance/httpapi/middleware/workspace-routing.ts`
pub async fn workspace_routing_middleware(
    State(state): State<Arc<AppState>>,
    req: Request,
    next: Next,
) -> Response {
    let method = req.method().to_string();
    let path = req.uri().path().to_string();

    // Extract directory from header or query param
    let directory = req
        .headers()
        .get("x-opencode-directory")
        .and_then(|v| v.to_str().ok())
        .map(|s| s.to_string())
        .or_else(|| {
            req.uri().query().and_then(|q| {
                for pair in q.split('&') {
                    if let Some((key, value)) = pair.split_once('=') {
                        if key == "directory" && !value.is_empty() {
                            return Some(urldecode(value));
                        }
                    }
                }
                None
            })
        })
        .unwrap_or_else(|| {
            std::env::current_dir()
                .ok()
                .map(|p| p.to_string_lossy().to_string())
                .unwrap_or_default()
        });

    // Check if this route should stay on control plane
    if is_local_workspace_route(&method, &path) || path.starts_with("/console") {
        let mut req = req;
        req.extensions_mut().insert(WorkspaceRouteContext {
            directory,
            workspace_id: None,
        });
        return next.run(req).await;
    }

    // Look up workspace by directory from the DB
    let workspace = state
        .sessions
        .db()
        .get_workspace_by_directory(&directory)
        .await
        .ok()
        .and_then(|rows| rows.into_iter().next());

    let workspace_id = workspace.as_ref().map(|w| w.id.clone());

    // If the workspace `extra` field contains a `{"url":"..."}` it's remote
    if let Some(ws) = &workspace {
        if let Some(extra) = &ws.extra {
            if let Ok(val) = serde_json::from_str::<serde_json::Value>(extra) {
                if let Some(url) = val.get("url").and_then(|v| v.as_str()) {
                    let query = req.uri().query().unwrap_or("");
                    let proxy_url = workspace_proxy_url(url, &path, query);
                    let mut req = req;
                    req.extensions_mut().insert(WorkspaceRouteContext {
                        directory,
                        workspace_id,
                    });
                    req.extensions_mut()
                        .insert(RemoteWorkspaceTarget { url: proxy_url });
                    return next.run(req).await;
                }
            }
        }
    }

    // Local workspace — just inject context
    let mut req = req;
    req.extensions_mut().insert(WorkspaceRouteContext {
        directory,
        workspace_id,
    });
    next.run(req).await
}
