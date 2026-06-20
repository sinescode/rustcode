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

// ── Configuration ──────────────────────────────────────────────────────

/// Configuration for skill discovery.
///
/// # Source
/// Ported from `packages/core/src/v1/config/skills.ts`.
#[derive(Debug, Clone, Default)]
pub struct SkillDiscoveryConfig {
    /// Additional filesystem paths to scan for skills.
    pub paths: Vec<PathBuf>,
    /// Remote URLs to pull skills from (HTTP index.json).
    pub urls: Vec<String>,
    /// Disable external skills (.claude, .agents directories).
    pub disable_external: bool,
    /// Disable .claude-specific skills separately from other external skills.
    ///
    /// # Source
    /// Ported from `packages/opencode/src/skill/index.ts` `disableClaudeCodeSkills`.
    pub disable_claude_code_skills: bool,
}

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
    pub fn builtin(
        name: impl Into<String>,
        description: impl Into<String>,
        content: impl Into<String>,
    ) -> Self {
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
fn extract_frontmatter(
    content: &str,
    path: &str,
) -> Result<(SkillFrontmatter, String), ParseError> {
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

    let frontmatter: SkillFrontmatter =
        serde_yaml::from_str(yaml_str).map_err(|e| ParseError::InvalidYaml {
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

/// The `.claude` external directory name, for separate disable control.
const CLAUDE_DIR: &str = ".claude";

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
/// Searches `~/.claude/skills/` and `~/.agents/skills/` (or subset if
/// `disable_claude_code_skills` is active).
pub fn glob_home_skills(home: &Path, dirs: &[&str]) -> Vec<PathBuf> {
    let mut matches = Vec::new();
    for ext_dir in dirs {
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
/// 4. Additional paths from `extra_paths` and config `skills.paths`
///
/// Returns deduplicated list of SKILL.md file paths.
///
/// # Source
/// Ported from `packages/opencode/src/skill/index.ts` `discoverSkills()`.
pub fn discover_skill_files(
    worktree: &Path,
    directory: &Path,
    home: &Path,
    extra_paths: &[PathBuf],
    config: &SkillDiscoveryConfig,
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
    if !config.disable_external {
        let external_dirs = if config.disable_claude_code_skills {
            &[".agents"][..]
        } else {
            EXTERNAL_DIRS
        };

        for path in glob_external_skills(directory, external_dirs) {
            if seen.insert(path.clone()) {
                matches.push(path);
            }
        }
        if directory != worktree {
            for path in glob_external_skills(worktree, external_dirs) {
                if seen.insert(path.clone()) {
                    matches.push(path);
                }
            }
        }

        // 3. Scan home directory
        for path in glob_home_skills(home, external_dirs) {
            if seen.insert(path.clone()) {
                matches.push(path);
            }
        }
    }

    // 4. Scan additional paths (from function params + config)
    let all_extra: Vec<&Path> = extra_paths
        .iter()
        .chain(config.paths.iter().map(|p| p.as_path()))
        .collect();

    for extra in &all_extra {
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

// ── Remote skill discovery ────────────────────────────────────────────

/// Concurrency limits for remote skill fetching.
const SKILL_CONCURRENCY: usize = 4;
const FILE_CONCURRENCY: usize = 8;

/// Check if a path segment is safe (no traversal, no null bytes).
///
/// # Source
/// Ported from `packages/core/src/skill/discovery.ts` `isSafeSegment()`.
fn is_safe_segment(value: &str) -> bool {
    !value.is_empty()
        && value != "."
        && value != ".."
        && !value.contains('/')
        && !value.contains('\\')
        && !value.contains('\0')
}

/// Check if a relative path is safe for use in skill file resolution.
///
/// # Source
/// Ported from `packages/core/src/skill/discovery.ts` `isSafeRelativePath()`.
fn is_safe_relative_path(value: &str) -> bool {
    if value.is_empty() || value.contains('\\') || value.contains('\0') || value.contains('?') || value.contains('#') {
        return false;
    }

    // Check it's not an absolute path (Unix or Windows)
    if value.starts_with('/') || value.starts_with('\\') {
        return false;
    }

    // Check it's not a URL
    if url::Url::parse(value).is_ok() {
        return false;
    }

    // Check each segment
    for segment in value.split('/') {
        // Try URL-decoding the segment
        let decoded = match urlencoding::decode(segment) {
            Ok(d) => d.into_owned(),
            Err(_) => return false,
        };

        if decoded.is_empty()
            || decoded == "."
            || decoded == ".."
            || decoded.contains('/')
            || decoded.contains('\\')
            || decoded.contains('\0')
        {
            return false;
        }
    }

    true
}

/// A skill entry from a remote index.json.
///
/// # Source
/// Ported from `packages/core/src/skill/discovery.ts` `IndexSkill`.
#[derive(Debug, Clone, serde::Deserialize)]
pub struct RemoteIndexSkill {
    /// Skill name.
    pub name: String,
    /// List of files to download for this skill.
    pub files: Vec<String>,
}

/// A remote skill index (index.json).
///
/// # Source
/// Ported from `packages/core/src/skill/discovery.ts` `Index`.
#[derive(Debug, Clone, serde::Deserialize)]
pub struct RemoteIndex {
    /// List of skills available at this URL.
    pub skills: Vec<RemoteIndexSkill>,
}

/// Errors that can occur during remote skill discovery.
#[derive(Debug, thiserror::Error)]
pub enum RemoteDiscoveryError {
    /// Failed to fetch the skill index.
    #[error("failed to fetch skill index from {url}: {source}")]
    FetchIndex {
        url: String,
        source: reqwest::Error,
    },

    /// The skill index has invalid JSON.
    #[error("invalid skill index from {url}: {source}")]
    InvalidIndex {
        url: String,
        source: serde_json::Error,
    },

    /// Failed to download a skill file.
    #[error("failed to download skill file from {url}: {source}")]
    DownloadFile {
        url: String,
        source: reqwest::Error,
    },

    /// Failed to write a skill file to disk.
    #[error("failed to write skill file to {path}: {source}")]
    WriteFile {
        path: String,
        source: std::io::Error,
    },

    /// Failed to create cache directory.
    #[error("failed to create cache directory {path}: {source}")]
    CreateCacheDir {
        path: String,
        source: std::io::Error,
    },
}

/// Pull skills from a remote URL and cache them locally.
///
/// Fetches `index.json` from the given URL, validates skills, downloads
/// files, and returns paths to cached skill directories containing
/// `SKILL.md` or `{name}.md` files.
///
/// # Source
/// Ported from `packages/core/src/skill/discovery.ts` `Service.pull()`.
pub async fn pull_remote_skills(
    url: &str,
    cache_dir: &Path,
) -> Result<Vec<PathBuf>, RemoteDiscoveryError> {
    let base = if url.ends_with('/') {
        url.to_string()
    } else {
        format!("{}/", url)
    };

    let index_url = format!("{}index.json", base);

    // Fetch the index.json
    let client = reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(30))
        .build()
        .map_err(|e| RemoteDiscoveryError::FetchIndex {
            url: index_url.clone(),
            source: e,
        })?;

    let response = client
        .get(&index_url)
        .send()
        .await
        .map_err(|e| RemoteDiscoveryError::FetchIndex {
            url: index_url.clone(),
            source: e,
        })?;

    let index: RemoteIndex = response
        .json()
        .await
        .map_err(|e| RemoteDiscoveryError::InvalidIndex {
            url: index_url.clone(),
            source: e,
        })?;

    // Create cache directory for this URL
    let url_hash = compute_url_hash(&base);
    let source_root = cache_dir.join("skills").join(&url_hash);

    if !source_root.exists() {
        std::fs::create_dir_all(&source_root).map_err(|e| RemoteDiscoveryError::CreateCacheDir {
            path: source_root.display().to_string(),
            source: e,
        })?;
    }

    let mut discovered_dirs = Vec::new();

    // Process skills with concurrency limit
    let skill_futures: Vec<_> = index
        .skills
        .iter()
        .filter(|skill| is_safe_segment(&skill.name))
        .filter(|skill| {
            skill.files.contains(&"SKILL.md".to_string())
                || skill
                    .files
                    .contains(&format!("{}.md", skill.name))
        })
        .map(|skill| {
            let skill_name = skill.name.clone();
            let skill_files = skill.files.clone();
            let source_root = source_root.clone();
            let base_url = base.clone();
            let client = client.clone();

            async move {
                let root = source_root.join(&skill_name);

                // Validate path doesn't escape source_root
                if !root.starts_with(&source_root) || root == source_root {
                    return Ok::<Vec<PathBuf>, RemoteDiscoveryError>(Vec::new());
                }

                // Create skill directory
                if !root.exists() {
                    std::fs::create_dir_all(&root).map_err(|e| {
                        RemoteDiscoveryError::WriteFile {
                            path: root.display().to_string(),
                            source: e,
                        }
                    })?;
                }

                let skill_url = format!(
                    "{}{}/",
                    base_url,
                    urlencoding::encode(&skill_name)
                );

                // Download all files with concurrency limit
                let file_futures: Vec<_> = skill_files
                    .iter()
                    .filter(|file| is_safe_relative_path(file))
                    .filter_map(|file| {
                        let resource_url = url::Url::parse(&format!("{}{}", skill_url, file))
                            .ok()?;

                        // Verify same origin
                        let base_origin = url::Url::parse(&base_url).ok()?;
                        if resource_url.origin() != base_origin.origin() {
                            return None;
                        }

                        let destination = root.join(file);

                        // Verify destination is within root
                        if !destination.starts_with(&root) || destination == root {
                            return None;
                        }

                        Some((resource_url.to_string(), destination))
                    })
                    .collect();

                // Download files (simplified - sequential for now)
                for (file_url, destination) in &file_futures {
                    if destination.exists() {
                        continue;
                    }

                    if let Some(parent) = destination.parent() {
                        if !parent.exists() {
                            std::fs::create_dir_all(parent).map_err(|e| {
                                RemoteDiscoveryError::WriteFile {
                                    path: parent.display().to_string(),
                                    source: e,
                                }
                            })?;
                        }
                    }

                    match client.get(file_url).send().await {
                        Ok(resp) => match resp.bytes().await {
                            Ok(bytes) => {
                                std::fs::write(destination, &bytes).map_err(|e| {
                                    RemoteDiscoveryError::WriteFile {
                                        path: destination.display().to_string(),
                                        source: e,
                                    }
                                })?;
                            }
                            Err(e) => {
                                tracing::warn!(
                                    "failed to download skill file {}: {}",
                                    file_url,
                                    e
                                );
                            }
                        },
                        Err(e) => {
                            tracing::warn!("failed to download skill file {}: {}", file_url, e);
                        }
                    }
                }

                // Check if SKILL.md or {name}.md exists
                let skill_md = root.join("SKILL.md");
                let name_md = root.join(format!("{}.md", skill_name));

                if skill_md.exists() || name_md.exists() {
                    Ok(vec![root])
                } else {
                    Ok(Vec::new())
                }
            }
        })
        .collect();

    // Execute skill fetches
    for future in skill_futures {
        match future.await {
            Ok(dirs) => discovered_dirs.extend(dirs),
            Err(e) => {
                tracing::warn!("failed to process remote skill: {}", e);
            }
        }
    }

    Ok(discovered_dirs)
}

/// Compute a hash for a URL to use as a cache directory name.
///
/// Uses SHA-256 and takes the first 16 hex characters.
fn compute_url_hash(url: &str) -> String {
    use sha2::{Digest, Sha256};
    let mut hasher = Sha256::new();
    hasher.update(url.as_bytes());
    let result = hasher.finalize();
    hex::encode(&result[..8])
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
            tracing::warn!("duplicate skill name detected, later skill overrides earlier one");
        }
        prev
    }

    /// Look up a skill by name.
    pub fn get(&self, name: &str) -> Option<&Skill> {
        self.skills.get(name)
    }

    /// Require a skill by name, returning an error if not found.
    pub fn require(&self, name: &str) -> Result<&Skill, SkillError> {
        self.skills.get(name).ok_or_else(|| SkillError::NotFound {
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

    /// Return skills available to an agent, filtered by permission rules.
    ///
    /// If `agent_permissions` is `None`, returns all skills with descriptions.
    /// If `Some(rulesets)`, evaluates `Permission::evaluate("skill", name, rulesets)`
    /// for each skill and includes only those with `Allow` action.
    ///
    /// # Source
    /// Ported from `packages/opencode/src/skill/index.ts` `available(agent?)`.
    pub fn available(
        &self,
        agent_permissions: Option<&[&crate::permission::PermissionRuleset]>,
    ) -> Vec<&Skill> {
        let mut skills: Vec<&Skill> = self
            .skills
            .values()
            .filter(|s| s.description.is_some())
            .collect();

        if let Some(rulesets) = agent_permissions {
            skills.retain(|s| {
                let eval = crate::permission::evaluate("skill", &s.name, rulesets);
                matches!(eval.action, crate::permission::PermissionAction::Allow)
            });
        }

        skills.sort_by(|a, b| a.name.cmp(&b.name));
        skills
    }
}

// ── Skill guidance system context ─────────────────────────────────────

/// Generate system context text that lists available skills for the LLM.
///
/// Produces XML-formatted `<available_skills>` block suitable for injection
/// into the agent's system prompt.
///
/// # Source
/// Ported from `packages/core/src/skill/guidance.ts` `SkillGuidance.Service`.
pub fn generate_skill_guidance(
    skills: &[&Skill],
    notify_changed: bool,
) -> String {
    if skills.is_empty() {
        return String::new();
    }

    let mut lines = Vec::new();

    if notify_changed {
        lines.push(
            "The available skills have changed since this session started.".to_string(),
        );
        lines.push(String::new());
    }

    lines.push(
        "You have access to the following skills. Use the skill tool to load a skill's full instructions when needed.".to_string(),
    );
    lines.push(String::new());
    lines.push("<available_skills>".to_string());

    for skill in skills {
        lines.push("  <skill>".to_string());
        lines.push(format!("    <name>{}</name>", skill.name));
        if let Some(ref desc) = skill.description {
            lines.push(format!("    <description>{}</description>", desc));
        }
        lines.push("  </skill>".to_string());
    }

    lines.push("</available_skills>".to_string());
    lines.join("\n")
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
/// # Source
/// Ported from `packages/opencode/src/skill/index.ts` `loadSkills()`.
pub fn discover_and_load(
    worktree: &Path,
    directory: &Path,
    home: &Path,
    extra_paths: &[PathBuf],
    config: &SkillDiscoveryConfig,
) -> SkillRegistry {
    let mut registry = SkillRegistry::new();

    // Register the built-in customize-opencode skill first so user skills can override it
    registry.register(Skill::builtin(
        "customize-opencode",
        "Use ONLY when the user is editing or creating opencode's own configuration: opencode.json, opencode.jsonc, files under .opencode/, or files under ~/.config/opencode/. Also use when creating or fixing opencode agents, subagents, skills, plugins, MCP servers, or permission rules. Do not use for the user's own application code, or for any project that is not configuring opencode itself.",
        include_str!("skill/customize-opencode.md"),
    ));

    let files = discover_skill_files(worktree, directory, home, extra_paths, config);

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

/// Discover and load all skills (local + remote) into a registry.
///
/// This async version adds remote skill discovery from `config.urls` on top
/// of the local filesystem discovery.
///
/// # Source
/// Extended from `discover_and_load` to include remote discovery.
pub async fn discover_and_load_async(
    worktree: &Path,
    directory: &Path,
    home: &Path,
    extra_paths: &[PathBuf],
    config: &SkillDiscoveryConfig,
) -> SkillRegistry {
    // Start with local discovery (sync)
    let mut registry = discover_skill_files_and_load(worktree, directory, home, extra_paths, config);

    // Add remote skills if URLs are configured
    if !config.urls.is_empty() {
        let cache_dir = home.join(".cache").join("opencode");
        for url in &config.urls {
            match pull_remote_skills(url, &cache_dir).await {
                Ok(dirs) => {
                    for dir in &dirs {
                        // Try SKILL.md first, then {name}.md
                        let skill_md = dir.join("SKILL.md");
                        let name = dir
                            .file_name()
                            .and_then(|n| n.to_str())
                            .unwrap_or("unknown");
                        let name_md = dir.join(format!("{}.md", name));

                        let file_path = if skill_md.exists() {
                            &skill_md
                        } else if name_md.exists() {
                            &name_md
                        } else {
                            continue;
                        };

                        match parse_skill_file(file_path) {
                            Ok(Some(skill)) => {
                                registry.register(skill);
                            }
                            Ok(None) => {}
                            Err(e) => {
                                tracing::warn!("failed to parse remote skill: {}", e);
                            }
                        }
                    }
                }
                Err(e) => {
                    tracing::warn!("failed to pull remote skills from {}: {}", url, e);
                }
            }
        }
    }

    registry
}

/// Internal helper: discover local skill files and load them into a registry.
fn discover_skill_files_and_load(
    worktree: &Path,
    directory: &Path,
    home: &Path,
    extra_paths: &[PathBuf],
    config: &SkillDiscoveryConfig,
) -> SkillRegistry {
    let mut registry = SkillRegistry::new();

    // Register the built-in customize-opencode skill first
    registry.register(Skill::builtin(
        "customize-opencode",
        "Use ONLY when the user is editing or creating opencode's own configuration: opencode.json, opencode.jsonc, files under .opencode/, or files under ~/.config/opencode/. Also use when creating or fixing opencode agents, subagents, skills, plugins, MCP servers, or permission rules. Do not use for the user's own application code, or for any project that is not configuring opencode itself.",
        include_str!("skill/customize-opencode.md"),
    ));

    let files = discover_skill_files(worktree, directory, home, extra_paths, config);

    let mut matched_count = 0;
    for file_path in &files {
        match parse_skill_file(file_path) {
            Ok(Some(skill)) => {
                registry.register(skill);
                matched_count += 1;
            }
            Ok(None) => {}
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
        let (fm, body) = extract_frontmatter(content, "test.md").expect("should parse frontmatter");
        assert_eq!(fm.name, "test-skill");
        assert_eq!(fm.description.as_deref(), Some("A skill for testing"));
        assert!(body.contains("# Body"));
        assert!(body.contains("Instructions here."));
    }

    #[test]
    fn test_extract_frontmatter_no_description() {
        let content = "---\nname: simple-skill\n---\n\n# Simple\n\nJust content.\n";
        let (fm, body) = extract_frontmatter(content, "test.md").expect("should parse frontmatter");
        assert_eq!(fm.name, "simple-skill");
        assert!(fm.description.is_none());
        assert!(body.contains("# Simple"));
    }

    #[test]
    fn test_extract_frontmatter_windows_newlines() {
        let content =
            "---\r\nname: win-skill\r\ndescription: Windows newlines\r\n---\r\n\r\n# Body\r\n";
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
        assert_eq!(
            registry.get("dup").unwrap().description.as_deref(),
            Some("Second")
        );
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
        let config = SkillDiscoveryConfig::default();
        let files = discover_skill_files(&tmp, &tmp, &tmp, &[], &config);
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

        let files = discover_skill_files(&tmp, &tmp, &tmp, &[], &SkillDiscoveryConfig::default());
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

        let files = discover_skill_files(&tmp, &tmp, &tmp, &[], &SkillDiscoveryConfig::default());
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

        let registry = discover_and_load(&tmp, &tmp, &tmp, &[], &SkillDiscoveryConfig { disable_external: true, ..Default::default() });
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
        std::fs::write(
            skill_dir.join("SKILL.md"),
            "# No frontmatter\n\nJust content.\n",
        )
        .expect("write SKILL.md");

        let registry = discover_and_load(&tmp, &tmp, &tmp, &[], &SkillDiscoveryConfig { disable_external: true, ..Default::default() });
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

        let registry = discover_and_load(&tmp, &tmp, &tmp, &[], &SkillDiscoveryConfig::default());
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
        let registry = discover_and_load(&tmp, &tmp, &tmp, &[], &SkillDiscoveryConfig { disable_external: true, ..Default::default() });
        let _ = std::fs::remove_dir_all(&tmp);

        assert!(!registry.contains("disabled-skill"));
        assert!(registry.contains("customize-opencode"));
    }

    // ── YAML frontmatter parsing edge cases ──────────────────────────

    #[test]
    fn test_extract_frontmatter_empty_block() {
        let content = "---\n---\n\nBody content here.\n";
        let err = extract_frontmatter(content, "test.md")
            .expect_err("empty frontmatter block should fail");
        assert!(
            matches!(err, ParseError::NoFrontmatter { .. }),
            "expected NoFrontmatter, got {:?}",
            err
        );
    }

    #[test]
    fn test_extract_frontmatter_null_name() {
        // YAML `name:` with no value is null — serde cannot deserialize null as String
        let content = "---\nname:\n---\n\nBody\n";
        let err = extract_frontmatter(content, "test.md")
            .expect_err("null name should fail to deserialize");
        assert!(
            matches!(err, ParseError::InvalidYaml { .. }),
            "expected InvalidYaml, got {:?}",
            err
        );
    }

    #[test]
    fn test_extract_frontmatter_extra_fields() {
        let content = "---\nname: my-skill\ndescription: A skill\nversion: 1.0\nauthor: test-user\ntags:\n  - rust\n  - skill\n---\n\n# Body\n\nExtra fields should be ignored.\n";
        let (fm, body) = extract_frontmatter(content, "test.md")
            .expect("extra fields should not prevent parsing");
        assert_eq!(fm.name, "my-skill");
        assert_eq!(fm.description.as_deref(), Some("A skill"));
        assert!(body.contains("# Body"));
        assert!(body.contains("Extra fields should be ignored."));
    }

    #[test]
    fn test_extract_frontmatter_triple_dash_in_body() {
        let content = "---\nname: dash-test\ndescription: Test triple dash in body\n---\n\n# Heading\n\nSome text with --- in the middle.\n\n---\n\nAnother section.\n";
        let (fm, body) = extract_frontmatter(content, "test.md")
            .expect("triple dash in body should not confuse parser");
        assert_eq!(fm.name, "dash-test");
        assert!(body.contains("---"));
        assert!(body.contains("# Heading"));
        assert!(body.contains("Another section."));
    }

    // ── discover_skill_files with nested directories ─────────────────

    #[test]
    fn test_discover_skill_files_deeply_nested() {
        let tmp = std::env::temp_dir().join("rustcode-skill-deep-nested");
        let _ = std::fs::remove_dir_all(&tmp);
        let skill_dir = tmp
            .join(".opencode")
            .join("skill")
            .join("a")
            .join("b")
            .join("c");
        std::fs::create_dir_all(&skill_dir).expect("create deeply nested dirs");
        std::fs::write(
            skill_dir.join("SKILL.md"),
            "---\nname: deep-nested\ndescription: Deeply nested skill\n---\n\n# Deep\n",
        )
        .expect("write SKILL.md");

        let files = discover_skill_files(&tmp, &tmp, &tmp, &[], &SkillDiscoveryConfig::default());
        let _ = std::fs::remove_dir_all(&tmp);

        assert!(!files.is_empty());
        assert!(files.iter().any(|f| f.ends_with("SKILL.md")));
    }

    #[test]
    fn test_discover_skill_files_extra_paths() {
        let tmp = std::env::temp_dir().join("rustcode-skill-extra-paths");
        let _ = std::fs::remove_dir_all(&tmp);
        let extra_dir = tmp.join("extra-skills");
        let skill_subdir = extra_dir.join("my-extra-skill");
        std::fs::create_dir_all(&skill_subdir).expect("create extra dirs");
        std::fs::write(
            skill_subdir.join("SKILL.md"),
            "---\nname: extra-skill\ndescription: From extra paths\n---\n\n# Extra\n",
        )
        .expect("write SKILL.md");

        let worktree = std::env::temp_dir().join("rustcode-skill-extra-wt");
        std::fs::create_dir_all(&worktree).expect("create worktree dir");

        let extra_paths = vec![extra_dir.clone()];
        let files = discover_skill_files(&worktree, &worktree, &worktree, &extra_paths, true);
        let _ = std::fs::remove_dir_all(&tmp);
        let _ = std::fs::remove_dir_all(&worktree);

        assert!(!files.is_empty());
        assert!(files.iter().any(|f| f.ends_with("SKILL.md")));
    }

    // ── glob_home_skills tests ───────────────────────────────────────

    #[test]
    fn test_glob_home_skills_empty_dir() {
        let tmp = std::env::temp_dir().join("rustcode-home-empty");
        let _ = std::fs::remove_dir_all(&tmp);
        std::fs::create_dir_all(&tmp).expect("create empty home dir");

        let results = glob_home_skills(&tmp, EXTERNAL_DIRS);
        let _ = std::fs::remove_dir_all(&tmp);

        assert!(results.is_empty());
    }

    #[test]
    fn test_glob_home_skills_nonexistent_dir() {
        let nonexistent = std::path::Path::new("/tmp/rustcode-definitely-does-not-exist-98765");
        let results = glob_home_skills(nonexistent, EXTERNAL_DIRS);
        assert!(results.is_empty());
    }

    // ── format_skill_list with various skill sets ────────────────────

    #[test]
    fn test_format_skill_list_multiple_non_verbose() {
        let s1 = Skill {
            name: "alpha-skill".to_string(),
            description: Some("First test skill".to_string()),
            location: "/tmp/s1/SKILL.md".to_string(),
            content: "# Alpha".to_string(),
        };
        let s2 = Skill {
            name: "beta-skill".to_string(),
            description: Some("Second test skill".to_string()),
            location: "/tmp/s2/SKILL.md".to_string(),
            content: "# Beta".to_string(),
        };
        let s3 = Skill {
            name: "gamma-skill".to_string(),
            description: Some("Third test skill".to_string()),
            location: "/tmp/s3/SKILL.md".to_string(),
            content: "# Gamma".to_string(),
        };

        let result = format_skill_list(&[&s1, &s2, &s3], false);

        assert!(result.contains("## Available Skills"));
        assert!(result.contains("- **alpha-skill**: First test skill"));
        assert!(result.contains("- **beta-skill**: Second test skill"));
        assert!(result.contains("- **gamma-skill**: Third test skill"));
    }

    #[test]
    fn test_format_skill_list_long_description() {
        let long_desc = "A".repeat(250);
        let skill = Skill {
            name: "verbose-skill".to_string(),
            description: Some(long_desc.clone()),
            location: "/tmp/long/SKILL.md".to_string(),
            content: "# Long".to_string(),
        };

        let result = format_skill_list(&[&skill], false);
        assert!(result.contains("## Available Skills"));
        assert!(result.contains("- **verbose-skill**: "));
        assert!(result.contains(&long_desc));
    }

    #[test]
    fn test_format_skill_list_verbose_multiple() {
        let s1 = Skill {
            name: "first-skill".to_string(),
            description: Some("First".to_string()),
            location: "/tmp/first/SKILL.md".to_string(),
            content: "# First".to_string(),
        };
        let s2 = Skill {
            name: "second-skill".to_string(),
            description: Some("Second".to_string()),
            location: "/tmp/second/SKILL.md".to_string(),
            content: "# Second".to_string(),
        };

        let result = format_skill_list(&[&s1, &s2], true);

        assert!(result.contains("<available_skills>"));
        assert!(result.contains("</available_skills>"));
        // Verify two <skill> elements
        assert_eq!(result.matches("<skill>").count(), 2);
        assert_eq!(result.matches("</skill>").count(), 2);
        assert!(result.contains("<name>first-skill</name>"));
        assert!(result.contains("<name>second-skill</name>"));
    }

    // ── SkillRegistry edge cases ─────────────────────────────────────

    #[test]
    fn test_registry_register_empty_content() {
        let mut registry = SkillRegistry::new();
        let skill = Skill {
            name: "empty-content".to_string(),
            description: Some("Has empty content".to_string()),
            location: "/tmp/empty/SKILL.md".to_string(),
            content: String::new(),
        };
        let prev = registry.register(skill);
        assert!(prev.is_none());

        let retrieved = registry
            .get("empty-content")
            .expect("skill should be registered");
        assert_eq!(retrieved.name, "empty-content");
        assert!(retrieved.content.is_empty());
    }

    #[test]
    fn test_registry_contains() {
        let mut registry = SkillRegistry::new();

        // Should return false for non-existent skill
        assert!(!registry.contains("no-such-skill"));

        // Register a skill and check contains returns true
        let skill = Skill::builtin("present-skill", "I exist", "# Present");
        registry.register(skill);

        assert!(registry.contains("present-skill"));
        assert!(!registry.contains("still-missing"));
    }

    #[test]
    fn test_registry_counts_after_multiple_registrations() {
        let mut registry = SkillRegistry::new();
        assert_eq!(registry.count(), 0);

        // Register first skill
        registry.register(Skill::builtin("skill-a", "First", "# A"));
        assert_eq!(registry.count(), 1);

        // Register second skill (different name)
        registry.register(Skill::builtin("skill-b", "Second", "# B"));
        assert_eq!(registry.count(), 2);

        // Register third skill
        registry.register(Skill::builtin("skill-c", "Third", "# C"));
        assert_eq!(registry.count(), 3);

        // Override skill-a (duplicate name) — count should stay at 3
        let prev = registry.register(Skill::builtin("skill-a", "First v2", "# A v2"));
        assert!(prev.is_some());
        assert_eq!(registry.count(), 3);

        // Verify the override took effect
        let skill_a = registry.get("skill-a").expect("skill-a should exist");
        assert_eq!(skill_a.description.as_deref(), Some("First v2"));
    }

    // ── ParseError Display ───────────────────────────────────────────

    #[test]
    fn test_parse_error_read_display() {
        let io_err = std::io::Error::new(std::io::ErrorKind::PermissionDenied, "permission denied");
        let err = ParseError::Read {
            path: "/home/user/.claude/skills/SKILL.md".to_string(),
            source: io_err,
        };
        let display = err.to_string();
        assert!(
            display.contains("/home/user/.claude/skills/SKILL.md"),
            "display should contain the path, got: {}",
            display
        );
        assert!(
            display.contains("permission denied"),
            "display should contain the source error message, got: {}",
            display
        );
    }

    // ── Remote discovery tests ────────────────────────────────────────

    #[test]
    fn test_is_safe_segment() {
        assert!(is_safe_segment("my-skill"));
        assert!(is_safe_segment("skill123"));
        assert!(is_safe_segment("a-b-c"));

        assert!(!is_safe_segment(""));
        assert!(!is_safe_segment("."));
        assert!(!is_safe_segment(".."));
        assert!(!is_safe_segment("skill/../../etc"));
        assert!(!is_safe_segment("skill\\..\\etc"));
        assert!(!is_safe_segment("skill\0null"));
    }

    #[test]
    fn test_is_safe_relative_path() {
        assert!(is_safe_relative_path("SKILL.md"));
        assert!(is_safe_relative_path("subdir/file.md"));
        assert!(is_safe_relative_path("a/b/c.md"));

        assert!(!is_safe_relative_path(""));
        assert!(!is_safe_relative_path("/absolute/path"));
        assert!(!is_safe_relative_path("\\windows\\path"));
        assert!(!is_safe_relative_path("file?query"));
        assert!(!is_safe_relative_path("file#anchor"));
        assert!(!is_safe_relative_path("http://example.com"));
        assert!(!is_safe_relative_path("../../../etc/passwd"));
        assert!(!is_safe_relative_path("file\0null"));
    }

    #[test]
    fn test_compute_url_hash() {
        let hash1 = compute_url_hash("https://example.com/skills/");
        let hash2 = compute_url_hash("https://example.com/skills/");
        assert_eq!(hash1, hash2);

        let hash3 = compute_url_hash("https://different.com/skills/");
        assert_ne!(hash1, hash3);

        // Hash should be 16 hex chars (8 bytes)
        assert_eq!(hash1.len(), 16);
    }

    #[test]
    fn test_remote_index_deserialize() {
        let json = r#"{
            "skills": [
                {
                    "name": "test-skill",
                    "files": ["SKILL.md", "helper.md"]
                },
                {
                    "name": "another-skill",
                    "files": ["SKILL.md"]
                }
            ]
        }"#;

        let index: RemoteIndex = serde_json::from_str(json).expect("should deserialize");
        assert_eq!(index.skills.len(), 2);
        assert_eq!(index.skills[0].name, "test-skill");
        assert_eq!(index.skills[0].files, vec!["SKILL.md", "helper.md"]);
        assert_eq!(index.skills[1].name, "another-skill");
    }
}
