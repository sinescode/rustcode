//! Credential types — OAuth tokens, API keys, and their persistence model.
//!
//! Ported from:
//! - `packages/core/src/credential.ts`
//! - `packages/core/src/credential/sql.ts`
//!
//! Credentials authenticate integrations. Two variants exist:
//! - **OAuth** — access/refresh token pair with expiry (e.g. GitHub, Google).
//! - **Key** — a single secret string (e.g. API key).
//!
//! Every stored credential carries an ascending time-sortable ID prefixed with
//! `cred_`, a reference to the integration it belongs to, a human-readable label,
//! and the credential value itself.

use serde::{Deserialize, Serialize};
use std::collections::HashMap;

// Re-export identifiers owned by the integration module so callers can
// reach them through `rustcode_core::credential::IntegrationId` etc.
pub use crate::integration::{IntegrationId, MethodId};

// ---------------------------------------------------------------------------
// Type aliases (branded strings in TS)
// ---------------------------------------------------------------------------

/// Credential identifier — branded string with `cred_` prefix followed by an
/// ascending time-sortable ID.
///
/// # Source
/// `packages/core/src/credential.ts` — `Credential.ID`
pub type CredentialId = String;

// ---------------------------------------------------------------------------
// Credential variants
// ---------------------------------------------------------------------------

/// OAuth credential with access/refresh tokens and expiration timestamp.
///
/// The `type` field is not stored on this struct directly — it is injected
/// by the [`CredentialInfo`] tagged-enum wrapper during serialization.
///
/// # Source
/// `packages/core/src/credential.ts` — `Credential.OAuth`
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct CredentialOAuth {
    /// Integration method that produced this credential.
    #[serde(rename = "methodID")]
    pub method_id: MethodId,
    /// Refresh token (long-lived, used to obtain new access tokens).
    pub refresh: String,
    /// Access token (short-lived bearer token).
    pub access: String,
    /// Expiration timestamp as a non-negative integer (milliseconds since epoch).
    pub expires: u64,
    /// Optional provider-specific metadata.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub metadata: Option<HashMap<String, String>>,
}

/// API-key credential with a single secret string.
///
/// The `type` field is not stored on this struct directly — it is injected
/// by the [`CredentialInfo`] tagged-enum wrapper during serialization.
///
/// # Source
/// `packages/core/src/credential.ts` — `Credential.Key`
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct CredentialKey {
    /// The secret key value.
    pub key: String,
    /// Optional provider-specific metadata.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub metadata: Option<HashMap<String, String>>,
}

/// Tagged union of all credential variants.
///
/// Serialized as `{"type": "oauth", ...}` or `{"type": "key", ...}`.
///
/// # Source
/// `packages/core/src/credential.ts` — `Credential.Info`
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(tag = "type")]
pub enum CredentialInfo {
    /// OAuth token pair credential.
    #[serde(rename = "oauth")]
    OAuth(CredentialOAuth),
    /// API key credential.
    #[serde(rename = "key")]
    Key(CredentialKey),
}

// ---------------------------------------------------------------------------
// Stored credential
// ---------------------------------------------------------------------------

/// Stored credential with ID, integration reference, label, and value.
///
/// # Source
/// `packages/core/src/credential.ts` — `Credential.Stored`
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct CredentialStored {
    /// Unique credential identifier (`cred_` prefix).
    pub id: CredentialId,
    /// Integration this credential belongs to.
    #[serde(rename = "integrationID")]
    pub integration_id: IntegrationId,
    /// Human-readable label for this credential (e.g. "default", "production").
    pub label: String,
    /// The credential value (OAuth or Key).
    pub value: CredentialInfo,
}

impl CredentialStored {
    /// Create a new stored credential with an auto-generated ascending ID.
    ///
    /// The ID is generated with the `cred_` prefix via the same time-sortable
    /// algorithm used throughout the codebase.
    ///
    /// # Source
    /// `packages/core/src/credential.ts` — `Credential.ID.create()`
    #[must_use]
    pub fn new(integration_id: IntegrationId, label: String, value: CredentialInfo) -> Self {
        let id = crate::id::create("cred", crate::id::Direction::Ascending, None);
        Self {
            id,
            integration_id,
            label,
            value,
        }
    }

    /// Create a stored credential with an explicit ID (for round-tripping
    /// existing credentials).
    #[must_use]
    pub fn with_id(
        id: CredentialId,
        integration_id: IntegrationId,
        label: String,
        value: CredentialInfo,
    ) -> Self {
        Self {
            id,
            integration_id,
            label,
            value,
        }
    }
}

// ---------------------------------------------------------------------------
// Database row representation
// ---------------------------------------------------------------------------

/// A row in the `credential` SQLite table.
///
/// Mirrors the drizzle-orm schema defined in `credential/sql.ts`. The `value`
/// column is stored as JSON text in SQLite and (de)serialized as
/// [`CredentialInfo`].
///
/// # Source
/// `packages/core/src/credential/sql.ts` — `CredentialTable`
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct CredentialTableRow {
    /// Primary key — credential ID.
    pub id: CredentialId,
    /// Integration this credential belongs to.
    pub integration_id: IntegrationId,
    /// Human-readable label.
    pub label: String,
    /// Credential payload (stored as JSON in SQLite).
    pub value: CredentialInfo,
    /// Optional connector reference.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub connector_id: Option<IntegrationId>,
    /// Optional method reference.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub method_id: Option<MethodId>,
    /// Whether this credential is the active one for its integration.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub active: Option<bool>,
    /// ISO 8601 creation timestamp.
    pub time_created: String,
    /// ISO 8601 last-update timestamp.
    pub time_updated: String,
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    // Helper to build a sample OAuth credential info.
    fn sample_oauth_info() -> CredentialInfo {
        CredentialInfo::OAuth(CredentialOAuth {
            method_id: "mth_abc123".into(),
            refresh: "rt_secret".into(),
            access: "at_secret".into(),
            expires: 1_700_000_000_000,
            metadata: None,
        })
    }

    // Helper to build a sample Key credential info.
    fn sample_key_info() -> CredentialInfo {
        CredentialInfo::Key(CredentialKey {
            key: "sk-1234567890abcdef".into(),
            metadata: None,
        })
    }

    // ── CredentialId construction ───────────────────────────────────────

    #[test]
    fn construct_stored_generates_cred_prefix_id() {
        let stored =
            CredentialStored::new("int_github".into(), "default".into(), sample_key_info());
        assert!(
            stored.id.starts_with("cred_"),
            "expected 'cred_' prefix, got: {}",
            stored.id
        );
    }

    #[test]
    fn construct_stored_id_is_unique_per_call() {
        let a = CredentialStored::new("int_a".into(), "lbl".into(), sample_key_info());
        let b = CredentialStored::new("int_b".into(), "lbl".into(), sample_key_info());
        assert_ne!(a.id, b.id, "each constructor call must produce a unique ID");
    }

    #[test]
    fn construct_stored_id_has_expected_length() {
        let stored = CredentialStored::new("int_test".into(), "test".into(), sample_key_info());
        // "cred_" (5) + 12 hex + 14 base62 = 31 characters
        assert_eq!(
            stored.id.len(),
            31,
            "expected 31 chars (cred_ + 26), got {}: {}",
            stored.id.len(),
            stored.id
        );
    }

    #[test]
    fn construct_stored_with_explicit_id() {
        let stored = CredentialStored::with_id(
            "cred_manual123".into(),
            "int_x".into(),
            "manual".into(),
            sample_oauth_info(),
        );
        assert_eq!(stored.id, "cred_manual123");
        assert_eq!(stored.integration_id, "int_x");
        assert_eq!(stored.label, "manual");
    }

    // ── CredentialOAuth serialization ───────────────────────────────────

    #[test]
    fn oauth_serialize_contains_expected_fields() {
        let oauth = CredentialOAuth {
            method_id: "mth_gh".into(),
            refresh: "rt_abc".into(),
            access: "at_xyz".into(),
            expires: 9_999_999,
            metadata: None,
        };
        let json = serde_json::to_value(&oauth).expect("serialize CredentialOAuth");
        assert_eq!(json["methodID"], "mth_gh");
        assert_eq!(json["refresh"], "rt_abc");
        assert_eq!(json["access"], "at_xyz");
        assert_eq!(json["expires"], 9_999_999);
        assert!(json.get("metadata").is_none());
    }

    #[test]
    fn oauth_serialize_with_metadata() {
        let mut meta = HashMap::new();
        meta.insert("scope".into(), "repo,user".into());
        let oauth = CredentialOAuth {
            method_id: "mth_x".into(),
            refresh: "r".into(),
            access: "a".into(),
            expires: 1,
            metadata: Some(meta),
        };
        let json = serde_json::to_value(&oauth).expect("serialize with metadata");
        let md = &json["metadata"];
        assert_eq!(md["scope"], "repo,user");
    }

    #[test]
    fn oauth_deserialize_roundtrip() {
        let original = CredentialOAuth {
            method_id: "mth_r".into(),
            refresh: "rt_r".into(),
            access: "at_r".into(),
            expires: 42,
            metadata: None,
        };
        let json = serde_json::to_string(&original).expect("serialize");
        let restored: CredentialOAuth = serde_json::from_str(&json).expect("deserialize");
        assert_eq!(restored, original);
    }

    // ── CredentialKey serialization ─────────────────────────────────────

    #[test]
    fn key_serialize_contains_expected_fields() {
        let key = CredentialKey {
            key: "secret-token".into(),
            metadata: None,
        };
        let json = serde_json::to_value(&key).expect("serialize CredentialKey");
        assert_eq!(json["key"], "secret-token");
        assert!(json.get("metadata").is_none());
    }

    #[test]
    fn key_deserialize_roundtrip() {
        let original = CredentialKey {
            key: "sk-test-123".into(),
            metadata: None,
        };
        let json = serde_json::to_string(&original).expect("serialize");
        let restored: CredentialKey = serde_json::from_str(&json).expect("deserialize");
        assert_eq!(restored, original);
    }

    // ── CredentialInfo tagged enum ──────────────────────────────────────

    #[test]
    fn info_serialize_oauth_includes_type_tag() {
        let info = sample_oauth_info();
        let json = serde_json::to_value(&info).expect("serialize CredentialInfo::OAuth");
        assert_eq!(json["type"], "oauth");
        assert_eq!(json["methodID"], "mth_abc123");
        assert_eq!(json["access"], "at_secret");
    }

    #[test]
    fn info_serialize_key_includes_type_tag() {
        let info = sample_key_info();
        let json = serde_json::to_value(&info).expect("serialize CredentialInfo::Key");
        assert_eq!(json["type"], "key");
        assert_eq!(json["key"], "sk-1234567890abcdef");
    }

    #[test]
    fn info_deserialize_oauth_from_json() {
        let json = serde_json::json!({
            "type": "oauth",
            "methodID": "mth_test",
            "refresh": "rt_test",
            "access": "at_test",
            "expires": 100
        });
        let info: CredentialInfo = serde_json::from_value(json).expect("deserialize oauth");
        match info {
            CredentialInfo::OAuth(oauth) => {
                assert_eq!(oauth.method_id, "mth_test");
                assert_eq!(oauth.expires, 100);
            }
            other => panic!("expected OAuth variant, got: {other:?}"),
        }
    }

    #[test]
    fn info_deserialize_key_from_json() {
        let json = serde_json::json!({
            "type": "key",
            "key": "my-api-token"
        });
        let info: CredentialInfo = serde_json::from_value(json).expect("deserialize key");
        match info {
            CredentialInfo::Key(key) => {
                assert_eq!(key.key, "my-api-token");
            }
            other => panic!("expected Key variant, got: {other:?}"),
        }
    }

    #[test]
    fn info_roundtrip_oauth_via_json() {
        let original = CredentialInfo::OAuth(CredentialOAuth {
            method_id: "mth_rt".into(),
            refresh: "rt_rt".into(),
            access: "at_rt".into(),
            expires: 1_000,
            metadata: {
                let mut m = HashMap::new();
                m.insert("org".into(), "acme".into());
                Some(m)
            },
        });
        let json = serde_json::to_string(&original).expect("serialize");
        let restored: CredentialInfo = serde_json::from_str(&json).expect("deserialize");
        assert_eq!(restored, original);
    }

    #[test]
    fn info_roundtrip_key_via_json() {
        let original = CredentialInfo::Key(CredentialKey {
            key: "sk-roundtrip-test".into(),
            metadata: {
                let mut m = HashMap::new();
                m.insert("env".into(), "production".into());
                Some(m)
            },
        });
        let json = serde_json::to_string(&original).expect("serialize");
        let restored: CredentialInfo = serde_json::from_str(&json).expect("deserialize");
        assert_eq!(restored, original);
    }

    // ── CredentialStored serialization ──────────────────────────────────

    #[test]
    fn stored_serialize_includes_integration_id() {
        let stored =
            CredentialStored::new("int_gh".into(), "github-prod".into(), sample_oauth_info());
        let json = serde_json::to_value(&stored).expect("serialize CredentialStored");
        assert_eq!(json["integrationID"], "int_gh");
        assert_eq!(json["label"], "github-prod");
        assert!(json["id"]
            .as_str()
            .expect("id is string")
            .starts_with("cred_"));
        assert_eq!(json["value"]["type"], "oauth");
    }

    #[test]
    fn stored_roundtrip_via_json() {
        let original =
            CredentialStored::new("int_rt".into(), "roundtrip".into(), sample_key_info());
        let json = serde_json::to_string(&original).expect("serialize");
        let restored: CredentialStored = serde_json::from_str(&json).expect("deserialize");
        assert_eq!(restored, original);
    }

    // ── CredentialTableRow serialization ────────────────────────────────

    #[test]
    fn table_row_serialize_full() {
        let row = CredentialTableRow {
            id: "cred_full".into(),
            integration_id: "int_full".into(),
            label: "full-label".into(),
            value: sample_oauth_info(),
            connector_id: Some("conn_1".into()),
            method_id: Some("mth_1".into()),
            active: Some(true),
            time_created: "2025-01-01T00:00:00Z".into(),
            time_updated: "2025-06-01T12:00:00Z".into(),
        };
        let json = serde_json::to_value(&row).expect("serialize table row");
        assert_eq!(json["id"], "cred_full");
        assert_eq!(json["integration_id"], "int_full");
        assert_eq!(json["connector_id"], "conn_1");
        assert_eq!(json["method_id"], "mth_1");
        assert_eq!(json["active"], true);
        assert_eq!(json["value"]["type"], "oauth");
        assert_eq!(json["time_created"], "2025-01-01T00:00:00Z");
        assert_eq!(json["time_updated"], "2025-06-01T12:00:00Z");
    }

    #[test]
    fn table_row_serialize_minimal() {
        let row = CredentialTableRow {
            id: "cred_min".into(),
            integration_id: "int_min".into(),
            label: "minimal".into(),
            value: sample_key_info(),
            connector_id: None,
            method_id: None,
            active: None,
            time_created: "2025-01-01T00:00:00Z".into(),
            time_updated: "2025-01-01T00:00:00Z".into(),
        };
        let json = serde_json::to_value(&row).expect("serialize minimal row");
        // Optional fields with `skip_serializing_if = "Option::is_none"`
        // should be absent from the output.
        assert!(json.get("connector_id").is_none());
        assert!(json.get("method_id").is_none());
        assert!(json.get("active").is_none());
    }

    #[test]
    fn table_row_roundtrip_via_json() {
        let original = CredentialTableRow {
            id: "cred_rt2".into(),
            integration_id: "int_rt2".into(),
            label: "rt2".into(),
            value: CredentialInfo::Key(CredentialKey {
                key: "secret".into(),
                metadata: None,
            }),
            connector_id: Some("c2".into()),
            method_id: None,
            active: Some(false),
            time_created: "2024-12-31T23:59:59Z".into(),
            time_updated: "2025-01-01T00:00:01Z".into(),
        };
        let json = serde_json::to_string(&original).expect("serialize");
        let restored: CredentialTableRow = serde_json::from_str(&json).expect("deserialize");
        assert_eq!(restored, original);
    }

    // ── Wrong type discriminant handling ───────────────────────────────

    #[test]
    fn deserialize_unknown_type_errors() {
        let json = serde_json::json!({
            "type": "unknown_variant",
            "key": "val"
        });
        let result: Result<CredentialInfo, _> = serde_json::from_value(json);
        assert!(
            result.is_err(),
            "unknown variant should fail deserialization"
        );
    }
}
