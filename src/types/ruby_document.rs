use lsp_types::{Position, Range, Url};
use ruby_prism::Location;
use std::cmp;

/// A document representation that handles conversions between byte offsets and LSP positions
#[derive(Clone)]
pub struct RubyDocument {
    pub uri: Url,
    pub content: String,
    pub version: i32,
    /// Byte offset at the start of each line (last element is total content length)
    line_offsets: Vec<usize>,
}

impl RubyDocument {
    /// Creates a new document with the given URI, content, and version
    pub fn new(uri: Url, content: String, version: i32) -> Self {
        let mut doc = Self {
            uri,
            content,
            version,
            line_offsets: Vec::new(),
        };
        doc.compute_line_offsets();
        doc
    }

    /// Updates document content and version, recomputing line offsets
    pub fn update_content(&mut self, content: String, version: i32) {
        self.content = content;
        self.version = version;
        self.compute_line_offsets();
    }

    /// Computes byte offsets at the start of each line
    fn compute_line_offsets(&mut self) {
        self.line_offsets = vec![0]; // First line starts at offset 0

        let mut offset = 0;
        for c in self.content.chars() {
            offset += c.len_utf8();
            if c == '\n' {
                self.line_offsets.push(offset);
            }
        }

        // Ensure the last element is the total content length
        if self.line_offsets.last() != Some(&self.content.len()) {
            self.line_offsets.push(self.content.len());
        }
    }

    /// Converts a byte offset to an LSP Position (line, character)
    pub fn offset_to_position(&self, offset: usize) -> Position {
        let offset = cmp::min(offset, self.content.len());

        // Find line containing this offset
        let line_index = match self.line_offsets.binary_search(&offset) {
            Ok(exact) => exact,      // Offset is exactly at line start
            Err(after) => after - 1, // Offset is within a line
        };

        // Count UTF-8 characters from line start to offset
        let line_start = self.line_offsets[line_index];
        let character = self.content[line_start..offset].chars().count();

        Position::new(line_index as u32, character as u32)
    }

    /// Converts an LSP Position to a byte offset
    pub fn position_to_offset(&self, position: Position) -> usize {
        let line = position.line as usize;

        // Handle out-of-bounds line
        if line >= self.line_offsets.len() - 1 {
            return self.content.len();
        }

        let line_start = self.line_offsets[line];
        let line_end = self.line_offsets[line + 1];
        let target_char = position.character as usize;

        // Count characters until we reach the target position
        let mut chars_seen = 0;
        let mut byte_offset = 0;

        for c in self.content[line_start..line_end].chars() {
            if chars_seen >= target_char || c == '\n' {
                break;
            }
            chars_seen += 1;
            byte_offset += c.len_utf8();
        }

        line_start + byte_offset
    }

    /// Converts a ruby_prism Location to an LSP Range
    pub fn location_to_range(&self, location: &Location) -> Range {
        Range::new(
            self.offset_to_position(location.start_offset()),
            self.offset_to_position(location.end_offset()),
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Helper to create a test document with sample content
    fn create_test_document() -> RubyDocument {
        let content = "def foo\n  puts 'Hello'\nend\n";
        let uri = Url::parse("file:///test.rb").unwrap();
        RubyDocument::new(uri, content.to_string(), 1)
    }

    #[test]
    fn test_compute_line_offsets() {
        let doc = create_test_document();
        // Expected: [0, 8, 23, 27] representing start offsets of each line
        assert_eq!(doc.line_offsets, vec![0, 8, 23, 27]);
    }

    #[test]
    fn test_offset_to_position() {
        let doc = create_test_document();

        // Various positions in the document
        assert_eq!(doc.offset_to_position(0), Position::new(0, 0)); // Start of document
        assert_eq!(doc.offset_to_position(5), Position::new(0, 5)); // Middle of first line
        assert_eq!(doc.offset_to_position(8), Position::new(1, 0)); // Start of second line
        assert_eq!(doc.offset_to_position(15), Position::new(1, 7)); // Middle of second line
        assert_eq!(doc.offset_to_position(23), Position::new(2, 0)); // Start of third line
        assert_eq!(doc.offset_to_position(27), Position::new(3, 0)); // End of document

        // Edge case: beyond document end
        assert_eq!(doc.offset_to_position(100), Position::new(3, 0)); // Clamped to end
    }

    #[test]
    fn test_position_to_offset() {
        let doc = create_test_document();

        // Various positions in the document
        assert_eq!(doc.position_to_offset(Position::new(0, 0)), 0); // Start of document
        assert_eq!(doc.position_to_offset(Position::new(0, 5)), 5); // Middle of first line
        assert_eq!(doc.position_to_offset(Position::new(1, 0)), 8); // Start of second line
        assert_eq!(doc.position_to_offset(Position::new(1, 7)), 15); // Middle of second line

        // Edge cases
        assert_eq!(doc.position_to_offset(Position::new(0, 100)), 7); // Beyond line length
        assert_eq!(doc.position_to_offset(Position::new(100, 0)), 27); // Beyond document
    }

    #[test]
    fn test_update_content() {
        let mut doc = create_test_document();
        let new_content = "class Foo\n  def bar\n  end\nend";
        doc.update_content(new_content.to_string(), 2);

        // Verify content and version updated
        assert_eq!(doc.version, 2);
        assert_eq!(doc.content, new_content);

        // Verify line offsets recomputed correctly
        assert_eq!(doc.line_offsets, vec![0, 10, 20, 26, 29]);

        // Verify position conversions with new content
        assert_eq!(doc.offset_to_position(15), Position::new(1, 5));
        assert_eq!(doc.position_to_offset(Position::new(2, 2)), 22);
    }

    #[test]
    fn test_location_to_range() {
        let doc = create_test_document();

        // Create a test location by using the node() method from ParseResult
        let parsed_doc = ruby_prism::parse(doc.content.as_bytes());
        let node = parsed_doc.node();

        // Create a range covering "puts" in the second line (byte offsets 10-14)
        // Line 1 starts at offset 8, "puts" starts at offset 10 (2 chars in)
        let start_pos = doc.offset_to_position(0); // Position of 'p' in "puts"
        let end_pos = doc.offset_to_position(26); // Position after 's' in "puts"
        let expected_range = Range::new(start_pos, end_pos);

        // Test with the location from a real node
        let def_node_loc = node.location();
        let actual_range = doc.location_to_range(&def_node_loc);

        assert_eq!(expected_range, actual_range);
    }

    #[test]
    fn test_empty_document() {
        let uri = Url::parse("file:///empty.rb").unwrap();
        let doc = RubyDocument::new(uri, "".to_string(), 1);

        // Empty document should have just one line offset
        assert_eq!(doc.line_offsets, vec![0]);

        // Test basic position conversions
        assert_eq!(doc.offset_to_position(0), Position::new(0, 0));
        assert_eq!(doc.position_to_offset(Position::new(0, 0)), 0);

        // Test out of bounds cases
        assert_eq!(doc.offset_to_position(100), Position::new(0, 0));
        assert_eq!(doc.position_to_offset(Position::new(100, 100)), 0);
    }

    #[test]
    fn test_multibyte_characters() {
        let uri = Url::parse("file:///multibyte.rb").unwrap();
        let content = "def 你好\n  puts '世界'\nend\n";
        let doc = RubyDocument::new(uri, content.to_string(), 1);

        // Calculate expected byte offsets
        let first_line = "def 你好\n";
        let second_line = "  puts '世界'\n";
        let third_line = "end\n";

        let first_line_bytes = first_line.len();
        let second_line_bytes = second_line.len();

        // Verify line offsets are computed correctly
        assert_eq!(doc.line_offsets[0], 0);
        assert_eq!(doc.line_offsets[1], first_line_bytes);
        assert_eq!(doc.line_offsets[2], first_line_bytes + second_line_bytes);
        assert_eq!(
            doc.line_offsets[3],
            first_line_bytes + second_line_bytes + third_line.len()
        );

        // Test position conversion with multibyte characters
        assert_eq!(doc.offset_to_position(7), Position::new(0, 5)); // Middle of "你好"
        assert_eq!(doc.position_to_offset(Position::new(0, 5)), 7); // Same position
    }
}
