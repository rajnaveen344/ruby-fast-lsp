use log::error;
use ruby_prism::ModuleNode;

use crate::analyzer_prism::utils;
use crate::indexer::entry::{entry_builder::EntryBuilder, entry_kind::EntryKind, NamespaceKind};
use crate::types::fully_qualified_name::FullyQualifiedName;
use crate::types::scope::{LVScope, LVScopeKind};

use super::IndexVisitor;

impl IndexVisitor {
    pub fn process_module_node_entry(&mut self, node: &ModuleNode) {
        let body_loc = utils::get_body_location(
            node.body().map(|b| b.location()),
            &node.location(),
            &self.document,
        );

        if self
            .scope_tracker
            .push_namespace_from_constant_path(&node.constant_path(), node.name().as_slice())
            .is_err()
        {
            error!("Error creating namespace for module");
            return;
        }

        let scope_id = self.document.position_to_offset(body_loc.range.start);
        self.scope_tracker.push_lv_scope(LVScope::new(
            scope_id,
            body_loc.clone(),
            LVScopeKind::Constant,
        ));

        let module_name = String::from_utf8_lossy(node.name().as_slice()).to_string();
        self.document.scope_tree_mut().enter_scope(
            LVScopeKind::Constant,
            body_loc.range,
            Some(module_name),
        );

        let ns_stack = self.scope_tracker.get_ns_stack();
        let location = self
            .document
            .prism_location_to_lsp_location(&node.location());

        // Create Instance namespace entry (the module itself)
        let instance_fqn = FullyQualifiedName::Namespace(ns_stack.clone(), NamespaceKind::Instance);
        let instance_entry_result = {
            let mut index = self.index.lock();
            EntryBuilder::new()
                .fqn(instance_fqn)
                .location(location.clone())
                .kind(EntryKind::new_module())
                .build(&mut index)
        };

        if let Ok(entry) = instance_entry_result {
            self.add_entry(entry);
        } else {
            error!(
                "Error creating instance entry: {:?}",
                instance_entry_result.err()
            );
        }

        // Create Singleton namespace entry (the singleton class of the module)
        let singleton_fqn = FullyQualifiedName::Namespace(ns_stack, NamespaceKind::Singleton);
        let singleton_entry_result = {
            let mut index = self.index.lock();
            EntryBuilder::new()
                .fqn(singleton_fqn)
                .location(location)
                .kind(EntryKind::new_module()) // Module's singleton is also module-like
                .build(&mut index)
        };

        if let Ok(entry) = singleton_entry_result {
            self.add_entry(entry);
        } else {
            error!(
                "Error creating singleton entry: {:?}",
                singleton_entry_result.err()
            );
        }
    }

    pub fn process_module_node_exit(&mut self, _node: &ModuleNode) {
        self.scope_tracker.pop_ns_scope();
        self.scope_tracker.pop_lv_scope();
        self.document.scope_tree_mut().exit_scope();
    }
}
