use log::{debug, error};
use ruby_prism::{
    LocalVariableAndWriteNode, LocalVariableOperatorWriteNode, LocalVariableOrWriteNode,
    LocalVariableTargetNode, LocalVariableWriteNode, Location, Node,
};

use crate::indexer::entry::{entry_builder::EntryBuilder, entry_kind::EntryKind};
use crate::type_inference::ruby_type::RubyType;
use crate::types::{
    fully_qualified_name::FullyQualifiedName,
    ruby_variable::{RubyVariable, RubyVariableKind},
};

use super::IndexVisitor;

impl IndexVisitor {
    /// Infer type from a value node during indexing
    fn infer_type_from_value(&self, value_node: &Node) -> RubyType {
        // Try literal analysis first
        if let Some(literal_type) = self.literal_analyzer.analyze_literal(value_node) {
            return literal_type;
        }

        // Default to unknown type
        RubyType::Unknown
    }

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

        let var = RubyVariable::new(
            &variable_name,
            RubyVariableKind::Local(self.scope_tracker.get_lv_stack().clone()),
        );

        match var {
            Ok(variable) => {
                let fqn = FullyQualifiedName::variable(variable.clone());

                let entry = EntryBuilder::new()
                    .fqn(fqn)
                    .location(self.document.prism_location_to_lsp_location(&name_loc))
                    .kind(EntryKind::new_variable(
                        variable.clone(),
                        inferred_type.clone(),
                    ))
                    .build();

                if let Ok(entry) = entry {
                    let mut index = self.index.lock();
                    index.add_entry(entry.clone());

                    // Safely get the current scope before adding local variable entry
                    if let Some(current_scope) = self.scope_tracker.current_lv_scope() {
                        self.document
                            .add_local_var_entry(current_scope.scope_id(), entry.clone());
                        debug!(
                            "Added local variable entry with type: {:?} -> {:?}",
                            variable, inferred_type
                        );
                    } else {
                        error!(
                            "No current local variable scope available for variable: {}",
                            variable_name
                        );
                    }
                } else {
                    error!("Error creating entry for local variable: {}", variable_name);
                }
            }
            Err(err) => {
                error!("Invalid local variable name '{}': {}", variable_name, err);
            }
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
    use crate::type_inference::ruby_type::RubyType;
    use parking_lot::Mutex;
    use ruby_prism::Visit;
    use std::sync::Arc;
    use tower_lsp::lsp_types::Url;

    fn create_test_visitor(content: &str) -> (IndexVisitor, ruby_prism::ParseResult) {
        let uri = Url::parse("file:///test.rb").unwrap();
        let index = Arc::new(Mutex::new(RubyIndex::new()));
        let document =
            crate::types::ruby_document::RubyDocument::new(uri.clone(), content.to_string(), 1);
        let scope_tracker = crate::analyzer_prism::scope_tracker::ScopeTracker::new(&document);
        let literal_analyzer = crate::type_inference::literal_analyzer::LiteralAnalyzer::new();

        let visitor = IndexVisitor {
            index,
            document,
            scope_tracker,
            dependency_tracker: None,
            literal_analyzer,
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

        // Check that type information was stored in Variable entries
        let index = visitor.index.lock();
        let uri = visitor.document.uri.clone();
        let entries = index
            .file_entries
            .get(&uri)
            .expect("Should have entries for file");

        // Find the variable entry and check its type
        let variable_entry = entries
            .iter()
            .find(|entry| matches!(&entry.kind, EntryKind::LocalVariable { .. }));

        assert!(
            variable_entry.is_some(),
            "No type information was stored by IndexVisitor"
        );

        if let Some(entry) = variable_entry {
            if let EntryKind::LocalVariable { r#type, .. } = &entry.kind {
                assert_eq!(*r#type, RubyType::string(), "Expected String type");
            }
        }
    }

    #[test]
    fn test_index_visitor_infers_integer_type() {
        let content = "age = 25";
        let (mut visitor, parse_result) = create_test_visitor(content);
        let node = parse_result.node();

        visitor.visit(&node);

        // Check that type information was stored in Variable entries
        let index = visitor.index.lock();
        let uri = visitor.document.uri.clone();
        let entries = index
            .file_entries
            .get(&uri)
            .expect("Should have entries for file");

        // Find the variable entry and check its type
        let variable_entry = entries
            .iter()
            .find(|entry| matches!(&entry.kind, EntryKind::LocalVariable { .. }));

        assert!(
            variable_entry.is_some(),
            "No type information was stored for integer assignment"
        );

        if let Some(entry) = variable_entry {
            if let EntryKind::LocalVariable { r#type, .. } = &entry.kind {
                assert_eq!(*r#type, RubyType::integer(), "Expected Integer type");
            }
        }
    }

    #[test]
    fn test_index_visitor_infers_float_type() {
        let content = "price = 19.99";
        let (mut visitor, parse_result) = create_test_visitor(content);
        let node = parse_result.node();

        visitor.visit(&node);

        // Check that type information was stored in Variable entries
        let index = visitor.index.lock();
        let uri = visitor.document.uri.clone();
        let entries = index
            .file_entries
            .get(&uri)
            .expect("Should have entries for file");

        // Find the variable entry and check its type
        let variable_entry = entries
            .iter()
            .find(|entry| matches!(&entry.kind, EntryKind::LocalVariable { .. }));

        assert!(
            variable_entry.is_some(),
            "No type information was stored for float assignment"
        );

        if let Some(entry) = variable_entry {
            if let EntryKind::LocalVariable { r#type, .. } = &entry.kind {
                assert_eq!(*r#type, RubyType::float(), "Expected Float type");
            }
        }
    }

    #[test]
    fn test_index_visitor_infers_boolean_type() {
        let content = "active = true";
        let (mut visitor, parse_result) = create_test_visitor(content);
        let node = parse_result.node();

        visitor.visit(&node);

        // Check that type information was stored in Variable entries
        let index = visitor.index.lock();
        let uri = visitor.document.uri.clone();
        let entries = index
            .file_entries
            .get(&uri)
            .expect("Should have entries for file");

        // Find the variable entry and check its type
        let variable_entry = entries
            .iter()
            .find(|entry| matches!(&entry.kind, EntryKind::LocalVariable { .. }));

        assert!(
            variable_entry.is_some(),
            "No type information was stored for boolean assignment"
        );

        if let Some(entry) = variable_entry {
            if let EntryKind::LocalVariable { r#type, .. } = &entry.kind {
                assert_eq!(*r#type, RubyType::true_class(), "Expected TrueClass type");
            }
        }
    }

    #[test]
    fn test_index_visitor_handles_unknown_type() {
        let content = "name = some_method";
        let (mut visitor, parse_result) = create_test_visitor(content);
        let node = parse_result.node();

        visitor.visit(&node);

        // Verify that unknown types are not stored in entries
        let index = visitor.index.lock();

        // Find variable entries and check they don't have type information for unknown types
        for entry_vec in index.definitions.values() {
            for entry in entry_vec {
                match &entry.kind {
                    crate::indexer::entry::entry_kind::EntryKind::LocalVariable { r#type, .. }
                    | crate::indexer::entry::entry_kind::EntryKind::InstanceVariable { r#type, .. }
                    | crate::indexer::entry::entry_kind::EntryKind::ClassVariable { r#type, .. }
                    | crate::indexer::entry::entry_kind::EntryKind::GlobalVariable { r#type, .. } => {
                        assert!(
                            *r#type == RubyType::Unknown,
                            "Unknown types should be stored as RubyType::Unknown in Variable entries"
                        );
                    }
                    _ => {}
                }
            }
        }
    }

    #[test]
    fn test_index_visitor_generates_type_hints() {
        let content = "name = 'John'\nage = 25";
        let (mut visitor, parse_result) = create_test_visitor(content);
        let node = parse_result.node();

        visitor.visit(&node);

        // Verify that type information is stored in Variable entries
        let index = visitor.index.lock();

        let mut found_string_type = false;
        let mut found_integer_type = false;

        for entry_vec in index.definitions.values() {
            for entry in entry_vec {
                match &entry.kind {
                    crate::indexer::entry::entry_kind::EntryKind::LocalVariable { r#type, .. }
                    | crate::indexer::entry::entry_kind::EntryKind::InstanceVariable { r#type, .. }
                    | crate::indexer::entry::entry_kind::EntryKind::ClassVariable { r#type, .. }
                    | crate::indexer::entry::entry_kind::EntryKind::GlobalVariable { r#type, .. } => {
                        if *r#type == RubyType::string() {
                            found_string_type = true;
                        } else if *r#type == RubyType::integer() {
                            found_integer_type = true;
                        }
                    }
                    _ => {}
                }
            }
        }

        assert!(
            found_string_type,
            "Should store String type in Variable entry"
        );
        assert!(
            found_integer_type,
            "Should store Integer type in Variable entry"
        );
    }

    // Note: test_combined_hints_functionality removed as we've moved to entry-based type storage
    // Type hints are now computed from indexed Variable entries rather than stored in document
}
