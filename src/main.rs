#![forbid(unsafe_code)]
#![allow(dead_code, unused_imports)]
#![warn(clippy::all)]

//! rustcode — AI-powered development tool.
//!
//! A Rust port of the OpenCode TypeScript/Bun AI coding agent.

use clap::{Parser, Subcommand};
use rustcode_core::config::Config;
use std::path::PathBuf;

/// AI-powered development tool
#[derive(Parser)]
#[command(
    name = "rustcode",
    version,
    about = "AI-powered development tool — Rust port of OpenCode"
)]
struct Cli {
    /// Print logs to stderr
    #[arg(long)]
    print_logs: bool,

    /// Log level
    #[arg(long, value_name = "LEVEL", default_value = "INFO")]
    log_level: String,

    /// Run without external plugins
    #[arg(long)]
    pure: bool,

    #[command(subcommand)]
    command: Option<Commands>,
}

#[derive(Subcommand)]
enum Commands {
    /// Start an interactive session (default)
    Run {
        /// Initial prompt
        #[arg(trailing_var_arg = true)]
        prompt: Vec<String>,
    },
    /// Start the HTTP server
    Serve {
        /// Port to listen on
        #[arg(long, default_value = "3000")]
        port: u16,
    },
    /// List sessions
    Session {
        /// Show all sessions
        #[arg(long)]
        all: bool,
    },
    /// Show version info
    Version,
}

fn main() -> anyhow::Result<()> {
    let cli = Cli::parse();

    // Initialize logging
    let filter = if cli.print_logs {
        &cli.log_level
    } else {
        "off"
    };
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| tracing_subscriber::EnvFilter::new(filter)),
        )
        .init();

    tracing::info!("rustcode starting");

    match cli.command {
        Some(Commands::Run { prompt }) => {
            let config = Config::load().unwrap_or_default();
            tracing::info!("Config loaded: {} providers", config.provider.len());

            if prompt.is_empty() {
                println!("Interactive mode not yet implemented. Use: rustcode run <prompt>");
            } else {
                let prompt_text = prompt.join(" ");
                println!("Prompt: {prompt_text}");
                println!("LLM integration not yet implemented.");
            }
        }
        Some(Commands::Serve { port }) => {
            println!("Server mode on port {port} — not yet implemented.");
        }
        Some(Commands::Session { all }) => {
            let _config = Config::load().unwrap_or_default();
            let data_dir = Config::data_dir().unwrap_or_else(|_| PathBuf::from("."));
            println!("Sessions directory: {:?}", data_dir.join("storage"));
            println!("Show all: {all}");
        }
        Some(Commands::Version) => {
            println!("rustcode {}", env!("CARGO_PKG_VERSION"));
            println!("Port of OpenCode (TypeScript/Bun) to Rust");
        }
        None => {
            // Default: show help
            Cli::parse_from(["rustcode", "--help"]);
        }
    }

    Ok(())
}
