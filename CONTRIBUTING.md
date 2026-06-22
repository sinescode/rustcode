# Contributing to BlazeCode

## Getting Started

```bash
git clone https://github.com/sinescode/blazecode
cd blazecode
cargo check
```

## Development

### Local Commands

- `cargo check` — Verify compilation (fast, preferred locally)
- `cargo fmt` — Format code
- `cargo clippy` — Lint
- `cargo test` — Run tests
- `cargo build` — Full build (slow, CI handles this)

### CI Pipeline

All changes must pass CI before merging:
1. **Format**: `cargo fmt --all -- --check`
2. **Clippy**: `cargo clippy --all-targets --all-features -- -D warnings`
3. **Test**: `cargo build && cargo test` on ubuntu-latest + macos-latest
4. **Cargo Deny**: license + advisory checks

### Code Standards

- `#![forbid(unsafe_code)]` in every crate
- No `.unwrap()` in library code — use `?`, `.ok_or()`, `.unwrap_or()`, or `expect()` with reason
- Stream everything — use `tokio::sync::broadcast`, `tokio_stream`, `futures::Stream`
- Cite the TS source in doc comments: `/// Ported from: packages/<pkg>/src/<path>`
- Use `tokio::fs` for all filesystem I/O (not `std::fs`) in async contexts
- Use `tokio::task::spawn_blocking` for CPU-bound or synchronous operations

## Architecture

```
blazecode/                              # workspace root + binary crate
├── src/main.rs                        # CLI entry (clap: Run, Serve, Tui, etc.)
├── crates/
│   ├── blazecode-core/                 # Core library
│   │   ├── config/                    # Configuration
│   │   ├── session/                   # Session management (V1 + V2)
│   │   ├── provider/                  # LLM provider trait + implementations
│   │   ├── tool/                      # Tool system
│   │   ├── permission/               # Permission system
│   │   ├── event/                     # Event sourcing
│   │   ├── filesystem/               # File I/O
│   │   ├── database/                  # SQLite access
│   │   └── encryption/               # At-rest encryption
│   ├── blazecode-server/              # HTTP/SSE API server
│   ├── blazecode-tui/                 # Terminal UI (ratatui)
│   ├── blazecode-lsp/                 # LSP integration
│   └── blazecode-mcp/                 # Model Context Protocol
```

## Development

- When referencing the TypeScript source, match the module structure and naming
- Add tests alongside implementations

## Commit Style

- Short, imperative mood: `fix(ci): relax lints for scaffold`
- Prefix with scope: `feat(provider):`, `fix(session):`, `refactor(tool):`
- One logical change per commit

## Adding a New Provider

1. Add provider config type to `config.rs`
2. Implement `Provider` trait in `crates/blazecode-core/src/providers/`
3. Register in `providers/mod.rs`
4. Test with `cargo test -- provider_name`
5. Create a PR

## Questions?

Open an issue at https://github.com/sinescode/blazecode/issues
