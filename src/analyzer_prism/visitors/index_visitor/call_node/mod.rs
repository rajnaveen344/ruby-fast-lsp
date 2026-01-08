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
        // We only care about include, extend, prepend, and attr_* macros
        let name_slice = node.name().as_slice();
        match name_slice {
            b"include" | b"extend" | b"prepend" => {}
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
                if let Ok(fqn) = FullyQualifiedName::try_from("Object") {
                    fqn
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
}
