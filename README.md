# RustCode

A Rust port of [OpenCode](https://github.com/sst/opencode) — an AI-powered development tool.

## Features

- **AI-assisted coding** — LLM-powered agent in your terminal
- **Multi-provider** — Anthropic, OpenAI, Google Gemini, AWS Bedrock, Azure, xAI, OpenRouter, and more
- **Tool ecosystem** — Bash, file read/write/edit, git, ripgrep, image analysis
- **Plugin system** — Config-based, closure, and trait plugins
- **Session management** — Persistent sessions with event sourcing and history
- **MCP support** — Model Context Protocol for tool discovery
- **LSP integration** — Language Server Protocol for code intelligence
- **TUI** — Terminal user interface (ratatui)
- **Server mode** — HTTP/SSE API server

## Quick Start

```bash
curl -sSf https://raw.githubusercontent.com/sinescode/rustcode/main/install | sh
```

Or build from source:
```bash
git clone https://github.com/sinescode/rustcode
cd rustcode
cargo build --release
./target/release/rustcode
```

## Commands

- `rustcode` — Interactive REPL
- `rustcode run` — Run a prompt
- `rustcode serve` — Start HTTP/SSE server
- `rustcode tui` — Terminal UI
- `rustcode session` — Manage sessions
- `rustcode provider` — List/manage providers
- `rustcode config` — View configuration
- `rustcode models` — List available models

## Configuration

Config at `~/.config/opencode/config.json` (shared with OpenCode).

## Architecture

```
rustcode/                    # Workspace root + binary
├── crates/
│   ├── rustcode-core/       # Core library (config, provider, session, tool, etc.)
│   ├── rustcode-server/     # HTTP/SSE API server
│   ├── rustcode-tui/        # Terminal UI
│   ├── rustcode-lsp/        # LSP integration
│   └── rustcode-mcp/        # Model Context Protocol
└── src/main.rs              # CLI entry
```

## Status

Active port in progress. Core modules are structurally complete; provider integrations and session runner are being actively implemented. See `report/` for the full gap analysis against OpenCode.

## License

MIT
