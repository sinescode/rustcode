//! Account management types — authentication, device polling, and
//! SQLite persistence.
//!
//! Ported from:
//! - `packages/core/src/account.ts`
//! - `packages/core/src/account/sql.ts`

use serde::{Deserialize, Serialize};

// ---------------------------------------------------------------------------
// Branded type aliases
// ---------------------------------------------------------------------------

/// Branded string type for account identifiers.
///
/// Ported from: `packages/core/src/account.ts` — `ID` type alias
/// (`Schema.String.pipe(Schema.brand("AccountID"))`).
pub type AccountId = String;

/// Branded string type for organization identifiers.
///
/// Ported from: `packages/core/src/account.ts` — `OrgID` type alias
/// (`Schema.String.pipe(Schema.brand("OrgID"))`).
pub type OrgId = String;

/// Branded string type for OAuth access tokens.
///
/// Ported from: `packages/core/src/account.ts` — `AccessToken` type alias
/// (`Schema.String.pipe(Schema.brand("AccessToken"))`).
pub type AccessToken = String;

/// Branded string type for OAuth refresh tokens.
///
/// Ported from: `packages/core/src/account.ts` — `RefreshToken` type alias
/// (`Schema.String.pipe(Schema.brand("RefreshToken"))`).
pub type RefreshToken = String;

/// Branded string type for device authorization codes.
///
/// Ported from: `packages/core/src/account.ts` — `DeviceCode` type alias
/// (`Schema.String.pipe(Schema.brand("DeviceCode"))`).
pub type DeviceCode = String;

/// Branded string type for user-facing authorization codes
/// (displayed during device auth flow).
///
/// Ported from: `packages/core/src/account.ts` — `UserCode` type alias
/// (`Schema.String.pipe(Schema.brand("UserCode"))`).
pub type UserCode = String;

// ---------------------------------------------------------------------------
// Core data types
// ---------------------------------------------------------------------------

/// An authenticated user account.
///
/// Ported from: `packages/core/src/account.ts` — `Info` class
/// (`Schema.Class<Info>("Account")`).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AccountInfo {
    /// Unique account identifier.
    pub id: AccountId,
    /// Email address associated with the account.
    pub email: String,
    /// Server URL for this account.
    pub url: String,
    /// Currently active organization ID, if any.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub active_org_id: Option<OrgId>,
}

/// An organization that belongs to an account.
///
/// Ported from: `packages/core/src/account.ts` — `Org` class
/// (`Schema.Class<Org>("Org")`).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AccountOrg {
    /// Unique organization identifier.
    pub id: OrgId,
    /// Display name of the organization.
    pub name: String,
}

/// Data returned when the device authorization flow is initiated.
///
/// Ported from: `packages/core/src/account.ts` — `Login` class
/// (`Schema.Class<Login>("Login")`).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AccountLogin {
    /// The device code to use when polling for completion.
    pub code: DeviceCode,
    /// The user code to display to the user for manual entry.
    pub user: UserCode,
    /// The verification URL the user should visit.
    pub url: String,
    /// The server hostname used for this login flow.
    pub server: String,
    /// How long until the login session expires (serialized `Duration`).
    pub expiry: String,
    /// How long to wait between polling attempts (serialized `Duration`).
    pub interval: String,
}

// ---------------------------------------------------------------------------
// Poll result types
// ---------------------------------------------------------------------------

/// The device authorization poll succeeded — the user completed the flow.
///
/// Ported from: `packages/core/src/account.ts` — `PollSuccess` class
/// (`Schema.TaggedClass<PollSuccess>()("PollSuccess", ...)`).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PollSuccess {
    /// Email address of the authenticated user.
    pub email: String,
}

/// The device authorization poll is still pending — keep polling.
///
/// Ported from: `packages/core/src/account.ts` — `PollPending` class
/// (`Schema.TaggedClass<PollPending>()("PollPending", {})`).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PollPending {}

/// The server has requested a slower polling interval.
///
/// Ported from: `packages/core/src/account.ts` — `PollSlow` class
/// (`Schema.TaggedClass<PollSlow>()("PollSlow", {})`).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PollSlow {}

/// The device authorization code has expired.
///
/// Ported from: `packages/core/src/account.ts` — `PollExpired` class
/// (`Schema.TaggedClass<PollExpired>()("PollExpired", {})`).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PollExpired {}

/// The user denied the device authorization request.
///
/// Ported from: `packages/core/src/account.ts` — `PollDenied` class
/// (`Schema.TaggedClass<PollDenied>()("PollDenied", {})`).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PollDenied {}

/// An error occurred during the device authorization poll.
///
/// Ported from: `packages/core/src/account.ts` — `PollError` class
/// (`Schema.TaggedClass<PollError>()("PollError", { cause: Schema.Defect })`).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PollError {
    /// The underlying error that caused the poll to fail.
    pub cause: String,
}

/// Tagged union of all possible device authorization poll outcomes.
///
/// Ported from: `packages/core/src/account.ts` — `PollResult` union
/// (`Schema.Union([PollSuccess, PollPending, PollSlow, PollExpired, PollDenied, PollError])`).
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type")]
pub enum PollResult {
    /// Authentication succeeded — tokens are now available.
    #[serde(rename = "PollSuccess")]
    Success(PollSuccess),
    /// Authorization is still pending — continue polling.
    #[serde(rename = "PollPending")]
    Pending(PollPending),
    /// Server requests a slower polling interval.
    #[serde(rename = "PollSlow")]
    Slow(PollSlow),
    /// The device code has expired — restart the flow.
    #[serde(rename = "PollExpired")]
    Expired(PollExpired),
    /// The user denied the authorization request.
    #[serde(rename = "PollDenied")]
    Denied(PollDenied),
    /// An error occurred during polling.
    #[serde(rename = "PollError")]
    Error(PollError),
}

// ---------------------------------------------------------------------------
// Error types
// ---------------------------------------------------------------------------

/// A repository-level account error (e.g. local storage failure).
///
/// Ported from: `packages/core/src/account.ts` — `AccountRepoError` class
/// (`Schema.TaggedErrorClass<AccountRepoError>()("AccountRepoError", ...)`).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AccountRepoError {
    /// Human-readable error message.
    pub message: String,
    /// Optional underlying error that caused this failure.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub cause: Option<String>,
}

/// A service-level account error (e.g. API returned an error response).
///
/// Ported from: `packages/core/src/account.ts` — `AccountServiceError` class
/// (`Schema.TaggedErrorClass<AccountServiceError>()("AccountServiceError", ...)`).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AccountServiceError {
    /// Human-readable error message.
    pub message: String,
    /// Optional underlying error that caused this failure.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub cause: Option<String>,
}

/// A transport-level account error — the request never reached the server.
///
/// Ported from: `packages/core/src/account.ts` — `AccountTransportError` class
/// (`Schema.TaggedErrorClass<AccountTransportError>()("AccountTransportError", ...)`).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AccountTransportError {
    /// HTTP method used for the request.
    pub method: String,
    /// Target URL that could not be reached.
    pub url: String,
    /// Optional human-readable description of the transport failure.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
    /// Optional underlying error that caused this transport failure.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub cause: Option<String>,
}

impl AccountTransportError {
    /// Returns a formatted, multi-line error message describing the transport
    /// failure. Includes the method, URL, an optional description, and a
    /// suggestion to check network/proxy/VPN configuration.
    ///
    /// Ported from: `packages/core/src/account.ts` —
    /// `AccountTransportError.get message()` getter.
    pub fn message(&self) -> String {
        let lines = [
            format!("Could not reach {} {}.", self.method, self.url),
            "This failed before the server returned an HTTP response.".to_string(),
            self.description.clone().unwrap_or_default(),
            "Check your network, proxy, or VPN configuration and try again.".to_string(),
        ];
        lines
            .into_iter()
            .filter(|s| !s.is_empty())
            .collect::<Vec<_>>()
            .join("\n")
    }
}

/// All possible account-related errors.
///
/// Ported from: `packages/core/src/account.ts` — `AccountError` union type
/// (`AccountRepoError | AccountServiceError | AccountTransportError`).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum AccountError {
    /// Repository-level error (storage, filesystem, etc.).
    RepoError(AccountRepoError),
    /// Service-level error (API returned an error response).
    ServiceError(AccountServiceError),
    /// Transport-level error (network, proxy, VPN, DNS).
    TransportError(AccountTransportError),
}

// ---------------------------------------------------------------------------
// SQLite table row types
// ---------------------------------------------------------------------------

/// A row from the `account` SQLite table.
///
/// Ported from: `packages/core/src/account/sql.ts` — `AccountTable`
/// (`sqliteTable("account", ...)` via drizzle-orm).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AccountTableRow {
    /// Unique account identifier (primary key).
    pub id: AccountId,
    /// Email address associated with the account.
    pub email: String,
    /// Server URL for this account.
    pub url: String,
    /// OAuth access token.
    pub access_token: AccessToken,
    /// OAuth refresh token.
    pub refresh_token: RefreshToken,
    /// Unix timestamp (milliseconds) when the access token expires, if known.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub token_expiry: Option<i64>,
    /// Timestamp when this row was created.
    pub time_created: String,
    /// Timestamp when this row was last updated.
    pub time_updated: String,
}

/// A row from the `account_state` SQLite table.
///
/// Ported from: `packages/core/src/account/sql.ts` — `AccountStateTable`
/// (`sqliteTable("account_state", ...)` via drizzle-orm).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AccountStateTableRow {
    /// Row ID (primary key).
    pub id: i64,
    /// The currently active account ID, if any.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub active_account_id: Option<AccountId>,
    /// The currently active organization ID, if any.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub active_org_id: Option<OrgId>,
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    // -- Aliases are functional ------------------------------------------

    #[test]
    fn test_type_aliases_are_usable() {
        let id: AccountId = "acct_123".to_string();
        let org: OrgId = "org_456".to_string();
        let access: AccessToken = "tok_access".to_string();
        let refresh: RefreshToken = "tok_refresh".to_string();
        let device: DeviceCode = "dev_code".to_string();
        let user: UserCode = "USR-ABCD".to_string();

        assert_eq!(id, "acct_123");
        assert_eq!(org, "org_456");
        assert_eq!(access, "tok_access");
        assert_eq!(refresh, "tok_refresh");
        assert_eq!(device, "dev_code");
        assert_eq!(user, "USR-ABCD");
    }

    // -- AccountInfo construction + round-trip ---------------------------

    #[test]
    fn test_account_info_round_trip() {
        let info = AccountInfo {
            id: "acct_1".to_string(),
            email: "user@example.com".to_string(),
            url: "https://api.example.com".to_string(),
            active_org_id: Some("org_1".to_string()),
        };

        let json = serde_json::to_string(&info).expect("serialize AccountInfo");
        let parsed: AccountInfo = serde_json::from_str(&json).expect("deserialize AccountInfo");

        assert_eq!(parsed.id, "acct_1");
        assert_eq!(parsed.email, "user@example.com");
        assert_eq!(parsed.url, "https://api.example.com");
        assert_eq!(parsed.active_org_id, Some("org_1".to_string()));
    }

    #[test]
    fn test_account_info_none_org_omitted() {
        let info = AccountInfo {
            id: "acct_2".to_string(),
            email: "noorg@example.com".to_string(),
            url: "https://api.example.com".to_string(),
            active_org_id: None,
        };

        let json = serde_json::to_string(&info).expect("serialize AccountInfo");
        // active_org_id should be absent, not null
        assert!(!json.contains("active_org_id"));
    }

    // -- AccountOrg round-trip -------------------------------------------

    #[test]
    fn test_account_org_round_trip() {
        let org = AccountOrg {
            id: "org_42".to_string(),
            name: "Engineering".to_string(),
        };

        let json = serde_json::to_string(&org).expect("serialize AccountOrg");
        let parsed: AccountOrg = serde_json::from_str(&json).expect("deserialize AccountOrg");

        assert_eq!(parsed.id, "org_42");
        assert_eq!(parsed.name, "Engineering");
    }

    // -- AccountLogin round-trip -----------------------------------------

    #[test]
    fn test_account_login_round_trip() {
        let login = AccountLogin {
            code: "device-code-abc".to_string(),
            user: "USR-XYZ".to_string(),
            url: "https://example.com/verify".to_string(),
            server: "example.com".to_string(),
            expiry: "15 minutes".to_string(),
            interval: "5 seconds".to_string(),
        };

        let json = serde_json::to_string(&login).expect("serialize AccountLogin");
        let parsed: AccountLogin = serde_json::from_str(&json).expect("deserialize AccountLogin");

        assert_eq!(parsed.code, "device-code-abc");
        assert_eq!(parsed.user, "USR-XYZ");
        assert_eq!(parsed.url, "https://example.com/verify");
        assert_eq!(parsed.server, "example.com");
        assert_eq!(parsed.expiry, "15 minutes");
        assert_eq!(parsed.interval, "5 seconds");
    }

    // -- PollResult tagged enum serialization ----------------------------

    #[test]
    fn test_poll_result_success_has_type_tag() {
        let result = PollResult::Success(PollSuccess {
            email: "dev@example.com".to_string(),
        });

        let json = serde_json::to_string(&result).expect("serialize PollResult::Success");
        assert!(json.contains(r#""type":"PollSuccess""#));
        assert!(json.contains(r#""email":"dev@example.com""#));
    }

    #[test]
    fn test_poll_result_pending_has_type_tag() {
        let result = PollResult::Pending(PollPending {});

        let json = serde_json::to_string(&result).expect("serialize PollResult::Pending");
        assert!(json.contains(r#""type":"PollPending""#));
    }

    #[test]
    fn test_poll_result_deserialize_success() {
        let json = r#"{"type":"PollSuccess","email":"dev@example.com"}"#;
        let parsed: PollResult = serde_json::from_str(json).expect("deserialize PollResult::Success");

        match parsed {
            PollResult::Success(s) => assert_eq!(s.email, "dev@example.com"),
            other => panic!("expected PollSuccess, got {other:?}"),
        }
    }

    #[test]
    fn test_poll_result_deserialize_denied() {
        let json = r#"{"type":"PollDenied"}"#;
        let parsed: PollResult = serde_json::from_str(json).expect("deserialize PollResult::Denied");

        assert!(matches!(parsed, PollResult::Denied(_)));
    }

    #[test]
    fn test_poll_result_deserialize_error() {
        let json = r#"{"type":"PollError","cause":"network timeout"}"#;
        let parsed: PollResult = serde_json::from_str(json).expect("deserialize PollResult::Error");

        match parsed {
            PollResult::Error(e) => assert_eq!(e.cause, "network timeout"),
            other => panic!("expected PollError, got {other:?}"),
        }
    }

    // -- Transport error message formatting ------------------------------

    #[test]
    fn test_transport_error_message_with_description() {
        let err = AccountTransportError {
            method: "POST".to_string(),
            url: "https://api.example.com/token".to_string(),
            description: Some("connection refused".to_string()),
            cause: Some("ECONNREFUSED".to_string()),
        };

        let msg = err.message();
        assert!(msg.contains("Could not reach POST https://api.example.com/token."));
        assert!(msg.contains("This failed before the server returned an HTTP response."));
        assert!(msg.contains("connection refused"));
        assert!(msg.contains("Check your network"));
    }

    #[test]
    fn test_transport_error_message_without_description() {
        let err = AccountTransportError {
            method: "GET".to_string(),
            url: "https://api.example.com/status".to_string(),
            description: None,
            cause: None,
        };

        let msg = err.message();
        assert!(msg.contains("Could not reach GET https://api.example.com/status."));
        assert!(msg.contains("This failed before the server returned an HTTP response."));
        assert!(msg.contains("Check your network"));
        // Description line should not appear as a blank line
        let lines: Vec<&str> = msg.lines().collect();
        assert_eq!(lines.len(), 3, "expected 3 non-empty lines, got: {msg:?}");
    }

    // -- AccountError variant round-trips --------------------------------

    #[test]
    fn test_account_error_repo_round_trip() {
        let err = AccountError::RepoError(AccountRepoError {
            message: "failed to read config".to_string(),
            cause: Some("ENOENT".to_string()),
        });

        let json = serde_json::to_string(&err).expect("serialize AccountError::RepoError");
        let parsed: AccountError =
            serde_json::from_str(&json).expect("deserialize AccountError::RepoError");

        match parsed {
            AccountError::RepoError(e) => {
                assert_eq!(e.message, "failed to read config");
                assert_eq!(e.cause, Some("ENOENT".to_string()));
            }
            other => panic!("expected RepoError, got {other:?}"),
        }
    }

    #[test]
    fn test_account_error_service_round_trip() {
        let err = AccountError::ServiceError(AccountServiceError {
            message: "invalid_grant".to_string(),
            cause: None,
        });

        let json = serde_json::to_string(&err).expect("serialize AccountError::ServiceError");
        assert!(!json.contains("cause"));

        let parsed: AccountError =
            serde_json::from_str(&json).expect("deserialize AccountError::ServiceError");

        match parsed {
            AccountError::ServiceError(e) => {
                assert_eq!(e.message, "invalid_grant");
                assert_eq!(e.cause, None);
            }
            other => panic!("expected ServiceError, got {other:?}"),
        }
    }

    #[test]
    fn test_account_error_transport_round_trip() {
        let err = AccountError::TransportError(AccountTransportError {
            method: "DELETE".to_string(),
            url: "https://api.example.com/session".to_string(),
            description: None,
            cause: Some("DNS resolution failed".to_string()),
        });

        let json = serde_json::to_string(&err).expect("serialize AccountError::TransportError");
        let parsed: AccountError =
            serde_json::from_str(&json).expect("deserialize AccountError::TransportError");

        match parsed {
            AccountError::TransportError(e) => {
                assert_eq!(e.method, "DELETE");
                assert_eq!(e.url, "https://api.example.com/session");
                assert_eq!(e.description, None);
                assert_eq!(e.cause, Some("DNS resolution failed".to_string()));
            }
            other => panic!("expected TransportError, got {other:?}"),
        }
    }

    // -- SQLite row types ------------------------------------------------

    #[test]
    fn test_account_table_row_round_trip() {
        let row = AccountTableRow {
            id: "acct_99".to_string(),
            email: "row@example.com".to_string(),
            url: "https://api.example.com".to_string(),
            access_token: "tok_acc".to_string(),
            refresh_token: "tok_ref".to_string(),
            token_expiry: Some(1718000000000i64),
            time_created: "1718000000000".to_string(),
            time_updated: "1718100000000".to_string(),
        };

        let json = serde_json::to_string(&row).expect("serialize AccountTableRow");
        let parsed: AccountTableRow = serde_json::from_str(&json).expect("deserialize AccountTableRow");

        assert_eq!(parsed.id, "acct_99");
        assert_eq!(parsed.email, "row@example.com");
        assert_eq!(parsed.access_token, "tok_acc");
        assert_eq!(parsed.refresh_token, "tok_ref");
        assert_eq!(parsed.token_expiry, Some(1718000000000));
        assert_eq!(parsed.time_created, "1718000000000");
        assert_eq!(parsed.time_updated, "1718100000000");
    }

    #[test]
    fn test_account_state_table_row_round_trip() {
        let row = AccountStateTableRow {
            id: 1,
            active_account_id: Some("acct_1".to_string()),
            active_org_id: None,
        };

        let json = serde_json::to_string(&row).expect("serialize AccountStateTableRow");
        let parsed: AccountStateTableRow =
            serde_json::from_str(&json).expect("deserialize AccountStateTableRow");

        assert_eq!(parsed.id, 1);
        assert_eq!(parsed.active_account_id, Some("acct_1".to_string()));
        assert_eq!(parsed.active_org_id, None);
    }

    #[test]
    fn test_account_state_table_row_all_none() {
        let row = AccountStateTableRow {
            id: 0,
            active_account_id: None,
            active_org_id: None,
        };

        let json = serde_json::to_string(&row).expect("serialize AccountStateTableRow");
        // Neither optional field should appear
        assert!(!json.contains("active_account_id"));
        assert!(!json.contains("active_org_id"));
        assert!(json.contains(r#""id":0"#));
    }
}
