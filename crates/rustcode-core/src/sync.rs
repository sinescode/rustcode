//! Sync event identifiers — branded EventID type for sync events.
//!
//! Ported from: `packages/opencode/src/sync/schema.ts`
//! OpenCode commit: 5d0f86606ac30690f79f0a6a9f41a1f49fe95d0b
//!
//! The TS source defines a branded `EventID` type that must start with `evt_`.
//! This is used by the sync system to track event positions.

use crate::id;

/// A branded event identifier for sync operations.
///
/// Always starts with `evt_` followed by an ascending ID.
///
/// # Source
/// `packages/opencode/src/sync/schema.ts` lines 6–10.
#[derive(Debug, Clone, PartialEq, Eq, Hash, serde::Serialize, serde::Deserialize)]
pub struct EventID(String);

impl EventID {
    /// Generate a new ascending event ID.
    pub fn ascending() -> Self {
        Self(id::ascending(id::IdPrefix::Event, None).expect("Event ID generation should not fail"))
    }

    /// Create an EventID from a string, validating the `evt_` prefix.
    pub fn new(id: impl Into<String>) -> Option<Self> {
        let s = id.into();
        if s.starts_with("evt") {
            Some(Self(s))
        } else {
            None
        }
    }

    /// Returns the inner string.
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_event_id_ascending() {
        let id = EventID::ascending();
        assert!(id.as_str().starts_with("evt_"), "got: {}", id.as_str());
    }

    #[test]
    fn test_event_id_new_valid() {
        let id = EventID::new("evt_abc123");
        assert!(id.is_some());
        assert_eq!(id.unwrap().as_str(), "evt_abc123");
    }

    #[test]
    fn test_event_id_new_invalid_prefix() {
        let id = EventID::new("ses_abc123");
        assert!(id.is_none());
    }

    #[test]
    fn test_event_id_serde() {
        let id = EventID::ascending();
        let json = serde_json::to_string(&id).unwrap();
        let parsed: EventID = serde_json::from_str(&json).unwrap();
        assert_eq!(id, parsed);
    }
}
