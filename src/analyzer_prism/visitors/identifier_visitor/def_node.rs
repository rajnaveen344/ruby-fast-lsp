use log::warn;
use ruby_prism::DefNode;

use crate::{
    analyzer_prism::{Identifier, MethodReceiver},
    indexer::entry::MethodKind,
    types::{
        ruby_method::RubyMethod,
        scope::{LVScope, LVScopeKind},
    },
};

use super::{IdentifierType, IdentifierVisitor};

impl IdentifierVisitor {
    pub fn process_def_node_entry(&mut self, node: &DefNode) {
        if self.is_result_set() || !self.is_position_in_location(&node.location()) {
            return;
        }

        let mut method_kind = MethodKind::Instance;

        if let Some(receiver) = node.receiver() {
            if receiver.as_self_node().is_some() {
                method_kind = MethodKind::Class;
            } else if receiver.as_constant_path_node().is_some() {
                method_kind = MethodKind::Class;
            } else if receiver.as_constant_read_node().is_some() {
                method_kind = MethodKind::Class;
            }
        }

        let name = String::from_utf8_lossy(node.name().as_slice()).to_string();
        let method = RubyMethod::new(name.as_str(), method_kind);

        if method.is_err() {
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
        let scope_kind = match method_kind {
            MethodKind::Class => LVScopeKind::ClassMethod,
            MethodKind::Instance => LVScopeKind::InstanceMethod,
            MethodKind::Unknown => LVScopeKind::InstanceMethod, // Default to instance method
        };
        self.scope_tracker
            .push_lv_scope(LVScope::new(scope_id, body_loc.clone(), scope_kind));

        // Is position on method name
        let name_loc = node.name_loc();
        if self.is_position_in_location(&name_loc) {
            // Determine receiver for method definition
            let receiver = if node.receiver().is_some() {
                MethodReceiver::SelfReceiver // Method definitions with receivers are typically self methods
            } else {
                MethodReceiver::None // Instance methods have no receiver in definition
            };

            self.set_result(
                Some(Identifier::RubyMethod {
                    namespace: self.scope_tracker.get_ns_stack(),
                    receiver,
                    iden: method,
                }),
                Some(IdentifierType::MethodDef),
                self.scope_tracker.get_ns_stack(),
                self.scope_tracker.current_lv_scope().map(|s| s.scope_id()),
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
