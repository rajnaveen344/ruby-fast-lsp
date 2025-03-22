use log::info;
use ruby_prism::CallNode;

use crate::indexer::entry::Visibility;

use super::Visitor;

impl Visitor {
    pub fn process_call_node_entry(&mut self, node: &CallNode) {
        info!(
            "Visiting call node: {}",
            String::from_utf8_lossy(node.name().as_slice())
        );

        let message = String::from_utf8_lossy(node.name().as_slice()).to_string();

        // Handle special method calls
        match message.as_str() {
            "private" => {
                self.visibility_stack.push(Visibility::Private);
            }
            "protected" => {
                self.visibility_stack.push(Visibility::Protected);
            }
            "public" => {
                self.visibility_stack.push(Visibility::Public);
            }
            "attr_reader" | "attr_writer" | "attr_accessor" => {
                // Handle attribute methods
                if let Some(_owner) = self.owner_stack.last().cloned() {
                    // Process attribute declarations
                    if let Some(_args) = node.arguments() {
                        // Implement attr_* handling
                        // This is simplified; a complete implementation would traverse all arguments
                        // and handle string/symbol arguments correctly
                    }
                }
            }
            "include" | "prepend" | "extend" => {
                // Handle module operations
                // Simplified implementation; would need to extract included module names
            }
            _ => {
                // Regular method call
            }
        }

        // Visit children - will be called from mod.rs
        // visit_call_node(self, node);
    }

    pub fn process_call_node_exit(&mut self, node: &CallNode) {
        let message = String::from_utf8_lossy(node.name().as_slice()).to_string();

        // Clean up visibility stack on leaving the special method call
        match message.as_str() {
            "private" | "protected" | "public" => {
                // Only pop if we're not leaving a method def with this visibility
                if !node.arguments().map_or(false, |args| {
                    args.arguments()
                        .iter()
                        .any(|arg| arg.as_def_node().is_some())
                }) {
                    self.visibility_stack.pop();
                }
            }
            _ => {}
        }
    }
}
