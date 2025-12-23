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

            if current_fqn.is_empty() {
                return;
            }

            let mut index = self.index.lock();
            if let Some(entry) = index.get_last_definition_mut(&current_fqn) {
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
