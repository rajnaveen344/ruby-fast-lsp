use log::warn;
use ruby_analysis_core::NamespaceKind;
use ruby_prism::DefNode;

use crate::{
    analyzer_prism::{utils, Identifier, MethodReceiver},
    types::{ruby_method::RubyMethod, scope::LVScopeKind},
};

use super::{IdentifierType, IdentifierVisitor};

impl IdentifierVisitor {
    pub fn process_def_node_entry(&mut self, node: &DefNode) {
        if self.is_result_set() || !self.is_position_in_location(&node.location()) {
            return;
        }

        let mut namespace_kind = utils::get_method_namespace_kind_simple(node.receiver().as_ref());
        // Account for `class << self` context — get_method_namespace_kind_simple
        // only checks for explicit `self.` receiver, not the singleton class scope
        if self.scope_tracker.in_singleton() && namespace_kind == NamespaceKind::Instance {
            namespace_kind = NamespaceKind::Singleton;
        }

        let name = String::from_utf8_lossy(node.name().as_slice()).to_string();
        let method = RubyMethod::new(name.as_str());

        if method.is_err() {
            warn!("Invalid method name: {}", name);
            return;
        }

        let _body_loc = utils::get_body_location(
            node.body().map(|b| b.location()),
            &node.location(),
            &self.document,
        );

        let method = method.unwrap();
        let scope_kind = match namespace_kind {
            NamespaceKind::Singleton => LVScopeKind::ClassMethod,
            NamespaceKind::Instance => LVScopeKind::InstanceMethod,
        };
        self.scope_tracker.push_scope_kind(scope_kind);

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
                Some(0),
            );
        }
    }

    pub fn process_def_node_exit(&mut self, node: &DefNode) {
        if self.is_result_set() || !self.is_position_in_location(&node.location()) {
            return;
        }

        let body_loc = utils::get_body_location(
            node.body().map(|b| b.location()),
            &node.location(),
            &self.document,
        );

        if !(self.position >= body_loc.range.start && self.position <= body_loc.range.end) {
            self.scope_tracker.pop_scope_kind();
        }
    }
}
