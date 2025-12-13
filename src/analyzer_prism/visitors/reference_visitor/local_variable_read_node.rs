use ruby_prism::LocalVariableReadNode;
use ustr::ustr;

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

        // LocalVariable references are stored in document.lvar_references (NOT global index)
        let lv_stack = self.scope_tracker.get_lv_stack();
        let scope_ids: Vec<_> = lv_stack.iter().map(|s| s.scope_id()).collect();

        if let Some(found_scope_id) = self
            .document
            .find_local_var_scope(&variable_name, &scope_ids)
        {
            self.document
                .add_lvar_reference(found_scope_id, ustr(&variable_name), location);
        }
    }

    pub fn process_local_variable_read_node_exit(&mut self, _node: &LocalVariableReadNode) {
        // No cleanup needed
    }
}
