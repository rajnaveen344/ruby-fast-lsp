use ruby_analysis_indexer::LocalScopeKind as LVScopeKind;
use ruby_prism::BlockNode;

use super::FactCollector;

impl FactCollector {
    pub fn process_block_node_entry(&mut self, node: &BlockNode) {
        let body_range = self.body_text_range(node.body().map(|b| b.location()), &node.location());
        self.scope_tracker.push_scope_kind(LVScopeKind::Block);
        self.document
            .variable_scopes_mut()
            .enter_scope(LVScopeKind::Block, body_range, None);
    }

    pub fn process_block_node_exit(&mut self, _node: &BlockNode) {
        self.scope_tracker.pop_scope_kind();
        self.document.variable_scopes_mut().exit_scope();
    }
}
