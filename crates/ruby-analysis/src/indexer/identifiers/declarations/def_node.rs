use crate::core::{FullyQualifiedName, NamespaceKind, RubyConstant, RubyMethod};
use log::warn;
use ruby_prism::{DefNode, Node};

use crate::core::MethodReceiver;
use crate::indexer::documents::scope_rules;
use crate::indexer::{queries::syntax, Identifier, LVScopeKind};

use crate::indexer::identifiers::{IdentifierType, IdentifierVisitor};

impl IdentifierVisitor<'_> {
    /// Pushes the method's scopes and returns whether it did. A def that is
    /// skipped here, such as one whose name is still being typed, must not be
    /// exited.
    pub fn process_def_node_entry(&mut self, node: &DefNode) -> bool {
        if self.is_result_set() || !self.is_position_in_location(&node.location()) {
            return false;
        }

        let (definition_namespace, namespace_kind) = match node.receiver() {
            None => self.scope_tracker.method_definition_context(),
            Some(receiver) if receiver.as_self_node().is_some() => {
                let (namespace, receiver_kind) = self.scope_tracker.implicit_receiver_context();
                if receiver_kind != NamespaceKind::Singleton {
                    return false;
                }
                (namespace, NamespaceKind::Singleton)
            }
            // A receiver this file does not declare keeps the enclosing
            // namespace so the body still has a method scope.
            Some(receiver) => match self.def_receiver_namespace(&receiver) {
                Some(namespace) => (namespace, NamespaceKind::Singleton),
                None => {
                    let kind = match syntax::get_method_namespace_kind_simple(Some(&receiver)) {
                        NamespaceKind::Instance if !self.scope_tracker.in_singleton() => {
                            NamespaceKind::Instance
                        }
                        NamespaceKind::Instance | NamespaceKind::Singleton => {
                            NamespaceKind::Singleton
                        }
                    };
                    (self.scope_tracker.get_ns_stack(), kind)
                }
            },
        };

        let name = String::from_utf8_lossy(node.name().as_slice()).to_string();
        let method = RubyMethod::new(name.as_str());

        if method.is_err() {
            warn!("Invalid method name: {}", name);
            return false;
        }

        let method = method.unwrap();
        let scope_kind = match namespace_kind {
            NamespaceKind::Singleton => LVScopeKind::ClassMethod,
            NamespaceKind::Instance => LVScopeKind::InstanceMethod,
        };
        self.scope_tracker.push_scope_kind(scope_kind);
        self.scope_tracker.push_method_fqn(
            FullyQualifiedName::method(definition_namespace.clone(), method),
            namespace_kind,
        );
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
        true
    }

    /// The class or module a constant `def` receiver names, resolved
    /// against declarations earlier in this file and the project.
    fn def_receiver_namespace(&self, receiver: &Node<'_>) -> Option<Vec<RubyConstant>> {
        scope_rules::resolve_receiver_namespace(
            receiver,
            scope_rules::implicit_singleton_namespace(&self.scope_tracker).as_deref(),
            &self.scope_tracker.get_ns_stack(),
            &|fqn| self.namespace_is_known(fqn),
        )
    }

    pub fn process_def_node_exit(&mut self, node: &DefNode) {
        if self.is_result_set() || !self.is_position_in_location(&node.location()) {
            return;
        }

        let (body_start, body_end) =
            syntax::get_body_offsets(node.body().map(|body| body.location()), &node.location());

        if !self.is_position_in_offsets(body_start, body_end) {
            self.scope_tracker.pop_execution_context();
            self.scope_tracker.pop_scope_kind();
            self.scope_tracker.pop_method_fqn();
        }
    }
}
