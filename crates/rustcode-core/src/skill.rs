//! Skill discovery and management.
//!
//! Ported from: `packages/opencode/src/skill/*.ts`

/// Skill information.
#[derive(Debug, Clone)]
pub struct Skill {
    /// Skill name
    pub name: String,
    /// Skill description
    pub description: String,
    /// Skill file path
    pub path: std::path::PathBuf,
}

/// Discover skills from the filesystem.
pub fn discover(worktree: &std::path::Path) -> Vec<Skill> {
    let skills_dir = worktree.join(".opencode").join("skills");
    if !skills_dir.exists() {
        return Vec::new();
    }
    let mut skills = Vec::new();
    if let Ok(entries) = std::fs::read_dir(&skills_dir) {
        for entry in entries.flatten() {
            let path = entry.path();
            if path.extension().is_some_and(|e| e == "md") {
                let name = path
                    .file_stem()
                    .and_then(|s| s.to_str())
                    .unwrap_or("unknown")
                    .to_string();
                skills.push(Skill {
                    name,
                    description: String::new(),
                    path,
                });
            }
        }
    }
    skills
}
