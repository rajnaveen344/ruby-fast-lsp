use crate::core::{SourceFileId, TextRange};
use ruby_prism::{Node, Visit};

/// Return one inner-to-outer syntax range chain for each requested byte offset.
pub fn selection_range_chains(
    file_id: SourceFileId,
    source: &str,
    byte_offsets: &[u32],
) -> Vec<Vec<TextRange>> {
    let parse_result = ruby_prism::parse(source.as_bytes());
    let root = parse_result.node();
    let mut collector = SelectionRangeCollector {
        file_id,
        byte_offsets,
        candidates: vec![Vec::new(); byte_offsets.len()],
    };
    collector.visit(&root);
    collector.finish()
}

struct SelectionRangeCollector<'a> {
    file_id: SourceFileId,
    byte_offsets: &'a [u32],
    candidates: Vec<Vec<TextRange>>,
}

impl SelectionRangeCollector<'_> {
    fn collect_node(&mut self, node: Node<'_>) {
        self.collect_location(&node.location());

        if let Some(call) = node.as_call_node() {
            if let Some(message) = call.message_loc() {
                self.collect_location(&message);
            }
        }
        if let Some(definition) = node.as_def_node() {
            self.collect_location(&definition.name_loc());
        }
        if let Some(write) = node.as_local_variable_write_node() {
            self.collect_location(&write.name_loc());
        }
        if let Some(write) = node.as_constant_write_node() {
            self.collect_location(&write.name_loc());
        }
        if let Some(write) = node.as_instance_variable_write_node() {
            self.collect_location(&write.name_loc());
        }
        if let Some(write) = node.as_class_variable_write_node() {
            self.collect_location(&write.name_loc());
        }
        if let Some(write) = node.as_global_variable_write_node() {
            self.collect_location(&write.name_loc());
        }
    }

    fn collect_location(&mut self, location: &ruby_prism::Location<'_>) {
        let start = u32_offset(location.start_offset());
        let end = u32_offset(location.end_offset());
        if start == end {
            return;
        }
        for (index, offset) in self.byte_offsets.iter().copied().enumerate() {
            if start <= offset && offset < end {
                self.candidates[index].push(TextRange::new(self.file_id, start, end));
            }
        }
    }

    fn finish(mut self) -> Vec<Vec<TextRange>> {
        for (index, ranges) in self.candidates.iter_mut().enumerate() {
            ranges.sort_by_key(|range| {
                (
                    range.end_byte - range.start_byte,
                    std::cmp::Reverse(range.start_byte),
                    range.end_byte,
                )
            });
            ranges.dedup();

            let mut chain = Vec::new();
            for range in ranges.drain(..) {
                let Some(inner) = chain.last() else {
                    chain.push(range);
                    continue;
                };
                if range.start_byte <= inner.start_byte && range.end_byte >= inner.end_byte {
                    chain.push(range);
                }
            }
            if chain.is_empty() {
                let offset = self.byte_offsets[index];
                chain.push(TextRange::new(self.file_id, offset, offset));
            }
            *ranges = chain;
        }
        self.candidates
    }
}

impl<'pr> Visit<'pr> for SelectionRangeCollector<'_> {
    fn visit_branch_node_enter(&mut self, node: Node<'pr>) {
        self.collect_node(node);
    }

    fn visit_leaf_node_enter(&mut self, node: Node<'pr>) {
        self.collect_node(node);
    }
}

fn u32_offset(offset: usize) -> u32 {
    u32::try_from(offset).expect(
        "INVARIANT VIOLATED: selection range byte offset exceeded u32. \
         This is a bug because TextRange currently stores u32 offsets. \
         Fix: widen TextRange before parsing files larger than u32::MAX bytes.",
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn call_chain_includes_message_expression_assignment_and_method() {
        let source = "def label(user)\n  result = user.profile.name\nend\n";
        let chains = selection_range_chains(SourceFileId(1), source, &[41]);

        assert_eq!(
            chains[0]
                .iter()
                .map(|range| (range.start_byte, range.end_byte))
                .collect::<Vec<_>>(),
            vec![(40, 44), (27, 44), (18, 44), (0, 48)]
        );
    }
}
