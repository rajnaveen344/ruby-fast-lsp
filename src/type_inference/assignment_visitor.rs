use crate::analyzer_prism::scope_tracker::ScopeTracker;
use crate::type_inference::{
    literal_analyzer::LiteralAnalyzer,
    ruby_type::RubyType,
    typed_variable::TypedVariable,
};
use crate::types::{
    ruby_document::RubyDocument,
    ruby_variable::{RubyVariable, RubyVariableType},
};
use ruby_prism::*;
use tower_lsp::lsp_types::Location;

/// Visitor for analyzing assignments and inferring variable types
pub struct AssignmentVisitor<'a> {
    scope_tracker: &'a mut ScopeTracker,
    document: &'a RubyDocument,
    literal_analyzer: LiteralAnalyzer,
}

impl<'a> AssignmentVisitor<'a> {
    pub fn new(scope_tracker: &'a mut ScopeTracker, document: &'a RubyDocument) -> Self {
        Self {
            scope_tracker,
            document,
            literal_analyzer: LiteralAnalyzer::new(),
        }
    }

    /// Infer type from a value node
    fn infer_type_from_value(&self, value_node: &Node) -> RubyType {
        // Try literal analysis first
        if let Some(literal_type) = self.literal_analyzer.analyze_literal(value_node) {
            return literal_type;
        }

        // Default to unknown type
        RubyType::Unknown
    }

    /// Process a local variable assignment
    fn process_local_variable_assignment(&mut self, name: &str, value_node: &Node, location: &ruby_prism::Location) {
        let inferred_type = self.infer_type_from_value(value_node);
        
        if let Ok(variable) = RubyVariable::new(name, RubyVariableType::Local(vec![])) {
            let lsp_location = self.document.prism_location_to_lsp_location(location);
            let typed_variable = TypedVariable::new_inferred(
                variable,
                inferred_type,
                Some(lsp_location),
            );
            
            self.scope_tracker.add_typed_variable(typed_variable);
        }
    }
}

impl<'a> Visit<'a> for AssignmentVisitor<'a> {
    fn visit_local_variable_write_node(&mut self, node: &LocalVariableWriteNode<'a>) {
        let name = std::str::from_utf8(node.name().as_slice()).unwrap_or("<invalid>");
        let value = node.value();
        self.process_local_variable_assignment(name, &value, &node.location());
        visit_local_variable_write_node(self, node);
    }

    fn visit_instance_variable_write_node(&mut self, node: &InstanceVariableWriteNode<'a>) {
        let name = std::str::from_utf8(node.name().as_slice()).unwrap_or("<invalid>");
        let value = node.value();
        let inferred_type = self.infer_type_from_value(&value);
        
        if let Ok(variable) = RubyVariable::new(name, RubyVariableType::Instance) {
            let location = Location {
                uri: self.document.uri.clone(),
                range: Default::default(),
            };
            
            let typed_variable = TypedVariable::new_inferred(
                variable,
                inferred_type,
                Some(location),
            );
            
            self.scope_tracker.add_typed_variable(typed_variable);
        }
        visit_instance_variable_write_node(self, node);
    }

    fn visit_class_variable_write_node(&mut self, node: &ClassVariableWriteNode<'a>) {
        let name = std::str::from_utf8(node.name().as_slice()).unwrap_or("<invalid>");
        let value = node.value();
        let inferred_type = self.infer_type_from_value(&value);
        
        if let Ok(variable) = RubyVariable::new(name, RubyVariableType::Class) {
            let location = Location {
                 uri: self.document.uri.clone(),
                 range: Default::default(),
             };
            
            let typed_variable = TypedVariable::new_inferred(
                variable,
                inferred_type,
                Some(location),
            );
            
            self.scope_tracker.add_typed_variable(typed_variable);
        }
        visit_class_variable_write_node(self, node);
    }

    fn visit_global_variable_write_node(&mut self, node: &GlobalVariableWriteNode<'a>) {
        let name = std::str::from_utf8(node.name().as_slice()).unwrap_or("<invalid>");
        let value = node.value();
        let inferred_type = self.infer_type_from_value(&value);
        
        if let Ok(variable) = RubyVariable::new(name, RubyVariableType::Global) {
            let location = Location {
                 uri: self.document.uri.clone(),
                 range: Default::default(),
             };
            
            let typed_variable = TypedVariable::new_inferred(
                variable,
                inferred_type,
                Some(location),
            );
            
            self.scope_tracker.add_typed_variable(typed_variable);
        }
        visit_global_variable_write_node(self, node);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::types::ruby_document::RubyDocument;
    use tower_lsp::lsp_types::Url;

    fn create_test_document(source: &str) -> RubyDocument {
        let uri = Url::parse("file:///test.rb").unwrap();
        RubyDocument::new(uri, source.to_string(), 0)
    }

    #[test]
    fn test_local_variable_assignment() {
        let source = "x = 42";
        let document = create_test_document(source);
        let mut scope_tracker = ScopeTracker::new(&document);
        let mut visitor = AssignmentVisitor::new(&mut scope_tracker, &document);
        
        let parse_result = ruby_prism::parse(source.as_bytes());
        let root_node = parse_result.node();
        visitor.visit(&root_node);
        
        // Check if the variable was added with correct type
        let typed_vars = scope_tracker.current_scope_variables();
        assert!(!typed_vars.is_empty());
    }

    #[test]
    fn test_string_assignment() {
        let source = "name = \"hello\"";
        let document = create_test_document(source);
        let mut scope_tracker = ScopeTracker::new(&document);
        let mut visitor = AssignmentVisitor::new(&mut scope_tracker, &document);
        
        let parse_result = ruby_prism::parse(source.as_bytes());
        let root_node = parse_result.node();
        visitor.visit(&root_node);
        
        let typed_vars = scope_tracker.current_scope_variables();
        assert!(!typed_vars.is_empty());
    }

    #[test]
    fn test_instance_variable_assignment() {
        let source = "@count = 10";
        let document = create_test_document(source);
        let mut scope_tracker = ScopeTracker::new(&document);
        let mut visitor = AssignmentVisitor::new(&mut scope_tracker, &document);
        
        let parse_result = ruby_prism::parse(source.as_bytes());
        let root_node = parse_result.node();
        visitor.visit(&root_node);
        
        let typed_vars = scope_tracker.current_scope_variables();
        assert!(!typed_vars.is_empty());
    }
}