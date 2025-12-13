use log::debug;
use ruby_prism::ConstantReadNode;

use crate::indexer::index::UnresolvedEntry;
use crate::types::{fully_qualified_name::FullyQualifiedName, ruby_namespace::RubyConstant};

use super::ReferenceVisitor;

impl ReferenceVisitor {
    pub fn process_constant_read_node_entry(&mut self, node: &ConstantReadNode) {
        let current_namespace = self.scope_tracker.get_ns_stack();
        let name = String::from_utf8_lossy(node.name().as_slice()).to_string();
        let constant = match RubyConstant::new(&name) {
            Ok(c) => c,
            Err(_) => {
                debug!("Skipping invalid constant name: {}", name);
                return;
            }
        };

        // Acquire lock once for all lookups (avoid lock thrashing)
        let mut index = self.index.lock();

        // Check from current namespace to root namespace
        let mut ancestors = current_namespace.clone();
        while !ancestors.is_empty() {
            let mut combined_ns = ancestors.clone();
            combined_ns.push(constant.clone());

            let fqn = FullyQualifiedName::namespace(combined_ns);
            if index.contains_fqn(&fqn) {
                let location = self
                    .document
                    .prism_location_to_lsp_location(&node.location());
                debug!("Adding reference: {}", fqn);
                index.add_reference(fqn, location);
                return;
            }
            ancestors.pop();
        }

        // Check in root namespace
        let fqn = FullyQualifiedName::namespace(vec![constant]);
        if index.contains_fqn(&fqn) {
            let location = self
                .document
                .prism_location_to_lsp_location(&node.location());
            debug!("Adding reference: {}", fqn);
            index.add_reference(fqn, location);
        } else if self.track_unresolved {
            // Constant not found - track as unresolved with namespace context
            let location = self
                .document
                .prism_location_to_lsp_location(&node.location());
            let namespace_context: Vec<String> =
                current_namespace.iter().map(|c| c.to_string()).collect();
            debug!(
                "Adding unresolved constant: {} in context {:?}",
                name, namespace_context
            );
            index.add_unresolved_entry(
                self.document.uri.clone(),
                UnresolvedEntry::constant_with_context(name.clone(), namespace_context, location),
            );
        }
    }

    pub fn process_constant_read_node_exit(&mut self, _node: &ConstantReadNode) {
        // No cleanup needed
    }
}
