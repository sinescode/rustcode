//! # Event types — core event definitions

use serde::{Deserialize, Serialize};
use uuid::Uuid;

/// Unique event identifier.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct EventId(Uuid);

impl EventId {
    /// Create a new random event ID.
    pub fn create() -> Self {
        Self(Uuid::new_v4())
    }
}

impl std::fmt::Display for EventId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.0)
    }
}

/// A single event payload.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EventPayload {
    /// Unique event ID.
    pub id: EventId,
    /// Event type string (e.g., "session.next.text.started").
    pub event_type: String,
    /// Event data.
    pub data: serde_json::Value,
    /// Sequence number (monotonic per aggregate).
    pub seq: Option<u64>,
    /// Event version.
    pub version: Option<u64>,
    /// Event source location.
    pub location: Option<String>,
    /// Additional metadata.
    pub metadata: Option<serde_json::Value>,
    /// Whether this is a replay event.
    pub replay: bool,
}

impl EventPayload {
    /// Create a new event payload.
    pub fn new(event_type: impl Into<String>, data: serde_json::Value) -> Self {
        Self {
            id: EventId::create(),
            event_type: event_type.into(),
            data,
            seq: None,
            version: None,
            location: None,
            metadata: None,
            replay: false,
        }
    }

    /// Set the sequence number.
    pub fn with_seq(mut self, seq: u64) -> Self {
        self.seq = Some(seq);
        self
    }

    /// Set the event version.
    pub fn with_version(mut self, version: u64) -> Self {
        self.version = Some(version);
        self
    }
}

/// Configuration for syncing events to external stores.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SyncConfig {
    /// Schema version.
    pub version: u64,
    /// Aggregate identifier.
    pub aggregate: String,
}

impl SyncConfig {
    /// Create a new sync configuration.
    pub fn new(version: u64, aggregate: impl Into<String>) -> Self {
        Self {
            version,
            aggregate: aggregate.into(),
        }
    }
}

/// Definition of a registered event type.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EventDefinition {
    /// Event type string.
    pub event_type: String,
    /// Optional sync configuration.
    pub sync: Option<SyncConfig>,
    /// JSON schema for event data validation.
    pub schema: serde_json::Value,
}

impl EventDefinition {
    /// Create a new event definition.
    pub fn new(
        event_type: impl Into<String>,
        sync: Option<SyncConfig>,
        schema: serde_json::Value,
    ) -> Self {
        Self {
            event_type: event_type.into(),
            sync,
            schema,
        }
    }
}

/// A filtered event stream that replays events matching a type pattern.
#[derive(Debug, Clone)]
pub struct EventStream {
    /// The event type to filter on.
    pub event_type: String,
    /// Offset sequence.
    pub seq_offset: u64,
    /// Maximum number of events to return.
    pub limit: Option<usize>,
}

/// Well-known lifecycle event type constants.
pub mod lifecycle {
    // Session lifecycle events
    /// Session conversation text started.
    pub const SESSION_TEXT_STARTED: &str = "session.next.text.started";
    /// Session conversation text ended.
    pub const SESSION_TEXT_ENDED: &str = "session.next.text.ended";
    /// Tool call started.
    pub const TOOL_CALL_STARTED: &str = "tool.call.started";
    /// Tool call completed.
    pub const TOOL_CALL_COMPLETED: &str = "tool.call.completed";
    /// Stream started.
    pub const STREAM_STARTED: &str = "stream.started";
    /// Stream ended.
    pub const STREAM_ENDED: &str = "stream.ended";
    /// Error event.
    pub const ERROR_OCCURRED: &str = "error.occurred";

    // Projector lifecycle events
    /// Projector registered.
    pub const PROJECTOR_REGISTERED: &str = "projector.registered";
    /// Projector triggered.
    pub const PROJECTOR_TRIGGERED: &str = "projector.triggered";
    /// Projector failed.
    pub const PROJECTOR_FAILED: &str = "projector.failed";

    // System events
    /// Session started.
    pub const SESSION_STARTED: &str = "session.started";
    /// Session ended.
    pub const SESSION_ENDED: &str = "session.ended";
    /// Compaction completed.
    pub const COMPACTION_COMPLETED: &str = "session.compaction.completed";
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_event_id_creation() {
        let id = EventId::create();
        let id2 = EventId::create();
        assert_ne!(id, id2);
    }

    #[test]
    fn test_event_payload_creation() {
        let payload = EventPayload::new("test.event", serde_json::json!({"key": "value"}));
        assert_eq!(payload.event_type, "test.event");
        assert_eq!(payload.data["key"], "value");
        assert!(!payload.replay);
    }

    #[test]
    fn test_event_payload_chaining() {
        let payload = EventPayload::new("test", serde_json::json!({}))
            .with_seq(42)
            .with_version(1);
        assert_eq!(payload.seq, Some(42));
        assert_eq!(payload.version, Some(1));
    }

    #[test]
    fn test_event_definition_creation() {
        let def = EventDefinition::new(
            "user.created",
            Some(SyncConfig::new(1, "user")),
            serde_json::json!({}),
        );
        assert_eq!(def.event_type, "user.created");
        assert!(def.sync.is_some());
    }
}
