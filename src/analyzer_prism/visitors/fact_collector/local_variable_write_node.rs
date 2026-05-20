use log::error;
use ruby_analysis_core::{TypeFact, TypeProvenance, TypeSubject};
use ruby_prism::{
    LocalVariableAndWriteNode, LocalVariableOperatorWriteNode, LocalVariableOrWriteNode,
    LocalVariableTargetNode, LocalVariableWriteNode, Location, Node,
};

use super::FactCollector;
use crate::inferrer::r#type::ruby::RubyType;

impl FactCollector {
    /// Process local variable write with type inference
    fn process_local_variable_write(
        &mut self,
        name: &[u8],
        name_loc: Location,
        value_node: Option<&Node>,
    ) {
        let variable_name = String::from_utf8_lossy(name).to_string();

        // Infer type from value if available
        let inferred_type = if let Some(value) = value_node {
            self.infer_type_from_value(value)
        } else {
            RubyType::Unknown
        };

        // Validate the variable name
        if variable_name.is_empty() {
            error!("Local variable name cannot be empty");
            return;
        }

        let mut chars = variable_name.chars();
        let first = chars.next().unwrap();

        // Local variables must start with lowercase or underscore
        if !(first.is_lowercase() || first == '_') {
            error!(
                "Local variable name must start with lowercase or _: {}",
                variable_name
            );
            return;
        }

        // Check for valid characters (alphanumeric and underscore)
        if !variable_name
            .chars()
            .all(|c| c.is_alphanumeric() || c == '_')
        {
            error!(
                "Local variable name contains invalid characters: {}",
                variable_name
            );
            return;
        }

        // Get location for both index entry and VariableScopes
        let location = self.document.prism_location_to_lsp_location(&name_loc);

        self.document
            .variable_scopes_mut()
            .define_variable(&variable_name, location.clone());

        if let Some(current_scope_id) = self.document.variable_scopes().current_scope() {
            self.document.variable_scopes_mut().add_type_assignment(
                current_scope_id,
                &variable_name,
                location.range,
                inferred_type.clone(),
            );
            let scope_id = u32::try_from(current_scope_id).expect(
                "INVARIANT VIOLATED: local variable scope id exceeded u32. \
                 This is a bug because ruby-analysis-core TypeSubject::Local stores u32 scope ids. \
                 Fix: widen TypeSubject::Local scope_id before indexing more than u32::MAX scopes.",
            );
            self.type_store.add(TypeFact::new(
                TypeSubject::Local {
                    scope_id,
                    name: variable_name.clone(),
                },
                inferred_type.clone(),
                self.document.prism_location_to_text_range(&name_loc),
                TypeProvenance::Assignment,
            ));
        }
    }

    // LocalVariableWriteNode
    pub fn process_local_variable_write_node_entry(&mut self, node: &LocalVariableWriteNode) {
        self.process_local_variable_write(
            node.name().as_slice(),
            node.name_loc(),
            Some(&node.value()),
        );
    }

    pub fn process_local_variable_write_node_exit(&mut self, _node: &LocalVariableWriteNode) {
        // No-op for now
    }

    // LocalVariableTargetNode
    pub fn process_local_variable_target_node_entry(&mut self, node: &LocalVariableTargetNode) {
        self.process_local_variable_write(node.name().as_slice(), node.location(), None);
    }

    pub fn process_local_variable_target_node_exit(&mut self, _node: &LocalVariableTargetNode) {
        // No-op for now
    }

    // LocalVariableOrWriteNode
    pub fn process_local_variable_or_write_node_entry(&mut self, node: &LocalVariableOrWriteNode) {
        self.process_local_variable_write(
            node.name().as_slice(),
            node.name_loc(),
            Some(&node.value()),
        );
    }

    pub fn process_local_variable_or_write_node_exit(&mut self, _node: &LocalVariableOrWriteNode) {
        // No-op for now
    }

    // LocalVariableAndWriteNode
    pub fn process_local_variable_and_write_node_entry(
        &mut self,
        node: &LocalVariableAndWriteNode,
    ) {
        self.process_local_variable_write(
            node.name().as_slice(),
            node.name_loc(),
            Some(&node.value()),
        );
    }

    pub fn process_local_variable_and_write_node_exit(
        &mut self,
        _node: &LocalVariableAndWriteNode,
    ) {
        // No-op for now
    }

    // LocalVariableOperatorWriteNode
    pub fn process_local_variable_operator_write_node_entry(
        &mut self,
        node: &LocalVariableOperatorWriteNode,
    ) {
        self.process_local_variable_write(
            node.name().as_slice(),
            node.name_loc(),
            Some(&node.value()),
        );
    }

    pub fn process_local_variable_operator_write_node_exit(
        &mut self,
        _node: &LocalVariableOperatorWriteNode,
    ) {
        // No-op for now
    }
}
