//! Process management types — spawn, run, stream, abort.
//!
//! Ported from: `packages/core/src/process.ts` (236 lines)
//! OpenCode commit: 5d0f86606ac30690f79f0a6a9f41a1f49fe95d0b

use serde::{Deserialize, Serialize};

// ---------------------------------------------------------------------------
// Error types
// ---------------------------------------------------------------------------

/// Error from a child process.
///
/// Ported from: `process.ts` — `AppProcessError`
#[derive(Debug, Clone, Serialize, Deserialize, thiserror::Error)]
#[error("process '{command}' exited with code {exit_code:?}")]
pub struct AppProcessError {
    /// The command that was executed
    pub command: String,
    /// Exit code (None if killed by signal)
    pub exit_code: Option<i32>,
    /// Captured stderr
    #[serde(skip_serializing_if = "Option::is_none")]
    pub stderr: Option<String>,
    /// Underlying cause
    #[serde(skip)]
    pub cause: Option<String>,
}

// ---------------------------------------------------------------------------
// Run options
// ---------------------------------------------------------------------------

/// Options for running a process.
///
/// Ported from: `process.ts` — `RunOptions`
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct RunOptions {
    /// Maximum stdout bytes to capture
    #[serde(skip_serializing_if = "Option::is_none")]
    pub max_output_bytes: Option<usize>,
    /// Maximum stderr bytes to capture
    #[serde(skip_serializing_if = "Option::is_none")]
    pub max_error_bytes: Option<usize>,
    /// Input to pipe to stdin
    #[serde(skip_serializing_if = "Option::is_none")]
    pub stdin: Option<StdinInput>,
    /// Timeout in milliseconds
    #[serde(skip_serializing_if = "Option::is_none")]
    pub timeout_ms: Option<u64>,
}

/// Input to pipe to a process's stdin.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(untagged)]
pub enum StdinInput {
    /// Text input
    Text(String),
    /// Binary input (base64 encoded in serde)
    Binary(Vec<u8>),
}

/// Options for running a process in streaming mode.
///
/// Ported from: `process.ts` — `RunStreamOptions`
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct RunStreamOptions {
    /// Include stderr in the output stream
    #[serde(default, skip_serializing_if = "is_false")]
    pub include_stderr: bool,
    /// Exit codes considered successful (default: just [0])
    #[serde(skip_serializing_if = "Option::is_none")]
    pub ok_exit_codes: Option<Vec<i32>>,
    /// Maximum error bytes to buffer
    #[serde(skip_serializing_if = "Option::is_none")]
    pub max_error_bytes: Option<usize>,
}

fn is_false(b: &bool) -> bool {
    !b
}

// ---------------------------------------------------------------------------
// Run result
// ---------------------------------------------------------------------------

/// Result of a completed process run.
///
/// Ported from: `process.ts` — `RunResult`
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RunResult {
    /// The command that was executed
    pub command: String,
    /// Exit code
    pub exit_code: i32,
    /// Standard output (may be truncated)
    pub stdout: Vec<u8>,
    /// Standard error (may be truncated)
    pub stderr: Vec<u8>,
    /// Whether stdout was truncated
    pub stdout_truncated: bool,
    /// Whether stderr was truncated
    pub stderr_truncated: bool,
}

impl RunResult {
    /// Check if the process exited successfully.
    pub fn success(&self) -> bool {
        self.exit_code == 0
    }

    /// Get stdout as a UTF-8 string, if possible.
    pub fn stdout_str(&self) -> Option<&str> {
        std::str::from_utf8(&self.stdout).ok()
    }

    /// Get stderr as a UTF-8 string, if possible.
    pub fn stderr_str(&self) -> Option<&str> {
        std::str::from_utf8(&self.stderr).ok()
    }
}

/// Require a successful exit code, returning the result or an error.
///
/// Ported from: `process.ts` — `requireSuccess()`
pub fn require_success(result: RunResult) -> Result<RunResult, AppProcessError> {
    if result.exit_code == 0 {
        Ok(result)
    } else {
        Err(AppProcessError {
            command: result.command.clone(),
            exit_code: Some(result.exit_code),
            stderr: result.stderr_str().map(|s| s.to_string()),
            cause: None,
        })
    }
}

/// Require the exit code to be in a specific set.
///
/// Ported from: `process.ts` — `requireExitIn()`
pub fn require_exit_in(
    result: RunResult,
    ok_codes: &[i32],
) -> Result<RunResult, AppProcessError> {
    if ok_codes.contains(&result.exit_code) {
        Ok(result)
    } else {
        Err(AppProcessError {
            command: result.command.clone(),
            exit_code: Some(result.exit_code),
            stderr: result.stderr_str().map(|s| s.to_string()),
            cause: None,
        })
    }
}

// ---------------------------------------------------------------------------
// Process command
// ---------------------------------------------------------------------------

/// A command to execute as a child process.
///
/// Ported from: `process.ts` — `ChildProcess.Command`
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProcessCommand {
    /// The executable to run
    pub command: String,
    /// Arguments to pass
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub args: Vec<String>,
    /// Environment variables to set/override
    #[serde(default, skip_serializing_if = "std::collections::HashMap::is_empty")]
    pub env: std::collections::HashMap<String, String>,
    /// Working directory
    #[serde(skip_serializing_if = "Option::is_none")]
    pub cwd: Option<String>,
}

impl ProcessCommand {
    /// Create a simple command with no arguments.
    pub fn new(command: impl Into<String>) -> Self {
        Self {
            command: command.into(),
            args: Vec::new(),
            env: std::collections::HashMap::new(),
            cwd: None,
        }
    }

    /// Add an argument.
    pub fn arg(mut self, arg: impl Into<String>) -> Self {
        self.args.push(arg.into());
        self
    }

    /// Set the working directory.
    pub fn cwd(mut self, dir: impl Into<String>) -> Self {
        self.cwd = Some(dir.into());
        self
    }
}

impl std::fmt::Display for ProcessCommand {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.command)?;
        for arg in &self.args {
            write!(f, " {arg}")?;
        }
        Ok(())
    }
}

// ---------------------------------------------------------------------------
// Process status
// ---------------------------------------------------------------------------

/// Status of a child process.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum ProcessStatus {
    Running,
    Exited,
    Killed,
    Errored,
}

// ---------------------------------------------------------------------------
// ProcessService
// ---------------------------------------------------------------------------

/// Service for spawning and managing child processes.
///
/// Ported from: `process.ts` — main service
pub struct ProcessService;

impl ProcessService {
    /// Internal helper: spawn a child process from a ProcessCommand.
    fn spawn_child(cmd: &ProcessCommand) -> Result<tokio::process::Child, std::io::Error> {
        let mut c = tokio::process::Command::new(&cmd.command);
        c.args(&cmd.args);
        c.stdout(std::process::Stdio::piped());
        c.stderr(std::process::Stdio::piped());
        c.stdin(std::process::Stdio::piped());
        c.kill_on_drop(true);

        if let Some(ref cwd) = cmd.cwd {
            c.current_dir(cwd);
        }
        for (k, v) in &cmd.env {
            c.env(k, v);
        }
        c.spawn()
    }

    /// Run a process to completion, capturing stdout and stderr.
    ///
    /// Ported from: `process.ts` — `run()`
    pub async fn run(
        cmd: &ProcessCommand,
        opts: &RunOptions,
    ) -> Result<RunResult, AppProcessError> {
        let mut child = Self::spawn_child(cmd).map_err(|e| AppProcessError {
            command: cmd.to_string(),
            exit_code: None,
            stderr: None,
            cause: Some(e.to_string()),
        })?;

        let timeout_ms = opts.timeout_ms.unwrap_or(0);
        let max_output = opts.max_output_bytes.unwrap_or(usize::MAX);
        let max_error = opts.max_error_bytes.unwrap_or(usize::MAX);

        // Write stdin if provided
        if let Some(ref stdin_input) = opts.stdin {
            use std::io::Write;
            if let Some(mut stdin) = child.stdin.take() {
                match stdin_input {
                    StdinInput::Text(s) => {
                        let _ = stdin.write_all(s.as_bytes());
                    }
                    StdinInput::Binary(b) => {
                        let _ = stdin.write_all(b);
                    }
                }
            }
        }

        let result = if timeout_ms > 0 {
            match tokio::time::timeout(
                std::time::Duration::from_millis(timeout_ms),
                child.wait_with_output(),
            )
            .await
            {
                Ok(Ok(output)) => Ok(output),
                Ok(Err(e)) => Err(e),
                Err(_) => {
                    child.kill().await.ok();
                    return Err(AppProcessError {
                        command: cmd.to_string(),
                        exit_code: None,
                        stderr: Some("process timed out".into()),
                        cause: Some(format!("timed out after {timeout_ms}ms")),
                    });
                }
            }
        } else {
            child
                .wait_with_output()
                .await
                .map_err(|e| AppProcessError {
                    command: cmd.to_string(),
                    exit_code: None,
                    stderr: None,
                    cause: Some(e.to_string()),
                })
        };

        match result {
            Ok(output) => {
                let stdout_len = output.stdout.len();
                let stderr_len = output.stderr.len();
                let stdout = if stdout_len > max_output {
                    output.stdout[..max_output].to_vec()
                } else {
                    output.stdout
                };
                let stderr = if stderr_len > max_error {
                    output.stderr[..max_error].to_vec()
                } else {
                    output.stderr
                };

                Ok(RunResult {
                    command: cmd.to_string(),
                    exit_code: output.status.code().unwrap_or(-1),
                    stdout,
                    stderr,
                    stdout_truncated: stdout_len > max_output,
                    stderr_truncated: stderr_len > max_error,
                })
            }
            Err(e) => Err(AppProcessError {
                command: cmd.to_string(),
                exit_code: None,
                stderr: None,
                cause: Some(e.to_string()),
            }),
        }
    }

    /// Run a process and stream output lines as they become available.
    ///
    /// Returns a stream of (line, is_stderr) tuples and the child process handle.
    ///
    /// Ported from: `process.ts` — `runStream()`
    pub async fn run_stream(
        cmd: &ProcessCommand,
        opts: &RunStreamOptions,
    ) -> Result<
        (
            impl futures::Stream<Item = Result<(String, bool), std::io::Error>>,
            tokio::process::Child,
        ),
        AppProcessError,
    > {
        use futures::StreamExt;
        use tokio::io::{AsyncBufReadExt, BufReader};

        let mut child = Self::spawn_child(cmd).map_err(|e| AppProcessError {
            command: cmd.to_string(),
            exit_code: None,
            stderr: None,
            cause: Some(e.to_string()),
        })?;

        let stdout = child.stdout.take();
        let stderr = child.stderr.take();

        let max_error = opts.max_error_bytes.unwrap_or(1024 * 1024);

        let (tx, rx) = tokio::sync::mpsc::channel::<Result<(String, bool), std::io::Error>>(64);

        // Spawn stdout reader
        if let Some(stdout) = stdout {
            let tx = tx.clone();
            tokio::spawn(async move {
                let reader = BufReader::new(stdout);
                let mut lines = reader.lines();
                while let Ok(Some(line)) = lines.next_line().await {
                    if tx.send(Ok((line, false))).await.is_err() {
                        break;
                    }
                }
            });
        }

        // Spawn stderr reader
        if let Some(stderr) = stderr {
            let tx = tx.clone();
            let include_stderr = opts.include_stderr;
            tokio::spawn(async move {
                let reader = BufReader::new(stderr);
                let mut lines = reader.lines();
                let mut total: usize = 0;
                while let Ok(Some(line)) = lines.next_line().await {
                    total += line.len() + 1;
                    if total > max_error {
                        break;
                    }
                    if include_stderr {
                        if tx.send(Ok((line, true))).await.is_err() {
                            break;
                        }
                    }
                }
            });
        }

        let stream = tokio_stream::wrappers::ReceiverStream::new(rx);
        Ok((stream, child))
    }

    /// Validate that a process result has a successful exit code.
    ///
    /// Ported from: `process.ts` — `requireSuccess()`
    pub fn require_success(result: &RunResult) -> Result<(), AppProcessError> {
        if result.exit_code == 0 {
            Ok(())
        } else {
            Err(AppProcessError {
                command: result.command.clone(),
                exit_code: Some(result.exit_code),
                stderr: result.stderr_str().map(|s| s.to_string()),
                cause: None,
            })
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_run_result_success() {
        let result = RunResult {
            command: "echo hi".into(),
            exit_code: 0,
            stdout: b"hi\n".to_vec(),
            stderr: vec![],
            stdout_truncated: false,
            stderr_truncated: false,
        };
        assert!(result.success());
    }

    #[test]
    fn test_run_result_stdout_str() {
        let result = RunResult {
            command: "echo".into(),
            exit_code: 0,
            stdout: b"hello".to_vec(),
            stderr: vec![],
            stdout_truncated: false,
            stderr_truncated: false,
        };
        assert_eq!(result.stdout_str(), Some("hello"));
    }

    #[test]
    fn test_require_success_ok() {
        let result = RunResult {
            command: "ls".into(),
            exit_code: 0,
            stdout: vec![],
            stderr: vec![],
            stdout_truncated: false,
            stderr_truncated: false,
        };
        let checked = require_success(result);
        assert!(checked.is_ok());
    }

    #[test]
    fn test_require_success_fail() {
        let result = RunResult {
            command: "bad".into(),
            exit_code: 1,
            stdout: vec![],
            stderr: b"error".to_vec(),
            stdout_truncated: false,
            stderr_truncated: false,
        };
        let checked = require_success(result);
        assert!(checked.is_err());
    }

    #[test]
    fn test_require_exit_in() {
        let result = RunResult {
            command: "cmd".into(),
            exit_code: 2,
            stdout: vec![],
            stderr: vec![],
            stdout_truncated: false,
            stderr_truncated: false,
        };
        // 2 is in the ok list
        let checked = require_exit_in(result, &[0, 2]);
        assert!(checked.is_ok());
    }

    #[test]
    fn test_process_command_display() {
        let cmd = ProcessCommand::new("rg")
            .arg("--json")
            .arg("pattern");
        let display = cmd.to_string();
        assert!(display.contains("rg"));
        assert!(display.contains("--json"));
        assert!(display.contains("pattern"));
    }

    #[test]
    fn test_process_command_default_args() {
        let cmd = ProcessCommand::new("echo");
        assert!(cmd.args.is_empty());
        assert!(cmd.env.is_empty());
        assert!(cmd.cwd.is_none());
    }

    #[test]
    fn test_run_options_default() {
        let opts = RunOptions::default();
        assert!(opts.max_output_bytes.is_none());
        assert!(opts.stdin.is_none());
        assert!(opts.timeout_ms.is_none());
    }

    #[test]
    fn test_run_stream_options_default() {
        let opts = RunStreamOptions::default();
        assert!(!opts.include_stderr);
    }

    #[test]
    fn test_app_process_error_display() {
        let err = AppProcessError {
            command: "bad_cmd".into(),
            exit_code: Some(127),
            stderr: Some("command not found".into()),
            cause: None,
        };
        let msg = err.to_string();
        assert!(msg.contains("bad_cmd"));
    }

    #[test]
    fn test_process_status_serde() {
        for status in [ProcessStatus::Running, ProcessStatus::Exited] {
            let json = serde_json::to_string(&status).expect("serialize");
            let parsed: ProcessStatus =
                serde_json::from_str(&json).expect("deserialize");
            assert_eq!(parsed, status);
        }
    }

    #[tokio::test]
    async fn test_process_service_run_echo() {
        let cmd = ProcessCommand::new("echo").arg("hello");
        let opts = RunOptions::default();
        let result = ProcessService::run(&cmd, &opts).await.expect("should run echo");
        assert_eq!(result.exit_code, 0);
        let stdout = result.stdout_str().expect("valid utf8");
        assert!(stdout.contains("hello"));
    }

    #[test]
    fn test_process_service_require_success_ok() {
        let result = RunResult {
            command: "ls".into(),
            exit_code: 0,
            stdout: vec![],
            stderr: vec![],
            stdout_truncated: false,
            stderr_truncated: false,
        };
        assert!(ProcessService::require_success(&result).is_ok());
    }

    #[test]
    fn test_process_service_require_success_fail() {
        let result = RunResult {
            command: "bad".into(),
            exit_code: 1,
            stdout: vec![],
            stderr: b"error".to_vec(),
            stdout_truncated: false,
            stderr_truncated: false,
        };
        assert!(ProcessService::require_success(&result).is_err());
    }

    #[tokio::test]
    async fn test_process_service_run_nonexistent() {
        let cmd = ProcessCommand::new("nonexistent_command_xyz_123");
        let opts = RunOptions::default();
        let result = ProcessService::run(&cmd, &opts).await;
        assert!(result.is_err());
    }

    #[tokio::test]
    async fn test_process_service_run_with_timeout() {
        let cmd = ProcessCommand::new("sleep").arg("10");
        let opts = RunOptions {
            timeout_ms: Some(100),
            ..RunOptions::default()
        };
        let result = ProcessService::run(&cmd, &opts).await;
        assert!(result.is_err());
        let err = result.unwrap_err();
        assert_eq!(err.stderr.as_deref(), Some("process timed out"));
    }

    #[tokio::test]
    async fn test_process_service_run_stream() {
        use futures::StreamExt;
        let cmd = ProcessCommand::new("echo").arg("hello");
        let opts = RunStreamOptions::default();
        let (mut stream, mut child) = ProcessService::run_stream(&cmd, &opts)
            .await
            .expect("should start streaming");
        let mut lines = Vec::new();
        while let Some(item) = stream.next().await {
            if let Ok((line, _is_stderr)) = item {
                lines.push(line);
            }
        }
        child.wait().await.expect("process should exit");
        assert!(!lines.is_empty());
        assert!(lines.iter().any(|l| l.contains("hello")));
    }

    #[tokio::test]
    async fn test_process_service_run_with_env() {
        let mut cmd = ProcessCommand::new("sh");
        cmd.args = vec!["-c".into(), "echo $MY_VAR".into()];
        cmd.env = {
            let mut env = std::collections::HashMap::new();
            env.insert("MY_VAR".into(), "test_value".into());
            env
        };
        let opts = RunOptions::default();
        let result = ProcessService::run(&cmd, &opts).await.expect("should run");
        assert_eq!(result.exit_code, 0);
        let stdout = result.stdout_str().expect("valid utf8");
        assert!(stdout.contains("test_value"));
    }
}
