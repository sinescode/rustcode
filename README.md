# BlazeCode

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
curl -sSf https://raw.githubusercontent.com/sinescode/blazecode/main/install | sh
```

Or build from source:
```bash
git clone https://github.com/sinescode/blazecode
cd blazecode
cargo build --release
./target/release/blazecode
```

## Commands

- `blazecode` — Interactive REPL
- `blazecode run` — Run a prompt
- `blazecode serve` — Start HTTP/SSE server
- `blazecode tui` — Terminal UI
- `blazecode session` — Manage sessions
- `blazecode provider` — List/manage providers
- `blazecode config` — View configuration
- `blazecode models` — List available models

## Configuration

Config at `~/.config/blazecode/config.json` (shared with OpenCode).

## Architecture

```
blazecode/                    # Workspace root + binary
├── crates/
│   ├── blazecode-core/       # Core library (config, provider, session, tool, etc.)
│   ├── blazecode-server/     # HTTP/SSE API server
│   ├── blazecode-tui/        # Terminal UI
│   ├── blazecode-lsp/        # LSP integration
│   └── blazecode-mcp/        # Model Context Protocol
└── src/main.rs              # CLI entry
```

## Status

Active port in progress. Core modules are structurally complete; provider integrations and session runner are being actively implemented. See `report/` for the full gap analysis against OpenCode.

## License

MIT
