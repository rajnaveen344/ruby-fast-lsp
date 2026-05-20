use ruby_prism::BlockNode;

use crate::types::scope::LVScopeKind;

use super::FactCollector;

impl FactCollector {
    pub fn process_block_node_entry(&mut self, node: &BlockNode) {
        let body_loc = if let Some(body) = node.body() {
            self.document
                .prism_location_to_lsp_location(&body.location())
        } else {
            self.document
                .prism_location_to_lsp_location(&node.location())
        };
        self.scope_tracker.push_scope_kind(LVScopeKind::Block);
        self.document
            .variable_scopes_mut()
            .enter_scope(LVScopeKind::Block, body_loc.range, None);
    }

    pub fn process_block_node_exit(&mut self, _node: &BlockNode) {
        self.scope_tracker.pop_scope_kind();
        self.document.variable_scopes_mut().exit_scope();
    }
}
