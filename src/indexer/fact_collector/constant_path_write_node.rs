use log::error;
use ruby_analysis_core::{TypeFact, TypeProvenance, TypeSubject};
use ruby_prism::ConstantPathWriteNode;

use crate::analyzer_prism::utils;
use crate::types::{fully_qualified_name::FullyQualifiedName, ruby_namespace::RubyConstant};

use super::FactCollector;

impl FactCollector {
    pub fn process_constant_path_write_node_entry(&mut self, node: &ConstantPathWriteNode) {
        // Extract the constant path
        let constant_path = node.target();

        // Extract the constant name (the rightmost part of the path)
        let constant_name = match constant_path.name() {
            Some(name) => String::from_utf8_lossy(name.as_slice()).to_string(),
            None => {
                error!("Could not extract constant name from ConstantPathWriteNode");
                return;
            }
        };

        // Create a RubyConstant from the name
        let constant = match RubyConstant::new(&constant_name) {
            Ok(constant) => constant,
            Err(e) => {
                error!("Error creating constant: {}", e);
                return;
            }
        };

        // Extract the full constant path from the target.
        let mut namespace_parts = Vec::new();
        utils::collect_namespaces(&constant_path, &mut namespace_parts);

        // Get the current namespace and add the collected parts
        let mut fqn_parts = self.scope_tracker.get_ns_stack();
        fqn_parts.extend(namespace_parts);
        assert!(
            fqn_parts.last() == Some(&constant),
            "INVARIANT VIOLATED: constant path write target `{}` did not end with its name. \
             This is a bug because Prism target path collection must preserve the written constant. \
             Fix: inspect collect_namespaces for ConstantPathWriteNode targets.",
            constant_name
        );

        // Value constants use Constant variant, not Namespace
        let fqn = FullyQualifiedName::constant(fqn_parts);
        let inferred_type = self.infer_type_from_value(&node.value());
        self.type_store.add(TypeFact::new(
            TypeSubject::Constant(fqn.clone()),
            inferred_type.clone(),
            self.document.prism_location_to_text_range(&node.location()),
            TypeProvenance::Assignment,
        ));
    }

    pub fn process_constant_path_write_node_exit(&mut self, _node: &ConstantPathWriteNode) {
        // No-op for now
    }
}
