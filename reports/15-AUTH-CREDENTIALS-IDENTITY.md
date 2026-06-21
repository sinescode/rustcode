# Auth/Credentials/Identity — Gap Analysis

## Auth Architecture

| Feature | TS | Rust | Status |
|---------|----|------|--------|
| Auth types | `Oauth`, `Api`, `WellKnown` (Schema.Class) | Matching structs | ✅ Equivalent |
| Discriminated union | `Schema.Union` with `type` discriminator | `AuthInfo` enum with `#[serde(tag = "type")]` | ✅ Equivalent |
| CRUD (get/all/set/remove) | Full | Full | ✅ Equivalent |
| Trailing-slash normalization | Strip + remove variants | Same logic | ✅ Equivalent |
| File permissions | `0o600` on write | `0o600` via `#[cfg(unix)]` | ✅ Equivalent |
| ENV override | `OPENCODE_AUTH_CONTENT` | Same | ✅ Equivalent |

## Credential Management

| Feature | TS | Rust | Status |
|---------|----|------|--------|
| Credential variants | OAuth, Key | OAuth, Key, + WellKnown (extra) | ⚠️ Rust extra |
| ID creation | `"cred_" + ascending()` | `id::create("cred", ...)` | ✅ |
| CRUD service | Full (all/list/create/update/remove) | **Types only — no service** | **CRITICAL** |
| DB persistence | SQLite via Drizzle | sqlx types defined but no queries | **CRITICAL** |

## Account System

| Feature | TS | Rust | Status |
|---------|----|------|--------|
| Branded types | Effect/Schema brands | Type aliases (no compile-time safety) | ⚠️ Weaker |
| Account Info | `Info { id, email, url, active_org_id }` | Matching struct | ✅ |
| Login flow | `Login { code, user, url, server, expiry, interval }` | Matching | ✅ |
| Poll results | 6 variants | Same 6 on `PollResult` | ✅ |
| Error hierarchy | 3-level | Same 3 levels on `AccountError` | ✅ |
| **Token refresh with cache** | `Cache.make` dedup | No dedup | **CRITICAL** |
| **Eager refresh threshold** | 5min before expiry | After expiry only | **CRITICAL** |
| **OAuth device code URL** | `/auth/device/code` | `/device/code` | **WRONG PATH** |
| **Poll request body** | `{grant_type, device_code, client_id}` | `{code}` | **WRONG FORMAT** |
| **Refresh request body** | `{grant_type, refresh_token, client_id}` | `{refresh_token}` | **WRONG FORMAT** |
| **Client ID** | `"opencode-cli"` | Not sent | **MISSING** |
| **fetchUser after poll** | Fetches `/api/user` | **Missing** | **CRITICAL** |
| **fetchOrgs after poll** | Fetches orgs | **Missing** | **CRITICAL** |
| **persistAccount after poll** | Inserts into DB | **Missing** | **CRITICAL** |
| **orgsByAccount** | Concurrent fetch | **Missing** | **CRITICAL** |
| **config(accountID, orgID)** | Remote config fetch | **Missing** | **CRITICAL** |

## Installation / Upgrade

| Feature | TS | Rust | Status |
|---------|----|------|--------|
| InstallationVersion | Compile-time constant | **Missing** | **CRITICAL** |
| `method()` detection | Checks process.execPath + runs cmds | **Missing** | **CRITICAL** |
| `latest()` version check | 6 sources (GitHub, npm, brew, etc.) | **Missing** | **CRITICAL** |
| `upgrade()` | 8 upgrade strategies | **Missing** | **CRITICAL** |
| Event definitions | `installation.updated`, etc. | **Missing** | **CRITICAL** |

## Location System — ✅ Full Parity

All of `Location.Ref`, `Info`, `Interface`, `MutationKind`, `PathError`, `ExternalDirectoryAuthorization`, `MutationTarget`, `LocationServiceMap`, `MutationService.resolve` are ported.

## 5 Most Critical Gaps

### 1. No Credential CRUD Service Layer
TS provides full `Credential.Service` with SQLite-backed CRUD. Rust has type definitions only — no service, no DB queries, no transaction logic.

**TS**: `credential.ts:44-150`
**Rust**: `credential.rs:120-172`

### 2. OAuth Device Flow Is Incomplete
- Wrong endpoint paths (missing `/auth/` prefix)
- Wrong request body format (missing `grant_type`, `client_id`)
- No `fetchUser` or `fetchOrgs` after successful poll
- No `persistAccount` call — account never saved

### 3. Missing Token Refresh Dedup with Eager Threshold
TS wraps in `Cache.make` — concurrent calls share one refresh. TS refreshes 5min *before* expiry; Rust refreshes *after* expiry.

**TS**: `account.ts:248-265`, `138-142`
**Rust**: `account.rs:531-566`, `544-547`

### 4. No Installation/Upgrade Service Implementation
Types only. No method detection, no version check, no upgrade strategies.

**Rust**: `installation.rs:16-137`

### 5. Missing Account Service Features
- No `orgsByAccount` (concurrent org fetching)
- No `config(accountID, orgID)` with `x-org-id` header
- In-memory `Vec<AccountTableRow>` instead of SQLite
- No legacy `control_account` table support
