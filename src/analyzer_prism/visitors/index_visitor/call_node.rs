use ruby_prism::{CallNode, Node};

use crate::indexer::entry::MixinRef;
use crate::types::fully_qualified_name::FullyQualifiedName;

use super::IndexVisitor;
use crate::analyzer_prism::utils;

impl IndexVisitor {
    /// To index meta-programming
    /// Implemented: include, extend, prepend
    pub fn process_call_node_entry(&mut self, node: &CallNode) {
        let method_name = String::from_utf8_lossy(node.name().as_slice()).to_string();

        if node.receiver().is_some() {
            return;
        }

        // Skip require statements - we no longer track file dependencies
        if matches!(method_name.as_str(), "require" | "require_relative") {
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
                    // Store the mixin refs in the entry - resolution will happen later
                    match method_name.as_str() {
                        "include" => {
                            entry.add_includes(mixin_refs);
                        }
                        "extend" => {
                            entry.add_extends(mixin_refs);
                        }
                        "prepend" => {
                            entry.add_prepends(mixin_refs);
                        }
                        _ => {}
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
