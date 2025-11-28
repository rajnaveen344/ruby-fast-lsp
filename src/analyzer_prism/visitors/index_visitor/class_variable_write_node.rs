use log::{debug, error};
use ruby_prism::{
    ClassVariableAndWriteNode, ClassVariableOperatorWriteNode, ClassVariableOrWriteNode,
    ClassVariableTargetNode, ClassVariableWriteNode, Node,
};

use crate::indexer::entry::{entry_builder::EntryBuilder, entry_kind::EntryKind};
use crate::type_inference::ruby_type::RubyType;
use crate::types::fully_qualified_name::FullyQualifiedName;

use super::IndexVisitor;

impl IndexVisitor {
    fn process_class_variable_write(
        &mut self,
        name: &[u8],
        name_loc: ruby_prism::Location,
        value_node: Option<&Node>,
    ) {
        let variable_name = String::from_utf8_lossy(name).to_string();

        // Validate class variable name
        if !variable_name.starts_with("@@") {
            error!("Class variable name must start with @@: {}", variable_name);
            return;
        }

        if variable_name.len() < 3 {
            error!("Class variable name too short: {}", variable_name);
            return;
        }

        // Infer type from value if available
        let inferred_type = if let Some(value) = value_node {
            self.literal_analyzer
                .analyze_literal(value)
                .unwrap_or(RubyType::Unknown)
        } else {
            RubyType::Unknown
        };

        // Class variables are associated with the class/module, not with methods
        let fqn = FullyQualifiedName::class_variable(variable_name.clone()).unwrap();

        debug!(
            "Adding class variable entry: {:?} with type: {:?}",
            fqn, inferred_type
        );

        let entry = EntryBuilder::new()
            .fqn(fqn)
            .location(self.document.prism_location_to_lsp_location(&name_loc))
            .kind(EntryKind::new_class_variable(
                variable_name.clone(),
                inferred_type.clone(),
            ))
            .build();

        if let Ok(entry) = entry {
            let mut index = self.index.lock();
            index.add_entry(entry);
            debug!(
                "Added class variable entry: {} -> {:?}",
                variable_name, inferred_type
            );
        } else {
            error!("Error creating entry for class variable: {}", variable_name);
        }
    }

    // ClassVariableWriteNode
    pub fn process_class_variable_write_node_entry(&mut self, node: &ClassVariableWriteNode) {
        self.process_class_variable_write(
            node.name().as_slice(),
            node.name_loc(),
            Some(&node.value()),
        );
    }

    pub fn process_class_variable_write_node_exit(&mut self, _node: &ClassVariableWriteNode) {
        // No-op for now
    }

    // ClassVariableTargetNode
    pub fn process_class_variable_target_node_entry(&mut self, node: &ClassVariableTargetNode) {
        self.process_class_variable_write(node.name().as_slice(), node.location(), None);
    }

    pub fn process_class_variable_target_node_exit(&mut self, _node: &ClassVariableTargetNode) {
        // No-op for now
    }

    // ClassVariableOrWriteNode
    pub fn process_class_variable_or_write_node_entry(&mut self, node: &ClassVariableOrWriteNode) {
        self.process_class_variable_write(
            node.name().as_slice(),
            node.name_loc(),
            Some(&node.value()),
        );
    }

    pub fn process_class_variable_or_write_node_exit(&mut self, _node: &ClassVariableOrWriteNode) {
        // No-op for now
    }

    // ClassVariableAndWriteNode
    pub fn process_class_variable_and_write_node_entry(
        &mut self,
        node: &ClassVariableAndWriteNode,
    ) {
        self.process_class_variable_write(
            node.name().as_slice(),
            node.name_loc(),
            Some(&node.value()),
        );
    }

    pub fn process_class_variable_and_write_node_exit(
        &mut self,
        _node: &ClassVariableAndWriteNode,
    ) {
        // No-op for now
    }

    // ClassVariableOperatorWriteNode
    pub fn process_class_variable_operator_write_node_entry(
        &mut self,
        node: &ClassVariableOperatorWriteNode,
    ) {
        self.process_class_variable_write(
            node.name().as_slice(),
            node.name_loc(),
            Some(&node.value()),
        );
    }

    pub fn process_class_variable_operator_write_node_exit(
        &mut self,
        _node: &ClassVariableOperatorWriteNode,
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
    fn test_class_variable_infers_integer_type() {
        let content = "@@count = 0";
        let (mut visitor, parse_result) = create_test_visitor(content);
        let node = parse_result.node();

        visitor.visit(&node);

        let index = visitor.index.lock();
        let uri = visitor.document.uri.clone();
        let entries = index
            .file_entries
            .get(&uri)
            .expect("Should have entries for file");

        let variable_entry = entries
            .iter()
            .find(|entry| matches!(&entry.kind, EntryKind::ClassVariable { .. }));

        assert!(variable_entry.is_some(), "Should have class variable entry");

        if let Some(entry) = variable_entry {
            if let EntryKind::ClassVariable { r#type, .. } = &entry.kind {
                assert_eq!(*r#type, RubyType::integer(), "Expected Integer type");
            }
        }
    }

    #[test]
    fn test_class_variable_infers_hash_type() {
        let content = "@@config = { debug: true }";
        let (mut visitor, parse_result) = create_test_visitor(content);
        let node = parse_result.node();

        visitor.visit(&node);

        let index = visitor.index.lock();
        let uri = visitor.document.uri.clone();
        let entries = index
            .file_entries
            .get(&uri)
            .expect("Should have entries for file");

        let variable_entry = entries
            .iter()
            .find(|entry| matches!(&entry.kind, EntryKind::ClassVariable { .. }));

        assert!(variable_entry.is_some(), "Should have class variable entry");

        if let Some(entry) = variable_entry {
            if let EntryKind::ClassVariable { r#type, .. } = &entry.kind {
                assert!(
                    matches!(r#type, RubyType::Hash(_, _)),
                    "Expected Hash type, got {:?}",
                    r#type
                );
            }
        }
    }

    #[test]
    fn test_class_variable_or_write_infers_type() {
        let content = "@@instance ||= nil";
        let (mut visitor, parse_result) = create_test_visitor(content);
        let node = parse_result.node();

        visitor.visit(&node);

        let index = visitor.index.lock();
        let uri = visitor.document.uri.clone();
        let entries = index
            .file_entries
            .get(&uri)
            .expect("Should have entries for file");

        let variable_entry = entries
            .iter()
            .find(|entry| matches!(&entry.kind, EntryKind::ClassVariable { .. }));

        assert!(variable_entry.is_some(), "Should have class variable entry");

        if let Some(entry) = variable_entry {
            if let EntryKind::ClassVariable { r#type, .. } = &entry.kind {
                assert_eq!(*r#type, RubyType::nil_class(), "Expected NilClass type");
            }
        }
    }
}
