use ruby_prism::{ConstantPathNode, ConstantReadNode};

use crate::{
    analyzer_prism::{utils, Identifier},
    types::ruby_namespace::RubyConstant,
};

use super::{IdentifierType, IdentifierVisitor};

impl IdentifierVisitor {
    pub fn process_constant_path_node_entry(&mut self, node: &ConstantPathNode) {
        if self.is_result_set() || !self.is_position_in_location(&node.location()) {
            return;
        }

        // Based on a constant node target, a constant path node parent and a position, this method will find the exact
        // portion of the constant path that matches the requested position, for higher precision in hover and
        // definition. For example:
        //
        // ```ruby
        // Foo::Bar::BAZ
        //           ^ Going to definition here should go to Foo::Bar::BAZ
        //      ^ Going to definition here should go to Foo::Bar - Parent of ConstantPathNode BAZ
        // ^ Going to definition here should go to Foo - Parent of ConstantPathNode Bar
        // ```
        if let Some(parent_node) = node.parent() {
            if self.is_position_in_location(&parent_node.location()) {
                return;
            }
        }

        let mut namespaces = vec![];
        utils::collect_namespaces(node, &mut namespaces);

        // Check if first two char are ::
        let code = self.document.content.as_bytes();
        let start = node.location().start_offset();
        let end = start + 2;
        let target_str = String::from_utf8_lossy(&code[start..end]).to_string();
        let is_root_constant = target_str.starts_with("::");

        // Process the namespace
        if !namespaces.is_empty() {
            // Determine the namespace context based on whether it's a root constant
            let namespace_context = if is_root_constant {
                vec![] // Root constants have empty namespace context
            } else {
                self.scope_tracker.get_ns_stack()
            };

            self.set_result(
                Some(Identifier::RubyConstant {
                    namespace: namespace_context,
                    iden: namespaces,
                }),
                Some(IdentifierType::ConstantDef),
                self.scope_tracker.get_ns_stack(),
                self.scope_tracker.current_lv_scope().map(|s| s.scope_id()),
            );
        }
    }

    pub fn process_constant_path_node_exit(&mut self, _node: &ConstantPathNode) {
        // No cleanup needed for constant paths
    }

    pub fn process_constant_read_node_entry(&mut self, node: &ConstantReadNode) {
        if self.is_result_set() || !self.is_position_in_location(&node.location()) {
            return;
        }

        let name = String::from_utf8_lossy(node.name().as_slice()).to_string();
        let constant = RubyConstant::new(&name).unwrap();

        self.set_result(
            Some(Identifier::RubyConstant {
                namespace: self.scope_tracker.get_ns_stack(),
                iden: vec![constant],
            }),
            Some(IdentifierType::ConstantDef),
            self.scope_tracker.get_ns_stack(),
            self.scope_tracker.current_lv_scope().map(|s| s.scope_id()),
        );
    }

    pub fn process_constant_read_node_exit(&mut self, _node: &ConstantReadNode) {
        // No cleanup needed for constant reads
    }
}
