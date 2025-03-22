use log::info;
use ruby_prism::DefNode;

use crate::indexer::{
    entry::{EntryBuilder, EntryType, Visibility},
    types::{constant::Constant, fully_qualified_constant::FullyQualifiedName, method::Method},
};

use super::Visitor;

impl Visitor {
    pub fn process_def_node_entry(&mut self, node: &DefNode) {
        info!(
            "Visiting def node: {}",
            String::from_utf8_lossy(node.name().as_slice())
        );

        // Get the current owner namespace
        let owner = self.owner_stack.last();
        if owner.is_none() {
            // No need to call visit_def_node here - it'll be called from mod.rs
            return;
        }

        // Extract the method name
        let method_name = String::from_utf8_lossy(node.name().as_slice()).to_string();

        // Store the current method name for param processing
        self.current_method = Some(method_name.clone());

        // Determine method visibility
        let visibility = self
            .visibility_stack
            .last()
            .cloned()
            .unwrap_or(Visibility::Public);

        // Get receiver information to determine if it's a singleton method
        let is_singleton_method = node.receiver().is_some();

        if is_singleton_method {
            // Handle singleton methods (class methods)
            if let Some(receiver) = node.receiver() {
                if receiver.as_self_node().is_some() {
                    // This is a class method (defined with self.)
                    if let Some(owner) = owner.cloned() {
                        // Create singleton class entry to use as the owner
                        let owner_name = owner.constant_name.to_string();
                        let singleton_name = format!("<Class:{}>", owner_name);
                        let _singleton_fqn = self.build_fully_qualified_name(
                            Constant::from(singleton_name.clone()),
                            None,
                        );

                        // Create method entry and add to index
                        let fqn = FullyQualifiedName::new(
                            vec![],
                            Some(Method::from(method_name.clone())),
                        );
                        let method_location = self.prism_loc_to_lsp_loc(node.location());
                        let _method_name_location = self.prism_loc_to_lsp_loc(node.name_loc());

                        let method_entry = EntryBuilder::new(Constant::from(method_name))
                            .fully_qualified_name(fqn)
                            .location(method_location)
                            .entry_type(EntryType::Method)
                            .visibility(visibility)
                            .build()
                            .unwrap();

                        self.index.lock().unwrap().add_entry(method_entry);
                    }
                }
            }
        } else {
            // Regular instance method
            if let Some(_) = owner.cloned() {
                let method_location = self.prism_loc_to_lsp_loc(node.location());
                let _method_name_location = self.prism_loc_to_lsp_loc(node.name_loc());
                let fqn = FullyQualifiedName::new(vec![], Some(Method::from(method_name.clone())));

                let method_entry = EntryBuilder::new(Constant::from(method_name))
                    .fully_qualified_name(fqn)
                    .location(method_location)
                    .entry_type(EntryType::Method)
                    .visibility(visibility)
                    .build()
                    .unwrap();

                self.index.lock().unwrap().add_entry(method_entry);
            }
        }

        // Descend into method body - will be called from mod.rs
        // visit_def_node(self, node);
    }

    pub fn process_def_node_exit(&mut self, _node: &DefNode) {
        // Clear the current method
        self.current_method = None;
    }
}
