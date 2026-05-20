use ruby_prism::LocalVariableReadNode;

use super::FactCollector;

impl FactCollector {
    pub fn process_local_variable_read_node_entry(&mut self, node: &LocalVariableReadNode) {
        if !self.include_local_vars {
            return;
        }

        let variable_name = crate::utf8_str(node.name().as_slice());
        let location = self.document.prism_location_to_text_range(&node.location());

        self.document
            .variable_scopes_mut()
            .reference_variable(variable_name, location);
    }

    pub fn process_local_variable_read_node_exit(&mut self, _node: &LocalVariableReadNode) {}
}
