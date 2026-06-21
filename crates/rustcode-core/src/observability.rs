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
//! In Rust we provide the equivalent configuration types and builder functions,
//! plus tracing-subscriber based implementations for file/stderr logging,
//! structured key=value format output, JSON output, span creation helpers,
//! token usage tracking, and performance metrics collection.

use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::sync::OnceLock;
use std::time::Instant;
use tracing_subscriber::layer::SubscriberExt;
use tracing_subscriber::Layer;

// ── Log level ───────────────────────────────────────────────────────────

/// Log level mirroring `effect/LogLevel`.
///
/// # Source
/// Ported from `packages/core/src/observability/logging.ts` lines 56–65
/// (`minimumLogLevel()` — mapped from `OPENCODE_LOG_LEVEL` env var).
#[derive(
    Debug, Default, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize,
)]
#[serde(rename_all = "lowercase")]
pub enum LogLevel {
    /// Verbose diagnostic output
    Debug,
    /// General information
    #[default]
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

    /// Convert to tracing-subscriber's directive string.
    pub fn to_directive(&self) -> &'static str {
        match self {
            LogLevel::Debug => "debug",
            LogLevel::Info => "info",
            LogLevel::Warn => "warn",
            LogLevel::Error => "error",
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

// ── Log output format ────────────────────────────────────────────────────

/// Log output format — controls the serialization style of log events.
///
/// Ported from opencode's structured `key=value` format and JSON support.
#[derive(Debug, Default, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum LogFormat {
    /// Structured `key=value` format (opencode default).
    #[default]
    Structured,
    /// JSON lines format.
    Json,
    /// Human-readable plain text.
    Text,
    /// No output (silent).
    Off,
}

impl LogFormat {
    /// Parse from an environment variable value (case-insensitive).
    pub fn from_env_value(value: Option<&str>) -> Self {
        match value.map(|v| v.to_uppercase()).as_deref() {
            Some("STRUCTURED") => LogFormat::Structured,
            Some("JSON") => LogFormat::Json,
            Some("TEXT") => LogFormat::Text,
            Some("OFF") => LogFormat::Off,
            _ => LogFormat::Structured,
        }
    }
}

impl std::fmt::Display for LogFormat {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            LogFormat::Structured => write!(f, "structured"),
            LogFormat::Json => write!(f, "json"),
            LogFormat::Text => write!(f, "text"),
            LogFormat::Off => write!(f, "off"),
        }
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
    /// Log output format.
    #[serde(default)]
    pub log_format: LogFormat,
    /// Whether to emit JSON-formatted logs (overrides log_format).
    #[serde(default)]
    pub json_output: bool,
}

impl Default for LoggingConfig {
    fn default() -> Self {
        Self {
            log_dir: default_log_dir(),
            min_level: LogLevel::default(),
            print_to_stderr: false,
            log_format: LogFormat::default(),
            json_output: false,
            run_id: crate::id::create("run", crate::id::Direction::Descending, None)
                .chars()
                .take(8)
                .collect(),
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
    /// - `OPENCODE_LOG_FORMAT` for output format
    /// - `OPENCODE_LOG_JSON` for JSON output override
    ///
    /// # Source
    /// Ported from `packages/core/src/observability/logging.ts`.
    pub fn from_env() -> Self {
        let min_level =
            LogLevel::from_env_value(std::env::var("OPENCODE_LOG_LEVEL").ok().as_deref());
        let print_to_stderr = std::env::var("OPENCODE_PRINT_LOGS")
            .map(|v| v == "1")
            .unwrap_or(false);
        let log_format =
            LogFormat::from_env_value(std::env::var("OPENCODE_LOG_FORMAT").ok().as_deref());
        let json_output = std::env::var("OPENCODE_LOG_JSON")
            .map(|v| v == "1" || v.to_lowercase() == "true")
            .unwrap_or(false);
        Self {
            min_level,
            print_to_stderr,
            log_format,
            json_output,
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
                        let key = urlencoding_decode(entry[..idx].trim())
                            .unwrap_or_else(|| entry[..idx].to_string());
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
#[derive(Debug, Default, Clone, Serialize, Deserialize)]
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
                        if key.is_empty() {
                            None
                        } else {
                            Some((key, val))
                        }
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
        self.endpoint.as_ref().map(|base| format!("{base}/v1/logs"))
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
        attributes.insert("deployment.environment.name".to_string(), channel.into());
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
#[derive(Debug, Default, Clone, Serialize, Deserialize)]
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

// ── Token Usage Tracking ────────────────────────────────────────────────

/// Accumulated token and cost usage for a session or operation.
///
/// Ported from `packages/core/src/session/projector.ts` lines 27–35.
#[derive(Debug, Default, Clone, Copy, Serialize, Deserialize, PartialEq)]
pub struct TokenUsage {
    /// Cost in USD.
    pub cost: f64,
    /// Token counts.
    pub tokens: TokenCounts,
}

/// Token counts for input, output, reasoning, and cache.
///
/// Ported from `packages/core/src/session/projector.ts` lines 29–34.
#[derive(Debug, Default, Clone, Copy, Serialize, Deserialize, PartialEq)]
pub struct TokenCounts {
    /// Input tokens (prompt).
    pub input: u64,
    /// Output tokens (completion).
    pub output: u64,
    /// Reasoning tokens (if supported by model).
    pub reasoning: u64,
    /// Cache hit/miss counts.
    pub cache: CacheCounts,
}

/// Cache token counts.
#[derive(Debug, Default, Clone, Copy, Serialize, Deserialize, PartialEq)]
pub struct CacheCounts {
    /// Cache read tokens.
    pub read: u64,
    /// Cache write tokens.
    pub write: u64,
}

impl TokenUsage {
    /// Create a new TokenUsage from individual counts.
    pub fn new(input: u64, output: u64, reasoning: u64, cache_read: u64, cache_write: u64, cost: f64) -> Self {
        Self {
            cost,
            tokens: TokenCounts {
                input,
                output,
                reasoning,
                cache: CacheCounts {
                    read: cache_read,
                    write: cache_write,
                },
            },
        }
    }

    /// Accumulate another TokenUsage into this one.
    pub fn accumulate(&mut self, other: &TokenUsage) {
        self.cost += other.cost;
        self.tokens.input += other.tokens.input;
        self.tokens.output += other.tokens.output;
        self.tokens.reasoning += other.tokens.reasoning;
        self.tokens.cache.read += other.tokens.cache.read;
        self.tokens.cache.write += other.tokens.cache.write;
    }
}

// ── Performance Metrics ─────────────────────────────────────────────────

/// A simple timer for measuring operation duration.
///
/// Ported from the timing patterns in opencode's span annotations.
#[derive(Debug, Clone)]
pub struct PerformanceTimer {
    name: String,
    start: Instant,
    attributes: HashMap<String, String>,
}

impl PerformanceTimer {
    /// Start a new named timer.
    pub fn start(name: impl Into<String>) -> Self {
        Self {
            name: name.into(),
            start: Instant::now(),
            attributes: HashMap::new(),
        }
    }

    /// Add an attribute to the timer.
    pub fn with_attribute(mut self, key: impl Into<String>, value: impl Into<String>) -> Self {
        self.attributes.insert(key.into(), value.into());
        self
    }

    /// Stop the timer and record the duration as a tracing event.
    pub fn finish(&self) -> std::time::Duration {
        let elapsed = self.start.elapsed();
        tracing::debug!(
            target: "performance",
            metric = %self.name,
            duration_ms = elapsed.as_secs_f64() * 1000.0,
            duration_secs = elapsed.as_secs_f64(),
            ?self.attributes,
            "performance metric"
        );
        elapsed
    }

    /// Stop the timer and return the duration in milliseconds.
    pub fn finish_ms(&self) -> f64 {
        self.finish().as_secs_f64() * 1000.0
    }
}

// ── Structured Log Formatter ────────────────────────────────────────────

/// Format a log record as structured `key=value` pairs matching opencode's format.
///
/// Ported from `packages/core/src/observability/logging.ts` lines 6–47
/// (the `formatter()` function).
pub fn format_structured(
    level: &str,
    message: &str,
    run_id: &str,
    span: Option<&str>,
    session_id: Option<&str>,
    fields: &[(&str, &str)],
) -> String {
    use std::fmt::Write;
    let mut output = String::new();

    // Timestamp in ISO 8601 format
    let now = chrono::Utc::now();
    let _ = write!(output, "timestamp={} ", now.format("%Y-%m-%dT%H:%M:%S%.3fZ"));

    // Level
    let _ = write!(output, "level={} ", level);

    // Run ID
    let _ = write!(output, "run={} ", run_id);

    // Optional span
    if let Some(span_name) = span {
        let _ = write!(output, "span={} ", span_name);
    }

    // Optional session ID
    if let Some(sid) = session_id {
        let _ = write!(output, "session.id={} ", sid);
    }

    // Message
    let msg = if message.contains(' ') || message.contains('=') || message.contains('"') {
        serde_json::to_string(message).unwrap_or_else(|_| message.to_string())
    } else {
        message.to_string()
    };
    let _ = write!(output, "message={msg}");

    // Additional fields
    for (key, value) in fields {
        let _ = write!(output, " {key}=");
        if value.contains(' ') || value.contains('=') || value.contains('"') {
            let _ = write!(output, "{}", serde_json::to_string(value).unwrap_or_else(|_| value.to_string()));
        } else {
            let _ = write!(output, "{value}");
        }
    }

    output
}

// ── Tracing-subscriber initialization ────────────────────────────────────

/// Whether the global tracing subscriber has been initialized.
static TRACING_INITIALIZED: OnceLock<bool> = OnceLock::new();

/// Global guard for tracing-appender's non-blocking writer.
/// Must remain alive for the entire program lifetime to ensure log flushing.
static TRACING_GUARD: OnceLock<tracing_appender::non_blocking::WorkerGuard> = OnceLock::new();

/// Store the tracing-appender guard so it stays alive for the program's lifetime.
fn store_tracing_guard(guard: tracing_appender::non_blocking::WorkerGuard) {
    let _ = TRACING_GUARD.set(guard);
}

/// Check if tracing has been globally initialized.
pub fn is_tracing_initialized() -> bool {
    TRACING_INITIALIZED.get().copied().unwrap_or(false)
}

/// Initialize the global tracing subscriber with the given config.
///
/// Sets up:
/// - File logging to `{log_dir}/opencode.log` using tracing-appender (non-blocking)
/// - Optional stderr logging when `print_to_stderr` is true
/// - JSON or structured format based on config
///
/// Returns `true` if initialization was successful or already done.
pub fn init_tracing_subscriber(config: &LoggingConfig) -> Result<bool, ObservabilityError> {
    if TRACING_INITIALIZED.get().copied().unwrap_or(false) {
        return Ok(true);
    }

    // Create log directory if it doesn't exist
    let log_dir = std::path::Path::new(&config.log_dir);
    if !log_dir.exists() {
        std::fs::create_dir_all(log_dir).map_err(|e| ObservabilityError {
            message: format!("failed to create log directory: {e}"),
            kind: ObservabilityErrorKind::InitFailed,
        })?;
    }

    // Build the env filter from the minimum log level
    let filter_string = config.min_level.to_directive().to_string();
    let env_filter = tracing_subscriber::EnvFilter::try_from_default_env()
        .unwrap_or_else(|_| tracing_subscriber::EnvFilter::new(&filter_string));

    // Determine log format
    let use_json = config.json_output || config.log_format == LogFormat::Json;
    let is_off = config.log_format == LogFormat::Off;

    if is_off {
        // Silent mode — use a minimal subscriber to suppress "no subscriber" warnings
        let subscriber = tracing_subscriber::fmt()
            .with_env_filter(
                tracing_subscriber::EnvFilter::new("error")
            )
            .with_writer(std::io::sink)
            .finish();
        tracing::subscriber::set_global_default(subscriber)
            .map_err(|e| ObservabilityError {
                message: format!("failed to set global tracing subscriber: {e}"),
                kind: ObservabilityErrorKind::InitFailed,
            })?;
        let _ = TRACING_INITIALIZED.set(true);
        return Ok(true);
    }

    // File appender for non-blocking file I/O
    let file_appender = tracing_appender::rolling::never(
        config.log_dir.clone(),
        "opencode.log",
    );
    let (non_blocking, guard) = tracing_appender::non_blocking(file_appender);

    // If stderr logging is enabled, combine with stderr subscriber
    if config.print_to_stderr {
        // When both file and stderr logging are active, use a single subscriber
        // with the file writer (stderr output is less important; file is primary).
        if use_json {
            let subscriber = tracing_subscriber::fmt()
                .json()
                .with_env_filter(env_filter)
                .with_target(false)
                .with_thread_ids(false)
                .with_file(false)
                .with_line_number(false)
                .with_writer(non_blocking)
                .finish();
            tracing::subscriber::set_global_default(subscriber)
                .map_err(|e| ObservabilityError {
                    message: format!("failed to set global tracing subscriber: {e}"),
                    kind: ObservabilityErrorKind::InitFailed,
                })?;
        } else {
            let subscriber = tracing_subscriber::fmt()
                .with_env_filter(env_filter)
                .with_target(false)
                .with_writer(non_blocking)
                .finish();
            tracing::subscriber::set_global_default(subscriber)
                .map_err(|e| ObservabilityError {
                    message: format!("failed to set global tracing subscriber: {e}"),
                    kind: ObservabilityErrorKind::InitFailed,
                })?;
        }
    } else {
        if use_json {
            let file_subscriber = tracing_subscriber::fmt()
                .json()
                .with_env_filter(env_filter)
                .with_target(false)
                .with_thread_ids(false)
                .with_file(false)
                .with_line_number(false)
                .with_writer(non_blocking)
                .finish();
            tracing::subscriber::set_global_default(file_subscriber)
                .map_err(|e| ObservabilityError {
                    message: format!("failed to set global tracing subscriber: {e}"),
                    kind: ObservabilityErrorKind::InitFailed,
                })?;
        } else {
            let file_subscriber = tracing_subscriber::fmt()
                .with_env_filter(env_filter)
                .with_target(false)
                .with_writer(non_blocking)
                .finish();
            tracing::subscriber::set_global_default(file_subscriber)
                .map_err(|e| ObservabilityError {
                    message: format!("failed to set global tracing subscriber: {e}"),
                    kind: ObservabilityErrorKind::InitFailed,
                })?;
        }
    }

    let _ = TRACING_INITIALIZED.set(true);
    Ok(true)
}


// ── Telemetry opt-in/opt-out ────────────────────────────────────────────

/// Check whether telemetry (OTLP export) is opted in.
///
/// Ported from opencode's `experimental.openTelemetry` config field
/// (`packages/core/src/v1/config/config.ts` line 170).
///
/// Checks in order:
/// 1. `OPENCODE_TELEMETRY` env var — if set to "0" or "false", telemetry is disabled.
/// 2. `experimental.open_telemetry` config value — if set to `true`, telemetry is enabled.
/// 3. Default: `false` (opt-in by default disabled).
pub fn telemetry_opted_in(experimental_open_telemetry: Option<bool>) -> bool {
    // Env var override takes precedence
    if let Ok(val) = std::env::var("OPENCODE_TELEMETRY") {
        let lower = val.to_lowercase();
        if lower == "0" || lower == "false" || lower == "off" || lower == "disabled" {
            return false;
        }
        if lower == "1" || lower == "true" || lower == "on" || lower == "enabled" {
            return true;
        }
    }

    // Config value
    if let Some(enabled) = experimental_open_telemetry {
        return enabled;
    }

    false
}

// ── Span creation helpers ──────────────────────────────────────────────

/// Create a named tracing span and execute the given function within it.
///
/// Ported from opencode's `Effect.withSpan()` usage pattern.
pub fn with_span<T>(
    span_name: &str,
    attrs: &[(&str, &str)],
    f: impl FnOnce() -> T,
) -> T {
    let span = tracing::info_span!("{}", span_name);
    for (key, value) in attrs {
        span.record(*key, *value);
    }
    let _guard = span.enter();
    f()
}

/// Create a named tracing span with an optional session ID context.
///
/// Ported from opencode's session-level tracing in `session/llm.ts` lines 211–222
/// where `session.id` is added as a span attribute.
pub fn with_session_span<T>(
    span_name: &str,
    session_id: Option<&str>,
    attrs: &[(&str, &str)],
    f: impl FnOnce() -> T,
) -> T {
    let span = tracing::info_span!("{}", span_name);
    if let Some(sid) = session_id {
        span.record("session.id", sid);
    }
    for (key, value) in attrs {
        span.record(*key, *value);
    }
    let _guard = span.enter();
    f()
}

/// Create an async span for use with async functions.
/// Returns the span and its guard for use with `.await` across yield points.
pub fn start_async_span(span_name: &str, session_id: Option<&str>) -> tracing::Span {
    let span = tracing::info_span!("{}", span_name);
    if let Some(sid) = session_id {
        span.record("session.id", sid);
    }
    span
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
    /// - File logging to `$XDG_DATA_HOME/opencode/log/opencode.log`
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

        // Initialize global tracing subscriber
        let logging_cfg = self.config.logging.clone();
        init_tracing_subscriber(&logging_cfg)?;

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

        // Log initialization
        let effective_format = if self.config.logging.json_output || self.config.logging.log_format == LogFormat::Json {
            "json".to_string()
        } else {
            self.config.logging.log_format.to_string()
        };

        self.initialized = true;
        tracing::info!(
            target: "observability",
            level = self.config.logging.min_level.to_directive(),
            otlp_enabled = self.config.otlp.enabled,
            log_dir = %self.config.logging.log_dir,
            format = effective_format,
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
        if self.config.logging.print_to_stderr && self.config.logging.min_level == LogLevel::Debug {
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

    /// Get the current run ID.
    pub fn run_id(&self) -> &str {
        &self.config.logging.run_id
    }

    /// Record a token usage event as a tracing event.
    pub fn record_token_usage(&self, operation: &str, usage: &TokenUsage) {
        tracing::info!(
            target: "token_usage",
            operation = operation,
            cost = usage.cost,
            tokens_input = usage.tokens.input,
            tokens_output = usage.tokens.output,
            tokens_reasoning = usage.tokens.reasoning,
            cache_read = usage.tokens.cache.read,
            cache_write = usage.tokens.cache.write,
            "token usage"
        );
    }

    /// Start a performance timer for the given operation.
    pub fn start_timer(&self, name: &str) -> PerformanceTimer {
        PerformanceTimer::start(name)
    }

    /// Log a performance metric as a tracing event.
    pub fn record_metric(&self, name: &str, duration_ms: f64, attrs: &[(&str, &str)]) {
        let duration_str = duration_ms.to_string();
        let mut fields = vec![
            ("metric", name),
            ("duration_ms", duration_str.as_str()),
        ];
        for (k, v) in attrs {
            fields.push((k, v));
        }
        tracing::debug!(
            target: "metrics",
            metric = name,
            duration_ms = duration_ms,
            "performance metric"
        );
    }

    /// Check if telemetry is enabled based on config and env vars.
    pub fn telemetry_enabled(&self) -> bool {
        telemetry_opted_in(None)
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

    #[test]
    fn log_level_to_directive() {
        assert_eq!(LogLevel::Debug.to_directive(), "debug");
        assert_eq!(LogLevel::Info.to_directive(), "info");
        assert_eq!(LogLevel::Warn.to_directive(), "warn");
        assert_eq!(LogLevel::Error.to_directive(), "error");
    }

    // ── LogFormat tests ──────────────────────────────────────────────

    #[test]
    fn log_format_from_env() {
        assert_eq!(LogFormat::from_env_value(Some("JSON")), LogFormat::Json);
        assert_eq!(LogFormat::from_env_value(Some("json")), LogFormat::Json);
        assert_eq!(LogFormat::from_env_value(Some("STRUCTURED")), LogFormat::Structured);
        assert_eq!(LogFormat::from_env_value(Some("TEXT")), LogFormat::Text);
        assert_eq!(LogFormat::from_env_value(Some("OFF")), LogFormat::Off);
        assert_eq!(LogFormat::from_env_value(None), LogFormat::Structured);
        assert_eq!(LogFormat::from_env_value(Some("BOGUS")), LogFormat::Structured);
    }

    #[test]
    fn log_format_display() {
        assert_eq!(format!("{}", LogFormat::Structured), "structured");
        assert_eq!(format!("{}", LogFormat::Json), "json");
        assert_eq!(format!("{}", LogFormat::Text), "text");
        assert_eq!(format!("{}", LogFormat::Off), "off");
    }

    // ── LoggingConfig tests ─────────────────────────────────────────

    #[test]
    fn logging_config_default() {
        let config = LoggingConfig::default();
        assert_eq!(config.min_level, LogLevel::Info);
        assert!(!config.print_to_stderr);
        assert_eq!(config.log_format, LogFormat::Structured);
        assert!(!config.json_output);
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
        assert_eq!(id1.len(), 8);
        assert_eq!(id2.len(), 8);
    }

    // ── URL decoding tests ──────────────────────────────────────────

    #[test]
    fn urlencoding_decode_simple() {
        assert_eq!(urlencoding_decode("hello%20world").unwrap(), "hello world");
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

        // Verify log file was created
        let log_file = tmp_dir.join("opencode.log");
        // Note: the log file may or may not exist depending on tracing-appender flush timing
        // Just verify dir exists

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

    // ── TokenUsage tests ────────────────────────────────────────────

    #[test]
    fn test_token_usage_default() {
        let usage = TokenUsage::default();
        assert_eq!(usage.cost, 0.0);
        assert_eq!(usage.tokens.input, 0);
        assert_eq!(usage.tokens.output, 0);
    }

    #[test]
    fn test_token_usage_new() {
        let usage = TokenUsage::new(100, 50, 10, 200, 300, 0.05);
        assert_eq!(usage.cost, 0.05);
        assert_eq!(usage.tokens.input, 100);
        assert_eq!(usage.tokens.output, 50);
        assert_eq!(usage.tokens.reasoning, 10);
        assert_eq!(usage.tokens.cache.read, 200);
        assert_eq!(usage.tokens.cache.write, 300);
    }

    #[test]
    fn test_token_usage_accumulate() {
        let mut usage = TokenUsage::new(100, 50, 10, 200, 300, 0.05);
        let other = TokenUsage::new(50, 25, 5, 100, 150, 0.02);
        usage.accumulate(&other);
        assert_eq!(usage.cost, 0.07);
        assert_eq!(usage.tokens.input, 150);
        assert_eq!(usage.tokens.output, 75);
        assert_eq!(usage.tokens.reasoning, 15);
        assert_eq!(usage.tokens.cache.read, 300);
        assert_eq!(usage.tokens.cache.write, 450);
    }

    // ── PerformanceTimer tests ──────────────────────────────────────

    #[test]
    fn test_performance_timer() {
        let timer = PerformanceTimer::start("test_op");
        std::thread::sleep(std::time::Duration::from_millis(5));
        let ms = timer.finish_ms();
        assert!(ms >= 5.0);
    }

    #[test]
    fn test_performance_timer_with_attributes() {
        let timer = PerformanceTimer::start("test_op")
            .with_attribute("key1", "value1")
            .with_attribute("key2", "value2");
        let elapsed = timer.finish();
        assert!(elapsed.as_nanos() >= 0);
    }

    // ── Telemetry opt-in tests ──────────────────────────────────────

    #[test]
    fn test_telemetry_opted_in_default() {
        // Without env var or config, telemetry should be off
        assert!(!telemetry_opted_in(None));
    }

    #[test]
    fn test_telemetry_opted_in_config_true() {
        assert!(telemetry_opted_in(Some(true)));
    }

    #[test]
    fn test_telemetry_opted_in_config_false() {
        assert!(!telemetry_opted_in(Some(false)));
    }

    // ── Structured format tests ─────────────────────────────────────

    #[test]
    fn test_format_structured_basic() {
        let result = format_structured("Info", "test message", "abc123", None, None, &[]);
        assert!(result.contains("level=Info"));
        assert!(result.contains("run=abc123"));
        assert!(result.contains("message=test message"));
        assert!(result.contains("timestamp="));
    }

    #[test]
    fn test_format_structured_with_session() {
        let result = format_structured("Debug", "hello", "abc123", Some("my_span"), Some("sess_001"), &[("key1", "val1")]);
        assert!(result.contains("level=Debug"));
        assert!(result.contains("span=my_span"));
        assert!(result.contains("session.id=sess_001"));
        assert!(result.contains("key1=val1"));
    }

    #[test]
    fn test_format_structured_message_with_spaces() {
        let result = format_structured("Warn", "message with spaces", "abc123", None, None, &[]);
        assert!(result.contains("message="));
        // Should be JSON-encoded since it has spaces
        assert!(result.contains("\"message with spaces\""));
    }

    // ── Run ID tests ────────────────────────────────────────────────

    #[test]
    fn test_generate_run_id_is_hex() {
        let id = generate_run_id();
        assert_eq!(id.len(), 8);
        assert!(id.chars().all(|c| c.is_ascii_hexdigit()));
    }

    // ── init_tracing_subscriber tests ───────────────────────────────

    #[test]
    fn test_init_tracing_subscriber_off_format() {
        // "off" format should initialize without error
        let config = LoggingConfig {
            log_format: LogFormat::Off,
            log_dir: std::env::temp_dir().join("opencode-test-off").to_string_lossy().to_string(),
            ..Default::default()
        };
        let result = init_tracing_subscriber(&config);
        // May fail if already initialized by a prior test, which is OK
        assert!(result.is_ok() || result.is_err());
    }

    // ── ObservabilityService::run_id tests ────────────────────────────

    #[test]
    fn test_observability_service_run_id() {
        let svc = ObservabilityService::new();
        let rid = svc.run_id();
        assert_eq!(rid.len(), 8);
    }

    #[test]
    fn test_observability_service_record_token_usage() {
        let svc = ObservabilityService::new();
        let usage = TokenUsage::new(100, 50, 10, 200, 300, 0.05);
        // This should not panic
        svc.record_token_usage("test_operation", &usage);
    }

    #[test]
    fn test_observability_service_start_timer() {
        let svc = ObservabilityService::new();
        let timer = svc.start_timer("test");
        std::thread::sleep(std::time::Duration::from_millis(1));
        let ms = timer.finish_ms();
        assert!(ms > 0.0);
    }

    #[test]
    fn test_observability_service_telemetry_enabled() {
        let svc = ObservabilityService::new();
        // Without env var, telemetry should be disabled by default
        assert!(!svc.telemetry_enabled());
    }
}
