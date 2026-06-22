//! # Audit logging
//!
//! Every process execution is recorded with a tamper-evident audit entry.
//! Entries include: command hash, stdout hash, exit code, duration, policy snapshot.

use crate::policy::SecurityPolicy;
use sha2::{Digest, Sha256};
use std::time::Duration;

/// An audit entry for a single process execution.
#[derive(Debug, Clone)]
pub struct AuditEntry {
    /// ISO-8601 timestamp.
    pub timestamp: String,

    /// SHA-256 hash of the command string.
    pub command_hash: String,

    /// SHA-256 hash of stdout.
    pub stdout_hash: Option<String>,

    /// Exit code.
    pub exit_code: i32,

    /// Duration in milliseconds.
    pub duration_ms: u64,

    /// Snapshot of the security policy used (serialized).
    pub policy_snapshot: String,

    /// Whether the process was killed.
    pub killed: bool,
}

impl AuditEntry {
    /// Create a new audit entry.
    pub fn new(
        command: &str,
        exit_code: i32,
        duration: Duration,
        policy: SecurityPolicy,
    ) -> Self {
        let command_hash = hex::encode(Sha256::digest(command.as_bytes()));
        let policy_snapshot = serde_json::to_string(&policy).unwrap_or_default();

        Self {
            timestamp: chrono::Utc::now().to_rfc3339(),
            command_hash,
            stdout_hash: None,
            exit_code,
            duration_ms: duration.as_millis() as u64,
            policy_snapshot,
            killed: exit_code == -1,
        }
    }

    /// Set the stdout hash after execution completes.
    pub fn with_stdout_hash(mut self, stdout: &[u8]) -> Self {
        if !stdout.is_empty() {
            self.stdout_hash = Some(hex::encode(Sha256::digest(stdout)));
        }
        self
    }

    /// Serialize to JSON for storage.
    pub fn to_json(&self) -> String {
        serde_json::to_string(self).unwrap_or_default()
    }
}

impl serde::Serialize for AuditEntry {
    fn serialize<S: serde::Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        use serde::ser::SerializeStruct;
        let mut s = serializer.serialize_struct("AuditEntry", 7)?;
        s.serialize_field("timestamp", &self.timestamp)?;
        s.serialize_field("command_hash", &self.command_hash)?;
        s.serialize_field("stdout_hash", &self.stdout_hash)?;
        s.serialize_field("exit_code", &self.exit_code)?;
        s.serialize_field("duration_ms", &self.duration_ms)?;
        s.serialize_field("killed", &self.killed)?;
        s.serialize_field("policy", &self.policy_snapshot)?;
        s.end()
    }
}

impl<'de> serde::Deserialize<'de> for AuditEntry {
    fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        #[derive(serde::Deserialize)]
        struct AuditEntryRaw {
            timestamp: String,
            command_hash: String,
            stdout_hash: Option<String>,
            exit_code: i32,
            duration_ms: u64,
            killed: bool,
            policy: String,
        }

        let raw = AuditEntryRaw::deserialize(deserializer)?;
        Ok(AuditEntry {
            timestamp: raw.timestamp,
            command_hash: raw.command_hash,
            stdout_hash: raw.stdout_hash,
            exit_code: raw.exit_code,
            duration_ms: raw.duration_ms,
            policy_snapshot: raw.policy,
            killed: raw.killed,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_audit_entry_creation() {
        let policy = SecurityPolicy::default();
        let entry = AuditEntry::new("echo hello", 0, Duration::from_millis(42), policy);
        assert_eq!(entry.command_hash.len(), 64); // SHA-256 hex
        assert_eq!(entry.exit_code, 0);
        assert_eq!(entry.duration_ms, 42);
        assert!(entry.timestamp.contains('T'));
    }

    #[test]
    fn test_audit_entry_json_roundtrip() {
        let policy = SecurityPolicy::default();
        let entry = AuditEntry::new("ls -la", 0, Duration::from_secs(1), policy);
        let json = entry.to_json();
        let deserialized: AuditEntry = serde_json::from_str(&json).unwrap();
        assert_eq!(deserialized.command_hash, entry.command_hash);
        assert_eq!(deserialized.exit_code, entry.exit_code);
        assert_eq!(deserialized.duration_ms, entry.duration_ms);
    }
}
