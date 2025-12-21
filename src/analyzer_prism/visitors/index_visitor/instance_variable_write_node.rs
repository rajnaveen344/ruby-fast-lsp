use log::{debug, error};
use ruby_prism::{
    InstanceVariableAndWriteNode, InstanceVariableOperatorWriteNode, InstanceVariableOrWriteNode,
    InstanceVariableTargetNode, InstanceVariableWriteNode, Node,
};

use crate::indexer::entry::{entry_builder::EntryBuilder, entry_kind::EntryKind};
use crate::type_inference::ruby_type::RubyType;
use crate::types::fully_qualified_name::FullyQualifiedName;

use super::IndexVisitor;

impl IndexVisitor {
    fn process_instance_variable_write(
        &mut self,
        name: &[u8],
        name_loc: ruby_prism::Location,
        value_node: Option<&Node>,
    ) {
        let variable_name = String::from_utf8_lossy(name).to_string();
        debug!("Processing instance variable: {}", variable_name);

        // Validate instance variable name
        if !variable_name.starts_with('@') {
            error!(
                "Instance variable name must start with @: {}",
                variable_name
            );
            return;
        }

        if variable_name.len() < 2 {
            error!("Instance variable name too short: {}", variable_name);
            return;
        }

        // Infer type from value if available
        let inferred_type = if let Some(value) = value_node {
            self.infer_type_from_value(value)
        } else {
            RubyType::Unknown
        };

        // Instance variables are associated with the class/module, not with methods
        let fqn = FullyQualifiedName::instance_variable(variable_name.clone()).unwrap();

        debug!(
            "Adding instance variable entry: {:?} with type: {:?}",
            fqn, inferred_type
        );

        let entry = {
            let mut index = self.index.lock();
            EntryBuilder::new()
                .fqn(fqn)
                .location(self.document.prism_location_to_lsp_location(&name_loc))
                .kind(EntryKind::new_instance_variable(
                    variable_name.clone(),
                    inferred_type.clone(),
                ))
                .build(&mut index)
        };

        if let Ok(entry) = entry {
            self.add_entry(entry);
            debug!(
                "Added instance variable entry: {} -> {:?}",
                variable_name, inferred_type
            );
        } else {
            error!(
                "Error creating entry for instance variable: {}",
                variable_name
            );
        }
    }

    // InstanceVariableWriteNode
    pub fn process_instance_variable_write_node_entry(&mut self, node: &InstanceVariableWriteNode) {
        self.process_instance_variable_write(
            node.name().as_slice(),
            node.name_loc(),
            Some(&node.value()),
        );
    }

    pub fn process_instance_variable_write_node_exit(&mut self, _node: &InstanceVariableWriteNode) {
        // No-op for now
    }

    // InstanceVariableTargetNode
    pub fn process_instance_variable_target_node_entry(
        &mut self,
        node: &InstanceVariableTargetNode,
    ) {
        self.process_instance_variable_write(node.name().as_slice(), node.location(), None);
    }

    pub fn process_instance_variable_target_node_exit(
        &mut self,
        _node: &InstanceVariableTargetNode,
    ) {
        // No-op for now
    }

    // InstanceVariableOrWriteNode
    pub fn process_instance_variable_or_write_node_entry(
        &mut self,
        node: &InstanceVariableOrWriteNode,
    ) {
        self.process_instance_variable_write(
            node.name().as_slice(),
            node.name_loc(),
            Some(&node.value()),
        );
    }

    pub fn process_instance_variable_or_write_node_exit(
        &mut self,
        _node: &InstanceVariableOrWriteNode,
    ) {
        // No-op for now
    }

    // InstanceVariableAndWriteNode
    pub fn process_instance_variable_and_write_node_entry(
        &mut self,
        node: &InstanceVariableAndWriteNode,
    ) {
        self.process_instance_variable_write(
            node.name().as_slice(),
            node.name_loc(),
            Some(&node.value()),
        );
    }

    pub fn process_instance_variable_and_write_node_exit(
        &mut self,
        _node: &InstanceVariableAndWriteNode,
    ) {
        // No-op for now
    }

    // InstanceVariableOperatorWriteNode
    pub fn process_instance_variable_operator_write_node_entry(
        &mut self,
        node: &InstanceVariableOperatorWriteNode,
    ) {
        self.process_instance_variable_write(
            node.name().as_slice(),
            node.name_loc(),
            Some(&node.value()),
        );
    }

    pub fn process_instance_variable_operator_write_node_exit(
        &mut self,
        _node: &InstanceVariableOperatorWriteNode,
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

    fn create_test_visitor(content: &str) -> (IndexVisitor, ruby_prism::ParseResult<'_>) {
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
            literal_analyzer,
            diagnostics: Vec::new(),
        };

        let parse_result = ruby_prism::parse(content.as_bytes());
        (visitor, parse_result)
    }

    #[test]
    fn test_instance_variable_infers_string_type() {
        let content = "@name = 'John'";
        let (mut visitor, parse_result) = create_test_visitor(content);
        let node = parse_result.node();

        visitor.visit(&node);

        let index = visitor.index.lock();
        let uri = visitor.document.uri.clone();
        let entries = index.file_entries(&uri);

        let variable_entry = entries
            .iter()
            .find(|entry| matches!(&entry.kind, EntryKind::InstanceVariable(_)));

        assert!(
            variable_entry.is_some(),
            "Should have instance variable entry"
        );

        if let Some(entry) = variable_entry {
            if let EntryKind::InstanceVariable(data) = &entry.kind {
                assert_eq!(&data.r#type, &RubyType::string(), "Expected String type");
            }
        }
    }

    #[test]
    fn test_instance_variable_infers_integer_type() {
        let content = "@age = 25";
        let (mut visitor, parse_result) = create_test_visitor(content);
        let node = parse_result.node();

        visitor.visit(&node);

        let index = visitor.index.lock();
        let uri = visitor.document.uri.clone();
        let entries = index.file_entries(&uri);

        let variable_entry = entries
            .iter()
            .find(|entry| matches!(&entry.kind, EntryKind::InstanceVariable(_)));

        assert!(
            variable_entry.is_some(),
            "Should have instance variable entry"
        );

        if let Some(entry) = variable_entry {
            if let EntryKind::InstanceVariable(data) = &entry.kind {
                assert_eq!(&data.r#type, &RubyType::integer(), "Expected Integer type");
            }
        }
    }

    #[test]
    fn test_instance_variable_infers_array_type() {
        let content = "@items = [1, 2, 3]";
        let (mut visitor, parse_result) = create_test_visitor(content);
        let node = parse_result.node();

        visitor.visit(&node);

        let index = visitor.index.lock();
        let uri = visitor.document.uri.clone();
        let entries = index.file_entries(&uri);

        let variable_entry = entries
            .iter()
            .find(|entry| matches!(&entry.kind, EntryKind::InstanceVariable(_)));

        assert!(
            variable_entry.is_some(),
            "Should have instance variable entry"
        );

        if let Some(entry) = variable_entry {
            if let EntryKind::InstanceVariable(data) = &entry.kind {
                assert!(
                    matches!(&data.r#type, RubyType::Array(_)),
                    "Expected Array type, got {:?}",
                    &data.r#type
                );
            }
        }
    }

    #[test]
    fn test_instance_variable_or_write_infers_type() {
        let content = "@name ||= 'default'";
        let (mut visitor, parse_result) = create_test_visitor(content);
        let node = parse_result.node();

        visitor.visit(&node);

        let index = visitor.index.lock();
        let uri = visitor.document.uri.clone();
        let entries = index.file_entries(&uri);

        let variable_entry = entries
            .iter()
            .find(|entry| matches!(&entry.kind, EntryKind::InstanceVariable(_)));

        assert!(
            variable_entry.is_some(),
            "Should have instance variable entry"
        );

        if let Some(entry) = variable_entry {
            if let EntryKind::InstanceVariable(data) = &entry.kind {
                assert_eq!(&data.r#type, &RubyType::string(), "Expected String type");
            }
        }
    }
}
