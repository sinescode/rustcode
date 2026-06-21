//! Auth module — OAuth, API key, and well-known credential storage.
//!
//! Ported from: `packages/opencode/src/auth/index.ts`
//! OpenCode commit: 5d0f86606ac30690f79f0a6a9f41a1f49fe95d0b
//!
//! ## Architecture
//!
//! The TS source stores authentication credentials as a JSON file at
//! `Global.Path.data/auth.json`. It provides get, set, remove, and
//! list operations on the stored credentials.
//!
//! Three credential variants exist:
//! - **OAuth** — access/refresh token pair with expiry (e.g. GitHub, Google)
//! - **Api** — a single API key string
//! - **WellKnown** — a key + token pair for pre-established services
//!
//! This module mirrors the TS `Auth` service and provides the same
//! `get`, `all`, `set`, `remove` operations.

use serde::{Deserialize, Serialize};
use std::collections::HashMap;

// ---------------------------------------------------------------------------
// Constants
// ---------------------------------------------------------------------------

/// Dummy key value used to reserve OAuth credential slots.
///
/// # Source
/// `packages/opencode/src/auth/index.ts` line 8.
pub const OAUTH_DUMMY_KEY: &str = "opencode-oauth-dummy-key";

// ---------------------------------------------------------------------------
// Auth credential variants
// ---------------------------------------------------------------------------

/// OAuth credential — access token, refresh token, and expiry.
///
/// # Source
/// `packages/opencode/src/auth/index.ts` — `Oauth` class.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct AuthOauth {
    /// Refresh token (long-lived).
    pub refresh: String,
    /// Access token (short-lived bearer token).
    pub access: String,
    /// Expiration timestamp (non-negative integer, milliseconds since epoch).
    pub expires: i64,
    /// Optional account identifier.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub account_id: Option<String>,
    /// Optional enterprise URL (for GitHub Enterprise, etc.).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub enterprise_url: Option<String>,
}

impl AuthOauth {
    /// Create a new OAuth credential.
    #[must_use]
    pub fn new(refresh: String, access: String, expires: i64) -> Self {
        Self {
            refresh,
            access,
            expires,
            account_id: None,
            enterprise_url: None,
        }
    }
}

/// API key credential.
///
/// # Source
/// `packages/opencode/src/auth/index.ts` — `Api` class.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct AuthApi {
    /// The API key value.
    pub key: String,
    /// Optional provider-specific metadata.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub metadata: Option<HashMap<String, String>>,
}

impl AuthApi {
    /// Create a new API key credential.
    #[must_use]
    pub fn new(key: String) -> Self {
        Self {
            key,
            metadata: None,
        }
    }
}

/// Well-known credential — key + token pair for pre-established services.
///
/// # Source
/// `packages/opencode/src/auth/index.ts` — `WellKnown` class.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct AuthWellKnown {
    /// The well-known key identifier.
    pub key: String,
    /// The well-known token value.
    pub token: String,
}

impl AuthWellKnown {
    /// Create a new well-known credential.
    #[must_use]
    pub fn new(key: String, token: String) -> Self {
        Self {
            key,
            token,
        }
    }
}

/// Variant discriminant for auth credential types.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum AuthVariant {
    /// OAuth credential.
    Oauth,
    /// API key credential.
    Api,
    /// Well-known credential.
    Wellknown,
}

/// Tagged union of all auth credential variants.
///
/// Serialized as `{"type": "oauth", ...}`, `{"type": "api", ...}`, or
/// `{"type": "wellknown", ...}`.
///
/// # Source
/// `packages/opencode/src/auth/index.ts` — `Info` (lines 35–36).
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(tag = "type", rename_all = "lowercase")]
pub enum AuthInfo {
    /// OAuth token pair credential.
    Oauth(AuthOauth),
    /// API key credential.
    Api(AuthApi),
    /// Well-known credential.
    Wellknown(AuthWellKnown),
}

/// Auth file — the on-disk representation matching `auth.json`.
///
/// A map of provider ID → auth credential info.
pub type AuthStore = HashMap<String, AuthInfo>;

// ---------------------------------------------------------------------------
// Auth service
// ---------------------------------------------------------------------------

/// Auth service — manages credential persistence to `auth.json`.
///
/// Reads/writes a JSON file at the application data directory. Provides
/// `get`, `all`, `set`, and `remove` operations.
///
/// # Source
/// Ported from `packages/opencode/src/auth/index.ts` lines 43–91.
#[derive(Debug, Clone)]
pub struct Auth {
    /// Path to the auth.json file.
    file_path: String,
}

impl Auth {
    /// Create a new auth service with a custom file path.
    #[must_use]
    pub fn new(file_path: impl Into<String>) -> Self {
        Self {
            file_path: file_path.into(),
        }
    }

    /// Create an auth service using the global data directory path.
    ///
    /// Uses `GlobalPaths.data` joined with `auth.json`.
    #[must_use]
    pub fn from_data_dir(data_dir: &str) -> Self {
        let file_path = format!("{}/auth.json", data_dir.trim_end_matches('/'));
        Self { file_path }
    }

    /// Load the full auth store from disk.
    ///
    /// Returns an empty map if the file does not exist or cannot be parsed.
    ///
    /// # Source
    /// Ported from `packages/opencode/src/auth/index.ts` lines 58–66 (`all`).
    fn load_store(&self) -> AuthStore {
        // Check OPENCODE_AUTH_CONTENT env var first (TS line 59–63)
        if let Ok(content) = std::env::var("OPENCODE_AUTH_CONTENT") {
            if let Ok(store) = serde_json::from_str::<AuthStore>(&content) {
                return store;
            }
        }

        match std::fs::read_to_string(&self.file_path) {
            Ok(content) => serde_json::from_str::<AuthStore>(&content).unwrap_or_default(),
            Err(_) => AuthStore::new(),
        }
    }

    /// Save the full auth store to disk.
    ///
    /// Creates parent directories if needed and writes with restricted
    /// permissions (0o600 on Unix).
    fn save_store(&self, store: &AuthStore) -> Result<(), crate::error::Error> {
        let json = serde_json::to_string_pretty(store)
            .map_err(|e| crate::error::Error::Auth(format!("failed to serialize auth data: {e}")))?;

        // Ensure parent directory exists
        if let Some(parent) = std::path::Path::new(&self.file_path).parent() {
            std::fs::create_dir_all(parent)
                .map_err(|e| crate::error::Error::Auth(format!("failed to create auth dir: {e}")))?;
        }

        std::fs::write(&self.file_path, &json)
            .map_err(|e| crate::error::Error::Auth(format!("failed to write auth file: {e}")))?;

        // Set file permissions to 0o600 on Unix
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            if let Ok(meta) = std::fs::metadata(&self.file_path) {
                let mut perms = meta.permissions();
                perms.set_mode(0o600);
                let _ = std::fs::set_permissions(&self.file_path, perms);
            }
        }

        Ok(())
    }

    /// Get a credential by provider ID.
    ///
    /// # Source
    /// Ported from `packages/opencode/src/auth/index.ts` lines 69–71 (`get`).
    #[must_use]
    pub fn get(&self, provider_id: &str) -> Option<AuthInfo> {
        let store = self.load_store();
        store.get(provider_id).cloned()
    }

    /// Get all stored credentials.
    ///
    /// # Source
    /// Ported from `packages/opencode/src/auth/index.ts` lines 58–66 (`all`).
    #[must_use]
    pub fn all(&self) -> AuthStore {
        self.load_store()
    }

    /// Set a credential for a given provider ID.
    ///
    /// Normalizes trailing slashes (strips them). If the key already exists
    /// with a different trailing-slash variant, the old variant is removed.
    ///
    /// # Source
    /// Ported from `packages/opencode/src/auth/index.ts` lines 73–81 (`set`).
    pub fn set(&self, key: &str, info: &AuthInfo) -> Result<(), crate::error::Error> {
        let norm = key.trim_end_matches('/').to_string();
        let mut store = self.load_store();

        // If the normalized key differs from the original, remove the original
        if norm != key {
            store.remove(key);
        }
        // Remove any entry with trailing slash variant
        store.remove(&format!("{norm}/"));

        store.insert(norm, info.clone());
        self.save_store(&store)
    }

    /// Remove a credential by provider ID.
    ///
    /// Normalizes trailing slashes (strips them). Removes both the exact key
    /// and the normalized variant.
    ///
    /// # Source
    /// Ported from `packages/opencode/src/auth/index.ts` lines 83–89 (`remove`).
    pub fn remove(&self, key: &str) -> Result<(), crate::error::Error> {
        let norm = key.trim_end_matches('/').to_string();
        let mut store = self.load_store();

        store.remove(key);
        store.remove(&norm);

        self.save_store(&store)
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashMap;

    fn sample_oauth() -> AuthInfo {
        AuthInfo::Oauth(AuthOauth {
            refresh: "rt_secret".into(),
            access: "at_secret".into(),
            expires: 1_700_000_000_000,
            account_id: None,
            enterprise_url: None,
        })
    }

    fn sample_api() -> AuthInfo {
        AuthInfo::Api(AuthApi {
            key: "sk-1234567890abcdef".into(),
            metadata: None,
        })
    }

    fn sample_wellknown() -> AuthInfo {
        AuthInfo::Wellknown(AuthWellKnown {
            key: "my-service".into(),
            token: "my-token".into(),
        })
    }

    fn temp_auth() -> (Auth, std::path::PathBuf) {
        let dir = std::env::temp_dir().join(format!("auth_test_{}", std::process::id()));
        std::fs::create_dir_all(&dir).ok();
        let path = dir.join("auth.json");
        let auth = Auth::new(path.to_string_lossy().to_string());
        (auth, dir)
    }

    fn cleanup(dir: &std::path::Path) {
        std::fs::remove_dir_all(dir).ok();
    }

    // ── Variant construction ──────────────────────────────────────────────

    #[test]
    fn construct_oauth() {
        let oauth = AuthOauth::new("rt".into(), "at".into(), 1000);
        assert_eq!(oauth.refresh, "rt");
        assert_eq!(oauth.access, "at");
        assert_eq!(oauth.expires, 1000);
        assert!(oauth.account_id.is_none());
        assert!(oauth.enterprise_url.is_none());
    }

    #[test]
    fn construct_api() {
        let api = AuthApi::new("sk-key".into());
        assert_eq!(api.key, "sk-key");
        assert!(api.metadata.is_none());
    }

    #[test]
    fn construct_wellknown() {
        let wk = AuthWellKnown::new("svc".into(), "tok".into());
        assert_eq!(wk.key, "svc");
        assert_eq!(wk.token, "tok");
    }

    // ── OAuth serialization ───────────────────────────────────────────────

    #[test]
    fn oauth_serialize_contains_expected_fields() {
        let oauth = AuthOauth {
            refresh: "rt_abc".into(),
            access: "at_xyz".into(),
            expires: 9_999_999,
            account_id: Some("acct_1".into()),
            enterprise_url: Some("https://enterprise.example.com".into()),
        };
        let json = serde_json::to_value(&oauth).expect("serialize AuthOauth");
        assert_eq!(json["refresh"], "rt_abc");
        assert_eq!(json["access"], "at_xyz");
        assert_eq!(json["expires"], 9_999_999);
        assert_eq!(json["accountId"], "acct_1");
        assert_eq!(json["enterpriseUrl"], "https://enterprise.example.com");
    }

    #[test]
    fn oauth_deserialize_roundtrip() {
        let original = AuthOauth {
            refresh: "rt_r".into(),
            access: "at_r".into(),
            expires: 42,
            account_id: None,
            enterprise_url: None,
        };
        let json = serde_json::to_string(&original).expect("serialize");
        let restored: AuthOauth = serde_json::from_str(&json).expect("deserialize");
        assert_eq!(restored, original);
    }

    // ── API key serialization ─────────────────────────────────────────────

    #[test]
    fn api_serialize_contains_expected_fields() {
        let api = AuthApi {
            key: "secret-token".into(),
            metadata: None,
        };
        let json = serde_json::to_value(&api).expect("serialize AuthApi");
        assert_eq!(json["key"], "secret-token");
        assert!(json.get("metadata").is_none());
    }

    #[test]
    fn api_deserialize_roundtrip() {
        let original = AuthApi {
            key: "sk-test-123".into(),
            metadata: Some({
                let mut m = HashMap::new();
                m.insert("org".into(), "acme".into());
                m
            }),
        };
        let json = serde_json::to_string(&original).expect("serialize");
        let restored: AuthApi = serde_json::from_str(&json).expect("deserialize");
        assert_eq!(restored, original);
    }

    // ── WellKnown serialization ───────────────────────────────────────────

    #[test]
    fn wellknown_serialize_contains_expected_fields() {
        let wk = AuthWellKnown {
            key: "my-service".into(),
            token: "my-token".into(),
        };
        let json = serde_json::to_value(&wk).expect("serialize AuthWellKnown");
        assert_eq!(json["key"], "my-service");
        assert_eq!(json["token"], "my-token");
    }

    #[test]
    fn wellknown_deserialize_roundtrip() {
        let original = AuthWellKnown {
            key: "github-app".into(),
            token: "ghs_abc123".into(),
        };
        let json = serde_json::to_string(&original).expect("serialize");
        let restored: AuthWellKnown = serde_json::from_str(&json).expect("deserialize");
        assert_eq!(restored, original);
    }

    // ── AuthInfo tagged enum ──────────────────────────────────────────────

    #[test]
    fn info_serialize_oauth_includes_type_tag() {
        let info = sample_oauth();
        let json = serde_json::to_value(&info).expect("serialize AuthInfo::Oauth");
        assert_eq!(json["type"], "oauth");
        assert_eq!(json["access"], "at_secret");
    }

    #[test]
    fn info_serialize_api_includes_type_tag() {
        let info = sample_api();
        let json = serde_json::to_value(&info).expect("serialize AuthInfo::Api");
        assert_eq!(json["type"], "api");
        assert_eq!(json["key"], "sk-1234567890abcdef");
    }

    #[test]
    fn info_serialize_wellknown_includes_type_tag() {
        let info = sample_wellknown();
        let json = serde_json::to_value(&info).expect("serialize AuthInfo::Wellknown");
        assert_eq!(json["type"], "wellknown");
        assert_eq!(json["key"], "my-service");
    }

    #[test]
    fn info_deserialize_oauth_from_json() {
        let json = serde_json::json!({
            "type": "oauth",
            "refresh": "rt_test",
            "access": "at_test",
            "expires": 100
        });
        let info: AuthInfo = serde_json::from_value(json).expect("deserialize oauth");
        match info {
            AuthInfo::Oauth(oauth) => {
                assert_eq!(oauth.refresh, "rt_test");
                assert_eq!(oauth.expires, 100);
            }
            other => panic!("expected Oauth variant, got: {other:?}"),
        }
    }

    #[test]
    fn info_deserialize_api_from_json() {
        let json = serde_json::json!({
            "type": "api",
            "key": "my-api-token"
        });
        let info: AuthInfo = serde_json::from_value(json).expect("deserialize api");
        match info {
            AuthInfo::Api(api) => {
                assert_eq!(api.key, "my-api-token");
            }
            other => panic!("expected Api variant, got: {other:?}"),
        }
    }

    #[test]
    fn info_deserialize_wellknown_from_json() {
        let json = serde_json::json!({
            "type": "wellknown",
            "key": "wk-key",
            "token": "wk-token"
        });
        let info: AuthInfo = serde_json::from_value(json).expect("deserialize wellknown");
        match info {
            AuthInfo::Wellknown(wk) => {
                assert_eq!(wk.key, "wk-key");
                assert_eq!(wk.token, "wk-token");
            }
            other => panic!("expected Wellknown variant, got: {other:?}"),
        }
    }

    // ── CRUD operations ───────────────────────────────────────────────────

    #[test]
    fn auth_get_nonexistent_returns_none() {
        let (auth, dir) = temp_auth();
        let result = auth.get("nonexistent_provider");
        assert!(result.is_none());
        cleanup(&dir);
    }

    #[test]
    fn auth_set_and_get_roundtrip() {
        let (auth, dir) = temp_auth();
        let info = sample_oauth();
        auth.set("github", &info).expect("set should succeed");
        let retrieved = auth.get("github").expect("get should return Some");
        assert_eq!(retrieved, info);
        cleanup(&dir);
    }

    #[test]
    fn auth_all_returns_stored() {
        let (auth, dir) = temp_auth();
        auth.set("provider_a", &sample_api()).expect("set a");
        auth.set("provider_b", &sample_oauth()).expect("set b");
        let all = auth.all();
        assert_eq!(all.len(), 2);
        assert!(all.contains_key("provider_a"));
        assert!(all.contains_key("provider_b"));
        cleanup(&dir);
    }

    #[test]
    fn auth_remove_removes_credential() {
        let (auth, dir) = temp_auth();
        auth.set("remove_me", &sample_oauth()).expect("set");
        auth.remove("remove_me").expect("remove");
        let retrieved = auth.get("remove_me");
        assert!(retrieved.is_none());
        cleanup(&dir);
    }

    #[test]
    fn auth_set_normalizes_trailing_slash() {
        let (auth, dir) = temp_auth();
        auth.set("provider/", &sample_oauth()).expect("set with slash");
        let retrieved = auth.get("provider");
        assert!(retrieved.is_some());
        // The key with slash should no longer exist
        let with_slash = auth.get("provider/");
        assert!(with_slash.is_none());
        cleanup(&dir);
    }

    #[test]
    fn auth_remove_normalizes_trailing_slash() {
        let (auth, dir) = temp_auth();
        auth.set("mykey", &sample_api()).expect("set");
        auth.remove("mykey/").expect("remove with trailing slash");
        let retrieved = auth.get("mykey");
        assert!(retrieved.is_none());
        cleanup(&dir);
    }

    #[test]
    fn auth_store_persistence() {
        let dir = std::env::temp_dir().join(format!("auth_persist_{}", std::process::id()));
        std::fs::create_dir_all(&dir).ok();
        let path = dir.join("auth.json");

        // Write
        let auth = Auth::new(path.to_string_lossy().to_string());
        auth.set("persist_test", &sample_oauth()).expect("set");

        // Read with a new instance
        let auth2 = Auth::new(path.to_string_lossy().to_string());
        let retrieved = auth2.get("persist_test").expect("get from new instance");
        assert_eq!(retrieved, sample_oauth());

        std::fs::remove_dir_all(&dir).ok();
    }

    // ── OAUTH_DUMMY_KEY constant ──────────────────────────────────────────

    #[test]
    fn oauth_dummy_key_constant() {
        assert_eq!(OAUTH_DUMMY_KEY, "opencode-oauth-dummy-key");
    }

    // ── Deserialization error handling ─────────────────────────────────────

    #[test]
    fn deserialize_unknown_type_errors() {
        let json = serde_json::json!({
            "type": "unknown_variant",
            "key": "val"
        });
        let result: Result<AuthInfo, _> = serde_json::from_value(json);
        assert!(
            result.is_err(),
            "unknown variant should fail deserialization"
        );
    }

    #[test]
    fn auth_get_empty_store_returns_none() {
        let (auth, dir) = temp_auth();
        assert!(auth.get("anything").is_none());
        cleanup(&dir);
    }

    #[test]
    fn auth_all_empty_store() {
        let (auth, dir) = temp_auth();
        let all = auth.all();
        assert!(all.is_empty());
        cleanup(&dir);
    }

    #[test]
    fn auth_set_then_all_contains_key() {
        let (auth, dir) = temp_auth();
        auth.set("provider_x", &sample_wellknown()).expect("set");
        let all = auth.all();
        assert!(all.contains_key("provider_x"));
        cleanup(&dir);
    }
}
