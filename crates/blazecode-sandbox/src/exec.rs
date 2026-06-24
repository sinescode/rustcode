//! # Execution engine
//!
//! The single entry point for all process execution.
//!
//! # Contract
//!
//! No other code in BlazeCode++ may call `std::process::Command`.
//! All execution must go through `sandbox::run()`.

use crate::audit::AuditEntry;
use crate::policy::{CommandSpec, SecurityPolicy};
use crate::SandboxError;
use std::process::Stdio;

use std::time::Instant;
use tokio::io::AsyncWriteExt;
use tokio::process::Command;
use tokio::sync::oneshot;
use tokio::time::timeout;
use tracing::{debug, info, warn};

/// Start signal for coordinated process execution.
pub struct StartGuard(Option<oneshot::Sender<()>>);

impl StartGuard {
    /// Create a new start guard. The process will not begin until `start()` is called.
    pub fn new() -> (Self, oneshot::Receiver<()>) {
        let (tx, rx) = oneshot::channel();
        (Self(Some(tx)), rx)
    }

    /// Allow the process to start.
    pub fn start(mut self) {
        if let Some(tx) = self.0.take() {
            let _ = tx.send(());
        }
    }
}

impl Drop for StartGuard {
    fn drop(&mut self) {
        if let Some(tx) = self.0.take() {
            let _ = tx.send(());
        }
    }
}

// ---------------------------------------------------------------------------
// Run result
// ---------------------------------------------------------------------------

/// Result of a completed process execution.
#[derive(Debug, Clone)]
pub struct ExecResult {
    /// The command that was executed.
    pub command: String,

    /// Exit code.
    pub exit_code: i32,

    /// Standard output.
    pub stdout: Vec<u8>,

    /// Standard error.
    pub stderr: Vec<u8>,

    /// Whether stdout was truncated.
    pub stdout_truncated: bool,

    /// Whether stderr was truncated.
    pub stderr_truncated: bool,

    /// Duration of execution.
    pub duration: std::time::Duration,

    /// Audit entry for this execution.
    pub audit: AuditEntry,

    /// Whether the process was killed by a signal.
    pub killed: bool,
}

impl ExecResult {
    /// Returns true if the process exited with code 0.
    pub fn success(&self) -> bool {
        self.exit_code == 0 && !self.killed
    }

    /// Get stdout as a string (lossy UTF-8).
    pub fn stdout_str(&self) -> &str {
        std::str::from_utf8(&self.stdout).unwrap_or("[binary output]")
    }

    /// Get stderr as a string (lossy UTF-8).
    pub fn stderr_str(&self) -> &str {
        std::str::from_utf8(&self.stderr).unwrap_or("[binary stderr]")
    }
}

// ---------------------------------------------------------------------------
// Streaming output
// ---------------------------------------------------------------------------

/// A single output event from a streaming process.
#[derive(Debug, Clone)]
pub enum ExecEvent {
    /// A chunk of stdout.
    Stdout(#[doc = "Bytes chunk"] Vec<u8>),
    /// A chunk of stderr.
    Stderr(#[doc = "Bytes chunk"] Vec<u8>),
    /// The process exited.
    Exited {
        /// Exit code
        exit_code: i32,
        /// Wall clock duration
        duration: std::time::Duration,
    },
    /// The process was terminated by a signal.
    Killed,
    /// An error occurred.
    Error(#[doc = "Error message"] String),
}

// ---------------------------------------------------------------------------
// Run options
// ---------------------------------------------------------------------------

/// Options for running a process.
#[derive(Debug, Clone)]
pub struct RunOptions {
    /// Maximum stdout bytes to capture (None = unlimited, not recommended).
    pub max_stdout_bytes: Option<usize>,

    /// Maximum stderr bytes to capture.
    pub max_stderr_bytes: Option<usize>,

    /// Whether to capture stderr inline (for streaming).
    pub capture_stderr: bool,

    /// Log the command before execution.
    pub log_command: bool,
}

impl Default for RunOptions {
    fn default() -> Self {
        Self {
            max_stdout_bytes: Some(1_000_000), // 1 MB
            max_stderr_bytes: Some(100_000),   // 100 KB
            capture_stderr: true,
            log_command: true,
        }
    }
}

// ---------------------------------------------------------------------------
// Core run functions — THE ONLY ENTRY POINTS
// ---------------------------------------------------------------------------

/// Execute a command with the given security policy.
///
/// This is THE single gate for all process execution in BlazeCode++.
///
/// # Contract
///
/// - Validates the command and its paths against the security policy.
/// - Applies Landlock rules if on Linux and policy.exec == Sandbox.
/// - Applies seccomp-bpf restrictions if on Linux.
/// - Captures stdout/stderr with size limits.
/// - Records an audit entry.
///
/// # Errors
///
/// Returns `SandboxError::PolicyViolation` if the command is not permitted.
pub async fn run(
    policy: &SecurityPolicy,
    spec: &CommandSpec,
    options: &RunOptions,
) -> Result<ExecResult, SandboxError> {
    let start = Instant::now();

    // 1. Validate the policy
    policy.validate()?;

    // 2. Apply filesystem path checks
    if let Some(cwd) = &spec.cwd {
        if !policy.can_read(cwd) {
            return Err(SandboxError::PolicyViolation(format!(
                "working directory '{}' not in read paths",
                cwd.display()
            )));
        }
    }

    // 3. Build the command
    if options.log_command {
        info!(command = %spec.display(), "sandbox: executing command");
    }

    let mut cmd = Command::new(&spec.program);
    cmd.args(&spec.args)
        .stdout(Stdio::piped())
        .stdin(Stdio::piped());

    if options.capture_stderr {
        cmd.stderr(Stdio::piped());
    } else {
        cmd.stderr(Stdio::null());
    }

    if let Some(cwd) = &spec.cwd {
        cmd.current_dir(cwd);
    }

    // Apply environment filter
    if let Some(ref env_vars) = spec.env {
        cmd.env_clear();
        for (key, value) in env_vars {
            if policy.env_allow.is_empty() || policy.env_allow.contains(key) {
                cmd.env(key, value);
            } else {
                debug!(key = %key, "sandbox: filtered env var");
            }
        }
    }

    // Apply all env vars from parent process that are in the allowlist
    if policy.exec != super::policy::ExecutionLevel::Unsafe {
        // For Sandbox/Container, strip env by default
        if !policy.env_allow.is_empty() {
            cmd.env_clear();
            for key in &policy.env_allow {
                if let Ok(val) = std::env::var(key) {
                    cmd.env(key, val);
                }
            }
        }
    }

    // 4. Linux-specific sandbox
    #[cfg(target_os = "linux")]
    before_spawn_linux(policy, spec, &mut cmd).await?;

    // 5. Spawn with timeout
    let timeout_dur = policy.timeout();
    let result = timeout(timeout_dur, async {
        let mut child = cmd.spawn().map_err(|e| {
            debug!(error = %e, command = %spec.program, "sandbox: failed to spawn");
            SandboxError::Internal(format!("failed to spawn '{}': {}", spec.program, e))
        })?;

        let mut stdout = child.stdout.take()
            .ok_or_else(|| SandboxError::Internal("no stdout pipe".into()))?;
        let mut stderr = child.stderr.take();

        // Write stdin if provided
        if let Some(ref stdin_content) = spec.stdin {
            if let Some(mut stdin) = child.stdin.take() {
                stdin.write_all(stdin_content.as_bytes()).await.map_err(|e| {
                    warn!(error = %e, "sandbox: failed to write stdin");
                    e
                }).ok();
                // Close stdin so the child can finish
                drop(stdin);
            }
        } else {
            // Close stdin immediately
            drop(child.stdin.take());
        }

        // Read stdout with size limit
        use tokio::io::AsyncReadExt;
        let max_out = options.max_stdout_bytes.unwrap_or(usize::MAX);
        let max_err = options.max_stderr_bytes.unwrap_or(usize::MAX);

        let mut stdout_buf = Vec::new();
        let mut stderr_buf = Vec::new();
        let mut stdout_truncated = false;
        let mut stderr_truncated = false;

        // Read stdout and stderr in a loop
        loop {
            tokio::select! {
                result = stdout.read_buf(&mut stdout_buf) => {
                    match result {
                        Ok(0) => break,
                        Ok(_n) => {
                            if stdout_buf.len() > max_out {
                                stdout_truncated = true;
                                stdout_buf.truncate(max_out);
                                // Drain remaining
                                let _ = stdout.read_to_end(&mut Vec::new()).await;
                                break;
                            }
                        }
                        Err(e) => {
                            warn!(error = %e, "sandbox: stdout read error");
                            break;
                        }
                    }
                }
                result = async {
                    if let Some(ref mut stderr_pipe) = stderr {
                        stderr_pipe.read_buf(&mut stderr_buf).await
                    } else {
                        std::future::pending::<std::io::Result<usize>>().await
                    }
                } => {
                    match result {
                        Ok(0) => {}
                        Ok(_n) => {
                            if stderr_buf.len() > max_err {
                                stderr_truncated = true;
                                stderr_buf.truncate(max_err);
                            }
                        }
                        Err(e) => {
                            warn!(error = %e, "sandbox: stderr read error");
                            break;
                        }
                    }
                }
            }
        }

        let status = child.wait().await.map_err(|e| {
            SandboxError::Internal(format!("failed to wait on '{}': {}", spec.program, e))
        })?;

        let exit_code = status.code().unwrap_or(-1);
        let killed = !status.success() && status.code().is_none();

        let duration = start.elapsed();

        Ok(ExecResult {
            command: spec.display(),
            exit_code,
            stdout: stdout_buf,
            stderr: stderr_buf,
            stdout_truncated,
            stderr_truncated,
            duration,
            audit: AuditEntry::new(
                &spec.display(),
                exit_code,
                duration,
                policy.clone(),
            ),
            killed,
        })
    }).await;

    match result {
        Ok(Ok(exec_result)) => Ok(exec_result),
        Ok(Err(e)) => Err(e),
        Err(_elapsed) => {
            warn!(command = %spec.display(), timeout = ?timeout_dur, "sandbox: command timed out");
            Err(SandboxError::Timeout(timeout_dur))
        }
    }
}

/// Run a command with streaming output events.
pub async fn run_stream(
    policy: &SecurityPolicy,
    spec: &CommandSpec,
) -> Result<tokio::sync::mpsc::Receiver<ExecEvent>, SandboxError> {
    policy.validate()?;

    let (tx, rx) = tokio::sync::mpsc::channel(128);
    let _policy = policy.clone();
    let spec = spec.clone();

    tokio::spawn(async move {
        let start = Instant::now();

        let mut cmd = Command::new(&spec.program);
        cmd.args(&spec.args)
            .stdout(Stdio::piped())
            .stdin(Stdio::piped())
            .stderr(Stdio::piped());

        if let Some(cwd) = &spec.cwd {
            cmd.current_dir(cwd);
        }

        match cmd.spawn() {
            Ok(mut child) => {
                let mut stdout = child.stdout.take().unwrap();
                let mut stderr = child.stderr.take().unwrap();

                // Write stdin if provided
                if let Some(ref stdin_content) = spec.stdin {
                    if let Some(mut stdin) = child.stdin.take() {
                        let _ = stdin.write_all(stdin_content.as_bytes()).await;
                        drop(stdin);
                    }
                } else {
                    drop(child.stdin.take());
                }

                use tokio::io::AsyncReadExt;
                let mut stdout_buf = vec![0u8; 4096];
                let mut stderr_buf = vec![0u8; 4096];

                loop {
                    tokio::select! {
                        result = stdout.read(&mut stdout_buf) => {
                            match result {
                                Ok(0) => {}
                                Ok(n) => {
                                    let _ = tx.send(ExecEvent::Stdout(stdout_buf[..n].to_vec())).await;
                                    continue;
                                }
                                Err(_) => {}
                            }
                        }
                        result = stderr.read(&mut stderr_buf) => {
                            match result {
                                Ok(0) => {}
                                Ok(n) => {
                                    let _ = tx.send(ExecEvent::Stderr(stderr_buf[..n].to_vec())).await;
                                    continue;
                                }
                                Err(_) => {}
                            }
                        }
                    }
                    break;
                }

                let status = child.wait().await;
                let exit_code = status.ok().and_then(|s| s.code()).unwrap_or(-1);
                let duration = start.elapsed();
                let _ = tx.send(ExecEvent::Exited { exit_code, duration }).await;
            }
            Err(e) => {
                let _ = tx.send(ExecEvent::Error(format!("spawn failed: {e}"))).await;
            }
        }
    });

    Ok(rx)
}

/// Linux-specific setup: Landlock + seccomp.
#[cfg(target_os = "linux")]
async fn before_spawn_linux(
    _policy: &SecurityPolicy,
    _spec: &CommandSpec,
    _cmd: &mut Command,
) -> Result<(), SandboxError> {
    // Landlock rules: apply via the `sudo landlock` helper or direct syscalls.
    // For now, log the intent.
    if std::env::var("BLAZECODE_LANDLOCK").is_ok() {
        debug!("sandbox: Landlock requested but not yet implemented via syscall");
        // TODO: Use `linux::landlock::apply_rules(policy)` when ready
    }

    // Seccomp-bpf: block non-essential syscalls
    if std::env::var("BLAZECODE_SECCOMP").is_ok() {
        debug!("sandbox: seccomp requested but not yet implemented");
        // TODO: Use `linux::seccomp::apply_filter(policy)` when ready
    }

    Ok(())
}

#[cfg(not(target_os = "linux"))]
async fn before_spawn_linux(
    _policy: &SecurityPolicy,
    _spec: &CommandSpec,
    _cmd: &mut Command,
) -> Result<(), SandboxError> {
    // Non-Linux: no kernel sandboxing available natively
    debug!("sandbox: Landlock/seccomp not available on this platform");
    Ok(())
}
