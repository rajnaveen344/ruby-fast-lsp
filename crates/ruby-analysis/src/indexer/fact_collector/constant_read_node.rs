use crate::core::{ReferenceCandidate, RubyConstant, TextRange};
use log::trace;
use ruby_prism::ConstantReadNode;

use super::FactCollector;

impl FactCollector {
    pub fn process_constant_read_node_entry(&mut self, node: &ConstantReadNode) {
        let name = crate::utf8_str(node.name().as_slice());
        let constant = match RubyConstant::new(name) {
            Ok(c) => c,
            Err(_) => {
                trace!("Skipping invalid constant name: {}", name);
                return;
            }
        };

        let range = self.text_range_from_prism_location(&node.location(), "constant reference");
        self.reference_candidates.push(ReferenceCandidate::constant(
            range,
            vec![constant],
            self.scope_tracker.get_ns_stack(),
        ));
    }

    pub fn process_constant_read_node_exit(&mut self, _node: &ConstantReadNode) {}

    pub(super) fn text_range_from_prism_location(
        &self,
        location: &ruby_prism::Location,
        kind: &str,
    ) -> TextRange {
        TextRange::new(
            self.document.analysis_file_id(),
            u32_text_range_offset(location.start_offset(), kind, TextRangeBoundary::Start),
            u32_text_range_offset(location.end_offset(), kind, TextRangeBoundary::End),
        )
    }

    pub(super) fn text_range_from_lsp_range(
        &self,
        range: tower_lsp::lsp_types::Range,
        kind: &str,
    ) -> TextRange {
        TextRange::new(
            self.document.analysis_file_id(),
            u32_text_range_offset(
                self.document.position_to_offset(range.start),
                kind,
                TextRangeBoundary::Start,
            ),
            u32_text_range_offset(
                self.document.position_to_offset(range.end),
                kind,
                TextRangeBoundary::End,
            ),
        )
    }
}

#[derive(Clone, Copy)]
enum TextRangeBoundary {
    Start,
    End,
}

fn u32_text_range_offset(offset: usize, kind: &str, boundary: TextRangeBoundary) -> u32 {
    match u32::try_from(offset) {
        Ok(offset) => offset,
        Err(_) => {
            let boundary = match boundary {
                TextRangeBoundary::Start => "start",
                TextRangeBoundary::End => "end",
            };
            panic!(
                "INVARIANT VIOLATED: {kind} {boundary} offset exceeded u32. \
                 This is a bug because ruby-analysis::core TextRange currently stores u32 offsets. \
                 Fix: widen TextRange offsets before indexing files larger than u32::MAX bytes."
            )
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn text_range_offset_context_is_typed_and_only_formatted_on_failure() {
        assert_eq!(
            u32_text_range_offset(7, "constant reference", TextRangeBoundary::Start),
            7
        );

        let overflow = usize::try_from(u64::from(u32::MAX) + 1)
            .expect("test platform must represent a byte offset larger than u32::MAX");
        let panic = std::panic::catch_unwind(|| {
            u32_text_range_offset(
                overflow,
                "method diagnostic candidate",
                TextRangeBoundary::End,
            )
        })
        .expect_err("an offset larger than u32::MAX must fail loudly");
        let message = panic
            .downcast_ref::<String>()
            .map(String::as_str)
            .or_else(|| panic.downcast_ref::<&str>().copied())
            .expect("overflow panic must carry a readable invariant message");
        assert!(message.contains("method diagnostic candidate end offset exceeded u32"));
    }
}
