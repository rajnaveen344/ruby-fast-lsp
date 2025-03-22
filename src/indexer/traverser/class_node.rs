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
        let parent_class_name = if let Some(superclass) = node.superclass() {
            if let Some(cread) = superclass.as_constant_read_node() {
                let parent_name = String::from_utf8_lossy(cread.name().as_slice()).to_string();
                info!("Class {} inherits from {}", name, parent_name);
                Some(parent_name)
            } else if let Some(cpath) = superclass.as_constant_path_node() {
                // For constant path nodes, we need to access the final part
                if let Some(final_name) = self.extract_constant_path_name(&cpath) {
                    info!("Class {} inherits from {} via path", name, final_name);
                    Some(final_name)
                } else {
                    info!("Class {} has a complex parent class path", name);
                    None
                }
            } else {
                info!("Class {} has an unknown parent class type", name);
                None
            }
        } else {
            // Default parent is Object unless this is already Object
            if name != "Object" {
                info!("Class {} implicitly inherits from Object", name);
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

        // Record the parent class in the namespace_ancestors map
        if let Some(parent_name) = parent_class_name {
            let mut index = self.index.lock().unwrap();
            index.add_namespace_ancestor(Constant::from(name.clone()), Constant::from(parent_name));
        }

        self.push_namespace(Constant::from(name), entry);

        // Process children - this will be called externally in mod.rs
        // visit_class_node(self, node);
    }

    pub fn process_class_node_exit(&mut self, _node: &ClassNode) {
        self.pop_namespace();
    }
}
