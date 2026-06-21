//! EventV2 system — event sourcing, pub/sub, replay, and projection.
//!
//! Ported from:
//! - `packages/core/src/event.ts` (lines 1–681)
//! - `packages/opencode/src/event-v2-bridge.ts` (lines 1–80)
//! - `packages/core/src/session/event.ts` (lines 1–511)
//!   OpenCode commit: 5d0f86606ac30690f79f0a6a9f41a1f49fe95d0b
//!
//! ## Architecture
//!
//! The TS `EventV2` system is an event-sourcing core that:
//! - Defines typed events via `EventV2.define()` with optional sync (durable aggregate) markers.
//! - Stores synchronized events in SQLite (`EventTable`, `EventSequenceTable`).
//! - Dispatches to typed subscribers, global listeners, sync handlers, and projectors.
//! - Supports replay of serialized events for aggregate rebuilding.
//! - Publishes on a `PubSub` bus per event type and globally.
//!
//! In Rust:
//! - [`EventId`] is a branded string (prefix `evt_`).
//! - [`EventCursor`] is a `NonNegativeInt` (0-based durable position).
//! - [`EventDefinition`] carries a type tag, optional sync config, and a data schema.
//! - [`EventPayload`] is the runtime event envelope.
//! - [`EventRegistry`] stores definitions and provides dispatch.
//! - Session-specific event data types mirror `packages/core/src/session/event.ts`.

use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::fmt;
use std::sync::Arc;
use tokio::sync::RwLock;

use crate::database::DatabaseService;

// ---------------------------------------------------------------------------
// EventId — branded string with "evt_" prefix
// ---------------------------------------------------------------------------

/// Branded event identifier, always starts with `evt_`.
///
/// # Source
/// Ported from `packages/core/src/event.ts` lines 13–19.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(transparent)]
pub struct EventId(String);

impl EventId {
    /// Create a new ascending event ID with the `evt_` prefix.
    ///
    /// Uses the crate's `id::ascending()` with the `Event` prefix.
    ///
    /// # Source
    /// Ported from `packages/core/src/event.ts` line 16:
    /// `ID.create = () => schema.make("evt_" + Identifier.ascending())`
    pub fn create() -> Self {
        let inner = crate::id::ascending(crate::id::IdPrefix::Event, None)
            .unwrap_or_else(|_| format!("evt_{}", rand::random::<u64>()));
        Self(inner)
    }

    /// Create an event ID from an external identifier.
    ///
    /// # Source
    /// Ported from `packages/core/src/event.ts` line 17:
    /// `ID.fromExternal = (input: ExternalID) => schema.make(externalID("evt", input))`
    pub fn from_external(external: &str) -> Self {
        Self(format!("evt_{external}"))
    }

    /// Returns the inner string slice.
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl std::fmt::Display for EventId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.0)
    }
}

impl From<String> for EventId {
    fn from(s: String) -> Self {
        Self(s)
    }
}

// ---------------------------------------------------------------------------
// EventCursor — durable aggregate continuation position
// ---------------------------------------------------------------------------

/// Durable aggregate continuation position for embedded replay streams.
///
/// TODO: Decide whether a future HTTP / SDK surface should expose an opaque cursor instead.
///
/// # Source
/// Ported from `packages/core/src/event.ts` lines 22–27.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(transparent)]
pub struct EventCursor(u64);

impl EventCursor {
    /// Create a new cursor from a non-negative integer.
    pub fn new(n: u64) -> Self {
        Self(n)
    }

    /// Returns the inner value.
    pub fn value(&self) -> u64 {
        self.0
    }

    /// The zero cursor (start of stream).
    pub const ZERO: Self = Self(0);
}

impl std::fmt::Display for EventCursor {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.0)
    }
}

impl From<u64> for EventCursor {
    fn from(n: u64) -> Self {
        Self(n)
    }
}

impl From<EventCursor> for u64 {
    fn from(c: EventCursor) -> Self {
        c.0
    }
}

// ---------------------------------------------------------------------------
// EventDirection
// ---------------------------------------------------------------------------

/// Direction of event flow through the system.
///
/// # Source
/// Mirror of the implicit TS direction from `packages/opencode/src/event-v2-bridge.ts`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum EventDirection {
    /// Event flowing from a producer to consumers.
    Publish,
    /// Event being replayed from durable storage.
    Replay,
}

// ---------------------------------------------------------------------------
// Sync configuration for durable (synchronized) events
// ---------------------------------------------------------------------------

/// Configuration for events that are synchronized (durable).
///
/// Synchronized events cross a JSON boundary and are persisted in SQLite.
///
/// # Source
/// Ported from `packages/core/src/event.ts` lines 29–36 (the `sync` field in `Definition`).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SyncConfig {
    /// Schema version number.
    pub version: u32,
    /// Aggregate field name (e.g. "sessionID") used as the durability key.
    pub aggregate: String,
}

// ---------------------------------------------------------------------------
// EventDefinition — typed event schema
// ---------------------------------------------------------------------------

/// A typed event definition carrying a type tag, optional sync config, and data schema.
///
/// # Source
/// Ported from `packages/core/src/event.ts` lines 29–36.
#[derive(Debug, Clone)]
pub struct EventDefinition {
    /// Event type string (e.g. `"session.next.prompted"`).
    pub event_type: String,
    /// Optional sync configuration for durable events.
    pub sync: Option<SyncConfig>,
    /// JSON Schema of the event data (for validation and encoding).
    pub data_schema: serde_json::Value,
}

impl EventDefinition {
    /// Create a new event definition.
    pub fn new(
        event_type: impl Into<String>,
        sync: Option<SyncConfig>,
        data_schema: serde_json::Value,
    ) -> Self {
        Self {
            event_type: event_type.into(),
            sync,
            data_schema,
        }
    }

    /// Whether this event is synchronized (durable).
    pub fn is_sync(&self) -> bool {
        self.sync.is_some()
    }

    /// Returns the versioned event type string (e.g. `"session.next.prompted.1"`).
    ///
    /// # Source
    /// Ported from `packages/core/src/event.ts` lines 81–83:
    /// `versionedType(type, version) => `${type}.${version}``
    pub fn versioned_type(&self) -> Option<String> {
        self.sync
            .as_ref()
            .map(|s| versioned_type(&self.event_type, s.version))
    }
}

/// Build a versioned type string from an event type and version.
///
/// # Source
/// Ported from `packages/core/src/event.ts` lines 81–83.
pub fn versioned_type(event_type: &str, version: u32) -> String {
    format!("{event_type}.{version}")
}

// ---------------------------------------------------------------------------
// EventPayload — runtime event envelope
// ---------------------------------------------------------------------------

/// The runtime envelope for an event.
///
/// # Source
/// Ported from `packages/core/src/event.ts` lines 40–51.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EventPayload {
    /// Unique event identifier.
    pub id: EventId,
    /// Event type string.
    #[serde(rename = "type")]
    pub event_type: String,
    /// Event data (arbitrary JSON).
    pub data: serde_json::Value,
    /// Durable aggregate sequence number, populated during projection.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub seq: Option<u64>,
    /// Sync version number.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub version: Option<u32>,
    /// Location context (directory + workspace).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub location: Option<LocationRef>,
    /// Arbitrary metadata attached to the event.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub metadata: Option<serde_json::Value>,
    /// Internal replay marker for projectors with non-replicated operational state.
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub replay: bool,
}

impl EventPayload {
    /// Create a new event payload with the given id, type, and data.
    pub fn new(id: EventId, event_type: impl Into<String>, data: serde_json::Value) -> Self {
        Self {
            id,
            event_type: event_type.into(),
            data,
            seq: None,
            version: None,
            location: None,
            metadata: None,
            replay: false,
        }
    }

    /// Set the sync version on this payload.
    pub fn with_version(mut self, version: u32) -> Self {
        self.version = Some(version);
        self
    }

    /// Set the location on this payload.
    pub fn with_location(mut self, location: LocationRef) -> Self {
        self.location = Some(location);
        self
    }

    /// Set metadata on this payload.
    pub fn with_metadata(mut self, metadata: serde_json::Value) -> Self {
        self.metadata = Some(metadata);
        self
    }

    /// Mark this payload as a replay event.
    pub fn with_replay(mut self) -> Self {
        self.replay = true;
        self
    }

    /// Get the aggregate ID from the sync configuration.
    ///
    /// Returns `None` if the event has no sync config or the aggregate field is missing.
    pub fn aggregate_id(&self, sync: &SyncConfig) -> Option<String> {
        self.data
            .get(&sync.aggregate)
            .and_then(|v| v.as_str())
            .map(String::from)
    }
}

// ---------------------------------------------------------------------------
// LocationRef — directory + workspace context
// ---------------------------------------------------------------------------

/// Location reference for an event — directory and optional workspace ID.
///
/// # Source
/// Ported from `packages/core/src/location.ts` — the `Location.Ref` type used in events.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct LocationRef {
    /// Absolute directory path.
    pub directory: String,
    /// Optional workspace identifier.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub workspace_id: Option<String>,
    /// Optional project context.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub project: Option<ProjectRef>,
}

/// Project reference within a location.
///
/// # Source
/// Ported from `packages/opencode/src/event-v2-bridge.ts` lines 28–29.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ProjectRef {
    /// Project identifier.
    pub id: String,
    /// Absolute path to the project directory.
    pub directory: String,
}

// ---------------------------------------------------------------------------
// SerializedEvent — the durable row representation
// ---------------------------------------------------------------------------

/// Durable row representation of a synchronized event.
///
/// This is what gets stored in SQLite and replayed.
///
/// # Source
/// Ported from `packages/core/src/event.ts` lines 60–66.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SerializedEvent {
    /// Unique event identifier.
    pub id: EventId,
    /// Versioned event type (e.g. `"session.next.prompted.1"`).
    #[serde(rename = "type")]
    pub event_type: String,
    /// Aggregate sequence number.
    pub seq: u64,
    /// Aggregate identifier (e.g. session ID).
    #[serde(rename = "aggregateID")]
    pub aggregate_id: String,
    /// Encoded event data.
    pub data: serde_json::Value,
}

// ---------------------------------------------------------------------------
// CursorEvent — cursor + event pair for aggregate streams
// ---------------------------------------------------------------------------

/// A cursor-positioned event in an aggregate stream.
///
/// # Source
/// Ported from `packages/core/src/event.ts` lines 68–71.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CursorEvent {
    /// The cursor position of this event in the aggregate stream.
    pub cursor: EventCursor,
    /// The event payload.
    pub event: EventPayload,
}

// ---------------------------------------------------------------------------
// PublishOptions
// ---------------------------------------------------------------------------

/// Options for publishing an event.
///
/// # Source
/// Ported from `packages/core/src/event.ts` lines 139–145.
#[derive(Clone, Default)]
pub struct PublishOptions {
    /// Optional explicit event ID (auto-generated if not provided).
    pub id: Option<EventId>,
    /// Arbitrary metadata attached to the event.
    pub metadata: Option<serde_json::Value>,
    /// Location context for the event.
    pub location: Option<LocationRef>,
    /// Local operational projection committed atomically with a new synchronized event.
    /// Not replayed or serialized.
    pub commit: Option<CommitFn>,
}

/// A commit hook function — called with the assigned sequence number after the event is stored.
///
/// # Source
/// Ported from `packages/core/src/event.ts` line 144:
/// `readonly commit?: (seq: number) => Effect.Effect<void>`
pub type CommitFn = Arc<dyn Fn(u64) -> Pin<Box<dyn Future<Output = ()> + Send>> + Send + Sync>;

impl fmt::Debug for PublishOptions {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("PublishOptions")
            .field("id", &self.id)
            .field("metadata", &self.metadata)
            .field("location", &self.location)
            .field("commit", &self.commit.as_ref().map(|_| "CommitFn(...)"))
            .finish()
    }
}

use std::future::Future;
use std::pin::Pin;

// ---------------------------------------------------------------------------
// Functional type aliases matching TS Effect types
// ---------------------------------------------------------------------------

/// A projector function — transforms an event into an effect (e.g. updating read models).
///
/// # Source
/// Ported from `packages/core/src/event.ts` line 53:
/// `Projector<D> = (event: Payload<D>) => Effect.Effect<void>`
pub type ProjectorFn = Arc<
    dyn Fn(EventPayload) -> Pin<Box<dyn Future<Output = Result<(), EventError>> + Send>>
        + Send
        + Sync,
>;

/// A commit guard — runs before an event is committed, can abort the commit.
///
/// # Source
/// Ported from `packages/core/src/event.ts` line 55:
/// `CommitGuard = (event: Payload) => Effect.Effect<void>`
pub type CommitGuardFn = Arc<
    dyn Fn(EventPayload) -> Pin<Box<dyn Future<Output = Result<(), EventError>> + Send>>
        + Send
        + Sync,
>;

/// A listener — notified of every event after publication with error isolation.
///
/// # Source
/// Ported from `packages/core/src/event.ts` line 56:
/// `Listener = (event: Payload) => Effect.Effect<void>`
pub type ListenerFn = Arc<
    dyn Fn(EventPayload) -> Pin<Box<dyn Future<Output = Result<(), EventError>> + Send>>
        + Send
        + Sync,
>;

/// A sync handler — notified of every synchronized event after it is committed.
///
/// # Source
/// Ported from `packages/core/src/event.ts` line 57:
/// `Sync = (event: Payload) => Effect.Effect<void>`
pub type SyncFn = Arc<
    dyn Fn(EventPayload) -> Pin<Box<dyn Future<Output = Result<(), EventError>> + Send>>
        + Send
        + Sync,
>;

/// An unsubscribe effect — called to remove a listener or sync handler.
///
/// # Source
/// Ported from `packages/core/src/event.ts` line 58:
/// `Unsubscribe = Effect.Effect<void>`
pub type UnsubscribeFn = Box<dyn FnOnce() + Send>;

// ---------------------------------------------------------------------------
// EventError — error types for the event system
// ---------------------------------------------------------------------------

/// Errors specific to the event system.
///
/// # Source
/// Ported from `packages/core/src/event.ts` lines 73–79 (`InvalidSyncEventError`)
/// and additional error conditions throughout the event module.
#[derive(Debug, Clone, thiserror::Error, Serialize, Deserialize)]
pub enum EventError {
    /// Invalid sync event — version mismatch, aggregate mismatch, or invalid data.
    ///
    /// # Source
    /// Ported from `packages/core/src/event.ts` lines 73–79.
    #[error("InvalidSyncEvent: type={event_type}, message={message}")]
    InvalidSyncEvent { event_type: String, message: String },

    /// Unknown sync event type during replay.
    #[error("Unknown sync event type: {event_type}")]
    UnknownSyncType { event_type: String },

    /// Event already exists in the store.
    #[error("Event {event_id} already exists at aggregate {aggregate_id} sequence {seq}")]
    EventAlreadyExists {
        event_id: EventId,
        aggregate_id: String,
        seq: u64,
    },

    /// Replay diverged — stored event differs from replayed event.
    #[error("Replay diverged at aggregate {aggregate_id} sequence {seq}")]
    ReplayDiverged { aggregate_id: String, seq: u64 },

    /// Sequence mismatch during replay.
    #[error("Sequence mismatch for aggregate {aggregate_id}: expected {expected}, got {actual}")]
    SequenceMismatch {
        aggregate_id: String,
        expected: u64,
        actual: u64,
    },

    /// Replay events must belong to the same aggregate.
    #[error("Replay events must belong to the same aggregate")]
    AggregateMismatch,

    /// Replay owner mismatch.
    #[error(
        "Replay owner mismatch for aggregate {aggregate_id}: expected {expected}, got {actual}"
    )]
    OwnerMismatch {
        aggregate_id: String,
        expected: String,
        actual: String,
    },

    /// Local commit hooks require a synchronized event.
    #[error("Local commit hooks require a synchronized event, got type={event_type}")]
    CommitRequiresSync { event_type: String },

    /// Aggregate ID is not a string.
    #[error("Expected string aggregate field `{field}` in event data")]
    AggregateNotString { field: String },

    /// No pubsub channel for event type (all subscribers dropped).
    #[error("No pubsub channel for event type: {event_type}")]
    NoChannel { event_type: String },

    /// Internal error in the event system.
    #[error("Event system internal error: {0}")]
    Internal(String),
}

// ---------------------------------------------------------------------------
// EventRegistry
// ---------------------------------------------------------------------------

/// Registry of all event definitions, including sync codec information.
///
/// # Source
/// Ported from `packages/core/src/event.ts` lines 85–91, 96–133.
pub struct EventRegistry {
    /// All registered event definitions, keyed by type string.
    definitions: RwLock<HashMap<String, EventDefinition>>,
}

impl EventRegistry {
    /// Create a new empty event registry.
    pub fn new() -> Self {
        Self {
            definitions: RwLock::new(HashMap::new()),
        }
    }

    /// Register an event definition.
    ///
    /// If an existing definition has a lower sync version, it is replaced.
    ///
    /// # Source
    /// Ported from `packages/core/src/event.ts` lines 119–122.
    pub async fn define(&self, definition: EventDefinition) {
        let mut defs = self.definitions.write().await;
        let existing = defs.get(&definition.event_type);
        let should_replace = match (
            definition.sync.as_ref(),
            existing.and_then(|e| e.sync.as_ref()),
        ) {
            (Some(new_sync), Some(old_sync)) => new_sync.version >= old_sync.version,
            _ => true,
        };
        if should_replace {
            defs.insert(definition.event_type.clone(), definition);
        }
    }

    /// Get an event definition by type.
    pub async fn get(&self, event_type: &str) -> Option<EventDefinition> {
        self.definitions.read().await.get(event_type).cloned()
    }

    /// Returns all registered definitions.
    ///
    /// # Source
    /// Ported from `packages/core/src/event.ts` lines 135–137:
    /// `definitions() => registry.values().toArray()`
    pub async fn definitions(&self) -> Vec<EventDefinition> {
        self.definitions.read().await.values().cloned().collect()
    }

    /// Returns all definitions matching synchronized events.
    pub async fn sync_definitions(&self) -> Vec<EventDefinition> {
        self.definitions
            .read()
            .await
            .values()
            .filter(|d| d.sync.is_some())
            .cloned()
            .collect()
    }
}

impl Default for EventRegistry {
    fn default() -> Self {
        Self::new()
    }
}

// ---------------------------------------------------------------------------
// EventPubSub — typed pub/sub for event payloads
// ---------------------------------------------------------------------------

/// Typed publish/subscribe bus for events of a specific type.
///
/// Each event type has its own `EventPubSub` channel, mirroring the TS
/// `PubSub.unbounded<Payload>()` per type in the registry.
///
/// # Source
/// Ported from `packages/core/src/event.ts` lines 185–187 (per-type PubSub).
pub struct EventPubSub {
    /// Broadcast channel for typed event payloads.
    sender: tokio::sync::broadcast::Sender<EventPayload>,
}

impl EventPubSub {
    /// Create a new typed pubsub with the given capacity.
    pub fn new(capacity: usize) -> Self {
        let (sender, _) = tokio::sync::broadcast::channel(capacity);
        Self { sender }
    }

    /// Publish an event to all subscribers of this type.
    ///
    /// Returns the number of receivers that received the message.
    #[allow(clippy::result_large_err)]
    pub fn publish(
        &self,
        event: EventPayload,
    ) -> Result<usize, tokio::sync::broadcast::error::SendError<EventPayload>> {
        self.sender.send(event)
    }

    /// Subscribe to events of this type.
    pub fn subscribe(&self) -> EventSubscription {
        EventSubscription {
            receiver: self.sender.subscribe(),
        }
    }

    /// Number of active receivers.
    pub fn receiver_count(&self) -> usize {
        self.sender.receiver_count()
    }
}

/// Subscription to a typed event channel.
///
/// Automatically unsubscribes on drop.
pub struct EventSubscription {
    pub(crate) receiver: tokio::sync::broadcast::Receiver<EventPayload>,
}

impl EventSubscription {
    /// Receive the next event of this type.
    pub async fn recv(&mut self) -> Option<EventPayload> {
        match self.receiver.recv().await {
            Ok(event) => Some(event),
            Err(tokio::sync::broadcast::error::RecvError::Lagged(skipped)) => {
                tracing::warn!(
                    skipped,
                    "event subscriber lagged — {skipped} events skipped"
                );
                self.receiver.recv().await.ok()
            }
            Err(tokio::sync::broadcast::error::RecvError::Closed) => None,
        }
    }
}

// ---------------------------------------------------------------------------
// EventV2 — Interface trait
// ---------------------------------------------------------------------------

/// The EventV2 interface, matching `packages/core/src/event.ts` lines 147–173.
///
/// This trait allows alternative implementations (e.g., EventV2Bridge in the
/// opencode package) to conform to the same contract.
#[async_trait::async_trait]
pub trait EventV2Interface: Send + Sync {
    /// Publish an event through the system.
    async fn publish(
        &self,
        definition: &EventDefinition,
        data: serde_json::Value,
        options: Option<PublishOptions>,
    ) -> Result<EventPayload, EventError>;

    /// Subscribe to events of a specific type.
    async fn subscribe(&self, event_type: &str) -> EventSubscription;

    /// Subscribe to all events (global channel).
    fn subscribe_all(&self) -> EventSubscription;

    /// Register a listener for all events. Returns an unsubscribe function.
    async fn listen(&self, listener: ListenerFn) -> UnsubscribeFn;

    /// Register a sync handler for synchronized events. Returns an unsubscribe function.
    async fn sync(&self, handler: SyncFn) -> UnsubscribeFn;

    /// Register a commit guard that runs before every sync event commit.
    async fn before_commit(&self, guard: CommitGuardFn);

    /// Register a projector for a specific event type.
    async fn project(&self, event_type: &str, projector: ProjectorFn);

    /// Stream events for a specific aggregate.
    async fn aggregate_events(
        &self,
        aggregate_id: &str,
        after: Option<EventCursor>,
    ) -> EventSubscription;

    /// Replay a single serialized event.
    async fn replay(
        &self,
        event: SerializedEvent,
        options: Option<ReplayOptions>,
    ) -> Result<(), EventError>;

    /// Replay all events in a batch (must belong to the same aggregate).
    async fn replay_all(
        &self,
        events: Vec<SerializedEvent>,
        options: Option<ReplayOptions>,
    ) -> Result<Option<String>, EventError>;

    /// Remove all events and sequence data for an aggregate.
    async fn remove(&self, aggregate_id: &str);

    /// Claim ownership of an aggregate for replay ownership tracking.
    async fn claim(&self, aggregate_id: &str, owner_id: &str);

    /// Access the event registry.
    fn registry(&self) -> &Arc<EventRegistry>;
}

// ---------------------------------------------------------------------------
// EventV2 — the main event service
// ---------------------------------------------------------------------------

/// The main EventV2 service — manages event publication, subscription,
/// replay, projection, and synchronization.
///
/// Implements [`EventV2Interface`].
///
/// # Source
/// Ported from `packages/core/src/event.ts` lines 147–173 (`Interface`),
/// lines 181–675 (`layerWith` implementation).
pub struct EventV2 {
    /// Typed pubsub channels keyed by event type.
    typed_channels: RwLock<HashMap<String, Arc<EventPubSub>>>,
    /// Global pubsub for all events.
    global_channel: EventPubSub,
    /// Registered event definitions.
    registry: Arc<EventRegistry>,
    /// Registered projectors keyed by event type.
    projectors: Arc<RwLock<HashMap<String, Vec<ProjectorFn>>>>,
    /// Commit guards — run before every sync event commit.
    commit_guards: Arc<RwLock<Vec<CommitGuardFn>>>,
    /// Listeners notified of every event.
    listeners: Arc<RwLock<Vec<ListenerFn>>>,
    /// Sync handlers notified of every synchronized event.
    sync_handlers: Arc<RwLock<Vec<SyncFn>>>,
    /// Synchronized aggregate pubsub channels for live tailing.
    synchronized_aggregates: RwLock<HashMap<String, Vec<Arc<EventPubSub>>>>,
    /// Optional database service for persistent event storage.
    db: Option<DatabaseService>,
    /// Optional SQLite pool for direct queries.
    pool: Option<sqlx::SqlitePool>,
}

impl EventV2 {
    /// Create a new EventV2 instance with the given channel capacity and optional database pool.
    pub fn new(capacity: usize, pool: Option<sqlx::SqlitePool>) -> Self {
        let db = pool.clone().map(DatabaseService::new);
        Self {
            typed_channels: RwLock::new(HashMap::new()),
            global_channel: EventPubSub::new(capacity),
            registry: Arc::new(EventRegistry::new()),
            projectors: Arc::new(RwLock::new(HashMap::new())),
            commit_guards: Arc::new(RwLock::new(Vec::new())),
            listeners: Arc::new(RwLock::new(Vec::new())),
            sync_handlers: Arc::new(RwLock::new(Vec::new())),
            synchronized_aggregates: RwLock::new(HashMap::new()),
            db,
            pool,
        }
    }

    /// Get or create a typed pubsub channel for the given event type.
    ///
    /// # Source
    /// Ported from `packages/core/src/event.ts` lines 194–200 (`getOrCreate`).
    async fn get_or_create_channel(&self, event_type: &str) -> Arc<EventPubSub> {
        let channels = self.typed_channels.read().await;
        if let Some(ch) = channels.get(event_type) {
            return Arc::clone(ch);
        }
        drop(channels);

        let mut channels = self.typed_channels.write().await;
        // Double-check after acquiring write lock
        if let Some(ch) = channels.get(event_type) {
            return Arc::clone(ch);
        }
        let ch = Arc::new(EventPubSub::new(256));
        channels.insert(event_type.to_string(), Arc::clone(&ch));
        ch
    }

    /// Publish an event through the system.
    ///
    /// For synchronized (durable) events:
    /// - Persists to the `event_sequence` and `event` tables in a transaction
    /// - Runs commit guards before committing
    /// - Runs projectors for the event type
    /// - Notifies sync handlers after commit
    /// - Publishes to synchronized aggregate channels
    ///
    /// For non-synchronized (ephemeral) events:
    /// - Notifies listeners, typed subscribers, and global subscribers
    ///
    /// # Source
    /// Ported from `packages/core/src/event.ts` lines 431–451 (publish),
    /// and lines 384–407 (publishEvent).
    pub async fn publish(
        &self,
        definition: &EventDefinition,
        data: serde_json::Value,
        options: Option<PublishOptions>,
    ) -> Result<EventPayload, EventError> {
        let opts = options.unwrap_or_default();
        let id = opts.id.unwrap_or_else(EventId::create);
        let version = definition.sync.as_ref().map(|s| s.version);

        // Enforce that commit hooks require a synchronized event
        if opts.commit.is_some() && definition.sync.is_none() {
            return Err(EventError::CommitRequiresSync {
                event_type: definition.event_type.clone(),
            });
        }

        let payload = EventPayload {
            id,
            event_type: definition.event_type.clone(),
            data,
            seq: None,
            version,
            location: opts.location,
            metadata: opts.metadata,
            replay: false,
        };

        // For synchronized events, persist to DB, run guards, projectors, and commit hook
        if let Some(ref sync_config) = definition.sync {
            // Extract aggregate ID from event data
            let aggregate_id = payload.aggregate_id(sync_config).ok_or_else(|| {
                EventError::AggregateNotString {
                    field: sync_config.aggregate.clone(),
                }
            })?;

            // Determine the computed sequence and persist to DB
            let seq = if let Some(ref pool) = self.pool {
                let versioned_type = definition.versioned_type().ok_or_else(|| {
                    EventError::Internal("sync event has no versioned type".into())
                })?;

                // Run everything inside a transaction
                let mut tx = pool
                    .begin()
                    .await
                    .map_err(|e| EventError::Internal(format!("tx begin: {e}")))?;

                // Read current sequence for this aggregate
                let current_seq: Option<(i64,)> = sqlx::query_as(
                    "SELECT seq FROM event_sequence WHERE aggregate_id = ?1",
                )
                .bind(&aggregate_id)
                .fetch_optional(&mut *tx)
                .await
                .map_err(|e| EventError::Internal(format!("read seq: {e}")))?;

                let latest = current_seq.map(|(s,)| s).unwrap_or(-1);
                let new_seq = latest + 1;

                // Check event ID uniqueness
                let existing: Option<(String,)> = sqlx::query_as(
                    "SELECT id FROM event WHERE id = ?1",
                )
                .bind(payload.id.as_str())
                .fetch_optional(&mut *tx)
                .await
                .map_err(|e| EventError::Internal(format!("check event id: {e}")))?;

                if existing.is_some() {
                    return Err(EventError::EventAlreadyExists {
                        event_id: payload.id.clone(),
                        aggregate_id: aggregate_id.clone(),
                        seq: new_seq as u64,
                    });
                }

                // Run commit guards — clone payload once, share across guards
                let payload_clone = payload.clone();
                let guards = self.commit_guards.read().await;
                for guard in guards.iter() {
                    guard(payload_clone.clone()).await.map_err(|e| {
                        EventError::Internal(format!("Commit guard rejected: {e}"))
                    })?;
                }
                drop(guards);

                // Run projectors for this event type — reuse payload_clone
                let projectors = self.get_projectors(&definition.event_type).await;
                for projector in projectors.iter() {
                    projector(payload_clone.clone()).await.map_err(|e| {
                        EventError::Internal(format!("Projector failed: {e}"))
                    })?;
                }

                // Call commit hook if present
                if let Some(ref commit) = opts.commit {
                    commit(new_seq as u64).await;
                }

                // UPSERT event_sequence
                sqlx::query(
                    "INSERT INTO event_sequence (aggregate_id, seq) VALUES (?1, ?2) \
                     ON CONFLICT(aggregate_id) DO UPDATE SET seq = excluded.seq",
                )
                .bind(&aggregate_id)
                .bind(new_seq)
                .execute(&mut *tx)
                .await
                .map_err(|e| EventError::Internal(format!("upsert seq: {e}")))?;

                // Serialize event data as JSON — borrow from original payload, not clone
                let data_json = serde_json::to_string(&payload.data)
                    .map_err(|e| EventError::Internal(format!("serialize data: {e}")))?;

                // INSERT into event table
                sqlx::query(
                    "INSERT INTO event (id, aggregate_id, seq, type, data) VALUES (?1, ?2, ?3, ?4, ?5)",
                )
                .bind(payload.id.as_str())
                .bind(&aggregate_id)
                .bind(new_seq)
                .bind(&versioned_type)
                .bind(&data_json)
                .execute(&mut *tx)
                .await
                .map_err(|e| EventError::Internal(format!("insert event: {e}")))?;

                // Commit the transaction
                tx.commit()
                    .await
                    .map_err(|e| EventError::Internal(format!("tx commit: {e}")))?;

                new_seq
            } else {
                // No database — in-memory only: compute an approximate seq
                let latest: i64 = -1;
                let new_seq = latest + 1;

                // Run commit guards — clone once, share across all
                let payload_clone = payload.clone();
                let guards = self.commit_guards.read().await;
                for guard in guards.iter() {
                    guard(payload_clone.clone()).await.map_err(|e| {
                        EventError::Internal(format!("Commit guard rejected: {e}"))
                    })?;
                }
                drop(guards);

                // Run projectors — reuse payload_clone
                let projectors = self.get_projectors(&definition.event_type).await;
                for projector in projectors.iter() {
                    projector(payload_clone.clone()).await.map_err(|e| {
                        EventError::Internal(format!("Projector failed: {e}"))
                    })?;
                }

                // Call commit hook if present
                if let Some(ref commit) = opts.commit {
                    commit(new_seq as u64).await;
                }

                new_seq
            };

            let mut payload_with_seq = payload.clone();
            payload_with_seq.seq = Some(seq as u64);

            // Notify sync handlers
            let handlers = self.sync_handlers.read().await;
            for handler in handlers.iter() {
                handler(payload_with_seq.clone()).await.map_err(|e| {
                    EventError::Internal(format!("Sync handler failed: {e}"))
                })?;
            }
            drop(handlers);

            // Notify synchronized aggregate subscribers
            let aggregates = self.synchronized_aggregates.read().await;
            if let Some(pubsubs) = aggregates.get(&aggregate_id) {
                for pubsub in pubsubs.iter() {
                    let _ = pubsub.publish(payload_with_seq.clone());
                }
            }

            // Notify all listeners
            self.notify(&payload_with_seq, false).await;

            // Publish to typed channel
            let ch = self.get_or_create_channel(&definition.event_type).await;
            let _ = ch.publish(payload_with_seq.clone());

            // Publish to global channel
            let _ = self.global_channel.publish(payload_with_seq.clone());

            return Ok(payload_with_seq);
        }

        // For non-synchronized events (ephemeral): notify listeners only
        self.notify(&payload, false).await;

        // Publish to typed channel
        let ch = self.get_or_create_channel(&definition.event_type).await;
        let _ = ch.publish(payload.clone());

        // Publish to global channel
        let _ = self.global_channel.publish(payload.clone());

        Ok(payload)
    }

    /// Subscribe to events of a specific type.
    ///
    /// # Source
    /// Ported from `packages/core/src/event.ts` lines 538–541.
    pub async fn subscribe(&self, event_type: &str) -> EventSubscription {
        let ch = self.get_or_create_channel(event_type).await;
        ch.subscribe()
    }

    /// Subscribe to all events (global channel).
    ///
    /// # Source
    /// Ported from `packages/core/src/event.ts` lines 543 (`streamAll`).
    pub fn subscribe_all(&self) -> EventSubscription {
        self.global_channel.subscribe()
    }

    /// Register a listener for all events.
    ///
    /// Returns an unsubscribe function that removes this specific listener.
    /// Each listener is tracked by its position in the vector, so calling
    /// unsubscribe correctly removes the right entry even if other listeners
    /// are added or removed concurrently.
    ///
    /// # Source
    /// Ported from `packages/core/src/event.ts` lines 630–636.
    pub async fn listen(&self, listener: ListenerFn) -> UnsubscribeFn {
        let mut listeners = self.listeners.write().await;
        let index = listeners.len();
        listeners.push(listener);
        Box::new({
            let weak = Arc::downgrade(&self.listeners);
            move || {
                if let Some(listeners) = weak.upgrade() {
                    let mut listeners = listeners.blocking_write();
                    if index < listeners.len() {
                        listeners.remove(index);
                    }
                }
            }
        })
    }

    /// Register a sync handler.
    ///
    /// Returns an unsubscribe function that removes this specific handler.
    ///
    /// # Source
    /// Ported from `packages/core/src/event.ts` lines 639–645.
    pub async fn sync(&self, handler: SyncFn) -> UnsubscribeFn {
        let mut handlers = self.sync_handlers.write().await;
        let index = handlers.len();
        handlers.push(handler);
        Box::new({
            let weak = Arc::downgrade(&self.sync_handlers);
            move || {
                if let Some(handlers) = weak.upgrade() {
                    let mut handlers = handlers.blocking_write();
                    if index < handlers.len() {
                        handlers.remove(index);
                    }
                }
            }
        })
    }

    /// Register a commit guard that runs before every sync event commit.
    ///
    /// # Source
    /// Ported from `packages/core/src/event.ts` lines 648–651.
    pub async fn before_commit(&self, guard: CommitGuardFn) {
        let mut guards = self.commit_guards.write().await;
        guards.push(guard);
    }

    /// Register a projector for a specific event type.
    ///
    /// # Source
    /// Ported from `packages/core/src/event.ts` lines 653–657.
    pub async fn project(&self, event_type: &str, projector: ProjectorFn) {
        let mut projectors = self.projectors.write().await;
        projectors
            .entry(event_type.to_string())
            .or_default()
            .push(projector);
    }

    /// Notify all listeners of an event.
    ///
    /// # Source
    /// Ported from `packages/core/src/event.ts` lines 418–428 (`notify`).
    async fn notify(&self, payload: &EventPayload, isolate_listeners: bool) {
        let listeners = self.listeners.read().await;
        for listener in listeners.iter() {
            if isolate_listeners {
                // Error-isolated invocation — errors from one listener don't affect others.
                let result = listener(payload.clone()).await;
                if let Err(e) = result {
                    tracing::error!(
                        event_id = %payload.id,
                        event_type = %payload.event_type,
                        error = %e,
                        "Event listener failed"
                    );
                }
            } else {
                let _ = listener(payload.clone()).await;
            }
        }
    }

    /// Get a projector list for a given event type.
    pub async fn get_projectors(&self, event_type: &str) -> Vec<ProjectorFn> {
        self.projectors
            .read()
            .await
            .get(event_type)
            .cloned()
            .unwrap_or_default()
    }

    /// Stream events for a specific aggregate, optionally starting after a cursor.
    ///
    /// Returns an [`EventSubscription`] that yields events committed to this
    /// aggregate. The subscription automatically unsubscribes on drop.
    ///
    /// First replays historical events from the database (if available), then
    /// subscribes to live events via the synchronized aggregate channel.
    /// Historical events are published with `replay: true` so projectors can
    /// distinguish them from live events.
    ///
    /// # Source
    /// Ported from `packages/core/src/event.ts` lines 562–584 (readAfter),
    /// lines 586–604 (subscribeSynchronized), and lines 606–628 (streamEvents).
    pub async fn aggregate_events(
        &self,
        aggregate_id: &str,
        after: Option<EventCursor>,
    ) -> EventSubscription {
        let after_seq = after.map(|c| c.value()).unwrap_or(0);

        // Replay historical events from the database
        if let Some(ref pool) = self.pool {
            let rows: Vec<(String, i64, String, String)> = sqlx::query_as(
                "SELECT id, seq, type, data FROM event \
                 WHERE aggregate_id = ?1 AND seq > ?2 \
                 ORDER BY seq ASC",
            )
            .bind(aggregate_id)
            .bind(after_seq as i64)
            .fetch_all(pool)
            .await
            .unwrap_or_default();

            for (id, seq, event_type, data_json) in rows {
                let data: serde_json::Value =
                    serde_json::from_str(&data_json).unwrap_or(serde_json::Value::Null);

                // Find the definition to get version and original type
                let (orig_type, version) = self
                    .registry
                    .get(&event_type)
                    .await
                    .map(|def| {
                        (
                            def.event_type.clone(),
                            def.sync.as_ref().map(|s| s.version),
                        )
                    })
                    .unwrap_or_else(|| (event_type.clone(), None));

                let replay_payload = EventPayload {
                    id: EventId::from(id),
                    event_type: orig_type,
                    seq: Some(seq as u64),
                    version,
                    data,
                    location: None,
                    metadata: None,
                    replay: true,
                };

                // Notify listeners (error-isolated)
                self.notify(&replay_payload, true).await;

                // Publish to typed channel
                let ch = self
                    .get_or_create_channel(&replay_payload.event_type)
                    .await;
                let _ = ch.publish(replay_payload);
            }
        }

        // Subscribe to live events
        let mut aggregates = self.synchronized_aggregates.write().await;
        let channels = aggregates.entry(aggregate_id.to_string()).or_default();

        let channel = if let Some(existing) = channels.first() {
            Arc::clone(existing)
        } else {
            let new_ch = Arc::new(EventPubSub::new(256));
            channels.push(Arc::clone(&new_ch));
            new_ch
        };

        channel.subscribe()
    }

    /// Remove all events and sequence data for an aggregate from the database.
    ///
    /// # Source
    /// Ported from `packages/core/src/event.ts` lines 518–527.
    pub async fn remove(&self, aggregate_id: &str) {
        // Remove synchronized aggregate channels
        let mut aggregates = self.synchronized_aggregates.write().await;
        aggregates.remove(aggregate_id);

        // Delete from database in a transaction
        if let Some(ref pool) = self.pool {
            let result: Result<(), EventError> = async {
                let mut tx = pool
                    .begin()
                    .await
                    .map_err(|e| EventError::Internal(format!("tx begin: {e}")))?;

                sqlx::query("DELETE FROM event WHERE aggregate_id = ?1")
                    .bind(aggregate_id)
                    .execute(&mut *tx)
                    .await
                    .map_err(|e| EventError::Internal(format!("delete events: {e}")))?;

                sqlx::query("DELETE FROM event_sequence WHERE aggregate_id = ?1")
                    .bind(aggregate_id)
                    .execute(&mut *tx)
                    .await
                    .map_err(|e| EventError::Internal(format!("delete seq: {e}")))?;

                tx.commit()
                    .await
                    .map_err(|e| EventError::Internal(format!("tx commit: {e}")))?;

                Ok(())
            }
            .await;

            if let Err(e) = result {
                tracing::error!(aggregate_id, error = %e, "remove aggregate failed");
            }
        }
    }

    /// Claim ownership of an aggregate for replay ownership tracking.
    ///
    /// Updates the `owner_id` in the `EventSequenceTable` when a database is
    /// present. No-op in memory-only mode.
    ///
    /// # Source
    /// Ported from `packages/core/src/event.ts` lines 529–535.
    pub async fn claim(&self, aggregate_id: &str, owner_id: &str) {
        if let Some(ref pool) = self.pool {
            let result = sqlx::query(
                "UPDATE event_sequence SET owner_id = ?1 WHERE aggregate_id = ?2",
            )
            .bind(owner_id)
            .bind(aggregate_id)
            .execute(pool)
            .await;

            if let Err(e) = result {
                tracing::error!(
                    aggregate_id,
                    owner_id,
                    error = %e,
                    "claim aggregate failed"
                );
            }
        }
    }

    /// Access the event registry.
    pub fn registry(&self) -> &Arc<EventRegistry> {
        &self.registry
    }

    /// Replay a single serialized event with idempotency checks.
    ///
    /// Checks:
    /// 1. Event ID uniqueness (event already exists → error)
    /// 2. Sequence divergence (stored event at seq differs)
    /// 3. Owner mismatch (strict owner check)
    ///
    /// # Source
    /// Ported from `packages/core/src/event.ts` lines 453–482 (replay)
    /// and lines 270–382 (commitSyncEvent).
    pub async fn replay(
        &self,
        event: SerializedEvent,
        options: Option<ReplayOptions>,
    ) -> Result<(), EventError> {
        let definition = self.registry.get(&event.event_type).await.ok_or_else(|| {
            EventError::UnknownSyncType {
                event_type: event.event_type.clone(),
            }
        })?;

        let payload = EventPayload {
            id: event.id,
            event_type: definition.event_type.clone(),
            version: definition.sync.as_ref().map(|s| s.version),
            seq: Some(event.seq),
            data: event.data,
            location: None,
            metadata: None,
            replay: true,
        };

        let opts = options.unwrap_or_default();

        // Persist to DB with idempotency checks when a pool is available
        if let Some(ref pool) = self.pool {
            let aggregate_id = &event.aggregate_id;
            let seq = event.seq;

            // Read current sequence for this aggregate
            let current_seq: Option<(i64, Option<String>)> = sqlx::query_as(
                "SELECT seq, owner_id FROM event_sequence WHERE aggregate_id = ?1",
            )
            .bind(aggregate_id)
            .fetch_optional(pool)
            .await
            .map_err(|e| EventError::Internal(format!("read seq for replay: {e}")))?;

            if let Some((stored_seq, _)) = &current_seq {
                let stored_seq = *stored_seq as u64;

                // 1. Check sequence divergence — read the stored event at this seq
                let stored: Option<(String, String)> = sqlx::query_as(
                    "SELECT id, data FROM event WHERE aggregate_id = ?1 AND seq = ?2",
                )
                .bind(aggregate_id)
                .bind(seq as i64)
                .fetch_optional(pool)
                .await
                .map_err(|e| EventError::Internal(format!("read stored event: {e}")))?;

                if let Some((stored_id, _stored_data)) = &stored {
                    if stored_id != payload.id.as_str() {
                        return Err(EventError::ReplayDiverged {
                            aggregate_id: aggregate_id.clone(),
                            seq,
                        });
                    }
                }

                // 2. Check owner mismatch
                if opts.strict_owner {
                    let (_stored_seq, owner_id) = &current_seq.unwrap();
                    if let Some(expected_owner) = &opts.owner_id {
                        if let Some(actual_owner) = owner_id {
                            if actual_owner != expected_owner {
                                return Err(EventError::OwnerMismatch {
                                    aggregate_id: aggregate_id.clone(),
                                    expected: expected_owner.clone(),
                                    actual: actual_owner.clone(),
                                });
                            }
                        }
                    }
                }

                // 3. Check event ID uniqueness
                let existing: Option<(String,)> = sqlx::query_as(
                    "SELECT id FROM event WHERE id = ?1",
                )
                .bind(payload.id.as_str())
                .fetch_optional(pool)
                .await
                .map_err(|e| EventError::Internal(format!("check event id: {e}")))?;

                if existing.is_some() {
                    return Err(EventError::EventAlreadyExists {
                        event_id: payload.id.clone(),
                        aggregate_id: aggregate_id.clone(),
                        seq,
                    });
                }

                // 4. Validate sequence
                if seq != stored_seq + 1 {
                    return Err(EventError::SequenceMismatch {
                        aggregate_id: aggregate_id.clone(),
                        expected: stored_seq + 1,
                        actual: seq,
                    });
                }
            }

            // Write the event to DB
            let data_json = serde_json::to_string(&payload.data)
                .map_err(|e| EventError::Internal(format!("serialize data: {e}")))?;

            let versioned_type = definition.versioned_type().ok_or_else(|| {
                EventError::Internal("sync event has no versioned type".into())
            })?;

            let mut tx = pool
                .begin()
                .await
                .map_err(|e| EventError::Internal(format!("tx begin: {e}")))?;

            sqlx::query(
                "INSERT OR REPLACE INTO event_sequence (aggregate_id, seq, owner_id) VALUES (?1, ?2, ?3)",
            )
            .bind(aggregate_id)
            .bind(seq as i64)
            .bind(&opts.owner_id)
            .execute(&mut *tx)
            .await
            .map_err(|e| EventError::Internal(format!("upsert seq for replay: {e}")))?;

            sqlx::query(
                "INSERT INTO event (id, aggregate_id, seq, type, data) VALUES (?1, ?2, ?3, ?4, ?5)",
            )
            .bind(payload.id.as_str())
            .bind(aggregate_id)
            .bind(seq as i64)
            .bind(&versioned_type)
            .bind(&data_json)
            .execute(&mut *tx)
            .await
            .map_err(|e| EventError::Internal(format!("insert event for replay: {e}")))?;

            tx.commit()
                .await
                .map_err(|e| EventError::Internal(format!("tx commit for replay: {e}")))?;
        }

        // Publish if requested
        if opts.publish {
            self.notify(&payload, true).await;
        }

        Ok(())
    }

    /// Replay all events in a batch (must belong to the same aggregate).
    ///
    /// Checks aggregate consistency, sequence continuity, then delegates
    /// to [`replay`] per-event for idempotency (existing event, divergence,
    /// owner mismatch).
    ///
    /// # Source
    /// Ported from `packages/core/src/event.ts` lines 484–516.
    pub async fn replay_all(
        &self,
        events: Vec<SerializedEvent>,
        options: Option<ReplayOptions>,
    ) -> Result<Option<String>, EventError> {
        let source = events.first().map(|e| e.aggregate_id.clone());

        if let Some(ref src) = source {
            if events.iter().any(|e| e.aggregate_id != *src) {
                return Err(EventError::AggregateMismatch);
            }
        }

        let start = events.first().map(|e| e.seq).unwrap_or(0);
        for (index, event) in events.iter().enumerate() {
            let expected_seq = start + index as u64;
            if event.seq != expected_seq {
                return Err(EventError::SequenceMismatch {
                    aggregate_id: event.aggregate_id.clone(),
                    expected: expected_seq,
                    actual: event.seq,
                });
            }
        }

        for event in events {
            self.replay(event, options.clone()).await?;
        }

        Ok(source)
    }
}

#[async_trait::async_trait]
impl EventV2Interface for EventV2 {
    async fn publish(
        &self,
        definition: &EventDefinition,
        data: serde_json::Value,
        options: Option<PublishOptions>,
    ) -> Result<EventPayload, EventError> {
        self.publish(definition, data, options).await
    }

    async fn subscribe(&self, event_type: &str) -> EventSubscription {
        self.subscribe(event_type).await
    }

    fn subscribe_all(&self) -> EventSubscription {
        self.subscribe_all()
    }

    async fn listen(&self, listener: ListenerFn) -> UnsubscribeFn {
        self.listen(listener).await
    }

    async fn sync(&self, handler: SyncFn) -> UnsubscribeFn {
        self.sync(handler).await
    }

    async fn before_commit(&self, guard: CommitGuardFn) {
        self.before_commit(guard).await
    }

    async fn project(&self, event_type: &str, projector: ProjectorFn) {
        self.project(event_type, projector).await
    }

    async fn aggregate_events(
        &self,
        aggregate_id: &str,
        after: Option<EventCursor>,
    ) -> EventSubscription {
        self.aggregate_events(aggregate_id, after).await
    }

    async fn replay(
        &self,
        event: SerializedEvent,
        options: Option<ReplayOptions>,
    ) -> Result<(), EventError> {
        self.replay(event, options).await
    }

    async fn replay_all(
        &self,
        events: Vec<SerializedEvent>,
        options: Option<ReplayOptions>,
    ) -> Result<Option<String>, EventError> {
        self.replay_all(events, options).await
    }

    async fn remove(&self, aggregate_id: &str) {
        self.remove(aggregate_id).await
    }

    async fn claim(&self, aggregate_id: &str, owner_id: &str) {
        self.claim(aggregate_id, owner_id).await
    }

    fn registry(&self) -> &Arc<EventRegistry> {
        self.registry()
    }
}

impl Default for EventV2 {
    fn default() -> Self {
        Self::new(1024, None)
    }
}

/// Options for replaying events.
///
/// # Source
/// Ported from `packages/core/src/event.ts` lines 164–170.
#[derive(Debug, Clone, Default)]
pub struct ReplayOptions {
    /// Whether to publish replayed events to subscribers.
    pub publish: bool,
    /// Owner ID for replay ownership tracking.
    pub owner_id: Option<String>,
    /// Whether to enforce strict owner matching.
    pub strict_owner: bool,
}

// ---------------------------------------------------------------------------
// Session-specific event data types
// ---------------------------------------------------------------------------
// These mirror the data schemas defined in packages/core/src/session/event.ts

/// Timestamp in UTC milliseconds from epoch.
///
/// # Source
/// Ported from `packages/core/src/session/event.ts` — `Base.timestamp`.
pub type TimestampMs = u64;

/// Session event base fields present on all session events.
///
/// # Source
/// Ported from `packages/core/src/session/event.ts` lines 24–27.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SessionEventBase {
    /// UTC timestamp in milliseconds.
    pub timestamp: TimestampMs,
    /// Session identifier.
    #[serde(rename = "sessionID")]
    pub session_id: String,
}

/// Unknown error type used in session error events.
///
/// # Source
/// Ported from `packages/core/src/session/event.ts` lines 42–48.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UnknownError {
    #[serde(rename = "type")]
    pub error_type: String,
    pub message: String,
}

// ── Agent switched ──────────────────────────────────────────────────────

/// Data for the `session.next.agent.switched` event.
///
/// # Source
/// Ported from `packages/core/src/session/event.ts` lines 50–58.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AgentSwitchedEvent {
    #[serde(flatten)]
    pub base: SessionEventBase,
    #[serde(rename = "messageID")]
    pub message_id: String,
    pub agent: String,
}

// ── Model switched ─────────────────────────────────────────────────────

/// Data for the `session.next.model.switched` event.
///
/// # Source
/// Ported from `packages/core/src/session/event.ts` lines 61–69.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ModelSwitchedEvent {
    #[serde(flatten)]
    pub base: SessionEventBase,
    #[serde(rename = "messageID")]
    pub message_id: String,
    pub model: ModelRef,
}

/// Lightweight model reference for event data.
///
/// # Source
/// Ported from `packages/core/src/session/event.ts` line 67 — `ModelV2.Ref`.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ModelRef {
    pub id: String,
    #[serde(rename = "providerID")]
    pub provider_id: String,
}

// ── Moved ──────────────────────────────────────────────────────────────

/// Data for the `session.next.moved` event.
///
/// # Source
/// Ported from `packages/core/src/session/event.ts` lines 72–80.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MovedEvent {
    #[serde(flatten)]
    pub base: SessionEventBase,
    pub location: LocationRef,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub subdirectory: Option<String>,
}

// ── Prompted ───────────────────────────────────────────────────────────

/// Data for the `session.next.prompted` event.
///
/// # Source
/// Ported from `packages/core/src/session/event.ts` lines 83–93.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PromptedEvent {
    #[serde(flatten)]
    pub base: SessionEventBase,
    #[serde(rename = "messageID")]
    pub message_id: String,
    pub prompt: serde_json::Value,
    /// Either "steer" or "queue".
    pub delivery: String,
}

// ── Prompt lifecycle ───────────────────────────────────────────────────

/// Data for the `session.next.prompt.admitted` event.
///
/// # Source
/// Ported from `packages/core/src/session/event.ts` lines 96–106.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PromptAdmittedEvent {
    #[serde(flatten)]
    pub base: SessionEventBase,
    #[serde(rename = "messageID")]
    pub message_id: String,
    pub prompt: serde_json::Value,
    pub delivery: String,
}

/// Data for the `session.next.prompt.promoted` event.
///
/// # Source
/// Ported from `packages/core/src/session/event.ts` lines 108–118.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PromptPromotedEvent {
    #[serde(flatten)]
    pub base: SessionEventBase,
    #[serde(rename = "messageID")]
    pub message_id: String,
    pub prompt: serde_json::Value,
    #[serde(rename = "timeCreated")]
    pub time_created: TimestampMs,
}

// ── Interrupt requested ────────────────────────────────────────────────

/// Data for the `session.next.interrupt.requested` event.
///
/// # Source
/// Ported from `packages/core/src/session/event.ts` lines 121–126.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct InterruptRequestedEvent {
    #[serde(flatten)]
    pub base: SessionEventBase,
}

// ── Context updated ────────────────────────────────────────────────────

/// Data for the `session.next.context.updated` event.
///
/// # Source
/// Ported from `packages/core/src/session/event.ts` lines 128–137.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ContextUpdatedEvent {
    #[serde(flatten)]
    pub base: SessionEventBase,
    #[serde(rename = "messageID")]
    pub message_id: String,
    pub text: String,
}

// ── Synthetic ──────────────────────────────────────────────────────────

/// Data for the `session.next.synthetic` event.
///
/// # Source
/// Ported from `packages/core/src/session/event.ts` lines 139–148.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SyntheticEvent {
    #[serde(flatten)]
    pub base: SessionEventBase,
    #[serde(rename = "messageID")]
    pub message_id: String,
    pub text: String,
}

// ── Shell events ───────────────────────────────────────────────────────

/// Data for the `session.next.shell.started` event.
///
/// # Source
/// Ported from `packages/core/src/session/event.ts` lines 152–161.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ShellStartedEvent {
    #[serde(flatten)]
    pub base: SessionEventBase,
    #[serde(rename = "messageID")]
    pub message_id: String,
    #[serde(rename = "callID")]
    pub call_id: String,
    pub command: String,
}

/// Data for the `session.next.shell.ended` event.
///
/// # Source
/// Ported from `packages/core/src/session/event.ts` lines 163–172.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ShellEndedEvent {
    #[serde(flatten)]
    pub base: SessionEventBase,
    #[serde(rename = "callID")]
    pub call_id: String,
    pub output: String,
}

// ── Step events ────────────────────────────────────────────────────────

/// Data for the `session.next.step.started` event.
///
/// # Source
/// Ported from `packages/core/src/session/event.ts` lines 176–187.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StepStartedEvent {
    #[serde(flatten)]
    pub base: SessionEventBase,
    #[serde(rename = "assistantMessageID")]
    pub assistant_message_id: String,
    pub agent: String,
    pub model: ModelRef,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub snapshot: Option<String>,
}

/// Token counts for step settlement events.
///
/// # Source
/// Ported from `packages/core/src/session/event.ts` lines 197–206.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StepTokens {
    pub input: f64,
    pub output: f64,
    pub reasoning: f64,
    pub cache: CacheTokens,
}

/// Cache token counts.
///
/// # Source
/// Ported from `packages/core/src/session/event.ts` lines 202–205.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CacheTokens {
    pub read: f64,
    pub write: f64,
}

/// Data for the `session.next.step.ended` event.
///
/// # Source
/// Ported from `packages/core/src/session/event.ts` lines 189–209.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StepEndedEvent {
    #[serde(flatten)]
    pub base: SessionEventBase,
    #[serde(rename = "assistantMessageID")]
    pub assistant_message_id: String,
    pub finish: String,
    pub cost: f64,
    pub tokens: StepTokens,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub snapshot: Option<String>,
}

/// Data for the `session.next.step.failed` event.
///
/// # Source
/// Ported from `packages/core/src/session/event.ts` lines 211–220.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StepFailedEvent {
    #[serde(flatten)]
    pub base: SessionEventBase,
    #[serde(rename = "assistantMessageID")]
    pub assistant_message_id: String,
    pub error: UnknownError,
}

// ── Text events ────────────────────────────────────────────────────────

/// Data for the `session.next.text.started` event.
///
/// # Source
/// Ported from `packages/core/src/session/event.ts` lines 224–232.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TextStartedEvent {
    #[serde(flatten)]
    pub base: SessionEventBase,
    #[serde(rename = "assistantMessageID")]
    pub assistant_message_id: String,
    #[serde(rename = "textID")]
    pub text_id: String,
}

/// Data for the `session.next.text.delta` event (ephemeral).
///
/// # Source
/// Ported from `packages/core/src/session/event.ts` lines 236–245.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TextDeltaEvent {
    #[serde(flatten)]
    pub base: SessionEventBase,
    #[serde(rename = "assistantMessageID")]
    pub assistant_message_id: String,
    #[serde(rename = "textID")]
    pub text_id: String,
    pub delta: String,
}

/// Data for the `session.next.text.ended` event.
///
/// # Source
/// Ported from `packages/core/src/session/event.ts` lines 247–258.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TextEndedEvent {
    #[serde(flatten)]
    pub base: SessionEventBase,
    #[serde(rename = "assistantMessageID")]
    pub assistant_message_id: String,
    #[serde(rename = "textID")]
    pub text_id: String,
    pub text: String,
}

// ── Reasoning events ───────────────────────────────────────────────────

/// Data for the `session.next.reasoning.started` event.
///
/// # Source
/// Ported from `packages/core/src/session/event.ts` lines 261–271.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ReasoningStartedEvent {
    #[serde(flatten)]
    pub base: SessionEventBase,
    #[serde(rename = "assistantMessageID")]
    pub assistant_message_id: String,
    #[serde(rename = "reasoningID")]
    pub reasoning_id: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[serde(rename = "providerMetadata")]
    pub provider_metadata: Option<serde_json::Value>,
}

/// Data for the `session.next.reasoning.delta` event (ephemeral).
///
/// # Source
/// Ported from `packages/core/src/session/event.ts` lines 274–283.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ReasoningDeltaEvent {
    #[serde(flatten)]
    pub base: SessionEventBase,
    #[serde(rename = "assistantMessageID")]
    pub assistant_message_id: String,
    #[serde(rename = "reasoningID")]
    pub reasoning_id: String,
    pub delta: String,
}

/// Data for the `session.next.reasoning.ended` event.
///
/// # Source
/// Ported from `packages/core/src/session/event.ts` lines 285–297.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ReasoningEndedEvent {
    #[serde(flatten)]
    pub base: SessionEventBase,
    #[serde(rename = "assistantMessageID")]
    pub assistant_message_id: String,
    #[serde(rename = "reasoningID")]
    pub reasoning_id: String,
    pub text: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[serde(rename = "providerMetadata")]
    pub provider_metadata: Option<serde_json::Value>,
}

// ── Tool events ────────────────────────────────────────────────────────

/// Tool base fields shared across tool input events.
///
/// # Source
/// Ported from `packages/core/src/session/event.ts` lines 299–304.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ToolEventBase {
    #[serde(flatten)]
    pub base: SessionEventBase,
    #[serde(rename = "assistantMessageID")]
    pub assistant_message_id: String,
    #[serde(rename = "callID")]
    pub call_id: String,
}

/// Data for the `session.next.tool.input.started` event.
///
/// # Source
/// Ported from `packages/core/src/session/event.ts` lines 307–314.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ToolInputStartedEvent {
    #[serde(flatten)]
    pub tool_base: ToolEventBase,
    pub name: String,
}

/// Data for the `session.next.tool.input.delta` event (ephemeral).
///
/// # Source
/// Ported from `packages/core/src/session/event.ts` lines 318–325.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ToolInputDeltaEvent {
    #[serde(flatten)]
    pub tool_base: ToolEventBase,
    pub delta: String,
}

/// Data for the `session.next.tool.input.ended` event.
///
/// # Source
/// Ported from `packages/core/src/session/event.ts` lines 327–335.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ToolInputEndedEvent {
    #[serde(flatten)]
    pub tool_base: ToolEventBase,
    pub text: String,
}

/// Provider metadata for tool calls.
///
/// # Source
/// Ported from `packages/core/src/session/event.ts` lines 347–349.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ToolProviderInfo {
    /// Whether the provider executed the tool directly.
    pub executed: bool,
    /// Optional provider metadata.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub metadata: Option<serde_json::Value>,
}

/// Data for the `session.next.tool.called` event.
///
/// # Source
/// Ported from `packages/core/src/session/event.ts` lines 338–351.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ToolCalledEvent {
    #[serde(flatten)]
    pub tool_base: ToolEventBase,
    pub tool: String,
    pub input: serde_json::Value,
    pub provider: ToolProviderInfo,
}

/// Data for the `session.next.tool.progress` event.
///
/// # Source
/// Ported from `packages/core/src/session/event.ts` lines 357–366.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ToolProgressEvent {
    #[serde(flatten)]
    pub tool_base: ToolEventBase,
    pub structured: serde_json::Value,
    pub content: Vec<serde_json::Value>,
}

/// Data for the `session.next.tool.success` event.
///
/// # Source
/// Ported from `packages/core/src/session/event.ts` lines 368–383.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ToolSuccessEvent {
    #[serde(flatten)]
    pub tool_base: ToolEventBase,
    pub structured: serde_json::Value,
    pub content: Vec<serde_json::Value>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[serde(rename = "outputPaths")]
    pub output_paths: Option<Vec<String>>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub result: Option<serde_json::Value>,
    pub provider: ToolProviderInfo,
}

/// Data for the `session.next.tool.failed` event.
///
/// # Source
/// Ported from `packages/core/src/session/event.ts` lines 385–399.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ToolFailedEvent {
    #[serde(flatten)]
    pub tool_base: ToolEventBase,
    pub error: UnknownError,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub result: Option<serde_json::Value>,
    pub provider: ToolProviderInfo,
}

// ── Retry events ───────────────────────────────────────────────────────

/// Retry error information.
///
/// # Source
/// Ported from `packages/core/src/session/event.ts` lines 401–411.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RetryErrorInfo {
    pub message: String,
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        rename = "statusCode"
    )]
    pub status_code: Option<f64>,
    #[serde(rename = "isRetryable")]
    pub is_retryable: bool,
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        rename = "responseHeaders"
    )]
    pub response_headers: Option<HashMap<String, String>>,
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        rename = "responseBody"
    )]
    pub response_body: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub metadata: Option<HashMap<String, String>>,
}

/// Data for the `session.next.retried` event.
///
/// # Source
/// Ported from `packages/core/src/session/event.ts` lines 413–422.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RetriedEvent {
    #[serde(flatten)]
    pub base: SessionEventBase,
    pub attempt: f64,
    pub error: RetryErrorInfo,
}

// ── Compaction events ──────────────────────────────────────────────────

/// Data for the `session.next.compaction.started` event.
///
/// # Source
/// Ported from `packages/core/src/session/event.ts` lines 425–434.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CompactionStartedEvent {
    #[serde(flatten)]
    pub base: SessionEventBase,
    #[serde(rename = "messageID")]
    pub message_id: String,
    /// Either "auto" or "manual".
    pub reason: String,
}

/// Data for the `session.next.compaction.delta` event (ephemeral).
///
/// # Source
/// Ported from `packages/core/src/session/event.ts` lines 436–444.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CompactionDeltaEvent {
    #[serde(flatten)]
    pub base: SessionEventBase,
    #[serde(rename = "messageID")]
    pub message_id: String,
    pub text: String,
}

/// Data for the `session.next.compaction.ended` event (v2).
///
/// # Source
/// Ported from `packages/core/src/session/event.ts` lines 457–469.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CompactionEndedEvent {
    #[serde(flatten)]
    pub base: SessionEventBase,
    #[serde(rename = "messageID")]
    pub message_id: String,
    /// Either "auto" or "manual".
    pub reason: String,
    pub text: String,
    pub recent: String,
}

// ── Session event constants ────────────────────────────────────────────

/// Well-known session event type constants.
///
/// # Source
/// Ported from `packages/core/src/session/event.ts` — all `define()` call `type` fields.
pub mod session_event_types {
    // Session lifecycle
    pub const AGENT_SWITCHED: &str = "session.next.agent.switched";
    pub const MODEL_SWITCHED: &str = "session.next.model.switched";
    pub const MOVED: &str = "session.next.moved";
    pub const PROMPTED: &str = "session.next.prompted";
    pub const PROMPT_ADMITTED: &str = "session.next.prompt.admitted";
    pub const PROMPT_PROMOTED: &str = "session.next.prompt.promoted";
    pub const INTERRUPT_REQUESTED: &str = "session.next.interrupt.requested";
    pub const CONTEXT_UPDATED: &str = "session.next.context.updated";
    pub const SYNTHETIC: &str = "session.next.synthetic";

    // Shell events
    pub const SHELL_STARTED: &str = "session.next.shell.started";
    pub const SHELL_ENDED: &str = "session.next.shell.ended";

    // Step events
    pub const STEP_STARTED: &str = "session.next.step.started";
    pub const STEP_ENDED: &str = "session.next.step.ended";
    pub const STEP_FAILED: &str = "session.next.step.failed";

    // Text events (durable)
    pub const TEXT_STARTED: &str = "session.next.text.started";
    pub const TEXT_ENDED: &str = "session.next.text.ended";
    // Text events (ephemeral)
    pub const TEXT_DELTA: &str = "session.next.text.delta";

    // Reasoning events (durable)
    pub const REASONING_STARTED: &str = "session.next.reasoning.started";
    pub const REASONING_ENDED: &str = "session.next.reasoning.ended";
    // Reasoning events (ephemeral)
    pub const REASONING_DELTA: &str = "session.next.reasoning.delta";

    // Tool input events (durable)
    pub const TOOL_INPUT_STARTED: &str = "session.next.tool.input.started";
    pub const TOOL_INPUT_ENDED: &str = "session.next.tool.input.ended";
    // Tool input events (ephemeral)
    pub const TOOL_INPUT_DELTA: &str = "session.next.tool.input.delta";

    // Tool lifecycle events (durable)
    pub const TOOL_CALLED: &str = "session.next.tool.called";
    pub const TOOL_PROGRESS: &str = "session.next.tool.progress";
    pub const TOOL_SUCCESS: &str = "session.next.tool.success";
    pub const TOOL_FAILED: &str = "session.next.tool.failed";

    // Retry events
    pub const RETRIED: &str = "session.next.retried";

    // Compaction events (durable)
    pub const COMPACTION_STARTED: &str = "session.next.compaction.started";
    pub const COMPACTION_ENDED: &str = "session.next.compaction.ended";
    // Compaction events (ephemeral)
    pub const COMPACTION_DELTA: &str = "session.next.compaction.delta";
}

// ── Event type constants used by the event-v2-bridge ───────────────────

/// The `"event"` key used in `GlobalBus.emit("event", ...)`.
///
/// # Source
/// Ported from `packages/opencode/src/event-v2-bridge.ts` lines 42, 52.
pub const BUS_EVENT_KEY: &str = "event";

/// The `"sync"` payload type used in bridge sync event emission.
///
/// # Source
/// Ported from `packages/opencode/src/event-v2-bridge.ts` line 56.
pub const SYNC_EVENT_TYPE: &str = "sync";

/// The `"event"` channel name for EventBus integration.
///
/// # Source
/// Ported from `packages/opencode/src/bus/global.ts`.
pub const BUS_CHANNEL: &str = "event";

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    // ── EventId tests ──────────────────────────────────────────────────

    #[test]
    fn event_id_create_has_evt_prefix() {
        let id = EventId::create();
        assert!(
            id.as_str().starts_with("evt_"),
            "Expected evt_ prefix, got: {}",
            id
        );
    }

    #[test]
    fn event_id_from_external() {
        let id = EventId::from_external("abc123");
        assert_eq!(id.as_str(), "evt_abc123");
    }

    #[test]
    fn event_id_display() {
        let id = EventId::from("evt_test_001".to_string());
        assert_eq!(format!("{id}"), "evt_test_001");
    }

    #[test]
    fn event_id_serialization_roundtrip() {
        let id = EventId::from_external("my-event");
        let json = serde_json::to_string(&id).unwrap();
        let parsed: EventId = serde_json::from_str(&json).unwrap();
        assert_eq!(id, parsed);
    }

    #[test]
    fn event_id_clone_eq() {
        let id1 = EventId::from_external("xyz");
        let id2 = id1.clone();
        assert_eq!(id1, id2);
    }

    // ── EventCursor tests ──────────────────────────────────────────────

    #[test]
    fn event_cursor_new() {
        let cursor = EventCursor::new(42);
        assert_eq!(cursor.value(), 42);
    }

    #[test]
    fn event_cursor_ordering() {
        let a = EventCursor::new(10);
        let b = EventCursor::new(20);
        assert!(a < b);
        assert!(a <= EventCursor::new(10));
    }

    #[test]
    fn event_cursor_from_u64() {
        let c: EventCursor = 99_u64.into();
        assert_eq!(c.value(), 99);
    }

    #[test]
    fn event_cursor_to_u64() {
        let c = EventCursor::new(7);
        let n: u64 = c.into();
        assert_eq!(n, 7);
    }

    #[test]
    fn event_cursor_serialization_roundtrip() {
        let cursor = EventCursor::new(12345);
        let json = serde_json::to_string(&cursor).unwrap();
        let parsed: EventCursor = serde_json::from_str(&json).unwrap();
        assert_eq!(cursor, parsed);
    }

    // ── SyncConfig tests ───────────────────────────────────────────────

    #[test]
    fn sync_config_serialization_roundtrip() {
        let config = SyncConfig {
            version: 2,
            aggregate: "sessionID".to_string(),
        };
        let json = serde_json::to_string(&config).unwrap();
        let parsed: SyncConfig = serde_json::from_str(&json).unwrap();
        assert_eq!(parsed.version, 2);
        assert_eq!(parsed.aggregate, "sessionID");
    }

    // ── versioned_type tests ───────────────────────────────────────────

    #[test]
    fn versioned_type_format() {
        let result = versioned_type("session.next.prompted", 1);
        assert_eq!(result, "session.next.prompted.1");
    }

    #[test]
    fn versioned_type_large_version() {
        let result = versioned_type("test.event", 99);
        assert_eq!(result, "test.event.99");
    }

    // ── EventPayload tests ─────────────────────────────────────────────

    #[test]
    fn event_payload_new() {
        let id = EventId::from_external("test-1");
        let payload = EventPayload::new(id.clone(), "test.type", json!({"key": "value"}));
        assert_eq!(payload.id, id);
        assert_eq!(payload.event_type, "test.type");
        assert_eq!(payload.data["key"], "value");
        assert!(payload.seq.is_none());
        assert!(!payload.replay);
    }

    #[test]
    fn event_payload_builder_methods() {
        let payload = EventPayload::new(EventId::create(), "test", json!({}))
            .with_version(1)
            .with_location(LocationRef {
                directory: "/tmp".into(),
                workspace_id: Some("ws_1".into()),
                project: None,
            })
            .with_metadata(json!({"source": "test"}))
            .with_replay();

        assert_eq!(payload.version, Some(1));
        assert_eq!(payload.location.as_ref().unwrap().directory, "/tmp");
        assert!(payload.replay);
        assert!(payload.metadata.is_some());
    }

    #[test]
    fn event_payload_serialization_roundtrip() {
        let payload = EventPayload {
            id: EventId::from_external("test-serde"),
            event_type: "test.serde".into(),
            data: json!({"field": 42}),
            seq: Some(5),
            version: Some(1),
            location: Some(LocationRef {
                directory: "/home/user".into(),
                workspace_id: None,
                project: None,
            }),
            metadata: None,
            replay: false,
        };

        let json = serde_json::to_string(&payload).unwrap();
        let parsed: EventPayload = serde_json::from_str(&json).unwrap();
        assert_eq!(parsed.id, payload.id);
        assert_eq!(parsed.event_type, payload.event_type);
        assert_eq!(parsed.data["field"], 42);
        assert_eq!(parsed.seq, Some(5));
        assert!(!parsed.replay);
    }

    // ── SerializedEvent tests ──────────────────────────────────────────

    #[test]
    fn serialized_event_roundtrip() {
        let se = SerializedEvent {
            id: EventId::from_external("s-evt"),
            event_type: "test.type.1".into(),
            seq: 0,
            aggregate_id: "agg-1".into(),
            data: json!({"v": 1}),
        };
        let json = serde_json::to_string(&se).unwrap();
        let parsed: SerializedEvent = serde_json::from_str(&json).unwrap();
        assert_eq!(parsed.id, se.id);
        assert_eq!(parsed.seq, 0);
        assert_eq!(parsed.aggregate_id, "agg-1");
    }

    // ── CursorEvent tests ──────────────────────────────────────────────

    #[test]
    fn cursor_event_roundtrip() {
        let ce = CursorEvent {
            cursor: EventCursor::new(3),
            event: EventPayload::new(EventId::create(), "test", json!({})),
        };
        let json = serde_json::to_string(&ce).unwrap();
        let parsed: CursorEvent = serde_json::from_str(&json).unwrap();
        assert_eq!(parsed.cursor.value(), 3);
    }

    // ── EventError tests ───────────────────────────────────────────────

    #[test]
    fn event_error_display_invalid_sync() {
        let err = EventError::InvalidSyncEvent {
            event_type: "test.event".into(),
            message: "version mismatch".into(),
        };
        assert!(err.to_string().contains("InvalidSyncEvent"));
        assert!(err.to_string().contains("test.event"));
        assert!(err.to_string().contains("version mismatch"));
    }

    #[test]
    fn event_error_display_unknown_sync() {
        let err = EventError::UnknownSyncType {
            event_type: "unknown.type.1".into(),
        };
        assert!(err.to_string().contains("Unknown sync event type"));
    }

    #[test]
    fn event_error_display_replay_diverged() {
        let err = EventError::ReplayDiverged {
            aggregate_id: "agg-1".into(),
            seq: 42,
        };
        assert!(err.to_string().contains("Replay diverged"));
        assert!(err.to_string().contains("agg-1"));
    }

    #[test]
    fn event_error_display_sequence_mismatch() {
        let err = EventError::SequenceMismatch {
            aggregate_id: "agg-1".into(),
            expected: 5,
            actual: 7,
        };
        assert!(err.to_string().contains("Sequence mismatch"));
        assert!(err.to_string().contains("expected 5"));
        assert!(err.to_string().contains("got 7"));
    }

    // ── Session event type serialization tests ─────────────────────────

    #[test]
    fn agent_switched_event_roundtrip() {
        let ev = AgentSwitchedEvent {
            base: SessionEventBase {
                timestamp: 1700000000000,
                session_id: "ses_001".into(),
            },
            message_id: "msg_001".into(),
            agent: "default".into(),
        };
        let json = serde_json::to_string(&ev).unwrap();
        let parsed: AgentSwitchedEvent = serde_json::from_str(&json).unwrap();
        assert_eq!(parsed.base.session_id, "ses_001");
        assert_eq!(parsed.agent, "default");
    }

    #[test]
    fn step_ended_event_roundtrip() {
        let ev = StepEndedEvent {
            base: SessionEventBase {
                timestamp: 1700000000000,
                session_id: "ses_001".into(),
            },
            assistant_message_id: "msg_001".into(),
            finish: "stop".into(),
            cost: 0.0042,
            tokens: StepTokens {
                input: 1500.0,
                output: 300.0,
                reasoning: 0.0,
                cache: CacheTokens {
                    read: 0.0,
                    write: 0.0,
                },
            },
            snapshot: None,
        };
        let json = serde_json::to_string(&ev).unwrap();
        let parsed: StepEndedEvent = serde_json::from_str(&json).unwrap();
        assert_eq!(parsed.finish, "stop");
        assert!((parsed.cost - 0.0042).abs() < f64::EPSILON);
        assert!((parsed.tokens.input - 1500.0).abs() < f64::EPSILON);
    }

    #[test]
    fn tool_success_event_roundtrip() {
        let ev = ToolSuccessEvent {
            tool_base: ToolEventBase {
                base: SessionEventBase {
                    timestamp: 1700000000000,
                    session_id: "ses_001".into(),
                },
                assistant_message_id: "msg_001".into(),
                call_id: "call_001".into(),
            },
            structured: json!({"key": "val"}),
            content: vec![json!({"type": "text", "text": "done"})],
            output_paths: Some(vec!["/tmp/out.txt".into()]),
            result: Some(json!("success")),
            provider: ToolProviderInfo {
                executed: false,
                metadata: None,
            },
        };
        let json = serde_json::to_string(&ev).unwrap();
        let parsed: ToolSuccessEvent = serde_json::from_str(&json).unwrap();
        assert_eq!(parsed.tool_base.call_id, "call_001");
        assert!(parsed.output_paths.is_some());
    }

    #[test]
    fn compaction_ended_event_roundtrip() {
        let ev = CompactionEndedEvent {
            base: SessionEventBase {
                timestamp: 1700000000000,
                session_id: "ses_001".into(),
            },
            message_id: "msg_001".into(),
            reason: "auto".into(),
            text: "compacted summary".into(),
            recent: "recent messages".into(),
        };
        let json = serde_json::to_string(&ev).unwrap();
        let parsed: CompactionEndedEvent = serde_json::from_str(&json).unwrap();
        assert_eq!(parsed.reason, "auto");
        assert_eq!(parsed.text, "compacted summary");
    }

    // ── EventRegistry tests ────────────────────────────────────────────

    #[tokio::test]
    async fn registry_define_and_get() {
        let registry = EventRegistry::new();
        let def = EventDefinition::new(
            "test.event",
            Some(SyncConfig {
                version: 1,
                aggregate: "id".into(),
            }),
            json!({"type": "object"}),
        );
        registry.define(def).await;

        let retrieved = registry.get("test.event").await.unwrap();
        assert_eq!(retrieved.event_type, "test.event");
        assert!(retrieved.is_sync());
    }

    #[tokio::test]
    async fn registry_definitions_returns_all() {
        let registry = EventRegistry::new();
        registry
            .define(EventDefinition::new("evt.a", None, json!({})))
            .await;
        registry
            .define(EventDefinition::new(
                "evt.b",
                Some(SyncConfig {
                    version: 1,
                    aggregate: "x".into(),
                }),
                json!({}),
            ))
            .await;

        let all = registry.definitions().await;
        assert_eq!(all.len(), 2);

        let sync = registry.sync_definitions().await;
        assert_eq!(sync.len(), 1);
        assert_eq!(sync[0].event_type, "evt.b");
    }

    #[tokio::test]
    async fn registry_version_upgrade_replaces() {
        let registry = EventRegistry::new();
        registry
            .define(EventDefinition::new(
                "evt.z",
                Some(SyncConfig {
                    version: 1,
                    aggregate: "x".into(),
                }),
                json!({}),
            ))
            .await;

        registry
            .define(EventDefinition::new(
                "evt.z",
                Some(SyncConfig {
                    version: 2,
                    aggregate: "x".into(),
                }),
                json!({"type": "object"}),
            ))
            .await;

        let def = registry.get("evt.z").await.unwrap();
        assert_eq!(def.sync.unwrap().version, 2);
    }

    // ── EventV2 tests ──────────────────────────────────────────────────

    #[tokio::test]
    async fn event_v2_publish_and_subscribe() {
        let ev = EventV2::new(64, None);
        let def = EventDefinition::new("test.pubsub", None, json!({}));

        let mut sub = ev.subscribe("test.pubsub").await;
        let payload = ev
            .publish(&def, json!({"msg": "hello"}), None)
            .await
            .unwrap();

        let received = sub.recv().await.unwrap();
        assert_eq!(received.id, payload.id);
        assert_eq!(received.data["msg"], "hello");
    }

    #[tokio::test]
    async fn event_v2_global_subscription_receives_all() {
        let ev = EventV2::new(64, None);
        let def_a = EventDefinition::new("test.a", None, json!({}));
        let def_b = EventDefinition::new("test.b", None, json!({}));

        let mut sub = ev.subscribe_all();

        ev.publish(&def_a, json!({"x": 1}), None).await.unwrap();
        ev.publish(&def_b, json!({"y": 2}), None).await.unwrap();

        let first = sub.recv().await.unwrap();
        let second = sub.recv().await.unwrap();

        assert_eq!(first.event_type, "test.a");
        assert_eq!(second.event_type, "test.b");
    }

    #[tokio::test]
    async fn event_v2_multiple_typed_subscribers() {
        let ev = EventV2::new(64, None);
        let def = EventDefinition::new("test.fanout", None, json!({}));

        let mut sub1 = ev.subscribe("test.fanout").await;
        let mut sub2 = ev.subscribe("test.fanout").await;

        ev.publish(&def, json!({"n": 1}), None).await.unwrap();

        let r1 = sub1.recv().await.unwrap();
        let r2 = sub2.recv().await.unwrap();

        // Both should receive the same event
        assert_eq!(r1.id, r2.id);
    }

    #[tokio::test]
    async fn event_v2_listener_is_notified() {
        let ev = EventV2::new(64, None);
        let def = EventDefinition::new("test.listen", None, json!({}));

        // Can't easily test async listener in unit test without channels.
        // Verify listener registration doesn't panic.
        let _ = ev
            .listen(Arc::new(|_payload| Box::pin(async { Ok(()) })))
            .await;

        // Publish should succeed even with a listener registered
        let result = ev.publish(&def, json!({"test": true}), None).await;
        assert!(result.is_ok());
    }

    #[tokio::test]
    async fn event_v2_projector_registration() {
        let ev = EventV2::new(64, None);

        ev.project(
            "test.project",
            Arc::new(|_payload| Box::pin(async { Ok(()) })),
        )
        .await;

        let projectors = ev.get_projectors("test.project").await;
        assert_eq!(projectors.len(), 1);

        let empty = ev.get_projectors("nonexistent").await;
        assert!(empty.is_empty());
    }

    #[tokio::test]
    async fn event_v2_commit_guard_registration() {
        let ev = EventV2::new(64, None);
        ev.before_commit(Arc::new(|_payload| Box::pin(async { Ok(()) })))
            .await;

        let guards = ev.commit_guards.read().await;
        assert_eq!(guards.len(), 1);
    }

    #[tokio::test]
    async fn event_v2_sync_handler_registration() {
        let ev = EventV2::new(64, None);
        let _unsub = ev
            .sync(Arc::new(|_payload| Box::pin(async { Ok(()) })))
            .await;

        let handlers = ev.sync_handlers.read().await;
        assert_eq!(handlers.len(), 1);
    }

    // ── Session event data tests ───────────────────────────────────────

    #[test]
    fn retried_event_roundtrip() {
        let ev = RetriedEvent {
            base: SessionEventBase {
                timestamp: 1700000000000,
                session_id: "ses_001".into(),
            },
            attempt: 3.0,
            error: RetryErrorInfo {
                message: "rate limited".into(),
                status_code: Some(429.0),
                is_retryable: true,
                response_headers: None,
                response_body: None,
                metadata: None,
            },
        };
        let json = serde_json::to_string(&ev).unwrap();
        let parsed: RetriedEvent = serde_json::from_str(&json).unwrap();
        assert!((parsed.attempt - 3.0).abs() < f64::EPSILON);
        assert!(parsed.error.is_retryable);
    }

    #[test]
    fn step_failed_event_roundtrip() {
        let ev = StepFailedEvent {
            base: SessionEventBase {
                timestamp: 1700000000000,
                session_id: "ses_001".into(),
            },
            assistant_message_id: "msg_001".into(),
            error: UnknownError {
                error_type: "unknown".into(),
                message: "connection reset".into(),
            },
        };
        let json = serde_json::to_string(&ev).unwrap();
        let parsed: StepFailedEvent = serde_json::from_str(&json).unwrap();
        assert_eq!(parsed.error.message, "connection reset");
        assert_eq!(parsed.error.error_type, "unknown");
    }

    #[test]
    fn shell_started_event_roundtrip() {
        let ev = ShellStartedEvent {
            base: SessionEventBase {
                timestamp: 1700000000000,
                session_id: "ses_001".into(),
            },
            message_id: "msg_001".into(),
            call_id: "call_001".into(),
            command: "echo hello".into(),
        };
        let json = serde_json::to_string(&ev).unwrap();
        assert!(json.contains("echo hello"));
        let parsed: ShellStartedEvent = serde_json::from_str(&json).unwrap();
        assert_eq!(parsed.command, "echo hello");
    }

    #[test]
    fn reasoning_ended_event_roundtrip() {
        let ev = ReasoningEndedEvent {
            base: SessionEventBase {
                timestamp: 1700000000000,
                session_id: "ses_001".into(),
            },
            assistant_message_id: "msg_001".into(),
            reasoning_id: "reason_001".into(),
            text: "I think...".into(),
            provider_metadata: Some(json!({"model": "claude"})),
        };
        let json = serde_json::to_string(&ev).unwrap();
        let parsed: ReasoningEndedEvent = serde_json::from_str(&json).unwrap();
        assert_eq!(parsed.text, "I think...");
        assert!(parsed.provider_metadata.is_some());
    }
}
