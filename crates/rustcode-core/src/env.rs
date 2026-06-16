//! Environment variable management — mutable, in-process env store with
//! per-directory isolation.
//!
//! Ported from: `packages/opencode/src/env/index.ts`
//! OpenCode commit: 5d0f86606ac30690f79f0a6a9f41a1f49fe95d0b
//!
//! ## Architecture
//!
//! The TS source wraps `process.env` inside an Effect `InstanceState` so
//! the env map can be mutated per-session (e.g. tools can set env vars) and
//! isolated between working directories.
//!
//! In Rust:
//! - [`Env`] is a single directory's mutable env state — seeded from OS vars.
//! - [`EnvStore`] maps directory paths → [`Env`] via [`Arc`], matching
//!   InstanceState isolation.
//!
//! ```text
//! EnvStore
//!   ├── global: Arc<Env>           (initialized from std::env::vars())
//!   └── instances: {              (per-directory, lazily created)
//!         "/home/user/proj1": Arc<Env>,
//!         "/home/user/proj2": Arc<Env>,
//!       }
//! ```

use std::collections::HashMap;
use std::sync::{Arc, RwLock};

// ---------------------------------------------------------------------------
// Env — per-directory mutable env state
// ---------------------------------------------------------------------------

/// A thread-safe, mutable environment variable store for a single directory.
///
/// Initialised from `std::env::vars()` on creation. Subsequent `set` / `remove`
/// calls only affect this instance — they do **not** modify the OS environment.
///
/// # Source
/// Ported from `packages/opencode/src/env/index.ts` lines 6–13 (`Interface`).
#[derive(Debug)]
pub struct Env {
    vars: RwLock<HashMap<String, String>>,
}

impl Env {
    /// Create a new store seeded from the current OS environment.
    ///
    /// # Source
    /// Ported from `packages/opencode/src/env/index.ts` line 22:
    /// `Effect.succeed({ ...process.env })`
    pub fn new() -> Self {
        Self {
            vars: RwLock::new(std::env::vars().collect()),
        }
    }

    // -- Read ---------------------------------------------------------------

    /// Get a single variable.
    ///
    /// Returns `None` when the key is absent.
    ///
    /// # Source
    /// Ported from `packages/opencode/src/env/index.ts` line 9
    /// (`Effect.fn("Env.get")`).
    pub fn get(&self, key: &str) -> Option<String> {
        self.vars
            .read()
            .expect("Env lock poisoned")
            .get(key)
            .cloned()
    }

    /// Get a variable, returning `default` when absent.
    pub fn get_or(&self, key: &str, default: &str) -> String {
        self.get(key).unwrap_or_else(|| default.to_owned())
    }

    /// Check whether a variable is set.
    pub fn has(&self, key: &str) -> bool {
        self.vars
            .read()
            .expect("Env lock poisoned")
            .contains_key(key)
    }

    /// Return a snapshot of all variables.
    ///
    /// # Source
    /// Ported from `packages/opencode/src/env/index.ts` line 10
    /// (`Effect.fn("Env.all")`).
    pub fn all(&self) -> HashMap<String, String> {
        self.vars.read().expect("Env lock poisoned").clone()
    }

    // -- Write --------------------------------------------------------------

    /// Set (or overwrite) a variable.
    ///
    /// # Source
    /// Ported from `packages/opencode/src/env/index.ts` lines 26–28
    /// (`Effect.fn("Env.set")`).
    pub fn set(&self, key: &str, value: &str) {
        self.vars
            .write()
            .expect("Env lock poisoned")
            .insert(key.to_owned(), value.to_owned());
    }

    /// Remove a variable. No-op if the key does not exist.
    ///
    /// # Source
    /// Ported from `packages/opencode/src/env/index.ts` lines 30–33
    /// (`Effect.fn("Env.remove")`).
    pub fn remove(&self, key: &str) {
        self.vars.write().expect("Env lock poisoned").remove(key);
    }
}

impl Default for Env {
    fn default() -> Self {
        Self::new()
    }
}

// ---------------------------------------------------------------------------
// EnvStore — per-directory isolation (mirrors InstanceState)
// ---------------------------------------------------------------------------

/// A store that isolates environment state per working directory.
///
/// This mirrors the TS `InstanceState` pattern used by the Env service:
/// the first access for a directory creates a fresh copy of OS env vars;
/// subsequent accesses return the same instance (shared via [`Arc`]).
///
/// # Source
/// Ported from `packages/opencode/src/env/index.ts` line 22:
/// `InstanceState.make(() => Effect.succeed({ ...process.env }))`
///
/// # Example
///
/// ```rust
/// use rustcode_core::env::EnvStore;
///
/// let store = EnvStore::new();
///
/// // Global access (no directory context)
/// store.global().set("FOO", "bar");
///
/// // Per-directory access — isolated
/// let dir_a = store.for_directory("/home/user/proj1");
/// let dir_b = store.for_directory("/home/user/proj2");
/// dir_a.set("PROJECT", "alpha");
/// dir_b.set("PROJECT", "beta");
/// assert_eq!(dir_a.get("PROJECT").unwrap(), "alpha");
/// assert_eq!(dir_b.get("PROJECT").unwrap(), "beta");
/// ```
pub struct EnvStore {
    /// Per-directory env instances, keyed by absolute directory path.
    instances: RwLock<HashMap<String, Arc<Env>>>,
    /// The global/shared env instance.
    global: Arc<Env>,
}

impl EnvStore {
    /// Create a new store with a global instance seeded from OS env.
    pub fn new() -> Self {
        Self {
            instances: RwLock::new(HashMap::new()),
            global: Arc::new(Env::new()),
        }
    }

    /// Return a handle to the global env (no directory context).
    ///
    /// Use this when no working directory is associated with the current
    /// operation — e.g. during CLI boot or global config loading.
    pub fn global(&self) -> EnvHandle {
        EnvHandle {
            inner: Arc::clone(&self.global),
        }
    }

    /// Get or create the environment for a specific working directory.
    ///
    /// The directory path acts as the cache key. The first call for a given
    /// directory clones the global OS environment into a new [`Env`];
    /// subsequent calls return a handle sharing the same instance.
    ///
    /// # Source
    /// Ported from `packages/opencode/src/effect/instance-state.ts`
    /// `InstanceState.get()` — scoped cache lookup.
    pub fn for_directory(&self, dir: &str) -> EnvHandle {
        let inner = {
            let mut instances = self.instances.write().expect("EnvStore lock poisoned");
            instances
                .entry(dir.to_owned())
                .or_insert_with(|| Arc::new(Env::new()))
                .clone()
        };
        EnvHandle { inner }
    }

    /// Invalidate the cached env for a directory, forcing a fresh copy on
    /// next access.
    ///
    /// # Source
    /// Ported from `packages/opencode/src/effect/instance-state.ts`
    /// `InstanceState.invalidate()`.
    pub fn invalidate(&self, dir: &str) {
        self.instances
            .write()
            .expect("EnvStore lock poisoned")
            .remove(dir);
    }

    /// Remove all directory-scoped instances (does not touch the global).
    ///
    /// # Source
    /// Ported from `packages/opencode/src/effect/instance-state.ts`
    /// `disposeAllInstancesEffect`.
    pub fn invalidate_all(&self) {
        self.instances
            .write()
            .expect("EnvStore lock poisoned")
            .clear();
    }
}

impl Default for EnvStore {
    fn default() -> Self {
        Self::new()
    }
}

// ---------------------------------------------------------------------------
// EnvHandle — handle to a directory-scoped or global Env
// ---------------------------------------------------------------------------

/// A handle that dispatches env operations to either a directory-scoped
/// [`Env`] or the global instance, sharing state via [`Arc`].
///
/// Created by [`EnvStore::for_directory`] or [`EnvStore::global`].
#[derive(Clone)]
pub struct EnvHandle {
    inner: Arc<Env>,
}

impl EnvHandle {
    /// Get a single variable.
    pub fn get(&self, key: &str) -> Option<String> {
        self.inner.get(key)
    }

    /// Get with fallback default.
    pub fn get_or(&self, key: &str, default: &str) -> String {
        self.inner.get_or(key, default)
    }

    /// Check whether a variable is set.
    pub fn has(&self, key: &str) -> bool {
        self.inner.has(key)
    }

    /// Return a snapshot of all variables.
    pub fn all(&self) -> HashMap<String, String> {
        self.inner.all()
    }

    /// Set (or overwrite) a variable.
    pub fn set(&self, key: &str, value: &str) {
        self.inner.set(key, value);
    }

    /// Remove a variable. No-op if the key does not exist.
    pub fn remove(&self, key: &str) {
        self.inner.remove(key);
    }
}

impl std::fmt::Debug for EnvHandle {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("EnvHandle").finish_non_exhaustive()
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    // -- Env basic operations -----------------------------------------------

    #[test]
    fn get_set_round_trip() {
        let env = Env::new();
        assert!(env.get("RUSTCODE_TEST_VAR").is_none());
        env.set("RUSTCODE_TEST_VAR", "hello");
        assert_eq!(env.get("RUSTCODE_TEST_VAR").unwrap(), "hello");
    }

    #[test]
    fn get_or_returns_default() {
        let env = Env::new();
        let val = env.get_or("NONEXISTENT_KEY_XYZ", "fallback");
        assert_eq!(val, "fallback");
    }

    #[test]
    fn get_or_returns_real_value() {
        let env = Env::new();
        env.set("REAL_KEY", "real_value");
        let val = env.get_or("REAL_KEY", "fallback");
        assert_eq!(val, "real_value");
    }

    #[test]
    fn has_detects_presence() {
        let env = Env::new();
        assert!(!env.has("TEMP_KEY"));
        env.set("TEMP_KEY", "1");
        assert!(env.has("TEMP_KEY"));
    }

    #[test]
    fn remove_deletes_key() {
        let env = Env::new();
        env.set("DELETE_ME", "bye");
        assert!(env.has("DELETE_ME"));
        env.remove("DELETE_ME");
        assert!(!env.has("DELETE_ME"));
    }

    #[test]
    fn remove_nonexistent_is_noop() {
        let env = Env::new();
        env.remove("DOES_NOT_EXIST"); // should not panic
    }

    #[test]
    fn all_returns_snapshot() {
        let env = Env::new();
        env.set("SNAPSHOT_A", "1");
        env.set("SNAPSHOT_B", "2");
        let snap = env.all();
        assert_eq!(snap.get("SNAPSHOT_A").map(|s| s.as_str()), Some("1"));
        assert_eq!(snap.get("SNAPSHOT_B").map(|s| s.as_str()), Some("2"));
    }

    #[test]
    fn set_overwrites_existing() {
        let env = Env::new();
        env.set("OVERWRITE", "first");
        env.set("OVERWRITE", "second");
        assert_eq!(env.get("OVERWRITE").unwrap(), "second");
    }

    // -- Env unchanged by other instances -----------------------------------

    #[test]
    fn env_instances_are_independent() {
        let a = Env::new();
        let b = Env::new();
        a.set("SHARED", "from-a");
        b.set("SHARED", "from-b");
        assert_eq!(a.get("SHARED").unwrap(), "from-a");
        assert_eq!(b.get("SHARED").unwrap(), "from-b");
    }

    // -- EnvStore per-directory isolation -----------------------------------

    #[test]
    fn env_store_isolates_directories() {
        let store = EnvStore::new();

        let dir_a = store.for_directory("/home/user/proj1");
        let dir_b = store.for_directory("/home/user/proj2");

        dir_a.set("PROJECT", "alpha");
        dir_b.set("PROJECT", "beta");

        assert_eq!(dir_a.get("PROJECT").unwrap(), "alpha");
        assert_eq!(dir_b.get("PROJECT").unwrap(), "beta");
    }

    #[test]
    fn env_store_same_directory_returns_same_instance() {
        let store = EnvStore::new();

        let first = store.for_directory("/tmp/test-dir");
        first.set("COUNTER", "1");

        let second = store.for_directory("/tmp/test-dir");
        assert_eq!(second.get("COUNTER").unwrap(), "1");

        // Mutation through either handle affects the same instance
        second.set("COUNTER", "2");
        assert_eq!(first.get("COUNTER").unwrap(), "2");
    }

    #[test]
    fn env_store_global_is_shared() {
        let store = EnvStore::new();
        store.global().set("GLOBAL_KEY", "shared");

        // Global access
        assert_eq!(store.global().get("GLOBAL_KEY").unwrap(), "shared");
    }

    #[test]
    fn env_store_invalidate_removes_directory_state() {
        let store = EnvStore::new();

        store
            .for_directory("/tmp/ephemeral")
            .set("EPHEMERAL", "value");
        assert_eq!(
            store
                .for_directory("/tmp/ephemeral")
                .get("EPHEMERAL")
                .unwrap(),
            "value"
        );

        store.invalidate("/tmp/ephemeral");

        // After invalidation, the key is gone (fresh copy from OS env)
        assert!(store
            .for_directory("/tmp/ephemeral")
            .get("EPHEMERAL")
            .is_none());
    }

    #[test]
    fn env_store_invalidate_all_clears_all_dirs() {
        let store = EnvStore::new();

        store.for_directory("/tmp/a").set("X", "1");
        store.for_directory("/tmp/b").set("X", "2");

        store.invalidate_all();

        assert!(store.for_directory("/tmp/a").get("X").is_none());
        assert!(store.for_directory("/tmp/b").get("X").is_none());
        // Global is untouched
        store.global().set("GLOBAL_ONLY", "survives");
        assert_eq!(store.global().get("GLOBAL_ONLY").unwrap(), "survives");
    }

    #[test]
    fn env_store_concurrent_directories() {
        let store = EnvStore::new();
        let dirs = ["/a", "/b", "/c", "/d"];

        for dir in &dirs {
            store.for_directory(dir).set("DIR", &format!("dir-{dir}"));
        }

        for dir in &dirs {
            assert_eq!(
                store.for_directory(dir).get("DIR").unwrap(),
                format!("dir-{dir}")
            );
        }
    }
}
