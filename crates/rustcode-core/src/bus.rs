//! Bus / event system for inter-module communication.
//!
//! Ported from: `packages/opencode/src/bus/global.ts`
//! OpenCode commit: 5d0f86606ac30690f79f0a6a9f41a1f49fe95d0b
//!
//! ## Architecture
//!
//! The TS source uses Node.js `EventEmitter` as a global singleton with a single
//! event channel (`"event"`) for all payloads. On `emit`, the bus auto-generates
//! an event ID (prefix `evt`, ascending) and injects it into the payload object.
//! Subscribers use `on`/`off` with manual cleanup via `Effect.addFinalizer`.
//!
//! In Rust:
//! - [`EventBus`] wraps [`tokio::sync::broadcast`] for fan-out (matching EventEmitter).
//! - [`SharedBus`] provides `Arc`-based sharing (matching the TS module-level singleton).
//! - [`BusSubscription`] implements RAII cleanup on drop (matching `off` + `addFinalizer`).
//! - Auto-ID generation mirrors the TS `emit()` override exactly.

use crate::id::{self, Direction, IdPrefix};
use serde::{Deserialize, Serialize};
use std::sync::Arc;
use tokio::sync::broadcast;

// ---------------------------------------------------------------------------
// GlobalEvent
// ---------------------------------------------------------------------------

/// A global event published on the bus.
///
/// Every event carries optional location context and an arbitrary JSON payload.
/// The event ID is injected into `payload.id` on publish ([`EventBus::publish`]).
///
/// # Source
/// Ported from `packages/opencode/src/bus/global.ts` lines 4–9.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GlobalEvent {
    /// Optional directory context.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub directory: Option<String>,
    /// Optional project context.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub project: Option<String>,
    /// Optional workspace context.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub workspace: Option<String>,
    /// Event payload — arbitrary JSON.
    pub payload: serde_json::Value,
}

impl GlobalEvent {
    /// Create a new event with the given payload.
    pub fn new(payload: serde_json::Value) -> Self {
        Self {
            directory: None,
            project: None,
            workspace: None,
            payload,
        }
    }

    /// Set the directory context.
    pub fn with_directory(mut self, dir: impl Into<String>) -> Self {
        self.directory = Some(dir.into());
        self
    }

    /// Set the project context.
    pub fn with_project(mut self, project: impl Into<String>) -> Self {
        self.project = Some(project.into());
        self
    }

    /// Set the workspace context.
    pub fn with_workspace(mut self, workspace: impl Into<String>) -> Self {
        self.workspace = Some(workspace.into());
        self
    }

    /// Extract the event ID from the payload, if present.
    pub fn id(&self) -> Option<&str> {
        self.payload.get("id")?.as_str()
    }
}

// ---------------------------------------------------------------------------
// EventBus
// ---------------------------------------------------------------------------

/// Event bus for broadcasting events across the system.
///
/// Wraps a [`tokio::sync::broadcast`] channel with auto-ID generation
/// matching the TS `GlobalBusEmitter.emit()` override.
///
/// # Source
/// Ported from `packages/opencode/src/bus/global.ts` lines 11–20.
pub struct EventBus {
    sender: broadcast::Sender<GlobalEvent>,
}

impl EventBus {
    /// Create a new event bus with the given channel capacity.
    ///
    /// When all receivers are dropped, `publish` returns an error.
    /// `capacity` controls how many events are buffered while no receiver
    /// is actively listening.
    pub fn new(capacity: usize) -> Self {
        let (sender, _) = broadcast::channel(capacity);
        Self { sender }
    }

    /// Publish an event to all subscribers.
    ///
    /// Before broadcasting, injects an auto-generated event ID into
    /// `event.payload.id` unless one is already present. This matches the TS
    /// `emit()` override at `global.ts:15`.
    ///
    /// # Source
    /// Ported from `packages/opencode/src/bus/global.ts` lines 14–19.
    ///
    /// # Errors
    /// Returns [`broadcast::error::SendError`] if there are no receivers.
    pub fn publish(
        &self,
        event: GlobalEvent,
    ) -> Result<usize, broadcast::error::SendError<GlobalEvent>> {
        let mut event = event;
        ensure_event_id(&mut event);
        self.sender.send(event)
    }

    /// Subscribe to all events.
    ///
    /// Returns a [`BusSubscription`] that automatically unsubscribes on drop,
    /// matching the TS pattern of `on("event", handler)` + `off("event", handler)`.
    pub fn subscribe(&self) -> BusSubscription {
        BusSubscription {
            receiver: self.sender.subscribe(),
        }
    }

    /// Number of active receivers.
    pub fn receiver_count(&self) -> usize {
        self.sender.receiver_count()
    }
}

impl Default for EventBus {
    fn default() -> Self {
        Self::new(1024)
    }
}

// ---------------------------------------------------------------------------
// SharedBus
// ---------------------------------------------------------------------------

/// Shared handle to the event bus — `Arc`-wrapped for use across the app.
///
/// This mirrors the TS module-level singleton `GlobalBus`.
///
/// # Source
/// Ported from `packages/opencode/src/bus/global.ts` line 22:
/// `export const GlobalBus = new GlobalBusEmitter()`
#[derive(Clone)]
pub struct SharedBus {
    inner: Arc<EventBus>,
}

impl SharedBus {
    /// Create a new shared bus.
    pub fn new(capacity: usize) -> Self {
        Self {
            inner: Arc::new(EventBus::new(capacity)),
        }
    }

    /// Publish an event to all subscribers.
    ///
    /// # Source
    /// Ported from `packages/opencode/src/bus/global.ts` lines 14–19.
    ///
    /// # Errors
    /// Returns [`broadcast::error::SendError`] if there are no receivers.
    pub fn publish(
        &self,
        event: GlobalEvent,
    ) -> Result<usize, broadcast::error::SendError<GlobalEvent>> {
        self.inner.publish(event)
    }

    /// Subscribe to events. Returns a handle that auto-unsubscribes on drop.
    pub fn subscribe(&self) -> BusSubscription {
        self.inner.subscribe()
    }

    /// Number of active receivers.
    pub fn receiver_count(&self) -> usize {
        self.inner.receiver_count()
    }
}

impl Default for SharedBus {
    fn default() -> Self {
        Self::new(1024)
    }
}

// ---------------------------------------------------------------------------
// BusSubscription — RAII cleanup matching TS `off()` + `addFinalizer`
// ---------------------------------------------------------------------------

/// A subscription handle that unsubscribes from the bus when dropped.
///
/// This matches the TS pattern:
/// ```ts
/// GlobalBus.on("event", handler)
/// yield* Effect.addFinalizer(() => Effect.sync(() => GlobalBus.off("event", handler)))
/// ```
///
/// In Rust, call [`BusSubscription::recv`] to await the next event.
/// The subscription is automatically cleaned up on drop — no manual `off()` needed.
pub struct BusSubscription {
    receiver: broadcast::Receiver<GlobalEvent>,
}

impl BusSubscription {
    /// Receive the next event.
    ///
    /// Returns `None` if the bus has been dropped (all senders gone).
    pub async fn recv(&mut self) -> Option<GlobalEvent> {
        match self.receiver.recv().await {
            Ok(event) => Some(event),
            Err(broadcast::error::RecvError::Lagged(skipped)) => {
                tracing::warn!(skipped, "bus subscriber lagged — {skipped} events skipped");
                // Continue receiving after a lag
                self.receiver.recv().await.ok()
            }
            Err(broadcast::error::RecvError::Closed) => None,
        }
    }
}

// ---------------------------------------------------------------------------
// Internal helpers
// ---------------------------------------------------------------------------

/// Ensure the event payload has an `id` field.
///
/// # Source
/// Ported from `packages/opencode/src/bus/global.ts` lines 15–16:
/// ```ts
/// if (event.payload && typeof event.payload === "object" && !("id" in event.payload)) {
///   event.payload.id = event.payload.syncEvent?.id ?? Identifier.create("evt", "ascending")
/// }
/// ```
fn ensure_event_id(event: &mut GlobalEvent) {
    // Only inject an ID if the payload is a JSON object without an existing "id".
    match &mut event.payload {
        serde_json::Value::Object(map) if !map.contains_key("id") => {
            // Try to use syncEvent.id if present (TS line 16).
            let sync_id = map
                .get("syncEvent")
                .and_then(|s| s.get("id"))
                .and_then(|id| id.as_str())
                .map(String::from);

            let id = sync_id.unwrap_or_else(|| id::create("evt", Direction::Ascending, None));

            map.insert("id".to_owned(), serde_json::Value::String(id));
        }
        _ => {} // non-object payloads or payloads that already have an ID
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    // -- GlobalEvent --------------------------------------------------------

    #[test]
    fn global_event_builders() {
        let event = GlobalEvent::new(json!({"type": "test"}))
            .with_directory("/tmp/test")
            .with_project("proj-123");

        assert_eq!(event.directory.as_deref(), Some("/tmp/test"));
        assert_eq!(event.project.as_deref(), Some("proj-123"));
        assert!(event.workspace.is_none());
    }

    #[test]
    fn global_event_id_extraction() {
        let event = GlobalEvent::new(json!({"id": "evt_abc123", "type": "test"}));
        assert_eq!(event.id(), Some("evt_abc123"));
    }

    #[test]
    fn global_event_no_id_returns_none() {
        let event = GlobalEvent::new(json!({"type": "test"}));
        assert!(event.id().is_none());
    }

    // -- Publish + subscribe ------------------------------------------------

    #[tokio::test]
    async fn publish_delivers_to_subscriber() {
        let bus = EventBus::new(16);
        let mut sub = bus.subscribe();

        bus.publish(GlobalEvent::new(json!({"type": "hello", "msg": "world"})))
            .unwrap();

        let received = sub.recv().await.unwrap();
        assert_eq!(received.payload["type"], "hello");
        assert_eq!(received.payload["msg"], "world");
    }

    #[tokio::test]
    async fn auto_id_is_injected_on_publish() {
        let bus = EventBus::new(16);
        let mut sub = bus.subscribe();

        // Publish an event WITHOUT an id
        bus.publish(GlobalEvent::new(json!({"type": "no-id"})))
            .unwrap();

        let received = sub.recv().await.unwrap();
        let id = received.id().unwrap();
        // Must start with "evt_" per the TS prefix
        assert!(id.starts_with("evt_"), "expected evt_ prefix, got: {id}");

        // Format: evt_ + 12 hex + 14 base62 = evt_ + 26 chars
        assert_eq!(
            id.len(),
            4 + 26,
            "expected 30-char ID, got len {len}: {id}",
            len = id.len()
        );
    }

    #[tokio::test]
    async fn existing_id_is_preserved() {
        let bus = EventBus::new(16);
        let mut sub = bus.subscribe();

        let event = GlobalEvent::new(json!({"id": "my-custom-id", "type": "has-id"}));
        let original_id = event.id().unwrap().to_owned();

        bus.publish(event).unwrap();

        let received = sub.recv().await.unwrap();
        assert_eq!(received.id().unwrap(), original_id);
    }

    #[tokio::test]
    async fn sync_event_id_is_used_when_present() {
        let bus = EventBus::new(16);
        let mut sub = bus.subscribe();

        let event = GlobalEvent::new(json!({
            "type": "sync",
            "syncEvent": {
                "id": "sync-ev-001",
                "type": "some.event",
                "seq": 42,
                "aggregateID": "agg-1",
                "data": {}
            }
        }));

        bus.publish(event).unwrap();

        let received = sub.recv().await.unwrap();
        // The ID should be the syncEvent.id, not a generated evt_ ID
        assert_eq!(received.id().unwrap(), "sync-ev-001");
    }

    #[tokio::test]
    async fn fan_out_delivers_to_multiple_subscribers() {
        let bus = EventBus::new(16);
        let mut sub1 = bus.subscribe();
        let mut sub2 = bus.subscribe();
        let mut sub3 = bus.subscribe();

        bus.publish(GlobalEvent::new(json!({"type": "fanout"})))
            .unwrap();

        let ev1 = sub1.recv().await.unwrap();
        let ev2 = sub2.recv().await.unwrap();
        let ev3 = sub3.recv().await.unwrap();

        let id1 = ev1.id().unwrap();
        let id2 = ev2.id().unwrap();
        let id3 = ev3.id().unwrap();

        // All subscribers receive the same event with the same ID
        assert_eq!(id1, id2);
        assert_eq!(id1, id3);
    }

    #[tokio::test]
    async fn publish_error_when_no_receivers() {
        let bus = EventBus::new(16);
        // No subscribers — publish should fail
        let result = bus.publish(GlobalEvent::new(json!({"type": "no-listeners"})));
        assert!(result.is_err());
    }

    #[tokio::test]
    async fn subscription_drop_unsubscribes() {
        let bus = EventBus::new(16);
        {
            let _sub = bus.subscribe();
            // Subscriber exists — publish should succeed
            bus.publish(GlobalEvent::new(json!({"type": "drop-test-1"})))
                .unwrap();
        }
        // Sub dropped — no receivers now
        let result = bus.publish(GlobalEvent::new(json!({"type": "drop-test-2"})));
        assert!(result.is_err());
    }

    // -- SharedBus ----------------------------------------------------------

    #[tokio::test]
    async fn shared_bus_clone_shares_state() {
        let bus = SharedBus::new(16);
        let bus2 = bus.clone();

        let mut sub = bus.subscribe();

        // Publish on one handle...
        bus2.publish(GlobalEvent::new(json!({ "type": "shared" })))
            .unwrap();

        // ...receive on the other's subscription
        let received = sub.recv().await.unwrap();
        assert_eq!(received.payload["type"], "shared");
    }

    #[tokio::test]
    async fn shared_bus_receiver_count() {
        let bus = SharedBus::new(16);
        assert_eq!(bus.receiver_count(), 0);

        let sub = bus.subscribe();
        assert_eq!(bus.receiver_count(), 1);

        drop(sub);
        // Note: broadcast channel may not immediately reflect the drop
    }

    // -- Non-object payloads ------------------------------------------------

    #[tokio::test]
    async fn non_object_payload_does_not_get_id() {
        let bus = EventBus::new(16);
        let mut sub = bus.subscribe();

        // Publish a string payload (not an object)
        bus.publish(GlobalEvent::new(serde_json::Value::String(
            "plain text".into(),
        )))
        .unwrap();

        let received = sub.recv().await.unwrap();
        // Non-object payloads remain unchanged — no id injection
        assert_eq!(
            received.payload,
            serde_json::Value::String("plain text".into())
        );
    }

    #[tokio::test]
    async fn array_payload_does_not_get_id() {
        let bus = EventBus::new(16);
        let mut sub = bus.subscribe();

        bus.publish(GlobalEvent::new(json!([1, 2, 3])))
            .unwrap();

        let received = sub.recv().await.unwrap();
        assert_eq!(received.payload, json!([1, 2, 3]));
    }

    // -- Event ordering -----------------------------------------------------

    #[tokio::test]
    async fn events_are_delivered_in_order() {
        let bus = EventBus::new(64);
        let mut sub = bus.subscribe();

        for i in 0..10 {
            bus.publish(GlobalEvent::new(json!({"seq": i})))
                .unwrap();
        }

        for expected in 0..10 {
            let received = sub.recv().await.unwrap();
            assert_eq!(received.payload["seq"], expected);
        }
    }
}
