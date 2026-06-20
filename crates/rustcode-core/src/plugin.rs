//! Plugin system — discovery, loading, and hook management.
//!
//! # Source
//! Ported from:
//! - `packages/opencode/src/plugin/index.ts` — Plugin service layer, triggers, hook lifecycle
//! - `packages/opencode/src/plugin/loader.ts` — Plugin resolver and loader (`PluginLoader`)
//! - `packages/opencode/src/plugin/shared.ts` — Plugin spec parsing, entrypoint resolution
//! - `packages/opencode/src/plugin/meta.ts` — Plugin metadata store (touch, fingerprint, state)
//! - `packages/opencode/src/plugin/install.ts` — Plugin installation and config patching
//! - `packages/core/src/plugin/provider/` — Provider plugin definitions
//!
//! OpenCode commit: 5d0f86606ac30690f79f0a6a9f41a1f49fe95d0b

use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::Arc;

// ── Provider plugin system ────────────────────────────────────────────

/// Context passed to provider plugin hooks during catalog transformation.
///
/// Allows plugins to modify provider settings (headers, API keys, enabled state)
/// before the provider is initialized.
///
/// # Source
/// Ported from `packages/core/src/plugin.ts` HookSpec `catalog.transform`.
pub struct CatalogTransformContext<'a> {
    /// The provider ID being transformed.
    pub provider_id: &'a str,
    /// Mutable reference to the provider's request headers.
    pub headers: &'a mut HashMap<String, String>,
    /// Whether the provider is enabled.
    pub enabled: &'a mut bool,
    /// Provider-specific options (API key, base URL, etc.).
    pub options: &'a mut HashMap<String, serde_json::Value>,
}

/// Context for custom model discovery.
///
/// Plugins can return a custom model list that replaces or augments the
/// default catalog for a provider.
///
/// # Source
/// Ported from `packages/opencode/src/provider/provider.ts` model loaders.
pub struct ModelDiscoverContext<'a> {
    /// The provider ID.
    pub provider_id: &'a str,
    /// The provider's base URL.
    pub base_url: &'a str,
    /// The API key (if available).
    pub api_key: Option<&'a str>,
    /// Provider-specific options.
    pub options: &'a HashMap<String, serde_json::Value>,
}

/// Context for custom auth credential loading.
///
/// Plugins can provide custom auth flows (OAuth, token refresh, etc.)
/// that return provider options to be merged into the catalog.
///
/// # Source
/// Ported from `packages/plugin/src/index.ts` `AuthHook`.
pub struct AuthLoadContext<'a> {
    /// The provider ID.
    pub provider_id: &'a str,
    /// The provider's environment variable names.
    pub env_vars: &'a [String],
}

/// A plugin that customizes provider behavior.
///
/// Provider plugins can:
/// - Transform provider catalog settings (headers, auth, enabled state)
/// - Discover custom model lists
/// - Load custom auth credentials
///
/// # Source
/// Ported from `packages/core/src/plugin/provider/*.ts` (33 built-in plugins).
#[async_trait::async_trait]
pub trait ProviderPlugin: Send + Sync {
    /// Unique identifier for this plugin.
    fn id(&self) -> &str;

    /// Human-readable name.
    fn name(&self) -> &str;

    /// Hook: Transform provider catalog settings before initialization.
    ///
    /// Called once per provider during catalog setup. Use this to inject
    /// headers, modify options, or disable providers.
    async fn transform_catalog(&self, _ctx: &mut CatalogTransformContext<'_>) {}

    /// Hook: Discover models for a provider.
    ///
    /// Return `Some(models)` to replace the default catalog, or `None`
    /// to use the built-in model list.
    async fn discover_models(
        &self,
        _ctx: &ModelDiscoverContext<'_>,
    ) -> Option<Vec<crate::provider::Model>> {
        None
    }

    /// Hook: Load custom auth credentials.
    ///
    /// Return `Some(options)` to merge into the provider's options, or
    /// `None` to use default env-var-based auth.
    async fn load_auth(
        &self,
        _ctx: &AuthLoadContext<'_>,
    ) -> Option<HashMap<String, serde_json::Value>> {
        None
    }
}

/// Registry that stores and triggers provider plugins.
///
/// Plugins are registered at startup and triggered during provider
/// initialization to customize catalog, models, and auth.
///
/// # Source
/// Ported from `packages/core/src/plugin/provider.ts` `ProviderPlugins`.
pub struct ProviderPluginRegistry {
    plugins: Vec<Arc<dyn ProviderPlugin>>,
}

impl ProviderPluginRegistry {
    /// Create an empty registry.
    pub fn new() -> Self {
        Self {
            plugins: Vec::new(),
        }
    }

    /// Register a provider plugin.
    pub fn register(&mut self, plugin: Arc<dyn ProviderPlugin>) {
        self.plugins.push(plugin);
    }

    /// Register multiple plugins.
    pub fn register_all(&mut self, plugins: Vec<Arc<dyn ProviderPlugin>>) {
        self.plugins.extend(plugins);
    }

    /// Trigger `transform_catalog` on all registered plugins for a provider.
    pub async fn transform_catalog(&self, ctx: &mut CatalogTransformContext<'_>) {
        for plugin in &self.plugins {
            plugin.transform_catalog(ctx).await;
        }
    }

    /// Trigger `discover_models` on plugins until one returns a result.
    pub async fn discover_models(
        &self,
        ctx: &ModelDiscoverContext<'_>,
    ) -> Option<Vec<crate::provider::Model>> {
        for plugin in &self.plugins {
            if let Some(models) = plugin.discover_models(ctx).await {
                return Some(models);
            }
        }
        None
    }

    /// Trigger `load_auth` on plugins until one returns a result.
    pub async fn load_auth(
        &self,
        ctx: &AuthLoadContext<'_>,
    ) -> Option<HashMap<String, serde_json::Value>> {
        for plugin in &self.plugins {
            if let Some(options) = plugin.load_auth(ctx).await {
                return Some(options);
            }
        }
        None
    }

    /// Number of registered plugins.
    pub fn count(&self) -> usize {
        self.plugins.len()
    }

    /// Find a plugin by id.
    pub fn get(&self, id: &str) -> Option<Arc<dyn ProviderPlugin>> {
        self.plugins.iter().find(|p| p.id() == id).cloned()
    }
}

impl Default for ProviderPluginRegistry {
    fn default() -> Self {
        Self::new()
    }
}

/// A simple provider plugin created from closures.
///
/// Allows quick ad-hoc plugins without defining a full struct:
///
/// ```ignore
/// let plugin = ClosureProviderPlugin::new("my-plugin", "My Plugin")
///     .with_transform(|ctx| {
///         ctx.headers.insert("X-Custom".into(), "value".into());
///     });
/// registry.register(Arc::new(plugin));
/// ```
pub struct ClosureProviderPlugin {
    id: String,
    name: String,
    transform_fn:
        Option<Box<dyn Fn(&mut CatalogTransformContext<'_>) -> BoxFuture<()> + Send + Sync>>,
    discover_fn: Option<
        Box<
            dyn Fn(&ModelDiscoverContext<'_>) -> BoxFuture<Option<Vec<crate::provider::Model>>>
                + Send
                + Sync,
        >,
    >,
    auth_fn: Option<
        Box<
            dyn Fn(&AuthLoadContext<'_>) -> BoxFuture<Option<HashMap<String, serde_json::Value>>>
                + Send
                + Sync,
        >,
    ),
}

type BoxFuture<T> = std::pin::Pin<Box<dyn std::future::Future<Output = T> + Send>>;

impl ClosureProviderPlugin {
    /// Create a new closure-based plugin with just an id and name.
    pub fn new(id: impl Into<String>, name: impl Into<String>) -> Self {
        Self {
            id: id.into(),
            name: name.into(),
            transform_fn: None,
            discover_fn: None,
            auth_fn: None,
        }
    }

    /// Set the catalog transform hook.
    pub fn with_transform(
        mut self,
        f: impl Fn(&mut CatalogTransformContext<'_>) -> BoxFuture<()> + Send + Sync + 'static,
    ) -> Self {
        self.transform_fn = Some(Box::new(f));
        self
    }

    /// Set the model discover hook.
    pub fn with_discover(
        mut self,
        f: impl Fn(&ModelDiscoverContext<'_>) -> BoxFuture<Option<Vec<crate::provider::Model>>>
            + Send
            + Sync
            + 'static,
    ) -> Self {
        self.discover_fn = Some(Box::new(f));
        self
    }

    /// Set the auth loader hook.
    pub fn with_auth(
        mut self,
        f: impl Fn(&AuthLoadContext<'_>) -> BoxFuture<Option<HashMap<String, serde_json::Value>>>
            + Send
            + Sync
            + 'static,
    ) -> Self {
        self.auth_fn = Some(Box::new(f));
        self
    }
}

#[async_trait::async_trait]
impl ProviderPlugin for ClosureProviderPlugin {
    fn id(&self) -> &str {
        &self.id
    }
    fn name(&self) -> &str {
        &self.name
    }
    async fn transform_catalog(&self, ctx: &mut CatalogTransformContext<'_>) {
        if let Some(ref f) = self.transform_fn {
            f(ctx).await;
        }
    }
    async fn discover_models(
        &self,
        ctx: &ModelDiscoverContext<'_>,
    ) -> Option<Vec<crate::provider::Model>> {
        if let Some(ref f) = self.discover_fn {
            f(ctx).await
        } else {
            None
        }
    }
    async fn load_auth(
        &self,
        ctx: &AuthLoadContext<'_>,
    ) -> Option<HashMap<String, serde_json::Value>> {
        if let Some(ref f) = self.auth_fn {
            f(ctx).await
        } else {
            None
        }
    }
}

// ── Custom provider definition (from config) ──────────────────────────

/// A custom provider defined in `opencode.json` configuration.
///
/// Users can add custom providers via config without writing a plugin:
///
/// ```json
/// {
///   "provider": {
///     "my-provider": {
///       "name": "My Provider",
///       "env": ["MY_API_KEY"],
///       "models": { ... }
///     }
///   }
/// }
/// ```
///
/// # Source
/// Ported from `packages/opencode/src/provider/provider.ts` config providers.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CustomProviderConfig {
    /// Display name.
    pub name: String,
    /// Environment variable names for API key lookup.
    #[serde(default)]
    pub env: Vec<String>,
    /// Base URL for the provider's API.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub base_url: Option<String>,
    /// Models offered by this provider.
    #[serde(default)]
    pub models: HashMap<String, CustomModelConfig>,
    /// Extra HTTP headers to send with requests.
    #[serde(default)]
    pub headers: HashMap<String, String>,
    /// Whether this provider is enabled.
    #[serde(default = "default_true")]
    pub enabled: bool,
}

fn default_true() -> bool {
    true
}

/// Configuration for a single model within a custom provider.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CustomModelConfig {
    /// Display name.
    pub name: String,
    /// Context window size in tokens.
    #[serde(default = "default_context")]
    pub context: u64,
    /// Max output tokens.
    #[serde(default = "default_output")]
    pub output: u64,
    /// Whether this model supports reasoning.
    #[serde(default)]
    pub reasoning: bool,
    /// Whether this model accepts image input.
    #[serde(default)]
    pub image_input: bool,
    /// Model family (e.g. "claude", "gpt").
    #[serde(skip_serializing_if = "Option::is_none")]
    pub family: Option<String>,
}

fn default_context() -> u64 {
    128_000
}
fn default_output() -> u64 {
    16_384
}

impl CustomProviderConfig {
    /// Convert this config into a list of [`Model`] entries.
    pub fn build_models(
        &self,
        provider_id: &str,
        base_url: &str,
    ) -> Vec<crate::provider::Model> {
        self.models
            .iter()
            .map(|(id, m)| crate::provider::Model {
                id: id.into(),
                provider_id: provider_id.into(),
                name: m.name.clone(),
                api: crate::provider::ApiInfo {
                    id: id.into(),
                    url: base_url.into(),
                    npm: format!("@custom/{provider_id}"),
                },
                family: m.family.clone(),
                capabilities: crate::provider::Capabilities {
                    temperature: true,
                    reasoning: m.reasoning,
                    attachment: false,
                    toolcall: true,
                    input: crate::provider::Modality {
                        text: true,
                        image: m.image_input,
                        ..Default::default()
                    },
                    output: crate::provider::Modality {
                        text: true,
                        ..Default::default()
                    },
                    ..Default::default()
                },
                cost: crate::provider::Cost::default(),
                limit: crate::provider::TokenLimit {
                    context: m.context,
                    input: None,
                    output: m.output,
                },
                status: crate::provider::ModelStatus::Active,
                options: HashMap::new(),
                headers: self.headers.clone(),
                release_date: "2025".into(),
                variants: None,
            })
            .collect()
    }
}

// ── Plugin source ─────────────────────────────────────────────────────

/// Where a plugin is loaded from.
///
/// Ported from `packages/opencode/src/plugin/shared.ts` `PluginSource`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum PluginSource {
    /// Plugin loaded from a local file path.
    File,
    /// Plugin loaded from an npm package.
    Npm,
}

impl std::fmt::Display for PluginSource {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::File => write!(f, "file"),
            Self::Npm => write!(f, "npm"),
        }
    }
}

// ── Plugin kind ───────────────────────────────────────────────────────

/// What execution environment a plugin targets.
///
/// Ported from `packages/opencode/src/plugin/shared.ts` `PluginKind`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum PluginKind {
    /// Plugin runs in the server (headless) environment.
    Server,
    /// Plugin runs in the terminal UI environment.
    Tui,
}

impl std::fmt::Display for PluginKind {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Server => write!(f, "server"),
            Self::Tui => write!(f, "tui"),
        }
    }
}

// ── Plugin state ──────────────────────────────────────────────────────

/// Change state of a plugin between loads.
///
/// Ported from `packages/opencode/src/plugin/meta.ts` `State`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum PluginState {
    /// First time this plugin has been loaded.
    First,
    /// Plugin was loaded before but its fingerprint changed.
    Updated,
    /// Plugin fingerprint is unchanged since last load.
    Same,
}

impl std::fmt::Display for PluginState {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::First => write!(f, "first"),
            Self::Updated => write!(f, "updated"),
            Self::Same => write!(f, "same"),
        }
    }
}

// ── Plugin load error ──────────────────────────────────────────────────

/// Error type for plugin loading failures.
///
/// Ported from plugin loading error handling in the TypeScript source.
#[derive(Debug, thiserror::Error)]
pub enum PluginLoadError {
    #[error("plugin spec is empty")]
    EmptySpec,
    #[error("plugin `{spec}` is deprecated")]
    DeprecatedPlugin { spec: String },
    #[error("plugin `{spec}` not found")]
    NotFound { spec: String },
}

// ── Plugin hook names ─────────────────────────────────────────────────

/// Named hooks that plugins can register to intercept or transform.
///
/// Ported from `packages/opencode/src/plugin/index.ts` `TriggerName`.
/// The TypeScript source defines 15+ hook types; this enum covers all of them.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum PluginHook {
    // ── Lifecycle hooks ─────────────────────────────────────────────
    /// Cleanup hook called when plugin is unloaded.
    Dispose,

    // ── Event/config hooks ─────────────────────────────────────────
    /// Triggered for custom/named events from the event bus.
    Event,
    /// Config hook — plugins are notified when configuration changes.
    Config,

    // ── Tool hooks ─────────────────────────────────────────────────
    /// Register custom tools via plugin.
    Tool,
    /// Modify tool definitions before use.
    ToolDefinition,
    /// Before a tool executes — can intercept or modify.
    ToolExecuteBefore,
    /// After a tool executes — can inspect or transform result.
    ToolExecuteAfter,

    // ── Auth/provider hooks ────────────────────────────────────────
    /// Provider authentication hooks (OAuth, token refresh).
    Auth,
    /// Dynamic model discovery for providers.
    Provider,

    // ── Chat hooks ─────────────────────────────────────────────────
    /// Intercept and modify user messages before processing.
    ChatMessage,
    /// Modify LLM request parameters (temperature, max_tokens, etc.).
    ChatParams,
    /// Modify LLM request headers.
    ChatHeaders,

    // ── Permission hooks ───────────────────────────────────────────
    /// Intercept permission decisions.
    PermissionAsk,

    // ── Command hooks ──────────────────────────────────────────────
    /// Before a command executes.
    CommandExecuteBefore,

    // ── Shell hooks ────────────────────────────────────────────────
    /// Modify shell environment variables.
    ShellEnv,

    // ── Experimental hooks ─────────────────────────────────────────
    /// Triggered when text completion is requested.
    ExperimentalTextComplete,
    /// Triggered when a session is being compacted (context window management).
    ExperimentalSessionCompacting,
    /// Triggered when chat messages are being transformed before sending to the LLM.
    ExperimentalChatMessagesTransform,
    /// Modify the system prompt before sending to LLM.
    ExperimentalChatSystemTransform,
    /// Toggle auto-continue after compaction.
    ExperimentalCompactionAutocontinue,
    /// Pick a small model for lightweight tasks.
    ExperimentalProviderSmallModel,
}

impl PluginHook {
    /// Return the string name of this hook as used in plugin manifests.
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Dispose => "dispose",
            Self::Event => "event",
            Self::Config => "config",
            Self::Tool => "tool",
            Self::ToolDefinition => "tool.definition",
            Self::ToolExecuteBefore => "tool.execute.before",
            Self::ToolExecuteAfter => "tool.execute.after",
            Self::Auth => "auth",
            Self::Provider => "provider",
            Self::ChatMessage => "chat.message",
            Self::ChatParams => "chat.params",
            Self::ChatHeaders => "chat.headers",
            Self::PermissionAsk => "permission.ask",
            Self::CommandExecuteBefore => "command.execute.before",
            Self::ShellEnv => "shell.env",
            Self::ExperimentalTextComplete => "experimental.text.complete",
            Self::ExperimentalSessionCompacting => "experimental.session.compacting",
            Self::ExperimentalChatMessagesTransform => "experimental.chat.messages.transform",
            Self::ExperimentalChatSystemTransform => "experimental.chat.system.transform",
            Self::ExperimentalCompactionAutocontinue => "experimental.compaction.autocontinue",
            Self::ExperimentalProviderSmallModel => "experimental.provider.small_model",
        }
    }

    /// Parse a hook name string into a [`PluginHook`], returning `None` if unknown.
    pub fn from_str(s: &str) -> Option<Self> {
        match s {
            "dispose" => Some(Self::Dispose),
            "event" => Some(Self::Event),
            "config" => Some(Self::Config),
            "tool" => Some(Self::Tool),
            "tool.definition" => Some(Self::ToolDefinition),
            "tool.execute.before" => Some(Self::ToolExecuteBefore),
            "tool.execute.after" => Some(Self::ToolExecuteAfter),
            "auth" => Some(Self::Auth),
            "provider" => Some(Self::Provider),
            "chat.message" => Some(Self::ChatMessage),
            "chat.params" => Some(Self::ChatParams),
            "chat.headers" => Some(Self::ChatHeaders),
            "permission.ask" => Some(Self::PermissionAsk),
            "command.execute.before" => Some(Self::CommandExecuteBefore),
            "shell.env" => Some(Self::ShellEnv),
            "experimental.text.complete" => Some(Self::ExperimentalTextComplete),
            "experimental.session.compacting" => Some(Self::ExperimentalSessionCompacting),
            "experimental.chat.messages.transform" => Some(Self::ExperimentalChatMessagesTransform),
            "experimental.chat.system.transform" => Some(Self::ExperimentalChatSystemTransform),
            "experimental.compaction.autocontinue" => Some(Self::ExperimentalCompactionAutocontinue),
            "experimental.provider.small_model" => Some(Self::ExperimentalProviderSmallModel),
            _ => None,
        }
    }

    /// All known hook name strings.
    pub fn all_strs() -> &'static [&'static str] {
        &[
            "dispose",
            "event",
            "config",
            "tool",
            "tool.definition",
            "tool.execute.before",
            "tool.execute.after",
            "auth",
            "provider",
            "chat.message",
            "chat.params",
            "chat.headers",
            "permission.ask",
            "command.execute.before",
            "shell.env",
            "experimental.text.complete",
            "experimental.session.compacting",
            "experimental.chat.messages.transform",
            "experimental.chat.system.transform",
            "experimental.compaction.autocontinue",
            "experimental.provider.small_model",
        ]
    }
}

// Custom serde: serialize/deserialize using the dot-notation string names
impl Serialize for PluginHook {
    fn serialize<S: serde::Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        serializer.serialize_str(self.as_str())
    }
}

impl<'de> Deserialize<'de> for PluginHook {
    fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        let s = String::deserialize(deserializer)?;
        PluginHook::from_str(&s)
            .ok_or_else(|| serde::de::Error::unknown_variant(&s, PluginHook::all_strs()))
    }
}

// ── Plugin info ───────────────────────────────────────────────────────

/// Metadata for a loaded plugin.
///
/// Ported from `packages/opencode/src/plugin/index.ts` Plugin instance shape.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Plugin {
    /// Unique identifier for this plugin (from package.json name or explicit id export).
    pub id: String,
    /// Human-readable plugin name.
    pub name: String,
    /// Optional description.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
    /// Plugin version string (from package.json or explicit export).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub version: Option<String>,
    /// Where the plugin was loaded from.
    pub source: PluginSource,
    /// The plugin's specifier string (npm package name or file path).
    pub spec: String,
    /// Resolved target path on disk.
    pub target: Option<PathBuf>,
    /// Which hooks this plugin has registered.
    pub hooks: Vec<PluginHook>,
}

impl Plugin {
    /// Create a new plugin with the given id and source.
    pub fn new(id: impl Into<String>, name: impl Into<String>, source: PluginSource) -> Self {
        Self {
            id: id.into(),
            name: name.into(),
            description: None,
            version: None,
            source,
            spec: String::new(),
            target: None,
            hooks: Vec::new(),
        }
    }

    /// Set the plugin specifier.
    pub fn with_spec(mut self, spec: impl Into<String>) -> Self {
        self.spec = spec.into();
        self
    }

    /// Set the plugin target path.
    pub fn with_target(mut self, target: PathBuf) -> Self {
        self.target = Some(target);
        self
    }

    /// Set the plugin version.
    pub fn with_version(mut self, version: impl Into<String>) -> Self {
        self.version = Some(version.into());
        self
    }

    /// Add a hook that this plugin supports.
    pub fn with_hook(mut self, hook: PluginHook) -> Self {
        self.hooks.push(hook);
        self
    }
}

// ── V1 Plugin Hooks interface ─────────────────────────────────────────

/// Context passed to plugins when they are initialized.
///
/// Ported from `packages/plugin/src/index.ts` `PluginInput`.
pub struct PluginInput {
    /// The project root directory.
    pub project: std::path::PathBuf,
    /// The working directory.
    pub directory: std::path::PathBuf,
    /// The git worktree path (if in a worktree).
    pub worktree: Option<std::path::PathBuf>,
    /// The workspace root.
    pub workspace: std::path::PathBuf,
    /// Server URL for API calls.
    pub server_url: Option<String>,
}

/// The V1 plugin hooks interface.
///
/// Plugins implement this trait to intercept and transform various stages
/// of the agent lifecycle. Each method corresponds to a named hook that
/// can be triggered during execution.
///
/// # Source
/// Ported from `packages/plugin/src/index.ts` `Hooks` interface.
#[async_trait::async_trait]
pub trait PluginHooks: Send + Sync {
    /// Called when the plugin is unloaded. Use for cleanup.
    async fn dispose(&self) {}

    /// Called when an event is received from the event bus.
    async fn on_event(&self, _event: &str, _data: &serde_json::Value) {}

    /// Called when configuration changes.
    async fn on_config_change(&self, _config: &serde_json::Value) {}

    /// Intercept and modify user messages before processing.
    ///
    /// Return the (possibly modified) message text.
    async fn on_chat_message(&self, message: String) -> String {
        message
    }

    /// Modify LLM request parameters before sending.
    async fn on_chat_params(&self, params: &mut serde_json::Value) {}

    /// Modify LLM request headers before sending.
    async fn on_chat_headers(&self, headers: &mut HashMap<String, String>) {}

    /// Intercept permission decisions.
    ///
    /// Return `true` to allow, `false` to deny, or `None` to use default.
    async fn on_permission_ask(
        &self,
        _permission: &str,
        _target: &str,
    ) -> Option<bool> {
        None
    }

    /// Before a tool executes — can intercept or modify the call.
    async fn on_tool_execute_before(
        &self,
        _tool_name: &str,
        _args: &serde_json::Value,
    ) -> Option<serde_json::Value> {
        None
    }

    /// After a tool executes — can inspect or transform the result.
    async fn on_tool_execute_after(
        &self,
        _tool_name: &str,
        _result: &serde_json::Value,
    ) -> Option<serde_json::Value> {
        None
    }

    /// Modify tool definitions before they're sent to the LLM.
    async fn on_tool_definition(&self, _definitions: &mut Vec<serde_json::Value>) {}

    /// Modify shell environment variables.
    async fn on_shell_env(&self, _env: &mut HashMap<String, String>) {}

    /// Before a command executes.
    async fn on_command_execute_before(&self, _command: &str) -> Option<String> {
        None
    }

    /// Provider authentication — load custom credentials.
    async fn on_auth(
        &self,
        _provider_id: &str,
    ) -> Option<HashMap<String, serde_json::Value>> {
        None
    }

    /// Dynamic model discovery for a provider.
    async fn on_provider_discover(
        &self,
        _provider_id: &str,
    ) -> Option<Vec<serde_json::Value>> {
        None
    }

    /// Modify the system prompt before sending to LLM.
    async fn on_chat_system_transform(&self, _system: &mut String) {}

    /// Toggle auto-continue after compaction.
    async fn on_compaction_autocontinue(&self) -> Option<bool> {
        None
    }

    /// Pick a small model for lightweight tasks.
    async fn on_provider_small_model(&self) -> Option<String> {
        None
    }
}

// ── Auth plugin types ─────────────────────────────────────────────────

/// Type of authentication method.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum AuthMethodType {
    /// API key authentication.
    Api,
    /// OAuth authentication.
    OAuth,
}

/// A prompt to display to the user during auth setup.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct AuthPrompt {
    /// Prompt type (e.g., "text").
    #[serde(rename = "type")]
    pub prompt_type: String,
    /// Key to store the response.
    pub key: String,
    /// Message to display.
    pub message: String,
    /// Placeholder text.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub placeholder: Option<String>,
}

/// An authentication method provided by a plugin.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct AuthMethod {
    /// Method type (api or oauth).
    #[serde(rename = "type")]
    pub method_type: AuthMethodType,
    /// Human-readable label.
    pub label: String,
    /// Prompts to display during setup.
    #[serde(default)]
    pub prompts: Vec<AuthPrompt>,
}

/// Auth hook provided by a plugin.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct AuthHook {
    /// Provider ID this auth hook applies to.
    pub provider: String,
    /// Available authentication methods.
    #[serde(default)]
    pub methods: Vec<AuthMethod>,
}

// ── Built-in auth plugins ─────────────────────────────────────────────

/// Create the Azure auth plugin.
///
/// Provides API key auth with optional resource name prompt.
///
/// # Source
/// Ported from `packages/opencode/src/plugin/azure.ts`.
pub fn azure_auth_plugin() -> AuthHook {
    let mut prompts = Vec::new();
    if std::env::var("AZURE_RESOURCE_NAME").is_err() {
        prompts.push(AuthPrompt {
            prompt_type: "text".to_string(),
            key: "resourceName".to_string(),
            message: "Enter Azure Resource Name".to_string(),
            placeholder: Some("e.g. my-models".to_string()),
        });
    }

    AuthHook {
        provider: "azure".to_string(),
        methods: vec![AuthMethod {
            method_type: AuthMethodType::Api,
            label: "API key".to_string(),
            prompts,
        }],
    }
}

/// Create the DigitalOcean auth plugin.
///
/// # Source
/// Ported from `packages/opencode/src/plugin/digitalocean.ts`.
pub fn digitalocean_auth_plugin() -> AuthHook {
    let mut prompts = Vec::new();
    if std::env::var("DIGITALOCEAN_API_KEY").is_err() {
        prompts.push(AuthPrompt {
            prompt_type: "text".to_string(),
            key: "apiKey".to_string(),
            message: "Enter DigitalOcean API Key".to_string(),
            placeholder: Some("e.g. dgo_v1_...".to_string()),
        });
    }

    AuthHook {
        provider: "digitalocean".to_string(),
        methods: vec![AuthMethod {
            method_type: AuthMethodType::Api,
            label: "API key".to_string(),
            prompts,
        }],
    }
}

/// Create the xAI auth plugin.
///
/// Provides OAuth (browser and headless) and API key auth.
///
/// # Source
/// Ported from `packages/opencode/src/plugin/xai.ts`.
pub fn xai_auth_plugin() -> AuthHook {
    AuthHook {
        provider: "xai".to_string(),
        methods: vec![
            AuthMethod {
                method_type: AuthMethodType::OAuth,
                label: "xAI Grok OAuth (SuperGrok Subscription)".to_string(),
                prompts: Vec::new(),
            },
            AuthMethod {
                method_type: AuthMethodType::OAuth,
                label: "xAI Grok OAuth (Headless / Remote / VPS)".to_string(),
                prompts: Vec::new(),
            },
            AuthMethod {
                method_type: AuthMethodType::Api,
                label: "Manually enter API Key".to_string(),
                prompts: Vec::new(),
            },
        ],
    }
}

/// Create the Cloudflare Workers auth plugin.
///
/// # Source
/// Ported from `packages/opencode/src/plugin/cloudflare.ts`.
pub fn cloudflare_workers_auth_plugin() -> AuthHook {
    let mut prompts = Vec::new();
    if std::env::var("CLOUDFLARE_API_TOKEN").is_err() {
        prompts.push(AuthPrompt {
            prompt_type: "text".to_string(),
            key: "apiToken".to_string(),
            message: "Enter Cloudflare API Token".to_string(),
            placeholder: Some("e.g. ...".to_string()),
        });
    }

    AuthHook {
        provider: "cloudflare".to_string(),
        methods: vec![AuthMethod {
            method_type: AuthMethodType::Api,
            label: "API token".to_string(),
            prompts,
        }],
    }
}

/// Create the Snowflake Cortex auth plugin.
///
/// # Source
/// Ported from `packages/opencode/src/plugin/snowflake-cortex.ts`.
pub fn snowflake_cortex_auth_plugin() -> AuthHook {
    let mut prompts = Vec::new();
    if std::env::var("SNOWFLAKE_ACCOUNT").is_err() {
        prompts.push(AuthPrompt {
            prompt_type: "text".to_string(),
            key: "account".to_string(),
            message: "Enter Snowflake Account".to_string(),
            placeholder: Some("e.g. myorg-myaccount".to_string()),
        });
    }
    if std::env::var("SNOWFLAKE_PASSWORD").is_err() {
        prompts.push(AuthPrompt {
            prompt_type: "text".to_string(),
            key: "password".to_string(),
            message: "Enter Snowflake Password".to_string(),
            placeholder: None,
        });
    }

    AuthHook {
        provider: "snowflake-cortex".to_string(),
        methods: vec![AuthMethod {
            method_type: AuthMethodType::Api,
            label: "Username/password".to_string(),
            prompts,
        }],
    }
}

/// Get all built-in auth plugins.
pub fn built_in_auth_plugins() -> Vec<AuthHook> {
    vec![
        azure_auth_plugin(),
        digitalocean_auth_plugin(),
        xai_auth_plugin(),
        cloudflare_workers_auth_plugin(),
        snowflake_cortex_auth_plugin(),
    ]
}

// ── V2 Plugin system ──────────────────────────────────────────────────

/// V2 Plugin hook names.
///
/// # Source
/// Ported from `packages/core/src/plugin.ts` `PluginV2.Hooks`.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum PluginV2Hook {
    /// Transform provider catalog settings.
    CatalogTransform,
    /// Customize AI SDK language model selection.
    AiSdkLanguage,
    /// Customize AI SDK provider instance.
    AiSdkSdk,
}

impl PluginV2Hook {
    /// Return the string name of this hook.
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::CatalogTransform => "catalog.transform",
            Self::AiSdkLanguage => "aisdk.language",
            Self::AiSdkSdk => "aisdk.sdk",
        }
    }

    /// Parse a hook name string.
    pub fn from_str(s: &str) -> Option<Self> {
        match s {
            "catalog.transform" => Some(Self::CatalogTransform),
            "aisdk.language" => Some(Self::AiSdkLanguage),
            "aisdk.sdk" => Some(Self::AiSdkSdk),
            _ => None,
        }
    }
}

/// V2 Plugin definition.
///
/// # Source
/// Ported from `packages/core/src/plugin.ts` `PluginV2.define()`.
pub struct PluginV2Definition {
    /// Unique plugin ID.
    pub id: String,
    /// Hooks provided by this plugin.
    pub hooks: Vec<PluginV2Hook>,
}

impl PluginV2Definition {
    /// Create a new V2 plugin definition.
    pub fn new(id: impl Into<String>) -> Self {
        Self {
            id: id.into(),
            hooks: Vec::new(),
        }
    }

    /// Add a hook to this plugin.
    pub fn with_hook(mut self, hook: PluginV2Hook) -> Self {
        self.hooks.push(hook);
        self
    }
}

/// V2 Plugin service for managing scoped plugins.
///
/// # Source
/// Ported from `packages/core/src/plugin.ts` `PluginV2.Service`.
pub struct PluginV2Service {
    /// Registered V2 plugins keyed by ID.
    plugins: HashMap<String, PluginV2Definition>,
    /// Active scopes for each plugin.
    scopes: HashMap<String, bool>,
}

impl Default for PluginV2Service {
    fn default() -> Self {
        Self::new()
    }
}

impl PluginV2Service {
    /// Create a new V2 plugin service.
    pub fn new() -> Self {
        Self {
            plugins: HashMap::new(),
            scopes: HashMap::new(),
        }
    }

    /// Add a V2 plugin with a new scope.
    ///
    /// # Source
    /// Ported from `packages/core/src/plugin.ts` `PluginV2.Service.add()`.
    pub fn add(&mut self, plugin: PluginV2Definition) {
        let id = plugin.id.clone();
        self.scopes.insert(id.clone(), true);
        self.plugins.insert(id, plugin);
    }

    /// Remove a V2 plugin and close its scope.
    ///
    /// # Source
    /// Ported from `packages/core/src/plugin.ts` `PluginV2.Service.remove()`.
    pub fn remove(&mut self, id: &str) -> Option<PluginV2Definition> {
        self.scopes.remove(id);
        self.plugins.remove(id)
    }

    /// Trigger a hook on all registered V2 plugins.
    ///
    /// # Source
    /// Ported from `packages/core/src/plugin.ts` `PluginV2.Service.trigger()`.
    pub fn trigger(&self, hook: &PluginV2Hook) -> Vec<&str> {
        self.plugins
            .values()
            .filter(|p| p.hooks.contains(hook))
            .map(|p| p.id.as_str())
            .collect()
    }

    /// Trigger a hook on a specific V2 plugin.
    ///
    /// # Source
    /// Ported from `packages/core/src/plugin.ts` `PluginV2.Service.triggerFor()`.
    pub fn trigger_for(&self, plugin_id: &str, hook: &PluginV2Hook) -> bool {
        self.plugins
            .get(plugin_id)
            .map(|p| p.hooks.contains(hook))
            .unwrap_or(false)
    }

    /// Check if a plugin has a specific hook.
    pub fn has_hook(&self, plugin_id: &str, hook: &PluginV2Hook) -> bool {
        self.trigger_for(plugin_id, hook)
    }

    /// Get a plugin by ID.
    pub fn get(&self, id: &str) -> Option<&PluginV2Definition> {
        self.plugins.get(id)
    }

    /// Get all registered plugin IDs.
    pub fn ids(&self) -> Vec<&str> {
        self.plugins.keys().map(|s| s.as_str()).collect()
    }

    /// Check if a plugin's scope is active.
    pub fn is_scope_active(&self, id: &str) -> bool {
        self.scopes.get(id).copied().unwrap_or(false)
    }

    /// Get the number of registered plugins.
    pub fn count(&self) -> usize {
        self.plugins.len()
    }
}

// ── Plugin spec parsing ───────────────────────────────────────────────

/// Result of parsing a plugin specifier string.
///
/// Ported from `packages/opencode/src/plugin/shared.ts` `parsePluginSpecifier()`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ParsedSpec {
    /// Package name or path.
    pub pkg: String,
    /// Version string (empty if none specified, "latest" for bare package names).
    pub version: String,
}

/// Parse a plugin specifier into its package name and version components.
///
/// Handles npm-style specs like `foo`, `foo@1.2.3`, `@scope/foo@latest`,
/// and npm-package-arg alias format like `npm:foo@*`.
///
/// Ported from `packages/opencode/src/plugin/shared.ts` `parsePluginSpecifier()`.
pub fn parse_specifier(spec: &str) -> ParsedSpec {
    // Handle npm-package-arg alias format: `npm:package@version`
    if let Some(rest) = spec.strip_prefix("npm:") {
        if let Some((name, version)) = rest.rsplit_once('@') {
            if !version.is_empty() && version != "*" {
                return ParsedSpec {
                    pkg: name.to_string(),
                    version: version.to_string(),
                };
            }
        }
        // Just a bare name after npm: prefix
        return ParsedSpec {
            pkg: rest.to_string(),
            version: "latest".to_string(),
        };
    }

    // Handle scoped packages: `@scope/name@version`
    if spec.starts_with('@') {
        if let Some(at_pos) = spec[1..].find('@') {
            let pkg = spec[..=at_pos].to_string();
            let version = spec[1 + at_pos + 1..].to_string();
            if version.is_empty() {
                return ParsedSpec {
                    pkg,
                    version: "latest".to_string(),
                };
            }
            return ParsedSpec { pkg, version };
        }
        // Scoped package without version
        return ParsedSpec {
            pkg: spec.to_string(),
            version: "latest".to_string(),
        };
    }

    // Handle bare packages: `name@version` or just `name`
    if let Some((name, version)) = spec.rsplit_once('@') {
        if !name.is_empty() && !version.is_empty() {
            return ParsedSpec {
                pkg: name.to_string(),
                version: version.to_string(),
            };
        }
    }

    ParsedSpec {
        pkg: spec.to_string(),
        version: if spec.is_empty() {
            String::new()
        } else {
            "latest".to_string()
        },
    }
}

/// Determine the plugin source from its specifier string.
///
/// File plugins start with `file://`, `.`, or an absolute path.
///
/// Ported from `packages/opencode/src/plugin/shared.ts` `pluginSource()`.
pub fn plugin_source(spec: &str) -> PluginSource {
    if is_path_plugin_spec(spec) {
        PluginSource::File
    } else {
        PluginSource::Npm
    }
}

/// Check if a specifier refers to a local file path.
///
/// Ported from `packages/opencode/src/plugin/shared.ts` `isPathPluginSpec()`.
pub fn is_path_plugin_spec(spec: &str) -> bool {
    spec.starts_with("file://")
        || spec.starts_with('.')
        || spec.starts_with('/')
        || (spec.len() >= 2
            && spec.as_bytes().get(1) == Some(&b':')
            && spec.as_bytes()[0].is_ascii_alphabetic())
}

/// Deprecated plugin package names that are now built-in.
///
/// Ported from `packages/opencode/src/plugin/shared.ts` `DEPRECATED_PLUGIN_PACKAGES`.
static DEPRECATED_PLUGIN_PACKAGES: &[&str] =
    &["opencode-openai-codex-auth", "opencode-copilot-auth"];

/// Check if a plugin spec refers to a deprecated (now built-in) package.
pub fn is_deprecated_plugin(spec: &str) -> bool {
    DEPRECATED_PLUGIN_PACKAGES
        .iter()
        .any(|pkg| spec.contains(pkg))
}

// ── Plugin metadata entry ─────────────────────────────────────────────

/// Persistent metadata about a loaded plugin, stored in the plugin-meta.json file.
///
/// Ported from `packages/opencode/src/plugin/meta.ts` `Entry`.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PluginMetaEntry {
    /// Plugin identifier.
    pub id: String,
    /// Plugin source (file or npm).
    pub source: PluginSource,
    /// Plugin specifier string.
    pub spec: String,
    /// Resolved target path.
    pub target: String,
    /// Requested version from the spec.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub requested: Option<String>,
    /// Actual resolved version (for npm plugins).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub version: Option<String>,
    /// First time this plugin was loaded (Unix timestamp millis).
    pub first_time: u64,
    /// Last time this plugin was loaded (Unix timestamp millis).
    pub last_time: u64,
    /// Time when this plugin's fingerprint last changed.
    pub time_changed: u64,
    /// How many times this plugin has been loaded.
    pub load_count: u64,
    /// Fingerprint string used to detect changes.
    pub fingerprint: String,
}

/// Compute a fingerprint for a plugin entry.
///
/// File plugins: `target|modified_ms`
/// Npm plugins: `target|requested|version`
///
/// Ported from `packages/opencode/src/plugin/meta.ts` `fingerprint()`.
pub fn compute_fingerprint(
    source: PluginSource,
    target: &str,
    requested: Option<&str>,
    version: Option<&str>,
    modified: Option<u64>,
) -> String {
    match source {
        PluginSource::File => {
            format!(
                "{}|{}",
                target,
                modified.map(|m| m.to_string()).unwrap_or_default()
            )
        }
        PluginSource::Npm => {
            format!(
                "{}|{}|{}",
                target,
                requested.unwrap_or(""),
                version.unwrap_or("")
            )
        }
    }
}

// ── Plugin manager ────────────────────────────────────────────────────

/// Runtime flags that affect plugin loading and behavior.
///
/// # Source
/// Ported from `packages/opencode/src/plugin/index.ts` runtime flags.
#[derive(Debug, Clone, Default)]
pub struct RuntimeFlags {
    /// Skip all external plugins when true.
    pub pure: bool,
    /// Skip built-in auth plugins when true.
    pub disable_default_plugins: bool,
    /// Skip .claude-specific skills when true.
    pub disable_external_skills: bool,
    /// Disable Claude Code skills separately from other external skills.
    pub disable_claude_code_skills: bool,
}

impl RuntimeFlags {
    /// Create flags with all defaults (all false).
    pub fn new() -> Self {
        Self::default()
    }

    /// Create flags for pure mode (no external plugins).
    pub fn pure() -> Self {
        Self {
            pure: true,
            ..Default::default()
        }
    }

    /// Check if external plugins should be loaded.
    pub fn should_load_external(&self) -> bool {
        !self.pure
    }

    /// Check if default auth plugins should be loaded.
    pub fn should_load_default_auth(&self) -> bool {
        !self.disable_default_plugins
    }
}

/// Manages the lifecycle of plugins: registration, hook dispatch, and disposal.
///
/// Ported from `packages/opencode/src/plugin/index.ts` `Service`.
pub struct PluginManager {
    /// All loaded plugins.
    plugins: Vec<Plugin>,
    /// Plugin metadata store keyed by plugin id.
    meta: HashMap<String, PluginMetaEntry>,
    /// Hook handlers keyed by plugin id.
    handlers: HashMap<String, Arc<dyn PluginHooks>>,
    /// Runtime flags affecting plugin behavior.
    flags: RuntimeFlags,
    /// Error tracker for plugin errors.
    errors: PluginErrorTracker,
    /// Timestamp of last initialization (Unix millis).
    last_init: Option<u64>,
}

impl Default for PluginManager {
    fn default() -> Self {
        Self::new()
    }
}

impl std::fmt::Debug for PluginManager {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("PluginManager")
            .field("plugins", &self.plugins)
            .field("meta", &self.meta)
            .field("handler_count", &self.handlers.len())
            .field("flags", &self.flags)
            .field("last_init", &self.last_init)
            .finish()
    }
}

impl PluginManager {
    /// Create an empty plugin manager.
    pub fn new() -> Self {
        Self {
            plugins: Vec::new(),
            meta: HashMap::new(),
            handlers: HashMap::new(),
            flags: RuntimeFlags::new(),
            errors: PluginErrorTracker::new(),
            last_init: None,
        }
    }

    /// Create a plugin manager with custom runtime flags.
    pub fn with_flags(flags: RuntimeFlags) -> Self {
        Self {
            plugins: Vec::new(),
            meta: HashMap::new(),
            handlers: HashMap::new(),
            flags,
            errors: PluginErrorTracker::new(),
            last_init: None,
        }
    }

    /// Get the current runtime flags.
    pub fn flags(&self) -> &RuntimeFlags {
        &self.flags
    }

    /// Get mutable access to runtime flags.
    pub fn flags_mut(&mut self) -> &mut RuntimeFlags {
        &mut self.flags
    }

    /// Get the error tracker.
    pub fn errors(&self) -> &PluginErrorTracker {
        &self.errors
    }

    /// Get mutable access to the error tracker.
    pub fn errors_mut(&mut self) -> &mut PluginErrorTracker {
        &mut self.errors
    }

    /// Record an error for a plugin.
    pub fn record_error(&mut self, plugin_id: &str, stage: PluginErrorStage, message: &str) {
        let error = PluginError {
            plugin_id: plugin_id.to_string(),
            stage,
            message: message.to_string(),
            timestamp: current_time_millis(),
        };
        self.errors.record(error);
    }

    /// Register a plugin in the manager.
    ///
    /// If a plugin with the same id already exists, it is replaced.
    /// Returns the previous plugin if one existed.
    pub fn register(&mut self, plugin: Plugin) -> Option<Plugin> {
        let prev = self
            .plugins
            .iter()
            .position(|p| p.id == plugin.id)
            .map(|i| self.plugins.remove(i));
        self.plugins.push(plugin);
        prev
    }

    /// Remove a plugin by id.
    pub fn unregister(&mut self, id: &str) -> Option<Plugin> {
        if let Some(pos) = self.plugins.iter().position(|p| p.id == id) {
            Some(self.plugins.remove(pos))
        } else {
            None
        }
    }

    /// List all loaded plugins.
    pub fn list(&self) -> &[Plugin] {
        &self.plugins
    }

    /// Find a plugin by id.
    pub fn get(&self, id: &str) -> Option<&Plugin> {
        self.plugins.iter().find(|p| p.id == id)
    }

    /// Count of loaded plugins.
    pub fn count(&self) -> usize {
        self.plugins.len()
    }

    /// Return a list of all plugin ids.
    pub fn ids(&self) -> Vec<&str> {
        self.plugins.iter().map(|p| p.id.as_str()).collect()
    }

    /// Check if any registered plugin supports the given hook.
    pub fn has_hook(&self, hook: &PluginHook) -> bool {
        self.plugins.iter().any(|p| p.hooks.contains(hook))
    }

    /// Return all plugins that support the given hook.
    pub fn plugins_for_hook(&self, hook: &PluginHook) -> Vec<&Plugin> {
        self.plugins
            .iter()
            .filter(|p| p.hooks.contains(hook))
            .collect()
    }

    /// Register a hook handler for a plugin.
    ///
    /// The handler is called when the corresponding hook is triggered.
    pub fn register_handler(&mut self, plugin_id: &str, handler: Arc<dyn PluginHooks>) {
        self.handlers.insert(plugin_id.to_string(), handler);
    }

    /// Get a plugin's hook handler by id.
    pub fn get_handler(&self, plugin_id: &str) -> Option<&Arc<dyn PluginHooks>> {
        self.handlers.get(plugin_id)
    }

    /// Remove a plugin's hook handler.
    pub fn remove_handler(&mut self, plugin_id: &str) -> Option<Arc<dyn PluginHooks>> {
        self.handlers.remove(plugin_id)
    }

    /// Trigger a hook on all registered handlers.
    ///
    /// Calls the corresponding method on each handler that belongs to a plugin
    /// with the given hook registered.
    ///
    /// # Source
    /// Ported from `packages/opencode/src/plugin/index.ts` `trigger()`.
    pub async fn trigger(&self, hook: &PluginHook) {
        for plugin in &self.plugins {
            if !plugin.hooks.contains(hook) {
                continue;
            }
            if let Some(handler) = self.handlers.get(&plugin.id) {
                match hook {
                    PluginHook::Dispose => handler.dispose().await,
                    PluginHook::Event => {
                        // Event hooks receive data externally
                    }
                    PluginHook::Config => {
                        // Config hooks receive config externally
                    }
                    _ => {}
                }
            }
        }
    }

    /// Trigger the `on_event` hook on all handlers.
    pub async fn trigger_event(&self, event: &str, data: &serde_json::Value) {
        for plugin in &self.plugins {
            if !plugin.hooks.contains(&PluginHook::Event) {
                continue;
            }
            if let Some(handler) = self.handlers.get(&plugin.id) {
                handler.on_event(event, data).await;
            }
        }
    }

    /// Trigger the `on_config_change` hook on all handlers.
    pub async fn trigger_config_change(&self, config: &serde_json::Value) {
        for plugin in &self.plugins {
            if !plugin.hooks.contains(&PluginHook::Config) {
                continue;
            }
            if let Some(handler) = self.handlers.get(&plugin.id) {
                handler.on_config_change(config).await;
            }
        }
    }

    /// Trigger `on_chat_message` on all handlers, collecting transformations.
    pub async fn trigger_chat_message(&self, message: String) -> String {
        let mut msg = message;
        for plugin in &self.plugins {
            if !plugin.hooks.contains(&PluginHook::ChatMessage) {
                continue;
            }
            if let Some(handler) = self.handlers.get(&plugin.id) {
                msg = handler.on_chat_message(msg).await;
            }
        }
        msg
    }

    /// Trigger `on_chat_params` on all handlers.
    pub async fn trigger_chat_params(&self, params: &mut serde_json::Value) {
        for plugin in &self.plugins {
            if !plugin.hooks.contains(&PluginHook::ChatParams) {
                continue;
            }
            if let Some(handler) = self.handlers.get(&plugin.id) {
                handler.on_chat_params(params).await;
            }
        }
    }

    /// Trigger `on_chat_headers` on all handlers.
    pub async fn trigger_chat_headers(&self, headers: &mut HashMap<String, String>) {
        for plugin in &self.plugins {
            if !plugin.hooks.contains(&PluginHook::ChatHeaders) {
                continue;
            }
            if let Some(handler) = self.handlers.get(&plugin.id) {
                handler.on_chat_headers(headers).await;
            }
        }
    }

    /// Trigger `on_permission_ask` on all handlers until one responds.
    pub async fn trigger_permission_ask(
        &self,
        permission: &str,
        target: &str,
    ) -> Option<bool> {
        for plugin in &self.plugins {
            if !plugin.hooks.contains(&PluginHook::PermissionAsk) {
                continue;
            }
            if let Some(handler) = self.handlers.get(&plugin.id) {
                if let Some(result) = handler.on_permission_ask(permission, target).await {
                    return Some(result);
                }
            }
        }
        None
    }

    /// Trigger `on_tool_execute_before` on all handlers.
    pub async fn trigger_tool_execute_before(
        &self,
        tool_name: &str,
        args: &serde_json::Value,
    ) -> Option<serde_json::Value> {
        let mut current_args = args.clone();
        for plugin in &self.plugins {
            if !plugin.hooks.contains(&PluginHook::ToolExecuteBefore) {
                continue;
            }
            if let Some(handler) = self.handlers.get(&plugin.id) {
                if let Some(modified) = handler.on_tool_execute_before(tool_name, &current_args).await {
                    current_args = modified;
                }
            }
        }
        Some(current_args)
    }

    /// Trigger `on_tool_execute_after` on all handlers.
    pub async fn trigger_tool_execute_after(
        &self,
        tool_name: &str,
        result: &serde_json::Value,
    ) -> Option<serde_json::Value> {
        let mut current_result = result.clone();
        for plugin in &self.plugins {
            if !plugin.hooks.contains(&PluginHook::ToolExecuteAfter) {
                continue;
            }
            if let Some(handler) = self.handlers.get(&plugin.id) {
                if let Some(modified) = handler.on_tool_execute_after(tool_name, &current_result).await {
                    current_result = modified;
                }
            }
        }
        Some(current_result)
    }

    /// Trigger `on_tool_definition` on all handlers.
    pub async fn trigger_tool_definition(&self, definitions: &mut Vec<serde_json::Value>) {
        for plugin in &self.plugins {
            if !plugin.hooks.contains(&PluginHook::ToolDefinition) {
                continue;
            }
            if let Some(handler) = self.handlers.get(&plugin.id) {
                handler.on_tool_definition(definitions).await;
            }
        }
    }

    /// Trigger `on_shell_env` on all handlers.
    pub async fn trigger_shell_env(&self, env: &mut HashMap<String, String>) {
        for plugin in &self.plugins {
            if !plugin.hooks.contains(&PluginHook::ShellEnv) {
                continue;
            }
            if let Some(handler) = self.handlers.get(&plugin.id) {
                handler.on_shell_env(env).await;
            }
        }
    }

    /// Trigger `on_chat_system_transform` on all handlers.
    pub async fn trigger_chat_system_transform(&self, system: &mut String) {
        for plugin in &self.plugins {
            if !plugin.hooks.contains(&PluginHook::ExperimentalChatSystemTransform) {
                continue;
            }
            if let Some(handler) = self.handlers.get(&plugin.id) {
                handler.on_chat_system_transform(system).await;
            }
        }
    }

    /// Trigger `dispose` on all handlers for cleanup.
    pub async fn dispose_all(&self) {
        for plugin in &self.plugins {
            if let Some(handler) = self.handlers.get(&plugin.id) {
                handler.dispose().await;
            }
        }
    }

    // ── Metadata persistence ─────────────────────────────────────────

    /// Save plugin metadata to a JSON file.
    ///
    /// # Source
    /// Ported from `packages/opencode/src/plugin/meta.ts` `touch()`.
    pub fn save_meta(&self, path: &std::path::Path) -> Result<(), std::io::Error> {
        let data = serde_json::json!({
            "plugins": self.meta,
        });
        let json = serde_json::to_string_pretty(&data).unwrap_or_default();

        // Ensure parent directory exists
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)?;
        }

        std::fs::write(path, json)
    }

    /// Load plugin metadata from a JSON file.
    ///
    /// # Source
    /// Ported from `packages/opencode/src/plugin/meta.ts` `touch()`.
    pub fn load_meta(&mut self, path: &std::path::Path) -> Result<(), std::io::Error> {
        if !path.exists() {
            return Ok(());
        }

        let content = std::fs::read_to_string(path)?;
        let data: serde_json::Value = serde_json::from_str(&content)
            .map_err(|e| std::io::Error::new(std::io::ErrorKind::InvalidData, e))?;

        if let Some(plugins) = data.get("plugins").and_then(|v| v.as_object()) {
            for (id, entry_value) in plugins {
                if let Ok(entry) = serde_json::from_value::<PluginMetaEntry>(entry_value.clone()) {
                    self.meta.insert(id.clone(), entry);
                }
            }
        }

        Ok(())
    }

    /// Get the default metadata path (~/.local/share/opencode/plugin-meta.json).
    pub fn default_meta_path() -> Option<std::path::PathBuf> {
        dirs::data_local_dir().map(|d| d.join("opencode").join("plugin-meta.json"))
    }

    /// Load metadata from the default location.
    pub fn load_default_meta(&mut self) -> Result<(), std::io::Error> {
        if let Some(path) = Self::default_meta_path() {
            self.load_meta(&path)?;
        }
        Ok(())
    }

    /// Save metadata to the default location.
    pub fn save_default_meta(&self) -> Result<(), std::io::Error> {
        if let Some(path) = Self::default_meta_path() {
            self.save_meta(&path)?;
        }
        Ok(())
    }

    // ── Event bus integration ─────────────────────────────────────────

    /// Forward an event to all registered plugin event hooks.
    ///
    /// This method should be called when events are published to the event bus.
    ///
    /// # Source
    /// Ported from `packages/opencode/src/plugin/index.ts` event bus subscription.
    pub async fn on_event(&self, event: &str, data: &serde_json::Value) {
        self.trigger_event(event, data).await;
    }

    /// Notify all plugins of a config change.
    ///
    /// # Source
    /// Ported from `packages/opencode/src/plugin/index.ts` config notification.
    pub async fn on_config_change(&self, config: &serde_json::Value) {
        self.trigger_config_change(config).await;
    }

    /// Publish a plugin error to the event bus.
    ///
    /// # Source
    /// Ported from `packages/opencode/src/plugin/index.ts` error publishing.
    pub fn publish_error(&self, plugin_id: &str, stage: &str, error: &str) {
        tracing::error!(
            plugin_id = plugin_id,
            stage = stage,
            "plugin error: {}",
            error
        );
    }

    /// Initialize all plugins and call their init hooks.
    ///
    /// # Source
    /// Ported from `packages/opencode/src/plugin/index.ts` `init()`.
    pub async fn init(&mut self) {
        self.last_init = Some(current_time_millis());
        tracing::info!("plugin manager initialized with {} plugins", self.plugins.len());
    }

    /// Record metadata for a plugin after loading.
    ///
    /// Computes the fingerprint and state transition.
    pub fn touch_meta(
        &mut self,
        id: &str,
        source: PluginSource,
        spec: &str,
        target: &str,
        requested: Option<&str>,
        version: Option<&str>,
        modified: Option<u64>,
    ) -> (PluginState, &PluginMetaEntry) {
        let now = current_time_millis();
        let fingerprint = compute_fingerprint(source, target, requested, version, modified);

        let entry = if let Some(prev) = self.meta.get(id) {
            let state = if prev.fingerprint == fingerprint {
                PluginState::Same
            } else {
                PluginState::Updated
            };
            let time_changed = if state == PluginState::Updated {
                now
            } else {
                prev.time_changed
            };
            let new_entry = PluginMetaEntry {
                id: id.to_string(),
                source,
                spec: spec.to_string(),
                target: target.to_string(),
                requested: requested.map(String::from),
                version: version.map(String::from),
                first_time: prev.first_time,
                last_time: now,
                time_changed,
                load_count: prev.load_count + 1,
                fingerprint,
            };
            self.meta.insert(id.to_string(), new_entry);
            (state, &self.meta[id])
        } else {
            let new_entry = PluginMetaEntry {
                id: id.to_string(),
                source,
                spec: spec.to_string(),
                target: target.to_string(),
                requested: requested.map(String::from),
                version: version.map(String::from),
                first_time: now,
                last_time: now,
                time_changed: now,
                load_count: 1,
                fingerprint,
            };
            self.meta.insert(id.to_string(), new_entry);
            (PluginState::First, &self.meta[id])
        };

        (entry.0, entry.1)
    }

    /// Get metadata for a plugin by id.
    pub fn get_meta(&self, id: &str) -> Option<&PluginMetaEntry> {
        self.meta.get(id)
    }

    /// Get all plugin metadata.
    pub fn all_meta(&self) -> &HashMap<String, PluginMetaEntry> {
        &self.meta
    }

    /// Clear all plugins and metadata.
    pub fn clear(&mut self) {
        self.plugins.clear();
        self.meta.clear();
        self.last_init = None;
    }

    /// Load a plugin from a specifier string.
    ///
    /// For file plugins, the target path is resolved from the spec.
    /// For npm plugins, the target is a placeholder (actual install handled externally).
    ///
    /// Ported from `packages/opencode/src/plugin/index.ts` plugin loading.
    pub fn load(&mut self, spec: impl Into<String>) -> Result<&Plugin, PluginLoadError> {
        let spec: String = spec.into();
        if spec.is_empty() {
            return Err(PluginLoadError::EmptySpec);
        }
        if is_deprecated_plugin(&spec) {
            return Err(PluginLoadError::DeprecatedPlugin { spec });
        }

        let source = plugin_source(&spec);
        let parsed = parse_specifier(&spec);

        let target = match source {
            PluginSource::File => {
                let path = spec.strip_prefix("file://").unwrap_or(&spec);
                PathBuf::from(path)
            }
            PluginSource::Npm => PathBuf::from(format!("/node_modules/{}", parsed.pkg)),
        };

        let mut plugin = Plugin::new(parsed.pkg.as_str(), parsed.pkg.as_str(), source)
            .with_spec(spec)
            .with_target(target);

        if !parsed.version.is_empty() {
            plugin = plugin.with_version(parsed.version);
        }

        self.register(plugin);
        Ok(self.plugins.last().expect("plugin was just registered"))
    }

    /// Validate a plugin specifier and return what would be installed.
    ///
    /// This is a stub — actual npm install is handled externally.
    ///
    /// Ported from `packages/opencode/src/plugin/install.ts`.
    pub fn install_validate(&self, spec: &str) -> Result<ParsedSpec, PluginLoadError> {
        if spec.is_empty() {
            return Err(PluginLoadError::EmptySpec);
        }
        if is_deprecated_plugin(spec) {
            return Err(PluginLoadError::DeprecatedPlugin {
                spec: spec.to_string(),
            });
        }
        Ok(parse_specifier(spec))
    }
}

// ── Plugin file resolution ────────────────────────────────────────────

/// Errors during plugin file resolution.
#[derive(Debug, thiserror::Error)]
pub enum PluginResolveError {
    /// The plugin directory does not exist.
    #[error("plugin directory not found: {path}")]
    DirectoryNotFound { path: String },

    /// No package.json found in the plugin directory.
    #[error("no package.json found in {path}")]
    NoPackageJson { path: String },

    /// package.json is invalid JSON.
    #[error("invalid package.json in {path}: {source}")]
    InvalidPackageJson {
        path: String,
        source: serde_json::Error,
    },

    /// No entrypoint found for the plugin kind.
    #[error("no entrypoint found for {kind} in {path}")]
    NoEntrypoint { kind: String, path: String },

    /// Plugin compatibility check failed.
    #[error("plugin requires opencode {required}, but current version is {current}")]
    IncompatibleVersion { required: String, current: String },
}

/// Plugin error stages for reporting.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PluginErrorStage {
    /// Error during plugin installation.
    Install,
    /// Error during plugin loading.
    Load,
    /// Error during plugin initialization.
    Init,
    /// Error during hook execution.
    Hook,
    /// Error during plugin disposal.
    Dispose,
}

impl std::fmt::Display for PluginErrorStage {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Install => write!(f, "install"),
            Self::Load => write!(f, "load"),
            Self::Init => write!(f, "init"),
            Self::Hook => write!(f, "hook"),
            Self::Dispose => write!(f, "dispose"),
        }
    }
}

/// A plugin error with context.
#[derive(Debug, Clone)]
pub struct PluginError {
    /// Plugin ID that encountered the error.
    pub plugin_id: String,
    /// Error stage.
    pub stage: PluginErrorStage,
    /// Error message.
    pub message: String,
    /// Timestamp (Unix millis).
    pub timestamp: u64,
}

impl std::fmt::Display for PluginError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "[{}] plugin `{}` {} error: {}",
            self.timestamp, self.plugin_id, self.stage, self.message
        )
    }
}

/// Error tracker for plugins.
#[derive(Debug, Default)]
pub struct PluginErrorTracker {
    /// Recent errors keyed by plugin ID.
    errors: HashMap<String, Vec<PluginError>>,
    /// Maximum errors to keep per plugin.
    max_errors: usize,
}

impl PluginErrorTracker {
    /// Create a new error tracker.
    pub fn new() -> Self {
        Self {
            errors: HashMap::new(),
            max_errors: 100,
        }
    }

    /// Record an error for a plugin.
    pub fn record(&mut self, error: PluginError) {
        let errors = self.errors.entry(error.plugin_id.clone()).or_default();
        errors.push(error);
        // Trim to max errors
        if errors.len() > self.max_errors {
            errors.drain(0..errors.len() - self.max_errors);
        }
    }

    /// Get recent errors for a plugin.
    pub fn get_errors(&self, plugin_id: &str) -> &[PluginError] {
        self.errors.get(plugin_id).map(|v| v.as_slice()).unwrap_or(&[])
    }

    /// Get all errors.
    pub fn all_errors(&self) -> Vec<&PluginError> {
        self.errors.values().flat_map(|v| v.iter()).collect()
    }

    /// Clear errors for a plugin.
    pub fn clear(&mut self, plugin_id: &str) {
        self.errors.remove(plugin_id);
    }

    /// Clear all errors.
    pub fn clear_all(&mut self) {
        self.errors.clear();
    }
}

/// Parsed package.json for a plugin.
#[derive(Debug, Clone, serde::Deserialize)]
pub struct PluginPackageJson {
    /// Package name.
    #[serde(default)]
    pub name: Option<String>,

    /// Package version.
    #[serde(default)]
    pub version: Option<String>,

    /// Package description.
    #[serde(default)]
    pub description: Option<String>,

    /// Main entrypoint (legacy).
    #[serde(default)]
    pub main: Option<String>,

    /// Exports map for conditional exports.
    #[serde(default)]
    pub exports: Option<serde_json::Value>,

    /// Engine requirements.
    #[serde(default)]
    pub engines: Option<PluginEngines>,

    /// OpenCode plugin ID (explicit override).
    #[serde(rename = "opencode", default)]
    pub opencode_id: Option<String>,

    /// Themes provided by this plugin.
    #[serde(rename = "oc-themes", default)]
    pub themes: Option<Vec<PluginTheme>>,
}

/// Engine requirements from package.json.
#[derive(Debug, Clone, serde::Deserialize)]
pub struct PluginEngines {
    /// Required opencode version (semver range).
    #[serde(default)]
    pub opencode: Option<String>,
}

/// A theme definition from a plugin's package.json.
///
/// # Source
/// Ported from `packages/opencode/src/plugin/shared.ts` `readPackageThemes()`.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct PluginTheme {
    /// Theme name.
    pub name: String,
    /// Display label.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub label: Option<String>,
    /// Theme file path (relative to plugin directory).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub path: Option<String>,
    /// Color scheme (light or dark).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub color_scheme: Option<String>,
}

/// Read themes from a plugin's package.json.
///
/// # Source
/// Ported from `packages/opencode/src/plugin/shared.ts` `readPackageThemes()`.
pub fn read_package_themes(pkg: &PluginPackageJson) -> Vec<PluginTheme> {
    pkg.themes.clone().unwrap_or_default()
}

/// Theme manager for tracking available and active themes.
#[derive(Debug, Default)]
pub struct ThemeManager {
    /// Available themes keyed by name.
    themes: HashMap<String, PluginTheme>,
    /// Currently active theme name.
    active: Option<String>,
}

impl ThemeManager {
    /// Create a new empty theme manager.
    pub fn new() -> Self {
        Self::default()
    }

    /// Register a theme from a plugin.
    pub fn register(&mut self, theme: PluginTheme) {
        self.themes.insert(theme.name.clone(), theme);
    }

    /// Register multiple themes from a plugin package.
    pub fn register_package_themes(&mut self, pkg: &PluginPackageJson) {
        for theme in read_package_themes(pkg) {
            self.register(theme);
        }
    }

    /// Set the active theme.
    pub fn set_active(&mut self, name: &str) -> bool {
        if self.themes.contains_key(name) {
            self.active = Some(name.to_string());
            true
        } else {
            false
        }
    }

    /// Get the active theme.
    pub fn active(&self) -> Option<&PluginTheme> {
        self.active.as_ref().and_then(|name| self.themes.get(name))
    }

    /// Get all available themes.
    pub fn all(&self) -> Vec<&PluginTheme> {
        self.themes.values().collect()
    }

    /// Get a theme by name.
    pub fn get(&self, name: &str) -> Option<&PluginTheme> {
        self.themes.get(name)
    }

    /// Remove a theme.
    pub fn remove(&mut self, name: &str) -> Option<PluginTheme> {
        if self.active.as_deref() == Some(name) {
            self.active = None;
        }
        self.themes.remove(name)
    }
}

/// Read and parse a plugin's package.json.
///
/// # Source
/// Ported from `packages/opencode/src/plugin/loader.ts` `readPluginPackage()`.
pub fn read_plugin_package(plugin_dir: &std::path::Path) -> Result<PluginPackageJson, PluginResolveError> {
    let pkg_path = plugin_dir.join("package.json");
    if !pkg_path.exists() {
        return Err(PluginResolveError::NoPackageJson {
            path: plugin_dir.display().to_string(),
        });
    }

    let content = std::fs::read_to_string(&pkg_path).map_err(|e| PluginResolveError::InvalidPackageJson {
        path: pkg_path.display().to_string(),
        source: e.into(),
    })?;

    let pkg: PluginPackageJson = serde_json::from_str(&content).map_err(|e| {
        PluginResolveError::InvalidPackageJson {
            path: pkg_path.display().to_string(),
            source: e,
        }
    })?;

    Ok(pkg)
}

/// Resolve the entrypoint path for a plugin kind (server or tui).
///
/// Checks exports map first, then falls back to main/index.
///
/// # Source
/// Ported from `packages/opencode/src/plugin/loader.ts` `resolvePackageEntrypoint()`.
pub fn resolve_package_entrypoint(
    pkg: &PluginPackageJson,
    kind: PluginKind,
) -> Result<String, PluginResolveError> {
    let kind_str = match kind {
        PluginKind::Server => "./server",
        PluginKind::Tui => "./tui",
    };

    // Try exports map first
    if let Some(ref exports) = pkg.exports {
        if let Some(entry) = exports.get(kind_str) {
            if let Some(s) = entry.as_str() {
                return Ok(s.to_string());
            }
            // Handle nested { default: "..." } format
            if let Some(default) = entry.get("default").and_then(|d| d.as_str()) {
                return Ok(default.to_string());
            }
        }
    }

    // Fallback to main
    if let Some(ref main) = pkg.main {
        return Ok(main.clone());
    }

    // Fallback to index.js
    Ok("index.js".to_string())
}

/// Resolve the plugin ID from package.json or explicit export.
///
/// Uses explicit `opencode` field if present, otherwise falls back to
/// the package name.
///
/// # Source
/// Ported from `packages/opencode/src/plugin/loader.ts` `resolvePluginId()`.
pub fn resolve_plugin_id(pkg: &PluginPackageJson) -> Option<String> {
    // Explicit opencode ID takes priority
    if let Some(ref id) = pkg.opencode_id {
        if !id.is_empty() {
            return Some(id.clone());
        }
    }

    // Fall back to package name
    pkg.name.clone()
}

/// Check if a plugin is compatible with the current opencode version.
///
/// # Source
/// Ported from `packages/opencode/src/plugin/loader.ts` `checkPluginCompatibility()`.
pub fn check_plugin_compatibility(
    pkg: &PluginPackageJson,
    current_version: &str,
) -> Result<(), PluginResolveError> {
    let required = match pkg.engines.as_ref().and_then(|e| e.opencode.as_ref()) {
        Some(req) if !req.is_empty() => req,
        _ => return Ok(()), // No requirement means compatible
    };

    // Simple version check: if required is a specific version, check equality
    // For now, just check if current version starts with the major version
    // A proper implementation would use a semver crate
    if required == "*" {
        return Ok(());
    }

    // Basic check: extract major versions and compare
    let required_major = required.split('.').next().unwrap_or("0");
    let current_major = current_version.split('.').next().unwrap_or("0");

    if required_major == current_major {
        Ok(())
    } else {
        Err(PluginResolveError::IncompatibleVersion {
            required: required.clone(),
            current: current_version.to_string(),
        })
    }
}

/// Resolve a file plugin's target path and read its entrypoint.
///
/// For file plugins, this resolves the actual path and reads package.json
/// to find the correct entrypoint file.
///
/// # Source
/// Ported from `packages/opencode/src/plugin/loader.ts` `resolve()`.
pub fn resolve_file_plugin(
    spec: &str,
    kind: PluginKind,
) -> Result<(std::path::PathBuf, PluginPackageJson), PluginResolveError> {
    let path_str = spec.strip_prefix("file://").unwrap_or(spec);
    let plugin_dir = std::path::PathBuf::from(path_str);

    if !plugin_dir.is_dir() {
        return Err(PluginResolveError::DirectoryNotFound {
            path: plugin_dir.display().to_string(),
        });
    }

    let pkg = read_plugin_package(&plugin_dir)?;
    Ok((plugin_dir, pkg))
}

// ── TUI plugin loading ────────────────────────────────────────────────

/// Errors during TUI plugin loading.
#[derive(Debug, thiserror::Error)]
pub enum TuiPluginError {
    /// Plugin directory not found.
    #[error("TUI plugin directory not found: {path}")]
    DirectoryNotFound { path: String },

    /// No TUI entrypoint found.
    #[error("no TUI entrypoint found in {path}")]
    NoEntrypoint { path: String },

    /// Failed to load TUI plugin.
    #[error("failed to load TUI plugin from {path}: {message}")]
    LoadFailed { path: String, message: String },
}

/// TUI-specific plugin configuration.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct TuiPluginConfig {
    /// Plugin ID.
    pub id: String,
    /// Plugin name.
    pub name: String,
    /// Path to the TUI plugin directory.
    pub path: std::path::PathBuf,
    /// Shell environment modifications.
    #[serde(default)]
    pub shell_env: HashMap<String, String>,
}

/// Load TUI plugins from configured directories.
///
/// Scans `.opencode/tui/` and registered plugin directories for TUI plugins.
///
/// # Source
/// Ported from `packages/opencode/src/plugin/tui/internal.ts` and `runtime.ts`.
pub fn load_tui_plugins(
    worktree: &std::path::Path,
    extra_dirs: &[std::path::PathBuf],
) -> Vec<TuiPluginConfig> {
    let mut plugins = Vec::new();

    // Scan .opencode/tui/ directory
    let tui_dir = worktree.join(".opencode").join("tui");
    if tui_dir.is_dir() {
        if let Ok(entries) = std::fs::read_dir(&tui_dir) {
            for entry in entries.filter_map(|e| e.ok()) {
                let path = entry.path();
                if path.is_dir() {
                    if let Ok(Some(config)) = read_tui_plugin_config(&path) {
                        plugins.push(config);
                    }
                }
            }
        }
    }

    // Scan extra directories
    for dir in extra_dirs {
        if dir.is_dir() {
            if let Ok(entries) = std::fs::read_dir(dir) {
                for entry in entries.filter_map(|e| e.ok()) {
                    let path = entry.path();
                    if path.is_dir() {
                        if let Ok(Some(config)) = read_tui_plugin_config(&path) {
                            plugins.push(config);
                        }
                    }
                }
            }
        }
    }

    plugins
}

/// Read TUI plugin config from a directory.
fn read_tui_plugin_config(
    dir: &std::path::Path,
) -> Result<Option<TuiPluginConfig>, std::io::Error> {
    let pkg_path = dir.join("package.json");
    if !pkg_path.exists() {
        return Ok(None);
    }

    let content = std::fs::read_to_string(&pkg_path)?;
    let pkg: PluginPackageJson = serde_json::from_str(&content)
        .map_err(|e| std::io::Error::new(std::io::ErrorKind::InvalidData, e))?;

    let name = pkg.name.unwrap_or_else(|| {
        dir.file_name()
            .and_then(|n| n.to_str())
            .unwrap_or("unknown")
            .to_string()
    });

    // Check if this plugin has a TUI export
    if let Some(ref exports) = pkg.exports {
        if exports.get("./tui").is_some() || exports.get(".").is_some() {
            return Ok(Some(TuiPluginConfig {
                id: format!("tui-{}", name),
                name,
                path: dir.to_path_buf(),
                shell_env: HashMap::new(),
            }));
        }
    }

    // Fallback to main if it exists
    if pkg.main.is_some() {
        return Ok(Some(TuiPluginConfig {
            id: format!("tui-{}", name),
            name,
            path: dir.to_path_buf(),
            shell_env: HashMap::new(),
        }));
    }

    Ok(None)
}

/// Shell environment plugin for PTY integration.
///
/// This plugin modifies the shell environment for terminal sessions.
///
/// # Source
/// Ported from `packages/opencode/src/plugin/pty-environment.ts`.
pub struct PtyEnvironmentPlugin {
    /// Environment variables to inject.
    env: HashMap<String, String>,
}

impl PtyEnvironmentPlugin {
    /// Create a new PTY environment plugin.
    pub fn new() -> Self {
        Self {
            env: HashMap::new(),
        }
    }

    /// Add an environment variable.
    pub fn with_env(mut self, key: impl Into<String>, value: impl Into<String>) -> Self {
        self.env.insert(key.into(), value.into());
        self
    }

    /// Get the environment variables.
    pub fn env(&self) -> &HashMap<String, String> {
        &self.env
    }
}

impl Default for PtyEnvironmentPlugin {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait::async_trait]
impl PluginHooks for PtyEnvironmentPlugin {
    async fn on_shell_env(&self, env: &mut HashMap<String, String>) {
        for (key, value) in &self.env {
            env.insert(key.clone(), value.clone());
        }
    }
}

// ── Plugin installation ───────────────────────────────────────────────

/// Errors during plugin installation or config patching.
#[derive(Debug, thiserror::Error)]
pub enum PluginInstallError {
    /// Failed to resolve plugin target.
    #[error("failed to resolve plugin target: {spec}: {source}")]
    ResolveFailed {
        spec: String,
        source: PluginResolveError,
    },

    /// Failed to read config file.
    #[error("failed to read config file {path}: {source}")]
    ReadConfig {
        path: String,
        source: std::io::Error,
    },

    /// Config file has invalid JSON.
    #[error("invalid JSON in config file {path}: {source}")]
    InvalidConfig {
        path: String,
        source: serde_json::Error,
    },

    /// Failed to write config file.
    #[error("failed to write config file {path}: {source}")]
    WriteConfig {
        path: String,
        source: std::io::Error,
    },

    /// No targets found in plugin manifest.
    #[error("no targets found in plugin manifest at {path}")]
    NoTargets { path: String },
}

/// Target type for a plugin (server or tui).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PluginTarget {
    /// Target kind.
    pub kind: PluginKind,
    /// Optional configuration options for this target.
    pub opts: Option<serde_json::Value>,
}

/// Result of installing a plugin.
#[derive(Debug)]
pub struct InstallResult {
    /// Resolved target path.
    pub target: String,
}

/// Read a plugin's manifest and determine its targets.
///
/// # Source
/// Ported from `packages/opencode/src/plugin/install.ts` `readPluginManifest()`.
pub fn read_plugin_manifest(
    target_dir: &std::path::Path,
) -> Result<Vec<PluginTarget>, PluginInstallError> {
    let pkg = read_plugin_package(target_dir).map_err(|e| PluginInstallError::ResolveFailed {
        spec: target_dir.display().to_string(),
        source: e,
    })?;

    let mut targets = Vec::new();

    // Check for server target
    if let Some(entrypoint) = resolve_package_entrypoint(&pkg, PluginKind::Server).ok() {
        if !entrypoint.is_empty() {
            targets.push(PluginTarget {
                kind: PluginKind::Server,
                opts: None,
            });
        }
    } else if pkg.main.is_some() {
        // Fallback to main if no explicit server export
        targets.push(PluginTarget {
            kind: PluginKind::Server,
            opts: None,
        });
    }

    // Check for tui target
    if let Ok(entrypoint) = resolve_package_entrypoint(&pkg, PluginKind::Tui) {
        if !entrypoint.is_empty() {
            targets.push(PluginTarget {
                kind: PluginKind::Tui,
                opts: None,
            });
        }
    }

    if targets.is_empty() {
        return Err(PluginInstallError::NoTargets {
            path: target_dir.display().to_string(),
        });
    }

    Ok(targets)
}

/// Read the plugin list from a JSON config value.
fn plugin_list_from_json(data: &serde_json::Value) -> Option<&Vec<serde_json::Value>> {
    data.get("plugin")
        .and_then(|v| v.as_array())
}

/// Get the plugin spec from a list item.
fn plugin_spec_from_item(item: &serde_json::Value) -> Option<String> {
    if let Some(s) = item.as_str() {
        return Some(s.to_string());
    }
    if let Some(arr) = item.as_array() {
        if let Some(s) = arr.first().and_then(|v| v.as_str()) {
            return Some(s.to_string());
        }
    }
    None
}

/// Patch the plugin list in a JSON config string.
///
/// Returns the updated JSON string and whether a change was made.
fn patch_plugin_list(
    text: &str,
    list: Option<&Vec<serde_json::Value>>,
    spec: &str,
    force: bool,
) -> Result<(String, bool), PluginInstallError> {
    let parsed: serde_json::Value =
        serde_json::from_str(text).map_err(|e| PluginInstallError::InvalidConfig {
            path: "config".to_string(),
            source: e,
        })?;

    let pkg = parse_specifier(spec).pkg;
    let items = list.unwrap_or(&Vec::new());

    // Check for duplicates
    let duplicates: Vec<(usize, &serde_json::Value)> = items
        .iter()
        .enumerate()
        .filter(|(_, item)| {
            if let Some(item_spec) = plugin_spec_from_item(item) {
                if item_spec == spec {
                    return true;
                }
                // Check if same package (for npm plugins)
                if !item_spec.starts_with("file://") {
                    return parse_specifier(&item_spec).pkg == pkg;
                }
            }
            false
        })
        .collect();

    if duplicates.is_empty() {
        // No duplicate - add to list
        let mut new_list = items.clone();
        new_list.push(serde_json::Value::String(spec.to_string()));

        let mut new_config = parsed.clone();
        new_config["plugin"] = serde_json::Value::Array(new_list);

        let output = serde_json::to_string_pretty(&new_config).map_err(|e| {
            PluginInstallError::InvalidConfig {
                path: "config".to_string(),
                source: e,
            }
        })?;
        return Ok((output, true));
    }

    if !force {
        // Duplicate exists and not forced - no change
        return Ok((text.to_string(), false));
    }

    // Force update - keep first duplicate, remove others
    let keep_idx = duplicates[0].0;

    // If there's more than one duplicate, or the spec differs, update
    if duplicates.len() > 1 || plugin_spec_from_item(duplicates[0].1) != Some(spec.to_string()) {
        let mut new_config = parsed.clone();
        if let Some(plugin_array) = new_config.get_mut("plugin").and_then(|v| v.as_array_mut()) {
            // Update the kept entry
            plugin_array[keep_idx] = serde_json::Value::String(spec.to_string());

            // Remove other duplicates in reverse order
            for &(idx, _) in duplicates.iter().skip(1).rev() {
                plugin_array.remove(idx);
            }
        }

        let output = serde_json::to_string_pretty(&new_config).map_err(|e| {
            PluginInstallError::InvalidConfig {
                path: "config".to_string(),
                source: e,
            }
        })?;
        return Ok((output, true));
    }

    // Same spec already exists - no change
    Ok((text.to_string(), false))
}

/// Patch a config file to add or update a plugin entry.
///
/// # Source
/// Ported from `packages/opencode/src/plugin/install.ts` `patchPluginConfig()`.
pub fn patch_plugin_config(
    config_dir: &std::path::Path,
    spec: &str,
    targets: &[PluginTarget],
    force: bool,
) -> Result<Vec<(PluginKind, String)>, PluginInstallError> {
    let mut results = Vec::new();

    for target in targets {
        let config_name = match target.kind {
            PluginKind::Server => "opencode.json",
            PluginKind::Tui => "tui.json",
        };

        let config_path = config_dir.join(config_name);

        // Read existing config or start with empty object
        let text = if config_path.exists() {
            std::fs::read_to_string(&config_path).map_err(|e| PluginInstallError::ReadConfig {
                path: config_path.display().to_string(),
                source: e,
            })?
        } else {
            "{}".to_string()
        };

        let parsed: serde_json::Value =
            serde_json::from_str(&text).map_err(|e| PluginInstallError::InvalidConfig {
                path: config_path.display().to_string(),
                source: e,
            })?;

        let list = plugin_list_from_json(&parsed);
        let (new_text, changed) = patch_plugin_list(&text, list, spec, force)?;

        if changed {
            std::fs::write(&config_path, &new_text).map_err(|e| {
                PluginInstallError::WriteConfig {
                    path: config_path.display().to_string(),
                    source: e,
                }
            })?;
        }

        results.push((target.kind.clone(), config_path.display().to_string()));
    }

    Ok(results)
}

// ── Config-driven plugins ─────────────────────────────────────────────

/// A plugin definition from config (opencode.json).
///
/// Users can define agents, commands, and skills in config without writing
/// a separate plugin package.
///
/// # Source
/// Ported from `packages/core/src/config/plugin/agent.ts`, `command.ts`, `skill.ts`.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct ConfigPluginDefinition {
    /// Plugin type (agent, command, skill, provider, reference).
    #[serde(rename = "type")]
    pub plugin_type: String,
    /// Plugin name/ID.
    pub name: String,
    /// Optional description.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
    /// Plugin-specific configuration.
    #[serde(default)]
    pub config: serde_json::Value,
}

/// Read config-driven plugin definitions from a config file.
///
/// Looks for `plugin` array in the config and returns definitions.
pub fn read_config_plugins(
    config_path: &std::path::Path,
) -> Result<Vec<ConfigPluginDefinition>, PluginInstallError> {
    if !config_path.exists() {
        return Ok(Vec::new());
    }

    let text = std::fs::read_to_string(config_path).map_err(|e| PluginInstallError::ReadConfig {
        path: config_path.display().to_string(),
        source: e,
    })?;

    let config: serde_json::Value =
        serde_json::from_str(&text).map_err(|e| PluginInstallError::InvalidConfig {
            path: config_path.display().to_string(),
            source: e,
        })?;

    let mut definitions = Vec::new();

    // Check for agent definitions
    if let Some(agents) = config.get("agent").and_then(|v| v.as_object()) {
        for (name, agent_config) in agents {
            definitions.push(ConfigPluginDefinition {
                plugin_type: "agent".to_string(),
                name: name.clone(),
                description: agent_config
                    .get("description")
                    .and_then(|v| v.as_str())
                    .map(String::from),
                config: agent_config.clone(),
            });
        }
    }

    // Check for command definitions
    if let Some(commands) = config.get("command").and_then(|v| v.as_object()) {
        for (name, cmd_config) in commands {
            definitions.push(ConfigPluginDefinition {
                plugin_type: "command".to_string(),
                name: name.clone(),
                description: cmd_config
                    .get("description")
                    .and_then(|v| v.as_str())
                    .map(String::from),
                config: cmd_config.clone(),
            });
        }
    }

    // Check for skill definitions
    if let Some(skills) = config.get("skills").and_then(|v| v.as_object()) {
        if let Some(paths) = skills.get("paths").and_then(|v| v.as_array()) {
            for path_value in paths {
                if let Some(path_str) = path_value.as_str() {
                    definitions.push(ConfigPluginDefinition {
                        plugin_type: "skill_path".to_string(),
                        name: path_str.to_string(),
                        description: Some(format!("Skill path: {}", path_str)),
                        config: serde_json::Value::Null,
                    });
                }
            }
        }
    }

    // Check for provider definitions
    if let Some(providers) = config.get("provider").and_then(|v| v.as_object()) {
        for (name, provider_config) in providers {
            definitions.push(ConfigPluginDefinition {
                plugin_type: "provider".to_string(),
                name: name.clone(),
                description: provider_config
                    .get("name")
                    .and_then(|v| v.as_str())
                    .map(String::from),
                config: provider_config.clone(),
            });
        }
    }

    Ok(definitions)
}

/// Register config-driven plugins in a PluginManager.
///
/// Creates Plugin entries for each config definition and registers them.
pub fn register_config_plugins(
    manager: &mut PluginManager,
    config_path: &std::path::Path,
) -> Result<usize, PluginInstallError> {
    let definitions = read_config_plugins(config_path)?;
    let count = definitions.len();

    for def in &definitions {
        let mut plugin = Plugin::new(
            format!("config-{}-{}", def.plugin_type, def.name),
            &def.name,
            PluginSource::File,
        )
        .with_spec(config_path.display().to_string());

        // Add appropriate hooks based on plugin type
        match def.plugin_type.as_str() {
            "agent" => {
                plugin = plugin
                    .with_hook(PluginHook::ChatMessage)
                    .with_hook(PluginHook::Tool);
            }
            "command" => {
                plugin = plugin.with_hook(PluginHook::CommandExecuteBefore);
            }
            "skill_path" | "skill" => {
                plugin = plugin.with_hook(PluginHook::ChatMessage);
            }
            "provider" => {
                plugin = plugin
                    .with_hook(PluginHook::Auth)
                    .with_hook(PluginHook::Provider);
            }
            _ => {}
        }

        manager.register(plugin);
    }

    Ok(count)
}

/// Get the current time in milliseconds since Unix epoch.
fn current_time_millis() -> u64 {
    #[allow(clippy::cast_sign_loss)]
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_millis() as u64)
        .unwrap_or(0)
}

// ── Tests ─────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    // ── Spec parsing tests ─────────────────────────────────────────

    #[test]
    fn test_parse_bare_package() {
        let result = parse_specifier("express");
        assert_eq!(result.pkg, "express");
        assert_eq!(result.version, "latest");
    }

    #[test]
    fn test_parse_package_with_version() {
        let result = parse_specifier("express@4.18.2");
        assert_eq!(result.pkg, "express");
        assert_eq!(result.version, "4.18.2");
    }

    #[test]
    fn test_parse_scoped_package() {
        let result = parse_specifier("@anthropic/claude-code");
        assert_eq!(result.pkg, "@anthropic/claude-code");
        assert_eq!(result.version, "latest");
    }

    #[test]
    fn test_parse_scoped_package_with_version() {
        let result = parse_specifier("@anthropic/claude-code@1.2.3");
        assert_eq!(result.pkg, "@anthropic/claude-code");
        assert_eq!(result.version, "1.2.3");
    }

    #[test]
    fn test_parse_npm_alias() {
        let result = parse_specifier("npm:express@4.18.2");
        assert_eq!(result.pkg, "express");
        assert_eq!(result.version, "4.18.2");
    }

    #[test]
    fn test_parse_npm_alias_latest() {
        let result = parse_specifier("npm:express@*");
        assert_eq!(result.pkg, "express");
        assert_eq!(result.version, "latest");
    }

    #[test]
    fn test_parse_empty_spec() {
        let result = parse_specifier("");
        assert_eq!(result.pkg, "");
        assert_eq!(result.version, "");
    }

    // ── Plugin source tests ────────────────────────────────────────

    #[test]
    fn test_plugin_source_npm() {
        assert_eq!(plugin_source("express"), PluginSource::Npm);
        assert_eq!(plugin_source("@scope/pkg"), PluginSource::Npm);
    }

    #[test]
    fn test_plugin_source_file() {
        assert_eq!(plugin_source("./local-plugin"), PluginSource::File);
        assert_eq!(plugin_source("/absolute/path"), PluginSource::File);
        assert_eq!(plugin_source("file:///path/to/plugin"), PluginSource::File);
    }

    #[test]
    fn test_is_path_plugin_spec() {
        assert!(is_path_plugin_spec("./local"));
        assert!(is_path_plugin_spec("../parent"));
        assert!(is_path_plugin_spec("/absolute/path"));
        assert!(is_path_plugin_spec("file:///path"));
        assert!(!is_path_plugin_spec("express"));
        assert!(!is_path_plugin_spec("@scope/pkg"));
    }

    #[test]
    fn test_is_deprecated_plugin() {
        assert!(is_deprecated_plugin("opencode-openai-codex-auth"));
        assert!(is_deprecated_plugin("opencode-copilot-auth"));
        assert!(!is_deprecated_plugin("express"));
    }

    // ── Plugin hook tests ──────────────────────────────────────────

    #[test]
    fn test_hook_as_str_roundtrip() {
        let hooks = [
            PluginHook::ExperimentalTextComplete,
            PluginHook::ExperimentalSessionCompacting,
            PluginHook::ExperimentalChatMessagesTransform,
            PluginHook::Event,
            PluginHook::Config,
        ];
        for hook in &hooks {
            let s = hook.as_str();
            let parsed = PluginHook::from_str(s);
            assert_eq!(parsed, Some(hook.clone()));
        }
    }

    #[test]
    fn test_hook_from_str_invalid() {
        assert_eq!(PluginHook::from_str("nonexistent.hook"), None);
        assert_eq!(PluginHook::from_str(""), None);
    }

    // ── Plugin manager tests ───────────────────────────────────────

    #[test]
    fn test_plugin_manager_register_and_list() {
        let mut manager = PluginManager::new();
        assert_eq!(manager.count(), 0);
        assert!(manager.list().is_empty());

        let plugin = Plugin::new("test-plugin", "Test Plugin", PluginSource::File)
            .with_spec("./test-plugin");
        manager.register(plugin);
        assert_eq!(manager.count(), 1);
        assert_eq!(manager.list()[0].id, "test-plugin");
    }

    #[test]
    fn test_plugin_manager_unregister() {
        let mut manager = PluginManager::new();
        let plugin = Plugin::new("p1", "Plugin One", PluginSource::Npm);
        manager.register(plugin);

        let removed = manager.unregister("p1");
        assert!(removed.is_some());
        assert_eq!(removed.unwrap().id, "p1");
        assert_eq!(manager.count(), 0);
        assert!(manager.unregister("nonexistent").is_none());
    }

    #[test]
    fn test_plugin_manager_hooks() {
        let mut manager = PluginManager::new();
        let plugin = Plugin::new("hook-plugin", "Hook Plugin", PluginSource::File)
            .with_hook(PluginHook::ExperimentalTextComplete)
            .with_hook(PluginHook::Config);
        manager.register(plugin);

        assert!(manager.has_hook(&PluginHook::ExperimentalTextComplete));
        assert!(manager.has_hook(&PluginHook::Config));
        assert!(!manager.has_hook(&PluginHook::Event));

        let text_plugins = manager.plugins_for_hook(&PluginHook::ExperimentalTextComplete);
        assert_eq!(text_plugins.len(), 1);
        assert_eq!(text_plugins[0].id, "hook-plugin");
    }

    #[test]
    fn test_plugin_manager_duplicate_replace() {
        let mut manager = PluginManager::new();
        let p1 = Plugin::new("same-id", "Version 1", PluginSource::Npm);
        let p2 = Plugin::new("same-id", "Version 2", PluginSource::Npm);

        let prev = manager.register(p1);
        assert!(prev.is_none());

        let prev = manager.register(p2);
        assert!(prev.is_some());
        assert_eq!(prev.unwrap().name, "Version 1");
        assert_eq!(manager.count(), 1);
        assert_eq!(manager.get("same-id").unwrap().name, "Version 2");
    }

    // ── Plugin meta tests ─────────────────────────────────────────────

    #[test]
    fn test_plugin_meta_first_touch() {
        let mut manager = PluginManager::new();
        let (state, entry) = manager.touch_meta(
            "my-plugin",
            PluginSource::File,
            "file:///my-plugin",
            "/path/to/my-plugin",
            None,
            None,
            Some(1_700_000_000_000),
        );

        assert_eq!(state, PluginState::First);
        assert_eq!(entry.load_count, 1);
        assert_eq!(entry.first_time, entry.last_time);
    }

    #[test]
    fn test_plugin_meta_same_touch() {
        let mut manager = PluginManager::new();
        manager.touch_meta(
            "my-plugin",
            PluginSource::File,
            "file:///my-plugin",
            "/path/to/my-plugin",
            None,
            None,
            Some(1_700_000_000_000),
        );

        let (state, entry) = manager.touch_meta(
            "my-plugin",
            PluginSource::File,
            "file:///my-plugin",
            "/path/to/my-plugin",
            None,
            None,
            Some(1_700_000_000_000),
        );

        assert_eq!(state, PluginState::Same);
        assert_eq!(entry.load_count, 2);
    }

    #[test]
    fn test_plugin_meta_updated_touch() {
        let mut manager = PluginManager::new();
        manager.touch_meta(
            "my-plugin",
            PluginSource::File,
            "file:///my-plugin",
            "/path/to/my-plugin",
            None,
            None,
            Some(1_700_000_000_000),
        );

        let (state, entry) = manager.touch_meta(
            "my-plugin",
            PluginSource::File,
            "file:///my-plugin",
            "/path/to/my-plugin",
            None,
            None,
            Some(1_700_000_000_001), // different mtime
        );

        assert_eq!(state, PluginState::Updated);
        assert_eq!(entry.load_count, 2);
        assert!(entry.time_changed > entry.first_time);
    }

    #[test]
    fn test_fingerprint_file_plugin() {
        let fp = compute_fingerprint(
            PluginSource::File,
            "/path/to/plugin",
            None,
            None,
            Some(1_700_000_000_000),
        );
        assert_eq!(fp, "/path/to/plugin|1700000000000");
    }

    #[test]
    fn test_fingerprint_npm_plugin() {
        let fp = compute_fingerprint(
            PluginSource::Npm,
            "/node_modules/express",
            Some("4.18.2"),
            Some("4.18.2"),
            None,
        );
        assert_eq!(fp, "/node_modules/express|4.18.2|4.18.2");
    }

    #[test]
    fn test_plugin_manager_clear() {
        let mut manager = PluginManager::new();
        manager.register(Plugin::new("p1", "One", PluginSource::Npm));
        manager.touch_meta("p1", PluginSource::Npm, "p1", "/t", None, None, None);
        manager.clear();

        assert_eq!(manager.count(), 0);
        assert!(manager.all_meta().is_empty());
    }

    #[test]
    fn test_plugin_hook_as_str_values() {
        assert_eq!(
            PluginHook::ExperimentalTextComplete.as_str(),
            "experimental.text.complete"
        );
        assert_eq!(
            PluginHook::ExperimentalSessionCompacting.as_str(),
            "experimental.session.compacting"
        );
        assert_eq!(
            PluginHook::ExperimentalChatMessagesTransform.as_str(),
            "experimental.chat.messages.transform"
        );
        assert_eq!(PluginHook::Event.as_str(), "event");
        assert_eq!(PluginHook::Config.as_str(), "config");
    }

    #[test]
    fn test_plugin_hook_serde() {
        let hook = PluginHook::ExperimentalTextComplete;
        let json = serde_json::to_string(&hook).unwrap();
        assert_eq!(json, "\"experimental.text.complete\"");
        let parsed: PluginHook = serde_json::from_str(&json).unwrap();
        assert_eq!(parsed, PluginHook::ExperimentalTextComplete);
    }

    // ── Additional spec parsing tests ──────────────────────────────

    #[test]
    fn test_parse_specifier_with_at_sign_in_name() {
        let result = parse_specifier("@scope/pkg");
        assert_eq!(result.pkg, "@scope/pkg");
        assert_eq!(result.version, "latest");
    }

    #[test]
    fn test_parse_specifier_npm_alias_bare() {
        let result = parse_specifier("npm:express");
        assert_eq!(result.pkg, "express");
        assert_eq!(result.version, "latest");
    }

    // ── Fingerprint edge-case tests ────────────────────────────────

    #[test]
    fn test_fingerprint_file_plugin_no_modified() {
        let fp = compute_fingerprint(PluginSource::File, "/path/to/plugin", None, None, None);
        assert_eq!(fp, "/path/to/plugin|");
    }

    #[test]
    fn test_fingerprint_npm_plugin_no_version() {
        let fp = compute_fingerprint(
            PluginSource::Npm,
            "/node_modules/express",
            Some("4.18.2"),
            None,
            None,
        );
        assert_eq!(fp, "/node_modules/express|4.18.2|");
    }

    #[test]
    fn test_fingerprint_npm_plugin_no_requested() {
        let fp = compute_fingerprint(
            PluginSource::Npm,
            "/node_modules/express",
            None,
            Some("4.18.2"),
            None,
        );
        assert_eq!(fp, "/node_modules/express||4.18.2");
    }

    #[test]
    fn test_compute_fingerprint_stability() {
        let fp1 = compute_fingerprint(
            PluginSource::Npm,
            "/node_modules/express",
            Some("4.18.2"),
            Some("4.18.2"),
            None,
        );
        let fp2 = compute_fingerprint(
            PluginSource::Npm,
            "/node_modules/express",
            Some("4.18.2"),
            Some("4.18.2"),
            None,
        );
        assert_eq!(fp1, fp2);
    }

    // ── PluginManager load/install tests ──────────────────────────

    #[test]
    fn test_plugin_manager_load_file() {
        let mut manager = PluginManager::new();
        let plugin = manager.load("./my-plugin").expect("load file plugin");
        assert_eq!(plugin.spec, "./my-plugin");
        assert_eq!(plugin.source, PluginSource::File);
        assert!(!plugin.id.is_empty());
    }

    #[test]
    fn test_plugin_manager_load_npm() {
        let mut manager = PluginManager::new();
        let plugin = manager.load("express").expect("load npm plugin");
        assert_eq!(plugin.spec, "express");
        assert_eq!(plugin.source, PluginSource::Npm);
        assert_eq!(plugin.id, "express");
        assert_eq!(plugin.version.as_deref(), Some("latest"));
    }

    #[test]
    fn test_plugin_manager_load_deprecated() {
        let mut manager = PluginManager::new();
        let result = manager.load("opencode-openai-codex-auth");
        assert!(result.is_err());
        match result {
            Err(PluginLoadError::DeprecatedPlugin { spec }) => {
                assert!(spec.contains("opencode-openai-codex-auth"));
            }
            _ => panic!("expected DeprecatedPlugin error"),
        }
    }

    #[test]
    fn test_plugin_manager_load_empty() {
        let mut manager = PluginManager::new();
        let result = manager.load("");
        assert!(result.is_err());
        match result {
            Err(PluginLoadError::EmptySpec) => {}
            _ => panic!("expected EmptySpec error"),
        }
    }

    #[test]
    fn test_plugin_manager_install_validate() {
        let manager = PluginManager::new();

        // Valid spec
        let result = manager.install_validate("express@4.18.2");
        assert!(result.is_ok());
        let parsed = result.expect("valid spec");
        assert_eq!(parsed.pkg, "express");
        assert_eq!(parsed.version, "4.18.2");

        // Empty spec
        let result = manager.install_validate("");
        assert!(result.is_err());
        assert!(matches!(result, Err(PluginLoadError::EmptySpec)));

        // Deprecated spec
        let result = manager.install_validate("opencode-copilot-auth");
        assert!(result.is_err());
        assert!(matches!(
            result,
            Err(PluginLoadError::DeprecatedPlugin { .. })
        ));
    }

    // ── Serde and Display round-trip tests ─────────────────────────

    #[test]
    fn test_plugin_hook_all_variants_serde() {
        let hooks = [
            PluginHook::ExperimentalTextComplete,
            PluginHook::ExperimentalSessionCompacting,
            PluginHook::ExperimentalChatMessagesTransform,
            PluginHook::Event,
            PluginHook::Config,
        ];
        for hook in &hooks {
            let json = serde_json::to_string(hook).expect("serialize hook");
            let parsed: PluginHook = serde_json::from_str(&json).expect("deserialize hook");
            assert_eq!(&parsed, hook, "roundtrip failed for {:?}", hook);
        }
    }

    #[test]
    fn test_plugin_source_display() {
        assert_eq!(format!("{}", PluginSource::File), "file");
        assert_eq!(format!("{}", PluginSource::Npm), "npm");
    }

    #[test]
    fn test_plugin_kind_display() {
        assert_eq!(format!("{}", PluginKind::Server), "server");
        assert_eq!(format!("{}", PluginKind::Tui), "tui");
    }

    #[test]
    fn test_plugin_state_display() {
        assert_eq!(format!("{}", PluginState::First), "first");
        assert_eq!(format!("{}", PluginState::Updated), "updated");
        assert_eq!(format!("{}", PluginState::Same), "same");
    }

    // ── Provider plugin tests ──────────────────────────────────────

    #[test]
    fn test_provider_plugin_registry_empty() {
        let registry = ProviderPluginRegistry::new();
        assert_eq!(registry.count(), 0);
        assert!(registry.get("nonexistent").is_none());
    }

    #[test]
    fn test_closure_provider_plugin() {
        use std::sync::Arc;

        let plugin = ClosureProviderPlugin::new("test", "Test Plugin");
        assert_eq!(plugin.id(), "test");
        assert_eq!(plugin.name(), "Test Plugin");

        let mut registry = ProviderPluginRegistry::new();
        registry.register(Arc::new(plugin));
        assert_eq!(registry.count(), 1);
        assert!(registry.get("test").is_some());
    }

    #[test]
    fn test_custom_provider_config_build_models() {
        let mut models = HashMap::new();
        models.insert(
            "my-model".to_string(),
            CustomModelConfig {
                name: "My Model".to_string(),
                context: 64_000,
                output: 4_096,
                reasoning: true,
                image_input: false,
                family: Some("custom".to_string()),
            },
        );

        let config = CustomProviderConfig {
            name: "My Provider".to_string(),
            env: vec!["MY_API_KEY".to_string()],
            base_url: Some("https://api.example.com/v1".to_string()),
            models,
            headers: HashMap::new(),
            enabled: true,
        };

        let models = config.build_models("my-provider", "https://api.example.com/v1");
        assert_eq!(models.len(), 1);
        assert_eq!(models[0].id, "my-model");
        assert_eq!(models[0].name, "My Model");
        assert_eq!(models[0].limit.context, 64_000);
        assert_eq!(models[0].limit.output, 4_096);
        assert!(models[0].capabilities.reasoning);
        assert!(!models[0].capabilities.input.image);
    }

    #[test]
    fn test_custom_provider_config_defaults() {
        let config: CustomProviderConfig = serde_json::from_value(serde_json::json!({
            "name": "Minimal Provider",
            "env": ["API_KEY"],
            "models": {
                "model-a": { "name": "Model A" }
            }
        }))
        .unwrap();

        assert!(config.enabled);
        assert!(config.base_url.is_none());
        assert!(config.headers.is_empty());

        let models = config.build_models("minimal", "https://default.api");
        assert_eq!(models.len(), 1);
        assert_eq!(models[0].limit.context, 128_000);
        assert_eq!(models[0].limit.output, 16_384);
        assert!(!models[0].capabilities.reasoning);
    }

    // ── Plugin file resolution tests ──────────────────────────────

    #[test]
    fn test_read_plugin_package_invalid_json() {
        let tmp = std::env::temp_dir().join("rustcode-plugin-test-invalid");
        let _ = std::fs::remove_dir_all(&tmp);
        std::fs::create_dir_all(&tmp).expect("create dirs");
        std::fs::write(tmp.join("package.json"), "{ invalid json }").expect("write");

        let result = read_plugin_package(&tmp);
        let _ = std::fs::remove_dir_all(&tmp);

        assert!(result.is_err());
        assert!(matches!(
            result.unwrap_err(),
            PluginResolveError::InvalidPackageJson { .. }
        ));
    }

    #[test]
    fn test_resolve_package_entrypoint_server() {
        let pkg = PluginPackageJson {
            name: Some("test-plugin".to_string()),
            version: Some("1.0.0".to_string()),
            description: None,
            main: Some("index.js".to_string()),
            exports: Some(serde_json::json!({
                "./server": "server.js",
                "./tui": "tui.js"
            })),
            engines: None,
            opencode_id: None,
        };

        let entry = resolve_package_entrypoint(&pkg, PluginKind::Server).unwrap();
        assert_eq!(entry, "server.js");
    }

    #[test]
    fn test_resolve_package_entrypoint_tui() {
        let pkg = PluginPackageJson {
            name: Some("test-plugin".to_string()),
            version: Some("1.0.0".to_string()),
            description: None,
            main: Some("index.js".to_string()),
            exports: Some(serde_json::json!({
                "./server": "server.js",
                "./tui": "tui.js"
            })),
            engines: None,
            opencode_id: None,
        };

        let entry = resolve_package_entrypoint(&pkg, PluginKind::Tui).unwrap();
        assert_eq!(entry, "tui.js");
    }

    #[test]
    fn test_resolve_package_entrypoint_fallback_to_main() {
        let pkg = PluginPackageJson {
            name: Some("test-plugin".to_string()),
            version: Some("1.0.0".to_string()),
            description: None,
            main: Some("main.js".to_string()),
            exports: None,
            engines: None,
            opencode_id: None,
        };

        let entry = resolve_package_entrypoint(&pkg, PluginKind::Server).unwrap();
        assert_eq!(entry, "main.js");
    }

    #[test]
    fn test_resolve_package_entrypoint_fallback_to_index() {
        let pkg = PluginPackageJson {
            name: Some("test-plugin".to_string()),
            version: Some("1.0.0".to_string()),
            description: None,
            main: None,
            exports: None,
            engines: None,
            opencode_id: None,
        };

        let entry = resolve_package_entrypoint(&pkg, PluginKind::Server).unwrap();
        assert_eq!(entry, "index.js");
    }

    #[test]
    fn test_resolve_plugin_id_explicit() {
        let pkg = PluginPackageJson {
            name: Some("package-name".to_string()),
            version: None,
            description: None,
            main: None,
            exports: None,
            engines: None,
            opencode_id: Some("explicit-id".to_string()),
        };

        let id = resolve_plugin_id(&pkg).unwrap();
        assert_eq!(id, "explicit-id");
    }

    #[test]
    fn test_resolve_plugin_id_fallback_to_name() {
        let pkg = PluginPackageJson {
            name: Some("my-plugin".to_string()),
            version: None,
            description: None,
            main: None,
            exports: None,
            engines: None,
            opencode_id: None,
        };

        let id = resolve_plugin_id(&pkg).unwrap();
        assert_eq!(id, "my-plugin");
    }

    #[test]
    fn test_resolve_plugin_id_no_name() {
        let pkg = PluginPackageJson {
            name: None,
            version: None,
            description: None,
            main: None,
            exports: None,
            engines: None,
            opencode_id: None,
        };

        let id = resolve_plugin_id(&pkg);
        assert!(id.is_none());
    }

    #[test]
    fn test_check_plugin_compatibility_no_requirement() {
        let pkg = PluginPackageJson {
            name: None,
            version: None,
            description: None,
            main: None,
            exports: None,
            engines: None,
            opencode_id: None,
        };

        let result = check_plugin_compatibility(&pkg, "0.1.0");
        assert!(result.is_ok());
    }

    #[test]
    fn test_check_plugin_compatibility_wildcard() {
        let pkg = PluginPackageJson {
            name: None,
            version: None,
            description: None,
            main: None,
            exports: None,
            engines: Some(PluginEngines {
                opencode: Some("*".to_string()),
            }),
            opencode_id: None,
        };

        let result = check_plugin_compatibility(&pkg, "0.1.0");
        assert!(result.is_ok());
    }

    #[test]
    fn test_check_plugin_compatibility_matching_major() {
        let pkg = PluginPackageJson {
            name: None,
            version: None,
            description: None,
            main: None,
            exports: None,
            engines: Some(PluginEngines {
                opencode: Some("0.2.0".to_string()),
            }),
            opencode_id: None,
        };

        let result = check_plugin_compatibility(&pkg, "0.5.0");
        assert!(result.is_ok());
    }

    #[test]
    fn test_check_plugin_compatibility_mismatched_major() {
        let pkg = PluginPackageJson {
            name: None,
            version: None,
            description: None,
            main: None,
            exports: None,
            engines: Some(PluginEngines {
                opencode: Some("1.0.0".to_string()),
            }),
            opencode_id: None,
        };

        let result = check_plugin_compatibility(&pkg, "0.1.0");
        assert!(result.is_err());
    }
}
