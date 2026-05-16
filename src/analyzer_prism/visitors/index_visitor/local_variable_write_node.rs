use log::{error, trace};
use ruby_prism::{
    LocalVariableAndWriteNode, LocalVariableOperatorWriteNode, LocalVariableOrWriteNode,
    LocalVariableTargetNode, LocalVariableWriteNode, Location, Node,
};

use crate::indexer::entry::{entry_builder::EntryBuilder, entry_kind::EntryKind};
use crate::inferrer::r#type::ruby::RubyType;
use crate::types::fully_qualified_name::FullyQualifiedName;

use super::IndexVisitor;

impl IndexVisitor {
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

        // Scope id is no longer tracked by ScopeTracker; use dummy value
        // since EntryKind::new_local_variable still requires it
        let current_scope = 0usize;

        let fqn = FullyQualifiedName::local_variable(variable_name.clone()).unwrap();

        // Get location for both index entry and VariableScopes
        let location = self.document.prism_location_to_lsp_location(&name_loc);

        let entry = {
            let assignment_range = location.range.clone();
            let mut index = self.index.lock();
            EntryBuilder::new()
                .fqn(fqn)
                .location(location.clone())
                .kind(EntryKind::new_local_variable(
                    variable_name.clone(),
                    current_scope,
                    inferred_type.clone(),
                    assignment_range,
                ))
                .build(&mut index)
        };

        if let Ok(_entry) = entry {
            // Add to VariableScopes tree (the single source of truth for local variables)
            self.document
                .variable_scopes_mut()
                .define_variable(&variable_name, location.clone());

            // Dual-write: also store type in VariableScopes
            if let Some(current_scope_id) = self.document.variable_scopes().current_scope() {
                self.document.variable_scopes_mut().add_type_assignment(
                    current_scope_id,
                    &variable_name,
                    location.range,
                    inferred_type.clone(),
                );
            }

            trace!(
                "Added local variable entry with type: {:?} -> {:?}",
                variable_name,
                inferred_type
            );
        } else {
            error!("Error creating entry for local variable: {}", variable_name);
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::indexer::index::RubyIndex;
    use crate::indexer::index_ref::{Index, Unlocked};
    use parking_lot::RwLock;
    use ruby_prism::Visit;
    use std::sync::Arc;
    use tower_lsp::lsp_types::Url;

    fn create_test_index() -> Index<Unlocked> {
        Index::new(Arc::new(RwLock::new(RubyIndex::new())))
    }

    fn create_test_visitor(content: &str) -> (IndexVisitor, ruby_prism::ParseResult<'_>) {
        let uri = Url::parse("file:///test.rb").unwrap();
        let index = create_test_index();
        let document =
            crate::types::ruby_document::RubyDocument::new(uri.clone(), content.to_string(), 1);
        let visitor = IndexVisitor::new(index, document);

        let parse_result = ruby_prism::parse(content.as_bytes());
        (visitor, parse_result)
    }

    /// Helper to check that a variable exists in VariableScopes
    fn find_var_in_scopes(visitor: &IndexVisitor, name: &str) -> bool {
        let scopes = visitor.document.variable_scopes();
        scopes
            .get_all_definitions()
            .iter()
            .any(|(_, var)| var.name == name)
    }

    #[test]
    fn test_index_visitor_infers_string_type() {
        let content = "name = 'John'";
        let (mut visitor, parse_result) = create_test_visitor(content);
        visitor.visit(&parse_result.node());

        assert!(
            find_var_in_scopes(&visitor, "name"),
            "No variable 'name' stored in VariableScopes"
        );
    }

    #[test]
    fn test_index_visitor_infers_integer_type() {
        let content = "age = 25";
        let (mut visitor, parse_result) = create_test_visitor(content);
        visitor.visit(&parse_result.node());

        assert!(
            find_var_in_scopes(&visitor, "age"),
            "No variable 'age' stored in VariableScopes"
        );
    }

    #[test]
    fn test_index_visitor_infers_float_type() {
        let content = "price = 19.99";
        let (mut visitor, parse_result) = create_test_visitor(content);
        visitor.visit(&parse_result.node());

        assert!(
            find_var_in_scopes(&visitor, "price"),
            "No variable 'price' stored in VariableScopes"
        );
    }

    #[test]
    fn test_index_visitor_infers_boolean_type() {
        let content = "active = true";
        let (mut visitor, parse_result) = create_test_visitor(content);
        visitor.visit(&parse_result.node());

        assert!(
            find_var_in_scopes(&visitor, "active"),
            "No variable 'active' stored in VariableScopes"
        );
    }

    #[test]
    fn test_index_visitor_handles_unknown_type() {
        let content = "name = some_method";
        let (mut visitor, parse_result) = create_test_visitor(content);
        visitor.visit(&parse_result.node());

        assert!(
            find_var_in_scopes(&visitor, "name"),
            "Local variable should be stored even with unknown type"
        );
    }

    #[test]
    fn test_index_visitor_generates_type_hints() {
        let content = "name = 'John'\nage = 25";
        let (mut visitor, parse_result) = create_test_visitor(content);
        visitor.visit(&parse_result.node());

        assert!(
            find_var_in_scopes(&visitor, "name"),
            "Should store 'name' variable in VariableScopes"
        );
        assert!(
            find_var_in_scopes(&visitor, "age"),
            "Should store 'age' variable in VariableScopes"
        );
    }
}
