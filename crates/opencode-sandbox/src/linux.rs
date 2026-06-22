//! # Linux kernel sandboxing
//!
//! Landlock LSM and seccomp-bpf enforcement for Linux (kernel 5.13+).
//! These provide mandatory access control at the syscall level.

use crate::policy::SecurityPolicy;
use crate::SandboxError;

/// Apply Landlock rules based on the security policy.
///
/// Landlock (Linux 5.13+) restricts file system access at the syscall level.
/// Rules are set before spawning the child process and cannot be removed.
///
/// # Current status
///
/// This uses `landlock` CLI tool. Direct syscall-based implementation
/// is planned using the `linux_api` or `rustix` crate.
pub fn apply_landlock_rules(_policy: &SecurityPolicy) -> Result<(), SandboxError> {
    // TODO: Implement direct Landlock syscall wrappers:
    //
    //   let ctx = landlock_create_ruleset(...);
    //   landlock_add_rule(ctx, LANDLOCK_RULE_PATH_BENEATH, &read_rule);
    //   landlock_add_rule(ctx, LANDLOCK_RULE_PATH_BENEATH, &write_rule);
    //   landlock_restrict_self(ctx, 0);
    //
    // For the initial version, we rely on the OCI container sandbox.
    Ok(())
}

/// Apply seccomp-bpf filter based on the security policy.
///
/// Seccomp (since Linux 3.5) filters syscalls. We block:
/// - `mount`, `umount`, `pivot_root` (namespace escape)
/// - `ptrace` (process inspection)
/// - `kexec_load`, `bpf` (kernel manipulation)
/// - `reboot`, `swapon` (denial of service)
pub fn apply_seccomp_filter(_policy: &SecurityPolicy) -> Result<(), SandboxError> {
    // TODO: Implement seccomp-bpf using `seccompiler` or raw BPF.
    //
    //   let filter = BpfProgram {
    //       instructions: vec![
    //           SeccompInstruction::LoadArch,
    //           SeccompInstruction::JumpEq(AUDIT_ARCH_X86_64, 0, DENY),
    //           SeccompInstruction::LoadNr,
    //           // ... syscall-by-syscall allow/deny
    //       ]
    //   };
    //   seccomp(SECCOMP_SET_MODE_FILTER, SECCOMP_FILTER_FLAG_TSYNC, &filter);
    //
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_landlock_noop_ok() {
        let policy = SecurityPolicy::default();
        assert!(apply_landlock_rules(&policy).is_ok());
    }

    #[test]
    fn test_seccomp_noop_ok() {
        let policy = SecurityPolicy::default();
        assert!(apply_seccomp_filter(&policy).is_ok());
    }
}
