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
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum PluginHook {
    /// Triggered when text completion is requested.
    ExperimentalTextComplete,
    /// Triggered when a session is being compacted (context window management).
    ExperimentalSessionCompacting,
    /// Triggered when chat messages are being transformed before sending to the LLM.
    ExperimentalChatMessagesTransform,
    /// Triggered for custom/named events from the event bus.
    Event,
    /// Config hook — plugins are notified when configuration changes.
    Config,
}

impl PluginHook {
    /// Return the string name of this hook as used in plugin manifests.
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::ExperimentalTextComplete => "experimental.text.complete",
            Self::ExperimentalSessionCompacting => "experimental.session.compacting",
            Self::ExperimentalChatMessagesTransform => "experimental.chat.messages.transform",
            Self::Event => "event",
            Self::Config => "config",
        }
    }

    /// Parse a hook name string into a [`PluginHook`], returning `None` if unknown.
    pub fn from_str(s: &str) -> Option<Self> {
        match s {
            "experimental.text.complete" => Some(Self::ExperimentalTextComplete),
            "experimental.session.compacting" => Some(Self::ExperimentalSessionCompacting),
            "experimental.chat.messages.transform" => Some(Self::ExperimentalChatMessagesTransform),
            "event" => Some(Self::Event),
            "config" => Some(Self::Config),
            _ => None,
        }
    }

    /// All known hook name strings.
    pub fn all_strs() -> &'static [&'static str] {
        &[
            "experimental.text.complete",
            "experimental.session.compacting",
            "experimental.chat.messages.transform",
            "event",
            "config",
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

/// Manages the lifecycle of plugins: registration, hook dispatch, and disposal.
///
/// Ported from `packages/opencode/src/plugin/index.ts` `Service`.
#[derive(Debug, Default)]
pub struct PluginManager {
    /// All loaded plugins.
    plugins: Vec<Plugin>,
    /// Plugin metadata store keyed by plugin id.
    meta: HashMap<String, PluginMetaEntry>,
    /// Timestamp of last initialization (Unix millis).
    last_init: Option<u64>,
}

impl PluginManager {
    /// Create an empty plugin manager.
    pub fn new() -> Self {
        Self {
            plugins: Vec::new(),
            meta: HashMap::new(),
            last_init: None,
        }
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
}
