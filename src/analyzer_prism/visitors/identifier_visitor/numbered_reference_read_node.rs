use ruby_prism::NumberedReferenceReadNode;

use crate::analyzer_prism::Identifier;

use super::{IdentifierType, IdentifierVisitor};

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

        let identifier = Identifier::RubyGlobalVariable {
            namespace: self.scope_tracker.get_ns_stack(),
            name: variable_name,
        };

        self.set_result(
            Some(identifier),
            Some(IdentifierType::GVarRead),
            self.scope_tracker.get_ns_stack(),
            self.scope_tracker.current_lv_scope().map(|s| s.scope_id()),
        );
    }

    pub fn process_numbered_reference_read_node_exit(&mut self, _node: &NumberedReferenceReadNode) {
        // No cleanup needed for numbered reference read nodes
    }
}
