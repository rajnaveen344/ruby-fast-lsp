use ruby_prism::{CallNode, Node};

use crate::indexer::dependency_tracker::RequireStatement;
use crate::indexer::entry::MixinRef;
use crate::types::fully_qualified_name::FullyQualifiedName;

use super::IndexVisitor;
use crate::analyzer_prism::utils;

impl IndexVisitor {
    /// To index meta-programming and dependency tracking
    /// Implemented: include, extend, prepend, require, require_relative
    pub fn process_call_node_entry(&mut self, node: &CallNode) {
        let method_name = String::from_utf8_lossy(node.name().as_slice()).to_string();

        if node.receiver().is_some() {
            return;
        }

        // Handle require statements for dependency tracking
        if matches!(method_name.as_str(), "require" | "require_relative") {
            self.process_require_statement(node, &method_name);
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

    /// Process require and require_relative statements for dependency tracking
    fn process_require_statement(&mut self, node: &CallNode, method_name: &str) {
        if let Some(arguments) = node.arguments() {
            let args = arguments.arguments();
            if let Some(first_arg) = args.iter().next() {
                if let Some(require_path) = self.extract_string_from_node(&first_arg) {
                    let require_stmt = RequireStatement {
                        path: require_path,
                        is_relative: method_name == "require_relative",
                        source_file: self.document.uri.clone(),
                    };

                    // Add to dependency tracker if available
                    if let Some(dependency_tracker) = &self.dependency_tracker {
                        let mut tracker = dependency_tracker.lock();
                        log::debug!(
                            "Added require statement to dependency tracker: {} (relative: {})",
                            require_stmt.path,
                            require_stmt.is_relative
                        );
                        tracker.add_require(require_stmt);
                    } else {
                        log::debug!(
                            "Found require statement but no dependency tracker available: {:?}",
                            require_stmt
                        );
                    }
                }
            }
        }
    }

    /// Extract string literal from a node (for require paths)
    fn extract_string_from_node(&self, node: &Node) -> Option<String> {
        if let Some(string_node) = node.as_string_node() {
            // Get the unescaped content of the string
            let unescaped = string_node.unescaped();
            Some(String::from_utf8_lossy(unescaped).to_string())
        } else if node.as_interpolated_string_node().is_some() {
            // For now, we don't handle interpolated strings in require statements
            // as they are dynamic and can't be resolved statically
            None
        } else {
            None
        }
    }
}
