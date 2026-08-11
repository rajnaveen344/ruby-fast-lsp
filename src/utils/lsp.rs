//! LSP utility functions

use ruby_analysis::core::{SourcePosition, SourceRange, TextRange};
use ruby_analysis::indexer::RubyDocument;
use tower_lsp::lsp_types::{Location, Position, Range};

pub const fn source_position(position: Position) -> SourcePosition {
    SourcePosition::new(position.line, position.character)
}

pub fn lsp_position(position: SourcePosition) -> Position {
    Position::new(position.line, position.character)
}

pub const fn source_range(range: Range) -> SourceRange {
    SourceRange::new(source_position(range.start), source_position(range.end))
}

pub fn lsp_range(range: SourceRange) -> Range {
    Range::new(lsp_position(range.start), lsp_position(range.end))
}

pub fn text_range(document: &RubyDocument, range: Range) -> TextRange {
    document.source_range_to_text_range(source_range(range))
}

pub fn lsp_text_range(document: &RubyDocument, range: TextRange) -> Range {
    lsp_range(document.text_range_to_source_range(range))
}

pub fn lsp_text_location(document: &RubyDocument, range: TextRange) -> Location {
    Location::new(document.uri.clone(), lsp_text_range(document, range))
}

/// Remove duplicate locations from a vector.
pub fn deduplicate_locations(locations: Vec<Location>) -> Vec<Location> {
    let mut unique = Vec::new();
    for loc in locations {
        if !unique
            .iter()
            .any(|existing: &Location| existing.uri == loc.uri && existing.range == loc.range)
        {
            unique.push(loc);
        }
    }
    unique
}
