//! Standard LSP commands.

use std::path::PathBuf;

use anyhow::{anyhow, Result};
use lsp_types::{DocumentSymbolResponse, GotoDefinitionResponse, HoverContents, MarkedString, Url};

use super::CommandResult;
use crate::client::LspClient;

/// Open a file.
pub async fn open(client: &mut LspClient, args: &[&str]) -> Result<CommandResult> {
    if args.is_empty() {
        return Ok(CommandResult::Output("Usage: open <file>".to_string()));
    }

    let path = PathBuf::from(args[0]);
    let path = if path.is_absolute() {
        path
    } else {
        std::env::current_dir()?.join(path)
    };

    if !path.exists() {
        return Ok(CommandResult::Output(format!(
            "File not found: {}",
            path.display()
        )));
    }

    client.open_document(&path).await?;

    let line_count = client.current_line_count().unwrap_or(0);
    let file_name = client
        .current_file_name()
        .unwrap_or_else(|| "unknown".to_string());

    Ok(CommandResult::Output(format!(
        "Opened: {} ({} lines)",
        file_name, line_count
    )))
}

/// Get hover information.
/// Usage: hover <line> <col> [filename]
/// Or: hover <filename> <line> <col>
pub async fn hover(client: &mut LspClient, args: &[&str]) -> Result<CommandResult> {
    let (uri, line, col) = parse_position_with_file(client, args)?;

    match client.hover_in_document(&uri, line, col).await? {
        Some(hover) => {
            let content = format_hover_contents(&hover.contents);
            Ok(CommandResult::Output(content))
        }
        None => Ok(CommandResult::Output("No hover information".to_string())),
    }
}

/// Go to definition.
/// Usage: def <line> <col> [filename]
/// Or: def <filename> <line> <col>
pub async fn definition(client: &mut LspClient, args: &[&str]) -> Result<CommandResult> {
    let (uri, line, col) = parse_position_with_file(client, args)?;

    match client.definition_in_document(&uri, line, col).await? {
        Some(response) => {
            let output = format_definition_response(&response);
            Ok(CommandResult::Output(output))
        }
        None => Ok(CommandResult::Output("No definition found".to_string())),
    }
}

/// Find references.
/// Usage: refs <line> <col> [filename]
/// Or: refs <filename> <line> <col>
pub async fn references(client: &mut LspClient, args: &[&str]) -> Result<CommandResult> {
    let (uri, line, col) = parse_position_with_file(client, args)?;

    match client.references_in_document(&uri, line, col).await? {
        Some(locations) if !locations.is_empty() => {
            let mut output = format!("Found {} reference(s):\n", locations.len());
            for loc in &locations {
                let path = loc.uri.path();
                let file_name = path.rsplit('/').next().unwrap_or(path);
                output.push_str(&format!(
                    "  {}:{}:{}\n",
                    file_name,
                    loc.range.start.line + 1,
                    loc.range.start.character + 1
                ));
            }
            Ok(CommandResult::Output(output))
        }
        _ => Ok(CommandResult::Output("No references found".to_string())),
    }
}

/// Get completions.
/// Usage: complete <line> <col> [filename]
/// Or: complete <filename> <line> <col>
pub async fn completion(client: &mut LspClient, args: &[&str]) -> Result<CommandResult> {
    let (uri, line, col) = parse_position_with_file(client, args)?;

    match client.completion_in_document(&uri, line, col).await? {
        Some(response) => {
            let items = match response {
                lsp_types::CompletionResponse::Array(items) => items,
                lsp_types::CompletionResponse::List(list) => list.items,
            };

            if items.is_empty() {
                return Ok(CommandResult::Output("No completions".to_string()));
            }

            let mut output = format!("Completions ({}):\n", items.len());
            for item in items.iter().take(20) {
                let kind = item.kind.map(|k| format!("{:?}", k)).unwrap_or_default();
                output.push_str(&format!("  {} ({})\n", item.label, kind));
            }
            if items.len() > 20 {
                output.push_str(&format!("  ... and {} more\n", items.len() - 20));
            }
            Ok(CommandResult::Output(output))
        }
        None => Ok(CommandResult::Output("No completions".to_string())),
    }
}

/// Get document symbols.
/// Usage: symbols [filename]
pub async fn symbols(client: &mut LspClient, args: &[&str]) -> Result<CommandResult> {
    let uri = if args.is_empty() {
        client
            .current_document
            .clone()
            .ok_or_else(|| anyhow!("No document open"))?
    } else {
        resolve_file(client, args[0])?
    };

    match client.document_symbols_in_document(&uri).await? {
        Some(response) => {
            let output = format_symbols_response(&response);
            Ok(CommandResult::Output(output))
        }
        None => Ok(CommandResult::Output("No symbols found".to_string())),
    }
}

/// List all open files.
pub fn files(client: &LspClient) -> Result<CommandResult> {
    let docs = client.open_documents();

    if docs.is_empty() {
        return Ok(CommandResult::Output("No files open".to_string()));
    }

    let mut output = format!("Open files ({}):\n", docs.len());
    for doc in &docs {
        let current = client.current_document.as_ref() == Some(&doc.uri);
        let marker = if current { " *" } else { "" };
        output.push_str(&format!(
            "  {}{} ({} lines)\n",
            doc.file_name, marker, doc.line_count
        ));
    }
    output.push_str("\n(* = current file)");

    Ok(CommandResult::Output(output))
}

/// Switch to a different open file.
pub fn switch(client: &mut LspClient, args: &[&str]) -> Result<CommandResult> {
    if args.is_empty() {
        return Ok(CommandResult::Output(
            "Usage: switch <filename>".to_string(),
        ));
    }

    let filename = args[0];
    let matches = client.find_documents(filename);

    match matches.len() {
        0 => Ok(CommandResult::Output(format!(
            "No open file matches '{}'. Use 'open' to open a file first.",
            filename
        ))),
        1 => {
            client.set_current_document(&matches[0].uri);
            Ok(CommandResult::Output(format!(
                "Switched to: {}",
                matches[0].file_name
            )))
        }
        _ => {
            // Multiple matches - show options
            let mut output = format!(
                "Multiple files match '{}'. Please be more specific:\n",
                filename
            );
            for (i, doc) in matches.iter().enumerate() {
                output.push_str(&format!("  [{}] {}\n", i + 1, doc.file_name));
            }
            Ok(CommandResult::Output(output))
        }
    }
}

/// Close an open file.
pub async fn close(client: &mut LspClient, args: &[&str]) -> Result<CommandResult> {
    if args.is_empty() {
        // Close current file
        let uri = client
            .current_document
            .clone()
            .ok_or_else(|| anyhow!("No document open"))?;

        let file_name = uri
            .path_segments()
            .and_then(|mut s| s.next_back())
            .unwrap_or("unknown")
            .to_string();

        client.close_document(&uri).await?;

        let remaining = client.open_documents().len();
        if remaining > 0 {
            let current = client
                .current_file_name()
                .unwrap_or_else(|| "none".to_string());
            Ok(CommandResult::Output(format!(
                "Closed: {} (switched to: {})",
                file_name, current
            )))
        } else {
            Ok(CommandResult::Output(format!("Closed: {}", file_name)))
        }
    } else {
        let filename = args[0];
        let uri = resolve_file(client, filename)?;

        let file_name = uri
            .path_segments()
            .and_then(|mut s| s.next_back())
            .unwrap_or("unknown")
            .to_string();

        client.close_document(&uri).await?;

        Ok(CommandResult::Output(format!("Closed: {}", file_name)))
    }
}

/// Wait for background indexing to complete.
pub async fn wait(client: &mut LspClient, args: &[&str]) -> Result<CommandResult> {
    let timeout: u64 = if args.is_empty() {
        60 // Default 60 seconds
    } else {
        args[0].parse().unwrap_or(60)
    };

    println!(
        "Waiting for indexing to complete (timeout: {}s)...",
        timeout
    );

    match client.wait_for_indexing(timeout).await {
        Ok(()) => Ok(CommandResult::Output("Indexing complete.".to_string())),
        Err(e) => Ok(CommandResult::Output(format!("Warning: {}", e))),
    }
}

/// Show current file info.
pub fn current_file(client: &LspClient) -> Result<CommandResult> {
    match &client.current_document {
        Some(uri) => {
            let line_count = client.current_line_count().unwrap_or(0);
            Ok(CommandResult::Output(format!(
                "Current file: {} ({} lines)",
                uri.path(),
                line_count
            )))
        }
        None => Ok(CommandResult::Output("No file open".to_string())),
    }
}

/// Parse line, column, and optional filename from args.
/// Supports multiple formats:
///   - <line> <col>             - uses current file
///   - <line> <col> <filename>  - uses specified file
///   - <filename> <line> <col>  - uses specified file
fn parse_position_with_file(client: &LspClient, args: &[&str]) -> Result<(Url, u32, u32)> {
    if args.len() < 2 {
        return Err(anyhow!(
            "Usage: <line> <col> [filename] or <filename> <line> <col>"
        ));
    }

    // Try to detect format
    let (uri, line_str, col_str) = if args.len() >= 3 {
        // Three or more args - either "line col filename" or "filename line col"
        if args[0].parse::<u32>().is_ok() {
            // First arg is a number, so format is: line col filename
            let uri = resolve_file(client, args[2])?;
            (uri, args[0], args[1])
        } else {
            // First arg is not a number, so format is: filename line col
            let uri = resolve_file(client, args[0])?;
            (uri, args[1], args[2])
        }
    } else {
        // Two args - must be line col, use current file
        let uri = client
            .current_document
            .clone()
            .ok_or_else(|| anyhow!("No document open. Open a file first or specify filename."))?;
        (uri, args[0], args[1])
    };

    let line: u32 = line_str
        .parse()
        .map_err(|_| anyhow!("Invalid line number: '{}'", line_str))?;
    let col: u32 = col_str
        .parse()
        .map_err(|_| anyhow!("Invalid column number: '{}'", col_str))?;

    // Convert from 1-indexed (user-friendly) to 0-indexed (LSP)
    let line = line.saturating_sub(1);
    let col = col.saturating_sub(1);

    Ok((uri, line, col))
}

/// Resolve a filename to a URI.
/// If multiple files match, returns an error with options.
fn resolve_file(client: &LspClient, filename: &str) -> Result<Url> {
    let matches = client.find_documents(filename);

    match matches.len() {
        0 => Err(anyhow!(
            "No open file matches '{}'. Use 'open' to open the file first.",
            filename
        )),
        1 => Ok(matches[0].uri.clone()),
        _ => {
            // Multiple matches - build error with options
            let mut msg = format!(
                "Multiple files match '{}'. Please be more specific:\n",
                filename
            );
            for doc in &matches {
                msg.push_str(&format!("  - {}\n", doc.file_name));
            }
            Err(anyhow!("{}", msg))
        }
    }
}

/// Format hover contents for display.
fn format_hover_contents(contents: &HoverContents) -> String {
    match contents {
        HoverContents::Scalar(marked) => format_marked_string(marked),
        HoverContents::Array(arr) => arr
            .iter()
            .map(format_marked_string)
            .collect::<Vec<_>>()
            .join("\n---\n"),
        HoverContents::Markup(markup) => markup.value.clone(),
    }
}

fn format_marked_string(marked: &MarkedString) -> String {
    match marked {
        MarkedString::String(s) => s.clone(),
        MarkedString::LanguageString(ls) => format!("```{}\n{}\n```", ls.language, ls.value),
    }
}

/// Format definition response for display.
fn format_definition_response(response: &GotoDefinitionResponse) -> String {
    match response {
        GotoDefinitionResponse::Scalar(location) => {
            let path = location.uri.path();
            let file_name = path.rsplit('/').next().unwrap_or(path);
            format!(
                "Definition: {}:{}:{}",
                file_name,
                location.range.start.line + 1,
                location.range.start.character + 1
            )
        }
        GotoDefinitionResponse::Array(locations) => {
            if locations.is_empty() {
                return "No definition found".to_string();
            }
            let mut output = format!("Found {} definition(s):\n", locations.len());
            for loc in locations {
                let path = loc.uri.path();
                let file_name = path.rsplit('/').next().unwrap_or(path);
                output.push_str(&format!(
                    "  {}:{}:{}\n",
                    file_name,
                    loc.range.start.line + 1,
                    loc.range.start.character + 1
                ));
            }
            output
        }
        GotoDefinitionResponse::Link(links) => {
            if links.is_empty() {
                return "No definition found".to_string();
            }
            let mut output = format!("Found {} definition(s):\n", links.len());
            for link in links {
                let path = link.target_uri.path();
                let file_name = path.rsplit('/').next().unwrap_or(path);
                output.push_str(&format!(
                    "  {}:{}:{}\n",
                    file_name,
                    link.target_range.start.line + 1,
                    link.target_range.start.character + 1
                ));
            }
            output
        }
    }
}

/// Format document symbols response for display.
fn format_symbols_response(response: &DocumentSymbolResponse) -> String {
    match response {
        DocumentSymbolResponse::Flat(symbols) => {
            if symbols.is_empty() {
                return "No symbols found".to_string();
            }
            let mut output = format!("Symbols ({}):\n", symbols.len());
            for sym in symbols {
                output.push_str(&format!(
                    "  {:?} {} (line {})\n",
                    sym.kind,
                    sym.name,
                    sym.location.range.start.line + 1
                ));
            }
            output
        }
        DocumentSymbolResponse::Nested(symbols) => {
            if symbols.is_empty() {
                return "No symbols found".to_string();
            }
            let mut output = String::from("Symbols:\n");
            format_nested_symbols(&mut output, symbols, 0);
            output
        }
    }
}

fn format_nested_symbols(output: &mut String, symbols: &[lsp_types::DocumentSymbol], depth: usize) {
    let indent = "  ".repeat(depth + 1);
    for sym in symbols {
        output.push_str(&format!(
            "{}{:?} {} (line {})\n",
            indent,
            sym.kind,
            sym.name,
            sym.range.start.line + 1
        ));
        if let Some(children) = &sym.children {
            format_nested_symbols(output, children, depth + 1);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use lsp_types::{
        LanguageString, Location, MarkupContent, MarkupKind, Position, Range, SymbolInformation,
        SymbolKind,
    };

    #[test]
    fn test_format_hover_contents_markup() {
        let contents = HoverContents::Markup(MarkupContent {
            kind: MarkupKind::Markdown,
            value: "```ruby\ndef foo\n```".to_string(),
        });
        let result = format_hover_contents(&contents);
        assert_eq!(result, "```ruby\ndef foo\n```");
    }

    #[test]
    fn test_format_hover_contents_scalar_string() {
        let contents = HoverContents::Scalar(MarkedString::String("Hello".to_string()));
        let result = format_hover_contents(&contents);
        assert_eq!(result, "Hello");
    }

    #[test]
    fn test_format_hover_contents_scalar_language_string() {
        let contents = HoverContents::Scalar(MarkedString::LanguageString(LanguageString {
            language: "ruby".to_string(),
            value: "def foo".to_string(),
        }));
        let result = format_hover_contents(&contents);
        assert_eq!(result, "```ruby\ndef foo\n```");
    }

    #[test]
    fn test_format_hover_contents_array() {
        let contents = HoverContents::Array(vec![
            MarkedString::String("First".to_string()),
            MarkedString::String("Second".to_string()),
        ]);
        let result = format_hover_contents(&contents);
        assert_eq!(result, "First\n---\nSecond");
    }

    #[test]
    fn test_format_definition_response_scalar() {
        let response = GotoDefinitionResponse::Scalar(Location {
            uri: Url::parse("file:///path/to/user.rb").unwrap(),
            range: Range {
                start: Position {
                    line: 9,
                    character: 4,
                },
                end: Position {
                    line: 9,
                    character: 10,
                },
            },
        });
        let result = format_definition_response(&response);
        assert_eq!(result, "Definition: user.rb:10:5");
    }

    #[test]
    fn test_format_definition_response_array_empty() {
        let response = GotoDefinitionResponse::Array(vec![]);
        let result = format_definition_response(&response);
        assert_eq!(result, "No definition found");
    }

    #[test]
    fn test_format_definition_response_array_multiple() {
        let response = GotoDefinitionResponse::Array(vec![
            Location {
                uri: Url::parse("file:///path/to/a.rb").unwrap(),
                range: Range::default(),
            },
            Location {
                uri: Url::parse("file:///path/to/b.rb").unwrap(),
                range: Range::default(),
            },
        ]);
        let result = format_definition_response(&response);
        assert!(result.contains("Found 2 definition(s):"));
        assert!(result.contains("a.rb:1:1"));
        assert!(result.contains("b.rb:1:1"));
    }

    #[test]
    fn test_format_symbols_response_flat() {
        let response = DocumentSymbolResponse::Flat(vec![SymbolInformation {
            name: "User".to_string(),
            kind: SymbolKind::CLASS,
            location: Location {
                uri: Url::parse("file:///path/to/user.rb").unwrap(),
                range: Range {
                    start: Position {
                        line: 0,
                        character: 0,
                    },
                    end: Position {
                        line: 10,
                        character: 0,
                    },
                },
            },
            tags: None,
            #[allow(deprecated)]
            deprecated: None,
            container_name: None,
        }]);
        let result = format_symbols_response(&response);
        assert!(result.contains("Symbols (1):"));
        assert!(result.contains("Class User (line 1)"));
    }

    #[test]
    fn test_format_symbols_response_empty() {
        let response = DocumentSymbolResponse::Flat(vec![]);
        let result = format_symbols_response(&response);
        assert_eq!(result, "No symbols found");
    }

    #[test]
    fn test_line_number_conversion() {
        // Verify 1-indexed user input is converted to 0-indexed
        // Line 10 user input -> line 9 LSP
        let line: u32 = 10;
        let converted = line.saturating_sub(1);
        assert_eq!(converted, 9);

        // Line 1 user input -> line 0 LSP
        let line: u32 = 1;
        let converted = line.saturating_sub(1);
        assert_eq!(converted, 0);

        // Line 0 user input -> line 0 LSP (no underflow)
        let line: u32 = 0;
        let converted = line.saturating_sub(1);
        assert_eq!(converted, 0);
    }
}
