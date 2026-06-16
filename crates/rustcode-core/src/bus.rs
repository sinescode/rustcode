//! Bus / event system for inter-module communication.
//!
//! Ported from: `packages/opencode/src/bus/global.ts`

use serde::{Deserialize, Serialize};
use tokio::sync::broadcast;

/// A global event published on the bus.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GlobalEvent {
    /// Optional directory context
    #[serde(skip_serializing_if = "Option::is_none")]
    pub directory: Option<String>,
    /// Optional project context
    #[serde(skip_serializing_if = "Option::is_none")]
    pub project: Option<String>,
    /// Optional workspace context
    #[serde(skip_serializing_if = "Option::is_none")]
    pub workspace: Option<String>,
    /// Event payload
    pub payload: serde_json::Value,
}

/// Event bus for broadcasting events across the system.
pub struct EventBus {
    sender: broadcast::Sender<GlobalEvent>,
}

impl EventBus {
    /// Create a new event bus with the given channel capacity.
    pub fn new(capacity: usize) -> Self {
        let (sender, _) = broadcast::channel(capacity);
        Self { sender }
    }

    /// Publish an event to all subscribers.
    ///
    /// # Errors
    /// Returns an error if there are no receivers.
    pub fn publish(
        &self,
        event: GlobalEvent,
    ) -> Result<usize, broadcast::error::SendError<GlobalEvent>> {
        self.sender.send(event)
    }

    /// Subscribe to events.
    pub fn subscribe(&self) -> broadcast::Receiver<GlobalEvent> {
        self.sender.subscribe()
    }
}

impl Default for EventBus {
    fn default() -> Self {
        Self::new(1024)
    }
}

/// Shared handle to the event bus.
#[derive(Clone)]
pub struct SharedBus {
    inner: std::sync::Arc<EventBus>,
}

impl SharedBus {
    /// Create a new shared bus.
    pub fn new(capacity: usize) -> Self {
        Self {
            inner: std::sync::Arc::new(EventBus::new(capacity)),
        }
    }

    /// Publish an event.
    ///
    /// # Errors
    /// Returns an error if there are no receivers.
    pub fn publish(
        &self,
        event: GlobalEvent,
    ) -> Result<usize, broadcast::error::SendError<GlobalEvent>> {
        self.inner.publish(event)
    }

    /// Subscribe to events.
    pub fn subscribe(&self) -> broadcast::Receiver<GlobalEvent> {
        self.inner.subscribe()
    }
}

impl Default for SharedBus {
    fn default() -> Self {
        Self::new(1024)
    }
}
