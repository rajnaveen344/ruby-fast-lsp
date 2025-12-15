use log::{debug, error};
use ruby_prism::{
    GlobalVariableAndWriteNode, GlobalVariableOperatorWriteNode, GlobalVariableOrWriteNode,
    GlobalVariableTargetNode, GlobalVariableWriteNode, Node,
};

use crate::indexer::entry::{entry_builder::EntryBuilder, entry_kind::EntryKind};
use crate::type_inference::ruby_type::RubyType;
use crate::types::fully_qualified_name::FullyQualifiedName;

use super::IndexVisitor;

impl IndexVisitor {
    fn process_global_variable_write(
        &mut self,
        name: &[u8],
        name_loc: ruby_prism::Location,
        value_node: Option<&Node>,
    ) {
        let variable_name = String::from_utf8_lossy(name).to_string();
        debug!("Processing global variable: {}", variable_name);

        // Validate global variable name
        if !variable_name.starts_with('$') {
            error!("Global variable name must start with $: {}", variable_name);
            return;
        }

        if variable_name.len() < 2 {
            error!("Global variable name too short: {}", variable_name);
            return;
        }

        // Infer type from value if available
        let inferred_type = if let Some(value) = value_node {
            self.infer_type_from_value(value)
        } else {
            RubyType::Unknown
        };

        // Global variables are not associated with any namespace or method
        let fqn = match FullyQualifiedName::global_variable(variable_name.clone()) {
            Ok(fqn) => fqn,
            Err(err) => {
                error!(
                    "Invalid global variable name '{}': {}. Skipping indexing.",
                    variable_name, err
                );
                return;
            }
        };

        debug!(
            "Adding global variable entry: {:?} with type: {:?}",
            fqn, inferred_type
        );

        let entry = EntryBuilder::new()
            .fqn(fqn)
            .location(self.document.prism_location_to_lsp_location(&name_loc))
            .kind(EntryKind::new_global_variable(
                variable_name.clone(),
                inferred_type.clone(),
            ))
            .build();

        if let Ok(entry) = entry {
            self.add_entry(entry);
            debug!(
                "Added global variable entry: {} -> {:?}",
                variable_name, inferred_type
            );
        } else {
            error!(
                "Error creating entry for global variable: {}",
                variable_name
            );
        }
    }

    // GlobalVariableWriteNode
    pub fn process_global_variable_write_node_entry(&mut self, node: &GlobalVariableWriteNode) {
        self.process_global_variable_write(
            node.name().as_slice(),
            node.name_loc(),
            Some(&node.value()),
        );
    }

    pub fn process_global_variable_write_node_exit(&mut self, _node: &GlobalVariableWriteNode) {
        // No-op for now
    }

    // GlobalVariableTargetNode
    pub fn process_global_variable_target_node_entry(&mut self, node: &GlobalVariableTargetNode) {
        self.process_global_variable_write(node.name().as_slice(), node.location(), None);
    }

    pub fn process_global_variable_target_node_exit(&mut self, _node: &GlobalVariableTargetNode) {
        // No-op for now
    }

    // GlobalVariableOrWriteNode
    pub fn process_global_variable_or_write_node_entry(
        &mut self,
        node: &GlobalVariableOrWriteNode,
    ) {
        self.process_global_variable_write(
            node.name().as_slice(),
            node.name_loc(),
            Some(&node.value()),
        );
    }

    pub fn process_global_variable_or_write_node_exit(
        &mut self,
        _node: &GlobalVariableOrWriteNode,
    ) {
        // No-op for now
    }

    // GlobalVariableAndWriteNode
    pub fn process_global_variable_and_write_node_entry(
        &mut self,
        node: &GlobalVariableAndWriteNode,
    ) {
        self.process_global_variable_write(
            node.name().as_slice(),
            node.name_loc(),
            Some(&node.value()),
        );
    }

    pub fn process_global_variable_and_write_node_exit(
        &mut self,
        _node: &GlobalVariableAndWriteNode,
    ) {
        // No-op for now
    }

    // GlobalVariableOperatorWriteNode
    pub fn process_global_variable_operator_write_node_entry(
        &mut self,
        node: &GlobalVariableOperatorWriteNode,
    ) {
        self.process_global_variable_write(
            node.name().as_slice(),
            node.name_loc(),
            Some(&node.value()),
        );
    }

    pub fn process_global_variable_operator_write_node_exit(
        &mut self,
        _node: &GlobalVariableOperatorWriteNode,
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
        };

        let parse_result = ruby_prism::parse(content.as_bytes());
        (visitor, parse_result)
    }

    #[test]
    fn test_global_variable_infers_boolean_type() {
        let content = "$debug = true";
        let (mut visitor, parse_result) = create_test_visitor(content);
        let node = parse_result.node();

        visitor.visit(&node);

        let index = visitor.index.lock();
        let uri = visitor.document.uri.clone();
        let entries = index.file_entries(&uri);

        let variable_entry = entries
            .iter()
            .find(|entry| matches!(&entry.kind, EntryKind::GlobalVariable(_)));

        assert!(
            variable_entry.is_some(),
            "Should have global variable entry"
        );

        if let Some(entry) = variable_entry {
            if let EntryKind::GlobalVariable(data) = &entry.kind {
                assert_eq!(
                    &data.r#type,
                    &RubyType::true_class(),
                    "Expected TrueClass type"
                );
            }
        }
    }

    #[test]
    fn test_global_variable_infers_string_type() {
        let content = "$app_name = 'MyApp'";
        let (mut visitor, parse_result) = create_test_visitor(content);
        let node = parse_result.node();

        visitor.visit(&node);

        let index = visitor.index.lock();
        let uri = visitor.document.uri.clone();
        let entries = index.file_entries(&uri);

        let variable_entry = entries
            .iter()
            .find(|entry| matches!(&entry.kind, EntryKind::GlobalVariable(_)));

        assert!(
            variable_entry.is_some(),
            "Should have global variable entry"
        );

        if let Some(entry) = variable_entry {
            if let EntryKind::GlobalVariable(data) = &entry.kind {
                assert_eq!(&data.r#type, &RubyType::string(), "Expected String type");
            }
        }
    }

    #[test]
    fn test_global_variable_infers_float_type() {
        let content = "$pi = 3.14159";
        let (mut visitor, parse_result) = create_test_visitor(content);
        let node = parse_result.node();

        visitor.visit(&node);

        let index = visitor.index.lock();
        let uri = visitor.document.uri.clone();
        let entries = index.file_entries(&uri);

        let variable_entry = entries
            .iter()
            .find(|entry| matches!(&entry.kind, EntryKind::GlobalVariable(_)));

        assert!(
            variable_entry.is_some(),
            "Should have global variable entry"
        );

        if let Some(entry) = variable_entry {
            if let EntryKind::GlobalVariable(data) = &entry.kind {
                assert_eq!(&data.r#type, &RubyType::float(), "Expected Float type");
            }
        }
    }

    #[test]
    fn test_global_variable_or_write_infers_type() {
        let content = "$verbose ||= false";
        let (mut visitor, parse_result) = create_test_visitor(content);
        let node = parse_result.node();

        visitor.visit(&node);

        let index = visitor.index.lock();
        let uri = visitor.document.uri.clone();
        let entries = index.file_entries(&uri);

        let variable_entry = entries
            .iter()
            .find(|entry| matches!(&entry.kind, EntryKind::GlobalVariable(_)));

        assert!(
            variable_entry.is_some(),
            "Should have global variable entry"
        );

        if let Some(entry) = variable_entry {
            if let EntryKind::GlobalVariable(data) = &entry.kind {
                assert_eq!(
                    &data.r#type,
                    &RubyType::false_class(),
                    "Expected FalseClass type"
                );
            }
        }
    }
}
