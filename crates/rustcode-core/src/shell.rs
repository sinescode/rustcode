//! Shell execution types — shell detection, process spawning, tree killing.
//!
//! Ported from: `packages/core/src/shell.ts` (226 lines)
//! OpenCode commit: 5d0f86606ac30690f79f0a6a9f41a1f49fe95d0b

use serde::{Deserialize, Serialize};
use std::path::PathBuf;

/// Timeout in ms before sending SIGKILL after SIGTERM.
///
/// Ported from: `shell.ts` — `SIGKILL_TIMEOUT_MS`
pub const SIGKILL_TIMEOUT_MS: u64 = 200;

/// A detected shell on the system.
///
/// Ported from: `shell.ts` — `Item`
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ShellItem {
    /// Path to the shell binary
    pub path: PathBuf,
    /// Name of the shell (e.g., "bash", "zsh")
    pub name: String,
    /// Whether this shell is acceptable for use
    pub acceptable: bool,
}

/// Metadata about a shell type.
///
/// Ported from: `shell.ts` — `META`
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ShellMeta {
    /// Shell is denied (blocklisted)
    #[serde(default, skip_serializing_if = "is_false")]
    pub deny: bool,
    /// Shell supports `--login` flag
    #[serde(default, skip_serializing_if = "is_false")]
    pub login: bool,
    /// Shell supports POSIX mode (`--posix` or `set -o posix`)
    #[serde(default, skip_serializing_if = "is_false")]
    pub posix: bool,
    /// Shell is PowerShell-based (pwsh, powershell)
    #[serde(default, skip_serializing_if = "is_false")]
    pub ps: bool,
}

fn is_false(b: &bool) -> bool {
    !b
}

/// Built-in shell metadata registry.
///
/// Ported from: `shell.ts` — `META` record
pub fn shell_meta(name: &str) -> ShellMeta {
    match name {
        "bash" => ShellMeta {
            deny: false,
            login: true,
            posix: true,
            ps: false,
        },
        "dash" => ShellMeta {
            deny: false,
            login: true,
            posix: true,
            ps: false,
        },
        "fish" => ShellMeta {
            deny: true,
            login: true,
            posix: false,
            ps: false,
        },
        "ksh" => ShellMeta {
            deny: false,
            login: true,
            posix: true,
            ps: false,
        },
        "nu" => ShellMeta {
            deny: true,
            login: false,
            posix: false,
            ps: false,
        },
        "powershell" => ShellMeta {
            deny: false,
            login: false,
            posix: false,
            ps: true,
        },
        "pwsh" => ShellMeta {
            deny: false,
            login: false,
            posix: false,
            ps: true,
        },
        "sh" => ShellMeta {
            deny: false,
            login: true,
            posix: true,
            ps: false,
        },
        "zsh" => ShellMeta {
            deny: false,
            login: true,
            posix: true,
            ps: false,
        },
        _ => ShellMeta {
            deny: true,
            login: false,
            posix: false,
            ps: false,
        },
    }
}

/// Check if a shell name is allowed.
pub fn is_shell_allowed(name: &str) -> bool {
    !shell_meta(name).deny
}

/// Shell execution environment configuration.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ShellConfig {
    /// Name/path of the preferred shell
    pub shell: Option<String>,
    /// Additional environment variables
    pub env: std::collections::HashMap<String, String>,
    /// Working directory for shell execution
    pub cwd: Option<PathBuf>,
    /// Shell initialization command (run before user command)
    pub init_command: Option<String>,
    /// Timeout for shell commands (seconds)
    pub timeout_seconds: Option<u64>,
}

impl Default for ShellConfig {
    fn default() -> Self {
        Self {
            shell: None,
            env: std::collections::HashMap::new(),
            cwd: None,
            init_command: None,
            timeout_seconds: None,
        }
    }
}

/// Result of a shell command execution.
///
/// Ported from: `shell.ts` — return type of shell execution
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ShellResult {
    /// Exit code
    pub exit_code: i32,
    /// Standard output
    pub stdout: String,
    /// Standard error
    pub stderr: String,
    /// Whether stdout was truncated
    pub stdout_truncated: bool,
    /// Whether stderr was truncated
    pub stderr_truncated: bool,
    /// Wall-clock duration in milliseconds
    pub duration_ms: u64,
    /// Whether the process was killed by signal
    pub killed: bool,
}

impl ShellResult {
    /// Check if the command succeeded (exit code 0, not killed).
    pub fn success(&self) -> bool {
        self.exit_code == 0 && !self.killed
    }
}

/// Error during shell operations.
#[derive(Debug, thiserror::Error)]
pub enum ShellError {
    #[error("shell not found: {0}")]
    NotFound(String),
    #[error("shell '{name}' is denied")]
    Denied { name: String },
    #[error("shell timeout after {seconds}s")]
    Timeout { seconds: u64 },
    #[error("I/O error: {0}")]
    Io(#[from] std::io::Error),
}

/// Common shell names to try (in order of preference).
///
/// Ported from: `shell.ts` — detection order
pub const COMMON_SHELLS: &[&str] = &[
    "bash", "zsh", "fish", "dash", "sh", "ksh", "pwsh", "powershell",
];

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_shell_meta_bash() {
        let meta = shell_meta("bash");
        assert!(!meta.deny);
        assert!(meta.login);
        assert!(meta.posix);
        assert!(!meta.ps);
    }

    #[test]
    fn test_shell_meta_zsh() {
        let meta = shell_meta("zsh");
        assert!(!meta.deny);
        assert!(meta.login);
        assert!(meta.posix);
    }

    #[test]
    fn test_shell_meta_fish_denied() {
        let meta = shell_meta("fish");
        assert!(meta.deny);
    }

    #[test]
    fn test_shell_meta_nu_denied() {
        let meta = shell_meta("nu");
        assert!(meta.deny);
    }

    #[test]
    fn test_shell_meta_powershell() {
        let meta = shell_meta("powershell");
        assert!(meta.ps);
        assert!(!meta.deny);
    }

    #[test]
    fn test_shell_meta_unknown_denied() {
        let meta = shell_meta("unknown_shell");
        assert!(meta.deny);
    }

    #[test]
    fn test_is_shell_allowed() {
        assert!(is_shell_allowed("bash"));
        assert!(is_shell_allowed("zsh"));
        assert!(!is_shell_allowed("fish"));
        assert!(!is_shell_allowed("nu"));
        assert!(!is_shell_allowed("unknown"));
    }

    #[test]
    fn test_shell_item_serde() {
        let item = ShellItem {
            path: PathBuf::from("/bin/bash"),
            name: "bash".into(),
            acceptable: true,
        };
        let json = serde_json::to_string(&item).expect("serialize");
        let parsed: ShellItem = serde_json::from_str(&json).expect("deserialize");
        assert_eq!(parsed.name, "bash");
        assert!(parsed.acceptable);
    }

    #[test]
    fn test_shell_result_success() {
        let result = ShellResult {
            exit_code: 0,
            stdout: "output".into(),
            stderr: String::new(),
            stdout_truncated: false,
            stderr_truncated: false,
            duration_ms: 150,
            killed: false,
        };
        assert!(result.success());
    }

    #[test]
    fn test_shell_result_failure_exit_code() {
        let result = ShellResult {
            exit_code: 1,
            stdout: String::new(),
            stderr: "error".into(),
            stdout_truncated: false,
            stderr_truncated: false,
            duration_ms: 100,
            killed: false,
        };
        assert!(!result.success());
    }

    #[test]
    fn test_shell_result_failure_killed() {
        let result = ShellResult {
            exit_code: 0,
            stdout: String::new(),
            stderr: String::new(),
            stdout_truncated: false,
            stderr_truncated: false,
            duration_ms: 50,
            killed: true,
        };
        assert!(!result.success());
    }

    #[test]
    fn test_shell_error_not_found() {
        let err = ShellError::NotFound("/bin/noshell".into());
        assert!(err.to_string().contains("/bin/noshell"));
    }

    #[test]
    fn test_shell_error_denied() {
        let err = ShellError::Denied {
            name: "fish".into(),
        };
        assert!(err.to_string().contains("fish"));
        assert!(err.to_string().contains("denied"));
    }

    #[test]
    fn test_shell_config_default() {
        let cfg = ShellConfig::default();
        assert!(cfg.shell.is_none());
        assert!(cfg.cwd.is_none());
        assert!(cfg.env.is_empty());
    }

    #[test]
    fn test_common_shells() {
        assert!(COMMON_SHELLS.contains(&"bash"));
        assert!(COMMON_SHELLS.contains(&"zsh"));
        // bash should be first (preferred)
        assert_eq!(COMMON_SHELLS[0], "bash");
    }
}
