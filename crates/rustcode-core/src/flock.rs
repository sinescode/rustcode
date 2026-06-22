//! File locking — directory-based advisory locking with stale detection.
//!
//! Ported from: `packages/core/src/util/flock.ts`
//! OpenCode commit: 5d0f86606ac30690f79f0a6a9f41a1f49fe95d0b
//!
//! ## Architecture
//!
//! Each lock is a directory named `{hash(key)}.lock` under a locks root.
//! The directory itself (`mkdir`) acts as the atomic acquire primitive.
//! A heartbeat file prevents stale eviction during long critical sections.
//! A `meta.json` file stores the owner's token for verified release.
//!
//! Stale recovery uses a `.breaker` intermediary directory so only one
//! contender performs cleanup when the lock holder has crashed.

use rand::Rng;
use sha2::{Digest, Sha256};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::time::{Duration, Instant};

/// Default stale threshold: 60 seconds.
const DEFAULT_STALE_MS: u64 = 60_000;
/// Default acquire timeout: 5 minutes.
const DEFAULT_TIMEOUT_MS: u64 = 300_000;
/// Default base delay for retry backoff.
const DEFAULT_BASE_DELAY_MS: u64 = 100;
/// Default max delay for retry backoff.
const DEFAULT_MAX_DELAY_MS: u64 = 2_000;

/// Options for acquiring a lock.
#[derive(Debug, Clone)]
pub struct FlockOptions {
    /// Directory to store lock files (default: `{state_dir}/locks`).
    pub dir: Option<PathBuf>,
    /// Milliseconds after which a lock without heartbeat is considered stale.
    pub stale_ms: u64,
    /// Milliseconds after which acquire attempts time out.
    pub timeout_ms: u64,
    /// Base delay in ms for retry backoff.
    pub base_delay_ms: u64,
    /// Maximum delay in ms for retry backoff.
    pub max_delay_ms: u64,
}

impl Default for FlockOptions {
    fn default() -> Self {
        Self {
            dir: None,
            stale_ms: DEFAULT_STALE_MS,
            timeout_ms: DEFAULT_TIMEOUT_MS,
            base_delay_ms: DEFAULT_BASE_DELAY_MS,
            max_delay_ms: DEFAULT_MAX_DELAY_MS,
        }
    }
}

/// A held lock lease. Dropping it releases the lock.
#[derive(Debug)]
pub struct FlockLease {
    lock_dir: PathBuf,
    token: String,
    released: Arc<AtomicBool>,
    heartbeat_stop: Option<Arc<AtomicBool>>,
}

impl FlockLease {
    /// Release the lock explicitly.
    pub async fn release(mut self) -> Result<(), String> {
        self.release_inner().await
    }

    async fn release_inner(&mut self) -> Result<(), String> {
        if self.released.swap(true, Ordering::SeqCst) {
            return Ok(()); // Already released
        }

        // Stop heartbeat
        if let Some(ref stop) = self.heartbeat_stop {
            stop.store(true, Ordering::SeqCst);
        }

        // Verify token before releasing
        let meta_path = self.lock_dir.join("meta.json");
        let content = tokio::fs::read_to_string(&meta_path)
            .await
            .map_err(|e| format!("Refusing to release: lock is compromised (metadata missing): {e}"))?;
        let parsed: serde_json::Value =
            serde_json::from_str(&content).map_err(|_| {
                "Refusing to release: lock is compromised (metadata invalid)".to_string()
            })?;
        let stored_token = parsed
            .get("token")
            .and_then(|v| v.as_str())
            .ok_or_else(|| "Refusing to release: lock token missing".to_string())?;

        if stored_token != self.token {
            return Err("Refusing to release: lock token mismatch (not the owner).".to_string());
        }

        tokio::fs::remove_dir_all(&self.lock_dir)
            .await
            .map_err(|e| format!("Failed to remove lock directory: {e}"))?;

        Ok(())
    }
}

impl Drop for FlockLease {
    fn drop(&mut self) {
        if !self.released.load(Ordering::SeqCst) {
            // Best-effort cleanup on drop
            if let Ok(content) = std::fs::read_to_string(self.lock_dir.join("meta.json")) {
                if let Ok(parsed) = serde_json::from_str::<serde_json::Value>(&content) {
                    if parsed.get("token").and_then(|v| v.as_str()) == Some(&self.token) {
                        let _ = std::fs::remove_dir_all(&self.lock_dir);
                    }
                }
            }
        }
    }
}

fn wall_now() -> f64 {
    // Approximate wall time in milliseconds (for stale comparison)
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs_f64()
        * 1000.0
}

fn hash_key(key: &str) -> String {
    hex::encode(Sha256::digest(key.as_bytes()))
}

fn jitter(ms: u64) -> u64 {
    let j = (ms as f64 * 0.3) as u64;
    let d = rand::thread_rng().gen_range(0..=(2 * j));
    ms.saturating_add(d).saturating_sub(j)
}

async fn is_stale(
    lock_dir: &Path,
    stale_ms: u64,
) -> Result<bool, std::io::Error> {
    let now = wall_now();
    let heartbeat_path = lock_dir.join("heartbeat");
    let meta_path = lock_dir.join("meta.json");

    // Check heartbeat first
    if let Ok(meta) = tokio::fs::metadata(&heartbeat_path).await {
        if let Ok(modified) = meta.modified() {
            let age = now - modified.duration_since(std::time::UNIX_EPOCH).unwrap_or_default().as_secs_f64() * 1000.0;
            return Ok(age > stale_ms as f64);
        }
    }

    // Fall back to meta.json mtime
    if let Ok(meta) = tokio::fs::metadata(&meta_path).await {
        if let Ok(modified) = meta.modified() {
            let age = now - modified.duration_since(std::time::UNIX_EPOCH).unwrap_or_default().as_secs_f64() * 1000.0;
            return Ok(age > stale_ms as f64);
        }
    }

    // Fall back to lock directory mtime
    if let Ok(meta) = tokio::fs::metadata(lock_dir).await {
        if let Ok(modified) = meta.modified() {
            let age = now - modified.duration_since(std::time::UNIX_EPOCH).unwrap_or_default().as_secs_f64() * 1000.0;
            return Ok(age > stale_ms as f64);
        }
    }

    Ok(false)
}

async fn try_acquire_lock_dir(
    lock_dir: &Path,
    stale_ms: u64,
) -> Result<Option<(String, Arc<AtomicBool>)>, String> {
    let token = uuid::Uuid::new_v4().to_string();
    let meta_path = lock_dir.join("meta.json");
    let heartbeat_path = lock_dir.join("heartbeat");

    // Try to create the lock directory atomically
    match tokio::fs::create_dir(lock_dir).await {
        Ok(()) => {
            // Successfully acquired
            let stop_flag = Arc::new(AtomicBool::new(false));

            // Write heartbeat file
            tokio::fs::write(&heartbeat_path, "")
                .await
                .map_err(|e| format!("Failed to write heartbeat: {e}"))?;

            // Write meta.json
            let meta = serde_json::json!({
                "token": token,
                "pid": std::process::id(),
                "hostname": hostname(),
                "createdAt": chrono::Utc::now().to_rfc3339(),
            });
            tokio::fs::write(&meta_path, serde_json::to_string_pretty(&meta).expect("meta serialization must succeed"))
                .await
                .map_err(|e| format!("Failed to write meta.json: {e}"))?;

            Ok(Some((token, stop_flag)))
        }
        Err(e) if e.kind() == std::io::ErrorKind::AlreadyExists => {
            // Lock directory exists — check staleness
            if !is_stale(lock_dir, stale_ms).await.unwrap_or(false) {
                return Ok(None); // Lock is still valid
            }

            // Stale — try to claim via breaker
            let breaker_path = lock_dir.with_extension("lock.breaker");
            match tokio::fs::create_dir(&breaker_path).await {
                Ok(()) => {
                    // We own the breaker — check staleness again
                    let still_stale = is_stale(lock_dir, stale_ms).await.unwrap_or(false);
                    if !still_stale {
                        // Lock was re-acquired between our checks
                        let _ = tokio::fs::remove_dir(&breaker_path).await;
                        return Ok(None);
                    }

                    // Remove stale lock directory
                    let _ = tokio::fs::remove_dir_all(lock_dir).await;

                    // Try to create it fresh
                    match tokio::fs::create_dir(lock_dir).await {
                        Ok(()) => {
                            let _ = tokio::fs::remove_dir(&breaker_path).await;
                            let stop_flag = Arc::new(AtomicBool::new(false));

                            tokio::fs::write(&heartbeat_path, "")
                                .await
                                .map_err(|e| format!("Failed to write heartbeat: {e}"))?;

                            let meta = serde_json::json!({
                                "token": token,
                                "pid": std::process::id(),
                                "hostname": hostname(),
                                "createdAt": chrono::Utc::now().to_rfc3339(),
                            });
                            tokio::fs::write(&meta_path, serde_json::to_string_pretty(&meta).expect("meta serialization must succeed"))
                                .await
                                .map_err(|e| format!("Failed to write meta.json: {e}"))?;

                            Ok(Some((token, stop_flag)))
                        }
                        Err(create_err) => {
                            let _ = tokio::fs::remove_dir(&breaker_path).await;
                            if create_err.kind() == std::io::ErrorKind::AlreadyExists
                                || create_err.kind() == std::io::ErrorKind::DirectoryNotEmpty
                            {
                                return Ok(None);
                            }
                            Err(format!("Failed to recreate lock dir: {create_err}"))
                        }
                    }
                }
                Err(breaker_err) if breaker_err.kind() == std::io::ErrorKind::AlreadyExists => {
                    // Another contender holds the breaker
                    let breaker_stale = tokio::fs::metadata(&breaker_path)
                        .await
                        .ok()
                        .and_then(|m| m.modified().ok())
                        .map(|t| {
                            let age = wall_now()
                                - t.duration_since(std::time::UNIX_EPOCH)
                                    .unwrap_or_default()
                                    .as_secs_f64()
                                    * 1000.0;
                            age > stale_ms as f64
                        })
                        .unwrap_or(false);

                    if breaker_stale {
                        let _ = tokio::fs::remove_dir(&breaker_path).await;
                    }
                    Ok(None)
                }
                Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
                    Ok(None)
                }
                Err(e) => Err(format!("Breaker error: {e}")),
            }
        }
        Err(e) => Err(format!("Failed to create lock dir: {e}")),
    }
}

/// Acquire a lock for the given key.
///
/// Returns a `FlockLease` that releases the lock on drop.
///
/// # Source
/// Ported from `packages/core/src/util/flock.ts` `acquire()`.
pub async fn acquire(key: &str, options: &FlockOptions) -> Result<FlockLease, String> {
    let lock_root = options
        .dir
        .clone()
        .unwrap_or_else(|| PathBuf::from("/tmp/opencode/locks"));

    tokio::fs::create_dir_all(&lock_root)
        .await
        .map_err(|e| format!("Failed to create lock root: {e}"))?;

    let lock_dir = lock_root.join(format!("{}.lock", hash_key(key)));

    let start = Instant::now();
    let timeout = Duration::from_millis(options.timeout_ms);
    let mut attempt = 0u64;
    let mut waited = 0u64;
    let mut delay = options.base_delay_ms;

    loop {
        if let Some((token, stop_flag)) =
            try_acquire_lock_dir(&lock_dir, options.stale_ms).await?
        {
            // Start heartbeat
            let hb_path = lock_dir.join("heartbeat");
            let hb_stop = stop_flag.clone();
            let hb_interval = std::cmp::max(100, options.stale_ms / 3);

            tokio::spawn(async move {
                loop {
                    tokio::time::sleep(Duration::from_millis(hb_interval)).await;
                    if hb_stop.load(Ordering::SeqCst) {
                        break;
                    }
                    // Touch the heartbeat file by re-writing
                    let _ = tokio::fs::write(&hb_path, "").await;
                }
            });

            return Ok(FlockLease {
                lock_dir,
                token,
                released: Arc::new(AtomicBool::new(false)),
                heartbeat_stop: Some(stop_flag),
            });
        }

        if start.elapsed() > timeout {
            return Err(format!("Timed out waiting for lock: {key}"));
        }

        let _ = attempt += 1;
        let ms = jitter(delay);
        tokio::time::sleep(Duration::from_millis(ms)).await;
        let _ = waited += ms;
        delay = std::cmp::min(options.max_delay_ms, (delay as f64 * 1.7) as u64);
    }
}

/// Clean up all stale locks in the lock directory.
///
/// Removes lock directories whose heartbeat file is older than `stale_ms`.
/// Should be called on startup to clean up locks from crashed processes.
///
/// # Source
/// Ported from `packages/core/src/util/flock.ts` — stale recovery pattern.
pub async fn cleanup_stale_locks(lock_dir: &std::path::Path, stale_ms: u64) -> Result<usize, String> {
    let mut cleaned = 0usize;
    let mut entries = tokio::fs::read_dir(lock_dir).await
        .map_err(|e| format!("read lock dir: {e}"))?;
    while let Some(entry) = entries.next_entry().await
        .map_err(|e| format!("read entry: {e}"))? {
        let path = entry.path();
        if path.extension().map(|e| e == "lock").unwrap_or(false) {
            if is_stale(&path, stale_ms).await.unwrap_or(false) {
                tokio::fs::remove_dir_all(&path).await
                    .map_err(|e| format!("remove stale lock {:?}: {e}", path))?;
                cleaned += 1;
            }
        }
    }
    Ok(cleaned)
}

/// Run a function with an exclusive lock.
///
/// # Source
/// Ported from `packages/core/src/util/flock.ts` `withLock()`.
pub async fn with_lock<T, F, Fut>(key: &str, options: &FlockOptions, f: F) -> Result<T, String>
where
    F: FnOnce() -> Fut,
    Fut: std::future::Future<Output = T>,
{
    let _lease = acquire(key, options).await?;
    Ok(f().await)
}

fn hostname() -> String {
    std::env::var("HOSTNAME")
        .or_else(|_| std::env::var("HOST"))
        .unwrap_or_else(|_| "unknown".to_string())
}

// ── Tests ──────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_acquire_and_release() {
        let dir = std::env::temp_dir().join("rustcode-flock-test-simple");
        let _ = std::fs::remove_dir_all(&dir);

        let opts = FlockOptions {
            dir: Some(dir.clone()),
            ..Default::default()
        };

        let lease = acquire("test-key", &opts).await.unwrap();
        // Try to acquire again — should fail immediately
        let second = try_acquire_lock_dir(
            &dir.join(format!("{}.lock", hash_key("test-key"))),
            opts.stale_ms,
        )
        .await
        .unwrap();
        assert!(second.is_none());

        lease.release().await.unwrap();
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[tokio::test]
    async fn test_with_lock() {
        let dir = std::env::temp_dir().join("rustcode-flock-test-with");
        let _ = std::fs::remove_dir_all(&dir);

        let opts = FlockOptions {
            dir: Some(dir.clone()),
            ..Default::default()
        };

        let result = with_lock("with-key", &opts, || async { 42 }).await.unwrap();
        assert_eq!(result, 42);

        let _ = std::fs::remove_dir_all(&dir);
    }

    #[tokio::test]
    async fn test_acquire_timeout() {
        let dir = std::env::temp_dir().join("rustcode-flock-test-timeout");
        let _ = std::fs::remove_dir_all(&dir);

        let opts = FlockOptions {
            dir: Some(dir.clone()),
            timeout_ms: 100,
            base_delay_ms: 10,
            max_delay_ms: 50,
            ..Default::default()
        };

        // Acquire first lock
        let _lease = acquire("timeout-key", &opts).await.unwrap();

        // Second acquire should time out
        let result = acquire("timeout-key", &opts).await;
        assert!(result.is_err());

        let _ = std::fs::remove_dir_all(&dir);
    }

    #[tokio::test]
    async fn test_token_verified_release() {
        let dir = std::env::temp_dir().join("rustcode-flock-test-token");
        let _ = std::fs::remove_dir_all(&dir);

        let opts = FlockOptions {
            dir: Some(dir.clone()),
            ..Default::default()
        };

        let lease = acquire("token-key", &opts).await.unwrap();

        // Corrupt the meta.json token
        let lock_dir = dir.join(format!("{}.lock", hash_key("token-key")));
        let bad_meta = serde_json::json!({"token": "wrong-token"});
        tokio::fs::write(
            lock_dir.join("meta.json"),
            serde_json::to_string_pretty(&bad_meta).unwrap(),
        )
        .await
        .unwrap();

        // Release should fail due to token mismatch
        let result = lease.release().await;
        assert!(result.is_err());

        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn test_hash_key() {
        let h1 = hash_key("hello");
        let h2 = hash_key("hello");
        let h3 = hash_key("world");
        assert_eq!(h1, h2);
        assert_ne!(h1, h3);
        assert_eq!(h1.len(), 64); // SHA-256 hex
    }

    #[test]
    fn test_jitter() {
        let base = 1000u64;
        for _ in 0..100 {
            let j = jitter(base);
            // Jitter should be within ±30% of base
            assert!(j >= 700 && j <= 1300, "jitter {j} out of range for base {base}");
        }
    }

    #[tokio::test]
    async fn test_stale_detection() {
        let dir = std::env::temp_dir().join("rustcode-flock-test-stale");
        let _ = std::fs::remove_dir_all(&dir);
        tokio::fs::create_dir_all(&dir).await.unwrap();

        // Create a lock dir with old heartbeat
        let lock_dir = dir.join("stale.lock");
        tokio::fs::create_dir_all(&lock_dir).await.unwrap();
        tokio::fs::write(lock_dir.join("heartbeat"), "").await.unwrap();

        // Should not be stale initially
        let stale = is_stale(&lock_dir, 60_000).await.unwrap();
        assert!(!stale);

        let _ = std::fs::remove_dir_all(&dir);
    }
}
