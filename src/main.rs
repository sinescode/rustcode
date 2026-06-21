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
use futures::StreamExt;
use rustcode_core::config::Config;
use std::collections::HashMap;
use std::io::{IsTerminal, Write as _};
use std::path::{Path, PathBuf};
use std::sync::Arc;

mod cli_error;
use cli_error::CliErrorFormatter;

use sqlx::Column;
#[allow(unused_imports)]
use sqlx::Row as _;
use sqlx::TypeInfo;

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

    /// Generate shell completion scripts.
    ///
    /// Ported from: `packages/opencode/src/index.ts` — completion subcommand
    /// (not in TS, but a common CLI convention).
    #[command(name = "completion")]
    Completion(CompletionArgs),
}

/// Arguments for shell completion generation.
///
/// Ported from: standard `clap_complete` convention.
#[derive(clap::Args)]
struct CompletionArgs {
    /// Shell type (bash, fish, zsh, powershell).
    #[arg(value_parser = clap::builder::PossibleValuesParser::new(["bash", "fish", "zsh", "powershell"]))]
    shell: String,
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

    /// Output structured JSON events on stdout (one per line).
    ///
    /// Useful for CI/CD pipelines and scripting. Events include:
    /// session created, message sent, tool called, response received.
    #[arg(long)]
    json: bool,
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
        /// GitHub mock event type to run the agent for
        /// (e.g. `issues`, `pull_request`, `issue_comment`,
        /// `pull_request_review_comment`, `schedule`, `workflow_dispatch`).
        ///
        /// Ported from: `github.ts` — `--event` string
        #[arg(long)]
        event: Option<String>,

        /// Path to a JSON file containing the GitHub event payload.
        /// If not provided, the payload is read from stdin.
        ///
        /// When running in CI (GitHub Actions), the event payload is
        /// available at `$GITHUB_EVENT_PATH`.
        #[arg(long = "event-payload")]
        event_payload: Option<String>,

        /// GitHub personal access token (github_pat_********).
        /// Falls back to the `GITHUB_TOKEN` environment variable.
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

    // Set environment variables for observability (matching opencode middleware).
    //
    // Ported from: `packages/opencode/src/index.ts` lines 66-68 — middleware sets
    // OPENCODE_PRINT_LOGS and OPENCODE_LOG_LEVEL env vars before the observability
    // layer reads them, so it sees the CLI overrides.
    if cli.print_logs {
        std::env::set_var("OPENCODE_PRINT_LOGS", "1");
    }
    std::env::set_var("OPENCODE_LOG_LEVEL", cli.log_level.to_string());

    // Initialize the observability subsystem.
    //
    // Ported from: `packages/core/src/observability.ts` — the `layer` composition
    // that sets up file logging, optional stderr output, and OTLP export.
    let mut observability = rustcode_core::observability::ObservabilityService::new();
    match observability.init() {
        Ok(true) => {
            tracing::info!(
                "rustcode starting (version={}, pure={}, print_logs={}, log_level={})",
                env!("CARGO_PKG_VERSION"),
                cli.pure,
                cli.print_logs,
                cli.log_level,
            );
        }
        Ok(false) => {
            // Already initialized — fall through
        }
        Err(e) => {
            eprintln!("{}Warning: failed to initialize observability: {e}{}", cli_error::TEXT_WARNING, cli_error::TEXT_RESET);
        }
    }

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
    // Create the CLI error formatter for styled error output.
    //
    // Ported from: `packages/opencode/src/cli/error.ts` — `FormatError()`.
    let mut error_fmt = CliErrorFormatter::new();

    // Load config eagerly (matches TS middleware that sets env vars).
    //
    // Ported from: `packages/opencode/src/index.ts` — middleware sets
    // AGENT=1, OPENCODE=1, OPENCODE_PID.
    let config = Config::load().unwrap_or_default();
    tracing::debug!("Config loaded ({} providers)", config.provider.len());

    let print_logs = cli.print_logs;
    let exit_code = match &cli.command {
        Some(cmd) => dispatch(cmd, print_logs, &config, &mut error_fmt).await,
        None => {
            // No subcommand given — show help.
            // Ported from: TS — when no subcommand is matched, yargs shows help.
            cli_error::format_cli_error("Use --help for usage information.");
            1
        }
    };

    error_fmt.has_errors |= exit_code != 0;
    if exit_code != 0 {
        std::process::exit(exit_code);
    }
}

/// Dispatch to the appropriate subcommand handler.
///
/// Each handler returns an exit code (0 = success, non-zero = failure).
/// Errors from handlers are captured and formatted through the
/// [`CliErrorFormatter`].
///
/// Ported from: `packages/opencode/src/index.ts` — `try { await cli.parse() }
/// catch (e) { FormatError(e) }`.
async fn dispatch(
    cmd: &Commands,
    print_logs: bool,
    config: &rustcode_core::config::Info,
    error_fmt: &mut CliErrorFormatter,
) -> i32 {
    let result = dispatch_inner(cmd, print_logs, config).await;
    if result != 0 {
        error_fmt.has_errors = true;
    }
    result
}

/// Inner dispatch without error formatting — called by [`dispatch`].
async fn dispatch_inner(
    cmd: &Commands,
    print_logs: bool,
    config: &rustcode_core::config::Info,
) -> i32 {
    match cmd {
        Commands::Acp(args) => cmd_acp(args, config).await,
        Commands::Mcp { cmd: mcp_cmd } => cmd_mcp(mcp_cmd).await,
        Commands::Tui(args) => cmd_tui(args, print_logs, config).await,
        Commands::Attach(args) => cmd_attach(args).await,
        Commands::Run(args) => cmd_run(args, config).await,
        Commands::Generate => cmd_generate().await,
        Commands::Debug { cmd: debug_cmd } => cmd_debug(debug_cmd).await,
        Commands::Console { cmd: console_cmd } => cmd_console(console_cmd).await,
        Commands::Providers { cmd: providers_cmd } => cmd_providers(providers_cmd).await,
        Commands::Agent { cmd: agent_cmd } => cmd_agent(agent_cmd).await,
        Commands::Upgrade(args) => cmd_upgrade(args).await,
        Commands::Uninstall(args) => cmd_uninstall(args).await,
        Commands::Serve(args) => cmd_serve(args, config).await,
        Commands::Web(args) => cmd_web(args, config).await,
        Commands::Models(args) => cmd_models(args, config).await,
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
        Commands::Completion(args) => cmd_completion(args),
    }
}

// ═════════════════════════════════════════════════════════════════════════════
// command handlers — each returns an exit code
// ═════════════════════════════════════════════════════════════════════════════

// ── helpers ──────────────────────────────────────────────────────────────────

/// Parse "provider/model" string into (provider_id, model_id).
fn parse_model_spec(spec: &str) -> Option<(&str, &str)> {
    let (provider, model) = spec.split_once('/')?;

    if provider.is_empty() || model.is_empty() {
        return None;
    }
    Some((provider, model))
}

/// Check if a binary executable is on PATH (like `which`).
fn has_binary(name: &str) -> bool {
    std::process::Command::new(name)
        .arg("--version")
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .status()
        .map(|s| s.success())
        .unwrap_or(false)
}

/// Print a header with optional styling for terminal output.
fn print_header(title: &str) {
    let width = 56;
    let bar = "\u{2500}".repeat(width);
    println!("\u{250c}{}\u{2510}", bar);
    let _pad = (width.saturating_sub(title.len())) / 2;
    println!("\u{2502}{:^width$}\u{2502}", title, width = width);
    println!("\u{2514}{}\u{2518}", bar);
}

/// Format a number with K/M suffix for readability.
fn format_count(num: u64) -> String {
    if num >= 1_000_000 {
        format!("{:.1}M", num as f64 / 1_000_000.0)
    } else if num >= 1_000 {
        format!("{:.1}K", num as f64 / 1_000.0)
    } else {
        num.to_string()
    }
}

/// Format bytes as human-readable size.
fn format_size(bytes: u64) -> String {
    if bytes < 1024 {
        format!("{} B", bytes)
    } else if bytes < 1024 * 1024 {
        format!("{:.1} KB", bytes as f64 / 1024.0)
    } else if bytes < 1024 * 1024 * 1024 {
        format!("{:.1} MB", bytes as f64 / (1024.0 * 1024.0))
    } else {
        format!("{:.1} GB", bytes as f64 / (1024.0 * 1024.0 * 1024.0))
    }
}

/// Shorten a path by replacing the home directory with ~.
fn shorten_path(p: &Path) -> String {
    let s = p.display().to_string();
    if let Ok(home) = std::env::var("HOME") {
        if s.starts_with(&home) {
            return s.replacen(&home, "~", 1);
        }
    }
    s
}

/// Get the (elapsed since startup) in milliseconds for startup timing.
fn elapsed_ms() -> f64 {
    use std::time::Instant;
    static START: std::sync::OnceLock<Instant> = std::sync::OnceLock::new();
    let start = START.get_or_init(Instant::now);
    start.elapsed().as_secs_f64() * 1000.0
}

/// Try to run `gh` CLI and return stdout + stderr + exit status.
async fn run_gh(args: &[&str]) -> std::io::Result<std::process::Output> {
    tokio::process::Command::new("gh").args(args).output().await
}

// ═════════════════════════════════════════════════════════════════════════════
// run
// ═════════════════════════════════════════════════════════════════════════════

/// `run` — Run OpenCode with a message.
///
/// Ported from: `packages/opencode/src/cli/cmd/run.ts`
///
/// Supports two modes:
/// - **Local**: resolves providers/models locally and runs the agentic loop.
/// - **SSE attach** (`--attach <url>`): connects to a remote server via SSE,
///   sends the prompt via HTTP POST, and streams results back.
async fn cmd_run(args: &RunArgs, config: &rustcode_core::config::Info) -> i32 {
    let msg = args.message.join(" ");

    // ── validation ──────────────────────────────────────────────────
    if msg.is_empty() && args.command.is_none() && !args.interactive {
        cli_error::format_cli_error("You must provide a message or a command");
        return 1;
    }
    if args.interactive && args.command.is_some() {
        cli_error::format_cli_error("--interactive cannot be used with --command");
        return 1;
    }
    if args.demo && !args.interactive {
        cli_error::format_cli_error("--demo requires --interactive");
        return 1;
    }
    if args.interactive && args.format == "json" {
        cli_error::format_cli_error("--interactive cannot be used with --format json");
        return 1;
    }
    if args.replay_limit.is_some() && !args.interactive {
        cli_error::format_cli_error("--replay-limit requires --interactive");
        return 1;
    }
    if let Some(limit) = args.replay_limit {
        if limit == 0 {
            cli_error::format_cli_error("--replay-limit must be a positive integer");
            return 1;
        }
    }
    if args.fork && !args.r#continue && args.session.is_none() {
        cli_error::format_cli_error("--fork requires --continue or --session");
        return 1;
    }

    // ── SSE attach mode: connect to a remote server ────────────────
    if let Some(attach_url) = &args.attach {
        return cmd_run_attach(args, attach_url, &msg).await;
    }

    // ── resolve model spec ──────────────────────────────────────────
    let (provider_filter, model_filter) = args.model.as_deref().and_then(parse_model_spec).unzip();

    // ── auto-detect providers via shared runtime ────────────────────
    let ctx = match rustcode_core::runtime::initialize_runtime(config) {
        Ok(c) => c,
        Err(e) => {
            cli_error::format_provider_init_error("runtime", &e.to_string());
            return 1;
        }
    };
    let providers = ctx.providers;

    if providers.is_empty() {
        cli_error::format_cli_error(
            "No LLM providers detected. Set an API key environment variable.",
        );
        eprintln!("{}  ANTHROPIC_API_KEY              — Claude (Anthropic){}", cli_error::TEXT_DIM, cli_error::TEXT_RESET);
        eprintln!("{}  OPENAI_API_KEY                 — GPT (OpenAI){}", cli_error::TEXT_DIM, cli_error::TEXT_RESET);
        eprintln!("{}  GOOGLE_GENERATIVE_AI_API_KEY   — Gemini (Google){}", cli_error::TEXT_DIM, cli_error::TEXT_RESET);
        eprintln!("{}  OPENROUTER_API_KEY             — OpenRouter (multi-provider){}", cli_error::TEXT_DIM, cli_error::TEXT_RESET);
        eprintln!("{}  DEEPSEEK_API_KEY               — DeepSeek{}", cli_error::TEXT_DIM, cli_error::TEXT_RESET);
        eprintln!("{}  GROQ_API_KEY                   — Groq{}", cli_error::TEXT_DIM, cli_error::TEXT_RESET);
        eprintln!("{}  ...and more (see docs for full list){}", cli_error::TEXT_DIM, cli_error::TEXT_RESET);
        return 1;
    }

    // ── pick provider ───────────────────────────────────────────────
    let provider_entry = if let Some(pid) = provider_filter {
        match providers.get(pid) {
            Some(p) => (pid.to_string(), Arc::clone(p)),
            None => {
                cli_error::format_provider_model_not_found("", pid);
                return 1;
            }
        }
    } else {
        let id = providers.keys().next().unwrap().clone();
        let p = Arc::clone(providers.get(&id).unwrap());
        (id, p)
    };

    let provider_id = &provider_entry.0;
    let provider = provider_entry.1;

    // ── get models ──────────────────────────────────────────────────
    let models = match provider.list_models().await {
        Ok(m) => m,
        Err(e) => {
            cli_error::format_provider_init_error(provider_id, &e.to_string());
            return 1;
        }
    };

    if models.is_empty() {
        cli_error::format_provider_model_not_found("", provider_id);
        return 1;
    }

    let model = if let Some(mf) = model_filter {
        match models.iter().find(|m| m.id == mf) {
            Some(m) => m,
            None => {
                cli_error::format_provider_model_not_found(mf, provider_id);
                return 1;
            }
        }
    } else {
        &models[0]
    };

    let agent = args.agent.as_deref().unwrap_or("primary");

    // ── build prompt input ─────────────────────────────────────────
    let user_content = if msg.is_empty() && args.command.is_some() {
        format!(
            "Run command: /{}",
            args.command.as_deref().unwrap_or("help")
        )
    } else {
        msg.clone()
    };

    if user_content.is_empty() && !args.interactive {
        eprintln!("Error: No message content to send");
        return 1;
    }

    let cwd = std::env::current_dir()
        .map(|p| p.display().to_string())
        .unwrap_or_else(|_| ".".into());

    // Build instructions (system prompt content)
    let instructions = vec![
        format!(
            "You are a helpful AI coding assistant. You have access to tools for reading, \
             writing, and editing files, searching code, running shell commands, and fetching URLs. \
             You work in the directory: {cwd}. \
             Be concise and direct in your responses. When you need to interact with files, \
             use tools rather than describing what you would do."
        ),
    ];

    use rustcode_core::provider::{ChatMessage, MessageContent};
    use rustcode_core::session_prompt::{PromptPart, PromptTextPart, SessionPromptInput};

    let session_id = format!("local-{}", std::process::id());
    let runner = &ctx.runner;

    // ── Interactive REPL mode ──────────────────────────────────────
    if args.interactive {
        if !std::io::stdin().is_terminal() {
            eprintln!("Error: --interactive requires a TTY for input");
            eprintln!("Tip: use `rustcode run \"message\"` for non-interactive mode");
            return 1;
        }

        let variant_display = args.variant.as_deref().unwrap_or("default");

        // Print header with agent, provider/model info
        if args.format != "json" {
            let variant_suffix = if args.variant.is_some() {
                format!(" [variant: {}]", variant_display)
            } else {
                String::new()
            };
            println!(
                "> {agent} \u{b7} {provider_id}/{id}{variant_suffix}",
                id = model.id
            );
            if args.demo {
                println!("Demo mode: /help for available slash commands.");
            }
            println!("Entering interactive mode. Type your messages, /exit to quit.");
            println!();
        }

        // ── File resolution (--file) ──────────────────────────────
        let attached_files: Vec<String> = if args.file.is_empty() {
            Vec::new()
        } else {
            let mut files = Vec::new();
            for f in &args.file {
                let path = std::path::Path::new(f);
                if path.exists() {
                    match std::fs::read_to_string(path) {
                        Ok(content) => {
                            let filename = path.file_name()
                                .map(|n| n.to_string_lossy())
                                .unwrap_or_else(|| std::borrow::Cow::Borrowed("unknown"));
                            files.push(format!("<file name=\"{filename}\">\n{content}\n</file>"));
                            if args.format != "json" {
                                println!("(attached: {filename})");
                            }
                        }
                        Err(e) => {
                            eprintln!("Warning: could not read {f}: {e}");
                        }
                    }
                } else {
                    eprintln!("Warning: file not found: {f}");
                }
            }
            files
        };

        // ── Session management (continue / fork) ──────────────────
        let session_id = if args.r#continue || args.session.is_some() {
            args.session.clone().unwrap_or_else(|| "last-session".to_string())
        } else {
            format!("interactive-{}", std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap_or_default()
                .as_nanos())
        };
        let session_label = if args.r#continue { "continued" } else if args.session.is_some() { "resumed" } else { "new" };

        if args.format != "json" {
            println!("Session {session_label}: {session_id}");
            println!();
        }

        // Build initial message list: system prompt
        let system_prompt = runner.build_system_prompt(&instructions);
        let mut messages: Vec<ChatMessage> = Vec::new();
        if !system_prompt.is_empty() {
            messages.push(ChatMessage::System {
                content: MessageContent::Text(system_prompt),
            });
        }

        // ── Demo Mode ─────────────────────────────────────────────
        // Run a demo prompt on start if no user message was given
        if args.demo && user_content.is_empty() {
            let demo_prompt = format!(
                "Welcome to rustcode interactive demo! I'm running in {} mode. \
                 You are working in directory: {}. \
                 Available slash commands: /exit, /help, /clear, /model, /tokens.",
                agent, cwd
            );
            messages.push(ChatMessage::User {
                content: MessageContent::Text(demo_prompt),
            });
            match runner
                .run_with_messages(provider.as_ref(), model, &mut messages)
                .await
            {
                Ok(result) => {
                    if !result.text.is_empty() {
                        print!("{}", result.text);
                        let _ = std::io::stdout().flush();
                        println!();
                    }
                }
                Err(e) => {
                    eprintln!("LLM error: {e}");
                }
            }
        }

        // If an initial message was provided, send it first
        if !user_content.is_empty() {
            // Build the message with attached files
            let final_content = if attached_files.is_empty() {
                user_content.clone()
            } else {
                format!("{}\n\n{}", attached_files.join("\n\n"), user_content)
            };
            messages.push(ChatMessage::User {
                content: MessageContent::Text(final_content),
            });
            match runner
                .run_with_messages(provider.as_ref(), model, &mut messages)
                .await
            {
                Ok(result) => {
                    if !result.text.is_empty() {
                        print!("{}", result.text);
                        let _ = std::io::stdout().flush();
                        println!();
                    }
                    if let Some(ref err) = result.error {
                        if !result.success {
                            eprintln!("Session aborted: {err}");
                        }
                    }
                }
                Err(e) => {
                    eprintln!("LLM error: {e}");
                }
            }
        } else if !attached_files.is_empty() {
            // If only files attached with no message, send them
            let final_content = attached_files.join("\n\n");
            messages.push(ChatMessage::User {
                content: MessageContent::Text(final_content),
            });
            match runner
                .run_with_messages(provider.as_ref(), model, &mut messages)
                .await
            {
                Ok(result) => {
                    if !result.text.is_empty() {
                        print!("{}", result.text);
                        let _ = std::io::stdout().flush();
                        println!();
                    }
                }
                Err(e) => {
                    eprintln!("LLM error: {e}");
                }
            }
        }

        // REPL loop
        loop {
            use std::io::{BufRead, Write};
            print!("> ");
            let _ = std::io::stdout().flush();

            let mut line = String::new();
            match std::io::stdin().lock().read_line(&mut line) {
                Ok(0) => {
                    println!();
                    break;
                }
                Ok(_) => {}
                Err(e) => {
                    eprintln!("Error reading input: {e}");
                    break;
                }
            }

            let line = line.trim().to_string();
            if line.is_empty() {
                continue;
            }

            // ── Slash commands ────────────────────────────────────
            let (should_break, should_continue) = handle_slash_command(&line, &mut messages, args);
            if should_break {
                break;
            }
            if should_continue {
                continue;
            }

            messages.push(ChatMessage::User {
                content: MessageContent::Text(line),
            });

            match runner
                .run_with_messages(provider.as_ref(), model, &mut messages)
                .await
            {
                Ok(result) => {
                    if !result.text.is_empty() {
                        print!("{}", result.text);
                        let _ = std::io::stdout().flush();
                        println!();
                    }
                    if !result.tool_calls.is_empty() {
                        let ok = result.tool_calls.iter().filter(|t| t.success).count();
                        let fail = result.tool_calls.len() - ok;
                        println!(
                            "\u{2500}\u{2500}\u{2500} {} tool call(s) ({} ok, {} failed) in {} iteration(s)",
                            result.tool_calls.len(), ok, fail, result.iterations
                        );
                    }
                    if let Some(ref err) = result.error {
                        if !result.success {
                            eprintln!("Session aborted: {err}");
                        }
                    }
                }
                Err(e) => {
                    eprintln!("LLM error: {e}");
                }
            }
        }

        return 0;
    }

    // ── Non-interactive mode ──────────────────────────────────────

    // Build the session prompt input
    let input = SessionPromptInput {
        session_id: session_id.clone(),
        message_id: None,
        model: Some(rustcode_core::session_info::ModelRef {
            id: model.id.clone(),
            provider_id: provider_id.clone(),
            variant: None,
        }),
        agent: Some(agent.to_string()),
        no_reply: false,
        tools: None,
        format: None,
        system: None,
        variant: None,
        parts: vec![PromptPart::Text(PromptTextPart {
            id: None,
            text: user_content.clone(),
            synthetic: false,
        })],
    };

    // ── print header ────────────────────────────────────────────────
    if args.format == "json" {
        println!(
            r#"{{"type":"start","timestamp":{},"sessionID":"{}","provider":"{}","model":"{}"}}"#,
            chrono::Utc::now().timestamp_millis(),
            session_id,
            provider_id,
            model.id,
        );
    } else {
        println!("> {agent} \u{b7} {provider_id}/{id}", id = model.id);
        println!();
    }

    // ── run the agentic loop ──────────────────────────────────────
    match runner
        .run(provider.as_ref(), model, &input, &instructions)
        .await
    {
        Ok(result) => {
            if !result.text.is_empty() {
                print!("{}", result.text);
                let _ = std::io::stdout().flush();
                if args.format != "json" {
                    println!();
                }
            }

            if !result.tool_calls.is_empty() && args.format != "json" {
                let success_count = result.tool_calls.iter().filter(|t| t.success).count();
                let fail_count = result.tool_calls.len() - success_count;
                println!(
                    "\n\u{2500}\u{2500}\u{2500} {} tool calls ({}) in {} iterations",
                    result.tool_calls.len(),
                    if fail_count > 0 {
                        format!("{} ok, {} failed", success_count, fail_count)
                    } else {
                        "all ok".to_string()
                    },
                    result.iterations
                );
            }

            if args.format == "json" {
                println!(
                    r#"{{"type":"done","finish_reason":"stop","iterations":{},"tool_calls":{}}}"#,
                    result.iterations,
                    result.tool_calls.len(),
                );
            }

            if let Some(ref err) = result.error {
                if !result.success {
                    eprintln!("Session aborted: {err}");
                    return 1;
                }
            }
        }
        Err(e) => {
            eprintln!("LLM error: {e}");
            return 1;
        }
    }

    0
}

/// Handle a slash command in interactive mode.
///
/// Returns (should_break, should_continue).
/// Ported from: `packages/opencode/src/cli/cmd/run.ts` — interactive slash commands.
fn handle_slash_command(
    line: &str,
    messages: &mut Vec<ChatMessage>,
    _args: &RunArgs,
) -> (bool, bool) {
    use rustcode_core::provider::MessageContent;
    let trimmed = line.trim().to_lowercase();
    match trimmed.as_str() {
        "/exit" | "/quit" | "exit" | "quit" | "/q" => return (true, false),
        "/clear" | "/reset" => {
            // Keep only the system prompt
            messages.retain(|m| matches!(m, ChatMessage::System { .. }));
            println!("(session context cleared)");
            return (false, true);
        }
        "/help" | "/?" => {
            println!("Available slash commands:");
            println!("  /exit, /quit, /q  — Exit interactive mode");
            println!("  /clear, /reset    — Clear conversation context");
            println!("  /help, /?         — Show this help message");
            println!("  /tokens           — Show approximate token count");
            return (false, true);
        }
        "/tokens" | "/stats" => {
            let total_chars: usize = messages
                .iter()
                .map(|m| match m {
                    ChatMessage::System { content }
                    | ChatMessage::User { content }
                    | ChatMessage::Assistant { content } => match content {
                        MessageContent::Text(t) => t.len(),
                        MessageContent::Parts(parts) => {
                            parts.iter().map(|p| serde_json::to_string(p).unwrap_or_default().len()).sum()
                        }
                    },
                    ChatMessage::Tool { content } => {
                        content.iter().map(|p| serde_json::to_string(p).unwrap_or_default().len()).sum()
                    }
                })
                .sum();
            println!("Messages: {} | Approx chars: {}", messages.len(), total_chars);
            return (false, true);
        }
        _ => {}
    }
    (false, false)
}

/// Run a prompt against a remote rustcode server via SSE + HTTP POST.
///
/// This is the attach-mode branch of [`cmd_run`]. It connects to the
/// server's SSE endpoint (`GET /event`), sends the prompt via HTTP POST to
/// `POST /session/{sessionID}/message`, and streams `session.next.*` events
/// (text deltas, tool calls, errors) back to stdout in real time.
///
/// ## Flow
///
/// 1. Build auth headers from `--username`/`--password` or env vars.
/// 2. Health-check the server.
/// 3. Create a new session via `POST /session`.
/// 4. Open an SSE stream at `GET /event`.
/// 5. Send the prompt via `POST /session/{sessionID}/message`.
/// 6. Stream and dispatch SSE events filtered by `sessionID`:
///    - `session.next.text.delta` — print delta to stdout immediately.
///    - `session.next.tool.called` — track tool name.
///    - `session.next.tool.success` / `.tool.failed` — print result.
///    - `session.next.step.ended` — print summary and stop.
///    - `session.next.step.failed` — print error and stop.
/// 7. On Ctrl+C, `POST /session/{sessionID}/abort` to interrupt.
/// 8. Print final summary (tool count, success/fail) in non-JSON mode.
///
/// # Source
/// Ported from: `packages/opencode/src/cli/cmd/run.ts` — `--attach` branch.
async fn cmd_run_attach(args: &RunArgs, attach_url: &str, msg: &str) -> i32 {
    let url = attach_url.trim_end_matches('/');

    // ── Build auth headers ─────────────────────────────────────────
    let mut headers = reqwest::header::HeaderMap::new();
    let username = args
        .username
        .clone()
        .or_else(|| std::env::var("OPENCODE_SERVER_USERNAME").ok())
        .unwrap_or_else(|| "opencode".to_string());
    let password = args
        .password
        .clone()
        .or_else(|| std::env::var("OPENCODE_SERVER_PASSWORD").ok());

    if let Some(pw) = &password {
        let auth = format!("{username}:{pw}");
        let encoded = base64_encode(&auth);
        if let Ok(hv) = reqwest::header::HeaderValue::from_str(&format!("Basic {}", encoded)) {
            headers.insert(reqwest::header::AUTHORIZATION, hv);
        }
    }

    let client = reqwest::Client::builder()
        .default_headers(headers.clone())
        .build()
        .expect("Failed to build HTTP client");

    // ── Health check ───────────────────────────────────────────────
    let health_url = format!("{url}/api/health");
    match client.get(&health_url).send().await {
        Ok(resp) if resp.status().is_success() => {
            tracing::info!("Connected to server at {url}");
        }
        Ok(resp) => {
            eprintln!(
                "Server responded with HTTP {} at {health_url}",
                resp.status()
            );
            return 1;
        }
        Err(e) => {
            eprintln!("Could not connect to server at {health_url}: {e}");
            return 1;
        }
    }

    // ── Create or reuse a session on the server ────────────────────
    let agent = args.agent.as_deref().unwrap_or("primary");
    let cwd = args
        .dir
        .clone()
        .or_else(|| {
            std::env::current_dir()
                .ok()
                .map(|p| p.display().to_string())
        })
        .unwrap_or_else(|| ".".into());

    // If --session is provided, reuse the existing remote session ID.
    // Otherwise, create a new session via POST /session.
    let session_id: String = if let Some(ref sid) = args.session {
        // Validate that the session exists on the remote server
        let get_url = format!("{url}/session/{sid}");
        match client.get(&get_url).send().await {
            Ok(resp) if resp.status().is_success() => {
                tracing::info!(%sid, "Reusing existing remote session");
                sid.clone()
            }
            Ok(resp) => {
                eprintln!(
                    "Session '{sid}' not found on remote server (HTTP {}).",
                    resp.status()
                );
                return 1;
            }
            Err(e) => {
                eprintln!("Failed to verify session '{sid}' on remote server: {e}");
                return 1;
            }
        }
    } else {
        let create_body = serde_json::json!({
            "directory": cwd,
            "agent": agent,
            "model": {
                "id": args.model.as_deref().unwrap_or(""),
                "provider_id": "",
            },
        });

        let create_url = format!("{url}/session");
        match client.post(&create_url).json(&create_body).send().await {
            Ok(resp) if resp.status().is_success() => {
                match resp.json::<serde_json::Value>().await {
                    Ok(json) => json
                        .get("id")
                        .and_then(|v| v.as_str())
                        .map(String::from)
                        .unwrap_or_else(|| format!("remote-{}", std::process::id())),
                    Err(e) => {
                        eprintln!("Failed to parse session create response: {e}");
                        return 1;
                    }
                }
            }
            Ok(resp) => {
                eprintln!("Server rejected session creation: HTTP {}", resp.status());
                eprintln!("Body: {}", resp.text().await.unwrap_or_default());
                return 1;
            }
            Err(e) => {
                eprintln!("Failed to create session: {e}");
                return 1;
            }
        }
    };

    tracing::info!(%session_id, "Created remote session");

    // ── Print start header (before SSE streaming begins) ───────────
    if args.format == "json" {
        println!(
            r#"{{"type":"start","timestamp":{},"sessionID":"{}","url":"{}","agent":"{}"}}"#,
            chrono::Utc::now().timestamp_millis(),
            session_id,
            url,
            agent,
        );
    } else {
        println!("> {agent} \u{b7} remote @ {url}");
        println!();
    }

    // ── Open SSE stream ────────────────────────────────────────────
    // We MUST subscribe to the SSE bus BEFORE sending the prompt so
    // that we receive all session.next.* events from the start.
    let event_url = format!("{url}/event");
    let sse_response = match client
        .get(&event_url)
        .header("Accept", "text/event-stream")
        .header("Cache-Control", "no-cache")
        .send()
        .await
    {
        Ok(resp) if resp.status().is_success() => resp,
        Ok(resp) => {
            eprintln!("SSE connection failed: HTTP {}", resp.status());
            return 1;
        }
        Err(e) => {
            eprintln!("Failed to connect to SSE endpoint at {event_url}: {e}");
            return 1;
        }
    };

    // ── Send prompt via POST (spawned to run concurrently with SSE) ─
    let user_content = if msg.is_empty() && args.command.is_some() {
        format!(
            "Run command: /{}",
            args.command.as_deref().unwrap_or("help")
        )
    } else {
        msg.to_string()
    };

    let prompt_url = format!("{url}/session/{session_id}/message");
    let prompt_body = serde_json::json!({
        "text": user_content,
        "agent": agent,
        "model": {
            "id": args.model.as_deref().unwrap_or(""),
            "provider_id": "",
        },
    });

    // Use a oneshot channel so we can select on the POST result
    let (post_tx, mut post_rx) = tokio::sync::oneshot::channel();
    let prompt_client = client.clone();
    let prompt_url_clone = prompt_url.clone();
    let prompt_body_clone = prompt_body.clone();
    tokio::spawn(async move {
        let result = prompt_client
            .post(&prompt_url_clone)
            .json(&prompt_body_clone)
            .send()
            .await;
        let _ = post_tx.send(result);
    });

    // ── Abort URL for Ctrl+C interruption ──────────────────────────
    let abort_url = format!("{url}/session/{session_id}/abort");
    let abort_client = client.clone();

    // ── Stream SSE events ──────────────────────────────────────────
    let mut stream = sse_response.bytes_stream();
    let mut buffer = String::new();
    let mut current_event: Option<String> = None;
    let mut current_data = String::new();

    // Track tool / text state across events
    let mut accumulated_text = String::new();
    let mut tool_infos: Vec<ToolCallInfo> = Vec::new();
    let mut pending_tool_name: Option<String> = None;
    let mut step_count: u64 = 0;
    let mut running = true;
    let mut exit_code: i32 = 0;
    let mut finish_reason: Option<String> = None;

    while running {
        tokio::select! {
            // ── SSE chunk ───────────────────────────────────────────
            chunk_opt = stream.next() => {
                match chunk_opt {
                    Some(Ok(chunk)) => {
                        buffer.push_str(&String::from_utf8_lossy(&chunk));

                        // Process complete lines from the buffer
                        while let Some(line_end) = buffer.find('\n') {
                            let mut line = buffer[..=line_end].to_string();
                            buffer = buffer[line_end + 1..].to_string();
                            line = line.trim_end_matches(['\r', '\n']).to_string();

                            if line.is_empty() {
                                // Blank line = end of one SSE event
                                if !current_data.is_empty() {
                                    let event_type = current_event
                                        .take()
                                        .unwrap_or_else(|| "message".to_string());
                                    match handle_sse_event(
                                        &event_type,
                                        &current_data,
                                        &session_id,
                                        &mut accumulated_text,
                                        &mut tool_infos,
                                        &mut pending_tool_name,
                                        &mut step_count,
                                        &mut finish_reason,
                                        args,
                                    ) {
                                        SseAction::Continue => {}
                                        SseAction::Stop(ec) => {
                                            exit_code = ec;
                                            running = false;
                                        }
                                    }
                                    current_data.clear();
                                }
                            } else if let Some(field_value) = line.strip_prefix("event:") {
                                current_event = Some(field_value.trim().to_string());
                            } else if let Some(field_value) = line.strip_prefix("data:") {
                                if !current_data.is_empty() {
                                    current_data.push('\n');
                                }
                                current_data.push_str(field_value.trim());
                            }
                            // Ignore comment lines (starting with ':') and id:/retry: fields
                        }
                    }
                    Some(Err(e)) => {
                        if running {
                            eprintln!("\nSSE stream error: {e}");
                            exit_code = 1;
                        }
                        running = false;
                    }
                    None => {
                        // SSE stream ended normally
                        tracing::info!("SSE stream ended");
                        running = false;
                    }
                }
            }

            // ── Prompt POST result ──────────────────────────────────
            post_result = &mut post_rx => {
                match post_result {
                    Ok(Ok(resp)) => {
                        let status = resp.status();
                        if !status.is_success() {
                            let body = resp.text().await.unwrap_or_default();
                            eprintln!("\nServer returned HTTP {} for prompt", status);
                            if !body.is_empty() {
                                eprintln!("Body: {body}");
                            }
                            exit_code = 1;
                            running = false;
                        }
                        // On success, results come via SSE — keep streaming
                    }
                    Ok(Err(e)) => {
                        eprintln!("\nFailed to send prompt: {e}");
                        exit_code = 1;
                        running = false;
                    }
                    Err(_recv_err) => {
                        // oneshot sender dropped — shouldn't happen, but ignore
                    }
                }
            }

            // ── Ctrl+C handler ──────────────────────────────────────
            _ = tokio::signal::ctrl_c() => {
                eprintln!("\nInterrupted. Aborting remote session {session_id}...");
                let _ = abort_client.post(&abort_url).send().await;
                eprintln!("Session aborted.");
                running = false;
            }
        }
    }

    // ── Print final summary ────────────────────────────────────────
    let tool_count = tool_infos.len() as u64;
    let success_count = tool_infos.iter().filter(|t| t.success).count() as u64;
    let fail_count = tool_count - success_count;

    if args.format == "json" {
        // Emit a final done event with aggregated stats
        let reason = finish_reason.unwrap_or_else(|| "interrupted".to_string());
        println!(
            r#"{{"type":"done","finish_reason":"{}","iterations":{},"tool_calls":{},"success_tools":{},"failed_tools":{}}}"#,
            reason, step_count, tool_count, success_count, fail_count,
        );
    } else if tool_count > 0 {
        // Non-JSON: print text + tool summary
        if !accumulated_text.is_empty() {
            println!();
        }
        println!(
            "\n\u{2500}\u{2500}\u{2500} {} tool calls executed on remote server",
            tool_count,
        );
        if fail_count > 0 {
            eprintln!(
                "  {} succeeded, {} failed ({} iterations)",
                success_count, fail_count, step_count,
            );
        }
        for info in &tool_infos {
            let status = if info.success { "\u{2713}" } else { "\u{2717}" };
            println!("  {status} {}", info.name);
            if let Some(ref err) = info.error {
                eprintln!("    error: {err}");
            }
        }
    } else {
        // No tool calls — just ensure the accumulated text has been flushed
        if !accumulated_text.is_empty() && args.format != "json" {
            println!();
        }
    }

    exit_code
}

// ── SSE event dispatch helpers for cmd_run_attach ─────────────────────

/// Action returned by SSE event dispatch: continue streaming or stop.
enum SseAction {
    Continue,
    Stop(i32),
}

/// Track info about a completed tool call.
struct ToolCallInfo {
    name: String,
    success: bool,
    error: Option<String>,
}

/// Handle a single SSE event dispatched from the remote server.
///
/// Filters by `sessionID`, dispatches by event type, and updates
/// the shared accumulators and counters.
#[allow(clippy::too_many_arguments)]
fn handle_sse_event(
    event_type: &str,
    data: &str,
    session_id: &str,
    accumulated_text: &mut String,
    tool_infos: &mut Vec<ToolCallInfo>,
    pending_tool_name: &mut Option<String>,
    step_count: &mut u64,
    finish_reason: &mut Option<String>,
    args: &RunArgs,
) -> SseAction {
    // Parse the event payload
    let payload: serde_json::Value = match serde_json::from_str(data) {
        Ok(v) => v,
        Err(_) => return SseAction::Continue,
    };

    // Filter by sessionID (most session.next.* events carry it)
    if let Some(evt_sid) = payload.get("sessionID").and_then(|v| v.as_str()) {
        if evt_sid != session_id {
            return SseAction::Continue;
        }
    }
    // Events without sessionID (like server.connected) pass through

    // ── Dispatch by event type ──────────────────────────────────────
    match event_type {
        // ── Server / heartbeat (ignore) ─────────────────────────────
        "server.connected" | "server.heartbeat" => {}

        // ── Text streaming ──────────────────────────────────────────
        "session.next.text.delta" => {
            if let Some(delta) = payload.get("delta").and_then(|v| v.as_str()) {
                accumulated_text.push_str(delta);
                if args.format != "json" {
                    print!("{delta}");
                    let _ = std::io::stdout().flush();
                }
            }
            // JSON mode: emit structured text-delta event
            if args.format == "json" && !payload.is_null() {
                println!("{}", serde_json::to_string(&payload).unwrap_or_default());
            }
        }
        "session.next.text.started" => {
            if args.format == "json" && !payload.is_null() {
                println!("{}", serde_json::to_string(&payload).unwrap_or_default());
            }
        }
        "session.next.text.ended" => {
            if args.format == "json" && !payload.is_null() {
                println!("{}", serde_json::to_string(&payload).unwrap_or_default());
            }
        }

        // ── Tool lifecycle ──────────────────────────────────────────
        "session.next.tool.called" => {
            let tool_name = payload
                .get("tool")
                .and_then(|v| v.as_str())
                .unwrap_or("unknown")
                .to_string();
            if args.format != "json" {
                println!("\n  \u{2699}  calling {}...", tool_name);
            }
            if args.format == "json" {
                println!("{}", serde_json::to_string(&payload).unwrap_or_default());
            }
            *pending_tool_name = Some(tool_name);
        }
        "session.next.tool.success" => {
            if let Some(name) = pending_tool_name.take() {
                tool_infos.push(ToolCallInfo {
                    name,
                    success: true,
                    error: None,
                });
            }
            if args.format == "json" && !payload.is_null() {
                println!("{}", serde_json::to_string(&payload).unwrap_or_default());
            }
        }
        "session.next.tool.failed" => {
            let err_msg = payload
                .get("error")
                .and_then(|e| e.get("message"))
                .and_then(|v| v.as_str())
                .unwrap_or("unknown error");
            if let Some(name) = pending_tool_name.take() {
                tool_infos.push(ToolCallInfo {
                    name: name.clone(),
                    success: false,
                    error: Some(err_msg.to_string()),
                });
                if args.format != "json" {
                    eprintln!("  \u{2717} {name} failed: {err_msg}");
                }
            }
            if args.format == "json" && !payload.is_null() {
                println!("{}", serde_json::to_string(&payload).unwrap_or_default());
            }
        }
        "session.next.tool.progress" => {
            if args.format == "json" && !payload.is_null() {
                println!("{}", serde_json::to_string(&payload).unwrap_or_default());
            }
        }

        // ── Step events ─────────────────────────────────────────────
        "session.next.step.started" => {
            *step_count += 1;
            if args.format == "json" && !payload.is_null() {
                println!("{}", serde_json::to_string(&payload).unwrap_or_default());
            }
        }
        "session.next.step.ended" => {
            let finish = payload
                .get("finish")
                .and_then(|v| v.as_str())
                .unwrap_or("stop");
            *finish_reason = Some(finish.to_string());
            // Return Stop to exit the event loop gracefully
            return SseAction::Stop(0);
        }
        "session.next.step.failed" => {
            let err_msg = payload
                .get("error")
                .and_then(|v| v.as_str())
                .unwrap_or("step failed");
            *finish_reason = Some("error".to_string());
            eprintln!("\nSession step failed: {err_msg}");
            if args.format == "json" && !payload.is_null() {
                println!("{}", serde_json::to_string(&payload).unwrap_or_default());
            }
            return SseAction::Stop(1);
        }

        // ── Session abort (remote interrupt) ────────────────────────
        "session.abort" => {
            eprintln!("\nSession aborted by server.");
            *finish_reason = Some("aborted".to_string());
            return SseAction::Stop(0);
        }

        // ── Session lifecycle (informational) ───────────────────────
        "session.next.prompted" | "session.next.prompt.admitted" => {
            if args.format == "json" && !payload.is_null() {
                println!("{}", serde_json::to_string(&payload).unwrap_or_default());
            }
        }

        // ── Reasoning (show in JSON mode, ignore otherwise) ─────────
        "session.next.reasoning.delta" => {
            if args.format == "json" && !payload.is_null() {
                println!("{}", serde_json::to_string(&payload).unwrap_or_default());
            }
        }

        // ── Compaction (informational) ──────────────────────────────
        "session.next.compaction.started" | "session.next.compaction.ended" => {
            if args.format == "json" && !payload.is_null() {
                println!("{}", serde_json::to_string(&payload).unwrap_or_default());
            }
        }

        // ── Unknown / unhandled ─────────────────────────────────────
        other => {
            // Forward unknown session events in JSON mode for debugging
            if args.format == "json" && other.starts_with("session.next.") {
                println!("{}", serde_json::to_string(&payload).unwrap_or_default());
            }
        }
    }

    SseAction::Continue
}

// ═════════════════════════════════════════════════════════════════════════════
// tui
// ═════════════════════════════════════════════════════════════════════════════

/// `tui` — Start OpenCode TUI.
///
/// Ported from: `packages/opencode/src/cli/cmd/tui.ts`
async fn cmd_tui(args: &TuiArgs, print_logs: bool, config: &rustcode_core::config::Info) -> i32 {
    if args.fork && !args.r#continue && args.session.is_none() {
        cli_error::format_cli_error("--fork requires --continue or --session");
        return 1;
    }

    tracing::info!(
        "tui: project={:?}, model={:?}, continue={}, session={:?}, json={}, print_logs={}",
        args.project,
        args.model,
        args.r#continue,
        args.session,
        args.json,
        print_logs,
    );

    // When --print-logs is active, LLM call logs appear on stderr
    // alongside the TUI rendering on stdout.  Advertise this if the
    // user is running with detailed logging enabled.
    if print_logs {
        tracing::debug!("TUI running with --print-logs: LLM calls will log to stderr");
    }

    // Resolve working directory
    let cwd = if let Some(ref proj) = args.project {
        let p = std::path::Path::new(proj);
        if p.is_absolute() {
            p.to_path_buf()
        } else {
            std::env::current_dir().unwrap_or_default().join(p)
        }
    } else {
        std::env::current_dir().unwrap_or_default()
    };

    // Change to working directory (TS: process.chdir before worker start)
    if let Err(e) = std::env::set_current_dir(&cwd) {
        eprintln!(
            "Error: failed to change directory to {}: {e}",
            cwd.display()
        );
        return 1;
    }

    if !args.json {
        println!("rustcode TUI v{}", env!("CARGO_PKG_VERSION"));
        println!("Working directory: {}", cwd.display());
    }

    // ── Initialize shared runtime ──────────────────────────────────
    let ctx = match rustcode_core::runtime::initialize_runtime(config) {
        Ok(c) => c,
        Err(e) => {
            cli_error::format_provider_init_error("runtime", &e.to_string());
            return 1;
        }
    };

    if ctx.providers.is_empty() && !args.json {
        cli_error::format_cli_error(
            "No LLM providers detected. Set an API key environment variable.",
        );
        eprintln!("{}  ANTHROPIC_API_KEY              — Claude (Anthropic){}", cli_error::TEXT_DIM, cli_error::TEXT_RESET);
        eprintln!("{}  OPENAI_API_KEY                 — GPT (OpenAI){}", cli_error::TEXT_DIM, cli_error::TEXT_RESET);
        eprintln!("{}  GOOGLE_GENERATIVE_AI_API_KEY   — Gemini (Google){}", cli_error::TEXT_DIM, cli_error::TEXT_RESET);
        eprintln!("{}  OPENROUTER_API_KEY             — OpenRouter (multi-provider){}", cli_error::TEXT_DIM, cli_error::TEXT_RESET);
        eprintln!("{}  DEEPSEEK_API_KEY               — DeepSeek{}", cli_error::TEXT_DIM, cli_error::TEXT_RESET);
        eprintln!("{}  GROQ_API_KEY                   — Groq{}", cli_error::TEXT_DIM, cli_error::TEXT_RESET);
        eprintln!();
        eprintln!("{}Continuing in offline mode — prompts will not call an LLM.{}", cli_error::TEXT_WARNING, cli_error::TEXT_RESET);
    }

    let bus = ctx.bus.clone();
    let sessions = ctx.sessions.clone();
    let runner = ctx.runner.clone();
    let tools = ctx.tools.clone();
    let providers_map = ctx.providers;

    // ── JSON mode: emit session-created event ──────────────────────
    if args.json {
        let event = serde_json::json!({
            "type": "session.created",
            "timestamp": chrono::Utc::now().timestamp_millis(),
            "cwd": cwd.display().to_string(),
            "providers": providers_map.keys().collect::<Vec<_>>(),
        });
        println!("{}", serde_json::to_string(&event).unwrap_or_default());
    }

    // ── Event forwarding: bus subscriber for TUI events ────────────
    // Subscribe to the bus so that published events (from server or
    // internal components) are forwarded to the TUI.
    let _bus_sub = bus.subscribe();

    // ── JSON mode: background session-event forwarder ──────────────
    // When --json is set, spawn a task that subscribes to the bus and
    // prints `session.next.*` events as JSON lines on stdout.  This
    // runs alongside the TUI rendering so that CI/scripting consumers
    // can capture structured progress while the TUI runs interactively.
    //
    // Events emitted include: text deltas, tool calls/results, step
    // boundaries, reasoning, and compaction summaries.
    let json_bus = if args.json {
        let bus_clone = bus.clone();
        Some(tokio::spawn(async move {
            let mut sub = bus_clone.subscribe();
            while let Some(event) = sub.recv().await {
                let evt_type = event
                    .payload
                    .get("type")
                    .and_then(|v| v.as_str())
                    .unwrap_or("");
                // Forward session.next.* events and other structured events
                if evt_type.starts_with("session.next.")
                    || evt_type.starts_with("session.")
                    || evt_type == "server.connected"
                    || evt_type == "server.heartbeat"
                {
                    // Skip ephemeral heartbeats to keep output clean
                    if evt_type == "server.heartbeat" {
                        continue;
                    }
                    if let Ok(line) = serde_json::to_string(&event.payload) {
                        println!("{line}");
                    }
                }
            }
        }))
    } else {
        None
    };

    // Spawn a background heartbeat task to detect bus/server health.
    // Periodically publishes a ping event; if no subscriber is alive
    // the publish fails, indicating disconnection.
    let heartbeat_bus = bus.clone();
    let heartbeat_handle = tokio::spawn(async move {
        let mut interval = tokio::time::interval(std::time::Duration::from_secs(30));
        loop {
            interval.tick().await;
            let ping = rustcode_core::bus::GlobalEvent::new(serde_json::json!({
                "type": "tui.heartbeat.ping",
                "timestamp": chrono::Utc::now().timestamp_millis(),
            }));
            if heartbeat_bus.publish(ping).is_err() {
                tracing::warn!("TUI heartbeat: no active bus subscribers");
                break;
            }
        }
    });

    // ── Launch TUI ─────────────────────────────────────────────────
    let tool_defs = tools.llm_definitions();
    match rustcode_tui::TuiApp::new(sessions, runner, providers_map, bus.clone(), tool_defs) {
        Ok(mut app) => {
            let rt = tokio::runtime::Runtime::new().unwrap();

            let exit_code = rt.block_on(app.run_async());

            // When TUI exits, publish a shutdown event for cleanup.
            let _ = rt.block_on(async {
                let shutdown_event = rustcode_core::bus::GlobalEvent::new(serde_json::json!({
                    "type": "tui.shutdown",
                    "timestamp": chrono::Utc::now().timestamp_millis(),
                }));
                bus.publish(shutdown_event)
            });

            // Abort the heartbeat task
            heartbeat_handle.abort();

            // Abort the JSON bus forwarder if active
            if let Some(ref handle) = json_bus {
                handle.abort();
            }

            // Graceful cleanup
            if let Err(e) = app.cleanup() {
                if !args.json {
                    eprintln!("Terminal cleanup error: {e}");
                }
                return 1;
            }

            match exit_code {
                Ok(()) => {
                    if args.json {
                        let event = serde_json::json!({
                            "type": "session.ended",
                            "timestamp": chrono::Utc::now().timestamp_millis(),
                        });
                        println!("{}", serde_json::to_string(&event).unwrap_or_default());
                    }
                    0
                }
                Err(e) => {
                    if args.json {
                        let event = serde_json::json!({
                            "type": "session.error",
                            "timestamp": chrono::Utc::now().timestamp_millis(),
                            "error": e.to_string(),
                        });
                        println!("{}", serde_json::to_string(&event).unwrap_or_default());
                    } else {
                        eprintln!("TUI error: {e}");
                    }
                    1
                }
            }
        }
        Err(e) => {
            heartbeat_handle.abort();
            if let Some(ref handle) = json_bus {
                handle.abort();
            }
            if args.json {
                let event = serde_json::json!({
                    "type": "tui.init_error",
                    "timestamp": chrono::Utc::now().timestamp_millis(),
                    "error": e.to_string(),
                });
                println!("{}", serde_json::to_string(&event).unwrap_or_default());
            } else {
                eprintln!("Failed to initialize TUI: {e}");
            }
            1
        }
    }
}

// ═════════════════════════════════════════════════════════════════════════════
// serve
// ═════════════════════════════════════════════════════════════════════════════

/// `serve` — Start a headless OpenCode server.
///
/// Ported from: `packages/opencode/src/cli/cmd/serve.ts`
async fn cmd_serve(args: &NetworkArgs, config: &rustcode_core::config::Info) -> i32 {
    let hostname = if args.mdns && args.hostname == "127.0.0.1" {
        "0.0.0.0".to_string()
    } else {
        args.hostname.clone()
    };

    if std::env::var("OPENCODE_SERVER_PASSWORD").is_err() {
        eprintln!("{}Warning: OPENCODE_SERVER_PASSWORD is not set; server will be unsecured.{}", cli_error::TEXT_WARNING, cli_error::TEXT_RESET);
    }

    eprintln!(
        "rustcode serve: starting server on {hostname}:{}...",
        args.port
    );
    println!();

    // Build the AppState from the shared runtime
    let ctx = match rustcode_core::runtime::initialize_runtime(config) {
        Ok(c) => c,
        Err(e) => {
            cli_error::format_provider_init_error("runtime", &e.to_string());
            return 1;
        }
    };
    let state = build_server_state(&ctx);
    let config = rustcode_server::ServerConfig {
        hostname,
        port: args.port,
        cors_origins: if args.cors.is_empty() {
            None
        } else {
            Some(args.cors.clone())
        },
    };

    match rustcode_server::serve(state, config).await {
        Ok(_) => {
            eprintln!("{}Server shut down.{}", cli_error::TEXT_DIM, cli_error::TEXT_RESET);
            0
        }
        Err(e) => {
            cli_error::format_cli_error(&format!("Failed to start server: {e}"));
            1
        }
    }
}

/// Build the shared application state from a [`RuntimeContext`].
///
/// This replaces the hand-rolled `build_server_state()` that previously
/// duplicated the service construction logic across cmd_serve and cmd_web.
fn build_server_state(
    ctx: &rustcode_core::runtime::RuntimeContext,
) -> Arc<rustcode_server::AppState> {
    // Build agent service from config if available
    let agent_service = build_agent_service();

    // Build command data from config
    let command_data = Arc::new(build_command_data());

    // Integration service starts empty — integrations are registered at runtime
    let integration_service = Arc::new(rustcode_core::integration::IntegrationService::new());

    // Reference service — loaded from config
    let reference_service = Arc::new(rustcode_core::reference::ReferenceService::new());

    let server_features: Vec<String> = vec![
        "agents".into(),
        "commands".into(),
        "skills".into(),
        "integrations".into(),
        "references".into(),
        "models".into(),
        "providers".into(),
        "sessions".into(),
        "events".into(),
        "mcp".into(),
        "lsp".into(),
        "tools".into(),
        "permissions".into(),
        "questions".into(),
        "projects".into(),
        "worktree".into(),
    ];

    Arc::new(rustcode_server::AppState::new(
        ctx.bus.clone(),
        ctx.sessions.clone(),
        ctx.tools.clone(),
        ctx.permissions.clone(),
        ctx.questions.clone(),
        ctx.runner.clone(),
        ctx.providers.clone(),
        agent_service,
        command_data,
        integration_service,
        reference_service,
        server_features,
    ))
}

/// Build an agent service from the global config, if it can be loaded.
fn build_agent_service() -> Option<Arc<rustcode_core::agent::AgentService>> {
    let cfg = rustcode_core::config::Config::load_global().ok()?;
    let worktree = std::env::current_dir().ok()?;
    let data_dir = dirs::data_dir()?.join("opencode");
    let tmp_dir = std::env::temp_dir();
    let skill_dirs: Vec<std::path::PathBuf> = Vec::new();
    Some(Arc::new(rustcode_core::agent::AgentService::new(
        &cfg, worktree, data_dir, tmp_dir, skill_dirs,
    )))
}

/// Build command data from the global config's command section.
fn build_command_data() -> rustcode_core::command::CommandData {
    let mut data = rustcode_core::command::CommandData::default();
    if let Ok(cfg) = rustcode_core::config::Config::load_global() {
        for (name, cmd_cfg) in &cfg.command {
            let model_ref = cmd_cfg
                .model
                .as_ref()
                .and_then(|m| rustcode_core::command::CommandModelRef::parse(m));
            data.upsert(rustcode_core::command::CommandUpdateInput {
                name: name.clone(),
                template: cmd_cfg.template.clone(),
                description: cmd_cfg.description.clone(),
                agent: cmd_cfg.agent.clone(),
                model: model_ref,
                subtask: cmd_cfg.subtask.unwrap_or(false),
            });
        }
    }
    data
}

// ═════════════════════════════════════════════════════════════════════════════
// web
// ═════════════════════════════════════════════════════════════════════════════

/// `web` — Start server and open web interface.
///
/// Ported from: `packages/opencode/src/cli/cmd/web.ts`
async fn cmd_web(args: &NetworkArgs, config: &rustcode_core::config::Info) -> i32 {
    let hostname = if args.mdns && args.hostname == "127.0.0.1" {
        "0.0.0.0".to_string()
    } else {
        args.hostname.clone()
    };

    if std::env::var("OPENCODE_SERVER_PASSWORD").is_err() {
        eprintln!("{}!  OPENCODE_SERVER_PASSWORD is not set; server will be unsecured.{}", cli_error::TEXT_WARNING, cli_error::TEXT_RESET);
    }

    let port = if args.port == 0 { 4096u16 } else { args.port };
    let server_url = format!("http://{hostname}:{port}");

    println!("rustcode web: starting server + web interface...");
    println!();
    println!("  Local access:      {server_url}");

    if hostname == "0.0.0.0" {
        if let Ok(addrs) = get_network_ips() {
            for ip in &addrs {
                println!("  Network access:    http://{ip}:{port}");
            }
        }
    }
    if args.mdns {
        println!("  mDNS:              http://{}:{}", args.mdns_domain, port);
    }
    println!();

    // Try to open browser
    open_url(&server_url);

    // Build and start the server from shared runtime
    let ctx = match rustcode_core::runtime::initialize_runtime(config) {
        Ok(c) => c,
        Err(e) => {
            cli_error::format_provider_init_error("runtime", &e.to_string());
            return 1;
        }
    };
    let state = build_server_state(&ctx);
    let config = rustcode_server::ServerConfig {
        hostname,
        port,
        cors_origins: if args.cors.is_empty() {
            None
        } else {
            Some(args.cors.clone())
        },
    };

    match rustcode_server::serve(state, config).await {
        Ok(_) => 0,
        Err(e) => {
            cli_error::format_cli_error(&format!("Server error: {e}"));
            1
        }
    }
}

/// Open a URL in the default browser (cross-platform).
fn open_url(url: &str) {
    #[cfg(target_os = "linux")]
    {
        let _ = std::process::Command::new("xdg-open").arg(url).spawn();
    }
    #[cfg(target_os = "macos")]
    {
        let _ = std::process::Command::new("open").arg(url).spawn();
    }
    #[cfg(target_os = "windows")]
    {
        let _ = std::process::Command::new("cmd")
            .args(["/c", "start", url])
            .spawn();
    }
    #[cfg(not(any(target_os = "linux", target_os = "macos", target_os = "windows")))]
    {
        let _ = url; // no-op on other platforms
    }
}

/// Get non-internal IPv4 network addresses (like TS getNetworkIPs).
fn get_network_ips() -> Result<Vec<String>, Box<dyn std::error::Error>> {
    use std::net::UdpSocket;
    // Simple approach: bind a UDP socket and get the local address
    let socket = UdpSocket::bind("0.0.0.0:0")?;
    socket.connect("8.8.8.8:80")?;
    let addr = socket.local_addr()?.ip().to_string();
    Ok(vec![addr])
}

// ═════════════════════════════════════════════════════════════════════════════
// models
// ═════════════════════════════════════════════════════════════════════════════

/// `models` — List all available models from detected providers.
///
/// Ported from: `packages/opencode/src/cli/cmd/models.ts`
async fn cmd_models(args: &ModelsArgs, _config: &rustcode_core::config::Info) -> i32 {
    if args.refresh {
        eprintln!("Models cache refresh requested — not yet wired to models.dev API.");
    }

    let providers: Vec<Box<dyn rustcode_core::provider::Provider>> =
        rustcode_core::providers::auto_detect_all();

    if providers.is_empty() {
        eprintln!("No providers detected. Set API key environment variables.");
        eprintln!("Run `rustcode providers list` to see available credential slots.");
        return 1;
    }

    // Sort providers: priority order then alphabetical
    let mut sorted: Vec<(String, Box<dyn rustcode_core::provider::Provider>)> = providers
        .into_iter()
        .map(|p| {
            let id = p.provider_id().to_string();
            (id, p)
        })
        .collect();
    sorted.sort_by(|(a, _), (b, _)| {
        let a_prio = provider_priority(a);
        let b_prio = provider_priority(b);
        a_prio.cmp(&b_prio).then_with(|| a.cmp(b))
    });

    for (provider_id, provider) in &sorted {
        // Filter by provider if specified
        if let Some(ref filter) = args.provider {
            if provider_id != filter {
                continue;
            }
        }

        match provider.list_models().await {
            Ok(models) => {
                let mut model_names: Vec<&str> = models.iter().map(|m| m.id.as_str()).collect();
                model_names.sort();

                for model_id in &model_names {
                    println!("{provider_id}/{model_id}");
                    if args.verbose {
                        if let Some(model) = models.iter().find(|m| m.id == *model_id) {
                            if let Ok(json) = serde_json::to_string_pretty(model) {
                                println!("{json}");
                            }
                        }
                    }
                }
            }
            Err(e) => {
                eprintln!("Failed to list models for {provider_id}: {e}");
            }
        }
    }

    if sorted.is_empty() {
        eprintln!("No models found.");
    }

    0
}

/// Priority for sorting providers (lower = shown first).
fn provider_priority(id: &str) -> u8 {
    match id {
        "anthropic" => 0,
        "openai" => 1,
        "google" => 2,
        "openrouter" => 3,
        _ => 99,
    }
}

// ═════════════════════════════════════════════════════════════════════════════
// stats
// ═════════════════════════════════════════════════════════════════════════════

/// `stats` — Show token usage and cost statistics.
///
/// Ported from: `packages/opencode/src/cli/cmd/stats.ts`
async fn cmd_stats(args: &StatsArgs) -> i32 {
    let days_label = args.days.map_or("all time".to_string(), |d| {
        if d == 0 {
            "today".to_string()
        } else {
            format!("last {d} days")
        }
    });

    let project_label = match &args.project {
        Some(p) if p.is_empty() => "current project".to_string(),
        Some(p) => p.clone(),
        None => "all projects".to_string(),
    };

    let db_path = get_db_path();
    if !db_path.exists() {
        println!("rustcode stats ({days_label}, {project_label})");
        println!();
        eprintln!("No session database found at {}", db_path.display());
        eprintln!("Start `rustcode serve` and run sessions to populate stats.");
        return 0;
    }

    // Build connection URL and query the database
    let db_url = format!("sqlite:{}", db_path.display());

    let pool = match sqlx::SqlitePool::connect(&db_url).await {
        Ok(p) => p,
        Err(e) => {
            eprintln!("Failed to connect to database: {e}");
            return 1;
        }
    };

    // ── Build dynamic query ──────────────────────────────────────────────
    let mut where_clauses: Vec<String> = Vec::new();
    let _params: Vec<String> = Vec::new();

    // --days filter
    if let Some(days) = args.days {
        if days > 0 {
            let cutoff_ms = chrono::Utc::now().timestamp_millis() as u64 - (days as u64 * 86400000);
            where_clauses.push(format!("s.time_created > {}", cutoff_ms));
        }
    }

    // --project filter
    if let Some(ref proj) = args.project {
        if !proj.is_empty() {
            where_clauses.push(format!("s.project_id = '{}'", proj.replace('\'', "''")));
        }
    }

    let where_sql = if where_clauses.is_empty() {
        String::new()
    } else {
        format!("WHERE {}", where_clauses.join(" AND "))
    };

    // ── OVERVIEW ────────────────────────────────────────────────────────
    println!("rustcode stats ({days_label}, {project_label})");
    println!();
    print_header("OVERVIEW");

    let overview_sql = format!(
        "SELECT COUNT(*) as sessions, COALESCE(SUM(s.cost),0) as total_cost, \
         COALESCE(SUM(s.tokens_input),0) as total_input, \
         COALESCE(SUM(s.tokens_output),0) as total_output \
         FROM session s {where_sql}"
    );

    match sqlx::query(&overview_sql).fetch_one(&pool).await {
        Ok(row) => {
            let sessions: i64 = row.get(0);
            let total_cost: f64 = row.get(1);
            let total_input: i64 = row.get(2);
            let total_output: i64 = row.get(3);
            println!("Sessions     {sessions}");
            println!("Messages     -- (use --models for breakdown)");
            println!("Days         {days_label}");
            println!();

            print_header("COST & TOKENS");
            println!("Total Cost              ${total_cost:.2}");
            let avg_cost = if sessions > 0 {
                total_cost / sessions as f64
            } else {
                0.0
            };
            println!("Avg Cost/Session        ${avg_cost:.4}");
            let total_tokens = total_input + total_output;
            let avg_tokens = if sessions > 0 {
                total_tokens / sessions
            } else {
                0
            };
            println!("Avg Tokens/Session      {avg_tokens}");
            println!(
                "Total Input             {}",
                format_count(total_input as u64)
            );
            println!(
                "Total Output            {}",
                format_count(total_output as u64)
            );
            println!();
        }
        Err(e) => {
            eprintln!("Failed to query session stats: {e}");
            let _ = pool.close().await;
            return 1;
        }
    }

    // ── MODEL USAGE (--models flag) ─────────────────────────────────────
    if args.models.is_some() {
        print_header("MODEL USAGE");
        let model_sql = format!(
            "SELECT s.model, COUNT(*) as cnt, COALESCE(SUM(s.cost),0) as cost, \
             COALESCE(SUM(s.tokens_input),0) as input_tok, \
             COALESCE(SUM(s.tokens_output),0) as output_tok \
             FROM session s {where_sql} GROUP BY s.model ORDER BY cnt DESC"
        );

        match sqlx::query(&model_sql).fetch_all(&pool).await {
            Ok(rows) => {
                if rows.is_empty() {
                    println!("  No session data available.");
                } else {
                    println!(
                        "  {:<30} {:>8} {:>10} {:>12}",
                        "Model", "Sessions", "Cost", "Tokens"
                    );
                    println!("  {}", "\u{2500}".repeat(62));
                    for row in &rows {
                        let model: String = row.get(0);
                        let cnt: i64 = row.get(1);
                        let cost: f64 = row.get(2);
                        let input_tok: i64 = row.get(3);
                        let output_tok: i64 = row.get(4);
                        let tot = input_tok + output_tok;
                        println!(
                            "  {:<30} {:>8} ${:>9.2} {:>11}",
                            model,
                            cnt,
                            cost,
                            format_count(tot as u64)
                        );
                    }
                }
            }
            Err(e) => eprintln!("  Failed to query model usage: {e}"),
        }
        println!();
    }

    // ── TOOL USAGE ──────────────────────────────────────────────────────
    if args.tools.is_some() || args.tools.is_none() {
        print_header("TOOL USAGE");
        // Query tools from the tool part entries in the message/part tables
        let tool_sql = format!(
            "SELECT p.tool_name, COUNT(*) as cnt \
             FROM part p \
             INNER JOIN message m ON m.id = p.message_id \
             INNER JOIN session s ON s.id = m.session_id \
             {where_clause_str} \
             WHERE p.tool_name IS NOT NULL \
             GROUP BY p.tool_name ORDER BY cnt DESC",
            where_clause_str = if where_clauses.is_empty() {
                String::new()
            } else {
                let session_where = where_clauses
                    .iter()
                    .map(String::as_str)
                    .collect::<Vec<_>>()
                    .join(" AND ");
                if session_where.is_empty() {
                    String::new()
                } else {
                    format!("AND {session_where}")
                }
            }
        );

        match sqlx::query(&tool_sql).fetch_all(&pool).await {
            Ok(rows) => {
                if rows.is_empty() {
                    println!("  No tool usage data available.");
                } else {
                    let limit = args.tools.unwrap_or(rows.len());
                    println!("  {:<30} {:>8}", "Tool", "Calls");
                    println!("  {}", "\u{2500}".repeat(40));
                    for row in rows.iter().take(limit) {
                        let tool: String = row.get(0);
                        let cnt: i64 = row.get(1);
                        println!("  {:<30} {:>8}", tool, cnt);
                    }
                }
            }
            Err(e) => eprintln!("  Failed to query tool usage: {e}"),
        }
        println!();
    }

    let _ = pool.close().await;
    0
}

// ═════════════════════════════════════════════════════════════════════════════
// export / import
// ═════════════════════════════════════════════════════════════════════════════

/// `export` — Export session data as JSON.
///
/// Ported from: `packages/opencode/src/cli/cmd/export.ts`
async fn cmd_export(args: &ExportArgs) -> i32 {
    let db_path = get_db_path();
    if !db_path.exists() {
        eprintln!("No local session database found at {}", db_path.display());
        eprintln!("Export requires session data created by `rustcode serve` or `rustcode tui`.");
        return 1;
    }

    let db_url = format!("sqlite:{}", db_path.display());
    let pool = match sqlx::SqlitePool::connect(&db_url).await {
        Ok(p) => p,
        Err(e) => {
            eprintln!("Failed to connect to database: {e}");
            return 1;
        }
    };

    // ── Determine which session to export ───────────────────────────────
    let session_id = if let Some(ref sid) = args.session_id {
        sid.clone()
    } else {
        // "latest" — pick the most recently updated session
        match sqlx::query("SELECT id FROM session ORDER BY time_updated DESC LIMIT 1")
            .fetch_optional(&pool)
            .await
        {
            Ok(Some(row)) => {
                let sid: String = row.get(0);
                sid
            }
            Ok(None) => {
                eprintln!("No sessions found in database.");
                let _ = pool.close().await;
                return 1;
            }
            Err(e) => {
                eprintln!("Failed to query sessions: {e}");
                let _ = pool.close().await;
                return 1;
            }
        }
    };

    if args.sanitize {
        eprintln!("Sanitize mode: sensitive data will be redacted.");
    }

    eprintln!("Exporting session: {session_id}");

    // ── Query session info ──────────────────────────────────────────────
    let session_row = match sqlx::query("SELECT * FROM session WHERE id = ?")
        .bind(&session_id)
        .fetch_optional(&pool)
        .await
    {
        Ok(Some(row)) => row,
        Ok(None) => {
            eprintln!("Session '{session_id}' not found.");
            let _ = pool.close().await;
            return 1;
        }
        Err(e) => {
            eprintln!("Failed to query session: {e}");
            let _ = pool.close().await;
            return 1;
        }
    };

    // Build session info JSON from the row columns
    let mut session_info = serde_json::Map::new();
    let columns = session_row.columns();
    for col in columns {
        let name = col.name().to_string();
        let val: serde_json::Value = match col.type_info().name() {
            "TEXT" | "BLOB" => {
                let s: Option<String> = session_row.try_get(name.as_str()).ok();
                match s {
                    Some(_v) if args.sanitize && is_sensitive_field(&name) => {
                        serde_json::Value::String("[REDACTED]".into())
                    }
                    Some(v) => serde_json::Value::String(v),
                    None => serde_json::Value::Null,
                }
            }
            "INTEGER" => {
                if let Ok(v) = session_row.try_get::<i64, _>(name.as_str()) {
                    serde_json::Value::Number(v.into())
                } else {
                    serde_json::Value::Null
                }
            }
            "REAL" => {
                if let Ok(v) = session_row.try_get::<f64, _>(name.as_str()) {
                    serde_json::json!(v)
                } else {
                    serde_json::Value::Null
                }
            }
            _ => serde_json::Value::Null,
        };
        session_info.insert(name, val);
    }

    // ── Query messages ──────────────────────────────────────────────────
    let messages =
        match sqlx::query("SELECT * FROM message WHERE session_id = ? ORDER BY time_created")
            .bind(&session_id)
            .fetch_all(&pool)
            .await
        {
            Ok(rows) => rows,
            Err(e) => {
                eprintln!("Failed to query messages: {e}");
                let _ = pool.close().await;
                return 1;
            }
        };

    let mut messages_json = Vec::new();
    for msg_row in &messages {
        let mut msg_info = serde_json::Map::new();
        for col in msg_row.columns() {
            let name = col.name().to_string();
            let val: serde_json::Value = match col.type_info().name() {
                "TEXT" | "BLOB" => {
                    let s: Option<String> = msg_row.try_get(name.as_str()).ok();
                    match s {
                        Some(_v) if args.sanitize && is_sensitive_field(&name) => {
                            serde_json::Value::String("[REDACTED]".into())
                        }
                        Some(v) => serde_json::Value::String(v),
                        None => serde_json::Value::Null,
                    }
                }
                "INTEGER" => {
                    if let Ok(v) = msg_row.try_get::<i64, _>(name.as_str()) {
                        serde_json::Value::Number(v.into())
                    } else {
                        serde_json::Value::Null
                    }
                }
                "REAL" => {
                    if let Ok(v) = msg_row.try_get::<f64, _>(name.as_str()) {
                        serde_json::json!(v)
                    } else {
                        serde_json::Value::Null
                    }
                }
                _ => serde_json::Value::Null,
            };
            msg_info.insert(name, val);
        }

        // ── Query parts for this message ─────────────────────────────────
        let msg_id: String = msg_row.try_get("id").unwrap_or_default();
        let parts = match sqlx::query("SELECT * FROM part WHERE message_id = ? ORDER BY time_start")
            .bind(&msg_id)
            .fetch_all(&pool)
            .await
        {
            Ok(rows) => rows,
            Err(e) => {
                eprintln!("Failed to query parts for message {msg_id}: {e}");
                continue;
            }
        };

        let mut parts_json = Vec::new();
        for part_row in &parts {
            let mut part_info = serde_json::Map::new();
            for col in part_row.columns() {
                let name = col.name().to_string();
                let val: serde_json::Value = match col.type_info().name() {
                    "TEXT" | "BLOB" => {
                        let s: Option<String> = part_row.try_get(name.as_str()).ok();
                        match s {
                            Some(_v) if args.sanitize && is_sensitive_field(&name) => {
                                serde_json::Value::String("[REDACTED]".into())
                            }
                            Some(v) => serde_json::Value::String(v),
                            None => serde_json::Value::Null,
                        }
                    }
                    "INTEGER" => {
                        if let Ok(v) = part_row.try_get::<i64, _>(name.as_str()) {
                            serde_json::Value::Number(v.into())
                        } else {
                            serde_json::Value::Null
                        }
                    }
                    "REAL" => {
                        if let Ok(v) = part_row.try_get::<f64, _>(name.as_str()) {
                            serde_json::json!(v)
                        } else {
                            serde_json::Value::Null
                        }
                    }
                    _ => serde_json::Value::Null,
                };
                part_info.insert(name, val);
            }
            parts_json.push(serde_json::Value::Object(part_info));
        }

        msg_info.insert("parts".into(), serde_json::Value::Array(parts_json));
        messages_json.push(serde_json::Value::Object(msg_info));
    }

    // ── Serialize to JSON ───────────────────────────────────────────────
    let output = serde_json::json!({
        "info": session_info,
        "messages": messages_json,
    });

    match serde_json::to_string_pretty(&output) {
        Ok(json) => println!("{json}"),
        Err(e) => {
            eprintln!("Failed to serialize export: {e}");
            let _ = pool.close().await;
            return 1;
        }
    }

    let _ = pool.close().await;
    0
}

/// Check if a database column name looks like it holds sensitive data.
fn is_sensitive_field(name: &str) -> bool {
    name.contains("token")
        || name.contains("secret")
        || name.contains("password")
        || name.contains("credential")
        || name.contains("api_key")
        || name.contains("auth")
}

/// `import` — Import session data from JSON file or URL.
///
/// Ported from: `packages/opencode/src/cli/cmd/import.ts`
async fn cmd_import(args: &ImportArgs) -> i32 {
    let is_url = args.file.starts_with("http://") || args.file.starts_with("https://");

    let data: serde_json::Value = if is_url {
        eprintln!("Fetching session data from URL: {}", args.file);
        match reqwest::get(&args.file).await {
            Ok(response) => {
                if !response.status().is_success() {
                    eprintln!("Failed to fetch share data: HTTP {}", response.status());
                    return 1;
                }
                match response.json::<serde_json::Value>().await {
                    Ok(json) => json,
                    Err(e) => {
                        eprintln!("Failed to parse JSON response: {e}");
                        return 1;
                    }
                }
            }
            Err(e) => {
                eprintln!("Failed to fetch URL: {e}");
                return 1;
            }
        }
    } else {
        let path = PathBuf::from(&args.file);
        if !path.exists() {
            eprintln!("File not found: {}", args.file);
            return 1;
        }
        match std::fs::read_to_string(&path) {
            Ok(contents) => match serde_json::from_str::<serde_json::Value>(&contents) {
                Ok(json) => json,
                Err(e) => {
                    eprintln!("Failed to parse JSON: {e}");
                    return 1;
                }
            },
            Err(e) => {
                eprintln!("Failed to read file: {e}");
                return 1;
            }
        }
    };

    eprintln!(
        "Read session data ({} bytes).",
        serde_json::to_string(&data).unwrap_or_default().len()
    );

    // ── Open database ───────────────────────────────────────────────────
    let db_path = get_db_path();
    let db_url = format!("sqlite:{}", db_path.display());

    let pool = match sqlx::SqlitePool::connect(&db_url).await {
        Ok(p) => p,
        Err(e) => {
            eprintln!("Failed to connect to database: {e}");
            return 1;
        }
    };

    // ── Parse session info ──────────────────────────────────────────────
    let info = data.get("info").and_then(|i| i.as_object());
    let messages_arr = data.get("messages").and_then(|m| m.as_array());

    let session_id = info
        .and_then(|i| i.get("id"))
        .and_then(|v| v.as_str())
        .map(|s| s.to_string())
        .unwrap_or_else(|| format!("imported-{}", chrono::Utc::now().timestamp_millis()));

    eprintln!("Importing session: {session_id}");

    // ── Check if session already exists ─────────────────────────────────
    let existing: Option<String> = sqlx::query_scalar("SELECT id FROM session WHERE id = ?")
        .bind(&session_id)
        .fetch_optional(&pool)
        .await
        .unwrap_or(None);

    if existing.is_some() {
        eprintln!("Session '{session_id}' already exists. Use a different ID or delete it first.");
        let _ = pool.close().await;
        return 1;
    }

    // ── Insert session row ──────────────────────────────────────────────
    if let Some(info_map) = info {
        let mut columns = Vec::new();
        let mut values: Vec<String> = Vec::new();

        for (col, val) in info_map {
            if col == "parts" || col == "messages" {
                continue;
            }
            let sql_val = match val {
                serde_json::Value::String(s) => s.clone(),
                serde_json::Value::Number(n) => n.to_string(),
                serde_json::Value::Null => continue,
                other => other.to_string(),
            };
            columns.push(col.clone());
            values.push(sql_val);
        }

        if !columns.is_empty() {
            let insert_sql = format!(
                "INSERT OR REPLACE INTO session ({}) VALUES ({})",
                columns.join(", "),
                columns.iter().map(|_| "?").collect::<Vec<_>>().join(", ")
            );

            let mut query = sqlx::query(&insert_sql);
            for val in &values {
                query = query.bind(val);
            }
            if let Err(e) = query.execute(&pool).await {
                eprintln!("Failed to insert session: {e}");
                let _ = pool.close().await;
                return 1;
            }
        }
    }

    // ── Insert messages and parts ───────────────────────────────────────
    if let Some(messages) = messages_arr {
        for msg_val in messages {
            if let Some(msg_map) = msg_val.as_object() {
                let msg_id = msg_map
                    .get("id")
                    .and_then(|v| v.as_str())
                    .map(|s| s.to_string())
                    .unwrap_or_else(|| format!("msg-{}", chrono::Utc::now().timestamp_millis()));

                let mut msg_columns = Vec::new();
                let mut msg_values: Vec<String> = Vec::new();

                for (col, val) in msg_map {
                    if col == "parts" {
                        continue;
                    }
                    let sql_val = match val {
                        serde_json::Value::String(s) => s.clone(),
                        serde_json::Value::Number(n) => n.to_string(),
                        serde_json::Value::Null => continue,
                        other => other.to_string(),
                    };
                    msg_columns.push(col.clone());
                    msg_values.push(sql_val);
                }

                if !msg_columns.is_empty() {
                    let insert_msg = format!(
                        "INSERT OR REPLACE INTO message ({}) VALUES ({})",
                        msg_columns.join(", "),
                        msg_columns
                            .iter()
                            .map(|_| "?")
                            .collect::<Vec<_>>()
                            .join(", ")
                    );

                    let mut query = sqlx::query(&insert_msg);
                    for val in &msg_values {
                        query = query.bind(val);
                    }
                    if let Err(e) = query.execute(&pool).await {
                        eprintln!("Failed to insert message {msg_id}: {e}");
                        continue;
                    }
                }

                // ── Insert parts ────────────────────────────────────────
                if let Some(parts) = msg_map.get("parts").and_then(|p| p.as_array()) {
                    for part_val in parts {
                        if let Some(part_map) = part_val.as_object() {
                            let mut part_columns = Vec::new();
                            let mut part_values: Vec<String> = Vec::new();

                            for (col, val) in part_map {
                                let sql_val = match val {
                                    serde_json::Value::String(s) => s.clone(),
                                    serde_json::Value::Number(n) => n.to_string(),
                                    serde_json::Value::Null => continue,
                                    other => other.to_string(),
                                };
                                part_columns.push(col.clone());
                                part_values.push(sql_val);
                            }

                            if !part_columns.is_empty() {
                                let insert_part = format!(
                                    "INSERT OR REPLACE INTO part ({}) VALUES ({})",
                                    part_columns.join(", "),
                                    part_columns
                                        .iter()
                                        .map(|_| "?")
                                        .collect::<Vec<_>>()
                                        .join(", ")
                                );

                                let mut query = sqlx::query(&insert_part);
                                for val in &part_values {
                                    query = query.bind(val);
                                }
                                if let Err(e) = query.execute(&pool).await {
                                    eprintln!("Failed to insert part for message {msg_id}: {e}");
                                }
                            }
                        }
                    }
                }
            }
        }
    }

    println!("Imported session: {session_id}");
    eprintln!(
        "Successfully imported session '{session_id}' into {}",
        db_path.display()
    );

    let _ = pool.close().await;
    0
}

/// Get the database path (matches TS Database.path()).
fn get_db_path() -> PathBuf {
    // Use the same path as RuntimeContext for consistency.
    rustcode_core::runtime::default_db_path()
}

// ═════════════════════════════════════════════════════════════════════════════
// session
// ═════════════════════════════════════════════════════════════════════════════

/// `session` — Manage sessions (list, delete).
///
/// Ported from: `packages/opencode/src/cli/cmd/session.ts`
async fn cmd_session(cmd: &SessionCommand) -> i32 {
    match cmd {
        SessionCommand::List { max_count, format } => {
            let db_path = get_db_path();
            if !db_path.exists() {
                eprintln!("No session database found at {}", db_path.display());
                eprintln!("Sessions are persisted by the server. Start `rustcode serve` or");
                eprintln!("run `rustcode tui` to create sessions, then list them here.");
                return 0;
            }

            let limit = max_count.unwrap_or(50) as i64;
            let db_url = format!("sqlite:{}", db_path.display());

            let pool = match sqlx::SqlitePool::connect(&db_url).await {
                Ok(p) => p,
                Err(e) => {
                    eprintln!("Failed to connect to database: {e}");
                    return 1;
                }
            };

            let query_sql = "SELECT s.id, s.title, s.agent, s.model, s.time_created, \
                             COUNT(m.id) as msg_count \
                             FROM session s \
                             LEFT JOIN message m ON m.session_id = s.id \
                             GROUP BY s.id \
                             ORDER BY s.time_updated DESC \
                             LIMIT ?";

            let rows = match sqlx::query(query_sql).bind(limit).fetch_all(&pool).await {
                Ok(r) => r,
                Err(e) => {
                    eprintln!("Failed to query sessions: {e}");
                    let _ = pool.close().await;
                    return 1;
                }
            };

            if *format == "json" {
                let mut json_rows = Vec::new();
                for row in &rows {
                    let id: String = row.get(0);
                    let title: String = row.get(1);
                    let agent: Option<String> = row.try_get(2).ok();
                    let model: Option<String> = row.try_get(3).ok();
                    let created: i64 = row.get(4);
                    let msg_count: i64 = row.get(5);
                    json_rows.push(serde_json::json!({
                        "id": id,
                        "title": title,
                        "agent": agent,
                        "model": model,
                        "created": created,
                        "message_count": msg_count,
                    }));
                }
                match serde_json::to_string_pretty(&json_rows) {
                    Ok(json) => println!("{json}"),
                    Err(e) => {
                        eprintln!("Failed to serialize JSON: {e}");
                        let _ = pool.close().await;
                        return 1;
                    }
                }
            } else {
                eprintln!(
                    "Listing up to {limit} most recent sessions from {}",
                    db_path.display()
                );
                eprintln!();
                println!(
                    "{:<36} {:<30} {:<8} {:<20} {:<6} Created",
                    "ID", "Title", "Agent", "Model", "Msgs"
                );
                println!("{}", "\u{2500}".repeat(116));
                for row in &rows {
                    let id: String = row.get(0);
                    let title: String = row.get(1);
                    let agent: String = row.try_get::<String, _>(2).unwrap_or_else(|_| "-".into());
                    let model: String = row.try_get::<String, _>(3).unwrap_or_else(|_| "-".into());
                    let created: i64 = row.get(4);
                    let msg_count: i64 = row.get(5);

                    // Format timestamp
                    let ts = chrono::DateTime::from_timestamp_millis(created)
                        .map(|dt| dt.format("%Y-%m-%d %H:%M").to_string())
                        .unwrap_or_else(|| created.to_string());

                    // Truncate title and model for display
                    let title_trunc = if title.len() > 29 {
                        format!("{}...", &title[..26])
                    } else {
                        title
                    };
                    let model_trunc = if model.len() > 19 {
                        format!("{}...", &model[..16])
                    } else {
                        model
                    };

                    println!(
                        "{:<36} {:<30} {:<8} {:<20} {:<6} {ts}",
                        id, title_trunc, agent, model_trunc, msg_count
                    );
                }
            }

            let _ = pool.close().await;
        }
        SessionCommand::Delete { session_id } => {
            let db_path = get_db_path();
            if !db_path.exists() {
                eprintln!("No session database found. Nothing to delete.");
                return 0;
            }

            let db_url = format!("sqlite:{}", db_path.display());
            let pool = match sqlx::SqlitePool::connect(&db_url).await {
                Ok(p) => p,
                Err(e) => {
                    eprintln!("Failed to connect to database: {e}");
                    return 1;
                }
            };

            eprintln!("Deleting session: {session_id}");

            // Recursively collect child session IDs
            let all_ids = match collect_child_session_ids(&pool, session_id).await {
                Ok(ids) => ids,
                Err(e) => {
                    eprintln!("Failed to query session tree: {e}");
                    let _ = pool.close().await;
                    return 1;
                }
            };

            if all_ids.is_empty() {
                eprintln!("Session '{session_id}' not found.");
                let _ = pool.close().await;
                return 0;
            }

            for sid in &all_ids {
                // Delete parts
                if let Err(e) = sqlx::query("DELETE FROM part WHERE session_id = ?")
                    .bind(sid)
                    .execute(&pool)
                    .await
                {
                    eprintln!("Failed to delete parts for session {sid}: {e}");
                }

                // Delete messages
                if let Err(e) = sqlx::query("DELETE FROM message WHERE session_id = ?")
                    .bind(sid)
                    .execute(&pool)
                    .await
                {
                    eprintln!("Failed to delete messages for session {sid}: {e}");
                }
            }

            // Delete sessions (in reverse order — children first)
            for sid in all_ids.iter().rev() {
                if let Err(e) = sqlx::query("DELETE FROM session WHERE id = ?")
                    .bind(sid)
                    .execute(&pool)
                    .await
                {
                    eprintln!("Failed to delete session {sid}: {e}");
                }
            }

            let count = all_ids.len();
            if count == 1 {
                eprintln!("Session '{session_id}' deleted.");
            } else {
                eprintln!(
                    "Session '{session_id}' and {} child session(s) deleted.",
                    count - 1
                );
            }

            let _ = pool.close().await;
        }
    }

    0
}

/// Recursively collect all child session IDs for a given parent session ID.
async fn collect_child_session_ids(
    pool: &sqlx::SqlitePool,
    root_id: &str,
) -> Result<Vec<String>, sqlx::Error> {
    let mut all_ids = Vec::new();
    let _stack = [root_id.to_string()];

    // First verify root exists
    let exists: Option<String> = sqlx::query_scalar("SELECT id FROM session WHERE id = ?")
        .bind(root_id)
        .fetch_optional(pool)
        .await?;

    if exists.is_none() {
        return Ok(Vec::new());
    }

    all_ids.push(root_id.to_string());

    // Breadth-first collect children
    let mut idx = 0;
    while idx < all_ids.len() {
        let parent = &all_ids[idx];
        let children: Vec<String> =
            sqlx::query_scalar("SELECT id FROM session WHERE parent_id = ?")
                .bind(parent)
                .fetch_all(pool)
                .await?;
        all_ids.extend(children);
        idx += 1;
    }

    Ok(all_ids)
}

// ═════════════════════════════════════════════════════════════════════════════
// agent
// ═════════════════════════════════════════════════════════════════════════════

/// `agent` — Manage agents (create, list).
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
            let target_path = path
                .clone()
                .unwrap_or_else(|| ".opencode/agents".to_string());

            let desc = description.as_deref().unwrap_or("No description provided");

            let agent_mode = mode.as_deref().unwrap_or("all");
            let perm_list = permissions.as_deref().unwrap_or("all");

            eprintln!("Creating agent:");
            eprintln!("  Path:        {target_path}");
            eprintln!("  Description: {desc}");
            eprintln!("  Mode:        {agent_mode}");
            eprintln!("  Permissions: {perm_list}");
            if let Some(ref m) = model {
                eprintln!("  Model:       {m}");
            }

            // TS: Uses LLM to generate agent config, writes markdown file with
            // gray-matter frontmatter, configures permissions.
            eprintln!();
            eprintln!("For the full interactive agent creation experience, use `rustcode tui`");
            eprintln!("or `rustcode run --interactive` which provides LLM-powered generation.");
            eprintln!();
            eprintln!("Agent creation from CLI args will be implemented with LLM integration.");
            eprintln!("For now, manually create agent markdown files in:");
            eprintln!("  {}/agents/", target_path);
        }
        AgentCommand::List => {
            eprintln!("Listing available agents...");
            eprintln!();

            // TS: agent list — sorted by native first, then by name
            // In Rust, we scan ~/.config/opencode/agents/ and .opencode/agents/

            let global_agents = get_agents_dir_global();
            let local_agents = PathBuf::from(".opencode/agents");

            let mut found = false;

            for dir in &[&global_agents, &local_agents] {
                if dir.exists() {
                    if let Ok(entries) = std::fs::read_dir(dir) {
                        for entry in entries.flatten() {
                            let path = entry.path();
                            if path.extension().is_some_and(|e| e == "md") {
                                let name = path
                                    .file_stem()
                                    .map(|s| s.to_string_lossy().to_string())
                                    .unwrap_or_default();
                                // Try to read frontmatter for mode
                                let mode = read_agent_mode(&path);
                                println!("{name} ({mode})");
                                found = true;
                            }
                        }
                    }
                }
            }

            if !found {
                eprintln!("No agents found.");
                eprintln!();
                eprintln!("Agents are markdown files in:");
                eprintln!("  {}", global_agents.display());
                eprintln!("  .opencode/agents/");
                eprintln!();
                eprintln!("Create an agent with: rustcode agent create");
            }
        }
    }

    0
}

/// Get the global agents directory.
fn get_agents_dir_global() -> PathBuf {
    dirs::config_dir()
        .unwrap_or_else(|| PathBuf::from("."))
        .join("opencode")
        .join("agents")
}

/// Read the mode from an agent markdown file's YAML frontmatter.
fn read_agent_mode(path: &Path) -> String {
    match std::fs::read_to_string(path) {
        Ok(content) => {
            // Simple YAML frontmatter parser (--- ... ---)
            if let Some(rest) = content.strip_prefix("---") {
                if let Some(frontmatter) = rest.split("---").next() {
                    for line in frontmatter.lines() {
                        if let Some(val) = line.strip_prefix("mode:").map(str::trim) {
                            return val.to_string();
                        }
                    }
                }
            }
            "subagent".to_string()
        }
        Err(_) => "subagent".to_string(),
    }
}

// ═════════════════════════════════════════════════════════════════════════════
// providers
// ═════════════════════════════════════════════════════════════════════════════

/// `providers` — Manage AI provider credentials.
///
/// Ported from: `packages/opencode/src/cli/cmd/providers.ts`
async fn cmd_providers(cmd: &ProvidersCommand) -> i32 {
    match cmd {
        ProvidersCommand::List => {
            eprintln!();
            eprintln!("Configured providers and credentials:");
            eprintln!();

            // TS: Lists credentials from auth.json + active env vars
            let auth_path = dirs::data_dir()
                .unwrap_or_else(|| PathBuf::from("."))
                .join("opencode")
                .join("auth.json");

            let auth_display = shorten_path(&auth_path);
            eprintln!("Credentials ({auth_display})");

            // Check for stored credentials in auth.json
            if auth_path.exists() {
                if let Ok(contents) = std::fs::read_to_string(&auth_path) {
                    if let Ok(auth_map) =
                        serde_json::from_str::<HashMap<String, serde_json::Value>>(&contents)
                    {
                        for (provider_id, info) in &auth_map {
                            let auth_type = info
                                .get("type")
                                .and_then(|t| t.as_str())
                                .unwrap_or("unknown");
                            eprintln!("  {provider_id} ({auth_type})");
                        }
                        eprintln!("  {} credentials", auth_map.len());
                    } else {
                        eprintln!("  (empty or unparseable)");
                    }
                }
            } else {
                eprintln!("  No stored credentials found.");
            }

            // Show active environment variable providers
            eprintln!();
            eprintln!("Environment variables:");
            let env_checks = [
                ("anthropic", "ANTHROPIC_API_KEY"),
                ("openai", "OPENAI_API_KEY"),
                ("google", "GOOGLE_GENERATIVE_AI_API_KEY"),
                ("openrouter", "OPENROUTER_API_KEY"),
                ("deepseek", "DEEPSEEK_API_KEY"),
                ("groq", "GROQ_API_KEY"),
                ("xai", "XAI_API_KEY"),
                ("cerebras", "CEREBRAS_API_KEY"),
                ("mistral", "MISTRAL_API_KEY"),
                ("cohere", "COHERE_API_KEY"),
                ("perplexity", "PPLX_API_KEY"),
                ("together", "TOGETHER_API_KEY"),
                ("fireworks", "FIREWORKS_API_KEY"),
            ];

            let mut active_count = 0;
            for (provider, var) in &env_checks {
                if std::env::var(var).is_ok() {
                    eprintln!("  {provider} ({var})");
                    active_count += 1;
                }
            }
            eprintln!("  {active_count} active environment variable(s)");
        }
        ProvidersCommand::Login {
            url,
            provider,
            method,
        } => {
            // TS: Interactive login flow
            if let Some(auth_url) = url {
                eprintln!("Logging in via auth provider URL: {auth_url}");
                eprintln!("This would fetch {auth_url}/.well-known/opencode and run the");
                eprintln!("configured auth command, then store the resulting credential.");
                eprintln!("In the full implementation, this authenticates via the auth provider.");
            } else if let Some(provider_id) = provider {
                eprintln!("Logging in to provider: {provider_id}");
                eprint!("Enter API key for {provider_id}: ");
                let _ = std::io::stdout().flush();

                let mut api_key = String::new();
                match std::io::stdin().read_line(&mut api_key) {
                    Ok(_) => {
                        let api_key = api_key.trim().to_string();
                        if api_key.is_empty() {
                            eprintln!("No API key entered. Aborting.");
                            return 1;
                        }

                        // Build credential
                        let credential = serde_json::json!({
                            "type": "api_key",
                            "key": api_key,
                        });

                        // Read existing auth.json, merge, write back
                        let auth_path = dirs::data_dir()
                            .unwrap_or_else(|| PathBuf::from("."))
                            .join("opencode")
                            .join("auth.json");

                        let mut providers_map: HashMap<String, serde_json::Value> =
                            if auth_path.exists() {
                                std::fs::read_to_string(&auth_path)
                                    .ok()
                                    .and_then(|c| serde_json::from_str(&c).ok())
                                    .unwrap_or_default()
                            } else {
                                HashMap::new()
                            };

                        providers_map.insert(provider_id.clone(), credential);

                        if let Some(parent) = auth_path.parent() {
                            if !parent.as_os_str().is_empty() {
                                let _ = std::fs::create_dir_all(parent);
                            }
                        }

                        let wrapped = serde_json::json!({ "providers": providers_map });
                        match serde_json::to_string_pretty(&wrapped) {
                            Ok(json) => {
                                if let Err(e) = std::fs::write(&auth_path, &json) {
                                    eprintln!("Failed to write auth.json: {e}");
                                    return 1;
                                }
                                eprintln!(
                                    "Credential for '{provider_id}' saved to {}",
                                    shorten_path(&auth_path)
                                );
                                eprintln!("You can now use this provider in rustcode sessions.");
                            }
                            Err(e) => {
                                eprintln!("Failed to serialize auth data: {e}");
                                return 1;
                            }
                        }
                    }
                    Err(e) => {
                        eprintln!("Failed to read input: {e}");
                        return 1;
                    }
                }
            } else {
                let method_name = method.as_deref().unwrap_or("auto");
                eprintln!("Logging in (method: {method_name})");
                eprintln!();
                eprintln!("Interactive provider login:");
                eprintln!("  Use --provider to specify the provider ID and enter API key.");
                eprintln!();
                eprintln!("For now, set API keys via environment variables:");
                eprintln!("  export ANTHROPIC_API_KEY=sk-ant-...");
                eprintln!("  export OPENAI_API_KEY=sk-...");
            }
        }
        ProvidersCommand::Logout { provider } => {
            let provider_id = match provider {
                Some(p) => p.clone(),
                None => {
                    eprintln!("Please specify a provider to log out from:");
                    eprintln!("  rustcode providers logout <provider-id>");
                    return 1;
                }
            };

            let auth_path = dirs::data_dir()
                .unwrap_or_else(|| PathBuf::from("."))
                .join("opencode")
                .join("auth.json");

            if !auth_path.exists() {
                eprintln!("No auth.json found. Nothing to log out from.");
                return 0;
            }

            let contents = match std::fs::read_to_string(&auth_path) {
                Ok(c) => c,
                Err(e) => {
                    eprintln!("Failed to read auth.json: {e}");
                    return 1;
                }
            };

            let mut providers_map: HashMap<String, serde_json::Value> =
                serde_json::from_str(&contents).unwrap_or_default();

            if providers_map.remove(&provider_id).is_some() {
                let wrapped = serde_json::json!({ "providers": providers_map });
                match serde_json::to_string_pretty(&wrapped) {
                    Ok(json) => {
                        if let Err(e) = std::fs::write(&auth_path, &json) {
                            eprintln!("Failed to write auth.json: {e}");
                            return 1;
                        }
                        eprintln!("Logged out from '{provider_id}'.");
                        eprintln!("Removed credential from {}.", shorten_path(&auth_path));
                    }
                    Err(e) => {
                        eprintln!("Failed to serialize auth data: {e}");
                        return 1;
                    }
                }
            } else {
                eprintln!("No credential found for '{provider_id}' in auth.json.");
            }
            eprintln!("Environment variable credentials must be unset manually.");
        }
    }

    0
}

// ═════════════════════════════════════════════════════════════════════════════
// mcp
// ═════════════════════════════════════════════════════════════════════════════

// ── MCP helper functions ──────────────────────────────────────────────────

/// Generate a random UUID v4 as a hex string for OAuth state parameters.
fn uuid_v4_hex() -> String {
    uuid::Uuid::new_v4().as_simple().to_string()
}

/// Try to open a URL in the user's default browser.
///
/// Returns `true` if the browser was likely opened successfully.
fn try_open_browser(url: &str) -> bool {
    let (cmd, arg) = if cfg!(target_os = "linux") {
        ("xdg-open", url)
    } else if cfg!(target_os = "macos") {
        ("open", url)
    } else if cfg!(target_os = "windows") {
        ("cmd", url) // "start" is a cmd builtin
    } else {
        ("xdg-open", url)
    };

    match std::process::Command::new(cmd).arg(arg).spawn() {
        Ok(mut child) => {
            // Detach — we don't wait for the browser to exit
            let _ = child.stdin.take();
            true
        }
        Err(_) => false,
    }
}

/// Discover all MCP servers from configuration files that support OAuth.
///
/// Searches local project config (`opencode.json`, `opencode.jsonc`,
/// `.opencode/*`) and global config (`~/.config/opencode/opencode.jsonc`).
///
/// Returns a list of `(name, url, oauth_config)` tuples for remote MCP servers
/// with OAuth not explicitly disabled.
fn discover_oauth_mcp_servers() -> Vec<(
    Option<String>,
    String,
    serde_json::Map<String, serde_json::Value>,
)> {
    let mut servers = Vec::new();

    let candidates = [
        PathBuf::from("opencode.json"),
        PathBuf::from("opencode.jsonc"),
        PathBuf::from(".opencode/opencode.json"),
        PathBuf::from(".opencode/opencode.jsonc"),
    ];

    // Also check global config
    let global_config = dirs::config_dir()
        .unwrap_or_else(|| PathBuf::from("."))
        .join("opencode")
        .join("opencode.jsonc");

    let all_paths: Vec<PathBuf> = candidates
        .into_iter()
        .chain(std::iter::once(global_config))
        .filter(|p| p.exists())
        .collect();

    for config_path in &all_paths {
        if let Ok(contents) = std::fs::read_to_string(config_path) {
            let cleaned = if config_path.extension().is_some_and(|e| e == "jsonc") {
                rustcode_core::config::parse_jsonc(&contents, config_path).ok()
            } else {
                serde_json::from_str(&contents).ok()
            };

            if let Some(config) = cleaned {
                if let Some(mcp) = config.get("mcp").and_then(|m| m.as_object()) {
                    for (server_name, server) in mcp {
                        let connection_type =
                            server.get("type").and_then(|t| t.as_str()).unwrap_or("");
                        let server_url = server.get("url").and_then(|u| u.as_str()).unwrap_or("");

                        // Must be remote with a URL
                        if connection_type != "remote" || server_url.is_empty() {
                            continue;
                        }

                        // OAuth must not be explicitly disabled
                        let oauth_disabled = server
                            .get("oauth")
                            .map(|o| o.is_null() || o.as_bool() == Some(false))
                            .unwrap_or(false);

                        if oauth_disabled {
                            continue;
                        }

                        let oauth_config = server
                            .get("oauth")
                            .and_then(|o| o.as_object())
                            .cloned()
                            .unwrap_or_default();

                        servers.push((
                            Some(server_name.clone()),
                            server_url.to_string(),
                            oauth_config,
                        ));
                    }
                }
            }
        }
    }

    servers
}

/// Find a specific MCP server configuration by name.
///
/// Searches local project config and global config, returning the first match
/// along with the path of the config file where it was found.
fn find_mcp_server_config(name: &str) -> (Option<serde_json::Value>, Option<PathBuf>) {
    let candidates = [
        PathBuf::from("opencode.json"),
        PathBuf::from("opencode.jsonc"),
        PathBuf::from(".opencode/opencode.json"),
        PathBuf::from(".opencode/opencode.jsonc"),
    ];

    let global_config = dirs::config_dir()
        .unwrap_or_else(|| PathBuf::from("."))
        .join("opencode")
        .join("opencode.jsonc");

    let all_paths: Vec<PathBuf> = candidates
        .into_iter()
        .chain(std::iter::once(global_config))
        .filter(|p| p.exists())
        .collect();

    for config_path in &all_paths {
        if let Ok(contents) = std::fs::read_to_string(config_path) {
            let cleaned = if config_path.extension().is_some_and(|e| e == "jsonc") {
                rustcode_core::config::parse_jsonc(&contents, config_path).ok()
            } else {
                serde_json::from_str(&contents).ok()
            };

            if let Some(config) = cleaned {
                if let Some(mcp) = config.get("mcp").and_then(|m| m.as_object()) {
                    if let Some(server) = mcp.get(name) {
                        return (Some(server.clone()), Some(config_path.clone()));
                    }
                }
            }
        }
    }

    (None, None)
}

/// List all MCP server names and URLs across all config files.
///
/// Returns a list of `(name, url_or_command, type)` tuples.
fn list_all_mcp_servers() -> Vec<(String, String, String)> {
    let mut servers = Vec::new();

    let candidates = [
        PathBuf::from("opencode.json"),
        PathBuf::from("opencode.jsonc"),
        PathBuf::from(".opencode/opencode.json"),
        PathBuf::from(".opencode/opencode.jsonc"),
    ];

    let global_config = dirs::config_dir()
        .unwrap_or_else(|| PathBuf::from("."))
        .join("opencode")
        .join("opencode.jsonc");

    let all_paths: Vec<PathBuf> = candidates
        .into_iter()
        .chain(std::iter::once(global_config))
        .filter(|p| p.exists())
        .collect();

    let mut seen = std::collections::HashSet::new();

    for config_path in &all_paths {
        if let Ok(contents) = std::fs::read_to_string(config_path) {
            let cleaned = if config_path.extension().is_some_and(|e| e == "jsonc") {
                rustcode_core::config::parse_jsonc(&contents, config_path).ok()
            } else {
                serde_json::from_str(&contents).ok()
            };

            if let Some(config) = cleaned {
                if let Some(mcp) = config.get("mcp").and_then(|m| m.as_object()) {
                    for (server_name, server) in mcp {
                        if seen.contains(server_name) {
                            continue;
                        }
                        seen.insert(server_name.clone());

                        let connection_type = server
                            .get("type")
                            .and_then(|t| t.as_str())
                            .unwrap_or("unknown")
                            .to_string();
                        let detail = match connection_type.as_str() {
                            "remote" => server
                                .get("url")
                                .and_then(|u| u.as_str())
                                .unwrap_or("(no URL)")
                                .to_string(),
                            "local" => server
                                .get("command")
                                .and_then(|c| c.as_array())
                                .map(|a| {
                                    a.iter()
                                        .filter_map(|v| v.as_str())
                                        .collect::<Vec<_>>()
                                        .join(" ")
                                })
                                .unwrap_or_else(|| "(no command)".to_string()),
                            _ => "(unknown)".to_string(),
                        };
                        servers.push((server_name.clone(), detail, connection_type));
                    }
                }
            }
        }
    }

    servers
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
            let mcp_name = name.as_deref().unwrap_or("unnamed");

            // Parse --env KEY=VALUE pairs
            let env_vars: HashMap<String, String> = env
                .iter()
                .filter_map(|e| {
                    let mut parts = e.splitn(2, '=');
                    Some((parts.next()?.to_string(), parts.next()?.to_string()))
                })
                .collect();

            // Parse --header KEY=VALUE pairs
            let headers: HashMap<String, String> = header
                .iter()
                .filter_map(|h| {
                    let mut parts = h.splitn(2, '=');
                    Some((parts.next()?.to_string(), parts.next()?.to_string()))
                })
                .collect();

            // Build MCP config JSON
            let mcp_config = if let Some(server_url) = url {
                eprintln!("Adding remote MCP server '{mcp_name}' at {server_url}");
                let mut config = serde_json::json!({
                    "type": "remote",
                    "url": server_url,
                });
                if !headers.is_empty() {
                    config["headers"] = serde_json::to_value(&headers).unwrap_or_default();
                }
                config
            } else if !env_vars.is_empty() || !env.is_empty() {
                eprintln!("Adding local MCP server '{mcp_name}'");
                let mut config: serde_json::Map<String, serde_json::Value> = serde_json::Map::new();
                config.insert("type".into(), serde_json::Value::String("local".into()));
                if !env_vars.is_empty() {
                    config.insert(
                        "env".into(),
                        serde_json::to_value(&env_vars).unwrap_or_default(),
                    );
                }
                // If env args look like command args, use them as command
                let command: Vec<String> =
                    env.iter().filter(|e| !e.contains('=')).cloned().collect();
                if !command.is_empty() {
                    config.insert(
                        "command".into(),
                        serde_json::to_value(&command).unwrap_or_default(),
                    );
                }
                serde_json::Value::Object(config)
            } else {
                eprintln!("Adding MCP server '{mcp_name}'");
                eprintln!();
                eprintln!("In the full interactive implementation, this prompts for:");
                eprintln!("  - Server type (local command or remote URL)");
                eprintln!("  - Server URL or command");
                eprintln!("  - OAuth configuration (for remote servers)");
                eprintln!();
                eprintln!("The config is written to opencode.json in the current directory.");
                return 0;
            };

            // Read existing opencode.json from current directory
            let config_path = PathBuf::from("opencode.json");
            let mut config_root: serde_json::Map<String, serde_json::Value> =
                if config_path.exists() {
                    match std::fs::read_to_string(&config_path) {
                        Ok(contents) => match serde_json::from_str(&contents) {
                            Ok(val) => val,
                            Err(_) => serde_json::Map::new(),
                        },
                        Err(_) => serde_json::Map::new(),
                    }
                } else {
                    serde_json::Map::new()
                };

            // Add/update the mcp.<name> key
            let mcp_map = config_root
                .entry("mcp".to_string())
                .or_insert_with(|| serde_json::Value::Object(serde_json::Map::new()));

            if let Some(mcp_obj) = mcp_map.as_object_mut() {
                mcp_obj.insert(mcp_name.to_string(), mcp_config);
            }

            // Write back to opencode.json (clean JSON, not jsonc)
            match serde_json::to_string_pretty(&config_root) {
                Ok(json) => {
                    if let Err(e) = std::fs::write(&config_path, json) {
                        eprintln!("Failed to write opencode.json: {e}");
                        return 1;
                    }
                    eprintln!("MCP server '{mcp_name}' added to {}", config_path.display());
                    if !headers.is_empty() {
                        eprintln!("  Headers: {} configured", headers.len());
                    }
                    if !env_vars.is_empty() {
                        eprintln!("  Environment vars: {} configured", env_vars.len());
                    }
                }
                Err(e) => {
                    eprintln!("Failed to serialize config: {e}");
                    return 1;
                }
            }
        }
        McpCommand::List => {
            println!("MCP servers:");
            println!();
            println!(
                "{:<20} {:<10} {:<50} {:<10}",
                "Name", "Type", "URL/Command", "Status"
            );
            println!("{}", "\u{2500}".repeat(92));

            // Check opencode.json for MCP config
            let candidates = [
                PathBuf::from("opencode.json"),
                PathBuf::from("opencode.jsonc"),
                PathBuf::from(".opencode/opencode.json"),
                PathBuf::from(".opencode/opencode.jsonc"),
            ];

            let mut found_config = false;
            for candidate in &candidates {
                if candidate.exists() {
                    if let Ok(contents) = std::fs::read_to_string(candidate) {
                        // Handle jsonc by stripping comments
                        let cleaned = if candidate.extension().is_some_and(|e| e == "jsonc") {
                            rustcode_core::config::parse_jsonc(&contents, candidate).ok()
                        } else {
                            serde_json::from_str(&contents).ok()
                        };

                        if let Some(config) = cleaned {
                            if let Some(mcp) = config.get("mcp").and_then(|m| m.as_object()) {
                                for (name, server) in mcp {
                                    let connection_type = server
                                        .get("type")
                                        .and_then(|t| t.as_str())
                                        .unwrap_or("unknown");
                                    let detail = match connection_type {
                                        "remote" => server
                                            .get("url")
                                            .and_then(|u| u.as_str())
                                            .unwrap_or("no URL")
                                            .to_string(),
                                        "local" => server
                                            .get("command")
                                            .and_then(|c| c.as_array())
                                            .map(|a| {
                                                a.iter()
                                                    .filter_map(|v| v.as_str())
                                                    .collect::<Vec<_>>()
                                                    .join(" ")
                                            })
                                            .unwrap_or_else(|| "no command".to_string()),
                                        _ => "unknown config".to_string(),
                                    };

                                    // Compute status: check connectivity for remote servers
                                    // (In CLI mode, we check if the string looks valid)
                                    let status = match connection_type {
                                        "remote" => {
                                            if detail.contains("://") {
                                                "configured"
                                            } else {
                                                "invalid"
                                            }
                                        }
                                        "local" => "configured",
                                        _ => "unknown",
                                    };

                                    // Truncate for display
                                    let name_trunc = if name.len() > 19 {
                                        format!("{}...", &name[..16])
                                    } else {
                                        name.clone()
                                    };
                                    let detail_trunc = if detail.len() > 49 {
                                        format!("{}...", &detail[..46])
                                    } else {
                                        detail.clone()
                                    };
                                    println!(
                                        "{:<20} {:<10} {:<50} {:<10}",
                                        name_trunc, connection_type, detail_trunc, status
                                    );
                                    found_config = true;
                                }
                            }
                        }
                    }
                }
            }

            // Also check global config
            let global_config = dirs::config_dir()
                .unwrap_or_else(|| PathBuf::from("."))
                .join("opencode")
                .join("opencode.jsonc");
            if global_config.exists() {
                if let Ok(contents) = std::fs::read_to_string(&global_config) {
                    let cleaned =
                        rustcode_core::config::parse_jsonc(&contents, &global_config).ok();
                    if let Some(config) = cleaned {
                        if let Some(mcp) = config.get("mcp").and_then(|m| m.as_object()) {
                            for (name, server) in mcp {
                                let connection_type = server
                                    .get("type")
                                    .and_then(|t| t.as_str())
                                    .unwrap_or("unknown");
                                let detail = match connection_type {
                                    "remote" => server
                                        .get("url")
                                        .and_then(|u| u.as_str())
                                        .unwrap_or("no URL")
                                        .to_string(),
                                    "local" => "see global config".to_string(),
                                    _ => "unknown".to_string(),
                                };
                                let status = "configured";
                                let name_trunc = if name.len() > 19 {
                                    format!("{}...", &name[..16])
                                } else {
                                    name.clone()
                                };
                                let detail_trunc = if detail.len() > 49 {
                                    format!("{}...", &detail[..46])
                                } else {
                                    detail.clone()
                                };
                                println!(
                                    "{:<20} {:<10} {:<50} {:<10}",
                                    name_trunc, connection_type, detail_trunc, status
                                );
                                found_config = true;
                            }
                        }
                    }
                }
            }

            if !found_config {
                eprintln!("  No MCP servers configured.");
                eprintln!();
                eprintln!("Add a server with: rustcode mcp add <name> --url <url>");
                eprintln!("Or: rustcode mcp add <name> --env KEY=VALUE");
                eprintln!();
                eprintln!("Remote example:");
                eprintln!(
                    r#"  rustcode mcp add my-server --url https://example.com/mcp --header "Authorization:Bearer token""#
                );
                eprintln!();
                eprintln!("Local example (via config file):");
                eprintln!(r#"  rustcode mcp add my-tool --env "NODE_ENV=production""#);
            }
        }
        McpCommand::Auth { name } => {
            let data_dir = match rustcode_core::config::Config::data_dir() {
                Ok(d) => d,
                Err(e) => {
                    eprintln!("Error: cannot determine data directory: {e}");
                    return 1;
                }
            };
            let auth_path = data_dir.join("auth.json");

            // ── discover OAuth-capable MCP servers ───────────────────
            let oauth_servers = discover_oauth_mcp_servers();

            if oauth_servers.is_empty() {
                eprintln!("No OAuth-capable MCP servers found in configuration.");
                eprintln!();
                eprintln!("To configure an MCP server with OAuth, add it to opencode.json:");
                eprintln!(r#"  {{"mcp": {{"my-server": {{"type": "remote", "url": "https://...","#);
                eprintln!(r#"    "oauth": {{"client_id": "...", "scopes": "..."}} }} }} }}"#);
                eprintln!();
                eprintln!("Then run: rustcode mcp auth my-server");
                return 0;
            }

            // If name is specified, find that specific server
            let target_server = if let Some(server_name) = name {
                let found = oauth_servers
                    .iter()
                    .find(|(n, _, _)| n.as_deref() == Some(&server_name.clone()))
                    .cloned();
                match found {
                    Some(s) => {
                        eprintln!("MCP OAuth authentication for: {server_name}");
                        s
                    }
                    None => {
                        eprintln!(
                            "Error: MCP server '{server_name}' not found or not OAuth-capable."
                        );
                        eprintln!();
                        eprintln!("Available OAuth-capable MCP servers:");
                        for (n, url, oauth) in &oauth_servers {
                            let name_str = n.as_deref().unwrap_or("(unnamed)");
                            let scopes = oauth
                                .get("scope")
                                .and_then(|v| v.as_str())
                                .unwrap_or("(default)");
                            eprintln!("  - {name_str} ({url}) [scopes: {scopes}]");
                        }
                        return 1;
                    }
                }
            } else if oauth_servers.len() == 1 {
                let only = oauth_servers.into_iter().next().unwrap();
                eprintln!(
                    "MCP OAuth authentication for: {}",
                    only.0.as_deref().unwrap_or("(unnamed)")
                );
                only
            } else {
                // Multiple servers — list them
                eprintln!("Multiple OAuth-capable MCP servers found:");
                for (n, url, oauth) in &oauth_servers {
                    let name_str = n.as_deref().unwrap_or("(unnamed)");
                    let scopes = oauth
                        .get("scope")
                        .and_then(|v| v.as_str())
                        .unwrap_or("(default)");
                    eprintln!("  - {name_str} ({url}) [scopes: {scopes}]");
                }
                eprintln!();
                eprintln!("Specify which server to authenticate with:");
                eprintln!("  rustcode mcp auth <name>");
                return 0;
            };

            let (server_name_opt, server_url, oauth_config) = &target_server;
            let server_name = server_name_opt.as_deref().unwrap_or("unnamed");
            let client_id = oauth_config
                .get("client_id")
                .and_then(|v| v.as_str())
                .unwrap_or("");
            let client_secret = oauth_config.get("client_secret").and_then(|v| v.as_str());
            let scopes = oauth_config
                .get("scope")
                .and_then(|v| v.as_str())
                .unwrap_or("openid profile");

            eprintln!();
            eprintln!("OAuth Configuration:");
            eprintln!("  Server URL:  {server_url}");
            eprintln!(
                "  Client ID:   {}",
                if client_id.is_empty() {
                    "(not set — will attempt dynamic registration)"
                } else {
                    client_id
                }
            );
            if let Some(secret) = client_secret {
                let masked: String = secret
                    .chars()
                    .take(4)
                    .chain(std::iter::repeat_n('*', secret.len().saturating_sub(4)))
                    .collect();
                eprintln!("  Client Secret: {masked}");
            }
            eprintln!("  Scopes:      {scopes}");
            eprintln!();

            // ── check current auth status ────────────────────────────
            let auth_key = format!("mcp:{server_name}");
            let existing_auth = rustcode_core::config::Config::load_auth().ok();
            let is_authenticated = existing_auth
                .as_ref()
                .and_then(|auth| auth.get(&auth_key))
                .map(|creds| {
                    // Check if tokens exist and are not expired
                    let has_tokens = creds.get("access_token").is_some();
                    let not_expired = creds
                        .get("expires_at")
                        .and_then(|v| v.as_i64())
                        .map(|exp| {
                            let now = chrono::Utc::now().timestamp();
                            now < exp
                        })
                        .unwrap_or(true); // no expiration = assume valid
                    has_tokens && not_expired
                })
                .unwrap_or(false);

            if is_authenticated {
                eprintln!(
                    "Already authenticated. Tokens found in {}.",
                    auth_path.display()
                );
                eprintln!("To re-authenticate, first run:");
                eprintln!("  rustcode mcp logout {server_name}");
                eprintln!("Then run `rustcode mcp auth {server_name}` again.");
                return 0;
            }

            // Check for expired tokens
            let has_expired = existing_auth
                .as_ref()
                .and_then(|auth| auth.get(&auth_key))
                .map(|creds| {
                    creds.get("access_token").is_some()
                        && creds
                            .get("expires_at")
                            .and_then(|v| v.as_i64())
                            .map(|exp| {
                                let now = chrono::Utc::now().timestamp();
                                now >= exp
                            })
                            .unwrap_or(false)
                })
                .unwrap_or(false);

            if has_expired {
                eprintln!("Existing credentials have expired.");
                eprintln!("Proceeding with re-authentication...");
                eprintln!();
            }

            // ── initiate OAuth flow ──────────────────────────────────
            eprintln!("Initiating OAuth flow...");
            eprintln!();

            // Build the authorization URL
            let redirect_uri = oauth_config
                .get("redirect_uri")
                .and_then(|v| v.as_str())
                .unwrap_or("http://localhost:0/callback");

            let state = uuid_v4_hex();

            // Try opening browser for auth
            let auth_url = if client_id.is_empty() {
                // Dynamic client registration needed
                eprintln!("Client ID not configured. Dynamic client registration is required.");
                eprintln!();
                eprintln!("In the full implementation, this would:");
                eprintln!("  1. Call the MCP server's registration endpoint");
                eprintln!("  2. Obtain a client_id and client_secret");
                eprintln!("  3. Proceed with the OAuth authorization code flow");
                eprintln!();
                eprintln!("For now, configure a client_id in opencode.json:");
                eprintln!(
                    r#"  {{"mcp": {{"{}": {{..., "oauth": {{"client_id": "your-client-id"}}}}}} }}"#,
                    server_name
                );
                return 0;
            } else {
                format!(
                    "{server_url}/authorize\
                     ?response_type=code\
                     &client_id={client_id}\
                     &redirect_uri={redirect_uri}\
                     &scope={scopes}\
                     &state={state}"
                )
            };

            eprintln!("Opening browser for authorization...");
            eprintln!("  URL: {auth_url}");
            eprintln!();

            if try_open_browser(&auth_url) {
                eprintln!("Browser opened successfully.");
                eprintln!();
                eprintln!("Complete the authorization in your browser.");
                eprintln!("After authorization, the callback will be handled automatically");
                eprintln!("if a local HTTP server is listening on the redirect URI port.");
                eprintln!();
            } else {
                eprintln!("Could not open browser automatically.");
                eprintln!();
                eprintln!("Please open this URL manually:");
                eprintln!("  {auth_url}");
            }

            eprintln!("In the full implementation, this would:");
            eprintln!("  1. Start a local HTTP server to receive the OAuth callback");
            eprintln!("  2. Exchange the authorization code for tokens");
            eprintln!(
                "  3. Store access/refresh tokens in {}",
                auth_path.display()
            );
            eprintln!("  4. Print a success message");
            eprintln!();
            eprintln!("For now, you can manually configure tokens by setting env vars");
            eprintln!("or editing the MCP server headers in opencode.json directly.");
        }
        McpCommand::Logout { name } => {
            let data_dir = match rustcode_core::config::Config::data_dir() {
                Ok(d) => d,
                Err(e) => {
                    eprintln!("Error: cannot determine data directory: {e}");
                    return 1;
                }
            };
            let auth_path = data_dir.join("auth.json");

            if let Some(server_name) = name {
                let auth_key = format!("mcp:{server_name}");
                eprintln!("Removing OAuth credentials for MCP server: {server_name}");
                eprintln!("  Auth key: {auth_key}");
                eprintln!();

                // Check current auth state
                let existing_auth = rustcode_core::config::Config::load_auth().ok();
                let has_creds = existing_auth
                    .as_ref()
                    .map(|auth| auth.contains_key(&auth_key))
                    .unwrap_or(false);

                if !has_creds {
                    eprintln!("No stored credentials found for '{server_name}'.");
                    eprintln!();
                    eprintln!("Available stored credentials:");
                    if let Some(auth) = existing_auth {
                        let mcp_creds: Vec<_> =
                            auth.keys().filter(|k| k.starts_with("mcp:")).collect();
                        if mcp_creds.is_empty() {
                            eprintln!("  (none)");
                        } else {
                            for k in mcp_creds {
                                let display_name = k.strip_prefix("mcp:").unwrap_or(k);
                                eprintln!("  - {display_name}");
                            }
                        }
                    } else {
                        eprintln!("  (none)");
                    }
                    return 0;
                }

                // Show what is being removed
                if let Some(ref auth) = existing_auth {
                    if let Some(creds) = auth.get(&auth_key) {
                        let token_types: Vec<&str> = [
                            ("access_token", "access token"),
                            ("refresh_token", "refresh token"),
                            ("id_token", "ID token"),
                            ("client_id", "client registration"),
                        ]
                        .iter()
                        .filter(|(k, _)| creds.get(*k).is_some())
                        .map(|(_, label)| *label)
                        .collect();

                        if !token_types.is_empty() {
                            eprintln!("Removing: {}", token_types.join(", "));
                        }

                        if let Some(exp) = creds.get("expires_at").and_then(|v| v.as_i64()) {
                            let expiry = chrono::DateTime::from_timestamp(exp, 0)
                                .map(|dt| dt.format("%Y-%m-%d %H:%M:%S UTC").to_string())
                                .unwrap_or_else(|| "unknown".to_string());
                            eprintln!("  Expiration: {expiry}");
                        }
                    }
                }

                match rustcode_core::config::Config::remove_auth(&auth_key) {
                    Ok(()) => {
                        eprintln!();
                        eprintln!("Credentials removed successfully.");
                        eprintln!("  Removed from: {}", auth_path.display());
                    }
                    Err(e) => {
                        eprintln!("Error removing credentials: {e}");
                        return 1;
                    }
                }
            } else {
                // No name provided — remove ALL MCP credentials
                let existing_auth = match rustcode_core::config::Config::load_auth() {
                    Ok(auth) => auth,
                    Err(e) => {
                        eprintln!("Error reading auth: {e}");
                        return 1;
                    }
                };

                let mcp_keys: Vec<String> = existing_auth
                    .keys()
                    .filter(|k| k.starts_with("mcp:"))
                    .cloned()
                    .collect();

                if mcp_keys.is_empty() {
                    eprintln!("No MCP OAuth credentials found in {}.", auth_path.display());
                    return 0;
                }

                eprintln!("Removing all MCP OAuth credentials:");
                for key in &mcp_keys {
                    let display_name = key.strip_prefix("mcp:").unwrap_or(key);
                    eprintln!("  - {display_name}");
                }
                eprintln!();

                let mut errors = 0;
                for key in &mcp_keys {
                    match rustcode_core::config::Config::remove_auth(key) {
                        Ok(()) => {
                            let display_name = key.strip_prefix("mcp:").unwrap_or(key);
                            eprintln!("  Removed: {display_name}");
                        }
                        Err(e) => {
                            eprintln!("  Error removing {}: {e}", key);
                            errors += 1;
                        }
                    }
                }

                if errors > 0 {
                    eprintln!();
                    eprintln!("{errors} removal(s) failed.");
                    return 1;
                }

                eprintln!();
                eprintln!("All MCP credentials removed from {}.", auth_path.display());
            }
        }
        McpCommand::Debug { name } => {
            eprintln!("MCP OAuth Debug: {name}");
            eprintln!();

            // ── 1. Find server config ────────────────────────────────
            let (server_config, config_source) = find_mcp_server_config(name);

            let server_config = match server_config {
                Some(c) => c,
                None => {
                    eprintln!("Error: MCP server '{name}' not found in config.");
                    eprintln!();
                    eprintln!("Searched in: opencode.json, opencode.jsonc,");
                    eprintln!("  .opencode/opencode.json, .opencode/opencode.jsonc,");
                    eprintln!("  and global config (~/.config/opencode/opencode.jsonc).");
                    eprintln!();
                    eprintln!("Available servers:");
                    let all_servers = list_all_mcp_servers();
                    if all_servers.is_empty() {
                        eprintln!("  (none configured)");
                        eprintln!();
                        eprintln!("Add a server with: rustcode mcp add {name} --url <url>");
                    } else {
                        for (n, url, mcp_type) in &all_servers {
                            eprintln!("  - {n} ({mcp_type}) -> {url}");
                        }
                    }
                    return 1;
                }
            };

            let is_remote = server_config
                .get("type")
                .and_then(|v| v.as_str())
                .map(|t| t == "remote")
                .unwrap_or(false);

            // ── Show server config ───────────────────────────────────
            eprintln!("━━━ Server Configuration ━━━");
            eprintln!("  Name:    {name}");
            if let Some(source) = &config_source {
                eprintln!("  Source:  {}", source.display());
            }

            let server_url = server_config
                .get("url")
                .and_then(|v| v.as_str())
                .unwrap_or("(not set)");
            eprintln!("  URL:     {server_url}");

            let server_type = server_config
                .get("type")
                .and_then(|v| v.as_str())
                .unwrap_or("unknown");
            eprintln!("  Type:    {server_type}");

            if let Some(headers) = server_config.get("headers").and_then(|v| v.as_object()) {
                eprintln!("  Headers: {} configured", headers.len());
                for (key, val) in headers {
                    let masked = if key.to_lowercase().contains("auth")
                        || key.to_lowercase().contains("token")
                    {
                        let s = val.as_str().unwrap_or("");
                        if s.len() > 8 {
                            format!("{}...{}", &s[..4], &s[s.len() - 4..])
                        } else {
                            "***".to_string()
                        }
                    } else {
                        val.as_str().unwrap_or("?").to_string()
                    };
                    eprintln!("    {key}: {masked}");
                }
            }

            let oauth_enabled = server_config
                .get("oauth")
                .map(|o| !o.is_null() && o.as_object().map(|obj| !obj.is_empty()).unwrap_or(false))
                .unwrap_or(false);
            eprintln!(
                "  OAuth:   {}",
                if oauth_enabled {
                    "enabled"
                } else {
                    "disabled / not configured"
                }
            );

            if let Some(oauth) = server_config.get("oauth").and_then(|v| v.as_object()) {
                if let Some(cid) = oauth.get("client_id").and_then(|v| v.as_str()) {
                    eprintln!("    client_id: {cid}");
                }
                if let Some(sc) = oauth
                    .get("scopes")
                    .or_else(|| oauth.get("scope"))
                    .and_then(|v| v.as_str())
                {
                    eprintln!("    scopes: {sc}");
                }
                if let Some(ru) = oauth.get("redirect_uri").and_then(|v| v.as_str()) {
                    eprintln!("    redirect_uri: {ru}");
                }
            }

            eprintln!();

            // ── 2. Check stored tokens ───────────────────────────────
            eprintln!("━━━ Stored Credentials ━━━");
            let auth_key = format!("mcp:{name}");
            match rustcode_core::config::Config::load_auth() {
                Ok(auth) => {
                    if let Some(creds) = auth.get(&auth_key) {
                        eprintln!("  Status: credentials found");

                        // Access token preview
                        if let Some(token) = creds.get("access_token").and_then(|v| v.as_str()) {
                            let preview: String = token.chars().take(20).collect();
                            eprintln!("  Access Token: {preview}...");
                        } else {
                            eprintln!("  Access Token: (not present)");
                        }

                        // Refresh token
                        if creds.get("refresh_token").is_some() {
                            eprintln!("  Refresh Token: present");
                        } else {
                            eprintln!("  Refresh Token: (not present)");
                        }

                        // Expiration
                        if let Some(exp) = creds.get("expires_at").and_then(|v| v.as_i64()) {
                            let now = chrono::Utc::now().timestamp();
                            let expiry_dt = chrono::DateTime::from_timestamp(exp, 0)
                                .map(|dt| dt.format("%Y-%m-%d %H:%M:%S UTC").to_string())
                                .unwrap_or_else(|| "unknown".to_string());
                            if now >= exp {
                                eprintln!("  Expiration:  {expiry_dt} [EXPIRED]");
                            } else {
                                let remaining = exp - now;
                                let hours = remaining / 3600;
                                let mins = (remaining % 3600) / 60;
                                eprintln!(
                                    "  Expiration:  {expiry_dt} (valid for {hours}h {mins}m)"
                                );
                            }
                        } else {
                            eprintln!("  Expiration:  (not set)");
                        }

                        // Client ID (registration)
                        if let Some(cid) = creds.get("client_id").and_then(|v| v.as_str()) {
                            eprintln!("  Client Registration: {cid}");
                        }

                        // Additional metadata
                        if let Some(meta) = creds.get("metadata") {
                            if let Some(obj) = meta.as_object() {
                                eprintln!("  Metadata: {} fields", obj.len());
                                for (k, v) in obj {
                                    if let Some(s) = v.as_str() {
                                        eprintln!("    {k}: {s}");
                                    }
                                }
                            }
                        }
                    } else {
                        eprintln!("  Status: no credentials stored for this server");
                        eprintln!("  Auth key: {auth_key}");
                        eprintln!();
                        eprintln!("  Run `rustcode mcp auth {name}` to authenticate.");
                    }
                }
                Err(e) => {
                    eprintln!("  Error reading auth: {e}");
                }
            }

            eprintln!();

            // ── 3. Test HTTP connectivity ────────────────────────────
            if !is_remote {
                eprintln!("━━━ Connectivity ━━━");
                eprintln!("  Server type is 'local' — HTTP connectivity test skipped.");
                eprintln!();
                eprintln!("  Local MCP servers are started as subprocesses.");
                eprintln!("  To test: run the command manually and verify it accepts stdin/stdout JSON-RPC.");
                eprintln!();
                return 0;
            }

            eprintln!("━━━ HTTP Connectivity ━━━");
            eprintln!("  Testing connection to {server_url}...");

            // Try a basic HTTP reachability check first
            let client = reqwest::Client::builder()
                .timeout(std::time::Duration::from_secs(10))
                .build()
                .ok();

            if let Some(ref http_client) = client {
                match http_client.head(server_url).send().await {
                    Ok(resp) => {
                        eprintln!("  HTTP Status: {}", resp.status());
                        if let Some(server_hdr) = resp.headers().get("server") {
                            eprintln!("  Server: {}", server_hdr.to_str().unwrap_or("(binary)"));
                        }
                    }
                    Err(e) => {
                        eprintln!("  HTTP check failed: {e}");
                        eprintln!("  The server may still accept POST requests — continuing...");
                    }
                }
            }

            // ── 4. Validate server via initialize request ────────────
            eprintln!();
            eprintln!("━━━ MCP Initialize ━━━");

            let initialize_payload = serde_json::json!({
                "jsonrpc": "2.0",
                "method": "initialize",
                "params": {
                    "protocolVersion": "2024-11-05",
                    "capabilities": {},
                    "clientInfo": {
                        "name": "rustcode-mcp-debug",
                        "version": env!("CARGO_PKG_VERSION")
                    }
                },
                "id": 1
            });

            if let Some(ref http_client) = client {
                // Add auth headers from config or stored tokens
                let mut req = http_client
                    .post(server_url)
                    .header("Content-Type", "application/json")
                    .json(&initialize_payload);

                // Add configured headers
                if let Some(headers) = server_config.get("headers").and_then(|v| v.as_object()) {
                    for (key, val) in headers {
                        if let Some(v) = val.as_str() {
                            req = req.header(key.as_str(), v);
                        }
                    }
                }

                // If OAuth token is stored, use it
                if let Ok(auth) = rustcode_core::config::Config::load_auth() {
                    if let Some(creds) = auth.get(&auth_key) {
                        if let Some(token) = creds.get("access_token").and_then(|v| v.as_str()) {
                            req = req.header("Authorization", format!("Bearer {token}"));
                            eprintln!("  Using stored access token for authentication.");
                        }
                    }
                }

                match req.send().await {
                    Ok(resp) => {
                        let status = resp.status();
                        let www_auth = resp
                            .headers()
                            .get("www-authenticate")
                            .and_then(|v| v.to_str().ok())
                            .map(|v| v.to_string());
                        eprintln!("  Response status: {status}");

                        match resp.text().await {
                            Ok(body) => {
                                // Parse the JSON-RPC response
                                if let Ok(json_body) =
                                    serde_json::from_str::<serde_json::Value>(&body)
                                {
                                    if let Some(error) = json_body.get("error") {
                                        let code = error
                                            .get("code")
                                            .and_then(|v| v.as_i64())
                                            .unwrap_or(-1);
                                        let msg = error
                                            .get("message")
                                            .and_then(|v| v.as_str())
                                            .unwrap_or("unknown");
                                        eprintln!("  JSON-RPC Error: [{code}] {msg}");

                                        // 401 / 403 → OAuth likely needed
                                        if status == reqwest::StatusCode::UNAUTHORIZED
                                            || status == reqwest::StatusCode::FORBIDDEN
                                        {
                                            eprintln!();
                                            eprintln!("  ━━━ OAuth Diagnostics ━━━");
                                            eprintln!("  The server returned {status}. This suggests authentication is required.");
                                            eprintln!();

                                            // Check WWW-Authenticate header
                                            if let Some(ref auth_val) = www_auth {
                                                eprintln!("  WWW-Authenticate: {auth_val}",);
                                            }

                                            if oauth_enabled {
                                                eprintln!("  OAuth IS configured for this server.");
                                                eprintln!();
                                                eprintln!("  Possible issues:");
                                                eprintln!("    1. Token expired — run `rustcode mcp auth {name}` to refresh");
                                                eprintln!("    2. Client not registered — check client_id in opencode.json");
                                                eprintln!("    3. Scopes insufficient — verify scopes in oauth config");
                                            } else {
                                                eprintln!(
                                                    "  OAuth is NOT configured for this server."
                                                );
                                                eprintln!();
                                                eprintln!(
                                                    "  To enable OAuth, add to opencode.json:"
                                                );
                                                eprintln!(
                                                    r#"    "oauth": {{"client_id": "...", "scopes": "..."}}"#
                                                );
                                            }

                                            if let Some(data) = error.get("data") {
                                                if let Some(details) = data.as_str() {
                                                    eprintln!();
                                                    eprintln!("  Error details: {details}");
    }
}

// ═════════════════════════════════════════════════════════════════════════════
// completion
// ═════════════════════════════════════════════════════════════════════════════

/// `completion` — Generate shell completion scripts.
///
/// Ported from: standard `clap_complete` convention.
/// Supports bash, fish, zsh, and powershell.
async fn cmd_completion(args: &CompletionArgs) -> i32 {
    use clap::CommandFactory;
    use clap_complete::{generate, Shell};
    let shell = match args.shell.as_str() {
        "bash" => Shell::Bash,
        "fish" => Shell::Fish,
        "zsh" => Shell::Zsh,
        "powershell" => Shell::PowerShell,
        other => {
            eprintln!("Unsupported shell: {other}");
            eprintln!("Supported shells: bash, fish, zsh, powershell");
            return 1;
        }
    };
    let mut cmd = Cli::command();
    let name = cmd.get_name().to_string();
    generate(shell, &mut cmd, name, &mut std::io::stdout());
    0
}

                                        }
                                    } else if let Some(result) = json_body.get("result") {
                                        eprintln!("  ✓ Initialize succeeded!");
                                        if let Some(si) = result.get("serverInfo") {
                                            let sname = si
                                                .get("name")
                                                .and_then(|v| v.as_str())
                                                .unwrap_or("?");
                                            let sver = si
                                                .get("version")
                                                .and_then(|v| v.as_str())
                                                .unwrap_or("?");
                                            eprintln!("    Server: {sname} v{sver}");
                                        }
                                        if let Some(caps) = result.get("capabilities") {
                                            if let Some(obj) = caps.as_object() {
                                                eprintln!(
                                                    "    Capabilities: {}",
                                                    obj.keys()
                                                        .map(|k| k.as_str())
                                                        .collect::<Vec<_>>()
                                                        .join(", ")
                                                );
                                            }
                                        }
                                        if let Some(prot) = result.get("protocolVersion") {
                                            eprintln!("    Protocol: {prot}");
                                        }

                                        eprintln!();
                                        eprintln!(
                                            "  Server is operational and responding correctly."
                                        );
                                    }
                                } else {
                                    eprintln!("  Response (raw, first 500 chars):");
                                    let preview: String = body.chars().take(500).collect();
                                    eprintln!("    {preview}");
                                    if body.len() > 500 {
                                        eprintln!("    ... ({} more chars)", body.len() - 500);
                                    }
                                }
                            }
                            Err(e) => {
                                eprintln!("  Error reading response body: {e}");
                            }
                        }
                    }
                    Err(e) => {
                        eprintln!("  Connection failed: {e}");
                        eprintln!();
                        eprintln!("  Troubleshooting:");
                        eprintln!("    1. Verify the server URL is correct");
                        eprintln!("    2. Check the server is running");
                        eprintln!("    3. Verify network/firewall allows the connection");
                        if e.is_timeout() {
                            eprintln!(
                                "    4. Connection timed out — increase timeout or check network"
                            );
                        }
                        if e.is_connect() {
                            eprintln!("    4. Connection refused — server may not be listening on this URL");
                        }
                    }
                }
            } else {
                eprintln!("  Error: could not create HTTP client.");
            }

            eprintln!();
            eprintln!("━━━ Debug Summary ━━━");
            eprintln!(
                "  Config:  {}",
                if server_url.is_empty() || server_url == "(not set)" {
                    "MISSING URL"
                } else {
                    "loaded"
                }
            );
            let auth_state = rustcode_core::config::Config::load_auth()
                .ok()
                .and_then(|a| a.get(&auth_key).cloned());
            match auth_state {
                Some(ref creds) => {
                    let has_token = creds.get("access_token").is_some();
                    let expired = creds
                        .get("expires_at")
                        .and_then(|v| v.as_i64())
                        .map(|exp| chrono::Utc::now().timestamp() >= exp)
                        .unwrap_or(false);
                    if expired {
                        eprintln!("  Auth:    credentials present but EXPIRED — run `rustcode mcp auth {name}`");
                    } else if has_token {
                        eprintln!("  Auth:    authenticated");
                    } else {
                        eprintln!("  Auth:    credentials found but no access token");
                    }
                }
                None => {
                    if oauth_enabled {
                        eprintln!("  Auth:    not authenticated — run `rustcode mcp auth {name}`");
                    } else {
                        eprintln!("  Auth:    N/A (OAuth not configured)");
                    }
                }
            }
        }
    }

    0
}

// ═════════════════════════════════════════════════════════════════════════════
// acp
// ═════════════════════════════════════════════════════════════════════════════

/// `acp` — Start ACP (Agent Client Protocol) server.
///
/// Starts an internal HTTP server and bridges ACP messages over
/// newline-delimited JSON (NDJSON) on stdin/stdout.
///
/// Ported from: `packages/opencode/src/cli/cmd/acp.ts`
async fn cmd_acp(args: &AcpArgs, config: &rustcode_core::config::Info) -> i32 {
    let cwd = args.cwd.clone().unwrap_or_else(|| {
        std::env::current_dir()
            .map(|p| p.display().to_string())
            .unwrap_or_else(|_| ".".into())
    });

    let hostname = args.network.hostname.clone();
    let port = args.network.port;

    eprintln!("ACP (Agent Client Protocol) server");
    eprintln!("  CWD:      {cwd}");
    eprintln!();

    // Set the OPENCODE_CLIENT environment variable
    std::env::set_var("OPENCODE_CLIENT", "acp");

    // Initialize the runtime
    let ctx = match rustcode_core::runtime::initialize_runtime(config) {
        Ok(ctx) => ctx,
        Err(e) => {
            eprintln!("Failed to initialize runtime: {e}");
            return 1;
        }
    };

    if ctx.providers.is_empty() {
        eprintln!("No LLM providers configured. Set an API key environment variable.");
        return 1;
    }

    eprintln!("Providers: {}", ctx.providers.len());
    eprintln!("Starting ACP server...");

    // Start the internal HTTP server on a random available port
    let server_port = if port == 0 {
        // Find an available port
        let listener = match std::net::TcpListener::bind("127.0.0.1:0") {
            Ok(l) => l,
            Err(e) => {
                eprintln!("Failed to find available port: {e}");
                return 1;
            }
        };
        listener.local_addr().unwrap().port()
    } else {
        port
    };

    let server_url = format!("http://{hostname}:{server_port}");
    eprintln!("Internal server: {server_url}");

    // Build server state
    let state = build_server_state(&ctx);

    // Start the server in a background task
    let state_clone = state.clone();
    let server_config = rustcode_server::ServerConfig {
        hostname: hostname.clone(),
        port: server_port,
        cors_origins: None,
    };
    let server_handle = tokio::spawn(async move {
        if let Err(e) = rustcode_server::serve(state_clone, server_config).await {
            eprintln!("Server error: {e}");
        }
    });

    // Give the server a moment to start
    tokio::time::sleep(std::time::Duration::from_millis(100)).await;
    eprintln!("ACP server ready.");

    // Create an SDK client pointing at the local server
    let sdk_client = match reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(30))
        .build()
    {
        Ok(c) => c,
        Err(e) => {
            eprintln!("Failed to create HTTP client: {e}");
            server_handle.abort();
            return 1;
        }
    };

    // NDJSON transport over stdin/stdout
    let stdin = tokio::io::stdin();
    let stdout = tokio::io::stdout();
    let mut reader = tokio::io::BufReader::new(stdin);
    let mut writer = tokio::io::BufWriter::new(stdout);
    use tokio::io::{AsyncBufReadExt, AsyncWriteExt};

    eprintln!("ACP transport ready (NDJSON over stdin/stdout).");

    // Main ACP message loop
    let mut line = String::new();
    loop {
        line.clear();
        match reader.read_line(&mut line).await {
            Ok(0) => {
                // EOF — client disconnected
                eprintln!("Client disconnected (stdin closed).");
                break;
            }
            Ok(_) => {}
            Err(e) => {
                eprintln!("Failed to read from stdin: {e}");
                break;
            }
        }

        let trimmed = line.trim();
        if trimmed.is_empty() {
            continue;
        }

        // Parse the incoming message
        let msg: serde_json::Value = match serde_json::from_str(trimmed) {
            Ok(v) => v,
            Err(e) => {
                eprintln!("Failed to parse message: {e}");
                let err_response = serde_json::json!({
                    "error": {
                        "code": -32700,
                        "message": format!("Parse error: {e}")
                    }
                });
                let mut err_line = serde_json::to_string(&err_response).unwrap_or_default();
                err_line.push('\n');
                let _ = writer.write_all(err_line.as_bytes()).await;
                let _ = writer.flush().await;
                continue;
            }
        };

        // Extract message type and method
        let msg_type = msg["type"].as_str().unwrap_or("");
        let method = msg["method"].as_str().unwrap_or("");
        let id = msg["id"].clone();

        // Handle the message
        let response = match msg_type {
            "request" => handle_acp_request(method, &msg, &server_url, &sdk_client, &ctx).await,
            "notification" => {
                // Notifications don't get responses
                handle_acp_notification(method, &msg, &server_url, &sdk_client).await;
                continue;
            }
            _ => {
                serde_json::json!({
                    "error": {
                        "code": -32600,
                        "message": format!("Unknown message type: {msg_type}")
                    }
                })
            }
        };

        // Build the response with the original id
        let mut response_with_id = response;
        if !id.is_null() {
            response_with_id["id"] = id;
        }

        // Send response
        let mut resp_line = serde_json::to_string(&response_with_id).unwrap_or_default();
        resp_line.push('\n');
        if let Err(e) = writer.write_all(resp_line.as_bytes()).await {
            eprintln!("Failed to write response: {e}");
            break;
        }
        if let Err(e) = writer.flush().await {
            eprintln!("Failed to flush response: {e}");
            break;
        }
    }

    server_handle.abort();
    eprintln!("ACP server shut down.");
    0
}

/// Handle an ACP request message.
async fn handle_acp_request(
    method: &str,
    msg: &serde_json::Value,
    server_url: &str,
    client: &reqwest::Client,
    _ctx: &rustcode_core::runtime::RuntimeContext,
) -> serde_json::Value {
    let params = msg["params"].as_object().cloned().unwrap_or_default();

    match method {
        "initialize" => {
            serde_json::json!({
                "result": {
                    "protocolVersion": "1",
                    "capabilities": {
                        "tools": { "listChanged": false },
                        "logging": {}
                    },
                    "serverInfo": {
                        "name": "rustcode",
                        "version": "0.1.0"
                    }
                }
            })
        }
        "authenticate" => {
            // No-op authentication — local server handles auth
            serde_json::json!({
                "result": {}
            })
        }
        "newSession" => {
            let directory = params["directory"]
                .as_str()
                .map(|s| s.to_string())
                .unwrap_or_else(|| {
                    std::env::current_dir()
                        .map(|p| p.display().to_string())
                        .unwrap_or_else(|_| ".".into())
                });

            // Create a new session via the server
            let session_url = format!("{server_url}/session");
            let resp = client
                .post(&session_url)
                .json(&serde_json::json!({
                    "directory": directory
                }))
                .send()
                .await;

            match resp {
                Ok(r) => {
                    let body: serde_json::Value = r.json().await.unwrap_or_default();
                    let session_id = body["id"].as_str().unwrap_or("").to_string();
                    serde_json::json!({
                        "result": {
                            "sessionId": session_id,
                            "directory": directory
                        }
                    })
                }
                Err(e) => {
                    serde_json::json!({
                        "error": {
                            "code": -32000,
                            "message": format!("Failed to create session: {e}")
                        }
                    })
                }
            }
        }
        "loadSession" => {
            let session_id = params["sessionId"].as_str().unwrap_or("");
            if session_id.is_empty() {
                return serde_json::json!({
                    "error": {
                        "code": -32602,
                        "message": "Missing sessionId parameter"
                    }
                });
            }

            let session_url = format!("{server_url}/session/{session_id}");
            let resp = client.get(&session_url).send().await;

            match resp {
                Ok(r) => {
                    let body: serde_json::Value = r.json().await.unwrap_or_default();
                    serde_json::json!({
                        "result": body
                    })
                }
                Err(e) => {
                    serde_json::json!({
                        "error": {
                            "code": -32000,
                            "message": format!("Failed to load session: {e}")
                        }
                    })
                }
            }
        }
        "listSessions" => {
            let limit = params["limit"].as_u64().unwrap_or(100);
            let cursor = params["cursor"].as_str().unwrap_or("");

            let mut url = format!("{server_url}/session?limit={limit}");
            if !cursor.is_empty() {
                url.push_str(&format!("&cursor={cursor}"));
            }

            let resp = client.get(&url).send().await;

            match resp {
                Ok(r) => {
                    let body: serde_json::Value = r.json().await.unwrap_or_default();
                    serde_json::json!({
                        "result": body
                    })
                }
                Err(e) => {
                    serde_json::json!({
                        "error": {
                            "code": -32000,
                            "message": format!("Failed to list sessions: {e}")
                        }
                    })
                }
            }
        }
        "prompt" => {
            let session_id = params["sessionId"].as_str().unwrap_or("");
            let prompt = params["prompt"].as_str().unwrap_or("");

            if session_id.is_empty() || prompt.is_empty() {
                return serde_json::json!({
                    "error": {
                        "code": -32602,
                        "message": "Missing sessionId or prompt parameter"
                    }
                });
            }

            // Send prompt via the server
            let prompt_url = format!("{server_url}/session/{session_id}/prompt");
            let resp = client
                .post(&prompt_url)
                .json(&serde_json::json!({
                    "prompt": prompt
                }))
                .send()
                .await;

            match resp {
                Ok(r) => {
                    let body: serde_json::Value = r.json().await.unwrap_or_default();
                    serde_json::json!({
                        "result": body
                    })
                }
                Err(e) => {
                    serde_json::json!({
                        "error": {
                            "code": -32000,
                            "message": format!("Failed to send prompt: {e}")
                        }
                    })
                }
            }
        }
        "cancel" => {
            let session_id = params["sessionId"].as_str().unwrap_or("");
            if session_id.is_empty() {
                return serde_json::json!({
                    "error": {
                        "code": -32602,
                        "message": "Missing sessionId parameter"
                    }
                });
            }

            let cancel_url = format!("{server_url}/session/{session_id}/cancel");
            let resp = client.post(&cancel_url).send().await;

            match resp {
                Ok(_) => serde_json::json!({
                    "result": {}
                }),
                Err(e) => {
                    serde_json::json!({
                        "error": {
                            "code": -32000,
                            "message": format!("Failed to cancel session: {e}")
                        }
                    })
                }
            }
        }
        "closeSession" => {
            let session_id = params["sessionId"].as_str().unwrap_or("");
            if session_id.is_empty() {
                return serde_json::json!({
                    "error": {
                        "code": -32602,
                        "message": "Missing sessionId parameter"
                    }
                });
            }

            let delete_url = format!("{server_url}/session/{session_id}");
            let resp = client.delete(&delete_url).send().await;

            match resp {
                Ok(_) => serde_json::json!({
                    "result": {}
                }),
                Err(e) => {
                    serde_json::json!({
                        "error": {
                            "code": -32000,
                            "message": format!("Failed to close session: {e}")
                        }
                    })
                }
            }
        }
        _ => {
            serde_json::json!({
                "error": {
                    "code": -32601,
                    "message": format!("Method not found: {method}")
                }
            })
        }
    }
}

/// Handle an ACP notification message (no response expected).
async fn handle_acp_notification(
    method: &str,
    msg: &serde_json::Value,
    server_url: &str,
    client: &reqwest::Client,
) {
    // Notifications are fire-and-forget; we just log them
    match method {
        "initialized" => {
            eprintln!("ACP: Client initialized");
        }
        "cancelRequest" => {
            let session_id = msg["params"]["sessionId"].as_str().unwrap_or("");
            if !session_id.is_empty() {
                let cancel_url = format!("{server_url}/session/{session_id}/cancel");
                let _ = client.post(&cancel_url).send().await;
            }
        }
        _ => {
            eprintln!("ACP: Unknown notification: {method}");
        }
    }
}

// ═════════════════════════════════════════════════════════════════════════════
// console (account)
// ═════════════════════════════════════════════════════════════════════════════

/// `console` — Account management.
///
/// Ported from: `packages/opencode/src/cli/cmd/account.ts`
async fn cmd_console(cmd: &ConsoleCommand) -> i32 {
    match cmd {
        ConsoleCommand::Login { url } => {
            let server_url = url.as_deref().unwrap_or("https://console.opencode.ai");
            cmd_console_login(server_url).await
        }
        ConsoleCommand::Logout { email } => cmd_console_logout(email.as_deref()).await,
        ConsoleCommand::Switch => cmd_console_switch().await,
        ConsoleCommand::Orgs => cmd_console_orgs().await,
        ConsoleCommand::Open => {
            let console_url = "https://console.opencode.ai";
            eprintln!("Opening: {console_url}");
            open_url(console_url);
            0
        }
    }
}

/// Console login via OAuth 2.0 Device Authorization Flow.
///
/// Flow:
/// 1. POST /auth/device/code — get device code + user code
/// 2. Display URL and user code, open browser
/// 3. Poll POST /auth/device/token until authorized
/// 4. GET /api/user — fetch user profile
/// 5. GET /api/orgs — fetch organizations
/// 6. Store account in SQLite
async fn cmd_console_login(server_url: &str) -> i32 {
    let client = match reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(30))
        .build()
    {
        Ok(c) => c,
        Err(e) => {
            eprintln!("Failed to create HTTP client: {e}");
            return 1;
        }
    };

    // Step 1: Request device code
    eprintln!("Requesting device authorization from {server_url}...");
    let device_code_url = format!("{server_url}/auth/device/code");
    let resp = match client
        .post(&device_code_url)
        .json(&serde_json::json!({
            "client_id": "opencode-cli"
        }))
        .send()
        .await
    {
        Ok(r) => r,
        Err(e) => {
            eprintln!("Failed to request device code: {e}");
            eprintln!("Check your network connection and try again.");
            return 1;
        }
    };

    if !resp.status().is_success() {
        let status = resp.status();
        let body = resp.text().await.unwrap_or_default();
        eprintln!("Device code request failed (HTTP {status}): {body}");
        return 1;
    }

    let device_resp: serde_json::Value = match resp.json().await {
        Ok(v) => v,
        Err(e) => {
            eprintln!("Failed to parse device code response: {e}");
            return 1;
        }
    };

    let device_code = device_resp["device_code"].as_str().unwrap_or("");
    let user_code = device_resp["user_code"].as_str().unwrap_or("UNKNOWN");
    let default_verification_url = format!("{server_url}/login");
    let verification_url = device_resp["verification_uri"]
        .as_str()
        .unwrap_or(&default_verification_url);
    let interval = device_resp["interval"].as_u64().unwrap_or(5);
    let expires_in = device_resp["expires_in"].as_u64().unwrap_or(900);

    if device_code.is_empty() {
        eprintln!("Invalid device code response: missing device_code");
        eprintln!("Response: {device_resp}");
        return 1;
    }

    // Step 2: Display user code and open browser
    eprintln!();
    eprintln!("To authenticate, open this URL in your browser:");
    eprintln!();
    eprintln!("  {verification_url}");
    eprintln!();
    eprintln!("And enter this code:");
    eprintln!();
    eprintln!("  {user_code}");
    eprintln!();
    eprintln!("Waiting for authorization (expires in {expires_in}s, polling every {interval}s)...");
    eprintln!();

    open_url(verification_url);

    // Step 3: Poll for token
    let token_url = format!("{server_url}/auth/device/token");
    let start = std::time::Instant::now();
    let max_duration = std::time::Duration::from_secs(expires_in);
    let poll_interval = std::time::Duration::from_secs(interval);

    loop {
        if start.elapsed() > max_duration {
            eprintln!();
            eprintln!("Device authorization expired. Please try again.");
            return 1;
        }

        tokio::time::sleep(poll_interval).await;

        let poll_resp = match client
            .post(&token_url)
            .json(&serde_json::json!({
                "grant_type": "urn:ietf:params:oauth:grant-type:device_code",
                "device_code": device_code,
                "client_id": "opencode-cli"
            }))
            .send()
            .await
        {
            Ok(r) => r,
            Err(e) => {
                eprintln!("Poll request failed: {e}");
                continue;
            }
        };

        if !poll_resp.status().is_success() {
            let status = poll_resp.status();
            let body = poll_resp.text().await.unwrap_or_default();
            eprintln!("Poll failed (HTTP {status}): {body}");
            continue;
        }

        let poll_body: serde_json::Value = match poll_resp.json().await {
            Ok(v) => v,
            Err(e) => {
                eprintln!("Failed to parse poll response: {e}");
                continue;
            }
        };

        // Check for errors
        if let Some(error) = poll_body["error"].as_str() {
            match error {
                "authorization_pending" => {
                    // Keep polling
                    continue;
                }
                "slow_down" => {
                    // Increase polling interval
                    tokio::time::sleep(std::time::Duration::from_secs(5)).await;
                    continue;
                }
                "expired_token" => {
                    eprintln!();
                    eprintln!("Device code expired. Please try again.");
                    return 1;
                }
                "access_denied" => {
                    eprintln!();
                    eprintln!("Authorization denied by user.");
                    return 1;
                }
                _ => {
                    eprintln!();
                    eprintln!("Authorization error: {error}");
                    if let Some(desc) = poll_body["error_description"].as_str() {
                        eprintln!("Description: {desc}");
                    }
                    return 1;
                }
            }
        }

        // Success — extract tokens
        let access_token = poll_body["access_token"].as_str().unwrap_or("");
        let refresh_token = poll_body["refresh_token"].as_str().unwrap_or("");

        if access_token.is_empty() {
            eprintln!("Authorization succeeded but no access token received.");
            eprintln!("Response: {poll_body}");
            return 1;
        }

        eprintln!("Authorization successful!");

        // Step 4: Fetch user profile
        let user_url = format!("{server_url}/api/user");
        let user_resp = match client.get(&user_url).bearer_auth(access_token).send().await {
            Ok(r) => r,
            Err(e) => {
                eprintln!("Failed to fetch user profile: {e}");
                return 1;
            }
        };

        let user_body: serde_json::Value = match user_resp.json().await {
            Ok(v) => v,
            Err(e) => {
                eprintln!("Failed to parse user profile: {e}");
                return 1;
            }
        };

        let user_id = user_body["id"].as_str().unwrap_or("").to_string();
        let user_email = user_body["email"].as_str().unwrap_or("").to_string();

        if user_email.is_empty() {
            eprintln!("Failed to fetch user email from profile.");
            eprintln!("Response: {user_body}");
            return 1;
        }

        eprintln!("Logged in as: {user_email}");

        // Step 5: Fetch organizations
        let orgs_url = format!("{server_url}/api/orgs");
        let orgs_body: Vec<serde_json::Value> =
            match client.get(&orgs_url).bearer_auth(access_token).send().await {
                Ok(r) => r.json().await.unwrap_or_default(),
                Err(e) => {
                    eprintln!("Warning: Failed to fetch organizations: {e}");
                    vec![]
                }
            };

        let active_org_id = orgs_body
            .first()
            .and_then(|o| o["id"].as_str())
            .map(String::from);

        // Step 6: Store account in SQLite
        let db_path = get_db_path();
        let pool =
            match sqlx::sqlite::SqlitePool::connect(&format!("{}?mode=rwc", db_path.display()))
                .await
            {
                Ok(p) => p,
                Err(e) => {
                    eprintln!("Failed to open database: {e}");
                    return 1;
                }
            };

        let now_ms = chrono::Utc::now().timestamp_millis();
        let account_id = if user_id.is_empty() {
            format!("acct_{}", uuid_v4_hex())
        } else {
            user_id
        };

        // Upsert account
        if let Err(e) = sqlx::query(
            r#"INSERT OR REPLACE INTO account (id, email, url, access_token, refresh_token, token_expiry, time_created, time_updated)
               VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)"#,
        )
        .bind(&account_id)
        .bind(&user_email)
        .bind(server_url)
        .bind(access_token)
        .bind(refresh_token)
        .bind(now_ms + 3600 * 1000) // 1 hour from now
        .bind(now_ms)
        .bind(now_ms)
        .execute(&pool)
        .await
        {
            eprintln!("Failed to store account: {e}");
            return 1;
        }

        // Upsert account state
        if let Err(e) = sqlx::query(
            r#"INSERT OR REPLACE INTO account_state (id, active_account_id, active_org_id)
               VALUES (1, ?1, ?2)"#,
        )
        .bind(&account_id)
        .bind(&active_org_id)
        .execute(&pool)
        .await
        {
            eprintln!("Failed to update account state: {e}");
            return 1;
        }

        if let Some(ref org_id) = active_org_id {
            let org_name = orgs_body
                .iter()
                .find(|o| o["id"].as_str() == Some(org_id.as_str()))
                .and_then(|o| o["name"].as_str())
                .unwrap_or("unknown");
            eprintln!("Active organization: {org_name} ({org_id})");
        }

        eprintln!();
        eprintln!("Account stored successfully. You can now use console features.");
        return 0;
    }
}

/// Console logout — remove a saved account.
async fn cmd_console_logout(email: Option<&str>) -> i32 {
    let db_path = get_db_path();
    let pool =
        match sqlx::sqlite::SqlitePool::connect(&format!("{}?mode=ro", db_path.display())).await {
            Ok(p) => p,
            Err(e) => {
                eprintln!("Failed to open database: {e}");
                return 1;
            }
        };

    // List accounts
    let accounts: Vec<(String, String)> =
        match sqlx::query_as("SELECT id, email FROM account ORDER BY time_created DESC")
            .fetch_all(&pool)
            .await
        {
            Ok(rows) => rows,
            Err(e) => {
                eprintln!("Failed to list accounts: {e}");
                return 1;
            }
        };

    if accounts.is_empty() {
        eprintln!("No saved accounts found.");
        return 0;
    }

    let target_email = match email {
        Some(e) => e.to_string(),
        None => {
            // Show accounts and pick first (or prompt in interactive mode)
            eprintln!("Saved accounts:");
            for (i, (id, email)) in accounts.iter().enumerate() {
                eprintln!("  {}. {email} ({id})", i + 1);
            }
            if let Some((_, email)) = accounts.first() {
                email.clone()
            } else {
                eprintln!("No accounts to remove.");
                return 0;
            }
        }
    };

    // Find account by email
    let account_id = match accounts.iter().find(|(_, e)| e == &target_email) {
        Some((id, _)) => id.clone(),
        None => {
            eprintln!("Account not found: {target_email}");
            return 1;
        }
    };

    // Drop pool before opening rwc
    drop(pool);

    // Delete account
    let pool_rw =
        match sqlx::sqlite::SqlitePool::connect(&format!("{}?mode=rwc", db_path.display())).await {
            Ok(p) => p,
            Err(e) => {
                eprintln!("Failed to open database: {e}");
                return 1;
            }
        };

    if let Err(e) = sqlx::query("DELETE FROM account WHERE id = ?1")
        .bind(&account_id)
        .execute(&pool_rw)
        .await
    {
        eprintln!("Failed to delete account: {e}");
        return 1;
    }

    // Clear active state if this was the active account
    let _ = sqlx::query("UPDATE account_state SET active_account_id = NULL, active_org_id = NULL WHERE active_account_id = ?1")
        .bind(&account_id)
        .execute(&pool_rw)
        .await;

    eprintln!("Removed account: {target_email}");
    0
}

/// Console switch — switch active organization.
async fn cmd_console_switch() -> i32 {
    let db_path = get_db_path();
    let pool =
        match sqlx::sqlite::SqlitePool::connect(&format!("{}?mode=ro", db_path.display())).await {
            Ok(p) => p,
            Err(e) => {
                eprintln!("Failed to open database: {e}");
                return 1;
            }
        };

    // Get active account
    let active: Option<(Option<String>, Option<String>)> =
        sqlx::query_as("SELECT active_account_id, active_org_id FROM account_state WHERE id = 1")
            .fetch_optional(&pool)
            .await
            .ok()
            .flatten();

    let (active_account_id, active_org_id) = match active {
        Some((a, o)) => (a, o),
        None => {
            eprintln!("No active account. Run `console login` first.");
            return 1;
        }
    };

    if active_account_id.is_none() {
        eprintln!("No active account. Run `console login` first.");
        return 1;
    }

    // Get all accounts
    let accounts: Vec<(String, String, String)> =
        match sqlx::query_as("SELECT id, email, url FROM account ORDER BY time_created DESC")
            .fetch_all(&pool)
            .await
        {
            Ok(rows) => rows,
            Err(e) => {
                eprintln!("Failed to list accounts: {e}");
                return 1;
            }
        };

    if accounts.is_empty() {
        eprintln!("No saved accounts. Run `console login` first.");
        return 0;
    }

    // Fetch orgs for each account
    let client = match reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(10))
        .build()
    {
        Ok(c) => c,
        Err(e) => {
            eprintln!("Failed to create HTTP client: {e}");
            return 1;
        }
    };

    eprintln!("Available organizations:");
    eprintln!();

    let mut all_orgs: Vec<(String, String, String, String)> = Vec::new(); // (account_id, account_email, org_id, org_name)

    for (account_id, email, url) in &accounts {
        // Get access token for this account
        let token: Option<String> =
            sqlx::query_scalar("SELECT access_token FROM account WHERE id = ?1")
                .bind(account_id)
                .fetch_optional(&pool)
                .await
                .ok()
                .flatten();

        let access_token = match token {
            Some(t) => t,
            None => continue,
        };

        let orgs_url = format!("{url}/api/orgs");
        let orgs_resp = match client
            .get(&orgs_url)
            .bearer_auth(&access_token)
            .send()
            .await
        {
            Ok(r) => r,
            Err(_) => continue,
        };

        let orgs: Vec<serde_json::Value> = orgs_resp.json().await.unwrap_or_default();

        for org in &orgs {
            let org_id = org["id"].as_str().unwrap_or("").to_string();
            let org_name = org["name"].as_str().unwrap_or("unknown").to_string();
            let is_active = active_org_id.as_deref() == Some(&org_id);
            let marker = if is_active { " *" } else { "" };
            eprintln!("  {org_name} ({org_id}) — {email}{marker}");
            all_orgs.push((account_id.clone(), email.clone(), org_id, org_name));
        }
    }

    if all_orgs.is_empty() {
        eprintln!("No organizations found across any accounts.");
        return 0;
    }

    eprintln!();
    eprintln!("(* = currently active)");
    eprintln!();
    eprintln!("To switch, update the active organization in the database:");
    eprintln!("  rustcode db \"UPDATE account_state SET active_account_id='<account_id>', active_org_id='<org_id>' WHERE id=1\"");
    0
}

/// Console orgs — list all organizations.
async fn cmd_console_orgs() -> i32 {
    let db_path = get_db_path();
    let pool =
        match sqlx::sqlite::SqlitePool::connect(&format!("{}?mode=ro", db_path.display())).await {
            Ok(p) => p,
            Err(e) => {
                eprintln!("Failed to open database: {e}");
                return 1;
            }
        };

    // Get active org
    let active_org: Option<Option<String>> =
        sqlx::query_scalar("SELECT active_org_id FROM account_state WHERE id = 1")
            .fetch_optional(&pool)
            .await
            .ok()
            .flatten()
            .flatten();

    // Get all accounts
    let accounts: Vec<(String, String, String)> =
        match sqlx::query_as("SELECT id, email, url FROM account ORDER BY time_created DESC")
            .fetch_all(&pool)
            .await
        {
            Ok(rows) => rows,
            Err(e) => {
                eprintln!("Failed to list accounts: {e}");
                return 1;
            }
        };

    if accounts.is_empty() {
        eprintln!("No saved accounts. Run `console login` first.");
        return 0;
    }

    let client = match reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(10))
        .build()
    {
        Ok(c) => c,
        Err(e) => {
            eprintln!("Failed to create HTTP client: {e}");
            return 1;
        }
    };

    eprintln!("Organizations:");
    eprintln!();

    for (account_id, email, url) in &accounts {
        let token: Option<String> =
            sqlx::query_scalar("SELECT access_token FROM account WHERE id = ?1")
                .bind(account_id)
                .fetch_optional(&pool)
                .await
                .ok()
                .flatten();

        let access_token = match token {
            Some(t) => t,
            None => continue,
        };

        let orgs_url = format!("{url}/api/orgs");
        let orgs_resp = match client
            .get(&orgs_url)
            .bearer_auth(&access_token)
            .send()
            .await
        {
            Ok(r) => r,
            Err(_) => continue,
        };

        let orgs: Vec<serde_json::Value> = orgs_resp.json().await.unwrap_or_default();

        if orgs.is_empty() {
            eprintln!("  {email}: No organizations");
            continue;
        }

        for org in &orgs {
            let org_id = org["id"].as_str().unwrap_or("");
            let org_name = org["name"].as_str().unwrap_or("unknown");
            let is_active = active_org.as_ref().and_then(|o| o.as_deref()) == Some(org_id);
            let marker = if is_active { " ●" } else { "" };
            eprintln!("  {org_name} ({org_id}) — {email}{marker}");
        }
    }

    0
}

// ═════════════════════════════════════════════════════════════════════════════
// debug
// ═════════════════════════════════════════════════════════════════════════════

/// `debug` — Debugging and troubleshooting tools.
///
/// Ported from: `packages/opencode/src/cli/cmd/debug/index.ts`
async fn cmd_debug(cmd: &DebugCommand) -> i32 {
    match cmd {
        DebugCommand::Config => {
            // TS: Loads config and writes JSON to stdout
            let config = Config::load().unwrap_or_default();
            match serde_json::to_string_pretty(&config) {
                Ok(json) => println!("{json}"),
                Err(e) => {
                    eprintln!("Failed to serialize config: {e}");
                    return 1;
                }
            }
            0
        }
        DebugCommand::Lsp { cmd: lsp_cmd } => {
            match lsp_cmd {
                DebugLspCommand::Diagnostics { file } => {
                    eprintln!("LSP diagnostics for: {file}");
                    eprintln!("LSP integration not yet implemented.");
                    eprintln!("When available, this will query the LSP server for diagnostics");
                    eprintln!("in the given file and print them as JSON.");
                }
                DebugLspCommand::Symbols { query } => {
                    eprintln!("LSP workspace symbols for: {query}");
                    eprintln!("LSP integration not yet implemented.");
                }
                DebugLspCommand::DocumentSymbols { uri } => {
                    eprintln!("LSP document symbols for: {uri}");
                    eprintln!("LSP integration not yet implemented.");
                }
            }
            0
        }
        DebugCommand::Rg { cmd: rg_cmd } => {
            match rg_cmd {
                DebugRgCommand::Files { query, glob, limit } => {
                    // TS: Uses ripgrep to list files
                    eprintln!("Ripgrep file search:");
                    if let Some(q) = query {
                        eprintln!("  query: {q}");
                    }
                    if let Some(g) = glob {
                        eprintln!("  glob: {g}");
                    }
                    if let Some(l) = limit {
                        eprintln!("  limit: {l}");
                    }

                    // Try to use `rg --files` for actual file listing
                    let mut cmd = tokio::process::Command::new("rg");
                    cmd.arg("--files");
                    if let Some(g) = glob {
                        cmd.arg("--glob").arg(g);
                    }

                    match cmd.output().await {
                        Ok(output) => {
                            if output.status.success() {
                                let stdout = String::from_utf8_lossy(&output.stdout);
                                let mut lines: Vec<&str> = stdout.lines().collect();
                                if let Some(l) = limit {
                                    lines.truncate(*l);
                                }
                                // Filter by query if provided
                                for line in &lines {
                                    if let Some(q) = query {
                                        if line.contains(q.as_str()) {
                                            println!("{line}");
                                        }
                                    } else {
                                        println!("{line}");
                                    }
                                }
                            } else {
                                eprintln!("ripgrep not found or failed. Install ripgrep:");
                                eprintln!("  apt install ripgrep  /  brew install ripgrep");
                                eprintln!("Falling back to find-based file listing...");
                                // Fallback: use `find`
                                list_files_fallback(limit.unwrap_or(100));
                            }
                        }
                        Err(_) => {
                            eprintln!(
                                "ripgrep not available. Falling back to find-based file listing."
                            );
                            list_files_fallback(limit.unwrap_or(100));
                        }
                    }
                }
                DebugRgCommand::Search {
                    pattern,
                    glob,
                    limit,
                } => {
                    eprintln!("Ripgrep content search: {pattern}");

                    let mut cmd = tokio::process::Command::new("rg");
                    cmd.arg("--line-number").arg(pattern);
                    for g in glob {
                        cmd.arg("--glob").arg(g);
                    }
                    if let Some(l) = limit {
                        cmd.arg("--max-count").arg(l.to_string());
                    }

                    match cmd.output().await {
                        Ok(output) => {
                            if output.status.success() {
                                print!("{}", String::from_utf8_lossy(&output.stdout));
                            } else if output.status.code() == Some(1) {
                                eprintln!("No matches found.");
                            } else {
                                eprintln!(
                                    "ripgrep error: {}",
                                    String::from_utf8_lossy(&output.stderr)
                                );
                            }
                        }
                        Err(e) => {
                            eprintln!("Failed to run ripgrep: {e}");
                            eprintln!(
                                "Install ripgrep: apt install ripgrep / brew install ripgrep"
                            );
                        }
                    }
                }
            }
            0
        }
        DebugCommand::File { cmd: file_cmd } => {
            match file_cmd {
                DebugFileCommand::Search { query } => {
                    eprintln!("File search: {query}");
                    // TS: Uses ripgrep `--files` + filter. Same as debug rg files
                    let mut cmd = tokio::process::Command::new("rg");
                    cmd.arg("--files");

                    match cmd.output().await {
                        Ok(output) => {
                            let stdout = String::from_utf8_lossy(&output.stdout);
                            for line in stdout.lines() {
                                if line.contains(query.as_str()) {
                                    println!("{line}");
                                }
                            }
                        }
                        Err(_) => {
                            eprintln!("ripgrep not available.");
                            list_files_fallback(200);
                        }
                    }
                }
                DebugFileCommand::Read { path } => {
                    let p = PathBuf::from(path);
                    match std::fs::read_to_string(&p) {
                        Ok(content) => {
                            match serde_json::to_string_pretty(&serde_json::json!({
                                "path": p.display().to_string(),
                                "size": content.len(),
                                "content": content,
                            })) {
                                Ok(json) => println!("{json}"),
                                Err(e) => eprintln!("Serialization error: {e}"),
                            }
                        }
                        Err(e) => {
                            eprintln!("Failed to read file {}: {e}", p.display());
                            return 1;
                        }
                    }
                }
                DebugFileCommand::List { path } => {
                    let p = PathBuf::from(path);
                    match std::fs::read_dir(&p) {
                        Ok(entries) => {
                            let items: Vec<serde_json::Value> = entries
                                .flatten()
                                .map(|e| {
                                    let ft = e.file_type().ok();
                                    serde_json::json!({
                                        "name": e.file_name().to_string_lossy(),
                                        "path": e.path().display().to_string(),
                                        "is_dir": ft.map(|t| t.is_dir()).unwrap_or(false),
                                        "is_file": ft.map(|t| t.is_file()).unwrap_or(false),
                                    })
                                })
                                .collect();
                            println!(
                                "{}",
                                serde_json::to_string_pretty(&items).unwrap_or_default()
                            );
                        }
                        Err(e) => {
                            eprintln!("Failed to list directory {}: {e}", p.display());
                            return 1;
                        }
                    }
                }
            }
            0
        }
        DebugCommand::Scrap => {
            // TS: Lists all known projects from Project.Service
            eprintln!("Known projects:");
            eprintln!();

            // In the TS version, this queries the project table from SQLite.
            // We scan for .git directories in known locations.
            let home = dirs::home_dir().unwrap_or_default();
            let known_dirs = [
                home.join("gitaction"),
                home.join("projects"),
                home.join("src"),
                home.join("dev"),
                home.join("workspace"),
            ];

            let mut found = false;
            for dir in &known_dirs {
                if !dir.exists() {
                    continue;
                }
                if let Ok(entries) = std::fs::read_dir(dir) {
                    for entry in entries.flatten() {
                        let path = entry.path();
                        if path.is_dir() && path.join(".git").exists() {
                            println!("{}", shorten_path(&path));
                            found = true;
                        }
                    }
                }
            }

            if !found {
                eprintln!("No known projects found in standard locations.");
                eprintln!("Projects are tracked in the session database by the server.");
            }

            println!("[]"); // JSON empty array for piping
            0
        }
        DebugCommand::Skill => {
            // TS: Lists all available skills from Skill.Service
            eprintln!("Available skills:");
            eprintln!();

            // Scan .opencode/skills/ directories
            let skill_dirs = [
                PathBuf::from(".opencode/skills"),
                dirs::config_dir()
                    .unwrap_or_default()
                    .join("opencode")
                    .join("skills"),
            ];

            let mut found = false;
            for dir in &skill_dirs {
                if !dir.exists() {
                    continue;
                }
                if let Ok(entries) = std::fs::read_dir(dir) {
                    for entry in entries.flatten() {
                        let path = entry.path();
                        if path.extension().is_some_and(|e| e == "md") {
                            let name = path
                                .file_stem()
                                .map(|s| s.to_string_lossy().to_string())
                                .unwrap_or_default();
                            println!("  - {name}  ({})", shorten_path(&path));
                            found = true;
                        }
                    }
                }
            }

            if !found {
                eprintln!("  No skills found.");
                eprintln!();
                eprintln!("Skills are markdown files with YAML frontmatter that provide");
                eprintln!("specialized instructions to agents. Place them in:");
                eprintln!("  .opencode/skills/");
                eprintln!("  ~/.config/opencode/skills/");
            }

            println!("[]"); // JSON empty array
            0
        }
        DebugCommand::Snapshot { cmd: snap_cmd } => {
            match snap_cmd {
                DebugSnapshotCommand::Track => {
                    eprintln!("Snapshot tracking not yet implemented.");
                    eprintln!("Tracks file system state for undo/redo capabilities.");
                }
                DebugSnapshotCommand::Patch { hash } => {
                    eprintln!("Snapshot patch for hash: {hash}");
                    eprintln!("Snapshot system not yet implemented.");
                }
                DebugSnapshotCommand::Diff { hash } => {
                    eprintln!("Snapshot diff for hash: {hash}");
                    eprintln!("Snapshot system not yet implemented.");
                }
            }
            0
        }
        DebugCommand::Startup => {
            // TS: prints `performance.now()` — elapsed ms since process start
            let elapsed = elapsed_ms();
            println!("{elapsed:.2}"); // milliseconds since startup
            0
        }
        DebugCommand::Agent { name, tool, params } => {
            eprintln!("Debug agent: {name}");
            if let Some(t) = tool {
                eprintln!("  Tool: {t}");
            }
            if let Some(p) = params {
                eprintln!("  Params: {p}");
            }
            eprintln!();
            eprintln!("Agent debugging not yet implemented.");
            eprintln!("When available, this tests agent tool execution in isolation.");
            0
        }
        DebugCommand::V2 => {
            // TS: Lists v2 catalog providers and default models
            eprintln!("V2 catalog debug:");
            eprintln!();

            let providers = rustcode_core::providers::auto_detect_all();
            let provider_ids: Vec<String> = providers
                .iter()
                .map(|p| p.provider_id().to_string())
                .collect();
            let default = provider_ids.first().cloned().unwrap_or_default();

            let result = serde_json::json!({
                "providers": provider_ids,
                "default": default,
                "small": {},
            });

            println!(
                "{}",
                serde_json::to_string_pretty(&result).unwrap_or_default()
            );
            0
        }
        DebugCommand::Info => {
            // TS: prints version, OS, terminal, plugins
            println!("rustcode version: {}", env!("CARGO_PKG_VERSION"));
            println!("os: {} {}", std::env::consts::OS, std::env::consts::ARCH);
            println!(
                "terminal: {}",
                std::env::var("TERM").unwrap_or_else(|_| "unknown".to_string())
            );
            println!("plugins:");

            if std::env::var("OPENCODE_PURE").is_ok() {
                println!("  external plugins disabled (--pure)");
                return 0;
            }

            // Check config for plugins
            let config = Config::load().unwrap_or_default();
            if config.plugin.is_empty() {
                println!("  none");
            } else {
                for plugin_spec in &config.plugin {
                    let name = match plugin_spec {
                        rustcode_core::config::PluginSpec::Simple(s) => s.as_str(),
                        rustcode_core::config::PluginSpec::WithOptions(s, _) => s.as_str(),
                    };
                    println!("  - {name}");
                }
            }
            0
        }
        DebugCommand::Paths => {
            // TS: iterates Global.Path entries
            let paths = rustcode_core::global::paths();
            println!("{:<10} {}", "data", paths.data);
            println!("{:<10} {}", "config", paths.config);
            println!("{:<10} {}", "cache", paths.cache);
            println!("{:<10} {}", "state", paths.state);
            println!("{:<10} {}", "tmp", paths.tmp);
            println!("{:<10} {}", "bin", paths.bin);
            println!("{:<10} {}", "log", paths.log);
            println!("{:<10} {}", "repos", paths.repos);
            0
        }
        DebugCommand::Wait => {
            eprintln!("Waiting indefinitely (press Ctrl+C to stop)...");
            // TS: sleeps for 1 day. Block on Ctrl+C.
            tokio::signal::ctrl_c().await.ok();
            eprintln!();
            eprintln!("Interrupted.");
            0
        }
    }
}

/// Fallback file listing using `find`.
fn list_files_fallback(limit: usize) {
    let mut count = 0;
    let cwd = std::env::current_dir().unwrap_or_default();
    if let Ok(entries) = std::fs::read_dir(&cwd) {
        for entry in entries.flatten() {
            if count >= limit {
                break;
            }
            let path = entry.path();
            if !path.is_dir() {
                println!("{}", path.display());
                count += 1;
            }
        }
    }
    if count == 0 {
        eprintln!("No files found in current directory.");
    }
}

// ═════════════════════════════════════════════════════════════════════════════
// upgrade
// ═════════════════════════════════════════════════════════════════════════════

/// `upgrade` — Upgrade OpenCode to the latest or a specific version.
///
/// Ported from: `packages/opencode/src/cli/cmd/upgrade.ts`
async fn cmd_upgrade(args: &UpgradeArgs) -> i32 {
    let target = args
        .target
        .as_deref()
        .map(|t| t.trim_start_matches('v'))
        .unwrap_or("latest");
    let method = args.method.as_deref().unwrap_or("auto");

    let current = env!("CARGO_PKG_VERSION");

    println!("rustcode upgrade");
    println!();
    println!("  Current version: {current}");
    println!("  Target version:  {target}");
    println!("  Method:          {method}");
    println!();

    if target == current {
        println!("rustcode is already at version {target}.");
        println!("No upgrade needed.");
        return 0;
    }

    // TS: Detects installation method (curl, npm, brew, etc.) and runs the
    // appropriate upgrade command. For Rust/Cargo installs, the process is:
    match method {
        "cargo" | "auto" => {
            if has_binary("cargo") {
                println!("rustcode appears to be installed via cargo.");
                println!("To upgrade, run:");
                if target == "latest" {
                    println!("  cargo install --git https://github.com/sst/opencode.git rustcode");
                } else {
                    println!("  cargo install --git https://github.com/sst/opencode.git --tag v{target} rustcode");
                }
            } else {
                println!("Automatic upgrade is not yet implemented for this installation method.");
                println!();
                println!("For Rust/Cargo-based installations:");
                println!("  cargo install --force rustcode");
                println!();
                println!("Check https://github.com/sst/opencode/releases for the latest version.");
            }
        }
        "npm" => {
            println!("Upgrading via npm...");
            println!("  npm install -g opencode-ai@{target}");
        }
        "pnpm" => {
            println!("Upgrading via pnpm...");
            println!("  pnpm install -g opencode-ai@{target}");
        }
        "bun" => {
            println!("Upgrading via bun...");
            println!("  bun install -g opencode-ai@{target}");
        }
        "brew" => {
            println!("Upgrading via Homebrew...");
            println!("  brew upgrade opencode");
        }
        "choco" => {
            println!("Upgrading via Chocolatey...");
            println!("  choco upgrade opencode --version={target}");
        }
        "scoop" => {
            println!("Upgrading via Scoop...");
            println!("  scoop update opencode");
        }
        "curl" => {
            println!("Upgrading via curl script...");
            eprintln!("  curl -fsSL https://opencode.ai/install.sh | bash");
        }
        _ => {
            println!("Unknown installation method: {method}");
            println!("Please upgrade manually or use a supported method.");
        }
    }

    0
}

// ═════════════════════════════════════════════════════════════════════════════
// uninstall
// ═════════════════════════════════════════════════════════════════════════════

/// `uninstall` — Uninstall OpenCode and remove all related files.
///
/// Ported from: `packages/opencode/src/cli/cmd/uninstall.ts`
async fn cmd_uninstall(args: &UninstallArgs) -> i32 {
    let paths = rustcode_core::global::paths();

    println!("rustcode uninstall");
    println!();
    println!("The following directories would be affected:");
    println!();

    // List what would be removed
    let targets = [
        ("Data", &paths.data, args.keep_data),
        ("Cache", &paths.cache, false),
        ("Config", &paths.config, args.keep_config),
        ("State", &paths.state, false),
    ];

    for (label, path, keep) in &targets {
        let path_buf = PathBuf::from(path);
        let exists = path_buf.exists();
        let size_label = if exists {
            dir_size(&path_buf)
                .map(format_size)
                .unwrap_or_else(|_| "?".to_string())
        } else {
            "(does not exist)".to_string()
        };

        let status = if *keep {
            " (keeping)"
        } else if exists {
            ""
        } else {
            " (does not exist)"
        };

        let prefix = if *keep { "\u{25cb}" } else { "\u{2713}" };
        println!(
            "  {prefix} {label}: {} ({size_label}){status}",
            shorten_path(&path_buf)
        );
    }

    // Show binary location
    if let Ok(exe) = std::env::current_exe() {
        println!("  \u{2713} Binary: {}", shorten_path(&exe));
    }

    println!();

    if args.dry_run {
        println!("Dry run — no changes made.");
        return 0;
    }

    if !args.force {
        eprintln!("To proceed with removal, re-run with --force.");
        eprintln!("To keep config files, use --keep-config.");
        eprintln!("To keep session data, use --keep-data.");
        return 0;
    }

    // Actually remove directories
    for (label, path, keep) in &targets {
        if *keep {
            println!("Skipping {label} (--keep-{})", label.to_lowercase());
            continue;
        }
        let path_buf = PathBuf::from(path);
        if !path_buf.exists() {
            continue;
        }
        print!("Removing {label}... ");
        match std::fs::remove_dir_all(&path_buf) {
            Ok(()) => println!("done"),
            Err(e) => eprintln!("failed: {e}"),
        }
    }

    println!();
    println!("Thank you for using rustcode!");
    0
}

/// Calculate total directory size recursively.
fn dir_size(path: &Path) -> Result<u64, std::io::Error> {
    let mut total: u64 = 0;
    if path.is_dir() {
        for entry in std::fs::read_dir(path)? {
            let entry = entry?;
            let path = entry.path();
            if path.is_dir() {
                total += dir_size(&path).unwrap_or(0);
            } else if path.is_file() {
                total += path.metadata().map(|m| m.len()).unwrap_or(0);
            }
        }
    }
    Ok(total)
}

// ═════════════════════════════════════════════════════════════════════════════
/// Extract the repository name from a GitHub event payload.
///
/// Looks for `repository.full_name` or constructs from `repository.owner.login`
/// and `repository.name`.
fn extract_repo_from_payload(payload: &serde_json::Value, _event_type: &str) -> Option<String> {
    let repo = payload.get("repository")?;
    if let Some(full) = repo.get("full_name").and_then(|v| v.as_str()) {
        return Some(full.to_string());
    }
    let owner = repo.get("owner")?.get("login")?.as_str()?;
    let name = repo.get("name")?.as_str()?;
    Some(format!("{owner}/{name}"))
}

/// Extract the issue or PR number from a GitHub event payload.
fn extract_issue_number(payload: &serde_json::Value, event_type: &str) -> Option<u64> {
    match event_type {
        "issues" | "issue_comment" => payload.get("issue")?.get("number")?.as_u64(),
        "pull_request" | "pull_request_review_comment" => {
            payload.get("pull_request")?.get("number")?.as_u64()
        }
        _ => payload
            .get("issue")
            .and_then(|i| i.get("number"))
            .or_else(|| payload.get("pull_request").and_then(|p| p.get("number")))
            .and_then(|v| v.as_u64()),
    }
}

/// Extract the comment body from a GitHub event payload.
fn extract_comment_body(payload: &serde_json::Value, event_type: &str) -> Option<String> {
    match event_type {
        "issue_comment" | "pull_request_review_comment" => payload
            .get("comment")?
            .get("body")?
            .as_str()
            .map(|s| s.to_string()),
        "issues" => payload
            .get("issue")?
            .get("body")?
            .as_str()
            .map(|s| s.to_string()),
        "pull_request" => payload
            .get("pull_request")?
            .get("body")?
            .as_str()
            .map(|s| s.to_string()),
        _ => payload
            .get("comment")
            .and_then(|c| c.get("body"))
            .or_else(|| payload.get("issue").and_then(|i| i.get("body")))
            .and_then(|v| v.as_str())
            .map(|s| s.to_string()),
    }
}

/// Build a descriptive task string for the AI agent based on the event.
fn build_github_task(
    event_type: &str,
    action: &str,
    comment_body: &Option<String>,
    repo_info: &Option<String>,
    issue_number: Option<u64>,
) -> String {
    let repo = repo_info.as_deref().unwrap_or("this repository");
    let issue_ref = issue_number
        .map(|n| format!("#{n}"))
        .unwrap_or_else(|| "(unknown)".to_string());

    match event_type {
        "issues" if action == "opened" => format!(
            "A new issue {issue_ref} was opened in {repo}. \
             Analyze the issue and provide a helpful response. \
             If the issue describes a bug, suggest possible fixes. \
             If it's a feature request, evaluate feasibility and suggest an approach."
        ),
        "issues" if action == "closed" => format!(
            "Issue {issue_ref} in {repo} was closed. \
             Review the resolution and provide a summary of the outcome if details are available."
        ),
        "issues" => format!(
            "Issue {issue_ref} in {repo} received an update (action: {action}). \
             Review the changes and provide relevant context or analysis."
        ),
        "issue_comment" => {
            let body_hint = comment_body.as_ref().map(|b| {
                let preview: String = b.chars().take(200).collect();
                if b.len() > 200 {
                    format!("{preview}...")
                } else {
                    preview
                }
            });
            match body_hint {
                Some(hint) => format!(
                    "A new comment was posted on {issue_ref} in {repo}. \
                     Comment preview: \"{hint}\". \
                     Respond to the comment with analysis, code suggestions, or answers to questions raised."
                ),
                None => format!(
                    "A new comment was posted on {issue_ref} in {repo}. \
                     Review and respond with relevant analysis or code suggestions."
                ),
            }
        }
        "pull_request" if action == "opened" => format!(
            "A new pull request {issue_ref} was opened in {repo}. \
             Perform a thorough code review: check for bugs, security vulnerabilities, \
             performance issues, code style, and test coverage. \
             Suggest improvements where applicable and summarize your findings."
        ),
        "pull_request" if action == "synchronize" => format!(
            "Pull request {issue_ref} in {repo} was updated with new commits. \
             Review the new changes and provide feedback on the delta."
        ),
        "pull_request" if action == "reopened" => format!(
            "Pull request {issue_ref} in {repo} was reopened. \
             Review the changes and provide feedback."
        ),
        "pull_request" => format!(
            "Pull request {issue_ref} in {repo} had an update (action: {action}). \
             Review the changes and provide relevant feedback."
        ),
        "pull_request_review_comment" => format!(
            "A review comment was posted on pull request {issue_ref} in {repo}. \
             Analyze the comment context and provide a detailed code review \
             for the specific lines or files mentioned."
        ),
        "workflow_dispatch" => format!(
            "A workflow was manually dispatched in {repo}. \
             Execute the requested automation steps based on the workflow inputs."
        ),
        "schedule" => format!(
            "A scheduled workflow triggered in {repo}. \
             Execute the periodic automation tasks."
        ),
        _ => format!(
            "A '{event_type}' event occurred in {repo} (action: {action}). \
             Process this event and provide appropriate analysis or actions."
        ),
    }
}

// github
// ═════════════════════════════════════════════════════════════════════════════

/// `github` — Manage GitHub agent.
///
/// Ported from: `packages/opencode/src/cli/cmd/github.ts`
async fn cmd_github(cmd: &GithubCommand) -> i32 {
    match cmd {
        GithubCommand::Install => {
            eprintln!("Installing GitHub agent...");
            eprintln!();

            if has_binary("gh") {
                println!("GitHub CLI found.");

                // Check gh auth status
                match run_gh(&["auth", "status"]).await {
                    Ok(output) => {
                        if output.status.success() {
                            println!("GitHub CLI is authenticated.");
                        } else {
                            eprintln!("GitHub CLI is not authenticated.");
                            eprintln!("Run: gh auth login");
                            return 1;
                        }
                    }
                    Err(e) => {
                        eprintln!("Failed to check GitHub auth: {e}");
                        return 1;
                    }
                }

                eprintln!();
                eprintln!("GitHub agent installation would register a GitHub App or webhook");
                eprintln!("to enable AI-powered PR reviews and issue triage.");
                eprintln!();
                eprintln!("For now, you can manually configure the GitHub integration by:");
                eprintln!("  1. Creating a GitHub App in your account/organization settings");
                eprintln!("  2. Setting GITHUB_TOKEN environment variable");
                eprintln!("  3. Running: rustcode github run");
            } else {
                eprintln!("GitHub CLI (`gh`) is not installed.");
                eprintln!("Install it from: https://cli.github.com");
                eprintln!();
                eprintln!("The GitHub agent requires `gh` CLI for authentication and API access.");
                return 1;
            }
        }
        GithubCommand::Run {
            event,
            event_payload,
            token,
        } => {
            let event_type = event.as_deref().unwrap_or("issue_comment");
            eprintln!("GitHub agent — event: {event_type}");

            let auth_token = token.clone().or_else(|| std::env::var("GITHUB_TOKEN").ok());
            if auth_token.is_none() {
                eprintln!();
                eprintln!("Error: No GitHub token provided.");
                eprintln!("Set GITHUB_TOKEN env var or pass --token.");
                return 1;
            }
            let token = auth_token.unwrap();

            // ── read event payload ──────────────────────────────────
            let payload_json: serde_json::Value = if let Some(ref path) = event_payload {
                match std::fs::read_to_string(path) {
                    Ok(contents) => match serde_json::from_str(&contents) {
                        Ok(v) => v,
                        Err(e) => {
                            eprintln!("Error: failed to parse event payload file {path}: {e}");
                            return 1;
                        }
                    },
                    Err(e) => {
                        eprintln!("Error: cannot read event payload file {path}: {e}");
                        return 1;
                    }
                }
            } else {
                // Read from stdin
                let mut buf = String::new();
                match std::io::Read::read_to_string(&mut std::io::stdin().lock(), &mut buf) {
                    Ok(_) => match serde_json::from_str(&buf) {
                        Ok(v) => v,
                        Err(e) => {
                            eprintln!("Error: failed to parse event payload from stdin: {e}");
                            eprintln!(
                                "Tip: use --event-payload <path> to read from a file instead."
                            );
                            return 1;
                        }
                    },
                    Err(e) => {
                        eprintln!("Error: cannot read stdin: {e}");
                        return 1;
                    }
                }
            };

            eprintln!(
                "Payload received ({:.0} bytes).",
                serde_json::to_string(&payload_json)
                    .map(|s| s.len() as f64)
                    .unwrap_or(0.0)
            );

            // ── extract event context ─────────────────────────────────
            let repo_info = extract_repo_from_payload(&payload_json, event_type);
            let issue_number = extract_issue_number(&payload_json, event_type);
            let comment_body = extract_comment_body(&payload_json, event_type);
            let action = payload_json
                .get("action")
                .and_then(|v| v.as_str())
                .unwrap_or("");

            eprintln!(
                "  Repository: {}",
                repo_info.as_deref().unwrap_or("(unknown)")
            );
            if let Some(num) = issue_number {
                eprintln!("  Issue/PR #: {num}");
            }

            let task_description =
                build_github_task(event_type, action, &comment_body, &repo_info, issue_number);
            eprintln!();
            eprintln!("Task: {task_description}");

            // ── detect AI providers ──────────────────────────────────
            use rustcode_core::providers::auto_detect_all;
            let providers = auto_detect_all();

            if providers.is_empty() {
                eprintln!();
                eprintln!("Error: No AI providers detected.");
                eprintln!("Set environment variables for at least one provider:");
                eprintln!("  ANTHROPIC_API_KEY for Anthropic (Claude)");
                eprintln!("  OPENAI_API_KEY for OpenAI (GPT)");
                eprintln!("  GOOGLE_API_KEY for Google (Gemini)");
                return 1;
            }

            eprintln!();
            eprintln!("Detected {} provider(s):", providers.len());
            for p in &providers {
                eprintln!("  - {}", p.provider_id());
                match p.list_models().await {
                    Ok(models) => {
                        if let Some(first) = models.first() {
                            eprintln!("    default model: {}", first.id);
                        }
                    }
                    Err(e) => {
                        eprintln!("    (model listing failed: {e})");
                    }
                }
            }

            // Use the first available provider
            let provider = &providers[0];
            let models = match provider.list_models().await {
                Ok(m) => m,
                Err(e) => {
                    eprintln!("Error: failed to list models: {e}");
                    return 1;
                }
            };

            if models.is_empty() {
                eprintln!(
                    "Error: no models available for provider '{}'",
                    provider.provider_id()
                );
                return 1;
            }

            // Prefer a model with tool-calling capability, fall back to first
            let model = models
                .iter()
                .find(|m| m.capabilities.toolcall)
                .unwrap_or(&models[0]);

            eprintln!("Using: {}/{}", provider.provider_id(), model.id);
            eprintln!();

            // ── build instructions ───────────────────────────────────
            let cwd = std::env::current_dir()
                .map(|p| p.display().to_string())
                .unwrap_or_else(|_| ".".into());

            let instructions = vec![
                format!(
                    "You are an AI-powered GitHub agent. You are running in response to a '{event_type}' event. \
                     You work in the directory: {cwd}. \
                     When dealing with pull requests, review code changes thoroughly — look for bugs, \
                     security issues, performance problems, and style inconsistencies. \
                     When dealing with issues, provide helpful analysis and context. \
                     Be concise and actionable in your responses."
                ),
                format!(
                    "GitHub API token is available as the GITHUB_TOKEN environment variable if you need \
                     to fetch additional context from the GitHub API via shell commands."
                ),
            ];

            // ── build prompt ─────────────────────────────────────────
            let prompt = if let Some(body) = comment_body {
                format!("{task_description}\n\nComment body:\n{body}")
            } else {
                task_description
            };

            use rustcode_core::session_prompt::{PromptPart, PromptTextPart, SessionPromptInput};

            let session_id = format!("gh-{}-{}", event_type, std::process::id());
            let input = SessionPromptInput {
                session_id: session_id.clone(),
                message_id: None,
                model: Some(rustcode_core::session_info::ModelRef {
                    id: model.id.clone(),
                    provider_id: provider.provider_id().to_string(),
                    variant: None,
                }),
                agent: Some("build".to_string()),
                no_reply: false,
                tools: None,
                format: None,
                system: None,
                variant: None,
                parts: vec![PromptPart::Text(PromptTextPart {
                    id: None,
                    text: prompt,
                    synthetic: false,
                })],
            };

            // ── set up tool registry ─────────────────────────────────
            use rustcode_core::tool::ToolRegistry;
            let tool_registry = Arc::new(ToolRegistry::new());
            tool_registry.register_builtins();

            // Register a lightweight GitHub API helper that injects the token
            // so the agent can access PR diffs, issue details, etc.
            let _gh_token = token.clone();
            // (In the full implementation, a dedicated `gh_api` tool would
            // be registered here that wraps `octokit` calls.)

            // Note: the `question` tool is NOT registered in this headless
            // mode — it requires a QuestionService wired to a session bus
            // with a reply path (TUI or similar).  The runtime.rs
            // `initialize_runtime_with_path()` registers it for interactive
            // sessions.

            let runner = rustcode_core::session_runner::SessionRunner::new(tool_registry);

            // ── run the agent ────────────────────────────────────────
            eprintln!("Starting agent session {} ...", session_id);
            eprintln!();

            match runner
                .run(provider.as_ref(), model, &input, &instructions)
                .await
            {
                Ok(result) => {
                    if !result.text.is_empty() {
                        println!("{}", result.text);
                    }

                    if !result.tool_calls.is_empty() {
                        let ok = result.tool_calls.iter().filter(|t| t.success).count();
                        let fail = result.tool_calls.len() - ok;
                        eprintln!();
                        eprintln!(
                            "--- {} tool calls ({} ok, {} failed) in {} iterations ---",
                            result.tool_calls.len(),
                            ok,
                            fail,
                            result.iterations,
                        );
                    }

                    if let Some(ref err) = result.error {
                        eprintln!("Session error: {err}");
                        if !result.success {
                            return 1;
                        }
                    }

                    eprintln!();
                    eprintln!("Agent session completed successfully.");
                }
                Err(e) => {
                    eprintln!();
                    eprintln!("Agent error: {e}");
                    return 1;
                }
            }
        }
    }

    0
}

// ═════════════════════════════════════════════════════════════════════════════
// pr
// ═════════════════════════════════════════════════════════════════════════════

/// `pr` — Fetch and checkout a GitHub PR branch.
///
/// Ported from: `packages/opencode/src/cli/cmd/pr.ts`
async fn cmd_pr(args: &PrArgs) -> i32 {
    let pr_number = args.number;
    eprintln!("Fetching and checking out PR #{pr_number}...");

    // Check that we're in a git repo
    let git_dir = PathBuf::from(".git");
    if !git_dir.exists() {
        eprintln!("Error: Not in a git repository.");
        eprintln!("Run this command from the root of a git repository.");
        return 1;
    }

    // Check if `gh` CLI is available
    if !has_binary("gh") {
        eprintln!("Error: GitHub CLI (`gh`) is not installed.");
        eprintln!("Install it from: https://cli.github.com");
        eprintln!();
        eprintln!("This command uses `gh pr checkout` to fetch and switch to the PR branch.");
        return 1;
    }

    // Check gh auth status
    let auth_check = run_gh(&["auth", "status"]).await;
    match auth_check {
        Ok(output) if output.status.success() => {}
        _ => {
            eprintln!("Error: GitHub CLI is not authenticated.");
            eprintln!("Run: gh auth login");
            return 1;
        }
    }

    // Run `gh pr checkout`
    let local_branch = format!("pr/{pr_number}");
    eprintln!("Checking out PR #{pr_number} as branch '{local_branch}'...");

    match run_gh(&[
        "pr",
        "checkout",
        &pr_number.to_string(),
        "--branch",
        &local_branch,
        "--force",
    ])
    .await
    {
        Ok(output) => {
            if output.status.success() {
                println!("Successfully checked out PR #{pr_number} as branch '{local_branch}'");

                // Try to get PR info for cross-repo fork handling
                if let Ok(info_output) = run_gh(&[
                    "pr",
                    "view",
                    &pr_number.to_string(),
                    "--json",
                    "headRepository,headRepositoryOwner,isCrossRepository,headRefName,body",
                ])
                .await
                {
                    if info_output.status.success() {
                        if let Ok(info) = serde_json::from_str::<serde_json::Value>(
                            &String::from_utf8_lossy(&info_output.stdout),
                        ) {
                            // Handle cross-repository PRs
                            if info
                                .get("isCrossRepository")
                                .and_then(|v| v.as_bool())
                                .unwrap_or(false)
                            {
                                if let (Some(owner), Some(repo)) = (
                                    info.get("headRepositoryOwner")
                                        .and_then(|o| o.get("login"))
                                        .and_then(|l| l.as_str()),
                                    info.get("headRepository")
                                        .and_then(|r| r.get("name"))
                                        .and_then(|n| n.as_str()),
                                ) {
                                    let remote_url =
                                        format!("https://github.com/{owner}/{repo}.git");
                                    eprintln!(
                                        "Cross-repo PR detected. Adding fork remote: {owner}"
                                    );
                                    let _ = run_gh(&["remote", "add", owner, &remote_url]).await;
                                }
                            }

                            // Look for session URLs in PR body
                            if let Some(body) = info.get("body").and_then(|b| b.as_str()) {
                                // Check for opncd.ai share links
                                if let Some(session_url) = extract_session_url(body) {
                                    eprintln!("Found session URL in PR: {session_url}");
                                    eprintln!("Run `rustcode import {session_url}` to import the session.");
                                }
                            }
                        }
                    }
                }

                println!();
                println!("Starting rustcode... Use `rustcode run` or `rustcode tui` to");
                println!("work on this PR with AI assistance.");
            } else {
                let stderr = String::from_utf8_lossy(&output.stderr);
                eprintln!("Failed to checkout PR #{pr_number}:");
                eprintln!("{stderr}");
                eprintln!();
                eprintln!("Make sure:");
                eprintln!("  1. The PR number is correct");
                eprintln!("  2. `gh` CLI is authenticated");
                eprintln!("  3. You have access to the repository");
                return 1;
            }
        }
        Err(e) => {
            eprintln!("Failed to run gh command: {e}");
            return 1;
        }
    }

    0
}

/// Extract a session URL from PR body text (opncd.ai share links).
fn extract_session_url(text: &str) -> Option<String> {
    // Pattern: https://opncd.ai/s/<session-id> or https://opncd.ai/share/<id>
    for prefix in &["https://opncd.ai/s/", "https://opncd.ai/share/"] {
        if let Some(pos) = text.find(prefix) {
            let start = pos;
            let end = text[start..]
                .find(|c: char| c.is_whitespace())
                .map(|p| start + p)
                .unwrap_or(text.len());
            return Some(text[start..end].to_string());
        }
    }
    None
}

// ═════════════════════════════════════════════════════════════════════════════
// plugin
// ═════════════════════════════════════════════════════════════════════════════

/// `plugin` — Install plugin and update config.
///
/// Ported from: `packages/opencode/src/cli/cmd/plug.ts`
async fn cmd_plugin(args: &PluginArgs) -> i32 {
    let module = args.module.trim();
    if module.is_empty() {
        eprintln!("Error: module is required");
        return 1;
    }

    let scope = if args.global { "global" } else { "project" };
    let force = args.force;

    println!("Installing plugin: {module}");
    println!("  Scope:  {scope}");
    println!("  Force:  {force}");
    println!();

    // ── 1. Parse spec, determine source, check deprecation ──
    let source = rustcode_core::plugin::plugin_source(module);
    let is_npm = source == rustcode_core::plugin::PluginSource::Npm;

    if rustcode_core::plugin::is_deprecated_plugin(module) {
        eprintln!("Warning: `{module}` is deprecated and now built-in.");
    }

    let parsed = rustcode_core::plugin::parse_specifier(module);

    // ── 2. Install npm package or validate file path ─────────
    let plugin_dir = if is_npm {
        let pm = match detect_package_manager() {
            "npm (not found)" => {
                eprintln!("Error: no package manager found (bun, pnpm, or npm)");
                return 1;
            }
            other => other,
        };
        eprintln!("Package manager: {pm}");

        let add_cmd = match pm {
            "bun" | "pnpm" => "add",
            _ => "install",
        };

        eprintln!("Running: {pm} {add_cmd} {module}");
        let status = std::process::Command::new(pm)
            .arg(add_cmd)
            .arg(module)
            .stdout(std::process::Stdio::inherit())
            .stderr(std::process::Stdio::inherit())
            .status();

        match status {
            Ok(s) if s.success() => {
                eprintln!("Package installed successfully.");
            }
            Ok(s) => {
                eprintln!("Error: {pm} exited with code {}", s.code().unwrap_or(-1));
                return 1;
            }
            Err(e) => {
                eprintln!("Error: failed to run {pm}: {e}");
                return 1;
            }
        }

        let node_modules = std::env::current_dir()
            .unwrap_or_else(|_| PathBuf::from("."))
            .join("node_modules");
        let dir = node_modules.join(&parsed.pkg);
        if !dir.exists() {
            eprintln!("Warning: package directory not found at {}", dir.display());
        }
        dir
    } else {
        let path_str = module.strip_prefix("file://").unwrap_or(module);
        let dir = PathBuf::from(path_str);
        if !dir.exists() {
            eprintln!("Error: plugin path not found: {}", dir.display());
            return 1;
        }
        if !dir.is_dir() {
            eprintln!("Error: plugin path is not a directory: {}", dir.display());
            return 1;
        }
        eprintln!("Plugin directory: {}", dir.display());
        dir
    };

    // ── 3. Read package.json ────────────────────────────────
    let pkg = match rustcode_core::plugin::read_plugin_package(&plugin_dir) {
        Ok(pkg) => pkg,
        Err(e) => {
            eprintln!("Error reading plugin package: {e}");
            return 1;
        }
    };

    let plugin_id = rustcode_core::plugin::resolve_plugin_id(&pkg)
        .unwrap_or_else(|| parsed.pkg.clone());
    let plugin_version = pkg.version.as_deref().unwrap_or("unknown");

    eprintln!("Plugin ID:      {plugin_id}");
    eprintln!("Plugin version: {plugin_version}");

    // ── 4. Compatibility check (engines.opencode) ───────────
    if let Err(e) =
        rustcode_core::plugin::check_plugin_compatibility(&pkg, env!("CARGO_PKG_VERSION"))
    {
        eprintln!("Warning: {e}");
        if !force {
            eprintln!("Use --force to install anyway.");
            return 1;
        }
        eprintln!("Proceeding (forced).");
    }

    // ── 5. Read plugin manifest → determine targets ─────────
    let targets = match rustcode_core::plugin::read_plugin_manifest(&plugin_dir) {
        Ok(t) => t,
        Err(e) => {
            eprintln!("Error reading plugin manifest: {e}");
            return 1;
        }
    };

    print!("Detected targets:");
    for t in &targets {
        print!(" {}", t.kind);
    }
    println!();

    // ── 6. Patch opencode config to register the plugin ─────
    let config_dir = if args.global {
        dirs::config_dir()
            .map(|d| d.join("opencode"))
            .unwrap_or_else(|| PathBuf::from(".opencode"))
    } else {
        PathBuf::from(".opencode")
    };

    match rustcode_core::plugin::patch_plugin_config(&config_dir, module, &targets, force) {
        Ok(results) => {
            for (kind, path) in &results {
                eprintln!("Patched config ({kind}): {path}");
            }
        }
        Err(e) => {
            eprintln!("Error patching config: {e}");
            return 1;
        }
    }

    // ── 7. Save plugin metadata ────────────────────────────
    let mut meta_manager = rustcode_core::plugin::PluginManager::new();
    if let Some(meta_path) = rustcode_core::plugin::PluginManager::default_meta_path() {
        let _ = meta_manager.load_meta(&meta_path);
        meta_manager.touch_meta(
            &plugin_id,
            source,
            module,
            &plugin_dir.display().to_string(),
            if is_npm { Some(&parsed.version) } else { None },
            pkg.version.as_deref(),
            None,
        );
        if let Err(e) = meta_manager.save_meta(&meta_path) {
            eprintln!("Warning: failed to save plugin metadata: {e}");
        }
    }

    eprintln!();
    eprintln!("Plugin `{module}` installed successfully.");
    let target_str: Vec<String> = targets.iter().map(|t| t.kind.to_string()).collect();
    eprintln!("  ID:      {plugin_id}");
    eprintln!("  Version: {plugin_version}");
    eprintln!("  Targets: {}", target_str.join(", "));
    0
}

/// Detect which npm-compatible package manager is available.
fn detect_package_manager() -> &'static str {
    if has_binary("bun") {
        "bun"
    } else if has_binary("pnpm") {
        "pnpm"
    } else if has_binary("npm") {
        "npm"
    } else {
        "npm (not found)"
    }
}

// ═════════════════════════════════════════════════════════════════════════════
// db
// ═════════════════════════════════════════════════════════════════════════════

/// `db` — Database tools (SQL query or interactive shell).
///
/// Ported from: `packages/opencode/src/cli/cmd/db.ts`
async fn cmd_db(args: &DbArgs) -> i32 {
    let db_path = get_db_path();

    if let Some(query) = &args.query {
        // Execute SQL query
        eprintln!("Database: {}", db_path.display());
        eprintln!("Query: {query}");

        if !db_path.exists() {
            eprintln!("Database file not found. Start the server first to create it.");
            return 1;
        }

        // Run sqlite3 with the query
        let result = tokio::process::Command::new("sqlite3")
            .arg(&db_path)
            .arg("-readonly")
            .arg(match args.format.as_str() {
                "json" => "-json",
                _ => "-separator", // use -separator for TSV mode
            })
            .arg(query)
            .output()
            .await;

        match result {
            Ok(output) => {
                if output.status.success() {
                    print!("{}", String::from_utf8_lossy(&output.stdout));
                } else {
                    eprintln!("SQL error: {}", String::from_utf8_lossy(&output.stderr));
                    return 1;
                }
            }
            Err(e) => {
                eprintln!("Failed to run sqlite3: {e}");
                eprintln!("Install sqlite3: apt install sqlite3 / brew install sqlite");
                return 1;
            }
        }
    } else {
        // Open interactive sqlite3 shell
        if !db_path.exists() {
            eprintln!("Database file not found at: {}", db_path.display());
            eprintln!("Start the server first to create the database.");
            return 1;
        }

        eprintln!(
            "Opening interactive sqlite3 shell for: {}",
            db_path.display()
        );
        eprintln!("Type .exit to quit, .help for sqlite3 commands.");
        eprintln!();

        // TS: spawn("sqlite3", [Database.path()], { stdio: "inherit" })
        let status = std::process::Command::new("sqlite3")
            .arg(&db_path)
            .stdin(std::process::Stdio::inherit())
            .stdout(std::process::Stdio::inherit())
            .stderr(std::process::Stdio::inherit())
            .status();

        match status {
            Ok(s) if s.success() => {}
            Ok(s) => {
                eprintln!("sqlite3 exited with code: {}", s.code().unwrap_or(-1));
            }
            Err(e) => {
                eprintln!("Failed to start sqlite3 shell: {e}");
                eprintln!("Install sqlite3: apt install sqlite3 / brew install sqlite");
                return 1;
            }
        }
    }

    0
}

// ═════════════════════════════════════════════════════════════════════════════
// attach
// ═════════════════════════════════════════════════════════════════════════════

/// `attach` — Attach to a running OpenCode server.
///
/// Ported from: `packages/opencode/src/cli/cmd/attach.ts`
async fn cmd_attach(args: &AttachArgs) -> i32 {
    if args.fork && !args.r#continue && args.session.is_none() {
        cli_error::format_cli_error("--fork requires --continue or --session");
        return 1;
    }

    let url = args.url.trim_end_matches('/');
    eprintln!("Attaching to server: {url}");

    // TS: Changes directory, validates session, launches TUI connected to remote.
    // Try to connect and verify the server is reachable

    // Build auth headers if credentials provided
    let mut headers = reqwest::header::HeaderMap::new();
    let username = args
        .username
        .clone()
        .or_else(|| std::env::var("OPENCODE_SERVER_USERNAME").ok())
        .unwrap_or_else(|| "opencode".to_string());
    let password = args
        .password
        .clone()
        .or_else(|| std::env::var("OPENCODE_SERVER_PASSWORD").ok());

    if let Some(pw) = &password {
        let auth = format!("{username}:{pw}");
        let encoded = base64_encode(&auth);
        if let Ok(hv) = reqwest::header::HeaderValue::from_str(&format!("Basic {}", encoded)) {
            headers.insert(reqwest::header::AUTHORIZATION, hv);
        }
    }

    // Test connection
    let health_url = format!("{url}/api/health");
    eprintln!("Testing connection to {health_url}...");

    let client = reqwest::Client::builder()
        .default_headers(headers.clone())
        .build()
        .expect("Failed to build HTTP client");

    match client.get(&health_url).send().await {
        Ok(response) => {
            if response.status().is_success() {
                eprintln!("Connected to server successfully.");
            } else if response.status().as_u16() == 401 {
                eprintln!("Server requires authentication.");
                eprintln!("Provide --username and --password, or set:");
                eprintln!("  OPENCODE_SERVER_USERNAME");
                eprintln!("  OPENCODE_SERVER_PASSWORD");
                return 1;
            } else {
                eprintln!("Server responded with: HTTP {}", response.status());
                eprintln!("Continuing anyway...");
            }
        }
        Err(e) => {
            eprintln!("Could not connect to server: {e}");
            eprintln!();
            eprintln!("Make sure the server is running:");
            eprintln!("  rustcode serve --port <port>");
            eprintln!();
            eprintln!("Then attach with:");
            eprintln!("  rustcode attach http://localhost:<port>");
            return 1;
        }
    }

    // ── Create SSE client ──────────────────────────────────────────────
    let sse_url = format!("{url}/event");
    eprintln!("Connecting to SSE endpoint: {sse_url}");

    // Build the SseClient with auth and spawn its connect loop.
    // SseClient handles SSE parsing, TuiEvent conversion, and broadcast
    // to subscribers (including the TUI's Remote-mode event loop).
    let mut sse_client = rustcode_tui::SseClient::new(url);
    if let Some(pw) = &password {
        sse_client.set_auth(&username, Some(pw));
    }
    let sse_client = Arc::new(sse_client);

    // Spawn the SSE connect loop in the background — auto-reconnects on drop.
    let sse_for_connect = sse_client.clone();
    tokio::spawn(async move {
        if let Err(e) = sse_for_connect.connect().await {
            eprintln!("SSE connection fatal error: {e}");
        }
    });

    // HTTP client for sending commands (prompts, replies) to the remote server.
    // This is separate from the reqwest::Client inside SseClient::connect().
    let http_client = reqwest::Client::builder()
        .default_headers(headers)
        .build()
        .expect("Failed to build HTTP client");

    // ── Launch TUI (Remote mode) ───────────────────────────────────────
    // Remote mode: TUI subscribes to SseClient for events and uses the
    // http_client to POST prompts/permission-replies/question-answers back
    // to the server.  The run_async() loop handles SSE → TuiEvent dispatch
    // internally via sse.recv() → handle_tui_event().
    eprintln!("Launching TUI connected to {url}...");
    eprintln!("Press Ctrl+C to disconnect.");

    let tui_url = url.to_string();
    match rustcode_tui::TuiApp::new_remote(sse_client, tui_url.clone(), http_client) {
        Ok(mut tui) => {
            let result = tokio::select! {
                // Run TUI event loop (Remote mode — SSE handled internally)
                result = tui.run_async() => {
                    match result {
                        Ok(()) => 0,
                        Err(e) => {
                            eprintln!("TUI error: {e}");
                            1
                        }
                    }
                }

                // Handle Ctrl+C
                _ = tokio::signal::ctrl_c() => {
                    eprintln!("Interrupted. Disconnecting from {tui_url}...");
                    0
                }
            };

            // Cleanup terminal
            if let Err(e) = tui.cleanup() {
                eprintln!("Failed to restore terminal: {e}");
            }

            result
        }
        Err(e) => {
            eprintln!("Failed to initialize TUI: {e}");
            1
        }
    }
}

/// Simple base64 encoder (no external dependency needed).
fn base64_encode(input: &str) -> String {
    const CHARS: &[u8] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
    let bytes = input.as_bytes();
    let mut result = String::new();

    for chunk in bytes.chunks(3) {
        let b0 = chunk[0] as u32;
        let b1 = if chunk.len() > 1 { chunk[1] as u32 } else { 0 };
        let b2 = if chunk.len() > 2 { chunk[2] as u32 } else { 0 };
        let combined = (b0 << 16) | (b1 << 8) | b2;

        result.push(CHARS[((combined >> 18) & 0x3F) as usize] as char);
        result.push(CHARS[((combined >> 12) & 0x3F) as usize] as char);

        if chunk.len() > 1 {
            result.push(CHARS[((combined >> 6) & 0x3F) as usize] as char);
        } else {
            result.push('=');
        }

        if chunk.len() > 2 {
            result.push(CHARS[(combined & 0x3F) as usize] as char);
        } else {
            result.push('=');
        }
    }

    result
}

// ═════════════════════════════════════════════════════════════════════════════
// generate
// ═════════════════════════════════════════════════════════════════════════════

/// `generate` — Generate shell completions.
///
/// Ported from: `packages/opencode/src/cli/cmd/generate.ts` (OpenAPI code samples)
/// and the TS yargs `.completion("completion", ...)` registration.
async fn cmd_generate() -> i32 {
    // TS: Generates OpenAPI spec with x-codeSamples for the JS SDK.
    // In Rust, clap natively supports completions via `clap_complete` crate.
    // Since we avoid adding extra deps, print help about completions.

    println!("# rustcode shell completions");
    println!();
    println!("Shell completions are generated by clap. To enable them:");
    println!();
    println!("  # bash");
    println!("  rustcode --help | complete -F _rustcode rustcode");
    println!();
    println!("  # zsh");
    println!("  eval \"$(rustcode --help 2>&1 | grep -v '^error')\"  # placeholder");
    println!();
    println!("  # fish");
    println!("  rustcode --help | source  # placeholder");
    println!();
    println!("For full shell completion support, install clap_complete and rebuild.");
    println!("See: https://docs.rs/clap_complete/latest/clap_complete/");

    0
}

// ═════════════════════════════════════════════════════════════════════════════
// version
// ═════════════════════════════════════════════════════════════════════════════

/// `version` — Show version information.
///
/// Ported from: `packages/opencode/src/index.ts` — `.version("version", ...)`
fn cmd_version() {
    println!("rustcode {}", env!("CARGO_PKG_VERSION"));
    println!("Port of OpenCode (TypeScript/Bun) to Rust");
    println!();

    // Show build info
    println!(
        "Build profile: {}",
        option_env!("PROFILE").unwrap_or("unknown")
    );
    println!("Target: {}", option_env!("TARGET").unwrap_or("unknown"));
    println!(
        "Rustc: {}",
        option_env!("RUSTC_VERSION").unwrap_or("unknown")
    );

    // Show detected providers
    let providers = rustcode_core::providers::auto_detect_all();
    if providers.is_empty() {
        println!();
        println!("No providers detected. Set API key environment variables.");
    } else {
        println!();
        println!("Detected providers:");
        for p in &providers {
            println!("  - {}", p.provider_id());
        }
    }
}
