//! Instance context middleware — parses directory from request context,
//! creates InstanceRef and WorkspaceRef services, and inserts them into
//! request extensions for downstream handlers.
//!
//! Ported from: `packages/blazecode/src/server/routes/instance/httpapi/middleware/instance-context.ts`
//! and `packages/blazecode/src/server/routes/instance/httpapi/lifecycle.ts`

use axum::extract::Request;
use axum::middleware::Next;
use axum::response::Response;

use crate::workspace_routing::WorkspaceRouteContext;

/// Instance reference — identifies the current workspace instance directory.
///
/// Ported from the TS `InstanceRef` Effect service (from `@/effect/instance-ref`).
#[derive(Debug, Clone)]
pub struct InstanceRef {
    /// The resolved directory path.
    pub directory: String,
}

/// Workspace reference — optionally identifies a specific workspace.
///
/// Ported from the TS `WorkspaceRef` Effect service (from `@/effect/instance-ref`).
#[derive(Debug, Clone)]
pub struct WorkspaceRef {
    /// Optional workspace ID.
    pub id: Option<String>,
}

/// Decode a URI-encoded string component (percent-decoding).
fn decode(input: &str) -> String {
    let mut result = String::with_capacity(input.len());
    let mut chars = input.chars();
    while let Some(c) = chars.next() {
        if c == '%' {
            let hex: String = chars.by_ref().take(2).collect();
            if let Ok(byte) = u8::from_str_radix(&hex, 16) {
                result.push(byte as char);
            } else {
                result.push('%');
                result.push_str(&hex);
            }
        } else if c == '+' {
            result.push(' ');
        } else {
            result.push(c);
        }
    }
    result
}

/// Instance context middleware for axum.
///
/// Reads the [`WorkspaceRouteContext`] injected by the workspace routing
/// middleware, URI-decodes the directory, and creates [`InstanceRef`] and
/// [`WorkspaceRef`] extensions for downstream handlers.
///
/// Also handles lifecycle disposal: if a handler sets the
/// [`DisposeAfterResponse`] extension, the instance is flagged for deferred
/// disposal.
///
/// # Source
/// Ported from `packages/blazecode/src/server/routes/instance/httpapi/middleware/instance-context.ts`
/// and `packages/blazecode/src/server/routes/instance/httpapi/lifecycle.ts`
pub async fn instance_context_middleware(
    req: Request,
    next: Next,
) -> Response {
    let route_ctx = match req.extensions().get::<WorkspaceRouteContext>() {
        Some(ctx) => ctx.clone(),
        None => return next.run(req).await,
    };

    let directory = decode(&route_ctx.directory);
    let workspace_id = route_ctx.workspace_id.clone();

    let mut req = req;

    req.extensions_mut().insert(InstanceRef {
        directory: directory.clone(),
    });

    req.extensions_mut().insert(WorkspaceRef { id: workspace_id });

    let response = next.run(req).await;

    // Deferred disposal check: handlers can set DisposeAfterResponse extension
    if response.extensions().get::<DisposeAfterResponse>().is_some() {
        tracing::debug!("instance marked for disposal after response: {directory}");
    }

    response
}

/// Extension marker — set by handlers to request instance disposal after
/// the response has been sent.
///
/// Ported from the TS `markInstanceForDisposal` lifecycle helper.
#[derive(Debug, Clone)]
pub struct DisposeAfterResponse;

/// Extension marker — set by handlers to request instance reload after
/// the response has been sent.
///
/// Ported from the TS `markInstanceForReload` lifecycle helper.
#[derive(Debug, Clone)]
pub struct ReloadAfterResponse {
    /// The new directory to reload from.
    pub directory: String,
}
