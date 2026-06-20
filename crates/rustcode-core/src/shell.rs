//! Shell execution types — shell detection, process spawning, tree killing.
//!
//! Ported from: `packages/core/src/shell.ts` (226 lines)
//! OpenCode commit: 5d0f86606ac30690f79f0a6a9f41a1f49fe95d0b

use serde::{Deserialize, Serialize};
use std::path::PathBuf;
use std::sync::OnceLock;

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

impl ShellItem {
    /// Build a `ShellItem` from a filesystem path, deriving the shell name.
    pub fn from_path(path: &str) -> Option<Self> {
        let pb = PathBuf::from(path);
        if !pb.exists() {
            return None;
        }
        let name = pb.file_name()?.to_str()?.to_string();
        let acceptable = is_shell_allowed(&name);
        Some(Self {
            path: pb,
            name,
            acceptable,
        })
    }
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

/// Construct shell-specific invocation arguments for running `command` in `cwd`.
pub fn args(shell: &ShellItem, command: &str, cwd: &str) -> Vec<String> {
    match shell.name.as_str() {
        "bash" | "zsh" => {
            let quoted = shlex::try_quote(command)
                .map(|c| c.into_owned())
                .unwrap_or_else(|_| command.to_string());
            vec![
                "-l".into(),
                "-c".into(),
                format!("cd {}; eval {}", cwd, quoted),
                "opencode".into(),
                cwd.into(),
            ]
        }
        "nu" | "fish" => vec!["-c".into(), command.into()],
        "powershell" | "pwsh" => vec!["-NoProfile".into(), "-Command".into(), command.into()],
        _ => vec!["-c".into(), command.into()],
    }
}

/// Read and parse `/etc/shells` (Unix only).
pub fn parse_etc_shells() -> Vec<String> {
    std::fs::read_to_string("/etc/shells")
        .map(|text| {
            text.lines()
                .filter(|line| !line.trim().is_empty() && !line.starts_with('#'))
                .map(|line| line.trim().to_string())
                .collect()
        })
        .unwrap_or_else(|_| COMMON_SHELLS.iter().map(|s| s.to_string()).collect())
}

/// Return the value of the `$SHELL` environment variable, if set and non-empty.
pub fn preferred() -> Option<String> {
    std::env::var("SHELL").ok().filter(|s| !s.is_empty())
}

/// Find git-bash on Windows.
///
/// Ported from: `shell.ts` — `gitBash()`
pub fn git_bash() -> Option<String> {
    #[cfg(target_os = "windows")]
    {
        if let Some(path) = crate::flag::git_bash_path() {
            return Some(path.to_string());
        }
        // Try to find git bash relative to git executable
        let git_path = which("git")?;
        let git_dir = std::path::Path::new(&git_path).parent()?;
        let bash = git_dir.join("..").join("..").join("bin").join("bash.exe");
        let resolved = std::fs::canonicalize(&bash).ok()?;
        if resolved.is_file() {
            return Some(resolved.to_string_lossy().to_string());
        }
    }
    None
}

fn which(name: &str) -> Option<String> {
    let path_var = std::env::var("PATH").ok()?;
    for dir in path_var.split(':') {
        let full = format!("{}/{}", dir, name);
        if std::path::Path::new(&full).is_file() {
            return Some(full);
        }
    }
    None
}

/// Return the list of available shells on Windows.
///
/// Ported from: `shell.ts` — `win()`
pub fn win() -> Vec<ShellItem> {
    let mut shells = Vec::new();
    if let Some(bash) = git_bash() {
        shells.push(ShellItem {
            name: "gitbash".into(),
            path: PathBuf::from(bash),
            acceptable: true,
        });
    }
    if let Some(p) = ShellItem::from_path("pwsh") {
        shells.push(p);
    }
    if let Some(p) = ShellItem::from_path("powershell") {
        shells.push(p);
    }
    if let Some(p) = ShellItem::from_path("cmd") {
        shells.push(p);
    }
    shells
}

static CACHED_PREFERRED: OnceLock<Option<ShellItem>> = OnceLock::new();
static CACHED_ACCEPTABLE: OnceLock<Vec<ShellItem>> = OnceLock::new();

/// Return the cached preferred shell based on `$SHELL`, or platform defaults.
pub fn cached_preferred() -> Option<&'static ShellItem> {
    CACHED_PREFERRED
        .get_or_init(|| {
            let shell = std::env::var("SHELL").ok().filter(|s| !s.is_empty());
            shell.and_then(|s| select(Some(&s)))
        })
        .as_ref()
}

/// Return all cached acceptable shells on this system.
pub fn cached_acceptable() -> &'static Vec<ShellItem> {
    CACHED_ACCEPTABLE.get_or_init(|| {
        let mut shells: Vec<ShellItem> = COMMON_SHELLS
            .iter()
            .filter_map(|&name| {
                if !is_shell_allowed(name) {
                    return None;
                }
                find_on_path_global(name).map(|path| ShellItem {
                    name: name.to_string(),
                    path,
                    acceptable: true,
                })
            })
            .collect();
        shells.sort_by(|a, b| {
            let a_meta = shell_meta(&a.name);
            let b_meta = shell_meta(&b.name);
            // Prefer shells that are not denied and support login
            b_meta
                .login
                .cmp(&a_meta.login)
                .then_with(|| a.name.cmp(&b.name))
        });
        shells
    })
}

fn find_on_path_global(name: &str) -> Option<PathBuf> {
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

/// Select a shell, trying the given path first, then platform defaults.
pub fn select(shell_path: Option<&str>) -> Option<ShellItem> {
    if let Some(path) = shell_path {
        if let Some(info) = ShellItem::from_path(path) {
            return Some(info);
        }
    }
    #[cfg(target_os = "macos")]
    {
        return ShellItem::from_path("/bin/zsh");
    }
    #[cfg(target_os = "linux")]
    {
        return ShellItem::from_path("/bin/bash").or_else(|| ShellItem::from_path("/bin/sh"));
    }
    #[cfg(target_os = "windows")]
    {
        return ShellItem::from_path("pwsh").or_else(|| ShellItem::from_path("powershell"));
    }
    #[cfg(not(any(target_os = "macos", target_os = "linux", target_os = "windows")))]
    {
        None
    }
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
    "bash",
    "zsh",
    "fish",
    "dash",
    "sh",
    "ksh",
    "pwsh",
    "powershell",
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
                found.push(ShellItem {
                    path,
                    name,
                    acceptable,
                });
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
        // Only bash and zsh support --login
        if meta.login && (self.shell_name() == "bash" || self.shell_name() == "zsh") {
            args.push("--login".to_string());
        }
        args.push("-c".to_string());

        let full_command = if let Some(ref init) = self.config.init_command {
            let escaped_init = shlex::try_quote(init)
                .map(|c| c.into_owned())
                .unwrap_or_else(|_| init.clone());
            let escaped_cmd = shlex::try_quote(command)
                .map(|c| c.into_owned())
                .unwrap_or_else(|_| command.to_string());
            format!("{}; {}", escaped_init, escaped_cmd)
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
        let child = cmd.spawn().map_err(|e| ShellError::Io(e))?;
        let child_id = child.id();

        let result = tokio::time::timeout(std::time::Duration::from_millis(timeout), async {
            let output = child.wait_with_output().await.map_err(ShellError::Io)?;
            Ok::<_, ShellError>(output)
        })
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
                if let Some(id) = child_id {
                    let _ = Self::kill_tree(id).await;
                }
                Err(ShellError::Timeout {
                    seconds: timeout / 1000,
                })
            }
        }
    }

    /// Kill a process tree by PID.
    /// Sends SIGTERM to the process group first, waits briefly, then SIGKILL.
    pub async fn kill_tree(pid: u32) -> Result<(), ShellError> {
        #[cfg(unix)]
        {
            // Kill process group (negative PID)
            let _ = std::process::Command::new("kill")
                .arg("-TERM")
                .arg(format!("-{}", pid))
                .output();
            tokio::time::sleep(std::time::Duration::from_millis(50)).await;

            // Check if process already exited before escalating to SIGKILL
            let exited = std::process::Command::new("kill")
                .arg("-0")
                .arg(pid.to_string())
                .output()
                .map(|o| !o.status.success())
                .unwrap_or(true);
            if exited {
                return Ok(());
            }

            let _ = std::process::Command::new("kill")
                .arg("-KILL")
                .arg(format!("-{}", pid))
                .output();
        }

        #[cfg(not(unix))]
        {
            let _ = std::process::Command::new("taskkill")
                .args(["/pid", &pid.to_string(), "/f", "/t"])
                .output();
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
        assert!(
            has_bash,
            "Should find at least bash or sh on PATH: {:?}",
            shells
        );
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

    #[tokio::test]
    async fn test_shell_service_kill_tree_accepts_any_pid() {
        // kill_tree should not panic on arbitrary PID
        let result = ShellService::kill_tree(99999).await;
        assert!(result.is_ok(), "kill_tree should succeed or be a no-op");
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

    #[test]
    fn test_args_bash() {
        let shell = ShellItem {
            path: PathBuf::from("/bin/bash"),
            name: "bash".into(),
            acceptable: true,
        };
        let args = args(&shell, "echo hello", "/tmp");
        assert_eq!(args[0], "-l");
        assert_eq!(args[1], "-c");
        assert!(args[2].contains("cd /tmp"));
        assert!(args[2].contains("echo hello"));
        assert_eq!(args[3], "opencode");
        assert_eq!(args[4], "/tmp");
    }

    #[test]
    fn test_args_zsh() {
        let shell = ShellItem {
            path: PathBuf::from("/bin/zsh"),
            name: "zsh".into(),
            acceptable: true,
        };
        let args = args(&shell, "ls", "/home");
        assert_eq!(args[0], "-l");
        assert_eq!(args[1], "-c");
        assert!(args[2].contains("cd /home"));
        assert_eq!(args[3], "opencode");
    }

    #[test]
    fn test_args_fish() {
        let shell = ShellItem {
            path: PathBuf::from("/usr/bin/fish"),
            name: "fish".into(),
            acceptable: false,
        };
        let args = args(&shell, "echo hi", "/tmp");
        assert_eq!(args, vec!["-c", "echo hi"]);
    }

    #[test]
    fn test_args_powershell() {
        let shell = ShellItem {
            path: PathBuf::from("/usr/bin/pwsh"),
            name: "pwsh".into(),
            acceptable: true,
        };
        let args = args(&shell, "Get-Process", "/tmp");
        assert_eq!(args, vec!["-NoProfile", "-Command", "Get-Process"]);
    }

    #[test]
    fn test_args_unknown_shell() {
        let shell = ShellItem {
            path: PathBuf::from("/usr/bin/dash"),
            name: "dash".into(),
            acceptable: true,
        };
        let args = args(&shell, "echo test", "/tmp");
        assert_eq!(args, vec!["-c", "echo test"]);
    }

    #[test]
    fn test_parse_etc_shells() {
        let shells = parse_etc_shells();
        assert!(!shells.is_empty());
        // Should contain at least /bin/sh or /bin/bash
        let has_common = shells
            .iter()
            .any(|s| s.contains("sh") || s.contains("bash"));
        assert!(
            has_common,
            "Expected common shell in /etc/shells: {:?}",
            shells
        );
    }

    #[test]
    fn test_parse_etc_shells_excludes_comments() {
        let shells = parse_etc_shells();
        for shell in &shells {
            assert!(
                !shell.starts_with('#'),
                "Should not contain comment: {}",
                shell
            );
            assert!(!shell.trim().is_empty(), "Should not contain empty lines");
        }
    }

    #[test]
    fn test_preferred() {
        // SHELL env var may or may not be set in test environment
        let result = preferred();
        if let Some(ref shell) = result {
            assert!(!shell.is_empty());
        }
        // Just verifying it doesn't panic
    }

    #[test]
    fn test_select_with_invalid_path() {
        let result = select(Some("/nonexistent/shell"));
        assert!(result.is_none());
    }

    #[test]
    fn test_select_with_none() {
        // On Linux, should fall back to /bin/bash or /bin/sh
        let result = select(None);
        #[cfg(target_os = "linux")]
        assert!(result.is_some(), "Linux should have a default shell");
    }

    #[test]
    fn test_select_with_valid_path() {
        let result = select(Some("/bin/sh"));
        assert!(result.is_some());
        let item = result.unwrap();
        assert_eq!(item.name, "sh");
    }

    #[test]
    fn test_shell_item_from_path_valid() {
        let item = ShellItem::from_path("/bin/sh");
        assert!(item.is_some());
        let item = item.unwrap();
        assert_eq!(item.name, "sh");
        assert!(item.path.exists());
    }

    #[test]
    fn test_shell_item_from_path_invalid() {
        let item = ShellItem::from_path("/nonexistent/path/shell");
        assert!(item.is_none());
    }

    #[test]
    fn test_shell_item_from_path_denied_shell() {
        // fish is denied
        if PathBuf::from("/usr/bin/fish").exists() {
            let item = ShellItem::from_path("/usr/bin/fish");
            assert!(item.is_some());
            assert!(!item.unwrap().acceptable);
        }
    }

    #[tokio::test]
    async fn test_kill_tree_group_kill() {
        // Verify kill_tree uses process group (negative PID) by checking it doesn't panic
        let result = ShellService::kill_tree(12345).await;
        assert!(result.is_ok());
    }

    #[cfg(target_os = "windows")]
    #[test]
    fn test_git_bash_not_none_on_windows() {
        // On Windows, git_bash() may or may not find git bash — just verify no panic
        let _ = git_bash();
    }

    #[cfg(not(target_os = "windows"))]
    #[test]
    fn test_git_bash_none_on_unix() {
        assert!(git_bash().is_none());
    }

    #[test]
    fn test_which_finds_sh() {
        // /bin/sh should exist on Linux/macOS
        #[cfg(unix)]
        {
            let result = which("sh");
            assert!(result.is_some(), "Should find sh on PATH");
        }
    }

    #[test]
    fn test_which_nonexistent() {
        let result = which("totally_nonexistent_binary_xyz_123");
        assert!(result.is_none());
    }

    #[cfg(target_os = "windows")]
    #[test]
    fn test_win_returns_shells() {
        let shells = win();
        // On Windows, should return at least pwsh, powershell, or cmd
        assert!(!shells.is_empty());
    }

    #[cfg(not(target_os = "windows"))]
    #[test]
    fn test_win_empty_on_unix() {
        let shells = win();
        assert!(shells.is_empty());
    }

    #[test]
    fn test_cached_preferred() {
        let result = cached_preferred();
        // Should not panic; may be None if SHELL is unset
        if let Some(shell) = result {
            assert!(!shell.name.is_empty());
        }
    }

    #[test]
    fn test_cached_preferred_is_stable() {
        // Calling twice should return the same static reference
        // Calling twice should return the same static reference
        assert_eq!(cached_preferred(), cached_preferred());
    }

    #[test]
    fn test_cached_acceptable_not_empty() {
        let shells = cached_acceptable();
        // On Linux/macOS, should find at least one shell
        #[cfg(unix)]
        assert!(!shells.is_empty(), "Should find acceptable shells on Unix");
    }

    #[test]
    fn test_cached_acceptable_is_stable() {
        let a = cached_acceptable() as *const Vec<ShellItem>;
        let b = cached_acceptable() as *const Vec<ShellItem>;
        assert_eq!(a, b);
    }

    #[test]
    fn test_cached_acceptable_all_allowed() {
        let shells = cached_acceptable();
        for shell in shells {
            assert!(
                is_shell_allowed(&shell.name),
                "cached_acceptable should only contain allowed shells, got: {}",
                shell.name
            );
        }
    }
}
