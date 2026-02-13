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

        // First, use ScopeTree to record the reference and find the scope
        if let Some((found_scope_id, _var_idx, _captured)) = self
            .document
            .scope_tree_mut()
            .reference_variable(&variable_name, location.clone())
        {
            // Also store in legacy lvar_references for now (until we fully migrate)
            self.document
                .add_lvar_reference(found_scope_id, ustr(&variable_name), location);
        }
    }

    pub fn process_local_variable_read_node_exit(&mut self, _node: &LocalVariableReadNode) {
        // No cleanup needed
    }
}
