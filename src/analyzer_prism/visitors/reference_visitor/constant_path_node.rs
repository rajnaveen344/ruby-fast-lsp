use log::debug;
use ruby_prism::ConstantPathNode;

use crate::analyzer_prism::utils::collect_namespaces;
use crate::indexer::index::UnresolvedEntry;
use crate::types::fully_qualified_name::FullyQualifiedName;

use super::ReferenceVisitor;

impl ReferenceVisitor {
    pub fn process_constant_path_node_entry(&mut self, node: &ConstantPathNode) {
        // Collect namespaces from constant path node
        // Find in namespace stack
        // If found, add reference for each namespace
        // Eg. Example: Given this source
        //
        // ```ruby
        // module Core
        //   module Platform
        //     module API
        //       module Users; end
        //     end
        //
        //     module Something
        //       include API::Users
        //     end
        //   end
        // end
        // ```
        // `include API::Users` under `Core::Platform::Something` should add references to:
        // Core::Platform::API
        // Core::Platform::API::Users
        //
        // If not found, track as unresolved when enabled
        let current_namespace = self.scope_tracker.get_ns_stack();
        let mut namespaces = Vec::new();
        collect_namespaces(node, &mut namespaces);

        let mut found = false;

        // Check from current namespace to root namespace
        let mut ancestors = current_namespace.clone();
        while !ancestors.is_empty() {
            let mut combined_ns = ancestors.clone();
            combined_ns.extend(namespaces.clone());

            let fqn = FullyQualifiedName::namespace(combined_ns);
            let mut index = self.index.lock();
            let entries = index.definitions.get(&fqn);
            if entries.is_some() {
                let location = self
                    .document
                    .prism_location_to_lsp_location(&node.location());
                index.add_reference(fqn.clone(), location);
                found = true;
            }
            drop(index);

            ancestors.pop();
        }

        // Check from root namespace
        let fqn = FullyQualifiedName::namespace(namespaces.clone());
        let mut index = self.index.lock();
        let entries = index.definitions.get(&fqn);
        if entries.is_some() {
            let location = self
                .document
                .prism_location_to_lsp_location(&node.location());
            index.add_reference(fqn.clone(), location);
            found = true;
        }

        // If not found anywhere and tracking is enabled, add as unresolved
        if !found && self.track_unresolved && !namespaces.is_empty() {
            let name = namespaces
                .iter()
                .map(|c| c.to_string())
                .collect::<Vec<_>>()
                .join("::");
            let location = self
                .document
                .prism_location_to_lsp_location(&node.location());
            let namespace_context: Vec<String> = current_namespace
                .iter()
                .map(|c| c.to_string())
                .collect();
            debug!(
                "Adding unresolved constant path: {} in context {:?}",
                name, namespace_context
            );
            index.add_unresolved_entry(
                self.document.uri.clone(),
                UnresolvedEntry::constant_with_context(name, namespace_context, location),
            );
        }

        drop(index);
    }

    pub fn process_constant_path_node_exit(&mut self, _node: &ConstantPathNode) {
        // No cleanup needed
    }
}
