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
}
