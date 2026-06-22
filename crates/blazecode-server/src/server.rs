//! HTTP server setup, router construction, and graceful shutdown.
//!
//! Ported from: `packages/blazecode/src/server/server.ts`

use axum::Router;
use std::net::SocketAddr;
use std::sync::Arc;
use std::time::Instant;
use tokio::net::TcpListener;
use tokio::signal;
use tower_http::compression::CompressionLayer;
use tracing::info;

use crate::cors::cors_layer;
use crate::routes;

/// Application state shared across all request handlers.
///
/// # Source
/// Ported from `packages/blazecode/src/server/routes/instance/httpapi/server.ts`
/// (the `context` and `Layer.buildLayer(app)` pattern).
pub struct AppState {
    /// Database service for SQLite connectivity.
    pub db: Arc<blazecode_core::database::DatabaseService>,

    /// The global event bus for broadcasting events to SSE subscribers.
    pub bus: blazecode_core::bus::SharedBus,

    /// Session manager for CRUD and message operations.
    pub sessions: Arc<blazecode_core::session::SessionManager>,

    /// Tool registry for tool discovery and execution.
    pub tools: Arc<blazecode_core::tool::ToolRegistry>,

    /// Permission service for evaluating and managing permissions.
    pub permissions: Arc<blazecode_core::permission::PermissionService>,

    /// Question service for managing pending user Q&A.
    pub questions: Arc<blazecode_core::question::QuestionService>,

    /// Session runner for executing prompts against LLMs.
    pub runner: Arc<blazecode_core::session_runner::SessionRunner>,

    /// Registered LLM providers (provider_id → provider).
    pub providers: std::collections::HashMap<String, Arc<dyn blazecode_core::provider::Provider>>,

    /// Server version string.
    pub version: String,

    /// Server start time for uptime calculation.
    pub start_time: Instant,

    /// Agent service for listing/managing agents (optional — may be
    /// unset if agent config has not been loaded).
    pub agent_service: Option<Arc<blazecode_core::agent::AgentService>>,

    /// Command definitions loaded from project and global config.
    pub command_data: Arc<blazecode_core::command::CommandData>,

    /// Integration service for third-party OAuth/API-key connections.
    pub integration_service: Arc<blazecode_core::integration::IntegrationService>,

    /// Reference service for code references and context items.
    pub reference_service: Arc<blazecode_core::reference::ReferenceService>,

    /// Feature flags exposed via the metadata endpoint.
    pub server_features: Vec<String>,
}

impl AppState {
    /// Create a new `AppState` with the given components.
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        db: Arc<blazecode_core::database::DatabaseService>,
        bus: blazecode_core::bus::SharedBus,
        sessions: Arc<blazecode_core::session::SessionManager>,
        tools: Arc<blazecode_core::tool::ToolRegistry>,
        permissions: Arc<blazecode_core::permission::PermissionService>,
        questions: Arc<blazecode_core::question::QuestionService>,
        runner: Arc<blazecode_core::session_runner::SessionRunner>,
        providers: std::collections::HashMap<String, Arc<dyn blazecode_core::provider::Provider>>,
        agent_service: Option<Arc<blazecode_core::agent::AgentService>>,
        command_data: Arc<blazecode_core::command::CommandData>,
        integration_service: Arc<blazecode_core::integration::IntegrationService>,
        reference_service: Arc<blazecode_core::reference::ReferenceService>,
        server_features: Vec<String>,
    ) -> Self {
        Self {
            db,
            bus,
            sessions,
            tools,
            permissions,
            questions,
            runner,
            providers,
            version: env!("CARGO_PKG_VERSION").to_string(),
            start_time: Instant::now(),
            agent_service,
            command_data,
            integration_service,
            reference_service,
            server_features,
        }
    }
}

/// Server configuration.
///
/// # Source
/// Ported from `packages/blazecode/src/server/server.ts` `ListenOptions`.
#[derive(Debug, Clone)]
pub struct ServerConfig {
    /// Hostname to bind to (e.g. "127.0.0.1", "0.0.0.0").
    pub hostname: String,
    /// Port to listen on.
    pub port: u16,
    /// Allowed CORS origins. `None` means allow all origins.
    pub cors_origins: Option<Vec<String>>,
}

impl Default for ServerConfig {
    fn default() -> Self {
        Self {
            hostname: "127.0.0.1".to_string(),
            port: 4096,
            cors_origins: None,
        }
    }
}

/// Build the complete axum router with all routes and middleware layers.
///
/// Layer order (innermost → outermost, applied via `.layer()` bottom-to-top):
///   Compression → Fence → Instance context → Workspace routing →
///   Schema error → Auth → CORS
///
/// Incoming request execution order:
///   CORS → Auth → Schema error → Workspace routing →
///   Instance context → Fence → Compression → handler
///
/// # Source
/// Ported from `packages/blazecode/src/server/routes/instance/httpapi/server.ts`
/// `createRoutes()` function (lines 261–285) which merges all route layers and middleware.
pub fn build_router(state: Arc<AppState>, config: &ServerConfig) -> Router {
    // Build CORS layer from config — no longer hardcoded empty
    let cors = match &config.cors_origins {
        Some(origins) => cors_layer(origins),
        None => cors_layer(&[]), // None means allow all
    };

    // Auth is handled by the auth middleware (which reads env vars at request time)

    Router::new()
        // ── Global routes (no auth — health, global/health) ────────────
        .merge(routes::global::global_routes(state.clone()))
        .merge(routes::health::health_routes(state.clone()))
        // ── Control routes ──────────────────────────────────────────────
        .merge(routes::control::control_routes(state.clone()))
        // ── Control-plane routes ────────────────────────────────────────
        .merge(routes::control_plane::control_plane_routes(state.clone()))
        // ── Instance routes (workspace-scoped) ──────────────────────────
        .merge(routes::agent::agent_routes(state.clone()))
        .merge(routes::command::command_routes(state.clone()))
        .merge(routes::config::config_routes(state.clone()))
        .merge(routes::credential::credential_routes(state.clone()))
        .merge(routes::experimental::experimental_routes(state.clone()))
        .merge(routes::file::file_routes(state.clone()))
        .merge(routes::instance::instance_routes(state.clone()))
        .merge(routes::integration::integration_routes(state.clone()))
        .merge(routes::mcp::mcp_routes(state.clone()))
        .merge(routes::model::model_routes(state.clone()))
        .merge(routes::permission::permission_routes(state.clone()))
        .merge(routes::project::project_routes(state.clone()))
        .merge(routes::project_copy::project_copy_routes(state.clone()))
        .merge(routes::provider::provider_routes(state.clone()))
        .merge(routes::pty::pty_routes(state.clone()))
        .merge(routes::question::question_routes(state.clone()))
        .merge(routes::reference::reference_routes(state.clone()))
        .merge(routes::session::session_routes(state.clone()))
        .merge(routes::skill::skill_routes(state.clone()))
        .merge(routes::sync::sync_routes(state.clone()))
        .merge(routes::tui::tui_routes(state.clone()))
        .merge(routes::workspace::workspace_routes(state.clone()))
        // ── Event stream (SSE) ──────────────────────────────────────────
        .merge(routes::event::event_routes(state.clone()))
        // ── Metadata ─────────────────────────────────────────────────────
        .merge(routes::metadata::metadata_routes(state.clone()))
        // ── Structured query ─────────────────────────────────────────────
        .merge(routes::query::query_routes(state.clone()))
        // ── V2 API routes (under /api/) ──────────────────────────────────
        .merge(routes::api::api_routes(state.clone()))
        // ── Middleware stack (axum layers stack inside-out) ────────────
        //
        // Execution order: CORS → Auth → Schema error →
        //   Workspace routing → Instance context → Fence → Compression → handler
        //
        // `.layer()` builds bottom-to-top: first call = innermost (wraps handler),
        // last call = outermost (applied first to incoming request).

        // Innermost: Compression (wraps response body before sending)
        .layer(CompressionLayer::new())
        // Fence — captures event_sequence before/after handler
        .layer(axum::middleware::from_fn_with_state(
            state.clone(),
            crate::fence::fence_middleware,
        ))
        // Instance context — injects InstanceRef/WorkspaceRef
        .layer(axum::middleware::from_fn(
            crate::instance_context::instance_context_middleware,
        ))
        // Workspace routing — resolves directory → workspace context (needs state for DB)
        .layer(axum::middleware::from_fn_with_state(
            state.clone(),
            crate::workspace_routing::workspace_routing_middleware,
        ))
        // Schema error — catches JSON deserialization errors
        .layer(axum::middleware::from_fn(
            crate::schema_error::schema_error_middleware,
        ))
        // Auth — checks BLAZECODE_SERVER_PASSWORD
        .layer(axum::middleware::from_fn(crate::auth::auth_middleware))
        // Outermost: CORS
        .layer(cors)
}

/// Start the server and block until a shutdown signal is received.
///
/// # Source
/// Ported from `packages/blazecode/src/server/server.ts` `listen()` and
/// `listenEffect()` functions (lines 72–96).
///
/// # Errors
/// Returns an error if the server fails to bind to the address.
pub async fn serve(state: Arc<AppState>, config: ServerConfig) -> anyhow::Result<()> {
    let router = build_router(state, &config);
    let host: std::net::IpAddr = config
        .hostname
        .parse()
        .unwrap_or_else(|_| "127.0.0.1".parse().expect("hardcoded IP is valid"));
    let addr = SocketAddr::new(host, config.port);

    let listener = TcpListener::bind(addr).await?;
    let bound_addr = listener.local_addr()?;
    info!("blazecode-server listening on http://{bound_addr}");

    axum::serve(listener, router)
        .with_graceful_shutdown(shutdown_signal())
        .await?;

    info!("blazecode-server shut down gracefully");
    Ok(())
}

/// Wait for a shutdown signal (Ctrl+C or SIGTERM).
///
/// # Source
/// Ported from `packages/blazecode/src/server/server.ts` `makeStop()` (lines 171–192).
async fn shutdown_signal() {
    let ctrl_c = async {
        signal::ctrl_c()
            .await
            .expect("failed to install Ctrl+C handler");
    };

    #[cfg(unix)]
    let terminate = async {
        signal::unix::signal(signal::unix::SignalKind::terminate())
            .expect("failed to install signal handler")
            .recv()
            .await;
    };

    #[cfg(not(unix))]
    let terminate = std::future::pending::<()>();

    tokio::select! {
        () = ctrl_c => {
            info!("received Ctrl+C, starting graceful shutdown");
        }
        () = terminate => {
            info!("received SIGTERM, starting graceful shutdown");
        }
    }
}
