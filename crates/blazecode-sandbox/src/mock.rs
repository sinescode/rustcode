//! # Mock sandbox for testing
//!
//! A no-op sandbox that logs all execution requests instead of enforcing.
//! Used in unit tests where real kernel sandboxing is not available.

use crate::audit::AuditEntry;
use crate::exec::{ExecEvent, ExecResult, RunOptions};
use crate::policy::{CommandSpec, SecurityPolicy};
use crate::SandboxError;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::Instant;

/// A sandbox that records all execution requests for test assertions.
#[derive(Debug, Default)]
pub struct MockSandbox {
    /// All commands that were executed.
    pub executed_commands: Vec<String>,

    /// Whether to fail on the next execution.
    pub fail_next: Arc<AtomicBool>,
}

impl MockSandbox {
    /// Create a new mock sandbox.
    pub fn new() -> Self {
        Self::default()
    }

    /// Execute a command (mocked — does not actually run anything).
    pub async fn run(
        &mut self,
        _policy: &SecurityPolicy,
        spec: &CommandSpec,
        _options: &RunOptions,
    ) -> Result<ExecResult, SandboxError> {
        self.executed_commands.push(spec.display());

        if self.fail_next.load(Ordering::SeqCst) {
            self.fail_next.store(false, Ordering::SeqCst);
            return Err(SandboxError::PolicyViolation("mock failure".into()));
        }

        Ok(ExecResult {
            command: spec.display(),
            exit_code: 0,
            stdout: Vec::new(),
            stderr: Vec::new(),
            stdout_truncated: false,
            stderr_truncated: false,
            duration: std::time::Duration::ZERO,
            audit: AuditEntry::new(
                &spec.display(),
                0,
                std::time::Duration::ZERO,
                SecurityPolicy::default(),
            ),
            killed: false,
        })
    }

    /// Stream execution (mocked).
    pub async fn run_stream(
        &mut self,
        _policy: &SecurityPolicy,
        spec: &CommandSpec,
    ) -> Result<tokio::sync::mpsc::Receiver<ExecEvent>, SandboxError> {
        self.executed_commands.push(spec.display());
        let (tx, rx) = tokio::sync::mpsc::channel(8);
        let _ = tx.send(ExecEvent::Exited {
            exit_code: 0,
            duration: std::time::Duration::ZERO,
        }).await;
        Ok(rx)
    }

    /// Assert that a specific command was executed.
    pub fn assert_executed(&self, command: &str) {
        assert!(
            self.executed_commands.iter().any(|c| c.contains(command)),
            "expected command containing '{command}' to have been executed, got: {:?}",
            self.executed_commands,
        );
    }

    /// Assert that no commands were executed.
    pub fn assert_no_executions(&self) {
        assert!(
            self.executed_commands.is_empty(),
            "expected no commands executed, got: {:?}",
            self.executed_commands,
        );
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_mock_sandbox_records_commands() {
        let mut sandbox = MockSandbox::new();
        let policy = SecurityPolicy::default();
        let spec = CommandSpec::new("echo").arg("hello");

        sandbox.run(&policy, &spec, &RunOptions::default()).await.unwrap();
        sandbox.assert_executed("echo hello");
    }

    #[tokio::test]
    async fn test_mock_sandbox_fail_next() {
        let mut sandbox = MockSandbox::new();
        sandbox.fail_next.store(true, Ordering::SeqCst);

        let policy = SecurityPolicy::default();
        let spec = CommandSpec::new("rm").arg("-rf").arg("/");

        let result = sandbox.run(&policy, &spec, &RunOptions::default()).await;
        assert!(result.is_err());
        assert!(sandbox.executed_commands.is_empty());
    }
}
