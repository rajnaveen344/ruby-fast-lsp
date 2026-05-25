use crate::core::{NamespaceKind, RubyMethod};
use crate::{Identifier, LVScopeKind, MethodReceiver};
use ruby_prism::{AliasMethodNode, Node};

use super::{IdentifierType, IdentifierVisitor};

impl IdentifierVisitor {
    pub fn process_alias_method_node_entry(&mut self, node: &AliasMethodNode) {
        if self.is_result_set() {
            return;
        }

        let new_name_node = node.new_name();
        if !self.is_position_in_location(&new_name_node.location()) {
            return;
        }

        let Some(name) = symbol_name(&new_name_node) else {
            return;
        };
        let Ok(method) = RubyMethod::new(&name) else {
            return;
        };
        let namespace_kind = self.scope_tracker.current_macro_definition_context();
        let receiver = match namespace_kind {
            NamespaceKind::Instance => MethodReceiver::None,
            NamespaceKind::Singleton => MethodReceiver::SelfReceiver,
        };
        let scope_kind = match namespace_kind {
            NamespaceKind::Instance => LVScopeKind::InstanceMethod,
            NamespaceKind::Singleton => LVScopeKind::ClassMethod,
        };
        self.scope_tracker.push_scope_kind(scope_kind);

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

fn symbol_name(node: &Node<'_>) -> Option<String> {
    node.as_symbol_node()
        .map(|symbol| String::from_utf8_lossy(symbol.unescaped()).to_string())
}
