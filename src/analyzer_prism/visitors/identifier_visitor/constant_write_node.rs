use ruby_prism::ConstantWriteNode;

use crate::{
    analyzer_prism::Identifier,
    types::ruby_namespace::RubyConstant,
};

use super::{IdentifierVisitor, IdentifierType};

impl IdentifierVisitor {
    pub fn process_constant_write_node_entry(&mut self, node: &ConstantWriteNode) {
        if self.is_result_set() || !self.is_position_in_location(&node.location()) {
            return;
        }

        let name = String::from_utf8_lossy(node.name().as_slice()).to_string();
        let constant = RubyConstant::new(&name).unwrap();

        let name_loc = node.name_loc();
        if self.is_position_in_location(&name_loc) {
            self.set_result(
                Some(Identifier::RubyConstant {
                    namespace: self.scope_tracker.get_ns_stack(),
                    iden: vec![constant],
                }),
                Some(IdentifierType::ConstantDef),
                self.scope_tracker.get_ns_stack(),
                self.scope_tracker.get_lv_stack(),
            );
            return;
        }
    }

    pub fn process_constant_write_node_exit(&mut self, _node: &ConstantWriteNode) {
        // No cleanup needed for constant writes
    }
}