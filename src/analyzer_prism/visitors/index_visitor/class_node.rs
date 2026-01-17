use log::error;
use ruby_prism::ClassNode;

use crate::analyzer_prism::utils;
use crate::indexer::entry::{entry_builder::EntryBuilder, entry_kind::EntryKind, MixinRef, NamespaceKind};
use crate::types::compact_location::CompactLocation;
use crate::types::scope::{LVScope, LVScopeKind};
use crate::types::{fully_qualified_name::FullyQualifiedName, ruby_namespace::RubyConstant};

use super::IndexVisitor;

impl IndexVisitor {
    pub fn process_class_node_entry(&mut self, node: &ClassNode) {
        let name_str = String::from_utf8_lossy(node.name().as_slice()).to_string();
        let body_loc = self.get_body_location(node);

        // Handle namespace setup
        if let Err(()) = self.setup_class_namespace(node, &name_str) {
            return;
        }

        // Setup local variable scope
        let scope_id = self.document.position_to_offset(body_loc.range.start);
        self.scope_tracker
            .push_lv_scope(LVScope::new(scope_id, body_loc.clone(), LVScopeKind::Constant));

        let ns_stack = self.scope_tracker.get_ns_stack();
        let location = self.document.prism_location_to_lsp_location(&node.location());

        // Create Instance namespace entry (the class itself)
        let instance_fqn = FullyQualifiedName::Namespace(ns_stack.clone(), NamespaceKind::Instance);
        let instance_entry_result = {
            let mut index = self.index.lock();
            EntryBuilder::new()
                .fqn(instance_fqn)
                .location(location.clone())
                .kind(EntryKind::new_class(None))
                .build(&mut index)
        };

        if let Ok(mut entry) = instance_entry_result {
            // Set superclass using MixinRef for deferred resolution
            if let Some(superclass_ref) = self.create_superclass_mixin_ref(node) {
                entry.set_superclass(superclass_ref);
            }
            self.add_entry(entry);
        } else {
            error!("Error creating instance entry: {:?}", instance_entry_result.err());
        }

        // Create Singleton namespace entry (the singleton class)
        // Reuse Class kind - the FQN's NamespaceKind::Singleton distinguishes it
        let singleton_fqn = FullyQualifiedName::Namespace(ns_stack, NamespaceKind::Singleton);
        let singleton_entry_result = {
            let mut index = self.index.lock();
            EntryBuilder::new()
                .fqn(singleton_fqn)
                .location(location)
                .kind(EntryKind::new_class(None)) // No explicit superclass for singleton
                .build(&mut index)
        };

        if let Ok(entry) = singleton_entry_result {
            self.add_entry(entry);
        } else {
            error!("Error creating singleton entry: {:?}", singleton_entry_result.err());
        }
    }

    pub fn process_class_node_exit(&mut self, _node: &ClassNode) {
        self.scope_tracker.pop_ns_scope();
        self.scope_tracker.pop_lv_scope();
    }

    /// Get the body location for a class node, falling back to the node location if no body exists
    fn get_body_location(&self, node: &ClassNode) -> tower_lsp::lsp_types::Location {
        if let Some(body) = node.body() {
            self.document
                .prism_location_to_lsp_location(&body.location())
        } else {
            self.document
                .prism_location_to_lsp_location(&node.location())
        }
    }

    /// Setup the namespace for a class, handling both constant paths and simple names
    fn setup_class_namespace(&mut self, node: &ClassNode, name_str: &str) -> Result<(), ()> {
        let const_path = node.constant_path();
        if let Some(path_node) = const_path.as_constant_path_node() {
            let mut namespace_parts = Vec::new();
            utils::collect_namespaces(&path_node, &mut namespace_parts);
            self.scope_tracker.push_ns_scopes(namespace_parts);
        } else {
            match RubyConstant::new(name_str) {
                Ok(namespace) => self.scope_tracker.push_ns_scope(namespace),
                Err(e) => {
                    error!("Error creating namespace: {}", e);
                    return Err(());
                }
            }
        }
        Ok(())
    }

    /// Create a MixinRef for the superclass constant path
    fn create_superclass_mixin_ref(&self, node: &ClassNode) -> Option<MixinRef> {
        if let Some(superclass_node) = node.superclass() {
            // Get location of the superclass declaration
            let lsp_location = self
                .document
                .prism_location_to_lsp_location(&superclass_node.location());
            let file_id = self.index.lock().get_or_insert_file(&lsp_location.uri);
            let location = CompactLocation::new(file_id, lsp_location.range);

            if let Some(const_read_node) = superclass_node.as_constant_read_node() {
                let superclass_name =
                    String::from_utf8_lossy(const_read_node.name().as_slice()).to_string();
                if let Ok(constant) = RubyConstant::new(&superclass_name) {
                    return Some(MixinRef {
                        parts: vec![constant],
                        absolute: false, // relative lookup
                        location,
                    });
                }
            } else if let Some(const_path_node) = superclass_node.as_constant_path_node() {
                let mut parts = Vec::new();
                utils::collect_namespaces(&const_path_node, &mut parts);
                if !parts.is_empty() {
                    let absolute = const_path_node.parent().is_none();
                    return Some(MixinRef {
                        parts,
                        absolute,
                        location,
                    });
                }
            }
        }
        None
    }
}
