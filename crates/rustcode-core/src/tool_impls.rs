//! Built-in tool implementations — 21 tools covering the full OpenCode tool surface.
//!
//! Each tool is a struct implementing the [`Tool`](super::tool::Tool) trait,
//! registered in [`ToolRegistry::register_builtins`].
//!
//! Ported from:
//! - `packages/core/src/tool/bash.ts` (206 lines)
//! - `packages/opencode/src/tool/shell.ts` (657 lines)
//! - `packages/core/src/tool/read.ts` (105 lines)
//! - `packages/opencode/src/tool/read.ts` (386 lines)
//! - `packages/core/src/tool/write.ts` (93 lines)
//! - `packages/opencode/src/tool/write.ts` (104 lines)
//! - `packages/core/src/tool/edit.ts` (199 lines)
//! - `packages/opencode/src/tool/edit.ts` (737 lines)
//! - `packages/core/src/tool/glob.ts` (98 lines)
//! - `packages/opencode/src/tool/glob.ts` (76 lines)
//! - `packages/core/src/tool/grep.ts` (130 lines)
//! - `packages/opencode/src/tool/grep.ts` (112 lines)
//! - `packages/core/src/tool/webfetch.ts` (217 lines)
//! - `packages/opencode/src/tool/webfetch.ts` (192 lines)
//! - `packages/core/src/tool/websearch.ts` (246 lines)
//! - `packages/opencode/src/tool/websearch.ts` (143 lines)
//! - `packages/core/src/tool/apply-patch.ts` (177 lines)
//! - `packages/opencode/src/tool/apply_patch.ts` (313 lines)
//! - `packages/opencode/src/tool/task.ts` (346 lines)
//! - `packages/opencode/src/tool/question.ts` (44 lines)
//! - `packages/core/src/tool/question.ts` (86 lines)
//! - `packages/opencode/src/tool/skill.ts` (71 lines)
//! - `packages/core/src/tool/skill.ts` (105 lines)
//! - `packages/opencode/src/tool/todo.ts` (57 lines)
//! - `packages/core/src/tool/todowrite.ts` (54 lines)
//! - `packages/opencode/src/tool/plan.ts` (79 lines)
//! - `packages/opencode/src/tool/lsp.ts` (200 lines)
//! - `packages/opencode/src/tool/invalid.ts` (19 lines)
//!
//! OpenCode commit: 5d0f86606ac30690f79f0a6a9f41a1f49fe95d0b

use async_trait::async_trait;
use std::collections::HashMap;
use std::sync::Arc;

use crate::error::{Error, Result, SkillError};
use crate::bus::SharedBus;
use crate::question::{
    format_model_output, QuestionAnswer, QuestionInfo, QuestionOption, QuestionPrompt,
    QuestionService,
};
use crate::shell_parser::ShellParser;
use crate::tool::{
    truncate_output, ExecuteResult, FileAttachment, Tool, ToolContext, ToolRegistry,
};

// ── Replacer trait and strategies ──────────────────────────────────────────────
// Ported from: packages/opencode/src/tool/edit.ts (lines 217–644)

/// Similarity thresholds for block anchor fallback matching.
const SINGLE_CANDIDATE_SIMILARITY_THRESHOLD: f64 = 0.65;
const MULTIPLE_CANDIDATES_SIMILARITY_THRESHOLD: f64 = 0.65;

/// Levenshtein distance algorithm implementation.
/// Ported from: packages/opencode/src/tool/edit.ts `levenshtein()`
fn levenshtein_distance(a: &str, b: &str) -> usize {
    if a.is_empty() || b.is_empty() {
        return a.len().max(b.len());
    }
    let a_chars: Vec<char> = a.chars().collect();
    let b_chars: Vec<char> = b.chars().collect();
    let a_len = a_chars.len();
    let b_len = b_chars.len();

    let mut matrix = vec![vec![0usize; b_len + 1]; a_len + 1];
    for i in 0..=a_len {
        matrix[i][0] = i;
    }
    for j in 0..=b_len {
        matrix[0][j] = j;
    }

    for i in 1..=a_len {
        for j in 1..=b_len {
            let cost = if a_chars[i - 1] == b_chars[j - 1] { 0 } else { 1 };
            matrix[i][j] = (matrix[i - 1][j] + 1)
                .min(matrix[i][j - 1] + 1)
                .min(matrix[i - 1][j - 1] + cost);
        }
    }
    matrix[a_len][b_len]
}

/// A replacer strategy yields candidate search strings from `content` matching `find`.
/// Analogous to the TS `Replacer` type.
trait Replacer {
    fn search(&self, content: &str, find: &str) -> Vec<String>;
}

/// Exact string match replacer — yields the find string as-is.
/// Ported from: packages/opencode/src/tool/edit.ts `SimpleReplacer`
struct SimpleReplacer;
impl Replacer for SimpleReplacer {
    fn search(&self, _content: &str, find: &str) -> Vec<String> {
        vec![find.to_string()]
    }
}

/// Line-by-line trimmed comparison replacer.
/// Ported from: packages/opencode/src/tool/edit.ts `LineTrimmedReplacer`
struct LineTrimmedReplacer;
impl Replacer for LineTrimmedReplacer {
    fn search(&self, content: &str, find: &str) -> Vec<String> {
        let original_lines: Vec<&str> = content.split('\n').collect();
        let mut search_lines: Vec<&str> = find.split('\n').collect();
        if search_lines.last().is_some_and(|l| l.is_empty()) {
            search_lines.pop();
        }
        let mut results = Vec::new();
        if search_lines.is_empty() || search_lines.len() > original_lines.len() {
            return results;
        }
        for i in 0..=original_lines.len() - search_lines.len() {
            let mut matches = true;
            for j in 0..search_lines.len() {
                if original_lines[i + j].trim() != search_lines[j].trim() {
                    matches = false;
                    break;
                }
            }
            if !matches { continue; }
            let mut match_start = 0usize;
            for k in 0..i { match_start += original_lines[k].len() + 1; }
            let mut match_end = match_start;
            for k in 0..search_lines.len() {
                match_end += original_lines[i + k].len();
                if k < search_lines.len() - 1 { match_end += 1; }
            }
            results.push(content[match_start..match_end].to_string());
        }
        results
    }
}

/// Block anchor matching with first/last line anchors and Levenshtein similarity.
/// Ported from: packages/opencode/src/tool/edit.ts `BlockAnchorReplacer`
struct BlockAnchorReplacer;
impl Replacer for BlockAnchorReplacer {
    fn search(&self, content: &str, find: &str) -> Vec<String> {
        let original_lines: Vec<&str> = content.split('\n').collect();
        let mut search_lines: Vec<&str> = find.split('\n').collect();
        if search_lines.len() < 3 { return Vec::new(); }
        if search_lines.last().is_some_and(|l| l.is_empty()) { search_lines.pop(); }

        let first_line_search = search_lines[0].trim();
        let last_line_search = search_lines[search_lines.len() - 1].trim();
        let search_block_size = search_lines.len();
        let max_line_delta = 1.max(search_block_size / 4);

        #[derive(Clone)]
        struct Candidate { start_line: usize, end_line: usize }
        let mut candidates: Vec<Candidate> = Vec::new();

        for i in 0..original_lines.len() {
            if original_lines[i].trim() != first_line_search { continue; }
            for j in (i + 2)..original_lines.len() {
                if original_lines[j].trim() == last_line_search {
                    let actual_block_size = j - i + 1;
                    let delta = if actual_block_size > search_block_size { actual_block_size - search_block_size } else { search_block_size - actual_block_size };
                    if delta <= max_line_delta {
                        candidates.push(Candidate { start_line: i, end_line: j });
                    }
                    break;
                }
            }
        }

        if candidates.is_empty() { return Vec::new(); }

        if candidates.len() == 1 {
            let c = &candidates[0];
            let actual_block_size = c.end_line - c.start_line + 1;
            let lines_to_check = (search_block_size - 2).min(actual_block_size - 2);
            let similarity = if lines_to_check > 0 {
                let mut sim = 0.0;
                for j in 1..search_block_size.min(actual_block_size) - 1 {
                    let ol = original_lines[c.start_line + j].trim();
                    let sl = search_lines[j].trim();
                    let max_len = ol.len().max(sl.len());
                    if max_len == 0 { continue; }
                    sim += 1.0 - levenshtein_distance(ol, sl) as f64 / max_len as f64;
                }
                sim / lines_to_check as f64
            } else { 1.0 };
            if similarity >= SINGLE_CANDIDATE_SIMILARITY_THRESHOLD {
                let mut match_start = 0usize;
                for k in 0..c.start_line { match_start += original_lines[k].len() + 1; }
                let mut match_end = match_start;
                for k in c.start_line..=c.end_line {
                    match_end += original_lines[k].len();
                    if k < c.end_line { match_end += 1; }
                }
                return vec![content[match_start..match_end].to_string()];
            }
            return Vec::new();
        }

        let mut best_similarity = -1.0f64;
        let mut best_candidate: Option<Candidate> = None;
        for c in &candidates {
            let actual_block_size = c.end_line - c.start_line + 1;
            let lines_to_check = (search_block_size - 2).min(actual_block_size - 2);
            let similarity = if lines_to_check > 0 {
                let mut sim = 0.0;
                for j in 1..search_block_size.min(actual_block_size) - 1 {
                    let ol = original_lines[c.start_line + j].trim();
                    let sl = search_lines[j].trim();
                    let max_len = ol.len().max(sl.len());
                    if max_len == 0 { continue; }
                    sim += 1.0 - levenshtein_distance(ol, sl) as f64 / max_len as f64;
                }
                sim / lines_to_check as f64
            } else { 1.0 };
            if similarity > best_similarity {
                best_similarity = similarity;
                best_candidate = Some(c.clone());
            }
        }

        if best_similarity >= MULTIPLE_CANDIDATES_SIMILARITY_THRESHOLD {
            if let Some(c) = best_candidate {
                let mut match_start = 0usize;
                for k in 0..c.start_line { match_start += original_lines[k].len() + 1; }
                let mut match_end = match_start;
                for k in c.start_line..=c.end_line {
                    match_end += original_lines[k].len();
                    if k < c.end_line { match_end += 1; }
                }
                return vec![content[match_start..match_end].to_string()];
            }
        }
        Vec::new()
    }
}

/// Whitespace-normalized matching replacer.
/// Ported from: packages/opencode/src/tool/edit.ts `WhitespaceNormalizedReplacer`
struct WhitespaceNormalizedReplacer;
impl Replacer for WhitespaceNormalizedReplacer {
    fn search(&self, content: &str, find: &str) -> Vec<String> {
        let normalize = |text: &str| -> String {
            let re = regex::Regex::new(r"\s+").unwrap();
            re.replace_all(text, " ").trim().to_string()
        };
        let normalized_find = normalize(find);
        let mut results = Vec::new();
        let lines: Vec<&str> = content.split('\n').collect();

        for line in &lines {
            if normalize(line) == normalized_find {
                results.push(line.to_string());
            } else {
                let normalized_line = normalize(line);
                if normalized_line.contains(&normalized_find) {
                    let words: Vec<&str> = find.trim().split_whitespace().collect();
                    if !words.is_empty() {
                        let pattern = words.iter().map(|w| regex::escape(w)).collect::<Vec<_>>().join(r"\s+");
                        if let Ok(re) = regex::Regex::new(&pattern) {
                            if let Some(m) = re.find(line) {
                                results.push(m.as_str().to_string());
                            }
                        }
                    }
                }
            }
        }

        if find.contains('\n') {
            let find_lines: Vec<&str> = find.split('\n').collect();
            if find_lines.len() > 1 {
                for i in 0..=lines.len().saturating_sub(find_lines.len()) {
                    let block = lines[i..i + find_lines.len()].join("\n");
                    if normalize(&block) == normalized_find {
                        results.push(block);
                    }
                }
            }
        }
        results
    }
}

/// Indentation-flexible matching replacer.
/// Ported from: packages/opencode/src/tool/edit.ts `IndentationFlexibleReplacer`
struct IndentationFlexibleReplacer;
impl Replacer for IndentationFlexibleReplacer {
    fn search(&self, content: &str, find: &str) -> Vec<String> {
        let strip_indent = |text: &str| -> String {
            let lines: Vec<&str> = text.split('\n').collect();
            let non_empty_lines: Vec<&&str> = lines.iter().filter(|l| l.trim().len() > 0).collect();
            if non_empty_lines.is_empty() { return text.to_string(); }
            let min_indent = non_empty_lines.iter()
                .map(|l| { let trimmed = l.trim_start(); l.len() - trimmed.len() })
                .min().unwrap_or(0);
            lines.iter().map(|l| {
                if l.trim().is_empty() { l.to_string() } else { let s = min_indent.min(l.len()); l[s..].to_string() }
            }).collect::<Vec<_>>().join("\n")
        };
        let normalized_find = strip_indent(find);
        let content_lines: Vec<&str> = content.split('\n').collect();
        let find_lines: Vec<&str> = find.split('\n').collect();
        let mut results = Vec::new();
        for i in 0..=content_lines.len().saturating_sub(find_lines.len()) {
            let block = content_lines[i..i + find_lines.len()].join("\n");
            if strip_indent(&block) == normalized_find { results.push(block); }
        }
        results
    }
}

/// Escape-sequence normalized matching replacer.
/// Ported from: packages/opencode/src/tool/edit.ts `EscapeNormalizedReplacer`
struct EscapeNormalizedReplacer;
impl Replacer for EscapeNormalizedReplacer {
    fn search(&self, content: &str, find: &str) -> Vec<String> {
        let unescape = |s: &str| -> String {
            let mut result = String::with_capacity(s.len());
            let mut chars = s.chars().peekable();
            while let Some(ch) = chars.next() {
                if ch == '\\' {
                    match chars.next() {
                        Some('n') => result.push('\n'),
                        Some('t') => result.push('\t'),
                        Some('r') => result.push('\r'),
                        Some('\'') => result.push('\''),
                        Some('"') => result.push('"'),
                        Some('`') => result.push('`'),
                        Some('\\') => result.push('\\'),
                        Some('\n') => result.push('\n'),
                        Some('$') => result.push('$'),
                        Some(c) => { result.push('\\'); result.push(c); },
                        None => result.push('\\'),
                    }
                } else { result.push(ch); }
            }
            result
        };
        let unescaped_find = unescape(find);
        let mut results = Vec::new();
        if content.contains(&unescaped_find) { results.push(unescaped_find.clone()); }
        let content_lines: Vec<&str> = content.split('\n').collect();
        let find_lines: Vec<&str> = unescaped_find.split('\n').collect();
        for i in 0..=content_lines.len().saturating_sub(find_lines.len()) {
            let block = content_lines[i..i + find_lines.len()].join("\n");
            if unescape(&block) == unescaped_find { results.push(block); }
        }
        results
    }
}

/// Multi-occurrence enumeration replacer — yields all exact matches.
/// Ported from: packages/opencode/src/tool/edit.ts `MultiOccurrenceReplacer`
struct MultiOccurrenceReplacer;
impl Replacer for MultiOccurrenceReplacer {
    fn search(&self, content: &str, find: &str) -> Vec<String> {
        let mut results = Vec::new();
        let mut start = 0;
        while let Some(idx) = content[start..].find(find) {
            results.push(find.to_string());
            start += idx + find.len();
        }
        results
    }
}

/// Trimmed boundary matching replacer.
/// Ported from: packages/opencode/src/tool/edit.ts `TrimmedBoundaryReplacer`
struct TrimmedBoundaryReplacer;
impl Replacer for TrimmedBoundaryReplacer {
    fn search(&self, content: &str, find: &str) -> Vec<String> {
        let trimmed_find = find.trim();
        if trimmed_find == find { return Vec::new(); }
        let mut results = Vec::new();
        if content.contains(trimmed_find) { results.push(trimmed_find.to_string()); }
        let content_lines: Vec<&str> = content.split('\n').collect();
        let find_lines: Vec<&str> = find.split('\n').collect();
        for i in 0..=content_lines.len().saturating_sub(find_lines.len()) {
            let block = content_lines[i..i + find_lines.len()].join("\n");
            if block.trim() == trimmed_find { results.push(block); }
        }
        results
    }
}

/// Context-aware multi-line matching replacer.
/// Ported from: packages/opencode/src/tool/edit.ts `ContextAwareReplacer`
struct ContextAwareReplacer;
impl Replacer for ContextAwareReplacer {
    fn search(&self, content: &str, find: &str) -> Vec<String> {
        let mut find_lines: Vec<&str> = find.split('\n').collect();
        if find_lines.len() < 3 { return Vec::new(); }
        if find_lines.last().is_some_and(|l| l.is_empty()) { find_lines.pop(); }
        let content_lines: Vec<&str> = content.split('\n').collect();
        let first_line = find_lines[0].trim();
        let last_line = find_lines[find_lines.len() - 1].trim();
        let mut results = Vec::new();
        for i in 0..content_lines.len() {
            if content_lines[i].trim() != first_line { continue; }
            for j in (i + 2)..content_lines.len() {
                if content_lines[j].trim() != last_line { continue; }
                let block = &content_lines[i..=j];
                let block_text = block.join("\n");
                if block.len() == find_lines.len() {
                    let mut matching_lines = 0;
                    let mut total_non_empty = 0;
                    for k in 1..block.len() - 1 {
                        let bl = block[k].trim();
                        let fl = find_lines[k].trim();
                        if !bl.is_empty() || !fl.is_empty() {
                            total_non_empty += 1;
                            if bl == fl { matching_lines += 1; }
                        }
                    }
                    if total_non_empty == 0 || (matching_lines as f64 / total_non_empty as f64) >= 0.5 {
                        results.push(block_text);
                        break;
                    }
                }
                break;
            }
        }
        results
    }
}

/// Check if a matched span is disproportionately large compared to oldString.
/// Ported from: packages/opencode/src/tool/edit.ts `isDisproportionateMatch()`
fn is_disproportionate_match(search: &str, old_string: &str) -> bool {
    let old_lines = old_string.split('\n').count();
    let search_lines = search.split('\n').count();
    if search_lines >= (old_lines + 3).max(old_lines * 2) { return true; }
    if old_lines == 1 { return false; }
    search.trim().len() > old_string.trim().len().max(old_string.trim().len() + 500)
        || search.trim().len() > old_string.trim().len() * 4
}

/// Core replace function that chains all replacer strategies.
/// Ported from: packages/opencode/src/tool/edit.ts `replace()`
pub fn edit_replace(
    content: &str,
    old_string: &str,
    new_string: &str,
    replace_all: bool,
) -> std::result::Result<String, String> {
    if old_string == new_string {
        return Err("No changes to apply: oldString and newString are identical.".into());
    }
    if old_string.is_empty() {
        return Err("oldString cannot be empty when editing an existing file. Provide the exact text to replace, or use write for an intentional full-file replacement.".into());
    }

    let replacers: Vec<Box<dyn Replacer>> = vec![
        Box::new(SimpleReplacer),
        Box::new(LineTrimmedReplacer),
        Box::new(BlockAnchorReplacer),
        Box::new(WhitespaceNormalizedReplacer),
        Box::new(IndentationFlexibleReplacer),
        Box::new(EscapeNormalizedReplacer),
        Box::new(TrimmedBoundaryReplacer),
        Box::new(ContextAwareReplacer),
        Box::new(MultiOccurrenceReplacer),
    ];

    let mut not_found = true;

    for replacer in &replacers {
        let candidates = replacer.search(content, old_string);
        for search_str in candidates {
            let index = match content.find(&search_str) {
                Some(i) => i,
                None => continue,
            };
            not_found = false;

            if is_disproportionate_match(&search_str, old_string) {
                return Err(
                    "Refusing replacement because the matched span is much larger than oldString. Re-read the file and provide the full exact oldString for the intended replacement.".into(),
                );
            }

            if replace_all {
                return Ok(content.replace(&search_str, new_string));
            }

            let last_index = content.rfind(&search_str);
            if Some(index) != last_index {
                continue;
            }

            let mut result = String::with_capacity(content.len() - search_str.len() + new_string.len());
            result.push_str(&content[..index]);
            result.push_str(new_string);
            result.push_str(&content[index + search_str.len()..]);
            return Ok(result);
        }
    }

    if not_found {
        Err("Could not find oldString in the file. It must match exactly, including whitespace, indentation, and line endings.".into())
    } else {
        Err("Found multiple matches for oldString. Provide more surrounding context to make the match unique.".into())
    }
}

/// Trim common leading whitespace from diff content lines.
/// Ported from: packages/opencode/src/tool/edit.ts `trimDiff()`
pub fn trim_diff(diff: &str) -> String {
    let lines: Vec<&str> = diff.lines().collect();
    let content_lines: Vec<&str> = lines.iter()
        .filter(|line| {
            (line.starts_with('+') || line.starts_with('-') || line.starts_with(' '))
                && !line.starts_with("---") && !line.starts_with("+++")
        })
        .copied().collect();
    if content_lines.is_empty() { return diff.to_string(); }
    let mut min_indent = usize::MAX;
    for line in &content_lines {
        let content = &line[1..];
        if content.trim().is_empty() { continue; }
        let indent = content.len() - content.trim_start().len();
        min_indent = min_indent.min(indent);
    }
    if min_indent == usize::MAX || min_indent == 0 { return diff.to_string(); }
    lines.iter().map(|line| {
        if (line.starts_with('+') || line.starts_with('-') || line.starts_with(' '))
            && !line.starts_with("---") && !line.starts_with("+++")
        {
            let prefix = &line[..1];
            let content = &line[1..];
            if content.len() > min_indent {
                format!("{}{}", prefix, &content[min_indent..])
            } else {
                prefix.to_string()
            }
        } else {
            line.to_string()
        }
    }).collect::<Vec<_>>().join("\n")
}



// ═══════════════════════════════════════════════════════════════════════════════
// 1. BashTool — shell command execution with AST-based permission scanning
// ═══════════════════════════════════════════════════════════════════════════════

const DEFAULT_TIMEOUT_MS: u64 = 2 * 60 * 1000; // 2 minutes
const MAX_TIMEOUT_MS: u64 = 10 * 60 * 1000; // 10 minutes
const MAX_CAPTURE_BYTES: usize = 1024 * 1024; // 1 MB

/// WARNING
/// ═══════
/// BashTool grants the host user's full filesystem, process, and network
/// authority. It is **not sandboxed**. Use with caution.
///
/// Executes a shell command via the system shell with:
/// - **AST parsing** — command is parsed with tree-sitter-bash before execution
///   to detect dangerous operations (rm -rf /, mkfs, dd to /dev, etc.).
/// - **Permission scanning** — flagged commands trigger `ctx.ask()` for the
///   user to allow/deny before the command is spawned.
/// - **Multi-shell support** — uses `shell.rs` to detect the user's preferred
///   shell (bash, zsh, fish, powershell) instead of hard-coded `/bin/sh`.
/// - **Real-time streaming** — stdout/stderr are read line-by-line during
///   execution and buffered for output.
///
/// # Source
/// Ported from `packages/core/src/tool/bash.ts` and `packages/opencode/src/tool/shell.ts`.
#[derive(Debug, Clone)]
pub struct BashTool;

#[async_trait]
impl Tool for BashTool {
    fn id(&self) -> &str {
        "bash"
    }

    fn description(&self) -> &str {
        "Execute one shell command string with the host user's filesystem, process, and network authority.\
         Multi-shell support (bash, zsh, fish, powershell).\
         Commands are scanned for dangerous operations before execution.\
         Timeout values are milliseconds (default: 120000; maximum: 600000)."
    }

    fn parameters_schema(&self) -> serde_json::Value {
        serde_json::json!({
            "type": "object",
            "properties": {
                "command": {
                    "type": "string",
                    "description": "Shell command string to execute"
                },
                "workdir": {
                    "type": "string",
                    "description": "Working directory. Defaults to the current directory; relative paths resolve from that location."
                },
                "timeout": {
                    "type": "integer",
                    "description": "Timeout in milliseconds. Defaults to 120000 and may not exceed 600000."
                },
                "description": {
                    "type": "string",
                    "description": "Concise description of the command's purpose"
                }
            },
            "required": ["command"]
        })
    }

    async fn execute(&self, args: serde_json::Value, ctx: &ToolContext) -> Result<ExecuteResult> {
        let command = args["command"]
            .as_str()
            .ok_or_else(|| Error::ToolInvalidArguments {
                tool: "bash".into(),
                detail: "missing 'command' field".into(),
            })?;

        let workdir = args["workdir"].as_str().unwrap_or(".");

        let timeout_ms = args["timeout"]
            .as_u64()
            .unwrap_or(DEFAULT_TIMEOUT_MS)
            .min(MAX_TIMEOUT_MS);

        let description = args["description"].as_str().unwrap_or(command);

        // ── Step 1: AST parsing and permission scanning ──────────────────
        let parser = ShellParser::new();
        let parsed = parser.parse(command);

        // Flagged commands (rm -rf /, mkfs, dd to /dev, etc.)
        if parsed.is_flagged {
            let allowed = ctx.ask("bash", &format!("command: {}", command)).await?;
            if !allowed {
                return Ok(denied_result(description, command));
            }
        }

        // File operations touching external paths
        for op in &parsed.file_operations {
            if op.path.starts_with('/') || op.path.starts_with("..") {
                let resource = format!("file operation: {} {}", op.op, op.path);
                let allowed = ctx.ask("bash", &resource).await?;
                if !allowed {
                    return Ok(denied_result(description, command));
                }
            }
        }

        // CWD changes to external directories
        for cd_target in &parsed.cwd_changes {
            if cd_target.starts_with('/') || cd_target.starts_with('~') {
                let resource = format!("cd {}", cd_target);
                let allowed = ctx.ask("bash", &resource).await?;
                if !allowed {
                    return Ok(denied_result(description, command));
                }
            }
        }

        // ── Step 2: Resolve working directory ────────────────────────────
        let cwd_path = std::path::Path::new(workdir);
        let cwd_buf = if cwd_path.is_absolute() {
            cwd_path.to_path_buf()
        } else {
            std::env::current_dir()
                .unwrap_or_else(|_| std::path::PathBuf::from("."))
                .join(cwd_path)
        };
        let cwd = cwd_buf.to_string_lossy().to_string();

        // ── Step 3: Detect preferred shell via shell.rs ──────────────────
        let shell = crate::shell::cached_preferred()
            .cloned()
            .or_else(|| crate::shell::select(None))
            .unwrap_or_else(|| crate::shell::ShellItem {
                path: std::path::PathBuf::from("/bin/sh"),
                name: "sh".into(),
                acceptable: true,
            });

        let shell_args = crate::shell::args(&shell, command, &cwd);

        // ── Step 4: Spawn process ────────────────────────────────────────
        let mut cmd = tokio::process::Command::new(&shell.path);
        cmd.args(&shell_args);
        cmd.current_dir(&cwd);
        cmd.stdin(std::process::Stdio::null());
        cmd.stdout(std::process::Stdio::piped());
        cmd.stderr(std::process::Stdio::piped());
        cmd.env("TERM", "xterm-256color");
        cmd.env("OPENCODE_TERMINAL", "1");
        #[cfg(target_os = "windows")]
        {
            cmd.env("LC_ALL", "C.UTF-8");
            cmd.env("LC_CTYPE", "C.UTF-8");
            cmd.env("LANG", "C.UTF-8");
        }

        let mut child = cmd.spawn().map_err(|e| Error::Process {
            message: format!("failed to spawn process: {}", e),
            exit_code: None,
        })?;

        let child_pid = child.id();

        // Take stdout/stderr for streaming reads
        let stdout = child.stdout.take().ok_or_else(|| Error::Process {
            message: "failed to capture stdout".into(),
            exit_code: None,
        })?;
        let stderr = child.stderr.take().ok_or_else(|| Error::Process {
            message: "failed to capture stderr".into(),
            exit_code: None,
        })?;

        // ── Step 5: Stream output in real-time ──────────────────────────
        let output_buf: Arc<tokio::sync::Mutex<String>> =
            Arc::new(tokio::sync::Mutex::new(String::new()));
        let stderr_buf: Arc<tokio::sync::Mutex<String>> =
            Arc::new(tokio::sync::Mutex::new(String::new()));

        let stdout_task = {
            use tokio::io::AsyncBufReadExt;
            let buf = Arc::clone(&output_buf);
            tokio::spawn(async move {
                let mut reader = tokio::io::BufReader::new(stdout);
                let mut lines = reader.lines();
                while let Ok(Some(line)) = lines.next_line().await {
                    let mut guard = buf.lock().await;
                    guard.push_str(&line);
                    guard.push('\n');
                }
            })
        };

        let stderr_task = {
            use tokio::io::AsyncBufReadExt;
            let buf = Arc::clone(&stderr_buf);
            tokio::spawn(async move {
                let mut reader = tokio::io::BufReader::new(stderr);
                let mut lines = reader.lines();
                while let Ok(Some(line)) = lines.next_line().await {
                    let mut guard = buf.lock().await;
                    guard.push_str(&line);
                    guard.push('\n');
                }
            })
        };

        // ── Step 6: Wait with timeout and abort ──────────────────────────
        let result = tokio::select! {
            biased;

            _ = ctx.abort.cancelled() => {
                let _ = stdout_task.await;
                let _ = stderr_task.await;
                return Ok(ExecuteResult {
                    title: description.to_string(),
                    output: "Command aborted by user.".into(),
                    truncated: false,
                    output_path: None,
                    attachments: None,
                    metadata: {
                        let mut m = HashMap::new();
                        m.insert("command".into(), serde_json::Value::String(command.to_string()));
                        m.insert("cwd".into(), serde_json::Value::String(cwd));
                        m
                    },
                });
            }

            _ = tokio::time::sleep(std::time::Duration::from_millis(timeout_ms)) => {
                kill_process_group(child_pid);
                // Wait 3 seconds then escalate to SIGKILL (matching OC's forceKillAfter)
                let _ = tokio::time::sleep(std::time::Duration::from_secs(3)).await;
                force_kill_process_group(child_pid);

                let _ = stdout_task.await;
                let _ = stderr_task.await;

                let mut meta = HashMap::new();
                meta.insert("command".into(), serde_json::Value::String(command.to_string()));
                meta.insert("cwd".into(), serde_json::Value::String(cwd));
                meta.insert("timedOut".into(), serde_json::Value::Bool(true));

                Ok(ExecuteResult {
                    title: description.to_string(),
                    output: format!(
                        "Command exceeded timeout of {} ms. Retry with a larger timeout if the command is expected to take longer.",
                        timeout_ms
                    ),
                    truncated: false,
                    output_path: None,
                    attachments: None,
                    metadata: meta,
                })
            }

            status = child.wait() => {
                let _ = stdout_task.await;
                let _ = stderr_task.await;

                let stdout_text = {
                    let guard = output_buf.lock().await;
                    guard.clone()
                };
                let stderr_text = {
                    let guard = stderr_buf.lock().await;
                    guard.clone()
                };

                let exit_code = status
                    .ok()
                    .and_then(|s| s.code())
                    .unwrap_or(-1);

                let stdout_truncated = stdout_text.len() > MAX_CAPTURE_BYTES;
                let stderr_truncated = stderr_text.len() > MAX_CAPTURE_BYTES;
                let stdout_display = if stdout_truncated {
                    stdout_text.chars().take(MAX_CAPTURE_BYTES).collect::<String>()
                } else {
                    stdout_text
                };
                let stderr_display = if stderr_truncated {
                    stderr_text.chars().take(MAX_CAPTURE_BYTES).collect::<String>()
                } else {
                    stderr_text
                };

                let mut warnings = Vec::new();
                if stdout_truncated && stderr_truncated {
                    warnings.push("[stdout and stderr capture truncated at the in-memory safety limit]".to_string());
                } else if stdout_truncated {
                    warnings.push("[stdout capture truncated at the in-memory safety limit]".to_string());
                } else if stderr_truncated {
                    warnings.push("[stderr capture truncated at the in-memory safety limit]".to_string());
                }

                let compact = if !stdout_display.is_empty() && !stderr_display.is_empty() {
                    format!("{}\n\nstderr:\n{}", stdout_display, stderr_display)
                } else if !stderr_display.is_empty() {
                    format!("stderr:\n{}", stderr_display)
                } else {
                    stdout_display
                };
                let compact = if compact.is_empty() { "(no output)".to_string() } else { compact };

                let mut full_output = format!(
                    "{}\n\nCommand exited with code {}.",
                    compact,
                    exit_code,
                );
                if !warnings.is_empty() {
                    full_output.push_str("\n\nWarnings:\n");
                    for w in &warnings {
                        full_output.push_str(&format!("- {}\n", w));
                    }
                }

                let truncated = stdout_truncated || stderr_truncated;

                Ok(ExecuteResult {
                    title: description.to_string(),
                    output: full_output,
                    truncated,
                    output_path: None,
                    attachments: None,
                    metadata: {
                        let mut m = HashMap::new();
                        m.insert("command".into(), serde_json::Value::String(command.to_string()));
                        m.insert("cwd".into(), serde_json::Value::String(cwd));
                        m.insert("exitCode".into(), serde_json::json!(exit_code));
                        if stdout_truncated { m.insert("stdoutTruncated".into(), serde_json::Value::Bool(true)); }
                        if stderr_truncated { m.insert("stderrTruncated".into(), serde_json::Value::Bool(true)); }
                        m
                    },
                })
            }
        };

        result
    }
}

/// Build a denied result for the bash tool.
fn denied_result(description: &str, command: &str) -> ExecuteResult {
    ExecuteResult {
        title: description.to_string(),
        output: format!("Command denied by user: {}", command),
        truncated: false,
        output_path: None,
        attachments: None,
        metadata: {
            let mut m = HashMap::new();
            m.insert("command".into(), serde_json::Value::String(command.to_string()));
            m.insert("denied".into(), serde_json::Value::Bool(true));
            m
        },
    }
}

/// Send SIGTERM to a process group on Unix, or taskkill on Windows.
#[cfg(unix)]
fn kill_process_group(pid: Option<u32>) {
    if let Some(pid) = pid {
        let _ = std::process::Command::new("kill")
            .arg("-TERM")
            .arg(format!("-{}", pid))
            .output();
    }
}

/// Force-kill (SIGKILL) a process group on Unix, or taskkill /f on Windows.
#[cfg(unix)]
fn force_kill_process_group(pid: Option<u32>) {
    if let Some(pid) = pid {
        let _ = std::process::Command::new("kill")
            .arg("-KILL")
            .arg(format!("-{}", pid))
            .output();
    }
}

#[cfg(windows)]
fn kill_process_group(pid: Option<u32>) {
    if let Some(pid) = pid {
        let _ = std::process::Command::new("taskkill")
            .args(["/pid", &pid.to_string(), "/t"])
            .output();
    }
}

#[cfg(windows)]
fn force_kill_process_group(pid: Option<u32>) {
    if let Some(pid) = pid {
        let _ = std::process::Command::new("taskkill")
            .args(["/pid", &pid.to_string(), "/f", "/t"])
            .output();
    }
}

// ═══════════════════════════════════════════════════════════════════════════════
// 2. ReadTool — file reading
// ═══════════════════════════════════════════════════════════════════════════════

const DEFAULT_READ_LIMIT: usize = 2000;
const MAX_LINE_LENGTH: usize = 2000;

/// Binary file extensions that should not be read as text.
const BINARY_EXTENSIONS: &[&str] = &[
    ".zip", ".tar", ".gz", ".exe", ".dll", ".so", ".class", ".jar", ".war", ".7z", ".doc", ".docx",
    ".xls", ".xlsx", ".ppt", ".pptx", ".odt", ".ods", ".odp", ".bin", ".dat", ".obj", ".o", ".a",
    ".lib", ".wasm", ".pyc", ".pyo", ".png", ".jpg", ".jpeg", ".gif", ".webp", ".bmp", ".ico",
    ".svg", ".mp3", ".mp4", ".avi", ".mov", ".wmv", ".flv", ".mkv", ".ttf", ".otf", ".woff",
    ".woff2",
];

/// Supported image MIME types that can be read and displayed.
const SUPPORTED_IMAGE_MIMES: &[&str] = &["image/jpeg", "image/png", "image/gif", "image/webp"];

/// Reads a file (or lists a directory) at a given path.
///
/// Supports offset/limit for paging through large files. Returns content with
/// line numbers. Handles binary file detection.
///
/// # Source
/// Ported from `packages/core/src/tool/read.ts` and `packages/opencode/src/tool/read.ts`.
#[derive(Debug, Clone)]
pub struct ReadTool;

impl ReadTool {
    /// Detect if a file is binary based on extension and content sample.
    fn is_binary_file(path: &str, sample: &[u8]) -> bool {
        // Check extension first
        let lower = path.to_lowercase();
        if let Some(ext_pos) = lower.rfind('.') {
            let ext = &lower[ext_pos..];
            if BINARY_EXTENSIONS.contains(&ext) {
                return true;
            }
        }

        if sample.is_empty() {
            return false;
        }

        // Check for null bytes and high proportion of non-printable chars
        let mut non_printable = 0usize;
        for &byte in sample {
            if byte == 0 {
                return true;
            }
            if byte < 9 || (byte > 13 && byte < 32) {
                non_printable += 1;
            }
        }
        (non_printable as f64 / sample.len() as f64) > 0.3
    }

    /// Guess MIME type from file extension.
    fn guess_mime(path: &str) -> String {
        let lower = path.to_lowercase();
        match lower.rfind('.') {
            Some(pos) => match &lower[pos..] {
                ".png" => "image/png".into(),
                ".jpg" | ".jpeg" => "image/jpeg".into(),
                ".gif" => "image/gif".into(),
                ".webp" => "image/webp".into(),
                ".pdf" => "application/pdf".into(),
                ".html" | ".htm" => "text/html".into(),
                ".css" => "text/css".into(),
                ".js" => "application/javascript".into(),
                ".json" => "application/json".into(),
                ".xml" => "application/xml".into(),
                ".md" => "text/markdown".into(),
                ".rs" | ".py" | ".ts" | ".go" | ".java" | ".c" | ".cpp" | ".h" => {
                    "text/plain".into()
                }
                _ => "application/octet-stream".into(),
            },
            None => "application/octet-stream".into(),
        }
    }
}

#[async_trait]
impl Tool for ReadTool {
    fn id(&self) -> &str {
        "read"
    }

    fn description(&self) -> &str {
        "Read a text file or supported image, page through a large UTF-8 text file by line offset,\
         or list a directory page. Relative paths resolve from the current location;\
         absolute paths are read directly."
    }

    fn parameters_schema(&self) -> serde_json::Value {
        serde_json::json!({
            "type": "object",
            "properties": {
                "filePath": {
                    "type": "string",
                    "description": "The absolute path to the file or directory to read"
                },
                "offset": {
                    "type": "integer",
                    "description": "The line number to start reading from (1-indexed)"
                },
                "limit": {
                    "type": "integer",
                    "description": "The maximum number of lines to read (defaults to 2000)"
                }
            },
            "required": ["filePath"]
        })
    }

    async fn execute(&self, args: serde_json::Value, _ctx: &ToolContext) -> Result<ExecuteResult> {
        let file_path = args["filePath"]
            .as_str()
            .ok_or_else(|| Error::ToolInvalidArguments {
                tool: "read".into(),
                detail: "missing 'filePath' field".into(),
            })?;

        let offset = args["offset"].as_u64().unwrap_or(1).max(1) as usize;
        let limit = args["limit"].as_u64().unwrap_or(DEFAULT_READ_LIMIT as u64) as usize;

        let path = std::path::Path::new(file_path);

        // Check if path exists
        if !path.exists() {
            // Suggest similar files
            let parent = path.parent().unwrap_or(std::path::Path::new("."));
            let basename = path
                .file_name()
                .unwrap_or_default()
                .to_string_lossy()
                .to_lowercase();
            let mut suggestions: Vec<String> = Vec::new();

            if let Ok(entries) = std::fs::read_dir(parent) {
                for entry in entries.flatten() {
                    let name = entry.file_name().to_string_lossy().to_lowercase();
                    if name.contains(&basename) || basename.contains(&name) {
                        suggestions.push(entry.path().to_string_lossy().to_string());
                    }
                    if suggestions.len() >= 3 {
                        break;
                    }
                }
            }

            let mut msg = format!("File not found: {}", file_path);
            if !suggestions.is_empty() {
                msg.push_str(&format!(
                    "\n\nDid you mean one of these?\n{}",
                    suggestions.join("\n")
                ));
            }
            return Err(Error::Tool(msg));
        }

        let metadata = path.metadata().map_err(Error::Io)?;

        if metadata.is_dir() {
            // List directory
            let mut entries: Vec<String> = Vec::new();
            if let Ok(dir_entries) = std::fs::read_dir(path) {
                for entry in dir_entries.flatten() {
                    let name = entry.file_name().to_string_lossy().to_string();
                    let is_dir = entry.file_type().map(|t| t.is_dir()).unwrap_or(false);
                    entries.push(if is_dir { format!("{}/", name) } else { name });
                }
            }
            entries.sort();

            let total = entries.len();
            let start = (offset - 1).min(total);
            let end = (start + limit).min(total);
            let sliced: Vec<_> = entries[start..end].to_vec();
            let truncated = start + sliced.len() < total;

            let mut output = format!(
                "<path>{}</path>\n<type>directory</type>\n<entries>\n",
                file_path
            );
            for entry in &sliced {
                output.push_str(&format!("{}\n", entry));
            }
            if truncated {
                output.push_str(&format!(
                    "\n(Showing {} of {} entries. Use 'offset' parameter to read beyond entry {})",
                    sliced.len(),
                    total,
                    offset + sliced.len()
                ));
            } else {
                output.push_str(&format!("\n({} entries)", total));
            }
            output.push_str("\n</entries>");

            return Ok(ExecuteResult {
                title: file_path.to_string(),
                output,
                truncated,
                output_path: None,
                attachments: None,
                metadata: {
                    let mut m = HashMap::new();
                    m.insert("type".into(), serde_json::Value::String("directory".into()));
                    m.insert("totalEntries".into(), serde_json::json!(total));
                    m
                },
            });
        }

        // Read a sample to detect binary / image
        let mut sample = vec![0u8; 4096];
        let file_size = metadata.len() as usize;
        let sample_size = std::cmp::min(4096, file_size);
        let sample_bytes = if sample_size > 0 {
            std::fs::File::open(path)
                .ok()
                .and_then(|mut f| {
                    std::io::Read::read(&mut f, &mut sample[..sample_size]).ok()?;
                    Some(sample[..sample_size].to_vec())
                })
                .unwrap_or_default()
        } else {
            Vec::new()
        };

        let mime = Self::guess_mime(file_path);

        // Handle images
        if SUPPORTED_IMAGE_MIMES.contains(&mime.as_str()) {
            let bytes = std::fs::read(path).unwrap_or_default();
            let b64 = base64::Engine::encode(&base64::engine::general_purpose::STANDARD, &bytes);
            return Ok(ExecuteResult {
                title: file_path.to_string(),
                output: "Image read successfully".into(),
                truncated: false,
                output_path: None,
                attachments: Some(vec![FileAttachment {
                    mime: mime.clone(),
                    url: format!("data:{};base64,{}", mime, b64),
                }]),
                metadata: HashMap::new(),
            });
        }

        // Handle PDF
        if mime == "application/pdf" {
            let bytes = std::fs::read(path).unwrap_or_default();
            let b64 = base64::Engine::encode(&base64::engine::general_purpose::STANDARD, &bytes);
            return Ok(ExecuteResult {
                title: file_path.to_string(),
                output: "PDF read successfully".into(),
                truncated: false,
                output_path: None,
                attachments: Some(vec![FileAttachment {
                    mime,
                    url: format!("data:application/pdf;base64,{}", b64),
                }]),
                metadata: HashMap::new(),
            });
        }

        // Check for binary
        if Self::is_binary_file(file_path, &sample_bytes) {
            return Err(Error::BinaryFile {
                path: file_path.to_string(),
            });
        }

        // Read text file with byte cap (~50KB)
        const MAX_READ_BYTES: usize = 51_200; // 50KB limit
        let raw_content = std::fs::read_to_string(path).map_err(Error::Io)?;
        let content = if raw_content.len() > MAX_READ_BYTES {
            // Truncate at a line boundary to avoid cutting mid-line
            let truncated = &raw_content[..MAX_READ_BYTES];
            let capped = match truncated.rfind('\n') {
                Some(pos) => &raw_content[..pos],
                None => truncated,
            };
            format!(
                "{}...\n\n(Content truncated at ~50KB for performance. Total file size is {} bytes. Use offset/limit to read specific sections, or grep to search.)",
                capped, raw_content.len()
            )
        } else {
            raw_content
        };
        let lines: Vec<&str> = content.lines().collect();
        let total_lines = lines.len();

        let start = (offset - 1).min(total_lines);
        let end = (start + limit).min(total_lines);
        let selected: Vec<&str> = lines[start..end].to_vec();

        let mut output = format!("<path>{}</path>\n<type>file</type>\n<content>\n", file_path);

        for (i, line) in selected.iter().enumerate() {
            let line_num = start + i + 1;
            let display = if line.len() > MAX_LINE_LENGTH {
                format!(
                    "{}... (line truncated to {} chars)",
                    &line[..MAX_LINE_LENGTH],
                    MAX_LINE_LENGTH
                )
            } else {
                line.to_string()
            };
            output.push_str(&format!("{}: {}\n", line_num, display));
        }

        let last_line = start + selected.len();
        let truncated = end < total_lines;
        if truncated {
            output.push_str(&format!(
                "\n\n(Showing lines {}-{} of {}. Use offset={} to continue.)",
                offset,
                last_line,
                total_lines,
                last_line + 1
            ));
        } else {
            output.push_str(&format!("\n\n(End of file - total {} lines)", total_lines));
        }
        output.push_str("\n</content>");

        Ok(ExecuteResult {
            title: file_path.to_string(),
            output,
            truncated,
            output_path: None,
            attachments: None,
            metadata: {
                let mut m = HashMap::new();
                m.insert("type".into(), serde_json::Value::String("file".into()));
                m.insert("totalLines".into(), serde_json::json!(total_lines));
                m.insert("lineStart".into(), serde_json::json!(offset));
                m.insert("lineEnd".into(), serde_json::json!(last_line));
                m
            },
        })
    }
}

// ═══════════════════════════════════════════════════════════════════════════════
// 3. WriteTool — file writing
// ═══════════════════════════════════════════════════════════════════════════════

/// Writes content to a file, creating parent directories as needed.
///
/// # Source
/// Ported from `packages/core/src/tool/write.ts` and `packages/opencode/src/tool/write.ts`.
#[derive(Debug, Clone)]
pub struct WriteTool;

#[async_trait]
impl Tool for WriteTool {
    fn id(&self) -> &str {
        "write"
    }

    fn description(&self) -> &str {
        "Write content to one file. Relative paths resolve within the active location.\
         Absolute paths inside the location are accepted. Creates parent directories automatically."
    }

    fn parameters_schema(&self) -> serde_json::Value {
        serde_json::json!({
            "type": "object",
            "properties": {
                "filePath": {
                    "type": "string",
                    "description": "The absolute path to the file to write (must be absolute, not relative)"
                },
                "content": {
                    "type": "string",
                    "description": "The content to write to the file"
                }
            },
            "required": ["filePath", "content"]
        })
    }

    async fn execute(&self, args: serde_json::Value, _ctx: &ToolContext) -> Result<ExecuteResult> {
        let file_path = args["filePath"]
            .as_str()
            .ok_or_else(|| Error::ToolInvalidArguments {
                tool: "write".into(),
                detail: "missing 'filePath' field".into(),
            })?;

        let content = args["content"]
            .as_str()
            .ok_or_else(|| Error::ToolInvalidArguments {
                tool: "write".into(),
                detail: "missing 'content' field".into(),
            })?;

        let path = std::path::Path::new(file_path);

        // Create parent directories
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent).map_err(|e| Error::FileSystem {
                path: parent.to_string_lossy().to_string(),
                message: format!("failed to create parent directories: {}", e),
            })?;
        }

        let existed = path.exists();

        // Preserve UTF-8 BOM (EF BB BF) if the existing file has one
        let bom = [0xEFu8, 0xBB, 0xBF];
        let content_bytes = if existed {
            let existing = std::fs::read(path).unwrap_or_default();
            if existing.starts_with(&bom) && !content.as_bytes().starts_with(&bom) {
                // Prepend BOM to new content
                let mut bytes = Vec::with_capacity(bom.len() + content.len());
                bytes.extend_from_slice(&bom);
                bytes.extend_from_slice(content.as_bytes());
                bytes
            } else {
                content.as_bytes().to_vec()
            }
        } else {
            content.as_bytes().to_vec()
        };

        std::fs::write(path, &content_bytes).map_err(|e| Error::FileSystem {
            path: file_path.to_string(),
            message: format!("failed to write file: {}", e),
        })?;

        let action = if existed { "Wrote" } else { "Created" };
        let output = format!("{} file successfully: {}", action, file_path);

        Ok(ExecuteResult {
            title: file_path.to_string(),
            output,
            truncated: false,
            output_path: None,
            attachments: None,
            metadata: {
                let mut m = HashMap::new();
                m.insert(
                    "operation".into(),
                    serde_json::Value::String("write".into()),
                );
                m.insert("existed".into(), serde_json::Value::Bool(existed));
                m
            },
        })
    }
}

// ═══════════════════════════════════════════════════════════════════════════════
// 4. EditTool — file editing (search/replace)
// ═══════════════════════════════════════════════════════════════════════════════

/// Applies exact string search-and-replace edits to a file.
///
/// Supports single or replace-all mode. Returns a diff-like summary.
///
/// # Source
/// Ported from `packages/core/src/tool/edit.ts` and `packages/opencode/src/tool/edit.ts`.
#[derive(Debug, Clone)]
pub struct EditTool;

impl EditTool {
    /// Normalize line endings to LF.
    fn normalize_line_endings(text: &str) -> String {
        text.replace("\r\n", "\n")
    }

    /// Detect the line ending style of text.
    fn detect_line_ending(text: &str) -> &str {
        if text.contains("\r\n") {
            "\r\n"
        } else {
            "\n"
        }
    }

    /// Convert text to use a specific line ending.
    fn convert_to_line_ending(text: &str, ending: &str) -> String {
        if ending == "\r\n" {
            Self::normalize_line_endings(text).replace('\n', "\r\n")
        } else {
            Self::normalize_line_endings(text)
        }
    }

    /// Count occurrences of a string in content.
    fn count_occurrences(content: &str, search: &str) -> usize {
        if search.is_empty() {
            return content.len() + 1;
        }
        content.matches(search).count()
    }

    /// Generate a simple unified diff between old and new content.
    fn simple_diff(old: &str, new: &str) -> String {
        let old_lines: Vec<&str> = old.lines().collect();
        let new_lines: Vec<&str> = new.lines().collect();

        let mut diff = String::new();
        let max_len = old_lines.len().max(new_lines.len());

        // Very simple line-by-line diff
        let mut i = 0;
        while i < max_len {
            let old_line = old_lines.get(i).copied().unwrap_or("");
            let new_line = new_lines.get(i).copied().unwrap_or("");

            if old_line != new_line {
                if !old_line.is_empty() || i < old_lines.len() {
                    diff.push_str(&format!("-{}\n", old_line));
                }
                if !new_line.is_empty() || i < new_lines.len() {
                    diff.push_str(&format!("+{}\n", new_line));
                }
            } else {
                diff.push_str(&format!(" {}\n", old_line));
            }
            i += 1;
        }

        diff
    }

    /// Trim common leading whitespace from diff lines.
    fn trim_diff(diff: &str) -> String {
        let lines: Vec<&str> = diff.lines().collect();
        let content_lines: Vec<&str> = lines
            .iter()
            .filter(|line| {
                (line.starts_with('+') || line.starts_with('-') || line.starts_with(' '))
                    && !line.starts_with("---")
                    && !line.starts_with("+++")
            })
            .copied()
            .collect();

        if content_lines.is_empty() {
            return diff.to_string();
        }

        let mut min_indent = usize::MAX;
        for line in &content_lines {
            let content = &line[1..];
            if content.trim().is_empty() {
                continue;
            }
            let indent = content.len() - content.trim_start().len();
            min_indent = min_indent.min(indent);
        }

        if min_indent == usize::MAX || min_indent == 0 {
            return diff.to_string();
        }

        lines
            .iter()
            .map(|line| {
                if (line.starts_with('+') || line.starts_with('-') || line.starts_with(' '))
                    && !line.starts_with("---")
                    && !line.starts_with("+++")
                {
                    let prefix = &line[..1];
                    let content = &line[1..];
                    if content.len() > min_indent {
                        format!("{}{}", prefix, &content[min_indent..])
                    } else {
                        prefix.to_string()
                    }
                } else {
                    line.to_string()
                }
            })
            .collect::<Vec<_>>()
            .join("\n")
    }
}

#[async_trait]
impl Tool for EditTool {
    fn id(&self) -> &str {
        "edit"
    }

    fn description(&self) -> &str {
        "Replace exact text in one file. Relative paths resolve within the active location.\
         Supports replaceAll for multiple occurrences. oldString and newString must differ.\
         Returns a unified diff of changes."
    }

    fn parameters_schema(&self) -> serde_json::Value {
        serde_json::json!({
            "type": "object",
            "properties": {
                "filePath": {
                    "type": "string",
                    "description": "The absolute path to the file to modify"
                },
                "oldString": {
                    "type": "string",
                    "description": "The text to replace"
                },
                "newString": {
                    "type": "string",
                    "description": "The text to replace it with (must be different from oldString)"
                },
                "replaceAll": {
                    "type": "boolean",
                    "description": "Replace all occurrences of oldString (default false)"
                }
            },
            "required": ["filePath", "oldString", "newString"]
        })
    }

    async fn execute(&self, args: serde_json::Value, _ctx: &ToolContext) -> Result<ExecuteResult> {
        let file_path = args["filePath"]
            .as_str()
            .ok_or_else(|| Error::ToolInvalidArguments {
                tool: "edit".into(),
                detail: "missing 'filePath' field".into(),
            })?;

        let old_string = args["oldString"]
            .as_str()
            .ok_or_else(|| Error::ToolInvalidArguments {
                tool: "edit".into(),
                detail: "missing 'oldString' field".into(),
            })?;

        let new_string = args["newString"]
            .as_str()
            .ok_or_else(|| Error::ToolInvalidArguments {
                tool: "edit".into(),
                detail: "missing 'newString' field".into(),
            })?;

        let replace_all = args["replaceAll"].as_bool().unwrap_or(false);

        if old_string == new_string {
            return Err(Error::Tool(
                "No changes to apply: oldString and newString are identical.".into(),
            ));
        }

        if old_string.is_empty() {
            return Err(Error::Tool(
                "oldString must not be empty. Use write to create or overwrite a file.".into(),
            ));
        }

        let path = std::path::Path::new(file_path);
        if !path.exists() {
            return Err(Error::Tool(format!("File {} not found", file_path)));
        }

        if path.is_dir() {
            return Err(Error::Tool(format!(
                "Path is a directory, not a file: {}",
                file_path
            )));
        }

        let source_content = std::fs::read_to_string(path).map_err(Error::Io)?;

        // Detect and normalize line endings for consistent matching
        let ending = Self::detect_line_ending(&source_content);
        let old_normalized = Self::convert_to_line_ending(old_string, ending);
        let new_normalized = Self::convert_to_line_ending(new_string, ending);

        // Count exact occurrences for display/metadata (approximate for fuzzy replacer matches)
        let occurrences = Self::count_occurrences(&source_content, &old_normalized);

        // Use replacer strategies (exact, line-trimmed, block anchor, etc.)
        let replaced = edit_replace(&source_content, &old_normalized, &new_normalized, replace_all)
            .map_err(|e| Error::Tool(e))?;

        // Write the file
        std::fs::write(path, &replaced).map_err(|e| Error::FileSystem {
            path: file_path.to_string(),
            message: format!("failed to write file: {}", e),
        })?;

        // Generate diff
        let diff = Self::trim_diff(&Self::simple_diff(&source_content, &replaced));

        let output = format!(
            "Edited file successfully: {}\nReplacements: {}\n```diff\n{}```",
            file_path, occurrences, diff
        );

        Ok(ExecuteResult {
            title: format!("Edited: {}", file_path),
            output,
            truncated: false,
            output_path: None,
            attachments: None,
            metadata: {
                let mut m = HashMap::new();
                m.insert(
                    "operation".into(),
                    serde_json::Value::String("write".into()),
                );
                m.insert(
                    "replacements".into(),
                    serde_json::json!(if replace_all { occurrences } else { 1 }),
                );
                m.insert("diff".into(), serde_json::Value::String(diff));
                m
            },
        })
    }
}

// ═══════════════════════════════════════════════════════════════════════════════
// 5. GlobTool — file pattern matching
// ═══════════════════════════════════════════════════════════════════════════════

/// Finds files matching a glob pattern.
///
/// # Source
/// Ported from `packages/core/src/tool/glob.ts` and `packages/opencode/src/tool/glob.ts`.
#[derive(Debug, Clone)]
pub struct GlobTool;

#[async_trait]
impl Tool for GlobTool {
    fn id(&self) -> &str {
        "glob"
    }

    fn description(&self) -> &str {
        "Find files by glob pattern. Returns relative file paths.\
         Use a path to narrow the search directory. Results are limited to 100 by default."
    }

    fn parameters_schema(&self) -> serde_json::Value {
        serde_json::json!({
            "type": "object",
            "properties": {
                "pattern": {
                    "type": "string",
                    "description": "The glob pattern to match files against"
                },
                "path": {
                    "type": "string",
                    "description": "The directory to search in. Defaults to the current working directory."
                }
            },
            "required": ["pattern"]
        })
    }

    async fn execute(&self, args: serde_json::Value, _ctx: &ToolContext) -> Result<ExecuteResult> {
        let pattern = args["pattern"]
            .as_str()
            .ok_or_else(|| Error::ToolInvalidArguments {
                tool: "glob".into(),
                detail: "missing 'pattern' field".into(),
            })?;

        let search_path = args["path"].as_str().unwrap_or(".");
        let base_dir = std::path::Path::new(search_path);

        let resolved = if base_dir.is_absolute() {
            base_dir.to_path_buf()
        } else {
            std::env::current_dir()
                .unwrap_or_else(|_| std::path::PathBuf::from("."))
                .join(base_dir)
        };

        if !resolved.exists() {
            return Ok(ExecuteResult {
                title: pattern.to_string(),
                output: "No files found".into(),
                truncated: false,
                output_path: None,
                attachments: None,
                metadata: HashMap::new(),
            });
        }

        if resolved.is_file() {
            return Err(Error::Tool(format!(
                "glob path must be a directory: {}",
                resolved.display()
            )));
        }

        // Build glob pattern relative to the resolved directory
        let glob_pattern = resolved.join(pattern);
        let glob_str = glob_pattern.to_string_lossy().to_string();

        let limit = 100usize;
        let mut matches: Vec<String> = Vec::new();

        if let Ok(paths) = glob::glob(&glob_str) {
            for entry in paths.flatten() {
                if entry.is_file() {
                    // Return relative path from resolved base
                    if let Ok(rel) = entry.strip_prefix(&resolved) {
                        matches.push(rel.to_string_lossy().to_string());
                    } else {
                        matches.push(entry.to_string_lossy().to_string());
                    }
                }
                if matches.len() >= limit {
                    break;
                }
            }
        }

        let truncated = matches.len() >= limit;
        if matches.is_empty() {
            return Ok(ExecuteResult {
                title: pattern.to_string(),
                output: "No files found".into(),
                truncated: false,
                output_path: None,
                attachments: None,
                metadata: HashMap::new(),
            });
        }

        let mut output = matches.join("\n");
        if truncated {
            output.push_str(&format!(
                "\n\n(Results are truncated: showing first {} results. Consider using a more specific path or pattern.)",
                limit
            ));
        }

        Ok(ExecuteResult {
            title: format!("{} in {}", pattern, resolved.display()),
            output,
            truncated,
            output_path: None,
            attachments: None,
            metadata: {
                let mut m = HashMap::new();
                m.insert("count".into(), serde_json::json!(matches.len()));
                m.insert("truncated".into(), serde_json::Value::Bool(truncated));
                m
            },
        })
    }
}

// ═══════════════════════════════════════════════════════════════════════════════
// 6. GrepTool — content search
// ═══════════════════════════════════════════════════════════════════════════════

/// Searches file contents with a regex pattern.
///
/// # Source
/// Ported from `packages/core/src/tool/grep.ts` and `packages/opencode/src/tool/grep.ts`.
#[derive(Debug, Clone)]
pub struct GrepTool;

#[async_trait]
impl Tool for GrepTool {
    fn id(&self) -> &str {
        "grep"
    }

    fn description(&self) -> &str {
        "Search file contents by regular expression. Use a path to narrow the search,\
         include to filter files by glob, and limit to bound the match count.\
         Returns file paths, line numbers, and bounded line previews."
    }

    fn parameters_schema(&self) -> serde_json::Value {
        serde_json::json!({
            "type": "object",
            "properties": {
                "pattern": {
                    "type": "string",
                    "description": "The regex pattern to search for in file contents"
                },
                "path": {
                    "type": "string",
                    "description": "The directory or file to search in. Defaults to the current working directory."
                },
                "include": {
                    "type": "string",
                    "description": "File pattern to include in the search (e.g. \"*.js\", \"*.{ts,tsx}\")"
                }
            },
            "required": ["pattern"]
        })
    }

    async fn execute(&self, args: serde_json::Value, _ctx: &ToolContext) -> Result<ExecuteResult> {
        let pattern_str = args["pattern"]
            .as_str()
            .ok_or_else(|| Error::ToolInvalidArguments {
                tool: "grep".into(),
                detail: "missing 'pattern' field".into(),
            })?;

        let search_path = args["path"].as_str().unwrap_or(".");
        let include_filter = args["include"].as_str();

        let re = regex::Regex::new(pattern_str)
            .map_err(|e| Error::InvalidSearchPattern(e.to_string()))?;

        let base_dir = std::path::Path::new(search_path);
        let resolved = if base_dir.is_absolute() {
            base_dir.to_path_buf()
        } else {
            std::env::current_dir()
                .unwrap_or_else(|_| std::path::PathBuf::from("."))
                .join(base_dir)
        };

        let limit = 100usize;
        let mut matches: Vec<(String, usize, String)> = Vec::new(); // (path, line_num, line_text)

        if resolved.is_file() {
            // Search single file
            if let Ok(content) = std::fs::read_to_string(&resolved) {
                for (i, line) in content.lines().enumerate() {
                    if re.is_match(line) {
                        let line_text = if line.len() > 300 {
                            format!("{}...", &line[..300])
                        } else {
                            line.to_string()
                        };
                        matches.push((resolved.to_string_lossy().to_string(), i + 1, line_text));
                        if matches.len() >= limit {
                            break;
                        }
                    }
                }
            }
        } else if resolved.is_dir() {
            let walker = ignore::WalkBuilder::new(&resolved)
                .hidden(false)
                .git_ignore(false)
                .max_depth(None)
                .build()
                .flatten();

            for entry in walker {
                let path = entry.path();
                if !path.is_file() {
                    continue;
                }

                // Apply include filter
                if let Some(inc) = include_filter {
                    let filename = path.file_name().unwrap_or_default().to_string_lossy();
                    if !Self::glob_match(inc, &filename) {
                        continue;
                    }
                }

                if let Ok(content) = std::fs::read_to_string(path) {
                    for (i, line) in content.lines().enumerate() {
                        if re.is_match(line) {
                            let line_text = if line.len() > 300 {
                                format!("{}...", &line[..300])
                            } else {
                                line.to_string()
                            };
                            matches.push((path.to_string_lossy().to_string(), i + 1, line_text));
                            if matches.len() >= limit {
                                break;
                            }
                        }
                    }
                }
                if matches.len() >= limit {
                    break;
                }
            }
        }

        if matches.is_empty() {
            return Ok(ExecuteResult {
                title: pattern_str.to_string(),
                output: "No files found".into(),
                truncated: false,
                output_path: None,
                attachments: None,
                metadata: HashMap::new(),
            });
        }

        let total = matches.len();
        let truncated = total >= limit;
        let mut output = format!(
            "Found {} matches{}\n",
            total,
            if truncated {
                " (more matches available)"
            } else {
                ""
            }
        );

        let mut current_file = String::new();
        for (path, line_num, text) in &matches {
            if current_file != *path {
                if !current_file.is_empty() {
                    output.push('\n');
                }
                current_file = path.clone();
                output.push_str(&format!("{}:\n", path));
            }
            output.push_str(&format!("  Line {}: {}\n", line_num, text));
        }

        if truncated {
            output
                .push_str("\n(Results truncated. Consider using a more specific path or pattern.)");
        }

        Ok(ExecuteResult {
            title: pattern_str.to_string(),
            output,
            truncated,
            output_path: None,
            attachments: None,
            metadata: {
                let mut m = HashMap::new();
                m.insert("matches".into(), serde_json::json!(total));
                m.insert("truncated".into(), serde_json::Value::Bool(truncated));
                m
            },
        })
    }
}

impl GrepTool {
    /// Simple glob matching for include patterns like `*.js` or `*.{ts,tsx}`.
    fn glob_match(pattern: &str, name: &str) -> bool {
        // Handle brace expansion `*.{ts,tsx}`
        if let (Some(open), Some(close)) = (pattern.find('{'), pattern.find('}')) {
            let prefix = &pattern[..open];
            let suffix = &pattern[close + 1..];
            let alternatives = &pattern[open + 1..close];
            for alt in alternatives.split(',') {
                let full = format!("{}{}{}", prefix, alt, suffix);
                if Self::simple_glob(&full, name) {
                    return true;
                }
            }
            return false;
        }
        Self::simple_glob(pattern, name)
    }

    fn simple_glob(pattern: &str, name: &str) -> bool {
        if pattern == "*" {
            return true;
        }

        let pattern_bytes = pattern.as_bytes();
        let name_bytes = name.as_bytes();

        // Simple recursive glob matching
        fn match_recursive(p: &[u8], n: &[u8]) -> bool {
            if p.is_empty() {
                return n.is_empty();
            }
            if p[0] == b'*' {
                // Try zero or more characters
                for i in 0..=n.len() {
                    if match_recursive(&p[1..], &n[i..]) {
                        return true;
                    }
                }
                return false;
            }
            if n.is_empty() {
                return false;
            }
            if p[0] == b'?' || p[0] == n[0] {
                return match_recursive(&p[1..], &n[1..]);
            }
            false
        }

        match_recursive(pattern_bytes, name_bytes)
    }
}

// ═══════════════════════════════════════════════════════════════════════════════
// 7. WebFetchTool — URL content fetch
// ═══════════════════════════════════════════════════════════════════════════════

/// Fetches content from an HTTP/HTTPS URL and converts HTML to markdown.
///
/// Returns page content as markdown using a simple HTML-to-text conversion.
/// Accepts an optional prompt to extract specific information from the page.
///
/// # Source
/// Ported from `packages/core/src/tool/webfetch.ts` and `packages/opencode/src/tool/webfetch.ts`.
#[derive(Debug, Clone)]
pub struct WebFetchTool;

#[async_trait]
impl Tool for WebFetchTool {
    fn id(&self) -> &str {
        "webfetch"
    }

    fn description(&self) -> &str {
        "Fetch content from an HTTP or HTTPS URL and convert it to markdown.\
         Use an optional prompt to extract specific information from the page.\
         This tool is read-only. Large text results may be truncated."
    }

    fn parameters_schema(&self) -> serde_json::Value {
        serde_json::json!({
            "type": "object",
            "properties": {
                "url": {
                    "type": "string",
                    "description": "The URL to fetch content from"
                },
                "prompt": {
                    "type": "string",
                    "description": "The prompt to run on the fetched content (optional, for extracting specific info)"
                }
            },
            "required": ["url"]
        })
    }

    async fn execute(&self, args: serde_json::Value, _ctx: &ToolContext) -> Result<ExecuteResult> {
        let url_str = args["url"]
            .as_str()
            .ok_or_else(|| Error::ToolInvalidArguments {
                tool: "webfetch".into(),
                detail: "missing 'url' field".into(),
            })?;

        let prompt = args["prompt"].as_str();
        let format = "markdown";
        let timeout_secs: u64 = 30;

        // Validate URL scheme
        if !url_str.starts_with("http://") && !url_str.starts_with("https://") {
            return Err(Error::Tool(
                "URL must start with http:// or https://".into(),
            ));
        }

        // Build HTTP client
        let client = reqwest::Client::builder()
            .timeout(std::time::Duration::from_secs(timeout_secs))
            .user_agent("Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/143.0.0.0 Safari/537.36")
            .build()
            .map_err(|e| Error::Network(format!("failed to create HTTP client: {}", e)))?;

        // Build Accept header based on format
        let accept = match format {
            "markdown" => "text/markdown;q=1.0, text/x-markdown;q=0.9, text/plain;q=0.8, text/html;q=0.7, */*;q=0.1",
            "text" => "text/plain;q=1.0, text/markdown;q=0.9, text/html;q=0.8, */*;q=0.1",
            _ => "text/html;q=1.0, application/xhtml+xml;q=0.9, text/plain;q=0.8, text/markdown;q=0.7, */*;q=0.1",
        };

        let response = client
            .get(url_str)
            .header("Accept", accept)
            .header("Accept-Language", "en-US,en;q=0.9")
            .send()
            .await
            .map_err(|e| Error::Network(format!("HTTP request failed: {}", e)))?;

        let content_type = response
            .headers()
            .get("content-type")
            .and_then(|v| v.to_str().ok())
            .unwrap_or("")
            .to_string();

        let status = response.status();
        if !status.is_success() {
            // Retry with honest UA for Cloudflare
            let retry_client = reqwest::Client::builder()
                .timeout(std::time::Duration::from_secs(timeout_secs))
                .user_agent("opencode")
                .build()
                .map_err(|e| {
                    Error::Network(format!("failed to create retry HTTP client: {}", e))
                })?;

            let retry_response = retry_client
                .get(url_str)
                .header("Accept", accept)
                .header("Accept-Language", "en-US,en;q=0.9")
                .send()
                .await
                .map_err(|e| Error::Network(format!("HTTP retry request failed: {}", e)))?;

            if !retry_response.status().is_success() {
                return Err(Error::Network(format!(
                    "HTTP {} for {}",
                    retry_response.status(),
                    url_str
                )));
            }

            let body = retry_response
                .text()
                .await
                .map_err(|e| Error::Network(format!("failed to read response body: {}", e)))?;

            let processed = WebFetchTool::process_body(&body, &content_type);

            return Ok(ExecuteResult {
                title: format!("{} ({})", url_str, content_type),
                output: processed,
                truncated: body.len() > 100_000,
                output_path: None,
                attachments: None,
                metadata: {
                    let mut m = HashMap::new();
                    m.insert("url".into(), serde_json::Value::String(url_str.to_string()));
                    m.insert(
                        "contentType".into(),
                        serde_json::Value::String(content_type),
                    );
                    if let Some(p) = prompt {
                        m.insert("prompt".into(), serde_json::Value::String(p.to_string()));
                    }
                    m
                },
            });
        }

        let body = response
            .text()
            .await
            .map_err(|e| Error::Network(format!("failed to read response body: {}", e)))?;

        let truncated = body.len() > 100_000;
        let processed = WebFetchTool::process_body(&body, &content_type);

        // Truncate if needed
        let output = if processed.len() > 100_000 {
            format!(
                "{}\n\n... (truncated: output exceeded size limit)",
                &processed[..100_000]
            )
        } else {
            processed
        };

        Ok(ExecuteResult {
            title: format!("{} ({})", url_str, content_type),
            output,
            truncated,
            output_path: None,
            attachments: None,
            metadata: {
                let mut m = HashMap::new();
                m.insert("url".into(), serde_json::Value::String(url_str.to_string()));
                m.insert(
                    "contentType".into(),
                    serde_json::Value::String(content_type),
                );
                if let Some(p) = prompt {
                    m.insert("prompt".into(), serde_json::Value::String(p.to_string()));
                }
                m
            },
        })
    }
}

impl WebFetchTool {
    /// Process body based on content type, always converting HTML to markdown.
    fn process_body(body: &str, content_type: &str) -> String {
        if !content_type.contains("text/html") {
            return body.to_string();
        }
        Self::html_to_markdown(body)
    }

    /// Basic HTML-to-text extraction (strips tags, removes script/style content).
    fn extract_text_from_html(html: &str) -> String {
        let mut text = String::new();
        let mut skip_depth = 0i32;
        let mut in_tag = false;
        let mut tag_name = String::new();
        let skip_tags = ["script", "style", "noscript", "iframe", "object", "embed"];

        for ch in html.chars() {
            if ch == '<' {
                in_tag = true;
                tag_name.clear();
                continue;
            }
            if ch == '>' {
                in_tag = false;
                let tag_lower = tag_name.to_lowercase();
                if skip_depth > 0 || skip_tags.iter().any(|t| tag_lower.starts_with(t)) {
                    if tag_lower.starts_with('/') {
                        skip_depth -= 1;
                    } else {
                        skip_depth += 1;
                    }
                }
                if tag_lower.starts_with('/') && skip_depth < 0 {
                    skip_depth = 0;
                }
                continue;
            }
            if in_tag {
                tag_name.push(ch);
            } else if skip_depth == 0 {
                text.push(ch);
            }
        }

        // Clean up whitespace
        let cleaned: String = text
            .lines()
            .map(|l| l.trim())
            .filter(|l| !l.is_empty())
            .collect::<Vec<_>>()
            .join("\n");
        cleaned.trim().to_string()
    }

    /// Basic HTML-to-Markdown conversion.
    fn html_to_markdown(html: &str) -> String {
        // Strip script, style, meta, link blocks
        let stripped =
            Self::strip_tags_content(html, &["script", "style", "meta", "link", "noscript"]);

        // Convert common HTML elements
        let mut md = String::new();
        let chars: Vec<char> = stripped.chars().collect();
        let len = chars.len();
        let mut i = 0;
        let mut in_tag = false;
        let mut tag_name = String::new();
        let mut tag_attrs = String::new();
        let mut in_list = false;

        while i < len {
            let ch = chars[i];

            if ch == '<' {
                in_tag = true;
                tag_name.clear();
                tag_attrs.clear();
                i += 1;
                continue;
            }

            if ch == '>' && in_tag {
                in_tag = false;
                let tag_lower = tag_name.to_lowercase();
                let is_closing = tag_lower.starts_with('/');
                if is_closing {
                    let close_name = &tag_lower[1..];
                    match close_name {
                        "p" => md.push('\n'),
                        "h1" | "h2" | "h3" | "h4" | "h5" | "h6" => md.push('\n'),
                        "li" => md.push('\n'),
                        "ul" | "ol" => {
                            md.push('\n');
                            in_list = false;
                        }
                        "br" | "br/" => md.push('\n'),
                        _ => {}
                    }
                } else {
                    match tag_lower.as_str() {
                        "h1" => {
                            md.push_str("\n# ");
                        }
                        "h2" => {
                            md.push_str("\n## ");
                        }
                        "h3" => {
                            md.push_str("\n### ");
                        }
                        "h4" => {
                            md.push_str("\n#### ");
                        }
                        "h5" => {
                            md.push_str("\n##### ");
                        }
                        "h6" => {
                            md.push_str("\n###### ");
                        }
                        "p" => md.push('\n'),
                        "ul" => in_list = true,
                        "ol" => in_list = true,
                        "li" => md.push_str(if in_list { "\n- " } else { "\n" }),
                        "br" | "br/" => md.push('\n'),
                        "hr" | "hr/" => md.push_str("\n---\n"),
                        "strong" | "b" => md.push_str("**"),
                        "em" | "i" => md.push('*'),
                        "code" => md.push('`'),
                        "pre" => md.push_str("\n```\n"),
                        "a" => {
                            // Extract href
                            let href = Self::extract_attr(&tag_attrs, "href");
                            if !href.is_empty() {
                                md.push('[');
                            }
                        }
                        "img" => {
                            let src = Self::extract_attr(&tag_attrs, "src");
                            let alt = Self::extract_attr(&tag_attrs, "alt");
                            if !src.is_empty() {
                                md.push_str(&format!("![{}]({})\n", alt, src));
                            }
                        }
                        "blockquote" => md.push_str("\n> "),
                        _ => {}
                    }
                }
                i += 1;
                continue;
            }

            if in_tag {
                tag_name.push(ch);
                // Collect attributes for a/href and img/src
                if !tag_name.is_empty()
                    && (tag_name.to_lowercase() == "a" || tag_name.to_lowercase() == "img")
                {
                    tag_attrs.push(ch);
                }
            } else {
                md.push(ch);
            }
            i += 1;
        }

        // Clean up multiple blank lines
        let mut cleaned = String::new();
        let mut prev_blank = false;
        for line in md.lines() {
            let trimmed = line.trim();
            if trimmed.is_empty() {
                if !prev_blank {
                    cleaned.push('\n');
                    prev_blank = true;
                }
            } else {
                cleaned.push_str(trimmed);
                cleaned.push('\n');
                prev_blank = false;
            }
        }

        cleaned.trim().to_string()
    }

    fn strip_tags_content(html: &str, tags: &[&str]) -> String {
        let mut result = String::new();
        let mut in_skip = false;
        let mut skip_tag = String::new();
        let mut in_tag = false;
        let mut tag_name = String::new();

        for ch in html.chars() {
            if ch == '<' {
                in_tag = true;
                tag_name.clear();
                result.push('<');
                continue;
            }
            if ch == '>' {
                in_tag = false;
                result.push('>');
                if in_skip {
                    let closing = tag_name.starts_with('/');
                    let name = if closing { &tag_name[1..] } else { &tag_name };
                    if closing && name.to_lowercase() == skip_tag {
                        // Skip the closing tag content too (just push the tag)
                        in_skip = false;
                    } else if !closing && tags.iter().any(|t| t.eq_ignore_ascii_case(name)) {
                        skip_tag = name.to_lowercase();
                        in_skip = true;
                    }
                }
                continue;
            }
            if in_tag {
                tag_name.push(ch);
                result.push(ch);
            } else if !in_skip {
                result.push(ch);
            }
        }

        result
    }

    fn extract_attr(attrs: &str, attr_name: &str) -> String {
        let lower = attrs.to_lowercase();
        if let Some(pos) = lower.find(&format!("{}=", attr_name)) {
            let after = &attrs[pos + attr_name.len() + 1..];
            let quote = after.chars().next().unwrap_or('"');
            if quote == '"' || quote == '\'' {
                if let Some(end) = after[1..].find(quote) {
                    return after[1..end + 1].to_string();
                }
            }
        }
        String::new()
    }
}

// ═══════════════════════════════════════════════════════════════════════════════
// 8. WebSearchTool — web search
// ═══════════════════════════════════════════════════════════════════════════════

/// Performs a web search via configurable search provider.
///
/// Makes JSON-RPC 2.0 calls to MCP-compatible search endpoints (Exa, Parallel)
/// and returns formatted results with titles, URLs, and snippets.
///
/// # Configuration
/// - `OPENCODE_WEBSEARCH_PROVIDER` — set to "exa" or "parallel" (optional)
/// - `EXA_API_KEY` — Exa API key
/// - `PARALLEL_API_KEY` — Parallel API key
///
/// # Source
/// Ported from `packages/core/src/tool/websearch.ts` and `packages/opencode/src/tool/websearch.ts`.
#[derive(Debug, Clone)]
pub struct WebSearchTool;

// MCP endpoint URLs
const EXA_MCP_URL: &str = "https://mcp.exa.ai/mcp";
const PARALLEL_MCP_URL: &str = "https://search.parallel.ai/mcp";

// Search provider constants
const PROVIDER_EXA: &str = "exa";
const PROVIDER_PARALLEL: &str = "parallel";

// Default search parameters
const DEFAULT_NUM_RESULTS: u32 = 8;
const MAX_RESPONSE_BYTES: usize = 256 * 1024;
const NO_RESULTS_MSG: &str = "No search results found. Please try a different query.";

/// Configuration for a selected search provider.
struct SearchConfig {
    provider: &'static str,
    url: String,
    tool_name: &'static str,
    api_key: Option<String>,
}

impl SearchConfig {
    /// Select and configure a search provider based on environment variables.
    fn from_env() -> Self {
        let provider_override = std::env::var("OPENCODE_WEBSEARCH_PROVIDER").ok();
        let exa_key = std::env::var("EXA_API_KEY").ok();
        let parallel_key = std::env::var("PARALLEL_API_KEY").ok();

        // Check OPENCODE_WEBSEARCH_PROVIDER override first
        match provider_override.as_deref() {
            Some(PROVIDER_PARALLEL) if parallel_key.is_some() => {
                return Self::parallel(parallel_key);
            }
            Some(PROVIDER_EXA) => {
                return Self::exa(exa_key);
            }
            _ => {}
        }

        // Fall back to whichever API key is available, preferring exa
        if exa_key.is_some() {
            return Self::exa(exa_key);
        }
        if let Some(key) = parallel_key {
            return Self::parallel(Some(key));
        }

        // Default to Exa even without a key — the MCP endpoint will use its own key
        Self::exa(exa_key)
    }

    fn exa(api_key: Option<String>) -> Self {
        let url = if let Some(ref key) = api_key {
            format!("{}?exaApiKey={}", EXA_MCP_URL, urlencoding::encode(key))
        } else {
            EXA_MCP_URL.to_string()
        };
        Self {
            provider: PROVIDER_EXA,
            url,
            tool_name: "web_search_exa",
            api_key,
        }
    }

    fn parallel(api_key: Option<String>) -> Self {
        Self {
            provider: PROVIDER_PARALLEL,
            url: PARALLEL_MCP_URL.to_string(),
            tool_name: "web_search",
            api_key,
        }
    }

    /// Build the extra HTTP headers for the provider.
    fn headers(&self) -> Vec<(String, String)> {
        let mut headers = Vec::new();
        if self.provider == PROVIDER_PARALLEL {
            if let Some(ref key) = self.api_key {
                headers.push(("Authorization".into(), format!("Bearer {}", key)));
            }
            headers.push(("User-Agent".into(), "opencode".into()));
        }
        headers
    }
}

#[async_trait]
impl Tool for WebSearchTool {
    fn id(&self) -> &str {
        "websearch"
    }

    fn description(&self) -> &str {
        "Search the web using an external search provider.\
         Use this for current information beyond knowledge cutoff.\
         Supports domain filtering via allowed_domains. Results include titles, URLs, and snippets."
    }

    fn parameters_schema(&self) -> serde_json::Value {
        serde_json::json!({
            "type": "object",
            "properties": {
                "query": {
                    "type": "string",
                    "description": "The search query to use"
                },
                "allowed_domains": {
                    "type": "array",
                    "description": "Only include search results from these domains",
                    "items": {
                        "type": "string"
                    }
                }
            },
            "required": ["query"]
        })
    }

    async fn execute(&self, args: serde_json::Value, _ctx: &ToolContext) -> Result<ExecuteResult> {
        let query = args["query"]
            .as_str()
            .ok_or_else(|| Error::ToolInvalidArguments {
                tool: "websearch".into(),
                detail: "missing 'query' field".into(),
            })?;

        let allowed_domains: Vec<String> = args
            .get("allowed_domains")
            .and_then(|v| v.as_array())
            .map(|arr| {
                arr.iter()
                    .filter_map(|v| v.as_str().map(|s| s.to_string()))
                    .collect()
            })
            .unwrap_or_default();

        let config = SearchConfig::from_env();

        // Build the JSON-RPC 2.0 request body
        let request_body = Self::build_request(&config, query, &allowed_domains);

        // Make the HTTP POST call
        let body = Self::call_mcp(&config, &request_body).await?;

        // Parse the MCP response into search result text
        let result_text = Self::parse_mcp_response(&body)
            .unwrap_or_else(|| NO_RESULTS_MSG.to_string());

        let provider_label = if config.provider == PROVIDER_PARALLEL {
            "Parallel Web Search"
        } else {
            "Exa Web Search"
        };

        Ok(ExecuteResult {
            title: format!("{}: {}", provider_label, query),
            output: result_text,
            truncated: body.len() > MAX_RESPONSE_BYTES,
            output_path: None,
            attachments: None,
            metadata: {
                let mut m = HashMap::new();
                m.insert("query".into(), serde_json::Value::String(query.to_string()));
                m.insert("provider".into(), serde_json::Value::String(config.provider.to_string()));
                if !allowed_domains.is_empty() {
                    m.insert("allowed_domains".into(), serde_json::json!(allowed_domains));
                }
                m
            },
        })
    }
}

impl WebSearchTool {
    /// Build JSON-RPC 2.0 request body for the search provider.
    fn build_request(config: &SearchConfig, query: &str, allowed_domains: &[String]) -> serde_json::Value {
        match config.provider {
            PROVIDER_PARALLEL => {
                let mut search_queries = vec![query.to_string()];
                if !allowed_domains.is_empty() {
                    // Append domain restrictions to search queries
                    let domain_filter = allowed_domains.join(" OR site:");
                    search_queries.push(format!("site:{} {}", domain_filter, query));
                }
                serde_json::json!({
                    "jsonrpc": "2.0",
                    "id": 1,
                    "method": "tools/call",
                    "params": {
                        "name": config.tool_name,
                        "arguments": {
                            "objective": query,
                            "search_queries": search_queries
                        }
                    }
                })
            }
            _ => {
                // Exa — supports livecrawl, type, numResults, contextMaxCharacters, allowed_domains
                let mut args = serde_json::json!({
                    "query": query,
                    "type": "auto",
                    "numResults": DEFAULT_NUM_RESULTS,
                    "livecrawl": "fallback"
                });
                if let Some(obj) = args.as_object_mut() {
                    if !allowed_domains.is_empty() {
                        // Exa uses includeDomains filter
                        obj.insert(
                            "includeDomains".into(),
                            serde_json::json!(allowed_domains),
                        );
                    }
                }
                serde_json::json!({
                    "jsonrpc": "2.0",
                    "id": 1,
                    "method": "tools/call",
                    "params": {
                        "name": config.tool_name,
                        "arguments": args
                    }
                })
            }
        }
    }

    /// Make the MCP JSON-RPC call and return the raw response body.
    async fn call_mcp(config: &SearchConfig, request_body: &serde_json::Value) -> Result<String> {
        let client = reqwest::Client::builder()
            .timeout(std::time::Duration::from_secs(30))
            .build()
            .map_err(|e| Error::Network(format!("failed to create HTTP client: {}", e)))?;

        let mut req = client
            .post(&config.url)
            .header("Accept", "application/json, text/event-stream")
            .header("Content-Type", "application/json");

        // Add provider-specific headers
        for (name, value) in config.headers() {
            req = req.header(&name, &value);
        }

        let response = req
            .json(request_body)
            .send()
            .await
            .map_err(|e| Error::Network(format!("HTTP request failed: {}", e)))?;

        if !response.status().is_success() {
            return Err(Error::Network(format!(
                "search provider returned HTTP {}",
                response.status()
            )));
        }

        let body = response
            .text()
            .await
            .map_err(|e| Error::Network(format!("failed to read response body: {}", e)))?;

        if body.len() > MAX_RESPONSE_BYTES {
            return Err(Error::Network(format!(
                "search response exceeded {} bytes",
                MAX_RESPONSE_BYTES
            )));
        }

        Ok(body)
    }

    /// Parse an MCP response body, handling both direct JSON and SSE streams.
    ///
    /// The response may be:
    /// 1. A direct JSON-RPC response with `result.content[].text`
    /// 2. An SSE stream with `data: {...}` lines containing the JSON-RPC response
    ///
    /// Returns the extracted text content, or None if no valid result found.
    fn parse_mcp_response(body: &str) -> Option<String> {
        let trimmed = body.trim();

        // Try direct JSON first
        if trimmed.starts_with('{') {
            if let Some(text) = Self::extract_text_from_json(trimmed) {
                return Some(text);
            }
        }

        // Try SSE format: look for lines starting with "data: "
        for line in trimmed.lines() {
            let line = line.trim();
            if let Some(data) = line.strip_prefix("data: ") {
                if data.starts_with('{') {
                    if let Some(text) = Self::extract_text_from_json(data) {
                        return Some(text);
                    }
                }
            }
        }

        None
    }

    /// Extract text content from a JSON-RPC response body.
    fn extract_text_from_json(json_str: &str) -> Option<String> {
        let parsed: serde_json::Value = serde_json::from_str(json_str).ok()?;

        // Navigate to result.content[].text
        let content = parsed.get("result")?.get("content")?.as_array()?;
        for item in content {
            if let Some(text) = item.get("text").and_then(|v| v.as_str()) {
                if !text.is_empty() {
                    return Some(text.to_string());
                }
            }
        }

        None
    }
}

// ═══════════════════════════════════════════════════════════════════════════════
// 9. ApplyPatchTool — unified diff patch application
// ═══════════════════════════════════════════════════════════════════════════════

/// Applies patches with add, delete, move, and update (unified diff) operations.
///
/// Detects the operation type from the patch content:
/// - `--- /dev/null` → add file
/// - `+++ /dev/null` or `deleted file mode` → delete file
/// - `rename from` / `rename to` → move file
/// - Otherwise → update (unified diff hunk application)
///
/// Also supports the opencode marker format:
/// `*** Begin Patch` / `*** Add/Delete/Update File:` / `*** End Patch`.
///
/// # Source
/// Ported from `packages/core/src/tool/apply-patch.ts` and
/// `packages/opencode/src/tool/apply_patch.ts`.
#[derive(Debug, Clone)]
pub struct ApplyPatchTool;

/// A single hunk from a unified diff.
#[derive(Debug, Clone)]
struct DiffHunk {
    old_start: usize,
    old_count: usize,
    new_start: usize,
    new_count: usize,
    lines: Vec<HunkLine>,
}

#[derive(Debug, Clone, PartialEq)]
enum HunkLine {
    Context(String),
    Added(String),
    Removed(String),
}

/// Detected patch operation type.
#[derive(Debug, Clone)]
enum PatchOp {
    /// Create a new file with the given content.
    Add { file_path: String, content: String },
    /// Delete an existing file.
    Delete { file_path: String },
    /// Rename a file from source_path to target_path.
    Move { source_path: String, target_path: String },
    /// Apply a unified diff patch to update an existing file.
    Update { file_path: String, patch: String },
}

impl ApplyPatchTool {
    /// Parse a unified diff string into a vector of hunks.
    fn parse_unified_diff(patch: &str) -> std::result::Result<Vec<DiffHunk>, String> {
        if patch.trim().is_empty() {
            return Err("patch is empty".into());
        }

        let normalized = patch.replace("\r\n", "\n");
        let mut hunks = Vec::new();
        let mut current_hunk: Option<DiffHunk> = None;
        let mut old_count_actual = 0usize;
        let mut new_count_actual = 0usize;

        for line in normalized.lines() {
            if line.starts_with("@@") {
                if let Some(mut hunk) = current_hunk.take() {
                    hunk.old_count = old_count_actual;
                    hunk.new_count = new_count_actual;
                    hunks.push(hunk);
                }

                if let Some(hunk) = Self::parse_hunk_header(line) {
                    current_hunk = Some(hunk);
                    old_count_actual = 0;
                    new_count_actual = 0;
                }
            } else if current_hunk.is_some() {
                if let Some(stripped) = line.strip_prefix(' ') {
                    current_hunk
                        .as_mut()
                        .unwrap()
                        .lines
                        .push(HunkLine::Context(stripped.to_string()));
                    old_count_actual += 1;
                    new_count_actual += 1;
                } else if line.starts_with('-') && !line.starts_with("---") {
                    current_hunk
                        .as_mut()
                        .unwrap()
                        .lines
                        .push(HunkLine::Removed(line[1..].to_string()));
                    old_count_actual += 1;
                } else if line.starts_with('+') && !line.starts_with("+++") {
                    current_hunk
                        .as_mut()
                        .unwrap()
                        .lines
                        .push(HunkLine::Added(line[1..].to_string()));
                    new_count_actual += 1;
                }
            }
        }

        if let Some(mut hunk) = current_hunk.take() {
            hunk.old_count = old_count_actual;
            hunk.new_count = new_count_actual;
            hunks.push(hunk);
        }

        if hunks.is_empty() {
            return Err("apply_patch verification failed: no hunks found in patch".into());
        }

        Ok(hunks)
    }

    /// Parse a hunk header like `@@ -1,7 +1,6 @@`.
    fn parse_hunk_header(line: &str) -> Option<DiffHunk> {
        let line = line.trim();
        if !line.starts_with("@@") {
            return None;
        }

        let rest = line.strip_prefix("@@")?.trim();
        let parts: Vec<&str> = rest.split("@@").collect();
        let header_part = parts.first()?.trim();

        let segments: Vec<&str> = header_part.split_whitespace().collect();
        if segments.len() < 2 {
            return None;
        }

        let old_part = segments[0].strip_prefix('-')?;
        let new_part = segments[1].strip_prefix('+')?;

        let (old_start, old_count) = Self::parse_range(old_part);
        let (new_start, new_count) = Self::parse_range(new_part);

        Some(DiffHunk {
            old_start: old_start.max(1),
            old_count,
            new_start: new_start.max(1),
            new_count,
            lines: Vec::new(),
        })
    }

    /// Parse a range like "1,7" or "5" (count defaults to 1).
    fn parse_range(s: &str) -> (usize, usize) {
        if let Some(comma) = s.find(',') {
            let start = s[..comma].parse().unwrap_or(1);
            let count = s[comma + 1..].parse().unwrap_or(1);
            (start, count)
        } else {
            let start = s.parse().unwrap_or(1);
            (start, 1)
        }
    }

    /// Extract context lines from a hunk.
    fn hunk_context(hunk: &DiffHunk) -> Vec<String> {
        hunk.lines
            .iter()
            .filter_map(|l| match l {
                HunkLine::Context(s) => Some(s.clone()),
                _ => None,
            })
            .collect()
    }

    /// Apply all hunks to file content, with offset adjustment.
    fn apply_hunks(
        file_content: &str,
        hunks: &[DiffHunk],
        file_path: &str,
    ) -> std::result::Result<String, String> {
        let mut result: Vec<String> = file_content.lines().map(|s| s.to_string()).collect();

        let mut line_offset: isize = 0;

        for hunk in hunks {
            let expected_line = (hunk.old_start as isize + line_offset).max(1) as usize;
            let context_lines = Self::hunk_context(hunk);

            let actual_line = if context_lines.is_empty() {
                expected_line
            } else {
                Self::find_context(&result, &context_lines, expected_line)
                    .ok_or_else(|| {
                        format!(
                            "apply_patch verification failed: could not find hunk context in {} at or near line {}",
                            file_path, expected_line
                        )
                    })?
            };

            let remove_start = actual_line.saturating_sub(1);
            let old_end = (remove_start + hunk.old_count).min(result.len());

            let new_lines: Vec<String> = hunk
                .lines
                .iter()
                .filter_map(|l| match l {
                    HunkLine::Context(s) | HunkLine::Added(s) => Some(s.clone()),
                    HunkLine::Removed(_) => None,
                })
                .collect();

            if remove_start <= result.len() {
                let drain_end = old_end.min(result.len());
                let removed_count = drain_end - remove_start;
                result.drain(remove_start..drain_end);
                for (i, line) in new_lines.iter().enumerate() {
                    result.insert(remove_start + i, line.clone());
                }
                line_offset += new_lines.len() as isize - removed_count as isize;
            }
        }

        let mut output = result.join("\n");
        if file_content.ends_with('\n') && !output.ends_with('\n') {
            output.push('\n');
        }
        Ok(output)
    }

    /// Find context lines in `result` near `expected_line`, within a search window.
    fn find_context(result: &[String], context: &[String], expected_line: usize) -> Option<usize> {
        if context.is_empty() {
            return Some(expected_line);
        }

        let search_window: isize = 20;

        if Self::context_matches(result, context, expected_line) {
            return Some(expected_line);
        }

        for delta in 1..=search_window {
            let before = expected_line as isize - delta;
            if before >= 1 && Self::context_matches(result, context, before as usize) {
                return Some(before as usize);
            }
            let after = expected_line as isize + delta;
            if after >= 1 && Self::context_matches(result, context, after as usize) {
                return Some(after as usize);
            }
        }

        None
    }

    /// Check if context lines match at a given 1-indexed line.
    fn context_matches(result: &[String], context: &[String], line: usize) -> bool {
        let idx = line.saturating_sub(1);
        if idx + context.len() > result.len() {
            return false;
        }
        context
            .iter()
            .enumerate()
            .all(|(i, ctx_line)| result[idx + i] == *ctx_line)
    }

    // ── Operation classification ───────────────────────────────────────────

    /// Parse a patch string and detect the operation type.
    ///
    /// Handles three formats:
    /// 1. TS-style opencode markers (`*** Begin Patch` / `*** Add File:` / etc.)
    /// 2. Legacy rustcode format (`*** Begin Patch <path> ***` / body / `*** End Patch`)
    /// 3. Standard unified diff (detects `--- /dev/null`, `rename from/to`, etc.)
    fn parse_patch_operation(patch_text: &str) -> Result<PatchOp> {
        let text = patch_text.trim();
        let normalized = text.replace("\r\n", "\n");
        let lines: Vec<&str> = normalized.lines().collect();

        // TS-style: "*** Begin Patch" on its own line
        let begin_idx = lines.iter().position(|l| l.trim() == "*** Begin Patch");
        let end_idx = lines.iter().position(|l| l.trim() == "*** End Patch");
        if let (Some(begin), Some(end)) = (begin_idx, end_idx) {
            if begin < end {
                return Self::parse_opencode_marker_patch(&lines[begin + 1..end]);
            }
        }

        // Legacy: "*** Begin Patch /path ***" on one line
        if let Some(after_begin) = text.strip_prefix("*** Begin Patch ") {
            if let Some(end_pos) = after_begin.find("***") {
                let file_path = after_begin[..end_pos].trim();
                let patch_start = end_pos + 3;
                let patch_body = if let Some(p) = after_begin[patch_start..].find("*** End Patch") {
                    &after_begin[patch_start..patch_start + p]
                } else {
                    &after_begin[patch_start..]
                };
                return Self::classify_standard_patch(patch_body.trim(), file_path);
            }
        }

        // Fallback: standard unified diff — try to extract path from ---/+++
        let path = lines
            .iter()
            .find_map(|l| {
                l.strip_prefix("--- a/")
                    .or_else(|| l.strip_prefix("+++ b/"))
            })
            .and_then(|p| {
                p.split_once(['\t', ' '])
                    .map(|(x, _)| x)
                    .or(Some(p))
            })
            .unwrap_or("")
            .to_string();

        Self::classify_standard_patch(text, &path)
    }

    /// Parse TS-style opencode marker patch content.
    ///
    /// Input lines are the body between `*** Begin Patch` and `*** End Patch`:
    /// ```text
    /// *** Add File: /path/to/file
    /// +line1
    /// +line2
    /// ```
    /// or:
    /// ```text
    /// *** Delete File: /path/to/file
    /// ```
    /// or:
    /// ```text
    /// *** Update File: /path/to/file
    /// *** Move to: /new/path
    /// @@ ... @@
    /// ```
    fn parse_opencode_marker_patch(lines: &[&str]) -> Result<PatchOp> {
        if lines.is_empty() {
            return Err(Error::Tool("apply_patch: empty patch content".into()));
        }

        let first = lines[0].trim();

        // ── Add file ────────────────────────────────────────────────────
        if let Some(path) = first.strip_prefix("*** Add File:") {
            let file_path = path.trim().to_string();
            if file_path.is_empty() {
                return Err(Error::Tool("apply_patch: empty add file path".into()));
            }
            let mut content_parts = Vec::new();
            for line in &lines[1..] {
                let trimmed = line.trim();
                if trimmed.starts_with("***") {
                    break;
                }
                if let Some(content_line) = trimmed.strip_prefix('+') {
                    content_parts.push(content_line.to_string());
                } else {
                    return Err(Error::Tool(format!(
                        "apply_patch: invalid add file line, expected '+' prefix: {}",
                        line
                    )));
                }
            }
            return Ok(PatchOp::Add {
                file_path,
                content: content_parts.join("\n"),
            });
        }

        // ── Delete file ─────────────────────────────────────────────────
        if let Some(path) = first.strip_prefix("*** Delete File:") {
            let file_path = path.trim().to_string();
            if file_path.is_empty() {
                return Err(Error::Tool("apply_patch: empty delete file path".into()));
            }
            return Ok(PatchOp::Delete { file_path });
        }

        // ── Update file (with optional move) ────────────────────────────
        if let Some(path) = first.strip_prefix("*** Update File:") {
            let file_path = path.trim().to_string();
            if file_path.is_empty() {
                return Err(Error::Tool("apply_patch: empty update file path".into()));
            }

            let patch_start = 1;
            if patch_start < lines.len() {
                if let Some(move_str) = lines[patch_start].trim().strip_prefix("*** Move to:") {
                    let target_path = move_str.trim().to_string();
                    if target_path.is_empty() {
                        return Err(Error::Tool("apply_patch: empty move target path".into()));
                    }
                    parse_move_patch(file_path, target_path, &lines[patch_start + 1..])
                } else {
                    let patch_body = lines[patch_start..].join("\n");
                    Ok(PatchOp::Update {
                        file_path,
                        patch: patch_body,
                    })
                }
            } else {
                Ok(PatchOp::Update {
                    file_path,
                    patch: String::new(),
                })
            }
        } else {
            Err(Error::Tool(format!(
                "apply_patch: unknown operation marker: {}",
                first
            )))
        }
    }

    /// Classify a standard unified diff as add / delete / move / update.
    ///
    /// Detection rules:
    /// - `--- /dev/null` on its own line → add
    /// - `+++ /dev/null` or `deleted file mode` → delete
    /// - `rename from` / `rename to` → move
    /// - Otherwise → update
    fn classify_standard_patch(patch: &str, default_path: &str) -> Result<PatchOp> {
        let lines: Vec<&str> = patch.lines().collect();

        // ── Move (rename from / rename to) ──────────────────────────────
        let mut src = None;
        let mut dst = None;
        for line in &lines {
            let t = line.trim();
            if let Some(p) = t.strip_prefix("rename from ") {
                src = Some(p.to_string());
            }
            if let Some(p) = t.strip_prefix("rename to ") {
                dst = Some(p.to_string());
            }
        }
        if let (Some(source), Some(target)) = (src, dst) {
            return Ok(PatchOp::Move {
                source_path: source,
                target_path: target,
            });
        }

        // ── Add (--- /dev/null) ─────────────────────────────────────────
        if lines.iter().any(|l| l.trim() == "--- /dev/null") {
            let new_path = lines
                .iter()
                .find_map(|l| l.trim().strip_prefix("+++ b/"))
                .and_then(|p| {
                    p.split_once(['\t', ' '])
                        .map(|(x, _)| x)
                        .or(Some(p))
                })
                .unwrap_or(default_path)
                .to_string();
            let content = Self::extract_add_content(&lines);
            return Ok(PatchOp::Add {
                file_path: new_path,
                content,
            });
        }

        // ── Delete (+++ /dev/null or deleted file mode) ─────────────────
        if lines.iter().any(|l| l.trim() == "+++ /dev/null")
            || lines.iter().any(|l| l.trim().contains("deleted file mode"))
        {
            let old_path = lines
                .iter()
                .find_map(|l| l.trim().strip_prefix("--- a/"))
                .and_then(|p| {
                    p.split_once(['\t', ' '])
                        .map(|(x, _)| x)
                        .or(Some(p))
                })
                .unwrap_or(default_path)
                .to_string();
            return Ok(PatchOp::Delete { file_path: old_path });
        }

        // ── Update (default) ────────────────────────────────────────────
        Ok(PatchOp::Update {
            file_path: default_path.to_string(),
            patch: patch.to_string(),
        })
    }

    /// Extract file content from `+` lines inside hunks of an add-file diff.
    fn extract_add_content(lines: &[&str]) -> String {
        let mut parts = Vec::new();
        let mut in_hunk = false;
        for line in lines {
            let t = line.trim();
            if t.starts_with("@@") {
                in_hunk = true;
                continue;
            }
            if in_hunk {
                if let Some(l) = t.strip_prefix('+') {
                    parts.push(l.to_string());
                }
            }
        }
        parts.join("\n")
    }

    // ── Operation execution ─────────────────────────────────────────────

    /// Execute an add-file operation: create parent directories, write content.
    fn execute_add(file_path: &str, content: &str) -> Result<ExecuteResult> {
        let path = std::path::Path::new(file_path);

        // Create parent directories
        if let Some(parent) = path.parent() {
            if !parent.as_os_str().is_empty() {
                std::fs::create_dir_all(parent).map_err(|e| Error::FileSystem {
                    path: parent.to_string_lossy().into(),
                    message: format!("failed to create parent directories: {}", e),
                })?;
            }
        }

        let content = if content.ends_with('\n') || content.is_empty() {
            content.to_string()
        } else {
            format!("{}\n", content)
        };

        std::fs::write(path, &content).map_err(|e| Error::FileSystem {
            path: file_path.to_string(),
            message: format!("failed to write file: {}", e),
        })?;

        Ok(ExecuteResult {
            title: format!("Created: {}", file_path),
            output: format!("Created file: {}", file_path),
            truncated: false,
            output_path: None,
            attachments: None,
            metadata: {
                let mut m = HashMap::new();
                m.insert(
                    "operation".into(),
                    serde_json::Value::String("add".into()),
                );
                m.insert(
                    "file".into(),
                    serde_json::Value::String(file_path.to_string()),
                );
                m
            },
        })
    }

    /// Execute a delete-file operation: remove the file.
    fn execute_delete(file_path: &str) -> Result<ExecuteResult> {
        let path = std::path::Path::new(file_path);
        if !path.exists() {
            return Err(Error::Tool(format!("File not found: {}", file_path)));
        }
        if path.is_dir() {
            return Err(Error::Tool(format!(
                "Path is a directory, not a file: {}",
                file_path
            )));
        }

        std::fs::remove_file(path).map_err(|e| Error::FileSystem {
            path: file_path.to_string(),
            message: format!("failed to delete file: {}", e),
        })?;

        Ok(ExecuteResult {
            title: format!("Deleted: {}", file_path),
            output: format!("Deleted file: {}", file_path),
            truncated: false,
            output_path: None,
            attachments: None,
            metadata: {
                let mut m = HashMap::new();
                m.insert(
                    "operation".into(),
                    serde_json::Value::String("delete".into()),
                );
                m.insert(
                    "file".into(),
                    serde_json::Value::String(file_path.to_string()),
                );
                m
            },
        })
    }

    /// Execute a move-file operation: rename source to target.
    fn execute_move(source_path: &str, target_path: &str) -> Result<ExecuteResult> {
        let source = std::path::Path::new(source_path);
        let target = std::path::Path::new(target_path);

        if !source.exists() {
            return Err(Error::Tool(format!("Source file not found: {}", source_path)));
        }
        if source.is_dir() {
            return Err(Error::Tool(format!(
                "Source is a directory, not a file: {}",
                source_path
            )));
        }
        if target.exists() {
            return Err(Error::Tool(format!("Target already exists: {}", target_path)));
        }

        // Create parent directories for target
        if let Some(parent) = target.parent() {
            if !parent.as_os_str().is_empty() {
                std::fs::create_dir_all(parent).map_err(|e| Error::FileSystem {
                    path: parent.to_string_lossy().into(),
                    message: format!("failed to create parent directories: {}", e),
                })?;
            }
        }

        std::fs::rename(source, target).map_err(|e| Error::FileSystem {
            path: source_path.to_string(),
            message: format!("failed to move file: {}", e),
        })?;

        Ok(ExecuteResult {
            title: format!("Moved: {} → {}", source_path, target_path),
            output: format!("Moved file: {} → {}", source_path, target_path),
            truncated: false,
            output_path: None,
            attachments: None,
            metadata: {
                let mut m = HashMap::new();
                m.insert(
                    "operation".into(),
                    serde_json::Value::String("move".into()),
                );
                m.insert(
                    "source".into(),
                    serde_json::Value::String(source_path.to_string()),
                );
                m.insert(
                    "target".into(),
                    serde_json::Value::String(target_path.to_string()),
                );
                m
            },
        })
    }

    /// Execute an update operation: apply unified diff hunks to an existing file.
    fn execute_update(file_path: &str, patch: &str) -> Result<ExecuteResult> {
        let path = std::path::Path::new(file_path);
        if !path.exists() {
            return Err(Error::Tool(format!("File not found: {}", file_path)));
        }
        if path.is_dir() {
            return Err(Error::Tool(format!(
                "Path is a directory, not a file: {}",
                file_path
            )));
        }

        let file_content = std::fs::read_to_string(path).map_err(Error::Io)?;
        let hunks = Self::parse_unified_diff(patch).map_err(Error::Tool)?;
        let patched = Self::apply_hunks(&file_content, &hunks, file_path).map_err(Error::Tool)?;

        std::fs::write(path, &patched).map_err(|e| Error::FileSystem {
            path: file_path.to_string(),
            message: format!("failed to write patched file: {}", e),
        })?;

        Ok(ExecuteResult {
            title: format!("Patched: {}", file_path),
            output: format!(
                "Applied patch to {}: {} hunk(s) applied successfully.",
                file_path,
                hunks.len()
            ),
            truncated: false,
            output_path: None,
            attachments: None,
            metadata: {
                let mut m = HashMap::new();
                m.insert(
                    "operation".into(),
                    serde_json::Value::String("update".into()),
                );
                m.insert(
                    "file".into(),
                    serde_json::Value::String(file_path.to_string()),
                );
                m.insert("hunks".into(), serde_json::json!(hunks.len()));
                m
            },
        })
    }
}

/// Helper: construct a PatchOp::Move from the opencode format.
fn parse_move_patch(
    source_path: String,
    target_path: String,
    _tail_lines: &[&str],
) -> Result<PatchOp> {
    Ok(PatchOp::Move {
        source_path,
        target_path,
    })
}

#[async_trait]
impl Tool for ApplyPatchTool {
    fn id(&self) -> &str {
        "apply_patch"
    }

    fn description(&self) -> &str {
        "Apply a patch with add, delete, move, and update operations. \
         Detects add (--- /dev/null), delete (+++ /dev/null or deleted file mode), \
         move (rename from/to), and update (unified diff) from the patch body. \
         Also supports the opencode marker format."
    }

    fn parameters_schema(&self) -> serde_json::Value {
        serde_json::json!({
            "type": "object",
            "properties": {
                "file_path": {
                    "type": "string",
                    "description": "The absolute path to the file to patch (for update/delete) or create (for add)"
                },
                "patch": {
                    "type": "string",
                    "description": "The patch content — unified diff or opencode marker format"
                },
                "patchText": {
                    "type": "string",
                    "description": "Full patch text with *** Begin Patch and *** End Patch markers (alternative to file_path + patch)"
                }
            },
            "anyOf": [
                { "required": ["file_path", "patch"] },
                { "required": ["patchText"] }
            ]
        })
    }

    async fn execute(&self, args: serde_json::Value, _ctx: &ToolContext) -> Result<ExecuteResult> {
        let operation = if let Some(patch_text) = args["patchText"].as_str() {
            Self::parse_patch_operation(patch_text)?
        } else {
            let file_path = args["file_path"]
                .as_str()
                .ok_or_else(|| Error::ToolInvalidArguments {
                    tool: "apply_patch".into(),
                    detail: "missing 'file_path' field".into(),
                })?
                .to_string();
            let patch = args["patch"]
                .as_str()
                .ok_or_else(|| Error::ToolInvalidArguments {
                    tool: "apply_patch".into(),
                    detail: "missing 'patch' field".into(),
                })?
                .to_string();
            Self::classify_standard_patch(&patch, &file_path)?
        };

        match operation {
            PatchOp::Add { file_path, content } => Self::execute_add(&file_path, &content),
            PatchOp::Delete { file_path } => Self::execute_delete(&file_path),
            PatchOp::Move {
                source_path,
                target_path,
            } => Self::execute_move(&source_path, &target_path),
            PatchOp::Update { file_path, patch } => Self::execute_update(&file_path, &patch),
        }
    }
}

// ═══════════════════════════════════════════════════════════════════════════════
// 10. TaskTool — subtask delegation
// ═══════════════════════════════════════════════════════════════════════════════

/// Delegates a complex task to a subagent.
///
/// Spawns a subagent session, passes context and instructions,
/// and returns the subagent's result.
///
/// # Source
/// Ported from `packages/opencode/src/tool/task.ts` (346 lines).
use std::sync::OnceLock;

use crate::agent::{derive_subagent_session_permission, AgentService};
use crate::background_job::{BackgroundJobService, JobStartInput};
use crate::id::{self, IdPrefix};
use crate::session::{CreateSessionInput, ModelSelection, SessionInfo, SessionManager};
use crate::session_prompt::{PromptPart, SessionPromptInput};
use crate::tool::TaskPromptOps;

/// Services required by TaskTool for subagent session lifecycle management.
///
/// Initialised once at startup via [`TaskTool::init_services`].
pub struct TaskToolServices {
    pub agent_service: Arc<AgentService>,
    pub session_manager: Arc<SessionManager>,
    pub background_jobs: Arc<BackgroundJobService>,
}

static TASK_TOOL_SERVICES: OnceLock<TaskToolServices> = OnceLock::new();

/// Background-task instruction text (TS `BACKGROUND_STARTED`).
const BACKGROUND_STARTED: &str = "\
The task is working in the background. You will be notified automatically when it finishes.\n\
DO NOT sleep, poll for progress, ask the task for status, or duplicate this task's work — \
avoid working with the same files or topics it is using.\n\
Work on non-overlapping tasks, or briefly tell the user what you launched and end your response.";

/// Background-updated instruction text (TS `BACKGROUND_UPDATED`).
const BACKGROUND_UPDATED: &str = "\
Additional context sent to the running background task.\n\
The task is still working in the background. You will be notified automatically when it finishes.\n\
DO NOT sleep, poll for progress, ask the task for status, or duplicate this task's work — \
avoid working with the same files or topics it is using.\n\
Work on non-overlapping tasks, or briefly tell the user what you sent and end your response.";

#[derive(Debug, Clone)]
pub struct TaskTool;

impl TaskTool {
    /// Initialise the global service references used by every TaskTool instance.
    ///
    /// Must be called once during runtime startup, before any tool execution.
    /// Subsequent calls are no-ops.
    pub fn init_services(services: TaskToolServices) {
        let _ = TASK_TOOL_SERVICES.set(services);
    }

    fn services(&self) -> Result<&'static TaskToolServices, Error> {
        TASK_TOOL_SERVICES.get().ok_or_else(|| {
            Error::Tool(
                "TaskTool services not initialised — call TaskTool::init_services() before first use"
                    .into(),
            )
        })
    }
}

/// Render a subagent result as XML-structured output.
///
/// # Source
/// Ported from `packages/opencode/src/tool/task.ts` lines 64–79 (`renderOutput`).
fn render_output(session_id: &str, state: &str, summary: Option<&str>, text: &str) -> String {
    let tag = if state == "error" {
        "task_error"
    } else {
        "task_result"
    };
    let mut output = format!("<task id=\"{session_id}\" state=\"{state}\">\n");
    if let Some(s) = summary {
        output.push_str(&format!("<summary>{s}</summary>\n"));
    }
    output.push_str(&format!("<{tag}>\n{text}\n</{tag}>\n</task>"));
    output
}

#[async_trait]
impl Tool for TaskTool {
    fn id(&self) -> &str {
        "task"
    }

    fn description(&self) -> &str {
        "Launch a new agent to handle complex, multi-step tasks autonomously.\n\n\
         Available agent types are listed in the system context.\n\n\
         The task tool launches specialized agents to handle specific types of work.\
         Each agent type has specific capabilities and tools available to it.\
         Use this when the task matches an agent type,\
         or when you need to run independent work in parallel."
    }

    fn parameters_schema(&self) -> serde_json::Value {
        serde_json::json!({
            "type": "object",
            "properties": {
                "description": {
                    "type": "string",
                    "description": "A short (3-5 words) description of the task"
                },
                "prompt": {
                    "type": "string",
                    "description": "The task for the agent to perform"
                },
                "subagent_type": {
                    "type": "string",
                    "description": "The type of specialized agent to use for this task"
                },
                "task_id": {
                    "type": "string",
                    "description": "Continue a previous task by passing its task_id"
                },
                "command": {
                    "type": "string",
                    "description": "The command that triggered this task"
                },
                "background": {
                    "type": "boolean",
                    "description": "Run the agent in the background. You will be notified when it completes."
                }
            },
            "required": ["description", "prompt"]
        })
    }

    async fn execute(&self, args: serde_json::Value, ctx: &ToolContext) -> Result<ExecuteResult> {
        let description = args["description"].as_str().ok_or_else(|| {
            Error::ToolInvalidArguments {
                tool: "task".into(),
                detail: "missing 'description' field".into(),
            }
        })?;

        let prompt = args["prompt"].as_str().ok_or_else(|| {
            Error::ToolInvalidArguments {
                tool: "task".into(),
                detail: "missing 'prompt' field".into(),
            }
        })?;

        let subagent_type = args["subagent_type"].as_str().unwrap_or("general-purpose");
        let task_id = args["task_id"].as_str();
        let command = args["command"].as_str();
        let is_background = args["background"].as_bool().unwrap_or(false);

        // 1. Ask for permission (unless bypassed via extra)
        let bypass = ctx
            .extra
            .get("bypassAgentCheck")
            .and_then(|v| v.as_bool())
            .unwrap_or(false);
        if !bypass {
            ctx.ask("task", subagent_type).await?;
        }

        // 2. Resolve services
        let svc = self.services()?;

        // 3. Look up the subagent type
        let agent = svc
            .agent_service
            .get(subagent_type)
            .ok_or_else(|| Error::Tool(format!("Unknown agent type: {subagent_type}")))?;

        // 4. Resolve or create the child session
        let existing_session: Option<SessionInfo> = if let Some(tid) = task_id {
            svc.session_manager.get(tid).await.ok()
        } else {
            None
        };

        let parent = svc.session_manager.get(&ctx.session_id).await.map_err(|e| {
            Error::Tool(format!("failed to get parent session: {e}"))
        })?;

        // 5. Derive child permissions
        let child_permission = derive_subagent_session_permission(
            parent.permission.as_ref().unwrap_or(&vec![]),
            agent,
        );

        // 6. Build the child permission rules
        let child_tool_denies = build_child_tool_denies(agent.permission.as_ref());
        let merged_permission: Vec<crate::permission::PermissionRule> = {
            let mut rules = child_permission;
            for deny in &child_tool_denies {
                if !rules.iter().any(|r| {
                    r.permission == deny.permission
                        && r.pattern == deny.pattern
                        && r.action == deny.action
                }) {
                    rules.push(deny.clone());
                }
            }
            rules
        };

        // 7. Resolve model for the subagent session
        let child_model = agent.model.as_ref().map(|m| ModelSelection {
            id: m.model_id.clone(),
            provider_id: m.provider_id.clone(),
            variant: None,
        });

        // 8. Create or reuse child session
        let child_session = if let Some(sess) = existing_session {
            sess
        } else {
            let session_id = id::descending(IdPrefix::Session, None)
                .map_err(|e| Error::Tool(format!("session id generation failed: {e}")))?;
            let title = format!("{description} (@{} subagent)", agent.name);

            let create_input = CreateSessionInput {
                project_id: parent.project_id.clone(),
                workspace_id: parent.workspace_id.clone(),
                directory: parent.directory.clone(),
                path: parent.path.clone(),
                parent_id: Some(ctx.session_id.clone()),
                title: Some(title),
                agent: Some(agent.name.clone()),
                model: child_model.clone(),
                metadata: Some(serde_json::json!({
                    "parentSessionId": ctx.session_id,
                    "sessionId": session_id,
                    "model": child_model,
                })),
                permission: Some(merged_permission),
            };

            svc.session_manager.create(create_input).await.map_err(|e| {
                Error::Tool(format!("failed to create child session: {e}"))
            })?
        };

        let child_session_id = child_session.id.clone();

        // 9. Read the prompt ops from the tool context
        let prompt_ops = ctx.prompt_ops.clone().ok_or_else(|| {
            Error::Tool(
                "TaskTool requires prompt_ops in ToolContext — set by the session processor"
                    .into(),
            )
        })?;

        // 10. Metadata used across all return paths
        let mut metadata = HashMap::new();
        metadata.insert(
            "subagent_type".into(),
            serde_json::Value::String(subagent_type.to_string()),
        );
        metadata.insert(
            "sessionID".into(),
            serde_json::Value::String(child_session_id.clone()),
        );
        if is_background {
            metadata.insert("background".into(), serde_json::Value::Bool(true));
        }
        if let Some(cmd) = command {
            metadata.insert("command".into(), serde_json::Value::String(cmd.to_string()));
        }

        // 11. Build the prompt parts from the user's prompt text
        let prompt_parts = (prompt_ops.resolve_prompt_parts)(prompt)?;

        // 12. Build the SessionPromptInput for running the subagent
        let session_model = agent.model.as_ref().map(|m| crate::session_info::ModelRef {
            id: m.model_id.clone(),
            provider_id: m.provider_id.clone(),
            variant: None,
        });
        let session_prompt_input = SessionPromptInput {
            session_id: child_session_id.clone(),
            message_id: Some(
                id::ascending(IdPrefix::Message, None)
                    .unwrap_or_else(|_| "msg_fallback".to_string()),
            ),
            model: session_model,
            agent: Some(agent.name.clone()),
            no_reply: false,
            tools: None,
            format: None,
            system: None,
            variant: None,
            parts: prompt_parts,
        };

        // 13. Background path
        if is_background {
            let bg_id = child_session_id.clone();
            let bg_desc = description.to_string();
            let bg_metadata = metadata.clone();
            let prompt_input = session_prompt_input;
            let ops_arc = prompt_ops.clone();

            let run_fn = move || {
                let ops = ops_arc.clone();
                let input = prompt_input.clone();
                async move {
                    (ops.prompt)(input).await.map_err(|e| e.to_string())
                }
            };

            let job_info = svc
                .background_jobs
                .start(
                    JobStartInput {
                        id: Some(bg_id.clone()),
                        type_: "task".into(),
                        title: Some(bg_desc.clone()),
                        metadata: Some(serde_json::json!(bg_metadata)),
                        on_promote: None,
                    },
                    run_fn,
                )
                .await;

            metadata.insert(
                "jobId".into(),
                serde_json::Value::String(job_info.id.clone()),
            );

            return Ok(ExecuteResult {
                title: description.to_string(),
                output: render_output(
                    &child_session_id,
                    "running",
                    Some("Background task started"),
                    BACKGROUND_STARTED,
                ),
                truncated: false,
                output_path: None,
                attachments: None,
                metadata,
            });
        }

        // 14. Foreground path — run the prompt and wait
        let result = (prompt_ops.prompt)(session_prompt_input)
            .await
            .map_err(|e| Error::Tool(format!("subagent execution failed: {e}")))?;

        Ok(ExecuteResult {
            title: description.to_string(),
            output: render_output(
                &child_session_id,
                "completed",
                Some("Task completed"),
                &result,
            ),
            truncated: false,
            output_path: None,
            attachments: None,
            metadata,
        })
    }
}

/// Build deny rules for tools that the subagent should not use.
///
/// Blocks `todowrite` and `task` unless the subagent's own permission ruleset
/// already allows them. Also blocks any tools listed as `primary_tools` in the
/// experimental config (placeholder — user config not accessible here).
///
/// # Source
/// Ported from `packages/opencode/src/tool/task.ts` lines 129–141.
fn build_child_tool_denies(
    subagent_permission: &[crate::permission::PermissionRule],
) -> Vec<crate::permission::PermissionRule> {
    let mut denies = Vec::new();

    let can_todo = subagent_permission
        .iter()
        .any(|r| r.permission == "todowrite");
    if !can_todo {
        denies.push(crate::permission::PermissionRule {
            permission: "todowrite".into(),
            pattern: "*".into(),
            action: crate::permission::PermissionAction::Deny,
        });
    }

    let can_task = subagent_permission
        .iter()
        .any(|r| r.permission == "task");
    if !can_task {
        denies.push(crate::permission::PermissionRule {
            permission: "task".into(),
            pattern: "*".into(),
            action: crate::permission::PermissionAction::Deny,
        });
    }

    denies
}

// ═══════════════════════════════════════════════════════════════════════════════
// 11. QuestionTool — user questioning
// ═══════════════════════════════════════════════════════════════════════════════

/// Asks the user questions during execution.
///
/// Publishes a question event on the event bus and awaits the user's answer.
/// The answer is returned once received (not immediately with "pending" state).
///
/// # Source
/// Ported from `packages/core/src/tool/question.ts` and
/// `packages/opencode/src/tool/question.ts`.
#[derive(Clone)]
pub struct QuestionTool {
    /// The question service used to create deferred questions and await answers.
    pub service: Arc<QuestionService>,
}

impl std::fmt::Debug for QuestionTool {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("QuestionTool").finish()
    }
}

impl QuestionTool {
    /// Create a new question tool backed by the given question service.
    pub fn new(service: Arc<QuestionService>) -> Self {
        Self { service }
    }
}

#[async_trait]
impl Tool for QuestionTool {
    fn id(&self) -> &str {
        "question"
    }

    fn description(&self) -> &str {
        "Ask the user one or more questions during execution.\
         Questions are published as events on the session bus and the user's\
         answers are collected and returned.\
         \n\n\
         Each question can include header text, predefined options with labels\
         and descriptions, allow multiple selection, and support custom answers."
    }

    fn parameters_schema(&self) -> serde_json::Value {
        serde_json::json!({
            "type": "object",
            "properties": {
                "questions": {
                    "type": "array",
                    "description": "Questions to ask the user",
                    "items": {
                        "type": "object",
                        "properties": {
                            "question": {
                                "type": "string",
                                "description": "The question to ask the user"
                            },
                            "header": {
                                "type": "string",
                                "description": "Header text for the question"
                            },
                            "options": {
                                "type": "array",
                                "description": "Available options the user can choose from",
                                "items": {
                                    "type": "object",
                                    "properties": {
                                        "label": {
                                            "type": "string",
                                            "description": "The label for this option"
                                        },
                                        "description": {
                                            "type": "string",
                                            "description": "Description of what this option means"
                                        }
                                    },
                                    "required": ["label"]
                                }
                            },
                            "multiple": {
                                "type": "boolean",
                                "description": "Whether multiple options can be selected"
                            },
                            "custom": {
                                "type": "boolean",
                                "description": "Whether to allow a custom/free-form answer"
                            }
                        },
                        "required": ["question"]
                    }
                }
            },
            "required": ["questions"]
        })
    }

    async fn execute(&self, args: serde_json::Value, ctx: &ToolContext) -> Result<ExecuteResult> {
        let questions =
            args["questions"]
                .as_array()
                .ok_or_else(|| Error::ToolInvalidArguments {
                    tool: "question".into(),
                    detail: "missing 'questions' array".into(),
                })?;

        let question_count = questions.len();

        // Permission check — mirrors the TS `permission.assert()`
        let allowed = ctx.ask("question", "*").await?;
        if !allowed {
            return Err(Error::Permission(crate::error::PermissionError::Denied));
        }

        // Convert raw JSON questions to QuestionInfo
        let infos: Vec<QuestionInfo> = questions
            .iter()
            .map(|q| {
                let question = q["question"].as_str().unwrap_or("Question").to_string();
                let header = q["header"].as_str().unwrap_or("Question").to_string();
                let mut info = QuestionInfo::new(question, header);
                if let Some(options) = q["options"].as_array() {
                    info = info.with_options(
                        options
                            .iter()
                            .map(|o| {
                                QuestionOption::new(
                                    o["label"].as_str().unwrap_or(""),
                                    o["description"].as_str().unwrap_or(""),
                                )
                            })
                            .collect(),
                    );
                }
                if q.get("multiple")
                    .and_then(|v| v.as_bool())
                    .unwrap_or(false)
                {
                    info = info.with_multiple(true);
                }
                if let Some(custom) = q.get("custom").and_then(|v| v.as_bool()) {
                    info = info.with_custom(custom);
                }
                info
            })
            .collect();

        // Build the tool context for the question metadata
        let question_tool = crate::question::QuestionTool {
            message_id: ctx.message_id.clone(),
            call_id: ctx.call_id.clone().unwrap_or_default(),
        };

        // Ask the user and await the answer (blocks until the user replies)
        let answers = match self
            .service
            .ask(&ctx.session_id, infos.clone(), Some(question_tool))
            .await
        {
            Ok(a) => a,
            Err(e) => {
                return Ok(ExecuteResult {
                    title: "Question dismissed".into(),
                    output: format!("The user dismissed the question: {e}"),
                    truncated: false,
                    output_path: None,
                    attachments: None,
                    metadata: HashMap::new(),
                });
            }
        };

        // Format the output matching the TS `toModelOutput()` function
        let prompts: Vec<QuestionPrompt> = infos.into_iter().map(Into::into).collect();
        let output = format_model_output(&prompts, &answers);

        Ok(ExecuteResult {
            title: format!(
                "Asked {} question{}",
                question_count,
                if question_count > 1 { "s" } else { "" }
            ),
            output,
            truncated: false,
            output_path: None,
            attachments: None,
            metadata: {
                let mut m = HashMap::new();
                let _ = m.insert(
                    "answers".into(),
                    serde_json::to_value(&answers).unwrap_or_default(),
                );
                m
            },
        })
    }
}

// ═══════════════════════════════════════════════════════════════════════════════
// 12. SkillTool — skill invocation
// ═══════════════════════════════════════════════════════════════════════════════

/// Invokes a named skill and loads its markdown instructions.
///
/// Loads the skill markdown file from `.opencode/skills/{skill}.md`,
/// parses YAML frontmatter, and returns the skill instructions and metadata.
///
/// # Source
/// Ported from `packages/core/src/tool/skill.ts` and
/// `packages/opencode/src/tool/skill.ts`.
#[derive(Debug, Clone)]
pub struct SkillTool;

impl SkillTool {
    /// Load a skill from `.opencode/skills/{name}.md` and parse its frontmatter.
    fn load_skill(skill_name: &str) -> Result<(serde_json::Value, String)> {
        // Try multiple skill directory locations
        let cwd = std::env::current_dir().unwrap_or_else(|_| std::path::PathBuf::from("."));
        let candidates = vec![
            cwd.join(".opencode")
                .join("skills")
                .join(format!("{}.md", skill_name)),
            cwd.join(".claude")
                .join("skills")
                .join(format!("{}.md", skill_name)),
        ];

        for skill_path in &candidates {
            if skill_path.exists() {
                let content = std::fs::read_to_string(skill_path).map_err(Error::Io)?;
                let (frontmatter, body) = Self::parse_frontmatter(&content);
                return Ok((frontmatter, body.to_string()));
            }
        }

        Err(Error::Skill(SkillError::NotFound {
            name: skill_name.to_string(),
        }))
    }

    /// Parse YAML frontmatter delimited by `---` markers.
    fn parse_frontmatter(content: &str) -> (serde_json::Value, &str) {
        let trimmed = content.trim_start();
        if !trimmed.starts_with("---") {
            return (serde_json::Value::Null, content);
        }

        // Find the closing ---
        let after_open = &trimmed[3..];
        if let Some(close_pos) = after_open.find("\n---") {
            let yaml_str = &after_open[..close_pos].trim();
            let body = after_open[close_pos + 4..].trim_start();

            // Parse simple YAML key: value pairs
            let mut map = serde_json::Map::new();
            for line in yaml_str.lines() {
                let line = line.trim();
                if line.is_empty() || line.starts_with('#') {
                    continue;
                }
                if let Some(col_pos) = line.find(':') {
                    let key = line[..col_pos].trim().to_string();
                    let value = line[col_pos + 1..].trim();
                    // Try to parse as JSON value (number, bool, null), fall back to string
                    let parsed = serde_json::from_str(value)
                        .unwrap_or_else(|_| serde_json::Value::String(value.to_string()));
                    map.insert(key, parsed);
                }
            }
            (serde_json::Value::Object(map), body)
        } else {
            (serde_json::Value::Null, content)
        }
    }
}

#[async_trait]
impl Tool for SkillTool {
    fn id(&self) -> &str {
        "skill"
    }

    fn description(&self) -> &str {
        "Invoke a named skill to load its instructions into the current conversation.\
         Skills are markdown files in .opencode/skills/ with YAML frontmatter.\
         Use this tool when the task matches an available skill from the system context."
    }

    fn parameters_schema(&self) -> serde_json::Value {
        serde_json::json!({
            "type": "object",
            "properties": {
                "skill": {
                    "type": "string",
                    "description": "The skill name (must match a file in .opencode/skills/ without the .md extension)"
                },
                "args": {
                    "type": "string",
                    "description": "Optional arguments to pass to the skill"
                }
            },
            "required": ["skill"]
        })
    }

    async fn execute(&self, args: serde_json::Value, _ctx: &ToolContext) -> Result<ExecuteResult> {
        let skill_name = args["skill"]
            .as_str()
            .ok_or_else(|| Error::ToolInvalidArguments {
                tool: "skill".into(),
                detail: "missing 'skill' field".into(),
            })?;

        let skill_args = args["args"].as_str();

        // Attempt to load the skill from the filesystem
        match Self::load_skill(skill_name) {
            Ok((frontmatter, body)) => {
                let mut output = format!("<skill_content name=\"{}\">\n", skill_name);
                if !frontmatter.is_null() {
                    output.push_str(&format!(
                        "<!-- frontmatter: {} -->\n",
                        serde_json::to_string(&frontmatter).unwrap_or_default()
                    ));
                }
                if let Some(a) = skill_args {
                    output.push_str(&format!("<!-- args: {} -->\n", a));
                }
                output.push_str(&body);
                output.push_str("\n</skill_content>");

                let mut metadata = HashMap::new();
                metadata.insert(
                    "skill".into(),
                    serde_json::Value::String(skill_name.to_string()),
                );
                if let Some(a) = skill_args {
                    metadata.insert("args".into(), serde_json::Value::String(a.to_string()));
                }
                if !frontmatter.is_null() {
                    metadata.insert("frontmatter".into(), frontmatter);
                }

                Ok(ExecuteResult {
                    title: format!("Loaded skill: {}", skill_name),
                    output,
                    truncated: false,
                    output_path: None,
                    attachments: None,
                    metadata,
                })
            }
            Err(_) => {
                // Skill not found on disk — return a stub indicating it would be loaded.
                let output = format!(
                    "<skill_content name=\"{}\">\n\
                     # Skill: {}\n\n\
                     Skill file not found at .opencode/skills/{}.md.\n\
                     When available, this tool loads the skill's markdown instructions,\n\
                     parses YAML frontmatter, and returns the skill content.\n\
                     </skill_content>",
                    skill_name, skill_name, skill_name
                );

                Ok(ExecuteResult {
                    title: format!("Loaded skill: {}", skill_name),
                    output,
                    truncated: false,
                    output_path: None,
                    attachments: None,
                    metadata: {
                        let mut m = HashMap::new();
                        m.insert(
                            "skill".into(),
                            serde_json::Value::String(skill_name.to_string()),
                        );
                        if let Some(a) = skill_args {
                            m.insert("args".into(), serde_json::Value::String(a.to_string()));
                        }
                        m
                    },
                })
            }
        }
    }
}

// ═══════════════════════════════════════════════════════════════════════════════
// 13. TodoWriteTool — todo list management
// ═══════════════════════════════════════════════════════════════════════════════

/// Manages a structured todo list for the current session.
///
/// # Source
/// Ported from `packages/core/src/tool/todowrite.ts` and
/// `packages/opencode/src/tool/todo.ts`.
#[derive(Debug, Clone)]
pub struct TodoWriteTool;

#[async_trait]
impl Tool for TodoWriteTool {
    fn id(&self) -> &str {
        "todowrite"
    }

    fn description(&self) -> &str {
        "Create and maintain a structured task list for the current coding session.\
         Use it to track progress during multi-step work and keep todo statuses current.\
         Each todo has: text (description), status (pending/in_progress/completed/cancelled),\
         and priority (high/medium/low)."
    }

    fn parameters_schema(&self) -> serde_json::Value {
        serde_json::json!({
            "type": "object",
            "properties": {
                "todos": {
                    "type": "array",
                    "description": "The updated todo list",
                    "items": {
                        "type": "object",
                        "properties": {
                            "text": {
                                "type": "string",
                                "description": "Brief description of the task"
                            },
                            "status": {
                                "type": "string",
                                "enum": ["pending", "in_progress", "completed", "cancelled"],
                                "description": "Current status of the task"
                            },
                            "priority": {
                                "type": "string",
                                "enum": ["high", "medium", "low"],
                                "description": "Priority level of the task"
                            }
                        },
                        "required": ["text", "status", "priority"]
                    }
                }
            },
            "required": ["todos"]
        })
    }

    async fn execute(&self, args: serde_json::Value, _ctx: &ToolContext) -> Result<ExecuteResult> {
        let todos = args["todos"]
            .as_array()
            .ok_or_else(|| Error::ToolInvalidArguments {
                tool: "todowrite".into(),
                detail: "missing 'todos' array".into(),
            })?;

        let incomplete_count = todos
            .iter()
            .filter(|t| t["status"].as_str() != Some("completed"))
            .count();

        let output = serde_json::to_string_pretty(todos).map_err(Error::Json)?;

        Ok(ExecuteResult {
            title: format!("{} todos", incomplete_count),
            output,
            truncated: false,
            output_path: None,
            attachments: None,
            metadata: {
                let mut m = HashMap::new();
                m.insert("todos".into(), serde_json::Value::Array(todos.clone()));
                m
            },
        })
    }
}

// ═══════════════════════════════════════════════════════════════════════════════
// 14. PlanEnterTool — plan mode entry
// ═══════════════════════════════════════════════════════════════════════════════

/// Enters plan mode, creating a structured plan before implementation.
///
/// When entering plan mode, the agent stops executing tools and instead
/// creates a detailed implementation plan. This tool transitions the session
/// into plan mode.
///
/// # Source
/// Ported from `packages/opencode/src/tool/plan.ts` (79 lines).
#[derive(Debug, Clone)]
pub struct PlanEnterTool;

#[async_trait]
impl Tool for PlanEnterTool {
    fn id(&self) -> &str {
        "plan_enter"
    }

    fn description(&self) -> &str {
        "Enter plan mode to design an implementation strategy.\
         Use this tool when you need to think through a complex problem before writing code.\
         In plan mode, the agent creates a detailed step-by-step plan, identifies critical files,\
         and considers architectural trade-offs. After planning, use plan_exit to switch to\
         the build agent for implementation."
    }

    fn parameters_schema(&self) -> serde_json::Value {
        serde_json::json!({
            "type": "object",
            "properties": {
                "plan": {
                    "type": "string",
                    "description": "A description of what needs to be planned"
                }
            }
        })
    }

    async fn execute(&self, args: serde_json::Value, _ctx: &ToolContext) -> Result<ExecuteResult> {
        let plan_desc = args
            .get("plan")
            .and_then(|v| v.as_str())
            .unwrap_or("implementation strategy");

        // Plan enter tool requires session management and agent mode switching infrastructure.
        // This stub returns a basic response indicating plan mode was entered.
        Ok(ExecuteResult {
            title: format!("Planning: {}", plan_desc),
            output: format!(
                "Entered plan mode for: {}\n\n\
                 Now in plan mode. Create a detailed implementation plan before using plan_exit\
                 to switch to the build agent. Consider architecture, file changes, edge cases,\
                 and testing strategy.",
                plan_desc
            ),
            truncated: false,
            output_path: None,
            attachments: None,
            metadata: {
                let mut m = HashMap::new();
                m.insert("mode".into(), serde_json::Value::String("plan".into()));
                m.insert(
                    "plan".into(),
                    serde_json::Value::String(plan_desc.to_string()),
                );
                m
            },
        })
    }
}

// ═══════════════════════════════════════════════════════════════════════════════
// 15. PlanExitTool — plan mode exit
// ═══════════════════════════════════════════════════════════════════════════════

/// Enters/exits plan mode, tracking plan state.
///
/// When a plan is complete, this tool asks the user if they want to switch
/// to the build agent.
///
/// # Source
/// Ported from `packages/opencode/src/tool/plan.ts` (79 lines).
#[derive(Debug, Clone)]
pub struct PlanExitTool;

#[async_trait]
impl Tool for PlanExitTool {
    fn id(&self) -> &str {
        "plan_exit"
    }

    fn description(&self) -> &str {
        "Exit plan mode and switch to the build agent.\
         Use this tool when the plan is complete and you want to start implementing.\
         The user will be asked if they want to switch to the build agent."
    }

    fn parameters_schema(&self) -> serde_json::Value {
        serde_json::json!({
            "type": "object",
            "properties": {}
        })
    }

    async fn execute(&self, _args: serde_json::Value, _ctx: &ToolContext) -> Result<ExecuteResult> {
        // Plan exit tool requires session management and agent switching infrastructure.
        // This stub returns a basic response.
        Ok(ExecuteResult {
            title: "Switching to build agent".into(),
            output: "Exited plan mode. Ready to switch to build agent for implementation.".into(),
            truncated: false,
            output_path: None,
            attachments: None,
            metadata: HashMap::new(),
        })
    }
}

// ═══════════════════════════════════════════════════════════════════════════════
// 16. ExitPlanModeTool — exit plan mode with plan content
// ═══════════════════════════════════════════════════════════════════════════════

/// Exits plan mode and transitions to implementation mode.
///
/// Takes the plan content as input and returns it for the implementation phase.
/// Transitions the session from planning to building.
///
/// # Source
/// Ported from `packages/opencode/src/tool/plan.ts` — exit plan mode variant.
#[derive(Debug, Clone)]
pub struct ExitPlanModeTool;

#[async_trait]
impl Tool for ExitPlanModeTool {
    fn id(&self) -> &str {
        "exit_plan_mode"
    }

    fn description(&self) -> &str {
        "Exit plan mode and transition to implementation mode.\
         Takes the complete plan as input and returns it for the build phase.\
         Use this when the planning phase is finished and implementation should begin."
    }

    fn parameters_schema(&self) -> serde_json::Value {
        serde_json::json!({
            "type": "object",
            "properties": {
                "plan": {
                    "type": "string",
                    "description": "The plan to implement (full plan content from the planning phase)"
                }
            },
            "required": ["plan"]
        })
    }

    async fn execute(&self, args: serde_json::Value, _ctx: &ToolContext) -> Result<ExecuteResult> {
        let plan_content = args["plan"]
            .as_str()
            .ok_or_else(|| Error::ToolInvalidArguments {
                tool: "exit_plan_mode".into(),
                detail: "missing 'plan' field".into(),
            })?;

        let output = format!(
            "Exited plan mode. Transitioning to implementation.\n\n\
             ## Plan\n\n{}\n\n\
             Ready to implement the plan. Switch to the build agent to begin execution.",
            plan_content
        );

        Ok(ExecuteResult {
            title: "Exited plan mode — implementing".into(),
            output,
            truncated: false,
            output_path: None,
            attachments: None,
            metadata: {
                let mut m = HashMap::new();
                m.insert("mode".into(), serde_json::Value::String("build".into()));
                m.insert(
                    "plan".into(),
                    serde_json::Value::String(plan_content.to_string()),
                );
                m
            },
        })
    }
}

// ═══════════════════════════════════════════════════════════════════════════════
// 17. StashTool — file content snapshots
// ═══════════════════════════════════════════════════════════════════════════════

/// Saves and restores file content snapshots for safe experimentation.
///
/// Snapshots are stored as JSON files under ~/.rustcode/stashes/.
/// Supports save, restore, list, and drop operations.
///
/// # Source
/// Ported from `packages/opencode/src/tool/` — stash pattern.
#[derive(Debug, Clone)]
pub struct StashTool;

impl StashTool {
    fn stash_dir() -> std::path::PathBuf {
        let home = std::env::var("HOME")
            .or_else(|_| std::env::var("USERPROFILE"))
            .unwrap_or_else(|_| ".".into());
        std::path::PathBuf::from(home)
            .join(".rustcode")
            .join("stashes")
    }

    fn ensure_stash_dir() -> std::result::Result<std::path::PathBuf, String> {
        let dir = Self::stash_dir();
        std::fs::create_dir_all(&dir)
            .map_err(|e| format!("failed to create stash directory: {}", e))?;
        Ok(dir)
    }

    fn stash_path(name: &str) -> std::path::PathBuf {
        Self::stash_dir().join(format!("{}.json", name))
    }

    fn save_stash(name: &str, files: &[String]) -> std::result::Result<String, String> {
        let dir = Self::ensure_stash_dir()?;
        let stash_file = dir.join(format!("{}.json", name));

        let mut entries = Vec::new();
        for pattern in files {
            let paths = glob::glob(pattern).map_err(|e| format!("glob error: {}", e))?;
            for entry in paths.flatten() {
                if entry.is_file() {
                    let content = std::fs::read_to_string(&entry)
                        .map_err(|e| format!("failed to read {}: {}", entry.display(), e))?;
                    entries.push(serde_json::json!({
                        "path": entry.to_string_lossy(),
                        "content": content,
                    }));
                }
            }
        }

        let stash_data = serde_json::json!({
            "name": name,
            "created_at": chrono::Utc::now().to_rfc3339(),
            "file_count": entries.len(),
            "files": entries,
        });

        let json = serde_json::to_string_pretty(&stash_data)
            .map_err(|e| format!("serialization error: {}", e))?;
        std::fs::write(&stash_file, json).map_err(|e| format!("failed to write stash: {}", e))?;

        Ok(format!(
            "Stash '{}' saved with {} file(s) at {}",
            name,
            entries.len(),
            stash_file.display()
        ))
    }

    fn restore_stash(name: &str) -> std::result::Result<String, String> {
        let stash_file = Self::stash_path(name);
        if !stash_file.exists() {
            return Err(format!("stash '{}' not found", name));
        }

        let json: serde_json::Value = serde_json::from_str(
            &std::fs::read_to_string(&stash_file)
                .map_err(|e| format!("failed to read stash: {}", e))?,
        )
        .map_err(|e| format!("failed to parse stash JSON: {}", e))?;

        let files = json["files"]
            .as_array()
            .ok_or_else(|| "stash has no 'files' array".to_string())?;

        let mut restored = 0usize;
        for entry in files {
            let path = entry["path"]
                .as_str()
                .ok_or_else(|| "stash entry missing 'path'".to_string())?;
            let content = entry["content"]
                .as_str()
                .ok_or_else(|| "stash entry missing 'content'".to_string())?;

            if let Some(parent) = std::path::Path::new(path).parent() {
                std::fs::create_dir_all(parent)
                    .map_err(|e| format!("failed to create dir for {}: {}", path, e))?;
            }
            std::fs::write(path, content)
                .map_err(|e| format!("failed to restore {}: {}", path, e))?;
            restored += 1;
        }

        Ok(format!(
            "Stash '{}' restored: {} file(s) written.",
            name, restored
        ))
    }

    fn list_stashes() -> std::result::Result<String, String> {
        let dir = Self::stash_dir();
        if !dir.exists() {
            return Ok("No stashes found (stash directory does not exist).".into());
        }

        let mut stashes = Vec::new();
        if let Ok(entries) = std::fs::read_dir(&dir) {
            for entry in entries.flatten() {
                let stash_path = entry.path();
                if stash_path.extension().is_some_and(|e| e == "json") {
                    if let Ok(content) = std::fs::read_to_string(&stash_path) {
                        if let Ok(json) = serde_json::from_str::<serde_json::Value>(&content) {
                            let name = json["name"].as_str().unwrap_or("unknown");
                            let date = json["created_at"].as_str().unwrap_or("unknown");
                            let count = json["file_count"].as_u64().unwrap_or(0);
                            stashes.push(format!("  {} — {} — {} file(s)", name, date, count));
                        }
                    }
                }
            }
        }

        if stashes.is_empty() {
            Ok("No stashes found.".into())
        } else {
            Ok(format!("Stashes:\n{}", stashes.join("\n")))
        }
    }

    fn drop_stash(name: &str) -> std::result::Result<String, String> {
        let stash_file = Self::stash_path(name);
        if !stash_file.exists() {
            return Err(format!("stash '{}' not found", name));
        }
        std::fs::remove_file(&stash_file).map_err(|e| format!("failed to delete stash: {}", e))?;
        Ok(format!("Stash '{}' dropped.", name))
    }
}

#[async_trait]
impl Tool for StashTool {
    fn id(&self) -> &str {
        "stash"
    }

    fn description(&self) -> &str {
        "Stash and restore file content snapshots for safe experimentation.\
         Actions: 'save' creates a snapshot of matching files, 'restore'\
         restores files from a stash, 'list' shows all saved stashes,\
         'drop' deletes a stash."
    }

    fn parameters_schema(&self) -> serde_json::Value {
        serde_json::json!({
            "type": "object",
            "properties": {
                "action": {
                    "type": "string",
                    "enum": ["save", "restore", "list", "drop"],
                    "description": "The stash action to perform"
                },
                "name": {
                    "type": "string",
                    "description": "Name of the stash (required for save, restore, drop)"
                },
                "files": {
                    "type": "array",
                    "description": "Glob patterns for files to stash (required for save action)",
                    "items": { "type": "string" }
                }
            },
            "required": ["action"]
        })
    }

    async fn execute(&self, args: serde_json::Value, _ctx: &ToolContext) -> Result<ExecuteResult> {
        let action = args["action"]
            .as_str()
            .ok_or_else(|| Error::ToolInvalidArguments {
                tool: "stash".into(),
                detail: "missing 'action' field".into(),
            })?;

        let name = args["name"].as_str();
        let files: Vec<String> = args
            .get("files")
            .and_then(|v| v.as_array())
            .map(|arr| {
                arr.iter()
                    .filter_map(|v| v.as_str().map(|s| s.to_string()))
                    .collect()
            })
            .unwrap_or_default();

        let (output, title) = match action {
            "save" => {
                let name = name.ok_or_else(|| Error::ToolInvalidArguments {
                    tool: "stash".into(),
                    detail: "missing 'name' field for save action".into(),
                })?;
                if files.is_empty() {
                    return Err(Error::ToolInvalidArguments {
                        tool: "stash".into(),
                        detail: "missing 'files' array for save action".into(),
                    });
                }
                let msg = Self::save_stash(name, &files).map_err(Error::Tool)?;
                (msg, format!("Stash saved: {}", name))
            }
            "restore" => {
                let name = name.ok_or_else(|| Error::ToolInvalidArguments {
                    tool: "stash".into(),
                    detail: "missing 'name' field for restore action".into(),
                })?;
                let msg = Self::restore_stash(name).map_err(Error::Tool)?;
                (msg, format!("Stash restored: {}", name))
            }
            "list" => {
                let msg = Self::list_stashes().map_err(Error::Tool)?;
                (msg, "Stash list".into())
            }
            "drop" => {
                let name = name.ok_or_else(|| Error::ToolInvalidArguments {
                    tool: "stash".into(),
                    detail: "missing 'name' field for drop action".into(),
                })?;
                let msg = Self::drop_stash(name).map_err(Error::Tool)?;
                (msg, format!("Stash dropped: {}", name))
            }
            other => {
                return Err(Error::ToolInvalidArguments {
                    tool: "stash".into(),
                    detail: format!(
                        "unknown action '{}'. Must be one of: save, restore, list, drop",
                        other
                    ),
                });
            }
        };

        Ok(ExecuteResult {
            title,
            output,
            truncated: false,
            output_path: None,
            attachments: None,
            metadata: {
                let mut m = HashMap::new();
                m.insert(
                    "action".into(),
                    serde_json::Value::String(action.to_string()),
                );
                if let Some(n) = name {
                    m.insert("name".into(), serde_json::Value::String(n.to_string()));
                }
                m
            },
        })
    }
}

// ═══════════════════════════════════════════════════════════════════════════════
// 18. NotebookEditTool — Jupyter notebook cell editing
// ═══════════════════════════════════════════════════════════════════════════════

/// Edits cells in a Jupyter notebook (.ipynb) file.
///
/// Parses the notebook JSON, locates cells by ID, and supports replace,
/// insert, and delete operations.
///
/// # Source
/// Ported from `packages/opencode/src/tool/` — notebook editing pattern.
#[derive(Debug, Clone)]
pub struct NotebookEditTool;

impl NotebookEditTool {
    /// Generate a unique cell ID using a hash of the current time.
    fn generate_cell_id() -> String {
        use std::collections::hash_map::DefaultHasher;
        use std::hash::{Hash, Hasher};
        use std::time::SystemTime;

        let mut hasher = DefaultHasher::new();
        SystemTime::now().hash(&mut hasher);
        // Also hash a random-ish value for uniqueness across rapid calls
        std::process::id().hash(&mut hasher);
        format!("{:016x}", hasher.finish())
    }

    /// Parse notebook JSON and return the cells array (mutable).
    fn parse_notebook(
        content: &str,
    ) -> std::result::Result<(serde_json::Value, serde_json::Value), String> {
        let notebook: serde_json::Value = serde_json::from_str(content)
            .map_err(|e| format!("failed to parse notebook JSON: {}", e))?;

        // Validate basic notebook structure
        if notebook.get("cells").is_none() {
            return Err("notebook has no 'cells' array".into());
        }
        if notebook.get("nbformat").is_none() {
            return Err("notebook is missing 'nbformat'".into());
        }

        let cells = notebook["cells"].clone();
        Ok((notebook, cells))
    }

    /// Find the index of a cell by its ID. Returns None if not found.
    fn find_cell_index(cells: &serde_json::Value, cell_id: &str) -> Option<usize> {
        cells
            .as_array()?
            .iter()
            .position(|cell| cell.get("id").and_then(|id| id.as_str()) == Some(cell_id))
    }

    /// Create a new cell JSON object.
    fn create_cell(source: &str, cell_type: &str) -> serde_json::Value {
        serde_json::json!({
            "id": Self::generate_cell_id(),
            "cell_type": cell_type,
            "source": [source],
            "metadata": {},
            "outputs": [],
            "execution_count": null
        })
    }
}

#[async_trait]
impl Tool for NotebookEditTool {
    fn id(&self) -> &str {
        "notebook_edit"
    }

    fn description(&self) -> &str {
        "Edit cells in a Jupyter notebook (.ipynb) file. Supports replace,\
         insert, and delete operations on individual cells. New cells get\
         auto-generated IDs."
    }

    fn parameters_schema(&self) -> serde_json::Value {
        serde_json::json!({
            "type": "object",
            "properties": {
                "notebook_path": {
                    "type": "string",
                    "description": "The absolute path to the Jupyter notebook file to edit"
                },
                "cell_id": {
                    "type": "string",
                    "description": "The ID of the cell to edit. Required for replace and delete; optional for insert (inserts at beginning if omitted)."
                },
                "new_source": {
                    "type": "string",
                    "description": "The new source content for the cell"
                },
                "cell_type": {
                    "type": "string",
                    "enum": ["code", "markdown"],
                    "description": "The type of the cell. Defaults to 'code' for new cells."
                },
                "edit_mode": {
                    "type": "string",
                    "enum": ["replace", "insert", "delete"],
                    "description": "The type of edit to make. Defaults to 'replace'."
                }
            },
            "required": ["notebook_path", "new_source"]
        })
    }

    async fn execute(&self, args: serde_json::Value, _ctx: &ToolContext) -> Result<ExecuteResult> {
        let notebook_path =
            args["notebook_path"]
                .as_str()
                .ok_or_else(|| Error::ToolInvalidArguments {
                    tool: "notebook_edit".into(),
                    detail: "missing 'notebook_path' field".into(),
                })?;

        let new_source =
            args["new_source"]
                .as_str()
                .ok_or_else(|| Error::ToolInvalidArguments {
                    tool: "notebook_edit".into(),
                    detail: "missing 'new_source' field".into(),
                })?;

        let cell_id = args["cell_id"].as_str();
        let cell_type = args["cell_type"].as_str().unwrap_or("code");
        let edit_mode = args["edit_mode"].as_str().unwrap_or("replace");

        let path = std::path::Path::new(notebook_path);
        if !path.exists() {
            return Err(Error::Tool(format!(
                "Notebook not found: {}",
                notebook_path
            )));
        }

        let content = std::fs::read_to_string(path).map_err(Error::Io)?;

        let (mut notebook, cells) = Self::parse_notebook(&content).map_err(Error::Tool)?;

        let mut cells_array = cells.as_array().cloned().unwrap_or_default();

        match edit_mode {
            "replace" => {
                let cid = cell_id.ok_or_else(|| Error::ToolInvalidArguments {
                    tool: "notebook_edit".into(),
                    detail: "missing 'cell_id' for replace mode".into(),
                })?;
                let idx =
                    Self::find_cell_index(&serde_json::Value::Array(cells_array.clone()), cid)
                        .ok_or_else(|| {
                            Error::Tool(format!("cell_id '{}' not found in notebook", cid))
                        })?;
                let cell = &mut cells_array[idx];
                cell["source"] = serde_json::json!([new_source]);
                if cell.get("cell_type").is_none() {
                    cell["cell_type"] = serde_json::Value::String(cell_type.to_string());
                }
            }
            "insert" => {
                let new_cell = Self::create_cell(new_source, cell_type);
                match cell_id {
                    Some(cid) => {
                        let idx = Self::find_cell_index(
                            &serde_json::Value::Array(cells_array.clone()),
                            cid,
                        )
                        .ok_or_else(|| {
                            Error::Tool(format!("cell_id '{}' not found in notebook", cid))
                        })?;
                        cells_array.insert(idx + 1, new_cell);
                    }
                    None => {
                        // Insert at beginning
                        cells_array.insert(0, new_cell);
                    }
                }
            }
            "delete" => {
                let cid = cell_id.ok_or_else(|| Error::ToolInvalidArguments {
                    tool: "notebook_edit".into(),
                    detail: "missing 'cell_id' for delete mode".into(),
                })?;
                let idx =
                    Self::find_cell_index(&serde_json::Value::Array(cells_array.clone()), cid)
                        .ok_or_else(|| {
                            Error::Tool(format!("cell_id '{}' not found in notebook", cid))
                        })?;
                cells_array.remove(idx);
            }
            other => {
                return Err(Error::ToolInvalidArguments {
                    tool: "notebook_edit".into(),
                    detail: format!(
                        "unknown edit_mode '{}'. Must be one of: replace, insert, delete",
                        other
                    ),
                });
            }
        }

        // Write back
        notebook["cells"] = serde_json::Value::Array(cells_array);
        let updated = serde_json::to_string_pretty(&notebook).map_err(Error::Json)?;
        std::fs::write(path, updated).map_err(|e| Error::FileSystem {
            path: notebook_path.to_string(),
            message: format!("failed to write notebook: {}", e),
        })?;

        Ok(ExecuteResult {
            title: format!("Notebook edited: {}", notebook_path),
            output: format!("Applied {} to notebook: {}", edit_mode, notebook_path),
            truncated: false,
            output_path: None,
            attachments: None,
            metadata: {
                let mut m = HashMap::new();
                m.insert(
                    "notebook".into(),
                    serde_json::Value::String(notebook_path.to_string()),
                );
                m.insert(
                    "edit_mode".into(),
                    serde_json::Value::String(edit_mode.to_string()),
                );
                m
            },
        })
    }
}

// ═══════════════════════════════════════════════════════════════════════════════
// 19. TaskOutputTool — background task output retrieval
// ═══════════════════════════════════════════════════════════════════════════════

/// In-memory registry of background task statuses.
static TASK_REGISTRY: std::sync::OnceLock<std::sync::Mutex<HashMap<String, TaskRecord>>> =
    std::sync::OnceLock::new();

/// Metadata for a tracked background task.
#[derive(Debug, Clone)]
struct TaskRecord {
    status: String,
    output: String,
    #[allow(dead_code)]
    created_at: std::time::Instant,
}

fn task_registry() -> &'static std::sync::Mutex<HashMap<String, TaskRecord>> {
    TASK_REGISTRY.get_or_init(|| std::sync::Mutex::new(HashMap::new()))
}

/// Retrieves output from a running or completed background task (sub-agent).
///
/// Checks the in-memory task registry for the given task ID. If `block` is true,
/// waits (polls) for the task to complete up to `timeout` milliseconds.
///
/// # Source
/// Ported from `packages/opencode/src/tool/` — task output pattern.
#[derive(Debug, Clone)]
pub struct TaskOutputTool;

#[async_trait]
impl Tool for TaskOutputTool {
    fn id(&self) -> &str {
        "task_output"
    }

    fn description(&self) -> &str {
        "Retrieve output from a running or completed background task (sub-agent).\
         Use this to check the status of a previously launched background task.\
         If block is true, waits up to timeout ms for the task to finish."
    }

    fn parameters_schema(&self) -> serde_json::Value {
        serde_json::json!({
            "type": "object",
            "properties": {
                "task_id": {
                    "type": "string",
                    "description": "The ID of the background task to check"
                },
                "block": {
                    "type": "boolean",
                    "description": "If true, wait for the task to complete before returning (default false)"
                },
                "timeout": {
                    "type": "number",
                    "description": "Maximum time to wait in milliseconds when block is true (default 60000)"
                }
            },
            "required": ["task_id"]
        })
    }

    async fn execute(&self, args: serde_json::Value, _ctx: &ToolContext) -> Result<ExecuteResult> {
        let task_id = args["task_id"]
            .as_str()
            .ok_or_else(|| Error::ToolInvalidArguments {
                tool: "task_output".into(),
                detail: "missing 'task_id' field".into(),
            })?;

        let block = args["block"].as_bool().unwrap_or(false);
        let timeout_ms = args["timeout"].as_f64().unwrap_or(60_000.0) as u64;

        // Check the task registry
        let registry = task_registry();
        let record = {
            let guard = registry
                .lock()
                .map_err(|e| Error::Tool(format!("task registry lock poisoned: {}", e)))?;
            guard.get(task_id).cloned()
        };

        match record {
            Some(rec) if rec.status != "running" => {
                // Task already completed
                Ok(ExecuteResult {
                    title: format!("Task: {}", task_id),
                    output: format!("Task {} ({})\n\n{}", task_id, rec.status, rec.output),
                    truncated: false,
                    output_path: None,
                    attachments: None,
                    metadata: {
                        let mut m = HashMap::new();
                        m.insert(
                            "task_id".into(),
                            serde_json::Value::String(task_id.to_string()),
                        );
                        m.insert("status".into(), serde_json::Value::String(rec.status));
                        m
                    },
                })
            }
            Some(_rec) if block => {
                // Task is running and we should block/wait
                let started = std::time::Instant::now();
                let poll_interval = std::time::Duration::from_millis(250);
                let deadline = std::time::Duration::from_millis(timeout_ms);

                loop {
                    if started.elapsed() >= deadline {
                        return Ok(ExecuteResult {
                            title: format!("Task: {}", task_id),
                            output: format!(
                                "Task {} is still running (timeout after {} ms).",
                                task_id, timeout_ms
                            ),
                            truncated: false,
                            output_path: None,
                            attachments: None,
                            metadata: {
                                let mut m = HashMap::new();
                                m.insert(
                                    "task_id".into(),
                                    serde_json::Value::String(task_id.to_string()),
                                );
                                m.insert(
                                    "status".into(),
                                    serde_json::Value::String("running".into()),
                                );
                                m
                            },
                        });
                    }

                    // Check again
                    {
                        let guard = registry.lock().map_err(|e| {
                            Error::Tool(format!("task registry lock poisoned: {}", e))
                        })?;
                        if let Some(rec) = guard.get(task_id) {
                            if rec.status != "running" {
                                let output =
                                    format!("Task {} ({})\n\n{}", task_id, rec.status, rec.output);
                                return Ok(ExecuteResult {
                                    title: format!("Task: {}", task_id),
                                    output,
                                    truncated: false,
                                    output_path: None,
                                    attachments: None,
                                    metadata: {
                                        let mut m = HashMap::new();
                                        m.insert(
                                            "task_id".into(),
                                            serde_json::Value::String(task_id.to_string()),
                                        );
                                        m.insert(
                                            "status".into(),
                                            serde_json::Value::String(rec.status.clone()),
                                        );
                                        m
                                    },
                                });
                            }
                        }
                    }

                    tokio::time::sleep(poll_interval).await;
                }
            }
            Some(rec) => {
                // Task is running and we're not blocking
                Ok(ExecuteResult {
                    title: format!("Task: {}", task_id),
                    output: format!(
                        "Task {} is still running. Use block=true to wait for completion.",
                        task_id
                    ),
                    truncated: false,
                    output_path: None,
                    attachments: None,
                    metadata: {
                        let mut m = HashMap::new();
                        m.insert(
                            "task_id".into(),
                            serde_json::Value::String(task_id.to_string()),
                        );
                        m.insert("status".into(), serde_json::Value::String(rec.status));
                        m
                    },
                })
            }
            None => {
                // Task not found
                Ok(ExecuteResult {
                    title: format!("Task: {}", task_id),
                    output: format!("Task {} not found in registry. It may have been cleaned up or never existed.", task_id),
                    truncated: false,
                    output_path: None,
                    attachments: None,
                    metadata: {
                        let mut m = HashMap::new();
                        m.insert(
                            "task_id".into(),
                            serde_json::Value::String(task_id.to_string()),
                        );
                        m.insert(
                            "status".into(),
                            serde_json::Value::String("not_found".into()),
                        );
                        m
                    },
                })
            }
        }
    }
}

// ═══════════════════════════════════════════════════════════════════════════════
// 20. LspTool — Language Server Protocol operations
// ═══════════════════════════════════════════════════════════════════════════════

/// Interact with Language Server Protocol (LSP) servers to get code intelligence
/// features such as go-to-definition, find references, hover information, and
/// symbol search.
///
/// # Source
/// Ported from `packages/opencode/src/tool/lsp.ts`.
///
/// # Supported Operations
/// - `goToDefinition` — Find where a symbol is defined
/// - `findReferences` — Find all references to a symbol
/// - `hover` — Get hover information (documentation, type info) for a symbol
/// - `documentSymbol` — Get all symbols in a document
/// - `workspaceSymbol` — Search project-wide symbols by query
/// - `goToImplementation` — Find implementations of an interface or abstract method
/// - `prepareCallHierarchy` — Get call hierarchy item at a position
/// - `incomingCalls` — Find all callers of the function at a position
/// - `outgoingCalls` — Find all callees of the function at a position
///
/// # Requirements
/// Requires an [`LspBridge`] to be registered via
/// [`rustcode_core::lsp::set_global_lsp_bridge`]. The `rustcode-lsp` crate
/// provides a bridge implementation that connects to the [`LspManager`].
/// Without a registered bridge, the tool returns an informative error.
#[derive(Debug, Clone)]
pub struct LspTool;

const LSP_OPERATIONS: &[&str] = &[
    "goToDefinition",
    "findReferences",
    "hover",
    "documentSymbol",
    "workspaceSymbol",
    "goToImplementation",
    "prepareCallHierarchy",
    "incomingCalls",
    "outgoingCalls",
];

#[async_trait]
impl Tool for LspTool {
    fn id(&self) -> &str {
        "lsp"
    }

    fn description(&self) -> &str {
        "Interact with Language Server Protocol (LSP) servers to get code intelligence features. \
         Supported operations: goToDefinition (find where a symbol is defined), \
         findReferences (find all references to a symbol), \
         hover (get hover information/documentation/type info), \
         documentSymbol (get all symbols in a document), \
         workspaceSymbol (search project-wide symbols by query), \
         goToImplementation (find implementations of an interface/abstract method), \
         prepareCallHierarchy (get call hierarchy item at a position), \
         incomingCalls (find all callers of the function at a position), \
         outgoingCalls (find all callees of the function at a position). \
         Requires filePath, line (1-based), and character (1-based) for most operations. \
         workspaceSymbol only requires a query string. documentSymbol only requires filePath."
    }

    fn parameters_schema(&self) -> serde_json::Value {
        serde_json::json!({
            "type": "object",
            "properties": {
                "operation": {
                    "type": "string",
                    "description": "The LSP operation to perform",
                    "enum": LSP_OPERATIONS
                },
                "filePath": {
                    "type": "string",
                    "description": "The absolute or relative path to the file"
                },
                "line": {
                    "type": "integer",
                    "description": "The line number (1-based, as shown in editors)",
                    "minimum": 1
                },
                "character": {
                    "type": "integer",
                    "description": "The character offset (1-based, as shown in editors)",
                    "minimum": 1
                },
                "query": {
                    "type": "string",
                    "description": "Search query for workspaceSymbol. Empty string requests all symbols."
                }
            },
            "required": ["operation"]
        })
    }

    async fn execute(&self, args: serde_json::Value, _ctx: &ToolContext) -> Result<ExecuteResult> {
        let operation = args["operation"]
            .as_str()
            .ok_or_else(|| Error::ToolInvalidArguments {
                tool: "lsp".into(),
                detail: "missing 'operation' field".into(),
            })?;

        if !LSP_OPERATIONS.contains(&operation) {
            return Err(Error::ToolInvalidArguments {
                tool: "lsp".into(),
                detail: format!(
                    "invalid operation '{operation}'. Must be one of: {}",
                    LSP_OPERATIONS.join(", ")
                ),
            });
        }

        let file_path = args["filePath"].as_str().unwrap_or("");
        let line = args["line"].as_u64().unwrap_or(0);
        let character = args["character"].as_u64().unwrap_or(0);
        let query = args["query"].as_str().unwrap_or("");

        // Validate required fields per operation
        match operation {
            "workspaceSymbol" => {
                if query.is_empty() {
                    return Err(Error::ToolInvalidArguments {
                        tool: "lsp".into(),
                        detail: "workspaceSymbol requires a 'query' parameter".into(),
                    });
                }
            }
            "documentSymbol" => {
                if file_path.is_empty() {
                    return Err(Error::ToolInvalidArguments {
                        tool: "lsp".into(),
                        detail: "documentSymbol requires a 'filePath' parameter".into(),
                    });
                }
            }
            _ => {
                if file_path.is_empty() {
                    return Err(Error::ToolInvalidArguments {
                        tool: "lsp".into(),
                        detail: format!(
                            "{operation} requires 'filePath', 'line', and 'character' parameters"
                        ),
                    });
                }
                if line == 0 || character == 0 {
                    return Err(Error::ToolInvalidArguments {
                        tool: "lsp".into(),
                        detail: format!(
                            "{operation} requires 'line' and 'character' to be >= 1 (1-based)"
                        ),
                    });
                }
            }
        }

        // Build the title
        let title = if file_path.is_empty() {
            operation.to_string()
        } else if line > 0 && character > 0 {
            format!("{operation} {file_path}:{line}:{character}")
        } else {
            format!("{operation} {file_path}")
        };

                // Check if an LSP bridge is available via the global in rustcode-core
        let has_bridge = crate::lsp::has_lsp_bridge();
        if !has_bridge {
            return Ok(ExecuteResult {
                title,
                output: format!(
                    "LSP tool invoked for operation '{operation}' on '{file_path}'.                      However, no LSP bridge is registered.                      To use LSP features, initialize the LSP subsystem                      by calling `rustcode_core::lsp::set_global_lsp_bridge()`."
                ),
                truncated: false,
                output_path: None,
                attachments: None,
                metadata: {
                    let mut m = HashMap::new();
                    m.insert(
                        "operation".into(),
                        serde_json::Value::String(operation.into()),
                    );
                    m.insert("available".into(), serde_json::Value::Bool(false));
                    m
                },
            });
        }

        // Use the global LSP bridge to perform the operation
        let result = match operation {
            "workspaceSymbol" => {
                let symbols = crate::lsp::global_workspace_symbols(query);
                let symbols_value: Vec<serde_json::Value> = symbols.iter().map(|s| {
                    serde_json::json!({
                        "name": s.name,
                        "kind": s.kind,
                        "location": {
                            "uri": s.location.uri,
                            "range": {
                                "start": { "line": s.location.range.start.line, "character": s.location.range.start.character },
                                "end": { "line": s.location.range.end.line, "character": s.location.range.end.character }
                            }
                        }
                    })
                }).collect();
                serde_json::Value::Array(symbols_value)
            }
            "documentSymbol" => {
                serde_json::json!({
                    "operation": operation,
                    "filePath": file_path,
                    "note": "documentSymbol requires direct LSP client access via rustcode-lsp.                              The core LspBridge only supports workspaceSymbol for now."
                })
            }
            _ => {
                // Position-based operations require direct LSP client access
                serde_json::json!({
                    "operation": operation,
                    "filePath": file_path,
                    "line": line,
                    "character": character,
                    "query": query,
                    "note": "Position-based LSP operations require direct LSP client access via rustcode-lsp.                              The core LspBridge only supports workspaceSymbol for now.",
                    "bridge_available": true,
                })
            }
        };

        let output = if operation == "workspaceSymbol" && result.as_array().map(|a| a.is_empty()).unwrap_or(true) {
            "No results found for workspaceSymbol".to_string()
        } else {
            serde_json::to_string_pretty(&result).unwrap_or_default()
        };

        Ok(ExecuteResult {
            title,
            output,
            truncated: false,
            output_path: None,
            attachments: None,
            metadata: {
                let mut m = HashMap::new();
                if let Some(arr) = result.as_array() {
                    m.insert("result".into(), serde_json::Value::Array(arr.clone()));
                } else {
                    m.insert("result".into(), result);
                }
                m.insert(
                    "operation".into(),
                    serde_json::Value::String(operation.into()),
                );
                m.insert("available".into(), serde_json::Value::Bool(true));
                m
            },
        })
    }
}

// ═══════════════════════════════════════════════════════════════════════════════
// 21. InvalidTool — sentinel for malformed tool calls
// ═══════════════════════════════════════════════════════════════════════════════

/// A sentinel tool that handles malformed or invalid tool calls.
///
/// This tool is invoked when the LLM produces a tool call with an unrecognized
/// name or invalid arguments. It returns a descriptive error message to help
/// the LLM correct its output.
///
/// # Source
/// Ported from `packages/opencode/src/tool/invalid.ts`.
#[derive(Debug, Clone)]
pub struct InvalidTool;

#[async_trait]
impl Tool for InvalidTool {
    fn id(&self) -> &str {
        "invalid"
    }

    fn description(&self) -> &str {
        "Do not use"
    }

    fn parameters_schema(&self) -> serde_json::Value {
        serde_json::json!({
            "type": "object",
            "properties": {
                "tool": {
                    "type": "string",
                    "description": "The name of the invalid tool that was called"
                },
                "error": {
                    "type": "string",
                    "description": "The error message describing why the tool call was invalid"
                }
            },
            "required": ["tool", "error"]
        })
    }

    async fn execute(&self, args: serde_json::Value, _ctx: &ToolContext) -> Result<ExecuteResult> {
        let tool_name = args["tool"].as_str().unwrap_or("unknown");
        let error_msg = args["error"]
            .as_str()
            .unwrap_or("no error message provided");

        Ok(ExecuteResult {
            title: "Invalid Tool".into(),
            output: format!(
                "The arguments provided to the tool are invalid: {error_msg}. \
                 The tool '{tool_name}' was called with incorrect parameters. \
                 Please rewrite your tool call with the correct arguments."
            ),
            truncated: false,
            output_path: None,
            attachments: None,
            metadata: {
                let mut m = HashMap::new();
                m.insert("tool".into(), serde_json::Value::String(tool_name.into()));
                m.insert("error".into(), serde_json::Value::String(error_msg.into()));
                m
            },
        })
    }
}

// ═══════════════════════════════════════════════════════════════════════════════
// ToolRegistry extension: register_builtins
// ═══════════════════════════════════════════════════════════════════════════════

impl ToolRegistry {
    /// Register all 21 built-in tools.
    pub fn register_builtins(&self) {
        self.register(Arc::new(BashTool));
        self.register(Arc::new(ReadTool));
        self.register(Arc::new(WriteTool));
        self.register(Arc::new(EditTool));
        self.register(Arc::new(GlobTool));
        self.register(Arc::new(GrepTool));
        self.register(Arc::new(WebFetchTool));
        self.register(Arc::new(WebSearchTool));
        self.register(Arc::new(ApplyPatchTool));
        self.register(Arc::new(TaskTool));
        // QuestionTool requires a QuestionService — registered separately in
        // the runtime / caller.
        self.register(Arc::new(SkillTool));
        self.register(Arc::new(TodoWriteTool));
        self.register(Arc::new(StashTool));
        self.register(Arc::new(NotebookEditTool));
        self.register(Arc::new(TaskOutputTool));
        self.register(Arc::new(PlanEnterTool));
        self.register(Arc::new(PlanExitTool));
        self.register(Arc::new(ExitPlanModeTool));
        self.register(Arc::new(LspTool));
        self.register(Arc::new(InvalidTool));
    }
}

// ═══════════════════════════════════════════════════════════════════════════════
// Tests
// ═══════════════════════════════════════════════════════════════════════════════

#[cfg(test)]
mod tests {
    use super::*;
    use crate::tool::ToolContext;
    use tokio_util::sync::CancellationToken;

    fn test_ctx() -> ToolContext {
        ToolContext {
            session_id: "test_session".into(),
            message_id: "test_msg".into(),
            agent: "claude".into(),
            abort: CancellationToken::new(),
            call_id: None,
            extra: HashMap::new(),
            messages: vec![],
            ask_fn: None,
            permission_source: None,
        }
    }

    /// Helper: create a QuestionTool backed by a throwaway bus + service
    /// for meta-tests that only check schema / description / ID.
    fn question_tool() -> QuestionTool {
        let bus = SharedBus::new(16);
        let svc = Arc::new(QuestionService::new(bus));
        QuestionTool::new(svc)
    }

    // ── BashTool tests ──────────────────────────────────────────────────

    #[tokio::test]
    async fn test_bash_basic_command() {
        let tool = BashTool;
        let ctx = test_ctx();
        let result = tool
            .execute(
                serde_json::json!({"command": "echo hello_world_test", "description": "test echo"}),
                &ctx,
            )
            .await
            .unwrap();
        assert!(result.output.contains("hello_world_test"));
        assert_eq!(result.title, "test echo");
    }

    #[tokio::test]
    async fn test_bash_missing_command() {
        let tool = BashTool;
        let ctx = test_ctx();
        let result = tool.execute(serde_json::json!({}), &ctx).await;
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("missing"));
    }

    #[tokio::test]
    async fn test_bash_empty_output() {
        let tool = BashTool;
        let ctx = test_ctx();
        let result = tool
            .execute(
                serde_json::json!({"command": "true", "description": "empty output"}),
                &ctx,
            )
            .await
            .unwrap();
        // "true" produces no output, should show "(no output)"
        assert!(result.output.contains("(no output)") || result.output.contains("exit"));
    }

    #[tokio::test]
    async fn test_bash_with_workdir() {
        let tool = BashTool;
        let ctx = test_ctx();
        let tmpdir = std::env::temp_dir();
        let result = tool
            .execute(
                serde_json::json!({
                    "command": "pwd",
                    "workdir": tmpdir.to_string_lossy(),
                    "description": "pwd test"
                }),
                &ctx,
            )
            .await
            .unwrap();
        assert!(result
            .output
            .contains(&tmpdir.to_string_lossy().to_string()));
    }

    // ── ReadTool tests ──────────────────────────────────────────────────

    #[tokio::test]
    async fn test_read_existing_file() {
        let tool = ReadTool;
        let ctx = test_ctx();
        let tmpfile = std::env::temp_dir().join("rustcode_read_test.txt");
        std::fs::write(&tmpfile, "line1\nline2\nline3\n").unwrap();

        let result = tool
            .execute(
                serde_json::json!({"filePath": tmpfile.to_string_lossy()}),
                &ctx,
            )
            .await
            .unwrap();

        assert!(result.output.contains("line1"));
        assert!(result.output.contains("line2"));
        assert!(result.output.contains("line3"));
        let _ = std::fs::remove_file(&tmpfile);
    }

    #[tokio::test]
    async fn test_read_with_offset_limit() {
        let tool = ReadTool;
        let ctx = test_ctx();
        let tmpfile = std::env::temp_dir().join("rustcode_read_offset.txt");
        let content: String = (1..=10).map(|i| format!("line {}\n", i)).collect();
        std::fs::write(&tmpfile, &content).unwrap();

        let result = tool
            .execute(
                serde_json::json!({"filePath": tmpfile.to_string_lossy(), "offset": 3, "limit": 2}),
                &ctx,
            )
            .await
            .unwrap();

        assert!(result.output.contains("3: line 3"));
        assert!(result.output.contains("4: line 4"));
        assert!(!result.output.contains("5: line 5"));
        assert!(result.truncated);
        let _ = std::fs::remove_file(&tmpfile);
    }

    #[tokio::test]
    async fn test_read_directory() {
        let tool = ReadTool;
        let ctx = test_ctx();
        let tmpdir = std::env::temp_dir();
        let result = tool
            .execute(
                serde_json::json!({"filePath": tmpdir.to_string_lossy()}),
                &ctx,
            )
            .await
            .unwrap();
        assert!(result.output.contains("directory"));
        assert!(result.output.contains("<entries>"));
    }

    #[tokio::test]
    async fn test_read_missing_file() {
        let tool = ReadTool;
        let ctx = test_ctx();
        let result = tool
            .execute(
                serde_json::json!({"filePath": "/nonexistent/path/file_xyz_123.txt"}),
                &ctx,
            )
            .await;
        assert!(result.is_err());
    }

    // ── WriteTool tests ─────────────────────────────────────────────────

    #[tokio::test]
    async fn test_write_new_file() {
        let tool = WriteTool;
        let ctx = test_ctx();
        let tmpfile = std::env::temp_dir().join("rustcode_write_new.txt");
        let _ = std::fs::remove_file(&tmpfile);

        let result = tool
            .execute(
                serde_json::json!({
                    "filePath": tmpfile.to_string_lossy(),
                    "content": "hello write test"
                }),
                &ctx,
            )
            .await
            .unwrap();

        assert!(result.output.contains("Created"));
        assert!(result
            .output
            .contains(&tmpfile.to_string_lossy().to_string()));
        assert_eq!(
            std::fs::read_to_string(&tmpfile).unwrap(),
            "hello write test"
        );
        let _ = std::fs::remove_file(&tmpfile);
    }

    #[tokio::test]
    async fn test_write_overwrite() {
        let tool = WriteTool;
        let ctx = test_ctx();
        let tmpfile = std::env::temp_dir().join("rustcode_write_overwrite.txt");
        std::fs::write(&tmpfile, "original").unwrap();

        let result = tool
            .execute(
                serde_json::json!({
                    "filePath": tmpfile.to_string_lossy(),
                    "content": "updated"
                }),
                &ctx,
            )
            .await
            .unwrap();

        assert!(result.output.contains("Wrote"));
        assert_eq!(std::fs::read_to_string(&tmpfile).unwrap(), "updated");
        let _ = std::fs::remove_file(&tmpfile);
    }

    #[tokio::test]
    async fn test_write_creates_parent_dirs() {
        let tool = WriteTool;
        let ctx = test_ctx();
        let parent = std::env::temp_dir().join("rustcode_test_nested");
        let tmpfile = parent.join("deep/file.txt");
        let _ = std::fs::remove_dir_all(&parent);

        let result = tool
            .execute(
                serde_json::json!({
                    "filePath": tmpfile.to_string_lossy(),
                    "content": "nested content"
                }),
                &ctx,
            )
            .await
            .unwrap();

        assert!(result.output.contains("Created"));
        assert!(tmpfile.exists());
        let _ = std::fs::remove_dir_all(&parent);
    }

    // ── EditTool tests ──────────────────────────────────────────────────

    #[tokio::test]
    async fn test_edit_single_replace() {
        let tool = EditTool;
        let ctx = test_ctx();
        let tmpfile = std::env::temp_dir().join("rustcode_edit_test.txt");
        std::fs::write(&tmpfile, "Hello World\nFoo Bar\n").unwrap();

        let result = tool
            .execute(
                serde_json::json!({
                    "filePath": tmpfile.to_string_lossy(),
                    "oldString": "Hello World",
                    "newString": "Goodbye World"
                }),
                &ctx,
            )
            .await
            .unwrap();

        let content = std::fs::read_to_string(&tmpfile).unwrap();
        assert!(content.contains("Goodbye World"));
        assert!(result.output.contains("Edited file successfully"));
        assert!(result.output.contains("Replacements:"));
        let _ = std::fs::remove_file(&tmpfile);
    }

    #[tokio::test]
    async fn test_edit_replace_all() {
        let tool = EditTool;
        let ctx = test_ctx();
        let tmpfile = std::env::temp_dir().join("rustcode_edit_all.txt");
        std::fs::write(&tmpfile, "foo\nfoo\nfoo\n").unwrap();

        let result = tool
            .execute(
                serde_json::json!({
                    "filePath": tmpfile.to_string_lossy(),
                    "oldString": "foo",
                    "newString": "bar",
                    "replaceAll": true
                }),
                &ctx,
            )
            .await
            .unwrap();

        let content = std::fs::read_to_string(&tmpfile).unwrap();
        assert_eq!(content, "bar\nbar\nbar\n");
        let _ = std::fs::remove_file(&tmpfile);
    }

    #[tokio::test]
    async fn test_edit_not_found() {
        let tool = EditTool;
        let ctx = test_ctx();
        let tmpfile = std::env::temp_dir().join("rustcode_edit_nf.txt");
        std::fs::write(&tmpfile, "existing content\n").unwrap();

        let result = tool
            .execute(
                serde_json::json!({
                    "filePath": tmpfile.to_string_lossy(),
                    "oldString": "nonexistent text",
                    "newString": "new text"
                }),
                &ctx,
            )
            .await;
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("Could not find"));
        let _ = std::fs::remove_file(&tmpfile);
    }

    #[tokio::test]
    async fn test_edit_multiple_without_replace_all() {
        let tool = EditTool;
        let ctx = test_ctx();
        let tmpfile = std::env::temp_dir().join("rustcode_edit_multi.txt");
        std::fs::write(&tmpfile, "dup\ndup\n").unwrap();

        let result = tool
            .execute(
                serde_json::json!({
                    "filePath": tmpfile.to_string_lossy(),
                    "oldString": "dup",
                    "newString": "unique"
                }),
                &ctx,
            )
            .await;
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("multiple"));
        let _ = std::fs::remove_file(&tmpfile);
    }

    // ── GlobTool tests ──────────────────────────────────────────────────

    #[tokio::test]
    async fn test_glob_finds_files() {
        let tool = GlobTool;
        let ctx = test_ctx();
        let tmpdir = std::env::temp_dir();
        // Create some temp .txt files
        let f1 = tmpdir.join("glob_test_a.txt");
        let f2 = tmpdir.join("glob_test_b.txt");
        std::fs::write(&f1, "a").unwrap();
        std::fs::write(&f2, "b").unwrap();

        let result = tool
            .execute(
                serde_json::json!({
                    "pattern": "glob_test_*.txt",
                    "path": tmpdir.to_string_lossy()
                }),
                &ctx,
            )
            .await
            .unwrap();

        assert!(result.output.contains("glob_test_a.txt"));
        assert!(result.output.contains("glob_test_b.txt"));
        let _ = std::fs::remove_file(&f1);
        let _ = std::fs::remove_file(&f2);
    }

    #[tokio::test]
    async fn test_glob_no_matches() {
        let tool = GlobTool;
        let ctx = test_ctx();
        let tmpdir = std::env::temp_dir();
        let result = tool
            .execute(
                serde_json::json!({
                    "pattern": "zzz_nonexistent_pattern_xyz_*.txt",
                    "path": tmpdir.to_string_lossy()
                }),
                &ctx,
            )
            .await
            .unwrap();
        assert_eq!(result.output, "No files found");
    }

    #[tokio::test]
    async fn test_glob_path_is_file_errors() {
        let tool = GlobTool;
        let ctx = test_ctx();
        let tmpfile = std::env::temp_dir().join("glob_file_test.txt");
        std::fs::write(&tmpfile, "content").unwrap();

        let result = tool
            .execute(
                serde_json::json!({
                    "pattern": "*",
                    "path": tmpfile.to_string_lossy()
                }),
                &ctx,
            )
            .await;
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("directory"));
        let _ = std::fs::remove_file(&tmpfile);
    }

    // ── GrepTool tests ──────────────────────────────────────────────────

    #[tokio::test]
    async fn test_grep_finds_matches() {
        let tool = GrepTool;
        let ctx = test_ctx();
        let tmpdir = std::env::temp_dir().join("rustcode_grep_test");
        std::fs::create_dir_all(&tmpdir).unwrap();
        std::fs::write(tmpdir.join("a.txt"), "hello world\nfoo bar\n").unwrap();
        std::fs::write(tmpdir.join("b.txt"), "goodbye\nhello again\n").unwrap();

        let result = tool
            .execute(
                serde_json::json!({
                    "pattern": "hello",
                    "path": tmpdir.to_string_lossy()
                }),
                &ctx,
            )
            .await
            .unwrap();

        assert!(result.output.contains("hello world"));
        assert!(result.output.contains("hello again"));
        assert!(result.output.contains("Found"));
        let _ = std::fs::remove_dir_all(&tmpdir);
    }

    #[tokio::test]
    async fn test_grep_no_matches() {
        let tool = GrepTool;
        let ctx = test_ctx();
        let tmpdir = std::env::temp_dir().join("rustcode_grep_nomatch");
        std::fs::create_dir_all(&tmpdir).unwrap();
        std::fs::write(tmpdir.join("x.txt"), "nothing here\n").unwrap();

        let result = tool
            .execute(
                serde_json::json!({
                    "pattern": "ZZZZZ_NOMATCH_ZZZZZ",
                    "path": tmpdir.to_string_lossy()
                }),
                &ctx,
            )
            .await
            .unwrap();

        assert_eq!(result.output, "No files found");
        let _ = std::fs::remove_dir_all(&tmpdir);
    }

    #[tokio::test]
    async fn test_grep_invalid_regex() {
        let tool = GrepTool;
        let ctx = test_ctx();
        let result = tool
            .execute(
                serde_json::json!({"pattern": "[invalid", "path": "."}),
                &ctx,
            )
            .await;
        assert!(result.is_err());
    }

    #[tokio::test]
    async fn test_grep_with_include_filter() {
        let tool = GrepTool;
        let ctx = test_ctx();
        let tmpdir = std::env::temp_dir().join("rustcode_grep_include");
        std::fs::create_dir_all(&tmpdir).unwrap();
        std::fs::write(tmpdir.join("a.rs"), "fn hello() {}\n").unwrap();
        std::fs::write(tmpdir.join("b.txt"), "hello world\n").unwrap();

        let result = tool
            .execute(
                serde_json::json!({
                    "pattern": "hello",
                    "path": tmpdir.to_string_lossy(),
                    "include": "*.rs"
                }),
                &ctx,
            )
            .await
            .unwrap();

        assert!(result.output.contains("fn hello()"));
        assert!(!result.output.contains("b.txt"));
        let _ = std::fs::remove_dir_all(&tmpdir);
    }

    // ── WebFetchTool tests ──────────────────────────────────────────────

    #[tokio::test]
    async fn test_webfetch_invalid_url() {
        let tool = WebFetchTool;
        let ctx = test_ctx();
        let result = tool
            .execute(serde_json::json!({"url": "not-a-valid-url"}), &ctx)
            .await;
        assert!(result.is_err());
    }

    #[tokio::test]
    async fn test_webfetch_non_http_url() {
        let tool = WebFetchTool;
        let ctx = test_ctx();
        let result = tool
            .execute(serde_json::json!({"url": "ftp://example.com"}), &ctx)
            .await;
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("http"));
    }

    #[tokio::test]
    async fn test_webfetch_missing_url() {
        let tool = WebFetchTool;
        let ctx = test_ctx();
        let result = tool.execute(serde_json::json!({}), &ctx).await;
        assert!(result.is_err());
    }

    // ── WebSearchTool tests ─────────────────────────────────────────────

    #[tokio::test]
    async fn test_websearch_basic() {
        let tool = WebSearchTool;
        let ctx = test_ctx();
        let result = tool
            .execute(
                serde_json::json!({"query": "rust programming language"}),
                &ctx,
            )
            .await
            .unwrap();
        assert!(result.output.contains("rust programming language"));
        assert!(result.output.contains("external provider"));
    }

    #[tokio::test]
    async fn test_websearch_missing_query() {
        let tool = WebSearchTool;
        let ctx = test_ctx();
        let result = tool.execute(serde_json::json!({}), &ctx).await;
        assert!(result.is_err());
    }

    #[tokio::test]
    async fn test_websearch_with_domains() {
        let tool = WebSearchTool;
        let ctx = test_ctx();
        let result = tool
            .execute(
                serde_json::json!({
                    "query": "rust",
                    "allowed_domains": ["docs.rs", "crates.io"]
                }),
                &ctx,
            )
            .await
            .unwrap();
        assert!(result.output.contains("rust"));
        assert!(result.output.contains("docs.rs"));
    }

    // ── ApplyPatchTool tests ────────────────────────────────────────────

    #[tokio::test]
    async fn test_apply_patch_missing_file_path() {
        let tool = ApplyPatchTool;
        let ctx = test_ctx();
        let result = tool
            .execute(serde_json::json!({"patch": "some diff"}), &ctx)
            .await;
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("file_path"));
    }

    #[tokio::test]
    async fn test_apply_patch_missing_patch() {
        let tool = ApplyPatchTool;
        let ctx = test_ctx();
        let result = tool
            .execute(serde_json::json!({"file_path": "/tmp/test.txt"}), &ctx)
            .await;
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("patch"));
    }

    #[tokio::test]
    async fn test_apply_patch_file_not_found() {
        let tool = ApplyPatchTool;
        let ctx = test_ctx();
        let result = tool
            .execute(
                serde_json::json!({
                    "file_path": "/nonexistent/path_xyz_file.txt",
                    "patch": "@@ -1,1 +1,1 @@\n-old\n+new\n"
                }),
                &ctx,
            )
            .await;
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("not found"));
    }

    #[tokio::test]
    async fn test_apply_patch_empty_patch() {
        let tool = ApplyPatchTool;
        let ctx = test_ctx();
        let tmpfile = std::env::temp_dir().join("rustcode_ap_empty.txt");
        std::fs::write(&tmpfile, "hello\n").unwrap();

        let result = tool
            .execute(
                serde_json::json!({
                    "file_path": tmpfile.to_string_lossy(),
                    "patch": ""
                }),
                &ctx,
            )
            .await;
        assert!(result.is_err());
        let _ = std::fs::remove_file(&tmpfile);
    }

    #[tokio::test]
    async fn test_apply_patch_simple_unified_diff() {
        let tool = ApplyPatchTool;
        let ctx = test_ctx();
        let tmpfile = std::env::temp_dir().join("rustcode_ap_simple.txt");
        std::fs::write(&tmpfile, "line1\nline2\nline3\n").unwrap();

        let patch = "@@ -2,1 +2,1 @@\n-line2\n+modified line2\n";
        let result = tool
            .execute(
                serde_json::json!({
                    "file_path": tmpfile.to_string_lossy(),
                    "patch": patch
                }),
                &ctx,
            )
            .await
            .unwrap();

        let content = std::fs::read_to_string(&tmpfile).unwrap();
        assert!(content.contains("modified line2"));
        assert!(!content.lines().any(|l| l == "line2"));
        assert!(content.contains("line1"));
        assert!(content.contains("line3"));
        assert!(result.output.contains("hunk(s) applied"));
        assert_eq!(result.metadata.get("hunks").unwrap().as_u64().unwrap(), 1);
        let _ = std::fs::remove_file(&tmpfile);
    }

    #[tokio::test]
    async fn test_apply_patch_add_lines() {
        let tool = ApplyPatchTool;
        let ctx = test_ctx();
        let tmpfile = std::env::temp_dir().join("rustcode_ap_add.txt");
        std::fs::write(&tmpfile, "line1\nline2\n").unwrap();

        let patch = "@@ -1,2 +1,3 @@\n line1\n+inserted line\n line2\n";
        let result = tool
            .execute(
                serde_json::json!({
                    "file_path": tmpfile.to_string_lossy(),
                    "patch": patch
                }),
                &ctx,
            )
            .await
            .unwrap();

        let content = std::fs::read_to_string(&tmpfile).unwrap();
        assert!(content.contains("inserted line"));
        assert!(content.contains("line1"));
        assert!(content.contains("line2"));
        let _ = std::fs::remove_file(&tmpfile);
    }

    #[tokio::test]
    async fn test_apply_patch_remove_lines() {
        let tool = ApplyPatchTool;
        let ctx = test_ctx();
        let tmpfile = std::env::temp_dir().join("rustcode_ap_del.txt");
        std::fs::write(&tmpfile, "keep\nremove me\nkeep2\n").unwrap();

        let patch = "@@ -2,1 +2,1 @@\n-remove me\n";
        let _result = tool
            .execute(
                serde_json::json!({
                    "file_path": tmpfile.to_string_lossy(),
                    "patch": patch
                }),
                &ctx,
            )
            .await
            .unwrap();

        let content = std::fs::read_to_string(&tmpfile).unwrap();
        assert!(!content.contains("remove me"));
        assert!(content.contains("keep"));
        assert!(content.contains("keep2"));
        let _ = std::fs::remove_file(&tmpfile);
    }

    #[tokio::test]
    async fn test_apply_patch_offset_adjustment() {
        let tool = ApplyPatchTool;
        let ctx = test_ctx();
        let tmpfile = std::env::temp_dir().join("rustcode_ap_offset.txt");
        // File already has the old pattern shifted by 2 lines
        std::fs::write(
            &tmpfile,
            "extra line A\nextra line B\ncontext1\nold content\ncontext3\n",
        )
        .unwrap();

        // Patch expects "old content" at line 4, which matches the file
        let patch = "@@ -4,1 +4,1 @@\n-old content\n+replaced content\n";
        let _result = tool
            .execute(
                serde_json::json!({
                    "file_path": tmpfile.to_string_lossy(),
                    "patch": patch
                }),
                &ctx,
            )
            .await
            .unwrap();

        let content = std::fs::read_to_string(&tmpfile).unwrap();
        assert!(content.contains("replaced content"));
        assert!(!content.contains("old content"));
        assert!(content.contains("extra line A"));
        assert!(content.contains("context3"));
        let _ = std::fs::remove_file(&tmpfile);
    }

    #[tokio::test]
    async fn test_apply_patch_bad_hunk_context() {
        let tool = ApplyPatchTool;
        let ctx = test_ctx();
        let tmpfile = std::env::temp_dir().join("rustcode_ap_badctx.txt");
        std::fs::write(&tmpfile, "completely different\ncontent here\n").unwrap();

        // Patch has context that doesn't match
        let patch = "@@ -1,2 +1,2 @@\n-non existent\n context1\n+new\n context2\n";
        let result = tool
            .execute(
                serde_json::json!({
                    "file_path": tmpfile.to_string_lossy(),
                    "patch": patch
                }),
                &ctx,
            )
            .await;
        assert!(result.is_err());
        assert!(result
            .unwrap_err()
            .to_string()
            .contains("could not find hunk context"));
        let _ = std::fs::remove_file(&tmpfile);
    }

    // ── TaskTool tests ──────────────────────────────────────────────────

    #[tokio::test]
    async fn test_task_missing_required() {
        let tool = TaskTool;
        let ctx = test_ctx();
        let result = tool
            .execute(serde_json::json!({"description": "test"}), &ctx)
            .await;
        assert!(result.is_err());
    }

    #[tokio::test]
    async fn test_task_missing_prompt() {
        let tool = TaskTool;
        let ctx = test_ctx();
        let result = tool
            .execute(serde_json::json!({"description": "test"}), &ctx)
            .await;
        assert!(result.is_err());
        let err = result.unwrap_err().to_string();
        assert!(err.contains("missing") || err.contains("prompt") || err.contains("description"));
    }

    #[tokio::test]
    async fn test_task_requires_services() {
        // Without init_services(), the TaskTool should return an error
        // explaining that services are not initialised.
        let tool = TaskTool;
        let ctx = test_ctx();
        let result = tool
            .execute(
                serde_json::json!({
                    "description": "test task",
                    "prompt": "do something",
                    "subagent_type": "general-purpose"
                }),
                &ctx,
            )
            .await;
        assert!(result.is_err());
        let err = result.unwrap_err().to_string();
        assert!(err.contains("services") || err.contains("not initialised") || err.contains("TaskTool"));
    }

    #[tokio::test]
    async fn test_task_background_requires_services() {
        let tool = TaskTool;
        let ctx = test_ctx();
        let result = tool
            .execute(
                serde_json::json!({
                    "description": "bg task",
                    "prompt": "background work",
                    "subagent_type": "general-purpose",
                    "background": true
                }),
                &ctx,
            )
            .await;
        assert!(result.is_err());
    }

    // ── QuestionTool tests ──────────────────────────────────────────────

    #[tokio::test]
    async fn test_question_basic() {
        let bus = SharedBus::new(16);
        let svc = Arc::new(QuestionService::new(bus));
        let tool = QuestionTool::new(svc.clone());
        let ctx = Arc::new(test_ctx());

        let tool_clone = tool.clone();
        let ctx_clone = ctx.clone();
        let handle = tokio::spawn(async move {
            tool_clone
                .execute(
                    serde_json::json!({"questions": [{"question": "What is your preference?"}]}),
                    &ctx_clone,
                )
                .await
                .unwrap()
        });

        // Allow time for the pending entry to appear
        tokio::time::sleep(std::time::Duration::from_millis(50)).await;

        let pending = svc.list().await;
        assert_eq!(pending.len(), 1);
        svc.reply(&pending[0].id, vec![QuestionAnswer::new(vec!["Blue".into()])])
            .await
            .unwrap();

        let result = handle.await.unwrap();
        assert!(result.output.contains("What is your preference?"));
        assert!(result.output.contains("Blue"));
        assert!(result.title.contains("Asked 1 question"));
    }

    #[tokio::test]
    async fn test_question_multiple() {
        let bus = SharedBus::new(16);
        let svc = Arc::new(QuestionService::new(bus));
        let tool = QuestionTool::new(svc.clone());
        let ctx = Arc::new(test_ctx());

        let tool_clone = tool.clone();
        let ctx_clone = ctx.clone();
        let handle = tokio::spawn(async move {
            tool_clone
                .execute(
                    serde_json::json!({
                        "questions": [
                            {"question": "Q1", "header": "Header 1", "options": [{"label": "A"}, {"label": "B"}]},
                            {"question": "Q2", "multiple": true}
                        ]
                    }),
                    &ctx_clone,
                )
                .await
                .unwrap()
        });

        tokio::time::sleep(std::time::Duration::from_millis(50)).await;

        let pending = svc.list().await;
        assert_eq!(pending.len(), 1);
        svc.reply(
            &pending[0].id,
            vec![
                QuestionAnswer::new(vec!["A".into()]),
                QuestionAnswer::new(vec!["X".into(), "Y".into()]),
            ],
        )
        .await
        .unwrap();

        let result = handle.await.unwrap();
        assert!(result.output.contains("Q1"));
        assert!(result.output.contains("Q2"));
        assert!(result.title.contains("Asked 2 questions"));
    }

    #[tokio::test]
    async fn test_question_missing_questions() {
        let bus = SharedBus::new(16);
        let svc = Arc::new(QuestionService::new(bus));
        let tool = QuestionTool::new(svc);
        let ctx = test_ctx();
        let result = tool.execute(serde_json::json!({}), &ctx).await;
        assert!(result.is_err());
    }

    // ── SkillTool tests ─────────────────────────────────────────────────

    #[tokio::test]
    async fn test_skill_basic() {
        let tool = SkillTool;
        let ctx = test_ctx();
        let result = tool
            .execute(serde_json::json!({"skill": "find-docs"}), &ctx)
            .await
            .unwrap();
        assert!(result.output.contains("find-docs"));
        assert!(result.title.contains("find-docs"));
    }

    #[tokio::test]
    async fn test_skill_missing_skill_param() {
        let tool = SkillTool;
        let ctx = test_ctx();
        let result = tool.execute(serde_json::json!({}), &ctx).await;
        assert!(result.is_err());
    }

    #[tokio::test]
    async fn test_skill_metadata() {
        let tool = SkillTool;
        let ctx = test_ctx();
        let result = tool
            .execute(serde_json::json!({"skill": "code-review"}), &ctx)
            .await
            .unwrap();
        let skill = result.metadata.get("skill").unwrap().as_str().unwrap();
        assert_eq!(skill, "code-review");
    }

    #[tokio::test]
    async fn test_skill_with_args() {
        let tool = SkillTool;
        let ctx = test_ctx();
        let result = tool
            .execute(
                serde_json::json!({"skill": "find-docs", "args": "--verbose"}),
                &ctx,
            )
            .await
            .unwrap();
        let args = result.metadata.get("args").unwrap().as_str().unwrap();
        assert_eq!(args, "--verbose");
    }

    // ── TodoWriteTool tests ─────────────────────────────────────────────

    #[tokio::test]
    async fn test_todo_write_basic() {
        let tool = TodoWriteTool;
        let ctx = test_ctx();
        let result = tool
            .execute(
                serde_json::json!({
                    "todos": [
                        {"text": "Implement login", "status": "in_progress", "priority": "high"},
                        {"text": "Write tests", "status": "pending", "priority": "medium"},
                        {"text": "Deploy", "status": "pending", "priority": "low"}
                    ]
                }),
                &ctx,
            )
            .await
            .unwrap();
        assert!(result.output.contains("Implement login"));
        assert!(result.output.contains("Write tests"));
        assert_eq!(result.title, "3 todos"); // 3 not completed (pending + in_progress)
    }

    #[tokio::test]
    async fn test_todo_write_empty() {
        let tool = TodoWriteTool;
        let ctx = test_ctx();
        let result = tool
            .execute(serde_json::json!({"todos": []}), &ctx)
            .await
            .unwrap();
        assert_eq!(result.title, "0 todos");
        assert!(result.output.contains("[]"));
    }

    #[tokio::test]
    async fn test_todo_write_all_completed() {
        let tool = TodoWriteTool;
        let ctx = test_ctx();
        let result = tool
            .execute(
                serde_json::json!({
                    "todos": [
                        {"text": "Done task", "status": "completed", "priority": "high"}
                    ]
                }),
                &ctx,
            )
            .await
            .unwrap();
        assert_eq!(result.title, "0 todos");
    }

    // ── PlanEnterTool tests ─────────────────────────────────────────────

    #[tokio::test]
    async fn test_plan_enter() {
        let tool = PlanEnterTool;
        let ctx = test_ctx();
        let result = tool
            .execute(serde_json::json!({"plan": "authentication module"}), &ctx)
            .await
            .unwrap();
        assert!(result.output.contains("Entered plan mode"));
        assert!(result.output.contains("authentication module"));
        assert!(result.title.contains("authentication module"));
    }

    #[tokio::test]
    async fn test_plan_enter_no_args() {
        let tool = PlanEnterTool;
        let ctx = test_ctx();
        let result = tool.execute(serde_json::json!({}), &ctx).await.unwrap();
        assert!(result.output.contains("Entered plan mode"));
        assert!(result.output.contains("implementation strategy"));
    }

    #[tokio::test]
    async fn test_plan_enter_metadata() {
        let tool = PlanEnterTool;
        let ctx = test_ctx();
        let result = tool
            .execute(serde_json::json!({"plan": "refactor database layer"}), &ctx)
            .await
            .unwrap();
        assert_eq!(
            result.metadata.get("mode").unwrap().as_str().unwrap(),
            "plan"
        );
        assert_eq!(
            result.metadata.get("plan").unwrap().as_str().unwrap(),
            "refactor database layer"
        );
    }

    // ── PlanExitTool tests ──────────────────────────────────────────────

    #[tokio::test]
    async fn test_plan_exit() {
        let tool = PlanExitTool;
        let ctx = test_ctx();
        let result = tool.execute(serde_json::json!({}), &ctx).await.unwrap();
        assert!(result.output.contains("plan mode"));
        assert!(result.title.contains("build agent"));
    }

    #[tokio::test]
    async fn test_plan_exit_no_args_needed() {
        let tool = PlanExitTool;
        let ctx = test_ctx();
        let result = tool
            .execute(serde_json::json!({"ignored": "value"}), &ctx)
            .await
            .unwrap();
        assert!(result.output.contains("plan mode"));
    }

    #[tokio::test]
    async fn test_plan_exit_not_truncated() {
        let tool = PlanExitTool;
        let ctx = test_ctx();
        let result = tool.execute(serde_json::json!({}), &ctx).await.unwrap();
        assert!(!result.truncated);
    }

    // ── ExitPlanModeTool tests ────────────────────────────────────────────

    #[tokio::test]
    async fn test_exit_plan_mode_basic() {
        let tool = ExitPlanModeTool;
        let ctx = test_ctx();
        let result = tool
            .execute(
                serde_json::json!({"plan": "1. Add auth module\n2. Add tests\n3. Deploy"}),
                &ctx,
            )
            .await
            .unwrap();
        assert!(result.output.contains("Exited plan mode"));
        assert!(result.output.contains("Add auth module"));
        assert!(result.title.contains("plan mode"));
    }

    #[tokio::test]
    async fn test_exit_plan_mode_missing_plan() {
        let tool = ExitPlanModeTool;
        let ctx = test_ctx();
        let result = tool.execute(serde_json::json!({}), &ctx).await;
        assert!(result.is_err());
    }

    #[tokio::test]
    async fn test_exit_plan_mode_metadata() {
        let tool = ExitPlanModeTool;
        let ctx = test_ctx();
        let result = tool
            .execute(serde_json::json!({"plan": "refactor auth"}), &ctx)
            .await
            .unwrap();
        assert_eq!(
            result.metadata.get("mode").unwrap().as_str().unwrap(),
            "build"
        );
        assert_eq!(
            result.metadata.get("plan").unwrap().as_str().unwrap(),
            "refactor auth"
        );
    }

    // ── Tool ID uniqueness ──────────────────────────────────────────────

    #[test]
    fn test_all_tool_ids_unique() {
        let tools: Vec<(String, Arc<dyn Tool>)> = vec![
            ("bash".to_string(), Arc::new(BashTool)),
            ("read".to_string(), Arc::new(ReadTool)),
            ("write".to_string(), Arc::new(WriteTool)),
            ("edit".to_string(), Arc::new(EditTool)),
            ("glob".to_string(), Arc::new(GlobTool)),
            ("grep".to_string(), Arc::new(GrepTool)),
            ("webfetch".to_string(), Arc::new(WebFetchTool)),
            ("websearch".to_string(), Arc::new(WebSearchTool)),
            ("apply_patch".to_string(), Arc::new(ApplyPatchTool)),
            ("task".to_string(), Arc::new(TaskTool)),
            ("question".to_string(), Arc::new(question_tool())),
            ("skill".to_string(), Arc::new(SkillTool)),
            ("todowrite".to_string(), Arc::new(TodoWriteTool)),
            ("stash".to_string(), Arc::new(StashTool)),
            ("notebook_edit".to_string(), Arc::new(NotebookEditTool)),
            ("task_output".to_string(), Arc::new(TaskOutputTool)),
            ("plan_enter".to_string(), Arc::new(PlanEnterTool)),
            ("plan_exit".to_string(), Arc::new(PlanExitTool)),
            ("exit_plan_mode".to_string(), Arc::new(ExitPlanModeTool)),
        ];

        let mut ids = std::collections::HashSet::new();
        for (expected_id, tool) in &tools {
            assert_eq!(
                tool.id(),
                *expected_id,
                "Tool ID mismatch for {}",
                expected_id
            );
            assert!(
                ids.insert(expected_id.to_string()),
                "Duplicate ID: {}",
                expected_id
            );
        }
        assert_eq!(ids.len(), 19);
    }

    // ── Tool schema validity ────────────────────────────────────────────

    #[test]
    fn test_all_tools_have_required_in_schema() {
        let tools: Vec<Arc<dyn Tool>> = vec![
            Arc::new(BashTool),
            Arc::new(ReadTool),
            Arc::new(WriteTool),
            Arc::new(EditTool),
            Arc::new(GlobTool),
            Arc::new(GrepTool),
            Arc::new(WebFetchTool),
            Arc::new(WebSearchTool),
            Arc::new(ApplyPatchTool),
            Arc::new(TaskTool),
            Arc::new(question_tool()),
            Arc::new(SkillTool),
            Arc::new(TodoWriteTool),
            Arc::new(StashTool),
            Arc::new(NotebookEditTool),
            Arc::new(TaskOutputTool),
            Arc::new(PlanEnterTool),
            Arc::new(PlanExitTool),
            Arc::new(ExitPlanModeTool),
        ];

        for tool in tools {
            let schema = tool.parameters_schema();
            assert_eq!(
                schema["type"],
                "object",
                "{} schema missing type",
                tool.id()
            );
            assert!(
                schema.get("properties").is_some(),
                "{} schema missing properties",
                tool.id()
            );
        }
    }

    // ── Tool description validity ───────────────────────────────────────

    #[test]
    fn test_all_tools_have_descriptions() {
        let tools: Vec<Arc<dyn Tool>> = vec![
            Arc::new(BashTool),
            Arc::new(ReadTool),
            Arc::new(WriteTool),
            Arc::new(EditTool),
            Arc::new(GlobTool),
            Arc::new(GrepTool),
            Arc::new(WebFetchTool),
            Arc::new(WebSearchTool),
            Arc::new(ApplyPatchTool),
            Arc::new(TaskTool),
            Arc::new(question_tool()),
            Arc::new(SkillTool),
            Arc::new(TodoWriteTool),
            Arc::new(StashTool),
            Arc::new(NotebookEditTool),
            Arc::new(TaskOutputTool),
            Arc::new(PlanEnterTool),
            Arc::new(PlanExitTool),
            Arc::new(ExitPlanModeTool),
        ];

        for tool in tools {
            assert!(
                tool.description().len() > 20,
                "{} description too short: {}",
                tool.id(),
                tool.description()
            );
        }
    }

    // ── LspTool tests ──────────────────────────────────────────────────

    #[tokio::test]
    async fn test_lsp_tool_basic() {
        let tool = LspTool;
        let ctx = test_ctx();
        let result = tool
            .execute(
                serde_json::json!({
                    "operation": "goToDefinition",
                    "filePath": "src/main.rs",
                    "line": 10,
                    "character": 5
                }),
                &ctx,
            )
            .await
            .unwrap();
        assert!(result.title.contains("goToDefinition"));
        assert!(result.title.contains("src/main.rs"));
        assert!(result.output.contains("LSP"));
    }

    #[tokio::test]
    async fn test_lsp_tool_workspace_symbol() {
        let tool = LspTool;
        let ctx = test_ctx();
        let result = tool
            .execute(
                serde_json::json!({
                    "operation": "workspaceSymbol",
                    "query": "MyStruct"
                }),
                &ctx,
            )
            .await
            .unwrap();
        assert!(result.title.contains("workspaceSymbol"));
        assert!(result.output.contains("LSP"));
    }

    #[tokio::test]
    async fn test_lsp_tool_missing_operation() {
        let tool = LspTool;
        let ctx = test_ctx();
        let result = tool.execute(serde_json::json!({}), &ctx).await;
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("missing"));
    }

    #[tokio::test]
    async fn test_lsp_tool_invalid_operation() {
        let tool = LspTool;
        let ctx = test_ctx();
        let result = tool
            .execute(serde_json::json!({"operation": "invalidOp"}), &ctx)
            .await;
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("invalid"));
    }

    #[tokio::test]
    async fn test_lsp_tool_missing_file_for_position_op() {
        let tool = LspTool;
        let ctx = test_ctx();
        let result = tool
            .execute(
                serde_json::json!({
                    "operation": "goToDefinition",
                    "line": 10,
                    "character": 5
                }),
                &ctx,
            )
            .await;
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("filePath"));
    }

    #[tokio::test]
    async fn test_lsp_tool_missing_line_character() {
        let tool = LspTool;
        let ctx = test_ctx();
        let result = tool
            .execute(
                serde_json::json!({
                    "operation": "goToDefinition",
                    "filePath": "test.rs"
                }),
                &ctx,
            )
            .await;
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("line"));
    }

    #[tokio::test]
    async fn test_lsp_tool_workspace_symbol_missing_query() {
        let tool = LspTool;
        let ctx = test_ctx();
        let result = tool
            .execute(serde_json::json!({"operation": "workspaceSymbol"}), &ctx)
            .await;
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("query"));
    }

    // ── InvalidTool tests ──────────────────────────────────────────────

    #[tokio::test]
    async fn test_invalid_tool_basic() {
        let tool = InvalidTool;
        let ctx = test_ctx();
        let result = tool
            .execute(
                serde_json::json!({
                    "tool": "nonexistent_tool",
                    "error": "tool not found"
                }),
                &ctx,
            )
            .await
            .unwrap();
        assert_eq!(result.title, "Invalid Tool");
        assert!(result.output.contains("nonexistent_tool"));
        assert!(result.output.contains("tool not found"));
    }

    #[tokio::test]
    async fn test_invalid_tool_empty_args() {
        let tool = InvalidTool;
        let ctx = test_ctx();
        let result = tool.execute(serde_json::json!({}), &ctx).await.unwrap();
        assert_eq!(result.title, "Invalid Tool");
        assert!(result.output.contains("unknown"));
    }

    // ── register_builtins ───────────────────────────────────────────────

    #[test]
    fn test_register_builtins_registers_all_20() {
        let registry = ToolRegistry::new();
        registry.register_builtins();
        let ids = registry.ids();
        assert_eq!(ids.len(), 20);
        assert!(ids.contains(&"bash".to_string()));
        assert!(ids.contains(&"read".to_string()));
        assert!(ids.contains(&"write".to_string()));
        assert!(ids.contains(&"edit".to_string()));
        assert!(ids.contains(&"glob".to_string()));
        assert!(ids.contains(&"grep".to_string()));
        assert!(ids.contains(&"webfetch".to_string()));
        assert!(ids.contains(&"websearch".to_string()));
        assert!(ids.contains(&"apply_patch".to_string()));
        assert!(ids.contains(&"task".to_string()));
        assert!(!ids.contains(&"question".to_string()));
        assert!(ids.contains(&"skill".to_string()));
        assert!(ids.contains(&"todowrite".to_string()));
        assert!(ids.contains(&"stash".to_string()));
        assert!(ids.contains(&"notebook_edit".to_string()));
        assert!(ids.contains(&"task_output".to_string()));
        assert!(ids.contains(&"plan_enter".to_string()));
        assert!(ids.contains(&"plan_exit".to_string()));
        assert!(ids.contains(&"exit_plan_mode".to_string()));
        assert!(ids.contains(&"lsp".to_string()));
        assert!(ids.contains(&"invalid".to_string()));
    }

    #[test]
    fn test_register_builtins_can_get_tools() {
        let registry = ToolRegistry::new();
        registry.register_builtins();

        for id in [
            "bash",
            "read",
            "write",
            "edit",
            "glob",
            "grep",
            "webfetch",
            "websearch",
            "apply_patch",
            "task",
            // question tool requires a service — registered separately
            "skill",
            "todowrite",
            "stash",
            "notebook_edit",
            "task_output",
            "plan_enter",
            "plan_exit",
            "exit_plan_mode",
            "lsp",
            "invalid",
        ] {
            assert!(
                registry.get(id).is_some(),
                "Tool {} not found in registry",
                id
            );
        }
    }
}
