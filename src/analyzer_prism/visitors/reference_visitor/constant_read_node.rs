use log::trace;
use ruby_prism::ConstantReadNode;

use crate::analyzer_prism::utils;
use crate::types::unresolved_index::UnresolvedEntry;
use crate::types::{fully_qualified_name::FullyQualifiedName, ruby_namespace::RubyConstant};

use super::ReferenceVisitor;

impl ReferenceVisitor {
    pub fn process_constant_read_node_entry(&mut self, node: &ConstantReadNode) {
        let name = utils::utf8_str(node.name().as_slice());
        let constant = match RubyConstant::new(name) {
            Ok(c) => c,
            Err(_) => {
                trace!("Skipping invalid constant name: {}", name);
                return;
            }
        };

        // Get namespace stack once (this clones, but only once)
        let current_namespace = self.scope_tracker.get_ns_stack();
        let ns_len = current_namespace.len();

        let resolution = self
            .resolve_constant_from_analysis(std::slice::from_ref(&constant), &current_namespace);
        let resolution = if resolution.is_some() || self.analysis_engine.is_some() {
            resolution
        } else {
            let resolution = {
                let index = self.index.read();
                let mut resolved: Option<FullyQualifiedName> = None;
                for depth in (0..=ns_len).rev() {
                    let mut combined_ns: Vec<RubyConstant> = current_namespace[0..depth].to_vec();
                    combined_ns.push(constant.clone());

                    // Try as Namespace first (class/module definitions)
                    let namespace_fqn = FullyQualifiedName::namespace(combined_ns.clone());
                    if index.contains_fqn(&namespace_fqn) {
                        resolved = Some(namespace_fqn);
                        break;
                    }

                    // Then try as Constant (value constants like VALUE = 42)
                    let constant_fqn = FullyQualifiedName::Constant(combined_ns);
                    if index.contains_fqn(&constant_fqn) {
                        resolved = Some(constant_fqn);
                        break;
                    }
                }
                resolved
            };
            resolution
        };

        if let Some(fqn) = resolution {
            let location = self
                .document
                .prism_location_to_lsp_location(&node.location());
            self.staged.push_reference(fqn, location, None);
            return;
        }

        // Not found anywhere → optionally record unresolved.
        if self.track_unresolved {
            let location = self
                .document
                .prism_location_to_lsp_location(&node.location());
            let namespace_context: Vec<String> =
                current_namespace.iter().map(|c| c.to_string()).collect();
            self.staged.push_unresolved(
                self.document.uri.clone(),
                UnresolvedEntry::constant_with_context(
                    name.to_string(),
                    namespace_context,
                    location,
                ),
            );
        }
    }

    pub fn process_constant_read_node_exit(&mut self, _node: &ConstantReadNode) {
        // No cleanup needed
    }
}
