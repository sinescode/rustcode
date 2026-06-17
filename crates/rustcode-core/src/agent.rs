//! Agent definitions — built-in agents, config merging, and agent generation.
//!
//! Ported from:
//! - `packages/opencode/src/agent/agent.ts` (459 lines)
//! - `packages/opencode/src/agent/subagent-permissions.ts` (27 lines)
//! - `packages/core/src/agent.ts` (143 lines)
//! - `packages/core/src/v1/config/agent.ts` (89 lines)
//! - `packages/core/src/config/agent.ts` (25 lines)
//!
//! OpenCode commit: 5d0f86606ac30690f79f0a6a9f41a1f49fe95d0b
//!
//! ## Architecture
//!
//! The agent system provides:
//!
//! - **Built-in agents**: build (default primary), plan (read-only mode),
//!   general (subagent for parallel work), explore (search-only subagent),
//!   and hidden agents (compaction, title, summary).
//! - **Config merging**: user config from `config.agent` overrides built-in
//!   defaults — fields like model, variant, prompt, temperature, steps, and
//!   permissions are merged per-agent.
//! - **Permission layering**: defaults → built-in overrides → user config →
//!   Truncate.GLOB always-allowed unless explicitly denied.
//! - **Subagent session permissions**: when a subagent is spawned via the
//!   task tool, its session permission is derived by combining the parent
//!   session's deny rules with subagent-specific defaults.

use crate::config::{self, AgentMode};
use crate::permission::{self, PermissionRuleset};
use crate::provider;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::{Path, PathBuf};

// ══════════════════════════════════════════════════════════════════════════════
// Prompt templates (embedded from TS .txt files)
// ══════════════════════════════════════════════════════════════════════════════

/// Prompt for the explore (search) agent.
///
/// # Source
/// Ported from `packages/opencode/src/agent/prompt/explore.txt`.
pub const PROMPT_EXPLORE: &str = "\
You are a file search specialist. You excel at thoroughly navigating and exploring codebases.\n\
\n\
Your strengths:\n\
- Rapidly finding files using glob patterns\n\
- Searching code and text with powerful regex patterns\n\
- Reading and analyzing file contents\n\
\n\
Guidelines:\n\
- Use Glob for broad file pattern matching\n\
- Use Grep for searching file contents with regex\n\
- Use Read when you know the specific file path you need to read\n\
- Use Bash for file operations like copying, moving, or listing directory contents\n\
- Adapt your search approach based on the thoroughness level specified by the caller\n\
- Return file paths as absolute paths in your final response\n\
- For clear communication, avoid using emojis\n\
- Do not create any files, or run bash commands that modify the user's system state in any way\n\
\n\
Complete the user's search request efficiently and report your findings clearly.";

/// Prompt for the compaction (context summarization) agent.
///
/// # Source
/// Ported from `packages/opencode/src/agent/prompt/compaction.txt`.
pub const PROMPT_COMPACTION: &str = "\
You are an anchored context summarization assistant for coding sessions.\n\
\n\
Summarize only the conversation history you are given. The newest turns may be kept verbatim outside your summary, so focus on the older context that still matters for continuing the work.\n\
\n\
If the prompt includes a <previous-summary> block, treat it as the current anchored summary. Update it with the new history by preserving still-true details, removing stale details, and merging in new facts.\n\
\n\
Always follow the exact output structure requested by the user prompt. Keep every section, preserve exact file paths and identifiers when known, and prefer terse bullets over paragraphs.\n\
\n\
Do not answer the conversation itself. Do not mention that you are summarizing, compacting, or merging context. Respond in the same language as the conversation.";

/// Prompt for the summary agent.
///
/// # Source
/// Ported from `packages/opencode/src/agent/prompt/summary.txt`.
pub const PROMPT_SUMMARY: &str = "\
Summarize what was done in this conversation. Write like a pull request description.\n\
\n\
Rules:\n\
- 2-3 sentences max\n\
- Describe the changes made, not the process\n\
- Do not mention running tests, builds, or other validation steps\n\
- Do not explain what the user asked for\n\
- Write in first person (I added..., I fixed...)\n\
- Never ask questions or add new questions\n\
- If the conversation ends with an unanswered question to the user, preserve that exact question\n\
- If the conversation ends with an imperative statement or request to the user (e.g. \"Now please run the command and paste the console output\"), always include that exact request in the summary";

/// Prompt for the title generation agent.
///
/// # Source
/// Ported from `packages/opencode/src/agent/prompt/title.txt`.
pub const PROMPT_TITLE: &str = "\
You are a title generator. You output ONLY a thread title. Nothing else.\n\
\n\
<task>\n\
Generate a brief title that would help the user find this conversation later.\n\
\n\
Follow all rules in <rules>\n\
Use the <examples> so you know what a good title looks like.\n\
Your output must be:\n\
- A single line\n\
- ≤50 characters\n\
- No explanations\n\
</task>\n\
\n\
<rules>\n\
- you MUST use the same language as the user message you are summarizing\n\
- Title must be grammatically correct and read naturally - no word salad\n\
- Never include tool names in the title (e.g. \"read tool\", \"bash tool\", \"edit tool\")\n\
- Focus on the main topic or question the user needs to retrieve\n\
- Vary your phrasing - avoid repetitive patterns like always starting with \"Analyzing\"\n\
- When a file is mentioned, focus on WHAT the user wants to do WITH the file, not just that they shared it\n\
- Keep exact: technical terms, numbers, filenames, HTTP codes\n\
- Remove: the, this, my, a, an\n\
- Never assume tech stack\n\
- Never use tools\n\
- NEVER respond to questions, just generate a title for the conversation\n\
- The title should NEVER include \"summarizing\" or \"generating\" when generating a title\n\
- DO NOT SAY YOU CANNOT GENERATE A TITLE OR COMPLAIN ABOUT THE INPUT\n\
- Always output something meaningful, even if the input is minimal.\n\
- If the user message is short or conversational (e.g. \"hello\", \"lol\", \"what's up\", \"hey\"):\n\
  → create a title that reflects the user's tone or intent (such as Greeting, Quick check-in, Light chat, Intro message, etc.)\n\
</rules>\n\
\n\
<examples>\n\
\"debug 500 errors in production\" → Debugging production 500 errors\n\
\"refactor user service\" → Refactoring user service\n\
\"why is app.js failing\" → app.js failure investigation\n\
\"implement rate limiting\" → Rate limiting implementation\n\
\"how do I connect postgres to my API\" → Postgres API connection\n\
\"best practices for React hooks\" → React hooks best practices\n\
\"@src/auth.ts can you add refresh token support\" → Auth refresh token support\n\
\"@utils/parser.ts this is broken\" → Parser bug fix\n\
\"look at @config.json\" → Config review\n\
\"@App.tsx add dark mode toggle\" → Dark mode toggle in App\n\
</examples>";

/// Prompt for the agent generation LLM call.
///
/// # Source
/// Ported from `packages/opencode/src/agent/generate.txt`.
pub const PROMPT_GENERATE: &str = "\
You are an elite AI agent architect specializing in crafting high-performance agent configurations. Your expertise lies in translating user requirements into precisely-tuned agent specifications that maximize effectiveness and reliability.\n\
\n\
**Important Context**: You may have access to project-specific instructions from CLAUDE.md files and other context that may include coding standards, project structure, and custom requirements. Consider this context when creating agents to ensure they align with the project's established patterns and practices.\n\
\n\
When a user describes what they want an agent to do, you will:\n\
\n\
1. **Extract Core Intent**: Identify the fundamental purpose, key responsibilities, and success criteria for the agent. Look for both explicit requirements and implicit needs. Consider any project-specific context from CLAUDE.md files. For agents that are meant to review code, you should assume that the user is asking to review recently written code and not the whole codebase, unless the user has explicitly instructed you otherwise.\n\
\n\
2. **Design Expert Persona**: Create a compelling expert identity that embodies deep domain knowledge relevant to the task. The persona should inspire confidence and guide the agent's decision-making approach.\n\
\n\
3. **Architect Comprehensive Instructions**: Develop a system prompt that:\n\
   - Establishes clear behavioral boundaries and operational parameters\n\
   - Provides specific methodologies and best practices for task execution\n\
   - Anticipates edge cases and provides guidance for handling them\n\
   - Incorporates any specific requirements or preferences mentioned by the user\n\
   - Defines output format expectations when relevant\n\
   - Aligns with project-specific coding standards and patterns from CLAUDE.md\n\
\n\
4. **Optimize for Performance**: Include:\n\
   - Decision-making frameworks appropriate to the domain\n\
   - Quality control mechanisms and self-verification steps\n\
   - Efficient workflow patterns\n\
   - Clear escalation or fallback strategies\n\
\n\
5. **Create Identifier**: Design a concise, descriptive identifier that:\n\
   - Uses lowercase letters, numbers, and hyphens only\n\
   - Is typically 2-4 words joined by hyphens\n\
   - Clearly indicates the agent's primary function\n\
   - Is memorable and easy to type\n\
   - Avoids generic terms like \"helper\" or \"assistant\"\n";

// ══════════════════════════════════════════════════════════════════════════════
// Agent info
// ══════════════════════════════════════════════════════════════════════════════

/// Complete agent information — maps to the TS `Agent.Info` type.
///
/// # Source
/// Ported from `packages/opencode/src/agent/agent.ts` lines 35–56 (`Info` Schema.Struct).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AgentInfo {
    /// Agent name (e.g., "build", "plan", "explore")
    pub name: String,

    /// Human-readable description shown in autocomplete
    #[serde(skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,

    /// Agent mode: subagent (spawned by task tool), primary (user-facing), all (both)
    pub mode: AgentMode,

    /// Whether this is a built-in native agent (true) or user-defined (false)
    #[serde(default)]
    pub native: bool,

    /// Whether this agent is hidden from the autocomplete menu
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub hidden: bool,

    /// Top-p sampling parameter
    #[serde(skip_serializing_if = "Option::is_none")]
    pub top_p: Option<f64>,

    /// Temperature parameter
    #[serde(skip_serializing_if = "Option::is_none")]
    pub temperature: Option<f64>,

    /// Display color (hex or theme name)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub color: Option<String>,

    /// Permission ruleset for this agent
    #[serde(default)]
    pub permission: PermissionRuleset,

    /// Override model (providerID + modelID)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub model: Option<provider::ModelRef>,

    /// Default model variant when using this agent
    #[serde(skip_serializing_if = "Option::is_none")]
    pub variant: Option<String>,

    /// System prompt override (appended to the base system prompt)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub prompt: Option<String>,

    /// Additional provider/model-specific options
    #[serde(default)]
    pub options: HashMap<String, serde_json::Value>,

    /// Maximum agentic iterations before forcing text-only
    #[serde(skip_serializing_if = "Option::is_none")]
    pub steps: Option<u32>,
}

impl AgentInfo {
    /// Create a minimal agent info with defaults.
    pub fn new(name: impl Into<String>, mode: AgentMode) -> Self {
        Self {
            name: name.into(),
            description: None,
            mode,
            native: false,
            hidden: false,
            top_p: None,
            temperature: None,
            color: None,
            permission: PermissionRuleset::new(),
            model: None,
            variant: None,
            prompt: None,
            options: HashMap::new(),
            steps: None,
        }
    }
}

/// Output of the agent generation LLM call.
///
/// # Source
/// Ported from `packages/opencode/src/agent/agent.ts` lines 58–62 (`GeneratedAgent` Schema.Struct).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GeneratedAgent {
    /// Unique agent identifier (kebab-case)
    pub identifier: String,
    /// When to use this agent, including examples
    #[serde(rename = "whenToUse")]
    pub when_to_use: String,
    /// The complete system prompt for the agent
    #[serde(rename = "systemPrompt")]
    pub system_prompt: String,
}

// ══════════════════════════════════════════════════════════════════════════════
// Subagent session permission derivation
// ══════════════════════════════════════════════════════════════════════════════

/// Derive the permission ruleset for a subagent session when spawned via `task`.
///
/// Combines:
/// 1. The parent session's deny rules and external_directory rules.
///    Parent agent restrictions only govern that agent; the subagent's own
///    permissions determine its capabilities.
/// 2. Default `todowrite` and `task` denies if the subagent's own ruleset
///    doesn't already permit them.
///
/// # Source
/// Ported from `packages/opencode/src/agent/subagent-permissions.ts` lines 14–27.
#[must_use]
pub fn derive_subagent_session_permission(
    parent_session_permission: &PermissionRuleset,
    subagent: &AgentInfo,
) -> PermissionRuleset {
    let can_task = subagent.permission.iter().any(|rule| rule.permission == "task");
    let can_todo = subagent
        .permission
        .iter()
        .any(|rule| rule.permission == "todowrite");

    let mut rules = PermissionRuleset::new();

    // Carry forward parent deny rules and external_directory rules
    for rule in parent_session_permission {
        if rule.permission == "external_directory" || rule.action == permission::PermissionAction::Deny {
            rules.push(rule.clone());
        }
    }

    // Add default denies for task/todowrite unless the subagent already allows them
    if !can_todo {
        rules.push(permission::PermissionRule {
            permission: "todowrite".into(),
            pattern: "*".into(),
            action: permission::PermissionAction::Deny,
        });
    }
    if !can_task {
        rules.push(permission::PermissionRule {
            permission: "task".into(),
            pattern: "*".into(),
            action: permission::PermissionAction::Deny,
        });
    }

    rules
}

// ══════════════════════════════════════════════════════════════════════════════
// Agent service
// ══════════════════════════════════════════════════════════════════════════════

/// The agent service manages agent definitions, resolves defaults, and
/// merges user config overrides into built-in agent definitions.
///
/// # Source
/// Ported from `packages/opencode/src/agent/agent.ts` lines 84–437 (Service + layer).
#[derive(Debug)]
pub struct AgentService {
    /// All agents keyed by name
    agents: HashMap<String, AgentInfo>,
    /// Worktree root for computing relative paths
    worktree: PathBuf,
    /// Global data directory (for truncation glob paths)
    data_dir: PathBuf,
    /// Skill directories for whitelisting
    skill_dirs: Vec<PathBuf>,
    /// Whether to auto-allow Truncate.GLOB (set false for tests)
    ensure_truncate_glob: bool,
}

impl AgentService {
    /// Build a new agent service from config, worktree, and directory state.
    ///
    /// This is the main constructor — it creates all built-in agents, merges
    /// user config overrides, computes skill dirs for permission whitelisting,
    /// and ensures the truncation directory is always allowed.
    ///
    /// # Source
    /// Ported from `packages/opencode/src/agent/agent.ts` lines 88–308
    /// (the `InstanceState.make<State>` closure).
    pub fn new(
        cfg: &config::Info,
        worktree: PathBuf,
        data_dir: PathBuf,
        tmp_dir: PathBuf,
        skill_dirs: Vec<PathBuf>,
    ) -> Self {
        // Build permission defaults (same as TS agent.ts lines 117–133)
        let defaults = build_default_permissions(&tmp_dir, &data_dir, &skill_dirs);
        let user = permission::rules_from_config(&cfg.permission.clone().unwrap_or_default());

        let mut agents = build_builtin_agents(&defaults, &user, &worktree, &data_dir);

        // Merge user config overrides (TS lines 265–292)
        for (key, value) in &cfg.agent {
            if value.disable.unwrap_or(false) {
                agents.remove(key);
                continue;
            }
            let item = agents.entry(key.clone()).or_insert_with(|| {
                AgentInfo::new(key, AgentMode::All)
            });
            apply_agent_config_override(item, value);
            // Re-merge permissions after config override
            item.permission = permission::merge_rulesets(&[
                item.permission.clone(),
                permission::rules_from_config(&value.permission.clone().unwrap_or_default()),
            ]);
        }

        // Handle deprecated `mode` field (TS merges mode → agent internally)
        for (key, value) in &cfg.mode {
            let item = agents.entry(key.clone()).or_insert_with(|| {
                AgentInfo::new(key, AgentMode::All)
            });
            apply_agent_config_override(item, value);
            item.permission = permission::merge_rulesets(&[
                item.permission.clone(),
                permission::rules_from_config(&value.permission.clone().unwrap_or_default()),
            ]);
        }

        // Ensure Truncate.GLOB is always allowed unless explicitly denied (TS lines 295–308)
        // The truncation GLOB is `<data_dir>/tool-output/*`
        let truncate_glob = format!("{}/*", data_dir.join("tool-output").display());
        for agent in agents.values_mut() {
            let explicitly_denied = agent.permission.iter().any(|r| {
                r.permission == "external_directory"
                    && r.action == permission::PermissionAction::Deny
                    && r.pattern == truncate_glob
            });
            if !explicitly_denied {
                let truncate_allow = vec![permission::PermissionRule {
                    permission: "external_directory".into(),
                    pattern: truncate_glob.clone(),
                    action: permission::PermissionAction::Allow,
                }];
                agent.permission = permission::merge_rulesets(&[
                    agent.permission.clone(),
                    truncate_allow,
                ]);
            }
        }

        Self {
            agents,
            worktree,
            data_dir,
            skill_dirs,
            ensure_truncate_glob: true,
        }
    }

    /// Create an agent service without built-in defaults — useful for testing.
    pub fn empty(worktree: PathBuf, data_dir: PathBuf) -> Self {
        Self {
            agents: HashMap::new(),
            worktree,
            data_dir,
            skill_dirs: Vec::new(),
            ensure_truncate_glob: false,
        }
    }

    /// Register or replace an agent directly.
    pub fn register(&mut self, info: AgentInfo) {
        self.agents.insert(info.name.clone(), info);
    }

    /// Get an agent by name.
    ///
    /// # Source
    /// Ported from `packages/opencode/src/agent/agent.ts` line 310–312 (`get`).
    #[must_use]
    pub fn get(&self, agent: &str) -> Option<&AgentInfo> {
        self.agents.get(agent)
    }

    /// List all agents, sorted: default agent first, then alphabetically.
    ///
    /// # Source
    /// Ported from `packages/opencode/src/agent/agent.ts` lines 314–323 (`list`).
    #[must_use]
    pub fn list(&self, default_agent: Option<&str>) -> Vec<&AgentInfo> {
        let mut list: Vec<&AgentInfo> = self.agents.values().collect();
        list.sort_by(|a, b| {
            let a_default = default_agent.is_some_and(|d| a.name == d);
            let b_default = default_agent.is_some_and(|d| b.name == d);
            match (a_default, b_default) {
                (true, false) => std::cmp::Ordering::Less,
                (false, true) => std::cmp::Ordering::Greater,
                _ => a.name.cmp(&b.name),
            }
        });
        list
    }

    /// Return the default agent's info.
    ///
    /// Resolves: `config.default_agent` → first primary + visible → error.
    ///
    /// # Source
    /// Ported from `packages/opencode/src/agent/agent.ts` lines 326–338 (`defaultInfo`).
    #[must_use]
    pub fn default_info(&self, configured_default: Option<&str>) -> Option<&AgentInfo> {
        if let Some(name) = configured_default {
            if let Some(agent) = self.agents.get(name) {
                if !matches!(agent.mode, AgentMode::Subagent) && !agent.hidden {
                    return Some(agent);
                }
            }
        }
        // Fallback: find the first primary + non-hidden agent
        let build = self.agents.get("build");
        if let Some(agent) = build {
            if !agent.hidden && !matches!(agent.mode, AgentMode::Subagent) {
                return Some(agent);
            }
        }
        self.agents.values().find(|a| !a.hidden && !matches!(a.mode, AgentMode::Subagent))
    }

    /// Return the default agent's name.
    ///
    /// # Source
    /// Ported from `packages/opencode/src/agent/agent.ts` lines 340–342 (`defaultAgent`).
    #[must_use]
    pub fn default_agent_name(&self, configured_default: Option<&str>) -> Option<String> {
        self.default_info(configured_default).map(|a| a.name.clone())
    }

    /// Generate a new agent definition via LLM.
    ///
    /// This sends a prompt to the configured LLM asking it to design an agent
    /// based on a user's natural-language description. The LLM returns a JSON
    /// object with `identifier`, `whenToUse`, and `systemPrompt`.
    ///
    /// # Source
    /// Ported from `packages/opencode/src/agent/agent.ts` lines 366–435 (`generate`).
    ///
    /// # Note
    /// This is a stub — the actual LLM call requires the full provider pipeline
    /// (auth, model resolution, streaming). The stub returns a basic generated
    /// agent for now. The full implementation will be wired when the provider
    /// catalog is complete.
    #[allow(unused_variables)]
    pub async fn generate(
        &self,
        description: &str,
        model: Option<&provider::ModelRef>,
        _existing_names: &[String],
    ) -> Result<GeneratedAgent, crate::error::Error> {
        // TODO: Full implementation requires:
        // 1. Resolve model (default or provided)
        // 2. Get auth for provider
        // 3. Build system prompt with PROMPT_GENERATE
        // 4. Send generateObject request with GeneratedAgent schema
        // 5. Handle OpenAI OAuth special case
        // 6. Return the parsed GeneratedAgent

        // For now, return a placeholder — the caller should use the
        // provider pipeline directly until this is wired.
        Err(crate::error::Error::NotImplemented(
            "Agent.generate requires full provider pipeline — use provider directly".into(),
        ))
    }

    // -- Accessors ---------------------------------------------------------

    /// The worktree root path.
    #[must_use]
    pub fn worktree(&self) -> &Path {
        &self.worktree
    }

    /// The global data directory.
    #[must_use]
    pub fn data_dir(&self) -> &Path {
        &self.data_dir
    }

    /// Skill directories for permission whitelisting.
    #[must_use]
    pub fn skill_dirs(&self) -> &[PathBuf] {
        &self.skill_dirs
    }
}

// ══════════════════════════════════════════════════════════════════════════════
// Internal helpers
// ══════════════════════════════════════════════════════════════════════════════

/// Build the default permission ruleset used as the base for all agents.
///
/// # Source
/// Ported from `packages/opencode/src/agent/agent.ts` lines 117–134.
fn build_default_permissions(
    tmp_dir: &Path,
    data_dir: &Path,
    skill_dirs: &[PathBuf],
) -> PermissionRuleset {
    let mut whitelisted_dirs: Vec<String> = vec![
        format!("{}/*", data_dir.join("tool-output").display()),
        format!("{}/*", tmp_dir.display()),
    ];
    // Add skill dirs
    for dir in skill_dirs {
        whitelisted_dirs.push(format!("{}/*", dir.display()));
    }

    // Build external_directory whitelist used by all agents
    let mut ext_dir_map: HashMap<String, config::PermissionAction> = HashMap::new();
    ext_dir_map.insert("*".into(), config::PermissionAction::Ask);
    for dir in &whitelisted_dirs {
        ext_dir_map.insert(dir.clone(), config::PermissionAction::Allow);
    }

    let defaults_config = config::PermissionConfig {
        wildcard: Some(config::PermissionAction::Allow),
        doom_loop: Some(config::PermissionAction::Ask),
        external_directory: Some(config::PermissionRule::Object(ext_dir_map)),
        question: Some(config::PermissionAction::Deny),
        // mirrors github.com/github/gitignore Node.gitignore pattern for .env files
        read: Some(config::PermissionRule::Object({
            let mut m = HashMap::new();
            m.insert("*".into(), config::PermissionAction::Allow);
            m.insert("*.env".into(), config::PermissionAction::Ask);
            m.insert("*.env.*".into(), config::PermissionAction::Ask);
            m.insert("*.env.example".into(), config::PermissionAction::Allow);
            m
        })),
        // plan_enter / plan_exit go in extra since they're custom permissions
        extra: {
            let mut m = HashMap::new();
            m.insert(
                "plan_enter".into(),
                config::PermissionRule::Action(config::PermissionAction::Deny),
            );
            m.insert(
                "plan_exit".into(),
                config::PermissionRule::Action(config::PermissionAction::Deny),
            );
            m
        },
        ..Default::default()
    };

    permission::rules_from_config(&defaults_config)
}

/// Build all built-in native agents.
///
/// # Source
/// Ported from `packages/opencode/src/agent/agent.ts` lines 138–263.
fn build_builtin_agents(
    defaults: &PermissionRuleset,
    user_rules: &PermissionRuleset,
    worktree: &Path,
    data_dir: &Path,
) -> HashMap<String, AgentInfo> {
    let plans_glob = format!("{}/*", data_dir.join("plans").display());
    let dot_opencode_plans = ".opencode/plans/*.md".to_string();
    // Compute relative plans path for worktree-local plan files
    let relative_plans = data_dir
        .join("plans")
        .strip_prefix(worktree)
        .map(|p| format!("{}/*.md", p.display()))
        .unwrap_or_else(|_| plans_glob.clone());

    // build — default primary agent
    let build_permission = permission::merge_rulesets(&[
        defaults.clone(),
        permission::rules_from_config(&config::PermissionConfig {
            question: Some(config::PermissionAction::Allow),
            extra: {
                let mut m = HashMap::new();
                m.insert(
                    "plan_enter".into(),
                    config::PermissionRule::Action(config::PermissionAction::Allow),
                );
                m
            },
            ..Default::default()
        }),
        user_rules.clone(),
    ]);

    // plan — read-only planning agent
    let mut plan_ext_dir: HashMap<String, config::PermissionAction> = HashMap::new();
    plan_ext_dir.insert(plans_glob.clone(), config::PermissionAction::Allow);
    let mut plan_edit: HashMap<String, config::PermissionAction> = HashMap::new();
    plan_edit.insert("*".into(), config::PermissionAction::Deny);
    plan_edit.insert(dot_opencode_plans.clone(), config::PermissionAction::Allow);
    plan_edit.insert(relative_plans.clone(), config::PermissionAction::Allow);

    let plan_permission = permission::merge_rulesets(&[
        defaults.clone(),
        permission::rules_from_config(&config::PermissionConfig {
            question: Some(config::PermissionAction::Allow),
            task: Some(config::PermissionRule::Object({
                let mut m = HashMap::new();
                m.insert("general".into(), config::PermissionAction::Deny);
                m
            })),
            external_directory: Some(config::PermissionRule::Object(plan_ext_dir)),
            edit: Some(config::PermissionRule::Object(plan_edit)),
            extra: {
                let mut m = HashMap::new();
                m.insert(
                    "plan_exit".into(),
                    config::PermissionRule::Action(config::PermissionAction::Allow),
                );
                m
            },
            ..Default::default()
        }),
        user_rules.clone(),
    ]);

    // general — subagent for parallel work
    let general_permission = permission::merge_rulesets(&[
        defaults.clone(),
        permission::rules_from_config(&config::PermissionConfig {
            todowrite: Some(config::PermissionAction::Deny),
            ..Default::default()
        }),
        user_rules.clone(),
    ]);

    // explore — search-only subagent
    let explore_permission = permission::merge_rulesets(&[
        defaults.clone(),
        permission::rules_from_config(&config::PermissionConfig {
            wildcard: Some(config::PermissionAction::Deny),
            grep: Some(config::PermissionRule::Action(config::PermissionAction::Allow)),
            glob: Some(config::PermissionRule::Action(config::PermissionAction::Allow)),
            list: Some(config::PermissionRule::Action(config::PermissionAction::Allow)),
            bash: Some(config::PermissionRule::Action(config::PermissionAction::Allow)),
            webfetch: Some(config::PermissionAction::Allow),
            websearch: Some(config::PermissionAction::Allow),
            read: Some(config::PermissionRule::Action(config::PermissionAction::Allow)),
            // Copy the readonly external directory whitelist from defaults
            ..Default::default()
        }),
        user_rules.clone(),
    ]);

    // Hidden agents — no tools allowed
    let hidden_permission = permission::merge_rulesets(&[
        defaults.clone(),
        permission::rules_from_config(&config::PermissionConfig {
            wildcard: Some(config::PermissionAction::Deny),
            ..Default::default()
        }),
        user_rules.clone(),
    ]);

    let mut agents = HashMap::new();
    agents.insert(
        "build".into(),
        AgentInfo {
            name: "build".into(),
            description: Some("The default agent. Executes tools based on configured permissions.".into()),
            mode: AgentMode::Primary,
            native: true,
            hidden: false,
            permission: build_permission,
            options: HashMap::new(),
            ..Default::default()
        },
    );
    agents.insert(
        "plan".into(),
        AgentInfo {
            name: "plan".into(),
            description: Some("Plan mode. Disallows all edit tools.".into()),
            mode: AgentMode::Primary,
            native: true,
            hidden: false,
            permission: plan_permission,
            options: HashMap::new(),
            ..Default::default()
        },
    );
    agents.insert(
        "general".into(),
        AgentInfo {
            name: "general".into(),
            description: Some(
                "General-purpose agent for researching complex questions and executing multi-step tasks. Use this agent to execute multiple units of work in parallel."
                    .into(),
            ),
            mode: AgentMode::Subagent,
            native: true,
            hidden: false,
            permission: general_permission,
            options: HashMap::new(),
            ..Default::default()
        },
    );
    agents.insert(
        "explore".into(),
        AgentInfo {
            name: "explore".into(),
            description: Some(
                "Fast agent specialized for exploring codebases. Use this when you need to quickly find files by patterns (eg. \"src/components/**/*.tsx\"), search code for keywords (eg. \"API endpoints\"), or answer questions about the codebase (eg. \"how do API endpoints work?\"). When calling this agent, specify the desired thoroughness level: \"quick\" for basic searches, \"medium\" for moderate exploration, or \"very thorough\" for comprehensive analysis across multiple locations and naming conventions."
                    .into(),
            ),
            mode: AgentMode::Subagent,
            native: true,
            hidden: false,
            permission: explore_permission,
            prompt: Some(PROMPT_EXPLORE.to_string()),
            options: HashMap::new(),
            ..Default::default()
        },
    );
    agents.insert(
        "compaction".into(),
        AgentInfo {
            name: "compaction".into(),
            mode: AgentMode::Primary,
            native: true,
            hidden: true,
            permission: hidden_permission.clone(),
            prompt: Some(PROMPT_COMPACTION.to_string()),
            options: HashMap::new(),
            ..Default::default()
        },
    );
    agents.insert(
        "title".into(),
        AgentInfo {
            name: "title".into(),
            mode: AgentMode::Primary,
            native: true,
            hidden: true,
            temperature: Some(0.5),
            permission: hidden_permission.clone(),
            prompt: Some(PROMPT_TITLE.to_string()),
            options: HashMap::new(),
            ..Default::default()
        },
    );
    agents.insert(
        "summary".into(),
        AgentInfo {
            name: "summary".into(),
            mode: AgentMode::Primary,
            native: true,
            hidden: true,
            permission: hidden_permission,
            prompt: Some(PROMPT_SUMMARY.to_string()),
            options: HashMap::new(),
            ..Default::default()
        },
    );

    agents
}

/// Apply user-config overrides to an agent info.
///
/// # Source
/// Ported from `packages/opencode/src/agent/agent.ts` lines 279–291.
fn apply_agent_config_override(info: &mut AgentInfo, cfg: &config::AgentConfig) {
    if let Some(ref model) = cfg.model {
        info.model = Some(provider::parse_model(model));
    }
    if let Some(ref variant) = cfg.variant {
        info.variant = Some(variant.clone());
    }
    if let Some(ref prompt) = cfg.prompt {
        info.prompt = Some(prompt.clone());
    }
    if let Some(ref description) = cfg.description {
        info.description = Some(description.clone());
    }
    if let Some(temperature) = cfg.temperature {
        info.temperature = Some(temperature);
    }
    if let Some(top_p) = cfg.top_p {
        info.top_p = Some(top_p);
    }
    if let Some(ref mode) = cfg.mode {
        info.mode = mode.clone();
    }
    if let Some(ref color) = cfg.color {
        info.color = Some(color.clone());
    }
    if let Some(hidden) = cfg.hidden {
        info.hidden = hidden;
    }
    if let Some(ref name) = cfg.name {
        info.name = name.clone();
    }
    if let Some(steps) = cfg.steps.or(cfg.max_steps) {
        info.steps = Some(steps);
    }
    // Merge options via serde_json::Value recursive merge
    for (key, value) in &cfg.options {
        merge_json_value(info.options.entry(key.clone()).or_insert_with(|| value.clone()), value);
    }
}

/// Simple deep merge of JSON values (mutates `target`).
fn merge_json_value(target: &mut serde_json::Value, source: &serde_json::Value) {
    if let (serde_json::Value::Object(t), serde_json::Value::Object(s)) = (target, source) {
        for (key, value) in s {
            match t.get_mut(key) {
                Some(existing) => merge_json_value(existing, value),
                None => {
                    t.insert(key.clone(), value.clone());
                }
            }
        }
    } else {
        *target = source.clone();
    }
}

impl Default for AgentInfo {
    fn default() -> Self {
        Self {
            name: String::new(),
            description: None,
            mode: AgentMode::Primary,
            native: false,
            hidden: false,
            top_p: None,
            temperature: None,
            color: None,
            permission: PermissionRuleset::new(),
            model: None,
            variant: None,
            prompt: None,
            options: HashMap::new(),
            steps: None,
        }
    }
}

// ══════════════════════════════════════════════════════════════════════════════
// Tests
// ══════════════════════════════════════════════════════════════════════════════

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::{self, AgentMode};
    use crate::permission::{PermissionAction, PermissionRule};

    // Helper: create a minimal config with default agent settings
    fn empty_config() -> config::Info {
        config::Info::default()
    }

    fn test_dirs() -> (PathBuf, PathBuf, PathBuf) {
        let worktree = PathBuf::from("/tmp/test-worktree");
        let data_dir = PathBuf::from("/tmp/test-data");
        let tmp_dir = PathBuf::from("/tmp");
        (worktree, data_dir, tmp_dir)
    }

    fn make_service() -> AgentService {
        let cfg = empty_config();
        let (worktree, data_dir, tmp_dir) = test_dirs();
        AgentService::new(&cfg, worktree, data_dir, tmp_dir, Vec::new())
    }

    #[test]
    fn test_builtin_agents_exist() {
        let svc = make_service();
        assert!(svc.get("build").is_some());
        assert!(svc.get("plan").is_some());
        assert!(svc.get("general").is_some());
        assert!(svc.get("explore").is_some());
        assert!(svc.get("compaction").is_some());
        assert!(svc.get("title").is_some());
        assert!(svc.get("summary").is_some());
    }

    #[test]
    fn test_build_is_default_primary() {
        let svc = make_service();
        let build = svc.get("build").unwrap();
        assert_eq!(build.mode, AgentMode::Primary);
        assert!(!build.hidden);
        assert!(build.native);
    }

    #[test]
    fn test_general_is_subagent() {
        let svc = make_service();
        let general = svc.get("general").unwrap();
        assert_eq!(general.mode, AgentMode::Subagent);
        assert!(!general.hidden);
    }

    #[test]
    fn test_hidden_agents_are_hidden() {
        let svc = make_service();
        assert!(svc.get("compaction").unwrap().hidden);
        assert!(svc.get("title").unwrap().hidden);
        assert!(svc.get("summary").unwrap().hidden);
    }

    #[test]
    fn test_explore_has_prompt() {
        let svc = make_service();
        let explore = svc.get("explore").unwrap();
        assert!(explore.prompt.is_some());
        assert!(explore.prompt.as_ref().unwrap().contains("file search specialist"));
    }

    #[test]
    fn test_default_info_returns_build() {
        let svc = make_service();
        let info = svc.default_info(None);
        assert!(info.is_some());
        assert_eq!(info.unwrap().name, "build");
    }

    #[test]
    fn test_default_info_respects_configured_default() {
        let svc = make_service();
        let info = svc.default_info(Some("plan"));
        assert!(info.is_some());
        assert_eq!(info.unwrap().name, "plan");
    }

    #[test]
    fn test_default_info_skips_hidden() {
        let svc = make_service();
        // "title" is hidden, should fall back to "build"
        let info = svc.default_info(Some("title"));
        assert!(info.is_some());
        assert_eq!(info.unwrap().name, "build");
    }

    #[test]
    fn test_default_info_skips_subagent() {
        let svc = make_service();
        // "general" is a subagent, should fall back to "build"
        let info = svc.default_info(Some("general"));
        assert!(info.is_some());
        assert_eq!(info.unwrap().name, "build");
    }

    #[test]
    fn test_list_sorts_default_first() {
        let svc = make_service();
        let list = svc.list(Some("plan"));
        assert!(!list.is_empty());
        // "build" is the usual default, but we explicitly chose "plan"
        assert_eq!(list[0].name, "plan");
        // After plan, should be alphabetical
        let names: Vec<&str> = list.iter().map(|a| a.name.as_str()).collect();
        assert_eq!(names[0], "plan");
        // Verify the rest are alphabetical
        let mut rest = names[1..].to_vec();
        let mut sorted_rest = rest.clone();
        sorted_rest.sort();
        assert_eq!(rest, sorted_rest);
    }

    #[test]
    fn test_user_config_disables_agent() {
        let (worktree, data_dir, tmp_dir) = test_dirs();
        let mut cfg = empty_config();
        let mut disable_agent = config::AgentConfig::default();
        disable_agent.disable = Some(true);
        cfg.agent.insert("general".into(), disable_agent);

        let svc = AgentService::new(&cfg, worktree, data_dir, tmp_dir, Vec::new());
        assert!(svc.get("general").is_none(), "general should be disabled");
        assert!(svc.get("build").is_some(), "build should still exist");
    }

    #[test]
    fn test_user_config_adds_custom_agent() {
        let (worktree, data_dir, tmp_dir) = test_dirs();
        let mut cfg = empty_config();
        let mut custom = config::AgentConfig::default();
        custom.description = Some("Custom test agent".into());
        custom.mode = Some(AgentMode::Subagent);
        cfg.agent.insert("my-custom".into(), custom);

        let svc = AgentService::new(&cfg, worktree, data_dir, tmp_dir, Vec::new());
        let agent = svc.get("my-custom").unwrap();
        assert_eq!(agent.description.as_deref(), Some("Custom test agent"));
        assert_eq!(agent.mode, AgentMode::Subagent);
        assert!(!agent.native);
    }

    #[test]
    fn test_user_config_overrides_builtin() {
        let (worktree, data_dir, tmp_dir) = test_dirs();
        let mut cfg = empty_config();
        let mut build_cfg = config::AgentConfig::default();
        build_cfg.description = Some("Overridden description".into());
        build_cfg.temperature = Some(0.8);
        cfg.agent.insert("build".into(), build_cfg);

        let svc = AgentService::new(&cfg, worktree, data_dir, tmp_dir, Vec::new());
        let build = svc.get("build").unwrap();
        assert_eq!(build.description.as_deref(), Some("Overridden description"));
        assert_eq!(build.temperature, Some(0.8));
        assert!(build.native, "native flag should be preserved");
    }

    #[test]
    fn test_title_agent_has_temperature_05() {
        let svc = make_service();
        let title = svc.get("title").unwrap();
        assert_eq!(title.temperature, Some(0.5));
    }

    #[test]
    fn test_registered_agent_overwrites() {
        let (worktree, data_dir, _tmp_dir) = test_dirs();
        let mut svc = AgentService::empty(worktree, data_dir);
        svc.register(AgentInfo {
            name: "test".into(),
            mode: AgentMode::Primary,
            ..Default::default()
        });
        assert!(svc.get("test").is_some());

        // Overwrite
        svc.register(AgentInfo {
            name: "test".into(),
            description: Some("updated".into()),
            mode: AgentMode::Subagent,
            ..Default::default()
        });
        let agent = svc.get("test").unwrap();
        assert_eq!(agent.description.as_deref(), Some("updated"));
        assert_eq!(agent.mode, AgentMode::Subagent);
    }

    // ── derive_subagent_session_permission tests ─────────────────────────

    #[test]
    fn test_subagent_permission_derives_deny_for_task_todo() {
        let parent = PermissionRuleset::new();
        let subagent = AgentInfo {
            name: "test".into(),
            mode: AgentMode::Subagent,
            native: false,
            permission: PermissionRuleset::new(),
            ..Default::default()
        };

        let derived = derive_subagent_session_permission(&parent, &subagent);
        let has_todo_deny = derived.iter().any(|r| {
            r.permission == "todowrite"
                && r.pattern == "*"
                && r.action == PermissionAction::Deny
        });
        let has_task_deny = derived.iter().any(|r| {
            r.permission == "task" && r.pattern == "*" && r.action == PermissionAction::Deny
        });
        assert!(has_todo_deny, "should deny todowrite by default");
        assert!(has_task_deny, "should deny task by default");
    }

    #[test]
    fn test_subagent_permission_respects_existing_task_allow() {
        let parent = PermissionRuleset::new();
        let subagent = AgentInfo {
            name: "test".into(),
            mode: AgentMode::Subagent,
            native: false,
            permission: vec![PermissionRule {
                permission: "task".into(),
                pattern: "*".into(),
                action: PermissionAction::Allow,
            }],
            ..Default::default()
        };

        let derived = derive_subagent_session_permission(&parent, &subagent);
        let has_task_deny = derived.iter().any(|r| {
            r.permission == "task" && r.pattern == "*" && r.action == PermissionAction::Deny
        });
        assert!(!has_task_deny, "should not deny task when already allowed");
    }

    #[test]
    fn test_subagent_permission_carries_parent_deny_rules() {
        let parent = vec![PermissionRule {
            permission: "bash".into(),
            pattern: "/etc/**".into(),
            action: PermissionAction::Deny,
        }];
        let subagent = AgentInfo {
            name: "test".into(),
            mode: AgentMode::Subagent,
            native: false,
            permission: PermissionRuleset::new(),
            ..Default::default()
        };

        let derived = derive_subagent_session_permission(&parent, &subagent);
        let carries_deny = derived.iter().any(|r| {
            r.permission == "bash"
                && r.pattern == "/etc/**"
                && r.action == PermissionAction::Deny
        });
        assert!(carries_deny, "should carry parent deny rules");
    }

    #[test]
    fn test_generate_returns_not_implemented() {
        let (worktree, data_dir, _tmp_dir) = test_dirs();
        let svc = AgentService::empty(worktree, data_dir);
        let rt = tokio::runtime::Runtime::new().unwrap();
        let result = rt.block_on(svc.generate("test description", None, &[]));
        assert!(result.is_err());
        match result {
            Err(crate::error::Error::NotImplemented(_)) => {} // expected
            _ => panic!("expected NotImplemented error"),
        }
    }
}
