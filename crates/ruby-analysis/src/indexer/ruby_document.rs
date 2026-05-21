use crate::core::{RubyType, SourceFileId, TextRange};
use ruby_prism::Location as PrismLocation;
use tower_lsp::lsp_types::{InlayHint, Location as LspLocation, Position, Range, Url};

use crate::{LVScopeId, SourceDocument, VariableScopes};

/// A document representation that handles conversions between byte offsets and LSP positions
#[derive(Clone)]
pub struct RubyDocument {
    pub uri: Url,
    pub content: String,
    pub version: i32,
    /// The version at which this document was last indexed (None if never indexed)
    pub indexed_version: Option<i32>,
    source: SourceDocument,

    /// Inlay hints in the document for modules, classes, methods, etc.
    inlay_hints: Vec<InlayHint>,

    /// Variable scopes for local variable tracking (definitions, references, types)
    pub variable_scopes: VariableScopes,
}

impl RubyDocument {
    /// Creates a new document with the given URI, content, and version
    pub fn new(uri: Url, content: String, version: i32) -> Self {
        Self::with_analysis_file_id(uri, content, version, SourceFileId(0))
    }

    pub fn with_analysis_file_id(
        uri: Url,
        content: String,
        version: i32,
        analysis_file_id: SourceFileId,
    ) -> Self {
        Self {
            uri,
            source: SourceDocument::new(content.clone(), analysis_file_id),
            content,
            version,
            indexed_version: None,
            inlay_hints: Vec::new(),
            variable_scopes: VariableScopes::new(),
        }
    }

    /// Parses the document content and returns a Prism ParseResult
    pub fn parse(&self) -> ruby_prism::ParseResult<'_> {
        ruby_prism::parse(self.content.as_bytes())
    }

    pub fn get_comments(&self) -> &[(usize, usize)] {
        self.source.comments()
    }

    /// Updates document content and version, recomputing line offsets
    /// Clears variable scopes since they will be re-indexed.
    pub fn update(&mut self, content: String, version: i32) {
        self.source.update(content.clone());
        self.content = content;
        self.version = version;
        self.variable_scopes = VariableScopes::new();
        self.compute_inlay_hints();
    }

    /// Source file id for type facts emitted from this document.
    pub fn analysis_file_id(&self) -> SourceFileId {
        self.source.file_id()
    }

    pub fn set_analysis_file_id(&mut self, analysis_file_id: SourceFileId) {
        self.source.set_file_id(analysis_file_id);
    }

    /// Converts a byte offset to an LSP Position (line, character)
    pub fn offset_to_position(&self, offset: usize) -> Position {
        let (line, character) = self.source.offset_to_line_character(offset);
        Position::new(line, character)
    }

    /// Converts an LSP Position to a byte offset
    pub fn position_to_offset(&self, position: Position) -> usize {
        self.source
            .line_character_to_offset(position.line, position.character)
    }

    pub fn position_to_analysis_offset(&self, position: Position) -> u32 {
        u32::try_from(self.position_to_offset(position)).expect(
            "INVARIANT VIOLATED: LSP position offset exceeded u32. \
             This is a bug because ruby-analysis::core TextRange currently stores u32 offsets. \
             Fix: widen TextRange offsets before indexing files larger than u32::MAX bytes.",
        )
    }

    /// Converts a ruby_prism Location to an LSP Range
    pub fn prism_location_to_lsp_range(&self, location: &PrismLocation) -> Range {
        Range::new(
            self.offset_to_position(location.start_offset()),
            self.offset_to_position(location.end_offset()),
        )
    }

    pub fn prism_location_to_text_range(&self, location: &PrismLocation) -> TextRange {
        self.source.prism_location_to_text_range(location)
    }

    pub fn lsp_range_to_text_range(&self, range: Range) -> TextRange {
        TextRange::new(
            self.analysis_file_id(),
            self.position_to_analysis_offset(range.start),
            self.position_to_analysis_offset(range.end),
        )
    }

    pub fn text_range_to_lsp_range(&self, range: TextRange) -> Range {
        assert_eq!(
            range.file_id,
            self.analysis_file_id(),
            "INVARIANT VIOLATED: text range belongs to a different source file. \
             This is a bug because RubyDocument can only convert ranges from its own file. \
             Fix: route cross-file ranges through the owning document."
        );
        Range::new(
            self.offset_to_position(range.start_byte as usize),
            self.offset_to_position(range.end_byte as usize),
        )
    }

    pub fn text_range_to_lsp_location(&self, range: TextRange) -> LspLocation {
        LspLocation::new(self.uri.clone(), self.text_range_to_lsp_range(range))
    }

    pub fn find_scope_for_variable_at(&self, name: &str, position: Position) -> Option<LVScopeId> {
        self.variable_scopes.find_scope_for_variable_at(
            name,
            self.analysis_file_id(),
            self.position_to_analysis_offset(position),
        )
    }

    pub fn scope_at_position(&self, position: Position) -> Option<LVScopeId> {
        self.variable_scopes.scope_at_position(
            self.analysis_file_id(),
            self.position_to_analysis_offset(position),
        )
    }

    pub fn variable_type_at_position(
        &self,
        name: &str,
        scope_id: LVScopeId,
        position: Position,
    ) -> Option<&RubyType> {
        self.variable_scopes.get_type_at_position(
            name,
            scope_id,
            self.analysis_file_id(),
            self.position_to_analysis_offset(position),
        )
    }

    pub fn local_variable_definition_range_before(
        &self,
        name: &str,
        byte_offset: u32,
    ) -> Option<TextRange> {
        let file_id = self.analysis_file_id();
        let scope_id = self
            .variable_scopes
            .find_scope_for_variable_at(name, file_id, byte_offset)
            .or_else(|| self.variable_scopes.scope_at_position(file_id, byte_offset))?;
        let (_sid, variable) = self.variable_scopes.find_variable(name, scope_id)?;
        if variable.definition_location.start_byte < byte_offset {
            Some(variable.definition_location)
        } else {
            None
        }
    }

    pub fn local_variable_reference_ranges_at(
        &self,
        name: &str,
        byte_offset: u32,
    ) -> Vec<TextRange> {
        let file_id = self.analysis_file_id();
        let Some(scope_id) =
            self.variable_scopes
                .find_scope_for_variable_at(name, file_id, byte_offset)
        else {
            return Vec::new();
        };

        self.variable_scopes
            .find_rename_targets(name, scope_id)
            .into_iter()
            .map(|target| target.location)
            .collect()
    }

    pub fn prism_location_to_lsp_location(&self, location: &PrismLocation) -> LspLocation {
        LspLocation::new(self.uri.clone(), self.prism_location_to_lsp_range(location))
    }

    /// Computes inlay hints for the document (now only clears old hints)
    pub fn compute_inlay_hints(&mut self) {
        // Clear previous structural hints - type hints are managed separately by FactCollector
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

    /// Returns a mutable reference to variable scopes for building during indexing
    pub fn variable_scopes_mut(&mut self) -> &mut VariableScopes {
        &mut self.variable_scopes
    }

    /// Get variable scopes for queries
    pub fn variable_scopes(&self) -> &VariableScopes {
        &self.variable_scopes
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
        assert_eq!(doc.source.line_offsets(), &[0, 8, 23, 27]);
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
        assert_eq!(doc.source.line_offsets(), &[0, 10, 20, 26, 29]);

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
        assert_eq!(doc.source.line_offsets(), &[0]);

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
        assert_eq!(doc.source.line_offsets()[0], 0);
        assert_eq!(doc.source.line_offsets()[1], first_line_bytes);
        assert_eq!(
            doc.source.line_offsets()[2],
            first_line_bytes + second_line_bytes
        );
        assert_eq!(
            doc.source.line_offsets()[3],
            first_line_bytes + second_line_bytes + third_line.len()
        );

        // Test position conversion with multibyte characters
        assert_eq!(doc.offset_to_position(7), Position::new(0, 5)); // Middle of "你好"
        assert_eq!(doc.position_to_offset(Position::new(0, 5)), 7); // Same position
    }
}
