use log::debug;
use ruby_prism::{CallNode, Node};

use crate::indexer::entry::MixinRef;
use crate::types::fully_qualified_name::FullyQualifiedName;

use super::IndexVisitor;
use crate::analyzer_prism::utils;

impl IndexVisitor {
    /// To index meta-programming
    /// Implemented: include, extend, prepend
    pub fn process_call_node_entry(&mut self, node: &CallNode) {
        let mixin_kind = String::from_utf8_lossy(node.name().as_slice()).to_string();
        debug!("Processing call node: {}", mixin_kind);
        
        if node.receiver().is_some() {
            debug!("Call node has receiver, skipping: {}", mixin_kind);
            return;
        }

        if let Some(arguments) = node.arguments() {
            let mixin_refs: Vec<MixinRef> = arguments
                .arguments()
                .iter()
                .filter_map(|arg| self.resolve_mixin_ref(&arg))
                .collect();

            debug!("Found {} mixin refs for {}: {:?}", mixin_refs.len(), mixin_kind, mixin_refs);

            let current_fqn = FullyQualifiedName::namespace(self.scope_tracker.get_ns_stack());
            debug!("Current FQN: {:?}", current_fqn);
            
            if current_fqn.is_empty() {
                debug!("Cannot apply mixin to top-level");
                return;
            }

            let mut index = self.index.lock();
            if let Some(entries) = index.get_mut(&current_fqn) {
                if let Some(entry) = entries.last_mut() {
                    let should_update_reverse_mixins = match mixin_kind.as_str() {
                        "include" => {
                            debug!("Adding includes to {}: {:?}", current_fqn, mixin_refs);
                            entry.add_includes(mixin_refs);
                            true
                        },
                        "extend" => {
                            debug!("Adding extends to {}: {:?}", current_fqn, mixin_refs);
                            entry.add_extends(mixin_refs);
                            true
                        },
                        "prepend" => {
                            debug!("Adding prepends to {}: {:?}", current_fqn, mixin_refs);
                            entry.add_prepends(mixin_refs);
                            true
                        },
                        _ => {
                            debug!("Unknown mixin kind: {}", mixin_kind);
                            false
                        }
                    };
                    
                    // Update reverse mixin tracking after adding mixins
                    if should_update_reverse_mixins {
                        // Clone the entry to avoid borrow checker issues
                        let entry_clone = entry.clone();
                        let _ = entries; // Release the mutable borrow on entries
                        index.update_reverse_mixins(&entry_clone);
                    }
                } else {
                    debug!("No entry found for {}", current_fqn);
                }
            } else {
                debug!("No entries found for {}", current_fqn);
            }
        } else {
            debug!("No arguments for call: {}", mixin_kind);
        }
    }

    pub fn process_call_node_exit(&mut self, _node: &CallNode) {}

    fn resolve_mixin_ref(&self, node: &Node) -> Option<MixinRef> {
        utils::mixin_ref_from_node(node)
    }
}
