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
            name,
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
            u32_offset(location.start_offset(), &format!("{kind} start offset")),
            u32_offset(location.end_offset(), &format!("{kind} end offset")),
        )
    }

    pub(super) fn text_range_from_lsp_range(
        &self,
        range: tower_lsp::lsp_types::Range,
        kind: &str,
    ) -> TextRange {
        TextRange::new(
            self.document.analysis_file_id(),
            u32_offset(
                self.document.position_to_offset(range.start),
                &format!("{kind} start offset"),
            ),
            u32_offset(
                self.document.position_to_offset(range.end),
                &format!("{kind} end offset"),
            ),
        )
    }
}

fn u32_offset(offset: usize, message: &str) -> u32 {
    u32::try_from(offset).unwrap_or_else(|_| {
        panic!(
            "INVARIANT VIOLATED: {message} exceeded u32. \
             This is a bug because ruby-analysis::core TextRange currently stores u32 offsets. \
             Fix: widen TextRange offsets before indexing files larger than u32::MAX bytes."
        )
    })
}
