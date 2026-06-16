//! Unique identifier generation — time-sortable, prefix-typed IDs.
//!
//! Ported from: `packages/core/src/id/id.ts`
//!
//! Every ID is composed as `{prefix}_{6_byte_hex}{14_char_base62}` (26 chars after prefix).
//! The hex segment encodes `(timestamp_ms * 0x1000 + counter)` in 6 bytes,
//! optionally bitwise-NOT'd for descending sort order. The base62 suffix provides
//! uniqueness within the same millisecond across independent generators.
//!
//! ```text
//! ses_019133b4e4a03K8gRsKQD9xxgT
//!  ^^^^ prefix
//!      ^^^^^^^^^^^^ 6-byte encoded time+counter (hex)
//!                   ^^^^^^^^^^^^^^ 14-char random base62 suffix
//! ```

use rand::RngCore;
use std::sync::Mutex;

// ---------------------------------------------------------------------------
// Public types
// ---------------------------------------------------------------------------

/// All valid ID prefixes.
///
/// Every entity type in OpenCode has a fixed 3-letter prefix so IDs are
/// self-describing and can be validated.
///
/// # Source
/// Ported from `packages/core/src/id/id.ts` lines 3–14 (`prefixes` const).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, serde::Serialize, serde::Deserialize)]
pub enum IdPrefix {
    /// Background job.
    Job,
    /// Bus event.
    Event,
    /// Session.
    Session,
    /// Chat message.
    Message,
    /// Permission request.
    Permission,
    /// Interactive question.
    Question,
    /// Streamed message part.
    Part,
    /// PTY instance.
    Pty,
    /// Tool invocation.
    Tool,
    /// Git worktree workspace.
    Workspace,
}

impl IdPrefix {
    /// Short string used in the ID's prefix segment.
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Job => "job",
            Self::Event => "evt",
            Self::Session => "ses",
            Self::Message => "msg",
            Self::Permission => "per",
            Self::Question => "que",
            Self::Part => "prt",
            Self::Pty => "pty",
            Self::Tool => "tool",
            Self::Workspace => "wrk",
        }
    }
}

/// Sort direction for generated IDs.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Direction {
    /// Oldest-first (chronological).
    Ascending,
    /// Newest-first (reverse chronological).
    Descending,
}

// ---------------------------------------------------------------------------
// State
// ---------------------------------------------------------------------------

/// Monotonic counter guard.
///
/// The TS source (Node.js event loop) uses unsynchronized module-level globals.
/// In Rust we serialise access through a `Mutex` so the counter stays correct
/// across concurrent ID generation on the multi-threaded tokio runtime.
static STATE: Mutex<(i64, u64)> = Mutex::new((0, 0));

/// Total encoded length (excluding prefix + underscore): 12 hex + 14 base62.
const LENGTH: usize = 26;

/// Multiplier applied to the millisecond timestamp before mixing in the counter.
/// This gives 12 bits (0x1000 = 4096) for the counter — enough for 4096 IDs
/// per millisecond before a collision in the time-encoded portion.
const TIMESTAMP_MULTIPLIER: u64 = 0x1000;

// ---------------------------------------------------------------------------
// Public API
// ---------------------------------------------------------------------------

/// Generate an ascending (oldest-first) ID.
///
/// If `given` is provided it is validated (must start with the correct prefix)
/// and returned as-is — this lets callers round-trip existing IDs.
///
/// # Source
/// Ported from `packages/core/src/id/id.ts` lines 22–24.
pub fn ascending(prefix: IdPrefix, given: Option<&str>) -> Result<String, IdError> {
    generate(prefix, Direction::Ascending, given)
}

/// Generate a descending (newest-first) ID.
///
/// # Source
/// Ported from `packages/core/src/id/id.ts` lines 26–28.
pub fn descending(prefix: IdPrefix, given: Option<&str>) -> Result<String, IdError> {
    generate(prefix, Direction::Descending, given)
}

/// Extract the Unix-millisecond timestamp embedded in an *ascending* ID.
///
/// Does **not** work on descending IDs — those have the time bits inverted.
///
/// # Source
/// Ported from `packages/core/src/id/id.ts` lines 72–78.
pub fn timestamp(id: &str) -> Result<i64, IdError> {
    let (_prefix, rest) = id.split_once('_').ok_or(IdError::Malformed)?;
    let hex_part = rest.get(..12).ok_or(IdError::Malformed)?;
    let encoded = u64::from_str_radix(hex_part, 16).map_err(|_| IdError::Malformed)?;
    Ok((encoded / TIMESTAMP_MULTIPLIER) as i64)
}

// ---------------------------------------------------------------------------
// Internal helpers
// ---------------------------------------------------------------------------

/// Shared dispatch for [`ascending`] and [`descending`].
///
/// # Source
/// Ported from `packages/core/src/id/id.ts` lines 30–38 (`generateID`).
fn generate(
    prefix: IdPrefix,
    direction: Direction,
    given: Option<&str>,
) -> Result<String, IdError> {
    if let Some(id) = given {
        let expected = prefix.as_str();
        if !id.starts_with(expected) {
            return Err(IdError::InvalidPrefix {
                expected: expected.to_owned(),
                given: id.to_owned(),
            });
        }
        return Ok(id.to_owned());
    }
    Ok(create(prefix.as_str(), direction, None))
}

/// Core ID creation.
///
/// Encodes `(timestamp_ms * 0x1000 + counter)` into 6 hex bytes,
/// appends a random base62 suffix, and optionally applies bitwise-NOT
/// for descending sort order.
///
/// # Source
/// Ported from `packages/core/src/id/id.ts` lines 51–69.
pub fn create(prefix_str: &str, direction: Direction, timestamp_ms: Option<i64>) -> String {
    let current = timestamp_ms.unwrap_or_else(now_ms);

    // Advance the monotonic counter, resetting when the timestamp changes.
    let counter = {
        let mut state = STATE.lock().expect("id state lock poisoned");
        if current != state.0 {
            state.0 = current;
            state.1 = 0;
        }
        state.1 = state.1.wrapping_add(1);
        state.1
    };

    let mut encoded: u64 = (current as u64)
        .wrapping_mul(TIMESTAMP_MULTIPLIER)
        .wrapping_add(counter);

    // Descending IDs invert the bit pattern so lexicographic sort is reversed.
    if direction == Direction::Descending {
        encoded = !encoded;
    }

    // Extract 6 bytes (48 bits) big-endian, matching the TS `Buffer` loop.
    let mut time_bytes = [0u8; 6];
    for (i, b) in time_bytes.iter_mut().enumerate() {
        *b = ((encoded >> (40 - 8 * i)) & 0xff) as u8;
    }

    let suffix = random_base62(LENGTH - 12); // 14 chars
    format!("{}_{}{}", prefix_str, hex::encode(time_bytes), suffix)
}

/// Current Unix time in milliseconds, or 0 if the clock is before the epoch
/// (should never happen on a real system).
fn now_ms() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_millis() as i64)
        .unwrap_or(0)
}

/// Generate a cryptographically random base62 string.
///
/// Uses the OS CSPRNG (`getrandom` via `OsRng`) — identical intent to
/// the TS `randomBytes` call.
///
/// # Source
/// Ported from `packages/core/src/id/id.ts` lines 41–49.
fn random_base62(length: usize) -> String {
    const CHARS: &[u8] = b"0123456789ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz";

    let mut buf = vec![0u8; length];
    rand::rngs::OsRng.fill_bytes(&mut buf);

    let mut result = String::with_capacity(length);
    for &byte in &buf {
        result.push(CHARS[(byte % 62) as usize] as char);
    }
    result
}

// ---------------------------------------------------------------------------
// Errors
// ---------------------------------------------------------------------------

/// Error returned by ID operations.
///
/// # Source
/// Ported from `packages/core/src/id/id.ts` line 36 (thrown `Error`).
#[derive(Debug, thiserror::Error)]
pub enum IdError {
    /// The provided ID does not start with the expected prefix.
    #[error("ID `{given}` does not start with `{expected}`")]
    InvalidPrefix {
        /// The prefix that was expected.
        expected: String,
        /// The ID that was actually provided.
        given: String,
    },

    /// The ID is structurally malformed (missing underscore, short hex segment,
    /// or invalid hex).
    #[error("malformed ID")]
    Malformed,
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    // -- Prefix ---------------------------------------------------------

    #[test]
    fn all_prefixes_are_3_chars() {
        let prefixes = [
            IdPrefix::Job,
            IdPrefix::Event,
            IdPrefix::Session,
            IdPrefix::Message,
            IdPrefix::Permission,
            IdPrefix::Question,
            IdPrefix::Part,
            IdPrefix::Pty,
            IdPrefix::Tool,
            IdPrefix::Workspace,
        ];
        for p in &prefixes {
            assert_eq!(p.as_str().len(), 3, "prefix {:?} should be 3 chars", p);
        }
    }

    // -- Generation ------------------------------------------------------

    #[test]
    fn ascending_creates_valid_id() {
        let id = ascending(IdPrefix::Session, None).unwrap();
        assert!(id.starts_with("ses_"), "got: {id}");
        // 3 prefix + '_' + 12 hex + 14 base62
        assert_eq!(id.len(), 3 + 1 + 12 + 14, "got len {}: {id}", id.len());
    }

    #[test]
    fn descending_creates_valid_id() {
        let id = descending(IdPrefix::Message, None).unwrap();
        assert!(id.starts_with("msg_"), "got: {id}");
        assert_eq!(id.len(), 3 + 1 + 12 + 14);
    }

    #[test]
    fn ids_are_unique() {
        let mut seen = std::collections::HashSet::new();
        for _ in 0..100 {
            let id = ascending(IdPrefix::Job, None).unwrap();
            assert!(seen.insert(id), "duplicate ID generated");
        }
    }

    #[test]
    fn descending_ids_sort_reversed() {
        let a = ascending(IdPrefix::Event, None).unwrap();
        std::thread::sleep(std::time::Duration::from_millis(2));
        let b = ascending(IdPrefix::Event, None).unwrap();

        let da = descending(IdPrefix::Event, None).unwrap();
        std::thread::sleep(std::time::Duration::from_millis(2));
        let db = descending(IdPrefix::Event, None).unwrap();

        // Ascending: lex sort matches chronological order
        assert!(a < b, "ascending: {a} should be < {b}");
        // Descending: lex sort is REVERSED (newest-first)
        assert!(da > db, "descending: {da} should be > {db}");
    }

    // -- Given round-trip ------------------------------------------------

    #[test]
    fn given_matching_prefix_passes_through() {
        let id = "ses_abc123def456K8gRsKQD9xxgT";
        let result = ascending(IdPrefix::Session, Some(id)).unwrap();
        assert_eq!(result, id);
    }

    #[test]
    fn given_wrong_prefix_errors() {
        let err = ascending(IdPrefix::Session, Some("msg_badprefixhere")).unwrap_err();
        match err {
            IdError::InvalidPrefix { expected, given } => {
                assert_eq!(expected, "ses");
                assert_eq!(given, "msg_badprefixhere");
            }
            _ => panic!("expected InvalidPrefix"),
        }
    }

    // -- Timestamp extraction --------------------------------------------

    #[test]
    fn round_trip_timestamp() {
        let ts = 1_700_000_000_000i64; // ~Nov 2023
        let id = create("ses", Direction::Ascending, Some(ts));
        let extracted = timestamp(&id).unwrap();
        // The encoded value is ts*0x1000 + counter, so division truncates
        // the counter bits. Should match exactly for counter < 0x1000.
        assert_eq!(extracted, ts);
    }

    #[test]
    fn timestamp_on_descending_does_not_round_trip() {
        let ts = 1_700_000_000_000i64;
        let id = create("ses", Direction::Descending, Some(ts));
        let extracted = timestamp(&id).unwrap();
        // Descending IDs have the bit pattern inverted — extracted value
        // is NOT the original timestamp.
        assert_ne!(extracted, ts);
    }

    #[test]
    fn timestamp_malformed_errors() {
        assert!(matches!(
            timestamp("no_underscore").unwrap_err(),
            IdError::Malformed
        ));
        assert!(matches!(
            timestamp("ses_short").unwrap_err(),
            IdError::Malformed
        ));
        assert!(matches!(
            timestamp("ses_ZZZZZZZZZZZZ").unwrap_err(),
            IdError::Malformed
        ));
    }

    // -- create with explicit timestamp ----------------------------------

    #[test]
    fn create_respects_explicit_timestamp() {
        let a = create("job", Direction::Ascending, Some(1000));
        let b = create("job", Direction::Ascending, Some(2000));
        assert!(a < b, "{a} should be < {b}");
    }

    #[test]
    fn create_respects_direction() {
        let a = create("evt", Direction::Ascending, Some(1000));
        let d = create("evt", Direction::Descending, Some(1000));
        assert!(a < d, "ascending {a} should be < descending {d}");
    }

    // -- Counter deduplication -------------------------------------------

    #[test]
    fn same_timestamp_increments_counter() {
        let ts = Some(1_000_000i64);
        let a = create("wrk", Direction::Ascending, ts);
        let b = create("wrk", Direction::Ascending, ts);
        // Same timestamp + counter → different IDs
        assert_ne!(a, b);
    }
}
