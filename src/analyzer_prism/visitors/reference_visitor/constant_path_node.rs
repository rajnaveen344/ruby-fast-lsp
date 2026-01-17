use log::trace;
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
        let mut namespaces = Vec::new();
        collect_namespaces(node, &mut namespaces);

        if namespaces.is_empty() {
            return;
        }

        // Get namespace stack once
        let current_namespace = self.scope_tracker.get_ns_stack();
        let ns_len = current_namespace.len();

        let mut found_any = false;

        // Acquire lock once for all lookups
        let mut index = self.index.lock();

        // Optimized ancestor search: avoid cloning in loop
        // Check from current namespace down to root
        for depth in (0..=ns_len).rev() {
            // Build namespace: current_namespace[0..depth] + namespaces
            // We slice the current namespace to the current depth and extend with the path namespaces
            let mut combined_ns: Vec<_> = current_namespace[0..depth].to_vec();
            combined_ns.extend(namespaces.iter().cloned());

            // Try as Namespace first (for class/module definitions)
            let namespace_fqn = FullyQualifiedName::namespace(combined_ns.clone());
            if index.contains_fqn(&namespace_fqn) {
                let location = self
                    .document
                    .prism_location_to_lsp_location(&node.location());

                index.add_reference(namespace_fqn, location);

                found_any = true;
                // Once found, we stop searching up the ancestor chain
                break;
            }

            // Then try as Constant (for value constants like BETA = 100)
            let constant_fqn = FullyQualifiedName::Constant(combined_ns);
            if index.contains_fqn(&constant_fqn) {
                let location = self
                    .document
                    .prism_location_to_lsp_location(&node.location());

                index.add_reference(constant_fqn, location);

                found_any = true;
                break;
            }
        }

        // If not found and tracking is enabled, add as unresolved
        if !found_any && self.track_unresolved {
            let name = namespaces
                .iter()
                .map(|c| c.to_string())
                .collect::<Vec<_>>()
                .join("::");
            let location = self
                .document
                .prism_location_to_lsp_location(&node.location());
            let namespace_context: Vec<String> =
                current_namespace.iter().map(|c| c.to_string()).collect();
            trace!(
                "Adding unresolved constant path: {} in context {:?}",
                name, namespace_context
            );
            index.add_unresolved_entry(
                self.document.uri.clone(),
                UnresolvedEntry::constant_with_context(name, namespace_context, location),
            );
        }
    }

    pub fn process_constant_path_node_exit(&mut self, _node: &ConstantPathNode) {
        // No cleanup needed
    }
}
