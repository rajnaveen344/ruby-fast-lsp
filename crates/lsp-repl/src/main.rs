//! lsp-repl: A generic LSP debugging REPL.
//!
//! This tool allows you to interact with any language server via a command-line REPL.
//! It supports standard LSP operations (hover, definition, references, etc.) and
//! can discover custom debug commands from the server via the `$/listCommands` method.

mod client;
mod commands;
mod protocol;
mod repl;
mod transport;

use std::path::PathBuf;

use anyhow::Result;
use clap::Parser;

use client::LspClient;

/// A generic LSP debugging REPL.
#[derive(Parser, Debug)]
#[command(name = "lsp-repl")]
#[command(author, version, about, long_about = None)]
struct Args {
    /// The language server command to spawn (e.g., "ruby-fast-lsp --stdio")
    #[arg(required = true)]
    server: String,

    /// The workspace root directory
    #[arg(short, long)]
    workspace: Option<PathBuf>,

    /// Enable verbose logging
    #[arg(short, long)]
    verbose: bool,
}

#[tokio::main]
async fn main() -> Result<()> {
    let args = Args::parse();

    // Initialize logging
    if args.verbose {
        env_logger::Builder::from_env(env_logger::Env::default().default_filter_or("debug")).init();
    } else {
        env_logger::Builder::from_env(env_logger::Env::default().default_filter_or("warn")).init();
    }

    // Resolve workspace path
    let workspace = args.workspace.map(|p| {
        if p.is_absolute() {
            p
        } else {
            std::env::current_dir().unwrap().join(p)
        }
    });

    println!("Connecting to: {}", args.server);
    if let Some(ref ws) = workspace {
        println!("Workspace: {}", ws.display());
    }

    // Spawn and connect to the language server
    let client = LspClient::new(&args.server, workspace).await?;

    // Run the REPL
    repl::run(client).await?;

    Ok(())
}

