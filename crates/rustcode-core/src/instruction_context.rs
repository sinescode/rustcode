//! Instruction context — system context assembly from configuration and environment.
//!
//! Ported from: `packages/core/src/instruction-context.ts` (92 lines)
//! OpenCode commit: 5d0f86606ac30690f79f0a6a9f41a1f49fe95d0b
//!
//! This module provides:
//! - [`InstructionDiscovery`] — filesystem traversal for AGENTS.md instruction files
//! - [`InstructionContextSource`] — [`SystemContextSource`] implementation (key: `core/instructions`)
//! - [`InstructionContext`] — assembled context from all instruction sources
//! - `OPENCODE_DISABLE_PROJECT_CONFIG` env var support

use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

use crate::global;
use crate::system_context::{SystemContextKey, SystemContextSource};

/// Env var that disables project-level config discovery when set to `"1"` or `"true"`.
///
/// When enabled, the upward walk for project `AGENTS.md` files is skipped entirely.
/// Global instructions remain eligible.
///
/// # Source
/// Ported from `packages/core/src/flag/flag.ts` lines 54–55 and
/// `packages/core/src/instruction-context.ts` line 46.
pub const OPENCODE_DISABLE_PROJECT_CONFIG: &str = "OPENCODE_DISABLE_PROJECT_CONFIG";

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

// ── Instruction Discovery ─────────────────────────────────────────────

/// Discovers instruction files on the filesystem.
///
/// Walks from a start directory upward to a project root looking for
/// recognized instruction file names (e.g., `AGENTS.md`). Also discovers
/// global config instruction files.
///
/// # Source
/// Ported from `packages/core/src/instruction-context.ts` lines 39–71 (`observe`).
pub struct InstructionDiscovery {
    /// The global config directory path (e.g., `~/.config/opencode`).
    config_dir: String,
}

impl InstructionDiscovery {
    /// Create a new discovery instance rooted at the global config directory.
    ///
    /// # Source
    /// Ported from `packages/core/src/instruction-context.ts` — `global.config`.
    pub fn new(config_dir: impl Into<String>) -> Self {
        Self {
            config_dir: config_dir.into(),
        }
    }

    /// Create a discovery instance using the default global config path.
    pub fn discover() -> Self {
        Self::new(global::paths().resolve_config_dir())
    }

    /// Walk from `start_dir` upward to `project_root` looking for instruction files.
    ///
    /// Returns an ordered list of found instruction file paths, starting from
    /// `start_dir` and walking upward (deepest first). The search stops at
    /// `project_root` (inclusive) or the filesystem root.
    ///
    /// If `project_root` is `None`, walks to the filesystem root.
    ///
    /// # Source
    /// Ported from `packages/core/src/fs-util.ts` — `FSUtil.up()` method,
    /// and `packages/core/src/instruction-context.ts` lines 48–53.
    pub fn discover_files(start_dir: &Path, project_root: Option<&Path>) -> Vec<PathBuf> {
        let mut results = Vec::new();
        let mut current = start_dir.to_path_buf();

        loop {
            for name in INSTRUCTION_FILE_NAMES {
                let candidate = current.join(name);
                if candidate.is_file() {
                    results.push(candidate);
                }
            }

            if let Some(stop) = project_root {
                if current == stop {
                    break;
                }
            }

            match current.parent() {
                Some(parent) => {
                    if parent == current {
                        break;
                    }
                    current = parent.to_path_buf();
                }
                None => break,
            }
        }

        results
    }

    /// Find the global config `AGENTS.md` file.
    ///
    /// # Source
    /// Ported from `packages/core/src/instruction-context.ts` line 55:
    /// `Array.dedupe([FSUtil.resolve(join(global.config, "AGENTS.md")), ...discovered])`
    pub fn discover_global(&self) -> Vec<PathBuf> {
        let path = PathBuf::from(&self.config_dir).join("AGENTS.md");
        if path.is_file() {
            vec![path]
        } else {
            Vec::new()
        }
    }

    /// Discover and read all instruction files.
    ///
    /// Combines global and project instruction files. Respects the
    /// `OPENCODE_DISABLE_PROJECT_CONFIG` env var — when set, project
    /// discovery is skipped.
    ///
    /// # Source
    /// Ported from `packages/core/src/instruction-context.ts` lines 39–71.
    pub fn load_all(
        &self,
        start_dir: &Path,
        project_root: Option<&Path>,
    ) -> Vec<InstructionSource> {
        let mut paths: Vec<PathBuf> = self.discover_global();

        if !self.is_project_config_disabled() {
            let inside_project = Self::is_inside_project(start_dir, project_root);
            if inside_project {
                let discovered = Self::discover_files(start_dir, project_root);
                for path in discovered {
                    if !paths.contains(&path) {
                        paths.push(path);
                    }
                }
            }
        }

        paths
            .into_iter()
            .filter_map(|path| {
                let content = std::fs::read_to_string(&path).ok()?;
                Some(InstructionSource { path, content })
            })
            .collect()
    }

    /// Check if `OPENCODE_DISABLE_PROJECT_CONFIG` is truthy (`"1"` or `"true"`).
    ///
    /// # Source
    /// Ported from `packages/core/src/flag/flag.ts` lines 54–55.
    pub fn is_project_config_disabled(&self) -> bool {
        is_truthy_env(OPENCODE_DISABLE_PROJECT_CONFIG)
    }

    /// Determine whether `start_dir` is lexically inside `project_root`.
    ///
    /// Uses `Path::strip_prefix` to check containment. If `start_dir` equals
    /// `project_root` or is a descendant of it, returns `true`.
    ///
    /// # Source
    /// Ported from `packages/core/src/instruction-context.ts` lines 42–44 and
    /// `packages/core/src/fs-util.ts` lines 248–251.
    fn is_inside_project(start_dir: &Path, project_root: Option<&Path>) -> bool {
        let stop = match project_root {
            Some(p) => p,
            None => return false,
        };

        // strip_prefix returns Ok(relative) when start_dir == stop or is a descendant
        start_dir.strip_prefix(stop).is_ok()
    }
}

/// Check whether an environment variable is set to a truthy value (`"1"` or `"true"`).
///
/// # Source
/// Ported from `packages/core/src/flag/flag.ts` lines 3–6 (`truthy()`).
fn is_truthy_env(key: &str) -> bool {
    match std::env::var(key) {
        Ok(val) => {
            let lower = val.to_lowercase();
            lower == "true" || lower == "1"
        }
        Err(_) => false,
    }
}

// ── SystemContextSource implementation ─────────────────────────────────

/// System context source for instruction files.
///
/// Registers as `core/instructions` and provides filesystem-based instruction
/// discovery and rendering.
///
/// # Source
/// Ported from `packages/core/src/instruction-context.ts` lines 28–37 and 73–86.
pub struct InstructionContextSource {
    /// The discovery instance for finding instruction files.
    discovery: InstructionDiscovery,
    /// The current working directory (start for upward walk).
    start_dir: PathBuf,
    /// The project root (stop for upward walk).
    project_root: Option<PathBuf>,
    /// The stable key for this source.
    key: SystemContextKey,
}

impl InstructionContextSource {
    /// Create a new instruction context source.
    pub fn new(
        config_dir: impl Into<String>,
        start_dir: PathBuf,
        project_root: Option<PathBuf>,
    ) -> Self {
        Self {
            discovery: InstructionDiscovery::new(config_dir),
            start_dir,
            project_root,
            key: SystemContextKey::new("core/instructions")
                .expect("core/instructions is a valid key"),
        }
    }

    /// Load instruction files from the filesystem.
    ///
    /// Returns a JSON array of `{path, content}` objects.
    fn load_files(&self) -> Result<serde_json::Value, Box<dyn std::error::Error + Send + Sync>> {
        let sources = self
            .discovery
            .load_all(&self.start_dir, self.project_root.as_deref());
        let files: Vec<serde_json::Value> = sources
            .iter()
            .map(|s| {
                serde_json::json!({
                    "path": s.path.to_string_lossy(),
                    "content": s.content,
                })
            })
            .collect();
        Ok(serde_json::json!(files))
    }
}

impl SystemContextSource for InstructionContextSource {
    /// Stable key: `core/instructions`.
    ///
    /// # Source
    /// Ported from `packages/core/src/instruction-context.ts` line 19.
    fn key(&self) -> &SystemContextKey {
        &self.key
    }

    /// Load the current instruction files as a JSON array.
    ///
    /// # Source
    /// Ported from `packages/core/src/instruction-context.ts` lines 39–71.
    fn load(&self) -> Result<serde_json::Value, Box<dyn std::error::Error + Send + Sync>> {
        self.load_files()
    }

    /// Render baseline text from loaded instruction files.
    ///
    /// Format: `"Instructions from: {path}\n{content}"` joined by `"\n\n"`.
    ///
    /// # Source
    /// Ported from `packages/core/src/instruction-context.ts` lines 90–92 (`render`).
    fn baseline(&self, data: &serde_json::Value) -> String {
        render_instruction_files(data)
    }

    /// Render update text — superseded instructions.
    ///
    /// Format: `"These instructions replace all previously loaded ambient instructions.\n\n" + render(current)`.
    ///
    /// # Source
    /// Ported from `packages/core/src/instruction-context.ts` lines 34–35.
    fn update(&self, data: &serde_json::Value) -> String {
        format!(
            "These instructions replace all previously loaded ambient instructions.\n\n{}",
            render_instruction_files(data)
        )
    }

    /// Render removal text when all instructions are dropped.
    ///
    /// # Source
    /// Ported from `packages/core/src/instruction-context.ts` line 36.
    fn removed(&self) -> String {
        "Previously loaded instructions no longer apply.".to_string()
    }
}

/// Render a JSON array of `{path, content}` files into instruction text.
///
/// # Source
/// Ported from `packages/core/src/instruction-context.ts` lines 90–92.
fn render_instruction_files(data: &serde_json::Value) -> String {
    let files = match data.as_array() {
        Some(arr) => arr,
        None => return String::new(),
    };

    files
        .iter()
        .filter_map(|file| {
            let path = file.get("path")?.as_str()?;
            let content = file.get("content")?.as_str()?;
            Some(format!("Instructions from: {path}\n{content}"))
        })
        .collect::<Vec<_>>()
        .join("\n\n")
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
    use std::fs;

    // ── InstructionContext ─────────────────────────────────────────

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
        assert_eq!(ctx.scanned_paths[0], PathBuf::from("/project/CLAUDE.md"));
    }

    #[test]
    fn test_dedup_scanned_paths() {
        let mut ctx = InstructionContext::empty();
        let path = PathBuf::from("/project/CLAUDE.md");
        for _ in 0..3 {
            ctx.add(Instruction {
                text: "dup".into(),
                source: InstructionOrigin::File { path: path.clone() },
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

    // ── InstructionDiscovery ───────────────────────────────────────

    #[test]
    fn test_discover_files_walks_upward() {
        let tmp = std::env::temp_dir().join("rustcode_test_discovery");
        let _ = fs::remove_dir_all(&tmp);
        let project = tmp.join("project");
        let nested = project.join("packages").join("core");
        fs::create_dir_all(&nested).expect("create dirs");

        let agents_md = nested.join("AGENTS.md");
        fs::write(&agents_md, "package instructions").expect("write file");

        let result = InstructionDiscovery::discover_files(&nested, Some(&project));
        assert_eq!(result.len(), 1);
        assert_eq!(result[0], agents_md);

        let _ = fs::remove_dir_all(&tmp);
    }

    #[test]
    fn test_discover_files_finds_multiple_levels() {
        let tmp = std::env::temp_dir().join("rustcode_test_discovery_multi");
        let _ = fs::remove_dir_all(&tmp);
        let project = tmp.join("project");
        let nested = project.join("packages").join("core");
        fs::create_dir_all(&nested).expect("create dirs");

        fs::write(nested.join("AGENTS.md"), "package").expect("write package");
        fs::write(project.join("AGENTS.md"), "project").expect("write project");

        let result = InstructionDiscovery::discover_files(&nested, Some(&project));
        assert_eq!(result.len(), 2);
        // Deepest first
        assert!(result[0].to_string_lossy().contains("packages"));
        assert!(result[1].to_string_lossy().contains("project/AGENTS.md"));

        let _ = fs::remove_dir_all(&tmp);
    }

    #[test]
    fn test_discover_files_stops_at_project_root() {
        let tmp = std::env::temp_dir().join("rustcode_test_discovery_stop");
        let _ = fs::remove_dir_all(&tmp);
        let project = tmp.join("project");
        fs::create_dir_all(&project).expect("create dirs");

        // Place AGENTS.md above the project root — should NOT be found
        fs::write(tmp.join("AGENTS.md"), "outside").expect("write outside");

        let result = InstructionDiscovery::discover_files(&project, Some(&project));
        assert!(result.is_empty());

        let _ = fs::remove_dir_all(&tmp);
    }

    #[test]
    fn test_discover_files_no_project_root_walks_to_fs_root() {
        let tmp = std::env::temp_dir().join("rustcode_test_discovery_noroot");
        let _ = fs::remove_dir_all(&tmp);
        fs::create_dir_all(&tmp).expect("create dirs");

        fs::write(tmp.join("AGENTS.md"), "found").expect("write file");

        let result = InstructionDiscovery::discover_files(&tmp, None);
        assert_eq!(result.len(), 1);
        assert_eq!(result[0], tmp.join("AGENTS.md"));

        let _ = fs::remove_dir_all(&tmp);
    }

    #[test]
    fn test_discover_global_finds_config_agents_md() {
        let tmp = std::env::temp_dir().join("rustcode_test_discovery_global");
        let _ = fs::remove_dir_all(&tmp);
        fs::create_dir_all(&tmp).expect("create dirs");

        fs::write(tmp.join("AGENTS.md"), "global instructions").expect("write file");

        let discovery = InstructionDiscovery::new(tmp.to_string_lossy());
        let result = discovery.discover_global();
        assert_eq!(result.len(), 1);
        assert_eq!(result[0], tmp.join("AGENTS.md"));

        let _ = fs::remove_dir_all(&tmp);
    }

    #[test]
    fn test_discover_global_missing_returns_empty() {
        let tmp = std::env::temp_dir().join("rustcode_test_discovery_global_missing");
        let _ = fs::remove_dir_all(&tmp);
        fs::create_dir_all(&tmp).expect("create dirs");

        let discovery = InstructionDiscovery::new(tmp.to_string_lossy());
        let result = discovery.discover_global();
        assert!(result.is_empty());

        let _ = fs::remove_dir_all(&tmp);
    }

    #[test]
    fn test_load_all_combines_global_and_project() {
        let tmp = tempfile::tempdir().expect("tempdir");
        let global_config = tmp.path().join("global");
        let project = tmp.path().join("project");
        let nested = project.join("src");
        fs::create_dir_all(&nested).expect("create dirs");
        fs::create_dir_all(&global_config).expect("create dirs");

        fs::write(global_config.join("AGENTS.md"), "global").expect("write global");
        fs::write(project.join("AGENTS.md"), "project").expect("write project");
        fs::write(nested.join("AGENTS.md"), "nested").expect("write nested");

        let discovery = InstructionDiscovery::new(global_config.to_string_lossy());
        let sources = discovery.load_all(&nested, Some(&project));
        assert_eq!(sources.len(), 3);

        let contents: Vec<&str> = sources.iter().map(|s| s.content.as_str()).collect();
        assert!(contents.contains(&"global"));
        assert!(contents.contains(&"project"));
        assert!(contents.contains(&"nested"));
    }

    #[test]
    fn test_load_all_respects_disable_project_config() {
        let tmp = std::env::temp_dir().join("rustcode_test_load_all_disable");
        let _ = fs::remove_dir_all(&tmp);
        let global_config = tmp.join("global");
        let project = tmp.join("project");
        fs::create_dir_all(&project).expect("create dirs");
        fs::create_dir_all(&global_config).expect("create dirs");

        fs::write(global_config.join("AGENTS.md"), "global").expect("write global");
        fs::write(project.join("AGENTS.md"), "project").expect("write project");

        // Set the env var to disable project config
        std::env::set_var(OPENCODE_DISABLE_PROJECT_CONFIG, "1");

        let discovery = InstructionDiscovery::new(global_config.to_string_lossy());
        let sources = discovery.load_all(&project, Some(&project));
        // Only global should be found
        assert_eq!(sources.len(), 1);
        assert_eq!(sources[0].content, "global");

        std::env::remove_var(OPENCODE_DISABLE_PROJECT_CONFIG);
        let _ = fs::remove_dir_all(&tmp);
    }

    #[test]
    fn test_is_inside_project_same_dir() {
        let dir = PathBuf::from("/home/user/project");
        assert!(InstructionDiscovery::is_inside_project(&dir, Some(&dir)));
    }

    #[test]
    fn test_is_inside_project_descendant() {
        let project = PathBuf::from("/home/user/project");
        let nested = PathBuf::from("/home/user/project/src");
        assert!(InstructionDiscovery::is_inside_project(
            &nested,
            Some(&project)
        ));
    }

    #[test]
    fn test_is_inside_project_outside() {
        let project = PathBuf::from("/home/user/project");
        let outside = PathBuf::from("/home/user/other");
        assert!(!InstructionDiscovery::is_inside_project(
            &outside,
            Some(&project)
        ));
    }

    #[test]
    fn test_is_inside_project_no_root() {
        let dir = PathBuf::from("/home/user/project");
        assert!(!InstructionDiscovery::is_inside_project(&dir, None));
    }

    // ── InstructionContextSource (SystemContextSource) ──────────────

    #[test]
    fn test_source_key() {
        let tmp = std::env::temp_dir().join("rustcode_test_source_key");
        let _ = fs::remove_dir_all(&tmp);
        fs::create_dir_all(&tmp).expect("create dirs");

        let source = InstructionContextSource::new(tmp.to_string_lossy(), tmp.clone(), None);
        assert_eq!(source.key().as_str(), "core/instructions");

        let _ = fs::remove_dir_all(&tmp);
    }

    #[test]
    fn test_source_load_returns_json_array() {
        let tmp = std::env::temp_dir().join("rustcode_test_source_load");
        let _ = fs::remove_dir_all(&tmp);
        fs::create_dir_all(&tmp).expect("create dirs");
        fs::write(tmp.join("AGENTS.md"), "hello").expect("write file");

        let source = InstructionContextSource::new(tmp.to_string_lossy(), tmp.clone(), None);
        let data = source.load().expect("load");
        let arr = data.as_array().expect("should be array");
        assert_eq!(arr.len(), 1);
        assert_eq!(arr[0]["content"], "hello");

        let _ = fs::remove_dir_all(&tmp);
    }

    #[test]
    fn test_source_baseline_renders_correctly() {
        let tmp = std::env::temp_dir().join("rustcode_test_source_baseline");
        let _ = fs::remove_dir_all(&tmp);
        fs::create_dir_all(&tmp).expect("create dirs");
        fs::write(tmp.join("AGENTS.md"), "test content").expect("write file");

        let source = InstructionContextSource::new(tmp.to_string_lossy(), tmp.clone(), None);
        let data = source.load().expect("load");
        let text = source.baseline(&data);
        assert!(text.contains("Instructions from:"));
        assert!(text.contains("AGENTS.md"));
        assert!(text.contains("test content"));

        let _ = fs::remove_dir_all(&tmp);
    }

    #[test]
    fn test_source_update_supersedes() {
        let source = InstructionContextSource::new("/tmp", PathBuf::from("/tmp"), None);
        let data = serde_json::json!([{"path": "/tmp/AGENTS.md", "content": "new content"}]);
        let text = source.update(&data);
        assert!(
            text.contains("These instructions replace all previously loaded ambient instructions.")
        );
        assert!(text.contains("Instructions from: /tmp/AGENTS.md"));
        assert!(text.contains("new content"));
    }

    #[test]
    fn test_source_removed_text() {
        let source = InstructionContextSource::new("/tmp", PathBuf::from("/tmp"), None);
        let text = source.removed();
        assert_eq!(text, "Previously loaded instructions no longer apply.");
    }

    #[test]
    fn test_source_load_empty_when_no_files() {
        let tmp = std::env::temp_dir().join("rustcode_test_source_empty");
        let _ = fs::remove_dir_all(&tmp);
        fs::create_dir_all(&tmp).expect("create dirs");

        let source = InstructionContextSource::new(tmp.to_string_lossy(), tmp.clone(), None);
        let data = source.load().expect("load");
        let arr = data.as_array().expect("should be array");
        assert!(arr.is_empty());

        // baseline of empty should be empty string
        let text = source.baseline(&data);
        assert!(text.is_empty());

        let _ = fs::remove_dir_all(&tmp);
    }

    // ── render_instruction_files ───────────────────────────────────

    #[test]
    fn test_render_instruction_files_single() {
        let data = serde_json::json!([
            {"path": "/project/AGENTS.md", "content": "use rustfmt"}
        ]);
        let text = render_instruction_files(&data);
        assert_eq!(text, "Instructions from: /project/AGENTS.md\nuse rustfmt");
    }

    #[test]
    fn test_render_instruction_files_multiple() {
        let data = serde_json::json!([
            {"path": "/global/AGENTS.md", "content": "global rules"},
            {"path": "/project/AGENTS.md", "content": "project rules"}
        ]);
        let text = render_instruction_files(&data);
        assert!(text.contains("Instructions from: /global/AGENTS.md\nglobal rules"));
        assert!(text.contains("\n\n"));
        assert!(text.contains("Instructions from: /project/AGENTS.md\nproject rules"));
    }

    #[test]
    fn test_render_instruction_files_empty() {
        let data = serde_json::json!([]);
        let text = render_instruction_files(&data);
        assert!(text.is_empty());
    }

    #[test]
    fn test_render_instruction_files_non_array() {
        let data = serde_json::json!({});
        let text = render_instruction_files(&data);
        assert!(text.is_empty());
    }

    // ── is_truthy_env ─────────────────────────────────────────────

    #[test]
    fn test_is_truthy_env_one() {
        std::env::set_var("RUSTCODE_TEST_TRUTHY_1", "1");
        assert!(is_truthy_env("RUSTCODE_TEST_TRUTHY_1"));
        std::env::remove_var("RUSTCODE_TEST_TRUTHY_1");
    }

    #[test]
    fn test_is_truthy_env_true() {
        std::env::set_var("RUSTCODE_TEST_TRUTHY_TRUE", "true");
        assert!(is_truthy_env("RUSTCODE_TEST_TRUTHY_TRUE"));
        std::env::remove_var("RUSTCODE_TEST_TRUTHY_TRUE");
    }

    #[test]
    fn test_is_truthy_env_case_insensitive() {
        std::env::set_var("RUSTCODE_TEST_TRUTHY_TRUE_UPPER", "TRUE");
        assert!(is_truthy_env("RUSTCODE_TEST_TRUTHY_TRUE_UPPER"));
        std::env::remove_var("RUSTCODE_TEST_TRUTHY_TRUE_UPPER");
    }

    #[test]
    fn test_is_truthy_env_false() {
        std::env::set_var("RUSTCODE_TEST_TRUTHY_FALSE", "false");
        assert!(!is_truthy_env("RUSTCODE_TEST_TRUTHY_FALSE"));
        std::env::remove_var("RUSTCODE_TEST_TRUTHY_FALSE");
    }

    #[test]
    fn test_is_truthy_env_unset() {
        std::env::remove_var("RUSTCODE_TEST_TRUTHY_UNSET");
        assert!(!is_truthy_env("RUSTCODE_TEST_TRUTHY_UNSET"));
    }

    // ── Integration: full source load → baseline → reconcile ───────

    #[test]
    fn test_full_source_lifecycle() {
        let tmp = std::env::temp_dir().join("rustcode_test_full_lifecycle");
        let _ = fs::remove_dir_all(&tmp);
        let global_config = tmp.join("global");
        let project = tmp.join("project");
        let nested = project.join("src");
        fs::create_dir_all(&global_config).expect("create global dir");
        fs::create_dir_all(&nested).expect("create dirs");

        fs::write(global_config.join("AGENTS.md"), "global rules").expect("write global");
        fs::write(project.join("AGENTS.md"), "project rules").expect("write project");
        fs::write(nested.join("AGENTS.md"), "package rules").expect("write nested");

        let source = InstructionContextSource::new(
            global_config.to_string_lossy(),
            nested.clone(),
            Some(project.clone()),
        );
        let data = source.load().expect("load");

        // Baseline
        let baseline = source.baseline(&data);
        assert!(baseline.contains("global rules"));
        assert!(baseline.contains("project rules"));
        assert!(baseline.contains("package rules"));

        // Simulate file removal — only global remains
        fs::remove_file(nested.join("AGENTS.md")).expect("remove nested");
        fs::remove_file(project.join("AGENTS.md")).expect("remove project");

        let updated_data = source.load().expect("load after removal");
        let updated_baseline = source.baseline(&updated_data);
        assert!(updated_baseline.contains("global rules"));
        assert!(!updated_baseline.contains("package rules"));
        assert!(!updated_baseline.contains("project rules"));

        // Removed text
        assert_eq!(
            source.removed(),
            "Previously loaded instructions no longer apply."
        );

        let _ = fs::remove_dir_all(&tmp);
    }
}
