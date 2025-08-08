use ruby_prism::ClassNode;

use crate::analyzer_prism::utils;
use crate::types::{ruby_namespace::RubyConstant, scope::{LVScope, LVScopeKind}};

use super::ReferenceVisitor;

impl ReferenceVisitor {
    pub fn process_class_node_entry(&mut self, node: &ClassNode) {
        let const_path = node.constant_path();

        let body_loc = if let Some(body) = node.body() {
            self.document
                .prism_location_to_lsp_location(&body.location())
        } else {
            self.document
                .prism_location_to_lsp_location(&node.location())
        };

        if let Some(path_node) = const_path.as_constant_path_node() {
            let mut namespace_parts = Vec::new();
            utils::collect_namespaces(&path_node, &mut namespace_parts);
            self.scope_tracker.push_ns_scopes(namespace_parts);
            let scope_id = self.document.position_to_offset(body_loc.range.start);
            self.scope_tracker.push_lv_scope(LVScope::new(
                scope_id,
                body_loc,
                LVScopeKind::Constant,
            ));
        } else {
            let name = String::from_utf8_lossy(node.name().as_slice());
            self.scope_tracker
                .push_ns_scope(RubyConstant::new(&name).unwrap());
            let scope_id = self.document.position_to_offset(body_loc.range.start);
            self.scope_tracker.push_lv_scope(LVScope::new(
                scope_id,
                body_loc,
                LVScopeKind::Constant,
            ));
        }
    }

    pub fn process_class_node_exit(&mut self, _node: &ClassNode) {
        self.scope_tracker.pop_ns_scope();
        self.scope_tracker.pop_lv_scope();
    }
}