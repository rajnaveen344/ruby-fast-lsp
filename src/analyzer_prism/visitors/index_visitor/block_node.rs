use ruby_prism::BlockNode;

use crate::types::scope::LVScopeKind;

use super::IndexVisitor;

impl IndexVisitor {
    pub fn process_block_node_entry(&mut self, node: &BlockNode) {
        let body_loc = if let Some(body) = node.body() {
            self.document
                .prism_location_to_lsp_location(&body.location())
        } else {
            self.document
                .prism_location_to_lsp_location(&node.location())
        };
        let scope_id = self.document.position_to_offset(body_loc.range.start);
        self.push_lv_scope(scope_id, body_loc, LVScopeKind::Block);
    }

    pub fn process_block_node_exit(&mut self, _node: &BlockNode) {
        self.pop_lv_scope();
    }
}
