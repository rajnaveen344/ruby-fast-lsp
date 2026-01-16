use log::warn;
use ruby_prism::DefNode;

use crate::indexer::entry::NamespaceKind;
use crate::types::{
    ruby_method::RubyMethod,
    scope::{LVScope, LVScopeKind},
};

use super::ReferenceVisitor;

impl ReferenceVisitor {
    pub fn process_def_node_entry(&mut self, node: &DefNode) {
        let body_loc = if let Some(body) = node.body() {
            self.document
                .prism_location_to_lsp_location(&body.location())
        } else {
            self.document
                .prism_location_to_lsp_location(&node.location())
        };
        let scope_id = self.document.position_to_offset(body_loc.range.start);

        let mut namespace_kind = NamespaceKind::Instance;
        let mut scope_kind = LVScopeKind::InstanceMethod;

        if let Some(receiver) = node.receiver() {
            if receiver.as_self_node().is_some() {
                namespace_kind = NamespaceKind::Singleton;
                scope_kind = LVScopeKind::ClassMethod;
            } else if receiver.as_constant_path_node().is_some() {
                namespace_kind = NamespaceKind::Singleton;
                scope_kind = LVScopeKind::ClassMethod;
            } else if receiver.as_constant_read_node().is_some() {
                namespace_kind = NamespaceKind::Singleton;
                scope_kind = LVScopeKind::ClassMethod;
            }
        }

        self.scope_tracker
            .push_lv_scope(LVScope::new(scope_id, body_loc, scope_kind));

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
    }
}
