//! Permission system — gates tool execution.
//!
//! Ported from: `packages/opencode/src/permission/evaluate.ts`

use serde::{Deserialize, Serialize};

/// Permission action.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum PermissionAction {
    Allow,
    Deny,
    Ask,
}

/// A single permission rule.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PermissionRule {
    /// Tool name or pattern
    pub tool: String,
    /// Action to take
    pub action: PermissionAction,
    /// Optional pattern for matching arguments
    #[serde(skip_serializing_if = "Option::is_none")]
    pub patterns: Option<Vec<String>>,
}

/// Permission ruleset — list of rules.
pub type PermissionRuleset = Vec<PermissionRule>;

/// Evaluate a permission request against a ruleset.
///
/// # Source
/// Ported from `packages/opencode/src/permission/evaluate.ts`.
pub fn evaluate(tool: &str, patterns: &[String], ruleset: &PermissionRuleset) -> EvaluatedPermission {
    for rule in ruleset {
        if matches_pattern(tool, &rule.tool) {
            if let Some(ref rule_patterns) = rule.patterns {
                if patterns.iter().any(|p| rule_patterns.iter().any(|rp| matches_pattern(p, rp))) {
                    return EvaluatedPermission {
                        action: rule.action.clone(),
                        reason: Some(format!("Matched rule for tool '{}'", rule.tool)),
                    };
                }
            } else {
                return EvaluatedPermission {
                    action: rule.action.clone(),
                    reason: Some(format!("Matched rule for tool '{}'", rule.tool)),
                };
            }
        }
    }

    // Default: ask for permission
    EvaluatedPermission {
        action: PermissionAction::Ask,
        reason: Some("No matching rule found".into()),
    }
}

/// Result of a permission evaluation.
#[derive(Debug, Clone)]
pub struct EvaluatedPermission {
    pub action: PermissionAction,
    pub reason: Option<String>,
}

/// Simple pattern matching (supports * wildcards).
fn matches_pattern(input: &str, pattern: &str) -> bool {
    if pattern == "*" {
        return true;
    }
    if !pattern.contains('*') {
        return input == pattern;
    }
    let parts: Vec<&str> = pattern.split('*').collect();
    let mut remaining = input;
    for (i, part) in parts.iter().enumerate() {
        if part.is_empty() {
            continue;
        }
        if i == 0 {
            if !remaining.starts_with(part) {
                return false;
            }
            remaining = &remaining[part.len()..];
        } else if i == parts.len() - 1 {
            if !remaining.ends_with(part) {
                return false;
            }
        } else {
            if let Some(pos) = remaining.find(part) {
                remaining = &remaining[pos + part.len()..];
            } else {
                return false;
            }
        }
    }
    true
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_exact_match() {
        let ruleset = vec![PermissionRule {
            tool: "shell".into(),
            action: PermissionAction::Allow,
            patterns: None,
        }];
        let result = evaluate("shell", &[], &ruleset);
        assert_eq!(result.action, PermissionAction::Allow);
    }

    #[test]
    fn test_wildcard_match() {
        let ruleset = vec![PermissionRule {
            tool: "*".into(),
            action: PermissionAction::Ask,
            patterns: None,
        }];
        let result = evaluate("any_tool", &[], &ruleset);
        assert_eq!(result.action, PermissionAction::Ask);
    }

    #[test]
    fn test_no_match_default_ask() {
        let ruleset = vec![];
        let result = evaluate("shell", &[], &ruleset);
        assert_eq!(result.action, PermissionAction::Ask);
    }
}
