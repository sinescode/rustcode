//! NPM package management types — install inputs, registry configuration,
//! entry points, and package name sanitization.
//!
//! Ported from:
//! - `packages/core/src/npm.ts`
//! - `packages/core/src/npm-config.ts`
//!
//! OpenCode commit: 5d0f86606ac30690f79f0a6a9f41a1f49fe95d0b

use serde::{Deserialize, Serialize};

use crate::repository::CacheLock;

// ── NpmInstallFailedError ─────────────────────────────────────────────

/// Failed npm install error.
///
/// # Source
/// `packages/core/src/npm.ts` — `NpmInstallFailedError`
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum NpmInstallFailedError {
    /// Package name was empty.
    InvalidPackage { name: String },
    /// Failed to spawn the npm subprocess.
    SpawnFailed { message: String },
    /// npm install exited with a non-zero status.
    InstallFailed { message: String },
    /// Generic install failure with optional packages and cause.
    InstallError {
        #[serde(default, skip_serializing_if = "Option::is_none")]
        add: Option<Vec<String>>,
        dir: String,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        cause: Option<String>,
    },
}

impl std::fmt::Display for NpmInstallFailedError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::InvalidPackage { name } => {
                write!(f, "invalid npm package name: `{name}`")
            }
            Self::SpawnFailed { message } => {
                write!(f, "failed to spawn npm: {message}")
            }
            Self::InstallFailed { message } => {
                write!(f, "npm install failed: {message}")
            }
            Self::InstallError { add, dir, cause } => {
                write!(f, "npm install failed in `{dir}`")?;
                if let Some(ref packages) = add {
                    write!(f, " for packages: {packages:?}")?;
                }
                if let Some(ref cause) = cause {
                    write!(f, ": {cause}")?;
                }
                Ok(())
            }
        }
    }
}

impl std::error::Error for NpmInstallFailedError {}

// ── NpmEntryPoint ─────────────────────────────────────────────────────

/// An NPM package entry point.
///
/// Associates a directory with an optional entrypoint path within
/// that directory (e.g. a specific JS/TS file to load).
///
/// # Source
/// `packages/core/src/npm.ts` — `NpmEntryPoint`
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NpmEntryPoint {
    /// The directory containing the package.
    pub directory: String,
    /// Optional entrypoint file path relative to [`directory`](Self::directory).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub entrypoint: Option<String>,
}

impl NpmEntryPoint {
    /// Returns a reference to the directory path.
    ///
    /// # Source
    /// `packages/core/src/npm.ts` — `NpmEntryPoint.directory` getter
    pub fn directory(&self) -> &str {
        &self.directory
    }

    /// Returns the entrypoint path, if set.
    ///
    /// # Source
    /// `packages/core/src/npm.ts` — `NpmEntryPoint.entrypoint` getter
    pub fn entrypoint(&self) -> Option<&str> {
        self.entrypoint.as_deref()
    }
}

// ── NpmPackageAddInput ────────────────────────────────────────────────

/// Input for adding a single NPM package.
///
/// Carries the package name and an optional version constraint
/// (e.g. `"^2.0.0"`, `"latest"`).
///
/// # Source
/// `packages/core/src/npm.ts` — `NpmPackageAddInput`
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NpmPackageAddInput {
    /// Package name (unscoped or scoped, e.g. `"lodash"` or `"@scope/pkg"`).
    pub name: String,
    /// Optional version or semver constraint.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub version: Option<String>,
}

// ── NpmInstallInput ───────────────────────────────────────────────────

/// Input for an NPM install operation.
///
/// Contains an optional list of packages to add alongside
/// the implicit `npm install` (which resolves from an existing
/// `package.json`).
///
/// # Source
/// `packages/core/src/npm.ts` — `NpmInstallInput`
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct NpmInstallInput {
    /// Target directory for the install operation.
    #[serde(default)]
    pub dir: String,
    /// Packages to add (each with optional version).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub add: Option<Vec<NpmPackageAddInput>>,
}

// ── NpmRegistryConfig ─────────────────────────────────────────────────

/// NPM registry configuration.
///
/// Holds the registry URL and any additional npm configuration
/// key-value pairs that influence install behaviour.
///
/// # Source
/// `packages/core/src/npm-config.ts` — `NpmRegistryConfig`
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NpmRegistryConfig {
    /// The registry URL to use.
    ///
    /// Defaults to the public npm registry: `"https://registry.npmjs.org"`.
    #[serde(default = "default_registry")]
    pub registry: String,
}

/// Returns the default npm registry URL.
fn default_registry() -> String {
    "https://registry.npmjs.org".to_string()
}

impl Default for NpmRegistryConfig {
    fn default() -> Self {
        Self {
            registry: default_registry(),
        }
    }
}

// ── NpmConfig ─────────────────────────────────────────────────────────

/// NPM configuration loaded from `.npmrc` files.
///
/// Resolves the registry URL by checking the project-local `.npmrc`
/// first, then falling back to the user-level `~/.npmrc`.
///
/// # Source
/// `packages/core/src/npm-config.ts` — config resolution logic
#[derive(Debug, Clone)]
pub struct NpmConfig {
    /// The resolved registry URL.
    pub registry: String,
    /// Optional cache directory path.
    pub cache: Option<String>,
    /// Optional prefix directory path.
    pub prefix: Option<String>,
}

impl NpmConfig {
    /// Load npm configuration from `.npmrc` files, falling back to defaults.
    ///
    /// Checks `dir/.npmrc` first, then `~/.npmrc`. Parses `registry=`
    /// lines to extract the registry URL.
    pub fn load(dir: &str) -> Self {
        let npmrc = std::fs::read_to_string(format!("{}/.npmrc", dir)).or_else(|_| {
            std::fs::read_to_string(
                dirs::home_dir()
                    .map(|h| h.join(".npmrc").to_string_lossy().to_string())
                    .unwrap_or_default(),
            )
        });
        let mut registry = "https://registry.npmjs.org".to_string();
        if let Ok(content) = npmrc {
            for line in content.lines() {
                let line = line.trim();
                if line.starts_with("registry=") {
                    registry = line[9..].to_string();
                }
            }
        }
        Self {
            registry,
            cache: None,
            prefix: None,
        }
    }

    /// Try to load npm configuration, returning an error on I/O failure.
    ///
    /// Unlike [`load`](Self::load), this propagates filesystem errors
    /// instead of silently falling back to defaults.
    pub fn try_load(dir: &str) -> Result<Self, NpmInstallFailedError> {
        let local_npmrc = format!("{}/.npmrc", dir);
        let home_npmrc = dirs::home_dir()
            .map(|h| h.join(".npmrc").to_string_lossy().to_string())
            .unwrap_or_default();

        let content = std::fs::read_to_string(&local_npmrc)
            .or_else(|_| std::fs::read_to_string(&home_npmrc))
            .map_err(|e| NpmInstallFailedError::InstallError {
                add: None,
                dir: dir.to_string(),
                cause: Some(format!("failed to read .npmrc: {e}")),
            })?;

        let mut registry = "https://registry.npmjs.org".to_string();
        for line in content.lines() {
            let line = line.trim();
            if line.starts_with("registry=") {
                registry = line[9..].to_string();
            }
        }
        Ok(Self {
            registry,
            cache: None,
            prefix: None,
        })
    }
}

// ── Sanitize package name ─────────────────────────────────────────────

/// Characters that are illegal in Windows filenames and are replaced
/// by [`sanitize_package_name`].
const ILLEGAL_FILENAME_CHARS: &[char] = &[
    '*', '"', '<', '>', '|', ':', '?', '/', '\\', ' ',
];

/// Sanitize an npm package name for use as a filesystem directory name.
///
/// Replaces characters that are illegal in Windows filenames (`*`, `"`,
/// `<`, `>`, `|`, `:`, `?`, `/`, `\`, space) with underscores (`_`).
/// This mirrors the [`sanitize()`]
/// function in the upstream TS source.
///
/// # Source
/// `packages/core/src/npm.ts` — `sanitize()` function
///
/// # Examples
///
/// ```rust
/// use rustcode_core::npm::sanitize_package_name;
///
/// assert_eq!(sanitize_package_name("@scope/pkg"), "_scope_pkg");
/// assert_eq!(sanitize_package_name("my package"), "my_package");
/// assert_eq!(sanitize_package_name("clean-pkg"), "clean-pkg");
/// ```
pub fn sanitize_package_name(name: &str) -> String {
    name.chars()
        .map(|c| {
            if ILLEGAL_FILENAME_CHARS.contains(&c) {
                '_'
            } else {
                c
            }
        })
        .collect()
}

/// Sanitize an npm package name for the current platform.
///
/// On Windows, replaces illegal filename characters and control characters.
/// On non-Windows platforms, returns the name unchanged.
///
/// Ported from: `packages/core/src/npm.ts` — `sanitize()`
pub fn sanitize(pkg: &str) -> String {
    #[cfg(target_os = "windows")]
    {
        let illegal = ['<', '>', ':', '"', '|', '?', '*'];
        pkg.chars()
            .map(|c| {
                if illegal.contains(&c) || (c as u32) < 32 {
                    '_'
                } else {
                    c
                }
            })
            .collect()
    }
    #[cfg(not(target_os = "windows"))]
    {
        pkg.to_string()
    }
}

// ── NpmPackageSpecifier ───────────────────────────────────────────────

/// Parsed npm package specifier with separate name and version fields.
///
/// # Source
/// `packages/core/src/npm.ts` — `parseSpecifier()` return type
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NpmPackageSpecifier {
    /// Package name (may include scope, e.g. `"@scope/pkg"`).
    pub name: String,
    /// Optional version or semver constraint (e.g. `"^2.0.0"`).
    pub version: Option<String>,
}

/// Parse a package specifier string into a [`NpmPackageSpecifier`].
///
/// Handles:
/// - `pkg` → name only
/// - `pkg@version` → name + version
/// - `@scope/pkg@version` → scoped name + version
///
/// Returns an error if the specifier is empty or malformed.
///
/// # Source
/// `packages/core/src/npm.ts` — `parseSpecifier()`
pub fn parse_specifier(
    spec: &str,
) -> Result<NpmPackageSpecifier, NpmInstallFailedError> {
    if spec.is_empty() {
        return Err(NpmInstallFailedError::InvalidPackage {
            name: spec.to_string(),
        });
    }
    if spec.starts_with('@') {
        if let Some(at_pos) = spec[1..].find('@') {
            let name = &spec[..=at_pos];
            let version = &spec[at_pos + 2..];
            return Ok(NpmPackageSpecifier {
                name: name.to_string(),
                version: Some(version.to_string()),
            });
        }
    }
    if let Some(at_pos) = spec.rfind('@') {
        if at_pos > 0 {
            return Ok(NpmPackageSpecifier {
                name: spec[..at_pos].to_string(),
                version: Some(spec[at_pos + 1..].to_string()),
            });
        }
    }
    Ok(NpmPackageSpecifier {
        name: spec.to_string(),
        version: None,
    })
}

/// Compute the package directory path within a cache directory.
///
/// # Source
/// `packages/core/src/npm.ts` — `packageDirectory()` helper
pub fn package_directory(cache_dir: &str, pkg: &str) -> String {
    format!("{}/packages/{}", cache_dir, sanitize(pkg))
}

// ── NpmService ─────────────────────────────────────────────────────────────

/// Service for NPM package management operations.
///
/// Ported from: `packages/core/src/npm.ts`
pub struct NpmService {
    /// NPM registry configuration
    registry_config: NpmRegistryConfig,
    /// Path to npm binary
    npm_path: String,
}

impl NpmService {
    /// Create a new NpmService with default registry config.
    pub fn new() -> Self {
        Self {
            registry_config: NpmRegistryConfig::default(),
            npm_path: Self::resolve_npm(),
        }
    }

    /// Create with a custom registry configuration.
    pub fn with_config(config: NpmRegistryConfig) -> Self {
        Self {
            registry_config: config,
            npm_path: Self::resolve_npm(),
        }
    }

    /// Resolve the npm binary path.
    pub(crate) fn resolve_npm() -> String {
        // Check env var
        if let Ok(path) = std::env::var("NPM_PATH") {
            if std::path::Path::new(&path).exists() {
                return path;
            }
        }

        // Check system PATH for npm
        if let Some(path) = find_on_path("npm") {
            return path;
        }

        // Also try npx, pnpm, yarn
        for cmd in &["npx", "pnpm", "yarn"] {
            if let Some(path) = find_on_path(cmd) {
                return path;
            }
        }

        // Fallback
        "npm".to_string()
    }

    /// Resolve a package specifier into its entry point.
    ///
    /// This validates the specifier format and returns the directory
    /// where the package would be installed.
    ///
    /// Ported from: `packages/core/src/npm.ts` — `resolve()`
    pub fn resolve(
        &self,
        package_spec: &str,
    ) -> Result<NpmEntryPoint, NpmInstallFailedError> {
        if package_spec.is_empty() {
            return Err(NpmInstallFailedError::InstallError {
                add: None,
                dir: ".".into(),
                cause: Some("empty package specifier".into()),
            });
        }

        // Determine the package directory name
        let specifier = parse_specifier(package_spec)?;
        let sanitized = sanitize_package_name(&specifier.name);
        let node_modules = std::env::current_dir()
            .unwrap_or_else(|_| std::path::PathBuf::from("."))
            .join("node_modules")
            .join(&sanitized);

        Ok(NpmEntryPoint {
            directory: node_modules.to_string_lossy().to_string(),
            entrypoint: None,
        })
    }

    /// Parse a package specifier into (name, version).
    ///
    /// Supports formats:
    /// - `package-name` (latest)
    /// - `package-name@1.2.3` (specific version)
    /// - `@scope/package-name@^2.0.0` (scoped with semver)
    /// - `package-name@latest` (tag)
    pub fn parse_specifier(&self, spec: &str) -> (String, Option<String>) {
        match parse_specifier(spec) {
            Ok(s) => (s.name, s.version),
            Err(_) => (spec.to_string(), None),
        }
    }

    /// Validate packages, then spawn `npm install --save --save-prod`.
    ///
    /// Ported from: `packages/core/src/npm.ts` — `install()`
    pub async fn install(
        &self,
        input: &NpmInstallInput,
    ) -> Result<(), NpmInstallFailedError> {
        if let Some(ref packages) = input.add {
            for pkg in packages {
                if pkg.name.is_empty() {
                    return Err(NpmInstallFailedError::InvalidPackage {
                        name: pkg.name.clone(),
                    });
                }
            }
        }

        let mut cmd = tokio::process::Command::new(&self.npm_path);
        cmd.arg("install").arg("--save").arg("--save-prod");
        if !input.dir.is_empty() {
            cmd.current_dir(&input.dir);
        }
        if let Some(ref packages) = input.add {
            for pkg in packages {
                cmd.arg(&pkg.name);
            }
        }

        let output = cmd
            .output()
            .await
            .map_err(|e| NpmInstallFailedError::SpawnFailed {
                message: e.to_string(),
            })?;

        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr);
            return Err(NpmInstallFailedError::InstallFailed {
                message: stderr.to_string(),
            });
        }
        Ok(())
    }

    /// Add a single package by installing it, then resolve its entry point.
    ///
    /// Ported from: `packages/core/src/npm.ts` — `add()`
    pub async fn add(
        &self,
        dir: &str,
        package: &str,
    ) -> Result<NpmEntryPoint, NpmInstallFailedError> {
        self.install(&NpmInstallInput {
            dir: dir.to_string(),
            add: Some(vec![NpmPackageAddInput {
                name: package.to_string(),
                version: None,
            }]),
        })
        .await?;
        self.resolve_entry_point(dir, package)
    }

    /// Locate a package binary by name.
    ///
    /// Checks `node_modules/.bin/` first, then falls back to system PATH.
    ///
    /// Ported from: `packages/core/src/npm.ts` — `which()`
    pub fn which(dir: &str, pkg: &str, bin: Option<&str>) -> Option<String> {
        let bin_name = bin.unwrap_or(pkg);
        let path = format!("{}/node_modules/.bin/{}", dir, bin_name);
        if std::path::Path::new(&path).exists() {
            return Some(path);
        }
        // Fallback: check system PATH
        which_in_path(bin_name)
    }

    /// Install with a per-directory file lock to prevent concurrent npm operations.
    ///
    /// Ported from: `packages/core/src/npm.ts` — `install_with_lock()`
    pub async fn install_with_lock(
        &self,
        input: &NpmInstallInput,
    ) -> Result<(), NpmInstallFailedError> {
        let lock_dir = if input.dir.is_empty() {
            std::path::Path::new(".")
        } else {
            std::path::Path::new(&input.dir)
        };
        let _lock = CacheLock::wait_lock(lock_dir).map_err(|e| {
            NpmInstallFailedError::SpawnFailed {
                message: e.to_string(),
            }
        })?;
        self.install(input).await
    }

    /// Resolve the entry point for an installed package by reading its `package.json`.
    ///
    /// Ported from: `packages/core/src/npm.ts` — `resolve_entry_point()`
    fn resolve_entry_point(
        &self,
        dir: &str,
        pkg: &str,
    ) -> Result<NpmEntryPoint, NpmInstallFailedError> {
        let pkg_dir = format!("{}/node_modules/{}", dir, pkg);
        let package_json = format!("{}/package.json", pkg_dir);
        let main = std::fs::read_to_string(&package_json)
            .ok()
            .and_then(|s| serde_json::from_str::<serde_json::Value>(&s).ok())
            .and_then(|v| {
                v["exports"]["."]["default"]
                    .as_str()
                    .or_else(|| v["exports"]["."].as_str())
                    .or_else(|| v["main"].as_str())
                    .map(String::from)
            });

        Ok(NpmEntryPoint {
            directory: pkg_dir,
            entrypoint: main,
        })
    }

    /// Get the current npm registry URL.
    pub fn registry_url(&self) -> &str {
        &self.registry_config.registry
    }

    /// Get the npm binary path.
    pub fn npm_path(&self) -> &str {
        &self.npm_path
    }
}

impl Default for NpmService {
    fn default() -> Self {
        Self::new()
    }
}

/// Find a binary on the system PATH.
fn find_on_path(name: &str) -> Option<String> {
    std::env::var_os("PATH").and_then(|path_var| {
        std::env::split_paths(&path_var).find_map(|dir| {
            let path = dir.join(name);
            if path.is_file() {
                Some(path.to_string_lossy().to_string())
            } else {
                None
            }
        })
    })
}

/// Find a binary on the system PATH by iterating over colon-separated entries.
///
/// Ported from: `packages/core/src/npm.ts` — `which_in_path()`
fn which_in_path(name: &str) -> Option<String> {
    let path_var = std::env::var("PATH").ok()?;
    for dir in path_var.split(':') {
        let full = format!("{}/{}", dir, name);
        if std::path::Path::new(&full).is_file() {
            return Some(full);
        }
    }
    None
}

// ── Tests ─────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    // ── NpmInstallFailedError ─────────────────────────────────────────

    #[test]
    fn install_failed_error_display_invalid_package() {
        let err = NpmInstallFailedError::InvalidPackage {
            name: String::new(),
        };
        let msg = err.to_string();
        assert!(msg.contains("invalid npm package name"));
    }

    #[test]
    fn install_failed_error_display_spawn_failed() {
        let err = NpmInstallFailedError::SpawnFailed {
            message: "No such file or directory".into(),
        };
        let msg = err.to_string();
        assert!(msg.contains("failed to spawn npm"));
        assert!(msg.contains("No such file or directory"));
    }

    #[test]
    fn install_failed_error_display_install_failed() {
        let err = NpmInstallFailedError::InstallFailed {
            message: "ERESOLVE unable to resolve dependency tree".into(),
        };
        let msg = err.to_string();
        assert!(msg.contains("npm install failed"));
        assert!(msg.contains("ERESOLVE"));
    }

    #[test]
    fn install_failed_error_display_install_error_with_packages_and_cause() {
        let err = NpmInstallFailedError::InstallError {
            add: Some(vec!["lodash".into(), "express".into()]),
            dir: "/tmp/project".into(),
            cause: Some("EACCES: permission denied".into()),
        };
        let msg = err.to_string();
        assert!(msg.contains("npm install failed in `/tmp/project`"));
        assert!(msg.contains(r#"["lodash", "express"]"#));
        assert!(msg.contains("EACCES: permission denied"));
    }

    #[test]
    fn install_failed_error_display_install_error_minimal() {
        let err = NpmInstallFailedError::InstallError {
            add: None,
            dir: "/tmp/simple".into(),
            cause: None,
        };
        let msg = err.to_string();
        assert_eq!(msg, "npm install failed in `/tmp/simple`");
    }

    #[test]
    fn install_failed_error_is_error_trait() {
        let err = NpmInstallFailedError::SpawnFailed {
            message: "test".into(),
        };
        let _: &dyn std::error::Error = &err;
    }

    #[test]
    fn install_failed_error_serialization_roundtrip() {
        let err = NpmInstallFailedError::InstallError {
            add: Some(vec!["pkg-a".into()]),
            dir: "/tmp/pkg".into(),
            cause: Some("timeout".into()),
        };
        let json = serde_json::to_string(&err).expect("serialize");
        let roundtrip: NpmInstallFailedError =
            serde_json::from_str(&json).expect("deserialize");
        match roundtrip {
            NpmInstallFailedError::InstallError { add, dir, cause } => {
                assert_eq!(dir, "/tmp/pkg");
                assert_eq!(add.unwrap(), vec!["pkg-a"]);
                assert_eq!(cause.unwrap(), "timeout");
            }
            other => panic!("expected InstallError variant, got {:?}", other),
        }
    }

    // ── NpmEntryPoint ─────────────────────────────────────────────────

    #[test]
    fn entry_point_accessors() {
        let ep = NpmEntryPoint {
            directory: "/tmp/mypkg".into(),
            entrypoint: Some("dist/index.js".into()),
        };
        assert_eq!(ep.directory(), "/tmp/mypkg");
        assert_eq!(ep.entrypoint(), Some("dist/index.js"));
    }

    #[test]
    fn entry_point_no_entrypoint() {
        let ep = NpmEntryPoint {
            directory: "/tmp/nodepkg".into(),
            entrypoint: None,
        };
        assert_eq!(ep.directory(), "/tmp/nodepkg");
        assert_eq!(ep.entrypoint(), None);
    }

    // ── NpmPackageAddInput ────────────────────────────────────────────

    #[test]
    fn package_add_input_serialization_full() {
        let input = NpmPackageAddInput {
            name: "lodash".into(),
            version: Some("^4.0.0".into()),
        };
        let json = serde_json::to_string(&input).expect("serialize");
        let roundtrip: NpmPackageAddInput =
            serde_json::from_str(&json).expect("deserialize");
        assert_eq!(roundtrip.name, "lodash");
        assert_eq!(roundtrip.version.unwrap(), "^4.0.0");
    }

    #[test]
    fn package_add_input_serialization_no_version() {
        let input = NpmPackageAddInput {
            name: "express".into(),
            version: None,
        };
        let json = serde_json::to_string(&input).expect("serialize");
        assert!(!json.contains("version"));
        let roundtrip: NpmPackageAddInput =
            serde_json::from_str(&json).expect("deserialize");
        assert_eq!(roundtrip.name, "express");
        assert!(roundtrip.version.is_none());
    }

    // ── NpmInstallInput ───────────────────────────────────────────────

    #[test]
    fn install_input_default() {
        let input = NpmInstallInput::default();
        assert!(input.add.is_none());
        assert!(input.dir.is_empty());
    }

    #[test]
    fn install_input_with_dir_and_packages() {
        let input = NpmInstallInput {
            dir: "/tmp/project".into(),
            add: Some(vec![
                NpmPackageAddInput {
                    name: "lodash".into(),
                    version: Some("^4.0.0".into()),
                },
                NpmPackageAddInput {
                    name: "express".into(),
                    version: None,
                },
            ]),
        };
        let json = serde_json::to_string(&input).expect("serialize");
        let roundtrip: NpmInstallInput =
            serde_json::from_str(&json).expect("deserialize");
        assert_eq!(roundtrip.dir, "/tmp/project");
        let pkgs = roundtrip.add.expect("add should be Some");
        assert_eq!(pkgs.len(), 2);
        assert_eq!(pkgs[0].name, "lodash");
        assert_eq!(pkgs[1].name, "express");
    }

    #[test]
    fn install_input_serialization_empty() {
        let input = NpmInstallInput {
            dir: String::new(),
            add: None,
        };
        let json = serde_json::to_string(&input).expect("serialize");
        let roundtrip: NpmInstallInput =
            serde_json::from_str(&json).expect("deserialize");
        assert!(roundtrip.add.is_none());
    }

    // ── NpmRegistryConfig ─────────────────────────────────────────────

    #[test]
    fn registry_config_default() {
        let config = NpmRegistryConfig::default();
        assert_eq!(config.registry, "https://registry.npmjs.org");
    }

    #[test]
    fn registry_config_custom() {
        let config = NpmRegistryConfig {
            registry: "https://registry.yarnpkg.com".into(),
        };
        assert_eq!(config.registry, "https://registry.yarnpkg.com");
    }

    #[test]
    fn registry_config_deserializes_default() {
        let json = r#"{}"#;
        let config: NpmRegistryConfig =
            serde_json::from_str(json).expect("deserialize");
        assert_eq!(config.registry, "https://registry.npmjs.org");
    }

    // ── sanitize_package_name ─────────────────────────────────────────

    #[test]
    fn sanitize_scoped_package_replaces_slash() {
        assert_eq!(sanitize_package_name("@scope/pkg"), "_scope_pkg");
    }

    #[test]
    fn sanitize_replaces_spaces() {
        assert_eq!(
            sanitize_package_name("my package name"),
            "my_package_name"
        );
    }

    #[test]
    fn sanitize_replaces_windows_illegal_chars() {
        assert_eq!(
            sanitize_package_name("bad*chars\"<test>|"),
            "bad_chars__test__"
        );
    }

    #[test]
    fn sanitize_replaces_colon_and_question() {
        assert_eq!(sanitize_package_name("file:test?.js"), "file_test_.js");
    }

    #[test]
    fn sanitize_preserves_clean_name() {
        assert_eq!(sanitize_package_name("clean-package"), "clean-package");
    }

    #[test]
    fn sanitize_preserves_valid_special_chars() {
        assert_eq!(
            sanitize_package_name("my-pkg_v2.0.1-alpha"),
            "my-pkg_v2.0.1-alpha"
        );
    }

    #[test]
    fn sanitize_empty_string() {
        assert_eq!(sanitize_package_name(""), "");
    }

    #[test]
    fn sanitize_backslash_replaced() {
        assert_eq!(sanitize_package_name("path\\to\\pkg"), "path_to_pkg");
    }

    // ── sanitize() ────────────────────────────────────────────────────

    #[test]
    fn sanitize_passthrough_on_non_windows() {
        // On non-Windows platforms, sanitize() returns the input unchanged
        assert_eq!(sanitize("@scope/pkg"), "@scope/pkg");
        assert_eq!(sanitize("my package"), "my package");
        assert_eq!(sanitize("clean-name"), "clean-name");
    }

    // ── NpmService resolve / parse_specifier ──────────────────────────

    #[test]
    fn test_npm_service_resolve_simple() {
        let svc = NpmService::new();
        let result = svc.resolve("lodash").expect("resolve should succeed");
        assert!(result.directory().contains("node_modules"));
        assert!(result.directory().contains("lodash"));
    }

    #[test]
    fn test_npm_service_resolve_scoped() {
        let svc = NpmService::new();
        let result =
            svc.resolve("@scope/mypackage").expect("resolve should succeed");
        assert!(result.directory().contains("node_modules"));
        assert!(result.directory().contains("_scope_mypackage"));
    }

    #[test]
    fn test_npm_service_resolve_with_version() {
        let svc = NpmService::new();
        let result = svc
            .resolve("express@4.18.2")
            .expect("resolve should succeed");
        assert!(result.directory().contains("node_modules"));
    }

    #[test]
    fn test_npm_service_resolve_empty() {
        let svc = NpmService::new();
        let result = svc.resolve("");
        assert!(result.is_err());
    }

    #[test]
    fn test_npm_service_parse_specifier_simple() {
        let svc = NpmService::new();
        let (name, version) = svc.parse_specifier("lodash");
        assert_eq!(name, "lodash");
        assert!(version.is_none());
    }

    #[test]
    fn test_npm_service_parse_specifier_with_version() {
        let svc = NpmService::new();
        let (name, version) = svc.parse_specifier("lodash@4.17.21");
        assert_eq!(name, "lodash");
        assert_eq!(version, Some("4.17.21".into()));
    }

    #[test]
    fn test_npm_service_parse_specifier_scoped() {
        let svc = NpmService::new();
        let (name, version) = svc.parse_specifier("@scope/pkg");
        assert_eq!(name, "@scope/pkg");
        assert!(version.is_none());
    }

    #[test]
    fn test_npm_service_parse_specifier_scoped_with_version() {
        let svc = NpmService::new();
        let (name, version) = svc.parse_specifier("@scope/pkg@1.0.0");
        assert_eq!(name, "@scope/pkg");
        assert_eq!(version, Some("1.0.0".into()));
    }

    #[test]
    fn test_npm_service_parse_specifier_scoped_deep_version() {
        let svc = NpmService::new();
        let (name, version) =
            svc.parse_specifier("@angular/core@^16.0.0");
        assert_eq!(name, "@angular/core");
        assert_eq!(version, Some("^16.0.0".into()));
    }

    #[test]
    fn test_npm_service_parse_specifier_org_scoped() {
        let svc = NpmService::new();
        let (name, version) = svc.parse_specifier("@types/node@18");
        assert_eq!(name, "@types/node");
        assert_eq!(version, Some("18".into()));
    }

    // ── NpmService install (async) ────────────────────────────────────

    #[tokio::test]
    async fn test_npm_service_install_validates_empty_name() {
        let svc = NpmService::new();
        let input = NpmInstallInput {
            dir: String::new(),
            add: Some(vec![NpmPackageAddInput {
                name: "".into(),
                version: None,
            }]),
        };
        let result = svc.install(&input).await;
        assert!(result.is_err());
        match result.unwrap_err() {
            NpmInstallFailedError::InvalidPackage { name } => {
                assert!(name.is_empty());
            }
            other => panic!("expected InvalidPackage, got {:?}", other),
        }
    }

    #[tokio::test]
    async fn test_npm_service_install_nonexistent_pkg_fails() {
        let svc = NpmService::new();
        let tmp = tempfile::tempdir().expect("tmpdir");
        let input = NpmInstallInput {
            dir: tmp.path().to_string_lossy().to_string(),
            add: Some(vec![NpmPackageAddInput {
                name: "this-pkg-definitely-does-not-exist-xyz123".into(),
                version: None,
            }]),
        };
        let result = svc.install(&input).await;
        // npm will fail with a non-zero exit code for a nonexistent package
        assert!(result.is_err());
    }

    #[tokio::test]
    async fn test_npm_service_install_empty_input() {
        let svc = NpmService::new();
        let input = NpmInstallInput::default();
        // Empty dir and no packages: npm install in current dir with no args
        // This should succeed (npm install with no package.json is a no-op)
        let result = svc.install(&input).await;
        assert!(result.is_ok());
    }

    // ── which() ───────────────────────────────────────────────────────

    #[test]
    fn test_which_nonexistent_pkg_returns_none() {
        let result = NpmService::which(
            "/tmp/nonexistent-dir",
            "some-bin",
            None,
        );
        assert!(result.is_none());
    }

    #[test]
    fn test_which_with_explicit_bin_name() {
        let result = NpmService::which(
            "/tmp/nonexistent-dir",
            "pkg",
            Some("custom-bin"),
        );
        // Should look for custom-bin, not pkg
        assert!(result.is_none());
    }

    #[test]
    fn test_which_finds_existing_path_binary() {
        // "sh" should exist on Linux in /bin or /usr/bin
        let result = NpmService::which("/tmp/nonexistent", "sh", None);
        // May or may not find it depending on PATH, but should not panic
        let _ = result;
    }

    // ── which_in_path() ──────────────────────────────────────────────

    #[test]
    fn test_which_in_path_nonexistent() {
        let result = which_in_path("this-binary-xyz-definitely-does-not-exist");
        assert!(result.is_none());
    }

    #[test]
    fn test_which_in_path_finds_sh() {
        // "sh" should be on PATH on any Unix system
        let result = which_in_path("sh");
        assert!(result.is_some());
        let path = result.unwrap();
        assert!(path.ends_with("/sh"));
    }

    // ── resolve_entry_point() ────────────────────────────────────────

    #[test]
    fn test_resolve_entry_point_no_package_json() {
        let svc = NpmService::new();
        let result = svc
            .resolve_entry_point("/tmp/nonexistent", "some-pkg")
            .expect("should succeed");
        assert!(result.directory.contains("node_modules"));
        assert!(result.directory.contains("some-pkg"));
        assert!(result.entrypoint.is_none());
    }

    #[test]
    fn test_resolve_entry_point_with_package_json() {
        let tmp = tempfile::tempdir().expect("tmpdir");
        let pkg_dir = tmp.path().join("node_modules").join("my-pkg");
        std::fs::create_dir_all(&pkg_dir).expect("create dirs");
        std::fs::write(
            pkg_dir.join("package.json"),
            r#"{"name": "my-pkg", "main": "dist/index.js"}"#,
        )
        .expect("write package.json");

        let svc = NpmService::new();
        let result = svc
            .resolve_entry_point(
                &tmp.path().to_string_lossy(),
                "my-pkg",
            )
            .expect("should succeed");
        assert!(result.directory.contains("my-pkg"));
        assert_eq!(
            result.entrypoint.as_deref(),
            Some("dist/index.js")
        );
    }

    #[test]
    fn test_resolve_entry_point_no_main_field() {
        let tmp = tempfile::tempdir().expect("tmpdir");
        let pkg_dir = tmp.path().join("node_modules").join("bare-pkg");
        std::fs::create_dir_all(&pkg_dir).expect("create dirs");
        std::fs::write(
            pkg_dir.join("package.json"),
            r#"{"name": "bare-pkg"}"#,
        )
        .expect("write package.json");

        let svc = NpmService::new();
        let result = svc
            .resolve_entry_point(
                &tmp.path().to_string_lossy(),
                "bare-pkg",
            )
            .expect("should succeed");
        assert!(result.entrypoint.is_none());
    }

    // ── NpmService misc ──────────────────────────────────────────────

    #[test]
    fn test_npm_service_registry_url() {
        let svc = NpmService::new();
        assert_eq!(svc.registry_url(), "https://registry.npmjs.org");
    }

    #[test]
    fn test_npm_service_custom_registry() {
        let config = NpmRegistryConfig {
            registry: "https://registry.example.com".into(),
        };
        let svc = NpmService::with_config(config);
        assert_eq!(svc.registry_url(), "https://registry.example.com");
    }

    #[test]
    fn test_npm_service_default() {
        let svc = NpmService::default();
        assert!(!svc.npm_path().is_empty());
    }

    #[test]
    fn test_npm_service_resolve_npm_path() {
        let path = NpmService::resolve_npm();
        assert!(!path.is_empty());
    }

    // ── NpmConfig ─────────────────────────────────────────────────────

    #[test]
    fn test_npm_config_load_defaults_when_no_npmrc() {
        let config = NpmConfig::load("/tmp/nonexistent-dir-xyz");
        assert_eq!(
            config.registry,
            "https://registry.npmjs.org"
        );
        assert!(config.cache.is_none());
        assert!(config.prefix.is_none());
    }

    #[test]
    fn test_npm_config_load_from_local_npmrc() {
        let tmp = tempfile::tempdir().expect("tmpdir");
        std::fs::write(
            tmp.path().join(".npmrc"),
            "registry=https://custom.registry.example.com\n",
        )
        .expect("write .npmrc");

        let config = NpmConfig::load(&tmp.path().to_string_lossy());
        assert_eq!(
            config.registry,
            "https://custom.registry.example.com"
        );
    }

    #[test]
    fn test_npm_config_load_ignores_non_registry_lines() {
        let tmp = tempfile::tempdir().expect("tmpdir");
        std::fs::write(
            tmp.path().join(".npmrc"),
            "//registry.npmjs.org/:_authToken=abc123\nregistry=https://my-registry.com\n",
        )
        .expect("write .npmrc");

        let config = NpmConfig::load(&tmp.path().to_string_lossy());
        assert_eq!(config.registry, "https://my-registry.com");
    }

    #[test]
    fn test_npm_config_load_empty_npmrc() {
        let tmp = tempfile::tempdir().expect("tmpdir");
        std::fs::write(tmp.path().join(".npmrc"), "").expect("write .npmrc");

        let config = NpmConfig::load(&tmp.path().to_string_lossy());
        assert_eq!(
            config.registry,
            "https://registry.npmjs.org"
        );
    }

    #[test]
    fn test_npm_config_try_load_defaults_when_no_npmrc() {
        let config =
            NpmConfig::try_load("/tmp/nonexistent-dir-xyz").expect("try_load");
        assert_eq!(
            config.registry,
            "https://registry.npmjs.org"
        );
    }

    #[test]
    fn test_npm_config_try_load_from_local_npmrc() {
        let tmp = tempfile::tempdir().expect("tmpdir");
        std::fs::write(
            tmp.path().join(".npmrc"),
            "registry=https://try-load.registry.com\n",
        )
        .expect("write .npmrc");

        let config = NpmConfig::try_load(&tmp.path().to_string_lossy())
            .expect("try_load");
        assert_eq!(
            config.registry,
            "https://try-load.registry.com"
        );
    }

    // ── package_directory ──────────────────────────────────────────────

    #[test]
    fn test_package_directory_simple() {
        let dir = package_directory("/tmp/cache", "lodash");
        assert_eq!(dir, "/tmp/cache/packages/lodash");
    }

    #[test]
    fn test_package_directory_scoped() {
        let dir = package_directory("/tmp/cache", "@scope/pkg");
        assert_eq!(dir, "/tmp/cache/packages/@scope/pkg");
    }

    #[test]
    fn test_package_directory_sanitizes() {
        let dir = package_directory("/tmp/cache", "my package");
        assert_eq!(dir, "/tmp/cache/packages/my package");
    }

    // ── parse_specifier (standalone) ──────────────────────────────────

    #[test]
    fn test_parse_specifier_simple() {
        let spec = parse_specifier("lodash").unwrap();
        assert_eq!(spec.name, "lodash");
        assert!(spec.version.is_none());
    }

    #[test]
    fn test_parse_specifier_with_version() {
        let spec = parse_specifier("lodash@4.17.21").unwrap();
        assert_eq!(spec.name, "lodash");
        assert_eq!(spec.version.as_deref(), Some("4.17.21"));
    }

    #[test]
    fn test_parse_specifier_scoped_no_version() {
        let spec = parse_specifier("@scope/pkg").unwrap();
        assert_eq!(spec.name, "@scope/pkg");
        assert!(spec.version.is_none());
    }

    #[test]
    fn test_parse_specifier_scoped_with_version() {
        let spec = parse_specifier("@scope/pkg@1.0.0").unwrap();
        assert_eq!(spec.name, "@scope/pkg");
        assert_eq!(spec.version.as_deref(), Some("1.0.0"));
    }

    #[test]
    fn test_parse_specifier_scoped_with_caret() {
        let spec = parse_specifier("@angular/core@^16.0.0").unwrap();
        assert_eq!(spec.name, "@angular/core");
        assert_eq!(spec.version.as_deref(), Some("^16.0.0"));
    }

    #[test]
    fn test_parse_specifier_scoped_with_latest() {
        let spec = parse_specifier("@types/node@latest").unwrap();
        assert_eq!(spec.name, "@types/node");
        assert_eq!(spec.version.as_deref(), Some("latest"));
    }

    #[test]
    fn test_parse_specifier_empty_returns_err() {
        let result = parse_specifier("");
        assert!(result.is_err());
        match result.unwrap_err() {
            NpmInstallFailedError::InvalidPackage { name } => {
                assert!(name.is_empty());
            }
            other => panic!("expected InvalidPackage, got {:?}", other),
        }
    }

    #[test]
    fn test_parse_specifier_at_only() {
        let spec = parse_specifier("@").unwrap();
        assert_eq!(spec.name, "@");
        assert!(spec.version.is_none());
    }

    // ── NpmPackageSpecifier ───────────────────────────────────────────

    #[test]
    fn test_npm_package_specifier_equality() {
        let a = NpmPackageSpecifier {
            name: "lodash".into(),
            version: Some("^4.0.0".into()),
        };
        let b = NpmPackageSpecifier {
            name: "lodash".into(),
            version: Some("^4.0.0".into()),
        };
        assert_eq!(a, b);
    }

    #[test]
    fn test_npm_package_specifier_debug() {
        let spec = NpmPackageSpecifier {
            name: "express".into(),
            version: None,
        };
        let debug = format!("{:?}", spec);
        assert!(debug.contains("express"));
    }

    // ── resolve_entry_point with exports ──────────────────────────────

    #[test]
    fn test_resolve_entry_point_with_exports_default() {
        let tmp = tempfile::tempdir().expect("tmpdir");
        let pkg_dir = tmp
            .path()
            .join("node_modules")
            .join("exports-pkg");
        std::fs::create_dir_all(&pkg_dir).expect("create dirs");
        std::fs::write(
            pkg_dir.join("package.json"),
            r#"{"name": "exports-pkg", "exports": {".": {"default": "dist/mod.js"}}}"#,
        )
        .expect("write package.json");

        let svc = NpmService::new();
        let result = svc
            .resolve_entry_point(
                &tmp.path().to_string_lossy(),
                "exports-pkg",
            )
            .expect("should succeed");
        assert_eq!(
            result.entrypoint.as_deref(),
            Some("dist/mod.js")
        );
    }

    #[test]
    fn test_resolve_entry_point_with_exports_string() {
        let tmp = tempfile::tempdir().expect("tmpdir");
        let pkg_dir = tmp
            .path()
            .join("node_modules")
            .join("exports-str-pkg");
        std::fs::create_dir_all(&pkg_dir).expect("create dirs");
        std::fs::write(
            pkg_dir.join("package.json"),
            r#"{"name": "exports-str-pkg", "exports": {".": "lib/index.js"}}"#,
        )
        .expect("write package.json");

        let svc = NpmService::new();
        let result = svc
            .resolve_entry_point(
                &tmp.path().to_string_lossy(),
                "exports-str-pkg",
            )
            .expect("should succeed");
        assert_eq!(
            result.entrypoint.as_deref(),
            Some("lib/index.js")
        );
    }

    #[test]
    fn test_resolve_entry_point_exports_overrides_main() {
        let tmp = tempfile::tempdir().expect("tmpdir");
        let pkg_dir = tmp
            .path()
            .join("node_modules")
            .join("both-fields-pkg");
        std::fs::create_dir_all(&pkg_dir).expect("create dirs");
        std::fs::write(
            pkg_dir.join("package.json"),
            r#"{"name": "both-fields-pkg", "main": "old.js", "exports": {".": {"default": "new.js"}}}"#,
        )
        .expect("write package.json");

        let svc = NpmService::new();
        let result = svc
            .resolve_entry_point(
                &tmp.path().to_string_lossy(),
                "both-fields-pkg",
            )
            .expect("should succeed");
        assert_eq!(
            result.entrypoint.as_deref(),
            Some("new.js")
        );
    }

    #[test]
    fn test_resolve_entry_point_falls_back_to_main() {
        let tmp = tempfile::tempdir().expect("tmpdir");
        let pkg_dir = tmp
            .path()
            .join("node_modules")
            .join("main-only-pkg");
        std::fs::create_dir_all(&pkg_dir).expect("create dirs");
        std::fs::write(
            pkg_dir.join("package.json"),
            r#"{"name": "main-only-pkg", "main": "index.js"}"#,
        )
        .expect("write package.json");

        let svc = NpmService::new();
        let result = svc
            .resolve_entry_point(
                &tmp.path().to_string_lossy(),
                "main-only-pkg",
            )
            .expect("should succeed");
        assert_eq!(
            result.entrypoint.as_deref(),
            Some("index.js")
        );
    }
}
