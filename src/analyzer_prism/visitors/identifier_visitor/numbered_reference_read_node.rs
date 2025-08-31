use ruby_prism::NumberedReferenceReadNode;

use crate::{
    analyzer_prism::Identifier,
    types::ruby_variable::{RubyVariable, RubyVariableKind},
};

use super::{IdentifierVisitor, IdentifierType};

impl IdentifierVisitor {
    pub fn process_numbered_reference_read_node_entry(&mut self, node: &NumberedReferenceReadNode) {
        if !self.is_position_in_location(&node.location()) {
            return;
        }

        if self.is_result_set() {
            return;
        }

        // Numbered references like $1, $2, etc. are special global variables
        let variable_name = format!("${}", node.number());
        
        // Create a RubyVariable with Global type
        let variable = match RubyVariable::new(&variable_name, RubyVariableKind::Global) {
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

    pub fn process_numbered_reference_read_node_exit(&mut self, _node: &NumberedReferenceReadNode) {
        // No cleanup needed for numbered reference read nodes
    }
}