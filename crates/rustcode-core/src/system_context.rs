//! System context for composing typed context sources into LLM system prompts.
//!
//! Ported from: `packages/core/src/system-context/index.ts`
//!
//! Models privileged system context as independently refreshable typed sources.
//! A [`SystemContextSource`] describes how to observe, compare, and render one
//! value. [`SystemContext`] composes uniformly with contexts built from other
//! sources. Interpreters observe the composed context, then produce a durable
//! [`Snapshot`] alongside the exact model-visible baseline or update text.
//!
//! `load` returning `Err` means the source could not be observed temporarily.
//! This differs from removing a source: refresh preserves the admitted snapshot,
//! and replacement waits rather than silently constructing an incomplete baseline.

use std::collections::{HashMap, HashSet};
use std::fmt;
use std::sync::{Arc, OnceLock};

use regex::Regex;
use serde::{Deserialize, Serialize};

// ── Key ──────────────────────────────────────────────────────────────

/// Stable namespaced identity for one independently refreshable context source.
///
/// Must match `^[a-z0-9][a-z0-9._-]*/[a-z0-9][a-z0-9._/-]*$`.
///
/// # Source
/// Ported from `packages/core/src/system-context/index.ts` lines 22–24 (`Key`).
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct SystemContextKey(String);

impl SystemContextKey {
    /// Create a new validated key.
    ///
    /// Returns `Err([SystemContextError::InvalidKey])` if the string doesn't
    /// match the required pattern.
    pub fn new(key: impl Into<String>) -> Result<Self, SystemContextError> {
        let key = key.into();
        if key_regex().is_match(&key) {
            Ok(Self(key))
        } else {
            Err(SystemContextError::InvalidKey(key))
        }
    }

    /// The key as a string slice.
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl fmt::Display for SystemContextKey {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}

impl From<SystemContextKey> for String {
    fn from(key: SystemContextKey) -> String {
        key.0
    }
}

fn key_regex() -> &'static Regex {
    static RE: OnceLock<Regex> = OnceLock::new();
    RE.get_or_init(|| {
        Regex::new(r"^[a-z0-9][a-z0-9._-]*/[a-z0-9][a-z0-9._/-]*$")
            .expect("invalid system context key regex")
    })
}

// ── Source snapshot ───────────────────────────────────────────────────

/// Durable comparison state for one admitted source.
///
/// # Source
/// Ported from `packages/core/src/system-context/index.ts` lines 49–53 (`SourceSnapshot`).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SourceSnapshot {
    /// The serialized value of the source.
    pub json: serde_json::Value,
    /// Text to display when the source is removed from context.
    pub removal: Option<String>,
}

// ── Snapshot ─────────────────────────────────────────────────────────

/// Durable structured comparison state for one active context generation.
///
/// # Source
/// Ported from `packages/core/src/system-context/index.ts` lines 56–57 (`Snapshot`).
pub type Snapshot = HashMap<SystemContextKey, SourceSnapshot>;

// ── Generation ───────────────────────────────────────────────────────

/// Immutable baseline and durable snapshot for a new context generation.
///
/// # Source
/// Ported from `packages/core/src/system-context/index.ts` lines 59–62 (`Generation`).
#[derive(Debug, Clone)]
pub struct Generation {
    /// The full baseline text (all sources joined).
    pub baseline: String,
    /// The snapshot state for comparison.
    pub snapshot: Snapshot,
}

// ── Source trait ──────────────────────────────────────────────────────

/// Defines one typed source before its value type is hidden by [`SystemContext`].
///
/// # Source
/// Ported from `packages/core/src/system-context/index.ts` lines 32–39 (`Source<A>`).
pub trait SystemContextSource: Send + Sync {
    /// Stable namespaced identity for this source.
    fn key(&self) -> &SystemContextKey;

    /// Load the current value.
    ///
    /// Returns `Err` if the source is temporarily unavailable.
    fn load(&self) -> Result<serde_json::Value, Box<dyn std::error::Error + Send + Sync>>;

    /// Render the initial baseline text from a loaded value.
    fn baseline(&self, data: &serde_json::Value) -> String;

    /// Render update text from the current value.
    fn update(&self, data: &serde_json::Value) -> String;

    /// Render removal text when the source is dropped from context.
    fn removed(&self) -> String;
}

// ── Reconcile result ─────────────────────────────────────────────────

/// Result of reconciling current source values with a previous snapshot.
///
/// # Source
/// Ported from `packages/core/src/system-context/index.ts` lines 64–80.
#[derive(Debug, Clone)]
pub enum ReconcileResult {
    /// No changes detected.
    Unchanged,
    /// One or more sources changed — incremental update text available.
    Updated {
        /// Combined update text for all changed sources.
        text: String,
        /// The new snapshot after applying updates.
        snapshot: Snapshot,
    },
    /// A complete replacement generation is ready.
    ReplacementReady {
        /// The new generation to replace the current one.
        generation: Generation,
    },
    /// Replacement blocked because an unavailable source has a previous snapshot.
    ReplacementBlocked,
}

// ── UpdatedItem ──────────────────────────────────────────────────────

/// A single updated source item.
///
/// # Source
/// Ported from `packages/core/src/system-context/index.ts` (implicit in `Updated`).
#[derive(Debug, Clone)]
pub struct UpdatedItem {
    /// The source key that changed.
    pub key: SystemContextKey,
    /// The rendered update text.
    pub text: String,
}

// ── Errors ───────────────────────────────────────────────────────────

/// Errors for system context operations.
///
/// # Source
/// Ported from `packages/core/src/system-context/index.ts` lines 82–93.
#[derive(Debug, thiserror::Error)]
pub enum SystemContextError {
    /// The key doesn't match the required pattern.
    #[error("invalid system context key: `{0}`")]
    InvalidKey(String),

    /// Initialization blocked because one or more sources are unavailable.
    #[error("initialization blocked: sources unavailable: {keys:?}")]
    InitializationBlocked {
        /// Keys of the unavailable sources.
        keys: Vec<SystemContextKey>,
    },

    /// Duplicate source key when combining contexts.
    #[error("duplicate system context key: `{key}`")]
    DuplicateKeyError {
        /// The duplicated key.
        key: SystemContextKey,
    },
}

// ── Internal types ───────────────────────────────────────────────────

/// Rendered text + snapshot pair.
#[derive(Debug, Clone)]
struct Rendered {
    text: String,
    snapshot: SourceSnapshot,
}

/// Comparison result for one source.
#[derive(Debug)]
enum Compared {
    /// Current value matches previous — no re-render needed.
    Unchanged,
    /// Current value differs — a re-render is needed.
    Updated,
}

/// Available entry after loading.
struct AvailableEntry {
    key: SystemContextKey,
    loaded: Loaded,
}

/// Entry — either available or unavailable.
enum Entry {
    Available(AvailableEntry),
    Unavailable { key: SystemContextKey },
}

/// Captures a loaded source value along with the source for rendering.
struct Loaded {
    source: Arc<dyn SystemContextSource>,
    data: serde_json::Value,
}

impl Loaded {
    /// Render the baseline text and snapshot.
    fn baseline(&self) -> Result<Rendered, SystemContextError> {
        let text = require_text(self.source.key(), "baseline", self.source.baseline(&self.data))?;
        let removal = self.source.removed();
        Ok(Rendered {
            text,
            snapshot: SourceSnapshot {
                json: self.data.clone(),
                removal: if removal.is_empty() {
                    None
                } else {
                    Some(removal)
                },
            },
        })
    }

    /// Compare with a stored snapshot.
    fn compare(&self, stored: &SourceSnapshot) -> Compared {
        if stored.json == self.data {
            Compared::Unchanged
        } else {
            Compared::Updated
        }
    }

    /// Render update text and snapshot.
    fn render_update(&self) -> Result<Rendered, SystemContextError> {
        let text = require_text(self.source.key(), "update", self.source.update(&self.data))?;
        let removal = self.source.removed();
        Ok(Rendered {
            text,
            snapshot: SourceSnapshot {
                json: self.data.clone(),
                removal: if removal.is_empty() {
                    None
                } else {
                    Some(removal)
                },
            },
        })
    }
}

// ── PackedSource ─────────────────────────────────────────────────────

/// Internal packed source — a type-erased source wrapping an [`Arc<dyn SystemContextSource>`].
///
/// # Source
/// Ported from `packages/core/src/system-context/index.ts` lines 95–98 (`PackedSource`).
struct PackedSource {
    source: Arc<dyn SystemContextSource>,
}

impl PackedSource {
    /// Load the source and produce a [`Loaded`] entry.
    fn load(&self) -> Result<Loaded, Box<dyn std::error::Error + Send + Sync>> {
        let data = self.source.load()?;
        Ok(Loaded {
            source: self.source.clone(),
            data,
        })
    }
}

// ── SystemContext ─────────────────────────────────────────────────────

/// Opaque carrier for composable system context sources.
///
/// # Source
/// Ported from `packages/core/src/system-context/index.ts` lines 43–46 (`SystemContext`).
pub struct SystemContext {
    sources: Vec<PackedSource>,
}

impl SystemContext {
    /// The identity context — no sources.
    ///
    /// # Source
    /// Ported from `packages/core/src/system-context/index.ts` line 128 (`empty`).
    pub fn empty() -> Self {
        Self {
            sources: Vec::new(),
        }
    }

    /// Close a typed source into a context that composes with differently typed sources.
    ///
    /// # Source
    /// Ported from `packages/core/src/system-context/index.ts` lines 131–169 (`make`).
    pub fn make(source: impl SystemContextSource + 'static) -> Self {
        Self {
            sources: vec![PackedSource {
                source: Arc::new(source),
            }],
        }
    }

    /// Combine contexts in order and reject duplicate source keys immediately.
    ///
    /// # Source
    /// Ported from `packages/core/src/system-context/index.ts` lines 172–176 (`combine`).
    pub fn combine(contexts: Vec<Self>) -> Result<Self, SystemContextError> {
        let mut all_sources = Vec::new();
        let mut seen_keys = HashSet::new();

        for ctx in contexts {
            for packed in ctx.sources {
                let key = packed.source.key().clone();
                if !seen_keys.insert(key.clone()) {
                    return Err(SystemContextError::DuplicateKeyError { key });
                }
                all_sources.push(packed);
            }
        }

        Ok(Self {
            sources: all_sources,
        })
    }

    /// Observe all sources and classify each as available or unavailable.
    fn observe(&self) -> Vec<Entry> {
        self.sources
            .iter()
            .map(|packed| match packed.load() {
                Ok(loaded) => Entry::Available(AvailableEntry {
                    key: loaded.source.key().clone(),
                    loaded,
                }),
                Err(_) => Entry::Unavailable {
                    key: packed.source.key().clone(),
                },
            })
            .collect()
    }

    /// Creates the immutable baseline and durable snapshot for a new generation.
    ///
    /// Fails with [`SystemContextError::InitializationBlocked`] if any source
    /// is temporarily unavailable.
    ///
    /// # Source
    /// Ported from `packages/core/src/system-context/index.ts` lines 194–211 (`initialize`).
    pub fn initialize(&self) -> Result<Generation, SystemContextError> {
        let entries = self.observe();
        let unavailable: Vec<_> = entries
            .iter()
            .filter_map(|e| match e {
                Entry::Unavailable { key } => Some(key.clone()),
                _ => None,
            })
            .collect();

        if !unavailable.is_empty() {
            return Err(SystemContextError::InitializationBlocked { keys: unavailable });
        }

        initialize_observation(&entries)
    }

    /// Reconciles current source values with one active generation.
    ///
    /// Returns [`ReconcileResult::Unchanged`] if nothing changed, or
    /// [`ReconcileResult::Updated`] with incremental text. Falls through to
    /// replacement if schema incompatibility is detected.
    ///
    /// # Source
    /// Ported from `packages/core/src/system-context/index.ts` lines 214–276 (`reconcile`).
    pub fn reconcile(&self, previous: &Snapshot) -> Result<ReconcileResult, SystemContextError> {
        let entries = self.observe();
        let first_pass = reconcile_observation(&entries, previous)?;
        match &first_pass {
            ReconcileResult::Unchanged | ReconcileResult::Updated { .. } => Ok(first_pass),
            _ => Ok(replace_observation(&entries, previous)),
        }
    }

    /// Creates a complete replacement generation or blocks while admitted
    /// context is unavailable.
    ///
    /// # Source
    /// Ported from `packages/core/src/system-context/index.ts` lines 278–287 (`replace`).
    pub fn replace(&self, previous: &Snapshot) -> ReconcileResult {
        let entries = self.observe();
        replace_observation(&entries, previous)
    }

    /// The number of packed sources in this context.
    pub fn len(&self) -> usize {
        self.sources.len()
    }

    /// Whether this context has no sources.
    pub fn is_empty(&self) -> bool {
        self.sources.is_empty()
    }
}

impl fmt::Debug for SystemContext {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("SystemContext")
            .field(
                "source_keys",
                &self
                    .sources
                    .iter()
                    .map(|s| s.source.key().as_str())
                    .collect::<Vec<_>>(),
            )
            .finish()
    }
}

// ── SystemContextRegistry ─────────────────────────────────────────────

/// Registry for collecting system context sources and building a [`SystemContext`].
///
/// # Source
/// Ported from `packages/core/src/system-context/index.ts` (registry pattern).
pub struct SystemContextRegistry {
    sources: Vec<Arc<dyn SystemContextSource>>,
}

impl SystemContextRegistry {
    /// Create an empty registry.
    pub fn new() -> Self {
        Self {
            sources: Vec::new(),
        }
    }

    /// Register a source. Panics if the same key is registered twice.
    pub fn register(&mut self, source: Arc<dyn SystemContextSource>) {
        let key = source.key().clone();
        for existing in &self.sources {
            if existing.key() == &key {
                panic!("duplicate system context key: `{key}`");
            }
        }
        self.sources.push(source);
    }

    /// Build a [`SystemContext`] from all registered sources.
    pub fn load(&self) -> SystemContext {
        SystemContext {
            sources: self
                .sources
                .iter()
                .map(|s| PackedSource { source: s.clone() })
                .collect(),
        }
    }

    /// The number of registered sources.
    pub fn len(&self) -> usize {
        self.sources.len()
    }

    /// Whether the registry has no sources.
    pub fn is_empty(&self) -> bool {
        self.sources.is_empty()
    }
}

impl Default for SystemContextRegistry {
    fn default() -> Self {
        Self::new()
    }
}

// ── Internal functions ───────────────────────────────────────────────

fn require_text(
    key: &SystemContextKey,
    kind: &str,
    text: String,
) -> Result<String, SystemContextError> {
    if text.is_empty() {
        Err(SystemContextError::InvalidKey(format!(
            "source `{key}` rendered an empty {kind}"
        )))
    } else {
        Ok(text)
    }
}

/// Build a [`Generation`] from observed entries.
fn initialize_observation(entries: &[Entry]) -> Result<Generation, SystemContextError> {
    let mut items: Vec<(SystemContextKey, Rendered)> = Vec::new();
    for entry in entries {
        if let Entry::Available(avail) = entry {
            items.push((avail.key.clone(), avail.loaded.baseline()?));
        }
    }

    let baseline = items
        .iter()
        .map(|(_, r)| r.text.as_str())
        .collect::<Vec<_>>()
        .join("\n\n");

    let snapshot: Snapshot = items
        .into_iter()
        .map(|(key, r)| (key, r.snapshot))
        .collect();

    Ok(Generation { baseline, snapshot })
}

/// Reconcile current entries against a previous snapshot.
///
/// Returns [`ReconcileResult::Unchanged`], [`ReconcileResult::Updated`],
/// or [`ReconcileResult::ReplacementReady`] / [`ReconcileResult::ReplacementBlocked`]
/// when a full replacement is needed.
///
/// # Source
/// Ported from `packages/core/src/system-context/index.ts` lines 224–276
/// (`reconcileObservation`).
fn reconcile_observation(
    entries: &[Entry],
    previous: &Snapshot,
) -> Result<ReconcileResult, SystemContextError> {
    let current_keys: HashSet<_> = entries.iter().filter_map(|e| match e {
        Entry::Available(a) => Some(a.key.clone()),
        Entry::Unavailable { key } => Some(key.clone()),
    }).collect();

    // First pass: compare available entries that have a previous snapshot.
    let mut comparisons: HashMap<SystemContextKey, Compared> = HashMap::new();
    for entry in entries {
        if let Entry::Available(avail) = entry {
            if let Some(stored) = previous.get(&avail.key) {
                let compared = avail.loaded.compare(stored);
                comparisons.insert(avail.key.clone(), compared);
            }
        }
    }

    // Second pass: check removed keys have removal text.
    for key in previous.keys() {
        if !current_keys.contains(key) {
            if previous[key].removal.is_none() {
                return Ok(ReconcileResult::ReplacementReady {
                    generation: initialize_observation(entries)?,
                });
            }
        }
    }

    // Third pass: build snapshot and collect updates.
    let mut snapshot: Snapshot = HashMap::new();
    let mut updates: Vec<String> = Vec::new();

    for entry in entries {
        let key = match entry {
            Entry::Available(a) => &a.key,
            Entry::Unavailable { key } => key,
        };
        let stored = previous.get(key);

        match entry {
            Entry::Unavailable { .. } => {
                if let Some(s) = stored {
                    snapshot.insert(key.clone(), s.clone());
                }
            }
            Entry::Available(avail) => {
                if stored.is_none() {
                    // New source — render baseline.
                    let rendered = avail.loaded.baseline()?;
                    updates.push(rendered.text);
                    snapshot.insert(avail.key.clone(), rendered.snapshot);
                    continue;
                }
                let stored = stored.expect("checked above");
                let compared = comparisons.get(&avail.key).expect("compared above");
                match compared {
                    Compared::Unchanged => {
                        snapshot.insert(avail.key.clone(), stored.clone());
                    }
                    Compared::Updated => {
                        let rendered = avail.loaded.render_update()?;
                        updates.push(rendered.text);
                        snapshot.insert(avail.key.clone(), rendered.snapshot);
                    }
                }
            }
        }
    }

    // Fourth pass: collect removal text for removed keys.
    let mut removed_keys: Vec<&SystemContextKey> = previous
        .keys()
        .filter(|k| !current_keys.contains(k))
        .collect();
    removed_keys.sort_by(|a, b| a.as_str().cmp(b.as_str()));

    for key in removed_keys {
        let removal = previous[key]
            .removal
            .as_deref()
            .expect("removal text was verified in second pass");
        updates.push(removal.to_string());
    }

    if updates.is_empty() {
        return Ok(ReconcileResult::Unchanged);
    }

    let text = updates.join("\n\n");
    Ok(ReconcileResult::Updated { text, snapshot })
}

/// Create a replacement or block if an unavailable source has a previous snapshot.
///
/// # Source
/// Ported from `packages/core/src/system-context/index.ts` lines 283–287
/// (`replaceObservation`).
fn replace_observation(entries: &[Entry], previous: &Snapshot) -> ReconcileResult {
    for entry in entries {
        if let Entry::Unavailable { key } = entry {
            if previous.contains_key(key) {
                return ReconcileResult::ReplacementBlocked;
            }
        }
    }
    match initialize_observation(entries) {
        Ok(generation) => ReconcileResult::ReplacementReady { generation },
        Err(_) => ReconcileResult::ReplacementBlocked,
    }
}

// ── Built-in context sources ─────────────────────────────────────────

/// System context source for the current working environment.
///
/// Provides information about the current working directory, platform,
/// and git status.
pub struct EnvironmentSource;

impl SystemContextSource for EnvironmentSource {
    fn key(&self) -> &SystemContextKey {
        static KEY: OnceLock<SystemContextKey> = OnceLock::new();
        KEY.get_or_init(|| {
            SystemContextKey::new("core/environment")
                .expect("valid system context key")
        })
    }

    fn load(&self) -> Result<serde_json::Value, Box<dyn std::error::Error + Send + Sync>> {
        let cwd = std::env::current_dir()
            .map(|p| p.to_string_lossy().to_string())
            .unwrap_or_else(|_| "unknown".to_string());

        let platform = std::env::consts::OS.to_string();

        // Try to get git branch
        let git_branch = std::process::Command::new("git")
            .args(["branch", "--show-current"])
            .output()
            .ok()
            .filter(|o| o.status.success())
            .map(|o| String::from_utf8_lossy(&o.stdout).trim().to_string())
            .unwrap_or_else(|| "unknown".to_string());

        Ok(serde_json::json!({
            "cwd": cwd,
            "platform": platform,
            "git_branch": git_branch,
        }))
    }

    fn baseline(&self, data: &serde_json::Value) -> String {
        format!(
            "Here is some useful information about the environment you are running in:\n\
             - Working directory: {}\n\
             - Platform: {}\n\
             - Git branch: {}",
            data["cwd"].as_str().unwrap_or("unknown"),
            data["platform"].as_str().unwrap_or("unknown"),
            data["git_branch"].as_str().unwrap_or("unknown"),
        )
    }

    fn update(&self, data: &serde_json::Value) -> String {
        format!(
            "The environment you are running in is now:\n\
             - Working directory: {}\n\
             - Platform: {}\n\
             - Git branch: {}",
            data["cwd"].as_str().unwrap_or("unknown"),
            data["platform"].as_str().unwrap_or("unknown"),
            data["git_branch"].as_str().unwrap_or("unknown"),
        )
    }

    fn removed(&self) -> String {
        "Environment information is no longer available.".to_string()
    }
}

/// System context source for the current date.
///
/// Provides today's date as context for the LLM.
pub struct DateSource;

impl SystemContextSource for DateSource {
    fn key(&self) -> &SystemContextKey {
        static KEY: OnceLock<SystemContextKey> = OnceLock::new();
        KEY.get_or_init(|| {
            SystemContextKey::new("core/date")
                .expect("valid system context key")
        })
    }

    fn load(&self) -> Result<serde_json::Value, Box<dyn std::error::Error + Send + Sync>> {
        let today = chrono::Utc::now().format("%Y-%m-%d").to_string();
        Ok(serde_json::Value::String(today))
    }

    fn baseline(&self, data: &serde_json::Value) -> String {
        format!("Today's date: {}", data.as_str().unwrap_or("unknown"))
    }

    fn update(&self, data: &serde_json::Value) -> String {
        format!(
            "Today's date is now: {}",
            data.as_str().unwrap_or("unknown")
        )
    }

    fn removed(&self) -> String {
        "Date information is no longer available.".to_string()
    }
}

// ── Tests ────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    // -- Test helpers ---------------------------------------------------

    struct StubSource {
        key: SystemContextKey,
        data: serde_json::Value,
        baseline_text: String,
        update_text: String,
        removal_text: String,
    }

    impl StubSource {
        fn new(
            key_str: &str,
            data: serde_json::Value,
            baseline_text: impl Into<String>,
            update_text: impl Into<String>,
            removal_text: impl Into<String>,
        ) -> Self {
            Self {
                key: SystemContextKey::new(key_str).expect("valid key"),
                data,
                baseline_text: baseline_text.into(),
                update_text: update_text.into(),
                removal_text: removal_text.into(),
            }
        }
    }

    impl SystemContextSource for StubSource {
        fn key(&self) -> &SystemContextKey {
            &self.key
        }

        fn load(&self) -> Result<serde_json::Value, Box<dyn std::error::Error + Send + Sync>> {
            Ok(self.data.clone())
        }

        fn baseline(&self, _data: &serde_json::Value) -> String {
            self.baseline_text.clone()
        }

        fn update(&self, _data: &serde_json::Value) -> String {
            self.update_text.clone()
        }

        fn removed(&self) -> String {
            self.removal_text.clone()
        }
    }

    struct UnavailableSource {
        key: SystemContextKey,
    }

    impl UnavailableSource {
        fn new(key_str: &str) -> Self {
            Self {
                key: SystemContextKey::new(key_str).expect("valid key"),
            }
        }
    }

    impl SystemContextSource for UnavailableSource {
        fn key(&self) -> &SystemContextKey {
            &self.key
        }

        fn load(&self) -> Result<serde_json::Value, Box<dyn std::error::Error + Send + Sync>> {
            Err("temporarily unavailable".into())
        }

        fn baseline(&self, _data: &serde_json::Value) -> String {
            unreachable!("should not be called on unavailable source")
        }

        fn update(&self, _data: &serde_json::Value) -> String {
            unreachable!("should not be called on unavailable source")
        }

        fn removed(&self) -> String {
            unreachable!("should not be called on unavailable source")
        }
    }

    // -- Key validation -------------------------------------------------

    #[test]
    fn key_valid_examples() {
        let valid = [
            "project/current",
            "git/branch",
            "env/dev",
            "a/b",
            "abc123/def456",
            "my.project/namespace/path",
        ];
        for s in valid {
            let key = SystemContextKey::new(s).expect(s);
            assert_eq!(key.as_str(), s);
        }
    }

    #[test]
    fn key_invalid_examples() {
        let invalid = [
            "",
            "no-slash",
            "/leading-slash",
            "trailing-slash/",
            "UPPER/Key",
            "has space/value",
            "a/b?c",
            "a/b=c",
            "a/b&c",
        ];
        for s in invalid {
            let result = SystemContextKey::new(s);
            assert!(result.is_err(), "expected error for key: {s}");
        }
    }

    #[test]
    fn key_display() {
        let key = SystemContextKey::new("project/current").expect("valid");
        assert_eq!(key.to_string(), "project/current");
    }

    #[test]
    fn key_into_string() {
        let key = SystemContextKey::new("git/branch").expect("valid");
        let s: String = key.into();
        assert_eq!(s, "git/branch");
    }

    #[test]
    fn key_serde_roundtrip() {
        let key = SystemContextKey::new("env/dev").expect("valid");
        let json = serde_json::to_string(&key).expect("serialize");
        let back: SystemContextKey = serde_json::from_str(&json).expect("deserialize");
        assert_eq!(key, back);
    }

    #[test]
    fn key_equality_and_hash() {
        let a = SystemContextKey::new("a/b").expect("valid");
        let b = SystemContextKey::new("a/b").expect("valid");
        let c = SystemContextKey::new("a/c").expect("valid");
        assert_eq!(a, b);
        assert_ne!(a, c);

        let mut set = HashSet::new();
        set.insert(a);
        set.insert(b);
        assert_eq!(set.len(), 1);
        set.insert(c);
        assert_eq!(set.len(), 2);
    }

    // -- SourceSnapshot -------------------------------------------------

    #[test]
    fn source_snapshot_serde() {
        let snap = SourceSnapshot {
            json: json!({"key": "value"}),
            removal: Some("removed text".to_string()),
        };
        let serialized = serde_json::to_string(&snap).expect("serialize");
        let deserialized: SourceSnapshot = serde_json::from_str(&serialized).expect("deserialize");
        assert_eq!(deserialized.json, json!({"key": "value"}));
        assert_eq!(deserialized.removal.as_deref(), Some("removed text"));
    }

    #[test]
    fn source_snapshot_optional_removal() {
        let snap = SourceSnapshot {
            json: json!(null),
            removal: None,
        };
        let serialized = serde_json::to_string(&snap).expect("serialize");
        let deserialized: SourceSnapshot = serde_json::from_str(&serialized).expect("deserialize");
        assert!(deserialized.removal.is_none());
    }

    // -- Empty context --------------------------------------------------

    #[test]
    fn empty_context() {
        let ctx = SystemContext::empty();
        assert!(ctx.is_empty());
        assert_eq!(ctx.len(), 0);
    }

    #[test]
    fn empty_context_initialize() {
        let ctx = SystemContext::empty();
        let gen = ctx.initialize().expect("empty context initializes");
        assert_eq!(gen.baseline, "");
        assert!(gen.snapshot.is_empty());
    }

    #[test]
    fn empty_context_debug() {
        let ctx = SystemContext::empty();
        let debug = format!("{ctx:?}");
        assert!(debug.contains("SystemContext"));
        assert!(debug.contains("source_keys"));
    }

    // -- Make -----------------------------------------------------------

    #[test]
    fn make_context_single_source() {
        let source = StubSource::new("project/current", json!("my-project"), "Project: my-project", "Updated: my-project", "Project removed");
        let ctx = SystemContext::make(source);
        assert_eq!(ctx.len(), 1);
        assert!(!ctx.is_empty());
    }

    #[test]
    fn make_context_initialize_baseline() {
        let source = StubSource::new(
            "env/dev",
            json!({"name": "dev"}),
            "Environment: dev",
            "Env changed",
            "Env removed",
        );
        let ctx = SystemContext::make(source);
        let gen = ctx.initialize().expect("initialize");
        assert_eq!(gen.baseline, "Environment: dev");
        assert!(gen.snapshot.contains_key(&SystemContextKey::new("env/dev").expect("valid")));
    }

    #[test]
    fn make_context_unavailable_blocks_initialization() {
        let source = UnavailableSource::new("git/branch");
        let ctx = SystemContext::make(source);
        let err = ctx.initialize().unwrap_err();
        match err {
            SystemContextError::InitializationBlocked { keys } => {
                assert_eq!(keys.len(), 1);
                assert_eq!(keys[0].as_str(), "git/branch");
            }
            _ => panic!("expected InitializationBlocked"),
        }
    }

    // -- Combine --------------------------------------------------------

    #[test]
    fn combine_two_contexts() {
        let a = SystemContext::make(StubSource::new("a/x", json!(1), "A", "A-upd", "A-rem"));
        let b = SystemContext::make(StubSource::new("b/y", json!(2), "B", "B-upd", "B-rem"));
        let combined = SystemContext::combine(vec![a, b]).expect("combine");
        assert_eq!(combined.len(), 2);
    }

    #[test]
    fn combine_duplicate_keys_errors() {
        let a = SystemContext::make(StubSource::new("a/x", json!(1), "A", "A-upd", "A-rem"));
        let b = SystemContext::make(StubSource::new("a/x", json!(2), "A2", "A2-upd", "A2-rem"));
        let result = SystemContext::combine(vec![a, b]);
        assert!(result.is_err());
        match result.unwrap_err() {
            SystemContextError::DuplicateKeyError { key } => {
                assert_eq!(key.as_str(), "a/x");
            }
            other => panic!("expected DuplicateKeyError, got: {other:?}"),
        }
    }

    #[test]
    fn combine_empty() {
        let combined = SystemContext::combine(vec![]).expect("combine empty");
        assert!(combined.is_empty());
    }

    // -- Reconcile: unchanged -------------------------------------------

    #[test]
    fn reconcile_unchanged() {
        let source = StubSource::new(
            "project/current",
            json!("my-project"),
            "Project: my-project",
            "Project updated",
            "Project removed",
        );
        let ctx = SystemContext::make(source);
        let gen = ctx.initialize().expect("initialize");

        // Same data → Unchanged
        let result = ctx.reconcile(&gen.snapshot).expect("reconcile");
        assert!(matches!(result, ReconcileResult::Unchanged));
    }

    // -- Reconcile: updated ---------------------------------------------

    #[test]
    fn reconcile_updated() {
        let source = StubSource::new(
            "project/current",
            json!("new-project"),
            "Project: new-project",
            "Project is now new-project",
            "Project removed",
        );
        let ctx = SystemContext::make(source);

        // Previous snapshot with different data
        let mut prev_snapshot = Snapshot::new();
        prev_snapshot.insert(
            SystemContextKey::new("project/current").expect("valid"),
            SourceSnapshot {
                json: json!("old-project"),
                removal: Some("Project removed".to_string()),
            },
        );

        let result = ctx.reconcile(&prev_snapshot).expect("reconcile");
        match result {
            ReconcileResult::Updated { text, snapshot } => {
                assert_eq!(text, "Project is now new-project");
                assert!(snapshot.contains_key(&SystemContextKey::new("project/current").expect("valid")));
            }
            other => panic!("expected Updated, got: {other:?}"),
        }
    }

    // -- Reconcile: new source adds baseline ----------------------------

    #[test]
    fn reconcile_new_source_adds_baseline() {
        let source = StubSource::new(
            "git/branch",
            json!("main"),
            "Branch: main",
            "Branch updated",
            "Branch removed",
        );
        let ctx = SystemContext::make(source);

        let prev_snapshot = Snapshot::new();
        let result = ctx.reconcile(&prev_snapshot).expect("reconcile");
        match result {
            ReconcileResult::Updated { text, snapshot } => {
                assert_eq!(text, "Branch: main");
                assert!(snapshot.contains_key(&SystemContextKey::new("git/branch").expect("valid")));
            }
            other => panic!("expected Updated, got: {other:?}"),
        }
    }

    // -- Reconcile: removed source uses removal text --------------------

    #[test]
    fn reconcile_removed_source_uses_removal_text() {
        let source = StubSource::new(
            "env/dev",
            json!("dev"),
            "Environment: dev",
            "Env updated",
            "Environment dev was removed",
        );
        let ctx = SystemContext::make(source);

        // Previous snapshot has a different source that's no longer loaded
        let mut prev_snapshot = Snapshot::new();
        prev_snapshot.insert(
            SystemContextKey::new("git/branch").expect("valid"),
            SourceSnapshot {
                json: json!("main"),
                removal: Some("Branch was removed".to_string()),
            },
        );
        prev_snapshot.insert(
            SystemContextKey::new("env/dev").expect("valid"),
            SourceSnapshot {
                json: json!("prod"),
                removal: Some("Environment was removed".to_string()),
            },
        );

        let result = ctx.reconcile(&prev_snapshot).expect("reconcile");
        match result {
            ReconcileResult::Updated { text, .. } => {
                assert!(text.contains("Branch was removed"));
                assert!(text.contains("Environment updated"));
            }
            other => panic!("expected Updated, got: {other:?}"),
        }
    }

    // -- Replace --------------------------------------------------------

    #[test]
    fn replace_ready() {
        let source = StubSource::new(
            "project/current",
            json!("new-project"),
            "Project: new-project",
            "Project updated",
            "Project removed",
        );
        let ctx = SystemContext::make(source);
        let mut prev_snapshot = Snapshot::new();
        prev_snapshot.insert(
            SystemContextKey::new("project/current").expect("valid"),
            SourceSnapshot {
                json: json!("old-project"),
                removal: Some("removed".to_string()),
            },
        );

        let result = ctx.replace(&prev_snapshot);
        assert!(matches!(result, ReconcileResult::ReplacementReady { .. }));
    }

    #[test]
    fn replace_blocked_when_unavailable_has_previous() {
        let source = UnavailableSource::new("git/branch");
        let ctx = SystemContext::make(source);
        let mut prev_snapshot = Snapshot::new();
        prev_snapshot.insert(
            SystemContextKey::new("git/branch").expect("valid"),
            SourceSnapshot {
                json: json!("main"),
                removal: Some("removed".to_string()),
            },
        );

        let result = ctx.replace(&prev_snapshot);
        assert!(matches!(result, ReconcileResult::ReplacementBlocked));
    }

    #[test]
    fn replace_not_blocked_when_unavailable_has_no_previous() {
        let source = UnavailableSource::new("git/branch");
        let ctx = SystemContext::make(source);
        let prev_snapshot = Snapshot::new();

        let result = ctx.replace(&prev_snapshot);
        // No previous snapshot for the unavailable key → not blocked
        // But initialize_observation will fail → ReplacementBlocked
        assert!(matches!(
            result,
            ReconcileResult::ReplacementBlocked
        ));
    }

    // -- Registry -------------------------------------------------------

    #[test]
    fn registry_register_and_load() {
        let mut registry = SystemContextRegistry::new();
        assert!(registry.is_empty());

        registry.register(Arc::new(StubSource::new(
            "project/current",
            json!("p"),
            "Project: p",
            "Project updated",
            "Project removed",
        )));
        assert_eq!(registry.len(), 1);

        let ctx = registry.load();
        assert_eq!(ctx.len(), 1);

        let gen = ctx.initialize().expect("initialize");
        assert_eq!(gen.baseline, "Project: p");
    }

    #[test]
    #[should_panic(expected = "duplicate system context key")]
    fn registry_duplicate_key_panics() {
        let mut registry = SystemContextRegistry::new();
        registry.register(Arc::new(StubSource::new(
            "a/b",
            json!(1),
            "A",
            "A-upd",
            "A-rem",
        )));
        registry.register(Arc::new(StubSource::new(
            "a/b",
            json!(2),
            "A2",
            "A2-upd",
            "A2-rem",
        )));
    }

    // -- Multiple sources -----------------------------------------------

    #[test]
    fn multiple_sources_initialize_joined_baseline() {
        let a = StubSource::new("a/one", json!(1), "Source A", "A updated", "A removed");
        let b = StubSource::new("b/two", json!(2), "Source B", "B updated", "B removed");
        let ctx = SystemContext::combine(vec![
            SystemContext::make(a),
            SystemContext::make(b),
        ])
        .expect("combine");

        let gen = ctx.initialize().expect("initialize");
        assert_eq!(gen.baseline, "Source A\n\nSource B");
        assert_eq!(gen.snapshot.len(), 2);
    }

    #[test]
    fn multiple_sources_reconcile_mixed_changes() {
        let a = StubSource::new("a/one", json!(1), "Source A", "A updated", "A removed");
        let b = StubSource::new("b/two", json!(2), "Source B", "B updated", "B removed");
        let ctx = SystemContext::combine(vec![
            SystemContext::make(a),
            SystemContext::make(b),
        ])
        .expect("combine");

        // Previous: a unchanged, b different
        let mut prev = Snapshot::new();
        prev.insert(
            SystemContextKey::new("a/one").expect("valid"),
            SourceSnapshot {
                json: json!(1),
                removal: Some("A removed".to_string()),
            },
        );
        prev.insert(
            SystemContextKey::new("b/two").expect("valid"),
            SourceSnapshot {
                json: json!(999),
                removal: Some("B removed".to_string()),
            },
        );

        let result = ctx.reconcile(&prev).expect("reconcile");
        match result {
            ReconcileResult::Updated { text, .. } => {
                assert_eq!(text, "B updated");
            }
            other => panic!("expected Updated with only B, got: {other:?}"),
        }
    }

    // ── EnvironmentSource ─────────────────────────────────────────────

    #[test]
    fn environment_source_key() {
        let source = EnvironmentSource;
        assert_eq!(source.key().as_str(), "core/environment");
    }

    #[test]
    fn environment_source_load() {
        let source = EnvironmentSource;
        let data = source.load().expect("load should succeed");
        assert!(data.is_object());
        assert!(data.get("cwd").is_some());
        assert!(data.get("platform").is_some());
        assert!(data.get("git_branch").is_some());
    }

    #[test]
    fn environment_source_baseline() {
        let source = EnvironmentSource;
        let data = json!({
            "cwd": "/home/user/project",
            "platform": "linux",
            "git_branch": "main"
        });
        let text = source.baseline(&data);
        assert!(text.contains("Here is some useful information about the environment"));
        assert!(text.contains("/home/user/project"));
        assert!(text.contains("linux"));
        assert!(text.contains("main"));
    }

    #[test]
    fn environment_source_update() {
        let source = EnvironmentSource;
        let data = json!({
            "cwd": "/home/user/project",
            "platform": "linux",
            "git_branch": "feature"
        });
        let text = source.update(&data);
        assert!(text.contains("The environment you are running in is now"));
        assert!(text.contains("feature"));
    }

    #[test]
    fn environment_source_removed() {
        let source = EnvironmentSource;
        let text = source.removed();
        assert_eq!(text, "Environment information is no longer available.");
    }

    #[test]
    fn environment_source_initialize() {
        let ctx = SystemContext::make(EnvironmentSource);
        let gen = ctx.initialize().expect("initialize");
        assert!(gen.baseline.contains("Here is some useful information"));
        assert!(gen.snapshot.contains_key(
            &SystemContextKey::new("core/environment").expect("valid")
        ));
    }

    // ── DateSource ────────────────────────────────────────────────────

    #[test]
    fn date_source_key() {
        let source = DateSource;
        assert_eq!(source.key().as_str(), "core/date");
    }

    #[test]
    fn date_source_load() {
        let source = DateSource;
        let data = source.load().expect("load should succeed");
        assert!(data.is_string());
        // Should be in YYYY-MM-DD format
        let date_str = data.as_str().expect("string value");
        assert_eq!(date_str.len(), 10);
        assert!(date_str.contains('-'));
    }

    #[test]
    fn date_source_baseline() {
        let source = DateSource;
        let data = json!("2026-01-15");
        let text = source.baseline(&data);
        assert_eq!(text, "Today's date: 2026-01-15");
    }

    #[test]
    fn date_source_update() {
        let source = DateSource;
        let data = json!("2026-06-19");
        let text = source.update(&data);
        assert_eq!(text, "Today's date is now: 2026-06-19");
    }

    #[test]
    fn date_source_removed() {
        let source = DateSource;
        let text = source.removed();
        assert_eq!(text, "Date information is no longer available.");
    }

    #[test]
    fn date_source_initialize() {
        let ctx = SystemContext::make(DateSource);
        let gen = ctx.initialize().expect("initialize");
        assert!(gen.baseline.starts_with("Today's date: "));
        assert!(gen.snapshot.contains_key(
            &SystemContextKey::new("core/date").expect("valid")
        ));
    }

    #[test]
    fn built_in_sources_in_registry() {
        let mut registry = SystemContextRegistry::new();
        registry.register(Arc::new(EnvironmentSource));
        registry.register(Arc::new(DateSource));
        assert_eq!(registry.len(), 2);

        let ctx = registry.load();
        let gen = ctx.initialize().expect("initialize");
        assert!(gen.baseline.contains("Here is some useful information"));
        assert!(gen.baseline.contains("Today's date: "));
    }
}
