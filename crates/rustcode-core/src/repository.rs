//! Repository types — reference parsing, branch validation, cache management.
//!
//! Ported from:
//! - `packages/core/src/repository.ts` — Reference types (BaseReference, RemoteReference,
//!   FileReference), parse/parseRemote/validateBranch, cache helpers, error classes
//! - `packages/core/src/repository-cache.ts` — Result, EnsureInput, cache error classes
//!
//! OpenCode commit: 5d0f86606ac30690f79f0a6a9f41a1f49fe95d0b

use serde::{Deserialize, Serialize};
use std::fs::OpenOptions;
use std::thread;
use std::time::Duration;

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
    InvalidReference { repository: String, message: String },

    /// Local file repositories are not supported for remote operations.
    #[error("local repository not supported '{repository}': {message}")]
    UnsupportedLocal { repository: String, message: String },

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
            segments: segments.clone(),
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
        message:
            "Repository must be a git URL, host/path reference, or GitHub owner/repo shorthand"
                .into(),
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
    let valid = !branch.is_empty()
        && branch
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

impl RepositoryCacheError {
    pub fn is_clone_error(&self) -> bool {
        matches!(self, Self::CloneFailed { .. })
    }

    pub fn is_fetch_error(&self) -> bool {
        matches!(self, Self::FetchFailed { .. })
    }

    pub fn is_lock_error(&self) -> bool {
        matches!(self, Self::LockFailed { .. })
    }
}

// ══════════════════════════════════════════════════════════════════════════════
// RepositoryCacheError — free-standing type guards
// ══════════════════════════════════════════════════════════════════════════════

/// Type guard: returns `true` if the error is a clone-related error.
#[must_use]
pub fn is_error(err: &RepositoryCacheError) -> bool {
    matches!(
        err,
        RepositoryCacheError::CloneFailed { .. }
            | RepositoryCacheError::FetchFailed { .. }
            | RepositoryCacheError::CheckoutFailed { .. }
            | RepositoryCacheError::ResetFailed { .. }
            | RepositoryCacheError::LockFailed { .. }
            | RepositoryCacheError::CacheOperation { .. }
    )
}

/// Returns true if the error is specifically a clone failure.
#[must_use]
pub fn is_clone_error(err: &RepositoryCacheError) -> bool {
    matches!(err, RepositoryCacheError::CloneFailed { .. })
}

/// Returns true if the error is specifically a fetch failure.
#[must_use]
pub fn is_fetch_error(err: &RepositoryCacheError) -> bool {
    matches!(err, RepositoryCacheError::FetchFailed { .. })
}

// ══════════════════════════════════════════════════════════════════════════════
// Cache Lock — process-wide file-based locking for cache operations
// ══════════════════════════════════════════════════════════════════════════════

/// Process-wide file lock for repository cache operations.
///
/// Creates a `.lock` file alongside the cache path using exclusive create
/// (O_CREAT|O_EXCL) for atomic lock acquisition. The lock is automatically
/// released when the `CacheLock` is dropped.
///
/// Ported from: `EffectFlock` in the TypeScript source.
#[derive(Debug)]
pub struct CacheLock {
    lock_path: std::path::PathBuf,
    _file: std::fs::File,
}

impl CacheLock {
    /// Default retry interval for `wait_lock`.
    const DEFAULT_RETRY_MS: u64 = 100;

    /// Default maximum number of retries for `wait_lock`.
    const DEFAULT_MAX_RETRIES: u32 = 30;

    /// Attempt to acquire a non-blocking lock on the given cache path.
    ///
    /// Creates a `.lock` file next to the target path. Returns `Err` with
    /// `RepositoryCacheError::LockFailed` if the lock is already held.
    pub fn try_lock(cache_path: &Path) -> Result<Self, RepositoryCacheError> {
        let lock_path = Self::lock_path_for(cache_path);
        Self::ensure_parent_dir(&lock_path)?;

        let file = OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&lock_path)
            .map_err(|e| RepositoryCacheError::LockFailed {
                local_path: cache_path.display().to_string(),
                message: if e.kind() == std::io::ErrorKind::AlreadyExists {
                    "cache is locked by another process".into()
                } else {
                    format!("failed to create lock file: {e}")
                },
            })?;

        Ok(Self {
            lock_path,
            _file: file,
        })
    }

    /// Attempt to acquire a lock, retrying with exponential backoff.
    ///
    /// Blocks the current thread until the lock is acquired or the retry
    /// limit is reached.
    pub fn wait_lock(cache_path: &Path) -> Result<Self, RepositoryCacheError> {
        Self::wait_lock_with_params(
            cache_path,
            Self::DEFAULT_RETRY_MS,
            Self::DEFAULT_MAX_RETRIES,
        )
    }

    /// Attempt to acquire a lock with configurable retry parameters.
    pub fn wait_lock_with_params(
        cache_path: &Path,
        retry_ms: u64,
        max_retries: u32,
    ) -> Result<Self, RepositoryCacheError> {
        let mut interval = Duration::from_millis(retry_ms);

        for attempt in 0..max_retries {
            match Self::try_lock(cache_path) {
                Ok(lock) => return Ok(lock),
                Err(_) if attempt + 1 < max_retries => {
                    thread::sleep(interval);
                    interval = (interval * 2).min(Duration::from_secs(5));
                }
                Err(e) => return Err(e),
            }
        }

        Err(RepositoryCacheError::LockFailed {
            local_path: cache_path.display().to_string(),
            message: format!("failed to acquire lock after {max_retries} retries"),
        })
    }

    /// Explicitly release the lock and remove the lock file.
    ///
    /// Called automatically on drop, but can be invoked explicitly for
    /// early release.
    pub fn unlock(self) -> Result<(), RepositoryCacheError> {
        drop(self);
        Ok(())
    }

    /// Return the filesystem path of the `.lock` file for a given cache path.
    #[must_use]
    pub fn lock_path_for(cache_path: &Path) -> PathBuf {
        let mut p = cache_path.as_os_str().to_owned();
        p.push(".lock");
        PathBuf::from(p)
    }

    fn ensure_parent_dir(lock_path: &Path) -> Result<(), RepositoryCacheError> {
        if let Some(parent) = lock_path.parent() {
            std::fs::create_dir_all(parent).map_err(|e| RepositoryCacheError::CacheOperation {
                operation: "mkdir".into(),
                path: parent.display().to_string(),
                message: e.to_string(),
            })?;
        }
        Ok(())
    }
}

impl Drop for CacheLock {
    fn drop(&mut self) {
        let _ = std::fs::remove_file(&self.lock_path);
    }
}

/// Resolve the reset target based on the current branch of a local repository.
///
/// Returns `origin/HEAD` for detached HEAD or empty branch, otherwise `origin/{branch}`.
fn resolve_reset_target(local_path: &Path) -> Result<String, RepositoryCacheError> {
    let branch_output = std::process::Command::new("git")
        .args(["rev-parse", "--abbrev-ref", "HEAD"])
        .current_dir(local_path)
        .output()
        .map_err(|e| RepositoryCacheError::CacheOperation {
            operation: "resolve_reset_target".into(),
            path: local_path.display().to_string(),
            message: e.to_string(),
        })?;

    let branch = String::from_utf8_lossy(&branch_output.stdout)
        .trim()
        .to_string();
    if branch == "HEAD" || branch.is_empty() {
        Ok("origin/HEAD".to_string())
    } else {
        Ok(format!("origin/{branch}"))
    }
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
    !host.is_empty()
        && !host.starts_with('-')
        && !host.contains(|c: char| c.is_whitespace() || c == '\\')
}

/// Validate a path segment: not `.` or `..`, no `:`, no whitespace/backslash.
fn is_safe_segment(segment: &str) -> bool {
    segment != "."
        && segment != ".."
        && !segment.contains(':')
        && !segment.contains(|c: char| c.is_whitespace() || c == '\\')
}

/// Generate the GitHub remote URL for a given pathname.
fn github_remote_url(_host: &str, pathname: &str) -> String {
    if let Ok(base_url) = std::env::var("OPENCODE_REPO_CLONE_GITHUB_BASE_URL") {
        let base = base_url.trim_end_matches('/');
        format!("{base}/{pathname}.git")
    } else {
        format!("https://github.com/{pathname}.git")
    }
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
            repo: segments.last().cloned().unwrap_or_else(|| "unknown".into()),
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
// Repository Service — clone, fetch, resolve
// ══════════════════════════════════════════════════════════════════════════════

use std::path::{Path, PathBuf};

/// Repository service — handles cloning, fetching, and resolving references.
///
/// # Source
/// Ported from `packages/core/src/repository.ts` and `repository-cache.ts`.
#[derive(Debug, Clone)]
pub struct RepositoryService {
    /// Root directory for caching cloned repositories.
    cache_root: PathBuf,
}

impl RepositoryService {
    /// Create a new RepositoryService with a cache root directory.
    pub fn new(cache_root: impl Into<PathBuf>) -> Self {
        Self {
            cache_root: cache_root.into(),
        }
    }

    /// Get the cache root directory.
    pub fn cache_root(&self) -> &Path {
        &self.cache_root
    }

    /// Compute the local cache path for a repository reference.
    ///
    /// # Source
    /// Ported from `packages/core/src/repository.ts` `cachePath()` (lines 121–123).
    pub fn cache_path(&self, reference: &RepositoryReference) -> PathBuf {
        let path_str = reference.cache_path(&self.cache_root.display().to_string());
        PathBuf::from(path_str)
    }

    /// Clone a repository (shallow, single-branch) into the cache.
    ///
    /// Acquires a process-wide lock on the cache path before cloning.
    ///
    /// # Source
    /// Ported from `packages/core/src/repository-cache.ts` `ensure()`.
    pub async fn clone(
        &self,
        reference: &RemoteReference,
        branch: Option<&str>,
    ) -> Result<RepositoryCacheResult, RepositoryCacheError> {
        let local_path = self.cache_path(&RepositoryReference::Remote(reference.clone()));
        let _lock = CacheLock::wait_lock(&local_path)?;
        self.clone_inner(reference, branch, &local_path).await
    }

    async fn clone_inner(
        &self,
        reference: &RemoteReference,
        branch: Option<&str>,
        local_path: &Path,
    ) -> Result<RepositoryCacheResult, RepositoryCacheError> {
        let remote_url = &reference.base.remote;

        // Ensure parent directory exists
        if let Some(parent) = local_path.parent() {
            std::fs::create_dir_all(parent).map_err(|e| RepositoryCacheError::CacheOperation {
                operation: "mkdir".into(),
                path: parent.display().to_string(),
                message: e.to_string(),
            })?;
        }

        // Check if already cloned
        if local_path.join(".git").exists() {
            // Already exists — just fetch if requested
            let head = self.resolve_head(local_path)?;
            let current_branch = self.resolve_branch(local_path)?;
            return Ok(RepositoryCacheResult {
                repository: reference.base.label.clone(),
                host: reference.base.host.clone(),
                remote: remote_url.clone(),
                local_path: local_path.display().to_string(),
                status: RepositoryCacheStatus::Cached,
                head: Some(head),
                branch: current_branch,
            });
        }

        // Build git clone command
        let mut cmd = std::process::Command::new("git");
        cmd.args(["clone", "--depth=1", "--single-branch"]);
        if let Some(b) = branch {
            cmd.args(["--branch", b]);
        }
        cmd.arg(remote_url).arg(local_path.display().to_string());

        let output = cmd
            .output()
            .map_err(|e| RepositoryCacheError::CloneFailed {
                repository: reference.base.label.clone(),
                message: format!("failed to run git clone: {e}"),
            })?;

        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr);
            return Err(RepositoryCacheError::CloneFailed {
                repository: reference.base.label.clone(),
                message: format!("git clone failed: {stderr}"),
            });
        }

        let head = self.resolve_head(local_path)?;
        let current_branch = self.resolve_branch(local_path)?;

        Ok(RepositoryCacheResult {
            repository: reference.base.label.clone(),
            host: reference.base.host.clone(),
            remote: remote_url.clone(),
            local_path: local_path.display().to_string(),
            status: RepositoryCacheStatus::Cloned,
            head: Some(head),
            branch: current_branch,
        })
    }

    /// Fetch updates for an already-cloned repository.
    ///
    /// Acquires a process-wide lock on the cache path before fetching.
    ///
    /// # Source
    /// Ported from `packages/core/src/repository-cache.ts` `refresh` logic.
    pub async fn fetch(
        &self,
        reference: &RemoteReference,
    ) -> Result<RepositoryCacheResult, RepositoryCacheError> {
        let local_path = self.cache_path(&RepositoryReference::Remote(reference.clone()));
        let _lock = CacheLock::wait_lock(&local_path)?;
        self.fetch_inner(reference, &local_path).await
    }

    async fn fetch_inner(
        &self,
        reference: &RemoteReference,
        local_path: &Path,
    ) -> Result<RepositoryCacheResult, RepositoryCacheError> {
        if !local_path.join(".git").exists() {
            // Not cloned yet — clone it first (lock already held)
            return self.clone_inner(reference, None, local_path).await;
        }

        // Run git fetch
        let output = std::process::Command::new("git")
            .args(["fetch", "--depth=1", "origin"])
            .current_dir(local_path)
            .output()
            .map_err(|e| RepositoryCacheError::FetchFailed {
                repository: reference.base.label.clone(),
                message: format!("failed to run git fetch: {e}"),
            })?;

        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr);
            return Err(RepositoryCacheError::FetchFailed {
                repository: reference.base.label.clone(),
                message: format!("git fetch failed: {stderr}"),
            });
        }

        let reset_target = resolve_reset_target(local_path)?;
        let _ = std::process::Command::new("git")
            .args(["reset", "--hard", &reset_target])
            .current_dir(local_path)
            .output();

        let head = self.resolve_head(local_path)?;
        let current_branch = self.resolve_branch(local_path)?;

        Ok(RepositoryCacheResult {
            repository: reference.base.label.clone(),
            host: reference.base.host.clone(),
            remote: reference.base.remote.clone(),
            local_path: local_path.display().to_string(),
            status: RepositoryCacheStatus::Refreshed,
            head: Some(head),
            branch: current_branch,
        })
    }

    /// Resolve a reference to a commit hash (HEAD).
    ///
    /// # Source
    /// Ported from `packages/core/src/repository-cache.ts` HEAD resolution.
    pub fn resolve(&self, local_path: &Path) -> Result<String, RepositoryCacheError> {
        self.resolve_head(local_path)
            .map_err(|e| RepositoryCacheError::CacheOperation {
                operation: "resolve".into(),
                path: local_path.display().to_string(),
                message: e.to_string(),
            })
    }

    /// Resolve the current branch name from a repository.
    pub fn resolve_branch(
        &self,
        local_path: &Path,
    ) -> Result<Option<String>, RepositoryCacheError> {
        let output = std::process::Command::new("git")
            .args(["rev-parse", "--abbrev-ref", "HEAD"])
            .current_dir(local_path)
            .output()
            .map_err(|e| RepositoryCacheError::CacheOperation {
                operation: "branch".into(),
                path: local_path.display().to_string(),
                message: e.to_string(),
            })?;

        let branch = String::from_utf8_lossy(&output.stdout).trim().to_string();
        if branch == "HEAD" || branch.is_empty() {
            Ok(None)
        } else {
            Ok(Some(branch))
        }
    }

    /// Ensure a repository is cached and up-to-date (clone if missing, fetch if present).
    ///
    /// Acquires a process-wide lock on the cache path for the duration of the operation.
    ///
    /// # Source
    /// Ported from `packages/core/src/repository-cache.ts` `ensure()`.
    pub async fn ensure(
        &self,
        input: &RepositoryCacheEnsureInput,
    ) -> Result<RepositoryCacheResult, RepositoryCacheError> {
        // Validate branch if provided
        if let Some(ref branch) = input.branch {
            validate_branch(branch).map_err(|e| RepositoryCacheError::InvalidBranch {
                branch: branch.clone(),
                message: e.to_string(),
            })?;
        }

        let local_path = self.cache_path(&RepositoryReference::Remote(input.reference.clone()));
        let _lock = CacheLock::wait_lock(&local_path)?;
        let needs_clone = !local_path.join(".git").exists();

        if needs_clone {
            self.clone_inner(&input.reference, input.branch.as_deref(), &local_path)
                .await
        } else if input.refresh {
            self.fetch_inner(&input.reference, &local_path).await
        } else {
            let head = self.resolve_head(&local_path)?;
            let branch = self.resolve_branch(&local_path)?;
            Ok(RepositoryCacheResult {
                repository: input.reference.base.label.clone(),
                host: input.reference.base.host.clone(),
                remote: input.reference.base.remote.clone(),
                local_path: local_path.display().to_string(),
                status: RepositoryCacheStatus::Cached,
                head: Some(head),
                branch,
            })
        }
    }

    // ── Internal helpers ─────────────────────────────────────────────

    /// Resolve HEAD commit hash from a local repository.
    fn resolve_head(&self, local_path: &Path) -> Result<String, RepositoryCacheError> {
        let output = std::process::Command::new("git")
            .args(["rev-parse", "HEAD"])
            .current_dir(local_path)
            .output()
            .map_err(|e| RepositoryCacheError::CacheOperation {
                operation: "rev-parse".into(),
                path: local_path.display().to_string(),
                message: e.to_string(),
            })?;

        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr);
            return Err(RepositoryCacheError::CacheOperation {
                operation: "rev-parse".into(),
                path: local_path.display().to_string(),
                message: format!("git rev-parse failed: {stderr}"),
            });
        }

        Ok(String::from_utf8_lossy(&output.stdout).trim().to_string())
    }
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

    // ── Extended URL parsing tests ────────────────────────────────────

    #[test]
    fn test_parse_https_gitlab() {
        let ref_ = parse_repository("https://gitlab.com/org/project").expect("parse");
        match ref_ {
            RepositoryReference::Remote(r) => {
                assert_eq!(r.base.host, "gitlab.com");
                assert_eq!(r.base.path, "org/project");
                assert_eq!(r.base.owner.as_deref(), Some("org"));
            }
            _ => panic!("expected Remote"),
        }
    }

    #[test]
    fn test_parse_https_with_dotgit() {
        let ref_ = parse_repository("https://github.com/user/repo.git").expect("parse");
        match ref_ {
            RepositoryReference::Remote(r) => {
                assert_eq!(r.base.repo, "repo");
                assert_eq!(r.base.path, "user/repo");
            }
            _ => panic!("expected Remote"),
        }
    }

    #[test]
    fn test_parse_ssh_with_user() {
        let ref_ = parse_repository("git@gitlab.com:group/subgroup/project.git").expect("parse");
        match ref_ {
            RepositoryReference::Remote(r) => {
                assert_eq!(r.base.host, "gitlab.com");
                assert_eq!(r.base.repo, "project");
                assert!(r.base.path.contains("group/subgroup/project"));
            }
            _ => panic!("expected Remote"),
        }
    }

    #[test]
    fn test_parse_host_path_style() {
        let ref_ = parse_repository("github.com/owner/repo").expect("parse");
        match ref_ {
            RepositoryReference::Remote(r) => {
                assert_eq!(r.base.host, "github.com");
                assert_eq!(r.base.path, "owner/repo");
            }
            _ => panic!("expected Remote"),
        }
    }

    #[test]
    fn test_parse_bitbucket_https() {
        let ref_ = parse_repository("https://bitbucket.org/team/repo.git").expect("parse");
        match ref_ {
            RepositoryReference::Remote(r) => {
                assert_eq!(r.base.host, "bitbucket.org");
                assert_eq!(r.base.repo, "repo");
            }
            _ => panic!("expected Remote"),
        }
    }

    #[test]
    fn test_parse_single_owner_no_repo() {
        // Single segment should not parse as GitHub shorthand
        let result = parse_repository("justowner");
        assert!(result.is_none());
    }

    #[test]
    fn test_parse_empty_after_trim() {
        assert!(parse_repository("   ").is_none());
        assert!(parse_repository("\t\n").is_none());
    }

    #[test]
    fn test_parse_strips_git_plus_ssh() {
        let ref_ = parse_repository("git+ssh://git@github.com/owner/repo.git").expect("parse");
        assert!(ref_.is_remote());
    }

    #[test]
    fn test_parse_trailing_slash() {
        let ref_ = parse_repository("owner/repo/").expect("parse");
        match ref_ {
            RepositoryReference::Remote(r) => {
                assert_eq!(r.base.path, "owner/repo");
            }
            _ => panic!("expected Remote"),
        }
    }

    #[test]
    fn test_parse_three_segment_path() {
        let ref_ = parse_repository("github.com/org/repo/sub").expect("parse");
        match ref_ {
            RepositoryReference::Remote(r) => {
                assert_eq!(r.base.host, "github.com");
                assert_eq!(r.base.owner, None); // Only 2-segment gets owner
                assert_eq!(r.base.repo, "sub");
            }
            _ => panic!("expected Remote"),
        }
    }

    #[test]
    fn test_parse_file_url_windows_path() {
        let ref_ = parse_repository("file:///C:/Users/dev/repo").expect("parse");
        assert!(ref_.is_file());
    }

    #[test]
    fn test_repository_error_invalid_reference_display() {
        let err = RepositoryError::InvalidReference {
            repository: "bad".into(),
            message: "nope".into(),
        };
        assert!(err.to_string().contains("bad"));
        assert!(err.to_string().contains("nope"));
    }

    #[test]
    fn test_repository_error_invalid_branch_display() {
        let err = RepositoryError::InvalidBranch {
            branch: "-bad".into(),
            message: "no dashes".into(),
        };
        assert!(err.to_string().contains("-bad"));
    }

    #[test]
    fn test_repository_cache_error_clone_failed_display() {
        let err = RepositoryCacheError::CloneFailed {
            repository: "x/y".into(),
            message: "boom".into(),
        };
        assert!(err.to_string().contains("x/y"));
        assert!(err.to_string().contains("boom"));
    }

    #[test]
    fn test_repository_cache_error_fetch_failed_display() {
        let err = RepositoryCacheError::FetchFailed {
            repository: "a/b".into(),
            message: "network error".into(),
        };
        assert!(err.to_string().contains("a/b"));
    }

    // ── Free-standing type guards ─────────────────────────────────

    #[test]
    fn test_is_error_clone() {
        let err = RepositoryCacheError::CloneFailed {
            repository: "x/y".into(),
            message: "boom".into(),
        };
        assert!(is_error(&err));
    }

    #[test]
    fn test_is_error_fetch() {
        let err = RepositoryCacheError::FetchFailed {
            repository: "x/y".into(),
            message: "boom".into(),
        };
        assert!(is_error(&err));
    }

    #[test]
    fn test_is_error_checkout() {
        let err = RepositoryCacheError::CheckoutFailed {
            repository: "x/y".into(),
            branch: "main".into(),
            message: "boom".into(),
        };
        assert!(is_error(&err));
    }

    #[test]
    fn test_is_error_reset() {
        let err = RepositoryCacheError::ResetFailed {
            repository: "x/y".into(),
            message: "boom".into(),
        };
        assert!(is_error(&err));
    }

    #[test]
    fn test_is_error_lock() {
        let err = RepositoryCacheError::LockFailed {
            local_path: "/tmp/repo".into(),
            message: "held".into(),
        };
        assert!(is_error(&err));
    }

    #[test]
    fn test_is_error_cache_operation() {
        let err = RepositoryCacheError::CacheOperation {
            operation: "mkdir".into(),
            path: "/tmp/repo".into(),
            message: "perm denied".into(),
        };
        assert!(is_error(&err));
    }

    #[test]
    fn test_is_error_invalid_repository() {
        let err = RepositoryCacheError::InvalidRepository {
            repository: "bad".into(),
            message: "nope".into(),
        };
        assert!(!is_error(&err));
    }

    #[test]
    fn test_is_error_invalid_branch() {
        let err = RepositoryCacheError::InvalidBranch {
            branch: "-bad".into(),
            message: "nope".into(),
        };
        assert!(!is_error(&err));
    }

    #[test]
    fn test_free_is_clone_error() {
        let clone_err = RepositoryCacheError::CloneFailed {
            repository: "x/y".into(),
            message: "boom".into(),
        };
        assert!(is_clone_error(&clone_err));

        let fetch_err = RepositoryCacheError::FetchFailed {
            repository: "x/y".into(),
            message: "net".into(),
        };
        assert!(!is_clone_error(&fetch_err));
    }

    #[test]
    fn test_free_is_fetch_error() {
        let fetch_err = RepositoryCacheError::FetchFailed {
            repository: "x/y".into(),
            message: "net".into(),
        };
        assert!(is_fetch_error(&fetch_err));

        let clone_err = RepositoryCacheError::CloneFailed {
            repository: "x/y".into(),
            message: "boom".into(),
        };
        assert!(!is_fetch_error(&clone_err));
    }

    // ── RepositoryService tests ───────────────────────────────────────

    fn setup_repo_cache() -> (tempfile::TempDir, RepositoryService) {
        let dir = tempfile::tempdir().expect("create temp dir");
        let svc = RepositoryService::new(dir.path().to_path_buf());
        (dir, svc)
    }

    #[test]
    fn test_cache_path_remote() {
        let (_dir, svc) = setup_repo_cache();
        let ref_ = parse_repository("github.com/owner/repo").expect("parse");
        let path = svc.cache_path(&ref_);
        assert!(path.starts_with(svc.cache_root()));
        assert!(path.to_string_lossy().contains("github.com"));
        assert!(path.to_string_lossy().contains("owner"));
        assert!(path.to_string_lossy().contains("repo"));
    }

    #[test]
    fn test_cache_path_file() {
        let (_dir, svc) = setup_repo_cache();
        let ref_ = parse_repository("file:///home/user/local-repo").expect("parse");
        let path = svc.cache_path(&ref_);
        assert!(path.starts_with(svc.cache_root()));
        assert!(path.to_string_lossy().contains("file"));
    }

    #[test]
    fn test_resolve_branch_detached_head() {
        let dir = tempfile::tempdir().expect("create temp dir");
        let repo_path = dir.path().join("detached-repo");
        std::fs::create_dir_all(&repo_path).expect("create repo dir");

        // Create a repo and make a commit
        std::process::Command::new("git")
            .args(["init", "--quiet"])
            .current_dir(&repo_path)
            .output()
            .expect("git init");

        std::fs::write(repo_path.join("test.txt"), "hello").unwrap();
        std::process::Command::new("git")
            .args(["add", "."])
            .current_dir(&repo_path)
            .output()
            .expect("git add");
        std::process::Command::new("git")
            .args(["commit", "-m", "init", "--quiet"])
            .current_dir(&repo_path)
            .output()
            .expect("git commit");

        let svc = RepositoryService::new(dir.path().to_path_buf());
        let branch = svc.resolve_branch(&repo_path).expect("resolve branch");
        // Should resolve to a branch name (not "HEAD")
        assert!(branch.is_some());
    }

    #[test]
    fn test_resolve_head_returns_hash() {
        let dir = tempfile::tempdir().expect("create temp dir");
        let repo_path = dir.path().join("hash-repo");
        std::fs::create_dir_all(&repo_path).expect("create repo dir");

        std::process::Command::new("git")
            .args(["init", "--quiet"])
            .current_dir(&repo_path)
            .output()
            .expect("git init");

        std::fs::write(repo_path.join("file.txt"), "data").unwrap();
        std::process::Command::new("git")
            .args(["add", "."])
            .current_dir(&repo_path)
            .output()
            .expect("git add");
        std::process::Command::new("git")
            .args(["commit", "-m", "init", "--quiet"])
            .current_dir(&repo_path)
            .output()
            .expect("git commit");

        let svc = RepositoryService::new(dir.path().to_path_buf());
        let hash = svc.resolve(&repo_path).expect("resolve HEAD");
        // Should be a 40-char hex string
        assert_eq!(hash.len(), 40);
        assert!(hash.chars().all(|c| c.is_ascii_hexdigit()));
    }

    #[test]
    fn test_validate_branch_more_cases() {
        // Valid cases
        assert!(validate_branch("main").is_ok());
        assert!(validate_branch("feature/PROJ-123").is_ok());
        assert!(validate_branch("release/2024.01").is_ok());
        assert!(validate_branch("fix/bug_123-v2_backport").is_ok());
        assert!(validate_branch("1.0.0").is_ok());

        // Invalid cases
        assert!(validate_branch("").is_err()); // Empty is not a valid branch
        assert!(validate_branch("-starts-with-dash").is_err());
        assert!(validate_branch("has space").is_err());
        assert!(validate_branch("has..dots").is_err());
        assert!(validate_branch("has@at").is_err());
    }

    #[test]
    fn test_validate_branch_empty_string_rejected() {
        assert!(
            validate_branch("").is_err(),
            "Empty string should not be a valid branch name"
        );
    }

    #[test]
    fn test_parse_remote_repository_via_ssh_url() {
        let r = parse_remote_repository("ssh://git@github.com/org/repo.git").expect("parse ssh");
        assert_eq!(r.base.host, "github.com");
        assert_eq!(r.base.repo, "repo");
    }

    #[test]
    fn test_reference_is_remote_and_is_file() {
        let remote = parse_repository("owner/repo").expect("parse");
        assert!(remote.is_remote());
        assert!(!remote.is_file());

        let file = parse_repository("file:///local/repo").expect("parse");
        assert!(file.is_file());
        assert!(!file.is_remote());
    }

    #[test]
    fn test_cache_identity_file() {
        let ref_ = parse_repository("file:///home/dev/project").expect("parse");
        // File references have host="file"
        assert!(ref_.cache_identity().starts_with("file/"));
    }

    // ── CacheLock tests ──────────────────────────────────────────────

    #[test]
    fn test_cache_lock_acquisition() {
        let dir = tempfile::tempdir().expect("create temp dir");
        let cache_path = dir.path().join("some-repo");

        let lock = CacheLock::try_lock(&cache_path).expect("should acquire lock");
        assert!(CacheLock::lock_path_for(&cache_path).exists());
        drop(lock);
        assert!(!CacheLock::lock_path_for(&cache_path).exists());
    }

    #[test]
    fn test_cache_lock_contention() {
        let dir = tempfile::tempdir().expect("create temp dir");
        let cache_path = dir.path().join("contended-repo");

        let _lock1 = CacheLock::try_lock(&cache_path).expect("first lock");

        // Second lock attempt on the same path should fail
        let result = CacheLock::try_lock(&cache_path);
        assert!(result.is_err());
        match result.unwrap_err() {
            RepositoryCacheError::LockFailed {
                local_path,
                message,
            } => {
                assert!(local_path.contains("contended-repo"));
                assert!(message.contains("locked"));
            }
            other => panic!("expected LockFailed, got {other:?}"),
        }
    }

    #[test]
    fn test_cache_lock_contention_via_thread() {
        let dir = tempfile::tempdir().expect("create temp dir");
        let cache_path = dir.path().join("threaded-repo");
        let cache_path_clone = cache_path.clone();

        // Hold the lock in the main thread
        let _lock1 = CacheLock::try_lock(&cache_path).expect("main thread lock");

        // Spawn a thread that tries to acquire the same lock — it should fail quickly
        let handle =
            std::thread::spawn(move || CacheLock::wait_lock_with_params(&cache_path_clone, 10, 3));

        let result = handle.join().expect("thread panicked");
        assert!(result.is_err(), "should fail to acquire contended lock");
    }

    #[test]
    fn test_cache_lock_release_allows_reacquire() {
        let dir = tempfile::tempdir().expect("create temp dir");
        let cache_path = dir.path().join("reacquire-repo");

        {
            let _lock = CacheLock::try_lock(&cache_path).expect("first lock");
        }
        // Lock should be released after drop
        let lock2 = CacheLock::try_lock(&cache_path).expect("reacquire after drop");
        drop(lock2);
    }

    #[test]
    fn test_cache_lock_wait_lock_succeeds_after_release() {
        let dir = tempfile::tempdir().expect("create temp dir");
        let cache_path = dir.path().join("wait-repo");

        {
            let _lock = CacheLock::try_lock(&cache_path).expect("hold lock");
        }

        let lock = CacheLock::wait_lock(&cache_path).expect("should succeed after release");
        drop(lock);
    }

    #[test]
    fn test_cache_lock_explicit_unlock() {
        let dir = tempfile::tempdir().expect("create temp dir");
        let cache_path = dir.path().join("explicit-repo");

        let lock = CacheLock::try_lock(&cache_path).expect("lock");
        let lock_path = CacheLock::lock_path_for(&cache_path);
        assert!(lock_path.exists());

        lock.unlock().expect("explicit unlock");
        assert!(!lock_path.exists());
    }

    #[test]
    fn test_cache_lock_path_convention() {
        let cache_path = PathBuf::from("/tmp/cache/github.com/owner/repo");
        let lock_path = CacheLock::lock_path_for(&cache_path);
        assert_eq!(
            lock_path,
            PathBuf::from("/tmp/cache/github.com/owner/repo.lock")
        );
    }

    // ── github_remote_url env var override ──────────────────────────

    #[test]
    fn test_github_remote_url_default() {
        let url = github_remote_url("github.com", "owner/repo");
        assert_eq!(url, "https://github.com/owner/repo.git");
    }

    #[test]
    fn test_github_remote_url_env_override() {
        std::env::set_var(
            "OPENCODE_REPO_CLONE_GITHUB_BASE_URL",
            "https://my-ghe.internal.com/github",
        );
        let url = github_remote_url("github.com", "owner/repo");
        assert_eq!(url, "https://my-ghe.internal.com/github/owner/repo.git");
        std::env::remove_var("OPENCODE_REPO_CLONE_GITHUB_BASE_URL");
    }

    #[test]
    fn test_github_remote_url_env_trailing_slash() {
        std::env::set_var(
            "OPENCODE_REPO_CLONE_GITHUB_BASE_URL",
            "https://mirror.example.com/",
        );
        let url = github_remote_url("github.com", "org/project");
        assert_eq!(url, "https://mirror.example.com/org/project.git");
        std::env::remove_var("OPENCODE_REPO_CLONE_GITHUB_BASE_URL");
    }

    // ── resolve_reset_target ────────────────────────────────────────

    #[test]
    fn test_resolve_reset_target_on_branch() {
        let dir = tempfile::tempdir().expect("create temp dir");
        let repo_path = dir.path().join("branch-repo");
        std::fs::create_dir_all(&repo_path).expect("create repo dir");

        std::process::Command::new("git")
            .args(["init", "--quiet", "-b", "main"])
            .current_dir(&repo_path)
            .output()
            .expect("git init");
        std::fs::write(repo_path.join("file.txt"), "hi").unwrap();
        std::process::Command::new("git")
            .args(["add", "."])
            .current_dir(&repo_path)
            .output()
            .expect("git add");
        std::process::Command::new("git")
            .args(["commit", "-m", "init", "--quiet"])
            .current_dir(&repo_path)
            .output()
            .expect("git commit");

        let target = resolve_reset_target(&repo_path).expect("resolve reset target");
        assert_eq!(target, "origin/main");
    }

    #[test]
    fn test_resolve_reset_target_detached_head() {
        let dir = tempfile::tempdir().expect("create temp dir");
        let repo_path = dir.path().join("detached-repo");
        std::fs::create_dir_all(&repo_path).expect("create repo dir");

        std::process::Command::new("git")
            .args(["init", "--quiet"])
            .current_dir(&repo_path)
            .output()
            .expect("git init");
        std::fs::write(repo_path.join("file.txt"), "hi").unwrap();
        std::process::Command::new("git")
            .args(["add", "."])
            .current_dir(&repo_path)
            .output()
            .expect("git add");
        std::process::Command::new("git")
            .args(["commit", "-m", "init", "--quiet"])
            .current_dir(&repo_path)
            .output()
            .expect("git commit");

        // Detach HEAD
        let output = std::process::Command::new("git")
            .args(["rev-parse", "HEAD"])
            .current_dir(&repo_path)
            .output()
            .expect("get HEAD");
        let head = String::from_utf8_lossy(&output.stdout).trim().to_string();
        std::process::Command::new("git")
            .args(["checkout", "--detach", &head])
            .current_dir(&repo_path)
            .output()
            .expect("detach HEAD");

        let target = resolve_reset_target(&repo_path).expect("resolve reset target");
        assert_eq!(target, "origin/HEAD");
    }

    // ── RepositoryCacheError type guards ────────────────────────────

    #[test]
    fn test_error_type_guards() {
        let clone_err = RepositoryCacheError::CloneFailed {
            repository: "x/y".into(),
            message: "boom".into(),
        };
        assert!(clone_err.is_clone_error());
        assert!(!clone_err.is_fetch_error());
        assert!(!clone_err.is_lock_error());

        let fetch_err = RepositoryCacheError::FetchFailed {
            repository: "a/b".into(),
            message: "net".into(),
        };
        assert!(!fetch_err.is_clone_error());
        assert!(fetch_err.is_fetch_error());
        assert!(!fetch_err.is_lock_error());

        let lock_err = RepositoryCacheError::LockFailed {
            local_path: "/tmp/repo".into(),
            message: "held".into(),
        };
        assert!(!lock_err.is_clone_error());
        assert!(!lock_err.is_fetch_error());
        assert!(lock_err.is_lock_error());

        let other_err = RepositoryCacheError::InvalidRepository {
            repository: "bad".into(),
            message: "nope".into(),
        };
        assert!(!other_err.is_clone_error());
        assert!(!other_err.is_fetch_error());
        assert!(!other_err.is_lock_error());
    }
}
