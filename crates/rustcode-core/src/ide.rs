//! IDE integration — detect and install VS Code extensions.
//!
//! Ported from: `packages/opencode/src/ide/index.ts`
//! OpenCode commit: 5d0f86606ac30690f79f0a6a9f41a1f49fe95d0b
//!
//! Provides:
//! - IDE detection from environment variables
//! - Extension installation for supported VS Code variants
//! - Event definitions for IDE lifecycle

use serde::{Deserialize, Serialize};

// ── Supported IDEs ────────────────────────────────────────────────────

/// A supported IDE with its CLI command name.
///
/// # Source
/// `packages/opencode/src/ide/index.ts` lines 6–12.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SupportedIde {
    pub name: IdeName,
    pub cmd: &'static str,
}

/// Names of supported IDE variants.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "PascalCase")]
pub enum IdeName {
    Windsurf,
    VisualStudioCodeInsiders,
    VisualStudioCode,
    Cursor,
    Vscodium,
}

impl IdeName {
    /// The human-readable name of the IDE.
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Windsurf => "Windsurf",
            Self::VisualStudioCodeInsiders => "Visual Studio Code - Insiders",
            Self::VisualStudioCode => "Visual Studio Code",
            Self::Cursor => "Cursor",
            Self::Vscodium => "VSCodium",
        }
    }

    /// The CLI command used to install extensions.
    pub fn cmd(self) -> &'static str {
        match self {
            Self::Windsurf => "windsurf",
            Self::VisualStudioCodeInsiders => "code-insiders",
            Self::VisualStudioCode => "code",
            Self::Cursor => "cursor",
            Self::Vscodium => "codium",
        }
    }
}

/// All supported IDEs.
pub const SUPPORTED_IDES: &[SupportedIde] = &[
    SupportedIde { name: IdeName::Windsurf, cmd: "windsurf" },
    SupportedIde { name: IdeName::VisualStudioCodeInsiders, cmd: "code-insiders" },
    SupportedIde { name: IdeName::VisualStudioCode, cmd: "code" },
    SupportedIde { name: IdeName::Cursor, cmd: "cursor" },
    SupportedIde { name: IdeName::Vscodium, cmd: "codium" },
];

// ── Detection ─────────────────────────────────────────────────────────

/// Detect the current IDE from environment variables.
///
/// Returns `None` when no supported IDE is detected.
///
/// # Source
/// `packages/opencode/src/ide/index.ts` lines 29–37.
pub fn detect_ide(term_program: Option<&str>, git_askpass: Option<&str>) -> Option<&'static str> {
    if term_program != Some("vscode") {
        return None;
    }
    let askpass = git_askpass?;
    for ide in SUPPORTED_IDES {
        if askpass.contains(ide.name.as_str()) {
            return Some(ide.name.as_str());
        }
    }
    None
}

/// Check if the caller is already an IDE extension.
///
/// # Source
/// `packages/opencode/src/ide/index.ts` lines 39–41.
pub fn already_installed(opencode_caller: Option<&str>) -> bool {
    matches!(opencode_caller, Some("vscode") | Some("vscode-insiders"))
}

// ── Extension name ────────────────────────────────────────────────────

/// The extension identifier installed by rustcode/opencode.
pub const EXTENSION_ID: &str = "sst-dev.opencode";

// ── Errors ────────────────────────────────────────────────────────────

/// Errors from IDE operations.
#[derive(Debug, thiserror::Error)]
pub enum IdeError {
    #[error("IDE extension already installed")]
    AlreadyInstalled,

    #[error("extension installation failed: {stderr}")]
    InstallFailed { stderr: String },

    #[error("unknown IDE: {0}")]
    UnknownIde(String),
}

// ── Tests ─────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_detect_ide_vscode() {
        let result = detect_ide(Some("vscode"), Some("Visual Studio Code"));
        assert_eq!(result, Some("Visual Studio Code"));
    }

    #[test]
    fn test_detect_ide_cursor() {
        let result = detect_ide(Some("vscode"), Some("/path/to/Cursor/GIT_ASKPASS"));
        assert_eq!(result, Some("Cursor"));
    }

    #[test]
    fn test_detect_ide_not_vscode() {
        let result = detect_ide(Some("iterm"), Some("Visual Studio Code"));
        assert_eq!(result, None);
    }

    #[test]
    fn test_detect_ide_no_askpass() {
        let result = detect_ide(Some("vscode"), None);
        assert_eq!(result, None);
    }

    #[test]
    fn test_already_installed() {
        assert!(already_installed(Some("vscode")));
        assert!(already_installed(Some("vscode-insiders")));
        assert!(!already_installed(Some("terminal")));
        assert!(!already_installed(None));
    }

    #[test]
    fn test_ide_name_cmd() {
        assert_eq!(IdeName::Windsurf.cmd(), "windsurf");
        assert_eq!(IdeName::VisualStudioCode.cmd(), "code");
        assert_eq!(IdeName::Cursor.cmd(), "cursor");
        assert_eq!(IdeName::Vscodium.cmd(), "codium");
    }

    #[test]
    fn test_supported_ides_count() {
        assert_eq!(SUPPORTED_IDES.len(), 5);
    }

    #[test]
    fn test_ide_name_serde() {
        let json = serde_json::to_string(&IdeName::Windsurf).unwrap();
        assert_eq!(json, "\"Windsurf\"");
        let parsed: IdeName = serde_json::from_str(&json).unwrap();
        assert_eq!(parsed, IdeName::Windsurf);
    }
}
