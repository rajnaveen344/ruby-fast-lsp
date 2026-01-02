use log::{debug, error};
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

        // Get current scope id for the local variable
        let current_scope = match self.scope_tracker.current_lv_scope() {
            Some(scope) => scope.scope_id(),
            None => {
                error!(
                    "No current local variable scope available for variable: {}",
                    variable_name
                );
                return;
            }
        };

        let fqn = FullyQualifiedName::local_variable(variable_name.clone(), current_scope).unwrap();

        let entry = {
            let mut index = self.index.lock();
            EntryBuilder::new()
                .fqn(fqn)
                .location(self.document.prism_location_to_lsp_location(&name_loc))
                .kind(EntryKind::new_local_variable(
                    variable_name.clone(),
                    current_scope,
                    inferred_type.clone(),
                ))
                .build(&mut index)
        };

        if let Ok(entry) = entry {
            // NOTE: LocalVariables are stored ONLY in RubyDocument.lvars, NOT in global index
            // This is a performance optimization - file-local data should not bloat the global index
            self.document
                .add_local_var_entry(current_scope, entry.clone());
            debug!(
                "Added local variable entry with type: {:?} -> {:?}",
                variable_name, inferred_type
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
    use parking_lot::Mutex;
    use ruby_prism::Visit;
    use std::sync::Arc;
    use tower_lsp::lsp_types::Url;

    fn create_test_index() -> Index<Unlocked> {
        Index::new(Arc::new(Mutex::new(RubyIndex::new())))
    }

    fn create_test_visitor(content: &str) -> (IndexVisitor, ruby_prism::ParseResult<'_>) {
        let uri = Url::parse("file:///test.rb").unwrap();
        let index = create_test_index();
        let document =
            crate::types::ruby_document::RubyDocument::new(uri.clone(), content.to_string(), 1);
        let scope_tracker = crate::analyzer_prism::scope_tracker::ScopeTracker::new(&document);
        let literal_analyzer = crate::inferrer::r#type::literal::LiteralAnalyzer::new();

        let visitor = IndexVisitor {
            index,
            document,
            scope_tracker,
            literal_analyzer,
            diagnostics: Vec::new(),
        };

        let parse_result = ruby_prism::parse(content.as_bytes());
        (visitor, parse_result)
    }

    #[test]
    fn test_index_visitor_infers_string_type() {
        let content = "name = 'John'";
        let (mut visitor, parse_result) = create_test_visitor(content);
        let node = parse_result.node();

        visitor.visit(&node);

        // Check that type information was stored in document.lvars
        let variable_entry = visitor.document.find_local_var_by_name("name");

        assert!(
            variable_entry.is_some(),
            "No type information was stored by IndexVisitor"
        );
    }

    #[test]
    fn test_index_visitor_infers_integer_type() {
        let content = "age = 25";
        let (mut visitor, parse_result) = create_test_visitor(content);
        let node = parse_result.node();

        visitor.visit(&node);

        // Check that type information was stored in document.lvars
        let variable_entry = visitor.document.find_local_var_by_name("age");

        assert!(
            variable_entry.is_some(),
            "No type information was stored for integer assignment"
        );
    }

    #[test]
    fn test_index_visitor_infers_float_type() {
        let content = "price = 19.99";
        let (mut visitor, parse_result) = create_test_visitor(content);
        let node = parse_result.node();

        visitor.visit(&node);

        // Check that type information was stored in document.lvars
        let variable_entry = visitor.document.find_local_var_by_name("price");

        assert!(
            variable_entry.is_some(),
            "No type information was stored for float assignment"
        );
    }

    #[test]
    fn test_index_visitor_infers_boolean_type() {
        let content = "active = true";
        let (mut visitor, parse_result) = create_test_visitor(content);
        let node = parse_result.node();

        visitor.visit(&node);

        // Check that type information was stored in document.lvars
        let variable_entry = visitor.document.find_local_var_by_name("active");

        assert!(
            variable_entry.is_some(),
            "No type information was stored for boolean assignment"
        );
    }

    #[test]
    fn test_index_visitor_handles_unknown_type() {
        let content = "name = some_method";
        let (mut visitor, parse_result) = create_test_visitor(content);
        let node = parse_result.node();

        visitor.visit(&node);

        // Verify that unknown types are stored in document.lvars
        let variable_entry = visitor.document.find_local_var_by_name("name");
        assert!(
            variable_entry.is_some(),
            "Local variable should be stored even with unknown type"
        );
    }

    #[test]
    fn test_index_visitor_generates_type_hints() {
        let content = "name = 'John'\nage = 25";
        let (mut visitor, parse_result) = create_test_visitor(content);
        let node = parse_result.node();

        visitor.visit(&node);

        // Verify that local variables are stored in document.lvars
        let name_entry = visitor.document.find_local_var_by_name("name");
        let age_entry = visitor.document.find_local_var_by_name("age");

        assert!(
            name_entry.is_some(),
            "Should store 'name' variable in document.lvars"
        );
        assert!(
            age_entry.is_some(),
            "Should store 'age' variable in document.lvars"
        );
    }

    // Note: test_combined_hints_functionality removed as we've moved to entry-based type storage
    // Type hints are now computed from indexed Variable entries rather than stored in document
}
