use log::debug;
use ruby_prism::ConstantReadNode;

use crate::indexer::index::UnresolvedConstant;
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

        // Check from current namespace to root namespace
        let mut ancestors = current_namespace;
        while !ancestors.is_empty() {
            let mut combined_ns = ancestors.clone();
            combined_ns.push(constant.clone());

            let fqn = FullyQualifiedName::namespace(combined_ns);
            let mut index = self.index.lock();
            if index.definitions.contains_key(&fqn) {
                let location = self
                    .document
                    .prism_location_to_lsp_location(&node.location());
                debug!("Adding reference: {}", fqn);
                index.add_reference(fqn, location);
                drop(index);
                return;
            }
            drop(index);
            ancestors.pop();
        }

        // Check in root namespace
        let fqn = FullyQualifiedName::namespace(vec![constant]);
        let mut index = self.index.lock();
        if index.definitions.contains_key(&fqn) {
            let location = self
                .document
                .prism_location_to_lsp_location(&node.location());
            debug!("Adding reference: {}", fqn);
            index.add_reference(fqn, location);
        } else if self.track_unresolved {
            // Constant not found - track as unresolved
            let location = self
                .document
                .prism_location_to_lsp_location(&node.location());
            debug!("Adding unresolved constant: {}", name);
            index.add_unresolved_constant(
                self.document.uri.clone(),
                UnresolvedConstant {
                    name: name.clone(),
                    location,
                },
            );
        }
        drop(index);
    }

    pub fn process_constant_read_node_exit(&mut self, _node: &ConstantReadNode) {
        // No cleanup needed
    }
}
