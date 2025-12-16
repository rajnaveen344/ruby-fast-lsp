use log::error;
use ruby_prism::ModuleNode;

use crate::analyzer_prism::utils;
use crate::indexer::entry::{entry_builder::EntryBuilder, entry_kind::EntryKind};
use crate::types::scope::{LVScope, LVScopeKind};
use crate::types::{fully_qualified_name::FullyQualifiedName, ruby_namespace::RubyConstant};

use super::IndexVisitor;

impl IndexVisitor {
    pub fn process_module_node_entry(&mut self, node: &ModuleNode) {
        let name_str = String::from_utf8_lossy(node.name().as_slice()).to_string();

        let body_loc = if let Some(body) = node.body() {
            self.document
                .prism_location_to_lsp_location(&body.location())
        } else {
            self.document
                .prism_location_to_lsp_location(&node.location())
        };

        let const_path = node.constant_path();
        if let Some(path_node) = const_path.as_constant_path_node() {
            let mut namespace_parts = Vec::new();
            utils::collect_namespaces(&path_node, &mut namespace_parts);
            self.scope_tracker.push_ns_scopes(namespace_parts);
        } else {
            match RubyConstant::new(&name_str) {
                Ok(namespace) => self.scope_tracker.push_ns_scope(namespace),
                Err(e) => {
                    error!("Error creating namespace: {}", e);
                    return;
                }
            }
        }

        let scope_id = self.document.position_to_offset(body_loc.range.start);
        self.scope_tracker
            .push_lv_scope(LVScope::new(scope_id, body_loc, LVScopeKind::Constant));

        let fqn = FullyQualifiedName::namespace(self.scope_tracker.get_ns_stack());

        let entry_result = {
            let mut index = self.index.lock();
            EntryBuilder::new()
                .fqn(fqn.clone())
                .location(
                    self.document
                        .prism_location_to_lsp_location(&node.location()),
                )
                .kind(EntryKind::new_module())
                .build(&mut index)
        };

        if let Err(e) = entry_result {
            error!("Error creating entry: {}", e);
            return;
        }

        self.add_entry(entry_result.unwrap());
    }

    pub fn process_module_node_exit(&mut self, _node: &ModuleNode) {
        self.scope_tracker.pop_ns_scope();
        self.scope_tracker.pop_lv_scope();
    }
}
