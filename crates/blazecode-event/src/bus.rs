//! # Event bus — typed pub/sub with event definitions

use crate::types::{EventDefinition, EventPayload, SyncConfig};
use crate::projector::{ProjectorFn, ProjectorRegistry};
use std::collections::HashMap;
use std::sync::Arc;
use thiserror::Error;
use tokio::sync::{broadcast, RwLock};
use tracing::debug;

/// Error types for event bus operations.
#[derive(Debug, Error)]
pub enum EventError {
    /// Event type is not registered.
    #[error("unknown event type: {0}")]
    UnknownEventType(String),

    /// Event payload failed validation.
    #[error("validation error for {event_type}: {message}")]
    Validation {
        /// Event type.
        event_type: String,
        /// Validation message.
        message: String,
    },

    /// Broadcast channel error.
    #[error("broadcast error: {0}")]
    Broadcast(#[from] broadcast::error::SendError<EventPayload>),

    /// Projector error.
    #[error("projector error: {0}")]
    Projector(String),
}

/// The event registry holds event type definitions.
///
/// All events must be defined before they can be emitted.
#[derive(Debug, Clone)]
pub struct EventRegistry {
    /// Map of event type -> EventDefinition
    definitions: Arc<RwLock<HashMap<String, EventDefinition>>>,
}

impl EventRegistry {
    /// Create a new empty event registry.
    pub fn new() -> Self {
        Self {
            definitions: Arc::new(RwLock::new(HashMap::new())),
        }
    }

    /// Register an event type definition.
    pub async fn define(&mut self, def: EventDefinition) {
        let event_type = def.event_type.clone();
        self.definitions.write().await.insert(event_type.clone(), def);
        debug!(event_type = %event_type, "event: registered event type");
    }

    /// Check if an event type is registered.
    pub async fn is_defined(&self, event_type: &str) -> bool {
        self.definitions.read().await.contains_key(event_type)
    }

    /// Get the definition for an event type.
    pub async fn get(&self, event_type: &str) -> Option<EventDefinition> {
        self.definitions.read().await.get(event_type).cloned()
    }

    /// Get all registered event types.
    pub async fn registered_types(&self) -> Vec<String> {
        self.definitions.read().await.keys().cloned().collect()
    }
}

impl Default for EventRegistry {
    fn default() -> Self {
        Self::new()
    }
}

/// The event bus — typed pub/sub for event-driven architecture.
///
/// # Example
///
/// ```ignore
/// let mut bus = EventBus::new(256);
/// bus.register_event_type("user.created", None).await;
/// bus.emit("user.created", serde_json::json!({"id": 1})).await;
/// ```
#[derive(Debug)]
pub struct EventBus {
    /// Event type registry.
    registry: EventRegistry,
    /// Broadcast channel for event publishing.
    tx: broadcast::Sender<EventPayload>,
    /// Sentinel receiver — keeps the broadcast channel alive even with 0 external subscribers.
    #[allow(dead_code)]
    _rx: broadcast::Receiver<EventPayload>,
    /// Projector registry.
    projectors: ProjectorRegistry,
    /// Counter for sequence numbers.
    seq_counter: Arc<RwLock<HashMap<String, u64>>>,
}

impl EventBus {
    /// Create a new event bus with the given broadcast channel capacity.
    pub fn new(capacity: usize) -> Self {
        let (tx, rx) = broadcast::channel(capacity);
        Self {
            registry: EventRegistry::new(),
            tx,
            _rx: rx,
            projectors: ProjectorRegistry::new(),
            seq_counter: Arc::new(RwLock::new(HashMap::new())),
        }
    }

    /// Get a reference to the event registry.
    pub fn registry(&self) -> &EventRegistry {
        &self.registry
    }

    /// Get a mutable reference to the event registry.
    pub fn registry_mut(&mut self) -> &mut EventRegistry {
        &mut self.registry
    }

    /// Get the projector registry.
    pub fn projectors(&self) -> &ProjectorRegistry {
        &self.projectors
    }

    /// Get a subscribe handle for all events.
    pub fn subscribe(&self) -> broadcast::Receiver<EventPayload> {
        self.tx.subscribe()
    }

    /// Emit an event — validates type, runs projectors, broadcasts.
    pub async fn emit(&self, event_type: &str, data: serde_json::Value) -> Result<EventPayload, EventError> {
        // Validate event type is registered
        if !self.registry.is_defined(event_type).await {
            return Err(EventError::UnknownEventType(event_type.to_string()));
        }

        // Get next sequence number
        let seq = {
            let mut counters = self.seq_counter.write().await;
            let next = counters.entry(event_type.to_string()).or_insert(0);
            *next += 1;
            *next
        };

        let payload = EventPayload::new(event_type, data).with_seq(seq);

        // Run projectors
        self.projectors.trigger(&payload).await;

        // Broadcast to subscribers
        self.tx.send(payload.clone()).map_err(EventError::Broadcast)?;

        debug!(event_type = %event_type, seq = %seq, "event: emitted");
        Ok(payload)
    }

    /// Register an event type and optionally set up a projector.
    pub async fn register_event_type(
        &mut self,
        event_type: impl Into<String>,
        sync: Option<SyncConfig>,
    ) {
        let event_type: String = event_type.into();
        let schema = serde_json::json!({});
        self.registry
            .define(EventDefinition::new(&event_type, sync, schema))
            .await;
        debug!(event_type = %event_type, "event: registered event type");
    }

    /// Register a projector that fires on matching events.
    pub async fn project(
        &self,
        event_type: impl Into<String>,
        projector: impl Into<ProjectorFn>,
    ) {
        self.projectors.register(event_type.into(), projector.into()).await;
    }
}

impl Default for EventBus {
    fn default() -> Self {
        Self::new(256)
    }
}

/// A convenience handle for the V2 event system (backward compat).
///
/// This wraps an EventBus and provides the same API as the original
/// blazecode-core EventV2.
#[derive(Debug, Clone)]
pub struct EventV2 {
    /// The bus capacity.
    capacity: usize,
    /// Shared event bus.
    pub bus: Arc<RwLock<EventBus>>,
}

impl EventV2 {
    /// Create a new V2 event handle with the given capacity.
    pub fn new(capacity: usize, _store: Option<()>) -> Self {
        Self {
            capacity,
            bus: Arc::new(RwLock::new(EventBus::new(capacity))),
        }
    }

    /// Get the event bus (read-only).
    pub async fn bus(&self) -> tokio::sync::RwLockReadGuard<'_, EventBus> {
        self.bus.read().await
    }

    /// Get the event bus (mutable).
    pub async fn bus_mut(&self) -> tokio::sync::RwLockWriteGuard<'_, EventBus> {
        self.bus.write().await
    }

    /// Emit an event.
    pub async fn emit(&self, event_type: &str, data: serde_json::Value) -> Result<EventPayload, EventError> {
        self.bus.read().await.emit(event_type, data).await
    }

    /// Register a projector.
    pub async fn project(
        &self,
        event_type: impl Into<String>,
        projector: impl Into<ProjectorFn>,
    ) {
        let event_type: String = event_type.into();
        let proj = projector.into();
        self.bus.read().await.project(event_type, proj).await;
    }

    /// Get projectors for an event type.
    pub async fn get_projectors(&self, event_type: &str) -> Vec<ProjectorFn> {
        self.bus.read().await.projectors().get(event_type).await
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_event_registration() {
        let mut bus = EventBus::new(16);
        bus.register_event_type("test.event", None).await;
        assert!(bus.registry().is_defined("test.event").await);
    }

    #[tokio::test]
    async fn test_emit_unknown_event_fails() {
        let mut bus = EventBus::new(16);
        let result = bus.emit("unknown.event", serde_json::json!({})).await;
        assert!(result.is_err());
        assert!(matches!(result.unwrap_err(), EventError::UnknownEventType(_)));
    }

    #[tokio::test]
    async fn test_emit_and_subscribe() {
        let mut bus = EventBus::new(16);
        let mut rx = bus.subscribe();
        bus.register_event_type("test.event", None).await;

        bus.emit("test.event", serde_json::json!({"msg": "hello"})).await.unwrap();

        let received = tokio::time::timeout(
            std::time::Duration::from_secs(1),
            rx.recv(),
        ).await.unwrap().unwrap();
        assert_eq!(received.event_type, "test.event");
        assert_eq!(received.data["msg"], "hello");
    }

    #[tokio::test]
    async fn test_projector_fires_on_event() {
        let mut bus = EventBus::new(16);
        bus.register_event_type("test.projector", None).await;

        let fired = std::sync::Arc::new(std::sync::atomic::AtomicBool::new(false));
        let fired_clone = fired.clone();

        bus.project("test.projector", crate::mk_projector_fn(move |_payload| {
            let f = fired_clone.clone();
            Box::pin(async move {
                f.store(true, std::sync::atomic::Ordering::SeqCst);
                Ok(())
            })
        })).await;

        bus.emit("test.projector", serde_json::json!({})).await.unwrap();

        // Give the async projector time to fire
        tokio::time::sleep(std::time::Duration::from_millis(50)).await;
        assert!(fired.load(std::sync::atomic::Ordering::SeqCst));
    }

    #[tokio::test]
    async fn test_event_v2_compat() {
        let ev = EventV2::new(16, None);
        ev.bus_mut().await.register_event_type("compat.test", None).await;
        ev.emit("compat.test", serde_json::json!({"ok": true})).await.unwrap();
    }

    #[tokio::test]
    async fn test_seq_counter() {
        let mut bus = EventBus::new(16);
        bus.register_event_type("seq.test", None).await;

        let e1 = bus.emit("seq.test", serde_json::json!({})).await.unwrap();
        let e2 = bus.emit("seq.test", serde_json::json!({})).await.unwrap();

        assert_eq!(e1.seq, Some(1));
        assert_eq!(e2.seq, Some(2));
    }

    #[tokio::test]
    async fn test_event_v2_get_projectors() {
        let ev = EventV2::new(16, None);
        ev.bus_mut().await.register_event_type("proj.test", None).await;

        let fired = std::sync::Arc::new(std::sync::atomic::AtomicBool::new(false));
        let f = fired.clone();
        ev.project("proj.test", crate::mk_projector_fn(move |_payload| {
            let inner = f.clone();
            Box::pin(async move {
                inner.store(true, std::sync::atomic::Ordering::SeqCst);
                Ok(())
            })
        })).await;

        let projectors = ev.get_projectors("proj.test").await;
        assert_eq!(projectors.len(), 1);
    }
}
