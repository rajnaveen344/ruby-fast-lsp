use log::debug;
use ruby_prism::LocalVariableReadNode;

use crate::types::{
    fully_qualified_name::FullyQualifiedName,
    ruby_variable::{RubyVariable, RubyVariableType},
};

use super::ReferenceVisitor;

impl ReferenceVisitor {
    pub fn process_local_variable_read_node_entry(&mut self, node: &LocalVariableReadNode) {
        if !self.include_local_vars {
            return;
        }

        let variable_name = String::from_utf8_lossy(node.name().as_slice()).to_string();
        let location = self
            .document
            .prism_location_to_lsp_location(&node.location());
        let mut index = self.index.lock();

        // Search through scope stack from innermost to outermost scope
        let lv_stack = self.scope_tracker.get_lv_stack();
        for i in (0..lv_stack.len()).rev() {
            // Take all scopes up to the current level
            let scopes = lv_stack[0..=i].to_vec();
            let var_type = RubyVariableType::Local(scopes);

            if let Ok(variable) = RubyVariable::new(&variable_name, var_type) {
                let fqn = FullyQualifiedName::variable(variable);

                debug!("Searching for variable: {:?}", fqn);

                // Check if this variable is defined in the current scope level
                if index.definitions.contains_key(&fqn) {
                    debug!(
                        "Adding local variable reference: {:?} at {:?}",
                        fqn, location
                    );
                    index.add_reference(fqn, location);
                    drop(index);
                    return;
                }
            }
        }

        // If we get here, no matching definition was found in any scope
        debug!(
            "No definition found for local variable '{}' at {:?}",
            variable_name, location
        );
        drop(index);
    }

    pub fn process_local_variable_read_node_exit(&mut self, _node: &LocalVariableReadNode) {
        // No cleanup needed
    }
}
