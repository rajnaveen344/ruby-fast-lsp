use log::warn;
use ruby_prism::DefNode;

use crate::analyzer_prism::utils;
use crate::indexer::entry::NamespaceKind;
use crate::types::{
    ruby_method::RubyMethod,
    scope::{LVScope, LVScopeKind},
};

use super::ReferenceVisitor;

impl ReferenceVisitor {
    pub fn process_def_node_entry(&mut self, node: &DefNode) {
        let body_loc = utils::get_body_location(
            node.body().map(|b| b.location()),
            &node.location(),
            &self.document,
        );
        let scope_id = self.document.position_to_offset(body_loc.range.start);

        let namespace_kind = utils::get_method_namespace_kind_simple(node.receiver().as_ref());
        let scope_kind = match namespace_kind {
            NamespaceKind::Singleton => LVScopeKind::ClassMethod,
            NamespaceKind::Instance => LVScopeKind::InstanceMethod,
        };

        self.scope_tracker
            .push_lv_scope(LVScope::new(scope_id, body_loc.clone(), scope_kind));

        // Navigate scope tree to match current scope for variable references
        self.document
            .scope_tree_mut()
            .enter_child_scope(body_loc.range);

        let name = String::from_utf8_lossy(node.name().as_slice()).to_string();
        let method = RubyMethod::new(name.as_str());

        // Mark namespace_kind as intentionally unused for now
        let _ = namespace_kind;

        if method.is_err() {
            warn!("Skipping invalid method name: {}", name);
        }
    }

    pub fn process_def_node_exit(&mut self, _node: &DefNode) {
        self.scope_tracker.pop_lv_scope();
        self.document.scope_tree_mut().exit_scope();
    }
}
