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
        
        if node.receiver().is_some() {
            return;
        }

        if let Some(arguments) = node.arguments() {
            let mixin_refs: Vec<MixinRef> = arguments
                .arguments()
                .iter()
                .filter_map(|arg| self.resolve_mixin_ref(&arg))
                .collect();



            let current_fqn = FullyQualifiedName::namespace(self.scope_tracker.get_ns_stack());
            
            if current_fqn.is_empty() {
                return;
            }

            let mut index = self.index.lock();
            if let Some(entries) = index.get_mut(&current_fqn) {
                if let Some(entry) = entries.last_mut() {
                    let should_update_reverse_mixins = match mixin_kind.as_str() {
                        "include" => {
                            entry.add_includes(mixin_refs);
                            true
                        },
                        "extend" => {
                            entry.add_extends(mixin_refs);
                            true
                        },
                        "prepend" => {
                            entry.add_prepends(mixin_refs);
                            true
                        },
                        _ => {
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
                }
            }
        }
    }

    pub fn process_call_node_exit(&mut self, _node: &CallNode) {}

    fn resolve_mixin_ref(&self, node: &Node) -> Option<MixinRef> {
        utils::mixin_ref_from_node(node)
    }
}
