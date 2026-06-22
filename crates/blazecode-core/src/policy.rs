//! Policy evaluation — action/resource rule matching with wildcard support.
//!
//! Ported from: `packages/core/src/policy.ts` (lines 1–47)
//!   BlazeCode commit: 5d0f86606ac30690f79f0a6a9f41a1f49fe95d0b
//!
//! ## Architecture
//!
//! The TS `Policy` module provides a simple policy engine:
//!
//! - Each [`PolicyStatement`] maps an action pattern + resource pattern to an effect (allow/deny).
//! - `Policy.evaluate(action, resource, fallback)` finds the **last** matching statement
//!   (wildcard matching) and returns its effect, or the fallback.
//! - `Wildcard.match()` is used for pattern matching (ported to Rust as regex-based matching).
//!
//! In Rust:
//! - [`PolicyEffect`] enum mirrors `allow` / `deny`.
//! - [`PolicyStatement`] holds `action`, `effect`, `resource`.
//! - [`PolicyEngine`] stores ordered statements and implements `evaluate()`.
//! - Wildcard matching uses regex conversion from glob-like patterns.

use serde::{Deserialize, Serialize};

// ---------------------------------------------------------------------------
// PolicyEffect
// ---------------------------------------------------------------------------

/// The effect of a policy statement — allow or deny.
///
/// # Source
/// Ported from `packages/core/src/policy.ts` line 7:
/// `Policy.Effect = Schema.Literals(["allow", "deny"])`
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum PolicyEffect {
    /// The action is explicitly allowed.
    Allow,
    /// The action is explicitly denied.
    Deny,
}

impl std::fmt::Display for PolicyEffect {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Allow => write!(f, "allow"),
            Self::Deny => write!(f, "deny"),
        }
    }
}

impl PolicyEffect {
    /// Returns true if this effect is `Allow`.
    pub fn is_allow(&self) -> bool {
        matches!(self, Self::Allow)
    }

    /// Returns true if this effect is `Deny`.
    pub fn is_deny(&self) -> bool {
        matches!(self, Self::Deny)
    }
}

// ---------------------------------------------------------------------------
// PolicyStatement
// ---------------------------------------------------------------------------

/// A single policy statement — maps action + resource to an effect.
///
/// # Source
/// Ported from `packages/core/src/policy.ts` lines 10–14.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct PolicyStatement {
    /// Action pattern (e.g. `"read"`, `"*"`, `"tool:*"`).
    pub action: String,
    /// The effect when this statement matches.
    pub effect: PolicyEffect,
    /// Resource pattern (e.g. `"*"`, `"/home/*"`, `"file:*.rs"`).
    pub resource: String,
}

impl PolicyStatement {
    /// Create a new policy statement.
    pub fn new(
        action: impl Into<String>,
        effect: PolicyEffect,
        resource: impl Into<String>,
    ) -> Self {
        Self {
            action: action.into(),
            effect,
            resource: resource.into(),
        }
    }

    /// An allow-all statement (action="*", effect=Allow, resource="*").
    pub fn allow_all() -> Self {
        Self {
            action: "*".into(),
            effect: PolicyEffect::Allow,
            resource: "*".into(),
        }
    }

    /// A deny-all statement (action="*", effect=Deny, resource="*").
    pub fn deny_all() -> Self {
        Self {
            action: "*".into(),
            effect: PolicyEffect::Deny,
            resource: "*".into(),
        }
    }
}

// ---------------------------------------------------------------------------
// Wildcard matching
// ---------------------------------------------------------------------------

/// Convert a wildcard pattern to a regex string.
///
/// Supports `*` (matches any sequence of characters except `/`) and
/// `**` (matches any sequence including `/`).
///
/// # Source
/// Ported from `packages/core/src/util/wildcard.ts` — `Wildcard.match()`.
fn pattern_to_regex(pattern: &str) -> String {
    let mut regex = String::from("^");
    let chars: Vec<char> = pattern.chars().collect();
    let mut i = 0;

    while i < chars.len() {
        match chars[i] {
            '*' => {
                if i + 1 < chars.len() && chars[i + 1] == '*' {
                    // ** — match any sequence including slashes
                    regex.push_str(".*");
                    i += 2;
                } else {
                    // * — match any sequence except slashes
                    regex.push_str("[^/]*");
                    i += 1;
                }
            }
            '?' => {
                regex.push_str("[^/]");
                i += 1;
            }
            c if ".^$+()[]{}|\\".contains(c) => {
                // Escape regex special characters
                regex.push('\\');
                regex.push(c);
                i += 1;
            }
            c => {
                regex.push(c);
                i += 1;
            }
        }
    }

    regex.push('$');
    regex
}

/// Test whether a value matches a wildcard pattern.
///
/// # Source
/// Ported from `packages/core/src/util/wildcard.ts` — `Wildcard.match(action, statement.action)`.
pub fn wildcard_match(pattern: &str, value: &str) -> bool {
    // Fast path: exact match or "*"
    if pattern == "*" || pattern == "**" || pattern == value {
        return true;
    }

    // Fast path: no wildcards — just compare
    if !pattern.contains('*') && !pattern.contains('?') {
        return pattern == value;
    }

    let regex_str = pattern_to_regex(pattern);
    match regex::Regex::new(&regex_str) {
        Ok(re) => re.is_match(value),
        Err(_) => {
            tracing::warn!(
                pattern = %pattern,
                regex = %regex_str,
                "Failed to compile wildcard pattern"
            );
            false
        }
    }
}

// ---------------------------------------------------------------------------
// PolicyEngine
// ---------------------------------------------------------------------------

/// A policy engine that stores ordered statements and evaluates actions.
///
/// # Source
/// Ported from `packages/core/src/policy.ts` lines 24–43.
pub struct PolicyEngine {
    /// Ordered policy statements. Last matching statement wins.
    statements: Vec<PolicyStatement>,
}

impl PolicyEngine {
    /// Create a new empty policy engine.
    pub fn new() -> Self {
        Self {
            statements: Vec::new(),
        }
    }

    /// Create a policy engine pre-loaded with statements.
    pub fn with_statements(statements: Vec<PolicyStatement>) -> Self {
        Self { statements }
    }

    /// Load (replace) all policy statements.
    ///
    /// # Source
    /// Ported from `packages/core/src/policy.ts` lines 30–31 (`load()`).
    pub fn load(&mut self, statements: Vec<PolicyStatement>) {
        self.statements = statements;
    }

    /// Evaluate an action against a resource and return the resulting effect.
    ///
    /// Finds the **last** matching statement (by wildcard matching on both
    /// action and resource) and returns its effect, or the fallback.
    ///
    /// # Source
    /// Ported from `packages/core/src/policy.ts` lines 35–41 (`evaluate()`).
    pub fn evaluate(&self, action: &str, resource: &str, fallback: PolicyEffect) -> PolicyEffect {
        self.statements
            .iter()
            .rev()
            .find(|stmt| {
                wildcard_match(&stmt.action, action) && wildcard_match(&stmt.resource, resource)
            })
            .map(|stmt| stmt.effect)
            .unwrap_or(fallback)
    }

    /// Returns true if the engine has any statements loaded.
    ///
    /// # Source
    /// Ported from `packages/core/src/policy.ts` line 32 (`hasStatements()`).
    pub fn has_statements(&self) -> bool {
        !self.statements.is_empty()
    }

    /// Returns the number of loaded statements.
    pub fn statement_count(&self) -> usize {
        self.statements.len()
    }

    /// Get a reference to all statements.
    pub fn statements(&self) -> &[PolicyStatement] {
        &self.statements
    }
}

impl Default for PolicyEngine {
    fn default() -> Self {
        Self::new()
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    // ── PolicyEffect tests ─────────────────────────────────────────────

    #[test]
    fn policy_effect_display() {
        assert_eq!(PolicyEffect::Allow.to_string(), "allow");
        assert_eq!(PolicyEffect::Deny.to_string(), "deny");
    }

    #[test]
    fn policy_effect_serialization_roundtrip() {
        let allow = PolicyEffect::Allow;
        let json = serde_json::to_string(&allow).unwrap();
        assert_eq!(json, r#""allow""#);

        let parsed: PolicyEffect = serde_json::from_str(r#""deny""#).unwrap();
        assert_eq!(parsed, PolicyEffect::Deny);
    }

    #[test]
    fn policy_effect_is_methods() {
        assert!(PolicyEffect::Allow.is_allow());
        assert!(!PolicyEffect::Allow.is_deny());
        assert!(PolicyEffect::Deny.is_deny());
        assert!(!PolicyEffect::Deny.is_allow());
    }

    // ── PolicyStatement tests ──────────────────────────────────────────

    #[test]
    fn policy_statement_new() {
        let stmt = PolicyStatement::new("read", PolicyEffect::Allow, "*.rs");
        assert_eq!(stmt.action, "read");
        assert_eq!(stmt.effect, PolicyEffect::Allow);
        assert_eq!(stmt.resource, "*.rs");
    }

    #[test]
    fn policy_statement_allow_all() {
        let stmt = PolicyStatement::allow_all();
        assert_eq!(stmt.action, "*");
        assert_eq!(stmt.effect, PolicyEffect::Allow);
        assert_eq!(stmt.resource, "*");
    }

    #[test]
    fn policy_statement_deny_all() {
        let stmt = PolicyStatement::deny_all();
        assert_eq!(stmt.action, "*");
        assert_eq!(stmt.effect, PolicyEffect::Deny);
        assert_eq!(stmt.resource, "*");
    }

    #[test]
    fn policy_statement_serialization_roundtrip() {
        let stmt = PolicyStatement::new("write", PolicyEffect::Deny, "/etc/*");
        let json = serde_json::to_string(&stmt).unwrap();
        let parsed: PolicyStatement = serde_json::from_str(&json).unwrap();
        assert_eq!(parsed.action, "write");
        assert_eq!(parsed.effect, PolicyEffect::Deny);
        assert_eq!(parsed.resource, "/etc/*");
    }

    // ── Wildcard matching tests ────────────────────────────────────────

    #[test]
    fn wildcard_star_matches_everything() {
        assert!(wildcard_match("*", "anything"));
        assert!(wildcard_match("**", "any/thing/at/all"));
        assert!(wildcard_match("*", ""));
        assert!(wildcard_match("**", "foo/bar/baz"));
    }

    #[test]
    fn wildcard_exact_match() {
        assert!(wildcard_match("foo", "foo"));
        assert!(!wildcard_match("foo", "bar"));
        assert!(wildcard_match("exact-match", "exact-match"));
    }

    #[test]
    fn wildcard_single_star_matches_within_segment() {
        assert!(wildcard_match("*.rs", "main.rs"));
        assert!(wildcard_match("*.rs", "lib.rs"));
        assert!(!wildcard_match("*.rs", "main.py"));
        assert!(!wildcard_match("*.rs", "src/main.rs"));
    }

    #[test]
    fn wildcard_double_star_matches_across_slashes() {
        assert!(wildcard_match("**/*.rs", "src/main.rs"));
        assert!(wildcard_match("**/*.rs", "src/lib/core/test.rs"));
        assert!(!wildcard_match("**/*.rs", "main.py"));
    }

    #[test]
    fn wildcard_question_mark_matches_single_char() {
        assert!(wildcard_match("file-?.txt", "file-a.txt"));
        assert!(wildcard_match("file-?.txt", "file-1.txt"));
        assert!(!wildcard_match("file-?.txt", "file-ab.txt"));
        assert!(!wildcard_match("file-?.txt", "file-a.csv"));
    }

    #[test]
    fn wildcard_prefix_match() {
        assert!(wildcard_match("tool:*", "tool:bash"));
        assert!(wildcard_match("tool:*", "tool:edit"));
        assert!(wildcard_match("tool:*", "tool:read"));
        assert!(!wildcard_match("tool:*", "bash"));
        assert!(!wildcard_match("tool:*", "skill:foo"));
    }

    #[test]
    fn wildcard_path_match() {
        assert!(wildcard_match("/home/*", "/home/user"));
        assert!(wildcard_match("/home/*", "/home/project"));
        assert!(!wildcard_match("/home/*", "/home/user/docs"));
        assert!(!wildcard_match("/home/*", "/etc/passwd"));
    }

    #[test]
    fn wildcard_recursive_path_match() {
        assert!(wildcard_match("/home/**", "/home/user"));
        assert!(wildcard_match("/home/**", "/home/user/docs/deep/file.txt"));
        assert!(!wildcard_match("/home/**", "/etc/passwd"));
    }

    #[test]
    fn wildcard_regex_special_chars() {
        // Characters that are special in regex should be escaped
        assert!(wildcard_match("file.v[0-9]", "file.v[0-9]"));
        assert!(!wildcard_match("file.v[0-9]", "file.v3"));
    }

    #[test]
    fn wildcard_star_in_middle() {
        assert!(wildcard_match("start-*-end", "start-middle-end"));
        assert!(wildcard_match("start-*-end", "start--end"));
        assert!(wildcard_match("start-*-end", "start-middle-extra-end"));
    }

    // ── PolicyEngine tests ─────────────────────────────────────────────

    #[test]
    fn engine_empty_returns_fallback() {
        let engine = PolicyEngine::new();
        assert_eq!(
            engine.evaluate("read", "file.txt", PolicyEffect::Allow),
            PolicyEffect::Allow
        );
        assert_eq!(
            engine.evaluate("read", "file.txt", PolicyEffect::Deny),
            PolicyEffect::Deny
        );
    }

    #[test]
    fn engine_allow_all_pattern() {
        let engine = PolicyEngine::with_statements(vec![PolicyStatement::allow_all()]);
        assert_eq!(
            engine.evaluate("any-action", "any-resource", PolicyEffect::Deny),
            PolicyEffect::Allow
        );
    }

    #[test]
    fn engine_deny_all_pattern() {
        let engine = PolicyEngine::with_statements(vec![PolicyStatement::deny_all()]);
        assert_eq!(
            engine.evaluate("any-action", "any-resource", PolicyEffect::Allow),
            PolicyEffect::Deny
        );
    }

    #[test]
    fn engine_last_matching_statement_wins() {
        let engine = PolicyEngine::with_statements(vec![
            PolicyStatement::new("*", PolicyEffect::Deny, "*"),
            PolicyStatement::new("read", PolicyEffect::Allow, "*.rs"),
        ]);
        // The last matching statement for read + *.rs is Allow
        assert_eq!(
            engine.evaluate("read", "main.rs", PolicyEffect::Deny),
            PolicyEffect::Allow
        );
    }

    #[test]
    fn engine_specific_override() {
        let engine = PolicyEngine::with_statements(vec![
            PolicyStatement::new("tool:*", PolicyEffect::Allow, "*"),
            PolicyStatement::new("tool:bash", PolicyEffect::Deny, "*"),
        ]);
        // tool:bash is denied by the last statement
        assert_eq!(
            engine.evaluate("tool:bash", "/tmp/script.sh", PolicyEffect::Allow),
            PolicyEffect::Deny
        );
        // tool:edit is allowed by the first statement (last match)
        assert_eq!(
            engine.evaluate("tool:edit", "/tmp/file.txt", PolicyEffect::Deny),
            PolicyEffect::Allow
        );
    }

    #[test]
    fn engine_no_match_returns_fallback() {
        let engine = PolicyEngine::with_statements(vec![
            PolicyStatement::new("read", PolicyEffect::Allow, "*.rs"),
            PolicyStatement::new("write", PolicyEffect::Deny, "/etc/*"),
        ]);
        // "delete" action doesn't match any statement
        assert_eq!(
            engine.evaluate("delete", "file.txt", PolicyEffect::Deny),
            PolicyEffect::Deny
        );
        assert_eq!(
            engine.evaluate("delete", "file.txt", PolicyEffect::Allow),
            PolicyEffect::Allow
        );
    }

    #[test]
    fn engine_resource_matching() {
        let engine = PolicyEngine::with_statements(vec![
            PolicyStatement::new("read", PolicyEffect::Deny, "/etc/**"),
            PolicyStatement::new("read", PolicyEffect::Allow, "/home/**"),
        ]);
        assert_eq!(
            engine.evaluate("read", "/etc/passwd", PolicyEffect::Allow),
            PolicyEffect::Deny
        );
        assert_eq!(
            engine.evaluate("read", "/home/user/config", PolicyEffect::Deny),
            PolicyEffect::Allow
        );
    }

    #[test]
    fn engine_has_statements() {
        let empty = PolicyEngine::new();
        assert!(!empty.has_statements());

        let loaded = PolicyEngine::with_statements(vec![PolicyStatement::allow_all()]);
        assert!(loaded.has_statements());
    }

    #[test]
    fn engine_load_replaces() {
        let mut engine = PolicyEngine::with_statements(vec![PolicyStatement::allow_all()]);
        assert_eq!(engine.statement_count(), 1);

        engine.load(vec![
            PolicyStatement::deny_all(),
            PolicyStatement::new("read", PolicyEffect::Allow, "*.txt"),
        ]);
        assert_eq!(engine.statement_count(), 2);
    }

    #[test]
    fn engine_statement_count() {
        let engine = PolicyEngine::new();
        assert_eq!(engine.statement_count(), 0);

        let engine = PolicyEngine::with_statements(vec![
            PolicyStatement::allow_all(),
            PolicyStatement::deny_all(),
            PolicyStatement::new("read", PolicyEffect::Allow, "*.rs"),
        ]);
        assert_eq!(engine.statement_count(), 3);
    }

    #[test]
    fn policy_statement_eq() {
        let s1 = PolicyStatement::new("read", PolicyEffect::Allow, "*.rs");
        let s2 = PolicyStatement::new("read", PolicyEffect::Allow, "*.rs");
        let s3 = PolicyStatement::new("write", PolicyEffect::Deny, "*.rs");

        assert_eq!(s1, s2);
        assert_ne!(s1, s3);
    }

    #[test]
    fn wildcard_empty_string() {
        assert!(wildcard_match("*", ""));
        assert!(wildcard_match("**", ""));
        assert!(!wildcard_match("foo*", ""));
        assert!(!wildcard_match("", "something"));
    }

    #[test]
    fn wildcard_case_sensitive() {
        assert!(!wildcard_match("*.RS", "main.rs"));
        assert!(wildcard_match("*.rs", "main.rs"));
        assert!(!wildcard_match("File.txt", "file.txt"));
    }
}
