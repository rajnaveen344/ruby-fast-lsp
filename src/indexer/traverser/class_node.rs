use log::info;
use ruby_prism::ClassNode;

use crate::indexer::{
    entry::{EntryBuilder, EntryType},
    types::constant::Constant,
};

use super::Visitor;

impl Visitor {
    pub fn process_class_node_entry(&mut self, node: &ClassNode) {
        info!(
            "Visiting class node: {}",
            String::from_utf8_lossy(node.name().as_slice())
        );

        let name = String::from_utf8_lossy(node.name().as_slice()).to_string();
        let full_loc = node.location();

        let fqn = self.build_fully_qualified_name(Constant::from(name.clone()), None);

        // Extract parent class information if available
        if let Some(superclass) = node.superclass() {
            if let Some(cread) = superclass.as_constant_read_node() {
                Some(String::from_utf8_lossy(cread.name().as_slice()).to_string())
            } else if let Some(_) = superclass.as_constant_path_node() {
                // For constant path nodes, we can't easily access the name
                // Just record a marker for now
                Some("ParentClass".to_string())
            } else {
                None
            }
        } else {
            // Default parent is Object unless this is already Object
            if name != "Object" {
                Some("Object".to_string())
            } else {
                None
            }
        };

        let entry = EntryBuilder::new(Constant::from(name.clone()))
            .fully_qualified_name(fqn.into())
            .location(self.prism_loc_to_lsp_loc(full_loc))
            .entry_type(EntryType::Class)
            .build()
            .unwrap();

        self.push_namespace(Constant::from(name), entry);

        // Process children - this will be called externally in mod.rs
        // visit_class_node(self, node);
    }

    pub fn process_class_node_exit(&mut self, _node: &ClassNode) {
        self.pop_namespace();
    }
}
