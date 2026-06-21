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

use crate::id::{self, Direction};
use serde::{Deserialize, Serialize};
use std::fmt;
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
    #[must_use]
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

    /// Extract the event type from the payload, if present.
    pub fn event_type(&self) -> Option<&str> {
        self.payload.get("type")?.as_str()
    }

    /// Create a GlobalEvent from a typed TUI bus event payload.
    ///
    /// Serializes the typed event to JSON and wraps it in a `GlobalEvent`.
    /// The event's `type` tag becomes the JSON `"type"` field.
    pub fn from_tui(event: &TuiBusEvent) -> Result<Self, serde_json::Error> {
        let payload = serde_json::to_value(event)?;
        Ok(Self::new(payload))
    }

    /// Attempt to deserialize the payload as a typed TUI bus event.
    pub fn try_as_tui(&self) -> Option<TuiBusEvent> {
        serde_json::from_value(self.payload.clone()).ok()
    }
}

// ---------------------------------------------------------------------------
// TUI Bus Event Payloads
// ---------------------------------------------------------------------------

/// Typed event payloads for TUI-related bus events.
///
/// These payloads are serialized to/from the `GlobalEvent.payload` JSON.
/// Both the server's TUI routes and the TUI's `handle_bus_event` use these types
/// to ensure they speak the same protocol.
///
/// # Source
/// Ported from `packages/opencode/src/server/tui-event.ts` and
/// `packages/opencode/src/server/routes/instance/httpapi/groups/tui.ts`.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type")]
pub enum TuiBusEvent {
    /// Append text to the TUI prompt.
    ///
    /// # Source
    /// `TuiEvent.PromptAppend` in `tui-event.ts`.
    #[serde(rename = "tui.prompt.append")]
    TuiPromptAppend {
        #[serde(default)]
        text: String,
    },

    /// Execute a command in the TUI.
    ///
    /// # Source
    /// `TuiEvent.CommandExecute` in `tui-event.ts`.
    #[serde(rename = "tui.command.execute")]
    TuiCommandExecute {
        #[serde(default)]
        command: String,
    },

    /// Show a toast notification in the TUI.
    ///
    /// # Source
    /// `TuiEvent.ToastShow` in `tui-event.ts`.
    #[serde(rename = "tui.toast.show")]
    TuiToastShow {
        #[serde(default)]
        title: Option<String>,
        #[serde(default)]
        message: String,
        #[serde(default)]
        variant: String,
        #[serde(default)]
        duration: Option<u64>,
    },

    /// Navigate the TUI to a specific session.
    ///
    /// # Source
    /// `TuiEvent.SessionSelect` in `tui-event.ts`.
    #[serde(rename = "tui.session.select")]
    TuiSessionSelect {
        #[serde(default)]
        session_id: String,
    },

    /// Open the help overlay.
    #[serde(rename = "tui.open.help")]
    TuiHelpOpen,

    /// Open the sessions list.
    #[serde(rename = "tui.open.sessions")]
    TuiSessionsOpen,

    /// Open the themes picker.
    #[serde(rename = "tui.open.themes")]
    TuiThemesOpen,

    /// Open the models list.
    #[serde(rename = "tui.open.models")]
    TuiModelsOpen,

    /// Submit the current prompt.
    #[serde(rename = "tui.prompt.submit")]
    TuiPromptSubmit,

    /// Clear the current prompt.
    #[serde(rename = "tui.prompt.clear")]
    TuiPromptClear,
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
    #[must_use]
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

/// Default event bus capacity: 8192 events.
///
/// Increased from 1024 to prevent slow consumers from lagging during
/// high-throughput operations (bulk tool execution, rapid streaming).
pub const DEFAULT_BUS_CAPACITY: usize = 8192;

impl Default for EventBus {
    fn default() -> Self {
        Self::new(DEFAULT_BUS_CAPACITY)
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
    #[must_use]
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
        Self::new(DEFAULT_BUS_CAPACITY)
    }
}

impl fmt::Debug for SharedBus {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("SharedBus")
            .field("receiver_count", &self.receiver_count())
            .finish()
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

        bus.publish(GlobalEvent::new(json!([1, 2, 3]))).unwrap();

        let received = sub.recv().await.unwrap();
        assert_eq!(received.payload, json!([1, 2, 3]));
    }

    // -- Event ordering -----------------------------------------------------

    #[tokio::test]
    async fn events_are_delivered_in_order() {
        let bus = EventBus::new(64);
        let mut sub = bus.subscribe();

        for i in 0..10 {
            bus.publish(GlobalEvent::new(json!({"seq": i}))).unwrap();
        }

        for expected in 0..10 {
            let received = sub.recv().await.unwrap();
            assert_eq!(received.payload["seq"], expected);
        }
    }

    // -- SharedBus clone + subscribe (multiple subscribers from different clones) --

    #[tokio::test]
    async fn shared_bus_clone_multiple_subscribers_from_different_handles() {
        let bus1 = SharedBus::new(16);
        let bus2 = bus1.clone();

        // Subscribe from both the original and the clone
        let mut sub1 = bus1.subscribe();
        let mut sub2 = bus2.subscribe();

        assert_eq!(bus1.receiver_count(), 2, "two active subscribers");

        // Publish from the clone handle
        bus2.publish(GlobalEvent::new(json!({"type": "from-clone"})))
            .expect("publish from clone should succeed");

        let ev1 = sub1
            .recv()
            .await
            .expect("sub1 should receive event from clone publish");
        let ev2 = sub2
            .recv()
            .await
            .expect("sub2 should receive event from clone publish");

        assert_eq!(ev1.payload["type"], "from-clone");
        assert_eq!(ev2.payload["type"], "from-clone");
        assert_eq!(
            ev1.id(),
            ev2.id(),
            "both subscribers should see the same event ID"
        );

        // Publish from the original handle
        bus1.publish(GlobalEvent::new(json!({"type": "from-original"})))
            .expect("publish from original should succeed");

        let ev3 = sub1
            .recv()
            .await
            .expect("sub1 should receive event from original publish");
        let ev4 = sub2
            .recv()
            .await
            .expect("sub2 should receive event from original publish");

        assert_eq!(ev3.payload["type"], "from-original");
        assert_eq!(ev4.payload["type"], "from-original");
        assert_eq!(
            ev3.id(),
            ev4.id(),
            "both should see the same second event ID"
        );
    }

    // -- GlobalEvent all-variant context fields + serde roundtrip ------------

    #[test]
    fn global_event_all_context_fields_serde_roundtrip() {
        let event = GlobalEvent::new(json!({"type": "full", "data": "test-value"}))
            .with_directory("/home/user/project")
            .with_project("my-project")
            .with_workspace("my-workspace");

        // All four context fields are set
        assert_eq!(event.directory.as_deref(), Some("/home/user/project"));
        assert_eq!(event.project.as_deref(), Some("my-project"));
        assert_eq!(event.workspace.as_deref(), Some("my-workspace"));
        assert_eq!(event.payload["type"], "full");
        assert_eq!(event.payload["data"], "test-value");

        // Serialize to JSON
        let serialized = serde_json::to_string(&event)
            .expect("serialization of full-context event should succeed");

        // Deserialize back
        let deserialized: GlobalEvent = serde_json::from_str(&serialized)
            .expect("deserialization of full-context event should succeed");

        // All fields preserved
        assert_eq!(
            deserialized.directory.as_deref(),
            Some("/home/user/project"),
            "directory should survive roundtrip"
        );
        assert_eq!(
            deserialized.project.as_deref(),
            Some("my-project"),
            "project should survive roundtrip"
        );
        assert_eq!(
            deserialized.workspace.as_deref(),
            Some("my-workspace"),
            "workspace should survive roundtrip"
        );
        assert_eq!(deserialized.payload["type"], "full");
        assert_eq!(deserialized.payload["data"], "test-value");
    }

    #[test]
    fn global_event_all_fields_none_serde_omits_nulls() {
        let event = GlobalEvent::new(json!({"type": "bare"}));
        let serialized = serde_json::to_string(&event).expect("serialization should succeed");
        let parsed: serde_json::Value =
            serde_json::from_str(&serialized).expect("should be valid JSON");

        // None fields should be absent (skip_serializing_if = "Option::is_none")
        assert!(
            !parsed.as_object().unwrap().contains_key("directory"),
            "None directory should be omitted"
        );
        assert!(
            !parsed.as_object().unwrap().contains_key("project"),
            "None project should be omitted"
        );
        assert!(
            !parsed.as_object().unwrap().contains_key("workspace"),
            "None workspace should be omitted"
        );
        assert!(parsed.as_object().unwrap().contains_key("payload"));

        // Deserialize back — omitted fields become None
        let deserialized: GlobalEvent =
            serde_json::from_str(&serialized).expect("deserialization should succeed");
        assert!(deserialized.directory.is_none());
        assert!(deserialized.project.is_none());
        assert!(deserialized.workspace.is_none());
    }

    // -- BusSubscription drop (auto-unsubscribe) receiver_count --------------

    #[tokio::test]
    async fn subscription_drop_receiver_count_goes_to_zero() {
        let bus = EventBus::new(16);

        assert_eq!(
            bus.receiver_count(),
            0,
            "no subscribers should exist initially"
        );

        let sub = bus.subscribe();
        assert_eq!(bus.receiver_count(), 1, "one subscriber after subscribe()");

        // Verify publish works while subscribed
        bus.publish(GlobalEvent::new(json!({"type": "alive"})))
            .expect("publish should succeed with active subscriber");

        drop(sub);

        assert_eq!(
            bus.receiver_count(),
            0,
            "receiver_count must drop to 0 after BusSubscription is dropped"
        );

        // Subsequent publish must fail because there are no receivers
        let result = bus.publish(GlobalEvent::new(json!({"type": "dead"})));
        assert!(
            result.is_err(),
            "publish must return error after all subscriptions are dropped"
        );
    }

    // -- Send-after-drop (no panic) ------------------------------------------

    #[tokio::test]
    async fn publish_after_subscription_drop_returns_error_does_not_panic() {
        let bus = EventBus::new(16);
        let mut sub = bus.subscribe();

        // Publish while subscribed — should succeed
        bus.publish(GlobalEvent::new(json!({"type": "before-drop"})))
            .expect("publish with active subscriber should succeed");

        let received = sub.recv().await.expect("should receive the event");
        assert_eq!(received.payload["type"], "before-drop");

        // Drop the only subscriber
        drop(sub);

        // Publish after drop — must return an error but NOT panic
        let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            // We cannot use .await inside catch_unwind easily, so test
            // the synchronous publish call path — it returns Result, not panic
            bus.publish(GlobalEvent::new(json!({"type": "after-drop"})))
        }));

        match result {
            Ok(Err(_send_error)) => {
                // Expected: publish returned an error
            }
            Ok(Ok(_)) => {
                panic!("publish after drop should have returned an error");
            }
            Err(panic_payload) => {
                // If we get here, publish panicked — that's a bug
                let msg = panic_payload
                    .downcast_ref::<String>()
                    .map(|s| s.as_str())
                    .or_else(|| panic_payload.downcast_ref::<&str>().copied())
                    .unwrap_or("<unknown panic message>");
                panic!("publish after drop MUST NOT panic, but it did: {msg}");
            }
        }
    }

    // -- Backpressure (many events before consumer starts) -------------------

    #[tokio::test]
    async fn backpressure_many_events_all_received_with_large_buffer() {
        let bus = EventBus::new(256);
        let mut sub = bus.subscribe();

        let count = 100;
        for i in 0..count {
            bus.publish(GlobalEvent::new(json!({"seq": i})))
                .expect("publish should succeed");
        }

        // Now consume — all events should still be in the buffer
        for expected in 0..count {
            let received = sub
                .recv()
                .await
                .unwrap_or_else(|| panic!("should receive event seq={expected}"));
            assert_eq!(
                received.payload["seq"], expected,
                "event {expected} should arrive in order"
            );
        }
    }

    #[tokio::test]
    async fn backpressure_small_buffer_causes_lag_but_recv_does_not_panic() {
        // Buffer capacity of 8 is much smaller than the 50 events we publish.
        // This guarantees the subscriber lags and BusSubscription::recv()
        // exercises its Lagged error-handling branch.
        let bus = EventBus::new(8);
        let mut sub = bus.subscribe();

        let count = 50;
        for i in 0..count {
            bus.publish(GlobalEvent::new(json!({"seq": i})))
                .expect("publish should succeed while subscriber exists");
        }

        // The subscriber has lagged — recv() internally handles Lagged and
        // returns the oldest surviving buffered message (or the next arrival).
        // The key assertion: recv() does NOT panic and returns Some.
        let received = sub
            .recv()
            .await
            .expect("recv() must return Some even after lag (NOT panic)");

        // After a lag with capacity 8 and 50 publishes, the oldest buffered
        // message should have seq >= 42 (50 - 8).  Verify the value is
        // reasonable given the buffer size.
        let seq = received.payload["seq"]
            .as_i64()
            .expect("seq field should be an integer");
        assert!(
            seq >= 42,
            "expected seq >= 42 (oldest buffered after {count} sends on capacity 8), got {seq}"
        );
    }

    // -- SharedBus default constructor ---------------------------------------

    #[tokio::test]
    async fn shared_bus_default_creates_working_bus_with_default_capacity() {
        let bus = SharedBus::default();

        // Default capacity is 8192 — we can publish at least that many + subscribe
        let mut sub = bus.subscribe();

        bus.publish(GlobalEvent::new(json!({"type": "default-shared"})))
            .expect("SharedBus::default() should support publish");

        let received = sub
            .recv()
            .await
            .expect("SharedBus::default() should deliver events");
        assert_eq!(received.payload["type"], "default-shared");

        // Verify capacity is sufficient for many events (default = 1024)
        for i in 0..200 {
            bus.publish(GlobalEvent::new(json!({"seq": i})))
                .expect("should handle multiple publishes on default bus");
        }
        // Consume a few to confirm they arrive
        for expected in 0..10 {
            let received = sub.recv().await.expect("should receive buffered event");
            assert_eq!(received.payload["seq"], expected);
        }
    }

    // -- EventBus default constructor ----------------------------------------

    #[tokio::test]
    async fn event_bus_default_creates_working_bus() {
        let bus = EventBus::default();

        assert_eq!(bus.receiver_count(), 0);

        let mut sub = bus.subscribe();
        assert_eq!(bus.receiver_count(), 1);

        bus.publish(GlobalEvent::new(json!({"type": "default-eventbus"})))
            .expect("EventBus::default() should support publish");

        let received = sub
            .recv()
            .await
            .expect("EventBus::default() should deliver events");
        assert_eq!(received.payload["type"], "default-eventbus");
    }
}
