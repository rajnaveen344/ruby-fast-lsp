use ruby_prism::BlockNode;

use crate::LVScopeKind;

use super::IdentifierVisitor;

impl IdentifierVisitor {
    pub fn process_block_node_entry(&mut self, node: &BlockNode) {
        if self.is_result_set() || !self.is_position_in_location(&node.location()) {
            return;
        }

        self.scope_tracker.push_scope_kind(LVScopeKind::Block);
    }

    pub fn process_block_node_exit(&mut self, node: &BlockNode) {
        if self.is_result_set() || !self.is_position_in_location(&node.location()) {
            return;
        }

        let (body_start, body_end) = match node.body() {
            Some(body) => {
                let location = body.location();
                (location.start_offset(), location.end_offset())
            }
            None => {
                let location = node.location();
                (location.start_offset(), location.end_offset())
            }
        };

        if !self.is_position_in_offsets(body_start, body_end) {
            self.scope_tracker.pop_scope_kind();
        }
    }
}
