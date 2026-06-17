//! Skill discovery and management.
//!
//! Skills are reusable agent behaviors defined in Markdown files with YAML
//! frontmatter. They live in `.opencode/skill/`, `.opencode/skills/`,
//! `.claude/skills/`, `.agents/skills/`, and configurable paths.
//!
//! # Source
//! Ported from:
//! - `packages/opencode/src/skill/index.ts` — Skill service, discovery, loading, errors
//! - `packages/opencode/src/skill/discovery.ts` — Remote skill discovery (HTTP pull)
//! - `packages/core/src/skill/discovery.ts` — Core skill discovery (index fetch, safe path validation)
//! - `packages/core/src/skill/guidance.ts` — Skill guidance context injection
//! - `packages/core/src/plugin/skill.ts` — Embedded `customize-opencode` skill
//!
//! OpenCode commit: 5d0f86606ac30690f79f0a6a9f41a1f49fe95d0b

use std::collections::HashMap;
use std::path::{Path, PathBuf};

pub use crate::error::SkillError;

// ── Skill frontmatter ─────────────────────────────────────────────────

/// Parsed YAML frontmatter from a skill Markdown file.
///
/// Ported from `packages/opencode/src/skill/index.ts` `isSkillFrontmatter()`.
#[derive(Debug, Clone, serde::Deserialize)]
struct SkillFrontmatter {
    /// Skill name (required).
    name: String,
    /// Optional description.
    #[serde(default)]
    description: Option<String>,
}

// ── Skill info ────────────────────────────────────────────────────────

/// A loaded skill definition.
///
/// Ported from `packages/opencode/src/skill/index.ts` `Info`.
#[derive(Debug, Clone)]
pub struct Skill {
    /// Skill name (from frontmatter).
    pub name: String,
    /// Optional human-readable description.
    pub description: Option<String>,
    /// Filesystem path to the SKILL.md file (or "<built-in>" for embedded).
    pub location: String,
    /// The markdown body content (everything after the frontmatter).
    pub content: String,
}

impl Skill {
    /// Create a built-in skill (not from a file).
    pub fn builtin(name: impl Into<String>, description: impl Into<String>, content: impl Into<String>) -> Self {
        Self {
            name: name.into(),
            description: Some(description.into()),
            location: "<built-in>".to_string(),
            content: content.into(),
        }
    }
}

// ── YAML frontmatter extraction ───────────────────────────────────────

/// Errors that can occur when parsing a skill file.
#[derive(Debug, thiserror::Error)]
pub enum ParseError {
    /// The file could not be read.
    #[error("failed to read skill file `{path}`: {source}")]
    Read {
        path: String,
        source: std::io::Error,
    },

    /// The file has no YAML frontmatter (no opening `---`).
    #[error("no frontmatter found in `{path}`")]
    NoFrontmatter { path: String },

    /// The YAML frontmatter could not be parsed.
    #[error("invalid YAML frontmatter in `{path}`: {message}")]
    InvalidYaml { path: String, message: String },

    /// The frontmatter is missing the required `name` field.
    #[error("missing required `name` field in frontmatter of `{path}`")]
    MissingName { path: String },
}

/// Extract and parse YAML frontmatter from a Markdown file's contents.
///
/// Frontmatter is delimited by `---` lines at the very beginning of the file.
/// Returns `(SkillFrontmatter, body_content)` on success.
///
/// Ported from the logic in `packages/opencode/src/skill/index.ts` `add()` function,
/// which uses `ConfigMarkdown.parse()` internally.
fn extract_frontmatter(content: &str, path: &str) -> Result<(SkillFrontmatter, String), ParseError> {
    let trimmed = content.trim_start();

    // Frontmatter must start with --- at the very beginning
    let without_opening = match trimmed.strip_prefix("---\n") {
        Some(rest) => rest,
        None => match trimmed.strip_prefix("---\r\n") {
            Some(rest) => rest,
            None => {
                // Also handle just "---" with no newline (empty frontmatter)
                if trimmed == "---" {
                    return Err(ParseError::NoFrontmatter {
                        path: path.to_string(),
                    });
                }
                return Err(ParseError::NoFrontmatter {
                    path: path.to_string(),
                });
            }
        },
    };

    // Find the closing ---
    let (yaml_str, body) = if let Some(end_pos) = without_opening.find("\n---") {
        // Standard: closing --- on its own line
        let yaml_part = &without_opening[..end_pos];
        let body_part = without_opening[end_pos + 4..].trim_start();
        (yaml_part, body_part)
    } else if let Some(end_pos) = without_opening.find("\r\n---") {
        let yaml_part = &without_opening[..end_pos];
        let body_part = without_opening[end_pos + 5..].trim_start();
        (yaml_part, body_part)
    } else {
        return Err(ParseError::NoFrontmatter {
            path: path.to_string(),
        });
    };

    let frontmatter: SkillFrontmatter = serde_yaml::from_str(yaml_str).map_err(|e| ParseError::InvalidYaml {
        path: path.to_string(),
        message: e.to_string(),
    })?;

    if frontmatter.name.trim().is_empty() {
        return Err(ParseError::MissingName {
            path: path.to_string(),
        });
    }

    Ok((frontmatter, body.to_string()))
}

/// Parse a skill Markdown file, returning a [`Skill`] on success.
///
/// Handles reading the file and extracting frontmatter + body content.
pub fn parse_skill_file(file_path: &Path) -> Result<Option<Skill>, ParseError> {
    let content = std::fs::read_to_string(file_path).map_err(|e| ParseError::Read {
        path: file_path.display().to_string(),
        source: e,
    })?;

    let (fm, body) = match extract_frontmatter(&content, &file_path.display().to_string()) {
        Ok(result) => result,
        Err(ParseError::NoFrontmatter { .. }) => return Ok(None),
        Err(e) => return Err(e),
    };

    Ok(Some(Skill {
        name: fm.name,
        description: fm.description.filter(|d| !d.is_empty()),
        location: file_path.display().to_string(),
        content: body,
    }))
}

// ── Skill discovery ───────────────────────────────────────────────────

/// Search directory patterns for SKILL.md files.
const SKILL_PATTERNS: &[&str] = &[
    ".opencode/skill/**/SKILL.md",
    ".opencode/skills/**/SKILL.md",
];

/// External skill directory names (relative to home or project root).
///
/// Ported from `packages/opencode/src/skill/index.ts` constants.
const EXTERNAL_DIRS: &[&str] = &[".claude", ".agents"];

/// Pattern to match SKILL.md files within external skill directories.
const EXTERNAL_SKILL_PATTERN: &str = "skills/**/SKILL.md";

/// Discover skill files from a given root directory using glob patterns.
///
/// Returns a list of matching file paths.
///
/// Ported from `packages/opencode/src/skill/index.ts` `scan()` function.
fn glob_skills(root: &Path, pattern: &str) -> Vec<PathBuf> {
    let full_pattern = root.join(pattern);
    let pattern_str = full_pattern.display().to_string();

    match glob::glob(&pattern_str) {
        Ok(paths) => paths.filter_map(|entry| entry.ok()).collect(),
        Err(_) => Vec::new(),
    }
}

/// Discover skill files from external directories (`.claude/skills/`, `.agents/skills/`).
///
/// Searches both the given root directory and walks up to find ancestor directories
/// that contain these external skill patterns.
fn glob_external_skills(root: &Path, dirs: &[&str]) -> Vec<PathBuf> {
    let mut matches = Vec::new();

    // Walk up from root to find ancestor directories with external skill dirs
    let mut current = Some(root.to_path_buf());
    while let Some(dir) = current {
        for ext_dir in dirs {
            let ext_root = dir.join(ext_dir);
            if ext_root.is_dir() {
                let found = glob_skills(&ext_root, EXTERNAL_SKILL_PATTERN);
                matches.extend(found);
            }
        }
        current = dir.parent().map(Path::to_path_buf);
    }

    matches
}

/// Discover skill files from the home directory's external skill dirs.
///
/// Searches `~/.claude/skills/` and `~/.agents/skills/`.
pub fn glob_home_skills(home: &Path) -> Vec<PathBuf> {
    let mut matches = Vec::new();
    for ext_dir in EXTERNAL_DIRS {
        let ext_root = home.join(ext_dir);
        if ext_root.is_dir() {
            let found = glob_skills(&ext_root, EXTERNAL_SKILL_PATTERN);
            matches.extend(found);
        }
    }
    matches
}

/// Discover skills from the filesystem by scanning standard directories.
///
/// Searches in order:
/// 1. `.opencode/skill/` and `.opencode/skills/` under `worktree`
/// 2. `.claude/skills/` and `.agents/skills/` walking up from `directory`
/// 3. `~/.claude/skills/` and `~/.agents/skills/`
/// 4. Additional paths from `extra_paths`
///
/// Returns deduplicated list of SKILL.md file paths.
///
/// Ported from `packages/opencode/src/skill/index.ts` `discoverSkills()`.
pub fn discover_skill_files(
    worktree: &Path,
    directory: &Path,
    home: &Path,
    extra_paths: &[PathBuf],
    disable_external: bool,
) -> Vec<PathBuf> {
    let mut seen = std::collections::HashSet::new();
    let mut matches = Vec::new();

    // 1. Scan .opencode/skill/ and .opencode/skills/ under worktree
    for pattern in SKILL_PATTERNS {
        for path in glob_skills(worktree, pattern) {
            if seen.insert(path.clone()) {
                matches.push(path);
            }
        }
    }

    // Also scan .opencode/ under directory if different from worktree
    if directory != worktree {
        for pattern in SKILL_PATTERNS {
            for path in glob_skills(directory, pattern) {
                if seen.insert(path.clone()) {
                    matches.push(path);
                }
            }
        }
    }

    // 2. Scan external dirs walking up from directory and worktree
    if !disable_external {
        for path in glob_external_skills(directory, EXTERNAL_DIRS) {
            if seen.insert(path.clone()) {
                matches.push(path);
            }
        }
        if directory != worktree {
            for path in glob_external_skills(worktree, EXTERNAL_DIRS) {
                if seen.insert(path.clone()) {
                    matches.push(path);
                }
            }
        }

        // 3. Scan home directory
        for path in glob_home_skills(home) {
            if seen.insert(path.clone()) {
                matches.push(path);
            }
        }
    }

    // 4. Scan additional paths
    for extra in extra_paths {
        if extra.is_dir() {
            for path in glob_skills(extra, "**/SKILL.md") {
                if seen.insert(path.clone()) {
                    matches.push(path);
                }
            }
        }
    }

    matches
}

// ── Skill registry ────────────────────────────────────────────────────

/// A registry of loaded skills, keyed by name.
///
/// Ported from `packages/opencode/src/skill/index.ts` `State.skills` record.
#[derive(Debug, Default)]
pub struct SkillRegistry {
    /// Skills keyed by name.
    skills: HashMap<String, Skill>,
    /// Directories that skills were discovered in.
    dirs: Vec<PathBuf>,
}

impl SkillRegistry {
    /// Create an empty skill registry.
    pub fn new() -> Self {
        Self {
            skills: HashMap::new(),
            dirs: Vec::new(),
        }
    }

    /// Register a skill in the registry.
    ///
    /// If a skill with the same name already exists, logs a warning and replaces it
    /// (matching the TS behavior where user skills override built-in and later
    /// skills override earlier ones within the same name).
    ///
    /// Returns the previous skill if one was replaced.
    pub fn register(&mut self, skill: Skill) -> Option<Skill> {
        // Track the directory this skill lives in
        if let Some(parent) = Path::new(&skill.location).parent() {
            if !self.dirs.iter().any(|d| d == parent) {
                self.dirs.push(parent.to_path_buf());
            }
        }

        let prev = self.skills.insert(skill.name.clone(), skill);
        if prev.is_some() {
            tracing::warn!(
                "duplicate skill name detected, later skill overrides earlier one"
            );
        }
        prev
    }

    /// Look up a skill by name.
    pub fn get(&self, name: &str) -> Option<&Skill> {
        self.skills.get(name)
    }

    /// Require a skill by name, returning an error if not found.
    pub fn require(&self, name: &str) -> Result<&Skill, SkillError> {
        self.skills
            .get(name)
            .ok_or_else(|| SkillError::NotFound {
                name: name.to_string(),
            })
    }

    /// Return all registered skills.
    pub fn all(&self) -> Vec<&Skill> {
        let mut skills: Vec<&Skill> = self.skills.values().collect();
        skills.sort_by(|a, b| a.name.cmp(&b.name));
        skills
    }

    /// Return directories where skills were discovered.
    pub fn dirs(&self) -> &[PathBuf] {
        &self.dirs
    }

    /// Number of registered skills.
    pub fn count(&self) -> usize {
        self.skills.len()
    }

    /// Check if any skill with the given name exists.
    pub fn contains(&self, name: &str) -> bool {
        self.skills.contains_key(name)
    }

    /// Return the names of all registered skills, sorted.
    pub fn names(&self) -> Vec<&str> {
        let mut names: Vec<&str> = self.skills.keys().map(|s| s.as_str()).collect();
        names.sort_unstable();
        names
    }
}

/// Discover and load all skills into a registry.
///
/// This is the main entry point: it discovers SKILL.md files using
/// [`discover_skill_files`], parses each one, and registers valid skills.
///
/// Skills without frontmatter or with invalid frontmatter are silently skipped
/// (matching the TS behavior where `isSkillFrontmatter` returns false and no
/// error is raised).
///
/// Ported from `packages/opencode/src/skill/index.ts` `loadSkills()`.
pub fn discover_and_load(
    worktree: &Path,
    directory: &Path,
    home: &Path,
    extra_paths: &[PathBuf],
    disable_external: bool,
) -> SkillRegistry {
    let mut registry = SkillRegistry::new();

    // Register the built-in customize-opencode skill first so user skills can override it
    registry.register(Skill::builtin(
        "customize-opencode",
        "Use ONLY when the user is editing or creating opencode's own configuration: opencode.json, opencode.jsonc, files under .opencode/, or files under ~/.config/opencode/. Also use when creating or fixing opencode agents, subagents, skills, plugins, MCP servers, or permission rules. Do not use for the user's own application code, or for any project that is not configuring opencode itself.",
        include_str!("skill/customize-opencode.md"),
    ));

    let files = discover_skill_files(worktree, directory, home, extra_paths, disable_external);

    let mut matched_count = 0;
    for file_path in &files {
        match parse_skill_file(file_path) {
            Ok(Some(skill)) => {
                registry.register(skill);
                matched_count += 1;
            }
            Ok(None) => {
                // No frontmatter — silently skip (matching TS behavior)
            }
            Err(e) => {
                tracing::warn!("failed to parse skill file: {}", e);
            }
        }
    }

    tracing::info!(
        "loaded {} skills from {} discovered files",
        matched_count,
        files.len()
    );

    registry
}

/// Format a list of skills for display to the user.
///
/// Ported from `packages/opencode/src/skill/index.ts` `fmt()`.
pub fn format_skill_list(skills: &[&Skill], verbose: bool) -> String {
    let described: Vec<&Skill> = skills
        .iter()
        .filter(|s| s.description.is_some())
        .copied()
        .collect();

    if described.is_empty() {
        return "No skills are currently available.".to_string();
    }

    if verbose {
        let mut lines = vec!["<available_skills>".to_string()];
        for skill in &described {
            lines.push("  <skill>".to_string());
            lines.push(format!("    <name>{}</name>", skill.name));
            lines.push(format!(
                "    <description>{}</description>",
                skill.description.as_deref().unwrap_or("")
            ));
            lines.push(format!("    <location>{}</location>", skill.location));
            lines.push("  </skill>".to_string());
        }
        lines.push("</available_skills>".to_string());
        lines.join("\n")
    } else {
        let mut lines = vec!["## Available Skills".to_string()];
        for skill in &described {
            lines.push(format!(
                "- **{}**: {}",
                skill.name,
                skill.description.as_deref().unwrap_or("")
            ));
        }
        lines.join("\n")
    }
}

// ── Tests ─────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    // ── Frontmatter extraction tests ───────────────────────────────

    #[test]
    fn test_extract_frontmatter_basic() {
        let content = "---\nname: test-skill\ndescription: A skill for testing\n---\n\n# Body\n\nInstructions here.\n";
        let (fm, body) =
            extract_frontmatter(content, "test.md").expect("should parse frontmatter");
        assert_eq!(fm.name, "test-skill");
        assert_eq!(fm.description.as_deref(), Some("A skill for testing"));
        assert!(body.contains("# Body"));
        assert!(body.contains("Instructions here."));
    }

    #[test]
    fn test_extract_frontmatter_no_description() {
        let content = "---\nname: simple-skill\n---\n\n# Simple\n\nJust content.\n";
        let (fm, body) =
            extract_frontmatter(content, "test.md").expect("should parse frontmatter");
        assert_eq!(fm.name, "simple-skill");
        assert!(fm.description.is_none());
        assert!(body.contains("# Simple"));
    }

    #[test]
    fn test_extract_frontmatter_windows_newlines() {
        let content = "---\r\nname: win-skill\r\ndescription: Windows newlines\r\n---\r\n\r\n# Body\r\n";
        let (fm, _body) =
            extract_frontmatter(content, "test.md").expect("should parse frontmatter");
        assert_eq!(fm.name, "win-skill");
        assert_eq!(fm.description.as_deref(), Some("Windows newlines"));
    }

    #[test]
    fn test_extract_frontmatter_no_frontmatter() {
        let content = "# Just a heading\n\nNo frontmatter here.\n";
        let err = extract_frontmatter(content, "test.md").unwrap_err();
        assert!(matches!(err, ParseError::NoFrontmatter { .. }));
    }

    #[test]
    fn test_extract_frontmatter_missing_name() {
        let content = "---\ndescription: No name field\n---\n\nBody\n";
        let err = extract_frontmatter(content, "test.md").unwrap_err();
        assert!(matches!(err, ParseError::MissingName { .. }));
    }

    #[test]
    fn test_extract_frontmatter_empty_name() {
        let content = "---\nname: \"\"\n---\n\nBody\n";
        let err = extract_frontmatter(content, "test.md").unwrap_err();
        assert!(matches!(err, ParseError::MissingName { .. }));
    }

    #[test]
    fn test_extract_frontmatter_invalid_yaml() {
        let content = "---\nname: [unclosed bracket\n---\n\nBody\n";
        let err = extract_frontmatter(content, "test.md").unwrap_err();
        assert!(matches!(err, ParseError::InvalidYaml { .. }));
    }

    // ── Skill registry tests ───────────────────────────────────────

    #[test]
    fn test_registry_register_and_get() {
        let mut registry = SkillRegistry::new();
        let skill = Skill::builtin("my-skill", "Test skill", "# Content");
        registry.register(skill.clone());

        let found = registry.get("my-skill");
        assert!(found.is_some());
        assert_eq!(found.unwrap().name, "my-skill");
        assert_eq!(found.unwrap().description.as_deref(), Some("Test skill"));
    }

    #[test]
    fn test_registry_require_missing() {
        let registry = SkillRegistry::new();
        let err = registry.require("nonexistent").unwrap_err();
        assert!(matches!(err, SkillError::NotFound { .. }));
        assert!(err.to_string().contains("nonexistent"));
    }

    #[test]
    fn test_registry_duplicate_override() {
        let mut registry = SkillRegistry::new();
        let s1 = Skill::builtin("dup", "First", "# First");
        let s2 = Skill::builtin("dup", "Second", "# Second");
        registry.register(s1);
        let prev = registry.register(s2);

        assert!(prev.is_some());
        assert_eq!(prev.unwrap().description.as_deref(), Some("First"));
        assert_eq!(registry.get("dup").unwrap().description.as_deref(), Some("Second"));
        assert_eq!(registry.count(), 1);
    }

    #[test]
    fn test_registry_all_sorted() {
        let mut registry = SkillRegistry::new();
        registry.register(Skill::builtin("ccc", "C skill", "# C"));
        registry.register(Skill::builtin("aaa", "A skill", "# A"));
        registry.register(Skill::builtin("bbb", "B skill", "# B"));

        let all = registry.all();
        assert_eq!(all.len(), 3);
        assert_eq!(all[0].name, "aaa");
        assert_eq!(all[1].name, "bbb");
        assert_eq!(all[2].name, "ccc");
    }

    #[test]
    fn test_registry_names() {
        let mut registry = SkillRegistry::new();
        registry.register(Skill::builtin("zulu", "", ""));
        registry.register(Skill::builtin("alpha", "", ""));
        let names = registry.names();
        assert_eq!(names, vec!["alpha", "zulu"]);
    }

    #[test]
    fn test_registry_dirs_tracking() {
        let mut registry = SkillRegistry::new();
        let skill = Skill {
            name: "dir-skill".to_string(),
            description: None,
            location: "/tmp/.opencode/skill/dir-skill/SKILL.md".to_string(),
            content: "# Test".to_string(),
        };
        registry.register(skill);
        assert_eq!(registry.dirs().len(), 1);
        assert!(registry.dirs()[0].ends_with("dir-skill"));
    }

    #[test]
    fn test_registry_empty_all() {
        let registry = SkillRegistry::new();
        assert!(registry.all().is_empty());
        assert_eq!(registry.count(), 0);
        assert!(registry.names().is_empty());
    }

    // ── format_skill_list tests ────────────────────────────────────

    #[test]
    fn test_format_empty_skills() {
        let result = format_skill_list(&[], false);
        assert_eq!(result, "No skills are currently available.");
    }

    #[test]
    fn test_format_skills_without_descriptions() {
        let skill = Skill {
            name: "no-desc".to_string(),
            description: None,
            location: "/tmp/SKILL.md".to_string(),
            content: "# No desc".to_string(),
        };
        let result = format_skill_list(&[&skill], false);
        // Skills without description are filtered out
        assert_eq!(result, "No skills are currently available.");
    }

    #[test]
    fn test_format_verbose() {
        let skill = Skill {
            name: "my-skill".to_string(),
            description: Some("A great skill".to_string()),
            location: "/tmp/skills/SKILL.md".to_string(),
            content: "# Body".to_string(),
        };
        let result = format_skill_list(&[&skill], true);
        assert!(result.contains("<available_skills>"));
        assert!(result.contains("<skill>"));
        assert!(result.contains("<name>my-skill</name>"));
        assert!(result.contains("<description>A great skill</description>"));
        assert!(result.contains("<location>/tmp/skills/SKILL.md</location>"));
        assert!(result.contains("</available_skills>"));
    }

    #[test]
    fn test_format_non_verbose() {
        let skill = Skill {
            name: "my-skill".to_string(),
            description: Some("A great skill".to_string()),
            location: "/tmp/skills/SKILL.md".to_string(),
            content: "# Body".to_string(),
        };
        let result = format_skill_list(&[&skill], false);
        assert!(result.contains("## Available Skills"));
        assert!(result.contains("- **my-skill**: A great skill"));
    }

    // ── discover_skill_files tests ─────────────────────────────────

    #[test]
    fn test_discover_skill_files_empty_dir() {
        let tmp = std::env::temp_dir().join("rustcode-skill-test-empty");
        let _ = std::fs::create_dir_all(&tmp);
        let files = discover_skill_files(&tmp, &tmp, &tmp, &[], false);
        // Clean up
        let _ = std::fs::remove_dir_all(&tmp);
        // Should find nothing in an empty temp dir
        assert!(files.is_empty());
    }

    #[test]
    fn test_discover_skill_files_with_opencode_skill() {
        let tmp = std::env::temp_dir().join("rustcode-skill-test-opencode");
        let _ = std::fs::remove_dir_all(&tmp);
        let skill_dir = tmp.join(".opencode").join("skill").join("test-skill");
        std::fs::create_dir_all(&skill_dir).expect("create dirs");
        std::fs::write(
            skill_dir.join("SKILL.md"),
            "---\nname: test-skill\ndescription: Unit test skill\n---\n\n# Body\n",
        )
        .expect("write SKILL.md");

        let files = discover_skill_files(&tmp, &tmp, &tmp, &[], true);
        // Clean up
        let _ = std::fs::remove_dir_all(&tmp);

        assert!(!files.is_empty());
        assert!(files.iter().any(|f| f.ends_with("SKILL.md")));
    }

    #[test]
    fn test_discover_skill_files_with_opencode_skills_plural() {
        let tmp = std::env::temp_dir().join("rustcode-skill-test-skills");
        let _ = std::fs::remove_dir_all(&tmp);
        let skill_dir = tmp.join(".opencode").join("skills").join("pl-skill");
        std::fs::create_dir_all(&skill_dir).expect("create dirs");
        std::fs::write(
            skill_dir.join("SKILL.md"),
            "---\nname: pl-skill\n---\n\n# Body\n",
        )
        .expect("write SKILL.md");

        let files = discover_skill_files(&tmp, &tmp, &tmp, &[], true);
        let _ = std::fs::remove_dir_all(&tmp);
        assert!(!files.is_empty());
    }

    // ── discover_and_load integration tests ────────────────────────

    #[test]
    fn test_discover_and_load_with_valid_skill() {
        let tmp = std::env::temp_dir().join("rustcode-skill-integration");
        let _ = std::fs::remove_dir_all(&tmp);
        let skill_dir = tmp.join(".opencode").join("skill").join("integration-test");
        std::fs::create_dir_all(&skill_dir).expect("create dirs");
        std::fs::write(
            skill_dir.join("SKILL.md"),
            "---\nname: integration-test\ndescription: Integration test skill\n---\n\n# Integration\n\nTest body.\n",
        )
        .expect("write SKILL.md");

        let registry = discover_and_load(&tmp, &tmp, &tmp, &[], true);
        let _ = std::fs::remove_dir_all(&tmp);

        // Should contain the built-in + our test skill
        assert!(registry.count() >= 2);
        assert!(registry.contains("integration-test"));
        assert!(registry.contains("customize-opencode"));

        let skill = registry.get("integration-test").expect("should exist");
        assert_eq!(skill.description.as_deref(), Some("Integration test skill"));
        assert!(skill.content.contains("# Integration"));
    }

    #[test]
    fn test_discover_and_load_skips_invalid() {
        let tmp = std::env::temp_dir().join("rustcode-skill-invalid");
        let _ = std::fs::remove_dir_all(&tmp);
        // Create a skill with no frontmatter — should be skipped silently
        let skill_dir = tmp.join(".opencode").join("skill").join("no-fm");
        std::fs::create_dir_all(&skill_dir).expect("create dirs");
        std::fs::write(skill_dir.join("SKILL.md"), "# No frontmatter\n\nJust content.\n")
            .expect("write SKILL.md");

        let registry = discover_and_load(&tmp, &tmp, &tmp, &[], true);
        let _ = std::fs::remove_dir_all(&tmp);

        // Should still have the built-in skill, but not the invalid one
        assert!(registry.contains("customize-opencode"));
        assert!(!registry.contains("no-fm"));
    }

    #[test]
    fn test_discover_and_load_with_external_skills() {
        let tmp = std::env::temp_dir().join("rustcode-skill-external");
        let _ = std::fs::remove_dir_all(&tmp);
        // Create a .claude/skills/ skill
        let skill_dir = tmp.join(".claude").join("skills").join("claude-skill");
        std::fs::create_dir_all(&skill_dir).expect("create dirs");
        std::fs::write(
            skill_dir.join("SKILL.md"),
            "---\nname: claude-skill\ndescription: External .claude skill\n---\n\n# Claude\n",
        )
        .expect("write SKILL.md");

        let registry = discover_and_load(&tmp, &tmp, &tmp, &[], false);
        let _ = std::fs::remove_dir_all(&tmp);

        assert!(registry.contains("claude-skill"));
        assert!(registry.contains("customize-opencode"));
    }

    #[test]
    fn test_discover_and_load_external_disabled() {
        let tmp = std::env::temp_dir().join("rustcode-skill-ext-disabled");
        let _ = std::fs::remove_dir_all(&tmp);
        let skill_dir = tmp.join(".claude").join("skills").join("disabled-skill");
        std::fs::create_dir_all(&skill_dir).expect("create dirs");
        std::fs::write(
            skill_dir.join("SKILL.md"),
            "---\nname: disabled-skill\ndescription: Should not appear\n---\n\n# Disabled\n",
        )
        .expect("write SKILL.md");

        // With external disabled, .claude skills should not be found
        let registry = discover_and_load(&tmp, &tmp, &tmp, &[], true);
        let _ = std::fs::remove_dir_all(&tmp);

        assert!(!registry.contains("disabled-skill"));
        assert!(registry.contains("customize-opencode"));
    }
}
