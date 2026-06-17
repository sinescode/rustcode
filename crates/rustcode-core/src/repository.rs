//! Repository types — reference parsing, branch validation, cache management.
//!
//! Ported from:
//! - `packages/core/src/repository.ts` — Reference types (BaseReference, RemoteReference,
//!   FileReference), parse/parseRemote/validateBranch, cache helpers, error classes
//! - `packages/core/src/repository-cache.ts` — Result, EnsureInput, cache error classes
//!
//! OpenCode commit: 5d0f86606ac30690f79f0a6a9f41a1f49fe95d0b

use serde::{Deserialize, Serialize};

// ══════════════════════════════════════════════════════════════════════════════
// Reference types — parsed repository identifiers
// ══════════════════════════════════════════════════════════════════════════════

/// Common fields shared by all repository references.
///
/// # Source
/// `packages/core/src/repository.ts` lines 5–13.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct BaseReference {
    /// Host component (e.g., `"github.com"`, `"gitlab.com"`, or `"file"`).
    pub host: String,

    /// Slash-separated repository path (e.g., `"owner/repo"`).
    pub path: String,

    /// The path split into segments.
    pub segments: Vec<String>,

    /// The repository owner (only set for two-segment GitHub-style paths).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub owner: Option<String>,

    /// The repository name (last segment, with `.git` suffix stripped).
    pub repo: String,

    /// The full remote URL for cloning.
    pub remote: String,

    /// Human-readable label for display.
    pub label: String,
}

/// A remote repository reference (HTTP/SSH URL or host/path shorthand).
///
/// # Source
/// `packages/core/src/repository.ts` lines 15–18.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RemoteReference {
    #[serde(flatten)]
    pub base: BaseReference,

    /// Optional protocol (e.g., `"https:"`, `"ssh:"`).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub protocol: Option<String>,
}

/// A local file-system repository reference.
///
/// # Source
/// `packages/core/src/repository.ts` lines 20–23.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct FileReference {
    #[serde(flatten)]
    pub base: BaseReference,

    /// Always `"file"` for local references.
    pub host: String,

    /// Always `"file:"` for local references.
    pub protocol: String,
}

/// Tagged union of remote and local file references.
///
/// # Source
/// `packages/core/src/repository.ts` line 24.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(untagged)]
pub enum RepositoryReference {
    Remote(RemoteReference),
    File(FileReference),
}

impl RepositoryReference {
    /// Check if this is a remote reference.
    ///
    /// # Source
    /// `packages/core/src/repository.ts` lines 113–115.
    #[must_use]
    pub fn is_remote(&self) -> bool {
        matches!(self, Self::Remote(_))
    }

    /// Check if this is a local file reference.
    ///
    /// # Source
    /// `packages/core/src/repository.ts` lines 117–119.
    #[must_use]
    pub fn is_file(&self) -> bool {
        matches!(self, Self::File(_))
    }

    /// Get the cache filesystem path for this reference.
    ///
    /// # Source
    /// `packages/core/src/repository.ts` lines 121–123.
    #[must_use]
    pub fn cache_path(&self, root: &str) -> String {
        let (host, segments) = match self {
            Self::Remote(r) => (&r.base.host, &r.base.segments),
            Self::File(f) => (&f.base.host, &f.base.segments),
        };
        let mut path = root.to_string();
        for part in host.split(':') {
            path.push('/');
            path.push_str(part);
        }
        for segment in segments {
            path.push('/');
            path.push_str(segment);
        }
        path
    }

    /// Get the cache identity string (host/path).
    ///
    /// # Source
    /// `packages/core/src/repository.ts` lines 125–127.
    #[must_use]
    pub fn cache_identity(&self) -> String {
        match self {
            Self::Remote(r) => format!("{}/{}", r.base.host, r.base.path),
            Self::File(f) => format!("{}/{}", f.base.host, f.base.path),
        }
    }

    /// Check whether two references refer to the same repository.
    ///
    /// # Source
    /// `packages/core/src/repository.ts` lines 129–131.
    #[must_use]
    pub fn same(&self, other: &Self) -> bool {
        self.cache_identity() == other.cache_identity()
    }
}

// ══════════════════════════════════════════════════════════════════════════════
// Repository errors
// ══════════════════════════════════════════════════════════════════════════════

/// Errors related to repository reference parsing and validation.
///
/// # Source
/// `packages/core/src/repository.ts` lines 26–46.
#[derive(Debug, Clone, thiserror::Error)]
pub enum RepositoryError {
    /// The repository string is not a valid reference.
    #[error("invalid repository reference '{repository}': {message}")]
    InvalidReference {
        repository: String,
        message: String,
    },

    /// Local file repositories are not supported for remote operations.
    #[error("local repository not supported '{repository}': {message}")]
    UnsupportedLocal {
        repository: String,
        message: String,
    },

    /// The branch name fails validation.
    #[error("invalid branch name '{branch}': {message}")]
    InvalidBranch { branch: String, message: String },
}

// ══════════════════════════════════════════════════════════════════════════════
// Repository parsing and validation
// ══════════════════════════════════════════════════════════════════════════════

/// Parse a repository reference string into a structured `RepositoryReference`.
///
/// Supports these input formats:
/// - `github:owner/repo` — explicit GitHub shorthand
/// - `host.com/owner/repo` — host/path style
/// - `owner/repo` — implicit GitHub shorthand
/// - `https://host.com/owner/repo` — full URL
/// - `git@host.com:owner/repo` — SCP-style SSH
/// - `file:///path/to/repo` — local file reference
///
/// # Source
/// `packages/core/src/repository.ts` lines 57–86.
#[must_use]
pub fn parse_repository(input: &str) -> Option<RepositoryReference> {
    let cleaned = normalize_repo_input(input);
    if cleaned.is_empty() {
        return None;
    }

    // github:owner/repo shorthand
    if let Some(caps) = regex_github_prefix(&cleaned) {
        return Some(build_remote(RemoteBuildInput {
            host: "github.com".into(),
            segments: vec![caps.owner, caps.repo],
            remote: None,
            protocol: None,
        }));
    }

    // No protocol prefix — try SCP-style or direct host/path
    if !cleaned.contains("://") {
        if let Some(caps) = regex_scp(&cleaned) {
            return Some(build_remote(RemoteBuildInput {
                host: caps.host,
                segments: repo_parts(&caps.path),
                remote: Some(cleaned.clone()),
                protocol: None,
            }));
        }

        let parts = repo_parts(&cleaned);
        if parts.len() >= 2 && is_host_like(&parts[0]) {
            return Some(build_remote(RemoteBuildInput {
                host: parts[0].clone(),
                segments: parts[1..].to_vec(),
                remote: None,
                protocol: None,
            }));
        }
        if parts.len() == 2 {
            return Some(build_remote(RemoteBuildInput {
                host: "github.com".into(),
                segments: parts,
                remote: None,
                protocol: None,
            }));
        }

        return None;
    }

    // Full URL
    if let Ok(url) = url::Url::parse(&cleaned) {
        if url.scheme() == "file" {
            return build_file_reference(&url, &cleaned);
        }
        let segments = repo_parts(url.path());
        return Some(build_remote(RemoteBuildInput {
            host: url.host_str().unwrap_or("").to_string(),
            segments,
            remote: if url.host_str() == Some("github.com") {
                Some(github_remote_url("github.com", &segments.join("/")))
            } else {
                Some(cleaned)
            },
            protocol: Some(url.scheme().to_string()),
        }));
    }

    None
}

/// Parse a repository reference, throwing an error if it is invalid or local-only.
///
/// # Source
/// `packages/core/src/repository.ts` lines 88–103.
pub fn parse_remote_repository(input: &str) -> Result<RemoteReference, RepositoryError> {
    let reference = parse_repository(input).ok_or_else(|| RepositoryError::InvalidReference {
        repository: input.to_string(),
        message: "Repository must be a git URL, host/path reference, or GitHub owner/repo shorthand".into(),
    })?;

    match reference {
        RepositoryReference::Remote(r) => Ok(r),
        RepositoryReference::File(_) => Err(RepositoryError::UnsupportedLocal {
            repository: input.to_string(),
            message: "Local file repositories are not supported".into(),
        }),
    }
}

/// Validate a branch name.
///
/// Allowed: alphanumeric, `/`, `_`, `.`, `-` (must not start with `-` or contain `..`).
///
/// # Source
/// `packages/core/src/repository.ts` lines 105–111.
pub fn validate_branch(branch: &str) -> Result<(), RepositoryError> {
    let valid = branch
        .chars()
        .all(|c| c.is_ascii_alphanumeric() || c == '/' || c == '_' || c == '.' || c == '-')
        && !branch.starts_with('-')
        && !branch.contains("..");

    if valid {
        Ok(())
    } else {
        Err(RepositoryError::InvalidBranch {
            branch: branch.to_string(),
            message: "Branch must contain only alphanumeric characters, /, _, ., and -, and cannot start with - or contain ..".into(),
        })
    }
}

// ══════════════════════════════════════════════════════════════════════════════
// Repository Cache — Result, EnsureInput, cache errors
// ══════════════════════════════════════════════════════════════════════════════

/// Result of a repository cache operation.
///
/// # Source
/// `packages/core/src/repository-cache.ts` lines 9–17.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RepositoryCacheResult {
    /// The repository label from the input reference.
    pub repository: String,

    /// The host component of the reference.
    pub host: String,

    /// The full remote clone URL.
    pub remote: String,

    /// Local filesystem path where the repository is cached.
    pub local_path: String,

    /// The operation performed.
    pub status: RepositoryCacheStatus,

    /// HEAD commit hash, if available.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub head: Option<String>,

    /// Current branch name, if available.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub branch: Option<String>,
}

/// Status of a repository cache operation.
///
/// # Source
/// `packages/core/src/repository-cache.ts` lines 14 — the `status` literal union.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RepositoryCacheStatus {
    /// Already cached and up-to-date.
    Cached,
    /// Freshly cloned.
    Cloned,
    /// Existing clone was refreshed (fetched + reset).
    Refreshed,
}

/// Input for the `ensure` cache operation.
///
/// # Source
/// `packages/core/src/repository-cache.ts` lines 19–23.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RepositoryCacheEnsureInput {
    /// The parsed remote reference to cache.
    pub reference: RemoteReference,

    /// Whether to force a refresh (fetch + reset) even if cached.
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub refresh: bool,

    /// Optional branch to checkout after cloning/refreshing.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub branch: Option<String>,
}

/// Errors that can occur during repository cache operations.
///
/// # Source
/// `packages/core/src/repository-cache.ts` lines 25–78.
#[derive(Debug, Clone, thiserror::Error)]
pub enum RepositoryCacheError {
    /// The repository reference is invalid.
    #[error("invalid repository '{repository}': {message}")]
    InvalidRepository { repository: String, message: String },

    /// The branch name is invalid.
    #[error("invalid branch '{branch}': {message}")]
    InvalidBranch { branch: String, message: String },

    /// Clone operation failed.
    #[error("clone failed for '{repository}': {message}")]
    CloneFailed { repository: String, message: String },

    /// Fetch operation failed.
    #[error("fetch failed for '{repository}': {message}")]
    FetchFailed { repository: String, message: String },

    /// Checkout operation failed.
    #[error("checkout failed for '{repository}' branch '{branch}': {message}")]
    CheckoutFailed {
        repository: String,
        branch: String,
        message: String,
    },

    /// Reset operation failed.
    #[error("reset failed for '{repository}': {message}")]
    ResetFailed { repository: String, message: String },

    /// Failed to acquire lock on the local cache directory.
    #[error("lock failed for '{local_path}': {message}")]
    LockFailed { local_path: String, message: String },

    /// Generic cache operation error (mkdir, remove, etc.).
    #[error("cache operation '{operation}' failed at '{path}': {message}")]
    CacheOperation {
        operation: String,
        path: String,
        message: String,
    },
}

// ══════════════════════════════════════════════════════════════════════════════
// Internal helpers
// ══════════════════════════════════════════════════════════════════════════════

/// Normalize input: trim, strip `git+` prefix, strip `#fragment`, strip trailing `/`.
fn normalize_repo_input(input: &str) -> String {
    let mut s = input.trim().to_string();
    if let Some(stripped) = s.strip_prefix("git+") {
        s = stripped.to_string();
    }
    if let Some(hash_pos) = s.find('#') {
        s.truncate(hash_pos);
    }
    s = s.trim_end_matches('/').to_string();
    s
}

/// Strip trailing `.git` suffix.
fn trim_git_suffix(input: &str) -> String {
    input.strip_suffix(".git").unwrap_or(input).to_string()
}

/// Split a path into non-empty segments, stripping `.git` suffixes.
fn repo_parts(input: &str) -> Vec<String> {
    input
        .split('/')
        .map(|s| trim_git_suffix(s.trim()))
        .filter(|s| !s.is_empty())
        .collect()
}

/// Check if a string looks like a hostname.
fn is_host_like(input: &str) -> bool {
    input.contains('.') || input.contains(':') || input == "localhost"
}

/// Validate a host string: non-empty, no leading `-`, no whitespace/backslash.
fn is_safe_host(host: &str) -> bool {
    !host.is_empty() && !host.starts_with('-') && !host.contains(|c: char| c.is_whitespace() || c == '\\')
}

/// Validate a path segment: not `.` or `..`, no `:`, no whitespace/backslash.
fn is_safe_segment(segment: &str) -> bool {
    segment != "." && segment != ".." && !segment.contains(':') && !segment.contains(|c: char| c.is_whitespace() || c == '\\')
}

/// Generate the GitHub remote URL for a given pathname.
fn github_remote_url(_host: &str, pathname: &str) -> String {
    // In the TS source, this reads OPENCODE_REPO_CLONE_GITHUB_BASE_URL env var.
    // Default: https://github.com/{pathname}.git
    format!("https://github.com/{pathname}.git")
}

struct RemoteBuildInput {
    host: String,
    segments: Vec<String>,
    remote: Option<String>,
    protocol: Option<String>,
}

/// Build a `RemoteReference` from parsed components.
fn build_remote(input: RemoteBuildInput) -> RepositoryReference {
    let segments: Vec<String> = input
        .segments
        .into_iter()
        .map(|s| trim_git_suffix(&s))
        .filter(|s| !s.is_empty())
        .collect();

    let host = input.host.to_lowercase();
    let repository_path = segments.join("/");

    RepositoryReference::Remote(RemoteReference {
        base: BaseReference {
            host: host.clone(),
            path: repository_path.clone(),
            segments: segments.clone(),
            owner: if segments.len() == 2 {
                Some(segments[0].clone())
            } else {
                None
            },
            repo: segments
                .last()
                .cloned()
                .unwrap_or_else(|| "unknown".into()),
            remote: input.remote.unwrap_or_else(|| {
                if host == "github.com" {
                    github_remote_url(&host, &repository_path)
                } else {
                    format!("https://{host}/{repository_path}.git")
                }
            }),
            label: if host == "github.com" && segments.len() == 2 {
                repository_path
            } else {
                format!("{host}/{repository_path}")
            },
        },
        protocol: input.protocol,
    })
}

/// Build a `FileReference` from a `file://` URL.
fn build_file_reference(url: &url::Url, remote: &str) -> Option<RepositoryReference> {
    let file_path = url.to_file_path().ok()?;
    let normalized = file_path.to_string_lossy().to_string();
    let segments: Vec<String> = normalized
        .split(&['/', '\\'][..])
        .map(|s| s.trim_end_matches(':').to_string())
        .filter(|s| !s.is_empty())
        .collect();

    if segments.is_empty() {
        return None;
    }

    let repo = trim_git_suffix(segments.last().unwrap());

    Some(RepositoryReference::File(FileReference {
        base: BaseReference {
            host: "file".into(),
            path: normalized.clone(),
            segments: segments.clone(),
            owner: None,
            repo,
            remote: remote.to_string(),
            label: normalized,
        },
        host: "file".into(),
        protocol: "file:".into(),
    }))
}

fn regex_github_prefix(input: &str) -> Option<GithubPrefixMatch> {
    let rest = input.strip_prefix("github:")?;
    let mut parts = rest.splitn(2, '/');
    let owner = parts.next()?.to_string();
    let repo = parts.next()?.to_string();
    if owner.is_empty() || repo.is_empty() {
        return None;
    }
    Some(GithubPrefixMatch { owner, repo })
}

fn regex_scp(input: &str) -> Option<ScpMatch> {
    let colon_pos = input.find(':')?;
    let before = &input[..colon_pos];
    let after = &input[colon_pos + 1..];

    // SCP: optional user@, then host, then colon, then path
    let host = if let Some(at_pos) = before.rfind('@') {
        &before[at_pos + 1..]
    } else {
        before
    };

    if host.is_empty() || after.is_empty() {
        return None;
    }

    // Must look like a hostname
    if !is_host_like(host) && !is_safe_host(host) {
        return None;
    }

    Some(ScpMatch {
        host: host.to_string(),
        path: after.to_string(),
    })
}

struct GithubPrefixMatch {
    owner: String,
    repo: String,
}

struct ScpMatch {
    host: String,
    path: String,
}

// ══════════════════════════════════════════════════════════════════════════════
// Tests
// ══════════════════════════════════════════════════════════════════════════════

#[cfg(test)]
mod tests {
    use super::*;

    // ── parse_repository ───────────────────────────────────────────

    #[test]
    fn test_parse_github_shorthand() {
        let ref_ = parse_repository("owner/repo").expect("parse");
        assert!(ref_.is_remote());
        match ref_ {
            RepositoryReference::Remote(r) => {
                assert_eq!(r.base.host, "github.com");
                assert_eq!(r.base.path, "owner/repo");
                assert_eq!(r.base.owner.as_deref(), Some("owner"));
                assert_eq!(r.base.repo, "repo");
                assert_eq!(r.base.label, "owner/repo");
            }
            _ => panic!("expected Remote"),
        }
    }

    #[test]
    fn test_parse_github_prefixed() {
        let ref_ = parse_repository("github:myorg/myrepo").expect("parse");
        match ref_ {
            RepositoryReference::Remote(r) => {
                assert_eq!(r.base.host, "github.com");
                assert_eq!(r.base.path, "myorg/myrepo");
            }
            _ => panic!("expected Remote"),
        }
    }

    #[test]
    fn test_parse_full_https_url() {
        let ref_ = parse_repository("https://github.com/rust-lang/rust").expect("parse");
        match ref_ {
            RepositoryReference::Remote(r) => {
                assert_eq!(r.base.host, "github.com");
                assert_eq!(r.base.path, "rust-lang/rust");
                assert_eq!(r.base.remote, "https://github.com/rust-lang/rust.git");
            }
            _ => panic!("expected Remote"),
        }
    }

    #[test]
    fn test_parse_ssh_scp() {
        let ref_ = parse_repository("git@github.com:user/repo.git").expect("parse");
        match ref_ {
            RepositoryReference::Remote(r) => {
                assert_eq!(r.base.host, "github.com");
                assert_eq!(r.base.repo, "repo");
            }
            _ => panic!("expected Remote"),
        }
    }

    #[test]
    fn test_parse_strips_git_suffix() {
        let ref_ = parse_repository("owner/repo.git").expect("parse");
        match ref_ {
            RepositoryReference::Remote(r) => {
                assert_eq!(r.base.repo, "repo");
                assert_eq!(r.base.path, "owner/repo");
            }
            _ => panic!("expected Remote"),
        }
    }

    #[test]
    fn test_parse_file_url() {
        let ref_ = parse_repository("file:///home/user/local-repo").expect("parse");
        assert!(ref_.is_file());
        match ref_ {
            RepositoryReference::File(f) => {
                assert_eq!(f.host, "file");
                assert_eq!(f.protocol, "file:");
            }
            _ => panic!("expected File"),
        }
    }

    #[test]
    fn test_parse_invalid_returns_none() {
        assert!(parse_repository("").is_none());
        assert!(parse_repository("   ").is_none());
    }

    #[test]
    fn test_parse_strips_git_plus_prefix() {
        let ref_ = parse_repository("git+https://github.com/owner/repo").expect("parse");
        assert!(ref_.is_remote());
    }

    #[test]
    fn test_parse_strips_fragment() {
        let ref_ = parse_repository("owner/repo#v1.0.0").expect("parse");
        assert_eq!(
            match ref_ {
                RepositoryReference::Remote(r) => r.base.path,
                _ => panic!(),
            },
            "owner/repo"
        );
    }

    // ── parse_remote_repository ────────────────────────────────────

    #[test]
    fn test_parse_remote_valid() {
        let r = parse_remote_repository("owner/repo").expect("parse remote");
        assert_eq!(r.base.host, "github.com");
    }

    #[test]
    fn test_parse_remote_invalid() {
        let err = parse_remote_repository("").expect_err("should fail");
        assert!(matches!(err, RepositoryError::InvalidReference { .. }));
    }

    #[test]
    fn test_parse_remote_local_unsupported() {
        let err = parse_remote_repository("file:///local/repo").expect_err("should fail");
        assert!(matches!(err, RepositoryError::UnsupportedLocal { .. }));
    }

    // ── validate_branch ────────────────────────────────────────────

    #[test]
    fn test_validate_branch_valid() {
        assert!(validate_branch("main").is_ok());
        assert!(validate_branch("feature/my-branch_v2.0").is_ok());
        assert!(validate_branch("release/2024.01").is_ok());
    }

    #[test]
    fn test_validate_branch_starts_with_dash() {
        assert!(validate_branch("-bad-branch").is_err());
    }

    #[test]
    fn test_validate_branch_double_dot() {
        assert!(validate_branch("branch..name").is_err());
    }

    #[test]
    fn test_validate_branch_special_chars() {
        assert!(validate_branch("bad branch").is_err());
        assert!(validate_branch("branch!").is_err());
    }

    // ── RepositoryReference helpers ────────────────────────────────

    #[test]
    fn test_cache_identity_remote() {
        let r = parse_repository("github.com/org/repo").expect("parse");
        assert_eq!(r.cache_identity(), "github.com/org/repo");
    }

    #[test]
    fn test_cache_path() {
        let r = parse_repository("github.com/org/repo").expect("parse");
        let path = r.cache_path("/var/cache");
        assert!(path.starts_with("/var/cache"));
        assert!(path.contains("github.com"));
        assert!(path.contains("org"));
        assert!(path.contains("repo"));
    }

    #[test]
    fn test_same_reference() {
        let a = parse_repository("owner/repo").expect("a");
        let b = parse_repository("owner/repo").expect("b");
        assert!(a.same(&b));
    }

    #[test]
    fn test_different_reference() {
        let a = parse_repository("owner1/repo").expect("a");
        let b = parse_repository("owner2/repo").expect("b");
        assert!(!a.same(&b));
    }

    // ── RepositoryCacheResult ──────────────────────────────────────

    #[test]
    fn test_cache_result_serde() {
        let result = RepositoryCacheResult {
            repository: "owner/repo".into(),
            host: "github.com".into(),
            remote: "https://github.com/owner/repo.git".into(),
            local_path: "/var/cache/github.com/owner/repo".into(),
            status: RepositoryCacheStatus::Cloned,
            head: Some("abc123".into()),
            branch: Some("main".into()),
        };
        let json = serde_json::to_string(&result).expect("serialize");
        let parsed: RepositoryCacheResult = serde_json::from_str(&json).expect("deserialize");
        assert_eq!(parsed.status, RepositoryCacheStatus::Cloned);
        assert_eq!(parsed.head.as_deref(), Some("abc123"));
    }

    // ── RepositoryCacheStatus ──────────────────────────────────────

    #[test]
    fn test_cache_status_serde() {
        assert_eq!(
            serde_json::to_string(&RepositoryCacheStatus::Cached).expect("serialize"),
            r#""cached""#
        );
        assert_eq!(
            serde_json::to_string(&RepositoryCacheStatus::Cloned).expect("serialize"),
            r#""cloned""#
        );
        assert_eq!(
            serde_json::to_string(&RepositoryCacheStatus::Refreshed).expect("serialize"),
            r#""refreshed""#
        );
    }

    // ── RepositoryCacheEnsureInput ─────────────────────────────────

    #[test]
    fn test_ensure_input_serde() {
        let ref_ = parse_remote_repository("owner/repo").expect("parse");
        let input = RepositoryCacheEnsureInput {
            reference: ref_,
            refresh: true,
            branch: None,
        };
        let json = serde_json::to_string(&input).expect("serialize");
        assert!(json.contains("refresh"));
    }
}
