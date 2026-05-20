use log::{error, trace};
use ruby_analysis_core::{TypeFact, TypeProvenance, TypeSubject};
use ruby_prism::ConstantWriteNode;

use crate::types::{fully_qualified_name::FullyQualifiedName, ruby_namespace::RubyConstant};

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
        let inferred_type = self.infer_type_from_value(&node.value());
        self.type_store.add(TypeFact::new(
            TypeSubject::Constant(fqn.clone()),
            inferred_type.clone(),
            self.document.prism_location_to_text_range(&node.location()),
            TypeProvenance::Assignment,
        ));
    }

    pub fn process_constant_write_node_exit(&mut self, _node: &ConstantWriteNode) {
        // No-op for now
    }
}
