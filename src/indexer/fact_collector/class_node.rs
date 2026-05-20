use log::error;
use ruby_analysis_indexer::LocalScopeKind as LVScopeKind;
use ruby_prism::ClassNode;

use super::FactCollector;

impl FactCollector {
    pub fn process_class_node_entry(&mut self, node: &ClassNode) {
        let body_loc = self.body_lsp_location(node.body().map(|b| b.location()), &node.location());

        // Handle namespace setup
        if self
            .scope_tracker
            .push_namespace_from_constant_path(&node.constant_path(), node.name().as_slice())
            .is_err()
        {
            error!("Error creating namespace for class");
            return;
        }

        // Setup local variable scope
        self.scope_tracker.push_scope_kind(LVScopeKind::Constant);

        // Get class name for scope tree
        let class_name = String::from_utf8_lossy(node.name().as_slice()).to_string();
        self.document.variable_scopes_mut().enter_scope(
            LVScopeKind::Constant,
            body_loc.range,
            Some(class_name),
        );
    }

    pub fn process_class_node_exit(&mut self, _node: &ClassNode) {
        self.scope_tracker.pop_ns_scope();
        self.scope_tracker.pop_scope_kind();
        self.document.variable_scopes_mut().exit_scope();
    }
}
