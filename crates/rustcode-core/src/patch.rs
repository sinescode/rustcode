//! Patch parsing and application types.
//!
//! Ported from: `packages/core/src/patch.ts` (197 lines)
//! OpenCode commit: 5d0f86606ac30690f79f0a6a9f41a1f49fe95d0b
//!
//! This module provides:
//! - [`Hunk`] — discriminated union for add/delete/update file operations
//! - [`UpdateFileChunk`] — a block of old→new line changes within an update
//! - [`FileUpdate`] — post-apply file content with BOM tracking
//! - [`parse`] — parse a patch text into a list of hunks
//! - [`derive`] — apply update chunks to original file content

use serde::{Deserialize, Serialize};
use thiserror::Error;

// ── Hunk ─────────────────────────────────────────────────────────────────────

/// A hunk representing one file-level change in a patch.
///
/// Ported from: `patch.ts` — `Hunk` discriminated union
///
/// Three variants map to the `type` tag:
/// - `"add"` — create a new file
/// - `"delete"` — remove an existing file
/// - `"update"` — modify an existing file via chunks
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "type")]
pub enum Hunk {
    /// Add a new file with the given content.
    ///
    /// Ported from: `patch.ts` — `{ type: "add", path, contents }`
    #[serde(rename = "add")]
    Add {
        /// Path of the new file to create.
        path: String,
        /// Full content of the new file.
        contents: String,
    },

    /// Delete an existing file.
    ///
    /// Ported from: `patch.ts` — `{ type: "delete", path }`
    #[serde(rename = "delete")]
    Delete {
        /// Path of the file to delete.
        path: String,
    },

    /// Update an existing file with one or more chunks.
    ///
    /// Ported from: `patch.ts` — `{ type: "update", path, movePath?, chunks }`
    #[serde(rename = "update")]
    Update {
        /// Path of the file to update.
        path: String,
        /// Optional new path if the file is being moved or renamed.
        #[serde(skip_serializing_if = "Option::is_none")]
        move_path: Option<String>,
        /// Ordered list of change chunks to apply.
        chunks: Vec<UpdateFileChunk>,
    },
}

// ── UpdateFileChunk ──────────────────────────────────────────────────────────

/// A chunk within an update — describes a specific change block with old→new
/// line replacements.
///
/// Ported from: `patch.ts` — `UpdateFileChunk` interface
///
/// Each chunk represents one `@@` block from the patch format. The `old_lines`
/// are the lines to find and remove; `new_lines` are the lines to insert in
/// their place. Context lines (unchanged) appear in both lists.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct UpdateFileChunk {
    /// Lines to find and remove from the original file.
    pub old_lines: Vec<String>,

    /// Lines to insert in place of `old_lines`.
    pub new_lines: Vec<String>,

    /// Optional context hint (from the `@@` header) used to locate where this
    /// chunk applies in the file.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub change_context: Option<String>,

    /// Whether this chunk applies at the end of the file. When `true`,
    /// `old_lines` are sought starting from the end rather than from the
    /// current cursor position.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub end_of_file: Option<bool>,
}

// ── FileUpdate ───────────────────────────────────────────────────────────────

/// A file that has been updated — the post-apply content and BOM flag.
///
/// Ported from: `patch.ts` — `FileUpdate` interface
///
/// The `bom` field tracks whether a UTF-8 BOM (`\u{FEFF}`) was present in the
/// original file (or produced by the update), so callers can re-insert it via
/// [`join_bom`] when writing the file back to disk.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct FileUpdate {
    /// New file content after applying the patch.
    pub content: String,

    /// Whether the original file (or the result) had a UTF-8 BOM.
    pub bom: bool,
}

// ── PatchError ───────────────────────────────────────────────────────────────

/// Error type for patch parsing and application.
///
/// Ported from: `patch.ts` — thrown `Error` instances (consolidated)
///
/// In the TS source, errors are plain `throw new Error("...")`. This enum
/// provides structured variants so callers can distinguish parse-time failures
/// from apply-time failures programmatically.
#[derive(Debug, Error)]
pub enum PatchError {
    /// An error encountered while parsing patch text.
    ///
    /// Ported from: `patch.ts` — `throw new Error(...)` in `parse()` and helpers
    #[error("parse error: {message}")]
    ParseError {
        /// Human-readable description of the parse failure.
        message: String,
    },

    /// An error encountered while applying a patch to file content.
    ///
    /// Ported from: `patch.ts` — `throw new Error(...)` in `derive()` and helpers
    #[error("apply error in `{path}`: {message}")]
    ApplyError {
        /// Path of the file the apply failed on.
        path: String,
        /// Human-readable description of the apply failure.
        message: String,
    },
}

// ── PatchResult ──────────────────────────────────────────────────────────────

/// Result type alias for patch operations.
///
/// Ported from: `patch.ts` — untyped `throw` / return convention
pub type PatchResult<T> = Result<T, PatchError>;

// ── Marker constants ─────────────────────────────────────────────────────────

/// Marker that opens a patch block.
///
/// Ported from: `patch.ts` — `"*** Begin Patch"`
pub const PATCH_BEGIN_MARKER: &str = "*** Begin Patch";

/// Marker that closes a patch block.
///
/// Ported from: `patch.ts` — `"*** End Patch"`
pub const PATCH_END_MARKER: &str = "*** End Patch";

/// Marker for an add-file hunk.
///
/// Ported from: `patch.ts` — `"*** Add File:"`
pub const ADD_FILE_MARKER: &str = "*** Add File:";

/// Marker for a delete-file hunk.
///
/// Ported from: `patch.ts` — `"*** Delete File:"`
pub const DELETE_FILE_MARKER: &str = "*** Delete File:";

/// Marker for an update-file hunk.
///
/// Ported from: `patch.ts` — `"*** Update File:"`
pub const UPDATE_FILE_MARKER: &str = "*** Update File:";

/// Marker for a file move/rename within an update hunk.
///
/// Ported from: `patch.ts` — `"*** Move to:"`
pub const MOVE_TO_MARKER: &str = "*** Move to:";

/// Marker for end-of-file within an update chunk.
///
/// Ported from: `patch.ts` — `"*** End of File"`
pub const END_OF_FILE_MARKER: &str = "*** End of File";

// ── strip_heredoc ───────────────────────────────────────────────────────────

/// Strip a shell heredoc wrapper from the input if present.
///
/// Ported from: `patch.ts` — `stripHeredoc()`
///
/// Handles inputs like:
/// ```text
/// cat << 'EOF'
/// *** Begin Patch
/// ...
/// *** End Patch
/// EOF
/// ```
///
/// If no heredoc wrapper is detected, returns the input unchanged.
fn strip_heredoc(input: &str) -> &str {
    let trimmed = input.trim_start();
    // Optional "cat " prefix
    let after_cat = trimmed.strip_prefix("cat ").unwrap_or(trimmed);

    // Must start with "<<"
    let rest = match after_cat.strip_prefix("<<") {
        Some(r) => r,
        None => return input,
    };

    // Optional quote character around the delimiter
    let (delim_start, quote_char) = if let Some(r) = rest.strip_prefix('\'') {
        (r, Some('\''))
    } else if let Some(r) = rest.strip_prefix('"') {
        (r, Some('"'))
    } else {
        (rest, None)
    };

    // Find end of delimiter word
    let delim_end = delim_start
        .find(|c: char| !c.is_alphanumeric() && c != '_')
        .unwrap_or(delim_start.len());
    let delimiter = &delim_start[..delim_end];
    if delimiter.is_empty() {
        return input;
    }

    let after_delim = &delim_start[delim_end..];

    // Consume matching close quote if present
    let after_delim = match quote_char {
        Some(qc) => match after_delim.strip_prefix(qc) {
            Some(r) => r,
            None => return input,
        },
        None => after_delim,
    };

    // Skip whitespace then newline
    let after_delim = after_delim.trim_start_matches(|c: char| c.is_whitespace() && c != '\n');
    let content_start = match after_delim.strip_prefix('\n') {
        Some(r) => r,
        None => return input,
    };

    // Find closing delimiter: \n<delimiter> at end, followed only by whitespace
    let closing_pattern = format!("\n{}", delimiter);
    if let Some(content_end) = content_start.rfind(&closing_pattern) {
        let after = &content_start[content_end + closing_pattern.len()..];
        if after.trim().is_empty() {
            return &content_start[..content_end];
        }
    }

    input
}

// ── split_bom ───────────────────────────────────────────────────────────────

/// Split a string into its BOM flag and BOM-stripped text.
///
/// Ported from: `patch.ts` — `splitBom()`
///
/// Returns `(has_bom, text_without_bom)`.
fn split_bom(text: &str) -> (bool, &str) {
    if let Some(stripped) = text.strip_prefix('\u{FEFF}') {
        (true, stripped)
    } else {
        (false, text)
    }
}

// ── join_bom ────────────────────────────────────────────────────────────────

/// Conditionally prepend a UTF-8 BOM to the given text.
///
/// Ported from: `patch.ts` — `joinBom()`
///
/// If `bom` is `true`, the returned string starts with U+FEFF regardless of
/// whether the input already had one. If `false`, any existing BOM is stripped.
pub fn join_bom(text: &str, bom: bool) -> String {
    let (_, stripped) = split_bom(text);
    if bom {
        let mut result = String::with_capacity(stripped.len() + 3);
        result.push('\u{FEFF}');
        result.push_str(stripped);
        result
    } else {
        stripped.to_string()
    }
}

// ── parse ────────────────────────────────────────────────────────────────────

/// Parse a patch text into a list of [`Hunk`]s.
///
/// Ported from: `patch.ts` — `parse()`
///
/// The expected format is:
/// ```text
/// *** Begin Patch
/// *** Add File: path/to/file.rs
/// +line 1 of new content
/// +line 2 of new content
/// *** Delete File: path/to/remove.rs
/// *** Update File: path/to/modify.rs
/// @@ some context hint
///  unchanged line
/// -removed line
/// +added line
/// *** End of File
/// *** End Patch
/// ```
///
/// # Errors
///
/// Returns [`PatchError::ParseError`] if markers are missing, paths are empty,
/// or lines have unexpected prefixes.
pub fn parse(patch_text: &str) -> PatchResult<Vec<Hunk>> {
    let stripped = strip_heredoc(patch_text.trim());
    let lines: Vec<&str> = stripped.lines().collect();

    let begin = lines
        .iter()
        .position(|l| l.trim() == PATCH_BEGIN_MARKER)
        .ok_or_else(|| PatchError::ParseError {
            message: "invalid patch format: missing Begin/End markers".into(),
        })?;
    let end = lines
        .iter()
        .position(|l| l.trim() == PATCH_END_MARKER)
        .ok_or_else(|| PatchError::ParseError {
            message: "invalid patch format: missing Begin/End markers".into(),
        })?;

    if begin >= end {
        return Err(PatchError::ParseError {
            message: "invalid patch format: missing Begin/End markers".into(),
        });
    }

    let mut hunks: Vec<Hunk> = Vec::new();
    let mut index = begin + 1;

    while index < end {
        let line = lines[index];

        if let Some(path) = line.strip_prefix(ADD_FILE_MARKER) {
            let path = path.trim().to_string();
            if path.is_empty() {
                return Err(PatchError::ParseError {
                    message: "invalid add file path".into(),
                });
            }
            let (contents, next) = parse_add(&lines, index + 1)?;
            hunks.push(Hunk::Add { path, contents });
            index = next;
            continue;
        }

        if let Some(path) = line.strip_prefix(DELETE_FILE_MARKER) {
            let path = path.trim().to_string();
            if path.is_empty() {
                return Err(PatchError::ParseError {
                    message: "invalid delete file path".into(),
                });
            }
            hunks.push(Hunk::Delete { path });
            index += 1;
            continue;
        }

        if let Some(path) = line.strip_prefix(UPDATE_FILE_MARKER) {
            let path = path.trim().to_string();
            if path.is_empty() {
                return Err(PatchError::ParseError {
                    message: "invalid update file path".into(),
                });
            }

            let mut next = index + 1;
            let mut move_path: Option<String> = None;

            // Check for optional move/rename directive
            if next < end {
                if let Some(mv) = lines[next].strip_prefix(MOVE_TO_MARKER) {
                    let mv = mv.trim().to_string();
                    if mv.is_empty() {
                        return Err(PatchError::ParseError {
                            message: "invalid move file path".into(),
                        });
                    }
                    move_path = Some(mv);
                    next += 1;
                }
            }

            let (chunks, next) = parse_update(&lines, next)?;
            if chunks.is_empty() {
                return Err(PatchError::ParseError {
                    message: format!(
                        "invalid update hunk for {}: expected at least one @@ chunk",
                        path
                    ),
                });
            }
            hunks.push(Hunk::Update {
                path,
                move_path,
                chunks,
            });
            index = next;
            continue;
        }

        // Unknown line — TS source throws, we do the same
        return Err(PatchError::ParseError {
            message: format!("invalid patch line: {}", line),
        });
    }

    Ok(hunks)
}

// ── parse_add ───────────────────────────────────────────────────────────────

/// Parse an add-file block from patch lines.
///
/// Ported from: `patch.ts` — `parseAdd()`
///
/// Each content line must start with `+`. The `+` is stripped and the remainder
/// is collected as file content. Parsing stops at the next `***` marker line.
fn parse_add(lines: &[&str], start: usize) -> PatchResult<(String, usize)> {
    let mut content: Vec<&str> = Vec::new();
    let mut index = start;

    while index < lines.len() && !lines[index].starts_with("***") {
        let line = lines[index];
        if !line.starts_with('+') {
            return Err(PatchError::ParseError {
                message: format!("invalid add file line: {}", line),
            });
        }
        // Slice after the '+' — keeping whatever follows (including leading space)
        content.push(&line[1..]);
        index += 1;
    }

    Ok((content.join("\n"), index))
}

// ── parse_update ────────────────────────────────────────────────────────────

/// Parse update-file chunks from patch lines.
///
/// Ported from: `patch.ts` — `parseUpdate()`
///
/// Each chunk starts with a `@@` context header line. Within a chunk:
/// - Lines starting with ` ` (space) are context — added to both old and new.
/// - Lines starting with `-` are removals — added to old only.
/// - Lines starting with `+` are additions — added to new only.
/// - `*** End of File` sets `end_of_file = true` and closes the chunk.
/// - Any other `***` marker closes the update block entirely.
fn parse_update(lines: &[&str], start: usize) -> PatchResult<(Vec<UpdateFileChunk>, usize)> {
    let mut chunks: Vec<UpdateFileChunk> = Vec::new();
    let mut index = start;

    while index < lines.len() && !lines[index].starts_with("***") {
        let header_line = lines[index];
        if !header_line.starts_with("@@") {
            return Err(PatchError::ParseError {
                message: format!("invalid update file line: {}", header_line),
            });
        }

        let change_context = {
            let ctx = header_line[2..].trim();
            if ctx.is_empty() {
                None
            } else {
                Some(ctx.to_string())
            }
        };

        let mut old_lines: Vec<String> = Vec::new();
        let mut new_lines: Vec<String> = Vec::new();
        let mut end_of_file = false;
        index += 1;

        while index < lines.len() && !lines[index].starts_with("@@") {
            let line = lines[index];

            if line == END_OF_FILE_MARKER {
                end_of_file = true;
                index += 1;
                break;
            }

            if line.starts_with("***") {
                break;
            }

            if let Some(rest) = line.strip_prefix(' ') {
                // Context line — appears in both old and new
                old_lines.push(rest.to_string());
                new_lines.push(rest.to_string());
            } else if let Some(rest) = line.strip_prefix('-') {
                // Removal — only in old
                old_lines.push(rest.to_string());
            } else if let Some(rest) = line.strip_prefix('+') {
                // Addition — only in new
                new_lines.push(rest.to_string());
            } else {
                return Err(PatchError::ParseError {
                    message: format!("invalid update chunk line: {}", line),
                });
            }
            index += 1;
        }

        let eof_flag = if end_of_file { Some(true) } else { None };

        chunks.push(UpdateFileChunk {
            old_lines,
            new_lines,
            change_context,
            end_of_file: eof_flag,
        });
    }

    Ok((chunks, index))
}

// ── derive ──────────────────────────────────────────────────────────────────

/// Apply update chunks to an original file's content, producing the updated
/// file content and BOM status.
///
/// Ported from: `patch.ts` — `derive()`
///
/// This is the core patch-application function. It takes the original file
/// content and a list of [`UpdateFileChunk`]s (from a [`Hunk::Update`]) and
/// produces a [`FileUpdate`] with the transformed content.
///
/// The algorithm:
/// 1. Split the original into lines (stripping a final empty line).
/// 2. For each chunk, locate where `old_lines` appear in the file using
///    increasingly lenient matching (exact → right-strip → trim → Unicode-normalize).
/// 3. Replace the matched old lines with the new lines.
/// 4. Re-join and track BOM status.
///
/// # Errors
///
/// Returns [`PatchError::ApplyError`] if any chunk's `old_lines` cannot be
/// located in the file, or if a `change_context` hint cannot be found.
pub fn derive(path: &str, chunks: &[UpdateFileChunk], original: &str) -> PatchResult<FileUpdate> {
    let (source_bom, source_text) = split_bom(original);

    // Split into lines, removing a trailing empty line (matching TS behavior)
    let mut file_lines: Vec<String> = source_text.lines().map(String::from).collect();
    if file_lines.last().is_some_and(|l| l.is_empty()) {
        file_lines.pop();
    }

    let replacements = compute_replacements(&file_lines, path, chunks)?;

    // Apply replacements in reverse order so indices remain stable
    let mut updated = file_lines;
    for (start, remove_count, insert_lines) in replacements.into_iter().rev() {
        let end = (start + remove_count).min(updated.len());
        let insert = insert_lines.into_iter();
        updated.splice(start..end, insert);
    }

    // Ensure trailing newline (as TS does: if last line is not empty, push "")
    if updated.last().is_none_or(|l| !l.is_empty()) {
        updated.push(String::new());
    }

    let rejoined = updated.join("\n");
    let (next_bom, next_text) = split_bom(&rejoined);

    Ok(FileUpdate {
        content: next_text.to_string(),
        bom: source_bom || next_bom,
    })
}

// ── compute_replacements ─────────────────────────────────────────────────────

/// Compute the list of `(start_index, remove_count, insert_lines)` replacements
/// by locating each chunk's old lines in the file.
///
/// Ported from: `patch.ts` — `computeReplacements()`
fn compute_replacements(
    lines: &[String],
    path: &str,
    chunks: &[UpdateFileChunk],
) -> PatchResult<Vec<(usize, usize, Vec<String>)>> {
    let mut replacements: Vec<(usize, usize, Vec<String>)> = Vec::new();
    let mut line_index: usize = 0;

    for chunk in chunks {
        // If a change_context hint is provided, locate it first
        if let Some(ref ctx) = chunk.change_context {
            let context_pos = seek(lines, std::slice::from_ref(ctx), line_index, false);
            match context_pos {
                Some(pos) => line_index = pos + 1,
                None => {
                    return Err(PatchError::ApplyError {
                        path: path.into(),
                        message: format!("failed to find context '{}' in {}", ctx, path),
                    });
                }
            }
        }

        // Empty old_lines means a pure insertion at end of file
        if chunk.old_lines.is_empty() {
            replacements.push((lines.len(), 0, chunk.new_lines.clone()));
            continue;
        }

        let mut old_lines = chunk.old_lines.clone();
        let mut new_lines = chunk.new_lines.clone();
        let eof = chunk.end_of_file.unwrap_or(false);

        let mut found = seek(lines, &old_lines, line_index, eof);

        // Retry without trailing empty line (matching TS fallback)
        if found.is_none() && old_lines.last().is_some_and(|l| l.is_empty()) {
            old_lines.pop();
            if new_lines.last().is_some_and(|l| l.is_empty()) {
                new_lines.pop();
            }
            found = seek(lines, &old_lines, line_index, eof);
        }

        match found {
            Some(pos) => {
                replacements.push((pos, old_lines.len(), new_lines));
                line_index = pos + old_lines.len();
            }
            None => {
                return Err(PatchError::ApplyError {
                    path: path.into(),
                    message: format!(
                        "failed to find expected lines in {}:\n{}",
                        path,
                        old_lines.join("\n")
                    ),
                });
            }
        }
    }

    // Sort by start position (ascending) — TS uses toSorted
    replacements.sort_by_key(|(start, _, _)| *start);

    Ok(replacements)
}

// ── seek ─────────────────────────────────────────────────────────────────────

/// Find the position of a pattern within lines, starting from a given index.
///
/// Ported from: `patch.ts` — `seek()`
///
/// Tries four comparison strategies in order of strictness:
/// 1. **exact** — byte-for-byte equality
/// 2. **rstrip** — equality after trimming trailing whitespace
/// 3. **trim** — equality after trimming both ends
/// 4. **normalized** — equality after Unicode character normalization
///
/// When `eof` is true, only checks the end-of-file position (the last possible
/// offset where the pattern could fit) rather than scanning from `start`.
///
/// Returns `None` if no match is found (equivalent to TS returning `-1`).
fn seek(lines: &[String], pattern: &[String], start: usize, eof: bool) -> Option<usize> {
    if pattern.is_empty() {
        return None;
    }

    let compare_fns: [fn(&str, &str) -> bool; 4] = [exact, rstrip, trim, normalized];

    for compare in &compare_fns {
        if eof {
            let offset = lines.len().checked_sub(pattern.len())?;
            if offset >= start && matches_pattern(lines, pattern, offset, *compare) {
                return Some(offset);
            }
        } else {
            let max_offset = lines.len().saturating_sub(pattern.len());
            for offset in start..=max_offset {
                if matches_pattern(lines, pattern, offset, *compare) {
                    return Some(offset);
                }
            }
        }
    }

    None
}

// ── matches_pattern ─────────────────────────────────────────────────────────

/// Check whether `pattern` matches `lines` starting at `offset` using the given
/// comparison function.
///
/// Ported from: `patch.ts` — `matches()`
fn matches_pattern(
    lines: &[String],
    pattern: &[String],
    offset: usize,
    compare: fn(&str, &str) -> bool,
) -> bool {
    pattern
        .iter()
        .enumerate()
        .all(|(i, pat_line)| compare(&lines[offset + i], pat_line))
}

// ── Comparison functions ─────────────────────────────────────────────────────

/// Exact byte-for-byte equality.
///
/// Ported from: `patch.ts` — `exact`
fn exact(left: &str, right: &str) -> bool {
    left == right
}

/// Equality after stripping trailing whitespace from both sides.
///
/// Ported from: `patch.ts` — `rstrip`
fn rstrip(left: &str, right: &str) -> bool {
    left.trim_end() == right.trim_end()
}

/// Equality after stripping leading and trailing whitespace from both sides.
///
/// Ported from: `patch.ts` — `trim`
fn trim(left: &str, right: &str) -> bool {
    left.trim() == right.trim()
}

/// Equality after trimming both sides and normalizing Unicode characters.
///
/// Ported from: `patch.ts` — `normalized`
fn normalized(left: &str, right: &str) -> bool {
    normalize(left.trim()) == normalize(right.trim())
}

// ── normalize ───────────────────────────────────────────────────────────────

/// Normalize Unicode punctuation characters to their ASCII equivalents.
///
/// Ported from: `patch.ts` — `normalize()`
///
/// Replaces:
/// - Curly single quotes (`'`, `'`, `‚`, `‛`) → `'`
/// - Curly double quotes (`"`, `"`, `„`, `‟`) → `"`
/// - Various dash characters (`‐`, `‑`, `‒`, `–`, `—`, `―`) → `-`
/// - Ellipsis (`…`) → `...`
/// - Non-breaking space (`\u{A0}`) → ` `
fn normalize(value: &str) -> String {
    let mut result = String::with_capacity(value.len());
    for ch in value.chars() {
        match ch {
            // Curly single quotes → ASCII single quote
            '\u{2018}' | '\u{2019}' | '\u{201A}' | '\u{201B}' => result.push('\''),
            // Curly double quotes → ASCII double quote
            '\u{201C}' | '\u{201D}' | '\u{201E}' | '\u{201F}' => result.push('"'),
            // Various dash characters → ASCII hyphen-minus
            '\u{2010}' | '\u{2011}' | '\u{2012}' | '\u{2013}' | '\u{2014}' | '\u{2015}' => {
                result.push('-')
            }
            // Ellipsis → three dots
            '\u{2026}' => result.push_str("..."),
            // Non-breaking space → regular space
            '\u{00A0}' => result.push(' '),
            c => result.push(c),
        }
    }
    result
}

// ── Tests ───────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    // ── parse tests ─────────────────────────────────────────────────────

    #[test]
    fn test_parse_empty_patch_fails() {
        let result = parse("");
        assert!(result.is_err());
        match result {
            Err(PatchError::ParseError { message }) => {
                assert!(message.contains("missing Begin/End markers"));
            }
            _ => panic!("expected ParseError"),
        }
    }

    #[test]
    fn test_parse_patch_no_markers() {
        let result = parse("just some random text\nwith multiple lines\n");
        assert!(result.is_err());
        match result {
            Err(PatchError::ParseError { message }) => {
                assert!(message.contains("missing Begin/End markers"));
            }
            _ => panic!("expected ParseError"),
        }
    }

    #[test]
    fn test_parse_only_begin_marker() {
        let result = parse("*** Begin Patch\nsome content\n");
        assert!(result.is_err());
    }

    #[test]
    fn test_parse_add_file() {
        let patch = concat!(
            "*** Begin Patch\n",
            "*** Add File: new_file.rs\n",
            "+fn main() {\n",
            "+    println!(\"hello\");\n",
            "+}\n",
            "*** End Patch\n",
        );
        let hunks = parse(patch).expect("should parse");
        assert_eq!(hunks.len(), 1);
        match &hunks[0] {
            Hunk::Add { path, contents } => {
                assert_eq!(path, "new_file.rs");
                assert!(contents.contains("fn main()"));
                assert!(contents.contains("println!"));
            }
            _ => panic!("expected Add hunk, got {:?}", hunks[0]),
        }
    }

    #[test]
    fn test_parse_add_file_with_leading_plus() {
        let patch = concat!(
            "*** Begin Patch\n",
            "*** Add File: hello.txt\n",
            "+hello world\n",
            "+goodbye world\n",
            "*** End Patch\n",
        );
        let hunks = parse(patch).expect("should parse");
        match &hunks[0] {
            Hunk::Add { contents, .. } => {
                assert_eq!(contents, "hello world\ngoodbye world");
            }
            _ => panic!("expected Add"),
        }
    }

    #[test]
    fn test_parse_add_file_empty_path() {
        let patch = concat!(
            "*** Begin Patch\n",
            "*** Add File: \n",
            "+content\n",
            "*** End Patch\n",
        );
        let result = parse(patch);
        assert!(result.is_err());
    }

    #[test]
    fn test_parse_delete_file() {
        let patch = concat!(
            "*** Begin Patch\n",
            "*** Delete File: old_file.rs\n",
            "*** End Patch\n",
        );
        let hunks = parse(patch).expect("should parse");
        assert_eq!(hunks.len(), 1);
        match &hunks[0] {
            Hunk::Delete { path } => assert_eq!(path, "old_file.rs"),
            _ => panic!("expected Delete hunk"),
        }
    }

    #[test]
    fn test_parse_delete_file_empty_path() {
        let patch = concat!(
            "*** Begin Patch\n",
            "*** Delete File: \n",
            "*** End Patch\n",
        );
        let result = parse(patch);
        assert!(result.is_err());
    }

    #[test]
    fn test_parse_update_file() {
        let patch = concat!(
            "*** Begin Patch\n",
            "*** Update File: src/lib.rs\n",
            "@@ -1 +1 @@\n",
            " unchanged context\n",
            "-old removed line\n",
            "+new added line\n",
            "*** End Patch\n",
        );
        let hunks = parse(patch).expect("should parse");
        assert_eq!(hunks.len(), 1);
        match &hunks[0] {
            Hunk::Update {
                path,
                move_path,
                chunks,
            } => {
                assert_eq!(path, "src/lib.rs");
                assert!(move_path.is_none());
                assert_eq!(chunks.len(), 1);
                assert_eq!(
                    chunks[0].old_lines,
                    vec!["unchanged context", "old removed line"]
                );
                assert_eq!(
                    chunks[0].new_lines,
                    vec!["unchanged context", "new added line"]
                );
                assert_eq!(chunks[0].change_context, Some("-1 +1 @@".to_string()));
            }
            _ => panic!("expected Update hunk"),
        }
    }

    #[test]
    fn test_parse_update_file_with_move() {
        let patch = concat!(
            "*** Begin Patch\n",
            "*** Update File: old_path.rs\n",
            "*** Move to: new_path.rs\n",
            "@@ context\n",
            " unchanged\n",
            "*** End Patch\n",
        );
        let hunks = parse(patch).expect("should parse");
        match &hunks[0] {
            Hunk::Update { move_path, .. } => {
                assert_eq!(move_path.as_deref(), Some("new_path.rs"));
            }
            _ => panic!("expected Update hunk"),
        }
    }

    #[test]
    fn test_parse_update_file_with_end_of_file() {
        let patch = concat!(
            "*** Begin Patch\n",
            "*** Update File: test.rs\n",
            "@@ end\n",
            "-old last line\n",
            "+new last line\n",
            "*** End of File\n",
            "*** End Patch\n",
        );
        let hunks = parse(patch).expect("should parse");
        match &hunks[0] {
            Hunk::Update { chunks, .. } => {
                assert_eq!(chunks.len(), 1);
                assert_eq!(chunks[0].end_of_file, Some(true));
            }
            _ => panic!("expected Update hunk"),
        }
    }

    #[test]
    fn test_parse_update_empty_chunks() {
        let patch = concat!(
            "*** Begin Patch\n",
            "*** Update File: test.rs\n",
            "*** End Patch\n",
        );
        let result = parse(patch);
        assert!(result.is_err());
        match result {
            Err(PatchError::ParseError { message }) => {
                assert!(message.contains("expected at least one @@ chunk"));
            }
            _ => panic!("expected ParseError"),
        }
    }

    #[test]
    fn test_parse_multiple_hunks() {
        let patch = concat!(
            "*** Begin Patch\n",
            "*** Add File: new.txt\n",
            "+hello\n",
            "*** Delete File: old.txt\n",
            "*** Update File: mod.txt\n",
            "@@ fix\n",
            " unchanged\n",
            "-bad\n",
            "+good\n",
            "*** End Patch\n",
        );
        let hunks = parse(patch).expect("should parse");
        assert_eq!(hunks.len(), 3);
        assert!(matches!(hunks[0], Hunk::Add { .. }));
        assert!(matches!(hunks[1], Hunk::Delete { .. }));
        assert!(matches!(hunks[2], Hunk::Update { .. }));
    }

    #[test]
    fn test_parse_with_heredoc() {
        let patch = concat!(
            "cat << 'EOF'\n",
            "*** Begin Patch\n",
            "*** Add File: test.rs\n",
            "+fn main() {}\n",
            "*** End Patch\n",
            "EOF\n",
        );
        let hunks = parse(patch).expect("should parse");
        assert_eq!(hunks.len(), 1);
        match &hunks[0] {
            Hunk::Add { path, .. } => assert_eq!(path, "test.rs"),
            _ => panic!("expected Add"),
        }
    }

    // ── derive tests ────────────────────────────────────────────────────

    #[test]
    fn test_derive_simple_replacement() {
        let original = "line 1\nline 2\nline 3\n";
        let chunks = vec![UpdateFileChunk {
            old_lines: vec!["line 2".into()],
            new_lines: vec!["line 2 modified".into()],
            change_context: None,
            end_of_file: None,
        }];
        let result = derive("test.txt", &chunks, original).expect("should apply");
        assert_eq!(result.content, "line 1\nline 2 modified\nline 3\n");
        assert!(!result.bom);
    }

    #[test]
    fn test_derive_addition() {
        let original = "line 1\nline 3\n";
        let chunks = vec![UpdateFileChunk {
            old_lines: vec!["line 3".into()],
            new_lines: vec!["line 2".into(), "line 3".into()],
            change_context: None,
            end_of_file: None,
        }];
        let result = derive("test.txt", &chunks, original).expect("should apply");
        assert_eq!(result.content, "line 1\nline 2\nline 3\n");
    }

    #[test]
    fn test_derive_deletion() {
        let original = "keep\nremove\nkeep\n";
        let chunks = vec![UpdateFileChunk {
            old_lines: vec!["remove".into()],
            new_lines: vec![],
            change_context: None,
            end_of_file: None,
        }];
        let result = derive("test.txt", &chunks, original).expect("should apply");
        assert_eq!(result.content, "keep\nkeep\n");
    }

    #[test]
    fn test_derive_context_hint() {
        let original = "header\nbody content\nfooter\n";
        let chunks = vec![UpdateFileChunk {
            old_lines: vec!["body content".into()],
            new_lines: vec!["modified body".into()],
            change_context: Some("header".into()),
            end_of_file: None,
        }];
        let result = derive("test.txt", &chunks, original).expect("should apply");
        assert_eq!(result.content, "header\nmodified body\nfooter\n");
    }

    #[test]
    fn test_derive_bom_preservation() {
        let original = "\u{FEFF}line 1\nline 2\n";
        let chunks = vec![UpdateFileChunk {
            old_lines: vec!["line 2".into()],
            new_lines: vec!["line 2 updated".into()],
            change_context: None,
            end_of_file: None,
        }];
        let result = derive("test.txt", &chunks, original).expect("should apply");
        assert!(result.bom);
        assert_eq!(result.content, "line 1\nline 2 updated\n");
    }

    #[test]
    fn test_derive_not_found() {
        let original = "line 1\nline 2\n";
        let chunks = vec![UpdateFileChunk {
            old_lines: vec!["nonexistent".into()],
            new_lines: vec!["replacement".into()],
            change_context: None,
            end_of_file: None,
        }];
        let result = derive("test.txt", &chunks, original);
        assert!(result.is_err());
        match result {
            Err(PatchError::ApplyError { path, message }) => {
                assert_eq!(path, "test.txt");
                assert!(message.contains("failed to find"));
            }
            _ => panic!("expected ApplyError"),
        }
    }

    // ── join_bom tests ──────────────────────────────────────────────────

    #[test]
    fn test_join_bom_add() {
        let result = join_bom("hello", true);
        assert!(result.starts_with('\u{FEFF}'));
        assert_eq!(&result[3..], "hello");
    }

    #[test]
    fn test_join_bom_strip() {
        let input = "\u{FEFF}hello";
        let result = join_bom(input, false);
        assert_eq!(result, "hello");
    }

    #[test]
    fn test_join_bom_add_to_already_bom() {
        // Should not double-BOM
        let input = "\u{FEFF}hello";
        let result = join_bom(input, true);
        assert!(result.starts_with('\u{FEFF}'));
        // After the first BOM char, the rest is "hello" — no double BOM
        assert!(!result[3..].starts_with('\u{FEFF}'));
        assert_eq!(&result[3..], "hello");
    }

    // ── strip_heredoc tests ─────────────────────────────────────────────

    #[test]
    fn test_strip_heredoc_no_wrapper() {
        let input = "just plain text";
        assert_eq!(strip_heredoc(input), input);
    }

    #[test]
    fn test_strip_heredoc_basic() {
        let input = "<<EOF\ncontent here\nEOF";
        let result = strip_heredoc(input);
        assert_eq!(result, "content here");
    }

    #[test]
    fn test_strip_heredoc_with_cat() {
        let input = "cat <<'DELIM'\ncontent\nDELIM";
        let result = strip_heredoc(input);
        assert_eq!(result, "content");
    }

    // ── split_bom tests ─────────────────────────────────────────────────

    #[test]
    fn test_split_bom_with_bom() {
        let (has_bom, text) = split_bom("\u{FEFF}hello");
        assert!(has_bom);
        assert_eq!(text, "hello");
    }

    #[test]
    fn test_split_bom_without_bom() {
        let (has_bom, text) = split_bom("hello");
        assert!(!has_bom);
        assert_eq!(text, "hello");
    }

    // ── serde tests ─────────────────────────────────────────────────────

    #[test]
    fn test_hunk_serde_add() {
        let hunk = Hunk::Add {
            path: "test.rs".into(),
            contents: "fn main() {}".into(),
        };
        let json = serde_json::to_string(&hunk).expect("serialize");
        assert!(json.contains("\"type\":\"add\""));
        let parsed: Hunk = serde_json::from_str(&json).expect("deserialize");
        match parsed {
            Hunk::Add { path, contents } => {
                assert_eq!(path, "test.rs");
                assert_eq!(contents, "fn main() {}");
            }
            _ => panic!("wrong variant after roundtrip"),
        }
    }

    #[test]
    fn test_hunk_serde_delete() {
        let hunk = Hunk::Delete {
            path: "gone.rs".into(),
        };
        let json = serde_json::to_string(&hunk).expect("serialize");
        assert!(json.contains("\"type\":\"delete\""));
        let parsed: Hunk = serde_json::from_str(&json).expect("deserialize");
        match parsed {
            Hunk::Delete { path } => assert_eq!(path, "gone.rs"),
            _ => panic!("wrong variant"),
        }
    }

    #[test]
    fn test_hunk_serde_update() {
        let hunk = Hunk::Update {
            path: "mod.rs".into(),
            move_path: Some("new_mod.rs".into()),
            chunks: vec![UpdateFileChunk {
                old_lines: vec!["old".into()],
                new_lines: vec!["new".into()],
                change_context: Some("ctx".into()),
                end_of_file: Some(true),
            }],
        };
        let json = serde_json::to_string(&hunk).expect("serialize");
        assert!(json.contains("\"type\":\"update\""));
        assert!(json.contains("\"move_path\""));
        let parsed: Hunk = serde_json::from_str(&json).expect("deserialize");
        match parsed {
            Hunk::Update {
                path,
                move_path,
                chunks,
            } => {
                assert_eq!(path, "mod.rs");
                assert_eq!(move_path, Some("new_mod.rs".into()));
                assert_eq!(chunks.len(), 1);
            }
            _ => panic!("wrong variant"),
        }
    }

    #[test]
    fn test_hunk_serde_update_no_move() {
        let hunk = Hunk::Update {
            path: "mod.rs".into(),
            move_path: None,
            chunks: vec![],
        };
        let json = serde_json::to_string(&hunk).expect("serialize");
        // move_path should be absent when None
        assert!(!json.contains("move_path"));
    }

    #[test]
    fn test_update_file_chunk_serde() {
        let chunk = UpdateFileChunk {
            old_lines: vec!["old line".into()],
            new_lines: vec!["new line".into()],
            change_context: Some("@@ -1 +1 @@".into()),
            end_of_file: Some(false),
        };
        let json = serde_json::to_string(&chunk).expect("serialize");
        let parsed: UpdateFileChunk = serde_json::from_str(&json).expect("deserialize");
        assert_eq!(parsed.old_lines, vec!["old line"]);
        assert_eq!(parsed.new_lines, vec!["new line"]);
        assert_eq!(parsed.change_context, Some("@@ -1 +1 @@".into()));
        assert_eq!(parsed.end_of_file, Some(false));
    }

    #[test]
    fn test_update_file_chunk_serde_minimal() {
        let chunk = UpdateFileChunk {
            old_lines: vec![],
            new_lines: vec![],
            change_context: None,
            end_of_file: None,
        };
        let json = serde_json::to_string(&chunk).expect("serialize");
        let parsed: UpdateFileChunk = serde_json::from_str(&json).expect("deserialize");
        assert!(parsed.old_lines.is_empty());
        assert!(parsed.new_lines.is_empty());
        assert!(parsed.change_context.is_none());
        assert!(parsed.end_of_file.is_none());
    }

    #[test]
    fn test_file_update_serde() {
        let fu = FileUpdate {
            content: "updated content".into(),
            bom: true,
        };
        let json = serde_json::to_string(&fu).expect("serialize");
        let parsed: FileUpdate = serde_json::from_str(&json).expect("deserialize");
        assert_eq!(parsed.content, "updated content");
        assert!(parsed.bom);
    }

    // ── error tests ─────────────────────────────────────────────────────

    #[test]
    fn test_patch_error_parse_display() {
        let err = PatchError::ParseError {
            message: "invalid patch format".into(),
        };
        assert_eq!(err.to_string(), "parse error: invalid patch format");
    }

    #[test]
    fn test_patch_error_apply_display() {
        let err = PatchError::ApplyError {
            path: "src/main.rs".into(),
            message: "chunk not found".into(),
        };
        assert_eq!(
            err.to_string(),
            "apply error in `src/main.rs`: chunk not found"
        );
    }

    // ── normalize tests ─────────────────────────────────────────────────

    #[test]
    fn test_normalize_curly_quotes() {
        let input = "\u{2018}hello\u{2019} \u{201C}world\u{201D}";
        let result = normalize(input);
        assert_eq!(result, "'hello' \"world\"");
    }

    #[test]
    fn test_normalize_dashes() {
        let input = "foo\u{2013}bar\u{2014}baz";
        let result = normalize(input);
        assert_eq!(result, "foo-bar-baz");
    }

    #[test]
    fn test_normalize_ellipsis() {
        let input = "wait for it\u{2026}";
        let result = normalize(input);
        assert_eq!(result, "wait for it...");
    }

    #[test]
    fn test_normalize_nbsp() {
        let input = "hello\u{00A0}world";
        let result = normalize(input);
        assert_eq!(result, "hello world");
    }

    // ── seek / comparison tests ─────────────────────────────────────────

    #[test]
    fn test_seek_exact_match() {
        let lines: Vec<String> = vec!["one".into(), "two".into(), "three".into()];
        let pattern = vec!["two".into()];
        let found = seek(&lines, &pattern, 0, false);
        assert_eq!(found, Some(1));
    }

    #[test]
    fn test_seek_whitespace_match() {
        let lines: Vec<String> = vec!["one  ".into(), "two".into()];
        let pattern = vec!["one".into()];
        // rstrip should match
        let found = seek(&lines, &pattern, 0, false);
        assert_eq!(found, Some(0));
    }

    #[test]
    fn test_seek_not_found() {
        let lines: Vec<String> = vec!["one".into(), "two".into()];
        let pattern = vec!["missing".into()];
        let found = seek(&lines, &pattern, 0, false);
        assert_eq!(found, None);
    }

    #[test]
    fn test_seek_empty_pattern() {
        let lines: Vec<String> = vec!["one".into()];
        let pattern: Vec<String> = vec![];
        let found = seek(&lines, &pattern, 0, false);
        assert_eq!(found, None);
    }

    #[test]
    fn test_seek_eof_mode() {
        let lines: Vec<String> = vec!["a".into(), "b".into(), "c".into()];
        let pattern = vec!["c".into()];
        let found = seek(&lines, &pattern, 0, true);
        assert_eq!(found, Some(2));
    }
}
