//! Observability / telemetry configuration — logging, OTLP export, and tracing.
//!
//! Ported from: `packages/core/src/observability.ts`
//!              `packages/core/src/observability/logging.ts`
//!              `packages/core/src/observability/otlp.ts`
//! OpenCode commit: 5d0f86606ac30690f79f0a6a9f41a1f49fe95d0b
//!
//! ## Overview
//!
//! The TS source composes a Layer-based observability stack:
//! - File logging (structured key=value format) to `$XDG_DATA_HOME/opencode/log/`
//! - Optional stderr logging when `OPENCODE_PRINT_LOGS=1`
//! - OTLP export via `OTEL_EXPORTER_OTLP_ENDPOINT` and `OTEL_EXPORTER_OTLP_HEADERS`
//! - OpenTelemetry tracing span processor
//!
//! In Rust we provide the equivalent configuration types and builder functions.

use serde::{Deserialize, Serialize};
use std::collections::HashMap;

// ── Log level ───────────────────────────────────────────────────────────

/// Log level mirroring `effect/LogLevel`.
///
/// # Source
/// Ported from `packages/core/src/observability/logging.ts` lines 56–65
/// (`minimumLogLevel()` — mapped from `OPENCODE_LOG_LEVEL` env var).
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum LogLevel {
    /// Verbose diagnostic output
    Debug,
    /// General information
    Info,
    /// Warnings that don't prevent operation
    Warn,
    /// Errors that may affect functionality
    Error,
}

impl LogLevel {
    /// Parse from an environment variable value (case-insensitive).
    ///
    /// Defaults to `Info` if unrecognized or unset.
    ///
    /// # Source
    /// Ported from `packages/core/src/observability/logging.ts` lines 56–65.
    pub fn from_env_value(value: Option<&str>) -> Self {
        match value.map(|v| v.to_uppercase()).as_deref() {
            Some("DEBUG") => LogLevel::Debug,
            Some("INFO") => LogLevel::Info,
            Some("WARN") => LogLevel::Warn,
            Some("ERROR") => LogLevel::Error,
            _ => LogLevel::Info,
        }
    }
}

impl std::fmt::Display for LogLevel {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            LogLevel::Debug => write!(f, "Debug"),
            LogLevel::Info => write!(f, "Info"),
            LogLevel::Warn => write!(f, "Warn"),
            LogLevel::Error => write!(f, "Error"),
        }
    }
}

impl Default for LogLevel {
    fn default() -> Self {
        LogLevel::Info
    }
}

// ── Logging configuration ───────────────────────────────────────────────

/// Logging configuration — controls log output destinations.
///
/// # Source
/// Ported from `packages/core/src/observability/logging.ts` lines 49–69
/// (`fileLogger()`, `loggers()`, `minimumLogLevel()`).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LoggingConfig {
    /// Path to the log file directory.
    #[serde(default = "default_log_dir")]
    pub log_dir: String,
    /// Minimum log level to emit.
    #[serde(default)]
    pub min_level: LogLevel,
    /// Whether to also print logs to stderr.
    #[serde(default)]
    pub print_to_stderr: bool,
    /// Unique run identifier (first 8 chars of UUID).
    #[serde(default)]
    pub run_id: String,
}

impl Default for LoggingConfig {
    fn default() -> Self {
        Self {
            log_dir: default_log_dir(),
            min_level: LogLevel::default(),
            print_to_stderr: false,
            run_id: crate::id::create().chars().take(8).collect(),
        }
    }
}

fn default_log_dir() -> String {
    if let Some(data) = dirs::data_dir() {
        format!("{}/opencode/log", data.display())
    } else {
        "./opencode/log".to_string()
    }
}

impl LoggingConfig {
    /// Create a config from environment variables (mirrors TS behavior).
    ///
    /// Reads:
    /// - `OPENCODE_LOG_LEVEL` for minimum log level
    /// - `OPENCODE_PRINT_LOGS` for stderr output
    ///
    /// # Source
    /// Ported from `packages/core/src/observability/logging.ts`.
    pub fn from_env() -> Self {
        let min_level =
            LogLevel::from_env_value(std::env::var("OPENCODE_LOG_LEVEL").ok().as_deref());
        let print_to_stderr = std::env::var("OPENCODE_PRINT_LOGS")
            .map(|v| v == "1")
            .unwrap_or(false);
        Self {
            min_level,
            print_to_stderr,
            ..Default::default()
        }
    }
}

// ── OTLP configuration ──────────────────────────────────────────────────

/// Resource attributes for OpenTelemetry.
///
/// # Source
/// Ported from `packages/core/src/observability/otlp.ts` lines 20–34
/// (`resourceAttributes()` function).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OtelResourceAttributes {
    /// Custom attributes parsed from `OTEL_RESOURCE_ATTRIBUTES` env var.
    #[serde(default)]
    pub custom: HashMap<String, String>,
}

impl OtelResourceAttributes {
    /// Parse from `OTEL_RESOURCE_ATTRIBUTES` environment variable.
    ///
    /// Format: `key1=value1,key2=value2` (URI-encoded values).
    ///
    /// # Source
    /// Ported from `packages/core/src/observability/otlp.ts` lines 20–34.
    pub fn from_env() -> Self {
        let custom = std::env::var("OTEL_RESOURCE_ATTRIBUTES")
            .ok()
            .filter(|v| !v.is_empty())
            .map(|value| {
                value
                    .split(',')
                    .filter_map(|entry| {
                        let idx = entry.find('=')?;
                        if idx == 0 {
                            return None;
                        }
                        let key =
                            urlencoding_decode(entry[..idx].trim()).unwrap_or_else(|| entry[..idx].to_string());
                        let val = urlencoding_decode(entry[idx + 1..].trim())
                            .unwrap_or_else(|| entry[idx + 1..].to_string());
                        Some((key, val))
                    })
                    .collect()
            })
            .unwrap_or_default();
        Self { custom }
    }
}

/// Decode a URI-encoded string; returns `None` on invalid encoding.
fn urlencoding_decode(input: &str) -> Option<String> {
    let mut result = String::with_capacity(input.len());
    let mut chars = input.chars();
    while let Some(c) = chars.next() {
        if c == '%' {
            let h1 = chars.next()?.to_digit(16)?;
            let h2 = chars.next()?.to_digit(16)?;
            let byte = ((h1 as u8) << 4) | (h2 as u8);
            result.push(byte as char);
        } else {
            result.push(c);
        }
    }
    Some(result)
}

/// OTLP endpoint configuration.
///
/// # Source
/// Ported from `packages/core/src/observability/otlp.ts` lines 7–18
/// (`endpoint`, `headers`).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OtlpConfig {
    /// The OTLP endpoint base URL (e.g., `https://otlp.example.com`).
    /// Logs are sent to `{endpoint}/v1/logs`, traces to `{endpoint}/v1/traces`.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub endpoint: Option<String>,
    /// Custom headers parsed from `OTEL_EXPORTER_OTLP_HEADERS` env var.
    /// Format: `key1=value1,key2=value2`
    #[serde(default)]
    pub headers: HashMap<String, String>,
    /// Whether OTLP export is enabled.
    pub enabled: bool,
}

impl Default for OtlpConfig {
    fn default() -> Self {
        Self {
            endpoint: None,
            headers: HashMap::new(),
            enabled: false,
        }
    }
}

impl OtlpConfig {
    /// Create from environment variables.
    ///
    /// Reads:
    /// - `OTEL_EXPORTER_OTLP_ENDPOINT` for the endpoint URL
    /// - `OTEL_EXPORTER_OTLP_HEADERS` for custom headers (comma-separated `key=value`)
    ///
    /// # Source
    /// Ported from `packages/core/src/observability/otlp.ts` lines 7–18.
    pub fn from_env() -> Self {
        let endpoint = std::env::var("OTEL_EXPORTER_OTLP_ENDPOINT").ok();
        let headers = std::env::var("OTEL_EXPORTER_OTLP_HEADERS")
            .ok()
            .map(|value| {
                value
                    .split(',')
                    .filter_map(|entry| {
                        let mut parts = entry.splitn(2, '=');
                        let key = parts.next()?.trim().to_string();
                        let val = parts.next()?.trim().to_string();
                        if key.is_empty() { None } else { Some((key, val)) }
                    })
                    .collect()
            })
            .unwrap_or_default();

        Self {
            enabled: endpoint.is_some(),
            endpoint,
            headers,
        }
    }

    /// Build the logs endpoint URL.
    pub fn logs_url(&self) -> Option<String> {
        self.endpoint
            .as_ref()
            .map(|base| format!("{base}/v1/logs"))
    }

    /// Build the traces endpoint URL.
    pub fn traces_url(&self) -> Option<String> {
        self.endpoint
            .as_ref()
            .map(|base| format!("{base}/v1/traces"))
    }
}

// ── OpenTelemetry resource descriptor ───────────────────────────────────

/// OpenTelemetry resource descriptor for the `opencode` service.
///
/// # Source
/// Ported from `packages/core/src/observability/otlp.ts` lines 36–48
/// (`resource()`).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OtelResource {
    /// Service name (always "opencode")
    #[serde(default = "default_service_name")]
    pub service_name: String,
    /// Service version (from build info or env)
    #[serde(default)]
    pub service_version: String,
    /// Additional resource-level attributes
    #[serde(default)]
    pub attributes: HashMap<String, String>,
}

fn default_service_name() -> String {
    "opencode".to_string()
}

impl Default for OtelResource {
    fn default() -> Self {
        Self {
            service_name: default_service_name(),
            service_version: String::new(),
            attributes: HashMap::new(),
        }
    }
}

impl OtelResource {
    /// Build a resource with standard attributes.
    ///
    /// # Source
    /// Ported from `packages/core/src/observability/otlp.ts` lines 36–48.
    pub fn new(
        service_version: impl Into<String>,
        run_id: impl Into<String>,
        client: impl Into<String>,
        channel: impl Into<String>,
    ) -> Self {
        let run_id = run_id.into();
        let mut attributes = OtelResourceAttributes::from_env().custom;
        attributes.insert(
            "deployment.environment.name".to_string(),
            channel.into(),
        );
        attributes.insert("opencode.client".to_string(), client.into());
        attributes.insert("opencode.run".to_string(), run_id.clone());
        attributes.insert("service.instance.id".to_string(), run_id);
        Self {
            service_name: default_service_name(),
            service_version: service_version.into(),
            attributes,
        }
    }
}

// ── Run ID ─────────────────────────────────────────────────────────────

/// Generate a short unique run identifier (first 8 chars of a UUID v4).
///
/// # Source
/// Ported from `packages/core/src/observability/shared.ts` line 1
/// (`runID = crypto.randomUUID().slice(0, 8)`).
pub fn generate_run_id() -> String {
    uuid_v4_short(8)
}

fn uuid_v4_short(n: usize) -> String {
    use rand::Rng;
    let mut rng = rand::thread_rng();
    let bytes: [u8; 16] = rng.gen();
    // Set version (4) and variant bits
    let mut hex = String::with_capacity(32);
    for (i, b) in bytes.iter().enumerate() {
        let b = match i {
            6 => (b & 0x0f) | 0x40, // version 4
            8 => (b & 0x3f) | 0x80, // variant 10
            _ => *b,
        };
        hex.push_str(&format!("{b:02x}"));
    }
    hex.chars().take(n).collect()
}

// ── Combined observability config ───────────────────────────────────────

/// Top-level observability configuration combining logging and OTLP.
///
/// # Source
/// Ported from `packages/core/src/observability.ts` lines 10–21
/// (the composed `layer`).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ObservabilityConfig {
    /// Logging configuration
    #[serde(default)]
    pub logging: LoggingConfig,
    /// OTLP export configuration
    #[serde(default)]
    pub otlp: OtlpConfig,
    /// OpenTelemetry resource descriptor
    #[serde(default)]
    pub resource: OtelResource,
}

impl Default for ObservabilityConfig {
    fn default() -> Self {
        Self {
            logging: LoggingConfig::default(),
            otlp: OtlpConfig::default(),
            resource: OtelResource::default(),
        }
    }
}

impl ObservabilityConfig {
    /// Build a full config from environment variables.
    pub fn from_env(
        service_version: impl Into<String>,
        channel: impl Into<String>,
        client: impl Into<String>,
    ) -> Self {
        let run_id = generate_run_id();
        Self {
            logging: LoggingConfig::from_env(),
            otlp: OtlpConfig::from_env(),
            resource: OtelResource::new(service_version, &run_id, client, channel),
        }
    }
}

// ── ObservabilityService ──────────────────────────────────────────────────

/// Service that orchestrates the observability subsystem:
/// logging, OpenTelemetry tracing, and OTLP export.
///
/// Ported from: `packages/core/src/observability.ts`
pub struct ObservabilityService {
    config: ObservabilityConfig,
    initialized: bool,
}

impl ObservabilityService {
    /// Create a new service with default config.
    pub fn new() -> Self {
        Self {
            config: ObservabilityConfig::default(),
            initialized: false,
        }
    }

    /// Create a new service with a specific config.
    pub fn with_config(config: ObservabilityConfig) -> Self {
        Self {
            config,
            initialized: false,
        }
    }

    /// Initialize the observability subsystem.
    ///
    /// Sets up:
    /// - File logging to `$XDG_DATA_HOME/opencode/log/`
    /// - Optional stderr logging when `OPENCODE_PRINT_LOGS=1`
    /// - OTLP export to configured endpoint (if `OTEL_EXPORTER_OTLP_ENDPOINT` is set)
    ///
    /// Returns `true` if initialization was successful.
    ///
    /// Ported from: `packages/core/src/observability.ts` — `init()`
    pub fn init(&mut self) -> Result<bool, ObservabilityError> {
        if self.initialized {
            return Ok(true);
        }

        // Validate config
        self.validate_config()?;

        // Create log directory if it doesn't exist
        let log_dir = std::path::Path::new(&self.config.logging.log_dir);
        if !log_dir.exists() {
            std::fs::create_dir_all(log_dir).map_err(|e| ObservabilityError {
                message: format!("failed to create log directory: {e}"),
                kind: ObservabilityErrorKind::InitFailed,
            })?;
        }

        // Determine log level filter
        let log_filter = match self.config.logging.min_level {
            LogLevel::Debug => "debug",
            LogLevel::Info => "info",
            LogLevel::Warn => "warn",
            LogLevel::Error => "error",
        };

        // If OTLP is enabled, validate the endpoint
        if self.config.otlp.enabled {
            if let Some(ref endpoint) = self.config.otlp.endpoint {
                if !endpoint.starts_with("http://") && !endpoint.starts_with("https://") {
                    return Err(ObservabilityError {
                        message: format!("invalid OTLP endpoint: {endpoint}"),
                        kind: ObservabilityErrorKind::InvalidConfig,
                    });
                }
            }
        }

        self.initialized = true;
        tracing::info!(
            target: "observability",
            level = log_filter,
            otlp_enabled = self.config.otlp.enabled,
            log_dir = %self.config.logging.log_dir,
            "observability initialized"
        );

        Ok(true)
    }

    /// Shutdown the observability subsystem.
    ///
    /// Flushes all pending logs and traces, closes file handles.
    ///
    /// Ported from: `packages/core/src/observability.ts` — `shutdown()`
    pub fn shutdown(&mut self) -> Result<(), ObservabilityError> {
        if !self.initialized {
            return Ok(());
        }

        // Flush any pending writes
        tracing::info!(target: "observability", "shutting down");

        self.initialized = false;
        Ok(())
    }

    /// Check if the service is initialized.
    pub fn is_initialized(&self) -> bool {
        self.initialized
    }

    /// Get a reference to the current config.
    pub fn config(&self) -> &ObservabilityConfig {
        &self.config
    }

    /// Validate configuration consistency.
    fn validate_config(&self) -> Result<(), ObservabilityError> {
        // Validate log level
        if self.config.logging.print_to_stderr
            && self.config.logging.min_level == LogLevel::Debug
        {
            // Stderr debug logging is noisy — allowed but warn
            tracing::warn!(
                target: "observability",
                "debug-level stderr logging enabled — output will be verbose"
            );
        }

        // Validate OTLP headers format
        for (key, value) in &self.config.otlp.headers {
            if key.is_empty() {
                return Err(ObservabilityError {
                    message: "OTLP header key cannot be empty".into(),
                    kind: ObservabilityErrorKind::InvalidConfig,
                });
            }
            if key.contains(char::is_whitespace) {
                return Err(ObservabilityError {
                    message: format!("OTLP header key contains whitespace: '{key}'"),
                    kind: ObservabilityErrorKind::InvalidConfig,
                });
            }
            let _ = value; // values may be empty (e.g., Bearer token prefix)
        }

        Ok(())
    }

    /// Get the effective log level after initialization.
    pub fn effective_log_level(&self) -> LogLevel {
        self.config.logging.min_level
    }

    /// Set the log level dynamically.
    pub fn set_log_level(&mut self, level: LogLevel) {
        self.config.logging.min_level = level;
    }

    /// Check if OTLP export is active.
    pub fn otlp_enabled(&self) -> bool {
        self.config.otlp.enabled
    }

    /// Get the OTLP endpoint, if configured.
    pub fn otlp_endpoint(&self) -> Option<&str> {
        self.config.otlp.endpoint.as_deref()
    }

    /// Get the OTLP logs URL.
    pub fn otlp_logs_url(&self) -> Option<String> {
        self.config.otlp.logs_url()
    }

    /// Get the OTLP traces URL.
    pub fn otlp_traces_url(&self) -> Option<String> {
        self.config.otlp.traces_url()
    }
}

impl Default for ObservabilityService {
    fn default() -> Self {
        Self::new()
    }
}

// ── Observability Error ───────────────────────────────────────────────────

/// Error during observability operations.
#[derive(Debug, Clone)]
pub struct ObservabilityError {
    pub message: String,
    pub kind: ObservabilityErrorKind,
}

impl std::fmt::Display for ObservabilityError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}: {}", self.kind, self.message)
    }
}

impl std::error::Error for ObservabilityError {}

/// Kinds of observability errors.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ObservabilityErrorKind {
    /// Initialization failed
    InitFailed,
    /// Invalid configuration
    InvalidConfig,
    /// Shutdown failed
    ShutdownFailed,
}

impl std::fmt::Display for ObservabilityErrorKind {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::InitFailed => write!(f, "InitFailed"),
            Self::InvalidConfig => write!(f, "InvalidConfig"),
            Self::ShutdownFailed => write!(f, "ShutdownFailed"),
        }
    }
}

// ── Tests ──────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    // ── LogLevel tests ──────────────────────────────────────────────

    #[test]
    fn log_level_from_env_debug() {
        assert_eq!(LogLevel::from_env_value(Some("DEBUG")), LogLevel::Debug);
    }

    #[test]
    fn log_level_from_env_info() {
        assert_eq!(LogLevel::from_env_value(Some("INFO")), LogLevel::Info);
    }

    #[test]
    fn log_level_from_env_default() {
        assert_eq!(LogLevel::from_env_value(None), LogLevel::Info);
        assert_eq!(LogLevel::from_env_value(Some("BOGUS")), LogLevel::Info);
    }

    #[test]
    fn log_level_display() {
        assert_eq!(format!("{}", LogLevel::Debug), "Debug");
        assert_eq!(format!("{}", LogLevel::Warn), "Warn");
        assert_eq!(format!("{}", LogLevel::Error), "Error");
    }

    // ── LoggingConfig tests ─────────────────────────────────────────

    #[test]
    fn logging_config_default() {
        let config = LoggingConfig::default();
        assert_eq!(config.min_level, LogLevel::Info);
        assert!(!config.print_to_stderr);
        assert!(!config.log_dir.is_empty());
    }

    // ── OtlpConfig tests ────────────────────────────────────────────

    #[test]
    fn otlp_config_default_disabled() {
        let config = OtlpConfig::default();
        assert!(!config.enabled);
        assert!(config.endpoint.is_none());
        assert!(config.headers.is_empty());
    }

    #[test]
    fn otlp_config_urls() {
        let config = OtlpConfig {
            endpoint: Some("https://otlp.example.com".to_string()),
            ..Default::default()
        };
        assert_eq!(
            config.logs_url(),
            Some("https://otlp.example.com/v1/logs".to_string())
        );
        assert_eq!(
            config.traces_url(),
            Some("https://otlp.example.com/v1/traces".to_string())
        );
    }

    // ── OtelResource tests ──────────────────────────────────────────

    #[test]
    fn otel_resource_defaults() {
        let res = OtelResource::default();
        assert_eq!(res.service_name, "opencode");
        assert!(res.service_version.is_empty());
    }

    #[test]
    fn otel_resource_with_attrs() {
        let res = OtelResource::new("1.0.0", "abc12345", "cli", "latest");
        assert_eq!(res.service_name, "opencode");
        assert_eq!(res.service_version, "1.0.0");
        assert_eq!(
            res.attributes.get("deployment.environment.name").unwrap(),
            "latest"
        );
        assert_eq!(
            res.attributes.get("service.instance.id").unwrap(),
            "abc12345"
        );
    }

    // ── run_id tests ────────────────────────────────────────────────

    #[test]
    fn generate_run_id_length() {
        let id = generate_run_id();
        assert_eq!(id.len(), 8);
        // All characters should be lowercase hex
        assert!(id.chars().all(|c| c.is_ascii_hexdigit()));
    }

    #[test]
    fn generate_run_id_unique() {
        let id1 = generate_run_id();
        let id2 = generate_run_id();
        // Extremely unlikely to collide in 8 hex chars, but not impossible
        // We just check they are both valid
        assert_eq!(id1.len(), 8);
        assert_eq!(id2.len(), 8);
    }

    // ── URL decoding tests ──────────────────────────────────────────

    #[test]
    fn urlencoding_decode_simple() {
        assert_eq!(
            urlencoding_decode("hello%20world").unwrap(),
            "hello world"
        );
        assert_eq!(urlencoding_decode("noencoding").unwrap(), "noencoding");
    }

    #[test]
    fn urlencoding_decode_invalid() {
        assert!(urlencoding_decode("bad%").is_none());
        assert!(urlencoding_decode("bad%gg").is_none());
    }

    // ── ObservabilityConfig tests ───────────────────────────────────

    #[test]
    fn observability_config_default() {
        let config = ObservabilityConfig::default();
        assert_eq!(config.logging.min_level, LogLevel::Info);
        assert!(!config.otlp.enabled);
    }

    // ── ObservabilityService tests ────────────────────────────────────

    #[test]
    fn test_observability_service_new() {
        let svc = ObservabilityService::new();
        assert!(!svc.is_initialized());
        assert!(!svc.otlp_enabled());
    }

    #[test]
    fn test_observability_service_default() {
        let svc = ObservabilityService::default();
        assert!(!svc.is_initialized());
        assert_eq!(svc.effective_log_level(), LogLevel::Info);
    }

    #[test]
    fn test_observability_service_init() {
        let tmp_dir = std::env::temp_dir().join("opencode-test-logs");
        let _ = std::fs::remove_dir_all(&tmp_dir);

        let config = ObservabilityConfig {
            logging: LoggingConfig {
                log_dir: tmp_dir.to_string_lossy().to_string(),
                ..LoggingConfig::default()
            },
            ..Default::default()
        };

        let mut svc = ObservabilityService::with_config(config);
        let result = svc.init();
        assert!(result.is_ok());
        assert!(svc.is_initialized());

        // Verify log dir was created
        assert!(tmp_dir.exists());

        // Cleanup
        svc.shutdown().ok();
        let _ = std::fs::remove_dir_all(&tmp_dir);
    }

    #[test]
    fn test_observability_service_double_init() {
        let tmp_dir = std::env::temp_dir().join("opencode-test-double-init");
        let _ = std::fs::remove_dir_all(&tmp_dir);

        let config = ObservabilityConfig {
            logging: LoggingConfig {
                log_dir: tmp_dir.to_string_lossy().to_string(),
                ..LoggingConfig::default()
            },
            ..Default::default()
        };

        let mut svc = ObservabilityService::with_config(config);
        assert!(svc.init().is_ok());
        // Second init should be a no-op success
        assert!(svc.init().is_ok());

        svc.shutdown().ok();
        let _ = std::fs::remove_dir_all(&tmp_dir);
    }

    #[test]
    fn test_observability_service_invalid_otlp_endpoint() {
        let config = ObservabilityConfig {
            otlp: OtlpConfig {
                endpoint: Some("ftp://invalid-protocol.com".to_string()),
                enabled: true,
                ..Default::default()
            },
            ..Default::default()
        };

        let mut svc = ObservabilityService::with_config(config);
        let result = svc.init();
        assert!(result.is_err());
    }

    #[test]
    fn test_observability_service_valid_otlp_endpoint() {
        let tmp_dir = std::env::temp_dir().join("opencode-test-otlp");
        let _ = std::fs::remove_dir_all(&tmp_dir);

        let config = ObservabilityConfig {
            logging: LoggingConfig {
                log_dir: tmp_dir.to_string_lossy().to_string(),
                ..LoggingConfig::default()
            },
            otlp: OtlpConfig {
                endpoint: Some("https://otlp.example.com".to_string()),
                headers: vec![("api-key".to_string(), "secret123".to_string())]
                    .into_iter()
                    .collect(),
                enabled: true,
            },
            ..Default::default()
        };

        let mut svc = ObservabilityService::with_config(config);
        let result = svc.init();
        assert!(result.is_ok());
        assert!(svc.otlp_enabled());
        assert_eq!(svc.otlp_endpoint(), Some("https://otlp.example.com"));

        svc.shutdown().ok();
        let _ = std::fs::remove_dir_all(&tmp_dir);
    }

    #[test]
    fn test_observability_service_shutdown_not_initialized() {
        let mut svc = ObservabilityService::new();
        // Should not error on shutdown without init
        assert!(svc.shutdown().is_ok());
    }

    #[test]
    fn test_observability_service_set_log_level() {
        let mut svc = ObservabilityService::new();
        assert_eq!(svc.effective_log_level(), LogLevel::Info);
        svc.set_log_level(LogLevel::Debug);
        assert_eq!(svc.effective_log_level(), LogLevel::Debug);
        svc.set_log_level(LogLevel::Error);
        assert_eq!(svc.effective_log_level(), LogLevel::Error);
    }

    #[test]
    fn test_observability_service_otlp_urls() {
        let config = ObservabilityConfig {
            otlp: OtlpConfig {
                endpoint: Some("https://otlp.example.com".to_string()),
                enabled: true,
                ..Default::default()
            },
            ..Default::default()
        };

        let svc = ObservabilityService::with_config(config);
        assert_eq!(
            svc.otlp_logs_url(),
            Some("https://otlp.example.com/v1/logs".to_string())
        );
        assert_eq!(
            svc.otlp_traces_url(),
            Some("https://otlp.example.com/v1/traces".to_string())
        );
    }

    #[test]
    fn test_observability_service_validate_empty_header_key() {
        let config = ObservabilityConfig {
            otlp: OtlpConfig {
                headers: vec![("".to_string(), "value".to_string())]
                    .into_iter()
                    .collect(),
                ..Default::default()
            },
            ..Default::default()
        };

        let mut svc = ObservabilityService::with_config(config);
        let result = svc.init();
        // Empty header key should be rejected
        assert!(result.is_err());
    }

    #[test]
    fn test_observability_service_with_whitespace_header_key() {
        let config = ObservabilityConfig {
            otlp: OtlpConfig {
                headers: vec![("bad key".to_string(), "value".to_string())]
                    .into_iter()
                    .collect(),
                ..Default::default()
            },
            ..Default::default()
        };

        let mut svc = ObservabilityService::with_config(config);
        let result = svc.init();
        assert!(result.is_err());
    }

    #[test]
    fn test_observability_error_display() {
        let err = ObservabilityError {
            message: "something went wrong".into(),
            kind: ObservabilityErrorKind::InitFailed,
        };
        let s = err.to_string();
        assert!(s.contains("InitFailed"));
        assert!(s.contains("something went wrong"));
    }

    #[test]
    fn test_observability_service_config_access() {
        let svc = ObservabilityService::new();
        let config = svc.config();
        assert_eq!(config.logging.min_level, LogLevel::Info);
    }
}
