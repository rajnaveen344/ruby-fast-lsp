use ruby_prism::ClassNode;

use crate::analyzer_prism::utils;
use crate::types::scope::LVScopeKind;

use super::ReferenceVisitor;

impl ReferenceVisitor {
    pub fn process_class_node_entry(&mut self, node: &ClassNode) {
        let const_path = node.constant_path();

        let body_loc = utils::get_body_location(
            node.body().map(|b| b.location()),
            &node.location(),
            &self.document,
        );

        if self
            .scope_tracker
            .push_namespace_from_constant_path(&const_path, node.name().as_slice())
            .is_err()
        {
            return; // Skip invalid names
        }
        self.scope_tracker.push_scope_kind(LVScopeKind::Constant);

        // Navigate scope tree to match current scope for variable references
        self.document
            .variable_scopes_mut()
            .enter_child_scope(body_loc.range);
    }

    pub fn process_class_node_exit(&mut self, _node: &ClassNode) {
        self.scope_tracker.pop_ns_scope();
        self.scope_tracker.pop_scope_kind();
        self.document.variable_scopes_mut().exit_scope();
    }
}
