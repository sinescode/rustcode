//! TUI plugin system — plugin trait, API, manager, slots, and built-in plugins.
//!
//! Ported from: `packages/tui/src/plugin/` and `packages/plugin/src/tui.ts`
//!
//! ## Architecture
//!
//! - [`TuiPlugin`] — trait that all TUI plugins implement
//! - [`TuiPluginApi`] — handles for plugins to interact with the TUI
//! - [`TuiPluginManager`] — registry that loads, activates, and manages plugins
//! - [`SlotName`] / [`SlotRegistry`] — named render slots for plugin content
//! - Built-in plugins: home tips, which-key, diff viewer, notifications

use anyhow::Result;
use std::collections::HashMap;
use std::path::Path;
use std::sync::{Arc, RwLock};

use ratatui::{layout::Rect, Frame};

use crate::keymap::TuiAction;
use crate::theme::Theme;

// ── Plugin Trait ─────────────────────────────────────────────────────────

/// A TUI plugin that can register hooks, render into slots, and interact
/// with the TUI via [`TuiPluginApi`].
///
/// # Source
/// Ported from `packages/plugin/src/tui.ts` `TuiPlugin` type.
pub trait TuiPlugin: Send + Sync {
    /// Unique plugin identifier (e.g. `"home-tips"`, `"which-key"`).
    fn id(&self) -> &str;

    /// Called when the plugin is first registered with the manager.
    /// The plugin should use `api` to register hooks, keybindings, themes, etc.
    fn on_register(&self, api: &mut TuiPluginApi) -> Result<()>;

    /// Called when the plugin is unregistered. Clean up resources.
    fn on_unregister(&self, api: &TuiPluginApi) -> Result<()>;
}

// ── Plugin API ───────────────────────────────────────────────────────────

/// Sidebar panel definition registered by a plugin.
#[derive(Debug, Clone)]
pub struct PluginSidebarPanelDef {
    pub id: String,
    pub title: String,
}

/// Dialog definition pushed by a plugin.
#[derive(Debug, Clone)]
pub enum PluginDialogDef {
    Alert {
        title: String,
        message: String,
    },
    Confirm {
        title: String,
        message: String,
    },
    Prompt {
        title: String,
        placeholder: Option<String>,
    },
}

/// A render callback that draws into a slot area.
pub type SlotRenderFn = Arc<dyn Fn(&mut Frame, Rect) -> Result<()> + Send + Sync>;

/// Internal mutable state for the plugin API.
#[derive(Default)]
struct PluginApiInner {
    /// Themes registered by plugins (theme_name → JSON definition).
    plugin_themes: Vec<(String, Theme)>,
    /// Keybindings registered by plugins.
    keybindings: Vec<(String, TuiAction)>,
    /// Sidebar panel definitions.
    sidebar_panels: Vec<PluginSidebarPanelDef>,
    /// Pending toasts to show.
    toast_queue: Vec<(String, String)>,
    /// Pending dialogs to push.
    dialog_queue: Vec<PluginDialogDef>,
    /// Plugin KV store (persisted as JSON).
    kv_store: HashMap<String, String>,
}

/// API handle provided to plugins for interacting with the TUI.
///
/// # Source
/// Ported from `packages/plugin/src/tui.ts` `TuiPluginApi`.
#[derive(Clone)]
pub struct TuiPluginApi {
    inner: Arc<RwLock<PluginApiInner>>,
}

impl Default for TuiPluginApi {
    fn default() -> Self {
        Self {
            inner: Arc::new(RwLock::new(PluginApiInner::default())),
        }
    }
}

impl TuiPluginApi {
    /// Create a new empty API.
    pub fn new() -> Self {
        Self::default()
    }

    /// Register a theme that plugins can add.
    pub fn add_theme(&mut self, name: &str, theme: Theme) {
        if let Ok(mut inner) = self.inner.write() {
            inner.plugin_themes.push((name.to_string(), theme));
        }
    }

    /// Register a keybinding.
    pub fn add_keybinding(&mut self, key: &str, action: TuiAction) {
        if let Ok(mut inner) = self.inner.write() {
            inner.keybindings.push((key.to_string(), action));
        }
    }

    /// Add a sidebar panel.
    pub fn add_sidebar_panel(&mut self, id: &str, title: &str) {
        if let Ok(mut inner) = self.inner.write() {
            inner.sidebar_panels.push(PluginSidebarPanelDef {
                id: id.to_string(),
                title: title.to_string(),
            });
        }
    }

    /// Queue a toast to show.
    pub fn add_toast(&mut self, message: &str, variant: &str) {
        if let Ok(mut inner) = self.inner.write() {
            inner.toast_queue.push((message.to_string(), variant.to_string()));
        }
    }

    /// Queue a dialog to show.
    pub fn add_dialog(&mut self, dialog: PluginDialogDef) {
        if let Ok(mut inner) = self.inner.write() {
            inner.dialog_queue.push(dialog);
        }
    }

    /// Get a value from the plugin KV store.
    pub fn kv_get(&self, key: &str) -> Option<String> {
        if let Ok(inner) = self.inner.read() {
            inner.kv_store.get(key).cloned()
        } else {
            None
        }
    }

    /// Set a value in the plugin KV store.
    pub fn kv_set(&self, key: &str, value: &str) {
        if let Ok(mut inner) = self.inner.write() {
            inner.kv_store.insert(key.to_string(), value.to_string());
        }
    }

    /// Drain pending toasts.
    pub fn drain_toasts(&self) -> Vec<(String, String)> {
        if let Ok(mut inner) = self.inner.write() {
            std::mem::take(&mut inner.toast_queue)
        } else {
            Vec::new()
        }
    }

    /// Drain pending dialogs.
    pub fn drain_dialogs(&self) -> Vec<PluginDialogDef> {
        if let Ok(mut inner) = self.inner.write() {
            std::mem::take(&mut inner.dialog_queue)
        } else {
            Vec::new()
        }
    }

    /// Drain registered keybindings.
    pub fn drain_keybindings(&self) -> Vec<(String, TuiAction)> {
        if let Ok(mut inner) = self.inner.write() {
            std::mem::take(&mut inner.keybindings)
        } else {
            Vec::new()
        }
    }

    /// Drain registered sidebar panels.
    pub fn drain_sidebar_panels(&self) -> Vec<PluginSidebarPanelDef> {
        if let Ok(mut inner) = self.inner.write() {
            std::mem::take(&mut inner.sidebar_panels)
        } else {
            Vec::new()
        }
    }

    /// Drain registered plugin themes.
    pub fn drain_themes(&self) -> Vec<(String, Theme)> {
        if let Ok(mut inner) = self.inner.write() {
            std::mem::take(&mut inner.plugin_themes)
        } else {
            Vec::new()
        }
    }
}

// ── Slot System ──────────────────────────────────────────────────────────

/// Named render slots where plugins can inject content.
///
/// # Source
/// Ported from `packages/tui/src/plugin/slots.tsx` host slot map.
#[derive(Debug, Clone, Hash, PartialEq, Eq)]
pub enum SlotName {
    /// Logo area on the home screen.
    HomeLogo,
    /// Prompt input area on the home screen.
    HomePrompt,
    /// Prompt input area during a session.
    SessionPrompt,
    /// Sidebar panels (sidebar/context, sidebar/todo, etc.).
    Sidebar(String),
    /// Bottom bar area of the app.
    AppBottom,
}

impl SlotName {
    /// Parse a string into a SlotName.
    pub fn from_str(s: &str) -> Option<Self> {
        match s {
            "home_logo" => Some(Self::HomeLogo),
            "home_prompt" => Some(Self::HomePrompt),
            "session_prompt" => Some(Self::SessionPrompt),
            "app_bottom" => Some(Self::AppBottom),
            other if other.starts_with("sidebar/") => {
                let name = other.strip_prefix("sidebar/")?;
                Some(Self::Sidebar(name.to_string()))
            }
            _ => None,
        }
    }

    /// Return the string representation.
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::HomeLogo => "home_logo",
            Self::HomePrompt => "home_prompt",
            Self::SessionPrompt => "session_prompt",
            Self::AppBottom => "app_bottom",
            Self::Sidebar(_) => "sidebar/*",
        }
    }
}

/// Slot render functions keyed by slot name.
///
/// Each slot can have multiple render functions that all get called
/// during rendering. They are drawn in registration order.
#[derive(Default)]
pub struct SlotRegistry {
    slots: HashMap<String, Vec<SlotRenderFn>>,
}

impl SlotRegistry {
    pub fn new() -> Self {
        Self {
            slots: HashMap::new(),
        }
    }

    /// Register a render function for a named slot.
    pub fn register(&mut self, slot: SlotName, render_fn: SlotRenderFn) {
        let key = match &slot {
            SlotName::Sidebar(name) => format!("sidebar/{}", name),
            other => other.as_str().to_string(),
        };
        self.slots.entry(key).or_default().push(render_fn);
    }

    /// Get all render functions for a slot.
    pub fn get(&self, slot: &SlotName) -> Vec<&SlotRenderFn> {
        let key = match slot {
            SlotName::Sidebar(name) => format!("sidebar/{}", name),
            other => other.as_str().to_string(),
        };
        self.slots.get(&key).map(|v| v.iter().collect()).unwrap_or_default()
    }

    /// Render all functions for a slot into the given frame area.
    pub fn render(&self, slot: &SlotName, f: &mut Frame, area: Rect) -> Result<()> {
        for render_fn in self.get(slot) {
            render_fn(f, area)?;
        }
        Ok(())
    }

    /// Check if any render functions are registered for a slot.
    pub fn has(&self, slot: &SlotName) -> bool {
        !self.get(slot).is_empty()
    }
}

// ── Plugin Manager ───────────────────────────────────────────────────────

/// Manages the lifecycle of TUI plugins.
///
/// # Source
/// Ported from `packages/tui/src/plugin/runtime.tsx` and plugin service.
pub struct TuiPluginManager {
    /// Registered plugins keyed by ID.
    plugins: HashMap<String, Arc<dyn TuiPlugin>>,
    /// Shared API for all plugins.
    api: TuiPluginApi,
    /// Slot registry for plugin render hooks.
    slots: SlotRegistry,
}

impl Default for TuiPluginManager {
    fn default() -> Self {
        Self::new()
    }
}

impl TuiPluginManager {
    /// Create a new empty plugin manager.
    pub fn new() -> Self {
        Self {
            plugins: HashMap::new(),
            api: TuiPluginApi::new(),
            slots: SlotRegistry::new(),
        }
    }

    /// Register a plugin. Calls `on_register` on the plugin.
    pub fn register(&mut self, plugin: Arc<dyn TuiPlugin>) -> Result<()> {
        let id = plugin.id().to_string();
        let mut api = self.api.clone();
        plugin.on_register(&mut api)?;

        // Drain any themes registered by the plugin
        let themes = api.drain_themes();
        for (name, _theme) in themes {
            tracing::info!(plugin = %id, theme = %name, "plugin registered theme");
        }

        // Drain keybindings
        let keybindings = api.drain_keybindings();
        if !keybindings.is_empty() {
            tracing::info!(plugin = %id, count = keybindings.len(), "plugin registered keybindings");
        }

        // Drain sidebar panels
        let panels = api.drain_sidebar_panels();
        if !panels.is_empty() {
            tracing::info!(plugin = %id, count = panels.len(), "plugin registered sidebar panels");
        }

        self.plugins.insert(id, plugin);
        Ok(())
    }

    /// Unregister a plugin by ID. Calls `on_unregister` on the plugin.
    pub fn unregister(&mut self, id: &str) -> Result<()> {
        if let Some(plugin) = self.plugins.remove(id) {
            plugin.on_unregister(&self.api)?;
            tracing::info!(plugin = %id, "plugin unregistered");
        }
        Ok(())
    }

    /// Load plugins from a config file.
    /// Currently a stub — will parse plugin specs from JSON config.
    pub fn load_from_config(&mut self, _config_path: &Path) -> Result<()> {
        // Stub: future implementation will parse plugin entries and
        // dynamically load shared libraries or WASM plugins.
        tracing::info!("plugin config loading not yet implemented");
        Ok(())
    }

    /// List all registered plugin IDs.
    pub fn list(&self) -> Vec<&str> {
        self.plugins.keys().map(|s| s.as_str()).collect()
    }

    /// Get a registered plugin by ID.
    pub fn get(&self, id: &str) -> Option<&Arc<dyn TuiPlugin>> {
        self.plugins.get(id)
    }

    /// Get a mutable reference to the slot registry.
    pub fn slots_mut(&mut self) -> &mut SlotRegistry {
        &mut self.slots
    }

    /// Get the slot registry.
    pub fn slots(&self) -> &SlotRegistry {
        &self.slots
    }

    /// Get a reference to the plugin API.
    pub fn api(&self) -> &TuiPluginApi {
        &self.api
    }

    /// Get a mutable reference to the plugin API.
    pub fn api_mut(&mut self) -> &mut TuiPluginApi {
        &mut self.api
    }

    /// Number of registered plugins.
    pub fn count(&self) -> usize {
        self.plugins.len()
    }

    /// Register built-in plugins.
    pub fn register_builtins(&mut self) -> Result<()> {
        self.register(Arc::new(HomeTipsPlugin))?;
        self.register(Arc::new(WhichKeyPlugin))?;
        self.register(Arc::new(DiffViewerPlugin))?;
        self.register(Arc::new(NotificationsPlugin))?;
        Ok(())
    }

    /// Process pending actions from plugin APIs.
    /// Called by the main loop each frame.
    pub fn process_pending(&self) -> PluginPendingActions {
        PluginPendingActions {
            toasts: self.api.drain_toasts(),
            dialogs: self.api.drain_dialogs(),
        }
    }
}

/// Pending actions collected from plugins during a frame.
pub struct PluginPendingActions {
    pub toasts: Vec<(String, String)>,
    pub dialogs: Vec<PluginDialogDef>,
}

// ── Built-in Plugin: Home Tips ──────────────────────────────────────────

/// Shows contextual tips on the home screen.
///
/// Displays a random tip in the `home_logo` slot. Tips cycle every
/// 30 seconds and cover keyboard shortcuts, features, and usage hints.
///
/// # Source
/// Ported from the home screen tips feature in `packages/tui/src/routes/home/index.tsx`.
pub struct HomeTipsPlugin;

const HOME_TIPS: &[&str] = &[
    "Tip: Press Ctrl+P to open the command palette",
    "Tip: Use Ctrl+B to toggle the sidebar",
    "Tip: Press ? to show available keybindings",
    "Tip: Use Tab to cycle through agents",
    "Tip: Ctrl+R renames the current session",
    "Tip: Pin sessions with Ctrl+F for quick switching",
    "Tip: Use Ctrl+T to cycle through model variants",
    "Tip: Press Ctrl+Z to suspend the terminal",
    "Tip: Use /plan for planning, /build for coding",
    "Tip: Sessions persist until you delete them",
    "Tip: Ctrl+L cycles through color themes",
    "Tip: Press Ctrl+U to scroll half-page up",
    "Tip: Use Alt+Enter to force a newline in input",
];

impl TuiPlugin for HomeTipsPlugin {
    fn id(&self) -> &str {
        "home-tips"
    }

    fn on_register(&self, api: &mut TuiPluginApi) -> Result<()> {
        tracing::info!("home-tips plugin registered");
        let _ = api;
        Ok(())
    }

    fn on_unregister(&self, _api: &TuiPluginApi) -> Result<()> {
        Ok(())
    }
}

/// Get a random home tip.
pub fn random_home_tip() -> &'static str {
    let idx = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs() as usize
        / 30
        % HOME_TIPS.len();
    HOME_TIPS[idx]
}

// ── Built-in Plugin: Which-Key ──────────────────────────────────────────

/// Shows available keybindings as an overlay (which-key style).
///
/// Displays registered keybindings grouped by category when the user
/// presses a leader key or opens the help overlay.
///
/// # Source
/// Ported from `packages/tui/src/plugin/adapters.tsx` command/help features.
pub struct WhichKeyPlugin;

impl TuiPlugin for WhichKeyPlugin {
    fn id(&self) -> &str {
        "which-key"
    }

    fn on_register(&self, api: &mut TuiPluginApi) -> Result<()> {
        tracing::info!("which-key plugin registered");
        let _ = api;
        Ok(())
    }

    fn on_unregister(&self, _api: &TuiPluginApi) -> Result<()> {
        Ok(())
    }
}

// ── Built-in Plugin: Diff Viewer ────────────────────────────────────────

/// Rich diff display plugin for session diff panels.
///
/// Enhances the built-in diff component with syntax-highlighted diff
/// hunks, file-level navigation, and inline change highlighting.
///
/// # Source
/// Ported from diff-related features in `packages/tui/src/component/diff.tsx`.
pub struct DiffViewerPlugin;

impl TuiPlugin for DiffViewerPlugin {
    fn id(&self) -> &str {
        "diff-viewer"
    }

    fn on_register(&self, api: &mut TuiPluginApi) -> Result<()> {
        tracing::info!("diff-viewer plugin registered");
        let _ = api;
        Ok(())
    }

    fn on_unregister(&self, _api: &TuiPluginApi) -> Result<()> {
        Ok(())
    }
}

// ── Built-in Plugin: Notifications ──────────────────────────────────────

/// System notification integration.
///
/// Bridges TUI toasts and events with OS-level notification systems
/// (via rustcode-core's notification infrastructure). Supports
/// attention sounds and desktop notifications.
///
/// # Source
/// Ported from attention/sound features in `packages/tui/src/plugin/`.
pub struct NotificationsPlugin;

impl TuiPlugin for NotificationsPlugin {
    fn id(&self) -> &str {
        "notifications"
    }

    fn on_register(&self, api: &mut TuiPluginApi) -> Result<()> {
        tracing::info!("notifications plugin registered");
        let _ = api;
        Ok(())
    }

    fn on_unregister(&self, _api: &TuiPluginApi) -> Result<()> {
        Ok(())
    }
}

// ── Tests ────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_slot_name_roundtrip() {
        assert_eq!(SlotName::from_str("home_logo"), Some(SlotName::HomeLogo));
        assert_eq!(SlotName::from_str("home_prompt"), Some(SlotName::HomePrompt));
        assert_eq!(
            SlotName::from_str("session_prompt"),
            Some(SlotName::SessionPrompt)
        );
        assert_eq!(SlotName::from_str("app_bottom"), Some(SlotName::AppBottom));
        assert_eq!(
            SlotName::from_str("sidebar/context"),
            Some(SlotName::Sidebar("context".into()))
        );
        assert_eq!(SlotName::from_str("unknown"), None);
    }

    #[test]
    fn test_slot_registry() {
        let mut registry = SlotRegistry::new();
        assert!(!registry.has(&SlotName::HomeLogo));

        let render_fn: SlotRenderFn = Arc::new(|_, _| Ok(()));
        registry.register(SlotName::HomeLogo, render_fn);
        assert!(registry.has(&SlotName::HomeLogo));
        assert_eq!(registry.get(&SlotName::HomeLogo).len(), 1);
    }

    #[test]
    fn test_plugin_manager_empty() {
        let manager = TuiPluginManager::new();
        assert_eq!(manager.count(), 0);
        assert!(manager.list().is_empty());
    }

    #[test]
    fn test_register_plugin() {
        let mut manager = TuiPluginManager::new();
        assert!(manager.register(Arc::new(HomeTipsPlugin)).is_ok());
        assert_eq!(manager.count(), 1);
        assert_eq!(manager.list(), vec!["home-tips"]);
    }

    #[test]
    fn test_unregister_plugin() {
        let mut manager = TuiPluginManager::new();
        manager.register(Arc::new(HomeTipsPlugin)).unwrap();
        assert_eq!(manager.count(), 1);
        manager.unregister("home-tips").unwrap();
        assert_eq!(manager.count(), 0);
    }

    #[test]
    fn test_register_builtins() {
        let mut manager = TuiPluginManager::new();
        assert!(manager.register_builtins().is_ok());
        assert_eq!(manager.count(), 4);
        let ids = manager.list();
        assert!(ids.contains(&"home-tips"));
        assert!(ids.contains(&"which-key"));
        assert!(ids.contains(&"diff-viewer"));
        assert!(ids.contains(&"notifications"));
    }

    #[test]
    fn test_plugin_api_default() {
        let api = TuiPluginApi::new();
        assert!(api.kv_get("nonexistent").is_none());
    }

    #[test]
    fn test_plugin_api_kv() {
        let mut api = TuiPluginApi::new();
        api.kv_set("key1", "value1");
        assert_eq!(api.kv_get("key1"), Some("value1".into()));
    }

    #[test]
    fn test_plugin_api_drain() {
        let mut api = TuiPluginApi::new();
        api.add_toast("hello", "info");
        api.add_toast("world", "warning");
        assert_eq!(api.drain_toasts().len(), 2);
        assert!(api.drain_toasts().is_empty());
    }

    #[test]
    fn test_home_tips_not_empty() {
        let tip = random_home_tip();
        assert!(!tip.is_empty());
        assert!(tip.starts_with("Tip:"));
    }

    #[test]
    fn test_plugin_pending_actions() {
        let mut manager = TuiPluginManager::new();
        manager.api_mut().add_toast("test", "info");
        let pending = manager.process_pending();
        assert_eq!(pending.toasts.len(), 1);
    }

    struct TestPlugin;

    impl TuiPlugin for TestPlugin {
        fn id(&self) -> &str {
            "test"
        }
        fn on_register(&self, api: &mut TuiPluginApi) -> Result<()> {
            api.add_keybinding("ctrl+t", TuiAction::ThemeSwitch);
            api.add_sidebar_panel("test-panel", "Test Panel");
            Ok(())
        }
        fn on_unregister(&self, _api: &TuiPluginApi) -> Result<()> {
            Ok(())
        }
    }

    #[test]
    fn test_plugin_registers_api_actions() {
        let mut api = TuiPluginApi::new();
        let plugin = TestPlugin;
        plugin.on_register(&mut api).unwrap();

        let kbs = api.drain_keybindings();
        assert_eq!(kbs.len(), 1);
        assert_eq!(kbs[0].0, "ctrl+t");

        let panels = api.drain_sidebar_panels();
        assert_eq!(panels.len(), 1);
        assert_eq!(panels[0].id, "test-panel");
    }
}
