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
                    self.scope_tracker.current_lv_scope().map(|s| s.scope_id()),
                );
            } else if let Some(constant_read_node) = constant_path.as_constant_read_node() {
                let name = String::from_utf8_lossy(constant_read_node.name().as_slice());
                let namespace = RubyConstant::new(name.as_ref()).unwrap();
                self.set_result(
                    Some(Identifier::RubyConstant {
                        namespace: self.scope_tracker.get_ns_stack(),
                        iden: vec![namespace],
                    }),
                    Some(IdentifierType::ClassDef),
                    self.scope_tracker.get_ns_stack(),
                    self.scope_tracker.current_lv_scope().map(|s| s.scope_id()),
                );
            }

            return;
        }

        let body_loc = utils::get_body_location(
            node.body().map(|b| b.location()),
            &node.location(),
            &self.document,
        );

        // Add the class name to the namespace stack
        if self
            .scope_tracker
            .push_namespace_from_constant_path(&constant_path, node.name().as_slice())
            .is_ok()
        {
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

        let body_loc = utils::get_body_location(
            node.body().map(|b| b.location()),
            &node.location(),
            &self.document,
        );

        if !(self.position >= body_loc.range.start && self.position <= body_loc.range.end) {
            self.scope_tracker.pop_ns_scope();
            self.scope_tracker.pop_lv_scope();
        }
    }
}
