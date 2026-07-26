use crate::core::{
    FullyQualifiedName, MethodCallSignatureCandidate, MethodReferenceAccess,
    MethodReferenceCandidate, MethodReferenceDiagnostics, ReferenceCandidate, RubyMethod,
};
use ruby_prism::{ForwardingSuperNode, SuperNode};

use super::FactCollector;

impl FactCollector {
    pub fn process_forwarding_super_node_entry(&mut self, node: &ForwardingSuperNode) {
        self.push_super_reference_candidate(&node.location());
    }

    pub fn process_super_node_entry(&mut self, node: &SuperNode) {
        self.push_super_reference_candidate(&node.keyword_loc());
    }

    fn push_super_reference_candidate(&mut self, location: &ruby_prism::Location) {
        let Some(FullyQualifiedName::Method(_, method)) = self.scope_tracker.current_method_fqn()
        else {
            return;
        };
        let method = RubyMethod::new(method.as_str()).expect(
            "INVARIANT VIOLATED: current method FQN contains invalid Ruby method. \
             This is a bug because RubyMethod validates names at construction. \
             Fix: keep current_method_fqn populated only from RubyMethod values.",
        );
        let range = self.text_range_from_prism_location(location, "super method reference");
        self.reference_candidates.push(ReferenceCandidate::method(
            range,
            MethodReferenceCandidate {
                owner: self.scope_tracker.get_ns_stack(),
                owner_kind: self.scope_tracker.current_method_context(),
                method,
                is_super: true,
                access: MethodReferenceAccess::Normal,
                caller: self.scope_tracker.current_method_fqn().cloned(),
                preferred_definition_range: None,
                diagnostics: MethodReferenceDiagnostics {
                    diagnostic_range: range,
                    receiver_label: Some("super".to_string()),
                    diagnose_unresolved: self.diagnostics_enabled,
                    allow_unindexed_owner: false,
                    signature: MethodCallSignatureCandidate::default(),
                },
            },
        ));
    }
}
