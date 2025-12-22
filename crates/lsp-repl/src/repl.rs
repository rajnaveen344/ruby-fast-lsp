//! Interactive REPL loop with rustyline.

use anyhow::Result;
use rustyline::error::ReadlineError;
use rustyline::history::DefaultHistory;
use rustyline::{Config, Editor};

use crate::client::LspClient;
use crate::commands::{self, CommandResult};

/// Run the interactive REPL loop.
pub async fn run(mut client: LspClient) -> Result<()> {
    // Print welcome message
    println!(
        "Connected to: {}",
        client.server_name.as_deref().unwrap_or("LSP Server")
    );

    if !client.custom_commands.is_empty() {
        println!(
            "Discovered {} custom command(s) from server.",
            client.custom_commands.len()
        );
    }

    println!("Type 'help' for available commands, 'quit' to exit.\n");

    // Set up rustyline with history
    let config = Config::builder()
        .history_ignore_space(true)
        .max_history_size(1000)?
        .build();

    let mut rl: Editor<(), DefaultHistory> = Editor::with_config(config)?;

    // Try to load history from file
    let history_file = dirs::home_dir()
        .map(|h| h.join(".lsp_repl_history"))
        .unwrap_or_else(|| std::path::PathBuf::from(".lsp_repl_history"));

    let _ = rl.load_history(&history_file);

    loop {
        // Build prompt with current file info
        let prompt = if let Some(name) = client.current_file_name() {
            format!("{}> ", name)
        } else {
            "> ".to_string()
        };

        match rl.readline(&prompt) {
            Ok(line) => {
                let line = line.trim();
                if line.is_empty() {
                    continue;
                }

                // Add to history
                let _ = rl.add_history_entry(line);

                // Execute command
                match commands::execute(&mut client, line).await {
                    Ok(CommandResult::Output(output)) => {
                        println!("{}", output);
                    }
                    Ok(CommandResult::Empty) => {}
                    Ok(CommandResult::Quit) => {
                        println!("Goodbye!");
                        break;
                    }
                    Ok(CommandResult::NotFound(cmd)) => {
                        println!(
                            "Unknown command: '{}'. Type 'help' for available commands.",
                            cmd
                        );
                    }
                    Err(e) => {
                        println!("Error: {}", e);
                    }
                }
            }
            Err(ReadlineError::Interrupted) => {
                println!("^C");
                continue;
            }
            Err(ReadlineError::Eof) => {
                println!("Goodbye!");
                break;
            }
            Err(err) => {
                println!("Error: {:?}", err);
                break;
            }
        }
    }

    // Save history
    let _ = rl.save_history(&history_file);

    // Gracefully shutdown the server
    client.shutdown().await?;

    Ok(())
}
