use log::info;
use ruby_prism::SingletonClassNode;

use crate::indexer::{
    entry::{EntryBuilder, EntryType},
    types::constant::Constant,
};

use super::Visitor;

impl Visitor {
    pub fn process_singleton_class_node_entry(&mut self, node: &SingletonClassNode) {
        info!("Visiting singleton class node");

        // Get the current namespace
        let current_owner = self.owner_stack.last();

        if let Some(_owner) = current_owner {
            // Create a singleton class name for the current namespace
            let expression = node.expression();
            let is_self_node = expression.as_self_node().is_some();

            let current_name = if let Some(last_name) = self.namespace_stack.last() {
                last_name.to_string()
            } else {
                "Anonymous".to_string()
            };

            let singleton_name = if is_self_node {
                format!("<Class:{}>", current_name)
            } else {
                let expr_name = if let Some(cread) = expression.as_constant_read_node() {
                    String::from_utf8_lossy(cread.name().as_slice()).to_string()
                } else if let Some(_) = expression.as_constant_path_node() {
                    // For constant path nodes, we can't easily access the name
                    "Class".to_string()
                } else {
                    "Unknown".to_string()
                };
                format!("<Class:{}>", expr_name)
            };

            let fqn = self.build_fully_qualified_name(Constant::from(singleton_name.clone()), None);
            let location = self.prism_loc_to_lsp_loc(node.location());

            // Create a singleton class entry
            let entry = EntryBuilder::new(Constant::from(singleton_name.clone()))
                .fully_qualified_name(fqn.into())
                .location(location)
                .entry_type(EntryType::SingletonClass)
                .build()
                .unwrap();

            self.push_namespace(Constant::from(singleton_name), entry);

            // Process children - this will be called externally
            // visit_singleton_class_node(self, node); - Moved to mod.rs
        }
        // If no owner, we don't do any processing
    }

    pub fn process_singleton_class_node_exit(&mut self, _node: &SingletonClassNode) {
        // Only pop if we pushed (i.e., had an owner)
        if self.owner_stack.len() > 1 {
            self.pop_namespace();
        }
    }
}
