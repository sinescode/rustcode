//! Account management types — authentication, device polling, and
//! SQLite persistence.
//!
//! Ported from:
//! - `packages/core/src/account.ts`
//! - `packages/core/src/account/sql.ts`

use chrono::Duration;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::sync::Arc;

// ---------------------------------------------------------------------------
// Serde helpers
// ---------------------------------------------------------------------------

mod duration_millis {
    use chrono::Duration;
    use serde::{Deserialize, Deserializer, Serialize, Serializer};
    pub fn serialize<S: Serializer>(duration: &Duration, serializer: S) -> Result<S::Ok, S::Error> {
        duration.num_milliseconds().serialize(serializer)
    }
    pub fn deserialize<'de, D: Deserializer<'de>>(deserializer: D) -> Result<Duration, D::Error> {
        let millis = i64::deserialize(deserializer)?;
        Ok(Duration::milliseconds(millis))
    }
}

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
    /// How long until the login session expires (serialized as milliseconds).
    #[serde(with = "duration_millis")]
    pub expiry: Duration,
    /// How long to wait between polling attempts (serialized as milliseconds).
    #[serde(with = "duration_millis")]
    pub interval: Duration,
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

impl PollError {
    /// Create a `PollError` from any type implementing `std::error::Error`.
    pub fn from_error(e: &dyn std::error::Error) -> Self {
        Self {
            cause: e.to_string(),
        }
    }

    /// Create a `PollError` from any type implementing `Display`.
    pub fn from_display(e: impl std::fmt::Display) -> Self {
        Self {
            cause: e.to_string(),
        }
    }
}

impl std::fmt::Display for PollError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "poll error: {}", self.cause)
    }
}

impl std::error::Error for PollError {}

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
// URL normalization
// ---------------------------------------------------------------------------

/// Normalize a server URL by trimming whitespace, ensuring `https://` scheme,
/// and stripping trailing slashes.
///
/// Ported from: `packages/core/src/account.ts` — `normalizeServerUrl()`.
pub fn normalize_server_url(url: &str) -> Result<String, AccountError> {
    let trimmed = url.trim().to_string();
    if trimmed.is_empty() {
        return Err(AccountError::ServiceError(AccountServiceError {
            message: "server URL must not be empty".to_string(),
            cause: None,
        }));
    }

    let lowered = trimmed.to_lowercase();

    let with_scheme = if lowered.starts_with("https://") || lowered.starts_with("http://") {
        trimmed
    } else {
        format!("https://{trimmed}")
    };

    let final_url = with_scheme.trim_end_matches('/').to_string();

    let lower_final = final_url.to_lowercase();
    if !lower_final.starts_with("https://") {
        return Err(AccountError::ServiceError(AccountServiceError {
            message: "server URL must use HTTPS".to_string(),
            cause: Some(format!("got {final_url}")),
        }));
    }

    Ok(final_url)
}

// ---------------------------------------------------------------------------
// SQLite table row types
// ---------------------------------------------------------------------------

/// A row from the `account` SQLite table.
///
/// Ported from: `packages/core/src/account/sql.ts` — `AccountTable`
/// (`sqliteTable("account", ...)` via drizzle-orm).
#[derive(Debug, Clone, Serialize, Deserialize, sqlx::FromRow)]
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
#[derive(Debug, Clone, Serialize, Deserialize, sqlx::FromRow)]
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

/// Internal response type for the `/api/user` endpoint.
#[derive(Debug, Clone, Serialize, Deserialize)]
struct UserInfo {
    /// Unique account identifier returned by the server.
    pub id: AccountId,
    /// Email address of the authenticated user.
    pub email: String,
}

/// How many milliseconds before actual expiry to eagerly refresh a token.
///
/// Ported from: `packages/opencode/src/account/account.ts` —
/// `eagerRefreshThresholdMs`.
const EAGER_REFRESH_THRESHOLD_MS: i64 = 300_000;

// ---------------------------------------------------------------------------
// Service layer
// ---------------------------------------------------------------------------

/// In-memory account service managing OAuth device flow, token storage,
/// and active-account selection.
///
/// Ported from: `packages/core/src/account.ts` — `AccountService` class.
pub struct AccountService {
    /// Base URL of the authentication server.
    server_url: String,
    /// HTTP client for server communication.
    http_client: reqwest::Client,
    /// All known accounts, keyed by ID.
    accounts: Arc<tokio::sync::RwLock<Vec<AccountTableRow>>>,
    /// Singleton state row tracking the active account/org.
    state: Arc<tokio::sync::RwLock<AccountStateTableRow>>,
    /// Per-account mutexes that serialise concurrent token refreshes so only
    /// one request hits the wire at a time (dedup).
    ///
    /// Ported from: `packages/opencode/src/account/account.ts` —
    /// `refreshTokenCache` (`Cache.make` with `capacity: Infinity`).
    refresh_locks: Arc<tokio::sync::Mutex<HashMap<AccountId, Arc<tokio::sync::Mutex<()>>>>>,
}

impl AccountService {
    /// Convert a `reqwest::Error` into an `AccountError::TransportError`.
    ///
    /// Ported from: `packages/core/src/account.ts` — static helper for
    /// wrapping HTTP client errors into the account error hierarchy.
    pub fn from_http_client_error(method: &str, url: &str, err: reqwest::Error) -> AccountError {
        AccountError::TransportError(AccountTransportError {
            method: method.to_string(),
            url: url.to_string(),
            description: Some(err.to_string()),
            cause: Some(err.to_string()),
        })
    }

    /// Create a new `AccountService` with the given server URL.
    ///
    /// Initialises an empty account list and a default state row with no
    /// active account or organisation.
    pub fn new(server_url: String) -> Self {
        Self {
            server_url,
            http_client: reqwest::Client::new(),
            accounts: Arc::new(tokio::sync::RwLock::new(Vec::new())),
            state: Arc::new(tokio::sync::RwLock::new(AccountStateTableRow {
                id: 1,
                active_account_id: None,
                active_org_id: None,
            })),
            refresh_locks: Arc::new(tokio::sync::Mutex::new(HashMap::new())),
        }
    }

    /// Initiate the OAuth device authorisation flow.
    ///
    /// POSTs to `{server}/auth/device/code` and returns the login data
    /// containing the device code, user code, and verification URL.
    ///
    /// Ported from: `packages/core/src/account.ts` — `login()`.
    pub async fn login(&self) -> Result<AccountLogin, AccountError> {
        let url = format!("{}/auth/device/code", self.server_url);

        let response = self
            .http_client
            .post(&url)
            .json(&serde_json::json!({ "client_id": "opencode-cli" }))
            .send()
            .await
            .map_err(|e| {
                AccountError::TransportError(AccountTransportError {
                    method: "POST".to_string(),
                    url: url.clone(),
                    description: Some(e.to_string()),
                    cause: Some(e.to_string()),
                })
            })?;

        let status = response.status();
        if !status.is_success() {
            let body = response.text().await.unwrap_or_default();
            return Err(AccountError::ServiceError(AccountServiceError {
                message: format!("device code request failed with status {status}"),
                cause: Some(body),
            }));
        }

        let login: AccountLogin = response.json().await.map_err(|e| {
            AccountError::ServiceError(AccountServiceError {
                message: "failed to parse device code response".to_string(),
                cause: Some(e.to_string()),
            })
        })?;

        Ok(login)
    }

    /// Poll the server for device authorisation completion.
    ///
    /// POSTs to `{server}/auth/device/token` with the device code and
    /// returns the tagged `PollResult` union. On success the account is
    /// automatically persisted into the in-memory store along with the
    /// active organisation.
    ///
    /// Ported from: `packages/core/src/account.ts` — `poll()`.
    pub async fn poll(&self, code: DeviceCode) -> Result<PollResult, AccountError> {
        let url = format!("{}/auth/device/token", self.server_url);

        let response = self
            .http_client
            .post(&url)
            .json(&serde_json::json!({
                "grant_type": "urn:ietf:params:oauth:grant-type:device_code",
                "device_code": code,
                "client_id": "opencode-cli",
            }))
            .send()
            .await
            .map_err(|e| {
                AccountError::TransportError(AccountTransportError {
                    method: "POST".to_string(),
                    url: url.clone(),
                    description: Some(e.to_string()),
                    cause: Some(e.to_string()),
                })
            })?;

        let status = response.status();
        if !status.is_success() {
            let body = response.text().await.unwrap_or_default();
            return Err(AccountError::ServiceError(AccountServiceError {
                message: format!("device token poll failed with status {status}"),
                cause: Some(body),
            }));
        }

        let value: serde_json::Value = response.json().await.map_err(|e| {
            AccountError::ServiceError(AccountServiceError {
                message: "failed to parse poll response".to_string(),
                cause: Some(e.to_string()),
            })
        })?;

        // Check for OAuth error response
        if let Some(error) = value.get("error").and_then(|v| v.as_str()) {
            return Ok(match error {
                "authorization_pending" => PollResult::Pending(PollPending {}),
                "slow_down" => PollResult::Slow(PollSlow {}),
                "expired_token" => PollResult::Expired(PollExpired {}),
                "access_denied" => PollResult::Denied(PollDenied {}),
                _ => PollResult::Error(PollError {
                    cause: error.to_string(),
                }),
            });
        }

        // Success path — extract tokens
        let access_token = value
            .get("access_token")
            .and_then(|v| v.as_str())
            .ok_or_else(|| {
                AccountError::ServiceError(AccountServiceError {
                    message: "poll response missing access_token".to_string(),
                    cause: None,
                })
            })?
            .to_string();

        let refresh_token = value
            .get("refresh_token")
            .and_then(|v| v.as_str())
            .ok_or_else(|| {
                AccountError::ServiceError(AccountServiceError {
                    message: "poll response missing refresh_token".to_string(),
                    cause: None,
                })
            })?
            .to_string();

        let expires_in = value
            .get("expires_in")
            .and_then(|v| v.as_i64())
            .unwrap_or(3600);

        // Fetch user info and orgs concurrently
        let (user_info, orgs) = tokio::try_join!(
            self.fetch_user(&access_token),
            self.fetch_orgs(&access_token),
        )?;

        let first_org_id = orgs.first().map(|o| o.id.clone());

        let now_ms = chrono::Utc::now().timestamp_millis();
        let expiry = now_ms + expires_in * 1000;
        let now_str = now_ms.to_string();

        // Persist account into in-memory store
        let mut accounts = self.accounts.write().await;
        accounts.push(AccountTableRow {
            id: user_info.id.clone(),
            email: user_info.email.clone(),
            url: self.server_url.clone(),
            access_token: access_token.clone(),
            refresh_token: refresh_token.clone(),
            token_expiry: Some(expiry),
            time_created: now_str.clone(),
            time_updated: now_str,
        });
        drop(accounts);

        // Set active account and org
        let mut state = self.state.write().await;
        state.active_account_id = Some(user_info.id);
        state.active_org_id = first_org_id;

        Ok(PollResult::Success(PollSuccess {
            email: user_info.email,
        }))
    }

    /// Fetch user info from the server using an access token.
    ///
    /// GETs `{server}/api/user` with a Bearer token.
    ///
    /// Ported from: `packages/opencode/src/account/account.ts` — `fetchUser`.
    async fn fetch_user(&self, access_token: &str) -> Result<UserInfo, AccountError> {
        let url = format!("{}/api/user", self.server_url);

        let response = self
            .http_client
            .get(&url)
            .bearer_auth(access_token)
            .send()
            .await
            .map_err(|e| {
                AccountError::TransportError(AccountTransportError {
                    method: "GET".to_string(),
                    url: url.clone(),
                    description: Some(e.to_string()),
                    cause: Some(e.to_string()),
                })
            })?;

        let status = response.status();
        if !status.is_success() {
            let body = response.text().await.unwrap_or_default();
            return Err(AccountError::ServiceError(AccountServiceError {
                message: format!("fetch user failed with status {status}"),
                cause: Some(body),
            }));
        }

        let user: UserInfo = response.json().await.map_err(|e| {
            AccountError::ServiceError(AccountServiceError {
                message: "failed to parse user response".to_string(),
                cause: Some(e.to_string()),
            })
        })?;

        Ok(user)
    }

    /// Fetch organisations for the authenticated user.
    ///
    /// GETs `{server}/api/orgs` with a Bearer token.
    ///
    /// Ported from: `packages/opencode/src/account/account.ts` — `fetchOrgs`.
    async fn fetch_orgs(&self, access_token: &str) -> Result<Vec<AccountOrg>, AccountError> {
        let url = format!("{}/api/orgs", self.server_url);

        let response = self
            .http_client
            .get(&url)
            .bearer_auth(access_token)
            .send()
            .await
            .map_err(|e| {
                AccountError::TransportError(AccountTransportError {
                    method: "GET".to_string(),
                    url: url.clone(),
                    description: Some(e.to_string()),
                    cause: Some(e.to_string()),
                })
            })?;

        let status = response.status();
        if !status.is_success() {
            let body = response.text().await.unwrap_or_default();
            return Err(AccountError::ServiceError(AccountServiceError {
                message: format!("fetch orgs failed with status {status}"),
                cause: Some(body),
            }));
        }

        let orgs: Vec<AccountOrg> = response.json().await.map_err(|e| {
            AccountError::ServiceError(AccountServiceError {
                message: "failed to parse orgs response".to_string(),
                cause: Some(e.to_string()),
            })
        })?;

        Ok(orgs)
    }

    /// Retrieve the access token for `account_id`, refreshing it if expired.
    ///
    /// Checks `token_expiry` against the current Unix timestamp (milliseconds).
    /// If the token is expired or missing, attempts a refresh using the
    /// stored refresh token via `{server}/auth/device/token`.
    ///
    /// Ported from: `packages/core/src/account.ts` — `token()`.
    pub async fn token(&self, account_id: &AccountId) -> Result<AccessToken, AccountError> {
        // Check current token freshness (eager refresh window matches TS)
        let (needs_refresh, current_token) = {
            let accounts = self.accounts.read().await;
            let account = accounts
                .iter()
                .find(|a| a.id == *account_id)
                .ok_or_else(|| {
                    AccountError::RepoError(AccountRepoError {
                        message: format!("account {account_id} not found"),
                        cause: None,
                    })
                })?;

            let now_ms = chrono::Utc::now().timestamp_millis();
            let needs = account
                .token_expiry
                .map(|exp| exp <= now_ms + EAGER_REFRESH_THRESHOLD_MS)
                .unwrap_or(true);

            (needs, account.access_token.clone())
        };

        if !needs_refresh {
            return Ok(current_token);
        }

        // Serialise refreshes per account so concurrent callers share one
        // in-flight refresh (port of Cache.make with capacity: Infinity).
        let lock = {
            let mut locks = self.refresh_locks.lock().await;
            locks
                .entry(account_id.clone())
                .or_insert_with(|| Arc::new(tokio::sync::Mutex::new(())))
                .clone()
        };
        let _guard = lock.lock().await;

        // Double-check: another thread may have refreshed while we waited
        let still_needs = {
            let accounts = self.accounts.read().await;
            accounts
                .iter()
                .find(|a| a.id == *account_id)
                .map(|a| {
                    let now_ms = chrono::Utc::now().timestamp_millis();
                    a.token_expiry
                        .map(|exp| exp <= now_ms + EAGER_REFRESH_THRESHOLD_MS)
                        .unwrap_or(true)
                })
                .unwrap_or(false)
        };

        if still_needs {
            self.refresh_token(account_id).await?;
        }

        let accounts = self.accounts.read().await;
        let account = accounts.iter().find(|a| a.id == *account_id).ok_or_else(|| {
            AccountError::RepoError(AccountRepoError {
                message: format!("account {account_id} not found after refresh"),
                cause: None,
            })
        })?;

        Ok(account.access_token.clone())
    }

    /// Return the currently active account as an `AccountInfo`, if any.
    ///
    /// Ported from: `packages/core/src/account.ts` — `active()`.
    pub async fn active(&self) -> Result<Option<AccountInfo>, AccountError> {
        let state = self.state.read().await;
        let active_id = match &state.active_account_id {
            Some(id) => id.clone(),
            None => return Ok(None),
        };
        drop(state);

        let active_org_id = state_from_accounts(&self.state).await;
        let accounts = self.accounts.read().await;
        let account = accounts.iter().find(|a| a.id == active_id);

        Ok(account.map(|a| AccountInfo {
            id: a.id.clone(),
            email: a.email.clone(),
            url: a.url.clone(),
            active_org_id: active_org_id.clone(),
        }))
    }

    /// Return all stored accounts as `AccountInfo` values.
    ///
    /// Ported from: `packages/core/src/account.ts` — `list()`.
    pub async fn list(&self) -> Result<Vec<AccountInfo>, AccountError> {
        let accounts = self.accounts.read().await;
        let org_id = state_from_accounts(&self.state).await;

        let infos = accounts
            .iter()
            .map(|a| AccountInfo {
                id: a.id.clone(),
                email: a.email.clone(),
                url: a.url.clone(),
                active_org_id: org_id.clone(),
            })
            .collect();

        Ok(infos)
    }

    /// Set the active account to `account_id`.
    ///
    /// Resets the active organisation to `None`.
    ///
    /// Ported from: `packages/core/src/account.ts` — `useAccount()`.
    pub async fn use_account(&self, account_id: AccountId) -> Result<(), AccountError> {
        let accounts = self.accounts.read().await;
        if !accounts.iter().any(|a| a.id == account_id) {
            return Err(AccountError::RepoError(AccountRepoError {
                message: format!("account {account_id} not found"),
                cause: None,
            }));
        }
        drop(accounts);

        let mut state = self.state.write().await;
        state.active_account_id = Some(account_id);
        state.active_org_id = None;
        Ok(())
    }

    /// Remove an account by ID.
    ///
    /// If it is the active account, the active selection is cleared.
    ///
    /// Ported from: `packages/core/src/account.ts` — `remove()`.
    pub async fn remove(&self, account_id: &AccountId) -> Result<(), AccountError> {
        let mut accounts = self.accounts.write().await;
        let before_len = accounts.len();
        accounts.retain(|a| a.id != *account_id);

        if accounts.len() == before_len {
            return Err(AccountError::RepoError(AccountRepoError {
                message: format!("account {account_id} not found"),
                cause: None,
            }));
        }
        drop(accounts);

        let mut state = self.state.write().await;
        if state.active_account_id.as_ref() == Some(account_id) {
            state.active_account_id = None;
            state.active_org_id = None;
        }
        Ok(())
    }

    /// List organisations belonging to `account_id`.
    ///
    /// POSTs to `{server}/account/{account_id}/orgs` using the account's
    /// access token.
    ///
    /// Ported from: `packages/core/src/account.ts` — `orgs()`.
    pub async fn orgs(&self, account_id: &AccountId) -> Result<Vec<AccountOrg>, AccountError> {
        let access_token = self.token(account_id).await?;
        let url = format!("{}/account/{}/orgs", self.server_url, account_id);

        let response = self
            .http_client
            .get(&url)
            .bearer_auth(&access_token)
            .send()
            .await
            .map_err(|e| {
                AccountError::TransportError(AccountTransportError {
                    method: "GET".to_string(),
                    url: url.clone(),
                    description: Some(e.to_string()),
                    cause: Some(e.to_string()),
                })
            })?;

        let status = response.status();
        if !status.is_success() {
            let body = response.text().await.unwrap_or_default();
            return Err(AccountError::ServiceError(AccountServiceError {
                message: format!("orgs request failed with status {status}"),
                cause: Some(body),
            }));
        }

        let orgs: Vec<AccountOrg> = response.json().await.map_err(|e| {
            AccountError::ServiceError(AccountServiceError {
                message: "failed to parse orgs response".to_string(),
                cause: Some(e.to_string()),
            })
        })?;

        Ok(orgs)
    }

    /// Fetch organisations for all stored accounts.
    ///
    /// Returns a list of (account, orgs) pairs. If org fetching fails for an
    /// account, it is omitted silently.
    ///
    /// Ported from: `packages/opencode/src/account/account.ts` lines 329–340.
    pub async fn orgs_by_account(&self) -> Vec<(AccountInfo, Vec<AccountOrg>)> {
        let accounts = {
            let list = self.accounts.read().await;
            list.clone()
        };

        let mut results = Vec::new();
        for account in &accounts {
            let account_info = AccountInfo {
                id: account.id.clone(),
                email: account.email.clone(),
                url: account.url.clone(),
                active_org_id: None,
            };
            match self.orgs(&account.id).await {
                Ok(orgs) => results.push((account_info, orgs)),
                Err(_) => continue,
            }
        }
        results
    }

    /// Fetch remote account configuration for a specific org.
    ///
    /// GETs `{server_url}/api/config` with `x-org-id` header and Bearer auth.
    /// Returns `None` if the server responds with 404.
    ///
    /// Ported from: `packages/opencode/src/account/account.ts` lines 351–373.
    pub async fn config(
        &self,
        account_id: &AccountId,
        org_id: &OrgId,
    ) -> Result<Option<HashMap<String, serde_json::Value>>, AccountError> {
        let resolved = self.resolve_access(account_id).await?;
        let (account, access_token) = match resolved {
            Some(pair) => pair,
            None => return Ok(None),
        };

        let url = format!("{}/api/config", account.url);

        let response = self
            .http_client
            .get(&url)
            .header("Accept", "application/json")
            .header("x-org-id", org_id.as_str())
            .bearer_auth(&access_token)
            .send()
            .await
            .map_err(|e| {
                AccountError::TransportError(AccountTransportError {
                    method: "GET".to_string(),
                    url: url.clone(),
                    description: Some(e.to_string()),
                    cause: Some(e.to_string()),
                })
            })?;

        if response.status() == 404 {
            return Ok(None);
        }

        let status = response.status();
        if !status.is_success() {
            let body = response.text().await.unwrap_or_default();
            return Err(AccountError::ServiceError(AccountServiceError {
                message: format!("config fetch failed with status {status}"),
                cause: Some(body),
            }));
        }

        #[derive(Deserialize)]
        struct RemoteConfigResponse {
            #[serde(default)]
            config: HashMap<String, serde_json::Value>,
        }

        let parsed: RemoteConfigResponse = response.json().await.map_err(|e| {
            AccountError::ServiceError(AccountServiceError {
                message: "failed to parse config response".to_string(),
                cause: Some(e.to_string()),
            })
        })?;

        Ok(Some(parsed.config))
    }

    /// Resolve account + access token for a given account ID.
    async fn resolve_access(
        &self,
        account_id: &AccountId,
    ) -> Result<Option<(AccountTableRow, AccessToken)>, AccountError> {
        let token = self.token(account_id).await?;
        let accounts = self.accounts.read().await;
        let account = accounts.iter().find(|a| a.id == *account_id).cloned();
        match account {
            Some(acct) => Ok(Some((acct, token))),
            None => Ok(None),
        }
    }

    /// Store or update access/refresh tokens for `account_id`.
    ///
    /// If the account already exists its tokens are overwritten; otherwise
    /// a new row is appended.
    ///
    /// Ported from: `packages/core/src/account.ts` — `persistToken()`.
    pub async fn persist_token(
        &self,
        account_id: AccountId,
        access: AccessToken,
        refresh: RefreshToken,
        expiry: Option<i64>,
    ) -> Result<(), AccountError> {
        let now = chrono::Utc::now().timestamp_millis().to_string();
        let mut accounts = self.accounts.write().await;

        if let Some(existing) = accounts.iter_mut().find(|a| a.id == account_id) {
            existing.access_token = access;
            existing.refresh_token = refresh;
            existing.token_expiry = expiry;
            existing.time_updated = now;
        } else {
            return Err(AccountError::RepoError(AccountRepoError {
                message: format!("account {account_id} not found"),
                cause: None,
            }));
        }
        Ok(())
    }

    /// (Internal) Refresh an expired access token.
    ///
    /// POSTs to `{url}/auth/device/token` with `grant_type: refresh_token`.
    async fn refresh_token(&self, account_id: &AccountId) -> Result<(), AccountError> {
        let (refresh_token, url) = {
            let accounts = self.accounts.read().await;
            let account = accounts
                .iter()
                .find(|a| a.id == *account_id)
                .ok_or_else(|| {
                    AccountError::RepoError(AccountRepoError {
                        message: format!("account {account_id} not found"),
                        cause: None,
                    })
                })?;
            (account.refresh_token.clone(), account.url.clone())
        };

        let refresh_url = format!("{url}/auth/device/token");
        let response = self
            .http_client
            .post(&refresh_url)
            .json(&serde_json::json!({
                "grant_type": "refresh_token",
                "refresh_token": refresh_token,
                "client_id": "opencode-cli",
            }))
            .send()
            .await
            .map_err(|e| {
                AccountError::TransportError(AccountTransportError {
                    method: "POST".to_string(),
                    url: refresh_url.clone(),
                    description: Some(e.to_string()),
                    cause: Some(e.to_string()),
                })
            })?;

        let status = response.status();
        if !status.is_success() {
            let body = response.text().await.unwrap_or_default();
            return Err(AccountError::ServiceError(AccountServiceError {
                message: format!("token refresh failed with status {status}"),
                cause: Some(body),
            }));
        }

        #[derive(Deserialize)]
        struct TokenResponse {
            access_token: AccessToken,
            refresh_token: RefreshToken,
            expires_in: Option<i64>,
        }

        let token_resp: TokenResponse = response.json().await.map_err(|e| {
            AccountError::ServiceError(AccountServiceError {
                message: "failed to parse token refresh response".to_string(),
                cause: Some(e.to_string()),
            })
        })?;

        let expiry = token_resp
            .expires_in
            .map(|secs| chrono::Utc::now().timestamp_millis() + secs * 1000);

        let now = chrono::Utc::now().timestamp_millis().to_string();
        let mut accounts = self.accounts.write().await;
        if let Some(account) = accounts.iter_mut().find(|a| a.id == *account_id) {
            account.access_token = token_resp.access_token;
            account.refresh_token = token_resp.refresh_token;
            account.token_expiry = expiry;
            account.time_updated = now;
        }

        Ok(())
    }
}

/// Helper to read the active org ID from the state.
async fn state_from_accounts(
    state: &Arc<tokio::sync::RwLock<AccountStateTableRow>>,
) -> Option<OrgId> {
    let s = state.read().await;
    s.active_org_id.clone()
}

// ---------------------------------------------------------------------------
// SQLite persistence — AccountRepo
// ---------------------------------------------------------------------------

/// SQLite-backed account repository using `sqlx`.
///
/// Ported from: `packages/core/src/account/sql.ts` — Drizzle ORM operations
/// over the `account` and `account_state` tables.
pub struct AccountRepo {
    pool: sqlx::SqlitePool,
}

impl AccountRepo {
    /// Create a new `AccountRepo` backed by the given connection pool.
    pub fn new(pool: sqlx::SqlitePool) -> Self {
        Self { pool }
    }

    /// Return the currently active account row, if any.
    ///
    /// Queries `account_state` for the active account ID, then fetches the
    /// corresponding `account` row.
    pub async fn active(&self) -> Result<Option<AccountTableRow>, AccountError> {
        let state_row: Option<AccountStateTableRow> = sqlx::query_as(
            "SELECT id, active_account_id, active_org_id FROM account_state WHERE id = 1",
        )
        .fetch_optional(&self.pool)
        .await
        .map_err(|e| {
            AccountError::RepoError(AccountRepoError {
                message: "failed to query account_state".to_string(),
                cause: Some(e.to_string()),
            })
        })?;

        let active_id = match state_row {
            Some(s) => match s.active_account_id {
                Some(id) => id,
                None => return Ok(None),
            },
            None => return Ok(None),
        };

        let row: Option<AccountTableRow> = sqlx::query_as(
            "SELECT id, email, url, access_token, refresh_token, token_expiry, time_created, time_updated FROM account WHERE id = ?",
        )
        .bind(&active_id)
        .fetch_optional(&self.pool)
        .await
        .map_err(|e| AccountError::RepoError(AccountRepoError {
            message: format!("failed to query account {active_id}"),
            cause: Some(e.to_string()),
        }))?;

        Ok(row)
    }

    /// List all stored account rows.
    pub async fn list(&self) -> Result<Vec<AccountTableRow>, AccountError> {
        let rows: Vec<AccountTableRow> = sqlx::query_as(
            "SELECT id, email, url, access_token, refresh_token, token_expiry, time_created, time_updated FROM account",
        )
        .fetch_all(&self.pool)
        .await
        .map_err(|e| AccountError::RepoError(AccountRepoError {
            message: "failed to list accounts".to_string(),
            cause: Some(e.to_string()),
        }))?;

        Ok(rows)
    }

    /// Remove an account by ID.
    ///
    /// Also clears the active account if it matches.
    pub async fn remove(&self, account_id: &str) -> Result<(), AccountError> {
        let result = sqlx::query("DELETE FROM account WHERE id = ?")
            .bind(account_id)
            .execute(&self.pool)
            .await
            .map_err(|e| {
                AccountError::RepoError(AccountRepoError {
                    message: format!("failed to remove account {account_id}"),
                    cause: Some(e.to_string()),
                })
            })?;

        if result.rows_affected() == 0 {
            return Err(AccountError::RepoError(AccountRepoError {
                message: format!("account {account_id} not found"),
                cause: None,
            }));
        }

        sqlx::query(
            "UPDATE account_state SET active_account_id = NULL, active_org_id = NULL WHERE active_account_id = ?",
        )
        .bind(account_id)
        .execute(&self.pool)
        .await
        .map_err(|e| AccountError::RepoError(AccountRepoError {
            message: "failed to clear active account state".to_string(),
            cause: Some(e.to_string()),
        }))?;

        Ok(())
    }

    /// Set the active account to `account_id`.
    pub async fn use_account(&self, account_id: &str) -> Result<(), AccountError> {
        let exists: Option<(String,)> = sqlx::query_as("SELECT id FROM account WHERE id = ?")
            .bind(account_id)
            .fetch_optional(&self.pool)
            .await
            .map_err(|e| {
                AccountError::RepoError(AccountRepoError {
                    message: format!("failed to check account {account_id}"),
                    cause: Some(e.to_string()),
                })
            })?;

        if exists.is_none() {
            return Err(AccountError::RepoError(AccountRepoError {
                message: format!("account {account_id} not found"),
                cause: None,
            }));
        }

        sqlx::query(
            "INSERT INTO account_state (id, active_account_id, active_org_id) VALUES (1, ?, NULL) \
             ON CONFLICT(id) DO UPDATE SET active_account_id = excluded.active_account_id, active_org_id = NULL",
        )
        .bind(account_id)
        .execute(&self.pool)
        .await
        .map_err(|e| AccountError::RepoError(AccountRepoError {
            message: "failed to update active account state".to_string(),
            cause: Some(e.to_string()),
        }))?;

        Ok(())
    }

    /// Fetch a single account row by ID.
    pub async fn get_row(&self, account_id: &str) -> Result<Option<AccountTableRow>, AccountError> {
        let row: Option<AccountTableRow> = sqlx::query_as(
            "SELECT id, email, url, access_token, refresh_token, token_expiry, time_created, time_updated FROM account WHERE id = ?",
        )
        .bind(account_id)
        .fetch_optional(&self.pool)
        .await
        .map_err(|e| AccountError::RepoError(AccountRepoError {
            message: format!("failed to get account {account_id}"),
            cause: Some(e.to_string()),
        }))?;

        Ok(row)
    }

    /// Store or update access/refresh tokens for an existing account.
    pub async fn persist_token(
        &self,
        account_id: &str,
        access: &str,
        refresh: &str,
        expiry: Option<i64>,
    ) -> Result<(), AccountError> {
        let now = chrono::Utc::now().timestamp_millis().to_string();
        let result = sqlx::query(
            "UPDATE account SET access_token = ?, refresh_token = ?, token_expiry = ?, time_updated = ? WHERE id = ?",
        )
        .bind(access)
        .bind(refresh)
        .bind(expiry)
        .bind(&now)
        .bind(account_id)
        .execute(&self.pool)
        .await
        .map_err(|e| AccountError::RepoError(AccountRepoError {
            message: format!("failed to persist token for account {account_id}"),
            cause: Some(e.to_string()),
        }))?;

        if result.rows_affected() == 0 {
            return Err(AccountError::RepoError(AccountRepoError {
                message: format!("account {account_id} not found"),
                cause: None,
            }));
        }

        Ok(())
    }

    /// Insert a new account row with tokens.
    pub async fn persist_account(
        &self,
        info: &AccountInfo,
        access: &str,
        refresh: &str,
    ) -> Result<(), AccountError> {
        let now = chrono::Utc::now().timestamp_millis().to_string();
        sqlx::query(
            "INSERT INTO account (id, email, url, access_token, refresh_token, token_expiry, time_created, time_updated) \
             VALUES (?, ?, ?, ?, ?, NULL, ?, ?)",
        )
        .bind(&info.id)
        .bind(&info.email)
        .bind(&info.url)
        .bind(access)
        .bind(refresh)
        .bind(&now)
        .bind(&now)
        .execute(&self.pool)
        .await
        .map_err(|e| AccountError::RepoError(AccountRepoError {
            message: format!("failed to persist account {}", info.id),
            cause: Some(e.to_string()),
        }))?;

        Ok(())
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::Duration;

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
            expiry: Duration::minutes(15),
            interval: Duration::seconds(5),
        };

        let json = serde_json::to_string(&login).expect("serialize AccountLogin");
        let parsed: AccountLogin = serde_json::from_str(&json).expect("deserialize AccountLogin");

        assert_eq!(parsed.code, "device-code-abc");
        assert_eq!(parsed.user, "USR-XYZ");
        assert_eq!(parsed.url, "https://example.com/verify");
        assert_eq!(parsed.server, "example.com");
        assert_eq!(parsed.expiry, Duration::minutes(15));
        assert_eq!(parsed.interval, Duration::seconds(5));
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
        let parsed: PollResult =
            serde_json::from_str(json).expect("deserialize PollResult::Success");

        match parsed {
            PollResult::Success(s) => assert_eq!(s.email, "dev@example.com"),
            other => panic!("expected PollSuccess, got {other:?}"),
        }
    }

    #[test]
    fn test_poll_result_deserialize_denied() {
        let json = r#"{"type":"PollDenied"}"#;
        let parsed: PollResult =
            serde_json::from_str(json).expect("deserialize PollResult::Denied");

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

    #[test]
    fn test_poll_error_from_error() {
        let source = std::io::Error::new(std::io::ErrorKind::TimedOut, "request timed out");
        let poll_err = PollError::from_error(&source);
        assert_eq!(poll_err.cause, "request timed out");

        let display_err = PollError::from_display(format!("status {0}", 503));
        assert_eq!(display_err.cause, "status 503");
    }

    #[test]
    fn test_poll_error_display() {
        let err = PollError {
            cause: "connection reset".to_string(),
        };
        assert_eq!(err.to_string(), "poll error: connection reset");
    }

    #[test]
    fn test_poll_error_is_std_error() {
        let err = PollError {
            cause: "oops".to_string(),
        };
        let std_err: &dyn std::error::Error = &err;
        assert_eq!(std_err.to_string(), "poll error: oops");
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
        let parsed: AccountTableRow =
            serde_json::from_str(&json).expect("deserialize AccountTableRow");

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

    // ===================================================================
    // AccountService tests (mock data, no real HTTP)
    // ===================================================================

    use tokio::sync::RwLock;

    /// Helper: create a service with pre-loaded accounts and state.
    fn service_with_accounts(
        accounts: Vec<AccountTableRow>,
        active_account_id: Option<AccountId>,
        active_org_id: Option<OrgId>,
    ) -> AccountService {
        AccountService {
            server_url: "https://auth.example.com".to_string(),
            http_client: reqwest::Client::new(),
            accounts: Arc::new(RwLock::new(accounts)),
            state: Arc::new(RwLock::new(AccountStateTableRow {
                id: 1,
                active_account_id,
                active_org_id,
            })),
            refresh_locks: Arc::new(tokio::sync::Mutex::new(HashMap::new())),
        }
    }

    fn make_account(id: &str, email: &str) -> AccountTableRow {
        AccountTableRow {
            id: id.to_string(),
            email: email.to_string(),
            url: "https://api.example.com".to_string(),
            access_token: format!("tok_acc_{id}"),
            refresh_token: format!("tok_ref_{id}"),
            token_expiry: Some(chrono::Utc::now().timestamp_millis() + 3_600_000),
            time_created: "1718000000000".to_string(),
            time_updated: "1718000000000".to_string(),
        }
    }

    #[tokio::test]
    async fn test_new_service_has_empty_accounts() {
        let svc = AccountService::new("https://auth.example.com".to_string());
        let accounts = svc.accounts.read().await;
        assert!(accounts.is_empty());
        let state = svc.state.read().await;
        assert!(state.active_account_id.is_none());
        assert!(state.active_org_id.is_none());
    }

    #[tokio::test]
    async fn test_active_returns_none_when_no_active() {
        let svc = service_with_accounts(vec![], None, None);
        assert!(svc.active().await.unwrap().is_none());
    }

    #[tokio::test]
    async fn test_active_returns_account_info() {
        let acct = make_account("acct_1", "alice@example.com");
        let svc = service_with_accounts(vec![acct], Some("acct_1".to_string()), None);

        let active = svc.active().await.unwrap();
        assert!(active.is_some());
        let info = active.unwrap();
        assert_eq!(info.id, "acct_1");
        assert_eq!(info.email, "alice@example.com");
    }

    #[tokio::test]
    async fn test_list_returns_all_accounts() {
        let acct1 = make_account("a1", "one@example.com");
        let acct2 = make_account("a2", "two@example.com");
        let svc = service_with_accounts(vec![acct1, acct2], None, None);

        let list = svc.list().await.unwrap();
        assert_eq!(list.len(), 2);
        assert_eq!(list[0].id, "a1");
        assert_eq!(list[1].id, "a2");
    }

    #[tokio::test]
    async fn test_use_account_sets_active() {
        let acct = make_account("acct_1", "alice@example.com");
        let svc = service_with_accounts(vec![acct], None, None);

        svc.use_account("acct_1".to_string()).await.unwrap();

        let state = svc.state.read().await;
        assert_eq!(state.active_account_id.as_deref(), Some("acct_1"));
        assert!(state.active_org_id.is_none());
    }

    #[tokio::test]
    async fn test_use_account_unknown_returns_error() {
        let svc = service_with_accounts(vec![], None, None);

        let result = svc.use_account("nonexistent".to_string()).await;
        assert!(result.is_err());
        match result.unwrap_err() {
            AccountError::RepoError(e) => assert!(e.message.contains("not found")),
            other => panic!("expected RepoError, got {other:?}"),
        }
    }

    #[tokio::test]
    async fn test_remove_deletes_account() {
        let acct = make_account("acct_1", "alice@example.com");
        let svc = service_with_accounts(vec![acct], None, None);

        svc.remove(&"acct_1".to_string()).await.unwrap();

        let accounts = svc.accounts.read().await;
        assert!(accounts.is_empty());
    }

    #[tokio::test]
    async fn test_remove_clears_active_if_removed() {
        let acct = make_account("acct_1", "alice@example.com");
        let svc = service_with_accounts(vec![acct], Some("acct_1".to_string()), None);

        svc.remove(&"acct_1".to_string()).await.unwrap();

        let state = svc.state.read().await;
        assert!(state.active_account_id.is_none());
    }

    #[tokio::test]
    async fn test_remove_unknown_returns_error() {
        let svc = service_with_accounts(vec![], None, None);
        let result = svc.remove(&"nonexistent".to_string()).await;
        assert!(result.is_err());
    }

    #[tokio::test]
    async fn test_persist_token_updates_existing() {
        let acct = make_account("acct_1", "alice@example.com");
        let svc = service_with_accounts(vec![acct], None, None);

        svc.persist_token(
            "acct_1".to_string(),
            "new_access".to_string(),
            "new_refresh".to_string(),
            Some(9999),
        )
        .await
        .unwrap();

        let accounts = svc.accounts.read().await;
        let a = accounts.iter().find(|a| a.id == "acct_1").unwrap();
        assert_eq!(a.access_token, "new_access");
        assert_eq!(a.refresh_token, "new_refresh");
        assert_eq!(a.token_expiry, Some(9999));
    }

    #[tokio::test]
    async fn test_persist_token_unknown_account_returns_error() {
        let svc = service_with_accounts(vec![], None, None);
        let result = svc
            .persist_token("ghost".to_string(), "a".to_string(), "r".to_string(), None)
            .await;
        assert!(result.is_err());
    }

    #[tokio::test]
    async fn test_token_returns_valid_token() {
        let acct = make_account("acct_1", "alice@example.com");
        let svc = service_with_accounts(vec![acct], None, None);

        let token = svc.token(&"acct_1".to_string()).await.unwrap();
        assert_eq!(token, "tok_acc_acct_1");
    }

    #[tokio::test]
    async fn test_token_unknown_account_returns_error() {
        let svc = service_with_accounts(vec![], None, None);
        let result = svc.token(&"ghost".to_string()).await;
        assert!(result.is_err());
    }

    // ===================================================================
    // normalize_server_url
    // ===================================================================

    #[test]
    fn test_normalize_server_url_https_passthrough() {
        let url = normalize_server_url("https://api.example.com").unwrap();
        assert_eq!(url, "https://api.example.com");
    }

    #[test]
    fn test_normalize_server_url_strips_trailing_slash() {
        let url = normalize_server_url("https://api.example.com/").unwrap();
        assert_eq!(url, "https://api.example.com");
    }

    #[test]
    fn test_normalize_server_url_strips_multiple_trailing_slashes() {
        let url = normalize_server_url("https://api.example.com///").unwrap();
        assert_eq!(url, "https://api.example.com");
    }

    #[test]
    fn test_normalize_server_url_prepends_https() {
        let url = normalize_server_url("api.example.com").unwrap();
        assert_eq!(url, "https://api.example.com");
    }

    #[test]
    fn test_normalize_server_url_trims_whitespace() {
        let url = normalize_server_url("  https://api.example.com  ").unwrap();
        assert_eq!(url, "https://api.example.com");
    }

    #[test]
    fn test_normalize_server_url_rejects_empty() {
        assert!(normalize_server_url("").is_err());
    }

    #[test]
    fn test_normalize_server_url_rejects_whitespace_only() {
        assert!(normalize_server_url("   ").is_err());
    }

    #[test]
    fn test_normalize_server_url_rejects_http() {
        let result = normalize_server_url("http://api.example.com");
        assert!(result.is_err());
        match result.unwrap_err() {
            AccountError::ServiceError(e) => {
                assert!(e.message.contains("HTTPS"));
            }
            other => panic!("expected ServiceError, got {other:?}"),
        }
    }

    // ===================================================================
    // from_http_client_error
    // ===================================================================

    #[tokio::test]
    async fn test_from_http_client_error_creates_transport_error() {
        // Build a reqwest error by making an invalid URL request synchronously
        let client = reqwest::Client::new();
        let err = client
            .get("https://definitely-not-a-real-host.invalid")
            .send()
            .await
            .unwrap_err();

        let account_err = AccountService::from_http_client_error("GET", "https://example.com", err);
        match account_err {
            AccountError::TransportError(te) => {
                assert_eq!(te.method, "GET");
                assert_eq!(te.url, "https://example.com");
                assert!(te.description.is_some());
                assert!(te.cause.is_some());
            }
            other => panic!("expected TransportError, got {other:?}"),
        }
    }

    // ===================================================================
    // orgs_by_account
    // ===================================================================

    #[tokio::test]
    async fn test_orgs_by_account_empty() {
        let svc = service_with_accounts(vec![], None, None);
        let results = svc.orgs_by_account().await;
        assert!(results.is_empty());
    }

    #[tokio::test]
    async fn test_orgs_by_account_single_no_network() {
        let acct = make_account("acct_1", "alice@example.com");
        let svc = service_with_accounts(vec![acct], None, None);
        let results = svc.orgs_by_account().await;
        assert!(results.is_empty());
    }

    #[tokio::test]
    async fn test_orgs_by_account_with_accounts() {
        let acct1 = make_account("acct_1", "alice@example.com");
        let acct2 = make_account("acct_2", "bob@example.com");
        let svc = service_with_accounts(vec![acct1, acct2], None, None);
        let results = svc.orgs_by_account().await;
        assert!(results.is_empty());
    }
}
