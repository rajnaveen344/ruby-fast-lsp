use log::{error, info};
use ruby_prism::ModuleNode;

use crate::indexer::{
    entry::{entry_builder::EntryBuilder, entry_kind::EntryKind},
    types::{fully_qualified_name::FullyQualifiedName, ruby_namespace::RubyNamespace},
};

use super::Visitor;

impl Visitor {
    pub fn process_module_node_entry(&mut self, node: &ModuleNode) {
        let name_str = String::from_utf8_lossy(node.name().as_slice()).to_string();
        info!("Visiting module node: {}", name_str);

        let namespace = RubyNamespace::new(&name_str);

        if let Err(e) = namespace {
            error!("Error creating namespace: {}", e);
            return;
        }

        self.namespace_stack.push(namespace.unwrap());

        let fqn = FullyQualifiedName::namespace(self.namespace_stack.clone());

        let entry = EntryBuilder::new()
            .fqn(fqn)
            .location(self.prism_loc_to_lsp_loc(node.location()))
            .kind(EntryKind::Module)
            .build();

        if let Err(e) = entry {
            error!("Error creating entry: {}", e);
            return;
        }

        info!("Adding module entry: {}", entry.clone().unwrap().fqn);

        self.index.lock().unwrap().add_entry(entry.unwrap());
    }

    pub fn process_module_node_exit(&mut self, _node: &ModuleNode) {
        self.namespace_stack.pop();
    }
}
