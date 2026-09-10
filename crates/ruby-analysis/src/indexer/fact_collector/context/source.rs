use crate::core::TextRange;
use crate::indexer::fact_collector::FactCollector;
use crate::indexer::yard::parser::{CommentLineInfo, YardParser};
use ruby_fast_lsp_extension_api::SourceRange;

impl FactCollector {
    pub fn text_range_from_offsets(&self, start: usize, end: usize) -> TextRange {
        let start_byte = u32::try_from(start).expect(
            "INVARIANT VIOLATED: diagnostic start offset exceeded u32. \
             This is a bug because ruby-analysis::core TextRange currently stores u32 offsets. \
             Fix: widen TextRange offsets before indexing files larger than u32::MAX bytes.",
        );
        let end_byte = u32::try_from(end).expect(
            "INVARIANT VIOLATED: diagnostic end offset exceeded u32. \
             This is a bug because ruby-analysis::core TextRange currently stores u32 offsets. \
             Fix: widen TextRange offsets before indexing files larger than u32::MAX bytes.",
        );
        TextRange::new(self.document.analysis_file_id(), start_byte, end_byte)
    }

    pub fn body_text_range(
        &self,
        body_location: Option<ruby_prism::Location>,
        node_location: &ruby_prism::Location,
    ) -> TextRange {
        if let Some(body_location) = body_location {
            self.document.prism_location_to_text_range(&body_location)
        } else {
            self.document.prism_location_to_text_range(node_location)
        }
    }

    /// Extract YARD documentation from comments preceding a method definition using Prism comments.
    pub fn extract_doc_comments(
        &self,
        method_start: usize,
    ) -> Option<crate::indexer::yard::types::YardMethodDoc> {
        // Find the first comment that starts AFTER or AT method_start.
        // We want the ones BEFORE it.
        let idx = self
            .document
            .get_comments()
            .partition_point(|c| c.0 < method_start);

        if idx == 0 {
            return None;
        }

        let mut comment_indices = Vec::new();
        let mut current_idx = idx - 1;

        // Check last comment is attached to method
        let (_, end) = self.document.get_comments()[current_idx];
        let range_between = &self.document.content[end..method_start];
        if !range_between.trim().is_empty() {
            return None;
        }
        comment_indices.push(current_idx);

        // Walk backwards to collect contiguous comment block
        while current_idx > 0 {
            let prev_idx = current_idx - 1;
            let (_, prev_end) = self.document.get_comments()[prev_idx];
            let (curr_start, _) = self.document.get_comments()[current_idx];

            let range_between = &self.document.content[prev_end..curr_start];
            if !range_between.trim().is_empty() {
                break;
            }
            comment_indices.push(prev_idx);
            current_idx = prev_idx;
        }

        comment_indices.reverse(); // Now in order top-down

        let mut line_infos = Vec::new();
        for &i in &comment_indices {
            let (start, end) = self.document.get_comments()[i];
            let raw_content = &self.document.content[start..end];
            let trimmed = raw_content.trim();
            // Prism comments include the #.
            let content = trimmed.trim_start_matches('#').trim_start();

            // Calculate precise location info for diagnostics
            // We need the position of the *content*, so find where it starts relative to the comment start
            let hash_offset = raw_content.find('#').unwrap_or(0);

            // Find content offset. If empty content, point to end of hash
            let content_offset_in_raw = if content.is_empty() {
                hash_offset + 1
            } else {
                raw_content.find(content).unwrap_or(hash_offset + 1)
            };

            let abs_content_start = start + content_offset_in_raw;
            let abs_pos = self.document.offset_to_position(abs_content_start);
            // YardParser uses line_length for diagnostic range end calculation in some cases
            // (end char is usually start char + content len, but passed as line_length in parser?)
            // Actually parser uses:
            // start: Position { line: line_info.line_number, character: line_info.content_start_char }
            // end: Position { line: line_info.line_number, character: line_info.line_length }
            // So line_length should be the COLUMN index of the end of the line (or length)
            let abs_end_pos = self.document.offset_to_position(end);

            line_infos.push(CommentLineInfo {
                content,
                line_number: abs_pos.line,
                content_start_char: abs_pos.character,
                line_length: abs_end_pos.character,
            });
        }

        let doc = YardParser::parse_lines(&line_infos, true);

        if doc.has_type_info() || doc.description.is_some() {
            Some(doc)
        } else {
            None
        }
    }
}

pub(in crate::indexer::fact_collector) fn source_range(
    visitor: &FactCollector,
    location: &ruby_prism::Location,
) -> SourceRange {
    let range = visitor.document.prism_location_to_source_range(location);
    SourceRange {
        start: ruby_fast_lsp_extension_api::SourcePosition {
            line: range.start.line,
            character: range.start.character,
        },
        end: ruby_fast_lsp_extension_api::SourcePosition {
            line: range.end.line,
            character: range.end.character,
        },
    }
}

pub(in crate::indexer::fact_collector) fn u32_offset(offset: usize) -> u32 {
    u32::try_from(offset).expect(
        "INVARIANT VIOLATED: source byte offset exceeded u32. \
         This is a bug because analysis facts currently store u32 ranges. \
         Fix: widen TextRange offsets before indexing files larger than u32::MAX bytes.",
    )
}
