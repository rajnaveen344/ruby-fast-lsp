use log::debug;
use ruby_prism::CallNode;

use crate::{
    analyzer_prism::utils,
    indexer::entry::MethodKind,
    types::{
        fully_qualified_name::FullyQualifiedName,
        ruby_method::RubyMethod,
        ruby_namespace::RubyConstant,
    },
};

use super::ReferenceVisitor;

impl ReferenceVisitor {
    pub fn process_call_node_entry(&mut self, node: &CallNode) {
        // Skip if this is a mixin call (include, extend, prepend) as those are handled by index_visitor
        let method_name = String::from_utf8_lossy(node.name().as_slice()).to_string();
        if matches!(method_name.as_str(), "include" | "extend" | "prepend") {
            return;
        }

        // Skip if this is an attribute accessor call
        if matches!(method_name.as_str(), "attr_reader" | "attr_writer" | "attr_accessor") {
            return;
        }

        // Skip method names that don't follow Ruby method naming conventions
        // (method names should start with lowercase letter or underscore)
        if !method_name.chars().next().map_or(false, |c| c.is_lowercase() || c == '_') {
            debug!("Skipping method call with invalid name: {}", method_name);
            return;
        }

        let location = self.document.prism_location_to_lsp_location(&node.location());
        let current_namespace = self.scope_tracker.get_ns_stack();

        // Determine the target namespace and method kind based on receiver
        let (target_namespace, method_kind) = if let Some(receiver_node) = node.receiver() {
            if let Some(_) = receiver_node.as_self_node() {
                // Self receiver - method is in current namespace
                (current_namespace.clone(), MethodKind::Instance)
            } else if let Some(constant_read) = receiver_node.as_constant_read_node() {
                // Constant receiver like "Class.method"
                let name = String::from_utf8_lossy(constant_read.name().as_slice()).to_string();
                if let Ok(constant) = RubyConstant::new(&name) {
                    let mut receiver_namespace = current_namespace.clone();
                    receiver_namespace.push(constant);
                    (receiver_namespace, MethodKind::Class)
                } else {
                    (current_namespace.clone(), MethodKind::Unknown)
                }
            } else if let Some(constant_path) = receiver_node.as_constant_path_node() {
                // Qualified constant receiver like "Module::Class.method"
                if let Some(mixin_ref) = utils::mixin_ref_from_node(&receiver_node) {
                    let final_namespace = if mixin_ref.absolute {
                        mixin_ref.parts
                    } else {
                        // For relative paths, resolve by checking namespace hierarchy
                        // Ruby constant resolution: look for the first part in current namespace and ancestors
                        if let Some(first_part) = mixin_ref.parts.first() {
                            let mut resolved = None;
                            
                            // Check if the first constant exists in current namespace or ancestors
                             for i in (0..=current_namespace.len()).rev() {
                                 let test_namespace = &current_namespace[..i];
                                 
                                 // Check if first_part exists at this level
                                 // Look for the constant in the namespace parts
                                 if test_namespace.iter().any(|c| c.to_string() == first_part.to_string()) {
                                     // Found the constant in the namespace hierarchy
                                      // Find the position where this constant appears
                                      if let Some(pos) = test_namespace.iter().position(|c| c.to_string() == first_part.to_string()) {
                                          let mut result = test_namespace[..=pos].to_vec();
                                          // Skip the first part since it's already in the namespace
                                          result.extend(mixin_ref.parts.iter().skip(1).cloned());
                                          
                                          resolved = Some(result);
                                          break;
                                      }
                                 }
                             }
                            
                            // If not found in hierarchy, try from root
                             let result = resolved.unwrap_or_else(|| {
                                 // Check if it should be resolved from a parent namespace
                                 // For Platform::PlatformServices in GoshPosh::Platform::SpecHelpers,
                                 // Platform should resolve to GoshPosh::Platform
                                 if current_namespace.len() >= 2 {
                                      let parent_ns = &current_namespace[..current_namespace.len() - 1];
                                      if parent_ns.last().map(|c| c.to_string()) == Some(first_part.to_string()) {
                                          let mut result = parent_ns.to_vec();
                                          result.extend(mixin_ref.parts.iter().cloned());
                                          return result;
                                      }
                                  }
                                  
                                  // Default: append to current namespace
                                  let mut ns = current_namespace.clone();
                                  ns.extend(mixin_ref.parts.iter().cloned());
                                  ns
                              });
                              result
                        } else {
                            current_namespace.clone()
                        }
                    };
                    (final_namespace, MethodKind::Class)
                } else {
                    (current_namespace.clone(), MethodKind::Unknown)
                }
            } else {
                // Expression receiver - use current namespace
                (current_namespace.clone(), MethodKind::Instance)
            }
        } else {
            // No receiver - instance method call in current namespace
            (current_namespace.clone(), MethodKind::Instance)
        };

        // Create the method, handling potential validation errors gracefully
        let method = match RubyMethod::new(&method_name, method_kind) {
            Ok(method) => method,
            Err(err) => {
                debug!("Failed to create RubyMethod for '{}': {}", method_name, err);
                return;
            }
        };

        let method_fqn = FullyQualifiedName::method(target_namespace, method);

        debug!(
            "Adding method call reference: {} at {:?}",
            method_fqn.to_string(),
            location
        );

        // Add the reference to the index
        let mut index = self.index.lock();
        index.add_reference(method_fqn, location);
    }

    pub fn process_call_node_exit(&mut self, _node: &CallNode) {
        // Nothing to do on exit
    }
}