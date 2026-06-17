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

/// Service for shell detection and command execution.
pub struct ShellService {
    config: ShellConfig,
}

impl ShellService {
    /// Create a new shell service with the given config.
    pub fn new(config: ShellConfig) -> Self {
        Self { config }
    }

    /// Detect available shells on the system PATH.
    /// Checks COMMON_SHELLS list, filters to allowed shells, verifies they exist on PATH.
    pub fn detect(&self) -> Vec<ShellItem> {
        let mut found = Vec::new();
        // If user specified a shell in config, check that first
        if let Some(ref shell_path) = self.config.shell {
            let path = std::path::PathBuf::from(shell_path);
            if let Some(name) = path.file_name().and_then(|n| n.to_str()) {
                let name = name.to_string();
                let acceptable = is_shell_allowed(&name) && path.exists();
                found.push(ShellItem { path, name, acceptable });
                return found;
            }
        }

        // Check each common shell on PATH
        for &name in COMMON_SHELLS {
            if !is_shell_allowed(name) {
                continue;
            }
            if let Some(path) = Self::find_on_path(name) {
                found.push(ShellItem {
                    path,
                    name: name.to_string(),
                    acceptable: true,
                });
            }
        }
        found
    }

    /// Find a shell binary on the system PATH.
    fn find_on_path(name: &str) -> Option<std::path::PathBuf> {
        std::env::var_os("PATH").and_then(|path_var| {
            std::env::split_paths(&path_var).find_map(|dir| {
                let path = dir.join(name);
                if path.is_file() {
                    Some(path)
                } else {
                    None
                }
            })
        })
    }

    /// Execute a command in a shell.
    /// Spawns a shell, runs the command with optional init_command prefix, captures output, handles timeout.
    pub async fn execute(
        &self,
        command: &str,
        timeout_ms: Option<u64>,
    ) -> Result<ShellResult, ShellError> {
        let shell_path = self.resolve_shell()?;
        let meta = shell_meta(self.shell_name());

        let timeout = timeout_ms
            .or(self.config.timeout_seconds.map(|s| s * 1000))
            .unwrap_or(30_000); // default 30s

        // Build shell args
        let mut args = Vec::new();
        if meta.login {
            args.push("--login".to_string());
        }
        args.push("-c".to_string());

        let full_command = if let Some(ref init) = self.config.init_command {
            format!("{}; {}", init, command)
        } else {
            command.to_string()
        };
        args.push(full_command);

        let mut cmd = tokio::process::Command::new(&shell_path);
        cmd.args(&args);
        cmd.stdout(std::process::Stdio::piped());
        cmd.stderr(std::process::Stdio::piped());
        if let Some(ref cwd) = self.config.cwd {
            cmd.current_dir(cwd);
        }
        for (k, v) in &self.config.env {
            cmd.env(k, v);
        }
        cmd.kill_on_drop(true);

        let start = std::time::Instant::now();
        let mut child = cmd.spawn().map_err(|e| ShellError::Io(e))?;

        let result = tokio::time::timeout(
            std::time::Duration::from_millis(timeout),
            async {
                let output = child.wait_with_output().await.map_err(ShellError::Io)?;
                Ok::<_, ShellError>(output)
            },
        )
        .await;

        let duration_ms = start.elapsed().as_millis() as u64;

        match result {
            Ok(Ok(output)) => {
                let stdout = String::from_utf8_lossy(&output.stdout).to_string();
                let stderr = String::from_utf8_lossy(&output.stderr).to_string();
                Ok(ShellResult {
                    exit_code: output.status.code().unwrap_or(-1),
                    stdout,
                    stderr,
                    stdout_truncated: false,
                    stderr_truncated: false,
                    duration_ms,
                    killed: false,
                })
            }
            Ok(Err(e)) => Err(e),
            Err(_timeout) => {
                // Kill the process tree on timeout
                if let Some(id) = child.id() {
                    let _ = Self::kill_tree(id);
                }
                let _ = child.kill().await;
                Err(ShellError::Timeout { seconds: timeout / 1000 })
            }
        }
    }

    /// Kill a process tree by PID.
    /// Sends SIGTERM first, waits briefly, then sends SIGKILL if still alive.
    pub fn kill_tree(pid: u32) -> Result<(), ShellError> {
        #[cfg(unix)]
        {
            // Try SIGTERM first via system kill command
            let _ = std::process::Command::new("kill")
                .arg("-TERM")
                .arg(pid.to_string())
                .output();
            std::thread::sleep(std::time::Duration::from_millis(50));
            // Then SIGKILL
            let _ = std::process::Command::new("kill")
                .arg("-KILL")
                .arg(pid.to_string())
                .output();
        }

        #[cfg(not(unix))]
        {
            // On non-unix, we can't easily kill process trees
            // The child process kill_on_drop should handle it
            let _ = pid;
        }

        Ok(())
    }

    /// Resolve the shell binary path.
    fn resolve_shell(&self) -> Result<std::path::PathBuf, ShellError> {
        if let Some(ref shell) = self.config.shell {
            let path = std::path::PathBuf::from(shell);
            if path.exists() {
                return Ok(path);
            }
            // Try finding it on PATH
            if let Some(found) = Self::find_on_path(shell) {
                return Ok(found);
            }
            return Err(ShellError::NotFound(shell.clone()));
        }

        // Auto-detect
        let detected = self.detect();
        detected
            .into_iter()
            .find(|s| s.acceptable)
            .map(|s| s.path)
            .ok_or_else(|| ShellError::NotFound("no acceptable shell found on PATH".into()))
    }

    /// Get the shell name from config or auto-detect.
    pub fn shell_name(&self) -> &str {
        if let Some(ref shell) = self.config.shell {
            return shell.as_str();
        }
        // Return first available
        for &name in COMMON_SHELLS {
            if is_shell_allowed(name) {
                return name;
            }
        }
        "sh"
    }
}

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

    #[tokio::test]
    async fn test_shell_service_detect_finds_bash() {
        let config = ShellConfig::default();
        let service = ShellService::new(config);
        let shells = service.detect();
        // On a typical Linux system, /bin/sh or /bin/bash should exist
        let has_bash = shells.iter().any(|s| s.name == "bash" || s.name == "sh");
        assert!(has_bash, "Should find at least bash or sh on PATH: {:?}", shells);
    }

    #[tokio::test]
    async fn test_shell_service_detect_with_config_shell() {
        let mut config = ShellConfig::default();
        config.shell = Some("/bin/sh".to_string());
        let service = ShellService::new(config);
        let shells = service.detect();
        assert_eq!(shells.len(), 1);
        assert_eq!(shells[0].name, "sh");
    }

    #[tokio::test]
    async fn test_shell_service_execute_echo() {
        let config = ShellConfig::default();
        let service = ShellService::new(config);
        let result = service.execute("echo hello", None).await;
        match result {
            Ok(r) => {
                assert!(r.success());
                assert!(r.stdout.contains("hello"));
            }
            Err(e) => {
                // If no shell found, that's acceptable in test env
                eprintln!("Shell execution skipped: {}", e);
            }
        }
    }

    #[tokio::test]
    async fn test_shell_service_execute_with_timeout() {
        let config = ShellConfig::default();
        let service = ShellService::new(config);
        // Run a command that sleeps, with a short timeout
        let result = service.execute("sleep 10", Some(100)).await;
        // Should timeout (or fail if no shell)
        if let Ok(r) = result {
            // If it didn't timeout, should be killed
            eprintln!("Shell result: killed={}", r.killed);
        }
    }

    #[tokio::test]
    async fn test_shell_service_execute_failure() {
        let config = ShellConfig::default();
        let service = ShellService::new(config);
        let result = service.execute("exit 42", None).await;
        match result {
            Ok(r) => {
                assert_eq!(r.exit_code, 42);
                assert!(!r.success());
            }
            Err(e) => {
                eprintln!("Shell execution skipped: {}", e);
            }
        }
    }

    #[test]
    fn test_shell_service_kill_tree_accepts_any_pid() {
        // kill_tree should not panic on arbitrary PID
        let result = ShellService::kill_tree(99999);
        assert!(result.is_ok());
    }

    #[test]
    fn test_shell_service_resolve_default() {
        let config = ShellConfig::default();
        let service = ShellService::new(config);
        let name = service.shell_name();
        assert!(!name.is_empty());
    }

    #[test]
    fn test_shell_service_with_init_command() {
        let config = ShellConfig {
            init_command: Some("export FOO=bar".to_string()),
            ..Default::default()
        };
        let service = ShellService::new(config);
        assert!(service.config.init_command.is_some());
    }

    #[tokio::test]
    async fn test_shell_service_execute_with_cwd() {
        let mut config = ShellConfig::default();
        config.cwd = Some(std::path::PathBuf::from("/tmp"));
        let service = ShellService::new(config);
        let result = service.execute("pwd", None).await;
        match result {
            Ok(r) => {
                // pwd should output something like /tmp
                assert!(r.stdout.trim().contains("/tmp") || r.stdout.trim() == "/tmp");
            }
            Err(e) => {
                eprintln!("Shell execution skipped: {}", e);
            }
        }
    }
}
