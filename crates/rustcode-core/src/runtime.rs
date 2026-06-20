//! Shared runtime initialisation for rustcode.
//!
//! Ported from: packages/opencode/src/index.ts — the Effect-TS Layer setup
//! and packages/opencode/src/cli/cmd/tui.ts — bootstrap() function.
//!
//! Centralises the boilerplate of building backend services (bus, database,
//! sessions, tools, permissions, questions, runner, providers) so that every
//! entry-point (cmd_tui, cmd_serve, cmd_run) shares the same wiring.
//!
//! ## Database persistence
//!
//! By default the runtime uses a file-backed SQLite database at
//! `$XDG_DATA_HOME/opencode/opencode.db` (or the platform equivalent).
//! Call `initialize_runtime()` for the default path, or
//! `initialize_runtime_with_path()` for a custom path.
//! Pass `:memory:` to get the old in-memory behaviour.

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::Arc;

use crate::bus;
use crate::config::Config;
use crate::database::DatabaseService;
use crate::permission::PermissionService;
use crate::provider::Provider;
use crate::question::QuestionService;
use crate::session::SessionManager;
use crate::session_runner::SessionRunner;
use crate::tool::ToolRegistry;

/// Fully-initialised runtime with all backend services wired together.
///
/// Constructed once at startup and shared across the application.
/// Cheap to clone (all fields are `Arc`-wrapped).
#[derive(Clone)]
pub struct RuntimeContext {
    /// Global event bus for pub/sub between services and TUI.
    pub bus: bus::SharedBus,

    /// SQLite database for session / permission / credential persistence.
    pub db: Arc<DatabaseService>,

    /// Path to the database file (empty for `:memory:`).
    pub db_path: PathBuf,

    /// Session lifecycle manager.
    pub sessions: Arc<SessionManager>,

    /// Built-in + plugin tool registry.
    pub tools: Arc<ToolRegistry>,

    /// Permission request/response service.
    pub permissions: Arc<PermissionService>,

    /// Question (ask-user) request/response service.
    pub questions: Arc<QuestionService>,

    /// Agentic-loop session runner.
    pub runner: Arc<SessionRunner>,

    /// Auto-detected LLM providers keyed by provider ID.
    pub providers: HashMap<String, Arc<dyn Provider>>,

    /// Provider catalog from the initialization pipeline.
    pub provider_catalog: Arc<crate::provider_service::ProviderCatalog>,
}

// ── Default database path ─────────────────────────────────────────────────────

/// Return the default database path: `$XDG_DATA_HOME/opencode/opencode.db`.
///
/// Falls back to `~/.local/share/opencode/opencode.db` on Linux,
/// `~/Library/Application Support/opencode/opencode.db` on macOS,
/// `%APPDATA%/opencode/opencode.db` on Windows.
pub fn default_db_path() -> PathBuf {
    dirs::data_dir()
        .unwrap_or_else(|| PathBuf::from("."))
        .join("opencode")
        .join("opencode.db")
}

// ── Initialisation ────────────────────────────────────────────────────────────

/// Initialise all backend services with the default file-backed database.
///
/// Creates the parent directory if it does not exist.  If provider detection
/// finds no API keys the map will be empty — callers should print a helpful
/// message listing the expected env vars.
pub fn initialize_runtime(config: &Config) -> anyhow::Result<RuntimeContext> {
    initialize_runtime_with_path(&default_db_path(), config)
}

/// Initialise the runtime with a custom database path.
///
/// Pass `Path::new(":memory:")` for an in-memory database (useful for tests
/// or one-shot runs that don't need persistence).
pub fn initialize_runtime_with_path(
    db_path: &Path,
    config: &Config,
) -> anyhow::Result<RuntimeContext> {
    let is_memory = db_path == Path::new(":memory:");

    // Ensure parent directory exists for file-backed databases.
    if !is_memory {
        if let Some(parent) = db_path.parent() {
            if !parent.as_os_str().is_empty() {
                std::fs::create_dir_all(parent)?;
            }
        }
    }

    let db_url = if is_memory {
        "sqlite::memory:".to_string()
    } else {
        format!("sqlite:{}", db_path.display())
    };

    let bus = bus::SharedBus::new(256);

    let db_pool = sqlx::sqlite::SqlitePoolOptions::new()
        .max_connections(5)
        .connect_lazy(&db_url);

    let db = Arc::new(DatabaseService::new(db_pool?));
    let sessions = Arc::new(SessionManager::new(bus.clone(), db.clone()));
    let tools = Arc::new(ToolRegistry::new());
    tools.register_builtins();
    let permissions = Arc::new(PermissionService::new(bus.clone()));
    let questions = Arc::new(QuestionService::default());
    let runner = Arc::new(SessionRunner::new(tools.clone()));

    // Use the provider initialization pipeline (plugin-aware).
    let provider_catalog = tokio::runtime::Handle::current()
        .block_on(crate::provider_service::init_providers(config))
        .map_err(|e| anyhow::anyhow!("Provider initialization failed: {e}"))?;

    // Convert to the Arc-based map for backward compatibility.
    let providers: HashMap<String, Arc<dyn Provider>> = provider_catalog
        .providers
        .into_iter()
        .map(|(id, p)| (id, Arc::from(p)))
        .collect();

    // Log detected providers at info level so they show up in `--print-logs`.
    if providers.is_empty() {
        tracing::info!("No LLM providers detected from environment variables.");
    } else {
        tracing::info!(
            count = providers.len(),
            "LLM providers detected via environment variables"
        );
        for id in providers.keys() {
            tracing::info!(provider = %id, "  provider ready");
        }
    }

    if !is_memory {
        tracing::info!(
            db = %db_path.display(),
            "database opened"
        );
    } else {
        tracing::info!("database opened (in-memory)");
    }

    // Re-create catalog with empty providers map (they're now in the providers HashMap).
    let provider_catalog = crate::provider_service::ProviderCatalog {
        providers: HashMap::new(),
        model_overrides: provider_catalog.model_overrides,
        disabled: provider_catalog.disabled,
        enabled: provider_catalog.enabled,
    };

    Ok(RuntimeContext {
        bus,
        db,
        db_path: db_path.to_path_buf(),
        sessions,
        tools,
        permissions,
        questions,
        runner,
        providers,
        provider_catalog: Arc::new(provider_catalog),
    })
}

// ── Helpers ───────────────────────────────────────────────────────────────────

impl RuntimeContext {
    /// True when at least one provider was detected and is ready for use.
    pub fn has_providers(&self) -> bool {
        !self.providers.is_empty()
    }

    /// Get the default provider ID (first alphabetically, well-known first).
    pub fn default_provider_id(&self) -> Option<&str> {
        let mut ids: Vec<&str> = self.providers.keys().map(|s| s.as_str()).collect();
        ids.sort_by_key(|id| match *id {
            "anthropic" => 0u8,
            "openai" => 1,
            "google" => 2,
            "openrouter" => 3,
            _ => 99,
        });
        ids.first().copied()
    }

    /// Get the default model for a given provider ID.
    pub fn default_model_for(&self, provider_id: &str) -> Option<&'static str> {
        match provider_id {
            "anthropic" => Some("claude-sonnet-4-20250514"),
            "openai" => Some("gpt-5.2"),
            "google" => Some("gemini-3.0-flash"),
            "openrouter" => Some("anthropic/claude-sonnet-4-20250514"),
            "deepseek" => Some("deepseek-chat"),
            "groq" => Some("llama-4-maverick"),
            _ => None,
        }
    }

    /// Print a helpful message listing the expected environment variables
    /// for provider detection.  Call this when `has_providers()` is false.
    pub fn print_provider_env_help() {
        eprintln!("No LLM providers detected. Set an API key environment variable:");
        eprintln!("  ANTHROPIC_API_KEY              — Claude (Anthropic)");
        eprintln!("  OPENAI_API_KEY                 — GPT (OpenAI)");
        eprintln!("  GOOGLE_GENERATIVE_AI_API_KEY   — Gemini (Google)");
        eprintln!("  OPENROUTER_API_KEY             — OpenRouter (multi-provider)");
        eprintln!("  DEEPSEEK_API_KEY               — DeepSeek");
        eprintln!("  GROQ_API_KEY                   — Groq");
        eprintln!("  XAI_API_KEY                    — xAI / Grok");
        eprintln!("  MISTRAL_API_KEY                — Mistral");
        eprintln!("  TOGETHER_API_KEY               — Together AI");
    }

    /// True when the database is file-backed (persistent), false for `:memory:`.
    pub fn is_persistent(&self) -> bool {
        self.db_path != PathBuf::from(":memory:")
    }
}

// ── Tests ─────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_default_db_path_is_absolute() {
        let p = default_db_path();
        assert!(p.is_absolute() || p.starts_with("."));
        assert!(p.ends_with("opencode.db"));
    }

    #[test]
    fn test_memory_runtime() {
        let rt = tokio::runtime::Runtime::new().unwrap();
        let _guard = rt.enter();
        let config = Config::new(PathBuf::new(), None);
        let ctx = initialize_runtime_with_path(Path::new(":memory:"), &config).unwrap();
        assert!(!ctx.is_persistent());
        assert!(ctx.db_path == PathBuf::from(":memory:"));
    }

    #[test]
    fn test_default_provider_ordering() {
        let mut ids = vec!["openrouter", "anthropic", "google", "openai"];
        ids.sort_by_key(|id| match *id {
            "anthropic" => 0u8,
            "openai" => 1,
            "google" => 2,
            "openrouter" => 3,
            _ => 99,
        });
        assert_eq!(ids, vec!["anthropic", "openai", "google", "openrouter"]);
    }

    #[test]
    fn test_default_model_known() {
        let rt = tokio::runtime::Runtime::new().unwrap();
        let _guard = rt.enter();
        let config = Config::new(PathBuf::new(), None);
        let ctx = initialize_runtime_with_path(Path::new(":memory:"), &config).unwrap();
        assert_eq!(
            ctx.default_model_for("anthropic"),
            Some("claude-sonnet-4-20250514")
        );
        assert_eq!(ctx.default_model_for("unknown-provider"), None);
    }

    #[test]
    fn test_has_providers_empty() {
        let rt = tokio::runtime::Runtime::new().unwrap();
        let _guard = rt.enter();
        let config = Config::new(PathBuf::new(), None);
        let ctx = initialize_runtime_with_path(Path::new(":memory:"), &config).unwrap();
        let expected =
            std::env::var("ANTHROPIC_API_KEY").is_ok() || std::env::var("OPENAI_API_KEY").is_ok();
        assert_eq!(ctx.has_providers(), expected);
    }
}
