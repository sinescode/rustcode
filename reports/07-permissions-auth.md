# Report 07: Permissions & Auth Subsystem — Gap Analysis and Fixes

## Overview

This report catalogs every exported symbol from opencode's `permission/` and `auth/`
directories, compares them with rustcode's `permission.rs` and `credential.rs`, and
documents the fixes applied for each gap.

---

## 1. Exported Symbols from OpenCode

### 1.1 `packages/opencode/src/permission/index.ts`

| Symbol | Kind | Status |
|---|---|---|
| `evaluate(permission, pattern, ...rulesets)` | Function | PRESENT |
| `expand(pattern)` | Function (private) | PRESENT as `expand_pattern()` |
| `fromConfig(permission)` | Function | PRESENT as `rules_from_config()` |
| `merge(...rulesets)` | Function | PRESENT as `merge_rulesets()` |
| `disabled(tools, ruleset)` | Function | PRESENT as `disabled_tools()` |
| `Event.Asked` | Event constant | PRESENT (inline event publish) |
| `Event.Replied` | Event constant | PRESENT (inline event publish) |
| `Interface` (ask, reply, list) | Interface | PRESENT (ask, reply, list + assert) |
| `Service` | Context.Service | PRESENT as `PermissionService` |
| `layer` / `defaultLayer` / `node` | Layers | N/A (Rust doesn't use Effect layers) |

### 1.2 `packages/opencode/src/permission/arity.ts`

| Symbol | Kind | Status |
|---|---|---|
| `prefix(tokens)` | Function | PRESENT as `bash_arity_prefix()` |
| `ARITY` dictionary | Constant | PRESENT as `arity_map()` |
| `BashArity` namespace | Namespace | PRESENT |

### 1.3 `packages/core/src/permission/schema.ts` (V2)

| Symbol | Kind | Status |
|---|---|---|
| `Effect` ("allow"/"deny"/"ask") | Schema | **FIXED** (`PermissionV2Effect`) |
| `Rule` { action, resource, effect } | Schema | **FIXED** (`PermissionV2Rule`) |
| `Ruleset` (Vec<Rule>) | Schema | **FIXED** (`PermissionV2Ruleset`) |
| `PermissionSchema` namespace | Namespace | PRESENT (types are directly exported) |

### 1.4 `packages/core/src/permission/saved.ts`

| Symbol | Kind | Status |
|---|---|---|
| `ID` (branded string) | Schema | PRESENT (via `SavedPermission.id`) |
| `Info` { id, projectID, action, resource } | Schema | PRESENT as `SavedPermission` |
| `ListInput` { projectID? } | Schema | PRESENT (method param) |
| `AddInput` { projectID, action, resources } | Schema | PRESENT as `AddSavedInput` |
| `Interface` (list, add, remove) | Interface | PRESENT |
| `Service` | Context.Service | PRESENT as `SavedPermissions` |
| `layer` / `defaultLayer` | Layers | N/A |

### 1.5 `packages/core/src/v1/permission.ts`

| Symbol | Kind | Status |
|---|---|---|
| `ID` (per_ prefix) | Branded string | PRESENT as `permission_id()` |
| `Action` ("allow"/"deny"/"ask") | Literals | PRESENT as `PermissionAction` |
| `Rule` { permission, pattern, action } | Schema | PRESENT as `PermissionRule` |
| `Ruleset` (Vec<Rule>) | Schema | PRESENT as `PermissionRuleset` |
| `Request` { id, sessionID, permission, patterns, metadata, always, tool? } | Schema | PRESENT as `PermissionRequest` |
| `Reply` ("once"/"always"/"reject") | Literals | PRESENT as `PermissionReply` |
| `ReplyBody` { reply, message? } | Schema | PRESENT (fields in `ReplyInput`) |
| `Approval` { projectID, patterns } | Schema | **MISSING** — not critical, used for V2 approval flow |
| `AskInput` | Schema | PRESENT as `AskInput` |
| `ReplyInput` | Schema | PRESENT as `ReplyInput` |
| `RejectedError` | Error class | PRESENT as `PermissionError::Rejected` |
| `CorrectedError` | Error class | PRESENT as `PermissionError::Corrected` |
| `DeniedError` | Error class | PRESENT as `PermissionError::Denied` |
| `NotFoundError` | Error class | PRESENT as `PermissionError::NotFound` |
| `Error` (union type) | Type | PRESENT as `PermissionError` |

### 1.6 `packages/core/src/permission.ts` (V2)

| Symbol | Kind | Status |
|---|---|---|
| `evaluate(action, resource, ...rulesets)` | Function | PRESENT as `evaluate_v2()` |
| `merge(...rulesets)` | Function | PRESENT as `merge_rulesets()` |
| `Effect`, `Rule`, `Ruleset` | Re-exports | **FIXED** |
| `ID` | Branded string | PRESENT |
| `Source` (tagged union) | Schema | **FIXED** as `PermissionSource` |
| `Request` { id, sessionID, action, resources, save?, metadata?, source? } | Schema | PRESENT as `PermissionRequest` |
| `Reply` ("once"/"always"/"reject") | Literals | PRESENT |
| `AssertInput` | Schema | **FIXED** as `AssertInputV2` |
| `ReplyInput` | Schema | PRESENT |
| `AskResult` { id, effect } | Schema | **FIXED** as `AskResult` |
| `Event.Asked` / `Event.Replied` | Events | PRESENT |
| `RejectedError`, `CorrectedError`, `DeniedError`, `NotFoundError` | Errors | PRESENT |
| `Interface` (ask, assert, reply, get, forSession, list) | Interface | **FIXED** (added `get()`, `for_session()`) |
| `Service` / `layer` / `locationLayer` | Service | PRESENT |

### 1.7 `packages/core/src/v1/config/permission.ts`

| Symbol | Kind | Status |
|---|---|---|
| `Action` | Literal | PRESENT |
| `Object` (Record<string, Action>) | Schema | PRESENT |
| `Rule` (Action | Object) | Union | PRESENT (enum `PermissionRule`) |
| `Info` (full config struct) | Schema | PRESENT as `PermissionConfig` |
| `ConfigPermissionV1` namespace | Namespace | PRESENT |

### 1.8 `packages/opencode/src/auth/index.ts`

| Symbol | Kind | Status |
|---|---|---|
| `OAUTH_DUMMY_KEY` | Constant | **FIXED** |
| `Oauth` class | Schema | **FIXED** as `AuthOauth` |
| `Api` class | Schema | **FIXED** as `AuthApi` |
| `WellKnown` class | Schema | **FIXED** as `AuthWellKnown` |
| `Info` (union) | Schema | **FIXED** as `AuthInfo` |
| `AuthError` | Error class | PRESENT (pre-existing `Error::Auth`) |
| `Interface` (get, all, set, remove) | Interface | **FIXED** |
| `Service` | Context.Service | **FIXED** as `Auth` struct |
| `layer` / `defaultLayer` / `node` | Layers | N/A |
| `Auth` namespace | Namespace | **FIXED** |

### 1.9 `packages/core/src/credential.ts`

| Symbol | Kind | Status |
|---|---|---|
| `ID` (cred_ prefix) | Branded string | PRESENT |
| `OAuth` class | Schema | PRESENT as `CredentialOAuth` |
| `Key` class | Schema | PRESENT as `CredentialKey` |
| `Info` (union) | Schema | PRESENT as `CredentialInfo` |
| `Stored` class | Schema | PRESENT as `CredentialStored` |
| `Interface` (all, list, get, create, update, remove) | Interface | PRESENT (via `CredentialStored` methods) |
| `Service` / `layer` / `defaultLayer` | Service | N/A |

### 1.10 Well-Known credential type

| Symbol | Kind | Status |
|---|---|---|
| `Credential.WellKnown` (open code `auth/index.ts` `WellKnown`) | Schema | **FIXED** as `CredentialWellKnown` |

---

## 2. Gaps Found and Fixes Applied

### GAP A: Missing Auth module (entire module)
**OpenCode source:** `packages/opencode/src/auth/index.ts`
**Severity:** HIGH
**Description:** The entire auth module for managing OAuth, API key, and well-known credentials was not ported to rustcode. This module handles reading/writing `auth.json` to the application data directory.

**Fix applied:** Created `/root/opencodesport/rustcode/crates/rustcode-core/src/auth.rs`
- `AuthOauth` struct — OAuth credential with refresh/access tokens and expiry (matches `packages/opencode/src/auth/index.ts` `Oauth` class)
- `AuthApi` struct — API key credential with optional metadata (matches `Api` class)
- `AuthWellKnown` struct — key + token pair for pre-established services (matches `WellKnown` class)
- `AuthVariant` enum — discriminant for credential types
- `AuthInfo` enum — tagged union serialized with `"type"` discriminant
- `AuthStore` type alias — `HashMap<String, AuthInfo>` matching the on-disk `auth.json`
- `Auth` struct — service with `get()`, `all()`, `set()`, `remove()` methods
- `OAUTH_DUMMY_KEY` constant (`"opencode-oauth-dummy-key"`)
- `OPENCODE_AUTH_CONTENT` env var support (matches TS behavior)
- 27 tests covering all credential variants, serialization, and CRUD operations
- Registered `pub mod auth` in `lib.rs`

### GAP B: Missing WellKnown credential type in CredentialInfo
**OpenCode source:** `packages/opencode/src/auth/index.ts` `WellKnown` class
**Severity:** MEDIUM
**Description:** The `WellKnown` credential type (key + token pair) exists in opencode's auth module but was not present in rustcode's `credential.rs`. The `CredentialInfo` enum only had `OAuth` and `Key` variants.

**Fix applied:** Modified `/root/opencodesport/rustcode/crates/rustcode-core/src/credential.rs`
- Added `CredentialWellKnown` struct with `key`, `token`, and optional `metadata` fields
- Added `WellKnown(CredentialWellKnown)` variant to `CredentialInfo` enum (serialized as `"wellknown"`)
- Added 8 tests for `CredentialWellKnown` serialization, deserialization, round-trip, and integration with `CredentialInfo` and `CredentialStored`

### GAP C: Missing V2 Permission Schema Types
**OpenCode source:** `packages/core/src/permission/schema.ts`
**Severity:** MEDIUM
**Description:** The V2 permission schema uses `action`/`resource`/`effect` naming (instead of `permission`/`pattern`/`action`). Rustcode only had the V1 naming convention.

**Fix applied:** Added to `/root/opencodesport/rustcode/crates/rustcode-core/src/permission.rs`
- `PermissionV2Effect` enum — same values as `PermissionAction`, with bidirectional `From` conversions
- `PermissionV2Rule` struct — `action`, `resource`, `effect` fields (matching TS `Rule`)
- `PermissionV2Ruleset` type alias — `Vec<PermissionV2Rule>` (matching TS `Ruleset`)

### GAP D: Missing PermissionSource type
**OpenCode source:** `packages/core/src/permission.ts` lines 27–33
**Severity:** LOW
**Description:** The V2 `Source` type uses a tagged union with a `"tool"` variant. Rustcode only had `ToolSource` (untagged struct).

**Fix applied:** Added to `/root/opencodesport/rustcode/crates/rustcode-core/src/permission.rs`
- `PermissionSource` enum with `Tool { message_id, call_id }` variant, serialized as a tagged union (`{"type": "tool", ...}`)

### GAP E: Missing AskResult type
**OpenCode source:** `packages/core/src/permission.ts` lines 68–72
**Severity:** LOW
**Description:** The V2 `AskResult` type (id + effect) was not present.

**Fix applied:** Added to `/root/opencodesport/rustcode/crates/rustcode-core/src/permission.rs`
- `AskResult` struct with `id` (String) and `effect` (PermissionV2Effect) fields

### GAP F: Missing V2 AssertInput type
**OpenCode source:** `packages/core/src/permission.ts` lines 54–59
**Severity:** LOW
**Description:** The V2 `AssertInput` with `save`/`source`/`agent` fields was not present as a dedicated type.

**Fix applied:** Added to `/root/opencodesport/rustcode/crates/rustcode-core/src/permission.rs`
- `AssertInputV2` struct with `id`, `session_id`, `action`, `resources`, `save`, `metadata`, `source`, `agent` fields

### GAP G: Missing PermissionService.get() and PermissionService.for_session()
**OpenCode source:** `packages/core/src/permission.ts` lines 317–323
**Severity:** MEDIUM
**Description:** The V2 `PermissionService` interface includes `get(id)` and `forSession(sessionID)` methods for querying pending requests. These were missing from rustcode's `PermissionService`.

**Fix applied:** Added to `PermissionService` impl block in `/root/opencodesport/rustcode/crates/rustcode-core/src/permission.rs`
- `pub fn get(&self, id: &str) -> Option<PermissionRequest>` — returns a specific pending request
- `pub fn for_session(&self, session_id: &str) -> Vec<PermissionRequest>` — returns all pending requests for a session

---

## 3. Items Already Present (No Fix Needed)

### Permission Core Types
- `PermissionAction` enum (Allow/Deny/Ask) ✓
- `PermissionRule` struct (permission, pattern, action) ✓
- `PermissionRuleset` type alias ✓
- `PermissionReply` enum (Once/Always/Reject) ✓
- `ToolSource` struct ✓
- `PermissionRequest` struct ✓
- `AskInput` struct ✓
- `ReplyInput` struct ✓
- `EvaluatedPermission` struct ✓

### Wildcard Matching
- `wildcard_match()` function ✓
- `regex_escape()` helper ✓

### Rule Evaluation
- `evaluate()` (V1 semantics, last-match-wins) ✓
- `evaluate_v2()` (V2 semantics, delegates to evaluate) ✓

### Bash Command Arity
- `bash_arity_prefix()` function ✓
- `arity_map()` dictionary (150+ command entries) ✓

### Config Conversion
- `rules_from_config()` function ✓
- `expand_pattern()` with ~/ and $HOME expansion ✓
- `process_config_field()`, `push_simple_rule()`, `convert_action()` helpers ✓

### Utility Functions
- `merge_rulesets()` function ✓
- `disabled_tools()` function ✓
- `permission_id()` function ✓

### Saved Permissions
- `SavedPermission` struct ✓
- `AddSavedInput` struct ✓
- `SavedPermissions` service with list/add/remove ✓

### Permission Service
- `PermissionService` struct ✓
- `ask()`, `assert()`, `reply()`, `list()`, `approved_rules()` methods ✓
- Event publishing (asked/replied) ✓
- Cascade reject/always logic ✓

### Credential Types
- `CredentialOAuth` struct ✓
- `CredentialKey` struct ✓
- `CredentialInfo` enum (tagged union with OAuth/Key) ✓
- `CredentialStored` struct with new/with_id ✓
- `CredentialTableRow` struct ✓

### Error Types
- `Error::Auth(String)` variant ✓ (pre-existing)
- `PermissionError` enum with Rejected/Corrected/Denied/NotFound ✓

### Config Types
- `PermissionConfig` struct with all known fields ✓
- `PermissionAction` enum (Ask/Allow/Deny) ✓
- `PermissionRule` enum (Action/Object) ✓

---

## 4. Files Modified/Created

```
CREATED:  /root/opencodesport/rustcode/crates/rustcode-core/src/auth.rs
            (auth module — 27 tests)

MODIFIED: /root/opencodesport/rustcode/crates/rustcode-core/src/credential.rs
            (added CredentialWellKnown + WellKnown variant + 8 tests)

MODIFIED: /root/opencodesport/rustcode/crates/rustcode-core/src/permission.rs
            (added V2 types: PermissionV2Effect, PermissionV2Rule,
             PermissionV2Ruleset, PermissionSource, AskResult, AssertInputV2;
             added PermissionService.get() and .for_session())

MODIFIED: /root/opencodesport/rustcode/crates/rustcode-core/src/lib.rs
            (added pub mod auth)
```

---

## 5. Verification Summary

| Check | Status |
|---|---|
| All exported symbols from opencode permission/ catalogued | ✓ |
| All exported symbols from opencode auth/ catalogued | ✓ |
| Rustcode permission.rs has all core types | ✓ |
| Rustcode credential.rs has all credential types | **FIXED** (added WellKnown) |
| Rustcode has auth module | **FIXED** (created auth.rs) |
| V2 schema types present | **FIXED** (added PermissionV2Rule etc.) |
| PermissionSource tagged union present | **FIXED** |
| AskResult type present | **FIXED** |
| PermissionService.get() and .for_session() present | **FIXED** |
| No unsafe code in new files | ✓ |
| No unwrap in library code | ✓ |
| Source references cited in doc comments | ✓ |

---

## 6. Key Design Decisions

### Auth module structure
The opencode auth module uses Effect.ts `Context.Service` and `Layer`. Rustcode
implements this as a plain struct `Auth` with synchronous methods since there is
no Effect runtime. The file I/O is handled directly via `std::fs` rather than
through an `FSUtil` service (which rustcode does not yet have as a DI service).

### OPENCODE_AUTH_CONTENT environment variable
The TS source checks an environment variable `OPENCODE_AUTH_CONTENT` before
reading the file. If set and parseable, its JSON content is returned directly.
This behavior is preserved in rustcode's `load_store()`.

### PermissionService.get() vs V1/V2
The `get()` and `for_session()` methods are V2 additions that query pending
requests without creating or resolving them. They are now available on the
rustcode `PermissionService`.

### V2 vs V1 type naming
V2 types use `PermissionV2*` naming to distinguish them from V1 types:
- `PermissionV2Effect` vs `PermissionAction` (same values, different purpose)
- `PermissionV2Rule` vs `PermissionRule` (action/resource/effect vs permission/pattern/action)
This mirrors the opencode convention where V1 and V2 have different struct/schema names.

---

*Report generated by fix-and-verify agent for the permissions/auth subsystem.*
