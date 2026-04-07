use log::warn;
use ruby_prism::DefNode;

use crate::analyzer_prism::utils;
use crate::indexer::entry::NamespaceKind;
use crate::types::{
    fully_qualified_name::FullyQualifiedName, ruby_method::RubyMethod, scope::LVScopeKind,
};

use super::ReferenceVisitor;

impl ReferenceVisitor {
    pub fn process_def_node_entry(&mut self, node: &DefNode) {
        let body_loc = utils::get_body_location(
            node.body().map(|b| b.location()),
            &node.location(),
            &self.document,
        );

        let namespace_kind = utils::get_method_namespace_kind_simple(node.receiver().as_ref());
        let scope_kind = match namespace_kind {
            NamespaceKind::Singleton => LVScopeKind::ClassMethod,
            NamespaceKind::Instance => LVScopeKind::InstanceMethod,
        };

        self.scope_tracker.push_scope_kind(scope_kind);

        // Navigate scope tree to match current scope for variable references
        self.document
            .variable_scopes_mut()
            .enter_child_scope(body_loc.range);

        let name = String::from_utf8_lossy(node.name().as_slice()).to_string();
        match RubyMethod::new(name.as_str()) {
            Ok(method) => {
                let ns = self.scope_tracker.get_ns_stack();
                let method_fqn = FullyQualifiedName::method(ns, method);
                self.scope_tracker.push_method_fqn(Some(method_fqn));
            }
            Err(_) => {
                warn!("Skipping invalid method name: {}", name);
                self.scope_tracker.push_method_fqn(None);
            }
        }
    }

    pub fn process_def_node_exit(&mut self, _node: &DefNode) {
        self.scope_tracker.pop_method_fqn();
        self.scope_tracker.pop_scope_kind();
        self.document.variable_scopes_mut().exit_scope();
    }
}
