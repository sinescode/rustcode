//! Instruction context — system context assembly from configuration and environment.
//!
//! Ported from: `packages/core/src/instruction-context.ts` (92 lines)
//! OpenCode commit: 5d0f86606ac30690f79f0a6a9f41a1f49fe95d0b

use serde::{Deserialize, Serialize};
use std::path::PathBuf;

/// A source of instructions loaded from a file path.
///
/// Ported from: `instruction-context.ts`
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct InstructionSource {
    /// The path to the instruction file (e.g., CLAUDE.md)
    pub path: PathBuf,
    /// The raw content of the instruction file
    pub content: String,
}

/// A single instruction block with metadata about its origin.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Instruction {
    /// The instruction text
    pub text: String,
    /// Where the instruction came from
    pub source: InstructionOrigin,
    /// The position/order for priority (lower = higher priority)
    pub priority: u32,
}

/// Origin of an instruction.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "type")]
pub enum InstructionOrigin {
    /// From a file on disk
    #[serde(rename = "file")]
    File {
        /// Path to the instruction file
        path: PathBuf,
    },
    /// From configuration (inline)
    #[serde(rename = "config")]
    Config,
    /// Built-in system instruction
    #[serde(rename = "builtin")]
    Builtin {
        /// Identifier for the built-in instruction
        name: String,
    },
    /// From a plugin
    #[serde(rename = "plugin")]
    Plugin {
        /// Plugin name
        plugin: String,
    },
    /// From a skill definition
    #[serde(rename = "skill")]
    Skill {
        /// Skill identifier
        skill: String,
    },
}

/// Context assembled from all instruction sources, ready for prompt injection.
///
/// Ported from: `instruction-context.ts` — assembled context
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct InstructionContext {
    /// Ordered list of instructions (priority order)
    pub instructions: Vec<Instruction>,
    /// Total character count of all instructions
    pub total_chars: usize,
    /// Paths that contributed instructions
    pub scanned_paths: Vec<PathBuf>,
}

impl InstructionContext {
    /// Create an empty instruction context.
    pub fn empty() -> Self {
        Self {
            instructions: Vec::new(),
            total_chars: 0,
            scanned_paths: Vec::new(),
        }
    }

    /// Add an instruction to the context.
    pub fn add(&mut self, instruction: Instruction) {
        self.total_chars += instruction.text.len();
        if let InstructionOrigin::File { ref path } = instruction.source {
            if !self.scanned_paths.contains(path) {
                self.scanned_paths.push(path.clone());
            }
        }
        self.instructions.push(instruction);
    }

    /// Sort instructions by priority (lower = first).
    pub fn sort_by_priority(&mut self) {
        self.instructions.sort_by_key(|i| i.priority);
    }

    /// Render all instructions into a single string, separated by newlines.
    pub fn render(&self) -> String {
        self.instructions
            .iter()
            .map(|i| i.text.as_str())
            .collect::<Vec<_>>()
            .join("\n\n")
    }
}

impl Default for InstructionContext {
    fn default() -> Self {
        Self::empty()
    }
}

/// Known instruction file names that the system searches for.
///
/// Ported from: `instruction-context.ts` — built-in file discovery
pub const INSTRUCTION_FILE_NAMES: &[&str] = &[
    "CLAUDE.md",
    "CLAUDE.MD",
    "claude.md",
    "AGENTS.md",
    "AGENTS.MD",
    "agents.md",
    "COPILOT.md",
    "CONTEXT.md",
    ".github/copilot-instructions.md",
];

/// Check if a file name is a recognized instruction file.
pub fn is_instruction_file(name: &str) -> bool {
    INSTRUCTION_FILE_NAMES.contains(&name)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_empty_context() {
        let ctx = InstructionContext::empty();
        assert!(ctx.instructions.is_empty());
        assert_eq!(ctx.total_chars, 0);
        assert!(ctx.scanned_paths.is_empty());
    }

    #[test]
    fn test_add_instruction() {
        let mut ctx = InstructionContext::empty();
        ctx.add(Instruction {
            text: "You are a helpful assistant.".into(),
            source: InstructionOrigin::Builtin {
                name: "system".into(),
            },
            priority: 0,
        });
        assert_eq!(ctx.instructions.len(), 1);
        assert!(ctx.total_chars > 0);
    }

    #[test]
    fn test_add_file_instruction_tracks_path() {
        let mut ctx = InstructionContext::empty();
        ctx.add(Instruction {
            text: "File instructions".into(),
            source: InstructionOrigin::File {
                path: PathBuf::from("/project/CLAUDE.md"),
            },
            priority: 10,
        });
        assert_eq!(ctx.scanned_paths.len(), 1);
        assert_eq!(
            ctx.scanned_paths[0],
            PathBuf::from("/project/CLAUDE.md")
        );
    }

    #[test]
    fn test_dedup_scanned_paths() {
        let mut ctx = InstructionContext::empty();
        let path = PathBuf::from("/project/CLAUDE.md");
        for _ in 0..3 {
            ctx.add(Instruction {
                text: "dup".into(),
                source: InstructionOrigin::File {
                    path: path.clone(),
                },
                priority: 5,
            });
        }
        assert_eq!(ctx.scanned_paths.len(), 1);
    }

    #[test]
    fn test_sort_by_priority() {
        let mut ctx = InstructionContext::empty();
        ctx.add(Instruction {
            text: "low".into(),
            source: InstructionOrigin::Config,
            priority: 100,
        });
        ctx.add(Instruction {
            text: "high".into(),
            source: InstructionOrigin::Builtin {
                name: "system".into(),
            },
            priority: 0,
        });
        ctx.sort_by_priority();
        assert_eq!(ctx.instructions[0].text, "high");
        assert_eq!(ctx.instructions[1].text, "low");
    }

    #[test]
    fn test_render() {
        let mut ctx = InstructionContext::empty();
        ctx.add(Instruction {
            text: "First instruction.".into(),
            source: InstructionOrigin::Config,
            priority: 0,
        });
        ctx.add(Instruction {
            text: "Second instruction.".into(),
            source: InstructionOrigin::Config,
            priority: 1,
        });
        let rendered = ctx.render();
        assert!(rendered.contains("First instruction."));
        assert!(rendered.contains("\n\n"));
        assert!(rendered.contains("Second instruction."));
    }

    #[test]
    fn test_is_instruction_file() {
        assert!(is_instruction_file("CLAUDE.md"));
        assert!(is_instruction_file("AGENTS.md"));
        assert!(!is_instruction_file("main.rs"));
        assert!(!is_instruction_file("README.md"));
    }

    #[test]
    fn test_instruction_origin_serde() {
        let origin = InstructionOrigin::File {
            path: PathBuf::from("/tmp/test.md"),
        };
        let json = serde_json::to_string(&origin).expect("serialize");
        let parsed: InstructionOrigin = serde_json::from_str(&json).expect("deserialize");
        match parsed {
            InstructionOrigin::File { path } => assert_eq!(path, PathBuf::from("/tmp/test.md")),
            _ => panic!("expected file origin"),
        }
    }
}
