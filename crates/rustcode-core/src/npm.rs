//! NPM package management types — install inputs, registry configuration,
//! entry points, and package name sanitization.
//!
//! Ported from:
//! - `packages/core/src/npm.ts`
//! - `packages/core/src/npm-config.ts`
//!
//! OpenCode commit: 5d0f86606ac30690f79f0a6a9f41a1f49fe95d0b

use serde::{Deserialize, Serialize};

// ── NpmInstallFailedError ─────────────────────────────────────────────

/// Failed npm install error.
///
/// Captures the packages that were requested, the target directory, and
/// an optional underlying cause string.
///
/// # Source
/// `packages/core/src/npm.ts` — `NpmInstallFailedError`
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NpmInstallFailedError {
    /// Packages requested for installation.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub add: Option<Vec<String>>,
    /// Target directory where install was attempted.
    pub dir: String,
    /// Optional underlying error message.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub cause: Option<String>,
}

impl std::fmt::Display for NpmInstallFailedError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "npm install failed in `{}`", self.dir)?;
        if let Some(ref packages) = self.add {
            write!(f, " for packages: {:?}", packages)?;
        }
        if let Some(ref cause) = self.cause {
            write!(f, ": {cause}")?;
        }
        Ok(())
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
            return Err(NpmInstallFailedError {
                add: None,
                dir: ".".into(),
                cause: Some("empty package specifier".into()),
            });
        }

        // Determine the package directory name
        let (name, _version) = self.parse_specifier(package_spec);
        let sanitized = sanitize_package_name(&name);
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
        if spec.starts_with('@') {
            // Scoped package: @scope/name or @scope/name@version
            let rest = &spec[1..];
            if let Some(slash_pos) = rest.find('/') {
                let after_slash = &rest[slash_pos + 1..];
                if let Some(at_pos) = after_slash.rfind('@') {
                    if at_pos > 0 {
                        let name = spec[..slash_pos + at_pos + 2].to_string();
                        let version = Some(after_slash[at_pos + 1..].to_string());
                        return (name, version);
                    }
                }
                // No version in scoped package
                return (spec.to_string(), None);
            }
        }

        // Unscoped package
        if let Some(at_pos) = spec.rfind('@') {
            if at_pos > 0 {
                let name = spec[..at_pos].to_string();
                let version = Some(spec[at_pos + 1..].to_string());
                return (name, version);
            }
        }

        (spec.to_string(), None)
    }

    /// Validate that a package can be installed (no-op stub).
    ///
    /// In production, this would check the npm registry for the package.
    /// Currently just validates the specifier is non-empty.
    ///
    /// Ported from: `packages/core/src/npm.ts` — `install()`
    pub fn install(
        &self,
        input: &NpmInstallInput,
    ) -> Result<(), NpmInstallFailedError> {
        // Validate package names
        if let Some(ref packages) = input.add {
            for pkg in packages {
                if pkg.name.is_empty() {
                    return Err(NpmInstallFailedError {
                        add: Some(vec![pkg.name.clone()]),
                        dir: ".".into(),
                        cause: Some("empty package name".into()),
                    });
                }
            }
        }

        // Stub — actual install would spawn npm process
        // This validates without performing installation
        Ok(())
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

// ── Tests ─────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    // ── NpmInstallFailedError ─────────────────────────────────────────

    #[test]
    fn install_failed_error_display_with_packages_and_cause() {
        let err = NpmInstallFailedError {
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
    fn install_failed_error_display_minimal() {
        let err = NpmInstallFailedError {
            add: None,
            dir: "/tmp/simple".into(),
            cause: None,
        };
        let msg = err.to_string();
        assert_eq!(msg, "npm install failed in `/tmp/simple`");
    }

    #[test]
    fn install_failed_error_is_error_trait() {
        let err = NpmInstallFailedError {
            add: None,
            dir: "/tmp/test".into(),
            cause: None,
        };
        // Verify it implements std::error::Error
        let _: &dyn std::error::Error = &err;
    }

    #[test]
    fn install_failed_error_serialization() {
        let err = NpmInstallFailedError {
            add: Some(vec!["pkg-a".into()]),
            dir: "/tmp/pkg".into(),
            cause: Some("timeout".into()),
        };
        let json = serde_json::to_string(&err).expect("serialize");
        let roundtrip: NpmInstallFailedError = serde_json::from_str(&json).expect("deserialize");
        assert_eq!(roundtrip.dir, "/tmp/pkg");
        assert_eq!(roundtrip.add.unwrap(), vec!["pkg-a"]);
        assert_eq!(roundtrip.cause.unwrap(), "timeout");
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
        let roundtrip: NpmPackageAddInput = serde_json::from_str(&json).expect("deserialize");
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
        // No version field should appear in JSON output
        assert!(!json.contains("version"));
        let roundtrip: NpmPackageAddInput = serde_json::from_str(&json).expect("deserialize");
        assert_eq!(roundtrip.name, "express");
        assert!(roundtrip.version.is_none());
    }

    // ── NpmInstallInput ───────────────────────────────────────────────

    #[test]
    fn install_input_default() {
        let input = NpmInstallInput::default();
        assert!(input.add.is_none());
    }

    #[test]
    fn install_input_serialization_with_packages() {
        let input = NpmInstallInput {
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
        let roundtrip: NpmInstallInput = serde_json::from_str(&json).expect("deserialize");
        let pkgs = roundtrip.add.expect("add should be Some");
        assert_eq!(pkgs.len(), 2);
        assert_eq!(pkgs[0].name, "lodash");
        assert_eq!(pkgs[1].name, "express");
    }

    #[test]
    fn install_input_serialization_empty() {
        let input = NpmInstallInput { add: None };
        let json = serde_json::to_string(&input).expect("serialize");
        let roundtrip: NpmInstallInput = serde_json::from_str(&json).expect("deserialize");
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
        let config: NpmRegistryConfig = serde_json::from_str(json).expect("deserialize");
        assert_eq!(config.registry, "https://registry.npmjs.org");
    }

    // ── sanitize_package_name ─────────────────────────────────────────

    #[test]
    fn sanitize_scoped_package_replaces_slash() {
        assert_eq!(sanitize_package_name("@scope/pkg"), "_scope_pkg");
    }

    #[test]
    fn sanitize_replaces_spaces() {
        assert_eq!(sanitize_package_name("my package name"), "my_package_name");
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
        // Hyphens, dots, underscores, and alphanumerics are all safe
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

    // ── NpmService tests ─────────────────────────────────────────────────

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
        let result = svc.resolve("@scope/mypackage").expect("resolve should succeed");
        assert!(result.directory().contains("node_modules"));
        assert!(result.directory().contains("_scope_mypackage"));
    }

    #[test]
    fn test_npm_service_resolve_with_version() {
        let svc = NpmService::new();
        let result = svc.resolve("express@4.18.2").expect("resolve should succeed");
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
        let (name, version) = svc.parse_specifier("@angular/core@^16.0.0");
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

    #[test]
    fn test_npm_service_install_validates_empty_name() {
        let svc = NpmService::new();
        let input = NpmInstallInput {
            add: Some(vec![NpmPackageAddInput {
                name: "".into(),
                version: None,
            }]),
        };
        let result = svc.install(&input);
        assert!(result.is_err());
    }

    #[test]
    fn test_npm_service_install_validates_ok() {
        let svc = NpmService::new();
        let input = NpmInstallInput {
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
        let result = svc.install(&input);
        assert!(result.is_ok());
    }

    #[test]
    fn test_npm_service_install_empty() {
        let svc = NpmService::new();
        let input = NpmInstallInput { add: None };
        let result = svc.install(&input);
        assert!(result.is_ok());
    }

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
        // Should not crash
        let path = NpmService::resolve_npm();
        assert!(!path.is_empty());
    }
}
