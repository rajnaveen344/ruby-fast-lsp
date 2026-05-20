use crate::LocalScopeKind as LVScopeKind;
use log::error;
use ruby_prism::ModuleNode;

use super::FactCollector;

impl FactCollector {
    pub fn process_module_node_entry(&mut self, node: &ModuleNode) {
        let body_range = self.body_text_range(node.body().map(|b| b.location()), &node.location());

        if self
            .scope_tracker
            .push_namespace_from_constant_path(&node.constant_path(), node.name().as_slice())
            .is_err()
        {
            error!("Error creating namespace for module");
            return;
        }

        self.scope_tracker.push_scope_kind(LVScopeKind::Constant);

        let module_name = String::from_utf8_lossy(node.name().as_slice()).to_string();
        self.document.variable_scopes_mut().enter_scope(
            LVScopeKind::Constant,
            body_range,
            Some(module_name),
        );
    }

    pub fn process_module_node_exit(&mut self, _node: &ModuleNode) {
        self.scope_tracker.pop_ns_scope();
        self.scope_tracker.pop_scope_kind();
        self.document.variable_scopes_mut().exit_scope();
    }
}
