//! Command handling for the LSP REPL.

pub mod dynamic;
pub mod standard;

use anyhow::Result;

use crate::client::LspClient;

/// Result of executing a command.
pub enum CommandResult {
    /// Command executed successfully with output
    Output(String),
    /// Command executed successfully with no output
    Empty,
    /// Request to quit the REPL
    Quit,
    /// Command not found
    NotFound(String),
}

/// Execute a command string.
pub async fn execute(client: &mut LspClient, input: &str) -> Result<CommandResult> {
    let input = input.trim();
    if input.is_empty() {
        return Ok(CommandResult::Empty);
    }

    let parts: Vec<&str> = input.split_whitespace().collect();
    let cmd = parts[0];
    let args = &parts[1..];

    // Try built-in commands first
    match cmd {
        "help" | "?" => Ok(CommandResult::Output(help_text(client))),
        "quit" | "exit" | "q" => Ok(CommandResult::Quit),
        "open" | "o" => standard::open(client, args).await,
        "close" => standard::close(client, args).await,
        "hover" | "h" => standard::hover(client, args).await,
        "def" | "d" | "definition" => standard::definition(client, args).await,
        "refs" | "r" | "references" => standard::references(client, args).await,
        "complete" | "c" | "completion" => standard::completion(client, args).await,
        "symbols" | "s" => standard::symbols(client, args).await,
        "file" | "f" => standard::current_file(client),
        "files" | "ls" => standard::files(client),
        "switch" | "sw" => standard::switch(client, args),
        "wait" | "w" => standard::wait(client, args).await,
        _ => {
            // Try custom commands from server
            dynamic::execute(client, cmd, args).await
        }
    }
}

/// Generate help text including custom commands.
fn help_text(client: &LspClient) -> String {
    let mut help = String::new();

    help.push_str("File Commands:\n");
    help.push_str("  open <file>                    Open a file for analysis (alias: o)\n");
    help.push_str("  close [file]                   Close a file (current if not specified)\n");
    help.push_str("  files                          List all open files (alias: ls)\n");
    help.push_str("  switch <filename>              Switch to an open file (alias: sw)\n");
    help.push_str("  file                           Show current file info (alias: f)\n");
    help.push('\n');
    help.push_str("Position Commands (work on current file or specify filename):\n");
    help.push_str("  hover <line> <col> [file]      Get hover information (alias: h)\n");
    help.push_str("  def <line> <col> [file]        Go to definition (alias: d)\n");
    help.push_str("  refs <line> <col> [file]       Find references (alias: r)\n");
    help.push_str("  complete <line> <col> [file]   Get completions (alias: c)\n");
    help.push_str("  symbols [file]                 Document symbols (alias: s)\n");
    help.push('\n');
    help.push_str("  Note: Position commands accept: <line> <col> [file] OR <file> <line> <col>\n");
    help.push('\n');
    help.push_str("Other:\n");
    help.push_str("  wait [timeout]                 Wait for indexing to complete (alias: w)\n");
    help.push_str("  help                           Show this help (alias: ?)\n");
    help.push_str("  quit                           Exit the REPL (alias: q, exit)\n");

    if !client.custom_commands.is_empty() {
        help.push_str("\nServer Commands");
        if let Some(name) = &client.server_name {
            help.push_str(&format!(" ({}):\n", name));
        } else {
            help.push_str(":\n");
        }

        for cmd in &client.custom_commands {
            let params_str: String = cmd
                .params
                .iter()
                .map(|p| {
                    if p.required {
                        format!("<{}>", p.name)
                    } else {
                        format!("[{}]", p.name)
                    }
                })
                .collect::<Vec<_>>()
                .join(" ");

            help.push_str(&format!(
                "  {:20} {}\n",
                format!("{} {}", cmd.name, params_str).trim(),
                cmd.description
            ));
        }
    }

    help
}
