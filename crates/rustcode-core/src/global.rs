//! Global application paths and configuration.
//!
//! Ported from: `packages/core/src/global.ts` (lines 1–88)
//!   OpenCode commit: 5d0f86606ac30690f79f0a6a9f41a1f49fe95d0b
//!
//! ## Architecture
//!
//! The TS `Global` module defines:
//! - XDG-compliant filesystem paths (`data`, `cache`, `config`, `state`, `tmp`)
//! - A `Service` context providing these paths to the Effect runtime
//! - `make()` factory — assembles the `Interface` from XDG dirs + optional overrides
//! - `layer` / `defaultLayer` — dependency injection layers
//!
//! In Rust:
//! - [`GlobalPaths`] holds all application paths computed from XDG directories.
//! - [`GlobalConfig`] wraps the paths and provides the service interface.
//! - [`make_global_paths()`] computes paths from `dirs` crate (XDG-compatible).

use serde::{Deserialize, Serialize};

// ---------------------------------------------------------------------------
// GlobalPaths — XDG-compliant application paths
// ---------------------------------------------------------------------------

/// All application filesystem paths, computed from XDG base directories.
///
/// # Source
/// Ported from `packages/core/src/global.ts` lines 1–29.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct GlobalPaths {
    /// User home directory.
    pub home: String,
    /// Data directory (XDG_DATA_HOME/appname).
    pub data: String,
    /// Cache directory (XDG_CACHE_HOME/appname).
    pub cache: String,
    /// Config directory (XDG_CONFIG_HOME/appname).
    pub config: String,
    /// State directory (XDG_STATE_HOME/appname).
    pub state: String,
    /// Temp directory.
    pub tmp: String,
    /// Binary installation directory (within cache).
    pub bin: String,
    /// Log directory (within data).
    pub log: String,
    /// Git repos directory (within data).
    pub repos: String,
}

/// Application name used for XDG directory computation.
const APP_NAME: &str = "opencode";

impl GlobalPaths {
    /// Create a new set of paths from XDG directories.
    ///
    /// # Source
    /// Ported from `packages/core/src/global.ts` lines 11–15 (path computation)
    /// and lines 74–78 (`make()` factory).
    pub fn discover() -> Self {
        Self::with_home(dirs::home_dir().map(|p| p.to_string_lossy().to_string()).unwrap_or_default())
    }

    /// Create paths rooted at a specific home directory.
    pub fn with_home(home: String) -> Self {
        let data = xdg_dir("XDG_DATA_HOME", &home, ".local/share", APP_NAME);
        let cache = xdg_dir("XDG_CACHE_HOME", &home, ".cache", APP_NAME);
        let config = xdg_dir("XDG_CONFIG_HOME", &home, ".config", APP_NAME);
        let state = xdg_dir("XDG_STATE_HOME", &home, ".local/state", APP_NAME);
        let tmp = std::path::PathBuf::from(std::env::temp_dir()).join(APP_NAME);

        let bin = std::path::PathBuf::from(&cache).join("bin");
        let log = std::path::PathBuf::from(&data).join("log");
        let repos = std::path::PathBuf::from(&data).join("repos");

        Self {
            home,
            data,
            cache,
            config,
            state,
            tmp: tmp.to_string_lossy().to_string(),
            bin: bin.to_string_lossy().to_string(),
            log: log.to_string_lossy().to_string(),
            repos: repos.to_string_lossy().to_string(),
        }
    }

    /// Resolve the config directory, respecting the `OPENCODE_CONFIG_DIR` env var.
    ///
    /// # Source
    /// Ported from `packages/core/src/global.ts` line 63:
    /// `config: Flag.OPENCODE_CONFIG_DIR ?? Path.config`
    pub fn resolve_config_dir(&self) -> String {
        std::env::var("OPENCODE_CONFIG_DIR").unwrap_or_else(|_| self.config.clone())
    }

    /// Resolve the test home directory override.
    ///
    /// # Source
    /// Ported from `packages/core/src/global.ts` lines 18–19:
    /// `get home() { return process.env.OPENCODE_TEST_HOME ?? os.homedir() }`
    pub fn resolve_home() -> String {
        std::env::var("OPENCODE_TEST_HOME").unwrap_or_else(|_| {
            dirs::home_dir()
                .map(|p| p.to_string_lossy().to_string())
                .unwrap_or_default()
        })
    }
}

impl Default for GlobalPaths {
    fn default() -> Self {
        Self::discover()
    }
}

/// Compute an XDG directory, falling back to a default path under home.
///
/// # Source
/// Ported from the `xdg-basedir` import in `packages/core/src/global.ts` lines 1, 11–15.
fn xdg_dir(env_var: &str, home: &str, default_subdir: &str, app_name: &str) -> String {
    std::env::var(env_var)
        .ok()
        .filter(|v| !v.is_empty())
        .map(|base| {
            std::path::PathBuf::from(base)
                .join(app_name)
                .to_string_lossy()
                .to_string()
        })
        .unwrap_or_else(|| {
            std::path::PathBuf::from(home)
                .join(default_subdir)
                .join(app_name)
                .to_string_lossy()
                .to_string()
        })
}

// ---------------------------------------------------------------------------
// GlobalConfig — configuration wrapper for the global service
// ---------------------------------------------------------------------------

/// Global application configuration wrapping filesystem paths.
///
/// This mirrors the TS `Global.Interface` + `Global.Service` pattern.
///
/// # Source
/// Ported from `packages/core/src/global.ts` lines 45–57 (`Interface`),
/// lines 45 (`Service`), and lines 74–78 (`layer`).
#[derive(Debug, Clone)]
pub struct GlobalConfig {
    /// Computed filesystem paths.
    pub paths: GlobalPaths,
}

impl GlobalConfig {
    /// Create a new global config from discovered paths.
    ///
    /// # Source
    /// Ported from `packages/core/src/global.ts` line 74:
    /// `const layer = Layer.effect(Service, Effect.sync(() => Service.of(make())))`
    pub fn new() -> Self {
        Self {
            paths: GlobalPaths::discover(),
        }
    }

    /// Create a global config with custom path overrides.
    ///
    /// # Source
    /// Ported from `packages/core/src/global.ts` lines 59–72 (`make()`)
    /// and lines 82–86 (`layerWith()`).
    pub fn with_overrides(overrides: GlobalPathsOverrides) -> Self {
        let base = GlobalPaths::discover();
        Self {
            paths: GlobalPaths {
                home: overrides.home.unwrap_or(base.home),
                data: overrides.data.unwrap_or(base.data),
                cache: overrides.cache.unwrap_or(base.cache),
                config: overrides.config.unwrap_or(base.config),
                state: overrides.state.unwrap_or(base.state),
                tmp: overrides.tmp.unwrap_or(base.tmp),
                bin: overrides.bin.unwrap_or(base.bin),
                log: overrides.log.unwrap_or(base.log),
                repos: overrides.repos.unwrap_or(base.repos),
            },
        }
    }

    /// User home directory.
    pub fn home(&self) -> &str {
        &self.paths.home
    }

    /// Data directory.
    pub fn data(&self) -> &str {
        &self.paths.data
    }

    /// Cache directory.
    pub fn cache(&self) -> &str {
        &self.paths.cache
    }

    /// Config directory.
    pub fn config(&self) -> &str {
        &self.paths.config
    }

    /// State directory.
    pub fn state(&self) -> &str {
        &self.paths.state
    }

    /// Temp directory.
    pub fn tmp(&self) -> &str {
        &self.paths.tmp
    }

    /// Binary directory.
    pub fn bin(&self) -> &str {
        &self.paths.bin
    }

    /// Log directory.
    pub fn log(&self) -> &str {
        &self.paths.log
    }

    /// Git repos directory.
    pub fn repos(&self) -> &str {
        &self.paths.repos
    }
}

impl Default for GlobalConfig {
    fn default() -> Self {
        Self::new()
    }
}

/// Overrides for individual global paths.
///
/// # Source
/// Ported from `packages/core/src/global.ts` line 59:
/// `make(input: Partial<Interface> = {}): Interface`
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct GlobalPathsOverrides {
    /// Override home directory.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub home: Option<String>,
    /// Override data directory.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub data: Option<String>,
    /// Override cache directory.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub cache: Option<String>,
    /// Override config directory.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub config: Option<String>,
    /// Override state directory.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub state: Option<String>,
    /// Override temp directory.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub tmp: Option<String>,
    /// Override bin directory.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub bin: Option<String>,
    /// Override log directory.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub log: Option<String>,
    /// Override repos directory.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub repos: Option<String>,
}

// ---------------------------------------------------------------------------
// Global module-level singleton path holder
// ---------------------------------------------------------------------------

/// Module-level global paths, lazily initialized.
///
/// This mirrors the TS module-level `const paths = { ... }` at the top of
/// `global.ts`. In Rust, we store it behind a `OnceLock`.
use std::sync::OnceLock;

static GLOBAL_PATHS: OnceLock<GlobalPaths> = OnceLock::new();

/// Get or initialize the global paths singleton.
///
/// # Source
/// Ported from `packages/core/src/global.ts` lines 17–29.
pub fn paths() -> &'static GlobalPaths {
    GLOBAL_PATHS.get_or_init(GlobalPaths::discover)
}

/// Initialize global paths with a custom configuration.
///
/// Must be called before any other access to [`paths()`].
pub fn init_paths(p: GlobalPaths) {
    let _ = GLOBAL_PATHS.set(p);
}

// ---------------------------------------------------------------------------
// Convenience free functions — lightweight accessors
// ---------------------------------------------------------------------------

/// Get the cache directory path.
pub fn cache_dir() -> &'static str {
    &paths().cache
}

/// Get the data directory path.
pub fn data_dir() -> &'static str {
    &paths().data
}

/// Get the binary installation directory path.
pub fn bin_dir() -> String {
    format!("{}/bin", data_dir())
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn global_paths_discover_has_home() {
        let paths = GlobalPaths::discover();
        // home should never be empty
        assert!(!paths.home.is_empty(), "home directory should not be empty");
    }

    #[test]
    fn global_paths_all_fields_present() {
        let paths = GlobalPaths::discover();
        assert!(!paths.data.is_empty());
        assert!(!paths.cache.is_empty());
        assert!(!paths.config.is_empty());
        assert!(!paths.state.is_empty());
        assert!(!paths.tmp.is_empty());
        assert!(!paths.bin.is_empty());
        assert!(!paths.log.is_empty());
        assert!(!paths.repos.is_empty());
    }

    #[test]
    fn global_paths_contain_app_name() {
        let paths = GlobalPaths::discover();
        assert!(paths.data.contains(APP_NAME), "data path should contain app name");
        assert!(paths.config.contains(APP_NAME), "config path should contain app name");
        assert!(paths.cache.contains(APP_NAME), "cache path should contain app name");
    }

    #[test]
    fn global_paths_bin_is_under_cache() {
        let paths = GlobalPaths::discover();
        assert!(
            paths.bin.starts_with(&paths.cache),
            "bin ({}) should be under cache ({})",
            paths.bin,
            paths.cache
        );
    }

    #[test]
    fn global_paths_log_is_under_data() {
        let paths = GlobalPaths::discover();
        assert!(
            paths.log.starts_with(&paths.data),
            "log ({}) should be under data ({})",
            paths.log,
            paths.data
        );
    }

    #[test]
    fn global_paths_repos_is_under_data() {
        let paths = GlobalPaths::discover();
        assert!(
            paths.repos.starts_with(&paths.data),
            "repos ({}) should be under data ({})",
            paths.repos,
            paths.data
        );
    }

    #[test]
    fn global_paths_with_home_uses_custom_home() {
        let paths = GlobalPaths::with_home("/custom/home".into());
        assert_eq!(paths.home, "/custom/home");
        assert!(paths.data.starts_with("/custom/home"));
        assert!(paths.config.starts_with("/custom/home"));
    }

    #[test]
    fn global_paths_serialization_roundtrip() {
        let paths = GlobalPaths::discover();
        let json = serde_json::to_string(&paths).unwrap();
        let parsed: GlobalPaths = serde_json::from_str(&json).unwrap();
        assert_eq!(paths.home, parsed.home);
        assert_eq!(paths.data, parsed.data);
        assert_eq!(paths.config, parsed.config);
    }

    #[test]
    fn global_paths_resolve_config_dir_env_override() {
        let paths = GlobalPaths::discover();
        // Without the env var, should return self.config
        std::env::remove_var("OPENCODE_CONFIG_DIR");
        assert_eq!(paths.resolve_config_dir(), paths.config);
    }

    #[test]
    fn global_config_accessors_match_paths() {
        let config = GlobalConfig::new();
        assert_eq!(config.home(), config.paths.home);
        assert_eq!(config.data(), config.paths.data);
        assert_eq!(config.cache(), config.paths.cache);
        assert_eq!(config.config(), config.paths.config);
        assert_eq!(config.state(), config.paths.state);
        assert_eq!(config.tmp(), config.paths.tmp);
        assert_eq!(config.bin(), config.paths.bin);
        assert_eq!(config.log(), config.paths.log);
        assert_eq!(config.repos(), config.paths.repos);
    }

    #[test]
    fn global_config_with_overrides() {
        let overrides = GlobalPathsOverrides {
            cache: Some("/custom/cache".into()),
            config: Some("/custom/config".into()),
            ..Default::default()
        };
        let config = GlobalConfig::with_overrides(overrides);
        assert_eq!(config.cache(), "/custom/cache");
        assert_eq!(config.config(), "/custom/config");
        // Other paths should still be from discover
        assert!(!config.home().is_empty());
        assert!(!config.data().is_empty());
    }

    #[test]
    fn global_paths_singleton_is_consistent() {
        let p1 = paths();
        let p2 = paths();
        assert_eq!(p1.home, p2.home);
        assert_eq!(p1.data, p2.data);
    }

    #[test]
    fn global_paths_overrides_serialization() {
        let overrides = GlobalPathsOverrides {
            home: Some("/test/home".into()),
            tmp: Some("/test/tmp".into()),
            ..Default::default()
        };
        let json = serde_json::to_string(&overrides).unwrap();
        assert!(json.contains("/test/home"));
        assert!(json.contains("/test/tmp"));

        let parsed: GlobalPathsOverrides = serde_json::from_str(&json).unwrap();
        assert_eq!(parsed.home, Some("/test/home".into()));
        assert_eq!(parsed.tmp, Some("/test/tmp".into()));
        assert!(parsed.data.is_none());
    }

    #[test]
    fn global_config_default_is_new() {
        let c1 = GlobalConfig::default();
        let c2 = GlobalConfig::new();
        assert_eq!(c1.home(), c2.home());
    }

    #[test]
    fn xdg_dir_uses_default_when_not_set() {
        // Ensure the env var is not set
        std::env::remove_var("XDG_TEST_VAR");
        let result = xdg_dir("XDG_TEST_VAR_NONEXISTENT", "/home/test", ".local/share", "testapp");
        assert!(result.starts_with("/home/test"));
        assert!(result.contains("testapp"));
    }

    // ── Free functions ──────────────────────────────────────────

    #[test]
    fn test_cache_dir_matches_paths() {
        assert_eq!(cache_dir(), paths().cache);
    }

    #[test]
    fn test_data_dir_matches_paths() {
        assert_eq!(data_dir(), paths().data);
    }

    #[test]
    fn test_bin_dir_is_data_bin() {
        let bd = bin_dir();
        assert!(bd.starts_with(data_dir()));
        assert!(bd.ends_with("/bin"));
    }
}
