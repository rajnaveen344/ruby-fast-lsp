use log::{debug, error};
use ruby_prism::ClassNode;

use crate::indexer::{
    entry::{entry_builder::EntryBuilder, entry_kind::EntryKind},
    types::{fully_qualified_name::FullyQualifiedName, ruby_namespace::RubyNamespace},
};

use super::IndexVisitor;

impl IndexVisitor {
    pub fn process_class_node_entry(&mut self, node: &ClassNode) {
        let name_str = String::from_utf8_lossy(node.name().as_slice()).to_string();
        debug!("Visiting class node: {}", name_str);

        let namespace = RubyNamespace::new(&name_str);

        if let Err(e) = namespace {
            error!("Error creating namespace: {}", e);
            return;
        }

        let namespace = namespace.unwrap();

        // Check if this is a constant path (e.g., A::B::C)
        let const_path = node.constant_path();
        let fqn = if let Some(path_node) = const_path.as_constant_path_node() {
            // Extract namespace parts from the constant path
            let mut namespace_parts = self.extract_namespace_parts(&path_node);
            // Add the current class name to the namespace parts
            namespace_parts.push(namespace.clone());
            // Push the namespace to the stack for proper scoping during traversal
            self.namespace_stack.extend(namespace_parts.clone());
            FullyQualifiedName::namespace(self.namespace_stack.clone())
        } else {
            // Regular class definition (not a constant path)
            self.namespace_stack.push(namespace);
            FullyQualifiedName::namespace(self.namespace_stack.clone())
        };

        let entry = EntryBuilder::new()
            .fqn(fqn)
            .location(self.prism_loc_to_lsp_loc(node.location()))
            .kind(EntryKind::Class {
                superclass: None,
                is_singleton: false,
            })
            .build();

        if let Err(e) = entry {
            error!("Error adding entry: {}", e);
            return;
        }

        debug!("Adding class entry: {}", entry.clone().unwrap().fqn);

        self.index.lock().unwrap().add_entry(entry.unwrap());
    }

    pub fn process_class_node_exit(&mut self, _node: &ClassNode) {
        self.namespace_stack.pop();
    }
}
