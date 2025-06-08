use ruby_prism::BlockNode;

use crate::types::scope_kind::LVScopeKind;

use super::IndexVisitor;

impl IndexVisitor {
    pub fn process_block_node_entry(&mut self, _node: &BlockNode) {
        self.push_lv_scope(LVScopeKind::Block);
    }

    pub fn process_block_node_exit(&mut self, _node: &BlockNode) {
        self.pop_lv_scope();
    }
}
