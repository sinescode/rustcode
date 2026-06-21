# Security Policy

## Reporting a Vulnerability

Please report security vulnerabilities to the repository maintainers via GitHub Issues with the label `security`.

Do NOT file a public issue for critical vulnerabilities — use the private vulnerability reporting mechanism.

## Supported Versions

| Version | Supported |
|---------|-----------|
| latest  | ✅         |

## Security Features

- **Memory safety**: `#![forbid(unsafe_code)]` in all crates
- **Encrypted credentials**: HMAC-SHA256 at-rest encryption for OAuth tokens and API keys
- **SQL injection prevention**: All queries use sqlx parameterized bindings (`?1`, `?2`, etc.)
- **Path traversal protection**: `{file:path}` config substitution verifies canonicalized path
- **Permission system**: Gated tool execution with allow/deny/ask semantics
- **Supply chain**: cargo-deny for license and advisory checking in CI
- **Dependency auditing**: Weekly cargo-audit scans
