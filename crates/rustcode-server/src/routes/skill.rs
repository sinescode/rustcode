//! Skill routes — discover and list skills from `.opencode/skills/`,
//! `~/.config/opencode/skills/`, and external skill directories.
//!
//! Ported from: `packages/opencode/src/server/routes/instance/httpapi/groups/skill.ts`

use axum::extract::State;
use axum::response::IntoResponse;
use axum::routing::get;
use axum::{Json, Router};
use std::path::PathBuf;
use std::sync::Arc;
use tracing::warn;

use crate::server::AppState;

/// Create the skill routes router.
///
/// # Source
/// Ported from `packages/opencode/src/server/routes/instance/httpapi/groups/skill.ts`
pub fn skill_routes(state: Arc<AppState>) -> Router {
    Router::new()
        .route("/skill", get(list_skills))
        .with_state(state)
}

/// List all discovered skills by scanning configured directories.
///
/// Searches:
/// 1. `./.opencode/skills/` (project-level)
/// 2. `./.opencode/skill/` (project-level, singular)
/// 3. `~/.config/opencode/skills/` (global)
/// 4. `./.claude/skills/` (external, walking up)
/// 5. `./.agents/skills/` (external, walking up)
///
/// Each skill file must be a Markdown file with YAML frontmatter containing
/// a `name` field and optional `description`.
///
/// # Source
/// Ported from `packages/opencode/src/server/routes/instance/httpapi/groups/skill.ts`
async fn list_skills(State(_state): State<Arc<AppState>>) -> impl IntoResponse {
    let home = std::env::var("HOME")
        .map(PathBuf::from)
        .or_else(|_| std::env::var("USERPROFILE").map(PathBuf::from))
        .unwrap_or_else(|_| PathBuf::from("/tmp"));
    let cwd = std::env::current_dir().unwrap_or_else(|_| PathBuf::from("."));

    let disable_external = std::env::var("OPENCODE_DISABLE_EXTERNAL_SKILLS").is_ok();
    let extra_paths: Vec<PathBuf> = Vec::new();
    let config = rustcode_core::skill::SkillDiscoveryConfig {
        disable_external,
        ..Default::default()
    };

    let files =
        rustcode_core::skill::discover_skill_files(&cwd, &cwd, &home, &extra_paths, &config);

    let mut skills: Vec<serde_json::Value> = Vec::new();
    for file_path in &files {
        match rustcode_core::skill::parse_skill_file(file_path) {
            Ok(Some(skill)) => {
                skills.push(serde_json::json!({
                    "name": skill.name,
                    "description": skill.description,
                    "source": skill.location,
                }));
            }
            Ok(None) => {
                // No frontmatter — skip silently
            }
            Err(e) => {
                warn!("Failed to parse skill file {}: {e}", file_path.display());
            }
        }
    }

    // Sort by name for stable output
    skills.sort_by(|a, b| {
        a["name"]
            .as_str()
            .unwrap_or("")
            .cmp(b["name"].as_str().unwrap_or(""))
    });

    Json(serde_json::to_value(skills).unwrap_or_default()).into_response()
}
