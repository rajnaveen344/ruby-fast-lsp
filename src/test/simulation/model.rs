//! # The Model (Oracle)
//!
//! A simplified, infallible representation of truth.
//! The Model doesn't parse Ruby or build ASTs - it just tracks what text
//! should be in each file. If the LSP's internal buffer ever differs from
//! the Model, something is broken.

use std::collections::HashMap;
use tower_lsp::lsp_types::{Position, Range};

/// The Model: What the LSP's state SHOULD be.
///
/// This is intentionally simple - just a HashMap of filename to content.
/// The simplicity is the point: it's impossible to get wrong.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct LspModel {
    /// Open documents: filename -> content
    pub files: HashMap<String, DocumentState>,
}

/// State of a single document in the model
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct DocumentState {
    /// The full text content
    pub content: String,
    /// Document version (incremented on each change)
    pub version: i32,
}

impl DocumentState {
    pub fn new(content: String) -> Self {
        Self {
            content,
            version: 1,
        }
    }

    /// Apply a text edit to the document
    pub fn apply_edit(&mut self, range: &Range, new_text: &str) {
        let new_content = apply_edit_to_string(&self.content, range, new_text);
        self.content = new_content;
        self.version += 1;
    }
}

impl LspModel {
    pub fn new() -> Self {
        Self::default()
    }

    /// Check if a file is open
    pub fn is_open(&self, filename: &str) -> bool {
        self.files.contains_key(filename)
    }

    /// Get content of a file (if open)
    pub fn get_content(&self, filename: &str) -> Option<&str> {
        self.files.get(filename).map(|d| d.content.as_str())
    }

    /// Get list of open files
    pub fn open_files(&self) -> Vec<String> {
        self.files.keys().cloned().collect()
    }

    /// Open a document
    pub fn open(&mut self, filename: String, content: String) {
        self.files.insert(filename, DocumentState::new(content));
    }

    /// Close a document
    pub fn close(&mut self, filename: &str) {
        self.files.remove(filename);
    }

    /// Apply an edit to a document
    pub fn edit(&mut self, filename: &str, range: &Range, new_text: &str) {
        if let Some(doc) = self.files.get_mut(filename) {
            doc.apply_edit(range, new_text);
        }
    }

    /// Get the number of lines in a file
    pub fn line_count(&self, filename: &str) -> usize {
        self.files
            .get(filename)
            .map(|d| d.content.lines().count().max(1))
            .unwrap_or(0)
    }

    /// Get the length of a specific line
    pub fn line_length(&self, filename: &str, line: usize) -> usize {
        self.files
            .get(filename)
            .and_then(|d| d.content.lines().nth(line))
            .map(|l| l.len())
            .unwrap_or(0)
    }
}

/// Apply a text edit to a string, returning the new string.
///
/// This is the core text synchronization logic - it must match exactly
/// what the LSP server does internally.
fn apply_edit_to_string(content: &str, range: &Range, new_text: &str) -> String {
    let lines: Vec<&str> = content.lines().collect();

    // Handle empty content
    if lines.is_empty() {
        return new_text.to_string();
    }

    // Calculate byte offsets
    let start_offset = position_to_offset(content, &range.start);
    let end_offset = position_to_offset(content, &range.end);

    // Apply the edit
    let mut result = String::new();
    result.push_str(&content[..start_offset]);
    result.push_str(new_text);
    result.push_str(&content[end_offset..]);

    result
}

/// Convert an LSP Position to a byte offset in the content
fn position_to_offset(content: &str, position: &Position) -> usize {
    let mut offset = 0;

    for (current_line, line) in content.lines().enumerate() {
        if current_line == position.line as usize {
            // Found the line, add character offset
            let char_offset = (position.character as usize).min(line.len());
            return offset + char_offset;
        }
        offset += line.len() + 1; // +1 for newline
    }

    // If position is beyond content, return end
    content.len()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_apply_edit_insert() {
        let content = "hello world";
        let range = Range {
            start: Position {
                line: 0,
                character: 5,
            },
            end: Position {
                line: 0,
                character: 5,
            },
        };
        let result = apply_edit_to_string(content, &range, " beautiful");
        assert_eq!(result, "hello beautiful world");
    }

    #[test]
    fn test_apply_edit_replace() {
        let content = "hello world";
        let range = Range {
            start: Position {
                line: 0,
                character: 6,
            },
            end: Position {
                line: 0,
                character: 11,
            },
        };
        let result = apply_edit_to_string(content, &range, "rust");
        assert_eq!(result, "hello rust");
    }

    #[test]
    fn test_apply_edit_multiline() {
        let content = "line1\nline2\nline3";
        let range = Range {
            start: Position {
                line: 1,
                character: 0,
            },
            end: Position {
                line: 1,
                character: 5,
            },
        };
        let result = apply_edit_to_string(content, &range, "replaced");
        assert_eq!(result, "line1\nreplaced\nline3");
    }

    #[test]
    fn test_model_open_close() {
        let mut model = LspModel::new();
        assert!(!model.is_open("test.rb"));

        model.open("test.rb".to_string(), "class Foo\nend".to_string());
        assert!(model.is_open("test.rb"));
        assert_eq!(model.get_content("test.rb"), Some("class Foo\nend"));

        model.close("test.rb");
        assert!(!model.is_open("test.rb"));
    }

    #[test]
    fn test_model_edit() {
        let mut model = LspModel::new();
        model.open("test.rb".to_string(), "class Foo\nend".to_string());

        let range = Range {
            start: Position {
                line: 0,
                character: 6,
            },
            end: Position {
                line: 0,
                character: 9,
            },
        };
        model.edit("test.rb", &range, "Bar");

        assert_eq!(model.get_content("test.rb"), Some("class Bar\nend"));
    }
}
