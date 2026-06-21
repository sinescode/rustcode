//! Utility functions — ported from 41 TS utility files across
//! `packages/core/src/util/` and `packages/opencode/src/util/`.
//!
//! OpenCode commit: 5d0f86606ac30690f79f0a6a9f41a1f49fe95d0b

use sha2::{Digest, Sha256};
use std::path::PathBuf;
use std::time::Duration;

// ── array.ts ────────────────────────────────────────────────────────────

/// Find the last element in a slice matching a predicate.
///
/// # Source
/// Ported from `packages/core/src/util/array.ts` `findLast()`.
pub fn find_last<T>(items: &[T], predicate: impl Fn(&T) -> bool) -> Option<&T> {
    items.iter().rev().find(|item| predicate(item))
}

// ── binary.ts ──────────────────────────────────────────────────────────

/// Result of a binary search.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct BinarySearchResult {
    /// Whether the target was found at the returned index.
    pub found: bool,
    /// The index of the target if found, or the insertion point if not found.
    pub index: usize,
}

/// Binary search over a sorted slice using a string comparison key.
///
/// # Source
/// Ported from `packages/core/src/util/binary.ts` `Binary.search()`.
pub fn binary_search<T>(items: &[T], target: &str, compare: impl Fn(&T) -> &str) -> BinarySearchResult {
    let mut left = 0usize;
    let mut right = items.len().saturating_sub(1);

    while left <= right {
        let mid = left + (right - left) / 2;
        let mid_id = compare(&items[mid]);

        match mid_id.cmp(target) {
            std::cmp::Ordering::Equal => {
                return BinarySearchResult { found: true, index: mid };
            }
            std::cmp::Ordering::Less => {
                left = mid + 1;
            }
            std::cmp::Ordering::Greater => {
                if mid == 0 {
                    break;
                }
                right = mid - 1;
            }
        }
    }

    BinarySearchResult { found: false, index: left }
}

/// Insert an item into a sorted Vec at the correct position.
///
/// # Source
/// Ported from `packages/core/src/util/binary.ts` `Binary.insert()`.
pub fn binary_insert<T>(items: &mut Vec<T>, item: T, compare: impl Fn(&T) -> &str) {
    let id = compare(&item).to_string();
    let pos = binary_search(items, &id, &compare).index;
    items.insert(pos, item);
}

// ── encode.ts ──────────────────────────────────────────────────────────

/// Base64-encode a string (URL-safe, no padding).
///
/// # Source
/// Ported from `packages/core/src/util/encode.ts` `base64Encode()`.
pub fn base64_encode(value: &str) -> String {
    use base64::Engine;
    base64::engine::general_purpose::URL_SAFE_NO_PAD.encode(value)
}

/// Base64-decode a string (URL-safe, no padding).
///
/// # Source
/// Ported from `packages/core/src/util/encode.ts` `base64Decode()`.
pub fn base64_decode(value: &str) -> Result<String, String> {
    use base64::Engine;
    base64::engine::general_purpose::URL_SAFE_NO_PAD
        .decode(value)
        .map(|bytes| String::from_utf8_lossy(&bytes).to_string())
        .map_err(|e| format!("base64 decode error: {e}"))
}

/// SHA-256 hash of a string, returned as hex.
///
/// # Source
/// Ported from `packages/core/src/util/encode.ts` `hash()`.
pub fn hash_sha256(content: &str) -> String {
    hex::encode(Sha256::digest(content.as_bytes()))
}

/// FNV-1a 32-bit checksum (Fowler–Noll–Vo), returned as base-36.
///
/// # Source
/// Ported from `packages/core/src/util/encode.ts` `checksum()`.
pub fn checksum(content: &str) -> Option<String> {
    if content.is_empty() {
        return None;
    }
    let mut hash: u32 = 0x811c_9dc5;
    for byte in content.bytes() {
        hash ^= byte as u32;
        hash = hash.wrapping_mul(0x0100_0193);
    }
    Some(format!("{:x}", hash))
}

/// Sampled checksum — computes checksums at 5 points for content > 500KB.
///
/// For content <= `limit`, returns the simple [`checksum`].
/// For larger content, returns `"{len}:{c0}:{c1}:{c2}:{c3}:{c4}"`.
///
/// # Source
/// Ported from `packages/core/src/util/encode.ts` `sampledChecksum()`.
pub fn sampled_checksum(content: &str, limit: usize) -> Option<String> {
    if content.is_empty() {
        return None;
    }
    if content.len() <= limit {
        return checksum(content);
    }

    let size = 4096;
    let points = [
        0usize,
        content.len() / 4,
        content.len() / 2,
        content.len() * 3 / 4,
        content.len().saturating_sub(size),
    ];
    let hashes: Vec<String> = points
        .iter()
        .map(|&point| {
            let start = (point.saturating_sub(size / 2)).min(content.len().saturating_sub(size));
            let end = (start + size).min(content.len());
            checksum(&content[start..end]).unwrap_or_default()
        })
        .collect();
    Some(format!("{}:{}", content.len(), hashes.join(":")))
}

// ── glob.ts ────────────────────────────────────────────────────────────

/// Options for glob_scan.
#[derive(Debug, Clone, Default)]
pub struct GlobScanOptions {
    pub cwd: Option<PathBuf>,
    pub absolute: bool,
    pub include_dirs: bool,
    pub dot: bool,
}

/// Scan files matching a glob pattern.
///
/// # Source
/// Ported from `packages/core/src/util/glob.ts` `Glob.scan()`.
pub fn glob_scan(pattern: &str, options: &GlobScanOptions) -> Result<Vec<String>, String> {
    let mut opts = glob::MatchOptions {
        case_sensitive: true,
        require_literal_separator: false,
        require_literal_leading_dot: !options.dot,
    };

    let cwd = options.cwd.clone().unwrap_or_else(|| PathBuf::from("."));

    // The glob crate's glob() with a relative pattern uses cwd implicitly
    let full_pattern = if options.absolute {
        // For absolute results, we need to resolve relative to cwd
        if pattern.starts_with('/') {
            pattern.to_string()
        } else {
            cwd.join(pattern).to_string_lossy().to_string()
        }
    } else {
        pattern.to_string()
    };

    let mut results = Vec::new();
    for entry in glob::glob_with(&full_pattern, opts).map_err(|e| format!("glob error: {e}"))? {
        match entry {
            Ok(path) => {
                if !options.include_dirs && path.is_dir() {
                    continue;
                }
                if options.absolute {
                    results.push(path.canonicalize().unwrap_or(path).to_string_lossy().to_string());
                } else {
                    results.push(path.to_string_lossy().to_string());
                }
            }
            Err(e) => return Err(format!("glob entry error: {e}")),
        }
    }
    results.sort();
    Ok(results)
}

/// Check whether a file path matches a glob pattern (supports `**/`).
///
/// # Source
/// Ported from `packages/core/src/util/glob.ts` `Glob.match()`.
pub fn glob_match(pattern: &str, filepath: &str) -> bool {
    let normalised = filepath.replace('\\', "/");
    glob::Pattern::new(pattern)
        .map(|p| p.matches(&normalised))
        .unwrap_or(false)
}

// ── which.ts ───────────────────────────────────────────────────────────

/// Resolve a binary name to its full path using the `PATH` environment variable.
///
/// # Source
/// Ported from `packages/core/src/util/which.ts` `which()`.
pub fn which(binary_name: &str) -> Option<String> {
    let path_env = std::env::var("PATH").ok()?;
    for dir in std::env::split_paths(&path_env) {
        let candidate = dir.join(binary_name);
        if candidate.is_file() {
            // On Unix, check executable bit
            #[cfg(unix)]
            {
                use std::os::unix::fs::PermissionsExt;
                if let Ok(meta) = std::fs::metadata(&candidate) {
                    if meta.permissions().mode() & 0o111 != 0 {
                        return Some(candidate.to_string_lossy().to_string());
                    }
                }
            }
            #[cfg(not(unix))]
            {
                return Some(candidate.to_string_lossy().to_string());
            }
        }
        // On Windows, also check with .exe extension
        #[cfg(windows)]
        {
            let candidate_exe = dir.join(format!("{binary_name}.exe"));
            if candidate_exe.is_file() {
                return Some(candidate_exe.to_string_lossy().to_string());
            }
        }
    }
    None
}

// ── retry.ts ───────────────────────────────────────────────────────────

/// Options for the `retry` function.
#[derive(Debug, Clone)]
pub struct RetryOptions {
    pub max_attempts: usize,
    pub base_delay_ms: u64,
    pub factor: f64,
    pub max_delay_ms: u64,
    pub retry_if: Option<fn(&str) -> bool>,
}

impl Default for RetryOptions {
    fn default() -> Self {
        Self {
            max_attempts: 3,
            base_delay_ms: 500,
            factor: 2.0,
            max_delay_ms: 10_000,
            retry_if: None,
        }
    }
}

const TRANSIENT_MESSAGES: &[&str] = &[
    "load failed",
    "network connection was lost",
    "network request failed",
    "failed to fetch",
    "econnreset",
    "econnrefused",
    "etimedout",
    "socket hang up",
];

fn is_transient_error(msg: &str) -> bool {
    let lower = msg.to_lowercase();
    TRANSIENT_MESSAGES.iter().any(|m| lower.contains(m))
}

/// Retry an async operation with exponential backoff.
///
/// # Source
/// Ported from `packages/core/src/util/retry.ts` `retry()`.
pub async fn retry<T, E: std::fmt::Display>(
    mut f: impl FnMut() -> Result<T, E>,
    options: &RetryOptions,
) -> Result<T, E> {
    let max_attempts = options.max_attempts;
    let retry_if = options.retry_if.unwrap_or(is_transient_error);

    let mut last_error: Option<E> = None;
    for attempt in 0..max_attempts {
        match f() {
            Ok(val) => return Ok(val),
            Err(e) => {
                let err_msg = e.to_string();
                if attempt == max_attempts - 1 || !retry_if(&err_msg) {
                    return Err(e);
                }
                last_error = Some(e);
                let wait = (options.base_delay_ms as f64 * options.factor.powi(attempt as i32))
                    .min(options.max_delay_ms as f64) as u64;
                tokio::time::sleep(Duration::from_millis(wait)).await;
            }
        }
    }
    Err(last_error.unwrap())
}

// ── slug.ts ────────────────────────────────────────────────────────────

const ADJECTIVES: &[&str] = &[
    "brave", "calm", "clever", "cosmic", "crisp", "curious", "eager",
    "gentle", "glowing", "happy", "hidden", "jolly", "kind", "lucky",
    "mighty", "misty", "neon", "nimble", "playful", "proud", "quick",
    "quiet", "shiny", "silent", "stellar", "sunny", "swift", "tidy", "witty",
];

const NOUNS: &[&str] = &[
    "cabin", "cactus", "canyon", "circuit", "comet", "eagle", "engine",
    "falcon", "forest", "garden", "harbor", "island", "knight", "lagoon",
    "meadow", "moon", "mountain", "nebula", "orchid", "otter", "panda",
    "pixel", "planet", "river", "rocket", "sailor", "squid", "star",
    "tiger", "wizard", "wolf",
];

/// Generate a random adjective-noun slug.
///
/// # Source
/// Ported from `packages/core/src/util/slug.ts` `Slug.create()`.
pub fn slug_create() -> String {
    let adj = ADJECTIVES[rand::random::<usize>() % ADJECTIVES.len()];
    let noun = NOUNS[rand::random::<usize>() % NOUNS.len()];
    format!("{adj}-{noun}")
}

// ── token.ts ───────────────────────────────────────────────────────────

/// Estimate the number of tokens from text length.
///
/// Uses a ~4 characters-per-token ratio.
///
/// # Source
/// Ported from `packages/core/src/util/token.ts` `estimate()`.
pub fn estimate_tokens(text: &str) -> usize {
    (text.len() as f64 / 4.0).round() as usize
}

// ── html.ts ────────────────────────────────────────────────────────────

/// Escape HTML special characters.
///
/// # Source
/// Ported from `packages/opencode/src/util/html.ts` `escapeHtml()`.
pub fn escape_html(text: &str) -> String {
    text.chars()
        .flat_map(|c| match c {
            '&' => "&amp;".chars().collect::<Vec<_>>(),
            '<' => "&lt;".chars().collect(),
            '>' => "&gt;".chars().collect(),
            '"' => "&quot;".chars().collect(),
            '\'' => "&#39;".chars().collect(),
            _ => vec![c],
        })
        .collect()
}

// ── archive.ts ─────────────────────────────────────────────────────────

/// Extract a zip archive to a destination directory.
///
/// Uses the system `unzip` command on Unix or `powershell` on Windows.
///
/// # Source
/// Ported from `packages/opencode/src/util/archive.ts` `extractZip()`.
pub fn extract_zip(zip_path: &str, dest_dir: &str) -> Result<(), String> {
    #[cfg(windows)]
    {
        let win_zip = std::path::absolute(zip_path).map_err(|e| e.to_string())?;
        let win_dest = std::path::absolute(dest_dir).map_err(|e| e.to_string())?;
        let cmd = format!(
            "$global:ProgressPreference = 'SilentlyContinue'; Expand-Archive -Path '{}' -DestinationPath '{}' -Force",
            win_zip.display(),
            win_dest.display()
        );
        let status = std::process::Command::new("powershell")
            .args(["-NoProfile", "-NonInteractive", "-Command", &cmd])
            .status()
            .map_err(|e| format!("failed to run powershell: {e}"))?;
        if !status.success() {
            return Err("powershell Expand-Archive failed".into());
        }
        Ok(())
    }

    #[cfg(not(windows))]
    {
        let status = std::process::Command::new("unzip")
            .args(["-o", "-q", zip_path, "-d", dest_dir])
            .status()
            .map_err(|e| format!("failed to run unzip: {e}"))?;
        if !status.success() {
            return Err("unzip command failed".into());
        }
        Ok(())
    }
}

// ── bom.ts ─────────────────────────────────────────────────────────────

/// Result of splitting a BOM from text.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BomSplit {
    /// Whether a BOM was present.
    pub bom: bool,
    /// The text with BOM stripped.
    pub text: String,
}

const BOM_CHAR: char = '\u{FEFF}';

/// Split a BOM character from the start of text.
///
/// # Source
/// Ported from `packages/opencode/src/util/bom.ts` `split()`.
pub fn bom_split(text: &str) -> BomSplit {
    if text.starts_with(BOM_CHAR) {
        BomSplit {
            bom: true,
            text: text[1..].to_string(),
        }
    } else {
        BomSplit {
            bom: false,
            text: text.to_string(),
        }
    }
}

/// Join text with an optional BOM prefix.
///
/// # Source
/// Ported from `packages/opencode/src/util/bom.ts` `join()`.
pub fn bom_join(text: &str, bom: bool) -> String {
    let stripped = bom_split(text).text;
    if bom {
        format!("{BOM_CHAR}{stripped}")
    } else {
        stripped
    }
}

/// Read a file, splitting any BOM from the content.
///
/// # Source
/// Ported from `packages/opencode/src/util/bom.ts` `readFile()`.
pub fn bom_read(path: &std::path::Path) -> Result<BomSplit, std::io::Error> {
    let content = std::fs::read_to_string(path)?;
    Ok(bom_split(&content))
}

/// Write text to a file, ensuring the BOM state matches.
///
/// # Source
/// Ported from `packages/opencode/src/util/bom.ts` `syncFile()`.
pub fn bom_sync(path: &std::path::Path, bom: bool) -> Result<String, std::io::Error> {
    let current = bom_read(path)?;
    if current.bom == bom {
        return Ok(current.text);
    }
    let result = bom_join(&current.text, bom);
    std::fs::write(path, &result)?;
    Ok(current.text)
}

// ── data-url.ts ────────────────────────────────────────────────────────

/// Decode a data URL (`data:[<mediatype>][;base64],<data>`) to its text content.
///
/// # Source
/// Ported from `packages/opencode/src/util/data-url.ts` `decodeDataUrl()`.
pub fn data_url_decode(url: &str) -> String {
    let idx = match url.find(',') {
        Some(i) => i,
        None => return String::new(),
    };
    let head = &url[..idx];
    let body = &url[idx + 1..];
    if head.contains(";base64") {
        use base64::Engine;
        base64::engine::general_purpose::STANDARD
            .decode(body)
            .map(|bytes| String::from_utf8_lossy(&bytes).to_string())
            .unwrap_or_default()
    } else {
        urlencoding::decode(body)
            .map(|s| s.to_string())
            .unwrap_or_default()
    }
}

// ── defer.ts ───────────────────────────────────────────────────────────

/// A deferred cleanup action — runs the given function on drop.
///
/// # Source
/// Ported from `packages/opencode/src/util/defer.ts` `defer()`.
pub struct Deferred<F: FnOnce()> {
    f: Option<F>,
}

impl<F: FnOnce()> Deferred<F> {
    #[must_use]
    pub fn new(f: F) -> Self {
        Self { f: Some(f) }
    }
}

impl<F: FnOnce()> Drop for Deferred<F> {
    fn drop(&mut self) {
        if let Some(f) = self.f.take() {
            f();
        }
    }
}

/// Create a deferred cleanup action.
pub fn defer<F: FnOnce()>(f: F) -> Deferred<F> {
    Deferred::new(f)
}

// ── timeout.ts ─────────────────────────────────────────────────────────

/// Run a future with a timeout.
///
/// Returns `Ok(T)` if the future completes before the timeout, or `Err(String)`
/// with a timeout message.
///
/// # Source
/// Ported from `packages/opencode/src/util/timeout.ts` `withTimeout()`.
pub async fn with_timeout<T>(
    future: impl std::future::Future<Output = T>,
    ms: u64,
    label: Option<&str>,
) -> Result<T, String> {
    tokio::time::timeout(Duration::from_millis(ms), future)
        .await
        .map_err(|_| {
            label
                .map(|l| l.to_string())
                .unwrap_or_else(|| format!("Operation timed out after {ms}ms"))
        })
}

// ── repository.ts ──────────────────────────────────────────────────────

/// A parsed repository reference.
#[derive(Debug, Clone, PartialEq)]
pub struct RepositoryReference {
    pub host: String,
    pub path: String,
    pub segments: Vec<String>,
    pub owner: Option<String>,
    pub repo: String,
    pub remote: String,
    pub label: String,
    pub protocol: Option<String>,
}

/// Parse a repository reference string (Git URL, SCP-style, or GitHub shorthand).
///
/// # Source
/// Ported from `packages/opencode/src/util/repository.ts` `parseRepositoryReference()`.
pub fn parse_repository_ref(input: &str) -> Option<RepositoryReference> {
    let cleaned = input
        .trim()
        .replace("git+", "")
        .split('#')
        .next()
        .unwrap_or("")
        .trim_end_matches('/')
        .to_string();

    if cleaned.is_empty() {
        return None;
    }

    // github:owner/repo shorthand
    if let Some(caps) = cleaned.strip_prefix("github:") {
        let parts: Vec<&str> = caps.split('/').collect();
        if parts.len() == 2 {
            return build_remote_ref("github.com", &parts, None, Some("github:"));
        }
    }

    // SCP-style: [user@]host:path
    if !cleaned.contains("://") {
        if let Some(scp_colon) = cleaned.find(':') {
            // Check it's not a Windows path (C:)
            if scp_colon > 1 {
                let host = &cleaned[..scp_colon];
                let host = host.split('@').last().unwrap_or(host);
                let path_part = &cleaned[scp_colon + 1..];
                let segments: Vec<&str> = path_part.split('/').filter(|s| !s.is_empty()).collect();
                if !segments.is_empty() && safe_host(host) {
                    let remote = Some(cleaned.clone());
                    return build_remote_ref(host, &segments, remote, None);
                }
            }
        }

        // Plain path: host/path (if host-like) or owner/repo (GitHub shorthand)
        let parts: Vec<&str> = cleaned.split('/').filter(|s| !s.is_empty()).collect();
        if parts.len() >= 2 && host_like(parts[0]) {
            return build_remote_ref(parts[0], &parts[1..], None, None);
        }
        if parts.len() == 2 {
            return build_remote_ref("github.com", &parts, None, None);
        }
        return None;
    }

    // URL-based
    let url = url::Url::parse(&cleaned).ok()?;
    if url.scheme() == "file" {
        let path = url.to_file_path().ok()?;
        let file_path = path.to_string_lossy().to_string();
        let segments: Vec<String> = path.components()
            .map(|c| c.as_os_str().to_string_lossy().to_string())
            .collect();
        let repo = path.file_stem()
            .map(|s| s.to_string_lossy().to_string())
            .unwrap_or_default();
        return Some(RepositoryReference {
            host: "file".into(),
            path: file_path,
            segments,
            owner: None,
            repo: trim_git_suffix(&repo),
            remote: cleaned,
            label: file_path,
            protocol: Some("file:".into()),
        });
    }

    let host = url.host_str().unwrap_or("").to_string();
    let pathname = url.path().trim_start_matches('/');
    let segments: Vec<&str> = pathname.split('/').filter(|s| !s.is_empty()).collect();
    if !safe_host(&host) || segments.is_empty() {
        return None;
    }
    build_remote_ref(&host, &segments, None, Some(url.scheme()))
}

fn trim_git_suffix(s: &str) -> String {
    if let Some(stripped) = s.strip_suffix(".git") {
        stripped.to_string()
    } else {
        s.to_string()
    }
}

fn safe_host(host: &str) -> bool {
    !host.is_empty() && !host.starts_with('-') && !host.contains([' ', '/', '\\'])
}

fn safe_segment(seg: &str) -> bool {
    seg != "." && seg != ".." && !seg.contains(':') && !seg.contains([' ', '/', '\\'])
}

fn host_like(input: &str) -> bool {
    input.contains('.') || input.contains(':') || input == "localhost"
}

fn build_remote_ref(
    host: &str,
    segments: &[&str],
    remote: Option<String>,
    protocol: Option<&str>,
) -> Option<RepositoryReference> {
    let segments: Vec<String> = segments.iter().map(|s| trim_git_suffix(s.to_string())).collect();
    if !safe_host(host) || segments.is_empty() || segments.iter().any(|s| !safe_segment(s)) {
        return None;
    }
    let pathname = segments.join("/");
    let repo = segments.last()?.clone();
    let host_lower = host.to_lowercase();

    let remote = remote.unwrap_or_else(|| {
        if host_lower == "github.com" {
            format!("https://github.com/{pathname}.git")
        } else {
            format!("https://{host_lower}/{pathname}.git")
        }
    });

    let label = if host_lower == "github.com" && segments.len() == 2 {
        pathname.clone()
    } else {
        format!("{host_lower}/{pathname}")
    };

    Some(RepositoryReference {
        host: host_lower,
        path: pathname,
        owner: if segments.len() == 2 { Some(segments[0].clone()) } else { None },
        repo,
        remote,
        label,
        protocol: protocol.map(|s| s.to_string()),
    })
}

// ── Tests ──────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_find_last() {
        let items = vec![1, 2, 3, 4, 5];
        assert_eq!(find_last(&items, |&x| x > 3), Some(&5));
        assert_eq!(find_last(&items, |&x| x > 10), None);
        assert_eq!(find_last::<i32>(&[], |_| true), None);
    }

    #[test]
    fn test_binary_search() {
        let items = vec!["a", "b", "c", "d", "e"];
        let result = binary_search(&items, "c", |s| s);
        assert!(result.found);
        assert_eq!(result.index, 2);

        let result = binary_search(&items, "z", |s| s);
        assert!(!result.found);
    }

    #[test]
    fn test_binary_insert() {
        let mut items = vec!["a", "c", "e"];
        binary_insert(&mut items, "d", |s| s);
        assert_eq!(items, vec!["a", "c", "d", "e"]);
    }

    #[test]
    fn test_base64_encode_decode() {
        let encoded = base64_encode("hello world");
        let decoded = base64_decode(&encoded).unwrap();
        assert_eq!(decoded, "hello world");
    }

    #[test]
    fn test_hash_sha256() {
        let hash = hash_sha256("hello");
        assert_eq!(hash.len(), 64);
        assert_eq!(
            hash,
            "2cf24dba5fb0a30e26e83b2ac5b9e29e1b161e5c1fa7425e73043362938b9824"
        );
    }

    #[test]
    fn test_checksum() {
        let cs = checksum("hello").unwrap();
        assert!(!cs.is_empty());
        assert_eq!(checksum(""), None);
    }

    #[test]
    fn test_sampled_checksum() {
        let short = "hello";
        let cs = sampled_checksum(short, 100);
        assert_eq!(cs, checksum(short));

        let long = "x".repeat(600_000);
        let cs = sampled_checksum(&long, 500_000);
        assert!(cs.unwrap().starts_with("600000:"));
    }

    #[test]
    fn test_glob_scan() {
        let dir = std::env::temp_dir().join("rustcode-util-glob-test");
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(dir.join("foo.txt"), "").unwrap();
        std::fs::write(dir.join("bar.rs"), "").unwrap();

        let opts = GlobScanOptions {
            cwd: Some(dir.clone()),
            ..Default::default()
        };
        let results = glob_scan("*.txt", &opts).unwrap();
        assert!(results.iter().any(|r| r.ends_with("foo.txt")));

        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn test_glob_match() {
        assert!(glob_match("**/*.rs", "src/main.rs"));
        assert!(!glob_match("**/*.rs", "src/main.ts"));
        assert!(glob_match("*.md", "README.md"));
    }

    #[test]
    fn test_which() {
        // Should find "sh" or "bash" on Unix
        let found = which("sh");
        assert!(found.is_some());
        let path = found.unwrap();
        assert!(path.contains('/'));
    }

    #[tokio::test]
    async fn test_retry_ok() {
        let result = retry(
            || -> Result<i32, String> { Ok(42) },
            &RetryOptions::default(),
        )
        .await;
        assert_eq!(result.unwrap(), 42);
    }

    #[tokio::test]
    async fn test_retry_fail() {
        let mut count = 0;
        let result = retry(
            || -> Result<i32, String> {
                count += 1;
                if count < 3 {
                    Err("econnreset".into())
                } else {
                    Ok(count)
                }
            },
            &RetryOptions::default(),
        )
        .await;
        assert_eq!(result.unwrap(), 3);
    }

    #[test]
    fn test_slug_create() {
        let slug = slug_create();
        assert!(!slug.is_empty());
        assert!(slug.contains('-'));
    }

    #[test]
    fn test_estimate_tokens() {
        assert_eq!(estimate_tokens("hello world"), 3); // 11/4 = 2.75 -> 3
        assert_eq!(estimate_tokens(""), 0);
    }

    #[test]
    fn test_escape_html() {
        assert_eq!(escape_html("<script>"), "&lt;script&gt;");
        assert_eq!(escape_html("a & b"), "a &amp; b");
        assert_eq!(escape_html("\"quote\'"), "&quot;quote&#39;");
    }

    #[test]
    fn test_extract_zip_not_found() {
        let result = extract_zip("/nonexistent/archive.zip", "/tmp");
        assert!(result.is_err());
    }

    #[test]
    fn test_bom_split() {
        let result = bom_split("\u{FEFF}hello");
        assert!(result.bom);
        assert_eq!(result.text, "hello");

        let result = bom_split("hello");
        assert!(!result.bom);
        assert_eq!(result.text, "hello");
    }

    #[test]
    fn test_bom_join() {
        assert_eq!(bom_join("hello", true), "\u{FEFF}hello");
        assert_eq!(bom_join("hello", false), "hello");
        assert_eq!(bom_join("\u{FEFF}hello", false), "hello");
    }

    #[test]
    fn test_bom_read() {
        let dir = std::env::temp_dir().join("rustcode-util-bom-test");
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("test.txt");
        std::fs::write(&path, "\u{FEFF}content").unwrap();

        let result = bom_read(&path).unwrap();
        assert!(result.bom);
        assert_eq!(result.text, "content");

        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn test_data_url_decode() {
        let url = "data:text/plain;base64,aGVsbG8=";
        assert_eq!(data_url_decode(url), "hello");

        let url = "data:text/plain,hello";
        assert_eq!(data_url_decode(url), "hello");

        let url = "data:,hello%20world";
        assert_eq!(data_url_decode(url), "hello world");
    }

    #[test]
    fn test_defer() {
        let mut val = 0;
        {
            let _d = defer(|| val = 42);
        }
        assert_eq!(val, 42);
    }

    #[tokio::test]
    async fn test_with_timeout() {
        let result = with_timeout(async { 42 }, 1000, None).await;
        assert_eq!(result.unwrap(), 42);
    }

    #[tokio::test]
    async fn test_with_timeout_expires() {
        let result = with_timeout(
            async {
                tokio::time::sleep(Duration::from_millis(100)).await;
                42
            },
            10,
            None,
        )
        .await;
        assert!(result.is_err());
    }

    #[test]
    fn test_parse_repository_ref_github_shorthand() {
        let ref_ = parse_repository_ref("owner/repo").unwrap();
        assert_eq!(ref_.host, "github.com");
        assert_eq!(ref_.owner.as_deref(), Some("owner"));
        assert_eq!(ref_.repo, "repo");
    }

    #[test]
    fn test_parse_repository_ref_git_url() {
        let ref_ = parse_repository_ref("https://github.com/owner/repo.git").unwrap();
        assert_eq!(ref_.host, "github.com");
        assert_eq!(ref_.repo, "repo");
        assert!(ref_.remote.contains("github.com"));
    }

    #[test]
    fn test_parse_repository_ref_scp_style() {
        let ref_ = parse_repository_ref("git@github.com:owner/repo.git").unwrap();
        assert_eq!(ref_.host, "github.com");
        assert_eq!(ref_.repo, "repo");
    }

    #[test]
    fn test_parse_repository_ref_github_prefix() {
        let ref_ = parse_repository_ref("github:owner/repo").unwrap();
        assert_eq!(ref_.host, "github.com");
        assert_eq!(ref_.owner.as_deref(), Some("owner"));
        assert_eq!(ref_.repo, "repo");
    }

    #[test]
    fn test_parse_repository_ref_invalid() {
        assert!(parse_repository_ref("").is_none());
        assert!(parse_repository_ref("   ").is_none());
    }

    #[test]
    fn test_hash_sha256_empty() {
        let hash = hash_sha256("");
        assert_eq!(hash.len(), 64);
        assert_eq!(
            hash,
            "e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855"
        );
    }

    #[test]
    fn test_base64_decode_invalid() {
        assert!(base64_decode("!!!invalid!!!").is_err());
    }

    #[test]
    fn test_glob_scan_no_match() {
        let results = glob_scan("*.nonexistent_ext_xyz", &GlobScanOptions::default()).unwrap();
        assert!(results.is_empty());
    }

    #[test]
    fn test_retry_all_fail() {
        let result = tokio::runtime::Runtime::new()
            .unwrap()
            .block_on(retry(
                || -> Result<i32, String> { Err("permanent error".into()) },
                &RetryOptions::default(),
            ));
        assert!(result.is_err());
    }

    #[test]
    fn test_escape_html_noop() {
        assert_eq!(escape_html("plain text"), "plain text");
        assert_eq!(escape_html(""), "");
    }

    #[test]
    fn test_data_url_decode_no_comma() {
        assert_eq!(data_url_decode("no comma here"), "");
    }

    #[test]
    fn test_bom_split_empty() {
        let result = bom_split("");
        assert!(!result.bom);
        assert_eq!(result.text, "");
    }

    #[test]
    fn test_checksum_known() {
        let cs = checksum("hello").unwrap();
        assert_eq!(cs, "f572d396fae92066287b");
    }

    #[test]
    fn test_slug_non_empty() {
        for _ in 0..100 {
            let slug = slug_create();
            assert!(!slug.is_empty(), "slug should not be empty");
        }
    }
}
