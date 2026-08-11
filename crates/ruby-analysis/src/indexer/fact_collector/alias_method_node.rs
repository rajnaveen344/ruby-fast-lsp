use crate::core::{
    FullyQualifiedName, MethodReferenceAccess, MethodReferenceCandidate,
    MethodReferenceDiagnostics, ReferenceCandidate, RubyMethod, TypeFact, TypeSubject,
};
use ruby_prism::{AliasMethodNode, Node};

use super::FactCollector;

impl FactCollector {
    pub fn process_alias_method_node_entry(&mut self, node: &AliasMethodNode) {
        let Some((new_name, old_name)) = alias_method_names(node) else {
            return;
        };
        let Ok(new_method) = RubyMethod::new(&new_name) else {
            return;
        };
        let Ok(old_method) = RubyMethod::new(&old_name) else {
            return;
        };

        let old_name_node = node.old_name();
        let Some(old_symbol) = old_name_node.as_symbol_node() else {
            return;
        };
        let old_location = old_symbol
            .value_loc()
            .unwrap_or_else(|| old_symbol.location());
        let old_range = self.direct_range(&old_location);
        self.reference_candidates.push(ReferenceCandidate::method(
            old_range,
            MethodReferenceCandidate {
                owner: self.scope_tracker.get_ns_stack(),
                owner_kind: self.scope_tracker.current_macro_definition_context(),
                method: old_method,
                is_super: false,
                access: MethodReferenceAccess::Normal,
                caller: self.scope_tracker.current_method_fqn().cloned(),
                call_expression_range: None,
                preferred_definition_range: None,
                diagnostics: MethodReferenceDiagnostics {
                    diagnostic_range: old_range,
                    receiver_label: None,
                    receiver_expression_range: None,
                    receiver_type: None,
                    diagnose_unresolved: false,
                    allow_unindexed_owner: false,
                    signature: None,
                },
            },
        ));

        let namespace_parts = self.scope_tracker.get_ns_stack();
        let old_fqn = FullyQualifiedName::method(namespace_parts.clone(), old_method);
        let new_fqn = FullyQualifiedName::method(namespace_parts, new_method);
        let old_subject = TypeSubject::MethodReturn(old_fqn);
        let Some(old_type) = self.type_store.facts_for(&old_subject).into_iter().next() else {
            return;
        };

        self.type_store.add(TypeFact::new(
            TypeSubject::MethodReturn(new_fqn),
            old_type.ruby_type,
            self.direct_range(&node.location()),
            old_type.provenance,
        ));
    }
}

fn alias_method_names(node: &AliasMethodNode<'_>) -> Option<(String, String)> {
    let new_name = symbol_name(&node.new_name())?;
    let old_name = symbol_name(&node.old_name())?;
    Some((new_name, old_name))
}

fn symbol_name(node: &Node<'_>) -> Option<String> {
    node.as_symbol_node()
        .map(|symbol| String::from_utf8_lossy(symbol.unescaped()).to_string())
}
