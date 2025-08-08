use ruby_prism::BlockNode;

use crate::types::scope::{LVScope, LVScopeKind};

use super::IdentifierVisitor;

impl IdentifierVisitor {
    pub fn process_block_node_entry(&mut self, node: &BlockNode) {
        if self.is_result_set() || !self.is_position_in_location(&node.location()) {
            return;
        }

        let body_loc = if let Some(body) = node.body() {
            self.document
                .prism_location_to_lsp_location(&body.location())
        } else {
            self.document
                .prism_location_to_lsp_location(&node.location())
        };
        let scope_id = self.document.position_to_offset(body_loc.range.start);
        self.scope_tracker.push_lv_scope(LVScope::new(
            scope_id,
            body_loc.clone(),
            LVScopeKind::Block,
        ));
    }

    pub fn process_block_node_exit(&mut self, node: &BlockNode) {
        if self.is_result_set() || !self.is_position_in_location(&node.location()) {
            return;
        }

        let body_loc = if let Some(body) = node.body() {
            self.document
                .prism_location_to_lsp_location(&body.location())
        } else {
            self.document
                .prism_location_to_lsp_location(&node.location())
        };

        if !(self.position >= body_loc.range.start && self.position <= body_loc.range.end) {
            self.scope_tracker.pop_lv_scope();
        }
    }
}
