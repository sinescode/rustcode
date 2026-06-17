#![forbid(unsafe_code)]
#![allow(dead_code, unused_imports)]
#![warn(clippy::all)]

//! rustcode — AI-powered development tool.
//!
//! A Rust port of the OpenCode TypeScript/Bun AI coding agent.
//!
//! # TS Source references
//! - Main entry: `packages/opencode/src/index.ts`
//! - CLI commands: `packages/opencode/src/cli/cmd/*.ts`
//! - Network options: `packages/opencode/src/cli/network.ts`
//! - Effect cmd wrapper: `packages/opencode/src/cli/effect-cmd.ts`
//! - OpenCode commit: 5d0f86606ac30690f79f0a6a9f41a1f49fe95d0b

use clap::{Parser, Subcommand};
use rustcode_core::config::Config;
use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::Arc;

// ── Top-level CLI ───────────────────────────────────────────────────────────
/// AI-powered development tool — Rust port of OpenCode.
///
/// Ported from: `packages/opencode/src/index.ts` — yargs-based CLI
#[derive(Parser)]
#[command(
    name = "rustcode",
    version = env!("CARGO_PKG_VERSION"),
    about = "AI-powered development tool — Rust port of OpenCode",
    long_about = None,
    // TS: `.scriptName("opencode").wrap(100)`
    max_term_width = 100
)]
#[command(arg_required_else_help = true)]
struct Cli {
    /// Print logs to stderr.
    ///
    /// Ported from: `packages/opencode/src/index.ts` — `--print-logs` boolean.
    /// Sets OPENCODE_PRINT_LOGS=1.
    #[arg(long, global = true)]
    print_logs: bool,

    /// Log level.
    ///
    /// Ported from: `packages/opencode/src/index.ts` — `--log-level` choices.
    /// Sets OPENCODE_LOG_LEVEL.
    #[arg(long, global = true, value_name = "LEVEL", default_value = "INFO")]
    log_level: LogLevel,

    /// Run without external plugins.
    ///
    /// Ported from: `packages/opencode/src/index.ts` — `--pure` boolean.
    /// Sets OPENCODE_PURE=1.
    #[arg(long, global = true)]
    pure: bool,

    #[command(subcommand)]
    command: Option<Commands>,
}

/// Log level enum matching TS choices.
///
/// Ported from: `packages/opencode/src/index.ts` —
/// `.option("log-level", { choices: ["DEBUG", "INFO", "WARN", "ERROR"] })`
#[derive(Clone, clap::ValueEnum)]
enum LogLevel {
    Debug,
    Info,
    Warn,
    Error,
}

impl std::fmt::Display for LogLevel {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            LogLevel::Debug => write!(f, "DEBUG"),
            LogLevel::Info => write!(f, "INFO"),
            LogLevel::Warn => write!(f, "WARN"),
            LogLevel::Error => write!(f, "ERROR"),
        }
    }
}

// ── Subcommands ─────────────────────────────────────────────────────────────
/// All subcommands matching the TS CLI.
///
/// Ported from: `packages/opencode/src/index.ts` — 23 `.command(...)` registrations
#[derive(Subcommand)]
enum Commands {
    /// Start ACP (Agent Client Protocol) server.
    ///
    /// Ported from: `packages/opencode/src/cli/cmd/acp.ts`
    Acp(AcpArgs),

    /// Manage MCP (Model Context Protocol) servers.
    ///
    /// Ported from: `packages/opencode/src/cli/cmd/mcp.ts`
    Mcp {
        #[command(subcommand)]
        cmd: McpCommand,
    },

    /// Start OpenCode TUI (terminal user interface).
    ///
    /// Ported from: `packages/opencode/src/cli/cmd/tui.ts` — `$0 [project]`
    Tui(TuiArgs),

    /// Attach to a running OpenCode server.
    ///
    /// Ported from: `packages/opencode/src/cli/cmd/attach.ts` — `attach <url>`
    Attach(AttachArgs),

    /// Run OpenCode with a message (default command).
    ///
    /// Ported from: `packages/opencode/src/cli/cmd/run.ts` — `run [message..]`
    #[command(name = "run")]
    Run(RunArgs),

    /// Generate OpenAPI code samples for the SDK.
    ///
    /// Ported from: `packages/opencode/src/cli/cmd/generate.ts`
    Generate,

    /// Debugging and troubleshooting tools.
    ///
    /// Ported from: `packages/opencode/src/cli/cmd/debug/index.ts`
    Debug {
        #[command(subcommand)]
        cmd: DebugCommand,
    },

    /// Console account management (login, logout, switch, orgs, open).
    ///
    /// Ported from: `packages/opencode/src/cli/cmd/account.ts` — `console`
    #[command(name = "console")]
    Console {
        #[command(subcommand)]
        cmd: ConsoleCommand,
    },

    /// Manage AI providers and credentials.
    ///
    /// Ported from: `packages/opencode/src/cli/cmd/providers.ts` — aliases: `auth`
    #[command(visible_alias = "auth")]
    Providers {
        #[command(subcommand)]
        cmd: ProvidersCommand,
    },

    /// Manage agents.
    ///
    /// Ported from: `packages/opencode/src/cli/cmd/agent.ts`
    #[command(name = "agent")]
    Agent {
        #[command(subcommand)]
        cmd: AgentCommand,
    },

    /// Upgrade OpenCode to the latest or a specific version.
    ///
    /// Ported from: `packages/opencode/src/cli/cmd/upgrade.ts`
    Upgrade(UpgradeArgs),

    /// Uninstall OpenCode and remove all related files.
    ///
    /// Ported from: `packages/opencode/src/cli/cmd/uninstall.ts`
    Uninstall(UninstallArgs),

    /// Start a headless OpenCode server.
    ///
    /// Ported from: `packages/opencode/src/cli/cmd/serve.ts`
    Serve(NetworkArgs),

    /// Start OpenCode server and open web interface.
    ///
    /// Ported from: `packages/opencode/src/cli/cmd/web.ts`
    Web(NetworkArgs),

    /// List all available models.
    ///
    /// Ported from: `packages/opencode/src/cli/cmd/models.ts`
    Models(ModelsArgs),

    /// Show token usage and cost statistics.
    ///
    /// Ported from: `packages/opencode/src/cli/cmd/stats.ts`
    Stats(StatsArgs),

    /// Export session data as JSON.
    ///
    /// Ported from: `packages/opencode/src/cli/cmd/export.ts`
    Export(ExportArgs),

    /// Import session data from JSON file or URL.
    ///
    /// Ported from: `packages/opencode/src/cli/cmd/import.ts`
    Import(ImportArgs),

    /// Manage GitHub agent.
    ///
    /// Ported from: `packages/opencode/src/cli/cmd/github.ts`
    Github {
        #[command(subcommand)]
        cmd: GithubCommand,
    },

    /// Fetch and checkout a GitHub PR branch, then run OpenCode.
    ///
    /// Ported from: `packages/opencode/src/cli/cmd/pr.ts`
    Pr(PrArgs),

    /// Manage sessions.
    ///
    /// Ported from: `packages/opencode/src/cli/cmd/session.ts`
    Session {
        #[command(subcommand)]
        cmd: SessionCommand,
    },

    /// Install plugin and update config.
    ///
    /// Ported from: `packages/opencode/src/cli/cmd/plug.ts` — aliases: `plug`
    #[command(visible_alias = "plug")]
    Plugin(PluginArgs),

    /// Database tools.
    ///
    /// Ported from: `packages/opencode/src/cli/cmd/db.ts`
    Db(DbArgs),

    /// Show version information.
    ///
    /// Ported from: `packages/opencode/src/index.ts` —
    /// `.version("version", "show version number", InstallationVersion)`
    #[command(name = "version")]
    Version,
}

// ── Shared network/flags ────────────────────────────────────────────────────
/// Network/server options shared across serve, web, acp, and tui.
///
/// Ported from: `packages/opencode/src/cli/network.ts`
#[derive(clap::Args)]
struct NetworkArgs {
    /// Port to listen on (0 = random).
    ///
    /// Ported from: `network.ts` — `port` default 0
    #[arg(long, default_value = "0")]
    port: u16,

    /// Hostname to listen on.
    ///
    /// Ported from: `network.ts` — `hostname` default "127.0.0.1"
    #[arg(long, default_value = "127.0.0.1")]
    hostname: String,

    /// Enable mDNS service discovery (defaults hostname to 0.0.0.0).
    ///
    /// Ported from: `network.ts` — `mdns` default false
    #[arg(long, default_value_t = false)]
    mdns: bool,

    /// Custom domain name for mDNS service.
    ///
    /// Ported from: `network.ts` — `mdns-domain` default "opencode.local"
    #[arg(long, default_value = "opencode.local")]
    mdns_domain: String,

    /// Additional domains to allow for CORS.
    ///
    /// Ported from: `network.ts` — `cors` string array, default []
    #[arg(long)]
    cors: Vec<String>,
}

// ── run command ─────────────────────────────────────────────────────────────
/// Arguments for the `run` subcommand.
///
/// Ported from: `packages/opencode/src/cli/cmd/run.ts` — `run [message..]`
#[derive(clap::Args)]
struct RunArgs {
    /// Message to send.
    ///
    /// Ported from: `run.ts` — `.positional("message", { type: "string", array: true, default: [] })`
    /// Also receives `--` args (populate--: true in TS).
    #[arg(trailing_var_arg = true, allow_hyphen_values = true)]
    message: Vec<String>,

    /// The command to run; use message for args.
    ///
    /// Ported from: `run.ts` — `--command` string
    #[arg(long)]
    command: Option<String>,

    /// Continue the last session.
    ///
    /// Ported from: `run.ts` — `--continue` alias `-c`
    #[arg(short = 'c', long)]
    r#continue: bool,

    /// Session ID to continue.
    ///
    /// Ported from: `run.ts` — `--session` alias `-s`
    #[arg(short = 's', long)]
    session: Option<String>,

    /// Fork the session before continuing (requires --continue or --session).
    ///
    /// Ported from: `run.ts` — `--fork` boolean
    #[arg(long)]
    fork: bool,

    /// Share the session.
    ///
    /// Ported from: `run.ts` — `--share` boolean
    #[arg(long)]
    share: bool,

    /// Model to use in the format of provider/model.
    ///
    /// Ported from: `run.ts` — `--model` alias `-m`
    #[arg(short = 'm', long)]
    model: Option<String>,

    /// Agent to use.
    ///
    /// Ported from: `run.ts` — `--agent` string
    #[arg(long)]
    agent: Option<String>,

    /// Output format: default (formatted) or json (raw JSON events).
    ///
    /// Ported from: `run.ts` — `--format` choices: ["default", "json"], default "default"
    #[arg(long, default_value = "default")]
    #[arg(value_parser = clap::builder::PossibleValuesParser::new(["default", "json"]))]
    format: String,

    /// File(s) to attach to message.
    ///
    /// Ported from: `run.ts` — `--file` alias `-f`, array
    #[arg(short = 'f', long)]
    file: Vec<String>,

    /// Title for the session (uses truncated prompt if no value provided).
    ///
    /// Ported from: `run.ts` — `--title` string
    #[arg(long)]
    title: Option<String>,

    /// Attach to a running OpenCode server (e.g., http://localhost:4096).
    ///
    /// Ported from: `run.ts` — `--attach` string
    #[arg(long)]
    attach: Option<String>,

    /// Basic auth password (defaults to OPENCODE_SERVER_PASSWORD).
    ///
    /// Ported from: `run.ts` — `--password` alias `-p`
    #[arg(short = 'p', long)]
    password: Option<String>,

    /// Basic auth username (defaults to OPENCODE_SERVER_USERNAME or 'opencode').
    ///
    /// Ported from: `run.ts` — `--username` alias `-u`
    #[arg(short = 'u', long)]
    username: Option<String>,

    /// Directory to run in; path on remote server if attaching.
    ///
    /// Ported from: `run.ts` — `--dir` string
    #[arg(long)]
    dir: Option<String>,

    /// Port for the local server (defaults to random port if no value provided).
    ///
    /// Ported from: `run.ts` — `--port` number
    #[arg(long)]
    port: Option<u16>,

    /// Model variant (provider-specific reasoning effort, e.g., high, max, minimal).
    ///
    /// Ported from: `run.ts` — `--variant` string
    #[arg(long)]
    variant: Option<String>,

    /// Show thinking blocks.
    ///
    /// Ported from: `run.ts` — `--thinking` boolean
    #[arg(long)]
    thinking: bool,

    /// Replay interactive session history on resume and after resize.
    ///
    /// Ported from: `run.ts` — `--replay` boolean, default true.
    /// Use `--no-replay` to disable.
    #[arg(long, default_value_t = true)]
    replay: bool,

    /// Cap visible interactive replay to the newest N messages.
    ///
    /// Ported from: `run.ts` — `--replay-limit` number
    #[arg(long)]
    replay_limit: Option<usize>,

    /// Run in direct interactive split-footer mode.
    ///
    /// Ported from: `run.ts` — `--interactive` alias `-i`
    #[arg(short = 'i', long)]
    interactive: bool,

    /// Auto-approve permissions that are not explicitly denied (dangerous!).
    ///
    /// Ported from: `run.ts` — `--dangerously-skip-permissions` boolean, default false
    #[arg(long, default_value_t = false)]
    dangerously_skip_permissions: bool,

    /// Enable direct interactive demo slash commands.
    ///
    /// Ported from: `run.ts` — `--demo` boolean, default false
    #[arg(long, default_value_t = false)]
    demo: bool,
}

// ── tui command ─────────────────────────────────────────────────────────────
/// Arguments for the `tui` subcommand.
///
/// Ported from: `packages/opencode/src/cli/cmd/tui.ts` — `$0 [project]`
#[derive(clap::Args)]
struct TuiArgs {
    /// Path to start OpenCode in.
    ///
    /// Ported from: `tui.ts` — `.positional("project", { type: "string" })`
    project: Option<String>,

    #[command(flatten)]
    network: NetworkArgs,

    /// Model to use in the format of provider/model.
    ///
    /// Ported from: `tui.ts` — `--model` alias `-m`
    #[arg(short = 'm', long)]
    model: Option<String>,

    /// Continue the last session.
    ///
    /// Ported from: `tui.ts` — `--continue` alias `-c`
    #[arg(short = 'c', long)]
    r#continue: bool,

    /// Session ID to continue.
    ///
    /// Ported from: `tui.ts` — `--session` alias `-s`
    #[arg(short = 's', long)]
    session: Option<String>,

    /// Fork the session when continuing (use with --continue or --session).
    ///
    /// Ported from: `tui.ts` — `--fork` boolean
    #[arg(long)]
    fork: bool,

    /// Prompt to use.
    ///
    /// Ported from: `tui.ts` — `--prompt` string
    #[arg(long)]
    prompt: Option<String>,

    /// Agent to use.
    ///
    /// Ported from: `tui.ts` — `--agent` string
    #[arg(long)]
    agent: Option<String>,
}

// ── attach command ──────────────────────────────────────────────────────────
/// Arguments for the `attach` subcommand.
///
/// Ported from: `packages/opencode/src/cli/cmd/attach.ts`
#[derive(clap::Args)]
struct AttachArgs {
    /// Server URL (e.g., http://localhost:4096).
    ///
    /// Ported from: `attach.ts` — `.positional("url", { type: "string", demandOption: true })`
    url: String,

    /// Directory to run in.
    ///
    /// Ported from: `attach.ts` — `--dir` string
    #[arg(long)]
    dir: Option<String>,

    /// Continue the last session.
    ///
    /// Ported from: `attach.ts` — `--continue` alias `-c`
    #[arg(short = 'c', long)]
    r#continue: bool,

    /// Session ID to continue.
    ///
    /// Ported from: `attach.ts` — `--session` alias `-s`
    #[arg(short = 's', long)]
    session: Option<String>,

    /// Fork the session when continuing (use with --continue or --session).
    ///
    /// Ported from: `attach.ts` — `--fork` boolean
    #[arg(long)]
    fork: bool,

    /// Basic auth password (defaults to OPENCODE_SERVER_PASSWORD).
    ///
    /// Ported from: `attach.ts` — `--password` alias `-p`
    #[arg(short = 'p', long)]
    password: Option<String>,

    /// Basic auth username (defaults to OPENCODE_SERVER_USERNAME or 'opencode').
    ///
    /// Ported from: `attach.ts` — `--username` alias `-u`
    #[arg(short = 'u', long)]
    username: Option<String>,
}

// ── upgrade command ─────────────────────────────────────────────────────────
/// Arguments for the `upgrade` subcommand.
///
/// Ported from: `packages/opencode/src/cli/cmd/upgrade.ts`
#[derive(clap::Args)]
struct UpgradeArgs {
    /// Version to upgrade to (e.g., '0.1.48' or 'v0.1.48').
    ///
    /// Ported from: `upgrade.ts` — `.positional("target", { type: "string" })`
    target: Option<String>,

    /// Installation method to use.
    ///
    /// Ported from: `upgrade.ts` — `--method` alias `-m`,
    /// choices: ["curl", "npm", "pnpm", "bun", "brew", "choco", "scoop"]
    #[arg(short = 'm', long)]
    #[arg(value_parser = clap::builder::PossibleValuesParser::new([
        "curl", "npm", "pnpm", "bun", "brew", "choco", "scoop"
    ]))]
    method: Option<String>,
}

// ── uninstall command ───────────────────────────────────────────────────────
/// Arguments for the `uninstall` subcommand.
///
/// Ported from: `packages/opencode/src/cli/cmd/uninstall.ts`
#[derive(clap::Args)]
struct UninstallArgs {
    /// Keep configuration files.
    ///
    /// Ported from: `uninstall.ts` — `--keep-config` alias `-c`, default false
    #[arg(short = 'c', long, default_value_t = false)]
    keep_config: bool,

    /// Keep session data and snapshots.
    ///
    /// Ported from: `uninstall.ts` — `--keep-data` alias `-d`, default false
    #[arg(short = 'd', long, default_value_t = false)]
    keep_data: bool,

    /// Show what would be removed without removing.
    ///
    /// Ported from: `uninstall.ts` — `--dry-run`, default false
    #[arg(long, default_value_t = false)]
    dry_run: bool,

    /// Skip confirmation prompts.
    ///
    /// Ported from: `uninstall.ts` — `--force` alias `-f`, default false
    #[arg(short = 'f', long, default_value_t = false)]
    force: bool,
}

// ── models command ──────────────────────────────────────────────────────────
/// Arguments for the `models` subcommand.
///
/// Ported from: `packages/opencode/src/cli/cmd/models.ts`
#[derive(clap::Args)]
struct ModelsArgs {
    /// Provider ID to filter models by.
    ///
    /// Ported from: `models.ts` — `.positional("provider", { type: "string" })`
    provider: Option<String>,

    /// Use more verbose model output (includes metadata like costs).
    ///
    /// Ported from: `models.ts` — `--verbose` boolean
    #[arg(long)]
    verbose: bool,

    /// Refresh the models cache from models.dev.
    ///
    /// Ported from: `models.ts` — `--refresh` boolean
    #[arg(long)]
    refresh: bool,
}

// ── stats command ───────────────────────────────────────────────────────────
/// Arguments for the `stats` subcommand.
///
/// Ported from: `packages/opencode/src/cli/cmd/stats.ts`
#[derive(clap::Args)]
struct StatsArgs {
    /// Show stats for the last N days (default: all time).
    ///
    /// Ported from: `stats.ts` — `--days` number
    #[arg(long)]
    days: Option<u32>,

    /// Number of tools to show (default: all).
    ///
    /// Ported from: `stats.ts` — `--tools` number
    #[arg(long)]
    tools: Option<usize>,

    /// Number of models to show (default: hidden, true = all, N = top N).
    ///
    /// Ported from: `stats.ts` — `--models` can be boolean or number
    #[arg(long)]
    models: Option<usize>,

    /// Filter by project (default: all projects, empty string: current project).
    ///
    /// Ported from: `stats.ts` — `--project` string
    #[arg(long)]
    project: Option<String>,
}

// ── export command ──────────────────────────────────────────────────────────
/// Arguments for the `export` subcommand.
///
/// Ported from: `packages/opencode/src/cli/cmd/export.ts`
#[derive(clap::Args)]
struct ExportArgs {
    /// Session ID to export.
    ///
    /// Ported from: `export.ts` — `.positional("sessionID", { type: "string" })`
    session_id: Option<String>,

    /// Redact sensitive transcript and file data.
    ///
    /// Ported from: `export.ts` — `--sanitize` boolean
    #[arg(long)]
    sanitize: bool,
}

// ── import command ──────────────────────────────────────────────────────────
/// Arguments for the `import` subcommand.
///
/// Ported from: `packages/opencode/src/cli/cmd/import.ts`
#[derive(clap::Args)]
struct ImportArgs {
    /// Path to JSON file or share URL.
    ///
    /// Ported from: `import.ts` — `.positional("file", { type: "string", demandOption: true })`
    file: String,
}

// ── pr command ──────────────────────────────────────────────────────────────
/// Arguments for the `pr` subcommand.
///
/// Ported from: `packages/opencode/src/cli/cmd/pr.ts`
#[derive(clap::Args)]
struct PrArgs {
    /// PR number to checkout.
    ///
    /// Ported from: `pr.ts` — `.positional("number", { type: "number", demandOption: true })`
    number: u64,
}

// ── plugin command ──────────────────────────────────────────────────────────
/// Arguments for the `plugin` subcommand.
///
/// Ported from: `packages/opencode/src/cli/cmd/plug.ts`
#[derive(clap::Args)]
struct PluginArgs {
    /// npm module name.
    ///
    /// Ported from: `plug.ts` — `.positional("module", { type: "string" })`
    module: String,

    /// Install in global config.
    ///
    /// Ported from: `plug.ts` — `--global` alias `-g`, default false
    #[arg(short = 'g', long, default_value_t = false)]
    global: bool,

    /// Replace existing plugin version.
    ///
    /// Ported from: `plug.ts` — `--force` alias `-f`, default false
    #[arg(short = 'f', long, default_value_t = false)]
    force: bool,
}

// ── db command ──────────────────────────────────────────────────────────────
/// Arguments for the `db` subcommand.
///
/// Ported from: `packages/opencode/src/cli/cmd/db.ts`
#[derive(clap::Args)]
struct DbArgs {
    /// SQL query to execute (if omitted, opens interactive sqlite3 shell).
    ///
    /// Ported from: `db.ts` — `.positional("query", { type: "string" })`
    query: Option<String>,

    /// Output format for query results.
    ///
    /// Ported from: `db.ts` — `--format` choices: ["json", "tsv"], default "tsv"
    #[arg(long, default_value = "tsv")]
    #[arg(value_parser = clap::builder::PossibleValuesParser::new(["json", "tsv"]))]
    format: String,
}

// ── ACP command (standalone args + network) ─────────────────────────────────
/// Arguments for the `acp` subcommand.
///
/// Ported from: `packages/opencode/src/cli/cmd/acp.ts`
#[derive(clap::Args)]
struct AcpArgs {
    #[command(flatten)]
    network: NetworkArgs,

    /// Working directory.
    ///
    /// Ported from: `acp.ts` — `--cwd` string, default process.cwd()
    #[arg(long)]
    cwd: Option<String>,
}

// ── MCP subcommands ─────────────────────────────────────────────────────────
/// MCP subcommands.
///
/// Ported from: `packages/opencode/src/cli/cmd/mcp.ts`
#[derive(Subcommand)]
enum McpCommand {
    /// Add an MCP server.
    ///
    /// Ported from: `mcp.ts` — `mcp add [name]`
    #[command(name = "add")]
    Add {
        /// Name of the MCP server.
        name: Option<String>,

        /// URL for a remote MCP server.
        ///
        /// Ported from: `mcp.ts` — `--url` string
        #[arg(long)]
        url: Option<String>,

        /// Environment variable for a local MCP server (KEY=VALUE).
        ///
        /// Ported from: `mcp.ts` — `--env` string array
        #[arg(long)]
        env: Vec<String>,

        /// HTTP header for a remote MCP server (KEY=VALUE).
        ///
        /// Ported from: `mcp.ts` — `--header` string array
        #[arg(long)]
        header: Vec<String>,
    },

    /// List MCP servers and their status.
    ///
    /// Ported from: `mcp.ts` — `mcp list` / `mcp ls`
    #[command(name = "list", visible_alias = "ls")]
    List,

    /// Authenticate with an OAuth-enabled MCP server.
    ///
    /// Ported from: `mcp.ts` — `mcp auth [name]` (also has `auth list` sub)
    #[command(name = "auth")]
    Auth {
        /// Name of the MCP server.
        name: Option<String>,
    },

    /// Remove OAuth credentials for an MCP server.
    ///
    /// Ported from: `mcp.ts` — `mcp logout [name]`
    #[command(name = "logout")]
    Logout {
        /// Name of the MCP server.
        name: Option<String>,
    },

    /// Debug OAuth connection for an MCP server.
    ///
    /// Ported from: `mcp.ts` — `mcp debug <name>`
    #[command(name = "debug")]
    Debug {
        /// Name of the MCP server (required).
        name: String,
    },
}

// ── debug subcommands ───────────────────────────────────────────────────────
/// Debug subcommands.
///
/// Ported from: `packages/opencode/src/cli/cmd/debug/index.ts`
#[derive(Subcommand)]
enum DebugCommand {
    /// Show resolved configuration.
    ///
    /// Ported from: `packages/opencode/src/cli/cmd/debug/config.ts`
    Config,

    /// LSP debugging utilities.
    ///
    /// Ported from: `packages/opencode/src/cli/cmd/debug/lsp.ts`
    Lsp {
        #[command(subcommand)]
        cmd: DebugLspCommand,
    },

    /// Ripgrep debugging utilities.
    ///
    /// Ported from: `packages/opencode/src/cli/cmd/debug/ripgrep.ts` — `debug rg`
    #[command(name = "rg")]
    Rg {
        #[command(subcommand)]
        cmd: DebugRgCommand,
    },

    /// File system debugging utilities.
    ///
    /// Ported from: `packages/opencode/src/cli/cmd/debug/file.ts`
    File {
        #[command(subcommand)]
        cmd: DebugFileCommand,
    },

    /// List all known projects.
    ///
    /// Ported from: `packages/opencode/src/cli/cmd/debug/scrap.ts`
    Scrap,

    /// List all available skills.
    ///
    /// Ported from: `packages/opencode/src/cli/cmd/debug/skill.ts`
    Skill,

    /// Snapshot debugging utilities.
    ///
    /// Ported from: `packages/opencode/src/cli/cmd/debug/snapshot.ts`
    Snapshot {
        #[command(subcommand)]
        cmd: DebugSnapshotCommand,
    },

    /// Print startup timing.
    ///
    /// Ported from: `packages/opencode/src/cli/cmd/debug/startup.ts`
    Startup,

    /// Show agent configuration details.
    ///
    /// Ported from: `packages/opencode/src/cli/cmd/debug/agent.ts`
    Agent {
        /// Agent name (required).
        name: String,

        /// Tool ID to execute.
        ///
        /// Ported from: `debug/agent.ts` — `--tool` string
        #[arg(long)]
        tool: Option<String>,

        /// Tool params as JSON or a JS object literal.
        ///
        /// Ported from: `debug/agent.ts` — `--params` string
        #[arg(long)]
        params: Option<String>,
    },

    /// Debug v2 catalog and built-in plugins.
    ///
    /// Ported from: `packages/opencode/src/cli/cmd/debug/v2.ts`
    V2,

    /// Show debug information.
    ///
    /// Ported from: `debug/index.ts` — `debug info`
    Info,

    /// Show global paths (data, config, cache, state).
    ///
    /// Ported from: `debug/index.ts` — `debug paths`
    Paths,

    /// Wait indefinitely (for debugging).
    ///
    /// Ported from: `debug/index.ts` — `debug wait`
    Wait,
}

/// LSP debug subcommands.
///
/// Ported from: `packages/opencode/src/cli/cmd/debug/lsp.ts`
#[derive(Subcommand)]
enum DebugLspCommand {
    /// Get diagnostics for a file.
    Diagnostics {
        /// File path (required).
        file: String,
    },

    /// Search workspace symbols.
    Symbols {
        /// Search query (required).
        query: String,
    },

    /// Get symbols from a document.
    #[command(name = "document-symbols")]
    DocumentSymbols {
        /// Document URI (required).
        uri: String,
    },
}

/// Ripgrep debug subcommands.
///
/// Ported from: `packages/opencode/src/cli/cmd/debug/ripgrep.ts`
#[derive(Subcommand)]
enum DebugRgCommand {
    /// List files using ripgrep.
    Files {
        /// Filter files by query.
        #[arg(long)]
        query: Option<String>,

        /// Glob pattern to match files.
        #[arg(long)]
        glob: Option<String>,

        /// Limit number of results.
        #[arg(long)]
        limit: Option<usize>,
    },

    /// Search file contents using ripgrep.
    Search {
        /// Search pattern (required).
        pattern: String,

        /// File glob patterns.
        #[arg(long)]
        glob: Vec<String>,

        /// Limit number of results.
        #[arg(long)]
        limit: Option<usize>,
    },
}

/// File debug subcommand.
///
/// Ported from: `packages/opencode/src/cli/cmd/debug/file.ts`
#[derive(Subcommand)]
enum DebugFileCommand {
    /// Search files by query.
    Search {
        /// Search query (required).
        query: String,
    },

    /// Read file contents as JSON.
    Read {
        /// File path to read (required).
        path: String,
    },

    /// List files in a directory.
    List {
        /// File path to list (required).
        path: String,
    },
}

/// Snapshot debug subcommands.
///
/// Ported from: `packages/opencode/src/cli/cmd/debug/snapshot.ts`
#[derive(Subcommand)]
enum DebugSnapshotCommand {
    /// Track current snapshot state.
    Track,

    /// Show patch for a snapshot hash.
    Patch {
        /// Snapshot hash (required).
        hash: String,
    },

    /// Show diff for a snapshot hash.
    Diff {
        /// Snapshot hash (required).
        hash: String,
    },
}

// ── console (account) subcommands ───────────────────────────────────────────
/// Console / account subcommands.
///
/// Ported from: `packages/opencode/src/cli/cmd/account.ts`
#[derive(Subcommand)]
enum ConsoleCommand {
    /// Log in to console.
    ///
    /// Ported from: `account.ts` — `console login [url]`
    Login {
        /// Server URL (default: https://console.opencode.ai).
        url: Option<String>,
    },

    /// Log out from console.
    ///
    /// Ported from: `account.ts` — `console logout [email]`
    Logout {
        /// Account email to log out from.
        email: Option<String>,
    },

    /// Switch active org.
    ///
    /// Ported from: `account.ts` — `console switch`
    Switch,

    /// List orgs.
    ///
    /// Ported from: `account.ts` — `console orgs`
    Orgs,

    /// Open active console account.
    ///
    /// Ported from: `account.ts` — `console open`
    Open,
}

// ── providers subcommands ───────────────────────────────────────────────────
/// Providers subcommands.
///
/// Ported from: `packages/opencode/src/cli/cmd/providers.ts`
#[derive(Subcommand)]
enum ProvidersCommand {
    /// List providers and credentials.
    ///
    /// Ported from: `providers.ts` — `providers list` / `providers ls`
    #[command(name = "list", visible_alias = "ls")]
    List,

    /// Log in to a provider.
    ///
    /// Ported from: `providers.ts` — `providers login [url]`
    Login {
        /// OpenCode auth provider URL.
        url: Option<String>,

        /// Provider ID or name to log in to (skips provider selection).
        ///
        /// Ported from: `providers.ts` — `--provider` alias `-p`
        #[arg(short = 'p', long)]
        provider: Option<String>,

        /// Login method label (skips method selection).
        ///
        /// Ported from: `providers.ts` — `--method` alias `-m`
        #[arg(short = 'm', long)]
        method: Option<String>,
    },

    /// Log out from a configured provider.
    ///
    /// Ported from: `providers.ts` — `providers logout [provider]`
    Logout {
        /// Provider ID or name to log out from.
        provider: Option<String>,
    },
}

// ── agent subcommands ───────────────────────────────────────────────────────
/// Agent subcommands.
///
/// Ported from: `packages/opencode/src/cli/cmd/agent.ts`
#[derive(Subcommand)]
enum AgentCommand {
    /// Create a new agent.
    Create {
        /// Directory path to generate the agent file.
        ///
        /// Ported from: `agent.ts` — `--path` string
        #[arg(long)]
        path: Option<String>,

        /// What the agent should do.
        ///
        /// Ported from: `agent.ts` — `--description` string
        #[arg(long)]
        description: Option<String>,

        /// Agent mode: all, primary, or subagent.
        ///
        /// Ported from: `agent.ts` — `--mode` choices
        #[arg(long)]
        #[arg(value_parser = clap::builder::PossibleValuesParser::new(["all", "primary", "subagent"]))]
        mode: Option<String>,

        /// Comma-separated list of permissions to allow.
        ///
        /// Ported from: `agent.ts` — `--permissions` alias `--tools`
        #[arg(long, visible_alias = "tools")]
        permissions: Option<String>,

        /// Model to use in the format of provider/model.
        ///
        /// Ported from: `agent.ts` — `--model` alias `-m`
        #[arg(short = 'm', long)]
        model: Option<String>,
    },

    /// List all available agents.
    List,
}

// ── github subcommands ──────────────────────────────────────────────────────
/// GitHub agent subcommands.
///
/// Ported from: `packages/opencode/src/cli/cmd/github.ts`
#[derive(Subcommand)]
enum GithubCommand {
    /// Install the GitHub agent.
    Install,

    /// Run the GitHub agent.
    Run {
        /// GitHub mock event to run the agent for.
        ///
        /// Ported from: `github.ts` — `--event` string
        #[arg(long)]
        event: Option<String>,

        /// GitHub personal access token (github_pat_********).
        ///
        /// Ported from: `github.ts` — `--token` string
        #[arg(long)]
        token: Option<String>,
    },
}

// ── session subcommands ─────────────────────────────────────────────────────
/// Session subcommands.
///
/// Ported from: `packages/opencode/src/cli/cmd/session.ts`
#[derive(Subcommand)]
enum SessionCommand {
    /// List sessions.
    List {
        /// Limit to N most recent sessions.
        ///
        /// Ported from: `session.ts` — `--max-count` alias `n`
        #[arg(short = 'n', long = "max-count")]
        max_count: Option<usize>,

        /// Output format.
        ///
        /// Ported from: `session.ts` — `--format` choices: ["table", "json"], default "table"
        #[arg(long, default_value = "table")]
        #[arg(value_parser = clap::builder::PossibleValuesParser::new(["table", "json"]))]
        format: String,
    },

    /// Delete a session.
    Delete {
        /// Session ID to delete (required).
        ///
        /// Ported from: `session.ts` — `delete <sessionID>`
        session_id: String,
    },
}

// ═════════════════════════════════════════════════════════════════════════════
// main
// ═════════════════════════════════════════════════════════════════════════════

fn main() {
    let cli = Cli::parse();

    // Initialize tracing subscriber.
    //
    // Ported from: `packages/opencode/src/index.ts` — middleware sets
    // OPENCODE_PRINT_LOGS and OPENCODE_LOG_LEVEL env vars. We mirror that
    // via clap args and configure tracing accordingly.
    let env_filter = if cli.print_logs {
        tracing_subscriber::EnvFilter::try_from_default_env()
            .unwrap_or_else(|_| tracing_subscriber::EnvFilter::new(cli.log_level.to_string()))
    } else {
        tracing_subscriber::EnvFilter::try_from_default_env()
            .unwrap_or_else(|_| tracing_subscriber::EnvFilter::new("off"))
    };

    tracing_subscriber::fmt()
        .with_env_filter(env_filter)
        .with_target(false)
        .init();

    tracing::info!(
        "rustcode starting (version={}, pure={}, print_logs={})",
        env!("CARGO_PKG_VERSION"),
        cli.pure,
        cli.print_logs
    );

    // Build the async runtime and dispatch.
    //
    // Ported from: `packages/opencode/src/index.ts` — `await cli.parse()`.
    // Each command handler can be async. Use current_thread for a CLI.
    let rt = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .expect("Failed to build tokio runtime");

    rt.block_on(async_main(cli));
}

/// Main async entry point.
///
/// Ported from: `packages/opencode/src/index.ts` —
/// `try { await cli.parse() } catch (e) { ... }`
async fn async_main(cli: Cli) {
    // Load config eagerly (matches TS middleware that sets env vars).
    //
    // Ported from: `packages/opencode/src/index.ts` — middleware sets
    // AGENT=1, OPENCODE=1, OPENCODE_PID.
    let config = Config::load().unwrap_or_default();
    tracing::debug!("Config loaded ({} providers)", config.provider.len());

    let exit_code = match &cli.command {
        Some(cmd) => dispatch(cmd).await,
        None => {
            // No subcommand given — show help.
            // Ported from: TS — when no subcommand is matched, yargs shows help.
            eprintln!("Use --help for usage information.");
            0
        }
    };

    if exit_code != 0 {
        std::process::exit(exit_code);
    }
}

/// Dispatch to the appropriate subcommand handler.
///
/// Each handler returns an exit code (0 = success, non-zero = failure).
async fn dispatch(cmd: &Commands) -> i32 {
    match cmd {
        Commands::Acp(args) => cmd_acp(args).await,
        Commands::Mcp { cmd: mcp_cmd } => cmd_mcp(mcp_cmd).await,
        Commands::Tui(args) => cmd_tui(args).await,
        Commands::Attach(args) => cmd_attach(args).await,
        Commands::Run(args) => cmd_run(args).await,
        Commands::Generate => cmd_generate().await,
        Commands::Debug { cmd: debug_cmd } => cmd_debug(debug_cmd).await,
        Commands::Console { cmd: console_cmd } => cmd_console(console_cmd).await,
        Commands::Providers { cmd: providers_cmd } => cmd_providers(providers_cmd).await,
        Commands::Agent { cmd: agent_cmd } => cmd_agent(agent_cmd).await,
        Commands::Upgrade(args) => cmd_upgrade(args).await,
        Commands::Uninstall(args) => cmd_uninstall(args).await,
        Commands::Serve(args) => cmd_serve(args).await,
        Commands::Web(args) => cmd_web(args).await,
        Commands::Models(args) => cmd_models(args).await,
        Commands::Stats(args) => cmd_stats(args).await,
        Commands::Export(args) => cmd_export(args).await,
        Commands::Import(args) => cmd_import(args).await,
        Commands::Github { cmd: gh_cmd } => cmd_github(gh_cmd).await,
        Commands::Pr(args) => cmd_pr(args).await,
        Commands::Session { cmd: session_cmd } => cmd_session(session_cmd).await,
        Commands::Plugin(args) => cmd_plugin(args).await,
        Commands::Db(args) => cmd_db(args).await,
        Commands::Version => {
            cmd_version();
            0
        }
    }
}

// ═════════════════════════════════════════════════════════════════════════════
// command handlers — each returns an exit code
// ═════════════════════════════════════════════════════════════════════════════

/// `run` — Run OpenCode with a message.
///
/// Ported from: `packages/opencode/src/cli/cmd/run.ts`
async fn cmd_run(args: &RunArgs) -> i32 {
    let msg = args.message.join(" ");

    // TS: "You must provide a message or a command"
    if msg.is_empty() && args.command.is_none() && !args.interactive {
        eprintln!("Error: You must provide a message or a command");
        return 1;
    }

    // TS: "--interactive cannot be used with --command"
    if args.interactive && args.command.is_some() {
        eprintln!("Error: --interactive cannot be used with --command");
        return 1;
    }

    // TS: "--demo requires --interactive"
    if args.demo && !args.interactive {
        eprintln!("Error: --demo requires --interactive");
        return 1;
    }

    // TS: "--interactive cannot be used with --format json"
    if args.interactive && args.format == "json" {
        eprintln!("Error: --interactive cannot be used with --format json");
        return 1;
    }

    // TS: "--replay-limit requires --interactive"
    if args.replay_limit.is_some() && !args.interactive {
        eprintln!("Error: --replay-limit requires --interactive");
        return 1;
    }

    // TS: "--replay-limit must be a positive integer"
    if let Some(limit) = args.replay_limit {
        if limit == 0 {
            eprintln!("Error: --replay-limit must be a positive integer");
            return 1;
        }
    }

    // TS: "--fork requires --continue or --session"
    if args.fork && !args.r#continue && args.session.is_none() {
        eprintln!("Error: --fork requires --continue or --session");
        return 1;
    }

    tracing::info!(
        "run: msg={:?}, interactive={}, model={:?}, agent={:?}, format={}",
        msg,
        args.interactive,
        args.model,
        args.agent,
        args.format,
    );

    // TODO: Wire to rustcode-core session management and LLM backend.
    eprintln!("run: LLM integration not yet implemented.");
    0
}

/// `serve` — Start a headless OpenCode server.
///
/// Ported from: `packages/opencode/src/cli/cmd/serve.ts`
async fn cmd_serve(args: &NetworkArgs) -> i32 {
    tracing::info!(
        "serve: hostname={}, port={}, mdns={}",
        args.hostname,
        args.port,
        args.mdns
    );

    // TODO: Wire to rustcode-server crate.
    // TS: `yield* Effect.never` — blocks forever.
    eprintln!("serve: server startup not yet implemented.");
    0
}

/// `web` — Start server and open web interface.
///
/// Ported from: `packages/opencode/src/cli/cmd/web.ts`
async fn cmd_web(args: &NetworkArgs) -> i32 {
    tracing::info!(
        "web: hostname={}, port={}, mdns={}",
        args.hostname,
        args.port,
        args.mdns
    );

    // TODO: Wire to rustcode-server. Open browser to server URL.
    eprintln!("web: server + browser launch not yet implemented.");
    0
}

/// `models` — List all available models.
///
/// Ported from: `packages/opencode/src/cli/cmd/models.ts`
async fn cmd_models(args: &ModelsArgs) -> i32 {
    if args.refresh {
        tracing::info!("Refreshing models cache...");
        // TODO: yield* ModelsDev.Service.use((s) => s.refresh(true))
    }

    if let Some(provider) = &args.provider {
        tracing::info!("Listing models for provider: {}", provider);
    } else {
        tracing::info!("Listing all models");
    }

    // TODO: Wire to rustcode-core provider registry.
    eprintln!("models: provider registry not yet implemented.");
    0
}

/// `stats` — Show token usage and cost statistics.
///
/// Ported from: `packages/opencode/src/cli/cmd/stats.ts`
async fn cmd_stats(args: &StatsArgs) -> i32 {
    tracing::info!(
        "stats: days={:?}, tools={:?}, models={:?}",
        args.days,
        args.tools,
        args.models
    );

    // TODO: Aggregate session stats from storage.
    eprintln!("stats: usage aggregation not yet implemented.");
    0
}

/// `export` — Export session data as JSON.
///
/// Ported from: `packages/opencode/src/cli/cmd/export.ts`
async fn cmd_export(args: &ExportArgs) -> i32 {
    let session_id = args.session_id.as_deref().unwrap_or("latest");
    tracing::info!(
        "export: session={}, sanitize={}",
        session_id,
        args.sanitize
    );

    // TODO: Load session from storage and serialize to stdout.
    eprintln!("export: session export not yet implemented.");
    0
}

/// `import` — Import session data from JSON file or URL.
///
/// Ported from: `packages/opencode/src/cli/cmd/import.ts`
async fn cmd_import(args: &ImportArgs) -> i32 {
    let is_url = args.file.starts_with("http://") || args.file.starts_with("https://");
    tracing::info!("import: file={}, is_url={}", args.file, is_url);

    // TODO: Parse JSON file or fetch URL, deserialize, write to storage.
    eprintln!("import: session import not yet implemented.");
    0
}

/// `session` — Manage sessions (list, delete).
///
/// Ported from: `packages/opencode/src/cli/cmd/session.ts`
async fn cmd_session(cmd: &SessionCommand) -> i32 {
    match cmd {
        SessionCommand::List { max_count, format } => {
            tracing::info!(
                "session list: max_count={:?}, format={}",
                max_count,
                format
            );
            // TODO: Query storage for session list.
            eprintln!("session list: not yet implemented.");
            0
        }
        SessionCommand::Delete { session_id } => {
            tracing::info!("session delete: id={}", session_id);
            // TODO: Call Session.Service.remove(sessionID).
            eprintln!("session delete: not yet implemented.");
            0
        }
    }
}

/// `tui` — Start OpenCode TUI.
///
/// Ported from: `packages/opencode/src/cli/cmd/tui.ts`
async fn cmd_tui(args: &TuiArgs) -> i32 {
    if args.fork && !args.r#continue && args.session.is_none() {
        eprintln!("Error: --fork requires --continue or --session");
        return 1;
    }

    tracing::info!(
        "tui: project={:?}, model={:?}, continue={}, session={:?}",
        args.project, args.model, args.r#continue, args.session,
    );

    // ── Bootstrap backend services ──────────────────────────────────
    use rustcode_core::bus;
    use rustcode_core::session::SessionManager;
    use rustcode_core::session_runner::SessionRunner;
    use rustcode_core::tool::ToolRegistry;

    let shared_bus = bus::SharedBus::new(256);
    let sessions = Arc::new(SessionManager::new(shared_bus.clone()));
    let tools = Arc::new(ToolRegistry::new());
    let runner = Arc::new(SessionRunner::new(tools.clone()));

    // Auto-detect providers from environment
    let providers_map: HashMap<String, Box<dyn rustcode_core::provider::Provider>> =
        rustcode_core::providers::auto_detect_all()
            .into_iter()
            .map(|p| {
                let id = p.provider_id().to_string();
                (id, p)
            })
            .collect();

    if providers_map.is_empty() {
        eprintln!("No LLM providers detected. Set an API key environment variable:");
        eprintln!("  ANTHROPIC_API_KEY  — for Claude (Anthropic)");
        eprintln!("  OPENAI_API_KEY     — for GPT (OpenAI)");
        eprintln!("  GOOGLE_GENERATIVE_AI_API_KEY — for Gemini (Google)");
        eprintln!("  OPENROUTER_API_KEY — for OpenRouter (multi-provider)");
        eprintln!("  DEEPSEEK_API_KEY   — for DeepSeek");
        eprintln!("  GROQ_API_KEY       — for Groq");
        eprintln!("  ...and more (see docs for full list)");
        eprintln!();
        eprintln!("Continuing in offline mode — prompts will not call an LLM.");
    } else {
        tracing::info!("Detected {} provider(s)", providers_map.len());
        for (id, _) in &providers_map {
            tracing::info!("  - {id}");
        }
    }

    // ── Launch TUI ─────────────────────────────────────────────────
    match rustcode_tui::TuiApp::new(sessions, runner, providers_map, shared_bus) {
        Ok(mut app) => {
            // Run the TUI — this blocks until the user quits
            let rt = tokio::runtime::Runtime::new().unwrap();
            if let Err(e) = rt.block_on(app.run_async()) {
                eprintln!("TUI error: {e}");
                let _ = app.cleanup();
                return 1;
            }
            if let Err(e) = app.cleanup() {
                eprintln!("Terminal cleanup error: {e}");
                return 1;
            }
        }
        Err(e) => {
            eprintln!("Failed to initialize TUI: {e}");
            return 1;
        }
    }

    0
}

/// `attach` — Attach to a running OpenCode server.
///
/// Ported from: `packages/opencode/src/cli/cmd/attach.ts`
async fn cmd_attach(args: &AttachArgs) -> i32 {
    // TS: "--fork requires --continue or --session"
    if args.fork && !args.r#continue && args.session.is_none() {
        eprintln!("Error: --fork requires --continue or --session");
        return 1;
    }

    tracing::info!(
        "attach: url={}, dir={:?}, continue={}, session={:?}",
        args.url,
        args.dir,
        args.r#continue,
        args.session,
    );

    // TODO: Connect to remote server via rustcode-server client.
    eprintln!("attach: server connection not yet implemented.");
    0
}

/// `generate` — Generate OpenAPI code samples for the SDK.
///
/// Ported from: `packages/opencode/src/cli/cmd/generate.ts`
async fn cmd_generate() -> i32 {
    tracing::info!("generate: OpenAPI code samples");

    // TODO: Generate OpenAPI specs and emit code samples as JSON.
    eprintln!("generate: OpenAPI generation not yet implemented.");
    0
}

/// `debug` — Debugging and troubleshooting tools.
///
/// Ported from: `packages/opencode/src/cli/cmd/debug/index.ts`
async fn cmd_debug(cmd: &DebugCommand) -> i32 {
    match cmd {
        DebugCommand::Config => {
            tracing::info!("debug config");
            // TODO: Emit resolved config as JSON to stdout.
            eprintln!("debug config: not yet implemented.");
            0
        }
        DebugCommand::Lsp { cmd: lsp_cmd } => {
            match lsp_cmd {
                DebugLspCommand::Diagnostics { file } => {
                    tracing::info!("debug lsp diagnostics: file={}", file);
                    eprintln!("debug lsp diagnostics: not yet implemented.");
                }
                DebugLspCommand::Symbols { query } => {
                    tracing::info!("debug lsp symbols: query={}", query);
                    eprintln!("debug lsp symbols: not yet implemented.");
                }
                DebugLspCommand::DocumentSymbols { uri } => {
                    tracing::info!("debug lsp document-symbols: uri={}", uri);
                    eprintln!("debug lsp document-symbols: not yet implemented.");
                }
            }
            0
        }
        DebugCommand::Rg { cmd: rg_cmd } => {
            match rg_cmd {
                DebugRgCommand::Files {
                    query,
                    glob,
                    limit,
                } => {
                    tracing::info!(
                        "debug rg files: query={:?}, glob={:?}, limit={:?}",
                        query,
                        glob,
                        limit
                    );
                    eprintln!("debug rg files: not yet implemented.");
                }
                DebugRgCommand::Search {
                    pattern,
                    glob,
                    limit,
                } => {
                    tracing::info!(
                        "debug rg search: pattern={}, glob={:?}, limit={:?}",
                        pattern,
                        glob,
                        limit
                    );
                    eprintln!("debug rg search: not yet implemented.");
                }
            }
            0
        }
        DebugCommand::File { cmd: file_cmd } => {
            match file_cmd {
                DebugFileCommand::Search { query } => {
                    tracing::info!("debug file search: query={}", query);
                }
                DebugFileCommand::Read { path } => {
                    tracing::info!("debug file read: path={}", path);
                }
                DebugFileCommand::List { path } => {
                    tracing::info!("debug file list: path={}", path);
                }
            }
            eprintln!("debug file: not yet implemented.");
            0
        }
        DebugCommand::Scrap => {
            tracing::info!("debug scrap");
            eprintln!("debug scrap: not yet implemented.");
            0
        }
        DebugCommand::Skill => {
            tracing::info!("debug skill");
            eprintln!("debug skill: not yet implemented.");
            0
        }
        DebugCommand::Snapshot { cmd: snap_cmd } => {
            match snap_cmd {
                DebugSnapshotCommand::Track => {
                    tracing::info!("debug snapshot track");
                }
                DebugSnapshotCommand::Patch { hash } => {
                    tracing::info!("debug snapshot patch: hash={}", hash);
                }
                DebugSnapshotCommand::Diff { hash } => {
                    tracing::info!("debug snapshot diff: hash={}", hash);
                }
            }
            eprintln!("debug snapshot: not yet implemented.");
            0
        }
        DebugCommand::Startup => {
            // Ported from: `debug/startup.ts` — prints `performance.now()`
            println!("startup timing not yet measured");
            0
        }
        DebugCommand::Agent { name, tool, params } => {
            tracing::info!(
                "debug agent: name={}, tool={:?}, params={:?}",
                name,
                tool,
                params
            );
            eprintln!("debug agent: not yet implemented.");
            0
        }
        DebugCommand::V2 => {
            tracing::info!("debug v2");
            eprintln!("debug v2: not yet implemented.");
            0
        }
        DebugCommand::Info => {
            // Ported from: `debug/index.ts` — `debug info`
            println!("rustcode version: {}", env!("CARGO_PKG_VERSION"));
            println!(
                "os: {} {}",
                std::env::consts::OS,
                std::env::consts::ARCH
            );
            println!("plugins: not yet implemented");
            0
        }
        DebugCommand::Paths => {
            // Ported from: `debug/index.ts` — `debug paths`
            // TS: iterates over `Global.Path` entries (data, config, cache, state).
            // Uses the `dirs` crate directly since Config only exposes data_dir()
            // and global_config_dir().
            let data_dir = Config::data_dir().unwrap_or_else(|_| PathBuf::from("."));
            println!("data      {}", data_dir.display());
            let config_dir = dirs::config_dir()
                .unwrap_or_else(|| PathBuf::from("."))
                .join("opencode");
            println!("config    {}", config_dir.display());
            let cache_dir = dirs::cache_dir()
                .unwrap_or_else(|| PathBuf::from("."))
                .join("opencode");
            println!("cache     {}", cache_dir.display());
            let state_dir = dirs::state_dir()
                .or_else(|| dirs::data_dir())
                .unwrap_or_else(|| PathBuf::from("."))
                .join("opencode");
            println!("state     {}", state_dir.display());
            0
        }
        DebugCommand::Wait => {
            tracing::info!("debug wait: sleeping for 24 hours");
            // Ported from: `debug/index.ts` — `debug wait` sleeps for 1 day.
            eprintln!("debug wait: waiting indefinitely (press Ctrl+C to stop)");
            tokio::signal::ctrl_c().await.ok();
            0
        }
    }
}

/// `console` — Account management.
///
/// Ported from: `packages/opencode/src/cli/cmd/account.ts`
async fn cmd_console(cmd: &ConsoleCommand) -> i32 {
    match cmd {
        ConsoleCommand::Login { url } => {
            let console_url = url.as_deref().unwrap_or("https://console.opencode.ai");
            tracing::info!("console login: url={}", console_url);
            // TS: Opens browser, polls for device code authorization.
            eprintln!("console login: not yet implemented.");
            0
        }
        ConsoleCommand::Logout { email } => {
            tracing::info!("console logout: email={:?}", email);
            eprintln!("console logout: not yet implemented.");
            0
        }
        ConsoleCommand::Switch => {
            tracing::info!("console switch");
            eprintln!("console switch: not yet implemented.");
            0
        }
        ConsoleCommand::Orgs => {
            tracing::info!("console orgs");
            eprintln!("console orgs: not yet implemented.");
            0
        }
        ConsoleCommand::Open => {
            tracing::info!("console open");
            eprintln!("console open: not yet implemented.");
            0
        }
    }
}

/// `providers` — Manage AI provider credentials.
///
/// Ported from: `packages/opencode/src/cli/cmd/providers.ts`
async fn cmd_providers(cmd: &ProvidersCommand) -> i32 {
    match cmd {
        ProvidersCommand::List => {
            tracing::info!("providers list");
            eprintln!("providers list: not yet implemented.");
            0
        }
        ProvidersCommand::Login {
            url,
            provider,
            method,
        } => {
            tracing::info!(
                "providers login: url={:?}, provider={:?}, method={:?}",
                url,
                provider,
                method
            );
            eprintln!("providers login: not yet implemented.");
            0
        }
        ProvidersCommand::Logout { provider } => {
            tracing::info!("providers logout: provider={:?}", provider);
            eprintln!("providers logout: not yet implemented.");
            0
        }
    }
}

/// `agent` — Manage agents.
///
/// Ported from: `packages/opencode/src/cli/cmd/agent.ts`
async fn cmd_agent(cmd: &AgentCommand) -> i32 {
    match cmd {
        AgentCommand::Create {
            path,
            description,
            mode,
            permissions,
            model,
        } => {
            tracing::info!(
                "agent create: path={:?}, description={:?}, mode={:?}, permissions={:?}, model={:?}",
                path,
                description,
                mode,
                permissions,
                model,
            );
            eprintln!("agent create: not yet implemented.");
            0
        }
        AgentCommand::List => {
            tracing::info!("agent list");
            eprintln!("agent list: not yet implemented.");
            0
        }
    }
}

/// `upgrade` — Upgrade OpenCode.
///
/// Ported from: `packages/opencode/src/cli/cmd/upgrade.ts`
async fn cmd_upgrade(args: &UpgradeArgs) -> i32 {
    let target = args.target.as_deref().unwrap_or("latest");
    let method = args.method.as_deref().unwrap_or("auto");
    tracing::info!("upgrade: target={}, method={}", target, method);

    eprintln!("upgrade: self-upgrade not yet implemented.");
    0
}

/// `uninstall` — Uninstall OpenCode.
///
/// Ported from: `packages/opencode/src/cli/cmd/uninstall.ts`
async fn cmd_uninstall(args: &UninstallArgs) -> i32 {
    tracing::info!(
        "uninstall: keep_config={}, keep_data={}, dry_run={}, force={}",
        args.keep_config,
        args.keep_data,
        args.dry_run,
        args.force,
    );

    if args.dry_run {
        eprintln!("Dry run — no changes made.");
        return 0;
    }

    eprintln!("uninstall: self-uninstall not yet implemented.");
    0
}

/// `version` — Show version.
///
/// Ported from: `packages/opencode/src/index.ts` — `.version("version", ...)`
fn cmd_version() {
    println!("rustcode {}", env!("CARGO_PKG_VERSION"));
    println!("Port of OpenCode (TypeScript/Bun) to Rust");
}

/// `github` — Manage GitHub agent.
///
/// Ported from: `packages/opencode/src/cli/cmd/github.ts`
async fn cmd_github(cmd: &GithubCommand) -> i32 {
    match cmd {
        GithubCommand::Install => {
            tracing::info!("github install");
            eprintln!("github install: not yet implemented.");
            0
        }
        GithubCommand::Run { event, token } => {
            tracing::info!(
                "github run: event={:?}, token_set={}",
                event,
                token.is_some()
            );
            eprintln!("github run: not yet implemented.");
            0
        }
    }
}

/// `pr` — Fetch and checkout a GitHub PR branch.
///
/// Ported from: `packages/opencode/src/cli/cmd/pr.ts`
async fn cmd_pr(args: &PrArgs) -> i32 {
    tracing::info!("pr: number={}", args.number);

    // TS: Uses `gh pr checkout` via Process.run.
    eprintln!("pr: gh CLI integration not yet implemented.");
    0
}

/// `plugin` — Install plugin and update config.
///
/// Ported from: `packages/opencode/src/cli/cmd/plug.ts`
async fn cmd_plugin(args: &PluginArgs) -> i32 {
    let module = args.module.trim();
    if module.is_empty() {
        eprintln!("Error: module is required");
        return 1;
    }

    tracing::info!(
        "plugin: module={}, global={}, force={}",
        module,
        args.global,
        args.force,
    );

    eprintln!("plugin: plugin installation not yet implemented.");
    0
}

/// `db` — Database tools.
///
/// Ported from: `packages/opencode/src/cli/cmd/db.ts`
async fn cmd_db(args: &DbArgs) -> i32 {
    if let Some(query) = &args.query {
        tracing::info!("db query: query={}, format={}", query, args.format);
        // TODO: Execute SQL query against the project database.
        eprintln!("db query: not yet implemented.");
    } else {
        // TS: Spawns `sqlite3 <db_path>` as an interactive shell.
        tracing::info!("db: opening interactive shell");
        eprintln!("db interactive shell: not yet implemented.");
    }
    0
}

/// `acp` — Start ACP (Agent Client Protocol) server.
///
/// Ported from: `packages/opencode/src/cli/cmd/acp.ts`
async fn cmd_acp(args: &AcpArgs) -> i32 {
    let cwd = args.cwd.clone().unwrap_or_else(|| {
        std::env::current_dir()
            .map(|p| p.display().to_string())
            .unwrap_or_else(|_| ".".into())
    });

    tracing::info!(
        "acp: hostname={}, port={}, mdns={}, cwd={}",
        args.network.hostname,
        args.network.port,
        args.network.mdns,
        cwd,
    );

    // TS: Sets OPENCODE_CLIENT="acp", starts server, creates ndJsonStream
    // over stdin/stdout, creates AgentSideConnection.
    eprintln!("acp: ACP server not yet implemented.");
    0
}

/// `mcp` — Manage MCP servers.
///
/// Ported from: `packages/opencode/src/cli/cmd/mcp.ts`
async fn cmd_mcp(cmd: &McpCommand) -> i32 {
    match cmd {
        McpCommand::Add {
            name,
            url,
            env,
            header,
        } => {
            tracing::info!(
                "mcp add: name={:?}, url={:?}, env_count={}, header_count={}",
                name,
                url,
                env.len(),
                header.len(),
            );
            eprintln!("mcp add: not yet implemented.");
            0
        }
        McpCommand::List => {
            tracing::info!("mcp list");
            eprintln!("mcp list: not yet implemented.");
            0
        }
        McpCommand::Auth { name } => {
            tracing::info!("mcp auth: name={:?}", name);
            eprintln!("mcp auth: not yet implemented.");
            0
        }
        McpCommand::Logout { name } => {
            tracing::info!("mcp logout: name={:?}", name);
            eprintln!("mcp logout: not yet implemented.");
            0
        }
        McpCommand::Debug { name } => {
            tracing::info!("mcp debug: name={}", name);
            eprintln!("mcp debug: not yet implemented.");
            0
        }
    }
}
