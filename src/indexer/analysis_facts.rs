use ruby_analysis_core::{FullyQualifiedName, ReferenceFact, SourceFileId, TextRange};
use tower_lsp::lsp_types::{Location, Range};

use crate::types::ruby_document::RubyDocument;

pub fn collect_reference_facts_from_locations<'a>(
    document: &RubyDocument,
    file_id: SourceFileId,
    references: impl Iterator<Item = &'a (FullyQualifiedName, Location, Option<FullyQualifiedName>)>,
) -> Vec<ReferenceFact> {
    references
        .map(|(target, location, caller)| {
            ReferenceFact::new(
                target.clone(),
                text_range_from_lsp_range(document, file_id, location.range, "reference"),
                caller.clone(),
            )
        })
        .collect()
}

fn byte_offset_u32(byte_offset: usize, message: &str) -> u32 {
    u32::try_from(byte_offset).unwrap_or_else(|_| {
        panic!(
            "INVARIANT VIOLATED: {message}. \
             This is a bug because ruby-analysis-core TextRange currently stores u32 offsets. \
             Fix: widen TextRange offsets before indexing files larger than u32::MAX bytes."
        )
    })
}

fn text_range_from_lsp_range(
    document: &RubyDocument,
    file_id: SourceFileId,
    range: Range,
    kind: &str,
) -> TextRange {
    let start_byte = byte_offset_u32(
        document.position_to_offset(range.start),
        &format!("{kind} start offset exceeded u32"),
    );
    let end_byte = byte_offset_u32(
        document.position_to_offset(range.end),
        &format!("{kind} end offset exceeded u32"),
    );
    TextRange::new(file_id, start_byte, end_byte)
}
