use crate::core::{FullyQualifiedName, GraphNodeKind};
use crate::indexer::documents::scope_rules::alias_reopen_target;
use crate::indexer::LocalScopeKind as LVScopeKind;
use log::error;
use ruby_prism::ModuleNode;

use crate::indexer::fact_collector::FactCollector;

impl FactCollector {
    pub(in crate::indexer::fact_collector) fn process_module_node_entry(
        &mut self,
        node: &ModuleNode,
    ) -> bool {
        let body_range = self.body_text_range(node.body().map(|b| b.location()), &node.location());

        let reopened_target = alias_reopen_target(
            &node.constant_path(),
            GraphNodeKind::Module,
            None,
            &self.scope_tracker.get_ns_stack(),
            |candidates| self.alias_value_type(candidates),
        );
        if let Some(target) = reopened_target {
            self.scope_tracker
                .push_absolute_ns_scopes(target.namespace_parts().to_vec());
        } else {
            if self
                .scope_tracker
                .push_namespace_from_constant_path(&node.constant_path(), node.name().as_slice())
                .is_err()
            {
                error!("Error creating namespace for module");
                return false;
            }
            let fqn = FullyQualifiedName::namespace(self.scope_tracker.get_ns_stack());
            let range = self.direct_range(&node.location());
            let name_range = self.direct_terminal_name_range(
                &node.constant_path().location(),
                node.name().as_slice(),
            );
            self.direct_push_namespace_facts(fqn, GraphNodeKind::Module, range, name_range);
        }

        self.scope_tracker.push_scope_kind(LVScopeKind::Constant);

        self.document
            .variable_scopes_mut()
            .enter_scope(LVScopeKind::Constant, body_range);
        true
    }

    pub(in crate::indexer::fact_collector) fn process_module_node_exit(
        &mut self,
        _node: &ModuleNode,
    ) {
        self.scope_tracker.pop_ns_scope();
        self.scope_tracker.pop_scope_kind();
        self.document.variable_scopes_mut().exit_scope();
    }
}
