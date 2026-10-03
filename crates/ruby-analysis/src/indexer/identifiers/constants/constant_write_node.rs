use ruby_prism::{
    ConstantAndWriteNode, ConstantOperatorWriteNode, ConstantOrWriteNode, ConstantPathWriteNode,
    ConstantTargetNode, ConstantWriteNode, Location,
};

use crate::core::RubyConstant;
use crate::indexer::documents::scope_rules::declaration_candidates;
use crate::indexer::{mixin_ref_from_node, Identifier};

use crate::indexer::identifiers::{IdentifierType, IdentifierVisitor};

impl IdentifierVisitor<'_> {
    fn set_simple_constant_def_at_name(&mut self, name: &[u8], name_loc: &Location<'_>) {
        if self.is_result_set() || !self.is_position_in_location(name_loc) {
            return;
        }

        let name = String::from_utf8_lossy(name).to_string();
        let constant = RubyConstant::new(&name).unwrap();
        self.set_result(
            Some(Identifier::RubyConstant {
                namespace: self.scope_tracker.get_ns_stack(),
                iden: vec![constant],
            }),
            Some(IdentifierType::ConstantDef),
            self.scope_tracker.get_ns_stack(),
            Some(0),
        );
    }

    pub fn process_constant_write_node_entry(&mut self, node: &ConstantWriteNode) {
        if let Some(value) = self.constant_value(&node.value()) {
            let name = String::from_utf8_lossy(node.name().as_slice());
            if let Ok(constant) = RubyConstant::new(&name) {
                let mut key = self.scope_tracker.get_ns_stack();
                key.push(constant);
                self.file_constant_types.insert(key, value);
            }
        }
        if self.is_result_set() || !self.is_position_in_location(&node.location()) {
            return;
        }
        self.set_simple_constant_def_at_name(node.name().as_slice(), &node.name_loc());
    }

    pub fn record_constant_path_write(&mut self, node: &ConstantPathWriteNode) {
        let Some(value) = self.constant_value(&node.value()) else {
            return;
        };
        let target = node.target().as_node();
        let Some(reference) = mixin_ref_from_node(&target) else {
            return;
        };
        let lexical_context = self.scope_tracker.get_ns_stack();
        let mut candidates =
            declaration_candidates(&reference.parts, reference.absolute, &lexical_context);
        self.file_constant_types
            .insert(candidates.swap_remove(0), value);
    }

    pub fn process_constant_write_node_exit(&mut self, _node: &ConstantWriteNode) {}

    pub fn process_constant_or_write_node_entry(&mut self, node: &ConstantOrWriteNode) {
        if self.is_result_set() || !self.is_position_in_location(&node.location()) {
            return;
        }
        self.set_simple_constant_def_at_name(node.name().as_slice(), &node.name_loc());
    }

    pub fn process_constant_or_write_node_exit(&mut self, _node: &ConstantOrWriteNode) {}

    pub fn process_constant_and_write_node_entry(&mut self, node: &ConstantAndWriteNode) {
        if self.is_result_set() || !self.is_position_in_location(&node.location()) {
            return;
        }
        self.set_simple_constant_def_at_name(node.name().as_slice(), &node.name_loc());
    }

    pub fn process_constant_and_write_node_exit(&mut self, _node: &ConstantAndWriteNode) {}

    pub fn process_constant_operator_write_node_entry(&mut self, node: &ConstantOperatorWriteNode) {
        if self.is_result_set() || !self.is_position_in_location(&node.location()) {
            return;
        }
        self.set_simple_constant_def_at_name(node.name().as_slice(), &node.name_loc());
    }

    pub fn process_constant_operator_write_node_exit(&mut self, _node: &ConstantOperatorWriteNode) {
    }

    pub fn process_constant_target_node_entry(&mut self, node: &ConstantTargetNode) {
        if self.is_result_set() || !self.is_position_in_location(&node.location()) {
            return;
        }
        self.set_simple_constant_def_at_name(node.name().as_slice(), &node.location());
    }

    pub fn process_constant_target_node_exit(&mut self, _node: &ConstantTargetNode) {}
}
