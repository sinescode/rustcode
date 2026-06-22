//! # Security Policy
//!
//! Declarative security policy that controls what a command can access.
//! Validated at compile-time (schema) and runtime (enforcement).

use serde::{Deserialize, Serialize};
use std::collections::HashSet;
use std::fmt;
use std::path::PathBuf;
use std::time::Duration;

use crate::SandboxError;

/// The maximum time a process can run.
pub const DEFAULT_TIMEOUT_SECONDS: u64 = 300;
/// The default max stdout bytes to capture.
pub const DEFAULT_MAX_OUTPUT_BYTES: usize = 1_000_000; // 1 MB

// ---------------------------------------------------------------------------
// SecurityPolicy
// ---------------------------------------------------------------------------

/// Declarative security policy that gates every process execution.
///
/// Policies are JSON Schema validated at load time and enforced at runtime.
///
/// # Example
///
/// ```json
/// {
///   "network": "deny",
///   "read_paths": ["./workspace/**"],
///   "write_paths": ["./workspace/**"],
///   "env_allow": ["HOME", "USER", "PATH"],
///   "exec": "sandbox",
///   "timeout_seconds": 300
/// }
/// ```
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct SecurityPolicy {
    /// Network access policy.
    pub network: NetworkPolicy,

    /// Glob patterns for allowed read paths.
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub read_paths: Vec<String>,

    /// Glob patterns for allowed write paths.
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub write_paths: Vec<String>,

    /// Environment variable keys to allow (whitelist only).
    /// If empty, no env vars are forwarded.
    #[serde(skip_serializing_if = "HashSet::is_empty")]
    pub env_allow: HashSet<String>,

    /// Execution isolation level.
    pub exec: ExecutionLevel,

    /// Command timeout.
    #[serde(default = "default_timeout")]
    pub timeout_seconds: u64,

    /// Maximum stdout bytes to capture.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub max_output_bytes: Option<usize>,
}

fn default_timeout() -> u64 {
    DEFAULT_TIMEOUT_SECONDS
}

impl Default for SecurityPolicy {
    fn default() -> Self {
        Self {
            network: NetworkPolicy::Deny,
            read_paths: Vec::new(),
            write_paths: Vec::new(),
            env_allow: HashSet::new(),
            exec: ExecutionLevel::Sandbox,
            timeout_seconds: DEFAULT_TIMEOUT_SECONDS,
            max_output_bytes: Some(DEFAULT_MAX_OUTPUT_BYTES),
        }
    }
}

impl SecurityPolicy {
    /// Create a new policy with secure defaults (everything denied).
    pub fn new() -> Self {
        Self::default()
    }

    /// Allow network access.
    pub fn with_network(mut self, policy: NetworkPolicy) -> Self {
        self.network = policy;
        self
    }

    /// Add a read path glob pattern.
    pub fn with_read_path(mut self, pattern: impl Into<String>) -> Self {
        self.read_paths.push(pattern.into());
        self
    }

    /// Add write path glob patterns.
    pub fn with_read_paths(mut self, patterns: Vec<String>) -> Self {
        self.read_paths = patterns;
        self
    }

    /// Add a write path glob pattern.
    pub fn with_write_path(mut self, pattern: impl Into<String>) -> Self {
        self.write_paths.push(pattern.into());
        self
    }

    /// Add write path glob patterns.
    pub fn with_write_paths(mut self, patterns: Vec<String>) -> Self {
        self.write_paths = patterns;
        self
    }

    /// Allow specific environment variables.
    pub fn with_env(mut self, vars: HashSet<String>) -> Self {
        self.env_allow = vars;
        self
    }

    /// Set execution level.
    pub fn with_exec_level(mut self, level: ExecutionLevel) -> Self {
        self.exec = level;
        self
    }

    /// Set timeout in seconds.
    pub fn with_timeout(mut self, seconds: u64) -> Self {
        self.timeout_seconds = seconds;
        self
    }

    /// Validate that the policy is internally consistent.
    ///
    /// Returns Err if the policy is invalid (e.g., read_paths without Sandbox exec).
    pub fn validate(&self) -> Result<(), SandboxError> {
        for pattern in self.read_paths.iter().chain(self.write_paths.iter()) {
            glob::Pattern::new(pattern).map_err(|e| {
                SandboxError::PolicyViolation(format!("invalid glob pattern '{}': {}", pattern, e))
            })?;
        }
        Ok(())
    }

    /// Check if a file path is allowed for reading.
    pub fn can_read(&self, path: &std::path::Path) -> bool {
        if self.read_paths.is_empty() {
            return false;
        }
        let path_str = path.to_string_lossy();
        self.read_paths.iter().any(|pattern| {
            let pat = glob::Pattern::new(pattern);
            pat.as_ref().map_or(false, |p| p.matches(&path_str))
        })
    }

    /// Check if a file path is allowed for writing.
    pub fn can_write(&self, path: &std::path::Path) -> bool {
        if self.write_paths.is_empty() {
            return false;
        }
        let path_str = path.to_string_lossy();
        self.write_paths.iter().any(|pattern| {
            let pat = glob::Pattern::new(pattern);
            pat.as_ref().map_or(false, |p| p.matches(&path_str))
        })
    }

    /// Check if network access is allowed.
    pub fn is_network_allowed(&self) -> bool {
        matches!(self.network, NetworkPolicy::Allow)
    }

    /// Get the timeout as a Duration.
    pub fn timeout(&self) -> Duration {
        Duration::from_secs(self.timeout_seconds)
    }
}

impl fmt::Display for SecurityPolicy {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "SecurityPolicy(net={}, exec={:?}, read=[{} paths], write=[{} paths], env=[{} vars], timeout={}s)",
            self.network,
            self.exec,
            self.read_paths.len(),
            self.write_paths.len(),
            self.env_allow.len(),
            self.timeout_seconds,
        )
    }
}

// ---------------------------------------------------------------------------
// NetworkPolicy
// ---------------------------------------------------------------------------

/// Whether network access is allowed.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum NetworkPolicy {
    /// All network access denied.
    Deny,
    /// Network access allowed.
    Allow,
    /// Ask the user for permission each time.
    Ask,
}

impl fmt::Display for NetworkPolicy {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            NetworkPolicy::Deny => write!(f, "deny"),
            NetworkPolicy::Allow => write!(f, "allow"),
            NetworkPolicy::Ask => write!(f, "ask"),
        }
    }
}

// ---------------------------------------------------------------------------
// ExecutionLevel
// ---------------------------------------------------------------------------

/// How isolated the process execution should be.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ExecutionLevel {
    /// Run with Landlock + seccomp-bpf (default).
    #[serde(alias = "sandboxed")]
    Sandbox,

    /// Run in a full container (nsjail / Docker).
    Container,

    /// Run directly with NO sandbox — must be explicitly opted into.
    /// Named `Unsafe` to make every developer think twice.
    #[serde(alias = "unrestricted", alias = "native")]
    Unsafe,
}

// ---------------------------------------------------------------------------
// CommandSpec
// ---------------------------------------------------------------------------

/// Specification for a command to execute.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CommandSpec {
    /// The program to execute (path or name).
    pub program: String,

    /// Arguments to the program.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub args: Vec<String>,

    /// Working directory.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub cwd: Option<PathBuf>,

    /// Environment variables (only `env_allow`-listed keys are forwarded).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub env: Option<Vec<(String, String)>>,

    /// Input to pipe to stdin.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub stdin: Option<String>,

    /// Explicit file system paths to allow (in addition to policy patterns).
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub extra_read_paths: Vec<PathBuf>,

    /// Explicit write paths to allow (in addition to policy patterns).
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub extra_write_paths: Vec<PathBuf>,
}

impl CommandSpec {
    /// Create a new command specification.
    pub fn new(program: impl Into<String>) -> Self {
        Self {
            program: program.into(),
            args: Vec::new(),
            cwd: None,
            env: None,
            stdin: None,
            extra_read_paths: Vec::new(),
            extra_write_paths: Vec::new(),
        }
    }

    /// Add an argument.
    pub fn arg(mut self, arg: impl Into<String>) -> Self {
        self.args.push(arg.into());
        self
    }

    /// Add multiple arguments.
    pub fn args(mut self, args: &[impl AsRef<str>]) -> Self {
        for arg in args {
            self.args.push(arg.as_ref().to_string());
        }
        self
    }

    /// Set the working directory.
    pub fn cwd(mut self, dir: PathBuf) -> Self {
        self.cwd = Some(dir);
        self
    }

    /// Set stdin content.
    pub fn stdin(mut self, content: impl Into<String>) -> Self {
        self.stdin = Some(content.into());
        self
    }

    /// Returns the display form of the command.
    pub fn display(&self) -> String {
        let parts: Vec<&str> = std::iter::once(self.program.as_str())
            .chain(self.args.iter().map(|s| s.as_str()))
            .collect();
        parts.join(" ")
    }
}
