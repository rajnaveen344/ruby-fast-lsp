use log::debug;
use ruby_prism::Location as PrismLocation;
use std::{cmp, collections::BTreeMap};
use tower_lsp::lsp_types::{InlayHint, Location as LspLocation, Position, Range, Url};

use crate::{indexer::entry::Entry, types::scope::LVScopeId};

/// A document representation that handles conversions between byte offsets and LSP positions
#[derive(Clone)]
pub struct RubyDocument {
    pub uri: Url,
    pub content: String,
    pub version: i32,
    /// The version at which this document was last indexed (None if never indexed)
    pub indexed_version: Option<i32>,
    /// Byte offset at the start of each line (last element is total content length)
    /// Eg. def foo\n  puts 'Hello'\nend\n
    ///     ^ -> 0   ^ -> 8          ^ -> 23
    ///     line_offsets = [0, 8, 23, 27]
    line_offsets: Vec<usize>,

    /// Inlay hints in the document for modules, classes, methods, etc.
    inlay_hints: Vec<InlayHint>,

    /// Local variables in the document (definitions)
    lvars: BTreeMap<LVScopeId, Vec<Entry>>,

    /// Local variable references (keyed by (scope_id, variable_name) for proper scoping)
    lvar_references: std::collections::HashMap<(LVScopeId, ustr::Ustr), Vec<LspLocation>>,

    /// Comments in the document (start_offset, end_offset)
    comments: Vec<(usize, usize)>,
}

impl RubyDocument {
    /// Creates a new document with the given URI, content, and version
    pub fn new(uri: Url, content: String, version: i32) -> Self {
        let comments = parse_comments(&content);
        let mut doc = Self {
            uri,
            content,
            version,
            indexed_version: None,
            line_offsets: Vec::new(),
            inlay_hints: Vec::new(),
            lvars: BTreeMap::new(),
            lvar_references: std::collections::HashMap::new(),
            comments,
        };
        doc.compute_line_offsets();
        doc
    }

    /// Parses the document content and returns a Prism ParseResult
    pub fn parse(&self) -> ruby_prism::ParseResult<'_> {
        ruby_prism::parse(self.content.as_bytes())
    }

    pub fn get_comments(&self) -> &Vec<(usize, usize)> {
        &self.comments
    }

    /// Updates document content and version, recomputing line offsets
    /// Also clears lvars and lvar_references since they will be re-indexed
    /// Note: comments are NOT cleared here because they are typically populated
    /// immediately after by the indexer. However, we should probably clear them
    /// to be safe, assuming the caller will re-populate them.
    pub fn update(&mut self, content: String, version: i32) {
        self.comments = parse_comments(&content);
        self.content = content;
        self.version = version;
        self.lvars.clear();
        self.lvar_references.clear();
        self.compute_line_offsets();
        self.compute_inlay_hints();
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

        let mut byte_offset = 0;

        for (chars_seen, c) in self.content[line_start..line_end].chars().enumerate() {
            if chars_seen >= target_char || c == '\n' {
                break;
            }
            byte_offset += c.len_utf8();
        }

        line_start + byte_offset
    }

    /// Converts a ruby_prism Location to an LSP Range
    pub fn prism_location_to_lsp_range(&self, location: &PrismLocation) -> Range {
        Range::new(
            self.offset_to_position(location.start_offset()),
            self.offset_to_position(location.end_offset()),
        )
    }

    pub fn prism_location_to_lsp_location(&self, location: &PrismLocation) -> LspLocation {
        LspLocation::new(self.uri.clone(), self.prism_location_to_lsp_range(location))
    }

    /// Computes inlay hints for the document (now only clears old hints)
    pub fn compute_inlay_hints(&mut self) {
        // Clear previous structural hints - type hints are managed separately by IndexVisitor
        self.inlay_hints.clear();
    }

    /// Get inlay hints for the document
    pub fn get_inlay_hints(&self) -> Vec<InlayHint> {
        self.inlay_hints.clone()
    }

    /// Add an inlay hint to the document
    pub fn add_inlay_hint(&mut self, hint: InlayHint) {
        self.inlay_hints.push(hint);
    }

    /// Clear all inlay hints from the document
    pub fn clear_inlay_hints(&mut self) {
        self.inlay_hints.clear();
    }

    /// Set multiple inlay hints for the document
    pub fn set_inlay_hints(&mut self, hints: Vec<InlayHint>) {
        self.inlay_hints = hints;
    }

    /// Get all hints (both inlay and type hints) combined
    pub fn get_all_hints(&self) -> Vec<InlayHint> {
        self.inlay_hints.clone()
    }

    pub fn add_local_var_entry(&mut self, scope_id: LVScopeId, entry: Entry) {
        debug!("Adding local variable entry with scope id: {:?}", scope_id);
        self.lvars.entry(scope_id).or_default().push(entry);
    }

    pub fn get_local_var_entries(&self, scope_id: LVScopeId) -> Option<&Vec<Entry>> {
        self.lvars.get(&scope_id)
    }

    /// Returns a reference to the entire lvars map for iteration
    pub fn get_all_lvars(&self) -> &std::collections::BTreeMap<LVScopeId, Vec<Entry>> {
        &self.lvars
    }

    /// Check if a local variable with the given name exists in any of the provided scope IDs
    /// Returns the scope_id where the variable was found, or None if not found
    pub fn find_local_var_scope(&self, name: &str, scope_ids: &[LVScopeId]) -> Option<LVScopeId> {
        use crate::indexer::entry::entry_kind::EntryKind;

        // Search from innermost to outermost scope
        for &scope_id in scope_ids.iter().rev() {
            if let Some(entries) = self.lvars.get(&scope_id) {
                for entry in entries {
                    if let EntryKind::LocalVariable(data) = &entry.kind {
                        if &data.name == name {
                            return Some(scope_id);
                        }
                    }
                }
            }
        }
        None
    }

    /// Search ALL scopes in the document for a local variable by name (fallback method)
    /// Returns the first matching location found
    pub fn find_local_var_by_name(&self, name: &str) -> Option<tower_lsp::lsp_types::Location> {
        use crate::indexer::entry::entry_kind::EntryKind;

        for (_scope_id, entries) in &self.lvars {
            for entry in entries {
                if let EntryKind::LocalVariable(data) = &entry.kind {
                    if &data.name == name {
                        // Convert CompactLocation to Location using document URI
                        return Some(tower_lsp::lsp_types::Location {
                            uri: self.uri.clone(),
                            range: entry.location.range,
                        });
                    }
                }
            }
        }
        None
    }

    /// Add a reference to a local variable (scoped by scope_id)
    pub fn add_lvar_reference(
        &mut self,
        scope_id: LVScopeId,
        name: ustr::Ustr,
        location: LspLocation,
    ) {
        self.lvar_references
            .entry((scope_id, name))
            .or_default()
            .push(location);
    }

    /// Get all references to a local variable by name within specific scopes
    pub fn get_lvar_references(&self, name: &str, scope_ids: &[LVScopeId]) -> Vec<LspLocation> {
        let uname = ustr::ustr(name);
        let mut refs = Vec::new();
        for &scope_id in scope_ids {
            if let Some(locations) = self.lvar_references.get(&(scope_id, uname)) {
                refs.extend(locations.iter().cloned());
            }
        }
        refs
    }

    /// Clear all local variable references (called before re-indexing)
    pub fn clear_lvar_references(&mut self) {
        self.lvar_references.clear();
    }

    /// Get all local variable references (for merging into another document)
    pub fn get_all_lvar_references(
        &self,
    ) -> &std::collections::HashMap<(LVScopeId, ustr::Ustr), Vec<LspLocation>> {
        &self.lvar_references
    }

    /// Count total number of local variable entries (for diagnostics)
    pub fn count_lvars(&self) -> usize {
        self.lvars.values().map(|v| v.len()).sum()
    }
}

/// Helper to parse comments from content
fn parse_comments(content: &str) -> Vec<(usize, usize)> {
    let parse_result = ruby_prism::parse(content.as_bytes());
    let mut comments = Vec::new();
    for comment in parse_result.comments() {
        let loc = comment.location();
        comments.push((loc.start_offset(), loc.end_offset()));
    }
    comments
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
        doc.update(new_content.to_string(), 2);

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

        let start_pos = doc.offset_to_position(0); // Position of 'd' in "def"
        let end_pos = doc.offset_to_position(26); // Position after 'd' in "end"
        let expected_range = Range::new(start_pos, end_pos);

        // Test with the location from a real node
        let def_node_loc = node.location();
        let actual_range = doc.prism_location_to_lsp_range(&def_node_loc);

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
