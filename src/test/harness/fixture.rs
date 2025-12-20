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

/// Extract type from <type>...</type> marker
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

/// Represents a tagged range with optional attributes.
#[derive(Debug, Clone)]
pub struct Tag {
    pub kind: String,
    pub range: Range,
    pub attributes: std::collections::HashMap<String, String>,
}

impl Tag {
    pub fn message(&self) -> Option<String> {
        self.attributes.get("message").cloned()
    }
}

/// Extracts all ranges marked with `<tag ...>...</tag>`.
/// Supports attributes like `<warn message="foo">`.
/// Also supports self-closing/point tags like `<hint label="type">` where end range == start range.
/// Accepting multiple tag names allows extracting mixed tags (e.g. err and warn) correctly in one pass.
pub fn extract_tags_with_attributes(text: &str, tag_names: &[&str]) -> (Vec<Tag>, String) {
    let open_tags: Vec<(String, String)> = tag_names
        .iter()
        .map(|name| (format!("<{}", name), name.to_string()))
        .collect();
    let close_tags: std::collections::HashMap<String, String> = tag_names
        .iter()
        .map(|name| (name.to_string(), format!("</{}>", name)))
        .collect();

    let mut tags = Vec::new();
    let mut clean_text = String::with_capacity(text.len());
    let mut remaining = text;
    let mut current_line = 0u32;
    let mut current_char = 0u32;

    while !remaining.is_empty() {
        // Find the earliest occurrence of any open tag
        let mut next_tag_idx = None;

        for (open_tag_start, name) in &open_tags {
            if let Some(idx) = remaining.find(open_tag_start) {
                // Check if it's a real match (followed by space or >)
                let char_after_start = remaining.chars().nth(idx + open_tag_start.len());
                let is_match = match char_after_start {
                    Some(' ') | Some('>') => true,
                    _ => false,
                };

                if is_match {
                    if next_tag_idx.map_or(true, |(min_idx, _, _)| idx < min_idx) {
                        next_tag_idx = Some((idx, open_tag_start.as_str(), name.as_str()));
                    }
                }
            }
        }

        if let Some((open_start_idx, open_tag_start, tag_name)) = next_tag_idx {
            // Append text before tag
            let before = &remaining[..open_start_idx];
            clean_text.push_str(before);
            for ch in before.chars() {
                if ch == '\n' {
                    current_line += 1;
                    current_char = 0;
                } else {
                    current_char += 1;
                }
            }

            // Parse opening tag looking for next '>'
            // We must respect quotes when looking for '>'
            let rest_from_start = &remaining[open_start_idx..];
            let mut tag_content_end = 0;
            let mut in_quote = false;
            let mut quote_char = '\0';

            for (i, ch) in rest_from_start.char_indices() {
                match ch {
                    '"' | '\'' => {
                        if !in_quote {
                            in_quote = true;
                            quote_char = ch;
                        } else if ch == quote_char {
                            in_quote = false;
                        }
                    }
                    '>' => {
                        if !in_quote {
                            tag_content_end = i;
                            break;
                        }
                    }
                    _ => {}
                }
            }

            if tag_content_end == 0 {
                panic!("Unclosed tag opening");
            }

            let tag_content = &rest_from_start[open_tag_start.len()..tag_content_end];

            // Parse attributes
            let mut attributes = std::collections::HashMap::new();
            let attr_regex = regex::Regex::new(r#"(\w+)="([^"]*)""#).unwrap();
            for cap in attr_regex.captures_iter(tag_content) {
                attributes.insert(cap[1].to_string(), cap[2].to_string());
            }

            // Check if it's a known range tag (has matching close tag) or point tag
            // We assume tags without a matching closing tag are point tags (start == end)
            // This is a heuristic: look ahead for the closing tag
            let content_after_open = &remaining[open_start_idx + tag_content_end + 1..];

            // Move past opening tag
            remaining = content_after_open;
            let start_pos = Position {
                line: current_line,
                character: current_char,
            };

            let close_tag = close_tags.get(tag_name).unwrap();
            if let Some(close_pos) = remaining.find(close_tag) {
                // It has a closing tag, treat as range
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

                tags.push(Tag {
                    kind: tag_name.to_string(),
                    range: Range {
                        start: start_pos,
                        end: Position {
                            line: current_line,
                            character: current_char,
                        },
                    },
                    attributes,
                });

                remaining = &remaining[close_pos + close_tag.len()..];
            } else {
                // No closing tag found, treat as point marker (start == end)
                tags.push(Tag {
                    kind: tag_name.to_string(),
                    range: Range {
                        start: start_pos,
                        end: start_pos,
                    },
                    attributes,
                });
                // 'remaining' was already advanced past the opening tag
            }
        } else {
            clean_text.push_str(remaining);
            break;
        }
    }

    (tags, clean_text)
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
