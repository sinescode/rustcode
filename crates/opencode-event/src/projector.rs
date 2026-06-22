//! # Event projectors — handlers that react to events

use crate::types::EventPayload;
use async_trait::async_trait;
use futures::future::BoxFuture;
use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::RwLock;
use tracing::{debug, warn};

/// A projector result.
pub type ProjectorResult = Result<(), Box<dyn std::error::Error + Send + Sync>>;

/// A projector function.
///
/// Projectors are async functions that receive an event payload and
/// produce a result. They run after the event is emitted but before
/// it's broadcast to subscribers.
pub struct ProjectorFn {
    /// The inner async function.
    pub inner: Arc<dyn Fn(EventPayload) -> BoxFuture<'static, ProjectorResult> + Send + Sync>,
}

impl ProjectorFn {
    /// Create a new projector function.
    pub fn new<F>(f: F) -> Self
    where
        F: Fn(EventPayload) -> BoxFuture<'static, ProjectorResult> + Send + Sync + 'static,
    {
        Self {
            inner: Arc::new(f),
        }
    }

    /// Call the projector with a payload.
    pub async fn call(&self, payload: EventPayload) -> ProjectorResult {
        (self.inner)(payload).await
    }
}

impl std::fmt::Debug for ProjectorFn {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("ProjectorFn").finish_non_exhaustive()
    }
}

impl Clone for ProjectorFn {
    fn clone(&self) -> Self {
        Self {
            inner: Arc::clone(&self.inner),
        }
    }
}

/// Convenience function to create a projector from an async closure.
///
/// # Example
///
/// ```ignore
/// bus.project("event.type", mk_projector_fn(move |payload| {
///     Box::pin(async move {
///         // handle event
///         Ok(())
///     })
/// })).await;
/// ```
pub fn mk_projector_fn<F>(f: F) -> ProjectorFn
where
    F: Fn(EventPayload) -> BoxFuture<'static, ProjectorResult> + Send + Sync + 'static,
{
    ProjectorFn::new(f)
}

/// Registry of projectors, indexed by event type.
#[derive(Debug, Clone)]
pub struct ProjectorRegistry {
    /// Map of event type -> list of projectors.
    projectors: Arc<RwLock<HashMap<String, Vec<ProjectorFn>>>>,
}

impl ProjectorRegistry {
    /// Create a new projector registry.
    pub fn new() -> Self {
        Self {
            projectors: Arc::new(RwLock::new(HashMap::new())),
        }
    }

    /// Register a projector for an event type.
    pub async fn register(&self, event_type: String, projector: ProjectorFn) {
        self.projectors
            .write()
            .await
            .entry(event_type.clone())
            .or_default()
            .push(projector);
        debug!(event_type = %event_type, "projector: registered");
    }

    /// Get all projectors registered for an event type.
    pub async fn get(&self, event_type: &str) -> Vec<ProjectorFn> {
        self.projectors
            .read()
            .await
            .get(event_type)
            .cloned()
            .unwrap_or_default()
    }

    /// Trigger all projectors registered for an event type.
    pub async fn trigger(&self, payload: &EventPayload) {
        let projectors = self.get(&payload.event_type).await;
        for projector in projectors {
            let payload = payload.clone();
            tokio::spawn(async move {
                if let Err(e) = projector.call(payload).await {
                    warn!(error = %e, "projector: execution failed");
                }
            });
        }
    }
}

impl Default for ProjectorRegistry {
    fn default() -> Self {
        Self::new()
    }
}

/// Trait for implementing projectors.
///
/// For simple use cases, prefer `mk_projector_fn` or `ProjectorFn::new`.
#[async_trait]
pub trait Projector: Send + Sync + 'static {
    /// Handle an event payload.
    async fn handle(&self, payload: EventPayload) -> ProjectorResult;
}

#[async_trait]
impl<F, Fut> Projector for F
where
    F: Fn(EventPayload) -> Fut + Send + Sync + 'static,
    Fut: std::future::Future<Output = ProjectorResult> + Send + 'static,
{
    async fn handle(&self, payload: EventPayload) -> ProjectorResult {
        (self)(payload).await
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_projector_fires() {
        let registry = ProjectorRegistry::new();
        let fired = std::sync::Arc::new(std::sync::atomic::AtomicBool::new(false));
        let f = fired.clone();

        registry.register(
            "test.event".into(),
            mk_projector_fn(move |_payload| {
                let inner = f.clone();
                Box::pin(async move {
                    inner.store(true, std::sync::atomic::Ordering::SeqCst);
                    Ok(())
                })
            }),
        ).await;

        let payload = EventPayload::new("test.event", serde_json::json!({}));
        registry.trigger(&payload).await;

        // Give the async task time to fire
        tokio::time::sleep(std::time::Duration::from_millis(50)).await;
        assert!(fired.load(std::sync::atomic::Ordering::SeqCst));
    }

    #[test]
    fn test_projector_fn_trait_impl() {
        let projector = mk_projector_fn(|_payload| {
            Box::pin(async { Ok(()) })
        });
        // Should be Clone + Send + Sync
        let _cloned = projector.clone();
    }
}
