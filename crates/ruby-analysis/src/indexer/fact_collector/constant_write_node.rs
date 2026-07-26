use crate::core::{
    FullyQualifiedName, RubyConstant, SymbolFact, SymbolKind, TypeFact, TypeSubject,
};
use log::{error, trace};
use ruby_prism::ConstantWriteNode;

use super::FactCollector;

impl FactCollector {
    pub fn process_constant_write_node_entry(&mut self, node: &ConstantWriteNode) {
        let constant_name = String::from_utf8_lossy(node.name().as_slice()).to_string();
        trace!("Visiting constant write node: {}", constant_name);

        // Create a RubyConstant from the name
        let constant = match RubyConstant::new(&constant_name) {
            Ok(constant) => constant,
            Err(e) => {
                error!("Error creating constant: {}", e);
                return;
            }
        };

        // Create a FullyQualifiedName using the current namespace stack and the constant
        // First get the current flattened namespace, then add the new constant
        let mut namespace = self.scope_tracker.get_ns_stack();
        namespace.push(constant);
        // Value constants use Constant variant, not Namespace
        let fqn = FullyQualifiedName::constant(namespace);
        self.direct_facts.symbols.push(
            SymbolFact::new(
                fqn.clone(),
                SymbolKind::Constant,
                self.direct_range(&node.location()),
            )
            .with_name_range(self.direct_range(&node.name_loc())),
        );
    }

    pub fn process_constant_write_node_exit(&mut self, node: &ConstantWriteNode) {
        let constant_name = String::from_utf8_lossy(node.name().as_slice()).to_string();
        let constant = RubyConstant::new(&constant_name).unwrap_or_else(|error| {
            panic!(
                "INVARIANT VIOLATED: constant write name became invalid between visitor entry and exit: {error}. \
                 This is a bug because Prism exposes the same constant name for the balanced traversal. \
                 Fix: keep constant validation and traversal lifecycle synchronized."
            )
        });
        let mut namespace = self.scope_tracker.get_ns_stack();
        namespace.push(constant);
        let fqn = FullyQualifiedName::constant(namespace);
        let (inferred_type, provenance) = self.assignment_type_and_provenance(&node.value());
        self.direct_push_type(
            TypeSubject::Constant(fqn.clone()),
            inferred_type.clone(),
            &node.name_loc(),
            provenance,
        );
        self.type_store.add(TypeFact::new(
            TypeSubject::Constant(fqn),
            inferred_type,
            self.document.prism_location_to_text_range(&node.location()),
            provenance,
        ));
    }
}
