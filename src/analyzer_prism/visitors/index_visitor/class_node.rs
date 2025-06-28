use log::{debug, error};
use ruby_prism::ClassNode;

use crate::analyzer_prism::utils;
use crate::indexer::entry::{entry_builder::EntryBuilder, entry_kind::EntryKind};
use crate::types::scope::LVScopeKind;
use crate::types::{fully_qualified_name::FullyQualifiedName, ruby_namespace::RubyConstant};

use super::IndexVisitor;

impl IndexVisitor {
    pub fn process_class_node_entry(&mut self, node: &ClassNode) {
        let name_str = String::from_utf8_lossy(node.name().as_slice()).to_string();
        debug!("Visiting class node: {}", name_str);

        let namespace = RubyConstant::new(&name_str);

        if let Err(e) = namespace {
            error!("Error creating namespace: {}", e);
            return;
        }

        let namespace = namespace.unwrap();

        let body_loc = if let Some(body) = node.body() {
            self.document
                .prism_location_to_lsp_location(&body.location())
        } else {
            self.document
                .prism_location_to_lsp_location(&node.location())
        };

        // Check if this is a constant path (e.g., A::B::C)
        let const_path = node.constant_path();
        let fqn = if let Some(path_node) = const_path.as_constant_path_node() {
            // Extract namespace parts from the constant path
            let mut namespace_parts = Vec::new();
            utils::collect_namespaces(&path_node, &mut namespace_parts);

            self.push_ns_scopes(namespace_parts);
            self.push_lv_scope(
                node.location().start_offset(),
                body_loc,
                LVScopeKind::Constant,
            );

            let current_namespace = self.current_namespace();

            FullyQualifiedName::namespace(current_namespace)
        } else {
            // For simple class definitions (not a constant path)
            self.push_ns_scope(namespace);
            self.push_lv_scope(
                node.location().start_offset(),
                body_loc,
                LVScopeKind::Constant,
            );

            let current_namespace = self.current_namespace();
            FullyQualifiedName::namespace(current_namespace)
        };

        debug!("Adding class entry: {:?}", fqn);

        let entry = EntryBuilder::new()
            .fqn(fqn)
            .location(self.prism_loc_to_lsp_loc(node.location()))
            .kind(EntryKind::new_class(None))
            .build();

        if let Err(e) = entry {
            error!("Error adding entry: {}", e);
            return;
        }

        debug!("Adding class entry: {}", entry.clone().unwrap().fqn);

        self.index.lock().unwrap().add_entry(entry.unwrap());
    }

    pub fn process_class_node_exit(&mut self, _node: &ClassNode) {
        self.pop_ns_scope();
        self.pop_lv_scope();
    }
}
