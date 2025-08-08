use log::warn;
use ruby_prism::DefNode;

use crate::{
    analyzer_prism::{Identifier, ReceiverKind},
    indexer::entry::MethodKind,
    types::{ruby_method::RubyMethod, scope::{LVScope, LVScopeKind}},
};

use super::{IdentifierVisitor, IdentifierType};

impl IdentifierVisitor {
    pub fn process_def_node_entry(&mut self, node: &DefNode) {
        if self.is_result_set() || !self.is_position_in_location(&node.location()) {
            return;
        }

        let mut method_kind = MethodKind::Instance;

        if let Some(receiver) = node.receiver() {
            if let Some(_) = receiver.as_self_node() {
                method_kind = MethodKind::Class;
            } else if let Some(_) = receiver.as_constant_path_node() {
                method_kind = MethodKind::Class;
            } else if let Some(_) = receiver.as_constant_read_node() {
                method_kind = MethodKind::Class;
            }
        }

        let name = String::from_utf8_lossy(&node.name().as_slice()).to_string();
        let method = RubyMethod::new(name.as_str(), method_kind);

        if let Err(_) = method {
            warn!("Invalid method name: {}", name);
            return;
        }

        let body_loc = if let Some(body) = node.body() {
            self.document
                .prism_location_to_lsp_location(&body.location())
        } else {
            self.document
                .prism_location_to_lsp_location(&node.location())
        };

        let method = method.unwrap();
        let scope_id = self.document.position_to_offset(body_loc.range.start);
        self.scope_tracker.push_lv_scope(LVScope::new(
            scope_id,
            body_loc.clone(),
            LVScopeKind::Method,
        ));

        // Is position on method name
        let name_loc = node.name_loc();
        if self.is_position_in_location(&name_loc) {
            // Determine receiver kind for method definition
            let receiver_kind = if node.receiver().is_some() {
                ReceiverKind::SelfReceiver // Method definitions with receivers are typically self methods
            } else {
                ReceiverKind::None // Instance methods have no receiver in definition
            };

            self.set_result(
                Some(Identifier::RubyMethod {
                    namespace: self.scope_tracker.get_ns_stack(),
                    receiver_kind,
                    receiver: None, // Method definitions don't have receiver information
                    iden: method,
                }),
                Some(IdentifierType::MethodDef),
                self.scope_tracker.get_ns_stack(),
                self.scope_tracker.get_lv_stack(),
            );
        }
    }

    pub fn process_def_node_exit(&mut self, node: &DefNode) {
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
            self.scope_tracker.pop_lv_scope();
        }
    }
}