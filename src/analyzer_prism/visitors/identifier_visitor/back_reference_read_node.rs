use ruby_prism::BackReferenceReadNode;

use crate::{
    analyzer_prism::Identifier,
    types::ruby_variable::{RubyVariable, RubyVariableKind},
};

use super::{IdentifierVisitor, IdentifierType};

impl IdentifierVisitor {
    pub fn process_back_reference_read_node_entry(&mut self, node: &BackReferenceReadNode) {
        if !self.is_position_in_location(&node.location()) {
            return;
        }

        if self.is_result_set() {
            return;
        }

        // Back references like $&, $+, etc. are special global variables
        let variable_name = String::from_utf8_lossy(node.name().as_slice()).to_string();
        // The name already includes the $, so don't add another one
        let full_name = variable_name;
        
        // Create a RubyVariable with Global type
        let variable = match RubyVariable::new(&full_name, RubyVariableKind::Global) {
            Ok(var) => var,
            Err(_) => {
                // If validation fails, skip this variable
                return;
            }
        };

        let identifier = Identifier::RubyVariable { iden: variable };

        self.set_result(
            Some(identifier),
            Some(IdentifierType::GVarRead),
            self.scope_tracker.get_ns_stack(),
            self.scope_tracker.get_lv_stack(),
        );
    }

    pub fn process_back_reference_read_node_exit(&mut self, _node: &BackReferenceReadNode) {
        // No cleanup needed for back reference read nodes
    }
}