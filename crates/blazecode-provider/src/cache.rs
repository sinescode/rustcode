//! # Content-addressed prompt cache.
//!
//! Caches provider responses by SHA-256 hash of the input.
//! Provider-agnostic — maps to Anthropic/OpenAI/Google caching mechanisms.

use crate::types::ProviderResponse;
use dashmap::DashMap;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;
use std::time::{Duration, Instant};
use tracing::debug;

/// Configuration for the prompt cache.
#[derive(Debug, Clone)]
pub struct CacheConfig {
    /// Maximum number of cached entries.
    pub max_entries: usize,
    /// Time-to-live for cached entries.
    pub ttl: Duration,
    /// Maximum total cached size in bytes (approximate).
    pub max_size_bytes: u64,
}

impl Default for CacheConfig {
    fn default() -> Self {
        Self {
            max_entries: 1000,
            ttl: Duration::from_secs(3600), // 1 hour
            max_size_bytes: 100 * 1024 * 1024, // 100 MB
        }
    }
}

/// A cached entry with metadata.
#[derive(Debug, Clone)]
struct CacheEntry {
    /// The cached response.
    response: ProviderResponse,
    /// When the entry was created.
    created_at: Instant,
    /// Approximate size in bytes.
    size_bytes: u64,
    /// How many times this entry was hit.
    hits: u64,
}

/// Content-addressed prompt cache.
///
/// Computes SHA-256 of (system_prompt + messages + tools) and caches the response.
/// Compatible with Anthropic prompt caching, OpenAI cached responses, etc.
#[derive(Debug, Clone)]
pub struct PromptCache {
    /// Cache storage.
    cache: Arc<DashMap<String, CacheEntry>>,
    /// Cache configuration.
    config: Arc<CacheConfig>,
    /// Total cached bytes.
    total_bytes: Arc<AtomicU64>,
    /// Total cache hits.
    total_hits: Arc<AtomicU64>,
    /// Total cache misses.
    total_misses: Arc<AtomicU64>,
}

impl PromptCache {
    /// Create a new prompt cache.
    pub fn new(config: CacheConfig) -> Self {
        Self {
            cache: Arc::new(DashMap::with_capacity(config.max_entries)),
            config: Arc::new(config),
            total_bytes: Arc::new(AtomicU64::new(0)),
            total_hits: Arc::new(AtomicU64::new(0)),
            total_misses: Arc::new(AtomicU64::new(0)),
        }
    }

    /// Compute the cache key for a request.
    ///
    /// The key is SHA-256(system_prompt + messages_concatenated + tools_concatenated).
    pub fn compute_key(system: Option<&str>, messages: &[impl AsRef<str>], tools: &[impl AsRef<str>]) -> String {
        let mut hasher = Sha256::new();
        if let Some(sys) = system {
            hasher.update(sys.as_bytes());
        }
        for msg in messages {
            hasher.update(msg.as_ref().as_bytes());
        }
        for tool in tools {
            hasher.update(tool.as_ref().as_bytes());
        }
        hex::encode(hasher.finalize())
    }

    /// Get a cached response, if available and not expired.
    pub fn get(&self, key: &str) -> Option<ProviderResponse> {
        if let Some(entry) = self.cache.get(key) {
            if entry.created_at.elapsed() < self.config.ttl {
                self.total_hits.fetch_add(1, Ordering::Relaxed);
                debug!(key = %key, hits = %entry.hits + 1, "cache: hit");
                Some(entry.response.clone())
            } else {
                // Expired — remove
                let size = entry.size_bytes;
                drop(entry);
                self.cache.remove(key);
                self.total_bytes.fetch_sub(size, Ordering::Relaxed);
                self.total_misses.fetch_add(1, Ordering::Relaxed);
                debug!(key = %key, "cache: expired");
                None
            }
        } else {
            self.total_misses.fetch_add(1, Ordering::Relaxed);
            None
        }
    }

    /// Insert a response into the cache.
    pub fn insert(&self, key: String, response: ProviderResponse) {
        // Estimate size
        let size_bytes = estimate_size(&response) as u64;

        // Evict if needed
        while self.total_bytes.load(Ordering::Relaxed) + size_bytes > self.config.max_size_bytes
            || self.cache.len() >= self.config.max_entries
        {
            if !self.evict_one() {
                break; // Nothing to evict
            }
        }

        self.total_bytes.fetch_add(size_bytes, Ordering::Relaxed);
        self.cache.insert(key, CacheEntry {
            response,
            created_at: Instant::now(),
            size_bytes,
            hits: 0,
        });
    }

    /// Evict a single entry (oldest first).
    fn evict_one(&self) -> bool {
        // Find the oldest entry
        let oldest_key = self.cache.iter()
            .min_by_key(|e| e.created_at)
            .map(|e| e.key().clone());

        if let Some(key) = oldest_key {
            if let Some((_, entry)) = self.cache.remove(&key) {
                self.total_bytes.fetch_sub(entry.size_bytes, Ordering::Relaxed);
                return true;
            }
        }
        false
    }

    /// Clear all cached entries.
    pub fn clear(&self) {
        self.cache.clear();
        self.total_bytes.store(0, Ordering::Relaxed);
        debug!("cache: cleared");
    }

    /// Get cache statistics.
    pub fn stats(&self) -> CacheStats {
        CacheStats {
            entries: self.cache.len() as u64,
            total_bytes: self.total_bytes.load(Ordering::Relaxed),
            total_hits: self.total_hits.load(Ordering::Relaxed),
            total_misses: self.total_misses.load(Ordering::Relaxed),
        }
    }
}

/// Cache statistics.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CacheStats {
    /// Number of entries.
    pub entries: u64,
    /// Total cached bytes.
    pub total_bytes: u64,
    /// Total cache hits.
    pub total_hits: u64,
    /// Total cache misses.
    pub total_misses: u64,
}

impl CacheStats {
    /// Hit rate (0.0 - 1.0).
    pub fn hit_rate(&self) -> f64 {
        let total = self.total_hits + self.total_misses;
        if total == 0 { 0.0 } else { self.total_hits as f64 / total as f64 }
    }
}

/// Rough estimate of the memory size of a ProviderResponse.
fn estimate_size(response: &ProviderResponse) -> usize {
    let mut size = std::mem::size_of::<ProviderResponse>();
    if let Some(ref content) = response.content {
        size += content.len();
    }
    if let Some(ref reasoning) = response.reasoning {
        size += reasoning.len();
    }
    for tc in &response.tool_calls {
        size += tc.id.len() + tc.name.len();
        size += serde_json::to_string(&tc.input).unwrap_or_default().len();
    }
    size
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::types::{FinishReason, Usage};

    fn sample_response() -> ProviderResponse {
        ProviderResponse {
            content: Some("Hello, world!".into()),
            reasoning: None,
            tool_calls: vec![],
            finish_reason: FinishReason::Stop,
            usage: Usage::default(),
            metadata: None,
        }
    }

    #[test]
    fn test_cache_key_deterministic() {
        let key1 = PromptCache::compute_key(Some("system"), &["msg1"], &["tool1"]);
        let key2 = PromptCache::compute_key(Some("system"), &["msg1"], &["tool1"]);
        assert_eq!(key1, key2);
    }

    #[test]
    fn test_cache_key_different() {
        let key1: String = PromptCache::compute_key(Some("system"), &["msg1"] as &[&str], &[] as &[&str]);
        let key2: String = PromptCache::compute_key(Some("system"), &["msg2"] as &[&str], &[] as &[&str]);
        assert_ne!(key1, key2);
    }

    #[test]
    fn test_cache_insert_and_get() {
        let cache = PromptCache::new(CacheConfig::default());
        let key = "test-key-1";
        assert!(cache.get(key).is_none());

        cache.insert(key.into(), sample_response());
        let result = cache.get(key);
        assert!(result.is_some());
        assert_eq!(result.unwrap().content.unwrap(), "Hello, world!");
    }

    #[test]
    fn test_cache_expiry() {
        let config = CacheConfig {
            ttl: Duration::from_millis(1),
            ..Default::default()
        };
        let cache = PromptCache::new(config);
        cache.insert("expiry-test".into(), sample_response());
        std::thread::sleep(Duration::from_millis(5));
        assert!(cache.get("expiry-test").is_none());
    }

    #[test]
    fn test_cache_stats() {
        let cache = PromptCache::new(CacheConfig::default());
        cache.insert("stats-key".into(), sample_response());
        cache.get("stats-key"); // hit
        cache.get("stats-key"); // hit
        cache.get("missing");   // miss

        let stats = cache.stats();
        assert_eq!(stats.entries, 1);
        assert!(stats.total_hits >= 2);
        assert!(stats.total_misses >= 1);
    }

    #[test]
    fn test_cache_clear() {
        let cache = PromptCache::new(CacheConfig::default());
        cache.insert("clear-key".into(), sample_response());
        assert!(cache.get("clear-key").is_some());
        cache.clear();
        assert!(cache.get("clear-key").is_none());
    }

    #[test]
    fn test_eviction_when_full() {
        let config = CacheConfig {
            max_entries: 2,
            max_size_bytes: 10_000_000,
            ..Default::default()
        };
        let cache = PromptCache::new(config);
        cache.insert("key1".into(), sample_response());
        cache.insert("key2".into(), sample_response());
        cache.insert("key3".into(), sample_response());

        let stats = cache.stats();
        assert!(stats.entries <= 2); // At most 2 entries
    }
}
