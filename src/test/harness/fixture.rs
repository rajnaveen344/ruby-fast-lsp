//! Core fixture utilities - marker extraction and server setup.

use std::sync::Arc;

use parking_lot::RwLock;
use tower_lsp::lsp_types::{InitializeParams, Position, Range, Url};
use tower_lsp::LanguageServer;

use crate::server::RubyLanguageServer;
use crate::types::ruby_document::RubyDocument;

/// Cursor marker indicating where the LSP action should be triggered.
pub const CURSOR_MARKER: &str = "$0";

/// Parsed inline fixture with all markers extracted.
#[derive(Debug)]
pub struct InlineFixture {
    /// The clean source code with all markers removed.
    pub content: String,
    /// The cursor position (from $0 marker).
    pub cursor: Position,
    /// Expected definition ranges (from <def>...</def> markers).
    pub def_ranges: Vec<Range>,
    /// Expected reference ranges (from <ref>...</ref> markers).
    pub ref_ranges: Vec<Range>,
    /// Expected type string (from <type>...</type> marker).
    pub expected_type: Option<String>,
}

/// Parse an inline fixture string, extracting all markers.
pub fn parse_fixture(text: &str) -> InlineFixture {
    let (cursor, text_no_cursor) = extract_cursor(text);
    let (def_ranges, text_no_defs) = extract_tags(&text_no_cursor, "def");
    let (ref_ranges, text_no_refs) = extract_tags(&text_no_defs, "ref");
    let (expected_type, content) = extract_type_tag(&text_no_refs);

    InlineFixture {
        content,
        cursor,
        def_ranges,
        ref_ranges,
        expected_type,
    }
}

/// Extracts the cursor position from text.
pub fn extract_cursor(text: &str) -> (Position, String) {
    let cursor_byte_pos = text
        .find(CURSOR_MARKER)
        .expect("Text should contain cursor marker '$0'");

    let mut clean_text = String::with_capacity(text.len() - CURSOR_MARKER.len());
    clean_text.push_str(&text[..cursor_byte_pos]);
    clean_text.push_str(&text[cursor_byte_pos + CURSOR_MARKER.len()..]);

    let before_cursor = &text[..cursor_byte_pos];
    let line = before_cursor.matches('\n').count() as u32;
    let last_newline = before_cursor.rfind('\n').map(|i| i + 1).unwrap_or(0);
    let character = (cursor_byte_pos - last_newline) as u32;

    (Position { line, character }, clean_text)
}

/// Extracts all ranges marked with `<tag>...</tag>` pairs.
pub fn extract_tags(text: &str, tag: &str) -> (Vec<Range>, String) {
    let open_tag = format!("<{}>", tag);
    let close_tag = format!("</{}>", tag);

    let mut ranges = Vec::new();
    let mut clean_text = String::with_capacity(text.len());
    let mut remaining = text;
    let mut current_line = 0u32;
    let mut current_char = 0u32;

    while !remaining.is_empty() {
        if let Some(open_pos) = remaining.find(&open_tag) {
            let before = &remaining[..open_pos];
            clean_text.push_str(before);

            for ch in before.chars() {
                if ch == '\n' {
                    current_line += 1;
                    current_char = 0;
                } else {
                    current_char += 1;
                }
            }

            let start_pos = Position {
                line: current_line,
                character: current_char,
            };

            remaining = &remaining[open_pos + open_tag.len()..];

            if let Some(close_pos) = remaining.find(&close_tag) {
                let content = &remaining[..close_pos];
                clean_text.push_str(content);

                for ch in content.chars() {
                    if ch == '\n' {
                        current_line += 1;
                        current_char = 0;
                    } else {
                        current_char += 1;
                    }
                }

                ranges.push(Range {
                    start: start_pos,
                    end: Position {
                        line: current_line,
                        character: current_char,
                    },
                });

                remaining = &remaining[close_pos + close_tag.len()..];
            } else {
                panic!("Unmatched <{}> tag", tag);
            }
        } else {
            clean_text.push_str(remaining);
            break;
        }
    }

    (ranges, clean_text)
}

/// Extracts expected type from `<type>...</type>` marker.
fn extract_type_tag(text: &str) -> (Option<String>, String) {
    let open_tag = "<type>";
    let close_tag = "</type>";

    if let Some(open_pos) = text.find(open_tag) {
        let after_open = &text[open_pos + open_tag.len()..];
        if let Some(close_pos) = after_open.find(close_tag) {
            let type_str = after_open[..close_pos].to_string();
            let mut clean = String::with_capacity(text.len());
            clean.push_str(&text[..open_pos]);
            clean.push_str(&after_open[close_pos + close_tag.len()..]);
            return (Some(type_str), clean);
        }
    }
    (None, text.to_string())
}

/// Virtual URI used for inline fixtures.
fn virtual_uri() -> Url {
    Url::parse("file:///inline_test.rb").expect("Invalid virtual URI")
}

/// Sets up a server with an inline fixture loaded.
pub async fn setup_with_fixture(content: &str) -> (RubyLanguageServer, Url) {
    let server = RubyLanguageServer::default();
    let _ = server.initialize(InitializeParams::default()).await;

    let uri = virtual_uri();

    let document = RubyDocument::new(uri.clone(), content.to_string(), 1);
    server
        .docs
        .lock()
        .insert(uri.clone(), Arc::new(RwLock::new(document)));

    // Index the document
    {
        use crate::indexer::file_processor::{FileProcessor, ProcessingOptions};
        let indexer = FileProcessor::new(server.index.clone());
        let options = ProcessingOptions {
            index_definitions: true,
            index_references: true,
            resolve_mixins: true,
            include_local_vars: true,
        };
        let _ = indexer.process_file(&uri, content, &server, options);
    }

    (server, uri)
}

/// Compare ranges exactly.
pub fn ranges_match(actual: &Range, expected: &Range) -> bool {
    actual.start.line == expected.start.line
        && actual.start.character == expected.start.character
        && actual.end.line == expected.end.line
        && actual.end.character == expected.end.character
}
