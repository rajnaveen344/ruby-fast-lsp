use crate::core::{FullyQualifiedName, NamespaceKind, RubyMethod};
use log::warn;
use ruby_prism::DefNode;

use crate::{analyzer_utils as utils, Identifier, LVScopeKind, MethodReceiver};

use super::{IdentifierType, IdentifierVisitor};

impl IdentifierVisitor {
    pub fn process_def_node_entry(&mut self, node: &DefNode) {
        if self.is_result_set() || !self.is_position_in_location(&node.location()) {
            return;
        }

        let (definition_namespace, namespace_kind) = match node.receiver() {
            None => self.scope_tracker.method_definition_context(),
            Some(receiver) if receiver.as_self_node().is_some() => {
                let (namespace, receiver_kind) = self.scope_tracker.implicit_receiver_context();
                if receiver_kind != NamespaceKind::Singleton {
                    return;
                }
                (namespace, NamespaceKind::Singleton)
            }
            Some(_) => {
                let mut kind = utils::get_method_namespace_kind_simple(node.receiver().as_ref());
                // Account for `class << self` context — get_method_namespace_kind_simple
                // only checks for explicit `self.` receiver, not the singleton class scope.
                if self.scope_tracker.in_singleton() && kind == NamespaceKind::Instance {
                    kind = NamespaceKind::Singleton;
                }
                (self.scope_tracker.get_ns_stack(), kind)
            }
        };

        let name = String::from_utf8_lossy(node.name().as_slice()).to_string();
        let method = RubyMethod::new(name.as_str());

        if method.is_err() {
            warn!("Invalid method name: {}", name);
            return;
        }

        let method = method.unwrap();
        let scope_kind = match namespace_kind {
            NamespaceKind::Singleton => LVScopeKind::ClassMethod,
            NamespaceKind::Instance => LVScopeKind::InstanceMethod,
        };
        self.scope_tracker.push_scope_kind(scope_kind);
        self.scope_tracker
            .push_method_fqn(Some(FullyQualifiedName::method(
                definition_namespace.clone(),
                method,
            )));
        self.scope_tracker.push_method_execution_context(
            definition_namespace.clone(),
            namespace_kind,
            definition_namespace.clone(),
            namespace_kind,
        );

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
                    namespace: definition_namespace.clone(),
                    receiver,
                    iden: method,
                }),
                Some(IdentifierType::MethodDef),
                definition_namespace,
                Some(0),
            );
        }
    }

    pub fn process_def_node_exit(&mut self, node: &DefNode) {
        if self.is_result_set() || !self.is_position_in_location(&node.location()) {
            return;
        }

        let (body_start, body_end) =
            utils::get_body_offsets(node.body().map(|body| body.location()), &node.location());

        if !self.is_position_in_offsets(body_start, body_end) {
            self.scope_tracker.pop_execution_context();
            self.scope_tracker.pop_scope_kind();
            self.scope_tracker.pop_method_fqn();
        }
    }
}
