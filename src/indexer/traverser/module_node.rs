use log::info;
use ruby_prism::ModuleNode;

use crate::indexer::{
    entry::{EntryBuilder, EntryType},
    types::constant::Constant,
};

use super::Visitor;

impl Visitor {
    pub fn process_module_node_entry(&mut self, node: &ModuleNode) {
        info!(
            "Visiting module node: {}",
            String::from_utf8_lossy(node.name().as_slice())
        );
        let name = String::from_utf8_lossy(node.name().as_slice()).to_string();
        let full_loc = node.location();

        let fqn = self.build_fully_qualified_name(Constant::from(name.clone()), None);

        let entry = EntryBuilder::new(Constant::from(name.clone()))
            .fully_qualified_name(fqn.into())
            .location(self.prism_loc_to_lsp_loc(full_loc))
            .entry_type(EntryType::Module)
            .build()
            .unwrap();

        self.push_namespace(Constant::from(name), entry);

        // Process children - this will be called externally in mod.rs
        // visit_module_node(self, node);
    }

    pub fn process_module_node_exit(&mut self, _node: &ModuleNode) {
        self.pop_namespace();
    }
}
