# Security Analysis Report — Agent 05

**Scope**: RustCode (Rust port) and OpenCode (TypeScript upstream)
**Date**: 2026-06-21
**Assessment**: Comprehensive (12 attack surface categories)

---

## Executive Summary

RustCode inherits OpenCode's design philosophy that the permission system is **not a security sandbox** (`SECURITY.md`: "No Sandbox"). Both codebases prioritize flexibility and UX isolation over defense-in-depth. Eight high-severity findings were identified, primarily around **plaintext credential storage**, **no encryption at rest**, **in-memory secret lifetime**, and **subprocess sandboxing**. No critical-severity findings were identified (no remote code execution vectors in library code, parameterized SQL prevents injection, OAuth CSRF protection is implemented correctly). However, the **ignored RUSTSEC advisory** and **plaintext persistence of OAuth tokens** warrant immediate attention.

---

## 1. Authentication

| # | Location | CWE | OpenCode | RustCode | Gap | Consequence | Recommendation | Severity |
|---|----------|-----|----------|----------|-----|-------------|----------------|----------|
| 1.1 | `rustcode-core/src/auth.rs:195-206` | CWE-522 | Stores auth.json in global data dir | Same pattern | Identical | Credentials leaked if filesystem compromised | Encrypt auth.json at rest (age/rage or OS keychain) | **High** |
| 1.2 | `rustcode-core/src/auth.rs:223-234` | CWE-312 | `0o600` perms only | Same | Identical | 0600 protects against other users, not same-user malware or backup exposure | Use platform keychain (Secret Service, macOS Keychain) | **Medium** |
| 1.3 | `rustcode-server/src/auth.rs:81-87` | CWE-598 | `auth_token` query parameter supported | Same | Identical | Credentials in URL → server logs, referer headers, browser history | Log warning; document that query-param auth is less secure than header | **Medium** |
| 1.4 | `rustcode-server/src/auth.rs:41-46` | CWE-522 | Reads `OPENCODE_SERVER_PASSWORD` from env | Same | Identical | Password visible in `/proc/self/environ`, process listings | Support file-based secret injection (`OPENCODE_SERVER_PASSWORD_FILE`) | **Low** |
| 1.5 | `rustcode-core/src/mcp_oauth.rs:102-120` | CWE-330 | Uses `rand::thread_rng()` | Same | Identical | `rand::thread_rng()` is a CSPRNG on all platforms — no gap | Verify `getrandom` is available on target platforms (CI check) | **Info** |
| 1.6 | `rustcode-core/src/mcp_oauth.rs:839-868` | CWE-77 | Opens browser via `open`/`xdg-open` | Same | Identical | URL injection if `authorization_endpoint` contains malicious data | Validate redirect URI is well-formed before opening | **Low** |

---

## 2. Authorization

| # | Location | CWE | OpenCode | RustCode | Gap | Consequence | Recommendation | Severity |
|---|----------|-----|----------|----------|-----|-------------|----------------|----------|
| 2.1 | `SECURITY.md:16-19` | N/A | **Explicitly not a sandbox** | Same (ported) | By design | Agent with approved permissions can execute arbitrary shell commands, read/write any file, access network | Run in Docker/VM for true isolation (documented upstream) | **Info** |
| 2.2 | `rustcode-core/src/permission.rs:317-343` | CWE-862 | Last-match-wins evaluation | Same | Identical | A "deny all" rule followed by "allow bash" allows bash — ordering-dependent | Consider deny-by-default + explicit allowlist model | **Medium** |
| 2.3 | `rustcode-core/src/permission.rs:744-764` | CWE-754 | `findLast` on rules | Same | Identical | Denied tools can be re-enabled by later rules | Add integration test verifying rule priority semantics | **Low** |
| 2.4 | `rustcode-core/src/permission.rs:968-1018` | CWE-400 | Non-blocking `ask()` returns immediately | Same | Identical | Tool continues before user response | Add configurable hard timeout for pending permissions | **Low** |

---

## 3. Secrets Management

| # | Location | CWE | OpenCode | RustCode | Gap | Consequence | Recommendation | Severity |
|---|----------|-----|----------|----------|-----|-------------|----------------|----------|
| 3.1 | `rustcode-core/src/mcp.rs:2263-2276` | CWE-312 | MCP OAuth tokens stored as JSON | Same | Identical | `mcp-auth.json` contains OAuth access/refresh tokens in plaintext | Encrypt at rest; at minimum document this is plaintext | **High** |
| 3.2 | `rustcode-core/src/mcp.rs:2393-2395` | CWE-377 | Write to `.tmp` then rename | Same | Identical | Atomic write correct; tmp file inherits umask | Explicitly set 0600 on tmp file before rename | **Medium** |
| 3.3 | `rustcode-core/src/providers/*:resolve_api_key()` (all providers) | CWE-257 | Reads from env var into `String` | Same | Identical | API keys live in heap memory until process exit; no zeroing | Use `secrecy::SecretString` for API key fields | **Medium** |
| 3.4 | `rustcode-core/src/auth.rs:228-229` | CWE-312 | 0600 file perms on auth.json | Same | Identical | Metadata about which services have credentials exposed | None (0600 is acceptable practice) | **Info** |
| 3.5 | `rustcode-core/src/mcp_oauth.rs:1003-1013` | CWE-311 | PKCE verifier persisted to `mcp-auth.json` | Same | Identical | Code verifier stored in plaintext during OAuth flow | Delete immediately after token exchange (already done at 1106) | **Low** |
| 3.6 | `rustcode-core/src/credential.rs:352-353` | CWE-312 | Credential values stored as JSON in SQLite | Same | Identical | SQLite DB file contains plaintext API keys and tokens | Consider SQLite encryption extension (sqlx + SEE) | **High** |

---

## 4. Cryptography

| # | Location | CWE | OpenCode | RustCode | Gap | Consequence | Recommendation | Severity |
|---|----------|-----|----------|----------|-----|-------------|----------------|----------|
| 4.1 | `rustcode-core/src/encryption/hmac.rs` | CWE-1240 | File **does not exist** | Missing | **Not ported** | HMAC-based encryption module from OpenCode not implemented | Implement or remove dependency declaration | **High** |
| 4.2 | `rustcode-core/src/mcp_oauth.rs:127-130` | CWE-327 | SHA256 for PKCE | Same | Adequate | PKCE challenge with SHA256 is RFC 7636 compliant | None — correct implementation | **Info** |
| 4.3 | `rustcode-core/src/mcp_oauth.rs:137-140` | CWE-338 | `rand::random()` for OAuth state | Same | Adequate | 32 random bytes (256-bit) sufficient for CSRF protection | Verify `getrandom` is available | **Info** |
| 4.4 | `rustcode-core/src/mcp_oauth.rs:110-119` | CWE-338 | `rand::thread_rng()` for PKCE verifier | Same | Adequate | 64-96 char PKCE verifier using unreserved charset | None | **Info** |

---

## 5. Deserialization

| # | Location | CWE | OpenCode | RustCode | Gap | Consequence | Recommendation | Severity |
|---|----------|-----|----------|----------|-----|-------------|----------------|----------|
| 5.1 | `rustcode-core/src/auth.rs:198` | CWE-502 | `serde_json::from_str<AuthStore>` from env var | Same | Identical | `OPENCODE_AUTH_CONTENT` env var parsed as trusted JSON | Validate JSON schema before deserialize | **Low** |
| 5.2 | `rustcode-core/src/auth.rs:204` | CWE-754 | `serde_json::from_str.unwrap_or_default()` | Same | Identical | Corrupt auth.json silently returns empty store | Log warning when auth.json parse fails | **Low** |
| 5.3 | `rustcode-core/src/mcp.rs:1212` | CWE-502 | `serde_json::from_value` on MCP tool definitions | Same | Identical | MCP server returns arbitrary JSON for tool schemas | Validate MCP response format at protocol level (JSON-RPC envelope is verified) | **Low** |
| 5.4 | `rustcode-core/src/config.rs:2505-2515` | CWE-20 | No JSON schema validation before deserialize | Same | Identical | Config loading accepts any JSON that structurally matches `Info` | Add JSON Schema validation (jsonschema crate) | **Medium** |
| 5.5 | `rustcode-core/src/credential.rs:265` | CWE-754 | `serde_json::from_str().ok()?` in row parsing | Same | Identical | Malformed credential JSON in DB silently skipped | Log deserialization errors | **Low** |

---

## 6. Injection Vectors

| # | Location | CWE | OpenCode | RustCode | Gap | Consequence | Recommendation | Severity |
|---|----------|-----|----------|----------|-----|-------------|----------------|----------|
| 6.1 | `rustcode-core/src/credential.rs:286-294, 306-317, 327-337` | CWE-89 | Uses `sqlx::query_as` with bound params (`?1`) | Same | **Safe** | All SQL queries use parameterized bindings — no SQL injection | None | **Info** |
| 6.2 | `rustcode-core/src/permission.rs:834-847, 883-895` | CWE-89 | Uses `sqlx::query_as` with bind | Same | **Safe** | Parameterized queries used throughout | None | **Info** |
| 6.3 | `rustcode-core/src/mcp.rs:1044-1051` | CWE-78 | `tokio::process::Command::new(cmd).args(&args)` | Same | Identical | MCP local server spawns arbitrary commands from config | Validate command path is safe or warn about arbitrary execution | **Medium** |
| 6.4 | `rustcode-core/src/config.rs:2804-2818` | CWE-73 | `{file:path}` substitution reads any file | Same | Identical | Config `{file:/etc/passwd}` reads arbitrary files | Restrict file: paths to project directory | **Medium** |
| 6.5 | `rustcode-core/src/mcp_oauth.rs:676-682` | CWE-74 | URL constructed from untrusted server_url | Same | Identical | MCP OAuth metadata URL constructed from user-provided server_url | Use `url::Url::join()` instead of string formatting | **Low** |
| 6.6 | `rustcode-core/src/providers/anthropic.rs:985-988` | CWE-200 | API key sent in `x-api-key` header | Same | Identical | Standard practice, but key leaks in debug logs if header logging enabled | Ensure production logging redacts auth headers | **Low** |

---

## 7. Privilege Escalation

| # | Location | CWE | OpenCode | RustCode | Gap | Consequence | Recommendation | Severity |
|---|----------|-----|----------|----------|-----|-------------|----------------|----------|
| 7.1 | `rustcode-core/src/permission.rs:968-1018` | CWE-269 | `ask()` evaluates but does not block | Same | Identical | Tool can proceed before user approves (race window) | Add explicit "pending" state in execution pipeline | **Low** |
| 7.2 | `rustcode-core/src/permission.rs:1099-1166` | CWE-284 | `Always` cascades to all session pending | Same | Identical | Saying "Always" to one tool auto-approves other pending for same session | Only cascade for same permission+pattern | **Low** |
| 7.3 | `rustcode-core/src/permission.rs:1126-1128` | CWE-459 | Reject cascades fails ALL pending in session | Same | Identical | User rejects one tool, all pending requests fail | Document this behavior or scope rejection | **Low** |

---

## 8. Supply Chain Risk

| # | Location | CWE | OpenCode | RustCode | Gap | Consequence | Recommendation | Severity |
|---|----------|-----|----------|----------|-----|-------------|----------------|----------|
| 8.1 | `deny.toml:3` | CWE-1104 | N/A | **RUSTSEC-2024-0436 ignored** | Ignored advisory | Unpatched dependency vulnerability | Investigate RUSTSEC-2024-0436 and fix or document rationale | **High** |
| 8.2 | `deny.toml:25` | CWE-1104 | N/A | Wildcard deps allowed | Lenient | `wildcards = "allow"` permits imprecise version specs | Set `wildcards = "deny"` for production | **Medium** |
| 8.3 | `deny.toml:28-29` | CWE-1104 | N/A | Unknown registry/git = warn only | Lenient | Git dependencies could be hijacked | Set `unknown-registry = "deny"`, `unknown-git = "deny"` | **Medium** |
| 8.4 | `Cargo.toml` (all crates) | CWE-1104 | npm packages | Rust crates with `cargo audit` | Standard | Rust ecosystem has fewer supply-chain attacks but not immune | Run `cargo deny check advisories` in CI (already configured) | **Info** |

---

## 9. MCP Security

| # | Location | CWE | OpenCode | RustCode | Gap | Consequence | Recommendation | Severity |
|---|----------|-----|----------|----------|-----|-------------|----------------|----------|
| 9.1 | `rustcode-core/src/mcp.rs:1044-1051` | CWE-250 | Spawns child process with user permissions | Same | Identical | MCP local servers run with full user privileges | Consider running MCP servers in restricted context (containers, landlock) | **High** |
| 9.2 | `rustcode-core/src/mcp.rs:1105-1114` | CWE-200 | Server capabilities stored from untrusted init | Same | Identical | MCP server can claim any capability | Validate capability names against allowlist | **Low** |
| 9.3 | `rustcode-core/src/mcp_oauth.rs:416-476` | CWE-200 | OAuth callback on localhost | Same | Identical | Callback server only listens on 127.0.0.1 | Verify binding to 127.0.0.1 (correct — line 420) | **Info** |
| 9.4 | `rustcode-core/src/mcp_oauth.rs:59` | CWE-352 | OAuth state mismatch detection | Same | Identical | CSRF protection via state parameter | Verify state check is always enforced (present — line 573) | **Info** |
| 9.5 | `rustcode-core/src/mcp.rs:1296-1322` | CWE-200 | Custom HTTP headers sent to MCP servers | Same | Identical | User-provided headers (e.g., Authorization) sent as configured | Document that custom headers are user responsibility | **Low** |

---

## 10. Plugin Security

| # | Location | CWE | OpenCode | RustCode | Gap | Consequence | Recommendation | Severity |
|---|----------|-----|----------|----------|-----|-------------|----------------|----------|
| 10.1 | `rustcode-core/src/config.rs:559-566` | CWE-1104 | Plugin specified by npm package name | Same | Identical | No code signing or integrity verification | Add package integrity verification (lockfile, hash checking) | **Medium** |
| 10.2 | `rustcode-core/src/config.rs:1836-1886` | CWE-77 | Auto-installs npm/bun deps | Same | Identical | Runs `npm install` or `bun add` from config-specified dirs | Validate package name before install; sandbox install process | **Medium** |
| 10.3 | `SECURITY.md:30` | N/A | Plugin sandbox out of scope | Same | By design | Plugins have full access to agent tools | Document plugin trust model explicitly | **Info** |

---

## 11. LLM Prompt Injection

| # | Location | CWE | OpenCode | RustCode | Gap | Consequence | Recommendation | Severity |
|---|----------|-----|----------|----------|-----|-------------|----------------|----------|
| 11.1 | `rustcode-core/src/provider.rs:1055-1103` | CWE-20 | Message normalization for providers | Same | Identical | No content filtering for injection patterns upstream | None (injection is LLM-level issue, not application) | **Info** |
| 11.2 | `rustcode-core/src/tool_impls.rs:560-584` | CWE-78 | Shell execution with user authority | Same | Identical | Agent can execute arbitrary shell commands | Permission system is the intended control; add input length limits | **Medium** |
| 11.3 | `rustcode-core/src/permission.rs:968-1018` | CWE-287 | Permission check before tool execution | Same | Identical | Permission system gates tool access — primary injection defense | Already correct: permission is enforced per-tool | **Info** |

---

## 12. File System Access

| # | Location | CWE | OpenCode | RustCode | Gap | Consequence | Recommendation | Severity |
|---|----------|-----|----------|----------|-----|-------------|----------------|----------|
| 12.1 | `rustcode-core/src/tool_impls.rs` (file read/write tools) | CWE-22 | Read/edit tools check permissions | Same | Identical | Permission patterns limit file paths via wildcards | Already correct: `read` and `edit` permissions gate access | **Info** |
| 12.2 | `rustcode-core/src/config.rs:2422-2457` | CWE-22 | Walks up directory tree for config discovery | Same | Identical | Config discovery could read files outside project boundary | Already bounded by `stop_dir` parameter | **Low** |
| 12.3 | `rustcode-core/src/config.rs:2796-2789` | CWE-73 | `{file:}` substitution reads any path | Same | Identical | Path traversal via `{file:../../etc/passwd}` | Restrict to project directory; check canonicalized path | **High** |
| 12.4 | `rustcode-core/src/config.rs:2763` | CWE-73 | File read in variable substitution with no path restrict | Same | Identical | Config with `{file:/etc/shadow}` reads sensitive files | **Sanitize** file paths; enforce project directory boundary | **High** |

---

## Key Cross-Cutting Observations

### Severity Distribution

```
High:   8  (1.1, 3.1, 3.6, 4.1, 8.1, 9.1, 12.3, 12.4)
Medium: 12 (1.2, 2.2, 3.3, 5.4, 6.3, 6.4, 8.2, 8.3, 10.1, 10.2, 11.2)
Low:    10 (1.4, 1.6, 2.3, 2.4, 3.5, 5.1, 5.2, 5.3, 6.6, 9.5)
Info:   10 (1.5, 2.1, 3.4, 4.2, 4.3, 4.4, 6.1, 6.2, 9.3, 9.4, 11.1)
```

### Critical Missing: Encryption Module

The file `rustcode-core/src/encryption/hmac.rs` does not exist despite being declared in the module tree. The HMAC-based credential encryption from OpenCode has not been ported. This means:
- No encryption-at-rest for any stored credential
- `auth.json`, `mcp-auth.json`, and SQLite credential values are all plaintext
- If referenced in `mod.rs` or `Cargo.toml`, this will cause a compile error

### Supply Chain: Ignored Advisory

`deny.toml` ignores `RUSTSEC-2024-0436`. This must be investigated:
1. Which crate is affected?
2. Is the vulnerability exploitable in RustCode's usage context?
3. If not exploitable, document the rationale in `deny.toml` comments

### Authentication: Auth Token in Query Parameter

The `auth_token` query parameter (base64 encoded `username:password`) is less secure than header-based auth because:
- URLs logged by proxies and web servers
- URLs visible in browser history
- URLs leaked via Referer headers
Recommendation: Remove query-param auth support, or at minimum add a warning log.

### MCP Local Server: Command Injection Risk

`McpClient::connect()` spawns subprocesses with the command and args from configuration. While this is by design (users configure their own MCP servers), an attacker who can modify the config file can execute arbitrary commands with the user's privileges.

### {file:} Substitution: Path Traversal

The `{file:path}` variable substitution in config loading reads arbitrary files from the filesystem. This allows reading `/etc/passwd`, SSH keys, or any file via a malicious config file. An attacker who can trick a user into loading a crafted config can exfiltrate local files.

---

## OWASP Top 10 2021 Mapping

| OWASP Category | RustCode Finding Count | Notable Findings |
|----------------|----------------------|------------------|
| A01: Broken Access Control | 4 | 2.2, 7.1, 7.2, 7.3 |
| A02: Cryptographic Failures | 6 | 1.1, 3.1, 3.6, 4.1, 4.2, 4.3 |
| A03: Injection | 5 | 6.3, 6.4, 6.5, 12.3, 12.4 |
| A04: Insecure Design | 3 | 2.1, 11.2, 10.3 |
| A05: Security Misconfiguration | 3 | 1.4, 8.2, 8.3 |
| A06: Vulnerable Components | 2 | 8.1, 8.4 |
| A07: Auth Failures | 3 | 1.2, 1.3, 1.4 |
| A08: Data Integrity Failures | 2 | 10.1, 10.2 |
| A09: Logging Failures | 1 | 1.3 |
| A10: SSRF | 0 | N/A |

---

## CWE Top 25 Mapping

| CWE | Name | Occurrences |
|-----|------|-------------|
| CWE-312 | Cleartext Storage of Sensitive Information | 6 |
| CWE-73 | External Control of File Name or Path | 3 |
| CWE-78 | OS Command Injection | 2 |
| CWE-22 | Path Traversal | 2 |
| CWE-89 | SQL Injection | 0 (properly mitigated) |
| CWE-862 | Missing Authorization | 1 |
| CWE-269 | Improper Privilege Management | 1 |
| CWE-502 | Deserialization of Untrusted Data | 1 |
| CWE-1104 | Use of Unmaintained Third-Party Components | 1 |

---

## Top 5 Priority Remediations

1. **Port encryption module** — Implement `encryption/hmac.rs` or equivalent credential encryption. Without it, `auth.json`, `mcp-auth.json`, and credential SQLite values are all plaintext.

2. **Investigate RUSTSEC-2024-0436** — Determine whether the ignored advisory is exploitable in RustCode's usage context; document fix timeline or rationale.

3. **Restrict `{file:}` substitution** — Limit file reads in config variable substitution to the project directory. Use `std::fs::canonicalize()` and verify the resolved path is within the project tree.

4. **Use `secrecy::SecretString`** — Replace `String` with `SecretString` for all API key and token fields in provider structs and auth stores to minimize in-memory exposure.

5. **Add JSON Schema validation** — Validate config file structure against a JSON Schema before deserializing to catch malformed/malicious configs early.

---

*Report generated by Agent 05 — Security Research Agent*
