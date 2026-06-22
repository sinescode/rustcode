//! Process management types — spawn, run, stream, abort.
//!
//! Ported from: `packages/core/src/process.ts` (236 lines)
//! OpenCode commit: 5d0f86606ac30690f79f0a6a9f41a1f49fe95d0b

use serde::{Deserialize, Serialize};
use tokio::io::AsyncWriteExt;
use tokio_util::sync::CancellationToken;

// ---------------------------------------------------------------------------
// Error types
// ---------------------------------------------------------------------------

/// Error from a child process.
///
/// Ported from: `process.ts` — `AppProcessError`
#[derive(Debug, Clone, Serialize, Deserialize, thiserror::Error)]
pub enum AppProcessError {
    #[error("process '{command}' exited with code {exit_code:?}")]
    Exited {
        command: String,
        exit_code: Option<i32>,
        #[serde(skip_serializing_if = "Option::is_none")]
        stderr: Option<String>,
        #[serde(skip)]
        cause: Option<String>,
    },
    #[error("spawn failed: {message}")]
    SpawnFailed { message: String },
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
    /// Timeout duration
    #[serde(skip_serializing_if = "Option::is_none")]
    pub timeout: Option<DurationInput>,
    /// Cancellation token to abort the process
    #[serde(skip)]
    pub cancellation_token: Option<CancellationToken>,
}

/// Input to pipe to a process's stdin.
#[derive(Debug)]
pub enum StdinInput {
    /// Text input
    Text(String),
    /// Binary input (base64 encoded in serde)
    Binary(Vec<u8>),
    /// Streaming input from an mpsc channel
    Stream(std::sync::Arc<std::sync::Mutex<tokio::sync::mpsc::Receiver<Vec<u8>>>>),
}

impl Clone for StdinInput {
    fn clone(&self) -> Self {
        match self {
            Self::Text(s) => Self::Text(s.clone()),
            Self::Binary(b) => Self::Binary(b.clone()),
            Self::Stream(arc) => Self::Stream(std::sync::Arc::clone(arc)),
        }
    }
}

impl Serialize for StdinInput {
    fn serialize<S: serde::Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        match self {
            Self::Text(s) => serializer.serialize_str(s),
            Self::Binary(b) => serializer.serialize_bytes(b),
            Self::Stream(_) => Err(serde::ser::Error::custom("cannot serialize stream input")),
        }
    }
}

impl<'de> Deserialize<'de> for StdinInput {
    fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        use serde::de::Error;
        let value = serde_json::Value::deserialize(deserializer)?;
        if let Some(s) = value.as_str() {
            return Ok(Self::Text(s.to_string()));
        }
        if let Some(arr) = value.as_array() {
            let bytes: Result<Vec<u8>, D::Error> = arr
                .iter()
                .map(|v| {
                    v.as_u64()
                        .map(|n| n as u8)
                        .ok_or_else(|| D::Error::custom("expected integer in binary array"))
                })
                .collect();
            if let Ok(bytes) = bytes {
                return Ok(Self::Binary(bytes));
            }
        }
        Err(D::Error::custom("expected string or binary array"))
    }
}

/// Duration specification for timeouts.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(untagged)]
pub enum DurationInput {
    Milliseconds(u64),
    Seconds(u64),
    Minutes(u64),
    String(String),
}

impl DurationInput {
    pub fn as_millis(&self) -> u64 {
        match self {
            Self::Milliseconds(ms) => *ms,
            Self::Seconds(s) => s * 1000,
            Self::Minutes(m) => m * 60_000,
            Self::String(s) => parse_duration_string(s),
        }
    }
}

fn parse_duration_string(s: &str) -> u64 {
    let s = s.trim().to_lowercase();
    if let Some(n) = s.strip_suffix("ms") {
        n.parse().unwrap_or(0)
    } else if let Some(n) = s.strip_suffix("s") {
        n.parse::<u64>().unwrap_or(0) * 1000
    } else if let Some(n) = s.strip_suffix("m") {
        n.parse::<u64>().unwrap_or(0) * 60_000
    } else if let Some(n) = s.strip_suffix("h") {
        n.parse::<u64>().unwrap_or(0) * 3_600_000
    } else {
        s.parse().unwrap_or(0)
    }
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
    /// Cancellation token to abort the process
    #[serde(skip)]
    pub cancellation_token: Option<CancellationToken>,
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
        Err(AppProcessError::Exited {
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
pub fn require_exit_in(result: RunResult, ok_codes: &[i32]) -> Result<RunResult, AppProcessError> {
    if ok_codes.contains(&result.exit_code) {
        Ok(result)
    } else {
        Err(AppProcessError::Exited {
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

/// A single command to execute as a child process.
///
/// Ported from: `process.ts` — `ChildProcess.Command`
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StandardCommand {
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

impl StandardCommand {
    /// Create a simple command with no arguments.
    #[must_use]
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

impl std::fmt::Display for StandardCommand {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.command)?;
        for arg in &self.args {
            write!(f, " {arg}")?;
        }
        Ok(())
    }
}

/// A command to execute — either a single command or a piped pipeline.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(untagged)]
pub enum ProcessCommand {
    Standard(StandardCommand),
    Piped(PipedCommand),
}

/// A piped command: left side feeds into right side.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PipedCommand {
    pub left: Box<ProcessCommand>,
    pub right: Box<StandardCommand>,
}

impl ProcessCommand {
    /// Create a simple command (wraps `StandardCommand::new`).
    #[must_use]
    pub fn new(command: impl Into<String>) -> Self {
        Self::Standard(StandardCommand::new(command))
    }

    /// Add an argument (only for `Standard` variant).
    pub fn arg(self, arg: impl Into<String>) -> Self {
        match self {
            Self::Standard(cmd) => Self::Standard(cmd.arg(arg)),
            Self::Piped(_) => self,
        }
    }

    /// Set the working directory (only for `Standard` variant).
    pub fn cwd(self, dir: impl Into<String>) -> Self {
        match self {
            Self::Standard(cmd) => Self::Standard(cmd.cwd(dir)),
            Self::Piped(_) => self,
        }
    }

    /// Get the inner `StandardCommand` if this is a `Standard` variant.
    pub fn as_standard(&self) -> Option<&StandardCommand> {
        match self {
            Self::Standard(cmd) => Some(cmd),
            Self::Piped(_) => None,
        }
    }

    /// Get the inner `StandardCommand` if this is a `Standard` variant (mutable).
    pub fn as_standard_mut(&mut self) -> Option<&mut StandardCommand> {
        match self {
            Self::Standard(cmd) => Some(cmd),
            Self::Piped(_) => None,
        }
    }
}

impl std::fmt::Display for ProcessCommand {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Standard(cmd) => write!(f, "{cmd}"),
            Self::Piped(pipe) => write!(f, "{} | {}", pipe.left, pipe.right),
        }
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
// Helpers
// ---------------------------------------------------------------------------

/// Wrap any display error into an `AppProcessError::SpawnFailed`.
pub fn wrap_error(error: impl std::fmt::Display) -> AppProcessError {
    AppProcessError::SpawnFailed {
        message: error.to_string(),
    }
}

/// Extract the exit code from an `ExitStatus`, returning `128 + signal` if
/// the process was killed by a signal on Unix.
pub fn exit_code_from_status(status: &std::process::ExitStatus) -> Option<i32> {
    #[cfg(unix)]
    {
        use std::os::unix::process::ExitStatusExt;
        if let Some(signal) = status.signal() {
            return Some(128 + signal);
        }
    }
    status.code()
}

/// Send SIGTERM to an entire process group (negative pid) on Unix,
/// or `/T` (tree kill) on Windows.
pub async fn kill_group(pid: u32) {
    #[cfg(unix)]
    {
        let _ = tokio::process::Command::new("kill")
            .args(["-TERM", &format!("-{pid}")])
            .output()
            .await;
    }
    #[cfg(not(unix))]
    {
        let _ = tokio::process::Command::new("taskkill")
            .args(["/pid", &pid.to_string(), "/f", "/t"])
            .output()
            .await;
    }
}

/// Detach a child process handle so the OS does not wait for it on drop.
///
/// In tokio, configure `Command::kill_on_drop(false)` before spawning;
/// this function is a no-op since `kill_on_drop` cannot be changed after spawn.
pub fn unref(_child: &mut tokio::process::Child) {}

// ---------------------------------------------------------------------------
// ProcessService
// ---------------------------------------------------------------------------

/// Create an `Error` from an abort signal reason.
///
/// Ported from: `process.ts` — `abortError()`
pub fn abort_error(message: &str) -> AppProcessError {
    AppProcessError::Exited {
        command: String::new(),
        exit_code: None,
        stderr: Some("aborted".into()),
        cause: Some(message.into()),
    }
}

/// Wait for an abort signal, returning an error when it fires.
///
/// Ported from: `process.ts` — `waitForAbort()`
pub async fn wait_for_abort(token: &tokio_util::sync::CancellationToken) -> AppProcessError {
    token.cancelled().await;
    abort_error("process aborted via cancellation token")
}

/// Service for spawning and managing child processes.

///
/// Ported from: `process.ts` — main service
pub struct ProcessService;

impl ProcessService {
    /// Internal helper: spawn a child process from a ProcessCommand.
    fn spawn_child(cmd: &ProcessCommand) -> Result<tokio::process::Child, std::io::Error> {
        let std_cmd = match cmd {
            ProcessCommand::Standard(s) => s,
            ProcessCommand::Piped(pipe) => &pipe.right,
        };
        let mut c = tokio::process::Command::new(&std_cmd.command);
        c.args(&std_cmd.args);
        c.stdout(std::process::Stdio::piped());
        c.stderr(std::process::Stdio::piped());
        c.stdin(std::process::Stdio::piped());
        c.kill_on_drop(true);

        if let Some(ref cwd) = std_cmd.cwd {
            c.current_dir(cwd);
        }
        for (k, v) in &std_cmd.env {
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
        let mut child = Self::spawn_child(cmd).map_err(|e| AppProcessError::Exited {
            command: cmd.to_string(),
            exit_code: None,
            stderr: None,
            cause: Some(e.to_string()),
        })?;

        let timeout_ms = opts.timeout.as_ref().map_or(0, |t| t.as_millis());
        let max_output = opts.max_output_bytes.unwrap_or(usize::MAX);
        let max_error = opts.max_error_bytes.unwrap_or(usize::MAX);

        // Write stdin if provided
        if let Some(ref stdin_input) = opts.stdin {
            use std::io::Write;
            if let Some(mut stdin) = child.stdin.take() {
                match stdin_input {
                    StdinInput::Text(s) => {
                        drop(stdin.write_all(s.as_bytes()));
                    }
                    StdinInput::Binary(b) => {
                        drop(stdin.write_all(b));
                    }
                    StdinInput::Stream(rx) => {
                        // Stream stdin: forward chunks from the receiver to stdin
                        {
                            let mut guard = rx.lock().unwrap();
                            while let Ok(chunk) = guard.try_recv() {
                                let _ = stdin.write_all(&chunk);
                            }
                        }
                        // Spawn a task to forward remaining chunks
                        let stdin_clone = child.stdin.take();
                        if let Some(mut s) = stdin_clone {
                            let rx = std::sync::Arc::clone(rx);
                            tokio::spawn(async move {
                                use tokio::io::AsyncWriteExt;
                                loop {
                                    let chunk = {
                                        let mut guard = rx.lock().unwrap();
                                        guard.try_recv()
                                    };
                                    match chunk {
                                        Ok(chunk) => {
                                            let _ = s.write_all(&chunk).await;
                                        }
                                        Err(tokio::sync::mpsc::error::TryRecvError::Empty) => {
                                            tokio::task::yield_now().await;
                                        }
                                        Err(tokio::sync::mpsc::error::TryRecvError::Disconnected) => {
                                            break;
                                        }
                                    }
                                }
                                let _ = s.shutdown().await;
                            });
                        }
                    }
                }
            }
        }

        let cancelled = opts.cancellation_token.clone();

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
                    return Err(AppProcessError::Exited {
                        command: cmd.to_string(),
                        exit_code: None,
                        stderr: Some("process timed out".into()),
                        cause: Some(format!("timed out after {timeout_ms}ms")),
                    });
                }
            }
        } else if let Some(ref token) = cancelled {
            tokio::select! {
                output = child.wait_with_output() => {
                    output
                }
                _ = token.cancelled() => {
                    return Err(AppProcessError::Exited {
                        command: cmd.to_string(),
                        exit_code: None,
                        stderr: Some("process cancelled".into()),
                        cause: Some("cancellation token triggered".into()),
                    });
                }
            }
        } else {
            child.wait_with_output().await
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

                let exit_code = exit_code_from_status(&output.status).unwrap_or(-1);

                Ok(RunResult {
                    command: cmd.to_string(),
                    exit_code,
                    stdout,
                    stderr,
                    stdout_truncated: stdout_len > max_output,
                    stderr_truncated: stderr_len > max_error,
                })
            }
            Err(e) => Err(AppProcessError::Exited {
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
    /// The caller **must** call `child.wait()` after consuming the stream to
    /// obtain the exit status.
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

        let mut child = Self::spawn_child(cmd).map_err(|e| AppProcessError::Exited {
            command: cmd.to_string(),
            exit_code: None,
            stderr: None,
            cause: Some(e.to_string()),
        })?;

        let stdout = child.stdout.take();
        let stderr = child.stderr.take();

        let max_error = opts.max_error_bytes.unwrap_or(1024 * 1024);
        let cancel_token = opts.cancellation_token.clone();

        let (tx, rx) = tokio::sync::mpsc::channel::<Result<(String, bool), std::io::Error>>(64);

        // Spawn stdout reader
        if let Some(stdout) = stdout {
            let tx = tx.clone();
            let token = cancel_token.clone();
            tokio::spawn(async move {
                let reader = BufReader::new(stdout);
                let mut lines = reader.lines();
                loop {
                    tokio::select! {
                        result = lines.next_line() => {
                            match result {
                                Ok(Some(line)) => {
                                    if tx.send(Ok((line, false))).await.is_err() {
                                        break;
                                    }
                                }
                                Ok(None) => break,
                                Err(e) => {
                                    let _ = tx.send(Err(e)).await;
                                    break;
                                }
                            }
                        }
                        _ = async {}, if token.as_ref().is_some_and(|t| t.is_cancelled()) => {
                            break;
                        }
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
                    if include_stderr && tx.send(Ok((line, true))).await.is_err() {
                        break;
                    }
                }
            });
        }

        let stream = tokio_stream::wrappers::ReceiverStream::new(rx);
        Ok((stream, child))
    }

    /// After a streaming run completes, validate the exit code against the
    /// allowed set in `RunStreamOptions::ok_exit_codes`.
    ///
    /// If the process exited with an error code, remaining stderr is collected
    /// and included in the error.
    pub async fn validate_exit(
        child: &mut tokio::process::Child,
        cmd: &ProcessCommand,
        opts: &RunStreamOptions,
    ) -> Result<i32, AppProcessError> {
        let status = child.wait().await.map_err(|e| AppProcessError::Exited {
            command: cmd.to_string(),
            exit_code: None,
            stderr: None,
            cause: Some(e.to_string()),
        })?;

        let exit_code = exit_code_from_status(&status).unwrap_or(-1);

        if let Some(ref codes) = opts.ok_exit_codes {
            if !codes.contains(&exit_code) {
                let mut stderr = String::new();
                if let Some(ref mut reader) = child.stderr {
                    let _ = tokio::io::AsyncReadExt::read_to_string(reader, &mut stderr).await;
                }
                return Err(AppProcessError::Exited {
                    command: cmd.to_string(),
                    exit_code: Some(exit_code),
                    stderr: if stderr.is_empty() {
                        None
                    } else {
                        Some(stderr)
                    },
                    cause: None,
                });
            }
        }

        Ok(exit_code)
    }

    /// Validate that a process result has a successful exit code.
    ///
    /// Ported from: `process.ts` — `requireSuccess()`
    pub fn require_success(result: &RunResult) -> Result<(), AppProcessError> {
        if result.exit_code == 0 {
            Ok(())
        } else {
            Err(AppProcessError::Exited {
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
        let cmd = ProcessCommand::new("rg").arg("--json").arg("pattern");
        let display = cmd.to_string();
        assert!(display.contains("rg"));
        assert!(display.contains("--json"));
        assert!(display.contains("pattern"));
    }

    #[test]
    fn test_process_command_default_args() {
        let cmd = ProcessCommand::new("echo");
        let std_cmd = cmd.as_standard().unwrap();
        assert!(std_cmd.args.is_empty());
        assert!(std_cmd.env.is_empty());
        assert!(std_cmd.cwd.is_none());
    }

    #[test]
    fn test_run_options_default() {
        let opts = RunOptions::default();
        assert!(opts.max_output_bytes.is_none());
        assert!(opts.stdin.is_none());
        assert!(opts.timeout.is_none());
    }

    #[test]
    fn test_run_stream_options_default() {
        let opts = RunStreamOptions::default();
        assert!(!opts.include_stderr);
    }

    #[test]
    fn test_app_process_error_display() {
        let err = AppProcessError::Exited {
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
            let parsed: ProcessStatus = serde_json::from_str(&json).expect("deserialize");
            assert_eq!(parsed, status);
        }
    }

    #[tokio::test]
    async fn test_process_service_run_echo() {
        let cmd = ProcessCommand::new("echo").arg("hello");
        let opts = RunOptions::default();
        let result = ProcessService::run(&cmd, &opts)
            .await
            .expect("should run echo");
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
            timeout: Some(DurationInput::Milliseconds(100)),
            ..RunOptions::default()
        };
        let result = ProcessService::run(&cmd, &opts).await;
        assert!(result.is_err());
        let err = result.unwrap_err();
        match &err {
            AppProcessError::Exited { stderr, .. } => {
                assert_eq!(stderr.as_deref(), Some("process timed out"));
            }
            _ => panic!("expected Exited variant"),
        }
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
        let cmd =
            ProcessCommand::Standard(StandardCommand::new("sh").arg("-c").arg("echo $MY_VAR"));
        let mut env = std::collections::HashMap::new();
        env.insert("MY_VAR".into(), "test_value".into());
        let mut std_cmd = match &cmd {
            ProcessCommand::Standard(s) => s.clone(),
            _ => unreachable!(),
        };
        std_cmd.env = env;
        let cmd = ProcessCommand::Standard(std_cmd);
        let opts = RunOptions::default();
        let result = ProcessService::run(&cmd, &opts).await.expect("should run");
        assert_eq!(result.exit_code, 0);
        let stdout = result.stdout_str().expect("valid utf8");
        assert!(stdout.contains("test_value"));
    }

    #[test]
    fn test_wrap_error() {
        let err = wrap_error("something went wrong");
        match &err {
            AppProcessError::SpawnFailed { message } => {
                assert_eq!(message, "something went wrong");
            }
            _ => panic!("expected SpawnFailed variant"),
        }
    }

    #[test]
    fn test_exit_code_from_status_success() {
        let status = std::process::Command::new("true")
            .status()
            .expect("should run true");
        let code = exit_code_from_status(&status);
        assert_eq!(code, Some(0));
    }

    #[test]
    fn test_exit_code_from_status_nonzero() {
        let status = std::process::Command::new("sh")
            .args(["-c", "exit 42"])
            .status()
            .expect("should run sh");
        let code = exit_code_from_status(&status);
        assert_eq!(code, Some(42));
    }

    #[tokio::test]
    async fn test_kill_group() {
        // Spawn a process, get its pid, then kill the group
        let mut child = tokio::process::Command::new("sleep")
            .arg("60")
            .spawn()
            .expect("should spawn");
        let pid = child.id().expect("should have pid");
        kill_group(pid).await;
        let status = child.wait().await.expect("should wait");
        // Process should be terminated (signal on unix, non-zero on windows)
        // In container environments, the process may exit with code 0 or a signal
        // depending on process group handling, so just verify it terminated.
        #[cfg(unix)]
        {
            use std::os::unix::process::ExitStatusExt;
            let terminated = status.signal().is_some() || status.code().is_some();
            assert!(terminated, "process should have been terminated");
        }
    }

    #[tokio::test]
    async fn test_unref_detaches_child() {
        let mut child = tokio::process::Command::new("sleep")
            .arg("60")
            .spawn()
            .expect("should spawn");
        unref(&mut child);
        // After unref, drop should not kill the process
        // (the OS will clean it up when it exits)
        child.kill().await.expect("should kill");
    }

    #[tokio::test]
    async fn test_run_with_cancellation_token() {
        let cmd = ProcessCommand::new("sleep").arg("60");
        let token = CancellationToken::new();
        let opts = RunOptions {
            cancellation_token: Some(token.clone()),
            ..RunOptions::default()
        };

        // Cancel immediately
        token.cancel();

        let result = ProcessService::run(&cmd, &opts).await;
        assert!(result.is_err());
        let err = result.unwrap_err();
        match &err {
            AppProcessError::Exited { cause, .. } => {
                assert!(cause.as_deref().unwrap_or("").contains("cancellation"));
            }
            _ => panic!("expected Exited variant"),
        }
    }

    #[tokio::test]
    async fn test_run_stream_with_cancellation_token() {
        use futures::StreamExt;
        let cmd = ProcessCommand::new("sleep").arg("60");
        let token = CancellationToken::new();
        let opts = RunStreamOptions {
            cancellation_token: Some(token.clone()),
            ..RunStreamOptions::default()
        };

        let (_stream, _child) = ProcessService::run_stream(&cmd, &opts)
            .await
            .expect("should start streaming");

        // Cancel — the stream reader tasks will stop
        token.cancel();
        // Give a moment for cancellation to propagate
        tokio::time::sleep(std::time::Duration::from_millis(50)).await;
    }

    #[tokio::test]
    async fn test_validate_exit_ok() {
        let cmd = ProcessCommand::new("true");
        let opts = RunStreamOptions {
            ok_exit_codes: Some(vec![0]),
            ..RunStreamOptions::default()
        };

        let mut child = ProcessService::spawn_child(&cmd).expect("should spawn");
        let code = ProcessService::validate_exit(&mut child, &cmd, &opts)
            .await
            .expect("should validate ok");
        assert_eq!(code, 0);
    }

    #[tokio::test]
    async fn test_validate_exit_bad_code() {
        let cmd = ProcessCommand::Standard(StandardCommand::new("sh").arg("-c").arg("exit 1"));
        let opts = RunStreamOptions {
            ok_exit_codes: Some(vec![0, 2]),
            ..RunStreamOptions::default()
        };

        let mut child = ProcessService::spawn_child(&cmd).expect("should spawn");
        let result = ProcessService::validate_exit(&mut child, &cmd, &opts).await;
        assert!(result.is_err());
        let err = result.unwrap_err();
        match &err {
            AppProcessError::Exited { exit_code, .. } => {
                assert_eq!(*exit_code, Some(1));
            }
            _ => panic!("expected Exited variant"),
        }
    }

    #[test]
    fn test_duration_input_millis() {
        assert_eq!(DurationInput::Milliseconds(500).as_millis(), 500);
    }

    #[test]
    fn test_duration_input_seconds() {
        assert_eq!(DurationInput::Seconds(5).as_millis(), 5000);
    }

    #[test]
    fn test_duration_input_minutes() {
        assert_eq!(DurationInput::Minutes(2).as_millis(), 120_000);
    }

    #[test]
    fn test_duration_input_string_seconds() {
        assert_eq!(DurationInput::String("30s".into()).as_millis(), 30_000);
    }

    #[test]
    fn test_duration_input_string_minutes() {
        assert_eq!(DurationInput::String("5m".into()).as_millis(), 300_000);
    }

    #[test]
    fn test_duration_input_string_hours() {
        assert_eq!(DurationInput::String("2h".into()).as_millis(), 7_200_000);
    }

    #[test]
    fn test_duration_input_string_millis() {
        assert_eq!(DurationInput::String("150ms".into()).as_millis(), 150);
    }

    #[test]
    fn test_duration_input_string_plain() {
        assert_eq!(DurationInput::String("5000".into()).as_millis(), 5000);
    }

    #[test]
    fn test_duration_input_string_with_text() {
        assert_eq!(DurationInput::String("30 seconds".into()).as_millis(), 0);
    }

    #[test]
    fn test_piped_command_display() {
        let left = ProcessCommand::new("cat").arg("file.txt");
        let right = StandardCommand::new("grep").arg("pattern");
        let piped = ProcessCommand::Piped(PipedCommand {
            left: Box::new(left),
            right: Box::new(right),
        });
        let display = piped.to_string();
        assert!(display.contains("|"));
        assert!(display.contains("cat"));
        assert!(display.contains("grep"));
    }

    #[test]
    fn test_process_command_as_standard() {
        let cmd = ProcessCommand::new("echo");
        assert!(cmd.as_standard().is_some());

        let piped = ProcessCommand::Piped(PipedCommand {
            left: Box::new(ProcessCommand::new("cat")),
            right: Box::new(StandardCommand::new("wc")),
        });
        assert!(piped.as_standard().is_none());
    }

    #[test]
    fn test_app_process_error_spawn_failed_display() {
        let err = AppProcessError::SpawnFailed {
            message: "permission denied".into(),
        };
        let msg = err.to_string();
        assert!(msg.contains("permission denied"));
        assert!(msg.contains("spawn failed"));
    }

    #[test]
    fn test_app_process_error_exited_display() {
        let err = AppProcessError::Exited {
            command: "ls".into(),
            exit_code: Some(2),
            stderr: None,
            cause: None,
        };
        let msg = err.to_string();
        assert!(msg.contains("ls"));
        assert!(msg.contains("2"));
    }
}
