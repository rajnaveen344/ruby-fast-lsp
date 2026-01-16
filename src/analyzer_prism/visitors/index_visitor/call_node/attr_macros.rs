use ruby_prism::CallNode;

use crate::types::fully_qualified_name::FullyQualifiedName;

use super::super::IndexVisitor;

impl IndexVisitor {
    pub(crate) fn process_attr_macros(&mut self, node: &CallNode) {
        let name_slice = node.name().as_slice();
        let (is_reader, is_writer) = match name_slice {
            b"attr_reader" => (true, false),
            b"attr_writer" => (false, true),
            b"attr_accessor" => (true, true),
            _ => return,
        };

        let Some(arguments) = node.arguments() else {
            return;
        };

        // Determine namespace kind based on scope (like def_node)
        let namespace_kind = if self.scope_tracker.in_singleton() {
            crate::indexer::entry::NamespaceKind::Singleton
        } else {
            crate::indexer::entry::NamespaceKind::Instance
        };

        let namespace_parts = self.scope_tracker.get_ns_stack();
        let owner_fqn = FullyQualifiedName::namespace_with_kind(namespace_parts.clone(), namespace_kind);

        for arg in arguments.arguments().iter() {
            let (name, location) = if let Some(sym_node) = arg.as_symbol_node() {
                (
                    String::from_utf8_lossy(sym_node.unescaped()).to_string(),
                    sym_node.location(),
                )
            } else if let Some(str_node) = arg.as_string_node() {
                (
                    String::from_utf8_lossy(str_node.unescaped()).to_string(),
                    str_node.content_loc(),
                )
            } else {
                continue;
            };

            let lsp_location = self.document.prism_location_to_lsp_location(&location);
            let file_id = self.index.lock().get_or_insert_file(&self.document.uri);
            let compact_location =
                crate::types::compact_location::CompactLocation::new(file_id, lsp_location.range);

            if is_reader {
                self.create_attr_method_entry(
                    &name,
                    &namespace_parts,
                    &owner_fqn,
                    compact_location.clone(),
                );
            }

            if is_writer {
                let sorted_name = format!("{}=", name);
                self.create_attr_method_entry(
                    &sorted_name,
                    &namespace_parts,
                    &owner_fqn,
                    compact_location.clone(),
                );
            }
        }
    }

    fn create_attr_method_entry(
        &mut self,
        name: &str,
        namespace: &Vec<crate::types::ruby_namespace::RubyConstant>,
        owner_fqn: &FullyQualifiedName,
        location: crate::types::compact_location::CompactLocation,
    ) {
        use crate::indexer::entry::{EntryBuilder, EntryKind, MethodOrigin, MethodVisibility};
        use crate::types::ruby_method::RubyMethod;

        let method = RubyMethod::new(name).unwrap();
        let fqn = FullyQualifiedName::method(namespace.clone(), method.clone());

        let entry = {
            let mut index = self.index.lock();
            EntryBuilder::new()
                .fqn(fqn)
                .compact_location(location)
                .kind(EntryKind::new_method(
                    method,
                    Vec::new(), // No params info extracted yet for attrs
                    owner_fqn.clone(),
                    MethodVisibility::Public,
                    MethodOrigin::Direct, // Treat as direct definition for now
                    None,                 // origin_visibility
                    None,                 // yard_doc
                    None,                 // return_type_position
                    None,                 // return_type
                    Vec::new(),           // param_types
                ))
                .build(&mut index)
                .unwrap()
        };

        self.add_entry(entry);
    }
}
