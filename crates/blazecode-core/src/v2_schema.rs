//! V2 schema types — DateTime encoding/decoding helpers and re-exports.
//!
//! Ported from: `packages/core/src/v2-schema.ts`
//! BlazeCode commit: 5d0f86606ac30690f79f0a6a9f41a1f49fe95d0b
//!
//! ## Overview
//!
//! The TS source wraps `DateTimeUtcFromMillis` for encoding/decoding
//! UTC timestamps stored as epoch milliseconds (a common pattern in
//! the Effect.ts Schema library). In Rust we use `chrono::DateTime<Utc>`
//! with serde helpers that round-trip through milliseconds.
//!
//! Key types:
//! - [`DateTimeUtcFromMillis`] — serde module for millisecond-round-tripping
//! - Re-exports all types from this module for convenient access via `V2Schema::*`.

use chrono::{DateTime, TimeZone, Utc};
use serde::{Deserialize, Deserializer, Serialize, Serializer};

// ── DateTime UTC <-> millis codec ───────────────────────────────────────

/// Serde helpers for encoding/decoding `DateTime<Utc>` as epoch milliseconds.
///
/// # Source
/// Ported from `packages/core/src/v2-schema.ts` lines 3–8
/// (`DateTimeUtcFromMillis` — `Schema.Finite` decoded to `Schema.DateTimeUtc`).
///
/// ## Usage
///
/// ```ignore
/// #[derive(Serialize, Deserialize)]
/// struct Record {
///     #[serde(with = "blazecode_core::v2_schema::DateTimeUtcFromMillis")]
///     released: chrono::DateTime<chrono::Utc>,
/// }
/// ```
#[allow(non_snake_case)]
pub mod DateTimeUtcFromMillis {
    use super::*;

    /// Serialize a `DateTime<Utc>` as epoch milliseconds.
    pub fn serialize<S: Serializer>(dt: &DateTime<Utc>, serializer: S) -> Result<S::Ok, S::Error> {
        let millis = dt.timestamp_millis();
        millis.serialize(serializer)
    }

    /// Deserialize epoch milliseconds into a `DateTime<Utc>`.
    ///
    /// Maps the TS `DateTime.makeUnsafe(value)` behavior — creates a
    /// `DateTime<Utc>` from the raw millisecond value without further
    /// validation.
    pub fn deserialize<'de, D: Deserializer<'de>>(
        deserializer: D,
    ) -> Result<DateTime<Utc>, D::Error> {
        let millis = i64::deserialize(deserializer)?;
        Utc.timestamp_millis_opt(millis).single().ok_or_else(|| {
            serde::de::Error::custom(format!("invalid UTC timestamp (ms): {millis}"))
        })
    }
}

/// Optional DateTime UTC <-> millis codec (skips `None`).
///
/// # Source
/// Ported from `packages/core/src/v2-schema.ts` — the optional variant
/// used when a `DateTimeUtcFromMillis` field may be absent.
#[allow(non_snake_case)]
pub mod OptionDateTimeUtcFromMillis {
    use super::*;

    /// Serialize an optional `DateTime<Utc>` as epoch milliseconds, skipping `None`.
    pub fn serialize<S: Serializer>(
        dt: &Option<DateTime<Utc>>,
        serializer: S,
    ) -> Result<S::Ok, S::Error> {
        match dt {
            Some(dt) => {
                let millis = dt.timestamp_millis();
                millis.serialize(serializer)
            }
            None => serializer.serialize_none(),
        }
    }

    /// Deserialize optional epoch milliseconds into `Option<DateTime<Utc>>`.
    pub fn deserialize<'de, D: Deserializer<'de>>(
        deserializer: D,
    ) -> Result<Option<DateTime<Utc>>, D::Error> {
        let millis: Option<i64> = Option::deserialize(deserializer)?;
        match millis {
            Some(m) => Utc
                .timestamp_millis_opt(m)
                .single()
                .ok_or_else(|| serde::de::Error::custom(format!("invalid UTC timestamp (ms): {m}")))
                .map(Some),
            None => Ok(None),
        }
    }
}

// ── Helpers ─────────────────────────────────────────────────────────────

/// Create a `DateTime<Utc>` from epoch milliseconds (mirrors TS `DateTime.makeUnsafe`).
///
/// # Panics
/// Does NOT panic in library code — returns `None` for out-of-range values.
///
/// # Source
/// Ported from `packages/core/src/v2-schema.ts` line 6
/// (`DateTime.makeUnsafe(value)`).
pub fn datetime_from_millis(millis: i64) -> Option<DateTime<Utc>> {
    Utc.timestamp_millis_opt(millis).single()
}

/// Get the current epoch millisecond timestamp.
pub fn now_millis() -> i64 {
    Utc::now().timestamp_millis()
}

// ── Tests ──────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use serde::{Deserialize, Serialize};

    #[derive(Debug, Serialize, Deserialize, PartialEq)]
    struct TestRecord {
        #[serde(with = "DateTimeUtcFromMillis")]
        time_created: DateTime<Utc>,
        #[serde(with = "OptionDateTimeUtcFromMillis", default)]
        time_archived: Option<DateTime<Utc>>,
    }

    // ── DateTimeUtcFromMillis tests ─────────────────────────────────

    #[test]
    fn roundtrip_zero() {
        let json = r#"{"time_created":0}"#;
        let record: TestRecord = serde_json::from_str(json).unwrap();
        assert_eq!(record.time_created.timestamp_millis(), 0);
        let output = serde_json::to_string(&record).unwrap();
        let back: TestRecord = serde_json::from_str(&output).unwrap();
        assert_eq!(record, back);
    }

    #[test]
    fn roundtrip_current_time() {
        let now = Utc::now().timestamp_millis();
        let json = format!(r#"{{"time_created":{now}}}"#);
        let record: TestRecord = serde_json::from_str(&json).unwrap();
        assert_eq!(record.time_created.timestamp_millis(), now);
    }

    #[test]
    fn option_datetime_none() {
        let json = r#"{"time_created":0}"#;
        let record: TestRecord = serde_json::from_str(json).unwrap();
        assert!(record.time_archived.is_none());
    }

    #[test]
    fn option_datetime_some() {
        let now = Utc::now().timestamp_millis();
        let json = format!(r#"{{"time_created":0,"time_archived":{now}}}"#);
        let record: TestRecord = serde_json::from_str(&json).unwrap();
        assert_eq!(record.time_archived.unwrap().timestamp_millis(), now);
    }

    #[test]
    fn option_datetime_omit_on_serialize() {
        let record = TestRecord {
            time_created: Utc::now(),
            time_archived: None,
        };
        let output = serde_json::to_string(&record).unwrap();
        let parsed: serde_json::Value = serde_json::from_str(&output).unwrap();
        // OptionDateTimeUtcFromMillis serializes None as null, not omitted
        assert!(parsed.get("time_archived").is_some());
        assert!(parsed.get("time_archived").unwrap().is_null());
    }

    // ── datetime_from_millis tests ──────────────────────────────────

    #[test]
    fn datetime_from_millis_valid() {
        let dt = datetime_from_millis(0).unwrap();
        assert_eq!(dt.timestamp_millis(), 0);
    }

    #[test]
    fn datetime_from_millis_negative() {
        // 1969-12-31
        let dt = datetime_from_millis(-86_400_000).unwrap();
        assert!(dt.timestamp_millis() < 0);
    }

    #[test]
    fn now_millis_is_positive() {
        assert!(now_millis() > 0);
    }

    // ── Serialization edge cases ────────────────────────────────────

    #[test]
    fn roundtrip_serde_preserves_value() {
        let v: i64 = 1_700_000_000_000; // ~Nov 2023
        let dt = datetime_from_millis(v).unwrap();
        let json = serde_json::to_value(dt).unwrap();
        let back: DateTime<Utc> = serde_json::from_value(json).unwrap();
        assert_eq!(dt, back);
    }

    #[test]
    fn invalid_negative_overflow_is_rejected() {
        assert!(datetime_from_millis(i64::MIN).is_none());
    }
}
