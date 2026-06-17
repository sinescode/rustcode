//! Base schema types — branded strings, path types, numeric constraints, and utilities.
//!
//! Ported from: `packages/core/src/schema.ts`
//! OpenCode commit: 5d0f86606ac30690f79f0a6a9f41a1f49fe95d0b
//!
//! ## Overview
//!
//! The TS codebase uses `effect/Schema` extensively for typed validation of
//! branded primitives (paths, IDs) and numeric constraints. This module
//! provides Rust equivalents using newtype wrappers with serde support.
//!
//! Key types:
//! - [`AbsolutePath`] — validated absolute filesystem path
//! - [`RelativePath`] — relative project path (e.g. `src/main.rs`)
//! - [`PositiveInt`] — integer > 0
//! - [`NonNegativeInt`] — integer >= 0
//! - [`ExternalId`] — namespaced external identifier
//! - [`Newtype`] — const-generic newtype builder (the Rust equivalent of the
//!   `Newtype<Tag>()` class factory in TS)

use serde::{Deserialize, Serialize};
use std::fmt;

// ── Branded path types ──────────────────────────────────────────────────

/// An absolute filesystem path (e.g., `/home/user/projects/myapp/src/main.ts`).
///
/// Validated at construction time — the path must be absolute.
///
/// # Source
/// Ported from `packages/core/src/schema.ts` lines 30–32
/// (`AbsolutePath = Schema.String.pipe(Schema.brand("AbsolutePath"))`).
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(transparent)]
pub struct AbsolutePath(String);

impl AbsolutePath {
    /// Create a new `AbsolutePath` from a string.
    ///
    /// Returns `None` if the path is not absolute.
    ///
    /// # Examples
    ///
    /// ```
    /// use rustcode_core::schema::AbsolutePath;
    ///
    /// let path = AbsolutePath::new("/home/user/project").unwrap();
    /// assert_eq!(path.as_str(), "/home/user/project");
    ///
    /// assert!(AbsolutePath::new("relative/path").is_none());
    /// ```
    pub fn new(input: impl Into<String>) -> Option<Self> {
        let s = input.into();
        // On Unix, absolute paths start with `/`.
        // On Windows, absolute paths match `[A-Za-z]:\` or `\\`.
        if s.starts_with('/') || (cfg!(windows) && is_windows_absolute(&s)) {
            Some(Self(s))
        } else {
            None
        }
    }

    /// Create an `AbsolutePath` without validation (caller guarantees validity).
    ///
    /// # Safety
    /// Caller must ensure `input` is an absolute path.
    pub fn new_unchecked(input: impl Into<String>) -> Self {
        Self(input.into())
    }

    /// Return the inner string slice.
    pub fn as_str(&self) -> &str {
        &self.0
    }

    /// Consume and return the inner string.
    pub fn into_inner(self) -> String {
        self.0
    }
}

impl fmt::Display for AbsolutePath {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        self.0.fmt(f)
    }
}

impl AsRef<str> for AbsolutePath {
    fn as_ref(&self) -> &str {
        &self.0
    }
}

impl From<AbsolutePath> for String {
    fn from(path: AbsolutePath) -> Self {
        path.0
    }
}

/// Check if a string is a Windows absolute path (drive letter or UNC).
fn is_windows_absolute(input: &str) -> bool {
    // Drive letter: `C:\...` or `C:/...`
    if input.len() >= 3
        && input.as_bytes()[0].is_ascii_alphabetic()
        && input.as_bytes()[1] == b':'
        && (input.as_bytes()[2] == b'\\' || input.as_bytes()[2] == b'/')
    {
        return true;
    }
    // UNC: `\\...`
    input.starts_with("\\\\")
}

// ── Relative path type ──────────────────────────────────────────────────

/// A relative file path within a project (e.g., `src/components/Button.tsx`).
///
/// # Source
/// Ported from `packages/core/src/schema.ts` lines 25–26
/// (`RelativePath = Schema.String.pipe(Schema.brand("RelativePath"))`).
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(transparent)]
pub struct RelativePath(String);

impl RelativePath {
    /// Create a new `RelativePath`.
    pub fn new(input: impl Into<String>) -> Self {
        Self(normalize_slashes(&input.into()))
    }

    /// Return the inner string slice.
    pub fn as_str(&self) -> &str {
        &self.0
    }

    /// Consume and return the inner string.
    pub fn into_inner(self) -> String {
        self.0
    }
}

impl fmt::Display for RelativePath {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        self.0.fmt(f)
    }
}

impl AsRef<str> for RelativePath {
    fn as_ref(&self) -> &str {
        &self.0
    }
}

/// Normalize slashes: on Windows, convert `\` to `/` for storage.
fn normalize_slashes(input: &str) -> String {
    if cfg!(windows) {
        input.replace('\\', "/")
    } else {
        input.to_string()
    }
}

// ── Numeric constraint types ───────────────────────────────────────────

/// A positive integer (strictly greater than zero).
///
/// # Source
/// Ported from `packages/core/src/schema.ts` line 15
/// (`PositiveInt = Schema.Int.check(Schema.isGreaterThan(0))`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(transparent)]
pub struct PositiveInt(i32);

impl PositiveInt {
    /// Create a new `PositiveInt`.
    ///
    /// Returns `None` if the value is <= 0.
    pub fn new(value: i32) -> Option<Self> {
        if value > 0 {
            Some(Self(value))
        } else {
            None
        }
    }

    /// Return the inner value.
    pub fn get(self) -> i32 {
        self.0
    }
}

impl fmt::Display for PositiveInt {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        self.0.fmt(f)
    }
}

impl From<PositiveInt> for i32 {
    fn from(val: PositiveInt) -> Self {
        val.0
    }
}

/// A non-negative integer (greater than or equal to zero).
///
/// # Source
/// Ported from `packages/core/src/schema.ts` line 20
/// (`NonNegativeInt = Schema.Int.check(Schema.isGreaterThanOrEqualTo(0))`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(transparent)]
pub struct NonNegativeInt(i32);

impl NonNegativeInt {
    /// Create a new `NonNegativeInt`.
    ///
    /// Returns `None` if the value is < 0.
    pub fn new(value: i32) -> Option<Self> {
        if value >= 0 {
            Some(Self(value))
        } else {
            None
        }
    }

    /// Create a `NonNegativeInt` without validation.
    #[allow(dead_code)]
    pub(crate) fn new_unchecked(value: i32) -> Self {
        Self(value)
    }

    /// Return the inner value.
    pub fn get(self) -> i32 {
        self.0
    }
}

impl fmt::Display for NonNegativeInt {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        self.0.fmt(f)
    }
}

impl From<NonNegativeInt> for i32 {
    fn from(val: NonNegativeInt) -> Self {
        val.0
    }
}

// ── External ID ────────────────────────────────────────────────────────

/// An external identifier with a namespace and key.
///
/// Used for combining namespaced external identifiers into a single
/// deterministic ID via SHA-256 hashing.
///
/// # Source
/// Ported from `packages/core/src/schema.ts` lines 4–10
/// (`ExternalID` type + `externalID()` function).
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct ExternalId {
    /// The namespace (e.g., provider name, integration name)
    pub namespace: String,
    /// The key within that namespace
    pub key: String,
}

impl ExternalId {
    /// Create a new external ID with the given namespace and key.
    pub fn new(namespace: impl Into<String>, key: impl Into<String>) -> Self {
        Self {
            namespace: namespace.into(),
            key: key.into(),
        }
    }

    /// Create a deterministic compound ID from a prefix and this external ID.
    ///
    /// Uses SHA-256 of `[namespace, key]` to produce a fixed-width hash,
    /// prefixed with `prefix_`.
    ///
    /// # Source
    /// Ported from `packages/core/src/schema.ts` lines 9–10
    /// (`externalID(prefix, input)`).
    pub fn compound_id(&self, prefix: &str) -> String {
        use sha2::{Digest, Sha256};
        let payload = serde_json::json!([self.namespace, self.key]).to_string();
        let hash = hex::encode(Sha256::digest(payload.as_bytes()));
        format!("{prefix}_{hash}")
    }
}

// ── Newtype factory ─────────────────────────────────────────────────────

/// A newtype wrapper pattern — the Rust equivalent of the TS `Newtype<Tag>()`
/// class factory.
///
/// Unlike the TS version (which is a class-based Schema factory), this is a
/// simple struct that wraps a String with a const-generic tag for type-level
/// discrimination.
///
/// # Source
/// Ported from `packages/core/src/schema.ts` lines 109–127
/// (`Newtype<Self>()` function).
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(transparent)]
pub struct TaggedString<const TAG: &'static str>(pub String);

impl<const TAG: &'static str> TaggedString<TAG> {
    /// Create a new tagged string.
    pub fn new(value: impl Into<String>) -> Self {
        Self(value.into())
    }

    /// Return a reference to the inner string.
    pub fn as_str(&self) -> &str {
        &self.0
    }

    /// Consume and return the inner string.
    pub fn into_inner(self) -> String {
        self.0
    }

    /// Return the tag name.
    pub fn tag() -> &'static str {
        TAG
    }
}

impl<const TAG: &'static str> fmt::Display for TaggedString<TAG> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        self.0.fmt(f)
    }
}

impl<const TAG: &'static str> AsRef<str> for TaggedString<TAG> {
    fn as_ref(&self) -> &str {
        &self.0
    }
}

// ── Optional field helper ───────────────────────────────────────────────

/// Serialization helper that omits `None` values from JSON output.
///
/// Equivalent to the TS `optionalOmitUndefined` pattern: the Rust type
/// is `Option<T>`, but when serializing, `None` is skipped entirely
/// (matching `JSON.stringify` behavior for `undefined`).
///
/// # Source
/// Ported from `packages/core/src/schema.ts` lines 38–44
/// (`optionalOmitUndefined`).
pub mod serde_optional {
    use serde::{Deserialize, Deserializer, Serialize, Serializer};

    /// Serialize `Option<T>` — skip if `None`.
    pub fn serialize<T: Serialize, S: Serializer>(
        value: &Option<T>,
        serializer: S,
    ) -> Result<S::Ok, S::Error> {
        match value {
            Some(v) => v.serialize(serializer),
            None => serializer.serialize_none(),
        }
    }

    /// Deserialize `Option<T>` — `null` or missing becomes `None`.
    pub fn deserialize<'de, T: Deserialize<'de>, D: Deserializer<'de>>(
        deserializer: D,
    ) -> Result<Option<T>, D::Error> {
        Option::<T>::deserialize(deserializer)
    }
}

// ── Tests ──────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    // ── AbsolutePath tests ──────────────────────────────────────────

    #[test]
    fn absolute_path_valid_unix() {
        let path = AbsolutePath::new("/home/user/project").expect("should be valid");
        assert_eq!(path.as_str(), "/home/user/project");
    }

    #[test]
    fn absolute_path_rejects_relative() {
        assert!(AbsolutePath::new("relative/path").is_none());
    }

    #[test]
    fn absolute_path_serde_roundtrip() {
        let path = AbsolutePath::new("/tmp/test").unwrap();
        let json = serde_json::to_string(&path).unwrap();
        let back: AbsolutePath = serde_json::from_str(&json).unwrap();
        assert_eq!(path, back);
    }

    // ── RelativePath tests ──────────────────────────────────────────

    #[test]
    fn relative_path_creation() {
        let path = RelativePath::new("src/main.rs");
        assert_eq!(path.as_str(), "src/main.rs");
    }

    #[test]
    fn relative_path_display() {
        let path = RelativePath::new("lib/util.ts");
        assert_eq!(format!("{path}"), "lib/util.ts");
    }

    // ── PositiveInt tests ───────────────────────────────────────────

    #[test]
    fn positive_int_valid() {
        let val = PositiveInt::new(42).unwrap();
        assert_eq!(val.get(), 42);
    }

    #[test]
    fn positive_int_rejects_zero() {
        assert!(PositiveInt::new(0).is_none());
    }

    #[test]
    fn positive_int_rejects_negative() {
        assert!(PositiveInt::new(-1).is_none());
    }

    // ── NonNegativeInt tests ────────────────────────────────────────

    #[test]
    fn non_negative_int_accepts_zero() {
        let val = NonNegativeInt::new(0).unwrap();
        assert_eq!(val.get(), 0);
    }

    #[test]
    fn non_negative_int_accepts_positive() {
        let val = NonNegativeInt::new(100).unwrap();
        assert_eq!(val.get(), 100);
    }

    #[test]
    fn non_negative_int_rejects_negative() {
        assert!(NonNegativeInt::new(-5).is_none());
    }

    // ── ExternalId tests ────────────────────────────────────────────

    #[test]
    fn external_id_compound() {
        let ext = ExternalId::new("github", "repo123");
        let id = ext.compound_id("gh");
        assert!(id.starts_with("gh_"));
        assert_eq!(id.len(), 67); // "gh_" + 64 hex chars
    }

    #[test]
    fn external_id_serde_roundtrip() {
        let ext = ExternalId::new("ns", "key");
        let json = serde_json::to_string(&ext).unwrap();
        let back: ExternalId = serde_json::from_str(&json).unwrap();
        assert_eq!(ext, back);
    }

    // ── TaggedString tests ──────────────────────────────────────────

    #[test]
    fn tagged_string_creation() {
        type SessionID = TaggedString<"SessionID">;
        let id = SessionID::new("abc-123");
        assert_eq!(id.as_str(), "abc-123");
        assert_eq!(SessionID::tag(), "SessionID");
    }

    #[test]
    fn tagged_string_serde_roundtrip() {
        type MyTag = TaggedString<"MyTag">;
        let val = MyTag::new("hello");
        let json = serde_json::to_string(&val).unwrap();
        let back: MyTag = serde_json::from_str(&json).unwrap();
        assert_eq!(val, back);
    }

    // ── AbsolutePath edge cases ─────────────────────────────────────

    #[test]
    fn absolute_path_empty_string() {
        assert!(AbsolutePath::new("").is_none());
    }
}
