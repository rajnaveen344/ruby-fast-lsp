use log::info;
use ruby_prism::CallNode;

use crate::indexer::entry::{EntryType, Visibility};
use crate::indexer::types::constant::Constant;
use crate::indexer::types::fully_qualified_constant::FullyQualifiedName;
use crate::indexer::types::method::Method;

use super::Visitor;

impl Visitor {
    pub fn process_call_node_entry(&mut self, node: &CallNode) {
        let method_name_str = String::from_utf8_lossy(node.name().as_slice()).to_string();
        info!("Visiting call node: {}", method_name_str);

        // Handle special method calls
        match method_name_str.as_str() {
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
                // Only index valid Ruby method names
                match Method::safe_from(method_name_str.as_str()) {
                    Ok(method) => {
                        // Clone the location so we can use it multiple times
                        let location = self.prism_loc_to_lsp_loc(node.location());
                        let mut namespaces = Vec::new();

                        // For method calls in a class/module context, use the current namespace
                        if let Some(owner_entry) = self.owner_stack.last() {
                            if owner_entry.entry_type == EntryType::Class
                                || owner_entry.entry_type == EntryType::Module
                            {
                                // Add the current namespace context to qualify the method call
                                if let Some(parts) = owner_entry
                                    .fully_qualified_name
                                    .to_string()
                                    .split("::")
                                    .next()
                                {
                                    namespaces.push(Constant::from(parts));
                                }
                            }
                        }

                        // Create the fully qualified name for this method call
                        let fqn = FullyQualifiedName::new(namespaces, Some(method.clone()));

                        // Add the reference to the index
                        info!("Adding method call reference: {}", fqn);
                        let mut index = self.index.lock().unwrap();
                        index.add_reference(fqn, location.clone());

                        // Also add a simple unqualified reference that can match any definition with this method name
                        let simple_fqn = FullyQualifiedName::new(Vec::new(), Some(method));
                        index.add_reference(simple_fqn, location);
                    }
                    Err(e) => {
                        // It's fine to just log and skip - not all method calls will match our regex
                        info!("Skipping invalid method name: {} - {}", method_name_str, e);
                    }
                }
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
