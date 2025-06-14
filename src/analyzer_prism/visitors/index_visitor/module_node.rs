use log::{debug, error, info};
use ruby_prism::ModuleNode;

use crate::analyzer_prism::utils;
use crate::indexer::entry::{entry_builder::EntryBuilder, entry_kind::EntryKind};
use crate::types::scope_kind::LVScopeKind;
use crate::types::{fully_qualified_name::FullyQualifiedName, ruby_namespace::RubyConstant};

use super::IndexVisitor;

impl IndexVisitor {
    pub fn process_module_node_entry(&mut self, node: &ModuleNode) {
        let name_str = String::from_utf8_lossy(node.name().as_slice()).to_string();
        debug!("Visiting module node: {}", name_str);

        let namespace = RubyConstant::new(&name_str);

        if let Err(e) = namespace {
            error!("Error creating namespace: {}", e);
            return;
        }

        let namespace = namespace.unwrap();

        // Check if this is a constant path (e.g., A::B::C)
        let const_path = node.constant_path();
        let fqn = if let Some(path_node) = const_path.as_constant_path_node() {
            // Extract namespace parts from the constant path
            let mut namespace_parts = Vec::new();
            utils::collect_namespaces(&path_node, &mut namespace_parts);
            self.push_ns_scopes(namespace_parts);
            self.push_lv_scope(LVScopeKind::Constant);

            // Get the current namespace (which includes all pushed scopes)
            let current_namespace = self.current_namespace();

            // Create a new namespace path that includes the module name
            let mut full_namespace = current_namespace;
            full_namespace.push(namespace);

            FullyQualifiedName::namespace(full_namespace)
        } else {
            self.push_ns_scope(namespace);
            self.push_lv_scope(LVScopeKind::Constant);

            // Get the current namespace (which includes the just-pushed namespace)
            let current_namespace = self.current_namespace();
            FullyQualifiedName::namespace(current_namespace)
        };

        info!("Adding module entry: {:?}", fqn);

        let entry = EntryBuilder::new()
            .fqn(fqn)
            .location(self.prism_loc_to_lsp_loc(node.location()))
            .kind(EntryKind::Module)
            .build();

        if let Err(e) = entry {
            error!("Error creating entry: {}", e);
            return;
        }

        debug!("Adding module entry: {}", entry.clone().unwrap().fqn);

        self.index.lock().unwrap().add_entry(entry.unwrap());
    }

    pub fn process_module_node_exit(&mut self, _node: &ModuleNode) {
        self.pop_ns_scope();
        self.pop_lv_scope();
    }
}
