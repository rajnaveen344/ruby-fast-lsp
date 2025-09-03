use ruby_prism::ClassNode;

use crate::{
    analyzer_prism::{utils, Identifier},
    types::{
        ruby_namespace::RubyConstant,
        scope::{LVScope, LVScopeKind},
    },
};

use super::{IdentifierType, IdentifierVisitor};

impl IdentifierVisitor {
    pub fn process_class_node_entry(&mut self, node: &ClassNode) {
        if self.is_result_set() || !self.is_position_in_location(&node.location()) {
            return;
        }

        let constant_path = node.constant_path();
        let name_loc = constant_path.location();

        if self.is_position_in_location(&name_loc) {
            // Handle constant path node for class definition
            if let Some(constant_path_node) = constant_path.as_constant_path_node() {
                let mut namespaces = Vec::new();
                utils::collect_namespaces(&constant_path_node, &mut namespaces);
                self.set_result(
                    Some(Identifier::RubyConstant {
                        namespace: self.scope_tracker.get_ns_stack(),
                        iden: namespaces,
                    }),
                    Some(IdentifierType::ClassDef),
                    self.scope_tracker.get_ns_stack(),
                    self.scope_tracker.get_lv_stack(),
                );
            } else if let Some(constant_read_node) = constant_path.as_constant_read_node() {
                let name = String::from_utf8_lossy(constant_read_node.name().as_slice());
                let namespace = RubyConstant::new(&name.to_string()).unwrap();
                self.set_result(
                    Some(Identifier::RubyConstant {
                        namespace: self.scope_tracker.get_ns_stack(),
                        iden: vec![namespace],
                    }),
                    Some(IdentifierType::ClassDef),
                    self.scope_tracker.get_ns_stack(),
                    self.scope_tracker.get_lv_stack(),
                );
            }

            return;
        }

        let body_loc = if let Some(body) = node.body() {
            self.document
                .prism_location_to_lsp_location(&body.location())
        } else {
            self.document
                .prism_location_to_lsp_location(&node.location())
        };

        // Add the class name to the namespace stack
        if let Some(constant_path_node) = constant_path.as_constant_path_node() {
            let mut namespaces = Vec::new();
            utils::collect_namespaces(&constant_path_node, &mut namespaces);
            self.scope_tracker.push_ns_scopes(namespaces);
            let scope_id = self.document.position_to_offset(body_loc.range.start);
            self.scope_tracker.push_lv_scope(LVScope::new(
                scope_id,
                body_loc.clone(),
                LVScopeKind::Constant,
            ));
        } else if let Some(constant_read_node) = constant_path.as_constant_read_node() {
            let name = String::from_utf8_lossy(constant_read_node.name().as_slice());
            let namespace = RubyConstant::new(&name.to_string()).unwrap();
            self.scope_tracker.push_ns_scope(namespace);
            let scope_id = self.document.position_to_offset(body_loc.range.start);
            self.scope_tracker.push_lv_scope(LVScope::new(
                scope_id,
                body_loc.clone(),
                LVScopeKind::Constant,
            ));
        }
    }

    pub fn process_class_node_exit(&mut self, node: &ClassNode) {
        if self.is_result_set() || !self.is_position_in_location(&node.location()) {
            return;
        }

        let body_loc = if let Some(body) = node.body() {
            self.document
                .prism_location_to_lsp_location(&body.location())
        } else {
            self.document
                .prism_location_to_lsp_location(&node.location())
        };

        if !(self.position >= body_loc.range.start && self.position <= body_loc.range.end) {
            self.scope_tracker.pop_ns_scope();
            self.scope_tracker.pop_lv_scope();
        }
    }
}
