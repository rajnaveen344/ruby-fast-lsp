use ruby_prism::LocalVariableReadNode;

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

        // Use VariableScopes to record the reference
        self.document
            .variable_scopes_mut()
            .reference_variable(&variable_name, location);
    }

    pub fn process_local_variable_read_node_exit(&mut self, _node: &LocalVariableReadNode) {
        // No cleanup needed
    }
}
