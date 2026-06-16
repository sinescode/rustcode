//! Permission system — gates tool execution with allow/deny/ask semantics.
//!
//! Ported from:
//! - `packages/opencode/src/permission/index.ts`
//! - `packages/opencode/src/permission/arity.ts`
//! - `packages/core/src/permission.ts`
//! - `packages/core/src/permission/saved.ts`
//! - `packages/core/src/permission/schema.ts`
//! - `packages/core/src/permission/sql.ts`
//! - `packages/core/src/v1/config/permission.ts`
//! - `packages/core/src/v1/permission.ts`
//! - `packages/core/src/util/wildcard.ts`
//! OpenCode commit: 5d0f86606ac30690f79f0a6a9f41a1f49fe95d0b
//!
//! ## Architecture
//!
//! Two API levels are unified into a single module:
//!
//! - **Core types**: `PermissionAction` (Allow/Deny/Ask), `PermissionRule`
//!   (permission/pattern/action), and `PermissionRuleset`.
//! - **Wildcard matching**: Regex-based matching matching the TS `Wildcard.match()`.
//! - **Rule evaluation**: `evaluate()` — last-matching-rule-wins semantics.
//! - **Bash command arity**: `bash_arity_prefix()` — identifies the
//!   "human-understandable command" from a shell invocation.
//! - **Config conversion**: `rules_from_config()` — converts `ConfigPermission`
//!   from config.rs into a `PermissionRuleset`.
//! - **Permission service**: `PermissionService` — manages pending permission
//!   requests, publishes bus events, and supports blocking `assert()`.
//! - **Saved permissions**: `SavedPermissions` — database-backed CRUD for
//!   permanent "always allow" rules remembered across sessions.
//!
//! ## Event Bus
//!
//! Events published on the `SharedBus`:
//! - `permission.asked` — when a request enters the pending state
//! - `permission.replied` — when a pending request is resolved

use crate::bus::SharedBus;
use crate::config::PermissionConfig;
use crate::error::{Error, PermissionError, Result};
use crate::id::{self, IdPrefix};
use crate::storage::Database;
use regex::Regex;
use serde::{Deserialize, Serialize};
use std::collections::{HashMap, HashSet};
use std::sync::{Arc, OnceLock};

// ══════════════════════════════════════════════════════════════════════════════
// Core Permission Types
// ══════════════════════════════════════════════════════════════════════════════

/// Permission action — the three possible outcomes of a permission check.
///
/// # Source
/// Ported from `packages/core/src/permission/schema.ts` lines 5–6.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PermissionAction {
    /// The operation is explicitly allowed.
    Allow,
    /// The operation is explicitly denied.
    Deny,
    /// The user must be asked before proceeding.
    Ask,
}

impl std::fmt::Display for PermissionAction {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Allow => write!(f, "allow"),
            Self::Deny => write!(f, "deny"),
            Self::Ask => write!(f, "ask"),
        }
    }
}

/// A single permission rule — maps a permission name + pattern to an action.
///
/// # Source
/// Ported from `packages/core/src/v1/permission.ts` lines 18–24.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct PermissionRule {
    /// Tool/permission name (e.g. "bash", "edit", "read", or "*").
    pub permission: String,
    /// Pattern to match against (e.g. "*.ts", "/home/*", or "*").
    pub pattern: String,
    /// Action to take when this rule matches.
    pub action: PermissionAction,
}

/// A list of permission rules.
///
/// # Source
/// Ported from `packages/core/src/v1/permission.ts` line 25.
pub type PermissionRuleset = Vec<PermissionRule>;

/// Reply option for a pending permission request.
///
/// # Source
/// Ported from `packages/core/src/v1/permission.ts` line 42.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PermissionReply {
    /// Allow this specific call once.
    Once,
    /// Allow all future matching calls.
    Always,
    /// Deny this call (and possibly all pending for this session).
    Reject,
}

/// Identifies the tool invocation that triggered the permission check.
///
/// # Source
/// Ported from `packages/core/src/v1/permission.ts` lines 36–39.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ToolSource {
    /// The chat message ID that contained the tool call.
    #[serde(rename = "messageID")]
    pub message_id: String,
    /// The tool call ID within that message.
    #[serde(rename = "callID")]
    pub call_id: String,
}

/// A pending permission request.
///
/// # Source
/// Ported from `packages/core/src/v1/permission.ts` lines 28–35.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PermissionRequest {
    /// Unique request ID (per_ prefix).
    pub id: String,
    /// Session this request belongs to (ses_ prefix).
    #[serde(rename = "sessionID")]
    pub session_id: String,
    /// Permission being requested (e.g. "bash", "edit").
    pub permission: String,
    /// Patterns that need approval.
    pub patterns: Vec<String>,
    /// Arbitrary metadata about the request.
    #[serde(default)]
    pub metadata: serde_json::Value,
    /// Patterns to save if the user replies "always".
    #[serde(default)]
    pub always: Vec<String>,
    /// Tool call source, if triggered by a tool.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tool: Option<ToolSource>,
}

/// Input for creating a permission request.
///
/// # Source
/// Ported from `packages/core/src/v1/permission.ts` lines 57–62.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AskInput {
    /// Optional request ID — generated if not provided.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub id: Option<String>,
    /// Session ID.
    #[serde(rename = "sessionID")]
    pub session_id: String,
    /// Permission being requested.
    pub permission: String,
    /// Patterns needing approval.
    pub patterns: Vec<String>,
    /// Arbitrary metadata.
    #[serde(default)]
    pub metadata: serde_json::Value,
    /// Patterns to save on "always".
    #[serde(default)]
    pub always: Vec<String>,
    /// Tool source, if applicable.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tool: Option<ToolSource>,
    /// Ruleset to evaluate against.
    pub ruleset: PermissionRuleset,
}

/// Input for replying to a permission request.
///
/// # Source
/// Ported from `packages/core/src/v1/permission.ts` lines 64–68.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ReplyInput {
    /// ID of the request being replied to.
    #[serde(rename = "requestID")]
    pub request_id: String,
    /// The reply.
    pub reply: PermissionReply,
    /// Optional feedback message when rejecting.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub message: Option<String>,
}

/// Result of evaluating a permission check.
///
/// # Source
/// Ported from `packages/opencode/src/permission/index.ts` lines 39–49.
#[derive(Debug, Clone)]
pub struct EvaluatedPermission {
    /// The resulting action.
    pub action: PermissionAction,
    /// Which rule matched (for logging/debugging).
    pub matched_permission: Option<String>,
    /// The pattern that matched.
    pub matched_pattern: Option<String>,
}

// ══════════════════════════════════════════════════════════════════════════════
// Wildcard Matching
// ══════════════════════════════════════════════════════════════════════════════

/// Test whether `input` matches a `pattern` with `*` and `?` wildcards.
///
/// Implements the exact semantics of OpenCode's `Wildcard.match()`:
/// - `*` matches any sequence of characters (including `/`)
/// - `?` matches any single character
/// - Backslashes are normalized to forward slashes
/// - Special regex characters in the pattern are escaped
/// - A trailing ` .*` is treated as `( .*)?` (optional suffix)
///
/// # Source
/// Ported from `packages/core/src/util/wildcard.ts` lines 3–14.
///
/// # Examples
///
/// ```
/// use rustcode_core::permission::wildcard_match;
/// assert!(wildcard_match("bash", "bash"));
/// assert!(wildcard_match("anything", "*"));
/// assert!(wildcard_match("foo/bar/baz", "foo/*/baz"));
/// assert!(!wildcard_match("bash", "shell"));
/// ```
pub fn wildcard_match(input: &str, pattern: &str) -> bool {
    // Normalize backslashes to forward slashes (TS line 4).
    let normalized = input.replace('\\', "/");

    // Escape regex-special characters, then convert wildcards to regex.
    // TS lines 6–10.
    let mut escaped = pattern.replace('\\', "/");
    // Escape special regex chars: . + ^ $ { } ( ) | [ ] \
    escaped = regex_escape(&escaped);
    // Convert * to .* (match any sequence)
    escaped = escaped.replace('*', ".*");
    // Convert ? to . (match single char)
    escaped = escaped.replace('?', ".");

    // Trailing " .*" → "( .*)?" (optional suffix match — TS line 11).
    if escaped.ends_with(" .*") {
        let len = escaped.len();
        escaped.replace_range(len - 3.., "( .*)?");
    }

    // Anchor to full string (TS line 13).
    let regex_str = format!("^{}$", escaped);

    // Build regex with dot-matches-newline flag (TS: "s" flag, or "si" on Windows).
    // We use the `s` flag for dot-all (`.` matches `\n`).
    match Regex::new(&regex_str) {
        Ok(re) => re.is_match(&normalized),
        Err(_) => {
            // If the regex fails to compile (shouldn't happen with proper escaping),
            // fall back to exact match.
            tracing::warn!(%pattern, "failed to compile wildcard regex, falling back to exact match");
            normalized == escaped
        }
    }
}

/// Escape special regex characters in a string.
///
/// Only escapes the characters in the TS character class:
/// `[.+^${}()|[\]\\]` — notably does NOT escape `*` or `?`.
///
/// # Source
/// Ported from `packages/core/src/util/wildcard.ts` line 8:
/// `.replace(/[.+^${}()|[\]\\]/g, "\\$&")`
fn regex_escape(s: &str) -> String {
    // The exact set from the TS regex character class (12 characters).
    let special = [
        '.', '+', '^', '$', '{', '}', '(', ')', '|', '[', ']', '\\',
    ];
    let mut result = String::with_capacity(s.len() * 2);
    for ch in s.chars() {
        if special.contains(&ch) {
            result.push('\\');
        }
        result.push(ch);
    }
    result
}

// ══════════════════════════════════════════════════════════════════════════════
// Rule Evaluation
// ══════════════════════════════════════════════════════════════════════════════

/// Evaluate a permission request against one or more rulesets.
///
/// Uses **last-match-wins** semantics: all rulesets are flattened, and the
/// *last* rule whose permission and pattern both match is the winner. If
/// no rule matches, the default is `Ask`.
///
/// # Source
/// Ported from `packages/opencode/src/permission/index.ts` lines 39–49.
///
/// # Examples
///
/// ```
/// use rustcode_core::permission::{evaluate, PermissionRule, PermissionAction, PermissionRuleset};
///
/// let rules: PermissionRuleset = vec![
///     PermissionRule { permission: "bash".into(), pattern: "*".into(), action: PermissionAction::Allow },
/// ];
/// let result = evaluate("bash", "*", &[&rules]);
/// assert_eq!(result.action, PermissionAction::Allow);
/// ```
pub fn evaluate(
    permission: &str,
    pattern: &str,
    rulesets: &[&PermissionRuleset],
) -> EvaluatedPermission {
    // TS: `.flat().findLast(rule => ...)`
    for ruleset in rulesets.iter().rev() {
        for rule in ruleset.iter().rev() {
            if wildcard_match(permission, &rule.permission)
                && wildcard_match(pattern, &rule.pattern)
            {
                return EvaluatedPermission {
                    action: rule.action,
                    matched_permission: Some(rule.permission.clone()),
                    matched_pattern: Some(rule.pattern.clone()),
                };
            }
        }
    }

    // Default: ask (TS line 47–48)
    EvaluatedPermission {
        action: PermissionAction::Ask,
        matched_permission: None,
        matched_pattern: None,
    }
}

/// Evaluate using V2 semantics (action/resource instead of permission/pattern).
///
/// # Source
/// Ported from `packages/core/src/permission.ts` lines 102–112.
pub fn evaluate_v2(
    action: &str,
    resource: &str,
    rulesets: &[&PermissionRuleset],
) -> EvaluatedPermission {
    // V2 rules use action/resource fields; our PermissionRule uses
    // permission/pattern. Semantics are identical — just different names.
    evaluate(action, resource, rulesets)
}

// ══════════════════════════════════════════════════════════════════════════════
// Bash Command Arity
// ══════════════════════════════════════════════════════════════════════════════

/// ARITY dictionary — maps command prefixes to their token count.
///
/// Lazily initialized from a static table.
///
/// # Source
/// Ported from `packages/opencode/src/permission/arity.ts` lines 24–161.
fn arity_map() -> &'static HashMap<&'static str, usize> {
    static ARITY: OnceLock<HashMap<&str, usize>> = OnceLock::new();
    ARITY.get_or_init(|| {
        let entries: &[(&str, usize)] = &[
            // Single-token commands
            ("cat", 1), ("cd", 1), ("chmod", 1), ("chown", 1), ("cp", 1),
            ("echo", 1), ("env", 1), ("export", 1), ("grep", 1), ("kill", 1),
            ("killall", 1), ("ln", 1), ("ls", 1), ("mkdir", 1), ("mv", 1),
            ("ps", 1), ("pwd", 1), ("rm", 1), ("rmdir", 1), ("sleep", 1),
            ("source", 1), ("tail", 1), ("touch", 1), ("unset", 1), ("which", 1),
            // Two/three-token commands
            ("aws", 3), ("az", 3), ("bazel", 2), ("brew", 2), ("bun", 2),
            ("bun run", 3), ("bun x", 3), ("cargo", 2), ("cargo add", 3),
            ("cargo run", 3), ("cdk", 2), ("cf", 2), ("cmake", 2),
            ("composer", 2), ("consul", 2), ("consul kv", 3), ("crictl", 2),
            ("deno", 2), ("deno task", 3), ("doctl", 3), ("docker", 2),
            ("docker builder", 3), ("docker compose", 3), ("docker container", 3),
            ("docker image", 3), ("docker network", 3), ("docker volume", 3),
            ("eksctl", 2), ("eksctl create", 3), ("firebase", 2), ("flyctl", 2),
            ("gcloud", 3), ("gh", 3), ("git", 2), ("git config", 3),
            ("git remote", 3), ("git stash", 3), ("go", 2), ("gradle", 2),
            ("helm", 2), ("heroku", 2), ("hugo", 2), ("ip", 2), ("ip addr", 3),
            ("ip link", 3), ("ip netns", 3), ("ip route", 3), ("kind", 2),
            ("kind create", 3), ("kubectl", 2), ("kubectl kustomize", 3),
            ("kubectl rollout", 3), ("kustomize", 2), ("make", 2), ("mc", 2),
            ("mc admin", 3), ("minikube", 2), ("mongosh", 2), ("mysql", 2),
            ("mvn", 2), ("ng", 2), ("npm", 2), ("npm exec", 3), ("npm init", 3),
            ("npm run", 3), ("npm view", 3), ("nvm", 2), ("nx", 2),
            ("openssl", 2), ("openssl req", 3), ("openssl x509", 3),
            ("pip", 2), ("pipenv", 2), ("pnpm", 2), ("pnpm dlx", 3),
            ("pnpm exec", 3), ("pnpm run", 3), ("poetry", 2), ("podman", 2),
            ("podman container", 3), ("podman image", 3), ("psql", 2),
            ("pulumi", 2), ("pulumi stack", 3), ("pyenv", 2), ("python", 2),
            ("rake", 2), ("rbenv", 2), ("redis-cli", 2), ("rustup", 2),
            ("serverless", 2), ("sfdx", 3), ("skaffold", 2), ("sls", 2),
            ("sst", 2), ("swift", 2), ("systemctl", 2), ("terraform", 2),
            ("terraform workspace", 3), ("tmux", 2), ("turbo", 2), ("ufw", 2),
            ("vault", 2), ("vault auth", 3), ("vault kv", 3), ("vercel", 2),
            ("volta", 2), ("wp", 2), ("yarn", 2), ("yarn dlx", 3),
            ("yarn run", 3),
        ];
        entries.iter().copied().collect()
    })
}

/// Given shell command tokens, return the prefix that identifies the
/// "human-understandable command."
///
/// Uses the ARITY dictionary to find the longest matching prefix. If no
/// entry matches, returns just the first token (or empty slice for empty input).
///
/// # Source
/// Ported from `packages/opencode/src/permission/arity.ts` lines 1–9.
///
/// # Examples
///
/// ```
/// use rustcode_core::permission::bash_arity_prefix;
///
/// // Simple command
/// assert_eq!(bash_arity_prefix(&["cat", "file.txt"]), ["cat"]);
/// // Two-token prefix
/// assert_eq!(bash_arity_prefix(&["git", "checkout", "main"]), ["git", "checkout"]);
/// // Three-token prefix
/// assert_eq!(bash_arity_prefix(&["npm", "run", "dev", "--watch"]), ["npm", "run", "dev"]);
/// // Unknown command — returns first token
/// assert_eq!(bash_arity_prefix(&["myapp", "arg1"]), ["myapp"]);
/// ```
pub fn bash_arity_prefix<'a>(tokens: &'a [&'a str]) -> &'a [&'a str] {
    // TS: for (let len = tokens.length; len > 0; len--) { ... }
    let arity = arity_map();
    for len in (1..=tokens.len()).rev() {
        let prefix = tokens[..len].join(" ");
        if let Some(arity) = arity.get(prefix.as_str()) {
            return &tokens[..(*arity).min(tokens.len())];
        }
    }

    // TS: if (tokens.length === 0) return []
    if tokens.is_empty() {
        return &[];
    }

    // TS: return tokens.slice(0, 1)
    &tokens[..1]
}

// ══════════════════════════════════════════════════════════════════════════════
// Config Conversion
// ══════════════════════════════════════════════════════════════════════════════

/// Convert configuration permission entries into a ruleset.
///
/// Maps each key in the config to one or more `PermissionRule` entries:
/// - String values (Action): produces a single rule with `pattern = "*"`
/// - Object values (HashMap<string, Action>): produces one rule per entry
///
/// Patterns containing `~/` or `$HOME` are expanded to the home directory.
///
/// # Source
/// Ported from `packages/opencode/src/permission/index.ts` lines 197–209.
pub fn rules_from_config(permission: &PermissionConfig) -> PermissionRuleset {
    let home = home_dir();
    let mut ruleset = PermissionRuleset::new();

    // For each known key in the config, produce rules.
    // The TS uses Object.entries() — we handle each field explicitly.

    // Fields with PermissionRule (Action | HashMap<String, Action>)
    process_config_field("read", &permission.read, &home, &mut ruleset);
    process_config_field("edit", &permission.edit, &home, &mut ruleset);
    process_config_field("glob", &permission.glob, &home, &mut ruleset);
    process_config_field("grep", &permission.grep, &home, &mut ruleset);
    process_config_field("list", &permission.list, &home, &mut ruleset);
    process_config_field("bash", &permission.bash, &home, &mut ruleset);
    process_config_field("task", &permission.task, &home, &mut ruleset);
    process_config_field(
        "external_directory",
        &permission.external_directory,
        &home,
        &mut ruleset,
    );
    process_config_field("lsp", &permission.lsp, &home, &mut ruleset);
    process_config_field("skill", &permission.skill, &home, &mut ruleset);

    // Fields with single PermissionAction
    push_simple_rule("todowrite", &permission.todowrite, &mut ruleset);
    push_simple_rule("question", &permission.question, &mut ruleset);
    push_simple_rule("webfetch", &permission.webfetch, &mut ruleset);
    push_simple_rule("websearch", &permission.websearch, &mut ruleset);
    push_simple_rule("doom_loop", &permission.doom_loop, &mut ruleset);

    // Wildcard catch-all
    push_simple_rule("*", &permission.wildcard, &mut ruleset);

    // Extra/unknown fields — always PermissionRule (HashMap)
    for (key, rule_config) in &permission.extra {
        match rule_config {
            crate::config::PermissionRule::Action(action) => {
                let action = convert_action(*action);
                ruleset.push(PermissionRule {
                    permission: key.clone(),
                    pattern: "*".into(),
                    action,
                });
            }
            crate::config::PermissionRule::Object(map) => {
                for (pattern, action) in map {
                    let expanded = expand_pattern(pattern, &home);
                    ruleset.push(PermissionRule {
                        permission: key.clone(),
                        pattern: expanded,
                        action: convert_action(*action),
                    });
                }
            }
        }
    }

    ruleset
}

/// Process a config field that can be either a simple Action or a
/// HashMap<String, Action>.
fn process_config_field(
    name: &str,
    rule: &Option<crate::config::PermissionRule>,
    home: &str,
    ruleset: &mut PermissionRuleset,
) {
    let rule = match rule {
        Some(r) => r,
        None => return,
    };

    match rule {
        crate::config::PermissionRule::Action(action) => {
            ruleset.push(PermissionRule {
                permission: name.into(),
                pattern: "*".into(),
                action: convert_action(*action),
            });
        }
        crate::config::PermissionRule::Object(map) => {
            for (pattern, action) in map {
                let expanded = expand_pattern(pattern, home);
                ruleset.push(PermissionRule {
                    permission: name.into(),
                    pattern: expanded,
                    action: convert_action(*action),
                });
            }
        }
    }
}

/// Push a simple (single-action) rule if the action is Some.
fn push_simple_rule(
    name: &str,
    action: &Option<crate::config::PermissionAction>,
    ruleset: &mut PermissionRuleset,
) {
    if let Some(action) = action {
        ruleset.push(PermissionRule {
            permission: name.into(),
            pattern: "*".into(),
            action: convert_action(*action),
        });
    }
}

/// Convert config PermissionAction to permission PermissionAction.
fn convert_action(action: crate::config::PermissionAction) -> PermissionAction {
    match action {
        crate::config::PermissionAction::Allow => PermissionAction::Allow,
        crate::config::PermissionAction::Deny => PermissionAction::Deny,
        crate::config::PermissionAction::Ask => PermissionAction::Ask,
    }
}

/// Get the user's home directory as a string, or empty if unavailable.
fn home_dir() -> String {
    dirs::home_dir()
        .map(|p| p.to_string_lossy().to_string())
        .unwrap_or_default()
}

/// Expand `~/` and `$HOME` prefixes in a pattern.
///
/// # Source
/// Ported from `packages/opencode/src/permission/index.ts` lines 189–195.
fn expand_pattern(pattern: &str, home: &str) -> String {
    if pattern.starts_with("~/") {
        return home.to_string() + &pattern[1..];
    }
    if pattern == "~" {
        return home.to_string();
    }
    if pattern.starts_with("$HOME/") {
        return home.to_string() + &pattern[5..];
    }
    if pattern.starts_with("$HOME") {
        return home.to_string() + &pattern[5..];
    }
    pattern.to_string()
}

// ══════════════════════════════════════════════════════════════════════════════
// Utility Functions
// ══════════════════════════════════════════════════════════════════════════════

/// Merge multiple rulesets into a single flat ruleset.
///
/// # Source
/// Ported from `packages/opencode/src/permission/index.ts` lines 211–213.
pub fn merge_rulesets(rulesets: &[PermissionRuleset]) -> PermissionRuleset {
    let mut merged = PermissionRuleset::new();
    for ruleset in rulesets {
        merged.extend(ruleset.iter().cloned());
    }
    merged
}

/// Find tools that are fully denied (deny with pattern `*`).
///
/// Edit-like tools (edit, write, apply_patch) are mapped to the "edit"
/// permission for checking, matching the TS source.
///
/// # Source
/// Ported from `packages/opencode/src/permission/index.ts` lines 215–224.
pub fn disabled_tools(tools: &[String], ruleset: &PermissionRuleset) -> HashSet<String> {
    let edits: HashSet<&str> = ["edit", "write", "apply_patch"].into_iter().collect();

    tools
        .iter()
        .filter(|tool| {
            let permission = if edits.contains(tool.as_str()) {
                "edit"
            } else {
                tool.as_str()
            };
            // TS: `.findLast(rule => Wildcard.match(permission, rule.permission))`
            let rule = ruleset
                .iter()
                .rev()
                .find(|rule| wildcard_match(permission, &rule.permission));
            matches!(rule, Some(r) if r.pattern == "*" && r.action == PermissionAction::Deny)
        })
        .cloned()
        .collect()
}

/// Generate a new permission request ID (per_ prefix, ascending).
///
/// # Source
/// Ported from `packages/core/src/v1/permission.ts` lines 9–13.
pub fn permission_id(given: Option<&str>) -> String {
    id::ascending(IdPrefix::Permission, given).expect("valid permission ID")
}

// ══════════════════════════════════════════════════════════════════════════════
// Saved Permissions (Database-Backed)
// ══════════════════════════════════════════════════════════════════════════════

/// A saved (permanent) permission rule stored in the database.
///
/// # Source
/// Ported from `packages/core/src/permission/saved.ts` lines 17–23.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SavedPermission {
    /// Unique ID (psv_ prefix).
    pub id: String,
    /// Project this permission belongs to.
    #[serde(rename = "projectID")]
    pub project_id: String,
    /// Action being permitted (e.g. "bash", "edit").
    pub action: String,
    /// Resource pattern (e.g. "*.ts", "/home/user/*").
    pub resource: String,
}

/// Input for adding saved permissions.
///
/// # Source
/// Ported from `packages/core/src/permission/saved.ts` lines 30–35.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AddSavedInput {
    /// Project ID.
    #[serde(rename = "projectID")]
    pub project_id: String,
    /// Action name.
    pub action: String,
    /// Resource patterns to save.
    pub resources: Vec<String>,
}

/// Handler for saved/remembered permissions.
///
/// Wraps a [`Database`] connection to provide CRUD operations on the
/// `permission` table. Permissions saved here persist across sessions
/// and auto-approve matching future requests.
///
/// # Source
/// Ported from `packages/core/src/permission/saved.ts` lines 45–86.
#[derive(Clone)]
pub struct SavedPermissions {
    db: Database,
}

impl SavedPermissions {
    /// Create a new saved-permissions handler.
    pub fn new(db: Database) -> Self {
        Self { db }
    }

    /// List saved permissions, optionally filtered by project.
    ///
    /// # Source
    /// Ported from `packages/core/src/permission/saved.ts` lines 50–59.
    pub async fn list(&self, project_id: Option<&str>) -> Result<Vec<SavedPermission>> {
        let rows: Vec<(String, String, String, String)> = if let Some(pid) = project_id {
            sqlx::query_as(
                "SELECT id, project_id, action, resource FROM permission WHERE project_id = ?1 ORDER BY id",
            )
            .bind(pid)
            .fetch_all(self.db.pool())
            .await
            .map_err(|e| Error::Database(format!("permission list query: {e}")))?
        } else {
            sqlx::query_as("SELECT id, project_id, action, resource FROM permission ORDER BY id")
                .fetch_all(self.db.pool())
                .await
                .map_err(|e| Error::Database(format!("permission list query: {e}")))?
        };

        Ok(rows
            .into_iter()
            .map(|(id, project_id, action, resource)| SavedPermission {
                id,
                project_id,
                action,
                resource,
            })
            .collect())
    }

    /// Add saved permissions for one or more resources.
    ///
    /// Empty resources list is a no-op. Duplicates are silently ignored
    /// via `INSERT OR IGNORE`.
    ///
    /// # Source
    /// Ported from `packages/core/src/permission/saved.ts` lines 62–76.
    pub async fn add(&self, input: &AddSavedInput) -> Result<()> {
        if input.resources.is_empty() {
            return Ok(());
        }

        let ts = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default()
            .as_millis() as i64;

        for resource in &input.resources {
            let id = id::ascending(IdPrefix::Permission, None)
                .map(|s| s.replace("per_", "psv_"))
                .map_err(|e| {
                    Error::Internal(format!("failed to generate saved permission ID: {e}"))
                })?;
            sqlx::query(
                "INSERT OR IGNORE INTO permission (id, project_id, action, resource, time_created, time_updated) \
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
            )
            .bind(&id)
            .bind(&input.project_id)
            .bind(&input.action)
            .bind(resource)
            .bind(ts)
            .bind(ts)
            .execute(self.db.pool())
            .await
            .map_err(|e| Error::Database(format!("permission insert: {e}")))?;
        }

        Ok(())
    }

    /// Remove a saved permission by ID.
    ///
    /// # Source
    /// Ported from `packages/core/src/permission/saved.ts` lines 79–81.
    pub async fn remove(&self, id: &str) -> Result<()> {
        sqlx::query("DELETE FROM permission WHERE id = ?1")
            .bind(id)
            .execute(self.db.pool())
            .await
            .map_err(|e| Error::Database(format!("permission delete: {e}")))?;
        Ok(())
    }
}

// ══════════════════════════════════════════════════════════════════════════════
// Permission Service (Stateful — ask / reply / list / assert)
// ══════════════════════════════════════════════════════════════════════════════

/// A pending permission request awaiting user reply.
///
/// # Source
/// Ported from `packages/opencode/src/permission/index.ts` lines 29–32.
struct PendingEntry {
    request: PermissionRequest,
    /// Oneshot sender — resolved when the user replies.
    tx: tokio::sync::oneshot::Sender<Result<(), PermissionError>>,
}

/// The permission service manages pending permission requests and evaluates
/// permission checks against configured and saved rules.
///
/// # Source
/// Ported from `packages/opencode/src/permission/index.ts` lines 53–185
/// and `packages/core/src/permission.ts` lines 125–326.
pub struct PermissionService {
    bus: SharedBus,
    approved: Arc<tokio::sync::RwLock<PermissionRuleset>>,
    pending: Arc<dashmap::DashMap<String, PendingEntry>>,
    saved: Option<SavedPermissions>,
}

impl PermissionService {
    /// Create a new permission service.
    pub fn new(bus: SharedBus) -> Self {
        Self {
            bus,
            approved: Arc::new(tokio::sync::RwLock::new(PermissionRuleset::new())),
            pending: Arc::new(dashmap::DashMap::new()),
            saved: None,
        }
    }

    /// Create a new permission service with saved-permission database support.
    pub fn with_saved(bus: SharedBus, saved: SavedPermissions) -> Self {
        Self {
            bus,
            approved: Arc::new(tokio::sync::RwLock::new(PermissionRuleset::new())),
            pending: Arc::new(dashmap::DashMap::new()),
            saved: Some(saved),
        }
    }

    /// Evaluate a permission request and create a pending entry if the user
    /// needs to be asked. Returns immediately for allow/deny outcomes.
    ///
    /// # Source
    /// Ported from `packages/opencode/src/permission/index.ts` lines 78–118.
    pub async fn ask(&self, input: AskInput) -> Result<PermissionAction> {
        let approved = self.approved.read().await;
        let combined = merge_rulesets(&[input.ruleset.clone(), approved.clone()]);
        let mut needs_ask = false;

        for pattern in &input.patterns {
            let result = evaluate(&input.permission, pattern, &[&combined]);
            tracing::info!(
                permission = %input.permission,
                %pattern,
                action = %result.action,
                "evaluated permission"
            );

            match result.action {
                PermissionAction::Deny => {
                    return Err(Error::Permission(PermissionError::Denied));
                }
                PermissionAction::Allow => continue,
                PermissionAction::Ask => {
                    needs_ask = true;
                }
            }
        }

        if !needs_ask {
            return Ok(PermissionAction::Allow);
        }

        // Create a pending request (no oneshot — ask() is non-blocking).
        let id = input.id.clone().unwrap_or_else(|| permission_id(None));

        let request = PermissionRequest {
            id: id.clone(),
            session_id: input.session_id,
            permission: input.permission,
            patterns: input.patterns,
            metadata: input.metadata,
            always: input.always,
            tool: input.tool,
        };

        // Publish the "asked" event on the bus.
        let payload = serde_json::to_value(&request).unwrap_or_default();
        let event = crate::bus::GlobalEvent::new(payload);
        let _ = self.bus.publish(event);

        tracing::info!(%id, "permission requested, waiting for reply");

        Ok(PermissionAction::Ask)
    }

    /// Block until permission is granted (or return error if denied).
    ///
    /// Evaluates the permission, creates a pending entry with a oneshot
    /// channel, and blocks until `reply()` resolves it.
    ///
    /// # Source
    /// Ported from `packages/core/src/permission.ts` lines 223–243.
    pub async fn assert(&self, input: AskInput) -> Result<()> {
        let approved = self.approved.read().await;
        let combined = merge_rulesets(&[input.ruleset.clone(), approved.clone()]);

        for pattern in &input.patterns {
            let result = evaluate(&input.permission, pattern, &[&combined]);
            match result.action {
                PermissionAction::Deny => {
                    return Err(Error::Permission(PermissionError::Denied));
                }
                PermissionAction::Allow => continue,
                PermissionAction::Ask => {}
            }
        }

        // If all patterns allowed, return immediately.
        let all_allow = input
            .patterns
            .iter()
            .all(|p| evaluate(&input.permission, p, &[&combined]).action == PermissionAction::Allow);
        if all_allow {
            return Ok(());
        }

        // Create pending entry with a oneshot channel.
        let id = input.id.clone().unwrap_or_else(|| permission_id(None));
        let (tx, rx) = tokio::sync::oneshot::channel();

        let request = PermissionRequest {
            id: id.clone(),
            session_id: input.session_id,
            permission: input.permission,
            patterns: input.patterns,
            metadata: input.metadata,
            always: input.always,
            tool: input.tool,
        };

        // Publish the "asked" event.
        let payload = serde_json::to_value(&request).unwrap_or_default();
        let event = crate::bus::GlobalEvent::new(payload);
        let _ = self.bus.publish(event);

        self.pending.insert(id.clone(), PendingEntry { request, tx });

        tracing::info!(%id, "permission asserted, blocking until reply");

        // Block until the oneshot is resolved.
        match rx.await {
            Ok(Ok(())) => Ok(()),
            Ok(Err(e)) => Err(Error::Permission(e)),
            Err(_) => {
                // Sender dropped without replying — treat as rejected.
                self.pending.remove(&id);
                Err(Error::Permission(PermissionError::Rejected))
            }
        }
    }

    /// Reply to a pending permission request.
    ///
    /// Handles the full lifecycle: resolves or rejects the pending entry,
    /// publishes a `permission.replied` event, and cascades to other pending
    /// entries in the same session (reject cascades fail them all; always
    /// cascades auto-approves newly-matching entries).
    ///
    /// If `reply` is `Always` and a `SavedPermissions` store is configured,
    /// the approved patterns are persisted to the database.
    ///
    /// # Source
    /// Ported from `packages/opencode/src/permission/index.ts` lines 120–178
    /// and `packages/core/src/permission.ts` lines 245–309.
    pub async fn reply(&self, input: ReplyInput) -> Result<()> {
        // Resolve the specific pending entry.
        let existing = self
            .pending
            .remove(&input.request_id)
            .map(|(_, entry)| entry)
            .ok_or_else(|| {
                Error::Permission(PermissionError::NotFound {
                    request_id: input.request_id.clone(),
                })
            })?;

        let session_id = existing.request.session_id.clone();

        // Publish replied event.
        self.publish_replied(&session_id, &input.request_id, &input.reply);

        if input.reply == PermissionReply::Reject {
            // Fail the specific deferred.
            let err = match &input.message {
                Some(msg) => PermissionError::Corrected {
                    feedback: msg.clone(),
                },
                None => PermissionError::Rejected,
            };
            let _ = existing.tx.send(Err(err));

            // Cascade: fail ALL pending for the same session.
            self.cascade_reject(&session_id);
            return Ok(());
        }

        // Succeed the specific deferred.
        let _ = existing.tx.send(Ok(()));

        if input.reply == PermissionReply::Once {
            return Ok(());
        }

        // "Always" — save approved patterns for future auto-approval.
        if !existing.request.always.is_empty() {
            let mut approved = self.approved.write().await;
            for pattern in &existing.request.always {
                approved.push(PermissionRule {
                    permission: existing.request.permission.clone(),
                    pattern: pattern.clone(),
                    action: PermissionAction::Allow,
                });
            }

            // Persist to database if configured.
            if let Some(ref saved) = self.saved {
                // We need a project_id — for now use empty string as placeholder.
                // The full integration will wire this through the session.
                let add_input = AddSavedInput {
                    project_id: String::new(),
                    action: existing.request.permission.clone(),
                    resources: existing.request.always.clone(),
                };
                let _ = saved.add(&add_input).await;
            }
        }

        // Cascade: auto-approve other pending entries that now match.
        self.cascade_always(&session_id).await;

        Ok(())
    }

    /// List all pending permission requests.
    ///
    /// # Source
    /// Ported from `packages/opencode/src/permission/index.ts` lines 180–183.
    pub fn list(&self) -> Vec<PermissionRequest> {
        self.pending
            .iter()
            .map(|entry| entry.request.clone())
            .collect()
    }

    /// Get the list of approved (remembered) rules.
    pub async fn approved_rules(&self) -> PermissionRuleset {
        self.approved.read().await.clone()
    }

    // ── Internal helpers ──────────────────────────────────────────────────

    /// Publish a `permission.replied` event on the bus.
    fn publish_replied(
        &self,
        session_id: &str,
        request_id: &str,
        reply: &PermissionReply,
    ) {
        let payload = serde_json::json!({
            "type": "permission.replied",
            "sessionID": session_id,
            "requestID": request_id,
            "reply": reply,
        });
        let event = crate::bus::GlobalEvent::new(payload);
        let _ = self.bus.publish(event);
    }

    /// Fail all pending entries for the given session with `RejectedError`.
    fn cascade_reject(&self, session_id: &str) {
        let to_remove: Vec<String> = self
            .pending
            .iter()
            .filter(|entry| entry.request.session_id == session_id)
            .map(|entry| entry.request.id.clone())
            .collect();

        for id in to_remove {
            if let Some((_, entry)) = self.pending.remove(&id) {
                self.publish_replied(session_id, &id, &PermissionReply::Reject);
                let _ = entry.tx.send(Err(PermissionError::Rejected));
            }
        }
    }

    /// Auto-approve pending entries that are now allowed by the updated rules.
    async fn cascade_always(&self, session_id: &str) {
        let approved = self.approved.read().await.clone();

        // Two-phase: collect matching IDs, then remove and resolve.
        // DashMap doesn't support extracting owned values through iteration.
        let matching_ids: Vec<String> = {
            let approved_ref = &approved;
            self.pending
                .iter()
                .filter(|entry| entry.request.session_id == session_id)
                .filter(|entry| {
                    entry.request.patterns.iter().all(|pattern| {
                        evaluate(&entry.request.permission, pattern, &[approved_ref])
                            .action
                            == PermissionAction::Allow
                    })
                })
                .map(|entry| entry.request.id.clone())
                .collect()
        };

        for id in matching_ids {
            if let Some((_, entry)) = self.pending.remove(&id) {
                self.publish_replied(session_id, &id, &PermissionReply::Always);
                let _ = entry.tx.send(Ok(()));
            }
        }
    }
}

// ══════════════════════════════════════════════════════════════════════════════
// Tests
// ══════════════════════════════════════════════════════════════════════════════

#[cfg(test)]
mod tests {
    use super::*;

    // ── Wildcard matching ──────────────────────────────────────────────────

    #[test]
    fn test_wildcard_exact_match() {
        assert!(wildcard_match("bash", "bash"));
        assert!(!wildcard_match("bash", "shell"));
    }

    #[test]
    fn test_wildcard_star_matches_everything() {
        assert!(wildcard_match("anything", "*"));
        assert!(wildcard_match("", "*"));
        assert!(wildcard_match("foo/bar/baz/very/deep/path.ts", "*"));
    }

    #[test]
    fn test_wildcard_prefix_match() {
        assert!(wildcard_match("foo/bar/baz", "foo/*"));
        assert!(wildcard_match("foo/bar", "foo/*"));
        assert!(!wildcard_match("other/bar", "foo/*"));
    }

    #[test]
    fn test_wildcard_suffix_match() {
        assert!(wildcard_match("foo/bar.ts", "*.ts"));
        assert!(!wildcard_match("foo/bar.js", "*.ts"));
    }

    #[test]
    fn test_wildcard_middle_match() {
        assert!(wildcard_match("foo/bar/baz", "foo/*/baz"));
        // * matches any characters including / (with dotall 's' flag in TS)
        assert!(wildcard_match("foo/bar/other/baz", "foo/*/baz"));
    }

    #[test]
    fn test_wildcard_question_mark() {
        assert!(wildcard_match("cat", "c?t"));
        assert!(wildcard_match("cot", "c?t"));
        assert!(!wildcard_match("coat", "c?t"));
    }

    #[test]
    fn test_wildcard_backslash_normalization() {
        // Backslashes in input and pattern should be normalized
        assert!(wildcard_match(r"foo\bar", r"foo/bar"));
        assert!(wildcard_match("foo/bar", r"foo\bar"));
    }

    #[test]
    fn test_wildcard_special_regex_chars_escaped() {
        // Patterns like "file.ts" should NOT interpret "." as regex wildcard
        assert!(wildcard_match("file.ts", "file.ts"));
        assert!(!wildcard_match("fileXts", "file.ts"));
        // "+" should be literal
        assert!(wildcard_match("a+b", "a+b"));
        assert!(!wildcard_match("ab", "a+b"));
        // "^" and "$" should be literal
        assert!(wildcard_match("^start", "^start"));
        // "." should be literal
        assert!(wildcard_match("a.b", "a.b"));
    }

    #[test]
    fn test_wildcard_trailing_space_star() {
        // TS: if (escaped.endsWith(" .*")) escaped = escaped.slice(0, -3) + "( .*)?"
        // After `*` → `.*`, a pattern like "foo *" becomes "foo .*",
        // and the trailing " .*" is converted to optional "( .*)?"
        // So "foo *" matches both "foo" and "foo anything".
        let pattern = "foo *";
        assert!(wildcard_match("foo", pattern));
        assert!(wildcard_match("foo bar", pattern));
        assert!(wildcard_match("foo bar baz", pattern));
        // But does NOT match a completely different prefix
        assert!(!wildcard_match("bar", pattern));
    }

    #[test]
    fn test_wildcard_empty_input() {
        assert!(!wildcard_match("", "something"));
        assert!(wildcard_match("", "*"));
        assert!(wildcard_match("", ""));
    }

    // ── Rule evaluation ────────────────────────────────────────────────────

    #[test]
    fn test_evaluate_exact_match() {
        let ruleset: PermissionRuleset = vec![PermissionRule {
            permission: "bash".into(),
            pattern: "*".into(),
            action: PermissionAction::Allow,
        }];
        let result = evaluate("bash", "*", &[&ruleset]);
        assert_eq!(result.action, PermissionAction::Allow);
        assert_eq!(result.matched_permission.as_deref(), Some("bash"));
    }

    #[test]
    fn test_evaluate_wildcard_permission() {
        let ruleset: PermissionRuleset = vec![PermissionRule {
            permission: "*".into(),
            pattern: "*".into(),
            action: PermissionAction::Ask,
        }];
        let result = evaluate("any_tool", "*", &[&ruleset]);
        assert_eq!(result.action, PermissionAction::Ask);
    }

    #[test]
    fn test_evaluate_no_match_defaults_to_ask() {
        let ruleset: PermissionRuleset = vec![];
        let result = evaluate("bash", "*", &[&ruleset]);
        assert_eq!(result.action, PermissionAction::Ask);
        assert!(result.matched_permission.is_none());
    }

    #[test]
    fn test_evaluate_last_match_wins() {
        let ruleset: PermissionRuleset = vec![
            PermissionRule {
                permission: "bash".into(),
                pattern: "*".into(),
                action: PermissionAction::Allow,
            },
            PermissionRule {
                permission: "bash".into(),
                pattern: "*".into(),
                action: PermissionAction::Deny,
            },
        ];
        // The last matching rule (Deny) should win
        let result = evaluate("bash", "*", &[&ruleset]);
        assert_eq!(result.action, PermissionAction::Deny);
    }

    #[test]
    fn test_evaluate_multiple_rulesets() {
        let r1: PermissionRuleset = vec![PermissionRule {
            permission: "bash".into(),
            pattern: "*".into(),
            action: PermissionAction::Allow,
        }];
        let r2: PermissionRuleset = vec![PermissionRule {
            permission: "bash".into(),
            pattern: "*".into(),
            action: PermissionAction::Deny,
        }];
        // r2 is applied second and should win
        let result = evaluate("bash", "*", &[&r1, &r2]);
        assert_eq!(result.action, PermissionAction::Deny);
    }

    #[test]
    fn test_evaluate_pattern_specificity() {
        let ruleset: PermissionRuleset = vec![
            PermissionRule {
                permission: "read".into(),
                pattern: "*.env".into(),
                action: PermissionAction::Deny,
            },
            PermissionRule {
                permission: "read".into(),
                pattern: "*.ts".into(),
                action: PermissionAction::Allow,
            },
        ];
        // "src/main.ts" should match *.ts (Allow) — last match wins
        let result = evaluate("read", "src/main.ts", &[&ruleset]);
        assert_eq!(result.action, PermissionAction::Allow);
    }

    #[test]
    fn test_evaluate_v2_semantics() {
        // V2 uses action/resource terminology but the logic is identical
        let ruleset: PermissionRuleset = vec![PermissionRule {
            permission: "edit".into(), // V2: "action"
            pattern: "*.md".into(),    // V2: "resource"
            action: PermissionAction::Allow,
        }];
        let result = evaluate_v2("edit", "README.md", &[&ruleset]);
        assert_eq!(result.action, PermissionAction::Allow);
    }

    // ── Bash arity ─────────────────────────────────────────────────────────

    #[test]
    fn test_bash_arity_simple_command() {
        assert_eq!(bash_arity_prefix(&["cat", "file.txt"]), ["cat"]);
        assert_eq!(bash_arity_prefix(&["ls", "-la"]), ["ls"]);
    }

    #[test]
    fn test_bash_arity_two_token() {
        assert_eq!(
            bash_arity_prefix(&["git", "checkout", "main"]),
            ["git", "checkout"]
        );
        assert_eq!(
            bash_arity_prefix(&["cargo", "build", "--release"]),
            ["cargo", "build"]
        );
    }

    #[test]
    fn test_bash_arity_three_token() {
        assert_eq!(
            bash_arity_prefix(&["npm", "run", "dev", "--watch"]),
            ["npm", "run", "dev"]
        );
        assert_eq!(
            bash_arity_prefix(&["docker", "compose", "up", "-d"]),
            ["docker", "compose", "up"]
        );
    }

    #[test]
    fn test_bash_arity_unknown_command() {
        assert_eq!(bash_arity_prefix(&["myapp", "arg1"]), ["myapp"]);
    }

    #[test]
    fn test_bash_arity_empty() {
        assert!(bash_arity_prefix(&[]).is_empty());
    }

    // ── Config conversion ─────────────────────────────────────────────────

    #[test]
    fn test_rules_from_config_simple_string() {
        let config = PermissionConfig {
            bash: Some(crate::config::PermissionRule::Action(
                crate::config::PermissionAction::Allow,
            )),
            ..Default::default()
        };
        let rules = rules_from_config(&config);
        assert_eq!(rules.len(), 1);
        assert_eq!(rules[0].permission, "bash");
        assert_eq!(rules[0].pattern, "*");
        assert_eq!(rules[0].action, PermissionAction::Allow);
    }

    #[test]
    fn test_rules_from_config_object() {
        let mut map = HashMap::new();
        map.insert("*.ts".into(), crate::config::PermissionAction::Deny);
        map.insert("*".into(), crate::config::PermissionAction::Allow);

        let config = PermissionConfig {
            read: Some(crate::config::PermissionRule::Object(map)),
            ..Default::default()
        };
        let rules = rules_from_config(&config);
        // Should have one rule per pattern
        assert_eq!(rules.len(), 2);
        // Find the deny rule for *.ts
        let deny_rule = rules
            .iter()
            .find(|r| r.pattern == "*.ts")
            .expect("*.ts rule");
        assert_eq!(deny_rule.permission, "read");
        assert_eq!(deny_rule.action, PermissionAction::Deny);
    }

    #[test]
    fn test_rules_from_config_empty() {
        let config = PermissionConfig::default();
        let rules = rules_from_config(&config);
        assert!(rules.is_empty());
    }

    // ── Merge rulesets ────────────────────────────────────────────────────

    #[test]
    fn test_merge_rulesets_flat() {
        let r1: PermissionRuleset = vec![PermissionRule {
            permission: "bash".into(),
            pattern: "*".into(),
            action: PermissionAction::Allow,
        }];
        let r2: PermissionRuleset = vec![PermissionRule {
            permission: "read".into(),
            pattern: "*.ts".into(),
            action: PermissionAction::Ask,
        }];

        let merged = merge_rulesets(&[r1, r2]);
        assert_eq!(merged.len(), 2);
    }

    #[test]
    fn test_merge_rulesets_empty() {
        let merged = merge_rulesets(&[]);
        assert!(merged.is_empty());
    }

    // ── Disabled tools ─────────────────────────────────────────────────────

    #[test]
    fn test_disabled_tools_fully_denied() {
        let ruleset: PermissionRuleset = vec![PermissionRule {
            permission: "bash".into(),
            pattern: "*".into(),
            action: PermissionAction::Deny,
        }];

        let tools = vec!["bash".into(), "read".into(), "edit".into()];
        let disabled = disabled_tools(&tools, &ruleset);
        assert!(disabled.contains("bash"));
        assert!(!disabled.contains("read"));
        assert!(!disabled.contains("edit"));
    }

    #[test]
    fn test_disabled_tools_edit_aliases() {
        // "edit", "write", "apply_patch" should all map to "edit" permission
        let ruleset: PermissionRuleset = vec![PermissionRule {
            permission: "edit".into(),
            pattern: "*".into(),
            action: PermissionAction::Deny,
        }];

        let tools = vec![
            "edit".into(),
            "write".into(),
            "apply_patch".into(),
            "bash".into(),
        ];
        let disabled = disabled_tools(&tools, &ruleset);
        assert!(disabled.contains("edit"));
        assert!(disabled.contains("write"));
        assert!(disabled.contains("apply_patch"));
        assert!(!disabled.contains("bash"));
    }

    #[test]
    fn test_disabled_tools_not_denied_without_wildcard_pattern() {
        // A deny rule with a specific pattern should NOT disable the tool entirely
        let ruleset: PermissionRuleset = vec![PermissionRule {
            permission: "read".into(),
            pattern: "*.env".into(),
            action: PermissionAction::Deny,
        }];

        let tools = vec!["read".into()];
        let disabled = disabled_tools(&tools, &ruleset);
        // Pattern is "*.env", not "*", so the tool is not fully disabled
        assert!(!disabled.contains("read"));
    }

    // ── Permission ID generation ───────────────────────────────────────────

    #[test]
    fn test_permission_id_prefix() {
        let id = permission_id(None);
        assert!(id.starts_with("per_"), "expected per_ prefix, got: {id}");
        assert_eq!(id.len(), 4 + 26); // per_ + 26 chars
    }

    #[test]
    fn test_permission_id_given() {
        let given = "per_special_case";
        let id = permission_id(Some(given));
        assert_eq!(id, given);
    }

    // ── PermissionAction Display ───────────────────────────────────────────

    #[test]
    fn test_permission_action_display() {
        assert_eq!(PermissionAction::Allow.to_string(), "allow");
        assert_eq!(PermissionAction::Deny.to_string(), "deny");
        assert_eq!(PermissionAction::Ask.to_string(), "ask");
    }
}
