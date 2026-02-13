use ruby_prism::BlockNode;

use crate::types::scope::{LVScope, LVScopeKind};

use super::ReferenceVisitor;

impl ReferenceVisitor {
    pub fn process_block_node_entry(&mut self, node: &BlockNode) {
        let body_loc = if let Some(body) = node.body() {
            self.document
                .prism_location_to_lsp_location(&body.location())
        } else {
            self.document
                .prism_location_to_lsp_location(&node.location())
        };
        let scope_id = self.document.position_to_offset(body_loc.range.start);
        self.scope_tracker
            .push_lv_scope(LVScope::new(scope_id, body_loc.clone(), LVScopeKind::Block));

        // Navigate scope tree to match current scope for variable references
        self.document
            .scope_tree_mut()
            .enter_child_scope(body_loc.range);
    }

    pub fn process_block_node_exit(&mut self, _node: &BlockNode) {
        self.scope_tracker.pop_lv_scope();
        self.document.scope_tree_mut().exit_scope();
    }
}
