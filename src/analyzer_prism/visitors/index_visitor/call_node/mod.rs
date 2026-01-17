use ruby_prism::CallNode;

use crate::indexer::entry::entry_kind::EntryKind;
use crate::indexer::entry::MixinRef;
use crate::types::compact_location::CompactLocation;
use crate::types::fully_qualified_name::FullyQualifiedName;

use super::IndexVisitor;
use crate::analyzer_prism::utils;

mod attr_macros;

impl IndexVisitor {
    /// To index meta-programming
    /// Implemented: include, extend, prepend
    pub fn process_call_node_entry(&mut self, node: &CallNode) {
        // Optimization: Fast fail for method calls with receivers (e.g. obj.method)
        if node.receiver().is_some() {
            return;
        }

        // Optimization: Fast fail for non-mixin methods without string allocation
        // We only care about include, extend, prepend, module_function, and attr_* macros
        let name_slice = node.name().as_slice();
        match name_slice {
            b"include" | b"extend" | b"prepend" => {}
            b"module_function" => {
                self.process_module_function(node);
                return;
            }
            b"attr_reader" | b"attr_writer" | b"attr_accessor" => {
                self.process_attr_macros(node);
                return;
            }
            _ => return,
        }

        let method_name = String::from_utf8_lossy(name_slice);

        if let Some(arguments) = node.arguments() {
            // Get the location of the include/extend/prepend call for provenance tracking
            let call_lsp_location = self
                .document
                .prism_location_to_lsp_location(&node.location());
            let file_id = self.index.lock().get_or_insert_file(&call_lsp_location.uri);
            let call_location = CompactLocation::new(file_id, call_lsp_location.range);

            let mixin_refs: Vec<MixinRef> = arguments
                .arguments()
                .iter()
                .filter_map(|arg| utils::mixin_ref_from_node(&arg, call_location.clone()))
                .collect();

            if mixin_refs.is_empty() {
                return;
            }

            let current_fqn = FullyQualifiedName::namespace(self.scope_tracker.get_ns_stack());

            let target_fqn = if current_fqn.is_empty() {
                // Top-level include/extend/prepend applies to Object
                // Use namespace() to create Namespace variant for class lookup
                if let Ok(constant) = crate::types::ruby_namespace::RubyConstant::new("Object") {
                    FullyQualifiedName::namespace(vec![constant])
                } else {
                    return;
                }
            } else {
                current_fqn
            };

            let mut index = self.index.lock();

            // Ensure Object exists if we are targeting it
            if target_fqn.to_string() == "Object" && index.get(&target_fqn).is_none() {
                let file_id = index.get_or_insert_file(&self.document.uri);
                let location = crate::types::compact_location::CompactLocation::new(
                    file_id,
                    self.document
                        .prism_location_to_lsp_location(&node.location())
                        .range,
                );

                let entry = crate::indexer::entry::EntryBuilder::new()
                    .fqn(target_fqn.clone())
                    .compact_location(location)
                    .kind(EntryKind::Class(Box::new(
                        crate::indexer::entry::entry_kind::ClassData {
                            superclass: None, // BasicObject implicit
                            includes: Vec::new(),
                            prepends: Vec::new(),
                            extends: Vec::new(),
                        },
                    )))
                    .build(&mut index)
                    .unwrap();
                index.add_entry(entry);
            }

            if let Some(entry) = index.get_last_definition_mut(&target_fqn) {
                // Only add mixins to class/module entries, not constants or other entries
                if !matches!(entry.kind, EntryKind::Class(_) | EntryKind::Module(_)) {
                    return;
                }
                match method_name.as_ref() {
                    "include" => entry.add_includes(mixin_refs),
                    "extend" => entry.add_extends(mixin_refs),
                    "prepend" => entry.add_prepends(mixin_refs),
                    _ => {}
                }
            }
        }
    }

    pub fn process_call_node_exit(&mut self, _node: &CallNode) {}

    /// Process `module_function :method_name` calls.
    /// This creates a singleton method entry for the module, pointing to the original
    /// instance method definition.
    fn process_module_function(&mut self, node: &CallNode) {
        use crate::indexer::entry::{EntryBuilder, MethodOrigin, MethodVisibility};
        use crate::types::ruby_method::RubyMethod;

        let Some(arguments) = node.arguments() else {
            return;
        };

        // Get current namespace
        let namespace_parts = self.scope_tracker.get_ns_stack();
        if namespace_parts.is_empty() {
            return; // module_function only makes sense inside a module
        }

        // Create owner as singleton namespace (this is a class method)
        let owner = FullyQualifiedName::singleton_namespace(namespace_parts.clone());

        for arg in arguments.arguments().iter() {
            // module_function accepts symbol arguments like :helper
            if let Some(symbol) = arg.as_symbol_node() {
                let method_name =
                    String::from_utf8_lossy(symbol.unescaped().as_ref()).to_string();

                // Create RubyMethod for the method name
                let method = match RubyMethod::new(&method_name) {
                    Ok(m) => m,
                    Err(_) => continue,
                };

                // Find the original instance method to get its location
                let instance_method_fqn =
                    FullyQualifiedName::method(namespace_parts.clone(), method.clone());

                // Look up the original method's location
                let compact_location = {
                    let index = self.index.lock();
                    if let Some(entries) = index.get(&instance_method_fqn) {
                        // Find the instance method (owner with Instance kind)
                        entries
                            .iter()
                            .find(|e| {
                                if let EntryKind::Method(data) = &e.kind {
                                    data.owner.namespace_kind()
                                        == Some(crate::indexer::entry::NamespaceKind::Instance)
                                } else {
                                    false
                                }
                            })
                            .map(|e| e.location.clone())
                    } else {
                        None
                    }
                };

                // If we found the original method, use its location
                // Otherwise fall back to the module_function call location
                let location = compact_location.unwrap_or_else(|| {
                    let call_lsp_location = self
                        .document
                        .prism_location_to_lsp_location(&node.location());
                    let file_id = self.index.lock().get_or_insert_file(&self.document.uri);
                    crate::types::compact_location::CompactLocation::new(
                        file_id,
                        call_lsp_location.range,
                    )
                });

                // Create the singleton method FQN (same method name, but with singleton owner)
                let method_fqn = FullyQualifiedName::method(namespace_parts.clone(), method.clone());

                // Create the method entry
                let entry = {
                    let mut index = self.index.lock();
                    EntryBuilder::new()
                        .fqn(method_fqn)
                        .compact_location(location)
                        .kind(EntryKind::new_method(
                            method,
                            Vec::new(), // No params info for module_function
                            owner.clone(),
                            MethodVisibility::Public,
                            MethodOrigin::Direct,
                            None, // origin_visibility
                            None, // yard_doc
                            None, // return_type_position
                            None, // return_type
                            Vec::new(), // param_types
                        ))
                        .build(&mut index)
                };

                if let Ok(entry) = entry {
                    self.add_entry(entry);
                }
            }
        }
    }
}
