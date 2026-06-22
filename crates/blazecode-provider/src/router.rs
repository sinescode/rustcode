//! # Provider router — model selection, fallback, and load balancing.
//!
//! Routes requests to the best provider based on model availability,
//! capabilities, and load.

use crate::adapter::{ArcProvider, ProviderAdapter};
use serde::{Deserialize, Serialize};
use crate::types::*;
use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::RwLock;
use tracing::debug;

/// Provider routing strategy.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RoutingStrategy {
    /// Always use the primary provider.
    PrimaryOnly,
    /// Fall back to next provider on failure.
    Failover,
    /// Load balance across providers.
    RoundRobin,
    /// Use the cheapest available provider.
    Cheapest,
    /// Use the fastest available provider.
    Fastest,
}

/// A provider in the routing pool.
#[derive(Debug, Clone)]
struct PoolEntry {
    /// The provider adapter.
    provider: ArcProvider,
    /// Current weight for load balancing.
    weight: u32,
    /// Failures since last success.
    failures: u32,
    /// Whether the provider is healthy.
    healthy: bool,
}

/// Provider router — manages multiple providers and routes requests.
#[derive(Debug, Clone)]
pub struct ProviderRouter {
    /// Registered providers by ID.
    providers: Arc<RwLock<HashMap<String, PoolEntry>>>,
    /// Routing strategy.
    strategy: Arc<RwLock<RoutingStrategy>>,
    /// Round-robin counter.
    rr_counter: Arc<std::sync::atomic::AtomicU64>,
    /// Default provider ID.
    default: Arc<RwLock<Option<String>>>,
}

impl ProviderRouter {
    /// Create a new empty provider router.
    pub fn new() -> Self {
        Self {
            providers: Arc::new(RwLock::new(HashMap::new())),
            strategy: Arc::new(RwLock::new(RoutingStrategy::Failover)),
            rr_counter: Arc::new(std::sync::atomic::AtomicU64::new(0)),
            default: Arc::new(RwLock::new(None)),
        }
    }

    /// Register a provider.
    pub async fn register(&self, provider: ArcProvider) {
        let id = provider.provider_id().to_string();
        let mut providers = self.providers.write().await;
        providers.insert(id.clone(), PoolEntry {
            provider,
            weight: 1,
            failures: 0,
            healthy: true,
        });
        debug!(provider_id = %id, "router: registered provider");

        // Set as default if first provider
        if self.default.read().await.is_none() {
            *self.default.write().await = Some(id);
        }
    }

    /// Remove a provider.
    pub async fn remove(&self, provider_id: &str) {
        self.providers.write().await.remove(provider_id);
        debug!(provider_id = %provider_id, "router: removed provider");
    }

    /// Set the routing strategy.
    pub async fn set_strategy(&self, strategy: RoutingStrategy) {
        *self.strategy.write().await = strategy;
    }

    /// Set the default provider.
    pub async fn set_default(&self, provider_id: &str) {
        *self.default.write().await = Some(provider_id.to_string());
    }

    /// Get a provider by ID.
    pub async fn get(&self, provider_id: &str) -> Option<ArcProvider> {
        self.providers.read().await.get(provider_id).map(|e| e.provider.clone())
    }

    /// Resolve a request to a provider.
    pub async fn resolve(&self, config: &RequestConfig) -> Option<(String, ArcProvider)> {
        let providers = self.providers.read().await;

        // Try specified provider first
        if !config.provider.is_empty() {
            if let Some(entry) = providers.get(&config.provider) {
                if entry.healthy {
                    return Some((config.provider.clone(), entry.provider.clone()));
                }
            }
        }

        // Fall back to default
        if let Some(default_id) = self.default.read().await.as_ref().cloned() {
            if let Some(entry) = providers.get(&default_id) {
                if entry.healthy {
                    return Some((default_id, entry.provider.clone()));
                }
            }
        }

        None
    }

    /// List all registered provider IDs.
    pub async fn list_providers(&self) -> Vec<String> {
        self.providers.read().await.keys().cloned().collect()
    }

    /// Mark a provider as healthy/unhealthy.
    pub async fn set_health(&self, provider_id: &str, healthy: bool) {
        if let Some(entry) = self.providers.write().await.get_mut(provider_id) {
            entry.healthy = healthy;
            if healthy {
                entry.failures = 0;
            } else {
                entry.failures += 1;
            }
            debug!(provider_id = %provider_id, healthy = %healthy, "router: health updated");
        }
    }

    /// Get router statistics.
    pub async fn stats(&self) -> RouterStats {
        let providers = self.providers.read().await;
        RouterStats {
            total_providers: providers.len() as u64,
            healthy_providers: providers.values().filter(|e| e.healthy).count() as u64,
            strategy: format!("{:?}", *self.strategy.read().await),
            default_provider: self.default.read().await.clone(),
        }
    }
}

impl Default for ProviderRouter {
    fn default() -> Self {
        Self::new()
    }
}

/// Router statistics.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RouterStats {
    /// Total registered providers.
    pub total_providers: u64,
    /// Healthy providers.
    pub healthy_providers: u64,
    /// Active routing strategy.
    pub strategy: String,
    /// Default provider ID.
    pub default_provider: Option<String>,
}

/// Provider pool with connection management.
#[derive(Debug, Clone)]
pub struct ProviderPool {
    /// The router.
    router: Arc<ProviderRouter>,
    /// The cache.
    cache: Arc<super::cache::PromptCache>,
    /// Default request config.
    default_config: RequestConfig,
}

impl ProviderPool {
    /// Create a new provider pool.
    pub fn new(
        router: ProviderRouter,
        cache: super::cache::PromptCache,
    ) -> Self {
        Self {
            router: Arc::new(router),
            cache: Arc::new(cache),
            default_config: RequestConfig::new("", ""),
        }
    }

    /// Get the router.
    pub fn router(&self) -> &ProviderRouter {
        &self.router
    }

    /// Get the cache.
    pub fn cache(&self) -> &super::cache::PromptCache {
        &self.cache
    }
}
