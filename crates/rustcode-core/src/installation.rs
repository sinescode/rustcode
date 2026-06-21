//! Installation management — version detection, upgrade methods, and user-agent.
//!
//! Ported from: `packages/opencode/src/installation/index.ts`
//! OpenCode commit: 5d0f86606ac30690f79f0a6a9f41a1f49fe95d0b
//!
//! Provides:
//! - [`Method`] — detected installation method (curl, npm, brew, etc.)
//! - [`ReleaseType`] — semver release classification (patch, minor, major)
//! - [`InstallationInfo`] — current + latest version info
//! - [`user_agent`] — build the user-agent string for HTTP requests

use serde::{Deserialize, Serialize};

// ── Types ─────────────────────────────────────────────────────────────

/// Installation method detected from the runtime environment.
///
/// # Source
/// `packages/opencode/src/installation/index.ts` line 17.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Method {
    Curl,
    Npm,
    Yarn,
    Pnpm,
    Bun,
    Brew,
    Scoop,
    Choco,
    Unknown,
}

/// Semver release type classification.
///
/// # Source
/// `packages/opencode/src/installation/index.ts` line 19.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ReleaseType {
    Patch,
    Minor,
    Major,
}

/// Installation version information.
///
/// # Source
/// `packages/opencode/src/installation/index.ts` lines 47–51 (`Info`).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct InstallationInfo {
    pub version: String,
    pub latest: String,
}

// ── Release type detection ────────────────────────────────────────────

/// Classify the difference between two semver versions as patch, minor, or major.
///
/// # Source
/// `packages/opencode/src/installation/index.ts` lines 36–45.
pub fn get_release_type(current: &str, latest: &str) -> ReleaseType {
    let curr_parts = parse_semver(current);
    let latest_parts = parse_semver(latest);

    if latest_parts.0 > curr_parts.0 {
        ReleaseType::Major
    } else if latest_parts.1 > curr_parts.1 {
        ReleaseType::Minor
    } else {
        ReleaseType::Patch
    }
}

/// Parse a `major.minor.patch` string into a tuple of `(u64, u64, u64)`.
/// Non-numeric segments default to 0.
fn parse_semver(v: &str) -> (u64, u64, u64) {
    let parts: Vec<&str> = v.split('.').collect();
    let major = parts.first().and_then(|s| s.parse().ok()).unwrap_or(0);
    let minor = parts.get(1).and_then(|s| s.parse().ok()).unwrap_or(0);
    let patch = parts.get(2).and_then(|s| s.parse().ok()).unwrap_or(0);
    (major, minor, patch)
}

// ── User-agent ────────────────────────────────────────────────────────

/// Build the user-agent string for HTTP requests.
///
/// Format: `opencode/{channel}/{version}/{client}`
///
/// # Source
/// `packages/opencode/src/installation/index.ts` lines 53–55.
pub fn user_agent(client: &str, channel: &str, version: &str) -> String {
    format!("opencode/{channel}/{version}/{client}")
}

// ── Tests ─────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_get_release_type_patch() {
        assert_eq!(get_release_type("1.2.3", "1.2.4"), ReleaseType::Patch);
    }

    #[test]
    fn test_get_release_type_minor() {
        assert_eq!(get_release_type("1.2.3", "1.3.0"), ReleaseType::Minor);
    }

    #[test]
    fn test_get_release_type_major() {
        assert_eq!(get_release_type("1.2.3", "2.0.0"), ReleaseType::Major);
    }

    #[test]
    fn test_get_release_type_same() {
        assert_eq!(get_release_type("1.2.3", "1.2.3"), ReleaseType::Patch);
    }

    #[test]
    fn test_parse_semver() {
        assert_eq!(parse_semver("1.2.3"), (1, 2, 3));
        assert_eq!(parse_semver("0.0.1"), (0, 0, 1));
        assert_eq!(parse_semver("10.20.30"), (10, 20, 30));
        assert_eq!(parse_semver("1.2"), (1, 2, 0));
        assert_eq!(parse_semver("1"), (1, 0, 0));
    }

    #[test]
    fn test_user_agent() {
        let ua = user_agent("cli", "latest", "1.0.0");
        assert_eq!(ua, "opencode/latest/1.0.0/cli");
    }
}
