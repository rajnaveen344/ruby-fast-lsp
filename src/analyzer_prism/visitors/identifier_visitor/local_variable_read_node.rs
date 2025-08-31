use ruby_prism::{LocalVariableReadNode, LocalVariableWriteNode, ClassVariableReadNode, ClassVariableWriteNode, InstanceVariableReadNode, InstanceVariableWriteNode, GlobalVariableReadNode, GlobalVariableWriteNode};

use crate::{
    analyzer_prism::Identifier,
    types::ruby_variable::{RubyVariable, RubyVariableKind},
};

use super::{IdentifierVisitor, IdentifierType};

impl IdentifierVisitor {
    pub fn process_local_variable_read_node_entry(&mut self, node: &LocalVariableReadNode) {
        if self.is_result_set() || !self.is_position_in_location(&node.location()) {
            return;
        }

        let var_name = String::from_utf8_lossy(node.name().as_slice()).to_string();
        let var_type = RubyVariableKind::Local(self.scope_tracker.get_lv_stack().clone());
        let var = RubyVariable::new(&var_name, var_type).unwrap();

        self.set_result(
            Some(Identifier::RubyVariable { iden: var }),
            Some(IdentifierType::LVarRead),
            self.scope_tracker.get_ns_stack(),
            self.scope_tracker.get_lv_stack(),
        );
    }

    pub fn process_local_variable_read_node_exit(&mut self, _node: &LocalVariableReadNode) {
        // No cleanup needed
    }

    pub fn process_local_variable_write_node_entry(&mut self, node: &LocalVariableWriteNode) {
        if self.is_result_set() || !self.is_position_in_location(&node.location()) {
            return;
        }

        let name_loc = node.name_loc();
        if self.is_position_in_location(&name_loc) {
            let var_name = String::from_utf8_lossy(node.name().as_slice()).to_string();
            let var_type = RubyVariableKind::Local(self.scope_tracker.get_lv_stack().clone());
            let var = RubyVariable::new(&var_name, var_type).unwrap();

            self.set_result(
                Some(Identifier::RubyVariable { iden: var }),
                Some(IdentifierType::LVarDef),
                self.scope_tracker.get_ns_stack(),
                self.scope_tracker.get_lv_stack(),
            );
        }
    }

    pub fn process_local_variable_write_node_exit(&mut self, _node: &LocalVariableWriteNode) {
        // No cleanup needed
    }

    pub fn process_class_variable_read_node_entry(&mut self, node: &ClassVariableReadNode) {
        if self.is_result_set() || !self.is_position_in_location(&node.location()) {
            return;
        }

        let var_name = String::from_utf8_lossy(node.name().as_slice()).to_string();
        let var_type = RubyVariableKind::Class;
        let var = RubyVariable::new(&var_name, var_type).unwrap();

        self.set_result(
            Some(Identifier::RubyVariable { iden: var }),
            Some(IdentifierType::CVarRead),
            self.scope_tracker.get_ns_stack(),
            self.scope_tracker.get_lv_stack(),
        );
    }

    pub fn process_class_variable_read_node_exit(&mut self, _node: &ClassVariableReadNode) {
        // No cleanup needed
    }

    pub fn process_class_variable_write_node_entry(&mut self, node: &ClassVariableWriteNode) {
        if self.is_result_set() || !self.is_position_in_location(&node.location()) {
            return;
        }

        let name_loc = node.name_loc();
        if self.is_position_in_location(&name_loc) {
            let var_name = String::from_utf8_lossy(node.name().as_slice()).to_string();
            let var_type = RubyVariableKind::Class;
            let var = RubyVariable::new(&var_name, var_type).unwrap();

            self.set_result(
                Some(Identifier::RubyVariable { iden: var }),
                Some(IdentifierType::CVarDef),
                self.scope_tracker.get_ns_stack(),
                self.scope_tracker.get_lv_stack(),
            );
        }
    }

    pub fn process_class_variable_write_node_exit(&mut self, _node: &ClassVariableWriteNode) {
        // No cleanup needed
    }

    pub fn process_instance_variable_read_node_entry(&mut self, node: &InstanceVariableReadNode) {
        if self.is_result_set() || !self.is_position_in_location(&node.location()) {
            return;
        }

        let var_name = String::from_utf8_lossy(node.name().as_slice()).to_string();
        let var_type = RubyVariableKind::Instance;
        let var = RubyVariable::new(&var_name, var_type).unwrap();

        self.set_result(
            Some(Identifier::RubyVariable { iden: var }),
            Some(IdentifierType::IVarRead),
            self.scope_tracker.get_ns_stack(),
            self.scope_tracker.get_lv_stack(),
        );
    }

    pub fn process_instance_variable_read_node_exit(&mut self, _node: &InstanceVariableReadNode) {
        // No cleanup needed
    }

    pub fn process_instance_variable_write_node_entry(&mut self, node: &InstanceVariableWriteNode) {
        if self.is_result_set() || !self.is_position_in_location(&node.location()) {
            return;
        }

        let name_loc = node.name_loc();
        if self.is_position_in_location(&name_loc) {
            let var_name = String::from_utf8_lossy(node.name().as_slice()).to_string();
            let var_type = RubyVariableKind::Instance;
            let var = RubyVariable::new(&var_name, var_type).unwrap();

            self.set_result(
                Some(Identifier::RubyVariable { iden: var }),
                Some(IdentifierType::IVarDef),
                self.scope_tracker.get_ns_stack(),
                self.scope_tracker.get_lv_stack(),
            );
        }
    }

    pub fn process_instance_variable_write_node_exit(&mut self, _node: &InstanceVariableWriteNode) {
        // No cleanup needed
    }

    pub fn process_global_variable_read_node_entry(&mut self, node: &GlobalVariableReadNode) {
        if self.is_result_set() || !self.is_position_in_location(&node.location()) {
            return;
        }

        let var_name = String::from_utf8_lossy(node.name().as_slice()).to_string();
        let var_type = RubyVariableKind::Global;
        let var = RubyVariable::new(&var_name, var_type).unwrap();

        self.set_result(
            Some(Identifier::RubyVariable { iden: var }),
            Some(IdentifierType::GVarRead),
            self.scope_tracker.get_ns_stack(),
            self.scope_tracker.get_lv_stack(),
        );
    }

    pub fn process_global_variable_read_node_exit(&mut self, _node: &GlobalVariableReadNode) {
        // No cleanup needed
    }

    pub fn process_global_variable_write_node_entry(&mut self, node: &GlobalVariableWriteNode) {
        if self.is_result_set() || !self.is_position_in_location(&node.location()) {
            return;
        }

        let name_loc = node.name_loc();
        if self.is_position_in_location(&name_loc) {
            let var_name = String::from_utf8_lossy(node.name().as_slice()).to_string();
            let var_type = RubyVariableKind::Global;
            let var = RubyVariable::new(&var_name, var_type).unwrap();

            self.set_result(
                Some(Identifier::RubyVariable { iden: var }),
                Some(IdentifierType::GVarDef),
                self.scope_tracker.get_ns_stack(),
                self.scope_tracker.get_lv_stack(),
            );
        }
    }

    pub fn process_global_variable_write_node_exit(&mut self, _node: &GlobalVariableWriteNode) {
        // No cleanup needed
    }
}